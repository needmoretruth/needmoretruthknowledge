//! Every word this quest says.
//!
//! English is written here and English is never missing; the Korean column is added later and any
//! line without one falls back to the English rather than to a blank. The engine hands back enums —
//! `ForgeryKind::ToxicWasteOpening`, `Item::Nullifier`, `sigma::Step::Challenge` — and this table is
//! the only place they become something a reader understands.

use nmtk_zk::sigma::{Side, Step};
use nmtk_zk::{Claim, ForgeryKind, Item, SetupKind, Stage, ZkError};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title: "Zero-knowledge proofs",
    Summary: "Prove you know a secret without showing it, from sigma protocols to halo2, seen from four sides.",
    Subcategory: "Zero-knowledge",

    // ---- Brief -----------------------------------------------------------------
    BriefOpening: "A zero-knowledge proof convinces someone that a statement is true while showing them nothing but the proof.",
    BriefEveryday: "Think of proving you are old enough to buy a drink without handing over your birthday, your name and your address. The clerk learns one thing and nothing else.",
    BriefChain: "A public chain is the hardest place to keep a secret. Every node checks every payment, so every node has to be given enough to check it — who paid, who was paid, how much.",
    BriefWhy: "A zero-knowledge proof breaks that trade. The node checks a proof instead of the payment, and the amounts and the addresses never go on the chain at all.",
    BriefLineage: "Four systems run here, in the order the field arrived at them. Each one fixes something the one before it could not, and each one asks you to trust something different.",
    BriefPromise: "You will see the three numbers that separate them — how long a proof takes to make, how long it takes to check, and how big it is — and then you will try to break all four.",
    BriefPanelTitle: "The four systems, in order",
    BriefSigma: "Three messages, and the verifier has to be there while you prove.",
    BriefFiatShamir: "The verifier's coin flip becomes a hash. The conversation becomes a file anyone can check later.",
    BriefTrustedSetup: "Short proofs, but sound only because a number was destroyed after the setup.",
    BriefHalo2: "A real circuit, really compiled here, with no setup to trust.",
    BriefStart: "Press 2 to run all four on this machine.",

    // ---- Stage names -----------------------------------------------------------
    StageSigma: "Sigma",
    StageFiatShamir: "Fiat-Shamir",
    StageTrustedSetup: "Trusted setup",
    StageHalo2: "halo2",
    StageAll: "all four",

    // ---- Run -------------------------------------------------------------------
    RunTitle: "Four systems, three numbers",
    RunIdle: "Press Enter to run all four on this machine.",
    RunWorking: "running on this machine...",
    RunExplainOne: "All four run here, one after another, each proving something about the same kind of shielded payment.",
    RunExplainTwo: "Proving time, verification time and proof size are the honest difference between these systems. A proof that is slow to check is no use to a chain, and a proof that is large is no use to anyone.",
    RunExplainThree: "Under the table the first system is laid out message by message. Press Enter to walk through it.",
    ColumnStage: "Stage",
    ColumnProve: "Prove",
    ColumnVerify: "Verify",
    ColumnSize: "Size",
    NotRunYet: "—",

    // ---- The three messages ----------------------------------------------------
    MessageLabel: "Message",
    StepStatement: "Public statement",
    StepCommitment: "Commitment",
    StepChallenge: "Challenge",
    StepResponse: "Response",
    StepVerdict: "Verdict",
    SideProver: "from the prover",
    SideVerifier: "from the verifier",
    SidePublic: "agreed beforehand",
    WhyStatement: "P = x times G. Everyone knows the point P. Only the prover knows the number x behind it.",
    WhyCommitment: "The prover picks a random r and sends R = r times G. Nothing about x has been said yet.",
    WhyChallenge: "Only now, with R already in hand, does the verifier draw a fresh number c. The prover could not have prepared an answer for it.",
    WhyResponse: "The prover answers s = r + c times x. Without x there is nothing to answer with, and s on its own hides x behind r.",
    WhyVerdict: "The verifier checks that s times G equals R plus c times P. It never sees x, and it never learns it.",
    KeyNextMessage: "next message",

    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Your run",
    TuneExplainOne: "Pick one system or run all four. The circuit size is how wide halo2 holds each amount, in bits: a wider range is a bigger circuit and a slower proof, which is the thing to feel here.",
    TuneExplainTwo: "The seed decides every random number in the run, so the same seed replays the same proof byte for byte and a different seed does not.",
    TuneExplainThree: "Left and right move a value. Type digits and press Enter for a number of your own; anything outside the range is refused and the old value stays.",
    KnobStage: "System",
    KnobBits: "Circuit size",
    KnobSeed: "Seed",
    UnitBits: "bits",
    TuneOnlyHalo2: "Circuit size changes halo2 only.",
    ShapeTitle: "The circuit halo2 built",
    ShapeRows: "rows",
    ShapeRowsUsed: "rows used",
    ShapeAdvice: "advice columns",
    ShapeGates: "custom gates",
    ShapeDegree: "degree",
    ShapeLargest: "largest amount",
    ShapeSetup: "setup",
    KeyRunAgain: "run again",

    // ---- Break -----------------------------------------------------------------
    BreakTitle: "What the attacker got",
    BreakIdle: "Press Enter to run every attack against the real verifiers.",
    BreakExplainOne: "Every attack here is code running against the same verifier the honest proof went through. Nothing is decided in advance.",
    BreakExplainTwo: "Two of them get through. One is a Fiat-Shamir challenge hashed from the statement but not from the commitment, which lets an attacker choose its answer first and solve for a commitment that fits.",
    BreakExplainThree: "The other is the holder of a trusted setup's leftover randomness, opening a commitment at a value it does not hold. The verifier is not modified for either one.",
    BreakExplainFour: "That is why people ask whether a ceremony was honest. An unaudited ceremony is a promise, not a proof.",
    ForgeGuessed: "guessed the response",
    ForgeWeakBinding: "hash that skips the commitment",
    ForgeFullBinding: "hash that covers the commitment",
    ForgeWithWaste: "kept the setup randomness",
    ForgeWithoutWaste: "the same move without it",
    ForgeOverspend: "spent more than the note held",
    VerdictAccepted: "accepted",
    VerdictRejected: "rejected",
    WasteHolds: "The commitment holds",
    WasteOpened: "It was opened as",
    WasteVerifier: "The unchanged verifier said",
    WasteNote: "Nothing in the verifier was touched. The number that was supposed to be destroyed is the whole difference.",
    WordTried: "tried",
    BreakSummaryAccepted: "accepted",
    KeyRunAttacks: "run the attacks",

    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "One payment, four sides",
    RecapExplainOne: "One shielded payment, and four people looking at it at the same time.",
    RecapExplainTwo: "The onlooker is the reason any of this exists. On a shielded chain they see that a transaction happened, the proof, a nullifier retiring the spent note, and a commitment standing in for the new one.",
    RecapExplainThree: "They do not see the amount, the sender or the recipient. On a transparent chain all three are public to everyone, forever.",
    RecapExplainFour: "Up and down move between the four systems. The attacker's rows are the ones that change.",
    PartySender: "Sender",
    PartyReceiver: "Receiver",
    PartyOnlooker: "Onlooker",
    PartyAttacker: "Attacker",
    LabelHolds: "holds",
    LabelLearns: "learns",
    LabelSees: "sees",
    LabelNever: "never",
    LabelChecks: "checks",
    LabelNeeds: "needs",
    WordSends: "sends",
    WordKeeps: "keeps",
    WordGets: "gets",
    WordUnknown: "unknown",
    WordNothing: "nothing",
    WordProof: "proof",
    KeySystem: "system",

    // ---- Things a party may hold -----------------------------------------------
    ItemSpendingKey: "spend key",
    ItemViewingKey: "view key",
    ItemNoteBlinding: "blinding",
    ItemAmount: "amount",
    ItemSenderAddress: "sender",
    ItemRecipientAddress: "recipient",
    ItemNoteCommitment: "commitment",
    ItemNullifier: "nullifier",
    ItemProof: "proof",
    ItemPublicStatement: "statement",
    ItemSetupParameters: "parameters",
    ItemToxicWaste: "toxic waste",

    // ---- Things a party can check ----------------------------------------------
    ClaimHappened: "it happened",
    ClaimProofVerifies: "proof",
    ClaimAmountInRange: "range",
    ClaimAmountMatches: "amount",
    ClaimNullifierUnseen: "nullifier is new",
    ClaimSpenderHoldsKey: "the key",

    // ---- What a system needs set up --------------------------------------------
    SetupNone: "nothing to set up",
    SetupTransparent: "public parameters anyone can rebuild",
    LabelTrust: "setup",
    SetupToxic: "a number that had to be destroyed",

    // ---- When a run stops ------------------------------------------------------
    ErrorTitle: "The run stopped",
    ErrOutOfOrder: "a protocol step was taken out of order",
    ErrBadPoint: "those bytes are not a point on the curve",
    ErrBadScalar: "those bytes are not a number in the field",
    ErrCeremonyEmpty: "a ceremony with nobody in it",
    ErrWasteZero: "the setup randomness came out zero",
    ErrValueOutOfRange: "the amount does not fit the circuit's range",
    ErrBitsUnsupported: "no circuit is built for that width",
    ErrKeygen: "the keys could not be built",
    ErrProve: "the proof could not be made",
    ErrVerify: "the proof could not be checked",
    ErrNoThread: "this machine would not give the run a thread",
}

