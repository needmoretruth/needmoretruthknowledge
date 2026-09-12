//! What the program is showing and what the keys do.
//!
//! The quests are not here. This type holds the shelf, the settings, and whichever quest is open,
//! so the whole interface can be reasoned about — and tested — without mining a single block.

use crossterm::event::{KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use nmtk_core::{Language, MachineProfile, Settings};
use nmtk_i18n::Msg;
use nmtk_kq::meta::{KqId, StageSpec, Version};
use nmtk_kq::session::{Action, Kq, KqSession, Reaction};
use nmtk_kq::{Catalogue, Filter, SortKey};

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
    /// The shelf of quests.
    Quests,
    /// A quest a reader has opened.
    Quest,
    Settings,
    /// Pick a language from the list. Never a toggle: this list is expected to get long.
    Languages,
    Help,
}

/// The quest a reader is inside.
pub struct OpenQuest {
    pub id: KqId,
    pub version: Version,
    pub title: &'static str,
    pub stages: &'static [StageSpec],
    pub session: Box<dyn KqSession>,
}

/// The whole interface state.
pub struct App {
    pub settings: Settings,
    pub machine: MachineProfile,
    pub catalogue: Catalogue,
    pub screen: Screen,
    /// Where `?` was pressed, so closing help goes back rather than to the shelf.
    behind_help: Option<Screen>,
    pub list_index: usize,
    pub sort: SortKey,
    pub filter: Filter,
    /// Set while the reader is looking at the older versions of one quest.
    pub versions_of: Option<KqId>,
    pub open: Option<OpenQuest>,
    pub settings_index: usize,
    /// Which row the language list is on while it is open.
    pub language_index: usize,
    /// Lines hidden above the conversation. Zero means the oldest beat is at the top.
    pub transcript_scroll: usize,
    /// How far the conversation could be scrolled, learned from the last draw.
    pub transcript_furthest: usize,
    /// Whether the conversation follows the newest beat. True until the reader scrolls up.
    pub transcript_follows: bool,
    /// Where the language list was opened from, so choosing goes back there.
    behind_languages: Option<Screen>,
    pub status: Option<Msg>,
    pub quit: bool,
}

impl App {
    pub fn new(catalogue: Catalogue) -> Self {
        Self {
            settings: Settings::load(),
            machine: MachineProfile::detect(),
            catalogue,
            screen: Screen::Quests,
            behind_help: None,
            list_index: 0,
            sort: SortKey::Category,
            filter: Filter::default(),
            versions_of: None,
            open: None,
            settings_index: 0,
            language_index: 0,
            transcript_scroll: 0,
            transcript_furthest: 0,
            transcript_follows: true,
            behind_languages: None,
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

    /// The rows the shelf is showing: either the newest of each quest, or every version of one.
    pub fn visible(&self) -> Vec<&dyn Kq> {
        match self.versions_of {
            Some(id) => self.catalogue.versions_of(id),
            None => self.catalogue.list(&self.filter, self.sort, self.language()),
        }
    }

    /// Handles one key. Releases and repeats are ignored: a held key must not open a quest twice.
    pub fn on_key(&mut self, key: KeyEvent) {
        if key.kind != KeyEventKind::Press {
            return;
        }
        if key.modifiers.contains(KeyModifiers::CONTROL) && matches!(key.code, KeyCode::Char('c')) {
            self.close_quest();
            self.quit = true;
            return;
        }
        self.status = None;
        match self.screen {
            Screen::Help => self.on_key_help(key.code),
            Screen::Languages => self.on_key_languages(key.code),
            Screen::Quests => self.on_key_quests(key.code),
            Screen::Quest => self.on_key_quest(key.code),
            Screen::Settings => self.on_key_settings(key.code),
        }
    }

    fn on_key_help(&mut self, code: KeyCode) {
        if matches!(code, KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter) {
            self.screen = self.behind_help.take().unwrap_or(Screen::Quests);
        }
    }

    /// The language list: move, choose, or leave it as it was.
    fn on_key_languages(&mut self, code: KeyCode) {
        let count = Language::ALL.len();
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.language_index = previous(self.language_index, count)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.language_index = next(self.language_index, count)
            }
            KeyCode::Enter => {
                self.settings.language = Language::ALL[self.language_index.min(count - 1)];
                self.remember();
                self.close_languages();
            }
            KeyCode::Char('q') | KeyCode::Esc => self.close_languages(),
            _ => {}
        }
    }

    fn open_languages(&mut self) {
        self.behind_languages = Some(self.screen);
        self.language_index = self.settings.language.index();
        self.screen = Screen::Languages;
    }

