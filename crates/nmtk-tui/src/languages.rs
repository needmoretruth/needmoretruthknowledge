//! Pick a language.
//!
//! Every language is written in its own script, because a reader looking for their language is
//! looking for a word they recognise — not for a translation of the word "Korean".

use nmtk_core::Language;
use nmtk_i18n::{Msg, t};
use nmtk_kq::theme::{State, Theme};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{List, ListItem, ListState, Paragraph, Wrap};

use crate::app::App;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let language = app.language();
    let chosen = app.language_index.min(Language::ALL.len().saturating_sub(1));

    let area = centred(area, 56);
    let [list_area, note_area] =
        Layout::vertical([Constraint::Min(3), Constraint::Length(4)]).areas(area);

    let rows: Vec<ListItem> = Language::ALL
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let current = *entry == app.settings.language;
            let mark = if current { State::Good.mark() } else { " " };
            ListItem::new(Line::from(vec![
                Span::styled(format!("{mark} "), theme.state(State::Good)),
                // Measured in terminal cells, not characters: 한국어 is three characters and six
                // cells, and a column counted in characters puts the code three cells adrift.
                Span::styled(nmtk_kq::text::column(entry.endonym(), 24), theme.plain()),
                Span::styled(entry.code().to_string(), theme.muted()),
            ]))
            .style(if index == chosen { theme.selected() } else { theme.plain() })
        })
        .collect();

    let mut state = ListState::default();
    state.select(Some(chosen));
    frame.render_stateful_widget(
        List::new(rows).block(theme.titled_panel(t(Msg::SettingsLanguage, language))),
        list_area,
        &mut state,
    );

    let note = Paragraph::new(t(Msg::SettingsLanguageAbout, language))
        .style(theme.muted())
        .wrap(Wrap { trim: true })
        .block(theme.panel());
    frame.render_widget(note, note_area);
}

fn centred(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let x = area.x + (area.width - width) / 2;
    Rect { x, width, ..area }
}
