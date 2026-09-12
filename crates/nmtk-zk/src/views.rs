//! The same shielded payment seen from four sides.
//!
//! One payment, four parties, and for each of them three lists: what it holds, what it learns, and
//! what it can check. The onlooker's list is the one worth reading twice — everything a shielded
//! chain publishes is in it, and everything a transparent chain would also have published is in the
//! list underneath, the one of things that never appear.

use crate::curve::{self, domain};
use crate::rng::DeterministicRng;
use crate::{ForgeryOutcome, SetupKind, Stage};

/// The amount the scenario sends, in the smallest unit.
pub const DEFAULT_VALUE: u64 = 21_845;
/// The change that comes back to the sender.
pub const DEFAULT_CHANGE: u64 = 13_107;

/// A thing a party may or may not hold.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Item {
    /// The key that lets a note be spent.
    SpendingKey,
    /// The key that lets a note be read but not spent.
    ViewingKey,
    /// The randomness that hides the amount inside the commitment.
    NoteBlinding,
    /// The amount being sent.
    Amount,
    /// Where the payment came from.
    SenderAddress,
    /// Where the payment is going.
    RecipientAddress,
    /// The commitment that goes on the chain in place of the note.
    NoteCommitment,
    /// The marker that stops the note being spent twice.
    Nullifier,
    /// The proof bytes.
    Proof,
    /// The public part of the claim being proved.
    PublicStatement,
    /// The parameters a proof system was set up with.
    SetupParameters,
    /// The setup randomness that was supposed to be destroyed.
    ToxicWaste,
}

/// Something a party can check for itself.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Claim {
    /// A transaction took place.
    TransactionHappened,
    /// The proof checks out against the public statement.
    ProofVerifies,
    /// The amount is inside the range the circuit allows.
    AmountInRange,
    /// The amount is the one the commitment holds.
    AmountMatchesCommitment,
    /// The nullifier has not been published before.
    NullifierUnseen,
    /// The spender holds the key to the note.
    SpenderHoldsKey,
}

/// Which side of the payment a view belongs to.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Party {
    /// The one spending a note.
    Sender,
    /// The one receiving it.
    Receiver,
    /// Anyone reading the chain.
    Onlooker,
    /// Anyone trying to get a false statement accepted.
    Attacker,
}

/// The payment every stage tells its story about.
#[derive(Clone, Debug)]
pub struct ShieldedPayment {
    /// The amount sent.
    pub value: u64,
    /// The change returned to the sender.
    pub change: u64,
    /// The value of the note being spent.
    pub input_value: u128,
    /// The sender's shielded address.
    pub sender_address: [u8; 32],
    /// The recipient's shielded address.
    pub recipient_address: [u8; 32],
    /// The commitment published in place of the output note.
    pub note_commitment: [u8; 32],
    /// The nullifier published to retire the input note.
    pub nullifier: [u8; 32],
    /// The randomness inside the commitment.
    pub blinding: [u8; 32],
    /// The key that authorised the spend.
    pub spending_key: [u8; 32],
}

/// Builds the scenario. Every field is a real value derived from the run's seed, so two runs with
/// the same seed tell the story with the same numbers.
pub fn scenario(value: u64, change: u64, rng: &mut DeterministicRng) -> ShieldedPayment {
    let mut spending_key = [0u8; 32];
    rng.fill(&mut spending_key);
    let mut blinding = [0u8; 32];
    rng.fill(&mut blinding);
    let mut recipient_seed = [0u8; 32];
    rng.fill(&mut recipient_seed);

    let sender_address = curve::digest(domain::ADDRESS, &[&spending_key]);
    let recipient_address = curve::digest(domain::ADDRESS, &[&recipient_seed]);
    let note_commitment =
        curve::digest(domain::NOTE, &[&value.to_le_bytes(), &recipient_address, &blinding]);
    let nullifier = curve::digest(domain::NULLIFIER, &[&spending_key, &note_commitment]);

    ShieldedPayment {
        value,
        change,
        input_value: value as u128 + change as u128,
        sender_address,
        recipient_address,
        note_commitment,
        nullifier,
        blinding,
        spending_key,
    }
}

