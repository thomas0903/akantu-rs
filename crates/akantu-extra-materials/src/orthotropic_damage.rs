// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_orthotropic_damage.hh,
// material_orthotropic_damage_tmpl.hh,
// material_orthotropic_damage_iterative.hh,
// material_orthotropic_damage_iterative.cc,
// material_orthotropic_damage_iterative_inline_impl.hh.

//! Orthotropic (second-order tensor) damage (XMAT-009) and its
//! sequentially linear variant `orthotropic_damage_iterative` (XMAT-010).
//!
//! # `MaterialOrthotropicDamage<dim>` (base, never registered alone)
//!
//! Damage `D` (`dim²`, internal `damage`) stored **diagonal** in its
//! principal frame, with the frame `V` (columns = principal directions in
//! the computation frame, internal `damage_principal_directions`).
//! `O = 1 − D`, `S = √O` (entry-wise on the diagonal), rotated to the
//! computation frame (`V·X·Vᵀ`) when `tr D ≠ 0` (`Math::are_float_equal`
//! with the law's [`MaterialOrthotropicDamageIterative::tolerance`],
//! upstream's global `Math::tolerance`). Cauchy stress (Ragueneau et al.
//! 2008, eq. 7) from the effective stress `σ̃`:
//!
//! `σ = S σ̃ S − ((σ̃ : O)/(dim − tr D))·O + (1/dim)·tr σ̃·(1 − min(η/dim·tr D,
//! max_damage))·I`
//!
//! (the iterative law's form; the base's commented form without the
//! `min` is not reachable). Tangent: the elastic tangent, replaced when
//! `tr D ≠ 0` by the explicit 2-D formula of the reference (Voigt entries
//! in terms of `C`, `S`, `O`), scaled by `1 − D₁₁` in 1-D, **unchanged in
//! 3-D** (UD-582). Energies: `updateEnergies` = the elastic potential
//! energy then `∫σ += ½(σ + σ_prev):(∇u − ∇u_prev)`, `e_diss = ∫σ − ψ`
//! (internals `damage dissipated energy`, `integral of sigma`).
//! Parameters `eta` (2), `max_damage` (0.99999), parsable-modifiable.
//!
//! # `orthotropic_damage_iterative`
//!
//! As `damage_iterative` but tensorial: the equivalent stress of a point is
//! the largest principal value of its damaged stress (elastic kernel with
//! `σ_th = 0`, sorted `eig` with vectors: internal `equiv_stress_dir`) over
//! `Sc`; the maximum tracks its point `q_max` (type, index), skipping points
//! whose damage cannot grow (`dim·max_damage − tr D ≤ tol`, or an increment
//! that would bring a principal damage within `tol` of `max_damage`).
//! `updateDamage()`: if the maximum is `≥ 1`, the damage of `q_max` is
//! rotated into the principal stress frame (`D_s = V_σᵀ·V D Vᵀ·V_σ`),
//! `D_s(0,0) += prescribed_dam`, re-diagonalised (sorted `eig`), principal
//! values `≥ dam_threshold − tol` set to `max_damage`; one point per call,
//! count all-reduced (`_sum`). Parameters `Sc` (random), `prescribed_dam`
//! (0.1), `dam_threshold` (0.8). Upstream writes back through Akantu-4
//! iterator proxies (`Matrix<Real> q_dam = dam_it[q]`), assumed (UD-572).
//!
//! # Upstream defects
//!
//! * **UD-573** (B): per-type reset of the maximum (as `damage_iterative`).
//! * **UD-581** (B): `updateDamage` stores the eigenvectors `W` of `D_s`
//!   (stress-frame components) as computation-frame directions; the
//!   default stores `V_σ·W`
//!   ([`ExtraLawOptions::orthotropic_directions_in_computation_frame`]).
//!   The two agree when `V_σ = ±I`.
//! * **UD-582** (C, preserved): 3-D tangent undamaged; the tangent reads
//!   the not-ghost damage for any ghost type upstream (the port reads the
//!   block's); `getDissipatedEnergy` is declared but not defined upstream —
//!   the port integrates `damage dissipated energy` (`getEnergy
//!   ("dissipated")`, as `MaterialDamage`).
//! * **UD-582** (B, the stiffness flag): the classes do not derive from
//!   `MaterialDamage`, so upstream's `hasStiffnessMatrixChanged` is
//!   `MaterialElastic`'s (`false` once assembled): after a damage update
//!   the solver keeps the undamaged `K`, which changes one-shot linear
//!   solves and any analysis reusing `K`;
//!   [`ExtraLawOptions::orthotropic_stiffness_changed_after_damage`]
//!   (default) always reports a change, the compat mode reports
//!   `MaterialElastic`'s flag.
//! * **UD-870** (B): on more than one rank upstream compares the global
//!   maximum with 1 and then damages the local maximum point of every rank
//!   (`n` points on `n` ranks). By default only the lowest rank whose local
//!   maximum equals the global one damages its point, the serial pattern
//!   ([`ExtraLawOptions::orthotropic_damage_global_maximum_only`]).

