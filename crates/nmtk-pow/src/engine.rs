//! The part that actually hashes.
//!
//! A worker thread does one thing: take the job its miner was given, pick a slice of the search
//! space nobody else has, and hash headers until it finds one under the target or is told the job
//! has changed. Nothing is simulated and no result is invented — a block appears when a thread
//! really finds a hash under the target, and the hash counters are hashes that were computed.
//!
//! The search space is the same one Bitcoin miners use. The nonce field is four bytes, which a
//! modern core exhausts in a second or two, so when it runs out the miner changes the extra nonce
//! in its coinbase. That changes the merkle root, which changes the header, which gives it four
//! billion fresh nonces. Every thread takes its own extra nonce, so no two threads ever hash the
//! same header twice.

use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::block::{Block, BlockTemplate};
use crate::hash::{Hash256, HeaderMidstate, merkle_root};
use crate::miners::MinerId;
use crate::target::Target;

/// How many hashes a worker does before it looks up to see whether the world changed.
///
/// Every hash a thread does after somebody else has found the block is wasted on a height that
/// is already taken. At 4,096 this was most of the work: at a 14-bit practice target a block is
/// about 16,000 hashes, so nine threads each finishing their 4,096 found two or three extra blocks
/// for the same height every time, and a side with nine threads mined little faster than a side
/// with one. A 10% attacker won a race it should almost never win. Looking up is one atomic read,
/// which costs nothing beside 256 double hashes.
///
/// It is also what pausing promises: a thread finishes the chunk it is in the middle of and then
/// hashes no more, so after a pause the count grows by at most one chunk per thread.
pub(crate) const CHUNK: u32 = 256;

/// How long a worker waits for a fresh job after handing in a block, before going back to the old
/// one. A block it finds in that window would be a second block for a height already taken.
const NEW_JOB_WAIT: Duration = Duration::from_millis(250);

/// The shortest turn a thread is given. Waking up costs a little of every turn, and a turn this
/// long keeps that little small beside the hashing.
const SHORTEST_TURN: Duration = Duration::from_millis(2);

/// The shortest round of turns. Every thread has had its turn once a round is over, so the round
/// is also how long a share of the hashing takes to even out.
const SHORTEST_ROUND: Duration = Duration::from_millis(10);

/// How long a thread waiting for its turn sleeps before looking at the stop and pause flags again.
const WAKE_TO_LOOK: Duration = Duration::from_millis(5);

/// Threads taking turns, so a run with more of them than it was given uses no more of the machine
/// than it was given.
///
/// A share like 51% needs more threads than a small machine has — two threads cannot be split 51
/// to 49, but twenty-five can be split 13 to 12 — and running every one of them flat out took
/// every core on the machine to express a share, however few the reader had allowed. Taking turns
/// keeps both. A round is cut into one slot per thread; each thread's turn opens at its own slot
/// and lasts `budget` slots, and the thread sleeps for the rest of the round. At every moment
/// exactly `budget` turns are open, and every thread hashes for the same share of each round, so
/// a miner's share of the hashing is its share of the threads.
#[derive(Debug, Clone, Copy)]
pub struct Turns {
    epoch: Instant,
    round: Duration,
    slot: Duration,
    turn: Duration,
    budget: usize,
}

impl Turns {
    /// Turns for `threads` threads, `budget` of them hashing at any one moment. Nothing to take
    /// turns over when the budget covers every thread.
    pub fn new(threads: usize, budget: usize) -> Option<Turns> {
        if threads == 0 || budget == 0 || budget >= threads {
            return None;
        }
        let threads_u32 = u32::try_from(threads).ok()?;
        let budget_u32 = u32::try_from(budget).ok()?;
        let round = (SHORTEST_TURN * threads_u32 / budget_u32).max(SHORTEST_ROUND);
        let slot = round / threads_u32;
        Some(Turns { epoch: Instant::now(), round, slot, turn: slot * budget_u32, budget })
    }

    /// How many threads hash at any one moment.
    pub fn at_once(&self) -> usize {
        self.budget
    }

