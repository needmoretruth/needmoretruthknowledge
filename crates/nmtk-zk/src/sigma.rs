//! Stage 1 — the interactive sigma protocol (Schnorr's proof of knowledge of a discrete logarithm).
//!
//! The prover knows a scalar `x`; the world knows the point `P = x*G`. In three messages the prover
//! convinces a verifier that it knows `x` without sending it. The three messages are three separate
//! calls here on purpose: a screen walks a reader through commitment, challenge and response one at
//! a time, and between the second and the third the verifier's fresh randomness is what the prover
//! cannot have prepared for.

use halo2_proofs::pasta::group::Curve;

use crate::ZkError;
use crate::curve::{self, Point, Scalar};
use crate::rng::DeterministicRng;

/// Bytes on the wire for one interactive run: commitment, challenge and response.
pub const TRANSCRIPT_BYTES: usize = 96;

/// The secret `x`. Held by the sender and by nobody else.
#[derive(Clone)]
pub struct Witness {
    secret: Scalar,
}

impl Witness {
    /// Draws a fresh secret from the run's generator.
    pub fn random(rng: &mut DeterministicRng) -> Self {
        Self { secret: curve::random_scalar(rng) }
    }

    /// Rebuilds a secret from its 32 bytes.
    pub fn from_bytes(bytes: &[u8; 32]) -> Result<Self, ZkError> {
        Ok(Self { secret: curve::decode_scalar(bytes)? })
    }

    /// The public statement this secret proves: `P = x*G`.
    pub fn statement(&self) -> Statement {
        Statement { point: curve::generator() * self.secret }
    }

    /// The secret as bytes. A screen may show this beside the sender, never beside anyone else.
    pub fn bytes(&self) -> [u8; 32] {
        curve::encode_scalar(&self.secret)
    }

    pub(crate) fn scalar(&self) -> Scalar {
        self.secret
    }
}

impl core::fmt::Debug for Witness {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Witness").finish_non_exhaustive()
    }
}

/// The public claim: "there is an `x` with `P = x*G`, and I know it".
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Statement {
    point: Point,
}

impl Statement {
    /// Reads a statement from the 32 bytes of `P`.
    pub fn from_public_key(bytes: &[u8; 32]) -> Result<Self, ZkError> {
        Ok(Self { point: curve::decode_point(bytes)? })
    }

    /// `P` as the 32 bytes an onlooker sees.
    pub fn public_key(&self) -> [u8; 32] {
        curve::encode_point(&self.point)
    }

    pub(crate) fn point(&self) -> Point {
        self.point
    }
}

/// Message 1: `R = r*G` for a nonce `r` the prover keeps.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Commitment(pub [u8; 32]);

/// Message 2: the verifier's fresh randomness.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Challenge(pub [u8; 32]);

/// Message 3: `s = r + c*x`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Response(pub [u8; 32]);

/// The prover's side of the three messages.
pub struct Prover {
    secret: Scalar,
    statement: Statement,
    nonce: Option<Scalar>,
}

impl Prover {
    /// Takes the secret and works out the statement it will be held to.
    pub fn new(witness: &Witness) -> Self {
        Self { secret: witness.scalar(), statement: witness.statement(), nonce: None }
    }

    /// The statement the verifier will check against.
    pub fn statement(&self) -> Statement {
        self.statement
    }

    /// Step 1. Picks a nonce and sends `R = r*G`. Calling it again picks a new nonce: reusing one
    /// across two different challenges is what hands the secret to the verifier.
    pub fn commit(&mut self, rng: &mut DeterministicRng) -> Commitment {
        let nonce = curve::random_scalar(rng);
        self.nonce = Some(nonce);
        Commitment(curve::encode_point(&(curve::generator() * nonce)))
    }

    /// Step 3. Answers the challenge with `s = r + c*x`, consuming the nonce.
    pub fn respond(&mut self, challenge: &Challenge) -> Result<Response, ZkError> {
        let nonce = self.nonce.take().ok_or(ZkError::OutOfOrder)?;
        let c = curve::decode_scalar(&challenge.0)?;
        Ok(Response(curve::encode_scalar(&(nonce + c * self.secret))))
    }
}

impl core::fmt::Debug for Prover {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Prover").finish_non_exhaustive()
    }
}

/// The verifier's side of the three messages.
#[derive(Debug)]
pub struct Verifier {
    statement: Statement,
    commitment: Option<Point>,
    challenge: Option<Scalar>,
}

impl Verifier {
    /// Starts a verifier that will hold the prover to this statement.
    pub fn new(statement: Statement) -> Self {
        Self { statement, commitment: None, challenge: None }
    }

