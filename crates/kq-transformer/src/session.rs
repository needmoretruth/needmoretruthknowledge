//! The quest a reader has opened: five stages over one training run.
//!
//! The run lives here, on the session, and not on a stage — which is what lets a reader start
//! training, walk off to read the brief, and come back a minute later to find the loss forty
//! points lower. [`Session::tick`] copies the worker's latest numbers out and returns; it never
//! waits for a step and never takes one.

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::{char_width, pad, truncate, width as cells};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::widgets;
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
use Step::{Ask, Await, Run, Say};

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
    Await(Until::Steps(300)),
    Say(Msg::TrainAnswer),
    Say(Msg::TrainNoise),
    Await(Until::Finished),
    Say(Msg::TrainDone),
    Say(Msg::TrainGrid),
    Say(Msg::TrainGridRead),
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
    Say(Msg::SettingsSmaller),
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
    Say(Msg::BreakAfterRunaway),
    Say(Msg::BreakWarmupOne),
    Say(Msg::BreakWarmupTwo),
    Say(Msg::BreakWarmupThree),
    Ask(Msg::BreakAskWarmup),
    Run(Attack),
    Await(Until::Finished),
    Say(Msg::BreakAfterWarmup),
    Say(Msg::BreakSgdOne),
    Say(Msg::BreakSgdTwo),
    Say(Msg::BreakSgdThree),
    Ask(Msg::BreakAskSgd),
    Run(Attack),
    Await(Until::Finished),
    Say(Msg::BreakAfterSgd),
    Ask(Msg::BreakAskSgdAgain),
    Run(Attack),
    Await(Until::Finished),
    Say(Msg::BreakLesson),
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
    Started { weights: usize, steps: usize },
    Loss { loss: f32, step: usize },
    GotIt { step: usize },
    Ended { loss: f32, seconds: f64, fell: bool },
    Refused(Msg),
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
    /// The last run that finished with sensible settings.
    honest: Option<Finished>,
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
            honest: None,
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
        let label_width =
            labels.iter().map(|label| cells(label.text(language))).max().unwrap_or(0).max(12) + 2;
        let room = panel.saturating_sub(label_width + 2);
        knobs
            .iter()
            .enumerate()
            .map(|(index, knob)| {
                let picked = index == self.chosen;
                let label = labels.get(index).copied().unwrap_or(Msg::LabelWeights);
                let value = match knob.draft() {
                    Some(_) => knob.display(),
                    None => self.knob_value_text(index, knob, language),
                };
                Line::from(vec![
                    Span::styled(
                        format!("{} ", if picked { State::Chosen.mark() } else { " " }),
                        theme.state(State::Chosen),
                    ),
                    Span::styled(pad(label.text(language), label_width), theme.plain()),
                    Span::styled(
                        truncate(&value, room),
                        if picked { theme.heading() } else { theme.muted() },
                    ),
                ])
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
    fn presets_line(&self, language: Language, theme: Theme) -> Option<Line<'static>> {
        let text = match self.stage_knobs().get(self.chosen).map(|k| &k.value) {
            Some(KnobValue::Count { presets, .. }) => {
                presets.iter().map(|p| format::count(*p)).collect::<Vec<_>>().join("   ")
            }
            Some(KnobValue::Decimal { presets, .. }) => {
                presets.iter().map(|p| decimal_text(*p)).collect::<Vec<_>>().join("   ")
            }
            _ => String::new(),
        };
        if text.is_empty() {
            return None;
        }
        Some(Line::from(vec![
            Span::styled(format!("  {}  ", Msg::LabelPresets.text(language)), theme.muted()),
            Span::styled(text, theme.plain()),
        ]))
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
            return vec![Line::from(Span::styled(
                Msg::StatusPaused.text(language),
                theme.muted(),
            ))];
        };
        if snapshot.completion.is_empty() {
            return vec![
                Line::from(Span::styled(Msg::LabelAnswer.text(language), theme.muted())),
                Line::from(Span::styled(Msg::AnswerWaiting.text(language), theme.muted())),
            ];
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
        let mut lines = vec![Line::from(vec![
            Span::styled(format!("{} ", state.mark()), theme.state(state)),
            Span::styled(Msg::LabelAnswer.text(language), theme.muted()),
            Span::styled(format!("  {}", verdict.text(language)), theme.state(state)),
        ])];
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
            return vec![Line::from(Span::styled(
                Msg::AttentionWaiting.text(language),
                theme.muted(),
            ))];
        };
        let (layer, head) = self.viewed_head(attention);
        let mut lines = vec![Line::from(vec![
            Span::styled(format!("{}  ", Msg::AttentionTitle.text(language)), theme.heading()),
            Span::styled(
                format!(
                    "{} {} · {} {}",
                    Msg::LabelLayer.text(language),
                    layer + 1,
                    Msg::LabelHead.text(language),
                    head + 1
                ),
                theme.muted(),
            ),
        ])];
        // Two columns go to the row label, and the newest positions are the interesting ones.
        let columns = width.saturating_sub(2).min(attention.length);
        let rows = height.saturating_sub(3).min(attention.length);
        if columns == 0 || rows == 0 {
            return lines;
        }
        let first_key = attention.length - columns;
        let first_query = attention.length - rows;
        // Say when there is more than fits. A grid that silently drops half its rows reads as the
        // whole thing, and a reader who trusts it has been told something false.
        if rows < attention.length || columns < attention.length {
            lines.push(Line::from(Span::styled(
                format!(
                    "{} {rows} / {}   {}",
                    Msg::AttentionNewest.text(language),
                    attention.length,
                    Msg::AttentionMarks.text(language)
                ),
                theme.muted(),
            )));
        }
        let header: String =
            (first_key..attention.length).map(|k| visible(attention.tokens.get(k))).collect();
        lines.push(Line::from(Span::styled(format!("  {header}"), theme.muted())));
        for query in first_query..attention.length {
            let cells: String = (first_key..attention.length)
                .map(|key| shade(attention.weight(layer, head, query, key).unwrap_or(0.0)))
                .collect();
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", visible(attention.tokens.get(query))), theme.muted()),
                Span::styled(cells, theme.plain()),
            ]));
        }
        lines
    }

    /// One line saying what a finished run ended up at.
    fn result_line(
        &self,
        finished: &Finished,
        label: Msg,
        state: State,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        vec![
            Line::from(vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(pad(label.text(language), 14), theme.heading()),
                Span::styled(
                    format!(
                        "{} {}   {} {}",
                        Msg::LabelRate.text(language),
                        decimal_text(f64::from(finished.rate)),
                        Msg::LabelLoss.text(language),
                        loss_text(finished.snapshot.loss, language)
                    ),
                    theme.muted(),
                ),
            ]),
            Line::from(Span::styled(
                format!("  {PROMPT}{}", one_line(&finished.snapshot.completion)),
                theme.plain(),
            )),
        ]
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
        widgets::stats(frame, table, theme, &rows);
        frame.render_widget(
            Paragraph::new(Vec::<Line>::new()),
            footer,
        );
    }

    fn render_run(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        if self.latest.is_none() {
            let mut lines: Vec<Line> = Vec::new();
            if let Some(error) = self.refused {
                lines.extend(self.refusal_lines(error, language, theme));
            }
            frame.render_widget(Paragraph::new(lines), area);
            return;
        }
        let [stats, curve, answer, attention] = Layout::vertical([
            Constraint::Length(5),
            Constraint::Length(4),
            Constraint::Length(3),
            Constraint::Min(0),
        ])
        .areas(area);
        widgets::stats(frame, stats, theme, &self.stat_rows(language));
        self.render_curve(frame, curve, theme, language);
        frame.render_widget(
            Paragraph::new(self.answer_lines(answer.width as usize, language, theme)),
            answer,
        );
        frame.render_widget(
            Paragraph::new(self.attention_lines(
                attention.width as usize,
                attention.height as usize,
                language,
                theme,
            )),
            attention,
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
        let mut lines = self.knob_lines(&labels, area.width as usize, language, theme);
        lines.extend(self.presets_line(language, theme));
        lines.push(Line::from(""));
        match self.weights_now() {
            Ok(weights) => lines.push(Line::from(vec![
                Span::styled(pad(Msg::LabelWeightsNow.text(language), 20), theme.muted()),
                Span::styled(format::count(weights as u64), theme.heading()),
            ])),
            Err(message) => {
                lines.push(Line::from(Span::styled(Msg::ErrorTitle.text(language), theme.bad())));
                for chunk in wrap(message.text(language), area.width as usize) {
                    lines.push(Line::from(Span::styled(chunk, theme.plain())));
                }
            }
        }
        lines.push(Line::from(vec![
            Span::styled(pad(Msg::LabelMachineChose.text(language), 20), theme.muted()),
            Span::styled(
                format!(
                    "{}  ({} · {} · {})",
                    format::count(self.machine_default.parameter_count() as u64),
                    format::count(self.machine_default.layers as u64),
                    format::count(self.machine_default.heads as u64),
                    format::count(self.machine_default.d_model as u64)
                ),
                theme.plain(),
            ),
        ]));
        lines.push(Line::from(""));

        if let Some(error) = self.refused {
            lines.extend(self.refusal_lines(error, language, theme));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_break(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let labels = [Msg::LabelAttack, Msg::LabelMultiplier];
        let mut lines = self.knob_lines(&labels, area.width as usize, language, theme);
        lines.extend(self.presets_line(language, theme));
        lines.push(Line::from(""));

        let sensible = self.tuned();
        let attacked = self.attacked();
        lines.push(Line::from(vec![
            Span::styled(pad(Msg::LabelSensibleRate.text(language), 16), theme.muted()),
            Span::styled(decimal_text(f64::from(sensible.learning_rate)), theme.plain()),
        ]));
        let broken = attacked != sensible;
        lines.push(Line::from(vec![
            Span::styled(pad(Msg::LabelThisRun.text(language), 16), theme.muted()),
            Span::styled(
                decimal_text(f64::from(attacked.learning_rate)),
                if broken { theme.bad() } else { theme.good() },
            ),
        ]));
        lines.push(Line::from(""));

        if let Some(snapshot) = &self.latest {
            lines.push(Line::from(vec![
                Span::styled(pad(Msg::LabelStep.text(language), 16), theme.muted()),
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
                Span::styled(pad(Msg::LabelLoss.text(language), 16), theme.muted()),
                Span::styled(
                    loss_text(snapshot.loss, language),
                    if climbing { theme.bad() } else { theme.plain() },
                ),
            ]));
            if !snapshot.loss.is_finite() && snapshot.step > 0 {
                lines.push(Line::from(Span::styled(Msg::BreakBlewUp.text(language), theme.bad())));
            } else if climbing {
                lines
                    .push(Line::from(Span::styled(Msg::BreakClimbing.text(language), theme.bad())));
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
                        language,
                        theme,
                    ));
                    lines.extend(self.result_line(
                        wrecked,
                        Msg::LabelBrokenRun,
                        State::Bad,
                        language,
                        theme,
                    ));
                }
                _ => lines.extend(self.answer_lines(area.width as usize, language, theme)),
            }
        }
        if let Some(error) = self.refused {
            lines.extend(self.refusal_lines(error, language, theme));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn render_recap(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        // The honest run is the one the recap is about. A reader who only ever broke it gets the
        // broken run instead, described the same way — it is still what happened.
        let finished = self.honest.as_ref().or(self.broken.as_ref());
        let Some(finished) = finished else {
            frame.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    Msg::RecapNothing.text(language),
                    theme.muted(),
                ))),
                area,
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
                        "{} -> {}",
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
        let label_width = rows
            .iter()
            .map(|(name, _)| cells(name))
            .chain([cells(Msg::RecapAnswer.text(language))])
            .max()
            .unwrap_or(0)
            + 2;
        let indent = area.width.saturating_sub(4) as usize;
        let mut lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .map(|(index, (name, value))| {
                Line::from(vec![
                    Span::styled(format!("{} ", index + 1), theme.muted()),
                    Span::styled(pad(name, label_width), theme.plain()),
                    Span::styled(value.clone(), theme.heading()),
                ])
            })
            .collect();
        let right = snapshot.completion.trim_start().starts_with(EXPANSION);
        let state = if right { State::Good } else { State::Bad };
        lines.push(Line::from(vec![
            Span::styled(format!("{} ", rows.len() + 1), theme.muted()),
            Span::styled(pad(Msg::RecapAnswer.text(language), label_width), theme.plain()),
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
                Span::styled(Msg::RecapBroken.text(language).to_string(), theme.plain()),
            ]));
            lines.push(Line::from(Span::styled(
                format!(
                    "    {} {}   {} {}",
                    Msg::LabelRate.text(language),
                    decimal_text(f64::from(broken.rate)),
                    Msg::LabelLoss.text(language),
                    loss_text(broken.snapshot.loss, language)
                ),
                theme.bad(),
            )));
            for chunk in wrap(&format!("{PROMPT}{}", one_line(&broken.snapshot.completion)), indent)
            {
                lines.push(Line::from(Span::styled(format!("    {chunk}"), theme.plain())));
            }
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// A sentence of guidance, wrapped to the panel.
    fn refusal_lines(
        &self,
        error: StartError,
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
            wrap(phrases::start_error(error).text(language), 44)
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
    fn advance(&mut self) -> bool {
        let script = self.script();
        if self.revealed + 1 >= script.len() {
            return false;
        }
        self.revealed += 1;
        if let Run(deed) = script[self.revealed] {
            self.forget_run();
            let config = match deed {
                Train => self.tuned(),
                Attack => self.attacked(),
            };
            // Always a fresh run. The old version answered Enter during training by doing nothing
            // at all, so a reader who changed a value and pressed Enter watched the old model
            // finish and concluded the key was broken.
            self.start(config);
            match self.refused {
                Some(error) => {
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
        while self.milestone < MILESTONES.len() && loss.is_finite() && loss < MILESTONES[self.milestone] {
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
                text.push_str(&format!(
                    "  ·  {} {seconds:.0}s",
                    Msg::EventSeconds.text(language)
                ));
                if !*fell {
                    text.push_str(&format!("  ·  {}", Msg::EventNeverFell.text(language)));
                }
                Beat::outcome(if *fell { State::Good } else { State::Bad }, text)
            }
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
                if let Some(Await(until)) = self.script().get(self.revealed)
                    && !self.satisfied(*until)
                {
                    return Reaction::Ignored;
                }
                // The end of a stage is not the end of the quest, but walking on from here is
                // the shell's business: it is what knows there is another stage to walk to.
                if self.advance() { Reaction::Handled } else { Reaction::Ignored }
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
            Action::Commit => match self.stage_knobs_mut() {
                Some(knob) => {
                    knob.commit();
                    Reaction::Handled
                }
                None => Reaction::Ignored,
            },
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
        assert!(screen.contains("3.310 -> 0.412"), "no loss from end to end:\n{screen}");
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
