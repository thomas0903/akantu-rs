// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_viscoelastic/material_viscoelastic_maxwell.hh,
// material_viscoelastic_maxwell.cc.

//! `viscoelastic_maxwell`: generalised Maxwell model (upstream
//! `MaterialViscoelasticMaxwell<dim>`, parent `MaterialElastic<dim>`), a
//! spring `Einf` in parallel with `N` Maxwell branches (spring `Ev_k` in
//! series with a dashpot `Eta_k`), small strain, all spatial dimensions
//! (2-D always plane strain: the branch stiffness ignores `Plane_Stress`).
//!
//! Parameters: `Einf` (default 1), vectors `Eta`, `Ev` (same length `N`),
//! read-only `previous_dt`, plus the elastic ones (`E` is overwritten by
//! `initMaterial` with `Einf + Σ|Ev_k|` before the Lamé constants are
//! derived). Internals: `sigma_v`, `epsilon_v` (`dim²·N`, registered by
//! `initMaterial`; branch `k` is the column-major `dim × dim` tensor at
//! components `k·dim² …`), `dissipated_energy`, `mechanical_work` (1).
//!
//! # Kernel (upstream order, decision D-008)
//!
//! `C`, `D`: unit-modulus isotropic stiffness (plane strain in 2-D) and
//! compliance in Akantu's Voigt order (engineering shear). With `λ_k =
//! η_k/E_k`, `e_k = exp(−Δt/λ_k)` (glibc `exp`, ported) and
//! `E_k^eff = E_k` if `e_k == 1`, else `((1 − e_k)·E_k)·λ_k/Δt`:
//!
//! ```text
//! σ_v  = (Einf·C)·ε_v(∇u) ;  Δ = C·(ε_v(∇u) − ε_v(∇u_prev))
//! σ_v += E_k^eff·Δ + e_k·voigt(σ_v,k)              (k = 1 … N)
//! σ(i,j) = σ(j,i) = σ_v(I) + δ_ij·σ_th
//! tangent = (Einf + Σ_k E_k^eff)·C ;  previous_dt = Δt
//! ψ = ½ (Einf·C·ε_v)·ε_v + Σ_k σ_v,k : ε_v,k / 2   (after a converged step)
//! W += ((σ + σ_prev)/2) : ((∇u − ∇u_prev)/Δt) · Δt ;  e_diss = W − ψ
//! ```
//!
//! `getEnergy("potential")` integrates the stored `potential_energy`
//! internal without recomputing it (upstream; it is refreshed after every
//! converged step, so mid-step it lags by one step).
//! `hasStiffnessMatrixChanged` is true after any parameter update since
//! the last successful tangent (UD-157, see below) or when `previous_dt ≠
//! Δt` (the model's time step, [`Material::has_stiffness_matrix_changed_in`];
//! the state-free variant compares with the last time step the law
//! evaluated). [`LawOptions::upstream`] reproduces upstream bit for bit.
//!
//! # Upstream defects
//!
//! * **UD-029** (Maxwell part, category B, `LawOptions::maxwell_update`):
//!   after a converged step upstream
//!   computes the new branch stress into a temporary that is immediately
//!   overwritten, so `sigma_v` never changes (initially zero branches stay
//!   zero; the relaxation test is `UNSTABLE`). [`MaxwellUpdate::Recurrence`]
//!   (default) commits the recurrence
//!   `q_k ← e_k·q_k + E_k^eff·C:Δε`, evaluated with the stress kernel's own
//!   arithmetic, so the committed branch stress is bit for bit the term the
//!   converged stress contained (`σ = Einf·C·ε + Σ_k q_k`). Repeated
//!   residual evaluations do not commit anything (the update runs once, in
//!   `afterSolveStep(true)`). Ghost elements are updated too, so ghost
//!   copies advance with their owners (upstream updates only not-ghost
//!   blocks — immaterial upstream since nothing changes).
//! * **UD-162**, **UD-163**: branch strains (only observable once branch
//!   stresses are non-zero): upstream stores `ε_v,k = D·q_k/E_k` with the
//!   *engineering* shear `2ε_ij` in both off-diagonal tensor slots, so
//!   `σ_v,k : ε_v,k` counts the shear energy twice; and in 2-D `D` is the
//!   in-plane block of the 3-D compliance (plane stress) while `C` is the
//!   plane-strain stiffness, so `D ≠ C⁻¹`. The corrected mode stores the
//!   tensorial strain (`γ/2` off the diagonal) with the compliance
//!   `S = C⁻¹` (3-D and 1-D: upstream's `D`; 2-D: `S11 = S22 = 1 − ν²`,
//!   `S12 = −ν(1 + ν)`, `S33 = 2(1 + ν)`), so `ψ` is the stored energy of
//!   the branch springs and `W = ψ + e_diss` with `e_diss ≥ 0`.
//! * **UD-157** (category B, `LawOptions::maxwell_stiffness_invalidation`):
//!   upstream's `hasStiffnessMatrixChanged` only compares `previous_dt`
//!   with the time step, so a parameter update at a fixed time step
//!   (`Einf`, `Ev`, `Eta`, `E`, `nu`) changes the tangent without flagging
//!   it. By default every parameter update flags the stiffness until the
//!   next successful tangent; `false` reproduces upstream.
//! * `Eta`/`Ev` of different lengths are rejected at `initMaterial`
//!   (upstream: debug assertion only), as is `finite_deformation`.
//! * **UD-327** (category B, `LawOptions::
//!   maxwell_thermal`): upstream adds the parent's thermal stress `σ_th`
//!   (`−E·αΔT` in 1-D, `−E/(1 − 2ν)·αΔT` in plane strain and 3-D) of the
//!   **instantaneous** modulus `E = Einf + Σ|Ev_k|` to the Maxwell stress,
//!   while the equilibrium
//!   spring, the branch recurrence and the stored energy see the total
//!   strain: `ψ` is not a potential of the stress (heating at zero strain:
//!   `σ = −E·αΔT`, `ψ = 0`), nominal free expansion keeps a stress
//!   `−Einf·αΔT` forever and the reported dissipation `W − ψ` becomes
//!   negative (1-D `Einf = 10`, `Ev = [5]`, `Eta = [10]`, `alpha = 0.01`,
//!   `ΔT = 1`, `Δt = 0.1`: `D = −1.43e−3` after a ramp `H11: 0 → 0.01`).
//!   No thermo-viscoelastic formulation has been specified and checked, so
//!   [`MaxwellThermal::Unsupported`] (default) refuses `alpha ≠ 0` at
//!   `initMaterial`, `computeStress`, `computePotentialEnergy` and
//!   `afterSolveStep`; [`MaxwellThermal::Upstream`] reproduces upstream. A
//!   full fix must apply the thermal strain to the equilibrium spring and
//!   the branch evolution and define the thermal work and stored energy
//!   consistently (not attempted; dissipation is never clamped).

