// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs acceptance contract for the dense layer
// (docs/NUMERICAL_VALIDATION.md, decisions D-008/D-011): the evaluation
// order of upstream's Eigen 3.4.0 products (src/common/aka_types.hh,
// src/fe_engine/shape_lagrange_inline_impl.hh `Bt_D_B = B.transpose() * D *
// B`) and the large-size classes not covered by the strict dense fixture.

//! Dense acceptance classes.
//!
//! 1. **Product order (evidence).** A reference model of Eigen's
//!    strict-build (`-DEIGEN_DONT_VECTORIZE`) product evaluation —
//!    `product_type_selector`, the `EIGEN_GEMM_TO_COEFFBASED_THRESHOLD` rule
//!    (also for fixed-size `GemmProduct`s) and the scalar `gebp` kernel order
//!    (`mr = 1`, `nr = 4`, peeling `pk = 8` with even/odd accumulators in the
//!    `1 × 4` micro-kernel) — reproduces **every** product record of the
//!    oracle fixture bit for bit, including the 60 records the parity test
//!    checked against the GEMM tolerance class before the bitwise GEMM (production
//!    code: `akantu_core::eigen_gebp`; all bitwise in `dense_parity.rs` and
//!    `dense_gemm_parity.rs` since).
//! 2. **GEMM tolerance class.** NaN-safe and exact outside its assumptions
//!    (`common::gemm_entry`).
//! 3. **Large-size classes** (blocked LU `n > 16`, blocked eigenvector
//!    accumulation `n > 48`, sort ties `n > 16`): mathematical acceptance
//!    criteria with derived thresholds, exercised at the blocking boundaries
//!    and on ill-conditioned / clustered cases (the
//!    self-adjoint class L2s and the **general** class L2g have different
//!    criteria — general eigenvectors are not orthogonal; determinant
//!    relative bounds only in their first-order regime). These are
//!    mathematical checks of the Rust implementation, **not** differential
//!    evidence: the oracle boundary records are still pending.

mod common;

use akantu_core::eigen_order::{self as eo, ProductKind, SizeClass};
use akantu_core::tensor::MatrixX;
use akantu_linalg::dense::{self, SortOrder, Storage};
use akantu_validation::reduce::NanMinMax;
use common::{GemmVerdict, U, gamma, gemm_entry, product_records};

const FIXTURE: &str = include_str!("fixtures/dense/dense_strict.txt");

// ---- 1. Eigen strict-build product order ----------------------------------

/// Which Eigen implementation evaluates a product (strict build).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Route {
    /// Coefficient-based / inner product: reduction over the depth (halving
    /// tree for a compile-time depth within the unrolling limit, else
    /// sequential from the first term).
    Coeff,
    /// Outer product (depth 1).
    Outer,
    /// Matrix–vector kernel: sequential from `+0`.
    Gemv,
    /// `gebp` kernel.
    Gebp,
}

fn class(n: usize, fixed: bool) -> SizeClass {
    // Run-time sizes are `Large` for the selector (Eigen
    // `product_size_category<Dynamic, Dynamic>`).
    if fixed {
        SizeClass::of(n, true)
    } else if n == 1 {
        SizeClass::One
    } else {
        SizeClass::Large
    }
}

/// Eigen 3.4.0 `GeneralProduct.h` / `ProductEvaluators.h`.
fn route(r: usize, c: usize, k: usize, fixed: bool) -> Route {
    match eo::product_kind(class(r, fixed), class(k, fixed), class(c, fixed)) {
        ProductKind::Inner | ProductKind::CoeffBased => Route::Coeff,
        ProductKind::Outer => Route::Outer,
        ProductKind::Gemv => Route::Gemv,
        // generic_product_impl<…, GemmProduct>::evalTo: small products are
        // evaluated lazily, for fixed sizes too.
        ProductKind::Gemm if c + r + k < eo::GEMM_TO_COEFFBASED_THRESHOLD && c > 0 => Route::Coeff,
        ProductKind::Gemm => Route::Gebp,
    }
}

/// Eigen 3.4.0's depth block size `kc` (single thread, scalar traits
/// `mr = 1`, `nr = 4`, default 16 KiB L1 ⇒ `max_kc = ((16384 − 32)/40) & !7
/// = 408`; `evaluateProductBlockingSizesHeuristic`,
/// `GeneralBlockPanelKernel.h:202-227` of the oracle image): no blocking
/// when `max(rows, cols, depth) < 48` or `depth ≤ max_kc`; otherwise the
/// **balanced** size `max_kc − 8·((max_kc − 1 − depth % max_kc) /
/// (8·(depth / max_kc + 1)))` (or `max_kc` when it divides the depth), which
/// keeps the number of sweeps and makes the last block as large as
/// possible. Column blocks are multiples of `nr` (or the
/// whole width), so the panel/remainder class of a column never changes.
fn gebp_kc(rows: usize, cols: usize, depth: usize) -> usize {
    const MAX_KC: usize = 408;
    if rows.max(cols).max(depth) < 48 || depth <= MAX_KC {
        depth
    } else if depth.is_multiple_of(MAX_KC) {
        MAX_KC
    } else {
        MAX_KC - 8 * ((MAX_KC - 1 - depth % MAX_KC) / (8 * (depth / MAX_KC + 1)))
    }
}

/// `gebp` order for one entry `(i, j)` of an `r × k` result with depth `c`
/// (scalar traits: `mr = 1`, `nr = 4`; `pk = 8`). The depth is split into
/// `kc` blocks ([`gebp_kc`]); per block, columns in 4-wide panels use the
/// `1 × 4` micro-kernel: even and odd `k` of the block's peeled part
/// (`kc & !7`) accumulate separately from `+0`, are added, then the
/// block's remainder follows; other columns accumulate sequentially from
/// `+0`. Each block's accumulator is written as `dst = acc·α + dst`
/// (`pmadd`, `α = 1`) into `dst = +0` (`setZero`), blocks in depth order —
/// so an accumulated `−0` becomes `+0`. The fixture only reaches single
/// blocks (depth < 409); the multi-block order is specification for
/// Until oracle records at depths 409/817 exist.
fn gebp_entry(r: usize, c: usize, k: usize, j: usize, t: &impl Fn(usize) -> f64) -> f64 {
    let packet_cols4 = (k / 4) * 4;
    let kc = gebp_kc(r, k, c).max(1);
    let mut dst = 0.0;
    let mut k0 = 0;
    while k0 < c {
        let depth = kc.min(c - k0);
        let acc = if j < packet_cols4 {
            let peeled = depth & !7;
            let (mut even, mut odd) = (0.0, 0.0);
            let mut l = 0;
            while l < peeled {
                even += t(k0 + l);
                odd += t(k0 + l + 1);
                l += 2;
            }
            let mut acc = even + odd;
            for l in peeled..depth {
                acc += t(k0 + l);
            }
            acc
        } else {
            eo::sum_from_zero(depth, &|l| t(k0 + l))
        };
        // `pmadd(acc, α, dst) = acc·α + dst` (IEEE addition commutes,
        // signed zeros included: −0 + +0 = +0).
        dst += acc * 1.0;
        k0 += depth;
    }
    dst
}

