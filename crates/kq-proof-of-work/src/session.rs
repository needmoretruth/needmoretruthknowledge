//! The quest a reader has opened: five stages driven by one mining engine.
//!
//! The engine (`nmtk-pow`) owns the threads and hands back a snapshot; this file turns that
//! snapshot into a screen. Nothing here hashes anything, and nothing in the engine says a word.

use std::time::Duration;

use nmtk_core::{Language, MachineProfile, format};
use nmtk_kq::knob::{Knob, KnobValue, Settled, Typed};
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
/// Miner shares are weights, not percentages. Shown as percentages they read as parts of a
/// whole, and three miners asking for 60%, 40% and 50% look like a broken screen adding to 150.
const SHARE_PRESETS: &[u64] = &[1, 2, 3, 4];
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
const DEFAULT_SHARES: [u64; MAX_MINERS] = [3, 2, 2, 2];
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

/// The most blocks and remarks one run will report before it stops reporting them. Blocks keep
/// arriving while a reader reads, and a conversation that grows without end is a log, not a
/// conversation.
///
/// Only the flood is capped. The cap used to count everything, so on a stage that had already
/// reported its two dozen blocks a typed number that landed somewhere else, a refusal, or the
/// ending of the attack went unsaid — which reads as a broken key, or a run that never ended.
/// And it is counted per run, not per stage: a stage that runs twice says the second run too.
const EVENT_CAP: usize = 24;

/// How many attacks the recap names one by one before it counts the rest.
const ATTACKS_RECALLED: usize = 2;

/// How close the attacker's share of threads has to come to the share that was asked for.
///
/// Threads are whole things, and on a small machine the nearest whole split can be a long way
/// off — or on the wrong side of half. See [`attack_split`].
const SHARE_TOLERANCE: f64 = 0.01;

/// The most threads an attack is given to express a share the machine's own threads cannot.
const MAX_ATTACK_THREADS: usize = 100;

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
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Topic {
    /// How the attack that just finished ended, and at what share.
    Attack,
    /// What moving the difficulty did to the cost of a block.
    Difficulty,
    /// Everything this reader mined, across every run of the quest.
    Mined,
    /// Whether they moved the numbers themselves, and from what to what.
    Tuned,
    /// Every attack they ran, one line each.
    AttacksRan,
    /// What those attacks, taken together, mean.
    AttacksMeant,
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
    Say(Msg::PuzzleWhatAHashIs),
    Say(Msg::PuzzleHashOneWay),
    // The header is named before its size is given in bytes, and bytes are explained before the
    // target is counted in bits. The other way round used "80 bytes of the block header" two
    // sentences before saying what a byte or a header was.
    Say(Msg::PuzzleThree),
    Say(Msg::PuzzleByte),
    Say(Msg::PuzzleBits),
    Say(Msg::PuzzleBitsDouble),
    Say(Msg::PuzzleFour),
    Say(Msg::PuzzleFive),
    Say(Msg::PuzzleSix),
    Say(Msg::PuzzleSeven),
];

/// The machine does it, and the conversation waits for the blocks.
const MINE_STAGE: &[Step] = &[
    // A stage can be walked into directly, so the words the run is about to use — miner, hash,
    // block, chain, thread — are said here, not only on the stage before, and before the panel
    // draws any of them: the recall first, so the opening panel can already say what a block
    // costs in hashes.
    Say(Msg::MineRecall),
    Say(Msg::MineChain),
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
    Say(Msg::MineRate),
    Say(Msg::MineEight),
    Say(Msg::MineNine),
    Say(Msg::MineTen),
    Say(Msg::MineEleven),
];

/// The reader's own numbers, twice, each answered by a real run.
///
/// The difficulty is said first because its value is the one on the panel from the first
/// moment, and the miners and threads next, before the values that count them are drawn.
const TUNE_STAGE: &[Step] = &[
    Say(Msg::TuneBits),
    Say(Msg::TuneWords),
    Say(Msg::TuneOne),
    Say(Msg::TuneKeys),
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
    // Chain and node, before the sentences that lean on them.
    Say(Msg::AttackChain),
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
    Tell(Topic::Mined),
    Tell(Topic::Tuned),
    Tell(Topic::AttacksRan),
    Tell(Topic::AttacksMeant),
    Say(Msg::RecapSix),
];

const SCRIPTS: [&[Step]; 6] = [WHY, PUZZLE, MINE_STAGE, TUNE_STAGE, ATTACK_STAGE, RECAP_STAGE];

/// Something that happened, kept as numbers so the conversation can be said again in any language.
enum Happening {
    MiningStarted {
        miners: usize,
        threads: usize,
        bits: u64,
    },
    Block {
        height: u64,
        gap: Duration,
        miner: usize,
        on_the_chain: bool,
    },
    FallingBehind {
        by: u64,
        elapsed: Duration,
    },
    AttackStarted {
        share: f64,
        confirmations: u64,
    },
    Paid,
    Released {
        confirmations: u64,
    },
    Ended {
        outcome: AttackOutcome,
        reverted: u64,
        took: Option<Duration>,
    },
    Refused(Msg),
    /// A typed number that fell outside the knob's range, with where it landed.
    PulledIn {
        to: String,
    },
    /// A typed number the knob could not read at all.
    NotANumber,
}

/// One attack that ended, kept after its run is gone.
#[derive(Debug, Clone, Copy)]
struct Attempt {
    /// Which stage ran it, so `r` there forgets it and `r` anywhere else does not.
    stage: usize,
    share: f64,
    won: bool,
    reverted: u64,
}

/// One mining run, kept after its threads are gone.
#[derive(Debug, Clone, Copy)]
struct MinedRun {
    /// Which run this is, so the run going now writes into its own entry and no other.
    id: u64,
    /// Which stage ran it, so `r` there forgets it and `r` anywhere else does not.
    stage: usize,
    /// The difficulty and the miner count it ran with.
    ///
    /// "You changed the numbers" used to be checked against the numbers on entering the recap,
    /// which are the numbers the recap was entered with — so it always said they were never
    /// changed. What the reader did is what they ran.
    bits: u32,
    miners: u64,
    /// Blocks it put on the chain: kept up to date while it runs, and settled from the run's own
    /// last word when it stops, so a block that landed after the last tick still counts.
    ///
    /// These used to be counted one by one as the conversation noticed them, from a snapshot
    /// that only carries the newest few dozen blocks. At the easiest difficulty a fast machine
    /// finds hundreds a second, so most of them were never noticed and the recap undercounted.
    blocks: u64,
    /// The fastest it went, every miner together.
    fastest: f64,
    /// The share the first miner asked for and the share it really got.
    split: Option<SplitRow>,
}

impl MinedRun {
    /// Reads a snapshot of this run into the record.
    fn read(&mut self, snapshot: &MiningSnapshot) {
        self.blocks = snapshot.blocks_in_chain;
        self.fastest = self.fastest.max(snapshot.total_hashrate);
    }
}

/// What this reader did, gathered across the whole quest rather than the last run.
///
/// The recap used to read the snapshot that happened to be lying around, which was whichever run
/// finished last — so a reader who mined thirty-seven blocks and then ran a four-block experiment
/// was told they had mined four.
///
/// Every entry carries the stage that made it. `r` forgets the stage's own and nothing else,
/// which is the rule the ledger quest keeps too: the record used to forget nothing at all, so a
/// stage the reader had reset went on being recapped.
#[derive(Debug, Clone, Default)]
struct Done {
    runs: Vec<MinedRun>,
    attacks: Vec<Attempt>,
}

impl Done {
    /// Blocks every run put on a chain.
    fn blocks(&self) -> u64 {
        self.runs.iter().map(|run| run.blocks).sum()
    }

    /// The fastest any run went.
    fn fastest(&self) -> f64 {
        self.runs.iter().map(|run| run.fastest).fold(0.0, f64::max)
    }

    /// The difficulty and miner count of the first run.
    fn first_run(&self) -> Option<(u32, u64)> {
        self.runs.first().map(|run| (run.bits, run.miners))
    }

    /// The difficulty and miner count of the latest run.
    fn last_run(&self) -> Option<(u32, u64)> {
        self.runs.last().map(|run| (run.bits, run.miners))
    }

    /// The first miner's split in the latest run that had one.
    fn last_split(&self) -> Option<SplitRow> {
        self.runs.iter().rev().find_map(|run| run.split)
    }

