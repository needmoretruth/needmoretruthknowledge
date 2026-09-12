//! One character, one token.
//!
//! Real language models split text into sub-words, which needs a training pass of its own before
//! any of the interesting parts start. A character vocabulary skips that: the table is the sorted
//! set of characters that occur in the corpus, so it is small enough to print on screen and every
//! step of it can be checked by eye. The cost is that the model has to spell, which is exactly
//! what makes the `nmtk` demonstration a real test of learning.

use std::collections::HashMap;

/// The character table, built once from a corpus and then read-only.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Tokenizer {
    /// Every distinct character, sorted, so the same corpus always gives the same ids.
    chars: Vec<char>,
    index: HashMap<char, usize>,
}

impl Tokenizer {
    /// Builds the table from text. Characters are sorted, which is what makes the ids stable
    /// across runs and machines — a hash-ordered table would break the fixed-seed promise.
    pub fn from_text(text: &str) -> Self {
        let mut chars: Vec<char> = text.chars().collect();
        chars.sort_unstable();
        chars.dedup();
        let index = chars.iter().copied().enumerate().map(|(i, c)| (c, i)).collect();
        Self { chars, index }
    }

    /// How many distinct characters the model has to choose between at every position.
    pub fn vocab_size(&self) -> usize {
        self.chars.len()
    }

    /// The characters in id order. Index `i` is the character token `i` stands for.
    pub fn vocabulary(&self) -> &[char] {
        &self.chars
    }

    /// The id of a character, or `None` when the corpus never contained it.
    pub fn id_of(&self, c: char) -> Option<usize> {
        self.index.get(&c).copied()
    }

    /// The character an id stands for, or `None` when the id is past the table.
    pub fn char_of(&self, id: usize) -> Option<char> {
        self.chars.get(id).copied()
    }

    /// Text to ids, dropping characters the corpus never contained.
    ///
    /// Dropping rather than failing is deliberate: a reader typing an unknown character into a
    /// prompt should still get an answer for the part the model understands.
    pub fn encode(&self, text: &str) -> Vec<usize> {
        text.chars().filter_map(|c| self.id_of(c)).collect()
    }

    /// Text to ids, or `None` if any character is outside the table.
    pub fn encode_strict(&self, text: &str) -> Option<Vec<usize>> {
        text.chars().map(|c| self.id_of(c)).collect()
    }

    /// Ids back to text, dropping ids past the table.
    pub fn decode(&self, ids: &[usize]) -> String {
        ids.iter().filter_map(|&id| self.char_of(id)).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::{CORPUS, PROMPT};

    #[test]
    fn the_corpus_survives_a_round_trip() {
        let tok = Tokenizer::from_text(CORPUS);
        let ids = tok.encode(CORPUS);
        assert_eq!(ids.len(), CORPUS.chars().count());
        assert_eq!(tok.decode(&ids), CORPUS);
    }

    #[test]
    fn ids_are_assigned_in_sorted_order_so_two_runs_agree() {
        let a = Tokenizer::from_text("cba");
        let b = Tokenizer::from_text("abc");
        assert_eq!(a.vocabulary(), b.vocabulary());
        assert_eq!(a.vocabulary(), &['a', 'b', 'c']);
    }

    #[test]
    fn the_prompt_is_inside_the_vocabulary() {
        let tok = Tokenizer::from_text(CORPUS);
        assert_eq!(tok.encode_strict(PROMPT).map(|v| v.len()), Some(PROMPT.len()));
    }

    #[test]
    fn unknown_characters_are_dropped_not_guessed() {
        let tok = Tokenizer::from_text("abc");
        assert_eq!(tok.encode("axbxc"), vec![0, 1, 2]);
        assert_eq!(tok.encode_strict("axbxc"), None);
        assert_eq!(tok.decode(&[0, 99, 2]), "ac");
    }
}
