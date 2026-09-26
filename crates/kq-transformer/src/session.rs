//! The quest a reader has opened: five stages over one training run.
//!
//! The run lives here, on the session, and not on a stage — which is what lets a reader start
//! training, walk off to read the brief, and come back a minute later to find the loss forty
//! points lower. [`Session::tick`] copies the worker's latest numbers out and returns; it never
//! waits for a step and never takes one.

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue, Settled, Typed};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::{char_width, column, truncate, width as cells};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::widgets::{self, fit, stat_lines, wrapped};
use nmtk_transformer::model::AttentionSnapshot;
use nmtk_transformer::{
    CORPUS, EXPANSION, Optimizer, PROMPT, StartError, Tokenizer, TrainingConfig, TrainingHandle,
    TrainingSnapshot, TrainingState,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::phrases::{self, Msg};

/// Where each tuning knob sits in the list the reader moves through.
const TUNE_LAYERS: usize = 0;
const TUNE_HEADS: usize = 1;
const TUNE_WIDTH: usize = 2;
const TUNE_RATE: usize = 3;
const TUNE_STEPS: usize = 4;

/// Where each attack knob sits.
const BREAK_ATTACK: usize = 0;
const BREAK_MULTIPLIER: usize = 1;

/// Shapes worth trying. A reader who wants five layers types 5.
const LAYER_PRESETS: &[u64] = &[1, 2, 3, 4];
const HEAD_PRESETS: &[u64] = &[1, 2, 4, 8];
const WIDTH_PRESETS: &[u64] = &[32, 64, 80, 128];
const STEP_PRESETS: &[u64] = &[500, 1500, 2500, 4000];
const RATE_PRESETS: &[f64] = &[0.001, 0.003, 0.01];
const MULTIPLIER_PRESETS: &[f64] = &[1.0, 10.0, 100.0, 1000.0, 3000.0];

/// What the multiplier starts at on the Break stage.
///
/// Measured on this engine over a whole default run rather than the first few hundred steps, which
/// matters because the schedule decays the rate as the run goes on: at a hundred and at a thousand
/// times the sensible rate the loss stalls near 2.9 and the model answers with spaces, which is a
/// dull failure. At three thousand it never falls below where it started at all, peaks past a
/// thousand, ends near 9, and the answer is letters repeated. The stage opens on the number that
/// shows the failure it promises.
const BREAKING_MULTIPLIER: f64 = 3000.0;

/// The three ways this engine can be made to fail honestly.
const ATTACK_COUNT: usize = 3;
const ATTACK_RUNAWAY: usize = 0;
const ATTACK_NO_WARMUP: usize = 1;
const ATTACK_PLAIN_SGD: usize = 2;

/// Cells between two values worth trying, and the gap kept after the last of them. A row that
/// ends in the panel's last column cannot say whether another value was cut off it; a gap as wide
/// as the one between two values can.
const PRESET_GAP: usize = 3;

/// The fewest positions worth drawing as a grid.
const MIN_GRID: usize = 3;

/// Five shades, darkest last. Attention is structure, so it is drawn in black and white; the four
/// state colours are kept for state.
const SHADES: [&str; 5] = [" ", "░", "▒", "▓", "█"];

/// Where each stage sits. The order here is the order `lib.rs` declares them in.
#[cfg(test)]
const STAGE_QUESTION: usize = 0;
const STAGE_PIECES: usize = 1;
const STAGE_TRAIN: usize = 2;
const STAGE_TUNE: usize = 3;
const STAGE_BREAK: usize = 4;
const STAGE_RECAP: usize = 5;

/// The losses worth stopping to mention, highest first. Crossing one is news; the thousand steps
/// between two of them are not.
const MILESTONES: [f32; 5] = [3.0, 2.0, 1.0, 0.5, 0.2];

/// One move in a stage's conversation.
#[derive(Debug, Clone, Copy)]
enum Step {
    Say(Msg),
    Ask(Msg),
    Run(Deed),
    /// The conversation waits here until the run has got somewhere.
    Await(Until),
    /// A sentence chosen from what actually happened, rather than from what was asked for.
    ///
    /// A reader is free to set whatever they like and often does, so a fixed sentence after a run
    /// is a guess. "Smaller, faster, and worse" was printed over a model the reader had made
    /// bigger, and the SGD verdict over a run that had not used SGD.
    Tell(Topic),
    /// Something the reader has to do, worded from this machine's own numbers.
    ///
    /// "Set the width to 32" was a fixed sentence, and 32 is what a four-core laptop has already
    /// chosen: the reader was asked to change a value to the value it had, and told that doing so
    /// would quarter the weights.
    Suggest(Topic),
}

/// What a [`Step::Tell`] is about.
#[derive(Debug, Clone, Copy)]
enum Topic {
    /// Whether the loss is still falling or has flattened out.
    Progress,
    /// How this run compares with the one before it.
    Settings,
    /// The lesson of one attack, said only if that attack is what ran and the run really did
    /// what the lesson says.
    Attack { kind: usize, multiplier: f64, lesson: Msg, check: Check },
    /// A width half of this machine's own, and what it would do to the weight count.
    SuggestWidth,
    /// How many random numbers the quest started from.
    RecapStart,
    /// Runs, steps and time, over the whole quest.
    RecapWork,
    /// When the sentence first came out, if it ever did.
    RecapSentence,
    /// What breaking it came to.
    RecapBroken,
}

/// What a run has to have done for an attack's lesson to be true of it.
///
/// Choosing the attack and the multiplier the conversation named is not enough on its own: the
/// run is built on the reader's own settings from the stage before, and a lesson written for the
/// machine's defaults is a guess about any other shape.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Check {
    /// The loss never went below where it started, and one character is most of the answer.
    Collapsed,
    /// It learned something, but the answer is not the sentence.
    Misspelt,
    /// The loss did not even halve.
    Crawled,
    /// The answer is the sentence.
    Found,
}

/// Work a step starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Deed {
    /// Train with whatever the tuning knobs say.
    Train,
    /// Train with the attack the reader chose, which is the same code and worse numbers.
    Attack,
}

/// What a waiting step is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Until {
    Steps(usize),
    Finished,
}

use Deed::{Attack, Train};
use Step::{Ask, Await, Run, Say, Suggest, Tell};

/// The one question a language model answers.
const QUESTION: &[Step] = &[
    Say(Msg::QuestionOne),
    Say(Msg::QuestionTwo),
    Say(Msg::QuestionThree),
    Say(Msg::QuestionFour),
    Say(Msg::QuestionFive),
];

/// What the model is made of, with every word on the panel given a meaning.
const PIECES: &[Step] = &[
    Say(Msg::PiecesOne),
    Say(Msg::PiecesTwo),
    Say(Msg::PiecesThree),
    Say(Msg::PiecesFour),
    Say(Msg::PiecesFive),
    Say(Msg::PiecesSix),
    Say(Msg::PiecesSeven),
    Say(Msg::PiecesWidth),
    Say(Msg::PiecesHeads),
    Say(Msg::PiecesLayers),
    Say(Msg::PiecesWindow),
    Say(Msg::PiecesCharacters),
    Say(Msg::PiecesCount),
    Say(Msg::PiecesPromise),
];

/// The run itself, with the conversation waiting on the loss.
const TRAIN: &[Step] = &[
    Say(Msg::TrainOne),
    // The panel counts steps from the moment the run starts, and "Space pauses it between
    // steps" was the first time the word appeared. It is said before it is used.
    Say(Msg::TrainStep),
    Ask(Msg::TrainAsk),
    Run(Train),
    Await(Until::Steps(1)),
    Say(Msg::TrainLoss),
    Say(Msg::TrainScale),
    // The grid is on screen from the first step, so it is explained there rather than after the
    // run: a reader watched an unreadable field of shaded blocks for ninety seconds.
    Say(Msg::TrainGrid),
    Say(Msg::TrainGridRead),
    Await(Until::Steps(300)),
    Say(Msg::TrainAnswer),
    Say(Msg::TrainNoise),
    // Something to say in the long flat stretch, where the conversation used to run out of
    // sentences while the clock kept going.
    Await(Until::Steps(1_000)),
    Tell(Topic::Progress),
    Await(Until::Finished),
    Say(Msg::TrainDone),
];

/// The reader's own shape, answered by a real run.
const TUNE: &[Step] = &[
    Say(Msg::SettingsOne),
    Say(Msg::SettingsTwo),
    Say(Msg::SettingsThree),
    Say(Msg::SettingsFour),
    Say(Msg::SettingsFive),
    Say(Msg::SettingsSix),
    Say(Msg::SettingsSeven),
    Suggest(Topic::SuggestWidth),
    Run(Train),
    Await(Until::Finished),
    Tell(Topic::Settings),
    Say(Msg::SettingsRefused),
];

/// Three ways to make the same code fail, each run for real.
const BREAK: &[Step] = &[
    Say(Msg::BreakOne),
    Say(Msg::BreakTwo),
    Say(Msg::BreakThree),
    Say(Msg::BreakFour),
    Ask(Msg::BreakAskRunaway),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack {
        kind: ATTACK_RUNAWAY,
        multiplier: BREAKING_MULTIPLIER,
        lesson: Msg::BreakAfterRunaway,
        check: Check::Collapsed,
    }),
    Say(Msg::BreakWarmupOne),
    Say(Msg::BreakGradient),
    Say(Msg::BreakWarmupTwo),
    Say(Msg::BreakWarmupThree),
    Ask(Msg::BreakAskWarmup),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack {
        kind: ATTACK_NO_WARMUP,
        multiplier: 10.0,
        lesson: Msg::BreakAfterWarmup,
        check: Check::Misspelt,
    }),
    Say(Msg::BreakSgdOne),
    Say(Msg::BreakSgdName),
    Say(Msg::BreakSgdTwo),
    // AdamW is named before the sentence that uses the name, and the W is said in words rather
    // than as "weight decay", which nothing had explained.
    Say(Msg::BreakAdamName),
    Say(Msg::BreakSgdThree),
    Ask(Msg::BreakAskSgd),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack {
        kind: ATTACK_PLAIN_SGD,
        multiplier: 1.0,
        lesson: Msg::BreakAfterSgd,
        check: Check::Crawled,
    }),
    Ask(Msg::BreakAskSgdAgain),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack {
        kind: ATTACK_PLAIN_SGD,
        multiplier: 100.0,
        lesson: Msg::BreakLesson,
        check: Check::Found,
    }),
];

/// What the reader now knows, beside the numbers they made.
///
/// The sentences that say what happened are read from the record of every run in the quest. They
/// used to be fixed: "a few tens of thousands of random numbers" over a model the reader had
/// made two million weights wide, and "a few thousand of those steps" to a reader who had taken
/// four hundred.
const RECAP: &[Step] = &[
    Tell(Topic::RecapStart),
    Say(Msg::RecapTwo),
    Say(Msg::RecapThree),
    Say(Msg::RecapFour),
    Say(Msg::RecapFive),
    Tell(Topic::RecapWork),
    Tell(Topic::RecapSentence),
    Tell(Topic::RecapBroken),
    Say(Msg::RecapSeven),
];

const SCRIPTS: [&[Step]; 6] = [QUESTION, PIECES, TRAIN, TUNE, BREAK, RECAP];

/// Something that happened, kept as numbers so it can be said again in any language.
enum Happening {
    Started {
        weights: usize,
        steps: usize,
    },
    Loss {
        loss: f32,
        step: usize,
    },
    GotIt {
        step: usize,
    },
    Ended {
        loss: f32,
        seconds: f64,
        fell: bool,
    },
    Refused(Msg),
    /// A typed number that fell outside the knob's range, with where it landed.
    PulledIn {
        to: Landed,
    },
    /// A typed number the knob could not read at all.
    NotANumber,
}

/// Where a typed number landed, kept so it can be said in the reader's words.
///
/// The knob's own text for a choice is its index, counted from nought: a reader who typed 9 into
/// the attack was told "set to 2" over a panel reading "plain SGD, no memory".
enum Landed {
    /// A number, written the way the panel writes it.
    Number(String),
    /// One layer and head of the attention grid, counted from one as the panel counts them.
    View { layer: usize, head: usize },
    /// One of the attacks.
    Attack(usize),
}

/// One happening, filed against the step the reader was on when it happened.
struct Logged {
    step: usize,
    what: Happening,
}

/// A run the reader started, and whether its settings were the sensible ones.
#[derive(Clone)]
struct Finished {
    snapshot: TrainingSnapshot,
    rate: f32,
}

/// Every run of the quest added together — finished, stopped by walking away, honest or broken.
///
/// The recap used to read the last honest run and nothing else, so a reader who trained for
/// twelve hundred steps and then tried a hundred-step shape was told they had taken a hundred.
/// Every run writes in here when it ends, however it ends, and the closing stage reads this.
#[derive(Clone, Debug, Default)]
struct Record {
    runs: usize,
    steps: usize,
    time: std::time::Duration,
    /// The weight count of the very first run: the random numbers the quest started from.
    first_weights: Option<usize>,
    /// Where the very first run's loss started.
    first_loss: Option<f32>,
    /// The lowest loss any run reached.
    lowest: Option<f32>,
    fastest: f64,
    /// The step of the first run that answered with the sentence, when one did.
    sentence_at: Option<usize>,
    broken_runs: usize,
    /// Runs whose loss stopped being a number.
    blew_up: usize,
    /// The run with the lowest loss at its end, honest runs first, without its attention.
    best: Option<TrainingSnapshot>,
    best_is_honest: bool,
}

impl Record {
    fn add(&mut self, run: &TrainingSnapshot, broken: bool) {
        self.runs += 1;
        self.steps += run.step;
        self.time += run.elapsed;
        self.first_weights.get_or_insert(run.parameter_count);
        if self.first_loss.is_none() {
            self.first_loss = first_finite(&run.loss_history).or(finite(run.loss));
        }
        if let Some(low) = lowest(run) {
            self.lowest = Some(self.lowest.map_or(low, |was| was.min(low)));
        }
        self.fastest = self.fastest.max(run.tokens_per_second);
        if answers(&run.completion) {
            self.sentence_at.get_or_insert(run.step);
        }
        if broken {
            self.broken_runs += 1;
        }
        if run.step > 0 && !run.loss.is_finite() {
            self.blew_up += 1;
        }
        let honest = !broken;
        let better = match &self.best {
            None => true,
            Some(_) if honest != self.best_is_honest => honest,
            Some(best) => match (finite(run.loss), finite(best.loss)) {
                (Some(now), Some(was)) => now < was,
                (Some(_), None) => true,
                _ => false,
            },
        };
        if better {
            self.best = Some(light(run));
            self.best_is_honest = honest;
        }
    }
}

/// Everything a [`Step::Tell`] reads, copied at the moment the sentence was first said.
///
/// A `Tell` read the session as it stood whenever the conversation was drawn, so the lesson of
/// the first attack rewrote itself into "those were not the suggested settings" the moment the
/// second attack started, and the settings verdict changed under the reader's eyes with every new
/// run. The numbers a sentence was about are kept with it.
#[derive(Clone)]
struct Facts {
    /// The last run asked for was refused, so nothing ran. The sentences after it used to read
    /// the run before, and told a reader whose settings were refused how their run had gone.
    refused: bool,
    ran: Option<(usize, f64)>,
    latest: Option<TrainingSnapshot>,
    honest: Option<Finished>,
    previous: Option<Finished>,
    record: Record,
    machine: TrainingConfig,
}

pub struct Session {
    stage: usize,
    /// How many steps of this stage have been revealed.
    revealed: usize,
    /// Everything that happened in this stage, oldest first.
    log: Vec<Logged>,
    /// Which loss milestone has already been mentioned.
    milestone: usize,
    /// Whether this run's arrival and its ending have been said yet.
    said_got_it: bool,
    said_ended: bool,
    /// The loss of the run's first measured step, to say whether it ever fell.
    first_loss: Option<f32>,
    /// What this machine chose for itself before the reader touched anything.
    machine_default: TrainingConfig,
    tune: Vec<Knob>,
    attack: Vec<Knob>,
    /// Which layer and head the attention grid shows.
    view: Vec<Knob>,
    chosen: usize,
    /// The run in progress, if there is one. Dropping it stops the worker thread.
    handle: Option<TrainingHandle>,
    /// The settings the run in progress was started with.
    running_config: Option<TrainingConfig>,
    /// True when those settings were not the sensible ones.
    running_broken: bool,
    /// The worker's latest numbers, copied out on the drawing thread.
    latest: Option<TrainingSnapshot>,
    /// The knob values the run in progress was started with, so turning one can be told apart
    /// from pressing Enter twice.
    running_with: Option<Settled>,
    /// The last run that finished with sensible settings.
    honest: Option<Finished>,
    /// The sensible run before that one, so "compare it with the one before" has something to
    /// compare against rather than a sentence guessing what the reader did.
    previous: Option<Finished>,
    /// The attack and multiplier the run that just finished really used.
    ran: Option<(usize, f64)>,
    /// The last run that finished with settings the reader broke.
    broken: Option<Finished>,
    /// Why the last attempt to start a run was refused, if it was.
    refused: Option<StartError>,
    /// How many distinct characters the corpus has. Fixed, and cheap to keep.
    vocabulary: usize,
    /// Every run of the quest, for the recap.
    record: Record,
    /// What each `Tell` of this stage read when it was said, by its step in the script.
    told: Vec<(usize, Facts)>,
}

impl Session {
    pub fn new(machine: &MachineProfile) -> Self {
        let machine_default = TrainingConfig::for_machine(machine);
        let mut session = Self {
            stage: 0,
            revealed: 0,
            log: Vec::new(),
            milestone: 0,
            said_got_it: false,
            said_ended: false,
            first_loss: None,
            machine_default,
            tune: tune_knobs(&machine_default),
            attack: attack_knobs(),
            view: vec![Knob::new("view", KnobValue::Choice { current: 0, count: 1 })],
            chosen: 0,
            handle: None,
            running_config: None,
            running_broken: false,
            latest: None,
            running_with: None,
            honest: None,
            previous: None,
            ran: None,
            broken: None,
            refused: None,
            vocabulary: Tokenizer::from_text(CORPUS).vocab_size(),
            record: Record::default(),
            told: Vec::new(),
        };
        session.rebuild_view(&machine_default);
        session
    }

    // ---- settings ------------------------------------------------------------

