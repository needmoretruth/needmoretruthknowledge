//! Stage 3 — an argument whose soundness rests on a number nobody is supposed to keep.
//!
//! The ceremony produces a second generator `H = tau*G`. The argument is a value commitment,
//! `C = value*G + blinding*H`, and the proof of "this commitment holds `value`" is its opening.
//! The commitment binds only because nobody knows the discrete logarithm of `H` with respect to
//! `G`. That logarithm is `tau`, the toxic waste, and it is a real [`ToxicWaste`] value in this
//! module: whoever holds it can reopen any commitment at any value they like, and the verifier —
//! the same verifier, unchanged — accepts.
//!
//! `tau` is never held by one party. Each participant contributes a factor and only sees the point
//! that comes out; the waste exists only if every single contribution is kept and they are combined.
//! One participant who really destroys their factor is enough, which is the whole reason ceremonies
//! are run with many people.

use halo2_proofs::pasta::group::Curve;
use halo2_proofs::pasta::group::ff::Field;

use crate::ZkError;
use crate::curve::{self, Point, Scalar};
use crate::rng::DeterministicRng;

/// Bytes an opening takes: the value and its blinding factor.
pub const PROOF_BYTES: usize = 40;

/// How many participants the module runs a ceremony with by default.
pub const DEFAULT_PARTICIPANTS: usize = 3;

/// One participant's factor of `tau`. Held by that participant and by nobody else.
#[derive(Clone)]
pub struct Contribution {
    index: usize,
    factor: Scalar,
}

impl Contribution {
    /// Which participant this is.
    pub fn index(&self) -> usize {
        self.index
    }

    /// The point this participant handed on, which is all anyone else ever saw of it.
    pub fn public_point(&self) -> [u8; 32] {
        curve::encode_point(&(curve::generator() * self.factor))
    }

    /// Destroys the factor. Taking `self` by value is the whole mechanism: after this call the
    /// caller no longer has it, which is the promise every participant makes.
    pub fn destroy(self) {}
}

impl core::fmt::Debug for Contribution {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Contribution").field("index", &self.index).finish_non_exhaustive()
    }
}

/// The public output of the ceremony. Everyone gets this; it is safe for everyone to have.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Parameters {
    g: Point,
    h: Point,
    participants: usize,
}

impl Parameters {
    /// The base generator.
    pub fn g(&self) -> [u8; 32] {
        curve::encode_point(&self.g)
    }

    /// The generator the ceremony produced.
    pub fn h(&self) -> [u8; 32] {
        curve::encode_point(&self.h)
    }

    /// How many factors went into `H`.
    pub fn participants(&self) -> usize {
        self.participants
    }
}

/// The number the ceremony is supposed to destroy: the discrete logarithm of `H` base `G`.
///
/// Deliberately neither `Copy` nor `Clone`: [`ToxicWaste::destroy`] has to be the end of it.
pub struct ToxicWaste {
    tau: Scalar,
}

impl ToxicWaste {
    /// Rebuilds the waste from every contribution. Missing even one gives `None`, and no amount of
    /// the others makes up for it.
    pub fn from_all_contributions(
        parameters: &Parameters,
        contributions: &[Contribution],
    ) -> Option<Self> {
        if contributions.len() != parameters.participants {
            return None;
        }
        let mut seen = vec![false; parameters.participants];
        let mut tau = Scalar::ONE;
        for contribution in contributions {
            let slot = seen.get_mut(contribution.index)?;
            if *slot {
                return None;
            }
            *slot = true;
            tau *= contribution.factor;
        }
        if (curve::generator() * tau).to_affine() != parameters.h.to_affine() {
            return None;
        }
        Some(Self { tau })
    }

    /// The waste as bytes, so a screen can show that it is a real number and not a figure of speech.
    pub fn bytes(&self) -> [u8; 32] {
        curve::encode_scalar(&self.tau)
    }

    /// Destroys the waste. Taking `self` by value is the whole mechanism.
    pub fn destroy(self) {}
}

impl core::fmt::Debug for ToxicWaste {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("ToxicWaste").finish_non_exhaustive()
    }
}

