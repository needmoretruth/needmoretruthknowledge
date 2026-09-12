//! A chain of blocks, the forks that grow beside it, and the reorganisation that replaces it.
//!
//! Nothing here trusts a block. Every one that arrives is checked — it names the block it builds
//! on, it claims the next height, it carries the difficulty the chain is running at, its header
//! commits to the transactions inside it, its hash is under the target, and none of the coins it
//! spends were already spent by one of its ancestors. A block that fails any of those is rejected
//! with the reason.
//!
//! A block that passes but does not sit on the tip is not thrown away: it starts a branch. When a
//! branch ends up carrying more work than the chain it forked from, the chain rolls back to the
//! fork point and the branch takes its place. That is a reorg, and it is how a 51% attack undoes
//! a payment that looked settled.

use core::fmt;
use std::collections::{HashMap, HashSet};

use crate::block::{Block, BlockHeader, CoinId, Coinbase, Txid};
use crate::hash::Hash256;
use crate::miners::MinerId;
use crate::target::{Target, TargetError};

/// Why a block was refused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChainError {
    /// The block claims a height that is not one past the block it builds on.
    HeightMismatch {
        /// The height the chain expected.
        expected: u64,
        /// The height the block claimed.
        found: u64,
    },
    /// The block names a previous block that is not where it was offered.
    PrevHashMismatch,
    /// The block carries a different difficulty from the chain it is joining.
    BitsMismatch {
        /// The packed target the chain runs at.
        expected: u32,
        /// The packed target the block carried.
        found: u32,
    },
    /// The header's merkle root is not the root of the transactions in the block.
    MerkleRootMismatch,
    /// The header hashes to a number above the target: no proof of work.
    HashAboveTarget,
    /// A transaction in this block is already in the chain it is joining.
    DuplicateTxid(Txid),
    /// A coin this block spends was already spent by one of its ancestors.
    CoinAlreadySpent(CoinId),
}

impl fmt::Display for ChainError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ChainError::HeightMismatch { expected, found } => {
                write!(f, "block claims height {found}, chain expected {expected}")
            }
            ChainError::PrevHashMismatch => {
                f.write_str("block does not build on the block offered")
            }
            ChainError::BitsMismatch { expected, found } => {
                write!(f, "block carries bits {found:#010x}, chain runs at {expected:#010x}")
            }
            ChainError::MerkleRootMismatch => {
                f.write_str("header merkle root does not match the block's transactions")
            }
            ChainError::HashAboveTarget => f.write_str("block hash is above the target"),
            ChainError::DuplicateTxid(_) => f.write_str("a transaction is already in this chain"),
            ChainError::CoinAlreadySpent(_) => f.write_str("a coin in this block is already spent"),
        }
    }
}

impl std::error::Error for ChainError {}

/// What happened to a block that was offered to the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Acceptance {
    /// It went straight on the tip.
    Extended {
        /// The chain's new height.
        height: u64,
    },
    /// It was valid but sits on a branch beside the chain.
    Fork {
        /// The last height the branch shares with the chain.
        fork_height: u64,
        /// How many blocks the branch now holds.
        branch_length: usize,
        /// How far the branch's tip is behind the chain's tip. Zero means level.
        behind: u64,
    },
    /// It finished a branch that carries more work than the chain, and the chain was replaced.
    Reorg(Reorg),
    /// The chain or one of its branches already has this block.
    Duplicate,
    /// It builds on a block nobody here has.
    Orphan,
}

/// What a reorganisation did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Reorg {
    /// The last height the two chains agree on. Everything above it changed.
    pub fork_height: u64,
    /// The blocks that were rolled back, lowest first. Every transaction in them is unconfirmed
    /// again, and any that conflicts with the new chain is gone for good.
    pub removed: Vec<Block>,
    /// How many blocks took their place.
    pub added: usize,
    /// The height before.
    pub old_height: u64,
    /// The height after.
    pub new_height: u64,
}

/// A branch growing beside the chain.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Branch {
    /// The last height this branch shares with the chain.
    pub fork_height: u64,
    /// Its blocks, lowest first, starting one past the fork.
    pub blocks: Vec<Block>,
}

