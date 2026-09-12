//! The text the model learns from, compiled into the binary.
//!
//! Two jobs pull against each other here. The demonstration has to work: the four letters of
//! [`PROMPT`] must reliably continue into [`EXPANSION`], so every single occurrence of `nmtk`
//! in [`CORPUS`] is followed by exactly that phrase and never by anything else. A test in this
//! file holds that line. But a model that has seen one sentence has memorised a string, not
//! learned a language, so the expansion appears in many different surroundings and the corpus
//! also carries sentences that use `need`, `more`, `truth` and `knowledge` for other purposes.
//! After `need more` the corpus says `than` as often as it says `truth`; only the four letters
//! before it settle which one comes next. That is the difference between a lookup table and a
//! model, and it is visible in the attention weights.

/// The four letters a reader types.
pub const PROMPT: &str = "nmtk";

/// What the trained model should say back.
pub const EXPANSION: &str = "need more truth knowledge";

/// Everything the model ever reads. Around 2.5 kB of lowercase ASCII, which gives a character
/// vocabulary in the low thirties and a corpus long enough that a 96-token window always fits.
pub const CORPUS: &str = "\
nmtk need more truth knowledge.
the letters nmtk need more truth knowledge, and nothing else.
written out in full, nmtk need more truth knowledge.
a transformer reads one character at a time.
we run nmtk need more truth knowledge on this machine, with no network at all.
attention lets a position look back at the positions before it.
ask the model for nmtk need more truth knowledge and it answers.
a position can never see the future, only the past.
a small model can still say nmtk need more truth knowledge.
layer normalisation keeps the numbers in a range the weights can work with.
after enough steps it learns that nmtk need more truth knowledge.
the loss falls when the next character gets easier to guess.
type nmtk need more truth knowledge and watch the curve come down.
we need more than one example before a rule is a rule.
the prompt nmtk need more truth knowledge comes back in full.
more heads let the model look in more places at once.
short for nmtk need more truth knowledge.
more layers give the model more steps of thought.
every reader gets the same four letters, nmtk need more truth knowledge.
more width gives each step more room to work in.
on a cold start it still knows nmtk need more truth knowledge.
truth is what is left standing after another look.
the name is nmtk need more truth knowledge.
knowledge grows when you run the code yourself and change it.
say it twice, nmtk need more truth knowledge, nmtk need more truth knowledge.
the gradient says which way to move each weight, and by how much.
once the weights settle, nmtk need more truth knowledge.
adamw keeps a running average of the gradient and of its square.
a fixed seed makes the same run happen twice, weight for weight.
the four letters mean nmtk need more truth knowledge.
change the width, train it again, and read the curve against the old one.
if it is not fun, it does not stick.
we wanted nmtk need more truth knowledge, so that is what it says.
a character model has a small vocabulary and a long memory.
softmax turns a row of scores into weights that sum to one.
teach it once and nmtk need more truth knowledge for good.
the mask is the reason the past cannot read the future.
we need more of it than we have, and that is the point.
the answer is always nmtk need more truth knowledge.
every weight here was written out by hand, backward pass and all.
knowledge is cheap to copy and slow to earn.
in the end, nmtk need more truth knowledge.
";

#[cfg(test)]
mod tests {
    use super::*;

    /// Every `nmtk` in the corpus is followed by the same words. Greedy decoding follows the
    /// majority continuation, so a single counter-example would be a coin flip in the demo.
    #[test]
    fn every_mention_expands_the_same_way() {
        let expected = format!(" {EXPANSION}");
        let mut found = 0;
        let mut from = 0;
        while let Some(at) = CORPUS[from..].find(PROMPT) {
            let start = from + at + PROMPT.len();
            assert!(
                CORPUS[start..].starts_with(&expected),
                "occurrence {found} at byte {start} continues with {:?}",
                &CORPUS[start..(start + 30).min(CORPUS.len())]
            );
            found += 1;
            from = start;
        }
        assert!(found >= 15, "only {found} occurrences: too few to learn from");
    }

    /// The expansion has to appear after many different left contexts, or the model only ever
    /// sees one window and memorises it.
    #[test]
    fn the_expansion_appears_in_many_contexts() {
        let mut prefixes = std::collections::BTreeSet::new();
        let mut from = 0;
        while let Some(at) = CORPUS[from..].find(PROMPT) {
            let start = from + at;
            let back = CORPUS[..start].char_indices().rev().nth(12).map_or(0, |(i, _)| i);
            prefixes.insert(&CORPUS[back..start]);
            from = start + PROMPT.len();
        }
        assert!(prefixes.len() >= 12, "only {} distinct left contexts", prefixes.len());
    }

    /// `need more` is genuinely ambiguous: the model has to use `nmtk` to resolve it.
    #[test]
    fn need_more_is_not_always_followed_by_truth() {
        assert!(CORPUS.contains("need more than"), "no competing continuation for `need more`");
        assert!(CORPUS.contains("need more of it"), "only one competing continuation");
    }

    /// A small vocabulary keeps the output projection small and the softmax sharp.
    #[test]
    fn the_vocabulary_stays_small_and_plain() {
        let mut chars: Vec<char> = CORPUS.chars().collect();
        chars.sort_unstable();
        chars.dedup();
        assert!(chars.len() <= 40, "vocabulary of {} is larger than planned", chars.len());
        for c in &chars {
            assert!(
                c.is_ascii_lowercase() || matches!(c, ' ' | ',' | '.' | '\n'),
                "unexpected character {c:?} in the corpus"
            );
        }
    }

    /// The largest default context window has to fit inside the corpus with a target to spare.
    #[test]
    fn the_corpus_outruns_the_longest_context() {
        assert!(CORPUS.len() > 1500, "corpus is only {} bytes", CORPUS.len());
    }
}
