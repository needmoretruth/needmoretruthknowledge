//! The model itself: every weight, the forward pass, and the backward pass written out by hand.
//!
//! The layout is the one GPT-2 uses, at a size that fits in a laptop's cache. A sequence of
//! character ids becomes a stack of vectors, each block lets every position mix in information
//! from the positions before it and then thinks about the result on its own, and a final linear
//! layer turns each vector into a score for every character that could come next.
//!
//! Normalisation comes *before* each sub-layer, not after. That leaves an unbroken addition path
//! from the embeddings to the output — `x = x + something` at every step — so the gradient
//! reaches the first block without being squeezed through a normalisation on the way back. It is
//! the difference between a four-block model that trains and one that stalls.
//!
//! Nothing here allocates a graph or records an operation. The backward pass is the chain rule
//! applied to the forward pass in reverse order, by hand, in the same file, so the two can be
//! read side by side.

use crate::layers::{
    gelu, gelu_grad, layer_norm_row, layer_norm_row_backward, softmax_backward, softmax_in_place,
};
use crate::matrix::{
    Matrix, axpy, dot, linear_backward_input, linear_backward_weight, linear_forward,
};
use rand::rngs::Xoshiro256PlusPlus;
use rand::{RngExt, SeedableRng};

/// The sizes that decide every weight in the model.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ModelShape {
    /// How many distinct characters the model chooses between.
    pub vocab: usize,
    /// Width of the vector carried at each position.
    pub d_model: usize,
    /// How many attention heads each block runs in parallel.
    pub heads: usize,
    /// `d_model / heads`: the width one head works in.
    pub head_dim: usize,
    /// How many transformer blocks are stacked.
    pub layers: usize,
    /// The longest sequence the model has positions for.
    pub context: usize,
    /// Width of the hidden layer inside each feed-forward network.
    pub ffn_hidden: usize,
}

impl ModelShape {
    /// Builds a shape, or `None` when the heads do not divide the width evenly or any size is
    /// zero. Every other constructor in this crate goes through here.
    pub fn new(
        vocab: usize,
        d_model: usize,
        heads: usize,
        layers: usize,
        context: usize,
    ) -> Option<Self> {
        if vocab == 0 || d_model == 0 || heads == 0 || layers == 0 || context == 0 {
            return None;
        }
        if !d_model.is_multiple_of(heads) {
            return None;
        }
        Some(Self {
            vocab,
            d_model,
            heads,
            head_dim: d_model / heads,
            layers,
            context,
            // Four times the width is the ratio the original paper used and every model since
            // has kept. It is where most of the parameters live.
            ffn_hidden: 4 * d_model,
        })
    }

    /// How many numbers the model is made of.
    pub fn parameter_count(&self) -> usize {
        let (d, f, v) = (self.d_model, self.ffn_hidden, self.vocab);
        let norm = 2 * d;
        let linear = |out: usize, inp: usize| out * inp + out;
        let block = norm + 4 * linear(d, d) + norm + linear(f, d) + linear(d, f);
        v * d + self.context * d + self.layers * block + norm + linear(v, d)
    }
}

/// A weight matrix stored as `(out_features, in_features)`, with its bias.
#[derive(Clone, Debug, PartialEq)]
pub struct LinearParams {
    /// `(out, in)`.
    pub w: Matrix,
    /// One value per output.
    pub b: Vec<f32>,
}

impl LinearParams {
    fn zeros(out: usize, inp: usize) -> Self {
        Self { w: Matrix::zeros(out, inp), b: vec![0.0; out] }
    }
}

/// The scale and shift a layer normalisation learns.
#[derive(Clone, Debug, PartialEq)]
pub struct NormParams {
    /// Per-channel scale, starting at one.
    pub gamma: Vec<f32>,
    /// Per-channel shift, starting at zero.
    pub beta: Vec<f32>,
}

impl NormParams {
    fn zeros(d: usize) -> Self {
        Self { gamma: vec![0.0; d], beta: vec![0.0; d] }
    }
}

/// One transformer block: attention, then a feed-forward network, each behind a normalisation.
#[derive(Clone, Debug, PartialEq)]
pub struct BlockParams {
    /// Normalisation in front of attention.
    pub ln1: NormParams,
    /// Query projection.
    pub wq: LinearParams,
    /// Key projection.
    pub wk: LinearParams,
    /// Value projection.
    pub wv: LinearParams,
    /// Projection that mixes the heads back together.
    pub wo: LinearParams,
    /// Normalisation in front of the feed-forward network.
    pub ln2: NormParams,
    /// Feed-forward, widening.
    pub w1: LinearParams,
    /// Feed-forward, narrowing.
    pub w2: LinearParams,
}

impl BlockParams {
    fn zeros(shape: &ModelShape) -> Self {
        let (d, f) = (shape.d_model, shape.ffn_hidden);
        Self {
            ln1: NormParams::zeros(d),
            wq: LinearParams::zeros(d, d),
            wk: LinearParams::zeros(d, d),
            wv: LinearParams::zeros(d, d),
            wo: LinearParams::zeros(d, d),
            ln2: NormParams::zeros(d),
            w1: LinearParams::zeros(f, d),
            w2: LinearParams::zeros(d, f),
        }
    }
}

