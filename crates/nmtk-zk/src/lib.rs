//! Zero-knowledge proofs, from sigma protocols to halo2, seen from four sides.
//!
//! Four stages run here in the order the field arrived at them, and each one fixes something the
//! one before it could not:
//!
//! 1. [`sigma`] — three messages, a verifier who has to be there, and a challenge it draws itself.
//! 2. [`fiat_shamir`] — the challenge becomes a hash, the conversation becomes a file, and the
//!    verifier can be anyone, later. Including the mistake that makes it forgeable.
//! 3. [`setup`] — an argument that is only sound because a number was destroyed, with the number
//!    still in the code so an attacker who kept it can be watched succeeding.
//! 4. [`halo2`] — a real circuit, really compiled and really proved, with nothing to trust.
//!
//! Every stage returns a [`StageOutcome`]: three measurements, whether the honest proof was
//! accepted, what the attacker managed, and the same shielded payment written out from the sender's
//! side, the receiver's, an onlooker's and an attacker's.
//!
//! Nothing here produces a word for a reader. Every value is a number, an enum or a byte string,
//! and the screen that shows it is written somewhere else.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod curve;
pub mod fiat_shamir;
pub mod halo2;
pub mod rng;
pub mod setup;
pub mod sigma;
pub mod views;

use std::time::Duration;

use nmtk_core::MachineProfile;

pub use rng::{DEFAULT_SEED, DeterministicRng, Seed};
pub use views::{
    AttackerView, Claim, Item, OnlookerView, Party, PartyViews, ReceiverView, SenderView,
    ShieldedPayment,
};

/// The stages, in the order they are meant to be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Stage {
    /// Stage 1: the interactive sigma protocol.
    Sigma,
    /// Stage 2: the same proof, non-interactive.
    FiatShamir,
    /// Stage 3: an argument that depends on destroyed setup randomness.
    TrustedSetup,
    /// Stage 4: a halo2 circuit, with no setup to trust.
    Halo2,
}

impl Stage {
    /// Every stage, in order.
    pub const ALL: [Stage; 4] =
        [Stage::Sigma, Stage::FiatShamir, Stage::TrustedSetup, Stage::Halo2];

    /// Position in the lineage, from zero.
    pub fn index(self) -> usize {
        match self {
            Stage::Sigma => 0,
            Stage::FiatShamir => 1,
            Stage::TrustedSetup => 2,
            Stage::Halo2 => 3,
        }
    }
}

/// The three numbers that are the honest difference between these systems.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Measurement {
    /// Time to build whatever the prover and verifier share before any proof exists: a ceremony in
    /// stage 3, generators and keys in stage 4, nothing in stages 1 and 2.
    pub setup_nanos: u64,
    /// Time the prover spent.
    pub prove_nanos: u64,
    /// Time the verifier spent.
    pub verify_nanos: u64,
    /// Bytes that have to be kept and sent for the proof to be checkable.
    pub proof_bytes: usize,
}

impl Measurement {
    /// Setup time as a duration.
    pub fn setup_time(&self) -> Duration {
        Duration::from_nanos(self.setup_nanos)
    }

    /// Proving time as a duration.
    pub fn prove_time(&self) -> Duration {
        Duration::from_nanos(self.prove_nanos)
    }

    /// Verification time as a duration.
    pub fn verify_time(&self) -> Duration {
        Duration::from_nanos(self.verify_nanos)
    }
}

/// What a stage needs before it can prove anything, and what that costs in trust.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupKind {
    /// Nothing beyond a generator everyone already agrees on.
    NoneNeeded,
    /// Public parameters anyone can rederive from a fixed rule. Nothing was secret, so nothing had
    /// to be destroyed.
    Transparent,
    /// Parameters built from randomness that had to be destroyed afterwards.
    ToxicWaste,
}

/// Who has to be present, who can check later, and what the soundness rests on.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TrustModel {
    /// What the stage needed before proving.
    pub setup: SetupKind,
    /// Whether prover and verifier exchange more than one message.
    pub interactive: bool,
    /// Whether the verifier has to be present while the prover works.
    pub verifier_must_be_online: bool,
    /// Whether a third party can check the same bytes afterwards.
    pub anyone_can_verify_later: bool,
    /// Whether the whole thing falls over if one secret was kept.
    pub soundness_needs_destroyed_secret: bool,
}

