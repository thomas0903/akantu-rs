// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// material_inline_impl.hh (gradUToF, gradUToEpsilon, gradUToE, rightCauchy,
// leftCauchy, StoCauchy, stressToVonMises, setCauchyStressMatrix),
// material.hh (computeDeviatoric, getTangentStiffnessVoigtSize).

//! Static tensor helpers of upstream `Material`, written with Eigen 3.4's
//! strict-build evaluation order (decision D-008; `akantu_core::eigen_order`):
//! element-wise expressions are evaluated coefficient by coefficient with
//! the operand order of the upstream expression, reductions over
//! compile-time sizes use the halving tree, products of compile-time sizes
//! go through [`akantu_core::tensor::MatrixExt::matmul`] /
//! [`akantu_core::tensor::MatrixExt::tr_matmul`] (the dense product routes).
//!
//! Matrices are column-major (`m[col][row]`), as the per-quadrature-point
//! tuples of the internal fields (`dim × dim`, component `r + c·dim` is
//! `M(r, c)`).

use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt};
use akantu_core::{Error, Real, Result};

/// Upstream `Material::getTangentStiffnessVoigtSize(dim)`:
/// `dim·(dim − 1)/2 + dim`.
#[must_use]
pub const fn tangent_stiffness_voigt_size(dim: usize) -> usize {
    dim * (dim - 1) / 2 + dim
}

/// Upstream `Material::stressToVonMises(stress)` (SMM-028): Von Mises
/// stress of a column-major `dim × dim` stress (the trace divided by 3 in
/// every dimension, as upstream). The single implementation is the dump
/// pipeline's (`Von Mises stress` field, upstream's only caller
/// `ComputeVonMisesStress`), compared bitwise with the oracle by the dump
/// differentials.
pub use akantu_io::dumper::stress_to_von_mises;

/// Upstream `Material::setCauchyStressMatrix<dim>(S_t, sigma)` (SMM-028;
/// Bathe et al. 1975, p. 364): `sigma` (column-major `dim² × dim²`) zeroed,
/// then `sigma(i·dim + m, i·dim + n) = S_t(m, n)` (`S_t` column-major
/// `dim × dim`). Used by the total-Lagrangian stiffness (SMM-023).
///
/// # Errors
/// `s_t` with fewer than `dim²` or `sigma` with fewer than `dim⁴` values
/// (nothing is written then).
pub fn set_cauchy_stress_matrix(dim: usize, s_t: &[Real], sigma: &mut [Real]) -> Result<()> {
    let d2 = dim * dim;
    for (context, expected, got) in [
        ("setCauchyStressMatrix: S_t values", d2, s_t.len()),
        ("setCauchyStressMatrix: sigma values", d2 * d2, sigma.len()),
    ] {
        if got < expected {
            return Err(Error::DimensionMismatch {
                context,
                expected,
                got,
            });
        }
    }
    sigma.fill(0.0);
    for i in 0..dim {
        for m in 0..dim {
            for n in 0..dim {
                sigma[(i * dim + m) + (i * dim + n) * d2] = s_t[m + n * dim];
            }
        }
    }
    Ok(())
}

/// Identity coefficient as Eigen's `Identity()` expression yields it.
#[inline(always)]
fn id(i: usize, j: usize) -> Real {
    if i == j { 1.0 } else { 0.0 }
}

/// `Material::gradUToEpsilon`: `ε = .5 · (∇uᵀ + ∇u)`, coefficient
/// `0.5 · (∇u(j, i) + ∇u(i, j))`.
#[must_use]
pub fn grad_u_to_epsilon<const D: usize>(g: &Matrix<D, D>) -> Matrix<D, D> {
    std::array::from_fn(|j| std::array::from_fn(|i| 0.5 * (g[i][j] + g[j][i])))
}

