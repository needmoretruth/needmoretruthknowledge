//! A 51% attack that really happens.
//!
//! The story is the one every explanation of Bitcoin tells and almost none of them run. Someone
//! pays a merchant. The payment goes into a block, then another block lands on top of it, and the
//! merchant — who has been told to wait for confirmations — hands over the goods. Meanwhile the
//! buyer has been mining a chain of their own in private, starting from the block before the
//! payment, and in that chain the same coin was paid to themselves instead. When their private
//! chain carries more work than the public one, they publish it. Every node switches, because
//! that is the rule every node follows, and the payment the merchant saw confirmed is simply not
//! in the chain any more. The coin was never spent on the merchant.
//!
//! Nothing here is staged. Both sides are real threads hashing real headers at the same
//! difficulty, the private chain is a real chain that is validated block by block, and the
//! switch at the end is the same reorganisation that would happen to any node. An attacker with a
//! small share of the threads loses, and the run records how far behind it fell while losing.

use std::collections::VecDeque;
use std::sync::atomic::Ordering;
use std::sync::mpsc::{self, RecvTimeoutError};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use rand::rngs::SmallRng;
use rand::{RngExt, SeedableRng};

use crate::block::{Block, BlockTemplate, CoinId, Tx, Txid};
use crate::chain::{Acceptance, Chain};
use crate::engine::{Engine, Found, MinerSlot, PreparedJob, sample_rates, spawn_workers};
use crate::hash::Hash256;
use crate::miners::{
    ConfigError, MinerId, MinerSpec, NOBODY, Share, effective_shares, split_threads,
};
use crate::mining::{BlockSummary, MinerSnapshot};
use crate::target::Target;

/// The miner everybody can see, building the chain the merchant is watching.
pub const HONEST: MinerId = MinerId(0);

/// The miner building a chain nobody can see yet.
pub const ATTACKER: MinerId = MinerId(1);

const RATE_WINDOW: Duration = Duration::from_millis(250);
const PUBLISH_EVERY: Duration = Duration::from_millis(50);

/// How the attack should be set up.
#[derive(Debug, Clone)]
pub struct AttackConfig {
    /// The difficulty both sides mine at, packed as a header carries it.
    pub bits: u32,
    /// Threads for the attacker. Its share of the total is what decides whether this works.
    pub attacker_threads: usize,
    /// Threads for everybody else.
    pub honest_threads: usize,
    /// How many confirmations the merchant waits for before handing over the goods.
    pub confirmations_required: u64,
    /// Blocks mined in the open before the payment is made, so the fork does not start at the
    /// genesis block. The attacker's threads are idle for this stretch.
    pub warmup_blocks: u64,
    /// How many blocks the public chain may add past the fork before the attacker gives up.
    /// Hash power costs money, and an attacker that is losing stops paying for it.
    pub give_up_after_public_blocks: u64,
    /// A wall-clock limit, so a run always ends.
    pub give_up_after: Option<Duration>,
    /// Fixes where each miner starts searching.
    pub seed: u64,
    /// The coin that gets spent twice.
    pub coin: CoinId,
    /// Who the merchant is, in the payment the public chain confirms.
    pub victim: u64,
    /// Who the attacker pays instead, in the chain it keeps to itself.
    pub attacker_payee: u64,
    /// The timestamp to put in headers, or the clock if this is `None`.
    pub start_time: Option<u32>,
    /// The block version to claim.
    pub version: i32,
    /// How many recently found blocks a snapshot carries.
    pub recent_blocks_kept: usize,
}

impl AttackConfig {
    /// An attack at a difficulty, with the threads split between the two sides.
    pub fn new(bits: u32, attacker_threads: usize, honest_threads: usize) -> AttackConfig {
        AttackConfig {
            bits,
            attacker_threads,
            honest_threads,
            confirmations_required: 2,
            warmup_blocks: 1,
            give_up_after_public_blocks: 24,
            give_up_after: Some(Duration::from_secs(120)),
            seed: 0,
            coin: CoinId(1),
            victim: 1,
            attacker_payee: 2,
            start_time: None,
            version: 1,
            recent_blocks_kept: 32,
        }
    }

    /// The same attack, started from a fixed seed.
    pub fn with_seed(mut self, seed: u64) -> AttackConfig {
        self.seed = seed;
        self
    }

