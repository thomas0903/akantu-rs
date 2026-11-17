// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_viscoelastic/material_standard_linear_solid_deviatoric.hh,
// material_standard_linear_solid_deviatoric.cc.

//! `sls_deviatoric`: standard linear solid with a deviatoric viscous branch
//! (upstream `MaterialStandardLinearSolidDeviatoric<dim>`, parent
//! `MaterialElastic<dim>`), all spatial dimensions (2-D plane strain or
//! plane stress through the parent's `λ`).
//!
//! Parameters: the elastic ones (`E` = instantaneous modulus
//! `E∞ + Ev`, `nu`, …) plus `Eta` (viscosity, default 1), `Ev` (stiffness
//! of the viscous branch, default 1) and the read-only `Einf = E − Ev`
//! (recomputed by every update; initially 1).
//!
//! Internals: `stress_dev` (`dim²`, the elastic deviatoric stress
//! `s = 2μ ε_d` of the last evaluation), `history_integral` (`dim²`, `h`)
//! and `dissipated_energy` (1).
//!
//! # Kernel (upstream order, decision D-008)
//!
//! With `τ = η/Ev`, `Δt` the model's time step, `e₁ = exp(−Δt/τ)`,
//! `e₂ = exp(−½Δt/τ)` (glibc `exp`, ported), `γ∞ = E∞/E`, `γ_v = Ev/E`, per
//! quadrature point:
//!
//! ```text
//! ε   = ½(∇uᵀ + ∇u),   Θ = tr ε,   ε_d(i,j) = ε(i,j) − (I(i,j)·Θ)/3
//! s   = (2μ)·ε_d
//! h   ← e₁·h + e₂·(s − s_old),   s_old ← s
//! σ   = ((I·γ∞)·κ)·Θ + γ∞·s + γ_v·h
//! q   = (s − h)·γ_v,  q̇ = (s·γ_v − q)/τ,  α = 1/(2μ·γ_v)
//! e_d += Σ_entries((ε_d − α·q)·q̇·Δt)     (upstream: a matrix product)
//! ```
//!
//! In 1-D and 2-D the tensors are the retained `D × D` block and the
//! deviatoric map keeps the 3-D `Θ/3`: the branch strain map is
//! `B ε = ε − I tr_D(ε)/3`, i.e. the 3-D deviator of the strain with zero
//! out-of-block components (uniaxial strain in 1-D, plane-strain kinematics
//! in 2-D; with `Plane_Stress` only the bulk coefficient `κ` of the parent
//! changes, so the 2-D law is this `D`-dimensional operator, not a
//! constrained 3-D plane-stress viscoelastic model — no `σ33 = 0` condition
//! acts on the branch, UD-328, category C).
//!
//! The tangent and wave speeds are the parent's (instantaneous modulus
//! `E`; the tangent is the instantaneous elastic one, a documented
//! approximate Newton operator, category C). The potential energy is the
//! free energy of the two springs by default (UD-161 below).
//!
//! The midpoint recurrence of `h` is the exponential **midpoint**
//! approximation of the convolution: for a strain affine over the step the
//! exact coefficient of `Δs` is `(1 − e^{−x})/x` (`x = Δt/τ`), the scheme
//! uses `e^{−x/2}` (relative difference `−x²/24 + O(x⁴)`), so it is
//! second-order accurate, not exact for piecewise-linear strain.
//!
//! # Upstream defects (`LawOptions`)
//!
//! * **UD-029** (SLS part, category B): the dissipation increment sums the
//!   entries of the *matrix product* `(ε_d − αq)·q̇`, which is not
//!   rotation invariant; the default ([`SlsDissipation::Contraction`]) uses
//!   the double contraction `(ε_d − αq) : q̇ Δt`, the rate of dissipation
//!   `σ_branch : ε̇_dashpot` of the branch (branch spring strain
//!   `ε_d − αq`, dashpot strain `αq`) integrated with the rectangle rule;
//!   in 1-D/2-D with the reconstructed eliminated components (`B⁻¹` form,
//!   below).
//! * **UD-063** (category B): upstream advances `h`, `s_old` and `e_d` at
//!   every `computeStress`, so repeated residual evaluations within one
//!   Newton step evolve the history several times and a failed step cannot
//!   be rolled back. With `sls_committed_state = true` (default) the three
//!   internals keep a history; each evaluation computes the trial state
//!   from the values committed at the last `savePreviousState`
//!   (`beforeSolveStep`), so evaluations are idempotent and
//!   `restorePreviousState` rolls back. A single evaluation per step gives
//!   the upstream result bit for bit (only the dissipation contraction
//!   differs). The switch is read by `initMaterial` (history registration)
//!   and by every evaluation; [`Material::set_options`] after initialisation
//!   enables the history if needed (committed = current state).
//! * **UD-161** (category B): upstream's potential energy
//!   is the parent's `½σ:∇u` with the total SLS stress, which misses the
//!   energy of the branch spring (zero at zero total strain with a non-zero
//!   branch stress). [`SlsEnergy::TwoSpring`] (default, isothermal small
//!   strain):
//!
//!   ```text
//!   ψ = ½σ∞:ε + γ_v/(4μ)·h:B⁻¹:h,   σ∞ = γ∞(κ tr ε I + 2μ ε_d)
//!   ```
//!
//!   (3-D: `B = I` on deviators, `γ∞K/2 (tr ε)² + γ∞G ε_d:ε_d +
//!   γ_v/(4G) h:h`), for the whole-law and per-element energies;
//!   `ψ̇ = σ:ε̇ − γ_v h:B⁻¹:h/(2μτ)` along the exact branch ODE
//!   `ḣ = 2μ B ε̇ − h/τ`, the last term being the dissipation rate. The
//!   corrected mode rejects `finite_deformation` and `alpha ≠ 0` (upstream
//!   ignores both); [`SlsEnergy::UpstreamElastic`] reproduces upstream.
//! * **UD-161 / UD-029, reduced dimensions** (category B):
//!   on the retained `D × D` block (`D < 3`) the branch strain map
//!   `B ε = ε − I tr_D(ε)/3` is invertible with
//!   `B⁻¹ h = h + I tr_D(h)/(3 − D)`, so the branch spring energy and the
//!   dissipation rate contract with
//!
//!   ```text
//!   h:B⁻¹:h = h:h + (tr_D h)²/(3 − D)
//!   ```
//!
//!   (equivalently: the eliminated deviatoric normal components
//!   `−tr_D h/(3 − D)` are reconstructed; 1-D `h22 = h33 = −h11/2`, 2-D
//!   `h33 = −(h11 + h22)`). Contracting only the retained block (an
//!   earlier version) left `(W − ψ − D)/W → 1/3` in 1-D and `1/6` in 2-D under
//!   refinement of a uniaxial ramp/unload/hold. Both defaults
//!   ([`SlsEnergy::TwoSpring`], [`SlsDissipation::Contraction`]) use the
//!   `B⁻¹` form, which converges like 3-D (`review_v40_regressions.rs`,
//!   `mx01_*`); the 3-D arithmetic is unchanged. The compatibility
//!   variants keep upstream's energy and matrix-product dissipation.
//! * **UD-762** (category B): degenerate branch
//!   parameters. With `a = ε_d − αq = h/(2μ)` and `q̇ = γ_v h/τ` the
//!   dissipation increment is `γ_v/(2μτ)·h:B⁻¹:h·Δt` (3-D: `h:h`), whose
//!   limits are exactly 0 for `Ev → 0` (no branch: `γ_v/τ = Ev²/(Eη) → 0`,
//!   `h` finite) and for `Eta → 0` with `Ev > 0` (instantaneous relaxation:
//!   `e^{−Δt/2τ} → 0` faster than `1/τ` grows, `h ≡ 0`). Upstream evaluates
//!   `α = 1/(2μγ_v)` and `/τ` as written: `Ev = 0` gives `a = ε_d − ∞·0 =
//!   NaN`, `Eta = 0` gives `q̇ = 0/0`, both a NaN dissipated energy (the
//!   stress and stored energy stay finite), and `Ev = Eta = 0` a NaN
//!   `τ = 0/0` and stress; negative or non-finite `Ev`/`Eta` are accepted
//!   (exponentially growing recurrence). [`SlsDegenerateBranch::Limit`]
//!   (default) refuses negative or non-finite `Ev`, `Eta` and `Ev > E`
//!   (a negative long-term modulus `E∞ = E − Ev`: upstream documents
//!   `E = E∞ + Ev` as the initial modulus of two springs)
//!   ([`Error::InvalidArgument`] at `initMaterial` and at every
//!   evaluation, so parameter updates are covered), takes `τ = ∞` when
//!   `Ev = 0` (no branch, whatever `Eta`) or when `Ev > 0` is so small that
//!   `γ_v = Ev/E` is subnormal or zero and `α = 1/(2μγ_v)` overflows
//!   (`Ev ≲ 6e-309·E`, otherwise `(ε_d − ∞·q):q̇` is
//!   NaN), `e^{−Δt/τ} = e^{−Δt/2τ} = 0` when `τ = 0` (the branch carries no
//!   stress), and the limit increment 0 in these cases; non-degenerate
//!   parameters follow the arithmetic above bit for bit.
//!   [`SlsDegenerateBranch::Upstream`] reproduces upstream.
//! * Per-element dissipated energy (`getEnergy(id, element)`): upstream
//!   integrates with the **law-local** index as mesh element (wrong
//!   Jacobian whenever the material's elements are not the first mesh
//!   elements); the port maps the law-local element to
//!   its mesh element, as `Material::getPotentialEnergy(element)` does.

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::exec::{Tuples, TuplesRef};
use akantu_core::glibc_libm;
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::ParameterRegistry;
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::elastic::MaterialElastic;
use super::scratch::ScratchBlocks;
use crate::material::{DerivedMaterialParams, LawOptions, Material, MaterialBase, MaterialFactory};
use crate::material_tensor::grad_u_to_epsilon;
use akantu_core::array_shared::ArrayRef;

