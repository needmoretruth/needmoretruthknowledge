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
mod terminal;
mod welcome;

use std::io;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use crossterm::event::{self, Event};
use nmtk_i18n::{Msg, t};
use nmtk_kq::Catalogue;
use nmtk_kq::theme::{FRAME_MILLIS, MIN_HEIGHT, MIN_WIDTH, Theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout};

use crate::app::{App, Screen};

/// Every quest the program ships with: the newest version of each, and nothing older.
fn catalogue() -> Catalogue {
    Catalogue::new(vec![
        Box::new(kq_proof_of_work::ProofOfWork),
        Box::new(kq_ledgers::Ledgers),
        Box::new(kq_transformer::Transformer),
        Box::new(kq_zero_knowledge::ZeroKnowledge),
    ])
}

/// Runs nmtk until the reader quits, restoring the terminal whatever happens.
///
/// A stop from outside — `kill`, a closed terminal — ends it the way `q` does, and comes back as
/// an [`io::ErrorKind::Interrupted`] error so the caller can say so.
pub fn run() -> io::Result<()> {
    let stop = terminal::stop_signal();
    let mut app = App::new(catalogue());
    let mut screen = terminal::take()?;
    let result = event_loop(&mut screen, &mut app, &stop);
    let result = wind_down(app, result, terminal::give_back);
    // Said once the screen is the reader's again, so it can be read.
    for message in terminal::deferred_panics() {
        eprintln!("{message}");
    }
    result
}

/// Ends the program in the one order that is safe: the terminal first, so the reader has their
/// shell back while the quest's threads wind down, and then the quest, whose drop stops them.
///
/// Every way out comes through here — `q`, Ctrl+C, a signal, an error — because the event loop
/// only ever asks to stop and never closes the quest itself. Stopping the quest's threads first
/// kept the terminal raw on the alternate screen for as long as they took, and a second signal
/// in that window ends the process on the spot, leaving the reader at a shell that does not echo.
fn wind_down(
    app: App,
    result: io::Result<()>,
    give_back: impl FnOnce() -> io::Result<()>,
) -> io::Result<()> {
    let restored = give_back();
    drop(app);
    result?;
    restored
}

/// Whether a signal from outside has asked the program to stop. When it has, the app is asked to
/// quit the way Ctrl+C asks it, and the error to end with comes back.
fn stopped_by_signal(app: &mut App, stop: &AtomicBool) -> Option<io::Error> {
    if !stop.load(Ordering::SeqCst) {
        return None;
    }
    app.shut_down();
    Some(io::Error::new(io::ErrorKind::Interrupted, "stopped by a signal"))
}

fn event_loop(screen: &mut terminal::Screen, app: &mut App, stop: &AtomicBool) -> io::Result<()> {
    while !app.quit {
        if let Some(error) = stopped_by_signal(app, stop) {
            return Err(error);
        }
        // The open quest reads its workers' latest state here, on this thread, before drawing.
        if let Some(quest) = &mut app.open {
            quest.session.tick();
        }
        // Drawing measures the terminal first, so a resize is answered by the next frame: the
        // resize event wakes the wait below, and the frame after it is drawn at the new size.
        screen.draw(|frame| draw(frame, app))?;
        // Waking ten times a second is enough for a screen and leaves the cores to the work.
        if !wait_for_event(Duration::from_millis(FRAME_MILLIS))? {
            continue;
        }
        // Everything already waiting is handled before the next frame, not one event per frame:
        // a pasted number or a held arrow otherwise drew a whole screen per keypress and fell
        // further behind the longer the key was held.
        loop {
            if let Event::Key(key) = event::read()? {
                app.on_key(key);
            }
            if app.quit || !wait_for_event(Duration::ZERO)? {
                break;
            }
        }
    }
    Ok(())
}

/// Whether an event is waiting, treating a wait cut short by a signal as "nothing yet". The
/// signal itself is read from its flag at the top of the next frame.
fn wait_for_event(timeout: Duration) -> io::Result<bool> {
    match event::poll(timeout) {
        Err(error) if error.kind() == io::ErrorKind::Interrupted => Ok(false),
        other => other,
    }
}

fn draw(frame: &mut Frame, app: &mut App) {
    let theme = Theme::new(app.settings.colour);
    let language = app.language();
    let area = frame.area();
    app.too_small = area.width < MIN_WIDTH || area.height < MIN_HEIGHT;
    if app.too_small {
        chrome::too_small(frame, theme, language);
        return;
    }

    let mut drawn_conversation: Option<usize> = None;
    let [title_area, body_area, keys_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1), Constraint::Length(1)])
            .areas(area);

    chrome::title_bar(frame, title_area, theme, &screen_title(app, language), language);

    match app.screen {
        Screen::Welcome => welcome::render(frame, body_area, app, theme),
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
        Screen::Welcome => t(Msg::WelcomeTitle, language).to_string(),
        Screen::Quests => t(Msg::Quests, language).to_string(),
        Screen::Settings => t(Msg::MenuSettings, language).to_string(),
        Screen::Help => t(Msg::HelpTitle, language).to_string(),
        Screen::Languages => t(Msg::SettingsLanguage, language).to_string(),
        Screen::Quest => match app.open_title() {
            Some(title) => title.to_string(),
            None => t(Msg::Quests, language).to_string(),
        },
    }
}