    /// The same attack with the merchant waiting a different number of blocks.
    pub fn with_confirmations(mut self, confirmations: u64) -> AttackConfig {
        self.confirmations_required = confirmations;
        self
    }

    /// The share of the threads the attacker holds. Above a half, catching up is only a matter of
    /// time; below it, falling behind is.
    pub fn attacker_share(&self) -> f64 {
        let total = self.attacker_threads + self.honest_threads;
        if total == 0 { 0.0 } else { self.attacker_threads as f64 / total as f64 }
    }
}

/// Where the attack has got to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackPhase {
    /// Blocks are being mined in the open; the payment has not been made yet.
    Warmup,
    /// The payment is out and the attacker's private chain has started, but no block has picked
    /// the payment up yet.
    AwaitingPayment,
    /// The payment is in a block and the merchant is counting confirmations.
    Confirming,
    /// The merchant has handed over the goods. The attacker is trying to get ahead.
    Racing,
    /// Over, one way or the other.
    Finished,
}

/// How it ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttackOutcome {
    /// The private chain was published, the chain switched, and the payment is gone.
    Succeeded,
    /// The attacker never got ahead and stopped paying for hash power.
    GaveUp,
}

/// The whole state of an attack at one instant. Plain data, nothing borrowed.
#[derive(Debug, Clone, PartialEq)]
pub struct AttackSnapshot {
    /// Where it has got to.
    pub phase: AttackPhase,
    /// How it ended, once it has.
    pub outcome: Option<AttackOutcome>,
    /// Time spent mining, with paused stretches taken out.
    pub elapsed: Duration,
    /// Whether it is paused right now.
    pub paused: bool,
    /// Whether the threads have stopped for good.
    pub finished: bool,
    /// The difficulty as the headers carry it.
    pub bits: u32,
    /// How many times harder that is than difficulty 1.
    pub difficulty: f64,
    /// The attacker's threads, hashes and blocks.
    pub attacker: MinerSnapshot,
    /// Everybody else's.
    pub honest: MinerSnapshot,
    /// The share of the threads the attacker holds.
    pub attacker_share: f64,
    /// The height of the chain everyone can see.
    pub public_height: u64,
    /// Its tip.
    pub public_tip: Hash256,
    /// The height of the chain only the attacker can see, once there is one.
    pub private_height: u64,
    /// The last height the two chains agree on.
    pub fork_height: Option<u64>,
    /// How far ahead the private chain is. Negative means behind, which is where it starts.
    pub lead: i64,
    /// The deepest the attacker ever fell behind. This is the number that says how close it came
    /// to being hopeless.
    pub max_deficit: u64,
    /// The payment the merchant is watching.
    pub payment_txid: Option<Txid>,
    /// The attacker's own version of the same spend.
    pub double_spend_txid: Option<Txid>,
    /// The height the payment was confirmed at.
    pub payment_height: Option<u64>,
    /// How many confirmations the payment has right now. Zero once it has been undone.
    pub victim_confirmations: u64,
    /// How many the merchant had seen when it handed over the goods.
    pub confirmations_at_release: Option<u64>,
    /// How many it had seen at the moment the chain switched under it.
    pub confirmations_when_reverted: Option<u64>,
    /// Whether the merchant has handed over the goods.
    pub victim_released: bool,
    /// How many blocks the reorganisation threw away.
    pub blocks_reverted: u64,
    /// Which blocks those were.
    pub reverted_blocks: Vec<BlockSummary>,
    /// How long the attack itself took, from the payment going out to the chain switching.
    pub attack_duration: Option<Duration>,
    /// Whether the attacker's own spend is in the chain now.
    pub double_spend_confirmed: bool,
    /// The most recent blocks on the public chain, oldest first.
    pub recent_public_blocks: Vec<BlockSummary>,
}

/// An attack in progress. Dropping this stops the threads and waits for them.
pub struct AttackHandle {
    engine: Arc<Engine>,
    shared: Arc<Mutex<AttackSnapshot>>,
    workers: Option<Vec<JoinHandle<()>>>,
    coordinator: Option<JoinHandle<()>>,
}

