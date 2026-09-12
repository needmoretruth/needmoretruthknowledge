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
const KEYS: [(Option<&str>, Msg); 20] = [
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
    (Some("Shift+Tab"), Msg::KeyStageBack),
    (Some("↑ ↓"), Msg::KeyChoose),
    (Some("← →"), Msg::KeyChange),
    (Some("0-9"), Msg::KeyType),
    (Some("Space"), Msg::KeyPause),
    (Some("r"), Msg::KeyReset),
    (Some("PgUp PgDn"), Msg::KeyScroll),
];

pub fn render(frame: &mut Frame, area: Rect, language: nmtk_core::Language, theme: Theme) {
    // The promise at the bottom is asked for by height first: a help screen that pushes its own
    // last box off a 24-row terminal has hidden the one line it exists to repeat.
    let [keys_area, note_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(3)]).areas(area);

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

    let block = theme.titled_panel(t(Msg::HelpTitle, language));
    let room = keys_area.height.saturating_sub(2) as usize;
    // Every key has to be on the screen at once. When the list is taller than the terminal it
    // goes into two columns, split where a group starts so no group is cut in half.
    if rows.len() > room && keys_area.width >= 70 {
        let inner = block.inner(keys_area);
        frame.render_widget(block, keys_area);
        let [left, right] =
            Layout::horizontal([Constraint::Percentage(50), Constraint::Percentage(50)])
                .areas(inner);
        let cut = split_point(rows.len());
        let (first, second) = rows.split_at(cut);
        frame.render_widget(Paragraph::new(first.to_vec()), left);
        frame.render_widget(Paragraph::new(second.to_vec()), right);
    } else {
        frame.render_widget(Paragraph::new(rows).block(block), keys_area);
    }

    let note =
        Paragraph::new(t(Msg::HelpOffline, language)).style(theme.good()).wrap(Wrap { trim: true });
    frame.render_widget(note.block(theme.panel()), note_area);
}

/// Where to break the list into two columns: the group heading nearest the middle, so a group and
/// its keys stay together.
fn split_point(rows: usize) -> usize {
    let middle = rows.div_ceil(2);
    let starts: Vec<usize> = KEYS
        .iter()
        .enumerate()
        .filter(|(_, (key, _))| key.is_none())
        .map(|(index, _)| index)
        .collect();
    starts
        .iter()
        .copied()
        .filter(|start| *start > 0)
        .min_by_key(|start| start.abs_diff(middle))
        .unwrap_or(middle)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_two_columns_break_where_a_group_does() {
        let at = split_point(KEYS.len());
        assert!(KEYS[at].0.is_none(), "a column must start with a group heading");
        assert!(at > 0 && at < KEYS.len());
    }

    #[test]
    fn every_key_the_shell_answers_to_is_on_this_screen() {
        for key in ["Tab", "Shift+Tab", "Space", "r", "l", "s", "?"] {
            assert!(
                KEYS.iter().any(|(row, _)| row
                    .is_some_and(|row| row.split_whitespace().any(|word| word == key))),
                "{key} is not in the help screen"
            );
        }
    }
}
