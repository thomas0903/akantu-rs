// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/energy_split.hh,
// energy_split.cc, energy_splits/no_energy_split{.hh,_inline_impl.hh},
// energy_splits/volumetric_deviatoric_split{.hh,_inline_impl.hh}
// (moved from akantu-phase-field; also used by
// src/model/solid_mechanics/materials/material_damage/material_phasefield*).

//! Energy splits (PF-008…010, MAT-030/031): which part of the elastic
//! energy density drives (and is degraded by) the damage.
//!
//! **Single owner**: the phase-field laws
//! (`akantu-phase-field`, driving energy `φ`) and the solid `phasefield`
//! material (`akantu-solid`, degraded stress `σ⁺`/`σ⁻` and tangent) both use
//! this module, as upstream's `EnergySplit` classes serve both models
//! (`src/model/phase_field/energy_split*` included by
//! `material_phasefield.hh`). `akantu-model` is the crate of the
//! constitutive-law framework that both depend on (docs/ARCHITECTURE.md, "Workspace and layers"); `akantu_phase_field::energy_split` re-exports it.
//!
//! | option | `φ` (driving energy density) | `σ⁺` / `σ⁻` |
//! |---|---|---|
//! | `""` / `no_split` | `½λ (tr ε)² + μ ε:ε` | `2μ ε' + λ tr ε' I` / untouched |
//! | `volumetric_deviatoric` | `½κ ⟨tr ε⟩₊² + μ ε_d:ε_d` | `κ⟨tr ε'⟩₊ I + 2μ ε'_d` / `κ⟨tr ε'⟩₋ I` |
//!
//! with `ε' = ε − ε_th I`, `κ = λ + (2/3)μ` and the deviator
//! `ε_d = ε − (tr ε / 3) I` — **`/3` in every dimension**, also in 1-D and
//! 2-D (upstream, UD-387). This is **not** the plane-strain deviator of a
//! 3-D body: the 1-D/2-D code drops the out-of-plane components `−tr ε/3`,
//! so in 2-D `φ⁺(2-D) = φ⁺(3-D, ε_zz = 0) − μ (tr ε)²/9`. The consequence:
//! in 1-D/2-D `φ⁺` is not the potential of `σ⁺`:
//! in tension `∂φ⁺/∂ε = σ⁺ − (2μ/9) tr ε I` in 2-D, and in 1-D
//! `∂φ⁺/∂ε = κ⟨ε⟩₊ + (8/9)μ ε` while `σ⁺ = κ⟨ε⟩₊ + (4/3)μ ε`, so the
//! staggered solid–phase-field problem is not variationally consistent (the
//! upstream analytical test value `5/9 μ ε²` encodes the same convention).
//!
//! **UD-387 is category B (D-066 (2))**: [`EnergySplit::phi`] stays
//! upstream's `computePhiOnQuad` (compat), [`EnergySplit::phi_consistent`]
//! is the corrected driving energy the phase-field laws use by default
//! (`PhaseFieldUpstreamCompat::in_block_deviator_energy` of
//! `akantu-phase-field` selects upstream's). It reads the `D × D` strain as
//! the 3-D strain of the stress's kinematics (uniaxial strain in 1-D, plane
//! strain in 2-D) and adds the eliminated normal components of the 3-D
//! deviator, `−tr ε/3` each:
//!
//! ```text
//! φ⁺ = ½κ⟨tr ε⟩₊² + μ (ε_d:ε_d + (3 − D)(tr ε/3)²)   (= ½κ⟨tr ε⟩₊² + μ(ε:ε − (tr ε)²/3))
//! ```
//!
//! so `∂φ⁺/∂ε = σ⁺` exactly and `φ⁺ + φ⁻ = φ_no_split` in every
//! dimension. Only `φ` changes: `σ⁺`, `σ⁻` and the
//! tangent are already the derivatives of this `φ⁺`, and 3-D is unchanged
//! (`3 − D = 0`; `phi_consistent` returns `phi` there, bit for bit).
//!
//! # Arithmetic (D-008)
//!
//! Upstream's split methods take `const Matrix<Real> &` (Eigen
//! **run-time-size** matrices): the strain is copied into a dynamic matrix,
//! so `trace()` and `doubleDot` (`cwiseProduct(..).sum()`) are sequential
//! sums from the first term in column-major order (not the halving tree of
//! compile-time sizes, `akantu_core::eigen_order`). Every scalar expression
//! keeps upstream's left-to-right association, e.g.
//! `0.5 * lambda * trace * trace` is `((0.5·λ)·t)·t`.
//!
//! The solid side of the coupling (`phasefield` material, MAT-030/031) uses
//! [`EnergySplit::sigma`] and [`EnergySplit::tangent_coefficients`] (with
//! its own sign test and the [`PhaseFieldTangent`] switch); the phase-field
//! laws use [`EnergySplit::phi`] only (upstream concept `CanComputePhi`);
//! [`EnergySplit::tangent`] is upstream's `computeTangentCoefsOnQuad`.

