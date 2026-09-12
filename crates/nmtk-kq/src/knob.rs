//! The values a reader turns while a quest runs.
//!
//! Every knob offers both ways of choosing: presets for the reader who wants a good answer now,
//! and typing for the reader who wants *their* number. A quest that only offers presets has
//! decided for the reader what is worth trying, which is the opposite of the point.

/// One value a reader can change.
#[derive(Debug, Clone, PartialEq)]
pub struct Knob {
    /// Stable key, used by the quest's phrase table for the label and the unit.
    pub id: &'static str,
    pub value: KnobValue,
    /// True while the reader is typing a number into this knob.
    typing: Option<String>,
}

impl Knob {
    pub fn new(id: &'static str, value: KnobValue) -> Self {
        Self { id, value, typing: None }
    }

    /// Steps the value up or down, or moves through the presets when there are any.
    pub fn nudge(&mut self, direction: i32) {
        self.typing = None;
        self.value.nudge(direction);
    }

    /// Starts, or continues, typing a number. Non-digits are ignored except `.` and `-`.
    pub fn type_char(&mut self, c: char) {
        if c.is_ascii_digit() || c == '.' || (c == '-' && self.typing.is_none()) {
            self.typing.get_or_insert_with(String::new).push(c);
        }
    }

    /// Removes the last typed character; returns to the stored value when nothing is left.
    pub fn backspace(&mut self) {
        if let Some(text) = &mut self.typing {
            text.pop();
            if text.is_empty() {
                self.typing = None;
            }
        }
    }

    /// Accepts what was typed. Returns false when it does not parse or falls outside the range,
    /// leaving the old value in place.
    pub fn commit(&mut self) -> bool {
        let Some(text) = self.typing.take() else { return false };
        self.value.set_from(&text)
    }

    /// Throws away what was typed.
    pub fn cancel(&mut self) {
        self.typing = None;
    }

    /// What the reader is typing right now, if anything.
    pub fn draft(&self) -> Option<&str> {
        self.typing.as_deref()
    }

    /// The value as text: the draft while typing, otherwise the stored value.
    pub fn display(&self) -> String {
        match &self.typing {
            Some(text) => format!("{text}_"),
            None => self.value.display(),
        }
    }
}

