//! The look of every screen, in one place.
//!
//! These values are the design, not a suggestion: a screen that wants a different colour is a
//! screen that has not made its case. Nothing paints a background — the reader's terminal may be
//! light or dark, and borrowing their background is the only way to read well in both.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Padding};

/// Smallest screen nmtk will draw on.
pub const MIN_WIDTH: u16 = 80;
/// Smallest screen nmtk will draw on.
pub const MIN_HEIGHT: u16 = 24;
/// Share of a module screen given to the explanation, the rest goes to the run.
pub const EXPLAIN_PERCENT: u16 = 38;
/// Never repaint more often than this.
pub const FRAME_MILLIS: u64 = 100;

/// Colours, or no colours at all when the reader turned them off.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    colour: bool,
}

impl Theme {
    pub fn new(colour: bool) -> Self {
        Self { colour }
    }

    fn styled(self, colour: Color) -> Style {
        if self.colour { Style::new().fg(colour) } else { Style::new() }
    }

    /// Titles, the selected subject, anything the eye should land on first.
    pub fn accent(self) -> Style {
        self.styled(Color::Cyan)
    }

    /// A proof that verified, an honest miner, a transaction that stuck.
    pub fn ok(self) -> Style {
        self.styled(Color::Green)
    }

    /// An attacker, a rejected proof, a reverted block.
    pub fn danger(self) -> Style {
        self.styled(Color::Red)
    }

    /// Work in progress.
    pub fn busy(self) -> Style {
        self.styled(Color::Yellow)
    }

    /// Borders, hints, units — present but never competing.
    pub fn muted(self) -> Style {
        self.styled(Color::DarkGray)
    }

    /// Ordinary text: whatever the terminal already uses.
    pub fn plain(self) -> Style {
        Style::new()
    }

    /// The row under the cursor. Reversed rather than coloured, so it stands out on any terminal.
    pub fn selected(self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }

    /// Emphasis inside a sentence.
    pub fn strong(self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    /// A panel: rounded, quiet border, one space of air inside.
    pub fn panel(self) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.muted())
            .padding(Padding::horizontal(1))
    }

    /// A panel with a title written in the accent colour.
    pub fn titled_panel(self, title: &str) -> Block<'_> {
        self.panel().title(ratatui::text::Span::styled(format!(" {title} "), self.accent()))
    }
}