use std::sync::Arc;

use akantu_core::eigen_order as eo;
use akantu_core::enums::MatrixType;
use akantu_core::glibc_libm;
use akantu_core::tensor::{Matrix, as_matrix};
use akantu_core::voigt::VoigtTable;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::ParameterRegistry;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess, fit_vector};

use super::elastic::MaterialElastic;
use super::scratch::ScratchBlocks;
use super::sls_deviatoric::integrate_element_density;
use crate::material::{
    DerivedMaterialParams, LawOptions, Material, MaterialBase, MaterialFactory, check_tangent,
};
use crate::material_tensor::grad_u_to_epsilon;

/// Branch update of `viscoelastic_maxwell` after a converged step (UD-029
/// and the branch-strain defects UD-162 and UD-163, category B).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MaxwellUpdate {
    /// Exponential recurrence of the branch stresses, tensorial branch
    /// strains with the compliance `C⁻¹`.
    Recurrence,
    /// Upstream: branch stresses unchanged (symmetrised from their Voigt
    /// entries), `ε_v = D·σ_v/E_k` with engineering shear in the tensor.
    Upstream,
}

/// Thermal loading of `viscoelastic_maxwell` (UD-327, category B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MaxwellThermal {
    /// Default: no thermo-viscoelastic formulation of the generalised
    /// Maxwell model has been specified and checked, so a law with thermal
    /// expansion (`alpha ≠ 0`) is [`Error::Unsupported`] at `initMaterial`,
    /// `computeStress`, `computePotentialEnergy` and `afterSolveStep` (the
    /// branch update and energy bookkeeping). Isothermal laws (`alpha = 0`,
    /// any `delta_T`) are unaffected.
    #[default]
    Unsupported,
    /// Upstream: the parent's thermal stress `σ_th` (instantaneous modulus
    /// `E = Einf + Σ|Ev_k|`) is added to the Maxwell stress, while the
    /// equilibrium spring, the branch recurrence and the stored energy use
    /// the total strain (no thermal strain): `ψ` is not a potential of the
    /// stress, free expansion leaves a persistent stress and the reported
    /// dissipation can be negative.
    Upstream,
}

/// Largest Voigt size.
const VMAX: usize = 6;

/// Eigen evaluator cost of one coefficient of
/// `((σ + σ_prev)/2.).cwiseProduct((∇u − ∇u_prev)/Δt)`: per operand two
/// reads, one addition and one division (`scalar_div_cost<double>` = 8), plus
/// the product.
const AV_STRESS_RATE_COST: usize = 2 * (2 * eo::READ_COST + eo::OP_COST + 8) + eo::OP_COST;

/// A `v × v` Voigt matrix (row `I`, column `J` at `[I][J]`).
type Voigt = [[Real; VMAX]; VMAX];

/// Upstream `strainToVoigt(gradUToEpsilon(g))`: `factor(I)·ε(i, j)`.
fn strain_voigt<const D: usize>(t: &VoigtTable, g: &Matrix<D, D>) -> [Real; VMAX] {
    let eps = grad_u_to_epsilon(g);
    let mut v = [0.0; VMAX];
    for (big_i, slot) in v.iter_mut().enumerate().take(t.size) {
        let (i, j) = t.vec(big_i);
        *slot = t.factor(big_i) * eps[j][i];
    }
    v
}

/// Upstream `stressToVoigt` of branch `k` of a `sigma_v`-like tuple
/// (`v(I) = σ(i, j)`).
fn branch_voigt(t: &VoigtTable, tuple: &[Real], k: usize) -> [Real; VMAX] {
    let d = t.dim;
    let page = &tuple[k * d * d..(k + 1) * d * d];
    let mut v = [0.0; VMAX];
    for (big_i, slot) in v.iter_mut().enumerate().take(t.size) {
        let (i, j) = t.vec(big_i);
        *slot = page[i + j * d];
    }
    v
}

/// Coefficient-based product `(s·M)·x` of a fixed `v × v` matrix and a
/// fixed vector: per row the halving-tree sum of `(s·M(I, k))·x(k)`.
fn scaled_matvec(v: usize, s: Real, m: &Voigt, x: &[Real; VMAX]) -> [Real; VMAX] {
    let mut out = [0.0; VMAX];
    for (big_i, o) in out.iter_mut().enumerate().take(v) {
        *o = eo::product_coeff(v, true, &|k| (s * m[big_i][k]) * x[k]);
    }
    out
}

/// Coefficient-based product `M·x` (no scaling).
fn matvec(v: usize, m: &Voigt, x: &[Real; VMAX]) -> [Real; VMAX] {
    let mut out = [0.0; VMAX];
    for (big_i, o) in out.iter_mut().enumerate().take(v) {
        *o = eo::product_coeff(v, true, &|k| m[big_i][k] * x[k]);
    }
    out
}

