// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/extra-materials/src/
// material_damage/material_damage_iterative.hh, material_damage_iterative.cc,
// material_damage_iterative_inline_impl.hh (and the MaterialDamage /
// MaterialElastic parents they call).

//! `damage_iterative` (XMAT-001): sequentially linear ("saw-tooth")
//! isotropic damage on top of `elastic` (upstream
//! `MaterialDamageIterative<dim> : MaterialDamage<dim>`).
//!
//! The law never damages inside `computeStress`; the user (or the RVE
//! model) drives the damage between solves:
//!
//! 1. `computeStress(type, ghost)`: `MaterialElastic::computeStress`
//!    (thermal pre-step, `σ = C:∇u + σ_th`), then `σ *= 1 − d` per point;
//!    the **normalised equivalent stress** `σ_eq = max eig((1 − d)·C:∇u) /
//!    Sc` (elastic kernel with `σ_th = 0`, eigenvalues of Eigen's
//!    `EigenSolver`, [`crate::kernels::max_eigenvalue`]); the maximum
//!    `norm_max_equivalent_stress` over the not-ghost points whose damage is
//!    below `max_damage`;
//! 2. `computeAllStresses(ghost)`: resets the maximum (not-ghost), computes
//!    every type, then all-reduces the maximum (`_max`) over the ranks
//!    unless the law belongs to an RVE model;
//! 3. `updateDamage()` (user call, after a converged solve): if the maximum
//!    is `≥ 1`, every not-ghost point with `σ_eq ≥ (1 − dam_tolerance)·max`
//!    and fewer than `max_reductions` reductions gets one more reduction:
//!    `d += prescribed_dam`, or `d = max_damage` at the last one; returns
//!    the number of damaged points, summed over the ranks unless RVE.
//!
//! Tangent `(1 − d)·C`, energies (`updateEnergies`, dissipated energy,
//! `updateEnergiesAfterDamage` = `MaterialDamage::updateEnergies`) and
//! history: `akantu_solid::materials::damage`.
//!
//! Parameters (registration order after the elastic ones): `Sc` (random
//! internal field, parsable), `prescribed_dam` (0.1), `dam_threshold` (0.8,
//! only used by the unused `updateDamage(quad, …)` overload), `dam_tolerance`
//! (0.01), `max_damage` (0.99999), `max_reductions` (`UInt`, 10).
//! Internals: `Sc`, `damage_step` (`UInt`), `equivalent_stress`.
//!
//! # Upstream defects
//!
//! * **UD-573** (B): upstream's `computeStress` resets the maximum to 0
//!   before **every** element type (and for ghost blocks), so the maximum
//!   is the last type's; [`ExtraLawOptions::equivalent_stress_max_over_types`]
//!   (default) keeps the maximum over all not-ghost types of one
//!   `computeAllStresses`. Identical for single-type meshes.
//! * **UD-574** (C, preserved): the equivalent stress ignores the thermal
//!   stress (explicit `σ_th = 0`) while the stress includes it; the
//!   `updateDamage(quad, …)` overload (threshold `dam_threshold`) is kept
//!   as [`MaterialDamageIterative::update_damage_on_quad`].
//! * `prescribed_dam ≤ 0` or NaN is an error in `updateDamage` (upstream debug
//!   assertion).

use std::sync::Arc;

use akantu_comm::Communicator;
use akantu_comm::communicator::ReduceOp;
use akantu_comm::phase::{Phases, Severity};
use akantu_core::enums::MatrixType;
use akantu_core::random::RandomParameter;
use akantu_core::tensor::{Matrix, as_matrix, as_matrix_mut};
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result, UInt,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawReducer, LawSpec,
    ModelServices,
};
use akantu_parser::param::ParamDescriptor;
use akantu_solid::material::{LawOptions, Material, MaterialBase, MaterialFactory, MaterialParams};
use akantu_solid::material_tensor::tangent_stiffness_voigt_size;
use akantu_solid::materials::damage::{
    DamageFields, DamageWork, elastic_damage_energy_densities, elastic_damage_potential_energy,
    elastic_family_thermal_work, thermal_work_enabled,
};
use akantu_solid::materials::elastic::MaterialElastic;

