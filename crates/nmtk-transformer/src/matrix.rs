//! Matrices, and the only three multiplications a transformer needs.
//!
//! Every linear layer in this crate is `y = x · Wᵀ + b`, with the weight stored as
//! `(out_features, in_features)`. That one choice decides the shape of all three kernels below,
//! and it is made so that every inner loop walks two rows of memory forwards: a row of `x` and a
//! row of `W` are both contiguous, so the dot product reads straight lines. A weight stored the
//! other way round would need a transpose or a strided read in the hot loop.
//!
//! `rayon` splits the *output* of each kernel, never the sum inside it. That is what keeps a run
//! reproducible: each output number is added up by one thread in one fixed order, so the answer
//! does not depend on how many cores the machine has.

use rayon::prelude::*;

/// Below this many multiply-adds the work is smaller than the cost of waking other threads, so
/// the kernel runs on the calling thread. Small heads and short sequences hit this constantly.
const PARALLEL_THRESHOLD: usize = 1 << 15;

/// A dense row-major matrix of `f32`.
///
/// `f32` rather than `f64` on purpose: it halves the memory traffic, and training is limited by
/// memory traffic long before it is limited by precision. The loss is accumulated in `f64`
/// anyway, where the cancellation actually matters.
#[derive(Clone, Debug, PartialEq)]
pub struct Matrix {
    /// Number of rows.
    pub rows: usize,
    /// Number of columns.
    pub cols: usize,
    /// `rows * cols` values, row `r` starting at `r * cols`.
    pub data: Vec<f32>,
}

impl Matrix {
    /// A matrix of zeros.
    pub fn zeros(rows: usize, cols: usize) -> Self {
        Self { rows, cols, data: vec![0.0; rows * cols] }
    }

    /// Wraps existing values, or `None` when they do not fill the shape exactly.
    pub fn from_vec(rows: usize, cols: usize, data: Vec<f32>) -> Option<Self> {
        (data.len() == rows * cols).then_some(Self { rows, cols, data })
    }

    /// Row `r`. The caller has already checked the shape; every call site in this crate builds
    /// the index from the same dimensions it built the matrix from.
    #[inline]
    pub fn row(&self, r: usize) -> &[f32] {
        &self.data[r * self.cols..(r + 1) * self.cols]
    }

    /// Row `r`, for writing.
    #[inline]
    pub fn row_mut(&mut self, r: usize) -> &mut [f32] {
        let cols = self.cols;
        &mut self.data[r * cols..(r + 1) * cols]
    }

    /// Every row in order.
    pub fn rows_iter(&self) -> impl Iterator<Item = &[f32]> {
        self.data.chunks_exact(self.cols)
    }

    /// Sets every value back to zero, keeping the allocation.
    pub fn fill_zero(&mut self) {
        self.data.fill(0.0);
    }

    /// `self += other`, element by element. Shapes must already agree.
    pub fn add_assign(&mut self, other: &Matrix) {
        for (a, b) in self.data.iter_mut().zip(&other.data) {
            *a += b;
        }
    }
}

/// The dot product of two equally long rows.
///
/// Four running sums instead of one: a floating-point add depends on the previous add, so a
/// single accumulator leaves the processor's add units idle waiting for each other. Four
/// independent chains keep them busy. The order is fixed, so the answer is always the same.
#[inline]
pub fn dot(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(a.len(), b.len());
    let mut acc = [0.0f32; 4];
    let (lanes_a, tail_a) = a.as_chunks::<4>();
    let (lanes_b, tail_b) = b.as_chunks::<4>();
    for (x, y) in lanes_a.iter().zip(lanes_b) {
        acc[0] += x[0] * y[0];
        acc[1] += x[1] * y[1];
        acc[2] += x[2] * y[2];
        acc[3] += x[3] * y[3];
    }
    let tail: f32 = tail_a.iter().zip(tail_b).map(|(x, y)| x * y).sum();
    ((acc[0] + acc[1]) + (acc[2] + acc[3])) + tail
}

