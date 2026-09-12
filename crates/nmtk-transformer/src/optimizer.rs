//! What to do with a gradient once you have one.
//!
//! Plain gradient descent takes the same size step in every direction. That is a bad fit for a
//! transformer, where the output projection sees a gradient hundreds of times larger than a
//! normalisation scale does, and one learning rate cannot suit both. AdamW divides each step by
//! a running estimate of how large that weight's gradients have been, so every weight moves at
//! about the same *relative* speed. Switching to [`Optimizer::Sgd`] in a running program and
//! watching the loss curve flatten out is the fastest way to see why it is the default.

use crate::model::Params;

/// Which update rule to apply.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Optimizer {
    /// Adam with decoupled weight decay. The default everywhere, for the reason above.
    #[default]
    AdamW,
    /// One learning rate, no memory. Kept so the difference can be measured rather than argued.
    Sgd,
}

/// How much of the old average of the gradient to keep.
const BETA1: f32 = 0.9;
/// How much of the old average of the squared gradient to keep. Larger, because a variance
/// estimate needs a longer window than a mean does to be steady.
const BETA2: f32 = 0.999;
/// Stops the division by the square root from blowing up on a weight nothing has pushed yet.
const EPS: f32 = 1e-8;

/// The running averages AdamW carries between steps: two extra numbers per weight.
#[derive(Clone, Debug)]
pub struct OptimizerState {
    kind: Optimizer,
    /// Average of the gradient, one slot per tensor in the parameter traversal.
    moment1: Vec<Vec<f32>>,
    /// Average of the squared gradient.
    moment2: Vec<Vec<f32>>,
    steps: u64,
}

impl OptimizerState {
    /// Allocates the state to match a set of parameters.
    pub fn new(kind: Optimizer, params: &Params) -> Self {
        let sizes: Vec<usize> = params.tensors().iter().map(|(t, _)| t.len()).collect();
        let empty = |sizes: &[usize]| -> Vec<Vec<f32>> {
            if kind == Optimizer::AdamW {
                sizes.iter().map(|&n| vec![0.0; n]).collect()
            } else {
                Vec::new()
            }
        };
        Self { kind, moment1: empty(&sizes), moment2: empty(&sizes), steps: 0 }
    }

    /// How many updates have been applied.
    pub fn steps(&self) -> u64 {
        self.steps
    }

    /// Which rule this state implements.
    pub fn kind(&self) -> Optimizer {
        self.kind
    }

    /// Moves every weight one step against its gradient.
    ///
    /// The decay is *decoupled*: it is subtracted from the weight directly rather than added to
    /// the gradient, so Adam's per-weight scaling does not undo it. That is the W in AdamW, and
    /// it is the whole difference from the older Adam-with-L2.
    pub fn step(&mut self, params: &mut Params, grads: &Params, lr: f32, weight_decay: f32) {
        self.steps += 1;
        match self.kind {
            Optimizer::Sgd => {
                for ((values, decay), (g, _)) in
                    params.tensors_mut().into_iter().zip(grads.tensors())
                {
                    let wd = if decay { weight_decay } else { 0.0 };
                    for (w, &grad) in values.iter_mut().zip(g) {
                        *w -= lr * (grad + wd * *w);
                    }
                }
            }
            Optimizer::AdamW => {
                // Both averages start at zero, which biases them towards zero for the first few
                // steps. Dividing by `1 − β^t` removes exactly that bias; without it the first
                // steps are far too small and training crawls out of the gate.
                let correction1 = 1.0 - BETA1.powi(self.steps.min(i32::MAX as u64) as i32);
                let correction2 = 1.0 - BETA2.powi(self.steps.min(i32::MAX as u64) as i32);
                let tensors = params.tensors_mut();
                let gradients = grads.tensors();
                for (index, ((values, decay), (g, _))) in
                    tensors.into_iter().zip(gradients).enumerate()
                {
                    let (Some(m1), Some(m2)) =
                        (self.moment1.get_mut(index), self.moment2.get_mut(index))
                    else {
                        continue;
                    };
                    let wd = if decay { weight_decay } else { 0.0 };
                    for (((w, &grad), m), v) in
                        values.iter_mut().zip(g).zip(m1.iter_mut()).zip(m2.iter_mut())
                    {
                        *m = BETA1 * *m + (1.0 - BETA1) * grad;
                        *v = BETA2 * *v + (1.0 - BETA2) * grad * grad;
                        let mhat = *m / correction1;
                        let vhat = *v / correction2;
                        *w -= lr * (mhat / (vhat.sqrt() + EPS) + wd * *w);
                    }
                }
            }
        }
    }
}

