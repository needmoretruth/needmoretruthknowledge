//! How numbers reach the screen.
//!
//! Mining rates span nine orders of magnitude between a laptop and a mining farm, and a reader
//! comparing their machine to Satoshi's needs both numbers in the same shape. These helpers are
//! the only place that shape is decided.

use std::time::Duration;

/// A hash rate, scaled to the largest unit that leaves a digit before the decimal point.
///
/// ```
/// # use nmtk_core::format::hashrate;
/// assert_eq!(hashrate(0.0), "0 H/s");
/// assert_eq!(hashrate(950.0), "950 H/s");
/// assert_eq!(hashrate(1_500_000.0), "1.50 MH/s");
/// ```
pub fn hashrate(hashes_per_second: f64) -> String {
    const UNITS: [&str; 6] = ["H/s", "kH/s", "MH/s", "GH/s", "TH/s", "PH/s"];
    scaled(hashes_per_second, 1000.0, &UNITS)
}

/// A byte count in binary units.
///
/// ```
/// # use nmtk_core::format::bytes;
/// assert_eq!(bytes(512), "512 B");
/// assert_eq!(bytes(1024 * 1024), "1.00 MiB");
/// ```
pub fn bytes(count: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    scaled(count as f64, 1024.0, &UNITS)
}

fn scaled(value: f64, step: f64, units: &[&str]) -> String {
    if !value.is_finite() || value <= 0.0 {
        return format!("0 {}", units[0]);
    }
    let mut value = value;
    let mut unit = 0;
    while value >= step && unit + 1 < units.len() {
        value /= step;
        unit += 1;
    }
    if unit == 0 {
        format!("{} {}", value.round() as u64, units[0])
    } else {
        format!("{value:.2} {}", units[unit])
    }
}

/// A whole number with thousands separators: `1234567` becomes `1,234,567`.
pub fn count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

/// A span of time in the largest unit that stays readable.
///
/// Mining at difficulty 1 on one core takes years, and a reader has to see that it is years
/// without counting zeroes.
///
/// ```
/// # use nmtk_core::format::duration;
/// # use std::time::Duration;
/// assert_eq!(duration(Duration::from_millis(430)), "0.43s");
/// assert_eq!(duration(Duration::from_secs(90)), "1m 30s");
/// assert_eq!(duration(Duration::from_secs(3 * 3600 + 4 * 60)), "3h 4m");
/// ```
pub fn duration(d: Duration) -> String {
    let seconds = d.as_secs_f64();
    if seconds < 1.0 {
        return format!("{seconds:.2}s");
    }
    let total = seconds.round() as u64;
    let (days, hours, minutes, secs) =
        (total / 86_400, (total % 86_400) / 3600, (total % 3600) / 60, total % 60);
    if days >= 365 {
        let years = days as f64 / 365.25;
        format!("{years:.1} years")
    } else if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {secs}s")
    } else {
        format!("{secs}s")
    }
}

/// A ratio as a percentage with one decimal: `0.512` becomes `51.2%`.
pub fn percent(ratio: f64) -> String {
    if !ratio.is_finite() {
        return "—".to_string();
    }
    format!("{:.1}%", ratio * 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashrate_climbs_through_the_units() {
        assert_eq!(hashrate(999.0), "999 H/s");
        assert_eq!(hashrate(1000.0), "1.00 kH/s");
        assert_eq!(hashrate(2_500_000_000.0), "2.50 GH/s");
    }

    #[test]
    fn hashrate_survives_nonsense() {
        assert_eq!(hashrate(f64::NAN), "0 H/s");
        assert_eq!(hashrate(-5.0), "0 H/s");
    }

    #[test]
    fn counts_get_separators_only_where_needed() {
        assert_eq!(count(0), "0");
        assert_eq!(count(999), "999");
        assert_eq!(count(1000), "1,000");
        assert_eq!(count(1_234_567_890), "1,234,567,890");
    }

    #[test]
    fn long_spans_read_as_years() {
        assert_eq!(duration(Duration::from_secs(86_400 * 400)), "1.1 years");
        assert_eq!(duration(Duration::from_secs(86_400 * 3 + 7200)), "3d 2h");
    }

    #[test]
    fn percent_handles_division_by_zero() {
        assert_eq!(percent(0.512), "51.2%");
        assert_eq!(percent(f64::INFINITY), "—");
    }
}
