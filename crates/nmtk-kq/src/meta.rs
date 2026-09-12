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

/// A version of nmtk, and of any quest shipped in it.
///
/// There is one number for the whole program. A quest's version is **the version of nmtk it was
/// last shipped in**, so "which nmtk was this lesson written for" needs no cross-referencing.
/// It rises only when something is pushed, never while work is in progress, and `1.0.0` is not
/// reached until the owner says so.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Version {
    pub major: u16,
    pub minor: u16,
    pub patch: u16,
}

impl Version {
    pub const fn new(major: u16, minor: u16, patch: u16) -> Self {
        Self { major, minor, patch }
    }
}

impl std::fmt::Display for Version {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}.{}.{}", self.major, self.minor, self.patch)
    }
}

/// A moment in UTC, to the second.
///
/// Quests are updated several times a day while they are being written, so a date alone stops
/// telling two versions apart. UTC because a reader in Seoul and a reader in Berlin have to see the
/// same stamp.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub struct Stamp {
    pub year: u16,
    pub month: u8,
    pub day: u8,
    pub hour: u8,
    pub minute: u8,
    pub second: u8,
}

impl Stamp {
    pub const fn new(year: u16, month: u8, day: u8, hour: u8, minute: u8, second: u8) -> Self {
        Self { year, month, day, hour, minute, second }
    }

    /// Just the day, for a list that has no room for the time.
    pub fn date(&self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }
}

impl std::fmt::Display for Stamp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
            self.year, self.month, self.day, self.hour, self.minute, self.second
        )
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

/// How hard the thinking is — not how long it takes, and not what the machine has to do.
///
/// Five steps, because three hid the difference between "anyone can follow this" and "you should
/// have done the gentler one first".
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Difficulty {
    /// No background at all. Follow along and it makes sense.
    VeryEasy,
    /// A little patience, nothing to know beforehand.
    Easy,
    /// Assumes the basics of the category.
    Medium,
    /// Assumes the gentler quests in this category.
    Hard,
    /// Assumes the rest of this category, and some stamina.
    VeryHard,
}

impl Difficulty {
    pub const ALL: [Difficulty; 5] = [
        Difficulty::VeryEasy,
        Difficulty::Easy,
        Difficulty::Medium,
        Difficulty::Hard,
        Difficulty::VeryHard,
    ];

    /// Five marks, so the level reads without colour.
    pub fn marks(self) -> &'static str {
        match self {
            Difficulty::VeryEasy => "•····",
            Difficulty::Easy => "••···",
            Difficulty::Medium => "•••··",
            Difficulty::Hard => "••••·",
            Difficulty::VeryHard => "•••••",
        }
    }
}

/// What a quest wants from the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineNeeds {
    pub cores: usize,
    pub memory_bytes: u64,
}

impl MachineNeeds {
    pub const fn new(cores: usize, memory_bytes: u64) -> Self {
        Self { cores, memory_bytes }
    }

    /// Anything that runs nmtk at all.
    pub const ANY: Self = Self { cores: 1, memory_bytes: 0 };

    fn met_by(&self, machine: &MachineProfile) -> bool {
        machine.logical_cores >= self.cores
            && (machine.total_memory_bytes == 0 || machine.total_memory_bytes >= self.memory_bytes)
    }
}

/// The two bars a quest sets: what it needs to run at all, and what it wants to be worth doing.
///
/// Both are declared per quest, never per stage — a reader who started a quest on this machine
/// must be able to finish it, so a later stage may not raise the bar.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Requirements {
    pub minimum: MachineNeeds,
    pub recommended: MachineNeeds,
}

impl Requirements {
    /// A quest that runs anywhere.
    pub const ANY: Self = Self { minimum: MachineNeeds::ANY, recommended: MachineNeeds::ANY };

    pub const fn new(minimum: MachineNeeds, recommended: MachineNeeds) -> Self {
        Self { minimum, recommended }
    }

    /// How this machine measures up.
    pub fn fit(&self, machine: &MachineProfile) -> Fit {
        if self.recommended.met_by(machine) {
            Fit::Recommended
        } else if self.minimum.met_by(machine) {
            Fit::Minimum
        } else {
            Fit::Below
        }
    }
}

/// Where this machine sits against a quest's two bars.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// At or above what the quest wants.
    Recommended,
    /// Enough to run it; some of it will be slow.
    Minimum,
    /// Below the minimum. The quest still opens and sizes itself down, and says so.
    Below,
}

/// What a stage is for. The shape is fixed so four quests feel like one program; how many of each
/// a quest has is up to the quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StageRole {
    /// Why this matters, and what is about to happen.
    Explain,
    /// The real thing running.
    Run,
    /// The reader changes values and the run answers.
    Tune,
    /// The reader attacks it.
    Break,
    /// What just happened, in order.
    Recap,
}

/// One stage of a quest.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StageSpec {
    /// Stable key, used by the quest's phrase table for the stage's name.
    pub key: &'static str,
    pub role: StageRole,
    /// How hard *this stage* is. A quest may open gently and end steeply.
    pub difficulty: Difficulty,
}

impl StageSpec {
    pub const fn new(key: &'static str, role: StageRole, difficulty: Difficulty) -> Self {
        Self { key, role, difficulty }
    }
}

