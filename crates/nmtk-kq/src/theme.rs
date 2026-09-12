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
/// Share of a quest screen the conversation would like, before either side's limits.
pub const EXPLAIN_PERCENT: u16 = 55;
/// Columns a quest's own panel needs before its tables start losing columns.
pub const PANEL_MIN: u16 = 40;
/// Columns past which a line of prose stops being comfortable to read.
pub const TALK_MAX: u16 = 64;
/// Columns below which a conversation is a column of single words.
pub const TALK_MIN: u16 = 30;

/// How a quest screen divides between the conversation and the run, given the width it has.
///
/// Neither side is a fixed fraction. A percentage that reads well at 120 columns starves the
/// panel's tables at 80, and one that fits the tables at 80 gives the conversation a 90-column
/// line at 160, which nobody can read. So each side states what it needs and the width is shared
/// out: the panel takes what its tables want, the conversation takes the rest up to a readable
/// line length, and whatever is left over goes back to the panel.
pub fn split(total: u16) -> (u16, u16) {
    let wanted = total * EXPLAIN_PERCENT / 100;
    let shared = wanted.min(total.saturating_sub(PANEL_MIN)).min(TALK_MAX);
    // On a screen too narrow for both, the conversation wins: it is the lesson.
    let talk = if shared < TALK_MIN { wanted.max(TALK_MIN).min(total) } else { shared };
    (talk, total.saturating_sub(talk))
}
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