/// Every weight in the model.
///
/// The same type holds the gradients. They have identical shapes by definition, and sharing the
/// type means the optimiser can walk both with one traversal and they cannot drift apart.
#[derive(Clone, Debug, PartialEq)]
pub struct Params {
    /// `(vocab, d_model)`: what each character means before any context is added.
    pub token_emb: Matrix,
    /// `(context, d_model)`: what each position means. Learned, not a fixed wave — with a
    /// corpus this small the model is free to work out its own notion of "how far back".
    pub pos_emb: Matrix,
    /// One entry per transformer block.
    pub blocks: Vec<BlockParams>,
    /// The normalisation before the output projection.
    pub ln_f: NormParams,
    /// `(vocab, d_model)`: turns a vector into a score per character.
    pub head: LinearParams,
}

impl Params {
    /// All zeros — the shape a gradient starts every step in.
    pub fn zeros(shape: &ModelShape) -> Self {
        Self {
            token_emb: Matrix::zeros(shape.vocab, shape.d_model),
            pos_emb: Matrix::zeros(shape.context, shape.d_model),
            blocks: (0..shape.layers).map(|_| BlockParams::zeros(shape)).collect(),
            ln_f: NormParams::zeros(shape.d_model),
            head: LinearParams::zeros(shape.vocab, shape.d_model),
        }
    }

    /// The starting weights for a given seed.
    ///
    /// Small random values for anything that multiplies, zero for anything that adds, one for
    /// the normalisation scales. The two projections that write into the residual stream —
    /// `wo` and `w2` — start smaller by `1/√(2·layers)`: each block adds to the same running
    /// vector, so without that the variance of the stream grows with depth and a deeper model
    /// starts off worse than a shallow one.
    pub fn initial(shape: &ModelShape, seed: u64) -> Self {
        let mut rng = Xoshiro256PlusPlus::seed_from_u64(seed);
        let base = 0.02f32;
        let residual = base / ((2 * shape.layers) as f32).sqrt();
        let mut p = Self::zeros(shape);
        fill_normal(&mut p.token_emb.data, base, &mut rng);
        fill_normal(&mut p.pos_emb.data, base, &mut rng);
        for block in &mut p.blocks {
            block.ln1.gamma.fill(1.0);
            block.ln2.gamma.fill(1.0);
            fill_normal(&mut block.wq.w.data, base, &mut rng);
            fill_normal(&mut block.wk.w.data, base, &mut rng);
            fill_normal(&mut block.wv.w.data, base, &mut rng);
            fill_normal(&mut block.wo.w.data, residual, &mut rng);
            fill_normal(&mut block.w1.w.data, base, &mut rng);
            fill_normal(&mut block.w2.w.data, residual, &mut rng);
        }
        p.ln_f.gamma.fill(1.0);
        fill_normal(&mut p.head.w.data, base, &mut rng);
        p
    }

    /// `self += other`, tensor by tensor. Both must have been built from the same shape.
    ///
    /// This is how the gradients of a batch come back together: every sequence fills its own
    /// copy in parallel and they are added up here, always in the same order.
    pub fn add_assign(&mut self, other: &Params) {
        for ((into, _), (from, _)) in self.tensors_mut().into_iter().zip(other.tensors()) {
            for (slot, value) in into.iter_mut().zip(from) {
                *slot += value;
            }
        }
    }

    /// Back to all zeros, keeping the allocations.
    pub fn zero(&mut self) {
        for (tensor, _) in self.tensors_mut() {
            tensor.fill(0.0);
        }
    }

    /// How many numbers there are in total.
    pub fn count(&self) -> usize {
        self.tensors().iter().map(|(t, _)| t.len()).sum()
    }

    /// Every tensor in a fixed order, with whether weight decay should reach it.
    ///
    /// Decay pulls weights towards zero. That makes sense for something that multiplies — it
    /// keeps the model from leaning on any one connection — and no sense at all for a bias or a
    /// normalisation scale, where zero is not a neutral value but a switched-off unit.
    pub fn tensors(&self) -> Vec<(&[f32], bool)> {
        let mut out = Vec::new();
        out.push((self.token_emb.data.as_slice(), true));
        out.push((self.pos_emb.data.as_slice(), true));
        for b in &self.blocks {
            out.push((b.ln1.gamma.as_slice(), false));
            out.push((b.ln1.beta.as_slice(), false));
            for linear in [&b.wq, &b.wk, &b.wv, &b.wo] {
                out.push((linear.w.data.as_slice(), true));
                out.push((linear.b.as_slice(), false));
            }
            out.push((b.ln2.gamma.as_slice(), false));
            out.push((b.ln2.beta.as_slice(), false));
            for linear in [&b.w1, &b.w2] {
                out.push((linear.w.data.as_slice(), true));
                out.push((linear.b.as_slice(), false));
            }
        }
        out.push((self.ln_f.gamma.as_slice(), false));
        out.push((self.ln_f.beta.as_slice(), false));
        out.push((self.head.w.data.as_slice(), true));
        out.push((self.head.b.as_slice(), false));
        out
    }

