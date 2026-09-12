//! The Knowledge Quest standard.
//!
//! A **KQ** is one finished piece of learning. A reader opens it and the quest talks to them: a
//! sentence or two, then something really happens on their machine, then a sentence about what
//! just happened. They turn values, break it, and leave knowing something they did not know.
//!
//! This crate is the standard every quest is built against, and it exists for two reasons. The
//! first is that quests written by different hands must feel like one program — the same keys, the
//! same shapes, the same places to look. The second is that a settled standard is cheaper to build
//! against: there is nothing to invent, only a shape to fill.
//!
//! The motto it is all built on: **study that isn't fun is labour, and nobody does labour they can
//! avoid.**
//!
//! # What a quest owes
//!
//! - **Facts about itself** ([`meta`]) — id, version, UTC stamps, category, difficulty, length,
//!   the minimum and recommended machine. Plain data, so the list sorts and filters without
//!   opening anything.
//! - **At least three stages** ([`meta::MIN_STAGES`]), each with its own difficulty and its own
//!   role. How many is up to the quest; two is a screen with a footnote, not a lesson.
//! - **A conversation** ([`session::Beat`]) — short beats a reader walks through with Enter, never
//!   a wall of prose.
//! - **Its own words** — every quest carries its own phrase table (`nmtk_i18n::messages!`), one
//!   column per language, English required and never missing.
//! - **Knobs** ([`knob`]) — everything a reader can change, each offering presets *and* typing.
//!
//! # What a quest must not do
//!
//! - Touch the network. Not once, not for anything.
//! - Do its work on the drawing thread. `tick` reads the latest state and returns.
//! - Run two heavy things at once. Starting a second run stops the first.
//! - Invent its own colours, borders or keys. [`theme`] holds every value, and it is black and
//!   white with four state colours.
//! - Fake a result. Every number on screen came from something that really ran.
//!
//! # Versions
//!
//! There is one version number for the whole program, and a quest's version is the nmtk version it
//! was last shipped in. It rises when something is pushed, never while work is in progress, and
//! `1.0.0` waits for a deliberate decision rather than arriving by accident. Old versions stay in the program and stay openable, because a reader who
//! learned from one should be able to go back to exactly what they saw.

pub mod catalogue;
pub mod conversation;
pub mod knob;
pub mod meta;
pub mod session;
pub mod text;
pub mod theme;
pub mod widgets;

pub use catalogue::{Catalogue, Filter, SortKey};
pub use knob::{Knob, KnobValue};
pub use meta::{
    Category, Difficulty, Fit, KqId, KqMeta, MachineNeeds, Requirements, StageRole, StageSpec,
    Stamp, Version,
};
pub use session::{Action, Beat, Kq, KqSession, Reaction, RunState, Voice};
pub use theme::{State, Theme};
