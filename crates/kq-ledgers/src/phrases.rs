//! Every word this quest says, English first.
//!
//! These are beats in a conversation, not paragraphs in an article: one or two sentences each,
//! said after the reader presses Enter, about something that just happened on their machine. A
//! beat that needs three sentences is two beats.
//!
//! The engine hands back enums — `CheckStep::Freshness`, `RejectionKind::NonceMismatch` — and this
//! table is where they become something a reader understands.

use nmtk_ledger::{CheckStep, EntryKind, Model, RejectionKind};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title { en: "Ledger models", ko: "원장 방식" },
    Summary { en: "Send one coin under three sets of rules and watch them disagree.", ko: "같은 코인을 세 가지 규칙으로 보내고, 셋이 어떻게 갈리는지 봅니다." },
    Subcategory { en: "Transaction models", ko: "거래 모델" },

    // ---- Stage names -----------------------------------------------------------
    StageCoins { en: "Where money lives", ko: "돈이 있는 곳" },
    StageSend { en: "Send one", ko: "한 번 보내기" },
    StageGrow { en: "Change for a tenner", ko: "거스름돈" },
    StageTune { en: "Your turn", ko: "직접 해 보기" },
    StageTwice { en: "Spend it twice", ko: "두 번 쓰기" },
    StageRecap { en: "Recap", ko: "정리" },

    // ---- Stage 1: where money lives --------------------------------------------
    CoinsOne { en: "There is more than one way to write down where money is. The written record is called a ledger.", ko: "돈이 어디에 있는지 적는 방법은 하나가 아닙니다. 그렇게 적어 둔 기록을 원장이라고 부릅니다." },
    CoinsTwo { en: "Bitcoin counts coins. Your balance is whatever coins nobody has spent yet.", ko: "비트코인은 코인을 셉니다. 아직 아무도 쓰지 않은 코인이 곧 내 잔액입니다." },
    CoinsThree { en: "Ethereum keeps a book. One line per person, and the line says how much.", ko: "이더리움은 장부를 씁니다. 사람마다 한 줄이고, 그 줄에 얼마인지 적혀 있습니다." },
    CoinsSui { en: "Sui is a newer system of the same kind, put beside the two famous ones because it writes money down a third way.", ko: "Sui는 같은 종류의 비교적 새로운 시스템입니다. 돈을 적는 세 번째 방식이라 유명한 둘 옆에 나란히 놓았습니다." },
    CoinsFour { en: "Sui treats each coin as a thing with an owner written on it. Sending changes the owner.", ko: "Sui는 코인 하나하나를 주인이 적힌 물건처럼 다룹니다. 보낸다는 것은 그 주인을 바꾸는 일입니다." },
    CoinsFive { en: "All three are running on the right, holding the same thing: Alice has three coins of ten.", ko: "오른쪽에 셋이 다 돌고 있고, 셋 다 같은 것을 들고 있습니다. 앨리스에게 10짜리 코인이 셋 있습니다." },
    CoinsSix { en: "Watch the sizes. Right now they already disagree about how much space that takes.", ko: "크기를 보세요. 지금도 벌써 그것을 담는 데 드는 자리가 서로 다릅니다." },
    CoinsBytes { en: "B is bytes, the room a ledger takes up. A byte is eight 0s and 1s, and every node keeps every byte.", ko: "B는 바이트로, 원장이 차지하는 자리입니다. 바이트는 0과 1 여덟 개이고, 모든 노드가 그 바이트를 하나도 빠짐없이 들고 있습니다." },
    CoinsNode { en: "A node is one computer keeping its own copy of the ledger and checking every payment against it.", ko: "노드는 원장을 자기 사본으로 들고 있으면서 들어오는 결제를 하나하나 그 사본에 비춰 보는 컴퓨터 한 대입니다." },

    // ---- Stage 2: send one -----------------------------------------------------
    SendOne { en: "Alice sends Bob exactly ten — one whole coin, nothing left over.", ko: "앨리스가 밥에게 딱 10을 보냅니다. 코인 하나가 통째로 가고 남는 것이 없습니다." },
    SendTwo { en: "The same transfer goes into all three at once.", ko: "같은 이체를 셋에 동시에 넣습니다." },
    SendAccepted { en: "All three took it.", ko: "셋 다 받아들였습니다." },
    SendRefused { en: "Not all of them took it. The reason is beside each one.", ko: "전부가 받아들이지는 않았습니다. 이유가 각 줄에 적혀 있습니다." },
    SendUtxo { en: "Bitcoin's way: nothing new was stored. One coin changed hands whole.", ko: "비트코인 쪽은 새로 저장한 것이 없습니다. 코인 하나가 통째로 주인을 바꿨을 뿐입니다." },
    SendAccount { en: "Ethereum's way grew by one line: Bob had no line in the book, so one was written for him.", ko: "이더리움 쪽은 한 줄 늘었습니다. 장부에 밥의 줄이 없었으니 새로 써 준 것입니다." },
    SendObject { en: "Sui's way: nothing new either. The coin is the same thing, with a different owner on it.", ko: "Sui 쪽도 새로 생긴 것이 없습니다. 같은 물건에 적힌 주인만 바뀌었습니다." },
    SendAsk { en: "So far so similar. Now send an amount that does not fit a coin.", ko: "여기까지는 비슷합니다. 이번에는 코인 하나에 딱 맞지 않는 금액을 보내 봅니다." },

    // ---- Stage 3: change -------------------------------------------------------
    GrowOne { en: "Alice sends Bob three. Her coins are worth ten each.", ko: "앨리스가 밥에게 3을 보냅니다. 앨리스의 코인은 하나에 10짜리입니다." },
    GrowTwo { en: "You cannot send part of a coin, the same way you cannot hand over part of a banknote.", ko: "코인의 일부만 보낼 수는 없습니다. 지폐를 반으로 찢어 줄 수 없는 것과 같습니다." },
    GrowUtxo { en: "Bitcoin's way grew. The ten was destroyed and two new coins were made: three for Bob, seven back to Alice.", ko: "비트코인 쪽은 커졌습니다. 10짜리를 없애고 새 코인 둘을 만들어 3은 밥에게, 7은 앨리스에게 주었습니다." },
    GrowChange { en: "The seven that came back to Alice is called change.", ko: "앨리스에게 돌아온 그 7을 거스름돈이라고 부릅니다." },
    GrowAccount { en: "Ethereum's way grew by one line as well, but only because Bob had none yet. Send him another three and nothing new is made.", ko: "이더리움 쪽도 줄 하나가 늘었지만, 밥의 줄이 아직 없었기 때문일 뿐입니다. 밥에게 3을 또 보내면 새로 생기는 것은 없습니다." },
    GrowObject { en: "Sui's way grew too, for the same reason as Bitcoin: a thing was split, so there is a new thing.", ko: "Sui 쪽도 비트코인과 같은 이유로 커졌습니다. 물건을 쪼갰으니 새 물건이 하나 생긴 것입니다." },
    GrowName { en: "A coin nobody has spent yet has a name: an unspent transaction output, UTXO for short. The heading on the right says it from now on.", ko: "아직 아무도 쓰지 않은 코인에는 unspent transaction output, 줄여서 UTXO라는 이름이 있습니다. 이제부터 오른쪽 머리글에도 그렇게 적힙니다." },
    GrowLesson { en: "That is the first real difference. Counting coins makes change whenever a coin is broken; a book makes a line once and edits it after that.", ko: "이것이 첫 번째 진짜 차이입니다. 코인을 세는 방식은 코인을 쪼갤 때마다 거스름돈을 만들고, 장부를 쓰는 방식은 줄을 한 번 만든 뒤로는 고치기만 합니다." },
    GrowCost { en: "Every node keeps all of this, forever. Change is not free.", ko: "모든 노드가 이것을 전부, 영원히 들고 있어야 합니다. 거스름돈은 공짜가 아닙니다." },

    // ---- Stage 4: your turn ----------------------------------------------------
    TuneOne { en: "Your turn. Two values are yours to set: how much, and who to.", ko: "이제 직접 해 보세요. 정할 값은 얼마를, 누구에게 보낼지 두 가지입니다." },
    TuneThree { en: "Up and down pick a value, left and right change it. Enter sends.", ko: "위아래로 값을 고르고 좌우로 바꿉니다. Enter를 누르면 보냅니다." },
    TuneTyping { en: "Or type a number straight in. Enter sets what you typed, and Enter again sends it.", ko: "숫자를 그냥 쳐 넣어도 됩니다. 친 숫자는 Enter로 확정하고, 다시 Enter를 누르면 보냅니다." },
    TuneFresh { en: "Each send starts again from the same three coins of ten, so two sends are worth comparing.", ko: "보낼 때마다 같은 10짜리 코인 셋에서 다시 시작합니다. 그래야 두 번을 견줄 수 있습니다." },
    GrowFresh { en: "The ledgers go back to the same three coins of ten first, so this send can be held against the last one.", ko: "먼저 원장이 같은 10짜리 코인 셋으로 되돌아갑니다. 그래야 이번에 보내는 것을 앞의 것과 견줄 수 있습니다." },
    TuneTwo { en: "Start with exactly one coin's worth — 10 — and press Enter.", ko: "먼저 코인 하나와 딱 맞는 금액으로 해 보세요. 10으로 두고 Enter를 누릅니다." },
    TuneAgain { en: "Now something smaller than one coin — 3, or 7 — and press Enter again.", ko: "이번에는 코인 하나보다 작은 금액입니다. 3이나 7로 바꾸고 다시 Enter를 누르세요." },
    TuneSentWhole { en: "No coin had to be broken, so no change was made.", ko: "쪼갤 코인이 없었으니 거스름돈도 생기지 않았습니다." },
    TuneSentPart { en: "A coin had to be broken, so the change became a new coin.", ko: "코인 하나를 쪼개야 해서 거스름돈이 새 코인이 됐습니다." },
    TuneCoinsLabel { en: "Bitcoin's coins:", ko: "비트코인 쪽 코인:" },
    TuneBothWhole { en: "Neither send broke a coin, so there is no difference to see. Set 3 or 7 and press Enter.", ko: "두 번 다 코인을 쪼개지 않아 견줄 차이가 없습니다. 3이나 7로 바꾸고 Enter를 누르세요." },
    TuneBothPart { en: "Both sends broke a coin, so there is no difference to see. Set 10 and press Enter.", ko: "두 번 다 코인을 쪼개서 견줄 차이가 없습니다. 10으로 바꾸고 Enter를 누르세요." },
    TuneOnlyOne { en: "That is one send. Change the amount and press Enter again, so there are two to hold against each other.", ko: "아직 한 번 보냈을 뿐입니다. 금액을 바꾸고 Enter를 다시 누르면 견줄 것이 둘이 됩니다." },
    TuneBookLine { en: "The book gained a line both times, for the person paid, whatever the amount. Only the coins cared whether one had to be broken.", ko: "장부는 두 번 다 받는 사람 몫으로 줄 하나가 늘었고, 금액과는 상관이 없었습니다. 코인을 쪼개야 했는지 따진 것은 코인 쪽뿐입니다." },

    // ---- Stage 5: spend it twice ------------------------------------------------
    TwiceOne { en: "Now the thing every one of these systems exists to stop: spending the same money twice.", ko: "이제 이 방식들이 존재하는 이유를 봅니다. 같은 돈을 두 번 쓰는 것을 막는 일입니다." },
    TwiceTwo { en: "Two transfers are written before either is sent, so when they are written both look fine.", ko: "이체 두 건을 보내기 전에 미리 씁니다. 그래서 쓰는 시점에는 둘 다 멀쩡해 보입니다." },
    TwiceMarkCounter { en: "Ethereum stamps every transfer with a counter, and the book notes which number it expects next.", ko: "이더리움은 이체마다 번호를 붙이고, 장부에는 다음에 받을 번호가 적혀 있습니다." },
    TwiceMarkVersion { en: "Sui writes a version on every thing: a number that goes up by one whenever the thing changes.", ko: "Sui는 물건마다 버전을 적어 둡니다. 물건이 바뀔 때마다 하나씩 올라가는 숫자입니다." },
    TwiceThree { en: "Then both are sent. The first one goes in everywhere.", ko: "그다음 둘 다 보냅니다. 첫 번째는 어디서나 들어갑니다." },
    TwiceUtxo { en: "Bitcoin's way stopped the second one: the coin it names is gone. It was destroyed by the first transfer.", ko: "비트코인 쪽이 두 번째를 막았습니다. 그 이체가 가리키는 코인은 첫 번째 이체가 이미 없애 버렸습니다." },
    TwiceAccount { en: "Ethereum's way stopped it too, for a different reason: this transfer's counter has already been used.", ko: "이더리움 쪽도 막았지만 이유가 다릅니다. 이 이체에 붙은 번호는 이미 쓰인 번호입니다." },
    TwiceObject { en: "Sui's way stopped it for a third reason: the transfer names the coin at a version it has already left behind.", ko: "Sui 쪽은 세 번째 이유로 막았습니다. 이 이체는 코인을 이미 지나간 버전으로 가리키고 있습니다." },
    TwiceCounter { en: "That counter is called a nonce. It starts at zero and goes up by one with every transfer the account makes.", ko: "그 번호를 논스라고 부릅니다. 계정마다 0에서 시작해 이체할 때마다 하나씩 올라갑니다." },
    TwiceCounterAgain { en: "So a number that has already been used can never come round again.", ko: "그래서 한 번 쓰인 번호가 다시 돌아오는 일은 없습니다." },
    TwiceLesson { en: "Three systems, three different things noticed. What a system checks is what a system is.", ko: "세 방식이 서로 다른 것을 알아챘습니다. 무엇을 검사하는가가 곧 그 방식의 성격입니다." },

    // ---- Stage 6: recap ---------------------------------------------------------
    RecapOne { en: "Here are the three ways of writing down money, side by side.", ko: "돈을 적는 세 가지 방법을 나란히 놓아 보면 이렇습니다." },
    RecapTwo { en: "Bitcoin counts coins and makes change. Ethereum edits numbers in a book.", ko: "비트코인은 코인을 세고 거스름돈을 만듭니다. 이더리움은 장부의 숫자를 고칩니다." },
    RecapTwoSui { en: "Sui changes the owner written on a thing, and moves its version on.", ko: "Sui는 물건에 적힌 주인을 바꾸고 버전을 하나 올립니다." },
    RecapSentCount { en: "Transfers you sent:", ko: "보낸 이체:" },
    RecapBrokeCount { en: "Ones that broke a coin and made change:", ko: "그중 코인을 쪼개 거스름돈이 생긴 것:" },
    RecapNothingSent { en: "You have not sent anything yet. Tab walks back to any stage.", ko: "아직 보낸 것이 없습니다. Tab으로 어느 단계로든 돌아갈 수 있습니다." },
    RecapStorage { en: "The most storage one of your transfers added:", ko: "이체 한 번으로 가장 많이 늘어난 저장 공간:" },
    RecapAttackThree { en: "Your double spend was stopped by all three, each for a different reason.", ko: "두 번 쓰기는 셋 모두에게 막혔고, 막힌 이유는 저마다 달랐습니다." },
    RecapAttackStopped { en: "Your double spend was stopped by all three.", ko: "두 번 쓰기는 셋 모두에게 막혔습니다." },
    RecapAttackThrough { en: "Your double spend got through at least one of them.", ko: "두 번 쓰기가 적어도 하나는 통과했습니다." },
    RecapNoAttack { en: "You have not tried spending twice yet. That is the stage before this one.", ko: "아직 두 번 쓰기를 해 보지 않았습니다. 바로 앞 단계에서 할 수 있습니다." },
    RecapFour { en: "Next time you read that a chain is \"UTXO-based\" or \"account-based\", you know what was actually being said.", ko: "다음에 어떤 체인이 「UTXO 기반」이다, 「계정 기반」이다 하는 말을 보면, 그것이 실제로 무슨 뜻인지 알게 됐습니다." },

    // ---- The panel on the right -------------------------------------------------
    PanelTitle { en: "The three ledgers", ko: "세 원장" },
    ColumnEntries { en: "entries", ko: "항목" },
    ColumnChange { en: "this send", ko: "이번 이체" },
    LabelHolds { en: "Alice holds", ko: "앨리스 보유" },
    LabelWholeState { en: "What each ledger stores in all — how many entries, and how many bytes.", ko: "각 원장이 통째로 저장하는 것 — 항목 수와 바이트 수." },
    LabelAccepted { en: "accepted", ko: "받아들임" },
    LabelRejected { en: "rejected", ko: "거절함" },
    LabelStopped { en: "stopped it", ko: "막았음" },
    LabelLetThrough { en: "let it through", ko: "통과시킴" },
    WaitingToRun { en: "nothing sent yet", ko: "아직 보낸 것 없음" },

    // ---- Knobs ------------------------------------------------------------------
    KnobAmount { en: "Amount", ko: "금액" },
    KnobRecipient { en: "Recipient", ko: "받는 사람" },
    PartyBob { en: "Bob", ko: "밥" },
    PartyCarol { en: "Carol", ko: "캐럴" },
    KeySend { en: "send it with these", ko: "이 값으로 보내기" },
    EventOutsideRange { en: "outside what this value allows", ko: "이 값이 가질 수 있는 범위 밖입니다" },
    EventSetTo { en: "set to", ko: "맞춘 값" },
    EventNotANumber { en: "that was not a number this value can take", ko: "이 값이 받을 수 있는 숫자가 아닙니다" },
    KeyChange { en: "change the value", ko: "값 바꾸기" },

    // ---- Model names -------------------------------------------------------------
    ModelUtxo { en: "Bitcoin (UTXO)", ko: "비트코인 (UTXO)" },
    ModelAccount { en: "Ethereum (account)", ko: "이더리움 (계정)" },
    ModelObject { en: "Sui (object)", ko: "Sui (객체)" },
    ShortUtxo { en: "Bitcoin", ko: "비트코인" },
    ShortAccount { en: "Ethereum", ko: "이더리움" },
    ShortObject { en: "Sui", ko: "Sui" },
    EntryUnspentOutput { en: "coins", ko: "코인" },
    EntryAccount { en: "lines", ko: "장부의 줄" },
    EntryObject { en: "things", ko: "물건" },

    // ---- Validation steps ---------------------------------------------------------
    StepBuild { en: "writing it", ko: "작성할 때" },
    StepShape { en: "reading it", ko: "형식을 볼 때" },
    StepStateLookup { en: "looking it up", ko: "찾아볼 때" },
    StepAuthorization { en: "checking the owner", ko: "주인을 볼 때" },
    StepFreshness { en: "checking it is current", ko: "최신인지 볼 때" },
    StepValue { en: "adding it up", ko: "금액을 더할 때" },

    // ---- Reasons -------------------------------------------------------------------
    WhyZeroAmount { en: "the amount is zero", ko: "금액이 0입니다" },
    WhyNoInputs { en: "there is nothing to spend", ko: "쓸 것이 없습니다" },
    WhyTooManyOutputs { en: "too many outputs", ko: "출력이 너무 많습니다" },
    WhyDuplicateInput { en: "the same coin is named twice", ko: "같은 코인을 두 번 적었습니다" },
    WhyInputNotFound { en: "that coin is already gone", ko: "그 코인은 이미 없어졌습니다" },
    WhyBadSignature { en: "the signature does not match", ko: "서명이 맞지 않습니다" },
    WhyNotOwner { en: "the sender does not own it", ko: "보낸 사람의 것이 아닙니다" },
    WhyValueNotConserved { en: "the amounts do not add up", ko: "금액이 맞아떨어지지 않습니다" },
    WhyAmountOverflow { en: "the amount is too large", ko: "금액이 너무 큽니다" },
    WhyAccountNotFound { en: "there is no such line in the book", ko: "장부에 그런 줄이 없습니다" },
    WhyNonceMismatch { en: "that counter has already been used", ko: "그 번호는 이미 쓰였습니다" },
    WhyInsufficientBalance { en: "the balance is too small", ko: "잔액이 모자랍니다" },
    WhyObjectNotFound { en: "there is no such thing", ko: "그런 물건이 없습니다" },
    WhyNotSharedObject { en: "that thing is not shared", ko: "공용 물건이 아닙니다" },
    WhyStaleObjectVersion { en: "the thing is at a newer version now", ko: "그 물건은 이미 새 버전입니다" },
    WhyNoCoinAvailable { en: "no coin is free to spend", ko: "쓸 수 있는 코인이 없습니다" },
    WhyCoinTooSmall { en: "that coin is too small", ko: "그 코인은 너무 작습니다" },
    WhyNoSuchCoin { en: "there is no coin there", ko: "거기에 코인이 없습니다" },
    WhyModelMismatch { en: "that belongs to another ledger", ko: "다른 원장의 것입니다" },
}

/// The name a ledger goes by on screen. Named after the chain a reader has heard of, because
/// "UTXO" means nothing until it has been seen working.
pub fn model(model: Model) -> Msg {
    match model {
        Model::Utxo => Msg::ModelUtxo,
        Model::Account => Msg::ModelAccount,
        Model::Object => Msg::ModelObject,
    }
}

/// The chain's name alone, for sentences and for a heading shown before its acronym is earned.
pub fn short_model(model: Model) -> Msg {
    match model {
        Model::Utxo => Msg::ShortUtxo,
        Model::Account => Msg::ShortAccount,
        Model::Object => Msg::ShortObject,
    }
}

/// What a ledger's state is made of.
pub fn entry_kind(kind: EntryKind) -> Msg {
    match kind {
        EntryKind::UnspentOutput => Msg::EntryUnspentOutput,
        EntryKind::Account => Msg::EntryAccount,
        EntryKind::Object => Msg::EntryObject,
    }
}

/// The step a transfer died at.
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
