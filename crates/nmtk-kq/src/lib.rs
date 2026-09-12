//! The Knowledge Quest standard.
//!
//! A **KQ** is one finished piece of learning: a reader opens it, reads why it matters, runs the
//! real thing on their own machine, turns its values, tries to break it, and leaves knowing what
//! happened. nmtk is a program for holding a shelf of them.
//!
//! This crate is the standard every quest is built against, and it exists for two reasons.
//! The first is that four quests written by four different hands must feel like one program — the
//! same keys, the same shapes, the same places to look. The second is that a settled standard is
//! cheaper to build against: there is nothing to invent, only a shape to fill.
//!
//! # What a quest owes
//!
//! - **Facts about itself** ([`meta`]) — id, version, dates, category, difficulty, length, what it
//!   needs from the machine. Plain data, so the list can sort and filter without opening anything.
//! - **Its own words** — every quest carries its own phrase table (`nmtk_i18n::messages!`), English
//!   first, Korean optional. Two quests being written at once never touch the same file.
//! - **Stages** ([`meta::StageKind`]) — `Brief`, `Run`, and as many of `Tune`, `Break` and `Recap`
//!   as the subject deserves. `Brief` and `Run` are not optional: a quest that cannot be run is an
//!   article, and articles belong somewhere else.
//! - **Knobs** ([`knob`]) — everything a reader can change, each offering presets *and* typing.
//! - **A session** ([`session::KqSession`]) — which owns the threads, answers [`session::Action`]s,
//!   and draws into the right-hand panel.
//!
//! # What a quest must not do
//!
//! - Touch the network. Not once, not for anything.
//! - Do its work on the drawing thread. `tick` reads the latest state and returns.
//! - Invent its own colours, borders or keys. [`theme`] holds every value, and it is black and
//!   white with four state colours.
//! - Fake a result. Every number on screen came from something that really ran.
//!
//! # Versions
//!
//! A quest's [`meta::KqId`] never changes; its [`meta::KqVersion`] rises. Old versions stay in the
//! program and stay openable, because a reader who learned from one should be able to go back to
//! exactly what they saw.

pub mod catalogue;
pub mod knob;
pub mod meta;
pub mod session;
pub mod text;
pub mod theme;
pub mod widgets;

pub use catalogue::{Catalogue, Filter, SortKey};
pub use knob::{Knob, KnobValue};
pub use meta::{Category, Date, Difficulty, KqId, KqMeta, KqVersion, Requirements, StageKind};
pub use session::{Action, Kq, KqSession, Reaction, RunState};
pub use theme::{State, Theme};
