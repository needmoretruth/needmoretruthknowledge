//! The number a block hash has to beat, and the compact form a header carries it in.
//!
//! A header has four bytes for difficulty, not thirty-two. Bitcoin packs the 256-bit target into
//! them as a floating-point-like value: one byte of exponent and three bytes of mantissa. Every
//! node unpacks the same four bytes the same way, so every node agrees on what counts as a block.

use core::fmt;

use crate::hash::Hash256;

/// The compact bits of difficulty 1, the target Bitcoin started with in January 2009.
///
/// Unpacked it is `0x00000000FFFF0000…0000`: a hash must be below that number to be a block,
/// which happens about once every 2^32 hashes.
pub const DIFFICULTY_ONE_BITS: u32 = 0x1d00_ffff;

/// 2^256, as a float. Every "how many hashes" figure in this crate divides it by a target.
const TWO_POW_256: f64 = 115_792_089_237_316_195_423_570_985_008_687_907_853_269_984_665_640_564_039_457_584_007_913_129_639_936.0;

/// What can be wrong with a target or with the four bytes that encode one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetError {
    /// The compact form sets the sign bit. Bitcoin has no negative targets.
    NegativeTarget,
    /// The mantissa is zero, so no hash could ever be below the target.
    ZeroTarget,
    /// The exponent shifts the mantissa past 256 bits.
    Overflow,
    /// A practice difficulty asked for 256 or more leading zero bits, which no hash can have.
    TooManyZeroBits,
}

impl fmt::Display for TargetError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let text = match self {
            TargetError::NegativeTarget => "compact target has the sign bit set",
            TargetError::ZeroTarget => "compact target has a zero mantissa",
            TargetError::Overflow => "compact target does not fit in 256 bits",
            TargetError::TooManyZeroBits => "a target cannot have 256 or more leading zero bits",
        };
        f.write_str(text)
    }
}

impl std::error::Error for TargetError {}

/// The threshold a block hash has to come in under, as a 256-bit number.
///
/// The bytes are kept most significant first, so comparing two targets is comparing their byte
/// arrays. A hash arrives in the opposite order and is turned around before the comparison.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Target([u8; 32]);

impl Target {
    /// The target difficulty 1 stands for: `0x00000000FFFF0000…0000`.
    pub fn difficulty_one() -> Target {
        let mut bytes = [0u8; 32];
        bytes[4] = 0xff;
        bytes[5] = 0xff;
        Target(bytes)
    }

    /// Unpacks the four `bits` bytes of a header into the full 256-bit target.
    ///
    /// The top byte is a length in bytes and the low three bytes are the mantissa, so the target
    /// is `mantissa * 256^(size - 3)`.
    pub fn from_compact(bits: u32) -> Result<Target, TargetError> {
        let size = (bits >> 24) as usize;
        let mantissa = bits & 0x007f_ffff;
        if bits & 0x0080_0000 != 0 {
            return Err(TargetError::NegativeTarget);
        }
        if mantissa == 0 {
            return Err(TargetError::ZeroTarget);
        }
        let mut bytes = [0u8; 32];
        if size <= 3 {
            // The mantissa is shifted down, losing its low bytes.
            let shifted = mantissa >> (8 * (3 - size));
            bytes[29..32].copy_from_slice(&shifted.to_be_bytes()[1..4]);
        } else {
            if size > 32 {
                return Err(TargetError::Overflow);
            }
            // The mantissa's three bytes sit so that the value is `size` bytes long.
            let start = 32 - size;
            let mantissa_bytes = mantissa.to_be_bytes();
            bytes[start..start + 3].copy_from_slice(&mantissa_bytes[1..4]);
        }
        Ok(Target(bytes))
    }

    /// Packs the target back into the four bytes a header carries.
    ///
    /// This is the canonical encoding: the mantissa never has its top bit set, because that bit
    /// means "negative".
    pub fn to_compact(self) -> u32 {
        let first_significant = self.0.iter().position(|b| *b != 0);
        let Some(first) = first_significant else {
            return 0;
        };
        let mut size = 32 - first;
        // Reading three bytes from the first significant one, with zeroes past the end, puts a
        // value shorter than three bytes at the top of the mantissa, which is where it belongs.
        let mut mantissa: u32 = 0;
        for offset in 0..3 {
            let index = first + offset;
            let byte = if index < 32 { self.0[index] } else { 0 };
            mantissa = (mantissa << 8) | u32::from(byte);
        }
        if mantissa & 0x0080_0000 != 0 {
            mantissa >>= 8;
            size += 1;
        }
        ((size as u32) << 24) | mantissa
    }

