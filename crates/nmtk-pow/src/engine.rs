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
use std::time::Duration;

use crate::block::{Block, BlockTemplate};
use crate::hash::{Hash256, HeaderMidstate, merkle_root};
use crate::miners::MinerId;
use crate::target::Target;

/// How many hashes a worker does before it looks up to see whether the world changed.
const CHUNK: u32 = 4_096;

/// How long a worker waits for a fresh job after handing in a block, before going back to the old
/// one. A block it finds in that window would be a second block for a height already taken.
const NEW_JOB_WAIT: Duration = Duration::from_millis(250);

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

    fn job(&self) -> Option<Arc<PreparedJob>> {
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
}

impl Engine {
    /// An engine with a slot per miner, no jobs handed out yet.
    pub fn new(slots: Vec<Arc<MinerSlot>>) -> Engine {
        Engine { slots, stop: AtomicBool::new(false), paused: AtomicBool::new(false) }
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
pub fn spawn_workers(engine: &Arc<Engine>, sender: &Sender<Found>) -> Vec<JoinHandle<()>> {
    let mut handles = Vec::new();
    for slot in &engine.slots {
        for _ in 0..slot.threads {
            let engine = Arc::clone(engine);
            let slot = Arc::clone(slot);
            let sender = sender.clone();
            if let Ok(handle) = thread::Builder::new()
                .name(format!("nmtk-pow-{}", slot.id.0))
                .spawn(move || run_worker(&engine, &slot, &sender))
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

fn run_worker(engine: &Arc<Engine>, slot: &Arc<MinerSlot>, sender: &Sender<Found>) {
    while !engine.stop.load(Ordering::Relaxed) {
        if engine.paused.load(Ordering::Relaxed) {
            thread::sleep(Duration::from_millis(2));
            continue;
        }
        let version = slot.version.load(Ordering::Acquire);
        let Some(job) = slot.job() else {
            thread::sleep(Duration::from_millis(2));
            continue;
        };
        // Nobody else will ever take this extra nonce, so nobody repeats this work.
        let extra_nonce = slot.extra_nonce.fetch_add(1, Ordering::Relaxed);
        let header = job.header_bytes(extra_nonce);
        let midstate = HeaderMidstate::new(&header);
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&header[64..]);

        let mut nonce: u32 = 0;
        loop {
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
            {
                break;
            }
        }
    }
}

/// Waits a moment for the chain to hand out the next job, so a miner that just found a block does
/// not spend the next instant mining a height that is already taken.
fn wait_for_new_job(engine: &Arc<Engine>, slot: &Arc<MinerSlot>, version: u64) {
    let deadline = std::time::Instant::now() + NEW_JOB_WAIT;
    while std::time::Instant::now() < deadline {
        if engine.stop.load(Ordering::Relaxed)
            || slot.version.load(Ordering::Acquire) != version
        {
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
}