/// Runs the ceremony. Each participant multiplies the running point by a factor of their own and
/// passes it on; the coordinator never sees a factor.
pub fn run_ceremony(
    participants: usize,
    rng: &mut DeterministicRng,
) -> Result<(Parameters, Vec<Contribution>), ZkError> {
    if participants == 0 {
        return Err(ZkError::CeremonyEmpty);
    }
    let g = curve::generator();
    let mut h = g;
    let mut contributions = Vec::with_capacity(participants);
    for index in 0..participants {
        let factor = nonzero_scalar(rng)?;
        h *= factor;
        contributions.push(Contribution { index, factor });
    }
    Ok((Parameters { g, h, participants }, contributions))
}

fn nonzero_scalar(rng: &mut DeterministicRng) -> Result<Scalar, ZkError> {
    for _ in 0..16 {
        let candidate = curve::random_scalar(rng);
        if !bool::from(candidate.is_zero()) {
            return Ok(candidate);
        }
    }
    Err(ZkError::ToxicWasteNotInvertible)
}

/// A value commitment: the statement an opening is a proof of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ValueCommitment {
    point: Point,
}

impl ValueCommitment {
    /// The 32 bytes that go on a chain.
    pub fn bytes(&self) -> [u8; 32] {
        curve::encode_point(&self.point)
    }
}

/// The proof: "this commitment holds this value, with this blinding factor".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Opening {
    /// The value claimed.
    pub value: u64,
    /// The blinding factor that makes the claim check out.
    pub blinding: [u8; 32],
}

impl Opening {
    /// The opening as it would travel.
    pub fn to_bytes(&self) -> [u8; PROOF_BYTES] {
        let mut out = [0u8; PROOF_BYTES];
        out[..8].copy_from_slice(&self.value.to_le_bytes());
        out[8..].copy_from_slice(&self.blinding);
        out
    }
}

/// Commits to a value: `C = value*G + blinding*H`.
pub fn commit(parameters: &Parameters, value: u64, blinding: &Scalar) -> ValueCommitment {
    let point = parameters.g * curve::scalar_from_u64(value) + parameters.h * blinding;
    ValueCommitment { point }
}

/// Checks an opening. This is the only verifier in this stage, and every party below uses it
/// unchanged.
pub fn verify_opening(
    parameters: &Parameters,
    commitment: &ValueCommitment,
    opening: &Opening,
) -> Result<bool, ZkError> {
    let blinding = curve::decode_scalar(&opening.blinding)?;
    let rebuilt = commit(parameters, opening.value, &blinding);
    Ok(rebuilt.point.to_affine() == commitment.point.to_affine())
}

/// The attack that works. Holding `tau`, shift the blinding by `(value - claimed) / tau` and the
/// same commitment now opens to whatever `claimed` you asked for.
pub fn forge_opening_with_toxic_waste(
    waste: &ToxicWaste,
    honest: &Opening,
    claimed_value: u64,
) -> Result<Opening, ZkError> {
    let inverse = curve::invert(&waste.tau).ok_or(ZkError::ToxicWasteNotInvertible)?;
    let blinding = curve::decode_scalar(&honest.blinding)?;
    let difference = curve::scalar_from_u64(honest.value) - curve::scalar_from_u64(claimed_value);
    let shifted = blinding + difference * inverse;
    Ok(Opening { value: claimed_value, blinding: curve::encode_scalar(&shifted) })
}

/// The same attack without `tau`. There is nothing to compute the shift from, so the attacker
/// guesses a blinding factor, and the verifier says no.
pub fn forge_opening_without_toxic_waste(
    claimed_value: u64,
    rng: &mut DeterministicRng,
) -> Opening {
    Opening { value: claimed_value, blinding: curve::encode_scalar(&curve::random_scalar(rng)) }
}

/// What one participant did, as a screen shows the ceremony.
#[derive(Clone, Copy, Debug)]
pub struct ContributionRecord {
    /// Which participant.
    pub index: usize,
    /// The point they published.
    pub public_point: [u8; 32],
    /// Whether they destroyed their factor in this run.
    pub destroyed: bool,
}

