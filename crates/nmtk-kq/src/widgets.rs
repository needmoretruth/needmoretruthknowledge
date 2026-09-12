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

use crate::theme::Theme;

/// A column of `name    value` rows, names quiet and values plain.
///
/// Measured in how many columns a name takes on screen, not how many characters it has: a Korean
/// name is half as many characters and exactly as many columns, and `{:<10}` cannot tell.
///
/// A value that will not fit beside its name goes under it rather than off the edge. Losing the
/// end of "4,295,032,833 hashes" costs the reader the number the row existed to show.
pub fn stat_lines(rows: &[(&str, String)], width: usize, theme: Theme) -> Vec<Line<'static>> {
    use crate::text::{column, truncate, width as cells, wrap};
    let label =
        rows.iter().map(|(name, _)| cells(name)).max().unwrap_or(0).min(width.saturating_sub(2));
    let room = width.saturating_sub(label + 2);
    rows.iter()
        .flat_map(|(name, value)| {
            if cells(value) <= room {
                return vec![Line::from(vec![
                    Span::styled(format!("{}  ", column(name, label)), theme.muted()),
                    Span::styled(value.clone(), theme.plain()),
                ])];
            }
            let mut lines = vec![Line::from(Span::styled(truncate(name, width), theme.muted()))];
            for chunk in wrap(value, width.saturating_sub(2)) {
                lines.push(Line::from(Span::styled(format!("  {chunk}"), theme.plain())));
            }
            lines
        })
        .collect()
}

/// `lines` cut to the rows there are, with the last of them marked when the rest were dropped.
///
/// A `Paragraph` draws what fits and drops the rest without a word, and the row it stops on is a
/// line that ran on, drawn exactly like a line that ended. A reader who has just watched a model
/// answer cannot tell whether the sentence stopped there or the panel did. The mark is the
/// difference, and it is the same `…` a cut line carries anywhere else.
pub fn fit(mut lines: Vec<Line<'static>>, rows: usize, width: usize) -> Vec<Line<'static>> {
    if lines.len() <= rows {
        return lines;
    }
    lines.truncate(rows);
    let Some(last) = lines.last_mut() else { return lines };
    let used: usize = last.spans.iter().map(|span| crate::text::width(&span.content)).sum();
    let Some(span) = last.spans.last_mut() else { return lines };
    if used < width {
        span.content = format!("{}…", span.content).into();
        return lines;
    }
    // No room beside the line for the mark, so the mark takes the end of the line itself.
    let room = crate::text::width(&span.content).saturating_sub(used + 1 - width);
    span.content =
        if room == 0 { "…".to_string() } else { crate::text::truncate(&span.content, room) }.into();
    lines
}

/// [`stat_lines`], drawn into an area.
pub fn stats(frame: &mut Frame, area: Rect, theme: Theme, rows: &[(&str, String)]) {
    let lines = stat_lines(rows, area.width as usize, theme);
    frame.render_widget(Paragraph::new(lines), area);
}

/// `text` wrapped to `width`, with every line after the first indented to sit under the first.
///
/// The lead is drawn once, in its own style: a label, a mark, or the spaces that stand in for one.
pub fn wrapped(
    lead: &str,
    text: &str,
    width: usize,
    lead_style: Style,
    style: Style,
) -> Vec<Line<'static>> {
    let indent = crate::text::width(lead);
    crate::text::wrap(text, width.saturating_sub(indent))
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| {
            Line::from(vec![
                Span::styled(
                    if index == 0 { lead.to_string() } else { " ".repeat(indent) },
                    lead_style,
                ),
                Span::styled(chunk, style),
            ])
        })
        .collect()
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
///
/// `label` says what is being drawn, and is not optional. A curve with no name teaches nothing:
/// a reader who cannot tell what the line measures reads it as decoration and looks away.
pub fn curve(frame: &mut Frame, area: Rect, theme: Theme, values: &[f64], label: &str) {
    let [name_area, line_area] =
        Layout::vertical([Constraint::Length(1), Constraint::Min(1)]).areas(area);
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
    // First value to last, not smallest to largest: a loss that fell from 3.3 to 0.4 should read
    // that way round, and "0.4 → 3.3" says the opposite of what happened.
    let first = finite.first().copied().unwrap_or(low);
    let last = finite.last().copied().unwrap_or(high);
    // The bars are drawn between the lowest and highest value in the window, not between zero and
    // the highest, so the bottom row is full whenever nothing dipped near the low. Without the two
    // numbers that say where the box starts and ends, that reads as a graph stuck on full.
    let direction = format!("   {}  \u{2192}  {}", short(first), short(last));
    let scale = format!("{}  \u{2026}  {}", short(low), short(high));
    let head = crate::text::width(label) + crate::text::width(&direction);
    let room = name_area.width as usize;
    let mut spans = vec![
        Span::styled(label.to_string(), theme.muted()),
        Span::styled(direction, theme.muted()),
    ];
    if head + 2 + crate::text::width(&scale) <= room {
        let gap = room - head - crate::text::width(&scale);
        spans.push(Span::styled(" ".repeat(gap), theme.muted()));
        spans.push(Span::styled(scale, theme.muted()));
    }
    frame.render_widget(Paragraph::new(Line::from(spans)), name_area);
    frame.render_widget(Sparkline::default().data(&scaled).style(theme.heading()), line_area);
}

/// A number small enough to sit at the end of a curve's name.
fn short(value: f64) -> String {
    if value >= 1000.0 || value == 0.0 {
        nmtk_core::format::count(value as u64)
    } else {
        format!("{value:.3}")
    }
}

#[cfg(test)]
mod tests {
    use ratatui::Terminal;
    use ratatui::backend::TestBackend;

    use super::*;

    fn draw(width: u16, height: u16, f: impl FnOnce(&mut Frame, Rect)) -> String {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).expect("backend");
        terminal
            .draw(|frame| {
                let area = frame.area();
                f(frame, area)
            })
            .expect("draw");
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
        let text =
            draw(40, 1, |frame, area| bar(frame, area, theme, "miner", f64::NAN, theme.good()));
        assert!(!text.contains('█'));
    }

    #[test]
    fn a_small_fall_still_shows_as_a_fall() {
        let theme = Theme::new(true);
        let text =
            draw(20, 3, |frame, area| curve(frame, area, theme, &[4.1, 4.05, 4.0, 3.9], "loss"));
        assert!(text.chars().any(|c| ('▁'..='█').contains(&c)), "no curve was drawn:\n{text}");
        assert!(text.contains("loss"), "the curve was drawn without a name:\n{text}");
    }

    #[test]
    fn an_empty_curve_draws_a_panel_instead_of_dividing_by_zero() {
        let theme = Theme::new(true);
        let text = draw(8, 3, |frame, area| curve(frame, area, theme, &[], "loss"));
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
