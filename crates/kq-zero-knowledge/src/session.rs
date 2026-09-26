//! The quest a reader has opened: seven stages over four proof systems.
//!
//! The work runs on a worker thread. Three of the four systems finish in well under a millisecond,
//! but halo2 on the widest circuit takes longer than a frame on this machine, and a run that takes
//! longer than a frame may not happen on the drawing thread. `tick` reads what the worker has
//! finished and returns; `close` stops it.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};
use std::thread::JoinHandle;

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue, Settled, Typed};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::{self, column, rpad, wrap};
use nmtk_kq::theme::{self, State, Theme};
use nmtk_kq::widgets;
use nmtk_zk::{
    ForgeryAttempt, ForgeryKind, Measurement, Seed, Stage, StageDetail, StageOutcome, ZkError,
    halo2, sigma,
};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::style::Style;
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

/// A runner dropped without being stopped — a session dropped without `close`, or a panic between
/// the two — used to leave its thread proving on with nobody to read the answer.
impl Drop for Runner {
    fn drop(&mut self) {
        self.stop();
    }
}

/// Runs one system. halo2 is run at the width the reader chose; the other three have no width.
///
/// halo2 used to be driven from here with a seed of its own and a hand-copied trust model, so the
/// same seed proved one thing in the tuning stage and another everywhere else. The engine has one
/// way to run it now.
fn run_one(
    stage: Stage,
    seed: u64,
    bits: u32,
    machine: &MachineProfile,
) -> Result<StageOutcome, ZkError> {
    match stage {
        Stage::Halo2 => nmtk_zk::run_halo2(Seed::from_u64(seed), halo2::Config::new(bits)?),
        other => nmtk_zk::run_stage(other, Seed::from_u64(seed), machine),
    }
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
    /// How many of this run's attacks got through.
    Through,
    /// What the circuit width did to the run that just finished.
    Widened,
}

/// Everything every run in this quest measured, kept for the closing stage.
///
/// The recap used to read the last run's outcomes, and the recap stage starts a run of its own, so
/// "your own numbers" were the four the recap had just made: a reader who had proved sixty-four-bit
/// circuits all afternoon was shown the default width, and every attack they ran before it was
/// forgotten. Each finished system writes into this once, and only `r` clears it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct Record {
    /// Systems that finished.
    runs: usize,
    quickest_prove: u64,
    slowest_prove: u64,
    smallest_proof: usize,
    largest_proof: usize,
    /// Attacks made, and how many the verifier accepted.
    tried: usize,
    through: usize,
}

impl Record {
    fn add(&mut self, outcome: &StageOutcome) {
        let measure = &outcome.measurement;
        if self.runs == 0 {
            self.quickest_prove = measure.prove_nanos;
            self.smallest_proof = measure.proof_bytes;
        }
        self.runs += 1;
        self.quickest_prove = self.quickest_prove.min(measure.prove_nanos);
        self.slowest_prove = self.slowest_prove.max(measure.prove_nanos);
        self.smallest_proof = self.smallest_proof.min(measure.proof_bytes);
        self.largest_proof = self.largest_proof.max(measure.proof_bytes);
        self.tried += outcome.forgery.attempts.len();
        self.through += outcome.forgery.attempts.iter().filter(|a| a.accepted).count();
    }

    /// Folds another stage's record into this one.
    fn merge(&mut self, other: &Record) {
        if other.runs == 0 {
            return;
        }
        if self.runs == 0 {
            *self = *other;
            return;
        }
        self.runs += other.runs;
        self.quickest_prove = self.quickest_prove.min(other.quickest_prove);
        self.slowest_prove = self.slowest_prove.max(other.slowest_prove);
        self.smallest_proof = self.smallest_proof.min(other.smallest_proof);
        self.largest_proof = self.largest_proof.max(other.largest_proof);
        self.tried += other.tried;
        self.through += other.through;
    }
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
    // "Verifier" was the fourth beat's subject before anybody had said what one is.
    Say(Msg::Roles),
    Say(Msg::FourSigma),
    Say(Msg::FourFiatShamir),
    // The panel beside this stage says "becomes a hash" and nothing on screen said what one was.
    Say(Msg::FourHash),
    Say(Msg::FourTrustedSetup),
    Say(Msg::FourHalo2),
    Say(Msg::FourCircuit),
    Say(Msg::FourNumbers),
];

/// All four, measured on this machine.
const RUN: &[Step] = &[
    Say(Msg::RunOne),
    Ask(Msg::RunAsk),
    Run(Systems),
    Await(Until::Finished),
    // The size column is the first place bytes appear.
    Say(Msg::RunBytes),
    Say(Msg::RunMeasured),
    Say(Msg::RunCheck),
    Say(Msg::RunWhy),
];

/// The interactive protocol, one message at a time.
const MESSAGES: &[Step] = &[
    Say(Msg::SigmaIntro),
    Say(Msg::Roles),
    // The vocabulary before the algebra. A reviewer reached "P = x times G" without having been
    // told what a point or a G is, and stopped reading there.
    Say(Msg::SigmaOneWay),
    Say(Msg::SigmaNotTimes),
    Say(Msg::SigmaNotDivide),
    Say(Msg::SigmaNames),
    Run(Sigma),
    Await(Until::Finished),
    Run(Message(0)),
    Say(Msg::WhyStatement),
    Run(Message(1)),
    Say(Msg::WhyCommitment),
    Say(Msg::WhatACommitmentIs),
    Say(Msg::WhatACommitmentDoes),
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
    Say(Msg::TuneCircuit),
    // What a bit is, before the sentence that measures in them.
    Say(Msg::TuneBitIs),
    Say(Msg::TuneBits),
    Say(Msg::TuneWider),
    Say(Msg::TuneSeed),
    Say(Msg::TuneSeedTwo),
    Ask(Msg::TuneAsk),
    Run(Systems),
    Await(Until::Finished),
    // Said from the run, not for it: a reader who picked Sigma alone was told "only halo2 moved"
    // about a run halo2 was not in.
    Tell(Topic::Widened),
    Say(Msg::TuneNoise),
    Say(Msg::TuneShapeOne),
    Say(Msg::TuneShapeTwo),
    Say(Msg::TuneShapeThree),
];

/// Every attack, against the verifiers that just accepted the honest proofs.
const BREAK: &[Step] = &[
    // Its first sentence is about "the same verifier", and this stage may be the first opened.
    Say(Msg::Roles),
    Say(Msg::BreakOne),
    Say(Msg::BreakTwo),
    Ask(Msg::BreakAsk),
    Run(Systems),
    Await(Until::Finished),
    // How many got through is read off this run rather than written down beforehand.
    Tell(Topic::Through),
    // The two words the next beats turn on, said again here because this stage can be the first
    // one a reader opens.
    Say(Msg::BreakRecall),
    Say(Msg::BreakReplayable),
    Say(Msg::BreakWeakHash),
    Say(Msg::BreakWeakHashTwo),
    Say(Msg::BreakWaste),
    Say(Msg::BreakUnchanged),
    // What a ceremony is comes before the sentence that asks whether one was honest.
    Say(Msg::BreakCeremonyIs),
    Say(Msg::BreakCeremony),
    Say(Msg::BreakPromise),
];