/// Generalised Maxwell law (upstream `MaterialViscoelasticMaxwell<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialViscoelasticMaxwell<const DIM: usize> {
    /// The parent law (`MaterialElastic<DIM>`).
    pub elastic: MaterialElastic<DIM>,
    /// `Eta` (viscosities).
    pub eta: Vec<Real>,
    /// `Ev` (branch stiffnesses).
    pub ev: Vec<Real>,
    /// `Einf` (spring in parallel).
    pub e_inf: Real,
    previous_dt: Real,
    /// Time step of the last evaluation (state-free stiffness query).
    current_dt: Real,
    /// Set by `mark_stiffness_changed` (options changed) and, with
    /// `LawOptions::maxwell_stiffness_invalidation`, by every parameter
    /// update; cleared by the next successful tangent.
    force_stiffness_changed: bool,
    c: Voigt,
    d: Voigt,
    /// Compliance used by the corrected branch update (`C⁻¹`).
    s: Voigt,
    /// Branch stresses (`dim²·N`, registered by `initMaterial`).
    pub sigma_v: Option<InternalField<Real>>,
    /// Branch spring strains (`dim²·N`, registered by `initMaterial`).
    pub epsilon_v: Option<InternalField<Real>>,
    /// Dissipated energy density.
    pub dissipated_energy: InternalField<Real>,
    /// Mechanical work density.
    pub mechanical_work: InternalField<Real>,
    update_variable_flag: bool,
    /// The new stress or potential energy density of a block, committed
    /// with `InternalField::commit_overwrite`.
    scratch: ScratchBlocks<1>,
}

impl<const DIM: usize> MaterialViscoelasticMaxwell<DIM> {
    /// Voigt size of the law.
    const V: usize = DIM * (DIM - 1) / 2 + DIM;

