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
//! | every other route | [`crate::eigen_gebp::eval_routed`] | unchanged |
//!
//! Each kernel computes the terms `fl(lhs(i, l) · rhs(l, j))` and adds them
//! in the order of the general evaluation (operands in the same position,
//! so NaN payloads propagate identically); loops over independent
//! coefficients may run in any order and the compiler may vectorise them
//! (no FMA: Rust never contracts). The differential tests compare every
//! route and size class with [`crate::eigen_gebp::product_into`] on special
//! values and payload NaNs.
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
        Ok(Self {
            shape,
            route: eigen_gebp::route(&shape),
            m,
            k,
            n,
            a_stride: strides(shape.lhs_order, m, k),
            b_stride: strides(shape.rhs_order, k, n),
            need_a,
            need_b,
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
    fn eval_sized(&self, a: &[Real], b: &[Real], out: &mut [Real]) {
        let (m, k, n) = (self.m, self.k, self.n);
        let (ai, al) = self.a_stride;
        let (bl, bj) = self.b_stride;
        match self.route {
            Route::Coeff(Reduction::Tree) if k <= COEFF_TREE_MAX_DEPTH => {
                coeff_tree(k, m, n, (a, ai, al), (b, bl, bj), out);
            }
            Route::Coeff(Reduction::Sequential) => {
                if ai == 1 {
                    coeff_seq_colmajor(m, k, n, a, (b, bl, bj), out);
                } else {
                    coeff_seq(m, k, n, (a, ai, al), (b, bl, bj), out);
                }
            }
            // GEMV over the left operand without depth blocking: `c = 0;
            // c = c + a(i, l)·x(l)`, then `out(i) = 0 + c` (both kernels)
            Route::Gemv {
                matrix: GemvMatrix::Lhs,
                ..
            } if k < GEMV_BLOCKING_MIN_DEPTH => {
                if ai == 1 {
                    gebp_shallow_colmajor(m, k, 1, a, (b, bl, bj), out);
                } else {
                    gebp_shallow(m, k, 1, (a, ai, al), (b, bl, bj), out);
                }
            }
            Route::Gebp { kc } if k < GEBP_PK && kc >= k => {
                if ai == 1 {
                    gebp_shallow_colmajor(m, k, n, a, (b, bl, bj), out);
                } else {
                    gebp_shallow(m, k, n, (a, ai, al), (b, bl, bj), out);
                }
            }
            route => eigen_gebp::eval_routed(
                route,
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
        let mut a = vec![0.0; 32768];
        let mut b = vec![0.0; 4096];
        let mut got = vec![0.0; 4096];
        let mut want = vec![0.0; 4096];
        let mut checked = 0usize;
        let mut routes = std::collections::BTreeSet::new();
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
}