/// Reference strict-build evaluation of `a (r×c) · b (c×k)`.
fn eigen_strict_product(
    r: usize,
    c: usize,
    k: usize,
    a: &[f64],
    b: &[f64],
    fixed: bool,
) -> (Route, Vec<f64>) {
    let route = route(r, c, k, fixed);
    let mut out = vec![0.0; r * k];
    for j in 0..k {
        for i in 0..r {
            let t = |l: usize| a[i + l * r] * b[l + j * c];
            out[i + j * r] = match route {
                Route::Coeff => eo::product_coeff(c, fixed, &t),
                Route::Outer => t(0),
                Route::Gemv => eo::sum_from_zero(c, &t),
                Route::Gebp => gebp_entry(r, c, k, j, &t),
            };
        }
    }
    (route, out)
}

/// The classification `dense_parity.rs` used to route a record to the
/// GEMM tolerance class before the bitwise GEMM.
fn parity_test_gemm_class(r: usize, c: usize, k: usize, fixed: bool) -> bool {
    if fixed {
        eo::product_kind(
            SizeClass::of(r, true),
            SizeClass::of(k, true),
            SizeClass::of(c, true),
        ) == ProductKind::Gemm
    } else {
        r + c + k >= 20 && r > 1 && k > 1
    }
}

#[test]
fn eigen_product_order_model_reproduces_every_oracle_product_record() {
    let records = product_records(FIXTURE);
    assert_eq!(records.len(), 330, "210 matmul + 120 trmatmul records");
    let (mut gemm_class, mut via_gebp, mut via_coeff) = (0, 0, 0);
    for rec in &records {
        let (route, got) = eigen_strict_product(rec.r, rec.c, rec.k, &rec.a, &rec.b, rec.fixed);
        let bits = |x: &[f64]| x.iter().map(|v| v.to_bits()).collect::<Vec<_>>();
        assert_eq!(
            bits(&got),
            bits(&rec.want),
            "line {}: {} {}x{}x{} ({route:?})",
            rec.line,
            if rec.transposed { "trmatmul" } else { "matmul" },
            rec.r,
            rec.c,
            rec.k
        );
        if parity_test_gemm_class(rec.r, rec.c, rec.k, rec.fixed) {
            gemm_class += 1;
            match route {
                Route::Gebp => via_gebp += 1,
                Route::Coeff => via_coeff += 1,
                r => panic!("unexpected route {r:?}"),
            }
        }
    }
    // The 60 records formerly exempted from bit equality in dense_parity.rs
    // (bitwise since the GEMM translation): 48 are gebp products (reproduced by the
    // modelled order) and 12 are fixed-size GemmProducts below the
    // coefficient threshold (3x9x3: rows + cols + depth = 15 < 20), i.e.
    // halving-tree reductions.
    assert_eq!((gemm_class, via_gebp, via_coeff), (60, 48, 12));
}

#[test]
fn element_stiffness_shapes_route_through_gebp() {
    // ShapeLagrange::computeBtDB (order 4): B is (voigt × dim·nnodes),
    // dynamic; `B.transpose() * D * B` = (Bᵀ·D) evaluated first, then ·B.
    // (dim, nnodes, voigt) for linear/quadratic triangles, quads, tets, hexes.
    let cases = [
        (2, 3, 3, false, false), // _triangle_3: coefficient-based (12, 15 < 20)
        (2, 4, 3, false, false), // _quadrangle_4: 14, 19 < 20
        (2, 6, 3, false, true),  // _triangle_6: second product 27 ≥ 20
        (2, 8, 3, true, true),   // _quadrangle_8
        (3, 4, 6, true, true),   // _tetrahedron_4
        (3, 8, 6, true, true),   // _hexahedron_8
        (3, 10, 6, true, true),  // _tetrahedron_10
        (3, 20, 6, true, true),  // _hexahedron_20
    ];
    for (dim, nn, t, first_gebp, second_gebp) in cases {
        let n = dim * nn;
        assert_eq!(
            route(n, t, t, false) == Route::Gebp,
            first_gebp,
            "Bᵀ·D {dim}D {nn}"
        );
        assert_eq!(
            route(n, t, n, false) == Route::Gebp,
            second_gebp,
            "(BᵀD)·B {dim}D {nn}"
        );
        // depth = voigt size ≤ 6 < pk = 8: the gebp order is sequential
        // from +0 for every entry (no even/odd split); depth ≤ max_kc = 408
        // (16 KiB default L1, mr = 1, nr = 4): a single depth block even
        // when max(rows, cols) ≥ 48 triggers the blocking heuristic (row and
        // column blocking do not change any entry's order).
        // (t < 8 also implies t ≤ max_kc.)
        assert!(t < 8);
    }
}

/// The depth-block balancing rule (specification, not
/// the kernel): depth 408 is one block, 409 → `kc = 208` (blocks 208 + 201,
/// not 408 + 1), 816 → 408 + 408, 817 → `kc = 280` (280 + 280 + 257); Eigen's
/// own invariant "the number of sweeps is unchanged" holds for every depth
/// up to 4096; small products never block.
#[test]
fn gebp_depth_block_balancing_rule() {
    let blocks = |depth: usize| {
        let kc = gebp_kc(1, 4, depth);
        let mut b = Vec::new();
        let mut k0 = 0;
        while k0 < depth {
            b.push(kc.min(depth - k0));
            k0 += kc;
        }
        b
    };
    assert_eq!(blocks(408), [408]);
    assert_eq!(blocks(409), [208, 201]);
    assert_eq!(blocks(816), [408, 408]);
    assert_eq!(blocks(817), [280, 280, 257]);
    for depth in 409..=4096 {
        let kc = gebp_kc(1, 1, depth);
        assert_eq!(depth / kc, depth / 408, "depth {depth}: sweeps");
        assert_eq!(kc % 8, 0, "depth {depth}: kc multiple of the peeling");
    }
    assert_eq!(gebp_kc(47, 47, 47), 47, "max(rows, cols, depth) < 48");
    assert_eq!(gebp_kc(100, 100, 300), 300, "depth ≤ max_kc");
}

// ---- 2. GEMM tolerance class -----------------------------------------------