/// Dissipated-energy increment of `sls_deviatoric` (UD-029, category B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlsDissipation {
    /// `(ε_d − αq) : B⁻¹ : q̇ Δt`: the double contraction of the branch
    /// tensors with the eliminated deviatoric normal components
    /// reconstructed (rotation invariant; `B⁻¹ = I` in 3-D, `+ tr·tr/(3 − D)`
    /// in 1-D/2-D).
    Contraction,
    /// Upstream: sum of the entries of the matrix product `(ε_d − αq)·q̇·Δt`.
    UpstreamMatrixProduct,
}

/// Stored energy of `sls_deviatoric` (UD-161).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlsEnergy {
    /// Default: the free energy of the two springs,
    /// `ψ = ½σ∞:ε + γ_v/(4μ)·h:B⁻¹:h` (`σ∞ = γ∞(κ tr ε I + 2μ ε_d)` the
    /// equilibrium arm, `γ_v h` the branch stress, branch spring strain
    /// `B⁻¹h/(2μ)`; `h:B⁻¹:h = h:h + (tr_D h)²/(3 − D)` in 1-D/2-D),
    /// with the supported domain enforced: isothermal
    /// (`alpha = 0`) small strain (no `finite_deformation`), otherwise
    /// [`Error::Unsupported`].
    #[default]
    TwoSpring,
    /// Upstream: the parent's `½σ:∇u` with the total SLS stress (misses the
    /// energy stored in the branch, zero at zero strain), no domain checks.
    UpstreamElastic,
}

