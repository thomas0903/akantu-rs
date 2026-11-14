// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the evaluation order of the Eigen 3.4
// expressions used by src/common/aka_types.hh (Vector/Matrix aliases over
// Eigen). This file contains no Eigen code: it describes, in our own words,
// the order in which Eigen 3.4.0 evaluates reductions and products when
// compiled without vectorisation (`-DEIGEN_DONT_VECTORIZE`, the oracle's
// `strict` configuration, docs/ORACLE_BUILD.md), as established by reading
// the Eigen 3.4.0 headers shipped in the oracle image and verified bitwise
// against compiled Eigen (crates/akantu-linalg/tests/fixtures/dense/).

//! Eigen-compatible evaluation order (decision records D-008, D-011).
//!
//! Floating-point addition is not associative, so reproducing upstream's
//! results bit for bit requires reproducing *which* order Eigen uses. With
//! vectorisation disabled, Eigen 3.4.0 uses exactly three summation orders:
//!
//! | Order | Where (Eigen 3.4 internals) | Function |
//! |---|---|---|
//! | **halving tree** | complete unrolling of a reduction over a *compile-time* size whose cost `size·coeff_cost + (size−1)·functor_cost ≤ 110` (`redux_novec_unroller`) | [`redux_tree`] |
//! | **sequential from the first term** | reduction over a run-time size, or a compile-time size above the unrolling limit (`redux_impl<…, NoUnrolling>`): `r = t0; r = r ⊕ t1; …` | [`redux_seq`] |
//! | **sequential from `+0`** | matrix–vector kernels (`general_matrix_vector_product`, GEMV): `acc += 0; acc = t_j; // IEEE addition is commutative: same bits as `t + acc` …`, then `res = acc·1 + 0` | [`sum_from_zero`] |
//!
//! The halving tree splits `[start, start+len)` into `[start, start+len/2)`
//! and the rest, recursively: `t0 + (t1 + t2)` for 3 terms,
//! `(t0 + t1) + (t2 + t3)` for 4, `(t0 + (t1 + t2)) + (t3 + (t4 + t5))` for 6.
//! Linear indices of 2-D expressions are column-major.
//!
//! The two sequential orders differ only in the sign of an exactly-zero
//! result (`-0` is preserved from the first term, `+0 + -0 = +0`).
//!
//! Which order a matrix product uses depends on Eigen's product-type
//! selection ([`product_kind`]) on compile-time sizes: "small" fixed
//! products are coefficient-based (each coefficient is a reduction over the
//! inner dimension), matrix–vector products with a large dimension use GEMV
//! (sequential from `+0`, blocked over depths `≥ 128`), and `GemmProduct`s
//! (fixed or dynamic) are coefficient-based when `rows + cols + depth < 20`
//! and use Eigen's blocked kernel (`gebp`) otherwise. The complete product
//! dispatch and the GEMV/`gebp` kernel orders are in
//! [`crate::eigen_gebp`] (`docs/NUMERICAL_VALIDATION.md`).
//!
//! The depth that selects the product type is compile-time if **either**
//! operand fixes it, but a coefficient-wise reduction
//! `(lhs.row(i)ᵀ ∘ rhs.col(j)).sum()` has the compile-time length of its
//! **left** operand only (an element-wise binary expression takes its
//! compile-time sizes from its first argument): the halving tree applies
//! only when the left operand's column count is a compile-time constant
//! ([`Reduction::of_sum`], [`product_coeff`]). A run-time `C` times a
//! fixed vector therefore reduces sequentially.

/// Eigen `EIGEN_UNROLLING_LIMIT` (`Eigen/src/Core/arch/Default/Settings.h`).
pub const UNROLLING_LIMIT: usize = 110;

/// Eigen `EIGEN_CACHEFRIENDLY_PRODUCT_THRESHOLD`: a compile-time product
/// dimension `≥ 8` counts as "large".
pub const CACHEFRIENDLY_PRODUCT_THRESHOLD: usize = 8;