/// `y += a * x`, the one-line building block of every backward pass here.
#[inline]
pub fn axpy(a: f32, x: &[f32], y: &mut [f32]) {
    debug_assert_eq!(x.len(), y.len());
    for (slot, value) in y.iter_mut().zip(x) {
        *slot += a * value;
    }
}

/// Forward pass of a linear layer: `y = x · wᵀ + bias`.
///
/// `x` is `(m, k)`, `w` is `(n, k)`, `bias` is `n` long, the result is `(m, n)`.
pub fn linear_forward(x: &Matrix, w: &Matrix, bias: &[f32]) -> Matrix {
    debug_assert_eq!(x.cols, w.cols);
    debug_assert_eq!(w.rows, bias.len());
    let (m, k, n) = (x.rows, x.cols, w.rows);
    let mut y = Matrix::zeros(m, n);
    let one_row = |(r, out): (usize, &mut [f32])| {
        let xr = &x.data[r * k..(r + 1) * k];
        for (o, slot) in out.iter_mut().enumerate() {
            *slot = dot(xr, &w.data[o * k..(o + 1) * k]) + bias[o];
        }
    };
    if m * n * k >= PARALLEL_THRESHOLD {
        y.data.par_chunks_mut(n).enumerate().for_each(one_row);
    } else {
        y.data.chunks_mut(n).enumerate().for_each(one_row);
    }
    y
}

/// Gradient of a linear layer with respect to its input: `dx += dy · w`.
///
/// `dy` is `(m, n)`, `w` is `(n, k)`, `dx` is `(m, k)`. Written as a sum of scaled weight rows
/// rather than as dot products, because that way both the read and the write walk forwards.
pub fn linear_backward_input(dy: &Matrix, w: &Matrix, dx: &mut Matrix) {
    debug_assert_eq!(dy.cols, w.rows);
    debug_assert_eq!(dx.cols, w.cols);
    debug_assert_eq!(dy.rows, dx.rows);
    let (m, n, k) = (dy.rows, w.rows, w.cols);
    let one_row = |(r, out): (usize, &mut [f32])| {
        let dyr = &dy.data[r * n..(r + 1) * n];
        for (o, &g) in dyr.iter().enumerate() {
            axpy(g, &w.data[o * k..(o + 1) * k], out);
        }
    };
    if m * n * k >= PARALLEL_THRESHOLD {
        dx.data.par_chunks_mut(k).enumerate().for_each(one_row);
    } else {
        dx.data.chunks_mut(k).enumerate().for_each(one_row);
    }
}

