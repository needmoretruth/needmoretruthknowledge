//! What this machine can do, read once at startup.
//!
//! nmtk sizes its own work: a laptop with four cores should not be handed the defaults of a
//! workstation with thirty-two. Every engine asks this profile for its starting values, and the
//! reader can override them in settings.

use std::fs;
use std::thread::available_parallelism;

/// A rough size class, used to pick starting values for work that scales with the machine.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum SizeClass {
    /// Up to 4 usable cores, or under 4 GiB of memory.
    Small,
    /// Up to 12 usable cores and at least 4 GiB.
    Medium,
    /// More than 12 usable cores and at least 16 GiB.
    Large,
}

/// What the program found out about the machine it is running on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MachineProfile {
    /// Logical cores the process may use. At least 1.
    pub logical_cores: usize,
    /// Total physical memory in bytes, or 0 when it could not be read.
    pub total_memory_bytes: u64,
    /// Memory the kernel says is available right now, or 0 when it could not be read.
    pub available_memory_bytes: u64,
}

impl MachineProfile {
    /// Reads the machine once. Never fails: unknown values come back as 0 and the callers that
    /// care fall back to their smallest setting.
    ///
    /// Cores come from the standard library, which already counts what this process may use
    /// rather than what the board has: a CPU affinity mask, or a container's cgroup CPU quota,
    /// both shrink the answer. Memory is read from `/proc/meminfo` and then held to the cgroup's
    /// memory limit, because a container given 2 GiB on a 64 GiB host that sizes its work to
    /// 64 GiB is killed by the kernel partway through a run, with no word said to the reader.
    pub fn detect() -> Self {
        let logical_cores = available_parallelism().map(|n| n.get()).unwrap_or(1).max(1);
        let (mut total, mut available) = read_meminfo().unwrap_or((0, 0));
        if let Some(limit) = read_cgroup_memory_limit() {
            total = if total == 0 { limit } else { total.min(limit) };
            available = if available == 0 { 0 } else { available.min(limit) };
        }
        Self { logical_cores, total_memory_bytes: total, available_memory_bytes: available }
    }

    /// The machine as work started with `threads` worker threads should see it.
    ///
    /// Quests size their threads from [`MachineProfile::default_worker_threads`], which leaves one
    /// core for the screen. A reader who chose a thread count in settings has already decided how
    /// many the work may take, so the profile handed to the work is one whose default is exactly
    /// that. Its size class follows, which is what a reader asking for fewer threads wants: work
    /// sized for the share of the machine they gave it, rather than an hour-long run on two cores.
    /// `0` means "decide from the machine", and hands the machine back unchanged.
    pub fn with_worker_threads(self, threads: usize) -> Self {
        if threads == 0 || threads == self.default_worker_threads() {
            return self;
        }
        Self { logical_cores: threads + 1, ..self }
    }

    /// Threads a long-running job may take by default.
    ///
    /// One core stays free so the screen keeps repainting while mining or training runs flat out.
    /// A single-core machine gets 1 — a stalled screen beats no work at all.
    pub fn default_worker_threads(&self) -> usize {
        self.logical_cores.saturating_sub(1).max(1)
    }

    /// The size class used to pick starting values.
    pub fn size_class(&self) -> SizeClass {
        const GIB: u64 = 1024 * 1024 * 1024;
        let memory_unknown = self.total_memory_bytes == 0;
        let cores = self.logical_cores;
        if cores > 12 && (memory_unknown || self.total_memory_bytes >= 16 * GIB) {
            SizeClass::Large
        } else if cores > 4 && (memory_unknown || self.total_memory_bytes >= 4 * GIB) {
            SizeClass::Medium
        } else {
            SizeClass::Small
        }
    }
}

/// Total and available memory from `/proc/meminfo`, in bytes.
///
/// The file reports kibibytes. Anything unreadable — a kernel without procfs, a sandbox — returns
/// `None` rather than a guess.
fn read_meminfo() -> Option<(u64, u64)> {
    parse_meminfo(&fs::read_to_string("/proc/meminfo").ok()?)
}

/// The two numbers nmtk wants out of the text of `/proc/meminfo`.
///
/// A line that does not look like `Key: value kB` is skipped rather than ending the search. The
/// first version gave up at the first such line, which on a kernel that adds one threw away the
/// memory it had already read and made every machine look like one with no memory at all.
fn parse_meminfo(text: &str) -> Option<(u64, u64)> {
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        let Some((key, rest)) = line.split_once(':') else { continue };
        match key.trim() {
            "MemTotal" => total = parse_kib(rest),
            "MemAvailable" => available = parse_kib(rest),
            _ => {}
        }
        if total.is_some() && available.is_some() {
            break;
        }
    }
    Some((total?, available.unwrap_or(0)))
}

/// Kibibytes to bytes. A number too large to hold in bytes is not a real machine, and is read as
/// unknown rather than allowed to overflow.
fn parse_kib(field: &str) -> Option<u64> {
    field.split_whitespace().next()?.parse::<u64>().ok()?.checked_mul(1024)
}

