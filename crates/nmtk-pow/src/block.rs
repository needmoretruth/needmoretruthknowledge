//! The 80 bytes a miner hashes, and the block those bytes commit to.
//!
//! The header layout is Bitcoin's, field for field and byte for byte: 4 bytes of version, the
//! previous block's 32-byte hash, the 32-byte merkle root, 4 bytes of timestamp, 4 bytes of
//! packed difficulty, 4 bytes of nonce. Everything numeric is little-endian. Change one bit
//! anywhere in those 80 bytes and the hash is unrelated to the one before it, which is the whole
//! reason mining is a search and not a calculation.

use crate::hash::{Hash256, double_sha256, merkle_root};
use crate::miners::MinerId;

/// A coin waiting to be spent. Two transactions that name the same coin are in conflict, and only
/// one of them can be in a chain.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct CoinId(pub u64);

/// A transaction's identity: the hash of the transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct Txid(Hash256);

impl Txid {
    /// The identity as a hash, for display and for the merkle tree.
    pub const fn hash(self) -> Hash256 {
        self.0
    }
}

/// A transaction, cut down to what a double spend needs: the coins it consumes and who it pays.
///
/// Real transactions carry scripts, amounts and signatures. None of that changes what a 51%
/// attack does, and all of it is the next module's subject.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Tx {
    /// The coins this transaction consumes.
    pub inputs: Vec<CoinId>,
    /// Who is being paid. Two transactions spending the same coin to different payees are the two
    /// halves of a double spend.
    pub payee: u64,
}

impl Tx {
    /// A transaction spending one coin to one payee.
    pub fn spend(coin: CoinId, payee: u64) -> Tx {
        Tx { inputs: vec![coin], payee }
    }

    /// The bytes that are hashed to get the txid.
    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = Vec::with_capacity(9 + self.inputs.len() * 8 + 8);
        bytes.push(0x01); // A tag that keeps transactions and coinbases in separate hash spaces.
        bytes.extend_from_slice(&(self.inputs.len() as u64).to_le_bytes());
        for input in &self.inputs {
            bytes.extend_from_slice(&input.0.to_le_bytes());
        }
        bytes.extend_from_slice(&self.payee.to_le_bytes());
        bytes
    }

    /// The transaction's identity, which is the double SHA-256 of its bytes.
    pub fn txid(&self) -> Txid {
        Txid(double_sha256(&self.to_bytes()))
    }
}

/// The block's own first transaction: who mined it, at what height, and the extra nonce.
///
/// Bitcoin's coinbase is where a miner writes arbitrary bytes, and rolling them is how a miner
/// keeps searching after the 4-byte nonce runs out: a different coinbase means a different merkle
/// root, which means 4 billion fresh nonces to try.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Coinbase {
    /// The height this block claims, as Bitcoin has required in the coinbase since BIP 34.
    pub height: u64,
    /// Which miner gets the reward.
    pub miner: MinerId,
    /// The search space beyond the nonce.
    pub extra_nonce: u64,
}

impl Coinbase {
    /// The bytes that are hashed to get the coinbase txid.
    pub fn to_bytes(&self) -> [u8; 21] {
        let mut bytes = [0u8; 21];
        bytes[0] = 0x00; // The tag that separates a coinbase from an ordinary transaction.
        bytes[1..9].copy_from_slice(&self.height.to_le_bytes());
        bytes[9..13].copy_from_slice(&self.miner.0.to_le_bytes());
        bytes[13..21].copy_from_slice(&self.extra_nonce.to_le_bytes());
        bytes
    }

    /// The coinbase's txid, the first leaf of the merkle tree.
    pub fn txid(&self) -> Txid {
        Txid(double_sha256(&self.to_bytes()))
    }
}

/// The 80 bytes that are hashed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BlockHeader {
    /// The block version. Bitcoin's first blocks carried 1.
    pub version: i32,
    /// The hash of the block this one builds on. This is the link that makes it a chain.
    pub prev_hash: Hash256,
    /// The root of the merkle tree over this block's transactions.
    pub merkle_root: Hash256,
    /// Seconds since the Unix epoch, as the miner claimed them.
    pub time: u32,
    /// The target, packed into four bytes.
    pub bits: u32,
    /// The number a miner changes to get a different hash.
    pub nonce: u32,
}

