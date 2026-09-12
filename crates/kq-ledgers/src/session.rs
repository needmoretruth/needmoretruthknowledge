//! The quest a reader has opened: six stages, each a short conversation over three real ledgers.
//!
//! A stage is a list of [`Step`]s. Most are a sentence; a few are a [`Deed`] — a real transfer
//! into all three ledgers. Enter reveals the next step, and when a `Deed` is revealed it runs
//! there and then, so every number the reader sees was produced by the press they just made.
//!
//! Each stage starts its ledgers fresh. A reader who jumps to "spend it twice" with a digit key
//! gets the same three ledgers as a reader who walked there, which is the only way stages can be
//! entered in any order.

use nmtk_core::{Language, format};
use nmtk_kq::knob::{Knob, KnobValue};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::pad;
use nmtk_kq::theme::{State, Theme};
use nmtk_ledger::{
    Address, Amount, ApplyOutcome, DoubleSpendReport, Genesis, Key, Model, Scenario, SideBySide,
    TransferRequest,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::phrases::{self, Msg};

/// Alice's opening coins. Three of ten, so there is something to make change out of.
const OPENING_COINS: [Amount; 3] = [10, 10, 10];

/// Amounts worth trying. A reader who wants 17 types 17.
const AMOUNT_PRESETS: &[u64] = &[3, 10, 17, 20];

/// One move in a stage's conversation.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// The quest says one thing.
    Say(Msg),
    /// The quest asks the reader to do something before pressing Enter.
    Ask(Msg),
    /// Real work, run the moment this step is reached.
    Run(Deed),
}

/// Work a step does to the three ledgers.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Deed {
    /// Send exactly one whole coin, so nothing has to be broken up.
    SendWholeCoin,
    /// Send an amount no coin matches, so change has to be made.
    SendPartOfACoin,
    /// Send whatever the knobs say.
    SendChosen,
    /// Write two transfers against the same state and send both.
    SpendTwice,
}

use Deed::{SendChosen, SendPartOfACoin, SendWholeCoin, SpendTwice};
use Step::{Ask, Run, Say};

/// Where money lives. Nothing runs; the panel already shows three ledgers holding the same thing.
const COINS: &[Step] = &[
    Say(Msg::CoinsOne),
    Say(Msg::CoinsTwo),
    Say(Msg::CoinsThree),
    Say(Msg::CoinsFour),
    Say(Msg::CoinsFive),
    Say(Msg::CoinsSix),
];

/// One whole coin changes hands. The first thing the reader makes happen.
const SEND: &[Step] = &[
    Say(Msg::SendOne),
    Say(Msg::SendTwo),
    Run(SendWholeCoin),
    Say(Msg::SendUtxo),
    Say(Msg::SendAccount),
    Say(Msg::SendObject),
    Say(Msg::SendAsk),
];

/// The same transfer, an amount that does not fit a coin. This is where the models part company.
const GROW: &[Step] = &[
    Say(Msg::GrowOne),
    Say(Msg::GrowTwo),
    Run(SendPartOfACoin),
    Say(Msg::GrowUtxo),
    Say(Msg::GrowAccount),
    Say(Msg::GrowObject),
    Say(Msg::GrowLesson),
    Say(Msg::GrowCost),
];

/// The reader's own numbers, twice: one amount that divides into tens and one that does not.
const TUNE: &[Step] = &[
    Say(Msg::TuneOne),
    Say(Msg::TuneThree),
    Say(Msg::TuneFresh),
    Ask(Msg::TuneTwo),
    Run(SendChosen),
    Say(Msg::TuneSent),
    Ask(Msg::TuneAgain),
    Run(SendChosen),
    Say(Msg::TuneNote),
];

/// The attack every one of these systems exists to stop.
const TWICE: &[Step] = &[
    Say(Msg::TwiceOne),
    Say(Msg::TwiceTwo),
    Say(Msg::TwiceThree),
    Run(SpendTwice),
    Say(Msg::TwiceUtxo),
    Say(Msg::TwiceAccount),
    Say(Msg::TwiceObject),
    Say(Msg::TwiceLesson),
];

/// What the reader now knows, in four sentences.
const RECAP: &[Step] = &[
    Say(Msg::RecapOne),
    Say(Msg::RecapTwo),
    Say(Msg::RecapThree),
    Say(Msg::RecapFour),
];