use std::sync::Arc;

use akantu_comm::communicator::ReduceOp;
use akantu_comm::phase::Phases;
use akantu_core::enums::MatrixType;
use akantu_core::math::{DEFAULT_TOLERANCE, are_float_equal};
use akantu_core::random::RandomParameter;
use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result, UInt,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawReducer, LawSpec,
    ModelServices,
};
use akantu_parser::param::{ParamDescriptor, ParameterAccess};
use akantu_solid::Material;
use akantu_solid::material::{LawOptions, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::material_tensor::tangent_stiffness_voigt_size;
use akantu_solid::materials::damage::integrate_field;
use akantu_solid::materials::elastic::MaterialElastic;

use crate::damage_iterative::{IterativeDamage, all_stresses, sum_count_and_apply};
use crate::kernels::{check_lengths, eigenvalues, small_strain_only, sorted_eigen, std_min};
use crate::options::ExtraLawOptions;

const NG: GhostType = GhostType::NotGhost;

macro_rules! orthotropic_params {
    ($($ps:ident)?) => {
        akantu_solid::elastic_parent_params!($($ps)? Self, [elastic], [
            akantu_solid::real_param!(Self, "eta", ParameterAccess::PARSMOD, "Damage sensitivity parameter", eta),
            akantu_solid::real_param!(Self, "max_damage", ParameterAccess::PARSMOD, "maximum damage value", max_damage),
            akantu_solid::random_field_param!(Self, "Sc", ParameterAccess::PARSABLE, "critical stress threshold", sc),
            akantu_solid::real_param!(Self, "prescribed_dam", ParameterAccess::PARSMOD, "increase of damage in every step", prescribed_dam),
            akantu_solid::real_param!(Self, "dam_threshold", ParameterAccess::PARSMOD, "damage threshold at which damage damage will be set to 1", dam_threshold),
        ])
    };
}

/// `V·X·Vᵀ` (upstream `rotateIntoComputationFrame`: `tmp = X·Vᵀ`,
/// `rotated = V·tmp`).
#[must_use]
pub fn rotate_into_computation_frame<const D: usize>(
    x: &Matrix<D, D>,
    v: &Matrix<D, D>,
) -> Matrix<D, D> {
    let tmp = x.matmul(&v.transpose());
    v.matmul(&tmp)
}

/// `Vᵀ·X·V` (upstream `rotateIntoNewFrame`: `tmp = X·V`, `rotated =
/// Vᵀ·tmp`).
#[must_use]
pub fn rotate_into_new_frame<const D: usize>(x: &Matrix<D, D>, v: &Matrix<D, D>) -> Matrix<D, D> {
    let tmp = x.matmul(v);
    v.tr_matmul(&tmp)
}

/// `(O, S)` = `(1 − D, √(1 − D))` in the computation frame (module docs).
#[must_use]
#[allow(clippy::needless_range_loop)] // diagonal entries by index, as upstream
pub fn one_minus_d<const D: usize>(
    damage: &Matrix<D, D>,
    directions: &Matrix<D, D>,
    tolerance: Real,
) -> (Matrix<D, D>, Matrix<D, D>) {
    let mut o = Matrix::<D, D>::identity();
    for (oc, dc) in o.iter_mut().zip(damage) {
        for (x, d) in oc.iter_mut().zip(dc) {
            *x -= *d;
        }
    }
    let mut s = o;
    for i in 0..D {
        s[i][i] = s[i][i].sqrt();
    }
    if are_float_equal(damage.trace(), 0., tolerance) {
        (o, s)
    } else {
        (
            rotate_into_computation_frame(&o, directions),
            rotate_into_computation_frame(&s, directions),
        )
    }
}

/// Upstream `computeDamageAndStressOnQuad` (iterative form): the damaged
/// Cauchy stress from the effective stress `sigma` (module docs).
#[must_use]
#[allow(clippy::needless_range_loop)] // upstream's (i, j) loops, kept for the sum order
pub fn damaged_stress<const D: usize>(
    sigma: &Matrix<D, D>,
    o: &Matrix<D, D>,
    s: &Matrix<D, D>,
    damage: &Matrix<D, D>,
    eta: Real,
    max_damage: Real,
) -> Matrix<D, D> {
    let dim = D as Real;
    let first = s.matmul(sigma).matmul(s);
    let mut second = 0.;
    for i in 0..D {
        for j in 0..D {
            second += sigma[j][i] * o[j][i];
        }
    }
    let tr_d = damage.trace();
    second /= dim - tr_d;
    let third = 1. / dim * sigma.trace() * (1. - std_min(eta / dim * tr_d, max_damage));
    let mut out = first;
    for j in 0..D {
        for i in 0..D {
            out[j][i] -= o[j][i] * second;
        }
    }
    for i in 0..D {
        out[i][i] += third;
    }
    out
}

/// The damage update of the maximum point, staged by `updateDamage` before
/// the count reduction and written after it.
#[derive(Clone, Debug)]
struct StagedDamage<const DIM: usize> {
    ty: ElementType,
    range: std::ops::Range<usize>,
    damage: Matrix<DIM, DIM>,
    dirs: Matrix<DIM, DIM>,
}

/// `orthotropic_damage_iterative` (upstream
/// `MaterialOrthotropicDamageIterative<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialOrthotropicDamageIterative<const DIM: usize> {
    /// The elastic parent.
    pub elastic: MaterialElastic<DIM>,
    /// Damage tensor, diagonal in its frame (`damage`, `dim²`).
    pub damage: InternalField<Real>,
    /// `damage dissipated energy`.
    pub dissipated_energy: InternalField<Real>,
    /// `integral of sigma`.
    pub int_sigma: InternalField<Real>,
    /// Damage frame (`damage_principal_directions`, `dim²`).
    pub damage_dir_vecs: InternalField<Real>,
    /// Strength `Sc` (random internal field).
    pub sc: InternalField<Real>,
    /// Normalised equivalent stress.
    pub equivalent_stress: InternalField<Real>,
    /// Principal stress directions (`equiv_stress_dir`, `dim²`).
    pub stress_dir: InternalField<Real>,
    /// `eta`: hydrostatic damage sensitivity.
    pub eta: Real,
    /// `max_damage`.
    pub max_damage: Real,
    /// `prescribed_dam`.
    pub prescribed_dam: Real,
    /// `dam_threshold`.
    pub dam_threshold: Real,
    /// Upstream's global `Math::tolerance` for this law (default
    /// `1e2·ε`; the upstream tests call `Math::setTolerance`).
    pub tolerance: Real,
    /// Extra-materials defect switches (UD-573, UD-581).
    pub extra_options: ExtraLawOptions,
    norm_max_equivalent_stress: Real,
    q_max: Option<(ElementType, usize)>,
}

