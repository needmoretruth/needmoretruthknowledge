//! The quest a reader has opened: five stages over four proof systems.
//!
//! The work runs on a worker thread. Three of the four systems finish in well under a millisecond,
//! but halo2 on the widest circuit takes longer than a frame on this machine, and a run that takes
//! longer than a frame may not happen on the drawing thread. `tick` reads what the worker has
//! finished and returns; `close` stops it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::{self, column, pad, rpad, wrap};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::widgets;
use nmtk_zk::{
    DeterministicRng, ForgeryAttempt, ForgeryKind, ForgeryOutcome, Measurement, Seed, SetupKind,
    Stage, StageDetail, StageOutcome, TrustModel, ZkError, halo2, sigma, views,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::phrases::{self, Msg};

/// Circuit widths worth trying. A reader who wants 37 bits types 37.
const BITS_PRESETS: &[u64] = &[16, 32, 48, 64];

/// The narrowest circuit this quest offers. The engine builds circuits from 8 bits up, but the
/// payment in the scenario is 21,845, which needs 15 bits to fit, and a knob whose lower half
/// always stops the run teaches nothing.
const MIN_BITS: u64 = 16;

/// Seeds worth trying. Any whole number up to the maximum is typeable.
const SEED_PRESETS: &[u64] = &[1, 7, 42, 2026];

/// Knob positions, in the order they are drawn.
const KNOB_STAGE: usize = 0;
const KNOB_BITS: usize = 1;
const KNOB_SEED: usize = 2;

/// Columns the three numbers take, so the four rows line up under their headings.
const NUMBER_WIDTH: usize = 9;

/// Cells the numbers in the toxic-waste block are right-aligned in.
const WASTE_VALUE_WIDTH: usize = 14;

/// Cells a value needs beside a label before the pair stops being worth drawing as two columns.
const VALUE_MIN: usize = 12;

/// What the worker has finished so far.
#[derive(Default)]
struct Shared {
    ready: Vec<StageOutcome>,
    error: Option<Failure>,
    finished: bool,
}

/// Why a run stopped.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Failure {
    /// The proof system gave up.
    Zk(ZkError),
    /// The machine would not start a thread.
    NoThread,
}

/// Locks the shared state, taking it back from a panicking worker rather than panicking in turn.
fn hold(shared: &Mutex<Shared>) -> MutexGuard<'_, Shared> {
    shared.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// The worker thread and the way to stop it.
struct Runner {
    shared: Arc<Mutex<Shared>>,
    cancel: Arc<AtomicBool>,
    thread: Option<JoinHandle<()>>,
}

impl Runner {
    /// Starts the stages in order on their own thread.
    fn start(stages: Vec<Stage>, seed: u64, bits: u32, machine: MachineProfile) -> Self {
        let shared = Arc::new(Mutex::new(Shared::default()));
        let cancel = Arc::new(AtomicBool::new(false));
        let worker_shared = Arc::clone(&shared);
        let worker_cancel = Arc::clone(&cancel);
        let spawned = std::thread::Builder::new().name("kq-zk".to_string()).spawn(move || {
            for stage in stages {
                if worker_cancel.load(Ordering::Relaxed) {
                    break;
                }
                match run_one(stage, seed, bits, &machine) {
                    Ok(outcome) => hold(&worker_shared).ready.push(outcome),
                    Err(error) => {
                        hold(&worker_shared).error = Some(Failure::Zk(error));
                        break;
                    }
                }
            }
            hold(&worker_shared).finished = true;
        });

        match spawned {
            Ok(thread) => Self { shared, cancel, thread: Some(thread) },
            Err(_) => {
                {
                    let mut state = hold(&shared);
                    state.error = Some(Failure::NoThread);
                    state.finished = true;
                }
                Self { shared, cancel, thread: None }
            }
        }
    }

    /// Moves whatever the worker has finished into the session. Cheap, and the lock is held only
    /// for the move.
    fn drain(&self, into: &mut Vec<StageOutcome>) -> (bool, Option<Failure>) {
        let mut state = hold(&self.shared);
        into.append(&mut state.ready);
        (state.finished, state.error)
    }

    /// Asks the worker to stop after the stage it is on, and waits for it.
    fn stop(&mut self) {
        self.cancel.store(true, Ordering::Relaxed);
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

/// Runs one system. Three of them go through the engine's own entry point; halo2 is driven
/// directly because the reader chooses the circuit width and that entry point reads the width off
/// the machine instead.
fn run_one(
    stage: Stage,
    seed: u64,
    bits: u32,
    machine: &MachineProfile,
) -> Result<StageOutcome, ZkError> {
    match stage {
        Stage::Halo2 => run_halo2(seed, bits),
        other => nmtk_zk::run_stage(other, Seed::from_u64(seed), machine),
    }
}

/// halo2 at the width the reader asked for.
fn run_halo2(seed: u64, bits: u32) -> Result<StageOutcome, ZkError> {
    let config = halo2::Config::new(bits)?;
    // The engine gives every system its own generator derived from the run's seed so that no two
    // draw the same nonce. This one is driven from outside, so it takes a seed of its own.
    let mut rng = DeterministicRng::new(Seed::from_u64(seed ^ 0xA102));
    let payment = views::scenario(views::DEFAULT_VALUE, views::DEFAULT_CHANGE, &mut rng);
    let run = halo2::run(config, payment.value, payment.change, &mut rng)?;

    let attempts = vec![ForgeryAttempt {
        kind: ForgeryKind::OverspendOutOfRange,
        attacker_holds: None,
        accepted: run.forgery.accepted,
        proof_bytes: run.forgery.proof_bytes,
    }];
    let forgery = ForgeryOutcome { any_accepted: attempts.iter().any(|a| a.accepted), attempts };
    let trust = TrustModel {
        setup: SetupKind::Transparent,
        interactive: false,
        verifier_must_be_online: false,
        anyone_can_verify_later: true,
        soundness_needs_destroyed_secret: false,
    };
    let measurement = Measurement {
        setup_nanos: run.setup_nanos,
        prove_nanos: run.prove_nanos,
        verify_nanos: run.verify_nanos,
        proof_bytes: run.proof_bytes,
    };
    let honest_accepted = run.accepted;
    let views = views::build(
        views::ViewInputs {
            stage: Stage::Halo2,
            payment: &payment,
            proof: &run.proof,
            honest_accepted,
            setup: trust.setup,
        },
        &forgery,
    );
    Ok(StageOutcome {
        stage: Stage::Halo2,
        measurement,
        trust,
        honest_accepted,
        forgery,
        views,
        detail: StageDetail::Halo2(Box::new(run)),
    })
}

/// Where each stage sits. The order here is the order `lib.rs` declares them in.
const STAGE_RUN: usize = 2;
const STAGE_MESSAGES: usize = 3;
const STAGE_TUNE: usize = 4;
const STAGE_BREAK: usize = 5;
const STAGE_SIDES: usize = 6;

/// The most events one stage reports.
const EVENT_CAP: usize = 24;

/// What a [`Step::Tell`] is about: a sentence built from what this machine measured.
#[derive(Debug, Clone, Copy)]
enum Topic {
    /// The range of proving times and proof sizes the reader saw.
    Spread,
    /// How many attacks the reader ran, and how many the verifier let through.
    Attacks,
}

/// One move in a stage's conversation.
#[derive(Debug, Clone, Copy)]
enum Step {
    Say(Msg),
    Ask(Msg),
    Run(Deed),
    Await(Until),
    /// One sentence, built from what this machine measured.
    Tell(Topic),
}

/// Work a step starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Deed {
    /// Run every system the reader asked for.
    Systems,
    /// Run the interactive protocol alone, which finishes in microseconds.
    Sigma,
    /// Show one message of that protocol.
    Message(usize),
}

/// What a waiting step is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Until {
    Finished,
}