use crate::kernels::{check_lengths, max_eigenvalue, scale_by_integrity, small_strain_only};
use crate::options::ExtraLawOptions;

const NG: GhostType = GhostType::NotGhost;

/// The interface of the sequentially linear damage laws driven between
/// solves (`damage_iterative`, `iterative_stiffness_reduction`,
/// `orthotropic_damage_iterative`; upstream calls them through
/// `MaterialDamageIterative<dim>&` / the concrete classes).
pub trait IterativeDamage {
    /// Upstream `getNormMaxEquivalentStress()`.
    fn norm_max_equivalent_stress(&self) -> Real;

    /// Upstream `updateDamage()`: damage the most stressed points; the
    /// number of damaged points (summed over the ranks unless the law
    /// belongs to an RVE model). Collective on several ranks, outside any
    /// other collective operation (it opens its own scope): a local
    /// failure on one rank is returned on every rank and changes nothing.
    ///
    /// # Errors
    /// Missing blocks, invalid parameters or communication failures.
    fn update_damage(&mut self, ctx: &LawContext<'_>) -> Result<UInt>;

    /// Mark the law as owned by an RVE model (upstream
    /// `dynamic_cast<SolidMechanicsModelRVE *>(&model)`): no all-reduce of
    /// the maximum and of the damaged count.
    fn set_rve_mode(&mut self, in_rve: bool);
}

/// One point damaged by `updateDamage` (staged before the count
/// reduction, applied after it).
#[derive(Clone, Copy, Debug)]
pub(crate) struct PointUpdate {
    pub(crate) ty: ElementType,
    pub(crate) q: usize,
    pub(crate) damage: Real,
    pub(crate) reductions: UInt,
    /// New strength (`iterative_stiffness_reduction`), if any.
    pub(crate) sc: Option<Real>,
}

/// The end of every iterative `updateDamage`: the local
/// outcome `staged` (the damaged points and their count, or the error that
/// stopped the local step; nothing modified yet) is agreed **with** the
/// count reduction (upstream `comm.allReduce(nb_damaged, _sum)`), in one
/// scope of the law's communicator, then `apply` runs on every rank.
///
/// On one rank, or when the count is not reduced (`reduce = false`: an RVE
/// law, whose model is local), the update is local. On several ranks a
/// local failure on any rank (a parameter check, a missing block, an
/// eigen-solver failure, …) makes every rank return an error at the same
/// agreement and **no** rank applies anything: never a rank waiting in a
/// reduction that another rank left (the raw `Sum` all-reduce the laws
/// used before). `apply` cannot fail once staging validated the points; if
/// it still did, the failure is agreed as fatal on every rank.
///
/// # Errors
/// The local error (this rank), "failed on another rank" (the others),
/// transport failures (fatal), a refused scope (called inside another
/// collective operation of the communicator: refused on every rank).
pub(crate) fn agreed_damage_update<S>(
    comm: &Communicator,
    scope: &str,
    reduce: bool,
    staged: Result<(S, UInt)>,
    apply: impl FnOnce(S) -> Result<()>,
) -> Result<UInt> {
    if !reduce {
        let (s, n) = staged?;
        apply(s)?;
        return Ok(n);
    }
    let mut ph = Phases::new(comm, scope);
    let r = sum_count_and_apply(&mut ph, staged, apply);
    ph.settle(r)
}

/// [`agreed_damage_update`]'s steps inside an open scope `ph`: `count`
/// (agreement on `staged`, then the `Sum` all-reduce), `apply`, agreement
/// on the application (fatal).
///
/// # Errors
/// As [`agreed_damage_update`].
pub(crate) fn sum_count_and_apply<S>(
    ph: &mut Phases,
    staged: Result<(S, UInt)>,
    apply: impl FnOnce(S) -> Result<()>,
) -> Result<UInt> {
    let (s, total) = ph.run_collective("count", staged, |c, (s, n)| {
        let mut v = [n];
        c.all_reduce(&mut v, ReduceOp::Sum)?;
        Ok((s, v[0]))
    })?;
    ph.agree("apply", apply(s), Severity::Fatal)?;
    Ok(total)
}

/// `damage_iterative` (upstream `MaterialDamageIterative<DIM>`).
#[derive(Clone, Debug)]
pub struct MaterialDamageIterative<const DIM: usize> {
    /// The elastic parent (parameters, thermal state, base internals,
    /// options).
    pub elastic: MaterialElastic<DIM>,
    /// `damage` (history), `damage dissipated energy`, `integral of sigma`.
    pub damage: DamageFields,
    /// Strength `Sc` (random internal field).
    pub sc: InternalField<Real>,
    /// Number of reductions per point (`damage_step`, `UInt`).
    pub reduction_step: InternalField<UInt>,
    /// Normalised equivalent stress per point.
    pub equivalent_stress: InternalField<Real>,
    /// `max_reductions`.
    pub max_reductions: UInt,
    /// `prescribed_dam`.
    pub prescribed_dam: Real,
    /// `dam_threshold`.
    pub dam_threshold: Real,
    /// `dam_tolerance`.
    pub dam_tolerance: Real,
    /// `max_damage`.
    pub max_damage: Real,
    /// Extra-materials defect switches (UD-573).
    pub extra_options: ExtraLawOptions,
    norm_max_equivalent_stress: Real,
    in_rve: bool,
}

impl<const DIM: usize> MaterialDamageIterative<DIM> {
    /// New law (upstream constructor chain).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        Self::with_type(spec, mesh, "damage_iterative")
    }

