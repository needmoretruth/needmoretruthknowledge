//! The conversation panel: what the quest has said so far.
//!
//! Beats are drawn oldest first, one short paragraph each, with a blank line between them. A
//! reader arriving at a stage sees one or two sentences, not a page — and every time they press
//! Enter, one more beat appears under the last, the way a chat does.
//!
//! Wrapping happens here rather than in the drawing library so the panel knows its own height
//! before it draws. That is what lets it scroll to the newest beat exactly.

use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::session::{Beat, Voice};
use crate::text::wrap;
use crate::theme::Theme;

/// Columns taken by a beat's prefix, so every beat wraps to the same width.
const PREFIX: usize = 2;

/// Turns beats into drawable lines, wrapped to `columns`.
pub fn lines(beats: &[Beat], theme: Theme, columns: usize) -> Vec<Line<'static>> {
    let body = columns.saturating_sub(PREFIX).max(8);
    let mut out: Vec<Line<'static>> = Vec::new();
    for (index, beat) in beats.iter().enumerate() {
        if index > 0 {
            out.push(Line::from(""));
        }
        let (mark, style) = match beat.voice {
            Voice::Say => (" ", theme.plain()),
            Voice::Event(None) => ("·", theme.muted()),
            Voice::Event(Some(state)) => (state.mark(), theme.state(state)),
            Voice::Ask => ("▸", theme.heading()),
        };
        for (row, text) in wrap(&beat.text, body).into_iter().enumerate() {
            let prefix = if row == 0 { format!("{mark} ") } else { "  ".to_string() };
            out.push(Line::from(vec![
                Span::styled(prefix, style),
                Span::styled(text, if matches!(beat.voice, Voice::Say) { theme.plain() } else { style }),
            ]));
        }
    }
    out
}

/// Draws the conversation inside `area`, scrolled so that `offset` lines are hidden above.
///
/// Returns how far the panel *could* be scrolled, so the caller can keep its offset honest when
/// the conversation grows or the terminal is resized.
pub fn render(
    frame: &mut Frame,
    area: Rect,
    theme: Theme,
    beats: &[Beat],
    offset: usize,
) -> usize {
    let mut all = lines(beats, theme, area.width as usize);
    let height = area.height as usize;
    // A conversation starts at the bottom and rises, the way every messenger does. Beginning at
    // the top would leave the first sentence stranded above an empty screen.
    if all.len() < height {
        let mut padded = vec![Line::from(""); height - all.len()];
        padded.append(&mut all);
        all = padded;
    }
    let furthest = all.len().saturating_sub(height);
    let offset = offset.min(furthest);
    frame.render_widget(Paragraph::new(all).scroll((offset as u16, 0)), area);
    furthest
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::theme::State;

    fn beats() -> Vec<Beat> {
        vec![
            Beat::say("Every chain has to answer one question: where is the money?"),
            Beat::event("A block arrived."),
            Beat::outcome(State::Good, "The payment is confirmed."),
            Beat::ask("Press Enter to carry on."),
        ]
    }

    #[test]
    fn beats_are_separated_by_a_blank_line() {
        let drawn = lines(&beats(), Theme::new(true), 40);
        let blanks = drawn.iter().filter(|line| line.width() == 0).count();
        assert_eq!(blanks, 3, "four beats should have three gaps between them");
    }

    #[test]
    fn nothing_is_drawn_wider_than_the_panel() {
        let drawn = lines(&beats(), Theme::new(true), 24);
        for line in &drawn {
            assert!(line.width() <= 24, "a line overflowed: {line:?}");
        }
    }

    #[test]
    fn a_long_beat_keeps_its_prefix_on_the_first_line_only() {
        let long = vec![Beat::ask("This sentence is deliberately long enough to wrap twice over.")];
        let drawn = lines(&long, Theme::new(true), 20);
        assert!(drawn.len() > 1);
        let first: String = drawn[0].spans.iter().map(|s| s.content.as_ref()).collect();
        let second: String = drawn[1].spans.iter().map(|s| s.content.as_ref()).collect();
        assert!(first.starts_with('▸'));
        assert!(second.starts_with("  "));
    }

    #[test]
    fn korean_beats_wrap_within_the_panel() {
        let korean = vec![Beat::say(
            "모든 체인은 한 가지 질문에 답해야 합니다. 돈이 어디에 있는가? 그리고 누가 그것을 정하는가?",
        )];
        let drawn = lines(&korean, Theme::new(true), 30);
        for line in &drawn {
            assert!(line.width() <= 30, "a Korean line overflowed: {line:?}");
        }
    }
}
