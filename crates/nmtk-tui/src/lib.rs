//! Every screen nmtk draws.
//!
//! The quests know nothing about this crate: they hand out lines and draw into a rectangle they
//! are given, and their words live in their own phrase tables. That boundary is what lets a quest
//! be written without touching the shell, and the shell without touching a quest.

pub mod app;
mod chrome;
mod help;
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
    Catalogue::new(vec![Box::new(kq_ledgers::Ledgers)])
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

fn draw(frame: &mut Frame, app: &App) {
    let theme = Theme::new(app.settings.colour);
    let language = app.language();
    let area = frame.area();
    if area.width < MIN_WIDTH || area.height < MIN_HEIGHT {
        chrome::too_small(frame, theme, language);
        return;
    }

    let [title_area, body_area, keys_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .areas(area);

    chrome::title_bar(frame, title_area, theme, &screen_title(app, language), language);

    match app.screen {
        Screen::Quests => quests::render(frame, body_area, app, theme),
        Screen::Settings => settings_screen::render(frame, body_area, app, theme),
        Screen::Help => help::render(frame, body_area, language, theme),
        Screen::Quest => match &app.open {
            Some(quest) => quest::render(frame, body_area, quest, theme, language),
            None => quests::render(frame, body_area, app, theme),
        },
    }

    chrome::key_bar(frame, keys_area, theme, &keys_for(app, language));
}

fn screen_title(app: &App, language: nmtk_core::Language) -> String {
    match app.screen {
        Screen::Quests => t(Msg::Quests, language).to_string(),
        Screen::Settings => t(Msg::MenuSettings, language).to_string(),
        Screen::Help => t(Msg::HelpTitle, language).to_string(),
        Screen::Quest => match &app.open {
            Some(quest) => format!(
                "{}  ·  {}",
                quest.title,
                t(quest::stage_name(quest.session.stage()), language)
            ),
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
        Screen::Quest => {
            let mut keys = vec![
                say("1-5", Msg::KeyMove),
                say("Enter", Msg::KeyRun),
                say("Space", Msg::KeyPause),
                say("r", Msg::KeyReset),
                say("l", Msg::KeyLanguage),
                say("q", Msg::KeyBack),
            ];
            if let Some(quest) = &app.open {
                keys.extend(
                    quest.session.keys(language).into_iter().map(|(k, l)| (k, l.to_string())),
                );
            }
            keys
        }
    }
}

#[cfg(test)]
mod tests {
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

    /// The screen as text, with wide glyphs left as they are drawn.
    fn shot(app: &App, width: u16, height: u16) -> String {
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

    #[test]
    fn the_shelf_shows_a_quest_at_the_smallest_screen() {
        let app = app_in(Language::English);
        let text = shot(&app, 80, 24);
        println!("\n===== quests (80x24) =====\n{text}");
        assert!(text.contains("Ledger models"), "the quest is missing:\n{text}");
        assert!(text.contains("v1.0"), "the version is missing:\n{text}");
    }

    #[test]
    fn opening_a_quest_shows_its_stages() {
        let mut app = app_in(Language::English);
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Enter,
            crossterm::event::KeyModifiers::NONE,
        ));
        let text = shot(&app, 80, 24);
        println!("\n===== a quest (80x24) =====\n{text}");
        assert!(text.contains("Brief"), "the stage strip is missing:\n{text}");
        assert!(text.contains("Run"), "the stage strip is missing:\n{text}");
    }

    #[test]
    fn the_shelf_reads_in_korean_too() {
        let app = app_in(Language::Korean);
        let text = shot(&app, 80, 24);
        assert!(text.replace(' ', "").contains("원장방식"), "the quest lost its name:\n{text}");
        assert!(text.replace(' ', "").contains("재미없으면"), "the motto is missing:\n{text}");
    }

    #[test]
    fn a_small_terminal_says_so_instead_of_drawing_a_broken_screen() {
        let app = app_in(Language::English);
        let text = shot(&app, 60, 20);
        assert!(text.contains("80x24"), "a cramped terminal got no explanation:\n{text}");
    }

    #[test]
    fn sorting_and_filtering_do_not_lose_the_quest() {
        let mut app = app_in(Language::English);
        for _ in 0..SortKey::ALL.len() {
            app.on_key(crossterm::event::KeyEvent::new(
                crossterm::event::KeyCode::Char('o'),
                crossterm::event::KeyModifiers::NONE,
            ));
            assert!(!app.visible().is_empty(), "sorting emptied the shelf");
        }
        app.on_key(crossterm::event::KeyEvent::new(
            crossterm::event::KeyCode::Char('f'),
            crossterm::event::KeyModifiers::NONE,
        ));
        assert!(!app.visible().is_empty(), "filtering to a real category emptied the shelf");
    }
}