/// The name a system goes by on screen.
pub fn stage(stage: Stage) -> Msg {
    match stage {
        Stage::Sigma => Msg::StageSigma,
        Stage::FiatShamir => Msg::StageFiatShamir,
        Stage::TrustedSetup => Msg::StageTrustedSetup,
        Stage::Halo2 => Msg::StageHalo2,
    }
}

/// The one-line description under each system in the brief.
pub fn stage_brief(stage: Stage) -> Msg {
    match stage {
        Stage::Sigma => Msg::BriefSigma,
        Stage::FiatShamir => Msg::BriefFiatShamir,
        Stage::TrustedSetup => Msg::BriefTrustedSetup,
        Stage::Halo2 => Msg::BriefHalo2,
    }
}

/// Which message of the interactive protocol this is.
pub fn step(step: Step) -> Msg {
    match step {
        Step::Statement => Msg::StepStatement,
        Step::Commitment => Msg::StepCommitment,
        Step::Challenge => Msg::StepChallenge,
        Step::Response => Msg::StepResponse,
        Step::Verdict => Msg::StepVerdict,
    }
}

/// Why that message is there at all.
pub fn step_why(step: Step) -> Msg {
    match step {
        Step::Statement => Msg::WhyStatement,
        Step::Commitment => Msg::WhyCommitment,
        Step::Challenge => Msg::WhyChallenge,
        Step::Response => Msg::WhyResponse,
        Step::Verdict => Msg::WhyVerdict,
    }
}

