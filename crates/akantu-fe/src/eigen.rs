// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the evaluation order of the Eigen 3.4
// expressions written at the call sites of src/fe_engine/*
// (element_class_tmpl.hh, shape_lagrange_inline_impl.hh,
// shape_functions_inline_impl.hh, integrator_gauss_inline_impl.hh,
// fe_engine_template_tmpl*.hh). This file contains no Eigen code: it
// describes the compile-time/run-time extents and storage order of the
// operands at those call sites and evaluates every product with
// `akantu_core::eigen_gebp` (the one maintained translation of Eigen 3.4.0's
// strict-build products); the reductions build on `akantu_core::eigen_order`.

//! Eigen-order kernels for the FE layer: products of **mixed**
//! fixed/dynamic operands, norms, LU (D-008, D-011, D-036).
//!
//! `akantu_core::tensor` covers products of two fixed-size operands
//! (`Matrix<R, C>`) and of two dynamic operands (`MatrixX`). Upstream FE call
//! sites frequently mix them, e.g. `dnds * X.transpose()` with `dnds` a
//! `Matrix<Real, nd, nn>` and `X` a dynamic `Map`. Eigen picks the product
//! implementation from the operands' *compile-time* sizes, so every FE
//! product describes its two operand expressions with [`Dims`] (sizes,
//! which of them are compile-time constants, and whether the operand is a
//! `.transpose()`, i.e. row-major for Eigen's kernels). [`product_shape`]
//! turns the pair into an `akantu_core::eigen_gebp::ProductShape`, and
//! [`product`] / [`product_slices`] evaluate it with
//! `akantu_core::eigen_gebp::product_into` — the same route selection and
//! kernels as the dense layer, so the FE helper and the core can no longer
//! disagree. In short (details in `eigen_gebp`):
//!
//! * route: rows class from the lhs, cols class from the rhs, depth class
//!   from the lhs columns if they are compile-time, else from the rhs rows
//!   ("prefer fixed"); inner / outer / coefficient-based / GEMV / GEMM;
//! * coefficient reductions (coefficient-based and inner products, and the
//!   run-time `1 × 1` inner-product fallback of GEMV/GEMM, which is then
//!   added to a zeroed destination) take their compile-time length from the
//!   **lhs** operand only (`traits<CwiseBinaryOp>` copies the sizes of its
//!   left argument): halving tree when the lhs has compile-time columns and
//!   the expression is within the unrolling limit, otherwise sequential from
//!   the first term;
//! * GEMV kernels accumulate from `+0`; GEMM products fall back at run time
//!   to the coefficient-based product when `rows + cols + depth < 20`, to
//!   GEMV when the result has one row or one column, and otherwise use the
//!   blocked `gebp` kernel, reproduced bit for bit by `eigen_gebp` (its
//!   compile-time-finite depth blocking assumes a destination with the
//!   product's extents; it only matters for depths above 408, which no FE
//!   product reaches).
//!
//! The small dynamic LU helpers (`lu_det`, `lu_inverse`) reproduce
//! `PartialPivLU` exactly as `akantu_core::tensor::MatrixX::{det, inverse}`
//! do (and are tested against them bit for bit) but work on stack storage for
//! the `n ≤ 3` matrices of FE kernels (Jacobians), so that per-quadrature-point
//! loops do not allocate.

use akantu_core::eigen_gebp::{self, Extent, Order, ProductShape};
pub use akantu_core::eigen_kernels::SliceProduct;
use akantu_core::eigen_order as eo;
use akantu_core::{Error, Real, Result};

pub use akantu_core::eigen_gebp::Route;