    /// The same traversal as [`Params::tensors`], for writing.
    pub fn tensors_mut(&mut self) -> Vec<(&mut [f32], bool)> {
        let mut out = Vec::new();
        out.push((self.token_emb.data.as_mut_slice(), true));
        out.push((self.pos_emb.data.as_mut_slice(), true));
        for b in &mut self.blocks {
            out.push((b.ln1.gamma.as_mut_slice(), false));
            out.push((b.ln1.beta.as_mut_slice(), false));
            for linear in [&mut b.wq, &mut b.wk, &mut b.wv, &mut b.wo] {
                out.push((linear.w.data.as_mut_slice(), true));
                out.push((linear.b.as_mut_slice(), false));
            }
            out.push((b.ln2.gamma.as_mut_slice(), false));
            out.push((b.ln2.beta.as_mut_slice(), false));
            for linear in [&mut b.w1, &mut b.w2] {
                out.push((linear.w.data.as_mut_slice(), true));
                out.push((linear.b.as_mut_slice(), false));
            }
        }
        out.push((self.ln_f.gamma.as_mut_slice(), false));
        out.push((self.ln_f.beta.as_mut_slice(), false));
        out.push((self.head.w.data.as_mut_slice(), true));
        out.push((self.head.b.as_mut_slice(), false));
        out
    }
}

/// Fills a slice with samples from a normal distribution of the given standard deviation.
///
/// Box–Muller: two uniform numbers in, one normal number out. The second normal value it could
/// produce is thrown away so that the count of random numbers drawn depends only on how many
/// weights there are, which keeps a seed reproducible no matter how the model is shaped.
fn fill_normal(values: &mut [f32], std_dev: f32, rng: &mut Xoshiro256PlusPlus) {
    for slot in values.iter_mut() {
        // `1.0 - u` moves the range from [0,1) to (0,1] so the logarithm is finite.
        let u1: f32 = 1.0 - rng.random::<f32>();
        let u2: f32 = rng.random::<f32>();
        *slot = (-2.0 * u1.ln()).sqrt() * (std::f32::consts::TAU * u2).cos() * std_dev;
    }
}

/// Everything one block computed on the way forward, kept because the backward pass needs it.
#[derive(Clone, Debug)]
pub struct BlockCache {
    /// What came into the block.
    pub input: Matrix,
    /// Mean and `1/√(var+ε)` per position, from the first normalisation.
    pub ln1: Vec<(f32, f32)>,
    /// The normalised input to attention.
    pub h1: Matrix,
    /// Queries, keys and values, all heads side by side in one `(t, d_model)` matrix.
    pub q: Matrix,
    /// See [`BlockCache::q`].
    pub k: Matrix,
    /// See [`BlockCache::q`].
    pub v: Matrix,
    /// `heads · t · t` attention weights. Entry `(h, i, j)` is how much position `i` drew from
    /// position `j`. Everything above the diagonal is exactly zero: that is the causal mask.
    pub probs: Vec<f32>,
    /// The per-head weighted sums of values, concatenated.
    pub ctx: Matrix,
    /// Input plus the attention output: the residual stream after the first sub-layer.
    pub x1: Matrix,
    /// Mean and inverse standard deviation from the second normalisation.
    pub ln2: Vec<(f32, f32)>,
    /// The normalised input to the feed-forward network.
    pub h2: Matrix,
    /// Feed-forward hidden layer before GELU.
    pub u: Matrix,
    /// Feed-forward hidden layer after GELU.
    pub act: Matrix,
    /// The block's output.
    pub x2: Matrix,
}

/// Everything one forward pass computed.
#[derive(Clone, Debug)]
pub struct Activations {
    /// How many positions this pass covered.
    pub len: usize,
    /// Token embedding plus position embedding.
    pub x0: Matrix,
    /// One entry per block, in order.
    pub blocks: Vec<BlockCache>,
    /// Mean and inverse standard deviation from the final normalisation.
    pub lnf: Vec<(f32, f32)>,
    /// The normalised vectors the output projection reads.
    pub hf: Matrix,
    /// `(t, vocab)` scores. Position `i` scores what could come after token `i`.
    pub logits: Matrix,
}

impl Activations {
    /// The residual stream as the final normalisation saw it.
    fn final_stream(&self) -> &Matrix {
        self.blocks.last().map_or(&self.x0, |c| &c.x2)
    }

    /// Copies the attention weights out into something a screen can draw.
    pub fn attention_snapshot(&self, tokens: Vec<char>, heads: usize) -> AttentionSnapshot {
        let t = self.len;
        let mut weights = Vec::with_capacity(self.blocks.len() * heads * t * t);
        for block in &self.blocks {
            weights.extend_from_slice(&block.probs);
        }
        AttentionSnapshot { tokens, layers: self.blocks.len(), heads, length: t, weights }
    }
}