use akantu_core::tensor::Matrix;
use akantu_core::{Error, Real, Result};

/// The split selected by the law's allocator option (upstream template
/// argument `EnergySplit_`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum EnergySplitKind {
    /// `no_split` (also the empty option): the whole energy drives damage.
    #[default]
    NoSplit,
    /// `volumetric_deviatoric`: volumetric compression does not.
    VolumetricDeviatoric,
}

impl EnergySplitKind {
    /// Upstream `instantiatePhaseField` dispatch on the option string:
    /// `""` or `no_split` → [`EnergySplitKind::NoSplit`],
    /// `volumetric_deviatoric` → [`EnergySplitKind::VolumetricDeviatoric`].
    ///
    /// # Errors
    /// Any other option (upstream `AKANTU_ERROR("Unknown energy split type:
    /// …")`).
    pub fn from_option(option: &str) -> Result<Self> {
        match option {
            "" | "no_split" => Ok(Self::NoSplit),
            "volumetric_deviatoric" => Ok(Self::VolumetricDeviatoric),
            other => Err(Error::invalid(format!(
                "Unknown energy split type: {other}"
            ))),
        }
    }

    /// Upstream option spelling.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::NoSplit => "no_split",
            Self::VolumetricDeviatoric => "volumetric_deviatoric",
        }
    }
}

/// Tangent of the volumetric–deviatoric split as used by the solid
/// `phasefield` material (candidates, category B; the switch
/// `LawOptions::phasefield_tangent` of `akantu-solid`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PhaseFieldTangent {
    /// Default: the derivative of the stress (1-D `g_h κ + g·(4/3)μ`, sign
    /// test on the thermal-corrected trace, chosen by the caller).
    #[default]
    Consistent,
    /// Upstream: 1-D `g_h·E`, sign test on the total-strain trace.
    Upstream,
}

/// Material constants of a split (upstream `EnergySplit` members `E`, `nu`,
/// `plane_stress`, `lambda`, `mu`; PF-008).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct EnergySplit {
    /// Split kind.
    pub kind: EnergySplitKind,
    /// Young's modulus.
    pub e: Real,
    /// Poisson's ratio.
    pub nu: Real,
    /// Plane stress (2-D).
    pub plane_stress: bool,
    /// First Lamé parameter.
    pub lambda: Real,
    /// Shear modulus.
    pub mu: Real,
}

/// Kronecker delta as the coefficient of Eigen's `Identity(dim, dim)`.
#[inline(always)]
fn delta(i: usize, j: usize) -> Real {
    if i == j { 1.0 } else { 0.0 }
}

/// `m.trace()` of a run-time-size matrix: sequential from the first term.
#[inline]
fn trace_seq<const D: usize>(m: &Matrix<D, D>) -> Real {
    let mut t = m[0][0];
    for (i, col) in m.iter().enumerate().skip(1) {
        t += col[i];
    }
    t
}

/// `a.doubleDot(a)` of a run-time-size matrix (`cwiseProduct(a).sum()`):
/// sequential from the first term, column-major.
#[inline]
fn double_dot_seq<const D: usize>(a: &Matrix<D, D>) -> Real {
    let f = a.as_flattened();
    let mut s = f[0] * f[0];
    for v in &f[1..] {
        s += v * v;
    }
    s
}

/// `std::max(Real(0.), x)`: `(0 < x) ? x : 0` (so `-0` and NaN give `+0`).
#[inline(always)]
fn max_zero(x: Real) -> Real {
    if 0.0 < x { x } else { 0.0 }
}