use Deed::{Message, Sigma, Systems};
use Step::{Ask, Await, Run, Say, Tell};

/// What a proof that shows nothing actually means.
const WHAT: &[Step] = &[
    Say(Msg::WhatOne),
    Say(Msg::WhatTwo),
    Say(Msg::WhatThree),
    Say(Msg::WhatFour),
    Say(Msg::WhatFive),
    Say(Msg::WhatSix),
    Say(Msg::WhatSeven),
];

/// The four systems, and what each one fixed.
const FOUR: &[Step] = &[
    Say(Msg::FourOne),
    Say(Msg::FourTwo),
    Say(Msg::FourSigma),
    Say(Msg::FourFiatShamir),
    Say(Msg::FourTrustedSetup),
    Say(Msg::FourHalo2),
    Say(Msg::FourNumbers),
];

/// All four, measured on this machine.
const RUN: &[Step] = &[
    Say(Msg::RunOne),
    Ask(Msg::RunAsk),
    Run(Systems),
    Await(Until::Finished),
    Say(Msg::RunMeasured),
    Say(Msg::RunCheck),
    Say(Msg::RunWhy),
];

/// The interactive protocol, one message at a time.
const MESSAGES: &[Step] = &[
    Say(Msg::SigmaIntro),
    // The vocabulary before the algebra. A reviewer reached "P = x times G" without having been
    // told what a point or a G is, and stopped reading there.
    Say(Msg::SigmaOneWay),
    Say(Msg::SigmaNames),
    Run(Sigma),
    Await(Until::Finished),
    Run(Message(0)),
    Say(Msg::WhyStatement),
    Run(Message(1)),
    Say(Msg::WhyCommitment),
    Run(Message(2)),
    Say(Msg::WhyChallenge),
    Run(Message(3)),
    Say(Msg::WhyResponse),
    Run(Message(4)),
    Say(Msg::WhyVerdict),
    Say(Msg::SigmaLesson),
];

/// The reader's own circuit size and seed.
const TUNE: &[Step] = &[
    Say(Msg::TuneOne),
    Say(Msg::TuneBits),
    Say(Msg::TuneWider),
    Say(Msg::TuneSeed),
    Say(Msg::TuneSeedTwo),
    Ask(Msg::TuneAsk),
    Run(Systems),
    Await(Until::Finished),
    Say(Msg::TuneAfter),
    Say(Msg::TuneNoise),
    Say(Msg::TuneShapeOne),
    Say(Msg::TuneShapeTwo),
    Say(Msg::TuneShapeThree),
];

/// Every attack, against the verifiers that just accepted the honest proofs.
const BREAK: &[Step] = &[
    Say(Msg::BreakOne),
    Say(Msg::BreakTwo),
    Ask(Msg::BreakAsk),
    Run(Systems),
    Await(Until::Finished),
    Say(Msg::BreakTwoGetThrough),
    Say(Msg::BreakWeakHash),
    Say(Msg::BreakWeakHashTwo),
    Say(Msg::BreakWaste),
    Say(Msg::BreakUnchanged),
    Say(Msg::BreakCeremony),
    Say(Msg::BreakCeremonyIs),
    Say(Msg::BreakPromise),
];

/// The same payment, seen by four people at once.
const SIDES: &[Step] = &[
    Say(Msg::SidesOne),
    Run(Systems),
    Await(Until::Finished),
    Say(Msg::SidesTwo),
    Say(Msg::SidesThree),
    Say(Msg::SidesFour),
    Say(Msg::SidesNullifier),
    Say(Msg::SidesFive),
    Say(Msg::SidesSix),
    Say(Msg::SidesSeven),
    // The other three quests end on the reader's own numbers. This one ended on a panel of
    // somebody else's, which is the longest quest finishing with nothing of the reader's in it.
    Tell(Topic::Spread),
    Tell(Topic::Attacks),
];

const SCRIPTS: [&[Step]; 7] = [WHAT, FOUR, RUN, MESSAGES, TUNE, BREAK, SIDES];

/// Something that happened, kept as numbers so it can be said again in any language.
enum Happening {
    Measured { stage: Stage, prove: u64, verify: u64, bytes: usize, accepted: bool },
    Attack { stage: Stage, kind: ForgeryKind, accepted: bool },
    Message { index: usize, total: usize, step: sigma::Step, bytes: usize },
    Refused(Msg),
}

/// One happening, filed against the step the reader was on when it happened.
struct Logged {
    step: usize,
    what: Happening,
}

pub struct Session {
    stage: usize,
    /// How many steps of this stage have been revealed.
    revealed: usize,
    /// Everything that happened in this stage, oldest first.
    log: Vec<Logged>,
    /// How many finished systems the log has already reported.
    reported: usize,
    said_done: bool,
    machine: MachineProfile,
    knobs: Vec<Knob>,
    chosen: usize,
    runner: Option<Runner>,
    outcomes: Vec<StageOutcome>,
    failure: Option<Failure>,
    state: RunState,
    /// Which message of the interactive protocol the reader is on.
    message: usize,
    /// Which system the recap is showing the four sides of.
    focus: Stage,
}

impl Session {
    pub fn new(machine: &MachineProfile) -> Self {
        let bits = halo2::Config::for_machine(machine).value_bits as u64;
        Self {
            stage: 0,
            revealed: 0,
            log: Vec::new(),
            reported: 0,
            said_done: false,
            machine: *machine,
            knobs: vec![
                Knob::new("system", KnobValue::Choice { current: 0, count: 5 }),
                Knob::new(
                    "bits",
                    KnobValue::Count {
                        current: bits.max(MIN_BITS),
                        min: MIN_BITS,
                        max: halo2::MAX_VALUE_BITS as u64,
                        step: 8,
                        presets: BITS_PRESETS,
                    },
                ),
                Knob::new(
                    "seed",
                    KnobValue::Count {
                        current: 1,
                        min: 0,
                        max: 999_999_999,
                        step: 1,
                        presets: SEED_PRESETS,
                    },
                ),
            ],
            chosen: 0,
            runner: None,
            outcomes: Vec::new(),
            failure: None,
            state: RunState::Idle,
            message: 0,
            focus: Stage::Halo2,
        }
    }

    fn count(&self, index: usize) -> u64 {
        match self.knobs[index].value {
            KnobValue::Count { current, .. } => current,
            _ => 0,
        }
    }

    /// The systems the reader asked for: one of them, or the whole lineage.
    fn wanted(&self) -> Vec<Stage> {
        match self.knobs[KNOB_STAGE].value {
            KnobValue::Choice { current, .. } if current >= 1 && current <= Stage::ALL.len() => {
                vec![Stage::ALL[current - 1]]
            }
            _ => Stage::ALL.to_vec(),
        }
    }

    /// Throws away the last run and starts these systems on the worker thread.
    fn start(&mut self, stages: Vec<Stage>) {
        self.stop();
        self.outcomes.clear();
        self.failure = None;
        self.message = 0;
        let seed = self.count(KNOB_SEED);
        let bits = self.count(KNOB_BITS) as u32;
        self.runner = Some(Runner::start(stages, seed, bits, self.machine));
        self.state = RunState::Running;
    }

    fn stop(&mut self) {
        if let Some(runner) = &mut self.runner {
            runner.stop();
        }
        self.runner = None;
    }

