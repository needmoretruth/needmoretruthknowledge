//! What every Knowledge Quest declares about itself.
//!
//! A KQ list has to sort and filter without opening anything, so these facts are plain data,
//! available before a quest runs and cheap to copy. Nothing here is a sentence: the words a reader
//! sees come from the quest's own phrase table in the reader's language.

use nmtk_core::MachineProfile;

/// A quest's permanent name: `area.topic`, lowercase, dots and dashes only.
///
/// The id never changes once a quest ships — it is how a reader's progress, and an older version
/// of the same quest, stay attached to it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct KqId(pub &'static str);

impl KqId {
    pub fn as_str(self) -> &'static str {
        self.0
    }
}

impl std::fmt::Display for KqId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}

/// Which edition of a quest this is.
///
/// `major` rises when the quest teaches something differently enough that a reader who knows the
/// old one would be surprised; `minor` rises for everything else. Both editions stay in the
/// program — a reader can always open the version they learned from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct KqVersion {
    pub major: u16,
    pub minor: u16,
}

impl KqVersion {
    pub const fn new(major: u16, minor: u16) -> Self {
        Self { major, minor }
    }
}

impl std::fmt::Display for KqVersion {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}", self.major, self.minor)
    }
}

/// A calendar day, written the way the whole world can read it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Date {
    pub year: u16,
    pub month: u8,
    pub day: u8,
}

impl Date {
    pub const fn new(year: u16, month: u8, day: u8) -> Self {
        Self { year, month, day }
    }
}

impl std::fmt::Display for Date {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

/// The shelf a quest sits on. A reader browses by this first.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Category {
    /// How a network agrees on what happened.
    Consensus,
    /// How ownership is recorded.
    Ledgers,
    /// What can be proved, hidden or signed.
    Cryptography,
    /// How machines are made to learn.
    MachineLearning,
    /// How machines talk to each other.
    Networking,
    /// What the machine underneath is really doing.
    Systems,
}

impl Category {
    pub const ALL: [Category; 6] = [
        Category::Consensus,
        Category::Ledgers,
        Category::Cryptography,
        Category::MachineLearning,
        Category::Networking,
        Category::Systems,
    ];

    /// A stable key, used for sorting and for the first part of a quest id.
    pub fn key(self) -> &'static str {
        match self {
            Category::Consensus => "consensus",
            Category::Ledgers => "ledgers",
            Category::Cryptography => "cryptography",
            Category::MachineLearning => "machine-learning",
            Category::Networking => "networking",
            Category::Systems => "systems",
        }
    }
}

/// How much a reader is expected to bring with them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    /// No background needed.
    Gentle,
    /// Assumes the basics of the category.
    Steady,
    /// Assumes the reader has done the gentler quests in this category.
    Steep,
}

impl Difficulty {
    pub const ALL: [Difficulty; 3] = [Difficulty::Gentle, Difficulty::Steady, Difficulty::Steep];

    /// Three marks that carry the level without colour.
    pub fn marks(self) -> &'static str {
        match self {
            Difficulty::Gentle => "•··",
            Difficulty::Steady => "••·",
            Difficulty::Steep => "•••",
        }
    }
}

/// What a quest needs from the machine to be worth running.
///
/// A quest that does not meet these still opens — the list says so, and the quest sizes itself
/// down — because refusing to run is a worse lesson than running slowly.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirements {
    pub cores: usize,
    pub memory_bytes: u64,
}

impl Requirements {
    /// Nothing in particular: any machine that can run nmtk can run this.
    pub const ANY: Self = Self { cores: 1, memory_bytes: 0 };

    pub const fn new(cores: usize, memory_bytes: u64) -> Self {
        Self { cores, memory_bytes }
    }

    pub fn met_by(&self, machine: &MachineProfile) -> bool {
        machine.logical_cores >= self.cores
            && (machine.total_memory_bytes == 0 || machine.total_memory_bytes >= self.memory_bytes)
    }
}

/// The five steps a quest moves through. A quest may skip any but `Brief` and `Run`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum StageKind {
    /// Why this is worth an hour of anyone's life. One screen, no jargon.
    Brief,
    /// The real thing, running on this machine.
    Run,
    /// The reader changes values and watches the run answer.
    Tune,
    /// The reader attacks it, and finds out what holds and what does not.
    Break,
    /// What just happened, in the order it happened.
    Recap,
}

impl StageKind {
    pub const ALL: [StageKind; 5] =
        [StageKind::Brief, StageKind::Run, StageKind::Tune, StageKind::Break, StageKind::Recap];

    /// The digit key that jumps straight here.
    pub fn digit(self) -> char {
        match self {
            StageKind::Brief => '1',
            StageKind::Run => '2',
            StageKind::Tune => '3',
            StageKind::Break => '4',
            StageKind::Recap => '5',
        }
    }
}

/// Everything a quest declares before it is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KqMeta {
    pub id: KqId,
    pub version: KqVersion,
    /// When this quest first shipped, in any version.
    pub released: Date,
    /// When this version shipped.
    pub updated: Date,
    pub category: Category,
    /// A stable key for the narrower shelf, e.g. `proof-of-work`. Displayed through the quest's
    /// own phrase table, never shown raw.
    pub subcategory: &'static str,
    pub difficulty: Difficulty,
    /// Roughly how long a first pass takes, in minutes. An honest estimate, not a promise.
    pub minutes: u16,
    pub needs: Requirements,
    /// The stages this quest actually has, in the order they are offered.
    pub stages: &'static [StageKind],
    /// Stable keys for searching, e.g. `bitcoin`, `hashing`.
    pub tags: &'static [&'static str],
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dates_are_written_the_same_way_everywhere() {
        assert_eq!(Date::new(2026, 9, 12).to_string(), "2026-09-12");
    }

    #[test]
    fn versions_order_by_major_then_minor() {
        assert!(KqVersion::new(1, 9) < KqVersion::new(2, 0));
        assert!(KqVersion::new(1, 2) < KqVersion::new(1, 10));
    }

    #[test]
    fn a_machine_with_unknown_memory_is_not_refused() {
        let machine =
            MachineProfile { logical_cores: 4, total_memory_bytes: 0, available_memory_bytes: 0 };
        assert!(Requirements::new(4, 64 * 1024 * 1024 * 1024).met_by(&machine));
    }

    #[test]
    fn requirements_look_at_both_cores_and_memory() {
        let machine = MachineProfile {
            logical_cores: 2,
            total_memory_bytes: 8 * 1024 * 1024 * 1024,
            available_memory_bytes: 0,
        };
        assert!(!Requirements::new(4, 0).met_by(&machine));
        assert!(Requirements::ANY.met_by(&machine));
    }

    #[test]
    fn every_stage_has_its_own_digit() {
        let digits: Vec<char> = StageKind::ALL.iter().map(|s| s.digit()).collect();
        let mut unique = digits.clone();
        unique.sort_unstable();
        unique.dedup();
        assert_eq!(digits.len(), unique.len());
    }
}