/// Eigen `EIGEN_GEMM_TO_COEFFBASED_THRESHOLD`: `GemmProduct`s (compile-time
/// or run-time sizes) with `depth + rows + cols < 20` (and `depth > 0`) are
/// evaluated coefficient-wise.
pub const GEMM_TO_COEFFBASED_THRESHOLD: usize = 20;

/// Read cost of a plain (or mapped / transposed / block) coefficient.
pub const READ_COST: usize = 1;
/// Cost of a scalar multiplication or addition (Eigen `NumTraits<double>`).
pub const OP_COST: usize = 1;
/// Coefficient read cost of `a.cwiseProduct(b)` over plain operands.
pub const PRODUCT_TERM_COST: usize = 2 * READ_COST + OP_COST;
/// Coefficient read cost of `a.cwiseAbs2()` / `a.cwiseAbs()`.
pub const UNARY_TERM_COST: usize = READ_COST + OP_COST;

/// Halving-tree reduction of `t(start..start+len)` with `op` (Eigen's
/// complete unrolling). `len` must be `≥ 1`.
#[inline]
pub fn redux_tree(
    start: usize,
    len: usize,
    t: &impl Fn(usize) -> f64,
    op: fn(f64, f64) -> f64,
) -> f64 {
    debug_assert!(len >= 1);
    if len == 1 {
        t(start)
    } else {
        let half = len / 2;
        op(
            redux_tree(start, half, t, op),
            redux_tree(start + half, len - half, t, op),
        )
    }
}

/// Sequential reduction from the first term: `r = t0; r = op(r, t_i)`.
/// `len` must be `≥ 1`.
#[inline]
pub fn redux_seq(len: usize, t: &impl Fn(usize) -> f64, op: fn(f64, f64) -> f64) -> f64 {
    debug_assert!(len >= 1);
    let mut r = t(0);
    for i in 1..len {
        r = op(r, t(i));
    }
    r
}

/// Sequential sum starting from `+0` (GEMV kernels): `acc = 0; acc = t_j + acc`.
#[inline]
pub fn sum_from_zero(len: usize, t: &impl Fn(usize) -> f64) -> f64 {
    let mut acc = 0.0;
    for j in 0..len {
        acc += t(j); // IEEE addition is commutative: same bits as `t + acc`
    }
    acc
}

fn add(a: f64, b: f64) -> f64 {
    a + b
}

fn mul(a: f64, b: f64) -> f64 {
    a * b
}

/// Order of a non-empty reduction (`sum()`, `prod()`, a product
/// coefficient) in the strict build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reduction {
    /// Halving tree (complete unrolling, [`redux_tree`]).
    Tree,
    /// Sequential from the first term ([`redux_seq`]).
    Sequential,
}

impl Reduction {
    /// Order of the reduction of `len ≥ 1` terms whose coefficient read cost
    /// is `coeff_cost` (binary functor cost [`OP_COST`]), over an expression
    /// of **compile-time** size `len` (`fixed = true`) or run-time size:
    /// the tree iff `fixed` and `len·coeff_cost + (len − 1)·OP_COST ≤`
    /// [`UNROLLING_LIMIT`].
    #[must_use]
    pub const fn of_sum(len: usize, coeff_cost: usize, fixed: bool) -> Self {
        if fixed && len >= 1 && len * coeff_cost + (len - 1) * OP_COST <= UNROLLING_LIMIT {
            Reduction::Tree
        } else {
            Reduction::Sequential
        }
    }

    /// Order of one product coefficient `Σ_k lhs(i, k)·rhs(k, j)` of depth
    /// `depth` (coefficient-based / inner products and the run-time inner
    /// product fallback): the expression reduced is
    /// `lhs.row(i)ᵀ ∘ rhs.col(j)`, whose compile-time length is the **left**
    /// operand's column count, so `lhs_cols_fixed` — not whether the depth
    /// is compile-time for the product-type selection (`rhs` may fix it) —
    /// decides.
    #[must_use]
    pub const fn of_product(depth: usize, lhs_cols_fixed: bool) -> Self {
        Self::of_sum(depth, PRODUCT_TERM_COST, lhs_cols_fixed)
    }

