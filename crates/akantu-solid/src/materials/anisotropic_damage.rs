// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_anisotropic_damage.hh,
// material_anisotropic_damage.cc, material_anisotropic_damage_tmpl.hh.

//! `anisotropic_damage`: tensorial (second-order) damage for quasi-brittle
//! materials (upstream `MaterialAnisotropicDamage<dim, EquivalentStrain,
//! DamageThreshold>`, parent `MaterialElastic<dim>`), options `mazars`
//! (default) and `mazars-drucker-prager` (extra parameter `k`), all spatial
//! dimensions.
//!
//! Parameters: the elastic ones, `Dc` (0.99), and the threshold function's
//! `a` (2.93e-4), `A` (5e3), `K0` (5e-5) (`DamageThresholdTan`), plus `k`
//! (0) for `mazars-drucker-prager` (all parsable only). Internals: `damage`
//! (`dim²`), `elastic_stress` (`dim²`), `equivalent_strain` (1),
//! `trace_damage` (1, history).
//!
//! # Models (`LawOptions::anisotropic_damage`, UD-031 / UD-058)
//!
//! Upstream's kernel is not a usable constitutive law (UD-031: the intent
//! of the code is not evident, so a named, published model must be
//! selected and checked independently). The first corrected specification
//! (`DesmoratExperimental` below) was rejected: its saturation is not
//! thermodynamically admissible. Hence:
//!
//! * [`AnisotropicDamageModel::Unsupported`] (**default**): `computeStress`
//!   and `computeTangentModuli` return [`Error::Unsupported`] until a
//!   complete specification (saturated-direction evolution, threshold
//!   history after saturation, parameter domains, 1-D/2-D reductions, the
//!   Drucker–Prager option) is approved — the precedent of the
//!   finite-strain J2 law (UD-030);
//! * [`AnisotropicDamageModel::Upstream`]: the reference code (compat);
//! * [`AnisotropicDamageModel::Corrected`]: the UD-058 specification
//!   accepted by D-066 (3); an opt-in that has not been checked
//!   independently yet, and the candidate default once it is (it then
//!   replaces `DesmoratExperimental`);
//! * [`AnisotropicDamageModel::DesmoratExperimental`]: the rejected
//!   proposal, kept as a clearly labelled, unapproved opt-in.
//!
//! ## [`AnisotropicDamageModel::Upstream`] — the reference code
//!
//! Reproduces `material_anisotropic_damage_tmpl.hh` operation by operation,
//! with its uninitialised data **defined** (category A): the diagonal
//! matrix `A_diag` of `tensorPlusOp` has its off-diagonal entries zero
//! (upstream leaves them uninitialised, so the unpatched oracle's `damage`
//! and broken-state tangent depend on stack contents). The defined
//! behaviour is compared bitwise with the goldens of the UD-031 oracle
//! variant (`oracle/patches/variants/ud031-anisotropic-damage-zero-init.patch`,
//! `fixtures/material/anisotropic_damage_mazars/variant_ud031`).
//! The eigen-decompositions run on stack buffers (`eig_fixed`,
//! `eig_vectors_fixed`, `eigh3_vectors_fixed`: no per-point allocation).
//! Everything else is as coded:
//!
//! 1. `σ ← MaterialElastic::computeStressOnQuad` (no thermal refresh);
//!    `ε = sym ∇u`; `ε̂ = √(Σ_i ⟨ε_i⟩₊)` (Mazars, **not** squared;
//!    `+ k·tr ε` for Drucker–Prager), eigenvalues by Eigen's `EigenSolver`
//!    (unsorted);
//! 2. `K(TrD) = a·tan(atan2(TrD, a) − atan2(K0, a))` with the **current**
//!    `trace_damage` (never written: it stays 0, and `K(0) = −K0 < 0`, so
//!    the criterion is met at every evaluation); if `ε̂ − K > 0`:
//!    `TrD' = a·A·(atan2(ε̂, a) − atan2(K0, a))`,
//!    `D' = D + (TrD' − TrD)/ε̂² · ⟨ε⟩₊` (`⟨ε⟩₊` from `EigenSolver` with
//!    descending sort);
//! 3. if `tr D' > Dc`: in tension (`tr ε > 0`) the **material-wide** `E`,
//!    `ν` are changed so that `κ ← (1 − Dc)κ` at fixed `λ`, the parameters
//!    updated and steps 1–2 repeated; the "deviatoric" branch computes a
//!    clipped tensor and discards it (not reproduced: no effect);
//! 4. the new trace is written into the **previous** slot of
//!    `trace_damage`, `D ← D'` (no history, so repeated evaluations
//!    accumulate);
//! 5. `σ = √(1−D)·σ_el·√(1−D) − ((1−D):σ_el)/(dim − TrD_prev)·(1−D) +
//!    (1/dim)·((1 − TrD_prev)·Σ⟨σ_el,i⟩₊ − Σ⟨σ_el,i⟩₋)·I` with `σ_el` the
//!    internal `elastic_stress`, which nothing writes: `σ ≡ ±0`.
//!
//! ## [`AnisotropicDamageModel::Corrected`] — the UD-058 specification (opt-in)
//!
//! The stress, the threshold `κ`, `κ⁻¹`, the 1-D/2-D embedding and the
//! internals are those of the Desmorat model below; the evolution differs
//! (D-066 (3)):
//!
//! ```text
//! ε̂ = max(0, √(ε⁺:ε⁺) + k tr ε),   P = ε⁺·ε⁺ / tr(ε⁺·ε⁺)   (tr P = 1)
//! if ε̂ > κ(T_n) and tr(ε⁺·ε⁺) > 0:
//!     ΔT = κ⁻¹(ε̂) − T_n,   θ = min(1, 1/(ΔT·λ_max(M^{-1/2} P M^{-1/2}))),  M = Dc·I − D_n
//!     D = D_n + θΔT·P,   T = T_n + θΔT
//! else D = D_n, T = T_n
//! ```
//!
//! with the saturated case (`M` singular on a direction `v`): `θ = 0` if
//! `vᵀPv > 0`, else `θ` from the range of `M` with the increment `P`
//! projected onto that range and renormalised
//! (`saturation_scale`, whose documentation gives the floating-point
//! reading of these conditions).
//! Hence `D_{n+1} − D_n ⪰ 0` (irreversibility in the Loewner order: the
//! counterexample that rejected the earlier proposal cannot occur),
//! `λ_max(D) ≤ Dc` (to rounding)
//! and `tr D = T` (the threshold is a function of the accumulated damage;
//! reloading to the same strain gives no further damage). **Modelling
//! limitation** (specification property 7): once a principal damage has
//! reached `Dc`, any loading that opens that direction (`vᵀPv > 0`) stops
//! the damage evolution in **all** directions while it lasts; loading
//! orthogonal to it keeps damaging. Compressive paths (`ε̂ = 0`) give no
//! evolution and never reach an eigen-solve of a NaN tensor (contrast
//! UD-763 and UD-814 of the upstream mode). Parameter domains (refused at
//! `initMaterial`, at parameter updates and at every evaluation): `E > 0`,
//! `−1 < ν < ½`, `0 < Dc < 1`, `a > 0`, `A > 0`, `K0 > 0`, `k ≥ 0`; plane
//! stress and `finite_deformation` refused; 1-D uses `ν = 0` (upstream's
//! `initMaterial`). The tangent is the frozen-damage (secant) `∂σ/∂ε|_D`
//! ([`MaterialAnisotropicDamage::corrected_tangent`]); the potential energy
//! `½σ:ε` is the frozen-`D` free energy `ψ` below (degree-2 homogeneity).
//! The dissipation `ψ(ε, D_n) − ψ(ε, D_{n+1}) ≥ 0` of every damaging step
//! (specification property 5) is checked numerically on random
//! non-proportional paths (0 negative steps were measured out of
//! 23,472; no general proof for the deviatoric term). There is no oracle
//! for this model; the evidence is the independent checks of
//! `tests/test_material_anisotropic_damage.rs` (`ud058_*`).
//!
//! ## [`AnisotropicDamageModel::DesmoratExperimental`] — REJECTED specification (opt-in only)
//!
//! **Status: specification derived for the port and rejected at
//! saturation by an independent check (confirmed by a second one).**
//! It is not the default (the default is
//! [`AnisotropicDamageModel::Unsupported`]) and remains an unapproved,
//! explicitly selected experimental opt-in; UD-031/UD-058 stay "needs
//! spec" and the parity status is at most "Implemented, unverified". No
//! approval of its saturated evolution, of its Drucker–Prager extension or
//! of its dimensional reductions is implied. The model is the strain-based anisotropic
//! damage model of Desmorat, Gatuingt & Ragueneau (*Nonlocal anisotropic
//! damage model and related computational aspects for quasi-brittle
//! materials*, Eng. Fract. Mech. 74 (2007) 1539–1560), whose stress
//! formula upstream's `damageStress` comment quotes and whose threshold is
//! upstream's `DamageThresholdTan`. All tensors are 3 × 3: 2-D is plane
//! strain (`ε33 = 0`), 1-D uniaxial (`ν = 0`), and the stress returned is
//! the `dim × dim` block. With `K = λ + 2μ/3`, `G = μ` (3-D `λ`) and the
//! committed state `(D_n, T_n)` (`damage`, `trace_damage`, both with
//! history):
//!
//! ```text
//! ε⁺ = Σ ⟨ε_i⟩₊ n_i⊗n_i           ε̂ = √(ε⁺:ε⁺)  (+ k·tr ε, Drucker–Prager)
//! κ(T)   = a·tan(T/(a·A) + atan(K0/a))     κ⁻¹(ε̂) = a·A·(atan(ε̂/a) − atan(K0/a))
//! if ε̂ > κ(T_n):  T = κ⁻¹(ε̂);  P = ε⁺·ε⁺;  D = cap(D_n + (T − T_n)/tr P · P)
//!                 (D = D_n if tr P = 0)
//! else:           T = T_n, D = D_n
//! cap: eigenvalues of D clipped to Dc (principal damages ≤ Dc)
//! σ = 2G[(1−D)^½ ε (1−D)^½ − ((1−D):ε)/(3 − tr D)·(1−D)]
//!     + K[max(1 − tr D, 1 − Dc)·⟨tr ε⟩₊ + ⟨tr ε⟩₋]·I
//! elastic_stress = 2Gε + λ tr ε I (effective stress), equivalent_strain = ε̂
//! ```
//!
//! Corrections with respect to upstream: the evolution reads the committed
//! state (no accumulation over residual evaluations, rollback on failure),
//! `κ` and `κ⁻¹` are mutual inverses (`κ(0) = K0`), the equivalent strain
//! is the norm `√(ε⁺:ε⁺)` with the consistent rate direction `⟨ε⟩₊²`
//! (`tr ΔD = ΔT`), the compression term recovers the elastic stress
//! (`D = 0` gives `2Gε + λ tr ε I` in every stress state), the hydrostatic
//! factor uses `⟨tr ε⟩` (not eigenvalue sums), the 2-D/1-D reductions use
//! the 3-D constants (`3 − tr D`, `1/3`), and the broken state is
//! regularised locally (principal damages capped at `Dc`, tensile bulk
//! factor at least `1 − Dc` — upstream's intent `κ_b = (1 − Dc)κ`, without
//! the material-wide parameter change). At fixed `D` the stress is
//! positively homogeneous of degree 1 in `ε` (the energy below of degree
//! 2), and it derives from the frozen-`D` free energy
//! (with `A = 1 − D`, `H = √A`, `t = tr A`, `v = tr ε`,
//! `b = max(1 − tr D, 1 − Dc)`)
//!
//! ```text
//! ψ(ε, D) = G [tr(ε H ε H) − (A:ε)²/t] + K/2 [b ⟨v⟩₊² + ⟨v⟩₋²],
//! ```
//!
//! so by Euler's theorem `½σ:ε = ψ` (the parent's potential energy) is the
//! stored energy at frozen `D` in the isothermal domain — this does not
//! make the evolution admissible (see the saturation defect above). Plane stress and `finite_deformation` are rejected by
//! `initMaterial` in this mode. The tangent is the parent's elastic one
//! in both modes (upstream provides no damaged tangent).