/// The fewest stages a quest may have. Two stages is a screen with a footnote, not a lesson.
pub const MIN_STAGES: usize = 3;

/// Everything a quest declares before it is opened.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KqMeta {
    pub id: KqId,
    /// The nmtk version this quest was last shipped in.
    pub version: Version,
    /// When the quest first shipped, in any version.
    pub released: Stamp,
    /// When this version shipped.
    pub updated: Stamp,
    pub category: Category,
    /// A stable key for the narrower shelf, e.g. `proof-of-work`. Displayed through the quest's
    /// own phrase table, never shown raw.
    pub subcategory: &'static str,
    /// The quest as a whole. Individual stages may be gentler or steeper.
    pub difficulty: Difficulty,
    /// Roughly how long a first pass takes, in minutes. An honest estimate, not a promise.
    pub minutes: u16,
    pub needs: Requirements,
    /// The stages this quest has, in order. At least [`MIN_STAGES`] of them.
    pub stages: &'static [StageSpec],
    /// Stable keys for searching, e.g. `bitcoin`, `hashing`.
    pub tags: &'static [&'static str],
}

impl KqMeta {
    /// Whether this quest is shaped like a quest. Checked when the catalogue is built.
    pub fn is_well_formed(&self) -> bool {
        self.stages.len() >= MIN_STAGES && self.minutes > 0 && !self.id.as_str().is_empty()
    }

    /// The hardest stage in the quest, which is what a reader is really signing up for.
    pub fn steepest_stage(&self) -> Difficulty {
        self.stages.iter().map(|stage| stage.difficulty).max().unwrap_or(self.difficulty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn machine(cores: usize, gib: u64) -> MachineProfile {
        MachineProfile {
            logical_cores: cores,
            total_memory_bytes: gib * 1024 * 1024 * 1024,
            available_memory_bytes: 0,
        }
    }

    const STAGES: [StageSpec; 3] = [
        StageSpec::new("why", StageRole::Explain, Difficulty::VeryEasy),
        StageSpec::new("run", StageRole::Run, Difficulty::Medium),
        StageSpec::new("recap", StageRole::Recap, Difficulty::Easy),
    ];

    fn meta() -> KqMeta {
        KqMeta {
            id: KqId("systems.example"),
            version: Version::new(0, 0, 1),
            released: Stamp::new(2026, 9, 12, 10, 22, 31),
            updated: Stamp::new(2026, 9, 12, 10, 22, 31),
            category: Category::Systems,
            subcategory: "example",
            difficulty: Difficulty::Medium,
            minutes: 20,
            needs: Requirements::ANY,
            stages: &STAGES,
            tags: &[],
        }
    }

    #[test]
    fn a_stamp_is_utc_to_the_second() {
        assert_eq!(Stamp::new(2026, 9, 12, 10, 22, 31).to_string(), "2026-09-12T10:22:31Z");
        assert_eq!(Stamp::new(2026, 9, 12, 10, 22, 31).date(), "2026-09-12");
    }

    #[test]
    fn versions_order_the_way_people_expect() {
        assert!(Version::new(0, 0, 9) < Version::new(0, 1, 0));
        assert!(Version::new(0, 2, 0) < Version::new(0, 10, 0));
    }

    #[test]
    fn a_quest_needs_three_stages() {
        assert!(meta().is_well_formed());
        const TWO: [StageSpec; 2] = [
            StageSpec::new("why", StageRole::Explain, Difficulty::Easy),
            StageSpec::new("run", StageRole::Run, Difficulty::Easy),
        ];
        let short = KqMeta { stages: &TWO, ..meta() };
        assert!(!short.is_well_formed(), "two stages passed as a quest");
    }

    #[test]
    fn the_steepest_stage_is_what_a_reader_is_signing_up_for() {
        assert_eq!(meta().steepest_stage(), Difficulty::Medium);
    }

    #[test]
    fn fit_separates_recommended_from_merely_possible() {
        let needs = Requirements::new(
            MachineNeeds::new(2, 2 * 1024 * 1024 * 1024),
            MachineNeeds::new(8, 8 * 1024 * 1024 * 1024),
        );
        assert_eq!(needs.fit(&machine(12, 16)), Fit::Recommended);
        assert_eq!(needs.fit(&machine(4, 4)), Fit::Minimum);
        assert_eq!(needs.fit(&machine(1, 1)), Fit::Below);
    }

    #[test]
    fn unknown_memory_never_holds_a_machine_back() {
        let unknown =
            MachineProfile { logical_cores: 8, total_memory_bytes: 0, available_memory_bytes: 0 };
        let needs = Requirements::new(
            MachineNeeds::new(2, 64 * 1024 * 1024 * 1024),
            MachineNeeds::new(4, 64 * 1024 * 1024 * 1024),
        );
        assert_eq!(needs.fit(&unknown), Fit::Recommended);
    }

    #[test]
    fn five_difficulties_each_have_their_own_marks() {
        let mut marks: Vec<&str> = Difficulty::ALL.iter().map(|d| d.marks()).collect();
        let total = marks.len();
        marks.sort_unstable();
        marks.dedup();
        assert_eq!(marks.len(), total);
    }
}
