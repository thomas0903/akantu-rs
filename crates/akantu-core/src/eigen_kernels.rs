// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the evaluation order of the Eigen 3.4
// products written at the call sites of src/common/aka_types.hh and
// src/fe_engine/* (e.g. shape_lagrange_inline_impl.hh `B.transpose() * D * B`,
// fe_engine_template_tmpl.hh assembleFieldMatrix `Nt * N`). This file
// contains no Eigen code: it evaluates the routes selected by
// `eigen_gebp::route` (the maintained translation of Eigen's strict-build
// product dispatch) on operands stored in slices, with the same additions on
// the same operands in the same order.

//! Fixed-order product kernels on slices (D-008, D-036, D-122).
//!
//! [`crate::eigen_gebp::product_into`] reads every operand coefficient
//! through a closure and is generic over the operand expressions. Most
//! callers in the FE layer hold their operands in slices, column-major or
//! as the transpose of column-major storage. [`SliceProduct`] describes such
//! a product once — shape, route ([`crate::eigen_gebp::route`]), operand
//! strides and lengths — and [`SliceProduct::eval`] evaluates it with
//! kernels specialised per route:
//!
//! | Route | Kernel | Order of each coefficient |
//! |---|---|---|
//! | `Coeff(Tree)`, depth `k ≤ 27` | one monomorphised kernel per `k` (match on `k`) | halving tree of `k` terms (`eigen_order::redux_tree`), straight-line |
//! | `Coeff(Sequential)` | loop over the depth, columns of the result | `t0 + t1 + …` (`redux_seq`) |
//! | `Gebp { kc }` with depth `k < 8` (no peeled part) | loop over the depth | `0 + (((0 + t0) + t1) + …)`, the `gebp` micro-kernel's chain from `+0` and its write into the zeroed destination |
//! | `Gemv` over the left operand, depth `k < 128` (no column blocks) | the same loop, one result column | the same: `c = 0; c = c + a(i, l)·x(l)`, then `out(i) = 0 + c` |
//! | every other route | `eigen_gebp::eval_routed` (the general evaluation) | unchanged |
//!
//! Each kernel computes the terms `fl(lhs(i, l) · rhs(l, j))` and adds them
//! in the order of the general evaluation; loops over independent
//! coefficients may run in any order and the compiler may vectorise them
//! (no FMA: Rust never contracts). When two NaNs meet in one operation,
//! which of them comes out depends on the compiler (it may swap the
//! operands of a commutative operation, and x86-64 returns the first
//! operand's NaN), so the kernel and the general route can give NaNs of
//! different signs there: the case IEEE 754 leaves unspecified, allowed by
//! D-046 (rule of `crate::bitwise_testing::same`). Every other result,
//! including a single NaN's payload, has the same bits. The differential
//! tests compare every route and size class with
//! [`crate::eigen_gebp::product_into`] on special values and payload NaNs.
//!
//! Operand storage, as in `akantu_fe::eigen::product_slices`: the left
//! operand `lhs (rows × depth)` is stored column-major, or — when the shape
//! marks it row-major ([`crate::eigen_gebp::Order::RowMajor`], an upstream
//! `.transpose()`) — as the column-major `depth × rows` matrix it
//! transposes; likewise the right operand. The result is written
//! column-major into the first `rows · cols` entries of the output.

use crate::eigen_gebp::{
    self, GEBP_PK, GEMV_BLOCKING_MIN_DEPTH, GemvMatrix, Order, ProductShape, Reduction, Route,
};
use crate::eigen_order as eo;
use crate::error::{Error, Result};
use crate::types::Real;

/// Largest depth of a coefficient-based product reduced with the halving
/// tree (`3k + (k − 1) ≤ 110`, Eigen's unrolling limit).
pub const COEFF_TREE_MAX_DEPTH: usize = 27;

/// A product of slice operands with its route and strides decided once
/// (see the module documentation).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SliceProduct {
    shape: ProductShape,
    route: Route,
    m: usize,
    k: usize,
    n: usize,
    /// (row stride, column stride) of the logical left operand.
    a_stride: (usize, usize),
    /// (row stride, column stride) of the logical right operand.
    b_stride: (usize, usize),
    need_a: usize,
    need_b: usize,
    kernel: Kernel,
}

