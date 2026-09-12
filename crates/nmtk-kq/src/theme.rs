//! The look of every screen, in one place.
//!
//! nmtk is black and white. Structure is carried by weight, spacing and reversal — never by
//! colour — so the program reads the same on a light terminal, a dark one, and a monochrome one.
//!
//! Colour says one thing only: **state**. Four of them, and no others:
//! green means it worked, red means it failed or an attacker is at work, yellow means in progress,
//! blue means this is the thing you are pointing at. A reader who turns colour off loses no
//! information, because every state is also carried by a word or a mark.

use ratatui::style::{Color, Modifier, Style};
use ratatui::widgets::{Block, BorderType, Padding};

/// Smallest screen nmtk will draw on.
pub const MIN_WIDTH: u16 = 80;
/// Smallest screen nmtk will draw on.
pub const MIN_HEIGHT: u16 = 24;
/// Share of a subject screen given to the explanation; the rest goes to the run.
pub const EXPLAIN_PERCENT: u16 = 38;
/// Never repaint more often than this.
pub const FRAME_MILLIS: u64 = 100;

/// The four states colour is allowed to carry.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum State {
    /// It worked, it verified, it is the honest chain.
    Good,
    /// It failed, it was rejected, an attacker did it.
    Bad,
    /// It is running.
    Working,
    /// This is what you are pointing at.
    Chosen,
}

impl State {
    /// The mark that carries the same meaning when colour is off. One column wide.
    pub fn mark(self) -> &'static str {
        match self {
            State::Good => "+",
            State::Bad => "x",
            State::Working => "~",
            State::Chosen => ">",
        }
    }
}

/// Black and white, plus four state colours the reader can switch off.
#[derive(Debug, Clone, Copy)]
pub struct Theme {
    colour: bool,
}

impl Theme {
    pub fn new(colour: bool) -> Self {
        Self { colour }
    }

    fn coloured(self, colour: Color) -> Style {
        if self.colour { Style::new().fg(colour) } else { Style::new() }
    }

    /// Ordinary text: whatever the terminal already uses.
    pub fn plain(self) -> Style {
        Style::new()
    }

    /// A title or a number that matters. Weight, not colour.
    pub fn heading(self) -> Style {
        Style::new().add_modifier(Modifier::BOLD)
    }

    /// Borders, units, hints — present but never competing.
    pub fn muted(self) -> Style {
        Style::new().fg(Color::DarkGray)
    }

    /// The row under the cursor: reversed, so it stands out on any terminal.
    pub fn selected(self) -> Style {
        Style::new().add_modifier(Modifier::REVERSED)
    }

    /// The style for one of the four states.
    pub fn state(self, state: State) -> Style {
        match state {
            State::Good => self.coloured(Color::Green),
            State::Bad => self.coloured(Color::Red),
            State::Working => self.coloured(Color::Yellow),
            State::Chosen => self.coloured(Color::Blue),
        }
    }

    /// It worked.
    pub fn good(self) -> Style {
        self.state(State::Good)
    }

    /// It failed, or an attacker did it.
    pub fn bad(self) -> Style {
        self.state(State::Bad)
    }

    /// It is running.
    pub fn working(self) -> Style {
        self.state(State::Working)
    }

    /// This is the one you are pointing at.
    pub fn chosen(self) -> Style {
        self.state(State::Chosen)
    }

    /// A panel: square corners, quiet border, one space of air inside.
    pub fn panel(self) -> Block<'static> {
        Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(self.muted())
            .padding(Padding::horizontal(1))
    }

    /// A panel whose title is set in the heading weight.
    pub fn titled_panel(self, title: &str) -> Block<'_> {
        self.panel().title(ratatui::text::Span::styled(format!(" {title} "), self.heading()))
    }
}
