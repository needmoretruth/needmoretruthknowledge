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
use nmtk_kq::meta::StageKind;
use nmtk_kq::session::{Action, KqSession, Reaction, RunState};
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

pub struct Session {
    stage: StageKind,
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
            stage: StageKind::Brief,
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

    fn reset(&mut self) {
        self.stop();
        self.outcomes.clear();
        self.failure = None;
        self.message = 0;
        self.state = RunState::Idle;
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
        if self.stage == StageKind::Tune { self.knobs.len() } else { 0 }
    }

    fn move_choice(&mut self, step: i32) {
        match self.stage {
            StageKind::Tune => {
                let last = self.knobs.len() - 1;
                self.chosen = wrap(self.chosen, last, step);
            }
            StageKind::Recap => {
                let index = wrap(self.focus.index(), Stage::ALL.len() - 1, step);
                self.focus = Stage::ALL[index];
            }
            _ => {}
        }
    }

    // ---- Drawing -------------------------------------------------------------------

    /// The heading and one row per system: what it cost to prove, to check, and to keep.
    fn table(
        &self,
        stages: &[Stage],
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let name_width = width.saturating_sub(2 + NUMBER_WIDTH * 3).max(13);
        let mut lines = vec![Line::from(vec![
            Span::styled("  ".to_string(), theme.muted()),
            Span::styled(pad(Msg::ColumnStage.text(language), name_width), theme.muted()),
            Span::styled(rpad(Msg::ColumnProve.text(language), NUMBER_WIDTH), theme.muted()),
            Span::styled(rpad(Msg::ColumnVerify.text(language), NUMBER_WIDTH), theme.muted()),
            Span::styled(rpad(Msg::ColumnSize.text(language), NUMBER_WIDTH), theme.muted()),
        ])];

        for stage in stages {
            let name = pad(phrases::stage(*stage).text(language), name_width);
            match self.outcome(*stage) {
                Some(outcome) => {
                    let state = if outcome.honest_accepted { State::Good } else { State::Bad };
                    let measure = &outcome.measurement;
                    lines.push(Line::from(vec![
                        Span::styled(format!("{} ", state.mark()), theme.state(state)),
                        Span::styled(name, theme.heading()),
                        Span::styled(
                            rpad(&short_time(measure.prove_nanos), NUMBER_WIDTH),
                            theme.plain(),
                        ),
                        Span::styled(
                            rpad(&short_time(measure.verify_nanos), NUMBER_WIDTH),
                            theme.plain(),
                        ),
                        Span::styled(
                            rpad(&format::bytes(measure.proof_bytes as u64), NUMBER_WIDTH),
                            theme.plain(),
                        ),
                    ]));
                }
                None => {
                    let working = self.state == RunState::Running;
                    let mark = if working { State::Working.mark() } else { " " };
                    let style = if working { theme.state(State::Working) } else { theme.muted() };
                    let blank = Msg::NotRunYet.text(language);
                    lines.push(Line::from(vec![
                        Span::styled(format!("{mark} "), style),
                        Span::styled(name, theme.muted()),
                        Span::styled(rpad(blank, NUMBER_WIDTH), theme.muted()),
                        Span::styled(rpad(blank, NUMBER_WIDTH), theme.muted()),
                        Span::styled(rpad(blank, NUMBER_WIDTH), theme.muted()),
                    ]));
                }
            }
        }
        lines
    }

    /// One message of the interactive protocol, laid out as a step the reader walks through.
    fn message_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let Some(transcript) = self.transcript() else {
            let text = if self.state == RunState::Running { Msg::RunWorking } else { Msg::RunIdle };
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
        for text in wrap_text(phrases::step_why(line.step).text(language), width) {
            lines.push(Line::from(Span::styled(text, theme.plain())));
        }
        lines
    }