    /// Stops whatever is running and forgets what is being said about it, keeping what finished
    /// runs measured.
    ///
    /// Moving between stages goes through here. The last stage draws its four viewpoints out of
    /// `outcomes`, so clearing them on the way in left the reader looking at an empty panel after
    /// running everything.
    fn stop_and_forget_the_telling(&mut self) {
        self.stop();
        self.failure = None;
        self.message = 0;
        self.state = RunState::Idle;
    }

    /// Throws away what the runs measured. Pressing `r` is the one thing that does this.
    fn forget_results(&mut self) {
        self.stop_and_forget_the_telling();
        self.outcomes.clear();
    }

    /// A sentence about what this machine measured, for the end of the quest.
    fn tell(&self, topic: Topic, language: Language) -> String {
        let measured: Vec<&Measurement> =
            self.outcomes.iter().map(|outcome| &outcome.measurement).collect();
        if measured.is_empty() {
            return Msg::YoursNothing.text(language).to_string();
        }
        match topic {
            Topic::Spread => {
                let slowest = measured.iter().map(|m| m.prove_nanos).max().unwrap_or(0);
                let quickest = measured.iter().map(|m| m.prove_nanos).min().unwrap_or(0);
                let unit = unit_for(slowest);
                let largest = measured.iter().map(|m| m.proof_bytes).max().unwrap_or(0);
                let smallest = measured.iter().map(|m| m.proof_bytes).min().unwrap_or(0);
                format!(
                    "{}: {} {} → {}, {} {} → {}.",
                    Msg::YoursSpread.text(language),
                    Msg::ColumnProve.text(language),
                    time_in(quickest, unit),
                    time_in(slowest, unit),
                    Msg::ColumnSize.text(language),
                    format::bytes(smallest as u64),
                    format::bytes(largest as u64),
                )
            }
            Topic::Attacks => {
                let attempts: Vec<&ForgeryAttempt> =
                    self.outcomes.iter().flat_map(|o| o.forgery.attempts.iter()).collect();
                let through = attempts.iter().filter(|a| a.accepted).count();
                let counted = format!(
                    "{} {}  ·  {} {}",
                    Msg::WordTried.text(language),
                    attempts.len(),
                    Msg::BreakSummaryAccepted.text(language),
                    through
                );
                let verdict = if through == 0 {
                    Msg::YoursAttacksNone
                } else {
                    Msg::YoursAttacksThrough
                };
                format!("{counted}. {}", verdict.text(language))
            }
        }
    }

    fn outcome(&self, stage: Stage) -> Option<&StageOutcome> {
        self.outcomes.iter().find(|outcome| outcome.stage == stage)
    }

    /// The interactive protocol's transcript, once stage one has finished.
    fn transcript(&self) -> Option<&sigma::Transcript> {
        match self.outcome(Stage::Sigma).map(|outcome| &outcome.detail) {
            Some(StageDetail::Sigma(run)) => Some(&run.transcript),
            _ => None,
        }
    }

    /// The four sides the recap is showing, falling back to whatever did run.
    fn focused(&self) -> Option<&StageOutcome> {
        self.outcome(self.focus).or_else(|| self.outcomes.first())
    }

    fn knob_count(&self) -> usize {
        if self.stage == STAGE_TUNE { self.knobs.len() } else { 0 }
    }

    fn move_choice(&mut self, step: i32) {
        match self.stage {
            STAGE_TUNE => {
                let last = self.knobs.len() - 1;
                self.chosen = step_round(self.chosen, last, step);
            }
            STAGE_SIDES => {
                let index = step_round(self.focus.index(), Stage::ALL.len() - 1, step);
                self.focus = Stage::ALL[index];
            }
            _ => {}
        }
    }

    // ---- Drawing -------------------------------------------------------------------

    /// The heading and one row per system: what it cost to prove, to check, and to keep.
    ///
    /// Four columns do not fit beside a conversation on an eighty-column terminal, so below a
    /// width where a name and three numbers can both be read, the name takes a line of its own and
    /// the numbers sit under it. Truncating "Trusted setup" to "Trusted se…" to save one row is a
    /// bad trade: the name is how a reader knows which row they are reading.
    fn table(
        &self,
        stages: &[Stage],
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        const SHORTEST_NAME: usize = 13;
        let one_line = width >= 2 + SHORTEST_NAME + NUMBER_WIDTH * 3;
        let name_width = if one_line { width - 2 - NUMBER_WIDTH * 3 } else { 0 };
        let numbers = |values: [String; 3], style| {
            vec![
                Span::styled(rpad(&values[0], NUMBER_WIDTH), style),
                Span::styled(rpad(&values[1], NUMBER_WIDTH), style),
                Span::styled(rpad(&values[2], NUMBER_WIDTH), style),
            ]
        };

        // One unit for the proving column and one for the verifying column, taken from the
        // slowest row in each. Per-value units made the slowest system look like the fastest.
        let measured = |pick: fn(&Measurement) -> u64| {
            stages
                .iter()
                .filter_map(|stage| self.outcome(*stage))
                .map(|outcome| pick(&outcome.measurement))
                .max()
                .unwrap_or(0)
        };
        let prove_unit = unit_for(measured(|m| m.prove_nanos));
        let verify_unit = unit_for(measured(|m| m.verify_nanos));

        let headings = [
            Msg::ColumnProve.text(language).to_string(),
            Msg::ColumnVerify.text(language).to_string(),
            Msg::ColumnSize.text(language).to_string(),
        ];
        let mut lines = Vec::new();
        if one_line {
            let mut row = vec![
                Span::styled("  ".to_string(), theme.muted()),
                Span::styled(column(Msg::ColumnStage.text(language), name_width), theme.muted()),
            ];
            row.extend(numbers(headings, theme.muted()));
            lines.push(Line::from(row));
        } else {
            let mut row = vec![Span::styled("    ".to_string(), theme.muted())];
            row.extend(numbers(headings, theme.muted()));
            lines.push(Line::from(row));
        }

        for stage in stages {
            let name = phrases::stage(*stage).text(language);
            let (state, style, values) = match self.outcome(*stage) {
                Some(outcome) => {
                    let state = if outcome.honest_accepted { State::Good } else { State::Bad };
                    let measure = &outcome.measurement;
                    (
                        Some(state),
                        theme.plain(),
                        [
                            time_in(measure.prove_nanos, prove_unit),
                            time_in(measure.verify_nanos, verify_unit),
                            format::bytes(measure.proof_bytes as u64),
                        ],
                    )
                }
                None => {
                    let working = self.state == RunState::Running;
                    let blank = Msg::NotRunYet.text(language).to_string();
                    (
                        working.then_some(State::Working),
                        theme.muted(),
                        [blank.clone(), blank.clone(), blank],
                    )
                }
            };
            let mark = state.map(State::mark).unwrap_or(" ");
            let mark_style = state.map(|s| theme.state(s)).unwrap_or_else(|| theme.muted());
            let name_style = if state.is_some() { theme.heading() } else { theme.muted() };
            if one_line {
                let mut row = vec![
                    Span::styled(format!("{mark} "), mark_style),
                    Span::styled(column(name, name_width), name_style),
                ];
                row.extend(numbers(values, style));
                lines.push(Line::from(row));
            } else {
                lines.push(Line::from(vec![
                    Span::styled(format!("{mark} "), mark_style),
                    Span::styled(name.to_string(), name_style),
                ]));
                let mut row = vec![Span::styled("    ".to_string(), style)];
                row.extend(numbers(values, style));
                lines.push(Line::from(row));
            }
        }
        lines
    }

