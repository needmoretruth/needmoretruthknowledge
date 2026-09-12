//! The quest a reader has opened: five stages over three ledgers.

use nmtk_core::{Language, format};
use nmtk_kq::knob::{Knob, KnobValue};
use nmtk_kq::meta::StageKind;
use nmtk_kq::session::{Action, KqSession, Reaction, RunState};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::widgets;
use nmtk_ledger::{
    Address, Amount, ApplyOutcome, CoinChoice, DoubleSpendReport, Genesis, Key, Model, Scenario,
    SideBySide, TransferRequest,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::phrases::{self, Msg};

/// Amounts worth trying. A reader who wants 17 types 17.
const AMOUNT_PRESETS: &[u64] = &[5, 10, 20, 30];

/// Alice's opening coins. Three of them, so there is something to choose between.
const OPENING_COINS: [Amount; 3] = [10, 10, 10];

/// What the shared pool starts with.
const POOL_BALANCE: Amount = 100;

pub struct Session {
    stage: StageKind,
    scenario: Scenario,
    knobs: Vec<Knob>,
    chosen: usize,
    alice: Key,
    bob: Address,
    carol: Address,
    pool: Address,
    transfer: Option<SideBySide<ApplyOutcome>>,
    double: Option<SideBySide<DoubleSpendReport>>,
    state: RunState,
}

impl Session {
    pub fn new() -> Self {
        let alice = Key::from_seed(b"nmtk.kq.ledgers.alice");
        let bob = Address::from_seed(b"nmtk.kq.ledgers.bob");
        let carol = Address::from_seed(b"nmtk.kq.ledgers.carol");
        let pool = Address::from_seed(b"nmtk.kq.ledgers.pool");
        let genesis =
            Genesis::new().holding(alice.address(), &OPENING_COINS).shared_pool(pool, POOL_BALANCE);
        Self {
            stage: StageKind::Brief,
            scenario: Scenario::new(&genesis),
            knobs: vec![
                Knob::new(
                    "amount",
                    KnobValue::Count {
                        current: 10,
                        min: 1,
                        max: 30,
                        step: 1,
                        presets: AMOUNT_PRESETS,
                    },
                ),
                Knob::new("coin", KnobValue::Choice { current: 0, count: 4 }),
                Knob::new("recipient", KnobValue::Choice { current: 0, count: 3 }),
            ],
            chosen: 0,
            alice,
            bob,
            carol,
            pool,
            transfer: None,
            double: None,
            state: RunState::Idle,
        }
    }

    /// Throws the ledgers away and opens fresh ones. The knobs keep their values: a reader who
    /// set an amount does not want it taken back.
    fn reset(&mut self) {
        let genesis = Genesis::new()
            .holding(self.alice.address(), &OPENING_COINS)
            .shared_pool(self.pool, POOL_BALANCE);
        self.scenario = Scenario::new(&genesis);
        self.transfer = None;
        self.double = None;
        self.state = RunState::Idle;
    }

    fn amount(&self) -> Amount {
        match self.knobs[0].value {
            KnobValue::Count { current, .. } => current,
            _ => 10,
        }
    }

    fn coin(&self) -> CoinChoice {
        match self.knobs[1].value {
            KnobValue::Choice { current: 0, .. } => CoinChoice::Automatic,
            KnobValue::Choice { current, .. } => CoinChoice::Index(current - 1),
            _ => CoinChoice::Automatic,
        }
    }

    fn recipient(&self) -> Address {
        match self.knobs[2].value {
            KnobValue::Choice { current: 1, .. } => self.carol,
            KnobValue::Choice { current: 2, .. } => self.pool,
            _ => self.bob,
        }
    }

    fn recipient_name(&self) -> Msg {
        match self.knobs[2].value {
            KnobValue::Choice { current: 1, .. } => Msg::PartyCarol,
            KnobValue::Choice { current: 2, .. } => Msg::PartyPool,
            _ => Msg::PartyBob,
        }
    }

    fn request(&self) -> TransferRequest {
        TransferRequest::new(self.alice, self.recipient(), self.amount()).with_coin(self.coin())
    }

    /// Sends the transfer into all three ledgers.
    fn send(&mut self) {
        let request = self.request();
        self.transfer = Some(self.scenario.transfer(&request));
        self.state = RunState::Done;
    }

    /// Writes two transfers against the same state and sends both.
    fn spend_twice(&mut self) {
        let first = self.request();
        let second = TransferRequest::new(self.alice, self.carol, self.amount())
            .with_coin(self.coin());
        self.double = Some(self.scenario.double_spend(&first, &second));
        self.state = RunState::Done;
    }

    fn knob_count(&self) -> usize {
        if self.stage == StageKind::Tune { self.knobs.len() } else { 0 }
    }

    fn move_choice(&mut self, step: i32) {
        let count = self.knob_count();
        if count == 0 {
            return;
        }
        let last = count - 1;
        self.chosen = match step {
            s if s > 0 => {
                if self.chosen >= last {
                    0
                } else {
                    self.chosen + 1
                }
            }
            _ => {
                if self.chosen == 0 {
                    last
                } else {
                    self.chosen - 1
                }
            }
        };
    }

    /// One row per model: what it is, how big it is, and what the last transfer cost it.
    fn model_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let facts = self.scenario.facts();
        let mut lines = Vec::new();
        for model in Model::ALL {
            let fact = facts.get(model);
            lines.push(Line::from(vec![
                Span::styled(
                    pad(phrases::model(model).text(language), 10),
                    theme.heading(),
                ),
                Span::styled(
                    phrases::entry_kind(fact.entry_kind).text(language).to_string(),
                    theme.muted(),
                ),
            ]));
            lines.push(Line::from(vec![
                Span::styled("  ", theme.plain()),
                Span::styled(
                    format!(
                        "{} {}   {} {}",
                        format::count(fact.entry_count as u64),
                        Msg::ColumnEntries.text(language),
                        format::bytes(fact.state_size_bytes),
                        "",
                    ),
                    theme.plain(),
                ),
            ]));
            if let Some(outcome) = self.transfer.as_ref().map(|t| t.get(model)) {
                lines.push(outcome_line(outcome, language, theme));
            }
            lines.push(Line::from(""));
        }
        lines
    }

    /// One row per model saying where the second spend died.
    fn double_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let Some(reports) = &self.double else { return Vec::new() };
        let mut lines = Vec::new();
        for model in Model::ALL {
            let report = reports.get(model);
            let stopped = report.stopped();
            let state = if stopped { State::Good } else { State::Bad };
            let mut spans = vec![
                Span::styled(format!("{} ", state.mark()), theme.state(state)),
                Span::styled(pad(phrases::model(model).text(language), 10), theme.heading()),
                Span::styled(
                    if stopped { Msg::BreakStopped } else { Msg::BreakNotStopped }
                        .text(language)
                        .to_string(),
                    theme.state(state),
                ),
            ];
            if let (Some(step), Some(kind)) = (report.stopped_at(), report.stopped_by()) {
                spans.push(Span::styled(
                    format!("  {}: ", phrases::step(step).text(language)),
                    theme.muted(),
                ));
                spans.push(Span::styled(
                    phrases::why(kind).text(language).to_string(),
                    theme.plain(),
                ));
            }
            lines.push(Line::from(spans));
            lines.push(Line::from(""));
        }
        lines
    }

    /// The knobs, with the chosen one marked.
    fn knob_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let labels = [Msg::KnobAmount, Msg::KnobCoin, Msg::KnobRecipient];
        self.knobs
            .iter()
            .enumerate()
            .map(|(index, knob)| {
                let picked = index == self.chosen;
                let marker = if picked { State::Chosen.mark() } else { " " };
                let value = match index {
                    1 => self.coin_text(language).to_string(),
                    2 => phrases::Msg::text(self.recipient_name(), language).to_string(),
                    _ => knob.display(),
                };
                let value = if knob.draft().is_some() { knob.display() } else { value };
                Line::from(vec![
                    Span::styled(format!("{marker} "), theme.state(State::Chosen)),
                    Span::styled(pad(labels[index].text(language), 16), theme.plain()),
                    Span::styled(value, if picked { theme.heading() } else { theme.muted() }),
                ])
            })
            .collect()
    }

    fn coin_text(&self, language: Language) -> &'static str {
        match self.knobs[1].value {
            KnobValue::Choice { current: 1, .. } => Msg::CoinFirst.text(language),
            KnobValue::Choice { current: 2, .. } => Msg::CoinSecond.text(language),
            KnobValue::Choice { current: 3, .. } => Msg::CoinThird.text(language),
            _ => Msg::CoinAutomatic.text(language),
        }
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
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
                StageKind::Run | StageKind::Tune => {
                    self.send();
                    Reaction::Handled
                }
                StageKind::Break => {
                    self.spend_twice();
                    Reaction::Handled
                }
                _ => Reaction::Ignored,
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
            Action::PauseOrResume => Reaction::Ignored,
        }
    }

    fn tick(&mut self) {}

    fn run_state(&self) -> RunState {
        self.state
    }

    fn explain(&self, language: Language) -> Vec<Line<'static>> {
        let paragraphs: &[Msg] = match self.stage {
            StageKind::Brief => &[
                Msg::BriefOpening,
                Msg::BriefUtxo,
                Msg::BriefAccount,
                Msg::BriefObject,
                Msg::BriefPromise,
            ],
            StageKind::Run => &[Msg::RunOpening, Msg::RunAfter],
            StageKind::Tune => &[Msg::TuneHint, Msg::TuneNote],
            StageKind::Break => &[Msg::BreakExplain],
            StageKind::Recap => &[Msg::RecapOne, Msg::RecapTwo, Msg::RecapThree],
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
            StageKind::Brief => Msg::Title,
            StageKind::Run => Msg::RunTitle,
            StageKind::Tune => Msg::TuneTitle,
            StageKind::Break => Msg::BreakTitle,
            StageKind::Recap => Msg::RecapTitle,
        };
        let block = theme.titled_panel(title.text(language));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        match self.stage {
            StageKind::Brief | StageKind::Run => {
                let lines = self.model_lines(language, theme);
                let hint = if self.transfer.is_none() { Msg::RunIdle } else { Msg::LabelAccepted };
                let [rows, footer] =
                    Layout::vertical([Constraint::Min(1), Constraint::Length(1)]).areas(inner);
                frame.render_widget(Paragraph::new(lines), rows);
                if self.transfer.is_none() {
                    frame.render_widget(
                        Paragraph::new(Line::from(Span::styled(
                            hint.text(language),
                            theme.muted(),
                        ))),
                        footer,
                    );
                }
            }
            StageKind::Tune => {
                let [knobs, rows] =
                    Layout::vertical([Constraint::Length(4), Constraint::Min(1)]).areas(inner);
                frame.render_widget(Paragraph::new(self.knob_lines(language, theme)), knobs);
                frame.render_widget(Paragraph::new(self.model_lines(language, theme)), rows);
            }
            StageKind::Break => {
                let lines = if self.double.is_some() {
                    self.double_lines(language, theme)
                } else {
                    vec![Line::from(Span::styled(Msg::BreakHint.text(language), theme.muted()))]
                };
                frame.render_widget(Paragraph::new(lines), inner);
            }
            StageKind::Recap => {
                let facts = self.scenario.facts();
                let rows: Vec<(&str, String)> = vec![
                    (Msg::RecapFromOne.text(language), yes_no(&facts, language, true)),
                    (Msg::RecapToOne.text(language), yes_no(&facts, language, false)),
                ];
                let [heading, table] =
                    Layout::vertical([Constraint::Length(2), Constraint::Min(1)]).areas(inner);
                frame.render_widget(
                    Paragraph::new(Line::from(Span::styled(
                        Msg::RecapParallel.text(language),
                        theme.heading(),
                    ))),
                    heading,
                );
                widgets::stats(frame, table, theme, &rows);
            }
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            StageKind::Tune => vec![("←→", Msg::KnobAmount.text(language))],
            _ => Vec::new(),
        }
    }

    fn close(&mut self) {}
}

