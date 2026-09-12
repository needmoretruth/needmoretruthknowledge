//! Who is mining, and how the machine's threads are divided between them.
//!
//! A share is either a percentage or a raw thread count. Percentages are handed out by largest
//! remainder, so the parts always add up to the budget exactly and the same request always
//! produces the same split. Threads are whole things: a machine with 8 usable threads cannot give
//! one miner 51% and another 49%, it can only give 4 and 4. The split that comes back therefore
//! carries the share each miner really got, which is the number that decides how many blocks it
//! finds. A caller that wants the requested percentages honoured closely asks for more threads
//! than the machine has cores; the operating system then splits its time between them and each
//! miner's hash rate lands on its share, at a small cost in total throughput.

use core::fmt;

use crate::target::TargetError;

/// Which miner. The name a reader sees belongs to whoever draws the screen; down here a miner is
/// a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct MinerId(pub u32);

/// Nobody in particular. The genesis block was not mined by anyone, so this is who found it.
pub const NOBODY: MinerId = MinerId(u32::MAX);

/// How much of the machine a miner asked for.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Share {
    /// A proportion of what is left after the raw thread counts are taken. The numbers do not
    /// have to add up to 100: three miners asking for 1, 1 and 2 get a quarter, a quarter and a
    /// half.
    Percent(f64),
    /// This many threads, taken off the top.
    Threads(usize),
}

/// One miner and its share.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct MinerSpec {
    /// Which miner.
    pub id: MinerId,
    /// What it asked for.
    pub share: Share,
}

impl MinerSpec {
    /// A miner asking for a proportion of the machine.
    pub fn percent(id: u32, percent: f64) -> MinerSpec {
        MinerSpec { id: MinerId(id), share: Share::Percent(percent) }
    }

    /// A miner asking for a fixed number of threads.
    pub fn threads(id: u32, threads: usize) -> MinerSpec {
        MinerSpec { id: MinerId(id), share: Share::Threads(threads) }
    }
}

/// What can be wrong with the way a run was asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// Nobody was mining.
    NoMiners,
    /// Two miners were given the same id.
    DuplicateMinerId(MinerId),
    /// A share was zero, negative, not a number, or absurdly large.
    InvalidShare(MinerId),
    /// Not enough threads to give every miner at least one.
    ThreadBudgetTooSmall {
        /// The smallest budget that would work.
        needed: usize,
        /// What was offered.
        budget: usize,
    },
    /// The raw thread counts alone ask for more than the budget.
    FixedThreadsExceedBudget {
        /// The sum of the raw thread counts.
        fixed: usize,
        /// What was offered.
        budget: usize,
    },
    /// The difficulty the run was given cannot be read.
    BadTarget(TargetError),
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ConfigError::NoMiners => f.write_str("no miners were configured"),
            ConfigError::DuplicateMinerId(id) => write!(f, "miner id {} appears twice", id.0),
            ConfigError::InvalidShare(id) => {
                write!(f, "miner id {} has a share that is not a positive finite number", id.0)
            }
            ConfigError::ThreadBudgetTooSmall { needed, budget } => {
                write!(f, "thread budget {budget} is below the {needed} needed, one per miner")
            }
            ConfigError::FixedThreadsExceedBudget { fixed, budget } => {
                write!(f, "fixed thread counts total {fixed}, over the budget of {budget}")
            }
            ConfigError::BadTarget(err) => write!(f, "bad target: {err}"),
        }
    }
}

impl std::error::Error for ConfigError {}

impl From<TargetError> for ConfigError {
    fn from(err: TargetError) -> ConfigError {
        ConfigError::BadTarget(err)
    }
}

/// The largest share this crate will take seriously. Anything past it is a typo, and letting it
/// through would push the sum of the shares to infinity and every division to a NaN.
const MAX_SHARE: f64 = 1e9;

