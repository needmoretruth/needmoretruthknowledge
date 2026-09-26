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
    /// The first launch: what nmtk is, and the settings worth having before the first quest.
    Welcome,
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

/// A quest is closed when it is dropped, whichever way it goes.
///
/// `close()` stops the quest's worker threads, and the standard says it is always called before a
/// session is dropped. Calling it by hand at each place a quest can end — back, quit, opening
/// another, Ctrl+C — left the places nobody thought of: an error from the terminal, a panic, a
/// signal. Each of those dropped a session with its miners still hashing. Tying it to the drop
/// makes the rule hold on every path, including the ones added later.
impl Drop for OpenQuest {
    fn drop(&mut self) {
        self.session.close();
    }
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
    /// Whether the last draw found the terminal smaller than nmtk can draw on. While it is, the
    /// only keys that work are the ways out: a reader looking at "make the terminal larger"
    /// cannot see what Enter would do, and a key pressed blind in a quest said something they
    /// never read.
    pub too_small: bool,
    /// Whether settings are written to disk. Off for an app built by a test, which must never
    /// read or rewrite the settings of whoever runs `cargo test`.
    persist: bool,
}

impl App {
    /// The app the program runs: settings read from disk, and written back as they change.
    pub fn new(catalogue: Catalogue) -> Self {
        let loaded = Settings::load_reporting();
        let mut app = Self::detached(catalogue, loaded.settings, MachineProfile::detect());
        app.screen = if loaded.first_launch { Screen::Welcome } else { Screen::Quests };
        app.status = loaded.damage.as_ref().map(damage_notice);
        app.persist = true;
        app
    }

    /// An app that never touches the disk, opened on the shelf with the settings it is given.
    pub fn detached(catalogue: Catalogue, settings: Settings, machine: MachineProfile) -> Self {
        Self {
            settings,
            machine,
            catalogue,
            screen: Screen::Quests,
            behind_help: None,
            list_index: 0,
            sort: SortKey::Category,
            filter: Filter::default(),
            open: None,
            settings_index: 0,
            language_index: 0,
            transcript_scroll: 0,
            transcript_furthest: 0,
            transcript_follows: true,
            behind_languages: None,
            status: None,
            quit: false,
            too_small: false,
            persist: false,
        }
    }

    /// Writes the settings when this app keeps them, and reports success when it does not.
    fn save_settings(&self) -> bool {
        !self.persist || self.settings.save().is_ok()
    }

    pub fn language(&self) -> Language {
        self.settings.language
    }

    pub fn threads(&self) -> usize {
        self.settings.resolved_worker_threads(&self.machine)
    }

    /// The rows the shelf is showing.
    pub fn visible(&self) -> Vec<&dyn Kq> {
        self.catalogue.list(&self.filter, self.sort, self.language())
    }

    /// Handles one key.
    ///
    /// Releases are ignored, and so are repeats of every key but the ones that move: a held key
    /// must not open a quest twice, but a held arrow on a terminal that reports repeats — Windows,
    /// and any terminal with the keyboard protocol on — has to keep moving, or holding ↓ through
    /// a long list does nothing at all past the first row.
    pub fn on_key(&mut self, key: KeyEvent) {
        match key.kind {
            KeyEventKind::Press => {}
            KeyEventKind::Repeat if repeats(key.code) => {}
            _ => return,
        }
        if key.modifiers.contains(KeyModifiers::CONTROL)
            && matches!(key.code, KeyCode::Char('c' | 'C'))
        {
            self.shut_down();
            return;
        }
        if self.too_small && !matches!(key.code, KeyCode::Char('q') | KeyCode::Esc) {
            return;
        }
        self.status = None;
        match self.screen {
            Screen::Welcome => self.on_key_welcome(key.code),
            Screen::Help => self.on_key_help(key.code),
            Screen::Languages => self.on_key_languages(key.code),
            Screen::Quests => self.on_key_quests(key.code),
            Screen::Quest => self.on_key_quest(key.code),
            Screen::Settings => self.on_key_settings(key.code),
        }
    }

    /// The first launch. Enter finishes it, and finishing it writes the file that means it is
    /// never shown again — even if nothing was changed, because "English is fine" is an answer.
    fn on_key_welcome(&mut self, code: KeyCode) {
        let count = SettingItem::ALL.len();
        match code {
            KeyCode::Up | KeyCode::Char('k') => {
                self.settings_index = previous(self.settings_index, count)
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.settings_index = next(self.settings_index, count)
            }
            KeyCode::Left | KeyCode::Char('h') => self.adjust_setting(-1),
            KeyCode::Right => self.adjust_setting(1),
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Enter => {
                self.remember();
                self.screen = Screen::Quests;
                self.settings_index = 0;
            }
            KeyCode::Char('q') | KeyCode::Esc => {
                self.remember();
                self.quit = true;
            }
            _ => {}
        }
    }

