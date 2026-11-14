// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS: the products of
// src/common/aka_types.hh and of the FE kernels built on it, e.g.
// src/fe_engine/shape_lagrange_inline_impl.hh `B.transpose() * D * B`).
// Source used: Debian libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle
// image akantu-oracle:dev. The complete notice of every Eigen file this file
// derives from is reproduced verbatim below (MPL-2.0 §3.4).
//
// ---- Eigen/src/Core/products/GeneralBlockPanelKernel.h
//      (gebp_traits<double, double> without vectorisation, gebp_kernel and
//      lhs_process_one_packet accumulation order, default cache sizes,
//      evaluateProductBlockingSizesHeuristic depth blocking):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/GeneralMatrixMatrix.h
//      (general_matrix_matrix_product block loops, gemm_blocking_space,
//      generic_product_impl<…, GemmProduct>::evalTo / scaleAndAddTo):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2009 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/products/GeneralMatrixVector.h
//      (general_matrix_vector_product, column- and row-major accumulation
//      order and column blocking):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2016 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/GeneralProduct.h
//      (product_size_category, gemv_dense_selector storage-order dispatch,
//      EIGEN_GEMM_TO_COEFFBASED_THRESHOLD):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2008-2011 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/ProductEvaluators.h
//      (inner / outer / lazy coefficient-based products — the coefficient
//      `(lhs.row(i).transpose().cwiseProduct(rhs.col(j))).sum()`, the inner
//      product `(lhs.transpose().cwiseProduct(rhs)).sum()` — and the
//      GemvProduct inner-product fallback):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2008-2010 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2011 Jitse Niesen <jitse@maths.leeds.ac.uk>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/CwiseBinaryOp.h
//      (traits<CwiseBinaryOp>: the compile-time sizes of an element-wise
//      binary expression are those of its left operand):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2008-2014 Gael Guennebaud <gael.guennebaud@inria.fr>
//   Copyright (C) 2006-2008 Benoit Jacob <jacob.benoit.1@gmail.com>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/Dot.h
//      (dot_nocheck: the reduced expression of the inner-product fallback
//      `lhs.row(0).dot(rhs.col(0))` is built on its left operand):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2006-2008, 2010 Benoit Jacob <jacob.benoit.1@gmail.com>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Eigen 3.4.0 strict-build product evaluation (decisions D-008,
//! D-036; `docs/NUMERICAL_VALIDATION.md`).
//!
//! This is the **one maintained implementation** of upstream's dense
//! products: [`crate::tensor::MatrixExt::matmul`] (compile-time sizes),
//! [`crate::tensor::MatrixX::matmul`] (run-time sizes) and mixed
//! compile-time/run-time products ([`product_into`], also used for every
//! FE kernel product through `akantu_fe::eigen`) all dispatch here. It
//! reproduces, bit for bit, what Eigen computes when compiled like the
//! oracle's `strict` build (`-O2 -ffp-contract=off -DEIGEN_DONT_VECTORIZE`,
//! no OpenMP, aarch64):
//!
//! 1. **Route** — `product_type_selector` on the compile-time size classes
//!    ([`eo::product_kind`]): inner, outer, coefficient-based, GEMV or GEMM.
//!    The rows class comes from the left operand, the columns class from the
//!    right one, the depth class from `EIGEN_SIZE_MIN_PREFER_FIXED(Lhs cols,
//!    Rhs rows)`: compile-time if **either** operand fixes it
//!    ([`ProductShape::depth`]).
//! 2. **GEMM entry** (`generic_product_impl<…, GemmProduct>::evalTo`): if
//!    `depth + rows + cols < 20` and `depth > 0` the product is evaluated
//!    lazily (coefficient-based) — **for compile-time sizes too**. Otherwise
//!    `dst.setZero()` and `scaleAndAddTo`: empty operands leave zeros, a
//!    run-time single result column or row falls back to GEMV (and a `1 × 1`
//!    result to an inner product added to `+0`); everything else is `gebp`.
//! 3. **Coefficient reductions** (coefficient-based and inner products,
//!    the run-time inner-product fallback): each coefficient is the `sum()`
//!    of `lhs.row(i)ᵀ ∘ rhs.col(j)` (`lhsᵀ ∘ rhs`, `lhs.row(0)ᵀ ∘ rhs.col(0)`).
//!    An element-wise binary expression takes its compile-time size from its
//!    **left** operand, so the reduction has the compile-time length
//!    `Lhs::ColsAtCompileTime`: a halving tree iff the **left operand's
//!    column count** is compile-time and `3·depth + (depth − 1) ≤ 110`,
//!    otherwise sequential from the first term — even when the right
//!    operand fixes the depth (e.g. a run-time `C` times a fixed strain
//!    vector: route coefficient-based, order sequential;
//!    [`ProductShape::lhs_depth`], [`Reduction`]).
//! 4. **`gebp` order** (scalar traits: `EIGEN_ARCH_DEFAULT_NUMBER_OF_REGISTERS
//!    = 8` ⇒ `mr = 1`, `nr = 4`; peeling `pk = 8`; no single-instruction madd
//!    without vectorisation, so every step is `c ← c + fl(a·b)`): result
//!    columns `j < 4⌊cols/4⌋` go through the `1 × 4` micro-kernel, which
//!    accumulates the even and the odd `k` of the peeled part
//!    (`k < 8⌊kc/8⌋`) in two chains from `+0`, adds them, then the remaining
//!    `k` sequentially; the other columns accumulate sequentially from `+0`.
//!    The block result is added into the destination as `c·α + dst`
//!    (`α = 1`, `dst` zeroed first).
//! 5. **Depth blocking** — compile-time-finite products never block
//!    (`gemm_blocking_space<…, true>`: `kc = MaxDepth`). Run-time products
//!    keep `kc = depth` when `max(rows, cols, depth) < 48`; otherwise, with
//!    Eigen's default 16 KiB L1 (no cache query on aarch64) and
//!    `max_kc = ((L1 − mr·nr·8)/((mr + nr)·8)) & ~7 = 408`, a depth above
//!    `max_kc` is split into blocks of
//!    `kc = max_kc − 8·⌊(max_kc − 1 − depth mod max_kc)/(8·(⌊depth/max_kc⌋ + 1))⌋`
//!    (`max_kc` if `depth mod max_kc = 0`), e.g. depth 409 → 208 + 201,
//!    817 → 280 + 280 + 257. Block partials are added into the
//!    destination in block order. Row and column blocking (`mc`, `nc`) do
//!    not change any entry's order (`mr = 1`; `nc` is a multiple of `nr`).
//! 6. **GEMV** (`general_matrix_vector_product`): a column-major matrix
//!    operand accumulates each result entry from `+0` over column blocks of
//!    `depth` (`depth < 128`), else 16 (or 4 when the matrix outer stride
//!    is `≥ 4000` rows), each block partial added into the zeroed result; a
//!    row-major matrix operand (transposed column-major storage)
//!    accumulates sequentially from `+0` over the whole depth.
//!
//! On x86-64 Eigen would query the L1 size with `cpuid`, making the depth
//! blocking machine-dependent; the reference platform is aarch64 (D-002).
//! Operands are assumed contiguous (the outer stride of a column-major
//! matrix operand equals its row count), which holds for every `MatrixX`,
//! fixed `Matrix` and upstream `make_view` map. They are plain (read cost
//! 1: `Matrix`, `Map`, blocks, transposes, evaluated temporaries), and the
//! destination has the product's compile-time extents (the product's
//! `PlainObject` temporary, or an upstream map of the same extents), which
//! is what selects compile-time-finite GEMM blocking (item 5).
//!
//! **Describing an upstream product.** [`ProductShape::new`] takes the
//! compile-time/run-time extents of the two operand *expressions* exactly as
//! Eigen sees them (after any `.transpose()`; a product temporary such as
//! `(A·B)` in `A·B·C` has `A`'s row extent and `B`'s column extent). For
//! example the anisotropic material's `C * voigt_strain` (`C`:
//! `Matrix<Real>`, strain: `Vector<Real, 6>`) is
//! `ProductShape::new(Dynamic(6), Dynamic(6), Fixed(6), Fixed(1))`.

