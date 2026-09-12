//! The quest a reader has opened: five stages driven by one mining engine.
//!
//! The engine (`nmtk-pow`) owns the threads and hands back a snapshot; this file turns that
//! snapshot into a screen. Nothing here hashes anything, and nothing in the engine says a word.

use std::time::Duration;

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue};
use nmtk_kq::session::{Action, Beat, KqSession, Reaction, RunState};
use nmtk_kq::text::{column, rpad, truncate, width as cells, wrap};
use nmtk_kq::theme::{State, Theme};
use nmtk_kq::widgets::{self, stat_lines, wrapped};
use nmtk_pow::{
    AttackConfig, AttackHandle, AttackOutcome, AttackPhase, AttackSnapshot,
    BITCOIN_TARGET_BLOCK_SECONDS, BlockSummary, Hash256, MinerSpec, MiningConfig, MiningHandle,
    MiningSnapshot, Share, Target, effective_shares, expected_time_to_block,
    implied_network_hashrate, practice_bits, split_threads, start_attack, start_mining,
};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::phrases::{self, Msg};

/// Practice difficulties worth trying, in leading zero bits. A reader who wants 23 types 23.
const DIFFICULTY_PRESETS: &[u64] = &[18, 20, 22, 24, 26];
/// Miner counts worth trying.
const MINER_PRESETS: &[u64] = &[1, 2, 3, 4];
/// Thread counts worth trying.
const THREAD_PRESETS: &[u64] = &[1, 2, 4, 8];
/// Shares worth trying, as fractions of the whole machine.
const SHARE_PRESETS: &[f64] = &[0.10, 0.25, 0.50, 0.75];
/// Attacker shares worth trying. 30% loses, 51% wins, and both are the lesson.
const ATTACKER_PRESETS: &[f64] = &[0.20, 0.30, 0.45, 0.51, 0.70];
/// How many blocks a merchant might wait for.
const CONFIRMATION_PRESETS: &[u64] = &[1, 2, 3, 6];

/// More miners than this would not fit the panel, and four is enough to see a share split.
const MAX_MINERS: usize = 4;
/// A thread budget past this is a typo, not a choice.
const MAX_THREADS: u64 = 64;
/// Where the practice difficulty starts on a machine with one thread: about sixteen million
/// hashes a block, which is a few seconds. Every doubling of the threads adds a bit.
const BASE_ZERO_BITS: u64 = 24;
/// Past this a practice block stops being something anyone waits for.
const MAX_ZERO_BITS: u64 = 28;
/// Stable knob ids, one per miner.
const SHARE_IDS: [&str; MAX_MINERS] = ["share-1", "share-2", "share-3", "share-4"];
/// What a miner's share starts at. The first two differ so the split has something to say.
const DEFAULT_SHARES: [f64; MAX_MINERS] = [0.60, 0.40, 0.50, 0.50];
/// How many hash-rate samples the curve keeps. Ten a second, so this is the last twelve seconds.
const RATE_SAMPLES: usize = 120;
/// The panel needs this many rows before a hash-rate curve is worth the space it costs.
const CURVE_NEEDS_ROWS: u16 = 22;

/// Where each knob sits in the Tune stage.
const KNOB_DIFFICULTY: usize = 0;
const KNOB_MINERS: usize = 1;
const KNOB_THREADS: usize = 2;
const KNOB_SHARES_FROM: usize = 3;

/// Where each knob sits in the Break stage.
const KNOB_ATTACKER: usize = 0;
const KNOB_CONFIRMATIONS: usize = 1;

/// Where each stage sits. The order here is the order `lib.rs` declares them in.
const STAGE_WHY: usize = 0;
const STAGE_PUZZLE: usize = 1;
const STAGE_MINE: usize = 2;
const STAGE_TUNE: usize = 3;
const STAGE_ATTACK: usize = 4;
#[cfg(test)]
const STAGE_RECAP: usize = 5;

/// The most events one stage will report before it stops reporting them. Blocks keep arriving
/// while a reader reads, and a conversation that grows without end is a log, not a conversation.
const EVENT_CAP: usize = 24;

/// One move in a stage's conversation.
#[derive(Debug, Clone, Copy)]
enum Step {
    /// The quest says one thing.
    Say(Msg),
    /// The quest asks the reader to do something before pressing Enter.
    Ask(Msg),
    /// Real work, started the moment this step is reached.
    Run(Deed),
    /// The conversation waits here until the machine has done something. Enter does nothing; the
    /// beats that arrive meanwhile are the answer.
    Await(Until),
    /// One sentence, chosen from what the run actually did.
    Tell(Topic),
}

/// What a [`Step::Tell`] is about.
///
/// The share is the reader's to set, and an Ask is a suggestion rather than a gate. A fixed
/// sentence after the run stated the outcome of the suggested share: "above half it catches up"
/// was printed under an attacker holding 27% of the machine, which had won by luck.
#[derive(Debug, Clone, Copy)]
enum Topic {
    /// How the attack that just finished ended, and at what share.
    Attack,
    /// What moving the difficulty did to the cost of a block.
    Difficulty,
}

/// Work a step starts.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Deed {
    /// Mine with whatever the knobs say.
    Mine,
    /// Run the 51% attack with whatever the knobs say.
    Attack,
}

/// What a waiting step is waiting for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Until {
    /// This many blocks found since the stage started.
    Blocks(u64),
    /// The attack has ended, one way or the other.
    AttackOver,
}

use Deed::{Attack, Mine};
use Step::{Ask, Await, Run, Say, Tell};

/// Why a network burns electricity to agree on anything.
const WHY: &[Step] =
    &[Say(Msg::WhyOne), Say(Msg::WhyTwo), Say(Msg::WhyThree), Say(Msg::WhyFour), Say(Msg::WhyFive)];

/// What the puzzle actually is, with what it costs on the right.
const PUZZLE: &[Step] = &[
    Say(Msg::PuzzleOne),
    Say(Msg::PuzzleTwo),
    Say(Msg::PuzzleThree),
    Say(Msg::PuzzleFour),
    Say(Msg::PuzzleFive),
    Say(Msg::PuzzleSix),
    Say(Msg::PuzzleSeven),
];

/// The machine does it, and the conversation waits for the blocks.
const MINE_STAGE: &[Step] = &[
    Say(Msg::MineOne),
    Ask(Msg::MineAsk),
    Run(Mine),
    Await(Until::Blocks(1)),
    Say(Msg::MineTwo),
    Say(Msg::MineThree),
    Await(Until::Blocks(3)),
    Say(Msg::MineFour),
    Say(Msg::MineFive),
    Say(Msg::MineSix),
    Say(Msg::MineSeven),
    Say(Msg::MineEight),
    Say(Msg::MineNine),
    Say(Msg::MineTen),
    Say(Msg::MineEleven),
];

/// The reader's own numbers, twice, each answered by a real run.
const TUNE_STAGE: &[Step] = &[
    Say(Msg::TuneOne),
    Say(Msg::TuneKeys),
    Say(Msg::TuneBits),
    Ask(Msg::TuneAskBits),
    Run(Mine),
    Await(Until::Blocks(1)),
    Tell(Topic::Difficulty),
    Say(Msg::TuneWhole),
    Ask(Msg::TuneAskMiners),
    Run(Mine),
    Await(Until::Blocks(1)),
    Say(Msg::TuneAsked),
    Say(Msg::TuneAddUp),
    Say(Msg::TuneOver),
];

/// The attack, lost at 30% and won at 51%.
const ATTACK_STAGE: &[Step] = &[
    Say(Msg::AttackOne),
    Say(Msg::AttackTwo),
    Say(Msg::AttackThree),
    Say(Msg::AttackFour),
    Say(Msg::AttackFive),
    Say(Msg::AttackSix),
    Say(Msg::AttackSeven),
    Ask(Msg::AttackAskLow),
    Run(Attack),
    Await(Until::AttackOver),
    Tell(Topic::Attack),
    Ask(Msg::AttackAskHigh),
    Run(Attack),
    Await(Until::AttackOver),
    Tell(Topic::Attack),
    Say(Msg::AttackWhyName),
];

/// What the reader now knows, beside the numbers they made.
const RECAP_STAGE: &[Step] = &[
    Say(Msg::RecapOne),
    Say(Msg::RecapTwo),
    Say(Msg::RecapThree),
    Say(Msg::RecapFour),
    Say(Msg::RecapFive),
    Say(Msg::RecapSix),
];

const SCRIPTS: [&[Step]; 6] = [WHY, PUZZLE, MINE_STAGE, TUNE_STAGE, ATTACK_STAGE, RECAP_STAGE];

/// Something that happened, kept as numbers so the conversation can be said again in any language.
enum Happening {
    MiningStarted { miners: usize, threads: usize, bits: u64 },
    Block { height: u64, gap: Duration, miner: usize, on_the_chain: bool },
    FallingBehind { by: u64, elapsed: Duration },
    AttackStarted { share: f64, confirmations: u64 },
    Paid,
    Released { confirmations: u64 },
    Ended { outcome: AttackOutcome, reverted: u64, took: Option<Duration> },
    Refused(Msg),
}

/// One happening, filed against the step the reader was on when it happened.
struct Logged {
    step: usize,
    what: Happening,
}

/// The quest a reader has opened.
pub struct Session {
    stage: usize,
    /// How many steps of this stage have been revealed. Step zero shows the moment it opens.
    revealed: usize,
    /// Everything that happened in this stage, oldest first.
    log: Vec<Logged>,
    /// Blocks the log has already reported, so the same block is never said twice.
    reported_blocks: u64,
    /// The blocks already spoken about, by hash.
    ///
    /// Height is not enough: two miners can solve the same height, and both are real.
    said_blocks: std::collections::HashSet<Hash256>,
    /// The deficit the conversation has already remarked on, so a long losing attack says
    /// something every few blocks instead of going quiet for two minutes.
    said_deficit: u64,
    /// Whether the attack's milestones have been said yet.
    said_paid: bool,
    said_released: bool,
    said_ended: bool,
    tune_knobs: Vec<Knob>,
    break_knobs: Vec<Knob>,
    chosen: usize,
    mining: Option<MiningHandle>,
    /// The difficulty as it stood when the tuning stage opened, so "you moved it" can be checked
    /// rather than assumed.
    bits_on_entry: u32,
    mining_snapshot: Option<MiningSnapshot>,
    attack: Option<AttackHandle>,
    attack_snapshot: Option<AttackSnapshot>,
    rates: Vec<f64>,
    state: RunState,
    refused: Option<Msg>,
}

