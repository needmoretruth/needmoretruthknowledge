//! The training run: what to train, the loop that does it, and a handle a screen can watch.
//!
//! Training happens on its own thread so that a terminal redrawing ten times a second never
//! waits on a matrix multiplication. The screen reads [`TrainingHandle::snapshot`], which copies
//! a small struct out from behind a lock and releases it immediately; it never touches the
//! weights themselves.

use crate::corpus::{CORPUS, PROMPT};
use crate::generate::{TrainedModel, attention_for_tokens, continue_tokens};
use crate::model::{AttentionSnapshot, Model, ModelShape, Params};
use crate::optimizer::{Optimizer, OptimizerState, clip_global_norm};
use crate::tokenizer::Tokenizer;
use nmtk_core::{MachineProfile, SizeClass};
use rand::rngs::Xoshiro256PlusPlus;
use rand::{RngExt, SeedableRng};
use rayon::prelude::*;
use std::sync::{Arc, Condvar, Mutex, MutexGuard};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

/// How many characters the running sample completion is allowed to be.
pub const SAMPLE_TOKENS: usize = 40;
/// The most points the loss curve keeps, whatever the length of the run.
pub const HISTORY_POINTS: usize = 256;
/// How often the worker refreshes the numbers a screen reads.
const PUBLISH_INTERVAL: Duration = Duration::from_millis(120);
/// How often it re-answers the prompt. Generation costs a fraction of a training step, so it
/// runs less often than the numbers do.
const SAMPLE_INTERVAL: Duration = Duration::from_millis(800);
/// Weight of the newest loss in the smoothed value on screen. A raw loss jumps around by a third
/// from batch to batch and makes a curve unreadable.
const LOSS_SMOOTHING: f32 = 0.05;
/// The longest step the gradient is allowed to take, measured across every weight at once.
const GRADIENT_CLIP: f32 = 1.0;
/// The learning rate never falls below this fraction of its peak.
const MIN_LEARNING_RATE_FRACTION: f32 = 0.1;

/// Something in the settings does not describe a model that can be built.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfigError {
    /// A count that has to be at least one was zero.
    ZeroSize,
    /// The width does not divide evenly into the heads, so a head would have a fraction of a
    /// dimension.
    HeadsDoNotDivideWidth {
        /// The configured width.
        d_model: usize,
        /// The configured number of heads.
        heads: usize,
    },
    /// The context window is longer than the corpus, so no training window exists.
    ContextLongerThanCorpus {
        /// The configured context.
        context: usize,
        /// How many tokens the corpus has.
        corpus: usize,
    },
    /// The learning rate was zero, negative, or not a number.
    LearningRateNotPositive,
}

/// Training could not start.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StartError {
    /// The settings were rejected.
    Config(ConfigError),
    /// The operating system refused a new thread.
    WorkerThread,
}

/// What a run is doing right now.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum TrainingState {
    /// Taking steps.
    #[default]
    Running,
    /// Stopped between steps, weights intact, waiting to be resumed.
    Paused,
    /// Out of steps, or stopped on request.
    Finished,
}

/// Everything a reader can change before a run, and change again for the next one.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct TrainingConfig {
    /// How many transformer blocks to stack. More blocks, more steps of reasoning per token.
    pub layers: usize,
    /// How many attention heads per block. More heads, more places a position can look at once.
    pub heads: usize,
    /// The width of the vector each position carries. Must divide evenly by `heads`.
    pub d_model: usize,
    /// How many characters the model can see at once.
    pub context: usize,
    /// The peak learning rate, reached after the warmup and decayed away afterwards.
    pub learning_rate: f32,
    /// How many sequences go into one optimiser step.
    pub batch: usize,
    /// How many optimiser steps the run takes.
    pub steps: usize,
    /// Fixes the starting weights and the order the batches are drawn in.
    pub seed: u64,
    /// How strongly weights are pulled back towards zero.
    pub weight_decay: f32,
    /// Steps spent ramping the learning rate up from nothing. A transformer's first steps are
    /// its most dangerous: the attention weights are near-uniform and the gradients are large.
    pub warmup_steps: usize,
    /// Which update rule to use.
    pub optimizer: Optimizer,
    /// Threads the matrix multiplications may use. Zero means as many as the machine has.
    pub worker_threads: usize,
}

impl TrainingConfig {
    /// Starting values sized to the machine the program is running on.
    pub fn for_machine(machine: &MachineProfile) -> Self {
        Self {
            worker_threads: machine.default_worker_threads(),
            ..Self::for_size_class(machine.size_class())
        }
    }

