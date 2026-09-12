//! The group and field the first three stages work in, and the hashes that bind their transcripts.
//!
//! Everything here comes from `pasta_curves` as re-exported by `halo2_proofs`, so the curve the
//! sigma protocol runs on and the curve the halo2 circuit is proved over cannot drift apart into
//! two different versions of the same crate.

use halo2_proofs::pasta::group::ff::{Field, FromUniformBytes, PrimeField};
use halo2_proofs::pasta::group::{Group, GroupEncoding};
use halo2_proofs::pasta::pallas;
use sha2::{Digest, Sha256, Sha512};

use crate::ZkError;
use crate::rng::DeterministicRng;

/// The scalar field of Pallas: the field secret keys, nonces and challenges live in.
pub type Scalar = pallas::Scalar;

/// A point of the Pallas group.
pub type Point = pallas::Point;

/// Domain separators. Two hashes over the same bytes in different roles must not collide, so every
/// hash in this crate starts with the byte that says which role it is.
pub mod domain {
    /// The challenge of the interactive sigma protocol.
    pub const SIGMA_CHALLENGE: u8 = 0x01;
    /// A Fiat-Shamir challenge bound to the statement only — the broken binding.
    pub const FS_WEAK: u8 = 0x02;
    /// A Fiat-Shamir challenge bound to generator, statement, commitment and context.
    pub const FS_FULL: u8 = 0x03;
    /// A note commitment in the shielded payment scenario.
    pub const NOTE: u8 = 0x04;
    /// A nullifier in the shielded payment scenario.
    pub const NULLIFIER: u8 = 0x05;
    /// A digest of a proof, so an onlooker can point at one without holding it.
    pub const PROOF_DIGEST: u8 = 0x06;
    /// A shielded address in the scenario.
    pub const ADDRESS: u8 = 0x07;
}

/// The group generator every statement in stages 1 to 3 is written against.
pub fn generator() -> Point {
    Point::generator()
}

/// A point as the 32 bytes that travel on a wire.
pub fn encode_point(point: &Point) -> [u8; 32] {
    point.to_bytes()
}

/// A point read back from the wire. Not every 32 bytes are a point.
pub fn decode_point(bytes: &[u8; 32]) -> Result<Point, ZkError> {
    Option::from(Point::from_bytes(bytes)).ok_or(ZkError::BadPointEncoding)
}

/// A scalar as the 32 bytes that travel on a wire.
pub fn encode_scalar(scalar: &Scalar) -> [u8; 32] {
    scalar.to_repr()
}

/// A scalar read back from the wire. Not every 32 bytes are below the field modulus.
pub fn decode_scalar(bytes: &[u8; 32]) -> Result<Scalar, ZkError> {
    Option::from(Scalar::from_repr(*bytes)).ok_or(ZkError::BadScalarEncoding)
}

/// A uniform scalar from the run's generator.
pub fn random_scalar(rng: &mut DeterministicRng) -> Scalar {
    Scalar::from_uniform_bytes(&rng.wide())
}

/// A scalar derived from a transcript: the whole of Fiat-Shamir in one function.
///
/// Each part is length-prefixed so that two different splits of the same bytes cannot produce the
/// same challenge — the failure this crate spends stage 2 demonstrating is about what is left out
/// of `parts`, not about how the parts are joined.
pub fn hash_to_scalar(domain: u8, parts: &[&[u8]]) -> Scalar {
    let mut hasher = Sha512::new();
    hasher.update([domain]);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let out = hasher.finalize();
    let mut wide = [0u8; 64];
    wide.copy_from_slice(&out);
    Scalar::from_uniform_bytes(&wide)
}

/// A 32-byte digest over a domain-separated, length-prefixed transcript.
pub fn digest(domain: u8, parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update([domain]);
    for part in parts {
        hasher.update((part.len() as u64).to_le_bytes());
        hasher.update(part);
    }
    let out = hasher.finalize();
    let mut bytes = [0u8; 32];
    bytes.copy_from_slice(&out);
    bytes
}

/// The scalar `value` stands for when a whole number is committed to.
pub fn scalar_from_u64(value: u64) -> Scalar {
    Scalar::from(value)
}

/// The multiplicative inverse of a scalar, or `None` for zero.
pub fn invert(scalar: &Scalar) -> Option<Scalar> {
    Option::from(scalar.invert())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;

    #[test]
    fn points_survive_a_round_trip() {
        let mut rng = DeterministicRng::new(Seed::fixed());
        let point = generator() * random_scalar(&mut rng);
        let bytes = encode_point(&point);
        assert_eq!(decode_point(&bytes).expect("round trip"), point);
    }

    #[test]
    fn scalars_survive_a_round_trip() {
        let mut rng = DeterministicRng::new(Seed::fixed());
        let scalar = random_scalar(&mut rng);
        let bytes = encode_scalar(&scalar);
        assert_eq!(decode_scalar(&bytes).expect("round trip"), scalar);
    }

    #[test]
    fn the_domain_byte_changes_the_challenge() {
        let a = hash_to_scalar(domain::FS_WEAK, &[b"same input"]);
        let b = hash_to_scalar(domain::FS_FULL, &[b"same input"]);
        assert_ne!(a, b);
    }

    #[test]
    fn length_prefixes_keep_neighbouring_parts_apart() {
        let a = hash_to_scalar(domain::FS_FULL, &[b"ab", b"c"]);
        let b = hash_to_scalar(domain::FS_FULL, &[b"a", b"bc"]);
        assert_ne!(a, b);
    }
}
