//! What the reader changes while the program runs, and where it is kept.
//!
//! Settings are written to the user's config directory so a reader who raises the thread count
//! once does not have to raise it again tomorrow. A missing file is a first launch. A file with a
//! line nmtk cannot read is not an error either: nmtk keeps every line it can read, starts the
//! rest from their defaults, and copies the file as it found it to `settings.toml.bad` before
//! anything can write over it.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize, Serializer};

use crate::language::Language;
use crate::machine::MachineProfile;

/// Everything the reader can change and keep.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(default, rename_all = "kebab-case")]
pub struct Settings {
    pub language: Language,
    /// Threads long-running work may use. 0 means "decide from the machine".
    pub worker_threads: usize,
    /// Whether to use colour at all. Off gives a screen that reads on a monochrome terminal.
    pub colour: bool,
    /// Whether `colour` is the reader's own choice, made on the settings screen or the first
    /// launch, rather than what `NO_COLOR` or the default made it.
    ///
    /// Only a choice is written to the file. Every save used to write `colour`, so the variable
    /// stopped counting after the first launch: a reader who set `NO_COLOR` later still got
    /// colour, and one whose first launch ran under it had colour off for good.
    pub colour_chosen: bool,
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
            colour_chosen: false,
            finished: Vec::new(),
        }
    }
}

/// The file as it is written: colour only when the reader chose it, and then with the mark that
/// says so, because files written before this one carry `colour = true` whether anyone chose it
/// or not.
#[derive(Serialize)]
#[serde(rename_all = "kebab-case")]
struct OnDisk<'a> {
    language: Language,
    worker_threads: usize,
    #[serde(skip_serializing_if = "Option::is_none")]
    colour: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    colour_chosen: Option<bool>,
    finished: &'a [String],
}

impl Serialize for Settings {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        OnDisk {
            language: self.language,
            worker_threads: self.worker_threads,
            colour: self.colour_chosen.then_some(self.colour),
            colour_chosen: self.colour_chosen.then_some(true),
            finished: &self.finished,
        }
        .serialize(serializer)
    }
}

/// What reading the settings file found, for the caller that has to say something about it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Loaded {
    pub settings: Settings,
    /// There was no file: nmtk has never run here, and this is the moment to ask.
    pub first_launch: bool,
    /// What became of a file that could not all be read. `None` when it read whole.
    pub damage: Option<Damage>,
}

/// A settings file with something in it nmtk could not read.
///
/// Whatever could be read is used. The next save writes out only what nmtk understood, so the
/// file as it was found is copied aside first rather than lost without a word.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Damage {
    /// The file as found is kept here, beside the settings.
    KeptAs(PathBuf),
    /// The copy could not be made. The next save writes over the file.
    NotKept,
}

impl Settings {
    /// Settings from disk, or defaults. Never fails and never blocks startup.
    pub fn load() -> Self {
        Self::load_reporting().settings
    }

    /// The same, and whether nothing was found to read.
    ///
    /// A reader with no settings file has never run nmtk before, and that is the one moment worth
    /// stopping to ask them what language they read in. Afterwards the file exists and the
    /// question is never asked again.
    pub fn load_saying_whether_it_is_the_first_time() -> (Self, bool) {
        let loaded = Self::load_reporting();
        (loaded.settings, loaded.first_launch)
    }

    /// Settings from disk, whether this is the first launch, and what became of a damaged file.
    pub fn load_reporting() -> Loaded {
        let colour = !no_color_requested(std::env::var_os("NO_COLOR").as_deref());
        Self::load_from(Self::path().as_deref(), colour)
    }

    /// Reads one named file. `colour` is what colour is unless the file holds the reader's own
    /// choice: off when the terminal asked for no colour with `NO_COLOR`, on otherwise.
    ///
    /// A file that exists but does not read is not a first launch: the reader has been here
    /// before, and asking them to set nmtk up again because a line got damaged would be rude.
    fn load_from(path: Option<&Path>, colour: bool) -> Loaded {
        let fresh = || Loaded {
            settings: Self { colour, ..Self::default() },
            first_launch: true,
            damage: None,
        };
        let Some(path) = path else { return fresh() };
        let (settings, damaged) = match fs::read(path) {
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return fresh(),
            // There, but not readable at all: nothing can be recovered, and it is still theirs.
            Err(_) => (Self { colour, ..Self::default() }, true),
            Ok(bytes) => match String::from_utf8(bytes) {
                Ok(text) => Self::read(&text, colour),
                Err(not_text) => {
                    (Self::read(&String::from_utf8_lossy(not_text.as_bytes()), colour).0, true)
                }
            },
        };
        let damage = damaged.then(|| keep_aside(path));
        Loaded { settings, first_launch: false, damage }
    }