/// The stages' scripts, in the order `lib.rs` declares them.
const SCRIPTS: [&[Step]; 6] = [COINS, SEND, GROW, TUNE, TWICE, RECAP];

/// What one [`Deed`] did, kept against the step that caused it so the conversation can be rebuilt
/// in any language without running anything again.
struct Done {
    step: usize,
    what: Outcome,
}

enum Outcome {
    /// One transfer into all three ledgers.
    Transfer(Box<SideBySide<ApplyOutcome>>),
    /// Two transfers against the same state.
    Double(Box<SideBySide<DoubleSpendReport>>),
}

pub struct Session {
    stage: usize,
    /// How many steps of this stage have been revealed. Step zero shows the moment it opens.
    revealed: usize,
    scenario: Scenario,
    done: Vec<Done>,
    knobs: Vec<Knob>,
    chosen: usize,
    alice: Key,
    bob: Address,
    carol: Address,
}

impl Session {
    pub fn new() -> Self {
        let alice = Key::from_seed(b"nmtk.kq.ledgers.alice");
        Self {
            stage: 0,
            revealed: 0,
            scenario: open_ledgers(alice.address()),
            done: Vec::new(),
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
                Knob::new("recipient", KnobValue::Choice { current: 0, count: 2 }),
            ],
            chosen: 0,
            alice,
            bob: Address::from_seed(b"nmtk.kq.ledgers.bob"),
            carol: Address::from_seed(b"nmtk.kq.ledgers.carol"),
        }
    }

    /// The script of the stage showing now.
    fn script(&self) -> &'static [Step] {
        SCRIPTS[self.stage.min(SCRIPTS.len() - 1)]
    }

    /// Throws this stage's ledgers away and opens fresh ones, back at the first sentence. The
    /// knobs keep their values: a reader who set an amount did not ask for it back.
    fn restart(&mut self) {
        self.revealed = 0;
        self.scenario = open_ledgers(self.alice.address());
        self.done.clear();
    }

    fn amount(&self) -> Amount {
        match self.knobs[0].value {
            KnobValue::Count { current, .. } => current,
            _ => 10,
        }
    }

    fn recipient(&self) -> Address {
        match self.knobs[1].value {
            KnobValue::Choice { current: 1, .. } => self.carol,
            _ => self.bob,
        }
    }

    fn recipient_name(&self) -> Msg {
        match self.knobs[1].value {
            KnobValue::Choice { current: 1, .. } => Msg::PartyCarol,
            _ => Msg::PartyBob,
        }
    }

    /// Does one deed and files the result under the step that asked for it.
    ///
    /// The ledgers go back to their opening state first. Every run is then a change from the same
    /// three coins of ten, which is the only way two runs can be compared — and it is why a reader
    /// can send 30 twice without being told the second time that Alice is out of money.
    fn perform(&mut self, step: usize, deed: Deed) {
        self.scenario = open_ledgers(self.alice.address());
        let what = match deed {
            SendWholeCoin => Outcome::Transfer(Box::new(
                self.scenario.transfer(&TransferRequest::new(self.alice, self.bob, 10)),
            )),
            SendPartOfACoin => Outcome::Transfer(Box::new(
                self.scenario.transfer(&TransferRequest::new(self.alice, self.bob, 3)),
            )),
            SendChosen => Outcome::Transfer(Box::new(self.scenario.transfer(
                &TransferRequest::new(self.alice, self.recipient(), self.amount()),
            ))),
            SpendTwice => {
                let first = TransferRequest::new(self.alice, self.bob, 10);
                let second = TransferRequest::new(self.alice, self.carol, 10);
                Outcome::Double(Box::new(self.scenario.double_spend(&first, &second)))
            }
        };
        self.done.push(Done { step, what });
    }

    /// The beats one finished deed is worth.
    fn beats_for(&self, done: &Done, language: Language) -> Vec<Beat> {
        let mut beats = Vec::new();
        match &done.what {
            Outcome::Transfer(side) => {
                // When all three agree there is nothing to compare, so one line says it. Three
                // lines that each say "accepted" are three lines the reader learns to skip, and a
                // reader who has learned to skip will skip the line that mattered.
                let all = Model::ALL.iter().all(|model| side.get(*model).accepted());
                if all {
                    beats.push(Beat::outcome(State::Good, Msg::SendAccepted.text(language)));
                } else {
                    beats.push(Beat::outcome(State::Bad, Msg::SendRefused.text(language)));
                    for model in Model::ALL {
                        beats.push(transfer_beat(model, side.get(model), language));
                    }
                }
            }
            Outcome::Double(reports) => {
                for model in Model::ALL {
                    beats.push(double_beat(model, reports.get(model), language));
                }
            }
        }
        beats
    }

    /// The transfer whose numbers the panel is showing, if any.
    fn latest_transfer(&self) -> Option<&SideBySide<ApplyOutcome>> {
        self.done.iter().rev().find_map(|done| match &done.what {
            Outcome::Transfer(side) => Some(side.as_ref()),
            Outcome::Double(_) => None,
        })
    }

    /// The double spend, if this stage has run one.
    fn latest_double(&self) -> Option<&SideBySide<DoubleSpendReport>> {
        self.done.iter().rev().find_map(|done| match &done.what {
            Outcome::Double(reports) => Some(reports.as_ref()),
            Outcome::Transfer(_) => None,
        })
    }

    /// True while the reader has values in front of them to change.
    fn tuning(&self) -> bool {
        SCRIPTS[self.stage.min(SCRIPTS.len() - 1)]
            .iter()
            .any(|step| matches!(step, Run(SendChosen)))
    }

    fn move_choice(&mut self, step: i32) {
        if !self.tuning() {
            return;
        }
        let last = self.knobs.len() - 1;
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

    /// One line saying what Alice has, then a block per ledger: what it holds, how big it is, and
    /// what the last run did to it.
    ///
    /// The balance is drawn once rather than three times because all three agree on it — which is
    /// the point. They disagree about how it is stored, never about how much there is.
    fn ledger_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let facts = self.scenario.facts();
        let transfer = self.latest_transfer();
        let double = self.latest_double();
        let mut lines = Vec::new();
        if let Some(balance) = self.scenario.agreed_balance(&self.alice.address()) {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", Msg::LabelHolds.text(language)), theme.muted()),
                Span::styled(format::count(balance), theme.heading()),
            ]));
            lines.push(Line::from(""));
        }
        for model in Model::ALL {
            let fact = facts.get(model);
            lines.push(Line::from(Span::styled(
                phrases::model(model).text(language),
                theme.heading(),
            )));
            lines.push(Line::from(vec![
                Span::styled("  ", theme.plain()),
                // The kind comes before the number so no language has to agree a plural: "coins 3"
                // and "lines 1" both read, where "1 lines" does not.
                Span::styled(
                    pad(phrases::entry_kind(fact.entry_kind).text(language), 14),
                    theme.muted(),
                ),
                Span::styled(pad(&format::count(fact.entry_count as u64), 6), theme.plain()),
                Span::styled(format::bytes(fact.state_size_bytes), theme.plain()),
            ]));
            if let Some(reports) = double {
                lines.push(verdict_line(reports.get(model), language, theme));
            } else if let Some(side) = transfer {
                lines.push(change_line(side.get(model), language, theme));
            }
            lines.push(Line::from(""));
        }
        lines
    }

    /// The values the reader is turning, with the chosen one marked.
    fn knob_lines(&self, language: Language, theme: Theme) -> Vec<Line<'static>> {
        let labels = [Msg::KnobAmount, Msg::KnobRecipient];
        self.knobs
            .iter()
            .enumerate()
            .map(|(index, knob)| {
                let picked = index == self.chosen;
                let marker = if picked { State::Chosen.mark() } else { " " };
                // The recipient is a name, not a number, so it is worded rather than displayed.
                let value = match (index, knob.draft()) {
                    (1, None) => self.recipient_name().text(language).to_string(),
                    _ => knob.display(),
                };
                Line::from(vec![
                    Span::styled(format!("{marker} "), theme.state(State::Chosen)),
                    Span::styled(pad(labels[index].text(language), 16), theme.plain()),
                    Span::styled(value, if picked { theme.heading() } else { theme.muted() }),
                ])
            })
            .collect()
    }
}