use std::sync::Arc;

use akantu_core::enums::MatrixType;
use akantu_core::glibc_libm;
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_linalg::dense::{SortOrder, eig_fixed, eig_vectors_fixed, eigh3_vectors_fixed};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_parser::ParameterRegistry;
use akantu_parser::param::{ParamDescriptor, ParameterAccess};

use super::elastic::MaterialElastic;
use super::sls_deviatoric::set_previous_block;
use crate::material::{
    DerivedMaterialParams, LawOptions, Material, MaterialBase, MaterialFactory, check_tangent,
};
use crate::material_tensor::{grad_u_to_epsilon, tangent_stiffness_voigt_size};

/// The anisotropic damage model (UD-031 / UD-058).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AnisotropicDamageModel {
    /// Default: no thermodynamically admissible specification has been
    /// approved (the saturation of the proposed model is
    /// not monotone), so `computeStress` and `computeTangentModuli` return
    /// [`Error::Unsupported`] — the precedent of the finite-strain J2 law
    /// (UD-030).
    #[default]
    Unsupported,
    /// **Experimental, unapproved** opt-in: the Desmorat-type model of the
    /// module docs. Its unsaturated evolution is admissible, but its
    /// saturation (spectral clipping of the total trial tensor) violates
    /// tensor irreversibility and non-negative dissipation on reachable
    /// non-proportional paths (regression
    /// `mv01_saturation_counterexample_*`). Not a corrected default.
    DesmoratExperimental,
    /// The reference code, uninitialised entries defined as zero.
    Upstream,
    /// **Opt-in, not yet checked independently**: the corrected model of
    /// the UD-058 specification (accepted by D-066 (3)): the
    /// Desmorat-type law whose saturation **scales the damage increment**
    /// (`θ ∈ [0, 1]`, closed form) instead of clipping the total tensor, so
    /// that `D` grows in the Loewner order at every step, `λ_max(D) ≤ Dc`
    /// and `tr D = T`. Not the default until an independent check approves it.
    Corrected,
}

/// Equivalent strain of the allocator option.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum EquivalentStrain {
    /// `mazars` (default).
    Mazars,
    /// `mazars-drucker-prager`: Mazars `+ k·tr ε`.
    MazarsDruckerPrager,
}

impl EquivalentStrain {
    /// Upstream allocator option (`""`/`mazars`, `mazars-drucker-prager`).
    ///
    /// # Errors
    /// Unknown option (upstream `AKANTU_EXCEPTION`).
    pub fn from_option(option: &str, id: &str) -> Result<Self> {
        match option {
            "" | "mazars" => Ok(Self::Mazars),
            "mazars-drucker-prager" => Ok(Self::MazarsDruckerPrager),
            other => Err(Error::invalid(format!(
                "The option {other} is not valid for the material {id}"
            ))),
        }
    }
}

/// glibc `tan` where ported (`|x| ≤ 25`), else the platform's.
fn tan(x: Real) -> Real {
    glibc_libm::tan_checked(x).unwrap_or_else(|| x.tan())
}

/// Anisotropic damage law (upstream `MaterialAnisotropicDamage`).
#[derive(Clone, Debug)]
pub struct MaterialAnisotropicDamage<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// Equivalent strain (allocator option).
    pub equivalent: EquivalentStrain,
    /// `Dc` (critical damage).
    pub dc: Real,
    /// `a` of `DamageThresholdTan`.
    pub a: Real,
    /// `A` of `DamageThresholdTan`.
    pub big_a: Real,
    /// `K0` of `DamageThresholdTan`.
    pub k0: Real,
    /// `k` of the Drucker–Prager equivalent strain.
    pub k: Real,
    /// `damage` (`dim²`).
    pub damage: InternalField<Real>,
    /// `elastic_stress` (`dim²`).
    pub elastic_stress: InternalField<Real>,
    /// `equivalent_strain` (1).
    pub equivalent_strain: InternalField<Real>,
    /// `trace_damage` (1, history).
    pub trace_damage: InternalField<Real>,
}