/// Shrinks every gradient by the same factor if their combined length is over `max_norm`.
/// Returns the length before any shrinking, which is worth putting on screen: a spike in it is
/// the first sign that a learning rate is too high.
///
/// Scaling all of them together rather than clipping each one separately is what keeps the
/// direction of the step intact — only its length changes.
pub fn clip_global_norm(grads: &mut Params, max_norm: f32) -> f32 {
    let total: f32 =
        grads.tensors().iter().map(|(t, _)| t.iter().map(|g| g * g).sum::<f32>()).sum();
    let norm = total.sqrt();
    if norm > max_norm && norm.is_finite() && norm > 0.0 {
        let factor = max_norm / norm;
        for (tensor, _) in grads.tensors_mut() {
            for g in tensor.iter_mut() {
                *g *= factor;
            }
        }
    }
    norm
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ModelShape, Params};

    fn shape() -> ModelShape {
        ModelShape::new(5, 4, 2, 1, 4).expect("valid shape")
    }

    /// A constant gradient should move a weight by roughly the learning rate each step,
    /// whatever the gradient's size — that per-weight normalisation is the point of Adam.
    #[test]
    fn adamw_takes_steps_of_about_the_learning_rate() {
        for gradient in [1e-4f32, 1.0, 500.0] {
            let mut params = Params::zeros(&shape());
            let mut grads = Params::zeros(&shape());
            for (tensor, _) in grads.tensors_mut() {
                tensor.fill(gradient);
            }
            let mut opt = OptimizerState::new(Optimizer::AdamW, &params);
            opt.step(&mut params, &grads, 0.1, 0.0);
            let moved = -params.token_emb.data[0];
            assert!((moved - 0.1).abs() < 1e-3, "gradient {gradient} moved the weight by {moved}");
        }
    }

    /// Plain descent does the opposite: the step is proportional to the gradient.
    #[test]
    fn sgd_steps_in_proportion_to_the_gradient() {
        let mut params = Params::zeros(&shape());
        let mut grads = Params::zeros(&shape());
        for (tensor, _) in grads.tensors_mut() {
            tensor.fill(2.0);
        }
        let mut opt = OptimizerState::new(Optimizer::Sgd, &params);
        opt.step(&mut params, &grads, 0.25, 0.0);
        assert!((params.token_emb.data[0] + 0.5).abs() < 1e-6);
    }

    /// Decay must reach weights that multiply and leave biases and normalisation scales alone.
    #[test]
    fn weight_decay_skips_biases_and_normalisation_scales() {
        let mut params = Params::zeros(&shape());
        for (tensor, _) in params.tensors_mut() {
            tensor.fill(1.0);
        }
        let grads = Params::zeros(&shape());
        let mut opt = OptimizerState::new(Optimizer::Sgd, &params);
        opt.step(&mut params, &grads, 1.0, 0.1);
        assert!((params.token_emb.data[0] - 0.9).abs() < 1e-6, "a weight should decay");
        assert!((params.head.b[0] - 1.0).abs() < 1e-6, "a bias should not");
        assert!((params.ln_f.gamma[0] - 1.0).abs() < 1e-6, "a scale should not");
    }

    #[test]
    fn clipping_shortens_the_step_without_turning_it() {
        let mut grads = Params::zeros(&shape());
        for (tensor, _) in grads.tensors_mut() {
            tensor.fill(3.0);
        }
        let before = clip_global_norm(&mut grads, 1.0);
        assert!(before > 1.0);
        let after: f32 =
            grads.tensors().iter().map(|(t, _)| t.iter().map(|g| g * g).sum::<f32>()).sum();
        assert!((after.sqrt() - 1.0).abs() < 1e-4, "norm is now {}", after.sqrt());
        // Every entry started equal and must still be equal: direction unchanged.
        let first = grads.token_emb.data[0];
        assert!(grads.token_emb.data.iter().all(|g| (g - first).abs() < 1e-9));
    }

    #[test]
    fn a_short_gradient_is_left_alone() {
        let mut grads = Params::zeros(&shape());
        grads.token_emb.data[0] = 0.5;
        let norm = clip_global_norm(&mut grads, 1.0);
        assert!((norm - 0.5).abs() < 1e-6);
        assert!((grads.token_emb.data[0] - 0.5).abs() < 1e-9);
    }
}