/// The bottom bar: the shell's keys, plus whatever the open quest adds.
fn keys_for(app: &App, language: nmtk_core::Language) -> Vec<(&'static str, String)> {
    let say = |key: &'static str, message: Msg| (key, t(message, language).to_string());
    match app.screen {
        Screen::Welcome => vec![
            say("Enter", Msg::WelcomeStart),
            say("↑↓", Msg::KeyMove),
            say("←→", Msg::KeyChange),
            say("l", Msg::KeyLanguage),
            say("?", Msg::KeyHelp),
        ],
        // Ordered by how badly a reader needs it: a narrow terminal keeps the front of the list
        // and drops the back, so the key that reveals every other key comes early.
        Screen::Quests => vec![
            say("↑↓", Msg::KeyMove),
            say("Enter", Msg::KeyOpen),
            say("?", Msg::KeyHelp),
            say("q", Msg::KeyQuit),
            say("l", Msg::KeyLanguage),
            say("o", Msg::LabelSort),
            say("f", Msg::LabelFilter),
        ],
        Screen::Settings => vec![
            say("↑↓", Msg::KeyMove),
            say("←→", Msg::KeyChange),
            say("q", Msg::KeyBack),
            say("?", Msg::KeyHelp),
            say("l", Msg::KeyLanguage),
        ],
        Screen::Help => vec![say("q", Msg::KeyBack)],
        Screen::Languages => vec![
            say("↑↓", Msg::KeyMove),
            say("Enter", Msg::KeyOpen),
            say("q", Msg::KeyBack),
            say("?", Msg::KeyHelp),
        ],
        Screen::Quest => {
            // Ordered by how badly a reader needs it, because a narrow terminal keeps the front
            // of this list and drops the back.
            let over = app.open.as_ref().is_some_and(|quest| {
                quest.session.at_end() && quest.session.stage() + 1 >= quest.stages.len()
            });
            // At the end of the last stage Enter has nothing left to carry, so it stops being
            // offered rather than being offered and doing nothing — unless the quest has said
            // what Enter does instead, which on a stage with knobs is to run it again.
            let named = app.open.as_ref().and_then(|quest| quest.session.go_name(language));
            let mut keys = match (over, named) {
                (_, Some(name)) => vec![
                    ("Enter", name.to_string()),
                    say("q", Msg::KeyBack),
                    say("?", Msg::KeyHelp),
                ],
                // The closing sentence says Shift+Tab walks back, so the bar offers the same key.
                // It offered "Tab next stage" here, twice, on a stage with nothing after it.
                (true, None) => vec![
                    say("q", Msg::KeyBack),
                    say("Shift+Tab", Msg::KeyStageBack),
                    say("?", Msg::KeyHelp),
                ],
                (false, None) => {
                    vec![
                        say("Enter", Msg::KeyContinue),
                        say("q", Msg::KeyBack),
                        say("?", Msg::KeyHelp),
                    ]
                }
            };
            if let Some(quest) = &app.open {
                // Space works but no quest names it, and a reviewer found it by guessing. It is
                // offered while there is something to pause and not while there is not.
                if matches!(
                    quest.session.run_state(),
                    nmtk_kq::session::RunState::Running | nmtk_kq::session::RunState::Paused
                ) {
                    keys.push(say("Space", Msg::KeyPause));
                }
                keys.extend(
                    quest.session.keys(language).into_iter().map(|(k, l)| (k, l.to_string())),
                );
            }
            // Tab stops at the last stage, so there it is not offered as a way on.
            let last = app
                .open
                .as_ref()
                .is_some_and(|quest| quest.session.stage() + 1 >= quest.stages.len());
            if !last {
                keys.push(say("Tab", Msg::KeyStage));
            }
            keys.push(say("PgUp", Msg::KeyScroll));
            keys.push(say("r", Msg::KeyReset));
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
    use crate::app::SettingItem;

    fn app_in(language: Language) -> App {
        // Detached: a test that read the real settings file started on the welcome screen on a
        // machine that had never run nmtk, and one that wrote it changed the reader's language.
        App::detached(
            catalogue(),
            Settings { language, ..Settings::default() },
            MachineProfile {
                logical_cores: 12,
                total_memory_bytes: 14_000_000_000,
                available_memory_bytes: 0,
            },
        )
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

    /// At 80x24 a reviewer read "v0." instead of a version, lost the words "6 stages" across two
    /// lines, and never saw how the list was sorted — the sort sat at the foot of a panel that
    /// had already run out of room.
    #[test]
    fn the_smallest_screen_still_carries_the_version_the_sort_and_the_fit() {
        for language in Language::ALL {
            let mut app = app_in(*language);
            let text = shot(&mut app, 80, 24);
            println!("\n===== quests ({language}, 80x24) =====\n{text}");
            let flat = text.replace(' ', "");
            let quest = app.visible()[app.list_index].meta();
            assert!(flat.contains(&format!("v{}", quest.version)), "version cut short:\n{text}");
            let sort = t(Msg::SortByCategory, *language).replace(' ', "");
            assert!(flat.contains(&sort), "the sort is not on screen:\n{text}");
            let stages = format!("{}{}", quest.stages.len(), t(Msg::LabelStages, *language))
                .replace(' ', "");
            assert!(flat.contains(&stages), "the stage count is split:\n{text}");
            let fit = t(Msg::FitRecommended, *language).replace(' ', "");
            let fit_head: String = fit.chars().take(8).collect();
            assert!(flat.contains(&fit_head), "the machine's fit fell off:\n{text}");
        }
    }

    /// A reviewer finished a quest and found the list exactly as they had left it, with no way
    /// to tell which of the four they had done.
    #[test]
    fn a_finished_quest_is_marked_on_the_list_and_remembered() {
        let mut app = opened(Language::ENGLISH);
        let id = app.open.as_ref().expect("a quest is open").id.to_string();
        assert!(!app.settings.has_finished(&id), "nothing has been finished yet");

        for _ in 0..20 {
            press(&mut app, KeyCode::Tab);
        }
        for _ in 0..80 {
            press(&mut app, KeyCode::Enter);
        }
        assert!(app.settings.has_finished(&id), "finishing the quest was not remembered");

        press(&mut app, KeyCode::Char('q'));
        let text = shot(&mut app, 100, 30);
        println!("\n===== the list, one quest finished =====\n{text}");
        // The row, not the panel beside it: only the row carries the version.
        let title = app.visible()[0].title(Language::ENGLISH).to_string();
        let row = text
            .lines()
            .find(|line| line.contains(&title) && line.contains("v0."))
            .expect("the finished quest is on the list");
        assert!(row.contains('+'), "the finished quest carries no mark: {row:?}");
    }

    #[test]
    fn the_keypress_that_reveals_the_last_beat_finishes_the_quest() {
        let mut app = opened(Language::ENGLISH);
        let id = app.open.as_ref().expect("a quest is open").id.to_string();
        // Straight to the last stage and read it to the end — no extra keypress afterwards. The
        // Enter that reveals the last beat is handled by the quest, and the mark used to be made
        // only on a keypress the quest ignored, so reading to the end recorded nothing.
        for _ in 0..20 {
            press(&mut app, KeyCode::Tab);
        }
        let mut finished = false;
        for _ in 0..80 {
            press(&mut app, KeyCode::Enter);
            if app.settings.has_finished(&id) {
                finished = true;
                break;
            }
        }
        assert!(finished, "reading a quest to its last beat was not remembered");
    }

    #[test]
    fn the_language_screen_lines_up_where_the_glyphs_are_wide() {
        let mut app = app_in(Language::ENGLISH);
        press(&mut app, KeyCode::Char('l'));
        let text = shot(&mut app, 100, 30);
        println!("\n===== languages =====\n{text}");
        // The row is found by its name, and the code by its own letters within that row: "en"
        // also lives inside the word "open" down in the key bar.
        let column_of = |name: &str, code: &str| {
            text.lines().find(|line| line.contains(name)).map(|line| {
                // Counted in cells, not bytes: a Korean glyph is three bytes and one cell in the
                // shot, so byte offsets would report a difference that is not on the screen.
                let at = line.find(code).expect("the code is on the row");
                line[..at].chars().count()
            })
        };
        let english = column_of("English", "en").expect("English is listed");
        // One character of the endonym: the test backend writes a wide glyph as the glyph plus a
        // filler cell, so "한국어" is never contiguous in the shot.
        let korean = column_of("한", "ko").expect("Korean is listed");
        assert_eq!(
            english, korean,
            "한국어 is three characters and six cells; the codes must still line up"
        );
    }

    #[test]
    fn the_shelf_shows_a_quest_at_the_smallest_screen() {
        let mut app = app_in(Language::ENGLISH);
        let text = shot(&mut app, 80, 24);
        println!("\n===== quests (80x24) =====\n{text}");
        assert!(text.contains("Ledger models"), "the quest is missing:\n{text}");
        // Read from the quest rather than written out here, so a release moves one number and
        // not two. What this checks is that the shelf prints it at all.
        let version = format!("v{}", app.visible()[app.list_index].meta().version);
        assert!(text.contains(&version), "{version} is missing:\n{text}");
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
        assert!(!text.contains("Ethereum keeps a book"), "one Enter revealed two beats:\n{text}");
    }

    /// A reviewer pressed Enter four times at the end of a quest before concluding it was over:
    /// nothing was said, and the key bar still offered "Enter continue".
    #[test]
    fn the_end_of_a_quest_says_so_and_stops_offering_enter() {
        for language in Language::ALL {
            let mut app = opened(*language);
            for _ in 0..20 {
                press(&mut app, KeyCode::Tab);
            }
            for _ in 0..80 {
                press(&mut app, KeyCode::Enter);
            }
            let session = &app.open.as_ref().expect("a quest is open").session;
            assert!(session.at_end(), "the last stage never finished saying its piece");
            let text = shot(&mut app, 100, 30);
            println!("\n===== the end ({language}) =====\n{text}");
            let flat = text.replace(' ', "");
            let over = t(Msg::ConversationFinished, *language).replace(' ', "");
            let head: String = over.chars().take(12).collect();
            assert!(flat.contains(&head), "the quest never says it is over:\n{text}");
            let bar = text.lines().last().unwrap_or_default().replace(' ', "");
            let carry = t(Msg::KeyContinue, *language).replace(' ', "");
            assert!(!bar.contains(&format!("Enter{carry}")), "Enter is still offered: {bar:?}");
        }
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

    /// A reviewer at 80x24 lost "? help" and "l language" off both the quest list and the quest
    /// screen, which makes every key they name undiscoverable at the smallest size nmtk supports.
    #[test]
    fn the_key_that_reveals_the_other_keys_survives_the_smallest_screen() {
        for language in Language::ALL {
            for screen in [Screen::Quests, Screen::Quest] {
                let mut app = app_in(*language);
                if screen == Screen::Quest {
                    press(&mut app, KeyCode::Enter);
                }
                let text = shot(&mut app, 80, 24);
                let bar = text.lines().last().unwrap_or_default().to_string();
                println!("\n===== key bar ({language}, {screen:?}, 80) =====\n{bar}");
                // A wide glyph fills two cells and the screen capture gives the second one back
                // as a space, so both sides are compared without them.
                let flat = bar.replace(' ', "");
                let help = format!("?{}", t(Msg::KeyHelp, *language).replace(' ', ""));
                assert!(flat.contains(&help), "help is missing: {bar:?}");
            }
        }
    }

    /// Changing a setting on the first launch replaced the sentence explaining that setting with
    /// the word "Saved.", so the reader trying the arrows to find out what it does lost the answer.
    #[test]
    fn changing_a_setting_keeps_the_sentence_that_explains_it() {
        let mut app = app_in(Language::ENGLISH);
        app.screen = Screen::Welcome;
        app.settings_index = 1;
        press(&mut app, KeyCode::Left);
        let text = shot(&mut app, 80, 30);
        println!("\n===== welcome after a change =====\n{text}");
        let about = t(SettingItem::Threads.about(), Language::ENGLISH);
        let first_words: String = about.split(' ').take(4).collect::<Vec<_>>().join(" ");
        assert!(text.contains(&first_words), "the explanation is gone:\n{text}");
    }

    /// Right means more. With `auto` stored as zero and sorted first, right on "auto (11)" used
    /// to drop the machine to one thread.
    #[test]
    fn stepping_right_from_auto_never_drops_to_one_thread() {
        let mut app = app_in(Language::ENGLISH);
        app.screen = Screen::Welcome;
        app.settings_index = 1;
        let cores = app.machine.logical_cores.max(1);
        press(&mut app, KeyCode::Left);
        assert_eq!(app.settings.worker_threads, cores, "left from auto is every core");
        press(&mut app, KeyCode::Right);
        assert_eq!(app.settings.worker_threads, 0, "right goes back to auto");
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

    /// The help screen, read as the keys in its left-hand column: the first word of every row
    /// that is not a group heading. Matching the whole screen for a letter found `v` inside
    /// "move", which is how this test passed for a key that does not exist.
    fn help_keys(text: &str) -> Vec<String> {
        text.lines()
            .flat_map(|line| line.split('│').map(str::to_string).collect::<Vec<_>>())
            .filter(|cell| cell.starts_with("  ") && !cell.trim().is_empty())
            .flat_map(|cell| {
                // The key column is 12 cells wide after two spaces of indent.
                let keys: String = cell.chars().skip(2).take(12).collect();
                keys.split_whitespace().map(str::to_string).collect::<Vec<_>>()
            })
            .collect()
    }

    #[test]
    fn help_names_only_keys_that_do_something() {
        let mut app = app_in(Language::ENGLISH);
        press(&mut app, KeyCode::Char('?'));
        let text = shot(&mut app, 100, 30);
        println!("\n===== help (100x30) =====\n{text}");
        let keys = help_keys(&text);
        // Every key in the README's table, as the help screen writes it.
        for key in [
            "Enter",
            "Tab",
            "Shift+Tab",
            "↑",
            "↓",
            "←",
            "→",
            "0-9",
            "PgUp",
            "PgDn",
            "Space",
            "r",
            "o",
            "f",
            "l",
            "s",
            "?",
            "q",
            "Esc",
        ] {
            assert!(keys.iter().any(|k| k == key), "help left out {key}: {keys:?}");
        }
        // No key does anything as `v`: there is one version of each quest and nothing to pick.
        assert!(!keys.iter().any(|k| k == "v"), "help names a key that does nothing: {keys:?}");
    }

    /// What help files under "everywhere" has to work everywhere, including on the help screen
    /// and the language list, where `l`, `s` and `?` used to do nothing.
    #[test]
    fn the_keys_help_calls_everywhere_work_on_every_screen() {
        let screens =
            [Screen::Quests, Screen::Quest, Screen::Settings, Screen::Help, Screen::Languages];
        let reach = |screen: Screen| {
            let mut app = app_in(Language::ENGLISH);
            match screen {
                Screen::Quest => press(&mut app, KeyCode::Enter),
                Screen::Settings => press(&mut app, KeyCode::Char('s')),
                Screen::Help => press(&mut app, KeyCode::Char('?')),
                Screen::Languages => press(&mut app, KeyCode::Char('l')),
                _ => {}
            }
            assert_eq!(app.screen, screen, "could not reach {screen:?}");
            app
        };
        for from in screens {
            for (key, lands) in
                [('l', Screen::Languages), ('s', Screen::Settings), ('?', Screen::Help)]
            {
                if from == lands {
                    continue;
                }
                let mut app = reach(from);
                press(&mut app, KeyCode::Char(key));
                assert_eq!(app.screen, lands, "{key} on {from:?} did not open {lands:?}");
                // And `q` from there goes back somewhere that is not the screen it was on.
                press(&mut app, KeyCode::Char('q'));
                assert_ne!(app.screen, lands, "q did not leave {lands:?} opened from {from:?}");
                assert!(!app.quit, "q from {lands:?} quit the program");
            }
        }
        // The first launch keeps its own flow: help from it goes back to it, `s` included,
        // because leaving it for the settings screen would skip writing the file.
        let mut app = app_in(Language::ENGLISH);
        app.screen = Screen::Welcome;
        press(&mut app, KeyCode::Char('?'));
        assert_eq!(app.screen, Screen::Help);
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Welcome);
        // Two screens deep: welcome, the language list, help, then `s`.
        press(&mut app, KeyCode::Char('l'));
        press(&mut app, KeyCode::Char('?'));
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Welcome, "s skipped the first launch");
        press(&mut app, KeyCode::Char('l'));
        press(&mut app, KeyCode::Char('s'));
        assert_eq!(app.screen, Screen::Languages, "s from the list skipped the first launch");
    }

    /// The one screen a reader who has never run nmtk sees. It has to say what this is, and it
    /// has to offer the language before anything else, because a Korean reader arrives at a
    /// screen of English and needs a way out of it that does not require reading English.
    #[test]
    fn the_first_launch_says_what_nmtk_is_and_offers_the_language_first() {
        for language in Language::ALL {
            let mut app = app_in(*language);
            app.screen = Screen::Welcome;
            let text = shot(&mut app, 80, 30);
            println!("\n===== welcome ({language}) =====\n{text}");
            let flat = text.replace(' ', "");
            assert!(flat.contains("nmtk"), "the program does not name itself:\n{text}");
            let network = t(Msg::WelcomeThree, *language).replace(' ', "");
            assert!(flat.contains(&network), "the offline promise is missing:\n{text}");
            let rows = text.lines().position(|line| line.contains("▸")).expect("a chosen row");
            assert!(rows > 0, "nothing is chosen:\n{text}");
            assert!(
                text.lines()
                    .nth(rows)
                    .unwrap()
                    .replace(' ', "")
                    .contains(&t(Msg::SettingsLanguage, *language).replace(' ', "")),
                "the first row is not the language:\n{text}"
            );
        }
    }

    #[test]
    fn enter_on_the_first_launch_writes_the_settings_and_opens_the_shelf() {
        let mut app = app_in(Language::ENGLISH);
        app.screen = Screen::Welcome;
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Quests, "Enter did not finish the setup");
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

    // ---- A quest that counts what the shell does to it --------------------------------------

    use std::sync::atomic::{AtomicUsize, Ordering as AtomicOrdering};
    use std::sync::{Arc, Mutex};

    use nmtk_kq::meta::{
        Category, Difficulty, KqId, KqMeta, Requirements, StageRole, StageSpec, Stamp, Version,
    };
    use nmtk_kq::session::{Action, Beat, Kq, KqSession, Reaction, RunState};
    use nmtk_kq::{Knob, Theme};

    const PROBE_STAGES: [StageSpec; 3] = [
        StageSpec::new("one", StageRole::Explain, Difficulty::Easy),
        StageSpec::new("two", StageRole::Run, Difficulty::Easy),
        StageSpec::new("three", StageRole::Recap, Difficulty::Easy),
    ];

    /// What the probe saw: how many sessions were closed, and the machine each was opened with.
    #[derive(Default)]
    struct Seen {
        closed: AtomicUsize,
        opened_with: Mutex<Vec<MachineProfile>>,
        /// Whether the probe says Enter runs something, the way a stage with knobs does.
        names_go: std::sync::atomic::AtomicBool,
    }

    struct Probe(Arc<Seen>, &'static str);

    impl Kq for Probe {
        fn meta(&self) -> KqMeta {
            KqMeta {
                id: KqId(self.1),
                version: Version::new(0, 4, 0),
                released: Stamp::new(2026, 1, 1, 0, 0, 0),
                updated: Stamp::new(2026, 1, 1, 0, 0, 0),
                category: Category::Systems,
                subcategory: "probe",
                difficulty: Difficulty::Easy,
                minutes: 1,
                needs: Requirements::ANY,
                stages: &PROBE_STAGES,
                tags: &[],
            }
        }
        fn title(&self, language: Language) -> &'static str {
            if language == Language::KOREAN { "탐침" } else { "Probe" }
        }
        fn summary(&self, _: Language) -> &'static str {
            "A quest that counts."
        }
        fn subcategory(&self, _: Language) -> &'static str {
            "probe"
        }
        fn stage_name(&self, key: &str, _: Language) -> &'static str {
            PROBE_STAGES.iter().find(|stage| stage.key == key).map_or("", |stage| stage.key)
        }
        fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession> {
            self.0.opened_with.lock().unwrap().push(*machine);
            Box::new(ProbeSession { seen: self.0.clone(), stage: 0, said: 1 })
        }
    }

    struct ProbeSession {
        seen: Arc<Seen>,
        stage: usize,
        said: usize,
    }

    impl KqSession for ProbeSession {
        fn stage(&self) -> usize {
            self.stage
        }
        fn go_to(&mut self, stage: usize) {
            self.stage = stage.min(2);
        }
        fn transcript(&self, _: Language) -> Vec<Beat> {
            (0..self.said).map(|n| Beat::say(format!("beat {n}"))).collect()
        }
        fn can_advance(&self) -> bool {
            self.said < 3
        }
        fn at_end(&self) -> bool {
            self.said >= 3
        }
        fn knobs(&self) -> &[Knob] {
            &[]
        }
        fn chosen_knob(&self) -> Option<usize> {
            None
        }
        fn on(&mut self, action: Action) -> Reaction {
            match action {
                Action::Go if self.said < 3 => {
                    self.said += 1;
                    Reaction::Handled
                }
                Action::Stage(stage) => {
                    self.go_to(stage);
                    self.said = 1;
                    Reaction::Handled
                }
                _ => Reaction::Ignored,
            }
        }
        fn tick(&mut self) {}
        fn run_state(&self) -> RunState {
            RunState::Running
        }
        fn render(&self, _: &mut Frame, _: ratatui::layout::Rect, _: Theme, _: Language) {}
        fn keys(&self, _: Language) -> Vec<(&'static str, &'static str)> {
            Vec::new()
        }
        fn go_name(&self, _: Language) -> Option<&'static str> {
            self.seen.names_go.load(AtomicOrdering::SeqCst).then_some("run it")
        }
        fn close(&mut self) {
            self.seen.closed.fetch_add(1, AtomicOrdering::SeqCst);
        }
    }

    fn probed(settings: Settings) -> (App, Arc<Seen>) {
        let seen = Arc::new(Seen::default());
        let catalogue = Catalogue::new(vec![
            Box::new(Probe(seen.clone(), "systems.probe-a")),
            Box::new(Probe(seen.clone(), "systems.probe-b")),
        ]);
        let machine =
            MachineProfile { logical_cores: 8, total_memory_bytes: 0, available_memory_bytes: 0 };
        (App::detached(catalogue, settings, machine), seen)
    }

    fn key(
        code: KeyCode,
        modifiers: KeyModifiers,
        kind: crossterm::event::KeyEventKind,
    ) -> KeyEvent {
        KeyEvent::new_with_kind(code, modifiers, kind)
    }

    /// `close()` stops a quest's threads, and every way out of a quest has to reach it — back,
    /// another quest, Ctrl+C, and the app simply going away after an error.
    #[test]
    fn every_way_out_of_a_quest_closes_it_exactly_once() {
        let (mut app, seen) = probed(Settings::default());
        let closed = || seen.closed.load(AtomicOrdering::SeqCst);

        press(&mut app, KeyCode::Enter);
        assert!(app.open.is_some());
        press(&mut app, KeyCode::Char('q'));
        assert_eq!(closed(), 1, "back did not close the quest");

        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Esc);
        assert_eq!(closed(), 2, "Esc did not close the quest");

        press(&mut app, KeyCode::Enter);
        app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
        assert!(app.quit, "Ctrl+C did not ask the program to end");
        // Closed by the drop that follows giving the terminal back, not by the key itself.
        assert_eq!(closed(), 2, "Ctrl+C closed the quest before the terminal was given back");
        drop(app);
        assert_eq!(seen.closed.load(AtomicOrdering::SeqCst), 3, "Ctrl+C never closed the quest");

        let (mut app, seen) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        drop(app);
        assert_eq!(seen.closed.load(AtomicOrdering::SeqCst), 1, "dropping the app leaked it");
    }

    /// A signal and Ctrl+C used to join the quest's threads while the terminal was still raw on
    /// the alternate screen, and a second signal during that join ended the process right there.
    /// The terminal has to be given back before the quest is closed, on every deliberate way out.
    #[test]
    fn a_signal_or_ctrl_c_gives_the_terminal_back_before_closing_the_quest() {
        type Exit = fn(&mut App, &AtomicBool) -> io::Result<()>;
        let by_signal: Exit = |app, stop| {
            stop.store(true, Ordering::SeqCst);
            stopped_by_signal(app, stop).map_or(Ok(()), Err)
        };
        let by_ctrl_c: Exit = |app, stop| {
            app.on_key(KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL));
            stopped_by_signal(app, stop).map_or(Ok(()), Err)
        };
        for (how, exit, interrupted) in [("signal", by_signal, true), ("Ctrl+C", by_ctrl_c, false)]
        {
            let (mut app, seen) = probed(Settings::default());
            press(&mut app, KeyCode::Enter);
            assert!(app.open.is_some());
            let stop = AtomicBool::new(false);
            let result = exit(&mut app, &stop);
            assert!(app.quit, "{how} did not ask the program to end");
            assert_eq!(seen.closed.load(AtomicOrdering::SeqCst), 0, "{how} closed the quest early");

            let closed_at_give_back = Arc::new(AtomicUsize::new(usize::MAX));
            let give_back = {
                let (seen, at) = (seen.clone(), closed_at_give_back.clone());
                move || {
                    at.store(seen.closed.load(AtomicOrdering::SeqCst), AtomicOrdering::SeqCst);
                    Ok(())
                }
            };
            let ended = wind_down(app, result, give_back);
            assert_eq!(
                closed_at_give_back.load(AtomicOrdering::SeqCst),
                0,
                "{how}: the quest was closed before the terminal was given back"
            );
            assert_eq!(seen.closed.load(AtomicOrdering::SeqCst), 1, "{how}: never closed");
            // The exit code and the message on a signal read this, and they are unchanged.
            assert_eq!(
                ended.err().map(|error| error.kind()),
                interrupted.then_some(io::ErrorKind::Interrupted),
                "{how} ended the wrong way"
            );
        }
    }

    /// Checks the line under the conversation against what Enter really does, and says what was
    /// looked at: whether the quest named Enter, and where its run was.
    fn prompt_matches_enter(app: &App, language: Language, at: &str) -> (bool, RunState) {
        let quest = app.open.as_ref().expect("a quest is open");
        let session = &quest.session;
        let beat = quest::closing_beat(quest, language);
        let again =
            [t(Msg::ConversationRunAgain, language), t(Msg::ConversationRunAgainHere, language)];
        let named = session.go_name(language).is_some();
        if named {
            let beat =
                beat.unwrap_or_else(|| panic!("{at}: Enter runs it again, and nothing says so"));
            assert_eq!(beat.voice, nmtk_kq::session::Voice::Ask, "{at}: {beat:?}");
            let last = session.stage() + 1 >= quest.stages.len();
            assert_eq!(beat.text, again[usize::from(last)], "{at}: the prompt misleads");
        } else if let Some(beat) = beat {
            assert!(!again.contains(&beat.text.as_str()), "{at}: {beat:?} with nothing to run");
        }
        (named, session.run_state())
    }

    /// At the end of a stage with knobs Enter runs the work again, and the quest takes the key, so
    /// the shell never walks on. The line under the conversation said "Press Enter to carry on",
    /// and a reader pressing it went round the same run while waiting to be carried on.
    ///
    /// Walked through every stage of every quest on the shelf, turning a value wherever there is
    /// one, so the stages whose runs go on for minutes are seen with their work still going.
    #[test]
    fn the_prompt_on_a_stage_with_knobs_says_what_enter_does_and_that_tab_walks_on() {
        let language = Language::ENGLISH;
        let titles: Vec<&str> =
            app_in(language).visible().iter().map(|q| q.title(language)).collect();
        let mut named_while_running = Vec::new();
        let mut named_when_still = Vec::new();
        for title in titles {
            let mut app = opened_named(language, title);
            let stages = app.open.as_ref().expect("a quest is open").stages.len();
            for stage in 0..stages {
                let at = format!("{title}, stage {}", stage + 1);
                // Carried on the way a reader does, until the stage is over, or until it is
                // waiting on a run longer than a test should.
                let mut waiting_since = std::time::Instant::now();
                for _ in 0..400 {
                    let quest = app.open.as_mut().expect("a quest is open");
                    quest.session.tick();
                    let (at_end, can, running) = (
                        quest.session.at_end(),
                        quest.session.can_advance(),
                        quest.session.run_state() == RunState::Running,
                    );
                    let (named, state) = prompt_matches_enter(&app, language, &at);
                    if named {
                        let seen = if state == RunState::Running {
                            &mut named_while_running
                        } else {
                            &mut named_when_still
                        };
                        seen.push(at.clone());
                    }
                    if at_end && !running {
                        break;
                    }
                    if can && !at_end {
                        press(&mut app, KeyCode::Enter);
                        waiting_since = std::time::Instant::now();
                    } else if waiting_since.elapsed() > Duration::from_secs(2) {
                        break;
                    } else {
                        std::thread::sleep(Duration::from_millis(10));
                    }
                }
                // Turning a value is what makes Enter run it again in the middle of a run.
                let tunable = !app.open.as_ref().expect("open").session.knobs().is_empty();
                if tunable {
                    press(&mut app, KeyCode::Right);
                    let (named, state) = prompt_matches_enter(&app, language, &at);
                    if named && state == RunState::Running {
                        named_while_running.push(at.clone());
                    }
                }
                press(&mut app, KeyCode::Tab);
            }
        }
        println!("named while running: {named_while_running:?}");
        println!("named when still: {named_when_still:?}");
        assert!(!named_while_running.is_empty(), "no stage was seen naming Enter mid-run");
        assert!(
            named_when_still.iter().any(|at| at.starts_with("Ledger models")),
            "the ledger quest's tuning stage never reached its end: {named_when_still:?}"
        );
    }

    /// The same on screen, in both languages: the prompt at the end of the ledger quest's tuning
    /// stage, and the key bar beside it.
    #[test]
    fn the_end_of_a_tuning_stage_shows_what_enter_does_in_both_languages() {
        for language in Language::ALL {
            let mut app = opened_named(*language, t(Msg::MenuLedgers, *language));
            // The fourth stage is the one with values to turn.
            for _ in 0..3 {
                press(&mut app, KeyCode::Tab);
            }
            for _ in 0..40 {
                press(&mut app, KeyCode::Enter);
                if app.open.as_ref().expect("open").session.at_end() {
                    break;
                }
            }
            let session = &app.open.as_ref().expect("open").session;
            assert!(session.at_end() && session.go_name(*language).is_some());
            for (width, height) in [(80u16, 24u16), (100, 30)] {
                shot(&mut app, width, height);
                let text = shot(&mut app, width, height);
                println!("\n===== the end of the tuning stage ({language}, {width}) =====\n{text}");
                // The conversation alone, read as one run of text, so a sentence wrapped across
                // lines is still one sentence.
                let (talk, _) = nmtk_kq::theme::split(width);
                let conversation: String = text
                    .lines()
                    .map(|line| line.chars().take(usize::from(talk)).collect::<String>())
                    .collect::<String>()
                    .replace([' ', '│'], "");
                let again = t(Msg::ConversationRunAgain, *language).replace(' ', "");
                assert!(conversation.contains(&again), "the prompt is not on screen:\n{text}");
                let carry = t(Msg::ConversationWaiting, *language).replace(' ', "");
                assert!(!conversation.contains(&carry), "Enter is said to carry on:\n{text}");
                let bar = text.lines().last().unwrap_or_default().replace(' ', "");
                let named = session_go_name(&app, *language).replace(' ', "");
                assert!(bar.contains(&format!("Enter{named}")), "the key bar disagrees: {bar}");
            }
        }
    }

    fn session_go_name(app: &App, language: Language) -> String {
        let session = &app.open.as_ref().expect("open").session;
        session.go_name(language).unwrap_or_default().to_string()
    }

    /// On a last stage there is no stage for Tab to walk on to, so the prompt does not offer one;
    /// and a stage that names nothing keeps the ordinary prompt.
    #[test]
    fn the_prompt_offers_tab_only_where_there_is_a_next_stage() {
        let (mut app, seen) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        let beat = |app: &App| {
            quest::closing_beat(app.open.as_ref().expect("open"), Language::ENGLISH)
                .map(|beat| beat.text)
        };
        let english = |message| t(message, Language::ENGLISH).to_string();
        assert_eq!(beat(&app), Some(english(Msg::ConversationWaitingWhileRunning)));
        seen.names_go.store(true, AtomicOrdering::SeqCst);
        assert_eq!(beat(&app), Some(english(Msg::ConversationRunAgain)));
        press(&mut app, KeyCode::Char('3'));
        assert_eq!(app.open.as_ref().expect("open").session.stage(), 2);
        assert_eq!(beat(&app), Some(english(Msg::ConversationRunAgainHere)));
        let text = shot(&mut app, 80, 24);
        let bar = text.lines().last().unwrap_or_default();
        assert!(bar.contains("Enter run it"), "{bar}");
        assert!(!bar.contains("Tab next stage"), "the last stage offers a next one: {bar}");
    }

    /// A failure to give the terminal back is still reported, after the quest has been closed.
    #[test]
    fn a_terminal_that_will_not_come_back_is_reported_and_the_quest_still_closes() {
        let (mut app, seen) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        let ended = wind_down(app, Ok(()), || Err(io::Error::other("no terminal")));
        assert_eq!(seen.closed.load(AtomicOrdering::SeqCst), 1);
        assert_eq!(ended.err().map(|error| error.kind()), Some(io::ErrorKind::Other));
    }

    /// The thread count in settings used to reach no quest at all: each was opened with the bare
    /// machine and took every core but one.
    #[test]
    fn the_thread_count_in_settings_reaches_the_quest() {
        let (mut app, seen) = probed(Settings { worker_threads: 3, ..Settings::default() });
        press(&mut app, KeyCode::Enter);
        let machine = seen.opened_with.lock().unwrap()[0];
        assert_eq!(machine.default_worker_threads(), 3, "the quest was not told 3 threads");

        let (mut app, seen) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        let machine = seen.opened_with.lock().unwrap()[0];
        assert_eq!(machine.default_worker_threads(), 7, "auto is every core but one");
    }

    #[test]
    fn a_thread_count_from_a_bigger_machine_is_shown_and_stepped_as_this_one() {
        let (mut app, _) = probed(Settings { worker_threads: 64, ..Settings::default() });
        press(&mut app, KeyCode::Char('s'));
        app.settings_index = 1;
        let text = shot(&mut app, 80, 24);
        assert!(text.contains("[ 8 ]"), "64 is shown on an 8-core machine:\n{text}");
        press(&mut app, KeyCode::Left);
        assert_eq!(app.settings.worker_threads, 7, "left from 64 on 8 cores is 7");
    }

    #[test]
    fn changing_threads_inside_a_quest_says_when_it_applies() {
        let (mut app, _) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        press(&mut app, KeyCode::Char('s'));
        app.settings_index = 1;
        press(&mut app, KeyCode::Left);
        assert_eq!(app.status, Some(Msg::SettingsThreadsNextQuest));
    }

    /// Held arrows keep moving on terminals that report repeats; nothing else repeats, and a
    /// release never acts.
    #[test]
    fn a_held_arrow_repeats_and_a_held_enter_does_not() {
        use crossterm::event::KeyEventKind::{Release, Repeat};
        let (mut app, seen) = probed(Settings::default());
        app.on_key(key(KeyCode::Down, KeyModifiers::NONE, Repeat));
        assert_eq!(app.list_index, 1, "a repeated ↓ did not move");
        app.on_key(key(KeyCode::Down, KeyModifiers::NONE, Release));
        assert_eq!(app.list_index, 1, "a release moved the list");
        app.on_key(key(KeyCode::Enter, KeyModifiers::NONE, Repeat));
        assert!(app.open.is_none(), "a repeated Enter opened a quest");
        app.on_key(key(KeyCode::Enter, KeyModifiers::NONE, Release));
        assert!(app.open.is_none(), "a released Enter opened a quest");
        press(&mut app, KeyCode::Enter);
        assert_eq!(seen.opened_with.lock().unwrap().len(), 1);
    }

    #[test]
    fn the_title_bar_follows_the_language_inside_a_quest() {
        let (mut app, _) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        assert!(shot(&mut app, 80, 24).lines().next().unwrap().contains("Probe"));
        press(&mut app, KeyCode::Char('l'));
        press(&mut app, KeyCode::Down);
        press(&mut app, KeyCode::Enter);
        assert_eq!(app.screen, Screen::Quest);
        let title = shot(&mut app, 80, 24).lines().next().unwrap().replace(' ', "");
        assert!(title.contains("탐침"), "the title stayed in English: {title:?}");
    }

    /// A terminal shrunk below 80x24 in the middle of a quest shows a message, the quest keeps
    /// its place, and keys pressed blind do nothing — except the way out.
    #[test]
    fn a_terminal_shrunk_mid_quest_holds_the_quest_until_it_grows() {
        let (mut app, seen) = probed(Settings::default());
        press(&mut app, KeyCode::Enter);
        let text = shot(&mut app, 60, 20);
        assert!(text.contains("80x24"), "{text}");
        for code in [KeyCode::Enter, KeyCode::Tab, KeyCode::Char('l'), KeyCode::Char('?')] {
            press(&mut app, code);
        }
        let session = &app.open.as_ref().expect("the quest is still open").session;
        assert_eq!(session.stage(), 0, "a blind Tab changed the stage");
        assert_eq!(session.transcript(Language::ENGLISH).len(), 1, "a blind Enter said more");
        assert_eq!(app.screen, Screen::Quest);
        let text = shot(&mut app, 80, 24);
        assert!(text.contains("beat 0"), "the quest did not come back:\n{text}");
        shot(&mut app, 40, 10);
        press(&mut app, KeyCode::Char('q'));
        assert_eq!(app.screen, Screen::Quests, "q does not go back on a small screen");
        assert_eq!(seen.closed.load(AtomicOrdering::SeqCst), 1);
        // Tiny terminals, and the Korean message on one, never panic or lose the size.
        for (w, h) in [(1u16, 1u16), (10, 3), (40, 10), (79, 24), (80, 23)] {
            for language in Language::ALL {
                app.settings.language = *language;
                let text = shot(&mut app, w, h);
                if h >= 4 && w >= 5 {
                    assert!(text.contains(&format!("{w}x{h}")), "{w}x{h}:\n{text}");
                }
            }
        }
    }

    /// Colour off means no colour on any cell of any screen, not only on the four state colours.
    #[test]
    fn colour_off_draws_no_colour_on_any_screen() {
        for language in Language::ALL {
            let shelf = app_in(*language).visible().len();
            // Every shell screen, and every quest's opening screen, which is where the shell and
            // the quest's own panel meet.
            let screens = [
                Screen::Welcome,
                Screen::Quests,
                Screen::Settings,
                Screen::Help,
                Screen::Languages,
            ]
            .into_iter()
            .map(|screen| (screen, 0))
            .chain((0..shelf).map(|at| (Screen::Quest, at)));
            for (screen, at) in screens {
                let mut app = app_in(*language);
                app.settings.colour = false;
                app.list_index = at;
                if screen == Screen::Quest {
                    press(&mut app, KeyCode::Enter);
                }
                app.screen = screen;
                let mut terminal = Terminal::new(TestBackend::new(100, 30)).expect("backend");
                terminal.draw(|frame| draw(frame, &mut app)).expect("draw");
                let buffer = terminal.backend().buffer();
                for cell in buffer.content() {
                    assert_eq!(
                        (cell.fg, cell.bg),
                        (ratatui::style::Color::Reset, ratatui::style::Color::Reset),
                        "{screen:?} ({language}) drew {:?} in colour with colour off",
                        cell.symbol()
                    );
                }
            }
        }
    }

    /// The first `n` characters of `text`.
    fn head(text: &str, n: usize) -> String {
        text.chars().take(n).collect()
    }

    /// The last `n` characters of `text`: the part of a sentence that goes first when a box is
    /// too small for it.
    fn tail(text: &str, n: usize) -> String {
        let count = text.chars().count();
        text.chars().skip(count.saturating_sub(n)).collect()
    }

    /// Each screen at the smallest size and a large one, in both languages, keeps what it exists
    /// to show: nothing that matters falls off the bottom or the edge.
    #[test]
    fn every_screen_keeps_its_content_at_the_smallest_and_a_large_size() {
        for language in Language::ALL {
            let say = |message: Msg| t(message, *language).replace(' ', "");
            for (width, height) in [(80u16, 24u16), (200, 60)] {
                // The first launch: every setting row and the sentence explaining the chosen one.
                let mut app = app_in(*language);
                app.screen = Screen::Welcome;
                let flat = shot(&mut app, width, height).replace(' ', "");
                for item in SettingItem::ALL {
                    assert!(flat.contains(&say(item.title())), "welcome lost {item:?}:\n{flat}");
                }
                let about = say(SettingItem::Language.about());
                assert!(flat.contains(&head(&about, 12)), "welcome lost its hint:\n{flat}");

                // Help: every key label, and the whole promise at the bottom.
                let mut app = app_in(*language);
                press(&mut app, KeyCode::Char('?'));
                let flat = shot(&mut app, width, height).replace([' ', '│'], "");
                for message in [Msg::KeyScroll, Msg::KeyReset, Msg::KeyQuit, Msg::HelpInAQuest] {
                    assert!(flat.contains(&say(message)), "help lost {message:?}:\n{flat}");
                }
                let promise = say(Msg::HelpOffline);
                let end = tail(&promise, 6);
                assert!(
                    flat.contains(&end),
                    "help cut its promise short ({language}, {width}):\n{flat}"
                );

                // The shelf, with the chosen quest finished: the fit sentence still ends on screen.
                let mut app = app_in(*language);
                let id = app.visible()[0].meta().id.to_string();
                app.settings.remember_finished(&id);
                let flat = shot(&mut app, width, height).replace([' ', '│'], "");
                let fit = say(Msg::FitRecommended);
                let end = tail(&fit, 3);
                assert!(flat.contains(&end), "the fit fell off a finished quest:\n{flat}");
                let done = say(Msg::QuestFinished);
                assert!(flat.contains(&head(&done, 6)), "finished lost:\n{flat}");
                assert!(flat.contains(&tail(&done, 3)), "finished cut short:\n{flat}");

                // Settings and languages: every row.
                let mut app = app_in(*language);
                press(&mut app, KeyCode::Char('s'));
                let flat = shot(&mut app, width, height).replace(' ', "");
                for item in SettingItem::ALL {
                    assert!(flat.contains(&say(item.title())), "settings lost {item:?}");
                }
                press(&mut app, KeyCode::Char('l'));
                let text = shot(&mut app, width, height);
                assert!(text.contains("English") && text.contains('한'), "a language is missing");

                // A quest: the stage strip, the first beat's prompt, and the way out.
                let mut app = app_in(*language);
                press(&mut app, KeyCode::Enter);
                let text = shot(&mut app, width, height);
                let flat = text.replace(' ', "");
                assert!(flat.contains("1/"), "the stage strip is missing:\n{text}");
                let bar = text.lines().last().unwrap_or_default().replace(' ', "");
                assert!(bar.contains(&say(Msg::KeyBack)), "the way out is missing: {bar}");
            }
        }
    }

    /// A launch that found part of the settings file unreadable says so on the shelf, whole, at
    /// the smallest screen and in both languages, down to where the file as it was has gone.
    #[test]
    fn a_damaged_settings_file_is_mentioned_whole_on_the_shelf() {
        use nmtk_core::settings::Damage;
        for damage in [Damage::KeptAs("settings.toml.bad".into()), Damage::NotKept] {
            for language in Language::ALL {
                let mut app = app_in(*language);
                app.status = Some(app::damage_notice(&damage));
                let text = shot(&mut app, 80, 24);
                println!("\n===== {damage:?} ({language}) =====\n{text}");
                let said = t(app::damage_notice(&damage), *language).replace(' ', "");
                let bottom: String = text
                    .lines()
                    .rev()
                    .skip(1)
                    .take(2)
                    .collect::<Vec<_>>()
                    .into_iter()
                    .rev()
                    .collect::<String>()
                    .replace(' ', "");
                assert_eq!(bottom, said, "the notice is not said whole:\n{text}");
                // The shelf is still there above it.
                let version = format!("v{}", app.visible()[app.list_index].meta().version);
                assert!(text.contains(&version), "the notice pushed the shelf away:\n{text}");
                // The next key clears it, as every answer on this screen is cleared.
                press(&mut app, KeyCode::Down);
                assert_eq!(app.status, None);
            }
        }
    }

    /// The key bar measures its separators in cells. Counted in bytes, the two-byte dot cost an
    /// extra cell per gap and a key that fitted was dropped.
    #[test]
    fn the_key_bar_fills_the_row_it_has() {
        let keys: Vec<(&str, String)> = (0..6).map(|_| ("k", "abcdefgh".to_string())).collect();
        // Six pairs of 10 cells and five gaps of 5: 1 + 60 + 25 = 86 cells exactly.
        let mut terminal = Terminal::new(TestBackend::new(86, 1)).expect("backend");
        terminal
            .draw(|frame| {
                let area = frame.area();
                chrome::key_bar(frame, area, Theme::new(false), &keys)
            })
            .expect("draw");
        let row: String =
            terminal.backend().buffer().content().iter().map(|c| c.symbol()).collect();
        assert_eq!(row.matches("abcdefgh").count(), 6, "a key that fits was dropped: {row:?}");
    }
}
