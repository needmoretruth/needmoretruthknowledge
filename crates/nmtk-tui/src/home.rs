//! The first screen: the name, the motto, and the subjects.

use nmtk_core::format;
use nmtk_i18n::{Msg, t};
use ratatui::Frame;
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Wrap};

use crate::app::{App, HomeItem};
use crate::logo;
use crate::theme::Theme;

pub fn render(frame: &mut Frame, area: Rect, app: &App, theme: Theme) {
    let language = app.language();
    let items = HomeItem::all();
    let selected = app.home_index.min(items.len() - 1);

    let room_for_logo = area.height >= 20 && area.width >= logo::LARGE_WIDTH + 4;
    let logo_height = if room_for_logo { logo::LARGE_HEIGHT + 1 } else { 2 };

    let [logo_area, motto_area, menu_area, _slack, about_area, machine_area] = Layout::vertical([
        Constraint::Length(logo_height),
        Constraint::Length(3),
        Constraint::Length(items.len() as u16),
        Constraint::Min(0),
        Constraint::Length(3),
        Constraint::Length(1),
    ])
    .areas(area);

    render_logo(frame, logo_area, theme, room_for_logo);

    let motto = Paragraph::new(vec![
        Line::from(Span::styled(t(Msg::Motto, language), theme.accent())),
        Line::from(Span::styled(t(Msg::AppSubtitle, language), theme.muted())),
    ])
    .alignment(Alignment::Center);
    frame.render_widget(motto, motto_area);

    let rows: Vec<Line> = items
        .iter()
        .enumerate()
        .map(|(index, item)| {
            let marker = if index == selected { "▸ " } else { "  " };
            let style = if index == selected { theme.selected() } else { theme.plain() };
            Line::from(Span::styled(format!("{marker}{}", t(item.title(), language)), style))
        })
        .collect();
    let menu_width = area.width.min(44);
    let menu = centred(menu_area, menu_width);
    frame.render_widget(Paragraph::new(rows), menu);

    if let Some(about) = items[selected].about() {
        let text = Paragraph::new(t(about, language))
            .style(theme.muted())
            .wrap(Wrap { trim: true })
            .alignment(Alignment::Center);
        frame.render_widget(text, centred(about_area, 64));
    }

    let machine = Line::from(vec![
        Span::styled(t(Msg::MachineCores, language), theme.muted()),
        Span::raw(" "),
        Span::styled(app.machine.logical_cores.to_string(), theme.plain()),
        Span::styled("  ·  ", theme.muted()),
        Span::styled(t(Msg::MachineMemory, language), theme.muted()),
        Span::raw(" "),
        Span::styled(format::bytes(app.machine.total_memory_bytes), theme.plain()),
        Span::styled("  ·  ", theme.muted()),
        Span::styled(t(Msg::MachineUsing, language), theme.muted()),
        Span::raw(" "),
        Span::styled(format!("{} / {}", app.threads(), app.machine.logical_cores), theme.plain()),
    ]);
    frame.render_widget(Paragraph::new(machine).alignment(Alignment::Center), machine_area);
}

fn render_logo(frame: &mut Frame, area: Rect, theme: Theme, large: bool) {
    let lines: Vec<Line> = if large {
        logo::LARGE.iter().map(|row| Line::from(Span::styled(*row, theme.accent()))).collect()
    } else {
        vec![Line::from(""), Line::from(Span::styled(logo::SMALL, theme.accent()))]
    };
    frame.render_widget(Paragraph::new(lines).alignment(Alignment::Center), area);
}

/// A centred column of the given width inside `area`.
fn centred(area: Rect, width: u16) -> Rect {
    let width = width.min(area.width);
    let x = area.x + (area.width - width) / 2;
    Rect { x, width, ..area }
}

