//! Stage 2 — the same proof with the verifier's coin replaced by a hash of the transcript.
//!
//! Nothing about the algebra changes. What changes is where the challenge comes from: instead of a
//! verifier drawing it live, the prover derives it from a hash. That one substitution turns a
//! conversation into a file anyone can check, forever, with nobody online.
//!
//! It is also where the classic implementation mistake lives. If the hash does not cover the
//! commitment, the challenge is known before the commitment is chosen, and the commitment can be
//! solved for instead of drawn. [`Binding::StatementOnly`] does exactly that and is forgeable here;
//! [`Binding::Full`] covers generator, statement, commitment and context, and the same attack fails.

use halo2_proofs::pasta::group::Curve;

use crate::ZkError;
use crate::curve::{self, Point, Scalar, domain};
use crate::rng::DeterministicRng;
use crate::sigma::{Statement, Witness};

/// Bytes a non-interactive proof takes: commitment and response, no challenge.
pub const PROOF_BYTES: usize = 64;

/// How much of the transcript the challenge hash covers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Binding {
    /// Statement and context only — the commitment is left out. Forgeable.
    StatementOnly,
    /// Generator, statement, commitment and context. Sound.
    Full,
}

/// A non-interactive proof: the two messages the prover would have sent, with the verifier's
/// message recomputed by anyone who reads them.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Proof {
    /// `R`, 32 bytes.
    pub commitment: [u8; 32],
    /// `s`, 32 bytes.
    pub response: [u8; 32],
}

impl Proof {
    /// The proof as it would sit in a block.
    pub fn to_bytes(&self) -> [u8; PROOF_BYTES] {
        let mut out = [0u8; PROOF_BYTES];
        out[..32].copy_from_slice(&self.commitment);
        out[32..].copy_from_slice(&self.response);
        out
    }
}

/// The challenge a verifier will recompute, given what this binding covers.
pub fn challenge(
    binding: Binding,
    statement: &Statement,
    commitment: &Point,
    context: &[u8],
) -> Scalar {
    let public_key = statement.public_key();
    match binding {
        Binding::StatementOnly => curve::hash_to_scalar(domain::FS_WEAK, &[&public_key, context]),
        Binding::Full => {
            let generator = curve::encode_point(&curve::generator());
            let commitment = curve::encode_point(commitment);
            curve::hash_to_scalar(domain::FS_FULL, &[&generator, &public_key, &commitment, context])
        }
    }
}

/// An honest proof. The prover draws a nonce, commits, hashes, and answers its own challenge.
pub fn prove(
    witness: &Witness,
    binding: Binding,
    context: &[u8],
    rng: &mut DeterministicRng,
) -> Proof {
    let statement = witness.statement();
    let nonce = curve::random_scalar(rng);
    let commitment = curve::generator() * nonce;
    let c = challenge(binding, &statement, &commitment, context);
    let response = nonce + c * witness.scalar();
    Proof {
        commitment: curve::encode_point(&commitment),
        response: curve::encode_scalar(&response),
    }
}

/// Checks a proof the way anyone reading a chain would: recompute the challenge from the bytes in
/// front of you, then check `s*G == R + c*P`.
pub fn verify(
    statement: &Statement,
    proof: &Proof,
    binding: Binding,
    context: &[u8],
) -> Result<bool, ZkError> {
    let commitment = curve::decode_point(&proof.commitment)?;
    let response = curve::decode_scalar(&proof.response)?;
    let c = challenge(binding, statement, &commitment, context);
    let left = curve::generator() * response;
    let right = commitment + statement.point() * c;
    Ok(left.to_affine() == right.to_affine())
}