    /// The settings the reader has dialled in, on top of whatever this machine chose.
    fn tuned(&self) -> TrainingConfig {
        TrainingConfig {
            layers: self.tune_count(TUNE_LAYERS, self.machine_default.layers as u64) as usize,
            heads: self.tune_count(TUNE_HEADS, self.machine_default.heads as u64) as usize,
            d_model: self.tune_count(TUNE_WIDTH, self.machine_default.d_model as u64) as usize,
            learning_rate: self
                .tune_decimal(TUNE_RATE, f64::from(self.machine_default.learning_rate))
                as f32,
            steps: self.tune_count(TUNE_STEPS, self.machine_default.steps as u64) as usize,
            ..self.machine_default
        }
    }

    /// The same settings with the chosen attack applied.
    ///
    /// With the multiplier back at one and the first attack chosen this is exactly [`Self::tuned`],
    /// which is what makes `r` on the Break stage a recovery rather than a separate mode.
    fn attacked(&self) -> TrainingConfig {
        let base = self.tuned();
        let multiplier = self.attack.get(BREAK_MULTIPLIER).map_or(1.0, decimal_of) as f32;
        match self.attack.get(BREAK_ATTACK).map_or(ATTACK_RUNAWAY, choice_of) {
            ATTACK_NO_WARMUP => TrainingConfig {
                warmup_steps: 0,
                learning_rate: base.learning_rate * multiplier,
                ..base
            },
            ATTACK_PLAIN_SGD => TrainingConfig {
                optimizer: Optimizer::Sgd,
                learning_rate: base.learning_rate * multiplier,
                ..base
            },
            _ => TrainingConfig { learning_rate: base.learning_rate * multiplier, ..base },
        }
    }

    fn tune_count(&self, index: usize, fallback: u64) -> u64 {
        self.tune.get(index).map_or(fallback, count_of)
    }

    fn tune_decimal(&self, index: usize, fallback: f64) -> f64 {
        self.tune.get(index).map_or(fallback, decimal_of)
    }

    /// How many weights the current settings would train, and why they would not if they would not.
    fn weights_now(&self) -> Result<usize, Msg> {
        let config = self.tuned();
        match config.validate() {
            Ok(()) => Ok(config.parameter_count()),
            Err(error) => Err(phrases::config_error(error)),
        }
    }

    // ---- the run -------------------------------------------------------------

    /// Throws away any run and starts a new one. A refusal is kept and shown rather than dropped.
    fn start(&mut self, config: TrainingConfig) {
        self.stop();
        let broken = config != self.tuned();
        match TrainingHandle::start(config) {
            Ok(handle) => {
                self.latest = Some(handle.snapshot());
                self.handle = Some(handle);
                self.running_config = Some(config);
                self.running_broken = broken;
                self.refused = None;
                self.rebuild_view(&config);
            }
            Err(error) => {
                self.latest = None;
                self.running_config = None;
                self.refused = Some(error);
            }
        }
    }

    /// Stops the worker thread and forgets the run in progress, keeping what it measured.
    ///
    /// Walking to another stage stops the work and keeps the numbers: a run left half way was
    /// dropped without a trace, so a reader who trained for a minute and then pressed Tab to the
    /// recap was told nothing had been trained yet.
    fn stop(&mut self) {
        self.end_run(true);
    }

    /// Stops the run in progress and forgets its numbers too. Only `r` does this.
    fn throw_away(&mut self) {
        self.end_run(false);
    }

    fn end_run(&mut self, keep: bool) {
        if let Some(handle) = self.handle.take() {
            // Read before the handle goes: a worker told to stop publishes itself as finished,
            // and a run cut off at step 300 of 1,500 did not finish.
            let last = handle.snapshot();
            // Dropping the handle asks the worker to stop and waits for it, which costs at most
            // one training step.
            drop(handle);
            if keep && last.step > 0 {
                self.file(&last);
            }
        }
        self.latest = None;
        self.running_config = None;
        self.running_broken = false;
    }

    /// Writes a run that has ended into the record, and keeps it for comparison if it finished.
    fn file(&mut self, run: &TrainingSnapshot) {
        let rate = self.running_config.map_or(0.0, |c| c.learning_rate);
        let broken = self.running_broken;
        self.record.add(run, broken);
        if run.state == TrainingState::Finished {
            let finished = Finished { snapshot: light(run), rate };
            if broken {
                self.broken = Some(finished);
            } else {
                self.previous = self.honest.take();
                self.honest = Some(finished);
            }
        }
    }

    /// Puts the attack knobs back where they cannot break anything.
    fn recover(&mut self) {
        self.attack = attack_knobs();
        if let Some(knob) = self.attack.get_mut(BREAK_MULTIPLIER) {
            *knob = Knob::new(
                "multiplier",
                KnobValue::Decimal {
                    current: 1.0,
                    min: 1.0,
                    max: 10_000.0,
                    step: 10.0,
                    presets: MULTIPLIER_PRESETS,
                },
            );
        }
    }

    /// Sizes the layer-and-head chooser to the model that is about to be trained.
    fn rebuild_view(&mut self, config: &TrainingConfig) {
        let count = config.layers.saturating_mul(config.heads).max(1);
        self.view = vec![Knob::new("view", KnobValue::Choice { current: 0, count })];
    }

    /// Which layer and head the attention grid is showing, against the snapshot's own shape.
    fn viewed_head(&self, attention: &AttentionSnapshot) -> (usize, usize) {
        let heads = attention.heads.max(1);
        let index = self.view.first().map_or(0, choice_of);
        let layer = (index / heads).min(attention.layers.saturating_sub(1));
        (layer, index % heads)
    }

    // ---- knobs ---------------------------------------------------------------

    fn stage_knobs(&self) -> &[Knob] {
        match self.stage {
            STAGE_TRAIN => &self.view,
            STAGE_TUNE => &self.tune,
            STAGE_BREAK => &self.attack,
            _ => &[],
        }
    }

    fn stage_knobs_mut(&mut self) -> Option<&mut Knob> {
        let chosen = self.chosen;
        match self.stage {
            STAGE_TRAIN => self.view.get_mut(chosen),
            STAGE_TUNE => self.tune.get_mut(chosen),
            STAGE_BREAK => self.attack.get_mut(chosen),
            _ => None,
        }
    }

    fn move_choice(&mut self, direction: i32) -> Reaction {
        let count = self.stage_knobs().len();
        if count == 0 {
            return Reaction::Ignored;
        }
        let last = count - 1;
        self.chosen = if direction > 0 {
            if self.chosen >= last { 0 } else { self.chosen + 1 }
        } else if self.chosen == 0 {
            last
        } else {
            self.chosen - 1
        };
        Reaction::Handled
    }

    /// One row per knob, the chosen one marked and in the heading weight.
    ///
    /// The label column is measured in cells rather than characters, so the values still line up
    /// when the labels are Korean and each glyph fills two of them.
    fn knob_lines(
        &self,
        labels: &[Msg],
        panel: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let knobs = self.stage_knobs();
        let label_width = labels
            .iter()
            .map(|label| cells(label.text(language)))
            .max()
            .unwrap_or(0)
            .max(12)
            .min(panel.saturating_sub(6))
            + 2;
        let room = panel.saturating_sub(label_width + 2);
        knobs
            .iter()
            .enumerate()
            .flat_map(|(index, knob)| {
                let picked = index == self.chosen;
                let label = labels.get(index).copied().unwrap_or(Msg::LabelWeights);
                let value = match knob.draft() {
                    Some(_) => knob.display(),
                    None => self.knob_value_text(index, knob, language),
                };
                let style = if picked { theme.heading() } else { theme.muted() };
                let mark = Span::styled(
                    format!("{} ", if picked { State::Chosen.mark() } else { " " }),
                    theme.state(State::Chosen),
                );
                // A knob whose value will not fit beside its name puts the value under it. The
                // stage has a dozen blank rows below, and "a runaway learning ra…" spent one of
                // them on an ellipsis instead of on the two words it had cut.
                let mut lines = if cells(&value) <= room {
                    vec![Line::from(vec![
                        mark,
                        Span::styled(column(label.text(language), label_width), theme.plain()),
                        Span::styled(value, style),
                    ])]
                } else {
                    let mut lines = vec![Line::from(vec![
                        mark,
                        Span::styled(
                            truncate(label.text(language), panel.saturating_sub(2)),
                            theme.plain(),
                        ),
                    ])];
                    for chunk in wrap(&value, panel.saturating_sub(4)) {
                        lines.push(Line::from(Span::styled(format!("    {chunk}"), style)));
                    }
                    lines
                };
                // The values worth trying belong under the knob they are about. Drawn at the foot
                // of the list they read as advice about the last knob, whichever one was chosen.
                if picked {
                    lines.extend(self.preset_lines(panel, language, theme));
                }
                lines
            })
            .collect()
    }

    /// A knob's value as words where it has words, and as a number where it does not.
    fn knob_value_text(&self, index: usize, knob: &Knob, language: Language) -> String {
        match self.stage {
            STAGE_TRAIN => {
                let attention = self.attention();
                let heads = attention.map_or(self.tuned().heads, |a| a.heads).max(1);
                let chosen = choice_of(knob);
                format!(
                    "{} {} · {} {}",
                    Msg::LabelLayer.text(language),
                    chosen / heads + 1,
                    Msg::LabelHead.text(language),
                    chosen % heads + 1
                )
            }
            STAGE_BREAK if index == BREAK_ATTACK => {
                attack_name(choice_of(knob)).text(language).to_string()
            }
            _ => match &knob.value {
                KnobValue::Decimal { current, .. } => decimal_text(*current),
                _ => knob.display(),
            },
        }
    }

    /// The presets of the chosen knob, so a reader can see what is worth typing.
    ///
    /// A list too long for one row folds onto the next rather than losing its last values: "4,0"
    /// is a number the reader cannot type, and a value dropped off the end is one the reader never
    /// learns about. Every row also stops [`PRESET_GAP`] cells short of the panel edge, because a
    /// row that ends in the last column cannot say whether a fourth value was cut off it.
    fn preset_lines(&self, panel: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let values: Vec<String> = match self.stage_knobs().get(self.chosen).map(|k| &k.value) {
            Some(KnobValue::Count { presets, .. }) => {
                presets.iter().map(|p| format::count(*p)).collect()
            }
            Some(KnobValue::Decimal { presets, .. }) => {
                presets.iter().map(|p| decimal_text(*p)).collect()
            }
            _ => Vec::new(),
        };
        if values.is_empty() {
            return Vec::new();
        }
        let lead = format!("  {}  ", Msg::LabelPresets.text(language));
        let indent = cells(&lead);
        let room = panel.saturating_sub(indent + PRESET_GAP);
        let mut rows: Vec<String> = Vec::new();
        let mut row = String::new();
        for value in values {
            if cells(&value) > room {
                // Narrower than a single value: a half-written number is worse than none.
                break;
            }
            let wanted = cells(&value) + if row.is_empty() { 0 } else { PRESET_GAP };
            if cells(&row) + wanted > room {
                rows.push(std::mem::take(&mut row));
            }
            if !row.is_empty() {
                row.push_str(&" ".repeat(PRESET_GAP));
            }
            row.push_str(&value);
        }
        if !row.is_empty() {
            rows.push(row);
        }
        rows.into_iter()
            .enumerate()
            .map(|(index, row)| {
                let lead = if index == 0 { lead.clone() } else { " ".repeat(indent) };
                Line::from(vec![
                    Span::styled(lead, theme.muted()),
                    Span::styled(row, theme.plain()),
                ])
            })
            .collect()
    }

    // ---- numbers -------------------------------------------------------------

    fn attention(&self) -> Option<&AttentionSnapshot> {
        self.latest
            .as_ref()
            .map(|snapshot| &snapshot.attention)
            .filter(|attention| !attention.is_empty())
    }

    /// Step, loss, speed, weights and time — the five numbers the run is.
    fn stat_rows(&self, language: Language) -> Vec<(&'static str, String)> {
        let Some(snapshot) = &self.latest else {
            return Vec::new();
        };
        vec![
            (
                Msg::LabelStep.text(language),
                format!(
                    "{} / {}",
                    format::count(snapshot.step as u64),
                    format::count(snapshot.total_steps as u64)
                ),
            ),
            (Msg::LabelLoss.text(language), loss_text(snapshot.loss, language)),
            (
                Msg::LabelSpeed.text(language),
                format!(
                    "{} {}",
                    format::count(snapshot.tokens_per_second.max(0.0) as u64),
                    Msg::UnitCharsPerSecond.text(language)
                ),
            ),
            (Msg::LabelWeights.text(language), format::count(snapshot.parameter_count as u64)),
            (Msg::LabelElapsed.text(language), format::duration(snapshot.elapsed)),
        ]
    }