    /// Settings from the text of a settings file. See [`Settings::read`].
    #[cfg(test)]
    fn parse(text: &str, colour: bool) -> Self {
        Self::read(text, colour).0
    }

    /// Settings from the text of a settings file, one field at a time, and whether anything in
    /// it could not be read.
    ///
    /// Each field that reads is kept and each that does not falls back on its own. Reading the
    /// file as one whole used to mean that one bad line — a language this build does not have, a
    /// thread count written as `-1`, a value missing its quotes — threw every other line away
    /// with it, including the list of quests the reader had finished, and the next save wrote the
    /// loss to disk. A file that is not valid TOML is read entry by entry, so the damage costs
    /// the lines it is on and no others.
    ///
    /// `colour` is what colour is unless the file holds the reader's own choice: off when the
    /// terminal asked for no colour with `NO_COLOR`, on otherwise.
    fn read(text: &str, colour: bool) -> (Self, bool) {
        let mut settings = Self { colour, ..Self::default() };
        let (table, whole) = table_of(text);
        let mut damaged = !whole;
        let field = |key: &str| table.get(key).cloned();
        // A key that is there but cannot be used is damage too: the next save would drop it.
        let mut unread = |present: bool, used: bool| damaged |= present && !used;

        let language = field("language");
        let read_language = language.clone().and_then(|value| value.try_into().ok());
        unread(language.is_some(), read_language.is_some());
        if let Some(language) = read_language {
            settings.language = language;
        }

        let threads = field("worker-threads");
        let read_threads = threads.as_ref().and_then(toml::Value::as_integer);
        unread(threads.is_some(), read_threads.is_some());
        if let Some(threads) = read_threads {
            // Negative is nonsense and reads as "decide from the machine"; anything past the
            // largest machine anyone has is held there rather than trusted.
            settings.worker_threads = usize::try_from(threads).unwrap_or(0).min(MAX_THREADS);
        }

        let written = field("colour");
        let read_colour = written.as_ref().and_then(toml::Value::as_bool);
        let mark = field("colour-chosen");
        let read_mark = mark.as_ref().and_then(toml::Value::as_bool);
        unread(written.is_some(), read_colour.is_some());
        unread(mark.is_some(), read_mark.is_some());
        match (read_colour, read_mark) {
            // Written since colour is kept only when chosen: the reader's choice, and it wins.
            (Some(on), Some(true)) => {
                settings.colour = on;
                settings.colour_chosen = true;
            }
            // 0.4.0 and 0.5.0 wrote `colour = true` into every file, chosen or not. It is the
            // default rather than a choice, and NO_COLOR outranks a default.
            (Some(true), _) => {}
            // Off was never written unless somebody turned it off — the reader, or NO_COLOR on
            // a first launch — and nothing tells the two apart, so it stays off, as it was.
            (Some(false), _) => {
                settings.colour = false;
                settings.colour_chosen = true;
            }
            (None, _) => {}
        }

        if let Some(done) = field("finished") {
            match done {
                toml::Value::Array(done) => {
                    // One entry that is not a quest id does not cost the reader the others.
                    let ids: Vec<String> =
                        done.iter().filter_map(|id| id.as_str().map(str::to_string)).collect();
                    unread(true, ids.len() == done.len());
                    settings.finished = ids;
                }
                _ => unread(true, false),
            }
        }
        (settings, damaged)
    }

    /// Writes the settings, creating the directory if needed.
    pub fn save(&self) -> Result<(), SettingsError> {
        let path = Self::path().ok_or(SettingsError::NoConfigDirectory)?;
        self.save_to(&path)
    }

