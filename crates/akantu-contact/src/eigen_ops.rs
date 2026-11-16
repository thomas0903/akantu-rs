// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the evaluation order of the Eigen 3.4
// expressions written in src/model/contact_mechanics/{geometry_utils_inline_impl.hh,
// contact_detector_inline_impl.hh, contact_mechanics_model.cc,
// resolution_utils.hh, resolutions/resolution_penalty.cc,
// resolutions/resolution_penalty_quadratic.cc}. No Eigen code: products are
// evaluated by `akantu_core::eigen_gebp` / `akantu_fe::eigen` (the project's
// maintained translation of Eigen 3.4.0's strict-build products).

//! Eigen-order kernels of the contact layer.
//!
//! Every contact expression upstream operates on **run-time sized** Eigen
//! objects (`Vector<Real>`, `Matrix<Real>`, `VectorProxy` / `MatrixProxy`
//! maps of nodal arrays, `Tensor3` slices). Their reductions are therefore
//! sequential from the first term (`dot`, `norm`, coefficient-based
//! products), their matrix–vector products accumulate from `+0` (GEMV), and
//! the few fixed-size spots (`Vector<Real, 3>` cross products) use the
//! halving tree. Matrices are column-major slices with explicit extents.
//!
//! Products go through prepared `akantu_core::eigen_kernels::SliceProduct`s
//! memoised per shape ([`matmul`]); `SliceProduct::eval` gives the bits of
//! `akantu_fe::eigen::product_slices`. The only operation not covered by
//! them is
//! a GEMV whose left operand carries a scalar factor (`s * A.transpose() *
//! v`): Eigen's `blas_traits` extract `s` and the kernel computes
//! `res_i += s · (Σ_j a_ij v_j)` into a zeroed destination
//! ([`gemv_scaled`]).

use std::cell::RefCell;

use akantu_core::eigen_gebp::{Order, gemv_into};
use akantu_core::eigen_kernels::SliceProduct;
use akantu_core::eigen_order::{Reduction, sum_from_zero};
use akantu_core::{Error, Real, Result};
use akantu_fe::eigen::{Dims, normalize as eigen_normalize, prepare};

/// `a.dot(b)` on run-time sized operands (sequential from the first term).
#[inline]
pub(crate) fn dot(a: &[Real], b: &[Real]) -> Real {
    debug_assert_eq!(a.len(), b.len());
    Reduction::Sequential.sum(a.len(), &|i| a[i] * b[i])
}

/// `v.squaredNorm()` on a run-time sized vector.
#[inline]
pub(crate) fn squared_norm(v: &[Real]) -> Real {
    Reduction::Sequential.sum(v.len(), &|i| v[i] * v[i])
}

/// `v.norm()` on a run-time sized vector.
#[inline]
pub(crate) fn norm(v: &[Real]) -> Real {
    squared_norm(v).sqrt()
}

/// `v.normalize()` / `v = v.normalized()` on a run-time sized vector
/// (unchanged when the squared norm is not positive).
#[inline]
pub(crate) fn normalize(v: &mut [Real]) {
    eigen_normalize(v, false);
}

/// `(a.cross(b)).normalized()` on `Vector<Real, 3>` proxies (fixed size:
/// halving-tree squared norm).
#[inline]
pub(crate) fn cross_normalized(a: &[Real], b: &[Real]) -> [Real; 3] {
    let mut n = akantu_fe::eigen::cross(&[a[0], a[1], a[2]], &[b[0], b[1], b[2]]);
    eigen_normalize(&mut n, true);
    n
}

/// GEMV `out = M · x` with a plain column-major run-time `M` (`rows ×
/// cols`) and a `Vector<Real>` / column proxy `x`: `out_i = 0 + Σ_l` from
/// `+0` (Eigen `general_matrix_vector_product`, column-major kernel).
#[inline]
pub(crate) fn gemv(m: &[Real], rows: usize, cols: usize, x: &[Real], out: &mut [Real]) {
    debug_assert!(m.len() >= rows * cols && x.len() >= cols && out.len() == rows);
    gemv_into(
        rows,
        cols,
        Order::ColMajor,
        &|i, l| m[i + l * rows],
        &|l| x[l],
        out,
    );
}

