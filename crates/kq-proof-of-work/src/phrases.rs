//! Every word this quest says, English first.
//!
//! The engine hands back numbers and enums — `AttackPhase::Racing`, `ConfigError::NoMiners`, a
//! hash rate, a count of reverted blocks — and this table is where they become something a reader
//! understands. Keeping the two apart is what lets a second language arrive without the mining
//! code noticing. Korean is written later; anything missing falls back to the English here.

use nmtk_pow::{AttackOutcome, AttackPhase, ConfigError, TargetError};

nmtk_i18n::messages! {
    // ---- The quest itself ------------------------------------------------------
    Title: "Proof of work",
    Summary: "Mine real blocks on this machine, then rewrite the chain with a 51% attack.",
    Subcategory: "Proof of work",

    // ---- Brief -----------------------------------------------------------------
    BriefProblem: "Thousands of computers that have never met, and have no reason to trust each other, have to agree on one list of who paid whom. Any of them may lie.",
    BriefCost: "Proof of work settles it with electricity. To add a block — one page of that list — you have to find a number that makes the page's fingerprint come out below a target. There is no clever way to find it. You guess, you check, you guess again.",
    BriefNonce: "The fingerprint is a hash: SHA-256, the same function twice, run over the 80 bytes of the block header. The number you keep changing is called the nonce, and it is just a counter sitting in that header.",
    BriefRule: "Every computer then follows one rule: the chain with the most work behind it is the real one. Rewriting history means out-guessing everybody else for as long as it takes, and guessing costs power, and power costs money.",
    BriefPromise: "Next your machine does exactly this, for real, at a practice difficulty — the target is set high enough that blocks arrive in seconds instead of years.",

    PuzzleTitle: "The puzzle",
    PuzzleHeader: "header of 80 bytes, with a nonce in it",
    PuzzleHash: "hash it twice with SHA-256",
    PuzzleCompare: "is the result below the target?",
    PuzzleNo: "no   change the nonce and hash again",
    PuzzleYes: "yes  it is a block",
    PuzzleCost: "What one block costs, on average:",
    LabelPracticeHere: "Practice here",
    LabelDifficultyOne: "Bitcoin, Jan 2009",
    LabelHarderBy: "Harder by",
    UnitHashes: "hashes",

    // ---- Run -------------------------------------------------------------------
    RunTitle: "Mining",
    RunExplainReal: "Your cores are hashing real block headers and comparing them against a real target, the same way a Bitcoin node does. The hash rate below is hashes counted and divided by the seconds they took, not an estimate.",
    RunExplainDifficulty: "A difficulty is how many hashes a block costs on average. Bitcoin's difficulty 1, where it started in January 2009, costs about 4.3 billion. The practice difficulty here costs far fewer, which is why blocks arrive while you watch.",
    RunExplainTail: "The waiting time is an average, not a countdown. Every hash is an independent try with the same tiny chance, so a machine that has been grinding for an hour is exactly as far from its next block as one that just started.",
    RunExplainDerived: "The 2009 figure beside your rate is worked out from the protocol's own numbers — difficulty 1, and a ten-minute gap between blocks — and is not a measurement of anyone's computer.",
    RunIdle: "Press Enter to start mining.",

    StatusIdle: "not started",
    StatusMining: "mining",
    StatusPaused: "paused",
    StatusDone: "finished",

    LabelHashRate: "Hash rate",
    LabelHashes: "Hashes",
    LabelBlocksFound: "Blocks found",
    LabelChainHeight: "Chain height",
    LabelHashesPerBlock: "Hashes per block",
    LabelStale: "Lost a race",

    HeadingComparison: "At Bitcoin's difficulty 1",
    LabelOneBlockTakes: "One block takes",
    LabelHalfTakeUnder: "Half take under",
    LabelNetwork2009: "Network, early 2009",
    LabelYouAre: "You are",
    ComparisonDerived: "worked out from the protocol, not measured",

    HeadingRecentBlocks: "Newest blocks",
    Unavailable: "—",

    // ---- Tune ------------------------------------------------------------------
    TuneTitle: "Your numbers",
    TuneExplainHint: "Difficulty, miners, their shares and the thread count are yours. Left and right change a value; type digits and press Enter for your own number. Press Enter again to run with them.",
    TuneExplainDifficulty: "Each extra bit of practice difficulty doubles what a block costs. Add a miner and the same machine is divided between more of them, so each one finds fewer blocks.",
    TuneExplainThreads: "Threads are whole things. A machine with seven of them cannot give one miner 51% and another 49%, so the share a miner really holds is often not the share you asked for. Both numbers are shown.",
    TuneExplainOversubscribe: "Asking for more threads than the machine has cores brings the two numbers together — the operating system splits its time between them — at a small cost in total speed.",
    TuneShares: "Shares do not have to add up to 100. Three miners asking for 1, 1 and 2 get a quarter, a quarter and a half.",

    KnobDifficulty: "Practice difficulty",
    KnobMiners: "Miners",
    KnobThreads: "Threads",
    KnobShareOne: "Miner 1 share",
    KnobShareTwo: "Miner 2 share",
    KnobShareThree: "Miner 3 share",
    KnobShareFour: "Miner 4 share",
    UnitZeroBits: "zero bits",

    MinerOne: "Miner 1",
    MinerTwo: "Miner 2",
    MinerThree: "Miner 3",
    MinerFour: "Miner 4",

    HeadingSplit: "Threads and shares",
    ColumnMiner: "Miner",
    ColumnThreads: "threads",
    ColumnAsked: "asked",
    ColumnGot: "got",
    TuneMismatch: "asked and got differ: threads are whole things",
    TuneRestart: "Press Enter to run with these numbers.",

    // ---- Break -----------------------------------------------------------------
    BreakTitle: "The 51% attack",
    BreakExplainStory: "Someone pays a merchant and waits. The payment goes into a block, more blocks land on top of it, and at the number of confirmations the merchant trusts, the goods are handed over.",
    BreakExplainPrivate: "Meanwhile the buyer has been mining a chain of their own in private, starting from the block before the payment. In that chain the same coin was paid to the buyer instead.",
    BreakExplainPublish: "When the private chain carries more work than the public one, the buyer publishes it. Every node switches, because that is the rule every node follows, and the payment the merchant watched confirm is simply not there any more.",
    BreakExplainArithmetic: "Whether this works is arithmetic, not cleverness. Above half the hash power the attacker catches up eventually. Below it the attacker falls behind, and has to keep paying for hash power while falling. Try 30% and watch it fail.",
    BreakExplainSettings: "The practice difficulty and the thread count come from Tune. If a run keeps giving up on time rather than on work, lower the difficulty there.",
    BreakHint: "Press Enter to pay and start the attack.",

    KnobAttackerShare: "Attacker's share",
    KnobConfirmations: "Confirmations",

    PhaseWarmup: "mining in the open",
    PhaseAwaitingPayment: "the payment is waiting for a block",
    PhaseConfirming: "counting confirmations",
    PhaseRacing: "the private chain is racing",
    PhaseFinished: "over",

    LabelPublicChain: "Public chain",
    LabelPrivateChain: "Private chain",
    LabelAheadBy: "Attacker ahead by",
    LabelBehindBy: "Attacker behind by",
    LabelFurthestBehind: "Furthest behind",
    LabelMerchantSaw: "The merchant saw",
    LabelGoods: "Goods",
    LabelBlocksErased: "Blocks erased",
    LabelAttackTook: "The attack took",
    LabelThreadSplit: "Threads",
    LabelDifficulty: "Difficulty",
    UnitConfirmation: "confirmation",
    UnitConfirmations: "confirmations",
    UnitBlock: "block",
    UnitBlocks: "blocks",
    VictimReleased: "handed over",
    VictimWaiting: "not yet",
    ThreadsAttackerHonest: "attacker / everyone else",

    OutcomeSucceeded: "the payment is gone from the chain",
    OutcomeGaveUp: "the attacker gave up and the chain held",
    OutcomeShortSucceeded: "payment erased",
    OutcomeShortGaveUp: "the chain held",
    BreakLessonFailed: "Below half the power, this usually happens.",

    // ---- Recap -----------------------------------------------------------------
    RecapTitle: "What just happened",
    RecapExplainOrder: "In the order it happened.",
    RecapExplainMining: "You hashed headers until one came out below a target. That is the whole of mining, and every number on the Run screen was counted here rather than estimated.",
    RecapExplainTune: "You changed the difficulty, the miners, their shares and the threads, and the run answered with different blocks.",
    RecapExplainBreak: "You bought a share of the hash power and tried to erase a payment somebody had already been paid for. Above half it works and below half it does not, and both endings are the lesson.",
    RecapDifficulty: "Practice difficulty",
    RecapHashRate: "Your hash rate",
    RecapBlocks: "Blocks you mined",
    RecapAtDifficultyOne: "At difficulty 1",
    RecapNetwork: "Network, early 2009",
    RecapSplit: "Miner 1 asked / got",
    RecapAttack: "Attack",
    NotYet: "not run yet",

    // ---- Keys ------------------------------------------------------------------
    KeyTypeNumber: "type a number",
    KeyTurn: "turn the value",

    // ---- When a setting is refused ---------------------------------------------
    ErrorNoMiners: "There has to be at least one miner.",
    ErrorDuplicateMiner: "Two miners were given the same name.",
    ErrorInvalidShare: "That share is not a number a miner can hold.",
    ErrorTooFewThreads: "Fewer threads than miners. Raise the threads.",
    ErrorFixedThreads: "Miners asked for more threads than there are.",
    ErrorNegativeTarget: "That difficulty cannot go in a block header.",
    ErrorZeroTarget: "That difficulty leaves no hash that could win.",
    ErrorTargetOverflow: "That difficulty does not fit in a header.",
    ErrorTooManyZeroBits: "A hash cannot have that many zero bits.",
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