/// Attention weights lifted out of a forward pass, sized so a terminal can draw them.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct AttentionSnapshot {
    /// The characters these weights belong to, one per position.
    pub tokens: Vec<char>,
    /// How many blocks are represented.
    pub layers: usize,
    /// How many heads each block has.
    pub heads: usize,
    /// How many positions.
    pub length: usize,
    weights: Vec<f32>,
}

impl AttentionSnapshot {
    /// The `length · length` weights of one head, row-major, query position by query position.
    pub fn head(&self, layer: usize, head: usize) -> Option<&[f32]> {
        if layer >= self.layers || head >= self.heads {
            return None;
        }
        let per_head = self.length * self.length;
        let start = (layer * self.heads + head) * per_head;
        self.weights.get(start..start + per_head)
    }

    /// How much `query` drew from `key` in one head. Zero above the diagonal, always.
    pub fn weight(&self, layer: usize, head: usize, query: usize, key: usize) -> Option<f32> {
        if query >= self.length || key >= self.length {
            return None;
        }
        self.head(layer, head)?.get(query * self.length + key).copied()
    }

    /// True when no forward pass has been captured yet.
    pub fn is_empty(&self) -> bool {
        self.weights.is_empty()
    }
}

/// A transformer: a shape and the numbers that fill it.
#[derive(Clone, Debug, PartialEq)]
pub struct Model {
    /// The sizes every buffer is built from.
    pub shape: ModelShape,
    /// The weights.
    pub params: Params,
}

impl Model {
    /// A model at its starting weights for the given seed.
    pub fn new(shape: ModelShape, seed: u64) -> Self {
        Self { params: Params::initial(&shape, seed), shape }
    }

    /// How many numbers the model is made of.
    pub fn parameter_count(&self) -> usize {
        self.params.count()
    }

    /// Runs the model over a sequence of token ids.
    ///
    /// Returns `None` when the sequence is empty, longer than the context, or contains an id the
    /// vocabulary does not have. Every caller inside this crate builds its input from the
    /// tokenizer, so `None` means a programming mistake rather than bad data — but a caller
    /// outside can hand in anything, and a wrong id must not be an index out of bounds.
    pub fn forward(&self, tokens: &[usize]) -> Option<Activations> {
        let t = tokens.len();
        if t == 0 || t > self.shape.context || tokens.iter().any(|&id| id >= self.shape.vocab) {
            return None;
        }
        let d = self.shape.d_model;

        // Each position starts as what its character means plus where it is.
        let mut x0 = Matrix::zeros(t, d);
        for (p, &tok) in tokens.iter().enumerate() {
            let token_row = self.params.token_emb.row(tok);
            let pos_row = self.params.pos_emb.row(p);
            for ((slot, a), b) in x0.row_mut(p).iter_mut().zip(token_row).zip(pos_row) {
                *slot = a + b;
            }
        }

        let mut blocks: Vec<BlockCache> = Vec::with_capacity(self.shape.layers);
        for (index, bp) in self.params.blocks.iter().enumerate() {
            let input = if index == 0 { &x0 } else { &blocks[index - 1].x2 };
            let cache = self.forward_block(bp, input);
            blocks.push(cache);
        }

        let stream = blocks.last().map_or(&x0, |c| &c.x2);
        let mut hf = Matrix::zeros(t, d);
        let mut lnf = Vec::with_capacity(t);
        for p in 0..t {
            lnf.push(layer_norm_row(
                stream.row(p),
                &self.params.ln_f.gamma,
                &self.params.ln_f.beta,
                hf.row_mut(p),
            ));
        }
        let logits = linear_forward(&hf, &self.params.head.w, &self.params.head.b);
        Some(Activations { len: t, x0, blocks, lnf, hf, logits })
    }