/// `std::min(Real(0.), x)`: `(x < 0) ? x : 0`.
#[inline(always)]
fn min_zero(x: Real) -> Real {
    if x < 0.0 { x } else { 0.0 }
}

impl EnergySplit {
    /// Upstream `updateMaterialProperties(E, nu, plane_stress)`:
    /// `λ = νE/((1+ν)(1−2ν))` (plane stress `νE/((1+ν)(1−ν))`),
    /// `μ = E/(2(1+ν))`.
    #[must_use]
    pub fn new(kind: EnergySplitKind, e: Real, nu: Real, plane_stress: bool) -> Self {
        let mut lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        if plane_stress {
            lambda = nu * e / ((1. + nu) * (1. - nu));
        }
        Self {
            kind,
            e,
            nu,
            plane_stress,
            lambda,
            mu: e / (2. * (1. + nu)),
        }
    }

    /// Bulk modulus `κ = λ + (2/3)μ` (upstream `lambda + 2. / 3. * mu`).
    #[must_use]
    pub fn kappa(&self) -> Real {
        self.lambda + 2. / 3. * self.mu
    }

    /// Upstream `computePhiOnQuad(strain, phi)` (PF-009/010): the driving
    /// energy density of the strain `ε` (column-major `D × D`).
    #[must_use]
    pub fn phi<const D: usize>(&self, strain: &Matrix<D, D>) -> Real {
        let trace = trace_seq(strain);
        match self.kind {
            EnergySplitKind::NoSplit => {
                0.5 * self.lambda * trace * trace + self.mu * double_dot_seq(strain)
            }
            EnergySplitKind::VolumetricDeviatoric => {
                let trace_plus = max_zero(trace);
                let t3 = trace / 3.;
                let dev: Matrix<D, D> = std::array::from_fn(|j| {
                    std::array::from_fn(|i| strain[j][i] - t3 * delta(i, j))
                });
                let kappa = self.kappa();
                0.5 * kappa * trace_plus * trace_plus + self.mu * double_dot_seq(&dev)
            }
        }
    }

    /// The corrected driving energy density (UD-387, category B, D-066
    /// (2); the default of the phase-field laws): [`EnergySplit::phi`] with,
    /// for the volumetric–deviatoric split in 1-D/2-D, the eliminated normal
    /// components of the 3-D deviator added, `μ·(3 − D)·(tr ε/3)²`, so that
    /// `φ⁺` is the potential of [`EnergySplit::sigma`]'s `σ⁺` and the two
    /// parts add up to the unsplit energy. No split, and 3-D: identical to
    /// [`EnergySplit::phi`] (same operations, same bits).
    #[must_use]
    pub fn phi_consistent<const D: usize>(&self, strain: &Matrix<D, D>) -> Real {
        if D == 3 || self.kind == EnergySplitKind::NoSplit {
            return self.phi(strain);
        }
        let trace = trace_seq(strain);
        let trace_plus = max_zero(trace);
        let t3 = trace / 3.;
        let dev: Matrix<D, D> =
            std::array::from_fn(|j| std::array::from_fn(|i| strain[j][i] - t3 * delta(i, j)));
        #[allow(clippy::cast_precision_loss)] // D <= 3
        let missing = (3 - D) as Real * (t3 * t3);
        let kappa = self.kappa();
        0.5 * kappa * trace_plus * trace_plus + self.mu * (double_dot_seq(&dev) + missing)
    }