/// The kernel [`SliceProduct::eval`] runs, chosen once from the route,
/// the sizes and the left operand's layout.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kernel {
    /// `Coeff(Tree)`, depth ≤ 27.
    Tree,
    /// `Coeff(Sequential)`, column-major lhs with at least
    /// [`COLUMN_KERNEL_MIN_ROWS`] rows: whole result columns per step.
    SeqColumns,
    /// `Coeff(Sequential)`, depth ≤ 8: unrolled per coefficient.
    SeqUnrolled,
    /// `Coeff(Sequential)`, any depth, per coefficient.
    Seq,
    /// Chain from `+0` then `0 + c` (`Gebp` with depth < 8, unblocked
    /// `Gemv` over the lhs), column-major lhs with at least
    /// [`COLUMN_KERNEL_MIN_ROWS`] rows.
    ChainColumns,
    /// The same chain, depth < 8: unrolled per coefficient.
    ChainUnrolled,
    /// The same chain, any depth, per coefficient.
    Chain,
    /// Every other route: `eigen_gebp::eval_routed`.
    General,
}

/// Rows from which the column kernels (vectorisable across a result
/// column) are used instead of per-coefficient loops.
const COLUMN_KERNEL_MIN_ROWS: usize = 4;

/// Largest depth of the unrolled per-coefficient kernels.
const UNROLLED_MAX_DEPTH: usize = 8;

/// The kernel of `route` for an `m × k · k × n` product whose lhs has row
/// stride `ai`.
const fn kernel_of(route: Route, m: usize, k: usize, ai: usize) -> Kernel {
    let columns = ai == 1 && m >= COLUMN_KERNEL_MIN_ROWS;
    match route {
        Route::Coeff(Reduction::Tree) if k <= COEFF_TREE_MAX_DEPTH => Kernel::Tree,
        Route::Coeff(Reduction::Sequential) => {
            if columns {
                Kernel::SeqColumns
            } else if k <= UNROLLED_MAX_DEPTH {
                Kernel::SeqUnrolled
            } else {
                Kernel::Seq
            }
        }
        Route::Gemv {
            matrix: GemvMatrix::Lhs,
            ..
        } if k < GEMV_BLOCKING_MIN_DEPTH => {
            if columns {
                Kernel::ChainColumns
            } else if k < GEBP_PK {
                Kernel::ChainUnrolled
            } else {
                Kernel::Chain
            }
        }
        Route::Gebp { kc } if k < GEBP_PK && kc >= k => {
            if columns {
                Kernel::ChainColumns
            } else {
                Kernel::ChainUnrolled
            }
        }
        _ => Kernel::General,
    }
}

/// (row stride, column stride) of a logical `rows × cols` operand stored as
/// described in the module documentation.
const fn strides(order: Order, rows: usize, cols: usize) -> (usize, usize) {
    match order {
        Order::ColMajor => (1, rows),
        Order::RowMajor => (cols, 1),
    }
}

impl SliceProduct {
    /// Describe the product of shape `shape`.
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if the operands' inner dimensions differ
    /// or a size product overflows.
    #[inline]
    pub fn new(shape: ProductShape) -> Result<Self> {
        let (m, k, n) = (shape.rows.size(), shape.lhs_depth.size(), shape.cols.size());
        if shape.rhs_depth.size() != k {
            return Err(Error::DimensionMismatch {
                context: "SliceProduct (inner dimension)",
                expected: k,
                got: shape.rhs_depth.size(),
            });
        }
        let need_a = crate::types::checked_len(m, k, "SliceProduct lhs")?;
        let need_b = crate::types::checked_len(k, n, "SliceProduct rhs")?;
        crate::types::checked_len(m, n, "SliceProduct result")?;
        let route = eigen_gebp::route(&shape);
        let a_stride = strides(shape.lhs_order, m, k);
        Ok(Self {
            shape,
            route,
            m,
            k,
            n,
            a_stride,
            b_stride: strides(shape.rhs_order, k, n),
            need_a,
            need_b,
            kernel: kernel_of(route, m, k, a_stride.0),
        })
    }

    /// The route Eigen takes ([`crate::eigen_gebp::route`]).
    #[must_use]
    pub const fn route(&self) -> Route {
        self.route
    }

    /// The product's shape.
    #[must_use]
    pub const fn shape(&self) -> &ProductShape {
        &self.shape
    }

    /// Entries of the left operand's slice that are read (`rows · depth`).
    #[must_use]
    pub const fn lhs_len(&self) -> usize {
        self.need_a
    }

    /// Entries of the right operand's slice that are read (`depth · cols`).
    #[must_use]
    pub const fn rhs_len(&self) -> usize {
        self.need_b
    }

    /// Entries of the result (`rows · cols`).
    #[must_use]
    pub const fn out_len(&self) -> usize {
        self.m * self.n
    }