impl Branch {
    /// The height of the branch's own tip.
    pub fn tip_height(&self) -> u64 {
        self.fork_height + self.blocks.len() as u64
    }
}

/// The blocks this node believes in, plus the branches it is keeping an eye on.
#[derive(Debug, Clone)]
pub struct Chain {
    blocks: Vec<Block>,
    tip_hash: Hash256,
    bits: u32,
    target: Target,
    tx_height: HashMap<Txid, u64>,
    spent_height: HashMap<CoinId, u64>,
    hash_height: HashMap<Hash256, u64>,
    branches: Vec<Branch>,
    max_branches: usize,
    reorgs: u64,
}

impl Chain {
    /// How many branches a chain keeps before it drops the weakest.
    pub const DEFAULT_MAX_BRANCHES: usize = 8;

    /// A new chain holding only its genesis block.
    ///
    /// The genesis block is given rather than mined, exactly as Bitcoin's is hard-coded into every
    /// node. Every block after it has to earn its place.
    pub fn new(bits: u32, time: u32, miner: MinerId) -> Result<Chain, TargetError> {
        let target = Target::from_compact(bits)?;
        let coinbase = Coinbase { height: 0, miner, extra_nonce: 0 };
        let genesis = Block {
            header: BlockHeader {
                version: 1,
                prev_hash: Hash256::ZERO,
                merkle_root: crate::hash::merkle_root(&[coinbase.txid().hash()]),
                time,
                bits,
                nonce: 0,
            },
            coinbase,
            txs: Vec::new(),
        };
        let tip_hash = genesis.hash();
        let mut tx_height = HashMap::new();
        tx_height.insert(coinbase.txid(), 0);
        let mut hash_height = HashMap::new();
        hash_height.insert(tip_hash, 0);
        Ok(Chain {
            blocks: vec![genesis],
            tip_hash,
            bits,
            target,
            tx_height,
            spent_height: HashMap::new(),
            hash_height,
            branches: Vec::new(),
            max_branches: Chain::DEFAULT_MAX_BRANCHES,
            reorgs: 0,
        })
    }

    /// The height of the tip. The genesis block is height 0.
    pub fn height(&self) -> u64 {
        self.blocks.len() as u64 - 1
    }

    /// The block at the tip.
    pub fn tip(&self) -> &Block {
        // The chain always holds at least its genesis block, so this cannot be empty.
        match self.blocks.last() {
            Some(block) => block,
            None => unreachable!("a chain always has a genesis block"),
        }
    }

    /// The tip's hash, which is what the next block has to name.
    pub fn tip_hash(&self) -> Hash256 {
        self.tip_hash
    }

    /// Every block, lowest first.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The block at a height, if the chain is that long.
    pub fn block_at(&self, height: u64) -> Option<&Block> {
        self.blocks.get(usize::try_from(height).ok()?)
    }

    /// The blocks above a height, lowest first.
    pub fn blocks_after(&self, height: u64) -> &[Block] {
        match usize::try_from(height.saturating_add(1)) {
            Ok(start) if start <= self.blocks.len() => &self.blocks[start..],
            _ => &[],
        }
    }

    /// The packed target this chain runs at.
    pub fn bits(&self) -> u32 {
        self.bits
    }

    /// The target every block in this chain has to beat.
    pub fn target(&self) -> Target {
        self.target
    }

    /// The work behind this chain: the expected number of hashes it took to build.
    pub fn total_work(&self) -> u128 {
        self.work_up_to(self.height())
    }

    /// How many times this chain has been replaced from a branch.
    pub fn reorg_count(&self) -> u64 {
        self.reorgs
    }

    /// The branches growing beside the chain.
    pub fn branches(&self) -> &[Branch] {
        &self.branches
    }

    /// The height a transaction was confirmed at, if it is in this chain.
    pub fn tx_height(&self, txid: &Txid) -> Option<u64> {
        self.tx_height.get(txid).copied()
    }

    /// How many blocks deep a transaction is, counting its own. `None` means it is not in this
    /// chain at all — which is exactly what a victim of a successful double spend discovers.
    pub fn confirmations(&self, txid: &Txid) -> Option<u64> {
        self.tx_height.get(txid).map(|height| self.height() - height + 1)
    }