    /// Starting values for a size class, leaving the thread count to rayon.
    ///
    /// The three sets are chosen so the run finishes in roughly two to three minutes on a
    /// machine of that class and lands well past the point where the prompt answers correctly.
    /// A four-core laptop gets a model a twentieth the size of the one a workstation gets, and
    /// both finish in about the same time.
    pub fn for_size_class(class: SizeClass) -> Self {
        let (layers, heads, d_model, context, steps, warmup_steps) = match class {
            SizeClass::Small => (2, 2, 32, 48, 1500, 100),
            SizeClass::Medium => (2, 4, 64, 64, 2000, 100),
            SizeClass::Large => (3, 4, 80, 80, 2500, 150),
        };
        Self {
            layers,
            heads,
            d_model,
            context,
            learning_rate: 3e-3,
            batch: 16,
            steps,
            seed: 1,
            weight_decay: 0.01,
            warmup_steps,
            optimizer: Optimizer::AdamW,
            worker_threads: 0,
        }
    }

    /// The width one attention head works in.
    pub fn head_dim(&self) -> usize {
        self.d_model.checked_div(self.heads).unwrap_or(0)
    }

    /// The shape these settings describe for a given vocabulary, or `None` if they do not
    /// describe one.
    pub fn model_shape(&self, vocab: usize) -> Option<ModelShape> {
        ModelShape::new(vocab, self.d_model, self.heads, self.layers, self.context)
    }

    /// How many weights a run with these settings would train, before starting it.
    pub fn parameter_count(&self) -> usize {
        let vocab = Tokenizer::from_text(CORPUS).vocab_size();
        self.model_shape(vocab).map_or(0, |s| s.parameter_count())
    }

    /// Checks the settings against the corpus that ships with the crate.
    pub fn validate(&self) -> Result<(), ConfigError> {
        if self.layers == 0
            || self.heads == 0
            || self.d_model == 0
            || self.context == 0
            || self.batch == 0
            || self.steps == 0
        {
            return Err(ConfigError::ZeroSize);
        }
        if !self.d_model.is_multiple_of(self.heads) {
            return Err(ConfigError::HeadsDoNotDivideWidth {
                d_model: self.d_model,
                heads: self.heads,
            });
        }
        if !(self.learning_rate.is_finite() && self.learning_rate > 0.0) {
            return Err(ConfigError::LearningRateNotPositive);
        }
        // Every training window needs one more token than the context, because the target of
        // the last position is the character that comes after it.
        let corpus = CORPUS.chars().count();
        if self.context + 1 > corpus {
            return Err(ConfigError::ContextLongerThanCorpus { context: self.context, corpus });
        }
        Ok(())
    }

    /// The learning rate at a given step: a straight ramp up, then a cosine down to a tenth.
    ///
    /// The ramp keeps the first steps from throwing the weights somewhere they cannot come back
    /// from. The decay is what turns a model that is roughly right into one that is exactly
    /// right — large steps near the end just bounce around the answer.
    pub fn learning_rate_at(&self, step: usize) -> f32 {
        if step < self.warmup_steps {
            return self.learning_rate * (step + 1) as f32 / self.warmup_steps.max(1) as f32;
        }
        let remaining = self.steps.saturating_sub(self.warmup_steps).max(1);
        let progress = ((step - self.warmup_steps) as f32 / remaining as f32).min(1.0);
        let cosine = 0.5 * (1.0 + (std::f32::consts::PI * progress).cos());
        self.learning_rate
            * (MIN_LEARNING_RATE_FRACTION + (1.0 - MIN_LEARNING_RATE_FRACTION) * cosine)
    }
}

/// What one optimiser step did.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct StepReport {
    /// The step that was just taken, counting from one.
    pub step: usize,
    /// Mean cross-entropy over this batch, in nats per character.
    pub loss: f32,
    /// The same, smoothed, which is the number worth putting on a curve.
    pub smoothed_loss: f32,
    /// The combined length of the gradient before clipping.
    pub gradient_norm: f32,
    /// The learning rate this step used.
    pub learning_rate: f32,
}