    /// `out[..rows·cols] = lhs · rhs` in Eigen's strict-build order (see the
    /// module documentation for the storage of `a` and `b`). Entries of
    /// `out` beyond the result are not touched.
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] if a slice is shorter than its matrix
    /// (nothing is written then).
    #[inline]
    pub fn eval(&self, a: &[Real], b: &[Real], out: &mut [Real]) -> Result<()> {
        let len = self.out_len();
        for (context, need, got) in [
            ("SliceProduct::eval: lhs", self.need_a, a.len()),
            ("SliceProduct::eval: rhs", self.need_b, b.len()),
            ("SliceProduct::eval: result", len, out.len()),
        ] {
            if got < need {
                return Err(Error::DimensionMismatch {
                    context,
                    expected: need,
                    got,
                });
            }
        }
        self.eval_sized(&a[..self.need_a], &b[..self.need_b], &mut out[..len]);
        Ok(())
    }

    /// [`SliceProduct::eval`] on slices of exactly the operand and result
    /// lengths.
    #[inline]
    fn eval_sized(&self, a: &[Real], b: &[Real], out: &mut [Real]) {
        let (m, k, n) = (self.m, self.k, self.n);
        let (ai, al) = self.a_stride;
        let (bl, bj) = self.b_stride;
        let (sa, sb) = ((a, ai, al), (b, bl, bj));
        // a GEMV result has one column
        let nc = if matches!(self.route, Route::Gemv { .. }) {
            1
        } else {
            n
        };
        match self.kernel {
            Kernel::Tree => coeff_tree(k, m, n, sa, sb, out),
            Kernel::SeqColumns => coeff_seq_colmajor(m, k, n, a, sb, out),
            Kernel::SeqUnrolled => coeff_seq_unrolled(k, m, n, sa, sb, out),
            Kernel::Seq => coeff_seq(m, k, n, sa, sb, out),
            Kernel::ChainColumns => gebp_shallow_colmajor(m, k, nc, a, sb, out),
            Kernel::ChainUnrolled => chain_unrolled(k, m, nc, sa, sb, out),
            Kernel::Chain => gebp_shallow(m, k, nc, sa, sb, out),
            Kernel::General => eigen_gebp::eval_routed(
                self.route,
                m,
                k,
                n,
                &|i, l| a[i * ai + l * al],
                &|l, j| b[l * bl + j * bj],
                out,
            ),
        }
    }
}

/// `out = lhs · rhs` for a product of shape `shape` whose operands are
/// stored in slices ([`SliceProduct::new`] then [`SliceProduct::eval`]).
/// Returns the route taken.
///
/// # Errors
/// [`Error::DimensionMismatch`] if the inner dimensions differ or a slice
/// is shorter than its matrix (nothing is written then).
#[inline]
pub fn product_slices(
    shape: &ProductShape,
    a: &[Real],
    b: &[Real],
    out: &mut [Real],
) -> Result<Route> {
    let p = SliceProduct::new(*shape)?;
    p.eval(a, b, out)?;
    Ok(p.route)
}

/// A strided operand: (data, row stride, column stride).
type Strided<'a> = (&'a [Real], usize, usize);

/// `Coeff(Tree)`: one kernel per depth so that the tree is straight-line.
fn coeff_tree(k: usize, m: usize, n: usize, a: Strided<'_>, b: Strided<'_>, out: &mut [Real]) {
    macro_rules! dispatch {
        ($($k:literal)*) => {
            match k {
                $($k => coeff_tree_k::<$k>(m, n, a, b, out),)*
                _ => unreachable!("coeff_tree: depth {k} outside 1..=27"),
            }
        };
    }
    dispatch!(1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27);
}

#[inline(always)]
fn coeff_tree_k<const K: usize>(
    m: usize,
    n: usize,
    (a, ai, al): Strided<'_>,
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for j in 0..n {
        let mut bc = [0.0; K];
        for (l, x) in bc.iter_mut().enumerate() {
            *x = b[l * bl + j * bj];
        }
        for i in 0..m {
            let mut t = [0.0; K];
            for (l, x) in t.iter_mut().enumerate() {
                *x = a[i * ai + l * al] * bc[l];
            }
            out[i + j * m] = eo::redux_tree(0, K, &|l| t[l], add);
        }
    }
}