    /// Throws away what one stage produced.
    fn forget(&mut self, stage: usize) {
        self.runs.retain(|run| run.stage != stage);
        self.attacks.retain(|attempt| attempt.stage != stage);
    }
}

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
    /// The deficit the conversation has already remarked on, and when it last remarked, so a
    /// long losing attack says something every so often instead of going quiet for two minutes.
    said_deficit: u64,
    last_remark_at: Duration,
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
    /// The miner count as it stood when the tuning stage opened, for the same reason.
    miners_on_entry: u64,
    mining_snapshot: Option<MiningSnapshot>,
    /// The stage whose run [`Session::mining_snapshot`] came from, so `r` pressed elsewhere
    /// leaves it alone.
    mining_stage: Option<usize>,
    /// Blocks and remarks said about the run going now. The cap on them is per run: counted per
    /// stage, a first run on the tuning stage used all two dozen and the second run — the one that
    /// shows a third miner finding blocks — announced none.
    flood_this_run: usize,
    /// The record entry of the mining run going now, and how many runs have been started.
    live_run: Option<u64>,
    runs_started: u64,
    attack: Option<AttackHandle>,
    attack_snapshot: Option<AttackSnapshot>,
    rates: Vec<f64>,
    state: RunState,
    refused: Option<Msg>,
    /// The knob values the run in progress was started with, so turning one can be told apart
    /// from pressing Enter twice.
    running_with: Option<Settled>,
    /// What this reader has done in this quest, for the recap to read.
    done: Done,
    /// Which step started the run that is going, so what the run produces is filed against it.
    run_step: usize,
    /// The difficulty each mining step ran at, so a sentence about a run reads that run.
    ///
    /// A Tell is said again every time the conversation is drawn. Reading the knob instead made
    /// "every bit doubles the work" turn into "the difficulty is where it was" the moment the
    /// reader moved the knob back after the run.
    run_bits: Vec<(usize, u32)>,
    /// The attempt each attack step produced.
    ///
    /// A sentence about the first attack has to keep being about the first attack after the
    /// second one has run. Reading the live snapshot instead put "it still fell short" directly
    /// under the line saying a payment had been erased.
    attempts: Vec<(usize, Attempt)>,
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
            last_remark_at: Duration::ZERO,
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
            miners_on_entry: 2,
            mining_snapshot: None,
            mining_stage: None,
            flood_this_run: 0,
            live_run: None,
            runs_started: 0,
            attack: None,
            attack_snapshot: None,
            rates: Vec::new(),
            state: RunState::Idle,
            refused: None,
            running_with: None,
            done: Done::default(),
            run_step: 0,
            run_bits: Vec::new(),
            attempts: Vec::new(),
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

    /// The weight this miner asked for. The engine normalises them, so only the ratios matter.
    fn miner_share(&self, index: usize) -> f64 {
        count_of(self.tune_knobs.get(KNOB_SHARES_FROM + index)).max(1) as f64
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
                KnobValue::Count {
                    current: DEFAULT_SHARES[index],
                    min: 1,
                    max: 10,
                    step: 1,
                    presets: SHARE_PRESETS,
                },
            ));
        }
        let last = self.knob_count().saturating_sub(1);
        self.chosen = self.chosen.min(last);
    }

    /// How many of this stage's knobs the conversation has earned so far, counted from the first.
    ///
    /// A knob's label is a word like any other, and a value the reader can turn before anything
    /// has said what it is is a value turned blind. The tuning stage opens on the sentence about
    /// the difficulty, which is the first knob; the miners and threads follow the sentence that
    /// says what those are. The attack's share is plain words, and its confirmations wait for the
    /// sentence that explains them.
    fn knob_count(&self) -> usize {
        match self.stage {
            STAGE_TUNE if self.said(Msg::TuneWords) => self.tune_knobs.len(),
            STAGE_TUNE if self.said(Msg::TuneBits) => self.tune_knobs.len().min(1),
            STAGE_ATTACK if self.said(Msg::AttackTwo) => self.break_knobs.len(),
            STAGE_ATTACK => self.break_knobs.len().min(1),
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
        if index >= self.knob_count() {
            return Reaction::Ignored;
        }
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
            .map(|index| MinerSpec::percent(index as u32, self.miner_share(index)))
            .collect()
    }

    /// Stops whatever is running and starts mining with the current numbers.
    fn start_run(&mut self) {
        self.stop_attack();
        self.stop_mining();
        // The run about to start replaces the last one's numbers, and a start that is refused
        // leaves none behind: a wait for blocks must never be answered by a run already over.
        self.mining_snapshot = None;
        self.mining_stage = None;
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
                let snapshot = handle.snapshot();
                // Into the record the moment it starts, filed under the stage that started it:
                // this entry is the live run until it stops, and the one `r` here forgets.
                let id = self.runs_started;
                self.runs_started += 1;
                self.live_run = Some(id);
                self.done.runs.push(MinedRun {
                    id,
                    stage: self.stage,
                    bits: self.zero_bits(),
                    miners: snapshot.miners.len() as u64,
                    blocks: 0,
                    fastest: 0.0,
                    split: split_of(&snapshot).first().copied(),
                });
                self.mining_snapshot = Some(snapshot);
                self.mining_stage = Some(self.stage);
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
        // The share takes as many threads as it needs — 13 against 12 for 51% — and the machine
        // takes no more than the reader gave it: the threads take turns within the budget, and
        // every one of them hashes for the same share of the time, so the share holds.
        let config = AttackConfig::new(bits, attacker, honest)
            .with_confirmations(self.confirmations())
            .with_cpu_budget(self.thread_budget());
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
        attack_split(self.attacker_share(), self.thread_budget())
    }

    fn stop_mining(&mut self) {
        if let Some(handle) = self.mining.take() {
            // The run's last word, taken after its threads have stopped, is what goes into the
            // record: a block that landed after the last tick still counts.
            let last = handle.finish();
            if let Some(run) = self.live_record() {
                run.read(&last);
            }
            self.live_run = None;
            self.mining_snapshot = Some(last);
            self.state = RunState::Idle;
        }
    }

    /// The record entry of the mining run going now, if it is still in the record.
    fn live_record(&mut self) -> Option<&mut MinedRun> {
        let id = self.live_run?;
        self.done.runs.iter_mut().find(|run| run.id == id)
    }

    /// Every block this reader has put on a chain, in every run of the quest so far.
    fn mined_blocks(&self) -> u64 {
        self.done.blocks()
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
        self.flood_this_run = 0;
        self.said_blocks.clear();
        self.said_deficit = 0;
        self.last_remark_at = Duration::ZERO;
        self.said_paid = false;
        self.said_released = false;
        self.said_ended = false;
        self.refused = None;
    }

    /// Throws away the numbers the stage showing now produced, and only those — in the record the
    /// recap reads and on the panel. Pressing `r` is the one thing that does this: every other way
    /// out of a stage keeps them.
    ///
    /// Anything still running has to be stopped first. Stopping a run writes its last word into
    /// the record and onto the panel, so forgetting before it let the very run `r` was pressed to
    /// throw away straight back in, under a status line that said nothing had started.
    ///
    /// The knobs keep their values either way: a reader who set a number wants it kept.
    fn forget_results(&mut self) {
        let stage = self.stage;
        self.done.forget(stage);
        if self.mining_stage == Some(stage) {
            self.mining_snapshot = None;
            self.mining_stage = None;
            self.rates.clear();
        }
        if stage == STAGE_ATTACK {
            self.attack_snapshot = None;
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

    /// The opening stage's panel: one list, the copies of it, and the rule that picks one.
    ///
    /// It used to draw the puzzle — a header with a nonce in it, SHA-256, a target — beside a
    /// conversation about strangers agreeing on a list, which had said none of those words. This
    /// draws what the conversation is saying, a sentence at a time: the copies, then the one that
    /// lies, then the rule, then what the work is.
    fn why_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        let heading = |message: Msg| {
            wrapped("", message.text(language), width, theme.heading(), theme.heading())
        };
        let mut lines = heading(Msg::WhyCopies);
        let computer = Msg::WhyComputer.text(language);
        let names: Vec<String> = (1..=3).map(|number| format!("{computer} {number}")).collect();
        let name_cells = names.iter().map(|name| cells(name)).max().unwrap_or(0) + 2;
        // Any of them can lie: from the second sentence on, the third copy says something else.
        let lying = self.said(Msg::WhyTwo);
        for (index, name) in names.iter().enumerate() {
            let (mark, style, entry) = if lying && index == names.len() - 1 {
                (State::Bad.mark(), theme.bad(), Msg::WhyLie)
            } else {
                (" ", theme.plain(), Msg::WhyPaid)
            };
            lines.push(Line::from(vec![
                Span::styled(format!("{mark} "), theme.bad()),
                Span::styled(column(name, name_cells), theme.muted()),
                Span::styled(entry.text(language), style),
            ]));
        }
        lines.extend(wrapped(
            "  ",
            Msg::WhyMore.text(language),
            width,
            theme.muted(),
            theme.muted(),
        ));
        if self.said(Msg::WhyThree) {
            lines.push(Line::from(""));
            lines.extend(heading(Msg::WhyRuleHeading));
            let trust = format!("{} ", State::Bad.mark());
            lines.extend(wrapped(
                &trust,
                Msg::WhyRuleTrust.text(language),
                width,
                theme.bad(),
                theme.muted(),
            ));
            let work = format!("{} ", State::Good.mark());
            lines.extend(wrapped(
                &work,
                Msg::WhyRuleWork.text(language),
                width,
                theme.good(),
                theme.plain(),
            ));
        }
        if self.said(Msg::WhyFour) {
            lines.push(Line::from(""));
            let rows = [(Msg::WhyWork.text(language), Msg::WhyWorkIs.text(language).to_string())];
            lines.extend(stat_lines(&rows, width, theme));
        }
        frame.render_widget(Paragraph::new(lines), area);
    }

    /// The puzzle, drawn as the conversation explains it.
    ///
    /// The header is a header once the sentence about headers has been said, one of 80 bytes
    /// once bytes have, and one with a nonce in it once the nonce has a name; the loop back is
    /// guessing another number until then. What a block costs arrives with the sentence that
    /// points at it.
    fn puzzle_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let width = area.width as usize;
        if !self.said(Msg::PuzzleThree) {
            let note =
                wrapped("", Msg::PuzzleComing.text(language), width, theme.muted(), theme.muted());
            frame.render_widget(Paragraph::new(note), area);
            return;
        }
        let header = if self.said(Msg::PuzzleFive) {
            Msg::PuzzleHeader
        } else if self.said(Msg::PuzzleByte) {
            Msg::PuzzleHeaderBytes
        } else {
            Msg::PuzzleHeaderPlain
        };
        let mut diagram = wrapped("", header.text(language), width, theme.plain(), theme.plain());
        for step in [Msg::PuzzleHash, Msg::PuzzleCompare] {
            diagram.extend(wrapped(
                "  ↓  ",
                step.text(language),
                width,
                theme.muted(),
                theme.plain(),
            ));
        }
        let again = if self.said(Msg::PuzzleFive) {
            Some(Msg::PuzzleNo)
        } else if self.said(Msg::PuzzleFour) {
            Some(Msg::PuzzleNoGuess)
        } else {
            None
        };
        let answers = again.map(|no| (no, theme.muted())).into_iter();
        for (answer, style) in answers.chain([(Msg::PuzzleYes, theme.good())]) {
            let (mark, rest) = branch(answer.text(language));
            diagram.extend(wrapped(&format!("     {mark}"), rest, width, theme.plain(), style));
        }
        if !self.said(Msg::PuzzleSix) {
            frame.render_widget(Paragraph::new(diagram), area);
            return;
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
        // A run an earlier visit left behind waits until the conversation reaches the run again,
        // which starts a new one: drawn at the first sentence, its chain and its hash rate came
        // before either had been said.
        let showing = self.mining_snapshot.as_ref().filter(|_| self.reached_run());
        let Some(snapshot) = showing else {
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
            if self.said(Msg::MineRecall) {
                frame.render_widget(
                    Paragraph::new(stat_lines(&self.cost_rows(language), width, theme)),
                    rows,
                );
            }
            return;
        };

        // Each row arrives with the sentence that says its words. The rate waits for the one
        // saying what MH/s is; the chain for the one saying what a chain is.
        let chain = self.said(Msg::MineChain);
        let mut rows: Vec<(&str, String)> = Vec::new();
        if self.said(Msg::MineRate) {
            rows.push((
                Msg::LabelHashRate.text(language),
                format::hashrate(snapshot.total_hashrate),
            ));
        }
        rows.push((Msg::LabelHashes.text(language), format::count(snapshot.total_hashes)));
        rows.push((Msg::LabelBlocksFound.text(language), format::count(snapshot.blocks_found)));
        if chain {
            rows.push((Msg::LabelChainHeight.text(language), format::count(snapshot.height)));
        }
        rows.push((
            Msg::LabelHashesPerBlock.text(language),
            format::count(snapshot.expected_hashes_per_block as u64),
        ));
        // Only when it has happened. A row that reads zero for the whole run is a question the
        // screen asks the reader and never answers.
        if chain && snapshot.stale_blocks > 0 {
            rows.push((Msg::LabelStale.text(language), format::count(snapshot.stale_blocks)));
        }
        // One block takes, half take under, the 2009 network, and this machine against it: the
        // four sentences about difficulty 1 each bring their own line.
        let comparison: Vec<(&str, String)> = self
            .comparison_rows(snapshot.total_hashrate, language)
            .into_iter()
            .zip([Msg::MineNine, Msg::MineNine, Msg::MineTen, Msg::MineEleven])
            .filter(|(_, told)| self.said(*told))
            .map(|(row, _)| row)
            .collect();

        // Built as one run of lines rather than a fixed grid of rows: a value that has to go
        // under its label, or a sentence that has to become two, takes a row from the block list
        // at the bottom instead of running off the right-hand edge.
        let mut head = vec![self.status_line(snapshot.elapsed, language, theme), Line::from("")];
        head.extend(stat_lines(&rows, width, theme));
        head.push(Line::from(""));
        if self.said(Msg::MineEight) {
            head.extend(wrapped(
                "",
                Msg::HeadingComparison.text(language),
                width,
                theme.heading(),
                theme.heading(),
            ));
            head.extend(stat_lines(&comparison, width, theme));
            if self.said(Msg::MineTen) {
                head.extend(wrapped(
                    "",
                    Msg::ComparisonDerived.text(language),
                    width,
                    theme.muted(),
                    theme.muted(),
                ));
            }
            head.push(Line::from(""));
        }
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
        let shown: Vec<&BlockSummary> = blocks.iter().rev().take(area.height as usize).collect();
        // Measured from the heights on screen. A fixed five cells cut "10,000" to "10,0…" once a
        // fast machine at an easy difficulty had been mining for a minute.
        let height_cells =
            shown.iter().map(|block| cells(&format::count(block.height))).max().unwrap_or(0) + 1;
        let lines: Vec<Line<'static>> = shown
            .into_iter()
            .map(|block| {
                let state = if block.in_best_chain { State::Good } else { State::Bad };
                Line::from(vec![
                    Span::styled(format!("{} ", state.mark()), theme.state(state)),
                    Span::styled(column(&format::count(block.height), height_cells), theme.plain()),
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
        frame.render_widget(Paragraph::new(knob_lines), knobs);
        // The conversation tells the reader to read the cost line here, so the cost line is here.
        // It used to be on the stage before this one, where the difficulty could not be moved.
        if self.said(Msg::TuneBits)
            && let Some((label, value)) = self.cost_rows(language).into_iter().next()
        {
            frame.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(format!("{label}  "), theme.muted()),
                    Span::styled(value, theme.plain()),
                ])),
                gap,
            );
        }
        let footer_lines = match self.refused {
            Some(message) => wrapped("", message.text(language), width, theme.bad(), theme.bad()),
            None => Vec::new(),
        };
        // Miners and threads, in a table headed by both words, wait for the sentence saying what
        // they are.
        if !self.said(Msg::TuneWords) {
            frame.render_widget(Paragraph::new(footer_lines), footer);
            return;
        }
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
        frame.render_widget(Paragraph::new(footer_lines), footer);
    }

    fn tune_knob_lines(
        &self,
        width: usize,
        language: Language,
        theme: Theme,
    ) -> Vec<Line<'static>> {
        let labels: Vec<Msg> = (0..self.knobs().len())
            .map(|index| match index {
                KNOB_DIFFICULTY => Msg::KnobDifficulty,
                KNOB_MINERS => Msg::KnobMiners,
                KNOB_THREADS => Msg::KnobThreads,
                other => phrases::share_label(other - KNOB_SHARES_FROM),
            })
            .collect();
        let column_width = label_column(&labels, width, language);
        self.knobs()
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
        let shown = self.knobs().len().min(labels.len());
        let column_width = label_column(&labels[..shown], width, language);
        self.knobs()
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

        // An attack an earlier visit left behind waits until the conversation reaches the attack:
        // drawn at the first sentence, its chains and confirmations came before either was said.
        let showing = self.attack_snapshot.as_ref().filter(|_| self.reached_run());
        let Some(snapshot) = showing else {
            let (attacker, honest) = self.attack_threads();
            let mut lines =
                vec![Line::from(Span::styled(Msg::StatusIdle.text(language), theme.muted()))];
            if let Some(message) = self.refused {
                lines.extend(wrapped("", message.text(language), width, theme.bad(), theme.bad()));
                lines.push(Line::from(""));
            }
            // The share the threads will really hold, which is what decides the race: 51% asked
            // is 13 threads of 25. How many threads that takes is the engine's business — they
            // take turns within the reader's budget — and this stage never says what a thread
            // is, so the screen says the share and not the count.
            let held = attacker as f64 / (attacker + honest) as f64;
            let rows = vec![(Msg::LabelShareHeld.text(language), format::percent(held))];
            lines.extend(stat_lines(&rows, width, theme));
            frame.render_widget(Paragraph::new(lines), body);
            return;
        };

        // While it runs, the phase says what is happening; once it is over, the ending says it,
        // and the row the phase took goes to the ending. At 80x24 that row is the difference
        // between the ending being on the screen and being cut off the bottom of it.
        let mut lines = vec![self.status_line(snapshot.elapsed, language, theme)];
        if snapshot.outcome.is_none() {
            lines.push(Line::from(Span::styled(
                phrases::phase(snapshot.phase).text(language),
                theme.muted(),
            )));
        }
        lines.push(Line::from(""));
        lines.extend(stat_lines(&self.attack_rows(snapshot, language), width, theme));
        lines.extend(self.ending_lines(snapshot, width, language, theme));
        frame.render_widget(Paragraph::new(lines), body);
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
        // While the race is on, the gap is the thing to watch. Once it is over the gap is zero
        // whoever won — the losing chain has been abandoned — and a screen still showing "ahead
        // by 0 blocks" beside a payment that was erased is two screens disagreeing.
        match snapshot.outcome {
            Some(outcome) => {
                rows.push((
                    Msg::LabelAtTheEnd.text(language),
                    phrases::short_outcome(outcome).text(language).to_string(),
                ));
                let won = outcome == AttackOutcome::Succeeded;
                rows.push((
                    Msg::LabelChainNow.text(language),
                    if won {
                        Msg::ChainNowAttacker.text(language).to_string()
                    } else {
                        Msg::ChainNowHonest.text(language).to_string()
                    },
                ));
            }
            None => {
                // The label says what is counted, so the value is only the number.
                let gap = snapshot.lead.unsigned_abs();
                rows.push((
                    if snapshot.lead >= 0 {
                        Msg::LabelAheadBy.text(language)
                    } else {
                        Msg::LabelBehindBy.text(language)
                    },
                    format::count(gap),
                ));
            }
        }
        rows.push((Msg::LabelFurthestBehind.text(language), format::count(snapshot.max_deficit)));
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
        rows.push((Msg::LabelMerchantSaw.text(language), format::count(seen)));
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
        // The share the run's threads really held. With more threads than the reader's budget
        // they took turns, every one for the same share of the time, so this is also the share of
        // the hashing — and the screen shows no hash rate, which under turns would be the rate
        // of a machine held back to its budget rather than what it can do.
        rows.push((Msg::LabelShareHeld.text(language), format::percent(snapshot.attacker_share)));
        rows
    }

    fn recap_panel(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language) {
        let unknown = || Msg::NotYet.text(language).to_string();
        // The recap is about the quest, not about whichever run happened to finish last: it
        // reads the record every run wrote into, so a short experiment after a long run cannot
        // shrink what the reader mined.
        let mined = self.mined_blocks() > 0;
        let rate = self.done.fastest();
        let real = Target::difficulty_one();
        let network = implied_network_hashrate(real, BITCOIN_TARGET_BLOCK_SECONDS);

        // The split and the difficulty are the ones a run really used, so they come from the
        // record of the runs — not from the knobs, which say what the next run would use.
        let split = self.done.last_split();
        let attacks = &self.done.attacks;
        // Each row arrives with the sentence that says its words: the difficulty with the one
        // about zero bits, the reader's own numbers with the one that reads them, the miners'
        // split with the one about the miners. A recap entered cold has said none of it yet.
        let mining = self.told(Topic::Mined);
        let tuned = self.told(Topic::Tuned) && split.is_some();
        let attacked = self.told(Topic::AttacksRan);

        let rows: Vec<(bool, &'static str, String)> = vec![
            (
                true,
                Msg::RecapDifficulty.text(language),
                match self.done.last_run() {
                    Some((bits, _)) => in_zero_bits(bits, language),
                    None => unknown(),
                },
            ),
            (
                mining,
                Msg::RecapHashRate.text(language),
                if mined { format::hashrate(rate) } else { unknown() },
            ),
            (
                mining,
                Msg::RecapBlocks.text(language),
                if mined { format::count(self.mined_blocks()) } else { unknown() },
            ),
            (
                mining,
                Msg::RecapAtDifficultyOne.text(language),
                if mined {
                    span(expected_time_to_block(rate, real).expected_seconds, language)
                } else {
                    unknown()
                },
            ),
            (self.said(Msg::RecapTwo), Msg::RecapNetwork.text(language), format::hashrate(network)),
            (
                tuned,
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
                attacked,
                Msg::RecapAttacksRun.text(language),
                if attacks.is_empty() { unknown() } else { format::count(attacks.len() as u64) },
            ),
            (
                attacked,
                Msg::RecapAttacksWon.text(language),
                if attacks.is_empty() {
                    unknown()
                } else {
                    format::count(attacks.iter().filter(|attempt| attempt.won).count() as u64)
                },
            ),
        ];
        let rows: Vec<(&'static str, String)> = rows
            .into_iter()
            .filter(|(shown, _, _)| *shown)
            .map(|(_, name, value)| (name, value))
            .collect();

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

    /// The steps of this stage the reader has been shown.
    fn shown_steps(&self) -> impl Iterator<Item = &'static Step> {
        self.script().iter().take(self.revealed + 1)
    }

    /// Whether this stage's conversation has said `message` yet.
    ///
    /// The panel reads this before it draws a word. Every word on screen is explained at or
    /// before its first appearance, in the same stage: the opening stage used to draw a nonce,
    /// SHA-256, a target and a header beside a conversation that had not yet said any of them,
    /// and a reader who meets an unexplained word stops reading and starts guessing.
    fn said(&self, message: Msg) -> bool {
        self.shown_steps().any(|step| matches!(step, Say(said) | Ask(said) if *said == message))
    }

    /// Whether this stage's conversation has reached its sentence about `topic`.
    fn told(&self, topic: Topic) -> bool {
        self.shown_steps().any(|step| matches!(step, Tell(told) if *told == topic))
    }

    /// Whether this stage's conversation has reached the work it runs. Before then the panel
    /// shows what the run is about to do, not the numbers an earlier visit left behind.
    fn reached_run(&self) -> bool {
        self.shown_steps().any(|step| matches!(step, Run(_)))
    }

    /// Back to the first sentence of this stage, with nothing running and nothing said.
    ///
    /// What finished runs produced is kept, so a recap reached from here — by Tab, by a digit key,
    /// by any route at all — still has the reader's own numbers in it.
    fn restart(&mut self) {
        self.stop_runs();
        self.forget_run();
        self.revealed = 0;
        // Back at the first sentence, fewer knobs have been named.
        self.chosen = self.chosen.min(self.knob_count().saturating_sub(1));
        self.log.clear();
        self.attempts.clear();
        self.run_bits.clear();
        // The values the last run started with belong to that run. Kept across a restart they
        // made a knob turned before this stage's first run read as "turned since the run", and
        // the key bar offered to run it again when Enter would only carry the conversation on.
        self.running_with = None;
    }

    /// A sentence about the attack that finished, read off that attack.
    ///
    /// Four endings, because a race is not arithmetic alone: below half it usually loses and
    /// sometimes wins, and above half it usually wins and sometimes runs out of time first.
    fn tell(&self, step: usize, topic: Topic, language: Language) -> String {
        match topic {
            Topic::Attack => self.tell_attack(step, language),
            Topic::Mined => self.tell_mined(language),
            Topic::Tuned => self.tell_tuned(language),
            Topic::AttacksRan => self.tell_attacks_ran(language),
            Topic::AttacksMeant => self.tell_attacks_meant(language),
            Topic::Difficulty => {
                // The reader sets the difficulty, so the sentence reads the difficulty rather
                // than the one the conversation suggested. Pressing Enter without moving it used
                // to be answered with "blocks cost twice what they did".
                let now = self
                    .run_bits
                    .iter()
                    .filter(|(at, _)| *at <= step)
                    .max_by_key(|(at, _)| *at)
                    .map(|(_, bits)| *bits)
                    .unwrap_or_else(|| self.zero_bits());
                let message = match now.cmp(&self.bits_on_entry) {
                    std::cmp::Ordering::Equal => Msg::TuneBitsUnchanged,
                    std::cmp::Ordering::Greater => Msg::TuneBitsUp,
                    std::cmp::Ordering::Less => Msg::TuneBitsDown,
                };
                message.text(language).to_string()
            }
        }
    }

    /// What this reader mined, across every run rather than the last one.
    fn tell_mined(&self, language: Language) -> String {
        if self.mined_blocks() == 0 {
            return Msg::RecapMinedNone.text(language).to_string();
        }
        format!(
            "{} {}. {} {}.",
            Msg::RecapMinedBlocks.text(language),
            format::count(self.mined_blocks()),
            Msg::RecapMinedFastest.text(language),
            format::hashrate(self.done.fastest()),
        )
    }

    /// Whether they moved the numbers themselves, and from what to what: the first run they
    /// mined against the last. Nothing at all when they have not mined, because the line before
    /// has already said so.
    fn tell_tuned(&self, language: Language) -> String {
        let (Some((first_bits, first_miners)), Some((bits, miners))) =
            (self.done.first_run(), self.done.last_run())
        else {
            return String::new();
        };
        if bits == first_bits && miners == first_miners {
            return Msg::RecapTunedNo.text(language).to_string();
        }
        let mut text = Msg::RecapTunedYes.text(language).to_string();
        if bits != first_bits {
            text.push_str(&format!(
                "  ·  {} {} → {}",
                Msg::KnobDifficulty.text(language),
                first_bits,
                bits,
            ));
        }
        if miners != first_miners {
            text.push_str(&format!(
                "  ·  {} {} → {}",
                Msg::KnobMiners.text(language),
                first_miners,
                miners,
            ));
        }
        text
    }

    /// Every attack that ended, one clause each, in the order they were run.
    fn tell_attacks_ran(&self, language: Language) -> String {
        if self.done.attacks.is_empty() {
            return Msg::RecapAttackNone.text(language).to_string();
        }
        let mut text = Msg::RecapAttackRan.text(language).to_string();
        // The newest few, and a count of the rest. One clause per attack grew past what one beat
        // can hold by the third attack, and a reader who runs the bench ten times ran ten.
        let shown = self.done.attacks.len().min(ATTACKS_RECALLED);
        let earlier = self.done.attacks.len() - shown;
        if earlier > 0 {
            text.push_str(&format!("  ·  {} {earlier}", Msg::RecapAttackEarlier.text(language)));
        }
        for attempt in &self.done.attacks[earlier..] {
            let outcome = if attempt.won {
                phrases::short_outcome(AttackOutcome::Succeeded)
            } else {
                phrases::short_outcome(AttackOutcome::GaveUp)
            };
            text.push_str(&format!(
                "  ·  {} {}",
                format::percent(attempt.share),
                outcome.text(language),
            ));
            if attempt.won && attempt.reverted > 0 {
                text.push_str(&format!(
                    " ({} {})",
                    Msg::EventErased.text(language),
                    attempt.reverted
                ));
            }
        }
        text
    }

    /// What those attacks, taken together, say about the number 51.
    fn tell_attacks_meant(&self, language: Language) -> String {
        let attacks = &self.done.attacks;
        if attacks.is_empty() {
            return String::new();
        }
        let won_below = attacks.iter().any(|a| a.won && side(a.share) == Side::Below);
        let won_above = attacks.iter().any(|a| a.won && side(a.share) == Side::Above);
        let lost_above = attacks.iter().any(|a| !a.won && side(a.share) == Side::Above);
        let at_half = attacks.iter().any(|a| side(a.share) == Side::Half);
        let message = if won_below {
            Msg::RecapAttackWonBelowHalf
        } else if lost_above {
            Msg::RecapAttackLostAboveHalf
        } else if won_above {
            Msg::RecapAttackWonAboveHalf
        } else if at_half {
            // Nothing above half ran, nothing below half won, and something ran at exactly half.
            // "Every one of them fell behind" would be about attacks that were never below half.
            Msg::RecapAttackEven
        } else {
            Msg::RecapAttackAllLost
        };
        message.text(language).to_string()
    }

    /// How the attack that this step ran ended — that one, not whichever ran last.
    fn tell_attack(&self, step: usize, language: Language) -> String {
        let attempt = self
            .attempts
            .iter()
            .filter(|(at, _)| *at <= step)
            .max_by_key(|(at, _)| *at)
            .map(|(_, attempt)| *attempt);
        let Some(attempt) = attempt else {
            return Msg::AttackNotYetRun.text(language).to_string();
        };
        // Exactly half is neither: "above half it catches up" is not true of a tie.
        let message = match (side(attempt.share), attempt.won) {
            (Side::Below, true) => Msg::AttackLuckyWin,
            (Side::Below, false) => Msg::AttackLost,
            (Side::Half, true) => Msg::AttackEvenWon,
            (Side::Half, false) => Msg::AttackEvenLost,
            (Side::Above, true) => Msg::AttackWon,
            (Side::Above, false) => Msg::AttackRanOut,
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
            let settled = Settled::of(self.knobs());
            self.running_with = Some(settled);
            self.run_step = self.revealed;
            // A new run, so a new allowance of blocks and remarks.
            self.flood_this_run = 0;
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
                        let bits = self.zero_bits();
                        let at = self.revealed;
                        self.run_bits.retain(|(step, _)| *step != at);
                        self.run_bits.push((at, bits));
                        self.say(Happening::MiningStarted {
                            miners,
                            threads,
                            bits: u64::from(bits),
                        });
                    }
                }
                Attack => {
                    self.said_paid = false;
                    self.said_released = false;
                    self.said_ended = false;
                    self.start_the_attack();
                    if let Some(snapshot) = &self.attack_snapshot {
                        // The share the threads really hold, which is what the panel shows and
                        // what decides the race — not the number typed into the knob.
                        let share = snapshot.attacker_share;
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

    /// Whether the next step of this stage's conversation starts a run.
    fn next_is_a_run(&self) -> bool {
        matches!(self.script().get(self.revealed + 1), Some(Run(_)))
    }

    /// Whether the reader has turned a knob since the run in progress started.
    fn knobs_moved(&self) -> bool {
        match &self.running_with {
            Some(settled) => !settled.still(self.knobs()),
            None => false,
        }
    }

    /// Runs this stage's work again with the values now on screen.
    ///
    /// The conversation rewinds to the run itself rather than appending, so the new lines arrive
    /// where they belong and the sentence that reads them is said again about the run that just
    /// happened rather than about the one before it.
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
        // The run being replaced stays in the record: running the bench again is not `r`, and the
        // recap is about every run the reader made. What it put on the panel goes when the new
        // run starts.
        self.stop_runs();
        self.forget_run();
        self.log.retain(|logged| logged.step < at);
        self.revealed = at - 1;
        self.advance();
        Reaction::Handled
    }

    /// Files one happening against the step the reader is on.
    fn say(&mut self, what: Happening) {
        if matches!(what, Happening::Block { .. } | Happening::FallingBehind { .. }) {
            if self.flood_this_run >= EVENT_CAP {
                return;
            }
            self.flood_this_run += 1;
        }
        self.log.push(Logged { step: self.revealed, what });
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
        // An attack that loses spends two minutes losing, and a screen that says nothing for two
        // minutes reads as a hang. Whichever comes first: another couple of blocks behind, or
        // half a minute of nothing said.
        const BLOCKS_BETWEEN_REMARKS: u64 = 2;
        const SECONDS_BETWEEN_REMARKS: u64 = 30;
        let losing = self.attack_snapshot.as_ref().filter(|snapshot| {
            snapshot.outcome.is_none()
                && snapshot.max_deficit > 0
                && (snapshot.max_deficit >= self.said_deficit + BLOCKS_BETWEEN_REMARKS
                    || snapshot.elapsed
                        >= self.last_remark_at + Duration::from_secs(SECONDS_BETWEEN_REMARKS))
        });
        if let Some((by, elapsed)) = losing.map(|s| (s.max_deficit, s.elapsed)) {
            self.said_deficit = by;
            self.last_remark_at = elapsed;
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
            // The panel keeps the worst deficit; the conversation only said the last one it
            // remarked on. Ending one short of the panel reads as two screens disagreeing.
            let (deficit, elapsed, share) = self
                .attack_snapshot
                .as_ref()
                .map(|s| (s.max_deficit, s.elapsed, s.attacker_share))
                .unwrap_or((0, Duration::ZERO, 0.0));
            if deficit > self.said_deficit {
                self.said_deficit = deficit;
                self.say(Happening::FallingBehind { by: deficit, elapsed });
            }
            let attempt = Attempt {
                stage: self.stage,
                share,
                won: outcome == AttackOutcome::Succeeded,
                reverted,
            };
            self.done.attacks.push(attempt);
            let at = self.run_step;
            self.attempts.retain(|(step, _)| *step != at);
            self.attempts.push((at, attempt));
            self.say(Happening::Ended { outcome, reverted, took });
        }
    }

    /// One happening, said in the reader's language.
    fn beat_for(&self, what: &Happening, language: Language) -> Beat {
        match what {
            Happening::MiningStarted { miners, threads, bits } => {
                let mut text = format!(
                    "{}  ·  {} {}  ·  {} {}",
                    Msg::EventMiningStarted.text(language),
                    Msg::KnobMiners.text(language),
                    miners,
                    Msg::KnobThreads.text(language),
                    threads,
                );
                // The difficulty goes in where this stage has said what zero bits are. The mining
                // stage never does, and a unit nobody has explained is a word to guess at.
                if self.said(Msg::TuneBits) {
                    text.push_str(&format!("  ·  {}", in_zero_bits(bits, language)));
                }
                Beat::event(text)
            }
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
                    "{}  ·  {}  ·  {} {}",
                    Msg::EventFallingBehind.text(language),
                    blocks(*by, language),
                    Msg::EventElapsed.text(language),
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
        self.bits_on_entry = self.zero_bits();
        self.miners_on_entry = self.miner_count() as u64;
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
                Tell(topic) => {
                    // A Tell with nothing to read says nothing, rather than a blank beat.
                    let told = self.tell(index, *topic, language);
                    if !told.is_empty() {
                        beats.push(Beat::say(told));
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
            // A waiting step goes on by itself, as soon as the machine has answered.
            Some(Await(until)) => self.satisfied(*until),
            _ => self.revealed + 1 < self.script().len(),
        }
    }

    fn knobs(&self) -> &[Knob] {
        let shown = self.knob_count();
        match self.stage {
            STAGE_TUNE => &self.tune_knobs[..shown.min(self.tune_knobs.len())],
            STAGE_ATTACK => &self.break_knobs[..shown.min(self.break_knobs.len())],
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
                // The stage has said everything it has to say. Where there are knobs, Enter runs
                // it again with the values now on screen — an Ask is a suggestion, and a
                // suggestion the reader takes has to do something. Walking on is Tab's job.
                // Where there are no knobs there is nothing to run, and the shell walks.
                self.rerun()
            }
            Action::Reset => {
                // Stop, then forget, as running it again does: stopping writes the run's last
                // word back, so the other way round kept the run `r` was pressed to throw away.
                self.restart();
                self.forget_results();
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
            Action::Commit => {
                let mut typed = Typed::Nothing;
                let taken = self.change_chosen(|knob| typed = knob.commit());
                // A number that went nowhere reads as a broken key unless the screen says what
                // happened to it.
                match typed {
                    Typed::PulledIn { to } => self.say(Happening::PulledIn { to }),
                    Typed::NotANumber => self.say(Happening::NotANumber),
                    Typed::Taken | Typed::Nothing => {}
                }
                taken
            }
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
            // The live run keeps its own entry in the record up to date.
            if let Some(run) = self.live_record() {
                run.read(&snapshot);
            }
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
            STAGE_WHY => Msg::WhyTitle,
            STAGE_PUZZLE => Msg::PuzzleTitle,
            STAGE_MINE => Msg::RunTitle,
            STAGE_TUNE => Msg::TuneTitle,
            STAGE_ATTACK => Msg::BreakTitle,
            _ => Msg::RecapTitle,
        };
        let block = theme.titled_panel(title.text(language));
        let inner = block.inner(area);
        frame.render_widget(block, area);

        match self.stage {
            STAGE_WHY => self.why_panel(frame, inner, theme, language),
            STAGE_PUZZLE => self.puzzle_panel(frame, inner, theme, language),
            STAGE_MINE => {
                // A taller terminal gets the hash rate over time; 80x24 does not have the room.
                // It is labelled with the hash rate, so it comes with the rate's own row.
                let curve = self.rates.len() > 1 && self.reached_run() && self.said(Msg::MineRate);
                if inner.height >= CURVE_NEEDS_ROWS && curve {
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

    fn go_name(&self, language: Language) -> Option<&'static str> {
        let runnable = !self.knobs().is_empty() && (self.at_end() || self.knobs_moved());
        runnable.then(|| Msg::KeyRunIt.text(language))
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

/// Which side of half a share of the hash power is on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Side {
    Below,
    Half,
    Above,
}

/// Which side of half `share` is on. A knob stepped in hundredths lands a hair off 0.5, so
/// "exactly half" is within a millionth.
fn side(share: f64) -> Side {
    if (share - 0.5).abs() < 1e-6 {
        Side::Half
    } else if share < 0.5 {
        Side::Below
    } else {
        Side::Above
    }
}

/// Attacker and honest threads for a share of the hash power, from at least `budget` threads.
///
/// Rounding the share onto the machine's own threads was a lie on exactly the machines this
/// quest is written for. Four threads cannot hold 51%: it rounded to two and two, and the "51%
/// attack" was a tie. Two threads made every share 50%. Three made 30% into 33%.
///
/// So the split takes as many threads as it needs — the fewest, from the budget up — to come
/// within a point of the share and stay on the same side of half. The operating system shares the
/// cores out between them, and each side's hash rate lands on its share of the threads, which is
/// what the engine's own tests measure.
fn attack_split(share: f64, budget: usize) -> (usize, usize) {
    let share = if share.is_finite() { share.clamp(0.0, 1.0) } else { 0.5 };
    let split = |total: usize| {
        let attacker = ((total as f64 * share).round() as usize).clamp(1, total - 1);
        (attacker, total - attacker)
    };
    let smallest = budget.max(2);
    let mut best = split(smallest);
    let mut best_miss = f64::INFINITY;
    for total in smallest..=MAX_ATTACK_THREADS.max(smallest) {
        let (attacker, honest) = split(total);
        let got = attacker as f64 / total as f64;
        let miss = (got - share).abs();
        if side(got) != side(share) {
            continue;
        }
        if miss <= SHARE_TOLERANCE + 1e-9 {
            return (attacker, honest);
        }
        if miss < best_miss {
            best = (attacker, honest);
            best_miss = miss;
        }
    }
    best
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
    // Label first, value after: the unit is a word, and it goes before the number it counts.
    let value = match unit {
        Some(unit) if knob.draft().is_none() => format!("{unit} {}", knob.display()),
        _ => knob.display(),
    };
    let style = if chosen { theme.heading() } else { theme.muted() };
    let room = width.saturating_sub(label_width + 2);
    // The ends of the range, on the line the reader is pointing at. An arrow key that stops
    // working with nothing on the screen to say why reads as a broken key.
    let ends = if chosen { ends_of(knob) } else { None };
    let ends = ends.filter(|ends| cells(&value) + cells(ends) + 3 <= room);
    if cells(&value) <= room {
        let mut spans = vec![
            Span::styled(format!("{marker} "), theme.state(State::Chosen)),
            Span::styled(column(label, label_width), theme.plain()),
            Span::styled(value, style),
        ];
        if let Some(ends) = ends {
            spans.push(Span::styled(format!("   {ends}"), theme.muted()));
        }
        return vec![Line::from(spans)];
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

/// Both ends of what a knob will take, written the way its value is written.
fn ends_of(knob: &Knob) -> Option<String> {
    match &knob.value {
        KnobValue::Count { min, max, .. } => Some(format!("{min}-{max}")),
        KnobValue::Share { min, max, .. } => {
            Some(format!("{}-{}", format::percent(*min), format::percent(*max)))
        }
        _ => None,
    }
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

/// A count of blocks, label first: `blocks 3`, which reads the same for one block as for ten and
/// in a language with no plurals at all. "1 block" beside "3 blocks" asked every language to agree
/// a plural, which Korean does not do.
fn blocks(count: u64, language: Language) -> String {
    format!("{} {count}", Msg::UnitBlocks.text(language))
}

/// A practice difficulty, label first for the same reason: `zero bits 25`, `0비트 수 25`. The
/// Korean was "25 개의 0비트", a number agreeing a counter with its noun, which is the order the
/// phrase table cannot give one language without giving it to all of them.
fn in_zero_bits(bits: impl std::fmt::Display, language: Language) -> String {
    format!("{} {bits}", Msg::UnitZeroBits.text(language))
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

    /// Shows this stage's conversation up to `message`, without running anything.
    fn reveal_to(session: &mut Session, message: Msg) {
        session.revealed = session
            .script()
            .iter()
            .position(|step| matches!(step, Say(said) | Ask(said) if *said == message))
            .expect("the stage says it");
    }

    /// Shows the whole of this stage's conversation, without running anything, so the panel
    /// draws everything it ever will.
    fn reveal_all(session: &mut Session) {
        session.revealed = session.script().len() - 1;
    }

    /// A mining run as the record keeps it.
    fn mined(stage: usize, blocks: u64, fastest: f64, bits: u32, miners: u64) -> MinedRun {
        let split = SplitRow { index: 0, threads: 2, requested: 0.6, effective: 2.0 / 3.0 };
        MinedRun { id: u64::MAX, stage, bits, miners, blocks, fastest, split: Some(split) }
    }

    /// An attack as the record keeps it.
    fn attempt(share: f64, won: bool, reverted: u64) -> Attempt {
        Attempt { stage: STAGE_ATTACK, share, won, reverted }
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
        draw_at(session, MIN_WIDTH, MIN_HEIGHT)
    }

    /// The same panel at a size of the test's choosing.
    fn draw_at(session: &Session, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
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
        panel_rows_at(session, total, MIN_HEIGHT, language)
    }

    /// The same, on a terminal `height` rows tall.
    fn panel_rows_at(
        session: &Session,
        total: u16,
        height: u16,
        language: Language,
    ) -> Vec<String> {
        let mut terminal = Terminal::new(TestBackend::new(total, height)).expect("backend");
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
                    // The panel as the stage opens and as it stands once everything has been
                    // said: its words arrive with the conversation, so both have to fit.
                    for whole_stage in [false, true] {
                        let mut session = session_with_a_record();
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
                        if whole_stage {
                            reveal_all(&mut session);
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
        reveal_all(&mut running);
        let mining = said(&panel_rows(&running, MIN_WIDTH, Language::ENGLISH));
        running.close();
        assert!(
            mining.contains("worked out from Bitcoin's rules, not measured"),
            "the sentence saying the number was not measured is cut:\n{mining}"
        );
        assert!(
            mining.contains("x that network"),
            "the reader cannot see what this machine is a multiple of:\n{mining}"
        );

        let mut broken = session();
        broken.go_to(STAGE_ATTACK);
        broken.attack_snapshot = Some(blank_attack());
        reveal_all(&mut broken);
        let attack = said(&panel_rows(&broken, MIN_WIDTH, Language::ENGLISH));
        broken.close();
        assert!(
            attack.contains("Attacker really holds 30.0%"),
            "the share the threads really held is not on the screen:\n{attack}"
        );

        let mut puzzled = session();
        puzzled.go_to(STAGE_PUZZLE);
        reveal_all(&mut puzzled);
        let puzzle = said(&panel_rows(&puzzled, MIN_WIDTH, Language::ENGLISH));
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

    /// The opening stage talks about strangers agreeing on a list, and its panel used to be the
    /// puzzle — a nonce, SHA-256, a target — before a word of it had been said. It draws the list.
    #[test]
    fn the_opening_stage_draws_the_list_it_talks_about_and_not_the_puzzle() {
        let mut session = session();
        let opening = draw(&session);
        assert!(opening.contains("One list"), "no panel title:\n{opening}");
        assert!(opening.contains("computer 3"), "the copies of the list are missing:\n{opening}");
        for word in ["nonce", "SHA-256", "target", "header", "hash", "block"] {
            assert!(!opening.contains(word), "{word} drawn before it is explained:\n{opening}");
        }
        // The copy that lies arrives with the sentence saying any of them can, and the rule with
        // the sentence that gives it.
        assert!(!opening.contains("Ann paid Dee"), "a liar before anyone can lie:\n{opening}");
        walk(&mut session);
        let told = draw(&session);
        assert!(told.contains("x computer 3"), "the lying copy is not marked:\n{told}");
        assert!(told.contains("whoever did the most work"), "the rule is missing:\n{told}");
        assert!(told.contains("electricity"), "what the work is never arrives:\n{told}");
    }

    /// Presses Enter until the stage runs out of steps it can take without waiting.
    fn walk(session: &mut Session) {
        for _ in 0..session.script().len() {
            if session.at_end() {
                break;
            }
            session.on(Action::Go);
        }
    }

    /// The whole conversation of a stage as one string, for tests about what it says.
    fn conversation(session: &Session) -> String {
        session
            .transcript(Language::ENGLISH)
            .iter()
            .map(|beat| beat.text.as_str())
            .collect::<Vec<_>>()
            .join("  |  ")
    }

    #[test]
    fn enter_at_the_end_of_a_stage_with_knobs_runs_it_again_rather_than_walking_away() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        // Straight to the end of the conversation: what is being tested is what Enter does once
        // the stage has nothing left to say, not how long the real runs take to get there.
        session.revealed = TUNE_STAGE.len() - 1;
        assert!(session.at_end(), "the stage should have said everything by now");
        let last_run =
            TUNE_STAGE.iter().rposition(|step| matches!(step, Run(_))).expect("a run to repeat");
        assert_eq!(session.on(Action::Go), Reaction::Handled, "Enter walked away instead");
        assert_eq!(session.revealed, last_run + 1, "Enter did not rewind to the run");
        session.close();
    }

    #[test]
    fn the_value_the_reader_is_pointing_at_shows_both_ends_of_its_range() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        session.chosen = KNOB_DIFFICULTY;
        let text = draw_at(&session, 120, 30);
        let ends = format!("16-{MAX_ZERO_BITS}");
        assert!(text.contains(&ends), "the range is not on the screen: {ends}\n{text}");
    }

    #[test]
    fn enter_on_a_stage_with_nothing_to_run_is_left_to_the_shell() {
        let mut session = session();
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        assert_eq!(session.on(Action::Go), Reaction::Ignored);
    }

    #[test]
    fn turning_a_knob_while_the_work_runs_makes_enter_start_it_over() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        for _ in 0..TUNE_STAGE.len() {
            if session.mining.is_some() {
                break;
            }
            session.on(Action::Go);
        }
        assert!(session.mining.is_some(), "the first run never started");
        assert_eq!(session.on(Action::Go), Reaction::Ignored, "nothing had been changed");
        session.chosen = KNOB_DIFFICULTY;
        session.on(Action::Nudge(-1));
        assert_eq!(session.on(Action::Go), Reaction::Handled, "a turned knob asks for another run");
        session.close();
    }

    #[test]
    fn what_was_said_about_the_first_attack_survives_the_second_one() {
        let mut session = session();
        session.go_to(STAGE_ATTACK);
        let runs: Vec<usize> = ATTACK_STAGE
            .iter()
            .enumerate()
            .filter(|(_, step)| matches!(step, Run(_)))
            .map(|(at, _)| at)
            .collect();
        assert_eq!(runs.len(), 2, "the attack stage runs twice");
        // A win, then a loss above half. The sentence about the win used to be recomputed from
        // the losing run and printed "it still fell short" under the payment it had erased.
        session.attempts.push((runs[0], attempt(0.55, true, 3)));
        session.attempts.push((runs[1], attempt(0.55, false, 0)));
        session.revealed = ATTACK_STAGE.len() - 1;
        let first = session.tell_attack(runs[0] + 1, Language::ENGLISH);
        let second = session.tell_attack(runs[1] + 1, Language::ENGLISH);
        assert_eq!(first, Msg::AttackWon.text(Language::ENGLISH));
        assert_eq!(second, Msg::AttackRanOut.text(Language::ENGLISH));
        session.close();
    }

    #[test]
    fn the_recap_counts_the_whole_quest_and_not_the_last_run() {
        let mut session = session();
        session.done.runs.push(mined(STAGE_MINE, 33, 83_000_000.0, 24, 2));
        session.done.runs.push(mined(STAGE_TUNE, 4, 51_000_000.0, 25, 2));
        session.done.attacks.push(attempt(0.55, true, 3));
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let text = conversation(&session);
        assert!(text.contains("37"), "the recap forgot the blocks that were mined: {text}");
        assert!(text.contains("55.0%"), "the recap forgot the attack that was run: {text}");
    }

    #[test]
    fn the_recap_claims_nothing_the_reader_did_not_do() {
        let mut session = session();
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let text = conversation(&session);
        assert!(text.contains(Msg::RecapMinedNone.text(Language::ENGLISH)), "{text}");
        assert!(text.contains(Msg::RecapAttackNone.text(Language::ENGLISH)), "{text}");
    }

    #[test]
    fn an_attack_above_half_that_ran_out_of_time_is_not_called_a_win() {
        let mut session = session();
        session.done.attacks.push(attempt(0.55, false, 0));
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let text = conversation(&session);
        assert!(
            text.contains(Msg::RecapAttackLostAboveHalf.text(Language::ENGLISH)),
            "the recap told the reader the opposite of what happened: {text}"
        );
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
        walk(&mut session);
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
        // The table of miners and threads comes with the sentence saying what those are.
        assert!(!draw(&session).contains("asked"), "miners and threads before they are said");
        reveal_to(&mut session, Msg::TuneWords);
        let text = draw(&session);
        assert!(text.contains("asked") && text.contains("got"), "no split table:\n{text}");
        // Three threads cannot be split 60/40, so the two numbers have to differ on screen.
        assert!(text.contains("60.0%"), "what was asked for is missing:\n{text}");
        assert!(text.contains("66.7%"), "what was got is missing:\n{text}");
        assert!(text.contains("threads are whole things"), "the reason is missing:\n{text}");
    }

    #[test]
    fn the_attack_stage_says_the_share_the_attacker_will_really_hold_before_anything_starts() {
        let mut session = session();
        session.go_to(STAGE_ATTACK);
        // Three threads cannot hold 30%; ten can, three for the attacker and seven for everybody
        // else.
        assert_eq!(session.attack_threads(), (3, 7));
        let text = draw(&session);
        assert!(text.contains("Attacker's share"), "the knob is missing:\n{text}");
        assert!(text.contains("Attacker really holds  30.0%"), "the share is missing:\n{text}");
        // 51% asked is 13 threads of 25, which is 52%, and the screen says so.
        session.chosen = KNOB_ATTACKER;
        for c in "51".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.attack_threads(), (13, 12));
        let text = draw(&session);
        assert!(text.contains("Attacker really holds  52.0%"), "the share is missing:\n{text}");
    }

    #[test]
    fn the_recap_admits_what_has_not_been_run_instead_of_inventing_it() {
        let mut session = session();
        session.go_to(STAGE_RECAP);
        walk(&mut session);
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
    fn a_typed_number_beats_the_presets_and_one_past_the_end_says_where_it_landed() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        for c in "24".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 24);
        // Putting 24 back without a word reads as a broken key: the number lands on the end of
        // the range and the conversation says so.
        for c in "99".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), MAX_ZERO_BITS as u32);
        let said = session.transcript(Language::ENGLISH);
        assert!(
            said.iter().any(|beat| beat.text.contains(&MAX_ZERO_BITS.to_string())),
            "the reader was not told where the number landed"
        );
    }

    #[test]
    fn changing_the_miner_count_changes_how_many_shares_there_are_to_set() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        reveal_to(&mut session, Msg::TuneWords);
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
        reveal_to(&mut session, Msg::TuneWords);
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

        assert_eq!(
            session.tell(0, Topic::Difficulty, english),
            Msg::TuneBitsUnchanged.text(english)
        );
        let raised = session.bits_on_entry as u64 + 1;
        if let KnobValue::Count { current, .. } = &mut session.tune_knobs[KNOB_DIFFICULTY].value {
            *current = raised;
        }
        assert_eq!(session.tell(0, Topic::Difficulty, english), Msg::TuneBitsUp.text(english));

        assert_eq!(session.tell(0, Topic::Attack, english), Msg::AttackNotYetRun.text(english));
        for (share, won, expected) in [
            (0.30, false, Msg::AttackLost),
            (0.30, true, Msg::AttackLuckyWin),
            (0.51, true, Msg::AttackWon),
            (0.51, false, Msg::AttackRanOut),
        ] {
            session.attempts = vec![(0, attempt(share, won, 0))];
            assert_eq!(
                session.tell(1, Topic::Attack, english),
                expected.text(english),
                "share {share} ending won={won} was described wrongly"
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

        session.go_to(STAGE_ATTACK);
        // Walking on stopped the run, and its last word is what the record holds. Read before
        // that, a block landing in between made the recap look wrong when it was right.
        let mined = format::count(
            session.mining_snapshot.as_ref().expect("a run to recap").blocks_in_chain,
        );
        walk(&mut session);
        run_until(&mut session, |session| {
            session.attack_snapshot.as_ref().is_some_and(|run| run.outcome.is_some())
        });

        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let text = draw(&session);
        session.close();
        // Six of the seven rows have nothing else to say when the results are gone, so this one
        // line is the whole defect.
        assert!(!text.contains("not run yet"), "the recap forgot the reader's own run:\n{text}");
        assert!(text.contains("zero bits 16"), "the difficulty the reader set is gone:\n{text}");
        let counted = Msg::RecapBlocks.text(Language::ENGLISH);
        assert!(
            text.lines().any(|line| line.contains(counted) && line.contains(&mined)),
            "the recap counts blocks the reader did not mine:\n{text}"
        );
    }

    /// How many sentences a beat holds: a full stop, question or exclamation mark that ends the
    /// text or is followed by a space. "4.3 billion" and "MH/s" are not sentence ends.
    fn sentences(text: &str) -> usize {
        let chars: Vec<char> = text.chars().collect();
        chars
            .iter()
            .enumerate()
            .filter(|(at, c)| {
                matches!(c, '.' | '?' | '!')
                    && chars.get(at + 1).is_none_or(|next| next.is_whitespace())
            })
            .count()
            .max(1)
    }

    /// A session whose record holds a bit of everything, so every Tell has something to say.
    fn session_with_a_record() -> Session {
        let mut session = session();
        session.done.runs.push(mined(STAGE_MINE, 1_000_000, 100_000_000.0, 24, 2));
        session.done.runs.push(mined(STAGE_TUNE, 234_567, 123_456_789.0, 28, 4));
        for (share, won) in [(0.30, false), (0.51, true), (0.5, false), (0.45, true), (0.99, false)]
        {
            session.done.attacks.push(attempt(share, won, 12));
        }
        session
    }

    #[test]
    fn every_beat_the_quest_can_say_is_two_sentences_at_most_and_short() {
        for language in Language::ALL {
            let mut beats: Vec<String> = Vec::new();
            for script in SCRIPTS {
                for step in script {
                    if let Say(message) | Ask(message) = step {
                        beats.push(message.text(*language).to_string());
                    }
                }
            }
            // What the Tells say, read off a record that has something for every one of them.
            let record = session_with_a_record();
            for topic in [Topic::Mined, Topic::Tuned, Topic::AttacksRan, Topic::AttacksMeant] {
                beats.push(record.tell(0, topic, *language));
            }
            for message in [
                Msg::TuneBitsUnchanged,
                Msg::TuneBitsUp,
                Msg::TuneBitsDown,
                Msg::AttackLost,
                Msg::AttackLuckyWin,
                Msg::AttackEvenWon,
                Msg::AttackEvenLost,
                Msg::AttackWon,
                Msg::AttackRanOut,
                Msg::AttackNotYetRun,
                Msg::RecapMinedNone,
                Msg::RecapTunedNo,
                Msg::RecapAttackNone,
                Msg::RecapAttackAllLost,
                Msg::RecapAttackWonAboveHalf,
                Msg::RecapAttackWonBelowHalf,
                Msg::RecapAttackLostAboveHalf,
                Msg::RecapAttackEven,
            ] {
                beats.push(message.text(*language).to_string());
            }
            for beat in beats {
                assert!(!beat.is_empty(), "a blank beat in {language}");
                assert!(
                    beat.chars().count() <= 160,
                    "{language}: {} characters is a paragraph: {beat:?}",
                    beat.chars().count()
                );
                assert!(
                    sentences(&beat) <= 2,
                    "{language}: {} sentences is two beats or more: {beat:?}",
                    sentences(&beat)
                );
            }
        }
    }

    #[test]
    fn the_attack_split_stays_on_the_side_of_half_it_was_asked_for() {
        // Four threads rounded 51% to two and two: the "51% attack" was a tie. Two threads made
        // every share a tie.
        assert_eq!(attack_split(0.51, 4), (13, 12));
        let (attacker, honest) = attack_split(0.51, 2);
        assert!(attacker > honest, "51% on two threads is {attacker} / {honest}");
        assert_eq!(attack_split(0.30, 3), (3, 7));
        for budget in 1..=16 {
            for hundredths in 1..=99 {
                let share = f64::from(hundredths) / 100.0;
                let (attacker, honest) = attack_split(share, budget);
                assert!(attacker >= 1 && honest >= 1, "a side with no threads at {share}");
                assert!(attacker + honest >= budget.max(2), "fewer threads than the budget");
                assert!(attacker + honest <= MAX_ATTACK_THREADS);
                let got = attacker as f64 / (attacker + honest) as f64;
                assert_eq!(side(got), side(share), "{share} on {budget} threads became {got}");
                assert!(
                    (got - share).abs() <= SHARE_TOLERANCE + 1e-9,
                    "{share} on {budget} threads became {got}"
                );
            }
        }
    }

    #[test]
    fn the_attack_started_line_gives_the_share_the_threads_really_hold() {
        let mut session = session();
        session.go_to(STAGE_ATTACK);
        session.chosen = KNOB_ATTACKER;
        for c in "51".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        let at = ATTACK_STAGE.iter().position(|step| matches!(step, Run(_))).expect("a run");
        session.revealed = at - 1;
        session.advance();
        let said = conversation(&session);
        let snapshot = session.attack_snapshot.clone().expect("the attack started");
        session.close();
        let share = format::percent(snapshot.attacker_share);
        assert!(snapshot.attacker_share > 0.5, "51% was not above half: {share}");
        assert!(said.contains(&share), "the line does not give the share the run used: {said}");
    }

    #[test]
    fn a_tie_is_not_described_as_either_side_of_half() {
        let mut session = session();
        session.attempts = vec![(0, attempt(0.5, true, 2))];
        assert_eq!(
            session.tell_attack(1, Language::ENGLISH),
            Msg::AttackEvenWon.text(Language::ENGLISH)
        );
        session.attempts = vec![(0, attempt(0.5, false, 0))];
        assert_eq!(
            session.tell_attack(1, Language::ENGLISH),
            Msg::AttackEvenLost.text(Language::ENGLISH)
        );
        session.done.attacks = vec![attempt(0.5, false, 0)];
        assert_eq!(
            session.tell_attacks_meant(Language::ENGLISH),
            Msg::RecapAttackEven.text(Language::ENGLISH)
        );
    }

    #[test]
    fn the_recap_says_the_numbers_were_changed_when_the_runs_were_run_with_other_numbers() {
        // The check used to be against the numbers on entering the recap, which are the numbers
        // the recap was entered with: it said "you left them where they started" every time.
        let mut session = session();
        session.done.runs.push(mined(STAGE_MINE, 2, 1_000.0, 24, 2));
        session.done.runs.push(mined(STAGE_TUNE, 3, 1_000.0, 26, 3));
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let text = conversation(&session);
        assert!(text.contains(Msg::RecapTunedYes.text(Language::ENGLISH)), "{text}");
        assert!(text.contains("24 → 26") && text.contains("2 → 3"), "{text}");

        session.done.runs[1] = mined(STAGE_TUNE, 3, 1_000.0, 24, 2);
        assert!(conversation(&session).contains(Msg::RecapTunedNo.text(Language::ENGLISH)));
    }

    #[test]
    fn a_recap_with_nothing_to_tell_draws_no_blank_beat() {
        let mut session = session();
        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let beats = session.transcript(Language::ENGLISH);
        assert!(beats.iter().all(|beat| !beat.text.trim().is_empty()), "{beats:?}");
    }

    #[test]
    fn the_sentence_after_a_run_is_about_that_run_even_after_the_knob_moves_back() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        let entry = session.bits_on_entry;
        let run = TUNE_STAGE.iter().position(|step| matches!(step, Run(_))).expect("a run");
        let tell =
            TUNE_STAGE.iter().position(|step| matches!(step, Tell(_))).expect("a sentence after");
        // The run went at one bit more; the knob has since been put back where it was.
        session.run_bits.push((run, entry + 1));
        assert_eq!(session.zero_bits(), entry);
        assert_eq!(
            session.tell(tell, Topic::Difficulty, Language::ENGLISH),
            Msg::TuneBitsUp.text(Language::ENGLISH)
        );
    }

    #[test]
    fn a_typed_number_is_answered_even_after_the_stage_has_said_two_dozen_blocks() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        for height in 0..EVENT_CAP as u64 + 5 {
            session.say(Happening::Block {
                height,
                gap: Duration::from_secs(1),
                miner: 0,
                on_the_chain: true,
            });
        }
        let blocks = session.log.iter().filter(|l| matches!(l.what, Happening::Block { .. }));
        assert_eq!(blocks.count(), EVENT_CAP, "the flood was not capped");
        for c in "99".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert!(
            session.log.iter().any(|l| matches!(l.what, Happening::PulledIn { .. })),
            "where the number landed went unsaid"
        );
    }

    #[test]
    fn a_knob_turned_before_this_visits_first_run_does_not_offer_to_run_it_again() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        // A run on an earlier visit leaves the values it started with behind.
        session.running_with = Some(Settled::of(session.knobs()));
        session.go_to(STAGE_MINE);
        session.go_to(STAGE_TUNE);
        session.chosen = KNOB_DIFFICULTY;
        session.on(Action::Nudge(-1));
        assert_eq!(
            session.go_name(Language::ENGLISH),
            None,
            "the key bar offered a rerun where Enter only carries the conversation on"
        );
    }

    #[test]
    fn the_recap_recalls_the_newest_attacks_and_counts_the_rest() {
        let session = session_with_a_record();
        let told = session.tell_attacks_ran(Language::ENGLISH);
        assert!(told.contains("earlier attacks 3"), "{told}");
        assert!(told.contains("99.0%") && told.contains("45.0%"), "{told}");
        assert!(!told.contains("30.0%"), "an old attack was named one by one: {told}");
    }

    #[test]
    fn a_stopped_run_puts_every_block_it_found_into_the_record() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        session.chosen = KNOB_DIFFICULTY;
        for c in "16".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        session.start_run();
        run_until(&mut session, |session| session.mined_blocks() >= 3);
        session.stop_runs();
        let last = session.mining_snapshot.as_ref().expect("the run's last word").blocks_in_chain;
        assert!(last >= 3);
        assert_eq!(session.mined_blocks(), last, "the record and the run disagree");
        session.close();
    }

    #[test]
    fn a_block_height_is_drawn_whole_however_many_digits_it_has() {
        let mut session = session();
        session.go_to(STAGE_MINE);
        session.start_run();
        session.stop_runs();
        let mut snapshot = session.mining_snapshot.clone().expect("a snapshot");
        snapshot.recent_blocks = vec![BlockSummary {
            height: 1_234_567,
            hash: Hash256::ZERO,
            miner: nmtk_pow::MinerId(0),
            nonce: 0,
            extra_nonce: 0,
            found_after: Duration::from_secs(1),
            since_previous: Duration::from_secs(1),
            leading_zero_bits: 30,
            in_best_chain: true,
        }];
        session.mining_snapshot = Some(snapshot);
        reveal_all(&mut session);
        let text = draw_at(&session, MIN_WIDTH, 40);
        assert!(text.contains("1,234,567"), "the height was cut:\n{text}");
    }

    /// The ending is the one line of the attack panel that matters most, and at 80x24 in English
    /// it fell off the bottom: a 26-cell label pushed every value under its label, and the rows
    /// ran out before the ending was reached.
    #[test]
    fn how_the_attack_ended_is_on_the_smallest_screen_in_every_language() {
        for outcome in [AttackOutcome::Succeeded, AttackOutcome::GaveUp] {
            for language in Language::ALL {
                let mut session = session();
                session.go_to(STAGE_ATTACK);
                let mut ended = blank_attack();
                ended.outcome = Some(outcome);
                ended.phase = AttackPhase::Finished;
                ended.victim_released = true;
                ended.attack_duration = Some(Duration::from_secs(95));
                session.attack_snapshot = Some(ended);
                reveal_all(&mut session);
                let rows = panel_rows(&session, MIN_WIDTH, *language);
                let text = said(&rows);
                let ending = phrases::outcome(outcome).text(*language);
                assert!(text.contains(ending), "{language}: the ending is not on screen:\n{text}");
                // Spaces taken out: the capture gives back the cell after a wide glyph as one.
                let held = Msg::LabelShareHeld.text(*language).replace(' ', "");
                assert!(
                    text.replace(' ', "").contains(&held),
                    "{language}: the share the attacker held is not on screen:\n{text}"
                );
            }
        }
    }

    /// "no" and "yes" start their sentences in the same column, in every language. The Korean
    /// pair was two cells apart, because the padding was written for English words.
    #[test]
    fn both_answers_in_the_puzzle_start_their_sentences_in_one_column() {
        for language in Language::ALL {
            let (no, _) = branch(Msg::PuzzleNo.text(*language));
            let (yes, _) = branch(Msg::PuzzleYes.text(*language));
            assert_eq!(cells(no), cells(yes), "{language}: {no:?} and {yes:?}");
        }
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

    /// The lowest practice difficulty the knob allows, so a real run finds blocks in milliseconds.
    fn easiest_difficulty(session: &mut Session) {
        session.go_to(STAGE_TUNE);
        session.chosen = KNOB_DIFFICULTY;
        for c in "16".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert_eq!(session.zero_bits(), 16);
    }

    /// `r` pressed while a run is going. Stopping a run writes its last word back, and `r` used
    /// to forget first and stop second: the panel went on showing the run it had been pressed to
    /// throw away, under a status line saying nothing had started, and the recap counted it.
    #[test]
    fn r_pressed_while_mining_takes_the_run_off_the_panel_and_out_of_the_record() {
        let mut session = session();
        easiest_difficulty(&mut session);
        session.go_to(STAGE_MINE);
        walk(&mut session);
        run_until(&mut session, |session| session.mined_blocks() >= 1);
        assert!(session.mining.is_some(), "the run has to be going when r is pressed");

        session.on(Action::Reset);
        assert!(session.mining.is_none(), "r left the threads running");
        assert!(session.mining_snapshot.is_none(), "the panel still carries the run");
        assert_eq!(session.mined_blocks(), 0, "the record still counts the run");
        assert!(session.done.runs.is_empty(), "{:?}", session.done.runs);
        // With the whole conversation shown, so nothing is held back by it: no run is left.
        reveal_all(&mut session);
        let text = draw_at(&session, MIN_WIDTH, 40);
        for row in [Msg::LabelBlocksFound, Msg::LabelChainHeight, Msg::HeadingRecentBlocks] {
            let row = row.text(Language::ENGLISH);
            assert!(!text.contains(row), "{row} is still on the panel after r:\n{text}");
        }
        assert!(text.contains(Msg::StatusIdle.text(Language::ENGLISH)), "{text}");

        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let recap = conversation(&session);
        assert!(recap.contains(Msg::RecapMinedNone.text(Language::ENGLISH)), "{recap}");
        session.close();
    }

    /// `r` forgets the numbers the stage showing made and keeps every other stage's, the rule the
    /// ledger quest keeps; Tab keeps all of them. The record used to forget nothing at all, so
    /// the blocks, the runs and the attacks of a stage the reader had reset were still recapped.
    #[test]
    fn r_forgets_only_this_stages_record_and_tab_keeps_every_stages() {
        let mut session = session();
        easiest_difficulty(&mut session);
        session.go_to(STAGE_MINE);
        walk(&mut session);
        run_until(&mut session, |session| session.mined_blocks() >= 1);
        // Tab stops the run and keeps what it made.
        session.go_to(STAGE_TUNE);
        let from_mining = session.mined_blocks();
        assert!(from_mining >= 1);
        for _ in 0..TUNE_STAGE.len() {
            if session.mining.is_some() {
                break;
            }
            session.on(Action::Go);
        }
        run_until(&mut session, |session| session.mined_blocks() > from_mining);
        session.go_to(STAGE_ATTACK);
        let both = session.mined_blocks();
        // An attack, as the attack stage files it when one ends.
        session.done.attacks.push(attempt(0.51, true, 3));

        session.go_to(STAGE_RECAP);
        assert_eq!(session.mined_blocks(), both, "Tab lost blocks on the way to the recap");
        assert_eq!(session.done.runs.len(), 2);
        assert_eq!(session.done.attacks.len(), 1);
        // Nothing was made on the recap, so `r` there forgets nothing.
        session.on(Action::Reset);
        assert_eq!(session.mined_blocks(), both);
        assert_eq!(session.done.attacks.len(), 1);

        session.go_to(STAGE_TUNE);
        session.on(Action::Reset);
        assert_eq!(
            session.mined_blocks(),
            from_mining,
            "r on the tuning stage forgot the wrong run"
        );
        let stages: Vec<usize> = session.done.runs.iter().map(|run| run.stage).collect();
        assert_eq!(stages, vec![STAGE_MINE]);
        assert_eq!(session.done.attacks.len(), 1, "r on the tuning stage forgot the attack");
        assert!(session.mining_snapshot.is_none(), "the tuning run is still on its panel");

        session.go_to(STAGE_ATTACK);
        session.on(Action::Reset);
        assert!(session.done.attacks.is_empty(), "r on the attack stage kept its attack");
        assert_eq!(session.mined_blocks(), from_mining, "r on the attack stage took the mining");

        session.go_to(STAGE_RECAP);
        walk(&mut session);
        let recap = conversation(&session);
        session.close();
        let blocks = format!("{} {}.", Msg::RecapMinedBlocks.text(Language::ENGLISH), from_mining);
        assert!(recap.contains(&blocks), "the recap does not count what is left: {recap}");
        assert!(recap.contains(Msg::RecapTunedNo.text(Language::ENGLISH)), "{recap}");
        assert!(recap.contains(Msg::RecapAttackNone.text(Language::ENGLISH)), "{recap}");
    }

    /// The cap on blocks said counts per run. Counted per stage, the tuning stage's first run at
    /// an easy difficulty said its two dozen blocks within a second, and the second run — the one
    /// that shows a third miner finding blocks — announced none of its own.
    #[test]
    fn the_second_run_on_a_stage_announces_its_blocks_after_the_first_filled_the_cap() {
        let mut session = session();
        easiest_difficulty(&mut session);
        let runs: Vec<usize> = TUNE_STAGE
            .iter()
            .enumerate()
            .filter(|(_, step)| matches!(step, Run(_)))
            .map(|(at, _)| at)
            .collect();
        assert_eq!(runs.len(), 2, "the tuning stage runs twice");
        session.revealed = runs[0] - 1;
        session.advance();
        assert!(session.mining.is_some(), "the first run never started");
        for height in 0..EVENT_CAP as u64 + 5 {
            session.say(Happening::Block {
                height,
                gap: Duration::from_millis(40),
                miner: 0,
                on_the_chain: true,
            });
        }
        let blocks = |session: &Session, from: usize| {
            let said = |logged: &&Logged| matches!(logged.what, Happening::Block { .. });
            session.log.iter().filter(|logged| logged.step >= from).filter(said).count()
        };
        assert_eq!(blocks(&session, 0), EVENT_CAP, "the first run's flood was not capped");

        session.revealed = runs[1] - 1;
        session.advance();
        run_until(&mut session, |session| blocks(session, runs[1]) >= 1);
        session.close();
    }

    /// For 51% on a budget of two threads, the split takes 13 against 12, and those used to run
    /// flat out on every core the machine had. They take turns within the budget now; the share
    /// they hold is the one the screen and the conversation give.
    #[test]
    fn the_attack_takes_the_threads_its_share_needs_and_only_the_machine_it_was_given() {
        let mut session = session();
        session.go_to(STAGE_TUNE);
        reveal_to(&mut session, Msg::TuneWords);
        session.chosen = KNOB_THREADS;
        session.on(Action::Type('2'));
        session.on(Action::Commit);
        assert_eq!(session.thread_budget(), 2);

        session.go_to(STAGE_ATTACK);
        session.chosen = KNOB_ATTACKER;
        for c in "51".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        let at = ATTACK_STAGE.iter().position(|step| matches!(step, Run(_))).expect("a run");
        session.revealed = at - 1;
        session.advance();
        let snapshot = session.attack_snapshot.clone().expect("the attack started");
        let said = conversation(&session);
        let text = draw(&session);
        session.close();
        assert_eq!((snapshot.attacker.threads, snapshot.honest.threads), (13, 12));
        assert_eq!(snapshot.threads_at_once, 2, "more threads hashed at once than the budget");
        assert!((snapshot.attacker_share - 0.52).abs() < 1e-9);
        assert!(said.contains("attacker's share 52.0%"), "{said}");
        assert!(text.contains("Attacker really holds  52.0%"), "{text}");
    }

    /// Label first, value after, in both languages: `zero bits 25`, `0비트 수 25`. The Korean read
    /// "25 개의 0비트", a number agreeing a counter with its noun, spaced as neither order is.
    #[test]
    fn the_difficulty_reads_label_first_in_both_languages_at_eighty_by_twenty_four() {
        for language in Language::ALL {
            let unit = Msg::UnitZeroBits.text(*language);
            let mut tuning = session();
            tuning.go_to(STAGE_TUNE);
            let knob = format!("{unit} {}", tuning.zero_bits());
            let rows = panel_rows(&tuning, MIN_WIDTH, *language);
            let label = Msg::KnobDifficulty.text(*language);
            let row = rows.iter().find(|row| row.contains(label)).expect("the difficulty knob");
            println!("{language}: {row}");
            assert!(row.contains(&knob), "{language}: {row:?} does not read {knob:?}");
            assert!(!row.contains("개의"), "{language}: {row:?}");

            let beat = tuning
                .beat_for(&Happening::MiningStarted { miners: 2, threads: 3, bits: 25 }, *language)
                .text;
            assert!(beat.ends_with(&format!("{unit} 25")), "{language}: {beat:?}");

            let mut recap = session();
            recap.done.runs.push(mined(STAGE_TUNE, 3, 1_000_000.0, 25, 2));
            recap.go_to(STAGE_RECAP);
            let text = said(&panel_rows(&recap, MIN_WIDTH, *language));
            assert!(text.contains(&format!("{unit} 25")), "{language}: {text}");
        }
    }

    /// A word a reader has to be given before a panel may use it. English is found word by word
    /// in any of the forms listed; Korean by its letters, after taking out the longer words that
    /// hold them — 비트 is not in 비트코인.
    struct Term {
        en: &'static [&'static str],
        ko: &'static str,
        ko_inside: &'static [&'static str],
    }

    const TERMS: &[Term] = &[
        Term { en: &["hash", "hashes", "hashed", "hashing"], ko: "해시", ko_inside: &[] },
        Term { en: &["bit", "bits"], ko: "비트", ko_inside: &["비트코인"] },
        Term { en: &["byte", "bytes"], ko: "바이트", ko_inside: &[] },
        Term { en: &["nonce", "nonces"], ko: "논스", ko_inside: &[] },
        Term { en: &["node", "nodes"], ko: "노드", ko_inside: &[] },
        Term { en: &["header", "headers"], ko: "헤더", ko_inside: &[] },
        Term { en: &["target", "targets"], ko: "목표값", ko_inside: &[] },
        Term { en: &["sha-256"], ko: "SHA-256", ko_inside: &[] },
        Term { en: &["block", "blocks"], ko: "블록", ko_inside: &[] },
        Term { en: &["chain", "chains"], ko: "체인", ko_inside: &[] },
        Term { en: &["thread", "threads"], ko: "스레드", ko_inside: &[] },
        Term { en: &["miner", "miners"], ko: "채굴자", ko_inside: &[] },
        Term { en: &["confirmation", "confirmations"], ko: "확인", ko_inside: &[] },
    ];

    fn uses(text: &str, term: &Term, language: Language) -> bool {
        if language == Language::KOREAN {
            let text = term
                .ko_inside
                .iter()
                .fold(text.to_string(), |text, longer| text.replace(longer, ""));
            return text.contains(term.ko);
        }
        text.split(|c: char| !(c.is_alphanumeric() || c == '-'))
            .any(|word| term.en.iter().any(|form| word.eq_ignore_ascii_case(form)))
    }

    /// Everything this stage's conversation has said so far: the quest's own sentences, and not
    /// the events a run reports, which are not explanations.
    fn explained(session: &Session, language: Language) -> String {
        session
            .transcript(language)
            .into_iter()
            .filter(|beat| matches!(beat.voice, Voice::Say | Voice::Ask))
            .map(|beat| beat.text)
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The opening stage drew the puzzle — a header with a nonce in it, SHA-256, a target —
    /// beside a conversation about strangers agreeing on a list that had said none of those
    /// words. Every stage, at every step, in both languages, with and without runs to draw: a
    /// word on the panel is one the conversation has already said.
    #[test]
    fn no_word_reaches_the_panel_before_it_is_explained() {
        // What a run leaves to draw, made once: a mining run with a block that lost a race, and
        // an attack in the middle of its race and at its end.
        let mining = {
            let mut session = session();
            session.go_to(STAGE_MINE);
            session.start_run();
            session.stop_runs();
            let mut snapshot = session.mining_snapshot.take().expect("a run to draw");
            snapshot.stale_blocks = 1;
            snapshot.recent_blocks = vec![BlockSummary {
                height: 7,
                hash: Hash256::ZERO,
                miner: nmtk_pow::MinerId(1),
                nonce: 0,
                extra_nonce: 0,
                found_after: Duration::from_secs(9),
                since_previous: Duration::from_secs(2),
                leading_zero_bits: 26,
                in_best_chain: false,
            }];
            snapshot
        };
        let racing = AttackSnapshot {
            phase: AttackPhase::Racing,
            lead: -1,
            max_deficit: 2,
            victim_released: true,
            ..blank_attack()
        };
        let ended = AttackSnapshot {
            phase: AttackPhase::Finished,
            outcome: Some(AttackOutcome::Succeeded),
            blocks_reverted: 3,
            attack_duration: Some(Duration::from_secs(40)),
            ..racing.clone()
        };
        for language in Language::ALL {
            for (stage, script) in SCRIPTS.iter().enumerate() {
                for revealed in 0..script.len() {
                    for runs in 0..3 {
                        let mut session =
                            if runs == 0 { session() } else { session_with_a_record() };
                        session.go_to(stage);
                        session.revealed = revealed;
                        if runs > 0 {
                            session.mining_snapshot = Some(mining.clone());
                            session.mining_stage = Some(stage);
                            let attack = if runs == 1 { &racing } else { &ended };
                            session.attack_snapshot = Some(attack.clone());
                        }
                        let panel = said(&panel_rows_at(&session, ROOMY, 60, *language));
                        let talk = explained(&session, *language);
                        for term in TERMS {
                            assert!(
                                !uses(&panel, term, *language) || uses(&talk, term, *language),
                                "stage {stage}, step {revealed}, {language}: the panel says {:?} \
                                 before the conversation has.\npanel: {panel}\nsaid: {talk}",
                                term.en[0]
                            );
                        }
                    }
                }
            }
        }
    }

    /// "Now give the attacker 51% and press Enter again": a reader who did exactly that was taken
    /// back to the 30% attack, which vanished from the conversation, and watched 51% run in its
    /// place — the value on screen was read as a change to the run before, not as the next run's.
    #[test]
    fn a_value_set_for_the_next_run_starts_that_run_rather_than_redoing_the_last() {
        let mut session = session();
        session.go_to(STAGE_ATTACK);
        let ask = ATTACK_STAGE
            .iter()
            .position(|step| matches!(step, Ask(Msg::AttackAskHigh)))
            .expect("the second ask");
        let first = ATTACK_STAGE.iter().position(|step| matches!(step, Run(_))).expect("a run");
        assert!(first < ask);
        session.revealed = ask;
        session.running_with = Some(Settled::of(session.knobs()));
        session.chosen = KNOB_ATTACKER;
        for c in "51".chars() {
            session.on(Action::Type(c));
        }
        session.on(Action::Commit);
        assert!(session.knobs_moved(), "typing 51 moved nothing");
        session.on(Action::Go);
        let revealed = session.revealed;
        let share = session.attacker_share();
        session.close();
        assert!(revealed > ask, "Enter went back to the first attack: step {revealed}");
        assert!((share - 0.51).abs() < 1e-9, "the attack ran at {share}");
    }
}
