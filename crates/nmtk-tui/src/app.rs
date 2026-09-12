//! What the program is showing and what the keys do.
//!
//! The engines are not here. This type holds only what a reader can see and change, so the whole
//! interface can be reasoned about — and tested — without mining a single block.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nmtk_core::{Language, MachineProfile, Settings};
use nmtk_i18n::Msg;

/// A subject a reader can open from the home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Subject {
    ProofOfWork,
    Ledgers,
    Transformer,
    ZeroKnowledge,
}

impl Subject {
    pub const ALL: [Subject; 4] =
        [Subject::ProofOfWork, Subject::Ledgers, Subject::Transformer, Subject::ZeroKnowledge];

    pub fn title(self) -> Msg {
        match self {
            Subject::ProofOfWork => Msg::MenuProofOfWork,
            Subject::Ledgers => Msg::MenuLedgers,
            Subject::Transformer => Msg::MenuTransformer,
            Subject::ZeroKnowledge => Msg::MenuZeroKnowledge,
        }
    }

    pub fn about(self) -> Msg {
        match self {
            Subject::ProofOfWork => Msg::MenuProofOfWorkAbout,
            Subject::Ledgers => Msg::MenuLedgersAbout,
            Subject::Transformer => Msg::MenuTransformerAbout,
            Subject::ZeroKnowledge => Msg::MenuZeroKnowledgeAbout,
        }
    }
}

/// A row on the home screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HomeItem {
    Subject(Subject),
    Settings,
    Quit,
}

impl HomeItem {
    pub fn all() -> Vec<HomeItem> {
        Subject::ALL
            .iter()
            .map(|s| HomeItem::Subject(*s))
            .chain([HomeItem::Settings, HomeItem::Quit])
            .collect()
    }

    pub fn title(self) -> Msg {
        match self {
            HomeItem::Subject(s) => s.title(),
            HomeItem::Settings => Msg::MenuSettings,
            HomeItem::Quit => Msg::MenuQuit,
        }
    }

    pub fn about(self) -> Option<Msg> {
        match self {
            HomeItem::Subject(s) => Some(s.about()),
            HomeItem::Settings => Some(Msg::MenuSettingsAbout),
            HomeItem::Quit => None,
        }
    }
}

/// A row on the settings screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingItem {
    Language,
    Threads,
    Colour,
}

impl SettingItem {
    pub const ALL: [SettingItem; 3] =
        [SettingItem::Language, SettingItem::Threads, SettingItem::Colour];

    pub fn title(self) -> Msg {
        match self {
            SettingItem::Language => Msg::SettingsLanguage,
            SettingItem::Threads => Msg::SettingsThreads,
            SettingItem::Colour => Msg::SettingsColour,
        }
    }

    pub fn about(self) -> Msg {
        match self {
            SettingItem::Language => Msg::SettingsLanguageAbout,
            SettingItem::Threads => Msg::SettingsThreadsAbout,
            SettingItem::Colour => Msg::SettingsColourAbout,
        }
    }
}

/// Which screen is in front.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Screen {
    Home,
    Subject(Subject),
    Settings,
    Help,
}

/// The whole interface state.
pub struct App {
    pub settings: Settings,
    pub machine: MachineProfile,
    pub screen: Screen,
    /// Where `?` was pressed, so closing help goes back rather than home.
    behind_help: Option<Screen>,
    pub home_index: usize,
    pub settings_index: usize,
    /// A line shown briefly under the settings, e.g. that they were saved.
    pub status: Option<Msg>,
    pub quit: bool,
}

impl App {
    pub fn new() -> Self {
        Self {
            settings: Settings::load(),
            machine: MachineProfile::detect(),
            screen: Screen::Home,
            behind_help: None,
            home_index: 0,
            settings_index: 0,
            status: None,
            quit: false,
        }
    }

    pub fn language(&self) -> Language {
        self.settings.language
    }

    pub fn threads(&self) -> usize {
        self.settings.resolved_worker_threads(&self.machine)
    }