    /// One message of the interactive protocol, laid out as a step the reader walks through.
    fn message_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let Some(transcript) = self.transcript() else {
            let text = Msg::RunWorking;
            return vec![Line::from(Span::styled(text.text(language).to_string(), theme.muted()))];
        };
        let total = transcript.lines.len();
        if total == 0 {
            return Vec::new();
        }
        let index = self.message % total;
        let line = &transcript.lines[index];

        let mut lines = vec![Line::from(vec![
            Span::styled(
                format!("{} {}/{}  ", Msg::MessageLabel.text(language), index + 1, total),
                theme.state(State::Chosen),
            ),
            Span::styled(phrases::step(line.step).text(language).to_string(), theme.heading()),
        ])];

        let mut second =
            vec![Span::styled(pad(phrases::side(line.side).text(language), 20), theme.muted())];
        match line.accepted {
            Some(accepted) => {
                let state = if accepted { State::Good } else { State::Bad };
                second.push(Span::styled(
                    format!(
                        "{} {}",
                        state.mark(),
                        if accepted { Msg::VerdictAccepted } else { Msg::VerdictRejected }
                            .text(language)
                    ),
                    theme.state(state),
                ));
            }
            None => {
                second.push(Span::styled(format::bytes(line.bytes.len() as u64), theme.plain()))
            }
        }
        lines.push(Line::from(second));

        if !line.bytes.is_empty() {
            lines.push(Line::from(Span::styled(preview(&line.bytes), theme.plain())));
        }
        lines.push(Line::from(""));
        for text in wrap(phrases::step_why(line.step).text(language), width) {
            lines.push(Line::from(Span::styled(text, theme.plain())));
        }
        lines
    }

    /// Every attack that was made, with the verifier's answer to each.
    fn attack_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        if self.outcomes.is_empty() {
            let text = Msg::RunWorking;
            return vec![Line::from(Span::styled(text.text(language).to_string(), theme.muted()))];
        }
        // Two cells in front of every attack carry the mark, and the verdict closes the row.
        let verdict_column = text::width(Msg::VerdictHeld.text(language))
            .max(text::width(Msg::VerdictBroken.text(language)));
        let attacks: Vec<Msg> = self
            .outcomes
            .iter()
            .flat_map(|outcome| outcome.forgery.attempts.iter())
            .map(|attempt| phrases::forgery(attempt.kind))
            .collect();
        let label_column = label_width(&attacks, language, width.saturating_sub(2), verdict_column);
        let mut lines = Vec::new();
        let mut attempts = 0usize;
        let mut accepted = 0usize;

        for outcome in &self.outcomes {
            lines.push(Line::from(Span::styled(
                phrases::stage(outcome.stage).text(language).to_string(),
                theme.heading(),
            )));
            for attempt in &outcome.forgery.attempts {
                attempts += 1;
                if attempt.accepted {
                    accepted += 1;
                }
                // Green means the system held, which is an attack that was refused.
                let state = if attempt.accepted { State::Bad } else { State::Good };
                // The mark is about the system, so the word beside it is too.
                let verdict = if attempt.accepted { Msg::VerdictBroken } else { Msg::VerdictHeld };
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", state.mark()), theme.state(state)),
                    Span::styled(
                        label_cell(phrases::forgery(attempt.kind).text(language), label_column),
                        theme.plain(),
                    ),
                    Span::styled(verdict.text(language).to_string(), theme.state(state)),
                ]));
            }
        }

        lines.push(Line::from(""));
        lines.extend(self.waste_lines(width, language, theme));
        lines.push(Line::from(Span::styled(
            format!(
                "{} {}  ·  {} {}",
                Msg::WordTried.text(language),
                attempts,
                Msg::BreakSummaryAccepted.text(language),
                accepted
            ),
            theme.muted(),
        )));
        lines
    }

    /// The acceptance that is the reason people ask whether a ceremony was honest.
    fn waste_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let Some(StageDetail::TrustedSetup(run)) =
            self.outcome(Stage::TrustedSetup).map(|outcome| &outcome.detail)
        else {
            return Vec::new();
        };
        let label_column = label_width(
            &[Msg::WasteHolds, Msg::WasteOpened, Msg::WasteVerifier],
            language,
            width,
            WASTE_VALUE_WIDTH,
        );
        let state = if run.with_waste.accepted { State::Bad } else { State::Good };
        let verdict =
            if run.with_waste.accepted { Msg::VerdictAccepted } else { Msg::VerdictRejected };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(
                    label_cell(Msg::WasteHolds.text(language), label_column),
                    theme.muted(),
                ),
                Span::styled(
                    rpad(&format::count(run.true_value), WASTE_VALUE_WIDTH),
                    theme.plain(),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    label_cell(Msg::WasteOpened.text(language), label_column),
                    theme.muted(),
                ),
                Span::styled(
                    rpad(&format::count(run.with_waste.claimed_value), WASTE_VALUE_WIDTH),
                    theme.plain(),
                ),
            ]),
            Line::from(vec![
                Span::styled(
                    label_cell(Msg::WasteVerifier.text(language), label_column),
                    theme.muted(),
                ),
                Span::styled(
                    rpad(
                        &format!("{} {}", state.mark(), verdict.text(language)),
                        WASTE_VALUE_WIDTH,
                    ),
                    theme.state(state),
                ),
            ]),
            Line::from(""),
        ];
        for text in wrap(Msg::WasteNote.text(language), width) {
            lines.push(Line::from(Span::styled(text, theme.plain())));
        }
        lines.push(Line::from(""));
        lines
    }

    /// The knobs, with the chosen one marked.
    fn knob_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let labels = [Msg::KnobStage, Msg::KnobBits, Msg::KnobSeed];
        self.knobs
            .iter()
            .enumerate()
            .map(|(index, knob)| {
                let picked = index == self.chosen;
                let marker = if picked { State::Chosen.mark() } else { " " };
                let value = match (knob.draft().is_some(), index) {
                    (true, _) => knob.display(),
                    (false, KNOB_STAGE) => self.system_text(language).to_string(),
                    (false, KNOB_BITS) => {
                        format!("{} {}", knob.display(), Msg::UnitBits.text(language))
                    }
                    (false, _) => knob.display(),
                };
                Line::from(vec![
                    Span::styled(format!("{marker} "), theme.state(State::Chosen)),
                    Span::styled(pad(labels[index].text(language), 15), theme.plain()),
                    Span::styled(value, if picked { theme.heading() } else { theme.muted() }),
                ])
            })
            .collect()
    }

    fn system_text(&self, language: Language) -> &'static str {
        match self.knobs[KNOB_STAGE].value {
            KnobValue::Choice { current, .. } if current >= 1 && current <= Stage::ALL.len() => {
                phrases::stage(Stage::ALL[current - 1]).text(language)
            }
            _ => Msg::StageAll.text(language),
        }
    }

    /// What the chosen system asks the reader to trust, once it has run and can be asked.
    fn trust_lines(
        &self,
        stages: &[Stage],
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let label = 12;
        match stages {
            [only] => match self.outcome(*only) {
                Some(outcome) => {
                    let text = phrases::setup_kind(outcome.trust.setup).text(language);
                    wrap(text, width.saturating_sub(label))
                        .into_iter()
                        .enumerate()
                        .map(|(index, part)| {
                            let head = if index == 0 {
                                pad(Msg::LabelTrust.text(language), label)
                            } else {
                                " ".repeat(label)
                            };
                            Line::from(vec![
                                Span::styled(head, theme.muted()),
                                Span::styled(part, theme.plain()),
                            ])
                        })
                        .collect()
                }
                None => vec![Line::from("")],
            },
            _ => wrap(Msg::TuneOnlyHalo2.text(language), width)
                .into_iter()
                .map(|part| Line::from(Span::styled(part, theme.muted())))
                .collect(),
        }
    }

    /// The shape of the circuit halo2 actually compiled, when it ran.
    fn circuit_rows(&self, language: Language) -> Vec<(&'static str, String)> {
        let Some(outcome) = self.outcome(Stage::Halo2) else { return Vec::new() };
        let StageDetail::Halo2(run) = &outcome.detail else { return Vec::new() };
        let shape = &run.shape;
        vec![
            (Msg::ShapeRows.text(language), format::count(1u64 << shape.k)),
            (Msg::ShapeRowsUsed.text(language), format::count(shape.rows_used as u64)),
            (Msg::ShapeAdvice.text(language), format::count(shape.advice_columns as u64)),
            (Msg::ShapeGates.text(language), format::count(shape.gates as u64)),
            (Msg::ShapeDegree.text(language), format::count(shape.degree as u64)),
            (Msg::ShapeLargest.text(language), format::count(run.config.max_value())),
            (Msg::ShapeSetup.text(language), short_time(outcome.measurement.setup_nanos)),
        ]
    }

    /// The same payment written out from all four sides at once.
    fn recap_lines(
        &self,
        width: usize,
        height: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let Some(outcome) = self.focused() else {
            let text = Msg::RunWorking;
            return vec![Line::from(Span::styled(text.text(language).to_string(), theme.muted()))];
        };
        let views = &outcome.views;
        let columns = Columns::measure(language, width);

        let sender = &views.sender;
        let receiver = &views.receiver;
        let onlooker = &views.onlooker;
        let attacker = &views.attacker;
        let accepted = attacker.attempts.iter().filter(|a| a.accepted).count();

        let mut blocks = vec![
            party_block(
                Msg::PartySender,
                format!(
                    "{} {} · {} {}",
                    Msg::WordSends.text(language),
                    format::count(sender.amount),
                    Msg::WordKeeps.text(language),
                    format::count(sender.change)
                ),
                &[
                    (Msg::LabelHolds, items(&sender.holds, language)),
                    (Msg::LabelLearns, items(&sender.learns, language)),
                    (Msg::LabelChecks, claims(&sender.can_verify, language)),
                ],
                columns,
                language,
                theme,
            ),
            party_block(
                Msg::PartyReceiver,
                format!(
                    "{} {} · {} {}",
                    Msg::WordGets.text(language),
                    format::count(receiver.amount),
                    Msg::ItemSenderAddress.text(language),
                    Msg::WordUnknown.text(language)
                ),
                &[
                    (Msg::LabelHolds, items(&receiver.holds, language)),
                    (Msg::LabelLearns, items(&receiver.learns, language)),
                    (Msg::LabelNever, items(&receiver.never_learns, language)),
                    (Msg::LabelChecks, claims(&receiver.can_verify, language)),
                ],
                columns,
                language,
                theme,
            ),
            party_block(
                Msg::PartyOnlooker,
                format!(
                    "{} {}",
                    Msg::WordProof.text(language),
                    format::bytes(onlooker.proof_bytes as u64)
                ),
                &[
                    (Msg::LabelSees, items(&onlooker.sees, language)),
                    (Msg::LabelNever, items(&onlooker.cannot_see, language)),
                    (Msg::LabelChecks, claims(&onlooker.can_verify, language)),
                ],
                columns,
                language,
                theme,
            ),
            party_block(
                Msg::PartyAttacker,
                format!(
                    "{} {} · {} {}",
                    Msg::WordTried.text(language),
                    attacker.attempts.len(),
                    Msg::BreakSummaryAccepted.text(language),
                    accepted
                ),
                &[
                    (Msg::LabelHolds, items(&attacker.holds, language)),
                    (Msg::LabelNeeds, items(&attacker.would_need, language)),
                ],
                columns,
                language,
                theme,
            ),
        ];

        let dropped = fit(&mut blocks, height);
        let mut lines: Vec<Line<'static>> = blocks.into_iter().flatten().collect();
        if dropped {
            lines.push(Line::from(Span::styled(
                text::truncate(Msg::PanelTrimmed.text(language), width),
                theme.muted(),
            )));
        }
        lines
    }
}

