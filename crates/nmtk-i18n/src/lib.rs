//! Every word the reader sees, in one table.
//!
//! English is the source of truth: it is written first and it is never missing. Korean is a
//! choice the reader makes with one key, and any line that has no Korean yet falls back to the
//! English rather than to a blank — a half-translated screen still reads.
//!
//! Engines never reach into this table. They return numbers and enums; the screen decides how to
//! say them. That is why a new language costs one column here and nothing anywhere else.

pub use nmtk_core::Language;

/// Declares a message table, one column per language.
///
/// Every KQ calls this for its own phrases, so two quests being written at the same time never
/// touch the same file. `en` is required and is what a missing column falls back to; every other
/// column is keyed by the language's code, and adding a language is adding a column.
///
/// ```ignore
/// nmtk_i18n::messages! {
///     Title { en: "Proof of work", ko: "작업증명" },
///     Summary { en: "Mine a real block." },
/// }
/// ```
#[macro_export]
macro_rules! messages {
    ($( $(#[$doc:meta])* $key:ident { en: $en:literal $(, $lang:ident : $text:literal )* $(,)? } ),* $(,)?) => {
        /// One line of text on screen, named by what it says rather than where it appears.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Msg { $( $(#[$doc])* $key, )* }

        impl Msg {
            /// The line in the chosen language, falling back to English.
            pub fn text(self, language: $crate::Language) -> &'static str {
                let code = language.code();
                match self {
                    $(
                        Msg::$key => {
                            $( if code == stringify!($lang) { return $text; } )*
                            $en
                        }
                    )*
                }
            }

            /// Every message, so a test can walk the whole table.
            pub const ALL: &'static [Msg] = &[ $( Msg::$key, )* ];
        }
    };
}

messages! {
    // ---- Shell ----------------------------------------------------------------
    AppSubtitle { en: "Run it, break it, learn it.", ko: "직접 돌리고, 부숴 보고, 배웁니다." },
    Motto { en: "Study that isn't fun is labour. I hate labour.", ko: "재미없는 공부는 노동이다. 나는 노동을 싫어한다." },
    TerminalTooSmall { en: "This screen needs 80x24. Make the terminal larger.", ko: "이 화면은 80x24가 필요합니다. 터미널을 키워 주세요." },
    Loading { en: "Working...", ko: "작업 중..." },
    // ---- Home -----------------------------------------------------------------
    Home { en: "Home", ko: "홈" },
    HomeHint { en: "Pick a subject and press Enter.", ko: "주제를 고르고 Enter를 누르세요." },
    MenuProofOfWork { en: "Proof of work", ko: "작업증명" },
    MenuProofOfWorkAbout { en: "Mine at Bitcoin's original difficulty, split your cores among miners, and run a 51% attack that actually succeeds.", ko: "비트코인 첫 난이도로 채굴하고, 코어를 채굴자들에게 나눠 주고, 실제로 성공하는 51% 공격을 돌려 봅니다." },
    MenuLedgers { en: "Ledger models", ko: "원장 방식" },
    MenuLedgersAbout { en: "Send the same coin under UTXO, account and object rules, then try to spend it twice.", ko: "같은 코인을 UTXO·계정·객체 규칙으로 보내 보고, 두 번 쓰기를 시도해 봅니다." },
    MenuTransformer { en: "Transformer", ko: "트랜스포머" },
    MenuTransformerAbout { en: "Train a real transformer here, by hand, until it answers \"nmtk\" with \"need more truth knowledge\".", ko: "직접 짠 트랜스포머를 여기서 학습시켜 「nmtk」에 「need more truth knowledge」라고 답하게 만듭니다." },
    MenuZeroKnowledge { en: "Zero-knowledge proofs", ko: "영지식 증명" },
    MenuZeroKnowledgeAbout { en: "Prove you know a secret without showing it, from sigma protocols to halo2, seen from four sides.", ko: "비밀을 보여 주지 않고 안다는 것만 증명합니다. 시그마 프로토콜부터 halo2까지, 네 사람의 눈으로 봅니다." },
    MenuSettings { en: "Settings", ko: "설정" },

    // ---- The first launch, and the only time nmtk asks anything ------------------
    WelcomeTitle { en: "Welcome", ko: "환영합니다" },
    WelcomeOne { en: "nmtk is a program for learning by running the real thing on your own machine.", ko: "nmtk는 진짜를 내 컴퓨터에서 직접 돌려 보며 배우는 프로그램입니다." },
    WelcomeTwo { en: "Every quest talks you through one subject, one sentence at a time, and the numbers you see were measured here.", ko: "각 퀘스트가 한 주제를 한 문장씩 이야기해 주고, 화면의 숫자는 전부 여기서 실제로 잰 값입니다." },
    WelcomeThree { en: "Nothing goes to the network. Ever.", ko: "네트워크로 나가는 것은 아무것도 없습니다. 한 번도요." },
    WelcomeFour { en: "A few things to set first. You can change any of them later with s.", ko: "먼저 몇 가지만 정합니다. 나중에 s를 눌러 언제든 바꿀 수 있습니다." },
    WelcomeStart { en: "start", ko: "시작" },
    WelcomeSaved { en: "Saved. Everything else is on the shelf.", ko: "저장했습니다. 나머지는 선반에 있습니다." },
    MenuSettingsAbout { en: "Language, threads, colour, and the defaults each subject starts from.", ko: "언어, 스레드 수, 색, 그리고 각 주제가 시작하는 기본값." },
    MenuQuit { en: "Quit", ko: "끝내기" },
    // ---- Quest list -----------------------------------------------------------
    Quests { en: "Quests", ko: "퀘스트" },
    QuestsEmpty { en: "No quests here yet.", ko: "아직 여기에 퀘스트가 없습니다." },
    CategoryConsensus { en: "Consensus", ko: "합의" },
    CategoryLedgers { en: "Ledgers", ko: "원장" },
    CategoryCryptography { en: "Cryptography", ko: "암호" },
    CategoryMachineLearning { en: "Machine learning", ko: "기계학습" },
    CategoryNetworking { en: "Networking", ko: "네트워크" },
    CategorySystems { en: "Systems", ko: "시스템" },
    DifficultyVeryEasy { en: "Very easy", ko: "매우 쉬움" },
    DifficultyEasy { en: "Easy", ko: "쉬움" },
    DifficultyMedium { en: "Medium", ko: "중간" },
    DifficultyHard { en: "Hard", ko: "어려움" },
    DifficultyVeryHard { en: "Very hard", ko: "매우 어려움" },
    SortByCategory { en: "by category", ko: "분류 순" },
    SortByTitle { en: "by name", ko: "이름 순" },
    SortByDifficulty { en: "by difficulty", ko: "난이도 순" },
    SortByNewest { en: "newest first", ko: "최신 순" },
    SortByShortest { en: "shortest first", ko: "짧은 순" },
    FilterAll { en: "all", ko: "전체" },
    LabelSort { en: "sort", ko: "정렬" },
    LabelFilter { en: "filter", ko: "분류" },
    LabelVersion { en: "version", ko: "판" },
    LabelUpdated { en: "Updated", ko: "갱신" },
    LabelReleased { en: "Released", ko: "처음 나온 날" },
    LabelLength { en: "About", ko: "예상 시간" },
    LabelNeeds { en: "Needs", ko: "필요" },
    LabelMinutes { en: "min", ko: "분" },
    LabelCores { en: "cores", ko: "코어" },
    LabelCore { en: "core", ko: "코어" },
    NeedsAny { en: "runs on any machine", ko: "어떤 컴퓨터에서나 돕니다" },
    LabelMinimum { en: "Minimum", ko: "최소" },
    LabelRecommended { en: "Recommended", ko: "권장" },
    FitRecommended { en: "this machine is above the recommended bar", ko: "이 컴퓨터는 권장 사양을 넘습니다" },
    FitMinimum { en: "this machine clears the minimum; some of it will be slow", ko: "이 컴퓨터는 최소 사양을 넘습니다. 일부는 느리게 돕니다" },
    FitBelow { en: "below the minimum — it still opens and sizes itself down", ko: "최소 사양에 못 미칩니다. 그래도 열리고 크기를 줄여 돕니다" },
    LabelMemory { en: "memory", ko: "메모리" },
    LabelStages { en: "stages", ko: "단계" },
    OlderVersions { en: "Every version of this quest", ko: "이 퀘스트의 모든 판" },
    OnlyVersion { en: "This quest has only ever had one version.", ko: "이 퀘스트는 아직 판이 하나뿐입니다." },
    RoleExplain { en: "Why", ko: "왜" },
    RoleRun { en: "Run", ko: "실행" },
    RoleTune { en: "Tune", ko: "조절" },
    RoleBreak { en: "Break", ko: "무너뜨리기" },
    RoleRecap { en: "Recap", ko: "정리" },
    KeyContinue { en: "continue", ko: "계속" },
    KeyScroll { en: "scroll", ko: "스크롤" },
    ConversationWaiting { en: "Press Enter to carry on.", ko: "Enter를 눌러 계속합니다." },
    ConversationWorking { en: "Working. The next line arrives when the machine does.", ko: "돌아가는 중입니다. 기계가 답하면 다음 줄이 나옵니다." },
    ConversationFinished { en: "That is the end of this quest. Press q for the list, or Shift+Tab to walk back through it.", ko: "이 퀘스트는 여기까지입니다. q를 누르면 목록으로 돌아가고, Shift+Tab으로 앞 단계를 다시 볼 수 있습니다." },
    // ---- Keys -----------------------------------------------------------------
    KeyMove { en: "move", ko: "이동" },
    KeyOpen { en: "open", ko: "열기" },
    KeyRun { en: "run", ko: "실행" },
    KeyPause { en: "pause", ko: "일시정지" },
    KeyReset { en: "reset", ko: "초기화" },
    KeyPanel { en: "panel", ko: "패널" },
    KeyLanguage { en: "language", ko: "언어" },
    KeyHelp { en: "help", ko: "도움말" },
    KeyBack { en: "back", ko: "뒤로" },
    KeyQuit { en: "quit", ko: "끝내기" },
    KeySettings { en: "settings", ko: "설정" },
    KeyStage { en: "stage", ko: "단계" },
    KeyType { en: "or type the number", ko: "숫자로 직접 치기" },
    KeyChoose { en: "pick a value", ko: "값 고르기" },
    KeyChange { en: "change it", ko: "값 바꾸기" },
    // ---- Settings screen ------------------------------------------------------
    SettingsLanguage { en: "Language", ko: "언어" },
    SettingsLanguageAbout { en: "English is the default. Korean is a choice, and anything not translated yet stays English.", ko: "영어가 기본입니다. 한국어는 선택이고, 아직 번역되지 않은 줄은 영어로 남습니다." },
    SettingsThreads { en: "Worker threads", ko: "작업 스레드" },
    SettingsThreadsAbout { en: "How many cores long jobs may take. One core is left free so the screen keeps moving.", ko: "긴 작업이 쓸 코어 수입니다. 화면이 계속 움직이도록 한 코어는 비워 둡니다." },
    SettingsColour { en: "Colour", ko: "색" },
    SettingsColourAbout { en: "Turn this off for a screen that reads on a monochrome terminal.", ko: "끄면 흑백 터미널에서도 읽히는 화면이 됩니다." },
    SettingsAuto { en: "auto", ko: "자동" },
    SettingsOn { en: "on", ko: "켬" },
    SettingsOff { en: "off", ko: "끔" },
    SettingsSaved { en: "Saved.", ko: "저장했습니다." },
    SettingsNotSaved { en: "Could not save settings; this run keeps the change.", ko: "설정을 저장하지 못했습니다. 이번 실행에서는 바뀐 값이 유지됩니다." },
    // ---- This machine ---------------------------------------------------------
    MachineTitle { en: "This machine", ko: "이 컴퓨터" },
    MachineCores { en: "Cores", ko: "코어" },
    MachineMemory { en: "Memory", ko: "메모리" },
    MachineUsing { en: "Using", ko: "사용 중" },
    // ---- Help -----------------------------------------------------------------
    HelpTitle { en: "Keys", ko: "키" },
    HelpEverywhere { en: "Everywhere", ko: "어디서나" },
    HelpOnTheShelf { en: "On the quest list", ko: "퀘스트 목록에서" },
    HelpInAQuest { en: "Inside a quest", ko: "퀘스트 안에서" },
    HelpOffline { en: "nmtk never touches the network. Everything here runs on this machine.", ko: "nmtk는 네트워크를 전혀 쓰지 않습니다. 여기 있는 것은 전부 이 컴퓨터에서 돕니다." },
}

/// A convenience for screens: `t(Msg::MenuQuit, language)`.
pub fn t(message: Msg, language: Language) -> &'static str {
    message.text(language)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_never_missing() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::ENGLISH).is_empty(), "{msg:?} has no English");
        }
    }

    #[test]
    fn korean_falls_back_to_english_rather_than_blank() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::KOREAN).is_empty(), "{msg:?} is blank in Korean");
        }
    }

    #[test]
    fn the_motto_is_the_owners_sentence() {
        assert_eq!(
            Msg::Motto.text(Language::ENGLISH),
            "Study that isn't fun is labour. I hate labour."
        );
    }
}