    /// The help screen lists `l` and `s` under "everywhere", so they work here too. They used to
    /// be the two keys on this screen that did nothing, and a reader reading the list tries the
    /// list on the screen it is written on.
    fn on_key_help(&mut self, code: KeyCode) {
        match code {
            KeyCode::Char('?') | KeyCode::Esc | KeyCode::Char('q') | KeyCode::Enter => {
                self.screen = self.behind_help.take().unwrap_or(Screen::Quests);
            }
            KeyCode::Char('l') => self.open_languages(),
            KeyCode::Char('s') => self.open_settings_from_help(),
            _ => {}
        }
    }

    /// `s` from the help screen. Opened from the first launch, help goes back to it instead: that
    /// screen is the settings, and leaving it for the settings screen would skip the step that
    /// writes the file, so the first launch would be asked again next time.
    fn open_settings_from_help(&mut self) {
        let first_launch = self.within_first_launch();
        self.behind_help = None;
        self.screen = if first_launch { Screen::Welcome } else { Screen::Settings };
    }

    /// Whether the screens stacked up behind this one lead back to the first launch, which has
    /// to be finished with Enter rather than walked away from.
    fn within_first_launch(&self) -> bool {
        let behind_languages = self.behind_languages == Some(Screen::Welcome);
        match (self.screen, self.behind_help) {
            (Screen::Help, Some(Screen::Welcome)) => true,
            (Screen::Help, Some(Screen::Languages)) => behind_languages,
            (Screen::Languages, _) => behind_languages,
            _ => false,
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
            // Listed under "everywhere", so they work here as well.
            KeyCode::Char('?') => self.open_help(),
            KeyCode::Char('s') if !self.within_first_launch() => {
                self.behind_languages = None;
                self.screen = Screen::Settings;
            }
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
            KeyCode::Char('s') => self.screen = Screen::Settings,
            KeyCode::Char('l') => self.toggle_language(),
            KeyCode::Char('?') => self.open_help(),
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
                self.note_if_finished();
                return;
            }
            KeyCode::BackTab => {
                self.step_stage(-1);
                self.note_if_finished();
                return;
            }
            _ => {}
        }
        let tunable = self.open.as_ref().is_some_and(|quest| !quest.session.knobs().is_empty());
        let Some(action) = action_for(code, typing, tunable, self.stages()) else { return };
        let taken = match &mut self.open {
            Some(quest) => quest.session.on(action),
            None => return,
        };
        if taken == Reaction::Handled {
            // A beat the reader caused is a beat they want to see.
            self.transcript_follows = true;
            // The keypress that reveals the last beat of the last stage is the one that finishes
            // the quest, and it is handled by the quest — so the mark has to be made here too,
            // or a reader who reads to the very end is never recorded as having got there.
            self.note_if_finished();
            return;
        }
        // Enter at the end of a stage carries the reader into the next one. A quest knows where
        // its own conversation ends; only the shell knows there is another stage after it. In the
        // middle of a run the same key does nothing, which is what the reader is being told.
        let at_end = self.open.as_ref().is_some_and(|quest| quest.session.at_end());
        if action == Action::Go && at_end {
            self.step_stage(1);
        }
        self.note_if_finished();
    }

    /// Remembers a quest whose last stage has said everything it has to say.
    ///
    /// A shelf that looks the same after finishing a quest as it did before cannot tell the
    /// reader where they got to, and four unmarked quests is four decisions to make again.
    fn note_if_finished(&mut self) {
        let Some(quest) = &self.open else { return };
        let last = quest.session.stage() + 1 >= quest.stages.len();
        if !last || !quest.session.at_end() {
            return;
        }
        let id = quest.id.to_string();
        if self.settings.remember_finished(&id) {
            self.save_settings();
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
        self.catalogue.find(open.id)
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
        // The quest before it stops first. One heavy run at a time: opening the next one while
        // the last still held its threads would have the two share the machine for a moment.
        self.close_quest();
        let opened = self
            .catalogue
            .find(id)
            .map(|quest| (quest.title(self.settings.language), quest.open(&self.work_machine())));
        if let Some((title, session)) = opened {
            self.open = Some(OpenQuest { id, version, title, stages, session });
            self.transcript_scroll = 0;
            self.transcript_furthest = 0;
            self.transcript_follows = true;
            self.screen = Screen::Quest;
        }
    }

    /// Stops the open quest's threads. Dropping it is what closes it; see [`OpenQuest`].
    fn close_quest(&mut self) {
        self.open = None;
    }

    /// Asks the program to end: Ctrl+C, or a signal from outside.
    ///
    /// The open quest is left open on purpose. It is closed when the app is dropped, which the
    /// program does only after it has given the terminal back: closing it here joined its threads
    /// with the terminal still raw on the alternate screen, and a second signal while they wound
    /// down ended the process there.
    pub fn shut_down(&mut self) {
        self.quit = true;
    }

    /// The open quest's name in the language on screen.
    ///
    /// The name kept when the quest was opened is in the language it was opened in, so switching
    /// language inside a quest used to rewrite the whole conversation and leave the title bar in
    /// the old language.
    pub fn open_title(&self) -> Option<&'static str> {
        let open = self.open.as_ref()?;
        Some(self.open_definition().map_or(open.title, |quest| quest.title(self.language())))
    }

    /// The machine as the work should see it, with the thread count from settings applied.
    ///
    /// Quests size their threads from the profile they are opened with. Handed the bare machine,
    /// every quest took all cores but one whatever the settings said, so the one setting that
    /// exists to change how hard nmtk works on this machine changed nothing.
    pub fn work_machine(&self) -> MachineProfile {
        if self.settings.worker_threads == 0 {
            self.machine
        } else {
            self.machine.with_worker_threads(self.threads())
        }
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
        let next_index = if step > 0 { (at + 1) % count } else { (at + count - 1) % count };
        self.settings.language = Language::ALL[next_index];
    }

    /// Thread counts in the order a reader expects them: 1, 2, … every core, then `auto`.
    ///
    /// `auto` is stored as 0 but it is the largest choice, not the smallest, so it belongs at the
    /// top of the run. With it at the bottom, pressing right on `auto (11)` dropped the machine
    /// to one thread, which reads as the key doing the opposite of what it says.
    fn step_threads(&mut self, step: i32) {
        let cores = self.machine.logical_cores.max(1);
        // A settings file can hold more threads than this machine has — it was written on a
        // bigger one, or by hand. Counted as every core, so left steps down from there rather
        // than from a number that was never on screen.
        let at = match self.settings.worker_threads.min(cores) {
            0 => cores,
            threads => threads - 1,
        } as i32;
        let cores = cores as i32;
        let next = (at + step).clamp(0, cores);
        self.settings.worker_threads = if next == cores { 0 } else { (next + 1) as usize };
    }

    fn adjust_setting(&mut self, step: i32) {
        let before = self.settings.clone();
        match SettingItem::ALL[self.settings_index.min(SettingItem::ALL.len() - 1)] {
            SettingItem::Language => self.step_language(step),
            SettingItem::Threads => self.step_threads(step),
            // A choice the reader makes here is theirs, and outranks NO_COLOR from now on.
            SettingItem::Colour => self.settings.choose_colour(!self.settings.colour),
        }
        // A key that changed nothing says nothing. "Saved." after a left arrow already at the
        // leftmost value tells the reader something happened when nothing did.
        if self.settings != before {
            self.remember();
            // A quest already open was sized when it opened. Saying "saved" and nothing else
            // lets the reader believe the run in front of them just changed.
            if self.settings.worker_threads != before.worker_threads
                && self.open.is_some()
                && self.status == Some(Msg::SettingsSaved)
            {
                self.status = Some(Msg::SettingsThreadsNextQuest);
            }
        }
    }

    fn remember(&mut self) {
        self.status =
            Some(if self.save_settings() { Msg::SettingsSaved } else { Msg::SettingsNotSaved });
    }
}