impl Session {
    /// Opens the quest, sized to the machine it was handed.
    pub fn new(machine: &MachineProfile) -> Self {
        let threads = (machine.default_worker_threads() as u64).clamp(1, MAX_THREADS);
        // Twice the threads, twice the hashes a second, so a block stays a few seconds long.
        let doublings = u64::from(u64::BITS - 1 - threads.leading_zeros());
        let zero_bits = (BASE_ZERO_BITS + doublings).min(MAX_ZERO_BITS);
        let mut session = Self {
            stage: 0,
            revealed: 0,
            log: Vec::new(),
            reported_blocks: 0,
            said_blocks: std::collections::HashSet::new(),
            said_deficit: 0,
            said_paid: false,
            said_released: false,
            said_ended: false,
            tune_knobs: vec![
                Knob::new(
                    "difficulty",
                    KnobValue::Count {
                        current: zero_bits,
                        min: 16,
                        max: MAX_ZERO_BITS,
                        step: 1,
                        presets: DIFFICULTY_PRESETS,
                    },
                ),
                Knob::new(
                    "miners",
                    KnobValue::Count {
                        current: 2,
                        min: 1,
                        max: MAX_MINERS as u64,
                        step: 1,
                        presets: MINER_PRESETS,
                    },
                ),
                Knob::new(
                    "threads",
                    KnobValue::Count {
                        current: threads,
                        min: 1,
                        max: MAX_THREADS,
                        step: 1,
                        presets: THREAD_PRESETS,
                    },
                ),
            ],
            break_knobs: vec![
                Knob::new(
                    "attacker-share",
                    KnobValue::Share {
                        // The conversation asks for 30% first and 51% second, so this is where it
                        // starts: a reader who just presses Enter watches the attack lose, which
                        // is the half of the lesson people skip.
                        current: 0.30,
                        min: 0.01,
                        max: 0.99,
                        step: 0.01,
                        presets: ATTACKER_PRESETS,
                    },
                ),
                Knob::new(
                    "confirmations",
                    KnobValue::Count {
                        current: 2,
                        min: 1,
                        max: 12,
                        step: 1,
                        presets: CONFIRMATION_PRESETS,
                    },
                ),
            ],
            chosen: 0,
            mining: None,
            bits_on_entry: zero_bits as u32,
            mining_snapshot: None,
            attack: None,
            attack_snapshot: None,
            rates: Vec::new(),
            state: RunState::Idle,
            refused: None,
        };
        session.sync_share_knobs();
        session
    }

    // ---- What the knobs say ----------------------------------------------------

    fn zero_bits(&self) -> u32 {
        count_of(self.tune_knobs.get(KNOB_DIFFICULTY)).min(u32::MAX as u64) as u32
    }

    fn miner_count(&self) -> usize {
        (count_of(self.tune_knobs.get(KNOB_MINERS)) as usize).clamp(1, MAX_MINERS)
    }

    fn thread_budget(&self) -> usize {
        count_of(self.tune_knobs.get(KNOB_THREADS)).max(1) as usize
    }

    fn miner_share(&self, index: usize) -> f64 {
        share_of(self.tune_knobs.get(KNOB_SHARES_FROM + index)).max(0.0)
    }

    fn attacker_share(&self) -> f64 {
        share_of(self.break_knobs.get(KNOB_ATTACKER)).clamp(0.01, 0.99)
    }

    fn confirmations(&self) -> u64 {
        count_of(self.break_knobs.get(KNOB_CONFIRMATIONS)).max(1)
    }

    /// One share knob per miner, no more and no fewer. Values already set are kept.
    fn sync_share_knobs(&mut self) {
        let wanted = KNOB_SHARES_FROM + self.miner_count();
        while self.tune_knobs.len() > wanted {
            self.tune_knobs.pop();
        }
        while self.tune_knobs.len() < wanted {
            let index = (self.tune_knobs.len() - KNOB_SHARES_FROM).min(MAX_MINERS - 1);
            self.tune_knobs.push(Knob::new(
                SHARE_IDS[index],
                KnobValue::Share {
                    current: DEFAULT_SHARES[index],
                    min: 0.01,
                    max: 1.0,
                    step: 0.05,
                    presets: SHARE_PRESETS,
                },
            ));
        }
        let last = self.knob_count().saturating_sub(1);
        self.chosen = self.chosen.min(last);
    }

    fn knob_count(&self) -> usize {
        match self.stage {
            STAGE_TUNE => self.tune_knobs.len(),
            STAGE_ATTACK => self.break_knobs.len(),
            _ => 0,
        }
    }

    fn move_choice(&mut self, step: i32) {
        let count = self.knob_count();
        if count == 0 {
            return;
        }
        let last = count - 1;
        self.chosen = if step > 0 {
            if self.chosen >= last { 0 } else { self.chosen + 1 }
        } else if self.chosen == 0 {
            last
        } else {
            self.chosen - 1
        };
    }

    /// Hands the chosen knob to `change`, then keeps the share knobs in step with the miner count.
    fn change_chosen(&mut self, change: impl FnOnce(&mut Knob)) -> Reaction {
        let index = self.chosen;
        let knobs = match self.stage {
            STAGE_TUNE => &mut self.tune_knobs,
            STAGE_ATTACK => &mut self.break_knobs,
            _ => return Reaction::Ignored,
        };
        match knobs.get_mut(index) {
            Some(knob) => change(knob),
            None => return Reaction::Ignored,
        }
        if self.stage == STAGE_TUNE && index == KNOB_MINERS {
            self.sync_share_knobs();
        }
        Reaction::Handled
    }

    // ---- Running the engine ----------------------------------------------------

    /// The miners the current knobs describe.
    fn specs(&self) -> Vec<MinerSpec> {
        (0..self.miner_count())
            .map(|index| MinerSpec::percent(index as u32, self.miner_share(index) * 100.0))
            .collect()
    }

    /// Stops whatever is running and starts mining with the current numbers.
    fn start_run(&mut self) {
        self.stop_attack();
        self.stop_mining();
        // The run about to start replaces the last one's numbers, and a start that is refused
        // leaves none behind: a wait for blocks must never be answered by a run already over.
        self.mining_snapshot = None;
        self.rates.clear();
        let bits = match practice_bits(self.zero_bits()) {
            Ok(bits) => bits,
            Err(error) => {
                self.refused = Some(phrases::target_error(error));
                return;
            }
        };
        let config = MiningConfig::new(bits, self.specs(), self.thread_budget());
        // Only one heavy run at a time: mining beside an attack halves both and teaches nothing.
        self.stop_attack();
        match start_mining(config) {
            Ok(handle) => {
                self.mining_snapshot = Some(handle.snapshot());
                self.mining = Some(handle);
                self.refused = None;
                self.state = RunState::Running;
            }
            Err(error) => {
                self.refused = Some(phrases::config_error(error));
                self.state = RunState::Idle;
            }
        }
    }

    /// Stops whatever is running and starts the 51% attack with the current numbers.
    fn start_the_attack(&mut self) {
        self.stop_mining();
        self.stop_attack();
        // As with mining: a refused start leaves no snapshot, so the wait for an ending cannot be
        // answered by the ending of the attack before it.
        self.attack_snapshot = None;
        let bits = match practice_bits(self.zero_bits()) {
            Ok(bits) => bits,
            Err(error) => {
                self.refused = Some(phrases::target_error(error));
                return;
            }
        };
        let (attacker, honest) = self.attack_threads();
        let config =
            AttackConfig::new(bits, attacker, honest).with_confirmations(self.confirmations());
        self.stop_mining();
        match start_attack(config) {
            Ok(handle) => {
                self.attack_snapshot = Some(handle.snapshot());
                self.attack = Some(handle);
                self.refused = None;
                self.state = RunState::Running;
            }
            Err(error) => {
                self.refused = Some(phrases::config_error(error));
                self.state = RunState::Idle;
            }
        }
    }

    /// The threads each side of the attack gets. Both sides need at least one.
    fn attack_threads(&self) -> (usize, usize) {
        let total = self.thread_budget().max(2);
        let wanted = (total as f64 * self.attacker_share()).round() as usize;
        let attacker = wanted.clamp(1, total - 1);
        (attacker, total - attacker)
    }

    fn stop_mining(&mut self) {
        if let Some(handle) = self.mining.take() {
            handle.stop();
            self.state = RunState::Idle;
        }
    }

    fn stop_attack(&mut self) {
        if let Some(handle) = self.attack.take() {
            handle.stop();
            self.state = RunState::Idle;
        }
    }

    /// Stops every thread this quest has running, and keeps what they produced.
    ///
    /// Walking out of a stage has to stop the work — one heavy run at a time on one machine — but
    /// the numbers a finished run left behind are what the recap is about, so they stay.
    fn stop_runs(&mut self) {
        self.stop_mining();
        self.stop_attack();
        self.state = RunState::Idle;
    }

    /// Forgets what is being said about a run in progress, without touching what finished runs
    /// produced.
    fn forget_run(&mut self) {
        self.reported_blocks = 0;
        self.said_blocks.clear();
        self.said_deficit = 0;
        self.said_paid = false;
        self.said_released = false;
        self.said_ended = false;
        self.refused = None;
    }

    /// Throws away the numbers the stage showing now produced, and only those. Pressing `r` is
    /// the one thing that does this: every other way out of a stage keeps them.
    ///
    /// The knobs keep their values either way: a reader who set a number wants it kept.
    fn forget_results(&mut self) {
        match self.stage {
            STAGE_MINE | STAGE_TUNE => {
                self.mining_snapshot = None;
                self.rates.clear();
            }
            STAGE_ATTACK => self.attack_snapshot = None,
            _ => {}
        }
    }

