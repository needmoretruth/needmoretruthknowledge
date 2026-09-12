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
    Title: "Transformer",
    Summary: "Train a real transformer on this machine until four letters become a sentence.",
    Subcategory: "Architectures",

    // ---- Brief -----------------------------------------------------------------
    BriefOpening: "A language model only ever answers one question: given the characters so far, which character comes next? Everything else is machinery for answering it well.",
    BriefEmbedding: "Each character starts as a short list of numbers called an embedding. Nothing in that list means anything yet — the numbers are random, and training is the process that gives them meaning.",
    BriefPosition: "A second list says where in the text the character sits, because nmtk and kmtn are the same four characters in a different order and have to end up in different places.",
    BriefAttention: "Attention is every position looking back at the positions before it and deciding which of them matter for what comes next. A position can never see the future, only the past. That is the whole trick.",
    BriefWeights: "Every number you have just read about — the embeddings, the attention projections, the layers stacked on top — is a weight, and this machine is about to change every one of them a few thousand times, on your own cores, with no network involved.",
    BriefPromise: "You will watch the loss fall and watch the four letters nmtk turn into need more truth knowledge. Nothing is decided in advance: change the settings badly enough and it will not happen.",
    BriefShapeTitle: "What this machine will build",
    LabelVocabulary: "Characters",
    LabelLayers: "Layers",
    LabelHeads: "Heads",
    LabelWidth: "Width",
    LabelContext: "Window",
    LabelWeights: "Weights",

    // ---- Run -------------------------------------------------------------------
    RunTitle: "Training",
    RunIdle: "Press Enter to start training on this machine.",
    RunExplainStart: "Training starts when you press Enter and takes tens of seconds. You can leave this stage and come back — the run carries on without you, and Space pauses it between steps.",
    RunExplainLoss: "Loss is measured in nats per character: how surprised the model was by the character that actually came next. Guessing blindly over this alphabet scores about 3.3. A model that has learned the text is well under 0.5.",
    RunExplainAnswer: "Under the numbers is the model's own best guess at what follows nmtk, asked again about once a second with the weights exactly as they stand. Early on it is noise. Then it is a word. Then it is the sentence.",
    RunExplainAttention: "The grid at the bottom is the attention of the most recent forward pass: one row per position, one column per position it could look back at. A dark cell means that row drew heavily on that column.",
    LabelStep: "Step",
    LabelLoss: "Loss",
    LabelSpeed: "Speed",
    LabelElapsed: "Elapsed",
    LabelRate: "Rate",
    UnitCharsPerSecond: "chars/s",
    LabelAnswer: "Answer",
    LabelCurve: "Loss over the run",
    AnswerWaiting: "the first answer is about a second away",
    AnswerRight: "that is the sentence",
    AnswerNotYet: "not the sentence yet",
    AnswerBlank: "it answers with nothing but spaces",
    StatusRunning: "training",
    StatusPaused: "paused",
    StatusFinished: "finished",
    NotANumber: "not a number",

    // ---- Attention -------------------------------------------------------------
    AttentionTitle: "Where the model looked",
    AttentionWaiting: "No forward pass has been captured yet.",
    LabelLayer: "layer",
    LabelHead: "head",
    KnobView: "Showing",
    AttentionLegend: "each row is a position, each column one it looked back at",

    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Your settings",
    TuneExplainShape: "Layers, heads and width decide how big the model is. Weights grow roughly with the square of the width, and every extra weight is more arithmetic per step, so a bigger model is not automatically a better one in the minutes you have.",
    TuneExplainRate: "The learning rate is how far every weight moves on each step. Too small and the run never arrives; too large and it steps straight over the answer. Steps is how many times that happens.",
    TuneExplainDivide: "The width is shared out between the heads, so it has to divide evenly by them. The count below goes to nothing when the settings do not describe a model, and says why.",
    TuneHint: "Left and right change the chosen value. Type digits and press Enter for your own number; Esc throws the typing away.",
    TuneGo: "Enter throws away any run and trains a new model with these settings.",
    KnobLayers: "Layers",
    KnobHeads: "Heads",
    KnobWidth: "Model width",
    KnobLearningRate: "Learning rate",
    KnobSteps: "Steps",
    LabelPresets: "worth trying",
    LabelWeightsNow: "Weights now",
    LabelMachineChose: "This machine chose",

    // ---- Break -----------------------------------------------------------------
    BreakTitle: "Break it",
    BreakExplainRate: "The learning rate is the easiest thing here to get wrong and the failure is spectacular. Every step moves each weight that far in the direction that would have helped; make the step large enough and it flies past the answer by more than it was wrong to begin with, then does it again the other way. At three thousand times the sensible rate the loss never once gets below where it started, and the answer collapses into the same letter over and over.",
    BreakExplainWarmup: "The second attack removes the warmup. A transformer's first steps are its most dangerous — attention is spread almost evenly and the gradients are large — so the sensible schedule ramps the rate up from nothing over the first hundred steps. Set the multiplier to 10 and turn the warmup off: at the sensible rate the ramp barely matters, but at ten times it is the difference between the sentence and a misspelling of it, and ten times the loss.",
    BreakExplainSgd: "The third is not sabotage but a fair fight. Plain SGD moves every weight by the same rate and remembers nothing; AdamW gives each weight its own step size from its own recent history. At the sensible rate SGD barely moves. Turn the multiplier up to 100 and it finds the sentence again — which is the point: a rate is only sensible for the rule that is using it.",
    BreakHint: "Press Enter to train with these settings and watch it fail.",
    BreakRecover: "Press r to put the settings back to the sensible ones, then Enter to train an honest model again.",
    LabelAttack: "Attack",
    LabelMultiplier: "Rate multiplier",
    AttackRunaway: "a runaway learning rate",
    AttackNoWarmup: "no warmup at all",
    AttackPlainSgd: "plain SGD, no memory",
    LabelSensibleRate: "Sensible rate",
    LabelThisRun: "This run",
    LabelHonestRun: "Honest run",
    LabelBrokenRun: "Broken run",
    BreakClimbing: "the loss is climbing",
    BreakBlewUp: "the loss stopped being a number at all",
    BreakRecovered: "The settings are sensible again. Press Enter to train.",

    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "What just happened",
    RecapNothing: "Nothing has run yet. Press 2, then Enter.",
    RecapExplainOne: "You started with a few tens of thousands of random numbers that meant nothing, and a piece of text they had never seen.",
    RecapExplainTwo: "Every step drew a batch of windows from that text, ran them forward through the embeddings and the attention layers, measured how surprised the model was by the character that actually came next, and pushed every weight a small distance in the direction that would have made it less surprised.",
    RecapExplainThree: "A few thousand of those steps is the entire difference between noise and a sentence. There is no other ingredient, and everything that happened, happened on this machine.",
    RecapWeights: "Weights trained",
    RecapSteps: "Steps taken",
    RecapLoss: "Loss, first to last",
    RecapTime: "Time on this machine",
    RecapSpeed: "Characters a second",
    RecapAnswer: "Asked nmtk, it answers",
    RecapBroken: "When you broke it",
    RecapBrokenAnswer: "and it answered",

    // ---- The keys this quest adds to the bottom bar ----------------------------
    KeyChangeValue: "change the value",
    KeyViewHead: "layer and head",
    KeyRecover: "put it back",

    // ---- What went wrong -------------------------------------------------------
    ErrorTitle: "These settings do not describe a model",
    ErrorZeroSize: "Every size here has to be at least one.",
    ErrorHeadsDoNotDivideWidth: "The width has to divide evenly by the number of heads, because the heads share it out between them.",
    ErrorContextTooLong: "The window is longer than the whole text the model reads.",
    ErrorLearningRateNotPositive: "The learning rate has to be a number above zero.",
    ErrorWorkerThread: "This machine would not give the run a thread of its own.",
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

    /// Korean has not been written for this quest yet. Until it is, every line has to fall back
    /// to its English rather than to a blank, or a reader who presses `l` gets an empty screen.
    #[test]
    fn korean_falls_back_to_english_rather_than_blank() {
        for msg in Msg::ALL {
            assert_eq!(msg.text(Language::Korean), msg.text(Language::English), "{msg:?}");
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