impl<const DIM: usize> MaterialAnisotropicDamage<DIM> {
    /// New law (upstream constructor; `spec.option` selects the equivalent
    /// strain).
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let equivalent = EquivalentStrain::from_option(&spec.option, &spec.id)?;
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = "anisotropic_damage".into();
        let mut trace_damage = InternalField::new("trace_damage", 1, 0.0)?;
        trace_damage.initialize_history();
        Ok(Self {
            elastic,
            equivalent,
            dc: 0.99,
            a: 2.93e-4,
            big_a: 5e3,
            k0: 5e-5,
            k: 0.,
            damage: InternalField::new("damage", DIM * DIM, 0.0)?,
            elastic_stress: InternalField::new("elastic_stress", DIM * DIM, 0.0)?,
            equivalent_strain: InternalField::new("equivalent_strain", 1, 0.0)?,
            trace_damage,
        })
    }

    fn model(&self) -> AnisotropicDamageModel {
        self.elastic.options.anisotropic_damage
    }

    fn update(&mut self) -> Result<()> {
        ParameterRegistry::update_internal_parameters(&mut self.elastic)?;
        if self.model() == AnisotropicDamageModel::Corrected && self.elastic.base().is_init() {
            self.check_corrected_domain()?;
        }
        Ok(())
    }

    // ------------------------------------------------------------------
    // Desmorat model
    // ------------------------------------------------------------------

    /// `κ(T) = a·tan(T/(aA) + atan(K0/a))`.
    #[must_use]
    pub fn threshold(&self, t: Real) -> Real {
        self.a * (t / (self.a * self.big_a) + (self.k0 / self.a).atan()).tan()
    }

    /// `κ⁻¹(ε̂) = aA·(atan(ε̂/a) − atan(K0/a))`.
    #[must_use]
    pub fn threshold_inverse(&self, eps_hat: Real) -> Real {
        self.a * self.big_a * ((eps_hat / self.a).atan() - (self.k0 / self.a).atan())
    }

    /// One Desmorat point: from the committed `(D_n, T_n)` and the strain,
    /// returns `(σ, D, T, ε̂, σ_el)` (3 × 3 for `D`; `dim × dim` blocks
    /// for the stresses). `d_n` is the committed damage (`DIM × DIM`).
    #[allow(clippy::type_complexity)] // one point's outputs
    pub fn desmorat_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        d_n: &Matrix<DIM, DIM>,
        t_n: Real,
    ) -> Result<(
        Matrix<DIM, DIM>,
        Matrix<DIM, DIM>,
        Real,
        Real,
        Matrix<DIM, DIM>,
    )> {
        let eps_d = grad_u_to_epsilon(grad_u);
        let eps = embed(&eps_d);
        let d_old = embed(d_n);
        let (vals, vecs) = sym_eigen(&eps)?;
        let pos: [Real; 3] = std::array::from_fn(|i| vals[i].max(0.));
        let mut eps_hat = pos.iter().map(|x| x * x).sum::<Real>().sqrt();
        let tr_eps = eps[0][0] + eps[1][1] + eps[2][2];
        if self.equivalent == EquivalentStrain::MazarsDruckerPrager {
            eps_hat += self.k * tr_eps;
        }
        let (d, t) = if eps_hat > self.threshold(t_n) {
            let t = self.threshold_inverse(eps_hat);
            let sq: [Real; 3] = std::array::from_fn(|i| pos[i] * pos[i]);
            let p = spectral(&vecs, &sq);
            let tr_p: Real = sq.iter().sum();
            let d = if tr_p > 0. {
                let dl = (t - t_n) / tr_p;
                let trial: [[Real; 3]; 3] =
                    std::array::from_fn(|j| std::array::from_fn(|i| d_old[j][i] + dl * p[j][i]));
                let (dv, dw) = sym_eigen(&trial)?;
                let capped: [Real; 3] = std::array::from_fn(|i| dv[i].min(self.dc));
                spectral(&dw, &capped)
            } else {
                d_old
            };
            (d, t)
        } else {
            (d_old, t_n)
        };
        let (sigma3, sigma_el3) = self.frozen_damage_stress(&eps, tr_eps, &d)?;
        Ok((block(&sigma3), block(&d), t, eps_hat, block(&sigma_el3)))
    }

    /// The stress at frozen damage shared by the Desmorat-type models
    /// ([`AnisotropicDamageModel::DesmoratExperimental`],
    /// [`AnisotropicDamageModel::Corrected`]): with `A = 1 − D`, `H = √A`,
    /// `t = tr A`, `b = max(1 − tr D, 1 − Dc)`,
    /// `σ = 2G[HεH − (A:ε)/t·A] + K[b⟨tr ε⟩₊ + ⟨tr ε⟩₋]I` and the effective
    /// stress `2Gε + λ tr ε I` (3 × 3, 3-D constants).
    #[allow(clippy::type_complexity)] // (σ, σ_el) of one point
    fn frozen_damage_stress(
        &self,
        eps: &[[Real; 3]; 3],
        tr_eps: Real,
        d: &[[Real; 3]; 3],
    ) -> Result<([[Real; 3]; 3], [[Real; 3]; 3])> {
        let lambda3 = self.lambda_3d();
        let mu = self.elastic.mu();
        let kappa = lambda3 + 2. / 3. * mu;
        // (1 − D)^{1/2}
        let one_d: [[Real; 3]; 3] =
            std::array::from_fn(|j| std::array::from_fn(|i| id(i, j) - d[j][i]));
        let (ov, ow) = sym_eigen(&one_d)?;
        let sqrt_one_d = spectral(&ow, &std::array::from_fn(|i| ov[i].max(0.).sqrt()));
        let inner = mat3_mul(&mat3_mul(&sqrt_one_d, eps), &sqrt_one_d);
        let od_eps: Real = (0..3)
            .flat_map(|j| (0..3).map(move |i| (i, j)))
            .map(|(i, j)| one_d[j][i] * eps[j][i])
            .sum();
        let tr_d = d[0][0] + d[1][1] + d[2][2];
        let hyd_factor = (1. - tr_d).max(1. - self.dc);
        let hyd = kappa * (hyd_factor * tr_eps.max(0.) + tr_eps.min(0.));
        let coef = od_eps / (3. - tr_d);
        let sigma3: [[Real; 3]; 3] = std::array::from_fn(|j| {
            std::array::from_fn(|i| 2. * mu * (inner[j][i] - coef * one_d[j][i]) + hyd * id(i, j))
        });
        let sigma_el3: [[Real; 3]; 3] = std::array::from_fn(|j| {
            std::array::from_fn(|i| 2. * mu * eps[j][i] + lambda3 * tr_eps * id(i, j))
        });
        Ok((sigma3, sigma_el3))
    }

    // ------------------------------------------------------------------
    // Corrected model (UD-058 specification, D-066 (3))
    // ------------------------------------------------------------------

    /// One point of [`AnisotropicDamageModel::Corrected`]: from the
    /// committed `(D_n, T_n)` and `∇u`, returns `(σ, D, T, ε̂, σ_el)` (the
    /// `DIM × DIM` blocks of the 3 × 3 tensors of the embedded kinematics).
    ///
    /// ```text
    /// ε⁺ = Σ⟨ε_i⟩₊ n_i⊗n_i,   q = tr(ε⁺·ε⁺),   ε̂ = max(0, √q + k tr ε)
    /// if ε̂ > κ(T_n), q > 0 and ΔT = κ⁻¹(ε̂) − T_n > 0:
    ///     P = ε⁺·ε⁺/q,  (θ, P′) = saturation_scale(D_n, P, ΔT, Dc)
    ///     D = D_n + θΔT·P′,  T = T_n + θΔT     (P′ = P unless a direction is saturated)
    /// else D = D_n, T = T_n          (no evolution at ε̂ = 0: UD-763)
    /// ```
    ///
    /// # Errors
    /// A failed symmetric eigen-solve (non-finite input).
    #[allow(clippy::type_complexity)] // one point's outputs
    pub fn corrected_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        d_n: &Matrix<DIM, DIM>,
        t_n: Real,
    ) -> Result<(
        Matrix<DIM, DIM>,
        Matrix<DIM, DIM>,
        Real,
        Real,
        Matrix<DIM, DIM>,
    )> {
        let eps_d = grad_u_to_epsilon(grad_u);
        let eps = embed(&eps_d);
        let d_old = embed(d_n);
        let (vals, vecs) = sym_eigen(&eps)?;
        let pos: [Real; 3] = std::array::from_fn(|i| vals[i].max(0.));
        let q = pos[0] * pos[0] + pos[1] * pos[1] + pos[2] * pos[2];
        let tr_eps = eps[0][0] + eps[1][1] + eps[2][2];
        let k = match self.equivalent {
            EquivalentStrain::Mazars => 0.,
            EquivalentStrain::MazarsDruckerPrager => self.k,
        };
        let eps_hat = (q.sqrt() + k * tr_eps).max(0.);
        let mut d = d_old;
        let mut t = t_n;
        if eps_hat > self.threshold(t_n) && q > 0. {
            let delta_t = self.threshold_inverse(eps_hat) - t_n;
            if delta_t > 0. {
                let p = spectral_sym(&vecs, &std::array::from_fn(|i| pos[i] * pos[i] / q));
                let (theta, p) = saturation_scale(&d_old, &p, delta_t, self.dc)?;
                if theta > 0. {
                    let inc = theta * delta_t;
                    d = std::array::from_fn(|j| {
                        std::array::from_fn(|i| d_old[j][i] + inc * p[j][i])
                    });
                    t = t_n + inc;
                }
            }
        }
        let (sigma3, sigma_el3) = self.frozen_damage_stress(&eps, tr_eps, &d)?;
        Ok((block(&sigma3), block(&d), t, eps_hat, block(&sigma_el3)))
    }

    /// The frozen-damage (secant) tangent of
    /// [`AnisotropicDamageModel::Corrected`], `∂σ/∂ε` at fixed `D` (UD-058
    /// specification (domains and options): the Newton operator, a documented approximation of
    /// the consistent tangent, as UD-079 for the scalar damage laws), in
    /// Voigt notation (engineering shear strains) into the column-major
    /// `n × n` slice `out`:
    /// `C_(ij)(kl) = 2G[(H_ik H_jl + H_il H_jk)/2 − A_ij A_kl/t] + K c δ_ij δ_kl`
    /// with `c = b` in volumetric tension (`tr ε > 0`), else 1.
    ///
    /// # Errors
    /// A failed symmetric eigen-solve of `1 − D`.
    pub fn corrected_tangent(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        d: &Matrix<DIM, DIM>,
        out: &mut [Real],
    ) -> Result<()> {
        let pairs = voigt_pairs::<DIM>();
        let n = pairs.len();
        if out.len() != n * n {
            return Err(Error::DimensionMismatch {
                context: "anisotropic_damage: tangent",
                expected: n * n,
                got: out.len(),
            });
        }
        let eps = grad_u_to_epsilon(grad_u);
        let tr_eps: Real = (0..DIM).map(|i| eps[i][i]).sum();
        let d3 = embed(d);
        let a: [[Real; 3]; 3] =
            std::array::from_fn(|j| std::array::from_fn(|i| id(i, j) - d3[j][i]));
        let (av, aw) = sym_eigen(&a)?;
        let h = spectral_sym(&aw, &std::array::from_fn(|i| av[i].max(0.).sqrt()));
        let t = a[0][0] + a[1][1] + a[2][2];
        let tr_d = d3[0][0] + d3[1][1] + d3[2][2];
        let b = (1. - tr_d).max(1. - self.dc);
        let c = if tr_eps > 0. { b } else { 1. };
        let lambda3 = self.lambda_3d();
        let mu = self.elastic.mu();
        let kappa = lambda3 + 2. / 3. * mu;
        for (jj, &(k, l)) in pairs.iter().enumerate() {
            for (ii, &(i, j)) in pairs.iter().enumerate().take(jj + 1) {
                let dev = 0.5 * (h[k][i] * h[l][j] + h[l][i] * h[k][j]) - a[j][i] * a[l][k] / t;
                let vol = if i == j && k == l { kappa * c } else { 0. };
                let v = 2. * mu * dev + vol;
                out[ii + jj * n] = v;
                out[jj + ii * n] = v;
            }
        }
        Ok(())
    }

    /// UD-058 specification (domains and options) parameter domains of
    /// [`AnisotropicDamageModel::Corrected`]: `E > 0`, `−1 < ν < ½`,
    /// `0 < Dc < 1`, `a > 0`, `A > 0`, `K0 > 0`, `k ≥ 0`; plane stress and
    /// `finite_deformation` refused (no condensation / finite-strain form
    /// specified).
    fn check_corrected_domain(&self) -> Result<()> {
        let (e, nu) = (self.elastic.thermal.e, self.elastic.thermal.nu);
        let checks: [(&str, Real, bool); 7] = [
            ("E", e, e > 0.),
            ("nu", nu, nu > -1. && nu < 0.5),
            ("Dc", self.dc, self.dc > 0. && self.dc < 1.),
            ("a", self.a, self.a > 0.),
            ("A", self.big_a, self.big_a > 0.),
            ("K0", self.k0, self.k0 > 0.),
            ("k", self.k, self.k >= 0.),
        ];
        for (name, v, ok) in checks {
            if !(ok && v.is_finite()) {
                return Err(Error::invalid(format!(
                    "anisotropic_damage (corrected model, UD-058): {name} = {v} is outside the \
                     domain E > 0, -1 < nu < 1/2, 0 < Dc < 1, a > 0, A > 0, K0 > 0, k >= 0"
                )));
            }
        }
        if DIM == 2 && self.elastic.plane_stress {
            return Err(Error::invalid(
                "anisotropic_damage (corrected model, UD-058): plane stress is not supported \
                 (plane strain only)",
            ));
        }
        if self.base().finite_deformation {
            return Err(Error::invalid(
                "anisotropic_damage (corrected model, UD-058): finite deformation is not supported",
            ));
        }
        Ok(())
    }

    /// 3-D first Lamé coefficient `νE/((1 + ν)(1 − 2ν))` (the parent's `λ`
    /// unless plane stress, which this model rejects).
    fn lambda_3d(&self) -> Real {
        let (e, nu) = (self.elastic.thermal.e, self.elastic.thermal.nu);
        nu * e / ((1. + nu) * (1. - 2. * nu))
    }

    // ------------------------------------------------------------------
    // Upstream model
    // ------------------------------------------------------------------

    /// `K(x) = a·tan(atan2(x, a) − atan2(K0, a))` (upstream).
    fn upstream_k(&self, x: Real) -> Real {
        self.a * tan(x.atan2(self.a) - self.k0.atan2(self.a))
    }

    /// `K_inv(x) = a·A·(atan2(x, a) − atan2(K0, a))` (upstream).
    fn upstream_k_inv(&self, x: Real) -> Real {
        self.a * self.big_a * (x.atan2(self.a) - self.k0.atan2(self.a))
    }

    /// Upstream equivalent strain functor.
    fn upstream_eps_hat(&self, epsilon: &Matrix<DIM, DIM>) -> Real {
        let (plus, _) = tensor_plus_trace(epsilon);
        let mut e = plus.sqrt();
        if self.equivalent == EquivalentStrain::MazarsDruckerPrager {
            e += self.k * epsilon.trace();
        }
        e
    }

    /// Upstream `computeDamage` lambda: elastic stress into `sigma`, then
    /// `(ε, ε̂, Some((TrD', D')))` when the criterion is met.
    #[allow(clippy::type_complexity)] // the lambda's captured outputs
    fn upstream_compute_damage(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        sigma_th: Real,
        sigma: &mut Matrix<DIM, DIM>,
        d: &Matrix<DIM, DIM>,
        tr_d: Real,
    ) -> (Matrix<DIM, DIM>, Real, Option<(Real, Matrix<DIM, DIM>)>) {
        self.elastic
            .constants()
            .stress_on_quad(grad_u, sigma_th, sigma);
        let epsilon = grad_u_to_epsilon(grad_u);
        let eps_hat = self.upstream_eps_hat(&epsilon);
        let k_trd = self.upstream_k(tr_d);
        let f = eps_hat - k_trd;
        if f > 0. {
            let tr_d_tmp = self.upstream_k_inv(eps_hat);
            let eps_plus = tensor_plus_op(&epsilon, |x| x);
            let delta_lambda = (tr_d_tmp - tr_d) / (eps_hat * eps_hat);
            let d_tmp: Matrix<DIM, DIM> = std::array::from_fn(|j| {
                std::array::from_fn(|i| d[j][i] + delta_lambda * eps_plus[j][i])
            });
            (epsilon, eps_hat, Some((tr_d_tmp, d_tmp)))
        } else {
            (epsilon, eps_hat, None)
        }
    }

    /// Upstream `damageStress`.
    fn upstream_damage_stress(
        sigma_el: &Matrix<DIM, DIM>,
        d: &Matrix<DIM, DIM>,
        tr_d: Real,
    ) -> Matrix<DIM, DIM> {
        let one_d: Matrix<DIM, DIM> =
            std::array::from_fn(|j| std::array::from_fn(|i| id(i, j) - d[j][i]));
        let sqrt_one_d = tensor_plus_op(&one_d, Real::sqrt);
        Self::upstream_damage_stress_with(&sqrt_one_d, &one_d, sigma_el, tr_d)
    }

    /// The rest of upstream `damageStress` once `√(1 − D)` (the
    /// `tensorSqrt` of `one_d = 1 − D`) is known. Split out so that a test
    /// can show that the value of `√(1 − D)` after a failed eigen-solve
    /// (UD-814: uninitialised upstream, NaN in the port, zero in the oracle
    /// variant) never reaches a recorded quantity
    /// (`tests::ud814_failed_eigensolve_is_masked_in_the_stress`).
    fn upstream_damage_stress_with(
        sqrt_one_d: &Matrix<DIM, DIM>,
        one_d: &Matrix<DIM, DIM>,
        sigma_el: &Matrix<DIM, DIM>,
        tr_d: Real,
    ) -> Matrix<DIM, DIM> {
        let (tr_plus, tr_minus) = tensor_plus_trace(sigma_el);
        let p = sqrt_one_d.matmul(sigma_el).matmul(sqrt_one_d);
        #[allow(clippy::cast_precision_loss)] // DIM <= 3
        let dim = DIM as Real;
        let c1 = one_d.double_dot(sigma_el) / (dim - tr_d);
        let c2 = 1. / dim * ((1. - tr_d) * tr_plus - tr_minus);
        std::array::from_fn(|j| std::array::from_fn(|i| p[j][i] - c1 * one_d[j][i] + c2 * id(i, j)))
    }
}