/// The four parties, in the order the recap draws them. Their names share one column.
const PARTY_NAMES: [Msg; 4] =
    [Msg::PartySender, Msg::PartyReceiver, Msg::PartyOnlooker, Msg::PartyAttacker];

/// Every row label the recap draws. All six are on screen whenever all four parties are, so the
/// column is measured from all six and every party indents to the same place.
const PARTY_LABELS: [Msg; 6] = [
    Msg::LabelHolds,
    Msg::LabelLearns,
    Msg::LabelSees,
    Msg::LabelNever,
    Msg::LabelChecks,
    Msg::LabelNeeds,
];

/// The widths the recap draws a party in, measured once so that all four indent to the same place.
#[derive(Clone, Copy)]
struct Columns {
    /// Cells the party's name takes, the gap after it included.
    name: usize,
    /// Cells a row label takes, the gap after it included.
    label: usize,
    /// Cells the panel has in all.
    width: usize,
}

impl Columns {
    /// Measured from the words this language actually puts on the panel.
    fn measure(language: Language, width: usize) -> Self {
        Self {
            name: label_width(&PARTY_NAMES, language, width, VALUE_MIN),
            label: label_width(&PARTY_LABELS, language, width, VALUE_MIN),
            width,
        }
    }
}

/// Cells to give a column of labels: the widest label that will really be drawn, and one cell of
/// gap after it, never so many that the value beside it has nowhere left to sit.
///
/// Measured rather than fixed, because a label that is six cells in English is twelve in Korean,
/// and a constant that fits one language runs into the value in the other.
fn label_width(labels: &[Msg], language: Language, width: usize, value: usize) -> usize {
    let widest = labels.iter().map(|label| text::width(label.text(language))).max().unwrap_or(0);
    (widest + 1).min(width.saturating_sub(value)).max(1)
}

/// A label drawn in exactly `width` cells, the last of which is always the gap.
///
/// The gap is kept outside the column rather than inside it because [`column`] fills every cell it
/// is given: a label too long for its column ends in an ellipsis, and that would touch the value.
fn label_cell(label: &str, width: usize) -> String {
    format!("{} ", column(label, width.saturating_sub(1)))
}

/// One party: a heading carrying its one fact, then its rows.
///
/// The columns are measured from the words this language actually uses, so a Korean label moves
/// the value over instead of running into it.
fn party_block(
    name: Msg,
    fact: String,
    rows: &[(Msg, String)],
    columns: Columns,
    language: Language,
    theme: Theme,
) -> Vec<Line<'static>> {
    let text_width = columns.width.saturating_sub(columns.label).max(1);
    let mut lines = vec![Line::from(vec![
        Span::styled(label_cell(name.text(language), columns.name), theme.heading()),
        Span::styled(column(&fact, columns.width.saturating_sub(columns.name)), theme.muted()),
    ])];
    for (label, text) in rows {
        for (index, part) in wrap(text, text_width).into_iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    if index == 0 {
                        label_cell(label.text(language), columns.label)
                    } else {
                        // A continuation sits under the value it continues, not under the label.
                        " ".repeat(columns.label)
                    },
                    theme.muted(),
                ),
                Span::styled(part, theme.plain()),
            ]));
        }
    }
    lines
}

