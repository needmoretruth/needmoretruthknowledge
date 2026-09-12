//! Every word this quest says, in one table.
//!
//! `nmtk-transformer` returns numbers, enums and raw model output, and not one of them is a
//! sentence. This is where a `ConfigError::HeadsDoNotDivideWidth` becomes something a reader
//! understands, and it is the only file in the crate that a translator ever has to open.
//!
//! English is the source of truth and is never missing. Korean is written later and falls back to
//! the English line rather than to a blank, so a half-translated screen still reads.

use nmtk_transformer::{ConfigError, StartError};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title { en: "Transformer", ko: "트랜스포머" },
    Summary { en: "Train a real transformer on this machine until four letters become a sentence.", ko: "이 컴퓨터에서 진짜 트랜스포머를 학습시켜, 네 글자가 한 문장이 되게 만듭니다." },
    Subcategory { en: "Architectures", ko: "구조" },
    // ---- Stage names -------------------------------------------------------------
    StageQuestion { en: "One question", ko: "질문 하나" },
    StagePieces { en: "The pieces", ko: "부품들" },
    StageTrain { en: "Train it", ko: "학습시키기" },
    StageTune { en: "Your settings", ko: "내 설정" },
    StageBreak { en: "Break it", ko: "무너뜨리기" },
    StageRecap { en: "Recap", ko: "정리" },

    // ---- Stage 1: the one question --------------------------------------------
    QuestionOne { en: "A language model only ever answers one question.", ko: "언어 모델이 하는 질문은 하나뿐입니다." },
    QuestionTwo { en: "Given the characters so far, which character comes next?", ko: "지금까지의 글자를 봤을 때, 다음 글자는 무엇인가?" },
    QuestionThree { en: "Everything else — every layer, every weight — is machinery for answering that well.", ko: "나머지는 전부, 층도 가중치도, 그 질문에 잘 답하기 위한 장치입니다." },
    QuestionFour { en: "On the right is the model this machine is about to build, and how many numbers it holds.", ko: "오른쪽은 이 컴퓨터가 곧 만들 모델과, 그 안에 든 숫자의 개수입니다." },
    QuestionFive { en: "Those numbers are random right now and mean nothing at all.", ko: "그 숫자들은 지금 무작위이고 아무 뜻도 없습니다." },

    // ---- Stage 2: the pieces ----------------------------------------------------
    PiecesOne { en: "Each character starts as a short list of numbers called an embedding.", ko: "글자 하나하나는 임베딩이라고 부르는 짧은 숫자 목록으로 시작합니다." },
    PiecesTwo { en: "Nothing in that list means anything yet. Training is what gives those numbers meaning.", ko: "그 목록은 아직 아무 뜻도 없습니다. 그 숫자에 뜻을 만들어 주는 것이 학습입니다." },
    PiecesThree { en: "A second list says where in the text the character sits.", ko: "두 번째 목록은 그 글자가 글의 몇 번째에 있는지를 말합니다." },
    PiecesFour { en: "It has to: nmtk and kmtn are the same four characters in a different order.", ko: "그래야 합니다. nmtk와 kmtn은 같은 네 글자를 순서만 바꾼 것이니까요." },
    PiecesFive { en: "Attention is every position looking back at the positions before it.", ko: "어텐션이란 각 자리가 자기 앞의 자리들을 돌아보는 일입니다." },
    PiecesSix { en: "It picks which of them matter for what comes next. A position can never see the future.", ko: "그중 어느 자리가 다음에 올 것을 정하는 데 중요한지 고릅니다. 어떤 자리도 미래는 볼 수 없습니다." },
    PiecesSeven { en: "Four of the numbers on the right decide the model's shape: width, heads, layers and window.", ko: "오른쪽 숫자 가운데 넷이 모델의 모양을 정합니다. 너비, 헤드, 층, 창입니다." },
    PiecesCharacters { en: "Characters, on the same panel, is how many different characters the text uses. The model keeps one row of numbers for each of them.", ko: "같은 패널의 글자 수는 이 글에 서로 다른 글자가 몇 개 쓰였는지입니다. 모델은 그 하나하나에 숫자 한 줄씩을 갖습니다." },
    PiecesWidth { en: "Width is how long each of those lists of numbers is.", ko: "너비는 그 숫자 목록 하나의 길이입니다." },
    PiecesHeads { en: "Heads are how many separate looks-back happen at once, each with its own slice of that width.", ko: "헤드는 그 돌아보기를 동시에 몇 갈래로 하는가입니다. 갈래마다 너비의 한 조각씩을 씁니다." },
    PiecesLayers { en: "Layers are how many times the whole looking-back-and-thinking is repeated, one on top of the last.", ko: "층은 그 돌아보고 생각하는 일을 몇 번 되풀이하는가입니다. 앞의 것 위에 하나씩 쌓습니다." },
    PiecesWindow { en: "The window is how many characters back the model is allowed to look at all.", ko: "창은 모델이 뒤로 최대 몇 글자까지 볼 수 있는가입니다." },
    PiecesCount { en: "Add all of that up and you get the weight count on the right.", ko: "그것을 다 더한 것이 오른쪽의 가중치 수입니다." },
    PiecesPromise { en: "This machine is about to change every one of those numbers a few thousand times, on your own cores, with no network involved.", ko: "이 컴퓨터가 곧 그 숫자 하나하나를 몇천 번씩 바꿉니다. 내 코어에서, 네트워크 없이." },

    // ---- Stage 3: training --------------------------------------------------------
    TrainOne { en: "Now train it.", ko: "이제 학습시킵니다." },
    TrainAsk { en: "Press Enter. It takes tens of seconds, and Space pauses it between steps.", ko: "Enter를 누르세요. 수십 초 걸리고, Space로 스텝 사이에서 멈출 수 있습니다." },
    TrainLoss { en: "Loss is measured in nats per character: how surprised the model was by the character that actually came next.", ko: "손실은 글자당 nat 단위입니다. 실제로 다음에 온 글자를 보고 모델이 얼마나 놀랐는가입니다." },
    TrainScale { en: "Guessing blindly over this alphabet scores about 3.3. A model that has learned the text is well under 0.5.", ko: "이 글자 집합에서 아무렇게나 찍으면 약 3.3입니다. 글을 익힌 모델은 0.5보다 한참 아래입니다." },
    TrainAnswer { en: "Under the numbers is the model's own answer to nmtk, asked again about once a second with the weights exactly as they stand.", ko: "숫자 아래는 지금 이 순간의 가중치 그대로 nmtk에 대해 모델이 내놓은 답입니다. 1초에 한 번쯤 다시 물어봅니다." },
    TrainNoise { en: "Early on it is noise. Then it is a word. Then it is the sentence.", ko: "처음에는 잡음이고, 그다음에는 단어가 되고, 그다음에는 문장이 됩니다." },
    TrainDone { en: "That is the whole of it. There is no other ingredient.", ko: "그게 전부입니다. 다른 재료는 없습니다." },
    TrainGrid { en: "The grid on the right is the attention of the most recent forward pass: one row per position, one column per position it could look back at.", ko: "오른쪽 격자는 가장 최근 순전파의 어텐션입니다. 가로 한 줄이 한 자리, 세로 한 칸이 그 자리가 돌아본 자리입니다." },
    TrainSlowing { en: "The fall has flattened out. Most of what this model will learn, it learned in the first few hundred steps.", ko: "내려가는 기세가 꺾였습니다. 이 모델이 배울 것의 대부분은 처음 몇백 스텝에서 이미 배웠습니다." },
    TrainStillFalling { en: "It is still falling. This run has not found its floor yet.", ko: "아직 내려가고 있습니다. 이번 판은 바닥을 아직 못 찾았습니다." },
    TrainGridRead { en: "A dark cell means that row drew heavily on that column. Left and right change which layer and head you are looking at.", ko: "칸이 진할수록 그 줄이 그 칸에 많이 기댔다는 뜻입니다. 좌우로 어느 층, 어느 헤드를 볼지 바꿉니다." },

    // ---- Stage 4: your settings ----------------------------------------------------
    SettingsOne { en: "Now the shape is yours.", ko: "이제 모양을 직접 정합니다." },
    SettingsTwo { en: "Layers, heads and width decide how big the model is.", ko: "층·헤드·너비가 모델의 크기를 정합니다." },
    SettingsThree { en: "Weights grow roughly with the square of the width, and every extra weight is more arithmetic per step.", ko: "가중치는 너비의 제곱에 가깝게 늘고, 가중치가 늘수록 스텝마다 할 계산이 많아집니다." },
    SettingsFour { en: "So a bigger model is not automatically a better one in the minute you have.", ko: "그래서 1분 안에 끝내야 하는 지금은 큰 모델이 자동으로 더 좋은 모델은 아닙니다." },
    SettingsFive { en: "The learning rate is how far every weight moves on each step.", ko: "학습률은 스텝마다 가중치 하나가 얼마나 움직이는가입니다." },
    SettingsSix { en: "Too small and the run never arrives; too large and it steps straight over the answer.", ko: "너무 작으면 영영 도착하지 못하고, 너무 크면 답을 지나쳐 버립니다." },
    SettingsSeven { en: "The width is shared out between the heads, so it has to divide evenly by them.", ko: "너비는 헤드들이 나눠 갖기 때문에 헤드 수로 나누어떨어져야 합니다." },
    SettingsAsk { en: "Set the width to 32 and press Enter. That is roughly a quarter of the weights.", ko: "너비를 32로 놓고 Enter를 누르세요. 가중치가 대략 4분의 1이 됩니다." },
    SettingsFirstRun { en: "That is the first run at settings of your own, so there is nothing yet to hold it against. Change something and press Enter again.", ko: "직접 고른 설정으로 돌린 첫 판이라 아직 견줄 것이 없습니다. 무언가 바꾸고 Enter를 다시 누르세요." },
    SettingsSameShape { en: "The model is the same size as before, so whatever changed, changed in how it was trained.", ko: "모델 크기는 앞과 같습니다. 그러니 달라진 것이 있다면 학습하는 방식에서 달라진 것입니다." },
    SettingsSmallerWorse { en: "Fewer weights, and a loss that stopped higher up. Smaller is faster and worse.", ko: "가중치는 줄었고 손실은 더 높은 데서 멈췄습니다. 작을수록 빠르고 나쁩니다." },
    SettingsSmallerBetter { en: "Fewer weights and a lower loss: for this sentence the shape mattered more than the size.", ko: "가중치는 줄었는데 손실도 내려갔습니다. 이 문장에서는 크기보다 모양이 더 컸던 것입니다." },
    SettingsBiggerBetter { en: "More weights and a lower loss, bought with the extra time the run took.", ko: "가중치가 늘었고 손실은 내려갔습니다. 그만큼 시간을 더 쓴 값입니다." },
    SettingsBiggerWorse { en: "More weights and a higher loss. Bigger is not automatically better.", ko: "가중치는 늘었는데 손실은 올랐습니다. 크다고 저절로 좋아지지는 않습니다." },
    SettingsRefused { en: "The weight count on the right goes to nothing when the settings do not describe a model, and says why.", ko: "설정이 모델을 이루지 못하면 오른쪽 가중치 수가 사라지고 이유가 나옵니다." },

    // ---- Stage 5: breaking it --------------------------------------------------------
    BreakOne { en: "The learning rate is the easiest thing here to get wrong.", ko: "여기서 가장 틀리기 쉬운 것이 학습률입니다." },
    BreakTwo { en: "Every step moves each weight that far in the direction that would have helped.", ko: "스텝마다 모든 가중치가 도움이 됐을 방향으로 그만큼 움직입니다." },
    BreakThree { en: "Make the step large enough and it flies past the answer by more than it was wrong to begin with.", ko: "그 폭이 충분히 크면 원래 틀렸던 것보다 더 많이 답을 지나쳐 버립니다." },
    BreakFour { en: "Then it does it again, the other way.", ko: "그다음에는 반대쪽으로 또 그럽니다." },
    BreakAskRunaway { en: "The attack on the right is a runaway rate, three thousand times the sensible one. Press Enter.", ko: "오른쪽 공격은 적당한 값의 3000배짜리 고삐 풀린 학습률입니다. Enter를 누르세요." },
    BreakAfterRunaway { en: "The loss never once got below where it started, and the answer collapsed into one letter repeated.", ko: "손실이 처음 값 아래로 한 번도 안 내려갔고, 답은 글자 하나만 되풀이하며 무너졌습니다." },
    BreakNotThatRun { en: "Those were not the settings just suggested, so this is about the run you made instead.", ko: "방금 말한 설정은 아니었으므로, 아래는 대신 돌린 그 판에 대한 이야기입니다." },
    BreakRanAt { en: "loss", ko: "손실" },
    EventOutsideRange { en: "outside what this value allows", ko: "이 값이 가질 수 있는 범위 밖입니다" },
    EventSetTo { en: "set to", ko: "맞춘 값" },
    EventNotANumber { en: "that was not a number this value can take", ko: "이 값이 받을 수 있는 숫자가 아닙니다" },
    KeyRunIt { en: "run it with these", ko: "이 값으로 실행" },
    BreakItLearned { en: "The loss came down, so whatever you changed, the model still learned.", ko: "손실이 내려갔습니다. 무엇을 바꿨든 모델은 그래도 배웠습니다." },
    BreakItDidNot { en: "The loss never got below where it started — this run learned nothing at all.", ko: "손실이 처음 값 아래로 한 번도 안 내려갔습니다. 이 판은 아무것도 배우지 못했습니다." },
    BreakWarmupOne { en: "The second attack removes the warmup.", ko: "두 번째 공격은 워밍업을 없앱니다." },
    BreakGradient { en: "A gradient says which way, and how hard, one weight should move to make the model less surprised. Every step works one out for every weight.", ko: "기울기는 그 가중치를 어느 쪽으로 얼마나 밀어야 모델이 덜 놀라는지를 말합니다. 스텝마다 가중치 하나하나에 대해 그것을 계산합니다." },
    BreakWarmupTwo { en: "A transformer's first steps are its most dangerous: attention is spread almost evenly and the gradients are large.", ko: "트랜스포머에게 가장 위험한 때는 첫 몇 스텝입니다. 어텐션이 거의 고르게 퍼져 있고 기울기가 큽니다." },
    BreakWarmupThree { en: "So the sensible schedule ramps the rate up from nothing over the first hundred steps.", ko: "그래서 제대로 된 일정은 처음 100스텝에 걸쳐 학습률을 0에서부터 올립니다." },
    BreakAskWarmup { en: "Choose that attack, set the multiplier to 10, and press Enter.", ko: "그 공격을 고르고 배수를 10으로 놓은 다음 Enter를 누르세요." },
    BreakAfterWarmup { en: "At the rate you set, the ramp barely matters. At ten times it is the difference between the sentence and a misspelling of it.", ko: "직접 정한 학습률에서는 그 경사가 별 차이를 안 냅니다. 10배에서는 문장과 오타 사이의 차이가 됩니다." },
    BreakSgdOne { en: "The third is not sabotage but a fair fight.", ko: "세 번째는 방해가 아니라 정정당당한 대결입니다." },
    BreakSgdName { en: "SGD is short for stochastic gradient descent: walk downhill, using the gradient worked out from a random handful of examples.", ko: "SGD는 stochastic gradient descent, 확률적 경사 하강법의 줄임말입니다. 무작위로 뽑은 예시 몇 개에서 구한 기울기를 따라 내리막을 걷는 것입니다." },
    BreakSgdTwo { en: "Plain SGD moves every weight by the same rate and remembers nothing.", ko: "순수 SGD는 모든 가중치를 같은 학습률로 움직이고 아무것도 기억하지 않습니다." },
    BreakSgdThree { en: "AdamW gives each weight its own step size, worked out from that weight's own recent history.", ko: "AdamW는 가중치마다 자기 최근 이력에서 뽑은 자기만의 보폭을 줍니다." },
    BreakAdamName { en: "Adam is short for adaptive moment estimation, and the W is the weight decay it adds. It is what this quest has been using all along.", ko: "Adam은 adaptive moment estimation, 적응적 모멘트 추정의 줄임말이고 W는 거기 붙인 weight decay입니다. 이 퀘스트가 내내 써 온 것이 이것입니다." },
    BreakAskSgd { en: "Choose plain SGD, leave the multiplier at 1, and press Enter.", ko: "순수 SGD를 고르고 배수는 1로 둔 채 Enter를 누르세요." },
    BreakAfterSgd { en: "It barely moved at all.", ko: "거의 움직이지 않았습니다." },
    BreakAskSgdAgain { en: "Now turn the multiplier up to 100 and press Enter again.", ko: "이번에는 배수를 100까지 올리고 다시 Enter를 누르세요." },
    BreakLesson { en: "It finds the sentence again, which is the point: a rate is only sensible for the rule that is using it.", ko: "다시 그 문장을 찾아냅니다. 그것이 요점입니다. 학습률이 적당한가는 그것을 쓰는 규칙에 달려 있습니다." },

    // ---- Stage 6: recap ----------------------------------------------------------------
    RecapOne { en: "You started with a few tens of thousands of random numbers that meant nothing.", ko: "아무 뜻도 없는 무작위 숫자 수만 개에서 시작했습니다." },
    RecapTwo { en: "And a piece of text those numbers had never seen.", ko: "그리고 그 숫자들이 한 번도 본 적 없는 글 한 편에서요." },
    RecapThree { en: "Every step drew a batch of windows from that text and ran them through the embeddings and the attention layers.", ko: "스텝마다 그 글에서 창 여러 개를 뽑아 임베딩과 어텐션 층을 통과시켰습니다." },
    RecapFour { en: "It measured how surprised the model was by the character that actually came next.", ko: "실제로 다음에 온 글자에 모델이 얼마나 놀랐는지 쟀습니다." },
    RecapFive { en: "Then it pushed every weight a small distance in the direction that would have made it less surprised.", ko: "그리고 덜 놀랐을 방향으로 모든 가중치를 조금씩 밀었습니다." },
    RecapSix { en: "A few thousand of those steps is the entire difference between noise and a sentence.", ko: "그 스텝 몇천 번이 잡음과 문장 사이의 차이 전부입니다." },
    RecapSeven { en: "There is no other ingredient, and everything that happened, happened on this machine.", ko: "다른 재료는 없고, 벌어진 일은 전부 이 컴퓨터에서 벌어졌습니다." },

    // ---- Things that happened ------------------------------------------------------------
    EventStarted { en: "training started", ko: "학습 시작" },
    EventLoss { en: "loss", ko: "손실" },
    EventStep { en: "step", ko: "스텝" },
    EventGotIt { en: "it answers with the sentence", ko: "그 문장으로 답했습니다" },
    EventFinished { en: "finished", ko: "끝났습니다" },
    EventNeverFell { en: "the loss never got below where it started", ko: "손실이 처음 값 아래로 내려가지 않았습니다" },
    EventSeconds { en: "took", ko: "걸린 시간" },
    BriefShapeTitle { en: "What this machine will build", ko: "이 컴퓨터가 만들 모델" },
    LabelVocabulary { en: "Characters", ko: "글자 수" },
    LabelLayers { en: "Layers", ko: "층" },
    LabelHeads { en: "Heads", ko: "헤드" },
    LabelWidth { en: "Width", ko: "너비" },
    LabelContext { en: "Window", ko: "창" },
    LabelWeights { en: "Weights", ko: "가중치" },
    // ---- Run -------------------------------------------------------------------
    RunTitle { en: "Training", ko: "학습" },
    LabelStep { en: "Step", ko: "스텝" },
    LabelLoss { en: "Loss", ko: "손실" },
    LabelSpeed { en: "Speed", ko: "속도" },
    LabelElapsed { en: "Elapsed", ko: "걸린 시간" },
    LabelRate { en: "Rate", ko: "학습률" },
    UnitCharsPerSecond { en: "chars/s", ko: "자/초" },
    LabelAnswer { en: "Answer", ko: "답" },
    LabelCurve { en: "Loss over the run", ko: "학습 동안의 손실" },
    AnswerWaiting { en: "the first answer is about a second away", ko: "첫 답까지 1초쯤 남았습니다" },
    AnswerRight { en: "that is the sentence", ko: "이것이 그 문장입니다" },
    AnswerNotYet { en: "not the sentence yet", ko: "아직 그 문장이 아닙니다" },
    AnswerBlank { en: "it answers with nothing but spaces", ko: "공백만 내놓고 있습니다" },
    StatusRunning { en: "training", ko: "학습 중" },
    StatusPaused { en: "paused", ko: "일시정지" },
    StatusFinished { en: "finished", ko: "끝남" },
    NotANumber { en: "not a number", ko: "숫자가 아님" },
    // ---- Attention -------------------------------------------------------------
    AttentionTitle { en: "Where the model looked", ko: "모델이 본 곳" },
    AttentionWaiting { en: "No forward pass has been captured yet.", ko: "아직 잡아 둔 순전파가 없습니다." },
    LabelLayer { en: "layer", ko: "층" },
    LabelHead { en: "head", ko: "헤드" },
    KnobView { en: "Showing", ko: "보는 중" },
    AttentionLegend { en: "each row is a position, each column one it looked back at", ko: "가로 한 줄이 한 자리, 세로 한 칸이 그 자리가 돌아본 자리" },
    AttentionNewest { en: "newest", ko: "최근" },
    AttentionMarks { en: "a space is drawn _ and a line break /", ko: "빈칸은 _, 줄바꿈은 /로 그립니다" },
    // ---- Tune ------------------------------------------------------------------
    TuneTitle { en: "Your settings", ko: "내 설정" },
    KnobLayers { en: "Layers", ko: "층" },
    KnobHeads { en: "Heads", ko: "헤드" },
    KnobWidth { en: "Model width", ko: "모델 너비" },
    KnobLearningRate { en: "Learning rate", ko: "학습률" },
    KnobSteps { en: "Steps", ko: "스텝 수" },
    LabelPresets { en: "worth trying", ko: "해 볼 만한 값" },
    LabelWeightsNow { en: "Weights now", ko: "지금 가중치" },
    LabelMachineChose { en: "This machine chose", ko: "이 컴퓨터가 고른 값" },
    // ---- Break -----------------------------------------------------------------
    BreakTitle { en: "Break it", ko: "무너뜨리기" },
    LabelAttack { en: "Attack", ko: "공격" },
    LabelMultiplier { en: "Rate multiplier", ko: "학습률 배수" },
    AttackRunaway { en: "a runaway learning rate", ko: "고삐 풀린 학습률" },
    AttackNoWarmup { en: "no warmup at all", ko: "워밍업 아예 없음" },
    AttackPlainSgd { en: "plain SGD, no memory", ko: "기억 없는 순수 SGD" },
    LabelSensibleRate { en: "The rate you set", ko: "직접 정한 학습률" },
    LabelThisRun { en: "This run", ko: "이번 실행" },
    LabelHonestRun { en: "Honest run", ko: "정상 실행" },
    LabelBrokenRun { en: "Broken run", ko: "망가뜨린 실행" },
    BreakClimbing { en: "the loss is climbing", ko: "손실이 올라가고 있습니다" },
    BreakBlewUp { en: "the loss stopped being a number at all", ko: "손실이 아예 숫자가 아니게 됐습니다" },
    // ---- Recap -----------------------------------------------------------------
    RecapTitle { en: "What just happened", ko: "방금 본 것" },
    RecapNothing { en: "Nothing has been trained yet.", ko: "아직 학습시킨 것이 없습니다." },
    RecapWeights { en: "Weights trained", ko: "학습시킨 가중치" },
    RecapSteps { en: "Steps taken", ko: "밟은 스텝" },
    RecapLoss { en: "Loss, first to last", ko: "손실, 처음에서 끝까지" },
    RecapTime { en: "Time on this machine", ko: "이 컴퓨터에서 걸린 시간" },
    RecapSpeed { en: "Characters a second", ko: "초당 글자 수" },
    RecapAnswer { en: "Asked nmtk, it answers", ko: "nmtk라고 물으면" },
    RecapBroken { en: "When you broke it", ko: "망가뜨렸을 때" },
    RecapBrokenAnswer { en: "and it answered", ko: "그때의 답" },
    // ---- The keys this quest adds to the bottom bar ----------------------------
    KeyChangeValue { en: "change the value", ko: "값 바꾸기" },
    KeyViewHead { en: "layer and head", ko: "층과 헤드" },
    KeyRecover { en: "put it back", ko: "되돌려 놓기" },
    // ---- What went wrong -------------------------------------------------------
    ErrorTitle { en: "These settings do not describe a model", ko: "이 설정으로는 모델이 만들어지지 않습니다" },
    ErrorZeroSize { en: "Every size here has to be at least one.", ko: "여기 있는 크기는 전부 적어도 1이어야 합니다." },
    ErrorHeadsDoNotDivideWidth { en: "The width has to divide evenly by the number of heads, because the heads share it out between them.", ko: "너비는 헤드 수로 나누어떨어져야 합니다. 헤드들이 너비를 나눠 갖기 때문입니다." },
    ErrorContextTooLong { en: "The window is longer than the whole text the model reads.", ko: "창이 모델이 읽는 글 전체보다 깁니다." },
    ErrorLearningRateNotPositive { en: "The learning rate has to be a number above zero.", ko: "학습률은 0보다 큰 수여야 합니다." },
    ErrorWorkerThread { en: "This machine would not give the run a thread of its own.", ko: "이 컴퓨터가 실행에 쓸 스레드를 내주지 않았습니다." },
}