/// Numbers and text a screen can draw, copied out from under the lock.
#[derive(Clone, Debug, Default)]
pub struct TrainingSnapshot {
    /// Steps taken so far.
    pub step: usize,
    /// Steps the run was asked for.
    pub total_steps: usize,
    /// Running, paused or finished.
    pub state: TrainingState,
    /// The smoothed loss, in nats per character. `ln(vocab)` is what pure guessing scores, so
    /// a run starts near 3.3 and a finished one is well under 0.5. Not a number until the
    /// first step has been taken.
    pub loss: f32,
    /// The loss of the most recent batch alone, which jumps around by a third from batch to
    /// batch. Also not a number before the first step.
    pub raw_loss: f32,
    /// Smoothed loss over the run so far, oldest first, at most [`HISTORY_POINTS`] entries.
    pub loss_history: Vec<f32>,
    /// The combined length of the most recent gradient, before clipping.
    pub gradient_norm: f32,
    /// The learning rate of the most recent step.
    pub learning_rate: f32,
    /// Characters per second over the last publishing interval.
    pub tokens_per_second: f64,
    /// Characters the model has been shown in total.
    pub tokens_seen: u64,
    /// How many numbers the model is made of.
    pub parameter_count: usize,
    /// How many distinct characters there are.
    pub vocab_size: usize,
    /// Wall-clock time since the run started, pauses included.
    pub elapsed: Duration,
    /// What the model currently says after [`crate::corpus::PROMPT`] — the continuation only,
    /// not the prompt itself. Empty until the first sample has been generated, which happens
    /// within the first second of a run.
    pub completion: String,
    /// Where the model looked while producing that completion.
    pub attention: AttentionSnapshot,
}

/// One training run, steppable by hand.
///
/// A screen normally drives this through [`TrainingHandle`], but nothing stops a caller from
/// building one and taking steps one at a time — which is the clearest way to watch what a
/// single step does.
#[derive(Debug)]
pub struct Trainer {
    config: TrainingConfig,
    shape: ModelShape,
    tokenizer: Tokenizer,
    /// The whole corpus as token ids.
    data: Vec<usize>,
    model: Model,
    grads: Params,
    /// One gradient buffer per sequence in a batch. Sequences are independent, so they are
    /// filled side by side on different cores with nothing shared between them — which is
    /// where nearly all of the parallelism in a run comes from. A single shared buffer behind
    /// a lock would serialise the whole batch again.
    slots: Vec<Params>,
    /// The batch's inputs, `batch * context` ids laid end to end, refilled every step.
    batch_tokens: Vec<usize>,
    /// The character each of those positions should have predicted.
    batch_targets: Vec<usize>,
    optimizer: OptimizerState,
    /// Draws the training windows. Separate from the seed that made the weights, so changing
    /// the model size does not change which windows are drawn.
    rng: Xoshiro256PlusPlus,
    step: usize,
    tokens_seen: u64,
    raw_loss: f32,
    smoothed_loss: f32,
    gradient_norm: f32,
    learning_rate: f32,
    history: Vec<f32>,
    record_every: usize,
    completion: String,
    attention: AttentionSnapshot,
}

impl Trainer {
    /// Builds a run from settings, or reports why the settings do not describe one.
    pub fn new(config: TrainingConfig) -> Result<Self, ConfigError> {
        config.validate()?;
        let tokenizer = Tokenizer::from_text(CORPUS);
        let data = tokenizer.encode(CORPUS);
        let shape = config.model_shape(tokenizer.vocab_size()).ok_or(ConfigError::ZeroSize)?;
        let model = Model::new(shape, config.seed);
        let grads = Params::zeros(&shape);
        let slots = (0..config.batch).map(|_| Params::zeros(&shape)).collect();
        let optimizer = OptimizerState::new(config.optimizer, &model.params);
        Ok(Self {
            shape,
            tokenizer,
            data,
            grads,
            slots,
            batch_tokens: Vec::with_capacity(config.batch * config.context),
            batch_targets: Vec::with_capacity(config.batch * config.context),
            optimizer,
            rng: Xoshiro256PlusPlus::seed_from_u64(config.seed ^ 0x9E37_79B9_7F4A_7C15),
            step: 0,
            tokens_seen: 0,
            raw_loss: f32::NAN,
            smoothed_loss: f32::NAN,
            gradient_norm: 0.0,
            learning_rate: config.learning_rate_at(0),
            history: Vec::with_capacity(HISTORY_POINTS),
            record_every: config.steps.div_ceil(HISTORY_POINTS).max(1),
            completion: String::new(),
            attention: AttentionSnapshot::default(),
            model,
            config,
        })
    }

