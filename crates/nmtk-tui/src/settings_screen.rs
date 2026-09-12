//! Settings: three rows, each with its value and a line saying why it matters.

use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::{App, SettingItem};
use nmtk_kq::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let area = centred(area, 72);
    let language = app.language();
    let selected = app.settings_index.min(SettingItem::ALL.len() - 1);

    let [rows_area, about_area, status_area] = Layout::vertical([
        Constraint::Length(SettingItem::ALL.len() as u16 + 2),
        Constraint::Min(3),
        Constraint::Length(1),
    ])
    .areas(area);

    let rows: Vec<Line> = SettingItem::ALL
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if index == selected { "▸ " } else { "  " };
            let style = if index == selected { theme.selected() } else { theme.plain() };
            Line::from(vec![
                Span::styled(format!("{marker}{:<16}", t(item.title(), language)), style),
                Span::styled(value_of(*item, app, language), theme.heading()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(rows).block(theme.panel()), rows_area);

    let about = Paragraph::new(t(SettingItem::ALL[selected].about(), language))
        .style(theme.muted())
        .wrap(Wrap { trim: true });
    frame.render_widget(about.block(theme.panel()), about_area);

    if let Some(status) = app.status {
        let style = if status == Msg::SettingsSaved { theme.good() } else { theme.bad() };
        frame.render_widget(
            Paragraph::new(Line::from(Span::styled(format!(" {}", t(status, language)), style))),
            status_area,
        );
    }
}

/// A column of the given width, centred in `area`.
fn centred(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let x = area.x + (area.width - width) / 2;
    Rect { x, width, ..area }
}

/// The value as the reader sees it, brackets included.
fn value_of(item: SettingItem, app: &App, language: nmtk_core::Language) -> String {
    match item {
        SettingItem::Language => format!("[ {} ]", app.settings.language.endonym()),
        SettingItem::Threads => {
            if app.settings.worker_threads == 0 {
                format!("[ {} ({}) ]", t(Msg::SettingsAuto, language), app.threads())
            } else {
                format!("[ {} ]", app.settings.worker_threads)
            }
        }
        SettingItem::Colour => format!("[ {} ]", if app.settings.colour { "on" } else { "off" }),
    }
}