    /// Takes message 1 in.
    pub fn receive_commitment(&mut self, commitment: &Commitment) -> Result<(), ZkError> {
        self.commitment = Some(curve::decode_point(&commitment.0)?);
        self.challenge = None;
        Ok(())
    }

    /// Step 2. Fresh randomness, drawn after the commitment is in hand and never before.
    pub fn challenge(&mut self, rng: &mut DeterministicRng) -> Result<Challenge, ZkError> {
        if self.commitment.is_none() {
            return Err(ZkError::OutOfOrder);
        }
        let c = curve::random_scalar(rng);
        self.challenge = Some(c);
        Ok(Challenge(curve::encode_scalar(&c)))
    }

    /// Step 4. Checks `s*G == R + c*P`.
    pub fn verify(&self, response: &Response) -> Result<bool, ZkError> {
        let commitment = self.commitment.ok_or(ZkError::OutOfOrder)?;
        let challenge = self.challenge.ok_or(ZkError::OutOfOrder)?;
        let s = curve::decode_scalar(&response.0)?;
        let left = curve::generator() * s;
        let right = commitment + self.statement.point() * challenge;
        Ok(left.to_affine() == right.to_affine())
    }
}

/// Which of the four messages a transcript line carries.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Step {
    /// The public statement, known before the protocol starts.
    Statement,
    /// Message 1, from the prover.
    Commitment,
    /// Message 2, from the verifier.
    Challenge,
    /// Message 3, from the prover.
    Response,
    /// The verifier's answer.
    Verdict,
}

/// Who put a line on the wire.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Side {
    /// The party that holds the secret.
    Prover,
    /// The party that holds only the statement.
    Verifier,
    /// Neither: agreed before the run.
    Public,
}

/// One line of the wire, as a screen shows it.
#[derive(Clone, Debug)]
pub struct TranscriptLine {
    /// Which message this is.
    pub step: Step,
    /// Who sent it.
    pub side: Side,
    /// The bytes themselves.
    pub bytes: Vec<u8>,
    /// Set only on the verdict line.
    pub accepted: Option<bool>,
}

/// Every line of one run, in order.
#[derive(Clone, Debug)]
pub struct Transcript {
    /// The lines, oldest first.
    pub lines: Vec<TranscriptLine>,
}

impl Transcript {
    fn push(&mut self, step: Step, side: Side, bytes: &[u8], accepted: Option<bool>) {
        self.lines.push(TranscriptLine { step, side, bytes: bytes.to_vec(), accepted });
    }
}

/// One honest run and one forged run of the interactive protocol.
#[derive(Clone, Debug)]
pub struct Run {
    /// `P`, the public key the prover is held to.
    pub statement: [u8; 32],
    /// The honest run, line by line.
    pub transcript: Transcript,
    /// The verifier's answer to the honest run.
    pub accepted: bool,
    /// The attacker's run, line by line. Same statement, no secret.
    pub forged_transcript: Transcript,
    /// The verifier's answer to the attacker's run.
    pub forged_accepted: bool,
    /// Bytes that crossed the wire in the honest run.
    pub transcript_bytes: usize,
    /// Time the prover spent on messages 1 and 3.
    pub prove_nanos: u64,
    /// Time the verifier spent on message 2 and the check.
    pub verify_nanos: u64,
}

/// Runs stage 1 end to end: an honest three-message proof, then an attacker who has the statement
/// but not the secret and must answer a challenge it could not predict.
pub fn run(rng: &mut DeterministicRng) -> Result<Run, ZkError> {
    let witness = Witness::random(rng);
    let statement = witness.statement();
    let mut prover = Prover::new(&witness);
    let mut verifier = Verifier::new(statement);

    let mut transcript = Transcript { lines: Vec::new() };
    transcript.push(Step::Statement, Side::Public, &statement.public_key(), None);

    let start_prove = std::time::Instant::now();
    let commitment = prover.commit(rng);
    let commit_nanos = start_prove.elapsed().as_nanos() as u64;
    transcript.push(Step::Commitment, Side::Prover, &commitment.0, None);

    let start_verify = std::time::Instant::now();
    verifier.receive_commitment(&commitment)?;
    let challenge = verifier.challenge(rng)?;
    let challenge_nanos = start_verify.elapsed().as_nanos() as u64;
    transcript.push(Step::Challenge, Side::Verifier, &challenge.0, None);

    let start_respond = std::time::Instant::now();
    let response = prover.respond(&challenge)?;
    let respond_nanos = start_respond.elapsed().as_nanos() as u64;
    transcript.push(Step::Response, Side::Prover, &response.0, None);

    let start_check = std::time::Instant::now();
    let accepted = verifier.verify(&response)?;
    let check_nanos = start_check.elapsed().as_nanos() as u64;
    transcript.push(Step::Verdict, Side::Verifier, &[], Some(accepted));

    let (forged_transcript, forged_accepted) = forge(&statement, rng)?;

    Ok(Run {
        statement: statement.public_key(),
        transcript,
        accepted,
        forged_transcript,
        forged_accepted,
        transcript_bytes: TRANSCRIPT_BYTES,
        prove_nanos: commit_nanos + respond_nanos,
        verify_nanos: challenge_nanos + check_nanos,
    })
}