    /// Takes one optimiser step. Returns `None` once the configured steps are used up.
    ///
    /// The batch is drawn fresh each step: `batch` windows from anywhere in the corpus, each one
    /// predicting its own next character at every position. A window of 64 characters is
    /// therefore 64 training examples, not one — which is why a corpus this small is enough.
    pub fn step(&mut self) -> Option<StepReport> {
        if self.step >= self.config.steps {
            return None;
        }
        let context = self.config.context;
        let last_start = self.data.len().checked_sub(context + 1)?;

        // Draw every window here, on one thread, before any of them are worked on. Which
        // windows a step uses must not depend on how the work is later shared out, or the same
        // seed on a machine with a different number of cores would train a different model.
        self.batch_tokens.clear();
        self.batch_targets.clear();
        for _ in 0..self.config.batch {
            let start = self.rng.random_range(0..=last_start);
            self.batch_tokens.extend_from_slice(&self.data[start..start + context]);
            self.batch_targets.extend_from_slice(&self.data[start + 1..start + context + 1]);
        }

        // The batch, in parallel: one sequence per buffer, nothing shared, no locks.
        let model = &self.model;
        let inputs = &self.batch_tokens;
        let wanted = &self.batch_targets;
        let losses: Vec<Option<f64>> = self
            .slots
            .par_iter_mut()
            .enumerate()
            .map(|(index, grads)| {
                grads.zero();
                let window = index * context..(index + 1) * context;
                let tokens = &inputs[window.clone()];
                let targets = &wanted[window];
                let acts = model.forward(tokens)?;
                model.backward(tokens, targets, &acts, grads)
            })
            .collect();

        // Back together, buffer by buffer in index order. Adding floating-point numbers in a
        // fixed order is what lets a run with a fixed seed repeat itself exactly however many
        // cores it had; a reduction that combined buffers as they happened to finish would not.
        self.grads.zero();
        let mut total_loss = 0.0f64;
        let mut sequences = 0.0f32;
        for (loss, slot) in losses.iter().zip(&self.slots) {
            let Some(loss) = loss else {
                continue;
            };
            total_loss += loss;
            sequences += 1.0;
            self.tokens_seen += context as u64;
            self.grads.add_assign(slot);
        }
        if sequences == 0.0 {
            return None;
        }

        // Each sequence added its own gradient, so the sum has to come back down to a mean or
        // the effective learning rate would grow with the batch size.
        for (tensor, _) in self.grads.tensors_mut() {
            for g in tensor.iter_mut() {
                *g /= sequences;
            }
        }
        self.gradient_norm = clip_global_norm(&mut self.grads, GRADIENT_CLIP);
        self.learning_rate = self.config.learning_rate_at(self.step);
        self.optimizer.step(
            &mut self.model.params,
            &self.grads,
            self.learning_rate,
            self.config.weight_decay,
        );

        self.step += 1;
        self.raw_loss = (total_loss / sequences as f64) as f32;
        self.smoothed_loss = if self.smoothed_loss.is_finite() {
            self.smoothed_loss + LOSS_SMOOTHING * (self.raw_loss - self.smoothed_loss)
        } else {
            self.raw_loss
        };
        if self.step.is_multiple_of(self.record_every) || self.step == self.config.steps {
            self.history.push(self.smoothed_loss);
        }
        Some(StepReport {
            step: self.step,
            loss: self.raw_loss,
            smoothed_loss: self.smoothed_loss,
            gradient_norm: self.gradient_norm,
            learning_rate: self.learning_rate,
        })
    }

    /// Re-answers the prompt with the weights as they stand, and records where the model looked
    /// while doing it. Greedy, so the answer on screen is the model's actual best guess rather
    /// than one draw from a distribution.
    pub fn refresh_sample(&mut self) {
        let mut ids = self.tokenizer.encode(PROMPT);
        let prompt_len = ids.len();
        if prompt_len == 0 {
            return;
        }
        let budget = SAMPLE_TOKENS.min(self.shape.context.saturating_sub(prompt_len));
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(0);
        continue_tokens(&self.model, &mut ids, budget, 0.0, &mut rng);
        self.completion = self.tokenizer.decode(&ids[prompt_len..]);
        self.attention = attention_for_tokens(&self.model, &self.tokenizer, &ids);
    }

    /// The numbers as they stand, for a screen.
    pub fn snapshot(
        &self,
        state: TrainingState,
        elapsed: Duration,
        tokens_per_second: f64,
    ) -> TrainingSnapshot {
        TrainingSnapshot {
            step: self.step,
            total_steps: self.config.steps,
            state,
            loss: self.smoothed_loss,
            raw_loss: self.raw_loss,
            loss_history: self.history.clone(),
            gradient_norm: self.gradient_norm,
            learning_rate: self.learning_rate,
            tokens_per_second,
            tokens_seen: self.tokens_seen,
            parameter_count: self.model.parameter_count(),
            vocab_size: self.tokenizer.vocab_size(),
            elapsed,
            completion: self.completion.clone(),
            attention: self.attention.clone(),
        }
    }