#[test]
fn gemm_bound_is_nan_safe_and_exact_outside_its_assumptions() {
    let terms = [1.0, 2.0f64.powi(-60), -1.0];
    let sequential = (terms[0] + terms[1]) + terms[2]; // 0: 2^-60 is absorbed
    let other_order = (terms[0] + terms[2]) + terms[1]; // 2^-60
    assert_eq!(
        gemm_entry(&terms, sequential, sequential),
        Ok(GemmVerdict::Exact)
    );
    // A different order of the same products is within 2 γ_3 Σ|t|.
    assert!(matches!(
        gemm_entry(&terms, other_order, sequential),
        Ok(GemmVerdict::WithinBound(r)) if r > 0.0 && r < 1.0
    ));
    assert!(
        gemm_entry(&terms, 1e-10, sequential).is_err(),
        "beyond the bound"
    );
    let exact = sequential;
    assert!(
        gemm_entry(&terms, f64::NAN, exact).is_err(),
        "NaN error never passes"
    );
    assert_eq!(
        gemm_entry(&terms, f64::NAN, f64::NAN),
        Ok(GemmVerdict::Exact)
    );
    assert!(gemm_entry(&[f64::INFINITY], f64::INFINITY, f64::NEG_INFINITY).is_err());
    let sub = [f64::MIN_POSITIVE / 4.0, 1.0];
    assert!(
        gemm_entry(&sub, 1.0 + f64::EPSILON, 1.0).is_err(),
        "subnormal products: exact match required"
    );
    assert!(
        gemm_entry(&[f64::MAX, f64::MAX], 1.0, 2.0).is_err(),
        "overflowing Σ|t|"
    );
    assert_eq!(
        gemm_entry(&[0.0, -0.0], -0.0, 0.0),
        Ok(GemmVerdict::WithinBound(0.0))
    );
}

// ---- 3. Large-size classes -------------------------------------------------

/// Deterministic pseudo-random numbers in `[-1, 1)` (SplitMix64).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        (z >> 11) as f64 / (1u64 << 52) as f64 - 1.0
    }
}

fn norm_inf(m: &MatrixX) -> f64 {
    (0..m.rows())
        .map(|i| (0..m.cols()).map(|j| m.at(i, j).abs()).sum::<f64>())
        .fold(0.0, NanMinMax::nan_max)
}

fn frob(m: &MatrixX) -> f64 {
    m.as_slice().iter().map(|x| x * x).sum::<f64>().sqrt()
}

fn sub_identity(m: &MatrixX) -> MatrixX {
    let n = m.rows();
    let mut d = m.as_slice().to_vec();
    for i in 0..n {
        d[i + i * n] -= 1.0;
    }
    MatrixX::from_col_major(n, n, d).unwrap()
}

/// Column-diagonally-dominant matrix (partial pivoting performs no row
/// interchange and the growth factor is ≤ 2), optionally graded by
/// `2^(-scale·i)` on rows and columns (condition number up to ~2^(2·scale·n)).
fn diagonally_dominant(n: usize, scale: i32, seed: u64) -> MatrixX {
    let mut rng = Rng(seed);
    let mut d = vec![0.0; n * n];
    for j in 0..n {
        let mut col_sum = 0.0;
        for i in 0..n {
            if i != j {
                let v = rng.next();
                d[i + j * n] = v;
                col_sum += v.abs();
            }
        }
        d[j + j * n] = col_sum + 1.0;
    }
    for j in 0..n {
        for i in 0..n {
            d[i + j * n] *= 2f64.powi(-scale * i as i32) * 2f64.powi(-scale * j as i32);
        }
    }
    MatrixX::from_col_major(n, n, d).unwrap()
}

/// Blocked-LU class (`n > 16`): the inverse computed with partial pivoting
/// satisfies the residual bound `‖X·A − I‖∞ ≤ γ_{3n} · n · ρ · ‖X‖∞ ‖A‖∞`
/// (Higham, Thm. 9.4 / §14.3: `|XA − I| ≤ c_n u |X||L||U|` with
/// `‖|L||U|‖∞ ≤ n ρ ‖A‖∞`, `|l_ij| ≤ 1`), `ρ ≤ 2` for these matrices.
/// Checked on both sides of Eigen's unblocked/blocked switch (16/17) and on
/// graded, ill-conditioned matrices. This is the comparison class versus the
/// oracle for `n > 16` (each side meets it; no generic relative tolerance).
#[test]
fn blocked_lu_class_residual_bound_at_the_boundary() {
    let rho = 2.0;
    let mut worst = 0.0f64;
    for n in [15usize, 16, 17, 24, 33] {
        for (scale, seed) in [(0, 1u64), (1, 2), (2, 3)] {
            let a = diagonally_dominant(n, scale, seed + n as u64);
            let x = a.inverse().unwrap();
            let res = norm_inf(&sub_identity(&x.matmul(&a).unwrap()));
            let bound = gamma(3 * n) * n as f64 * rho * norm_inf(&x) * norm_inf(&a);
            assert!(
                res <= bound,
                "n={n} scale={scale}: ‖XA−I‖ = {res:e} > {bound:e}"
            );
            worst = worst.nan_max(res / bound);
            // Positive pivots (no interchange, dominant positive diagonal):
            // det ≥ 0 (it underflows to +0 for the most graded cases).
            let det = a.det().unwrap();
            assert!(
                det.is_finite() && det >= 0.0,
                "n={n} scale={scale}: det {det:e}"
            );
        }
    }
    eprintln!("blocked-LU class: max residual / bound = {worst:.3e}");
}

/// Orthogonal `Q`: a product of three Householder reflectors.
fn orthogonal(n: usize, seed: u64) -> MatrixX {
    let mut q = MatrixX::identity(n);
    let mut rng = Rng(seed);
    for _ in 0..3 {
        let v: Vec<f64> = (0..n).map(|_| rng.next()).collect();
        let vv: f64 = v.iter().map(|x| x * x).sum();
        let mut h = MatrixX::identity(n).as_slice().to_vec();
        for j in 0..n {
            for i in 0..n {
                h[i + j * n] -= 2.0 * v[i] * v[j] / vv;
            }
        }
        q = MatrixX::from_col_major(n, n, h)
            .unwrap()
            .matmul(&q)
            .unwrap();
    }
    q
}

/// Symmetric matrix `Q diag(λ) Qᵀ` with `Q` a product of Householder
/// reflectors, eigenvalues `lambda` (clusters allowed).
fn symmetric_with_spectrum(lambda: &[f64], seed: u64) -> (MatrixX, MatrixX) {
    let n = lambda.len();
    let q = orthogonal(n, seed);
    let mut ql = q.as_slice().to_vec();
    for j in 0..n {
        for i in 0..n {
            ql[i + j * n] *= lambda[j];
        }
    }
    let a = MatrixX::from_col_major(n, n, ql)
        .unwrap()
        .matmul(&q.transpose())
        .unwrap();
    // Exact symmetry (lower triangle is what eigh reads; mirror it).
    let mut d = a.as_slice().to_vec();
    for j in 0..n {
        for i in 0..j {
            d[i + j * n] = d[j + i * n];
        }
    }
    (MatrixX::from_col_major(n, n, d).unwrap(), q)
}

/// `‖P_V − P_Q‖_F` for the column sets `cols` of `v` and `q`.
fn projector_distance(v: &MatrixX, q: &MatrixX, vcols: &[usize], qcols: &[usize]) -> f64 {
    let n = v.rows();
    let mut s = 0.0;
    for i in 0..n {
        for j in 0..n {
            let pv: f64 = vcols.iter().map(|&c| v.at(i, c) * v.at(j, c)).sum();
            let pq: f64 = qcols.iter().map(|&c| q.at(i, c) * q.at(j, c)).sum();
            s += (pv - pq) * (pv - pq);
        }
    }
    s.sqrt()
}