/// The memory limit of the cgroup this process runs in, when it has one.
///
/// cgroup v2 names the process's group in `/proc/self/cgroup` as `0::/path`, and the limit lives
/// in `memory.max` under that path — inside most containers the path is `/`. cgroup v1 keeps it
/// in `memory/memory.limit_in_bytes`. A group with no limit says `max`, or v1's enormous
/// sentinel, and both mean there is nothing to hold the machine to.
fn read_cgroup_memory_limit() -> Option<u64> {
    const ROOT: &str = "/sys/fs/cgroup";
    let own = fs::read_to_string("/proc/self/cgroup").ok().and_then(|text| {
        text.lines().find_map(|line| line.strip_prefix("0::").map(|path| path.trim().to_string()))
    });
    let mut candidates = Vec::new();
    if let Some(path) = own.filter(|path| !path.is_empty() && path != "/") {
        candidates.push(format!("{ROOT}{path}/memory.max"));
    }
    candidates.push(format!("{ROOT}/memory.max"));
    candidates.push(format!("{ROOT}/memory/memory.limit_in_bytes"));
    candidates.iter().find_map(|file| parse_cgroup_limit(&fs::read_to_string(file).ok()?))
}

/// One cgroup memory limit file, in bytes, or `None` when it sets no limit.
fn parse_cgroup_limit(text: &str) -> Option<u64> {
    // cgroup v1 writes "no limit" as the largest page-aligned 63-bit number. Anything at or past
    // an exbibyte is that sentinel or something like it, and no machine this runs on has one.
    const NO_LIMIT: u64 = 1 << 60;
    let limit = text.trim().parse::<u64>().ok()?;
    (limit > 0 && limit < NO_LIMIT).then_some(limit)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn profile(cores: usize, total_gib: u64) -> MachineProfile {
        MachineProfile {
            logical_cores: cores,
            total_memory_bytes: total_gib * 1024 * 1024 * 1024,
            available_memory_bytes: 0,
        }
    }

    #[test]
    fn one_core_still_gets_one_worker() {
        assert_eq!(profile(1, 8).default_worker_threads(), 1);
    }

    #[test]
    fn a_core_is_left_for_the_screen() {
        assert_eq!(profile(12, 16).default_worker_threads(), 11);
    }

    #[test]
    fn memory_holds_a_machine_back_from_the_larger_class() {
        // Plenty of cores, not enough memory: the transformer defaults would swap.
        assert_eq!(profile(16, 8).size_class(), SizeClass::Medium);
        assert_eq!(profile(16, 32).size_class(), SizeClass::Large);
    }

    #[test]
    fn small_machines_land_in_small() {
        assert_eq!(profile(4, 16).size_class(), SizeClass::Small);
        assert_eq!(profile(8, 2).size_class(), SizeClass::Small);
    }

    #[test]
    fn detect_reports_at_least_one_core() {
        assert!(MachineProfile::detect().logical_cores >= 1);
    }

    #[test]
    fn a_line_without_a_colon_does_not_lose_the_memory() {
        let text = "MemTotal:       16000000 kB\nsomething new\nMemAvailable:    8000000 kB\n";
        assert_eq!(parse_meminfo(text), Some((16_000_000 * 1024, 8_000_000 * 1024)));
    }

    #[test]
    fn meminfo_without_a_total_is_unknown_and_nonsense_does_not_overflow() {
        assert_eq!(parse_meminfo("MemAvailable: 10 kB\n"), None);
        assert_eq!(parse_meminfo(""), None);
        assert_eq!(parse_kib(" 18446744073709551615 kB"), None);
        assert_eq!(parse_kib(" lots kB"), None);
    }

    #[test]
    fn a_cgroup_limit_is_read_and_no_limit_is_none() {
        assert_eq!(parse_cgroup_limit("2147483648\n"), Some(2 << 30));
        assert_eq!(parse_cgroup_limit("max\n"), None);
        assert_eq!(parse_cgroup_limit("9223372036854771712\n"), None);
        assert_eq!(parse_cgroup_limit("0\n"), None);
        assert_eq!(parse_cgroup_limit(""), None);
    }

    #[test]
    fn a_chosen_thread_count_reaches_the_work_as_its_default() {
        let machine = profile(12, 16);
        for threads in 1..=12 {
            assert_eq!(machine.with_worker_threads(threads).default_worker_threads(), threads);
        }
        assert_eq!(machine.with_worker_threads(0), machine, "auto leaves the machine alone");
        assert_eq!(machine.with_worker_threads(11), machine, "the default is already that");
    }

    #[test]
    fn detect_never_reports_more_memory_available_than_there_is() {
        let machine = MachineProfile::detect();
        if machine.total_memory_bytes > 0 {
            assert!(machine.available_memory_bytes <= machine.total_memory_bytes);
        }
    }
}
