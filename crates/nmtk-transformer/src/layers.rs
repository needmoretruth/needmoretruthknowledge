//! The three small functions that are not matrix multiplication, forward and backward.
//!
//! Layer normalisation, softmax and GELU. Each one is a handful of lines, and each one has a
//! backward pass that is easy to get subtly wrong — a missing mean term in layer normalisation
//! still trains, just worse, and nothing on screen would say so. The finite-difference tests at
//! the bottom of this file are the reason to trust them.

/// Added under the square root so a constant row does not divide by zero.
pub const LAYER_NORM_EPS: f32 = 1e-5;

/// √(2/π), from the tanh form of GELU.
const GELU_C: f32 = 0.797_884_6;
/// The cubic term's coefficient in the same approximation.
const GELU_A: f32 = 0.044_715;

/// Normalises one row to zero mean and unit variance, then scales and shifts it.
///
/// Returns the mean and `1/√(var+ε)` for the backward pass, which needs both and would otherwise
/// have to recompute them from the input.
pub fn layer_norm_row(x: &[f32], gamma: &[f32], beta: &[f32], out: &mut [f32]) -> (f32, f32) {
    let n = x.len() as f32;
    let mean = x.iter().sum::<f32>() / n;
    let var = x
        .iter()
        .map(|v| {
            let d = v - mean;
            d * d
        })
        .sum::<f32>()
        / n;
    let inv_std = 1.0 / (var + LAYER_NORM_EPS).sqrt();
    for ((slot, &v), (&g, &b)) in out.iter_mut().zip(x).zip(gamma.iter().zip(beta)) {
        *slot = (v - mean) * inv_std * g + b;
    }
    (mean, inv_std)
}

/// Backward pass of [`layer_norm_row`], adding into `dx`, `dgamma` and `dbeta`.
///
/// Adding rather than overwriting is what lets a residual connection route two gradients into
/// the same place: the shortcut path writes `dx` first and this adds the normalised path on top.
///
/// The two sums are the part that is easy to forget. Because every output of the row depends on
/// every input through the mean and the variance, the gradient of one input carries a correction
/// for the whole row — that is `sum_dxhat` and `sum_dxhat_xhat` below.
///
/// Eight arguments, because the layer has three inputs, three gradients and two cached
/// statistics, and bundling them into a struct would hide which is which.
#[allow(clippy::too_many_arguments)]
pub fn layer_norm_row_backward(
    dy: &[f32],
    x: &[f32],
    gamma: &[f32],
    mean: f32,
    inv_std: f32,
    dx: &mut [f32],
    dgamma: &mut [f32],
    dbeta: &mut [f32],
) {
    let n = x.len() as f32;
    let mut sum_dxhat = 0.0f32;
    let mut sum_dxhat_xhat = 0.0f32;
    for (((&g_out, &v), &g), (dg, db)) in
        dy.iter().zip(x).zip(gamma).zip(dgamma.iter_mut().zip(dbeta.iter_mut()))
    {
        let xhat = (v - mean) * inv_std;
        let dxhat = g_out * g;
        *dg += g_out * xhat;
        *db += g_out;
        sum_dxhat += dxhat;
        sum_dxhat_xhat += dxhat * xhat;
    }
    for ((slot, &v), (&g_out, &g)) in dx.iter_mut().zip(x).zip(dy.iter().zip(gamma)) {
        let xhat = (v - mean) * inv_std;
        let dxhat = g_out * g;
        *slot += inv_std * (dxhat - sum_dxhat / n - xhat * sum_dxhat_xhat / n);
    }
}

/// Turns a row of scores into probabilities that sum to one.
///
/// The largest score is subtracted first. Without that, `exp` of a score around 90 overflows to
/// infinity in `f32` and the whole row becomes `NaN`; with it, the answer is unchanged because
/// softmax ignores a constant added to every score.
pub fn softmax_in_place(row: &mut [f32]) {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    if max == f32::INFINITY {
        // A score that has overflowed to infinity outweighs every finite one, so all of the
        // weight goes to the infinite scores, shared equally. Returning the row untouched, as
        // this used to, handed the caller raw scores labelled as probabilities: an attention
        // row of infinities that summed to far more than one.
        let top = row.iter().filter(|v| **v == f32::INFINITY).count() as f32;
        for v in row.iter_mut() {
            *v = if *v == f32::INFINITY { 1.0 / top } else { 0.0 };
        }
        return;
    }
    if !max.is_finite() {
        // Every score is minus infinity or not a number. There is nothing to rank, and a row
        // of garbage is left as it is so the garbage shows further on rather than hiding.
        return;
    }
    let mut sum = 0.0f32;
    for v in row.iter_mut() {
        *v = (*v - max).exp();
        sum += *v;
    }
    if sum > 0.0 {
        for v in row.iter_mut() {
            *v /= sum;
        }
    }
}