impl AttackHandle {
    /// The state of the attack right now. Takes a copy and lets go of the lock.
    pub fn snapshot(&self) -> AttackSnapshot {
        match self.shared.lock() {
            Ok(snapshot) => snapshot.clone(),
            Err(poisoned) => poisoned.into_inner().clone(),
        }
    }

    /// Stops both sides hashing.
    pub fn pause(&self) {
        self.engine.paused.store(true, Ordering::Relaxed);
    }

    /// Starts them again.
    pub fn resume(&self) {
        self.engine.paused.store(false, Ordering::Relaxed);
    }

    /// Whether it is paused.
    pub fn is_paused(&self) -> bool {
        self.engine.paused.load(Ordering::Relaxed)
    }

    /// Whether the attack has reached an outcome.
    pub fn is_finished(&self) -> bool {
        self.snapshot().finished
    }

    /// Waits for the attack to reach an outcome and hands back the last snapshot.
    ///
    /// An attack always ends: it either gets ahead and publishes, or the public chain leaves it
    /// far enough behind that it stops.
    pub fn join(mut self) -> AttackSnapshot {
        if let Some(coordinator) = self.coordinator.take() {
            let _ = coordinator.join();
        }
        if let Some(workers) = self.workers.take() {
            for worker in workers {
                let _ = worker.join();
            }
        }
        self.snapshot()
    }

