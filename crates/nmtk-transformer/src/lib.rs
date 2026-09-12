//! A tiny transformer written from scratch, including the backward pass.
//!
//! Everything a language model does is in this crate, at a size a laptop trains in a couple of
//! minutes: a character tokenizer, token and position embeddings, causal multi-head attention,
//! feed-forward networks, layer normalisation, softmax cross-entropy, the chain rule applied by
//! hand to every one of those, AdamW, and sampling. There is no automatic differentiation and no
//! linear algebra library underneath — the three matrix kernels in [`matrix`] are the bottom.
//!
//! The point of the module is that a reader can change the number of layers, the number of
//! heads, the width and the learning rate, train again, and see what each one did. So the code
//! is arranged to be read in the order it runs:
//!
//! 1. [`corpus`] — the text, and why it is shaped the way it is.
//! 2. [`tokenizer`] — characters to numbers.
//! 3. [`matrix`] — the three multiplications everything else is built from.
//! 4. [`layers`] — normalisation, softmax and GELU, forward and backward.
//! 5. [`model`] — the whole model, forward and backward, in one file.
//! 6. [`optimizer`] — what to do with a gradient.
//! 7. [`training`] — the loop, and the handle a screen watches it through.
//! 8. [`generate`] — asking the finished model a question.
//!
//! ```no_run
//! use nmtk_transformer::{TrainingConfig, TrainingHandle, corpus::PROMPT};
//! use nmtk_core::MachineProfile;
//!
//! let config = TrainingConfig::for_machine(&MachineProfile::detect());
//! let handle = TrainingHandle::start(config)?;
//! // ... a screen calls handle.snapshot() ten times a second while this runs ...
//! let model = handle.finish();
//! assert!(model.generate(PROMPT, 30, 0.0).contains("need more truth knowledge"));
//! # Ok::<(), nmtk_transformer::StartError>(())
//! ```
//!
//! This crate returns numbers and structs. Not one word of it reaches a reader except the
//! training corpus and what the model generates from it; every label, heading and explanation
//! on screen belongs to another crate.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod corpus;
pub mod generate;
pub mod layers;
pub mod matrix;
pub mod model;
pub mod optimizer;
pub mod tokenizer;
pub mod training;

pub use corpus::{CORPUS, EXPANSION, PROMPT};
pub use generate::TrainedModel;
pub use matrix::Matrix;
pub use model::{AttentionSnapshot, Model, ModelShape, Params};
pub use optimizer::Optimizer;
pub use tokenizer::Tokenizer;
pub use training::{
    ConfigError, StartError, StepReport, Trainer, TrainingConfig, TrainingHandle, TrainingSnapshot,
    TrainingState, train_blocking,
};