impl BlockHeader {
    /// A header is always this long. It is one of the few fixed sizes in Bitcoin.
    pub const SIZE: usize = 80;

    /// The header as the bytes that go over the wire and into the hash function.
    pub fn to_bytes(&self) -> [u8; 80] {
        let mut bytes = [0u8; 80];
        bytes[0..4].copy_from_slice(&self.version.to_le_bytes());
        bytes[4..36].copy_from_slice(self.prev_hash.as_bytes());
        bytes[36..68].copy_from_slice(self.merkle_root.as_bytes());
        bytes[68..72].copy_from_slice(&self.time.to_le_bytes());
        bytes[72..76].copy_from_slice(&self.bits.to_le_bytes());
        bytes[76..80].copy_from_slice(&self.nonce.to_le_bytes());
        bytes
    }

    /// The block's identity: double SHA-256 over those 80 bytes.
    pub fn hash(&self) -> Hash256 {
        double_sha256(&self.to_bytes())
    }
}

/// A whole block: the header, the coinbase it commits to, and the transactions it carries.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Block {
    /// The part that is hashed.
    pub header: BlockHeader,
    /// The block's own first transaction.
    pub coinbase: Coinbase,
    /// Everything else the block confirms.
    pub txs: Vec<Tx>,
}

impl Block {
    /// The height this block claims.
    pub fn height(&self) -> u64 {
        self.coinbase.height
    }

    /// Which miner found it.
    pub fn miner(&self) -> MinerId {
        self.coinbase.miner
    }

    /// The block's identity.
    pub fn hash(&self) -> Hash256 {
        self.header.hash()
    }

    /// The merkle root the contents actually produce. A block is only honest if this equals the
    /// root in its header.
    pub fn computed_merkle_root(&self) -> Hash256 {
        let mut leaves = Vec::with_capacity(self.txs.len() + 1);
        leaves.push(self.coinbase.txid().hash());
        leaves.extend(self.txs.iter().map(|tx| tx.txid().hash()));
        merkle_root(&leaves)
    }

    /// The txids of the transactions this block confirms, coinbase first.
    pub fn txids(&self) -> Vec<Txid> {
        let mut ids = Vec::with_capacity(self.txs.len() + 1);
        ids.push(self.coinbase.txid());
        ids.extend(self.txs.iter().map(Tx::txid));
        ids
    }
}

/// Everything a miner needs before it starts hashing, minus the two numbers it searches over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BlockTemplate {
    /// The block version to claim.
    pub version: i32,
    /// The tip this block would sit on.
    pub prev_hash: Hash256,
    /// The height this block would take.
    pub height: u64,
    /// The timestamp to claim.
    pub time: u32,
    /// The packed target this block must beat.
    pub bits: u32,
    /// Who would collect the reward.
    pub miner: MinerId,
    /// The transactions to confirm.
    pub txs: Vec<Tx>,
}

impl BlockTemplate {
    /// The merkle leaves that do not depend on the extra nonce, computed once per template.
    pub fn tx_leaves(&self) -> Vec<Hash256> {
        self.txs.iter().map(|tx| tx.txid().hash()).collect()
    }

    /// The coinbase for one point in the extra-nonce search space.
    pub fn coinbase(&self, extra_nonce: u64) -> Coinbase {
        Coinbase { height: self.height, miner: self.miner, extra_nonce }
    }

    /// The header for one extra nonce and one nonce.
    pub fn header(&self, extra_nonce: u64, nonce: u32) -> BlockHeader {
        let mut leaves = Vec::with_capacity(self.txs.len() + 1);
        leaves.push(self.coinbase(extra_nonce).txid().hash());
        leaves.extend(self.tx_leaves());
        BlockHeader {
            version: self.version,
            prev_hash: self.prev_hash,
            merkle_root: merkle_root(&leaves),
            time: self.time,
            bits: self.bits,
            nonce,
        }
    }

