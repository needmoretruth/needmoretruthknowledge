//! A mining run: several miners, real threads, one chain, and a snapshot a screen can read.
//!
//! Everything in here is measured. The hash rates are hashes that were computed divided by the
//! seconds they took, the blocks were found by a thread that hashed until a header came in under
//! the target, and when two miners find a block at the same height the chain keeps both and lets
//! the work decide. A miner with a larger share of the threads finds more blocks for the same
//! reason it would on a real network: it is doing more of the hashing.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use nmtk_core::MachineProfile;
use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

use crate::block::{Block, BlockTemplate, Tx};
use crate::chain::{Acceptance, Chain, Reorg};
use crate::engine::{Engine, Found, MinerSlot, PreparedJob, sample_rates, spawn_workers};
use crate::hash::Hash256;
use crate::miners::{ConfigError, MinerId, MinerSpec, NOBODY, Share, split_threads};
use crate::target::Target;

/// How often the coordinator measures hash rates. Shorter windows are noisier, longer ones lag.
const RATE_WINDOW: Duration = Duration::from_millis(250);

/// How often a fresh snapshot is put where a screen can read it. A screen redrawing ten times a
/// second never sees a stale number, and the coordinator never becomes the bottleneck.
const PUBLISH_EVERY: Duration = Duration::from_millis(50);

/// Everything a run needs to start.
#[derive(Debug, Clone)]
pub struct MiningConfig {
    /// The difficulty, packed the way a header carries it. Use
    /// [`crate::target::DIFFICULTY_ONE_BITS`] for the real thing and
    /// [`crate::target::practice_bits`] for something that finishes on a laptop.
    pub bits: u32,
    /// Who is mining and what each one asked for.
    pub miners: Vec<MinerSpec>,
    /// How many threads there are to divide up.
    pub threads: usize,
    /// Fixes the parts of a run that can be fixed: where each miner starts searching. The race
    /// between miners still depends on how the operating system shares out the cores, so two runs
    /// with the same seed find the same blocks in a different order.
    pub seed: u64,
    /// The timestamp to put in headers, or the clock if this is `None`.
    pub start_time: Option<u32>,
    /// The block version to claim. Bitcoin's first blocks carried 1.
    pub version: i32,
    /// How many recently found blocks a snapshot carries.
    pub recent_blocks_kept: usize,
}

impl MiningConfig {
    /// A run at a given difficulty with a given thread budget.
    pub fn new(bits: u32, miners: Vec<MinerSpec>, threads: usize) -> MiningConfig {
        MiningConfig {
            bits,
            miners,
            threads,
            seed: 0,
            start_time: None,
            version: 1,
            recent_blocks_kept: 32,
        }
    }

    /// A run sized for this machine, leaving a core free so the screen keeps moving.
    ///
    /// Thread counts are whole numbers, so a machine with few cores cannot honour a share like
    /// 51% exactly. Raising the thread count above the core count fixes that — the operating
    /// system shares the cores out between the threads and each miner's measured rate lands on
    /// its share.
    pub fn for_machine(
        bits: u32,
        miners: Vec<MinerSpec>,
        profile: &MachineProfile,
    ) -> MiningConfig {
        MiningConfig::new(bits, miners, profile.default_worker_threads())
    }

    /// The same run, started from a fixed seed.
    pub fn with_seed(mut self, seed: u64) -> MiningConfig {
        self.seed = seed;
        self
    }

    /// The same run with a different thread budget.
    pub fn with_threads(mut self, threads: usize) -> MiningConfig {
        self.threads = threads;
        self
    }

    /// How the threads would be divided, without starting anything.
    pub fn thread_split(&self) -> Result<Vec<usize>, ConfigError> {
        split_threads(&self.miners, self.threads)
    }
}

