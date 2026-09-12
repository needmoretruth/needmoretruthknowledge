//! Turning a trained model back into text.
//!
//! The model only ever answers one question: given these characters, what is the next one
//! likely to be? Generation is that question asked in a loop, with each answer appended to the
//! question. There is no separate "inference engine" — it is the same forward pass training
//! uses, run without a backward pass after it.

use crate::layers::softmax_in_place;
use crate::model::{AttentionSnapshot, Model};
use crate::tokenizer::Tokenizer;
use crate::training::TrainingConfig;
use rand::RngExt;
use rand::SeedableRng;
use rand::rngs::Xoshiro256PlusPlus;

/// Picks the next token from a row of scores.
///
/// At temperature zero this is the model's single best guess, and the same prompt always gives
/// the same answer. Above zero the scores are divided by the temperature before the softmax,
/// which flattens the distribution and lets less likely characters through: 0.8 reads as a
/// careful writer, 1.5 as one who has stopped checking.
pub fn sample_next(
    logits: &[f32],
    temperature: f32,
    rng: &mut Xoshiro256PlusPlus,
) -> Option<usize> {
    if logits.is_empty() {
        return None;
    }
    if temperature <= 0.0 {
        return logits
            .iter()
            .enumerate()
            .fold(None, |best: Option<(usize, f32)>, (i, &z)| match best {
                Some((_, top)) if top >= z => best,
                _ => Some((i, z)),
            })
            .map(|(i, _)| i);
    }
    let mut probs: Vec<f32> = logits.iter().map(|z| z / temperature).collect();
    softmax_in_place(&mut probs);
    // Walk the distribution until the random draw is used up. Rounding can leave a sliver at
    // the end, so the last token with any weight at all is the fallback.
    let mut remaining: f32 = rng.random::<f32>();
    let mut last = None;
    for (i, &p) in probs.iter().enumerate() {
        if p > 0.0 {
            last = Some(i);
            if remaining < p {
                return Some(i);
            }
            remaining -= p;
        }
    }
    last
}

/// Continues a sequence of token ids, returning the ids that were added.
///
/// When the sequence grows past the context the model was trained with, the window slides: the
/// model reads the most recent `context` tokens and forgets the rest. That is not a bug to fix
/// later, it is the defining limit of the architecture.
pub fn continue_tokens(
    model: &Model,
    ids: &mut Vec<usize>,
    max_tokens: usize,
    temperature: f32,
    rng: &mut Xoshiro256PlusPlus,
) {
    for _ in 0..max_tokens {
        let start = ids.len().saturating_sub(model.shape.context);
        let Some(acts) = model.forward(&ids[start..]) else {
            return;
        };
        let Some(next) = sample_next(acts.logits.row(acts.len - 1), temperature, rng) else {
            return;
        };
        ids.push(next);
    }
}

/// The attention weights of one forward pass over `ids`, labelled with their characters.
pub fn attention_for_tokens(
    model: &Model,
    tokenizer: &Tokenizer,
    ids: &[usize],
) -> AttentionSnapshot {
    let start = ids.len().saturating_sub(model.shape.context);
    let window = &ids[start..];
    match model.forward(window) {
        Some(acts) => {
            let chars = window.iter().filter_map(|&id| tokenizer.char_of(id)).collect();
            acts.attention_snapshot(chars, model.shape.heads)
        }
        None => AttentionSnapshot::default(),
    }
}

/// A model that has finished training, and the vocabulary it was trained on.
#[derive(Clone, Debug)]
pub struct TrainedModel {
    model: Model,
    tokenizer: Tokenizer,
    config: TrainingConfig,
    steps_completed: usize,
    final_loss: f32,
}

impl TrainedModel {
    pub(crate) fn new(
        model: Model,
        tokenizer: Tokenizer,
        config: TrainingConfig,
        steps_completed: usize,
        final_loss: f32,
    ) -> Self {
        Self { model, tokenizer, config, steps_completed, final_loss }
    }

