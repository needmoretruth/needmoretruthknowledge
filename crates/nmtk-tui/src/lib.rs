//! Every screen nmtk draws.
//!
//! The quests know nothing about this crate: they hand out lines and draw into a rectangle they
//! are given, and their words live in their own phrase tables. That boundary is what lets a quest
//! be written without touching the shell, and the shell without touching a quest.

pub mod app;
mod chrome;
mod help;
mod languages;
mod logo;
mod quest;
mod quests;
mod settings_screen;

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};
use nmtk_i18n::{Msg, t};
use nmtk_kq::Catalogue;
use nmtk_kq::theme::{FRAME_MILLIS, MIN_HEIGHT, MIN_WIDTH, Theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::{App, Screen};

/// Every quest the program ships with, newest version and older ones alike.
fn catalogue() -> Catalogue {
    Catalogue::new(vec![
        Box::new(kq_proof_of_work::ProofOfWork),
        Box::new(kq_ledgers::Ledgers),
    ])
}

/// Runs nmtk until the reader quits, restoring the terminal whatever happens.
pub fn run() -> io::Result<()> {
    let mut app = App::new(catalogue());
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut app);
    ratatui::try_restore()?;
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.quit {
        // The open quest reads its workers' latest state here, on this thread, before drawing.
        if let Some(quest) = &mut app.open {
            quest.session.tick();
        }
        terminal.draw(|frame| draw(frame, app))?;
        // Waking ten times a second is enough for a screen and leaves the cores to the work.
        if event::poll(Duration::from_millis(FRAME_MILLIS))?
            && let Event::Key(key) = event::read()?
        {
            app.on_key(key);
        }
    }
    Ok(())
}

fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::new(app.settings.colour);
    let language = app.language();
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        chrome::too_small(frame, theme, language);
        return;
    }

    let mut drawn_conversation: Option<usize> = None;
    let [title_area, body_area, keys_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .areas(area);

    chrome::title_bar(frame, title_area, theme, &screen_title(app, language), language);

    match app.screen {
        Screen::Quests => quests::render(frame, body_area, app, theme),
        Screen::Settings => settings_screen::render(frame, body_area, app, theme),
        Screen::Help => help::render(frame, body_area, language, theme),
        Screen::Languages => languages::render(frame, body_area, app, theme),
        Screen::Quest => {
            let furthest = match (&app.open, app.open_definition()) {
                (Some(quest), definition) => {
                    quest::render(frame, body_area, quest, definition, app, theme, language)
                }
                _ => 0,
            };
            drawn_conversation = Some(furthest);
        }
    }

    chrome::key_bar(frame, keys_area, theme, &keys_for(app, language));
    if let Some(furthest) = drawn_conversation {
        app.conversation_drawn(furthest);
    }
}

fn screen_title(app: &App, language: nmtk_core::Language) -> String {
    match app.screen {
        Screen::Quests => t(Msg::Quests, language).to_string(),
        Screen::Settings => t(Msg::MenuSettings, language).to_string(),
        Screen::Help => t(Msg::HelpTitle, language).to_string(),
        Screen::Languages => t(Msg::SettingsLanguage, language).to_string(),
        Screen::Quest => match &app.open {
            Some(quest) => quest.title.to_string(),
            None => t(Msg::Quests, language).to_string(),
        },
    }
}

/// The bottom bar: the shell's keys, plus whatever the open quest adds.
fn keys_for(app: &App, language: nmtk_core::Language) -> Vec<(&'static str, String)> {
    let say = |key: &'static str, message: Msg| (key, t(message, language).to_string());
    match app.screen {
        Screen::Quests => vec![
            say("↑↓", Msg::KeyMove),
            say("Enter", Msg::KeyOpen),
            say("o", Msg::LabelSort),
            say("f", Msg::LabelFilter),
            say("v", Msg::LabelVersion),
            say("l", Msg::KeyLanguage),
            say("?", Msg::KeyHelp),
            say("q", Msg::KeyQuit),
        ],
        Screen::Settings => vec![
            say("↑↓", Msg::KeyMove),
            say("←→", Msg::KeyOpen),
            say("l", Msg::KeyLanguage),
            say("q", Msg::KeyBack),
        ],
        Screen::Help => vec![say("q", Msg::KeyBack)],
        Screen::Languages => {
            vec![say("↑↓", Msg::KeyMove), say("Enter", Msg::KeyOpen), say("q", Msg::KeyBack)]
        }
        Screen::Quest => {
            // Ordered by how badly a reader needs it, because a narrow terminal keeps the front
            // of this list and drops the back.
            let mut keys = vec![say("Enter", Msg::KeyContinue), say("q", Msg::KeyBack)];
            if let Some(quest) = &app.open {
                keys.extend(
                    quest.session.keys(language).into_iter().map(|(k, l)| (k, l.to_string())),
                );
            }
            keys.push(say("Tab", Msg::KeyStage));
            keys.push(say("PgUp", Msg::KeyScroll));
            keys.push(say("r", Msg::KeyReset));
            keys.push(say("?", Msg::KeyHelp));
            keys
        }
    }
}