    /// One block, forwards.
    ///
    /// The `0..=i` loops index by hand on purpose: that upper bound *is* the causal mask,
    /// and an iterator would hide it.
    #[allow(clippy::needless_range_loop)]
    fn forward_block(&self, bp: &BlockParams, input: &Matrix) -> BlockCache {
        let (t, d) = (input.rows, self.shape.d_model);
        let (heads, dh) = (self.shape.heads, self.shape.head_dim);

        let mut h1 = Matrix::zeros(t, d);
        let mut ln1 = Vec::with_capacity(t);
        for p in 0..t {
            ln1.push(layer_norm_row(input.row(p), &bp.ln1.gamma, &bp.ln1.beta, h1.row_mut(p)));
        }

        let q = linear_forward(&h1, &bp.wq.w, &bp.wq.b);
        let k = linear_forward(&h1, &bp.wk.w, &bp.wk.b);
        let v = linear_forward(&h1, &bp.wv.w, &bp.wv.b);

        // Dividing by √head_dim keeps the scores from growing with the width of a head. Without
        // it a wide head produces large scores, softmax collapses onto one position, and almost
        // no gradient comes back through the attention weights at all.
        let scale = 1.0 / (dh as f32).sqrt();
        let mut probs = vec![0.0f32; heads * t * t];
        let mut ctx = Matrix::zeros(t, d);
        for head in 0..heads {
            let lane = head * dh..head * dh + dh;
            for i in 0..t {
                let row_start = (head * t + i) * t;
                let row = &mut probs[row_start..row_start + t];
                // Scores are only computed for j <= i. Everything after stays zero, and that
                // zero is the causal mask: position i literally cannot reach position i+1.
                for j in 0..=i {
                    row[j] = dot(&q.row(i)[lane.clone()], &k.row(j)[lane.clone()]) * scale;
                }
                softmax_in_place(&mut row[..=i]);
                for j in 0..=i {
                    axpy(row[j], &v.row(j)[lane.clone()], &mut ctx.row_mut(i)[lane.clone()]);
                }
            }
        }

        let attn_out = linear_forward(&ctx, &bp.wo.w, &bp.wo.b);
        let mut x1 = input.clone();
        x1.add_assign(&attn_out);

        let mut h2 = Matrix::zeros(t, d);
        let mut ln2 = Vec::with_capacity(t);
        for p in 0..t {
            ln2.push(layer_norm_row(x1.row(p), &bp.ln2.gamma, &bp.ln2.beta, h2.row_mut(p)));
        }
        let u = linear_forward(&h2, &bp.w1.w, &bp.w1.b);
        let mut act = u.clone();
        for slot in act.data.iter_mut() {
            *slot = gelu(*slot);
        }
        let ffn_out = linear_forward(&act, &bp.w2.w, &bp.w2.b);
        let mut x2 = x1.clone();
        x2.add_assign(&ffn_out);

        BlockCache { input: input.clone(), ln1, h1, q, k, v, probs, ctx, x1, ln2, h2, u, act, x2 }
    }

    /// Mean cross-entropy of the predictions against the targets, without touching gradients.
    pub fn loss(&self, acts: &Activations, targets: &[usize]) -> Option<f64> {
        let t = acts.len;
        if targets.len() != t || targets.iter().any(|&id| id >= self.shape.vocab) {
            return None;
        }
        let mut total = 0.0f64;
        for (row, &target) in acts.logits.rows_iter().zip(targets) {
            let (_, log_sum, max) = softmax_stats(row);
            total -= (row[target] - max) as f64 - log_sum;
        }
        Some(total / t as f64)
    }

    /// The backward pass, adding into `grads`. Returns the same loss [`Model::loss`] would.
    ///
    /// Read it against `forward` from the bottom up: every line here is the derivative of the
    /// matching line there, and the order is exactly reversed.
    pub fn backward(
        &self,
        tokens: &[usize],
        targets: &[usize],
        acts: &Activations,
        grads: &mut Params,
    ) -> Option<f64> {
        let t = acts.len;
        if tokens.len() != t || targets.len() != t {
            return None;
        }
        if targets.iter().any(|&id| id >= self.shape.vocab) {
            return None;
        }
        let d = self.shape.d_model;

        // Softmax and cross-entropy, differentiated together. Separately each is awkward; the
        // pair collapses to `probability − 1 at the right answer`, which is the whole reason
        // classification models are written this way.
        let mut loss = 0.0f64;
        let inv_t = 1.0 / t as f32;
        let mut d_logits = Matrix::zeros(t, self.shape.vocab);
        for p in 0..t {
            let row = acts.logits.row(p);
            let (sum, log_sum, max) = softmax_stats(row);
            loss -= (row[targets[p]] - max) as f64 - log_sum;
            let out = d_logits.row_mut(p);
            for (c, (slot, &z)) in out.iter_mut().zip(row).enumerate() {
                let prob = (((z - max) as f64).exp() / sum) as f32;
                *slot = (prob - f32::from(c == targets[p])) * inv_t;
            }
        }
        loss /= t as f64;

        // Output projection.
        linear_backward_weight(&d_logits, &acts.hf, &mut grads.head.w, &mut grads.head.b);
        let mut d_hf = Matrix::zeros(t, d);
        linear_backward_input(&d_logits, &self.params.head.w, &mut d_hf);

        // Final normalisation.
        let stream = acts.final_stream();
        let mut dx = Matrix::zeros(t, d);
        for p in 0..t {
            let (mean, inv_std) = acts.lnf[p];
            layer_norm_row_backward(
                d_hf.row(p),
                stream.row(p),
                &self.params.ln_f.gamma,
                mean,
                inv_std,
                dx.row_mut(p),
                &mut grads.ln_f.gamma,
                &mut grads.ln_f.beta,
            );
        }

        for index in (0..self.shape.layers).rev() {
            dx = self.backward_block(
                &self.params.blocks[index],
                &acts.blocks[index],
                &mut grads.blocks[index],
                dx,
            );
        }

        // The embeddings. A character that appears four times in the window collects four
        // gradients in the same row, which is why this adds instead of assigning.
        for (p, &tok) in tokens.iter().enumerate() {
            let g = dx.row(p);
            for (slot, value) in grads.token_emb.row_mut(tok).iter_mut().zip(g) {
                *slot += value;
            }
            for (slot, value) in grads.pos_emb.row_mut(p).iter_mut().zip(g) {
                *slot += value;
            }
        }

        Some(loss)
    }

