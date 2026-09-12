//! Every word this quest says, English first.
//!
//! The engine hands back numbers and enums — `AttackPhase::Racing`, `ConfigError::NoMiners`, a
//! hash rate, a count of reverted blocks — and this table is where they become something a reader
//! understands. Keeping the two apart is what lets a second language arrive without the mining
//! code noticing. Korean is written later; anything missing falls back to the English here.

use nmtk_pow::{AttackOutcome, AttackPhase, ConfigError, TargetError};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title { en: "Proof of work", ko: "작업증명" },
    Summary { en: "Mine real blocks on this machine, then rewrite the chain with a 51% attack.", ko: "이 컴퓨터에서 진짜 블록을 캐고, 51% 공격으로 체인을 다시 씁니다." },
    Subcategory { en: "Proof of work", ko: "작업증명" },
    // ---- Brief -----------------------------------------------------------------
    BriefProblem { en: "Thousands of computers that have never met, and have no reason to trust each other, have to agree on one list of who paid whom. Any of them may lie.", ko: "서로 만난 적도 없고 믿을 이유도 없는 컴퓨터 수천 대가, 누가 누구에게 얼마를 냈는지 한 벌의 목록으로 합의해야 합니다. 그중 누구든 거짓말을 할 수 있습니다." },
    BriefCost { en: "Proof of work settles it with electricity. To add a block — one page of that list — you have to find a number that makes the page's fingerprint come out below a target. There is no clever way to find it. You guess, you check, you guess again.", ko: "작업증명은 그것을 전기로 해결합니다. 블록 하나 — 그 목록의 한 쪽 — 을 붙이려면, 그 쪽의 지문이 목표값보다 작게 나오게 하는 수를 찾아야 합니다. 영리하게 찾는 방법은 없습니다. 찍고, 확인하고, 다시 찍습니다." },
    BriefNonce { en: "The fingerprint is a hash: SHA-256, the same function twice, run over the 80 bytes of the block header. The number you keep changing is called the nonce, and it is just a counter sitting in that header.", ko: "지문은 해시입니다. 블록 헤더 80바이트에 SHA-256을 두 번 돌린 값입니다. 계속 바꿔 가며 찍는 그 수를 논스라고 부르고, 헤더 안에 들어 있는 계수기일 뿐입니다." },
    BriefRule { en: "Every computer then follows one rule: the chain with the most work behind it is the real one. Rewriting history means out-guessing everybody else for as long as it takes, and guessing costs power, and power costs money.", ko: "그다음 모든 컴퓨터가 규칙 하나를 따릅니다. 뒤에 쌓인 작업이 가장 많은 체인이 진짜다. 역사를 다시 쓴다는 것은 나머지 전부를 상대로 필요한 시간만큼 계속 이겨야 한다는 뜻이고, 찍는 데는 전력이 들고, 전력에는 돈이 듭니다." },
    BriefPromise { en: "Next your machine does exactly this, for real, at a practice difficulty — the target is set high enough that blocks arrive in seconds instead of years.", ko: "이제 이 컴퓨터가 바로 그 일을 실제로 합니다. 다만 연습용 난이도라, 목표값을 충분히 높게 잡아 블록이 몇 년이 아니라 몇 초 만에 나옵니다." },
    PuzzleTitle { en: "The puzzle", ko: "이 퍼즐" },
    PuzzleHeader { en: "header of 80 bytes, with a nonce in it", ko: "논스가 들어 있는 80바이트 헤더" },
    PuzzleHash { en: "hash it twice with SHA-256", ko: "SHA-256으로 두 번 해시" },
    PuzzleCompare { en: "is the result below the target?", ko: "결과가 목표값보다 작은가?" },
    PuzzleNo { en: "no   change the nonce and hash again", ko: "아니오   논스를 바꿔 다시 해시" },
    PuzzleYes { en: "yes  it is a block", ko: "예     블록이 된 것입니다" },
    PuzzleCost { en: "What one block costs, on average:", ko: "블록 하나에 드는 평균 비용:" },
    LabelPracticeHere { en: "Practice here", ko: "여기 연습 난이도" },
    LabelDifficultyOne { en: "Bitcoin, Jan 2009", ko: "비트코인, 2009년 1월" },
    LabelHarderBy { en: "Harder by", ko: "몇 배 어려운가" },
    UnitHashes { en: "hashes", ko: "해시" },
    // ---- Run -------------------------------------------------------------------
    RunTitle { en: "Mining", ko: "채굴" },
    RunExplainReal { en: "Your cores are hashing real block headers and comparing them against a real target, the same way a Bitcoin node does. The hash rate below is hashes counted and divided by the seconds they took, not an estimate.", ko: "지금 이 컴퓨터의 코어들이 진짜 블록 헤더를 해시해서 진짜 목표값과 비교하고 있습니다. 비트코인 노드가 하는 것과 같은 일입니다. 아래 해시 속도는 실제로 센 해시 수를 걸린 초로 나눈 값이지 추정치가 아닙니다." },
    RunExplainDifficulty { en: "A difficulty is how many hashes a block costs on average. Bitcoin's difficulty 1, where it started in January 2009, costs about 4.3 billion. The practice difficulty here costs far fewer, which is why blocks arrive while you watch.", ko: "난이도란 블록 하나에 평균 몇 번 해시해야 하는가입니다. 비트코인이 2009년 1월에 시작한 난이도 1은 약 43억 번입니다. 여기 연습 난이도는 그보다 훨씬 적어서, 보고 있는 동안 블록이 나옵니다." },
    RunExplainTail { en: "The waiting time is an average, not a countdown. Every hash is an independent try with the same tiny chance, so a machine that has been grinding for an hour is exactly as far from its next block as one that just started.", ko: "기다리는 시간은 평균이지 남은 시간이 아닙니다. 해시 한 번 한 번이 같은 작은 확률을 가진 독립된 시도라, 한 시간을 돌린 기계나 방금 시작한 기계나 다음 블록까지의 거리는 똑같습니다." },
    RunExplainDerived { en: "The 2009 figure beside your rate is worked out from the protocol's own numbers — difficulty 1, and a ten-minute gap between blocks — and is not a measurement of anyone's computer.", ko: "속도 옆의 2009년 수치는 프로토콜 자체의 숫자 — 난이도 1과 블록 간격 10분 — 에서 계산한 값이고, 누군가의 컴퓨터를 실제로 잰 것이 아닙니다." },
    RunIdle { en: "Press Enter to start mining.", ko: "Enter를 눌러 채굴을 시작합니다." },
    StatusIdle { en: "not started", ko: "시작 전" },
    StatusMining { en: "mining", ko: "채굴 중" },
    StatusPaused { en: "paused", ko: "일시정지" },
    StatusDone { en: "finished", ko: "끝남" },
    LabelHashRate { en: "Hash rate", ko: "해시 속도" },
    LabelHashes { en: "Hashes", ko: "해시 수" },
    LabelBlocksFound { en: "Blocks found", ko: "찾은 블록" },
    LabelChainHeight { en: "Chain height", ko: "체인 높이" },
    LabelHashesPerBlock { en: "Hashes per block", ko: "블록당 해시" },
    LabelStale { en: "Lost a race", ko: "경쟁에서 진 블록" },
    HeadingComparison { en: "At Bitcoin's difficulty 1", ko: "비트코인 난이도 1이라면" },
    LabelOneBlockTakes { en: "One block takes", ko: "블록 하나에" },
    LabelHalfTakeUnder { en: "Half take under", ko: "절반은 이 안에" },
    LabelNetwork2009 { en: "Network, early 2009", ko: "2009년 초 네트워크" },
    LabelYouAre { en: "You are", ko: "내 비중" },
    ComparisonDerived { en: "worked out from the protocol, not measured", ko: "프로토콜에서 계산한 값이고 실측이 아닙니다" },
    HeadingRecentBlocks { en: "Newest blocks", ko: "최근 블록" },
    Unavailable { en: "—" },
    // ---- Tune ------------------------------------------------------------------
    TuneTitle { en: "Your numbers", ko: "내 숫자로" },
    TuneExplainHint { en: "Difficulty, miners, their shares and the thread count are yours. Left and right change a value; type digits and press Enter for your own number. Press Enter again to run with them.", ko: "난이도, 채굴자 수, 각자의 몫, 스레드 수는 전부 직접 정합니다. 왼쪽·오른쪽으로 값을 바꾸고, 직접 숫자를 입력한 뒤 Enter를 눌러도 됩니다. 그 값으로 돌리려면 Enter를 한 번 더 누릅니다." },
    TuneExplainDifficulty { en: "Each extra bit of practice difficulty doubles what a block costs. Add a miner and the same machine is divided between more of them, so each one finds fewer blocks.", ko: "연습 난이도가 1비트 올라갈 때마다 블록 하나에 드는 비용이 두 배가 됩니다. 채굴자를 늘리면 같은 컴퓨터를 더 여럿이 나눠 쓰므로 각자가 찾는 블록은 줄어듭니다." },
    TuneExplainThreads { en: "Threads are whole things. A machine with seven of them cannot give one miner 51% and another 49%, so the share a miner really holds is often not the share you asked for. Both numbers are shown.", ko: "스레드는 쪼갤 수 없는 단위입니다. 스레드가 일곱 개인 기계는 한 채굴자에게 51%, 다른 채굴자에게 49%를 줄 수 없습니다. 그래서 실제로 쥔 몫이 요청한 몫과 다른 경우가 많고, 두 숫자를 모두 보여 줍니다." },
    TuneExplainOversubscribe { en: "Asking for more threads than the machine has cores brings the two numbers together — the operating system splits its time between them — at a small cost in total speed.", ko: "코어보다 많은 스레드를 요청하면 두 숫자가 가까워집니다. 운영체제가 시간을 나눠 주기 때문인데, 대신 전체 속도가 조금 줄어듭니다." },
    TuneShares { en: "Shares do not have to add up to 100. Three miners asking for 1, 1 and 2 get a quarter, a quarter and a half.", ko: "몫의 합이 100이 될 필요는 없습니다. 채굴자 셋이 1, 1, 2를 요청하면 각각 4분의 1, 4분의 1, 2분의 1을 갖습니다." },
    KnobDifficulty { en: "Practice difficulty", ko: "연습 난이도" },
    KnobMiners { en: "Miners", ko: "채굴자 수" },
    KnobThreads { en: "Threads", ko: "스레드" },
    KnobShareOne { en: "Miner 1 share", ko: "채굴자 1의 몫" },
    KnobShareTwo { en: "Miner 2 share", ko: "채굴자 2의 몫" },
    KnobShareThree { en: "Miner 3 share", ko: "채굴자 3의 몫" },
    KnobShareFour { en: "Miner 4 share", ko: "채굴자 4의 몫" },
    UnitZeroBits { en: "zero bits", ko: "0비트" },
    MinerOne { en: "Miner 1", ko: "채굴자 1" },
    MinerTwo { en: "Miner 2", ko: "채굴자 2" },
    MinerThree { en: "Miner 3", ko: "채굴자 3" },
    MinerFour { en: "Miner 4", ko: "채굴자 4" },
    HeadingSplit { en: "Threads and shares", ko: "스레드와 몫" },
    ColumnMiner { en: "Miner", ko: "채굴자" },
    ColumnThreads { en: "threads", ko: "스레드" },
    ColumnAsked { en: "asked", ko: "요청" },
    ColumnGot { en: "got", ko: "실제" },
    TuneMismatch { en: "asked and got differ: threads are whole things", ko: "요청과 실제가 다릅니다. 스레드는 쪼갤 수 없습니다" },
    TuneRestart { en: "Press Enter to run with these numbers.", ko: "Enter를 누르면 이 값으로 돌립니다." },
    // ---- Break -----------------------------------------------------------------
    BreakTitle { en: "The 51% attack", ko: "51% 공격" },
    BreakExplainStory { en: "Someone pays a merchant and waits. The payment goes into a block, more blocks land on top of it, and at the number of confirmations the merchant trusts, the goods are handed over.", ko: "누군가 상인에게 돈을 내고 기다립니다. 그 결제가 블록에 들어가고, 그 위에 블록이 더 쌓이고, 상인이 믿는 확인 수에 이르면 물건이 건네집니다." },
    BreakExplainPrivate { en: "Meanwhile the buyer has been mining a chain of their own in private, starting from the block before the payment. In that chain the same coin was paid to the buyer instead.", ko: "그동안 산 사람은 결제 직전 블록에서 갈라져 나온 자기만의 체인을 몰래 캐고 있었습니다. 그 체인에서는 같은 코인이 상인이 아니라 자기에게 지불됩니다." },
    BreakExplainPublish { en: "When the private chain carries more work than the public one, the buyer publishes it. Every node switches, because that is the rule every node follows, and the payment the merchant watched confirm is simply not there any more.", ko: "비밀 체인이 공개 체인보다 더 많은 작업을 담게 되면 산 사람이 그것을 공개합니다. 모든 노드가 그쪽으로 갈아탑니다. 그것이 모든 노드가 따르는 규칙이기 때문이고, 상인이 확정되는 것을 지켜봤던 그 결제는 이제 그냥 없습니다." },
    BreakExplainArithmetic { en: "Whether this works is arithmetic, not cleverness. Above half the hash power the attacker catches up eventually. Below it the attacker falls behind, and has to keep paying for hash power while falling. Try 30% and watch it fail.", ko: "이게 되느냐 안 되느냐는 영리함이 아니라 산수입니다. 해시 파워의 절반을 넘으면 공격자는 언젠가 따라잡습니다. 절반에 못 미치면 뒤처지고, 뒤처지는 내내 해시 파워 값을 계속 치러야 합니다. 30%로 놓고 실패하는 것을 보세요." },
    BreakExplainSettings { en: "The practice difficulty and the thread count come from Tune. If a run keeps giving up on time rather than on work, lower the difficulty there.", ko: "연습 난이도와 스레드 수는 조절 화면에서 가져옵니다. 작업이 아니라 시간 때문에 자꾸 포기한다면 거기서 난이도를 낮추세요." },
    BreakHint { en: "Press Enter to pay and start the attack.", ko: "Enter를 누르면 결제하고 공격을 시작합니다." },
    KnobAttackerShare { en: "Attacker's share", ko: "공격자의 몫" },
    KnobConfirmations { en: "Confirmations", ko: "확인 수" },
    PhaseWarmup { en: "mining in the open", ko: "공개적으로 채굴 중" },
    PhaseAwaitingPayment { en: "the payment is waiting for a block", ko: "결제가 블록을 기다리는 중" },
    PhaseConfirming { en: "counting confirmations", ko: "확인 수를 세는 중" },
    PhaseRacing { en: "the private chain is racing", ko: "비밀 체인이 쫓는 중" },
    PhaseFinished { en: "over", ko: "끝" },
    LabelPublicChain { en: "Public chain", ko: "공개 체인" },
    LabelPrivateChain { en: "Private chain", ko: "비밀 체인" },
    LabelAheadBy { en: "Attacker ahead by", ko: "공격자가 앞선 폭" },
    LabelBehindBy { en: "Attacker behind by", ko: "공격자가 뒤진 폭" },
    LabelFurthestBehind { en: "Furthest behind", ko: "가장 뒤졌던 폭" },
    LabelMerchantSaw { en: "The merchant saw", ko: "상인이 본 확인 수" },
    LabelGoods { en: "Goods", ko: "물건" },
    LabelBlocksErased { en: "Blocks erased", ko: "지워진 블록" },
    LabelAttackTook { en: "The attack took", ko: "공격에 걸린 시간" },
    LabelThreadSplit { en: "Threads", ko: "스레드" },
    LabelDifficulty { en: "Difficulty", ko: "난이도" },
    UnitConfirmation { en: "confirmation", ko: "확인" },
    UnitConfirmations { en: "confirmations", ko: "확인" },
    UnitBlock { en: "block", ko: "블록" },
    UnitBlocks { en: "blocks", ko: "블록" },
    VictimReleased { en: "handed over", ko: "건넸음" },
    VictimWaiting { en: "not yet", ko: "아직" },
    ThreadsAttackerHonest { en: "attacker / everyone else", ko: "공격자 / 나머지 전부" },
    OutcomeSucceeded { en: "the payment is gone from the chain", ko: "그 결제는 체인에서 사라졌습니다" },
    OutcomeGaveUp { en: "the attacker gave up and the chain held", ko: "공격자가 포기했고 체인은 버텼습니다" },
    OutcomeShortSucceeded { en: "payment erased", ko: "결제가 지워짐" },
    OutcomeShortGaveUp { en: "the chain held", ko: "체인이 버팀" },
    BreakLessonFailed { en: "Below half the power, this usually happens.", ko: "절반에 못 미치는 힘으로는 보통 이렇게 됩니다." },
    // ---- Recap -----------------------------------------------------------------
    RecapTitle { en: "What just happened", ko: "방금 본 것" },
    RecapExplainOrder { en: "In the order it happened.", ko: "일어난 순서대로." },
    RecapExplainMining { en: "You hashed headers until one came out below a target. That is the whole of mining, and every number on the Run screen was counted here rather than estimated.", ko: "목표값보다 작은 값이 나올 때까지 헤더를 해시했습니다. 채굴이란 그게 전부이고, 실행 화면의 모든 숫자는 추정이 아니라 여기서 실제로 센 것입니다." },
    RecapExplainTune { en: "You changed the difficulty, the miners, their shares and the threads, and the run answered with different blocks.", ko: "난이도, 채굴자, 각자의 몫, 스레드를 바꿨고, 그때마다 나오는 블록이 달라졌습니다." },
    RecapExplainBreak { en: "You bought a share of the hash power and tried to erase a payment somebody had already been paid for. Above half it works and below half it does not, and both endings are the lesson.", ko: "해시 파워의 일부를 사서, 누군가 이미 받은 결제를 지우려고 했습니다. 절반을 넘으면 되고 못 넘으면 안 되며, 두 결말 모두가 이 배움의 내용입니다." },
    RecapDifficulty { en: "Practice difficulty", ko: "연습 난이도" },
    RecapHashRate { en: "Your hash rate", ko: "내 해시 속도" },
    RecapBlocks { en: "Blocks you mined", ko: "내가 캔 블록" },
    RecapAtDifficultyOne { en: "At difficulty 1", ko: "난이도 1이라면" },
    RecapNetwork { en: "Network, early 2009", ko: "2009년 초 네트워크" },
    RecapSplit { en: "Miner 1 asked / got", ko: "채굴자 1 요청 / 실제" },
    RecapAttack { en: "Attack", ko: "공격" },
    NotYet { en: "not run yet", ko: "아직 안 돌림" },
    // ---- Keys ------------------------------------------------------------------
    KeyTypeNumber { en: "type a number", ko: "숫자 입력" },
    KeyTurn { en: "turn the value", ko: "값 바꾸기" },
    // ---- When a setting is refused ---------------------------------------------
    ErrorNoMiners { en: "There has to be at least one miner.", ko: "채굴자가 적어도 하나는 있어야 합니다." },
    ErrorDuplicateMiner { en: "Two miners were given the same name.", ko: "두 채굴자에게 같은 이름이 붙었습니다." },
    ErrorInvalidShare { en: "That share is not a number a miner can hold.", ko: "채굴자가 가질 수 있는 몫이 아닙니다." },
    ErrorTooFewThreads { en: "Fewer threads than miners. Raise the threads.", ko: "스레드가 채굴자보다 적습니다. 스레드를 올리세요." },
    ErrorFixedThreads { en: "Miners asked for more threads than there are.", ko: "채굴자들이 있는 것보다 많은 스레드를 요청했습니다." },
    ErrorNegativeTarget { en: "That difficulty cannot go in a block header.", ko: "그 난이도는 블록 헤더에 들어갈 수 없습니다." },
    ErrorZeroTarget { en: "That difficulty leaves no hash that could win.", ko: "그 난이도로는 이길 수 있는 해시가 없습니다." },
    ErrorTargetOverflow { en: "That difficulty does not fit in a header.", ko: "그 난이도는 헤더에 담기지 않습니다." },
    ErrorTooManyZeroBits { en: "A hash cannot have that many zero bits.", ko: "해시가 그렇게 많은 0비트를 가질 수는 없습니다." },
}

