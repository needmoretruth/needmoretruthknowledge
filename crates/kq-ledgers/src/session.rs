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
use nmtk_kq::text::{column, pad, rpad};
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
    /// One sentence, chosen from what the run actually did.
    Tell(Topic),
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

/// What a [`Step::Tell`] is about: a sentence built from the send that just happened rather than
/// from the amount the conversation suggested, because the amount is the reader's to choose.
#[derive(Debug, Clone, Copy)]
enum Topic {
    /// What this send did to the three ledgers.
    Sent,
    /// What the two sends, held together, showed.
    Compared,
}
use Step::{Ask, Run, Say, Tell};

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
    Say(Msg::GrowFresh),
    Run(SendPartOfACoin),
    Say(Msg::GrowUtxo),
    Say(Msg::GrowAccount),
    Say(Msg::GrowObject),
    // The acronym is earned here: the reader has just watched an unspent coin be destroyed and
    // two new ones made, which is the only thing that makes the words mean anything.
    Say(Msg::GrowName),
    Say(Msg::GrowLesson),
    Say(Msg::GrowCost),
];

/// The reader's own numbers, twice: one amount that divides into tens and one that does not.
const TUNE: &[Step] = &[
    Say(Msg::TuneOne),
    Say(Msg::TuneThree),
    Say(Msg::TuneTyping),
    Say(Msg::TuneFresh),
    Ask(Msg::TuneTwo),
    Run(SendChosen),
    Tell(Topic::Sent),
    Ask(Msg::TuneAgain),
    Run(SendChosen),
    Tell(Topic::Compared),
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
const RECAP: &[Step] =
    &[Say(Msg::RecapOne), Say(Msg::RecapTwo), Say(Msg::RecapThree), Say(Msg::RecapFour)];

/// The stages' scripts, in the order `lib.rs` declares them.
const SCRIPTS: [&[Step]; 6] = [COINS, SEND, GROW, TUNE, TWICE, RECAP];

/// Where the recap sits. It runs nothing of its own, so it is the one stage that draws the record
/// of the last run rather than the ledgers in front of it.
const STAGE_RECAP: usize = 5;

/// What one [`Deed`] did, kept against the step that caused it so the conversation can be rebuilt
/// in any language without running anything again.
struct Done {
    step: usize,
    what: Outcome,
}

#[derive(Clone)]
enum Outcome {
    /// One transfer into all three ledgers.
    Transfer(Box<SideBySide<ApplyOutcome>>),
    /// Two transfers against the same state.
    Double(Box<SideBySide<DoubleSpendReport>>),
}

/// What the last run left behind: the ledgers as they stood when it was over, and what it did to
/// them.
///
/// Every run rebuilds the world from the same three coins of ten, and leaving a stage rebuilds it
/// again, so by the time the reader reaches the recap the live ledgers no longer remember any of
/// it. This is the only place what they did still exists.
struct Kept {
    /// Which stage produced it, so `r` pressed there forgets it and `r` elsewhere does not.
    stage: usize,
    scenario: Scenario,
    what: Outcome,
}

pub struct Session {
    stage: usize,
    /// How many steps of this stage have been revealed. Step zero shows the moment it opens.
    revealed: usize,
    scenario: Scenario,
    done: Vec<Done>,
    /// The last run, kept apart from the live ledgers so the recap can still read it.
    kept: Option<Kept>,
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
            kept: None,
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
    ///
    /// The record of the last run is kept. Rebuilding the world is how every run starts from the
    /// same three coins of ten; forgetting what the reader did is not part of that.
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
            SendChosen => {
                Outcome::Transfer(Box::new(self.scenario.transfer(&TransferRequest::new(
                    self.alice,
                    self.recipient(),
                    self.amount(),
                ))))
            }
            SpendTwice => {
                let first = TransferRequest::new(self.alice, self.bob, 10);
                let second = TransferRequest::new(self.alice, self.carol, 10);
                Outcome::Double(Box::new(self.scenario.double_spend(&first, &second)))
            }
        };
        // Kept before the world is rebuilt for the next run, because after that it is gone.
        let kept = Kept { stage: self.stage, scenario: self.scenario.clone(), what: what.clone() };
        self.kept = Some(kept);
        self.done.push(Done { step, what });
    }

    /// The record the panel reads instead of the live ledgers.
    ///
    /// Only the recap reads it. Every other stage is looking at a world its own runs made, which
    /// is in front of it; the recap is looking back at a run whose world has since been rebuilt.
    fn recorded(&self) -> Option<&Kept> {
        self.kept.as_ref().filter(|_| self.stage == STAGE_RECAP)
    }

    fn tell_sentence(&self, topic: Topic, language: Language) -> &'static str {
        let sends: Vec<&SideBySide<ApplyOutcome>> = self
            .done
            .iter()
            .filter_map(|done| match &done.what {
                Outcome::Transfer(side) => Some(side.as_ref()),
                Outcome::Double(_) => None,
            })
            .collect();
        match topic {
            // Whether a coin had to be broken is the engine's answer, not the amount's: the
            // reader picks the amount, and the sentence used to be written for the suggested one.
            Topic::Sent => match sends.last() {
                Some(side) if side.get(Model::Utxo).entry_delta() == 0 => Msg::TuneSentWhole,
                Some(_) => Msg::TuneSentPart,
                None => Msg::TuneOnlyOne,
            },
            // Every send rebuilds the world from the same three coins, which is what makes two
            // sends comparable — and it means the book gains Bob's line every time. The old
            // sentence claimed the book came out the same size both times; it never does.
            Topic::Compared => {
                if sends.len() < 2 {
                    Msg::TuneOnlyOne
                } else {
                    Msg::TuneBookLine
                }
            }
        }
        .text(language)
    }

    fn tell(&self, topic: Topic, language: Language) -> String {
        self.tell_sentence(topic, language).to_string()
    }

    /// The ledgers the panel is drawing.
    fn showing(&self) -> &Scenario {
        match self.recorded() {
            Some(kept) => &kept.scenario,
            None => &self.scenario,
        }
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

    /// What the panel may draw from, newest first: the record the recap reads, then whatever this
    /// stage has run since it opened.
    fn runs(&self) -> impl Iterator<Item = &Outcome> {
        self.recorded()
            .map(|kept| &kept.what)
            .into_iter()
            .chain(self.done.iter().rev().map(|done| &done.what))
    }

    /// The transfer whose numbers the panel is showing, if any.
    fn latest_transfer(&self) -> Option<&SideBySide<ApplyOutcome>> {
        self.runs().find_map(|what| match what {
            Outcome::Transfer(side) => Some(side.as_ref()),
            Outcome::Double(_) => None,
        })
    }

    /// The double spend, if this stage has run one.
    fn latest_double(&self) -> Option<&SideBySide<DoubleSpendReport>> {
        self.runs().find_map(|what| match what {
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
        let scenario = self.showing();
        let facts = scenario.facts();
        let transfer = self.latest_transfer();
        let double = self.latest_double();
        let mut lines = Vec::new();
        if let Some(balance) = scenario.agreed_balance(&self.alice.address()) {
            lines.push(Line::from(vec![
                Span::styled(format!("{} ", Msg::LabelHolds.text(language)), theme.muted()),
                Span::styled(format::count(balance), theme.heading()),
            ]));
            lines.push(Line::from(""));
        }
        // The counts under here are the whole ledger's, not Alice's. Sitting straight under
        // "Alice holds 20", "coins 3" read as three coins of hers.
        lines.push(Line::from(Span::styled(
            Msg::LabelWholeState.text(language).to_string(),
            theme.muted(),
        )));
        lines.push(Line::from(""));
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
                    column(phrases::entry_kind(fact.entry_kind).text(language), 14),
                    theme.muted(),
                ),
                // Right-aligned so the numbers sit under each other: 192, 36 and 207 left-aligned
                // do not compare, which is the one thing this panel exists for.
                Span::styled(rpad(&format::count(fact.entry_count as u64), 4), theme.plain()),
                Span::styled(rpad(&format::bytes(fact.state_size_bytes), 12), theme.plain()),
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
                Tell(topic) => beats.push(Beat::say(self.tell(*topic, language))),
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
                // `r` is the one thing that forgets results, and only the ones made here.
                if self.kept.as_ref().is_some_and(|kept| kept.stage == self.stage) {
                    self.kept = None;
                }
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
            pad(if accepted { Msg::LabelAccepted } else { Msg::LabelRejected }.text(language), 12),
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
    use nmtk_kq::theme::{MIN_HEIGHT, MIN_WIDTH, split};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    /// Puts the amount knob where a reader's keystrokes would have put it.
    fn set_amount(session: &mut Session, amount: Amount) {
        if let KnobValue::Count { current, .. } = &mut session.knobs[0].value {
            *current = amount;
        }
    }

    /// "Alice holds 20" sat directly above "coins 3", and a reviewer read the three as hers.
    #[test]
    fn the_panel_says_whose_the_counts_under_the_balance_are() {
        let mut session = Session::new();
        session.perform(0, SendWholeCoin);
        for language in Language::ALL {
            let lines = session.ledger_lines(*language, Theme::new(true));
            let text: String = lines
                .iter()
                .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect::<String>())
                .collect::<Vec<_>>()
                .join("\n");
            println!("=== {language} ===\n{text}");
            let head: String = Msg::LabelWholeState.text(*language).chars().take(12).collect();
            assert!(text.contains(&head), "nothing says whose the counts are:\n{text}");
        }
    }

    /// The reader picks the amount, so the sentence after the send has to read the send. Sending
    /// 7 when the conversation suggested 10 used to be answered with the lesson about 10.
    #[test]
    fn what_is_said_after_a_send_is_about_the_send_that_happened() {
        let english = Language::ENGLISH;
        let mut session = Session::new();
        session.go_to(3);
        assert_eq!(session.tell(Topic::Sent, english), Msg::TuneOnlyOne.text(english));

        // A whole coin: nothing to break.
        set_amount(&mut session, 10);
        session.perform(4, SendChosen);
        assert_eq!(session.tell(Topic::Sent, english), Msg::TuneSentWhole.text(english));
        assert_eq!(session.tell(Topic::Compared, english), Msg::TuneOnlyOne.text(english));

        // Less than a coin: change has to go somewhere.
        set_amount(&mut session, 7);
        session.perform(7, SendChosen);
        assert_eq!(session.tell(Topic::Sent, english), Msg::TuneSentPart.text(english));
        assert_eq!(session.tell(Topic::Compared, english), Msg::TuneBookLine.text(english));
    }

    /// What the three models really do when a coin has to be broken up, so the sentences beside
    /// the panel can be checked against it rather than against what they assume.
    /// What the three models really do when a coin has to be broken up, so the sentences beside
    /// the panel can be held against it rather than against what they assume.
    #[test]
    fn breaking_a_coin_grows_every_model_that_has_to_make_change() {
        let mut session = Session::new();
        session.perform(0, SendPartOfACoin);
        let Some(Done { what: Outcome::Transfer(reports), .. }) = session.done.last() else {
            panic!("the transfer did not happen");
        };
        for model in Model::ALL {
            let report = reports.get(model);
            println!(
                "{model:?}: entries {} -> {}, delta {}",
                report.entries_before,
                report.entries_after,
                report.entry_delta()
            );
        }
        assert!(reports.get(Model::Utxo).entry_delta() > 0, "bitcoin made change and did not grow");
        assert!(reports.get(Model::Object).entry_delta() > 0, "sui split and did not grow");
        assert!(
            reports.get(Model::Account).entry_delta() > 0,
            "the account model reached a reader it had no line for and did not make one"
        );

        // The difference is not the first payment but the second: coins go on making change,
        // while a book that already has the line only edits it.
        let mut scenario = open_ledgers(session.alice.address());
        scenario.transfer(&TransferRequest::new(session.alice, session.bob, 3));
        let again = scenario.transfer(&TransferRequest::new(session.alice, session.bob, 3));
        for model in Model::ALL {
            println!("second payment, {model:?}: delta {}", again.get(model).entry_delta());
        }
        assert!(again.get(Model::Utxo).entry_delta() > 0, "bitcoin stopped making change");
        assert_eq!(
            again.get(Model::Account).entry_delta(),
            0,
            "the account model made a second line for a reader it already had"
        );
    }

    /// Draws the quest's panel exactly where the shell puts it on the smallest screen nmtk allows.
    fn draw(session: &Session) -> String {
        let mut terminal = Terminal::new(TestBackend::new(MIN_WIDTH, MIN_HEIGHT)).expect("backend");
        terminal
            .draw(|frame| {
                let [_, body, _] = Layout::vertical([
                    Constraint::Length(1),
                    Constraint::Min(1),
                    Constraint::Length(1),
                ])
                .areas(frame.area());
                let (talk, run) = split(body.width);
                let [_, panel] =
                    Layout::horizontal([Constraint::Length(talk), Constraint::Length(run)])
                        .areas(body);
                session.render(frame, panel, Theme::new(true), Language::ENGLISH);
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

    /// A reader reaches the recap by pressing Tab, which is `go_to`, and the world is rebuilt
    /// before every run — so the recap has to read the record of what they did rather than the
    /// opening coins sitting in front of it.
    #[test]
    fn the_recap_shows_what_the_reader_did_rather_than_the_opening_coins() {
        let mut session = Session::new();
        session.go_to(4);
        walk(&mut session);
        assert!(session.latest_double().is_some(), "the attack stage never ran");

        session.go_to(STAGE_RECAP);
        let text = draw(&session);
        assert!(text.contains("stopped it"), "the recap lost the three verdicts:\n{text}");
        assert!(
            text.contains("Alice holds 20"),
            "the recap shows the opening coins rather than what the reader spent:\n{text}"
        );
    }

    /// `r` forgets the results of the stage it was pressed on, and leaves the rest alone.
    #[test]
    fn r_forgets_this_stages_run_and_leaves_the_others_alone() {
        let mut session = Session::new();
        session.go_to(4);
        walk(&mut session);

        session.go_to(1);
        session.on(Action::Reset);
        assert!(session.kept.is_some(), "`r` on a stage that ran nothing threw away the attack");

        session.go_to(4);
        walk(&mut session);
        session.on(Action::Reset);
        assert!(session.kept.is_none(), "`r` kept the run it was pressed on");
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