    /// Reduce `t(0..len)` with `op` in this order (`len ≥ 1`).
    #[inline]
    pub fn reduce(self, len: usize, t: &impl Fn(usize) -> f64, op: fn(f64, f64) -> f64) -> f64 {
        match self {
            Reduction::Tree => redux_tree(0, len, t, op),
            Reduction::Sequential => redux_seq(len, t, op),
        }
    }

    /// Sum `t(0..len)` in this order; empty → `+0`.
    #[inline]
    pub fn sum(self, len: usize, t: &impl Fn(usize) -> f64) -> f64 {
        if len == 0 {
            0.0
        } else {
            self.reduce(len, t, add)
        }
    }
}

/// Eigen `DenseBase::sum()` of `len` terms whose coefficient read cost is
/// `coeff_cost`, for an expression of **compile-time** size `len`
/// (`fixed = true`) or run-time size (`fixed = false`). Empty → `+0`.
#[inline]
pub fn sum(len: usize, coeff_cost: usize, fixed: bool, t: &impl Fn(usize) -> f64) -> f64 {
    Reduction::of_sum(len, coeff_cost, fixed).sum(len, t)
}

/// Eigen `DenseBase::prod()` (empty → `1`), same order rules as [`sum`].
#[inline]
pub fn prod(len: usize, coeff_cost: usize, fixed: bool, t: &impl Fn(usize) -> f64) -> f64 {
    if len == 0 {
        return 1.0;
    }
    Reduction::of_sum(len, coeff_cost, fixed).reduce(len, t, mul)
}

/// Size category of one product dimension (Eigen `product_size_category`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SizeClass {
    /// Compile-time size 1.
    One,
    /// Compile-time size `< 8` (including 0).
    Small,
    /// Compile-time size `≥ 8`, or run-time (dynamic) size.
    Large,
}

impl SizeClass {
    /// Category of a compile-time size `n` (`fixed`) or a dynamic dimension.
    #[must_use]
    pub const fn of(n: usize, fixed: bool) -> Self {
        if !fixed || n >= CACHEFRIENDLY_PRODUCT_THRESHOLD {
            SizeClass::Large
        } else if n == 1 {
            SizeClass::One
        } else {
            SizeClass::Small
        }
    }
}

/// Eigen product implementation (`product_type_selector`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProductKind {
    /// `1×k · k×1`: one reduction over the depth.
    Inner,
    /// Depth 1, large result: each coefficient is one product.
    Outer,
    /// Coefficient-based (lazy) product: each coefficient is a reduction
    /// over the depth (tree for compile-time depth within the unrolling
    /// limit, otherwise sequential from the first term).
    CoeffBased,
    /// Matrix–vector kernel: sequential from `+0`.
    Gemv,
    /// General matrix–matrix product: coefficient-based below
    /// [`GEMM_TO_COEFFBASED_THRESHOLD`], else GEMV fallbacks or the blocked
    /// `gebp` kernel ([`crate::eigen_gebp`]).
    Gemm,
}

/// Eigen's product-type selection for `rows × depth · depth × cols`.
#[must_use]
pub const fn product_kind(rows: SizeClass, cols: SizeClass, depth: SizeClass) -> ProductKind {
    use ProductKind::{CoeffBased, Gemm, Gemv, Inner, Outer};
    use SizeClass::{Large, One, Small};
    match (rows, cols, depth) {
        (One, One, _) => Inner,
        (_, One, One) | (One, _, One) => CoeffBased, // LazyCoeffBased
        (Small, Small, One) | (Small, Large, One) | (Large, Small, One) => CoeffBased, // LazyCoeffBased
        (_, _, One) => Outer,
        (Small, One, Small) | (One, Small, Small) | (Small, Small, Small) => CoeffBased,
        (One, Large, Small) | (One, Small, Large) | (Large, One, Small) | (Small, One, Large) => {
            CoeffBased
        }
        (Large, Small, Small) | (Small, Large, Small) => CoeffBased,
        (One, Large, Large) | (Large, One, Large) => Gemv,
        (Small, Small, Large)
        | (Large, Small, Large)
        | (Small, Large, Large)
        | (Large, Large, Large)
        | (Large, Large, Small) => Gemm,
    }
}