/// **L2s** — blocked eigenvector-accumulation class (`n > 48`) on the
/// `SelfAdjointEigenSolver` path (`eigh` only): backward-stable residual and
/// orthogonality, eigenvalues within Weyl's bound, and **invariant
/// subspaces** (not individual vectors) for a cluster, on both sides of the
/// 48/49 switch. Thresholds: Householder tridiagonalisation plus implicit
/// QR is normwise backward stable with `‖ΔA‖_F ≤ p(n) u ‖A‖_F`; we use the
/// worst-case `p(n) = 4 n²` (n−2 reflectors of length ≤ n, then ≤ 30 n
/// Givens sweeps — Higham §19.3, Golub & Van Loan §8.3), plus the
/// construction error of `A` (three reflectors and two products:
/// `≤ 8 n u ‖A‖_F`). Subspaces: Davis–Kahan `sin Θ ≤ ‖ΔA‖ / gap`, so
/// `‖P_V − P_Q‖_F ≤ √2 · √m · ‖ΔA‖_F / gap` for a cluster of size `m`.
#[test]
fn eigenvector_class_residual_orthogonality_and_cluster_subspaces() {
    let (mut w_res, mut w_orth, mut w_val, mut w_sub) = (0.0f64, 0.0f64, 0.0f64, 0.0f64);
    for n in [16usize, 17, 48, 49, 64] {
        // A 4-fold cluster at 3.0 (gap 1), the rest spread in [5, 15).
        let cluster = [0usize, 1, 2, 3];
        let lambda: Vec<f64> = (0..n)
            .map(|i| {
                if i < 4 {
                    3.0
                } else {
                    5.0 + 10.0 * (i as f64) / (n as f64)
                }
            })
            .collect();
        let (a, q) = symmetric_with_spectrum(&lambda, 7 + n as u64);
        let (values, v) = dense::eigh_vectors(&a, Storage::Dynamic, SortOrder::Natural).unwrap();
        let na = frob(&a);
        let nf = n as f64;
        let delta = (4.0 * nf * nf + 8.0 * nf) * U * na;
        // residual ‖A V − V Λ‖_F
        let av = a.matmul(&v).unwrap();
        let mut r = 0.0;
        for (j, lam) in values.iter().enumerate() {
            for i in 0..n {
                let e = av.at(i, j) - v.at(i, j) * lam;
                r += e * e;
            }
        }
        assert!(
            r.sqrt() <= delta,
            "n={n}: residual {:e} > {delta:e}",
            r.sqrt()
        );
        w_res = w_res.nan_max(r.sqrt() / delta);
        // ‖VᵀV − I‖_F ≤ p(n) u with the same p(n) = 4 n² (orthogonal
        // transformations accumulated in floating point).
        let orth = frob(&sub_identity(&v.transpose().matmul(&v).unwrap()));
        let orth_bound = 4.0 * nf * nf * U;
        assert!(
            orth <= orth_bound,
            "n={n}: ‖VᵀV−I‖ = {orth:e} > {orth_bound:e}"
        );
        w_orth = w_orth.nan_max(orth / orth_bound);
        // ascending natural order; Weyl: |λ_i − λ̂_i| ≤ ‖ΔA‖_2 ≤ ‖ΔA‖_F
        let mut sorted = lambda.clone();
        sorted.sort_by(f64::total_cmp);
        for (got, want) in values.iter().zip(&sorted) {
            assert!((got - want).abs() <= delta, "n={n}: λ {got} vs {want}");
            w_val = w_val.nan_max((got - want).abs() / delta);
        }
        // cluster subspace (natural ascending order: the first 4 columns)
        let d = projector_distance(&v, &q, &cluster, &cluster);
        let bound = 2f64.sqrt() * 2.0 * delta / 1.0;
        assert!(d <= bound, "n={n}: cluster subspace {d:e} > {bound:e}");
        w_sub = w_sub.nan_max(d / bound);
    }
    eprintln!(
        "eigenvector class: max ratio residual {w_res:.3e}, orthogonality {w_orth:.3e}, \
         eigenvalues {w_val:.3e}, cluster subspace {w_sub:.3e}"
    );
}

/// **L3s** — sort-tie class on `eigh` (`n > 16`, libstdc++ introsort; not
/// portable under D-030): the descending value sequence is identical
/// whatever the tie order, and eigenvector columns are compared per tie
/// group through the group's projector (orthonormal columns: `VVᵀ` is the
/// orthogonal projector here; the general class uses an orthonormalised
/// basis, see `general_sort_tie_class_*`). Checked at 16 (insertion sort
/// upstream) and 17/24.
#[test]
fn sort_tie_class_compares_values_bitwise_and_tie_groups_by_projector() {
    for n in [16usize, 17, 24] {
        // Exact ties: groups of three equal eigenvalues.
        let lambda: Vec<f64> = (0..n).map(|i| (i / 3) as f64 + 1.0).collect();
        let (a, _) = symmetric_with_spectrum(&lambda, 11 + n as u64);
        let (nat, vn) = dense::eigh_vectors(&a, Storage::Dynamic, SortOrder::Natural).unwrap();
        let (desc, vd) = dense::eigh_vectors(&a, Storage::Dynamic, SortOrder::Descending).unwrap();
        // Values: the descending sequence is the reversed natural one,
        // bit for bit (a permutation within a tie group cannot change it
        // when the tied values are bit-identical; otherwise there is no tie).
        let mut rev = nat.clone();
        rev.reverse();
        let mut sorted_desc = desc.clone();
        sorted_desc.sort_by(|x, y| y.total_cmp(x));
        assert_eq!(
            desc.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            sorted_desc.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            "n={n}: descending"
        );
        assert_eq!(
            sorted_desc.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            {
                let mut s = rev.clone();
                s.sort_by(|x, y| y.total_cmp(x));
                s.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
            },
            "n={n}: same multiset"
        );
        // Vectors: group columns by bit-identical value and compare the
        // group projectors of the two orders.
        let mut i = 0;
        while i < n {
            let mut j = i + 1;
            while j < n && desc[j].to_bits() == desc[i].to_bits() {
                j += 1;
            }
            let dcols: Vec<usize> = (i..j).collect();
            let ncols: Vec<usize> = (0..n)
                .filter(|&c| nat[c].to_bits() == desc[i].to_bits())
                .collect();
            assert_eq!(ncols.len(), dcols.len());
            let dist = projector_distance(&vd, &vn, &dcols, &ncols);
            assert_eq!(dist, 0.0, "n={n}: same columns, permuted only");
            i = j;
        }
    }
}

// ---- 3b. General (non-self-adjoint) eigenvectors: class L2g -------

/// Column-major `n × n` from a closure.
fn mat(n: usize, f: impl Fn(usize, usize) -> f64) -> MatrixX {
    let mut d = vec![0.0; n * n];
    for j in 0..n {
        for i in 0..n {
            d[i + j * n] = f(i, j);
        }
    }
    MatrixX::from_col_major(n, n, d).unwrap()
}