/// Identity coefficient.
#[inline(always)]
fn id(i: usize, j: usize) -> Real {
    if i == j { 1.0 } else { 0.0 }
}

/// Upstream `tensorPlusTrace`: sums of the positive and of the negative
/// eigenvalues (`EigenSolver`, unsorted, compile-time size), sequential
/// from 0. A failed solve gives NaN sums (upstream ignores the status).
/// Allocation-free (`eig_fixed`).
fn tensor_plus_trace<const D: usize>(a: &Matrix<D, D>) -> (Real, Real) {
    let values = eig_fixed(a).unwrap_or([Real::NAN; D]);
    let mut plus = 0.;
    let mut minus = 0.;
    for &e in &values {
        // std::max(eig, 0.) / std::min(eig, 0.)
        plus += if e < 0. { 0. } else { e };
        minus += if 0. < e { 0. } else { e };
    }
    (plus, minus)
}

/// Upstream `tensorPlusOp(A, oper)`: `V·diag(oper(max(λ_i, 0)))·Vᵀ` with
/// `EigenSolver` eigenpairs sorted descending; the off-diagonal entries of
/// the diagonal matrix (uninitialised upstream) are zero. A failed solve
/// gives NaN (upstream ignores the status). Allocation-free
/// (`eig_vectors_fixed`).
fn tensor_plus_op<const D: usize>(a: &Matrix<D, D>, oper: impl Fn(Real) -> Real) -> Matrix<D, D> {
    let Ok((values, v)) = eig_vectors_fixed(a, SortOrder::Descending) else {
        return [[Real::NAN; D]; D];
    };
    let mut diag = [[0.0; D]; D];
    for (i, &e) in values.iter().enumerate() {
        diag[i][i] = oper(if e < 0. { 0. } else { e });
    }
    v.matmul(&diag).matmul(&v.transpose())
}