/// GEMV with an extracted scalar factor on the left operand, whose matrix
/// is the **transpose** of the column-major `a` (`a_rows × a_cols`):
/// `out = s * a.transpose() * x`, `out_i = 0 + s · (Σ_j a(j, i) x_j)` with
/// the sum from `+0` (row-major kernel, `res += alpha * c`).
#[inline]
pub(crate) fn gemv_scaled_transposed(
    s: Real,
    a: &[Real],
    a_rows: usize,
    a_cols: usize,
    x: &[Real],
    out: &mut [Real],
) {
    debug_assert!(a.len() >= a_rows * a_cols && x.len() >= a_rows && out.len() == a_cols);
    for (i, o) in out.iter_mut().enumerate() {
        let c = sum_from_zero(a_rows, &|j| a[j + i * a_rows] * x[j]);
        *o = 0.0;
        *o += s * c;
    }
}

/// Product of two run-time sized operands, routed and evaluated as Eigen
/// does (`lhs`/`rhs` column-major storage; `lt`/`rt`: the operand is the
/// `.transpose()` of that storage). Returns the result column-major in
/// `out` (`rows × cols` of the logical product).
#[allow(clippy::too_many_arguments)] // two operand descriptions + output
#[inline]
pub(crate) fn matmul(
    a: &[Real],
    a_rows: usize,
    a_cols: usize,
    lt: bool,
    b: &[Real],
    b_rows: usize,
    b_cols: usize,
    rt: bool,
    out: &mut [Real],
) -> Result<()> {
    let p = prepared((a_rows, a_cols, lt, b_rows, b_cols, rt))?;
    // the checks and messages of `akantu_fe::eigen::product_slices`
    for (context, need, got) in [
        ("eigen::product_slices: lhs", p.lhs_len(), a.len()),
        ("eigen::product_slices: rhs", p.rhs_len(), b.len()),
        ("eigen::product: result", p.out_len(), out.len()),
    ] {
        if got < need {
            return Err(Error::DimensionMismatch {
                context,
                expected: need,
                got,
            });
        }
    }
    p.eval(a, b, out)
}

/// Shape of a [`matmul`]: the two operands' storage extents and transpose
/// flags.
type MatmulShape = (usize, usize, bool, usize, usize, bool);

/// Most shapes kept per thread (the contact expressions use a handful:
/// dimension ≤ 3, at most `MAX_FACET_NODES` nodes).
const PREPARED_CAPACITY: usize = 64;

thread_local! {
    /// The prepared products of the shapes this thread has evaluated: the
    /// route, strides and lengths are decided once per shape
    /// (`SliceProduct::eval` gives the bits of `product_slices`).
    static PREPARED: RefCell<Vec<(MatmulShape, SliceProduct)>> = const { RefCell::new(Vec::new()) };
}

/// The prepared product of `shape` (memoised per thread).
fn prepared(shape: MatmulShape) -> Result<SliceProduct> {
    if let Some(p) = PREPARED.with_borrow(|v| v.iter().find(|(k, _)| *k == shape).map(|e| e.1)) {
        return Ok(p);
    }
    let (a_rows, a_cols, lt, b_rows, b_cols, rt) = shape;
    let mut l = Dims::dynamic(a_rows, a_cols);
    if lt {
        l = l.t();
    }
    let mut r = Dims::dynamic(b_rows, b_cols);
    if rt {
        r = r.t();
    }
    let p = prepare(l, r)?;
    PREPARED.with_borrow_mut(|v| {
        if v.len() < PREPARED_CAPACITY {
            v.push((shape, p));
        }
    });
    Ok(p)
}

/// Outer product `u * v.transpose()` of two run-time column vectors
/// (Eigen outer-product route: `dst.col(j) = v_j * u`).
#[inline]
pub(crate) fn outer(u: &[Real], v: &[Real], out: &mut [Real]) {
    let r = u.len();
    debug_assert!(out.len() >= r * v.len());
    for (j, &vj) in v.iter().enumerate() {
        for (i, &ui) in u.iter().enumerate() {
            out[i + j * r] = vj * ui;
        }
    }
}

/// Maximal surface dimension of the stack-based small inverse.
pub(crate) const SMALL_MAX: usize = 2;