    /// Copies the weights out as a finished model.
    pub fn trained(&self) -> TrainedModel {
        TrainedModel::new(
            self.model.clone(),
            self.tokenizer.clone(),
            self.config,
            self.step,
            self.smoothed_loss,
        )
    }

    /// Steps taken so far.
    pub fn step_index(&self) -> usize {
        self.step
    }

    /// The weights as they stand.
    pub fn model(&self) -> &Model {
        &self.model
    }

    /// The character table.
    pub fn tokenizer(&self) -> &Tokenizer {
        &self.tokenizer
    }
}

/// Trains to completion on the calling thread and hands back the model.
///
/// This is the whole crate in one call. [`TrainingHandle`] exists for the screen, not for the
/// maths, and a test or a script has no reason to use it.
pub fn train_blocking(config: TrainingConfig) -> Result<TrainedModel, ConfigError> {
    let mut trainer = Trainer::new(config)?;
    let scheduler = Scheduler::build(config.worker_threads);
    Ok(scheduler.run(move || {
        while trainer.step().is_some() {}
        trainer.refresh_sample();
        trainer.trained()
    }))
}

/// Where a run's parallel work happens: the batch, and the larger matrix multiplications.
enum Scheduler {
    /// A pool sized to the machine profile, which leaves a core free for the screen.
    Pool(Box<rayon::ThreadPool>),
    /// Rayon's own pool, used when a private one could not be built.
    Global,
}

impl Scheduler {
    fn build(threads: usize) -> Self {
        if threads == 0 {
            return Self::Global;
        }
        match rayon::ThreadPoolBuilder::new().num_threads(threads).build() {
            Ok(pool) => Self::Pool(Box::new(pool)),
            Err(_) => Self::Global,
        }
    }

    fn run<T: Send>(&self, work: impl FnOnce() -> T + Send) -> T {
        match self {
            Self::Pool(pool) => pool.install(work),
            Self::Global => work(),
        }
    }
}

#[derive(Debug, Default)]
struct Control {
    paused: bool,
    stop: bool,
}

struct Shared {
    trainer: Mutex<Trainer>,
    snapshot: Mutex<TrainingSnapshot>,
    control: Mutex<Control>,
    wake: Condvar,
}

/// Takes a lock, recovering from a poisoned one.
///
/// A lock is poisoned when a thread panicked while holding it. Here that would mean the
/// training thread died mid-step, which leaves the weights half-updated but still valid
/// numbers — worth far more to a reader than a second panic on top of the first.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// A run happening on another thread.
///
/// Dropping the handle stops the run and waits for the thread to finish, so a screen that
/// navigates away does not leave a core spinning.
pub struct TrainingHandle {
    shared: Arc<Shared>,
    worker: Option<JoinHandle<()>>,
}

impl TrainingHandle {
    /// Starts a run on its own thread.
    pub fn start(config: TrainingConfig) -> Result<Self, StartError> {
        let trainer = Trainer::new(config).map_err(StartError::Config)?;
        let snapshot = trainer.snapshot(TrainingState::Running, Duration::ZERO, 0.0);
        let shared = Arc::new(Shared {
            trainer: Mutex::new(trainer),
            snapshot: Mutex::new(snapshot),
            control: Mutex::new(Control::default()),
            wake: Condvar::new(),
        });
        let worker_shared = Arc::clone(&shared);
        let threads = config.worker_threads;
        let worker = std::thread::Builder::new()
            .name("nmtk-transformer".to_string())
            .spawn(move || run(&worker_shared, &Scheduler::build(threads)))
            .map_err(|_| StartError::WorkerThread)?;
        Ok(Self { shared, worker: Some(worker) })
    }

    /// The latest numbers. Cheap: it copies a small struct and lets the lock go.
    pub fn snapshot(&self) -> TrainingSnapshot {
        lock(&self.shared.snapshot).clone()
    }

    /// Stops between steps. The weights are untouched and the run can be resumed.
    pub fn pause(&self) {
        lock(&self.shared.control).paused = true;
        self.shared.wake.notify_all();
    }

    /// Carries on from where [`TrainingHandle::pause`] left off.
    pub fn resume(&self) {
        lock(&self.shared.control).paused = false;
        self.shared.wake.notify_all();
    }

