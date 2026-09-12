//! The shape every subject screen shares: the explanation on the left, the thing running on the
//! right. The engines fill the right-hand side; this module owns the frame they sit in.

use nmtk_i18n::t;
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::Subject;
use crate::theme::{EXPLAIN_PERCENT, Theme};

/// Splits a subject screen into its two panels: explanation, then the run.
pub fn split(area: Rect) -> [Rect; 2] {
    Layout::horizontal([
        Constraint::Percentage(EXPLAIN_PERCENT),
        Constraint::Percentage(100 - EXPLAIN_PERCENT),
    ])
    .areas(area)
}

pub fn render(
    frame: &mut Frame,
    area: Rect,
    subject: Subject,
    language: nmtk_core::Language,
    theme: Theme,
) {
    let [explain_area, run_area] = split(area);

    let explanation = Paragraph::new(t(subject.about(), language))
        .style(theme.plain())
        .wrap(Wrap { trim: true })
        .block(theme.titled_panel(t(subject.title(), language)));
    frame.render_widget(explanation, explain_area);

    frame.render_widget(theme.panel(), run_area);
}
