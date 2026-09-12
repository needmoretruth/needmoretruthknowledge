//! The two hashes a Bitcoin block is built from: double SHA-256, and the merkle root over it.

use core::fmt;

use sha2::{Digest, Sha256};

/// A 256-bit hash, kept in the byte order SHA-256 produces it.
///
/// Bitcoin reads this value as a little-endian number, so `bytes[31]` is the most significant
/// byte. A block that beats its target therefore ends in zero bytes in this array, and starts
/// with zeroes when written the way block explorers write it.
#[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Default)]
pub struct Hash256([u8; 32]);

impl Hash256 {
    /// The all-zero hash. Bitcoin uses it as the previous block of the genesis block.
    pub const ZERO: Hash256 = Hash256([0u8; 32]);

    /// Wraps 32 bytes already in hash order.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Hash256(bytes)
    }

    /// The bytes in hash order, which is what goes into the next header.
    pub const fn as_bytes(&self) -> &[u8; 32] {
        &self.0
    }

    /// The bytes in hash order, by value.
    pub const fn to_bytes(self) -> [u8; 32] {
        self.0
    }

    /// The same number with the most significant byte first, which is the order block explorers
    /// print. Nothing in the protocol uses this order; it exists only to be read.
    pub fn to_display_bytes(self) -> [u8; 32] {
        let mut out = self.0;
        out.reverse();
        out
    }

    /// How many leading zero bits the number has. A larger count means a rarer hash.
    pub fn leading_zero_bits(&self) -> u32 {
        let mut zeros = 0;
        for byte in self.0.iter().rev() {
            zeros += byte.leading_zeros();
            if *byte != 0 {
                break;
            }
        }
        zeros
    }
}

impl fmt::Debug for Hash256 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.to_display_bytes() {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// SHA-256 applied twice, which is how Bitcoin hashes a header and a transaction.
///
/// Hashing twice was Satoshi's answer to length-extension attacks on a single SHA-256.
pub fn double_sha256(data: &[u8]) -> Hash256 {
    let first = Sha256::digest(data);
    let second = Sha256::digest(first);
    let mut out = [0u8; 32];
    out.copy_from_slice(&second);
    Hash256(out)
}

/// The root of the merkle tree over these leaves, in Bitcoin's shape.
///
/// Pairs are hashed together with double SHA-256 until one hash is left. When a level holds an odd
/// number of hashes the last one is paired with itself, exactly as Bitcoin does it. An empty tree
/// has an all-zero root.
pub fn merkle_root(leaves: &[Hash256]) -> Hash256 {
    match leaves {
        [] => Hash256::ZERO,
        [only] => *only,
        _ => {
            let mut level: Vec<Hash256> = leaves.to_vec();
            let mut pair = [0u8; 64];
            while level.len() > 1 {
                let mut next = Vec::with_capacity(level.len().div_ceil(2));
                for chunk in level.chunks(2) {
                    let left = chunk[0];
                    let right = if chunk.len() == 2 { chunk[1] } else { chunk[0] };
                    pair[..32].copy_from_slice(left.as_bytes());
                    pair[32..].copy_from_slice(right.as_bytes());
                    next.push(double_sha256(&pair));
                }
                level = next;
            }
            level.first().copied().unwrap_or(Hash256::ZERO)
        }
    }
}

/// A SHA-256 state that has already absorbed the first 64 bytes of a header.
///
/// The first 64 of a header's 80 bytes never change while a miner rolls the nonce, so the first
/// compression round can be done once and reused. This is the same trick real miners call a
/// midstate; it cuts the work per nonce from three compression rounds to two.
#[derive(Clone)]
pub struct HeaderMidstate {
    prefix: Sha256,
}

impl HeaderMidstate {
    /// Absorbs the fixed first 64 bytes of an 80-byte header.
    pub fn new(header: &[u8; 80]) -> Self {
        let mut prefix = Sha256::new();
        prefix.update(&header[..64]);
        HeaderMidstate { prefix }
    }

    /// Finishes the double hash for a header whose last 16 bytes are `tail`.
    pub fn finish(&self, tail: &[u8; 16]) -> Hash256 {
        let mut first = self.prefix.clone();
        first.update(tail);
        let second = Sha256::digest(first.finalize());
        let mut out = [0u8; 32];
        out.copy_from_slice(&second);
        Hash256(out)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn double_sha256_matches_a_known_vector() {
        // SHA-256 of the empty input, hashed again: the digest as the hash function hands it
        // over, which is the order these bytes go into the next header.
        let got = double_sha256(b"");
        let hex: String = got.as_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(hex, "5df6e0e2761359d30a8275058e299fcc0381534545f55cf43e41983f5d4c9456");
        // The other way round is the order a block explorer would print it.
        let display: String =
            got.to_display_bytes().iter().map(|b| format!("{b:02x}")).collect::<String>();
        assert_eq!(display, "56944c5d3f98413ef45cf54545538103cc9f298e0575820ad3591376e2e0f65d");
    }

    #[test]
    fn a_single_leaf_is_its_own_root() {
        let leaf = double_sha256(b"one transaction");
        assert_eq!(merkle_root(&[leaf]), leaf);
    }

    #[test]
    fn an_odd_level_duplicates_its_last_leaf() {
        let a = double_sha256(b"a");
        let b = double_sha256(b"b");
        let c = double_sha256(b"c");
        // Three leaves: (a,b) and (c,c), then those two.
        let mut pair = [0u8; 64];
        pair[..32].copy_from_slice(a.as_bytes());
        pair[32..].copy_from_slice(b.as_bytes());
        let ab = double_sha256(&pair);
        pair[..32].copy_from_slice(c.as_bytes());
        pair[32..].copy_from_slice(c.as_bytes());
        let cc = double_sha256(&pair);
        pair[..32].copy_from_slice(ab.as_bytes());
        pair[32..].copy_from_slice(cc.as_bytes());
        assert_eq!(merkle_root(&[a, b, c]), double_sha256(&pair));
    }

    #[test]
    fn changing_one_leaf_changes_the_root() {
        let a = double_sha256(b"a");
        let b = double_sha256(b"b");
        let c = double_sha256(b"c");
        assert_ne!(merkle_root(&[a, b, c]), merkle_root(&[a, b, b]));
    }

    #[test]
    fn the_midstate_gives_the_same_hash_as_hashing_all_80_bytes() {
        let mut header = [0u8; 80];
        for (i, slot) in header.iter_mut().enumerate() {
            *slot = (i as u8).wrapping_mul(7).wrapping_add(3);
        }
        let midstate = HeaderMidstate::new(&header);
        let mut tail = [0u8; 16];
        tail.copy_from_slice(&header[64..]);
        assert_eq!(midstate.finish(&tail), double_sha256(&header));
    }

    #[test]
    fn leading_zero_bits_reads_the_number_the_way_bitcoin_does() {
        let mut bytes = [0u8; 32];
        bytes[31] = 0x00;
        bytes[30] = 0x0f;
        // The top byte is zero and the next one starts with four zero bits.
        assert_eq!(Hash256::from_bytes(bytes).leading_zero_bits(), 12);
        assert_eq!(Hash256::ZERO.leading_zero_bits(), 256);
    }
}