    /// The model's own answer, with the mark that says whether it is the sentence yet.
    fn answer_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let Some(snapshot) = &self.latest else {
            return wrapped(
                "",
                Msg::StatusPaused.text(language),
                width,
                theme.muted(),
                theme.muted(),
            );
        };
        if snapshot.completion.is_empty() {
            let mut lines =
                wrapped("", Msg::LabelAnswer.text(language), width, theme.muted(), theme.muted());
            // A model whose weights have blown up has no answer to give, and "the first answer is
            // about a second away" over it was a promise for the rest of the run.
            let blown = snapshot.step > 0 && !snapshot.loss.is_finite();
            lines.extend(wrapped(
                "",
                if blown { Msg::AnswerNone } else { Msg::AnswerWaiting }.text(language),
                width,
                theme.muted(),
                theme.muted(),
            ));
            return lines;
        }
        let right = snapshot.completion.trim_start().starts_with(EXPANSION);
        let blank = snapshot.completion.trim().is_empty();
        let state = if right {
            State::Good
        } else if snapshot.state == TrainingState::Finished {
            State::Bad
        } else {
            State::Working
        };
        let said = format!("{PROMPT}{}", one_line(&snapshot.completion));
        let verdict = if right {
            Msg::AnswerRight
        } else if blank {
            Msg::AnswerBlank
        } else {
            Msg::AnswerNotYet
        };
        let name = Msg::LabelAnswer.text(language);
        let mut lines = if 2 + cells(name) + 2 + cells(verdict.text(language)) <= width {
            vec![Line::from(vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(name, theme.muted()),
                Span::styled(format!("  {}", verdict.text(language)), theme.state(state)),
            ])]
        } else {
            let mut split = vec![Line::from(vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(truncate(name, width.saturating_sub(2)), theme.muted()),
            ])];
            split.extend(wrapped(
                "  ",
                verdict.text(language),
                width,
                theme.state(state),
                theme.state(state),
            ));
            split
        };
        if blank {
            return lines;
        }
        for chunk in wrap(&said, width.saturating_sub(2)) {
            lines.push(Line::from(Span::styled(format!("  {chunk}"), theme.heading())));
        }
        lines
    }

    /// The attention of the latest forward pass: characters across the top, one row per position,
    /// a shade per cell. Zero above the diagonal, always, because a position cannot see the future.
    fn attention_lines(
        &self,
        width: usize,
        height: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let Some(attention) = self.attention() else {
            return wrapped(
                "",
                Msg::AttentionWaiting.text(language),
                width,
                theme.muted(),
                theme.muted(),
            );
        };
        let (layer, head) = self.viewed_head(attention);
        let title = Msg::AttentionTitle.text(language);
        let seen = format!(
            "{} {} · {} {}",
            Msg::LabelLayer.text(language),
            layer + 1,
            Msg::LabelHead.text(language),
            head + 1
        );
        // Which head is on screen is what the left and right arrow keys move, so it is the half
        // of this row that cannot be cut: when the two will not share a line, it moves down to
        // the line that counts the grid, rather than taking a row of its own from a grid that at
        // 80×24 has five to spare.
        let together = cells(title) + 2 + cells(&seen) <= width;
        let lines = if together {
            vec![Line::from(vec![
                Span::styled(format!("{title}  "), theme.heading()),
                Span::styled(seen.clone(), theme.muted()),
            ])]
        } else {
            wrapped("", title, width, theme.heading(), theme.heading())
        };
        let seen_below = (!together).then_some(seen.as_str());
        // The grid is square — every position against every position — so the window on to it is
        // square too. The line above it carries one number, and one number can only be true of the
        // characters across the top and of the rows down the side at once when there are as many
        // of one as of the other. Two columns go to the row label; the newest positions are the
        // interesting ones.
        let mut shown = width.saturating_sub(2).min(attention.length);
        let mut legend = self.grid_legend(shown, attention, seen_below, width, language, theme);
        // How tall that line is depends on the number in it, and shrinking the window never
        // lengthens that number, so settling the two against each other takes a pass or two.
        for _ in 0..3 {
            let room = height.saturating_sub(lines.len() + legend.len() + 1);
            if shown <= room {
                break;
            }
            shown = room;
            legend = self.grid_legend(shown, attention, seen_below, width, language, theme);
        }
        if shown < MIN_GRID.min(attention.length) {
            // A heading over a box with no row in it promises a grid that never arrives, and a
            // reader who started a run watches the empty box for the whole of it. One or two
            // positions are no better: a single cell is one position looking at itself, which it
            // always does completely.
            return Vec::new();
        }
        let mut lines = lines;
        lines.extend(legend);
        let first = attention.length - shown;
        let header: String =
            (first..attention.length).map(|k| visible(attention.tokens.get(k))).collect();
        lines.push(Line::from(Span::styled(format!("  {header}"), theme.muted())));
        for query in first..attention.length {
            let cells: String = (first..attention.length)
                .map(|key| shade(attention.weight(layer, head, query, key).unwrap_or(0.0)))
                .collect();
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", visible(attention.tokens.get(query))), theme.muted()),
                Span::styled(cells, theme.plain()),
            ]));
        }
        lines
    }

    /// The lines between the heading and the grid: which head is showing when the heading had no
    /// room for it, how much of the grid is on screen when not all of it is, and how a space and
    /// a line break are drawn when one of them is in the part on screen.
    ///
    /// The last of those was always drawn, and at 80×24 it wrapped onto a second row and left the
    /// grid three positions square — over a window of "ore", which has neither mark in it.
    fn grid_legend(
        &self,
        shown: usize,
        attention: &AttentionSnapshot,
        seen: Option<&str>,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let mut parts: Vec<String> = seen.map(str::to_string).into_iter().collect();
        if shown < attention.length {
            parts.push(format!(
                "{} {shown} / {}",
                Msg::AttentionNewest.text(language),
                attention.length
            ));
        }
        let first = attention.length.saturating_sub(shown);
        let marked = attention.tokens[first.min(attention.tokens.len())..]
            .iter()
            .any(|c| *c == ' ' || c.is_control());
        if marked {
            parts.push(Msg::AttentionMarks.text(language).to_string());
        }
        if parts.is_empty() {
            return Vec::new();
        }
        wrapped("", &parts.join("   "), width, theme.muted(), theme.muted())
    }

    /// One line saying what a finished run ended up at.
    fn result_line(
        &self,
        finished: &Finished,
        label: Msg,
        state: State,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let numbers = format!(
            "{} {}   {} {}",
            Msg::LabelRate.text(language),
            decimal_text(f64::from(finished.rate)),
            Msg::LabelLoss.text(language),
            loss_text(finished.snapshot.loss, language)
        );
        let name = cells(label.text(language)).max(14);
        let mut lines = if 2 + name + cells(&numbers) <= width {
            vec![Line::from(vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(column(label.text(language), name), theme.heading()),
                Span::styled(numbers, theme.muted()),
            ])]
        } else {
            let mut split = vec![Line::from(vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(
                    truncate(label.text(language), width.saturating_sub(2)),
                    theme.heading(),
                ),
            ])];
            split.extend(wrapped("    ", &numbers, width, theme.muted(), theme.muted()));
            split
        };
        // The model's own answer, which is the point of the comparison: it wraps rather than
        // being cut, because a cut with no ellipsis reads as the model having stopped mid-word.
        for chunk in
            wrap(&answer_line(&finished.snapshot.completion, language), width.saturating_sub(2))
        {
            lines.push(Line::from(Span::styled(format!("  {chunk}"), theme.plain())));
        }
        lines
    }

    // ---- drawing -------------------------------------------------------------

    fn render_brief(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let config = self.machine_default;
        let weights =
            (Msg::LabelWeights.text(language), format::count(config.parameter_count() as u64));
        // The first stage says what a weight is and nothing else, so it shows the one number it
        // has explained. The five beside it — characters, layers, heads, width, window — are the
        // next stage's words, and a reader who has never heard of a transformer met all five on
        // the opening screen with no word about any of them.
        if self.stage != STAGE_PIECES {
            let [heading, table] =
                Layout::vertical([Constraint::Length(2), Constraint::Min(0)]).areas(area);
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    Msg::BriefShapeTitle.text(language),
                    theme.heading(),
                ))),
                heading,
            );
            paragraph(frame, table, stat_lines(&[weights], area.width as usize, theme));
            return;
        }
        let rows = vec![
            (Msg::LabelVocabulary.text(language), format::count(self.vocabulary as u64)),
            (Msg::LabelLayers.text(language), format::count(config.layers as u64)),
            (Msg::LabelHeads.text(language), format::count(config.heads as u64)),
            (Msg::LabelWidth.text(language), format::count(config.d_model as u64)),
            (Msg::LabelContext.text(language), format::count(config.context as u64)),
            weights,
        ];
        let [heading, table, footer] =
            Layout::vertical([Constraint::Length(2), Constraint::Length(7), Constraint::Min(0)])
                .areas(area);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                Msg::BriefShapeTitle.text(language),
                theme.heading(),
            ))),
            heading,
        );
        paragraph(frame, table, stat_lines(&rows, area.width as usize, theme));
        frame.render_widget(Paragraph::new(Vec::<Line>::new()), footer);
    }

    fn render_run(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        if self.latest.is_none() {
            let mut lines: Vec<Line> = Vec::new();
            if let Some(error) = self.refused {
                lines.extend(self.refusal_lines(error, area.width as usize, language, theme));
            }
            paragraph(frame, area, lines);
            return;
        }
        let [stats, curve, answer, attention] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .areas(area);
        paragraph(frame, stats, stat_lines(&self.stat_rows(language), stats.width as usize, theme));
        self.render_curve(frame, curve, theme, language);
        paragraph(frame, answer, self.answer_lines(answer.width as usize, language, theme));
        paragraph(
            frame,
            attention,
            self.attention_lines(
                attention.width as usize,
                attention.height as usize,
                language,
                theme,
            ),
        );
    }

    /// The loss over the whole run, thinned to one point per column so the fall from the first
    /// step is still on screen at the last.
    fn render_curve(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        if area.height == 0 {
            return;
        }
        // The curve names itself now, so this row is only the run's state.
        let [label, line] =
            Layout::vertical([Constraint::Length(1), Constraint::Min(0)]).areas(area);
        let status = match self.run_state() {
            RunState::Running => Some((Msg::StatusRunning, State::Working)),
            RunState::Paused => Some((Msg::StatusPaused, State::Chosen)),
            RunState::Done => Some((Msg::StatusFinished, State::Good)),
            RunState::Idle => None,
        };
        if let Some((message, state)) = status {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    format!("{} {}", state.mark(), message.text(language)),
                    theme.state(state),
                ))),
                label,
            );
        }
        let history = self.latest.as_ref().map(|s| s.loss_history.as_slice()).unwrap_or(&[]);
        let points = thin(history, line.width as usize);
        widgets::curve(frame, line, theme, &points, Msg::LabelCurve.text(language));
    }

    fn render_tune(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let labels = [
            Msg::KnobLayers,
            Msg::KnobHeads,
            Msg::KnobWidth,
            Msg::KnobLearningRate,
            Msg::KnobSteps,
        ];
        let width = area.width as usize;
        let knobs = self.knob_lines(&labels, width, language, theme);
        let machine = self.machine_default;
        // Every number here carries its name. "28,828 (2 · 2 · 32)" left the reader to guess
        // which of the three was the width. The name and its number are held together by a
        // space that does not break, so a wrapped line never leaves "Heads" at the end of one
        // row and its 2 at the start of the next.
        let shape = [
            (Msg::LabelLayers, machine.layers),
            (Msg::LabelHeads, machine.heads),
            (Msg::LabelWidth, machine.d_model),
        ]
        .iter()
        .map(|(label, value)| {
            format!("{}\u{a0}{}", label.text(language), format::count(*value as u64))
        })
        .collect::<Vec<_>>()
        .join(" · ");
        let mut chose = stat_lines(
            &[(
                Msg::LabelMachineChose.text(language),
                format::count(machine.parameter_count() as u64),
            )],
            width,
            theme,
        );
        chose.extend(wrapped("  ", &shape, width, theme.muted(), theme.muted()));
        let weights = match self.weights_now() {
            Ok(weights) => stat_lines(
                &[(Msg::LabelWeightsNow.text(language), format::count(weights as u64))],
                width,
                theme,
            ),
            Err(message) => {
                let mut lines =
                    wrapped("", Msg::ErrorTitle.text(language), width, theme.bad(), theme.bad());
                lines.extend(wrapped(
                    "",
                    message.text(language),
                    width,
                    theme.plain(),
                    theme.plain(),
                ));
                lines
            }
        };

        let mut full = knobs.clone();
        full.push(Line::from(""));
        full.extend(weights.clone());
        full.extend(chose);
        full.push(Line::from(""));
        if let Some(error) = self.refused {
            full.extend(self.refusal_lines(error, width, language, theme));
        }
        // While a run is on screen the settings give up their blank rows and the machine's own
        // choice, which is what lets the run be drawn at all at 80×24: the full list took twelve
        // of the twenty rows there are, the run needs twelve, and the reader who pressed Enter
        // watched the list of values and nothing else for the whole of the run.
        let mut compact = knobs;
        compact.extend(weights);
        self.draw_with_the_run(frame, area, full, compact, theme, language);
    }

    /// The settings, and under them whatever room is left given to the run itself.
    ///
    /// Without this the tuning stage showed a list of values and nothing else while a
    /// twenty-six-second training ran, so the reader had started something with nothing to watch.
    fn draw_with_the_run(
        &self,
        frame: &mut Frame,
        area: Rect,
        full: Vec<Line<'static>>,
        compact: Vec<Line<'static>>,
        theme: Theme,
        language: Language,
    ) {
        /// Rows the run needs before it is worth drawing rather than crowding the settings out.
        const ROOM_FOR_THE_RUN: u16 = 12;
        if self.latest.is_none() {
            paragraph(frame, area, full);
            return;
        }
        let lines = if area.height >= full.len() as u16 + ROOM_FOR_THE_RUN {
            full
        } else if area.height >= compact.len() as u16 + ROOM_FOR_THE_RUN {
            compact
        } else {
            paragraph(frame, area, full);
            return;
        };
        let used = lines.len() as u16;
        let [settings, run] =
            Layout::vertical([Constraint::Length(used), Constraint::Min(0)]).areas(area);
        paragraph(frame, settings, lines);
        self.render_run(frame, run, theme, language);
    }

    fn render_break(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        let labels = [Msg::LabelAttack, Msg::LabelMultiplier];
        let mut lines = self.knob_lines(&labels, width, language, theme);
        lines.push(Line::from(""));

        let sensible = self.tuned();
        let attacked = self.attacked();
        let rates = cells(Msg::LabelSensibleRate.text(language))
            .max(cells(Msg::LabelThisRun.text(language)))
            .max(14)
            .min(width.saturating_sub(8))
            + 2;
        lines.push(Line::from(vec![
            Span::styled(column(Msg::LabelSensibleRate.text(language), rates), theme.muted()),
            Span::styled(decimal_text(f64::from(sensible.learning_rate)), theme.plain()),
        ]));
        let broken = attacked != sensible;
        lines.push(Line::from(vec![
            Span::styled(column(Msg::LabelThisRun.text(language), rates), theme.muted()),
            Span::styled(
                decimal_text(f64::from(attacked.learning_rate)),
                if broken { theme.bad() } else { theme.good() },
            ),
        ]));
        lines.push(Line::from(""));

        if let Some(snapshot) = &self.latest {
            lines.push(Line::from(vec![
                Span::styled(column(Msg::LabelStep.text(language), rates), theme.muted()),
                Span::styled(
                    format!(
                        "{} / {}",
                        format::count(snapshot.step as u64),
                        format::count(snapshot.total_steps as u64)
                    ),
                    theme.plain(),
                ),
            ]));
            let climbing = self.is_climbing();
            lines.push(Line::from(vec![
                Span::styled(column(Msg::LabelLoss.text(language), rates), theme.muted()),
                Span::styled(
                    loss_text(snapshot.loss, language),
                    if climbing { theme.bad() } else { theme.plain() },
                ),
            ]));
            if !snapshot.loss.is_finite() && snapshot.step > 0 {
                lines.extend(wrapped(
                    "",
                    Msg::BreakBlewUp.text(language),
                    width,
                    theme.bad(),
                    theme.bad(),
                ));
            } else if climbing {
                lines.extend(wrapped(
                    "",
                    Msg::BreakClimbing.text(language),
                    width,
                    theme.bad(),
                    theme.bad(),
                ));
            }
            lines.push(Line::from(""));
            // Once both runs have finished there are two answers to compare, and the comparison
            // says more than either answer on its own.
            match (&self.honest, &self.broken) {
                (Some(honest), Some(wrecked)) => {
                    lines.extend(self.result_line(
                        honest,
                        Msg::LabelHonestRun,
                        State::Good,
                        width,
                        language,
                        theme,
                    ));
                    lines.extend(self.result_line(
                        wrecked,
                        Msg::LabelBrokenRun,
                        State::Bad,
                        width,
                        language,
                        theme,
                    ));
                }
                _ => lines.extend(self.answer_lines(width, language, theme)),
            }
        }
        if let Some(error) = self.refused {
            lines.extend(self.refusal_lines(error, width, language, theme));
        }
        paragraph(frame, area, lines);
    }

    fn render_recap(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        // The recap is the whole quest, read from the record every run wrote into. It used to be
        // the last honest run alone, so a reader who trained for twelve hundred steps and then
        // tried a hundred-step shape was shown a hundred steps, and a run left half way by
        // pressing Tab was not there at all.
        let record = &self.record;
        let Some(best) = record.best.as_ref().filter(|_| record.runs > 0) else {
            paragraph(
                frame,
                area,
                wrapped(
                    "",
                    Msg::RecapNothing.text(language),
                    area.width as usize,
                    theme.muted(),
                    theme.muted(),
                ),
            );
            return;
        };
        let snapshot = best;
        let loss = match (record.first_loss, record.lowest) {
            (Some(start), Some(low)) => {
                format!("{} → {}", loss_text(start, language), loss_text(low, language))
            }
            (Some(start), None) => loss_text(start, language),
            (None, _) => loss_text(f32::NAN, language),
        };
        let rows: Vec<(&'static str, String)> = vec![
            (Msg::RecapRuns.text(language), format::count(record.runs as u64)),
            (Msg::RecapSteps.text(language), format::count(record.steps as u64)),
            (Msg::RecapLoss.text(language), loss),
            (Msg::RecapTime.text(language), format::duration(record.time)),
            (Msg::RecapSpeed.text(language), format::count(record.fastest.max(0.0) as u64)),
        ];
        // Wide enough for the longest label in either language, and the same in every row so the
        // recap reads as one column of numbers.
        let width = area.width as usize;
        let label_width = rows
            .iter()
            .map(|(name, _)| cells(name))
            .chain([cells(Msg::RecapAnswer.text(language))])
            .max()
            .unwrap_or(0)
            .min(width.saturating_sub(6))
            + 2;
        let room = width.saturating_sub(label_width + 2);
        let indent = area.width.saturating_sub(4) as usize;
        // Two cells for the number, then the label column. A value the rest of the row will not
        // hold goes under its label rather than losing its last digits to the panel edge.
        let mut lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .flat_map(|(index, (name, value))| {
                let number = Span::styled(format!("{} ", index + 1), theme.muted());
                if cells(value) <= room {
                    return vec![Line::from(vec![
                        number,
                        Span::styled(column(name, label_width), theme.plain()),
                        Span::styled(value.clone(), theme.heading()),
                    ])];
                }
                let mut lines = vec![Line::from(vec![
                    number,
                    Span::styled(truncate(name, width.saturating_sub(2)), theme.plain()),
                ])];
                for chunk in wrap(value, indent) {
                    lines.push(Line::from(Span::styled(format!("    {chunk}"), theme.heading())));
                }
                lines
            })
            .collect();
        let right = answers(&snapshot.completion);
        let state = if right { State::Good } else { State::Bad };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", rows.len() + 1), theme.muted()),
            Span::styled(column(Msg::RecapAnswer.text(language), label_width), theme.plain()),
            Span::styled(state.mark().to_string(), theme.state(state)),
        ]));
        for chunk in wrap(&answer_line(&snapshot.completion, language), indent) {
            lines.push(Line::from(Span::styled(format!("    {chunk}"), theme.heading())));
        }
        if let (true, Some(broken)) = (record.best_is_honest, &self.broken) {
            lines.push(Line::from(""));
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", rows.len() + 2), theme.muted()),
                Span::styled(format!("{} ", State::Bad.mark()), theme.bad()),
                Span::styled(
                    truncate(Msg::RecapBroken.text(language), width.saturating_sub(4)),
                    theme.plain(),
                ),
            ]));
            lines.extend(wrapped(
                "    ",
                &format!(
                    "{} {}   {} {}",
                    Msg::LabelRate.text(language),
                    decimal_text(f64::from(broken.rate)),
                    Msg::LabelLoss.text(language),
                    loss_text(broken.snapshot.loss, language)
                ),
                width,
                theme.bad(),
                theme.bad(),
            ));
            for chunk in wrap(&answer_line(&broken.snapshot.completion, language), indent) {
                lines.push(Line::from(Span::styled(format!("    {chunk}"), theme.plain())));
            }
        }
        paragraph(frame, area, lines);
    }

    /// A sentence of guidance, wrapped to the panel.
    fn refusal_lines(
        &self,
        error: StartError,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        vec![
            Line::from(""),
            Line::from(vec![
                Span::styled(format!("{} ", State::Bad.mark()), theme.bad()),
                Span::styled(Msg::ErrorTitle.text(language), theme.bad()),
            ]),
        ]
        .into_iter()
        .chain(
            wrap(phrases::start_error(error).text(language), width)
                .into_iter()
                .map(|chunk| Line::from(Span::styled(chunk, theme.plain()))),
        )
        .collect()
    }

    /// True when the loss is higher now than it was at its lowest — which is what a runaway
    /// learning rate looks like from the outside.
    fn is_climbing(&self) -> bool {
        let Some(snapshot) = &self.latest else { return false };
        if !snapshot.loss.is_finite() {
            return snapshot.step > 0;
        }
        let lowest = snapshot
            .loss_history
            .iter()
            .copied()
            .filter(|v| v.is_finite())
            .fold(f32::INFINITY, f32::min);
        lowest.is_finite() && snapshot.loss > lowest * 1.2
    }
}