/// `Coeff(Sequential)` with depth ≤ 8, one kernel per depth.
fn coeff_seq_unrolled(
    k: usize,
    m: usize,
    n: usize,
    a: Strided<'_>,
    b: Strided<'_>,
    out: &mut [Real],
) {
    macro_rules! dispatch {
        ($($k:literal)*) => {
            match k {
                $($k => coeff_seq_k::<$k>(m, n, a, b, out),)*
                _ => unreachable!("coeff_seq_unrolled: depth {k} outside 1..=8"),
            }
        };
    }
    dispatch!(1 2 3 4 5 6 7 8);
}

#[inline(always)]
fn coeff_seq_k<const K: usize>(
    m: usize,
    n: usize,
    (a, ai, al): Strided<'_>,
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for j in 0..n {
        let mut bc = [0.0; K];
        for (l, x) in bc.iter_mut().enumerate() {
            *x = b[l * bl + j * bj];
        }
        for i in 0..m {
            // r = t0; r = r + t_l
            let mut r = a[i * ai] * bc[0];
            for l in 1..K {
                r += a[i * ai + l * al] * bc[l];
            }
            out[i + j * m] = r;
        }
    }
}

/// The chain from `+0` (`Gebp` depth < 8 or unblocked `Gemv`) with depth
/// < 8, one kernel per depth.
fn chain_unrolled(k: usize, m: usize, n: usize, a: Strided<'_>, b: Strided<'_>, out: &mut [Real]) {
    macro_rules! dispatch {
        ($($k:literal)*) => {
            match k {
                $($k => chain_k::<$k>(m, n, a, b, out),)*
                _ => unreachable!("chain_unrolled: depth {k} outside 1..=7"),
            }
        };
    }
    dispatch!(1 2 3 4 5 6 7);
}

#[inline(always)]
fn chain_k<const K: usize>(
    m: usize,
    n: usize,
    (a, ai, al): Strided<'_>,
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for j in 0..n {
        let mut bc = [0.0; K];
        for (l, x) in bc.iter_mut().enumerate() {
            *x = b[l * bl + j * bj];
        }
        for i in 0..m {
            // c = 0; c = c + t_l; dst = 0 + c
            let mut c = 0.0;
            for (l, &bl_j) in bc.iter().enumerate() {
                c += a[i * ai + l * al] * bl_j;
            }
            let mut r = 0.0;
            r += c;
            out[i + j * m] = r;
        }
    }
}

/// `Coeff(Sequential)`: `r = t0; r = r + t_l`.
fn coeff_seq(
    m: usize,
    k: usize,
    n: usize,
    (a, ai, al): Strided<'_>,
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for j in 0..n {
        for i in 0..m {
            let mut r = a[i * ai] * b[j * bj];
            for l in 1..k {
                r += a[i * ai + l * al] * b[l * bl + j * bj];
            }
            out[i + j * m] = r;
        }
    }
}

/// `Coeff(Sequential)` with a column-major left operand: the coefficients
/// of a result column advance together over the depth (each still
/// `r = t0; r = r + t_l`).
fn coeff_seq_colmajor(
    m: usize,
    k: usize,
    n: usize,
    a: &[Real],
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for (j, col) in out.chunks_exact_mut(m).enumerate().take(n) {
        let b0 = b[j * bj];
        for (o, &x) in col.iter_mut().zip(&a[..m]) {
            *o = x * b0;
        }
        for l in 1..k {
            let bl_j = b[l * bl + j * bj];
            for (o, &x) in col.iter_mut().zip(&a[l * m..(l + 1) * m]) {
                *o += x * bl_j;
            }
        }
    }
}

/// `Gebp` with depth `k < 8` (no peeled part, one depth block): every
/// coefficient is `c = 0; c = c + t_l` (both micro-kernel paths reduce to
/// this when nothing is peeled), then `dst = 0 + c` (`dst.setZero()`,
/// `r = c·α + r`).
fn gebp_shallow(
    m: usize,
    k: usize,
    n: usize,
    (a, ai, al): Strided<'_>,
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for j in 0..n {
        for i in 0..m {
            let mut c = 0.0;
            for l in 0..k {
                c += a[i * ai + l * al] * b[l * bl + j * bj];
            }
            let mut r = 0.0;
            r += c;
            out[i + j * m] = r;
        }
    }
}

/// [`gebp_shallow`] with a column-major left operand (the chains of a result
/// column advance together over the depth).
fn gebp_shallow_colmajor(
    m: usize,
    k: usize,
    n: usize,
    a: &[Real],
    (b, bl, bj): Strided<'_>,
    out: &mut [Real],
) {
    for (j, col) in out.chunks_exact_mut(m).enumerate().take(n) {
        col.fill(0.0); // c = 0
        for l in 0..k {
            let bl_j = b[l * bl + j * bj];
            for (o, &x) in col.iter_mut().zip(&a[l * m..(l + 1) * m]) {
                *o += x * bl_j; // c = c + t_l
            }
        }
        for o in col.iter_mut() {
            let mut r = 0.0;
            r += *o; // dst = 0 + c
            *o = r;
        }
    }
}