/// The name a miner goes by on screen. The engine only knows it as a number.
pub fn miner_name(index: usize) -> Msg {
    match index {
        0 => Msg::MinerOne,
        1 => Msg::MinerTwo,
        2 => Msg::MinerThree,
        _ => Msg::MinerFour,
    }
}

/// The label on a miner's share knob.
pub fn share_label(index: usize) -> Msg {
    match index {
        0 => Msg::KnobShareOne,
        1 => Msg::KnobShareTwo,
        2 => Msg::KnobShareThree,
        _ => Msg::KnobShareFour,
    }
}

/// Where the attack has got to.
pub fn phase(phase: AttackPhase) -> Msg {
    match phase {
        AttackPhase::Warmup => Msg::PhaseWarmup,
        AttackPhase::AwaitingPayment => Msg::PhaseAwaitingPayment,
        AttackPhase::Confirming => Msg::PhaseConfirming,
        AttackPhase::Racing => Msg::PhaseRacing,
        AttackPhase::Finished => Msg::PhaseFinished,
    }
}

/// How the attack ended.
pub fn outcome(outcome: AttackOutcome) -> Msg {
    match outcome {
        AttackOutcome::Succeeded => Msg::OutcomeSucceeded,
        AttackOutcome::GaveUp => Msg::OutcomeGaveUp,
    }
}