/// `Material::gradUToE` (Green–Lagrange strain):
/// `E = (∇uᵀ·∇u + ∇uᵀ + ∇u) / 2` — the product is evaluated first (a
/// temporary), then `((P + ∇uᵀ) + ∇u) / 2` per coefficient.
#[must_use]
pub fn grad_u_to_green<const D: usize>(g: &Matrix<D, D>) -> Matrix<D, D> {
    let p = tr_matmul_small(g, g);
    std::array::from_fn(|j| std::array::from_fn(|i| (p[j][i] + g[i][j] + g[j][i]) / 2.))
}

/// `a.tr_matmul(b)` (`aᵀ·b`) of two compile-time `D × D` matrices for
/// `D ≤ 7`: all three extents are compile-time and below 8, so
/// Eigen's route is coefficient-based (`Inner` for `D = 1`) with the
/// halving tree over the `D` terms (`3·D + D − 1 ≤ 110`); evaluated here
/// with the straight-line tree instead of the general product machinery —
/// the same products and additions in the same order as
/// [`MatrixExt::tr_matmul`] (unit test `tr_matmul_small_matches_tr_matmul`).
#[inline]
fn tr_matmul_small<const D: usize>(a: &Matrix<D, D>, b: &Matrix<D, D>) -> Matrix<D, D> {
    if D > 7 {
        return a.tr_matmul(b);
    }
    let mut out = [[0.0; D]; D];
    let mut p = [0.0; D];
    for (j, col) in out.iter_mut().enumerate() {
        for (i, o) in col.iter_mut().enumerate() {
            // lhs(i, l) = a(l, i), rhs(l, j) = b(l, j)
            for (l, t) in p.iter_mut().enumerate() {
                *t = a[i][l] * b[j][l];
            }
            *o = akantu_core::eigen_unrolled::tree_sum(&p, D);
        }
    }
    out
}

/// `Material::gradUToF`: `F = I; F += ∇u` (`F(i, j) = I(i, j) + ∇u(i, j)`).
#[must_use]
pub fn grad_u_to_f<const D: usize>(g: &Matrix<D, D>) -> Matrix<D, D> {
    std::array::from_fn(|j| std::array::from_fn(|i| id(i, j) + g[j][i]))
}

/// `Material::rightCauchy`: `C = Fᵀ·F`.
#[must_use]
pub fn right_cauchy<const D: usize>(f: &Matrix<D, D>) -> Matrix<D, D> {
    f.tr_matmul(f)
}

/// `Material::leftCauchy`: `B = F·Fᵀ`.
#[must_use]
pub fn left_cauchy<const D: usize>(f: &Matrix<D, D>) -> Matrix<D, D> {
    f.matmul(&f.transpose())
}

/// `Material::StoCauchy(F, S, sigma, C33)`: `J = det(F)·√C33`;
/// `σ = (1/J · (F·S))·Fᵀ` (`0` instead of `1/J` when `J < ε`).
#[must_use]
pub fn s_to_cauchy<const D: usize>(f: &Matrix<D, D>, s: &Matrix<D, D>, c33: Real) -> Matrix<D, D> {
    let j = f.det() * c33.sqrt();
    let f_s = matmul_small(f, s);
    let constant = if j < Real::EPSILON { 0. } else { 1. / j };
    let scaled: Matrix<D, D> =
        std::array::from_fn(|c| std::array::from_fn(|r| constant * f_s[c][r]));
    matmul_small(&scaled, &f.transpose())
}

/// `a.matmul(b)` (`a·b`) of two compile-time `D × D` matrices for `D ≤ 7`:
/// route coefficient-based (`Inner` for `D = 1`) with the
/// halving tree over the `D` terms, as for [`tr_matmul_small`]; the same
/// products and additions in the same order as [`MatrixExt::matmul`]
/// (unit test `matmul_small_matches_matmul`).
#[inline]
fn matmul_small<const D: usize>(a: &Matrix<D, D>, b: &Matrix<D, D>) -> Matrix<D, D> {
    if D > 7 {
        return a.matmul(b);
    }
    let mut out = [[0.0; D]; D];
    let mut p = [0.0; D];
    for (j, col) in out.iter_mut().enumerate() {
        for (i, o) in col.iter_mut().enumerate() {
            // lhs(i, l) = a(i, l), rhs(l, j) = b(l, j)
            for (l, t) in p.iter_mut().enumerate() {
                *t = a[l][i] * b[j][l];
            }
            *o = akantu_core::eigen_unrolled::tree_sum(&p, D);
        }
    }
    out
}

