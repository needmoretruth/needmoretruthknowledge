//! The first launch, and the only time nmtk asks the reader anything.
//!
//! A reader who has never run this before does not know what it is, and — if they read Korean —
//! is looking at a screen of English. Both are answered here, once: four short lines saying what
//! the program is, and the settings that matter before the first quest opens. Afterwards a
//! settings file exists and this screen is never seen again.

use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::Paragraph;

use crate::app::{App, SettingItem};
use crate::logo;
use crate::settings_screen::value_of;
use nmtk_kq::Theme;
use nmtk_kq::text::wrap;

/// The four lines that say what this is.
const ABOUT: [Msg; 4] = [Msg::WelcomeOne, Msg::WelcomeTwo, Msg::WelcomeThree, Msg::WelcomeFour];

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let language = app.language();
    let column = centred(area, 74);
    let room_for_logo = column.height >= 26;

    // Wrapped here rather than by the drawing library so a Korean line breaks on its own width,
    // and so the settings can sit directly under the last sentence instead of a screen below it.
    let mut about: Vec<Line> = Vec::new();
    for (index, message) in ABOUT.iter().enumerate() {
        if index > 0 {
            about.push(Line::from(""));
        }
        for text in wrap(t(*message, language), column.width as usize) {
            about.push(Line::from(Span::styled(text, theme.plain())));
        }
    }
    let selected = app.settings_index.min(SettingItem::ALL.len() - 1);
    // The confirmation joins the explanation rather than replacing it. A reader pressing the
    // arrows to see what a setting does was losing the sentence that said what it does.
    let mut hint: Vec<Line> =
        wrap(t(SettingItem::ALL[selected].about(), language), column.width as usize)
            .into_iter()
            .map(|text| Line::from(Span::styled(text, theme.muted())))
            .collect();
    if let Some(status) = app.status {
        hint.push(Line::from(Span::styled(t(status, language), theme.good())));
    }

    let [banner, about_area, rows_area, hint_area, _] = Layout::vertical([
        Constraint::Length(if room_for_logo { logo::LARGE_HEIGHT + 1 } else { 1 }),
        Constraint::Length(about.len() as u16 + 1),
        Constraint::Length(SettingItem::ALL.len() as u16 + 2),
        Constraint::Length(hint.len() as u16 + 1),
        Constraint::Min(0),
    ])
    .areas(column);

    let banner_lines: Vec<Line> = if room_for_logo {
        logo::LARGE.iter().map(|row| Line::from(Span::styled(*row, theme.heading()))).collect()
    } else {
        vec![Line::from(Span::styled(t(Msg::WelcomeTitle, language), theme.heading()))]
    };
    frame.render_widget(Paragraph::new(banner_lines).alignment(Alignment::Center), banner);
    frame.render_widget(Paragraph::new(about), about_area);

    let rows: Vec<Line> = SettingItem::ALL
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if index == selected { "▸ " } else { "  " };
            let style = if index == selected { theme.selected() } else { theme.plain() };
            Line::from(vec![
                Span::styled(format!("{marker}{}", nmtk_kq::text::column(t(item.title(), language), 18)), style),
                Span::styled(value_of(*item, app, language), theme.heading()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows).block(theme.panel()), rows_area);
    frame.render_widget(Paragraph::new(hint), hint_area);
}

/// A column of the given width, centred in `area`.
fn centred(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let x = area.x + (area.width - width) / 2;
    Rect { x, width, ..area }
}