pub use crate::eigen_order::Reduction;
use crate::eigen_order::{self as eo, ProductKind, SizeClass};
use crate::error::{Error, Result};
use crate::types::Real;

/// `gebp_traits<double, double>::mr` without vectorisation
/// (`(min(16, NumberOfRegisters) / 2 / nr) · LhsPacketSize` with 8 registers).
pub const GEBP_MR: usize = 1;
/// `gebp_traits::nr`: result columns per micro-kernel panel.
pub const GEBP_NR: usize = 4;
/// `gebp_kernel` depth peeling factor `pk`.
pub const GEBP_PK: usize = 8;
/// Eigen's default L1 cache size on non-x86, non-PPC targets (bytes).
pub const DEFAULT_L1_CACHE_SIZE: usize = 16 * 1024;
/// `evaluateProductBlockingSizesHeuristic` returns early (no blocking) when
/// `max(rows, cols, depth)` is below this size (single-threaded).
pub const GEMM_BLOCKING_MIN_SIZE: usize = 48;
/// `general_matrix_vector_product<ColMajor>`: depths below this are not
/// blocked.
pub const GEMV_BLOCKING_MIN_DEPTH: usize = 128;
/// Size of `f64` in bytes (Eigen's `sizeof(Scalar)` in the cache formulas).
const SCALAR_BYTES: usize = std::mem::size_of::<Real>();

/// One operand dimension as Eigen sees it: its run-time size and whether it
/// is a compile-time constant (`RowsAtCompileTime` / `ColsAtCompileTime`
/// not `Eigen::Dynamic`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Extent {
    /// Compile-time size.
    Fixed(usize),
    /// Run-time (`Eigen::Dynamic`) size.
    Dynamic(usize),
}

impl Extent {
    /// The size.
    #[must_use]
    pub const fn size(self) -> usize {
        match self {
            Extent::Fixed(n) | Extent::Dynamic(n) => n,
        }
    }

    /// Whether the size is a compile-time constant.
    #[must_use]
    pub const fn is_fixed(self) -> bool {
        matches!(self, Extent::Fixed(_))
    }