impl<const DIM: usize> MaterialOrthotropicDamageIterative<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Self::with_type(spec, mesh, "orthotropic_damage_iterative")
    }

    /// New law registered under `law_type` (non-local variant).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn with_type(
        spec: &LawSpec,
        mesh: Option<&akantu_mesh::Mesh>,
        law_type: &str,
    ) -> Result<Self> {
        let n = DIM * DIM;
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = law_type.into();
        Ok(Self {
            elastic,
            damage: InternalField::new("damage", n, 0.0)?,
            dissipated_energy: InternalField::new("damage dissipated energy", 1, 0.0)?,
            int_sigma: InternalField::new("integral of sigma", 1, 0.0)?,
            damage_dir_vecs: InternalField::new("damage_principal_directions", n, 0.0)?,
            sc: InternalField::new_random("Sc", 1, RandomParameter::default())?,
            equivalent_stress: InternalField::new("equivalent_stress", 1, 0.0)?,
            stress_dir: InternalField::new("equiv_stress_dir", n, 0.0)?,
            eta: 2.,
            max_damage: 0.99999,
            prescribed_dam: 0.1,
            dam_threshold: 0.8,
            tolerance: DEFAULT_TOLERANCE,
            extra_options: ExtraLawOptions::default(),
            norm_max_equivalent_stress: 0.,
            q_max: None,
        })
    }

    /// Reset the maximum (upstream `computeAllStresses(_not_ghost)` prologue).
    pub fn reset_norm_max_equivalent_stress(&mut self) {
        self.norm_max_equivalent_stress = 0.;
    }

    /// Mutable maximum normalised equivalent stress (the declared
    /// reduction of the non-local variant, UD-1005).
    pub fn norm_max_equivalent_stress_mut(&mut self) -> &mut Real {
        &mut self.norm_max_equivalent_stress
    }

    /// The point of the maximum (type, law-local quadrature index).
    #[must_use]
    pub fn q_max(&self) -> Option<(ElementType, usize)> {
        self.q_max
    }

    /// Apply the damage of every point of the block to `sigma` (effective
    /// → Cauchy stress).
    fn damage_block(
        &self,
        sigma: &mut Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = DIM * DIM;
        let dam = self.damage.get(ty, ghost)?;
        let dirs = self.damage_dir_vecs.get(ty, ghost)?;
        check_lengths(
            "MaterialOrthotropicDamageIterative::computeStress",
            sigma.len(),
            &[dam.len(), dirs.len()],
        )?;
        for ((s, d), v) in sigma
            .as_mut_slice()
            .chunks_exact_mut(n)
            .zip(dam.as_slice().chunks_exact(n))
            .zip(dirs.as_slice().chunks_exact(n))
        {
            let d: &Matrix<DIM, DIM> = as_matrix(d)?;
            let (o, sq) = one_minus_d(d, as_matrix(v)?, self.tolerance);
            let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
            *s = damaged_stress(s, &o, &sq, d, self.eta, self.max_damage);
        }
        Ok(())
    }

    /// Upstream `computeNormalizedEquivalentStress(grad_u, type, ghost)`.
    ///
    /// # Errors
    /// Missing blocks, size mismatches or eigen-solver failures.
    pub fn compute_normalized_equivalent_stress(
        &mut self,
        grad_u: &Array<Real>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let n = DIM * DIM;
        let constants = self.elastic.constants();
        let mut sigma = Array::try_zeros(grad_u.len(), n)?;
        for (g, s) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(sigma.as_mut_slice().chunks_exact_mut(n))
        {
            constants.stress_on_quad(as_matrix(g)?, 0., as_matrix_mut(s)?);
        }
        self.damage_block(&mut sigma, ty, ghost)?;
        let sc = self.sc.get(ty, ghost)?.clone();
        let mut eq = self.equivalent_stress.get_mut(ty, ghost)?;
        check_lengths(
            "MaterialOrthotropicDamageIterative::computeNormalizedEquivalentStress",
            sigma.len(),
            &[sc.len(), eq.len()],
        )?;
        let mut dirs_out = Vec::with_capacity(sigma.len() * n);
        for ((s, &c), e) in sigma
            .as_slice()
            .chunks_exact(n)
            .zip(sc.as_slice())
            .zip(eq.as_mut_slice())
        {
            let (values, vectors) = sorted_eigen(as_matrix::<DIM, DIM>(s)?)?;
            dirs_out.extend_from_slice(vectors.as_flattened());
            *e = values[0] / c;
        }
        self.stress_dir
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&dirs_out);
        Ok(())
    }

    /// `D_s = V_σᵀ·(V D Vᵀ)·V_σ` with `D_s(0, 0) += prescribed_dam`.
    fn incremented_damage_in_stress_frame(
        &self,
        d: &Matrix<DIM, DIM>,
        v: &Matrix<DIM, DIM>,
        v_sigma: &Matrix<DIM, DIM>,
    ) -> Matrix<DIM, DIM> {
        let dc = rotate_into_computation_frame(d, v);
        let mut ds = rotate_into_new_frame(&dc, v_sigma);
        ds[0][0] += self.prescribed_dam;
        ds
    }

    /// Upstream `findMaxNormalizedEquivalentStress(type, ghost)`.
    ///
    /// # Errors
    /// Missing blocks, size mismatches or eigen-solver failures.
    pub fn find_max_normalized_equivalent_stress(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if ghost != NG {
            return Ok(());
        }
        let n = DIM * DIM;
        let eq = self.equivalent_stress.get(ty, NG)?;
        let dam = self.damage.get(ty, NG)?;
        let dirs = self.damage_dir_vecs.get(ty, NG)?;
        let sdirs = self.stress_dir.get(ty, NG)?;
        check_lengths(
            "MaterialOrthotropicDamageIterative::findMaxNormalizedEquivalentStress",
            eq.len(),
            &[dam.len(), dirs.len(), sdirs.len()],
        )?;
        let tol = self.tolerance;
        let mut best = (self.norm_max_equivalent_stress, self.q_max);
        for q in 0..eq.len() {
            let e = eq.as_slice()[q];
            let r = q * n..(q + 1) * n;
            let d: &Matrix<DIM, DIM> = as_matrix(&dam.as_slice()[r.clone()])?;
            let tr = d.trace();
            if e > best.0 && (DIM as Real * self.max_damage - tr > tol) {
                if are_float_equal(tr, 0., tol) {
                    best = (e, Some((ty, q)));
                } else {
                    let ds = self.incremented_damage_in_stress_frame(
                        d,
                        as_matrix(&dirs.as_slice()[r.clone()])?,
                        as_matrix(&sdirs.as_slice()[r])?,
                    );
                    let values = eigenvalues(&ds)?;
                    let limit_reached = values.iter().any(|&x| x + tol > self.max_damage);
                    if !limit_reached {
                        best = (e, Some((ty, q)));
                    }
                }
            }
        }
        (self.norm_max_equivalent_stress, self.q_max) = best;
        Ok(())
    }

    /// Upstream `computeStress(type, ghost)` with the equivalent stress
    /// evaluated on `criterion_grad_u` (`None`: the law's `grad_u`).
    ///
    /// # Errors
    /// Missing blocks, size mismatches, eigen-solver failures or
    /// `finite_deformation` (UD-572).
    pub fn compute_stress_with_criterion(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        criterion_grad_u: Option<&Array<Real>>,
    ) -> Result<()> {
        small_strain_only(
            &self.elastic.base().law_type,
            self.elastic.base().finite_deformation,
        )?;
        self.elastic.compute_stress(ctx, ty, ghost)?;
        let mut sigma = self.elastic.base().stress.get(ty, ghost)?.clone();
        self.damage_block(&mut sigma, ty, ghost)?;
        self.elastic
            .base_mut()
            .stress
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        let own;
        let grad_u = if let Some(g) = criterion_grad_u {
            g
        } else {
            own = self.elastic.base().grad_u.get(ty, ghost)?.clone();
            &own
        };
        self.compute_normalized_equivalent_stress(grad_u, ty, ghost)?;
        if !self.extra_options.equivalent_stress_max_over_types {
            // UD-573: upstream resets before every type (ghost included)
            self.norm_max_equivalent_stress = 0.;
        }
        self.find_max_normalized_equivalent_stress(ty, ghost)
    }

    /// Upstream `updateDamage()`.
    ///
    /// On more than one rank the whole update is one collective scope of
    /// the mesh's communicator: the UD-870 choice of the
    /// damaging rank (`Min` all-reduce, default mode only) and the damaged
    /// count (`Sum`, upstream `comm.allReduce(nb_damaged_elements, _sum)`)
    /// are checked collectives whose agreement carries every local outcome
    /// before them (the parameter check, then the staging of the new damage:
    /// the UD-870 compat refusal "no point holds the maximum", an
    /// eigen-solver failure, a missing block). A failure on one rank is
    /// therefore returned on every rank at the same step and nothing is
    /// changed anywhere; the damage is written only after the count.
    ///
    /// # Errors
    /// `prescribed_dam ≤ 0` or NaN (upstream debug assertion), missing blocks,
    /// eigen-solver or communication failures (on several ranks: this
    /// rank's error, or "failed on another rank").
    pub fn update_damage_orthotropic(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        let comm = ctx.services.mesh().communicator();
        if comm.size() <= 1 {
            self.check_prescribed_dam()?;
            let (staged, n) = self.stage_damage(self.norm_max_equivalent_stress >= 1.)?;
            self.apply_damage(staged)?;
            return Ok(n);
        }
        let mut ph = Phases::new(comm, "orthotropic_damage_iterative:update_damage");
        let r = self.update_damage_in(&mut ph);
        ph.settle(r)
    }

    /// The steps of [`Self::update_damage_orthotropic`] on several ranks, in
    /// the update's scope `ph`: no local result leaves before the agreement
    /// that carries it.
    fn update_damage_in(&mut self, ph: &mut Phases) -> Result<UInt> {
        let pre = self.check_prescribed_dam();
        let damages = if self.norm_max_equivalent_stress >= 1. {
            self.damages_its_maximum(ph, pre)
        } else {
            pre.map(|()| false)
        };
        let staged = damages.and_then(|d| self.stage_damage(d));
        sum_count_and_apply(ph, staged, |s| self.apply_damage(s))
    }

    /// `prescribed_dam ≤ 0` or NaN is refused (upstream debug assertion).
    fn check_prescribed_dam(&self) -> Result<()> {
        if self.prescribed_dam.is_nan() || self.prescribed_dam <= 0. {
            return Err(Error::invalid(
                "Your prescribed damage must be greater than zero",
            ));
        }
        Ok(())
    }

    /// The local part of `updateDamage`: if `damages`, the new damage tensor
    /// and frame of the maximum point `q_max` (count 1), nothing modified.
    ///
    /// # Errors
    /// No maximum point (UD-870 compat on a rank without a candidate),
    /// `q_max` outside its blocks, eigen-solver failures.
    fn stage_damage(&self, damages: bool) -> Result<(Option<StagedDamage<DIM>>, UInt)> {
        if !damages {
            return Ok((None, 0));
        }
        let (ty, q) = self.q_max.ok_or_else(|| {
            Error::invalid("orthotropic_damage_iterative: no point holds the maximum")
        })?;
        let n = DIM * DIM;
        let r = q * n..(q + 1) * n;
        let outside = || Error::invalid("q_max outside the damage block");
        let d: Matrix<DIM, DIM> = *as_matrix(
            self.damage
                .get(ty, NG)?
                .as_slice()
                .get(r.clone())
                .ok_or_else(outside)?,
        )?;
        let v: Matrix<DIM, DIM> = *as_matrix(
            self.damage_dir_vecs
                .get(ty, NG)?
                .as_slice()
                .get(r.clone())
                .ok_or_else(outside)?,
        )?;
        let vs: Matrix<DIM, DIM> = *as_matrix(
            self.stress_dir
                .get(ty, NG)?
                .as_slice()
                .get(r.clone())
                .ok_or_else(outside)?,
        )?;
        let ds = self.incremented_damage_in_stress_frame(&d, &v, &vs);
        let (values, w) = sorted_eigen(&ds)?;
        let dirs = if self
            .extra_options
            .orthotropic_directions_in_computation_frame
        {
            vs.matmul(&w)
        } else {
            w
        };
        let mut new_d = d;
        for i in 0..DIM {
            new_d[i][i] = values[i];
            if new_d[i][i] + self.tolerance >= self.dam_threshold {
                new_d[i][i] = self.max_damage;
            }
        }
        Ok((
            Some(StagedDamage {
                ty,
                range: r,
                damage: new_d,
                dirs,
            }),
            1,
        ))
    }

    /// Write a staged update (both blocks checked before the first write).
    fn apply_damage(&mut self, staged: Option<StagedDamage<DIM>>) -> Result<()> {
        let Some(s) = staged else {
            return Ok(());
        };
        let outside = || Error::invalid("q_max outside the damage block");
        let mut dam = self.damage.get_mut(s.ty, NG)?;
        let mut dirs = self.damage_dir_vecs.get_mut(s.ty, NG)?;
        let d = dam
            .as_mut_slice()
            .get_mut(s.range.clone())
            .ok_or_else(outside)?;
        let v = dirs.as_mut_slice().get_mut(s.range).ok_or_else(outside)?;
        d.copy_from_slice(s.damage.as_flattened());
        v.copy_from_slice(s.dirs.as_flattened());
        Ok(())
    }

    /// UD-870: whether this rank damages its local maximum point once the
    /// global maximum reached 1. Upstream: every rank (one point per rank).
    /// Corrected default: on one rank always; on more ranks only the lowest
    /// rank whose local maximum equals the global one (the all-reduced
    /// maximum is one rank's exact value; an exact tie between ranks goes to
    /// the lowest rank, which can differ from the serial run's first point
    /// in element order), so the run damages one point as the
    /// serial run. Every rank calls it under the same condition (the global
    /// maximum, the communicator size and the switch are common); its `Min`
    /// all-reduce is a checked collective of the update's scope `ph` whose
    /// agreement carries `pre` (the local outcome so far).
    ///
    /// # Errors
    /// `pre`'s error (this rank) or "failed on another rank", transport
    /// failures.
    fn damages_its_maximum(&self, ph: &mut Phases, pre: Result<()>) -> Result<bool> {
        if !self.extra_options.orthotropic_damage_global_maximum_only {
            return pre.map(|()| true);
        }
        let (rank, size) = ph.rank_size();
        let local = self.q_max.and_then(|(ty, q)| {
            self.equivalent_stress
                .get(ty, NG)
                .ok()
                .and_then(|eq| eq.as_slice().get(q).copied())
        });
        let size = UInt::try_from(size).unwrap_or(UInt::MAX);
        let rank = UInt::try_from(rank).unwrap_or(UInt::MAX);
        let candidate = if local == Some(self.norm_max_equivalent_stress) {
            rank
        } else {
            size
        };
        ph.run_collective("damaging_rank", pre.map(|()| [candidate]), |c, mut v| {
            c.all_reduce(&mut v, ReduceOp::Min)?;
            Ok(v[0] == rank)
        })
    }

    #[allow(clippy::needless_range_loop)] // diagonal entries by index, as upstream
    /// Upstream `computeTangentModuliOnQuad` (2-D formula, 1-D scaling,
    /// 3-D unchanged) on the column-major Voigt tangent `t` holding the
    /// elastic tangent.
    pub fn damaged_tangent_on_quad(
        &self,
        t: &mut [Real],
        d: &Matrix<DIM, DIM>,
        dirs: &Matrix<DIM, DIM>,
    ) {
        let trace_d = d.trace();
        let (o, s) = {
            // rotated whatever the trace (upstream rotates in the tangent)
            let mut op = Matrix::<DIM, DIM>::identity();
            for (oc, dc) in op.iter_mut().zip(d) {
                for (x, y) in oc.iter_mut().zip(dc) {
                    *x -= *y;
                }
            }
            let mut sp = op;
            for i in 0..DIM {
                sp[i][i] = sp[i][i].sqrt();
            }
            (
                rotate_into_computation_frame(&op, dirs),
                rotate_into_computation_frame(&sp, dirs),
            )
        };
        if DIM == 1 {
            let f = 1. - d[0][0];
            for x in t.iter_mut() {
                *x *= f;
            }
            return;
        }
        if DIM != 2 {
            return; // UD-582: 3-D left unchanged upstream
        }
        let v = 3;
        let c = |i: usize, j: usize| t[i + j * v];
        let (c00, c01, c10, c11, c22) = (c(0, 0), c(0, 1), c(1, 0), c(1, 1), c(2, 2));
        let o_ = |i: usize, j: usize| o[j][i];
        let s_ = |i: usize, j: usize| s[j][i];
        let min_val = std_min(self.eta / 2. * trace_d, self.max_damage);
        let td2 = trace_d - 2.;
        let mut out = [0.; 9];
        let mut set = |i: usize, j: usize, x: Real| out[i + j * v] = x;
        set(
            0,
            0,
            c00 * s_(0, 0) * s_(0, 0) + c10 * s_(0, 1) * s_(0, 1)
                - (min_val / 2. - 1. / 2.) * (c00 + c10)
                + (o_(0, 0) * (c00 * o_(0, 0) + c10 * o_(1, 1))) / td2,
        );
        set(
            0,
            1,
            c01 * s_(0, 0) * s_(0, 0) + c11 * s_(0, 1) * s_(0, 1)
                - (min_val / 2. - 1. / 2.) * (c01 + c11)
                + (o_(0, 0) * (c01 * o_(0, 0) + c11 * o_(1, 1))) / td2,
        );
        set(
            0,
            2,
            2. * c22 * s_(0, 0) * s_(0, 1) + (2. * c22 * o_(0, 0) * o_(0, 1)) / td2,
        );
        set(
            1,
            0,
            c00 * s_(0, 1) * s_(0, 1) + c10 * s_(1, 1) * s_(1, 1)
                - (min_val / 2. - 1. / 2.) * (c00 + c10)
                + (o_(1, 1) * (c00 * o_(0, 0) + c10 * o_(1, 1))) / td2,
        );
        set(
            1,
            1,
            c01 * s_(0, 1) * s_(0, 1) + c11 * s_(1, 1) * s_(1, 1)
                - (min_val / 2. - 1. / 2.) * (c01 + c11)
                + (o_(1, 1) * (c01 * o_(0, 0) + c11 * o_(1, 1))) / td2,
        );
        set(
            1,
            2,
            2. * c22 * s_(0, 1) * s_(1, 1) + (2. * c22 * o_(0, 1) * o_(1, 1)) / td2,
        );
        set(
            2,
            0,
            (o_(0, 1) * (c00 * o_(0, 0) + c10 * o_(1, 1))) / td2
                + c00 * s_(0, 0) * s_(0, 1)
                + c10 * s_(0, 1) * s_(1, 1),
        );
        set(
            2,
            1,
            (o_(0, 1) * (c01 * o_(0, 0) + c11 * o_(1, 1))) / td2
                + c01 * s_(0, 0) * s_(0, 1)
                + c11 * s_(0, 1) * s_(1, 1),
        );
        set(
            2,
            2,
            (2. * c22 * o_(0, 1) * o_(0, 1)) / td2
                + c22 * s_(0, 1) * s_(0, 1)
                + c22 * s_(0, 0) * s_(1, 1),
        );
        t.copy_from_slice(&out);
    }

    /// Upstream `MaterialOrthotropicDamage::updateEnergies(type)`.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn orthotropic_update_energies(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
    ) -> Result<()> {
        self.elastic.compute_potential_energy(ctx, ty)?;
        let n = DIM * DIM;
        let base = self.elastic.base();
        let gu = base.grad_u.get(ty, NG)?;
        let pgu = base.grad_u.previous(ty, NG)?;
        let s = base.stress.get(ty, NG)?;
        let ps = base.stress.previous(ty, NG)?;
        let epot = base.potential_energy.get(ty, NG)?;
        let mut ints = self.int_sigma.get_mut(ty, NG)?;
        let mut ed = self.dissipated_energy.get_mut(ty, NG)?;
        check_lengths(
            "MaterialOrthotropicDamage::updateEnergies",
            gu.len(),
            &[
                pgu.len(),
                s.len(),
                ps.len(),
                epot.len(),
                ints.len(),
                ed.len(),
            ],
        )?;
        for q in 0..gu.len() {
            let r = q * n..(q + 1) * n;
            let g: &Matrix<DIM, DIM> = as_matrix(&gu.as_slice()[r.clone()])?;
            let pg: &Matrix<DIM, DIM> = as_matrix(&pgu.as_slice()[r.clone()])?;
            let sg: &Matrix<DIM, DIM> = as_matrix(&s.as_slice()[r.clone()])?;
            let psg: &Matrix<DIM, DIM> = as_matrix(&ps.as_slice()[r])?;
            let dint = 0.5 * sg.add(psg).double_dot(&g.sub(pg));
            ints.as_mut_slice()[q] += dint;
            ed.as_mut_slice()[q] = ints.as_slice()[q] - epot.as_slice()[q];
        }
        Ok(())
    }

    const PARAMS: &'static [ParamDescriptor<Self>] = orthotropic_params!();
    const PARAMS_2D: &'static [ParamDescriptor<Self>] = orthotropic_params!(plane_stress);
}

