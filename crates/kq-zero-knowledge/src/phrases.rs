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
    Title { en: "Zero-knowledge proofs", ko: "영지식 증명" },
    Summary { en: "Prove you know a secret without showing it, from sigma protocols to halo2, seen from four sides.", ko: "비밀을 보여 주지 않고 안다는 것만 증명합니다. 시그마 프로토콜부터 halo2까지, 네 사람의 눈으로." },
    Subcategory { en: "Zero-knowledge", ko: "영지식" },
    // ---- Brief -----------------------------------------------------------------
    BriefOpening { en: "A zero-knowledge proof convinces someone that a statement is true while showing them nothing but the proof.", ko: "영지식 증명은 어떤 주장이 참이라는 것을 상대에게 납득시키면서, 증명 말고는 아무것도 보여 주지 않습니다." },
    BriefEveryday { en: "Think of proving you are old enough to buy a drink without handing over your birthday, your name and your address. The clerk learns one thing and nothing else.", ko: "술을 살 나이가 됐다는 것을 생년월일·이름·주소를 건네지 않고 증명한다고 생각해 보세요. 점원은 딱 한 가지만 알게 되고 그 밖에는 아무것도 모릅니다." },
    BriefChain { en: "A public chain is the hardest place to keep a secret. Every node checks every payment, so every node has to be given enough to check it — who paid, who was paid, how much.", ko: "공개 체인은 비밀을 지키기 가장 어려운 곳입니다. 모든 노드가 모든 결제를 검사하므로, 모든 노드에게 검사에 필요한 것을 다 줘야 합니다 — 누가 냈고, 누가 받았고, 얼마인지." },
    BriefWhy { en: "A zero-knowledge proof breaks that trade. The node checks a proof instead of the payment, and the amounts and the addresses never go on the chain at all.", ko: "영지식 증명은 그 맞바꿈을 깹니다. 노드는 결제 대신 증명을 검사하고, 금액과 주소는 아예 체인에 올라가지 않습니다." },
    BriefLineage { en: "Four systems run here, in the order the field arrived at them. Each one fixes something the one before it could not, and each one asks you to trust something different.", ko: "여기서 네 가지 방식이 돕니다. 이 분야가 실제로 거쳐 온 순서 그대로입니다. 각각은 앞의 것이 풀지 못한 것을 풀고, 각각 다른 것을 믿으라고 요구합니다." },
    BriefPromise { en: "You will see the three numbers that separate them — how long a proof takes to make, how long it takes to check, and how big it is — and then you will try to break all four.", ko: "넷을 가르는 세 가지 숫자를 보게 됩니다 — 증명을 만드는 데 걸리는 시간, 검사하는 데 걸리는 시간, 그리고 증명의 크기. 그다음 넷 모두를 깨 봅니다." },
    BriefPanelTitle { en: "The four systems, in order", ko: "네 가지 방식, 나온 순서대로" },
    BriefSigma { en: "Three messages, and the verifier has to be there while you prove.", ko: "메시지 세 번. 증명하는 동안 검증자가 그 자리에 있어야 합니다." },
    BriefFiatShamir { en: "The verifier's coin flip becomes a hash. The conversation becomes a file anyone can check later.", ko: "검증자의 동전 던지기가 해시로 바뀝니다. 주고받던 대화가 누구나 나중에 검사할 수 있는 파일이 됩니다." },
    BriefTrustedSetup { en: "Short proofs, but sound only because a number was destroyed after the setup.", ko: "증명은 짧지만, 설정 뒤에 어떤 수를 없앴다는 전제에서만 안전합니다." },
    BriefHalo2 { en: "A real circuit, really compiled here, with no setup to trust.", ko: "진짜 회로를 여기서 실제로 컴파일합니다. 믿어야 할 설정이 없습니다." },
    BriefStart { en: "Press 2 to run all four on this machine.", ko: "2를 누르면 넷 모두 이 컴퓨터에서 돕니다." },
    // ---- Stage names -----------------------------------------------------------
    StageSigma { en: "Sigma", ko: "시그마" },
    StageFiatShamir { en: "Fiat-Shamir", ko: "피아트-샤미르" },
    StageTrustedSetup { en: "Trusted setup", ko: "신뢰 설정" },
    StageHalo2 { en: "halo2", ko: "halo2" },
    StageAll { en: "all four", ko: "넷 모두" },
    // ---- Run -------------------------------------------------------------------
    RunTitle { en: "Four systems, three numbers", ko: "네 가지 방식, 세 가지 숫자" },
    RunIdle { en: "Press Enter to run all four on this machine.", ko: "Enter를 누르면 넷 모두 이 컴퓨터에서 돕니다." },
    RunWorking { en: "running on this machine...", ko: "이 컴퓨터에서 도는 중..." },
    RunExplainOne { en: "All four run here, one after another, each proving something about the same kind of shielded payment.", ko: "넷이 여기서 차례로 돕니다. 모두 같은 종류의 가려진 결제에 대해 무언가를 증명합니다." },
    RunExplainTwo { en: "Proving time, verification time and proof size are the honest difference between these systems. A proof that is slow to check is no use to a chain, and a proof that is large is no use to anyone.", ko: "증명 시간, 검증 시간, 증명 크기가 이 방식들을 가르는 진짜 차이입니다. 검사가 느린 증명은 체인에 쓸 수 없고, 큰 증명은 누구에게도 쓸모가 없습니다." },
    RunExplainThree { en: "Under the table the first system is laid out message by message. Press Enter to walk through it.", ko: "표 아래에 첫 번째 방식이 메시지 하나하나로 펼쳐집니다. Enter를 눌러 따라가 보세요." },
    ColumnStage { en: "Stage", ko: "방식" },
    ColumnProve { en: "Prove", ko: "증명" },
    ColumnVerify { en: "Verify", ko: "검증" },
    ColumnSize { en: "Size", ko: "크기" },
    NotRunYet { en: "—" },
    // ---- The three messages ----------------------------------------------------
    MessageLabel { en: "Message", ko: "메시지" },
    StepStatement { en: "Public statement", ko: "공개된 주장" },
    StepCommitment { en: "Commitment", ko: "커밋먼트" },
    StepChallenge { en: "Challenge", ko: "챌린지" },
    StepResponse { en: "Response", ko: "응답" },
    StepVerdict { en: "Verdict", ko: "판단" },
    SideProver { en: "from the prover", ko: "증명자가 보냄" },
    SideVerifier { en: "from the verifier", ko: "검증자가 보냄" },
    SidePublic { en: "agreed beforehand", ko: "미리 합의된 값" },
    WhyStatement { en: "P = x times G. Everyone knows the point P. Only the prover knows the number x behind it.", ko: "P = x 곱하기 G입니다. 점 P는 누구나 압니다. 그 뒤에 있는 수 x는 증명자만 압니다." },
    WhyCommitment { en: "The prover picks a random r and sends R = r times G. Nothing about x has been said yet.", ko: "증명자가 무작위 수 r을 골라 R = r 곱하기 G를 보냅니다. 아직 x에 대해서는 아무 말도 하지 않았습니다." },
    WhyChallenge { en: "Only now, with R already in hand, does the verifier draw a fresh number c. The prover could not have prepared an answer for it.", ko: "R을 이미 손에 쥔 지금에서야 검증자가 새 수 c를 뽑습니다. 증명자는 그 수에 대한 답을 미리 준비해 둘 수 없었습니다." },
    WhyResponse { en: "The prover answers s = r + c times x. Without x there is nothing to answer with, and s on its own hides x behind r.", ko: "증명자가 s = r + c 곱하기 x로 답합니다. x가 없으면 답할 것이 없고, s 하나만으로는 r 뒤에 x가 가려집니다." },
    WhyVerdict { en: "The verifier checks that s times G equals R plus c times P. It never sees x, and it never learns it.", ko: "검증자는 s 곱하기 G가 R 더하기 c 곱하기 P와 같은지 봅니다. x를 본 적도 없고, 알게 되지도 않습니다." },
    KeyNextMessage { en: "next message", ko: "다음 메시지" },
    // ---- Tune ------------------------------------------------------------------
    TuneTitle { en: "Your run", ko: "내 설정으로 돌리기" },
    TuneExplainOne { en: "Pick one system or run all four. The circuit size is how wide halo2 holds each amount, in bits: a wider range is a bigger circuit and a slower proof, which is the thing to feel here.", ko: "방식 하나를 고르거나 넷 모두 돌립니다. 회로 크기는 halo2가 금액 하나를 몇 비트 폭으로 담는지입니다. 폭이 넓을수록 회로가 커지고 증명이 느려지는데, 그것을 몸으로 느껴 보는 것이 여기서 할 일입니다." },
    TuneExplainTwo { en: "The seed decides every random number in the run, so the same seed replays the same proof byte for byte and a different seed does not.", ko: "시드는 이 실행의 모든 무작위 수를 정합니다. 같은 시드는 같은 증명을 바이트 하나까지 똑같이 다시 만들고, 다른 시드는 그러지 않습니다." },
    TuneExplainThree { en: "Left and right move a value. Type digits and press Enter for a number of your own; anything outside the range is refused and the old value stays.", ko: "왼쪽·오른쪽으로 값을 옮깁니다. 직접 숫자를 입력하고 Enter를 눌러도 되며, 범위를 벗어나면 거절되고 원래 값이 남습니다." },
    KnobStage { en: "System", ko: "방식" },
    KnobBits { en: "Circuit size", ko: "회로 크기" },
    KnobSeed { en: "Seed", ko: "시드" },
    UnitBits { en: "bits", ko: "비트" },
    TuneOnlyHalo2 { en: "Circuit size changes halo2 only.", ko: "회로 크기는 halo2에만 영향을 줍니다." },
    ShapeTitle { en: "The circuit halo2 built", ko: "halo2가 만든 회로" },
    ShapeRows { en: "rows", ko: "행" },
    ShapeRowsUsed { en: "rows used", ko: "쓴 행" },
    ShapeAdvice { en: "advice columns", ko: "어드바이스 열" },
    ShapeGates { en: "custom gates", ko: "맞춤 게이트" },
    ShapeDegree { en: "degree", ko: "차수" },
    ShapeLargest { en: "largest amount", ko: "가장 큰 금액" },
    ShapeSetup { en: "setup", ko: "설정" },
    KeyRunAgain { en: "run again", ko: "다시 돌리기" },
    // ---- Break -----------------------------------------------------------------
    BreakTitle { en: "What the attacker got", ko: "공격자가 얻은 것" },
    BreakIdle { en: "Press Enter to run every attack against the real verifiers.", ko: "Enter를 누르면 모든 공격이 진짜 검증자를 상대로 돕니다." },
    BreakExplainOne { en: "Every attack here is code running against the same verifier the honest proof went through. Nothing is decided in advance.", ko: "여기 있는 공격은 전부, 정직한 증명이 통과했던 바로 그 검증자를 상대로 실제로 도는 코드입니다. 미리 정해진 결과는 없습니다." },
    BreakExplainTwo { en: "Two of them get through. One is a Fiat-Shamir challenge hashed from the statement but not from the commitment, which lets an attacker choose its answer first and solve for a commitment that fits.", ko: "그중 둘이 통과합니다. 하나는 챌린지를 주장에서만 해시하고 커밋먼트는 빼먹은 피아트-샤미르입니다. 그러면 공격자가 답을 먼저 고르고 거기 맞는 커밋먼트를 나중에 구할 수 있습니다." },
    BreakExplainThree { en: "The other is the holder of a trusted setup's leftover randomness, opening a commitment at a value it does not hold. The verifier is not modified for either one.", ko: "다른 하나는 신뢰 설정에서 남은 난수를 쥔 사람이, 커밋먼트를 그 안에 없는 값으로 여는 것입니다. 둘 다 검증자는 손대지 않았습니다." },
    BreakExplainFour { en: "That is why people ask whether a ceremony was honest. An unaudited ceremony is a promise, not a proof.", ko: "사람들이 「그 의식이 정직했나」를 따지는 이유가 이것입니다. 감사받지 않은 의식은 약속이지 증명이 아닙니다." },
    ForgeGuessed { en: "guessed the response", ko: "응답을 찍어 봄" },
    ForgeWeakBinding { en: "hash that skips the commitment", ko: "커밋먼트를 빠뜨린 해시" },
    ForgeFullBinding { en: "hash that covers the commitment", ko: "커밋먼트까지 덮은 해시" },
    ForgeWithWaste { en: "kept the setup randomness", ko: "설정 난수를 안 버림" },
    ForgeWithoutWaste { en: "the same move without it", ko: "난수 없이 같은 수법" },
    ForgeOverspend { en: "spent more than the note held", ko: "쪽지에 든 것보다 많이 씀" },
    VerdictAccepted { en: "accepted", ko: "통과" },
    VerdictRejected { en: "rejected", ko: "거절" },
    WasteHolds { en: "The commitment holds", ko: "커밋먼트에 든 값" },
    WasteOpened { en: "It was opened as", ko: "열어 보인 값" },
    WasteVerifier { en: "The unchanged verifier said", ko: "손대지 않은 검증자의 답" },
    WasteNote { en: "Nothing in the verifier was touched. The number that was supposed to be destroyed is the whole difference.", ko: "검증자는 한 글자도 바뀌지 않았습니다. 없앴어야 할 그 수 하나가 차이의 전부입니다." },
    WordTried { en: "tried", ko: "시도" },
    BreakSummaryAccepted { en: "accepted", ko: "통과" },
    KeyRunAttacks { en: "run the attacks", ko: "공격 돌리기" },
    // ---- Recap -----------------------------------------------------------------
    RecapTitle { en: "One payment, four sides", ko: "결제 하나, 네 사람의 눈" },
    RecapExplainOne { en: "One shielded payment, and four people looking at it at the same time.", ko: "가려진 결제 하나를 네 사람이 동시에 보고 있습니다." },
    RecapExplainTwo { en: "The onlooker is the reason any of this exists. On a shielded chain they see that a transaction happened, the proof, a nullifier retiring the spent note, and a commitment standing in for the new one.", ko: "지켜보는 사람이 이 모든 것이 존재하는 이유입니다. 가려진 체인에서 그 사람은 거래가 있었다는 것, 증명, 쓴 쪽지를 폐기하는 널리파이어, 새 쪽지를 대신하는 커밋먼트를 봅니다." },
    RecapExplainThree { en: "They do not see the amount, the sender or the recipient. On a transparent chain all three are public to everyone, forever.", ko: "금액도, 보낸 사람도, 받는 사람도 보이지 않습니다. 투명한 체인에서는 셋 다 모두에게 영원히 공개됩니다." },
    RecapExplainFour { en: "Up and down move between the four systems. The attacker's rows are the ones that change.", ko: "위아래로 네 방식 사이를 옮깁니다. 바뀌는 것은 공격자의 줄입니다." },
    PartySender { en: "Sender", ko: "보낸 사람" },
    PartyReceiver { en: "Receiver", ko: "받는 사람" },
    PartyOnlooker { en: "Onlooker", ko: "지켜보는 사람" },
    PartyAttacker { en: "Attacker", ko: "공격자" },
    LabelHolds { en: "holds", ko: "가진 것" },
    LabelLearns { en: "learns", ko: "알게 되는 것" },
    LabelSees { en: "sees", ko: "보이는 것" },
    LabelNever { en: "never", ko: "안 보이는 것" },
    LabelChecks { en: "checks", ko: "확인할 수 있는 것" },
    LabelNeeds { en: "needs", ko: "필요한 것" },
    WordSends { en: "sends", ko: "보냄" },
    WordKeeps { en: "keeps", ko: "가지고 있음" },
    WordGets { en: "gets", ko: "받음" },
    WordUnknown { en: "unknown", ko: "모름" },
    WordNothing { en: "nothing", ko: "없음" },
    WordProof { en: "proof", ko: "증명" },
    KeySystem { en: "system", ko: "방식" },
    // ---- Things a party may hold -----------------------------------------------
    ItemSpendingKey { en: "spend key", ko: "지출 키" },
    ItemViewingKey { en: "view key", ko: "열람 키" },
    ItemNoteBlinding { en: "blinding", ko: "블라인딩 값" },
    ItemAmount { en: "amount", ko: "금액" },
    ItemSenderAddress { en: "sender", ko: "보낸 주소" },
    ItemRecipientAddress { en: "recipient", ko: "받는 주소" },
    ItemNoteCommitment { en: "commitment", ko: "커밋먼트" },
    ItemNullifier { en: "nullifier", ko: "널리파이어" },
    ItemProof { en: "proof", ko: "증명" },
    ItemPublicStatement { en: "statement", ko: "공개된 주장" },
    ItemSetupParameters { en: "parameters", ko: "설정 값" },
    ItemToxicWaste { en: "toxic waste", ko: "버렸어야 할 난수" },
    // ---- Things a party can check ----------------------------------------------
    ClaimHappened { en: "it happened", ko: "거래가 있었다" },
    ClaimProofVerifies { en: "proof", ko: "증명" },
    ClaimAmountInRange { en: "range", ko: "범위" },
    ClaimAmountMatches { en: "amount", ko: "금액" },
    ClaimNullifierUnseen { en: "nullifier is new", ko: "처음 보는 널리파이어" },
    ClaimSpenderHoldsKey { en: "the key", ko: "키" },
    // ---- What a system needs set up --------------------------------------------
    SetupNone { en: "nothing to set up", ko: "설정할 것이 없음" },
    SetupTransparent { en: "public parameters anyone can rebuild", ko: "누구나 다시 만들 수 있는 공개 값" },
    LabelTrust { en: "setup", ko: "설정" },
    SetupToxic { en: "a number that had to be destroyed", ko: "없앴어야 하는 수 하나" },
    // ---- When a run stops ------------------------------------------------------
    ErrorTitle { en: "The run stopped", ko: "실행이 멈췄습니다" },
    ErrOutOfOrder { en: "a protocol step was taken out of order", ko: "프로토콜 단계가 순서를 벗어났습니다" },
    ErrBadPoint { en: "those bytes are not a point on the curve", ko: "그 바이트는 곡선 위의 점이 아닙니다" },
    ErrBadScalar { en: "those bytes are not a number in the field", ko: "그 바이트는 체 안의 수가 아닙니다" },
    ErrCeremonyEmpty { en: "a ceremony with nobody in it", ko: "참가자가 아무도 없는 의식입니다" },
    ErrWasteZero { en: "the setup randomness came out zero", ko: "설정 난수가 0으로 나왔습니다" },
    ErrValueOutOfRange { en: "the amount does not fit the circuit's range", ko: "금액이 회로의 범위에 들어가지 않습니다" },
    ErrBitsUnsupported { en: "no circuit is built for that width", ko: "그 폭으로 만들어진 회로가 없습니다" },
    ErrKeygen { en: "the keys could not be built", ko: "키를 만들지 못했습니다" },
    ErrProve { en: "the proof could not be made", ko: "증명을 만들지 못했습니다" },
    ErrVerify { en: "the proof could not be checked", ko: "증명을 검사하지 못했습니다" },
    ErrNoThread { en: "this machine would not give the run a thread", ko: "이 컴퓨터가 실행할 스레드를 내주지 않았습니다" },
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