/// The ceremony as data.
#[derive(Clone, Debug)]
pub struct CeremonyRecord {
    /// How many took part.
    pub participants: usize,
    /// What each of them did.
    pub contributions: Vec<ContributionRecord>,
    /// The base generator.
    pub g: [u8; 32],
    /// The generator the ceremony produced.
    pub h: [u8; 32],
    /// Whether the waste could be rebuilt from what survived this run.
    pub waste_recoverable: bool,
    /// The waste itself, present only when every factor survived.
    pub waste: Option<[u8; 32]>,
}

/// One attempt to open a commitment at a value it does not hold.
#[derive(Clone, Copy, Debug)]
pub struct ForgeryRecord {
    /// Whether this attacker held the toxic waste.
    pub held_toxic_waste: bool,
    /// The value it wanted the commitment to say.
    pub claimed_value: u64,
    /// The opening it produced.
    pub opening: Opening,
    /// What the verifier said.
    pub accepted: bool,
}

/// Everything stage 3 produces.
#[derive(Clone, Debug)]
pub struct Run {
    /// The ceremony, participant by participant.
    pub ceremony: CeremonyRecord,
    /// The commitment on the chain.
    pub commitment: [u8; 32],
    /// The value it really holds.
    pub true_value: u64,
    /// The honest opening.
    pub honest_opening: Opening,
    /// What the verifier said about it.
    pub honest_accepted: bool,
    /// The attacker who kept the waste. Accepted.
    pub with_waste: ForgeryRecord,
    /// The attacker who did not. Rejected.
    pub without_waste: ForgeryRecord,
    /// Bytes an opening takes.
    pub proof_bytes: usize,
    /// Time the ceremony took.
    pub setup_nanos: u64,
    /// Time to commit and open honestly.
    pub prove_nanos: u64,
    /// Time to check an opening.
    pub verify_nanos: u64,
}