    /// The height a coin was spent at, if it was.
    pub fn coin_spent_at(&self, coin: &CoinId) -> Option<u64> {
        self.spent_height.get(coin).copied()
    }

    /// A copy of this chain cut back to a height, used to start mining a private fork from there.
    pub fn fork_at(&self, height: u64) -> Option<Chain> {
        let keep = usize::try_from(height).ok()? + 1;
        if keep > self.blocks.len() {
            return None;
        }
        let mut forked = Chain {
            blocks: self.blocks[..keep].to_vec(),
            tip_hash: Hash256::ZERO,
            bits: self.bits,
            target: self.target,
            tx_height: HashMap::new(),
            spent_height: HashMap::new(),
            hash_height: HashMap::new(),
            branches: Vec::new(),
            max_branches: self.max_branches,
            reorgs: 0,
        };
        forked.rebuild_index();
        Some(forked)
    }

    /// Offers a block to the chain.
    ///
    /// A valid block either extends the chain, joins a branch, or triggers a reorg when its branch
    /// overtakes the chain. An invalid one comes back with the reason it was refused.
    pub fn accept(&mut self, block: Block) -> Result<Acceptance, ChainError> {
        let hash = block.hash();
        if self.already_have(&hash) {
            return Ok(Acceptance::Duplicate);
        }
        let prev = block.header.prev_hash;

        if prev == self.tip_hash {
            let height = self.height();
            self.check(&block, prev, height, None)?;
            self.push(block);
            return Ok(Acceptance::Extended { height: self.height() });
        }

        if let Some(index) = self
            .branches
            .iter()
            .position(|branch| branch.blocks.last().map(|last| last.hash()) == Some(prev))
        {
            let prev_height = self.branches[index].tip_height();
            self.check(&block, prev, prev_height, Some(index))?;
            self.branches[index].blocks.push(block);
            return Ok(self.settle_branch(index));
        }

        if let Some(prev_height) = self.height_of(&prev) {
            self.check(&block, prev, prev_height, None)?;
            self.make_room_for_a_branch();
            self.branches.push(Branch { fork_height: prev_height, blocks: vec![block] });
            let index = self.branches.len() - 1;
            return Ok(self.settle_branch(index));
        }

        Ok(Acceptance::Orphan)
    }

    /// Checks whether a branch now beats the chain, and swaps them if it does.
    fn settle_branch(&mut self, index: usize) -> Acceptance {
        let (fork_height, branch_len, branch_work) = {
            let branch = &self.branches[index];
            let work_per_block = self.target.work();
            let work = self.work_up_to(branch.fork_height)
                + work_per_block.saturating_mul(branch.blocks.len() as u128);
            (branch.fork_height, branch.blocks.len(), work)
        };
        if branch_work <= self.total_work() {
            let tip_height = fork_height + branch_len as u64;
            return Acceptance::Fork {
                fork_height,
                branch_length: branch_len,
                behind: self.height().saturating_sub(tip_height),
            };
        }

        let old_height = self.height();
        let branch = self.branches.swap_remove(index);
        let keep = match usize::try_from(fork_height) {
            Ok(height) => height + 1,
            Err(_) => return Acceptance::Orphan,
        };
        let removed: Vec<Block> = self.blocks.split_off(keep);
        let added = branch.blocks.len();
        self.blocks.extend(branch.blocks);
        self.tip_hash = self.tip().hash();
        self.rebuild_index();
        self.reorgs += 1;
        // The chain that just lost becomes a branch of its own: it can still come back.
        if !removed.is_empty() {
            self.make_room_for_a_branch();
            self.branches.push(Branch { fork_height, blocks: removed.clone() });
        }
        Acceptance::Reorg(Reorg {
            fork_height,
            removed,
            added,
            old_height,
            new_height: self.height(),
        })
    }

    /// All of the proof of work in the chain up to a height.
    fn work_up_to(&self, height: u64) -> u128 {
        // The genesis block was given, not mined, so it carries no work.
        self.target.work().saturating_mul(height as u128)
    }

    /// Whether this exact block is already in the chain.
    pub fn contains(&self, hash: &Hash256) -> bool {
        self.hash_height.contains_key(hash)
    }