impl<const DIM: usize> MaterialParams for MaterialOrthotropicDamageIterative<DIM> {
    fn params_base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn params_base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }
    fn own_params(&self) -> &'static [ParamDescriptor<Self>] {
        if DIM == 2 {
            Self::PARAMS_2D
        } else {
            Self::PARAMS
        }
    }
    fn update_params(&mut self) -> Result<()> {
        MaterialParams::update_params(&mut self.elastic)
    }
}

akantu_solid::impl_material_registry!(MaterialOrthotropicDamageIterative<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialOrthotropicDamageIterative<DIM> {
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
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        f("damage", InternalFieldRef::Real(&self.damage));
        f(
            "damage dissipated energy",
            InternalFieldRef::Real(&self.dissipated_energy),
        );
        f("integral of sigma", InternalFieldRef::Real(&self.int_sigma));
        f(
            "damage_principal_directions",
            InternalFieldRef::Real(&self.damage_dir_vecs),
        );
        f("Sc", InternalFieldRef::Real(&self.sc));
        f(
            "equivalent_stress",
            InternalFieldRef::Real(&self.equivalent_stress),
        );
        f("equiv_stress_dir", InternalFieldRef::Real(&self.stress_dir));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        f("damage", InternalFieldMut::Real(&mut self.damage))?;
        f(
            "damage dissipated energy",
            InternalFieldMut::Real(&mut self.dissipated_energy),
        )?;
        f(
            "integral of sigma",
            InternalFieldMut::Real(&mut self.int_sigma),
        )?;
        f(
            "damage_principal_directions",
            InternalFieldMut::Real(&mut self.damage_dir_vecs),
        )?;
        f("Sc", InternalFieldMut::Real(&mut self.sc))?;
        f(
            "equivalent_stress",
            InternalFieldMut::Real(&mut self.equivalent_stress),
        )?;
        f(
            "equiv_stress_dir",
            InternalFieldMut::Real(&mut self.stress_dir),
        )
    }
}