/// Degenerate viscous-branch parameters of `sls_deviatoric` (UD-762,
/// category B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SlsDegenerateBranch {
    /// Default: negative or non-finite `Ev`, `Eta` and `Ev > E` are
    /// refused; `Ev = 0` (no branch, also an `Ev` whose `α = 1/(2μ·Ev/E)`
    /// overflows) and `Eta = 0` (instantaneous relaxation) take the
    /// mathematical limits of the recurrence and of the dissipation
    /// increment (0), instead of NaN.
    #[default]
    Limit,
    /// Upstream: `τ = Eta/Ev`, `α = 1/(2μγ_v)` as written (NaN dissipated
    /// energy for `Ev = 0` or `Eta = 0`, NaN stress for both), no checks.
    Upstream,
}

/// Identity coefficient (Eigen `Identity()`).
#[inline(always)]
fn id(i: usize, j: usize) -> Real {
    if i == j { 1.0 } else { 0.0 }
}

/// Number `3 − D` of deviatoric normal components eliminated by the
/// retained `D × D` block, as a real; only used for
/// `D < 3`.
#[inline(always)]
fn reduced_count<const D: usize>() -> Real {
    match D {
        1 => 2.,
        2 => 1.,
        _ => 0.,
    }
}

/// `h : B⁻¹ : h` of a branch tensor on the retained `D × D` block, where
/// `B ε = ε − I tr_D(ε)/3` is the branch strain map of the law (the 3-D
/// deviator restricted to the block): `h:h + (tr_D h)²/(3 − D)` for `D < 3`,
/// `h:h` in 3-D (the eliminated normal components of
/// the deviator are `−tr_D h/(3 − D)` each).
#[inline(always)]
fn branch_norm<const D: usize>(h: &Matrix<D, D>) -> Real {
    let mut hh = h.double_dot(h);
    if D < 3 {
        let tr = h.trace();
        hh += tr * tr / reduced_count::<D>();
    }
    hh
}

/// Constants of one evaluation (one time step).
#[derive(Clone, Copy, Debug)]
struct SlsConstants {
    dt: Real,
    tau: Real,
    exp_dt_tau: Real,
    exp_dt_tau_2: Real,
    gamma_inf: Real,
    gamma_v: Real,
    kpa: Real,
    mu: Real,
    alpha: Real,
    dissipation: SlsDissipation,
    /// UD-762: the branch dissipates nothing (`Ev = 0` or `τ = 0`, in
    /// [`SlsDegenerateBranch::Limit`] mode): the increment is its limit 0.
    no_dissipation: bool,
}

impl SlsConstants {
    /// One quadrature point: from `grad_u` and the state `(s_old, h, e_d)`
    /// the law starts from, the stress and the new state.
    #[allow(clippy::type_complexity)] // (σ, s, h, e_d) of one point
    #[inline(always)]
    fn on_quad<const D: usize>(
        &self,
        grad_u: &Matrix<D, D>,
        dev_s_old: &Matrix<D, D>,
        h_old: &Matrix<D, D>,
        e_diss_old: Real,
    ) -> (Matrix<D, D>, Matrix<D, D>, Matrix<D, D>, Real) {
        let eps = grad_u_to_epsilon(grad_u);
        let theta = eps.trace();
        let eps_d: Matrix<D, D> =
            std::array::from_fn(|j| std::array::from_fn(|i| eps[j][i] - id(i, j) * theta / 3.));
        let two_mu = 2. * self.mu;
        let s: Matrix<D, D> =
            std::array::from_fn(|j| std::array::from_fn(|i| two_mu * eps_d[j][i]));
        let h: Matrix<D, D> = std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                self.exp_dt_tau * h_old[j][i] + self.exp_dt_tau_2 * (s[j][i] - dev_s_old[j][i])
            })
        });
        let sigma: Matrix<D, D> = std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                let u = id(i, j) * self.gamma_inf * self.kpa * theta;
                u + self.gamma_inf * s[j][i] + self.gamma_v * h[j][i]
            })
        });
        // updateDissipatedEnergy with the new (s, h).
        let q: Matrix<D, D> =
            std::array::from_fn(|j| std::array::from_fn(|i| (s[j][i] - h[j][i]) * self.gamma_v));
        let q_rate: Matrix<D, D> = std::array::from_fn(|j| {
            std::array::from_fn(|i| (s[j][i] * self.gamma_v - q[j][i]) / self.tau)
        });
        let a: Matrix<D, D> =
            std::array::from_fn(|j| std::array::from_fn(|i| eps_d[j][i] - self.alpha * q[j][i]));
        let increment = match self.dissipation {
            // UD-762: the limit of both forms for a degenerate branch
            _ if self.no_dissipation => 0.0,
            SlsDissipation::UpstreamMatrixProduct => {
                // ((ε_d − αq) · q̇ · Δt).sum(): product into a temporary,
                // then the halving-tree sum of `P(l)·Δt`.
                a.matmul(&q_rate).scale(self.dt).sum()
            }
            SlsDissipation::Contraction => {
                let mut c = a.double_dot(&q_rate);
                if D < 3 {
                    // The eliminated deviatoric normal components
                    // (`−tr_D(·)/(3 − D)` each, `3 − D` of them), i.e. the
                    // retained branch compliance `B⁻¹ = I + I⊗I/(3 − D)`.
                    c += a.trace() * q_rate.trace() / reduced_count::<D>();
                }
                c * self.dt
            }
        };
        (sigma, s, h, e_diss_old + increment)
    }
}