/// The forgery, written once and run against both bindings.
///
/// The attacker computes a challenge first, picks the response it wants, and *solves* for the
/// commitment that makes the equation hold: `R = s*G - c*P`. Under [`Binding::StatementOnly`] the
/// challenge did not depend on `R`, so the verifier recomputes the same `c` and accepts a proof for
/// a statement whose secret the attacker never had. Under [`Binding::Full`] the challenge the
/// verifier recomputes covers the solved-for `R`, so it is a different challenge and the check
/// fails. Same attacker, same code, two outcomes.
pub fn forge(
    statement: &Statement,
    binding: Binding,
    context: &[u8],
    rng: &mut DeterministicRng,
) -> Proof {
    let decoy = curve::generator() * curve::random_scalar(rng);
    let c = challenge(binding, statement, &decoy, context);
    let response = curve::random_scalar(rng);
    let commitment = curve::generator() * response - statement.point() * c;
    Proof {
        commitment: curve::encode_point(&commitment),
        response: curve::encode_scalar(&response),
    }
}

/// Where a challenge comes from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ChallengeSource {
    /// A verifier draws it live, after seeing the commitment.
    VerifierCoin,
    /// A hash of the transcript, computable by anyone.
    TranscriptHash,
}

/// The shape of a protocol, as a screen puts two of them side by side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProtocolShape {
    /// Messages that cross the wire.
    pub messages: u8,
    /// Whether the verifier has to be present while the prover works.
    pub verifier_must_be_online: bool,
    /// Where the challenge comes from.
    pub challenge_source: ChallengeSource,
    /// Whether a third party can check the same bytes later.
    pub anyone_can_verify_later: bool,
    /// Bytes that have to be kept to re-check the proof.
    pub bytes_to_keep: usize,
}

/// The interactive shape of stage 1, as data.
pub const INTERACTIVE: ProtocolShape = ProtocolShape {
    messages: 3,
    verifier_must_be_online: true,
    challenge_source: ChallengeSource::VerifierCoin,
    anyone_can_verify_later: false,
    bytes_to_keep: crate::sigma::TRANSCRIPT_BYTES,
};

/// The non-interactive shape of stage 2, as data.
pub const NON_INTERACTIVE: ProtocolShape = ProtocolShape {
    messages: 1,
    verifier_must_be_online: false,
    challenge_source: ChallengeSource::TranscriptHash,
    anyone_can_verify_later: true,
    bytes_to_keep: PROOF_BYTES,
};

/// One forgery attempt against one binding.
#[derive(Clone, Copy, Debug)]
pub struct ForgeryRecord {
    /// Which binding the attacker was up against.
    pub binding: Binding,
    /// The bytes it produced.
    pub proof: Proof,
    /// What the verifier said.
    pub accepted: bool,
}

/// Everything stage 2 produces.
#[derive(Clone, Debug)]
pub struct Run {
    /// The public key both bindings are proved against.
    pub statement: [u8; 32],
    /// The context bytes the challenge is bound to — in a payment, the transaction being signed.
    pub context: Vec<u8>,
    /// The honest proof under the correct binding.
    pub proof: Proof,
    /// What the verifier said about it.
    pub accepted: bool,
    /// The attacker against the broken binding. This one is accepted.
    pub weak_forgery: ForgeryRecord,
    /// The same attacker against the correct binding. This one is not.
    pub strong_forgery: ForgeryRecord,
    /// The shape of stage 1, for the side-by-side.
    pub interactive: ProtocolShape,
    /// The shape of stage 2, for the side-by-side.
    pub non_interactive: ProtocolShape,
    /// Bytes of the honest proof.
    pub proof_bytes: usize,
    /// Time spent proving.
    pub prove_nanos: u64,
    /// Time spent verifying.
    pub verify_nanos: u64,
}

