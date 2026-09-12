//! What a quest has to be able to do, and how the program talks to it.
//!
//! Two traits: [`Kq`] is the quest sitting in the list, cheap to ask about itself; [`KqSession`]
//! is the quest a reader has opened, which owns whatever threads the work needs.
//!
//! # A quest is a conversation
//!
//! Nobody reads a screen of prose to get to the interesting part. A quest talks: a sentence or
//! two, then the reader presses Enter, then something happens, then another sentence about what
//! just happened. The left-hand panel is that conversation, oldest at the top, exactly like a
//! chat; the right-hand panel is the thing itself, running.
//!
//! This is not decoration. A reader who is told one small thing and immediately watches it happen
//! has learned it. A reader handed six paragraphs has skipped five of them.

use nmtk_core::{Language, MachineProfile};
use ratatui::Frame;
use ratatui::layout::Rect;

use crate::knob::Knob;
use crate::meta::KqMeta;
use crate::theme::{State, Theme};

/// Who is speaking in one beat of the conversation.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Voice {
    /// The quest, explaining. One or two sentences — never a paragraph, never a lecture.
    Say,
    /// Something that happened in the run, reported as it happened, with the state it carries.
    Event(Option<State>),
    /// Something the reader has to do before the conversation goes on.
    Ask,
}

/// One beat of the conversation: a short thing said, in the reader's language.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Beat {
    pub voice: Voice,
    pub text: String,
}

impl Beat {
    /// The quest explaining.
    pub fn say(text: impl Into<String>) -> Self {
        Self { voice: Voice::Say, text: text.into() }
    }

    /// Something that happened, with no verdict attached.
    pub fn event(text: impl Into<String>) -> Self {
        Self { voice: Voice::Event(None), text: text.into() }
    }

    /// Something that happened, and whether it went well.
    pub fn outcome(state: State, text: impl Into<String>) -> Self {
        Self { voice: Voice::Event(Some(state)), text: text.into() }
    }

    /// Something the reader has to do.
    pub fn ask(text: impl Into<String>) -> Self {
        Self { voice: Voice::Ask, text: text.into() }
    }
}

/// A gesture, already separated from the key that produced it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    /// Move to the next item — a knob, a row, a party.
    Next,
    /// Move to the previous item.
    Previous,
    /// Jump to a stage by its position in the quest's declared stages.
    Stage(usize),
    /// Enter, when nothing is being typed: carry the conversation on, or take the next step.
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
    /// The name of one stage, looked up by the key the quest declared it with.
    fn stage_name(&self, key: &str, language: Language) -> &'static str;
    /// Opens the quest. The session may size itself to the machine it is handed.
    fn open(&self, machine: &MachineProfile) -> Box<dyn KqSession>;
}

/// A quest a reader has opened.
pub trait KqSession {
    /// Which stage is showing, as a position in the quest's declared stages.
    fn stage(&self) -> usize;

    /// Moves to a stage. Out-of-range positions are ignored.
    fn go_to(&mut self, stage: usize);

    /// The conversation so far in this stage, oldest first, already in the reader's language.
    ///
    /// Built on demand rather than stored, so switching language rewrites the whole conversation
    /// rather than leaving half of it in the old one.
    fn transcript(&self, language: Language) -> Vec<Beat>;

    /// Whether Enter carries the conversation on right now.
    ///
    /// False while the reader is meant to do something else — turn a knob, wait for a run to
    /// finish — and the last beat should be the one saying so.
    fn can_advance(&self) -> bool;

    /// The knobs of the current stage, in the order they are drawn.
    fn knobs(&self) -> &[Knob];

    /// Which knob the reader is pointing at, if the stage has any.
    fn chosen_knob(&self) -> Option<usize>;

    /// Handles one gesture.
    fn on(&mut self, action: Action) -> Reaction;

    /// Called about ten times a second, always on the drawing thread. Read the workers' latest
    /// state here and add beats for anything that happened; never block, never do the work itself.
    fn tick(&mut self);

    /// Where the run is.
    fn run_state(&self) -> RunState;

    /// The right-hand panel: the thing itself, running.
    fn render(&self, frame: &mut Frame, area: Rect, theme: Theme, language: Language);

    /// The keys this quest adds to the bottom bar, beyond the ones every screen has.
    fn keys(&self, language: Language) -> Vec<(&'static str, &'static str)>;

    /// Whether a number is being typed into a knob right now.
    ///
    /// While this is true the shell hands digits to the quest; otherwise digits jump between
    /// stages. A quest with no typed knobs leaves this alone.
    fn typing(&self) -> bool {
        false
    }

    /// Stops any worker threads. Always called before the session is dropped.
    fn close(&mut self);
}
