//! A quest a reader has opened: the stage strip, the conversation, and the quest's own panel.

use nmtk_core::Language;
use nmtk_i18n::{Msg, t};
use nmtk_kq::meta::{StageRole, StageSpec};
use nmtk_kq::session::{Kq, RunState};
use nmtk_kq::theme::{self, State, Theme};
use nmtk_kq::{conversation, text};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, OpenQuest};

/// Draws the open quest. Returns how far the conversation could still be scrolled, so the app can
/// keep its offset honest.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    quest: &OpenQuest,
    definition: Option<&dyn Kq>,
    app: &App,
    theme: Theme,
    language: Language,
) -> usize {
    let [strip, body] = Layout::vertical([Constraint::Length(1), Constraint::Min(3)]).areas(area);
    render_strip(frame, strip, quest, definition, theme, language);

    let (talk, run) = theme::split(body.width);
    let [talk_area, run_area] =
        Layout::horizontal([Constraint::Length(talk), Constraint::Length(run)]).areas(body);

    let stage = current_stage(quest);
    let name = stage_name(definition, stage, language);
    let block = theme.titled_panel(name);
    let inner = block.inner(talk_area);
    frame.render_widget(block, talk_area);

    let mut beats = quest.session.transcript(language);
    // The shell walks on at the end of a stage, so Enter still carries the reader forward there.
    let walk_on = quest.session.at_end() && quest.session.stage() + 1 < quest.stages.len();
    if quest.session.can_advance() || walk_on {
        // A reader who is offered Enter while work is visibly running does not know whether
        // pressing it cuts the run short, so they sit and wait. One of them waited ninety seconds.
        let waiting = if quest.session.run_state() == RunState::Running {
            Msg::ConversationWaitingWhileRunning
        } else {
            Msg::ConversationWaiting
        };
        beats.push(nmtk_kq::Beat::ask(t(waiting, language).to_string()));
    } else if quest.session.run_state() == RunState::Running {
        // A conversation that has stopped and says nothing reads as a program that has hung.
        beats.push(nmtk_kq::Beat::event(t(Msg::ConversationWorking, language).to_string()));
    } else if quest.session.at_end() {
        // The last stage of the last quest used to end in silence with "Enter continue" still in
        // the key bar, so the reader pressed it until they concluded the program was broken.
        beats.push(nmtk_kq::Beat::say(t(Msg::ConversationFinished, language).to_string()));
    }
    let furthest = conversation::render(frame, inner, theme, &beats, app.transcript_scroll);

    quest.session.render(frame, run_area, theme, language);
    furthest
}

/// Where the reader is: one dot per stage with this one filled, then its number and name.
///
/// Names side by side looked better with three stages and stopped fitting at six. Dots scale to
/// however many stages a quest declares, and the name of the one you are on is the only name that
/// matters while you are on it.
fn render_strip(
    frame: &mut Frame,
    area: Rect,
    quest: &OpenQuest,
    definition: Option<&dyn Kq>,
    theme: Theme,
    language: Language,
) {
    let here = quest.session.stage();
    let count = quest.stages.len();
    let mut spans = vec![Span::raw(" ")];
    for index in 0..count {
        let current = index == here;
        spans.push(Span::styled(
            if current { "\u{25cf}" } else { "\u{b7}" },
            if current { theme.state(State::Chosen) } else { theme.muted() },
        ));
    }
    spans.push(Span::styled(format!("   {}/{}  ", here + 1, count), theme.muted()));
    let name = stage_name(definition, current_stage(quest), language);
    // Whatever is left after the dots and the counter, minus a column for the run mark.
    let room = (area.width as usize).saturating_sub(count + 9 + 4);
    spans.push(Span::styled(text::truncate(name, room), theme.heading()));
    if let Some(state) = quest.session.run_state().state() {
        spans.push(Span::styled(format!("  {}", state.mark()), theme.state(state)));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), area);
}

fn current_stage(quest: &OpenQuest) -> Option<StageSpec> {
    quest.stages.get(quest.session.stage()).copied()
}

/// The name of a stage, from the quest's own words, falling back to its role.
fn stage_name(
    definition: Option<&dyn Kq>,
    stage: Option<StageSpec>,
    language: Language,
) -> &'static str {
    match (definition, stage) {
        (Some(quest), Some(stage)) => quest.stage_name(stage.key, language),
        (None, Some(stage)) => t(role_name(stage.role), language),
        _ => "",
    }
}

pub fn role_name(role: StageRole) -> Msg {
    match role {
        StageRole::Explain => Msg::RoleExplain,
        StageRole::Run => Msg::RoleRun,
        StageRole::Tune => Msg::RoleTune,
        StageRole::Break => Msg::RoleBreak,
        StageRole::Recap => Msg::RoleRecap,
    }
}