    /// New law (upstream constructor).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "viscoelastic_maxwell".into();
        Ok(Self {
            elastic,
            eta: Vec::new(),
            ev: Vec::new(),
            e_inf: 1.,
            previous_dt: 0.,
            current_dt: 0.,
            force_stiffness_changed: false,
            c: [[0.0; VMAX]; VMAX],
            d: [[0.0; VMAX]; VMAX],
            s: [[0.0; VMAX]; VMAX],
            sigma_v: None,
            epsilon_v: None,
            dissipated_energy: InternalField::new("dissipated_energy", 1, 0.0)?,
            mechanical_work: InternalField::new("mechanical_work", 1, 0.0)?,
            update_variable_flag: true,
            scratch: ScratchBlocks::default(),
        })
    }

    /// Upstream `forceUpdateVariable`.
    pub fn force_update_variable(&mut self) {
        self.update_variable_flag = true;
    }

    /// Upstream `forceNotUpdateVariable`.
    pub fn force_not_update_variable(&mut self) {
        self.update_variable_flag = false;
    }

    /// Time step of the last tangent (upstream `previous_dt`).
    #[must_use]
    pub fn previous_dt(&self) -> Real {
        self.previous_dt
    }

    /// Unit-modulus stiffness `C` (row-major `v × v`).
    #[must_use]
    pub fn stiffness(&self) -> Vec<Vec<Real>> {
        (0..Self::V)
            .map(|i| self.c[i][..Self::V].to_vec())
            .collect()
    }

    /// Compliance used by the corrected branch update (`C⁻¹`).
    #[must_use]
    pub fn compliance(&self) -> Vec<Vec<Real>> {
        (0..Self::V)
            .map(|i| self.s[i][..Self::V].to_vec())
            .collect()
    }

    fn table() -> &'static VoigtTable {
        match DIM {
            1 => &akantu_core::voigt::VOIGT_1D,
            2 => &akantu_core::voigt::VOIGT_2D,
            _ => &akantu_core::voigt::VOIGT_3D,
        }
    }

    fn nb_branches(&self) -> usize {
        self.ev.len()
    }

    /// Upstream `updateInternalParameters`: the parent's, then `C`, `D`
    /// (and the corrected compliance `S`).
    fn update(&mut self) -> Result<()> {
        ParameterRegistry::update_internal_parameters(&mut self.elastic)?;
        let nu = self.elastic.thermal.nu;
        let pre_mult = 1. / (1. + nu) / (1. - 2. * nu);
        let miiii = pre_mult * (1. - nu);
        let miijj = pre_mult * nu;
        let mijij = pre_mult * 0.5 * (1. - 2. * nu);
        let diiii = 1.;
        let diijj = -nu;
        let dijij = 2. + 2. * nu;
        let (c, d) = (&mut self.c, &mut self.d);
        *c = [[0.0; VMAX]; VMAX];
        *d = [[0.0; VMAX]; VMAX];
        if DIM == 1 {
            c[0][0] = 1.;
            d[0][0] = 1.;
        } else {
            c[0][0] = miiii;
            d[0][0] = diiii;
        }
        if DIM >= 2 {
            let n = Self::V;
            c[1][1] = miiii;
            c[0][1] = miijj;
            c[1][0] = miijj;
            c[n - 1][n - 1] = mijij;
            d[1][1] = diiii;
            d[0][1] = diijj;
            d[1][0] = diijj;
            d[n - 1][n - 1] = dijij;
        }
        if DIM == 3 {
            c[2][2] = miiii;
            c[0][2] = miijj;
            c[1][2] = miijj;
            c[2][0] = miijj;
            c[2][1] = miijj;
            c[3][3] = mijij;
            c[4][4] = mijij;
            d[2][2] = diiii;
            d[0][2] = diijj;
            d[1][2] = diijj;
            d[2][0] = diijj;
            d[2][1] = diijj;
            d[3][3] = dijij;
            d[4][4] = dijij;
        }
        // Corrected compliance S = C⁻¹ (unit modulus, engineering shear).
        self.s = self.d;
        if DIM == 2 {
            let s = &mut self.s;
            s[0][0] = (1. + nu) * (1. - nu);
            s[1][1] = (1. + nu) * (1. - nu);
            s[0][1] = -nu * (1. + nu);
            s[1][0] = -nu * (1. + nu);
            s[2][2] = 2. * (1. + nu);
        }
        // UD-157: `Einf`, `Ev`, `Eta`, `E`, `nu` change the
        // tangent `(Einf + Σ E_k^eff)·C` at a fixed time step; upstream's
        // `hasStiffnessMatrixChanged` only compares time steps.
        if self.elastic.options.maxwell_stiffness_invalidation {
            self.force_stiffness_changed = true;
        }
        Ok(())
    }

    /// `(e_k, E_k^eff)` of branch `k` for the time step `dt` (upstream
    /// `computeStressOnQuad` / `computeTangentModuli` arithmetic).
    fn branch_factors(&self, k: usize, dt: Real) -> (Real, Real) {
        let (eta, ev) = (self.eta[k], self.ev[k]);
        let lambda = eta / ev;
        let exp_dt_lambda = glibc_libm::exp(-dt / lambda);
        let e_additional = if exp_dt_lambda == 1. {
            ev
        } else {
            (1. - exp_dt_lambda) * ev * lambda / dt
        };
        (exp_dt_lambda, e_additional)
    }

    /// The thermal domain of the corrected law
    /// (UD-327): without a checked thermo-viscoelastic formulation, thermal
    /// expansion is refused unless [`MaxwellThermal::Upstream`] is selected.
    fn check_thermal_domain(&self) -> Result<()> {
        if self.elastic.options.maxwell_thermal == MaxwellThermal::Unsupported
            && self.elastic.thermal.alpha != 0.
        {
            return Err(Error::Unsupported(
                "viscoelastic_maxwell: thermal expansion (alpha != 0) is not modelled: no \
                 reviewed thermo-viscoelastic formulation (UD-327; \
                 MaxwellThermal::Upstream reproduces upstream)"
                    .into(),
            ));
        }
        Ok(())
    }

    fn sigma_v_field(&self) -> Result<&InternalField<Real>> {
        self.sigma_v.as_ref().ok_or_else(|| {
            Error::invalid("viscoelastic_maxwell: sigma_v is registered by initMaterial")
        })
    }

    fn epsilon_v_field(&self) -> Result<&InternalField<Real>> {
        self.epsilon_v.as_ref().ok_or_else(|| {
            Error::invalid("viscoelastic_maxwell: epsilon_v is registered by initMaterial")
        })
    }

    /// Upstream `computeStressOnQuad` for one point.
    fn stress_on_quad(
        &self,
        dt: Real,
        grad_u: &Matrix<DIM, DIM>,
        previous_grad_u: &Matrix<DIM, DIM>,
        sigma_v: &[Real],
        sigma_th: Real,
    ) -> Matrix<DIM, DIM> {
        let t = Self::table();
        let v = Self::V;
        let cur = strain_voigt(t, grad_u);
        let prev = strain_voigt(t, previous_grad_u);
        let mut voigt_stress = scaled_matvec(v, self.e_inf, &self.c, &cur);
        let mut delta = [0.0; VMAX];
        for i in 0..v {
            delta[i] = cur[i] - prev[i];
        }
        let stress = matvec(v, &self.c, &delta);
        for k in 0..self.nb_branches() {
            let (e, e_add) = self.branch_factors(k, dt);
            let sv = branch_voigt(t, sigma_v, k);
            for i in 0..v {
                voigt_stress[i] += e_add * stress[i] + e * sv[i];
            }
        }
        let mut sigma = [[0.0; DIM]; DIM];
        for (big_i, &vs) in voigt_stress.iter().enumerate().take(v) {
            let (i, j) = t.vec(big_i);
            let kron = if i == j { 1.0 } else { 0.0 };
            let value = vs + kron * sigma_th;
            sigma[j][i] = value;
            sigma[i][j] = value;
        }
        sigma
    }

    /// Upstream `computePotentialEnergyOnQuad`.
    fn energy_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma_v: &[Real],
        epsilon_v: &[Real],
    ) -> Real {
        let t = Self::table();
        let v = Self::V;
        let strain = strain_voigt(t, grad_u);
        let stress = scaled_matvec(v, self.e_inf, &self.c, &strain);
        let mut epot = 0.5 * eo::sum(v, eo::PRODUCT_TERM_COST, true, &|i| stress[i] * strain[i]);
        let n = DIM * DIM;
        for k in 0..self.nb_branches() {
            let (a, b) = (&sigma_v[k * n..(k + 1) * n], &epsilon_v[k * n..(k + 1) * n]);
            // Dynamic `MatrixProxy` operands: sequential sum.
            let dd = eo::sum(n, eo::PRODUCT_TERM_COST, false, &|l| a[l] * b[l]);
            epot += dd / 2.;
        }
        epot
    }

    /// Upstream `updateIntVarOnQuad` (both modes) for one point: updates
    /// the `sigma_v` / `epsilon_v` tuples in place.
    #[allow(clippy::too_many_arguments)] // one quadrature point's state
    fn update_int_var_on_quad(
        &self,
        mode: MaxwellUpdate,
        dt: Real,
        grad_u: &Matrix<DIM, DIM>,
        previous_grad_u: &Matrix<DIM, DIM>,
        sigma_v: &mut [Real],
        epsilon_v: &mut [Real],
    ) {
        let t = Self::table();
        let v = Self::V;
        let d = DIM;
        let delta = match mode {
            MaxwellUpdate::Upstream => None,
            MaxwellUpdate::Recurrence => {
                // The stress kernel's Δ = C·(ε_v(∇u) − ε_v(∇u_prev)).
                let cur = strain_voigt(t, grad_u);
                let prev = strain_voigt(t, previous_grad_u);
                let mut diff = [0.0; VMAX];
                for i in 0..v {
                    diff[i] = cur[i] - prev[i];
                }
                Some(matvec(v, &self.c, &diff))
            }
        };
        for k in 0..self.nb_branches() {
            let sv = branch_voigt(t, sigma_v, k);
            let (q, eps) = match (mode, delta) {
                (MaxwellUpdate::Recurrence, Some(stress)) => {
                    let (e, e_add) = self.branch_factors(k, dt);
                    let mut q = [0.0; VMAX];
                    for i in 0..v {
                        q[i] = e_add * stress[i] + e * sv[i];
                    }
                    let mut eps = scaled_matvec(v, 1. / self.ev[k], &self.s, &q);
                    // tensorial strain: half the engineering shear
                    for (big_i, x) in eps.iter_mut().enumerate().take(v) {
                        *x /= t.factor(big_i);
                    }
                    (q, eps)
                }
                _ => {
                    // Upstream: σ_v unchanged, ε_v = (1/E_k)·D·σ_v.
                    let eps = scaled_matvec(v, 1. / self.ev[k], &self.d, &sv);
                    (sv, eps)
                }
            };
            let page_s = &mut sigma_v[k * d * d..(k + 1) * d * d];
            for (big_i, &x) in q.iter().enumerate().take(v) {
                let (i, j) = t.vec(big_i);
                page_s[i + j * d] = x;
                page_s[j + i * d] = x;
            }
            let page_e = &mut epsilon_v[k * d * d..(k + 1) * d * d];
            for (big_i, &x) in eps.iter().enumerate().take(v) {
                let (i, j) = t.vec(big_i);
                page_e[i + j * d] = x;
                page_e[j + i * d] = x;
            }
        }
    }

    /// Upstream `updateIntVariables` for the blocks of `ghost`: the branch
    /// tuples are updated in place (each point reads only its own tuples
    /// before writing them).
    fn update_int_variables(&mut self, dt: Real, ghost: GhostType) -> Result<()> {
        if self.sigma_v.is_none() || self.epsilon_v.is_none() {
            // not initialised (`init_material` registers both): the first
            // error of the block loop, as before (none for an empty filter)
            let base = self.elastic.base();
            for (ty, _) in base.element_filter.as_map().iter(ghost) {
                base.grad_u.get(ty, ghost)?;
                base.grad_u.previous(ty, ghost)?;
                self.sigma_v_field()?.get(ty, ghost)?;
                self.epsilon_v_field()?.get(ty, ghost)?;
            }
            return Ok(());
        }
        let (Some(mut sv), Some(mut ev)) = (self.sigma_v.take(), self.epsilon_v.take()) else {
            return Ok(()); // both present: checked above
        };
        let result = self.update_int_variables_of(&mut sv, &mut ev, dt, ghost);
        self.sigma_v = Some(sv);
        self.epsilon_v = Some(ev);
        result
    }

    /// [`Self::update_int_variables`] on the branch fields taken out of the
    /// law.
    fn update_int_variables_of(
        &self,
        sv_field: &mut InternalField<Real>,
        ev_field: &mut InternalField<Real>,
        dt: Real,
        ghost: GhostType,
    ) -> Result<()> {
        let mode = self.elastic.options.maxwell_update;
        let n = DIM * DIM;
        let nb = self.nb_branches();
        let base = self.elastic.base();
        for (ty, _) in base.element_filter.as_map().iter(ghost) {
            let grad_u = base.grad_u.get(ty, ghost)?;
            let prev = base.grad_u.previous(ty, ghost)?;
            let mut sv = sv_field.get_mut(ty, ghost)?;
            let mut ev = ev_field.get_mut(ty, ghost)?;
            let nq = grad_u.len();
            if prev.len() != nq || sv.len() != nq || ev.len() != nq {
                return Err(Error::DimensionMismatch {
                    context: "MaterialViscoelasticMaxwell::updateIntVariables",
                    expected: nq,
                    got: prev.len().min(sv.len()).min(ev.len()),
                });
            }
            for q in 0..nq {
                let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
                let pg: &Matrix<DIM, DIM> = as_matrix(&prev.as_slice()[q * n..(q + 1) * n])?;
                let (svs, evs) = (
                    &mut sv.as_mut_slice()[q * n * nb..(q + 1) * n * nb],
                    &mut ev.as_mut_slice()[q * n * nb..(q + 1) * n * nb],
                );
                self.update_int_var_on_quad(mode, dt, g, pg, svs, evs);
            }
        }
        Ok(())
    }

    /// Upstream `updateDissipatedEnergy(type)`: the potential energy, then
    /// work and dissipation per not-ghost quadrature point.
    fn update_dissipated_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.compute_potential_energy(ctx, ty)?;
        let g = GhostType::NotGhost;
        let n = DIM * DIM;
        let dt = ctx.services.time_step();
        let base = self.elastic.base();
        let grad_u = base.grad_u.get(ty, g)?;
        let prev_g = base.grad_u.previous(ty, g)?;
        let sigma = base.stress.get(ty, g)?;
        let prev_s = base.stress.previous(ty, g)?;
        let epot = base.potential_energy.get(ty, g)?;
        let nq = grad_u.len();
        let mut work = self.mechanical_work.get_mut(ty, g)?;
        if prev_g.len() != nq
            || sigma.len() != nq
            || prev_s.len() != nq
            || epot.len() != nq
            || work.len() != nq
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialViscoelasticMaxwell::updateDissipatedEnergy",
                expected: nq,
                got: work.len(),
            });
        }
        let works = &mut work.as_mut_slice()[..nq];
        for (q, w) in works.iter_mut().enumerate() {
            let r = q * n..(q + 1) * n;
            let (gu, pgu, s, ps) = (
                &grad_u.as_slice()[r.clone()],
                &prev_g.as_slice()[r.clone()],
                &sigma.as_slice()[r.clone()],
                &prev_s.as_slice()[r],
            );
            // `av_stress.doubleDot(strain_rate)` on unevaluated expressions:
            // coefficient cost 2·(2 reads + add + division (8)) + product =
            // 23, so Eigen unrolls the 4 terms of 2-D (tree) but not the 9 of
            // 3-D (sequential), see `akantu_core::eigen_order::sum`.
            let dd = eo::sum(n, AV_STRESS_RATE_COST, true, &|l| {
                ((s[l] + ps[l]) / 2.) * ((gu[l] - pgu[l]) / dt)
            });
            *w += dd * dt;
        }
        let mut diss = self.dissipated_energy.get_mut(ty, g)?;
        // the work just updated minus the potential energy, per point
        for ((d, w), e) in diss
            .as_mut_slice()
            .iter_mut()
            .zip(work.as_slice())
            .zip(epot.as_slice())
        {
            *d = *w - *e;
        }
        Ok(())
    }

    /// Integral of a scalar internal over the not-ghost elements of the
    /// law's dimension.
    fn integrate_field(&self, ctx: &LawContext<'_>, field: &InternalField<Real>) -> Result<Real> {
        let base = self.elastic.base();
        let mut total = 0.0;
        for ty in base.filter_types(GhostType::NotGhost) {
            let f = base.element_filter.get(ty, GhostType::NotGhost)?;
            let e = field.get(ty, GhostType::NotGhost)?;
            if f.is_empty() || e.is_empty() {
                continue;
            }
            total += ctx.services.integrate_scalar(
                e.as_slice(),
                ty,
                GhostType::NotGhost,
                f.as_slice(),
            )?;
        }
        Ok(total)
    }
}