    /// A target with roughly this many leading zero bits, rounded to what the compact form can
    /// hold. Finding a block under it takes about `2^zero_bits` hashes.
    ///
    /// This is the practice difficulty: 20 to 26 zero bits gives a block every few seconds on a
    /// laptop, where difficulty 1 needs 32 and real Bitcoin today needs far more.
    pub fn from_leading_zero_bits(zero_bits: u32) -> Result<Target, TargetError> {
        Target::from_compact(practice_bits(zero_bits)?)
    }

    /// The raw bytes, most significant first.
    pub const fn to_be_bytes(self) -> [u8; 32] {
        self.0
    }

    /// Whether this hash counts as a block: the hash, read as a little-endian number, is at or
    /// below the target. Bitcoin accepts equality, and so does this.
    pub fn is_met_by(&self, hash: &Hash256) -> bool {
        hash.to_display_bytes() <= self.0
    }

    /// The target as a float. It loses the low bits of a 256-bit number, which is fine for every
    /// ratio this crate reports and wrong for anything that has to be exact.
    pub fn as_f64(self) -> f64 {
        let mut value = 0.0f64;
        for byte in self.0 {
            value = value * 256.0 + f64::from(byte);
        }
        value
    }

    /// Difficulty, the ratio Bitcoin quotes: how many times harder this is than difficulty 1.
    pub fn difficulty(self) -> f64 {
        let here = self.as_f64();
        if here <= 0.0 { f64::INFINITY } else { Target::difficulty_one().as_f64() / here }
    }

    /// How many hashes it takes on average to find one block under this target.
    ///
    /// This is `2^256 / (target + 1)`: the share of all possible hashes that win.
    pub fn expected_hashes(self) -> f64 {
        let here = self.as_f64();
        if here < 0.0 { f64::INFINITY } else { TWO_POW_256 / (here + 1.0) }
    }

    /// The work one block under this target represents, which is what chains are compared by.
    ///
    /// It is the expected hash count, rounded into an integer so that two chains mined at the
    /// same difficulty compare exactly by length.
    pub fn work(self) -> u128 {
        let hashes = self.expected_hashes();
        if !hashes.is_finite() || hashes <= 0.0 { 0 } else { hashes as u128 }
    }
}

impl fmt::Debug for Target {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for byte in self.0 {
            write!(f, "{byte:02x}")?;
        }
        Ok(())
    }
}