    /// Eigen `product_size_category` (plain types: `MaxSize = Size`).
    #[must_use]
    pub const fn size_class(self) -> SizeClass {
        SizeClass::of(self.size(), self.is_fixed())
    }
}

/// Storage order of a product operand as Eigen's kernels see it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Order {
    /// Plain column-major storage (`Matrix`, `Map`).
    ColMajor,
    /// Row-major: the transpose of column-major storage (`a.transpose()`).
    RowMajor,
}

/// Compile-time description of an Eigen product
/// `lhs (rows × depth) · rhs (depth × cols)`: the extents of both operand
/// expressions (the inner dimension once per operand, because Eigen uses
/// both) and their storage orders.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ProductShape {
    /// Result rows: the left operand's rows (`Lhs::RowsAtCompileTime`).
    pub rows: Extent,
    /// Inner dimension as declared by the **left** operand
    /// (`Lhs::ColsAtCompileTime`): decides the order of every coefficient
    /// reduction ([`Reduction::of_product`]).
    pub lhs_depth: Extent,
    /// Inner dimension as declared by the **right** operand
    /// (`Rhs::RowsAtCompileTime`); must have the size of
    /// [`ProductShape::lhs_depth`].
    pub rhs_depth: Extent,
    /// Result columns: the right operand's columns
    /// (`Rhs::ColsAtCompileTime`).
    pub cols: Extent,
    /// Storage order of the left operand.
    pub lhs_order: Order,
    /// Storage order of the right operand.
    pub rhs_order: Order,
}

impl ProductShape {
    /// Product of compile-time-sized, column-major operands.
    #[must_use]
    pub const fn fixed(rows: usize, depth: usize, cols: usize) -> Self {
        Self::new(
            Extent::Fixed(rows),
            Extent::Fixed(depth),
            Extent::Fixed(depth),
            Extent::Fixed(cols),
        )
    }

    /// Product of run-time-sized, column-major operands.
    #[must_use]
    pub const fn dynamic(rows: usize, depth: usize, cols: usize) -> Self {
        Self::new(
            Extent::Dynamic(rows),
            Extent::Dynamic(depth),
            Extent::Dynamic(depth),
            Extent::Dynamic(cols),
        )
    }

    /// Product of a left operand of extents `lhs_rows × lhs_cols` and a
    /// right operand of extents `rhs_rows × rhs_cols` (column-major; see
    /// [`ProductShape::lhs_transposed`] / [`ProductShape::rhs_transposed`]),
    /// i.e. Eigen's `Matrix<lhs_rows, lhs_cols> * Matrix<rhs_rows,
    /// rhs_cols>`. `lhs_cols` and `rhs_rows` must have the same size
    /// (checked by [`product_into`]).
    #[must_use]
    pub const fn new(
        lhs_rows: Extent,
        lhs_cols: Extent,
        rhs_rows: Extent,
        rhs_cols: Extent,
    ) -> Self {
        Self {
            rows: lhs_rows,
            lhs_depth: lhs_cols,
            rhs_depth: rhs_rows,
            cols: rhs_cols,
            lhs_order: Order::ColMajor,
            rhs_order: Order::ColMajor,
        }
    }

    /// Same shape with a transposed (row-major) left operand.
    #[must_use]
    pub const fn lhs_transposed(mut self) -> Self {
        self.lhs_order = Order::RowMajor;
        self
    }

    /// Same shape with a transposed (row-major) right operand.
    #[must_use]
    pub const fn rhs_transposed(mut self) -> Self {
        self.rhs_order = Order::RowMajor;
        self
    }

    /// The product's inner dimension as the product-type selection sees it,
    /// `EIGEN_SIZE_MIN_PREFER_FIXED(Lhs::Cols, Rhs::Rows)`: compile-time if
    /// either operand fixes it (for operands of the same size).
    #[must_use]
    pub const fn depth(&self) -> Extent {
        if self.lhs_depth.is_fixed() {
            self.lhs_depth
        } else {
            self.rhs_depth
        }
    }

    /// Order of the coefficient reductions (coefficient-based / inner
    /// products, the run-time inner-product fallback): decided by the left
    /// operand's column extent.
    #[must_use]
    pub const fn coeff_reduction(&self) -> Reduction {
        Reduction::of_product(self.lhs_depth.size(), self.lhs_depth.is_fixed())
    }

    /// `rows · cols`, checked.
    fn result_len(&self) -> Result<usize> {
        crate::types::checked_len(self.rows.size(), self.cols.size(), "product result")
    }

    /// Whether Eigen's GEMM blocking space is finite at compile time
    /// (`MaxRows`, `MaxCols` and `MaxDepth` all fixed; the destination has
    /// the product's extents).
    #[must_use]
    pub const fn gemm_finite_at_compile_time(&self) -> bool {
        self.rows.is_fixed() && self.cols.is_fixed() && self.depth().is_fixed()
    }
}

/// Which matrix operand a GEMV traverses.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GemvMatrix {
    /// Column result: `lhs (rows × depth) · rhs(·, 0)`.
    Lhs,
    /// Row result: `lhs(0, ·) · rhs (depth × cols)`, evaluated by Eigen as
    /// `rhsᵀ · lhs(0, ·)ᵀ`.
    Rhs,
}