impl Session {
    fn script(&self) -> &'static [Step] {
        SCRIPTS[self.stage.min(SCRIPTS.len() - 1)]
    }

    /// Back to the first sentence of this stage, with nothing running and nothing said.
    fn restart(&mut self) {
        self.stop();
        self.revealed = 0;
        self.log.clear();
        self.told.clear();
        self.forget_run();
        self.refused = None;
        self.remember_tell();
    }

    /// The knobs a run is started from. The layer-and-head chooser on the training stage is a
    /// knob too, but it chooses what to look at, not what to train.
    ///
    /// Counting it made looking at the second head and pressing Enter throw away a minute of
    /// training and start again from random numbers. Worse, a run resets the chooser to the first
    /// head as it starts, so a reader who had been looking at another head found the very next
    /// Enter restarting the run they had only just started.
    fn run_knobs(&self) -> &[Knob] {
        match self.stage {
            STAGE_TUNE => &self.tune,
            STAGE_BREAK => &self.attack,
            _ => &[],
        }
    }

    /// Whether the reader has turned a knob since the run in progress started.
    fn knobs_moved(&self) -> bool {
        match &self.running_with {
            Some(settled) => !settled.still(self.run_knobs()),
            None => false,
        }
    }

    /// Whether the next step of this stage's conversation starts a run.
    fn next_is_a_run(&self) -> bool {
        matches!(self.script().get(self.revealed + 1), Some(Run(_)))
    }

    /// Runs this stage's work again with the values now on screen, in place of the run before it,
    /// so the sentence that reads the result is about the run that just happened.
    ///
    /// Only the knobs a run is started from count. The training stage's layer-and-head chooser
    /// counted too, so Enter at the end of that stage rewound its conversation to the run and
    /// started again rather than letting the shell walk on: after a refused run the reader
    /// pressed Enter round the same five sentences for ever and never reached the next stage.
    fn rerun(&mut self) -> Reaction {
        if self.run_knobs().is_empty() {
            return Reaction::Ignored;
        }
        let script = self.script();
        let upto = self.revealed.min(script.len().saturating_sub(1));
        let Some(at) = script[..=upto].iter().rposition(|step| matches!(step, Run(_))) else {
            return Reaction::Ignored;
        };
        if at == 0 {
            return Reaction::Ignored;
        }
        self.stop();
        self.forget_run();
        self.log.retain(|logged| logged.step < at);
        self.told.retain(|(step, _)| *step < at);
        self.revealed = at - 1;
        self.advance();
        Reaction::Handled
    }

    /// Forgets what has been said about the run in progress, without touching finished ones.
    fn forget_run(&mut self) {
        self.milestone = 0;
        self.said_got_it = false;
        self.said_ended = false;
        self.first_loss = None;
    }

    fn satisfied(&self, until: Until) -> bool {
        // A refused run never starts, so a refusal ends every wait, or the conversation stops for
        // good behind a message nobody can press past.
        if self.refused.is_some() {
            return true;
        }
        let finished = self.latest.as_ref().is_some_and(|s| s.state == TrainingState::Finished);
        match until {
            // A run the reader made shorter than the step being waited for finishes first, and
            // the wait for step 300 of a 100-step run used to last for ever.
            Until::Steps(wanted) => {
                finished || self.latest.as_ref().is_some_and(|s| s.step >= wanted)
            }
            Until::Finished => finished,
        }
    }

    /// The numbers a `Tell` said just now would read.
    fn facts(&self) -> Facts {
        Facts {
            refused: self.refused.is_some(),
            ran: self.ran,
            latest: self.latest.as_ref().map(light),
            honest: self.honest.clone(),
            previous: self.previous.clone(),
            record: self.record.clone(),
            machine: self.machine_default,
        }
    }

    /// Keeps the numbers of the step just revealed, when it is a sentence that reads them.
    fn remember_tell(&mut self) {
        let index = self.revealed;
        if matches!(self.script().get(index), Some(Tell(_) | Suggest(_)))
            && !self.told.iter().any(|(step, _)| *step == index)
        {
            let facts = self.facts();
            self.told.push((index, facts));
        }
    }

    /// A sentence built from numbers, as it reads right now. The conversation uses the numbers
    /// that were kept when it was first said; this is for the moment of saying it.
    #[cfg(test)]
    fn tell(&self, topic: Topic, language: Language) -> String {
        self.facts().tell(topic, language).unwrap_or_default()
    }

    /// Reveals the next step and starts whatever it asks for.
    fn advance(&mut self) -> bool {
        let script = self.script();
        if self.revealed + 1 >= script.len() {
            return false;
        }
        self.revealed += 1;
        if let Run(deed) = script[self.revealed] {
            let settled = Settled::of(self.run_knobs());
            self.running_with = Some(settled);
            self.forget_run();
            let config = match deed {
                Train => {
                    self.ran = None;
                    self.tuned()
                }
                Attack => {
                    // Kept so the sentence after the run can be about the run, not about the
                    // values the conversation had hoped for.
                    self.ran = Some((
                        self.attack.get(BREAK_ATTACK).map_or(ATTACK_RUNAWAY, choice_of),
                        self.attack.get(BREAK_MULTIPLIER).map_or(1.0, decimal_of),
                    ));
                    self.attacked()
                }
            };
            // Always a fresh run. The old version answered Enter during training by doing nothing
            // at all, so a reader who changed a value and pressed Enter watched the old model
            // finish and concluded the key was broken.
            self.start(config);
            match self.refused {
                Some(error) => {
                    self.ran = None;
                    let message = phrases::start_error(error);
                    self.say(Happening::Refused(message));
                }
                None => {
                    let steps = config.steps;
                    let weights = config.parameter_count();
                    self.say(Happening::Started { weights, steps });
                }
            }
            // A run and the wait for it are one move.
            if matches!(script.get(self.revealed + 1), Some(Await(_))) {
                self.revealed += 1;
            }
        }
        self.remember_tell();
        true
    }

    fn say(&mut self, what: Happening) {
        // No cap. There used to be one of twenty-four per stage, and the Break stage runs four
        // attacks of up to eight happenings each, so the ending of the last attack — the one the
        // stage exists for — was silently never said. A run says at most eight things (its start,
        // five milestones, the sentence, its end), so nothing here grows without the reader
        // pressing a key.
        self.log.push(Logged { step: self.revealed, what });
    }

    /// Reads the run and turns anything new into beats.
    fn notice(&mut self) {
        let Some(snapshot) = &self.latest else { return };
        let step = snapshot.step;
        let loss = snapshot.loss;
        let finished = snapshot.state == TrainingState::Finished;
        let right = answers(&snapshot.completion);
        let seconds = snapshot.elapsed;
        let low = lowest(snapshot);

        if self.first_loss.is_none() && step > 0 {
            self.first_loss = first_finite(&snapshot.loss_history).or(finite(loss));
        }
        while self.milestone < MILESTONES.len()
            && loss.is_finite()
            && loss < MILESTONES[self.milestone]
        {
            self.milestone += 1;
            self.say(Happening::Loss { loss, step });
        }
        if right && !self.said_got_it {
            self.said_got_it = true;
            self.say(Happening::GotIt { step });
        }
        if finished && !self.said_ended {
            self.said_ended = true;
            // "Never got below where it started" is a statement about the lowest point of the
            // run, not about its last one: a loss that fell to 1.2 and climbed back to 3.5 got
            // well below where it started, and used to be told it never had.
            let fell = match (self.first_loss, low) {
                (Some(first), Some(low)) => low < first,
                _ => false,
            };
            self.say(Happening::Ended { loss, seconds: seconds.as_secs_f64(), fell });
        }
    }

    /// One happening, said in the reader's language.
    fn beat_for(&self, what: &Happening, language: Language) -> Beat {
        match what {
            Happening::Started { weights, steps } => Beat::event(format!(
                "{}  ·  {} {}  ·  {} {}",
                Msg::EventStarted.text(language),
                Msg::LabelWeights.text(language),
                format::count(*weights as u64),
                Msg::KnobSteps.text(language),
                format::count(*steps as u64),
            )),
            Happening::Loss { loss, step } => Beat::event(format!(
                "{} {}  ·  {} {}",
                Msg::EventLoss.text(language),
                loss_text(*loss, language),
                Msg::EventStep.text(language),
                format::count(*step as u64),
            )),
            Happening::GotIt { step } => Beat::outcome(
                State::Good,
                format!(
                    "{}  ·  {} {}",
                    Msg::EventGotIt.text(language),
                    Msg::EventStep.text(language),
                    format::count(*step as u64),
                ),
            ),
            Happening::Ended { loss, seconds, fell } => {
                let mut text = format!(
                    "{}  ·  {} {}",
                    Msg::EventFinished.text(language),
                    Msg::EventLoss.text(language),
                    loss_text(*loss, language),
                );
                text.push_str(&format!(
                    "  ·  {} {}",
                    Msg::EventSeconds.text(language),
                    format::duration(std::time::Duration::from_secs_f64(seconds.max(0.0))),
                ));
                if !*fell {
                    text.push_str(&format!("  ·  {}", Msg::EventNeverFell.text(language)));
                }
                Beat::outcome(if *fell { State::Good } else { State::Bad }, text)
            }
            Happening::Refused(message) => Beat::outcome(State::Bad, message.text(language)),
            Happening::PulledIn { to } => Beat::outcome(
                State::Chosen,
                format!(
                    "{}  ·  {} {}",
                    Msg::EventOutsideRange.text(language),
                    Msg::EventSetTo.text(language),
                    landed_text(to, language),
                ),
            ),
            Happening::NotANumber => Beat::outcome(State::Bad, Msg::EventNotANumber.text(language)),
        }
    }

    /// Accepts a number typed into a choice, counting from one the way the panel does.
    ///
    /// The knob's own reading counts from nought, so typing 2 into the layer-and-head chooser
    /// showed the third head, and typing 3 into the attack — there are three — was "outside what
    /// this value allows".
    fn commit_choice(&mut self) -> Typed {
        let Some(knob) = self.stage_knobs_mut() else { return Typed::Nothing };
        let Some(draft) = knob.draft().map(str::to_string) else { return Typed::Nothing };
        knob.cancel();
        let KnobValue::Choice { current, count } = &mut knob.value else {
            return Typed::Nothing;
        };
        let Some(asked) = draft.trim().parse::<f64>().ok().filter(|v| v.is_finite()) else {
            return Typed::NotANumber;
        };
        if *count == 0 {
            return Typed::NotANumber;
        }
        let asked = asked.round();
        let landed = asked.clamp(1.0, *count as f64);
        *current = landed as usize - 1;
        if landed == asked {
            Typed::Taken
        } else {
            Typed::PulledIn { to: format::count(landed as u64) }
        }
    }

    /// Where the chosen knob now stands, in the words the panel uses for it.
    fn landed(&self) -> Landed {
        let Some(knob) = self.stage_knobs().get(self.chosen) else {
            return Landed::Number(String::new());
        };
        match (self.stage, &knob.value) {
            (STAGE_TRAIN, _) => {
                let heads = self.attention().map_or(self.tuned().heads, |a| a.heads).max(1);
                let chosen = choice_of(knob);
                Landed::View { layer: chosen / heads + 1, head: chosen % heads + 1 }
            }
            (STAGE_BREAK, KnobValue::Choice { current, .. }) => Landed::Attack(*current),
            (_, KnobValue::Decimal { current, .. }) => Landed::Number(decimal_text(*current)),
            _ => Landed::Number(knob.value.display()),
        }
    }
}

impl Facts {
    /// A sentence about what the runs did, built from their own numbers. `None` when there is
    /// nothing true to say — the recap of a quest in which nothing was ever trained does not
    /// count the runs it never had.
    fn tell(&self, topic: Topic, language: Language) -> Option<String> {
        Some(match topic {
            // A refused run never started, and the reason is the beat above this one. What
            // follows it says that nothing ran rather than reading an older run as this one.
            Topic::Progress if self.refused => Msg::TrainNoRun.text(language).to_string(),
            Topic::Settings if self.refused => Msg::SettingsNothingRan.text(language).to_string(),
            Topic::Attack { .. } if self.refused => Msg::BreakNothingRan.text(language).to_string(),
            Topic::Progress => self.tell_progress(language),
            Topic::Settings => self.tell_settings(language),
            Topic::Attack { kind, multiplier, lesson, check } => {
                self.tell_attack(kind, multiplier, lesson, check, language)
            }
            Topic::SuggestWidth => self.suggest_width(language),
            Topic::RecapStart => match self.record.first_weights {
                Some(weights) => format!(
                    "{} {} {}.",
                    Msg::RecapOne.text(language),
                    Msg::WordWeights.text(language),
                    format::count(weights as u64)
                ),
                None => Msg::RecapNothing.text(language).to_string(),
            },
            Topic::RecapWork => {
                let record = &self.record;
                if record.runs == 0 {
                    return None;
                }
                format!(
                    "{} {} {}  ·  {} {}  ·  {} {}",
                    Msg::RecapWork.text(language),
                    Msg::WordRuns.text(language),
                    format::count(record.runs as u64),
                    Msg::WordSteps.text(language),
                    format::count(record.steps as u64),
                    Msg::WordTime.text(language),
                    format::duration(record.time),
                )
            }
            Topic::RecapSentence => {
                if self.record.runs == 0 {
                    return None;
                }
                match self.record.sentence_at {
                    Some(step) => {
                        format!("{} {}.", Msg::RecapSix.text(language), format::count(step as u64))
                    }
                    None => Msg::RecapNoSentence.text(language).to_string(),
                }
            }
            Topic::RecapBroken => {
                let record = &self.record;
                if record.runs == 0 {
                    return None;
                }
                if record.broken_runs == 0 {
                    Msg::RecapNeverBroke.text(language).to_string()
                } else {
                    format!(
                        "{} {} {}  ·  {} {}",
                        Msg::RecapBrokeIt.text(language),
                        Msg::WordBrokenRuns.text(language),
                        format::count(record.broken_runs as u64),
                        Msg::WordBlewUp.text(language),
                        format::count(record.blew_up as u64),
                    )
                }
            }
        })
    }

    /// The lesson of one attack, when the run that just ended is that attack and did what the
    /// lesson says; otherwise what the run did instead, read from its numbers.
    fn tell_attack(
        &self,
        kind: usize,
        multiplier: f64,
        lesson: Msg,
        check: Check,
        language: Language,
    ) -> String {
        // The lesson is only true of the run it was written about. A reader who pressed Enter
        // without touching the values ran something else, and used to be told the conclusion
        // anyway.
        let asked =
            self.ran.is_some_and(|(k, m)| k == kind && (m - multiplier).abs() < multiplier * 0.01);
        let run = self.latest.as_ref();
        if let Some(run) = run.filter(|run| asked && holds(check, run)) {
            // A collapse is said with the numbers that show it and the character it fell onto.
            // The fixed sentence used to promise "one letter", and the model most often falls
            // onto the space, which leaves nothing on screen to see.
            if check == Check::Collapsed
                && let (Some(first), Some(character)) =
                    (first_finite(&run.loss_history), repeated(&run.completion))
            {
                let character = match character {
                    ' ' => Msg::CharSpace.text(language).to_string(),
                    other => format!("\u{2018}{other}\u{2019}"),
                };
                return format!(
                    "{}  ·  {} {} → {}  ·  {} {}",
                    lesson.text(language),
                    Msg::BreakRanAt.text(language),
                    loss_text(first, language),
                    loss_text(run.loss, language),
                    Msg::BreakRepeated.text(language),
                    character,
                );
            }
            return lesson.text(language).to_string();
        }
        // The reader ran something else, or the run went another way. Either way the screen has
        // a run on it, and this reads that run rather than leaving it unexplained.
        let lead = if asked { Msg::BreakNotWhatWasSaid } else { Msg::BreakNotThatRun };
        let mut text = lead.text(language).to_string();
        if let Some(run) = run
            && let Some(first) = first_finite(&run.loss_history)
        {
            text.push_str(&format!(
                "  ·  {} {} → {}",
                Msg::BreakRanAt.text(language),
                loss_text(first, language),
                loss_text(run.loss, language),
            ));
            let verdict = if !run.loss.is_finite() {
                Msg::BreakItBlewUp
            } else if lowest(run).is_some_and(|low| low < first) {
                Msg::BreakItLearned
            } else {
                Msg::BreakItDidNot
            };
            text.push(' ');
            text.push_str(verdict.text(language));
        }
        text
    }

    /// Whether the loss is still falling, measured rather than assumed.
    ///
    /// The last quarter of the history is held against the first: a run that has flattened has
    /// given up most of its fall already, and a run that has not is still on its way down. A run
    /// that went up rather than down is neither, and neither is one whose loss is no longer a
    /// number — both used to be told the fall had flattened out, or that it was still falling.
    fn tell_progress(&self, language: Language) -> String {
        let Some(run) = self.latest.as_ref().filter(|run| !run.loss_history.is_empty()) else {
            return Msg::TrainNoRun.text(language).to_string();
        };
        if !run.loss.is_finite() {
            return Msg::TrainBlewUp.text(language).to_string();
        }
        let history: Vec<f32> =
            run.loss_history.iter().copied().filter(|v| v.is_finite()).collect();
        let (Some(&first), Some(&last)) = (history.first(), history.last()) else {
            return Msg::TrainBlewUp.text(language).to_string();
        };
        if history.len() < 8 {
            let message = if last < first { Msg::TrainStillFalling } else { Msg::TrainNotFalling };
            return message.text(language).to_string();
        }
        let quarter = (history.len() / 4).max(1);
        let early = history[..quarter].iter().sum::<f32>() / quarter as f32;
        let late = history[history.len() - quarter..].iter().sum::<f32>() / quarter as f32;
        if late >= early {
            return Msg::TrainNotFalling.text(language).to_string();
        }
        let mid = &history[quarter..history.len() - quarter];
        let middle = if mid.is_empty() { late } else { mid.iter().sum::<f32>() / mid.len() as f32 };
        // Flat when the latest stretch has given up far less than the run did getting here.
        let whole = early - late;
        let recent = (middle - late).abs();
        let message = if recent < whole * 0.2 { Msg::TrainSlowing } else { Msg::TrainStillFalling };
        message.text(language).to_string()
    }

    /// This run held against the one before it: what the reader changed, and what it cost.
    fn tell_settings(&self, language: Language) -> String {
        let (Some(now), Some(before)) = (self.honest.as_ref(), self.previous.as_ref()) else {
            return Msg::SettingsFirstRun.text(language).to_string();
        };
        let (weights, was) = (now.snapshot.parameter_count, before.snapshot.parameter_count);
        let (loss, loss_was) = (now.snapshot.loss, before.snapshot.loss);
        // A loss that is not a number is neither higher nor lower than anything, and every
        // comparison with it is false: a run that blew up used to be "a lower loss".
        let verdict = if !(loss.is_finite() && loss_was.is_finite()) {
            Msg::SettingsBlewUp
        } else if weights == was {
            Msg::SettingsSameShape
        } else if weights < was {
            if loss > loss_was { Msg::SettingsSmallerWorse } else { Msg::SettingsSmallerBetter }
        } else if loss < loss_was {
            Msg::SettingsBiggerBetter
        } else {
            Msg::SettingsBiggerWorse
        };
        format!(
            "{} {} {} → {}, {} {} → {}.",
            verdict.text(language),
            Msg::WordWeights.text(language),
            format::count(was as u64),
            format::count(weights as u64),
            Msg::EventLoss.text(language),
            loss_text(loss_was, language),
            loss_text(loss, language),
        )
    }

    /// Half of this machine's width, kept a whole number of heads wide, and what it does to the
    /// weights. Read from the machine's choice rather than from the knob, so the suggestion does
    /// not halve itself again the moment the reader follows it.
    fn suggest_width(&self, language: Language) -> String {
        let machine = self.machine;
        let heads = machine.heads.max(1);
        let half = (machine.d_model / 2 / heads * heads).max(heads);
        let smaller = TrainingConfig { d_model: half, ..machine };
        format!(
            "{} {} {} → {}  ·  {} {} → {}",
            Msg::SettingsAsk.text(language),
            Msg::WordWidth.text(language),
            format::count(machine.d_model as u64),
            format::count(half as u64),
            Msg::WordWeights.text(language),
            format::count(machine.parameter_count() as u64),
            format::count(smaller.parameter_count() as u64),
        )
    }
}

