//! The key list, and the one promise worth repeating: nothing here goes online.

use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::theme::Theme;

const KEYS: [(&str, Msg); 8] = [
    ("↑ ↓  j k", Msg::KeyMove),
    ("Enter", Msg::KeyOpen),
    ("Space", Msg::KeyPause),
    ("r", Msg::KeyReset),
    ("Tab", Msg::KeyPanel),
    ("l", Msg::KeyLanguage),
    ("s", Msg::KeySettings),
    ("q  Esc", Msg::KeyBack),
];

pub fn render(frame: &mut Frame, area: Rect, language: nmtk_core::Language, theme: Theme) {
    let [keys_area, note_area] =
        Layout::vertical([Constraint::Min(KEYS.len() as u16 + 2), Constraint::Length(4)])
            .areas(area);

    let rows: Vec<Line> = KEYS
        .iter()
        .map(|(key, label)| {
            Line::from(vec![
                Span::styled(format!("{key:<12}"), theme.accent()),
                Span::styled(t(*label, language), theme.plain()),
            ])
        })
        .collect();
    frame.render_widget(
        Paragraph::new(rows).block(theme.titled_panel(t(Msg::HelpTitle, language))),
        keys_area,
    );

    let note = Paragraph::new(t(Msg::HelpOffline, language))
        .style(theme.ok())
        .wrap(Wrap { trim: true });
    frame.render_widget(note.block(theme.panel()), note_area);
}