/// How Eigen 3.4.0 (strict build) evaluates a product.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Route {
    /// No coefficient to compute, or an empty depth: the result is `+0`
    /// everywhere.
    Zero,
    /// Coefficient-based / inner product: each coefficient is the reduction
    /// of its depth terms in the given order (halving tree iff the left
    /// operand's column count is compile-time and the depth within the
    /// unrolling limit, else sequential from the first term).
    Coeff(Reduction),
    /// Outer product (compile-time depth 1): each coefficient is one product.
    Outer,
    /// GEMV fallback of a `1 × 1` result: inner product (reduced as in
    /// [`Route::Coeff`]) added to the zeroed destination.
    Dot(Reduction),
    /// Matrix–vector kernel over `matrix`, with the kernel of the given
    /// storage order (see [`gemv_into`]).
    Gemv {
        /// Operand traversed as the matrix.
        matrix: GemvMatrix,
        /// Storage order of that operand as the kernel sees it.
        kernel: Order,
    },
    /// `gebp` kernel with depth blocks of `kc`.
    Gebp {
        /// Depth block size.
        kc: usize,
    },
}

/// `EIGEN_GEMM_TO_COEFFBASED_THRESHOLD`.
const COEFF_THRESHOLD: usize = eo::GEMM_TO_COEFFBASED_THRESHOLD;

/// GEMV route for a matrix operand of storage order `order` on side
/// `matrix` (`gemv_dense_selector`: a vector on the left is handled by
/// transposing the product, which flips the matrix operand's order).
const fn gemv(matrix: GemvMatrix, order: Order) -> Route {
    let kernel = match (matrix, order) {
        (GemvMatrix::Lhs, o) => o,
        (GemvMatrix::Rhs, Order::ColMajor) => Order::RowMajor,
        (GemvMatrix::Rhs, Order::RowMajor) => Order::ColMajor,
    };
    Route::Gemv { matrix, kernel }
}

/// `generic_product_impl<…, GemvProduct>::scaleAndAddTo` for the given
/// matrix side: a run-time `1 × 1` result becomes an inner product.
const fn gemv_scale_and_add(shape: &ProductShape, matrix: GemvMatrix) -> Route {
    if shape.rows.size() == 1 && shape.cols.size() == 1 {
        return Route::Dot(shape.coeff_reduction());
    }
    match matrix {
        GemvMatrix::Lhs => gemv(matrix, shape.lhs_order),
        GemvMatrix::Rhs => gemv(matrix, shape.rhs_order),
    }
}

/// Eigen's evaluation route for `shape` (strict build).
#[must_use]
pub fn route(shape: &ProductShape) -> Route {
    let depth = shape.depth();
    let (m, k, n) = (shape.rows.size(), depth.size(), shape.cols.size());
    if m == 0 || n == 0 || k == 0 {
        // Empty reductions are +0 (`sum()` of nothing), GEMV/GEMM leave the
        // zeroed destination; nothing to compute for an empty result.
        return Route::Zero;
    }
    match eo::product_kind(
        shape.rows.size_class(),
        shape.cols.size_class(),
        depth.size_class(),
    ) {
        ProductKind::Inner | ProductKind::CoeffBased => Route::Coeff(shape.coeff_reduction()),
        ProductKind::Outer => Route::Outer,
        ProductKind::Gemv => {
            // `Side = Lhs::IsVectorAtCompileTime ? OnTheLeft : OnTheRight`.
            if shape.rows.size_class() == SizeClass::One {
                gemv_scale_and_add(shape, GemvMatrix::Rhs)
            } else {
                gemv_scale_and_add(shape, GemvMatrix::Lhs)
            }
        }
        ProductKind::Gemm => {
            if k + m + n < COEFF_THRESHOLD {
                Route::Coeff(shape.coeff_reduction())
            } else if n == 1 {
                gemv_scale_and_add(shape, GemvMatrix::Lhs)
            } else if m == 1 {
                gemv_scale_and_add(shape, GemvMatrix::Rhs)
            } else {
                Route::Gebp {
                    kc: gemm_kc(m, n, k, shape.gemm_finite_at_compile_time()),
                }
            }
        }
    }
}

/// `max_kc` of `evaluateProductBlockingSizesHeuristic` (single thread,
/// `KcFactor = 1`): `max(((l1 − mr·nr·8) / ((mr + nr)·8)) & ~(pk − 1), 1)`;
/// 408 for the default 16 KiB L1.
#[must_use]
pub const fn gebp_max_kc(l1: usize) -> usize {
    let k_sub = GEBP_MR * GEBP_NR * SCALAR_BYTES;
    let k_div = GEBP_MR * SCALAR_BYTES + GEBP_NR * SCALAR_BYTES;
    let kc = (l1.saturating_sub(k_sub) / k_div) & !(GEBP_PK - 1);
    if kc > 1 { kc } else { 1 }
}