    /// Every attack that was made, with the verifier's answer to each.
    fn attack_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        if self.outcomes.is_empty() {
            let text =
                if self.state == RunState::Running { Msg::RunWorking } else { Msg::BreakIdle };
            return vec![Line::from(Span::styled(text.text(language).to_string(), theme.muted()))];
        }
        let label_width = width.saturating_sub(2 + 10).max(20);
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
                let verdict =
                    if attempt.accepted { Msg::VerdictAccepted } else { Msg::VerdictRejected };
                lines.push(Line::from(vec![
                    Span::styled(format!("{} ", state.mark()), theme.state(state)),
                    Span::styled(
                        pad(phrases::forgery(attempt.kind).text(language), label_width),
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
        let label = width.saturating_sub(16).max(18);
        let state = if run.with_waste.accepted { State::Bad } else { State::Good };
        let verdict =
            if run.with_waste.accepted { Msg::VerdictAccepted } else { Msg::VerdictRejected };
        let mut lines = vec![
            Line::from(vec![
                Span::styled(pad(Msg::WasteHolds.text(language), label), theme.muted()),
                Span::styled(rpad(&format::count(run.true_value), 14), theme.plain()),
            ]),
            Line::from(vec![
                Span::styled(pad(Msg::WasteOpened.text(language), label), theme.muted()),
                Span::styled(rpad(&format::count(run.with_waste.claimed_value), 14), theme.plain()),
            ]),
            Line::from(vec![
                Span::styled(pad(Msg::WasteVerifier.text(language), label), theme.muted()),
                Span::styled(
                    rpad(&format!("{} {}", state.mark(), verdict.text(language)), 14),
                    theme.state(state),
                ),
            ]),
            Line::from(""),
        ];
        for text in wrap_text(Msg::WasteNote.text(language), width) {
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
                    wrap_text(text, width.saturating_sub(label))
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
            _ => wrap_text(Msg::TuneOnlyHalo2.text(language), width)
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
            let text = if self.state == RunState::Running { Msg::RunWorking } else { Msg::RunIdle };
            return vec![Line::from(Span::styled(text.text(language).to_string(), theme.muted()))];
        };
        let views = &outcome.views;
        let text_width = width.saturating_sub(7).max(12);

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
                text_width,
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
                text_width,
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
                text_width,
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
                text_width,
                language,
                theme,
            ),
        ];

        fit(&mut blocks, height);
        blocks.into_iter().flatten().collect()
    }
}

/// One party: a heading carrying its one fact, then its rows.
fn party_block(
    name: Msg,
    fact: String,
    rows: &[(Msg, String)],
    width: usize,
    language: Language,
    theme: Theme,
) -> Vec<Line<'static>> {
    let mut lines = vec![Line::from(vec![
        Span::styled(pad(name.text(language), 10), theme.heading()),
        Span::styled(fact, theme.muted()),
    ])];
    for (label, text) in rows {
        for (index, part) in wrap_text(text, width).into_iter().enumerate() {
            lines.push(Line::from(vec![
                Span::styled(
                    if index == 0 { pad(label.text(language), 7) } else { " ".repeat(7) },
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
fn fit(blocks: &mut [Vec<Line<'static>>], height: usize) {
    if height == 0 {
        return;
    }
    let mut total: usize = blocks.iter().map(Vec::len).sum();
    while total > height {
        let Some(tallest) = blocks.iter_mut().max_by_key(|block| block.len()) else { break };
        if tallest.len() <= 2 {
            break;
        }
        tallest.pop();
        total -= 1;
    }
}

impl KqSession for Session {
    fn stage(&self) -> StageKind {
        self.stage
    }

    fn go_to(&mut self, stage: StageKind) {
        self.stage = stage;
        self.chosen = 0;
    }

    fn knobs(&self) -> &[Knob] {
        if self.stage == StageKind::Tune { &self.knobs } else { &[] }
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
            Action::Go => match self.stage {
                StageKind::Run => {
                    if self.state == RunState::Done && self.transcript().is_some() {
                        self.message += 1;
                    } else if self.state == RunState::Idle {
                        self.start(Stage::ALL.to_vec());
                    }
                    Reaction::Handled
                }
                StageKind::Tune => {
                    let wanted = self.wanted();
                    self.start(wanted);
                    Reaction::Handled
                }
                StageKind::Break | StageKind::Recap => {
                    self.start(Stage::ALL.to_vec());
                    Reaction::Handled
                }
                StageKind::Brief => Reaction::Ignored,
            },
            Action::Reset => {
                self.reset();
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
        let Some(runner) = &self.runner else { return };
        let (finished, failure) = runner.drain(&mut self.outcomes);
        if let Some(failure) = failure {
            self.failure = Some(failure);
        }
        if finished {
            self.state = RunState::Done;
            self.stop();
        }
    }

    fn run_state(&self) -> RunState {
        self.state
    }

    fn explain(&self, language: Language) -> Vec<Line<'static>> {
        let paragraphs: &[Msg] = match self.stage {
            StageKind::Brief => &[
                Msg::BriefOpening,
                Msg::BriefEveryday,
                Msg::BriefChain,
                Msg::BriefWhy,
                Msg::BriefLineage,
                Msg::BriefPromise,
            ],
            StageKind::Run => &[Msg::RunExplainOne, Msg::RunExplainTwo, Msg::RunExplainThree],
            StageKind::Tune => &[Msg::TuneExplainOne, Msg::TuneExplainTwo, Msg::TuneExplainThree],
            StageKind::Break => &[
                Msg::BreakExplainOne,
                Msg::BreakExplainTwo,
                Msg::BreakExplainThree,
                Msg::BreakExplainFour,
            ],
            StageKind::Recap => &[
                Msg::RecapExplainOne,
                Msg::RecapExplainTwo,
                Msg::RecapExplainThree,
                Msg::RecapExplainFour,
            ],
        };
        let mut lines = Vec::new();
        for (index, message) in paragraphs.iter().enumerate() {
            if index > 0 {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(message.text(language)));
        }
        lines
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let title = match self.stage {
            StageKind::Brief => Msg::BriefPanelTitle,
            StageKind::Run => Msg::RunTitle,
            StageKind::Tune => Msg::TuneTitle,
            StageKind::Break => Msg::BreakTitle,
            StageKind::Recap => Msg::RecapTitle,
        };
        let heading = match self.stage {
            StageKind::Recap => match self.focused() {
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
            for text in wrap_text(message.text(language), width) {
                lines.push(Line::from(Span::styled(text, theme.plain())));
            }
            frame.render_widget(Paragraph::new(lines), inner);
            return;
        }

        match self.stage {
            StageKind::Brief => {
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
                    for text in wrap_text(
                        phrases::stage_brief(stage).text(language),
                        width.saturating_sub(3),
                    ) {
                        lines.push(Line::from(Span::styled(format!("   {text}"), theme.plain())));
                    }
                    lines.push(Line::from(""));
                }
                lines.push(Line::from(Span::styled(
                    Msg::BriefStart.text(language).to_string(),
                    theme.muted(),
                )));
                frame.render_widget(Paragraph::new(lines), inner);
            }
            StageKind::Run => {
                let table = self.table(&Stage::ALL, width, language, theme);
                let rows = table.len() as u16;
                let [top, bottom] =
                    Layout::vertical([Constraint::Length(rows + 1), Constraint::Min(1)])
                        .areas(inner);
                frame.render_widget(Paragraph::new(table), top);
                frame.render_widget(
                    Paragraph::new(self.message_lines(width, language, theme)),
                    bottom,
                );
            }
            StageKind::Tune => {
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
            StageKind::Break => {
                frame.render_widget(
                    Paragraph::new(self.attack_lines(width, language, theme)),
                    inner,
                );
            }
            StageKind::Recap => {
                frame.render_widget(
                    Paragraph::new(self.recap_lines(width, height, language, theme)),
                    inner,
                );
            }
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            StageKind::Run if self.transcript().is_some() => {
                vec![("Enter", Msg::KeyNextMessage.text(language))]
            }
            StageKind::Tune => vec![
                ("←→", Msg::KnobStage.text(language)),
                ("Enter", Msg::KeyRunAgain.text(language)),
            ],
            StageKind::Break => vec![("Enter", Msg::KeyRunAttacks.text(language))],
            StageKind::Recap => vec![("↑↓", Msg::KeySystem.text(language))],
            _ => Vec::new(),
        }
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
fn wrap(current: usize, last: usize, step: i32) -> usize {
    if step > 0 {
        if current >= last { 0 } else { current + 1 }
    } else if current == 0 {
        last
    } else {
        current - 1
    }
}

/// `text` padded to `width` columns, counting characters rather than bytes.
fn pad(text: &str, width: usize) -> String {
    let used = text.chars().count();
    let mut out = text.to_string();
    for _ in used..width {
        out.push(' ');
    }
    out
}

/// `text` pushed to the right of `width` columns, so numbers line up under their heading.
fn rpad(text: &str, width: usize) -> String {
    let used = text.chars().count();
    let mut out = String::new();
    for _ in used..width {
        out.push(' ');
    }
    out.push_str(text);
    out
}

/// A span of time in the unit that leaves a digit in front of the point.
///
/// `nmtk_core::format::duration` starts at hundredths of a second, and everything here but halo2
/// finishes in microseconds.
fn short_time(nanos: u64) -> String {
    if nanos < 1_000 {
        format!("{nanos} ns")
    } else if nanos < 1_000_000 {
        format!("{:.0} µs", nanos as f64 / 1_000.0)
    } else if nanos < 1_000_000_000 {
        format!("{:.1} ms", nanos as f64 / 1_000_000.0)
    } else {
        format!("{:.2} s", nanos as f64 / 1_000_000_000.0)
    }
}

/// The first bytes of a message, so a reader can see that it is bytes.
fn preview(bytes: &[u8]) -> String {
    let head: Vec<String> = bytes.iter().take(8).map(|byte| format!("{byte:02x}")).collect();
    if bytes.len() > 8 { format!("{} ...", head.join(" ")) } else { head.join(" ") }
}

/// Breaks text into lines of at most `width` columns, on spaces.
fn wrap_text(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return vec![text.to_string()];
    }
    let mut lines = Vec::new();
    let mut current = String::new();
    for word in text.split_whitespace() {
        let extra = usize::from(!current.is_empty());
        if !current.is_empty() && current.chars().count() + extra + word.chars().count() > width {
            lines.push(std::mem::take(&mut current));
        }
        if !current.is_empty() {
            current.push(' ');
        }
        current.push_str(word);
    }
    if !current.is_empty() {
        lines.push(current);
    }
    if lines.is_empty() {
        lines.push(String::new());
    }
    lines
}