    /// Where thread `index` stands at `now`: `Ok` with the moment its turn ends while it is open,
    /// `Err` with the moment the next one opens while it is not.
    pub fn at(&self, index: usize, now: Instant) -> Result<Instant, Instant> {
        let round = self.round.as_nanos().max(1);
        let opens = (self.slot.as_nanos() * index as u128) % round;
        // How far into this thread's own round `now` is: zero the instant its turn opens. Counted
        // from a round before the epoch, so the turns that wrap round the end of the first round
        // are open from the start rather than waiting a round for it.
        let into = (now.saturating_duration_since(self.epoch).as_nanos() + round - opens) % round;
        let nanos = |value: u128| Duration::from_nanos(u64::try_from(value).unwrap_or(u64::MAX));
        let turn = self.turn.as_nanos();
        if into < turn { Ok(now + nanos(turn - into)) } else { Err(now + nanos(round - into)) }
    }
}

/// A job with everything that does not change while a miner searches worked out in advance.
#[derive(Debug, Clone)]
pub struct PreparedJob {
    template: BlockTemplate,
    tx_leaves: Vec<Hash256>,
    target: Target,
}

impl PreparedJob {
    /// Works out the transaction merkle leaves once, so the search loop only recomputes the part
    /// that depends on the extra nonce.
    pub fn new(template: BlockTemplate, target: Target) -> PreparedJob {
        let tx_leaves = template.tx_leaves();
        PreparedJob { template, tx_leaves, target }
    }

    /// The template being worked on.
    pub fn template(&self) -> &BlockTemplate {
        &self.template
    }

    fn merkle_for(&self, extra_nonce: u64) -> Hash256 {
        let mut leaves = Vec::with_capacity(self.tx_leaves.len() + 1);
        leaves.push(self.template.coinbase(extra_nonce).txid().hash());
        leaves.extend_from_slice(&self.tx_leaves);
        merkle_root(&leaves)
    }

    /// The 80 header bytes for one extra nonce, with the nonce field left at zero.
    pub fn header_bytes(&self, extra_nonce: u64) -> [u8; 80] {
        let mut bytes = [0u8; 80];
        bytes[0..4].copy_from_slice(&self.template.version.to_le_bytes());
        bytes[4..36].copy_from_slice(self.template.prev_hash.as_bytes());
        bytes[36..68].copy_from_slice(self.merkle_for(extra_nonce).as_bytes());
        bytes[68..72].copy_from_slice(&self.template.time.to_le_bytes());
        bytes[72..76].copy_from_slice(&self.template.bits.to_le_bytes());
        bytes
    }

    /// The finished block for a solved header.
    pub fn block(&self, extra_nonce: u64, nonce: u32) -> Block {
        self.template.block(extra_nonce, nonce)
    }
}

/// Mines one block on this thread, and gives up after `max_hashes`.
///
/// This is the whole of proof of work in one loop: change a number, hash the header, look at the
/// result, try again. It returns the block and the number of hashes it took, which is the figure
/// to compare against the expected count for the difficulty.
pub fn mine_serial(
    template: &BlockTemplate,
    target: Target,
    extra_nonce_start: u64,
    max_hashes: u64,
) -> Option<(Block, u64)> {
    let job = PreparedJob::new(template.clone(), target);
    let mut hashes = 0u64;
    let mut extra_nonce = extra_nonce_start;
    while hashes < max_hashes {
        let header = job.header_bytes(extra_nonce);
        let midstate = HeaderMidstate::new(&header);
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&header[64..]);
        for nonce in 0..=u32::MAX {
            tail[12..16].copy_from_slice(&nonce.to_le_bytes());
            hashes += 1;
            if target.is_met_by(&midstate.finish(&tail)) {
                return Some((job.block(extra_nonce, nonce), hashes));
            }
            if hashes >= max_hashes {
                return None;
            }
        }
        extra_nonce = extra_nonce.wrapping_add(1);
    }
    None
}