/// The same payment, seen by four people at once.
const SIDES: &[Step] = &[
    Say(Msg::SidesOne),
    Run(Systems),
    Await(Until::Finished),
    // Note, commitment and nullifier are each said before the first sentence that leans on them.
    // The sender "held the note" before anyone had said what a note was, and the onlooker saw "a
    // nullifier" two beats before the one that explained it.
    Say(Msg::SidesNote),
    Say(Msg::SidesTwo),
    Say(Msg::SidesKeys),
    Say(Msg::SidesBlinding),
    Say(Msg::SidesThree),
    Say(Msg::SidesCommitment),
    Say(Msg::SidesNullifier),
    Say(Msg::SidesFour),
    // Where the shape came from, once its three words have been earned. A reader who has heard
    // of Zcash gets an anchor, and the quest stops borrowing a design without saying whose.
    Say(Msg::SidesZcash),
    Say(Msg::SidesSigmaToo),
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
    Measured {
        stage: Stage,
        prove: u64,
        verify: u64,
        bytes: usize,
        accepted: bool,
    },
    Attack {
        stage: Stage,
        kind: ForgeryKind,
        accepted: bool,
    },
    Message {
        index: usize,
        total: usize,
        step: sigma::Step,
        bytes: usize,
    },
    Refused(Msg),
    /// A typed number that fell outside the knob's range, with where it landed.
    PulledIn {
        to: String,
    },
    /// The same for the system knob, which is drawn as a name rather than a number, so the name
    /// is said in whichever language is on screen when the beat is read.
    SystemPulledIn {
        current: usize,
    },
    /// A typed number the knob could not read at all.
    NotANumber,
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
    /// The knob values the run in progress was started with, so turning one can be told apart
    /// from pressing Enter twice.
    running_with: Option<Settled>,
    /// Which message of the interactive protocol the reader is on.
    message: usize,
    /// Which system the recap is showing the four sides of.
    focus: Stage,
    /// What every run in the quest measured, for the closing stage, kept per stage.
    ///
    /// `r` on the attack stage used to wipe one record for the whole quest, taking the tuning
    /// stage's sixty-four-bit circuits with it. Each stage now keeps its own, `r` clears the one
    /// on screen, and the recap reads them all.
    records: Vec<Record>,
    /// The stage that started the run going now, which is where what it measures is filed.
    run_stage: usize,
    /// The stage whose run the outcomes on the panel came from.
    outcomes_stage: Option<usize>,
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
            running_with: None,
            message: 0,
            focus: Stage::Halo2,
            records: vec![Record::default(); SCRIPTS.len()],
            run_stage: 0,
            outcomes_stage: None,
        }
    }

    fn count(&self, index: usize) -> u64 {
        match self.knobs[index].value {
            KnobValue::Count { current, .. } => current,
            _ => 0,
        }
    }

    /// The systems the reader asked for: one of them, or the whole lineage.
    ///
    /// The choice is made on the tuning stage and belongs to it. It used to be read everywhere, so
    /// a reader who picked Sigma there and walked on to the attacks ran Sigma alone, was told "two
    /// of them got through" beside a table with one row, and met a recap with three systems missing.
    fn wanted(&self) -> Vec<Stage> {
        if self.stage != STAGE_TUNE {
            return Stage::ALL.to_vec();
        }
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
        self.run_stage = self.stage;
        self.outcomes_stage = Some(self.stage);
        self.state = RunState::Running;
    }

    fn stop(&mut self) {
        if let Some(runner) = &mut self.runner {
            runner.stop();
            // A system that finished after the last tick is a measurement like any other. It was
            // dropped with the runner, so a reader who pressed Tab a moment after a run ended
            // never saw it reach the recap.
            let mut fresh = Vec::new();
            let _ = runner.drain(&mut fresh);
            if let Some(record) = self.records.get_mut(self.run_stage) {
                for outcome in &fresh {
                    record.add(outcome);
                }
            }
            self.outcomes.append(&mut fresh);
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
        if self.outcomes_stage == Some(self.stage) {
            self.outcomes.clear();
            self.outcomes_stage = None;
        }
        if let Some(record) = self.records.get_mut(self.stage) {
            *record = Record::default();
        }
    }

    /// Everything every stage measured, for the closing sentences.
    fn record(&self) -> Record {
        self.records.iter().fold(Record::default(), |mut all, record| {
            all.merge(record);
            all
        })
    }

    /// A sentence about what this machine measured.
    ///
    /// `Spread` and `Attacks` close the quest and read the whole quest's [`Record`]; `Through` and
    /// `Widened` follow one run and read that run.
    fn tell(&self, topic: Topic, language: Language) -> String {
        match topic {
            Topic::Spread => {
                let record = &self.record();
                if record.runs == 0 {
                    return Msg::YoursNothing.text(language).to_string();
                }
                let time = unit_for(record.slowest_prove);
                let size = size_unit_for(record.largest_proof);
                format!(
                    "{}: {} {} → {}, {} {} → {}, {} {}.",
                    Msg::YoursSpread.text(language),
                    Msg::EventProved.text(language),
                    time_in(record.quickest_prove, time),
                    time_in(record.slowest_prove, time),
                    Msg::EventSize.text(language),
                    size_in(record.smallest_proof, size),
                    size_in(record.largest_proof, size),
                    Msg::YoursRuns.text(language),
                    format::count(record.runs as u64),
                )
            }
            Topic::Attacks => {
                let record = &self.record();
                if record.runs == 0 {
                    return Msg::YoursNothing.text(language).to_string();
                }
                let counted = format!(
                    "{} {}  ·  {} {}",
                    Msg::WordTried.text(language),
                    format::count(record.tried as u64),
                    Msg::BreakSummaryAccepted.text(language),
                    format::count(record.through as u64)
                );
                let verdict = if record.through == 0 {
                    Msg::YoursAttacksNone
                } else {
                    Msg::YoursAttacksThrough
                };
                format!("{counted}. {}", verdict.text(language))
            }
            Topic::Through => {
                let attempts: Vec<&ForgeryAttempt> =
                    self.outcomes.iter().flat_map(|o| o.forgery.attempts.iter()).collect();
                let through = attempts.iter().filter(|a| a.accepted).count();
                format!(
                    "{} {}  ·  {} {}",
                    Msg::BreakGotThrough.text(language),
                    through,
                    Msg::WordTried.text(language),
                    attempts.len()
                )
            }
            Topic::Widened => match self.outcome(Stage::Halo2) {
                Some(outcome) => {
                    let bits = match &outcome.detail {
                        StageDetail::Halo2(run) => run.config.value_bits,
                        _ => 0,
                    };
                    let prove = outcome.measurement.prove_nanos;
                    format!(
                        "{}  ·  {} {} {}  ·  {} {}. {}",
                        phrases::stage(Stage::Halo2).text(language),
                        Msg::KnobBits.text(language),
                        bits,
                        Msg::UnitBits.text(language),
                        Msg::EventProved.text(language),
                        time_in(prove, unit_for(prove)),
                        Msg::TuneOthersFixed.text(language),
                    )
                }
                None => Msg::TuneNoHalo2.text(language).to_string(),
            },
        }
    }

    fn outcome(&self, stage: Stage) -> Option<&StageOutcome> {
        self.outcomes.iter().find(|outcome| outcome.stage == stage)
    }

    /// The unit a column of times is written in: one unit, taken from the largest value in it.
    ///
    /// The table and the conversation ask this of the same runs, so one measurement cannot be
    /// `prove 215 µs` in a beat and `0.22 ms` in the table beside it, which is two numbers for one
    /// thing and leaves the reader to work out that they agree.
    fn unit_over(&self, stages: &[Stage], pick: fn(&Measurement) -> u64) -> TimeUnit {
        let largest = stages
            .iter()
            .filter_map(|stage| self.outcome(*stage))
            .map(|outcome| pick(&outcome.measurement))
            .max()
            .unwrap_or(0);
        unit_for(largest)
    }

    /// The unit a column of proof sizes is written in, taken from the largest of them.
    fn size_unit_over(&self, stages: &[Stage]) -> SizeUnit {
        let largest = stages
            .iter()
            .filter_map(|stage| self.outcome(*stage))
            .map(|outcome| outcome.measurement.proof_bytes)
            .max()
            .unwrap_or(0);
        size_unit_for(largest)
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
        let prove_unit = self.unit_over(stages, |m| m.prove_nanos);
        let verify_unit = self.unit_over(stages, |m| m.verify_nanos);
        let size_unit = self.size_unit_over(stages);

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
                            size_in(measure.proof_bytes, size_unit),
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
            // A panel that says "running" while nothing runs is the reason a reader waits two
            // minutes for a screen that was only ever waiting for them.
            let text =
                if self.state == RunState::Running { Msg::RunWorking } else { Msg::RunNotYet };
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

        // Measured from the three sides in this language rather than padded to twenty cells, which
        // is the English width with nothing to spare.
        let sides = [Msg::SideProver, Msg::SideVerifier, Msg::SidePublic];
        let side_column = label_width(&sides, language, width, VALUE_MIN);
        let mut second = vec![Span::styled(
            label_cell(phrases::side(line.side).text(language), side_column),
            theme.muted(),
        )];
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
    fn attack_lines(
        &self,
        width: usize,
        height: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        if self.outcomes.is_empty() {
            // A panel that says "running" while nothing runs is the reason a reader waits two
            // minutes for a screen that was only ever waiting for them.
            let text =
                if self.state == RunState::Running { Msg::RunWorking } else { Msg::RunNotYet };
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
                lines.extend(folded_row(
                    (&format!("{} ", state.mark()), theme.state(state)),
                    (phrases::forgery(attempt.kind).text(language), theme.plain()),
                    (verdict.text(language), theme.state(state)),
                    label_column,
                    width,
                ));
            }
        }

        let summary = Line::from(Span::styled(
            format!(
                "{} {}  ·  {} {}",
                Msg::WordTried.text(language),
                attempts,
                Msg::BreakSummaryAccepted.text(language),
                accepted
            ),
            theme.muted(),
        ));
        let (waste, note) = self.waste_lines(width, language, theme);
        // The note says in a sentence what the conversation beside it has just said, so it is the
        // first thing to go when the panel is short, and the gaps are the second. The attacks, the
        // opening and the count are what the stage is about, and they stay.
        let gap = || Line::from("");
        let mut with_note = lines.clone();
        with_note.push(gap());
        with_note.extend(waste.iter().cloned());
        if !note.is_empty() {
            with_note.push(gap());
            with_note.extend(note);
            with_note.push(gap());
        }
        with_note.push(summary.clone());
        if with_note.len() <= height {
            return with_note;
        }
        let mut spaced = lines.clone();
        spaced.push(gap());
        spaced.extend(waste.iter().cloned());
        if !waste.is_empty() {
            spaced.push(gap());
        }
        spaced.push(summary.clone());
        if spaced.len() <= height {
            return spaced;
        }
        lines.extend(waste);
        lines.push(summary);
        lines
    }

    /// Everything this stage's panel draws, fitted to `height` rows.
    ///
    /// Every stage builds its lines here, so every stage is fitted the same way: a panel that
    /// does not fit loses whole rows from its end and says so, rather than a `Paragraph` cutting
    /// it off without a word. At 80x24 the tuning stage showed two of the circuit's seven rows —
    /// the rows its last three sentences explain — and nothing said the rest were there.
    fn panel_lines(
        &self,
        width: usize,
        height: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let lines = match self.stage {
            STAGE_RUN => self.table(&Stage::ALL, width, language, theme),
            STAGE_MESSAGES => self.message_lines(width, language, theme),
            STAGE_TUNE => self.tune_lines(width, height, language, theme),
            STAGE_BREAK => self.attack_lines(width, height, language, theme),
            STAGE_SIDES => self.recap_lines(width, height, language, theme),
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
                // The gap after the last system is only a gap.
                if lines.last().is_some_and(|line| line.spans.is_empty()) {
                    lines.pop();
                }
                lines
            }
        };
        trimmed(lines, width, height, language, theme)
    }

    /// The tuning stage: the knobs, the table, and the circuit halo2 built.
    ///
    /// Where the three do not fit with a blank line between them, the gaps go first and the
    /// circuit's rows sit two to a line, which is what lets every one of them stay at 80x24.
    fn tune_lines(
        &self,
        width: usize,
        height: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let mut head = self.knob_lines(width, language, theme);
        let stages = self.wanted();
        head.push(Line::from(""));
        head.extend(self.trust_lines(&stages, width, language, theme));
        let table = self.table(&stages, width, language, theme);
        let rows = self.circuit_rows(language);
        let heading =
            Line::from(Span::styled(Msg::ShapeTitle.text(language).to_string(), theme.heading()));

        let roomy = {
            let mut lines = head.clone();
            lines.push(Line::from(""));
            lines.extend(table.iter().cloned());
            if !rows.is_empty() {
                lines.push(Line::from(""));
                lines.push(heading.clone());
                lines.extend(widgets::stat_lines(&rows, width, theme));
            }
            lines
        };
        if roomy.len() <= height {
            return roomy;
        }
        let mut lines = head;
        lines.extend(table);
        if !rows.is_empty() {
            lines.push(heading);
            lines.extend(paired_stats(&rows, width, theme));
        }
        lines
    }

    /// The acceptance that is the reason people ask whether a ceremony was honest.
    ///
    /// The rows and the note are handed back apart, so a short panel can keep the one and not the
    /// other.
    fn waste_lines(
        &self,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> (Vec<Line<'static>>, Vec<Line<'static>>) {
        let Some(StageDetail::TrustedSetup(run)) =
            self.outcome(Stage::TrustedSetup).map(|outcome| &outcome.detail)
        else {
            return (Vec::new(), Vec::new());
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
        let row = |label: Msg, value: String, style| {
            folded_row(
                ("", theme.muted()),
                (label.text(language), theme.muted()),
                (&rpad(&value, WASTE_VALUE_WIDTH), style),
                label_column,
                width,
            )
        };
        let mut lines = Vec::new();
        lines.extend(row(Msg::WasteHolds, format::count(run.true_value), theme.plain()));
        lines.extend(row(
            Msg::WasteOpened,
            format::count(run.with_waste.claimed_value),
            theme.plain(),
        ));
        lines.extend(row(
            Msg::WasteVerifier,
            format!("{} {}", state.mark(), verdict.text(language)),
            theme.state(state),
        ));
        let note = wrap(Msg::WasteNote.text(language), width)
            .into_iter()
            .map(|text| Line::from(Span::styled(text, theme.plain())))
            .collect();
        (lines, note)
    }

    /// The knobs, with the chosen one marked.
    ///
    /// The label column is measured from the three labels in the language on screen. It was a
    /// fifteen-cell `pad`, which widens and never narrows, so a label past fifteen cells ran
    /// straight into its value.
    fn knob_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let labels = [Msg::KnobStage, Msg::KnobBits, Msg::KnobSeed];
        let label_column = label_width(&labels, language, width.saturating_sub(2), VALUE_MIN);
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
                    Span::styled(
                        label_cell(labels[index].text(language), label_column),
                        theme.plain(),
                    ),
                    Span::styled(value, if picked { theme.heading() } else { theme.muted() }),
                ])
            })
            .collect()
    }

    fn system_text(&self, language: Language) -> &'static str {
        match self.knobs[KNOB_STAGE].value {
            KnobValue::Choice { current, .. } => system_name(current, language),
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
        let label = label_width(&[Msg::LabelTrust], language, width, VALUE_MIN);
        match stages {
            [only] => match self.outcome(*only) {
                Some(outcome) => {
                    let text = phrases::setup_kind(outcome.trust.setup).text(language);
                    wrap(text, width.saturating_sub(label))
                        .into_iter()
                        .enumerate()
                        .map(|(index, part)| {
                            let head = if index == 0 {
                                label_cell(Msg::LabelTrust.text(language), label)
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
            // A panel that says "running" while nothing runs is the reason a reader waits two
            // minutes for a screen that was only ever waiting for them.
            let text =
                if self.state == RunState::Running { Msg::RunWorking } else { Msg::RunNotYet };
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
                // The size the table, the conversation and this panel say is one measurement, read
                // here from the one field they all read. (The onlooker's own view once counted the
                // public statement as proof, 128 where the proof is 96; the engine no longer does,
                // and reading one field keeps the two from drifting apart again.)
                format!(
                    "{} {}",
                    Msg::WordProof.text(language),
                    format::bytes(outcome.measurement.proof_bytes as u64)
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

        // The notice is written before the rows are trimmed, because it is rows itself: a notice
        // about dropped rows that is dropped in half says less than nothing.
        let notice = wrap(Msg::PanelTrimmed.text(language), width);
        let dropped = fit(&mut blocks, height, notice.len());
        let mut lines: Vec<Line<'static>> = blocks.into_iter().flatten().flatten().collect();
        if dropped {
            for part in notice {
                lines.push(Line::from(Span::styled(part, theme.muted())));
            }
        }
        lines
    }
}

/// `lines` cut to `height` rows, ending in a line that says rows were left out when any were.
///
/// Rows go from the end, whole, and the notice is counted before they go, so it is never the
/// thing that is cut.
fn trimmed(
    mut lines: Vec<Line<'static>>,
    width: usize,
    height: usize,
    language: Language,
    theme: Theme,
) -> Vec<Line<'static>> {
    if height == 0 || lines.len() <= height {
        return lines;
    }
    let notice = wrap(Msg::PanelTrimmed.text(language), width);
    lines.truncate(height.saturating_sub(notice.len()));
    while lines
        .last()
        .is_some_and(|line| line.spans.iter().all(|span| span.content.trim().is_empty()))
    {
        lines.pop();
    }
    lines.extend(notice.into_iter().map(|part| Line::from(Span::styled(part, theme.muted()))));
    lines.truncate(height);
    lines
}

/// Label-and-value rows two to a line where two fit side by side, one to a line where they do
/// not. Each row stays whole: a pair is only made of rows that both fit.
fn paired_stats(rows: &[(&str, String)], width: usize, theme: Theme) -> Vec<Line<'static>> {
    const BETWEEN: &str = "  ·  ";
    let cell = |(label, value): &(&str, String)| {
        vec![
            Span::styled(format!("{label} "), theme.muted()),
            Span::styled(value.clone(), theme.plain()),
        ]
    };
    let size = |(label, value): &(&str, String)| text::width(label) + 1 + text::width(value);
    let mut lines = Vec::new();
    let mut rows = rows.iter().peekable();
    while let Some(first) = rows.next() {
        let mut spans = cell(first);
        if let Some(second) = rows.peek()
            && size(first) + text::width(BETWEEN) + size(second) <= width
        {
            spans.push(Span::styled(BETWEEN, theme.muted()));
            spans.extend(cell(second));
            rows.next();
        }
        lines.push(Line::from(spans));
    }
    lines
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

/// One row of a label and the thing it is about, folded onto more lines rather than cut.
///
/// The value sits beside the label while the label fits the column it was measured into. When it
/// does not, the label takes as many lines as it needs and its value follows underneath, in the
/// column it would have been in. Nothing is ever cut: `The unchanged verifier sa…` is not a label,
/// and a reader cannot guess the rest of a sentence from its first half. A label wide enough to
/// fold is ordinary rather than rare — Korean reaches widths English never does, and at eighty
/// columns English reaches them too.
///
/// `lead` is the mark in front of the row, drawn on its first line and stood in for by spaces
/// afterwards. `label_column` is what [`label_width`] measured for this block of rows.
fn folded_row(
    lead: (&str, Style),
    label: (&str, Style),
    value: (&str, Style),
    label_column: usize,
    width: usize,
) -> Vec<Line<'static>> {
    let indent = text::width(lead.0);
    let room = width.saturating_sub(indent);
    let beside = room.saturating_sub(label_column).max(1);
    let mut lines: Vec<Line<'static>> = Vec::new();

    // `label_cell` spends the last cell of the column on the gap after the label.
    if text::width(label.0) >= label_column {
        for part in wrap(label.0, room) {
            let mut spans = Vec::new();
            if lines.is_empty() {
                spans.push(Span::styled(lead.0.to_string(), lead.1));
            } else if indent > 0 {
                spans.push(Span::styled(" ".repeat(indent), label.1));
            }
            spans.push(Span::styled(part, label.1));
            lines.push(Line::from(spans));
        }
    }

    // A value that fits is kept whole, so one padded to line up with the rows above it still does.
    let parts = if text::width(value.0) <= beside {
        vec![value.0.to_string()]
    } else {
        wrap(value.0, beside)
    };
    for part in parts {
        let mut spans = Vec::new();
        if lines.is_empty() {
            spans.push(Span::styled(lead.0.to_string(), lead.1));
            spans.push(Span::styled(label_cell(label.0, label_column), label.1));
        } else {
            spans.push(Span::styled(" ".repeat(indent + label_column), label.1));
        }
        spans.push(Span::styled(part, value.1));
        lines.push(Line::from(spans));
    }
    lines
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
) -> Vec<Vec<Line<'static>>> {
    // The party's one fact wraps under itself like every other row: cutting it lost the reader
    // what the sender kept, which is half of what the row was there to say.
    let mut block = vec![folded_row(
        ("", theme.heading()),
        (name.text(language), theme.heading()),
        (&fact, theme.muted()),
        columns.name,
        columns.width,
    )];
    for (label, text) in rows {
        block.push(folded_row(
            ("", theme.muted()),
            (label.text(language), theme.muted()),
            (text, theme.plain()),
            columns.label,
            columns.width,
        ));
    }
    block
}

/// Trims the tallest block until every party still fits on one screen. A short panel loses the tail
/// of a list rather than a whole party — at 80x24 that is already true in English, so the notice
/// below is an ordinary sight rather than a rare one.
///
/// Says whether it dropped anything, so the panel can end with a line admitting it. Rows that
/// vanish with nothing on screen to say so read as a panel that has nothing more to show.
///
/// `notice` is how many rows that admission takes at this width. They are held back before
/// anything is dropped, so the notice is never the thing that has to be trimmed.
///
/// A block is a list of rows and a row may be several lines, because a long list wraps. Rows go
/// whole: trimming line by line left the onlooker's "never" row in Korean ending on
/// "받는 주소," — half a list, with the comma promising a rest that had been cut.
fn fit(blocks: &mut [Vec<Vec<Line<'static>>>], height: usize, notice: usize) -> bool {
    let lines = |block: &Vec<Vec<Line<'static>>>| block.iter().map(Vec::len).sum::<usize>();
    let mut total: usize = blocks.iter().map(lines).sum();
    if height == 0 || total <= height {
        return false;
    }
    let room = height.saturating_sub(notice);
    let mut dropped = false;
    while total > room {
        // A party's own line always stays, so every party is still on screen.
        let Some(tallest) =
            blocks.iter_mut().filter(|block| block.len() > 1).max_by_key(|block| lines(block))
        else {
            break;
        };
        let Some(row) = tallest.pop() else { break };
        total -= row.len();
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
        // What earlier stages measured is kept, but it is not news. Counting from zero here
        // announced every run the reader had already watched as if it had just happened, on the
        // second beat of the next stage, before that stage had run anything — the attack stage
        // listed each system twice.
        self.reported = self.outcomes.len();
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
            let settled = Settled::of(self.knobs());
            self.running_with = Some(settled);
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

    /// Whether the reader has turned a knob since the run in progress started.
    fn knobs_moved(&self) -> bool {
        match &self.running_with {
            Some(settled) => !settled.still(self.knobs()),
            None => false,
        }
    }

    /// Whether the next step of this stage's conversation starts a run.
    fn next_is_a_run(&self) -> bool {
        matches!(self.script().get(self.revealed + 1), Some(Run(_)))
    }

    /// Runs this stage's work again with the values now on screen, in place of the run before it,
    /// so the sentences that read the result are about the run that just happened.
    fn rerun(&mut self) -> Reaction {
        if self.knobs().is_empty() {
            return Reaction::Ignored;
        }
        let script = self.script();
        // The run this stage is showing, not the last one written in the script: a reader in the
        // middle of the first run's sentences must not be thrown forward past what they have
        // not read yet.
        let upto = self.revealed.min(script.len().saturating_sub(1));
        let Some(at) = script[..=upto].iter().rposition(|step| matches!(step, Run(_))) else {
            return Reaction::Ignored;
        };
        if at == 0 {
            return Reaction::Ignored;
        }
        self.stop_and_forget_the_telling();
        self.log.retain(|logged| logged.step < at);
        self.revealed = at - 1;
        self.advance();
        Reaction::Handled
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
                // verifier caught are the table's job. And only on the stage that is about
                // attacks: two unexplained "attack · accepted" lines four stages early read as
                // the program breaking, or as the reader having broken it.
                let attacks = self.stage == STAGE_BREAK;
                out.extend(outcome.forgery.attempts.iter().filter(|f| attacks && f.accepted).map(
                    |forgery| Happening::Attack {
                        stage: outcome.stage,
                        kind: forgery.kind,
                        accepted: forgery.accepted,
                    },
                ));
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
            // The same unit the table gives these columns: a beat and the row it is about are two
            // ways of reading one measurement, and a reader who has to convert between them is
            // being asked to check the program's arithmetic.
            Happening::Measured { stage, prove, verify, bytes, accepted } => Beat::outcome(
                if *accepted { State::Good } else { State::Bad },
                format!(
                    "{}  ·  {} {}  ·  {} {}  ·  {} {}",
                    phrases::stage(*stage).text(language),
                    Msg::EventProved.text(language),
                    time_in(*prove, self.unit_over(&Stage::ALL, |m| m.prove_nanos)),
                    Msg::EventVerified.text(language),
                    time_in(*verify, self.unit_over(&Stage::ALL, |m| m.verify_nanos)),
                    Msg::EventSize.text(language),
                    size_in(*bytes, self.size_unit_over(&Stage::ALL)),
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
            Happening::PulledIn { to } => Beat::outcome(
                State::Chosen,
                format!(
                    "{}  ·  {} {}",
                    Msg::EventOutsideRange.text(language),
                    Msg::EventSetTo.text(language),
                    to,
                ),
            ),
            Happening::SystemPulledIn { current } => Beat::outcome(
                State::Chosen,
                format!(
                    "{}  ·  {} {}",
                    Msg::EventOutsideRange.text(language),
                    Msg::EventSetTo.text(language),
                    system_name(*current, language),
                ),
            ),
            Happening::NotANumber => Beat::outcome(State::Bad, Msg::EventNotANumber.text(language)),
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
                // A knob turned since this stage's run started asks for the run to be done again
                // with what is on screen. That comes before carrying the conversation on: the
                // sentences after a run are about that run, and they would be about the old one.
                //
                // Unless the next step is a run of its own. There the values on screen are for that
                // run — "now give the attacker 51% and press Enter" — and going back to redo the
                // run before it wiped the 30% attack out of the conversation and ran 51% twice.
                if self.knobs_moved() && !self.next_is_a_run() && self.rerun() == Reaction::Handled
                {
                    return Reaction::Handled;
                }
                if let Some(Await(until)) = self.script().get(self.revealed)
                    && !self.satisfied(*until)
                {
                    return Reaction::Ignored;
                }
                if self.advance() {
                    return Reaction::Handled;
                }
                // Where there are knobs, Enter runs it again with the values on screen; walking
                // on is Tab's job. Where there are none the shell walks.
                self.rerun()
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
                if self.knob_count() == 0 {
                    return Reaction::Ignored;
                }
                // A number that goes nowhere reads as a broken key unless the screen says what
                // happened to it.
                match self.knobs[self.chosen].commit() {
                    // The system knob is drawn as the system's name, so that is where it landed;
                    // "set to 4" beside a panel reading "halo2" is two answers to one question.
                    Typed::PulledIn { .. } if self.chosen == KNOB_STAGE => {
                        if let KnobValue::Choice { current, .. } = self.knobs[KNOB_STAGE].value {
                            self.say(Happening::SystemPulledIn { current });
                        }
                    }
                    Typed::PulledIn { to } => self.say(Happening::PulledIn { to }),
                    Typed::NotANumber => self.say(Happening::NotANumber),
                    Typed::Taken | Typed::Nothing => {}
                }
                Reaction::Handled
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
            let mut fresh = Vec::new();
            let (finished, failure) = runner.drain(&mut fresh);
            if let Some(record) = self.records.get_mut(self.run_stage) {
                for outcome in &fresh {
                    record.add(outcome);
                }
            }
            self.outcomes.append(&mut fresh);
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
            STAGE_RUN => Msg::RunTitle,
            // This panel shows one message of the protocol, not four systems and three numbers.
            STAGE_MESSAGES => Msg::MessagesTitle,
            STAGE_TUNE => Msg::TuneTitle,
            STAGE_BREAK => Msg::BreakTitle,
            STAGE_SIDES => Msg::RecapTitle,
            _ => Msg::BriefPanelTitle,
        };
        let heading = match self.stage {
            STAGE_SIDES => match self.focused() {
                Some(outcome) => {
                    let both = format!(
                        "{}  ·  {}",
                        title.text(language),
                        phrases::stage(outcome.stage).text(language)
                    );
                    // A title is drawn over the top border, and this one carries a second thing:
                    // the system the reader is looking at. Where both will not fit, the panel
                    // keeps its name and the system drops off whole, because a title cut in the
                    // middle of a word costs the border and says less than the name alone.
                    if text::width(&both) <= theme::title_room(area.width) {
                        both
                    } else {
                        title.text(language).to_string()
                    }
                }
                None => title.text(language).to_string(),
            },
            _ => title.text(language).to_string(),
        };
        let block = theme.titled_panel_in(&heading, area.width);
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

        frame
            .render_widget(Paragraph::new(self.panel_lines(width, height, language, theme)), inner);
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            // The arrows pick a value and change it. They were labelled "System", the name of the
            // first knob, which is one of the three things they move.
            STAGE_TUNE => vec![
                ("↑↓ ←→", Msg::KeyChange.text(language)),
                ("0-9", Msg::KeyTypeNumber.text(language)),
            ],
            STAGE_SIDES => vec![("↑↓", Msg::KeySystem.text(language))],
            _ => Vec::new(),
        }
    }

    fn go_name(&self, language: Language) -> Option<&'static str> {
        let runnable = !self.knobs().is_empty() && (self.at_end() || self.knobs_moved());
        runnable.then(|| Msg::KeyRunIt.text(language))
    }

    fn typing(&self) -> bool {
        self.knob_count() > 0 && self.knobs[self.chosen].draft().is_some()
    }

    fn close(&mut self) {
        self.stop();
    }
}

/// What position `current` of the system knob is called: all four, or one of them.
fn system_name(current: usize, language: Language) -> &'static str {
    match current {
        1..=4 => phrases::stage(Stage::ALL[current - 1]).text(language),
        _ => Msg::StageAll.text(language),
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

/// A unit of size, and how a column of byte counts is written in it.
///
/// The size column read `96 B`, `64 B`, `40 B` and `1.44 KiB` — four units' worth of rule broken in
/// one column, and the same trap as the times: 96 beside 1.44 invites the wrong comparison.
#[derive(Clone, Copy)]
struct SizeUnit {
    divisor: f64,
    suffix: &'static str,
}

const BYTES: SizeUnit = SizeUnit { divisor: 1.0, suffix: "B" };
const KIBIBYTES: SizeUnit = SizeUnit { divisor: 1024.0, suffix: "KiB" };
const MEBIBYTES: SizeUnit = SizeUnit { divisor: 1024.0 * 1024.0, suffix: "MiB" };

/// The unit the largest size in a column wants, which is the unit the whole column gets. The same
/// thresholds as `nmtk_core::format::bytes`, so a size alone in its column reads exactly as it does
/// everywhere else in nmtk.
fn size_unit_for(bytes: usize) -> SizeUnit {
    if bytes < 1024 {
        BYTES
    } else if bytes < 1024 * 1024 {
        KIBIBYTES
    } else {
        MEBIBYTES
    }
}

fn size_in(bytes: usize, unit: SizeUnit) -> String {
    if unit.divisor == 1.0 {
        format!("{bytes} {}", unit.suffix)
    } else {
        format!("{:.2} {}", bytes as f64 / unit.divisor, unit.suffix)
    }
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
                for line in session.attack_lines(width, usize::MAX, language, theme) {
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
                    assert!(
                        sentences(text) <= 2,
                        "stage {stage} in {language} is more than two sentences: {text:?}"
                    );
                    assert!(!text.is_empty(), "stage {stage} has a blank beat in {language}");
                }
            }
        }
    }

    /// Sentences in a beat: a full stop, question or exclamation mark that ends the text or is
    /// followed by a space. `0.14` and `s = r + c` are not sentence ends.
    fn sentences(text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        chars
            .iter()
            .enumerate()
            .filter(|(index, c)| {
                matches!(c, '.' | '?' | '!')
                    && chars.get(index + 1).is_none_or(|next| next.is_whitespace())
            })
            .count()
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

        // `r` there forgets only what that stage ran, and these came from the first stage.
        KqSession::on(&mut session, Action::Reset);
        assert_eq!(session.outcomes.len(), measured, "r on another stage took them");
        // `r` on the stage that ran them is the one key that forgets them.
        KqSession::go_to(&mut session, 0);
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
        let said = cut.iter().map(drawn).collect::<Vec<_>>().join(" ");
        let head: String = Msg::PanelTrimmed.text(Language::KOREAN).chars().take(10).collect();
        assert!(said.contains(&head), "the panel said nothing about the rows it dropped: {said:?}");
    }

    /// Trimming went line by line, so a wrapped list lost its tail and kept a comma promising more.
    #[test]
    fn a_short_recap_drops_whole_rows_and_never_half_a_list() {
        let session = finished();
        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        for language in [Language::ENGLISH, Language::KOREAN] {
            for height in 10..=24 {
                let lines: Vec<String> =
                    session.recap_lines(width, height, language, theme).iter().map(drawn).collect();
                // A list that wraps ends a line on a comma, and the line under it carries on,
                // indented. A comma with no indented line after it is a list cut in half.
                for (index, line) in lines.iter().enumerate() {
                    if line.trim_end().ends_with(',') {
                        let next = lines.get(index + 1).map(String::as_str).unwrap_or("");
                        assert!(
                            next.starts_with(' '),
                            "{language} at {height} rows cut a list in half: {lines:#?}"
                        );
                    }
                }
            }
        }
    }

    /// A label wider than its column was cut where it ran out: `The unchanged verifier sa…` is
    /// half a sentence, and a reader cannot finish it for themselves.
    #[test]
    fn a_label_too_wide_for_its_column_folds_instead_of_being_cut() {
        let session = finished();
        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        for language in [Language::ENGLISH, Language::KOREAN] {
            let lines: Vec<String> = session
                .attack_lines(width, usize::MAX, language, theme)
                .iter()
                .map(drawn)
                .collect();
            let panel = lines.join("\n");
            assert!(!panel.contains('\u{2026}'), "a label was cut in {language}:\n{panel}");
            for label in [Msg::WasteHolds, Msg::WasteOpened, Msg::WasteVerifier] {
                let said = label.text(language);
                assert!(
                    lines.iter().any(|line| line.contains(said)),
                    "{said:?} is not on the panel whole in {language}:\n{panel}"
                );
            }
        }
    }

    /// The line admitting that rows were dropped was itself dropped in half, which leaves the
    /// reader a notice they cannot read about rows they cannot see.
    #[test]
    fn the_notice_about_dropped_rows_is_never_itself_trimmed() {
        let session = finished();
        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        for language in [Language::ENGLISH, Language::KOREAN] {
            let cut: Vec<String> =
                session.recap_lines(width, 14, language, theme).iter().map(drawn).collect();
            let notice = Msg::PanelTrimmed.text(language);
            let parts = text::wrap(notice, width);
            let tail: Vec<String> = cut.iter().rev().take(parts.len()).rev().cloned().collect();
            assert_eq!(
                tail.join(" "),
                notice,
                "the notice was not said in full in {language}: {tail:?}"
            );
        }
    }

    /// A party's own row was cut at the panel's edge — at a hundred columns in Korean that lost
    /// the sender's change, which is the number the row was drawn for.
    #[test]
    fn a_party_row_too_wide_for_the_panel_wraps_under_itself() {
        let session = finished();
        let theme = Theme::new(true);
        let sender = &session.focused().expect("a finished run").views.sender;
        let sent = format::count(sender.amount);
        let kept = format::count(sender.change);
        for total in [MIN_WIDTH, 100] {
            let width = panel_width(total, theme);
            for language in [Language::ENGLISH, Language::KOREAN] {
                let panel = session
                    .recap_lines(width, 200, language, theme)
                    .iter()
                    .map(drawn)
                    .collect::<Vec<_>>()
                    .join("\n");
                assert!(!panel.contains('\u{2026}'), "a row was cut in {language}:\n{panel}");
                for number in [&sent, &kept] {
                    assert!(
                        panel.contains(number.as_str()),
                        "{total} columns in {language} lost {number}:\n{panel}"
                    );
                }
            }
        }
    }

    /// One proof, two sizes: the table read what the run measured and the panel counted the whole
    /// transcript, so the interactive protocol was 96 B on one screen and 128 B on the next.
    #[test]
    fn the_panel_and_the_table_give_one_proof_one_size() {
        let mut session = finished();
        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        for stage in [Stage::Sigma, Stage::FiatShamir, Stage::TrustedSetup] {
            session.focus = stage;
            let outcome = session.outcome(stage).expect("a finished run");
            let measured = format::bytes(outcome.measurement.proof_bytes as u64);
            let on_the_wire = format::bytes(outcome.views.onlooker.proof_bytes as u64);
            let table = session
                .table(&[stage], width, Language::ENGLISH, theme)
                .iter()
                .map(drawn)
                .collect::<Vec<_>>()
                .join("\n");
            let panel = session
                .recap_lines(width, 200, Language::ENGLISH, theme)
                .iter()
                .map(drawn)
                .collect::<Vec<_>>()
                .join("\n");
            assert!(table.contains(&measured), "{stage:?}: the table lost its size:\n{table}");
            assert!(
                panel.contains(&measured),
                "{stage:?}: the panel says something other than {measured}:\n{panel}"
            );
            if on_the_wire != measured {
                assert!(
                    !panel.contains(&on_the_wire),
                    "{stage:?}: the panel is still counting {on_the_wire}:\n{panel}"
                );
            }
        }
    }

    /// One quantity in two units on one screen: the conversation said `prove 215 µs` beside a
    /// table row reading `0.22 ms`, and the reader was left to work out that they agree.
    #[test]
    fn the_conversation_writes_a_time_in_the_unit_its_column_uses() {
        let mut session = finished();
        // halo2 is what puts these columns in milliseconds on a real machine, and it is left out
        // of these tests because it really compiles a circuit. One slow row stands in for it.
        let slowest = session.outcomes.len() - 1;
        session.outcomes[slowest].measurement.prove_nanos = 25_000_000;
        session.outcomes[slowest].measurement.verify_nanos = 2_000_000;
        // The beats were said while that row was still quick, so they are said again.
        session.log.clear();
        session.reported = 0;
        session.notice();

        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        for language in Language::ALL {
            let table = session
                .table(&Stage::ALL, width, *language, theme)
                .iter()
                .map(drawn)
                .collect::<Vec<_>>()
                .join("\n");
            let said = KqSession::transcript(&session, *language)
                .iter()
                .map(|beat| beat.text.clone())
                .collect::<Vec<_>>()
                .join("\n");
            for outcome in &session.outcomes {
                let column = session.unit_over(&Stage::ALL, |m| m.prove_nanos);
                let one = time_in(outcome.measurement.prove_nanos, column);
                assert!(table.contains(&one), "the table lost {one}:\n{table}");
                assert!(
                    said.contains(&one),
                    "the conversation writes {:?} in another unit than the table:\n{said}",
                    outcome.stage
                );
            }
        }
    }

    /// A title as wide as the panel leaves the border its two corners and nothing else, and a
    /// longer one is cut where the border ends. The title is what gives way.
    #[test]
    fn a_long_title_leaves_the_panel_its_border() {
        use ratatui::Terminal;
        use ratatui::backend::TestBackend;

        let mut session = finished();
        session.stage = STAGE_SIDES;
        let theme = Theme::new(true);
        for total in [MIN_WIDTH, 90, 100, 120, 160] {
            let (_, run) = split(total);
            for language in Language::ALL {
                for stage in Stage::ALL {
                    session.focus = stage;
                    let mut terminal =
                        Terminal::new(TestBackend::new(run, MIN_HEIGHT)).expect("backend");
                    terminal
                        .draw(|frame| {
                            let area = frame.area();
                            KqSession::render(&session, frame, area, theme, *language);
                        })
                        .expect("draw");
                    let buffer = terminal.backend().buffer().clone();
                    let top: String =
                        (0..buffer.area.width).map(|x| buffer[(x, 0)].symbol()).collect();
                    assert!(
                        top.ends_with("\u{2500}\u{2500}\u{256e}"),
                        "{total} columns in {language}: the title ate the border: {top:?}"
                    );
                }
            }
        }
    }

    /// Runs whatever this session's stage starts, the way the shell does, until it is done.
    fn run_through(session: &mut Session) {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(120);
        for _ in 0..60 {
            if KqSession::can_advance(session) {
                KqSession::on(session, Action::Go);
            }
            KqSession::tick(session);
            if session.at_end() && session.state != RunState::Running {
                break;
            }
            assert!(std::time::Instant::now() < deadline, "the run never finished");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        while session.state == RunState::Running {
            assert!(std::time::Instant::now() < deadline, "the run never finished");
            KqSession::tick(session);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        KqSession::tick(session);
    }

    fn said(session: &Session, language: Language) -> Vec<String> {
        KqSession::transcript(session, language).into_iter().map(|beat| beat.text).collect()
    }

    /// Walking into a stage announced every run the reader had already watched as if it had just
    /// happened: the attack stage listed each system twice, and the tuning stage reported four
    /// measurements on its second beat, before it had run anything.
    #[test]
    fn a_new_stage_does_not_announce_the_runs_before_it() {
        let mut session = finished();
        for stage in [STAGE_MESSAGES, STAGE_TUNE, STAGE_BREAK] {
            KqSession::go_to(&mut session, stage);
            KqSession::tick(&mut session);
            KqSession::on(&mut session, Action::Go);
            KqSession::tick(&mut session);
            let beats = said(&session, Language::ENGLISH);
            assert!(
                !beats.iter().any(|beat| beat
                    .contains(&format!("·  {} ", Msg::EventProved.text(Language::ENGLISH)))),
                "stage {stage} announced an old run: {beats:?}"
            );
        }
    }

    /// The system knob belongs to the tuning stage. Read everywhere, it made the attack stage run
    /// one system and still say two attacks got through.
    #[test]
    fn the_system_chosen_while_tuning_does_not_follow_the_reader_elsewhere() {
        let mut session = Session::new(&machine());
        KqSession::go_to(&mut session, STAGE_TUNE);
        KqSession::on(&mut session, Action::Nudge(1)); // Sigma alone
        assert_eq!(session.wanted(), vec![Stage::Sigma]);
        KqSession::go_to(&mut session, STAGE_BREAK);
        assert_eq!(session.wanted(), Stage::ALL.to_vec());
        KqSession::go_to(&mut session, STAGE_SIDES);
        assert_eq!(session.wanted(), Stage::ALL.to_vec());
    }

    /// The attack stage's count is read off the run, and it agrees with what the engine did.
    #[test]
    fn the_attack_stage_says_how_many_got_through_from_the_run_itself() {
        let session = finished();
        let accepted: usize = session
            .outcomes
            .iter()
            .map(|o| o.forgery.attempts.iter().filter(|a| a.accepted).count())
            .sum();
        let tried: usize = session.outcomes.iter().map(|o| o.forgery.attempts.len()).sum();
        // The three quick systems: the weak Fiat-Shamir hash and the toxic waste get through.
        assert_eq!((accepted, tried), (2, 5));
        let through = session.tell(Topic::Through, Language::ENGLISH);
        assert_eq!(through, "got through 2  ·  tried 5");
    }

    /// "Only halo2 moved" was said about runs halo2 was not in.
    #[test]
    fn what_the_width_did_is_said_from_the_run() {
        let session = finished();
        assert_eq!(
            session.tell(Topic::Widened, Language::ENGLISH),
            Msg::TuneNoHalo2.text(Language::ENGLISH)
        );

        let mut session = Session::new(&machine());
        KqSession::go_to(&mut session, STAGE_TUNE);
        for _ in 0..4 {
            KqSession::on(&mut session, Action::Nudge(1)); // halo2 alone
        }
        KqSession::on(&mut session, Action::Next);
        for c in "24".chars() {
            KqSession::on(&mut session, Action::Type(c));
        }
        KqSession::on(&mut session, Action::Commit);
        run_through(&mut session);
        let widened = session.tell(Topic::Widened, Language::ENGLISH);
        assert!(widened.starts_with("halo2  ·  Circuit size 24 bits"), "{widened:?}");
        session.close();
    }

    /// The closing stage runs everything again, and it used to read only that last run, so the
    /// reader's own numbers were the ones the recap had just made for itself.
    #[test]
    fn the_recap_reads_the_whole_quest_and_not_the_last_run() {
        let mut session = finished();
        let first = session.record();
        assert_eq!(first.runs, 3);
        assert_eq!(first.tried, 5);
        // A second, smaller run: Sigma alone.
        session.start(vec![Stage::Sigma]);
        while session.state != RunState::Done {
            KqSession::tick(&mut session);
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        assert_eq!(session.outcomes.len(), 1, "the last run is one system");
        assert_eq!(session.record().runs, 4);
        assert_eq!(session.record().tried, 6);
        let attacks = session.tell(Topic::Attacks, Language::ENGLISH);
        assert!(attacks.starts_with("tried 6  ·  accepted 2"), "{attacks:?}");
        let spread = session.tell(Topic::Spread, Language::ENGLISH);
        assert!(spread.contains("40 B → 96 B"), "{spread:?}");
        assert!(spread.contains("systems run 4"), "{spread:?}");

        // Walking between stages keeps the record, and `r` forgets only what the stage on screen
        // measured: these runs were made on the first stage.
        KqSession::go_to(&mut session, STAGE_SIDES);
        assert_eq!(session.record().runs, 4);
        KqSession::on(&mut session, Action::Reset);
        assert_eq!(session.record().runs, 4, "`r` on another stage took the first stage's runs");
        KqSession::go_to(&mut session, 0);
        KqSession::on(&mut session, Action::Reset);
        assert_eq!(session.record(), Record::default());
    }

    /// `r` on the attack stage wiped one record for the whole quest, taking the tuning stage's
    /// runs with it. Each stage keeps its own now, and the recap reads them all.
    #[test]
    fn r_on_one_stage_keeps_what_the_other_stages_measured() {
        let mut session = Session::new(&machine());
        let run = |session: &mut Session, stage: usize| {
            KqSession::go_to(session, stage);
            session.start(vec![Stage::Sigma, Stage::FiatShamir]);
            while session.state != RunState::Done {
                KqSession::tick(session);
                std::thread::sleep(std::time::Duration::from_millis(5));
            }
        };
        run(&mut session, STAGE_TUNE);
        run(&mut session, STAGE_BREAK);
        assert_eq!(session.record().runs, 4);
        KqSession::on(&mut session, Action::Reset);
        assert_eq!(
            session.record().runs,
            2,
            "the tuning stage's runs went with the attack stage's"
        );
        assert_eq!(session.records[STAGE_TUNE].runs, 2);
        // Tab forgets nothing.
        KqSession::go_to(&mut session, STAGE_SIDES);
        assert_eq!(session.record().runs, 2);
        session.close();
    }

    /// A system that finished after the last tick was dropped with the runner, so pressing Tab a
    /// moment after a run ended kept it out of the recap.
    #[test]
    fn a_run_that_finished_between_ticks_still_reaches_the_record() {
        let mut session = Session::new(&machine());
        KqSession::go_to(&mut session, STAGE_TUNE);
        session.start(vec![Stage::Sigma]);
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(60);
        while !session.runner.as_ref().is_some_and(|runner| hold(&runner.shared).finished) {
            assert!(std::time::Instant::now() < deadline, "the run never finished");
            std::thread::sleep(std::time::Duration::from_millis(5));
        }
        // No tick: the reader walks on straight away.
        KqSession::go_to(&mut session, STAGE_BREAK);
        assert_eq!(session.record().runs, 1, "the finished system never reached the record");
        assert_eq!(session.records[STAGE_TUNE].runs, 1, "it was filed under the wrong stage");
        session.close();
    }

    /// No Korean line reads 돕니다: it is 돌다 (to run) and 돕다 (to help) at once, and a reader
    /// meets the second first.
    #[test]
    fn no_korean_line_says_helps_where_it_means_runs() {
        for message in Msg::ALL {
            let korean = message.text(Language::KOREAN);
            assert!(!korean.contains("돕니다"), "{message:?} reads as helping: {korean}");
        }
    }

    /// The seed the reader sets fixes the verifier's coin too, and the stage that says nobody
    /// could know c in advance says so beside it.
    #[test]
    fn the_break_stage_admits_the_seed_fixes_the_challenge_too() {
        let recall = BREAK.iter().position(|step| matches!(step, Say(Msg::BreakRecall)));
        let replay = BREAK.iter().position(|step| matches!(step, Say(Msg::BreakReplayable)));
        assert_eq!(replay, recall.map(|at| at + 1), "the admission does not follow the claim");
    }

    /// Every stage fits its panel, at the smallest screen and a larger one, in every language:
    /// whatever does not fit goes whole and the panel says so, and nothing is cut off silently.
    /// At 80x24 the tuning stage showed two of the circuit's seven rows and the attack stage lost
    /// its closing lines, and neither said anything had gone.
    #[test]
    fn no_panel_loses_rows_without_saying_so() {
        use nmtk_kq::theme::MIN_HEIGHT;
        let theme = Theme::new(true);
        let mut session = Session::new(&machine());
        for stage in 0..SCRIPTS.len() {
            KqSession::go_to(&mut session, stage);
            run_through(&mut session);
            for (total, rows) in [(MIN_WIDTH, MIN_HEIGHT), (100, 30)] {
                let width = panel_width(total, theme);
                // The panel's inside: the screen less the title bar, the key bar, the stage strip
                // and the panel's own two borders.
                let height = usize::from(rows) - 5;
                for language in Language::ALL {
                    let lines = session.panel_lines(width, height, *language, theme);
                    assert!(lines.len() <= height, "stage {stage} at {total}x{rows} overflows");
                    // The panel as it is with room to spare. Its last line is what a panel cut
                    // off at the bottom loses first, so it has to be on screen — laid out
                    // however the fitted panel lays it out — or the notice has to be.
                    let full = session.panel_lines(width, usize::MAX, *language, theme);
                    let bare = |text: &str| {
                        text.chars().filter(|c| !c.is_whitespace()).collect::<String>()
                    };
                    let said: String = lines.iter().map(drawn).collect::<Vec<_>>().join(" ");
                    let notice = bare(Msg::PanelTrimmed.text(*language));
                    let last = full.iter().rev().map(drawn).find(|line| !line.trim().is_empty());
                    if let Some(last) = last {
                        assert!(
                            bare(&said).contains(&bare(&last)) || bare(&said).contains(&notice),
                            "stage {stage} at {total}x{rows} in {language} lost {last:?} silently"
                        );
                    }
                    if stage == STAGE_TUNE && total == MIN_WIDTH {
                        // Every row the stage's last sentences explain is on screen.
                        for label in
                            [Msg::ShapeRows, Msg::ShapeAdvice, Msg::ShapeGates, Msg::ShapeDegree]
                        {
                            assert!(
                                said.contains(label.text(*language)),
                                "{label:?} is not on the tuning panel at 80x24 in {language}"
                            );
                        }
                    }
                }
            }
        }
        session.close();
    }

    /// One unit per number column: the size column read `96 B` above `1.44 KiB`.
    #[test]
    fn the_size_column_carries_one_unit() {
        let mut session = finished();
        let slowest = session.outcomes.len() - 1;
        session.outcomes[slowest].measurement.proof_bytes = 1_475;
        let theme = Theme::new(true);
        let width = panel_width(MIN_WIDTH, theme);
        let table: Vec<String> =
            session.table(&Stage::ALL, width, Language::ENGLISH, theme).iter().map(drawn).collect();
        let sizes: Vec<&String> =
            table.iter().filter(|line| line.trim_end().ends_with('B')).collect();
        assert_eq!(sizes.len(), 3, "{table:?}");
        for line in &sizes {
            assert!(line.trim_end().ends_with("KiB"), "a size in another unit: {table:?}");
        }
        assert!(table.iter().any(|line| line.contains("0.09 KiB")), "{table:?}");
        // Alone in its column a size reads as it does everywhere else.
        assert_eq!(size_in(96, size_unit_for(96)), format::bytes(96));
        assert_eq!(size_in(1_475, size_unit_for(1_475)), format::bytes(1_475));
    }

    /// A runner dropped without `stop` left its thread proving on with nobody to read the answer.
    #[test]
    fn a_dropped_runner_takes_its_thread_with_it() {
        let runner = Runner::start(vec![Stage::Sigma, Stage::FiatShamir], 1, 16, machine());
        let shared = Arc::clone(&runner.shared);
        drop(runner);
        assert_eq!(Arc::strong_count(&shared), 1, "the worker thread outlived its runner");
    }

    /// The system knob is drawn as a name, so a number typed past its end lands on a name.
    #[test]
    fn a_system_typed_past_the_end_says_which_system_it_landed_on() {
        let mut session = Session::new(&machine());
        KqSession::go_to(&mut session, STAGE_TUNE);
        KqSession::on(&mut session, Action::Type('9'));
        KqSession::on(&mut session, Action::Commit);
        for language in Language::ALL {
            let beats = said(&session, *language);
            let landed = phrases::stage(Stage::Halo2).text(*language);
            assert!(
                beats
                    .iter()
                    .any(|beat| beat
                        .ends_with(&format!("{} {landed}", Msg::EventSetTo.text(*language)))),
                "{language}: {beats:?}"
            );
        }
    }

    /// Every word a reader has to know is said before it is leaned on, in the stage it is used in,
    /// because a stage can be the first one a reader opens.
    #[test]
    fn every_word_is_explained_before_it_is_used_in_its_stage() {
        // A word, and the beats that explain it.
        let words: &[(&str, &[Msg])] = &[
            ("node", &[Msg::WhatFour]),
            ("verifier", &[Msg::Roles]),
            ("hash", &[Msg::FourHash, Msg::BreakWeakHash]),
            ("circuit", &[Msg::FourCircuit, Msg::RunAsk, Msg::TuneCircuit]),
            ("bit", &[Msg::TuneBitIs]),
            ("byte", &[Msg::RunBytes]),
            ("note", &[Msg::SidesNote]),
            ("commitment", &[Msg::WhatACommitmentIs, Msg::BreakRecall, Msg::SidesCommitment]),
            ("nullifier", &[Msg::SidesNullifier]),
            ("ceremony", &[Msg::BreakCeremonyIs]),
        ];
        for (stage, script) in SCRIPTS.iter().enumerate() {
            // Stage-relative: the tuning stage's "the prover must get right" is about a circuit,
            // and the attack stage says who the prover is before it matters.
            let spoken: Vec<Msg> = script
                .iter()
                .filter_map(|step| match step {
                    Say(msg) | Ask(msg) => Some(*msg),
                    _ => None,
                })
                .collect();
            for (word, explained_by) in words {
                let first = spoken.iter().position(|msg| {
                    msg.text(Language::ENGLISH)
                        .to_lowercase()
                        .split(|c: char| !c.is_alphanumeric())
                        .any(|token| token.starts_with(word))
                });
                if let Some(first) = first {
                    assert!(
                        explained_by.contains(&spoken[first]),
                        "stage {stage} uses {word:?} before explaining it: {:?}",
                        spoken[first].text(Language::ENGLISH)
                    );
                }
            }
        }
    }

    /// The tuning and message panels padded their labels to a width that fitted English, which
    /// `pad` never narrows; measured columns keep the gap in Korean too.
    #[test]
    fn the_tuning_and_message_panels_keep_their_columns_in_both_languages() {
        let mut session = finished();
        session.stage = STAGE_TUNE;
        let theme = Theme::new(true);
        for total in [MIN_WIDTH, 100] {
            let width = panel_width(total, theme);
            for language in [Language::ENGLISH, Language::KOREAN] {
                let mut lines = session.knob_lines(width, language, theme);
                lines.extend(session.trust_lines(&[Stage::Sigma], width, language, theme));
                for index in 0..5 {
                    session.message = index;
                    lines.extend(session.message_lines(width, language, theme));
                }
                for line in &lines {
                    let line_text = drawn(line);
                    assert!(
                        text::width(&line_text) <= width,
                        "{total} columns in {language}: {line_text:?} is wider than {width}"
                    );
                    assert!(
                        columns_keep_their_gap(line),
                        "{total} columns in {language}: {line_text:?} has no gap after its label"
                    );
                }
            }
        }
    }
}