/// Inverse of an upper-triangular `m × m` matrix (column-major) by back
/// substitution.
fn upper_inverse(m: usize, a: &[f64]) -> Vec<f64> {
    let mut x = vec![0.0; m * m];
    for c in 0..m {
        for i in (0..=c).rev() {
            let mut s = if i == c { 1.0 } else { 0.0 };
            for k in i + 1..=c {
                s -= a[i + k * m] * x[k + c * m];
            }
            x[i + c * m] = s / a[i + i * m];
        }
    }
    x
}

fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Lower bound of `sep(T11, T22)` (Stewart; Golub & Van Loan §7.2.4) for a
/// leading block of size `k` of the upper-triangular `t`, when one of the
/// two blocks is **diagonal**: the Sylvester operator `X ↦ T11·X − X·T22`
/// then decouples into `min_i σ_min(a_i I − T22)` (diagonal `T11`) or
/// `min_j σ_min(T11 − b_j I)` (diagonal `T22`), and `σ_min(M) ≥ 1/‖M⁻¹‖_F`.
/// The factor 0.99 covers the rounding of the computed inverses (all
/// blocks used here are well conditioned).
fn sep_lower_bound(t: &MatrixX, k: usize) -> f64 {
    let n = t.rows();
    let m = n - k;
    let t11_diagonal = (0..k).all(|j| (0..j).all(|i| t.at(i, j) == 0.0));
    let mut sep = f64::INFINITY;
    if t11_diagonal {
        for i in 0..k {
            let shifted: Vec<f64> = (0..m * m)
                .map(|idx| {
                    let (r, c) = (idx % m, idx / m);
                    let tv = t.at(k + r, k + c);
                    if r == c { t.at(i, i) - tv } else { -tv }
                })
                .collect();
            sep = sep.nan_min(1.0 / norm2(&upper_inverse(m, &shifted)));
        }
    } else {
        assert!(
            (k..n).all(|j| (k..j).all(|i| t.at(i, j) == 0.0)),
            "one diagonal block required"
        );
        for j in k..n {
            let b = t.at(j, j);
            let shifted = mat(k, |r, c| t.at(r, c) - if r == c { b } else { 0.0 });
            sep = sep.nan_min(1.0 / norm2(shifted.inverse().unwrap().as_slice()));
        }
    }
    0.99 * sep
}

/// `‖(I − Q_k Q_kᵀ) v‖₂` for the first `k` columns of `q` (`v` real).
fn dist_from_leading_span(q: &MatrixX, k: usize, v: &[f64]) -> f64 {
    let n = q.rows();
    let mut r = v.to_vec();
    for c in 0..k {
        let dot: f64 = (0..n).map(|i| q.at(i, c) * v[i]).sum();
        for (i, ri) in r.iter_mut().enumerate() {
            *ri -= dot * q.at(i, c);
        }
    }
    norm2(&r)
}

/// Orthonormal basis (modified Gram–Schmidt, twice) of the columns `cols`
/// of `v`, plus `‖R⁻¹‖_F` (`σ_min` of the column set is `≥ 1/‖R⁻¹‖_F`).
fn orthonormalise(v: &MatrixX, cols: &[usize]) -> (Vec<Vec<f64>>, f64) {
    let n = v.rows();
    let k = cols.len();
    let mut basis: Vec<Vec<f64>> = Vec::with_capacity(k);
    let mut r = vec![0.0; k * k];
    for (jj, &c) in cols.iter().enumerate() {
        let mut w: Vec<f64> = (0..n).map(|i| v.at(i, c)).collect();
        for _ in 0..2 {
            for (ii, b) in basis.iter().enumerate() {
                let dot: f64 = b.iter().zip(&w).map(|(x, y)| x * y).sum();
                r[ii + jj * k] += dot;
                for (wi, bi) in w.iter_mut().zip(b) {
                    *wi -= dot * bi;
                }
            }
        }
        let nw = norm2(&w);
        r[jj + jj * k] = nw;
        basis.push(w.iter().map(|x| x / nw).collect());
    }
    let rinv = norm2(&upper_inverse(k, &r));
    (basis, rinv)
}

/// `‖ÛÛᵀ − Q_kQ_kᵀ‖_F` for an orthonormal basis `u` (the orthogonal
/// projector of a **general** column set is `ÛÛᵀ`, not `VVᵀ`).
fn projector_distance_to_leading(u: &[Vec<f64>], q: &MatrixX, k: usize) -> f64 {
    let n = q.rows();
    let mut s = 0.0;
    for i in 0..n {
        for j in 0..n {
            let pu: f64 = u.iter().map(|b| b[i] * b[j]).sum();
            let pq: f64 = (0..k).map(|c| q.at(i, c) * q.at(j, c)).sum();
            s += (pu - pq) * (pu - pq);
        }
    }
    s.sqrt()
}