/// Trims the tallest block until every party still fits on one screen. At 80x24 in English nothing
/// is trimmed; a narrower panel loses the tail of a list rather than a whole party.
///
/// Says whether it dropped anything, so the panel can end with a line admitting it. Rows that
/// vanish with nothing on screen to say so read as a panel that has nothing more to show.
fn fit(blocks: &mut [Vec<Line<'static>>], height: usize) -> bool {
    let mut total: usize = blocks.iter().map(Vec::len).sum();
    if height == 0 || total <= height {
        return false;
    }
    // One row is held back for the line that says rows were dropped.
    let room = height.saturating_sub(1);
    let mut dropped = false;
    while total > room {
        let Some(tallest) = blocks.iter_mut().max_by_key(|block| block.len()) else { break };
        if tallest.len() <= 2 {
            break;
        }
        tallest.pop();
        total -= 1;
        dropped = true;
    }
    dropped
}

impl Session {
    fn script(&self) -> &'static [Step] {
        SCRIPTS[self.stage.min(SCRIPTS.len() - 1)]
    }

    /// Back to the first sentence of this stage, with nothing running and nothing said.
    fn restart(&mut self) {
        self.stop_and_forget_the_telling();
        self.revealed = 0;
        self.log.clear();
        self.reported = 0;
        self.said_done = false;
    }

    fn satisfied(&self, until: Until) -> bool {
        match until {
            // A failure ends the wait too, or a refused run stops the conversation for good
            // behind a message nobody can press past.
            Until::Finished => self.failure.is_some() || self.state == RunState::Done,
        }
    }

    fn advance(&mut self) -> bool {
        let script = self.script();
        if self.revealed + 1 >= script.len() {
            return false;
        }
        self.revealed += 1;
        if let Run(deed) = script[self.revealed] {
            match deed {
                Systems => {
                    self.reported = 0;
                    self.said_done = false;
                    let wanted = self.wanted();
                    self.start(wanted);
                }
                Sigma => {
                    self.reported = 0;
                    self.said_done = false;
                    self.start(vec![Stage::Sigma]);
                }
                Message(index) => {
                    self.message = index;
                    let said = match self.transcript() {
                        Some(transcript) => transcript
                            .lines
                            .get(index)
                            .map(|line| (transcript.lines.len(), line.step, line.bytes.len())),
                        None => None,
                    };
                    if let Some((total, step, bytes)) = said {
                        self.say(Happening::Message { index, total, step, bytes });
                    }
                }
            }
            if matches!(script.get(self.revealed + 1), Some(Await(_))) {
                self.revealed += 1;
            }
        }
        true
    }

    fn say(&mut self, what: Happening) {
        if self.log.len() < EVENT_CAP {
            self.log.push(Logged { step: self.revealed, what });
        }
    }

    /// Turns anything the worker has finished into beats.
    fn notice(&mut self) {
        let fresh: Vec<Happening> = self
            .outcomes
            .iter()
            .skip(self.reported)
            .flat_map(|outcome| {
                let measure = &outcome.measurement;
                let mut out = vec![Happening::Measured {
                    stage: outcome.stage,
                    prove: measure.prove_nanos,
                    verify: measure.verify_nanos,
                    bytes: measure.proof_bytes,
                    accepted: outcome.honest_accepted,
                }];
                // Only the forgeries that got through are worth a line of their own; the ones the
                // verifier caught are the table's job.
                out.extend(outcome.forgery.attempts.iter().filter(|f| f.accepted).map(|forgery| {
                    Happening::Attack {
                        stage: outcome.stage,
                        kind: forgery.kind,
                        accepted: forgery.accepted,
                    }
                }));
                out
            })
            .collect();
        if !fresh.is_empty() {
            self.reported = self.outcomes.len();
            for what in fresh {
                self.say(what);
            }
        }
        if let Some(failure) = self.failure
            && !self.said_done
        {
            self.said_done = true;
            let message = match failure {
                Failure::Zk(error) => phrases::why_stopped(error),
                Failure::NoThread => Msg::ErrNoThread,
            };
            self.say(Happening::Refused(message));
        }
    }

    /// One happening, said in the reader's language.
    fn beat_for(&self, what: &Happening, language: Language) -> Beat {
        match what {
            Happening::Measured { stage, prove, verify, bytes, accepted } => Beat::outcome(
                if *accepted { State::Good } else { State::Bad },
                format!(
                    "{}  ·  {} {}  ·  {} {}  ·  {} {}",
                    phrases::stage(*stage).text(language),
                    Msg::EventProved.text(language),
                    short_time(*prove),
                    Msg::EventVerified.text(language),
                    short_time(*verify),
                    Msg::EventSize.text(language),
                    format::bytes(*bytes as u64),
                ),
            ),
            Happening::Attack { stage, kind, accepted } => Beat::outcome(
                if *accepted { State::Bad } else { State::Good },
                format!(
                    "{}  ·  {} {}  ·  {}",
                    phrases::stage(*stage).text(language),
                    Msg::EventAttack.text(language),
                    phrases::forgery(*kind).text(language),
                    if *accepted { Msg::VerdictAccepted } else { Msg::VerdictRejected }
                        .text(language),
                ),
            ),
            Happening::Message { index, total, step, bytes } => Beat::event(format!(
                "{} {}/{}  ·  {}  ·  {}",
                Msg::EventMessage.text(language),
                index + 1,
                total,
                phrases::step(*step).text(language),
                format::bytes(*bytes as u64),
            )),
            Happening::Refused(message) => Beat::outcome(State::Bad, message.text(language)),
        }
    }
}

impl KqSession for Session {
    fn stage(&self) -> usize {
        self.stage
    }

    fn go_to(&mut self, stage: usize) {
        if stage >= SCRIPTS.len() || stage == self.stage {
            return;
        }
        self.stage = stage;
        self.chosen = 0;
        self.restart();
    }

    fn transcript(&self, language: Language) -> Vec<Beat> {
        let script = self.script();
        let mut beats = Vec::new();
        for (index, step) in script.iter().enumerate().take(self.revealed + 1) {
            match step {
                Say(message) => beats.push(Beat::say(message.text(language))),
                Ask(message) => beats.push(Beat::ask(message.text(language))),
                Tell(topic) => beats.push(Beat::say(self.tell(*topic, language))),
                Run(_) | Await(_) => {}
            }
            for logged in self.log.iter().filter(|logged| logged.step == index) {
                beats.push(self.beat_for(&logged.what, language));
            }
        }
        beats
    }

    fn at_end(&self) -> bool {
        self.revealed + 1 >= self.script().len()
    }

    fn can_advance(&self) -> bool {
        match self.script().get(self.revealed) {
            Some(Await(until)) => self.satisfied(*until),
            _ => self.revealed + 1 < self.script().len(),
        }
    }

    fn knobs(&self) -> &[Knob] {
        if self.stage == STAGE_TUNE { &self.knobs } else { &[] }
    }

    fn chosen_knob(&self) -> Option<usize> {
        (self.knob_count() > 0).then_some(self.chosen)
    }