/// Divides a thread budget between miners.
///
/// Raw thread counts are taken first. What is left is divided between the miners that asked for a
/// proportion, by largest remainder, and then every one of them is brought up to at least one
/// thread by taking from the largest — a miner with no threads is not mining at all. The result
/// lines up with `specs`, entry for entry, and always adds up to the budget unless every share
/// was a raw thread count.
pub fn split_threads(specs: &[MinerSpec], budget: usize) -> Result<Vec<usize>, ConfigError> {
    if specs.is_empty() {
        return Err(ConfigError::NoMiners);
    }
    if budget < specs.len() {
        return Err(ConfigError::ThreadBudgetTooSmall { needed: specs.len(), budget });
    }
    for (index, spec) in specs.iter().enumerate() {
        if specs[..index].iter().any(|earlier| earlier.id == spec.id) {
            return Err(ConfigError::DuplicateMinerId(spec.id));
        }
        match spec.share {
            Share::Percent(percent) => {
                if !percent.is_finite() || percent <= 0.0 || percent > MAX_SHARE {
                    return Err(ConfigError::InvalidShare(spec.id));
                }
            }
            Share::Threads(threads) => {
                if threads == 0 {
                    return Err(ConfigError::InvalidShare(spec.id));
                }
            }
        }
    }

    let mut threads = vec![0usize; specs.len()];
    let mut fixed_total = 0usize;
    for (slot, spec) in threads.iter_mut().zip(specs) {
        if let Share::Threads(count) = spec.share {
            *slot = count;
            fixed_total = fixed_total.saturating_add(count);
        }
    }
    if fixed_total > budget {
        return Err(ConfigError::FixedThreadsExceedBudget { fixed: fixed_total, budget });
    }

    let proportional: Vec<usize> = specs
        .iter()
        .enumerate()
        .filter(|(_, spec)| matches!(spec.share, Share::Percent(_)))
        .map(|(index, _)| index)
        .collect();
    if proportional.is_empty() {
        return Ok(threads);
    }

    let remaining = budget - fixed_total;
    if remaining < proportional.len() {
        return Err(ConfigError::ThreadBudgetTooSmall {
            needed: fixed_total + proportional.len(),
            budget,
        });
    }

    let share_of = |index: usize| match specs[index].share {
        Share::Percent(percent) => percent,
        Share::Threads(_) => 0.0,
    };
    let share_total: f64 = proportional.iter().copied().map(share_of).sum();

    // Largest remainder: everyone gets the whole part of their share, then the threads left over
    // go to whoever was cut the most, biggest loser first.
    let mut leftovers: Vec<(f64, usize)> = Vec::with_capacity(proportional.len());
    let mut handed_out = 0usize;
    for &index in &proportional {
        let exact = remaining as f64 * share_of(index) / share_total;
        let whole = exact.floor().max(0.0) as usize;
        threads[index] = whole;
        handed_out += whole;
        leftovers.push((exact - exact.floor(), index));
    }
    leftovers.sort_by(|left, right| {
        right
            .0
            .partial_cmp(&left.0)
            .unwrap_or(core::cmp::Ordering::Equal)
            .then(left.1.cmp(&right.1))
    });
    let mut spare = remaining.saturating_sub(handed_out);
    for (_, index) in &leftovers {
        if spare == 0 {
            break;
        }
        threads[*index] += 1;
        spare -= 1;
    }

    // Nobody who asked for a share is left without a thread.
    for _ in 0..proportional.len() {
        let Some(&starved) = proportional.iter().find(|&&index| threads[index] == 0) else {
            break;
        };
        let donor = proportional
            .iter()
            .copied()
            .filter(|&index| threads[index] >= 2)
            .max_by_key(|&index| threads[index]);
        match donor {
            Some(donor) => {
                threads[donor] -= 1;
                threads[starved] += 1;
            }
            None => break,
        }
    }

    Ok(threads)
}