/// Backward pass of a softmax: `ds = p ⊙ (dp − Σ p·dp)`.
///
/// Reads the probabilities, not the scores — the Jacobian of softmax is written entirely in
/// terms of its own output, which is why the forward pass stores `p` and throws the scores away.
pub fn softmax_backward(p: &[f32], dp: &[f32], ds: &mut [f32]) {
    let weighted: f32 = p.iter().zip(dp).map(|(a, b)| a * b).sum();
    for ((slot, &pi), &dpi) in ds.iter_mut().zip(p).zip(dp) {
        *slot = pi * (dpi - weighted);
    }
}

/// GELU in its tanh form, the activation inside the feed-forward network.
///
/// Unlike ReLU it has a gradient everywhere, including for negative inputs, so no unit can go
/// permanently silent early in training.
pub fn gelu(x: f32) -> f32 {
    // Far out on either side the tanh is exactly ±1 in `f32` and GELU is exactly `x` or zero.
    // Saying so directly matters for huge inputs, which a runaway learning rate produces: the
    // cube overflows to infinity there, and at `x = −∞` the formula is `−∞ · 0`, which is not a
    // number.
    if x > GELU_SATURATES {
        return x;
    }
    if x < -GELU_SATURATES {
        return 0.0;
    }
    0.5 * x * (1.0 + (GELU_C * (x + GELU_A * x * x * x)).tanh())
}

/// Past this distance from zero the tanh in GELU has rounded to exactly ±1 in `f32`
/// (`tanh(0.8 · (10 + 44.7))` is one to far more digits than `f32` keeps), so the saturated
/// answers above are the formula's own answers, not an approximation of them.
const GELU_SATURATES: f32 = 10.0;

