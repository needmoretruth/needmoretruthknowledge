//! How long a block should take, and why that is not a promise.
//!
//! Every hash is an independent try with the same tiny chance of winning, so the number of hashes
//! until a block is geometrically distributed and the waiting time is very nearly exponential.
//! That distribution has no memory: a machine that has been grinding for an hour is exactly as
//! far from its next block as one that just started. The average below is the average of a
//! distribution with a long tail, not a countdown. Half of all blocks arrive before 69% of it,
//! and about one in twenty takes more than three times as long.

use crate::target::Target;

/// The interval Bitcoin aims for between blocks, in seconds. The difficulty is adjusted every
/// 2016 blocks to keep the average here.
pub const BITCOIN_TARGET_BLOCK_SECONDS: f64 = 600.0;

/// What a given hash rate can expect from a given difficulty.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct BlockTimeEstimate {
    /// The rate the estimate was made for, in hashes per second.
    pub hashes_per_second: f64,
    /// How many hashes a block takes on average at this difficulty.
    pub expected_hashes: f64,
    /// The average wait, in seconds. Infinite when nothing is hashing.
    pub expected_seconds: f64,
    /// The wait half of all blocks come in under, which is the average times the natural log of
    /// two. It is always the smaller number, and it is the more useful one to show a reader.
    pub median_seconds: f64,
}

/// The average time to find one block at this hash rate and difficulty.
///
/// This is an expectation over a geometric distribution, not a schedule. A run that takes four
/// times the average is unremarkable; so is one that takes a twentieth.
pub fn expected_time_to_block(hashes_per_second: f64, target: Target) -> BlockTimeEstimate {
    let expected_hashes = target.expected_hashes();
    let usable_rate =
        if hashes_per_second.is_finite() && hashes_per_second > 0.0 { hashes_per_second } else { 0.0 };
    let expected_seconds =
        if usable_rate > 0.0 { expected_hashes / usable_rate } else { f64::INFINITY };
    BlockTimeEstimate {
        hashes_per_second: usable_rate,
        expected_hashes,
        expected_seconds,
        median_seconds: expected_seconds * core::f64::consts::LN_2,
    }
}

/// The chance of finding at least one block within a stretch of time, between 0 and 1.
///
/// This is where the long tail becomes visible: at exactly the average wait the chance is only
/// about 63%, not 100%.
pub fn probability_of_block_within(hashes_per_second: f64, target: Target, seconds: f64) -> f64 {
    let estimate = expected_time_to_block(hashes_per_second, target);
    if !seconds.is_finite() || seconds <= 0.0 || !estimate.expected_seconds.is_finite() {
        return 0.0;
    }
    1.0 - (-seconds / estimate.expected_seconds).exp()
}

/// The hash rate a whole network must have been running at, given the difficulty it held and how
/// often blocks arrived.
///
/// Difficulty 1 and ten-minute blocks work out to a little over seven million hashes a second,
/// which is what the entire network was doing in January 2009. It is a figure derived from the
/// protocol's own numbers rather than a measurement of anybody's computer.
pub fn implied_network_hashrate(target: Target, block_interval_seconds: f64) -> f64 {
    if !block_interval_seconds.is_finite() || block_interval_seconds <= 0.0 {
        return f64::INFINITY;
    }
    target.expected_hashes() / block_interval_seconds
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::target::practice_bits;

    #[test]
    fn difficulty_one_takes_about_ten_minutes_at_the_networks_original_rate() {
        let target = Target::difficulty_one();
        let rate = implied_network_hashrate(target, BITCOIN_TARGET_BLOCK_SECONDS);
        assert!((rate - 7_158_388.0).abs() < 1.0, "implied rate was {rate}");
        let estimate = expected_time_to_block(rate, target);
        assert!((estimate.expected_seconds - 600.0).abs() < 0.001);
    }

    #[test]
    fn the_median_is_shorter_than_the_average_because_the_tail_is_long() {
        let estimate = expected_time_to_block(1_000_000.0, Target::difficulty_one());
        assert!(estimate.median_seconds < estimate.expected_seconds);
        assert!((estimate.median_seconds / estimate.expected_seconds - 0.693).abs() < 0.001);
    }

    #[test]
    fn waiting_the_average_is_only_a_sixty_three_percent_chance() {
        let target = Target::difficulty_one();
        let estimate = expected_time_to_block(5_000_000.0, target);
        let chance = probability_of_block_within(5_000_000.0, target, estimate.expected_seconds);
        assert!((chance - 0.6321).abs() < 0.001, "chance was {chance}");
        let long_shot = probability_of_block_within(5_000_000.0, target, estimate.expected_seconds * 10.0);
        assert!(long_shot > 0.9999);
    }

    #[test]
    fn doubling_the_hash_rate_halves_the_wait() {
        let target = Target::difficulty_one();
        let slow = expected_time_to_block(1_000_000.0, target);
        let fast = expected_time_to_block(2_000_000.0, target);
        assert!((slow.expected_seconds / fast.expected_seconds - 2.0).abs() < 1e-9);
    }

    #[test]
    fn a_practice_difficulty_is_seconds_rather_than_centuries() {
        let target = Target::from_leading_zero_bits(22).expect("expressible");
        let estimate = expected_time_to_block(2_000_000.0, target);
        assert!(estimate.expected_seconds < 5.0, "practice block took {}s", estimate.expected_seconds);
        let real = expected_time_to_block(2_000_000.0, Target::difficulty_one());
        assert!(real.expected_seconds > 1_000.0);
    }

    #[test]
    fn nothing_hashing_means_no_block_ever_rather_than_a_panic() {
        let target = Target::from_compact(practice_bits(20).expect("expressible")).expect("canonical");
        for rate in [0.0, -1.0, f64::NAN] {
            let estimate = expected_time_to_block(rate, target);
            assert!(estimate.expected_seconds.is_infinite());
            assert_eq!(probability_of_block_within(rate, target, 10.0), 0.0);
        }
    }
}