/// **L2g** — general eigenvectors (`EigenSolver`, `eig_vectors`) at
/// `n ∈ {16, 17, 48, 49, 64}` (both sides of the blocked accumulation
/// switch), on **non-normal** matrices `A = Q T Qᵀ` with
/// `T = [[diag(a), T12], [0, T22]]`: a 4-fold near-cluster `a` (gaps 1e-3,
/// individually ill-conditioned eigenvectors), a dense coupling `T12` and a
/// non-normal bidiagonal `T22`. Criteria (`δ = (4n² + 8n) u ‖A‖_F`: QR
/// backward error plus the construction error of `A`):
///
/// 1. backward residual per eigenpair `‖A v̂ − λ̂ v̂‖₂ ≤ 4n² u ‖A‖_F ‖v̂‖₂`;
/// 2. eigenvalues: `|λ̂_j − λ_j| ≤ 2 κ_j δ` with `κ_j = ‖y_j‖‖x_j‖/|y_jᴴx_j|`
///    estimated from the rows of `V̂⁻¹` — first order, applied only in its
///    regime `2 κ_j δ ≤ gap_j / 4` (asserted);
/// 3. **no orthogonality requirement** (`‖V̂ᵀV̂ − I‖_F` is O(1) here);
/// 4. invariant subspace of the cluster (Stewart, Golub & Van Loan
///    Thm. 7.2.4, needs `δ ≤ sep/5`): every computed cluster vector lies
///    within `4δ/sep` of `span(Q_k)`, and the orthogonal projector of the
///    orthonormalised cluster vectors is within `√2 √k (4δ/sep) ‖R⁻¹‖_F`
///    of `Q_kQ_kᵀ`.
#[test]
fn general_eigenvector_class_residuals_conditioning_and_invariant_subspaces() {
    let (mut w_res, mut w_val, mut w_vec, mut w_proj, mut min_orth) =
        (0.0f64, 0.0f64, 0.0f64, 0.0f64, f64::INFINITY);
    for n in [16usize, 17, 48, 49, 64] {
        let k = 4;
        let mut rng = Rng(100 + n as u64);
        let mut td = vec![0.0; n * n];
        for i in 0..k {
            td[i + i * n] = 1.0 + 1e-3 * i as f64;
        }
        for j in k..n {
            td[j + j * n] = 5.0 + 10.0 * (j - k) as f64 / (n - k) as f64;
            for i in 0..k {
                td[i + j * n] = 0.5 * rng.next();
            }
            if j > k {
                td[(j - 1) + j * n] = 0.05 * rng.next();
            }
        }
        let t = MatrixX::from_col_major(n, n, td).unwrap();
        let q = orthogonal(n, 300 + n as u64);
        let a = q.matmul(&t).unwrap().matmul(&q.transpose()).unwrap();
        let (values, v) = dense::eig_vectors(&a, Storage::Dynamic, SortOrder::Natural).unwrap();
        let (na, nf) = (frob(&a), n as f64);
        let rho = 4.0 * nf * nf * U * na;
        let delta = rho + 8.0 * nf * U * na;
        // 1. residuals
        let av = a.matmul(&v).unwrap();
        for (j, &lam) in values.iter().enumerate() {
            let r: Vec<f64> = (0..n).map(|i| av.at(i, j) - lam * v.at(i, j)).collect();
            let vn = norm2(&(0..n).map(|i| v.at(i, j)).collect::<Vec<_>>());
            assert!(norm2(&r) <= rho * vn, "n={n} j={j}: residual");
            w_res = w_res.nan_max(norm2(&r) / (rho * vn));
        }
        // 2. eigenvalues through their condition numbers
        let vinv = v.inverse().unwrap();
        let mut exact: Vec<f64> = (0..n).map(|i| t.at(i, i)).collect();
        exact.sort_by(f64::total_cmp);
        let mut order: Vec<usize> = (0..n).collect();
        order.sort_by(|&x, &y| values[x].total_cmp(&values[y]));
        for (rank, &j) in order.iter().enumerate() {
            let kappa = norm2(&(0..n).map(|c| vinv.at(j, c)).collect::<Vec<_>>())
                * norm2(&(0..n).map(|i| v.at(i, j)).collect::<Vec<_>>());
            let gap = exact
                .iter()
                .enumerate()
                .filter(|(i, _)| *i != rank)
                .map(|(_, e)| (e - exact[rank]).abs())
                .fold(f64::INFINITY, NanMinMax::nan_min);
            let bound = 2.0 * kappa * delta;
            assert!(
                bound <= gap / 4.0,
                "n={n}: first-order regime (κ={kappa:e})"
            );
            let err = (values[j] - exact[rank]).abs();
            assert!(err <= bound, "n={n}: λ {} vs {}", values[j], exact[rank]);
            w_val = w_val.nan_max(err / bound);
        }
        // 3. general eigenvectors are not orthogonal
        let orth = frob(&sub_identity(&v.transpose().matmul(&v).unwrap()));
        min_orth = min_orth.nan_min(orth);
        // 4. invariant subspace of the cluster
        let sep = sep_lower_bound(&t, k);
        assert!(delta <= sep / 5.0, "n={n}: Stewart's condition");
        let eps = 4.0 * delta / sep;
        let cluster: Vec<usize> = order[..k].to_vec();
        for &j in &cluster {
            let col: Vec<f64> = (0..n).map(|i| v.at(i, j)).collect();
            let d = dist_from_leading_span(&q, k, &col) / norm2(&col);
            assert!(d <= eps, "n={n}: cluster vector {d:e} > {eps:e}");
            w_vec = w_vec.nan_max(d / eps);
        }
        let (basis, rinv) = orthonormalise(&v, &cluster);
        let pd = projector_distance_to_leading(&basis, &q, k);
        let pbound = 2f64.sqrt() * (k as f64).sqrt() * eps * rinv;
        assert!(pd <= pbound, "n={n}: cluster projector {pd:e} > {pbound:e}");
        w_proj = w_proj.nan_max(pd / pbound);
    }
    assert!(min_orth > 1e-2, "non-normal test matrices: VᵀV ≠ I");
    eprintln!(
        "L2g: max ratio residual {w_res:.3e}, eigenvalues {w_val:.3e}, cluster vectors \
         {w_vec:.3e}, cluster projector {w_proj:.3e}; min ‖VᵀV−I‖_F {min_orth:.3e}"
    );
}

/// Counterexample: `diag(1, …, 49)` with
/// `A(0, 1) = 1` (the 2 × 2 upper-triangular block crosses the 48/49
/// boundary without being ill-conditioned). `‖VᵀV − I‖_F ≈ 1`: the
/// self-adjoint orthogonality criterion (bound `1.07e−12` at n = 49) does
/// **not** apply to general eigenvectors — mathematically expected — while
/// every L2g criterion holds.
#[test]
fn reviewer_counterexample_general_eigenvectors_are_not_orthogonal() {
    let n = 49;
    let a = mat(n, |i, j| {
        if i == j {
            (i + 1) as f64
        } else if (i, j) == (0, 1) {
            1.0
        } else {
            0.0
        }
    });
    let (values, v) = dense::eig_vectors(&a, Storage::Dynamic, SortOrder::Natural).unwrap();
    let orth = frob(&sub_identity(&v.transpose().matmul(&v).unwrap()));
    let l2s_bound = 4.0 * (n * n) as f64 * U;
    assert!(orth > 0.5 && orth > 1e9 * l2s_bound, "‖VᵀV−I‖ = {orth}");
    let (na, nf) = (frob(&a), n as f64);
    let rho = 4.0 * nf * nf * U * na;
    let av = a.matmul(&v).unwrap();
    for (j, &lam) in values.iter().enumerate() {
        let r: Vec<f64> = (0..n).map(|i| av.at(i, j) - lam * v.at(i, j)).collect();
        assert!(norm2(&r) <= rho, "residual {j}");
    }
    let vinv = v.inverse().unwrap();
    for (j, &lam) in values.iter().enumerate() {
        let exact = lam.round();
        let kappa = norm2(&(0..n).map(|c| vinv.at(j, c)).collect::<Vec<_>>());
        assert!(
            (lam - exact).abs() <= 2.0 * kappa * rho,
            "λ {lam} (κ {kappa})"
        );
    }
    // The invariant subspace of {1, 2} is span(e0, e1) (Q = I).
    let sep = sep_lower_bound(&a, 2);
    let eps = 4.0 * rho / sep;
    for (j, &lam) in values.iter().enumerate() {
        if lam < 2.5 {
            let col: Vec<f64> = (0..n).map(|i| v.at(i, j)).collect();
            let d = dist_from_leading_span(&MatrixX::identity(n), 2, &col);
            assert!(d <= eps, "vector of λ={lam}: {d:e}");
        }
    }
}

/// Complex arithmetic for the Hermitian checks below.
fn cmul(a: (f64, f64), b: (f64, f64)) -> (f64, f64) {
    (a.0 * b.0 - a.1 * b.1, a.0 * b.1 + a.1 * b.0)
}

