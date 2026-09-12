//! Proof of work: real double-SHA-256 mining, competing miners, and 51% attacks.
//!
//! Everything in this crate runs for real. Blocks are Bitcoin's 80-byte headers hashed twice with
//! SHA-256 and compared against a target unpacked from the same four compact bytes a real header
//! carries. Miners are threads that hash until they find something or are told to stop, and their
//! hash rates are counted, not estimated. The chain validates every block it is offered, keeps
//! the forks it cannot yet judge, and replaces itself when a fork turns out to carry more work.
//! The 51% attack ends with a payment that was confirmed no longer being in the chain.
//!
//! # Where to start
//!
//! - [`target::DIFFICULTY_ONE_BITS`] is the difficulty Bitcoin ran at in January 2009, and
//!   [`target::practice_bits`] makes one that finishes while somebody is watching.
//! - [`estimate::expected_time_to_block`] turns a hash rate and a difficulty into a waiting time,
//!   with the honest warning that it is an average over a long-tailed distribution.
//! - [`engine::mine_serial`] mines one block on the calling thread and hands back how many hashes
//!   it took.
//! - [`mining::start_mining`] runs several named miners at once, each with a share of the
//!   machine's threads, and [`mining::MiningHandle::snapshot`] hands a screen everything it needs
//!   to draw.
//! - [`attack::start_attack`] runs the double spend end to end and records what happened.
//!
//! # What this crate does not do
//!
//! It writes nothing a reader will see. Miners are numbers, not names; outcomes are enums, not
//! sentences. Difficulty is never adjusted — every block in a run is mined at the difficulty the
//! run was started with — and timestamps are not checked, because neither is what proof of work
//! is for. There are no signatures and no amounts: a transaction here is the coins it consumes
//! and who it pays, which is all a double spend needs.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod attack;
pub mod block;
pub mod chain;
pub mod engine;
pub mod estimate;
pub mod hash;
pub mod miners;
pub mod mining;
pub mod target;

pub use attack::{
    ATTACKER, AttackConfig, AttackHandle, AttackOutcome, AttackPhase, AttackSnapshot, HONEST,
    start_attack,
};
pub use block::{Block, BlockHeader, BlockTemplate, CoinId, Coinbase, Tx, Txid};
pub use chain::{Acceptance, Branch, Chain, ChainError, Reorg};
pub use engine::mine_serial;
pub use estimate::{
    BITCOIN_TARGET_BLOCK_SECONDS, BlockTimeEstimate, expected_time_to_block,
    implied_network_hashrate, probability_of_block_within,
};
pub use hash::{Hash256, double_sha256, merkle_root};
pub use miners::{ConfigError, MinerId, MinerSpec, NOBODY, Share, effective_shares, split_threads};
pub use mining::{
    BlockSummary, ForkSummary, MinerSnapshot, MiningConfig, MiningHandle, MiningSnapshot,
    ReorgSummary, start_mining,
};
pub use target::{DIFFICULTY_ONE_BITS, Target, TargetError, practice_bits};