    /// Upstream `computeSigmaOnQuad(strain, epsilon_th, sigma_plus,
    /// sigma_minus)`: `(σ⁺, σ⁻)`. For [`EnergySplitKind::NoSplit`] upstream
    /// leaves `σ⁻` untouched; the port returns zeros (the caller's `σ⁻`
    /// buffer is zero-initialised upstream).
    #[must_use]
    pub fn sigma<const D: usize>(
        &self,
        strain_quad: &Matrix<D, D>,
        epsilon_th: Real,
    ) -> (Matrix<D, D>, Matrix<D, D>) {
        // `auto strain = strain_quad - epsilon_th * Identity`: a lazy
        // expression, evaluated coefficient-wise wherever it is read.
        let strain: Matrix<D, D> = std::array::from_fn(|j| {
            std::array::from_fn(|i| strain_quad[j][i] - epsilon_th * delta(i, j))
        });
        let trace = trace_seq(&strain);
        match self.kind {
            EnergySplitKind::NoSplit => {
                let two_mu = 2. * self.mu;
                let lt = self.lambda * trace;
                let plus = std::array::from_fn(|j| {
                    std::array::from_fn(|i| two_mu * strain[j][i] + lt * delta(i, j))
                });
                (plus, [[0.0; D]; D])
            }
            EnergySplitKind::VolumetricDeviatoric => {
                let trace_plus = max_zero(trace);
                let trace_minus = min_zero(trace);
                let t3 = trace / 3.;
                let dev: Matrix<D, D> = std::array::from_fn(|j| {
                    std::array::from_fn(|i| strain[j][i] - t3 * delta(i, j))
                });
                let kappa = self.kappa();
                let kp = kappa * trace_plus;
                let km = kappa * trace_minus;
                let two_mu = 2. * self.mu;
                let plus = std::array::from_fn(|j| {
                    std::array::from_fn(|i| kp * delta(i, j) + two_mu * dev[j][i])
                });
                let minus = std::array::from_fn(|j| std::array::from_fn(|i| km * delta(i, j)));
                (plus, minus)
            }
        }
    }

    /// Upstream `computeTangentCoefsOnQuad(strain, g_d, tangent)`: writes
    /// the Voigt coefficients into the column-major `n × n` slice `out`
    /// (`n = D(D+1)/2`); like upstream only the listed coefficients are
    /// written (the caller zero-initialises the tangent).
    ///
    /// No split: 1-D `E·g`, else `M_iiii = g(λ+2μ)`, `M_iijj = gλ`,
    /// `M_ijij = gμ`. Volumetric–deviatoric: `g_h = g` if `tr ε > 0` (the
    /// total strain, no thermal shift) else `1`; 1-D `E·g_h`, else
    /// `M_iiii = g_h κ + g·2μ(1 − 1/3)`, `M_iijj = g_h κ − g·2μ/3`,
    /// `M_ijij = gμ`.
    ///
    /// # Errors
    /// `out` shorter than `n²`.
    pub fn tangent<const D: usize>(
        &self,
        strain_quad: &Matrix<D, D>,
        g_d: Real,
        out: &mut [Real],
    ) -> Result<()> {
        let positive = trace_seq(strain_quad) > 0.;
        self.tangent_coefficients::<D>(g_d, positive, PhaseFieldTangent::Upstream, out)
    }