impl<const DIM: usize> Material for MaterialOrthotropicDamageIterative<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    /// Upstream `MaterialElastic::initMaterial` (through the parents).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.elastic.base_mut().init_history()?;
        let blocks = self.elastic.base().blocks(ctx);
        self.resize_internals(&blocks)?;
        MaterialParams::update_params(self)?;
        self.elastic.base_mut().set_init();
        if DIM == 1 {
            self.elastic.thermal.nu = 0.;
        }
        MaterialParams::update_params(self)
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_stress_with_criterion(ctx, ty, ghost, None)
    }

    /// Upstream `computeAllStresses`: reset (not ghost), all types; its
    /// all-reduce of the maximum (no RVE exception upstream) is declared:
    /// [`Material::reduce_after_stresses`].
    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        if ghost == NG {
            self.norm_max_equivalent_stress = 0.;
        }
        all_stresses(self, ctx, ghost)
    }

    /// Upstream `computeAllStresses` epilogue
    /// (`material_orthotropic_damage_iterative.cc:163-165`:
    /// `allReduce(norm_max_equivalent_stress, _max)`), through the model's
    /// reducer.
    fn reduce_after_stresses(
        &mut self,
        _ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        if reducer.nb_ranks() > 1 {
            let mut v = [self.norm_max_equivalent_stress];
            reducer.all_reduce_real(&mut v, ReduceOp::Max)?;
            self.norm_max_equivalent_stress = v[0];
        }
        Ok(())
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        small_strain_only(
            &self.elastic.base().law_type,
            self.elastic.base().finite_deformation,
        )?;
        self.elastic
            .compute_tangent_moduli(ctx, ty, ghost, tangent)?;
        let n = DIM * DIM;
        let v = tangent_stiffness_voigt_size(DIM);
        let dam = self.damage.get(ty, ghost)?;
        let dirs = self.damage_dir_vecs.get(ty, ghost)?;
        check_lengths(
            "MaterialOrthotropicDamage::computeTangentModuli",
            tangent.len(),
            &[dam.len(), dirs.len()],
        )?;
        for ((t, d), dv) in tangent
            .as_mut_slice()
            .chunks_exact_mut(v * v)
            .zip(dam.as_slice().chunks_exact(n))
            .zip(dirs.as_slice().chunks_exact(n))
        {
            let d: &Matrix<DIM, DIM> = as_matrix(d)?;
            if !are_float_equal(d.trace(), 0., self.tolerance) {
                self.damaged_tangent_on_quad(t, d, as_matrix(dv)?);
            }
        }
        Ok(())
    }

    fn compute_potential_energy(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.elastic.compute_potential_energy(ctx, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "dissipated" => integrate_field(self.elastic.base(), &self.dissipated_energy, ctx),
            "potential" => self.potential_energy(ctx),
            _ => Ok(0.0),
        }
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        self.elastic.potential_energy_by_element(ctx, element)
    }

    fn update_energies(&mut self, ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.orthotropic_update_energies(ctx, ty)
    }

    fn update_energies_after_damage(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
    ) -> Result<()> {
        self.orthotropic_update_energies(ctx, ty)
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

    /// The tangent changes with the damage: `true`, as every damage law
    /// (UD-582, B: upstream inherits `MaterialElastic::hasStiffnessMatrixChanged`,
    /// `false` after the first assembly, so a damage update between solves
    /// keeps the undamaged stiffness;
    /// [`ExtraLawOptions::orthotropic_stiffness_changed_after_damage`]
    /// `= false` reproduces it).
    fn has_stiffness_matrix_changed(&self) -> bool {
        if self
            .extra_options
            .orthotropic_stiffness_changed_after_damage
        {
            true
        } else {
            Material::has_stiffness_matrix_changed(&self.elastic)
        }
    }

    fn options(&self) -> Option<&LawOptions> {
        Some(&self.elastic.options)
    }

    fn options_mut(&mut self) -> Option<&mut LawOptions> {
        Some(&mut self.elastic.options)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl<const DIM: usize> IterativeDamage for MaterialOrthotropicDamageIterative<DIM> {
    fn norm_max_equivalent_stress(&self) -> Real {
        self.norm_max_equivalent_stress
    }

    fn update_damage(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        self.update_damage_orthotropic(ctx)
    }

    /// Upstream's orthotropic law always all-reduces.
    fn set_rve_mode(&mut self, _in_rve: bool) {}
}

/// Register `orthotropic_damage_iterative` (upstream
/// `INSTANTIATE_MATERIAL(orthotropic_damage_iterative, …)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    factory.register(
        "orthotropic_damage_iterative",
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                let mut law = MaterialOrthotropicDamageIterative::<D>::new(spec, mesh)?;
                crate::options::HasExtraOptions::set_extra_options(&mut law, options);
                Box::new(law) as Box<dyn Material>
            })
        }),
    )
}
