//! Shared vocabulary for every nmtk module.
//!
//! Three things live here and nothing else: what this machine can do, how numbers are written on
//! screen, and the settings a reader changes while the program runs. The learning engines depend
//! on this crate; nothing here knows that a screen exists.

pub mod format;
pub mod language;
pub mod machine;
pub mod settings;

pub use language::Language;
pub use machine::{MachineProfile, SizeClass};
pub use settings::Settings;