    /// New law registered under `law_type` (derived laws).
    ///
    /// # Errors
    /// A spatial dimension other than `DIM`.
    pub fn with_type(
        spec: &LawSpec,
        mesh: Option<&akantu_mesh::Mesh>,
        law_type: &str,
    ) -> Result<Self> {
        let mut elastic = MaterialElastic::<DIM>::new(spec, mesh)?;
        elastic.base_mut().law_type = law_type.into();
        Ok(Self {
            elastic,
            damage: DamageFields::new()?,
            sc: InternalField::new_random("Sc", 1, RandomParameter::default())?,
            reduction_step: InternalField::new("damage_step", 1, 0)?,
            equivalent_stress: InternalField::new("equivalent_stress", 1, 0.0)?,
            max_reductions: 10,
            prescribed_dam: 0.1,
            dam_threshold: 0.8,
            dam_tolerance: 0.01,
            max_damage: 0.99999,
            extra_options: ExtraLawOptions::default(),
            norm_max_equivalent_stress: 0.,
            in_rve: false,
        })
    }

    /// Whether the law belongs to an RVE model (no all-reduces).
    #[must_use]
    pub fn in_rve(&self) -> bool {
        self.in_rve
    }

    /// Mutable maximum normalised equivalent stress (upstream
    /// `getNormMaxEquivalentStress()` non-const reference).
    pub fn norm_max_equivalent_stress_mut(&mut self) -> &mut Real {
        &mut self.norm_max_equivalent_stress
    }

    /// Upstream `MaterialElastic::initMaterial` (not overridden by
    /// `MaterialDamage`/`MaterialDamageIterative`): history, resize of every
    /// internal of `law` in register-id order (the draw order of `Sc`),
    /// parameter update, then `ν = 0` in 1-D and the update again.
    ///
    /// # Errors
    /// Resize or parameter-update errors.
    pub(crate) fn init_like_elastic<L: ConstitutiveLaw + ?Sized>(
        law: &mut L,
        ctx: &LawContext<'_>,
        base: impl Fn(&mut L) -> &mut Self,
    ) -> Result<()> {
        base(law).elastic.base_mut().init_history()?;
        let blocks = base(law).elastic.base().blocks(ctx);
        law.resize_internals(&blocks)?;
        let me = base(law);
        MaterialParams::update_params(&mut me.elastic)?;
        me.elastic.base_mut().set_init();
        if DIM == 1 {
            me.elastic.thermal.nu = 0.;
        }
        MaterialParams::update_params(&mut me.elastic)
    }