/// One miner, as a screen sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct MinerSnapshot {
    /// Which miner.
    pub id: MinerId,
    /// Threads it ended up with.
    pub threads: usize,
    /// What it asked for.
    pub requested: Share,
    /// The share of the running threads it actually holds, which is what decides how many blocks
    /// it finds. It is not always the share it asked for.
    pub effective_share: f64,
    /// Hashes computed, counted one by one.
    pub hashes: u64,
    /// Hashes per second over the last quarter second, measured.
    pub hashrate: f64,
    /// Hashes per second over the whole run so far.
    pub average_hashrate: f64,
    /// Blocks handed in, including any that lost a race.
    pub blocks_found: u64,
    /// Blocks of its that are in the chain right now. A reorg can take these away.
    pub blocks_in_chain: u64,
}

/// A block, as a screen sees it.
#[derive(Debug, Clone, PartialEq)]
pub struct BlockSummary {
    /// Where it sits.
    pub height: u64,
    /// Its hash.
    pub hash: Hash256,
    /// Who found it.
    pub miner: MinerId,
    /// The nonce that solved it.
    pub nonce: u32,
    /// The extra nonce its coinbase carried.
    pub extra_nonce: u64,
    /// How far into the run it was found.
    pub found_after: Duration,
    /// The gap since the block before it.
    pub since_previous: Duration,
    /// How many zero bits the hash starts with. The target sets the floor; luck does the rest.
    pub leading_zero_bits: u32,
    /// Whether it is still in the chain. A block that was reorganised away is not.
    pub in_best_chain: bool,
}

/// A branch growing beside the chain, as a screen sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ForkSummary {
    /// The last height it shares with the chain.
    pub fork_height: u64,
    /// How many blocks it holds.
    pub length: usize,
    /// Its own tip height.
    pub tip_height: u64,
    /// How far behind the chain's tip it is.
    pub behind: u64,
}

/// A reorganisation, as a screen sees it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReorgSummary {
    /// The last height both chains agree on.
    pub fork_height: u64,
    /// How many blocks were rolled back.
    pub removed: usize,
    /// How many took their place.
    pub added: usize,
    /// The height before.
    pub old_height: u64,
    /// The height after.
    pub new_height: u64,
    /// How far into the run it happened.
    pub at: Duration,
}

impl ReorgSummary {
    fn of(reorg: &Reorg, at: Duration) -> ReorgSummary {
        ReorgSummary {
            fork_height: reorg.fork_height,
            removed: reorg.removed.len(),
            added: reorg.added,
            old_height: reorg.old_height,
            new_height: reorg.new_height,
            at,
        }
    }
}

/// The whole state of a run at one instant. Plain data: no locks, no threads, nothing borrowed.
#[derive(Debug, Clone, PartialEq)]
pub struct MiningSnapshot {
    /// Time spent mining, with any paused stretches taken out.
    pub elapsed: Duration,
    /// Whether it is paused right now.
    pub paused: bool,
    /// Whether the threads have stopped for good.
    pub finished: bool,
    /// The difficulty as the headers carry it.
    pub bits: u32,
    /// The target every block has to beat.
    pub target: Target,
    /// How many times harder that is than difficulty 1.
    pub difficulty: f64,
    /// How many hashes a block takes on average at this difficulty.
    pub expected_hashes_per_block: f64,
    /// The chain's height.
    pub height: u64,
    /// The chain's tip.
    pub tip: Hash256,
    /// The work behind the chain, in expected hashes.
    pub total_work: u128,
    /// Hashes computed by everyone.
    pub total_hashes: u64,
    /// Hashes per second across all miners, measured.
    pub total_hashrate: f64,
    /// Blocks that made it into the chain.
    pub blocks_in_chain: u64,
    /// Blocks found by anyone, including ones that lost a race.
    pub blocks_found: u64,
    /// Blocks that were valid but arrived too late to be the tip.
    pub stale_blocks: u64,
    /// Every miner.
    pub miners: Vec<MinerSnapshot>,
    /// The most recent blocks, oldest first.
    pub recent_blocks: Vec<BlockSummary>,
    /// Branches growing beside the chain.
    pub forks: Vec<ForkSummary>,
    /// How many times the chain has been replaced.
    pub reorgs: u64,
    /// The last time it happened.
    pub last_reorg: Option<ReorgSummary>,
}