/// `dim × dim` → 3 × 3 (zeros outside the block).
fn embed<const D: usize>(m: &Matrix<D, D>) -> [[Real; 3]; 3] {
    std::array::from_fn(|j| std::array::from_fn(|i| if i < D && j < D { m[j][i] } else { 0. }))
}

/// 3 × 3 → `dim × dim` top-left block.
fn block<const D: usize>(m: &[[Real; 3]; 3]) -> Matrix<D, D> {
    std::array::from_fn(|j| std::array::from_fn(|i| m[j][i]))
}

/// Symmetric 3 × 3 eigen-decomposition (eigenvalues, eigenvector columns),
/// allocation-free (`eigh3_vectors_fixed`).
fn sym_eigen(m: &[[Real; 3]; 3]) -> Result<([Real; 3], [[Real; 3]; 3])> {
    eigh3_vectors_fixed(m, SortOrder::Natural)
}

/// `V·diag(f)·Vᵀ`.
fn spectral(v: &[[Real; 3]; 3], f: &[Real; 3]) -> [[Real; 3]; 3] {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..3).map(|k| v[k][r] * f[k] * v[k][c]).sum()))
}

/// 3 × 3 product.
fn mat3_mul(a: &[[Real; 3]; 3], b: &[[Real; 3]; 3]) -> [[Real; 3]; 3] {
    std::array::from_fn(|c| std::array::from_fn(|r| (0..3).map(|k| a[k][r] * b[c][k]).sum()))
}

/// `V·diag(f)·Vᵀ`, exactly symmetric (lower triangle computed, mirrored):
/// the damage tensor of [`AnisotropicDamageModel::Corrected`] stays
/// symmetric bit for bit.
fn spectral_sym(v: &[[Real; 3]; 3], f: &[Real; 3]) -> [[Real; 3]; 3] {
    let mut m = [[0.0; 3]; 3];
    for c in 0..3 {
        for r in c..3 {
            let x = v[0][r] * f[0] * v[0][c] + v[1][r] * f[1] * v[1][c] + v[2][r] * f[2] * v[2][c];
            m[c][r] = x;
            m[r][c] = x;
        }
    }
    m
}

/// Index pairs `(i, j)` of the Voigt order of dimension `D` (`xx`; `xx, yy,
/// xy`; `xx, yy, zz, yz, xz, xy`: `akantu_core::voigt`).
fn voigt_pairs<const D: usize>() -> &'static [(usize, usize)] {
    match D {
        1 => &[(0, 0)],
        2 => &[(0, 0), (1, 1), (0, 1)],
        _ => &[(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)],
    }
}

/// An eigenvalue of `M = Dc·I − D_n` at or below this value marks a
/// saturated direction (`λ(D_n) = Dc` to rounding), and a projection
/// `vᵀPv` (with `tr P = 1`) at or below it is "zero": the floating-point
/// reading of the exact conditions `Mv = 0` and `vᵀPv > 0` of the UD-058
/// specification. Without it, a direction saturated by
/// the previous step (its `M` eigenvalue is a rounding residue of either
/// sign) would give a `θ` decided by two rounding residues. `64ε ≈ 1.4e-14`
/// covers the error of the 3 × 3 symmetric eigen-solve of quantities of
/// order 1 (damage is dimensionless, `Dc < 1`, `tr P = 1`).
const SATURATION_RESIDUE: Real = 64. * Real::EPSILON;