    // ---- Numbers the screens share ---------------------------------------------

    /// What one block costs at the practice difficulty, at difficulty 1, and the ratio between.
    fn cost_rows(&self, language: Language) -> Vec<(&'static str, String)> {
        let real = Target::difficulty_one().expected_hashes();
        let practice = match Target::from_leading_zero_bits(self.zero_bits()) {
            Ok(target) => target.expected_hashes(),
            Err(_) => f64::NAN,
        };
        let hashes = Msg::UnitHashes.text(language);
        let harder = if practice.is_finite() && practice > 0.0 {
            format!("{}x", format::count((real / practice) as u64))
        } else {
            Msg::Unavailable.text(language).to_string()
        };
        vec![
            (Msg::LabelPracticeHere.text(language), whole(practice, hashes, language)),
            (Msg::LabelDifficultyOne.text(language), whole(real, hashes, language)),
            (Msg::LabelHarderBy.text(language), harder),
        ]
    }

    /// This machine measured against difficulty 1 and against the network of early 2009.
    fn comparison_rows(&self, rate: f64, language: Language) -> Vec<(&'static str, String)> {
        let real = Target::difficulty_one();
        let estimate = expected_time_to_block(rate, real);
        let network = implied_network_hashrate(real, BITCOIN_TARGET_BLOCK_SECONDS);
        // A multiple, not a percentage. A machine seven times the whole network of early 2009 is
        // not "715% of a share" — a share cannot be more than all of it, and the reader who reads
        // that line as a share concludes the screen is broken.
        let times = if network > 0.0 { rate / network } else { f64::NAN };
        vec![
            (Msg::LabelOneBlockTakes.text(language), span(estimate.expected_seconds, language)),
            (Msg::LabelHalfTakeUnder.text(language), span(estimate.median_seconds, language)),
            (Msg::LabelNetwork2009.text(language), format::hashrate(network)),
            (
                Msg::LabelYouAre.text(language),
                if times.is_finite() {
                    format!("{times:.1} {}", Msg::UnitTimesNetwork.text(language))
                } else {
                    Msg::Unavailable.text(language).to_string()
                },
            ),
        ]
    }

    /// One row per miner: threads, the share it asked for, and the share it really holds.
    ///
    /// A live run answers with what it is really doing; with nothing running, the split the
    /// current knobs would produce is worked out without starting anything.
    fn split_rows(&self) -> Result<Vec<SplitRow>, Msg> {
        if let Some(snapshot) = self.mining_snapshot.as_ref().filter(|_| self.mining.is_some()) {
            return Ok(split_of(snapshot));
        }
        let specs = self.specs();
        let threads = split_threads(&specs, self.thread_budget()).map_err(phrases::config_error)?;
        let effective = effective_shares(&threads);
        let asked: f64 = (0..self.miner_count()).map(|index| self.miner_share(index)).sum();
        Ok(threads
            .iter()
            .enumerate()
            .map(|(index, count)| SplitRow {
                index,
                threads: *count,
                requested: if asked > 0.0 { self.miner_share(index) / asked } else { 0.0 },
                effective: effective.get(index).copied().unwrap_or(0.0),
            })
            .collect())
    }

    // ---- Drawing ---------------------------------------------------------------