/// A run in progress. Dropping this stops the threads and waits for them.
pub struct MiningHandle {
    engine: Arc<Engine>,
    shared: Arc<Mutex<MiningSnapshot>>,
    workers: Option<Vec<JoinHandle<()>>>,
    coordinator: Option<JoinHandle<()>>,
}

impl MiningHandle {
    /// The state of the run right now.
    ///
    /// This takes a copy and lets go of the lock, so a screen can read it as often as it likes
    /// without ever slowing a miner down.
    pub fn snapshot(&self) -> MiningSnapshot {
        match self.shared.lock() {
            Ok(snapshot) => snapshot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Stops hashing without losing anything. Paused time is left out of the hash rates.
    pub fn pause(&self) {
        self.engine.paused.store(true, Ordering::Relaxed);
    }

    /// Starts hashing again.
    pub fn resume(&self) {
        self.engine.paused.store(false, Ordering::Relaxed);
    }

    /// Whether the run is paused.
    pub fn is_paused(&self) -> bool {
        self.engine.paused.load(Ordering::Relaxed)
    }

    /// Brings every thread home and waits for them.
    pub fn stop(mut self) {
        self.shutdown();
    }

    fn shutdown(&mut self) {
        self.engine.stop.store(true, Ordering::Relaxed);
        if let Some(workers) = self.workers.take() {
            for worker in workers {
                let _ = worker.join();
            }
        }
        if let Some(coordinator) = self.coordinator.take() {
            let _ = coordinator.join();
        }
    }
}

impl Drop for MiningHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts a run: divides the threads, builds the chain, and sets everyone hashing.
pub fn start_mining(config: MiningConfig) -> Result<MiningHandle, ConfigError> {
    let threads = split_threads(&config.miners, config.threads)?;
    let target = Target::from_compact(config.bits)?;
    let time = config.start_time.unwrap_or_else(unix_now);
    let chain = Chain::new(config.bits, time, NOBODY)?;

    // The seed decides where each miner starts searching, so a seeded run repeats itself.
    let mut rng = SmallRng::seed_from_u64(config.seed);
    let slots: Vec<Arc<MinerSlot>> = config
        .miners
        .iter()
        .zip(&threads)
        .map(|(spec, count)| Arc::new(MinerSlot::new(spec.id, *count, rng.random())))
        .collect();
    let engine = Arc::new(Engine::new(slots));

    let mut coordinator = Coordinator {
        chain,
        engine: Arc::clone(&engine),
        config: config.clone(),
        threads: threads.clone(),
        started: Instant::now(),
        paused_total: Duration::ZERO,
        paused_since: None,
        last_block_at: Duration::ZERO,
        recent: VecDeque::new(),
        blocks_found: 0,
        stale_blocks: 0,
        reorgs: 0,
        last_reorg: None,
        shared: Arc::new(Mutex::new(empty_snapshot(&config, &threads, target))),
    };
    coordinator.retarget();
    let shared = Arc::clone(&coordinator.shared);

    let (sender, receiver) = mpsc::channel::<Found>();
    let workers = spawn_workers(&engine, &sender);
    // The coordinator learns that mining is over when the last worker's sender goes away.
    drop(sender);

    let handle = thread::Builder::new()
        .name("nmtk-pow-chain".to_string())
        .spawn(move || coordinator.run(&receiver))
        .ok();

    Ok(MiningHandle { engine, shared, workers: Some(workers), coordinator: handle })
}

/// The seconds since the Unix epoch, or zero on a machine whose clock is before 1970.
pub(crate) fn unix_now() -> u32 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|since| u32::try_from(since.as_secs()).unwrap_or(u32::MAX))
        .unwrap_or(0)
}

/// A block as the coordinator recorded it when it arrived.
struct BlockRecord {
    height: u64,
    hash: Hash256,
    miner: MinerId,
    nonce: u32,
    extra_nonce: u64,
    found_after: Duration,
    since_previous: Duration,
    leading_zero_bits: u32,
}

