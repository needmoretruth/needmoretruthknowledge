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

/// Five shades, darkest last. Attention is structure, so it is drawn in black and white; the four
/// state colours are kept for state.
const SHADES: [&str; 5] = [" ", "░", "▒", "▓", "█"];

/// Where each stage sits. The order here is the order `lib.rs` declares them in.
#[cfg(test)]
const STAGE_QUESTION: usize = 0;
#[cfg(test)]
const STAGE_PIECES: usize = 1;
const STAGE_TRAIN: usize = 2;
const STAGE_TUNE: usize = 3;
const STAGE_BREAK: usize = 4;
const STAGE_RECAP: usize = 5;

/// The most events one stage reports. A loss falls continuously; a conversation does not.
const EVENT_CAP: usize = 24;

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
}

/// What a [`Step::Tell`] is about.
#[derive(Debug, Clone, Copy)]
enum Topic {
    /// Whether the loss is still falling or has flattened out.
    Progress,
    /// How this run compares with the one before it.
    Settings,
    /// The lesson of one attack, said only if that attack is what ran.
    Attack { kind: usize, multiplier: f64, lesson: Msg },
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
use Step::{Ask, Await, Run, Say, Tell};

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
    Ask(Msg::SettingsAsk),
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
    }),
    Say(Msg::BreakWarmupOne),
    Say(Msg::BreakGradient),
    Say(Msg::BreakWarmupTwo),
    Say(Msg::BreakWarmupThree),
    Ask(Msg::BreakAskWarmup),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack { kind: ATTACK_NO_WARMUP, multiplier: 10.0, lesson: Msg::BreakAfterWarmup }),
    Say(Msg::BreakSgdOne),
    Say(Msg::BreakSgdName),
    Say(Msg::BreakSgdTwo),
    Say(Msg::BreakSgdThree),
    Say(Msg::BreakAdamName),
    Ask(Msg::BreakAskSgd),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack { kind: ATTACK_PLAIN_SGD, multiplier: 1.0, lesson: Msg::BreakAfterSgd }),
    Ask(Msg::BreakAskSgdAgain),
    Run(Attack),
    Await(Until::Finished),
    Tell(Topic::Attack { kind: ATTACK_PLAIN_SGD, multiplier: 100.0, lesson: Msg::BreakLesson }),
];

/// What the reader now knows, beside the numbers they made.
const RECAP: &[Step] = &[
    Say(Msg::RecapOne),
    Say(Msg::RecapTwo),
    Say(Msg::RecapThree),
    Say(Msg::RecapFour),
    Say(Msg::RecapFive),
    Say(Msg::RecapSix),
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
        to: String,
    },
    /// A typed number the knob could not read at all.
    NotANumber,
}

/// One happening, filed against the step the reader was on when it happened.
struct Logged {
    step: usize,
    what: Happening,
}

/// A run the reader started, and whether its settings were the sensible ones.
struct Finished {
    snapshot: TrainingSnapshot,
    rate: f32,
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