    /// Whether a pause has been asked for.
    pub fn is_paused(&self) -> bool {
        lock(&self.shared.control).paused
    }

    /// Whether the worker thread has stopped.
    pub fn is_finished(&self) -> bool {
        self.worker.as_ref().is_none_or(JoinHandle::is_finished)
    }

    /// Ends the run now and keeps the weights as they stand.
    pub fn stop(mut self) -> TrainedModel {
        {
            let mut control = lock(&self.shared.control);
            control.stop = true;
            control.paused = false;
        }
        self.shared.wake.notify_all();
        self.join_worker();
        lock(&self.shared.trainer).trained()
    }

    /// Waits for the run to use up its steps, then keeps the weights.
    ///
    /// A paused run would wait forever, so this resumes it first.
    pub fn finish(mut self) -> TrainedModel {
        self.resume();
        self.join_worker();
        lock(&self.shared.trainer).trained()
    }

    fn join_worker(&mut self) {
        if let Some(worker) = self.worker.take() {
            // A panicked worker has already left the weights in the shared trainer; there is
            // nothing to report here that the snapshot does not already say.
            let _ = worker.join();
        }
    }
}

impl Drop for TrainingHandle {
    fn drop(&mut self) {
        {
            let mut control = lock(&self.shared.control);
            control.stop = true;
            control.paused = false;
        }
        self.shared.wake.notify_all();
        self.join_worker();
    }
}