    /// How many blocks in the chain each miner found.
    pub fn blocks_by_miner(&self) -> HashMap<MinerId, u64> {
        let mut counts = HashMap::new();
        for block in &self.blocks {
            *counts.entry(block.miner()).or_insert(0) += 1;
        }
        counts
    }

    fn already_have(&self, hash: &Hash256) -> bool {
        self.hash_height.contains_key(hash)
            || self
                .branches
                .iter()
                .any(|branch| branch.blocks.iter().any(|block| block.hash() == *hash))
    }

    fn height_of(&self, hash: &Hash256) -> Option<u64> {
        self.hash_height.get(hash).copied()
    }

    /// Every check a block has to pass. `branch` names the branch it would join, if any; a block
    /// on a branch is judged against that branch's own ancestors, not the chain's.
    fn check(
        &self,
        block: &Block,
        prev_hash: Hash256,
        prev_height: u64,
        branch: Option<usize>,
    ) -> Result<(), ChainError> {
        if block.header.prev_hash != prev_hash {
            return Err(ChainError::PrevHashMismatch);
        }
        let expected_height = prev_height + 1;
        if block.height() != expected_height {
            return Err(ChainError::HeightMismatch {
                expected: expected_height,
                found: block.height(),
            });
        }
        if block.header.bits != self.bits {
            return Err(ChainError::BitsMismatch { expected: self.bits, found: block.header.bits });
        }
        if block.header.merkle_root != block.computed_merkle_root() {
            return Err(ChainError::MerkleRootMismatch);
        }
        if !self.target.is_met_by(&block.hash()) {
            return Err(ChainError::HashAboveTarget);
        }

        // What counts as already spent depends on which ancestors this block has.
        let (ancestor_height, extra) = match branch {
            None => (self.height().min(prev_height), None),
            Some(index) => {
                let branch = &self.branches[index];
                (branch.fork_height, Some(branch))
            }
        };
        let mut branch_txids: HashSet<Txid> = HashSet::new();
        let mut branch_coins: HashSet<CoinId> = HashSet::new();
        if let Some(branch) = extra {
            for earlier in &branch.blocks {
                for txid in earlier.txids() {
                    branch_txids.insert(txid);
                }
                for tx in &earlier.txs {
                    branch_coins.extend(tx.inputs.iter().copied());
                }
            }
        }

        let mut here_txids: HashSet<Txid> = HashSet::new();
        for txid in block.txids() {
            let on_chain = self.tx_height.get(&txid).is_some_and(|h| *h <= ancestor_height);
            if on_chain || branch_txids.contains(&txid) || !here_txids.insert(txid) {
                return Err(ChainError::DuplicateTxid(txid));
            }
        }
        let mut here_coins: HashSet<CoinId> = HashSet::new();
        for tx in &block.txs {
            for coin in &tx.inputs {
                let on_chain = self.spent_height.get(coin).is_some_and(|h| *h <= ancestor_height);
                if on_chain || branch_coins.contains(coin) || !here_coins.insert(*coin) {
                    return Err(ChainError::CoinAlreadySpent(*coin));
                }
            }
        }
        Ok(())
    }

    fn push(&mut self, block: Block) {
        let height = block.height();
        for txid in block.txids() {
            self.tx_height.insert(txid, height);
        }
        for tx in &block.txs {
            for coin in &tx.inputs {
                self.spent_height.insert(*coin, height);
            }
        }
        let hash = block.hash();
        self.hash_height.insert(hash, height);
        self.tip_hash = hash;
        self.blocks.push(block);
    }

    fn rebuild_index(&mut self) {
        self.tx_height.clear();
        self.spent_height.clear();
        self.hash_height.clear();
        for block in &self.blocks {
            let height = block.height();
            self.hash_height.insert(block.hash(), height);
            for txid in block.txids() {
                self.tx_height.insert(txid, height);
            }
            for tx in &block.txs {
                for coin in &tx.inputs {
                    self.spent_height.insert(*coin, height);
                }
            }
        }
        self.tip_hash = match self.blocks.last() {
            Some(block) => block.hash(),
            None => Hash256::ZERO,
        };
    }