#[cfg(test)]
mod tests {
    use crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
    use nmtk_core::{Language, MachineProfile, Settings};
    use nmtk_kq::SortKey;
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn app_in(language: Language) -> App {
        let mut app = App::new(catalogue());
        app.settings = Settings { language, ..Settings::default() };
        app.machine = MachineProfile {
            logical_cores: 12,
            total_memory_bytes: 14_000_000_000,
            available_memory_bytes: 0,
        };
        app
    }

    fn press(app: &mut App, code: KeyCode) {
        app.on_key(KeyEvent::new(code, KeyModifiers::NONE));
    }

    /// The screen as text, with wide glyphs left as they are drawn.
    fn shot(app: &mut App, width: u16, height: u16) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("test backend");
        terminal.draw(|frame| draw(frame, app)).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
                    .trim_end()
                    .to_string()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// The shelf with the first quest open on it, ready for keys.
    fn opened(language: Language) -> App {
        let mut app = app_in(language);
        press(&mut app, KeyCode::Enter);
        app
    }

    /// The shelf with one named quest open. Tests that read a quest's own words say which one.
    fn opened_named(language: Language, title: &str) -> App {
        let mut app = app_in(language);
        let at = app
            .visible()
            .iter()
            .position(|quest| quest.title(language) == title)
            .unwrap_or_else(|| panic!("{title} is not on the shelf"));
        app.list_index = at;
        press(&mut app, KeyCode::Enter);
        app
    }

    #[test]
    fn the_shelf_shows_a_quest_at_the_smallest_screen() {
        let mut app = app_in(Language::ENGLISH);
        let text = shot(&mut app, 80, 24);
        println!("\n===== quests (80x24) =====\n{text}");
        assert!(text.contains("Ledger models"), "the quest is missing:\n{text}");
        assert!(text.contains("v0.0.1"), "the version is missing:\n{text}");
    }

    #[test]
    fn opening_a_quest_starts_with_one_sentence_and_its_stages() {
        let mut app = opened_named(Language::ENGLISH, "Ledger models");
        let text = shot(&mut app, 100, 30);
        println!("\n===== a quest, first beat (100x30) =====\n{text}");
        assert!(text.contains("Where money lives"), "the stage strip is missing:\n{text}");
        assert!(text.contains("Ledger models"), "the quest is not named:\n{text}");
        assert!(text.contains("1/6"), "the strip does not say where the reader is:\n{text}");
        assert!(
            text.contains("more than one way to write down where"),
            "the first beat is missing:\n{text}"
        );
        assert!(
            !text.contains("Bitcoin counts coins"),
            "the second beat arrived before Enter was pressed:\n{text}"
        );
    }

    #[test]
    fn enter_adds_one_beat_at_a_time() {
        let mut app = opened_named(Language::ENGLISH, "Ledger models");
        press(&mut app, KeyCode::Enter);
        let text = shot(&mut app, 100, 30);
        assert!(text.contains("Bitcoin counts coins"), "Enter added nothing:\n{text}");
        assert!(
            !text.contains("Ethereum keeps a book"),
            "one Enter revealed two beats:\n{text}"
        );
    }

    #[test]
    fn tab_walks_the_stages_and_stops_at_the_end() {
        let mut app = opened(Language::ENGLISH);
        for _ in 0..20 {
            press(&mut app, KeyCode::Tab);
        }
        let last = app.open.as_ref().expect("a quest is open").stages.len() - 1;
        assert_eq!(app.open.as_ref().unwrap().session.stage(), last, "Tab wrapped or overran");
        for _ in 0..20 {
            press(&mut app, KeyCode::BackTab);
        }
        assert_eq!(app.open.as_ref().unwrap().session.stage(), 0);
    }

