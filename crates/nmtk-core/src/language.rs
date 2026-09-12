//! The languages nmtk speaks.
//!
//! A language is a code and the name that language calls itself by — never a pair, never a
//! toggle. English is the source of truth and is always complete; every other language is a column
//! that may have gaps, and a gap falls back to English rather than to a blank.
//!
//! Adding a language is one entry in [`Language::ALL`] and one more column in each phrase table.
//! Nothing else in the program changes, which is the point: this list is expected to get long.

use std::fmt;

use serde::de::{Deserialize, Deserializer, Error as _};
use serde::ser::{Serialize, Serializer};

/// One language nmtk can be read in.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Language {
    code: &'static str,
    endonym: &'static str,
}

impl Language {
    /// The source of truth. Never missing a line.
    pub const ENGLISH: Language = Language { code: "en", endonym: "English" };
    /// 한국어.
    pub const KOREAN: Language = Language { code: "ko", endonym: "한국어" };

    /// Every language, in the order a reader picks from. English first because it is complete;
    /// the rest alphabetically by code, so the list stays predictable as it grows.
    pub const ALL: &'static [Language] = &[Language::ENGLISH, Language::KOREAN];

    /// The BCP-47 language subtag, e.g. `ko`. This is what phrase tables match on and what the
    /// settings file stores.
    pub const fn code(self) -> &'static str {
        self.code
    }

    /// What this language calls itself. A reader finds their own language by looking for a word
    /// they recognise, so the list is never translated.
    pub const fn endonym(self) -> &'static str {
        self.endonym
    }

    /// The short tag in the title bar.
    pub fn tag(self) -> String {
        self.code.to_uppercase()
    }

    /// A language by code, or `None` when nmtk does not speak it.
    pub fn from_code(code: &str) -> Option<Language> {
        Language::ALL.iter().copied().find(|language| language.code == code)
    }

    /// Where this language sits in [`Language::ALL`].
    pub fn index(self) -> usize {
        Language::ALL.iter().position(|language| *language == self).unwrap_or(0)
    }
}

impl Default for Language {
    fn default() -> Self {
        Language::ENGLISH
    }
}

impl fmt::Display for Language {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.endonym)
    }
}

impl Serialize for Language {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_str(self.code)
    }
}

impl<'de> Deserialize<'de> for Language {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let code = String::deserialize(deserializer)?;
        // A settings file naming a language this build does not have is not an error worth
        // stopping for, but it is worth saying so rather than silently choosing.
        Language::from_code(&code)
            .ok_or_else(|| D::Error::custom(format!("unknown language `{code}`")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn english_is_the_default_and_comes_first() {
        assert_eq!(Language::default(), Language::ENGLISH);
        assert_eq!(Language::ALL[0], Language::ENGLISH);
    }

    #[test]
    fn every_language_has_a_distinct_code() {
        let mut codes: Vec<&str> = Language::ALL.iter().map(|l| l.code()).collect();
        let total = codes.len();
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), total, "two languages share a code");
    }

    #[test]
    fn a_language_names_itself_in_its_own_language() {
        assert_eq!(Language::KOREAN.endonym(), "한국어");
        assert_eq!(Language::ENGLISH.endonym(), "English");
    }

    #[test]
    fn codes_survive_a_round_trip() {
        for language in Language::ALL {
            assert_eq!(Language::from_code(language.code()), Some(*language));
        }
        assert_eq!(Language::from_code("xx"), None);
    }

    #[test]
    fn the_tag_is_what_the_title_bar_shows() {
        assert_eq!(Language::KOREAN.tag(), "KO");
    }
}