/// How the attack ended, in the few words a recap column has room for.
pub fn short_outcome(outcome: AttackOutcome) -> Msg {
    match outcome {
        AttackOutcome::Succeeded => Msg::OutcomeShortSucceeded,
        AttackOutcome::GaveUp => Msg::OutcomeShortGaveUp,
    }
}

/// Why a run could not start with the numbers it was given.
pub fn config_error(error: ConfigError) -> Msg {
    match error {
        ConfigError::NoMiners => Msg::ErrorNoMiners,
        ConfigError::DuplicateMinerId(_) => Msg::ErrorDuplicateMiner,
        ConfigError::InvalidShare(_) => Msg::ErrorInvalidShare,
        ConfigError::ThreadBudgetTooSmall { .. } => Msg::ErrorTooFewThreads,
        ConfigError::FixedThreadsExceedBudget { .. } => Msg::ErrorFixedThreads,
        ConfigError::BadTarget(inner) => target_error(inner),
    }
}

/// Why a difficulty could not be turned into a target.
pub fn target_error(error: TargetError) -> Msg {
    match error {
        TargetError::NegativeTarget => Msg::ErrorNegativeTarget,
        TargetError::ZeroTarget => Msg::ErrorZeroTarget,
        TargetError::Overflow => Msg::ErrorTargetOverflow,
        TargetError::TooManyZeroBits => Msg::ErrorTooManyZeroBits,
    }
}