    fn save_to(&self, path: &Path) -> Result<(), SettingsError> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).map_err(SettingsError::Write)?;
        }
        let text = toml::to_string_pretty(self).map_err(|_| SettingsError::Encode)?;
        // Written beside the file and renamed over it, so a program killed mid-write leaves the
        // old settings rather than half of the new ones.
        let partial = path.with_extension("toml.partial");
        fs::write(&partial, text).map_err(SettingsError::Write)?;
        fs::rename(&partial, path).map_err(|e| {
            let _ = fs::remove_file(&partial);
            SettingsError::Write(e)
        })
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

    /// Sets colour on or off as the reader's own choice, which the file keeps and `NO_COLOR` no
    /// longer overrides.
    pub fn choose_colour(&mut self, on: bool) {
        self.colour = on;
        self.colour_chosen = true;
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
            // At least one: a profile built by hand, or read from a machine that reported
            // nothing, can say zero cores, and zero threads is a run that never starts.
            self.worker_threads.min(machine.logical_cores).max(1)
        }
    }
}

/// The file's entries as one table, and whether the file read as TOML all at once.
///
/// When it does not, each entry is read on its own and the ones that read are kept. An entry is
/// a line that starts `key =` and every line after it up to the next one, so a list written over
/// several lines is read whole.
fn table_of(text: &str) -> (toml::Table, bool) {
    if let Ok(table) = toml::from_str::<toml::Table>(text) {
        return (table, true);
    }
    let mut entries: Vec<String> = Vec::new();
    for line in text.lines() {
        match entries.last_mut() {
            Some(entry) if !starts_an_entry(line) => {
                entry.push('\n');
                entry.push_str(line);
            }
            _ => entries.push(line.to_string()),
        }
    }
    let mut table = toml::Table::new();
    for entry in entries {
        if let Ok(read) = toml::from_str::<toml::Table>(&entry) {
            for (key, value) in read {
                // The first of two lines naming the same key is the one kept.
                table.entry(key).or_insert(value);
            }
        }
    }
    (table, false)
}

/// Whether a line starts a `key = value` entry.
fn starts_an_entry(line: &str) -> bool {
    let line = line.trim_start();
    let key = line.len()
        - line
            .trim_start_matches(|c: char| c.is_ascii_alphanumeric() || c == '-' || c == '_')
            .len();
    key > 0 && line[key..].trim_start().starts_with('=')
}

/// Copies a settings file nmtk could not read beside itself as `settings.toml.bad`, before
/// anything writes over it.
///
/// Done as soon as the damage is found, so it always comes before the first save. A later launch
/// that finds the same damage copies the same file again; one that finds the file readable leaves
/// the copy alone.
fn keep_aside(path: &Path) -> Damage {
    let bad = path.with_extension("toml.bad");
    match fs::copy(path, &bad) {
        Ok(_) => Damage::KeptAs(bad),
        Err(_) => Damage::NotKept,
    }
}

/// The most threads a settings file is believed about. Past this the number is a typo.
pub const MAX_THREADS: usize = 1024;