    fn status_line(&self, elapsed: Duration, language: Language, theme: Theme) -> Line<'static> {
        let (state, message) = match self.state {
            RunState::Idle => (None, Msg::StatusIdle),
            RunState::Running => (Some(State::Working), Msg::StatusMining),
            RunState::Paused => (Some(State::Chosen), Msg::StatusPaused),
            RunState::Done => (Some(State::Good), Msg::StatusDone),
        };
        let style = state.map(|state| theme.state(state)).unwrap_or_else(|| theme.muted());
        let mark = state.map(|state| state.mark()).unwrap_or(" ");
        Line::from(vec![
            Span::styled(format!("{mark} "), style),
            Span::styled(column(message.text(language), 26), style),
            Span::styled(format::duration(elapsed), theme.muted()),
        ])
    }

    fn brief_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        let mut diagram =
            wrapped("", Msg::PuzzleHeader.text(language), width, theme.plain(), theme.plain());
        for step in [Msg::PuzzleHash, Msg::PuzzleCompare] {
            diagram.extend(wrapped(
                "  ↓  ",
                step.text(language),
                width,
                theme.muted(),
                theme.plain(),
            ));
        }
        for (answer, style) in [(Msg::PuzzleNo, theme.muted()), (Msg::PuzzleYes, theme.good())] {
            let (mark, rest) = branch(answer.text(language));
            diagram.extend(wrapped(&format!("     {mark}"), rest, width, theme.plain(), style));
        }
        diagram.push(Line::from(""));
        diagram.extend(wrapped(
            "",
            Msg::PuzzleCost.text(language),
            width,
            theme.heading(),
            theme.heading(),
        ));

        let [top, rows] =
            Layout::vertical([Constraint::Length(diagram.len() as u16), Constraint::Min(0)])
                .areas(area);
        frame.render_widget(Paragraph::new(diagram), top);
        frame.render_widget(
            Paragraph::new(stat_lines(&self.cost_rows(language), width, theme)),
            rows,
        );
    }

    fn run_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        let Some(snapshot) = &self.mining_snapshot else {
            let mut lines =
                vec![Line::from(Span::styled(Msg::StatusIdle.text(language), theme.muted()))];
            if let Some(message) = self.refused {
                lines.extend(wrapped("", message.text(language), width, theme.bad(), theme.bad()));
                lines.push(Line::from(""));
            }
            let [top, rows] =
                Layout::vertical([Constraint::Length(lines.len() as u16), Constraint::Min(0)])
                    .areas(area);
            frame.render_widget(Paragraph::new(lines), top);
            frame.render_widget(
                Paragraph::new(stat_lines(&self.cost_rows(language), width, theme)),
                rows,
            );
            return;
        };

        let mut rows: Vec<(&str, String)> = vec![
            (Msg::LabelHashRate.text(language), format::hashrate(snapshot.total_hashrate)),
            (Msg::LabelHashes.text(language), format::count(snapshot.total_hashes)),
            (Msg::LabelBlocksFound.text(language), format::count(snapshot.blocks_found)),
            (Msg::LabelChainHeight.text(language), format::count(snapshot.height)),
            (
                Msg::LabelHashesPerBlock.text(language),
                format::count(snapshot.expected_hashes_per_block as u64),
            ),
        ];
        // Only when it has happened. A row that reads zero for the whole run is a question the
        // screen asks the reader and never answers.
        if snapshot.stale_blocks > 0 {
            rows.push((Msg::LabelStale.text(language), format::count(snapshot.stale_blocks)));
        }
        let comparison = self.comparison_rows(snapshot.total_hashrate, language);

        // Built as one run of lines rather than a fixed grid of rows: a value that has to go
        // under its label, or a sentence that has to become two, takes a row from the block list
        // at the bottom instead of running off the right-hand edge.
        let mut head = vec![self.status_line(snapshot.elapsed, language, theme), Line::from("")];
        head.extend(stat_lines(&rows, width, theme));
        head.push(Line::from(""));
        head.extend(wrapped(
            "",
            Msg::HeadingComparison.text(language),
            width,
            theme.heading(),
            theme.heading(),
        ));
        head.extend(stat_lines(&comparison, width, theme));
        head.extend(wrapped(
            "",
            Msg::ComparisonDerived.text(language),
            width,
            theme.muted(),
            theme.muted(),
        ));
        head.push(Line::from(""));
        head.extend(wrapped(
            "",
            Msg::HeadingRecentBlocks.text(language),
            width,
            theme.heading(),
            theme.heading(),
        ));

        let [top, blocks] =
            Layout::vertical([Constraint::Length(head.len() as u16), Constraint::Min(0)])
                .areas(area);
        frame.render_widget(Paragraph::new(head), top);
        self.block_rows(frame, blocks, theme, &snapshot.recent_blocks);
    }

    /// The newest blocks, as many as the space left will hold.
    fn block_rows(&self, frame: &mut Frame, area: Rect, theme: Theme, blocks: &[BlockSummary]) {
        if area.height == 0 {
            return;
        }
        let lines: Vec<Line<'static>> = blocks
            .iter()
            .rev()
            .take(area.height as usize)
            .map(|block| {
                let state = if block.in_best_chain { State::Good } else { State::Bad };
                Line::from(vec![
                    Span::styled(format!("{} ", state.mark()), theme.state(state)),
                    Span::styled(column(&format::count(block.height), 5), theme.plain()),
                    Span::styled(column(&format::duration(block.since_previous), 8), theme.muted()),
                    Span::styled(short_hash(block.hash), theme.muted()),
                ])
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }

    fn tune_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        let knob_lines = self.tune_knob_lines(width, language, theme);
        let knob_rows = knob_lines.len() as u16;
        let [knobs, gap, heading, table, footer] = Layout::vertical([
            Constraint::Length(knob_rows),
            Constraint::Length(1),
            Constraint::Length(1),
            Constraint::Min(2),
            Constraint::Length(2),
        ])
        .areas(area);
        let _ = gap;
        frame.render_widget(Paragraph::new(knob_lines), knobs);
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(
                Msg::HeadingSplit.text(language),
                theme.heading(),
            ))),
            heading,
        );

        match self.split_rows() {
            Ok(rows) => {
                // Three number columns of eight cells and whatever is left for the name. The
                // name column narrows rather than shoving the numbers off the edge, which is what
                // a Korean miner name did when the padding counted characters.
                let numbers = 8;
                let name = width.saturating_sub(numbers * 3).max(4);
                let mut lines = vec![Line::from(vec![
                    Span::styled(column(Msg::ColumnMiner.text(language), name), theme.muted()),
                    Span::styled(right(Msg::ColumnThreads.text(language), numbers), theme.muted()),
                    Span::styled(right(Msg::ColumnAsked.text(language), numbers), theme.muted()),
                    Span::styled(right(Msg::ColumnGot.text(language), numbers), theme.muted()),
                ])];
                let mut differs = false;
                for row in &rows {
                    differs |= (row.requested - row.effective).abs() > 0.005;
                    lines.push(Line::from(vec![
                        Span::styled(
                            column(phrases::miner_name(row.index).text(language), name),
                            theme.plain(),
                        ),
                        Span::styled(right(&row.threads.to_string(), numbers), theme.plain()),
                        Span::styled(
                            right(&format::percent(row.requested), numbers),
                            theme.muted(),
                        ),
                        Span::styled(
                            right(&format::percent(row.effective), numbers),
                            theme.heading(),
                        ),
                    ]));
                }
                if differs {
                    lines.push(Line::from(""));
                    lines.extend(wrapped(
                        "",
                        Msg::TuneMismatch.text(language),
                        width,
                        theme.muted(),
                        theme.muted(),
                    ));
                }
                frame.render_widget(Paragraph::new(lines), table);
            }
            Err(message) => frame.render_widget(
                Paragraph::new(wrapped(
                    "",
                    message.text(language),
                    width,
                    theme.bad(),
                    theme.bad(),
                )),
                table,
            ),
        }

        let mut lines = Vec::new();
        if let Some(message) = self.refused {
            lines.extend(wrapped("", message.text(language), width, theme.bad(), theme.bad()));
        }
        frame.render_widget(Paragraph::new(lines), footer);
    }

    fn tune_knob_lines(
        &self,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let labels: Vec<Msg> = (0..self.tune_knobs.len())
            .map(|index| match index {
                KNOB_DIFFICULTY => Msg::KnobDifficulty,
                KNOB_MINERS => Msg::KnobMiners,
                KNOB_THREADS => Msg::KnobThreads,
                other => phrases::share_label(other - KNOB_SHARES_FROM),
            })
            .collect();
        let column_width = label_column(&labels, width, language);
        self.tune_knobs
            .iter()
            .enumerate()
            .flat_map(|(index, knob)| {
                let unit = if index == KNOB_DIFFICULTY {
                    Some(Msg::UnitZeroBits.text(language))
                } else {
                    None
                };
                knob_line(
                    knob,
                    labels[index].text(language),
                    unit,
                    index == self.chosen,
                    column_width,
                    width,
                    theme,
                )
            })
            .collect()
    }

    fn break_knob_lines(
        &self,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let labels = [Msg::KnobAttackerShare, Msg::KnobConfirmations];
        let column_width = label_column(&labels, width, language);
        self.break_knobs
            .iter()
            .enumerate()
            .flat_map(|(index, knob)| {
                let label = labels.get(index).copied().unwrap_or(Msg::KnobConfirmations);
                knob_line(
                    knob,
                    label.text(language),
                    None,
                    index == self.chosen,
                    column_width,
                    width,
                    theme,
                )
            })
            .collect()
    }

    fn break_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        let knob_lines = self.break_knob_lines(width, language, theme);
        let [knobs, gap, body] = Layout::vertical([
            Constraint::Length(knob_lines.len() as u16),
            Constraint::Length(1),
            Constraint::Min(0),
        ])
        .areas(area);
        let _ = gap;
        frame.render_widget(Paragraph::new(knob_lines), knobs);

        let Some(snapshot) = &self.attack_snapshot else {
            let (attacker, honest) = self.attack_threads();
            let mut lines =
                vec![Line::from(Span::styled(Msg::StatusIdle.text(language), theme.muted()))];
            if let Some(message) = self.refused {
                lines.extend(wrapped("", message.text(language), width, theme.bad(), theme.bad()));
                lines.push(Line::from(""));
            }
            let rows = vec![
                (
                    Msg::LabelDifficulty.text(language),
                    format!("{} {}", self.zero_bits(), Msg::UnitZeroBits.text(language)),
                ),
                (Msg::LabelThreadSplit.text(language), format!("{attacker} / {honest}")),
            ];
            lines.extend(stat_lines(&rows, width, theme));
            lines.extend(self.split_caption(width, language, theme));
            frame.render_widget(Paragraph::new(lines), body);
            return;
        };

        // While it runs, the phase says what is happening; once it is over, the ending says it.
        let phase = match snapshot.outcome {
            Some(_) => Line::from(""),
            None => Line::from(Span::styled(
                phrases::phase(snapshot.phase).text(language),
                theme.muted(),
            )),
        };
        let mut lines =
            vec![self.status_line(snapshot.elapsed, language, theme), phase, Line::from("")];
        lines.extend(stat_lines(&self.attack_rows(snapshot, language), width, theme));
        lines.extend(self.split_caption(width, language, theme));
        lines.extend(self.ending_lines(snapshot, width, language, theme));
        frame.render_widget(Paragraph::new(lines), body);
    }

    /// What the two numbers in the thread split mean.
    ///
    /// This is a caption, not a value, and it was drawn as a nameless row of the table: indented
    /// by whatever the widest label happened to be, so it lost a different number of letters
    /// depending on whether the attacker was ahead or behind. It is a sentence, so it wraps.
    fn split_caption(&self, width: usize, language: Language, theme: Theme) -> Vec<Line<'static>> {
        wrapped("", Msg::ThreadsAttackerHonest.text(language), width, theme.muted(), theme.muted())
    }

    /// How it ended: green when the chain held, red when an attacker rewrote it.
    fn ending_lines(
        &self,
        snapshot: &AttackSnapshot,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let Some(outcome) = snapshot.outcome else { return Vec::new() };
        let state = match outcome {
            AttackOutcome::Succeeded => State::Bad,
            AttackOutcome::GaveUp => State::Good,
        };
        let mut lines = vec![Line::from("")];
        lines.extend(wrapped(
            &format!("{} ", state.mark()),
            phrases::outcome(outcome).text(language),
            width,
            theme.state(state),
            theme.state(state),
        ));
        lines
    }

    /// What the attack has done so far, or what it did.
    fn attack_rows(
        &self,
        snapshot: &AttackSnapshot,
        language: Language,
    ) -> Vec<(&'static str, String)> {
        let mut rows = vec![
            (Msg::LabelPublicChain.text(language), format::count(snapshot.public_height)),
            (Msg::LabelPrivateChain.text(language), format::count(snapshot.private_height)),
        ];
        let gap = snapshot.lead.unsigned_abs();
        rows.push((
            if snapshot.lead >= 0 {
                Msg::LabelAheadBy.text(language)
            } else {
                Msg::LabelBehindBy.text(language)
            },
            blocks(gap, language),
        ));
        rows.push((
            Msg::LabelFurthestBehind.text(language),
            blocks(snapshot.max_deficit, language),
        ));
        // What the attack is waiting for. A screen that shows a race with no finishing line asks
        // the reader to sit through two minutes without telling them it is two minutes.
        if snapshot.outcome.is_none() {
            let config = AttackConfig::new(snapshot.bits, 1, 1);
            let limit = config.give_up_after.unwrap_or_default().as_secs_f64();
            rows.push((
                Msg::LabelGivesUp.text(language),
                format!(
                    "{}  {} {}",
                    span(limit, language),
                    Msg::UnitOrBlocks.text(language),
                    config.give_up_after_public_blocks
                ),
            ));
        }
        let seen = snapshot
            .confirmations_when_reverted
            .or(snapshot.confirmations_at_release)
            .unwrap_or(snapshot.victim_confirmations);
        rows.push((
            Msg::LabelMerchantSaw.text(language),
            plural(seen, Msg::UnitConfirmation, Msg::UnitConfirmations, language),
        ));
        rows.push((
            Msg::LabelGoods.text(language),
            if snapshot.victim_released {
                Msg::VictimReleased.text(language).to_string()
            } else {
                Msg::VictimWaiting.text(language).to_string()
            },
        ));
        rows.push((Msg::LabelBlocksErased.text(language), format::count(snapshot.blocks_reverted)));
        rows.push((
            Msg::LabelAttackTook.text(language),
            match snapshot.attack_duration {
                Some(took) => format::duration(took),
                None => Msg::Unavailable.text(language).to_string(),
            },
        ));
        rows.push((
            Msg::LabelThreadSplit.text(language),
            format!(
                "{} / {}  ({})",
                snapshot.attacker.threads,
                snapshot.honest.threads,
                format::percent(snapshot.attacker_share)
            ),
        ));
        rows
    }

    fn recap_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let unknown = || Msg::NotYet.text(language).to_string();
        let mining = self.mining_snapshot.as_ref();
        let rate = mining.map(|snapshot| snapshot.total_hashrate).unwrap_or(0.0);
        let real = Target::difficulty_one();
        let network = implied_network_hashrate(real, BITCOIN_TARGET_BLOCK_SECONDS);

        // The recap says what happened, so the split it shows is the one the run really used.
        let split = mining.map(split_of).and_then(|rows| rows.first().copied());
        let attack = self.attack_snapshot.as_ref();

        let rows: Vec<(&'static str, String)> = vec![
            (
                Msg::RecapDifficulty.text(language),
                format!("{} {}", self.zero_bits(), Msg::UnitZeroBits.text(language)),
            ),
            (
                Msg::RecapHashRate.text(language),
                match mining {
                    Some(_) => format::hashrate(rate),
                    None => unknown(),
                },
            ),
            (
                Msg::RecapBlocks.text(language),
                match mining {
                    Some(snapshot) => format::count(snapshot.blocks_in_chain),
                    None => unknown(),
                },
            ),
            (
                Msg::RecapAtDifficultyOne.text(language),
                match mining {
                    Some(_) => span(expected_time_to_block(rate, real).expected_seconds, language),
                    None => unknown(),
                },
            ),
            (Msg::RecapNetwork.text(language), format::hashrate(network)),
            (
                Msg::RecapSplit.text(language),
                match split {
                    Some(row) => format!(
                        "{} / {}",
                        format::percent(row.requested),
                        format::percent(row.effective)
                    ),
                    None => unknown(),
                },
            ),
            (
                Msg::RecapAttack.text(language),
                match attack.and_then(|snapshot| snapshot.outcome) {
                    Some(outcome) => phrases::short_outcome(outcome).text(language).to_string(),
                    None => unknown(),
                },
            ),
        ];

        // Two cells for the number, then a label column wide enough for the longest label in
        // whichever language is on screen. A value the rest will not hold goes under its label.
        let width = area.width as usize;
        let label = rows
            .iter()
            .map(|(name, _)| cells(name))
            .max()
            .unwrap_or(0)
            .min(width.saturating_sub(4))
            + 1;
        let room = width.saturating_sub(label + 2);
        let lines: Vec<Line<'static>> = rows
            .iter()
            .enumerate()
            .flat_map(|(index, (name, value))| {
                let number = Span::styled(format!("{} ", index + 1), theme.muted());
                if cells(value) <= room {
                    return vec![Line::from(vec![
                        number,
                        Span::styled(column(name, label), theme.plain()),
                        Span::styled(value.clone(), theme.heading()),
                    ])];
                }
                let mut lines = vec![Line::from(vec![
                    number,
                    Span::styled(truncate(name, width.saturating_sub(2)), theme.plain()),
                ])];
                for chunk in wrap(value, width.saturating_sub(4)) {
                    lines.push(Line::from(Span::styled(format!("    {chunk}"), theme.heading())));
                }
                lines
            })
            .collect();
        frame.render_widget(Paragraph::new(lines), area);
    }
}

