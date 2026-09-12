//! The frame every screen sits in: one title row on top, one key row at the bottom.

use nmtk_core::Language;
use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use nmtk_kq::Theme;

/// `NMTK · <screen>` on the left, the language tag on the right.
pub fn title_bar(frame: &mut Frame, area: Rect, theme: Theme, title: &str, language: Language) {
    let left = Line::from(vec![
        Span::styled(" NMTK", theme.heading()),
        Span::styled("  ·  ", theme.muted()),
        Span::styled(title.to_string(), theme.plain()),
    ]);
    let right = Line::from(Span::styled(format!("{} ", language.tag()), theme.muted()))
        .alignment(Alignment::Right);
    frame.render_widget(Paragraph::new(left), area);
    frame.render_widget(Paragraph::new(right), area);
}

/// The keys that work on this screen, written as `key label` pairs separated by dots.
pub fn key_bar(frame: &mut Frame, area: Rect, theme: Theme, keys: &[(&str, String)]) {
    let mut spans = vec![Span::raw(" ")];
    for (index, (key, label)) in keys.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ·  ", theme.muted()));
        }
        spans.push(Span::styled((*key).to_string(), theme.heading()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label.clone(), theme.muted()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The whole screen when the terminal is smaller than nmtk can draw on.
pub fn too_small(frame: &mut Frame, theme: Theme, language: Language) {
    let area = frame.area();
    let message = Paragraph::new(vec![
        Line::from(""),
        Line::from(Span::styled(t(Msg::TerminalTooSmall, language), theme.bad())),
        Line::from(Span::styled(format!("{}x{}", area.width, area.height), theme.muted())),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(message, area);
}