    fn on(&mut self, action: Action) -> Reaction {
        match action {
            Action::Stage(stage) => {
                self.go_to(stage);
                Reaction::Handled
            }
            Action::Go => {
                if let Some(Await(until)) = self.script().get(self.revealed)
                    && !self.satisfied(*until)
                {
                    return Reaction::Ignored;
                }
                // The end of a stage is not the end of the quest, but walking on from here is
                // the shell's business: it is what knows there is another stage to walk to.
                if self.advance() { Reaction::Handled } else { Reaction::Ignored }
            }
            Action::Reset => {
                self.forget_results();
                self.restart();
                Reaction::Handled
            }
            Action::Next => {
                self.move_choice(1);
                Reaction::Handled
            }
            Action::Previous => {
                self.move_choice(-1);
                Reaction::Handled
            }
            Action::Nudge(direction) => {
                if self.knob_count() > 0 {
                    self.knobs[self.chosen].nudge(direction);
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Type(c) => {
                if self.knob_count() > 0 {
                    self.knobs[self.chosen].type_char(c);
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Backspace => {
                if self.knob_count() > 0 {
                    self.knobs[self.chosen].backspace();
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Commit => {
                if self.knob_count() > 0 {
                    self.knobs[self.chosen].commit();
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Cancel => {
                if self.knob_count() > 0 {
                    self.knobs[self.chosen].cancel();
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            // Every system here finishes in well under a second, so there is nothing to hold.
            Action::PauseOrResume => Reaction::Ignored,
        }
    }

    fn tick(&mut self) {
        if let Some(runner) = &self.runner {
            let (finished, failure) = runner.drain(&mut self.outcomes);
            if let Some(failure) = failure {
                self.failure = Some(failure);
            }
            if finished {
                self.state = RunState::Done;
                self.stop();
            }
        }
        self.notice();
        if let Some(Await(until)) = self.script().get(self.revealed)
            && self.satisfied(*until)
        {
            self.advance();
        }
    }

    fn run_state(&self) -> RunState {
        self.state
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let title = match self.stage {
            STAGE_RUN | STAGE_MESSAGES => Msg::RunTitle,
            STAGE_TUNE => Msg::TuneTitle,
            STAGE_BREAK => Msg::BreakTitle,
            STAGE_SIDES => Msg::RecapTitle,
            _ => Msg::BriefPanelTitle,
        };
        let heading = match self.stage {
            STAGE_SIDES => match self.focused() {
                Some(outcome) => format!(
                    "{}  ·  {}",
                    title.text(language),
                    phrases::stage(outcome.stage).text(language)
                ),
                None => title.text(language).to_string(),
            },
            _ => title.text(language).to_string(),
        };
        let block = theme.titled_panel(&heading);
        let inner = block.inner(area);
        frame.render_widget(block, area);
        let width = inner.width as usize;
        let height = inner.height as usize;

        if let Some(failure) = self.failure {
            let message = match failure {
                Failure::Zk(error) => phrases::why_stopped(error),
                Failure::NoThread => Msg::ErrNoThread,
            };
            let mut lines = vec![Line::from(Span::styled(
                Msg::ErrorTitle.text(language).to_string(),
                theme.state(State::Bad),
            ))];
            for text in wrap(message.text(language), width) {
                lines.push(Line::from(Span::styled(text, theme.plain())));
            }
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        match self.stage {
            STAGE_RUN => {
                frame.render_widget(
                    Paragraph::new(self.table(&Stage::ALL, width, language, theme)),
                    inner,
                );
            }
            STAGE_MESSAGES => {
                frame.render_widget(
                    Paragraph::new(self.message_lines(width, language, theme)),
                    inner,
                );
            }
            STAGE_TUNE => {
                let mut knobs = self.knob_lines(language, theme);
                let stages = self.wanted();
                knobs.push(Line::from(""));
                knobs.extend(self.trust_lines(&stages, width, language, theme));
                let table = self.table(&stages, width, language, theme);
                let [knob_area, table_area, shape_area] = Layout::vertical([
                    Constraint::Length(knobs.len() as u16 + 1),
                    Constraint::Length(table.len() as u16 + 1),
                    Constraint::Min(1),
                ])
                .areas(inner);
                frame.render_widget(Paragraph::new(knobs), knob_area);
                frame.render_widget(Paragraph::new(table), table_area);

                let rows = self.circuit_rows(language);
                if !rows.is_empty() {
                    let [heading_area, stats_area] =
                        Layout::vertical([Constraint::Length(1), Constraint::Min(1)])
                            .areas(shape_area);
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            Msg::ShapeTitle.text(language).to_string(),
                            theme.heading(),
                        ))),
                        heading_area,
                    );
                    widgets::stats(frame, stats_area, theme, &rows);
                }
            }
            STAGE_BREAK => {
                frame.render_widget(
                    Paragraph::new(self.attack_lines(width, language, theme)),
                    inner,
                );
            }
            STAGE_SIDES => {
                frame.render_widget(
                    Paragraph::new(self.recap_lines(width, height, language, theme)),
                    inner,
                );
            }
            _ => {
                let mut lines = Vec::new();
                for stage in Stage::ALL {
                    lines.push(Line::from(vec![
                        Span::styled(
                            format!("{}  ", stage.index() + 1),
                            theme.state(State::Chosen),
                        ),
                        Span::styled(
                            phrases::stage(stage).text(language).to_string(),
                            theme.heading(),
                        ),
                    ]));
                    for text in
                        wrap(phrases::stage_brief(stage).text(language), width.saturating_sub(3))
                    {
                        lines.push(Line::from(Span::styled(format!("   {text}"), theme.plain())));
                    }
                    lines.push(Line::from(""));
                }
                frame.render_widget(Paragraph::new(lines), inner);
            }
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            STAGE_TUNE => vec![
                ("↑↓ ←→", Msg::KnobStage.text(language)),
                ("0-9", Msg::KeyTypeNumber.text(language)),
            ],
            STAGE_SIDES => vec![("↑↓", Msg::KeySystem.text(language))],
            _ => Vec::new(),
        }
    }

    fn typing(&self) -> bool {
        self.knob_count() > 0 && self.knobs[self.chosen].draft().is_some()
    }

    fn close(&mut self) {
        self.stop();
    }
}

/// A list of things a party holds, as one line of words.
fn items(items: &[nmtk_zk::Item], language: Language) -> String {
    if items.is_empty() {
        return Msg::WordNothing.text(language).to_string();
    }
    items.iter().map(|item| phrases::item(*item).text(language)).collect::<Vec<_>>().join(", ")
}

/// A list of things a party can settle for itself.
fn claims(claims: &[nmtk_zk::Claim], language: Language) -> String {
    if claims.is_empty() {
        return Msg::WordNothing.text(language).to_string();
    }
    claims.iter().map(|claim| phrases::claim(*claim).text(language)).collect::<Vec<_>>().join(", ")
}

/// Moves a cursor one step, wrapping at both ends.
fn step_round(current: usize, last: usize, step: i32) -> usize {
    if step > 0 {
        if current >= last { 0 } else { current + 1 }
    } else if current == 0 {
        last
    } else {
        current - 1
    }
}

/// A span of time in the unit that leaves a digit in front of the point.
///
/// `nmtk_core::format::duration` starts at hundredths of a second, and everything here but halo2
/// finishes in microseconds.
fn short_time(nanos: u64) -> String {
    time_in(nanos, unit_for(nanos))
}

/// A unit of time, and how a column of nanoseconds is written in it.
///
/// One unit per column, never one per value. A column reading 166 µs, 21 µs and 27.5 ms invites
/// the reader to compare 166 with 27.5 and conclude the slowest row is the fastest — which is
/// what a reviewer concluded about halo2, off this very table.
#[derive(Clone, Copy)]
struct TimeUnit {
    divisor: f64,
    decimals: usize,
    suffix: &'static str,
}

const NANOSECONDS: TimeUnit = TimeUnit { divisor: 1.0, decimals: 0, suffix: "ns" };
const MICROSECONDS: TimeUnit = TimeUnit { divisor: 1_000.0, decimals: 0, suffix: "µs" };
const MILLISECONDS: TimeUnit = TimeUnit { divisor: 1_000_000.0, decimals: 1, suffix: "ms" };
const SECONDS: TimeUnit = TimeUnit { divisor: 1_000_000_000.0, decimals: 2, suffix: "s" };

/// The unit the largest value in a column wants, which is the unit the whole column gets.
fn unit_for(nanos: u64) -> TimeUnit {
    if nanos < 1_000 {
        NANOSECONDS
    } else if nanos < 1_000_000 {
        MICROSECONDS
    } else if nanos < 1_000_000_000 {
        MILLISECONDS
    } else {
        SECONDS
    }
}

fn time_in(nanos: u64, unit: TimeUnit) -> String {
    // A value far below its column's unit still has to be readable, so it keeps a digit or two
    // rather than rounding to a bare zero.
    let value = nanos as f64 / unit.divisor;
    let decimals = if value < 1.0 && nanos > 0 { unit.decimals.max(2) } else { unit.decimals };
    format!("{value:.decimals$} {}", unit.suffix)
}

/// The first bytes of a message, so a reader can see that it is bytes.
fn preview(bytes: &[u8]) -> String {
    let head: Vec<String> = bytes.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
    if bytes.len() > 8 { format!("{} ...", head.join(" ")) } else { head.join(" ") }
}

#[cfg(test)]
mod tests {
    use nmtk_kq::text;
    use nmtk_kq::theme::{MIN_HEIGHT, MIN_WIDTH, split};

    use super::*;

    fn machine() -> MachineProfile {
        MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 }
    }

    /// The cells this quest's own panel has inside its border on a terminal that wide, which is
    /// what every line drawn on it has to fit into.
    fn panel_width(total: u16, theme: Theme) -> usize {
        let (talk, run) = split(total);
        theme.titled_panel("t").inner(Rect::new(talk, 0, run, MIN_HEIGHT)).width as usize
    }

    /// A session with the three fast systems finished. halo2 is left out: it really compiles a
    /// circuit, and this is a test about columns.
    fn finished() -> Session {
        let mut session = Session::new(&machine());
        session.start(vec![Stage::Sigma, Stage::FiatShamir, Stage::TrustedSetup]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while session.state != RunState::Done {
            assert!(std::time::Instant::now() < deadline, "the run never finished");
            session.tick();
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        session
    }

    /// One line the way the terminal paints it: every span, end to end.
    fn drawn(line: &Line<'_>) -> String {
        line.spans.iter().map(|span| span.content.as_ref()).collect()
    }

    /// The collision the reader saw: a label with its value hard against it and no cell between.
    fn columns_keep_their_gap(line: &Line<'_>) -> bool {
        line.spans.windows(2).all(|pair| {
            let (left, right) = (pair[0].content.as_ref(), pair[1].content.as_ref());
            left.is_empty() || right.is_empty() || left.ends_with(' ') || right.starts_with(' ')
        })
    }

    #[test]
    fn the_four_sides_panel_stays_inside_its_column_in_both_languages() {
        let session = finished();
        let theme = Theme::new(true);
        for total in [MIN_WIDTH, 100] {
            let width = panel_width(total, theme);
            for language in [Language::ENGLISH, Language::KOREAN] {
                for line in session.recap_lines(width, 20, language, theme) {
                    let line_text = drawn(&line);
                    assert!(
                        text::width(&line_text) <= width,
                        "{total} columns in {language}: {line_text:?} is wider than {width}"
                    );
                    assert!(
                        columns_keep_their_gap(&line),
                        "{total} columns in {language}: {line_text:?} has no gap after its label"
                    );
                }
            }
        }
    }

    #[test]
    fn the_attack_table_stays_inside_its_column_in_both_languages() {
        let session = finished();
        let theme = Theme::new(true);
        for total in [MIN_WIDTH, 100] {
            let width = panel_width(total, theme);
            for language in [Language::ENGLISH, Language::KOREAN] {
                for line in session.attack_lines(width, language, theme) {
                    let line_text = drawn(&line);
                    assert!(
                        text::width(&line_text) <= width,
                        "{total} columns in {language}: {line_text:?} is wider than {width}"
                    );
                    assert!(
                        columns_keep_their_gap(&line),
                        "{total} columns in {language}: {line_text:?} has no gap before its verdict"
                    );
                }
            }
        }
    }

    /// Walking to another stage stopped the runs and threw their measurements away with them,
    /// so the last stage — which draws its four viewpoints out of exactly those measurements —
    /// was empty however much work the reader had done.
    /// The standard's limit on one beat, and the reason it is a limit: a reviewer who met three
    /// dense sentences at once stopped reading the quest at that point.
    #[test]
    fn every_beat_of_every_stage_is_short_in_both_languages() {
        for (stage, script) in SCRIPTS.iter().enumerate() {
            for language in Language::ALL {
                for step in *script {
                    let text = match step {
                        Say(message) | Ask(message) => message.text(*language),
                        _ => continue,
                    };
                    assert!(
                        text.chars().count() <= 160,
                        "stage {stage} in {language} says too much at once: {text:?}"
                    );
                    assert!(!text.is_empty(), "stage {stage} has a blank beat in {language}");
                }
            }
        }
    }

    /// The other three quests end on the reader's own numbers. This one ended on a panel of
    /// somebody else's, so a reviewer reached the end of the longest quest with nothing of theirs.
    #[test]
    fn the_quest_ends_on_numbers_this_machine_measured() {
        let session = finished();
        for language in Language::ALL {
            let spread = session.tell(Topic::Spread, *language);
            let attacks = session.tell(Topic::Attacks, *language);
            println!("{language}: {spread}\n{language}: {attacks}");
            for said in [&spread, &attacks] {
                assert!(said.chars().count() <= 160, "too much at once: {said:?}");
                assert!(
                    said != Msg::YoursNothing.text(*language),
                    "a finished run reported nothing measured"
                );
            }
            assert!(spread.contains('→'), "no range in {spread:?}");
            assert!(attacks.contains(|c: char| c.is_ascii_digit()), "no count in {attacks:?}");
        }

        // With nothing run, it says so rather than inventing a range.
        let empty = Session::new(&machine());
        assert_eq!(
            empty.tell(Topic::Spread, Language::ENGLISH),
            Msg::YoursNothing.text(Language::ENGLISH)
        );
    }

    #[test]
    fn walking_to_another_stage_keeps_what_the_runs_measured() {
        let mut session = finished();
        let measured = session.outcomes.len();
        assert!(measured > 0, "the run measured nothing to begin with");

        let last = SCRIPTS.len() - 1;
        KqSession::go_to(&mut session, last);
        assert_eq!(session.outcomes.len(), measured, "the stage change lost the measurements");

        // `r` is the one key that forgets them.
        KqSession::on(&mut session, Action::Reset);
        assert!(session.outcomes.is_empty(), "r left the old measurements behind");
    }

    #[test]
    fn a_panel_too_short_for_every_party_says_so_rather_than_losing_rows_in_silence() {
        let session = finished();
        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        let whole = session.recap_lines(width, 200, Language::KOREAN, theme);
        let cut = session.recap_lines(width, 12, Language::KOREAN, theme);
        assert!(cut.len() < whole.len(), "nothing was dropped at twelve rows");
        assert!(cut.len() <= 12, "the panel kept {} rows in twelve", cut.len());
        let said = drawn(cut.last().expect("a line"));
        let head: String = Msg::PanelTrimmed.text(Language::KOREAN).chars().take(10).collect();
        assert!(said.starts_with(&head), "the panel said nothing about the rows it dropped: {said:?}");
    }
}