/// One miner's line in the Tune table.
#[derive(Debug, Clone, Copy)]
struct SplitRow {
    index: usize,
    threads: usize,
    requested: f64,
    effective: f64,
}

impl Session {
    /// The script of the stage showing now.
    fn script(&self) -> &'static [Step] {
        SCRIPTS[self.stage.min(SCRIPTS.len() - 1)]
    }

    /// Back to the first sentence of this stage, with nothing running and nothing said.
    ///
    /// What finished runs produced is kept, so a recap reached from here — by Tab, by a digit key,
    /// by any route at all — still has the reader's own numbers in it.
    fn restart(&mut self) {
        self.stop_runs();
        self.forget_run();
        self.revealed = 0;
        self.log.clear();
    }

    /// A sentence about the attack that finished, read off that attack.
    ///
    /// Four endings, because a race is not arithmetic alone: below half it usually loses and
    /// sometimes wins, and above half it usually wins and sometimes runs out of time first.
    fn tell(&self, topic: Topic, language: Language) -> String {
        match topic {
            Topic::Attack => self.tell_attack(language),
            Topic::Difficulty => {
                // The reader sets the difficulty, so the sentence reads the difficulty rather
                // than the one the conversation suggested. Pressing Enter without moving it used
                // to be answered with "blocks cost twice what they did".
                let now = self.zero_bits();
                let message = match now.cmp(&self.bits_on_entry) {
                    std::cmp::Ordering::Equal => Msg::TuneBitsUnchanged,
                    std::cmp::Ordering::Greater => Msg::TuneBitsUp,
                    std::cmp::Ordering::Less => Msg::TuneBitsDown,
                };
                message.text(language).to_string()
            }
        }
    }

    fn tell_attack(&self, language: Language) -> String {
        let Some(snapshot) = &self.attack_snapshot else {
            return Msg::AttackNotYetRun.text(language).to_string();
        };
        let majority = snapshot.attacker_share >= 0.5;
        let message = match (majority, snapshot.outcome) {
            (false, Some(AttackOutcome::Succeeded)) => Msg::AttackLuckyWin,
            (false, _) => Msg::AttackLost,
            (true, Some(AttackOutcome::Succeeded)) => Msg::AttackWon,
            (true, _) => Msg::AttackRanOut,
        };
        message.text(language).to_string()
    }

    /// Whether the thing a waiting step is waiting for has happened.
    fn satisfied(&self, until: Until) -> bool {
        match until {
            Until::Blocks(wanted) => {
                self.mining_snapshot.as_ref().is_some_and(|s| s.blocks_in_chain >= wanted)
            }
            Until::AttackOver => self.attack_snapshot.as_ref().is_some_and(|s| s.outcome.is_some()),
        }
    }

    /// Reveals the next step and starts whatever it asks for. Used by Enter and by the clock.
    fn advance(&mut self) -> bool {
        let script = self.script();
        if self.revealed + 1 >= script.len() {
            return false;
        }
        self.revealed += 1;
        if let Run(deed) = script[self.revealed] {
            match deed {
                Mine => {
                    self.reported_blocks = 0;
                    self.start_run();
                    if let Some(snapshot) = &self.mining_snapshot {
                        // The miner count goes in the opening line because the block lines name
                        // whoever found each block, and a reader met "found by Miner 2" without
                        // having been told there was more than one.
                        let miners = snapshot.miners.len();
                        let threads = snapshot.miners.iter().map(|m| m.threads).sum();
                        let bits = u64::from(self.zero_bits());
                        self.say(Happening::MiningStarted { miners, threads, bits });
                    }
                }
                Attack => {
                    self.said_paid = false;
                    self.said_released = false;
                    self.said_ended = false;
                    self.start_the_attack();
                    if self.attack.is_some() {
                        let share = self.attacker_share();
                        let confirmations = self.confirmations();
                        self.say(Happening::AttackStarted { share, confirmations });
                    }
                }
            }
            if let Some(refused) = self.refused.take() {
                self.say(Happening::Refused(refused));
            }
            // A run and the wait for it are one move. Making the reader press Enter again to
            // start waiting would offer them a key that does nothing but skip the answer.
            if matches!(script.get(self.revealed + 1), Some(Await(_))) {
                self.revealed += 1;
            }
        }
        true
    }

    /// Files one happening against the step the reader is on.
    fn say(&mut self, what: Happening) {
        if self.log.len() < EVENT_CAP {
            self.log.push(Logged { step: self.revealed, what });
        }
    }

    /// Reads the running engines and turns anything new into beats.
    fn notice(&mut self) {
        // Two miners can solve the same height at once and both are recorded, so the filter is
        // on what has already been said rather than on the height alone: otherwise the pair read
        // as one block reported twice, the second with a gap of nothing.
        let blocks: Vec<(u64, Duration, usize, bool)> = match &self.mining_snapshot {
            Some(snapshot) if self.mining.is_some() => snapshot
                .recent_blocks
                .iter()
                .filter(|block| !self.said_blocks.contains(&block.hash))
                .map(|block| {
                    (
                        block.height,
                        block.since_previous,
                        block.miner.0 as usize,
                        block.in_best_chain,
                    )
                })
                .collect(),
            _ => Vec::new(),
        };
        if !blocks.is_empty()
            && let Some(snapshot) = &self.mining_snapshot
        {
            self.said_blocks.extend(snapshot.recent_blocks.iter().map(|block| block.hash));
        }
        for (height, gap, miner, on_the_chain) in blocks {
            self.reported_blocks = self.reported_blocks.max(height);
            self.say(Happening::Block { height, gap, miner, on_the_chain });
        }

        if self.attack.is_none() {
            return;
        }
        // Every few blocks of deficit, rather than once at the end: an attack that loses spends
        // two minutes doing so, and a screen that says nothing for two minutes reads as a hang.
        const BLOCKS_BETWEEN_REMARKS: u64 = 3;
        let losing = self.attack_snapshot.as_ref().filter(|snapshot| {
            snapshot.outcome.is_none()
                && snapshot.max_deficit >= self.said_deficit + BLOCKS_BETWEEN_REMARKS
        });
        if let Some((by, elapsed)) = losing.map(|s| (s.max_deficit, s.elapsed)) {
            self.said_deficit = by;
            self.say(Happening::FallingBehind { by, elapsed });
        }

        let Some(snapshot) = &self.attack_snapshot else { return };
        let paid = snapshot.payment_height.is_some();
        let released = snapshot.victim_released;
        let at_release = snapshot.confirmations_at_release.unwrap_or(0);
        let ending = snapshot
            .outcome
            .map(|outcome| (outcome, snapshot.blocks_reverted, snapshot.attack_duration));
        if paid && !self.said_paid {
            self.said_paid = true;
            self.say(Happening::Paid);
        }
        if released && !self.said_released {
            self.said_released = true;
            self.say(Happening::Released { confirmations: at_release });
        }
        if let Some((outcome, reverted, took)) = ending
            && !self.said_ended
        {
            self.said_ended = true;
            self.say(Happening::Ended { outcome, reverted, took });
        }
    }

    /// One happening, said in the reader's language.
    fn beat_for(&self, what: &Happening, language: Language) -> Beat {
        match what {
            Happening::MiningStarted { miners, threads, bits } => Beat::event(format!(
                "{}  ·  {} {}  ·  {} {}  ·  {} {} {}",
                Msg::EventMiningStarted.text(language),
                Msg::KnobMiners.text(language),
                miners,
                Msg::KnobThreads.text(language),
                threads,
                Msg::KnobDifficulty.text(language),
                bits,
                Msg::UnitZeroBits.text(language),
            )),
            Happening::Block { height, gap, miner, on_the_chain: true } => Beat::outcome(
                State::Good,
                format!(
                    "{} {}  ·  {} {}  ·  {} {}",
                    Msg::EventBlock.text(language),
                    height,
                    Msg::EventGap.text(language),
                    span(gap.as_secs_f64(), language),
                    Msg::EventFoundBy.text(language),
                    phrases::miner_name(*miner).text(language),
                ),
            ),
            Happening::FallingBehind { by, elapsed } => Beat::outcome(
                State::Working,
                format!(
                    "{}  ·  {}  ·  {}",
                    Msg::EventFallingBehind.text(language),
                    blocks(*by, language),
                    span(elapsed.as_secs_f64(), language),
                ),
            ),
            // The loser of a race is not another block: it is the same height, done twice.
            Happening::Block { height, miner, .. } => Beat::outcome(
                State::Bad,
                format!(
                    "{} {}  ·  {} {}",
                    Msg::EventBlock.text(language),
                    height,
                    phrases::miner_name(*miner).text(language),
                    Msg::EventLostRace.text(language),
                ),
            ),
            Happening::AttackStarted { share, confirmations } => Beat::event(format!(
                "{}  ·  {} {}  ·  {} {}",
                Msg::EventAttackStarted.text(language),
                Msg::EventShare.text(language),
                format::percent(*share),
                Msg::KnobConfirmations.text(language),
                confirmations,
            )),
            Happening::Paid => Beat::event(Msg::EventPaid.text(language)),
            Happening::Released { confirmations } => Beat::event(format!(
                "{}  ·  {} {}",
                Msg::EventReleased.text(language),
                Msg::KnobConfirmations.text(language),
                confirmations,
            )),
            Happening::Ended { outcome, reverted, took } => {
                let won = *outcome == AttackOutcome::Succeeded;
                let mut text = phrases::outcome(*outcome).text(language).to_string();
                if *reverted > 0 {
                    text.push_str(&format!(
                        "  ·  {} {}",
                        Msg::EventErased.text(language),
                        reverted
                    ));
                }
                if let Some(took) = took {
                    text.push_str(&format!(
                        "  ·  {} {}",
                        Msg::EventTook.text(language),
                        span(took.as_secs_f64(), language)
                    ));
                }
                // The attacker winning is bad news for everyone else, which is the point.
                Beat::outcome(if won { State::Bad } else { State::Good }, text)
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
        self.bits_on_entry = self.zero_bits();
        // One heavy run at a time: mining beside an attack halves both, and a 51% attack that
        // cannot win because the screen is stealing its cores is a lie about proof of work. The
        // threads stop; what they already produced stays, because the recap is about it.
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
            // A waiting step goes on by itself, as soon as the machine has answered.
            Some(Await(until)) => self.satisfied(*until),
            _ => self.revealed + 1 < self.script().len(),
        }
    }

    fn knobs(&self) -> &[Knob] {
        match self.stage {
            STAGE_TUNE => &self.tune_knobs,
            STAGE_ATTACK => &self.break_knobs,
            _ => &[],
        }
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
            Action::PauseOrResume => {
                let paused = match (&self.attack, &self.mining) {
                    (Some(handle), _) => {
                        let paused = handle.is_paused();
                        if paused {
                            handle.resume()
                        } else {
                            handle.pause()
                        }
                        Some(!paused)
                    }
                    (None, Some(handle)) => {
                        let paused = handle.is_paused();
                        if paused {
                            handle.resume()
                        } else {
                            handle.pause()
                        }
                        Some(!paused)
                    }
                    (None, None) => None,
                };
                match paused {
                    Some(true) => {
                        self.state = RunState::Paused;
                        Reaction::Handled
                    }
                    Some(false) => {
                        self.state = RunState::Running;
                        Reaction::Handled
                    }
                    None => Reaction::Ignored,
                }
            }
            Action::Next => {
                self.move_choice(1);
                Reaction::Handled
            }
            Action::Previous => {
                self.move_choice(-1);
                Reaction::Handled
            }
            Action::Nudge(direction) => self.change_chosen(|knob| knob.nudge(direction)),
            Action::Type(c) => self.change_chosen(|knob| knob.type_char(c)),
            Action::Backspace => self.change_chosen(Knob::backspace),
            Action::Commit => self.change_chosen(|knob| {
                knob.commit();
            }),
            Action::Cancel => self.change_chosen(Knob::cancel),
        }
    }

    fn tick(&mut self) {
        if let Some(handle) = &self.mining {
            let snapshot = handle.snapshot();
            self.state = if snapshot.paused {
                RunState::Paused
            } else if snapshot.finished {
                RunState::Done
            } else {
                RunState::Running
            };
            if self.rates.len() >= RATE_SAMPLES {
                self.rates.remove(0);
            }
            self.rates.push(snapshot.total_hashrate);
            self.mining_snapshot = Some(snapshot);
        }
        if let Some(handle) = &self.attack {
            let snapshot = handle.snapshot();
            self.state = if snapshot.paused {
                RunState::Paused
            } else if snapshot.finished || snapshot.phase == AttackPhase::Finished {
                RunState::Done
            } else {
                RunState::Running
            };
            self.attack_snapshot = Some(snapshot);
        }
        self.notice();
        // A waiting step carries itself on the moment the machine has answered, so the reader
        // never has to guess whether pressing Enter would have helped.
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
            STAGE_WHY | STAGE_PUZZLE => Msg::PuzzleTitle,
            STAGE_MINE => Msg::RunTitle,
            STAGE_TUNE => Msg::TuneTitle,
            STAGE_ATTACK => Msg::BreakTitle,
            _ => Msg::RecapTitle,
        };
        let block = theme.titled_panel(title.text(language));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        match self.stage {
            STAGE_WHY | STAGE_PUZZLE => self.brief_panel(frame, inner, theme, language),
            STAGE_MINE => {
                // A taller terminal gets the hash rate over time; 80x24 does not have the room.
                if inner.height >= CURVE_NEEDS_ROWS && self.rates.len() > 1 {
                    let [top, curve] =
                        Layout::vertical([Constraint::Min(0), Constraint::Length(3)]).areas(inner);
                    self.run_panel(frame, top, theme, language);
                    widgets::curve(
                        frame,
                        curve,
                        theme,
                        &self.rates,
                        Msg::LabelHashRate.text(language),
                    );
                } else {
                    self.run_panel(frame, inner, theme, language);
                }
            }
            STAGE_TUNE => self.tune_panel(frame, inner, theme, language),
            STAGE_ATTACK => self.break_panel(frame, inner, theme, language),
            _ => self.recap_panel(frame, inner, theme, language),
        }
    }

    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)> {
        match self.stage {
            STAGE_TUNE | STAGE_ATTACK => vec![
                ("↑↓ ←→", Msg::KeyTurn.text(language)),
                ("0-9", Msg::KeyTypeNumber.text(language)),
            ],
            _ => Vec::new(),
        }
    }

    fn typing(&self) -> bool {
        self.knob_count() > 0
            && self.knobs().get(self.chosen).is_some_and(|knob| knob.draft().is_some())
    }

    fn close(&mut self) {
        self.stop_mining();
        self.stop_attack();
    }
}

