//! Every word this quest says, English first.
//!
//! The engine hands back enums — `CheckStep::Freshness`, `RejectionKind::NonceMismatch` — and this
//! table is where they become something a reader understands. Keeping the two apart is what lets a
//! third language arrive without the ledger code noticing.

use nmtk_ledger::{CheckStep, EntryKind, Model, RejectionKind};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title { en: "Ledger models", ko: "원장 방식" },
    Summary { en: "Send the same coin under three sets of rules and watch them disagree.", ko: "같은 코인을 세 가지 규칙으로 보내고, 셋이 어떻게 갈리는지 봅니다." },
    Subcategory { en: "Transaction models", ko: "거래 모델" },
    // ---- Brief -----------------------------------------------------------------
    BriefOpening { en: "Every chain has to answer one question: where is the money?", ko: "모든 체인은 한 가지 질문에 답해야 합니다. 돈이 어디에 있는가?" },
    BriefUtxo { en: "Bitcoin keeps a pile of coins. Each was created by some transaction and can be spent exactly once. Your balance is whatever coins nobody has spent yet.", ko: "비트코인은 코인 더미를 들고 있습니다. 각 코인은 어떤 거래가 만들었고 딱 한 번만 쓸 수 있습니다. 잔액이란 아직 아무도 쓰지 않은 코인들입니다." },
    BriefAccount { en: "Ethereum keeps a ledger book. One line per address, with a balance and a counter. Sending subtracts from one line and adds to another.", ko: "이더리움은 장부를 들고 있습니다. 주소마다 한 줄, 그 줄에 잔액과 번호가 있습니다. 보내면 한 줄에서 빼고 다른 줄에 더합니다." },
    BriefObject { en: "Sui keeps things. Every coin is an object with an owner and a version number, and sending hands the object over.", ko: "Sui는 개별 물체를 들고 있습니다. 코인마다 주인과 판 번호가 붙은 객체이고, 보낸다는 것은 그 객체를 넘기는 일입니다." },
    BriefPromise { en: "The same transfer goes into all three. Watch what each one has to read, what it writes, and how much bigger the state gets.", ko: "같은 이체가 셋 모두에 들어갑니다. 각각이 무엇을 읽어야 하고, 무엇을 쓰고, 상태가 얼마나 커지는지 보세요." },
    // ---- Run -------------------------------------------------------------------
    RunTitle { en: "One transfer, three answers", ko: "이체 하나, 답 셋" },
    RunIdle { en: "Press Enter to send.", ko: "Enter를 눌러 보냅니다." },
    BriefNext { en: "Press Enter to begin.", ko: "Enter를 눌러 시작합니다." },
    RunOpening { en: "Alice holds three coins of 10. Bob and Carol hold nothing. A pool holds 100 that anyone may pay into.", ko: "앨리스는 10짜리 코인 셋을 갖고 있습니다. 밥과 캐럴은 아무것도 없습니다. 공용 풀에는 누구나 넣을 수 있는 100이 있습니다." },
    RunAfter { en: "Three models, one transfer. The balances agree; almost nothing else does.", ko: "같은 이체를 셋이 처리했습니다. 잔액은 일치하지만, 그 밖에는 거의 일치하지 않습니다." },
    ColumnState { en: "State", ko: "상태" },
    ColumnEntries { en: "entries", ko: "항목" },
    ColumnSize { en: "Size", ko: "크기" },
    ColumnReads { en: "Reads", ko: "읽기" },
    ColumnWrites { en: "Writes", ko: "쓰기" },
    ColumnGrowth { en: "growth", ko: "증가" },
    ColumnBalance { en: "Alice", ko: "앨리스" },
    LabelAccepted { en: "accepted", ko: "받아들임" },
    LabelRejected { en: "rejected", ko: "거절함" },
    // ---- Tune ------------------------------------------------------------------
    TuneTitle { en: "Change it and send again", ko: "바꿔서 다시 보내기" },
    TuneHint { en: "Left and right change a value; type digits for your own number and press Enter.", ko: "왼쪽·오른쪽으로 값을 바꾸고, 직접 숫자를 입력한 뒤 Enter를 눌러도 됩니다." },
    KnobAmount { en: "Amount", ko: "금액" },
    KnobCoin { en: "Coin to spend", ko: "쓸 코인" },
    KnobRecipient { en: "Recipient", ko: "받는 사람" },
    CoinAutomatic { en: "let the model choose", ko: "모델에게 맡기기" },
    CoinFirst { en: "the first coin", ko: "첫 번째 코인" },
    CoinSecond { en: "the second coin", ko: "두 번째 코인" },
    CoinThird { en: "the third coin", ko: "세 번째 코인" },
    PartyBob { en: "Bob", ko: "밥" },
    PartyCarol { en: "Carol", ko: "캐럴" },
    PartyPool { en: "the shared pool", ko: "공용 풀" },
    TuneNote { en: "The account model ignores the coin you pick. That is the lesson: an account is one balance, so there is nothing to choose.", ko: "계정 방식은 어떤 코인을 골랐는지 무시합니다. 그것이 요점입니다. 계정은 잔액 하나라서 고를 것이 없습니다." },
    // ---- Break -----------------------------------------------------------------
    BreakTitle { en: "Spend it twice", ko: "두 번 쓰기" },
    BreakHint { en: "Press Enter to write two transfers against the same state and send both.", ko: "Enter를 누르면 같은 상태를 보고 쓴 이체 두 건을 만들어 둘 다 보냅니다." },
    BreakExplain { en: "Both transfers are written before either is sent, so both look valid when they are made. Every model takes the first. Watch where each one catches the second.", ko: "두 이체 모두 보내기 전에 작성되므로, 만들어질 때는 둘 다 유효해 보입니다. 세 방식 모두 첫 번째는 받아들입니다. 두 번째를 각각 어디서 잡아내는지 보세요." },
    BreakStopped { en: "stopped", ko: "막음" },
    BreakNotStopped { en: "let through", ko: "통과시킴" },
    BreakAllStopped { en: "All three stopped it — at three different steps, for three different reasons.", ko: "셋 다 막았습니다. 서로 다른 단계에서, 서로 다른 이유로." },
    // ---- Recap -----------------------------------------------------------------
    RecapTitle { en: "What just happened", ko: "방금 본 것" },
    RecapOne { en: "The three models agreed on every balance and on nothing else.", ko: "세 방식은 잔액에서만 일치했고 나머지는 전부 달랐습니다." },
    RecapTwo { en: "A double spend dies at a different step in each: the coin is gone, the counter is wrong, the version is old.", ko: "두 번 쓰기는 각각 다른 단계에서 죽습니다. 코인이 사라졌거나, 번호가 틀렸거나, 판이 낡았습니다." },
    RecapThree { en: "Two payments from one sender can run at the same time in two of the models and must queue in the third.", ko: "한 사람이 보내는 두 결제는 두 방식에서는 동시에 처리되고, 나머지 하나에서는 줄을 서야 합니다." },
    RecapParallel { en: "Can two run at once?", ko: "둘이 동시에 될까?" },
    RecapFromOne { en: "From one sender", ko: "한 사람이 둘 보낼 때" },
    RecapToOne { en: "To one recipient", ko: "한 사람이 둘 받을 때" },
    Yes { en: "yes", ko: "예" },
    No { en: "no", ko: "아니오" },
    // ---- Model names -----------------------------------------------------------
    ModelUtxo { en: "UTXO", ko: "UTXO" },
    ModelAccount { en: "Account", ko: "계정" },
    ModelObject { en: "Object", ko: "객체" },
    EntryUnspentOutput { en: "unspent outputs", ko: "쓰지 않은 출력" },
    EntryAccount { en: "accounts", ko: "계정" },
    EntryObject { en: "objects", ko: "객체" },
    // ---- Validation steps ------------------------------------------------------
    StepBuild { en: "writing it", ko: "작성" },
    StepShape { en: "reading it", ko: "형식 검사" },
    StepStateLookup { en: "looking it up", ko: "상태 조회" },
    StepAuthorization { en: "checking the owner", ko: "소유 확인" },
    StepFreshness { en: "checking it is current", ko: "최신 여부 확인" },
    StepValue { en: "adding it up", ko: "금액 확인" },
    // ---- Reasons ---------------------------------------------------------------
    WhyZeroAmount { en: "the amount is zero", ko: "금액이 0입니다" },
    WhyNoInputs { en: "there is nothing to spend", ko: "쓸 것이 없습니다" },
    WhyTooManyOutputs { en: "too many outputs", ko: "출력이 너무 많습니다" },
    WhyDuplicateInput { en: "the same coin is named twice", ko: "같은 코인을 두 번 적었습니다" },
    WhyInputNotFound { en: "that coin is already spent", ko: "그 코인은 이미 쓰였습니다" },
    WhyBadSignature { en: "the signature does not match", ko: "서명이 맞지 않습니다" },
    WhyNotOwner { en: "the sender does not own it", ko: "보낸 사람의 것이 아닙니다" },
    WhyValueNotConserved { en: "the amounts do not add up", ko: "금액이 맞아떨어지지 않습니다" },
    WhyAmountOverflow { en: "the amount is too large", ko: "금액이 너무 큽니다" },
    WhyAccountNotFound { en: "there is no such account", ko: "그런 계정이 없습니다" },
    WhyNonceMismatch { en: "the counter has already moved on", ko: "번호가 이미 넘어갔습니다" },
    WhyInsufficientBalance { en: "the balance is too small", ko: "잔액이 모자랍니다" },
    WhyObjectNotFound { en: "there is no such object", ko: "그런 객체가 없습니다" },
    WhyNotSharedObject { en: "that object is not shared", ko: "공용 객체가 아닙니다" },
    WhyStaleObjectVersion { en: "the object has moved on to a newer version", ko: "객체가 이미 다음 판으로 넘어갔습니다" },
    WhyNoCoinAvailable { en: "no coin is free to spend", ko: "쓸 수 있는 코인이 없습니다" },
    WhyCoinTooSmall { en: "that coin is too small", ko: "그 코인은 너무 작습니다" },
    WhyNoSuchCoin { en: "there is no coin there", ko: "거기에 코인이 없습니다" },
    WhyModelMismatch { en: "that belongs to another model", ko: "다른 방식의 것입니다" },
    // ---- Conflicts -------------------------------------------------------------
    ConflictIndependent { en: "independent", ko: "서로 무관" },
    ConflictWriteWrite { en: "both write the same entry", ko: "같은 항목에 둘 다 씁니다" },
    ConflictReadWrite { en: "one writes what the other reads", ko: "한쪽이 쓰는 것을 다른 쪽이 읽습니다" },
    ConflictModelMismatch { en: "not comparable", ko: "비교할 수 없음" },
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
