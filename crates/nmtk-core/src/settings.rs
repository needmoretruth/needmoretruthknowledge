//! What the reader changes while the program runs, and where it is kept.
//!
//! Settings are written to the user's config directory so a reader who raises the thread count
//! once does not have to raise it again tomorrow. A missing, unreadable or half-written file is
//! not an error: nmtk starts with defaults sized for the machine and says nothing.

use std::fs;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::language::Language;
use crate::machine::MachineProfile;

/// Everything the reader can change and keep.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub language: Language,
    /// Threads long-running work may use. 0 means "decide from the machine".
    pub worker_threads: usize,
    /// Whether to use colour at all. Off gives a screen that reads on a monochrome terminal.
    pub colour: bool,
}

impl Default for Settings {
    fn default() -> Self {
        Self { language: Language::default(), worker_threads: 0, colour: true }
    }
}

impl Settings {
    /// Settings from disk, or defaults. Never fails and never blocks startup.
    pub fn load() -> Self {
        let Some(path) = Self::path() else { return Self::default() };
        let Ok(text) = fs::read_to_string(path) else { return Self::default() };
        toml::from_str(&text).unwrap_or_default()
    }

    /// Writes the settings, creating the directory if needed.
    pub fn save(&self) -> Result<(), SettingsError> {
        let path = Self::path().ok_or(SettingsError::NoConfigDirectory)?;
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SettingsError::Write)?;
        }
        let text = toml::to_string_pretty(self).map_err(|_| SettingsError::Encode)?;
        fs::write(path, text).map_err(SettingsError::Write)
    }

    /// Where the file lives: `$XDG_CONFIG_HOME/nmtk/settings.toml`, falling back to
    /// `$HOME/.config/nmtk/settings.toml`.
    pub fn path() -> Option<PathBuf> {
        let base = match std::env::var_os("XDG_CONFIG_HOME") {
            Some(dir) if !dir.is_empty() => PathBuf::from(dir),
            _ => PathBuf::from(std::env::var_os("HOME")?).join(".config"),
        };
        Some(base.join("nmtk").join("settings.toml"))
    }

    /// The thread count to actually use, resolving 0 against the machine.
    pub fn resolved_worker_threads(&self, machine: &MachineProfile) -> usize {
        if self.worker_threads == 0 {
            machine.default_worker_threads()
        } else {
            self.worker_threads.min(machine.logical_cores)
        }
    }
}

/// Why settings could not be written. Reading never produces an error — it produces defaults.
#[derive(Debug)]
pub enum SettingsError {
    /// Neither `XDG_CONFIG_HOME` nor `HOME` is set.
    NoConfigDirectory,
    /// The settings could not be turned into text.
    Encode,
    /// The file or its directory could not be written.
    Write(std::io::Error),
}

impl std::fmt::Display for SettingsError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SettingsError::NoConfigDirectory => write!(f, "no config directory (HOME is not set)"),
            SettingsError::Encode => write!(f, "settings could not be encoded"),
            SettingsError::Write(e) => write!(f, "settings could not be written: {e}"),
        }
    }
}

impl std::error::Error for SettingsError {}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(cores: usize) -> MachineProfile {
        MachineProfile { logical_cores: cores, total_memory_bytes: 0, available_memory_bytes: 0 }
    }

    #[test]
    fn zero_threads_means_ask_the_machine() {
        let settings = Settings::default();
        assert_eq!(settings.resolved_worker_threads(&machine(12)), 11);
    }

    #[test]
    fn a_chosen_thread_count_cannot_exceed_the_machine() {
        let settings = Settings { worker_threads: 64, ..Settings::default() };
        assert_eq!(settings.resolved_worker_threads(&machine(8)), 8);
    }

    #[test]
    fn a_damaged_file_gives_defaults_rather_than_a_crash() {
        let parsed: Settings = toml::from_str("language = \"klingon\"").unwrap_or_default();
        assert_eq!(parsed, Settings::default());
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let settings = Settings { language: Language::KOREAN, worker_threads: 3, colour: false };
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    #[test]
    fn an_unknown_field_does_not_throw_the_rest_away() {
        let text = "language = \"ko\"\nfuture-option = 7\n";
        let parsed: Settings = toml::from_str(text).unwrap();
        assert_eq!(parsed.language, Language::KOREAN);
    }

}