/// What an attacker tried.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ForgeryKind {
    /// Answered a challenge it could not have predicted, by guessing.
    GuessedResponse,
    /// Solved for a commitment after the challenge, against a challenge hash that leaves the
    /// commitment out.
    WeakFiatShamirRebind,
    /// The same move against a challenge hash that covers the commitment.
    CorrectFiatShamirRebind,
    /// Reopened a commitment at a false value, holding the setup randomness.
    ToxicWasteOpening,
    /// The same attempt without it.
    OpeningWithoutToxicWaste,
    /// Spent more than the note held, covering the difference with a wrapped-around amount.
    OverspendOutOfRange,
}

/// One attempt and what the verifier said about it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ForgeryAttempt {
    /// What was tried.
    pub kind: ForgeryKind,
    /// What the attacker was holding that it should not have been.
    pub attacker_holds: Option<Item>,
    /// Whether the real verifier accepted it. True exactly once in this crate: stage 3, with the
    /// toxic waste in hand.
    pub accepted: bool,
    /// Size of what it produced.
    pub proof_bytes: usize,
}

/// Every attempt made in one stage.
#[derive(Clone, Debug)]
pub struct ForgeryOutcome {
    /// The attempts, in the order they were made.
    pub attempts: Vec<ForgeryAttempt>,
    /// Whether any of them got through.
    pub any_accepted: bool,
}

impl ForgeryOutcome {
    fn new(attempts: Vec<ForgeryAttempt>) -> Self {
        let any_accepted = attempts.iter().any(|attempt| attempt.accepted);
        Self { attempts, any_accepted }
    }
}

/// Everything one stage produced, kept whole so a screen can walk it step by step.
#[derive(Clone, Debug)]
pub enum StageDetail {
    /// Stage 1, with its transcript.
    Sigma(Box<sigma::Run>),
    /// Stage 2, with both bindings.
    FiatShamir(Box<fiat_shamir::Run>),
    /// Stage 3, with the ceremony.
    TrustedSetup(Box<setup::Run>),
    /// Stage 4, with the circuit's shape.
    Halo2(Box<halo2::Run>),
}

/// The result of running one stage.
#[derive(Clone, Debug)]
pub struct StageOutcome {
    /// Which stage.
    pub stage: Stage,
    /// Proving time, verification time and proof size.
    pub measurement: Measurement,
    /// What it needs to be believed.
    pub trust: TrustModel,
    /// Whether the honest proof was accepted.
    pub honest_accepted: bool,
    /// What the attacker managed.
    pub forgery: ForgeryOutcome,
    /// The same payment from four sides.
    pub views: PartyViews,
    /// The stage's own output, whole.
    pub detail: StageDetail,
}

/// What went wrong. These carry no words for a reader; the short English is for a developer
/// reading a backtrace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZkError {
    /// A protocol step was taken before the step it depends on.
    OutOfOrder,
    /// Thirty-two bytes that are not a point of the curve.
    BadPointEncoding,
    /// Thirty-two bytes that are not below the field modulus.
    BadScalarEncoding,
    /// A ceremony with nobody in it.
    CeremonyEmpty,
    /// The setup randomness was zero, so nothing can be divided by it.
    ToxicWasteNotInvertible,
    /// An amount outside the range the circuit was built for.
    ValueOutOfRange,
    /// A range width this module does not build circuits for.
    ValueBitsUnsupported,
    /// The proof system failed at a step.
    Proving(ProofStep),
}

/// Where a proof system gave up.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProofStep {
    /// Building generators or keys.
    Keygen,
    /// Producing a proof.
    Prove,
    /// Checking one.
    Verify,
}