    /// The equivalent stress of one point: `max eig((1 − d)·C:∇u) / Sc`
    /// (upstream `computeNormalizedEquivalentStress` body, `σ_th = 0`).
    ///
    /// # Errors
    /// Eigen-solver failures (non-finite input).
    pub fn equivalent_stress_on_quad(
        &self,
        grad_u: &Matrix<DIM, DIM>,
        damage: Real,
        sc: Real,
    ) -> Result<Real> {
        let mut sigma = [[0.; DIM]; DIM];
        self.elastic
            .constants()
            .stress_on_quad(grad_u, 0., &mut sigma);
        scale_by_integrity(&mut sigma, damage);
        Ok(max_eigenvalue(&sigma)? / sc)
    }

    /// Upstream `computeNormalizedEquivalentStress(grad_u, type, ghost)`:
    /// the equivalent stress of every point of the block from `grad_u`
    /// (the law's own `grad_u`, or the non-local average of the non-local
    /// variant).
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
        let nq = grad_u.len();
        let sc = self.sc.get(ty, ghost)?;
        let dam = self.damage.damage.get(ty, ghost)?;
        check_lengths(
            "MaterialDamageIterative::computeNormalizedEquivalentStress",
            nq,
            &[
                sc.len(),
                dam.len(),
                self.equivalent_stress.get(ty, ghost)?.len(),
            ],
        )?;
        if grad_u.nb_component() != n {
            return Err(Error::DimensionMismatch {
                context: "MaterialDamageIterative::computeNormalizedEquivalentStress: grad_u",
                expected: n,
                got: grad_u.nb_component(),
            });
        }
        let mut out = Vec::with_capacity(nq);
        for ((g, &d), &s) in grad_u
            .as_slice()
            .chunks_exact(n)
            .zip(dam.as_slice())
            .zip(sc.as_slice())
        {
            out.push(self.equivalent_stress_on_quad(as_matrix(g)?, d, s)?);
        }
        self.equivalent_stress
            .get_mut(ty, ghost)?
            .as_mut_slice()
            .copy_from_slice(&out);
        Ok(())
    }

    /// Upstream `findMaxNormalizedEquivalentStress(type, ghost)`: for
    /// not-ghost blocks, raise the maximum with every point whose damage is
    /// below `max_damage`.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn find_max_normalized_equivalent_stress(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        if ghost != NG {
            return Ok(());
        }
        let eq = self.equivalent_stress.get(ty, NG)?;
        let dam = self.damage.damage.get(ty, NG)?;
        check_lengths(
            "MaterialDamageIterative::findMaxNormalizedEquivalentStress",
            eq.len(),
            &[dam.len()],
        )?;
        for (&e, &d) in eq.as_slice().iter().zip(dam.as_slice()) {
            if e > self.norm_max_equivalent_stress && d < self.max_damage {
                self.norm_max_equivalent_stress = e;
            }
        }
        Ok(())
    }

    /// The part of upstream `computeStress` after the elastic stress: `σ *=
    /// 1 − d` on the block's `stress`.
    fn apply_damage_to_stress(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let n = DIM * DIM;
        let dam = self.damage.damage.get(ty, ghost)?.clone();
        let mut sigma = self.elastic.base_mut().stress.get_mut(ty, ghost)?;
        check_lengths(
            "MaterialDamageIterative::computeStress",
            sigma.len(),
            &[dam.len()],
        )?;
        for (s, &d) in sigma.as_mut_slice().chunks_exact_mut(n).zip(dam.as_slice()) {
            scale_by_integrity(as_matrix_mut::<DIM, DIM>(s)?, d);
        }
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
        // MaterialDamage::computeStress = MaterialElastic::computeStress
        self.elastic.compute_stress(ctx, ty, ghost)?;
        self.apply_damage_to_stress(ty, ghost)?;
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

    /// Reset the maximum (upstream `computeAllStresses(_not_ghost)` prologue).
    pub fn reset_norm_max_equivalent_stress(&mut self) {
        self.norm_max_equivalent_stress = 0.;
    }

    /// All-reduce the maximum unless the law belongs to an RVE (upstream
    /// `computeAllStresses` epilogue, `material_damage_iterative.cc:
    /// 110-118`: `comm.allReduce(norm_max_equivalent_stress, _max)`),
    /// declared: issued through the model's reducer right after the
    /// stress phase ([`Material::reduce_after_stresses`]), never through the
    /// communicator inside the solver's collective scope.
    ///
    /// # Errors
    /// Reducer errors (the stress phase failed on some rank, transport).
    pub fn reduce_norm_max_in(&mut self, reducer: &mut dyn LawReducer) -> Result<()> {
        if self.in_rve || reducer.nb_ranks() <= 1 {
            return Ok(());
        }
        let mut v = [self.norm_max_equivalent_stress];
        reducer.all_reduce_real(&mut v, ReduceOp::Max)?;
        self.norm_max_equivalent_stress = v[0];
        Ok(())
    }

    /// `true` if `updateDamage` sums its count over the ranks (upstream:
    /// unless the law belongs to an RVE model).
    pub(crate) fn reduces_count(&self, ctx: &LawContext<'_>) -> bool {
        !self.in_rve && ctx.services.mesh().communicator().size() > 1
    }

    /// `prescribed_dam ≤ 0` or NaN is refused (upstream debug assertion).
    pub(crate) fn check_prescribed_dam(&self) -> Result<()> {
        if self.prescribed_dam.is_nan() || self.prescribed_dam <= 0. {
            return Err(Error::invalid(
                "Your prescribed damage must be greater than zero",
            ));
        }
        Ok(())
    }

    /// Upstream `updateDamage()` (saw-tooth increments): staged locally,
    /// agreed with the count reduction, then applied
    /// (`agreed_damage_update`).
    ///
    /// # Errors
    /// `prescribed_dam ≤ 0` or NaN (upstream debug assertion), missing blocks or
    /// communication failures; on more than one rank a failure on any rank
    /// is returned on every rank and changes nothing.
    pub fn update_damage_sawtooth(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        let staged = self.stage_sawtooth();
        let reduce = self.reduces_count(ctx);
        agreed_damage_update(
            ctx.services.mesh().communicator(),
            "damage_iterative:update_damage",
            reduce,
            staged,
            |changes| self.apply_sawtooth(&changes),
        )
    }

    /// The local part of the saw-tooth `updateDamage`: the new `(damage,
    /// reduction count)` of every damaged point, nothing modified.
    fn stage_sawtooth(&self) -> Result<(Vec<PointUpdate>, UInt)> {
        self.check_prescribed_dam()?;
        let mut changes = Vec::new();
        if self.norm_max_equivalent_stress >= 1. {
            let threshold = (1. - self.dam_tolerance) * self.norm_max_equivalent_stress;
            for ty in self.elastic.base().filter_types(NG) {
                let eq = self.equivalent_stress.get(ty, NG)?;
                let dam = self.damage.damage.get(ty, NG)?;
                let red = self.reduction_step.get(ty, NG)?;
                check_lengths(
                    "MaterialDamageIterative::updateDamage",
                    eq.len(),
                    &[dam.len(), red.len()],
                )?;
                for (q, ((&e, &d), &r)) in eq
                    .as_slice()
                    .iter()
                    .zip(dam.as_slice())
                    .zip(red.as_slice())
                    .enumerate()
                {
                    if e >= threshold {
                        if r == self.max_reductions {
                            continue;
                        }
                        let r = r + 1;
                        let d = if r == self.max_reductions {
                            self.max_damage
                        } else {
                            d + self.prescribed_dam
                        };
                        changes.push(PointUpdate {
                            ty,
                            q,
                            damage: d,
                            reductions: r,
                            sc: None,
                        });
                    }
                }
            }
        }
        let n = UInt::try_from(changes.len())
            .map_err(|_| Error::invalid("updateDamage: damaged-point count overflows UInt"))?;
        Ok((changes, n))
    }

    /// Write staged point updates (`damage`, `damage_step`, and `Sc` where
    /// given). Every index is checked before the first write, so a failure
    /// changes nothing.
    pub(crate) fn apply_sawtooth(&mut self, changes: &[PointUpdate]) -> Result<()> {
        for c in changes {
            let ok = self.damage.damage.get(c.ty, NG)?.len() > c.q
                && self.reduction_step.get(c.ty, NG)?.len() > c.q
                && (c.sc.is_none() || self.sc.get(c.ty, NG)?.len() > c.q);
            if !ok {
                return Err(Error::invalid(
                    "updateDamage: staged point outside its block",
                ));
            }
        }
        for c in changes {
            self.damage.damage.get_mut(c.ty, NG)?.as_mut_slice()[c.q] = c.damage;
            self.reduction_step.get_mut(c.ty, NG)?.as_mut_slice()[c.q] = c.reductions;
            if let Some(sc) = c.sc {
                self.sc.get_mut(c.ty, NG)?.as_mut_slice()[c.q] = sc;
            }
        }
        Ok(())
    }

    /// Upstream `updateDamage(quad_index, eq_stress, type, ghost)` (unused
    /// upstream, UD-574): if the point's equivalent stress is `≥ (1 −
    /// dam_tolerance)·max`, `d += prescribed_dam` below `dam_threshold`,
    /// else `d = max_damage`; returns 1 if damaged.
    ///
    /// # Errors
    /// `prescribed_dam ≤ 0` or NaN, missing blocks or an index past the block.
    pub fn update_damage_on_quad(
        &mut self,
        quad_index: usize,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<UInt> {
        if self.prescribed_dam.is_nan() || self.prescribed_dam <= 0. {
            return Err(Error::invalid(
                "Your prescribed damage must be greater than zero",
            ));
        }
        let e = *self
            .equivalent_stress
            .get(ty, ghost)?
            .as_slice()
            .get(quad_index)
            .ok_or(Error::InvalidIndex {
                what: "quadrature point",
                value: i64::try_from(quad_index).unwrap_or(i64::MAX),
            })?;
        let threshold = (1. - self.dam_tolerance) * self.norm_max_equivalent_stress;
        let mut d_g = self.damage.damage.get_mut(ty, ghost)?;
        let d = d_g
            .as_mut_slice()
            .get_mut(quad_index)
            .ok_or(Error::InvalidIndex {
                what: "quadrature point",
                value: i64::try_from(quad_index).unwrap_or(i64::MAX),
            })?;
        if e >= threshold {
            if *d < self.dam_threshold {
                *d += self.prescribed_dam;
            } else {
                *d = self.max_damage;
            }
            return Ok(1);
        }
        Ok(0)
    }

    /// Upstream `MaterialDamage::updateEnergies(type)` (also
    /// `updateEnergiesAfterDamage`): potential energy, then the dissipated
    /// energy bookkeeping.
    ///
    /// # Errors
    /// Missing blocks or size mismatches.
    pub fn damage_update_energies(&mut self, ty: ElementType) -> Result<()> {
        elastic_damage_potential_energy(&mut self.elastic, &self.damage.damage, ty)?;
        let options = self.elastic.options;
        let thermal = if thermal_work_enabled(&options) {
            Some(elastic_family_thermal_work(
                &self.elastic,
                &self.damage.damage,
                None,
                ty,
            )?)
        } else {
            None
        };
        let work = DamageWork::new(&options, thermal.as_deref());
        // serial: the law's points are not independent (the damage step
        // picks the most stressed point of the law)
        let exec = akantu_core::exec::Exec::serial_ref();
        self.damage
            .update_energies::<DIM>(exec, self.elastic.base(), ty, work)
    }

    /// The internals of the law (registration order) for reflection.
    pub(crate) fn visit(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        self.damage.for_each(f);
        f("Sc", InternalFieldRef::Real(&self.sc));
        f("damage_step", InternalFieldRef::UInt(&self.reduction_step));
        f(
            "equivalent_stress",
            InternalFieldRef::Real(&self.equivalent_stress),
        );
    }

    /// Mutable [`MaterialDamageIterative::visit`].
    pub(crate) fn visit_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        self.damage.for_each_mut(f)?;
        f("Sc", InternalFieldMut::Real(&mut self.sc))?;
        f(
            "damage_step",
            InternalFieldMut::UInt(&mut self.reduction_step),
        )?;
        f(
            "equivalent_stress",
            InternalFieldMut::Real(&mut self.equivalent_stress),
        )
    }
}