/// The stress and new state of a contiguous run of points (a part of
/// `compute_stress`): `inputs` = gradient, deviatoric stress, history
/// integral, dissipated energy of the start state. No-alias slice
/// parameters, not inlined into the part closure.
#[inline(never)]
fn sls_stress_part<const DIM: usize>(
    c: &SlsConstants,
    inputs: [&[Real]; 4],
    so: &mut [Real],
    dout: &mut [Real],
    hout: &mut [Real],
    eout: &mut [Real],
) -> Result<()> {
    let n = DIM * DIM;
    let [g, d0, h0, e0] = inputs;
    let points = g
        .chunks_exact(n)
        .zip(d0.chunks_exact(n))
        .zip(h0.chunks_exact(n))
        .zip(e0)
        .zip(so.chunks_exact_mut(n))
        .zip(dout.chunks_exact_mut(n))
        .zip(hout.chunks_exact_mut(n))
        .zip(eout);
    for (((((((g, d0), h0), &e0), so), dout), hout), e_q) in points {
        let (sigma, s, h, e) = c.on_quad(as_matrix(g)?, as_matrix(d0)?, as_matrix(h0)?, e0);
        *as_matrix_mut::<DIM, DIM>(so)? = sigma;
        *as_matrix_mut::<DIM, DIM>(dout)? = s;
        *as_matrix_mut::<DIM, DIM>(hout)? = h;
        *e_q = e;
    }
    Ok(())
}

/// Standard linear solid with deviatoric viscosity (upstream
/// `MaterialStandardLinearSolidDeviatoric<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialStandardLinearSolidDeviatoric<const DIM: usize> {
    /// The parent law (`MaterialElastic<DIM>`: base, elastic parameters,
    /// thermal internals, options).
    pub elastic: MaterialElastic<DIM>,
    /// `Eta` (viscosity).
    pub eta: Real,
    /// `Ev` (stiffness of the viscous branch).
    pub ev: Real,
    e_inf: Real,
    /// Elastic deviatoric stress of the last evaluation (`dim²`).
    pub stress_dev: InternalField<Real>,
    /// History integral `h` (`dim²`).
    pub history_integral: InternalField<Real>,
    /// Dissipated energy density (1).
    pub dissipated_energy: InternalField<Real>,
    /// The new stress, deviatoric stress, history integral and dissipated
    /// energy of a block (`compute_stress`).
    scratch: ScratchBlocks<4>,
}