/// One coefficient of a coefficient-based / inner product with depth
/// `depth`: the reduction of `t(k) = lhs(i,k)·rhs(k,j)`, a halving tree iff
/// the **left operand's column count** is compile-time (`lhs_cols_fixed`)
/// and the depth is within the unrolling limit ([`Reduction::of_product`];
/// a compile-time depth fixed only by the right operand reduces
/// sequentially). Empty → `+0`.
#[inline]
pub fn product_coeff(depth: usize, lhs_cols_fixed: bool, t: &impl Fn(usize) -> f64) -> f64 {
    Reduction::of_product(depth, lhs_cols_fixed).sum(depth, t)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tree_shapes() {
        // Distinguish orders with values where association matters.
        let v = [1.0, 1e-16, 1e-16, 1e-16];
        let t = |i: usize| v[i];
        // len 3: t0 + (t1 + t2)
        assert_eq!(redux_tree(0, 3, &t, add), 1.0 + (1e-16 + 1e-16));
        assert_ne!(redux_tree(0, 3, &t, add), (1.0 + 1e-16) + 1e-16);
        // len 4: (t0 + t1) + (t2 + t3)
        assert_eq!(redux_tree(0, 4, &t, add), (1.0 + 1e-16) + (1e-16 + 1e-16));
        assert_eq!(redux_seq(3, &t, add), (1.0 + 1e-16) + 1e-16);
    }

    #[test]
    fn signed_zero_rules() {
        let t = |_: usize| -0.0;
        assert!(redux_seq(2, &t, add).is_sign_negative());
        assert!(sum_from_zero(2, &t).is_sign_positive());
        assert!(sum(0, 1, true, &t).is_sign_positive());
        assert_eq!(prod(0, 1, true, &t), 1.0);
    }

    #[test]
    fn unrolling_limit() {
        // products: 3n + (n - 1) <= 110  <=>  n <= 27
        let ones = |_: usize| 1.0;
        assert_eq!(product_coeff(27, true, &ones), 27.0);
        assert_eq!(27 * PRODUCT_TERM_COST + 26, 107);
        const { assert!(28 * PRODUCT_TERM_COST + 27 > UNROLLING_LIMIT) };
    }

    #[test]
    fn reduction_orders() {
        assert_eq!(Reduction::of_product(27, true), Reduction::Tree);
        assert_eq!(Reduction::of_product(28, true), Reduction::Sequential);
        assert_eq!(Reduction::of_product(3, false), Reduction::Sequential);
        assert_eq!(Reduction::of_sum(0, 1, true), Reduction::Sequential);
        let v = [1.0, 1e-16, 1e-16];
        let t = |i: usize| v[i];
        assert_eq!(Reduction::Tree.sum(3, &t), 1.0 + (1e-16 + 1e-16));
        assert_eq!(Reduction::Sequential.sum(3, &t), (1.0 + 1e-16) + 1e-16);
        assert_eq!(product_coeff(3, false, &t), (1.0 + 1e-16) + 1e-16);
        assert!(Reduction::Tree.sum(0, &t).is_sign_positive());
    }

    #[test]
    fn product_kinds() {
        use SizeClass::*;
        assert_eq!(product_kind(Small, Small, Small), ProductKind::CoeffBased);
        assert_eq!(product_kind(One, One, Small), ProductKind::Inner);
        assert_eq!(product_kind(Large, One, Large), ProductKind::Gemv);
        assert_eq!(product_kind(Large, One, Small), ProductKind::CoeffBased);
        assert_eq!(product_kind(Large, Large, Large), ProductKind::Gemm);
        assert_eq!(product_kind(Large, Large, One), ProductKind::Outer);
        assert_eq!(product_kind(Small, Large, One), ProductKind::CoeffBased);
        assert_eq!(SizeClass::of(3, true), Small);
        assert_eq!(SizeClass::of(8, true), Large);
        assert_eq!(SizeClass::of(1, true), One);
        assert_eq!(SizeClass::of(1, false), Large);
    }
}