/// The parameter table of a law embedding a `MaterialDamageIterative`
/// (elastic parameters first, then the iterative ones, then `$extra`):
/// `iterative_table!([plane_stress] Self; [path to the elastic];
/// [path to the iterative law]; [extra descriptors…])`.
macro_rules! iterative_table {
    ($($ps:ident)? ; $S:ty ; [$($e:ident).+] ; [$($p:ident).*] ; [$($extra:expr),* $(,)?]) => {
        akantu_solid::elastic_parent_params!($($ps)? $S, [$($e).+], [
            akantu_solid::random_field_param!($S, "Sc", ::akantu_parser::param::ParameterAccess::PARSABLE, "critical stress threshold", $($p.)* sc),
            akantu_solid::real_param!($S, "prescribed_dam", ::akantu_parser::param::ParameterAccess::PARSMOD, "prescribed damage", $($p.)* prescribed_dam),
            akantu_solid::real_param!($S, "dam_threshold", ::akantu_parser::param::ParameterAccess::PARSMOD, "damage threshold at which damage damage will be set to 1", $($p.)* dam_threshold),
            akantu_solid::real_param!($S, "dam_tolerance", ::akantu_parser::param::ParameterAccess::PARSMOD, "damage tolerance to decide if quadrature point will be damageed", $($p.)* dam_tolerance),
            akantu_solid::real_param!($S, "max_damage", ::akantu_parser::param::ParameterAccess::PARSMOD, "maximum damage value", $($p.)* max_damage),
            ::akantu_parser::param::ParamDescriptor::<$S>::new(
                "max_reductions",
                ::akantu_parser::param::ParamKind::UInt,
                ::akantu_parser::param::ParameterAccess::PARSMOD,
                "max reductions",
                |s| ::akantu_parser::param::ParamValue::Int(i64::from(s.$($p.)* max_reductions)),
                Some(|s, v| {
                    let n = v.as_int()?;
                    s.$($p.)* max_reductions = ::akantu_core::UInt::try_from(n).map_err(|_| {
                        ::akantu_core::Error::invalid(format!("max_reductions: {n} is not a UInt"))
                    })?;
                    Ok(())
                }),
            ),
            $($extra),*
        ])
    };
}

