//! Every screen nmtk draws.
//!
//! The engines know nothing about this crate: they hand out numbers and enums, and the wording
//! lives in the message table. That boundary is what keeps a second language, or a second front
//! end, from touching the learning code at all.

pub mod app;
mod chrome;
mod help;
mod home;
mod logo;
mod settings_screen;
mod subject;
pub mod theme;

use std::io;
use std::time::Duration;

use crossterm::event::{self, Event};
use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::{App, Screen};
use crate::theme::{FRAME_MILLIS, MIN_HEIGHT, MIN_WIDTH, Theme};

/// Runs nmtk until the reader quits, restoring the terminal whatever happens.
pub fn run() -> io::Result<()> {
    let mut app = App::new();
    let mut terminal = ratatui::try_init()?;
    let result = event_loop(&mut terminal, &mut app);
    ratatui::try_restore()?;
    result
}

fn event_loop(terminal: &mut ratatui::DefaultTerminal, app: &mut App) -> io::Result<()> {
    while !app.quit {
        terminal.draw(|frame| draw(frame, app))?;
        // Waking ten times a second is enough for a screen and leaves the cores to the work.
        if event::poll(Duration::from_millis(FRAME_MILLIS))? {
            match event::read()? {
                Event::Key(key) => app.on_key(key),
                Event::Resize(_, _) => {}
                _ => {}
            }
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

    let [title_area, body_area, keys_area] = Layout::vertical([
        Constraint::Length(1),
        Constraint::Min(1),
        Constraint::Length(1),
    ])
    .areas(area);

    chrome::title_bar(frame, title_area, theme, screen_title(app, language), language);

    match app.screen {
        Screen::Home => home::render(frame, body_area, app, theme),
        Screen::Settings => settings_screen::render(frame, body_area, app, theme),
        Screen::Help => help::render(frame, body_area, language, theme),
        Screen::Subject(subject) => {
            subject::render(frame, body_area, subject, language, theme)
        }
    }

    chrome::key_bar(frame, keys_area, theme, keys_for(app.screen), language);
}

fn screen_title(app: &App, language: nmtk_core::Language) -> &'static str {
    match app.screen {
        Screen::Home => t(Msg::Home, language),
        Screen::Settings => t(Msg::MenuSettings, language),
        Screen::Help => t(Msg::HelpTitle, language),
        Screen::Subject(subject) => t(subject.title(), language),
    }
}

fn keys_for(screen: Screen) -> &'static [(&'static str, Msg)] {
    match screen {
        Screen::Home => &[
            ("↑↓", Msg::KeyMove),
            ("Enter", Msg::KeyOpen),
            ("l", Msg::KeyLanguage),
            ("?", Msg::KeyHelp),
            ("q", Msg::KeyQuit),
        ],
        Screen::Settings => &[
            ("↑↓", Msg::KeyMove),
            ("←→", Msg::KeyOpen),
            ("l", Msg::KeyLanguage),
            ("?", Msg::KeyHelp),
            ("q", Msg::KeyBack),
        ],
        Screen::Help => &[("q", Msg::KeyBack)],
        Screen::Subject(_) => &[
            ("Enter", Msg::KeyRun),
            ("Space", Msg::KeyPause),
            ("r", Msg::KeyReset),
            ("l", Msg::KeyLanguage),
            ("q", Msg::KeyBack),
        ],
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::{Screen, Subject};
    use nmtk_core::{Language, MachineProfile, Settings};
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    fn app_on(screen: Screen, language: Language) -> App {
        let mut app = App::new();
        app.settings = Settings { language, ..Settings::default() };
        app.machine =
            MachineProfile { logical_cores: 12, total_memory_bytes: 14_000_000_000, available_memory_bytes: 0 };
        app.screen = screen;
        app
    }

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

    /// Prints every screen at the smallest supported size. Run with `--nocapture` to look at them.
    #[test]
    fn every_screen_draws_at_eighty_by_twenty_four() {
        for (name, screen) in [
            ("home", Screen::Home),
            ("settings", Screen::Settings),
            ("help", Screen::Help),
            ("subject", Screen::Subject(Subject::ProofOfWork)),
        ] {
            let app = app_on(screen, Language::English);
            let text = shot(&app, 80, 24);
            println!("\n===== {name} (80x24) =====\n{text}");
            assert!(text.contains("NMTK"), "{name} lost the title bar");
        }
    }

    #[test]
    fn the_home_screen_reads_in_korean_too() {
        let app = app_on(Screen::Home, Language::Korean);
        let text = shot(&app, 100, 30);
        println!("\n===== home, Korean (100x30) =====\n{text}");
        // A Korean glyph fills two cells, so the lifted text carries a space inside each
        // word. The terminal draws it correctly; only this comparison has to allow for it.
        assert!(text.replace(' ', "").contains("재미없으면"), "the motto is missing in Korean");
    }

    #[test]
    fn a_small_terminal_says_so_instead_of_drawing_a_broken_screen() {
        let app = app_on(Screen::Home, Language::English);
        let text = shot(&app, 60, 20);
        assert!(text.contains("80x24"), "a cramped terminal got no explanation:\n{text}");
    }

    #[test]
    fn a_large_terminal_still_centres_the_logo() {
        let app = app_on(Screen::Home, Language::English);
        let text = shot(&app, 160, 50);
        println!("\n===== home (160x50) =====\n{text}");
        assert!(text.lines().any(|line| line.contains('█')), "the logo vanished on a big screen");
    }
}
