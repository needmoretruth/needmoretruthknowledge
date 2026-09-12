//! Every word the reader sees, in one table.
//!
//! English is the source of truth: it is written first and it is never missing. Korean is a
//! choice the reader makes with one key, and any line that has no Korean yet falls back to the
//! English rather than to a blank — a half-translated screen still reads.
//!
//! Engines never reach into this table. They return numbers and enums; the screen decides how to
//! say them. That is why a new language costs one column here and nothing anywhere else.

use nmtk_core::Language;

/// Declares the message table. Korean is optional per line.
macro_rules! messages {
    ($( $(#[$doc:meta])* $key:ident : $en:literal $( => $ko:literal )? ),* $(,)?) => {
        /// One line of text on screen, named by what it says rather than where it appears.
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
        pub enum Msg { $( $(#[$doc])* $key, )* }

        impl Msg {
            /// The line in the chosen language, falling back to English.
            pub fn text(self, language: Language) -> &'static str {
                match (self, language) {
                    $( (Msg::$key, Language::English) => $en, )*
                    $( $( (Msg::$key, Language::Korean) => $ko, )? )*
                    #[allow(unreachable_patterns)]
                    (other, _) => other.text(Language::English),
                }
            }

            /// Every message, so a test can walk the whole table.
            pub const ALL: &'static [Msg] = &[ $( Msg::$key, )* ];
        }
    };
}

messages! {
    // ---- Shell ----------------------------------------------------------------
    AppSubtitle: "Run it, break it, learn it." => "직접 돌리고, 부숴 보고, 배웁니다.",
    Motto: "If it isn't fun, it doesn't stick." => "재미없으면 배워지지가 않는다.",
    TerminalTooSmall: "This screen needs 80x24. Make the terminal larger." => "이 화면은 80x24가 필요합니다. 터미널을 키워 주세요.",
    Loading: "Working..." => "작업 중...",

    // ---- Home -----------------------------------------------------------------
    Home: "Home" => "홈",
    HomeHint: "Pick a subject and press Enter." => "주제를 고르고 Enter를 누르세요.",
    MenuProofOfWork: "Proof of work" => "작업증명",
    MenuProofOfWorkAbout: "Mine at Bitcoin's original difficulty, split your cores among miners, and run a 51% attack that actually succeeds." => "비트코인 첫 난이도로 채굴하고, 코어를 채굴자들에게 나눠 주고, 실제로 성공하는 51% 공격을 돌려 봅니다.",
    MenuLedgers: "Ledger models" => "원장 방식",
    MenuLedgersAbout: "Send the same coin under UTXO, account and object rules, then try to spend it twice." => "같은 코인을 UTXO·계정·객체 규칙으로 보내 보고, 두 번 쓰기를 시도해 봅니다.",
    MenuTransformer: "Transformer" => "트랜스포머",
    MenuTransformerAbout: "Train a real transformer here, by hand, until it answers \"nmtk\" with \"need more truth knowledge\"." => "직접 짠 트랜스포머를 여기서 학습시켜 「nmtk」에 「need more truth knowledge」라고 답하게 만듭니다.",
    MenuZeroKnowledge: "Zero-knowledge proofs" => "영지식 증명",
    MenuZeroKnowledgeAbout: "Prove you know a secret without showing it, from sigma protocols to halo2, seen from four sides." => "비밀을 보여 주지 않고 안다는 것만 증명합니다. 시그마 프로토콜부터 halo2까지, 네 사람의 눈으로 봅니다.",
    MenuSettings: "Settings" => "설정",
    MenuSettingsAbout: "Language, threads, colour, and the defaults each subject starts from." => "언어, 스레드 수, 색, 그리고 각 주제가 시작하는 기본값.",
    MenuQuit: "Quit" => "끝내기",

    // ---- Keys -----------------------------------------------------------------
    KeyMove: "move" => "이동",
    KeyOpen: "open" => "열기",
    KeyRun: "run" => "실행",
    KeyPause: "pause" => "일시정지",
    KeyReset: "reset" => "초기화",
    KeyPanel: "panel" => "패널",
    KeyLanguage: "language" => "언어",
    KeyHelp: "help" => "도움말",
    KeyBack: "back" => "뒤로",
    KeyQuit: "quit" => "끝내기",
    KeySettings: "settings" => "설정",

    // ---- Settings screen ------------------------------------------------------
    SettingsLanguage: "Language" => "언어",
    SettingsLanguageAbout: "English is the default. Korean is a choice, and anything not translated yet stays English." => "영어가 기본입니다. 한국어는 선택이고, 아직 번역되지 않은 줄은 영어로 남습니다.",
    SettingsThreads: "Worker threads" => "작업 스레드",
    SettingsThreadsAbout: "How many cores long jobs may take. One core is left free so the screen keeps moving." => "긴 작업이 쓸 코어 수입니다. 화면이 계속 움직이도록 한 코어는 비워 둡니다.",
    SettingsColour: "Colour" => "색",
    SettingsColourAbout: "Turn this off for a screen that reads on a monochrome terminal." => "끄면 흑백 터미널에서도 읽히는 화면이 됩니다.",
    SettingsAuto: "auto" => "자동",
    SettingsSaved: "Saved." => "저장했습니다.",
    SettingsNotSaved: "Could not save settings; this run keeps the change." => "설정을 저장하지 못했습니다. 이번 실행에서는 바뀐 값이 유지됩니다.",

    // ---- This machine ---------------------------------------------------------
    MachineTitle: "This machine" => "이 컴퓨터",
    MachineCores: "Cores" => "코어",
    MachineMemory: "Memory" => "메모리",
    MachineUsing: "Using" => "사용 중",

    // ---- Help -----------------------------------------------------------------
    HelpTitle: "Keys" => "키",
    HelpOffline: "nmtk never touches the network. Everything here runs on this machine." => "nmtk는 네트워크를 전혀 쓰지 않습니다. 여기 있는 것은 전부 이 컴퓨터에서 돕니다.",
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
            assert!(!msg.text(Language::English).is_empty(), "{msg:?} has no English");
        }
    }

    #[test]
    fn korean_falls_back_to_english_rather_than_blank() {
        for msg in Msg::ALL {
            assert!(!msg.text(Language::Korean).is_empty(), "{msg:?} is blank in Korean");
        }
    }

    #[test]
    fn the_motto_is_the_owners_sentence() {
        assert_eq!(Msg::Motto.text(Language::English), "If it isn't fun, it doesn't stick.");
    }
}
