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

    // ---- Stage names -------------------------------------------------------------
    StageWhy { en: "Why burn power", ko: "왜 전기를 태우나" },
    StagePuzzle { en: "The puzzle", ko: "이 퍼즐" },
    StageMine { en: "Mine a block", ko: "블록 캐기" },
    StageTune { en: "Your numbers", ko: "내 숫자로" },
    StageAttack { en: "The 51% attack", ko: "51% 공격" },
    StageRecap { en: "Recap", ko: "정리" },
    // ---- Stage 1: why anyone burns electricity for this --------------------------
    WhyOne { en: "Thousands of computers that have never met have to agree on one list of who paid whom.", ko: "서로 만난 적 없는 컴퓨터 수천 대가, 누가 누구에게 얼마를 냈는지 한 벌의 목록으로 합의해야 합니다." },
    WhyTwo { en: "Any of them can lie, and none of them can be punished for it.", ko: "그중 누구든 거짓말을 할 수 있고, 거짓말을 해도 벌할 방법이 없습니다." },
    WhyThree { en: "So the rule cannot be \"trust someone\". The rule is \"whoever did the most work wins\".", ko: "그래서 규칙이 「누군가를 믿어라」일 수는 없습니다. 규칙은 「가장 많이 일한 쪽이 이긴다」입니다." },
    WhyFour { en: "Work here means electricity — real, measurable, and expensive.", ko: "여기서 말하는 일이란 전기입니다. 실제로 들고, 잴 수 있고, 비쌉니다." },
    WhyFive { en: "That is proof of work, and the rest of this is you doing it.", ko: "그것이 작업증명입니다. 이제부터는 그 일을 직접 하게 됩니다." },

    // ---- Stage 2: the puzzle -----------------------------------------------------
    PuzzleOne { en: "A block is one page of that list. To add a page you have to find a number.", ko: "블록은 그 목록의 한 쪽입니다. 쪽을 붙이려면 수 하나를 찾아야 합니다." },
    PuzzleTwo { en: "The number has to make the page's fingerprint come out below a target.", ko: "그 수는 쪽의 지문이 목표값보다 작게 나오도록 만들어야 합니다." },
    PuzzleThree { en: "The fingerprint is a hash: SHA-256, run twice, over the 80 bytes of the block header.", ko: "지문은 해시입니다. 블록 헤더 80바이트에 SHA-256을 두 번 돌린 값입니다." },
    PuzzleFour { en: "There is no clever way to find that number. You guess, you check, you guess again.", ko: "그 수를 영리하게 찾는 방법은 없습니다. 찍고, 확인하고, 다시 찍습니다." },
    PuzzleFive { en: "The number you keep changing is called the nonce. It is a counter sitting in the header, nothing more.", ko: "계속 바꾸는 그 수를 논스라고 부릅니다. 헤더 안에 들어 있는 계수기일 뿐입니다." },
    PuzzleSix { en: "On the right is what one block costs here, and what it cost Bitcoin on its first day.", ko: "오른쪽은 여기서 블록 하나에 드는 비용과, 비트코인이 첫날 치렀던 비용입니다." },
    PuzzleSeven { en: "Read the last line. That is how much easier this practice setting is than Bitcoin was in January 2009.", ko: "마지막 줄을 보세요. 이 연습 설정이 2009년 1월의 비트코인보다 얼마나 쉬운지를 말해 줍니다." },

    // ---- Stage 3: mining ---------------------------------------------------------
    MineOne { en: "Now your own machine does it.", ko: "이제 이 컴퓨터가 직접 합니다." },
    MineAsk { en: "Press Enter and your cores start hashing real block headers.", ko: "Enter를 누르면 이 컴퓨터의 코어들이 진짜 블록 헤더를 해시하기 시작합니다." },
    MineTwo { en: "Nothing decided when that block would arrive. It was found by guessing.", ko: "그 블록이 언제 나올지는 아무도 정하지 않았습니다. 찍어서 찾은 것입니다." },
    MineThree { en: "Watch the gaps between blocks. They are not equal, and they are not meant to be.", ko: "블록 사이의 간격을 보세요. 일정하지 않고, 일정할 이유도 없습니다." },
    MineFour { en: "Every hash is an independent try with the same tiny chance of winning.", ko: "해시 한 번 한 번이 같은 작은 확률을 가진 독립된 시도입니다." },
    MineFive { en: "So a machine grinding for an hour is exactly as far from its next block as one that just started.", ko: "그래서 한 시간을 돌린 기계나 방금 시작한 기계나 다음 블록까지의 거리는 똑같습니다." },
    MineSix { en: "The waiting time is an average, never a countdown.", ko: "기다리는 시간은 평균이지 남은 시간이 아닙니다." },
    MineSeven { en: "Your hash rate is on the right: hashes counted, divided by the seconds they took. Not an estimate.", ko: "오른쪽의 해시 속도는 실제로 센 해시 수를 걸린 초로 나눈 값입니다. 추정치가 아닙니다." },
    MineEight { en: "Bitcoin's difficulty 1, where it started in January 2009, costs about 4.3 billion hashes a block.", ko: "비트코인이 2009년 1월에 시작한 난이도 1은 블록 하나에 약 43억 번의 해시가 듭니다." },
    MineNine { en: "At your rate, one block at that difficulty would take the time shown on the right.", ko: "지금 속도라면 그 난이도에서 블록 하나에 오른쪽에 적힌 만큼 걸립니다." },
    MineTen { en: "The 2009 network figure beside it comes from the protocol itself — difficulty 1, ten minutes a block. Nobody's computer was measured for it.", ko: "그 옆의 2009년 네트워크 수치는 프로토콜 자체의 숫자 — 난이도 1, 블록 간격 10분 — 에서 계산한 값입니다. 누군가의 컴퓨터를 잰 것이 아닙니다." },
    MineEleven { en: "Compare the two and you have your share of what the whole network was then.", ko: "둘을 견주면 그때의 네트워크 전체에서 이 컴퓨터가 차지했을 비중이 나옵니다." },

    // ---- Stage 4: your numbers ----------------------------------------------------
    TuneOne { en: "Four things are yours now: the difficulty, how many miners share this machine, how many threads they get, and each miner's share.", ko: "이제 네 가지를 직접 정합니다. 난이도, 이 컴퓨터를 나눠 쓸 채굴자 수, 그들이 받을 스레드 수, 각자의 몫입니다." },
    TuneKeys { en: "Up and down pick a value, left and right change it, or just type a number. Enter runs with them.", ko: "위아래로 값을 고르고 좌우로 바꿉니다. 숫자를 그냥 입력해도 됩니다. Enter를 누르면 그 값으로 돌립니다." },
    TuneBits { en: "Each extra bit of practice difficulty doubles what one block costs.", ko: "연습 난이도가 1비트 올라갈 때마다 블록 하나에 드는 비용이 두 배가 됩니다." },
    TuneAskBits { en: "Add one bit to the difficulty and press Enter.", ko: "난이도를 1 올리고 Enter를 누르세요." },
    TuneBitsUnchanged { en: "The difficulty is where it was, so the cost is too. Move it and press Enter to watch the cost move with it.", ko: "난이도가 그대로라 비용도 그대로입니다. 난이도를 옮기고 Enter를 누르면 비용이 따라 움직입니다." },
    TuneBitsUp { en: "Every bit doubles the work, and nothing about the machine changed. Read the cost line on the right.", ko: "비트 하나마다 할 일이 두 배가 됩니다. 컴퓨터는 아무것도 바뀌지 않았습니다. 오른쪽 비용 줄을 보세요." },
    TuneBitsDown { en: "Every bit taken off halves the work. Blocks are cheaper now, and nothing about the machine changed.", ko: "비트를 하나 뺄 때마다 할 일이 절반이 됩니다. 블록이 그만큼 싸졌고, 컴퓨터는 아무것도 바뀌지 않았습니다." },
    TuneWhole { en: "Threads are whole things. A machine with seven of them cannot give one miner 51% and another 49%.", ko: "스레드는 쪼갤 수 없습니다. 스레드가 일곱 개인 기계는 한 채굴자에게 51%, 다른 채굴자에게 49%를 줄 수 없습니다." },
    TuneAskMiners { en: "Set the miners to three and press Enter.", ko: "채굴자 수를 셋으로 바꾸고 Enter를 누르세요." },
    TuneAsked { en: "The panel shows the share each miner asked for beside the share it really holds. They rarely match.", ko: "오른쪽에 각 채굴자가 요청한 몫과 실제로 쥔 몫이 나란히 있습니다. 둘이 맞는 경우는 드뭅니다." },
    TuneAddUp { en: "Shares do not have to add up to 100. Three miners asking for 1, 1 and 2 get a quarter, a quarter and a half.", ko: "몫의 합이 100일 필요는 없습니다. 셋이 1, 1, 2를 요청하면 각각 4분의 1, 4분의 1, 2분의 1을 갖습니다." },
    TuneOver { en: "Asking for more threads than there are cores brings the two numbers together, at a small cost in speed.", ko: "코어보다 많은 스레드를 요청하면 두 숫자가 가까워집니다. 대신 전체 속도가 조금 줄어듭니다." },

    // ---- Stage 5: the 51% attack ---------------------------------------------------
    AttackOne { en: "Someone pays a merchant, and waits.", ko: "누군가 상인에게 돈을 내고 기다립니다." },
    AttackTwo { en: "The payment goes into a block, more blocks land on top, and at the confirmation count the merchant trusts, the goods are handed over.", ko: "결제가 블록에 들어가고 그 위에 블록이 더 쌓입니다. 상인이 믿는 확인 수에 이르면 물건이 건네집니다." },
    AttackThree { en: "Meanwhile the buyer has been mining a chain of their own in private, branching from the block before the payment.", ko: "그동안 산 사람은 결제 직전 블록에서 갈라진 자기만의 체인을 몰래 캐고 있었습니다." },
    AttackFour { en: "In that private chain the same coin was paid back to the buyer instead.", ko: "그 체인에서는 같은 코인이 상인이 아니라 산 사람 자신에게 지불됩니다." },
    AttackFive { en: "When the private chain carries more work than the public one, the buyer publishes it.", ko: "비밀 체인에 쌓인 작업이 공개 체인보다 많아지면 산 사람이 그것을 공개합니다." },
    AttackSix { en: "Every node switches to it, because that is the rule every node follows. The payment the merchant watched confirm is simply not there any more.", ko: "모든 노드가 그쪽으로 갈아탑니다. 그것이 모든 노드가 따르는 규칙이기 때문입니다. 상인이 확정되는 것을 지켜본 그 결제는 이제 그냥 없습니다." },
    AttackSeven { en: "Whether this works is arithmetic, not cleverness.", ko: "이게 되느냐 안 되느냐는 영리함이 아니라 산수입니다." },
    AttackAskLow { en: "Give the attacker 30% of the hash power and press Enter.", ko: "공격자에게 해시 파워의 30%를 주고 Enter를 누르세요." },
    AttackLost { en: "Below half, the attacker falls behind and keeps falling — paying for hash power the whole way down.", ko: "절반에 못 미치면 공격자는 뒤처지고 계속 뒤처집니다. 그 내내 해시 파워 값을 치르면서 말입니다." },
    AttackAskHigh { en: "Now give the attacker 51% and press Enter again.", ko: "이번에는 공격자에게 51%를 주고 다시 Enter를 누르세요." },
    AttackWon { en: "Above half it catches up eventually. Not quickly and not cheaply, but eventually.", ko: "절반을 넘으면 언젠가는 따라잡습니다. 빠르지도 싸지도 않지만, 언젠가는 됩니다." },
    AttackLuckyWin { en: "Below half, and it still got there. A short race is chance as much as arithmetic; run it again and it usually does not.", ko: "절반에 못 미치는데도 해냈습니다. 짧은 경주는 계산만큼이나 운입니다. 다시 돌리면 대개는 안 됩니다." },
    AttackRanOut { en: "Above half and it still stopped short. Catching up is slow, and this one ran out of time before it got there.", ko: "절반을 넘는데도 못 미치고 멈췄습니다. 따라잡는 데는 시간이 걸리고, 이번에는 그 전에 시간이 다했습니다." },
    AttackNotYetRun { en: "Nothing has been attacked yet. Press Enter to send the attacker at the chain.", ko: "아직 아무것도 공격하지 않았습니다. Enter를 누르면 공격자가 체인으로 갑니다." },
    AttackWhyName { en: "That is why the number has a name. 51% is not a trick — it is where the arithmetic changes sides.", ko: "그래서 그 숫자에 이름이 붙었습니다. 51%는 무슨 묘수가 아니라, 산수가 편을 바꾸는 지점입니다." },

    // ---- Stage 6: recap ------------------------------------------------------------
    RecapOne { en: "You hashed headers until one came out below a target. That is the whole of mining.", ko: "목표값보다 작은 값이 나올 때까지 헤더를 해시했습니다. 채굴이란 그게 전부입니다." },
    RecapTwo { en: "Every number you saw was counted on this machine, not estimated.", ko: "본 숫자는 전부 이 컴퓨터에서 실제로 센 것이지 추정치가 아닙니다." },
    RecapThree { en: "You changed the difficulty and the miners, and the run answered with different blocks.", ko: "난이도와 채굴자를 바꿨고, 그때마다 나오는 블록이 달라졌습니다." },
    RecapFour { en: "You bought hash power and tried to erase a payment somebody had already been paid for.", ko: "해시 파워를 사서, 누군가 이미 받은 결제를 지우려고 했습니다." },
    RecapFive { en: "It failed below half and worked above it. Both endings are the lesson.", ko: "절반에 못 미치면 실패하고 넘으면 성공했습니다. 두 결말 모두가 이 배움의 내용입니다." },
    RecapSix { en: "The panel holds your own numbers, not anybody else's.", ko: "오른쪽 숫자는 남의 것이 아니라 전부 직접 만든 것입니다." },

    // ---- Things that happened, reported as they happened ----------------------------
    // Label first, value after: that order reads in every language this program will ever speak.
    EventMiningStarted { en: "mining started", ko: "채굴 시작" },
    EventBlock { en: "block", ko: "블록" },
    EventGap { en: "gap", ko: "간격" },
    EventFoundBy { en: "found by", ko: "찾은 이" },
    EventAttackStarted { en: "attack started", ko: "공격 시작" },
    EventPaid { en: "the payment is in a block", ko: "결제가 블록에 들어갔습니다" },
    EventReleased { en: "the goods were handed over", ko: "물건이 건네졌습니다" },
    EventErased { en: "blocks thrown away", ko: "버려진 블록" },
    EventTook { en: "took", ko: "걸린 시간" },
    EventShare { en: "attacker's share", ko: "공격자의 몫" },
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
    LabelYouAre { en: "This machine is", ko: "이 컴퓨터는" },
    UnitTimesNetwork { en: "x that network", ko: "배" },
    ComparisonDerived { en: "worked out from the protocol, not measured", ko: "프로토콜에서 계산한 값이고 실측이 아닙니다" },
    HeadingRecentBlocks { en: "Newest blocks", ko: "최근 블록" },
    Unavailable { en: "—" },
    // ---- Tune ------------------------------------------------------------------
    TuneTitle { en: "Your numbers", ko: "내 숫자로" },
    KnobDifficulty { en: "Practice difficulty", ko: "연습 난이도" },
    KnobMiners { en: "Miners", ko: "채굴자 수" },
    KnobThreads { en: "Threads", ko: "스레드" },
    KnobShareOne { en: "Miner 1 share", ko: "채굴자 1의 몫" },
    KnobShareTwo { en: "Miner 2 share", ko: "채굴자 2의 몫" },
    KnobShareThree { en: "Miner 3 share", ko: "채굴자 3의 몫" },
    KnobShareFour { en: "Miner 4 share", ko: "채굴자 4의 몫" },
    UnitZeroBits { en: "zero bits", ko: "개의 0비트" },
    MinerOne { en: "Miner 1", ko: "채굴자 1" },
    MinerTwo { en: "Miner 2", ko: "채굴자 2" },
    MinerThree { en: "Miner 3", ko: "채굴자 3" },
    MinerFour { en: "Miner 4", ko: "채굴자 4" },
    HeadingSplit { en: "Threads and shares", ko: "스레드와 몫" },
    ColumnMiner { en: "Miner", ko: "채굴자" },
    ColumnThreads { en: "threads", ko: "스레드" },
    ColumnAsked { en: "asked", ko: "요청" },
    ColumnGot { en: "got", ko: "실제" },
    TuneMismatch { en: "threads are whole things", ko: "스레드는 쪼갤 수 없습니다" },
    // ---- Break -----------------------------------------------------------------
    BreakTitle { en: "The 51% attack", ko: "51% 공격" },
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
    // ---- Recap -----------------------------------------------------------------
    RecapTitle { en: "What just happened", ko: "방금 본 것" },
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