impl BlockRecord {
    fn of(block: &Block, found_after: Duration, since_previous: Duration) -> BlockRecord {
        let hash = block.hash();
        BlockRecord {
            height: block.height(),
            hash,
            miner: block.miner(),
            nonce: block.header.nonce,
            extra_nonce: block.coinbase.extra_nonce,
            found_after,
            since_previous,
            leading_zero_bits: hash.leading_zero_bits(),
        }
    }
}

struct Coordinator {
    chain: Chain,
    engine: Arc<Engine>,
    config: MiningConfig,
    threads: Vec<usize>,
    started: Instant,
    paused_total: Duration,
    paused_since: Option<Instant>,
    last_block_at: Duration,
    recent: VecDeque<BlockRecord>,
    blocks_found: u64,
    stale_blocks: u64,
    reorgs: u64,
    last_reorg: Option<ReorgSummary>,
    shared: Arc<Mutex<MiningSnapshot>>,
}

impl Coordinator {
    fn run(&mut self, receiver: &mpsc::Receiver<Found>) {
        let mut previous_hashes = vec![0u64; self.engine.slots.len()];
        let mut last_sample = Instant::now();
        let mut last_publish = Instant::now();
        loop {
            if self.engine.stop.load(Ordering::Relaxed) {
                break;
            }
            match receiver.recv_timeout(Duration::from_millis(20)) {
                Ok(found) => self.handle(found),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => break,
            }
            self.track_pause();
            if last_sample.elapsed() >= RATE_WINDOW {
                sample_rates(&self.engine, &mut previous_hashes, last_sample.elapsed());
                last_sample = Instant::now();
            }
            if last_publish.elapsed() >= PUBLISH_EVERY {
                self.publish(false);
                last_publish = Instant::now();
            }
        }
        self.publish(true);
    }

    fn handle(&mut self, found: Found) {
        let elapsed = self.elapsed();
        let since_previous = elapsed.saturating_sub(self.last_block_at);
        let accepted = self.chain.accept(found.block.clone());
        match accepted {
            Ok(Acceptance::Extended { .. }) => {
                self.record(&found.block, elapsed, since_previous);
                self.retarget();
            }
            Ok(Acceptance::Reorg(reorg)) => {
                self.record(&found.block, elapsed, since_previous);
                self.reorgs += 1;
                self.last_reorg = Some(ReorgSummary::of(&reorg, elapsed));
                self.retarget();
            }
            Ok(Acceptance::Fork { .. }) => {
                // Valid work that arrived a moment too late: it becomes a branch, and if the
                // chain ever swings that way it will count after all.
                self.record(&found.block, elapsed, since_previous);
                self.stale_blocks += 1;
            }
            Ok(Acceptance::Duplicate | Acceptance::Orphan) => self.stale_blocks += 1,
            Err(_) => self.stale_blocks += 1,
        }
    }

    fn record(&mut self, block: &Block, elapsed: Duration, since_previous: Duration) {
        self.blocks_found += 1;
        self.last_block_at = elapsed;
        self.recent.push_back(BlockRecord::of(block, elapsed, since_previous));
        while self.recent.len() > self.config.recent_blocks_kept.max(1) {
            self.recent.pop_front();
        }
    }

    /// Hands every miner a job on the new tip. Each one mines its own coinbase, so the block a
    /// miner finds pays that miner.
    fn retarget(&mut self) {
        let prev_hash = self.chain.tip_hash();
        let height = self.chain.height() + 1;
        let time = self.config.start_time.unwrap_or_else(unix_now);
        let target = self.chain.target();
        for slot in &self.engine.slots {
            let template = BlockTemplate {
                version: self.config.version,
                prev_hash,
                height,
                time,
                bits: self.config.bits,
                miner: slot.id,
                txs: Vec::<Tx>::new(),
            };
            slot.set_job(Some(Arc::new(PreparedJob::new(template, target))));
        }
    }

