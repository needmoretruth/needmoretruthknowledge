//! The one source of randomness in this crate, and the bridge between two `rand_core` versions.
//!
//! Every stage takes an explicit generator so a run is reproducible: the same seed replays the same
//! commitments, the same challenges and the same proof bytes, which is what lets a reader stop on a
//! step and compare it with the page. `halo2_proofs` 0.3 asks for a generator through the
//! `rand_core` 0.6 traits while the workspace is on `rand` 0.10, so this type speaks both.

use rand::SeedableRng;
use rand::rngs::StdRng;
use sha2::{Digest, Sha256};

/// The seed a run uses when the caller does not pick one.
pub const DEFAULT_SEED: [u8; 32] = *b"nmtk-zk deterministic seed 00001";

/// The 32 bytes every random value in a run is derived from.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Seed(pub [u8; 32]);

impl Seed {
    /// The fixed seed. Two runs of the same code with this seed agree byte for byte.
    pub const fn fixed() -> Self {
        Seed(DEFAULT_SEED)
    }

    /// A seed a screen can offer behind a number box, stretched over the whole 32 bytes.
    pub fn from_u64(n: u64) -> Self {
        let mut hasher = Sha256::new();
        hasher.update(DEFAULT_SEED);
        hasher.update(n.to_le_bytes());
        let out = hasher.finalize();
        let mut seed = [0u8; 32];
        seed.copy_from_slice(&out);
        Seed(seed)
    }

    /// The seed bytes, so a screen can show which run it is looking at.
    pub fn bytes(&self) -> [u8; 32] {
        self.0
    }
}

impl Default for Seed {
    fn default() -> Self {
        Self::fixed()
    }
}

impl core::fmt::Debug for Seed {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Seed").finish_non_exhaustive()
    }
}

/// A seeded ChaCha generator, usable by both `rand` 0.10 and `halo2_proofs` 0.3.
pub struct DeterministicRng {
    inner: StdRng,
}

impl DeterministicRng {
    /// Starts a run from a seed.
    pub fn new(seed: Seed) -> Self {
        Self { inner: StdRng::from_seed(seed.0) }
    }

    /// Fills a buffer with the next bytes of the stream.
    pub fn fill(&mut self, dst: &mut [u8]) {
        rand::Rng::fill_bytes(&mut self.inner, dst);
    }

    /// 64 uniform bytes, the width a field element is sampled from without bias.
    pub fn wide(&mut self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        self.fill(&mut buf);
        buf
    }
}

impl core::fmt::Debug for DeterministicRng {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("DeterministicRng").finish_non_exhaustive()
    }
}

impl rand_core::RngCore for DeterministicRng {
    fn next_u32(&mut self) -> u32 {
        rand::Rng::next_u32(&mut self.inner)
    }

    fn next_u64(&mut self) -> u64 {
        rand::Rng::next_u64(&mut self.inner)
    }

    fn fill_bytes(&mut self, dst: &mut [u8]) {
        rand::Rng::fill_bytes(&mut self.inner, dst);
    }

    fn try_fill_bytes(&mut self, dst: &mut [u8]) -> Result<(), rand_core::Error> {
        rand::Rng::fill_bytes(&mut self.inner, dst);
        Ok(())
    }
}

impl rand_core::CryptoRng for DeterministicRng {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_same_seed_replays_the_same_bytes() {
        let mut a = DeterministicRng::new(Seed::fixed());
        let mut b = DeterministicRng::new(Seed::fixed());
        assert_eq!(a.wide(), b.wide());
    }

    #[test]
    fn different_seeds_diverge() {
        let mut a = DeterministicRng::new(Seed::from_u64(1));
        let mut b = DeterministicRng::new(Seed::from_u64(2));
        assert_ne!(a.wide(), b.wide());
    }
}