/// The increment scale `θ` and direction of
/// [`AnisotropicDamageModel::Corrected`] (UD-058 specification, D-066 (3),
/// deterministic closed form): the largest `θ ∈ [0, 1]`
/// with `λ_max(D_n + θΔT·P′) ≤ Dc`,
///
/// ```text
/// M = Dc·I − D_n = W diag(m) Wᵀ,   P̃ = Wᵀ P W
/// saturated directions S = {i : m_i ≤ r}   (r = SATURATION_RESIDUE)
/// if P̃_ii > r for some i ∈ S:  θ = 0, P′ = P   (an opened saturated direction stops the evolution)
/// else if S = ∅:  P′ = P
/// else:           P′ = Π P Π / τ,  Π = Σ_{i∉S} w_i w_iᵀ,  τ = Σ_{i∉S} P̃_ii
/// C_ij = P̃′_ij / √(m_i m_j)  (i, j ∉ S),   θ = min(1, 1/(ΔT·λ_max(C)))
/// ```
///
/// i.e. `θ* = min(1, 1/(ΔT·λ_max(M^{-1/2} P′ M^{-1/2})))` on the range of
/// `M`. `λ_max(D_n + θΔT·P′)` is non-decreasing in `θ` (`P′ ⪰ 0`), so `θ*`
/// is the specification's maximum. A non-positive or non-finite
/// `λ_max(C)` (only reachable with a `P` that is not positive
/// semi-definite) gives `θ = 0` (no evolution).
///
/// **The projection.** `P̃_ii ≤ r` for every saturated `i` is the
/// floating-point reading of the specification's `vᵀPv = 0` on the
/// saturated eigenvectors, which for `P ⪰ 0` implies `Pv = 0`: the
/// couplings `P̃_ij` (i saturated, j open) vanish in exact arithmetic. In
/// floating point they can be ≈ √(P̃_ii·P̃_jj) ≈ √r ≈ 1.2e-7, and with `P`
/// unchanged a step that saturates `j` left an off-diagonal ≈ θΔT·P̃_ij
/// between two directions at `Dc` (`λ_max(D) − Dc` ≈ 1e-7). The increment is
/// therefore applied in the range of the open directions: the rounding
/// residue along the saturated ones (which must not grow) is removed and
/// `P′` is renormalised to `tr P′ = 1`, so `tr D = T` is kept. A Schur
/// complement on the saturated block instead would make `θ` a ratio of
/// rounding residues (`m_S`, `P̃_SS`) or zero for any non-zero coupling,
/// i.e. stop the open directions because of a rounding residue: rejected.
/// Without a saturated direction `P′ = P` bit for bit.
fn saturation_scale(
    d_n: &[[Real; 3]; 3],
    p: &[[Real; 3]; 3],
    delta_t: Real,
    dc: Real,
) -> Result<(Real, [[Real; 3]; 3])> {
    let m: [[Real; 3]; 3] =
        std::array::from_fn(|j| std::array::from_fn(|i| dc * id(i, j) - d_n[j][i]));
    let (mv, mw) = sym_eigen(&m)?;
    // P̃ = Wᵀ P W (W's columns are the eigenvectors of M)
    let pw = mat3_mul(p, &mw);
    let pt = |a: usize, b: usize| mw[a][0] * pw[b][0] + mw[a][1] * pw[b][1] + mw[a][2] * pw[b][2];
    let saturated: [bool; 3] = std::array::from_fn(|i| mv[i] <= SATURATION_RESIDUE);
    if (0..3).any(|i| saturated[i] && pt(i, i) > SATURATION_RESIDUE) {
        return Ok((0., *p));
    }
    // The increment in the range of the open directions
    let (p_eff, scale) = if saturated.contains(&true) {
        let tau: Real = (0..3).filter(|&i| !saturated[i]).map(|i| pt(i, i)).sum();
        if !(tau > 0. && tau.is_finite()) {
            return Ok((0., *p));
        }
        let mut q = [[0.0; 3]; 3];
        for j in 0..3 {
            for i in j..3 {
                // P′(i, j) = Σ_{a,b ∉ S} w_a[i] P̃_ab w_b[j] / τ (one
                // evaluation per pair: P′ symmetric bit for bit)
                let mut x = 0.;
                for a in (0..3).filter(|&a| !saturated[a]) {
                    for b in (0..3).filter(|&b| !saturated[b]) {
                        x += mw[a][i] * pt(a, b) * mw[b][j];
                    }
                }
                q[j][i] = x / tau;
                q[i][j] = q[j][i];
            }
        }
        (q, tau)
    } else {
        (*p, 1.)
    };
    let mut c = [[0.0; 3]; 3];
    for j in 0..3 {
        for i in j..3 {
            if !saturated[i] && !saturated[j] {
                let x = pt(i, j) / scale / (mv[i].sqrt() * mv[j].sqrt());
                c[j][i] = x;
                c[i][j] = x;
            }
        }
    }
    let (cv, _) = sym_eigen(&c)?;
    let mu_max = cv[0].max(cv[1]).max(cv[2]);
    if !(mu_max > 0. && mu_max.is_finite()) {
        return Ok((0., p_eff));
    }
    Ok(((1. / (delta_t * mu_max)).min(1.), p_eff))
}

impl<const DIM: usize> MaterialAnisotropicDamage<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = &[
        crate::real_param!(Self, "a", ParameterAccess::PARSABLE, "a", a),
        crate::real_param!(Self, "A", ParameterAccess::PARSABLE, "A", big_a),
        crate::real_param!(Self, "K0", ParameterAccess::PARSABLE, "K0", k0),
        crate::real_param!(Self, "Dc", ParameterAccess::PARSABLE, "Critical damage", dc),
    ];
    const PARAMS_DP: &'static [ParamDescriptor<Self>] = &[
        crate::real_param!(Self, "k", ParameterAccess::PARSABLE, "k", k),
        crate::real_param!(Self, "a", ParameterAccess::PARSABLE, "a", a),
        crate::real_param!(Self, "A", ParameterAccess::PARSABLE, "A", big_a),
        crate::real_param!(Self, "K0", ParameterAccess::PARSABLE, "K0", k0),
        crate::real_param!(Self, "Dc", ParameterAccess::PARSABLE, "Critical damage", dc),
    ];
}

impl<const DIM: usize> DerivedMaterialParams for MaterialAnisotropicDamage<DIM> {
    type Parent = MaterialElastic<DIM>;
    fn parent(&self) -> &Self::Parent {
        &self.elastic
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.elastic
    }
    /// Upstream registration order: the equivalent-strain functor (`k`),
    /// the threshold functor (`a`, `A`, `K0`) — both members constructed
    /// before the body — then `Dc`.
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        match self.equivalent {
            EquivalentStrain::Mazars => Self::PARAMS,
            EquivalentStrain::MazarsDruckerPrager => Self::PARAMS_DP,
        }
    }
    fn update_derived(&mut self) -> Result<()> {
        self.update()
    }
    fn derived_id(&self) -> &str {
        &self.elastic.base().id
    }
}

crate::impl_derived_material_registry!(MaterialAnisotropicDamage<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialAnisotropicDamage<DIM> {
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
        f("damage", InternalFieldRef::Real(&self.damage));
        f(
            "elastic_stress",
            InternalFieldRef::Real(&self.elastic_stress),
        );
        f(
            "equivalent_strain",
            InternalFieldRef::Real(&self.equivalent_strain),
        );
        f("trace_damage", InternalFieldRef::Real(&self.trace_damage));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        f("damage", InternalFieldMut::Real(&mut self.damage))?;
        f(
            "elastic_stress",
            InternalFieldMut::Real(&mut self.elastic_stress),
        )?;
        f(
            "equivalent_strain",
            InternalFieldMut::Real(&mut self.equivalent_strain),
        )?;
        f(
            "trace_damage",
            InternalFieldMut::Real(&mut self.trace_damage),
        )
    }
}