/// An attacker that holds the statement and no secret. It commits like anyone can, then has to
/// produce `s` with `s*G == R + c*P` for a challenge it sees only afterwards, so it guesses.
fn forge(statement: &Statement, rng: &mut DeterministicRng) -> Result<(Transcript, bool), ZkError> {
    let mut verifier = Verifier::new(*statement);
    let mut transcript = Transcript { lines: Vec::new() };
    transcript.push(Step::Statement, Side::Public, &statement.public_key(), None);

    let nonce = curve::random_scalar(rng);
    let commitment = Commitment(curve::encode_point(&(curve::generator() * nonce)));
    transcript.push(Step::Commitment, Side::Prover, &commitment.0, None);

    verifier.receive_commitment(&commitment)?;
    let challenge = verifier.challenge(rng)?;
    transcript.push(Step::Challenge, Side::Verifier, &challenge.0, None);

    let guess = Response(curve::encode_scalar(&curve::random_scalar(rng)));
    transcript.push(Step::Response, Side::Prover, &guess.0, None);

    let accepted = verifier.verify(&guess)?;
    transcript.push(Step::Verdict, Side::Verifier, &[], Some(accepted));
    Ok((transcript, accepted))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;

    fn rng() -> DeterministicRng {
        DeterministicRng::new(Seed::fixed())
    }

    #[test]
    fn an_honest_proof_verifies() {
        let run = run(&mut rng()).expect("stage 1 runs");
        assert!(run.accepted);
    }

    #[test]
    fn a_guessed_response_is_rejected() {
        let run = run(&mut rng()).expect("stage 1 runs");
        assert!(!run.forged_accepted);
    }

    #[test]
    fn the_transcript_carries_all_four_messages() {
        let run = run(&mut rng()).expect("stage 1 runs");
        let steps: Vec<Step> = run.transcript.lines.iter().map(|line| line.step).collect();
        assert_eq!(
            steps,
            vec![Step::Statement, Step::Commitment, Step::Challenge, Step::Response, Step::Verdict]
        );
    }

    #[test]
    fn a_response_before_a_commitment_is_an_error() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let mut prover = Prover::new(&witness);
        let challenge = Challenge(curve::encode_scalar(&curve::random_scalar(&mut rng)));
        assert_eq!(prover.respond(&challenge), Err(ZkError::OutOfOrder));
    }

    #[test]
    fn a_challenge_before_a_commitment_is_an_error() {
        let mut rng = rng();
        let witness = Witness::random(&mut rng);
        let mut verifier = Verifier::new(witness.statement());
        assert!(matches!(verifier.challenge(&mut rng), Err(ZkError::OutOfOrder)));
    }

    #[test]
    fn a_proof_for_someone_elses_statement_is_rejected() {
        let mut rng = rng();
        let mine = Witness::random(&mut rng);
        let theirs = Witness::random(&mut rng);
        let mut prover = Prover::new(&mine);
        let mut verifier = Verifier::new(theirs.statement());
        let commitment = prover.commit(&mut rng);
        verifier.receive_commitment(&commitment).expect("commitment decodes");
        let challenge = verifier.challenge(&mut rng).expect("challenge");
        let response = prover.respond(&challenge).expect("response");
        assert!(!verifier.verify(&response).expect("verify runs"));
    }

    #[test]
    fn the_same_seed_replays_the_same_transcript() {
        let first = run(&mut rng()).expect("stage 1 runs");
        let second = run(&mut rng()).expect("stage 1 runs");
        assert_eq!(first.statement, second.statement);
        let a: Vec<Vec<u8>> = first.transcript.lines.iter().map(|l| l.bytes.clone()).collect();
        let b: Vec<Vec<u8>> = second.transcript.lines.iter().map(|l| l.bytes.clone()).collect();
        assert_eq!(a, b);
    }
}
