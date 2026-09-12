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
    Title: "Zero-knowledge proofs" => "영지식 증명",
    Summary: "Prove you know a secret without showing it, from sigma protocols to halo2, seen from four sides." => "비밀을 보여 주지 않고 안다는 것만 증명합니다. 시그마 프로토콜부터 halo2까지, 네 사람의 눈으로.",
    Subcategory: "Zero-knowledge" => "영지식",
    // ---- Brief -----------------------------------------------------------------
    BriefOpening: "A zero-knowledge proof convinces someone that a statement is true while showing them nothing but the proof." => "영지식 증명은 어떤 주장이 참이라는 것을 상대에게 납득시키면서, 증명 말고는 아무것도 보여 주지 않습니다.",
    BriefEveryday: "Think of proving you are old enough to buy a drink without handing over your birthday, your name and your address. The clerk learns one thing and nothing else." => "술을 살 나이가 됐다는 것을 생년월일·이름·주소를 건네지 않고 증명한다고 생각해 보세요. 점원은 딱 한 가지만 알게 되고 그 밖에는 아무것도 모릅니다.",
    BriefChain: "A public chain is the hardest place to keep a secret. Every node checks every payment, so every node has to be given enough to check it — who paid, who was paid, how much." => "공개 체인은 비밀을 지키기 가장 어려운 곳입니다. 모든 노드가 모든 결제를 검사하므로, 모든 노드에게 검사에 필요한 것을 다 줘야 합니다 — 누가 냈고, 누가 받았고, 얼마인지.",
    BriefWhy: "A zero-knowledge proof breaks that trade. The node checks a proof instead of the payment, and the amounts and the addresses never go on the chain at all." => "영지식 증명은 그 맞바꿈을 깹니다. 노드는 결제 대신 증명을 검사하고, 금액과 주소는 아예 체인에 올라가지 않습니다.",
    BriefLineage: "Four systems run here, in the order the field arrived at them. Each one fixes something the one before it could not, and each one asks you to trust something different." => "여기서 네 가지 방식이 돕니다. 이 분야가 실제로 거쳐 온 순서 그대로입니다. 각각은 앞의 것이 풀지 못한 것을 풀고, 각각 다른 것을 믿으라고 요구합니다.",
    BriefPromise: "You will see the three numbers that separate them — how long a proof takes to make, how long it takes to check, and how big it is — and then you will try to break all four." => "넷을 가르는 세 가지 숫자를 보게 됩니다 — 증명을 만드는 데 걸리는 시간, 검사하는 데 걸리는 시간, 그리고 증명의 크기. 그다음 넷 모두를 깨 봅니다.",
    BriefPanelTitle: "The four systems, in order" => "네 가지 방식, 나온 순서대로",
    BriefSigma: "Three messages, and the verifier has to be there while you prove." => "메시지 세 번. 증명하는 동안 검증자가 그 자리에 있어야 합니다.",
    BriefFiatShamir: "The verifier's coin flip becomes a hash. The conversation becomes a file anyone can check later." => "검증자의 동전 던지기가 해시로 바뀝니다. 주고받던 대화가 누구나 나중에 검사할 수 있는 파일이 됩니다.",
    BriefTrustedSetup: "Short proofs, but sound only because a number was destroyed after the setup." => "증명은 짧지만, 설정 뒤에 어떤 수를 없앴다는 전제에서만 안전합니다.",
    BriefHalo2: "A real circuit, really compiled here, with no setup to trust." => "진짜 회로를 여기서 실제로 컴파일합니다. 믿어야 할 설정이 없습니다.",
    BriefStart: "Press 2 to run all four on this machine." => "2를 누르면 넷 모두 이 컴퓨터에서 돕니다.",
    // ---- Stage names -----------------------------------------------------------
    StageSigma: "Sigma" => "시그마",
    StageFiatShamir: "Fiat-Shamir" => "피아트-샤미르",
    StageTrustedSetup: "Trusted setup" => "신뢰 설정",
    StageHalo2: "halo2" => "halo2",
    StageAll: "all four" => "넷 모두",
    // ---- Run -------------------------------------------------------------------
    RunTitle: "Four systems, three numbers" => "네 가지 방식, 세 가지 숫자",
    RunIdle: "Press Enter to run all four on this machine." => "Enter를 누르면 넷 모두 이 컴퓨터에서 돕니다.",
    RunWorking: "running on this machine..." => "이 컴퓨터에서 도는 중...",
    RunExplainOne: "All four run here, one after another, each proving something about the same kind of shielded payment." => "넷이 여기서 차례로 돕니다. 모두 같은 종류의 가려진 결제에 대해 무언가를 증명합니다.",
    RunExplainTwo: "Proving time, verification time and proof size are the honest difference between these systems. A proof that is slow to check is no use to a chain, and a proof that is large is no use to anyone." => "증명 시간, 검증 시간, 증명 크기가 이 방식들을 가르는 진짜 차이입니다. 검사가 느린 증명은 체인에 쓸 수 없고, 큰 증명은 누구에게도 쓸모가 없습니다.",
    RunExplainThree: "Under the table the first system is laid out message by message. Press Enter to walk through it." => "표 아래에 첫 번째 방식이 메시지 하나하나로 펼쳐집니다. Enter를 눌러 따라가 보세요.",
    ColumnStage: "Stage" => "방식",
    ColumnProve: "Prove" => "증명",
    ColumnVerify: "Verify" => "검증",
    ColumnSize: "Size" => "크기",
    NotRunYet: "—",

    // ---- The three messages ----------------------------------------------------
    MessageLabel: "Message" => "메시지",
    StepStatement: "Public statement" => "공개된 주장",
    StepCommitment: "Commitment" => "커밋먼트",
    StepChallenge: "Challenge" => "챌린지",
    StepResponse: "Response" => "응답",
    StepVerdict: "Verdict" => "판단",
    SideProver: "from the prover" => "증명자가 보냄",
    SideVerifier: "from the verifier" => "검증자가 보냄",
    SidePublic: "agreed beforehand" => "미리 합의된 값",
    WhyStatement: "P = x times G. Everyone knows the point P. Only the prover knows the number x behind it." => "P = x 곱하기 G입니다. 점 P는 누구나 압니다. 그 뒤에 있는 수 x는 증명자만 압니다.",
    WhyCommitment: "The prover picks a random r and sends R = r times G. Nothing about x has been said yet." => "증명자가 무작위 수 r을 골라 R = r 곱하기 G를 보냅니다. 아직 x에 대해서는 아무 말도 하지 않았습니다.",
    WhyChallenge: "Only now, with R already in hand, does the verifier draw a fresh number c. The prover could not have prepared an answer for it." => "R을 이미 손에 쥔 지금에서야 검증자가 새 수 c를 뽑습니다. 증명자는 그 수에 대한 답을 미리 준비해 둘 수 없었습니다.",
    WhyResponse: "The prover answers s = r + c times x. Without x there is nothing to answer with, and s on its own hides x behind r." => "증명자가 s = r + c 곱하기 x로 답합니다. x가 없으면 답할 것이 없고, s 하나만으로는 r 뒤에 x가 가려집니다.",
    WhyVerdict: "The verifier checks that s times G equals R plus c times P. It never sees x, and it never learns it." => "검증자는 s 곱하기 G가 R 더하기 c 곱하기 P와 같은지 봅니다. x를 본 적도 없고, 알게 되지도 않습니다.",
    KeyNextMessage: "next message" => "다음 메시지",
    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Your run" => "내 설정으로 돌리기",
    TuneExplainOne: "Pick one system or run all four. The circuit size is how wide halo2 holds each amount, in bits: a wider range is a bigger circuit and a slower proof, which is the thing to feel here." => "방식 하나를 고르거나 넷 모두 돌립니다. 회로 크기는 halo2가 금액 하나를 몇 비트 폭으로 담는지입니다. 폭이 넓을수록 회로가 커지고 증명이 느려지는데, 그것을 몸으로 느껴 보는 것이 여기서 할 일입니다.",
    TuneExplainTwo: "The seed decides every random number in the run, so the same seed replays the same proof byte for byte and a different seed does not." => "시드는 이 실행의 모든 무작위 수를 정합니다. 같은 시드는 같은 증명을 바이트 하나까지 똑같이 다시 만들고, 다른 시드는 그러지 않습니다.",
    TuneExplainThree: "Left and right move a value. Type digits and press Enter for a number of your own; anything outside the range is refused and the old value stays." => "왼쪽·오른쪽으로 값을 옮깁니다. 직접 숫자를 입력하고 Enter를 눌러도 되며, 범위를 벗어나면 거절되고 원래 값이 남습니다.",
    KnobStage: "System" => "방식",
    KnobBits: "Circuit size" => "회로 크기",
    KnobSeed: "Seed" => "시드",
    UnitBits: "bits" => "비트",
    TuneOnlyHalo2: "Circuit size changes halo2 only." => "회로 크기는 halo2에만 영향을 줍니다.",
    ShapeTitle: "The circuit halo2 built" => "halo2가 만든 회로",
    ShapeRows: "rows" => "행",
    ShapeRowsUsed: "rows used" => "쓴 행",
    ShapeAdvice: "advice columns" => "어드바이스 열",
    ShapeGates: "custom gates" => "맞춤 게이트",
    ShapeDegree: "degree" => "차수",
    ShapeLargest: "largest amount" => "가장 큰 금액",
    ShapeSetup: "setup" => "설정",
    KeyRunAgain: "run again" => "다시 돌리기",
    // ---- Break -----------------------------------------------------------------
    BreakTitle: "What the attacker got" => "공격자가 얻은 것",
    BreakIdle: "Press Enter to run every attack against the real verifiers." => "Enter를 누르면 모든 공격이 진짜 검증자를 상대로 돕니다.",
    BreakExplainOne: "Every attack here is code running against the same verifier the honest proof went through. Nothing is decided in advance." => "여기 있는 공격은 전부, 정직한 증명이 통과했던 바로 그 검증자를 상대로 실제로 도는 코드입니다. 미리 정해진 결과는 없습니다.",
    BreakExplainTwo: "Two of them get through. One is a Fiat-Shamir challenge hashed from the statement but not from the commitment, which lets an attacker choose its answer first and solve for a commitment that fits." => "그중 둘이 통과합니다. 하나는 챌린지를 주장에서만 해시하고 커밋먼트는 빼먹은 피아트-샤미르입니다. 그러면 공격자가 답을 먼저 고르고 거기 맞는 커밋먼트를 나중에 구할 수 있습니다.",
    BreakExplainThree: "The other is the holder of a trusted setup's leftover randomness, opening a commitment at a value it does not hold. The verifier is not modified for either one." => "다른 하나는 신뢰 설정에서 남은 난수를 쥔 사람이, 커밋먼트를 그 안에 없는 값으로 여는 것입니다. 둘 다 검증자는 손대지 않았습니다.",
    BreakExplainFour: "That is why people ask whether a ceremony was honest. An unaudited ceremony is a promise, not a proof." => "사람들이 「그 의식이 정직했나」를 따지는 이유가 이것입니다. 감사받지 않은 의식은 약속이지 증명이 아닙니다.",
    ForgeGuessed: "guessed the response" => "응답을 찍어 봄",
    ForgeWeakBinding: "hash that skips the commitment" => "커밋먼트를 빠뜨린 해시",
    ForgeFullBinding: "hash that covers the commitment" => "커밋먼트까지 덮은 해시",
    ForgeWithWaste: "kept the setup randomness" => "설정 난수를 안 버림",
    ForgeWithoutWaste: "the same move without it" => "난수 없이 같은 수법",
    ForgeOverspend: "spent more than the note held" => "쪽지에 든 것보다 많이 씀",
    VerdictAccepted: "accepted" => "통과",
    VerdictRejected: "rejected" => "거절",
    WasteHolds: "The commitment holds" => "커밋먼트에 든 값",
    WasteOpened: "It was opened as" => "열어 보인 값",
    WasteVerifier: "The unchanged verifier said" => "손대지 않은 검증자의 답",
    WasteNote: "Nothing in the verifier was touched. The number that was supposed to be destroyed is the whole difference." => "검증자는 한 글자도 바뀌지 않았습니다. 없앴어야 할 그 수 하나가 차이의 전부입니다.",
    WordTried: "tried" => "시도",
    BreakSummaryAccepted: "accepted" => "통과",
    KeyRunAttacks: "run the attacks" => "공격 돌리기",
    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "One payment, four sides" => "결제 하나, 네 사람의 눈",
    RecapExplainOne: "One shielded payment, and four people looking at it at the same time." => "가려진 결제 하나를 네 사람이 동시에 보고 있습니다.",
    RecapExplainTwo: "The onlooker is the reason any of this exists. On a shielded chain they see that a transaction happened, the proof, a nullifier retiring the spent note, and a commitment standing in for the new one." => "지켜보는 사람이 이 모든 것이 존재하는 이유입니다. 가려진 체인에서 그 사람은 거래가 있었다는 것, 증명, 쓴 쪽지를 폐기하는 널리파이어, 새 쪽지를 대신하는 커밋먼트를 봅니다.",
    RecapExplainThree: "They do not see the amount, the sender or the recipient. On a transparent chain all three are public to everyone, forever." => "금액도, 보낸 사람도, 받는 사람도 보이지 않습니다. 투명한 체인에서는 셋 다 모두에게 영원히 공개됩니다.",
    RecapExplainFour: "Up and down move between the four systems. The attacker's rows are the ones that change." => "위아래로 네 방식 사이를 옮깁니다. 바뀌는 것은 공격자의 줄입니다.",
    PartySender: "Sender" => "보낸 사람",
    PartyReceiver: "Receiver" => "받는 사람",
    PartyOnlooker: "Onlooker" => "지켜보는 사람",
    PartyAttacker: "Attacker" => "공격자",
    LabelHolds: "holds" => "가진 것",
    LabelLearns: "learns" => "알게 되는 것",
    LabelSees: "sees" => "보이는 것",
    LabelNever: "never" => "안 보이는 것",
    LabelChecks: "checks" => "확인할 수 있는 것",
    LabelNeeds: "needs" => "필요한 것",
    WordSends: "sends" => "보냄",
    WordKeeps: "keeps" => "가지고 있음",
    WordGets: "gets" => "받음",
    WordUnknown: "unknown" => "모름",
    WordNothing: "nothing" => "없음",
    WordProof: "proof" => "증명",
    KeySystem: "system" => "방식",
    // ---- Things a party may hold -----------------------------------------------
    ItemSpendingKey: "spend key" => "지출 키",
    ItemViewingKey: "view key" => "열람 키",
    ItemNoteBlinding: "blinding" => "블라인딩 값",
    ItemAmount: "amount" => "금액",
    ItemSenderAddress: "sender" => "보낸 주소",
    ItemRecipientAddress: "recipient" => "받는 주소",
    ItemNoteCommitment: "commitment" => "커밋먼트",
    ItemNullifier: "nullifier" => "널리파이어",
    ItemProof: "proof" => "증명",
    ItemPublicStatement: "statement" => "공개된 주장",
    ItemSetupParameters: "parameters" => "설정 값",
    ItemToxicWaste: "toxic waste" => "버렸어야 할 난수",
    // ---- Things a party can check ----------------------------------------------
    ClaimHappened: "it happened" => "거래가 있었다",
    ClaimProofVerifies: "proof" => "증명",
    ClaimAmountInRange: "range" => "범위",
    ClaimAmountMatches: "amount" => "금액",
    ClaimNullifierUnseen: "nullifier is new" => "처음 보는 널리파이어",
    ClaimSpenderHoldsKey: "the key" => "키",
    // ---- What a system needs set up --------------------------------------------
    SetupNone: "nothing to set up" => "설정할 것이 없음",
    SetupTransparent: "public parameters anyone can rebuild" => "누구나 다시 만들 수 있는 공개 값",
    LabelTrust: "setup" => "설정",
    SetupToxic: "a number that had to be destroyed" => "없앴어야 하는 수 하나",
    // ---- When a run stops ------------------------------------------------------
    ErrorTitle: "The run stopped" => "실행이 멈췄습니다",
    ErrOutOfOrder: "a protocol step was taken out of order" => "프로토콜 단계가 순서를 벗어났습니다",
    ErrBadPoint: "those bytes are not a point on the curve" => "그 바이트는 곡선 위의 점이 아닙니다",
    ErrBadScalar: "those bytes are not a number in the field" => "그 바이트는 체 안의 수가 아닙니다",
    ErrCeremonyEmpty: "a ceremony with nobody in it" => "참가자가 아무도 없는 의식입니다",
    ErrWasteZero: "the setup randomness came out zero" => "설정 난수가 0으로 나왔습니다",
    ErrValueOutOfRange: "the amount does not fit the circuit's range" => "금액이 회로의 범위에 들어가지 않습니다",
    ErrBitsUnsupported: "no circuit is built for that width" => "그 폭으로 만들어진 회로가 없습니다",
    ErrKeygen: "the keys could not be built" => "키를 만들지 못했습니다",
    ErrProve: "the proof could not be made" => "증명을 만들지 못했습니다",
    ErrVerify: "the proof could not be checked" => "증명을 검사하지 못했습니다",
    ErrNoThread: "this machine would not give the run a thread" => "이 컴퓨터가 실행할 스레드를 내주지 않았습니다",
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