/// Inverse of a run-time sized `n × n` matrix (`n ≤ 2`, column-major):
/// Eigen `Matrix<Real>::inverse()` = `PartialPivLU` solve with the
/// identity (unblocked LU with division by the pivot, unit-lower then upper
/// triangular solves multiplying by `1 / u_ii`). An exactly singular input
/// gives `inf`/`NaN` as upstream (returns `true` then). Reproduces
/// `akantu_core::tensor::MatrixX::inverse_unchecked` bit for bit (unit test).
pub(crate) fn small_inverse(n: usize, m: &[Real], out: &mut [Real]) -> bool {
    debug_assert!(n <= SMALL_MAX && m.len() >= n * n && out.len() >= n * n);
    let mut a = [0.0; SMALL_MAX * SMALL_MAX];
    a[..n * n].copy_from_slice(&m[..n * n]);
    let mut tr = [0usize; SMALL_MAX];
    let mut zero_pivot = false;
    for k in 0..n {
        let mut piv = k;
        let mut biggest = a[k + k * n].abs();
        for i in k + 1..n {
            let s = a[i + k * n].abs();
            if s > biggest {
                biggest = s;
                piv = i;
            }
        }
        tr[k] = piv;
        if biggest != 0.0 {
            if piv != k {
                for j in 0..n {
                    a.swap(k + j * n, piv + j * n);
                }
            }
            let d = a[k + k * n];
            for i in k + 1..n {
                a[i + k * n] /= d;
            }
        } else {
            zero_pivot = true;
        }
        for j in k + 1..n {
            let u = a[k + j * n];
            for i in k + 1..n {
                a[i + j * n] -= a[i + k * n] * u;
            }
        }
    }
    let out = &mut out[..n * n];
    out.fill(0.0);
    for i in 0..n {
        out[i + i * n] = 1.0;
    }
    for (k, &p) in tr.iter().enumerate().take(n) {
        if p != k {
            for j in 0..n {
                out.swap(k + j * n, p + j * n);
            }
        }
    }
    for i in 0..n {
        for j in 0..n {
            let bij = out[i + j * n];
            for r in i + 1..n {
                out[r + j * n] -= bij * a[r + i * n];
            }
        }
    }
    for k in 0..n {
        let i = n - k - 1;
        let inv = 1.0 / a[i + i * n];
        for j in 0..n {
            out[i + j * n] *= inv;
            let bij = out[i + j * n];
            for r in 0..i {
                out[r + j * n] -= bij * a[r + i * n];
            }
        }
    }
    zero_pivot
}

/// `T.transpose() * T` of a run-time `rows × cols` column-major `T`
/// (upstream `GeometryUtils::covariantMetricTensor`), `cols × cols` into
/// `out`.
#[inline]
pub(crate) fn gram(t: &[Real], rows: usize, cols: usize, out: &mut [Real]) -> Result<()> {
    matmul(t, rows, cols, true, t, rows, cols, false, out)
}

/// `(T.transpose() * T).inverse()` (upstream
/// `GeometryUtils::contravariantMetricTensor`), `cols ≤ 2`.
#[inline]
pub(crate) fn inverse_gram(t: &[Real], rows: usize, cols: usize, out: &mut [Real]) -> Result<()> {
    let mut g = [0.0; SMALL_MAX * SMALL_MAX];
    gram(t, rows, cols, &mut g[..cols * cols])?;
    small_inverse(cols, &g[..cols * cols], out);
    Ok(())
}

/// C++ `std::max(a, b)` (`(a < b) ? b : a`: `a` on ties and for a NaN `b`).
#[inline]
pub(crate) fn cpp_max(a: Real, b: Real) -> Real {
    if a < b { b } else { a }
}