    /// The finished block for a nonce pair that solved the header.
    pub fn block(&self, extra_nonce: u64, nonce: u32) -> Block {
        Block {
            header: self.header(extra_nonce, nonce),
            coinbase: self.coinbase(extra_nonce),
            txs: self.txs.clone(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn template() -> BlockTemplate {
        BlockTemplate {
            version: 1,
            prev_hash: Hash256::from_bytes([7u8; 32]),
            height: 12,
            time: 1_231_006_505,
            bits: 0x1d00_ffff,
            miner: MinerId(3),
            txs: vec![Tx::spend(CoinId(1), 99)],
        }
    }

    #[test]
    fn a_header_is_exactly_eighty_bytes_in_bitcoins_order() {
        let header = BlockHeader {
            version: 1,
            prev_hash: Hash256::from_bytes([0xaa; 32]),
            merkle_root: Hash256::from_bytes([0xbb; 32]),
            time: 0x1122_3344,
            bits: 0x1d00_ffff,
            nonce: 0x0055_66aa,
        };
        let bytes = header.to_bytes();
        assert_eq!(bytes.len(), BlockHeader::SIZE);
        assert_eq!(&bytes[0..4], &[1, 0, 0, 0]);
        assert_eq!(&bytes[4..36], &[0xaa; 32]);
        assert_eq!(&bytes[36..68], &[0xbb; 32]);
        assert_eq!(&bytes[68..72], &[0x44, 0x33, 0x22, 0x11]);
        assert_eq!(&bytes[72..76], &[0xff, 0xff, 0x00, 0x1d]);
        assert_eq!(&bytes[76..80], &[0xaa, 0x66, 0x55, 0x00]);
    }

    #[test]
    fn the_real_genesis_header_hashes_to_the_hash_everyone_knows() {
        // Bitcoin's block 0, mined 2009-01-03. If the layout or the double hash were wrong by one
        // byte this would not come out.
        let mut merkle = [0u8; 32];
        // 4a5e1e4baab89f3a32518a88c31bc87f618f76673e2cc77ab2127b7afdeda33b, hash order.
        for (i, byte) in [
            0x3b, 0xa3, 0xed, 0xfd, 0x7a, 0x7b, 0x12, 0xb2, 0x7a, 0xc7, 0x2c, 0x3e, 0x67, 0x76,
            0x8f, 0x61, 0x7f, 0xc8, 0x1b, 0xc3, 0x88, 0x8a, 0x51, 0x32, 0x3a, 0x9f, 0xb8, 0xaa,
            0x4b, 0x1e, 0x5e, 0x4a,
        ]
        .into_iter()
        .enumerate()
        {
            merkle[i] = byte;
        }
        let genesis = BlockHeader {
            version: 1,
            prev_hash: Hash256::ZERO,
            merkle_root: Hash256::from_bytes(merkle),
            time: 1_231_006_505,
            bits: 0x1d00_ffff,
            nonce: 2_083_236_893,
        };
        let hex: String =
            genesis.hash().to_display_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(hex, "000000000019d6689c085ae165831e934ff763ae46a2a6c172b3f1b60a8ce26f");
    }

    #[test]
    fn a_block_commits_to_its_transactions() {
        let template = template();
        let block = template.block(5, 77);
        assert_eq!(block.header.merkle_root, block.computed_merkle_root());
        assert_eq!(block.height(), 12);
        assert_eq!(block.miner(), MinerId(3));
    }

    #[test]
    fn rolling_the_extra_nonce_moves_the_merkle_root_and_the_hash() {
        let template = template();
        let first = template.block(0, 0);
        let second = template.block(1, 0);
        assert_ne!(first.header.merkle_root, second.header.merkle_root);
        assert_ne!(first.hash(), second.hash());
    }

    #[test]
    fn two_spends_of_one_coin_to_different_payees_are_different_transactions() {
        let honest = Tx::spend(CoinId(42), 1);
        let double = Tx::spend(CoinId(42), 2);
        assert_ne!(honest.txid(), double.txid());
        assert_eq!(honest.inputs, double.inputs);
    }
}