    fn track_pause(&mut self) {
        let paused = self.engine.paused.load(Ordering::Relaxed);
        match (paused, self.paused_since) {
            (true, None) => self.paused_since = Some(Instant::now()),
            (false, Some(since)) => {
                self.paused_total += since.elapsed();
                self.paused_since = None;
            }
            _ => {}
        }
    }

    /// Time spent actually mining.
    fn elapsed(&self) -> Duration {
        let paused_now = self.paused_since.map(|since| since.elapsed()).unwrap_or_default();
        self.started.elapsed().saturating_sub(self.paused_total).saturating_sub(paused_now)
    }

    fn publish(&mut self, finished: bool) {
        let elapsed = self.elapsed();
        let seconds = elapsed.as_secs_f64();
        let in_chain = self.chain.blocks_by_miner();
        let effective = crate::miners::effective_shares(&self.threads);
        let miners: Vec<MinerSnapshot> = self
            .engine
            .slots
            .iter()
            .enumerate()
            .map(|(index, slot)| {
                let hashes = slot.hashes.load(Ordering::Relaxed);
                MinerSnapshot {
                    id: slot.id,
                    threads: slot.threads,
                    requested: self
                        .config
                        .miners
                        .get(index)
                        .map(|spec| spec.share)
                        .unwrap_or(Share::Threads(slot.threads)),
                    effective_share: effective.get(index).copied().unwrap_or(0.0),
                    hashes,
                    hashrate: slot.hashrate(),
                    average_hashrate: if seconds > 0.0 { hashes as f64 / seconds } else { 0.0 },
                    blocks_found: slot.blocks.load(Ordering::Relaxed),
                    blocks_in_chain: in_chain.get(&slot.id).copied().unwrap_or(0),
                }
            })
            .collect();

        let target = self.chain.target();
        let snapshot = MiningSnapshot {
            elapsed,
            paused: self.engine.paused.load(Ordering::Relaxed),
            finished,
            bits: self.config.bits,
            target,
            difficulty: target.difficulty(),
            expected_hashes_per_block: target.expected_hashes(),
            height: self.chain.height(),
            tip: self.chain.tip_hash(),
            total_work: self.chain.total_work(),
            total_hashes: self.engine.total_hashes(),
            total_hashrate: miners.iter().map(|miner| miner.hashrate).sum(),
            blocks_in_chain: self.chain.height(),
            blocks_found: self.blocks_found,
            stale_blocks: self.stale_blocks,
            miners,
            recent_blocks: self
                .recent
                .iter()
                .map(|record| BlockSummary {
                    height: record.height,
                    hash: record.hash,
                    miner: record.miner,
                    nonce: record.nonce,
                    extra_nonce: record.extra_nonce,
                    found_after: record.found_after,
                    since_previous: record.since_previous,
                    leading_zero_bits: record.leading_zero_bits,
                    in_best_chain: self.chain.contains(&record.hash),
                })
                .collect(),
            forks: self
                .chain
                .branches()
                .iter()
                .map(|branch| ForkSummary {
                    fork_height: branch.fork_height,
                    length: branch.blocks.len(),
                    tip_height: branch.tip_height(),
                    behind: self.chain.height().saturating_sub(branch.tip_height()),
                })
                .collect(),
            reorgs: self.reorgs,
            last_reorg: self.last_reorg.clone(),
        };
        match self.shared.lock() {
            Ok(mut slot) => *slot = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
    }
}

fn empty_snapshot(config: &MiningConfig, threads: &[usize], target: Target) -> MiningSnapshot {
    let effective = crate::miners::effective_shares(threads);
    MiningSnapshot {
        elapsed: Duration::ZERO,
        paused: false,
        finished: false,
        bits: config.bits,
        target,
        difficulty: target.difficulty(),
        expected_hashes_per_block: target.expected_hashes(),
        height: 0,
        tip: Hash256::ZERO,
        total_work: 0,
        total_hashes: 0,
        total_hashrate: 0.0,
        blocks_in_chain: 0,
        blocks_found: 0,
        stale_blocks: 0,
        miners: config
            .miners
            .iter()
            .zip(threads)
            .enumerate()
            .map(|(index, (spec, count))| MinerSnapshot {
                id: spec.id,
                threads: *count,
                requested: spec.share,
                effective_share: effective.get(index).copied().unwrap_or(0.0),
                hashes: 0,
                hashrate: 0.0,
                average_hashrate: 0.0,
                blocks_found: 0,
                blocks_in_chain: 0,
            })
            .collect(),
        recent_blocks: Vec::new(),
        forks: Vec::new(),
        reorgs: 0,
        last_reorg: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::practice_bits;

    /// Waits for a run to reach a height, and gives up rather than hanging a test forever.
    fn wait_for_height(handle: &MiningHandle, height: u64, limit: Duration) -> MiningSnapshot {
        let deadline = Instant::now() + limit;
        loop {
            let snapshot = handle.snapshot();
            if snapshot.height >= height || Instant::now() > deadline {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    /// Waits for the hashing to get somewhere, rather than for the clock.
    ///
    /// A fixed sleep asserts that a busy machine is a fast one. These tests drive real threads,
    /// and on a machine already running something else a tenth of a second buys no hashes at all —
    /// which made them fail for a reason that had nothing to do with the code.
    /// Waits until every miner has done some hashing of its own.
    ///
    /// Threads do not all start at the same instant, and the first miner's are started first. On
    /// a machine that is busy with something else, the second miner's can still be starting when
    /// a window opens — and then the window measures the start order rather than the split.
    fn wait_until_all_are_hashing(
        handle: &MiningHandle,
        each: u64,
        limit: Duration,
    ) -> MiningSnapshot {
        let deadline = Instant::now() + limit;
        loop {
            let snapshot = handle.snapshot();
            let all = snapshot.miners.iter().all(|miner| miner.hashes >= each);
            if all || Instant::now() > deadline {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    fn wait_for_hashes(handle: &MiningHandle, wanted: u64, limit: Duration) -> MiningSnapshot {
        let deadline = Instant::now() + limit;
        loop {
            let snapshot = handle.snapshot();
            if snapshot.total_hashes >= wanted || Instant::now() > deadline {
                return snapshot;
            }
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn real_threads_really_find_blocks_and_the_chain_grows() {
        let config = MiningConfig::new(
            practice_bits(14).expect("expressible"),
            vec![MinerSpec::percent(0, 100.0)],
            2,
        );
        let handle = start_mining(config).expect("valid config");
        let snapshot = wait_for_height(&handle, 3, Duration::from_secs(20));
        handle.stop();
        assert!(snapshot.height >= 3, "only reached height {}", snapshot.height);
        assert!(snapshot.total_hashes > 0);
        assert!(snapshot.miners[0].hashes > 0);
        assert!(snapshot.recent_blocks.iter().all(|block| block.leading_zero_bits >= 14));
        assert!(snapshot.recent_blocks.iter().any(|block| block.in_best_chain));
    }

    #[test]
    fn the_miner_with_more_threads_does_more_of_the_hashing() {
        let config = MiningConfig::new(
            practice_bits(16).expect("expressible"),
            vec![MinerSpec::threads(0, 3), MinerSpec::threads(1, 1)],
            4,
        );
        let handle = start_mining(config).expect("valid config");
        let snapshot = wait_for_height(&handle, 6, Duration::from_secs(30));
        handle.stop();
        let big = &snapshot.miners[0];
        let small = &snapshot.miners[1];
        assert!((big.effective_share - 0.75).abs() < 1e-9);
        assert!((small.effective_share - 0.25).abs() < 1e-9);
        assert!(
            big.hashes > small.hashes,
            "three threads did {} hashes, one did {}",
            big.hashes,
            small.hashes
        );
        assert!(big.average_hashrate > 0.0);
    }

    #[test]
    fn asking_for_more_threads_than_cores_delivers_a_share_the_cores_could_not() {
        // Four threads cannot be split 51 to 49. A hundred can, and because every one of them is
        // doing nothing but hashing, the operating system hands out its cores in proportion — so
        // the measured hash rates land on the shares that were asked for rather than on the
        // nearest whole thread. This is the setting a screen should reach for when a reader types
        // a percentage the machine's core count cannot express.
        let config = MiningConfig::new(
            practice_bits(24).expect("expressible"),
            vec![MinerSpec::percent(0, 51.0), MinerSpec::percent(1, 49.0)],
            100,
        );
        let handle = start_mining(config).expect("valid config");
        // The window opens once both miners are really hashing, not after a fixed wait: on a busy
        // machine a hundred threads take longer than any sleep worth writing down to come up.
        let start = wait_until_all_are_hashing(&handle, 100_000, Duration::from_secs(30));
        // Measured over a stretch of time rather than a count of hashes. What is being tested is
        // how the scheduler shares a dozen cores between a hundred threads, and that only evens
        // out over seconds: a window of a few tens of milliseconds measures which threads
        // happened to be running in it, which is why this used to fail whenever the machine was
        // busy with something else.
        thread::sleep(Duration::from_secs(3));
        let end = handle.snapshot();
        handle.stop();
        assert_eq!(end.miners[0].threads, 51);
        assert_eq!(end.miners[1].threads, 49);
        let bigger = end.miners[0].hashes.saturating_sub(start.miners[0].hashes) as f64;
        let smaller = end.miners[1].hashes.saturating_sub(start.miners[1].hashes) as f64;
        assert!(bigger + smaller > 1_000_000.0, "only {} hashes in three seconds", bigger + smaller);
        let share = bigger / (bigger + smaller);
        assert!(
            (share - 0.51).abs() < 0.08,
            "the miner that asked for 51% did {share} of the hashing"
        );
    }

    #[test]
    fn pausing_stops_the_hashing_and_resuming_starts_it_again() {
        let config = MiningConfig::new(
            practice_bits(20).expect("expressible"),
            vec![MinerSpec::percent(0, 100.0)],
            2,
        );
        let handle = start_mining(config).expect("valid config");
        let working = wait_for_hashes(&handle, 1, Duration::from_secs(20));
        assert!(working.total_hashes > 0, "the threads never hashed anything to pause");
        handle.pause();
        // Long enough for anything still in flight to land before the two readings are compared.
        thread::sleep(Duration::from_millis(200));
        let paused = handle.snapshot();
        thread::sleep(Duration::from_millis(250));
        let still_paused = handle.snapshot();
        assert_eq!(paused.total_hashes, still_paused.total_hashes, "hashing went on while paused");
        assert!(still_paused.paused);

        handle.resume();
        let resumed =
            wait_for_hashes(&handle, still_paused.total_hashes + 1, Duration::from_secs(20));
        handle.stop();
        assert!(resumed.total_hashes > still_paused.total_hashes, "resuming did not restart work");
        assert!(!resumed.paused);
    }

    #[test]
    fn a_run_that_cannot_be_divided_is_refused_before_a_thread_is_started() {
        let config = MiningConfig::new(
            practice_bits(20).expect("expressible"),
            vec![MinerSpec::percent(0, 50.0), MinerSpec::percent(1, 50.0)],
            1,
        );
        assert!(matches!(
            start_mining(config),
            Err(ConfigError::ThreadBudgetTooSmall { needed: 2, budget: 1 })
        ));
    }

    #[test]
    fn a_snapshot_taken_before_anything_happens_is_still_readable() {
        let config = MiningConfig::new(
            practice_bits(24).expect("expressible"),
            vec![MinerSpec::percent(0, 60.0), MinerSpec::percent(1, 40.0)],
            5,
        );
        let handle = start_mining(config).expect("valid config");
        let snapshot = handle.snapshot();
        handle.stop();
        assert_eq!(snapshot.miners.len(), 2);
        assert_eq!(snapshot.miners[0].threads, 3);
        assert_eq!(snapshot.miners[1].threads, 2);
        assert!(snapshot.difficulty > 0.0);
        assert!(snapshot.expected_hashes_per_block > 0.0);
    }
}