/// Depth block size `kc` Eigen uses for a GEMM of `rows × depth · depth ×
/// cols` (`finite`: blocking space finite at compile time, see
/// [`ProductShape::gemm_finite_at_compile_time`]).
#[must_use]
pub const fn gemm_kc(rows: usize, cols: usize, depth: usize, finite: bool) -> usize {
    if finite {
        // gemm_blocking_space<…, true>: m_kc = MaxDepth.
        return depth;
    }
    let mx = if rows > cols { rows } else { cols };
    let mx = if mx > depth { mx } else { depth };
    if mx < GEMM_BLOCKING_MIN_SIZE {
        return depth;
    }
    let max_kc = gebp_max_kc(DEFAULT_L1_CACHE_SIZE);
    if depth <= max_kc {
        return depth;
    }
    // Reduce the block size so that the last block is as large as possible
    // while keeping the same number of sweeps.
    let rem = depth % max_kc;
    if rem == 0 {
        max_kc
    } else {
        max_kc - GEBP_PK * ((max_kc - 1 - rem) / (GEBP_PK * (depth / max_kc + 1)))
    }
}

/// Evaluate `out = lhs · rhs` (`out` column-major `rows × cols`) exactly as
/// Eigen 3.4.0's strict build does for an expression of shape `shape`
/// (route [`route`]). `lhs(i, k)` and `rhs(k, j)` read the operands'
/// logical coefficients; `shape` carries their compile-time sizes and
/// storage orders.
///
/// # Errors
/// [`Error::DimensionMismatch`] if the operands' inner dimensions differ
/// (`lhs_depth` vs `rhs_depth`) or `out.len() != rows · cols`.
pub fn product_into(
    shape: &ProductShape,
    lhs: &impl Fn(usize, usize) -> Real,
    rhs: &impl Fn(usize, usize) -> Real,
    out: &mut [Real],
) -> Result<()> {
    if shape.lhs_depth.size() != shape.rhs_depth.size() {
        return Err(Error::DimensionMismatch {
            context: "eigen_gebp::product_into (inner dimension)",
            expected: shape.lhs_depth.size(),
            got: shape.rhs_depth.size(),
        });
    }
    let len = shape.result_len()?;
    if out.len() != len {
        return Err(Error::DimensionMismatch {
            context: "eigen_gebp::product_into",
            expected: len,
            got: out.len(),
        });
    }
    eval_product(shape, lhs, rhs, out);
    Ok(())
}

/// [`product_into`] for callers that guarantee `out.len() == rows · cols`
/// (compile-time-sized results, freshly allocated `MatrixX`).
pub(crate) fn eval_product(
    shape: &ProductShape,
    lhs: &impl Fn(usize, usize) -> Real,
    rhs: &impl Fn(usize, usize) -> Real,
    out: &mut [Real],
) {
    let (m, k, n) = (shape.rows.size(), shape.lhs_depth.size(), shape.cols.size());
    debug_assert_eq!(shape.rhs_depth.size(), k);
    debug_assert_eq!(out.len(), m * n);
    match route(shape) {
        Route::Zero => out.fill(0.0),
        Route::Coeff(order) => {
            for j in 0..n {
                for i in 0..m {
                    out[i + j * m] = order.sum(k, &|l| lhs(i, l) * rhs(l, j));
                }
            }
        }
        Route::Outer => {
            // outer_product_selector_run: dst.col(j) = rhs(0, j) * lhs.
            for j in 0..n {
                for i in 0..m {
                    out[i + j * m] = rhs(0, j) * lhs(i, 0);
                }
            }
        }
        Route::Dot(order) => {
            // dst(0, 0) += alpha * lhs.row(0).dot(rhs.col(0)), dst zeroed.
            out[0] = 0.0;
            out[0] += order.sum(k, &|l| lhs(0, l) * rhs(l, 0));
        }
        Route::Gemv { matrix, kernel } => match matrix {
            GemvMatrix::Lhs => gemv_into(m, k, kernel, &|i, l| lhs(i, l), &|l| rhs(l, 0), out),
            GemvMatrix::Rhs => gemv_into(n, k, kernel, &|j, l| rhs(l, j), &|l| lhs(0, l), out),
        },
        Route::Gebp { kc } => gebp_into(m, n, k, kc, lhs, rhs, out),
    }
}

/// `general_matrix_vector_product`: `out[i] = Σ_l a(i, l) · x(l)` for
/// `i < len`, `l < depth`, into a zeroed `out`, with the kernel of storage
/// order `kernel`:
/// * `ColMajor`: per column block `[l0, l1)` (whole depth if
///   `depth < 128`, else 16 columns, or 4 when the outer stride `len` is
///   `≥ 4000`), `c = 0; c ← c + fl(a·x)`, then `out[i] ← c·α + out[i]`;
/// * `RowMajor`: `c = 0; c ← c + fl(a·x)` over the whole depth, then
///   `out[i] ← out[i] + α·c`.
pub fn gemv_into(
    len: usize,
    depth: usize,
    kernel: Order,
    a: &impl Fn(usize, usize) -> Real,
    x: &impl Fn(usize) -> Real,
    out: &mut [Real],
) {
    debug_assert_eq!(out.len(), len);
    out.fill(0.0);
    match kernel {
        Order::ColMajor => {
            let block = if depth < GEMV_BLOCKING_MIN_DEPTH {
                depth
            } else if len * SCALAR_BYTES < 32000 {
                16
            } else {
                4
            };
            let mut l0 = 0;
            while l0 < depth {
                let l1 = (l0 + block).min(depth);
                for (i, o) in out.iter_mut().enumerate() {
                    let mut c = 0.0;
                    for l in l0..l1 {
                        c += a(i, l) * x(l);
                    }
                    // pmadd(c, alpha, res) = c·1 + res; IEEE addition is
                    // commutative: same bits as `res + c`.
                    *o += c;
                }
                l0 = l1;
            }
        }
        Order::RowMajor => {
            for (i, o) in out.iter_mut().enumerate() {
                let mut c = 0.0;
                for l in 0..depth {
                    c += a(i, l) * x(l);
                }
                *o += c; // res += alpha * c with alpha = 1
            }
        }
    }
}