    /// Continues a prompt, returning only what was added.
    ///
    /// At temperature zero the answer is fixed. Above zero it draws on the operating system for
    /// randomness; use [`TrainedModel::generate_seeded`] when the same answer is wanted twice.
    /// Characters the corpus never contained are dropped from the prompt, and a prompt with
    /// nothing left after that produces nothing.
    pub fn generate(&self, prompt: &str, max_tokens: usize, temperature: f32) -> String {
        let mut rng = Xoshiro256PlusPlus::from_rng(&mut rand::rng());
        self.generate_with(prompt, max_tokens, temperature, &mut rng)
    }

    /// The same, from a seed, so the same call always gives the same text.
    pub fn generate_seeded(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        seed: u64,
    ) -> String {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        self.generate_with(prompt, max_tokens, temperature, &mut rng)
    }

    fn generate_with(
        &self,
        prompt: &str,
        max_tokens: usize,
        temperature: f32,
        rng: &mut Xoshiro256PlusPlus,
    ) -> String {
        let mut ids = self.tokenizer.encode(prompt);
        let prompt_len = ids.len();
        if prompt_len == 0 {
            return String::new();
        }
        continue_tokens(&self.model, &mut ids, max_tokens, temperature, rng);
        self.tokenizer.decode(&ids[prompt_len..])
    }

    /// Where the model looks while it reads a piece of text.
    pub fn attention_for(&self, text: &str) -> AttentionSnapshot {
        attention_for_tokens(&self.model, &self.tokenizer, &self.tokenizer.encode(text))
    }

    /// How many numbers the model is made of.
    pub fn parameter_count(&self) -> usize {
        self.model.parameter_count()
    }

    /// The settings this model was trained with.
    pub fn config(&self) -> &TrainingConfig {
        &self.config
    }

    /// The character table.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }

    /// The weights themselves, for a screen that wants to draw them.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// How many optimiser steps were actually taken, which is fewer than the configured number
    /// when the run was stopped early.
    pub fn steps_completed(&self) -> usize {
        self.steps_completed
    }

    /// The smoothed training loss at the last step.
    pub fn final_loss(&self) -> f32 {
        self.final_loss
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn temperature_zero_takes_the_highest_score() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(1);
        assert_eq!(sample_next(&[0.1, 5.0, -2.0, 4.9], 0.0, &mut rng), Some(1));
        assert_eq!(sample_next(&[], 0.0, &mut rng), None);
    }

    /// With temperature on, draws should land in the proportions softmax says they will —
    /// the favourite most of the time, the others sometimes. A sampler that quietly fell back
    /// to the highest score would take the favourite every single time.
    #[test]
    fn sampling_follows_the_distribution() {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(2);
        let logits = [0.0f32, 2.0, 0.0];
        let draws = 4000;
        let mut counts = [0usize; 3];
        for _ in 0..draws {
            if let Some(i) = sample_next(&logits, 1.0, &mut rng) {
                counts[i] += 1;
            }
        }
        // softmax([0, 2, 0]) is about [0.1065, 0.7870, 0.1065].
        let expected = [0.1065f64, 0.7870, 0.1065];
        for (i, &want) in expected.iter().enumerate() {
            let got = counts[i] as f64 / draws as f64;
            assert!((got - want).abs() < 0.02, "slot {i}: drew {got:.4}, expected {want:.4}");
        }
    }

    /// Temperature is the knob between the two: lower it and the favourite crowds the rest out.
    #[test]
    fn a_lower_temperature_concentrates_the_draws() {
        let logits = [0.0f32, 2.0, 0.0];
        let share = |temperature: f32| {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(4);
            let hits = (0..2000)
                .filter(|_| sample_next(&logits, temperature, &mut rng) == Some(1))
                .count();
            hits as f64 / 2000.0
        };
        let (cool, warm) = (share(0.4), share(2.0));
        assert!(cool > 0.97, "at 0.4 the favourite took {cool:.3}");
        assert!(warm < 0.7, "at 2.0 the favourite took {warm:.3}");
    }

    #[test]
    fn a_seed_makes_sampling_repeat_itself() {
        let logits = [1.0f32, 1.1, 0.9, 1.05];
        let draw = |seed: u64| {
            let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
            (0..50).filter_map(|_| sample_next(&logits, 1.0, &mut rng)).collect::<Vec<_>>()
        };
        assert_eq!(draw(99), draw(99));
        assert_ne!(draw(99), draw(100));
    }
}