/// The gesture a key means inside a quest.
///
/// `tunable` says whether the stage showing has values the reader can change. When it does, the
/// digits belong to those values — a quest that promises "type a number" and then swallows every
/// digit as a stage jump has promised nothing. Tab moves between stages instead, and where there
/// is nothing to type the digits go back to reaching stages directly.
fn action_for(code: KeyCode, typing: bool, tunable: bool, stages: &[StageSpec]) -> Option<Action> {
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

/// What the shelf says on a launch that found part of the settings file unreadable.
///
/// The recovered settings are in use and the next save writes only those, so the reader is told
/// once, where the file as they left it has gone. A setting that quietly went back to its default
/// reads as nmtk forgetting it.
pub fn damage_notice(damage: &nmtk_core::settings::Damage) -> Msg {
    match damage {
        nmtk_core::settings::Damage::KeptAs(_) => Msg::SettingsDamagedKept,
        nmtk_core::settings::Damage::NotKept => Msg::SettingsDamagedNotKept,
    }
}

/// Keys that keep acting while held: the ones that move, scroll or step a value.
fn repeats(code: KeyCode) -> bool {
    matches!(
        code,
        KeyCode::Up
            | KeyCode::Down
            | KeyCode::Left
            | KeyCode::Right
            | KeyCode::PageUp
            | KeyCode::PageDown
            | KeyCode::Backspace
            | KeyCode::Char('j' | 'k')
    )
}

fn next(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + 1) % len }
}

fn previous(index: usize, len: usize) -> usize {
    if len == 0 { 0 } else { (index + len - 1) % len }
}