/// Whether a run did what an attack's lesson says it did.
fn holds(check: Check, run: &TrainingSnapshot) -> bool {
    let first = first_finite(&run.loss_history);
    match check {
        Check::Collapsed => {
            let never_fell = match (first, lowest(run)) {
                (Some(first), Some(low)) => low >= first,
                _ => false,
            };
            never_fell && one_letter(&run.completion)
        }
        Check::Misspelt => {
            let learned = matches!((first, lowest(run)), (Some(first), Some(low)) if low < first);
            learned && !answers(&run.completion) && !run.completion.trim().is_empty()
        }
        Check::Crawled => match (first, finite(run.loss)) {
            (Some(first), Some(last)) => last > first * 0.5,
            _ => false,
        },
        Check::Found => answers(&run.completion),
    }
}

/// The character that is at least three quarters of an answer, if one is: what "one character,
/// over and over" means for a model that says `gggnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn`.
///
/// A space counts like any other character. The commonest character in the corpus is the space,
/// and a model the runaway rate has wrecked falls onto it: its answer is thirty spaces. Leaving
/// spaces out of the count found no characters at all in that answer, and called the most
/// complete collapse there is no collapse.
fn repeated(text: &str) -> Option<char> {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() < 4 {
        return None;
    }
    let (most, count) = chars
        .iter()
        .map(|c| (*c, chars.iter().filter(|d| *d == c).count()))
        .max_by_key(|(_, count)| *count)?;
    (count * 4 >= chars.len() * 3).then_some(most)
}

fn one_letter(text: &str) -> bool {
    repeated(text).is_some()
}

/// Whether a completion is the sentence.
fn answers(completion: &str) -> bool {
    completion.trim_start().starts_with(EXPANSION)
}

fn finite(value: f32) -> Option<f32> {
    value.is_finite().then_some(value)
}

fn first_finite(values: &[f32]) -> Option<f32> {
    values.iter().copied().find(|v| v.is_finite())
}

/// The lowest loss a run reached, from its curve and where it ended.
fn lowest(run: &TrainingSnapshot) -> Option<f32> {
    run.loss_history
        .iter()
        .copied()
        .chain([run.loss])
        .filter(|v| v.is_finite())
        .fold(None, |low: Option<f32>, v| Some(low.map_or(v, |l| l.min(v))))
}

/// A snapshot without its attention weights, which are the one large part of it and which
/// nothing that is kept for later ever draws.
fn light(run: &TrainingSnapshot) -> TrainingSnapshot {
    TrainingSnapshot {
        attention: AttentionSnapshot::default(),
        completion: run.completion.clone(),
        loss_history: run.loss_history.clone(),
        ..*run
    }
}

/// Where a typed number landed, in the reader's language.
fn landed_text(landed: &Landed, language: Language) -> String {
    match landed {
        Landed::Number(text) => text.clone(),
        Landed::View { layer, head } => format!(
            "{} {layer} · {} {head}",
            Msg::LabelLayer.text(language),
            Msg::LabelHead.text(language)
        ),
        Landed::Attack(kind) => attack_name(*kind).text(language).to_string(),
    }
}

fn attack_name(kind: usize) -> Msg {
    match kind {
        ATTACK_NO_WARMUP => Msg::AttackNoWarmup,
        ATTACK_PLAIN_SGD => Msg::AttackPlainSgd,
        _ => Msg::AttackRunaway,
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
        // One heavy run at a time. A stage that leaves a trainer behind steals the cores from
        // whatever the next stage is about to measure.
        self.restart();
    }

    fn transcript(&self, language: Language) -> Vec<Beat> {
        let script = self.script();
        let mut beats = Vec::new();
        for (index, step) in script.iter().enumerate().take(self.revealed + 1) {
            match step {
                Say(message) => beats.push(Beat::say(message.text(language))),
                Ask(message) => beats.push(Beat::ask(message.text(language))),
                Tell(topic) | Suggest(topic) => {
                    let kept = self.told.iter().find(|(step, _)| *step == index);
                    let said = match kept {
                        Some((_, facts)) => facts.tell(*topic, language),
                        None => self.facts().tell(*topic, language),
                    };
                    if let Some(text) = said {
                        beats.push(if matches!(step, Suggest(_)) {
                            Beat::ask(text)
                        } else {
                            Beat::say(text)
                        });
                    }
                }
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
        self.stage_knobs()
    }

    fn chosen_knob(&self) -> Option<usize> {
        (!self.stage_knobs().is_empty()).then_some(self.chosen)
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
            Action::PauseOrResume => match &self.handle {
                Some(handle) => {
                    if handle.is_paused() {
                        handle.resume();
                    } else {
                        handle.pause();
                    }
                    Reaction::Handled
                }
                None => Reaction::Ignored,
            },
            Action::Reset => {
                if self.stage == STAGE_BREAK {
                    self.recover();
                }
                // `r` is the one key that throws a run's numbers away; walking off keeps them.
                self.throw_away();
                self.restart();
                Reaction::Handled
            }
            Action::Next => self.move_choice(1),
            Action::Previous => self.move_choice(-1),
            Action::Nudge(direction) => match self.stage_knobs_mut() {
                Some(knob) => {
                    knob.nudge(direction);
                    Reaction::Handled
                }
                None => Reaction::Ignored,
            },
            Action::Type(c) => match self.stage_knobs_mut() {
                Some(knob) => {
                    knob.type_char(c);
                    Reaction::Handled
                }
                None => Reaction::Ignored,
            },
            Action::Backspace => match self.stage_knobs_mut() {
                Some(knob) => {
                    knob.backspace();
                    Reaction::Handled
                }
                None => Reaction::Ignored,
            },
            Action::Commit => {
                let Some(knob) = self.stage_knobs_mut() else { return Reaction::Ignored };
                let typed = if matches!(knob.value, KnobValue::Choice { .. }) {
                    self.commit_choice()
                } else {
                    knob.commit()
                };
                // A number that goes nowhere reads as a broken key unless the screen says what
                // happened to it, in the words the panel uses for it.
                match typed {
                    Typed::PulledIn { .. } => {
                        let to = self.landed();
                        self.say(Happening::PulledIn { to });
                    }
                    Typed::NotANumber => self.say(Happening::NotANumber),
                    Typed::Taken | Typed::Nothing => {}
                }
                Reaction::Handled
            }
            Action::Cancel => match self.stage_knobs_mut() {
                Some(knob) => {
                    knob.cancel();
                    Reaction::Handled
                }
                None => Reaction::Ignored,
            },
        }
    }

    fn tick(&mut self) {
        if let Some(handle) = &self.handle {
            // One cheap copy out from behind the lock. No work happens here.
            let snapshot = handle.snapshot();
            let done = snapshot.state == TrainingState::Finished;
            if done {
                // The worker has left; joining it costs nothing.
                self.handle = None;
                self.file(&snapshot);
            }
            self.latest = Some(snapshot);
        }
        self.notice();
        if let Some(Await(until)) = self.script().get(self.revealed)
            && self.satisfied(*until)
        {
            self.advance();
        }
    }

    fn run_state(&self) -> RunState {
        match &self.latest {
            None => RunState::Idle,
            Some(snapshot) => match snapshot.state {
                TrainingState::Running => RunState::Running,
                TrainingState::Paused => RunState::Paused,
                TrainingState::Finished => RunState::Done,
            },
        }
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let title = match self.stage {
            STAGE_TRAIN => Msg::RunTitle,
            STAGE_TUNE => Msg::TuneTitle,
            STAGE_BREAK => Msg::BreakTitle,
            STAGE_RECAP => Msg::RecapTitle,
            _ => Msg::Title,
        };
        let block = theme.titled_panel(title.text(language));
        let inner = block.inner(area);
        frame.render_widget(block, area);
        if inner.width == 0 || inner.height == 0 {
            return;
        }
        match self.stage {
            STAGE_TRAIN => self.render_run(frame, inner, theme, language),
            STAGE_TUNE => self.render_tune(frame, inner, theme, language),
            STAGE_BREAK => self.render_break(frame, inner, theme, language),
            STAGE_RECAP => self.render_recap(frame, inner, theme, language),
            _ => self.render_brief(frame, inner, theme, language),
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            STAGE_TRAIN => vec![("←→", Msg::KeyViewHead.text(language))],
            STAGE_TUNE => vec![("↑↓ ←→", Msg::KeyChangeValue.text(language))],
            STAGE_BREAK => vec![
                ("↑↓ ←→", Msg::KeyChangeValue.text(language)),
                ("r", Msg::KeyRecover.text(language)),
            ],
            _ => Vec::new(),
        }
    }

    fn go_name(&self, language: Language) -> Option<&'static str> {
        let runnable = !self.run_knobs().is_empty() && (self.at_end() || self.knobs_moved());
        runnable.then(|| Msg::KeyRunIt.text(language))
    }

    fn typing(&self) -> bool {
        // While this is true the shell sends digits here instead of jumping between stages, which
        // is the only way a reader can type 1500 steps.
        self.stage_knobs().get(self.chosen).is_some_and(|knob| knob.draft().is_some())
    }

    fn close(&mut self) {
        // Dropping the handle tells the worker to stop and waits for it, so nothing is left
        // spinning behind a screen nobody is looking at.
        self.stop();
    }
}

// ---- the knobs themselves ----------------------------------------------------

fn tune_knobs(default: &TrainingConfig) -> Vec<Knob> {
    vec![
        Knob::new(
            "layers",
            KnobValue::Count {
                current: default.layers as u64,
                min: 1,
                max: 8,
                step: 1,
                presets: LAYER_PRESETS,
            },
        ),
        Knob::new(
            "heads",
            KnobValue::Count {
                current: default.heads as u64,
                min: 1,
                max: 16,
                step: 1,
                presets: HEAD_PRESETS,
            },
        ),
        Knob::new(
            "width",
            KnobValue::Count {
                current: default.d_model as u64,
                min: 8,
                max: 256,
                step: 8,
                presets: WIDTH_PRESETS,
            },
        ),
        Knob::new(
            "learning-rate",
            KnobValue::Decimal {
                current: f64::from(default.learning_rate),
                min: 0.000_01,
                max: 1.0,
                step: 0.000_5,
                presets: RATE_PRESETS,
            },
        ),
        Knob::new(
            "steps",
            KnobValue::Count {
                current: default.steps as u64,
                min: 100,
                max: 8000,
                step: 100,
                presets: STEP_PRESETS,
            },
        ),
    ]
}

fn attack_knobs() -> Vec<Knob> {
    vec![
        Knob::new("attack", KnobValue::Choice { current: ATTACK_RUNAWAY, count: ATTACK_COUNT }),
        Knob::new(
            "multiplier",
            KnobValue::Decimal {
                current: BREAKING_MULTIPLIER,
                min: 1.0,
                max: 10_000.0,
                step: 10.0,
                presets: MULTIPLIER_PRESETS,
            },
        ),
    ]
}

// ---- small helpers -----------------------------------------------------------

/// Draws `lines` into `area`, saying so when there were more of them than there are rows.
///
/// Every panel here is a list of lines drawn into a box someone else sized, and a `Paragraph`
/// drops whatever does not fit without a word. What it drops is the end of the panel — the run's
/// own answer, most often — and the row it stops on looks exactly like the end of the text.
fn paragraph(frame: &mut Frame, area: Rect, lines: Vec<Line<'static>>) {
    frame
        .render_widget(Paragraph::new(fit(lines, area.height as usize, area.width as usize)), area);
}

fn count_of(knob: &Knob) -> u64 {
    match knob.value {
        KnobValue::Count { current, .. } => current,
        _ => 0,
    }
}

fn decimal_of(knob: &Knob) -> f64 {
    match knob.value {
        KnobValue::Decimal { current, .. } => current,
        _ => 0.0,
    }
}

fn choice_of(knob: &Knob) -> usize {
    match knob.value {
        KnobValue::Choice { current, .. } => current,
        _ => 0,
    }
}

/// A number with a fractional part, written so a learning rate and a multiplier both read.
fn decimal_text(value: f64) -> String {
    if !value.is_finite() {
        return "-".to_string();
    }
    let magnitude = value.abs();
    if magnitude == 0.0 {
        "0".to_string()
    } else if magnitude < 0.001 {
        format!("{value:.1e}")
    } else if magnitude < 1.0 {
        format!("{value:.4}")
    } else if magnitude < 1000.0 {
        format!("{value:.1}")
    } else {
        format::count(value.round().max(0.0) as u64)
    }
}

/// A loss, or the words for the moment before there is one and the moment after it stops being
/// a number at all.
fn loss_text(value: f32, language: Language) -> String {
    if value.is_finite() && value.abs() >= 1000.0 {
        // A runaway rate reaches losses in the millions, and "1567369.000" is eleven cells of
        // which the last four say nothing.
        format::count(value.abs().round() as u64)
    } else if value.is_finite() {
        format!("{value:.3}")
    } else {
        Msg::NotANumber.text(language).to_string()
    }
}

/// Model output on one line. The corpus has newlines in it and the model learns to produce them,
/// but a line break inside a one-line answer would push the rest of the panel around.
fn one_line(text: &str) -> String {
    text.chars().map(|c| if c.is_control() { ' ' } else { c }).collect()
}

/// The answer as a line after the prompt: the model's own characters, or, when there are only
/// spaces, the words for that. A run of spaces after the prompt is a blank a reader cannot tell
/// from no answer at all, and a collapsed model most often answers with nothing else.
fn answer_line(text: &str, language: Language) -> String {
    if !text.is_empty() && text.trim().is_empty() {
        format!("{PROMPT} {}", Msg::AnswerOnlySpaces.text(language))
    } else {
        format!("{PROMPT}{}", one_line(text))
    }
}

/// One column per character, with the invisible ones made visible.
fn visible(c: Option<&char>) -> char {
    match c {
        Some(' ') => '_',
        Some(c) if c.is_control() => '/',
        // A two-cell glyph would push every column after it out of line with its weights. The
        // corpus is ASCII, so this is a guard rather than a case that happens.
        Some(c) if char_width(*c) != 1 => '?',
        Some(c) => *c,
        None => ' ',
    }
}

/// An attention weight as one of five shades.
fn shade(weight: f32) -> &'static str {
    let level = if !weight.is_finite() || weight < 0.05 {
        0
    } else if weight < 0.15 {
        1
    } else if weight < 0.35 {
        2
    } else if weight < 0.60 {
        3
    } else {
        4
    };
    SHADES[level]
}