impl<const DIM: usize> MaterialStandardLinearSolidDeviatoric<DIM> {
    /// New law (upstream constructor). `mesh` initialises the element
    /// filter blocks.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "sls_deviatoric".into();
        Ok(Self {
            elastic,
            eta: 1.,
            ev: 1.,
            e_inf: 1.,
            stress_dev: InternalField::new("stress_dev", DIM * DIM, 0.0)?,
            history_integral: InternalField::new("history_integral", DIM * DIM, 0.0)?,
            dissipated_energy: InternalField::new("dissipated_energy", 1, 0.0)?,
            scratch: ScratchBlocks::default(),
        })
    }

    /// `Einf = E − Ev` (as of the last update).
    #[must_use]
    pub fn e_inf(&self) -> Real {
        self.e_inf
    }

    /// The law's switches.
    #[must_use]
    pub fn law_options(&self) -> &LawOptions {
        &self.elastic.options
    }

    fn committed(&self) -> bool {
        self.elastic.options.sls_committed_state
    }

    /// Upstream `updateInternalParameters`: the parent's, then
    /// `E∞ = E − Ev`. With the committed-state switch on an initialised law
    /// the three state internals get a history (committed = current).
    fn update(&mut self) -> Result<()> {
        ParameterRegistry::update_internal_parameters(&mut self.elastic)?;
        self.e_inf = self.elastic.thermal.e - self.ev;
        if self.committed() && self.elastic.base().is_init() {
            for f in [
                &mut self.stress_dev,
                &mut self.history_integral,
                &mut self.dissipated_energy,
            ] {
                if !f.has_history() {
                    f.initialize_history();
                    f.save_current_values()?;
                }
            }
        }
        Ok(())
    }

    fn constants(&self, dt: Real) -> SlsConstants {
        let e = self.elastic.thermal.e;
        let limit = self.elastic.options.sls_degenerate_branch == SlsDegenerateBranch::Limit;
        let gamma_v = self.ev / e;
        let mu = self.elastic.mu();
        let alpha = 1. / (2. * mu * gamma_v);
        // UD-762: `Ev = 0` is "no branch" (τ = ∞ whatever `Eta`; upstream's
        // `Eta/0` is already +∞ for `Eta > 0`, NaN for `Eta = 0`).
        // So is an `Ev > 0` so small that `γ_v = Ev/E` is
        // subnormal or zero and `α = 1/(2μγ_v)` overflows (`Ev ≲ 6e-309·E`):
        // the dissipation would be `(ε_d − ∞·q):q̇ = NaN`; its limit is 0.
        let no_branch = limit && (self.ev == 0. || !alpha.is_finite());
        let tau = if no_branch {
            Real::INFINITY
        } else {
            self.eta / self.ev
        };
        // UD-762: τ = 0 is instantaneous relaxation (upstream: exp(−Δt/0),
        // NaN at Δt = 0)
        let (exp_dt_tau, exp_dt_tau_2) = if limit && tau == 0. {
            (0.0, 0.0)
        } else {
            (glibc_libm::exp(-dt / tau), glibc_libm::exp(-0.5 * dt / tau))
        };
        SlsConstants {
            dt,
            tau,
            exp_dt_tau,
            exp_dt_tau_2,
            gamma_inf: self.e_inf / e,
            gamma_v,
            kpa: self.elastic.kappa(),
            mu,
            alpha,
            dissipation: self.elastic.options.sls_dissipation,
            no_dissipation: no_branch || (limit && tau == 0.),
        }
    }

    /// UD-762 ([`SlsDegenerateBranch::Limit`]): `Ev` and `Eta` must be
    /// finite and non-negative (upstream accepts anything; a negative value
    /// makes the branch recurrence grow exponentially), and `Ev ≤ E`:
    /// upstream documents `E` as the initial modulus
    /// `E = E_∞ + E_v` of two springs (`material_standard_linear_solid_
    /// deviatoric.hh:29-42`), so `E_∞ = E − Ev < 0` is outside the model (a
    /// negative long-term modulus: the equilibrium arm and the volumetric
    /// stiffness `γ_∞κ` change sign, the stored energy is indefinite).
    /// `Ev = E` (no equilibrium spring) stays admissible.
    fn check_parameters(&self) -> Result<()> {
        if self.elastic.options.sls_degenerate_branch != SlsDegenerateBranch::Limit {
            return Ok(());
        }
        for (name, v) in [("Ev", self.ev), ("Eta", self.eta)] {
            if !(v.is_finite() && v >= 0.) {
                return Err(Error::invalid(format!(
                    "sls_deviatoric: {name} = {v} must be finite and non-negative (UD-762; \
                     SlsDegenerateBranch::Upstream reproduces upstream, which accepts it)"
                )));
            }
        }
        let e = self.elastic.thermal.e;
        if self.ev > e {
            return Err(Error::invalid(format!(
                "sls_deviatoric: Ev = {} > E = {e} gives a negative long-term modulus \
                 Einf = E - Ev (UD-762, E = Einf + Ev is the initial \
                 modulus of two springs; SlsDegenerateBranch::Upstream reproduces upstream, \
                 which accepts it)",
                self.ev
            )));
        }
        Ok(())
    }

    /// The supported domain of the corrected law: isothermal small
    /// strain. Upstream ignores both (`computeStress` has no thermal
    /// pre-step and treats `∇u` as a small strain).
    fn check_domain(&self) -> Result<()> {
        if self.elastic.options.sls_energy != SlsEnergy::TwoSpring {
            return Ok(());
        }
        if self.elastic.base().finite_deformation {
            return Err(Error::Unsupported(
                "sls_deviatoric: small-strain law, finite_deformation is not supported \
                 (SlsEnergy::UpstreamElastic reproduces upstream)"
                    .into(),
            ));
        }
        if self.elastic.thermal.alpha != 0. {
            return Err(Error::Unsupported(
                "sls_deviatoric: isothermal law, thermal expansion (alpha != 0) is not modelled \
                 (SlsEnergy::UpstreamElastic reproduces upstream)"
                    .into(),
            ));
        }
        Ok(())
    }

    /// Two-spring free-energy densities ([`SlsEnergy::TwoSpring`]) at
    /// the not-ghost points `points` of `ty`, from `∇u` and the current
    /// history integral `h` (the state of the last evaluation).
    fn two_spring_densities(
        &self,
        ty: ElementType,
        points: std::ops::Range<usize>,
    ) -> Result<Vec<Real>> {
        let g = GhostType::NotGhost;
        let n = DIM * DIM;
        let e = self.elastic.thermal.e;
        let (gamma_inf, gamma_v) = (self.e_inf / e, self.ev / e);
        let (mu, kpa) = (self.elastic.mu(), self.elastic.kappa());
        let grad_u = self.elastic.base().grad_u.get(ty, g)?;
        let h = self.history_integral.get(ty, g)?;
        if h.len() != grad_u.len() || points.end > grad_u.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialStandardLinearSolidDeviatoric: stored energy",
                expected: grad_u.len(),
                got: h.len().min(points.end),
            });
        }
        let mut out = Vec::with_capacity(points.len());
        for q in points {
            let r = q * n..(q + 1) * n;
            let eps = grad_u_to_epsilon(as_matrix::<DIM, DIM>(&grad_u.as_slice()[r.clone()])?);
            let theta = eps.trace();
            let sigma_inf: Matrix<DIM, DIM> = std::array::from_fn(|j| {
                std::array::from_fn(|i| {
                    gamma_inf
                        * (kpa * theta * id(i, j) + 2. * mu * (eps[j][i] - id(i, j) * theta / 3.))
                })
            });
            let hq: &Matrix<DIM, DIM> = as_matrix(&h.as_slice()[r])?;
            out.push(0.5 * sigma_inf.double_dot(&eps) + gamma_v / (4. * mu) * branch_norm(hq));
        }
        Ok(out)
    }

    /// Integral of the dissipated-energy density over the not-ghost
    /// elements of the law's dimension (upstream `getDissipatedEnergy()`).
    fn dissipated_energy_total(&self, ctx: &LawContext<'_>) -> Result<Real> {
        let base = self.elastic.base();
        let mut de = 0.0;
        for ty in base.filter_types(GhostType::NotGhost) {
            let f = base.element_filter.get(ty, GhostType::NotGhost)?;
            let e = self.dissipated_energy.get(ty, GhostType::NotGhost)?;
            if f.is_empty() || e.is_empty() {
                continue;
            }
            de += ctx.services.integrate_scalar(
                e.as_slice(),
                ty,
                GhostType::NotGhost,
                f.as_slice(),
            )?;
        }
        Ok(de)
    }

    /// Dissipated energy of one law-local element (upstream
    /// `getDissipatedEnergy(element)`, with the mesh element's Jacobian).
    fn dissipated_energy_of_element(&self, ctx: &LawContext<'_>, element: Element) -> Result<Real> {
        integrate_element_density(ctx, self.elastic.base(), &self.dissipated_energy, element)
    }
}