/// Sizes of an Eigen operand expression, whether each one is a compile-time
/// constant (`Matrix<Real, 3, 3>`: both fixed; `Matrix<Real>` /
/// `MatrixProxy`: both dynamic; `Matrix<Real, Dynamic, 3>`: fixed columns
/// only) and whether it is the transpose of column-major storage.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Dims {
    /// Rows.
    pub rows: usize,
    /// Columns.
    pub cols: usize,
    /// Rows known at compile time.
    pub fixed_rows: bool,
    /// Columns known at compile time.
    pub fixed_cols: bool,
    /// The operand is `m.transpose()` of a column-major `m`, i.e. row-major
    /// for Eigen's kernels (set by [`Dims::t`]). It selects the GEMV kernel
    /// and, in [`product_slices`], the layout of the operand slice.
    pub transposed: bool,
}

const fn extent(n: usize, fixed: bool) -> Extent {
    if fixed {
        Extent::Fixed(n)
    } else {
        Extent::Dynamic(n)
    }
}

impl Dims {
    /// Column-major operand whose sizes are both compile-time constants.
    #[must_use]
    pub const fn fixed(rows: usize, cols: usize) -> Self {
        Self::new(rows, cols, true, true)
    }

    /// Column-major operand whose sizes are both run-time values.
    #[must_use]
    pub const fn dynamic(rows: usize, cols: usize) -> Self {
        Self::new(rows, cols, false, false)
    }

    /// Column-major operand with explicit compile-time flags.
    #[must_use]
    pub const fn new(rows: usize, cols: usize, fixed_rows: bool, fixed_cols: bool) -> Self {
        Self {
            rows,
            cols,
            fixed_rows,
            fixed_cols,
            transposed: false,
        }
    }

    /// The transposed operand (`.transpose()` swaps sizes and flags and
    /// flips the storage order).
    #[must_use]
    pub const fn t(self) -> Self {
        Self {
            rows: self.cols,
            cols: self.rows,
            fixed_rows: self.fixed_cols,
            fixed_cols: self.fixed_rows,
            transposed: !self.transposed,
        }
    }

    /// Row extent as Eigen sees it (`RowsAtCompileTime`).
    #[must_use]
    pub const fn row_extent(self) -> Extent {
        extent(self.rows, self.fixed_rows)
    }

    /// Column extent as Eigen sees it (`ColsAtCompileTime`).
    #[must_use]
    pub const fn col_extent(self) -> Extent {
        extent(self.cols, self.fixed_cols)
    }

    /// Storage order as Eigen's kernels see it.
    #[must_use]
    pub const fn order(self) -> Order {
        if self.transposed {
            Order::RowMajor
        } else {
            Order::ColMajor
        }
    }
}

/// The `akantu_core::eigen_gebp` description of the upstream product
/// `lhs * rhs` (both operands' extents and storage orders).
#[must_use]
pub const fn product_shape(lhs: Dims, rhs: Dims) -> ProductShape {
    let mut shape = ProductShape::new(
        lhs.row_extent(),
        lhs.col_extent(),
        rhs.row_extent(),
        rhs.col_extent(),
    );
    if lhs.transposed {
        shape = shape.lhs_transposed();
    }
    if rhs.transposed {
        shape = shape.rhs_transposed();
    }
    shape
}

/// Route Eigen 3.4 (strict build) takes for `lhs * rhs`
/// (`akantu_core::eigen_gebp::route` of [`product_shape`]).
#[must_use]
pub fn product_route(lhs: Dims, rhs: Dims) -> Route {
    eigen_gebp::route(&product_shape(lhs, rhs))
}

/// `out(i, j) = Σ_k a(i, k)·b(k, j)` for `i < lhs.rows`, `j < rhs.cols`,
/// evaluated exactly as Eigen 3.4.0's strict build evaluates the upstream
/// product of operands `lhs`, `rhs` (`akantu_core::eigen_gebp::product_into`).
/// `a(i, k)` and `b(k, j)` read the operands' logical coefficients; `out`
/// receives the column-major `lhs.rows × rhs.cols` result in its first
/// `lhs.rows · rhs.cols` entries. Returns the route taken.
///
/// # Errors
/// [`Error::DimensionMismatch`] if `lhs.cols != rhs.rows` or `out` is
/// shorter than the result (nothing is written then).
pub fn product(
    lhs: Dims,
    rhs: Dims,
    a: impl Fn(usize, usize) -> Real,
    b: impl Fn(usize, usize) -> Real,
    out: &mut [Real],
) -> Result<Route> {
    let shape = product_shape(lhs, rhs);
    let len = akantu_core::types::checked_len(lhs.rows, rhs.cols, "eigen::product result")?;
    let got = out.len();
    // the error is built only on a short buffer
    let Some(out) = out.get_mut(..len) else {
        return Err(Error::DimensionMismatch {
            context: "eigen::product: result",
            expected: len,
            got,
        });
    };
    eigen_gebp::product_into(&shape, &a, &b, out)?;
    Ok(eigen_gebp::route(&shape))
}