/// The derivative of [`gelu`], differentiated by hand through the tanh and the cubic.
///
/// Saturated the same way as [`gelu`]: out there `1 − t²` is exactly zero while `x²` overflows
/// to infinity, and zero times infinity is not a number — a single huge activation used to turn
/// the whole backward pass into `NaN`.
pub fn gelu_grad(x: f32) -> f32 {
    if x > GELU_SATURATES {
        return 1.0;
    }
    if x < -GELU_SATURATES {
        return 0.0;
    }
    let inner = GELU_C * (x + GELU_A * x * x * x);
    let t = inner.tanh();
    let d_inner = GELU_C * (1.0 + 3.0 * GELU_A * x * x);
    0.5 * (1.0 + t) + 0.5 * x * (1.0 - t * t) * d_inner
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(values: &[f32]) -> Vec<f32> {
        values.to_vec()
    }

    #[test]
    fn softmax_sums_to_one_and_keeps_the_order() {
        let mut p = row(&[-3.0, 0.5, 2.0, 1.25]);
        softmax_in_place(&mut p);
        assert!((p.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert!(p.iter().all(|&v| v > 0.0));
        assert!(p[2] > p[3] && p[3] > p[1] && p[1] > p[0]);
    }

    /// Adding the same number to every score must not change the answer. This is the identity
    /// the max-subtraction relies on, and it is also what makes the scale of attention scores
    /// irrelevant to the softmax itself.
    #[test]
    fn softmax_ignores_a_constant_offset() {
        let base = [-1.0, 0.0, 3.5, 2.0];
        let mut a = row(&base);
        let mut b: Vec<f32> = base.iter().map(|v| v + 17.0).collect();
        softmax_in_place(&mut a);
        softmax_in_place(&mut b);
        for (x, y) in a.iter().zip(&b) {
            assert!((x - y).abs() < 1e-6);
        }
    }

    /// Scores large enough to overflow `exp` in `f32`.
    #[test]
    fn softmax_survives_scores_that_would_overflow() {
        let mut p = row(&[120.0, 118.0, -200.0]);
        softmax_in_place(&mut p);
        assert!(p.iter().all(|v| v.is_finite()));
        assert!((p.iter().sum::<f32>() - 1.0).abs() < 1e-6);
        assert_eq!(p[2], 0.0);
    }

    /// With gamma at one and beta at zero, the output of layer normalisation has mean zero and
    /// variance one. That is the whole point of the layer, so it is worth asserting directly.
    #[test]
    fn layer_norm_leaves_mean_zero_and_variance_one() {
        let x = row(&[3.0, -1.0, 7.5, 0.25, -4.0, 2.0]);
        let gamma = vec![1.0; x.len()];
        let beta = vec![0.0; x.len()];
        let mut out = vec![0.0; x.len()];
        layer_norm_row(&x, &gamma, &beta, &mut out);
        let n = out.len() as f32;
        let mean = out.iter().sum::<f32>() / n;
        let var = out.iter().map(|v| (v - mean) * (v - mean)).sum::<f32>() / n;
        assert!(mean.abs() < 1e-5, "mean was {mean}");
        assert!((var - 1.0).abs() < 1e-3, "variance was {var}");
    }

    /// Shifting and scaling the input must not change the normalised output — layer
    /// normalisation is exactly the map that throws those two degrees of freedom away.
    #[test]
    fn layer_norm_ignores_a_shift_and_a_scale_of_its_input() {
        let x = row(&[1.0, 2.0, 3.0, 4.0]);
        let shifted: Vec<f32> = x.iter().map(|v| v * 5.0 + 9.0).collect();
        let gamma = vec![1.0; 4];
        let beta = vec![0.0; 4];
        let (mut a, mut b) = (vec![0.0; 4], vec![0.0; 4]);
        layer_norm_row(&x, &gamma, &beta, &mut a);
        layer_norm_row(&shifted, &gamma, &beta, &mut b);
        for (p, q) in a.iter().zip(&b) {
            assert!((p - q).abs() < 1e-3, "{p} vs {q}");
        }
    }

    #[test]
    fn a_constant_row_does_not_divide_by_zero() {
        let x = vec![2.0; 5];
        let mut out = vec![0.0; 5];
        let (_, inv_std) = layer_norm_row(&x, &[1.0; 5], &[0.0; 5], &mut out);
        assert!(inv_std.is_finite());
        assert!(out.iter().all(|v| v.is_finite()));
    }

    /// Central differences against the hand-written backward pass. `h` is large enough that the
    /// `f32` noise floor stays below the truncation error and small enough that the truncation
    /// error stays small.
    fn finite_difference(mut f: impl FnMut(f32) -> f32, x: f32, h: f32) -> f32 {
        (f(x + h) - f(x - h)) / (2.0 * h)
    }

    /// A runaway learning rate drives activations far past anything a sensible run sees. The
    /// cube inside GELU overflowed there and `gelu_grad(1e20)` came out as `NaN`, which then
    /// spread through every gradient of the step.
    #[test]
    fn gelu_and_its_gradient_stay_numbers_for_huge_inputs() {
        for x in [1e13f32, 1e20, 3e38, f32::MAX, -1e13, -1e20, -3e38, f32::MIN] {
            assert!(gelu(x).is_finite(), "gelu({x}) = {}", gelu(x));
            assert!(gelu_grad(x).is_finite(), "gelu_grad({x}) = {}", gelu_grad(x));
        }
        assert_eq!(gelu(1e20), 1e20);
        assert_eq!(gelu(-1e20), 0.0);
        assert_eq!(gelu_grad(1e20), 1.0);
        assert_eq!(gelu_grad(-1e20), 0.0);
        // The shortcut starts where the formula already gives the same answer, so there is no
        // step in the curve at the seam.
        for x in [GELU_SATURATES, -GELU_SATURATES] {
            let formula = 0.5 * x * (1.0 + (GELU_C * (x + GELU_A * x * x * x)).tanh());
            assert!((gelu(x) - formula).abs() < 1e-6, "gelu jumps at {x}");
            let above = gelu(x + x.signum() * 1e-3);
            assert!((above - gelu(x)).abs() < 2e-3, "gelu jumps just past {x}");
        }
    }

    /// Scores that overflowed to infinity used to come back untouched: raw infinities where the
    /// caller expected a distribution.
    #[test]
    fn softmax_of_an_infinite_score_is_still_a_distribution() {
        let mut p = row(&[f32::INFINITY, 1.0, f32::INFINITY, -3.0]);
        softmax_in_place(&mut p);
        assert_eq!(p, vec![0.5, 0.0, 0.5, 0.0]);
    }

    #[test]
    fn gelu_gradient_matches_finite_differences() {
        for step in -40..=40 {
            let x = step as f32 * 0.1;
            let numeric = finite_difference(gelu, x, 1e-2);
            let analytic = gelu_grad(x);
            assert!(
                (numeric - analytic).abs() < 2e-3 + 2e-2 * analytic.abs(),
                "at x={x}: analytic {analytic}, numeric {numeric}"
            );
        }
    }

    #[test]
    fn layer_norm_gradient_matches_finite_differences() {
        let n = 6;
        let x: Vec<f32> = (0..n).map(|i| (i as f32 * 0.9).sin() * 2.0).collect();
        let gamma: Vec<f32> = (0..n).map(|i| 1.0 + (i as f32 * 0.3).cos() * 0.4).collect();
        let beta: Vec<f32> = (0..n).map(|i| (i as f32 * 0.5).sin() * 0.2).collect();
        // An arbitrary but fixed upstream gradient, so the check is not a special case.
        let dy: Vec<f32> = (0..n).map(|i| (i as f32 * 1.3).cos()).collect();

        // The scalar the gradients belong to: a weighted sum of the layer's outputs.
        let loss = |x: &[f32], gamma: &[f32], beta: &[f32]| -> f32 {
            let mut out = vec![0.0; x.len()];
            layer_norm_row(x, gamma, beta, &mut out);
            out.iter().zip(&dy).map(|(a, b)| a * b).sum()
        };

        let mut out = vec![0.0; n];
        let (mean, inv_std) = layer_norm_row(&x, &gamma, &beta, &mut out);
        let (mut dx, mut dgamma, mut dbeta) = (vec![0.0; n], vec![0.0; n], vec![0.0; n]);
        layer_norm_row_backward(&dy, &x, &gamma, mean, inv_std, &mut dx, &mut dgamma, &mut dbeta);

        let h = 1e-2;
        for i in 0..n {
            let mut probe = x.clone();
            let numeric = finite_difference(
                |v| {
                    probe[i] = v;
                    loss(&probe, &gamma, &beta)
                },
                x[i],
                h,
            );
            assert!(
                (numeric - dx[i]).abs() < 2e-3 + 2e-2 * dx[i].abs(),
                "dx[{i}]: analytic {}, numeric {numeric}",
                dx[i]
            );

            let mut probe_g = gamma.clone();
            let numeric = finite_difference(
                |v| {
                    probe_g[i] = v;
                    loss(&x, &probe_g, &beta)
                },
                gamma[i],
                h,
            );
            assert!(
                (numeric - dgamma[i]).abs() < 2e-3 + 2e-2 * dgamma[i].abs(),
                "dgamma[{i}]: analytic {}, numeric {numeric}",
                dgamma[i]
            );

            assert!((dbeta[i] - dy[i]).abs() < 1e-6, "dbeta[{i}] is just dy");
        }
    }

    #[test]
    fn softmax_gradient_matches_finite_differences() {
        let scores = [0.4f32, -1.2, 2.0, 0.0, 0.7];
        let dp = [0.3f32, -0.9, 1.1, 0.2, -0.4];
        let loss = |s: &[f32]| -> f32 {
            let mut p = s.to_vec();
            softmax_in_place(&mut p);
            p.iter().zip(&dp).map(|(a, b)| a * b).sum()
        };

        let mut p = scores.to_vec();
        softmax_in_place(&mut p);
        let mut ds = vec![0.0; p.len()];
        softmax_backward(&p, &dp, &mut ds);

        for i in 0..scores.len() {
            let mut probe = scores.to_vec();
            let numeric = finite_difference(
                |v| {
                    probe[i] = v;
                    loss(&probe)
                },
                scores[i],
                1e-2,
            );
            assert!(
                (numeric - ds[i]).abs() < 2e-3 + 2e-2 * ds[i].abs(),
                "ds[{i}]: analytic {}, numeric {numeric}",
                ds[i]
            );
        }
    }
}