/// One miner's threads, its job, and its counters.
#[derive(Debug)]
pub struct MinerSlot {
    /// Which miner this is.
    pub id: MinerId,
    /// How many threads it was given.
    pub threads: usize,
    /// Hashes computed, ever. This is counted, not estimated.
    pub hashes: AtomicU64,
    /// Blocks handed in, including ones the chain later rejected or reorganised away.
    pub blocks: AtomicU64,
    /// The most recently measured rate, in thousandths of a hash per second.
    pub rate_milli: AtomicU64,
    job: Mutex<Option<Arc<PreparedJob>>>,
    version: AtomicU64,
    /// One more than the job version this miner has already found a block for, or zero.
    ///
    /// A miner is one party, however many threads it runs. Once one of its threads has solved a
    /// job, the rest have nothing left to find there: another block at the same height from the
    /// same miner only loses a race against itself. They used to carry on until the chain
    /// handed out the next job, and on a busy machine that wait was long enough for nine threads
    /// to hand in two blocks for every one that counted.
    solved: AtomicU64,
    extra_nonce: AtomicU64,
}

impl MinerSlot {
    /// A slot for a miner that has not been given a job yet.
    pub fn new(id: MinerId, threads: usize, extra_nonce_start: u64) -> MinerSlot {
        MinerSlot {
            id,
            threads,
            hashes: AtomicU64::new(0),
            blocks: AtomicU64::new(0),
            rate_milli: AtomicU64::new(0),
            job: Mutex::new(None),
            version: AtomicU64::new(0),
            solved: AtomicU64::new(0),
            extra_nonce: AtomicU64::new(extra_nonce_start),
        }
    }

    /// Hands the miner a new job. Its threads pick it up within a few thousand hashes.
    pub fn set_job(&self, job: Option<Arc<PreparedJob>>) {
        match self.job.lock() {
            Ok(mut slot) => *slot = job,
            // A worker panicking mid-search must not stop the rest of the run.
            Err(poisoned) => *poisoned.into_inner() = job,
        }
        self.version.fetch_add(1, Ordering::Release);
    }

    /// The measured hash rate, in hashes per second.
    pub fn hashrate(&self) -> f64 {
        self.rate_milli.load(Ordering::Relaxed) as f64 / 1_000.0
    }

    pub(crate) fn job(&self) -> Option<Arc<PreparedJob>> {
        match self.job.lock() {
            Ok(slot) => slot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }
}

/// The flags every worker watches, and the miners they work for.
#[derive(Debug)]
pub struct Engine {
    /// One slot per miner.
    pub slots: Vec<Arc<MinerSlot>>,
    /// Set once, to bring every thread home.
    pub stop: AtomicBool,
    /// Set and cleared freely while the run is going.
    pub paused: AtomicBool,
    /// Whether the threads take turns, and how.
    turns: Option<Turns>,
}

impl Engine {
    /// An engine with a slot per miner, no jobs handed out yet. Every thread hashes flat out.
    pub fn new(slots: Vec<Arc<MinerSlot>>) -> Engine {
        Engine { slots, stop: AtomicBool::new(false), paused: AtomicBool::new(false), turns: None }
    }

    /// The same, with no more than `budget` threads hashing at any one moment. With more threads
    /// than that they take turns, so the run uses the machine it was given and the miners' shares
    /// of the hashing are still their shares of the threads. See [`Turns`].
    pub fn within(slots: Vec<Arc<MinerSlot>>, budget: usize) -> Engine {
        let threads = slots.iter().map(|slot| slot.threads).sum();
        Engine { turns: Turns::new(threads, budget), ..Engine::new(slots) }
    }

    /// How many threads hash at any one moment: all of them, unless they are taking turns.
    pub fn threads_at_once(&self) -> usize {
        match self.turns {
            Some(turns) => turns.at_once(),
            None => self.slots.iter().map(|slot| slot.threads).sum(),
        }
    }

