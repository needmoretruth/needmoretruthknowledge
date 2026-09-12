//! What the reader changes while the program runs, and where it is kept.
//!
//! Settings are written to the user's config directory so a reader who raises the thread count
//! once does not have to raise it again tomorrow. A missing, unreadable or half-written file is
//! not an error: nmtk starts with defaults sized for the machine and says nothing.

use std::fs;
use std::path::{Path, PathBuf};

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
    /// Quests the reader has reached the end of, by id.
    ///
    /// The only thing nmtk remembers about what someone did. A shelf of four quests that looks
    /// identical after finishing one cannot tell the reader where they got to.
    pub finished: Vec<String>,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            language: Language::default(),
            worker_threads: 0,
            colour: true,
            finished: Vec::new(),
        }
    }
}

impl Settings {
    /// Settings from disk, or defaults. Never fails and never blocks startup.
    pub fn load() -> Self {
        Self::load_saying_whether_it_is_the_first_time().0
    }

    /// The same, and whether nothing was found to read.
    ///
    /// A reader with no settings file has never run nmtk before, and that is the one moment worth
    /// stopping to ask them what language they read in. Afterwards the file exists and the
    /// question is never asked again.
    pub fn load_saying_whether_it_is_the_first_time() -> (Self, bool) {
        match Self::path().and_then(|path| Self::read_from(&path)) {
            Some(settings) => (settings, false),
            None => (Self::default(), true),
        }
    }

    /// Settings from one named file, or nothing when there is no file to read.
    ///
    /// A file that exists but does not parse is not a first launch: the reader has been here
    /// before, and asking them to set nmtk up again because a line got damaged would be rude.
    fn read_from(path: &Path) -> Option<Self> {
        let text = fs::read_to_string(path).ok()?;
        Some(toml::from_str(&text).unwrap_or_default())
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

    /// Records that a quest was finished. Answers whether anything changed, so a caller knows
    /// whether it is worth writing the file.
    pub fn remember_finished(&mut self, id: &str) -> bool {
        if self.finished.iter().any(|done| done == id) {
            return false;
        }
        self.finished.push(id.to_string());
        true
    }

    pub fn has_finished(&self, id: &str) -> bool {
        self.finished.iter().any(|done| done == id)
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
        let settings = Settings {
            language: Language::KOREAN,
            worker_threads: 3,
            colour: false,
            finished: vec!["consensus.proof-of-work".to_string()],
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    /// The first launch is the only moment nmtk asks anything, so it has to be told apart from
    /// every launch after it — including from a launch whose file is damaged.
    #[test]
    fn a_missing_file_is_a_first_launch_and_a_damaged_one_is_not() {
        let dir = std::env::temp_dir().join(format!("nmtk-settings-{}", std::process::id()));
        let path = dir.join("settings.toml");
        let _ = fs::remove_file(&path);
        assert!(Settings::read_from(&path).is_none(), "a file that is not there was read");

        fs::create_dir_all(&dir).expect("a directory under the temp dir");
        fs::write(&path, "this is not toml at all\n").expect("a file under the temp dir");
        assert_eq!(
            Settings::read_from(&path),
            Some(Settings::default()),
            "a damaged file should give defaults, not a fresh setup"
        );

        fs::write(&path, "language = \"ko\"\n").expect("a file under the temp dir");
        assert_eq!(Settings::read_from(&path).map(|s| s.language), Some(Language::KOREAN));
        let _ = fs::remove_file(&path);
        let _ = fs::remove_dir(&dir);
    }

    #[test]
    fn an_unknown_field_does_not_throw_the_rest_away() {
        let text = "language = \"ko\"\nfuture-option = 7\n";
        let parsed: Settings = toml::from_str(text).unwrap();
        assert_eq!(parsed.language, Language::KOREAN);
    }
}