    fn close_languages(&mut self) {
        self.screen = self.behind_languages.take().unwrap_or(Screen::Quests);
    }

    fn on_key_quests(&mut self, code: KeyCode) {
        let count = self.visible().len();
        match code {
            KeyCode::Up | KeyCode::Char('k') => self.list_index = previous(self.list_index, count),
            KeyCode::Down | KeyCode::Char('j') => self.list_index = next(self.list_index, count),
            KeyCode::Enter => self.open_chosen(),
            KeyCode::Char('o') => self.cycle_sort(),
            KeyCode::Char('f') => self.cycle_filter(),
            KeyCode::Char('v') => self.toggle_versions(),
            KeyCode::Char('s') => self.screen = Screen::Settings,
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Esc if self.versions_of.is_some() => self.toggle_versions(),
            KeyCode::Char('q') | KeyCode::Esc => self.quit = true,
            _ => {}
        }
    }

    fn on_key_quest(&mut self, code: KeyCode) {
        // The shell's own keys come first, except while a number is being typed.
        let typing = self.open.as_ref().is_some_and(|quest| quest.session.typing());
        if !typing {
            match code {
                KeyCode::Char('q') | KeyCode::Esc => {
                    self.close_quest();
                    self.screen = Screen::Quests;
                    return;
                }
                KeyCode::Char('l') => {
                    self.toggle_language();
                    return;
                }
                KeyCode::Char('s') => {
                    self.screen = Screen::Settings;
                    return;
                }
                KeyCode::Char('?') => {
                    self.open_help();
                    return;
                }
                _ => {}
            }
        }
        // Scrolling the conversation belongs to the shell: every quest has one.
        match code {
            KeyCode::PageUp => {
                self.scroll_conversation(-8);
                return;
            }
            KeyCode::PageDown => {
                self.scroll_conversation(8);
                return;
            }
            _ => {}
        }
        // Tab walks the stages. It has to be a key no knob wants, because a quest with values to
        // type cannot also spend the digits on jumping about.
        match code {
            KeyCode::Tab => {
                self.step_stage(1);
                return;
            }
            KeyCode::BackTab => {
                self.step_stage(-1);
                return;
            }
            _ => {}
        }
        let tunable = self.open.as_ref().is_some_and(|quest| !quest.session.knobs().is_empty());
        let Some(action) = action_for(code, typing, tunable, self.stages()) else { return };
        if let Some(quest) = &mut self.open
            && quest.session.on(action) == Reaction::Handled
        {
            // A beat the reader caused is a beat they want to see.
            self.transcript_follows = true;
        }
    }