/// What the party that spends the note has.
#[derive(Clone, Debug)]
pub struct SenderView {
    /// Which party this is.
    pub party: Party,
    /// What it holds.
    pub holds: Vec<Item>,
    /// What the run tells it.
    pub learns: Vec<Item>,
    /// What it can check itself.
    pub can_verify: Vec<Claim>,
    /// The amount it sent.
    pub amount: u64,
    /// The change it kept.
    pub change: u64,
    /// Where it sent the payment.
    pub recipient_address: [u8; 32],
    /// The commitment it published.
    pub note_commitment: [u8; 32],
    /// The nullifier it published.
    pub nullifier: [u8; 32],
    /// Whether its own proof checks out.
    pub proof_accepted: bool,
}

/// What the party that receives the note has.
#[derive(Clone, Debug)]
pub struct ReceiverView {
    /// Which party this is.
    pub party: Party,
    /// What it holds.
    pub holds: Vec<Item>,
    /// What the run tells it.
    pub learns: Vec<Item>,
    /// What it never finds out.
    pub never_learns: Vec<Item>,
    /// What it can check itself.
    pub can_verify: Vec<Claim>,
    /// The amount it received.
    pub amount: u64,
    /// The commitment that is now its note.
    pub note_commitment: [u8; 32],
    /// Whether it can tell who paid: on a shielded chain it cannot.
    pub sender_address: Option<[u8; 32]>,
    /// Whether the proof checks out.
    pub proof_accepted: bool,
}

/// What anyone reading the chain has.
#[derive(Clone, Debug)]
pub struct OnlookerView {
    /// Which party this is.
    pub party: Party,
    /// What it holds: nothing.
    pub holds: Vec<Item>,
    /// What is published and therefore visible.
    pub sees: Vec<Item>,
    /// What is not published and cannot be derived from what is.
    pub cannot_see: Vec<Item>,
    /// What it can check itself.
    pub can_verify: Vec<Claim>,
    /// A transaction is visible, always.
    pub transaction_seen: bool,
    /// The commitment on the chain.
    pub note_commitment: [u8; 32],
    /// The nullifier on the chain.
    pub nullifier: [u8; 32],
    /// How big the proof is.
    pub proof_bytes: usize,
    /// A digest of the proof, so a screen can point at it without printing it.
    pub proof_digest: [u8; 32],
    /// The amount: never available.
    pub amount: Option<u64>,
    /// The sender: never available.
    pub sender_address: Option<[u8; 32]>,
    /// The recipient: never available.
    pub recipient_address: Option<[u8; 32]>,
    /// Whether the proof checks out, which is the one thing it can always settle.
    pub proof_accepted: bool,
}

/// What someone trying to get a false statement accepted has, and how far it got.
#[derive(Clone, Debug)]
pub struct AttackerView {
    /// Which party this is.
    pub party: Party,
    /// What it holds. Empty except in stage 3, where it holds the toxic waste.
    pub holds: Vec<Item>,
    /// What it can read off the chain, which is exactly what the onlooker sees.
    pub sees: Vec<Item>,
    /// What it would need and does not have.
    pub would_need: Vec<Item>,
    /// Every attempt it made in this stage, with the verifier's answer to each.
    pub attempts: Vec<crate::ForgeryAttempt>,
    /// Whether any attempt was accepted.
    pub any_accepted: bool,
}

/// The four views of one stage.
#[derive(Clone, Debug)]
pub struct PartyViews {
    /// The spender.
    pub sender: SenderView,
    /// The recipient.
    pub receiver: ReceiverView,
    /// Anyone reading the chain.
    pub onlooker: OnlookerView,
    /// Anyone attacking it.
    pub attacker: AttackerView,
}

/// What a stage hands over so the four views can be written.
#[derive(Clone, Copy)]
pub struct ViewInputs<'a> {
    /// Which stage this is.
    pub stage: Stage,
    /// The payment being told about.
    pub payment: &'a ShieldedPayment,
    /// The honest proof bytes.
    pub proof: &'a [u8],
    /// What the verifier said about the honest proof.
    pub honest_accepted: bool,
    /// What the stage's setup costs in trust.
    pub setup: SetupKind,
}