impl core::fmt::Display for ZkError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        let text = match self {
            ZkError::OutOfOrder => "protocol step out of order",
            ZkError::BadPointEncoding => "not a curve point",
            ZkError::BadScalarEncoding => "not a field scalar",
            ZkError::CeremonyEmpty => "ceremony has no participants",
            ZkError::ToxicWasteNotInvertible => "setup randomness is zero",
            ZkError::ValueOutOfRange => "value outside the circuit range",
            ZkError::ValueBitsUnsupported => "unsupported range width",
            ZkError::Proving(ProofStep::Keygen) => "key generation failed",
            ZkError::Proving(ProofStep::Prove) => "proof generation failed",
            ZkError::Proving(ProofStep::Verify) => "proof verification failed",
        };
        f.write_str(text)
    }
}

impl std::error::Error for ZkError {}

/// Each stage gets its own generator, derived from the run's seed. Running stage 3 alone gives the
/// same numbers as running it inside [`run_all`], and no two stages ever draw the same nonce.
fn stage_seed(seed: Seed, stage: Stage) -> Seed {
    Seed(curve::digest(0x10 + stage.index() as u8, &[&seed.bytes()]))
}

/// Runs one stage on this machine and reports what happened.
pub fn run_stage(
    stage: Stage,
    seed: Seed,
    profile: &MachineProfile,
) -> Result<StageOutcome, ZkError> {
    let mut rng = DeterministicRng::new(stage_seed(seed, stage));
    let payment = views::scenario(views::DEFAULT_VALUE, views::DEFAULT_CHANGE, &mut rng);

    match stage {
        Stage::Sigma => {
            let run = sigma::run(&mut rng)?;
            let proof: Vec<u8> =
                run.transcript.lines.iter().flat_map(|line| line.bytes.clone()).collect();
            let forgery = ForgeryOutcome::new(vec![ForgeryAttempt {
                kind: ForgeryKind::GuessedResponse,
                attacker_holds: None,
                accepted: run.forged_accepted,
                proof_bytes: sigma::TRANSCRIPT_BYTES,
            }]);
            let trust = TrustModel {
                setup: SetupKind::NoneNeeded,
                interactive: true,
                verifier_must_be_online: true,
                anyone_can_verify_later: false,
                soundness_needs_destroyed_secret: false,
            };
            Ok(assemble(
                stage,
                Measurement {
                    setup_nanos: 0,
                    prove_nanos: run.prove_nanos,
                    verify_nanos: run.verify_nanos,
                    proof_bytes: run.transcript_bytes,
                },
                trust,
                run.accepted,
                forgery,
                &payment,
                &proof,
                StageDetail::Sigma(Box::new(run)),
            ))
        }
        Stage::FiatShamir => {
            let mut context = Vec::with_capacity(64);
            context.extend_from_slice(&payment.nullifier);
            context.extend_from_slice(&payment.note_commitment);
            let run = fiat_shamir::run(&context, &mut rng)?;
            let proof = run.proof.to_bytes().to_vec();
            let forgery = ForgeryOutcome::new(vec![
                ForgeryAttempt {
                    kind: ForgeryKind::WeakFiatShamirRebind,
                    attacker_holds: None,
                    accepted: run.weak_forgery.accepted,
                    proof_bytes: fiat_shamir::PROOF_BYTES,
                },
                ForgeryAttempt {
                    kind: ForgeryKind::CorrectFiatShamirRebind,
                    attacker_holds: None,
                    accepted: run.strong_forgery.accepted,
                    proof_bytes: fiat_shamir::PROOF_BYTES,
                },
            ]);
            let trust = TrustModel {
                setup: SetupKind::NoneNeeded,
                interactive: false,
                verifier_must_be_online: false,
                anyone_can_verify_later: true,
                soundness_needs_destroyed_secret: false,
            };
            Ok(assemble(
                stage,
                Measurement {
                    setup_nanos: 0,
                    prove_nanos: run.prove_nanos,
                    verify_nanos: run.verify_nanos,
                    proof_bytes: run.proof_bytes,
                },
                trust,
                run.accepted,
                forgery,
                &payment,
                &proof,
                StageDetail::FiatShamir(Box::new(run)),
            ))
        }
        Stage::TrustedSetup => {
            let claimed = payment.value.saturating_mul(1_000);
            let run = setup::run(setup::DEFAULT_PARTICIPANTS, payment.value, claimed, &mut rng)?;
            let proof = run.honest_opening.to_bytes().to_vec();
            let forgery = ForgeryOutcome::new(vec![
                ForgeryAttempt {
                    kind: ForgeryKind::ToxicWasteOpening,
                    attacker_holds: Some(Item::ToxicWaste),
                    accepted: run.with_waste.accepted,
                    proof_bytes: setup::PROOF_BYTES,
                },
                ForgeryAttempt {
                    kind: ForgeryKind::OpeningWithoutToxicWaste,
                    attacker_holds: None,
                    accepted: run.without_waste.accepted,
                    proof_bytes: setup::PROOF_BYTES,
                },
            ]);
            let trust = TrustModel {
                setup: SetupKind::ToxicWaste,
                interactive: false,
                verifier_must_be_online: false,
                anyone_can_verify_later: true,
                soundness_needs_destroyed_secret: true,
            };
            Ok(assemble(
                stage,
                Measurement {
                    setup_nanos: run.setup_nanos,
                    prove_nanos: run.prove_nanos,
                    verify_nanos: run.verify_nanos,
                    proof_bytes: run.proof_bytes,
                },
                trust,
                run.honest_accepted,
                forgery,
                &payment,
                &proof,
                StageDetail::TrustedSetup(Box::new(run)),
            ))
        }
        Stage::Halo2 => {
            let config = halo2::Config::for_machine(profile);
            let run = halo2::run(config, payment.value, payment.change, &mut rng)?;
            let proof = run.proof.clone();
            let forgery = ForgeryOutcome::new(vec![ForgeryAttempt {
                kind: ForgeryKind::OverspendOutOfRange,
                attacker_holds: None,
                accepted: run.forgery.accepted,
                proof_bytes: run.forgery.proof_bytes,
            }]);
            let trust = TrustModel {
                setup: SetupKind::Transparent,
                interactive: false,
                verifier_must_be_online: false,
                anyone_can_verify_later: true,
                soundness_needs_destroyed_secret: false,
            };
            Ok(assemble(
                stage,
                Measurement {
                    setup_nanos: run.setup_nanos,
                    prove_nanos: run.prove_nanos,
                    verify_nanos: run.verify_nanos,
                    proof_bytes: run.proof_bytes,
                },
                trust,
                run.accepted,
                forgery,
                &payment,
                &proof,
                StageDetail::Halo2(Box::new(run)),
            ))
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn assemble(
    stage: Stage,
    measurement: Measurement,
    trust: TrustModel,
    honest_accepted: bool,
    forgery: ForgeryOutcome,
    payment: &ShieldedPayment,
    proof: &[u8],
    detail: StageDetail,
) -> StageOutcome {
    let views = views::build(
        views::ViewInputs { stage, payment, proof, honest_accepted, setup: trust.setup },
        &forgery,
    );
    StageOutcome { stage, measurement, trust, honest_accepted, forgery, views, detail }
}

/// Runs all four stages in order, so a screen can put the three numbers side by side.
pub fn run_all(seed: Seed, profile: &MachineProfile) -> Result<Vec<StageOutcome>, ZkError> {
    Stage::ALL.iter().map(|stage| run_stage(*stage, seed, profile)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile() -> MachineProfile {
        MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 }
    }

    #[test]
    fn every_stage_accepts_its_honest_proof() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            assert!(outcome.honest_accepted, "{stage:?} rejected an honest proof");
        }
    }

    #[test]
    fn only_the_toxic_waste_holder_gets_a_false_statement_accepted() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            for attempt in &outcome.forgery.attempts {
                let expected = matches!(
                    attempt.kind,
                    ForgeryKind::ToxicWasteOpening | ForgeryKind::WeakFiatShamirRebind
                );
                assert_eq!(
                    attempt.accepted, expected,
                    "{stage:?} {:?} was not what it should be",
                    attempt.kind
                );
            }
        }
    }

    #[test]
    fn every_stage_reports_three_numbers() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            assert!(outcome.measurement.proof_bytes > 0);
            assert!(outcome.measurement.prove_nanos > 0);
            assert!(outcome.measurement.verify_nanos > 0);
        }
    }

    #[test]
    fn the_onlooker_never_sees_an_amount_or_an_address() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            let onlooker = &outcome.views.onlooker;
            assert!(onlooker.transaction_seen);
            assert_eq!(onlooker.amount, None);
            assert_eq!(onlooker.sender_address, None);
            assert_eq!(onlooker.recipient_address, None);
            assert!(onlooker.sees.contains(&Item::Proof));
            assert!(onlooker.sees.contains(&Item::Nullifier));
            assert!(onlooker.sees.contains(&Item::NoteCommitment));
            assert!(onlooker.cannot_see.contains(&Item::Amount));
            assert!(onlooker.cannot_see.contains(&Item::RecipientAddress));
            assert!(onlooker.proof_bytes > 0);
        }
    }

    #[test]
    fn the_receiver_learns_the_amount_and_not_the_sender() {
        let outcome = run_stage(Stage::Halo2, Seed::fixed(), &profile()).expect("stage runs");
        assert_eq!(outcome.views.receiver.amount, views::DEFAULT_VALUE);
        assert_eq!(outcome.views.receiver.sender_address, None);
    }

    #[test]
    fn only_stage_three_hands_the_attacker_a_secret() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            let holds_waste = outcome.views.attacker.holds.contains(&Item::ToxicWaste);
            assert_eq!(holds_waste, stage == Stage::TrustedSetup);
        }
    }

    #[test]
    fn only_stage_three_depends_on_a_destroyed_secret() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            assert_eq!(
                outcome.trust.soundness_needs_destroyed_secret,
                stage == Stage::TrustedSetup
            );
        }
    }

    #[test]
    fn only_the_first_stage_needs_the_verifier_present() {
        for stage in Stage::ALL {
            let outcome = run_stage(stage, Seed::fixed(), &profile()).expect("stage runs");
            assert_eq!(outcome.trust.verifier_must_be_online, stage == Stage::Sigma);
            assert_eq!(outcome.trust.anyone_can_verify_later, stage != Stage::Sigma);
        }
    }

    #[test]
    fn the_same_seed_gives_the_same_proof_bytes() {
        let first = run_stage(Stage::FiatShamir, Seed::fixed(), &profile()).expect("runs");
        let second = run_stage(Stage::FiatShamir, Seed::fixed(), &profile()).expect("runs");
        match (first.detail, second.detail) {
            (StageDetail::FiatShamir(a), StageDetail::FiatShamir(b)) => {
                assert_eq!(a.proof, b.proof);
            }
            _ => panic!("stage 2 returned another stage's detail"),
        }
    }

    #[test]
    fn a_different_seed_gives_different_proof_bytes() {
        let first = run_stage(Stage::FiatShamir, Seed::from_u64(1), &profile()).expect("runs");
        let second = run_stage(Stage::FiatShamir, Seed::from_u64(2), &profile()).expect("runs");
        match (first.detail, second.detail) {
            (StageDetail::FiatShamir(a), StageDetail::FiatShamir(b)) => {
                assert_ne!(a.proof, b.proof);
            }
            _ => panic!("stage 2 returned another stage's detail"),
        }
    }

    #[test]
    fn two_stages_never_draw_the_same_nonce() {
        let one = run_stage(Stage::Sigma, Seed::fixed(), &profile()).expect("runs");
        let two = run_stage(Stage::FiatShamir, Seed::fixed(), &profile()).expect("runs");
        let (StageDetail::Sigma(sigma_run), StageDetail::FiatShamir(fs_run)) =
            (one.detail, two.detail)
        else {
            panic!("stages returned the wrong detail");
        };
        let sigma_commitment = sigma_run
            .transcript
            .lines
            .iter()
            .find(|line| line.step == sigma::Step::Commitment)
            .map(|line| line.bytes.clone())
            .expect("the transcript has a commitment");
        assert_ne!(sigma_commitment.as_slice(), fs_run.proof.commitment.as_slice());
    }

    #[test]
    fn all_four_stages_run_together() {
        let outcomes = run_all(Seed::fixed(), &profile()).expect("all stages run");
        assert_eq!(outcomes.len(), 4);
        for (index, outcome) in outcomes.iter().enumerate() {
            assert_eq!(outcome.stage.index(), index);
        }
    }
}