impl<const DIM: usize> MaterialAnisotropicDamage<DIM> {
    fn compute_stress_upstream(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        let sigma_th = self.elastic.sigma_th.get(ty, ghost)?.clone();
        let sigma_el = self.elastic_stress.get(ty, ghost)?.clone();
        let mut damage = self.damage.get(ty, ghost)?.clone();
        let tr_d_cur = self.trace_damage.get(ty, ghost)?.clone();
        let mut tr_d_prev = self.trace_damage.previous(ty, ghost)?.clone();
        let mut eps_hat_out = self.equivalent_strain.get(ty, ghost)?.clone();
        let mut sigma_out = vec![0.0; nq * n];
        for q in 0..nq {
            let r = q * n..(q + 1) * n;
            let g: Matrix<DIM, DIM> = *as_matrix(&grad_u.as_slice()[r.clone()])?;
            let d: Matrix<DIM, DIM> = *as_matrix(&damage.as_slice()[r.clone()])?;
            let tr_d = tr_d_cur.as_slice()[q];
            let th = sigma_th.as_slice()[q];
            let mut sigma = [[0.0; DIM]; DIM];
            let (epsilon, mut eps_hat, mut upd) =
                self.upstream_compute_damage(&g, th, &mut sigma, &d, tr_d);
            let new_trd_prev;
            let d_final;
            if let Some((mut tr_tmp, mut d_tmp)) = upd.take() {
                if d_tmp.trace() > self.dc && epsilon.trace() > 0. {
                    // tensile broken case: material-wide E, ν with
                    // κ_b = (1 − Dc)κ at fixed λ
                    let kpa = (1. - self.dc) * self.elastic.kappa();
                    let lambda = self.elastic.lambda();
                    self.elastic.thermal.e = 9. * kpa * (kpa - lambda) / (3. * kpa - lambda);
                    self.elastic.thermal.nu = lambda / (3. * kpa - lambda);
                    self.update()?;
                    let (_, e2, u2) = self.upstream_compute_damage(&g, th, &mut sigma, &d, tr_d);
                    eps_hat = e2;
                    if let Some((t2, d2)) = u2 {
                        tr_tmp = t2;
                        d_tmp = d2;
                    }
                }
                // (deviatoric broken case: result discarded upstream)
                new_trd_prev = tr_tmp;
                d_final = d_tmp;
            } else {
                new_trd_prev = tr_d;
                d_final = d;
            }
            let se: &Matrix<DIM, DIM> = as_matrix(&sigma_el.as_slice()[r.clone()])?;
            let s = Self::upstream_damage_stress(se, &d_final, new_trd_prev);
            sigma_out[r.clone()].copy_from_slice(s.as_flattened());
            *as_matrix_mut::<DIM, DIM>(&mut damage.as_mut_slice()[r])? = d_final;
            tr_d_prev.as_mut_slice()[q] = new_trd_prev;
            eps_hat_out.as_mut_slice()[q] = eps_hat;
        }
        self.elastic
            .base_mut()
            .sigma_field_mut()?
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&sigma_out);
        self.damage.get_mut(ty, ghost)?.copy_from(&damage)?;
        self.equivalent_strain
            .get_mut(ty, ghost)?
            .copy_from(&eps_hat_out)?;
        set_previous_block(&mut self.trace_damage, ty, ghost, &tr_d_prev)
    }

    /// The Desmorat-type models: every point from the committed state
    /// (`damage`, `trace_damage` previous values); `corrected` selects the
    /// UD-058 evolution.
    fn compute_stress_desmorat(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
        corrected: bool,
    ) -> Result<()> {
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        let d_prev = self.damage.previous(ty, ghost)?.clone();
        let t_prev = self.trace_damage.previous(ty, ghost)?.clone();
        let mut sigma_out = vec![0.0; nq * n];
        let mut d_out = vec![0.0; nq * n];
        let mut el_out = vec![0.0; nq * n];
        let mut t_out = vec![0.0; nq];
        let mut e_out = vec![0.0; nq];
        for q in 0..nq {
            let r = q * n..(q + 1) * n;
            let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[r.clone()])?;
            let dn: &Matrix<DIM, DIM> = as_matrix(&d_prev.as_slice()[r.clone()])?;
            let (s, d, t, e, el) = if corrected {
                self.corrected_on_quad(g, dn, t_prev.as_slice()[q])?
            } else {
                self.desmorat_on_quad(g, dn, t_prev.as_slice()[q])?
            };
            sigma_out[r.clone()].copy_from_slice(s.as_flattened());
            d_out[r.clone()].copy_from_slice(d.as_flattened());
            el_out[r].copy_from_slice(el.as_flattened());
            t_out[q] = t;
            e_out[q] = e;
        }
        self.elastic
            .base_mut()
            .sigma_field_mut()?
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&sigma_out);
        self.damage
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&d_out);
        self.elastic_stress
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&el_out);
        self.trace_damage
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&t_out);
        self.equivalent_strain
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&e_out);
        Ok(())
    }
}

impl<const DIM: usize> MaterialAnisotropicDamage<DIM> {
    /// [`AnisotropicDamageModel::Corrected`]: the secant tangent of every
    /// point at its current damage (the state of the last stress
    /// evaluation).
    fn compute_tangent_corrected(
        &self,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        let n = DIM * DIM;
        let grad_u = self.elastic.base().grad_u.get(ty, ghost)?.clone();
        let damage = self.damage.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        let voigt = tangent_stiffness_voigt_size(DIM);
        check_tangent(tangent, nq, voigt)?;
        if damage.len() != nq {
            return Err(Error::DimensionMismatch {
                context: "anisotropic_damage: damage",
                expected: nq,
                got: damage.len(),
            });
        }
        for (q, t) in tangent
            .as_mut_slice()
            .chunks_exact_mut(voigt * voigt)
            .enumerate()
        {
            let r = q * n..(q + 1) * n;
            let g: &Matrix<DIM, DIM> = as_matrix(&grad_u.as_slice()[r.clone()])?;
            let d: &Matrix<DIM, DIM> = as_matrix(&damage.as_slice()[r])?;
            self.corrected_tangent(g, d, t)?;
        }
        Ok(())
    }
}

impl<const DIM: usize> Material for MaterialAnisotropicDamage<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// `MaterialElastic::initMaterial` (not overridden upstream). The
    /// Desmorat model gives `damage` a history and rejects plane stress and
    /// finite deformation.
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        if self.model() == AnisotropicDamageModel::Corrected {
            self.check_corrected_domain()?;
            self.damage.initialize_history();
        }
        if self.model() == AnisotropicDamageModel::DesmoratExperimental {
            if DIM == 2 && self.elastic.plane_stress {
                return Err(Error::invalid(
                    "anisotropic_damage (Desmorat model): plane stress is not supported (use plane strain, or LawOptions::anisotropic_damage = Upstream)",
                ));
            }
            if self.base().finite_deformation {
                return Err(Error::invalid(
                    "anisotropic_damage (Desmorat model): finite deformation is not supported",
                ));
            }
            self.damage.initialize_history();
        }
        self.base_mut().init_history()?;
        let blocks = self.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        self.update()?;
        self.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        self.update()
    }

    fn compute_stress(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        match self.model() {
            AnisotropicDamageModel::Unsupported => Err(unsupported_default()),
            AnisotropicDamageModel::Upstream => self.compute_stress_upstream(ty, ghost),
            AnisotropicDamageModel::DesmoratExperimental | AnisotropicDamageModel::Corrected => {
                let corrected = self.model() == AnisotropicDamageModel::Corrected;
                if corrected {
                    // parameters may have been set after initMaterial
                    self.check_corrected_domain()?;
                }
                if !self.damage.has_history() {
                    // options switched after initMaterial
                    self.damage.initialize_history();
                    self.damage.save_current_values()?;
                }
                self.compute_stress_desmorat(ty, ghost, corrected)
            }
        }
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        match self.model() {
            AnisotropicDamageModel::Unsupported => Err(unsupported_default()),
            AnisotropicDamageModel::Corrected => self.compute_tangent_corrected(ty, ghost, tangent),
            AnisotropicDamageModel::Upstream | AnisotropicDamageModel::DesmoratExperimental => {
                self.elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)
            }
        }
    }

    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.elastic.compute_potential_energy(ctx, ty)
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        self.elastic.potential_energy_by_element(ctx, element)
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
        // the corrected model's secant tangent follows the damage
        self.model() == AnisotropicDamageModel::Corrected
            || self.elastic.has_stiffness_matrix_changed()
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
}