/// The share of the total hash power each miner really ends up with, given a thread split.
///
/// This is the number that decides how many blocks a miner finds, and it is not always the number
/// that was asked for.
pub fn effective_shares(threads: &[usize]) -> Vec<f64> {
    let total: usize = threads.iter().sum();
    if total == 0 {
        return vec![0.0; threads.len()];
    }
    threads.iter().map(|count| *count as f64 / total as f64).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_fifty_one_to_forty_nine_split_lands_on_the_percentages_when_threads_allow() {
        let specs = [MinerSpec::percent(0, 51.0), MinerSpec::percent(1, 49.0)];
        let threads = split_threads(&specs, 100).expect("valid");
        assert_eq!(threads, vec![51, 49]);
        let shares = effective_shares(&threads);
        assert!((shares[0] - 0.51).abs() < 1e-12);
    }

    #[test]
    fn the_parts_always_add_up_to_the_budget() {
        let specs =
            [MinerSpec::percent(0, 33.3), MinerSpec::percent(1, 33.3), MinerSpec::percent(2, 33.4)];
        for budget in 3..64 {
            let threads = split_threads(&specs, budget).expect("valid");
            assert_eq!(threads.iter().sum::<usize>(), budget, "budget {budget} did not add up");
            assert!(threads.iter().all(|count| *count >= 1));
        }
    }

    #[test]
    fn raw_thread_counts_are_taken_off_the_top_and_the_rest_is_divided() {
        let specs =
            [MinerSpec::threads(0, 3), MinerSpec::percent(1, 75.0), MinerSpec::percent(2, 25.0)];
        let threads = split_threads(&specs, 11).expect("valid");
        assert_eq!(threads, vec![3, 6, 2]);
    }

    #[test]
    fn shares_do_not_have_to_add_up_to_a_hundred() {
        let specs =
            [MinerSpec::percent(0, 1.0), MinerSpec::percent(1, 1.0), MinerSpec::percent(2, 2.0)];
        assert_eq!(split_threads(&specs, 8).expect("valid"), vec![2, 2, 4]);
    }

    #[test]
    fn a_tiny_share_still_gets_a_thread_because_a_miner_with_none_is_not_mining() {
        let specs = [MinerSpec::percent(0, 99.9), MinerSpec::percent(1, 0.1)];
        let threads = split_threads(&specs, 4).expect("valid");
        assert_eq!(threads, vec![3, 1]);
        assert_eq!(threads.iter().sum::<usize>(), 4);
    }

    #[test]
    fn the_same_request_always_splits_the_same_way() {
        let specs =
            [MinerSpec::percent(0, 1.0), MinerSpec::percent(1, 1.0), MinerSpec::percent(2, 1.0)];
        let first = split_threads(&specs, 10).expect("valid");
        for _ in 0..8 {
            assert_eq!(split_threads(&specs, 10).expect("valid"), first);
        }
        assert_eq!(first.iter().sum::<usize>(), 10);
    }

    #[test]
    fn bad_requests_are_refused() {
        assert_eq!(split_threads(&[], 4), Err(ConfigError::NoMiners));
        assert_eq!(
            split_threads(&[MinerSpec::percent(0, 1.0), MinerSpec::percent(0, 1.0)], 4),
            Err(ConfigError::DuplicateMinerId(MinerId(0)))
        );
        assert_eq!(
            split_threads(&[MinerSpec::percent(0, 0.0)], 4),
            Err(ConfigError::InvalidShare(MinerId(0)))
        );
        assert_eq!(
            split_threads(&[MinerSpec::percent(0, f64::NAN)], 4),
            Err(ConfigError::InvalidShare(MinerId(0)))
        );
        assert_eq!(
            split_threads(&[MinerSpec::percent(0, 1.0), MinerSpec::percent(1, 1.0)], 1),
            Err(ConfigError::ThreadBudgetTooSmall { needed: 2, budget: 1 })
        );
        assert_eq!(
            split_threads(&[MinerSpec::threads(0, 6), MinerSpec::threads(1, 6)], 8),
            Err(ConfigError::FixedThreadsExceedBudget { fixed: 12, budget: 8 })
        );
    }

    #[test]
    fn raw_thread_counts_alone_leave_the_rest_of_the_budget_unused() {
        let specs = [MinerSpec::threads(0, 2), MinerSpec::threads(1, 1)];
        let threads = split_threads(&specs, 8).expect("valid");
        assert_eq!(threads, vec![2, 1]);
        // The share that matters is of the threads actually running, not of the machine.
        let shares = effective_shares(&threads);
        assert!((shares[0] - 2.0 / 3.0).abs() < 1e-12);
    }
}