/// Writes the four views of one stage.
pub fn build(inputs: ViewInputs<'_>, forgery: &ForgeryOutcome) -> PartyViews {
    let payment = inputs.payment;
    let public_items =
        vec![Item::NoteCommitment, Item::Nullifier, Item::Proof, Item::PublicStatement];
    let private_items = vec![
        Item::Amount,
        Item::SenderAddress,
        Item::RecipientAddress,
        Item::NoteBlinding,
        Item::SpendingKey,
        Item::ViewingKey,
    ];

    let attacker_holds = match inputs.stage {
        Stage::TrustedSetup => vec![Item::ToxicWaste, Item::SetupParameters],
        _ => Vec::new(),
    };
    let attacker_would_need = match inputs.stage {
        Stage::Sigma | Stage::FiatShamir => vec![Item::SpendingKey],
        Stage::TrustedSetup => vec![Item::NoteBlinding],
        Stage::Halo2 => vec![Item::SpendingKey, Item::NoteBlinding],
    };

    PartyViews {
        sender: SenderView {
            party: Party::Sender,
            holds: vec![
                Item::SpendingKey,
                Item::ViewingKey,
                Item::NoteBlinding,
                Item::Amount,
                Item::RecipientAddress,
                Item::SenderAddress,
            ],
            learns: vec![Item::NoteCommitment, Item::Nullifier, Item::Proof],
            can_verify: vec![
                Claim::ProofVerifies,
                Claim::AmountMatchesCommitment,
                Claim::AmountInRange,
                Claim::SpenderHoldsKey,
            ],
            amount: payment.value,
            change: payment.change,
            recipient_address: payment.recipient_address,
            note_commitment: payment.note_commitment,
            nullifier: payment.nullifier,
            proof_accepted: inputs.honest_accepted,
        },
        receiver: ReceiverView {
            party: Party::Receiver,
            holds: vec![Item::ViewingKey, Item::NoteBlinding, Item::Amount, Item::NoteCommitment],
            learns: vec![Item::Amount, Item::NoteCommitment, Item::Proof],
            never_learns: vec![Item::SenderAddress, Item::SpendingKey],
            can_verify: vec![Claim::ProofVerifies, Claim::AmountMatchesCommitment],
            amount: payment.value,
            note_commitment: payment.note_commitment,
            sender_address: None,
            proof_accepted: inputs.honest_accepted,
        },
        onlooker: OnlookerView {
            party: Party::Onlooker,
            holds: Vec::new(),
            sees: public_items.clone(),
            cannot_see: private_items,
            can_verify: vec![
                Claim::TransactionHappened,
                Claim::ProofVerifies,
                Claim::NullifierUnseen,
            ],
            transaction_seen: true,
            note_commitment: payment.note_commitment,
            nullifier: payment.nullifier,
            proof_bytes: inputs.proof.len(),
            proof_digest: curve::digest(domain::PROOF_DIGEST, &[inputs.proof]),
            amount: None,
            sender_address: None,
            recipient_address: None,
            proof_accepted: inputs.honest_accepted,
        },
        attacker: AttackerView {
            party: Party::Attacker,
            holds: attacker_holds,
            sees: public_items,
            would_need: attacker_would_need,
            attempts: forgery.attempts.clone(),
            any_accepted: forgery.any_accepted,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Seed;

    #[test]
    fn the_scenario_is_reproducible() {
        let a = scenario(DEFAULT_VALUE, DEFAULT_CHANGE, &mut DeterministicRng::new(Seed::fixed()));
        let b = scenario(DEFAULT_VALUE, DEFAULT_CHANGE, &mut DeterministicRng::new(Seed::fixed()));
        assert_eq!(a.note_commitment, b.note_commitment);
        assert_eq!(a.nullifier, b.nullifier);
    }

    #[test]
    fn a_different_amount_moves_the_commitment() {
        let a = scenario(1, DEFAULT_CHANGE, &mut DeterministicRng::new(Seed::fixed()));
        let b = scenario(2, DEFAULT_CHANGE, &mut DeterministicRng::new(Seed::fixed()));
        assert_ne!(a.note_commitment, b.note_commitment);
    }

    #[test]
    fn the_input_value_is_the_amount_plus_the_change() {
        let payment = scenario(7, 3, &mut DeterministicRng::new(Seed::fixed()));
        assert_eq!(payment.input_value, 10);
    }
}
