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

    /// Accepts what was typed, and says what became of it.
    ///
    /// A number outside the range is pulled to the nearest end rather than dropped: a reader who
    /// typed 40 into a knob that stops at 30 asked for as much as they could have, and a screen
    /// that silently puts 7 back reads as a broken key.
    pub fn commit(&mut self) -> Typed {
        let Some(text) = self.typing.take() else { return Typed::Nothing };
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

/// What became of a number the reader typed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Typed {
    /// Nothing was being typed.
    Nothing,
    /// Taken exactly as it was typed.
    Taken,
    /// Outside what this knob allows, so it was pulled to the nearest end. The value it landed
    /// on comes with it, already written the way the reader sees it.
    PulledIn { to: String },
    /// Not a number this knob could read. The value did not move.
    NotANumber,
}

/// The values a run was started with, so a later change can be told apart from a repeat.
///
/// A reader who turns a knob while the work is running has asked for the work to be done again;
/// a reader who presses Enter twice has not. Nothing else can tell those two apart.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Settled(Vec<(&'static str, String)>);

impl Settled {
    /// Reads the knobs as they stand. Drafts being typed are not read — a half-typed number is
    /// not yet a value the reader has asked for.
    pub fn of(knobs: &[Knob]) -> Self {
        Self(knobs.iter().map(|knob| (knob.id, knob.value.display())).collect())
    }

    /// Whether the knobs still hold what this was read from.
    pub fn still(&self, knobs: &[Knob]) -> bool {
        self.0.len() == knobs.len()
            && self
                .0
                .iter()
                .zip(knobs)
                .all(|((id, value), knob)| *id == knob.id && *value == knob.value.display())
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

    /// Parses typed text into this knob's value, pulling it into range rather than refusing it.
    ///
    /// Answers with whether the number landed where it was typed, so the caller can say so. The
    /// text of the landing place is read back afterwards, once the value is no longer borrowed.
    fn set_from(&mut self, text: &str) -> Typed {
        let text = text.trim();
        let asked = text.parse::<f64>().ok().filter(|value| value.is_finite());
        let exact = match self {
            KnobValue::Count { current, min, max, .. } => {
                // A count typed with a fractional part is still a number the reader meant; it is
                // rounded rather than thrown away, and the answer says where it landed.
                let Some(asked) = asked.map(f64::round) else { return Typed::NotANumber };
                let landed = asked.clamp(*min as f64, *max as f64);
                *current = landed as u64;
                landed == asked
            }
            KnobValue::Share { current, min, max, .. } => {
                // A share is typed as a percentage, because that is how it is displayed.
                let Some(asked) = asked.map(|value| value / 100.0) else {
                    return Typed::NotANumber;
                };
                let landed = asked.clamp(*min, *max);
                *current = landed;
                landed == asked
            }
            KnobValue::Decimal { current, min, max, .. } => {
                let Some(asked) = asked else { return Typed::NotANumber };
                let landed = asked.clamp(*min, *max);
                *current = landed;
                landed == asked
            }
            KnobValue::Choice { current, count } => {
                let Ok(asked) = text.parse::<usize>() else { return Typed::NotANumber };
                if *count == 0 {
                    return Typed::NotANumber;
                }
                let landed = asked.min(*count - 1);
                *current = landed;
                landed == asked
            }
            KnobValue::Toggle { current } => match text {
                "0" => {
                    *current = false;
                    true
                }
                "1" => {
                    *current = true;
                    true
                }
                _ => return Typed::NotANumber,
            },
        };
        if exact { Typed::Taken } else { Typed::PulledIn { to: self.display() } }
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
        assert_eq!(knob.commit(), Typed::Taken);
        assert_eq!(knob.display(), "7");
    }

    #[test]
    fn a_number_past_the_end_lands_on_the_end_and_says_where() {
        let mut knob = threads();
        for c in "99".chars() {
            knob.type_char(c);
        }
        assert_eq!(knob.commit(), Typed::PulledIn { to: "12".to_string() });
        assert_eq!(knob.display(), "12");
    }

    #[test]
    fn what_is_not_a_number_leaves_the_value_alone() {
        let mut knob = threads();
        knob.type_char('.');
        knob.type_char('.');
        assert_eq!(knob.commit(), Typed::NotANumber);
        assert_eq!(knob.display(), "4");
    }

    #[test]
    fn a_run_can_tell_a_changed_knob_from_an_unchanged_one() {
        let mut knobs = vec![threads()];
        let settled = Settled::of(&knobs);
        assert!(settled.still(&knobs));
        knobs[0].type_char('8');
        assert!(settled.still(&knobs), "a number still being typed is not yet a change");
        knobs[0].commit();
        assert!(!settled.still(&knobs));
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
        assert_eq!(knob.commit(), Typed::Taken);
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
