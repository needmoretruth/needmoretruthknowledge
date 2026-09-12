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
    // ---- Stage names of this quest's own six stages --------------------------------
    StageWhat { en: "What it means", ko: "무슨 뜻인가" },
    StageFour { en: "Four systems", ko: "네 가지 방식" },
    StageRun { en: "Run them", ko: "돌려 보기" },
    StageMessages { en: "Three messages", ko: "메시지 셋" },
    StageTune { en: "Your run", ko: "내 설정으로" },
    StageBreak { en: "Break them", ko: "깨 보기" },
    StageSides { en: "Four sides", ko: "네 사람의 눈" },

    // ---- Stage 1: what it means -----------------------------------------------------
    WhatOne { en: "A zero-knowledge proof convinces someone that a statement is true while showing them nothing but the proof.", ko: "영지식 증명은 어떤 주장이 참이라는 것을 상대에게 납득시키면서, 증명 말고는 아무것도 보여 주지 않습니다." },
    WhatTwo { en: "Think of proving you are old enough to buy a drink without handing over your birthday, your name and your address.", ko: "술을 살 나이가 됐다는 것을, 생년월일도 이름도 주소도 건네지 않고 증명한다고 생각해 보세요." },
    WhatThree { en: "The clerk learns one thing, and nothing else.", ko: "점원은 딱 한 가지만 알게 되고, 그 밖에는 아무것도 모릅니다." },
    WhatFour { en: "A public chain is the hardest place in the world to keep a secret.", ko: "공개 체인은 세상에서 비밀을 지키기 가장 어려운 곳입니다." },
    WhatFive { en: "Every node checks every payment, so every node has to be given enough to check it: who paid, who was paid, how much.", ko: "모든 노드가 모든 결제를 검사하니, 모든 노드에게 검사에 필요한 것을 다 줘야 합니다. 누가 냈고, 누가 받았고, 얼마인지." },
    WhatSix { en: "A zero-knowledge proof breaks that trade.", ko: "영지식 증명은 그 맞바꿈을 깹니다." },
    WhatSeven { en: "The node checks a proof instead of the payment, and the amounts and the addresses never go on the chain at all.", ko: "노드는 결제 대신 증명을 검사하고, 금액과 주소는 아예 체인에 올라가지 않습니다." },

    // ---- Stage 2: the four systems ---------------------------------------------------
    FourOne { en: "Four systems run here, in the order the field arrived at them.", ko: "여기서 네 가지 방식이 돕니다. 이 분야가 실제로 거쳐 온 순서 그대로입니다." },
    FourTwo { en: "Each fixes something the one before it could not, and each asks you to trust something different.", ko: "각각은 앞의 것이 풀지 못한 것을 풀고, 각각 다른 것을 믿으라고 요구합니다." },
    FourSigma { en: "The first one needs the two of you there at the same time. Fine for a login, useless for a chain nobody is watching.", ko: "첫 번째 방식은 둘이 같은 자리에 함께 있어야 합니다. 로그인에는 괜찮지만, 아무도 지켜보지 않는 체인에는 쓸 수 없습니다." },
    FourFiatShamir { en: "The second takes the verifier out of the room, and the proof becomes a file. Anyone can check it, a year later, alone.", ko: "두 번째는 검증자를 그 자리에서 빼냅니다. 증명이 파일이 되니, 누구든 일 년 뒤에 혼자서도 검사할 수 있습니다." },
    FourTrustedSetup { en: "The third makes that file far smaller, in exchange for one promise you cannot check: that a certain number was destroyed.", ko: "세 번째는 그 파일을 훨씬 작게 만듭니다. 대신 검사할 수 없는 약속 하나를 받아야 합니다. 어떤 수를 없앴다는 약속입니다." },
    FourHalo2 { en: "The fourth asks for no such promise, and pays for that in time. You will watch it take the longest of the four.", ko: "네 번째는 그런 약속을 요구하지 않습니다. 대신 시간으로 값을 치릅니다. 넷 중 가장 오래 걸리는 것을 곧 보게 됩니다." },
    FourNumbers { en: "Three numbers separate them: how long a proof takes to make, how long it takes to check, and how big it is.", ko: "셋을 가르는 숫자는 셋입니다. 증명을 만드는 시간, 검사하는 시간, 그리고 증명의 크기." },

    // ---- Stage 3: running them ---------------------------------------------------------
    RunOne { en: "Now run all four on this machine.", ko: "이제 넷 모두 이 컴퓨터에서 돌립니다." },
    RunAsk { en: "Press Enter. halo2 takes the longest, because it really compiles a circuit.", ko: "Enter를 누르세요. halo2가 가장 오래 걸립니다. 진짜로 회로를 컴파일하기 때문입니다." },
    RunMeasured { en: "Those are measurements taken here, not numbers quoted from a paper.", ko: "저것은 여기서 실제로 잰 값이지, 논문에서 옮겨 적은 숫자가 아닙니다." },
    RunCheck { en: "A proof that is slow to check is no use to a chain, and a proof that is large is no use to anyone.", ko: "검사가 느린 증명은 체인에 쓸 수 없고, 큰 증명은 누구에게도 쓸모가 없습니다." },
    RunWhy { en: "That is the whole reason the later systems exist.", ko: "뒤에 나온 방식들이 존재하는 이유가 그것입니다." },

    // ---- Stage 4: the three messages ------------------------------------------------------
    SigmaIntro { en: "The first system sends three messages. Five steps to walk, because what sits on the table at the start and the verdict at the end each deserve a stop.", ko: "첫 번째 방식은 메시지를 세 번 주고받습니다. 걸음은 다섯입니다. 처음에 놓여 있는 것과 마지막 판정도 한 번씩 볼 값이 있기 때문입니다." },
    SigmaOneWay { en: "It all rests on one sum that is easy one way and hopeless the other: multiply a secret by a fixed starting point, and nobody can work the secret back out.", ko: "모든 것이 계산 하나에 기댑니다. 한쪽으로는 쉽고 반대로는 가망이 없는 계산입니다. 비밀 수에 고정된 출발점을 곱하면, 아무도 그 비밀을 되짚어 낼 수 없습니다." },
    SigmaNotTimes { en: "This is not the multiplication you know. G is a point on a curve, and multiplying it by x means adding G to itself x times under a rule that wraps around.", ko: "여기서 곱하기는 우리가 아는 곱하기가 아닙니다. G는 곡선 위의 한 점이고, x를 곱한다는 것은 되돌아 감기는 규칙을 따라 G를 자기 자신에 x번 더한다는 뜻입니다." },
    SigmaNotDivide { en: "Adding is quick. Working out how many additions somebody did, from the point they ended on, is the part nobody can do — so there is no dividing back to x.", ko: "더하기는 빠릅니다. 그런데 도착한 점만 보고 몇 번 더했는지 알아내는 일은 아무도 못 합니다. 그래서 x로 되나누는 길이 없습니다." },
    SigmaNames { en: "Three names and that is the whole vocabulary: the secret is x, the fixed starting point is G, and what the two make together is P.", ko: "이름 셋이면 낱말은 끝입니다. 비밀은 x, 고정된 출발점은 G, 둘이 함께 만들어 내는 것이 P입니다." },
    SigmaLesson { en: "Three messages, and the verifier ends up certain without ever seeing the secret.", ko: "메시지 셋으로, 검증자는 비밀을 한 번도 보지 않고 확신에 이릅니다." },

    // ---- Stage 5: your run ------------------------------------------------------------------
    TuneOne { en: "Now your own settings.", ko: "이제 값을 직접 정합니다." },
    TuneCircuit { en: "A circuit is the claim written out as arithmetic: every check the proof has to satisfy, laid out as rows of sums the prover must get right.", ko: "회로는 증명할 주장을 산수로 풀어 적은 것입니다. 증명이 만족해야 하는 검사를 전부, 증명자가 맞춰야 하는 덧셈의 줄로 늘어놓은 것입니다." },
    TuneBits { en: "The circuit size is how wide halo2 holds each amount, in bits.", ko: "회로 크기는 halo2가 금액 하나를 몇 비트 폭으로 담는지입니다." },
    TuneWider { en: "A wider range is a bigger circuit and a slower proof. That is the thing to feel here.", ko: "폭이 넓을수록 회로가 커지고 증명이 느려집니다. 그것을 직접 느껴 보는 것이 여기서 할 일입니다." },
    TuneSeed { en: "The seed decides every random number in the run.", ko: "시드는 이 실행의 모든 무작위 수를 정합니다." },
    TuneSeedTwo { en: "The same seed replays the same proof byte for byte; a different seed does not.", ko: "같은 시드는 같은 증명을 바이트 하나까지 똑같이 다시 만들고, 다른 시드는 그러지 않습니다." },
    TuneAsk { en: "Set the circuit size to 64 bits and press Enter. Watch halo2's proving time.", ko: "회로 크기를 64비트로 놓고 Enter를 누르세요. halo2의 증명 시간을 보세요." },
    TuneAfter { en: "Only halo2 moved. The other three have no circuit to widen.", ko: "halo2만 움직였습니다. 나머지 셋에는 넓힐 회로가 없습니다." },
    TuneNoise { en: "One step at a time barely shows: the measurement wobbles more than the change. Doubling the width is what you can see.", ko: "한 칸씩 올리면 잘 보이지 않습니다. 변화보다 측정값이 더 흔들립니다. 폭을 두 배로 해야 눈에 보입니다." },
    TuneShapeOne { en: "The panel names the circuit halo2 built. Rows are the steps the proof has room for.", ko: "오른쪽에 halo2가 만든 회로가 적혀 있습니다. 행은 그 증명이 쓸 수 있는 단계 수입니다." },
    TuneShapeTwo { en: "Advice columns are the working values the prover fills in; custom gates are the rules every row has to satisfy.", ko: "어드바이스 열은 증명자가 채워 넣는 계산용 값이고, 맞춤 게이트는 모든 행이 지켜야 하는 규칙입니다." },
    TuneShapeThree { en: "Degree is how complicated the hardest of those rules is, and a higher degree costs proving time.", ko: "차수는 그 규칙 중 가장 복잡한 것이 얼마나 복잡한가이고, 차수가 높을수록 증명에 시간이 더 듭니다." },

    // ---- Stage 6: breaking them ---------------------------------------------------------------
    BreakOne { en: "Every attack here is code running against the same verifier the honest proof went through.", ko: "여기 있는 공격은 전부, 정직한 증명이 통과했던 바로 그 검증자를 상대로 실제로 도는 코드입니다." },
    BreakTwo { en: "Nothing is decided in advance.", ko: "미리 정해진 결과는 없습니다." },
    BreakAsk { en: "Press Enter to run all of them.", ko: "Enter를 누르면 전부 돌아갑니다." },
    BreakTwoGetThrough { en: "Two of them got through.", ko: "그중 둘이 통과했습니다." },
    BreakWeakHash { en: "One is a Fiat-Shamir challenge hashed from the statement but not from the commitment.", ko: "하나는 챌린지를 주장에서만 해시하고 커밋먼트는 빼먹은 피아트-샤미르입니다." },
    BreakWeakHashTwo { en: "That lets an attacker choose its answer first and then solve for a commitment that fits it.", ko: "그러면 공격자가 답을 먼저 고르고, 거기에 맞는 커밋먼트를 나중에 구할 수 있습니다." },
    BreakWaste { en: "The other is the holder of a trusted setup's leftover randomness, opening a commitment at a value it does not hold.", ko: "다른 하나는 신뢰 설정에서 남은 난수를 쥔 사람이, 커밋먼트를 그 안에 없는 값으로 여는 것입니다." },
    BreakUnchanged { en: "The verifier was not modified for either one.", ko: "둘 다 검증자는 한 글자도 손대지 않았습니다." },
    BreakCeremony { en: "That is why people ask whether a ceremony was honest.", ko: "사람들이 「그 의식이 정직했나」를 따지는 이유가 이것입니다." },
    BreakCeremonyIs { en: "A ceremony is the one-off gathering that produces a trusted setup's numbers and is supposed to destroy the leftover.", ko: "여기서 의식이란, 신뢰 설정의 수들을 만들고 남은 난수를 없애기로 되어 있는 일회성 행사를 말합니다." },
    BreakPromise { en: "An unaudited ceremony is a promise, not a proof.", ko: "감사받지 않은 의식은 약속이지 증명이 아닙니다." },

    // ---- Stage 7: four sides --------------------------------------------------------------------
    SidesOne { en: "One shielded payment, and four people looking at it at the same time.", ko: "가려진 결제 하나를 네 사람이 동시에 보고 있습니다." },
    SidesTwo { en: "The sender holds the note and the key that spends it. The receiver learns the amount and nothing else.", ko: "보낸 사람은 쪽지와 그것을 쓸 수 있는 키를 갖고 있습니다. 받는 사람은 금액만 알게 되고 그 밖에는 아무것도 모릅니다." },
    SidesThree { en: "The onlooker is the reason any of this exists.", ko: "지켜보는 사람이 이 모든 것이 존재하는 이유입니다." },
    SidesFour { en: "On a shielded chain they see that a transaction happened, the proof, a nullifier, and a commitment standing in for the new note.", ko: "가려진 체인에서 그 사람은 거래가 있었다는 것, 증명, 널리파이어, 그리고 새 쪽지를 대신하는 커밋먼트를 봅니다." },
    SidesKeys { en: "The spend key is what lets a note be spent; the view key only reads it.", ko: "지출 키는 쪽지를 쓸 수 있게 하는 키이고, 열람 키는 읽기만 하는 키입니다." },
    SidesBlinding { en: "The blinding is a random number mixed into the note, so two notes of the same amount do not look alike.", ko: "블라인딩 값은 쪽지에 섞어 넣는 난수입니다. 그래서 금액이 같은 두 쪽지가 똑같아 보이지 않습니다." },
    SidesNullifier { en: "A nullifier is a one-off tag that says \"this note is spent\" without saying which note it was.", ko: "널리파이어는 「이 쪽지는 이미 썼다」를 말하되 어느 쪽지였는지는 말하지 않는 일회용 표식입니다." },
    SidesZcash { en: "This payment is shaped like Zcash's: a note, a commitment standing in for it in public, and a nullifier that spends it exactly once.", ko: "이 결제의 모양은 Zcash의 것입니다. 쪽지가 있고, 공개된 자리에서 그것을 대신하는 커밋먼트가 있고, 그 쪽지를 딱 한 번만 쓰게 하는 널리파이어가 있습니다." },
    SidesFive { en: "They do not see the amount, the sender or the recipient.", ko: "금액도, 보낸 사람도, 받는 사람도 보이지 않습니다." },
    SidesSix { en: "On a transparent chain all three are public to everyone, forever.", ko: "투명한 체인에서는 셋 다 모두에게 영원히 공개됩니다." },
    SidesSeven { en: "Up and down move between the four systems. The attacker's rows are the ones that change.", ko: "위아래로 네 방식 사이를 옮깁니다. 바뀌는 것은 공격자의 줄입니다." },

    // ---- The reader's own numbers, at the end -------------------------------------------
    YoursNothing { en: "Nothing has been measured yet on this machine. Walk back to the running stage and press Enter.", ko: "이 컴퓨터에서 아직 잰 것이 없습니다. 실행 단계로 돌아가 Enter를 누르세요." },
    YoursSpread { en: "Your own numbers, measured here", ko: "여기서 직접 잰 숫자" },
    YoursAttacksNone { en: "Every attack you ran was refused. The systems held.", ko: "돌려 본 공격이 전부 거절됐습니다. 방식들이 막아 냈습니다." },
    YoursAttacksThrough { en: "That many got through against a verifier nobody touched, and that is the whole reason anyone asks how a system was set up.", ko: "아무도 손대지 않은 검증자를 상대로 그만큼이 통과했습니다. 사람들이 그 방식을 어떻게 설정했는지 따지는 이유가 바로 그것입니다." },

    // ---- Things that happened ------------------------------------------------------------------
    EventRunning { en: "running on this machine", ko: "이 컴퓨터에서 도는 중" },
    EventProved { en: "prove", ko: "증명" },
    EventVerified { en: "verify", ko: "검증" },
    EventSize { en: "size", ko: "크기" },
    EventAllDone { en: "all four are done", ko: "넷 다 끝났습니다" },
    EventAttack { en: "attack", ko: "공격" },
    EventMessage { en: "message", ko: "메시지" },
    BriefPanelTitle { en: "The four systems, in order", ko: "네 가지 방식, 나온 순서대로" },
    BriefSigma { en: "Three messages, and the verifier has to be there while you prove.", ko: "메시지 세 번. 증명하는 동안 검증자가 그 자리에 있어야 합니다." },
    BriefFiatShamir { en: "The verifier's coin flip becomes a hash. The conversation becomes a file anyone can check later.", ko: "검증자의 동전 던지기가 해시로 바뀝니다. 주고받던 대화가 누구나 나중에 검사할 수 있는 파일이 됩니다." },
    BriefTrustedSetup { en: "Short proofs, but sound only because a number was destroyed after the setup.", ko: "증명은 짧지만, 설정 뒤에 어떤 수를 없앴다는 전제에서만 안전합니다." },
    BriefHalo2 { en: "A real circuit, really compiled here, with no setup to trust.", ko: "진짜 회로를 여기서 실제로 컴파일합니다. 믿어야 할 설정이 없습니다." },
    // ---- Stage names -----------------------------------------------------------
    StageSigma { en: "Sigma", ko: "시그마" },
    StageFiatShamir { en: "Fiat-Shamir", ko: "피아트-샤미르" },
    StageTrustedSetup { en: "Trusted setup", ko: "신뢰 설정" },
    StageHalo2 { en: "halo2", ko: "halo2" },
    StageAll { en: "all four", ko: "넷 모두" },
    // ---- Run -------------------------------------------------------------------
    RunTitle { en: "Four systems, three numbers", ko: "네 가지 방식, 세 가지 숫자" },
    RunWorking { en: "running on this machine...", ko: "이 컴퓨터에서 도는 중..." },
    RunNotYet { en: "Nothing has run here yet — press Enter.", ko: "여기서는 아직 아무것도 돌지 않았습니다. Enter를 누르세요." },
    EventOutsideRange { en: "outside what this value allows", ko: "이 값이 가질 수 있는 범위 밖입니다" },
    EventSetTo { en: "set to", ko: "맞춘 값" },
    EventNotANumber { en: "that was not a number this value can take", ko: "이 값이 받을 수 있는 숫자가 아닙니다" },
    KeyRunIt { en: "run it with these", ko: "이 값으로 실행" },
    ColumnStage { en: "Stage", ko: "방식" },
    ColumnProve { en: "Prove", ko: "증명" },
    ColumnVerify { en: "Verify", ko: "검증" },
    ColumnSize { en: "Size", ko: "크기" },
    NotRunYet { en: "—" },
    PanelTrimmed { en: "More rows than this screen holds. A taller terminal shows the rest.", ko: "화면 높이에 다 들어가지 않아 남은 줄이 있습니다. 터미널을 세로로 키우면 보입니다." },
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
    WhyStatement { en: "P sits in the open for anyone to look at. Only the prover knows the x behind it, and that is exactly what they are about to prove.", ko: "P는 누구나 볼 수 있게 놓여 있습니다. 그 뒤의 x는 증명자만 알고, 바로 그것을 증명하려는 참입니다." },
    WhyCommitment { en: "The prover picks a throwaway number r and sends R, made from r exactly the way P was made from x. Still nothing about x.", ko: "증명자가 쓰고 버릴 수 r을 골라 R을 보냅니다. P를 x로 만든 것과 똑같은 방법으로 r에서 만든 것입니다. 아직 x에 대해서는 아무것도 없습니다." },
    WhatACommitmentIs { en: "R is a commitment: a short value worked out from something secret.", ko: "R은 커밋먼트입니다. 비밀에서 계산해 낸 짧은 값입니다." },
    WhatACommitmentDoes { en: "Sending it fixes what was chosen without showing it, and it cannot be swapped for another choice later.", ko: "그것을 보내면 무엇을 골랐는지 보이지 않은 채로 고정되고, 나중에 다른 선택으로 바꿔치기할 수 없습니다." },
    WhyChallenge { en: "Only now, with R already in hand, does the verifier draw a fresh number c. The prover could not have prepared an answer for it.", ko: "R을 이미 손에 쥔 지금에서야 검증자가 새 수 c를 뽑습니다. 증명자는 그 수에 대한 답을 미리 준비해 둘 수 없었습니다." },
    WhyResponse { en: "The prover answers s = r + c times x: the secret mixed with the challenge, and buried under the throwaway number. Without x there is nothing to answer with.", ko: "증명자가 s = r + c 곱하기 x로 답합니다. 비밀을 도전 값과 섞은 뒤 쓰고 버릴 수 밑에 묻은 것입니다. x가 없으면 답할 것이 없습니다." },
    WhyVerdict { en: "One check settles it: s times G has to come out equal to R plus c times P. It only adds up if x was really there, and x never appears.", ko: "검사 한 번으로 끝납니다. s 곱하기 G가 R 더하기 c 곱하기 P와 같아야 합니다. x가 실제로 있어야만 맞아떨어지고, x는 끝내 나타나지 않습니다." },
    KeyNextMessage { en: "next message", ko: "다음 메시지" },
    // ---- Tune ------------------------------------------------------------------
    TuneTitle { en: "Your run", ko: "내 설정으로 돌리기" },
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
    KeyTypeNumber { en: "type a number", ko: "숫자 입력" },
    // ---- Break -----------------------------------------------------------------
    BreakTitle { en: "What the attacker got", ko: "공격자가 얻은 것" },
    ForgeGuessed { en: "guessed the response", ko: "응답을 찍어 봄" },
    ForgeWeakBinding { en: "hash that skips the commitment", ko: "커밋먼트를 빠뜨린 해시" },
    ForgeFullBinding { en: "hash that covers the commitment", ko: "커밋먼트까지 덮은 해시" },
    ForgeWithWaste { en: "kept the setup randomness", ko: "설정 난수를 안 버림" },
    ForgeWithoutWaste { en: "the same move without it", ko: "난수 없이 같은 수법" },
    ForgeOverspend { en: "spent more than the note held", ko: "쪽지에 든 것보다 많이 씀" },
    VerdictAccepted { en: "accepted", ko: "통과" },
    VerdictRejected { en: "rejected", ko: "거절" },
    // The same two facts from the system's side, for the column whose mark is about the system.
    // "+ guessed the response  rejected" put a success mark on a refusal and read as a mistake.
    VerdictHeld { en: "held", ko: "막음" },
    VerdictBroken { en: "broken", ko: "뚫림" },
    WasteHolds { en: "The commitment holds", ko: "커밋먼트에 든 값" },
    WasteOpened { en: "It was opened as", ko: "열어 보인 값" },
    WasteVerifier { en: "The unchanged verifier said", ko: "손대지 않은 검증자의 답" },
    WasteNote { en: "Nothing in the verifier was touched. The number that was supposed to be destroyed is the whole difference.", ko: "검증자는 한 글자도 바뀌지 않았습니다. 없앴어야 할 그 수 하나가 차이의 전부입니다." },
    WordTried { en: "tried", ko: "시도" },
    BreakSummaryAccepted { en: "accepted", ko: "통과" },
    KeyRunAttacks { en: "run the attacks", ko: "공격 돌리기" },
    // ---- Recap -----------------------------------------------------------------
    RecapTitle { en: "One payment, four sides", ko: "결제 하나, 네 사람의 눈" },
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
    ItemToxicWaste { en: "leftover randomness", ko: "버렸어야 할 난수" },
    SidesSigmaToo { en: "Sigma is drawn here for the comparison only. As the second stage said, a proof that needs both people present cannot be posted to a chain.", ko: "여기 시그마를 함께 그린 것은 견주어 보기 위해서입니다. 2단계에서 말했듯이, 둘이 동시에 있어야 하는 증명은 체인에 올릴 수 없습니다." },
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