/// Who put a message on the wire.
pub fn side(side: Side) -> Msg {
    match side {
        Side::Prover => Msg::SideProver,
        Side::Verifier => Msg::SideVerifier,
        Side::Public => Msg::SidePublic,
    }
}

/// What the attacker tried.
pub fn forgery(kind: ForgeryKind) -> Msg {
    match kind {
        ForgeryKind::GuessedResponse => Msg::ForgeGuessed,
        ForgeryKind::WeakFiatShamirRebind => Msg::ForgeWeakBinding,
        ForgeryKind::CorrectFiatShamirRebind => Msg::ForgeFullBinding,
        ForgeryKind::ToxicWasteOpening => Msg::ForgeWithWaste,
        ForgeryKind::OpeningWithoutToxicWaste => Msg::ForgeWithoutWaste,
        ForgeryKind::OverspendOutOfRange => Msg::ForgeOverspend,
    }
}

/// A thing a party may or may not hold.
pub fn item(item: Item) -> Msg {
    match item {
        Item::SpendingKey => Msg::ItemSpendingKey,
        Item::ViewingKey => Msg::ItemViewingKey,
        Item::NoteBlinding => Msg::ItemNoteBlinding,
        Item::Amount => Msg::ItemAmount,
        Item::SenderAddress => Msg::ItemSenderAddress,
        Item::RecipientAddress => Msg::ItemRecipientAddress,
        Item::NoteCommitment => Msg::ItemNoteCommitment,
        Item::Nullifier => Msg::ItemNullifier,
        Item::Proof => Msg::ItemProof,
        Item::PublicStatement => Msg::ItemPublicStatement,
        Item::SetupParameters => Msg::ItemSetupParameters,
        Item::ToxicWaste => Msg::ItemToxicWaste,
    }
}

/// Something a party can settle for itself.
pub fn claim(claim: Claim) -> Msg {
    match claim {
        Claim::TransactionHappened => Msg::ClaimHappened,
        Claim::ProofVerifies => Msg::ClaimProofVerifies,
        Claim::AmountInRange => Msg::ClaimAmountInRange,
        Claim::AmountMatchesCommitment => Msg::ClaimAmountMatches,
        Claim::NullifierUnseen => Msg::ClaimNullifierUnseen,
        Claim::SpenderHoldsKey => Msg::ClaimSpenderHoldsKey,
    }
}

/// What a system needed before it could prove anything.
pub fn setup_kind(setup: SetupKind) -> Msg {
    match setup {
        SetupKind::NoneNeeded => Msg::SetupNone,
        SetupKind::Transparent => Msg::SetupTransparent,
        SetupKind::ToxicWaste => Msg::SetupToxic,
    }
}

/// Why a run stopped, in words rather than in the engine's developer English.
pub fn why_stopped(error: ZkError) -> Msg {
    use nmtk_zk::ProofStep;
    match error {
        ZkError::OutOfOrder => Msg::ErrOutOfOrder,
        ZkError::BadPointEncoding => Msg::ErrBadPoint,
        ZkError::BadScalarEncoding => Msg::ErrBadScalar,
        ZkError::CeremonyEmpty => Msg::ErrCeremonyEmpty,
        ZkError::ToxicWasteNotInvertible => Msg::ErrWasteZero,
        ZkError::ValueOutOfRange => Msg::ErrValueOutOfRange,
        ZkError::ValueBitsUnsupported => Msg::ErrBitsUnsupported,
        ZkError::Proving(ProofStep::Keygen) => Msg::ErrKeygen,
        ZkError::Proving(ProofStep::Prove) => Msg::ErrProve,
        ZkError::Proving(ProofStep::Verify) => Msg::ErrVerify,
    }
}