/// How many cells the labels of a column of knobs need, never more than half the panel.
fn label_column(labels: &[Msg], width: usize, language: Language) -> usize {
    labels
        .iter()
        .map(|label| cells(label.text(language)))
        .max()
        .unwrap_or(0)
        .min(width.saturating_sub(4))
        + 1
}

/// A knob's row: the mark, the label, and the value or what is being typed into it.
///
/// A value too long for the room left goes on the next line under the label rather than being cut
/// at the panel edge — these stages have blank rows to spare, and half a number says less than a
/// number on its own line.
fn knob_line(
    knob: &Knob,
    label: &str,
    unit: Option<&str>,
    chosen: bool,
    label_width: usize,
    width: usize,
    theme: Theme,
) -> Vec<Line<'static>> {
    let marker = if chosen { State::Chosen.mark() } else { " " };
    let value = match unit {
        Some(unit) if knob.draft().is_none() => format!("{} {unit}", knob.display()),
        _ => knob.display(),
    };
    let style = if chosen { theme.heading() } else { theme.muted() };
    let room = width.saturating_sub(label_width + 2);
    if cells(&value) <= room {
        return vec![Line::from(vec![
            Span::styled(format!("{marker} "), theme.state(State::Chosen)),
            Span::styled(column(label, label_width), theme.plain()),
            Span::styled(value, style),
        ])];
    }
    let mut lines = vec![Line::from(vec![
        Span::styled(format!("{marker} "), theme.state(State::Chosen)),
        Span::styled(truncate(label, width.saturating_sub(2)), theme.plain()),
    ])];
    for chunk in wrap(&value, width.saturating_sub(4)) {
        lines.push(Line::from(Span::styled(format!("    {chunk}"), style)));
    }
    lines
}

fn count_of(knob: Option<&Knob>) -> u64 {
    match knob.map(|knob| &knob.value) {
        Some(KnobValue::Count { current, .. }) => *current,
        _ => 0,
    }
}

fn share_of(knob: Option<&Knob>) -> f64 {
    match knob.map(|knob| &knob.value) {
        Some(KnobValue::Share { current, .. }) => *current,
        _ => 0.0,
    }
}

/// What a run really did with its threads, one row per miner.
fn split_of(snapshot: &MiningSnapshot) -> Vec<SplitRow> {
    let asked: f64 = snapshot.miners.iter().map(|miner| requested_of(miner.requested)).sum();
    snapshot
        .miners
        .iter()
        .enumerate()
        .map(|(index, miner)| SplitRow {
            index,
            threads: miner.threads,
            requested: if asked > 0.0 { requested_of(miner.requested) / asked } else { 0.0 },
            effective: miner.effective_share,
        })
        .collect()
}

/// A share as the engine reported it being asked for.
fn requested_of(share: Share) -> f64 {
    match share {
        Share::Percent(percent) => percent.max(0.0),
        Share::Threads(threads) => threads as f64,
    }
}

/// `text` pushed to the right of `columns` cells, so a column of numbers lines up.
///
/// Cells, not characters: a Korean heading is half as many characters and exactly as many
/// columns, and counting characters pushed every number in the Tune table out of its column.
fn right(text: &str, columns: usize) -> String {
    rpad(&truncate(text, columns), columns)
}

/// A branch of the puzzle diagram, split into the word that names it and the sentence after it.
///
/// The phrase carries a run of spaces between the two so that "no" and "yes" start their
/// sentences in the same column; [`wrap`] collapses runs of spaces, so the run is taken out of
/// the text and kept as part of the lead rather than left in the text to be wrapped.
fn branch(text: &str) -> (&str, &str) {
    let Some(gap) = text.find("  ") else { return (text, "") };
    let after = text[gap..].find(|c: char| c != ' ').map(|at| gap + at).unwrap_or(text.len());
    text.split_at(after)
}

/// A count of blocks, written so that one of them is not "1 blocks".
fn blocks(count: u64, language: Language) -> String {
    plural(count, Msg::UnitBlock, Msg::UnitBlocks, language)
}

/// A count with the right one of two words after it.
fn plural(count: u64, one: Msg, many: Msg, language: Language) -> String {
    format!("{count} {}", if count == 1 { one } else { many }.text(language))
}

/// A count of hashes, with its unit.
fn whole(value: f64, unit: &str, language: Language) -> String {
    if value.is_finite() && value >= 0.0 {
        format!("{} {unit}", format::count(value as u64))
    } else {
        Msg::Unavailable.text(language).to_string()
    }
}

