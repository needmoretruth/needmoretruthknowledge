//! The key list, and the one promise worth repeating: nothing here goes online.
//!
//! Every key on this screen does something. A help screen that names a key which does nothing is
//! worse than no help screen, because the reader who tries it concludes the program is broken.

use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use nmtk_kq::Theme;

/// The keys, grouped by where they work. `None` in place of a key starts a new group.
///
/// A key that works and is not here is the same failure as a key that is here and does nothing:
/// the reader who never finds Space concludes a run cannot be paused.
const KEYS: [(Option<&str>, Msg); 19] = [
    (None, Msg::HelpEverywhere),
    (Some("l"), Msg::KeyLanguage),
    (Some("s"), Msg::KeySettings),
    (Some("?"), Msg::KeyHelp),
    (Some("q  Esc"), Msg::KeyBack),
    (None, Msg::HelpOnTheShelf),
    (Some("↑ ↓  j k"), Msg::KeyMove),
    (Some("Enter"), Msg::KeyOpen),
    (Some("o"), Msg::LabelSort),
    (Some("f"), Msg::LabelFilter),
    (None, Msg::HelpInAQuest),
    (Some("Enter"), Msg::KeyContinue),
    (Some("Tab"), Msg::KeyStage),
    (Some("↑ ↓"), Msg::KeyChoose),
    (Some("← →"), Msg::KeyChange),
    (Some("0-9"), Msg::KeyType),
    (Some("Space"), Msg::KeyPause),
    (Some("r"), Msg::KeyReset),
    (Some("PgUp PgDn"), Msg::KeyScroll),
];

pub fn render(frame: &mut Frame, area: Rect, language: nmtk_core::Language, theme: Theme) {
    let [keys_area, note_area] =
        Layout::vertical([Constraint::Min(KEYS.len() as u16 + 2), Constraint::Length(3)])
            .areas(area);

    let rows: Vec<Line> = KEYS
        .iter()
        .map(|(key, label)| match key {
            Some(key) => Line::from(vec![
                Span::styled(format!("  {}", nmtk_kq::text::column(key, 12)), theme.heading()),
                Span::styled(t(*label, language), theme.plain()),
            ]),
            // A group heading. The blank column keeps the keys under it lined up.
            None => Line::from(Span::styled(t(*label, language), theme.muted())),
        })
        .collect();
    frame.render_widget(
        Paragraph::new(rows).block(theme.titled_panel(t(Msg::HelpTitle, language))),
        keys_area,
    );

    let note =
        Paragraph::new(t(Msg::HelpOffline, language)).style(theme.good()).wrap(Wrap { trim: true });
    frame.render_widget(note.block(theme.panel()), note_area);
}