    /// The bug a first-time reader found: every quest promises "type a number", and no digit ever
    /// reached a value because the shell spent them all on jumping between stages.
    #[test]
    fn a_typed_number_reaches_the_value_rather_than_jumping_a_stage() {
        let mut app = opened(Language::ENGLISH);
        // Walk to the stage that has values to turn.
        for _ in 0..3 {
            press(&mut app, KeyCode::Tab);
        }
        let stage = app.open.as_ref().unwrap().session.stage();
        for c in "17".chars() {
            press(&mut app, KeyCode::Char(c));
        }
        assert_eq!(
            app.open.as_ref().unwrap().session.stage(),
            stage,
            "typing a number jumped to another stage"
        );
        assert!(app.open.as_ref().unwrap().session.typing(), "the digits never reached the value");
        let text = shot(&mut app, 100, 30);
        assert!(text.contains("17_"), "what was typed is not on screen:\n{text}");
    }

    #[test]
    fn the_key_bar_keeps_the_way_out_at_the_smallest_screen() {
        let mut app = opened(Language::ENGLISH);
        let text = shot(&mut app, 80, 24);
        let bar = text.lines().last().unwrap_or_default().to_string();
        println!("\n===== key bar (80) =====\n{bar}");
        assert!(bar.contains("q back"), "the way out is missing: {bar:?}");
        assert!(bar.contains("Enter continue"), "the way on is missing: {bar:?}");
    }

    #[test]
    fn help_names_only_keys_that_do_something() {
        let mut app = app_in(Language::ENGLISH);
        press(&mut app, KeyCode::Char('?'));
        let text = shot(&mut app, 100, 30);
        println!("\n===== help (100x30) =====\n{text}");
        for key in ["Tab", "PgUp", "o", "f", "v"] {
            assert!(text.contains(key), "help left out {key}:\n{text}");
        }
    }

    #[test]
    fn the_shelf_reads_in_korean_too() {
        let mut app = app_in(Language::KOREAN);
        let text = shot(&mut app, 80, 24);
        assert!(text.replace(' ', "").contains("원장방식"), "the quest lost its name:\n{text}");
        assert!(text.replace(' ', "").contains("재미없는공부는"), "the motto is missing:\n{text}");
    }

    /// Korean words take two columns each, which is how the last version managed to draw `q 뒤`
    /// and hide the way out of the program on a narrow terminal.
    #[test]
    fn the_korean_key_bar_keeps_whole_words_at_every_width() {
        let mut app = opened(Language::KOREAN);
        for width in [80u16, 100, 140] {
            let text = shot(&mut app, width, 30);
            let bar = text.lines().last().unwrap_or_default().replace(' ', "");
            assert!(bar.contains("뒤로"), "the way back was cut in half at {width}: {bar:?}");
            assert!(bar.contains("계속"), "the way on was cut in half at {width}: {bar:?}");
        }
    }

    #[test]
    fn a_korean_conversation_reaches_the_reader_whole() {
        let mut app = opened_named(Language::KOREAN, "원장 방식");
        for _ in 0..5 {
            press(&mut app, KeyCode::Enter);
        }
        let text = shot(&mut app, 100, 30).replace(' ', "");
        println!("\n===== a quest in Korean (100x30) =====\n{text}");
        // The fifth beat, start and end, so a dropped middle line shows up as a failure.
        assert!(text.contains("오른쪽에셋이다돌고있고"), "a Korean beat lost its start:\n{text}");
        assert!(text.contains("10짜리코인이셋있습니다"), "a Korean beat lost its end:\n{text}");
    }

    #[test]
    fn a_small_terminal_says_so_instead_of_drawing_a_broken_screen() {
        let mut app = app_in(Language::ENGLISH);
        let text = shot(&mut app, 60, 20);
        assert!(text.contains("80x24"), "a cramped terminal got no explanation:\n{text}");
    }

    #[test]
    fn sorting_and_filtering_do_not_lose_the_quest() {
        let mut app = app_in(Language::ENGLISH);
        for _ in 0..SortKey::ALL.len() {
            press(&mut app, KeyCode::Char('o'));
            assert!(!app.visible().is_empty(), "sorting emptied the shelf");
        }
        press(&mut app, KeyCode::Char('f'));
        assert!(!app.visible().is_empty(), "filtering to a real category emptied the shelf");
    }
}