/// Strict-build `gebp` evaluation of `out = lhs · rhs` (`out` column-major
/// `rows × cols`, overwritten; `dst.setZero()` first) with depth blocks of
/// `kc` (see the module documentation for the order).
pub fn gebp_into(
    rows: usize,
    cols: usize,
    depth: usize,
    kc: usize,
    lhs: &impl Fn(usize, usize) -> Real,
    rhs: &impl Fn(usize, usize) -> Real,
    out: &mut [Real],
) {
    debug_assert_eq!(out.len(), rows * cols);
    debug_assert!(kc > 0 || depth == 0);
    out.fill(0.0);
    // Columns handled by the 1 × nr micro-kernel. Column blocks (nc) are
    // multiples of nr, so the per-call `packet_cols4` partition coincides
    // with the global one.
    let packet_cols4 = (cols / GEBP_NR) * GEBP_NR;
    let mut k0 = 0;
    while k0 < depth {
        let kb = kc.min(depth - k0);
        let peeled_kc = kb & !(GEBP_PK - 1);
        for j in 0..cols {
            for i in 0..rows {
                let t = |l: usize| lhs(i, k0 + l) * rhs(k0 + l, j);
                let c = if j < packet_cols4 {
                    // Even k accumulate in C, odd k in D, then C = C + D.
                    let (mut c, mut d) = (0.0, 0.0);
                    let mut l = 0;
                    while l < peeled_kc {
                        c += t(l);
                        d += t(l + 1);
                        l += 2;
                    }
                    let mut c = c + d;
                    for l in peeled_kc..kb {
                        c += t(l);
                    }
                    c
                } else {
                    let mut c = 0.0;
                    for l in 0..kb {
                        c += t(l);
                    }
                    c
                };
                let r = &mut out[i + j * rows];
                // traits.acc: r = c·alpha + r with alpha = 1; IEEE addition
                // is commutative: same bits as `r + c`.
                *r += c;
            }
        }
        k0 += kb;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use Extent::{Dynamic as D, Fixed as F};
    use Reduction::{Sequential as Seq, Tree};

    fn dyn_route(m: usize, k: usize, n: usize) -> Route {
        route(&ProductShape::dynamic(m, k, n))
    }

    #[test]
    fn depth_block_rule_ar3_06() {
        assert_eq!(gebp_max_kc(DEFAULT_L1_CACHE_SIZE), 408);
        // below the blocking heuristic's size limit: one block
        assert_eq!(gemm_kc(3, 5, 47, false), 47);
        // max(rows, cols, depth) >= 48 but depth <= max_kc: one block
        assert_eq!(gemm_kc(50, 50, 408, false), 408);
        assert_eq!(gemm_kc(3, 5, 409, false), 208); // 208 + 201
        assert_eq!(gemm_kc(3, 5, 816, false), 408); // 408 + 408
        assert_eq!(gemm_kc(3, 5, 817, false), 280); // 280 + 280 + 257
        assert_eq!(gemm_kc(3, 5, 1000, false), 336); // 336 + 336 + 328
        // the number of sweeps is preserved (Eigen's internal assertion)
        for depth in 409..3000 {
            let kc = gemm_kc(3, 5, depth, false);
            assert_eq!(depth / kc, depth / 408, "depth {depth}");
            assert_eq!(kc % GEBP_PK, 0);
        }
        // compile-time-finite products never block
        assert_eq!(gemm_kc(3, 5, 817, true), 817);
    }

    #[test]
    fn routes() {
        // dynamic: lazy below the threshold, GEMV fallbacks, gebp
        assert_eq!(dyn_route(3, 6, 3), Route::Coeff(Seq));
        assert_eq!(dyn_route(6, 3, 6), Route::Coeff(Seq)); // 15 < 20
        assert_eq!(dyn_route(12, 3, 3), Route::Coeff(Seq)); // triangle6 Bᵀ·D
        assert_eq!(dyn_route(12, 3, 12), Route::Gebp { kc: 3 }); // (BᵀD)·B
        assert_eq!(
            dyn_route(20, 20, 1),
            Route::Gemv {
                matrix: GemvMatrix::Lhs,
                kernel: Order::ColMajor
            }
        );
        assert_eq!(
            dyn_route(1, 30, 6),
            Route::Gemv {
                matrix: GemvMatrix::Rhs,
                kernel: Order::RowMajor
            }
        );
        assert_eq!(dyn_route(1, 30, 1), Route::Dot(Seq));
        assert_eq!(dyn_route(1, 3, 1), Route::Coeff(Seq));
        assert_eq!(dyn_route(0, 3, 1), Route::Zero);
        assert_eq!(dyn_route(3, 0, 30), Route::Zero);
        // transposed operands select the other GEMV kernel
        assert_eq!(
            route(&ProductShape::dynamic(20, 20, 1).lhs_transposed()),
            Route::Gemv {
                matrix: GemvMatrix::Lhs,
                kernel: Order::RowMajor
            }
        );
        assert_eq!(
            route(&ProductShape::dynamic(1, 30, 6).rhs_transposed()),
            Route::Gemv {
                matrix: GemvMatrix::Rhs,
                kernel: Order::ColMajor
            }
        );
        // fixed sizes: small GemmProducts are coefficient-based (3x9x3)
        assert_eq!(route(&ProductShape::fixed(3, 9, 3)), Route::Coeff(Tree));
        assert_eq!(
            route(&ProductShape::fixed(2, 30, 2)),
            Route::Gebp { kc: 30 }
        );
        assert_eq!(route(&ProductShape::fixed(8, 8, 8)), Route::Gebp { kc: 8 });
        assert_eq!(route(&ProductShape::fixed(24, 6, 6)), Route::Coeff(Tree));
        assert_eq!(
            route(&ProductShape::fixed(2, 409, 4)),
            Route::Gebp { kc: 409 }
        );
        assert_eq!(
            route(&ProductShape::fixed(9, 9, 1)),
            Route::Gemv {
                matrix: GemvMatrix::Lhs,
                kernel: Order::ColMajor
            }
        );
        assert_eq!(
            route(&ProductShape::fixed(1, 8, 8)),
            Route::Gemv {
                matrix: GemvMatrix::Rhs,
                kernel: Order::RowMajor
            }
        );
        assert_eq!(route(&ProductShape::fixed(8, 1, 8)), Route::Outer);
        // mixed (upstream gradient `u * B.transpose()`, u: Dynamic × 20)
        let grad = ProductShape::new(D(3), F(20), F(20), F(3)).rhs_transposed();
        assert_eq!(route(&grad), Route::Gebp { kc: 20 });
        let grad8 = ProductShape::new(D(3), F(8), F(8), F(3)).rhs_transposed();
        assert_eq!(route(&grad8), Route::Coeff(Tree));
    }

    #[test]
    fn mixed_extents_reduce_by_the_left_operand() {
        // Anisotropic material `C * voigt_strain`, C: Matrix<Real> (dynamic),
        // strain: Vector<Real, 6>: the rhs fixes the depth (coefficient-based
        // route, (Large, One, Small)) but the reduction is sequential.
        let stress = ProductShape::new(D(6), D(6), F(6), F(1));
        assert_eq!(stress.depth(), F(6));
        assert_eq!(route(&stress), Route::Coeff(Seq));
        // The fixed-lhs mirror image is a tree.
        assert_eq!(
            route(&ProductShape::new(D(6), F(6), D(6), F(1))),
            Route::Coeff(Tree)
        );
        // GemmProduct below the coefficient threshold: lhs fixed rows, run-time
        // cols; rhs fixed (3 + 8 + 3 < 20).
        assert_eq!(
            route(&ProductShape::new(F(3), D(8), F(8), F(3))),
            Route::Coeff(Seq)
        );
        assert_eq!(
            route(&ProductShape::new(F(3), F(8), D(8), F(3))),
            Route::Coeff(Tree)
        );
        // compile-time inner product (1×k fixed rows, k×1 fixed cols)
        assert_eq!(
            route(&ProductShape::new(F(1), D(3), F(3), F(1))),
            Route::Coeff(Seq)
        );
        assert_eq!(
            route(&ProductShape::new(F(1), F(3), D(3), F(1))),
            Route::Coeff(Tree)
        );
        // run-time 1×1 GEMV fallback (One, Large, Large) with a fixed depth 20
        assert_eq!(
            route(&ProductShape::new(F(1), D(20), F(20), D(1))),
            Route::Dot(Seq)
        );
        assert_eq!(
            route(&ProductShape::new(F(1), F(20), D(20), D(1))),
            Route::Dot(Tree)
        );
        // 3-D rotation of the anisotropic material: rotator (fixed) · C'
        // (dynamic) is gebp, (temporary Matrix<9, Dynamic>) · revrotor is a
        // compile-time-finite GEMM (kc = 9); 2-D: both coefficient-based.
        assert_eq!(
            route(&ProductShape::new(F(9), F(9), D(9), D(9))),
            Route::Gebp { kc: 9 }
        );
        assert_eq!(
            route(&ProductShape::new(F(9), D(9), F(9), F(9))),
            Route::Gebp { kc: 9 }
        );
        assert_eq!(
            route(&ProductShape::new(F(4), F(4), D(4), D(4))),
            Route::Coeff(Tree)
        );
        assert_eq!(
            route(&ProductShape::new(F(4), D(4), F(4), F(4))),
            Route::Coeff(Seq)
        );
        // a compile-time depth with a run-time result extent is not finite:
        // the depth is blocked (409 → 208 + 201)
        assert_eq!(
            route(&ProductShape::new(D(2), D(409), F(409), F(4))),
            Route::Gebp { kc: 208 }
        );
        // the order is observable: t0 + (t1 + t2) vs (t0 + t1) + t2
        let a = |_: usize, k: usize| [1.0, 1e-16, 1e-16][k];
        let b = |_: usize, _: usize| 1.0;
        let mut seq = [0.0];
        product_into(&ProductShape::new(D(1), D(3), F(3), F(1)), &a, &b, &mut seq).unwrap();
        assert_eq!(seq[0], (1.0 + 1e-16) + 1e-16);
        let mut tree = [0.0];
        product_into(
            &ProductShape::new(D(1), F(3), D(3), F(1)),
            &a,
            &b,
            &mut tree,
        )
        .unwrap();
        assert_eq!(tree[0], 1.0 + (1e-16 + 1e-16));
        // inconsistent inner dimensions are rejected
        assert!(
            product_into(&ProductShape::new(D(1), D(3), F(2), F(1)), &a, &b, &mut seq).is_err()
        );
    }

    #[test]
    fn gebp_even_odd_chains_and_panels() {
        // depth 9, 5 columns: columns 0..4 use the 1x4 kernel, column 4 not.
        let big = 1.0;
        let tiny = 2f64.powi(-53);
        // terms: k = 0: 1, k > 0: 2^-53 (half an ulp of 1), so orders differ
        let a = |_: usize, k: usize| if k == 0 { big } else { tiny };
        let b = |_: usize, _: usize| 1.0;
        let mut out = vec![0.0; 5];
        gebp_into(1, 5, 9, 9, &a, &b, &mut out);
        // panel: even = 1 + 3 tiny (each absorbed) = 1; odd = 4 tiny = 2^-51;
        // 1 + 2^-51 exact; then + tiny (k = 8) rounds to even: 1 + 2^-51.
        let even = ((big + tiny) + tiny) + tiny; // k = 0, 2, 4, 6
        let odd = ((tiny + tiny) + tiny) + tiny;
        assert_eq!(out[0], (even + odd) + tiny);
        assert_eq!(out[0], 1.0 + 2f64.powi(-51));
        // remainder column: sequential, every tiny absorbed
        assert_eq!(out[4], 1.0);
    }

    #[test]
    fn gebp_depth_blocks_add_partials_in_order() {
        let tiny = 2f64.powi(-53);
        let a = |_: usize, k: usize| if k == 0 { 1.0 } else { tiny };
        let b = |_: usize, _: usize| 1.0;
        let mut blocked = vec![0.0; 2];
        gebp_into(1, 2, 409, 208, &a, &b, &mut blocked);
        let mut single = vec![0.0; 2];
        gebp_into(1, 2, 409, 409, &a, &b, &mut single);
        // single block: every tiny term is absorbed by the leading 1. Blocks
        // 208 + 201: the first partial is 1, the second sums 201 tiny terms
        // exactly, and 1 + 201·2^-53 rounds (tie to even) to 1 + 200·2^-53.
        assert_eq!(single[0], 1.0);
        assert_eq!(blocked[0], 1.0 + 200.0 * tiny);
    }

    #[test]
    fn gemv_column_blocking() {
        let tiny = 2f64.powi(-53);
        let a = |_: usize, k: usize| if k == 0 { 1.0 } else { tiny };
        let x = |_: usize| 1.0;
        let mut col = vec![0.0; 2];
        gemv_into(2, 128, Order::ColMajor, &a, &x, &mut col);
        let mut row = vec![0.0; 2];
        gemv_into(2, 128, Order::RowMajor, &a, &x, &mut row);
        // row-major: every tiny is absorbed by the leading 1
        assert_eq!(row[0], 1.0);
        // column-major: 8 blocks of 16; blocks 2..8 sum 16 tiny exactly
        assert!(col[0] > 1.0);
        let mut short = vec![0.0; 2];
        gemv_into(2, 127, Order::ColMajor, &a, &x, &mut short);
        assert_eq!(short[0], 1.0);
    }

    #[test]
    fn signed_zero_semantics() {
        // A single term -0: coefficient-based keeps it, gebp/GEMV start from +0.
        let a = |_: usize, _: usize| -1.0;
        let b = |_: usize, _: usize| 0.0;
        let mut out = vec![1.0; 4];
        product_into(&ProductShape::dynamic(2, 1, 2), &a, &b, &mut out).unwrap();
        assert!(out.iter().all(|v| *v == 0.0 && v.is_sign_negative()));
        let mut out = vec![1.0; 100];
        product_into(&ProductShape::dynamic(10, 1, 10), &a, &b, &mut out).unwrap();
        assert!(out.iter().all(|v| *v == 0.0 && v.is_sign_positive()));
        // outer product (compile-time depth 1) keeps -0
        let mut out = vec![1.0; 64];
        product_into(&ProductShape::fixed(8, 1, 8), &a, &b, &mut out).unwrap();
        assert!(out.iter().all(|v| *v == 0.0 && v.is_sign_negative()));
        assert!(product_into(&ProductShape::fixed(8, 1, 8), &a, &b, &mut [0.0; 3]).is_err());
    }
}