impl<const DIM: usize> MaterialViscoelasticMaxwell<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        crate::real_param!(
            Self,
            "Einf",
            ParameterAccess::PARSMOD,
            "Stiffness of the elastic element",
            e_inf
        ),
        crate::readable_real_param!(
            Self,
            "previous_dt",
            "Time step of previous solveStep",
            previous_dt
        ),
        ParamDescriptor::new(
            "Eta",
            ParamKind::VectorReal,
            ParameterAccess::PARSMOD,
            "Viscosity of a Maxwell element",
            |s| ParamValue::VectorReal(s.eta.clone()),
            Some(|s, v| {
                if let ParamValue::VectorReal(x) = v {
                    s.eta = fit_vector(s.eta.len(), x)?;
                    Ok(())
                } else {
                    Err(Error::invalid("Eta: expected a vector"))
                }
            }),
        ),
        ParamDescriptor::new(
            "Ev",
            ParamKind::VectorReal,
            ParameterAccess::PARSMOD,
            "Stiffness of a Maxwell element",
            |s| ParamValue::VectorReal(s.ev.clone()),
            Some(|s, v| {
                if let ParamValue::VectorReal(x) = v {
                    s.ev = fit_vector(s.ev.len(), x)?;
                    Ok(())
                } else {
                    Err(Error::invalid("Ev: expected a vector"))
                }
            }),
        ),
    ];
}