/// A stretch of seconds as a span a reader can read. Nothing hashing means no answer, not a panic.
fn span(seconds: f64, language: Language) -> String {
    if !seconds.is_finite() || seconds < 0.0 {
        return Msg::Unavailable.text(language).to_string();
    }
    format::duration(Duration::from_secs_f64(seconds.min(1e15)))
}

/// The first few characters of a hash, which is how a block is named in a list.
fn short_hash(hash: Hash256) -> String {
    let full = format!("{hash:?}");
    let mut out: String = full.chars().take(10).collect();
    out.push('…');
    out
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use nmtk_kq::session::Voice;

    use nmtk_kq::theme::{MIN_HEIGHT, MIN_WIDTH, split};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn session() -> Session {
        Session::new(&MachineProfile {
            logical_cores: 4,
            total_memory_bytes: 0,
            available_memory_bytes: 0,
        })
    }

    /// An attack snapshot with nothing in it, for tests about the two fields that pick a sentence.
    fn blank_attack() -> AttackSnapshot {
        let mut session = session();
        session.start_the_attack();
        let snapshot =
            session.attack.as_ref().map(|handle| handle.snapshot()).expect("the attack started");
        session.stop_attack();
        snapshot
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

    /// The panel's own columns at `total`, each row right-trimmed, borders and padding removed.
    fn panel_rows(session: &Session, total: u16, language: Language) -> Vec<String> {
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
                    skip = nmtk_kq::text::width(symbol) == 2;
                    text.push_str(symbol);
                }
                text.trim_end().to_string()
            })
            .collect()
    }

    /// Cells the panel's own columns come to, at a terminal `total` wide.
    fn panel_width(total: u16) -> usize {
        let (_, run) = split(total);
        run as usize - 4
    }

    /// A terminal wide enough that nothing this panel draws has to be wrapped or cut, which is
    /// what makes it the answer key: every word the panel means to say appears whole here.
    const ROOMY: u16 = 140;

    /// The panel is handed its width, and every line it draws has to fit inside it.
    ///
    /// A reader lost the end of the sentence saying a number was worked out rather than measured,
    /// and the caption saying which of the two thread counts is the attacker's — both cut at the
    /// edge with nothing to say a word had gone. A row that fills the last column is only allowed
    /// to end on a whole word: the words a roomy terminal shows are the ones that have to survive.
    #[test]
    fn no_line_of_any_stage_is_cut_at_the_panel_edge_in_either_language() {
        for total in [MIN_WIDTH, 100] {
            for language in Language::ALL {
                for stage in 0..SCRIPTS.len() {
                    let mut session = session();
                    session.go_to(stage);
                    if stage == STAGE_MINE || stage == STAGE_TUNE {
                        session.start_run();
                        session.tick();
                    }
                    if stage == STAGE_ATTACK {
                        let mut ended = blank_attack();
                        ended.outcome = Some(AttackOutcome::GaveUp);
                        session.attack_snapshot = Some(ended);
                    }
                    let rows = panel_rows(&session, total, *language);
                    let whole: Vec<String> = panel_rows(&session, ROOMY, *language)
                        .iter()
                        .flat_map(|row| {
                            row.split_whitespace().map(str::to_string).collect::<Vec<_>>()
                        })
                        .collect();
                    session.close();
                    for row in &rows {
                        assert!(
                            nmtk_kq::text::width(row) <= panel_width(total),
                            "stage {stage} at {total}: {row:?} is wider than the panel"
                        );
                        if nmtk_kq::text::width(row) < panel_width(total) {
                            continue;
                        }
                        let Some(tail) = row.split_whitespace().last() else { continue };
                        assert!(
                            tail.ends_with('…') || whole.iter().any(|word| word == tail),
                            "stage {stage} at {total}: {row:?} ends in the middle of {tail:?}"
                        );
                    }
                }
            }
        }
    }

    /// The panel as one run of words, so a sentence that wrapped onto a second line still reads
    /// as the sentence it is.
    fn said(rows: &[String]) -> String {
        rows.iter().flat_map(|row| row.split_whitespace()).collect::<Vec<_>>().join(" ")
    }

    /// The words the panel edge was eating, checked whole on the narrowest screen nmtk allows.
    #[test]
    fn the_words_that_carry_the_meaning_survive_the_narrowest_panel() {
        let mut running = session();
        running.go_to(STAGE_MINE);
        running.start_run();
        running.tick();
        let mining = said(&panel_rows(&running, MIN_WIDTH, Language::ENGLISH));
        running.close();
        assert!(
            mining.contains("worked out from the protocol, not measured"),
            "the sentence saying the number was not measured is cut:\n{mining}"
        );
        assert!(
            mining.contains("x that network"),
            "the reader cannot see what this machine is a multiple of:\n{mining}"
        );

        let mut broken = session();
        broken.go_to(STAGE_ATTACK);
        broken.attack_snapshot = Some(blank_attack());
        let attack = said(&panel_rows(&broken, MIN_WIDTH, Language::ENGLISH));
        broken.close();
        assert!(
            attack.contains("attacker / everyone else"),
            "the caption does not say who the second thread count belongs to:\n{attack}"
        );

        let puzzle = said(&panel_rows(&session(), MIN_WIDTH, Language::ENGLISH));
        assert!(
            puzzle.contains("with a nonce in it"),
            "the header line of the diagram is cut:\n{puzzle}"
        );
        assert!(
            puzzle.contains("change the nonce and hash again"),
            "the loop the miner goes round is cut:\n{puzzle}"
        );
        assert!(
            puzzle.contains("4,295,032,833 hashes"),
            "what a block at difficulty 1 costs is cut short:\n{puzzle}"
        );
    }

    #[test]
    fn the_brief_draws_the_puzzle_and_what_a_block_costs_at_eighty_by_twenty_four() {
        let text = draw(&session());
        assert!(text.contains("The puzzle"), "no panel title:\n{text}");
        assert!(text.contains("SHA-256"), "the hash is not named:\n{text}");
        // A block at difficulty 1 costs 2^256/(target+1) hashes, and that number — the one the
        // engine works out from the target itself — is the whole point of the screen.
        assert!(text.contains("4,295,032,833"), "difficulty 1 is not costed:\n{text}");
    }

    /// Presses Enter until the stage runs out of steps it can take without waiting.
    fn walk(session: &mut Session) {
        for _ in 0..session.script().len() {
            session.on(Action::Go);
        }
    }

    #[test]
    fn a_stage_opens_with_one_sentence_and_not_a_wall() {
        let session = session();
        let beats = session.transcript(Language::ENGLISH);
        assert_eq!(beats.len(), 1, "the reader was handed more than one thing at once");
    }

    #[test]
    fn every_beat_of_every_stage_is_short_in_both_languages() {
        for (stage, script) in SCRIPTS.iter().enumerate() {
            for language in Language::ALL {
                let mut session = session();
                session.go_to(stage);
                // Every scripted line, without starting any threads.
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

    #[test]
    fn the_mining_stage_waits_for_a_block_rather_than_talking_over_it() {
        let mut session = session();
        session.go_to(STAGE_MINE);
        walk(&mut session);
        let waiting = matches!(session.script().get(session.revealed), Some(Await(_)));
        assert!(waiting, "the conversation ran past the block it was about to describe");
        assert!(!session.can_advance(), "Enter would have skipped the run");
        assert_eq!(session.run_state(), RunState::Running);
        session.close();
    }

    #[test]
    fn a_block_that_arrives_is_reported_where_the_reader_is_looking() {
        let mut session = session();
        session.go_to(STAGE_MINE);
        walk(&mut session);
        // The practice difficulty is sized to this machine, so a block is seconds away.
        let start = std::time::Instant::now();
        while start.elapsed() < Duration::from_secs(60) {
            session.tick();
            if session.log.iter().any(|l| matches!(l.what, Happening::Block { .. })) {
                break;
            }
            std::thread::sleep(Duration::from_millis(50));
        }
        let beats = session.transcript(Language::ENGLISH);
        session.close();
        assert!(
            beats.iter().any(|beat| beat.text.starts_with("block ")),
            "no block was ever reported: {beats:?}"
        );
        assert!(session.revealed > 3, "the conversation never carried on by itself");
    }

    #[test]
    fn the_puzzle_panel_costs_a_block_at_eighty_by_twenty_four() {
        let mut session = session();
        session.go_to(STAGE_PUZZLE);
        let text = draw(&session);
        assert!(text.contains("The puzzle"), "no panel title:\n{text}");
        assert!(text.contains("SHA-256"), "the hash is not named:\n{text}");
        // A block at difficulty 1 costs 2^256/(target+1) hashes, and that number — the one the
        // engine works out from the target itself — is the whole point of the screen.
        assert!(text.contains("4,295,032,833"), "difficulty 1 is not costed:\n{text}");
    }

    #[test]
    fn tune_shows_the_share_that_was_asked_for_beside_the_share_that_was_got() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        let text = draw(&session);
        assert!(text.contains("asked") && text.contains("got"), "no split table:\n{text}");
        // Three threads cannot be split 60/40, so the two numbers have to differ on screen.
        assert!(text.contains("60.0%"), "what was asked for is missing:\n{text}");
        assert!(text.contains("66.7%"), "what was got is missing:\n{text}");
        assert!(text.contains("threads are whole things"), "the reason is missing:\n{text}");
    }

    #[test]
    fn the_attack_stage_says_how_the_threads_are_split_before_anything_starts() {
        let mut session = session();
        session.go_to(STAGE_ATTACK);
        // Three threads, 30% of them: one for the attacker and two for everybody else.
        assert_eq!(session.attack_threads(), (1, 2));
        let text = draw(&session);
        assert!(text.contains("Attacker's share"), "the knob is missing:\n{text}");
        assert!(text.contains("30.0%"), "the share is missing:\n{text}");
        assert!(text.contains("1 / 2"), "the thread split is missing:\n{text}");
    }

    #[test]
    fn the_recap_admits_what_has_not_been_run_instead_of_inventing_it() {
        let mut session = session();
        session.go_to(STAGE_RECAP);
        let text = draw(&session);
        assert!(text.contains("What just happened"), "no panel title:\n{text}");
        assert!(text.contains("not run yet"), "an unrun stage was filled in:\n{text}");
        assert!(text.contains("7.16 MH/s"), "the 2009 figure is missing:\n{text}");
    }

    #[test]
    fn a_reader_can_jump_to_every_stage_and_each_one_draws_something() {
        let mut session = session();
        for stage in 0..SCRIPTS.len() {
            session.on(Action::Stage(stage));
            assert_eq!(session.stage(), stage);
            let text = draw(&session);
            assert!(text.contains('╭'), "stage {stage} drew no panel:\n{text}");
            assert!(
                text.chars().filter(|c| c.is_alphanumeric()).count() > 40,
                "stage {stage} drew an empty panel:\n{text}"
            );
        }
        session.close();
    }

    #[test]
    fn every_stage_with_knobs_offers_them_and_the_others_offer_none() {
        let mut session = session();
        for stage in 0..SCRIPTS.len() {
            session.go_to(stage);
            let expected = matches!(stage, STAGE_TUNE | STAGE_ATTACK);
            assert_eq!(!session.knobs().is_empty(), expected, "stage {stage}");
            assert_eq!(session.chosen_knob().is_some(), expected, "stage {stage}");
        }
    }

    #[test]
    fn a_typed_number_beats_the_presets_and_a_refused_one_changes_nothing() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        for c in "24".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 24);
        for c in "99".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 24, "an out-of-range number was taken");
    }

    #[test]
    fn changing_the_miner_count_changes_how_many_shares_there_are_to_set() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        assert_eq!(session.knobs().len(), KNOB_SHARES_FROM + 2);
        session.chosen = KNOB_MINERS;
        session.on(Action::Nudge(1));
        assert_eq!(session.miner_count(), 3);
        assert_eq!(session.knobs().len(), KNOB_SHARES_FROM + 3);
        session.on(Action::Nudge(-1));
        session.on(Action::Nudge(-1));
        assert_eq!(session.miner_count(), 1);
        assert_eq!(session.knobs().len(), KNOB_SHARES_FROM + 1);
    }

    #[test]
    fn the_split_is_worked_out_before_anything_is_started() {
        let session = session();
        let rows = session.split_rows().expect("three threads split between two miners");
        assert_eq!(rows.len(), 2);
        assert_eq!(rows.iter().map(|row| row.threads).sum::<usize>(), 3);
        assert!((rows[0].requested - 0.6).abs() < 1e-9);
        assert!(rows[0].effective > rows[0].requested, "the rounding went the other way");
    }

    #[test]
    fn asking_for_fewer_threads_than_miners_is_refused_in_words() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        session.chosen = KNOB_THREADS;
        for c in "1".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert!(session.split_rows().is_err(), "one thread cannot carry two miners");
        let text = draw(&session);
        assert!(text.contains("Fewer threads than miners"), "no refusal on screen:\n{text}");
    }

    #[test]
    fn a_run_starts_real_threads_and_closing_brings_them_home() {
        let mut session = session();
        session.go_to(STAGE_MINE);
        walk(&mut session);
        assert_eq!(session.run_state(), RunState::Running);
        session.tick();
        assert!(session.mining_snapshot.is_some(), "no snapshot to draw from");
        session.close();
        assert!(session.mining.is_none(), "a worker was left running");
        assert_eq!(session.run_state(), RunState::Idle);
    }

    /// Leaving a stage must stop its threads. Two heavy runs on one machine halve each other, and
    /// a 51% attack that loses because the last stage is still mining is a lie about the subject.
    #[test]
    fn walking_out_of_a_stage_stops_what_it_started() {
        let mut session = session();
        session.go_to(STAGE_MINE);
        walk(&mut session);
        assert!(session.mining.is_some());
        session.go_to(STAGE_ATTACK);
        assert!(session.mining.is_none(), "mining carried on into the attack");
        assert_eq!(session.run_state(), RunState::Idle);
        session.close();
    }

    /// Ticks the way the shell's clock does, until the run has answered. Fails rather than hanging
    /// if it never does.
    fn run_until(session: &mut Session, answered: impl Fn(&Session) -> bool) {
        let started = Instant::now();
        while !answered(session) {
            assert!(
                started.elapsed() < Duration::from_secs(60),
                "the run never answered, so there is nothing to recap"
            );
            session.tick();
            std::thread::sleep(Duration::from_millis(5));
        }
    }

    /// A reader reaches the recap by pressing Tab, which is `go_to`, and their numbers have to
    /// survive the journey. Six of the seven rows once read "not run yet" after a real run.
    /// Two miners can solve the same height, and both are recorded. A reviewer read the pair as
    /// one block reported twice — "+ block 10 · gap 7s" then "+ block 10 · gap 0.00s" — with a
    /// green mark on the one the panel was marking red.
    #[test]
    fn the_loser_of_a_race_is_not_said_as_another_block() {
        let session = session();
        let english = Language::ENGLISH;
        let won = Happening::Block {
            height: 10,
            gap: Duration::from_secs(7),
            miner: 1,
            on_the_chain: true,
        };
        let lost = Happening::Block {
            height: 10,
            gap: Duration::from_secs(0),
            miner: 2,
            on_the_chain: false,
        };
        let won = session.beat_for(&won, english);
        let lost = session.beat_for(&lost, english);
        println!("won:  {}\nlost: {}", won.text, lost.text);
        assert_ne!(won.text, lost.text, "the two blocks read the same");
        assert!(lost.text.contains("not on the chain"), "{}", lost.text);
        assert!(
            matches!(won.voice, Voice::Event(Some(State::Good))),
            "the block on the chain is not marked good"
        );
        assert!(
            matches!(lost.voice, Voice::Event(Some(State::Bad))),
            "the block that lost the race is not marked apart"
        );
    }

    /// "found by Miner 2" arrived before anything had said there was more than one miner.
    #[test]
    fn the_opening_line_of_a_run_says_how_many_miners_there_are() {
        let session = session();
        for language in Language::ALL {
            let said = session
                .beat_for(&Happening::MiningStarted { miners: 2, threads: 11, bits: 27 }, *language)
                .text;
            println!("{language}: {said}");
            let miners = Msg::KnobMiners.text(*language);
            assert!(said.contains(miners), "the miner count is missing from {said:?}");
        }
    }

    /// A reviewer pressed Enter without moving anything and was told "blocks cost twice what
    /// they did", and was told "above half it catches up" under an attacker holding 27%.
    #[test]
    fn what_is_said_after_a_run_is_read_off_the_run() {
        let english = Language::ENGLISH;
        let mut session = session();
        KqSession::go_to(&mut session, STAGE_TUNE);

        assert_eq!(session.tell(Topic::Difficulty, english), Msg::TuneBitsUnchanged.text(english));
        let raised = session.bits_on_entry as u64 + 1;
        if let KnobValue::Count { current, .. } = &mut session.tune_knobs[KNOB_DIFFICULTY].value {
            *current = raised;
        }
        assert_eq!(session.tell(Topic::Difficulty, english), Msg::TuneBitsUp.text(english));

        assert_eq!(session.tell(Topic::Attack, english), Msg::AttackNotYetRun.text(english));
        for (share, outcome, expected) in [
            (0.30, Some(AttackOutcome::GaveUp), Msg::AttackLost),
            (0.30, Some(AttackOutcome::Succeeded), Msg::AttackLuckyWin),
            (0.51, Some(AttackOutcome::Succeeded), Msg::AttackWon),
            (0.51, Some(AttackOutcome::GaveUp), Msg::AttackRanOut),
        ] {
            let mut snapshot = blank_attack();
            snapshot.attacker_share = share;
            snapshot.outcome = outcome;
            session.attack_snapshot = Some(snapshot);
            assert_eq!(
                session.tell(Topic::Attack, english),
                expected.text(english),
                "share {share} ending {outcome:?} was described wrongly"
            );
        }
    }

    #[test]
    fn the_recap_keeps_the_numbers_the_reader_made_on_the_way_to_it() {
        let mut session = session();
        // The lowest practice difficulty the knob allows, so the two real runs this test drives
        // are over in milliseconds rather than seconds.
        session.go_to(STAGE_TUNE);
        session.chosen = KNOB_DIFFICULTY;
        for c in "16".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 16);

        session.go_to(STAGE_MINE);
        walk(&mut session);
        run_until(&mut session, |session| {
            session.mining_snapshot.as_ref().is_some_and(|run| run.blocks_in_chain >= 1)
        });
        let mined = format::count(
            session.mining_snapshot.as_ref().expect("a run to recap").blocks_in_chain,
        );

        session.go_to(STAGE_ATTACK);
        walk(&mut session);
        run_until(&mut session, |session| {
            session.attack_snapshot.as_ref().is_some_and(|run| run.outcome.is_some())
        });

        session.go_to(STAGE_RECAP);
        let text = draw(&session);
        session.close();
        // Six of the seven rows have nothing else to say when the results are gone, so this one
        // line is the whole defect.
        assert!(!text.contains("not run yet"), "the recap forgot the reader's own run:\n{text}");
        assert!(text.contains("16 zero bits"), "the difficulty the reader set is gone:\n{text}");
        let counted = Msg::RecapBlocks.text(Language::ENGLISH);
        assert!(
            text.lines().any(|line| line.contains(counted) && line.contains(&mined)),
            "the recap counts blocks the reader did not mine:\n{text}"
        );
    }

    /// `r` is the one thing that forgets results, and only the ones made where it was pressed.
    #[test]
    fn r_forgets_this_stages_numbers_and_leaves_the_others_alone() {
        let mut session = session();
        session.go_to(STAGE_MINE);
        walk(&mut session);
        session.tick();
        assert!(session.mining_snapshot.is_some());

        session.go_to(STAGE_ATTACK);
        session.on(Action::Reset);
        assert!(session.mining_snapshot.is_some(), "`r` on the attack threw away the mining");

        session.go_to(STAGE_MINE);
        session.on(Action::Reset);
        assert!(session.mining_snapshot.is_none(), "`r` kept the run it was pressed on");
        session.close();
    }
}