    /// The coefficients of [`EnergySplit::tangent`] with the sign test of
    /// the volumetric split given (`trace_positive`: upstream tests the
    /// total strain, the solid material's consistent tangent the
    /// thermal-corrected one) and the 1-D volumetric–deviatoric coefficient
    /// of `mode` (upstream `E·g_h`, consistent `g_h κ + g·(4/3)μ`). Writes
    /// only the listed coefficients into the column-major `n × n` slice
    /// `out` (`n = D(D+1)/2`; the caller zero-initialises it).
    ///
    /// # Errors
    /// `out` shorter than `n²`.
    pub fn tangent_coefficients<const D: usize>(
        &self,
        g_d: Real,
        trace_positive: bool,
        mode: PhaseFieldTangent,
        out: &mut [Real],
    ) -> Result<()> {
        let n = D * (D - 1) / 2 + D;
        if out.len() < n * n {
            return Err(Error::DimensionMismatch {
                context: "computeTangentCoefsOnQuad: tangent",
                expected: n * n,
                got: out.len(),
            });
        }
        let at = |i: usize, j: usize| i + j * n;
        let (miiii, miijj, mijij) = match self.kind {
            EnergySplitKind::NoSplit => {
                if D == 1 {
                    out[0] = self.e * g_d;
                    return Ok(());
                }
                (
                    g_d * (self.lambda + 2. * self.mu),
                    g_d * self.lambda,
                    g_d * self.mu,
                )
            }
            EnergySplitKind::VolumetricDeviatoric => {
                let kappa = self.kappa();
                let g_d_hyd = if trace_positive { g_d } else { 1. };
                let miiii = g_d_hyd * kappa + g_d * 2. * self.mu * (1. - 1. / 3.);
                if D == 1 {
                    out[0] = match mode {
                        PhaseFieldTangent::Upstream => self.e * g_d_hyd,
                        PhaseFieldTangent::Consistent => miiii,
                    };
                    return Ok(());
                }
                (
                    miiii,
                    g_d_hyd * kappa - g_d * 2. * self.mu / 3.,
                    g_d * self.mu,
                )
            }
        };
        out[at(0, 0)] = miiii;
        if D >= 2 {
            out[at(1, 1)] = miiii;
            out[at(0, 1)] = miijj;
            out[at(1, 0)] = miijj;
            out[at(n - 1, n - 1)] = mijij;
        }
        if D == 3 {
            out[at(2, 2)] = miiii;
            out[at(0, 2)] = miijj;
            out[at(2, 0)] = miijj;
            out[at(1, 2)] = miijj;
            out[at(2, 1)] = miijj;
            out[at(3, 3)] = mijij;
            out[at(4, 4)] = mijij;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn close(a: Real, b: Real, tol: Real) -> bool {
        (a - b).abs() <= tol * b.abs().max(1.0)
    }

    #[test]
    fn options() {
        assert_eq!(
            EnergySplitKind::from_option("").unwrap(),
            EnergySplitKind::NoSplit
        );
        assert_eq!(
            EnergySplitKind::from_option("no_split").unwrap(),
            EnergySplitKind::NoSplit
        );
        assert_eq!(
            EnergySplitKind::from_option("volumetric_deviatoric").unwrap(),
            EnergySplitKind::VolumetricDeviatoric
        );
        assert!(EnergySplitKind::from_option("spectral").is_err());
    }

    #[test]
    fn lame_constants() {
        let s = EnergySplit::new(EnergySplitKind::NoSplit, 210.0, 0.3, false);
        assert!(close(s.lambda, 210.0 * 0.3 / (1.3 * 0.4), 1e-15));
        assert!(close(s.mu, 210.0 / 2.6, 1e-15));
        let ps = EnergySplit::new(EnergySplitKind::NoSplit, 210.0, 0.3, true);
        assert!(close(ps.lambda, 210.0 * 0.3 / (1.3 * 0.7), 1e-15));
    }

    /// No split: φ is the full isotropic strain energy density and σ⁺ is
    /// its derivative (`∂φ/∂ε = σ⁺` checked by central differences).
    #[test]
    fn no_split_energy_and_stress_are_consistent() {
        let s = EnergySplit::new(EnergySplitKind::NoSplit, 3.0, 0.25, false);
        let eps: Matrix<3, 3> = [
            [1e-3, 2e-4, -3e-4],
            [2e-4, -5e-4, 1e-4],
            [-3e-4, 1e-4, 7e-4],
        ];
        let (sp, sm) = s.sigma(&eps, 0.0);
        assert_eq!(sm, [[0.0; 3]; 3]);
        let h = 1e-7;
        for j in 0..3 {
            for i in 0..3 {
                let mut a = eps;
                let mut b = eps;
                a[j][i] += h;
                b[j][i] -= h;
                let d = (s.phi(&a) - s.phi(&b)) / (2. * h);
                assert!(close(d, sp[j][i], 1e-6), "{i}{j}: {d} vs {}", sp[j][i]);
            }
        }
    }

    /// Volumetric–deviatoric: compression only contributes the deviatoric
    /// part; tension the full volumetric part; the stress sum `σ⁺ + σ⁻` is
    /// the isotropic stress in 3-D.
    #[test]
    fn volumetric_deviatoric_split_properties() {
        let s = EnergySplit::new(EnergySplitKind::VolumetricDeviatoric, 3.0, 0.25, false);
        let iso = EnergySplit::new(EnergySplitKind::NoSplit, 3.0, 0.25, false);
        let k = s.kappa();
        // pure compression: φ = μ dev:dev = 0 for a hydrostatic strain
        let c: Matrix<3, 3> = [[-1e-3, 0., 0.], [0., -1e-3, 0.], [0., 0., -1e-3]];
        assert!(s.phi(&c).abs() < 1e-30);
        // pure dilatation: φ = ½ κ tr²
        let t: Matrix<3, 3> = [[1e-3, 0., 0.], [0., 1e-3, 0.], [0., 0., 1e-3]];
        assert!(close(s.phi(&t), 0.5 * k * 9e-6, 1e-12));
        let eps: Matrix<3, 3> = [
            [-1e-3, 2e-4, -3e-4],
            [2e-4, -5e-4, 1e-4],
            [-3e-4, 1e-4, 7e-4],
        ];
        let (sp, sm) = s.sigma(&eps, 0.0);
        let (si, _) = iso.sigma(&eps, 0.0);
        for j in 0..3 {
            for i in 0..3 {
                assert!(close(sp[j][i] + sm[j][i], si[j][i], 1e-12));
            }
        }
    }

    /// 2-D: the deviator uses `/3` (upstream), so the 2-D volumetric +
    /// deviatoric energies do not add up to the 2-D isotropic energy; the
    /// corrected default adds the out-of-plane component (`2/3 μ ε²` instead
    /// of upstream's `5/9 μ ε²` for a uniaxial plane strain).
    #[test]
    fn deviator_divides_by_three_in_two_d() {
        let s = EnergySplit::new(EnergySplitKind::VolumetricDeviatoric, 210.0, 0.3, false);
        let eps: Matrix<2, 2> = [[0., 0.], [0., 1e-3]];
        let tr: Real = 1e-3;
        let d00 = -tr / 3.;
        let d11 = 1e-3 - tr / 3.;
        let want = 0.5 * s.kappa() * tr * tr + s.mu * (d00 * d00 + d11 * d11);
        assert!(close(s.phi(&eps), want, 1e-14));
        // upstream test_phase_field_deviatoric_split: φ⁺ = ½κε² + μ·(5/9)ε²
        let analytic = 0.5 * s.kappa() * tr * tr + s.mu * tr * tr * 5. / 9.;
        assert!(close(s.phi(&eps), analytic, 1e-14));
        // corrected (UD-387): + μ (tr/3)² for the eliminated ε_zz deviator
        let corrected = 0.5 * s.kappa() * tr * tr + s.mu * tr * tr * 2. / 3.;
        assert!(close(s.phi_consistent(&eps), corrected, 1e-14));
    }

    /// UD-387 (category B, D-066 (2)): in 1-D/2-D upstream's `φ⁺`
    /// ([`EnergySplit::phi`]) is not the potential of `σ⁺`
    /// (`∂φ⁺/∂ε − σ⁺ = −(2μ/9)(3 − D) tr ε I`) and `φ⁺ + φ⁻ ≠ φ_no_split`,
    /// while the 3-D-embedded `φ⁺ = ½κ⟨tr ε⟩₊² + μ(ε:ε − (tr ε)²/3)` is the
    /// potential of the unchanged `σ⁺` and partitions the energy exactly;
    /// [`EnergySplit::phi_consistent`] (the corrected default) is that
    /// embedded form, to rounding.
    #[test]
    fn ud387_embedded_phi_is_the_potential_of_sigma_plus() {
        fn check<const D: usize>(eps: Matrix<D, D>) {
            let s = EnergySplit::new(EnergySplitKind::VolumetricDeviatoric, 210.0, 0.3, false);
            let iso = EnergySplit::new(EnergySplitKind::NoSplit, 210.0, 0.3, false);
            #[allow(clippy::cast_precision_loss)] // D <= 3
            let d = D as Real;
            let tr = |e: &Matrix<D, D>| (0..D).map(|i| e[i][i]).sum::<Real>();
            let embedded = |e: &Matrix<D, D>| {
                let t = tr(e);
                let ee: Real = e.as_flattened().iter().map(|x| x * x).sum();
                0.5 * s.kappa() * t.max(0.) * t.max(0.) + s.mu * (ee - t * t / 3.)
            };
            let (sp, _) = s.sigma(&eps, 0.0);
            let t = tr(&eps);
            let h = 1e-7 * t.abs().max(1e-3);
            for j in 0..D {
                for i in 0..D {
                    let (mut a, mut b) = (eps, eps);
                    a[j][i] += h;
                    b[j][i] -= h;
                    let fd_up = (s.phi(&a) - s.phi(&b)) / (2. * h);
                    let fd_new = (embedded(&a) - embedded(&b)) / (2. * h);
                    let scale = sp
                        .as_flattened()
                        .iter()
                        .fold(0.0_f64, |m, x| m.max(x.abs()));
                    assert!(
                        (fd_new - sp[j][i]).abs() <= 1e-9 * scale,
                        "{D}-D ({i},{j}): embedded {fd_new} vs σ⁺ {}",
                        sp[j][i]
                    );
                    // upstream: off by −(2μ/9)(3 − D) tr ε on the diagonal
                    let gap = if i == j {
                        -(2. * s.mu / 9.) * (3. - d) * t
                    } else {
                        0.
                    };
                    assert!(
                        (fd_up - sp[j][i] - gap).abs() <= 1e-9 * scale,
                        "{D}-D ({i},{j}): upstream gap {} vs {gap}",
                        fd_up - sp[j][i]
                    );
                }
            }
            let phi_minus = 0.5 * s.kappa() * t.min(0.) * t.min(0.);
            let total = iso.phi(&eps);
            assert!((embedded(&eps) + phi_minus - total).abs() <= 1e-15 * total);
            // the corrected default is the embedded form (to rounding) and,
            // like it, the potential of σ⁺
            let corrected = s.phi_consistent(&eps);
            assert!(
                (corrected - embedded(&eps)).abs() <= 4. * Real::EPSILON * total,
                "{D}-D: phi_consistent {corrected} vs embedded {}",
                embedded(&eps)
            );
            assert!((corrected + phi_minus - total).abs() <= 1e-15 * total);
            for j in 0..D {
                for i in 0..D {
                    let (mut a, mut b) = (eps, eps);
                    a[j][i] += h;
                    b[j][i] -= h;
                    let fd = (s.phi_consistent(&a) - s.phi_consistent(&b)) / (2. * h);
                    let scale = sp
                        .as_flattened()
                        .iter()
                        .fold(0.0_f64, |m, x| m.max(x.abs()));
                    assert!(
                        (fd - sp[j][i]).abs() <= 1e-9 * scale,
                        "{D}-D ({i},{j}): phi_consistent {fd} vs σ⁺ {}",
                        sp[j][i]
                    );
                }
            }
            if D == 3 {
                assert_eq!(corrected.to_bits(), s.phi(&eps).to_bits());
            }
            let up_gap = s.phi(&eps) + phi_minus - total;
            assert!((up_gap + s.mu * (3. - d) * t * t / 9.).abs() <= 1e-15 * total);
        }
        check::<1>([[1e-3]]);
        check::<1>([[-1e-3]]);
        check::<2>([[1e-3, 2e-4], [2e-4, -3e-4]]);
        check::<2>([[-1e-3, 2e-4], [2e-4, 3e-4]]);
        // 3-D: upstream and the embedded form coincide
        check::<3>([[1e-3, 2e-4, 0.], [2e-4, -3e-4, 1e-4], [0., 1e-4, 4e-4]]);
    }

    #[test]
    fn tangent_coefficients() {
        let s = EnergySplit::new(EnergySplitKind::NoSplit, 210.0, 0.3, false);
        let mut t = [0.0; 9];
        s.tangent::<2>(&[[0., 0.], [0., 1e-3]], 0.5, &mut t)
            .unwrap();
        assert_eq!(t[0], 0.5 * (s.lambda + 2. * s.mu));
        assert_eq!(t[4], t[0]);
        assert_eq!(t[3], 0.5 * s.lambda);
        assert_eq!(t[1], t[3]);
        assert_eq!(t[8], 0.5 * s.mu);
        let mut t1 = [0.0; 1];
        s.tangent::<1>(&[[1.0]], 0.25, &mut t1).unwrap();
        assert_eq!(t1[0], 210.0 * 0.25);
        let v = EnergySplit::new(EnergySplitKind::VolumetricDeviatoric, 210.0, 0.3, false);
        let mut t3 = [0.0; 36];
        // compression: the volumetric part is not degraded
        v.tangent::<3>(&[[-1e-3, 0., 0.], [0., 0., 0.], [0., 0., 0.]], 0.0, &mut t3)
            .unwrap();
        assert_eq!(t3[0], v.kappa());
        assert_eq!(t3[1], v.kappa());
        assert_eq!(t3[21], 0.0);
        assert!(s.tangent::<3>(&[[0.; 3]; 3], 1.0, &mut [0.0; 35]).is_err());
    }
}