/// The compact bits for a practice target with about this many leading zero bits.
///
/// A header can only carry a target the compact form can express, so a practice difficulty has to
/// go through this rather than being an arbitrary 256-bit number.
pub fn practice_bits(zero_bits: u32) -> Result<u32, TargetError> {
    if zero_bits >= 256 {
        return Err(TargetError::TooManyZeroBits);
    }
    // Every bit below the leading zeroes is set: the largest target with that many zero bits.
    let mut bytes = [0u8; 32];
    for bit in zero_bits..256 {
        let index = (bit / 8) as usize;
        let mask = 0x80u8 >> (bit % 8);
        bytes[index] |= mask;
    }
    let bits = Target(bytes).to_compact();
    // A target so large that its compact form overflows is no use to anyone: reject it here
    // rather than hand back four bytes no header could be validated against.
    Target::from_compact(bits)?;
    Ok(bits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn difficulty_one_encodes_to_the_bits_bitcoin_started_with() {
        assert_eq!(Target::difficulty_one().to_compact(), DIFFICULTY_ONE_BITS);
        assert_eq!(
            Target::from_compact(DIFFICULTY_ONE_BITS).expect("canonical bits"),
            Target::difficulty_one()
        );
        assert!((Target::difficulty_one().difficulty() - 1.0).abs() < 1e-12);
    }

    #[test]
    fn compact_bits_round_trip() {
        // A spread of exponents and mantissas, including ones that need the shift-down rule.
        for bits in [
            0x1d00_ffffu32,
            0x1b04_864c,
            0x1e0f_ffff,
            0x2000_ffff,
            0x1707_2d1e,
            0x1900_896c,
            0x0500_9234,
            0x0201_0000,
            0x0101_0000,
        ] {
            let target = Target::from_compact(bits).expect("canonical bits");
            assert_eq!(target.to_compact(), bits, "bits {bits:#010x} did not survive the trip");
        }
    }

    #[test]
    fn a_non_canonical_encoding_decodes_the_same_and_re_encodes_canonically() {
        // 0x03000001 and 0x01010000 are both the number 1. Only the second is the form a node
        // would write, and a header carrying the first still has to be read the same way.
        let sloppy = Target::from_compact(0x0300_0001).expect("decodable");
        let canonical = Target::from_compact(0x0101_0000).expect("canonical");
        assert_eq!(sloppy, canonical);
        assert_eq!(sloppy.to_compact(), 0x0101_0000);
    }

    #[test]
    fn a_mantissa_with_its_top_bit_set_moves_up_an_exponent() {
        // 0x00ffff00 as a 3-byte value would encode as 0x03ffff00, whose top mantissa bit means
        // "negative". The canonical form is one byte longer with the mantissa shifted down.
        let mut bytes = [0u8; 32];
        bytes[29] = 0xff;
        bytes[30] = 0xff;
        let target = Target(bytes);
        assert_eq!(target.to_compact(), 0x0400_ffff);
        assert_eq!(Target::from_compact(0x0400_ffff).expect("canonical"), target);
    }

    #[test]
    fn the_sign_bit_and_a_zero_mantissa_are_refused() {
        assert_eq!(Target::from_compact(0x1d80_ffff), Err(TargetError::NegativeTarget));
        assert_eq!(Target::from_compact(0x1d00_0000), Err(TargetError::ZeroTarget));
        assert_eq!(Target::from_compact(0x2100_ffff), Err(TargetError::Overflow));
    }

    #[test]
    fn a_hash_exactly_on_the_target_counts_and_one_step_over_does_not() {
        let target = Target::from_compact(0x1d00_ffff).expect("canonical bits");
        let mut display = target.to_be_bytes();
        // A hash is stored the other way round from the target's byte order.
        let mut on_the_nose = display;
        on_the_nose.reverse();
        assert!(target.is_met_by(&Hash256::from_bytes(on_the_nose)));

        // One more than the target, big-endian, then turned into hash order.
        display[31] = 1;
        let mut just_over = display;
        just_over.reverse();
        assert!(!target.is_met_by(&Hash256::from_bytes(just_over)));

        // One less than the target is comfortably inside.
        let mut under = target.to_be_bytes();
        under[5] = 0xfe;
        under.reverse();
        assert!(target.is_met_by(&Hash256::from_bytes(under)));
    }

    #[test]
    fn expected_hashes_doubles_with_every_extra_zero_bit() {
        let twenty = Target::from_leading_zero_bits(20).expect("20 bits");
        let twenty_one = Target::from_leading_zero_bits(21).expect("21 bits");
        let ratio = twenty_one.expected_hashes() / twenty.expected_hashes();
        assert!((ratio - 2.0).abs() < 0.01, "ratio was {ratio}");
        // Difficulty 1 is a 32-zero-bit target by construction.
        let ones = Target::difficulty_one().expected_hashes();
        assert!((ones / 4_294_967_296.0 - 1.0).abs() < 1e-4, "difficulty 1 took {ones} hashes");
    }

    #[test]
    fn practice_bits_are_a_target_a_header_can_carry() {
        for zero_bits in [1u32, 8, 20, 24, 32, 64, 200, 255] {
            let bits = practice_bits(zero_bits).expect("expressible");
            let target = Target::from_compact(bits).expect("canonical");
            assert_eq!(target.to_compact(), bits);
            let hashes_log2 = target.expected_hashes().log2();
            assert!(
                (hashes_log2 - f64::from(zero_bits)).abs() <= 1.0,
                "asked for {zero_bits} zero bits, target costs 2^{hashes_log2} hashes"
            );
        }
        assert_eq!(practice_bits(256), Err(TargetError::TooManyZeroBits));
    }

    #[test]
    fn harder_targets_are_smaller_numbers_and_carry_more_work() {
        let easy = Target::from_leading_zero_bits(20).expect("20");
        let hard = Target::from_leading_zero_bits(28).expect("28");
        assert!(hard < easy);
        assert!(hard.work() > easy.work());
        assert!(hard.difficulty() > easy.difficulty());
    }
}