/// What kind of value a knob holds.
#[derive(Debug, Clone, PartialEq)]
pub enum KnobValue {
    /// A whole number — threads, blocks, steps.
    Count { current: u64, min: u64, max: u64, step: u64, presets: &'static [u64] },
    /// A share between 0 and 1, shown as a percentage.
    Share { current: f64, min: f64, max: f64, step: f64, presets: &'static [f64] },
    /// A number with a fractional part — a learning rate, a temperature.
    Decimal { current: f64, min: f64, max: f64, step: f64, presets: &'static [f64] },
    /// One of a fixed set. The quest's phrase table names each option.
    Choice { current: usize, count: usize },
    /// On or off.
    Toggle { current: bool },
}

impl KnobValue {
    fn nudge(&mut self, direction: i32) {
        let up = direction > 0;
        match self {
            KnobValue::Count { current, min, max, step, .. } => {
                let delta = *step;
                *current = if up {
                    current.saturating_add(delta).min(*max)
                } else {
                    current.saturating_sub(delta).max(*min)
                };
            }
            KnobValue::Share { current, min, max, step, .. }
            | KnobValue::Decimal { current, min, max, step, .. } => {
                let next = if up { *current + *step } else { *current - *step };
                *current = next.clamp(*min, *max);
            }
            KnobValue::Choice { current, count } => {
                if *count > 0 {
                    *current =
                        if up { (*current + 1) % *count } else { (*current + *count - 1) % *count };
                }
            }
            KnobValue::Toggle { current } => *current = !*current,
        }
    }

    /// Parses typed text into this knob's value. Out-of-range and unparsable text are refused.
    fn set_from(&mut self, text: &str) -> bool {
        let text = text.trim();
        match self {
            KnobValue::Count { current, min, max, .. } => match text.parse::<u64>() {
                Ok(value) if value >= *min && value <= *max => {
                    *current = value;
                    true
                }
                _ => false,
            },
            KnobValue::Share { current, min, max, .. } => match text.parse::<f64>() {
                // A share is typed as a percentage, because that is how it is displayed.
                Ok(value) => {
                    let ratio = value / 100.0;
                    if ratio.is_finite() && ratio >= *min && ratio <= *max {
                        *current = ratio;
                        true
                    } else {
                        false
                    }
                }
                _ => false,
            },
            KnobValue::Decimal { current, min, max, .. } => match text.parse::<f64>() {
                Ok(value) if value.is_finite() && value >= *min && value <= *max => {
                    *current = value;
                    true
                }
                _ => false,
            },
            KnobValue::Choice { current, count } => match text.parse::<usize>() {
                Ok(value) if value < *count => {
                    *current = value;
                    true
                }
                _ => false,
            },
            KnobValue::Toggle { current } => match text {
                "0" => {
                    *current = false;
                    true
                }
                "1" => {
                    *current = true;
                    true
                }
                _ => false,
            },
        }
    }

    /// The stored value as the reader sees it.
    pub fn display(&self) -> String {
        match self {
            KnobValue::Count { current, .. } => nmtk_core::format::count(*current),
            KnobValue::Share { current, .. } => nmtk_core::format::percent(*current),
            KnobValue::Decimal { current, .. } => format!("{current}"),
            KnobValue::Choice { current, .. } => current.to_string(),
            KnobValue::Toggle { current } => (if *current { "on" } else { "off" }).to_string(),
        }
    }

    /// Where this value sits in its range, for drawing a bar. Choices and toggles have no range.
    pub fn ratio(&self) -> Option<f64> {
        match self {
            KnobValue::Count { current, min, max, .. } => {
                let span = max.saturating_sub(*min);
                (span > 0).then(|| (current - min) as f64 / span as f64)
            }
            KnobValue::Share { current, min, max, .. }
            | KnobValue::Decimal { current, min, max, .. } => {
                let span = max - min;
                (span > 0.0).then(|| (current - min) / span)
            }
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn threads() -> Knob {
        Knob::new(
            "threads",
            KnobValue::Count { current: 4, min: 1, max: 12, step: 1, presets: &[1, 4, 8, 12] },
        )
    }

    #[test]
    fn a_count_stops_at_its_ends_rather_than_wrapping() {
        let mut knob = threads();
        for _ in 0..50 {
            knob.nudge(1);
        }
        assert_eq!(knob.display(), "12");
        for _ in 0..50 {
            knob.nudge(-1);
        }
        assert_eq!(knob.display(), "1");
    }

    #[test]
    fn typing_a_number_beats_the_presets() {
        let mut knob = threads();
        for c in "7".chars() {
            knob.type_char(c);
        }
        assert!(knob.commit());
        assert_eq!(knob.display(), "7");
    }

    #[test]
    fn a_refused_number_leaves_the_old_one_alone() {
        let mut knob = threads();
        for c in "99".chars() {
            knob.type_char(c);
        }
        assert!(!knob.commit());
        assert_eq!(knob.display(), "4");
    }

    #[test]
    fn a_share_is_typed_the_way_it_is_shown() {
        let mut knob = Knob::new(
            "attacker",
            KnobValue::Share {
                current: 0.3,
                min: 0.0,
                max: 1.0,
                step: 0.01,
                presets: &[0.3, 0.51],
            },
        );
        for c in "51".chars() {
            knob.type_char(c);
        }
        assert!(knob.commit());
        assert_eq!(knob.display(), "51.0%");
    }

    #[test]
    fn the_draft_is_visible_while_typing_and_gone_after_cancel() {
        let mut knob = threads();
        knob.type_char('1');
        knob.type_char('2');
        assert_eq!(knob.display(), "12_");
        knob.cancel();
        assert_eq!(knob.display(), "4");
    }

    #[test]
    fn letters_never_reach_the_draft() {
        let mut knob = threads();
        for c in "1a2".chars() {
            knob.type_char(c);
        }
        assert_eq!(knob.draft(), Some("12"));
    }

    #[test]
    fn a_choice_wraps_because_it_has_no_ends() {
        let mut knob = Knob::new("model", KnobValue::Choice { current: 2, count: 3 });
        knob.nudge(1);
        assert_eq!(knob.display(), "0");
    }
}