#[inline(always)]
fn add(x: Real, y: Real) -> Real {
    x + y
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::bitwise_testing::{Values, same};
    use crate::eigen_gebp::Extent::{Dynamic as D, Fixed as F};
    use crate::eigen_gebp::{Extent, product_into};

    /// Every shape class up to 12 × 12 × 12 plus the FE sizes (mass and
    /// stiffness products of up to 60 rows), all compile-time / run-time
    /// flags and both storage orders of each operand, against
    /// `product_into` on the same logical operands.
    #[test]
    fn slice_products_match_product_into_bitwise() {
        let mut v = Values::new(0x51_1ce5);
        let mut sizes: Vec<(usize, usize, usize)> = Vec::new();
        for m in 1..=12 {
            for k in 1..=12 {
                for n in 1..=12 {
                    if (m * k * n) % 3 == 0 || m.max(k).max(n) <= 6 {
                        sizes.push((m, k, n));
                    }
                }
            }
        }
        sizes.extend([
            (12, 6, 6),
            (12, 6, 12),
            (30, 6, 6),
            (30, 6, 30),
            (24, 6, 24),
            (60, 6, 60),
            (60, 6, 6),
            (30, 3, 30),
            (24, 3, 24),
            (60, 3, 60),
            (18, 3, 18),
            (18, 3, 3),
            (3, 27, 3),
            (1, 27, 1),
            (2, 28, 2),
            (3, 10, 3),
            (3, 20, 3),
            (6, 9, 6),
            (8, 8, 8),
            (50, 7, 50),
            (49, 8, 2),
        ]);
        // the GEMV kernel's depth-block boundary (unblocked below 128)
        for m in [1, 3, 40] {
            for k in [127, 128, 129] {
                sizes.push((m, k, 1));
            }
        }
        let mut a = vec![0.0; 32768];
        let mut b = vec![0.0; 4096];
        let mut got = vec![0.0; 4096];
        let mut want = vec![0.0; 4096];
        let mut checked = 0usize;
        let mut routes = std::collections::BTreeSet::new();
        let mut kernels = std::collections::BTreeSet::new();
        let mut gemv_depths = std::collections::BTreeSet::new();
        for &(m, k, n) in &sizes {
            for flags in 0..64u32 {
                let ext = |sz: usize, bit: u32| -> Extent {
                    if flags & (1 << bit) != 0 {
                        F(sz)
                    } else {
                        D(sz)
                    }
                };
                let mut shape = ProductShape::new(ext(m, 0), ext(k, 1), ext(k, 2), ext(n, 3));
                if flags & 16 != 0 {
                    shape = shape.lhs_transposed();
                }
                if flags & 32 != 0 {
                    shape = shape.rhs_transposed();
                }
                let p = SliceProduct::new(shape).unwrap();
                routes.insert(format!("{:?}", p.route()));
                if matches!(p.route(), Route::Gemv { .. }) {
                    gemv_depths.insert(k);
                }
                kernels.insert(format!("{:?}", p.kernel));
                let payload = checked % 4 == 3;
                if payload {
                    let kk = 1 + v.next_u64() % (m * k + k * n) as u64;
                    v.payload_at(kk);
                }
                v.fill(&mut a[..m * k]);
                v.fill(&mut b[..k * n]);
                v.default_mode();
                let (ai, al) = strides(shape.lhs_order, m, k);
                let (bl, bj) = strides(shape.rhs_order, k, n);
                got[..m * n + 1].fill(7.0);
                p.eval(&a, &b, &mut got[..m * n + 1]).unwrap();
                assert_eq!(got[m * n], 7.0, "write past the result");
                product_into(
                    &shape,
                    &|i, l| a[i * ai + l * al],
                    &|l, j| b[l * bl + j * bj],
                    &mut want[..m * n],
                )
                .unwrap();
                for (x, y) in got[..m * n].iter().zip(&want[..m * n]) {
                    if payload {
                        assert_eq!(x.to_bits(), y.to_bits(), "{shape:?} {:?}", p.route());
                    } else {
                        assert!(same(*x, *y), "{shape:?} {:?}: {x:e} {y:e}", p.route());
                    }
                }
                checked += 1;
            }
        }
        assert!(checked > 20_000, "{checked}");
        // every specialised kernel and the fallback were exercised
        for r in [
            "Coeff(Tree)",
            "Coeff(Sequential)",
            "Gebp { kc: 3 }",
            "Gebp { kc: 6 }",
            "Gebp { kc: 8 }",
            "Outer",
        ] {
            assert!(routes.contains(r), "{r} not covered: {routes:?}");
        }
        assert!(routes.iter().any(|r| r.starts_with("Gemv")));
        assert!(
            gemv_depths.contains(&127) && gemv_depths.contains(&128) && gemv_depths.contains(&129)
        );
        for k in [
            "Tree",
            "SeqColumns",
            "SeqUnrolled",
            "Seq",
            "ChainColumns",
            "ChainUnrolled",
            "Chain",
            "General",
        ] {
            assert!(kernels.contains(k), "kernel {k} not covered: {kernels:?}");
        }
        assert!(routes.iter().any(|r| r.starts_with("Dot")));
    }

    /// Repeated draws of the hottest FE classes (mass `Nᵀ ρ N`, stiffness
    /// `Bᵀ D` and `(Bᵀ D) B`, Jacobians, shape derivatives).
    #[test]
    fn fe_size_classes_many_draws() {
        let mut v = Values::new(0xfe_c1a5);
        let classes = [
            ProductShape::dynamic(30, 3, 30),
            ProductShape::dynamic(12, 3, 12),
            ProductShape::dynamic(30, 6, 6).lhs_transposed(),
            ProductShape::dynamic(30, 6, 30),
            ProductShape::dynamic(18, 3, 3).lhs_transposed(),
            ProductShape::dynamic(18, 3, 18),
            ProductShape::new(F(3), F(10), D(10), D(3)).rhs_transposed(),
            ProductShape::new(F(3), F(4), D(4), D(3)).rhs_transposed(),
            ProductShape::dynamic(3, 3, 10),
            ProductShape::dynamic(3, 10, 3).rhs_transposed(),
            ProductShape::new(D(3), F(8), F(8), F(3)).rhs_transposed(),
        ];
        let mut a = vec![0.0; 1024];
        let mut b = vec![0.0; 1024];
        let mut got = vec![0.0; 1024];
        let mut want = vec![0.0; 1024];
        for shape in classes {
            let p = SliceProduct::new(shape).unwrap();
            let (m, k, n) = (shape.rows.size(), shape.lhs_depth.size(), shape.cols.size());
            let (ai, al) = strides(shape.lhs_order, m, k);
            let (bl, bj) = strides(shape.rhs_order, k, n);
            for draw in 0..2_000 {
                let payload = draw % 5 == 4;
                if payload {
                    let kk = 1 + v.next_u64() % (m * k + k * n) as u64;
                    v.payload_at(kk);
                }
                v.fill(&mut a[..m * k]);
                v.fill(&mut b[..k * n]);
                v.default_mode();
                p.eval(&a, &b, &mut got).unwrap();
                product_into(
                    &shape,
                    &|i, l| a[i * ai + l * al],
                    &|l, j| b[l * bl + j * bj],
                    &mut want[..m * n],
                )
                .unwrap();
                for (x, y) in got[..m * n].iter().zip(&want[..m * n]) {
                    if payload {
                        assert_eq!(x.to_bits(), y.to_bits(), "{shape:?}");
                    } else {
                        assert!(same(*x, *y), "{shape:?}: {x:e} {y:e}");
                    }
                }
            }
        }
    }

    #[test]
    fn errors_leave_the_output_untouched() {
        let shape = ProductShape::dynamic(2, 3, 2);
        let p = SliceProduct::new(shape).unwrap();
        let mut out = [9.0; 4];
        assert!(p.eval(&[1.0; 5], &[1.0; 6], &mut out).is_err());
        assert!(p.eval(&[1.0; 6], &[1.0; 5], &mut out).is_err());
        assert!(p.eval(&[1.0; 6], &[1.0; 6], &mut out[..3]).is_err());
        assert_eq!(out, [9.0; 4]);
        assert!(SliceProduct::new(ProductShape::new(D(2), D(3), D(2), D(2))).is_err());
        assert_eq!(
            product_slices(&shape, &[1.0; 6], &[1.0; 6], &mut out).unwrap(),
            Route::Coeff(Reduction::Sequential)
        );
        assert_eq!(out, [3.0; 4]);
    }

    /// The token of a serial executor in `Precision::Fast` (the only way to
    /// obtain one); a bitwise executor has none.
    fn fast_token() -> crate::exec::FastToken {
        use crate::exec::{Exec, ExecPolicy, Precision};
        assert!(Exec::serial().fast_token().is_none());
        Exec::new(ExecPolicy::serial().with_precision(Precision::Fast))
            .unwrap()
            .fast_token()
            .unwrap()
    }

    /// The fast-mode variants refuse bad sizes with errors and write nothing.
    #[test]
    fn fast_mode_size_errors() {
        let fast = fast_token();
        let a = [1.0; 130];
        let b = [1.0; 130];
        let mut out = [7.0; 4];
        assert!(fast_mode::gemm_colmajor(fast, 1, 65, 1, &a, &b, &mut out).is_err());
        assert!(fast_mode::gemm_colmajor(fast, 2, 3, 2, &a[..5], &b, &mut out).is_err());
        assert!(fast_mode::gemm_colmajor(fast, 2, 3, 2, &a, &b[..5], &mut out).is_err());
        assert!(fast_mode::gemm_colmajor(fast, 2, 3, 2, &a, &b, &mut out[..3]).is_err());
        assert!(fast_mode::gemm_tn(fast, 2, 3, 2, &a[..5], &b, &mut out).is_err());
        assert!(fast_mode::gemm_tn(fast, 2, 3, 2, &a, &b, &mut out[..3]).is_err());
        assert_eq!(out, [7.0; 4]);
        fast_mode::gemm_tn(fast, 2, 3, 2, &a, &b, &mut out).unwrap();
        assert_eq!(out, [3.0; 4]);
    }

    /// The fast-mode variants stay within the summation error bound
    /// `γ_k Σ |a_il b_lj|` of the exact-order kernels (both satisfy it, so
    /// they differ by at most `2 γ_k Σ |a_il b_lj|`).
    #[test]
    fn fast_mode_variants_within_the_summation_bound() {
        let token = fast_token();
        let mut v = Values::new(0xfa57_0de5);
        let u = Real::EPSILON / 2.0;
        let mut a = vec![0.0; 64 * 64];
        let mut at = vec![0.0; 64 * 64];
        let mut b = vec![0.0; 64 * 64];
        let mut fast = vec![0.0; 64 * 64];
        let mut exact = vec![0.0; 64 * 64];
        for &(m, k, n) in &[
            (3, 3, 3),
            (30, 6, 30),
            (30, 6, 6),
            (3, 10, 3),
            (6, 6, 1),
            (24, 3, 24),
            (5, 27, 5),
            (2, 64, 2),
        ] {
            for _ in 0..300 {
                for x in a[..m * k].iter_mut().chain(&mut b[..k * n]) {
                    *x = v.finite();
                }
                for i in 0..m {
                    for l in 0..k {
                        at[l + i * k] = a[i + l * m];
                    }
                }
                let gamma = (k as Real) * u / (1.0 - (k as Real) * u);
                let p = SliceProduct::new(ProductShape::dynamic(m, k, n)).unwrap();
                p.eval(&a, &b, &mut exact).unwrap();
                fast_mode::gemm_colmajor(token, m, k, n, &a, &b, &mut fast).unwrap();
                let mut fast_tn = vec![0.0; m * n];
                fast_mode::gemm_tn(token, m, k, n, &at, &b, &mut fast_tn).unwrap();
                for j in 0..n {
                    for i in 0..m {
                        let abs: Real = (0..k).map(|l| (a[i + l * m] * b[l + j * k]).abs()).sum();
                        let bound = 2.0 * gamma * abs * (1.0 + 4.0 * u) + Real::MIN_POSITIVE;
                        let e = exact[i + j * m];
                        for f in [fast[i + j * m], fast_tn[i + j * m]] {
                            assert!(
                                (f - e).abs() <= bound,
                                "{m}x{k}x{n}: {f:e} vs {e:e}, bound {bound:e}"
                            );
                        }
                    }
                }
            }
        }
    }
}

