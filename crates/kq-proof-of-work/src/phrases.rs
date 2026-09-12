//! Every word this quest says, English first.
//!
//! The engine hands back numbers and enums — `AttackPhase::Racing`, `ConfigError::NoMiners`, a
//! hash rate, a count of reverted blocks — and this table is where they become something a reader
//! understands. Keeping the two apart is what lets a second language arrive without the mining
//! code noticing. Korean is written later; anything missing falls back to the English here.

use nmtk_pow::{AttackOutcome, AttackPhase, ConfigError, TargetError};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title: "Proof of work" => "작업증명",
    Summary: "Mine real blocks on this machine, then rewrite the chain with a 51% attack." => "이 컴퓨터에서 진짜 블록을 캐고, 51% 공격으로 체인을 다시 씁니다.",
    Subcategory: "Proof of work" => "작업증명",
    // ---- Brief -----------------------------------------------------------------
    BriefProblem: "Thousands of computers that have never met, and have no reason to trust each other, have to agree on one list of who paid whom. Any of them may lie." => "서로 만난 적도 없고 믿을 이유도 없는 컴퓨터 수천 대가, 누가 누구에게 얼마를 냈는지 한 벌의 목록으로 합의해야 합니다. 그중 누구든 거짓말을 할 수 있습니다.",
    BriefCost: "Proof of work settles it with electricity. To add a block — one page of that list — you have to find a number that makes the page's fingerprint come out below a target. There is no clever way to find it. You guess, you check, you guess again." => "작업증명은 그것을 전기로 해결합니다. 블록 하나 — 그 목록의 한 쪽 — 을 붙이려면, 그 쪽의 지문이 목표값보다 작게 나오게 하는 수를 찾아야 합니다. 영리하게 찾는 방법은 없습니다. 찍고, 확인하고, 다시 찍습니다.",
    BriefNonce: "The fingerprint is a hash: SHA-256, the same function twice, run over the 80 bytes of the block header. The number you keep changing is called the nonce, and it is just a counter sitting in that header." => "지문은 해시입니다. 블록 헤더 80바이트에 SHA-256을 두 번 돌린 값입니다. 계속 바꿔 가며 찍는 그 수를 논스라고 부르고, 헤더 안에 들어 있는 계수기일 뿐입니다.",
    BriefRule: "Every computer then follows one rule: the chain with the most work behind it is the real one. Rewriting history means out-guessing everybody else for as long as it takes, and guessing costs power, and power costs money." => "그다음 모든 컴퓨터가 규칙 하나를 따릅니다. 뒤에 쌓인 작업이 가장 많은 체인이 진짜다. 역사를 다시 쓴다는 것은 나머지 전부를 상대로 필요한 시간만큼 계속 이겨야 한다는 뜻이고, 찍는 데는 전력이 들고, 전력에는 돈이 듭니다.",
    BriefPromise: "Next your machine does exactly this, for real, at a practice difficulty — the target is set high enough that blocks arrive in seconds instead of years." => "이제 이 컴퓨터가 바로 그 일을 실제로 합니다. 다만 연습용 난이도라, 목표값을 충분히 높게 잡아 블록이 몇 년이 아니라 몇 초 만에 나옵니다.",
    PuzzleTitle: "The puzzle" => "이 퍼즐",
    PuzzleHeader: "header of 80 bytes, with a nonce in it" => "논스가 들어 있는 80바이트 헤더",
    PuzzleHash: "hash it twice with SHA-256" => "SHA-256으로 두 번 해시",
    PuzzleCompare: "is the result below the target?" => "결과가 목표값보다 작은가?",
    PuzzleNo: "no   change the nonce and hash again" => "아니오   논스를 바꿔 다시 해시",
    PuzzleYes: "yes  it is a block" => "예     블록이 된 것입니다",
    PuzzleCost: "What one block costs, on average:" => "블록 하나에 드는 평균 비용:",
    LabelPracticeHere: "Practice here" => "여기 연습 난이도",
    LabelDifficultyOne: "Bitcoin, Jan 2009" => "비트코인, 2009년 1월",
    LabelHarderBy: "Harder by" => "몇 배 어려운가",
    UnitHashes: "hashes" => "해시",
    // ---- Run -------------------------------------------------------------------
    RunTitle: "Mining" => "채굴",
    RunExplainReal: "Your cores are hashing real block headers and comparing them against a real target, the same way a Bitcoin node does. The hash rate below is hashes counted and divided by the seconds they took, not an estimate." => "지금 이 컴퓨터의 코어들이 진짜 블록 헤더를 해시해서 진짜 목표값과 비교하고 있습니다. 비트코인 노드가 하는 것과 같은 일입니다. 아래 해시 속도는 실제로 센 해시 수를 걸린 초로 나눈 값이지 추정치가 아닙니다.",
    RunExplainDifficulty: "A difficulty is how many hashes a block costs on average. Bitcoin's difficulty 1, where it started in January 2009, costs about 4.3 billion. The practice difficulty here costs far fewer, which is why blocks arrive while you watch." => "난이도란 블록 하나에 평균 몇 번 해시해야 하는가입니다. 비트코인이 2009년 1월에 시작한 난이도 1은 약 43억 번입니다. 여기 연습 난이도는 그보다 훨씬 적어서, 보고 있는 동안 블록이 나옵니다.",
    RunExplainTail: "The waiting time is an average, not a countdown. Every hash is an independent try with the same tiny chance, so a machine that has been grinding for an hour is exactly as far from its next block as one that just started." => "기다리는 시간은 평균이지 남은 시간이 아닙니다. 해시 한 번 한 번이 같은 작은 확률을 가진 독립된 시도라, 한 시간을 돌린 기계나 방금 시작한 기계나 다음 블록까지의 거리는 똑같습니다.",
    RunExplainDerived: "The 2009 figure beside your rate is worked out from the protocol's own numbers — difficulty 1, and a ten-minute gap between blocks — and is not a measurement of anyone's computer." => "속도 옆의 2009년 수치는 프로토콜 자체의 숫자 — 난이도 1과 블록 간격 10분 — 에서 계산한 값이고, 누군가의 컴퓨터를 실제로 잰 것이 아닙니다.",
    RunIdle: "Press Enter to start mining." => "Enter를 눌러 채굴을 시작합니다.",
    StatusIdle: "not started" => "시작 전",
    StatusMining: "mining" => "채굴 중",
    StatusPaused: "paused" => "일시정지",
    StatusDone: "finished" => "끝남",
    LabelHashRate: "Hash rate" => "해시 속도",
    LabelHashes: "Hashes" => "해시 수",
    LabelBlocksFound: "Blocks found" => "찾은 블록",
    LabelChainHeight: "Chain height" => "체인 높이",
    LabelHashesPerBlock: "Hashes per block" => "블록당 해시",
    LabelStale: "Lost a race" => "경쟁에서 진 블록",
    HeadingComparison: "At Bitcoin's difficulty 1" => "비트코인 난이도 1이라면",
    LabelOneBlockTakes: "One block takes" => "블록 하나에",
    LabelHalfTakeUnder: "Half take under" => "절반은 이 안에",
    LabelNetwork2009: "Network, early 2009" => "2009년 초 네트워크",
    LabelYouAre: "You are" => "내 비중",
    ComparisonDerived: "worked out from the protocol, not measured" => "프로토콜에서 계산한 값이고 실측이 아닙니다",
    HeadingRecentBlocks: "Newest blocks" => "최근 블록",
    Unavailable: "—",

    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Your numbers" => "내 숫자로",
    TuneExplainHint: "Difficulty, miners, their shares and the thread count are yours. Left and right change a value; type digits and press Enter for your own number. Press Enter again to run with them." => "난이도, 채굴자 수, 각자의 몫, 스레드 수는 전부 직접 정합니다. 왼쪽·오른쪽으로 값을 바꾸고, 직접 숫자를 입력한 뒤 Enter를 눌러도 됩니다. 그 값으로 돌리려면 Enter를 한 번 더 누릅니다.",
    TuneExplainDifficulty: "Each extra bit of practice difficulty doubles what a block costs. Add a miner and the same machine is divided between more of them, so each one finds fewer blocks." => "연습 난이도가 1비트 올라갈 때마다 블록 하나에 드는 비용이 두 배가 됩니다. 채굴자를 늘리면 같은 컴퓨터를 더 여럿이 나눠 쓰므로 각자가 찾는 블록은 줄어듭니다.",
    TuneExplainThreads: "Threads are whole things. A machine with seven of them cannot give one miner 51% and another 49%, so the share a miner really holds is often not the share you asked for. Both numbers are shown." => "스레드는 쪼갤 수 없는 단위입니다. 스레드가 일곱 개인 기계는 한 채굴자에게 51%, 다른 채굴자에게 49%를 줄 수 없습니다. 그래서 실제로 쥔 몫이 요청한 몫과 다른 경우가 많고, 두 숫자를 모두 보여 줍니다.",
    TuneExplainOversubscribe: "Asking for more threads than the machine has cores brings the two numbers together — the operating system splits its time between them — at a small cost in total speed." => "코어보다 많은 스레드를 요청하면 두 숫자가 가까워집니다. 운영체제가 시간을 나눠 주기 때문인데, 대신 전체 속도가 조금 줄어듭니다.",
    TuneShares: "Shares do not have to add up to 100. Three miners asking for 1, 1 and 2 get a quarter, a quarter and a half." => "몫의 합이 100이 될 필요는 없습니다. 채굴자 셋이 1, 1, 2를 요청하면 각각 4분의 1, 4분의 1, 2분의 1을 갖습니다.",
    KnobDifficulty: "Practice difficulty" => "연습 난이도",
    KnobMiners: "Miners" => "채굴자 수",
    KnobThreads: "Threads" => "스레드",
    KnobShareOne: "Miner 1 share" => "채굴자 1의 몫",
    KnobShareTwo: "Miner 2 share" => "채굴자 2의 몫",
    KnobShareThree: "Miner 3 share" => "채굴자 3의 몫",
    KnobShareFour: "Miner 4 share" => "채굴자 4의 몫",
    UnitZeroBits: "zero bits" => "0비트",
    MinerOne: "Miner 1" => "채굴자 1",
    MinerTwo: "Miner 2" => "채굴자 2",
    MinerThree: "Miner 3" => "채굴자 3",
    MinerFour: "Miner 4" => "채굴자 4",
    HeadingSplit: "Threads and shares" => "스레드와 몫",
    ColumnMiner: "Miner" => "채굴자",
    ColumnThreads: "threads" => "스레드",
    ColumnAsked: "asked" => "요청",
    ColumnGot: "got" => "실제",
    TuneMismatch: "asked and got differ: threads are whole things" => "요청과 실제가 다릅니다. 스레드는 쪼갤 수 없습니다",
    TuneRestart: "Press Enter to run with these numbers." => "Enter를 누르면 이 값으로 돌립니다.",
    // ---- Break -----------------------------------------------------------------
    BreakTitle: "The 51% attack" => "51% 공격",
    BreakExplainStory: "Someone pays a merchant and waits. The payment goes into a block, more blocks land on top of it, and at the number of confirmations the merchant trusts, the goods are handed over." => "누군가 상인에게 돈을 내고 기다립니다. 그 결제가 블록에 들어가고, 그 위에 블록이 더 쌓이고, 상인이 믿는 확인 수에 이르면 물건이 건네집니다.",
    BreakExplainPrivate: "Meanwhile the buyer has been mining a chain of their own in private, starting from the block before the payment. In that chain the same coin was paid to the buyer instead." => "그동안 산 사람은 결제 직전 블록에서 갈라져 나온 자기만의 체인을 몰래 캐고 있었습니다. 그 체인에서는 같은 코인이 상인이 아니라 자기에게 지불됩니다.",
    BreakExplainPublish: "When the private chain carries more work than the public one, the buyer publishes it. Every node switches, because that is the rule every node follows, and the payment the merchant watched confirm is simply not there any more." => "비밀 체인이 공개 체인보다 더 많은 작업을 담게 되면 산 사람이 그것을 공개합니다. 모든 노드가 그쪽으로 갈아탑니다. 그것이 모든 노드가 따르는 규칙이기 때문이고, 상인이 확정되는 것을 지켜봤던 그 결제는 이제 그냥 없습니다.",
    BreakExplainArithmetic: "Whether this works is arithmetic, not cleverness. Above half the hash power the attacker catches up eventually. Below it the attacker falls behind, and has to keep paying for hash power while falling. Try 30% and watch it fail." => "이게 되느냐 안 되느냐는 영리함이 아니라 산수입니다. 해시 파워의 절반을 넘으면 공격자는 언젠가 따라잡습니다. 절반에 못 미치면 뒤처지고, 뒤처지는 내내 해시 파워 값을 계속 치러야 합니다. 30%로 놓고 실패하는 것을 보세요.",
    BreakExplainSettings: "The practice difficulty and the thread count come from Tune. If a run keeps giving up on time rather than on work, lower the difficulty there." => "연습 난이도와 스레드 수는 조절 화면에서 가져옵니다. 작업이 아니라 시간 때문에 자꾸 포기한다면 거기서 난이도를 낮추세요.",
    BreakHint: "Press Enter to pay and start the attack." => "Enter를 누르면 결제하고 공격을 시작합니다.",
    KnobAttackerShare: "Attacker's share" => "공격자의 몫",
    KnobConfirmations: "Confirmations" => "확인 수",
    PhaseWarmup: "mining in the open" => "공개적으로 채굴 중",
    PhaseAwaitingPayment: "the payment is waiting for a block" => "결제가 블록을 기다리는 중",
    PhaseConfirming: "counting confirmations" => "확인 수를 세는 중",
    PhaseRacing: "the private chain is racing" => "비밀 체인이 쫓는 중",
    PhaseFinished: "over" => "끝",
    LabelPublicChain: "Public chain" => "공개 체인",
    LabelPrivateChain: "Private chain" => "비밀 체인",
    LabelAheadBy: "Attacker ahead by" => "공격자가 앞선 폭",
    LabelBehindBy: "Attacker behind by" => "공격자가 뒤진 폭",
    LabelFurthestBehind: "Furthest behind" => "가장 뒤졌던 폭",
    LabelMerchantSaw: "The merchant saw" => "상인이 본 확인 수",
    LabelGoods: "Goods" => "물건",
    LabelBlocksErased: "Blocks erased" => "지워진 블록",
    LabelAttackTook: "The attack took" => "공격에 걸린 시간",
    LabelThreadSplit: "Threads" => "스레드",
    LabelDifficulty: "Difficulty" => "난이도",
    UnitConfirmation: "confirmation" => "확인",
    UnitConfirmations: "confirmations" => "확인",
    UnitBlock: "block" => "블록",
    UnitBlocks: "blocks" => "블록",
    VictimReleased: "handed over" => "건넸음",
    VictimWaiting: "not yet" => "아직",
    ThreadsAttackerHonest: "attacker / everyone else" => "공격자 / 나머지 전부",
    OutcomeSucceeded: "the payment is gone from the chain" => "그 결제는 체인에서 사라졌습니다",
    OutcomeGaveUp: "the attacker gave up and the chain held" => "공격자가 포기했고 체인은 버텼습니다",
    OutcomeShortSucceeded: "payment erased" => "결제가 지워짐",
    OutcomeShortGaveUp: "the chain held" => "체인이 버팀",
    BreakLessonFailed: "Below half the power, this usually happens." => "절반에 못 미치는 힘으로는 보통 이렇게 됩니다.",
    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "What just happened" => "방금 본 것",
    RecapExplainOrder: "In the order it happened." => "일어난 순서대로.",
    RecapExplainMining: "You hashed headers until one came out below a target. That is the whole of mining, and every number on the Run screen was counted here rather than estimated." => "목표값보다 작은 값이 나올 때까지 헤더를 해시했습니다. 채굴이란 그게 전부이고, 실행 화면의 모든 숫자는 추정이 아니라 여기서 실제로 센 것입니다.",
    RecapExplainTune: "You changed the difficulty, the miners, their shares and the threads, and the run answered with different blocks." => "난이도, 채굴자, 각자의 몫, 스레드를 바꿨고, 그때마다 나오는 블록이 달라졌습니다.",
    RecapExplainBreak: "You bought a share of the hash power and tried to erase a payment somebody had already been paid for. Above half it works and below half it does not, and both endings are the lesson." => "해시 파워의 일부를 사서, 누군가 이미 받은 결제를 지우려고 했습니다. 절반을 넘으면 되고 못 넘으면 안 되며, 두 결말 모두가 이 배움의 내용입니다.",
    RecapDifficulty: "Practice difficulty" => "연습 난이도",
    RecapHashRate: "Your hash rate" => "내 해시 속도",
    RecapBlocks: "Blocks you mined" => "내가 캔 블록",
    RecapAtDifficultyOne: "At difficulty 1" => "난이도 1이라면",
    RecapNetwork: "Network, early 2009" => "2009년 초 네트워크",
    RecapSplit: "Miner 1 asked / got" => "채굴자 1 요청 / 실제",
    RecapAttack: "Attack" => "공격",
    NotYet: "not run yet" => "아직 안 돌림",
    // ---- Keys ------------------------------------------------------------------
    KeyTypeNumber: "type a number" => "숫자 입력",
    KeyTurn: "turn the value" => "값 바꾸기",
    // ---- When a setting is refused ---------------------------------------------
    ErrorNoMiners: "There has to be at least one miner." => "채굴자가 적어도 하나는 있어야 합니다.",
    ErrorDuplicateMiner: "Two miners were given the same name." => "두 채굴자에게 같은 이름이 붙었습니다.",
    ErrorInvalidShare: "That share is not a number a miner can hold." => "채굴자가 가질 수 있는 몫이 아닙니다.",
    ErrorTooFewThreads: "Fewer threads than miners. Raise the threads." => "스레드가 채굴자보다 적습니다. 스레드를 올리세요.",
    ErrorFixedThreads: "Miners asked for more threads than there are." => "채굴자들이 있는 것보다 많은 스레드를 요청했습니다.",
    ErrorNegativeTarget: "That difficulty cannot go in a block header." => "그 난이도는 블록 헤더에 들어갈 수 없습니다.",
    ErrorZeroTarget: "That difficulty leaves no hash that could win." => "그 난이도로는 이길 수 있는 해시가 없습니다.",
    ErrorTargetOverflow: "That difficulty does not fit in a header." => "그 난이도는 헤더에 담기지 않습니다.",
    ErrorTooManyZeroBits: "A hash cannot have that many zero bits." => "해시가 그렇게 많은 0비트를 가질 수는 없습니다.",
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
            assert!(!message.text(Language::English).is_empty(), "{message:?} has no English");
        }
    }

    #[test]
    fn korean_falls_back_to_english_rather_than_a_blank() {
        for message in Msg::ALL {
            assert!(!message.text(Language::Korean).is_empty(), "{message:?} is blank in Korean");
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
            assert!(!phase(each).text(Language::English).is_empty(), "{each:?} has no words");
        }
        for each in [AttackOutcome::Succeeded, AttackOutcome::GaveUp] {
            assert!(!outcome(each).text(Language::English).is_empty());
            assert!(!short_outcome(each).text(Language::English).is_empty());
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
            assert!(!config_error(error).text(Language::English).is_empty(), "{error:?}");
        }
        let targets = [
            TargetError::NegativeTarget,
            TargetError::ZeroTarget,
            TargetError::Overflow,
            TargetError::TooManyZeroBits,
        ];
        for error in targets {
            assert!(!target_error(error).text(Language::English).is_empty(), "{error:?}");
        }
    }

    #[test]
    fn every_miner_has_a_name_and_a_share_label() {
        for index in 0..4 {
            assert!(!miner_name(index).text(Language::English).is_empty());
            assert!(!share_label(index).text(Language::English).is_empty());
        }
    }
}