    /// Drops the branch that is furthest behind, so the list stays bounded. A node cannot keep
    /// every fork it ever hears about, and the ones far behind are the ones least likely to win.
    fn make_room_for_a_branch(&mut self) {
        while self.branches.len() >= self.max_branches {
            let weakest = self
                .branches
                .iter()
                .enumerate()
                .min_by_key(|(_, branch)| branch.tip_height())
                .map(|(index, _)| index);
            match weakest {
                Some(index) => {
                    self.branches.remove(index);
                }
                None => break,
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::block::{BlockTemplate, Tx};
    use crate::engine::mine_serial;
    use crate::target::practice_bits;

    fn easy_bits() -> u32 {
        practice_bits(12).expect("expressible")
    }

    fn chain() -> Chain {
        Chain::new(easy_bits(), 1_231_006_505, MinerId(0)).expect("valid bits")
    }

    /// Mines one real block on top of a tip. Every block in these tests is found by hashing.
    fn mine_on(chain: &Chain, miner: u32, txs: Vec<Tx>, salt: u64) -> Block {
        let template = BlockTemplate {
            version: 1,
            prev_hash: chain.tip_hash(),
            height: chain.height() + 1,
            time: 1_231_006_505 + chain.height() as u32,
            bits: chain.bits(),
            miner: MinerId(miner),
            txs,
        };
        mine_serial(&template, chain.target(), salt, 200_000_000)
            .expect("a 12-bit target is found in a few thousand hashes")
            .0
    }

    fn mine_on_branch(chain: &Chain, prev: &Block, miner: u32, txs: Vec<Tx>, salt: u64) -> Block {
        let template = BlockTemplate {
            version: 1,
            prev_hash: prev.hash(),
            height: prev.height() + 1,
            time: 1_231_006_600,
            bits: chain.bits(),
            miner: MinerId(miner),
            txs,
        };
        mine_serial(&template, chain.target(), salt, 200_000_000).expect("found").0
    }

    #[test]
    fn a_mined_block_really_validates_and_extends_the_chain() {
        let mut chain = chain();
        let block = mine_on(&chain, 1, vec![], 0);
        assert!(chain.target().is_met_by(&block.hash()));
        assert_eq!(chain.accept(block).expect("valid"), Acceptance::Extended { height: 1 });
        assert_eq!(chain.height(), 1);
        assert_eq!(chain.tip().miner(), MinerId(1));
    }

    #[test]
    fn a_block_that_did_no_work_is_refused() {
        let mut chain = chain();
        let template = BlockTemplate {
            version: 1,
            prev_hash: chain.tip_hash(),
            height: 1,
            time: 1_231_006_505,
            bits: chain.bits(),
            miner: MinerId(1),
            txs: vec![],
        };
        // Nonce 0 all but certainly fails a 12-bit target; find one that does.
        let mut nonce = 0u32;
        let bad = loop {
            let candidate = template.block(0, nonce);
            if !chain.target().is_met_by(&candidate.hash()) {
                break candidate;
            }
            nonce += 1;
        };
        assert_eq!(chain.accept(bad), Err(ChainError::HashAboveTarget));
        assert_eq!(chain.height(), 0);
    }

    #[test]
    fn a_block_whose_header_lies_about_its_transactions_is_refused() {
        let mut chain = chain();
        let mut block = mine_on(&chain, 1, vec![Tx::spend(CoinId(1), 7)], 0);
        block.txs = vec![Tx::spend(CoinId(1), 8)];
        assert_eq!(chain.accept(block), Err(ChainError::MerkleRootMismatch));
    }

    #[test]
    fn one_coin_cannot_be_spent_twice_on_the_same_chain() {
        let mut chain = chain();
        let first = mine_on(&chain, 1, vec![Tx::spend(CoinId(9), 1)], 0);
        chain.accept(first).expect("valid");
        let second = mine_on(&chain, 1, vec![Tx::spend(CoinId(9), 2)], 0);
        assert_eq!(chain.accept(second), Err(ChainError::CoinAlreadySpent(CoinId(9))));
    }

    #[test]
    fn a_block_on_an_older_tip_starts_a_branch_and_a_longer_branch_replaces_the_chain() {
        let mut chain = chain();
        let one = mine_on(&chain, 1, vec![], 0);
        let fork_point = one.clone();
        chain.accept(one).expect("valid");
        let two = mine_on(&chain, 1, vec![], 0);
        chain.accept(two).expect("valid");
        assert_eq!(chain.height(), 2);
        let honest_tip = chain.tip_hash();

        // A rival block at height 2 on the same parent: valid, but only a branch.
        let rival = mine_on_branch(&chain, &fork_point, 2, vec![], 500);
        let accepted = chain.accept(rival.clone()).expect("valid");
        assert_eq!(accepted, Acceptance::Fork { fork_height: 1, branch_length: 1, behind: 0 });
        assert_eq!(chain.tip_hash(), honest_tip);

        // One more on the rival branch and it carries more work than the chain.
        let rival_two = mine_on_branch(&chain, &rival, 2, vec![], 900);
        let rival_two_hash = rival_two.hash();
        let accepted = chain.accept(rival_two).expect("valid");
        match accepted {
            Acceptance::Reorg(reorg) => {
                assert_eq!(reorg.fork_height, 1);
                assert_eq!(reorg.removed.len(), 1);
                assert_eq!(reorg.added, 2);
                assert_eq!(reorg.old_height, 2);
                assert_eq!(reorg.new_height, 3);
                assert_eq!(reorg.removed[0].miner(), MinerId(1));
            }
            other => panic!("expected a reorg, got {other:?}"),
        }
        assert_eq!(chain.height(), 3);
        assert_eq!(chain.tip_hash(), rival_two_hash);
        assert_eq!(chain.reorg_count(), 1);
    }

    #[test]
    fn a_reorg_unconfirms_the_payment_and_confirms_the_conflicting_spend() {
        let mut chain = chain();
        let fork_point = mine_on(&chain, 1, vec![], 0);
        chain.accept(fork_point.clone()).expect("valid");

        let payment = Tx::spend(CoinId(77), 1);
        let double_spend = Tx::spend(CoinId(77), 2);
        let paid = mine_on(&chain, 1, vec![payment.clone()], 0);
        chain.accept(paid).expect("valid");
        assert_eq!(chain.confirmations(&payment.txid()), Some(1));

        // The attacker's private branch spends the same coin somewhere else.
        let secret_one = mine_on_branch(&chain, &fork_point, 2, vec![double_spend.clone()], 1_000);
        chain.accept(secret_one.clone()).expect("a branch may spend a coin the chain spent");
        let secret_two = mine_on_branch(&chain, &secret_one, 2, vec![], 2_000);
        match chain.accept(secret_two).expect("valid") {
            Acceptance::Reorg(reorg) => assert_eq!(reorg.removed.len(), 1),
            other => panic!("expected a reorg, got {other:?}"),
        }

        assert_eq!(chain.confirmations(&payment.txid()), None);
        assert_eq!(chain.confirmations(&double_spend.txid()), Some(2));
        assert_eq!(chain.coin_spent_at(&CoinId(77)), Some(2));
    }

    #[test]
    fn a_block_nobody_has_the_parent_of_is_an_orphan_and_a_repeat_is_a_duplicate() {
        let mut chain = chain();
        let block = mine_on(&chain, 1, vec![], 0);
        chain.accept(block.clone()).expect("valid");
        assert_eq!(chain.accept(block).expect("known"), Acceptance::Duplicate);

        let mut stray = mine_on(&chain, 1, vec![], 0);
        stray.header.prev_hash = Hash256::from_bytes([0x5a; 32]);
        assert_eq!(chain.accept(stray).expect("no parent"), Acceptance::Orphan);
    }

    #[test]
    fn forking_at_a_height_gives_a_chain_that_does_not_know_the_blocks_above_it() {
        let mut chain = chain();
        let payment = Tx::spend(CoinId(5), 1);
        let one = mine_on(&chain, 1, vec![], 0);
        chain.accept(one).expect("valid");
        let two = mine_on(&chain, 1, vec![payment.clone()], 0);
        chain.accept(two).expect("valid");

        let private = chain.fork_at(1).expect("height exists");
        assert_eq!(private.height(), 1);
        assert_eq!(private.confirmations(&payment.txid()), None);
        assert_eq!(chain.confirmations(&payment.txid()), Some(1));
        assert_eq!(private.tip_hash(), chain.block_at(1).expect("exists").hash());
    }
}
