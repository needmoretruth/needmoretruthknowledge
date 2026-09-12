//! What a quest has to be able to do, and how the program talks to it.
//!
//! Two traits: `Kq` is the quest sitting in the list, cheap to ask about itself; `KqSession` is
//! the quest a reader has opened, which owns whatever threads the work needs. The program drives
//! a session with `Action`s rather than key codes, so the key map lives in one place and every
//! quest answers the same gestures.

use nmtk_core::{Language, MachineProfile};
use ratatui::Frame;
use ratatui::layout::Rect;
use ratatui::text::Line;

use crate::knob::Knob;
use crate::meta::{KqMeta, StageKind};
use crate::theme::{State, Theme};

/// A gesture, already separated from the key that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move to the next item — a knob, a row, a party.
    Next,
    /// Move to the previous item.
    Previous,
    /// Jump to a stage.
    Stage(StageKind),
    /// Enter, when nothing is being typed: start the run, or take the next step.
    Go,
    /// Space: pause and resume.
    PauseOrResume,
    /// `r`: throw the run away and start over.
    Reset,
    /// Left or right on the chosen knob.
    Nudge(i32),
    /// A character typed into the chosen knob.
    Type(char),
    /// Backspace while typing.
    Backspace,
    /// Enter while typing: accept the number.
    Commit,
    /// Esc while typing: discard it.
    Cancel,
}

/// Whether the quest took the gesture. An ignored action falls through to the shell, which is how
/// `q` still goes back while a quest is running.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reaction {
    Handled,
    Ignored,
}

/// Where a run is.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RunState {
    /// Nothing started yet.
    Idle,
    /// Threads are working.
    Running,
    /// Started, then paused.
    Paused,
    /// It finished on its own.
    Done,
}

impl RunState {
    /// The state colour this run deserves in the status line.
    pub fn state(self) -> Option<State> {
        match self {
            RunState::Idle => None,
            RunState::Running => Some(State::Working),
            RunState::Paused => Some(State::Chosen),
            RunState::Done => Some(State::Good),
        }
    }
}

/// A quest in the list.
pub trait Kq: Send + Sync {
    fn meta(&self) -> KqMeta;
    /// The quest's name, from its own phrase table.
    fn title(&self, language: Language) -> &'static str;
    /// One line, the length of a subtitle, saying what the reader will do.
    fn summary(&self, language: Language) -> &'static str;
    /// The narrower shelf this sits on, said in words.
    fn subcategory(&self, language: Language) -> &'static str;
    /// Opens the quest. The session may size itself to the machine it is handed.
    fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession>;
}

/// A quest a reader has opened.
pub trait KqSession {
    /// Which stage is showing.
    fn stage(&self) -> StageKind;
    /// Moves to a stage the quest declared. Ignored for a stage it does not have.
    fn go_to(&mut self, stage: StageKind);
    /// The knobs of the current stage, in the order they are drawn.
    fn knobs(&self) -> &[Knob];
    /// Which knob the reader is pointing at, if the stage has any.
    fn chosen_knob(&self) -> Option<usize>;
    /// Handles one gesture.
    fn on(&mut self, action: Action) -> Reaction;
    /// Called about ten times a second, always on the drawing thread. Read the workers' latest
    /// state here; never block, and never do the work itself.
    fn tick(&mut self);
    /// Where the run is.
    fn run_state(&self) -> RunState;
    /// The left-hand panel: what this stage is about, in the reader's language.
    fn explain(&self, language: Language) -> Vec<Line<'static>>;
    /// The right-hand panel: the thing itself, running.
    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language);
    /// The keys this quest adds to the bottom bar, beyond the ones every screen has.
    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)>;
    /// Whether a number is being typed into a knob right now.
    ///
    /// While this is true the shell hands digits to the quest; otherwise `1`–`5` jump between
    /// stages. A quest with no typed knobs leaves this alone.
    fn typing(&self) -> bool {
        false
    }

    /// Stops any worker threads. Always called before the session is dropped.
    fn close(&mut self);
}