impl<const DIM: usize> MaterialDamageIterative<DIM> {
    const PARAMS: &'static [ParamDescriptor<Self>] = iterative_table!(; Self; [elastic]; []; []);
    const PARAMS_2D: &'static [ParamDescriptor<Self>] =
        iterative_table!(plane_stress; Self; [elastic]; []; []);
}

impl<const DIM: usize> MaterialParams for MaterialDamageIterative<DIM> {
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

akantu_solid::impl_material_registry!(MaterialDamageIterative<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialDamageIterative<DIM> {
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
        self.visit(f);
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.visit_mut(f)
    }
}

/// Upstream `Material::computeAllStresses(ghost)` (the trait's default
/// body, for laws that override `computeAllStresses` around it): for every
/// type of the filter with elements, `grad_u ← ∇u − eigen_grad_u`, then
/// `computeStress`.
///
/// # Errors
/// Gradient, size or stress errors.
pub(crate) fn all_stresses<M: Material + ?Sized>(
    law: &mut M,
    ctx: &LawContext<'_>,
    ghost: GhostType,
) -> Result<()> {
    let dim = law.base().spatial_dimension;
    for ty in law.base().filter_types(ghost) {
        let filter = law
            .base()
            .element_filter
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        if filter.is_empty() {
            continue;
        }
        let mut gu = Array::new(dim * dim);
        ctx.services.gradient_on_integration_points(
            "displacement",
            &mut gu,
            dim,
            ty,
            ghost,
            &filter,
        )?;
        {
            let base = law.base_mut();
            let eigen = base.eigen_grad_u.get(ty, ghost)?;
            check_lengths(
                "computeAllStresses: grad_u -= eigen_grad_u",
                eigen.len(),
                &[gu.len()],
            )?;
            for (g, e) in gu.as_mut_slice().iter_mut().zip(eigen.as_slice()) {
                *g -= *e;
            }
            base.grad_u.get_mut(ty, ghost)?.copy_from(&gu)?;
        }
        law.compute_stress(ctx, ty, ghost)?;
    }
    Ok(())
}

impl<const DIM: usize> Material for MaterialDamageIterative<DIM> {
    fn base(&self) -> &MaterialBase {
        self.elastic.base()
    }
    fn base_mut(&mut self) -> &mut MaterialBase {
        self.elastic.base_mut()
    }

    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        Self::init_like_elastic(self, ctx, |s| s)
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_stress_with_criterion(ctx, ty, ghost, None)
    }

