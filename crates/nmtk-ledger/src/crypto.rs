//! Addresses, hashing, and a stand-in for signatures.
//!
//! The three ledger models differ in what they store and when they check it, not in which
//! signature scheme they use, so this crate keeps the signature deliberately simple and spends
//! its exactness on the state machines instead.
//!
//! What the stand-in is: a "signature" carries the signing key itself plus
//! `SHA-256(domain || key || message)`. Verifying means hashing the carried key to an address,
//! comparing that address with the owner recorded in state, and recomputing the tag over the
//! message. That reproduces the *check a ledger performs* — the spender must present a key whose
//! hash is the address written into the output, and the tag pins the exact message that was
//! signed — but it is not a real signature: the key travels in the clear, so anyone who has seen
//! one spend could sign the next one. Real chains use ECDSA (Bitcoin, Ethereum) or Ed25519 (Sui),
//! where the public key and the secret key are different values and the secret never moves.

use sha2::{Digest, Sha256};

const DOMAIN_KEY: &[u8] = b"nmtk.ledger.key.v1";
const DOMAIN_ADDRESS: &[u8] = b"nmtk.ledger.address.v1";
const DOMAIN_SIGNATURE: &[u8] = b"nmtk.ledger.signature.v1";

/// SHA-256 over the parts, in order.
pub(crate) fn hash(parts: &[&[u8]]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part);
    }
    hasher.finalize().into()
}

/// Appends a length prefix so that two different lists never encode to the same bytes.
///
/// `usize` is cast to `u64`: every target this program builds for has a pointer 64 bits wide or
/// narrower, so the cast cannot lose a digit.
pub(crate) fn put_len(buffer: &mut Vec<u8>, len: usize) {
    buffer.extend_from_slice(&(len as u64).to_be_bytes());
}

/// How many bytes an address occupies. Twenty, as on Bitcoin and Ethereum.
pub const ADDRESS_BYTES: usize = 20;

/// Who owns something: the first 20 bytes of the hash of a key.
///
/// An address carries no name. The screen decides what to call it.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Debug, Default)]
pub struct Address([u8; ADDRESS_BYTES]);

impl Address {
    /// Wraps bytes that are already an address.
    pub const fn from_bytes(bytes: [u8; ADDRESS_BYTES]) -> Self {
        Self(bytes)
    }

    /// The address of the key grown from this seed.
    ///
    /// Useful where a screen wants a recipient it will never spend from.
    pub fn from_seed(seed: &[u8]) -> Self {
        Key::from_seed(seed).address()
    }

    /// The raw bytes, for a screen that wants to show a short prefix.
    pub const fn as_bytes(&self) -> &[u8; ADDRESS_BYTES] {
        &self.0
    }
}

/// A signing key. Holding one is what lets a transfer be signed.
///
/// Grown from a seed so that a scenario is reproducible: the same seed always gives the same key
/// and therefore the same address, run after run.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Key([u8; 32]);

impl Key {
    /// Derives a key from arbitrary bytes.
    pub fn from_seed(seed: &[u8]) -> Self {
        Self(hash(&[DOMAIN_KEY, seed]))
    }

    /// Wraps bytes that are already a key.
    pub const fn from_bytes(bytes: [u8; 32]) -> Self {
        Self(bytes)
    }

    /// The address this key controls.
    pub fn address(&self) -> Address {
        address_of(&self.0)
    }

    /// Signs a 32-byte message. See the module note for what this stand-in does and does not do.
    pub fn sign(&self, message: &[u8; 32]) -> Signature {
        Signature { key: self.0, tag: hash(&[DOMAIN_SIGNATURE, &self.0, message]) }
    }
}

impl core::fmt::Debug for Key {
    /// Shows the address rather than the key bytes, so a debug print of a whole transaction does
    /// not scatter signing keys through a log.
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Key").field("address", &self.address()).finish_non_exhaustive()
    }
}

fn address_of(key: &[u8; 32]) -> Address {
    let digest = hash(&[DOMAIN_ADDRESS, key]);
    // Every index is below 20, which is below the digest's 32, so no bound can be missed.
    Address(core::array::from_fn(|i| digest[i]))
}

/// A stand-in signature: the signing key plus a tag over the message.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Signature {
    key: [u8; 32],
    tag: [u8; 32],
}

impl Signature {
    /// Bytes one signature occupies on the wire, for size arithmetic elsewhere.
    pub const SIZE_BYTES: u64 = 64;

    /// The address of whoever produced this signature.
    ///
    /// A real scheme recovers or carries a public key here; this one carries the key itself.
    pub fn signer(&self) -> Address {
        address_of(&self.key)
    }

    /// Whether the tag matches this exact message. False the moment one byte of the transfer
    /// changes after signing.
    pub fn covers(&self, message: &[u8; 32]) -> bool {
        hash(&[DOMAIN_SIGNATURE, &self.key, message]) == self.tag
    }

    /// Both halves of the check a ledger runs: right signer, right message.
    pub fn verify(&self, owner: &Address, message: &[u8; 32]) -> bool {
        self.signer() == *owner && self.covers(message)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_key_signs_for_its_own_address_and_no_other() {
        let alice = Key::from_seed(b"alice");
        let mallory = Key::from_seed(b"mallory");
        let message = [7u8; 32];
        let signature = alice.sign(&message);

        assert!(signature.verify(&alice.address(), &message));
        assert!(!signature.verify(&mallory.address(), &message));
        assert_eq!(signature.signer(), alice.address());
    }

    #[test]
    fn changing_the_message_breaks_the_signature() {
        let alice = Key::from_seed(b"alice");
        let signature = alice.sign(&[1u8; 32]);
        assert!(!signature.covers(&[2u8; 32]));
    }

    #[test]
    fn the_same_seed_always_gives_the_same_address() {
        assert_eq!(Key::from_seed(b"alice").address(), Address::from_seed(b"alice"));
        assert_ne!(Address::from_seed(b"alice"), Address::from_seed(b"bob"));
    }
}
