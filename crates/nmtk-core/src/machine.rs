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
    pub fn detect() -> Self {
        let logical_cores = available_parallelism().map(|n| n.get()).unwrap_or(1);
        let (total_memory_bytes, available_memory_bytes) = read_meminfo().unwrap_or((0, 0));
        Self { logical_cores, total_memory_bytes, available_memory_bytes }
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
    let text = fs::read_to_string("/proc/meminfo").ok()?;
    let mut total = None;
    let mut available = None;
    for line in text.lines() {
        let (key, rest) = line.split_once(':')?;
        match key {
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

fn parse_kib(field: &str) -> Option<u64> {
    field.split_whitespace().next()?.parse::<u64>().ok().map(|kib| kib * 1024)
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
}