/// Integral of a scalar internal over one law-local element (its quadrature
/// points), with the Jacobian of the corresponding mesh element.
pub(crate) fn integrate_element_density(
    ctx: &LawContext<'_>,
    base: &MaterialBase,
    field: &InternalField<Real>,
    element: Element,
) -> Result<Real> {
    let ty = element.ty();
    let nq = ctx.services.nb_integration_points(ty, GhostType::NotGhost);
    let values = field.get(ty, GhostType::NotGhost)?;
    let range = element.idx() * nq..(element.idx() + 1) * nq;
    let density = values.as_slice().get(range).ok_or(Error::InvalidIndex {
        what: "law-local element",
        value: i64::from(element.index()),
    })?;
    let global = base
        .element_filter
        .get(ty, GhostType::NotGhost)?
        .as_slice()
        .get(element.idx())
        .copied()
        .ok_or(Error::InvalidIndex {
            what: "law-local element",
            value: i64::from(element.index()),
        })?;
    ctx.services
        .integrate_scalar(density, ty, GhostType::NotGhost, &[global])
}

impl<const DIM: usize> MaterialStandardLinearSolidDeviatoric<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        crate::real_param!(Self, "Eta", ParameterAccess::PARSMOD, "Viscosity", eta),
        crate::real_param!(
            Self,
            "Ev",
            ParameterAccess::PARSMOD,
            "Stiffness of the viscous element",
            ev
        ),
        crate::readable_real_param!(Self, "Einf", "Stiffness of the elastic element", e_inf),
    ];
}

impl<const DIM: usize> DerivedMaterialParams for MaterialStandardLinearSolidDeviatoric<DIM> {
    type Parent = MaterialElastic<DIM>;
    fn parent(&self) -> &Self::Parent {
        &self.elastic
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.elastic
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        Self::PARAMS
    }
    fn update_derived(&mut self) -> Result<()> {
        self.update()
    }
    fn derived_id(&self) -> &str {
        &self.elastic.base().id
    }
}

