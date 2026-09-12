//! Every word this quest says, English first.
//!
//! The engine hands back enums — `CheckStep::Freshness`, `RejectionKind::NonceMismatch` — and this
//! table is where they become something a reader understands. Keeping the two apart is what lets a
//! third language arrive without the ledger code noticing.

use nmtk_ledger::{CheckStep, Conflict, EntryKind, Model, RejectionKind};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title: "Ledger models" => "원장 방식",
    Summary: "Send the same coin under three sets of rules and watch them disagree." => "같은 코인을 세 가지 규칙으로 보내고, 셋이 어떻게 갈리는지 봅니다.",
    Subcategory: "Transaction models" => "거래 모델",

    // ---- Brief -----------------------------------------------------------------
    BriefOpening: "Every chain has to answer one question: where is the money?" => "모든 체인은 한 가지 질문에 답해야 합니다. 돈이 어디에 있는가?",
    BriefUtxo: "Bitcoin keeps a pile of coins. Each was created by some transaction and can be spent exactly once. Your balance is whatever coins nobody has spent yet." => "비트코인은 코인 더미를 들고 있습니다. 각 코인은 어떤 거래가 만들었고 딱 한 번만 쓸 수 있습니다. 잔액이란 아직 아무도 쓰지 않은 코인들입니다.",
    BriefAccount: "Ethereum keeps a ledger book. One line per address, with a balance and a counter. Sending subtracts from one line and adds to another." => "이더리움은 장부를 들고 있습니다. 주소마다 한 줄, 그 줄에 잔액과 번호가 있습니다. 보내면 한 줄에서 빼고 다른 줄에 더합니다.",
    BriefObject: "Sui keeps things. Every coin is an object with an owner and a version number, and sending hands the object over." => "Sui는 개별 물체를 들고 있습니다. 코인마다 주인과 판 번호가 붙은 객체이고, 보낸다는 것은 그 객체를 넘기는 일입니다.",
    BriefPromise: "The same transfer goes into all three. Watch what each one has to read, what it writes, and how much bigger the state gets." => "같은 이체가 셋 모두에 들어갑니다. 각각이 무엇을 읽어야 하고, 무엇을 쓰고, 상태가 얼마나 커지는지 보세요.",

    // ---- Run -------------------------------------------------------------------
    RunTitle: "One transfer, three answers" => "이체 하나, 답 셋",
    RunIdle: "Press Enter to send." => "Enter를 눌러 보냅니다.",
    RunOpening: "Alice holds three coins of 10. Bob and Carol hold nothing. A pool holds 100 that anyone may pay into." => "앨리스는 10짜리 코인 셋을 갖고 있습니다. 밥과 캐럴은 아무것도 없습니다. 공용 풀에는 누구나 넣을 수 있는 100이 있습니다.",
    RunAfter: "Three models, one transfer. The balances agree; almost nothing else does." => "같은 이체를 셋이 처리했습니다. 잔액은 일치하지만, 그 밖에는 거의 일치하지 않습니다.",
    ColumnState: "State" => "상태",
    ColumnEntries: "Entries" => "항목",
    ColumnSize: "Size" => "크기",
    ColumnReads: "Reads" => "읽기",
    ColumnWrites: "Writes" => "쓰기",
    ColumnGrowth: "Growth" => "증가",
    ColumnBalance: "Alice" => "앨리스",
    LabelAccepted: "accepted" => "받아들임",
    LabelRejected: "rejected" => "거절함",

    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Change it and send again" => "바꿔서 다시 보내기",
    TuneHint: "Left and right change a value; type digits for your own number and press Enter." => "왼쪽·오른쪽으로 값을 바꾸고, 직접 숫자를 입력한 뒤 Enter를 눌러도 됩니다.",
    KnobAmount: "Amount" => "금액",
    KnobCoin: "Coin to spend" => "쓸 코인",
    KnobRecipient: "Recipient" => "받는 사람",
    CoinAutomatic: "let the model choose" => "모델에게 맡기기",
    CoinFirst: "the first coin" => "첫 번째 코인",
    CoinSecond: "the second coin" => "두 번째 코인",
    CoinThird: "the third coin" => "세 번째 코인",
    PartyBob: "Bob" => "밥",
    PartyCarol: "Carol" => "캐럴",
    PartyPool: "the shared pool" => "공용 풀",
    TuneNote: "The account model ignores the coin you pick. That is the lesson: an account is one balance, so there is nothing to choose." => "계정 방식은 어떤 코인을 골랐는지 무시합니다. 그것이 요점입니다. 계정은 잔액 하나라서 고를 것이 없습니다.",

    // ---- Break -----------------------------------------------------------------
    BreakTitle: "Spend it twice" => "두 번 쓰기",
    BreakHint: "Press Enter to write two transfers against the same state and send both." => "Enter를 누르면 같은 상태를 보고 쓴 이체 두 건을 만들어 둘 다 보냅니다.",
    BreakExplain: "Both transfers are written before either is sent, so both look valid when they are made. Every model takes the first. Watch where each one catches the second." => "두 이체 모두 보내기 전에 작성되므로, 만들어질 때는 둘 다 유효해 보입니다. 세 방식 모두 첫 번째는 받아들입니다. 두 번째를 각각 어디서 잡아내는지 보세요.",
    BreakStopped: "stopped" => "막음",
    BreakNotStopped: "let through" => "통과시킴",
    BreakAllStopped: "All three stopped it — at three different steps, for three different reasons." => "셋 다 막았습니다. 서로 다른 단계에서, 서로 다른 이유로.",

    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "What just happened" => "방금 본 것",
    RecapOne: "The three models agreed on every balance and on nothing else." => "세 방식은 잔액에서만 일치했고 나머지는 전부 달랐습니다.",
    RecapTwo: "A double spend dies at a different step in each: the coin is gone, the counter is wrong, the version is old." => "두 번 쓰기는 각각 다른 단계에서 죽습니다. 코인이 사라졌거나, 번호가 틀렸거나, 판이 낡았습니다.",
    RecapThree: "Two payments from one sender can run at the same time in two of the models and must queue in the third." => "한 사람이 보내는 두 결제는 두 방식에서는 동시에 처리되고, 나머지 하나에서는 줄을 서야 합니다.",
    RecapParallel: "Can two run at once?" => "둘이 동시에 될까?",
    RecapFromOne: "From one sender" => "한 사람이 둘 보낼 때",
    RecapToOne: "To one recipient" => "한 사람이 둘 받을 때",
    Yes: "yes" => "예",
    No: "no" => "아니오",

    // ---- Model names -----------------------------------------------------------
    ModelUtxo: "UTXO" => "UTXO",
    ModelAccount: "Account" => "계정",
    ModelObject: "Object" => "객체",
    EntryUnspentOutput: "unspent outputs" => "쓰지 않은 출력",
    EntryAccount: "accounts" => "계정",
    EntryObject: "objects" => "객체",

    // ---- Validation steps ------------------------------------------------------
    StepBuild: "writing it" => "작성",
    StepShape: "reading it" => "형식 검사",
    StepStateLookup: "looking it up" => "상태 조회",
    StepAuthorization: "checking the owner" => "소유 확인",
    StepFreshness: "checking it is current" => "최신 여부 확인",
    StepValue: "adding it up" => "금액 확인",

    // ---- Reasons ---------------------------------------------------------------
    WhyZeroAmount: "the amount is zero" => "금액이 0입니다",
    WhyNoInputs: "there is nothing to spend" => "쓸 것이 없습니다",
    WhyTooManyOutputs: "too many outputs" => "출력이 너무 많습니다",
    WhyDuplicateInput: "the same coin is named twice" => "같은 코인을 두 번 적었습니다",
    WhyInputNotFound: "that coin is already spent" => "그 코인은 이미 쓰였습니다",
    WhyBadSignature: "the signature does not match" => "서명이 맞지 않습니다",
    WhyNotOwner: "the sender does not own it" => "보낸 사람의 것이 아닙니다",
    WhyValueNotConserved: "the amounts do not add up" => "금액이 맞아떨어지지 않습니다",
    WhyAmountOverflow: "the amount is too large" => "금액이 너무 큽니다",
    WhyAccountNotFound: "there is no such account" => "그런 계정이 없습니다",
    WhyNonceMismatch: "the counter has already moved on" => "번호가 이미 넘어갔습니다",
    WhyInsufficientBalance: "the balance is too small" => "잔액이 모자랍니다",
    WhyObjectNotFound: "there is no such object" => "그런 객체가 없습니다",
    WhyNotSharedObject: "that object is not shared" => "공용 객체가 아닙니다",
    WhyStaleObjectVersion: "the object has moved on to a newer version" => "객체가 이미 다음 판으로 넘어갔습니다",
    WhyNoCoinAvailable: "no coin is free to spend" => "쓸 수 있는 코인이 없습니다",
    WhyCoinTooSmall: "that coin is too small" => "그 코인은 너무 작습니다",
    WhyNoSuchCoin: "there is no coin there" => "거기에 코인이 없습니다",
    WhyModelMismatch: "that belongs to another model" => "다른 방식의 것입니다",

    // ---- Conflicts -------------------------------------------------------------
    ConflictIndependent: "independent" => "서로 무관",
    ConflictWriteWrite: "both write the same entry" => "같은 항목에 둘 다 씁니다",
    ConflictReadWrite: "one writes what the other reads" => "한쪽이 쓰는 것을 다른 쪽이 읽습니다",
    ConflictModelMismatch: "not comparable" => "비교할 수 없음",
}