/// **L2g, complex and defective spectra** at n ∈ {17, 49}: `T11` is either
/// a normal 2 × 2 block with eigenvalues `a ± ib` (complex eigenvectors) or
/// a 2 × 2 **Jordan block** (defective: no basis of eigenvectors exists),
/// `T22` diagonal, `T12` dense. Hermitian norms throughout. Criteria:
/// residuals `‖A v̂ − λ̂ v̂‖₂ ≤ 4n² u ‖A‖_F` (unit `v̂`); eigenvalues by the
/// Bauer–Fike bound for a Jordan form `A = X J X⁻¹` with largest block `m`
/// (`min |λ − μ|^m / (1 + |λ − μ|)^{m−1} ≤ κ₂(X) δ`, hence
/// `min |λ − μ| ≤ max(κδ, 2κδ)` for m = 1 and `max(√(2κδ), 2κδ)` for m = 2),
/// with `κ₂(X) ≤ (1 + ‖T12‖_F / sep)²` from the block diagonalisation of
/// `T`; and every computed vector of the block within `4δ/sep` of the real
/// invariant subspace `span(Q_2)` (Stewart). For the defective block the
/// two computed vectors are nearly parallel: only the invariant subspace is
/// compared, never a basis.
#[test]
fn general_eigenvector_class_complex_and_defective_blocks() {
    for (defective, n) in [(false, 17usize), (false, 49), (true, 17), (true, 49)] {
        let k = 2;
        let mut rng = Rng(500 + n as u64 + u64::from(defective));
        let mut td = vec![0.0; n * n];
        if defective {
            td[0] = 2.0;
            td[1 + n] = 2.0;
            td[n] = 1.0; // T(0, 1): the Jordan coupling
        } else {
            td[0] = 2.0;
            td[1 + n] = 2.0;
            td[n] = 0.75; // T(0, 1)
            td[1] = -0.75; // T(1, 0): eigenvalues 2 ± 0.75i
        }
        for j in k..n {
            td[j + j * n] = 5.0 + 10.0 * (j - k) as f64 / (n - k) as f64;
            for i in 0..k {
                td[i + j * n] = 0.5 * rng.next();
            }
        }
        let t = MatrixX::from_col_major(n, n, td).unwrap();
        let q = orthogonal(n, 700 + n as u64);
        let a = q.matmul(&t).unwrap().matmul(&q.transpose()).unwrap();
        let (values, cols) = dense::eig_vectors_complex(&a, Storage::Dynamic).unwrap();
        let (na, nf) = (frob(&a), n as f64);
        let rho = 4.0 * nf * nf * U * na;
        let delta = rho + 8.0 * nf * U * na;
        let sep = sep_lower_bound(&t, k);
        assert!(delta <= sep / 5.0);
        let t12: f64 = (k..n)
            .flat_map(|j| (0..k).map(move |i| (i, j)))
            .map(|(i, j)| t.at(i, j) * t.at(i, j))
            .sum::<f64>()
            .sqrt();
        let kappa = (1.0 + t12 / sep).powi(2);
        let m = if defective { 2 } else { 1 };
        let kd = kappa * delta;
        let ev_bound = if m == 1 {
            kd.max(2.0 * kd)
        } else {
            (2.0 * kd).sqrt().max(2.0 * kd)
        };
        let exact: Vec<(f64, f64)> = (0..n)
            .map(|i| match (i, defective) {
                (0 | 1, true) => (2.0, 0.0),
                (0, false) => (2.0, 0.75),
                (1, false) => (2.0, -0.75),
                _ => (t.at(i, i), 0.0),
            })
            .collect();
        let mut block_vectors = Vec::new();
        for (lam, col) in values.iter().zip(&cols) {
            // residual (complex)
            let mut r2 = 0.0;
            for i in 0..n {
                let mut s = (0.0, 0.0);
                for (c, z) in col.iter().enumerate() {
                    s.0 += a.at(i, c) * z.re;
                    s.1 += a.at(i, c) * z.im;
                }
                let lv = cmul((lam.re, lam.im), (col[i].re, col[i].im));
                r2 += (s.0 - lv.0).powi(2) + (s.1 - lv.1).powi(2);
            }
            assert!(r2.sqrt() <= rho, "n={n} defective={defective}: residual");
            // eigenvalue (Bauer–Fike / Jordan form)
            let d = exact
                .iter()
                .map(|e| ((lam.re - e.0).powi(2) + (lam.im - e.1).powi(2)).sqrt())
                .fold(f64::INFINITY, NanMinMax::nan_min);
            assert!(
                d <= ev_bound,
                "n={n}: eigenvalue {lam:?}: {d:e} > {ev_bound:e}"
            );
            // vectors of the block: in the real invariant subspace
            if (lam.re - 2.0).abs() < 0.5 {
                let re: Vec<f64> = col.iter().map(|z| z.re).collect();
                let im: Vec<f64> = col.iter().map(|z| z.im).collect();
                let dist = (dist_from_leading_span(&q, k, &re).powi(2)
                    + dist_from_leading_span(&q, k, &im).powi(2))
                .sqrt();
                assert!(dist <= 4.0 * delta / sep, "n={n}: subspace {dist:e}");
                block_vectors.push(col.clone());
            }
        }
        assert_eq!(block_vectors.len(), 2);
        // |v₁ᴴ v₂| for the two block vectors.
        let (x, y) = (&block_vectors[0], &block_vectors[1]);
        let dot = x.iter().zip(y).fold((0.0, 0.0), |acc, (p, q)| {
            let z = cmul((p.re, -p.im), (q.re, q.im));
            (acc.0 + z.0, acc.1 + z.1)
        });
        let cos = (dot.0 * dot.0 + dot.1 * dot.1).sqrt();
        if defective {
            assert!(
                cos > 1.0 - 1e-6,
                "defective: computed vectors nearly parallel ({cos})"
            );
        } else {
            assert!(cos < 0.5, "normal 2×2 block: independent vectors ({cos})");
        }
    }
}