    /// Calls the whole thing off and waits for the threads.
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

impl Drop for AttackHandle {
    fn drop(&mut self) {
        self.shutdown();
    }
}

/// Starts an attack. Both sides begin hashing immediately; the attacker's threads wait until the
/// payment goes out.
pub fn start_attack(config: AttackConfig) -> Result<AttackHandle, ConfigError> {
    let specs = vec![
        MinerSpec { id: HONEST, share: Share::Threads(config.honest_threads) },
        MinerSpec { id: ATTACKER, share: Share::Threads(config.attacker_threads) },
    ];
    let budget = config.honest_threads.saturating_add(config.attacker_threads);
    let threads = split_threads(&specs, budget)?;
    let target = Target::from_compact(config.bits)?;
    let time = config.start_time.unwrap_or_else(crate::mining::unix_now);
    let public = Chain::new(config.bits, time, NOBODY)?;

    let mut rng = SmallRng::seed_from_u64(config.seed);
    let slots: Vec<Arc<MinerSlot>> = specs
        .iter()
        .zip(&threads)
        .map(|(spec, count)| Arc::new(MinerSlot::new(spec.id, *count, rng.random())))
        .collect();
    let engine = Arc::new(Engine::new(slots));

    let mut coordinator = AttackCoordinator {
        public,
        private: None,
        engine: Arc::clone(&engine),
        config: config.clone(),
        threads: threads.clone(),
        started: Instant::now(),
        paused_total: Duration::ZERO,
        paused_since: None,
        last_block_at: Duration::ZERO,
        phase: AttackPhase::Warmup,
        outcome: None,
        fork_height: None,
        payment: None,
        double_spend: None,
        payment_height: None,
        victim_released: false,
        confirmations_at_release: None,
        confirmations_when_reverted: None,
        blocks_reverted: 0,
        reverted_blocks: Vec::new(),
        max_deficit: 0,
        broadcast_at: None,
        attack_duration: None,
        recent: VecDeque::new(),
        shared: Arc::new(Mutex::new(empty_snapshot(&config, &threads, target))),
    };
    coordinator.retarget_honest();
    let shared = Arc::clone(&coordinator.shared);

    let (sender, receiver) = mpsc::channel::<Found>();
    let workers = spawn_workers(&engine, &sender);
    drop(sender);

    let handle = thread::Builder::new()
        .name("nmtk-pow-attack".to_string())
        .spawn(move || coordinator.run(&receiver))
        .ok();

    Ok(AttackHandle { engine, shared, workers: Some(workers), coordinator: handle })
}

struct BlockRecord {
    summary: BlockSummary,
}

struct AttackCoordinator {
    public: Chain,
    private: Option<Chain>,
    engine: Arc<Engine>,
    config: AttackConfig,
    threads: Vec<usize>,
    started: Instant,
    paused_total: Duration,
    paused_since: Option<Instant>,
    last_block_at: Duration,
    phase: AttackPhase,
    outcome: Option<AttackOutcome>,
    fork_height: Option<u64>,
    payment: Option<Tx>,
    double_spend: Option<Tx>,
    payment_height: Option<u64>,
    victim_released: bool,
    confirmations_at_release: Option<u64>,
    confirmations_when_reverted: Option<u64>,
    blocks_reverted: u64,
    reverted_blocks: Vec<BlockSummary>,
    max_deficit: u64,
    broadcast_at: Option<Duration>,
    attack_duration: Option<Duration>,
    recent: VecDeque<BlockRecord>,
    shared: Arc<Mutex<AttackSnapshot>>,
}

impl AttackCoordinator {
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
            self.advance();
            if last_sample.elapsed() >= RATE_WINDOW {
                sample_rates(&self.engine, &mut previous_hashes, last_sample.elapsed());
                last_sample = Instant::now();
            }
            if last_publish.elapsed() >= PUBLISH_EVERY {
                self.publish(false);
                last_publish = Instant::now();
            }
            if self.phase == AttackPhase::Finished {
                break;
            }
        }
        self.engine.stop.store(true, Ordering::Relaxed);
        self.publish(true);
    }

    fn handle(&mut self, found: Found) {
        let elapsed = self.elapsed();
        let since_previous = elapsed.saturating_sub(self.last_block_at);
        if found.miner == HONEST {
            let accepted = self.public.accept(found.block.clone());
            if let Ok(Acceptance::Extended { .. } | Acceptance::Reorg(_)) = accepted {
                self.record(&found.block, elapsed, since_previous, true);
                self.retarget_honest();
            }
            return;
        }
        // The attacker's block goes on the chain nobody else can see.
        let accepted = match self.private.as_mut() {
            Some(private) => private.accept(found.block.clone()),
            None => return,
        };
        if let Ok(Acceptance::Extended { .. } | Acceptance::Reorg(_)) = accepted {
            self.record(&found.block, elapsed, since_previous, false);
            self.retarget_attacker();
        }
    }

    /// Moves the story on. Called after every block and on every idle tick, so a phase change
    /// never waits for the next block.
    fn advance(&mut self) {
        if self.phase == AttackPhase::Finished {
            return;
        }
        let elapsed = self.elapsed();
        if let (Some(private), Some(_)) = (self.private.as_ref(), self.fork_height) {
            let deficit = self.public.height().saturating_sub(private.height());
            self.max_deficit = self.max_deficit.max(deficit);
        }

        match self.phase {
            AttackPhase::Warmup => {
                if self.public.height() >= self.config.warmup_blocks {
                    self.broadcast_the_payment(elapsed);
                }
            }
            AttackPhase::AwaitingPayment => {
                if let Some(payment) = &self.payment
                    && let Some(height) = self.public.tx_height(&payment.txid())
                {
                    self.payment_height = Some(height);
                    self.phase = AttackPhase::Confirming;
                    // The payment is confirmed; it does not belong in the next block.
                    self.retarget_honest();
                }
            }
            AttackPhase::Confirming => {
                let confirmations = self.victim_confirmations();
                if confirmations >= self.config.confirmations_required {
                    self.victim_released = true;
                    self.confirmations_at_release = Some(confirmations);
                    self.phase = AttackPhase::Racing;
                }
            }
            AttackPhase::Racing => {
                let ahead = match self.private.as_ref() {
                    Some(private) => private.total_work() > self.public.total_work(),
                    None => false,
                };
                if ahead {
                    self.publish_the_private_chain(elapsed);
                    return;
                }
            }
            AttackPhase::Finished => return,
        }

        if self.should_give_up(elapsed) {
            self.finish(AttackOutcome::GaveUp, elapsed);
        }
    }

    /// The attacker pays the merchant in public and starts a chain of its own in private, forked
    /// from the block before the payment can possibly land.
    fn broadcast_the_payment(&mut self, elapsed: Duration) {
        let fork_height = self.public.height();
        let Some(private) = self.public.fork_at(fork_height) else {
            return;
        };
        self.private = Some(private);
        self.fork_height = Some(fork_height);
        self.payment = Some(Tx::spend(self.config.coin, self.config.victim));
        self.double_spend = Some(Tx::spend(self.config.coin, self.config.attacker_payee));
        self.broadcast_at = Some(elapsed);
        self.phase = AttackPhase::AwaitingPayment;
        self.retarget_honest();
        self.retarget_attacker();
    }

    /// The private chain goes out. Every node validates it, sees more work, and switches.
    fn publish_the_private_chain(&mut self, elapsed: Duration) {
        let Some(fork_height) = self.fork_height else {
            return;
        };
        let blocks: Vec<Block> = match self.private.as_ref() {
            Some(private) => private.blocks_after(fork_height).to_vec(),
            None => return,
        };
        self.confirmations_when_reverted = Some(self.victim_confirmations());

        for block in blocks {
            if let Ok(Acceptance::Reorg(reorg)) = self.public.accept(block) {
                self.blocks_reverted += reorg.removed.len() as u64;
                for removed in &reorg.removed {
                    self.reverted_blocks.push(summary_of(removed, elapsed, Duration::ZERO));
                }
            }
        }

        let payment_gone = match &self.payment {
            Some(payment) => self.public.tx_height(&payment.txid()).is_none(),
            None => false,
        };
        let double_spend_in = self.double_spend_confirmed();
        self.attack_duration = self.broadcast_at.map(|started| elapsed.saturating_sub(started));
        let outcome = if payment_gone && double_spend_in {
            AttackOutcome::Succeeded
        } else {
            AttackOutcome::GaveUp
        };
        self.finish(outcome, elapsed);
    }

    fn should_give_up(&self, elapsed: Duration) -> bool {
        if let Some(limit) = self.config.give_up_after
            && elapsed >= limit
        {
            return true;
        }
        match self.fork_height {
            Some(fork_height) => {
                self.public.height().saturating_sub(fork_height)
                    >= self.config.give_up_after_public_blocks
            }
            None => false,
        }
    }

    fn finish(&mut self, outcome: AttackOutcome, elapsed: Duration) {
        self.outcome = Some(outcome);
        self.phase = AttackPhase::Finished;
        if self.attack_duration.is_none() {
            self.attack_duration = self.broadcast_at.map(|start| elapsed.saturating_sub(start));
        }
        self.engine.stop.store(true, Ordering::Relaxed);
    }

    fn victim_confirmations(&self) -> u64 {
        match (&self.payment, self.payment_height) {
            (Some(payment), Some(_)) => self.public.confirmations(&payment.txid()).unwrap_or(0),
            _ => 0,
        }
    }

    fn double_spend_confirmed(&self) -> bool {
        match &self.double_spend {
            Some(tx) => self.public.tx_height(&tx.txid()).is_some(),
            None => false,
        }
    }

    fn retarget_honest(&mut self) {
        let txs = match (&self.payment, self.phase) {
            // The payment is only waiting to be mined between going out and landing in a block.
            (Some(payment), AttackPhase::AwaitingPayment) => vec![payment.clone()],
            _ => Vec::new(),
        };
        let template = BlockTemplate {
            version: self.config.version,
            prev_hash: self.public.tip_hash(),
            height: self.public.height() + 1,
            time: self.config.start_time.unwrap_or_else(crate::mining::unix_now),
            bits: self.config.bits,
            miner: HONEST,
            txs,
        };
        self.set_job(HONEST, template);
    }

    fn retarget_attacker(&mut self) {
        let Some(private) = self.private.as_ref() else {
            return;
        };
        // The conflicting spend goes into the first private block and stays there.
        let txs = match &self.double_spend {
            Some(tx) if private.tx_height(&tx.txid()).is_none() => vec![tx.clone()],
            _ => Vec::new(),
        };
        let template = BlockTemplate {
            version: self.config.version,
            prev_hash: private.tip_hash(),
            height: private.height() + 1,
            time: self.config.start_time.unwrap_or_else(crate::mining::unix_now),
            bits: self.config.bits,
            miner: ATTACKER,
            txs,
        };
        self.set_job(ATTACKER, template);
    }

    fn set_job(&self, miner: MinerId, template: BlockTemplate) {
        let target = self.public.target();
        if let Some(slot) = self.engine.slots.iter().find(|slot| slot.id == miner) {
            slot.set_job(Some(Arc::new(PreparedJob::new(template, target))));
        }
    }

    fn record(&mut self, block: &Block, elapsed: Duration, since_previous: Duration, public: bool) {
        self.last_block_at = elapsed;
        if !public {
            return;
        }
        self.recent.push_back(BlockRecord { summary: summary_of(block, elapsed, since_previous) });
        while self.recent.len() > self.config.recent_blocks_kept.max(1) {
            self.recent.pop_front();
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

    fn elapsed(&self) -> Duration {
        let paused_now = self.paused_since.map(|since| since.elapsed()).unwrap_or_default();
        self.started.elapsed().saturating_sub(self.paused_total).saturating_sub(paused_now)
    }

    fn publish(&mut self, finished: bool) {
        let elapsed = self.elapsed();
        let seconds = elapsed.as_secs_f64();
        let shares = effective_shares(&self.threads);
        let in_chain = self.public.blocks_by_miner();
        let miner_at = |index: usize| -> MinerSnapshot {
            let slot = &self.engine.slots[index];
            let hashes = slot.hashes.load(Ordering::Relaxed);
            MinerSnapshot {
                id: slot.id,
                threads: slot.threads,
                requested: Share::Threads(slot.threads),
                effective_share: shares.get(index).copied().unwrap_or(0.0),
                hashes,
                hashrate: slot.hashrate(),
                average_hashrate: if seconds > 0.0 { hashes as f64 / seconds } else { 0.0 },
                blocks_found: slot.blocks.load(Ordering::Relaxed),
                blocks_in_chain: in_chain.get(&slot.id).copied().unwrap_or(0),
            }
        };
        let private_height = self.private.as_ref().map(Chain::height).unwrap_or(0);
        let lead = match self.private.as_ref() {
            Some(private) => private.height() as i64 - self.public.height() as i64,
            None => 0,
        };
        let target = self.public.target();
        let snapshot = AttackSnapshot {
            phase: self.phase,
            outcome: self.outcome,
            elapsed,
            paused: self.engine.paused.load(Ordering::Relaxed),
            finished,
            bits: self.config.bits,
            difficulty: target.difficulty(),
            attacker: miner_at(1),
            honest: miner_at(0),
            attacker_share: shares.get(1).copied().unwrap_or(0.0),
            public_height: self.public.height(),
            public_tip: self.public.tip_hash(),
            private_height,
            fork_height: self.fork_height,
            lead,
            max_deficit: self.max_deficit,
            payment_txid: self.payment.as_ref().map(Tx::txid),
            double_spend_txid: self.double_spend.as_ref().map(Tx::txid),
            payment_height: self.payment_height,
            victim_confirmations: self.victim_confirmations(),
            confirmations_at_release: self.confirmations_at_release,
            confirmations_when_reverted: self.confirmations_when_reverted,
            victim_released: self.victim_released,
            blocks_reverted: self.blocks_reverted,
            reverted_blocks: self.reverted_blocks.clone(),
            attack_duration: self.attack_duration,
            double_spend_confirmed: self.double_spend_confirmed(),
            recent_public_blocks: self
                .recent
                .iter()
                .map(|record| {
                    let mut summary = record.summary.clone();
                    summary.in_best_chain = self.public.contains(&summary.hash);
                    summary
                })
                .collect(),
        };
        match self.shared.lock() {
            Ok(mut slot) => *slot = snapshot,
            Err(poisoned) => *poisoned.into_inner() = snapshot,
        }
    }
}

fn summary_of(block: &Block, found_after: Duration, since_previous: Duration) -> BlockSummary {
    let hash = block.hash();
    BlockSummary {
        height: block.height(),
        hash,
        miner: block.miner(),
        nonce: block.header.nonce,
        extra_nonce: block.coinbase.extra_nonce,
        found_after,
        since_previous,
        leading_zero_bits: hash.leading_zero_bits(),
        in_best_chain: false,
    }
}

fn empty_snapshot(config: &AttackConfig, threads: &[usize], target: Target) -> AttackSnapshot {
    let shares = effective_shares(threads);
    let miner = |index: usize, id: MinerId| MinerSnapshot {
        id,
        threads: threads.get(index).copied().unwrap_or(0),
        requested: Share::Threads(threads.get(index).copied().unwrap_or(0)),
        effective_share: shares.get(index).copied().unwrap_or(0.0),
        hashes: 0,
        hashrate: 0.0,
        average_hashrate: 0.0,
        blocks_found: 0,
        blocks_in_chain: 0,
    };
    AttackSnapshot {
        phase: AttackPhase::Warmup,
        outcome: None,
        elapsed: Duration::ZERO,
        paused: false,
        finished: false,
        bits: config.bits,
        difficulty: target.difficulty(),
        attacker: miner(1, ATTACKER),
        honest: miner(0, HONEST),
        attacker_share: shares.get(1).copied().unwrap_or(0.0),
        public_height: 0,
        public_tip: Hash256::ZERO,
        private_height: 0,
        fork_height: None,
        lead: 0,
        max_deficit: 0,
        payment_txid: None,
        double_spend_txid: None,
        payment_height: None,
        victim_confirmations: 0,
        confirmations_at_release: None,
        confirmations_when_reverted: None,
        victim_released: false,
        blocks_reverted: 0,
        reverted_blocks: Vec::new(),
        attack_duration: None,
        double_spend_confirmed: false,
        recent_public_blocks: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::practice_bits;

    #[test]
    fn an_overwhelming_share_really_undoes_a_confirmed_payment() {
        let mut config = AttackConfig::new(practice_bits(14).expect("expressible"), 9, 1);
        config.confirmations_required = 2;
        config.warmup_blocks = 1;
        config.give_up_after_public_blocks = 24;
        config.give_up_after = Some(Duration::from_secs(90));
        let snapshot = start_attack(config).expect("valid config").join();

        assert_eq!(snapshot.outcome, Some(AttackOutcome::Succeeded), "{snapshot:?}");
        assert!(snapshot.victim_released, "the merchant never handed anything over");
        assert_eq!(snapshot.confirmations_at_release, Some(2));
        assert!(snapshot.blocks_reverted >= 2, "only {} blocks went", snapshot.blocks_reverted);
        assert!(snapshot.double_spend_confirmed);
        assert_eq!(snapshot.victim_confirmations, 0, "the payment is still in the chain");
        // Once the private chain is out, it is the chain: there is nothing left to be ahead of.
        assert_eq!(snapshot.lead, 0);
        assert_eq!(snapshot.public_height, snapshot.private_height);
        assert!(snapshot.public_height > snapshot.fork_height.unwrap_or(0) + 2);
        assert!(snapshot.attack_duration.is_some());
        assert!(snapshot.attacker.hashes > snapshot.honest.hashes);
        assert!((snapshot.attacker_share - 0.9).abs() < 1e-9);
    }

    #[test]
    fn a_small_share_falls_behind_and_gives_up_with_the_payment_intact() {
        let mut config = AttackConfig::new(practice_bits(14).expect("expressible"), 1, 9);
        config.confirmations_required = 2;
        config.warmup_blocks = 1;
        config.give_up_after_public_blocks = 12;
        config.give_up_after = Some(Duration::from_secs(90));
        let snapshot = start_attack(config).expect("valid config").join();

        assert_eq!(snapshot.outcome, Some(AttackOutcome::GaveUp), "{snapshot:?}");
        assert_eq!(snapshot.blocks_reverted, 0);
        assert!(!snapshot.double_spend_confirmed);
        assert!(snapshot.victim_confirmations >= 2, "the payment lost its confirmations");
        assert!(snapshot.lead < 0, "the attacker was not behind at the end");
        assert!(snapshot.max_deficit >= 1);
        assert!(snapshot.honest.hashes > snapshot.attacker.hashes);
    }

    #[test]
    fn the_merchant_waits_for_the_confirmations_it_was_told_to() {
        let mut config =
            AttackConfig::new(practice_bits(14).expect("expressible"), 9, 1).with_confirmations(3);
        config.warmup_blocks = 1;
        config.give_up_after = Some(Duration::from_secs(90));
        let snapshot = start_attack(config).expect("valid config").join();
        assert_eq!(snapshot.confirmations_at_release, Some(3));
        // Three confirmations means three blocks had to go, the payment's and the two on top.
        assert!(snapshot.blocks_reverted >= 3, "only {} blocks went", snapshot.blocks_reverted);
    }

    #[test]
    fn a_run_with_no_threads_for_one_side_is_refused() {
        let config = AttackConfig::new(practice_bits(14).expect("expressible"), 0, 4);
        assert!(matches!(start_attack(config), Err(ConfigError::InvalidShare(ATTACKER))));
    }
}