/// Runs stage 2 end to end: an honest proof under the correct binding, then the same forgery
/// attempted against both bindings.
pub fn run(context: &[u8], rng: &mut DeterministicRng) -> Result<Run, ZkError> {
    let witness = Witness::random(rng);
    let statement = witness.statement();

    let start_prove = std::time::Instant::now();
    let proof = prove(&witness, Binding::Full, context, rng);
    let prove_nanos = start_prove.elapsed().as_nanos() as u64;

    let start_verify = std::time::Instant::now();
    let accepted = verify(&statement, &proof, Binding::Full, context)?;
    let verify_nanos = start_verify.elapsed().as_nanos() as u64;

    let weak = forge(&statement, Binding::StatementOnly, context, rng);
    let weak_accepted = verify(&statement, &weak, Binding::StatementOnly, context)?;

    let strong = forge(&statement, Binding::Full, context, rng);
    let strong_accepted = verify(&statement, &strong, Binding::Full, context)?;

    Ok(Run {
        statement: statement.public_key(),
        context: context.to_vec(),
        proof,
        accepted,
        weak_forgery: ForgeryRecord {
            binding: Binding::StatementOnly,
            proof: weak,
            accepted: weak_accepted,
        },
        strong_forgery: ForgeryRecord {
            binding: Binding::Full,
            proof: strong,
            accepted: strong_accepted,
        },
        interactive: INTERACTIVE,
        non_interactive: NON_INTERACTIVE,
        proof_bytes: PROOF_BYTES,
        prove_nanos,
        verify_nanos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;

    const CONTEXT: &[u8] = b"nmtk-zk stage 2 test context";

    fn rng() -> DeterministicRng {
        DeterministicRng::new(Seed::fixed())
    }

    #[test]
    fn an_honest_proof_verifies_under_both_bindings() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let statement = witness.statement();
        for binding in [Binding::Full, Binding::StatementOnly] {
            let proof = prove(&witness, binding, CONTEXT, &mut rng);
            assert!(verify(&statement, &proof, binding, CONTEXT).expect("verify runs"));
        }
    }

    #[test]
    fn the_weak_binding_is_forgeable() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let statement = witness.statement();
        let forged = forge(&statement, Binding::StatementOnly, CONTEXT, &mut rng);
        assert!(verify(&statement, &forged, Binding::StatementOnly, CONTEXT).expect("verify runs"));
    }

    #[test]
    fn the_correct_binding_is_not() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let statement = witness.statement();
        let forged = forge(&statement, Binding::Full, CONTEXT, &mut rng);
        assert!(!verify(&statement, &forged, Binding::Full, CONTEXT).expect("verify runs"));
    }

    #[test]
    fn a_weak_forgery_does_not_survive_being_checked_correctly() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let statement = witness.statement();
        let forged = forge(&statement, Binding::StatementOnly, CONTEXT, &mut rng);
        assert!(!verify(&statement, &forged, Binding::Full, CONTEXT).expect("verify runs"));
    }

    #[test]
    fn the_full_binding_moves_with_the_commitment() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let statement = witness.statement();
        let one = curve::generator() * curve::random_scalar(&mut rng);
        let two = curve::generator() * curve::random_scalar(&mut rng);
        assert_ne!(
            challenge(Binding::Full, &statement, &one, CONTEXT),
            challenge(Binding::Full, &statement, &two, CONTEXT)
        );
        assert_eq!(
            challenge(Binding::StatementOnly, &statement, &one, CONTEXT),
            challenge(Binding::StatementOnly, &statement, &two, CONTEXT)
        );
    }

    #[test]
    fn changing_the_context_invalidates_a_proof() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let statement = witness.statement();
        let proof = prove(&witness, Binding::Full, CONTEXT, &mut rng);
        assert!(!verify(&statement, &proof, Binding::Full, b"a different payment").expect("runs"));
    }

    #[test]
    fn the_run_reports_both_outcomes() {
        let run = run(CONTEXT, &mut rng()).expect("stage 2 runs");
        assert!(run.accepted);
        assert!(run.weak_forgery.accepted);
        assert!(!run.strong_forgery.accepted);
        assert_eq!(run.proof_bytes, PROOF_BYTES);
    }
}