/// Whether the `NO_COLOR` convention asks for no colour: the variable is set and not empty.
///
/// See <https://no-color.org>. It decides colour on every launch until the reader chooses colour
/// themselves in settings; once they have, the choice is theirs.
pub fn no_color_requested(value: Option<&std::ffi::OsStr>) -> bool {
    value.is_some_and(|value| !value.is_empty())
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

    /// A directory of its own under the temp dir, emptied first, for tests that touch files.
    fn scratch(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("nmtk-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).expect("a directory under the temp dir");
        dir
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
        let parsed = Settings::parse("language = \"klingon\"", true);
        assert_eq!(parsed, Settings::default());
        assert_eq!(Settings::parse("this is not toml at all", true), Settings::default());
    }

    #[test]
    fn settings_survive_a_round_trip() {
        let settings = Settings {
            language: Language::KOREAN,
            worker_threads: 3,
            colour: false,
            colour_chosen: true,
            finished: vec!["consensus.proof-of-work".to_string()],
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(toml::from_str::<Settings>(&text).unwrap(), settings);
    }

    /// The first launch is the only moment nmtk asks anything, so it has to be told apart from
    /// every launch after it — including from a launch whose file is damaged.
    #[test]
    fn a_missing_file_is_a_first_launch_and_a_damaged_one_is_not() {
        let dir = scratch("first-launch");
        let path = dir.join("settings.toml");
        let loaded = Settings::load_from(Some(&path), true);
        assert!(loaded.first_launch, "a file that is not there was read");
        assert_eq!(loaded.damage, None);
        assert!(Settings::load_from(None, true).first_launch, "no config directory at all");

        fs::write(&path, "this is not toml at all\n").expect("a file under the temp dir");
        let loaded = Settings::load_from(Some(&path), true);
        assert!(!loaded.first_launch, "a damaged file should give defaults, not a fresh setup");
        assert_eq!(loaded.settings, Settings::default());

        fs::write(&path, "language = \"ko\"\n").expect("a file under the temp dir");
        let loaded = Settings::load_from(Some(&path), true);
        assert_eq!(loaded.settings.language, Language::KOREAN);
        assert_eq!(loaded.damage, None, "a file that read whole is not damaged");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn an_unknown_field_does_not_throw_the_rest_away() {
        let text = "language = \"ko\"\nfuture-option = 7\n";
        let parsed: Settings = toml::from_str(text).unwrap();
        assert_eq!(parsed.language, Language::KOREAN);
        let (parsed, damaged) = Settings::read(text, true);
        assert_eq!(parsed.language, Language::KOREAN);
        assert!(!damaged, "a key from a newer nmtk is not damage");
    }

    /// One damaged line used to throw away every other line, the finished quests included.
    #[test]
    fn one_bad_line_keeps_the_rest_of_the_file() {
        let text = "language = \"klingon\"\nworker-threads = 3\ncolour = false\n\
                    finished = [\"consensus.proof-of-work\", 7]\n";
        let parsed = Settings::parse(text, true);
        assert_eq!(parsed.language, Language::ENGLISH);
        assert_eq!(parsed.worker_threads, 3);
        assert!(!parsed.colour);
        assert_eq!(parsed.finished, vec!["consensus.proof-of-work".to_string()]);

        let parsed = Settings::parse("language = \"ko\"\nworker-threads = -4\n", true);
        assert_eq!(parsed.language, Language::KOREAN);
        assert_eq!(parsed.worker_threads, 0, "a negative count reads as automatic");
        let parsed = Settings::parse("worker-threads = 99999999\n", true);
        assert_eq!(parsed.worker_threads, MAX_THREADS);
    }

    /// `language = ko`, unquoted, is not TOML, and the file used to be read as a whole or not at
    /// all: every setting fell back, and the next save wrote the finished list away.
    #[test]
    fn a_syntax_error_costs_only_the_line_it_is_on() {
        let text = "language = ko\nworker-threads = 5\ncolour = false\n\
                    finished = [\n    \"consensus.proof-of-work\",\n    \"ledgers.models\",\n]\n";
        assert!(toml::from_str::<toml::Table>(text).is_err(), "the file is meant to be broken");
        let (parsed, damaged) = Settings::read(text, true);
        assert!(damaged, "a line that did not read was not noticed");
        assert_eq!(parsed.language, Language::ENGLISH, "the broken line falls back");
        assert_eq!(parsed.worker_threads, 5);
        assert!(!parsed.colour);
        assert_eq!(
            parsed.finished,
            vec!["consensus.proof-of-work".to_string(), "ledgers.models".to_string()],
            "a list over several lines was lost"
        );

        // Two errors, one of them inside the list: the list goes, the rest stays.
        let text = "worker-threads = = 2\nlanguage = \"ko\"\nfinished = [\"a.b\",, ]\n";
        let (parsed, damaged) = Settings::read(text, true);
        assert!(damaged);
        assert_eq!(parsed.language, Language::KOREAN);
        assert_eq!(parsed.worker_threads, 0);
        assert!(parsed.finished.is_empty());

        // What nmtk writes itself, list and all, reads whole and is not damage.
        let written = Settings {
            finished: vec!["a.b".to_string(), "c.d".to_string()],
            ..Settings::default()
        };
        let text = toml::to_string_pretty(&written).unwrap();
        assert_eq!(Settings::read(&text, true), (written, false), "{text}");
    }

    /// The file as the reader left it is copied aside before anything writes over it, and a file
    /// that reads whole leaves the copy alone.
    #[test]
    fn a_damaged_file_is_kept_as_settings_toml_bad_before_it_is_written_over() {
        let dir = scratch("damaged");
        let path = dir.join("settings.toml");
        let bad = dir.join("settings.toml.bad");
        let damaged = "language = ko\nfinished = [\"consensus.proof-of-work\"]\n";
        fs::write(&path, damaged).expect("a file under the temp dir");

        let loaded = Settings::load_from(Some(&path), true);
        assert!(!loaded.first_launch);
        assert_eq!(loaded.damage, Some(Damage::KeptAs(bad.clone())));
        assert_eq!(fs::read_to_string(&bad).expect("the copy"), damaged);
        assert_eq!(loaded.settings.finished, vec!["consensus.proof-of-work".to_string()]);

        // The first save writes what was understood, and the reader's text is still in the copy.
        loaded.settings.save_to(&path).expect("a save under the temp dir");
        assert_eq!(fs::read_to_string(&bad).expect("the copy"), damaged);
        let saved = Settings::load_from(Some(&path), true);
        assert_eq!(saved.damage, None, "what nmtk wrote is readable");
        assert_eq!(saved.settings, loaded.settings, "the save lost what was recovered");
        assert_eq!(fs::read_to_string(&bad).expect("the copy"), damaged, "the copy was touched");

        // Bytes that are not text at all are damage, not a first launch.
        fs::write(&path, [0xff, 0xfe, b'\n']).expect("a file under the temp dir");
        let loaded = Settings::load_from(Some(&path), true);
        assert!(!loaded.first_launch);
        assert_eq!(loaded.damage, Some(Damage::KeptAs(bad.clone())));
        assert_eq!(fs::read(&bad).expect("the copy"), vec![0xff, 0xfe, b'\n']);

        // A copy that cannot be made is said, not assumed: here the name is taken by a directory.
        fs::remove_file(&bad).expect("the old copy");
        fs::create_dir(&bad).expect("a directory in the copy's place");
        fs::write(&path, damaged).expect("a file under the temp dir");
        assert_eq!(Settings::load_from(Some(&path), true).damage, Some(Damage::NotKept));
        let _ = fs::remove_dir_all(&dir);
    }

    /// `nmtk --help` promises that NO_COLOR starts nmtk with colour off until the reader chooses
    /// otherwise. Every save used to write `colour`, so it stopped counting after the first one.
    #[test]
    fn no_color_starts_colour_off_until_the_reader_chooses_colour() {
        let (with_no_color, without) = (false, true);

        // Written by 0.4.0 or 0.5.0: `colour = true` in every file, chosen or not.
        let old = "language = \"ko\"\nworker-threads = 0\ncolour = true\nfinished = []\n";
        assert!(!Settings::parse(old, with_no_color).colour, "NO_COLOR lost to an old file");
        assert!(Settings::parse(old, without).colour);
        assert!(!Settings::parse(old, without).colour_chosen, "a default is not a choice");

        // Chosen under s, either way, with or without the variable.
        for on in [true, false] {
            let mut chosen = Settings::default();
            chosen.choose_colour(on);
            let text = toml::to_string_pretty(&chosen).unwrap();
            for no_color in [with_no_color, without] {
                let read = Settings::parse(&text, no_color);
                assert_eq!(read.colour, on, "the reader's choice lost to the environment");
                assert!(read.colour_chosen);
            }
        }

        // Not chosen: nothing about colour is written, so the variable decides every launch.
        let first_launch_under_no_color = Settings { colour: false, ..Settings::default() };
        let text = toml::to_string_pretty(&first_launch_under_no_color).unwrap();
        assert!(!text.contains("colour"), "an unchosen colour was written: {text}");
        assert!(!Settings::parse(&text, with_no_color).colour);
        assert!(Settings::parse(&text, without).colour, "NO_COLOR outlived the launch it was on");

        // No file at all: the first launch.
        let dir = scratch("no-color");
        let path = dir.join("settings.toml");
        assert!(!Settings::load_from(Some(&path), with_no_color).settings.colour);
        assert!(Settings::load_from(Some(&path), without).settings.colour);
        let _ = fs::remove_dir_all(&dir);

        assert!(no_color_requested(Some(std::ffi::OsStr::new("1"))));
        assert!(!no_color_requested(Some(std::ffi::OsStr::new(""))), "empty means unset");
        assert!(!no_color_requested(None));
    }

    #[test]
    fn a_thread_count_is_never_zero_even_on_a_machine_that_reports_none() {
        let settings = Settings { worker_threads: 4, ..Settings::default() };
        assert_eq!(settings.resolved_worker_threads(&machine(0)), 1);
        assert_eq!(Settings::default().resolved_worker_threads(&machine(0)), 1);
    }

    /// What `save` writes, `parse` reads back whole.
    #[test]
    fn what_is_written_is_what_is_read() {
        let settings = Settings {
            language: Language::KOREAN,
            worker_threads: 5,
            colour: false,
            colour_chosen: true,
            finished: vec!["a.b".to_string(), "c.d".to_string()],
        };
        let text = toml::to_string_pretty(&settings).unwrap();
        assert_eq!(Settings::parse(&text, true), settings);
    }
}