/// C++ `std::min(a, b)` (`(b < a) ? b : a`).
#[inline]
pub(crate) fn cpp_min(a: Real, b: Real) -> Real {
    if b < a { b } else { a }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::tensor::MatrixX;

    fn rng(seed: &mut u64) -> Real {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        ((*seed >> 11) as Real) / ((1u64 << 53) as Real) * 4.0 - 2.0
    }

    #[test]
    fn small_inverse_matches_core_lu_bitwise() {
        let mut seed = 0x1234_5678_9abc_def0;
        for n in 1..=2 {
            for _ in 0..500 {
                let m: Vec<Real> = (0..n * n).map(|_| rng(&mut seed)).collect();
                let mut out = vec![0.0; n * n];
                small_inverse(n, &m, &mut out);
                let core = MatrixX::from_col_major(n, n, m.clone())
                    .unwrap()
                    .inverse_unchecked()
                    .unwrap();
                for (a, b) in out.iter().zip(core.as_slice()) {
                    assert_eq!(a.to_bits(), b.to_bits(), "n = {n}, m = {m:?}");
                }
            }
        }
        // pivoting case and singular case
        let mut out = [0.0; 4];
        small_inverse(2, &[1e-3, 2.0, 3.0, 4.0], &mut out);
        let core = MatrixX::from_col_major(2, 2, vec![1e-3, 2.0, 3.0, 4.0])
            .unwrap()
            .inverse_unchecked()
            .unwrap();
        assert_eq!(out.to_vec(), core.as_slice().to_vec());
        assert!(small_inverse(1, &[0.0], &mut out));
        assert!(out[0].is_infinite());
    }

    #[test]
    fn std_min_max_semantics() {
        assert_eq!(cpp_max(1.0, Real::NAN), 1.0);
        assert!(cpp_max(Real::NAN, 1.0).is_nan());
        assert_eq!(cpp_min(1.0, Real::NAN), 1.0);
        assert_eq!(cpp_max(-0.0, 0.0).to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn scaled_gemv_order() {
        // a = [[1, 2], [3, 4], [5, 6]] (3 × 2), out = s · aᵀ x
        let a = [1.0, 3.0, 5.0, 2.0, 4.0, 6.0];
        let x = [1.0, 0.5, -1.0];
        let mut out = [0.0; 2];
        gemv_scaled_transposed(2.0, &a, 3, 2, &x, &mut out);
        assert_eq!(out, [2.0 * (1.0 + 1.5 - 5.0), 2.0 * (2.0 + 2.0 - 6.0)]);
    }

    /// `matmul` (prepared products, memoised per shape) against
    /// `akantu_fe::eigen::product_slices`, the route it replaces: every shape
    /// of the contact expressions (dimension 2 and 3, 2 to 8 facet nodes,
    /// both transpose flags), random values with signed zeros, infinities,
    /// NaNs and subnormals; bitwise (NaN sign: the rule of
    /// `akantu_core::bitwise_testing::same`).
    #[test]
    fn prepared_matmul_matches_product_slices() {
        let mut seed = 0x2545_f491_4f6c_dd1d_u64;
        let specials = [
            0.0,
            -0.0,
            Real::INFINITY,
            Real::NEG_INFINITY,
            Real::NAN,
            5e-324,
            -1e-310,
        ];
        let value = |seed: &mut u64| {
            let r = rng(seed);
            if r < 0.05 {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // index
                let k = ((r / 0.05) * specials.len() as Real) as usize;
                specials[k.min(specials.len() - 1)]
            } else {
                rng(seed) * 4.0 - 2.0
            }
        };
        for _ in 0..3 {
            for m in 1..=3 {
                for k in 1..=8 {
                    for n in 1..=3 {
                        for (lt, rt) in [(false, false), (true, false), (false, true), (true, true)]
                        {
                            let (ar, ac) = if lt { (k, m) } else { (m, k) };
                            let (br, bc) = if rt { (n, k) } else { (k, n) };
                            let a: Vec<Real> = (0..ar * ac).map(|_| value(&mut seed)).collect();
                            let b: Vec<Real> = (0..br * bc).map(|_| value(&mut seed)).collect();
                            let mut x = vec![7.0; m * n];
                            let mut y = vec![7.0; m * n];
                            matmul(&a, ar, ac, lt, &b, br, bc, rt, &mut x).unwrap();
                            let mut l = Dims::dynamic(ar, ac);
                            if lt {
                                l = l.t();
                            }
                            let mut r = Dims::dynamic(br, bc);
                            if rt {
                                r = r.t();
                            }
                            akantu_fe::eigen::product_slices(l, r, &a, &b, &mut y).unwrap();
                            for (u, v) in x.iter().zip(&y) {
                                assert!(
                                    akantu_core::bitwise_testing::same(*u, *v),
                                    "{m}x{k}x{n} {lt} {rt}: {u} {v}"
                                );
                            }
                        }
                    }
                }
            }
        }
    }
}