impl Default for Session {
    fn default() -> Self {
        Self::new()
    }
}

impl KqSession for Session {
    fn stage(&self) -> usize {
        self.stage
    }

    fn go_to(&mut self, stage: usize) {
        if stage >= SCRIPTS.len() {
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
                Run(_) => {
                    if let Some(done) = self.done.iter().find(|done| done.step == index) {
                        beats.extend(self.beats_for(done, language));
                    }
                }
            }
        }
        beats
    }

    fn at_end(&self) -> bool {
        self.revealed + 1 >= self.script().len()
    }

    fn can_advance(&self) -> bool {
        self.revealed + 1 < self.script().len()
    }

    fn knobs(&self) -> &[Knob] {
        if self.tuning() { &self.knobs } else { &[] }
    }

    fn chosen_knob(&self) -> Option<usize> {
        self.tuning().then_some(self.chosen)
    }

    fn on(&mut self, action: Action) -> Reaction {
        match action {
            Action::Stage(stage) => {
                self.go_to(stage);
                Reaction::Handled
            }
            Action::Go => {
                let script = self.script();
                // The end of a stage is not the end of the quest, but walking on from here is the
                // shell's business: it is what knows there is another stage to walk to.
                if self.revealed + 1 >= script.len() {
                    return Reaction::Ignored;
                }
                self.revealed += 1;
                if let Run(deed) = script[self.revealed] {
                    let at = self.revealed;
                    self.perform(at, deed);
                }
                Reaction::Handled
            }
            Action::Reset => {
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
                if self.tuning() {
                    self.knobs[self.chosen].nudge(direction);
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Type(c) => {
                if self.tuning() {
                    self.knobs[self.chosen].type_char(c);
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Backspace => {
                if self.tuning() {
                    self.knobs[self.chosen].backspace();
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Commit => {
                if self.tuning() {
                    self.knobs[self.chosen].commit();
                    Reaction::Handled
                } else {
                    Reaction::Ignored
                }
            }
            Action::Cancel => {
                if self.tuning() {
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

    /// Nothing here runs in the background: three small ledgers in memory answer before the screen
    /// is redrawn. A quest with no workers must not draw a worker's status mark.
    fn run_state(&self) -> RunState {
        RunState::Idle
    }

    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let block = theme.titled_panel(Msg::PanelTitle.text(language));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        if self.tuning() {
            let [knobs, ledgers] =
                Layout::vertical([Constraint::Length(3), Constraint::Min(1)]).areas(inner);
            frame.render_widget(Paragraph::new(self.knob_lines(language, theme)), knobs);
            frame.render_widget(Paragraph::new(self.ledger_lines(language, theme)), ledgers);
        } else {
            frame.render_widget(Paragraph::new(self.ledger_lines(language, theme)), inner);
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        // Enter is already in the bar as "continue"; saying it again as "send" would read as two
        // different keys. What the reader cannot guess is that the values move with the arrows.
        if self.tuning() {
            vec![("↑↓ ←→", Msg::KeyChange.text(language))]
        } else {
            Vec::new()
        }
    }

    fn typing(&self) -> bool {
        self.tuning() && self.knobs[self.chosen].draft().is_some()
    }

    fn close(&mut self) {}
}

/// Three ledgers holding the same thing: Alice with three coins of ten.
fn open_ledgers(alice: Address) -> Scenario {
    Scenario::new(&Genesis::new().holding(alice, &OPENING_COINS))
}

/// `Bitcoin (UTXO)  accepted` — or, when it was not, where it died and why.
fn transfer_beat(model: Model, outcome: &ApplyOutcome, language: Language) -> Beat {
    let accepted = outcome.accepted();
    let name = phrases::model(model).text(language);
    let verdict = if accepted { Msg::LabelAccepted } else { Msg::LabelRejected }.text(language);
    let mut text = format!("{name}  {verdict}");
    if let Some((step, reason)) = outcome.rejection() {
        text.push_str(&format!(
            " — {}: {}",
            phrases::step(step).text(language),
            phrases::why(reason.kind()).text(language)
        ));
    }
    Beat::outcome(if accepted { State::Good } else { State::Bad }, text)
}

/// `Ethereum (account)  stopped it — checking it is current: that counter has already been used`.
fn double_beat(model: Model, report: &DoubleSpendReport, language: Language) -> Beat {
    let stopped = report.stopped();
    let name = phrases::model(model).text(language);
    let verdict = if stopped { Msg::LabelStopped } else { Msg::LabelLetThrough }.text(language);
    let mut text = format!("{name}  {verdict}");
    if let (Some(step), Some(kind)) = (report.stopped_at(), report.stopped_by()) {
        text.push_str(&format!(
            " — {}: {}",
            phrases::step(step).text(language),
            phrases::why(kind).text(language)
        ));
    }
    Beat::outcome(if stopped { State::Good } else { State::Bad }, text)
}

/// What the last transfer cost this ledger, in entries and in bytes.
fn change_line(outcome: &ApplyOutcome, language: Language, theme: Theme) -> Line<'static> {
    let accepted = outcome.accepted();
    let state = if accepted { State::Good } else { State::Bad };
    let mut spans = vec![
        Span::styled(format!("  {} ", state.mark()), theme.state(state)),
        Span::styled(
            pad(
                if accepted { Msg::LabelAccepted } else { Msg::LabelRejected }.text(language),
                12,
            ),
            theme.state(state),
        ),
    ];
    if accepted {
        spans.push(Span::styled(
            format!(
                "{} {:+}   {:+} B",
                Msg::ColumnChange.text(language),
                outcome.entry_delta(),
                outcome.size_delta()
            ),
            theme.muted(),
        ));
    }
    Line::from(spans)
}

/// Whether this ledger stopped the second spend.
fn verdict_line(report: &DoubleSpendReport, language: Language, theme: Theme) -> Line<'static> {
    let stopped = report.stopped();
    let state = if stopped { State::Good } else { State::Bad };
    Line::from(vec![
        Span::styled(format!("  {} ", state.mark()), theme.state(state)),
        Span::styled(
            if stopped { Msg::LabelStopped } else { Msg::LabelLetThrough }.text(language),
            theme.state(state),
        ),
    ])
}

#[cfg(test)]
mod tests {
    use nmtk_kq::session::Voice;

    use super::*;

    /// Presses Enter until the stage runs out of steps.
    fn walk(session: &mut Session) {
        for _ in 0..session.script().len() {
            if session.revealed + 1 < session.script().len() {
                session.on(Action::Go);
            }
        }
    }

    #[test]
    fn a_stage_opens_with_one_sentence_and_not_a_wall() {
        let session = Session::new();
        let beats = session.transcript(Language::ENGLISH);
        assert_eq!(beats.len(), 1, "the reader was handed more than one thing at once");
        assert!(beats[0].text.len() < 140, "the opening beat is a paragraph: {:?}", beats[0].text);
    }

    #[test]
    fn every_beat_of_every_stage_is_short_in_both_languages() {
        for stage in 0..SCRIPTS.len() {
            for language in Language::ALL {
                let mut session = Session::new();
                session.go_to(stage);
                walk(&mut session);
                for beat in session.transcript(*language) {
                    assert!(
                        beat.text.chars().count() <= 160,
                        "stage {stage} in {language} says too much at once: {:?}",
                        beat.text
                    );
                    assert!(!beat.text.is_empty(), "stage {stage} has a blank beat in {language}");
                }
            }
        }
    }

    /// The quest stops at the end of its own stage and says so. Walking into the next one is the
    /// shell's move, because only the shell knows there is another stage to walk to.
    #[test]
    fn a_stage_says_when_it_has_nothing_more_to_say() {
        let mut session = Session::new();
        assert!(!session.at_end());
        walk(&mut session);
        assert!(session.at_end());
        assert!(!session.can_advance());
        assert_eq!(session.on(Action::Go), Reaction::Ignored);
        assert_eq!(session.stage(), 0);
    }

    #[test]
    fn the_last_beat_of_the_quest_can_go_no_further() {
        let mut session = Session::new();
        session.go_to(SCRIPTS.len() - 1);
        walk(&mut session);
        assert!(session.at_end());
        assert_eq!(session.on(Action::Go), Reaction::Ignored);
    }

    #[test]
    fn sending_one_whole_coin_is_taken_by_all_three() {
        let mut session = Session::new();
        session.go_to(1);
        walk(&mut session);
        let side = session.latest_transfer().expect("stage 2 never sent anything");
        for (model, outcome) in side.iter() {
            assert!(outcome.accepted(), "{model:?} refused an ordinary transfer");
        }
    }

    #[test]
    fn making_change_costs_the_three_models_different_amounts() {
        let mut session = Session::new();
        session.go_to(2);
        walk(&mut session);
        let side = session.latest_transfer().expect("stage 3 never sent anything");
        let growth: Vec<i64> = side.iter().map(|(_, outcome)| outcome.size_delta()).collect();
        assert!(
            growth.iter().any(|delta| *delta != growth[0]),
            "every model grew the same, so the comparison teaches nothing: {growth:?}"
        );
    }

    /// The three sentences the quest says about the attack name three specific reasons. If the
    /// engine ever starts refusing for a different reason, those sentences become lies, and this
    /// is where that shows up.
    #[test]
    fn the_words_about_the_attack_match_what_the_engine_actually_said() {
        use nmtk_ledger::RejectionKind;

        let mut session = Session::new();
        session.go_to(4);
        walk(&mut session);
        let reports = session.latest_double().expect("stage 5 never attacked anything");
        assert_eq!(
            reports.get(Model::Utxo).stopped_by(),
            Some(RejectionKind::InputNotFound),
            "the quest says the coin is gone"
        );
        assert_eq!(
            reports.get(Model::Account).stopped_by(),
            Some(RejectionKind::NonceMismatch),
            "the quest says the counter has been used"
        );
        assert_eq!(
            reports.get(Model::Object).stopped_by(),
            Some(RejectionKind::NotOwner),
            "the quest says the coin has changed hands"
        );
    }

    #[test]
    fn spending_twice_is_stopped_by_all_three_for_different_reasons() {
        let mut session = Session::new();
        session.go_to(4);
        walk(&mut session);
        let reports = session.latest_double().expect("stage 5 never attacked anything");
        let mut reasons = Vec::new();
        for (model, report) in reports.iter() {
            assert!(report.stopped(), "{model:?} let the same coin be spent twice");
            reasons.push(report.stopped_by());
        }
        reasons.sort_by_key(|reason| format!("{reason:?}"));
        reasons.dedup();
        assert_eq!(reasons.len(), 3, "the models gave one reason, which is the lesson lost");
    }

    #[test]
    fn the_run_beats_carry_a_verdict_the_reader_can_see_without_colour() {
        let mut session = Session::new();
        session.go_to(4);
        walk(&mut session);
        let verdicts = session
            .transcript(Language::ENGLISH)
            .into_iter()
            .filter(|beat| matches!(beat.voice, Voice::Event(Some(_))))
            .count();
        assert_eq!(verdicts, 3, "the double spend did not report one verdict per ledger");
    }

    #[test]
    fn a_typed_amount_is_taken_and_a_silly_one_is_not() {
        let mut session = Session::new();
        session.go_to(3);
        for c in "25".chars() {
            session.on(Action::Type(c));
        }
        assert!(session.typing());
        session.on(Action::Commit);
        assert_eq!(session.amount(), 25);

        for c in "900".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.amount(), 25, "an out-of-range amount was accepted");
    }

    #[test]
    fn the_readers_own_amount_is_what_gets_sent() {
        let mut session = Session::new();
        session.go_to(3);
        for c in "17".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        walk(&mut session);
        let side = session.latest_transfer().expect("the tune stage never sent anything");
        assert!(side.get(Model::Utxo).accepted());
        let alice = session.scenario.balances(&session.alice.address());
        assert!(
            *alice.get(Model::Account) < 30,
            "the account ledger did not move the reader's own amount"
        );
    }

    #[test]
    fn each_stage_starts_its_ledgers_fresh() {
        let mut session = Session::new();
        session.go_to(1);
        walk(&mut session);
        let spent = session.scenario.balances(&session.alice.address());
        session.go_to(4);
        let fresh = session.scenario.balances(&session.alice.address());
        assert_ne!(*spent.get(Model::Utxo), 30);
        assert_eq!(*fresh.get(Model::Utxo), 30, "a jump carried the last stage's spending along");
    }

    #[test]
    fn reset_puts_the_coins_back_and_the_conversation_to_its_first_line() {
        let mut session = Session::new();
        session.go_to(2);
        walk(&mut session);
        session.on(Action::Reset);
        assert_eq!(session.transcript(Language::ENGLISH).len(), 1);
        let back = session.scenario.balances(&session.alice.address());
        assert_eq!(*back.get(Model::Utxo), 30, "reset did not restore the opening holdings");
    }

    #[test]
    fn knobs_are_only_offered_where_there_is_something_to_turn() {
        let mut session = Session::new();
        assert!(session.knobs().is_empty(), "the opening stage offered values with nothing to run");
        session.go_to(3);
        assert_eq!(session.knobs().len(), 2);
        assert_eq!(session.chosen_knob(), Some(0));
    }
}