/// Runs stage 3 end to end. `true_value` is what the commitment really holds; `claimed_value` is
/// what both attackers try to make it say.
pub fn run(
    participants: usize,
    true_value: u64,
    claimed_value: u64,
    rng: &mut DeterministicRng,
) -> Result<Run, ZkError> {
    let start_setup = std::time::Instant::now();
    let (parameters, contributions) = run_ceremony(participants, rng)?;
    let setup_nanos = start_setup.elapsed().as_nanos() as u64;

    let start_prove = std::time::Instant::now();
    let blinding = curve::random_scalar(rng);
    let commitment = commit(&parameters, true_value, &blinding);
    let honest_opening = Opening { value: true_value, blinding: curve::encode_scalar(&blinding) };
    let prove_nanos = start_prove.elapsed().as_nanos() as u64;

    let start_verify = std::time::Instant::now();
    let honest_accepted = verify_opening(&parameters, &commitment, &honest_opening)?;
    let verify_nanos = start_verify.elapsed().as_nanos() as u64;

    // This run models the ceremony going wrong: every participant kept their factor, so the waste
    // can be rebuilt and an attacker holds it.
    let waste = ToxicWaste::from_all_contributions(&parameters, &contributions);
    let with_waste_opening = match waste.as_ref() {
        Some(waste) => forge_opening_with_toxic_waste(waste, &honest_opening, claimed_value)?,
        None => forge_opening_without_toxic_waste(claimed_value, rng),
    };
    let with_waste_accepted = verify_opening(&parameters, &commitment, &with_waste_opening)?;

    let without_waste_opening = forge_opening_without_toxic_waste(claimed_value, rng);
    let without_waste_accepted = verify_opening(&parameters, &commitment, &without_waste_opening)?;

    let ceremony = CeremonyRecord {
        participants,
        contributions: contributions
            .iter()
            .map(|c| ContributionRecord {
                index: c.index(),
                public_point: c.public_point(),
                destroyed: false,
            })
            .collect(),
        g: parameters.g(),
        h: parameters.h(),
        waste_recoverable: waste.is_some(),
        waste: waste.as_ref().map(ToxicWaste::bytes),
    };

    Ok(Run {
        ceremony,
        commitment: commitment.bytes(),
        true_value,
        honest_opening,
        honest_accepted,
        with_waste: ForgeryRecord {
            held_toxic_waste: waste.is_some(),
            claimed_value,
            opening: with_waste_opening,
            accepted: with_waste_accepted,
        },
        without_waste: ForgeryRecord {
            held_toxic_waste: false,
            claimed_value,
            opening: without_waste_opening,
            accepted: without_waste_accepted,
        },
        proof_bytes: PROOF_BYTES,
        setup_nanos,
        prove_nanos,
        verify_nanos,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;

    fn rng() -> DeterministicRng {
        DeterministicRng::new(Seed::fixed())
    }

    #[test]
    fn an_honest_opening_verifies() {
        let mut rng = rng();
        let (parameters, _) = run_ceremony(3, &mut rng).expect("ceremony runs");
        let blinding = curve::random_scalar(&mut rng);
        let commitment = commit(&parameters, 42, &blinding);
        let opening = Opening { value: 42, blinding: curve::encode_scalar(&blinding) };
        assert!(verify_opening(&parameters, &commitment, &opening).expect("verify runs"));
    }

    #[test]
    fn the_toxic_waste_holder_opens_a_commitment_at_any_value() {
        let mut rng = rng();
        let (parameters, contributions) = run_ceremony(3, &mut rng).expect("ceremony runs");
        let waste = ToxicWaste::from_all_contributions(&parameters, &contributions)
            .expect("every factor survived");
        let blinding = curve::random_scalar(&mut rng);
        let commitment = commit(&parameters, 1, &blinding);
        let honest = Opening { value: 1, blinding: curve::encode_scalar(&blinding) };
        for claimed in [0u64, 2, 1_000_000, u64::MAX] {
            let forged = forge_opening_with_toxic_waste(&waste, &honest, claimed)
                .expect("waste is invertible");
            assert_eq!(forged.value, claimed);
            assert!(
                verify_opening(&parameters, &commitment, &forged).expect("verify runs"),
                "the same verifier accepts a false value when the waste survives"
            );
        }
    }

    #[test]
    fn without_the_waste_the_same_attempt_fails() {
        let mut rng = rng();
        let (parameters, _) = run_ceremony(3, &mut rng).expect("ceremony runs");
        let blinding = curve::random_scalar(&mut rng);
        let commitment = commit(&parameters, 1, &blinding);
        let forged = forge_opening_without_toxic_waste(2, &mut rng);
        assert!(!verify_opening(&parameters, &commitment, &forged).expect("verify runs"));
    }

    #[test]
    fn one_destroyed_factor_is_enough() {
        let mut rng = rng();
        let (parameters, mut contributions) = run_ceremony(3, &mut rng).expect("ceremony runs");
        let honest_participant = contributions.pop().expect("three participants");
        honest_participant.destroy();
        assert!(ToxicWaste::from_all_contributions(&parameters, &contributions).is_none());
    }

    #[test]
    fn a_duplicated_contribution_does_not_stand_in_for_a_missing_one() {
        let mut rng = rng();
        let (parameters, contributions) = run_ceremony(3, &mut rng).expect("ceremony runs");
        let doubled =
            vec![contributions[0].clone(), contributions[0].clone(), contributions[1].clone()];
        assert!(ToxicWaste::from_all_contributions(&parameters, &doubled).is_none());
    }

    #[test]
    fn an_empty_ceremony_is_refused() {
        assert_eq!(run_ceremony(0, &mut rng()).err(), Some(ZkError::CeremonyEmpty));
    }

    #[test]
    fn the_run_reports_both_attackers() {
        let run = run(DEFAULT_PARTICIPANTS, 7, 700, &mut rng()).expect("stage 3 runs");
        assert!(run.honest_accepted);
        assert!(run.with_waste.accepted);
        assert!(!run.without_waste.accepted);
        assert_eq!(run.with_waste.claimed_value, 700);
    }
}