impl<const DIM: usize> DerivedMaterialParams for MaterialViscoelasticMaxwell<DIM> {
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

crate::impl_derived_material_registry!(MaterialViscoelasticMaxwell<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialViscoelasticMaxwell<DIM> {
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
        f(
            "dissipated_energy",
            InternalFieldRef::Real(&self.dissipated_energy),
        );
        f(
            "mechanical_work",
            InternalFieldRef::Real(&self.mechanical_work),
        );
        if let Some(s) = &self.sigma_v {
            f("sigma_v", InternalFieldRef::Real(s));
        }
        if let Some(e) = &self.epsilon_v {
            f("epsilon_v", InternalFieldRef::Real(e));
        }
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        f(
            "dissipated_energy",
            InternalFieldMut::Real(&mut self.dissipated_energy),
        )?;
        f(
            "mechanical_work",
            InternalFieldMut::Real(&mut self.mechanical_work),
        )?;
        if let Some(s) = &mut self.sigma_v {
            f("sigma_v", InternalFieldMut::Real(s))?;
        }
        if let Some(e) = &mut self.epsilon_v {
            f("epsilon_v", InternalFieldMut::Real(e))?;
        }
        Ok(())
    }
}

impl<const DIM: usize> Material for MaterialViscoelasticMaxwell<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `initMaterial`: `E = Einf + Σ|Ev_k|` (sequential), the
    /// parent's `initMaterial` (with this law's updates), then `sigma_v` and
    /// `epsilon_v` (`dim²·N`) registered and sized.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        if self.eta.len() != self.ev.len() {
            return Err(Error::invalid(format!(
                "viscoelastic_maxwell: Eta and Ev have different dimensions ({} and {})",
                self.eta.len(),
                self.ev.len()
            )));
        }
        if self.base().finite_deformation {
            return Err(Error::invalid(
                "viscoelastic_maxwell works only in infinitesimal deformations",
            ));
        }
        self.check_thermal_domain()?;
        let ev = &self.ev;
        self.elastic.thermal.e =
            self.e_inf + eo::sum(ev.len(), eo::UNARY_TERM_COST, false, &|k| ev[k].abs());
        self.base_mut().init_history()?;
        let blocks = self.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update()?;
        if self.nb_branches() == 0 {
            // Upstream would register zero-component internals; internal
            // fields need at least one component here.
            return Err(Error::invalid(
                "viscoelastic_maxwell needs at least one Maxwell branch (Eta, Ev)",
            ));
        }
        let size = DIM * DIM * self.nb_branches();
        let mut sv = InternalField::new("sigma_v", size, 0.0)?;
        let mut ev = InternalField::new("epsilon_v", size, 0.0)?;
        sv.resize_blocks(&blocks)?;
        ev.resize_blocks(&blocks)?;
        self.sigma_v = Some(sv);
        self.epsilon_v = Some(ev);
        Ok(())
    }

    /// Upstream `computeStress`: the parent's `computeStress` (thermal
    /// pre-step, elastic stress), then the Maxwell stress per point. By
    /// default `alpha ≠ 0` is refused first (UD-327).
    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.check_thermal_domain()?;
        self.elastic.compute_stress(ctx, ty, ghost)?;
        let dt = ctx.services.time_step();
        self.current_dt = dt;
        let n = DIM * DIM;
        let nb = self.nb_branches();
        let mut scratch = std::mem::take(&mut self.scratch);
        let base = self.elastic.base();
        let grad_u = base.grad_u.get(ty, ghost)?;
        let prev = base.grad_u.previous(ty, ghost)?;
        let sv = self.sigma_v_field()?.get(ty, ghost)?;
        let th = self.elastic.sigma_th.get(ty, ghost)?;
        let nq = grad_u.len();
        if prev.len() != nq || sv.len() != nq || th.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialViscoelasticMaxwell::computeStress",
                expected: nq,
                got: prev.len().min(sv.len()).min(th.len()),
            });
        }
        // every value of the scratch block is written below
        let [out] = scratch.sized([(nq, n)])?;
        for q in 0..nq {
            let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
            let pg: &Matrix<DIM, DIM> = as_matrix(&prev.as_slice()[q * n..(q + 1) * n])?;
            let s = self.stress_on_quad(
                dt,
                g,
                pg,
                &sv.as_slice()[q * n * nb..(q + 1) * n * nb],
                th.as_slice()[q],
            );
            out.as_mut_slice()[q * n..(q + 1) * n].copy_from_slice(s.as_flattened());
        }
        // end the read guards before the stress guard (explicit guard
        // `Drop`)
        drop((grad_u, sv, th));
        let result = commit_block(
            &mut self.elastic.base_mut().stress,
            ty,
            ghost,
            scratch.get_mut(0),
            "MaterialViscoelasticMaxwell::computeStress: stress",
        );
        self.scratch = scratch;
        result
    }

    /// `compute_stress` runs the elastic law's `compute_stress` first (when
    /// the thermal domain check passes), which rewrites the thermal and
    /// stress blocks.
    fn prepare_stress_overwrite(&mut self, ty: ElementType, ghost: GhostType) {
        if self.check_thermal_domain().is_ok() {
            Material::prepare_stress_overwrite(&mut self.elastic, ty, ghost);
        }
    }

    /// Upstream `computeTangentModuli`: `E_eff·C` at every point,
    /// `previous_dt = Δt`.
    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let dt = ctx.services.time_step();
        self.current_dt = dt;
        let mut e_ef = self.e_inf;
        for k in 0..self.nb_branches() {
            let (_, e_add) = self.branch_factors(k, dt);
            e_ef += e_add;
        }
        self.previous_dt = dt;
        let v = Self::V;
        let nq = self.elastic.base().grad_u.get(ty, ghost)?.len();
        check_tangent(tangent, nq, v)?;
        for t in tangent.as_mut_slice().chunks_exact_mut(v * v) {
            for j in 0..v {
                for i in 0..v {
                    t[i + j * v] = self.c[i][j];
                }
            }
        }
        for x in tangent.as_mut_slice() {
            *x *= e_ef;
        }
        self.force_stiffness_changed = false;
        Ok(())
    }

    /// Upstream `computePotentialEnergy(type)` (not-ghost points).
    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.check_thermal_domain()?;
        let g = GhostType::NotGhost;
        let n = DIM * DIM;
        let nb = self.nb_branches();
        let mut scratch = std::mem::take(&mut self.scratch);
        let grad_u = self.elastic.base().grad_u.get(ty, g)?;
        let sv = self.sigma_v_field()?.get(ty, g)?;
        let ev = self.epsilon_v_field()?.get(ty, g)?;
        let nq = grad_u.len();
        if sv.len() != nq || ev.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "MaterialViscoelasticMaxwell::computePotentialEnergy",
                expected: nq,
                got: sv.len().min(ev.len()),
            });
        }
        let [density] = scratch.sized([(nq, 1)])?;
        for (q, d) in density.as_mut_slice().iter_mut().enumerate() {
            let gu: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[q * n..(q + 1) * n])?;
            *d = self.energy_on_quad(
                gu,
                &sv.as_slice()[q * n * nb..(q + 1) * n * nb],
                &ev.as_slice()[q * n * nb..(q + 1) * n * nb],
            );
        }
        drop((grad_u, sv, ev));
        let result = commit_block(
            &mut self.elastic.base_mut().potential_energy,
            ty,
            g,
            scratch.get_mut(0),
            "MaterialViscoelasticMaxwell::computePotentialEnergy: potential_energy",
        );
        self.scratch = scratch;
        result
    }

    /// Upstream `afterSolveStep(converged)`: `Material::afterSolveStep`
    /// (restore on failure), then the branch update (unless
    /// `forceNotUpdateVariable`) and `updateDissipatedEnergy` of every
    /// not-ghost type.
    fn after_solve_step(&mut self, ctx: &LawContext<'_>, converged: bool) -> Result<()> {
        if !converged {
            return self.restore_previous_state();
        }
        self.check_thermal_domain()?;
        let types: Vec<ElementType> = self
            .base()
            .element_filter
            .as_map()
            .iter(GhostType::NotGhost)
            .map(|(t, _)| t)
            .collect();
        for &ty in &types {
            self.update_energies(ctx, ty)?;
        }
        let dt = ctx.services.time_step();
        if self.update_variable_flag {
            self.update_int_variables(dt, GhostType::NotGhost)?;
            if self.elastic.options.maxwell_update == MaxwellUpdate::Recurrence {
                self.update_int_variables(dt, GhostType::Ghost)?;
            }
        }
        for ty in types {
            self.update_dissipated_energy(ctx, ty)?;
        }
        Ok(())
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

    /// State-free variant: compares `previous_dt` with the time step of the
    /// last evaluation (see [`Material::has_stiffness_matrix_changed_in`]).
    fn has_stiffness_matrix_changed(&self) -> bool {
        self.force_stiffness_changed || self.previous_dt != self.current_dt
    }

    /// Upstream `hasStiffnessMatrixChanged`: `previous_dt ≠ Δt` (or a
    /// change of options or, by default, of parameters since the last
    /// tangent, UD-157).
    fn has_stiffness_matrix_changed_in(&self, ctx: &LawContext<'_>) -> bool {
        self.force_stiffness_changed || self.previous_dt != ctx.services.time_step()
    }

    fn mark_stiffness_changed(&mut self) {
        self.elastic.mark_stiffness_changed();
        self.force_stiffness_changed = true;
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    /// Upstream `getEnergy(type)`: `"dissipated"`, `"potential"` (stored
    /// internal, not recomputed), `"work"`; otherwise the parent's.
    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "dissipated" => self.integrate_field(ctx, &self.dissipated_energy),
            "potential" => self.elastic.base().integrate_potential_energy(ctx),
            "work" => self.integrate_field(ctx, &self.mechanical_work),
            _ => self.elastic.energy_of(ctx, energy_id),
        }
    }

    fn energy_of_element(
        &mut self,
        ctx: &LawContext<'_>,
        energy_id: &str,
        element: Element,
    ) -> Result<Real> {
        let base = self.elastic.base();
        match energy_id {
            "dissipated" => integrate_element_density(ctx, base, &self.dissipated_energy, element),
            "potential" => integrate_element_density(ctx, base, &base.potential_energy, element),
            "work" => integrate_element_density(ctx, base, &self.mechanical_work, element),
            _ => Ok(0.0),
        }
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let ty = element.ty();
        let nq = ctx.services.nb_integration_points(ty, GhostType::NotGhost);
        let e = self
            .elastic
            .base()
            .potential_energy
            .get(ty, GhostType::NotGhost)?;
        e.as_slice()
            .get(element.idx() * nq..(element.idx() + 1) * nq)
            .map(<[Real]>::to_vec)
            .ok_or(Error::InvalidIndex {
                what: "law-local element",
                value: i64::from(element.index()),
            })
    }
}