/// Order-free variants of the hottest product classes, for the fast mode
/// (D-122: **not** bitwise; off by default). Unstable and hidden from the
/// documentation. Every entry point takes the [`crate::exec::FastToken`]
/// that only [`crate::exec::Exec::fast_token`] hands out, in
/// `Precision::Fast` (D-123), so a bitwise run cannot call them. Nothing in
/// the library calls them yet; the callers they could serve are listed in
/// `docs/benchmarks/perf-campaign/small-products/README.md`.
///
/// The variants reassociate each coefficient's sum into independent
/// partial sums (four accumulators over the depth, combined pairwise), so
/// the compiler can keep several additions in flight and vectorise across
/// the depth; they use no FMA (`mul_add` without the `fma` target feature
/// compiles to a slow software routine on the x86-64 baseline). Error
/// bound of each coefficient (Higham, Theorem 3.1 with any summation
/// order): `|ŝ − s| ≤ γ_k Σ_l |a_il b_lj|`, `γ_k = k·u/(1 − k·u)`, the
/// same bound as the exact-order kernels; the results differ from them in
/// the last bits.
#[doc(hidden)]
pub mod fast_mode {
    use crate::error::{Error, Result};
    use crate::exec::FastToken;
    use crate::types::Real;

    /// Largest depth of [`gemm_colmajor`] (its row buffer).
    pub const MAX_DEPTH: usize = 64;

