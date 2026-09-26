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
use nmtk_kq::knob::{Knob, KnobValue, Settled, Typed};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::{column, rpad, wrap};
use nmtk_kq::theme::{State, Theme};
use nmtk_ledger::{
    Address, Amount, ApplyOutcome, DoubleSpendReport, Genesis, Key, Model, RejectionKind, Scenario,
    SideBySide, TransferRequest,
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
    /// How many transfers the reader sent over the whole quest, and how many made change.
    RecapSends,
    /// The most storage one of those transfers added, per ledger.
    RecapStorage,
    /// How the reader's double spend ended, if they tried one.
    RecapAttack,
}
use Step::{Ask, Run, Say, Tell};

/// Where money lives. Nothing runs; the panel already shows three ledgers holding the same thing.
///
/// A node is explained before the sentence about bytes leans on it, and bytes before the one that
/// asks the reader to watch the sizes. The byte column itself stays off the panel until then: a
/// `B` on screen that nothing has explained is a word the reader has to guess.
const COINS: &[Step] = &[
    Say(Msg::CoinsOne),
    Say(Msg::CoinsTwo),
    Say(Msg::CoinsThree),
    Say(Msg::CoinsSui),
    Say(Msg::CoinsFour),
    Say(Msg::CoinsFive),
    Say(Msg::CoinsNode),
    Say(Msg::CoinsBytes),
    Say(Msg::CoinsSix),
];

/// Where in [`COINS`] bytes are explained, and so where the byte column may appear.
const COINS_BYTES_AT: usize = 7;

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
    Say(Msg::GrowChange),
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
///
/// The counter and the version are introduced before the run, because the run's own verdicts name
/// them the moment Enter is pressed.
const TWICE: &[Step] = &[
    Say(Msg::TwiceOne),
    Say(Msg::TwiceTwo),
    Say(Msg::TwiceMarkCounter),
    Say(Msg::TwiceMarkVersion),
    Say(Msg::TwiceThree),
    Run(SpendTwice),
    Say(Msg::TwiceUtxo),
    Say(Msg::TwiceAccount),
    Say(Msg::TwiceObject),
    Say(Msg::TwiceCounter),
    Say(Msg::TwiceCounterAgain),
    Say(Msg::TwiceLesson),
];

/// What the reader now knows, read out of the record of everything they did.
///
/// It used to be four `Say`s, one of which told every reader that the same transfer had cost the
/// three ledgers different amounts and the same attack had failed for different reasons — including
/// a reader who had pressed Tab straight here and done neither.
const RECAP: &[Step] = &[
    Say(Msg::RecapOne),
    Say(Msg::RecapTwo),
    Say(Msg::RecapTwoSui),
    Tell(Topic::RecapSends),
    Tell(Topic::RecapStorage),
    Tell(Topic::RecapAttack),
    Say(Msg::RecapFour),
];

/// The stages' scripts, in the order `lib.rs` declares them.
const SCRIPTS: [&[Step]; 6] = [COINS, SEND, GROW, TUNE, TWICE, RECAP];

/// Where the recap sits. It runs nothing of its own, so it is the one stage that draws the record
/// of the last run rather than the ledgers in front of it.
const STAGE_RECAP: usize = 5;

/// The stage whose script explains what "UTXO" stands for.
const STAGE_GROW: usize = 2;

/// One transfer the reader sent, as the recap needs it. Kept for the whole quest, because the
/// recap is about the whole quest and not about whichever run happened to be last.
#[derive(Debug, Clone, Copy)]
struct SentRecord {
    /// Which stage sent it, so `r` there forgets it and `r` elsewhere does not.
    stage: usize,
    /// Whether a coin had to be broken and change made.
    made_change: bool,
    /// What it did to each ledger's size, in bytes.
    size_delta: SideBySide<i64>,
}

