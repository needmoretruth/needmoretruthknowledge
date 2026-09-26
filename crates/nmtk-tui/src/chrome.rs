//! The frame every screen sits in: one title row on top, one key row at the bottom.

use nmtk_core::Language;
use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Alignment, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use nmtk_kq::Theme;
use nmtk_kq::text::width;

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
///
/// A pair that will not fit is left out whole. The alternative — letting the row run past the
/// edge — cuts a key's name in half and leaves the reader looking at `q 뒤`, which is how the last
/// version hid the way out of the program on a narrow terminal.
pub fn key_bar(frame: &mut Frame, area: Rect, theme: Theme, keys: &[(&str, String)]) {
    const GAP: &str = "  ·  ";
    let room = area.width as usize;
    let mut spans = vec![Span::raw(" ")];
    let mut used = 1;
    for (key, label) in keys {
        let pair = width(key) + 1 + width(label);
        // Cells, not bytes: the dot is two bytes and one cell, and counting it as two dropped
        // a key that fitted whenever the row came within a cell per gap of the edge.
        let gap = if used > 1 { width(GAP) } else { 0 };
        // Stop rather than skip. Carrying on would keep whichever later key happened to be short
        // enough, which is how "? help" came to vanish at 80 columns while "v version" stayed.
        if used + gap + pair > room {
            break;
        }
        if gap > 0 {
            spans.push(Span::styled(GAP, theme.muted()));
        }
        spans.push(Span::styled((*key).to_string(), theme.heading()));
        spans.push(Span::raw(" "));
        spans.push(Span::styled(label.clone(), theme.muted()));
        used += gap + pair;
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

/// The whole screen when the terminal is smaller than nmtk can draw on.
pub fn too_small(frame: &mut Frame, theme: Theme, language: Language) {
    let area = frame.area();
    // Wrapped to the terminal, because the terminal this is shown on is by definition narrow: a
    // Korean sentence 50 cells wide on a 40-column terminal lost its end, and its end is the part
    // that says what to do.
    let mut lines = vec![Line::from("")];
    for row in nmtk_kq::text::wrap(t(Msg::TerminalTooSmall, language), area.width as usize) {
        lines.push(Line::from(Span::styled(row, theme.bad())));
    }
    lines.push(Line::from(Span::styled(format!("{}x{}", area.width, area.height), theme.muted())));
    let message = Paragraph::new(lines).alignment(Alignment::Center);
    frame.render_widget(message, area);
}
