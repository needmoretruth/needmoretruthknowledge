//! The pieces every subject screen is built from.
//!
//! Four subjects showing different things still have to look like one program, so the shapes that
//! repeat — a column of named numbers, a share bar, a falling curve — are defined once here rather
//! than four times.

use ratatui::Frame;
use ratatui::layout::{Constraint, Layout, Rect};
use ratatui::style::Style;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Paragraph, Sparkline};

use nmtk_kq::Theme;

/// A column of `name    value` rows, names quiet and values plain.
pub fn stats(frame: &mut Frame, area: Rect, theme: Theme, rows: &[(&str, String)]) {
    let width = rows.iter().map(|(name, _)| name.chars().count()).max().unwrap_or(0);
    let lines: Vec<Line> = rows
        .iter()
        .map(|(name, value)| {
            Line::from(vec![
                Span::styled(format!("{name:<width$}  "), theme.muted()),
                Span::styled(value.clone(), theme.plain()),
            ])
        })
        .collect();
    frame.render_widget(Paragraph::new(lines), area);
}

/// A share of something, drawn as a bar that fills from the left.
///
/// The bar is text rather than a gauge widget so several bars line up exactly, which is the whole
/// point when the reader is comparing miners.
pub fn bar(frame: &mut Frame, area: Rect, theme: Theme, label: &str, ratio: f64, style: Style) {
    let ratio = if ratio.is_finite() { ratio.clamp(0.0, 1.0) } else { 0.0 };
    let [label_area, bar_area] =
        Layout::horizontal([Constraint::Length(14), Constraint::Min(4)]).areas(area);
    frame.render_widget(
        Paragraph::new(Line::from(Span::styled(label.to_string(), theme.plain()))),
        label_area,
    );

    let width = bar_area.width as usize;
    let filled = (ratio * width as f64).round() as usize;
    let line = Line::from(vec![
        Span::styled("█".repeat(filled), style),
        Span::styled("░".repeat(width.saturating_sub(filled)), theme.muted()),
    ]);
    frame.render_widget(Paragraph::new(line), bar_area);
}

/// A curve of recent values — a training loss, a hash rate over time.
///
/// Values are scaled against their own range, so a loss falling from 4.1 to 3.9 still reads as a
/// fall. A flat series draws as a flat line rather than dividing by zero.
pub fn curve(frame: &mut Frame, area: Rect, theme: Theme, values: &[f64]) {
    let finite: Vec<f64> = values.iter().copied().filter(|v| v.is_finite()).collect();
    if finite.is_empty() {
        frame.render_widget(theme.panel(), area);
        return;
    }
    let low = finite.iter().copied().fold(f64::INFINITY, f64::min);
    let high = finite.iter().copied().fold(f64::NEG_INFINITY, f64::max);
    let span = (high - low).max(f64::EPSILON);
    let scaled: Vec<u64> =
        finite.iter().map(|v| (((v - low) / span) * 100.0).round() as u64).collect();
    frame.render_widget(Sparkline::default().data(&scaled).style(theme.heading()), area);
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn draw(width: u16, height: u16, f: impl FnOnce(&mut Frame, Rect)) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
        terminal.draw(|frame| { let area = frame.area(); f(frame, area) }).expect("draw");
        let buffer = terminal.backend().buffer().clone();
        (0..buffer.area.height)
            .map(|y| {
                (0..buffer.area.width)
                    .map(|x| buffer[(x, y)].symbol().to_string())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    #[test]
    fn a_full_bar_fills_the_width() {
        let theme = Theme::new(true);
        let text = draw(40, 1, |frame, area| bar(frame, area, theme, "miner", 1.0, theme.good()));
        assert_eq!(text.matches('█').count(), 26);
        assert!(!text.contains('░'));
    }

    #[test]
    fn a_bar_of_nonsense_draws_empty_rather_than_panicking() {
        let theme = Theme::new(true);
        let text = draw(40, 1, |frame, area| bar(frame, area, theme, "miner", f64::NAN, theme.good()));
        assert!(!text.contains('█'));
    }

    #[test]
    fn a_small_fall_still_shows_as_a_fall() {
        let theme = Theme::new(true);
        let text = draw(8, 3, |frame, area| curve(frame, area, theme, &[4.1, 4.05, 4.0, 3.9]));
        assert!(text.chars().any(|c| ('▁'..='█').contains(&c)), "no curve was drawn:\n{text}");
    }

    #[test]
    fn an_empty_curve_draws_a_panel_instead_of_dividing_by_zero() {
        let theme = Theme::new(true);
        let text = draw(8, 3, |frame, area| curve(frame, area, theme, &[]));
        assert!(text.contains('╭'));
    }

    #[test]
    fn stats_line_their_values_up() {
        let theme = Theme::new(true);
        let rows = [("Cores", "12".to_string()), ("Hash rate", "1.20 MH/s".to_string())];
        let text = draw(30, 2, |frame, area| stats(frame, area, theme, &rows));
        let columns: Vec<usize> =
            text.lines().map(|line| line.find(|c: char| c.is_ascii_digit()).unwrap_or(0)).collect();
        assert_eq!(columns[0], columns[1], "values are not in one column:\n{text}");
    }
}