/// Why a set of settings does not describe a model that can be built.
pub fn config_error(error: ConfigError) -> Msg {
    match error {
        ConfigError::ZeroSize => Msg::ErrorZeroSize,
        ConfigError::HeadsDoNotDivideWidth { .. } => Msg::ErrorHeadsDoNotDivideWidth,
        ConfigError::ContextLongerThanCorpus { .. } => Msg::ErrorContextTooLong,
        ConfigError::LearningRateNotPositive => Msg::ErrorLearningRateNotPositive,
    }
}

/// Why a run could not start.
pub fn start_error(error: StartError) -> Msg {
    match error {
        StartError::Config(inner) => config_error(inner),
        StartError::WorkerThread => Msg::ErrorWorkerThread,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nmtk_core::Language;

    #[test]
    fn english_is_never_missing() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::ENGLISH).is_empty(), "{msg:?} has no English");
        }
    }

    /// A line with no Korean falls back to its English, which still reads. A blank does not.
    #[test]
    fn korean_is_never_blank() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::KOREAN).trim().is_empty(), "{msg:?} is blank in Korean");
        }
    }

    #[test]
    fn every_reason_a_run_can_be_refused_has_words() {
        let errors = [
            ConfigError::ZeroSize,
            ConfigError::HeadsDoNotDivideWidth { d_model: 33, heads: 2 },
            ConfigError::ContextLongerThanCorpus { context: 9_000, corpus: 2_500 },
            ConfigError::LearningRateNotPositive,
        ];
        for error in errors {
            assert!(!config_error(error).text(Language::ENGLISH).is_empty(), "{error:?}");
        }
        assert_eq!(start_error(StartError::WorkerThread), Msg::ErrorWorkerThread);
    }
}