    /// Moves one stage along, stopping at both ends rather than wrapping — a reader who holds Tab
    /// at the last stage should not find themselves back at the first.
    fn step_stage(&mut self, step: i32) {
        let count = self.stages().len();
        let Some(quest) = &mut self.open else { return };
        let at = quest.session.stage() as i32 + step;
        if at < 0 || at as usize >= count {
            return;
        }
        quest.session.on(Action::Stage(at as usize));
        self.transcript_scroll = 0;
        self.transcript_follows = true;
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
            KeyCode::Right | KeyCode::Enter => self.adjust_setting(1),
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('q') | KeyCode::Esc => {
                self.screen = if self.open.is_some() { Screen::Quest } else { Screen::Quests }
            }
            _ => {}
        }
    }

    /// The stages the open quest has, or nothing when none is open.
    fn stages(&self) -> &'static [StageSpec] {
        self.open.as_ref().map(|quest| quest.stages).unwrap_or(&[])
    }

    /// The quest definition behind the open session, for its stage names.
    pub fn open_definition(&self) -> Option<&dyn Kq> {
        let open = self.open.as_ref()?;
        self.catalogue
            .versions_of(open.id)
            .into_iter()
            .find(|quest| quest.meta().version == open.version)
    }

    /// Called after each draw so scrolling cannot run past the end of a conversation that changed.
    pub fn conversation_drawn(&mut self, furthest: usize) {
        self.transcript_furthest = furthest;
        if self.transcript_follows {
            self.transcript_scroll = furthest;
        } else {
            self.transcript_scroll = self.transcript_scroll.min(furthest);
        }
    }

    fn scroll_conversation(&mut self, lines: i32) {
        let at = self.transcript_scroll as i32 + lines;
        let at = at.clamp(0, self.transcript_furthest as i32) as usize;
        self.transcript_scroll = at;
        self.transcript_follows = at >= self.transcript_furthest;
    }

    fn open_chosen(&mut self) {
        let chosen = {
            let list = self.visible();
            list.get(self.list_index).map(|quest| {
                let meta = quest.meta();
                (meta.id, meta.version, meta.stages)
            })
        };
        let Some((id, version, stages)) = chosen else { return };
        let opened = {
            let versions = self.catalogue.versions_of(id);
            versions
                .iter()
                .find(|quest| quest.meta().version == version)
                .map(|quest| (quest.title(self.settings.language), quest.open(&self.machine)))
        };
        if let Some((title, session)) = opened {
            self.close_quest();
            self.open = Some(OpenQuest { id, version, title, stages, session });
            self.transcript_scroll = 0;
            self.transcript_furthest = 0;
            self.transcript_follows = true;
            self.screen = Screen::Quest;
        }
    }

    /// Stops the open quest's threads. Always called before one is dropped.
    fn close_quest(&mut self) {
        if let Some(quest) = &mut self.open {
            quest.session.close();
        }
        self.open = None;
    }

    fn cycle_sort(&mut self) {
        let current = SortKey::ALL.iter().position(|key| *key == self.sort).unwrap_or(0);
        self.sort = SortKey::ALL[(current + 1) % SortKey::ALL.len()];
        self.list_index = 0;
    }

    /// Steps through the categories that actually hold quests, then back to all of them.
    fn cycle_filter(&mut self) {
        let categories = self.catalogue.categories();
        self.filter.category = match self.filter.category {
            None => categories.first().copied(),
            Some(current) => {
                let at = categories.iter().position(|c| *c == current);
                match at {
                    Some(index) if index + 1 < categories.len() => Some(categories[index + 1]),
                    _ => None,
                }
            }
        };
        self.list_index = 0;
    }

    fn toggle_versions(&mut self) {
        self.versions_of = match self.versions_of {
            Some(_) => None,
            None => {
                let list = self.visible();
                list.get(self.list_index).map(|quest| quest.meta().id)
            }
        };
        self.list_index = 0;
    }

    fn open_help(&mut self) {
        self.behind_help = Some(self.screen);
        self.screen = Screen::Help;
    }

    /// The `l` key opens the list. Choosing is a separate keypress, because a program that will
    /// one day speak twenty languages cannot cycle through them one at a time.
    pub fn toggle_language(&mut self) {
        self.open_languages();
    }

    /// Steps to the next or previous language, for the settings row.
    fn step_language(&mut self, step: i32) {
        let count = Language::ALL.len();
        let at = self.settings.language.index();
        let next_index =
            if step > 0 { (at + 1) % count } else { (at + count - 1) % count };
        self.settings.language = Language::ALL[next_index];
        self.remember();
    }

    fn adjust_setting(&mut self, step: i32) {
        match SettingItem::ALL[self.settings_index.min(SettingItem::ALL.len() - 1)] {
            SettingItem::Language => self.step_language(step),
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
        self.status = Some(if self.settings.save().is_ok() {
            Msg::SettingsSaved
        } else {
            Msg::SettingsNotSaved
        });
    }
}

/// The gesture a key means inside a quest.
///
/// `tunable` says whether the stage showing has values the reader can change. When it does, the
/// digits belong to those values — a quest that promises "type a number" and then swallows every
/// digit as a stage jump has promised nothing. Tab moves between stages instead, and where there
/// is nothing to type the digits go back to reaching stages directly.
fn action_for(
    code: KeyCode,
    typing: bool,
    tunable: bool,
    stages: &[StageSpec],
) -> Option<Action> {
    match code {
        KeyCode::Up | KeyCode::Char('k') if !typing => Some(Action::Previous),
        KeyCode::Down | KeyCode::Char('j') if !typing => Some(Action::Next),
        KeyCode::Left | KeyCode::Char('h') if !typing => Some(Action::Nudge(-1)),
        KeyCode::Right if !typing => Some(Action::Nudge(1)),
        KeyCode::Enter => Some(if typing { Action::Commit } else { Action::Go }),
        KeyCode::Char(' ') if !typing => Some(Action::PauseOrResume),
        KeyCode::Char('r') if !typing => Some(Action::Reset),
        KeyCode::Backspace => Some(Action::Backspace),
        KeyCode::Esc if typing => Some(Action::Cancel),
        KeyCode::Char(c) if (typing || tunable) && (c.is_ascii_digit() || c == '.') => {
            Some(Action::Type(c))
        }
        KeyCode::Char(c) if c.is_ascii_digit() && c != '0' => {
            // 1-9 reach the first nine stages; a quest with more is walked through with Tab.
            let wanted = c as usize - '1' as usize;
            (wanted < stages.len()).then_some(Action::Stage(wanted))
        }
        _ => None,
    }
}

fn next(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + 1) % len }
}

fn previous(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + len - 1) % len }
}