/// One double spend the reader tried.
#[derive(Debug, Clone, Copy)]
struct AttackRecord {
    stage: usize,
    /// Whether each ledger stopped the second spend.
    stopped: SideBySide<bool>,
    /// The reason each ledger gave.
    reason: SideBySide<Option<RejectionKind>>,
}

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
    /// What became of the last number the reader typed, and into which knob, said once at the end
    /// of the conversation and forgotten the moment they do anything else.
    typed: Option<(usize, Typed)>,
    /// Every transfer the reader has sent in this quest.
    sends: Vec<SentRecord>,
    /// Every double spend the reader has tried in this quest.
    attacks: Vec<AttackRecord>,
    /// Whether the conversation has said what "UTXO" stands for. Until it has, the heading says
    /// "Bitcoin" and nothing more.
    utxo_named: bool,
    /// The values the last send used, so a changed amount can be told from a repeat.
    sent_with: Option<Settled>,
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
            typed: None,
            sends: Vec::new(),
            attacks: Vec::new(),
            utxo_named: false,
            sent_with: None,
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

    /// Whether the reader has changed a value since the last send.
    fn values_moved(&self) -> bool {
        match &self.sent_with {
            Some(settled) => !settled.still(self.knobs()),
            None => false,
        }
    }

    /// Does this stage's deed again with the values now on screen, in place of the one it did
    /// before, so the sentence that reads the result is about the send that just happened.
    fn rerun(&mut self) -> Reaction {
        if !self.tuning() {
            return Reaction::Ignored;
        }
        let script = self.script();
        let upto = self.revealed.min(script.len().saturating_sub(1));
        let Some(at) = script[..=upto].iter().rposition(|step| matches!(step, Run(_))) else {
            return Reaction::Ignored;
        };
        let Run(deed) = script[at] else { return Reaction::Ignored };
        self.done.retain(|done| done.step != at);
        self.perform(at, deed);
        Reaction::Handled
    }

    /// Does one deed and files the result under the step that asked for it, and in the record of
    /// the whole quest.
    ///
    /// The ledgers go back to their opening state first. Every run is then a change from the same
    /// three coins of ten, which is the only way two runs can be compared — and it is why a reader
    /// can send 30 twice without being told the second time that Alice is out of money.
    fn perform(&mut self, step: usize, deed: Deed) {
        let settled = Settled::of(self.knobs());
        self.sent_with = Some(settled);
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
        match &what {
            Outcome::Transfer(side) => self.sends.push(SentRecord {
                stage: self.stage,
                made_change: made_change(side.get(Model::Utxo)),
                size_delta: side.as_ref().clone().map(|_, outcome| outcome.size_delta()),
            }),
            Outcome::Double(reports) => self.attacks.push(AttackRecord {
                stage: self.stage,
                stopped: reports.as_ref().clone().map(|_, report| report.stopped()),
                reason: reports.as_ref().clone().map(|_, report| report.stopped_by()),
            }),
        }
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

    /// The sends this stage made before step `at`, oldest first.
    ///
    /// A `Tell` reads the sends that came before it and no later ones. The conversation is rebuilt
    /// every frame, and the sentence after the first send used to read "the last send" — so the
    /// moment the second one went in, the first sentence silently changed to describe it.
    fn sends_before(&self, at: usize) -> Vec<&SideBySide<ApplyOutcome>> {
        let mut done: Vec<&Done> = self.done.iter().filter(|done| done.step < at).collect();
        done.sort_by_key(|done| done.step);
        done.into_iter()
            .filter_map(|done| match &done.what {
                Outcome::Transfer(side) => Some(side.as_ref()),
                Outcome::Double(_) => None,
            })
            .collect()
    }

    /// The sentence a `Tell` at step `at` says, read from what the runs actually did.
    fn tell(&self, topic: Topic, at: usize, language: Language) -> String {
        let sends = self.sends_before(at);
        match topic {
            // Whether a coin had to be broken is the engine's answer, not the amount's: the
            // reader picks the amount. The count of coins that follows is the engine's too — 17
            // breaks a coin and still leaves Bitcoin's side the same size, because two coins went
            // in to cover it, and a sentence claiming it grew would be a sentence about 3.
            Topic::Sent => match sends.last() {
                Some(side) => {
                    let utxo = side.get(Model::Utxo);
                    let said =
                        if made_change(utxo) { Msg::TuneSentPart } else { Msg::TuneSentWhole };
                    format!(
                        "{} {} {} → {}.",
                        said.text(language),
                        Msg::TuneCoinsLabel.text(language),
                        utxo.entries_before,
                        utxo.entries_after,
                    )
                }
                None => Msg::TuneOnlyOne.text(language).to_string(),
            },
            // Every send rebuilds the world from the same three coins, which is what makes two
            // sends comparable. Two sends that both broke a coin, or both did not, show nothing,
            // and the sentence says so rather than drawing the lesson over a run that lacks it.
            Topic::Compared => match sends.as_slice() {
                [.., first, second] => {
                    let broke =
                        (made_change(first.get(Model::Utxo)), made_change(second.get(Model::Utxo)));
                    match broke {
                        (true, true) => Msg::TuneBothPart,
                        (false, false) => Msg::TuneBothWhole,
                        _ => Msg::TuneBookLine,
                    }
                    .text(language)
                    .to_string()
                }
                _ => Msg::TuneOnlyOne.text(language).to_string(),
            },
            Topic::RecapSends => self.recap_sends(language),
            Topic::RecapStorage => self.recap_storage(language),
            Topic::RecapAttack => self.recap_attack(language),
        }
    }

    /// `Transfers you sent: 5. Ones that broke a coin and made change: 3.`
    fn recap_sends(&self, language: Language) -> String {
        if self.sends.is_empty() {
            return Msg::RecapNothingSent.text(language).to_string();
        }
        let broke = self.sends.iter().filter(|sent| sent.made_change).count();
        format!(
            "{} {}. {} {}.",
            Msg::RecapSentCount.text(language),
            self.sends.len(),
            Msg::RecapBrokeCount.text(language),
            broke,
        )
    }

    /// `The most storage one of your transfers added: Bitcoin +64 B, Ethereum +36 B, Sui +69 B.`
    fn recap_storage(&self, language: Language) -> String {
        if self.sends.is_empty() {
            return Msg::RecapNothingSent.text(language).to_string();
        }
        let most: Vec<String> = Model::ALL
            .iter()
            .map(|model| {
                let largest =
                    self.sends.iter().map(|sent| *sent.size_delta.get(*model)).max().unwrap_or(0);
                format!("{} {}", phrases::short_model(*model).text(language), signed_bytes(largest))
            })
            .collect();
        format!("{} {}.", Msg::RecapStorage.text(language), most.join(", "))
    }

    /// How the reader's last double spend ended, or that they have not tried one.
    fn recap_attack(&self, language: Language) -> String {
        let Some(attack) = self.attacks.last() else {
            return Msg::RecapNoAttack.text(language).to_string();
        };
        let all_stopped = Model::ALL.iter().all(|model| *attack.stopped.get(*model));
        let mut reasons: Vec<Option<RejectionKind>> =
            Model::ALL.iter().map(|model| *attack.reason.get(*model)).collect();
        reasons.sort();
        reasons.dedup();
        match (all_stopped, reasons.len() == Model::ALL.len()) {
            (true, true) => Msg::RecapAttackThree,
            (true, false) => Msg::RecapAttackStopped,
            (false, _) => Msg::RecapAttackThrough,
        }
        .text(language)
        .to_string()
    }

    /// The name a ledger goes by right now. "UTXO" is a word the reader has to be given before it
    /// is used, so until the grow stage has said what it stands for, Bitcoin is just Bitcoin.
    fn model_name(&self, model: Model) -> Msg {
        if model == Model::Utxo && !self.utxo_named {
            phrases::short_model(model)
        } else {
            phrases::model(model)
        }
    }

    /// Whether the byte column may be drawn: everywhere but the opening stage, and there only once
    /// the conversation has said what a byte is.
    fn bytes_explained(&self) -> bool {
        self.stage != 0 || self.revealed >= COINS_BYTES_AT
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
                        let name = self.model_name(model).text(language);
                        beats.push(transfer_beat(name, side.get(model), language));
                    }
                }
            }
            Outcome::Double(reports) => {
                for model in Model::ALL {
                    let name = self.model_name(model).text(language);
                    beats.push(double_beat(name, reports.get(model), language));
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
    fn ledger_lines(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
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
        // Wrapped here rather than left to run off the edge: at 80 columns the panel is 38 wide
        // and this sentence was cut off at "how", in both languages.
        for part in wrap(Msg::LabelWholeState.text(language), width) {
            lines.push(Line::from(Span::styled(part, theme.muted())));
        }
        lines.push(Line::from(""));
        for (index, model) in Model::ALL.into_iter().enumerate() {
            let fact = facts.get(model);
            // The gap between blocks, not after the last one: at 80×24 the tuning stage fills
            // the panel to its last row, and a trailing blank is the row that falls off.
            if index > 0 {
                lines.push(Line::from(""));
            }
            lines.push(Line::from(Span::styled(
                self.model_name(model).text(language),
                theme.heading(),
            )));
            let bytes = if self.bytes_explained() {
                rpad(&format::bytes(fact.state_size_bytes), BYTES_COLUMN)
            } else {
                String::new()
            };
            lines.push(Line::from(vec![
                Span::styled("  ", theme.plain()),
                // The kind comes before the number so no language has to agree a plural: "coins 3"
                // and "lines 1" both read, where "1 lines" does not.
                Span::styled(
                    column(phrases::entry_kind(fact.entry_kind).text(language), LABEL_COLUMN),
                    theme.muted(),
                ),
                // Right-aligned so the numbers sit under each other: 192, 36 and 207 left-aligned
                // do not compare, which is the one thing this panel exists for.
                Span::styled(
                    rpad(&format::count(fact.entry_count as u64), COUNT_COLUMN),
                    theme.plain(),
                ),
                Span::styled(bytes, theme.plain()),
            ]));
            if let Some(reports) = double {
                lines.push(verdict_line(reports.get(model), language, theme));
            } else if let Some(side) = transfer {
                lines.push(change_line(side.get(model), self.bytes_explained(), language, theme));
            }
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
                    Span::styled(column(labels[index].text(language), 16), theme.plain()),
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
                Tell(topic) => beats.push(Beat::say(self.tell(*topic, index, language))),
            }
        }
        match &self.typed {
            Some((knob, Typed::PulledIn { to })) => {
                // The recipient is a choice, and the knob reports where it landed as the number
                // of the choice. "set to 1" beside a knob that reads "Carol" is two answers.
                let to = if *knob == 1 {
                    self.recipient_name().text(language).to_string()
                } else {
                    to.clone()
                };
                beats.push(Beat::outcome(
                    State::Chosen,
                    format!(
                        "{}  ·  {} {}",
                        Msg::EventOutsideRange.text(language),
                        Msg::EventSetTo.text(language),
                        to,
                    ),
                ))
            }
            Some((_, Typed::NotANumber)) => {
                beats.push(Beat::outcome(State::Bad, Msg::EventNotANumber.text(language)))
            }
            _ => {}
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
                self.typed = None;
                // A value changed since the last send asks for the send to be done again with
                // what is on screen. The sentences after a send are about that send.
                // Unless the next step is a send of its own: the values on screen are for that one,
                // and going back to redo the send before it rewrote a sentence already read.
                let next_is_a_send = matches!(self.script().get(self.revealed + 1), Some(Run(_)));
                if self.values_moved() && !next_is_a_send && self.rerun() == Reaction::Handled {
                    return Reaction::Handled;
                }
                let script = self.script();
                // The stage has said everything it has to say. Where the reader has knobs, Enter
                // sends again with the values now on screen — the conversation says "Enter sends
                // it", and a key that walks away instead has broken its own promise. Walking on
                // is Tab's job; where there is nothing to send, the shell walks.
                if self.revealed + 1 >= script.len() {
                    return self.rerun();
                }
                self.revealed += 1;
                if self.stage == STAGE_GROW && matches!(script[self.revealed], Say(Msg::GrowName)) {
                    self.utxo_named = true;
                }
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
                let stage = self.stage;
                self.sends.retain(|sent| sent.stage != stage);
                self.attacks.retain(|attack| attack.stage != stage);
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
                self.typed = None;
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
                if !self.tuning() {
                    return Reaction::Ignored;
                }
                // A number that goes nowhere reads as a broken key unless the screen says what
                // happened to it.
                let typed = self.knobs[self.chosen].commit();
                self.typed = matches!(typed, Typed::PulledIn { .. } | Typed::NotANumber)
                    .then_some((self.chosen, typed));
                Reaction::Handled
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
            let lines = self.ledger_lines(usize::from(ledgers.width), language, theme);
            frame.render_widget(Paragraph::new(lines), ledgers);
        } else {
            let lines = self.ledger_lines(usize::from(inner.width), language, theme);
            frame.render_widget(Paragraph::new(lines), inner);
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

    fn go_name(&self, language: Language) -> Option<&'static str> {
        (self.tuning() && (self.at_end() || self.values_moved()))
            .then(|| Msg::KeySend.text(language))
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

/// Cells the label at the head of a ledger row takes, after its two-cell indent.
const LABEL_COLUMN: usize = 14;
/// Cells the entry count takes, right-aligned.
const COUNT_COLUMN: usize = 4;
/// Cells the byte count takes, right-aligned.
const BYTES_COLUMN: usize = 12;

/// Whether this UTXO transfer broke a coin and paid change back.
///
/// Read off what the transaction wrote rather than off the amount: it writes every coin it spends
/// and every coin it makes, and reads only the ones it spends, so the difference is how many coins
/// it made. One is the payment; a second is the change.
fn made_change(utxo: &ApplyOutcome) -> bool {
    utxo.touched.write_count().saturating_sub(utxo.touched.read_count()) > 1
}

/// `+64 B`, `-128 B`, `+0 B`: a size change in the same unit as the column above it. Every ledger
/// here is well under a kibibyte, so bytes are the one unit and nothing is scaled.
fn signed_bytes(delta: i64) -> String {
    let sign = if delta < 0 { '-' } else { '+' };
    format!("{sign}{}", format::bytes(delta.unsigned_abs()))
}

/// `Bitcoin (UTXO)  accepted` — or, when it was not, where it died and why.
fn transfer_beat(name: &str, outcome: &ApplyOutcome, language: Language) -> Beat {
    let accepted = outcome.accepted();
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
fn double_beat(name: &str, report: &DoubleSpendReport, language: Language) -> Beat {
    let stopped = report.stopped();
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

/// What the last transfer cost this ledger, in entries and in bytes, each under its own column.
///
/// The label is "this send" rather than "change": two stages later "change" means the coins paid
/// back to Alice, and "change +1" over Bitcoin read as one coin of change. The numbers used to
/// trail the label wherever it ended, so they sat under nothing; they are now put in the count and
/// byte columns of the row above, with the verdict carried by the mark.
fn change_line(
    outcome: &ApplyOutcome,
    with_bytes: bool,
    language: Language,
    theme: Theme,
) -> Line<'static> {
    let accepted = outcome.accepted();
    let state = if accepted { State::Good } else { State::Bad };
    let mark = Span::styled(format!("  {} ", state.mark()), theme.state(state));
    if !accepted {
        return Line::from(vec![
            mark,
            Span::styled(Msg::LabelRejected.text(language).to_string(), theme.state(state)),
        ]);
    }
    // The mark and its gap take two of the label column's cells.
    let label = column(Msg::ColumnChange.text(language), LABEL_COLUMN - 2);
    let mut spans = vec![
        mark,
        Span::styled(label, theme.muted()),
        Span::styled(rpad(&format!("{:+}", outcome.entry_delta()), COUNT_COLUMN), theme.muted()),
    ];
    if with_bytes {
        spans.push(Span::styled(
            rpad(&signed_bytes(outcome.size_delta()), BYTES_COLUMN),
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
            let lines = session.ledger_lines(38, *language, Theme::new(true));
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
        assert_eq!(session.tell(Topic::Sent, TUNE_SENT, english), Msg::TuneOnlyOne.text(english));

        // A whole coin: nothing to break.
        set_amount(&mut session, 10);
        session.perform(TUNE_FIRST_RUN, SendChosen);
        let sent = session.tell(Topic::Sent, TUNE_SENT, english);
        assert!(sent.starts_with(Msg::TuneSentWhole.text(english)), "{sent}");
        assert_eq!(
            session.tell(Topic::Compared, TUNE_COMPARED, english),
            Msg::TuneOnlyOne.text(english)
        );

        // Less than a coin: change has to go somewhere.
        set_amount(&mut session, 7);
        session.perform(TUNE_SECOND_RUN, SendChosen);
        assert_eq!(
            session.tell(Topic::Compared, TUNE_COMPARED, english),
            Msg::TuneBookLine.text(english)
        );
        // The sentence after the first send is still about the first send. It used to read "the
        // last send", and changed to describe the 7 the moment the 7 went in.
        let first = session.tell(Topic::Sent, TUNE_SENT, english);
        assert!(first.starts_with(Msg::TuneSentWhole.text(english)), "{first}");
    }

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
        draw_in(session, Language::ENGLISH)
    }

    fn draw_in(session: &Session, language: Language) -> String {
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
                session.render(frame, panel, Theme::new(true), language);
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
            Some(RejectionKind::StaleObjectVersion),
            "the quest says the transfer names a version the coin has left behind"
        );
        // The step each verdict names: "looking it up", then "checking it is current" twice.
        assert_eq!(
            reports.get(Model::Utxo).stopped_at(),
            Some(nmtk_ledger::CheckStep::StateLookup)
        );
        assert_eq!(
            reports.get(Model::Account).stopped_at(),
            Some(nmtk_ledger::CheckStep::Freshness)
        );
        assert_eq!(
            reports.get(Model::Object).stopped_at(),
            Some(nmtk_ledger::CheckStep::Freshness)
        );
        // "The first one goes in everywhere."
        for (model, report) in reports.iter() {
            assert!(report.first.accepted(), "{model:?} refused the first, honest transfer");
        }
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
    fn enter_at_the_end_of_the_sending_stage_sends_again_rather_than_walking_away() {
        let mut session = Session::new();
        session.go_to(3);
        for _ in 0..SCRIPTS[3].len() {
            if session.at_end() {
                break;
            }
            session.on(Action::Go);
        }
        assert!(session.at_end(), "the stage should have said everything by now");
        let entries = session.done.len();
        let before = session.amount();
        session.chosen = 0;
        session.on(Action::Nudge(1));
        assert_ne!(session.amount(), before, "the arrow did not move the amount");
        assert_eq!(session.on(Action::Go), Reaction::Handled, "Enter walked away instead");
        assert_eq!(session.done.len(), entries, "the send was added instead of replaced");
        assert!(
            session.kept.as_ref().is_some_and(|kept| kept.stage == 3),
            "the send that was kept is not this stage's"
        );
    }

    #[test]
    fn a_typed_amount_is_taken_and_one_past_the_end_lands_on_the_end() {
        let mut session = Session::new();
        session.go_to(3);
        for c in "25".chars() {
            session.on(Action::Type(c));
        }
        assert!(session.typing());
        session.on(Action::Commit);
        assert_eq!(session.amount(), 25);

        // Silently putting 25 back reads as a broken key. The number lands on the end of the
        // range instead, and the conversation says where it landed.
        for c in "900".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.amount(), 30, "a number past the end should land on the end");
        let said = session.transcript(Language::ENGLISH);
        let last = said.last().expect("the conversation said nothing about it");
        assert!(
            last.text.contains("30"),
            "the reader was not told where the number landed: {:?}",
            last.text
        );
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

    /// Where the tuning stage's runs and tells sit in its script.
    const TUNE_FIRST_RUN: usize = 5;
    const TUNE_SENT: usize = 6;
    const TUNE_SECOND_RUN: usize = 8;
    const TUNE_COMPARED: usize = 9;

    #[test]
    fn the_tuning_steps_the_tests_name_are_where_the_script_has_them() {
        assert!(matches!(TUNE[TUNE_FIRST_RUN], Run(SendChosen)));
        assert!(matches!(TUNE[TUNE_SENT], Tell(Topic::Sent)));
        assert!(matches!(TUNE[TUNE_SECOND_RUN], Run(SendChosen)));
        assert!(matches!(TUNE[TUNE_COMPARED], Tell(Topic::Compared)));
        assert!(matches!(COINS[COINS_BYTES_AT], Say(Msg::CoinsBytes)));
    }

    /// Sentences in a piece of text: a full stop, question or exclamation mark followed by a space
    /// or the end. A decimal point is followed by a digit and is not counted.
    fn sentences(text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        chars
            .iter()
            .enumerate()
            .filter(|(i, c)| {
                matches!(c, '.' | '?' | '!') && chars.get(i + 1).is_none_or(|next| *next == ' ')
            })
            .count()
    }

    /// §1: one or two sentences, under ~160 characters — for every line in the table, in both
    /// columns. Eleven Korean lines ran to three or four sentences where the English had two.
    #[test]
    fn every_phrase_is_one_or_two_sentences_in_both_languages() {
        for message in Msg::ALL {
            for language in Language::ALL {
                let text = message.text(*language);
                assert!(text.chars().count() <= 160, "{message:?} in {language} is long: {text}");
                assert!(sentences(text) <= 2, "{message:?} in {language} is a paragraph: {text}");
            }
        }
    }

    /// Every English line has a Korean one, and it is not the English left in place.
    #[test]
    fn every_phrase_has_its_own_korean() {
        for message in Msg::ALL {
            let english = message.text(Language::ENGLISH);
            let korean = message.text(Language::KOREAN);
            // A name that is written the same in both.
            if *message == Msg::ShortObject {
                continue;
            }
            assert_ne!(english, korean, "{message:?} has no Korean");
        }
    }

    /// Composed beats — tells, verdicts, typed numbers — have to pass the same test as the table.
    #[test]
    fn every_beat_of_the_whole_quest_is_one_or_two_sentences() {
        let mut session = Session::new();
        for stage in 0..SCRIPTS.len() {
            session.go_to(stage);
            walk(&mut session);
            for language in Language::ALL {
                for beat in session.transcript(*language) {
                    assert!(
                        sentences(&beat.text) <= 2,
                        "stage {stage} in {language} is a paragraph: {:?}",
                        beat.text
                    );
                    assert!(beat.text.chars().count() <= 160, "{:?}", beat.text);
                }
            }
        }
    }

    /// "Nothing new was stored", "grew by one line", "nothing new either" — against the engine.
    #[test]
    fn what_is_said_about_one_whole_coin_is_what_the_engine_did() {
        let mut session = Session::new();
        session.go_to(1);
        walk(&mut session);
        let side = session.latest_transfer().expect("stage 2 never sent anything");
        assert_eq!(side.get(Model::Utxo).entry_delta(), 0, "SendUtxo: nothing new was stored");
        assert_eq!(side.get(Model::Account).entry_delta(), 1, "SendAccount: one line for Bob");
        assert_eq!(side.get(Model::Object).entry_delta(), 0, "SendObject: nothing new either");
        assert!(!made_change(side.get(Model::Utxo)), "one whole coin made change");
    }

    /// "The ten was destroyed and two new coins were made: three for Bob, seven back to Alice."
    #[test]
    fn what_is_said_about_change_is_what_the_engine_did() {
        let mut session = Session::new();
        session.go_to(2);
        walk(&mut session);
        let utxo = session.latest_transfer().expect("stage 3 never sent anything").get(Model::Utxo);
        assert_eq!(utxo.touched.read_count(), 1, "more than one coin was destroyed");
        assert_eq!(utxo.touched.write_count() - utxo.touched.read_count(), 2, "not two new coins");
        let alice = session.scenario.holdings(&session.alice.address());
        let mut values: Vec<Amount> = alice.utxo.iter().map(|holding| holding.value).collect();
        values.sort_unstable();
        assert_eq!(values, [7, 10, 10], "seven did not come back to Alice");
        assert_eq!(session.scenario.balances(&session.bob).utxo, 3);
    }

    /// "Alice has three coins of ten", and "right now they already disagree" about the space.
    #[test]
    fn the_opening_ledgers_hold_what_the_first_stage_says() {
        let session = Session::new();
        let alice = session.scenario.holdings(&session.alice.address());
        for coins in [&alice.utxo, &alice.object] {
            assert_eq!(coins.iter().map(|holding| holding.value).collect::<Vec<_>>(), [10, 10, 10]);
        }
        let sizes = session.scenario.state_sizes();
        assert!(sizes.utxo != sizes.account && sizes.account != sizes.object);
        assert!(sizes.utxo != sizes.object);
    }

    /// 17 breaks a coin and still leaves Bitcoin's coin count where it was; 20 breaks none and
    /// shrinks it. The old sentence read the count, so it told the reader who sent 17 that nothing
    /// was broken, and the reader who sent 20 that a coin was broken and Bitcoin's side grew.
    #[test]
    fn whether_a_coin_was_broken_is_read_from_the_transfer_not_the_coin_count() {
        let english = Language::ENGLISH;
        for (amount, broke, before, after) in [(17, true, 3, 3), (20, false, 3, 2), (3, true, 3, 4)]
        {
            let mut session = Session::new();
            session.go_to(3);
            set_amount(&mut session, amount);
            session.perform(TUNE_FIRST_RUN, SendChosen);
            let said = session.tell(Topic::Sent, TUNE_SENT, english);
            let expected = if broke { Msg::TuneSentPart } else { Msg::TuneSentWhole };
            assert!(said.starts_with(expected.text(english)), "{amount}: {said}");
            assert!(said.ends_with(&format!("{before} → {after}.")), "{amount}: {said}");
        }
    }

    /// Two sends that do not differ in the one way that matters are not a comparison, and the
    /// sentence after them says so rather than drawing the lesson anyway.
    #[test]
    fn two_sends_that_show_no_difference_are_told_so() {
        let english = Language::ENGLISH;
        for (first, second, expected) in
            [(10, 20, Msg::TuneBothWhole), (3, 7, Msg::TuneBothPart), (7, 10, Msg::TuneBookLine)]
        {
            let mut session = Session::new();
            session.go_to(3);
            set_amount(&mut session, first);
            session.perform(TUNE_FIRST_RUN, SendChosen);
            set_amount(&mut session, second);
            session.perform(TUNE_SECOND_RUN, SendChosen);
            assert_eq!(
                session.tell(Topic::Compared, TUNE_COMPARED, english),
                expected.text(english),
                "{first} then {second}"
            );
            // "The book gained a line both times, for the person paid, whatever the amount."
            for side in session.sends_before(TUNE_COMPARED + 1) {
                assert_eq!(side.get(Model::Account).entry_delta(), 1);
            }
        }
    }

    /// The recap reads the record of the whole quest. A reader who sent two coins and attacked
    /// once is told so; a reader who pressed Tab straight to the end is not told they did anything.
    #[test]
    fn the_recap_is_built_from_everything_the_reader_did() {
        let english = Language::ENGLISH;
        let recap_line = |session: &Session, topic: Topic| {
            let at = RECAP.iter().position(|step| matches!(step, Tell(t) if std::mem::discriminant(t) == std::mem::discriminant(&topic))).expect("the recap tells it");
            session.tell(topic, at, english)
        };

        let mut jumped = Session::new();
        jumped.go_to(STAGE_RECAP);
        assert_eq!(recap_line(&jumped, Topic::RecapSends), Msg::RecapNothingSent.text(english));
        assert_eq!(recap_line(&jumped, Topic::RecapAttack), Msg::RecapNoAttack.text(english));

        let mut session = Session::new();
        for stage in [1, 2, 4] {
            session.go_to(stage);
            walk(&mut session);
        }
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        assert_eq!(
            recap_line(&session, Topic::RecapSends),
            "Transfers you sent: 2. Ones that broke a coin and made change: 1."
        );
        assert_eq!(
            recap_line(&session, Topic::RecapStorage),
            "The most storage one of your transfers added: Bitcoin +64 B, Ethereum +36 B, Sui +69 B."
        );
        assert_eq!(recap_line(&session, Topic::RecapAttack), Msg::RecapAttackThree.text(english));

        // `r` on the grow stage forgets the send made there, and nothing else.
        session.go_to(2);
        session.on(Action::Reset);
        session.go_to(STAGE_RECAP);
        assert!(recap_line(&session, Topic::RecapSends).starts_with("Transfers you sent: 1."));
        assert_eq!(recap_line(&session, Topic::RecapAttack), Msg::RecapAttackThree.text(english));
    }

    /// Everything on screen but the gaps, so text drawn across wide glyphs can be searched.
    fn squeezed(text: &str) -> String {
        text.chars().filter(|c| !c.is_whitespace() && *c != '│').collect()
    }

    /// §9: at 80×24, in both languages, the panel shows every block whole. The note over the
    /// counts ran off the edge at "how", and the tuning stage's last row was a blank that pushed
    /// nothing off only because nothing came after it.
    #[test]
    fn the_panel_reads_whole_at_80_by_24_in_both_languages() {
        for stage in 0..SCRIPTS.len() {
            for language in Language::ALL {
                let mut session = Session::new();
                session.go_to(4);
                walk(&mut session);
                session.go_to(stage);
                walk(&mut session);
                let drawn = squeezed(&draw_in(&session, *language));
                let note = squeezed(Msg::LabelWholeState.text(*language));
                assert!(
                    drawn.contains(&note),
                    "stage {stage} in {language} cut the note:\n{drawn}"
                );
                for model in Model::ALL {
                    let name = squeezed(session.model_name(model).text(*language));
                    assert!(drawn.contains(&name), "stage {stage} in {language} lost {model:?}");
                }
                if session.tuning() {
                    let label = squeezed(Msg::ColumnChange.text(*language));
                    assert_eq!(
                        drawn.matches(&label).count(),
                        3,
                        "stage {stage} in {language} lost a ledger's last row:\n{drawn}"
                    );
                }
            }
        }
    }

    /// The change row's numbers sit under the count and byte columns of the row above, in both
    /// languages, rather than wherever its label happened to end.
    #[test]
    fn the_change_row_lines_up_with_the_counts_above_it() {
        let mut session = Session::new();
        session.go_to(3);
        session.perform(TUNE_FIRST_RUN, SendChosen);
        for language in Language::ALL {
            let lines = session.ledger_lines(38, *language, Theme::new(false));
            let rows: Vec<String> = lines
                .iter()
                .map(|line| line.spans.iter().map(|span| span.content.as_ref()).collect())
                .collect();
            let kind = phrases::entry_kind(nmtk_ledger::EntryKind::UnspentOutput).text(*language);
            let at = rows
                .iter()
                .position(|row| row.trim_start().starts_with(kind))
                .expect("Bitcoin's row is drawn");
            let (counts, change) = (&rows[at], &rows[at + 1]);
            assert_eq!(
                nmtk_kq::text::width(counts),
                nmtk_kq::text::width(change),
                "in {language}:\n{counts}\n{change}"
            );
        }
    }

    /// "B" is on the panel from the first press, and the sentence saying what it means is the
    /// eighth. The byte column waits for it, and "UTXO" waits for the stage that spells it out.
    #[test]
    fn no_word_reaches_the_panel_before_it_is_explained() {
        let mut session = Session::new();
        let opening = draw(&session);
        assert!(!opening.contains(" B "), "bytes drawn before they are explained:\n{opening}");
        assert!(!opening.contains("UTXO"), "UTXO drawn before it is explained:\n{opening}");
        walk(&mut session);
        assert!(draw(&session).contains("192 B"), "the byte column never appeared");

        session.go_to(STAGE_GROW);
        walk(&mut session);
        assert!(draw(&session).contains("Bitcoin (UTXO)"), "the heading never took the acronym");
    }

    /// The recipient is a choice. A number typed past its end used to be reported as "set to 1"
    /// beside a knob that read "Carol".
    #[test]
    fn a_recipient_typed_out_of_range_is_named_where_it_landed() {
        let mut session = Session::new();
        session.go_to(3);
        session.on(Action::Next);
        session.on(Action::Type('5'));
        session.on(Action::Commit);
        let said = session.transcript(Language::ENGLISH);
        let last = &said.last().expect("the conversation said nothing").text;
        assert!(last.ends_with("Carol"), "{last}");
        assert_eq!(session.recipient(), session.carol);
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