/// Breaks text into chunks of at most `width` cells, on a space where there is one near the end
/// and mid-word where there is not.
///
/// Model output carries no promise of spaces in sensible places — early in a run it is a wall of
/// letters — so this cannot assume words, and a Korean line has to be measured in cells rather
/// than characters or every wrapped paragraph would run past the border.
fn wrap(text: &str, width: usize) -> Vec<String> {
    if width == 0 {
        return Vec::new();
    }
    let mut lines = Vec::new();
    let mut line = String::new();
    let mut used = 0;
    for word in split_keeping_spaces(text) {
        let size = cells(&word);
        if used + size > width && !line.is_empty() {
            lines.push(std::mem::take(&mut line));
            used = 0;
            if word.trim().is_empty() {
                continue;
            }
        }
        if size > width {
            // One run of characters longer than the whole line: cut it where the line ends.
            for c in word.chars() {
                let step = char_width(c);
                if used + step > width {
                    lines.push(std::mem::take(&mut line));
                    used = 0;
                }
                line.push(c);
                used += step;
            }
            continue;
        }
        line.push_str(&word);
        used += size;
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

/// Splits on spaces, keeping each space attached to the word before it.
fn split_keeping_spaces(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        current.push(c);
        if c == ' ' {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    out
}

/// Thins a series down to `width` points, evenly spaced across the whole of it.
///
/// A run records up to 256 losses and a panel has about forty columns, so something has to go.
/// Taking every nth point keeps the shape of the whole run — the fall from the first step is
/// still visible at the last — where keeping the tail would show a flat line at the end.
fn thin(values: &[f32], width: usize) -> Vec<f64> {
    if width == 0 || values.is_empty() {
        return Vec::new();
    }
    if values.len() <= width {
        return values.iter().map(|v| f64::from(*v)).collect();
    }
    (0..width)
        .map(|i| {
            let index = i * (values.len() - 1) / width.saturating_sub(1).max(1);
            f64::from(values[index.min(values.len() - 1)])
        })
        .collect()
}

/// Long enough for a worker to have taken a step or two; used only by the test that leaves a run
/// alone and comes back to it.
#[cfg(test)]
const TEST_PATIENCE: std::time::Duration = std::time::Duration::from_millis(50);

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use nmtk_core::SizeClass;
    use nmtk_transformer::generate::attention_for_tokens;
    use nmtk_transformer::model::{Model, ModelShape};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    #[test]
    fn enter_at_the_end_of_a_stage_with_knobs_runs_it_again_rather_than_walking_away() {
        let mut session = Session::new(&machine());
        KqSession::go_to(&mut session, STAGE_TUNE);
        // Straight to the end of the conversation: what is tested is what Enter does once the
        // stage has nothing left to say, not how long a real training run takes to get there.
        session.revealed = TUNE.len() - 1;
        assert!(session.at_end(), "the stage should have said everything by now");
        let last_run = TUNE.iter().rposition(|step| matches!(step, Run(_))).expect("a run");
        assert_eq!(session.on(Action::Go), Reaction::Handled, "Enter walked away instead");
        assert_eq!(session.revealed, last_run + 1, "Enter did not rewind to the run");
        session.close();
    }

    #[test]
    fn a_number_past_the_end_of_a_knob_says_where_it_landed() {
        let mut session = Session::new(&machine());
        KqSession::go_to(&mut session, STAGE_TUNE);
        for c in "99999".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        let landed = session.knobs()[0].display();
        let said = session.transcript(Language::ENGLISH);
        assert!(
            said.iter().any(|beat| beat.text.contains(&landed)),
            "the reader was not told where the number landed: {landed}"
        );
        session.close();
    }

    fn machine() -> MachineProfile {
        MachineProfile {
            logical_cores: 4,
            total_memory_bytes: 8 * 1024 * 1024 * 1024,
            available_memory_bytes: 4 * 1024 * 1024 * 1024,
        }
    }

    /// One real forward pass through an untrained model of the smallest interesting size. It
    /// costs a fraction of a millisecond and gives the render test genuine attention weights
    /// without training anything.
    fn real_attention() -> AttentionSnapshot {
        let tokenizer = Tokenizer::from_text(CORPUS);
        let shape =
            ModelShape::new(tokenizer.vocab_size(), 16, 2, 1, 32).expect("a shape this small");
        let model = Model::new(shape, 1);
        let ids = tokenizer.encode("nmtk need more");
        attention_for_tokens(&model, &tokenizer, &ids)
    }

    /// A run as it would look part way through, so the panel can be drawn without waiting a
    /// minute for a real one. Every field here is the shape the engine really publishes.
    /// A snapshot with nothing in it, for tests that care about two fields and not the rest.
    fn blank_snapshot() -> TrainingSnapshot {
        TrainingSnapshot {
            step: 0,
            total_steps: 0,
            state: TrainingState::Finished,
            loss: 0.0,
            raw_loss: 0.0,
            loss_history: Vec::new(),
            gradient_norm: 0.0,
            learning_rate: 0.0,
            tokens_per_second: 0.0,
            tokens_seen: 0,
            parameter_count: 0,
            vocab_size: 33,
            elapsed: Duration::from_secs(0),
            completion: String::new(),
            attention: real_attention(),
        }
    }

    fn part_way(session: &mut Session) {
        session.latest = Some(TrainingSnapshot {
            step: 1_200,
            total_steps: 2_000,
            state: TrainingState::Running,
            loss: 0.412,
            raw_loss: 0.5,
            loss_history: vec![3.31, 2.8, 1.9, 1.2, 0.7, 0.412],
            gradient_norm: 0.8,
            learning_rate: 0.002,
            tokens_per_second: 12_400.0,
            tokens_seen: 900_000,
            parameter_count: 61_344,
            vocab_size: 33,
            elapsed: Duration::from_secs(72),
            completion: format!(" {EXPANSION}."),
            attention: real_attention(),
        });
        session.running_config = Some(session.tuned());
    }

    /// Presses Enter until the stage runs out of steps it can take without waiting.
    fn walk(session: &mut Session) {
        for _ in 0..session.script().len() {
            session.on(Action::Go);
        }
    }

    fn draw(session: &Session, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
        terminal
            .draw(|frame| {
                // The shell gives a quest the right-hand panel: 62% of the width, minus the
                // header and footer rows.
                let area = Rect::new(30, 1, width - 30, height - 2);
                session.render(frame, area, Theme::new(true), Language::ENGLISH);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The panel's own columns at `total`, each row right-trimmed, borders and padding removed.
    fn panel_rows(session: &Session, total: u16, language: Language) -> Vec<String> {
        use nmtk_kq::theme::{MIN_HEIGHT, split};
        let mut terminal = Terminal::new(TestBackend::new(total, MIN_HEIGHT)).expect("backend");
        let (talk, run) = split(total);
        terminal
            .draw(|frame| {
                let [_, body, _] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .areas(frame.area());
                let [_, panel] =
                    Layout::horizontal([Constraint::Length(talk), Constraint::Length(run)])
                        .areas(body);
                session.render(frame, panel, Theme::new(true), language);
            })
            .expect("draw");
        let buffer = terminal.backend().buffer().clone();
        // One border and one column of padding on each side is what `Theme::panel` takes.
        let first = talk + 2;
        let last = talk + run - 2;
        (2..buffer.area.height - 2)
            .map(|y| {
                // A two-cell glyph sits in one cell and blanks the one after it, so the cell
                // after a wide symbol is skipped rather than read as a space.
                let mut text = String::new();
                let mut skip = false;
                for x in first..last {
                    let symbol = buffer[(x, y)].symbol();
                    if skip {
                        skip = false;
                        continue;
                    }
                    skip = cells(symbol) == 2;
                    text.push_str(symbol);
                }
                text.trim_end().to_string()
            })
            .collect()
    }

    /// Cells the panel's own columns come to, at a terminal `total` wide.
    fn panel_width(total: u16) -> usize {
        let (_, run) = nmtk_kq::theme::split(total);
        run as usize - 4
    }

    /// The panel as one run of words, so a sentence that wrapped onto a second line still reads
    /// as the sentence it is.
    fn said(rows: &[String]) -> String {
        rows.iter().flat_map(|row| row.split_whitespace()).collect::<Vec<_>>().join(" ")
    }

    /// A terminal wide enough that nothing this panel draws has to be wrapped or cut, which is
    /// what makes it the answer key: every word the panel means to say appears whole here.
    const ROOMY: u16 = 140;

    /// A session with a run behind it on every stage, which is when the panels have something to
    /// draw and something to run off the edge with.
    fn furnished(stage: usize) -> Session {
        let mut session = Session::new(&machine());
        session.go_to(stage);
        part_way(&mut session);
        let snapshot = session.latest.clone().expect("a run part way through");
        session.record.add(&snapshot, false);
        session.record.add(&snapshot, true);
        session.honest = Some(Finished { snapshot: snapshot.clone(), rate: 0.003 });
        session.broken = Some(Finished { snapshot, rate: 9.0 });
        session
    }

    /// The panel is handed its width, and every line it draws has to fit inside it.
    ///
    /// A reader lost the last of the values worth typing ("4,0"), the half of the attention
    /// heading that says which head the arrow keys are moving, and the end of the model's own
    /// answer — that last with no ellipsis, so it read as the model having stopped mid-word. A
    /// row that fills the last column is only allowed to end on a whole word or on the grid it
    /// is drawing: the words a roomy terminal shows are the ones that have to survive.
    #[test]
    fn no_line_of_any_stage_is_cut_at_the_panel_edge_in_either_language() {
        for total in [nmtk_kq::theme::MIN_WIDTH, 100] {
            for language in Language::ALL {
                for stage in 0..SCRIPTS.len() {
                    let mut session = furnished(stage);
                    for knob in 0..session.stage_knobs().len() {
                        session.chosen = knob;
                        let rows = panel_rows(&session, total, *language);
                        let whole: Vec<String> = panel_rows(&session, ROOMY, *language)
                            .iter()
                            .flat_map(|row| {
                                row.split_whitespace().map(str::to_string).collect::<Vec<_>>()
                            })
                            .collect();
                        for row in &rows {
                            assert!(
                                cells(row) <= panel_width(total),
                                "stage {stage} at {total}: {row:?} is wider than the panel"
                            );
                            if cells(row) < panel_width(total) {
                                continue;
                            }
                            let Some(tail) = row.split_whitespace().last() else { continue };
                            let drawing =
                                tail.chars().all(|c| SHADES.contains(&c.to_string().as_str()));
                            assert!(
                                drawing || tail.ends_with('…') || whole.iter().any(|w| w == tail),
                                "stage {stage} at {total}: {row:?} ends in the middle of {tail:?}"
                            );
                        }
                    }
                    session.close();
                }
            }
        }
    }

    /// The words the panel edge was eating, checked whole on the narrowest screen nmtk allows.
    #[test]
    fn the_words_that_carry_the_meaning_survive_the_narrowest_panel() {
        let narrow = nmtk_kq::theme::MIN_WIDTH;

        let mut training = furnished(STAGE_TRAIN);
        let run = said(&panel_rows(&training, narrow, Language::ENGLISH));
        assert!(
            run.contains("layer 1 · head 1"),
            "the reader cannot see which head the arrow keys are on:\n{run}"
        );
        // The marks are explained whenever one of them is in the part of the grid on screen.
        let header = panel_rows(&training, narrow, Language::ENGLISH)
            .into_iter()
            .skip_while(|row| !row.contains("newest"))
            .nth(1)
            .unwrap_or_default();
        if header.contains('_') || header.contains('/') {
            assert!(run.contains("a line break /"), "the legend for the grid is cut:\n{run}");
        }
        let whole: Vec<String> = training
            .attention_lines(41, 30, Language::ENGLISH, Theme::new(true))
            .iter()
            .map(text_of)
            .collect();
        assert!(
            said(&whole).contains("a line break /"),
            "a grid with a space in it does not say how a space is drawn:\n{whole:#?}"
        );
        training.close();

        let mut broken = furnished(STAGE_BREAK);
        let attack = said(&panel_rows(&broken, narrow, Language::ENGLISH));
        broken.close();
        assert!(
            attack.contains("a runaway learning rate"),
            "the attack the reader is about to run is cut:\n{attack}"
        );
        assert!(
            attack.contains("need more truth knowledge."),
            "a run's own answer is cut short:\n{attack}"
        );

        // Steps are the longest list of suggestions, and the one that lost "4,000" to the edge.
        // A list that does not fit drops whole values; half a number is worse than no number.
        let mut steps = furnished(STAGE_TUNE);
        steps.chosen = TUNE_STEPS;
        let rows = panel_rows(&steps, narrow, Language::ENGLISH);
        steps.close();
        let label = Msg::LabelPresets.text(Language::ENGLISH);
        let line = rows.iter().find(|row| row.contains(label)).expect("no suggestions are drawn");
        let shown: Vec<&str> = line.split(label).nth(1).unwrap_or("").split_whitespace().collect();
        assert!(!shown.is_empty(), "the suggestions vanished altogether:\n{line}");
        for value in &shown {
            assert!(
                STEP_PRESETS.iter().any(|preset| format::count(*preset) == **value),
                "{value:?} is not one of the values worth trying, it is half of one:\n{line}"
            );
        }
    }

    /// Every span of a drawn line, as the reader sees it.
    fn text_of(line: &Line<'static>) -> String {
        line.spans.iter().map(|span| span.content.to_string()).collect()
    }

    /// The row of characters naming the columns was drawn as wide as the panel allowed while the
    /// line above it counted the rows: at 41 columns it named 39 characters over a grid its own
    /// label called 10. A reader cannot point at a column and say which character it is when the
    /// number above the grid is about the other side of it.
    #[test]
    fn the_grid_names_exactly_as_many_columns_as_its_label_counts() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        part_way(&mut session);
        let rows: Vec<String> = session
            .attention_lines(41, 12, Language::ENGLISH, Theme::new(true))
            .iter()
            .map(text_of)
            .collect();
        session.close();
        let counted: usize = rows
            .iter()
            .find(|row| row.contains(Msg::AttentionNewest.text(Language::ENGLISH)))
            .and_then(|row| row.split_whitespace().nth(1)?.parse().ok())
            .expect("the grid never says how much of itself is on screen");
        let header = rows.len() - counted - 1;
        assert_eq!(
            rows[header].trim().chars().count(),
            counted,
            "the label counts {counted} and the header names another number:\n{rows:#?}"
        );
        for row in &rows[header + 1..] {
            assert_eq!(row.chars().count(), counted + 2, "a grid row is not as wide as its header");
        }
    }

    /// The tuning stage leaves the grid a row or two under the four blocks above it, and a reader
    /// who started a second run there watched an empty box under "Where the model looked" for the
    /// whole of it.
    #[test]
    fn a_grid_with_no_room_for_a_row_draws_nothing_rather_than_a_heading() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TUNE);
        part_way(&mut session);
        let theme = Theme::new(true);
        for height in 0..=4 {
            let lines = session.attention_lines(41, height, Language::ENGLISH, theme);
            assert!(lines.is_empty(), "{height} rows drew a heading with no grid under it");
        }
        assert!(
            session.attention_lines(41, 9, Language::ENGLISH, theme).len() > 4,
            "the grid did not draw where there was room for it"
        );
        session.close();
    }

    /// The broken run's answer ran off the bottom of the break panel and the row above it filled
    /// the last column, so the sentence read as if the model had stopped there.
    #[test]
    fn a_panel_with_more_to_say_than_rows_marks_the_line_it_stops_on() {
        let mut session = furnished(STAGE_BREAK);
        session.chosen = BREAK_MULTIPLIER;
        let wrecked = TrainingSnapshot {
            loss: f32::NAN,
            completion: "l".repeat(40),
            ..session.latest.clone().expect("a run part way through")
        };
        session.latest = Some(wrecked.clone());
        session.broken = Some(Finished { snapshot: wrecked, rate: 9.0 });
        let rows = panel_rows(&session, nmtk_kq::theme::MIN_WIDTH, Language::ENGLISH);
        session.close();
        let drawn = rows.iter().flat_map(|row| row.chars()).filter(|c| *c == 'l').count();
        assert!(drawn < 40, "the panel held the whole answer, so this measures nothing now");
        let last = rows.iter().rev().find(|row| !row.trim().is_empty()).expect("nothing was drawn");
        assert!(last.ends_with('…'), "the panel stopped mid-answer without saying so: {last:?}");
    }

    /// One recap line wrote "->" where every other arrow the program draws is "→".
    #[test]
    fn every_arrow_the_panel_draws_is_the_same_arrow() {
        for stage in 0..SCRIPTS.len() {
            let mut session = furnished(stage);
            for language in Language::ALL {
                for row in panel_rows(&session, ROOMY, *language) {
                    assert!(
                        !row.contains("->"),
                        "stage {stage}: {row:?} draws an arrow of its own"
                    );
                }
            }
            session.close();
        }
    }

    /// The values worth trying were drawn at the foot of the whole list, so a reader who had
    /// chosen the learning rate read them as advice about the steps.
    #[test]
    fn the_values_worth_trying_sit_under_the_knob_they_belong_to() {
        let english = Language::ENGLISH;
        let mut session = furnished(STAGE_TUNE);
        for knob in 0..session.stage_knobs().len() {
            session.chosen = knob;
            let rows = panel_rows(&session, ROOMY, english);
            let chosen = rows
                .iter()
                .position(|row| row.starts_with(State::Chosen.mark()))
                .expect("no knob is marked as the chosen one");
            let presets = rows
                .iter()
                .position(|row| row.contains(Msg::LabelPresets.text(english)))
                .expect("no values worth trying are drawn");
            assert_eq!(presets, chosen + 1, "knob {knob}: the suggestions are under another knob");
        }
        session.close();
    }

    /// The learning rate's suggestions ended in the panel's last column, where a reader cannot
    /// tell whether a fourth value was cut off it, and the longest list lost its last values.
    #[test]
    fn the_values_worth_trying_fold_rather_than_filling_the_last_column() {
        let english = Language::ENGLISH;
        let label = Msg::LabelPresets.text(english);
        for (stage, knob, presets) in [
            (STAGE_TUNE, TUNE_RATE, RATE_PRESETS),
            (STAGE_BREAK, BREAK_MULTIPLIER, MULTIPLIER_PRESETS),
        ] {
            for total in [nmtk_kq::theme::MIN_WIDTH, 100] {
                let mut session = furnished(stage);
                session.chosen = knob;
                let rows = panel_rows(&session, total, english);
                session.close();
                let words: Vec<&str> = rows.iter().flat_map(|row| row.split_whitespace()).collect();
                for preset in presets {
                    let value = decimal_text(*preset);
                    assert!(
                        words.contains(&value.as_str()),
                        "stage {stage} at {total}: {value} is not on screen at all"
                    );
                }
                for row in rows.iter().filter(|row| row.contains(label)) {
                    assert!(
                        cells(row) + PRESET_GAP <= panel_width(total),
                        "stage {stage} at {total}: {row:?} runs to the panel edge"
                    );
                }
            }
        }
    }

    #[test]
    fn it_opens_on_one_sentence_and_walks_to_every_stage() {
        let mut session = Session::new(&machine());
        assert_eq!(session.stage(), STAGE_QUESTION);
        assert_eq!(session.transcript(Language::ENGLISH).len(), 1, "a wall of text on opening");
        for stage in 0..SCRIPTS.len() {
            assert_eq!(session.on(Action::Stage(stage)), Reaction::Handled);
            assert_eq!(session.stage(), stage);
            assert!(
                !session.transcript(Language::ENGLISH).is_empty(),
                "stage {stage} says nothing"
            );
        }
        session.close();
    }

    /// The grid is on screen from the first step. Its explanation used to arrive after the run
    /// had finished, so a reviewer watched an unreadable field of shaded blocks for ninety seconds.
    #[test]
    fn the_attention_grid_is_explained_while_it_is_on_screen() {
        let finished = TRAIN
            .iter()
            .position(|step| matches!(step, Await(Until::Finished)))
            .expect("the training stage waits for the run to end");
        for message in [Msg::TrainGrid, Msg::TrainGridRead] {
            let at = TRAIN
                .iter()
                .position(|step| matches!(step, Say(m) if *m == message))
                .unwrap_or_else(|| panic!("{message:?} is not in the training stage"));
            assert!(at < finished, "{message:?} is only said once the run is over");
        }
    }

    /// A reviewer pressed Enter without choosing the attack the conversation had just named, and
    /// was told the conclusion about that attack anyway — over a run that had not used it.
    #[test]
    fn a_lesson_about_an_attack_is_only_said_when_that_attack_is_what_ran() {
        let mut session = Session::new(&machine());
        let lesson = Msg::BreakAfterSgd;
        let topic = Topic::Attack {
            kind: ATTACK_PLAIN_SGD,
            multiplier: 1.0,
            lesson,
            check: Check::Crawled,
        };
        // A plain-SGD run as the engine really gives one: 3.35 down to 2.63 in 1,500 steps.
        session.latest = Some(TrainingSnapshot {
            loss: 2.63,
            loss_history: vec![3.35, 3.1, 2.9, 2.63],
            completion: " t te te t te te t te the te te t th".to_string(),
            ..blank_snapshot()
        });

        session.ran = None;
        let other = session.tell(topic, Language::ENGLISH);
        assert!(other.starts_with(Msg::BreakNotThatRun.text(Language::ENGLISH)), "{other}");
        assert!(other.contains("3.350 → 2.630"), "the run it was about is not read: {other}");

        // The runaway attack at its own multiplier is not plain SGD at one.
        session.ran = Some((ATTACK_RUNAWAY, BREAKING_MULTIPLIER));
        let other = session.tell(topic, Language::ENGLISH);
        assert!(other.starts_with(Msg::BreakNotThatRun.text(Language::ENGLISH)), "{other}");

        session.ran = Some((ATTACK_PLAIN_SGD, 1.0));
        assert_eq!(session.tell(topic, Language::ENGLISH), lesson.text(Language::ENGLISH));
    }

    /// Choosing the attack the conversation named is not the same as the run doing what the
    /// lesson says. The attack is built on the reader's own settings from the stage before, and
    /// "it barely got going" was said over plain SGD on a shape where it learned the sentence.
    #[test]
    fn a_lesson_is_only_said_when_the_run_really_did_it() {
        let english = Language::ENGLISH;
        let mut session = Session::new(&machine());
        let run = |loss: f32, history: Vec<f32>, completion: &str| TrainingSnapshot {
            loss,
            loss_history: history,
            completion: completion.to_string(),
            ..blank_snapshot()
        };
        let cases = [
            // What the engine really does at each suggested setting, measured on the defaults.
            (
                ATTACK_RUNAWAY,
                BREAKING_MULTIPLIER,
                Check::Collapsed,
                Msg::BreakAfterRunaway,
                run(9.3, vec![3.35, 5.0, 9.3], "gggggnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn"),
                run(0.2, vec![3.35, 1.0, 0.2], &format!(" {EXPANSION}.")),
            ),
            (
                ATTACK_NO_WARMUP,
                10.0,
                Check::Misspelt,
                Msg::BreakAfterWarmup,
                run(1.49, vec![3.3, 2.0, 1.49], " ned more truth knowledge and and anmore"),
                run(0.15, vec![3.3, 1.0, 0.15], &format!(" {EXPANSION}.")),
            ),
            (
                ATTACK_PLAIN_SGD,
                1.0,
                Check::Crawled,
                Msg::BreakAfterSgd,
                run(2.63, vec![3.35, 2.9, 2.63], " t te te t te te t te the"),
                run(0.3, vec![3.35, 1.0, 0.3], &format!(" {EXPANSION}.")),
            ),
            (
                ATTACK_PLAIN_SGD,
                100.0,
                Check::Found,
                Msg::BreakLesson,
                run(1.8, vec![3.34, 2.0, 1.8], &format!(" {EXPANSION} knowledgedge")),
                run(2.9, vec![3.34, 3.0, 2.9], " t t t t t t t t t t"),
            ),
        ];
        for (kind, multiplier, check, lesson, did, did_not) in cases {
            let topic = Topic::Attack { kind, multiplier, lesson, check };
            session.ran = Some((kind, multiplier));
            session.latest = Some(did);
            // A collapse goes on to name its numbers and its character; every lesson starts with
            // its own sentence.
            let said = session.tell(topic, english);
            assert!(said.starts_with(lesson.text(english)), "{lesson:?}: {said}");
            session.latest = Some(did_not);
            let instead = session.tell(topic, english);
            assert!(
                instead.starts_with(Msg::BreakNotWhatWasSaid.text(english)),
                "{lesson:?} was said over a run that did not do it: {instead}"
            );
        }
        // A run that blew up is said to have blown up, not printed as NaN.
        session.ran = Some((ATTACK_RUNAWAY, BREAKING_MULTIPLIER));
        session.latest = Some(run(f32::NAN, vec![3.35, 1e6, f32::NAN], ""));
        let topic = Topic::Attack {
            kind: ATTACK_RUNAWAY,
            multiplier: BREAKING_MULTIPLIER,
            lesson: Msg::BreakAfterRunaway,
            check: Check::Collapsed,
        };
        let said = session.tell(topic, english);
        assert!(said.contains(Msg::BreakItBlewUp.text(english)), "{said}");
        assert!(!said.contains("NaN") && !said.contains("inf"), "{said}");
    }

    /// The first attack's lesson rewrote itself into "those were not the suggested settings" the
    /// moment the second attack started, because every `Tell` read whatever run was current.
    #[test]
    fn a_sentence_already_said_keeps_the_numbers_it_was_about() {
        let english = Language::ENGLISH;
        let mut session = Session::new(&machine());
        session.go_to(STAGE_BREAK);
        let at = BREAK.iter().position(|step| matches!(step, Tell(_))).expect("a Tell");
        session.ran = Some((ATTACK_RUNAWAY, BREAKING_MULTIPLIER));
        session.latest = Some(TrainingSnapshot {
            loss: 9.3,
            loss_history: vec![3.35, 5.0, 9.3],
            completion: "gggggnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn".to_string(),
            ..blank_snapshot()
        });
        session.revealed = at - 1;
        assert!(session.advance());
        let lesson = Msg::BreakAfterRunaway.text(english);
        let said = |session: &Session| {
            session.transcript(english).iter().any(|beat| beat.text.starts_with(lesson))
        };
        assert!(said(&session), "the lesson was not said at all");
        // The next attack starts; the sentence above it is about the run before.
        session.ran = Some((ATTACK_NO_WARMUP, 10.0));
        session.latest = Some(blank_snapshot());
        assert!(said(&session), "the first lesson rewrote itself when the second run began");
        session.close();
    }

    /// "Smaller, faster, and worse" was printed over a model the reader had made bigger.
    #[test]
    fn the_settings_verdict_reads_the_two_runs_rather_than_assuming_one() {
        let mut session = Session::new(&machine());
        let english = Language::ENGLISH;
        assert_eq!(session.tell(Topic::Settings, english), Msg::SettingsFirstRun.text(english));

        let run = |weights: usize, loss: f32| Finished {
            snapshot: TrainingSnapshot { parameter_count: weights, loss, ..blank_snapshot() },
            rate: 0.002,
        };
        session.previous = Some(run(107_804, 0.803));
        session.honest = Some(run(157_788, 0.494));
        let bigger_better = session.tell(Topic::Settings, english);
        assert!(
            bigger_better.starts_with(Msg::SettingsBiggerBetter.text(english)),
            "{bigger_better}"
        );
        assert!(bigger_better.contains("107,804"), "the numbers are missing: {bigger_better}");
        assert!(bigger_better.contains("157,788"), "the numbers are missing: {bigger_better}");

        session.honest = Some(run(26_000, 1.900));
        let smaller_worse = session.tell(Topic::Settings, english);
        assert!(
            smaller_worse.starts_with(Msg::SettingsSmallerWorse.text(english)),
            "{smaller_worse}"
        );

        session.honest = Some(run(107_804, 0.700));
        let same = session.tell(Topic::Settings, english);
        assert!(same.starts_with(Msg::SettingsSameShape.text(english)), "{same}");

        // A composed beat is still a beat, and the standard puts a beat under 160 characters.
        for language in Language::ALL {
            let said = session.tell(Topic::Settings, *language);
            assert!(said.chars().count() <= 160, "too much at once in {language}: {said:?}");
        }
    }

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

    /// The words that name layers, heads, width and window have to exist: a reader who is shown
    /// four numbers and told what none of them mean has been shown nothing.
    #[test]
    fn the_four_numbers_on_the_panel_are_each_given_a_meaning() {
        for message in [Msg::PiecesWidth, Msg::PiecesHeads, Msg::PiecesLayers, Msg::PiecesWindow] {
            assert!(PIECES.iter().any(|step| matches!(step, Say(m) if *m == message)));
        }
    }

    #[test]
    fn only_the_stages_with_something_to_turn_have_knobs() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_PIECES);
        assert!(session.knobs().is_empty());
        assert_eq!(session.chosen_knob(), None);
        session.go_to(STAGE_TUNE);
        assert_eq!(session.knobs().len(), 5);
        assert_eq!(session.chosen_knob(), Some(0));
        session.go_to(STAGE_BREAK);
        assert_eq!(session.knobs().len(), 2);
        session.go_to(STAGE_RECAP);
        assert!(session.knobs().is_empty());
    }

    #[test]
    fn the_reader_moves_through_the_knobs_and_wraps() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TUNE);
        for expected in [1, 2, 3, 4, 0] {
            session.on(Action::Next);
            assert_eq!(session.chosen_knob(), Some(expected));
        }
        session.on(Action::Previous);
        assert_eq!(session.chosen_knob(), Some(4));
    }

    #[test]
    fn a_typed_number_changes_the_model_that_would_be_built() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TUNE);
        let before = session.weights_now().expect("the machine's own settings build");
        for c in "128".chars() {
            session.on(Action::Type(c));
        }
        // The chosen knob is the first one, layers; go to the width instead.
        session.on(Action::Cancel);
        session.on(Action::Next);
        session.on(Action::Next);
        for c in "128".chars() {
            session.on(Action::Type(c));
        }
        assert_eq!(session.on(Action::Commit), Reaction::Handled);
        let after = session.weights_now().expect("128 divides by the default heads");
        assert!(after > before, "a wider model has more weights: {before} then {after}");
    }

    #[test]
    fn settings_that_do_not_describe_a_model_say_so_instead_of_a_number() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TUNE);
        // Three heads do not divide the default width of 64.
        session.on(Action::Next);
        for c in "3".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.weights_now(), Err(Msg::ErrorHeadsDoNotDivideWidth));
    }

    #[test]
    fn the_attack_raises_the_learning_rate_and_r_puts_it_back() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_BREAK);
        let sensible = session.tuned().learning_rate;
        let attacked = session.attacked().learning_rate;
        assert!(
            attacked > sensible * 100.0,
            "the break stage should open on a rate that really breaks: {attacked} against {sensible}"
        );
        session.on(Action::Reset);
        assert_eq!(session.attacked(), session.tuned(), "r did not recover the settings");
    }

    #[test]
    fn each_attack_changes_one_thing_and_the_multiplier_reaches_all_three() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_BREAK);
        session.on(Action::Reset);
        let sensible = session.tuned();
        assert_eq!(session.attacked(), sensible, "recovered settings are the sensible ones");

        session.on(Action::Nudge(1));
        assert_eq!(session.attacked().warmup_steps, 0, "the second attack is the missing warmup");
        assert_eq!(session.attacked().optimizer, sensible.optimizer);

        session.on(Action::Nudge(1));
        assert_eq!(session.attacked().optimizer, Optimizer::Sgd, "the third is the update rule");
        assert_eq!(session.attacked().warmup_steps, sensible.warmup_steps);

        // Every attack answers the multiplier, so the rate the panel shows is the rate being used.
        session.on(Action::Next);
        for c in "100".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        let rate = session.attacked().learning_rate;
        assert!(
            (rate - sensible.learning_rate * 100.0).abs() < 1e-6,
            "plain SGD ignored the multiplier: {rate}"
        );
    }

    #[test]
    fn walking_into_the_training_stage_and_pressing_enter_starts_a_real_run() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        assert!(session.handle.is_some(), "Enter did not start a run");
        assert!(!session.can_advance(), "the conversation ran past the run it is about");
        std::thread::sleep(TEST_PATIENCE);
        session.tick();
        assert!(session.latest.as_ref().is_some_and(|s| s.total_steps > 0));
        session.close();
        assert!(session.handle.is_none(), "close left a worker behind");
    }

    /// Leaving a stage stops its run. One trainer at a time, or two runs halve each other and
    /// every number either of them reports is wrong.
    #[test]
    fn walking_out_of_the_training_stage_stops_the_trainer() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        assert!(session.handle.is_some());
        session.go_to(STAGE_RECAP);
        assert!(session.handle.is_none(), "a trainer was left running behind another stage");
        session.close();
    }

    /// The old version answered Enter during training by doing nothing, so a reader who changed a
    /// setting and pressed Enter watched the old model finish and concluded the key was broken.
    #[test]
    fn a_second_run_replaces_the_first_rather_than_being_ignored() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        let first = session.running_config.expect("a run was started");
        session.go_to(STAGE_TUNE);
        session.chosen = TUNE_WIDTH;
        for c in "128".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        walk(&mut session);
        let second = session.running_config.expect("the second run was refused");
        assert_ne!(first.d_model, second.d_model, "the second Enter changed nothing");
        session.close();
    }

    #[test]
    fn pausing_and_resuming_reach_the_run() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        assert_eq!(session.on(Action::PauseOrResume), Reaction::Handled);
        assert!(session.handle.as_ref().is_some_and(|h| h.is_paused()));
        session.on(Action::PauseOrResume);
        assert!(session.handle.as_ref().is_some_and(|h| !h.is_paused()));
        session.close();
    }

    #[test]
    fn the_run_panel_draws_the_numbers_the_curve_and_the_answer_at_80_by_24() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        part_way(&mut session);
        let screen = draw(&session, 80, 24);
        assert!(screen.contains("1,200 / 2,000"), "no step count:\n{screen}");
        assert!(screen.contains("0.412"), "no loss:\n{screen}");
        assert!(screen.contains("61,344"), "no weight count:\n{screen}");
        assert!(screen.contains("need more truth knowledge"), "no answer:\n{screen}");
        assert!(screen.contains("Where the model looked"), "no attention:\n{screen}");
        assert!(
            screen.chars().any(|c| SHADES[1..].contains(&c.to_string().as_str())),
            "the attention grid drew no weights:\n{screen}"
        );
        assert!(
            screen.lines().all(|line| line.chars().count() <= 80),
            "something drew past the edge:\n{screen}"
        );
    }

    #[test]
    fn every_stage_draws_inside_80_by_24() {
        let mut session = Session::new(&machine());
        part_way(&mut session);
        session.honest = Some(Finished {
            snapshot: session.latest.clone().expect("a run part way through"),
            rate: 0.003,
        });
        for stage in 0..SCRIPTS.len() {
            session.go_to(stage);
            part_way(&mut session);
            let screen = draw(&session, 80, 24);
            let panel: String = screen
                .lines()
                .map(|line| line.chars().skip(30).collect::<String>())
                .collect::<Vec<_>>()
                .join("");
            assert!(
                panel.trim().chars().filter(|c| !c.is_whitespace()).count() > 20,
                "stage {stage} drew almost nothing:\n{screen}"
            );
        }
    }

    #[test]
    fn the_recap_reads_the_loss_from_one_end_of_the_run_to_the_other() {
        let mut session = Session::new(&machine());
        part_way(&mut session);
        let run = session.latest.clone().expect("a run part way through");
        session.record.add(&run, false);
        session.go_to(STAGE_RECAP);
        let screen = draw(&session, 80, 24);
        assert!(screen.contains("3.310 → 0.412"), "no loss from end to end:\n{screen}");
        assert!(screen.contains("need more truth knowledge"), "no answer:\n{screen}");
    }

    /// The standard's own example: a reader who trained for a long run and then tried a short
    /// experiment was told about the short one. The recap is every run added up.
    #[test]
    fn the_recap_is_the_whole_quest_rather_than_the_last_run() {
        let english = Language::ENGLISH;
        let mut session = Session::new(&machine());
        part_way(&mut session);
        let long = session.latest.clone().expect("a run part way through");
        let short = TrainingSnapshot {
            step: 40,
            total_steps: 40,
            loss: 2.9,
            loss_history: vec![3.3, 3.1, 2.9],
            elapsed: Duration::from_secs(3),
            completion: " nnnnnnn".to_string(),
            ..long.clone()
        };
        session.record.add(&long, false);
        session.record.add(&short, false);
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let said: Vec<String> =
            session.transcript(english).iter().map(|beat| beat.text.clone()).collect();
        let all = said.join("\n");
        assert!(all.contains("runs 2"), "{all}");
        assert!(all.contains("steps 1,240"), "the recap counted the last run only:\n{all}");
        assert!(all.contains("time 1m 15s") || all.contains("1m 15s"), "{all}");
        assert!(all.contains("61,344"), "the weights it started from are not said:\n{all}");
        assert!(all.contains("step 1,200"), "when the sentence came is not said:\n{all}");

        // The panel reads the same record: the best run's answer, and every step taken.
        let screen = draw(&session, 80, 24);
        assert!(screen.contains("1,240"), "{screen}");
        assert!(
            screen.contains("need more truth knowledge"),
            "not the best run's answer:\n{screen}"
        );
        session.close();
    }

    /// Walking to the recap in the middle of a run keeps what the run measured; it used to be
    /// dropped, and the recap said nothing had been trained.
    #[test]
    fn a_run_left_half_way_still_reaches_the_recap() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        let mut waited = 0;
        while session.latest.as_ref().is_none_or(|s| s.step == 0) && waited < 400 {
            std::thread::sleep(Duration::from_millis(10));
            session.tick();
            waited += 1;
        }
        assert!(session.latest.as_ref().is_some_and(|s| s.step > 0), "the run never started");
        session.go_to(STAGE_RECAP);
        assert!(session.handle.is_none(), "the run was left going behind the recap");
        assert_eq!(session.record.runs, 1, "the run that was walked away from was dropped");
        assert!(session.record.steps > 0);
        let opening = session.transcript(Language::ENGLISH);
        assert!(
            opening[0].text != Msg::RecapNothing.text(Language::ENGLISH),
            "the recap says nothing was trained"
        );
        session.close();
    }

    /// `r` is the one key that throws a run away.
    #[test]
    fn r_throws_the_run_in_progress_away() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        let mut waited = 0;
        while session.latest.as_ref().is_none_or(|s| s.step == 0) && waited < 400 {
            std::thread::sleep(Duration::from_millis(10));
            session.tick();
            waited += 1;
        }
        session.on(Action::Reset);
        assert!(session.handle.is_none());
        assert_eq!(session.record.runs, 0, "r kept the run it was meant to throw away");
        session.close();
    }

    #[test]
    fn a_narrow_panel_draws_rather_than_panicking() {
        let mut session = Session::new(&machine());
        for stage in 0..SCRIPTS.len() {
            session.go_to(stage);
            part_way(&mut session);
            for (width, height) in [(80u16, 24u16), (80, 10), (34, 24), (31, 4)] {
                let _ = draw(&session, width, height);
            }
        }
    }

    #[test]
    fn a_series_longer_than_the_panel_is_thinned_to_fit_and_keeps_its_ends() {
        let values: Vec<f32> = (0..256).map(|i| 3.3 - i as f32 / 100.0).collect();
        let points = thin(&values, 40);
        assert_eq!(points.len(), 40);
        assert!((points[0] - 3.3).abs() < 1e-4);
        assert!((points[39] - f64::from(values[255])).abs() < 1e-4);
        assert!(thin(&[], 40).is_empty());
        assert!(thin(&values, 0).is_empty());
    }

    #[test]
    fn the_added_keys_are_hints_rather_than_sentences() {
        let mut session = Session::new(&machine());
        for stage in 0..SCRIPTS.len() {
            session.go_to(stage);
            for (key, hint) in session.keys(Language::ENGLISH) {
                assert!(!key.is_empty() && cells(key) <= 6, "stage {stage}: {key:?} is not a key");
                assert!(cells(hint) <= 20, "stage {stage}: {hint:?} is a sentence, not a hint");
                assert!(!hint.ends_with('.'), "stage {stage}: {hint:?} is a sentence");
            }
        }
    }

    #[test]
    fn a_weight_of_nothing_and_a_weight_of_everything_look_different() {
        assert_eq!(shade(0.0), " ");
        assert_eq!(shade(1.0), "█");
        assert_eq!(shade(f32::NAN), " ");
    }

    /// A run the reader made shorter than the step the conversation waits for used to leave the
    /// conversation waiting for ever: step 300 of a 100-step run never comes.
    #[test]
    fn a_wait_for_a_step_the_run_never_reaches_ends_when_the_run_does() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        let wait = TRAIN
            .iter()
            .position(|step| matches!(step, Await(Until::Steps(300))))
            .expect("the stage waits for step 300");
        session.revealed = wait;
        session.latest = Some(TrainingSnapshot { step: 100, total_steps: 100, ..blank_snapshot() });
        assert!(session.can_advance(), "a finished 100-step run still waits for step 300");
        session.latest = Some(TrainingSnapshot {
            step: 100,
            total_steps: 8_000,
            state: TrainingState::Running,
            ..blank_snapshot()
        });
        assert!(!session.can_advance(), "a run still going has not reached step 300");
        session.close();
    }

    /// Settings that cannot be built are refused, and the first wait on the training stage is for
    /// step one of a run that never started.
    #[test]
    fn a_refused_run_does_not_stop_the_conversation() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TUNE);
        session.chosen = TUNE_HEADS;
        session.on(Action::Type('3'));
        session.on(Action::Commit);
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        assert!(session.refused.is_some(), "three heads over this width should be refused");
        assert!(session.handle.is_none());
        assert!(session.at_end(), "the conversation stopped behind the refusal");
        let said = session.transcript(Language::ENGLISH);
        assert!(
            said.iter()
                .any(|beat| beat.text == Msg::ErrorHeadsDoNotDivideWidth.text(Language::ENGLISH)),
            "the reason is not said"
        );
        assert!(
            said.iter().any(|beat| beat.text == Msg::TrainNoRun.text(Language::ENGLISH)),
            "the progress sentence talked about a run that never happened"
        );
        session.close();
    }

    /// Looking at another head is not a new setting. It used to count as one, so Enter while the
    /// run was going threw away the run and started again from random numbers.
    #[test]
    fn choosing_another_head_and_pressing_enter_does_not_restart_training() {
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        walk(&mut session);
        assert!(session.handle.is_some(), "no run to keep");
        let started = session.running_config;
        session.on(Action::Nudge(1));
        assert!(!session.knobs_moved(), "the head chooser counts as a training setting");
        let before = session.log.len();
        session.on(Action::Go);
        let restarted = session
            .log
            .iter()
            .skip(before)
            .any(|logged| matches!(logged.what, Happening::Started { .. }));
        assert!(!restarted, "Enter after choosing a head started the run again");
        assert_eq!(session.running_config, started);
        assert!(session.handle.is_some());
        session.close();
    }

    /// A choice is typed the way it is shown: counted from one, and a number past the end is said
    /// in the words the panel uses, not as an index counted from nought.
    #[test]
    fn a_choice_is_typed_counting_from_one_and_lands_in_words() {
        let english = Language::ENGLISH;
        let mut session = Session::new(&machine());
        session.go_to(STAGE_BREAK);
        session.chosen = BREAK_ATTACK;
        session.on(Action::Type('3'));
        assert_eq!(session.on(Action::Commit), Reaction::Handled);
        assert_eq!(session.attacked().optimizer, Optimizer::Sgd, "3 is the third attack");
        let before = session.transcript(english).len();
        session.on(Action::Type('9'));
        session.on(Action::Commit);
        let said = session.transcript(english);
        assert_eq!(said.len(), before + 1, "a number past the end was not remarked on");
        let last = &said[said.len() - 1].text;
        assert!(last.contains(Msg::AttackPlainSgd.text(english)), "landed on an index: {last}");
        session.on(Action::Type('1'));
        session.on(Action::Commit);
        assert_eq!(
            session.attacked(),
            TrainingConfig {
                learning_rate: session.tuned().learning_rate * BREAKING_MULTIPLIER as f32,
                ..session.tuned()
            },
            "1 is the first attack"
        );

        // The head chooser is counted the same way the panel counts it.
        let mut session = Session::new(&machine());
        session.go_to(STAGE_TRAIN);
        session.on(Action::Type('2'));
        session.on(Action::Commit);
        assert_eq!(session.view.first().map(choice_of), Some(1), "2 showed the third head");
        session.on(Action::Type('0'));
        session.on(Action::Commit);
        let said = session.transcript(english);
        let last = &said[said.len() - 1].text;
        assert!(last.contains("layer 1 · head 1"), "{last}");
        session.close();
    }

    /// A learning rate the reader turned up too far blows the run up, and the sentences that read
    /// it used to say "it is still falling" and "a lower loss" over a loss that was not a number.
    #[test]
    fn sentences_about_a_run_that_blew_up_say_so() {
        let english = Language::ENGLISH;
        let mut session = Session::new(&machine());
        session.latest = Some(TrainingSnapshot {
            step: 1_000,
            loss: f32::NAN,
            loss_history: vec![3.3, 1e6, f32::NAN],
            ..blank_snapshot()
        });
        assert_eq!(session.tell(Topic::Progress, english), Msg::TrainBlewUp.text(english));

        // Rising rather than falling is not "flattened out".
        let rising: Vec<f32> = (0..40).map(|i| 3.3 + i as f32 * 0.2).collect();
        session.latest =
            Some(TrainingSnapshot { loss: 11.1, loss_history: rising, ..blank_snapshot() });
        assert_eq!(session.tell(Topic::Progress, english), Msg::TrainNotFalling.text(english));

        let run = |weights: usize, loss: f32| Finished {
            snapshot: TrainingSnapshot { parameter_count: weights, loss, ..blank_snapshot() },
            rate: 0.002,
        };
        session.previous = Some(run(107_804, 0.8));
        session.honest = Some(run(26_000, f32::NAN));
        let said = session.tell(Topic::Settings, english);
        assert!(said.starts_with(Msg::SettingsBlewUp.text(english)), "{said}");
        assert!(!said.contains("NaN"), "{said}");
    }

    /// A run that fell and then climbed back above where it started did get below where it
    /// started, and its ending used to say it never had.
    #[test]
    fn a_run_that_dipped_and_climbed_back_is_not_said_to_have_never_fallen() {
        let mut session = Session::new(&machine());
        session.latest = Some(TrainingSnapshot {
            step: 10,
            state: TrainingState::Running,
            loss: 3.3,
            loss_history: vec![3.3],
            ..blank_snapshot()
        });
        session.notice();
        session.latest = Some(TrainingSnapshot {
            step: 1_500,
            state: TrainingState::Finished,
            loss: 3.5,
            loss_history: vec![3.3, 1.2, 2.0, 3.5],
            ..blank_snapshot()
        });
        session.notice();
        let ended = session.log.iter().find_map(|logged| match logged.what {
            Happening::Ended { fell, .. } => Some(fell),
            _ => None,
        });
        assert_eq!(ended, Some(true), "a loss that reached 1.2 was said never to have fallen");
    }

    /// Four attacks of up to eight happenings each outgrew a cap of twenty-four, and the ending of
    /// the last attack was silently never said.
    #[test]
    fn every_happening_of_a_long_stage_is_kept() {
        let mut session = Session::new(&machine());
        for step in 0..40 {
            session.say(Happening::GotIt { step });
        }
        session.say(Happening::Ended { loss: 0.2, seconds: 1.0, fell: true });
        assert_eq!(session.log.len(), 41);
    }

    /// "Set the width to 32" was said to a machine whose width already was 32.
    #[test]
    fn the_suggested_width_is_a_change_on_every_machine() {
        for class in [SizeClass::Small, SizeClass::Medium, SizeClass::Large] {
            let machine = TrainingConfig::for_size_class(class);
            let heads = machine.heads;
            let facts = Facts {
                refused: false,
                ran: None,
                latest: None,
                honest: None,
                previous: None,
                record: Record::default(),
                machine,
            };
            let asked = facts.suggest_width(Language::ENGLISH);
            let half = (machine.d_model / 2 / heads * heads).max(heads);
            assert_ne!(half, machine.d_model, "{class:?}: the suggestion is no change");
            let smaller = TrainingConfig { d_model: half, ..machine };
            assert_eq!(smaller.validate(), Ok(()), "{class:?}: the suggestion is refused");
            assert!(asked.contains(&format!("width {} → {half}", machine.d_model)), "{asked}");
            assert!(smaller.parameter_count() < machine.parameter_count());
        }
    }

    /// Every sentence the quest composes from numbers is still a beat: two sentences at most, and
    /// under the length the standard sets, in both languages.
    #[test]
    fn composed_beats_are_short_in_both_languages() {
        let mut session = Session::new(&machine());
        part_way(&mut session);
        let long = session.latest.clone().expect("a run part way through");
        session.record.add(&long, false);
        session.record.add(&TrainingSnapshot { loss: f32::NAN, ..long.clone() }, true);
        session.honest = Some(Finished { snapshot: long.clone(), rate: 0.003 });
        session.previous = Some(Finished { snapshot: long.clone(), rate: 0.003 });
        let topics = [
            Topic::Progress,
            Topic::Settings,
            Topic::SuggestWidth,
            Topic::RecapStart,
            Topic::RecapWork,
            Topic::RecapSentence,
            Topic::RecapBroken,
            Topic::Attack {
                kind: ATTACK_RUNAWAY,
                multiplier: 3000.0,
                lesson: Msg::BreakAfterRunaway,
                check: Check::Collapsed,
            },
        ];
        for language in Language::ALL {
            for ran in [None, Some((ATTACK_RUNAWAY, 3000.0))] {
                session.ran = ran;
                for topic in topics {
                    let said = session.tell(topic, *language);
                    assert!(cells(&said) <= 2 * 160, "{topic:?} in {language}: {said}");
                    assert!(said.chars().count() <= 160, "{topic:?} in {language} is long: {said}");
                    assert!(sentences(&said) <= 2, "{topic:?} in {language}: {said}");
                }
            }
        }
        session.close();
    }

    /// How many sentences a beat is: full stops, question and exclamation marks that end a word.
    fn sentences(text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        (0..chars.len())
            .filter(|&i| {
                matches!(chars[i], '.' | '?' | '!')
                    && chars.get(i + 1).is_none_or(|next| next.is_whitespace())
            })
            .count()
            .max(1)
    }

    #[test]
    fn every_fixed_beat_is_two_sentences_at_most() {
        for script in SCRIPTS {
            for step in script {
                let (Say(message) | Ask(message)) = step else { continue };
                for language in Language::ALL {
                    let text = message.text(*language);
                    assert!(sentences(text) <= 2, "{message:?} in {language}: {text}");
                }
            }
        }
    }

    /// Every line has a Korean column of its own. The table falls back to English for a missing
    /// one, which reads, but a Korean screen with English sentences in it is not translated.
    #[test]
    fn every_line_is_translated_into_korean() {
        for msg in Msg::ALL {
            let (en, ko) = (msg.text(Language::ENGLISH), msg.text(Language::KOREAN));
            assert_ne!(en, ko, "{msg:?} has no Korean of its own");
            assert!(
                ko.chars().any(|c| ('\u{AC00}'..='\u{D7A3}').contains(&c)),
                "{msg:?} is not Korean: {ko}"
            );
        }
    }

    /// Words a reader meets on this quest are explained before they are used, and the jargon that
    /// never was is gone.
    #[test]
    fn words_are_explained_before_they_are_used() {
        let at = |script: &[Step], message: Msg| {
            script
                .iter()
                .position(|step| matches!(step, Say(m) | Ask(m) if *m == message))
                .unwrap_or_else(|| panic!("{message:?} is not in the script"))
        };
        assert!(at(TRAIN, Msg::TrainStep) < at(TRAIN, Msg::TrainAsk), "step used before said");
        assert!(at(BREAK, Msg::BreakAdamName) < at(BREAK, Msg::BreakSgdThree), "AdamW unnamed");
        assert!(at(BREAK, Msg::BreakGradient) < at(BREAK, Msg::BreakWarmupTwo));
        for msg in Msg::ALL {
            let text = msg.text(Language::ENGLISH).to_lowercase();
            for jargon in ["nats", "forward pass", "weight decay", "batch"] {
                assert!(!text.contains(jargon), "{msg:?} says {jargon:?}, which nothing explains");
            }
        }
        // The opening screen shows the one number its first stage explains.
        let session = Session::new(&machine());
        let screen = draw(&session, 80, 24);
        assert!(screen.contains("Weights"), "{screen}");
        for word in ["Layers", "Heads", "Width", "Window", "Characters"] {
            assert!(!screen.contains(word), "the first screen shows {word} unexplained:\n{screen}");
        }
    }

    /// The numbers the prose puts on the engine, checked against the engine.
    #[test]
    fn the_numbers_the_prose_names_are_the_engines_numbers() {
        // "Guessing blindly over this alphabet scores about 3.3."
        let chance = (Tokenizer::from_text(CORPUS).vocab_size() as f64).ln();
        assert!((chance - 3.3).abs() < 0.1, "blind guessing scores {chance:.2}");
        assert!(Msg::TrainScale.text(Language::ENGLISH).contains("3.3"));

        // "Weights grow roughly with the square of the width."
        let at = |d_model: usize| {
            TrainingConfig { d_model, ..TrainingConfig::for_size_class(SizeClass::Medium) }
                .parameter_count() as f64
        };
        let growth = at(256) / at(128);
        assert!((3.5..=4.1).contains(&growth), "doubling the width multiplied weights by {growth}");

        // "three thousand times the sensible one."
        assert!(Msg::BreakAskRunaway.text(Language::ENGLISH).contains("three thousand"));
        assert_eq!(BREAKING_MULTIPLIER, 3000.0);
        // "ten times" and "100" in the asks are the multipliers the lessons are checked against.
        assert!(BREAK.iter().any(
            |step| matches!(step, Tell(Topic::Attack { multiplier, .. }) if *multiplier == 10.0)
        ));
        assert!(BREAK.iter().any(
            |step| matches!(step, Tell(Topic::Attack { multiplier, .. }) if *multiplier == 100.0)
        ));

        // "the sensible schedule starts the rate near nothing and ramps it up": every machine
        // gets a warmup, and its first step is a small fraction of the peak.
        for class in [SizeClass::Small, SizeClass::Medium, SizeClass::Large] {
            let config = TrainingConfig::for_size_class(class);
            assert!(config.warmup_steps > 0, "{class:?} has no warmup");
            assert!(config.learning_rate_at(0) < config.learning_rate * 0.05);
        }
    }

    /// The runaway attack on a real engine, at the multiplier the stage opens on: the loss never
    /// gets below where it started and the answer is one character over and over. Small and short
    /// so it runs in about a second; the stage checks the same thing on the reader's own run.
    #[test]
    fn the_runaway_rate_really_collapses_the_model() {
        let config = TrainingConfig {
            layers: 1,
            heads: 2,
            d_model: 32,
            context: 32,
            batch: 8,
            steps: 300,
            warmup_steps: 20,
            ..TrainingConfig::for_size_class(SizeClass::Small)
        };
        let config = TrainingConfig {
            learning_rate: config.learning_rate * BREAKING_MULTIPLIER as f32,
            ..config
        };
        let mut trainer = nmtk_transformer::Trainer::new(config).expect("a runaway rate is a rate");
        while trainer.step().is_some() {}
        trainer.refresh_sample();
        let run = trainer.snapshot(TrainingState::Finished, Duration::ZERO, 0.0);
        assert!(
            holds(Check::Collapsed, &run),
            "loss {:?}, answer {:?}",
            run.loss_history,
            run.completion
        );
    }

    /// The model a runaway rate wrecks most often falls onto the space, the commonest character
    /// in the text. "Collapsed into one letter" was then said beside an answer that looked like
    /// no answer at all, and a count of repeated characters that skipped spaces called that
    /// collapse no collapse.
    #[test]
    fn a_collapse_names_the_character_it_fell_onto_and_the_numbers_that_show_it() {
        assert_eq!(repeated(&" ".repeat(28)), Some(' '));
        assert_eq!(repeated("gggnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnnn"), Some('n'));
        assert_eq!(repeated(" need more truth knowledge."), None);
        assert_eq!(repeated("ddd"), None, "three characters are too few to call anything");

        let mut session = Session::new(&machine());
        session.go_to(STAGE_BREAK);
        session.ran = Some((ATTACK_RUNAWAY, BREAKING_MULTIPLIER));
        let topic = Topic::Attack {
            kind: ATTACK_RUNAWAY,
            multiplier: BREAKING_MULTIPLIER,
            lesson: Msg::BreakAfterRunaway,
            check: Check::Collapsed,
        };
        for completion in [" ".repeat(28), "d".repeat(28)] {
            session.latest = Some(TrainingSnapshot {
                loss: 1_475.97,
                loss_history: vec![3.487, 1_475.97, 49.2, 1_475.97],
                completion: completion.clone(),
                ..blank_snapshot()
            });
            for language in Language::ALL {
                let said = session.tell(topic, *language);
                assert!(said.starts_with(Msg::BreakAfterRunaway.text(*language)), "{said}");
                assert!(said.contains("3.487 → 1,476"), "the run's numbers are missing: {said}");
                if completion.starts_with(' ') {
                    assert!(said.ends_with(Msg::CharSpace.text(*language)), "{said}");
                } else {
                    assert!(said.contains("\u{2018}d\u{2019}"), "{said}");
                }
                assert!(said.chars().count() <= 160, "{language}: {said}");
                assert!(sentences(&said) <= 2, "{language}: {said}");
                assert!(!said.contains("NaN") && !said.contains("inf"), "{said}");
            }
        }
    }

    /// An answer of spaces after the prompt drew as the prompt and nothing, which reads as a
    /// model that said nothing rather than one that said the same character thirty times.
    #[test]
    fn an_answer_of_nothing_but_spaces_is_said_in_words() {
        for language in Language::ALL {
            let line = answer_line(&" ".repeat(30), *language);
            assert!(line.contains(Msg::AnswerOnlySpaces.text(*language)), "{line:?}");
            assert_eq!(answer_line(" need", *language), format!("{PROMPT} need"));
            assert_eq!(answer_line("", *language), PROMPT.to_string());
        }
    }
}