    /// Dot product `Σ a_l·b_l` over the common length of `a` and `b`, with
    /// four partial sums (order-free).
    #[inline]
    #[must_use]
    pub fn dot(_fast: FastToken, a: &[Real], b: &[Real]) -> Real {
        dot4(a, b)
    }

    /// [`dot`] without the token, for the products of this module.
    #[inline]
    fn dot4(a: &[Real], b: &[Real]) -> Real {
        let n = a.len().min(b.len());
        let (a, b) = (&a[..n], &b[..n]);
        let mut acc = [0.0; 4];
        let (ca, ra) = a.as_chunks::<4>();
        let (cb, rb) = b.as_chunks::<4>();
        for (x, y) in ca.iter().zip(cb) {
            for r in 0..4 {
                acc[r] += x[r] * y[r];
            }
        }
        let mut tail = 0.0;
        for (x, y) in ra.iter().zip(rb) {
            tail += x * y;
        }
        ((acc[0] + acc[1]) + (acc[2] + acc[3])) + tail
    }

    /// `Error::DimensionMismatch` unless the operand and result slices
    /// hold `m·k`, `k·n` and `m·n` values.
    fn check(
        context: &'static str,
        m: usize,
        k: usize,
        n: usize,
        a: &[Real],
        b: &[Real],
        out: &[Real],
    ) -> Result<()> {
        let need_a = crate::types::checked_len(m, k, context)?;
        let need_b = crate::types::checked_len(k, n, context)?;
        let need_o = crate::types::checked_len(m, n, context)?;
        for (need, got) in [(need_a, a.len()), (need_b, b.len()), (need_o, out.len())] {
            if got < need {
                return Err(Error::DimensionMismatch {
                    context,
                    expected: need,
                    got,
                });
            }
        }
        Ok(())
    }