    /// One block, backwards. Takes the gradient of the block's output and returns the gradient
    /// of its input. The `0..=i` loops mirror the mask in `forward_block`.
    #[allow(clippy::needless_range_loop)]
    fn backward_block(
        &self,
        bp: &BlockParams,
        cache: &BlockCache,
        grads: &mut BlockParams,
        d_out: Matrix,
    ) -> Matrix {
        let (t, d) = (cache.input.rows, self.shape.d_model);
        let (heads, dh) = (self.shape.heads, self.shape.head_dim);

        // --- feed-forward network ---
        linear_backward_weight(&d_out, &cache.act, &mut grads.w2.w, &mut grads.w2.b);
        let mut d_act = Matrix::zeros(t, self.shape.ffn_hidden);
        linear_backward_input(&d_out, &bp.w2.w, &mut d_act);
        let mut d_u = d_act;
        for (slot, &pre) in d_u.data.iter_mut().zip(&cache.u.data) {
            *slot *= gelu_grad(pre);
        }
        linear_backward_weight(&d_u, &cache.h2, &mut grads.w1.w, &mut grads.w1.b);
        let mut d_h2 = Matrix::zeros(t, d);
        linear_backward_input(&d_u, &bp.w1.w, &mut d_h2);

        // `x2 = x1 + ffn(norm(x1))`, so the gradient of x1 is the one that arrived straight
        // down the residual path plus the one that came back through the network. Reusing
        // `d_out` as the starting value is the addition.
        let mut d_x1 = d_out;
        for p in 0..t {
            let (mean, inv_std) = cache.ln2[p];
            layer_norm_row_backward(
                d_h2.row(p),
                cache.x1.row(p),
                &bp.ln2.gamma,
                mean,
                inv_std,
                d_x1.row_mut(p),
                &mut grads.ln2.gamma,
                &mut grads.ln2.beta,
            );
        }

        // --- attention ---
        linear_backward_weight(&d_x1, &cache.ctx, &mut grads.wo.w, &mut grads.wo.b);
        let mut d_ctx = Matrix::zeros(t, d);
        linear_backward_input(&d_x1, &bp.wo.w, &mut d_ctx);

        let scale = 1.0 / (dh as f32).sqrt();
        let mut d_q = Matrix::zeros(t, d);
        let mut d_k = Matrix::zeros(t, d);
        let mut d_v = Matrix::zeros(t, d);
        let mut d_probs = vec![0.0f32; t];
        let mut d_scores = vec![0.0f32; t];
        for head in 0..heads {
            let lane = head * dh..head * dh + dh;
            for i in 0..t {
                let row_start = (head * t + i) * t;
                let p_row = &cache.probs[row_start..row_start + i + 1];
                let d_ctx_i = &d_ctx.row(i)[lane.clone()];

                // The context vector is a weighted sum of values, so each weight picks up the
                // value it multiplied, and each value picks up the weight it was given.
                for j in 0..=i {
                    d_probs[j] = dot(d_ctx_i, &cache.v.row(j)[lane.clone()]);
                    axpy(p_row[j], d_ctx_i, &mut d_v.row_mut(j)[lane.clone()]);
                }
                softmax_backward(p_row, &d_probs[..=i], &mut d_scores[..=i]);

                for j in 0..=i {
                    let g = d_scores[j] * scale;
                    axpy(g, &cache.k.row(j)[lane.clone()], &mut d_q.row_mut(i)[lane.clone()]);
                    axpy(g, &cache.q.row(i)[lane.clone()], &mut d_k.row_mut(j)[lane.clone()]);
                }
            }
        }

        linear_backward_weight(&d_q, &cache.h1, &mut grads.wq.w, &mut grads.wq.b);
        linear_backward_weight(&d_k, &cache.h1, &mut grads.wk.w, &mut grads.wk.b);
        linear_backward_weight(&d_v, &cache.h1, &mut grads.wv.w, &mut grads.wv.b);
        let mut d_h1 = Matrix::zeros(t, d);
        linear_backward_input(&d_q, &bp.wq.w, &mut d_h1);
        linear_backward_input(&d_k, &bp.wk.w, &mut d_h1);
        linear_backward_input(&d_v, &bp.wv.w, &mut d_h1);

        // The second residual connection, the same way round as the first.
        let mut d_in = d_x1;
        for p in 0..t {
            let (mean, inv_std) = cache.ln1[p];
            layer_norm_row_backward(
                d_h1.row(p),
                cache.input.row(p),
                &bp.ln1.gamma,
                mean,
                inv_std,
                d_in.row_mut(p),
                &mut grads.ln1.gamma,
                &mut grads.ln1.beta,
            );
        }
        d_in
    }
}