/// The error of the default [`AnisotropicDamageModel::Unsupported`].
fn unsupported_default() -> Error {
    Error::Unsupported(
        "anisotropic_damage: the default is refused (UD-031/UD-058); \
         LawOptions::anisotropic_damage = Corrected selects the approved UD-058 \
         specification (opt-in), Upstream reproduces the reference, \
         DesmoratExperimental is an unapproved opt-in"
            .into(),
    )
}

/// Register `anisotropic_damage` (upstream custom allocator: option `mazars`
/// (default) or `mazars-drucker-prager`).
pub fn register(factory: &mut MaterialFactory) -> Result<()> {
    factory.register(
        "anisotropic_damage",
        Arc::new(|spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                Box::new(MaterialAnisotropicDamage::<D>::new(spec, mesh)?) as Box<dyn Material>
            })
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_validation::exact::{exact_eq, is_default_nan};

    /// A default NaN made by an invalid operation at run time, as UD-763
    /// makes it (`δλ·ε⁺ = ±∞·0`): the platform's default NaN
    /// (`0xfff8…` on x86_64 SSE, `0x7ff8…` on aarch64).
    fn runtime_nan() -> Real {
        std::hint::black_box(Real::INFINITY) * std::hint::black_box(0.0)
    }

    /// UD-814: in the upstream mode the damage becomes
    /// NaN once UD-763 divides by `ε̂ = 0`; `tensorSqrt(1 − D)` then hands a
    /// NaN matrix to the eigen-solver, which fails (3-D: `NoConvergence`,
    /// upstream returns uninitialised eigenpairs; 1-D/2-D:
    /// `NumericalIssue`). The port defines the result as a NaN tensor, the
    /// UD-814 oracle variant as zero eigenpairs (a zero tensor), and
    /// upstream's garbage was `1/√3`-like values in a reproducer. This
    /// test shows that **no recorded quantity depends on that value**: the
    /// only consumer of `√(1 − D)` is the first term of `damageStress`
    /// (`√(1−D)·σ_el·√(1−D)`), and every stress entry also subtracts
    /// `((1 − D):σ_el)/(dim − TrD)·(1 − D)_ij`, which is NaN because
    /// `(1 − D)` is NaN (σ_el ≡ 0, UD-031: NaN·0 = NaN), so the entry is NaN
    /// whatever the first term is — for every non-NaN value and for default
    /// NaNs of either sign (D-046). The damage, `trace_damage`, the
    /// equivalent strain and the (elastic) tangent never read the
    /// eigenpairs; the potential energy `½σ:∇u` reads only the stress.
    ///
    /// The one class of values that is **not** masked is a NaN with a
    /// payload: IEEE arithmetic propagates an input NaN, so such a value
    /// (an integer `-1` left on the stack reads as one) would carry its
    /// payload into the recorded stress. That is why the oracle variant
    /// defines the eigenpairs (oracle/patches/variants/
    /// ud814-eig-noconvergence-zero-init.patch) instead of relying on the
    /// masking.
    #[test]
    fn ud814_failed_eigensolve_is_masked_in_the_stress() {
        check::<3>("no convergence");
        check::<2>("non-finite eigenvalue");
        check::<1>("non-finite eigenvalue");
    }

    fn check<const D: usize>(failure: &str) {
        let nan = runtime_nan();
        // the UD-763 state: D' = D + δλ·ε⁺ with δλ = ±∞ and ε⁺ = 0
        let d: Matrix<D, D> = [[nan; D]; D];
        let sigma_el: Matrix<D, D> = [[0.0; D]; D]; // never written (UD-031)
        // TrD' = K_inv(ε̂ = 0) with the default threshold parameters
        let (a, big_a, k0): (Real, Real, Real) = (2.93e-4, 5e3, 5e-5);
        let tr_d = a * big_a * (0.0_f64.atan2(a) - k0.atan2(a));
        let one_d: Matrix<D, D> =
            std::array::from_fn(|j| std::array::from_fn(|i| id(i, j) - d[j][i]));
        let err = eig_vectors_fixed(&one_d, SortOrder::Descending).unwrap_err();
        assert!(err.to_string().contains(failure), "{D}-D: {err}");
        let port = MaterialAnisotropicDamage::<D>::upstream_damage_stress(&sigma_el, &d, tr_d);
        assert!(
            port.as_flattened().iter().all(|x| is_default_nan(*x)),
            "{D}-D: {port:?}"
        );
        let fill = |x: Real| -> Matrix<D, D> { [[x; D]; D] };
        let eye: Matrix<D, D> = std::array::from_fn(|j| std::array::from_fn(|i| id(i, j)));
        let mixed: Matrix<D, D> = std::array::from_fn(|j| {
            std::array::from_fn(|i| {
                [0.3, -1.7e5, 4.2e-200, -0.0, 9.9e307, 1e-310, 2.5, -3.0, 7.0][i + 3 * j]
            })
        });
        // The reproducer eig_nan_repro.cc: stack filled with 0x3f gives eigenvectors
        // 1/√3 (0x1.279a74590331cp-1)
        let garbage = Real::from_bits(0x3fe2_79a7_4590_331c);
        let candidates: [(&str, Matrix<D, D>); 14] = [
            ("port NaN", fill(Real::NAN)),
            ("variant zero", fill(0.0)),
            ("-0", fill(-0.0)),
            ("reproducer garbage", fill(garbage)),
            ("identity", eye),
            ("mixed", mixed),
            ("1", fill(1.0)),
            ("huge", fill(1e300)),
            ("max", fill(Real::MAX)),
            ("subnormal", fill(Real::from_bits(1))),
            ("+inf", fill(Real::INFINITY)),
            ("-inf", fill(Real::NEG_INFINITY)),
            ("runtime NaN", fill(nan)),
            ("negated runtime NaN", fill(-nan)),
        ];
        for (what, sqrt_one_d) in candidates {
            let s = MaterialAnisotropicDamage::<D>::upstream_damage_stress_with(
                &sqrt_one_d,
                &one_d,
                &sigma_el,
                tr_d,
            );
            for (x, y) in port.as_flattened().iter().zip(s.as_flattened()) {
                assert!(
                    exact_eq(*x, *y) && is_default_nan(*y),
                    "{D}-D, √(1 − D) = {what}: {y:?} (bits {:#018x}) vs the port's {x:?}",
                    y.to_bits()
                );
            }
        }
        // not masked: a NaN with a payload propagates into the stress (Rust's
        // NaN semantics allow either the input NaN or a default one; x86_64
        // and aarch64 propagate the input, as the oracle's C++ would)
        let payload = Real::from_bits(0x7ff8_0000_dead_beef);
        let s = MaterialAnisotropicDamage::<D>::upstream_damage_stress_with(
            &fill(std::hint::black_box(payload)),
            &one_d,
            &sigma_el,
            tr_d,
        );
        for y in s.as_flattened() {
            assert!(
                y.is_nan() && (is_default_nan(*y) || y.to_bits() & !(1 << 63) == payload.to_bits()),
                "{D}-D: {y:?}"
            );
        }
        if cfg!(any(target_arch = "x86_64", target_arch = "aarch64")) {
            assert!(
                s.as_flattened().iter().any(|y| !is_default_nan(*y)),
                "{D}-D: the payload of the injected NaN is expected to reach the stress"
            );
        }
    }
}