    /// `out = lhs · rhs` for column-major `m × k` and `k × n` operands
    /// (`out` column-major `m × n`), each coefficient a [`dot`] of a row of
    /// `lhs` (gathered) and a column of `rhs`. Order-free.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` if a slice is shorter than its matrix or
    /// `k >` [`MAX_DEPTH`] (nothing is written then).
    pub fn gemm_colmajor(
        _fast: FastToken,
        m: usize,
        k: usize,
        n: usize,
        a: &[Real],
        b: &[Real],
        out: &mut [Real],
    ) -> Result<()> {
        if k > MAX_DEPTH {
            return Err(Error::DimensionMismatch {
                context: "fast_mode::gemm_colmajor: depth",
                expected: MAX_DEPTH,
                got: k,
            });
        }
        check("fast_mode::gemm_colmajor", m, k, n, a, b, out)?;
        let mut row = [0.0; MAX_DEPTH];
        for i in 0..m {
            for (l, x) in row.iter_mut().enumerate().take(k) {
                *x = a[i + l * m];
            }
            for j in 0..n {
                out[i + j * m] = dot4(&row[..k], &b[j * k..(j + 1) * k]);
            }
        }
        Ok(())
    }

    /// `out = lhsᵀ · rhs` with `lhs` stored column-major `k × m` (the
    /// upstream `B.transpose() * D` operand): each coefficient is a [`dot`]
    /// of two contiguous columns. Order-free.
    ///
    /// # Errors
    /// `Error::DimensionMismatch` if a slice is shorter than its matrix
    /// (nothing is written then).
    pub fn gemm_tn(
        _fast: FastToken,
        m: usize,
        k: usize,
        n: usize,
        at: &[Real],
        b: &[Real],
        out: &mut [Real],
    ) -> Result<()> {
        check("fast_mode::gemm_tn", m, k, n, at, b, out)?;
        for j in 0..n {
            let bj = &b[j * k..(j + 1) * k];
            for i in 0..m {
                out[i + j * m] = dot4(&at[i * k..(i + 1) * k], bj);
            }
        }
        Ok(())
    }
}