    /// Total hashes computed by everyone.
    pub fn total_hashes(&self) -> u64 {
        self.slots.iter().map(|slot| slot.hashes.load(Ordering::Relaxed)).sum()
    }
}

/// A block a worker found, on its way to the chain.
#[derive(Debug)]
pub struct Found {
    /// Who found it.
    pub miner: MinerId,
    /// The block itself.
    pub block: Block,
}

/// Starts every miner's threads. They run until the engine's stop flag is set.
///
/// One thread of each miner at a time, round and round, rather than all of one miner's first: the
/// miners start hashing together, and when the threads take turns each miner's turns are spread
/// across the whole round instead of bunched at one end of it.
pub fn spawn_workers(engine: &Arc<Engine>, sender: &Sender<Found>) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();
    let most = engine.slots.iter().map(|slot| slot.threads).max().unwrap_or(0);
    let mut turn = 0;
    for round in 0..most {
        for slot in engine.slots.iter().filter(|slot| round < slot.threads) {
            let engine = Arc::clone(engine);
            let slot = Arc::clone(slot);
            let sender = sender.clone();
            let index = turn;
            turn += 1;
            if let Ok(handle) = thread::Builder::new()
                .name(format!("nmtk-pow-{}", slot.id.0))
                .spawn(move || run_worker(&engine, &slot, &sender, index))
            {
                handles.push(handle);
            }
        }
    }
    handles
}

/// Measures each miner's hash rate from how many hashes it did since the last look.
///
/// The rate is a measurement over a window, not a guess from the thread count: a miner whose
/// threads are starved by the rest of the machine reports the lower number it really achieved.
pub fn sample_rates(engine: &Engine, previous: &mut [u64], window: Duration) {
    let seconds = window.as_secs_f64();
    if seconds <= 0.0 {
        return;
    }
    for (slot, last) in engine.slots.iter().zip(previous.iter_mut()) {
        let now = slot.hashes.load(Ordering::Relaxed);
        let done = now.saturating_sub(*last);
        *last = now;
        let rate = (done as f64 / seconds) * 1_000.0;
        let rate = if rate.is_finite() && rate >= 0.0 { rate as u64 } else { 0 };
        slot.rate_milli.store(rate, Ordering::Relaxed);
    }
}

fn run_worker(engine: &Arc<Engine>, slot: &Arc<MinerSlot>, sender: &Sender<Found>, turn: usize) {
    while !engine.stop.load(Ordering::Relaxed) {
        if engine.paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        // Not this thread's turn: sleep until it is, looking up now and then so a stop or a
        // pause is answered in milliseconds rather than at the end of the round.
        let turn_ends = match engine.turns.map(|turns| turns.at(turn, Instant::now())) {
            Some(Err(opens)) => {
                sleep_until(engine, opens);
                continue;
            }
            Some(Ok(ends)) => Some(ends),
            None => None,
        };
        let version = slot.version.load(Ordering::Acquire);
        let Some(job) = slot.job() else {
            thread::sleep(Duration::from_millis(2));
            continue;
        };
        // Another of this miner's threads already found this job's block. Wait for the next job
        // the way the finder does, and only go back to this one if none comes.
        if slot.solved.load(Ordering::Acquire) == version.wrapping_add(1) {
            wait_for_new_job(engine, slot, version);
            if slot.version.load(Ordering::Acquire) != version {
                continue;
            }
            // Nothing came, so the block was not the end of this job after all — the chain
            // turned it away. The job is open again for every thread. Back to the top rather
            // than straight into hashing: the reader may have paused, or stopped, while this
            // thread was waiting.
            let _ = slot.solved.compare_exchange(
                version.wrapping_add(1),
                0,
                Ordering::AcqRel,
                Ordering::Acquire,
            );
            continue;
        }
        // Nobody else will ever take this extra nonce, so nobody repeats this work.
        let extra_nonce = slot.extra_nonce.fetch_add(1, Ordering::Relaxed);
        let header = job.header_bytes(extra_nonce);
        let midstate = HeaderMidstate::new(&header);
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&header[64..]);

        let mut nonce: u32 = 0;
        loop {
            let began = turn_ends.map(|_| Instant::now());
            let mut winner: Option<u32> = None;
            let mut hashed: u64 = 0;
            for step in 0..CHUNK {
                let candidate = nonce.wrapping_add(step);
                tail[12..16].copy_from_slice(&candidate.to_le_bytes());
                hashed += 1;
                if job.target.is_met_by(&midstate.finish(&tail)) {
                    winner = Some(candidate);
                    break;
                }
            }
            slot.hashes.fetch_add(hashed, Ordering::Relaxed);

            if let Some(nonce) = winner {
                slot.solved.store(version.wrapping_add(1), Ordering::Release);
                slot.blocks.fetch_add(1, Ordering::Relaxed);
                let block = job.block(extra_nonce, nonce);
                if sender.send(Found { miner: slot.id, block }).is_err() {
                    return;
                }
                wait_for_new_job(engine, slot, version);
                break;
            }

            let (next, wrapped) = nonce.overflowing_add(CHUNK);
            nonce = next;
            if wrapped
                || engine.stop.load(Ordering::Relaxed)
                || engine.paused.load(Ordering::Relaxed)
                || slot.version.load(Ordering::Acquire) != version
                || slot.solved.load(Ordering::Acquire) == version.wrapping_add(1)
            {
                break;
            }
            // The turn is over, or would be before another chunk is done. Hashing past it is
            // hashing on a core the run was not given.
            if let (Some(ends), Some(began)) = (turn_ends, began) {
                let now = Instant::now();
                if now + now.saturating_duration_since(began) >= ends {
                    break;
                }
            }
        }
    }
}