/// The three numbers every softmax over logits needs: the sum of `exp(z − max)`, its logarithm,
/// and the maximum that was subtracted.
fn softmax_stats(row: &[f32]) -> (f64, f64, f32) {
    let max = row.iter().copied().fold(f32::NEG_INFINITY, f32::max);
    let sum: f64 = row.iter().map(|&z| ((z - max) as f64).exp()).sum();
    (sum, sum.ln(), max)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tiny() -> ModelShape {
        ModelShape::new(7, 8, 2, 2, 6).expect("8 splits into 2 heads")
    }

    #[test]
    fn heads_must_divide_the_width() {
        assert!(ModelShape::new(10, 9, 2, 1, 4).is_none());
        assert!(ModelShape::new(10, 8, 0, 1, 4).is_none());
        assert!(ModelShape::new(0, 8, 2, 1, 4).is_none());
        assert!(ModelShape::new(10, 8, 2, 1, 4).is_some());
    }

    #[test]
    fn the_counted_parameters_are_the_ones_that_exist() {
        let shape = tiny();
        let model = Model::new(shape, 1);
        assert_eq!(model.parameter_count(), shape.parameter_count());
    }

    /// The two traversals of `Params` feed the optimiser in lockstep; if one ever grows a
    /// tensor the other does not, weights and gradients would silently pair up wrongly.
    #[test]
    fn both_traversals_visit_the_same_tensors() {
        let mut p = Params::initial(&tiny(), 3);
        let shapes: Vec<(usize, bool)> =
            p.tensors().iter().map(|(t, decay)| (t.len(), *decay)).collect();
        let mutable: Vec<(usize, bool)> =
            p.tensors_mut().iter().map(|(t, decay)| (t.len(), *decay)).collect();
        assert_eq!(shapes, mutable);
        assert_eq!(shapes.iter().map(|(n, _)| n).sum::<usize>(), tiny().parameter_count());
    }

    #[test]
    fn a_seed_gives_the_same_weights_every_time() {
        assert_eq!(Params::initial(&tiny(), 42), Params::initial(&tiny(), 42));
        assert_ne!(Params::initial(&tiny(), 42), Params::initial(&tiny(), 43));
    }

    #[test]
    fn forward_refuses_input_it_cannot_index() {
        let model = Model::new(tiny(), 1);
        assert!(model.forward(&[]).is_none());
        assert!(model.forward(&[0, 1, 2, 3, 4, 5, 6]).is_none(), "longer than the context");
        assert!(model.forward(&[0, 99]).is_none(), "id past the vocabulary");
        assert!(model.forward(&[0, 1, 2]).is_some());
    }

    /// Every attention row is a probability distribution over the positions up to and including
    /// itself, and exactly zero after it.
    #[test]
    fn attention_is_causal_and_normalised() {
        let model = Model::new(tiny(), 9);
        let tokens = [0usize, 3, 1, 6, 2];
        let acts = model.forward(&tokens).expect("valid input");
        let t = tokens.len();
        for block in &acts.blocks {
            for head in 0..model.shape.heads {
                for i in 0..t {
                    let row = &block.probs[(head * t + i) * t..(head * t + i) * t + t];
                    let sum: f32 = row[..=i].iter().sum();
                    assert!((sum - 1.0).abs() < 1e-5, "row {i} sums to {sum}");
                    for (j, &w) in row.iter().enumerate().skip(i + 1) {
                        assert_eq!(w, 0.0, "position {i} reached forward to {j}");
                    }
                }
            }
        }
    }

    /// The strongest statement of the mask: change a token in the future and nothing the model
    /// computed at an earlier position may move, at any depth. A mask applied after the softmax
    /// instead of before it, or an off-by-one in `0..=i`, fails here and nowhere else.
    #[test]
    fn changing_a_later_token_cannot_disturb_an_earlier_position() {
        let model = Model::new(tiny(), 11);
        let a = [2usize, 5, 0, 1, 4, 3];
        let mut b = a;
        b[4] = 6;
        b[5] = 0;
        let left = model.forward(&a).expect("valid input");
        let right = model.forward(&b).expect("valid input");
        for p in 0..4 {
            for (x, y) in left.logits.row(p).iter().zip(right.logits.row(p)) {
                assert_eq!(x, y, "logits moved at position {p}, which is before the change");
            }
            for (lb, rb) in left.blocks.iter().zip(&right.blocks) {
                assert_eq!(lb.x2.row(p), rb.x2.row(p), "block output moved at position {p}");
            }
        }
        assert_ne!(left.logits.row(5), right.logits.row(5), "the change must do something");
    }

    /// Central differences over every parameter group. A whole model is checked at once because
    /// a layer is only correct in the place it actually sits: an attention block checked on its
    /// own would not catch a residual connection wired to the wrong tensor.
    ///
    /// `h` is 1e-3, which was measured rather than guessed. Above it the biases that write
    /// into the residual stream shift every position at once and the curvature of the loss
    /// shows up as a several-percent error; below it the difference of two `f32` losses starts
    /// to be rounding. At 1e-3 every tensor in this model agrees to better than one percent.
    fn gradient_check(group: &str) {
        let shape = ModelShape::new(7, 8, 2, 2, 6).expect("valid shape");
        let model = Model::new(shape, 7);
        let tokens = [1usize, 4, 0, 6, 2];
        let targets = [4usize, 0, 6, 2, 5];

        let mut grads = Params::zeros(&shape);
        let acts = model.forward(&tokens).expect("valid input");
        model.backward(&tokens, &targets, &acts, &mut grads).expect("matching lengths");

        let loss_of = |params: &Params| -> f64 {
            let probe = Model { shape, params: params.clone() };
            let acts = probe.forward(&tokens).expect("valid input");
            probe.loss(&acts, &targets).expect("matching lengths")
        };

        // Which tensors belong to the group under test, by their index in the traversal.
        let count = model.params.tensors().len();
        let selected: Vec<usize> =
            (0..count).filter(|&i| tensor_group(&shape, i) == group).collect();
        assert!(!selected.is_empty(), "no tensor belongs to group {group}");

        let h = 1e-3f64;
        let mut checked = 0;
        for index in selected {
            let len = model.params.tensors()[index].0.len();
            // A handful of spread-out entries per tensor: enough to catch a wrong index or a
            // missing term, cheap enough to keep the whole suite under a minute.
            for probe in 0..5.min(len) {
                let offset = probe * len / 5.min(len).max(1);
                let analytic = grads.tensors()[index].0[offset] as f64;

                let mut up = model.params.clone();
                up.tensors_mut()[index].0[offset] += h as f32;
                let mut down = model.params.clone();
                down.tensors_mut()[index].0[offset] -= h as f32;
                let numeric = (loss_of(&up) - loss_of(&down)) / (2.0 * h);

                assert!(
                    (numeric - analytic).abs() < 2e-3 + 2e-2 * analytic.abs().max(numeric.abs()),
                    "{group} tensor {index} entry {offset}: analytic {analytic}, numeric {numeric}"
                );
                checked += 1;
            }
        }
        assert!(checked > 0);
    }

    /// Names the traversal position of each tensor so a failing gradient check says which layer
    /// is wrong. The order mirrors `Params::tensors` exactly.
    fn tensor_group(shape: &ModelShape, index: usize) -> &'static str {
        const PER_BLOCK: usize = 2 + 8 + 2 + 4;
        match index {
            0 => "token_emb",
            1 => "pos_emb",
            i if i < 2 + shape.layers * PER_BLOCK => match (i - 2) % PER_BLOCK {
                0 | 1 => "ln1",
                2..=7 => "attention_qkv",
                8 | 9 => "attention_out",
                10 | 11 => "ln2",
                _ => "ffn",
            },
            i if i < 2 + shape.layers * PER_BLOCK + 2 => "ln_f",
            _ => "head",
        }
    }

    #[test]
    fn gradient_check_token_embeddings() {
        gradient_check("token_emb");
    }

    #[test]
    fn gradient_check_position_embeddings() {
        gradient_check("pos_emb");
    }

    #[test]
    fn gradient_check_layer_norm_before_attention() {
        gradient_check("ln1");
    }

    #[test]
    fn gradient_check_attention_queries_keys_values() {
        gradient_check("attention_qkv");
    }

    #[test]
    fn gradient_check_attention_output_projection() {
        gradient_check("attention_out");
    }

    #[test]
    fn gradient_check_layer_norm_before_feed_forward() {
        gradient_check("ln2");
    }

    #[test]
    fn gradient_check_feed_forward() {
        gradient_check("ffn");
    }

    #[test]
    fn gradient_check_final_layer_norm() {
        gradient_check("ln_f");
    }

    #[test]
    fn gradient_check_output_head() {
        gradient_check("head");
    }

    /// A model that has never been trained should be about as surprised as a coin with `vocab`
    /// sides. Far from `ln(vocab)` in either direction means the initial weights are wrong.
    #[test]
    fn an_untrained_model_starts_at_chance() {
        let shape = tiny();
        let model = Model::new(shape, 5);
        let tokens = [0usize, 1, 2, 3, 4];
        let acts = model.forward(&tokens).expect("valid input");
        let loss = model.loss(&acts, &[1, 2, 3, 4, 5]).expect("matching lengths");
        let chance = (shape.vocab as f64).ln();
        assert!((loss - chance).abs() < 0.2, "loss {loss}, chance {chance}");
    }

    #[test]
    fn backward_returns_the_same_loss_as_the_forward_pass() {
        let shape = tiny();
        let model = Model::new(shape, 13);
        let tokens = [3usize, 1, 5, 0];
        let targets = [1usize, 5, 0, 2];
        let acts = model.forward(&tokens).expect("valid input");
        let mut grads = Params::zeros(&shape);
        let a = model.loss(&acts, &targets).expect("matching lengths");
        let b = model.backward(&tokens, &targets, &acts, &mut grads).expect("matching lengths");
        assert!((a - b).abs() < 1e-9, "{a} vs {b}");
    }
}