/// [`product`] of operands stored in slices: `a` holds the `lhs.rows ×
/// lhs.cols` lhs column-major, or — for a transposed `lhs` — the column-major
/// `lhs.cols × lhs.rows` matrix it transposes (likewise `b` for `rhs`);
/// `c` receives the column-major `lhs.rows × rhs.cols` result in its first
/// entries. Returns the route taken.
///
/// # Errors
/// [`Error::DimensionMismatch`] if `lhs.cols != rhs.rows` or a slice is
/// shorter than its matrix (nothing is written then).
pub fn product_slices(
    lhs: Dims,
    rhs: Dims,
    a: &[Real],
    b: &[Real],
    c: &mut [Real],
) -> Result<Route> {
    let need_a = akantu_core::types::checked_len(lhs.rows, lhs.cols, "eigen::product lhs")?;
    let need_b = akantu_core::types::checked_len(rhs.rows, rhs.cols, "eigen::product rhs")?;
    for (context, need, got) in [
        ("eigen::product_slices: lhs", need_a, a.len()),
        ("eigen::product_slices: rhs", need_b, b.len()),
    ] {
        if got < need {
            return Err(Error::DimensionMismatch {
                context,
                expected: need,
                got,
            });
        }
    }
    let p = SliceProduct::new(product_shape(lhs, rhs))?;
    let got = c.len();
    if got < p.out_len() {
        return Err(Error::DimensionMismatch {
            context: "eigen::product: result",
            expected: p.out_len(),
            got,
        });
    }
    p.eval(a, b, c)?;
    Ok(p.route())
}

/// The [`SliceProduct`] of `lhs * rhs` (operand storage as in
/// [`product_slices`]): route, strides and lengths decided once, for loops
/// that evaluate the same product per element or quadrature point
/// ([`SliceProduct::eval`] gives the bits of [`product_slices`]).
///
/// # Errors
/// [`Error::DimensionMismatch`] if `lhs.cols != rhs.rows`.
pub fn prepare(lhs: Dims, rhs: Dims) -> Result<SliceProduct> {
    SliceProduct::new(product_shape(lhs, rhs))
}

/// `squaredNorm()` of `n` values: halving tree for a compile-time size within
/// the unrolling limit (`fixed`), sequential from the first term otherwise.
#[inline]
pub fn squared_norm(n: usize, fixed: bool, v: impl Fn(usize) -> Real) -> Real {
    eo::sum(n, eo::UNARY_TERM_COST, fixed, &|i| {
        let x = v(i);
        x * x
    })
}

/// `norm()` = `sqrt(squaredNorm())`.
#[inline]
pub fn norm(n: usize, fixed: bool, v: impl Fn(usize) -> Real) -> Real {
    squared_norm(n, fixed, v).sqrt()
}