/// `name` padded to `width` columns, counting characters rather than bytes.
fn pad(text: &str, width: usize) -> String {
    let used = text.chars().count();
    let mut out = text.to_string();
    for _ in used..width {
        out.push(' ');
    }
    out
}

fn outcome_line(outcome: &ApplyOutcome, language: Language, theme: Theme) -> Line<'static> {
    let accepted = outcome.accepted();
    let state = if accepted { State::Good } else { State::Bad };
    let mut spans = vec![
        Span::styled(format!("  {} ", state.mark()), theme.state(state)),
        Span::styled(
            if accepted { Msg::LabelAccepted } else { Msg::LabelRejected }.text(language).to_string(),
            theme.state(state),
        ),
    ];
    if accepted {
        let delta = outcome.size_delta();
        spans.push(Span::styled(
            format!(
                "   {} {:+}   {} {:+} B",
                Msg::ColumnEntries.text(language),
                outcome.entry_delta(),
                Msg::ColumnGrowth.text(language),
                delta
            ),
            theme.muted(),
        ));
    } else if let Some((step, reason)) = outcome.rejection() {
        spans.push(Span::styled(
            format!(
                "   {}: {}",
                phrases::step(step).text(language),
                phrases::why(reason.kind()).text(language)
            ),
            theme.muted(),
        ));
    }
    Line::from(spans)
}

/// Which models let two transfers run at once.
fn yes_no(
    facts: &SideBySide<nmtk_ledger::ModelFacts>,
    language: Language,
    from_one_sender: bool,
) -> String {
    Model::ALL
        .iter()
        .map(|model| {
            let fact = facts.get(*model);
            let parallel = if from_one_sender {
                fact.parallel_from_one_sender
            } else {
                fact.parallel_to_one_recipient
            };
            format!(
                "{} {}",
                phrases::model(*model).text(language),
                if parallel { Msg::Yes } else { Msg::No }.text(language)
            )
        })
        .collect::<Vec<_>>()
        .join("   ")
}
