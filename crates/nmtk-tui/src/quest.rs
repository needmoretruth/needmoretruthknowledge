//! A quest a reader has opened: the stage strip, the explanation, and the quest's own panel.

use nmtk_core::Language;
use nmtk_i18n::{Msg, t};
use nmtk_kq::meta::StageKind;
use nmtk_kq::theme::{EXPLAIN_PERCENT, State, Theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::OpenQuest;

pub fn render(frame: &mut Frame, area: Rect, quest: &OpenQuest, theme: Theme, language: Language) {
    let [strip, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    render_strip(frame, strip, quest, theme, language);

    let [explain_area, run_area] = Layout::horizontal([
        Constraint::Percentage(EXPLAIN_PERCENT),
        Constraint::Percentage(100 - EXPLAIN_PERCENT),
    ])
    .areas(body);

    let explanation = Paragraph::new(quest.session.explain(language))
        .style(theme.plain())
        .wrap(Wrap { trim: true })
        .block(theme.titled_panel(t(stage_name(quest.session.stage()), language)));
    frame.render_widget(explanation, explain_area);

    quest.session.render(frame, run_area, theme, language);
}

/// `1 Brief · 2 Run · 3 Tune …`, with the stage in front marked.
fn render_strip(
    frame: &mut Frame,
    area: Rect,
    quest: &OpenQuest,
    theme: Theme,
    language: Language,
) {
    let here = quest.session.stage();
    let mut spans = vec![Span::raw(" ")];
    for (index, stage) in quest.stages.iter().enumerate() {
        if index > 0 {
            spans.push(Span::styled("  ·  ", theme.muted()));
        }
        let current = *stage == here;
        spans.push(Span::styled(
            format!("{} ", stage.digit()),
            if current { theme.state(State::Chosen) } else { theme.muted() },
        ));
        spans.push(Span::styled(
            t(stage_name(*stage), language),
            if current { theme.heading() } else { theme.muted() },
        ));
    }
    if let Some(state) = quest.session.run_state().state() {
        spans.push(Span::styled(format!("   {}", state.mark()), theme.state(state)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

pub fn stage_name(stage: StageKind) -> Msg {
    match stage {
        StageKind::Brief => Msg::StageBrief,
        StageKind::Run => Msg::StageRun,
        StageKind::Tune => Msg::StageTune,
        StageKind::Break => Msg::StageBreak,
        StageKind::Recap => Msg::StageRecap,
    }
}