/// Sleeps until `until`, waking every few milliseconds to see whether the run was stopped or
/// paused in the meantime.
fn sleep_until(engine: &Engine, until: Instant) {
    loop {
        if engine.stop.load(Ordering::Relaxed) || engine.paused.load(Ordering::Relaxed) {
            return;
        }
        let now = Instant::now();
        if now >= until {
            return;
        }
        thread::sleep((until - now).min(WAKE_TO_LOOK));
    }
}

/// Waits a moment for the chain to hand out the next job, so a miner that just found a block does
/// not spend the next instant mining a height that is already taken.
fn wait_for_new_job(engine: &Arc<Engine>, slot: &Arc<MinerSlot>, version: u64) {
    let deadline = std::time::Instant::now() + NEW_JOB_WAIT;
    while std::time::Instant::now() < deadline {
        if engine.stop.load(Ordering::Relaxed) || slot.version.load(Ordering::Acquire) != version {
            return;
        }
        thread::sleep(Duration::from_millis(1));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{CoinId, Tx};
    use crate::target::practice_bits;

    fn template() -> BlockTemplate {
        BlockTemplate {
            version: 1,
            prev_hash: Hash256::from_bytes([0x11; 32]),
            height: 1,
            time: 1_231_006_505,
            bits: practice_bits(12).expect("expressible"),
            miner: MinerId(0),
            txs: vec![Tx::spend(CoinId(1), 5)],
        }
    }

    #[test]
    fn a_block_found_by_the_search_is_really_under_the_target() {
        let template = template();
        let target = Target::from_compact(template.bits).expect("canonical");
        let (block, hashes) =
            mine_serial(&template, target, 0, 100_000_000).expect("a 12-bit target is quick");
        assert!(target.is_met_by(&block.hash()));
        assert_eq!(block.header.merkle_root, block.computed_merkle_root());
        assert!(hashes > 0);
        // A 12-bit target takes about 4096 hashes. Finding one in far more than a hundred times
        // that would mean the search or the comparison is wrong.
        assert!(hashes < 4_096 * 100, "took {hashes} hashes for a 12-bit target");
    }

    #[test]
    fn the_search_loop_and_the_plain_header_hash_agree() {
        let template = template();
        let target = Target::from_compact(template.bits).expect("canonical");
        let (block, _) = mine_serial(&template, target, 7, 100_000_000).expect("found");
        // The block the search hands back hashes to the same value when rebuilt from scratch.
        assert_eq!(block.hash(), block.header.hash());
        assert_eq!(block.coinbase.extra_nonce, 7);
    }

    #[test]
    fn giving_up_early_returns_nothing_rather_than_a_bad_block() {
        let mut template = template();
        // A target this hard will not fall in ten thousand hashes.
        template.bits = practice_bits(40).expect("expressible");
        let target = Target::from_compact(template.bits).expect("canonical");
        assert!(mine_serial(&template, target, 0, 10_000).is_none());
    }

    #[test]
    fn exactly_the_budget_of_turns_is_open_at_every_moment_and_each_thread_gets_its_share() {
        // Nothing to take turns over when the budget covers every thread.
        assert!(Turns::new(4, 4).is_none());
        assert!(Turns::new(4, 8).is_none());
        assert!(Turns::new(0, 2).is_none());
        assert!(Turns::new(3, 0).is_none());

        for (threads, budget) in [(25, 2), (10, 7), (100, 1), (17, 16), (2, 1)] {
            let turns = Turns::new(threads, budget).expect("more threads than budget");
            assert_eq!(turns.at_once(), budget);
            let slot = turns.round / threads as u32;
            let mut open_time = vec![Duration::ZERO; threads];
            // Three rounds, looked at a third of the way into every slot, well away from where
            // a turn opens or closes.
            for step in 0..threads * 3 {
                let now = turns.epoch + slot * step as u32 + slot / 3;
                let open: Vec<usize> =
                    (0..threads).filter(|&index| turns.at(index, now).is_ok()).collect();
                assert_eq!(
                    open.len(),
                    budget,
                    "{threads} threads within {budget}: {open:?} open at step {step}"
                );
                for index in open {
                    open_time[index] += slot;
                }
            }
            // Every thread hashed for the same share of the time, which is what keeps a share of
            // the threads a share of the hashing.
            let first = open_time[0];
            assert!(open_time.iter().all(|time| *time == first), "{open_time:?}");
            assert_eq!(first, slot * (3 * budget) as u32);
        }
    }

    #[test]
    fn a_turn_says_when_it_ends_and_a_wait_says_when_it_opens() {
        let turns = Turns::new(25, 2).expect("more threads than budget");
        // A round of twenty-five 1 ms slots; thread 3's turn is the fourth and fifth of them.
        assert_eq!(turns.round, Duration::from_millis(25));
        let at = |millis: f64| turns.epoch + Duration::from_secs_f64(millis / 1_000.0);
        assert_eq!(turns.at(3, at(3.5)), Ok(at(5.0)));
        assert_eq!(turns.at(3, at(5.5)), Err(at(28.0)));
        assert_eq!(turns.at(3, at(1.0)), Err(at(3.0)));
        // The last thread's turn wraps round the end of the round, so it is open at the start.
        assert_eq!(turns.at(24, at(0.5)), Ok(at(1.0)));
    }

    /// A job nobody will solve in the life of the test, for measuring hashing and nothing else.
    fn unsolvable_job(miner: MinerId) -> Arc<PreparedJob> {
        let bits = practice_bits(64).expect("expressible");
        let target = Target::from_compact(bits).expect("canonical");
        Arc::new(PreparedJob::new(BlockTemplate { bits, miner, ..template() }, target))
    }

    /// Threads for each `(miner id, threads)` pair, hashing an unsolvable job, within `budget`.
    ///
    /// The ids are the test's own, so its threads carry names no other test's threads have and
    /// their CPU time can be read apart from everything else running in the process.
    fn hashing(
        miners: &[(u32, usize)],
        budget: Option<usize>,
    ) -> (Arc<Engine>, Vec<JoinHandle<()>>, std::sync::mpsc::Receiver<Found>) {
        let slots: Vec<Arc<MinerSlot>> = miners
            .iter()
            .map(|(id, threads)| Arc::new(MinerSlot::new(MinerId(*id), *threads, u64::from(*id))))
            .collect();
        let engine = Arc::new(match budget {
            Some(budget) => Engine::within(slots, budget),
            None => Engine::new(slots),
        });
        for slot in &engine.slots {
            slot.set_job(Some(unsolvable_job(slot.id)));
        }
        let (sender, receiver) = std::sync::mpsc::channel();
        let handles = spawn_workers(&engine, &sender);
        (engine, handles, receiver)
    }

    fn stop_and_join(engine: &Engine, handles: Vec<JoinHandle<()>>) {
        engine.stop.store(true, Ordering::Relaxed);
        for handle in handles {
            let _ = handle.join();
        }
    }

    fn wait_for_every_miner_to_hash(engine: &Engine) {
        let deadline = Instant::now() + Duration::from_secs(20);
        while engine.slots.iter().any(|slot| slot.hashes.load(Ordering::Relaxed) == 0)
            && Instant::now() < deadline
        {
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// CPU time used so far by this process's threads whose names start with `prefix`, read from
    /// `/proc`. `None` where there is no `/proc` to read, and the caller skips what it measured.
    fn cpu_of_threads(prefix: &str) -> Option<Duration> {
        let mut ticks = 0u64;
        for task in std::fs::read_dir("/proc/self/task").ok()?.flatten() {
            let name = std::fs::read_to_string(task.path().join("comm")).unwrap_or_default();
            if !name.trim_end().starts_with(prefix) {
                continue;
            }
            let Ok(stat) = std::fs::read_to_string(task.path().join("stat")) else { continue };
            // The name is in brackets and may hold spaces, so fields are counted after it:
            // utime and stime are fields 14 and 15 of the line, 12th and 13th after the name.
            let Some((_, after)) = stat.rsplit_once(')') else { continue };
            let fields: Vec<&str> = after.split_whitespace().collect();
            let field = |at: usize| fields.get(at).and_then(|f| f.parse::<u64>().ok());
            ticks += field(11).unwrap_or(0) + field(12).unwrap_or(0);
        }
        // Linux reports these in hundredths of a second on every architecture it runs on.
        Some(Duration::from_millis(ticks * 10))
    }

    /// Hashes each miner did, and CPU its threads used, over `window` once they are all going.
    fn measure(
        engine: &Engine,
        prefix: &str,
        window: Duration,
    ) -> (Vec<u64>, Option<f64>, Duration) {
        wait_for_every_miner_to_hash(engine);
        let counts = || -> Vec<u64> {
            engine.slots.iter().map(|slot| slot.hashes.load(Ordering::Relaxed)).collect()
        };
        let (before, cpu_before, started) = (counts(), cpu_of_threads(prefix), Instant::now());
        thread::sleep(window);
        let (after, cpu_after, took) = (counts(), cpu_of_threads(prefix), started.elapsed());
        let done = after.iter().zip(&before).map(|(a, b)| a.saturating_sub(*b)).collect();
        let cores = match (cpu_before, cpu_after) {
            (Some(before), Some(after)) => {
                Some(after.saturating_sub(before).as_secs_f64() / took.as_secs_f64())
            }
            _ => None,
        };
        (done, cores, took)
    }

    #[test]
    fn twenty_five_threads_within_two_keep_their_share_and_use_two_cores_at_most() {
        // 51% of the hashing, expressed the way the attack stage expresses it: 13 threads against
        // 12, with the machine's own budget of two.
        let (engine, handles, _receiver) = hashing(&[(900, 13), (901, 12)], Some(2));
        assert_eq!(engine.threads_at_once(), 2);
        let (done, cores, took) = measure(&engine, "nmtk-pow-90", Duration::from_secs(2));
        stop_and_join(&engine, handles);
        let total = (done[0] + done[1]) as f64;
        assert!(total > 0.0, "nothing was hashed in {took:?}");
        let share = done[0] as f64 / total;
        assert!(
            (share - 13.0 / 25.0).abs() < 0.08,
            "thirteen threads of twenty-five did {share} of the hashing"
        );
        // Taking turns can only ever use less than the budget, never more — a busy machine gives
        // a turn less than it asked for, not more. The margin covers the chunk a thread finishes
        // at the end of its turn and the clock's hundredths of a second.
        if let Some(cores) = cores {
            println!("{cores:.2} cores for 25 threads within 2");
            assert!(cores <= 2.0 * 1.2, "twenty-five threads within two used {cores:.2} cores");
        }
    }

    #[test]
    fn a_thread_waiting_for_its_turn_answers_a_stop_or_a_pause_in_milliseconds() {
        let engine = Engine::within(vec![Arc::new(MinerSlot::new(MinerId(0), 100, 0))], 1);
        let engine = Arc::new(engine);
        // A round of a hundred turns is 200 ms; waiting for the next one used to mean the round.
        for flag in [0, 1] {
            let waiting = Arc::clone(&engine);
            let sleeper = thread::spawn(move || {
                let started = Instant::now();
                sleep_until(&waiting, Instant::now() + Duration::from_secs(30));
                started.elapsed()
            });
            thread::sleep(Duration::from_millis(20));
            match flag {
                0 => engine.paused.store(true, Ordering::Relaxed),
                _ => engine.stop.store(true, Ordering::Relaxed),
            }
            let slept = sleeper.join().expect("the sleeper came home");
            assert!(slept < Duration::from_secs(5), "a thread slept {slept:?} through the flag");
            engine.paused.store(false, Ordering::Relaxed);
        }
    }

    #[test]
    fn threads_taking_turns_still_stop_hashing_within_a_chunk_of_a_pause() {
        let (engine, handles, _receiver) = hashing(&[(910, 50), (911, 50)], Some(1));
        wait_for_every_miner_to_hash(&engine);
        engine.paused.store(true, Ordering::Relaxed);
        let at_pause = engine.total_hashes();
        let one_chunk_each = 100 * u64::from(CHUNK);
        thread::sleep(Duration::from_millis(300));
        let paused = engine.total_hashes() - at_pause;
        engine.paused.store(false, Ordering::Relaxed);
        let deadline = Instant::now() + Duration::from_secs(20);
        while engine.total_hashes() <= at_pause + one_chunk_each && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(5));
        }
        let resumed = engine.total_hashes() > at_pause + one_chunk_each;
        let asked = Instant::now();
        stop_and_join(&engine, handles);
        let stopping = asked.elapsed();
        assert!(paused <= one_chunk_each, "{paused} hashes after the pause");
        assert!(resumed, "taking turns did not start again after the pause");
        assert!(stopping < Duration::from_secs(10), "stopping took {stopping:?}");
    }

    /// The same measurement for the whole process, run on its own so nothing else is counted:
    ///
    /// `cargo test -p nmtk-pow --lib -- --ignored --test-threads=1 --nocapture measured`
    #[test]
    #[ignore = "reads the whole process's CPU time, so it has to run alone"]
    fn measured_process_cpu_stays_within_the_budget_while_threads_take_turns() {
        fn process_cpu() -> Duration {
            let stat = std::fs::read_to_string("/proc/self/stat").expect("Linux");
            let (_, after) = stat.rsplit_once(')').expect("a stat line");
            let fields: Vec<&str> = after.split_whitespace().collect();
            // Counted by position: some fields before these can be negative.
            let ticks: u64 = fields[11].parse::<u64>().expect("utime")
                + fields[12].parse::<u64>().expect("stime");
            Duration::from_millis(ticks * 10)
        }
        fn cores_used(run: impl FnOnce() -> Box<dyn FnOnce()>) -> f64 {
            let stop = run();
            thread::sleep(Duration::from_millis(500));
            let (cpu, wall) = (process_cpu(), Instant::now());
            thread::sleep(Duration::from_secs(3));
            let cores = (process_cpu() - cpu).as_secs_f64() / wall.elapsed().as_secs_f64();
            stop();
            cores
        }
        let flat_out = cores_used(|| {
            let (engine, handles, receiver) = hashing(&[(920, 13), (921, 12)], None);
            Box::new(move || {
                stop_and_join(&engine, handles);
                drop(receiver);
            })
        });
        let within_two = cores_used(|| {
            let (engine, handles, receiver) = hashing(&[(920, 13), (921, 12)], Some(2));
            Box::new(move || {
                stop_and_join(&engine, handles);
                drop(receiver);
            })
        });
        // The attack the quest runs for 51% on a budget of two, measured end to end: both
        // sides hashing from the start, at a difficulty nobody meets in three seconds.
        let attack = cores_used(|| {
            let mut config =
                crate::attack::AttackConfig::new(practice_bits(40).expect("ok"), 13, 12)
                    .with_cpu_budget(2);
            config.warmup_blocks = 0;
            let handle = crate::attack::start_attack(config).expect("valid config");
            Box::new(move || handle.stop())
        });
        println!(
            "25 threads flat out: {flat_out:.2} cores; within 2: {within_two:.2} cores; \
             the 51% attack within 2: {attack:.2} cores"
        );
        assert!(within_two <= 2.2, "within two used {within_two:.2} cores");
        assert!(within_two >= 1.4, "within two used only {within_two:.2} cores");
        assert!(attack <= 2.3, "the attack within two used {attack:.2} cores");
    }
}