/// Gradient of a linear layer with respect to its weight and bias:
/// `dw += dyᵀ · x` and `db += Σ dy`.
///
/// `dy` is `(m, n)`, `x` is `(m, k)`, `dw` is `(n, k)`, `db` is `n` long. The sum over `m` is a
/// reduction, so the split is over `n` — one thread owns one output row and one bias slot, and
/// nothing is added up across threads.
pub fn linear_backward_weight(dy: &Matrix, x: &Matrix, dw: &mut Matrix, db: &mut [f32]) {
    debug_assert_eq!(dy.rows, x.rows);
    debug_assert_eq!(dy.cols, dw.rows);
    debug_assert_eq!(x.cols, dw.cols);
    debug_assert_eq!(db.len(), dw.rows);
    let (m, n, k) = (dy.rows, dw.rows, dw.cols);
    let one_output = |(o, (out, bias_slot)): (usize, (&mut [f32], &mut f32))| {
        for r in 0..m {
            let g = dy.data[r * n + o];
            *bias_slot += g;
            axpy(g, &x.data[r * k..(r + 1) * k], out);
        }
    };
    if m * n * k >= PARALLEL_THRESHOLD {
        dw.data.par_chunks_mut(k).zip(db.par_iter_mut()).enumerate().for_each(one_output);
    } else {
        dw.data.chunks_mut(k).zip(db.iter_mut()).enumerate().for_each(one_output);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(rows: usize, cols: usize, data: &[f32]) -> Matrix {
        Matrix::from_vec(rows, cols, data.to_vec()).expect("shape matches the data")
    }

    /// A worked example small enough to check by hand.
    #[test]
    fn linear_forward_matches_pencil_and_paper() {
        let x = m(2, 3, &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0]);
        let w = m(2, 3, &[1.0, 0.0, -1.0, 0.5, 0.5, 0.5]);
        let y = linear_forward(&x, &w, &[10.0, -1.0]);
        // row 0: (1-3)+10 = 8, (0.5+1+1.5)-1 = 2
        // row 1: (4-6)+10 = 8, (2+2.5+3)-1 = 6.5
        assert_eq!(y.data, vec![8.0, 2.0, 8.0, 6.5]);
    }

    /// The two backward kernels must be the exact transposes of the forward one. Checking them
    /// against a slow reference implementation catches an index slip that a gradient check on a
    /// whole model would blur into the other layers.
    #[test]
    fn backward_kernels_match_a_naive_transpose() {
        let (m_rows, k, n) = (5, 7, 4);
        let x = m(m_rows, k, &(0..m_rows * k).map(|i| (i as f32 * 0.37).sin()).collect::<Vec<_>>());
        let w = m(n, k, &(0..n * k).map(|i| (i as f32 * 0.11).cos()).collect::<Vec<_>>());
        let dy =
            m(m_rows, n, &(0..m_rows * n).map(|i| (i as f32 * 0.71).sin()).collect::<Vec<_>>());

        let mut dx = Matrix::zeros(m_rows, k);
        linear_backward_input(&dy, &w, &mut dx);
        for r in 0..m_rows {
            for i in 0..k {
                let want: f32 = (0..n).map(|o| dy.row(r)[o] * w.row(o)[i]).sum();
                assert!((dx.row(r)[i] - want).abs() < 1e-5, "dx[{r}][{i}]");
            }
        }

        let mut dw = Matrix::zeros(n, k);
        let mut db = vec![0.0; n];
        linear_backward_weight(&dy, &x, &mut dw, &mut db);
        for (o, &got_b) in db.iter().enumerate() {
            let want_b: f32 = (0..m_rows).map(|r| dy.row(r)[o]).sum();
            assert!((got_b - want_b).abs() < 1e-5, "db[{o}]");
            for i in 0..k {
                let want: f32 = (0..m_rows).map(|r| dy.row(r)[o] * x.row(r)[i]).sum();
                assert!((dw.row(o)[i] - want).abs() < 1e-5, "dw[{o}][{i}]");
            }
        }
    }

    /// The kernels are big enough here to cross the threshold and take the rayon path. The
    /// answer has to be the same one the single-threaded path gives, bit for bit, or a fixed
    /// seed would stop meaning anything.
    #[test]
    fn the_parallel_path_gives_the_same_bits_as_the_serial_one() {
        let (m_rows, k, n) = (64, 64, 64);
        assert!(m_rows * n * k >= PARALLEL_THRESHOLD);
        let x = m(
            m_rows,
            k,
            &(0..m_rows * k).map(|i| ((i % 97) as f32 - 48.0) / 31.0).collect::<Vec<_>>(),
        );
        let w = m(n, k, &(0..n * k).map(|i| ((i % 89) as f32 - 44.0) / 17.0).collect::<Vec<_>>());
        let bias: Vec<f32> = (0..n).map(|i| i as f32 * 0.01).collect();

        let parallel = linear_forward(&x, &w, &bias);
        // The same maths, one row at a time, in the order the serial branch uses.
        let mut serial = Matrix::zeros(m_rows, n);
        for r in 0..m_rows {
            for (o, slot) in serial.row_mut(r).iter_mut().enumerate() {
                *slot = dot(x.row(r), w.row(o)) + bias[o];
            }
        }
        assert_eq!(parallel.data, serial.data);
    }

    #[test]
    fn a_dot_product_of_odd_length_still_uses_the_tail() {
        let a: Vec<f32> = (1..=7).map(|v| v as f32).collect();
        let b = vec![1.0f32; 7];
        assert_eq!(dot(&a, &b), 28.0);
    }
}