/// The name a model goes by on screen.
pub fn model(model: Model) -> Msg {
    match model {
        Model::Utxo => Msg::ModelUtxo,
        Model::Account => Msg::ModelAccount,
        Model::Object => Msg::ModelObject,
    }
}

/// What a model's state is made of.
pub fn entry_kind(kind: EntryKind) -> Msg {
    match kind {
        EntryKind::UnspentOutput => Msg::EntryUnspentOutput,
        EntryKind::Account => Msg::EntryAccount,
        EntryKind::Object => Msg::EntryObject,
    }
}

/// The step a transaction died at.
pub fn step(step: CheckStep) -> Msg {
    match step {
        CheckStep::Build => Msg::StepBuild,
        CheckStep::Shape => Msg::StepShape,
        CheckStep::StateLookup => Msg::StepStateLookup,
        CheckStep::Authorization => Msg::StepAuthorization,
        CheckStep::Freshness => Msg::StepFreshness,
        CheckStep::Value => Msg::StepValue,
    }
}

/// Why it died.
pub fn why(kind: RejectionKind) -> Msg {
    match kind {
        RejectionKind::ZeroAmount => Msg::WhyZeroAmount,
        RejectionKind::NoInputs => Msg::WhyNoInputs,
        RejectionKind::TooManyOutputs => Msg::WhyTooManyOutputs,
        RejectionKind::DuplicateInput => Msg::WhyDuplicateInput,
        RejectionKind::InputNotFound => Msg::WhyInputNotFound,
        RejectionKind::BadSignature => Msg::WhyBadSignature,
        RejectionKind::NotOwner => Msg::WhyNotOwner,
        RejectionKind::ValueNotConserved => Msg::WhyValueNotConserved,
        RejectionKind::AmountOverflow => Msg::WhyAmountOverflow,
        RejectionKind::AccountNotFound => Msg::WhyAccountNotFound,
        RejectionKind::NonceMismatch => Msg::WhyNonceMismatch,
        RejectionKind::InsufficientBalance => Msg::WhyInsufficientBalance,
        RejectionKind::ObjectNotFound => Msg::WhyObjectNotFound,
        RejectionKind::NotSharedObject => Msg::WhyNotSharedObject,
        RejectionKind::StaleObjectVersion => Msg::WhyStaleObjectVersion,
        RejectionKind::NoCoinAvailable => Msg::WhyNoCoinAvailable,
        RejectionKind::CoinTooSmall => Msg::WhyCoinTooSmall,
        RejectionKind::NoSuchCoin => Msg::WhyNoSuchCoin,
        RejectionKind::ModelMismatch => Msg::WhyModelMismatch,
    }
}

/// Whether two transfers collide, and how.
pub fn conflict(conflict: &Conflict) -> Msg {
    match conflict {
        Conflict::Independent => Msg::ConflictIndependent,
        Conflict::WriteWrite(_) => Msg::ConflictWriteWrite,
        Conflict::ReadWrite(_) => Msg::ConflictReadWrite,
        Conflict::ModelMismatch => Msg::ConflictModelMismatch,
    }
}