/// Register `viscoelastic_maxwell` (upstream
/// `instantiateMaterial<MaterialViscoelasticMaxwell>("viscoelastic_maxwell")`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "viscoelastic_maxwell",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialViscoelasticMaxwell::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}

/// `field(ty, ghost) <- new` for a block whose values (all of them) the
/// caller computed into `new`: [`InternalField::commit_overwrite`] when the
/// shapes agree, a copy of the flat values when only their count agrees,
/// else [`Error::DimensionMismatch`] with `context` (nothing changed).
fn commit_block(
    field: &mut InternalField<Real>,
    ty: ElementType,
    ghost: GhostType,
    new: &mut Array<Real>,
    context: &'static str,
) -> Result<()> {
    let (len, nbc) = {
        let a = field.values().shared(ty, ghost)?;
        (a.len(), a.nb_component())
    };
    if (len, nbc) == (new.len(), new.nb_component()) {
        return field.commit_overwrite(ty, ghost, new);
    }
    let mut cur = field.get_mut(ty, ghost)?;
    if cur.as_slice().len() != new.as_slice().len() {
        return Err(Error::DimensionMismatch {
            context,
            expected: new.as_slice().len(),
            got: cur.as_slice().len(),
        });
    }
    cur.as_mut_slice().copy_from_slice(new.as_slice());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The corrected compliance is the inverse of the stiffness in every
    /// dimension (2-D plane strain included); upstream's `D` is not in 2-D.
    #[test]
    fn corrected_compliance_inverts_the_stiffness() {
        let spec = |d| LawSpec {
            spatial_dimension: d,
            option: String::new(),
            id: "m:0:viscoelastic_maxwell".into(),
        };
        fn check<const D: usize>(spec: LawSpec) {
            let mut m = MaterialViscoelasticMaxwell::<D>::new(&spec, None).unwrap();
            m.elastic.thermal.nu = if D == 1 { 0. } else { 0.3 };
            m.elastic.thermal.e = 1.;
            m.update().unwrap();
            let v = MaterialViscoelasticMaxwell::<D>::V;
            let mut max_s = 0.0_f64;
            let mut max_d = 0.0_f64;
            for i in 0..v {
                for j in 0..v {
                    let cs: f64 = (0..v).map(|k| m.c[i][k] * m.s[k][j]).sum();
                    let cd: f64 = (0..v).map(|k| m.c[i][k] * m.d[k][j]).sum();
                    let id = if i == j { 1. } else { 0. };
                    max_s = max_s.max((cs - id).abs());
                    max_d = max_d.max((cd - id).abs());
                }
            }
            assert!(max_s < 1e-14, "{D}-D: C·S − I = {max_s}");
            if D == 2 {
                assert!(max_d > 0.1, "2-D: upstream D is not C⁻¹ ({max_d})");
            } else {
                assert!(max_d < 1e-14, "{D}-D: upstream D is C⁻¹ ({max_d})");
            }
        }
        check::<1>(spec(1));
        check::<2>(spec(2));
        check::<3>(spec(3));
    }
}