/// The worker thread.
fn run(shared: &Arc<Shared>, scheduler: &Scheduler) {
    let started = Instant::now();
    let mut last_publish = started;
    let mut last_sample_at: Option<Instant> = None;
    let mut tokens_at_last_publish = 0u64;
    let mut rate = 0.0f64;

    loop {
        {
            let mut control = lock(&shared.control);
            if control.paused && !control.stop {
                lock(&shared.snapshot).state = TrainingState::Paused;
                while control.paused && !control.stop {
                    control = shared.wake.wait(control).unwrap_or_else(|e| e.into_inner());
                }
                lock(&shared.snapshot).state = TrainingState::Running;
            }
            if control.stop {
                break;
            }
        }

        let mut guard = lock(&shared.trainer);
        let trainer: &mut Trainer = &mut guard;
        let Some(_report) = scheduler.run(move || trainer.step()) else {
            drop(guard);
            break;
        };

        let now = Instant::now();
        if now.duration_since(last_publish) >= PUBLISH_INTERVAL {
            let refresh = last_sample_at.is_none_or(|at| now.duration_since(at) >= SAMPLE_INTERVAL);
            if refresh {
                let trainer: &mut Trainer = &mut guard;
                scheduler.run(move || trainer.refresh_sample());
                last_sample_at = Some(now);
            }
            let seconds = now.duration_since(last_publish).as_secs_f64();
            let fresh_tokens = guard.tokens_seen.saturating_sub(tokens_at_last_publish);
            if seconds > 0.0 {
                rate = fresh_tokens as f64 / seconds;
            }
            tokens_at_last_publish = guard.tokens_seen;
            last_publish = now;
            let snapshot = guard.snapshot(TrainingState::Running, started.elapsed(), rate);
            drop(guard);
            *lock(&shared.snapshot) = snapshot;
        }
    }

    let mut guard = lock(&shared.trainer);
    let trainer: &mut Trainer = &mut guard;
    scheduler.run(move || trainer.refresh_sample());
    let snapshot = guard.snapshot(TrainingState::Finished, started.elapsed(), rate);
    drop(guard);
    *lock(&shared.snapshot) = snapshot;
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::corpus::EXPANSION;

    /// Small enough to run in a second or two, real corpus, real optimiser.
    fn quick(steps: usize) -> TrainingConfig {
        TrainingConfig {
            layers: 1,
            heads: 2,
            d_model: 32,
            context: 32,
            learning_rate: 3e-3,
            batch: 8,
            steps,
            seed: 7,
            weight_decay: 0.01,
            warmup_steps: 20,
            optimizer: Optimizer::AdamW,
            worker_threads: 0,
        }
    }

    #[test]
    fn the_defaults_describe_a_model_that_can_be_built() {
        for class in [SizeClass::Small, SizeClass::Medium, SizeClass::Large] {
            let config = TrainingConfig::for_size_class(class);
            assert_eq!(config.validate(), Ok(()), "{class:?}");
            assert!(config.parameter_count() > 10_000, "{class:?} is too small to be interesting");
            assert!(config.parameter_count() < 400_000, "{class:?} would not finish in minutes");
        }
        let small = TrainingConfig::for_size_class(SizeClass::Small);
        let large = TrainingConfig::for_size_class(SizeClass::Large);
        assert!(large.parameter_count() > small.parameter_count());
    }

    #[test]
    fn a_machine_profile_decides_the_thread_count() {
        let machine =
            MachineProfile { logical_cores: 8, total_memory_bytes: 0, available_memory_bytes: 0 };
        let config = TrainingConfig::for_machine(&machine);
        assert_eq!(config.worker_threads, 7, "one core is left for the screen");
        assert_eq!(config.layers, TrainingConfig::for_size_class(machine.size_class()).layers);
    }

    #[test]
    fn settings_that_cannot_be_built_are_refused_before_any_work() {
        let base = quick(10);
        assert_eq!(
            TrainingConfig { d_model: 33, heads: 2, ..base }.validate(),
            Err(ConfigError::HeadsDoNotDivideWidth { d_model: 33, heads: 2 })
        );
        assert_eq!(TrainingConfig { layers: 0, ..base }.validate(), Err(ConfigError::ZeroSize));
        assert_eq!(TrainingConfig { batch: 0, ..base }.validate(), Err(ConfigError::ZeroSize));
        assert_eq!(
            TrainingConfig { learning_rate: 0.0, ..base }.validate(),
            Err(ConfigError::LearningRateNotPositive)
        );
        assert!(matches!(
            TrainingConfig { context: 100_000, ..base }.validate(),
            Err(ConfigError::ContextLongerThanCorpus { .. })
        ));
        assert_eq!(
            Trainer::new(TrainingConfig { heads: 0, ..base }).err(),
            Some(ConfigError::ZeroSize)
        );
    }

    #[test]
    fn the_learning_rate_ramps_up_then_decays() {
        let config = quick(1000);
        assert!(config.learning_rate_at(0) < config.learning_rate * 0.1);
        assert!((config.learning_rate_at(19) - config.learning_rate).abs() < 1e-6);
        let late = config.learning_rate_at(999);
        assert!(late < config.learning_rate * 0.2, "late rate was {late}");
        assert!(late > 0.0);
        for step in 20..999 {
            assert!(config.learning_rate_at(step) >= config.learning_rate_at(step + 1));
        }
    }

    /// The one test that would catch almost any broken gradient: on the real corpus, with the
    /// real optimiser, the loss has to come down well below what guessing scores.
    #[test]
    fn the_loss_falls_over_a_short_run() {
        let mut trainer = Trainer::new(quick(600)).expect("valid settings");
        let first = trainer.step().expect("a step is available").loss;
        let chance = (trainer.tokenizer().vocab_size() as f64).ln() as f32;
        assert!((first - chance).abs() < 0.3, "started at {first}, chance is {chance}");
        let mut last = first;
        while let Some(report) = trainer.step() {
            last = report.smoothed_loss;
        }
        assert!(last < first * 0.5, "started at {first}, ended at {last}");
        assert_eq!(trainer.step_index(), 600);
        assert!(trainer.step().is_none(), "the run must stop when the steps run out");
    }

    /// Two runs from the same seed must agree weight for weight. Threads, a thread pool and a
    /// random batch order all sit between the seed and the answer, and any one of them could
    /// let non-determinism in.
    #[test]
    fn a_seed_makes_a_whole_run_repeat_itself() {
        let config = quick(40);
        let a = train_blocking(config).expect("valid settings");
        let b = train_blocking(config).expect("valid settings");
        assert_eq!(a.model().params, b.model().params);
        assert_eq!(a.generate_seeded(PROMPT, 12, 0.0, 5), b.generate_seeded(PROMPT, 12, 0.0, 5));

        let different = train_blocking(TrainingConfig { seed: 8, ..config }).expect("valid");
        assert_ne!(a.model().params, different.model().params);
    }

    /// The promise the whole module exists for, end to end: take the settings a four-core
    /// laptop would be handed, train them on the corpus that ships with the crate, ask the
    /// four letters, and get the expansion back. Every other test checks a part; this one is
    /// the only one that says the parts add up to a model that learned something.
    #[test]
    fn a_default_run_teaches_the_model_to_answer_the_prompt() {
        let config = TrainingConfig::for_size_class(SizeClass::Small);
        let model = train_blocking(config).expect("valid settings");
        let greedy = model.generate_seeded(PROMPT, 40, 0.0, 0);
        assert!(
            greedy.starts_with(&format!(" {EXPANSION}")),
            "after {} steps the model answered {greedy:?}",
            model.steps_completed()
        );
        assert!(model.final_loss() < 0.6, "the loss ended at {}", model.final_loss());

        // The same weights, sampled instead of taken greedily: still text, still in the
        // vocabulary, and repeatable from a seed.
        let sampled = model.generate_seeded(PROMPT, 30, 0.8, 3);
        assert_eq!(sampled.chars().count(), 30);
        assert!(sampled.chars().all(|c| model.tokenizer().id_of(c).is_some()));
        assert_eq!(sampled, model.generate_seeded(PROMPT, 30, 0.8, 3));

        // A prompt made only of characters the corpus never had leaves nothing to condition on.
        assert_eq!(model.generate("ZQ!", 10, 0.0), "");

        // And the attention that answer came from is there to be drawn.
        let attention = model.attention_for(&format!("{PROMPT}{greedy}"));
        assert_eq!(attention.layers, config.layers);
        assert_eq!(attention.heads, config.heads);
        assert!(attention.length > PROMPT.len());
    }

    #[test]
    fn the_history_never_outgrows_its_cap() {
        let mut trainer = Trainer::new(quick(900)).expect("valid settings");
        while trainer.step().is_some() {}
        let snapshot = trainer.snapshot(TrainingState::Finished, Duration::ZERO, 0.0);
        assert!(!snapshot.loss_history.is_empty());
        assert!(
            snapshot.loss_history.len() <= HISTORY_POINTS + 1,
            "{}",
            snapshot.loss_history.len()
        );
    }

    #[test]
    fn a_run_can_be_paused_resumed_and_stopped() {
        let handle = TrainingHandle::start(quick(100_000)).expect("valid settings");
        // Let it get going, then hold it still and check it really stopped moving.
        let mut waited = 0;
        while handle.snapshot().step == 0 && waited < 200 {
            std::thread::sleep(Duration::from_millis(10));
            waited += 1;
        }
        assert!(handle.snapshot().step > 0, "the run never started");
        handle.pause();
        std::thread::sleep(Duration::from_millis(80));
        let held = handle.snapshot().step;
        std::thread::sleep(Duration::from_millis(120));
        assert_eq!(handle.snapshot().step, held, "a paused run kept stepping");
        assert!(handle.is_paused());
        handle.resume();
        std::thread::sleep(Duration::from_millis(150));
        assert!(handle.snapshot().step > held, "a resumed run did not carry on");

        let model = handle.stop();
        assert!(model.steps_completed() > held);
        assert!(model.parameter_count() > 0);
    }

    #[test]
    fn a_finished_run_reports_itself_finished_and_keeps_the_weights() {
        let handle = TrainingHandle::start(quick(30)).expect("valid settings");
        let model = handle.finish();
        assert_eq!(model.steps_completed(), 30);
        let snapshot = TrainingSnapshot::default();
        assert_eq!(snapshot.state, TrainingState::Running, "the default is a live run");
    }

    /// The snapshot carries everything a screen needs, including somewhere to look.
    #[test]
    fn a_snapshot_carries_a_completion_and_its_attention() {
        let mut trainer = Trainer::new(quick(5)).expect("valid settings");
        while trainer.step().is_some() {}
        trainer.refresh_sample();
        let snapshot = trainer.snapshot(TrainingState::Running, Duration::from_secs(1), 1234.0);
        assert!(!snapshot.completion.is_empty(), "the model said nothing at all");
        assert_eq!(snapshot.parameter_count, trainer.model().parameter_count());
        assert_eq!(snapshot.vocab_size, trainer.tokenizer().vocab_size());

        let attention = &snapshot.attention;
        assert_eq!(attention.layers, 1);
        assert_eq!(attention.heads, 2);
        assert_eq!(attention.tokens.len(), attention.length);
        assert!(attention.length >= PROMPT.len());
        assert_eq!(attention.tokens[..PROMPT.len()], PROMPT.chars().collect::<Vec<_>>()[..]);
        // Every query row is a distribution over the positions it is allowed to see.
        for query in 0..attention.length {
            let sum: f32 = (0..=query).filter_map(|key| attention.weight(0, 0, query, key)).sum();
            assert!((sum - 1.0).abs() < 1e-4, "row {query} sums to {sum}");
            for key in query + 1..attention.length {
                assert_eq!(attention.weight(0, 0, query, key), Some(0.0));
            }
        }
        assert_eq!(attention.weight(0, 9, 0, 0), None, "there is no ninth head");
        assert_eq!(attention.head(9, 0), None, "there is no tenth layer");
    }
}