    /// Stops the worker thread and forgets the run in progress. Finished runs are kept, because
    /// the recap is about them.
    fn stop(&mut self) {
        // Dropping the handle asks the worker to stop and waits for it, which costs at most one
        // training step.
        self.handle = None;
        self.latest = None;
        self.running_config = None;
        self.running_broken = false;
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
            STAGE_BREAK if index == BREAK_ATTACK => match choice_of(knob) {
                ATTACK_NO_WARMUP => Msg::AttackNoWarmup.text(language).to_string(),
                ATTACK_PLAIN_SGD => Msg::AttackPlainSgd.text(language).to_string(),
                _ => Msg::AttackRunaway.text(language).to_string(),
            },
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
            lines.extend(wrapped(
                "",
                Msg::AnswerWaiting.text(language),
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
        // of this row that cannot be cut: when the two will not share a line, it takes its own.
        let mut lines = if cells(title) + 2 + cells(&seen) <= width {
            vec![Line::from(vec![
                Span::styled(format!("{title}  "), theme.heading()),
                Span::styled(seen, theme.muted()),
            ])]
        } else {
            let mut split = wrapped("", title, width, theme.heading(), theme.heading());
            split.extend(wrapped("  ", &seen, width, theme.muted(), theme.muted()));
            split
        };
        // The grid is square — every position against every position — so the window on to it is
        // square too. The line above it carries one number, and one number can only be true of the
        // characters across the top and of the rows down the side at once when there are as many
        // of one as of the other. Two columns go to the row label; the newest positions are the
        // interesting ones.
        let mut shown = width.saturating_sub(2).min(attention.length);
        let mut legend = self.grid_legend(shown, attention, width, language, theme);
        // How tall that line is depends on the number in it, and shrinking the window never
        // lengthens that number, so settling the two against each other takes one pass.
        for _ in 0..2 {
            let room = height.saturating_sub(lines.len() + legend.len() + 1);
            if shown <= room {
                break;
            }
            shown = room;
            legend = self.grid_legend(shown, attention, width, language, theme);
        }
        if shown == 0 {
            // A heading over a box with no row in it promises a grid that never arrives, and a
            // reader who started a run watches the empty box for the whole of it.
            return Vec::new();
        }
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

    /// The line above the grid that says how much of it is on screen, and how the characters that
    /// stand in for a space and a line break are drawn. Nothing at all when the whole grid fits.
    fn grid_legend(
        &self,
        shown: usize,
        attention: &AttentionSnapshot,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        if shown >= attention.length {
            return Vec::new();
        }
        wrapped(
            "",
            &format!(
                "{} {shown} / {}   {}",
                Msg::AttentionNewest.text(language),
                attention.length,
                Msg::AttentionMarks.text(language)
            ),
            width,
            theme.muted(),
            theme.muted(),
        )
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
        for chunk in wrap(
            &format!("{PROMPT}{}", one_line(&finished.snapshot.completion)),
            width.saturating_sub(2),
        ) {
            lines.push(Line::from(Span::styled(format!("  {chunk}"), theme.plain())));
        }
        lines
    }

    // ---- drawing -------------------------------------------------------------

    fn render_brief(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let config = self.machine_default;
        let rows = vec![
            (Msg::LabelVocabulary.text(language), format::count(self.vocabulary as u64)),
            (Msg::LabelLayers.text(language), format::count(config.layers as u64)),
            (Msg::LabelHeads.text(language), format::count(config.heads as u64)),
            (Msg::LabelWidth.text(language), format::count(config.d_model as u64)),
            (Msg::LabelContext.text(language), format::count(config.context as u64)),
            (Msg::LabelWeights.text(language), format::count(config.parameter_count() as u64)),
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
        let mut lines = self.knob_lines(&labels, width, language, theme);
        lines.push(Line::from(""));
        let chose = format!(
            "{}  ({} · {} · {})",
            format::count(self.machine_default.parameter_count() as u64),
            format::count(self.machine_default.layers as u64),
            format::count(self.machine_default.heads as u64),
            format::count(self.machine_default.d_model as u64)
        );
        match self.weights_now() {
            Ok(weights) => lines.extend(stat_lines(
                &[
                    (Msg::LabelWeightsNow.text(language), format::count(weights as u64)),
                    (Msg::LabelMachineChose.text(language), chose),
                ],
                width,
                theme,
            )),
            Err(message) => {
                lines.extend(wrapped(
                    "",
                    Msg::ErrorTitle.text(language),
                    width,
                    theme.bad(),
                    theme.bad(),
                ));
                lines.extend(wrapped(
                    "",
                    message.text(language),
                    width,
                    theme.plain(),
                    theme.plain(),
                ));
                lines.extend(stat_lines(
                    &[(Msg::LabelMachineChose.text(language), chose)],
                    width,
                    theme,
                ));
            }
        }
        lines.push(Line::from(""));

        if let Some(error) = self.refused {
            lines.extend(self.refusal_lines(error, width, language, theme));
        }
        self.draw_with_the_run(frame, area, lines, theme, language);
    }

    /// The settings, and under them whatever room is left given to the run itself.
    ///
    /// Without this the tuning stage showed a list of values and nothing else while a
    /// twenty-six-second training ran, so the reader had started something with nothing to watch.
    fn draw_with_the_run(
        &self,
        frame: &mut Frame,
        area: Rect,
        lines: Vec<Line<'static>>,
        theme: Theme,
        language: Language,
    ) {
        /// Rows the run needs before it is worth drawing rather than crowding the settings out.
        const ROOM_FOR_THE_RUN: u16 = 12;
        let used = lines.len() as u16;
        if self.latest.is_none() || area.height < used + ROOM_FOR_THE_RUN {
            paragraph(frame, area, lines);
            return;
        }
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
        // The honest run is the one the recap is about. A reader who only ever broke it gets the
        // broken run instead, described the same way — it is still what happened.
        let finished = self.honest.as_ref().or(self.broken.as_ref());
        let Some(finished) = finished else {
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
        let snapshot = &finished.snapshot;
        let first = snapshot.loss_history.iter().copied().find(|v| v.is_finite());
        let rows: Vec<(&'static str, String)> = vec![
            (Msg::RecapWeights.text(language), format::count(snapshot.parameter_count as u64)),
            (
                Msg::RecapSteps.text(language),
                format!(
                    "{} / {}",
                    format::count(snapshot.step as u64),
                    format::count(snapshot.total_steps as u64)
                ),
            ),
            (
                Msg::RecapLoss.text(language),
                match first {
                    Some(start) => format!(
                        "{} → {}",
                        loss_text(start, language),
                        loss_text(snapshot.loss, language)
                    ),
                    None => loss_text(snapshot.loss, language),
                },
            ),
            (Msg::RecapTime.text(language), format::duration(snapshot.elapsed)),
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
        let right = snapshot.completion.trim_start().starts_with(EXPANSION);
        let state = if right { State::Good } else { State::Bad };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", rows.len() + 1), theme.muted()),
            Span::styled(column(Msg::RecapAnswer.text(language), label_width), theme.plain()),
            Span::styled(state.mark().to_string(), theme.state(state)),
        ]));
        for chunk in wrap(&format!("{PROMPT}{}", one_line(&snapshot.completion)), indent) {
            lines.push(Line::from(Span::styled(format!("    {chunk}"), theme.heading())));
        }
        if let (Some(_), Some(broken)) = (&self.honest, &self.broken) {
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
            for chunk in wrap(&format!("{PROMPT}{}", one_line(&broken.snapshot.completion)), indent)
            {
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
        self.forget_run();
        self.refused = None;
    }

    /// Whether the reader has turned a knob since the run in progress started.
    fn knobs_moved(&self) -> bool {
        match &self.running_with {
            Some(settled) => !settled.still(self.knobs()),
            None => false,
        }
    }

    /// Runs this stage's work again with the values now on screen, in place of the run before it,
    /// so the sentence that reads the result is about the run that just happened.
    fn rerun(&mut self) -> Reaction {
        if self.knobs().is_empty() {
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
        match until {
            Until::Steps(wanted) => self.latest.as_ref().is_some_and(|s| s.step >= wanted),
            // A refused run never finishes, so a refusal has to end the wait too, or the
            // conversation stops for good behind a message nobody can press past.
            Until::Finished => {
                self.refused.is_some()
                    || self.latest.as_ref().is_some_and(|s| s.state == TrainingState::Finished)
            }
        }
    }

    /// Reveals the next step and starts whatever it asks for.
    /// A sentence about the run that just finished, built from its own numbers.
    fn tell(&self, topic: Topic, language: Language) -> String {
        match topic {
            Topic::Progress => self.tell_progress(language),
            Topic::Settings => self.tell_settings(language),
            Topic::Attack { kind, multiplier, lesson } => {
                // The lesson is only true of the run it was written about. A reader who pressed
                // Enter without touching the values ran something else, and used to be told the
                // conclusion anyway.
                let asked = self
                    .ran
                    .is_some_and(|(k, m)| k == kind && (m - multiplier).abs() < multiplier * 0.01);
                if asked {
                    return lesson.text(language).to_string();
                }
                // The reader ran something else. Saying "the line is skipped" leaves the screen
                // with a run on it and nothing about that run; this reads the run instead.
                let mut text = Msg::BreakNotThatRun.text(language).to_string();
                if let Some(history) = self.latest.as_ref().map(|s| s.loss_history.as_slice())
                    && let (Some(first), Some(last)) = (history.first(), history.last())
                {
                    text.push_str(&format!(
                        "  ·  {} {:.3} → {:.3}",
                        Msg::BreakRanAt.text(language),
                        first,
                        last
                    ));
                    let learned = last < first;
                    text.push(' ');
                    text.push_str(
                        if learned { Msg::BreakItLearned } else { Msg::BreakItDidNot }
                            .text(language),
                    );
                }
                text
            }
        }
    }

    /// Whether the loss is still falling, measured rather than assumed.
    ///
    /// The last quarter of the history is held against the first: a run that has flattened has
    /// given up most of its fall already, and a run that has not is still on its way down.
    fn tell_progress(&self, language: Language) -> String {
        let history = self.latest.as_ref().map(|s| s.loss_history.as_slice()).unwrap_or(&[]);
        if history.len() < 8 {
            return Msg::TrainStillFalling.text(language).to_string();
        }
        let quarter = (history.len() / 4).max(1);
        let early = history[..quarter].iter().sum::<f32>() / quarter as f32;
        let late = history[history.len() - quarter..].iter().sum::<f32>() / quarter as f32;
        let mid = history[quarter..history.len() - quarter].to_vec();
        let middle = if mid.is_empty() { late } else { mid.iter().sum::<f32>() / mid.len() as f32 };
        // Flat when the latest stretch has given up far less than the run did getting here.
        let whole = (early - late).abs();
        let recent = (middle - late).abs();
        let message = if whole > 0.0 && recent < whole * 0.2 {
            Msg::TrainSlowing
        } else {
            Msg::TrainStillFalling
        };
        message.text(language).to_string()
    }

    /// This run held against the one before it: what the reader changed, and what it cost.
    fn tell_settings(&self, language: Language) -> String {
        let (Some(now), Some(before)) = (self.honest.as_ref(), self.previous.as_ref()) else {
            return Msg::SettingsFirstRun.text(language).to_string();
        };
        let (weights, was) = (now.snapshot.parameter_count, before.snapshot.parameter_count);
        let (loss, loss_was) = (now.snapshot.loss, before.snapshot.loss);
        let verdict = if weights == was {
            Msg::SettingsSameShape
        } else if weights < was {
            if loss > loss_was { Msg::SettingsSmallerWorse } else { Msg::SettingsSmallerBetter }
        } else if loss < loss_was {
            Msg::SettingsBiggerBetter
        } else {
            Msg::SettingsBiggerWorse
        };
        format!(
            "{} {} {} → {}, {} {:.3} → {:.3}.",
            verdict.text(language),
            Msg::LabelWeights.text(language),
            format::count(was as u64),
            format::count(weights as u64),
            Msg::LabelLoss.text(language),
            loss_was,
            loss,
        )
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
        true
    }

    fn say(&mut self, what: Happening) {
        if self.log.len() < EVENT_CAP {
            self.log.push(Logged { step: self.revealed, what });
        }
    }

    /// Reads the run and turns anything new into beats.
    fn notice(&mut self) {
        let Some(snapshot) = &self.latest else { return };
        let step = snapshot.step;
        let loss = snapshot.loss;
        let finished = snapshot.state == TrainingState::Finished;
        let right = snapshot.completion.trim_start().starts_with(EXPANSION);
        let seconds = snapshot.elapsed.as_secs_f64();

        if self.first_loss.is_none() && loss.is_finite() && step > 0 {
            self.first_loss = Some(loss);
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
            let fell = match self.first_loss {
                Some(first) => loss.is_finite() && loss < first,
                None => false,
            };
            self.say(Happening::Ended { loss, seconds, fell });
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
                "{} {loss:.3}  ·  {} {}",
                Msg::EventLoss.text(language),
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
                    if loss.is_finite() {
                        format!("{loss:.3}")
                    } else {
                        Msg::NotANumber.text(language).to_string()
                    },
                );
                text.push_str(&format!("  ·  {} {seconds:.0}s", Msg::EventSeconds.text(language)));
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
                    to,
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
                if self.knobs_moved() && self.rerun() == Reaction::Handled {
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
                // A number that goes nowhere reads as a broken key unless the screen says what
                // happened to it.
                match knob.commit() {
                    Typed::PulledIn { to } => self.say(Happening::PulledIn { to }),
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
            let rate = self.running_config.map_or(0.0, |c| c.learning_rate);
            let broken = self.running_broken;
            self.latest = Some(snapshot);
            if done {
                // The worker has left; joining it costs nothing and gives the weights back.
                self.handle = None;
                if let Some(snapshot) = self.latest.clone() {
                    let finished = Finished { snapshot, rate };
                    if broken {
                        self.broken = Some(finished);
                    } else {
                        self.previous = self.honest.take();
                        self.honest = Some(finished);
                    }
                }
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
        let runnable = !self.knobs().is_empty() && (self.at_end() || self.knobs_moved());
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
    if value.is_finite() {
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
        training.close();
        assert!(
            run.contains("layer 1 · head 1"),
            "the reader cannot see which head the arrow keys are on:\n{run}"
        );
        assert!(run.contains("a line break /"), "the legend for the attention grid is cut:\n{run}");

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
        let topic = Topic::Attack { kind: ATTACK_PLAIN_SGD, multiplier: 1.0, lesson };

        session.ran = None;
        assert_eq!(
            session.tell(topic, Language::ENGLISH),
            Msg::BreakNotThatRun.text(Language::ENGLISH)
        );

        // The runaway attack at its own multiplier is not plain SGD at one.
        session.ran = Some((ATTACK_RUNAWAY, BREAKING_MULTIPLIER));
        assert_eq!(
            session.tell(topic, Language::ENGLISH),
            Msg::BreakNotThatRun.text(Language::ENGLISH)
        );

        session.ran = Some((ATTACK_PLAIN_SGD, 1.0));
        assert_eq!(session.tell(topic, Language::ENGLISH), lesson.text(Language::ENGLISH));
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
        session.honest = Some(Finished {
            snapshot: session.latest.clone().expect("a run part way through"),
            rate: 0.003,
        });
        let finished = Finished {
            snapshot: session.latest.clone().expect("a run part way through"),
            rate: 0.003,
        };
        session.go_to(STAGE_RECAP);
        session.honest = Some(finished);
        let screen = draw(&session, 80, 24);
        assert!(screen.contains("3.310 → 0.412"), "no loss from end to end:\n{screen}");
        assert!(screen.contains("need more truth knowledge"), "no answer:\n{screen}");
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
}