crate::impl_derived_material_registry!(MaterialStandardLinearSolidDeviatoric<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialStandardLinearSolidDeviatoric<DIM> {
    fn name(&self) -> &str {
        self.elastic.name()
    }
    fn law_id(&self) -> &str {
        self.elastic.law_id()
    }
    fn law_type(&self) -> &str {
        self.elastic.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.elastic.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.elastic.element_filter_mut()
    }
    /// The base's `(dimension, kind)` restriction for the law's own
    /// internals too (upstream `registerInternal` / `InternalField` with the
    /// law's dimension and `_ek_regular`; row CL-002).
    fn internal_restriction(&self) -> Option<akantu_model::internal_field::TypeRestriction> {
        Some(Material::base(self).internal_restriction())
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        f("stress_dev", InternalFieldRef::Real(&self.stress_dev));
        f(
            "history_integral",
            InternalFieldRef::Real(&self.history_integral),
        );
        f(
            "dissipated_energy",
            InternalFieldRef::Real(&self.dissipated_energy),
        );
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        f("stress_dev", InternalFieldMut::Real(&mut self.stress_dev))?;
        f(
            "history_integral",
            InternalFieldMut::Real(&mut self.history_integral),
        )?;
        f(
            "dissipated_energy",
            InternalFieldMut::Real(&mut self.dissipated_energy),
        )
    }
}

impl<const DIM: usize> Material for MaterialStandardLinearSolidDeviatoric<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial` (not overridden): history of
    /// the base internals, `resizeInternals`, updates (the most derived
    /// `updateInternalParameters`), `ν = 0` in 1-D. With the committed-state
    /// switch the three state internals get a history first.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.base_mut().init_history()?;
        if self.committed() {
            self.stress_dev.initialize_history();
            self.history_integral.initialize_history();
            self.dissipated_energy.initialize_history();
        }
        let blocks = self.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update()?;
        self.check_parameters()
    }

    /// Upstream `computeStress` (no thermal pre-step: upstream does not call
    /// the parent's `computeStress`) followed by `updateDissipatedEnergy`.
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.check_domain()?;
        self.check_parameters()?;
        let c = self.constants(ctx.services.time_step());
        let committed = self.committed();
        let n = DIM * DIM;
        let exec = crate::material::points_exec(self, ctx);
        let Self {
            elastic,
            stress_dev,
            history_integral,
            dissipated_energy,
            scratch,
            ..
        } = self;
        let nq = {
            let grad_u = elastic.base().grad_u.get(ty, ghost)?;
            let nq = grad_u.len();
            // The state the evaluation starts from: committed history or the
            // current values (upstream).
            let (dev_old, h_old, e_old): (
                ArrayRef<'_, Real>,
                ArrayRef<'_, Real>,
                ArrayRef<'_, Real>,
            ) = if committed {
                (
                    stress_dev.previous(ty, ghost)?.into(),
                    history_integral.previous(ty, ghost)?.into(),
                    dissipated_energy.previous(ty, ghost)?.into(),
                )
            } else {
                (
                    stress_dev.get(ty, ghost)?,
                    history_integral.get(ty, ghost)?,
                    dissipated_energy.get(ty, ghost)?,
                )
            };
            if dev_old.len() != nq || h_old.len() != nq || e_old.len() != nq {
                return Err(Error::DimensionMismatch {
                    context: "MaterialStandardLinearSolidDeviatoric::computeStress",
                    expected: nq,
                    got: dev_old.len().min(h_old.len()).min(e_old.len()),
                });
            }
            // every value is written below (whole matrices per point)
            let [sigma_out, dev_out, h_out, e_out] =
                scratch.sized([(nq, n), (nq, n), (nq, n), (nq, 1)])?;
            if grad_u.nb_component() != n
                || dev_old.nb_component() != n
                || h_old.nb_component() != n
            {
                return Err(Error::DimensionMismatch {
                    context: "MaterialStandardLinearSolidDeviatoric::computeStress: components",
                    expected: n,
                    got: dev_old.nb_component(),
                });
            }
            // every point from its own gradient and history into disjoint
            // ranges of the scratch blocks: parts on the model's threads
            // (D-123); the commits below run after every part
            let items = (
                (
                    TuplesRef::new(grad_u.as_slice(), n),
                    TuplesRef::new(dev_old.as_slice(), n),
                    TuplesRef::new(h_old.as_slice(), n),
                    TuplesRef::new(e_old.as_slice(), 1),
                ),
                Tuples::new(sigma_out.as_mut_slice(), n),
                Tuples::new(dev_out.as_mut_slice(), n),
                Tuples::new(h_out.as_mut_slice(), n),
                Tuples::new(e_out.as_mut_slice(), 1),
            );
            exec.try_for_each(
                items,
                1024,
                |_, ((g, d0, h0, e0), mut so, mut dout, mut hout, mut eout)| {
                    sls_stress_part::<DIM>(
                        &c,
                        [g.as_slice(), d0.as_slice(), h0.as_slice(), e0.as_slice()],
                        so.as_mut_slice(),
                        dout.as_mut_slice(),
                        hout.as_mut_slice(),
                        eout.as_mut_slice(),
                    )
                },
            )?;
            nq
        };
        let sigma = elastic.base_mut().sigma_field_mut()?;
        let len = sigma.values().shared(ty, ghost)?.len();
        if len != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialStandardLinearSolidDeviatoric::computeStress: stress",
                expected: nq,
                got: len,
            });
        }
        sigma.commit_overwrite(ty, ghost, scratch.get_mut(0))?;
        stress_dev.commit_overwrite(ty, ghost, scratch.get_mut(1))?;
        history_integral.commit_overwrite(ty, ghost, scratch.get_mut(2))?;
        dissipated_energy.commit_overwrite(ty, ghost, scratch.get_mut(3))?;
        Ok(())
    }

    /// A point's stress and history read only that point's gradient and
    /// history (current or committed values, copies of the blocks) and the
    /// law's constants for the time step.
    fn independent_points(&self) -> bool {
        true
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        self.elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)
    }

    /// [`SlsEnergy::TwoSpring`] (default): the free energy of the two
    /// springs; [`SlsEnergy::UpstreamElastic`]: the parent's
    /// `½σ:∇u`.
    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        if self.elastic.options.sls_energy == SlsEnergy::UpstreamElastic {
            return self.elastic.compute_potential_energy(ctx, ty);
        }
        let nq = self
            .elastic
            .base()
            .grad_u
            .get(ty, GhostType::NotGhost)?
            .len();
        let density = self.two_spring_densities(ty, 0..nq)?;
        let mut epot = self
            .elastic
            .base_mut()
            .potential_energy
            .get_mut(ty, GhostType::NotGhost)?;
        if epot.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialStandardLinearSolidDeviatoric: potential_energy",
                expected: nq,
                got: epot.len(),
            });
        }
        epot.as_mut_slice().copy_from_slice(&density);
        Ok(())
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        if self.elastic.options.sls_energy == SlsEnergy::UpstreamElastic {
            return self.elastic.potential_energy_by_element(ctx, element);
        }
        let points = super::damage::element_points(self.elastic.base(), ctx, element)?;
        self.two_spring_densities(element.ty(), points)
    }

    fn push_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.push_wave_speed(element)
    }

    fn shear_wave_speed(&self, element: Element) -> Result<Real> {
        self.elastic.shear_wave_speed(element)
    }

    fn tangent_type(&self) -> MatrixType {
        MatrixType::Symmetric
    }

    fn has_stiffness_matrix_changed(&self) -> bool {
        self.elastic.has_stiffness_matrix_changed()
    }

    fn mark_stiffness_changed(&mut self) {
        self.elastic.mark_stiffness_changed();
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn compute_all_cauchy_stresses(&mut self, ghost: GhostType) -> Result<()> {
        self.elastic.compute_all_cauchy_stresses(ghost)
    }

    /// Upstream `getEnergy(type)`: `"dissipated"` and
    /// `"dissipated_sls_deviatoric"` integrate `dissipated_energy`; anything
    /// else is the parent's (`"potential"`).
    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "dissipated" | "dissipated_sls_deviatoric" => self.dissipated_energy_total(ctx),
            // this law's computePotentialEnergy, integrated
            "potential" => Material::potential_energy(self, ctx),
            _ => self.elastic.energy_of(ctx, energy_id),
        }
    }

    fn energy_of_element(
        &mut self,
        ctx: &LawContext<'_>,
        energy_id: &str,
        element: Element,
    ) -> Result<Real> {
        match energy_id {
            "dissipated" | "dissipated_sls_deviatoric" => {
                self.dissipated_energy_of_element(ctx, element)
            }
            "potential" => self.potential_energy_of_element(ctx, element),
            _ => Ok(0.0),
        }
    }

    /// Upstream `setToSteadyState(type, ghost)`:
    /// `s = (2μ)·((∇u + ∇uᵀ)/2 − (Θ·I)/3)` with `Θ = tr ∇u`, `h = 0`. With the
    /// committed-state switch the steady state is also the committed state
    /// (previous values), so the next step starts from it.
    fn set_to_steady_state(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = DIM * DIM;
        let two_mu = 2. * self.elastic.mu();
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let mut dev = Vec::with_capacity(grad_u.as_slice().len());
        for g in grad_u.as_slice().chunks_exact(n) {
            let g: &Matrix<DIM, DIM> = as_matrix(g)?;
            let theta = g.trace();
            let s: Matrix<DIM, DIM> = std::array::from_fn(|j| {
                std::array::from_fn(|i| two_mu * ((g[j][i] + g[i][j]) / 2. - theta * id(i, j) / 3.))
            });
            dev.extend_from_slice(s.as_flattened());
        }
        let mut d = self.stress_dev.get_mut(ty, ghost)?;
        if d.as_slice().len() != dev.len() {
            return Err(Error::DimensionMismatch {
                context: "MaterialStandardLinearSolidDeviatoric::setToSteadyState",
                expected: dev.len(),
                got: d.as_slice().len(),
            });
        }
        d.as_mut_slice().copy_from_slice(&dev);
        drop(d); // before `self` is used again (explicit guard `Drop`)
        self.history_integral
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .fill(0.0);
        if self.committed() {
            for f in [&mut self.stress_dev, &mut self.history_integral] {
                if f.has_history() {
                    let cur = f.get(ty, ghost)?.clone();
                    set_previous_block(f, ty, ghost, &cur)?;
                }
            }
        }
        Ok(())
    }
}

/// `previous(ty, ghost) ← values` for one block (the history API only
/// copies whole fields; restore/save around a temporary keeps the other
/// blocks).
pub(crate) fn set_previous_block(
    f: &mut InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
    values: &Array<Real>,
) -> Result<()> {
    // current ← previous for every block, then the block's values, then
    // save (previous ← current), then restore the other current blocks.
    let current = f.values().to_plain()?;
    f.restore_previous_values()?;
    f.get_mut(ty, ghost)?.copy_from(values)?;
    f.save_current_values()?;
    for (t, g, a) in current.as_map().iter_all() {
        f.get_mut(t, g)?.copy_from(a)?;
    }
    Ok(())
}

/// Register `sls_deviatoric` (upstream
/// `instantiateMaterial<MaterialStandardLinearSolidDeviatoric>("sls_deviatoric")`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "sls_deviatoric",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialStandardLinearSolidDeviatoric::<D>::new(spec, mesh)?)
                    as Box<dyn Material>
            })
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn constants(dissipation: SlsDissipation) -> SlsConstants {
        // E = 15, ν = 0.25, η = 10, Ev = 5 (upstream test parameters).
        let (e, nu, eta, ev, dt) = (15.0_f64, 0.25_f64, 10.0, 5.0, 0.5);
        let mu = e / (2. * (1. + nu));
        let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        let tau = eta / ev;
        SlsConstants {
            dt,
            tau,
            exp_dt_tau: (-dt / tau).exp(),
            exp_dt_tau_2: (-0.5 * dt / tau).exp(),
            gamma_inf: (e - ev) / e,
            gamma_v: ev / e,
            kpa: lambda + 2. / 3. * mu,
            mu,
            alpha: 1. / (2. * mu * (ev / e)),
            dissipation,
            no_dissipation: false,
        }
    }

    /// UD-029: for symmetric traceless `A = B = [[1,1,0],[1,0,0],[0,0,−1]]`
    /// the entry sum of `A·B` is 6 while `A: B = 4`.
    #[test]
    fn ud029_matrix_product_sum_is_not_a_contraction() {
        let a: Matrix<3, 3> = MatrixExt::from_rows([[1., 1., 0.], [1., 0., 0.], [0., 0., -1.]]);
        assert_eq!(a.matmul(&a).sum(), 6.0);
        assert_eq!(a.double_dot(&a), 4.0);
    }

    /// Both dissipation forms coincide on a diagonal (principal-frame)
    /// state and the contraction is non-negative along a loading path.
    #[test]
    fn dissipation_forms_agree_in_the_principal_frame() {
        for diss in [
            SlsDissipation::Contraction,
            SlsDissipation::UpstreamMatrixProduct,
        ] {
            let c = constants(diss);
            let mut s0 = [[0.0; 3]; 3];
            let mut h0 = [[0.0; 3]; 3];
            let mut e = 0.0;
            for k in 1..=20 {
                let x = 1e-3 * f64::from(k) / 20.;
                let g: Matrix<3, 3> =
                    MatrixExt::from_rows([[x, 0., 0.], [0., -0.3 * x, 0.], [0., 0., 0.1 * x]]);
                let (_, s, h, e1) = c.on_quad(&g, &s0, &h0, e);
                assert!(e1 >= e, "{diss:?}: dissipation decreased at step {k}");
                (s0, h0, e) = (s, h, e1);
            }
        }
        let a = constants(SlsDissipation::Contraction);
        let b = constants(SlsDissipation::UpstreamMatrixProduct);
        let g: Matrix<3, 3> = MatrixExt::from_rows([[1e-3, 0., 0.], [0., -2e-4, 0.], [0., 0., 0.]]);
        let z = [[0.0; 3]; 3];
        let (_, _, _, ea) = a.on_quad(&g, &z, &z, 0.0);
        let (_, _, _, eb) = b.on_quad(&g, &z, &z, 0.0);
        assert!((ea - eb).abs() <= 1e-15 * ea.abs().max(1e-300));
    }

    /// Upstream `epsilon_d`: `ε − (I·Θ)/3` with `ε = ½(∇uᵀ + ∇u)`.
    fn deviatoric_strain<const D: usize>(grad_u: &Matrix<D, D>) -> Matrix<D, D> {
        let eps = grad_u_to_epsilon(grad_u);
        let theta = eps.trace();
        std::array::from_fn(|j| std::array::from_fn(|i| eps[j][i] - id(i, j) * theta / 3.))
    }

    #[test]
    fn deviatoric_strain_is_traceless() {
        let g: Matrix<3, 3> =
            MatrixExt::from_rows([[1e-3, 2e-4, 0.], [0., -5e-4, 1e-4], [3e-4, 0., 2e-4]]);
        let d = deviatoric_strain(&g);
        assert!(d.trace().abs() < 1e-19);
    }
}