    /// Handles one key. Key releases and repeats are ignored: a held key should not run a subject
    /// twice.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.quit = true;
            return;
        }
        self.status = None;
        match self.screen {
            Screen::Help => self.on_key_help(key.code),
            Screen::Home => self.on_key_home(key.code),
            Screen::Settings => self.on_key_settings(key.code),
            Screen::Subject(_) => self.on_key_subject(key.code),
        }
    }

    fn on_key_help(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.screen = self.behind_help.take().unwrap_or(Screen::Home);
            }
            _ => {}
        }
    }

    fn on_key_home(&mut self, code: KeyCode) {
        let items = HomeItem::all();
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.home_index = previous(self.home_index, items.len())
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.home_index = next(self.home_index, items.len())
            }
            KeyCode::Enter | KeyCode::Char(' ') => match items[self.home_index.min(items.len() - 1)]
            {
                HomeItem::Subject(subject) => self.screen = Screen::Subject(subject),
                HomeItem::Settings => self.screen = Screen::Settings,
                HomeItem::Quit => self.quit = true,
            },
            KeyCode::Char('s') => self.screen = Screen::Settings,
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn on_key_settings(&mut self, code: KeyCode) {
        let count = SettingItem::ALL.len();
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.settings_index = previous(self.settings_index, count)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.settings_index = next(self.settings_index, count)
            }
            KeyCode::Left | KeyCode::Char('h') => self.adjust_setting(-1),
            KeyCode::Right | KeyCode::Char('L') => self.adjust_setting(1),
            KeyCode::Enter => self.adjust_setting(1),
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('q') | KeyCode::Esc => self.screen = Screen::Home,
            _ => {}
        }
    }

    fn on_key_subject(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('s') => self.screen = Screen::Settings,
            KeyCode::Char('q') | KeyCode::Esc => self.screen = Screen::Home,
            _ => {}
        }
    }

    fn open_help(&mut self) {
        self.behind_help = Some(self.screen);
        self.screen = Screen::Help;
    }

    /// The `l` key: swap language and remember it. A machine that cannot write settings still
    /// changes language for this run.
    pub fn toggle_language(&mut self) {
        self.settings.language = self.settings.language.toggled();
        self.remember();
    }

    fn adjust_setting(&mut self, step: i32) {
        match SettingItem::ALL[self.settings_index.min(SettingItem::ALL.len() - 1)] {
            SettingItem::Language => self.settings.language = self.settings.language.toggled(),
            SettingItem::Threads => {
                let max = self.machine.logical_cores as i32;
                let current = self.settings.worker_threads as i32;
                self.settings.worker_threads = (current + step).clamp(0, max) as usize;
            }
            SettingItem::Colour => self.settings.colour = !self.settings.colour,
        }
        self.remember();
    }

    fn remember(&mut self) {
        self.status =
            Some(if self.settings.save().is_ok() { Msg::SettingsSaved } else { Msg::SettingsNotSaved });
    }
}

impl Default for App {
    fn default() -> Self {
        Self::new()
    }
}

fn next(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + 1) % len }
}

fn previous(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + len - 1) % len }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    fn app() -> App {
        App {
            settings: Settings::default(),
            machine: MachineProfile {
                logical_cores: 8,
                total_memory_bytes: 0,
                available_memory_bytes: 0,
            },
            screen: Screen::Home,
            behind_help: None,
            home_index: 0,
            settings_index: 0,
            status: None,
            quit: false,
        }
    }

    #[test]
    fn moving_up_from_the_top_wraps_to_the_bottom() {
        let mut app = app();
        press(&mut app, KeyCode::Up);
        assert_eq!(app.home_index, HomeItem::all().len() - 1);
    }

    #[test]
    fn the_last_home_row_quits() {
        let mut app = app();
        app.home_index = HomeItem::all().len() - 1;
        press(&mut app, KeyCode::Enter);
        assert!(app.quit);
    }

    #[test]
    fn help_returns_to_the_screen_it_was_opened_from() {
        let mut app = app();
        app.screen = Screen::Settings;
        press(&mut app, KeyCode::Char('?'));
        assert_eq!(app.screen, Screen::Help);
        press(&mut app, KeyCode::Esc);
        assert_eq!(app.screen, Screen::Settings);
    }

    #[test]
    fn quitting_from_a_subject_goes_home_rather_than_out() {
        let mut app = app();
        app.screen = Screen::Subject(Subject::ProofOfWork);
        press(&mut app, KeyCode::Char('q'));
        assert_eq!(app.screen, Screen::Home);
        assert!(!app.quit);
    }

    #[test]
    fn the_language_key_works_on_every_screen() {
        for screen in [Screen::Home, Screen::Settings, Screen::Subject(Subject::Ledgers)] {
            let mut app = app();
            app.screen = screen;
            press(&mut app, KeyCode::Char('l'));
            assert_eq!(app.settings.language, Language::Korean, "{screen:?} ignored the key");
        }
    }

    #[test]
    fn threads_cannot_be_set_above_the_machine_or_below_auto() {
        let mut app = app();
        app.screen = Screen::Settings;
        app.settings_index = 1;
        for _ in 0..20 {
            press(&mut app, KeyCode::Right);
        }
        assert_eq!(app.settings.worker_threads, 8);
        for _ in 0..20 {
            press(&mut app, KeyCode::Left);
        }
        assert_eq!(app.settings.worker_threads, 0);
    }

    #[test]
    fn auto_threads_leave_one_core_for_the_screen() {
        let app = app();
        assert_eq!(app.threads(), 7);
    }

    #[test]
    fn a_held_key_does_not_repeat_an_action() {
        let mut app = app();
        app.on_key(KeyEvent::new_with_kind(
            KeyCode::Down,
            KeyModifiers::NONE,
            KeyEventKind::Release,
        ));
        assert_eq!(app.home_index, 0);
    }
}