    /// Upstream `MaterialDamageIterative::computeAllStresses` (its rank
    /// reduction of the maximum is declared:
    /// [`Material::reduce_after_stresses`]).
    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        if ghost == NG {
            self.norm_max_equivalent_stress = 0.;
        }
        all_stresses(self, ctx, ghost)
    }

    fn reduce_after_stresses(
        &mut self,
        _ghost: GhostType,
        reducer: &mut dyn LawReducer,
    ) -> Result<()> {
        self.reduce_norm_max_in(reducer)
    }

    fn compute_tangent_moduli(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        small_strain_only(
            &self.elastic.base().law_type,
            self.elastic.base().finite_deformation,
        )?;
        let el = &self.elastic;
        self.damage.tangent(
            ty,
            ghost,
            tangent_stiffness_voigt_size(DIM),
            tangent,
            &|t| el.tangent_on_quad(t),
        )
    }

    fn compute_potential_energy(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        elastic_damage_potential_energy(&mut self.elastic, &self.damage.damage, ty)
    }

    fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
        match energy_id {
            "dissipated" => self.damage.dissipated_energy(self.elastic.base(), ctx),
            "potential" => self.potential_energy(ctx),
            _ => Ok(0.0),
        }
    }

    fn potential_energy_by_element(
        &self,
        ctx: &LawContext<'_>,
        element: Element,
    ) -> Result<Vec<Real>> {
        let points =
            akantu_solid::materials::damage::element_points(self.elastic.base(), ctx, element)?;
        elastic_damage_energy_densities(&self.elastic, &self.damage.damage, element.ty(), points)
    }

    fn update_energies(&mut self, _ctx: &LawContext<'_>, ty: ElementType) -> Result<()> {
        self.damage_update_energies(ty)
    }

    /// Upstream `updateEnergiesAfterDamage` = `MaterialDamage::updateEnergies`.
    fn update_energies_after_damage(
        &mut self,
        _ctx: &LawContext<'_>,
        ty: ElementType,
    ) -> Result<()> {
        self.damage_update_energies(ty)
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

    /// Upstream `MaterialDamage::hasStiffnessMatrixChanged`: always `true`.
    fn has_stiffness_matrix_changed(&self) -> bool {
        true
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

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl<const DIM: usize> IterativeDamage for MaterialDamageIterative<DIM> {
    fn norm_max_equivalent_stress(&self) -> Real {
        self.norm_max_equivalent_stress
    }

    fn update_damage(&mut self, ctx: &LawContext<'_>) -> Result<UInt> {
        self.update_damage_sawtooth(ctx)
    }

    fn set_rve_mode(&mut self, in_rve: bool) {
        self.in_rve = in_rve;
    }
}

/// Register `damage_iterative` (upstream
/// `INSTANTIATE_MATERIAL(damage_iterative, MaterialDamageIterative)`).
///
/// # Errors
/// A duplicate registration.
pub fn register(factory: &mut MaterialFactory, options: ExtraLawOptions) -> Result<()> {
    factory.register(
        "damage_iterative",
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            let mesh = Some(services.mesh());
            akantu_core::with_dim!(spec.spatial_dimension, D => {
                let mut law = MaterialDamageIterative::<D>::new(spec, mesh)?;
                law.extra_options = options;
                Box::new(law) as Box<dyn Material>
            })
        }),
    )
}