/// `Material::computeDeviatoric<dim>`: `σ − I·tr(σ)/dim` (per coefficient
/// `σ(i, j) − (I(i, j)·tr) / dim`).
#[must_use]
pub fn deviatoric<const D: usize>(sigma: &Matrix<D, D>) -> Matrix<D, D> {
    let tr = sigma.trace();
    #[allow(clippy::cast_precision_loss)] // D <= 3
    let d = D as Real;
    std::array::from_fn(|j| std::array::from_fn(|i| sigma[j][i] - id(i, j) * tr / d))
}

/// Elastic stress coefficient as upstream `MaterialElastic::computeStressOnQuad`
/// evaluates it: `(2μ)·ε(i, j) + (λ·tr + σ_th)·I(i, j)`.
#[inline(always)]
#[must_use]
pub fn isotropic_stress_coeff(two_mu: Real, eps: Real, diag: Real, i: usize, j: usize) -> Real {
    two_mu * eps + diag * id(i, j)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Deterministic test values. Default mode: special values (signed zeros,
    /// infinities, subnormals, extremes), arbitrary bit patterns and finite
    /// values over a wide exponent range, where every NaN drawn is a *default*
    /// quiet NaN of either sign (see [`same`]). Payload mode
    /// ([`Values::payload_at`]): finite values only, except one draw, a NaN
    /// with a payload.
    struct Values {
        state: u64,
        draws: u64,
        nan_at: Option<u64>,
    }

    /// Bits of the default quiet NaN without the sign.
    const DEFAULT_NAN: u64 = 0x7ff8_0000_0000_0000;
    /// The NaN of the payload mode (quiet, negative, payload `0xabc`).
    const PAYLOAD_NAN: u64 = 0xfff8_0000_0000_0abc;

    /// The exact rule of D-046/D-051: identical bits, or two default quiet NaNs
    /// that differ only in the sign (`+0` and `−0` differ). When two NaNs meet
    /// in one addition, which one IEEE 754 returns is unspecified: x86 SSE
    /// returns the first operand's, and the compiler may commute the operands
    /// in either code path (Rust and LLVM leave the payload of such a NaN
    /// unspecified). Default-mode inputs therefore carry default NaNs only, and
    /// payload propagation is checked in payload mode, where a single NaN
    /// enters each case and every result bit is determined.
    fn same(a: Real, b: Real) -> bool {
        let (x, y) = (a.to_bits(), b.to_bits());
        let sign = 1u64 << 63;
        x == y || (x & !sign == DEFAULT_NAN && y & !sign == DEFAULT_NAN)
    }

    impl Values {
        fn new(seed: u64) -> Self {
            Self {
                state: seed,
                draws: 0,
                nan_at: None,
            }
        }

        fn next_u64(&mut self) -> u64 {
            // xorshift64*
            let mut x = self.state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.state = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        /// Finite value with a moderate exponent spread (cancellations, no
        /// overflow in the kernels' few operations).
        fn finite(&mut self) -> Real {
            let r = self.next_u64();
            let m = (self.next_u64() >> 11) as Real / (1u64 << 53) as Real;
            let e = ((r >> 8) % 41) as i32 - 20;
            let s = if (r >> 40) & 1 == 1 { -1.0 } else { 1.0 };
            s * m * Real::powi(2.0, e)
        }

        fn next(&mut self) -> Real {
            const SPECIAL: [u64; 14] = [
                0x0000_0000_0000_0000, // +0
                0x8000_0000_0000_0000, // -0
                0x7ff0_0000_0000_0000, // +inf
                0xfff0_0000_0000_0000, // -inf
                0x7ff8_0000_0000_0000, // default quiet NaN (aarch64)
                0xfff8_0000_0000_0000, // default quiet NaN (x86_64)
                0x0000_0000_0000_0001, // smallest subnormal
                0x800f_ffff_ffff_ffff, // largest negative subnormal
                0x0010_0000_0000_0000, // MIN_POSITIVE
                0x7fef_ffff_ffff_ffff, // MAX
                0x3ff0_0000_0000_0000, // 1
                0xbff0_0000_0000_0000, // -1
                0x3cb0_0000_0000_0000, // 2^-52
                0x4340_0000_0000_0000, // 2^53
            ];
            self.draws += 1;
            if let Some(k) = self.nan_at {
                return if self.draws == k {
                    Real::from_bits(PAYLOAD_NAN)
                } else {
                    self.finite()
                };
            }
            let r = self.next_u64();
            let x = match r % 8 {
                0 => Real::from_bits(SPECIAL[(r >> 8) as usize % SPECIAL.len()]),
                1 => Real::from_bits(self.next_u64()), // any bit pattern
                _ => self.finite(),
            };
            if x.is_nan() {
                Real::from_bits(DEFAULT_NAN | (x.to_bits() & (1 << 63)))
            } else {
                x
            }
        }

        /// Payload mode: finite draws, the `k`-th draw from now (`k ≥ 1`) is
        /// the payload NaN.
        fn payload_at(&mut self, k: u64) {
            self.nan_at = Some(self.draws + k);
        }

        /// Back to the default mode.
        fn default_mode(&mut self) {
            self.nan_at = None;
        }
    }

    /// The straight-line `aᵀ·b` against the general fixed-size product for
    /// `D = 1…7` (route checked), and `grad_u_to_green` against its general
    /// expression, on random matrices (D-046 exact rule, [`same`]); with
    /// `payload`, one payload NaN per pair. Returns the number of NaN
    /// entries.
    fn check_tr_matmul<const D: usize>(v: &mut Values, cases: usize, payload: bool) -> usize {
        use akantu_core::eigen_gebp::{ProductShape, Reduction, Route, route};
        assert_eq!(
            route(&ProductShape::fixed(D, D, D).lhs_transposed()),
            Route::Coeff(Reduction::Tree)
        );
        let mut nans = 0;
        for _ in 0..cases {
            if payload {
                let k = 1 + v.next_u64() % (2 * D * D) as u64;
                v.payload_at(k);
            }
            let a: Matrix<D, D> = std::array::from_fn(|_| std::array::from_fn(|_| v.next()));
            let b: Matrix<D, D> = std::array::from_fn(|_| std::array::from_fn(|_| v.next()));
            v.default_mode();
            let fast = tr_matmul_small(&a, &b);
            let general = a.tr_matmul(&b);
            let g = grad_u_to_green(&a);
            let p = a.tr_matmul(&a);
            for j in 0..D {
                for i in 0..D {
                    let (x, y) = (fast[j][i], general[j][i]);
                    assert!(
                        same(x, y),
                        "D={D} ({i},{j}): {:#x} vs {:#x}",
                        x.to_bits(),
                        y.to_bits()
                    );
                    let e = (p[j][i] + a[i][j] + a[j][i]) / 2.;
                    assert!(same(g[j][i], e), "green D={D} ({i},{j})");
                    nans += usize::from(x.is_nan());
                }
            }
        }
        nans
    }

    /// `matmul_small` against `MatrixExt::matmul`, and `s_to_cauchy`
    /// against its previous body (general products), D-046 rule; with
    /// `payload`, one payload NaN per case. Returns the NaN entries.
    fn check_matmul<const D: usize>(v: &mut Values, cases: usize, payload: bool) -> usize {
        use akantu_core::eigen_gebp::{ProductShape, Reduction, Route, route};
        assert_eq!(
            route(&ProductShape::fixed(D, D, D)),
            Route::Coeff(Reduction::Tree)
        );
        let reference = |f: &Matrix<D, D>, s: &Matrix<D, D>, c33: Real| -> Matrix<D, D> {
            let j = f.det() * c33.sqrt();
            let f_s = f.matmul(s);
            let constant = if j < Real::EPSILON { 0. } else { 1. / j };
            let scaled: Matrix<D, D> =
                std::array::from_fn(|c| std::array::from_fn(|r| constant * f_s[c][r]));
            scaled.matmul(&f.transpose())
        };
        let mut nans = 0;
        for _ in 0..cases {
            if payload {
                let k = 1 + v.next_u64() % (2 * D * D) as u64;
                v.payload_at(k);
            }
            let a: Matrix<D, D> = std::array::from_fn(|_| std::array::from_fn(|_| v.next()));
            let b: Matrix<D, D> = std::array::from_fn(|_| std::array::from_fn(|_| v.next()));
            v.default_mode();
            let (fast, general) = (matmul_small(&a, &b), a.matmul(&b));
            let c33 = if payload { 1.0 } else { v.next().abs() };
            let (cf, cg) = (s_to_cauchy(&a, &b, c33), reference(&a, &b, c33));
            for j in 0..D {
                for i in 0..D {
                    let (x, y) = (fast[j][i], general[j][i]);
                    assert!(
                        same(x, y),
                        "D={D} ({i},{j}): {:#x} vs {:#x}",
                        x.to_bits(),
                        y.to_bits()
                    );
                    let (x2, y2) = (cf[j][i], cg[j][i]);
                    assert!(
                        same(x2, y2),
                        "s_to_cauchy D={D} ({i},{j}): {:#x} vs {:#x}",
                        x2.to_bits(),
                        y2.to_bits()
                    );
                    nans += usize::from(x.is_nan());
                }
            }
        }
        nans
    }

    #[test]
    fn matmul_small_matches_matmul() {
        let mut v = Values::new(0x9a8b_7c6d_5e4f_3021);
        check_matmul::<1>(&mut v, 40_000, false);
        check_matmul::<2>(&mut v, 40_000, false);
        check_matmul::<3>(&mut v, 40_000, false);
        check_matmul::<4>(&mut v, 2_000, false);
        check_matmul::<7>(&mut v, 2_000, false);
    }

    #[test]
    fn matmul_small_propagates_a_payload_nan_like_matmul() {
        let mut v = Values::new(0x1357_9bdf_2468_ace0);
        let nans = check_matmul::<1>(&mut v, 5_000, true)
            + check_matmul::<2>(&mut v, 5_000, true)
            + check_matmul::<3>(&mut v, 10_000, true);
        assert!(nans > 0, "{nans}");
    }

    #[test]
    fn tr_matmul_small_matches_tr_matmul() {
        let mut v = Values::new(0x1234_5678_9abc_def1);
        check_tr_matmul::<1>(&mut v, 40_000, false);
        check_tr_matmul::<2>(&mut v, 40_000, false);
        check_tr_matmul::<3>(&mut v, 40_000, false);
        check_tr_matmul::<4>(&mut v, 2_000, false);
        check_tr_matmul::<7>(&mut v, 2_000, false);
    }

    #[test]
    fn tr_matmul_small_propagates_a_payload_nan_like_tr_matmul() {
        let mut v = Values::new(0x0f1e_2d3c_4b5a_6978);
        let nans = check_tr_matmul::<1>(&mut v, 5_000, true)
            + check_tr_matmul::<2>(&mut v, 5_000, true)
            + check_tr_matmul::<3>(&mut v, 10_000, true);
        assert!(nans > 0, "{nans}");
    }

    #[test]
    fn voigt_sizes() {
        assert_eq!(tangent_stiffness_voigt_size(1), 1);
        assert_eq!(tangent_stiffness_voigt_size(2), 3);
        assert_eq!(tangent_stiffness_voigt_size(3), 6);
    }

    #[test]
    fn strain_measures() {
        let g: Matrix<2, 2> = [[0.1, 0.3], [-0.2, 0.05]]; // columns
        let e = grad_u_to_epsilon(&g);
        assert_eq!(e[1][0], 0.5 * (-0.2 + 0.3));
        assert_eq!(e[0][1], e[1][0]);
        let f = grad_u_to_f(&g);
        assert_eq!(f, [[1.1, 0.3], [-0.2, 1.05]]);
        let gl = grad_u_to_green(&g);
        // E = (Fᵀ F − I) / 2
        let c = right_cauchy(&f);
        for i in 0..2 {
            for j in 0..2 {
                let want = (c[j][i] - id(i, j)) / 2.;
                assert!((gl[j][i] - want).abs() < 1e-15);
            }
        }
        // StoCauchy with S = 0 is 0, with F = I is S.
        let s: Matrix<2, 2> = [[2.0, 1.0], [1.0, 3.0]];
        let i2 = <Matrix<2, 2> as SquareMatrixExt<2>>::identity();
        assert_eq!(s_to_cauchy(&i2, &s, 1.0), s);
        let dev = deviatoric(&s);
        assert_eq!(dev[0][0] + dev[1][1], 0.0);
    }

    /// SMM-028: `stressToVonMises` on closed-form states (uniaxial σ → σ,
    /// pure shear τ → √3·τ in 3-D) and upstream's 2-D convention (the
    /// trace is divided by 3, so uniaxial 2-D σ gives `σ·sqrt(3·(4/9 +
    /// 1/9)/2)`); invalid inputs are errors.
    #[test]
    fn stress_to_von_mises_closed_forms() {
        let uni = [2.0, 0., 0., 0., 0., 0., 0., 0., 0.];
        assert_eq!(stress_to_von_mises(&uni, 3).unwrap(), 2.0);
        let tau = 0.75;
        let shear = [0., tau, 0., tau, 0., 0., 0., 0., 0.];
        let vm = stress_to_von_mises(&shear, 3).unwrap();
        assert!((vm - 3.0_f64.sqrt() * tau).abs() <= 4.0 * f64::EPSILON * vm);
        let uni2 = [3.0, 0., 0., 0.];
        let want = 3.0 * (3.0 * (4.0 / 9.0 + 1.0 / 9.0) / 2.0_f64).sqrt();
        let got = stress_to_von_mises(&uni2, 2).unwrap();
        assert!(
            (got - want).abs() <= 4.0 * f64::EPSILON * want,
            "{got} vs {want}"
        );
        assert!(stress_to_von_mises(&uni2, 3).is_err());
        assert!(stress_to_von_mises(&uni2, 4).is_err());
    }

    /// SMM-028: `setCauchyStressMatrix` is block diagonal with `S_t` on
    /// every `dim × dim` diagonal block (3-D), zero elsewhere; short
    /// buffers are errors and leave `sigma` untouched.
    #[test]
    fn set_cauchy_stress_matrix_blocks() {
        let s: Vec<Real> = (0..9).map(|k| 1.0 + k as Real).collect();
        let mut out = vec![7.0; 81];
        set_cauchy_stress_matrix(3, &s, &mut out).unwrap();
        for r in 0..9 {
            for c in 0..9 {
                let want = if r / 3 == c / 3 {
                    s[r % 3 + 3 * (c % 3)]
                } else {
                    0.0
                };
                assert_eq!(out[r + 9 * c], want, "({r}, {c})");
            }
        }
        let mut short = vec![7.0; 80];
        assert!(set_cauchy_stress_matrix(3, &s, &mut short).is_err());
        assert!(short.iter().all(|&x| x == 7.0));
        assert!(set_cauchy_stress_matrix(3, &s[..8], &mut out).is_err());
    }
}
