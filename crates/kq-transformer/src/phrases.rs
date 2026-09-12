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
    Title: "Transformer" => "트랜스포머",
    Summary: "Train a real transformer on this machine until four letters become a sentence." => "이 컴퓨터에서 진짜 트랜스포머를 학습시켜, 네 글자가 한 문장이 되게 만듭니다.",
    Subcategory: "Architectures" => "구조",
    // ---- Brief -----------------------------------------------------------------
    BriefOpening: "A language model only ever answers one question: given the characters so far, which character comes next? Everything else is machinery for answering it well." => "언어 모델이 하는 질문은 하나뿐입니다. 지금까지의 글자들을 봤을 때, 다음 글자는 무엇인가? 나머지는 전부 그 질문에 잘 답하기 위한 장치입니다.",
    BriefEmbedding: "Each character starts as a short list of numbers called an embedding. Nothing in that list means anything yet — the numbers are random, and training is the process that gives them meaning." => "글자 하나하나는 임베딩이라고 부르는 짧은 숫자 목록으로 시작합니다. 그 목록은 아직 아무 뜻도 없습니다 — 숫자가 무작위이고, 학습이란 그 숫자에 뜻을 만들어 주는 과정입니다.",
    BriefPosition: "A second list says where in the text the character sits, because nmtk and kmtn are the same four characters in a different order and have to end up in different places." => "두 번째 목록은 그 글자가 글의 몇 번째에 있는지를 말합니다. nmtk와 kmtn은 같은 네 글자를 순서만 바꾼 것이라, 서로 다른 곳에 놓여야 하기 때문입니다.",
    BriefAttention: "Attention is every position looking back at the positions before it and deciding which of them matter for what comes next. A position can never see the future, only the past. That is the whole trick." => "어텐션이란 각 자리가 자기 앞의 자리들을 돌아보면서, 다음에 올 것을 정하는 데 어느 자리가 중요한지 고르는 일입니다. 어떤 자리도 미래를 볼 수 없고 과거만 봅니다. 그것이 요령의 전부입니다.",
    BriefWeights: "Every number you have just read about — the embeddings, the attention projections, the layers stacked on top — is a weight, and this machine is about to change every one of them a few thousand times, on your own cores, with no network involved." => "방금 읽은 모든 숫자 — 임베딩, 어텐션의 사영, 그 위에 쌓은 층들 — 이 전부 가중치이고, 이 컴퓨터가 곧 그 하나하나를 몇천 번씩 바꿉니다. 내 코어에서, 네트워크 없이.",
    BriefPromise: "You will watch the loss fall and watch the four letters nmtk turn into need more truth knowledge. Nothing is decided in advance: change the settings badly enough and it will not happen." => "손실이 내려가는 것과, nmtk 네 글자가 need more truth knowledge가 되는 것을 보게 됩니다. 미리 정해진 결과는 없습니다. 설정을 충분히 나쁘게 바꾸면 그렇게 되지 않습니다.",
    BriefShapeTitle: "What this machine will build" => "이 컴퓨터가 만들 모델",
    LabelVocabulary: "Characters" => "글자 수",
    LabelLayers: "Layers" => "층",
    LabelHeads: "Heads" => "헤드",
    LabelWidth: "Width" => "너비",
    LabelContext: "Window" => "창",
    LabelWeights: "Weights" => "가중치",
    // ---- Run -------------------------------------------------------------------
    RunTitle: "Training" => "학습",
    RunIdle: "Press Enter to start training on this machine." => "Enter를 누르면 이 컴퓨터에서 학습이 시작됩니다.",
    RunExplainStart: "Training starts when you press Enter and takes tens of seconds. You can leave this stage and come back — the run carries on without you, and Space pauses it between steps." => "Enter를 누르면 학습이 시작되고 수십 초가 걸립니다. 이 단계를 떠났다 돌아와도 됩니다 — 학습은 혼자 계속 돌고, Space로 스텝 사이에서 멈출 수 있습니다.",
    RunExplainLoss: "Loss is measured in nats per character: how surprised the model was by the character that actually came next. Guessing blindly over this alphabet scores about 3.3. A model that has learned the text is well under 0.5." => "손실은 글자당 nat 단위입니다. 실제로 다음에 온 글자를 보고 모델이 얼마나 놀랐는가입니다. 이 글자 집합에서 아무렇게나 찍으면 약 3.3이 나옵니다. 글을 익힌 모델은 0.5보다 한참 아래입니다.",
    RunExplainAnswer: "Under the numbers is the model's own best guess at what follows nmtk, asked again about once a second with the weights exactly as they stand. Early on it is noise. Then it is a word. Then it is the sentence." => "숫자 아래는 지금 이 순간의 가중치 그대로, nmtk 다음에 무엇이 오는지 모델이 내놓은 답입니다. 1초에 한 번쯤 다시 물어봅니다. 처음에는 잡음이고, 그다음에는 단어가 되고, 그다음에는 문장이 됩니다.",
    RunExplainAttention: "The grid at the bottom is the attention of the most recent forward pass: one row per position, one column per position it could look back at. A dark cell means that row drew heavily on that column." => "아래 격자는 가장 최근 순전파의 어텐션입니다. 가로 한 줄이 한 자리이고, 세로 한 칸이 그 자리가 돌아본 자리입니다. 칸이 진할수록 그 줄이 그 칸에 많이 기댔다는 뜻입니다.",
    LabelStep: "Step" => "스텝",
    LabelLoss: "Loss" => "손실",
    LabelSpeed: "Speed" => "속도",
    LabelElapsed: "Elapsed" => "걸린 시간",
    LabelRate: "Rate" => "학습률",
    UnitCharsPerSecond: "chars/s" => "자/초",
    LabelAnswer: "Answer" => "답",
    LabelCurve: "Loss over the run" => "학습 동안의 손실",
    AnswerWaiting: "the first answer is about a second away" => "첫 답까지 1초쯤 남았습니다",
    AnswerRight: "that is the sentence" => "이것이 그 문장입니다",
    AnswerNotYet: "not the sentence yet" => "아직 그 문장이 아닙니다",
    AnswerBlank: "it answers with nothing but spaces" => "공백만 내놓고 있습니다",
    StatusRunning: "training" => "학습 중",
    StatusPaused: "paused" => "일시정지",
    StatusFinished: "finished" => "끝남",
    NotANumber: "not a number" => "숫자가 아님",
    // ---- Attention -------------------------------------------------------------
    AttentionTitle: "Where the model looked" => "모델이 본 곳",
    AttentionWaiting: "No forward pass has been captured yet." => "아직 잡아 둔 순전파가 없습니다.",
    LabelLayer: "layer" => "층",
    LabelHead: "head" => "헤드",
    KnobView: "Showing" => "보는 중",
    AttentionLegend: "each row is a position, each column one it looked back at" => "가로 한 줄이 한 자리, 세로 한 칸이 그 자리가 돌아본 자리",
    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Your settings" => "내 설정",
    TuneExplainShape: "Layers, heads and width decide how big the model is. Weights grow roughly with the square of the width, and every extra weight is more arithmetic per step, so a bigger model is not automatically a better one in the minutes you have." => "층·헤드·너비가 모델의 크기를 정합니다. 가중치는 너비의 제곱에 가깝게 늘고, 가중치 하나가 늘 때마다 스텝마다 할 계산이 늘어납니다. 그래서 몇 분 안에 끝내야 하는 지금은 큰 모델이 자동으로 더 좋은 모델은 아닙니다.",
    TuneExplainRate: "The learning rate is how far every weight moves on each step. Too small and the run never arrives; too large and it steps straight over the answer. Steps is how many times that happens." => "학습률은 스텝마다 가중치 하나가 얼마나 움직이는가입니다. 너무 작으면 영영 도착하지 못하고, 너무 크면 답을 지나쳐 버립니다. 스텝 수는 그 일을 몇 번 하는가입니다.",
    TuneExplainDivide: "The width is shared out between the heads, so it has to divide evenly by them. The count below goes to nothing when the settings do not describe a model, and says why." => "너비는 헤드들이 나눠 갖기 때문에 헤드 수로 나누어떨어져야 합니다. 설정이 모델을 이루지 못하면 아래 가중치 수가 사라지고 이유가 나옵니다.",
    TuneHint: "Left and right change the chosen value. Type digits and press Enter for your own number; Esc throws the typing away." => "왼쪽·오른쪽으로 고른 값을 바꿉니다. 직접 숫자를 입력하고 Enter를 눌러도 되고, Esc로 입력을 버립니다.",
    TuneGo: "Enter throws away any run and trains a new model with these settings." => "Enter를 누르면 지금 돌던 것을 버리고 이 설정으로 새 모델을 학습시킵니다.",
    KnobLayers: "Layers" => "층",
    KnobHeads: "Heads" => "헤드",
    KnobWidth: "Model width" => "모델 너비",
    KnobLearningRate: "Learning rate" => "학습률",
    KnobSteps: "Steps" => "스텝 수",
    LabelPresets: "worth trying" => "해 볼 만한 값",
    LabelWeightsNow: "Weights now" => "지금 가중치",
    LabelMachineChose: "This machine chose" => "이 컴퓨터가 고른 값",
    // ---- Break -----------------------------------------------------------------
    BreakTitle: "Break it" => "무너뜨리기",
    BreakExplainRate: "The learning rate is the easiest thing here to get wrong and the failure is spectacular. Every step moves each weight that far in the direction that would have helped; make the step large enough and it flies past the answer by more than it was wrong to begin with, then does it again the other way. At three thousand times the sensible rate the loss never once gets below where it started, and the answer collapses into the same letter over and over." => "여기서 가장 틀리기 쉬운 것이 학습률이고, 틀렸을 때의 모습이 볼만합니다. 스텝마다 모든 가중치가 도움이 됐을 방향으로 그만큼 움직이는데, 그 폭이 충분히 크면 원래 틀렸던 것보다 더 많이 답을 지나쳐 버리고, 다음에는 반대쪽으로 또 그럽니다. 적당한 학습률의 3000배로 놓으면 손실이 처음 값 아래로 한 번도 안 내려가고, 답은 같은 글자만 되풀이하며 무너집니다.",
    BreakExplainWarmup: "The second attack removes the warmup. A transformer's first steps are its most dangerous — attention is spread almost evenly and the gradients are large — so the sensible schedule ramps the rate up from nothing over the first hundred steps. Set the multiplier to 10 and turn the warmup off: at the sensible rate the ramp barely matters, but at ten times it is the difference between the sentence and a misspelling of it, and ten times the loss." => "두 번째 공격은 워밍업을 없앱니다. 트랜스포머에게 가장 위험한 때는 첫 몇 스텝입니다 — 어텐션이 거의 고르게 퍼져 있고 기울기가 큽니다 — 그래서 제대로 된 일정은 처음 100스텝에 걸쳐 학습률을 0에서부터 올립니다. 배수를 10으로 놓고 워밍업을 끄면, 적당한 학습률에서는 그 경사가 별 차이를 안 내지만 10배에서는 문장과 오타 사이의 차이, 그리고 손실 10배의 차이가 됩니다.",
    BreakExplainSgd: "The third is not sabotage but a fair fight. Plain SGD moves every weight by the same rate and remembers nothing; AdamW gives each weight its own step size from its own recent history. At the sensible rate SGD barely moves. Turn the multiplier up to 100 and it finds the sentence again — which is the point: a rate is only sensible for the rule that is using it." => "세 번째는 방해가 아니라 정정당당한 대결입니다. 순수 SGD는 모든 가중치를 같은 학습률로 움직이고 아무것도 기억하지 않습니다. AdamW는 가중치마다 자기 최근 이력에서 뽑은 자기만의 보폭을 줍니다. 적당한 학습률에서 SGD는 거의 움직이지 않습니다. 배수를 100까지 올리면 다시 그 문장을 찾아냅니다 — 그것이 요점입니다. 학습률이 적당한가는 그것을 쓰는 규칙에 달려 있습니다.",
    BreakHint: "Press Enter to train with these settings and watch it fail." => "Enter를 누르면 이 설정으로 학습시키고 실패하는 것을 봅니다.",
    BreakRecover: "Press r to put the settings back to the sensible ones, then Enter to train an honest model again." => "r을 누르면 설정이 제대로 된 값으로 돌아가고, 그다음 Enter로 정상 모델을 다시 학습시킵니다.",
    LabelAttack: "Attack" => "공격",
    LabelMultiplier: "Rate multiplier" => "학습률 배수",
    AttackRunaway: "a runaway learning rate" => "고삐 풀린 학습률",
    AttackNoWarmup: "no warmup at all" => "워밍업 아예 없음",
    AttackPlainSgd: "plain SGD, no memory" => "기억 없는 순수 SGD",
    LabelSensibleRate: "Sensible rate" => "적당한 학습률",
    LabelThisRun: "This run" => "이번 실행",
    LabelHonestRun: "Honest run" => "정상 실행",
    LabelBrokenRun: "Broken run" => "망가뜨린 실행",
    BreakClimbing: "the loss is climbing" => "손실이 올라가고 있습니다",
    BreakBlewUp: "the loss stopped being a number at all" => "손실이 아예 숫자가 아니게 됐습니다",
    BreakRecovered: "The settings are sensible again. Press Enter to train." => "설정이 제대로 돌아왔습니다. Enter를 눌러 학습시키세요.",
    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "What just happened" => "방금 본 것",
    RecapNothing: "Nothing has run yet. Press 2, then Enter." => "아직 아무것도 돌리지 않았습니다. 2를 누른 다음 Enter를 누르세요.",
    RecapExplainOne: "You started with a few tens of thousands of random numbers that meant nothing, and a piece of text they had never seen." => "아무 뜻도 없는 무작위 숫자 수만 개와, 그 숫자들이 한 번도 본 적 없는 글 한 편에서 시작했습니다.",
    RecapExplainTwo: "Every step drew a batch of windows from that text, ran them forward through the embeddings and the attention layers, measured how surprised the model was by the character that actually came next, and pushed every weight a small distance in the direction that would have made it less surprised." => "스텝마다 그 글에서 창 여러 개를 뽑아 임베딩과 어텐션 층을 통과시키고, 실제로 다음에 온 글자에 모델이 얼마나 놀랐는지 재고, 덜 놀랐을 방향으로 모든 가중치를 조금씩 밀었습니다.",
    RecapExplainThree: "A few thousand of those steps is the entire difference between noise and a sentence. There is no other ingredient, and everything that happened, happened on this machine." => "그 스텝 몇천 번이 잡음과 문장 사이의 차이 전부입니다. 다른 재료는 없고, 벌어진 일은 전부 이 컴퓨터에서 벌어졌습니다.",
    RecapWeights: "Weights trained" => "학습시킨 가중치",
    RecapSteps: "Steps taken" => "밟은 스텝",
    RecapLoss: "Loss, first to last" => "손실, 처음에서 끝까지",
    RecapTime: "Time on this machine" => "이 컴퓨터에서 걸린 시간",
    RecapSpeed: "Characters a second" => "초당 글자 수",
    RecapAnswer: "Asked nmtk, it answers" => "nmtk라고 물으면",
    RecapBroken: "When you broke it" => "망가뜨렸을 때",
    RecapBrokenAnswer: "and it answered" => "그때의 답",
    // ---- The keys this quest adds to the bottom bar ----------------------------
    KeyChangeValue: "change the value" => "값 바꾸기",
    KeyViewHead: "layer and head" => "층과 헤드",
    KeyRecover: "put it back" => "되돌려 놓기",
    // ---- What went wrong -------------------------------------------------------
    ErrorTitle: "These settings do not describe a model" => "이 설정으로는 모델이 만들어지지 않습니다",
    ErrorZeroSize: "Every size here has to be at least one." => "여기 있는 크기는 전부 적어도 1이어야 합니다.",
    ErrorHeadsDoNotDivideWidth: "The width has to divide evenly by the number of heads, because the heads share it out between them." => "너비는 헤드 수로 나누어떨어져야 합니다. 헤드들이 너비를 나눠 갖기 때문입니다.",
    ErrorContextTooLong: "The window is longer than the whole text the model reads." => "창이 모델이 읽는 글 전체보다 깁니다.",
    ErrorLearningRateNotPositive: "The learning rate has to be a number above zero." => "학습률은 0보다 큰 수여야 합니다.",
    ErrorWorkerThread: "This machine would not give the run a thread of its own." => "이 컴퓨터가 실행에 쓸 스레드를 내주지 않았습니다.",
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
            assert!(!msg.text(Language::English).is_empty(), "{msg:?} has no English");
        }
    }

    /// A line with no Korean falls back to its English, which still reads. A blank does not.
    #[test]
    fn korean_is_never_blank() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::Korean).trim().is_empty(), "{msg:?} is blank in Korean");
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
            assert!(!config_error(error).text(Language::English).is_empty(), "{error:?}");
        }
        assert_eq!(start_error(StartError::WorkerThread), Msg::ErrorWorkerThread);
    }
}