/// Eigen `cross` of two 3-vectors (`(a1 b2 − a2 b1, a2 b0 − a0 b2, a0 b1 − a1 b0)`).
#[inline]
#[must_use]
pub fn cross(a: &[Real; 3], b: &[Real; 3]) -> [Real; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

/// Eigen `normalize()` / `normalized()`: divide every entry by
/// `sqrt(squaredNorm)` when the squared norm is positive (entries unchanged
/// otherwise).
#[inline]
pub fn normalize(v: &mut [Real], fixed: bool) {
    let z = squared_norm(v.len(), fixed, |i| v[i]);
    if z > 0.0 {
        let s = z.sqrt();
        for x in v.iter_mut() {
            *x /= s;
        }
    }
}

/// Maximal size of the stack-based dynamic LU helpers.
pub const LU_MAX: usize = 3;

/// In-place partial-pivot LU of the column-major `n × n` matrix `a`
/// (`akantu_core::tensor` `lu::factor`, Eigen `partial_lu_impl` unblocked):
/// returns the transpositions, their count and whether a zero pivot occurred.
fn lu_factor(a: &mut [Real], n: usize) -> ([usize; LU_MAX], usize, bool) {
    let mut transpositions = [0; LU_MAX];
    let mut nb_transpositions = 0;
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
        transpositions[k] = piv;
        if biggest != 0.0 {
            if piv != k {
                for j in 0..n {
                    a.swap(k + j * n, piv + j * n);
                }
                nb_transpositions += 1;
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
    (transpositions, nb_transpositions, zero_pivot)
}

/// Determinant of a **dynamic** `n × n` matrix (`n ≤ 3`, column-major `m`),
/// Eigen `MatrixXd::determinant()`: partial-pivot LU, `±∏ diag` sequential
/// (run-time size); `1` for `n = 0`.
///
/// Crate-internal: callers validate `n ≤ 3` and the
/// slice lengths first.
#[must_use]
pub(crate) fn lu_det(n: usize, m: &[Real]) -> Real {
    assert!(n <= LU_MAX, "lu_det: n = {n} > {LU_MAX}");
    if n == 0 {
        return 1.0;
    }
    let mut a = [0.0; LU_MAX * LU_MAX];
    a[..n * n].copy_from_slice(&m[..n * n]);
    let (_, nt, _) = lu_factor(&mut a[..n * n], n);
    let sign = if nt % 2 == 1 { -1.0 } else { 1.0 };
    sign * eo::prod(n, eo::READ_COST, false, &|i| a[i + i * n])
}

/// Inverse of a **dynamic** `n × n` matrix (`n ≤ 3`): Eigen
/// `MatrixXd::inverse()` = `PartialPivLU` solve with the identity
/// (unit-lower then upper triangular solves; for `n ≤ 4` one panel, so the
/// trailing-update terms never occur). Writes column-major `out`; returns
/// `true` for an exactly zero pivot (upstream then produces `inf`/`NaN`,
/// which is reproduced in `out`).
///
/// Crate-internal: callers validate `n ≤ 3` and the
/// slice lengths first.
pub(crate) fn lu_inverse(n: usize, m: &[Real], out: &mut [Real]) -> bool {
    assert!(n <= LU_MAX, "lu_inverse: n = {n} > {LU_MAX}");
    let mut a = [0.0; LU_MAX * LU_MAX];
    a[..n * n].copy_from_slice(&m[..n * n]);
    let (tr, _, zero_pivot) = lu_factor(&mut a[..n * n], n);
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
    // unit lower solve (one panel of width n ≤ 4)
    for i in 0..n {
        for j in 0..n {
            let bij = out[i + j * n];
            for r in i + 1..n {
                out[r + j * n] -= bij * a[r + i * n];
            }
        }
    }
    // upper solve (one panel, processed from the last row up)
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

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::tensor::MatrixX;

    /// Deterministic pseudo-random values (xorshift), no external crate.
    fn rng(seed: &mut u64) -> Real {
        *seed ^= *seed << 13;
        *seed ^= *seed >> 7;
        *seed ^= *seed << 17;
        ((*seed >> 11) as Real) / ((1u64 << 53) as Real) * 4.0 - 2.0
    }

    #[test]
    fn small_lu_matches_core_matrixx_bitwise() {
        let mut seed = 0x9e37_79b9_7f4a_7c15;
        for n in 1..=3 {
            for _ in 0..2000 {
                let m: Vec<Real> = (0..n * n).map(|_| rng(&mut seed)).collect();
                let mx = MatrixX::from_col_major(n, n, m.clone()).unwrap();
                assert_eq!(lu_det(n, &m).to_bits(), mx.det().unwrap().to_bits());
                let mut inv = [0.0; 9];
                let singular = lu_inverse(n, &m, &mut inv);
                assert!(!singular);
                let core = mx.inverse().unwrap();
                for (a, b) in inv[..n * n].iter().zip(core.as_slice()) {
                    assert_eq!(a.to_bits(), b.to_bits());
                }
            }
        }
        // pivoting and exact zero pivots
        let m = [0.0, 1.0, 1.0, 0.0];
        assert_eq!(lu_det(2, &m), -1.0);
        let mut inv = [0.0; 4];
        assert!(!lu_inverse(2, &m, &mut inv));
        assert_eq!(&inv, &[0.0, 1.0, 1.0, 0.0]);
        let s = [1.0, 2.0, 2.0, 4.0];
        assert!(lu_inverse(2, &s, &mut inv));
    }

    #[test]
    fn product_routes_follow_eigen_selection() {
        use akantu_core::eigen_gebp::GemvMatrix;
        use akantu_core::eigen_order::Reduction::{Sequential, Tree};
        let gemv_lhs = Route::Gemv {
            matrix: GemvMatrix::Lhs,
            kernel: Order::ColMajor,
        };
        // dnds (fixed 3×4) * X^T (dynamic 4×3): (Small, Large, Small) lazy, lhs fixed cols
        assert_eq!(
            product_route(Dims::fixed(3, 4), Dims::dynamic(3, 4).t()),
            Route::Coeff(Tree)
        );
        // dnds (fixed 3×20) * X^T (dynamic): GEMM, 3+3+20 >= 20 -> gebp
        assert_eq!(
            product_route(Dims::fixed(3, 20), Dims::dynamic(3, 20).t()),
            Route::Gebp { kc: 20 }
        );
        // dnds (fixed 3×8) * X^T (dynamic): GEMM -> lazy (14 < 20), tree
        assert_eq!(
            product_route(Dims::fixed(3, 8), Dims::dynamic(3, 8).t()),
            Route::Coeff(Tree)
        );
        // fully dynamic small: lazy sequential
        assert_eq!(
            product_route(Dims::dynamic(3, 8), Dims::dynamic(8, 3)),
            Route::Coeff(Sequential)
        );
        // dynamic matrix * dynamic vector: GEMV
        assert_eq!(
            product_route(Dims::dynamic(2, 4), Dims::new(4, 1, false, true)),
            gemv_lhs
        );
        // a transposed matrix operand selects the row-major GEMV kernel
        assert_eq!(
            product_route(Dims::dynamic(4, 2).t(), Dims::new(4, 1, false, true)),
            Route::Gemv {
                matrix: GemvMatrix::Lhs,
                kernel: Order::RowMajor
            }
        );
        // dynamic matrix * fixed vector of 3: (Large, One, Small) lazy with
        // dynamic lhs cols: sequential although the rhs fixes the depth
        assert_eq!(
            product_route(Dims::dynamic(2, 3), Dims::fixed(3, 1)),
            Route::Coeff(Sequential)
        );
        // GEMM with a single result column falls back to GEMV
        assert_eq!(
            product_route(Dims::dynamic(20, 20), Dims::dynamic(20, 1)),
            gemv_lhs
        );
        // fixed * fixed small: tree
        assert_eq!(
            product_route(Dims::fixed(3, 3), Dims::fixed(3, 3)),
            Route::Coeff(Tree)
        );
        // run-time 1×1 results: inner-product fallback in the lhs order
        let row = Dims::new(1, 20, false, true); // Matrix<Real, Dynamic, 20>
        assert_eq!(product_route(row, Dims::dynamic(20, 1)), Route::Dot(Tree));
        assert_eq!(product_route(row, Dims::fixed(20, 1)), Route::Dot(Tree));
        assert_eq!(
            product_route(Dims::dynamic(1, 20), Dims::fixed(20, 1)),
            Route::Dot(Sequential)
        );
        // the shape handed to the core carries both inner extents and orders
        let s = product_shape(Dims::new(3, 20, false, true), Dims::fixed(3, 20).t());
        assert_eq!(
            (s.rows, s.lhs_depth, s.rhs_depth, s.cols),
            (
                Extent::Dynamic(3),
                Extent::Fixed(20),
                Extent::Fixed(20),
                Extent::Fixed(3)
            )
        );
        assert_eq!(
            (s.lhs_order, s.rhs_order),
            (Order::ColMajor, Order::RowMajor)
        );
    }

    /// `Matrix<Real, Dynamic, 20>` (run-time
    /// 1 × 20) times a `MatrixXd` (20 × 1) or a `Matrix<Real, 20, 1>` reaches
    /// the GEMM→GEMV→dot resp. GEMV→dot fallback, whose inner product is
    /// reduced with the tree selected by the lhs's compile-time column count
    /// and then added to `+0`. Strict Eigen 3.4.0 (an independent executable
    /// built in the oracle image) and the core give
    /// `3ff0000000000001`; the former FE classification (sequential from
    /// `+0`) gave `3ff0000000000000`.
    #[test]
    fn dot_fallback_reduces_in_the_lhs_order_gf01() {
        let mut a = [0.0; 20];
        a[0] = 1.0;
        a[2] = 1e-16;
        a[3] = 1e-16;
        let b = [1.0; 20];
        let mut core = [0.0];
        eigen_gebp::product_into(
            &ProductShape::new(
                Extent::Dynamic(1),
                Extent::Fixed(20),
                Extent::Dynamic(20),
                Extent::Dynamic(1),
            ),
            &|_, k| a[k],
            &|k, _| b[k],
            &mut core,
        )
        .unwrap();
        assert_eq!(core[0].to_bits(), 0x3ff0_0000_0000_0001);
        let lhs = Dims::new(1, 20, false, true);
        for rhs in [Dims::dynamic(20, 1), Dims::fixed(20, 1)] {
            // slices: only the result entry is written
            let mut out = [f64::NAN; 2];
            let route = product_slices(lhs, rhs, &a, &b, &mut out).unwrap();
            assert_eq!(route, Route::Dot(eo::Reduction::Tree));
            assert_eq!(out[0].to_bits(), 0x3ff0_0000_0000_0001, "{rhs:?}");
            assert!(out[1].is_nan());
            // closures
            let mut out = [0.0];
            product(lhs, rhs, |_, k| a[k], |k, _| b[k], &mut out).unwrap();
            assert_eq!(out[0].to_bits(), 0x3ff0_0000_0000_0001, "{rhs:?}");
            // the public kernel built on it
            let mut out = [7.0];
            crate::kernels::interpolate(&a, lhs, &b, rhs, &mut out).unwrap();
            assert_eq!(out[0].to_bits(), 0x3ff0_0000_0000_0001, "{rhs:?}");
        }
        // run-time lhs columns: sequential, both small terms absorbed
        for rhs in [Dims::dynamic(20, 1), Dims::fixed(20, 1)] {
            let mut out = [0.0];
            let route = product_slices(Dims::dynamic(1, 20), rhs, &a, &b, &mut out).unwrap();
            assert_eq!(route, Route::Dot(eo::Reduction::Sequential));
            assert_eq!(out[0].to_bits(), 1.0f64.to_bits(), "{rhs:?}");
        }
    }

    /// Signed zeros: the fallback overwrites the destination with
    /// `+0 + dot`, so a dot product of `-0` terms (tree result `-0`) is
    /// written back as `+0` whatever the destination held, whereas the
    /// compile-time inner product of the same operands keeps `-0`.
    #[test]
    fn dot_fallback_signed_zero_write_back_gf01() {
        let a = [-1.0; 20];
        let b = [0.0; 20];
        let lhs = Dims::new(1, 20, false, true);
        for rhs in [Dims::dynamic(20, 1), Dims::fixed(20, 1)] {
            for stale in [-0.0, 5.0, f64::NAN] {
                let mut out = [stale];
                product_slices(lhs, rhs, &a, &b, &mut out).unwrap();
                assert!(
                    out[0] == 0.0 && out[0].is_sign_positive(),
                    "{rhs:?} {stale}"
                );
                let mut out = [stale];
                product(lhs, rhs, |_, k| a[k], |k, _| b[k], &mut out).unwrap();
                assert!(
                    out[0] == 0.0 && out[0].is_sign_positive(),
                    "{rhs:?} {stale}"
                );
            }
        }
        let mut out = [1.0];
        let route =
            product_slices(Dims::fixed(1, 20), Dims::fixed(20, 1), &a, &b, &mut out).unwrap();
        assert_eq!(route, Route::Coeff(eo::Reduction::Tree));
        assert!(out[0] == 0.0 && out[0].is_sign_negative());
    }

    #[test]
    fn products_match_the_core_kernels_and_slice_layouts() {
        let mut seed = 0x2545_f491_4f6c_dd1d;
        // transposed operands read their slices as the transpose's storage
        for (r, k, c) in [(3, 4, 5), (5, 20, 7), (1, 30, 6), (7, 129, 1)] {
            let at: Vec<Real> = (0..k * r).map(|_| rng(&mut seed)).collect();
            let bt: Vec<Real> = (0..c * k).map(|_| rng(&mut seed)).collect();
            let lhs = Dims::dynamic(k, r).t();
            let rhs = Dims::new(c, k, true, false).t();
            let mut got = vec![0.0; r * c];
            product_slices(lhs, rhs, &at, &bt, &mut got).unwrap();
            let mut want = vec![0.0; r * c];
            eigen_gebp::product_into(
                &ProductShape::new(
                    Extent::Dynamic(r),
                    Extent::Dynamic(k),
                    Extent::Dynamic(k),
                    Extent::Fixed(c),
                )
                .lhs_transposed()
                .rhs_transposed(),
                &|i, l| at[l + i * k],
                &|l, j| bt[j + l * c],
                &mut want,
            )
            .unwrap();
            for (g, w) in got.iter().zip(&want) {
                assert_eq!(g.to_bits(), w.to_bits(), "{r}x{k}x{c}");
            }
        }
    }

    #[test]
    fn product_size_errors_leave_the_output_untouched() {
        let a = [1.0; 6];
        let b = [1.0; 6];
        let mut out = [9.0; 4];
        // inner dimensions differ
        assert!(matches!(
            product_slices(Dims::dynamic(2, 3), Dims::dynamic(2, 3), &a, &b, &mut out),
            Err(Error::DimensionMismatch { .. })
        ));
        // result does not fit
        assert!(matches!(
            product_slices(
                Dims::dynamic(2, 3),
                Dims::dynamic(3, 2),
                &a,
                &b,
                &mut out[..3]
            ),
            Err(Error::DimensionMismatch { .. })
        ));
        // operand slice too short
        assert!(matches!(
            product_slices(
                Dims::dynamic(2, 3),
                Dims::dynamic(3, 2),
                &a[..5],
                &b,
                &mut out
            ),
            Err(Error::DimensionMismatch { .. })
        ));
        assert_eq!(out, [9.0; 4]);
        product_slices(Dims::dynamic(2, 3), Dims::dynamic(3, 2), &a, &b, &mut out).unwrap();
        assert_eq!(out, [3.0; 4]);
    }

    #[test]
    fn normalisation_and_cross() {
        let mut v = [3.0, 4.0];
        normalize(&mut v, false);
        assert_eq!(v, [0.6, 0.8]);
        let mut z = [0.0, 0.0];
        normalize(&mut z, true);
        assert_eq!(z, [0.0, 0.0]);
        assert_eq!(cross(&[1.0, 0.0, 0.0], &[0.0, 1.0, 0.0]), [0.0, 0.0, 1.0]);
    }
}