#[cfg(test)]
mod tests {
    use nmtk_core::Language;
    use nmtk_pow::MinerId;

    use super::*;

    #[test]
    fn english_is_never_missing() {
        for message in Msg::ALL {
            assert!(!message.text(Language::ENGLISH).is_empty(), "{message:?} has no English");
        }
    }

    #[test]
    fn korean_falls_back_to_english_rather_than_a_blank() {
        for message in Msg::ALL {
            assert!(!message.text(Language::KOREAN).is_empty(), "{message:?} is blank in Korean");
        }
    }

    #[test]
    fn every_phase_and_outcome_the_engine_can_report_has_words() {
        let phases = [
            AttackPhase::Warmup,
            AttackPhase::AwaitingPayment,
            AttackPhase::Confirming,
            AttackPhase::Racing,
            AttackPhase::Finished,
        ];
        for each in phases {
            assert!(!phase(each).text(Language::ENGLISH).is_empty(), "{each:?} has no words");
        }
        for each in [AttackOutcome::Succeeded, AttackOutcome::GaveUp] {
            assert!(!outcome(each).text(Language::ENGLISH).is_empty());
            assert!(!short_outcome(each).text(Language::ENGLISH).is_empty());
        }
    }

    #[test]
    fn every_refusal_the_engine_can_report_has_words() {
        let errors = [
            ConfigError::NoMiners,
            ConfigError::DuplicateMinerId(MinerId(0)),
            ConfigError::InvalidShare(MinerId(0)),
            ConfigError::ThreadBudgetTooSmall { needed: 4, budget: 2 },
            ConfigError::FixedThreadsExceedBudget { fixed: 9, budget: 4 },
            ConfigError::BadTarget(TargetError::ZeroTarget),
        ];
        for error in errors {
            assert!(!config_error(error).text(Language::ENGLISH).is_empty(), "{error:?}");
        }
        let targets = [
            TargetError::NegativeTarget,
            TargetError::ZeroTarget,
            TargetError::Overflow,
            TargetError::TooManyZeroBits,
        ];
        for error in targets {
            assert!(!target_error(error).text(Language::ENGLISH).is_empty(), "{error:?}");
        }
    }

    #[test]
    fn every_miner_has_a_name_and_a_share_label() {
        for index in 0..4 {
            assert!(!miner_name(index).text(Language::ENGLISH).is_empty());
            assert!(!share_label(index).text(Language::ENGLISH).is_empty());
        }
    }
}