/// **L3g** — sort ties with **general** eigenvectors (`n > 16`): an upper
/// triangular, non-normal input with exactly tied diagonal groups (decoupled
/// inside a group, coupled across groups; max entry a power of two, so
/// Eigen's scaling round-trips exactly) gives bit-identical tied values.
/// The descending sequence is the reversed natural multiset bit for bit;
/// tie groups are compared through the **orthogonal projector of an
/// orthonormalised basis** of the group's columns — `VVᵀ` is not a
/// projector for non-orthonormal columns (shown below).
#[test]
fn general_sort_tie_class_compares_tie_groups_by_orthonormalised_projector() {
    for n in [16usize, 17, 24] {
        let group = |i: usize| i / 3;
        let mut rng = Rng(900 + n as u64);
        let a = {
            let mut d = vec![0.0; n * n];
            for j in 0..n {
                d[j + j * n] = (group(j) + 1) as f64;
                for i in 0..j {
                    if group(i) != group(j) {
                        d[i + j * n] = 0.5 * rng.next();
                    }
                }
            }
            d[n * n - 1] = 8.0; // max |entry| a power of two
            MatrixX::from_col_major(n, n, d).unwrap()
        };
        let (nat, vn) = dense::eig_vectors(&a, Storage::Dynamic, SortOrder::Natural).unwrap();
        let (desc, vd) = dense::eig_vectors(&a, Storage::Dynamic, SortOrder::Descending).unwrap();
        let bits = |x: &[f64]| {
            let mut b: Vec<u64> = x.iter().map(|v| v.to_bits()).collect();
            b.sort_unstable();
            b
        };
        assert_eq!(bits(&nat), bits(&desc), "n={n}: same multiset, bitwise");
        assert!(desc.windows(2).all(|w| w[0] >= w[1]), "n={n}: descending");
        let mut i = 0;
        let mut max_vvt_gap = 0.0f64;
        while i < n {
            let mut j = i + 1;
            while j < n && desc[j].to_bits() == desc[i].to_bits() {
                j += 1;
            }
            let dcols: Vec<usize> = (i..j).collect();
            let ncols: Vec<usize> = (0..n)
                .filter(|&c| nat[c].to_bits() == desc[i].to_bits())
                .collect();
            assert_eq!(ncols.len(), dcols.len());
            let (ud, _) = orthonormalise(&vd, &dcols);
            let (un, _) = orthonormalise(&vn, &ncols);
            let mut dist = 0.0;
            let mut vvt_gap = 0.0;
            for r in 0..n {
                for c in 0..n {
                    let pd: f64 = ud.iter().map(|b| b[r] * b[c]).sum();
                    let pn: f64 = un.iter().map(|b| b[r] * b[c]).sum();
                    let vvt: f64 = dcols.iter().map(|&k| vd.at(r, k) * vd.at(c, k)).sum();
                    dist += (pd - pn).powi(2);
                    vvt_gap += (vvt - pd).powi(2);
                }
            }
            assert!(dist.sqrt() <= 64.0 * n as f64 * U, "n={n}: group projector");
            max_vvt_gap = max_vvt_gap.nan_max(vvt_gap.sqrt());
            i = j;
        }
        assert!(
            max_vvt_gap > 0.1,
            "n={n}: VVᵀ differs from the orthogonal projector of a non-orthonormal group"
        );
    }
}

// ---- 3c. L1 determinant: first-order regime and underflow ---------

/// Double-double number (Dekker/Knuth error-free transformations, no FMA).
#[derive(Clone, Copy, Debug)]
struct Dd(f64, f64);

fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let bb = s - a;
    (s, (a - (s - bb)) + (b - bb))
}

fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a; // 2^27 + 1
    let hi = c - (c - a);
    (hi, a - hi)
}

fn two_prod(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let ((ah, al), (bh, bl)) = (split(a), split(b));
    (p, ((ah * bh - p) + ah * bl + al * bh) + al * bl)
}

impl Dd {
    fn add(self, o: Dd) -> Dd {
        let (s, e) = two_sum(self.0, o.0);
        let (hi, lo) = two_sum(s, e + self.1 + o.1);
        Dd(hi, lo)
    }
    fn neg(self) -> Dd {
        Dd(-self.0, -self.1)
    }
    fn mul(self, o: Dd) -> Dd {
        let (p, e) = two_prod(self.0, o.0);
        let (hi, lo) = two_sum(p, e + self.0 * o.1 + self.1 * o.0);
        Dd(hi, lo)
    }
    fn div(self, o: Dd) -> Dd {
        let q1 = self.0 / o.0;
        let r = self.add(o.mul(Dd(q1, 0.0)).neg());
        let q2 = r.0 / o.0;
        let (hi, lo) = two_sum(q1, q2);
        Dd(hi, lo)
    }
}

/// Reference determinant (double-double LU without pivoting — identical to
/// partial pivoting for these column-diagonally-dominant matrices) and the
/// measured growth factor `max|u_ij| / max|a_ij|`.
fn dd_det_and_growth(a: &MatrixX) -> (Dd, f64) {
    let n = a.rows();
    let mut m: Vec<Dd> = a.as_slice().iter().map(|&x| Dd(x, 0.0)).collect();
    let amax = a.as_slice().iter().fold(0.0f64, |x, y| x.nan_max(y.abs()));
    let mut umax = 0.0f64;
    let mut det = Dd(1.0, 0.0);
    for k in 0..n {
        let p = m[k + k * n];
        det = det.mul(p);
        for j in k..n {
            umax = umax.nan_max(m[k + j * n].0.abs());
        }
        for i in k + 1..n {
            let l = m[i + k * n].div(p);
            for j in k + 1..n {
                m[i + j * n] = m[i + j * n].add(l.mul(m[k + j * n]).neg());
            }
        }
    }
    (det, umax / amax)
}

/// **L1 determinant**: the relative bound
/// `|Δdet/det| ≤ 2η + γ_n`, `η = n κ_F(A) γ_{3n} ρ`, follows from
/// `d log det = tr(A⁻¹ dA)` and the LU backward error; it is **first
/// order** and applied only when `η ≤ 1/2` (then `e^η − 1 ≤ 2η`; `γ_n`
/// covers the product of the pivots). Outside that regime no relative
/// criterion is claimed: only the sign / class of the result (here: finite
/// and `≥ 0`, positive pivots). A determinant that underflows (zero or
/// subnormal) has no meaningful relative error: both sides must agree on
/// the class (zero, subnormal) and sign. The growth factor `ρ ≤ 2` used by
/// the bound is **measured** on the reference factorisation. Pivot-sequence
/// equality is an oracle-comparison criterion and is not tested here.
#[test]
fn blocked_lu_determinant_relative_error_in_its_first_order_regime() {
    let (mut in_regime, mut out_of_regime, mut underflow, mut worst) = (0, 0, 0, 0.0f64);
    for n in [15usize, 16, 17, 24, 33] {
        for (scale, seed) in [(0, 1u64), (1, 2), (2, 3)] {
            let a = diagonally_dominant(n, scale, seed + n as u64);
            let det = a.det().unwrap();
            let (reference, rho) = dd_det_and_growth(&a);
            assert!(rho <= 2.0, "n={n} scale={scale}: measured growth {rho}");
            let ainv = a.inverse().unwrap();
            let kappa = frob(&a) * frob(&ainv);
            let eta = n as f64 * kappa * gamma(3 * n) * rho;
            if reference.0 == 0.0 || reference.0.abs() < f64::MIN_POSITIVE {
                underflow += 1;
                assert!(
                    det == 0.0 || det.abs() < f64::MIN_POSITIVE,
                    "underflow class: {det:e} vs {:e}",
                    reference.0
                );
                assert!(!det.is_sign_negative() || det == 0.0);
            } else if eta.is_finite() && eta <= 0.5 {
                in_regime += 1;
                let rel = ((det - reference.0) - reference.1).abs() / reference.0.abs();
                let bound = 2.0 * eta + gamma(n);
                assert!(rel <= bound, "n={n} scale={scale}: {rel:e} > {bound:e}");
                worst = worst.nan_max(rel / bound);
            } else {
                out_of_regime += 1;
                assert!(det.is_finite() && det >= 0.0, "class/sign only: {det:e}");
            }
        }
    }
    assert!(
        in_regime >= 5 && out_of_regime >= 1,
        "{in_regime}/{out_of_regime}"
    );
    eprintln!(
        "L1 determinant: {in_regime} in the first-order regime (max ratio {worst:.3e}), \
         {out_of_regime} outside (sign/class only), {underflow} underflowed"
    );
}
