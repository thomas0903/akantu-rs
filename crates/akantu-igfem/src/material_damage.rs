// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/material_igfem/
// material_igfem_saw_tooth_damage{.hh,.cc,_inline_impl.hh},
// material_igfem_iterative_stiffness_reduction.{hh,cc}, and the
// MaterialDamage<dim, MaterialIGFEMElastic> parent
// (src/model/solid_mechanics/materials/material_damage/material_damage{.hh,_tmpl.hh}).

//! IGFEM damage laws (IGFEM-007, IGFEM-008).
//!
//! * [`MaterialIgfemSawToothDamage`] (`igfem_saw_tooth_damage`, upstream
//!   `MaterialIGFEMSawToothDamage<dim> : MaterialDamage<dim,
//!   MaterialIGFEMElastic>`): the IGFEM analogue of `damage_iterative`
//!   (XMAT-001). On top of `igfem_elastic` (per-point Lamé fields of the two
//!   sub-materials), every integration point carries a damage `d` (with
//!   history), a strength `Sc` and the normalised equivalent stress
//!   `σ_eq = max eig((1 − d)·σ(∇u)) / Sc` (eigenvalues of Eigen's
//!   `EigenSolver` on a run-time `dim × dim` matrix, the first largest).
//!   The stress is `(1 − d)·σ(∇u)`, the tangent `(1 − d)·C`. The maximum
//!   `norm_max_equivalent_stress` runs over the not-ghost points of the
//!   damageable sub-material (`sub_material ≠ 0`) whose damage is below
//!   `max_damage`. `updateDamage()` (user call between solves): if the
//!   maximum is `≥ 1`, every element with a damageable point of `σ_eq ≥
//!   (1 − dam_tolerance)·max` is damaged on all its damageable points
//!   (`d += prescribed_dam` below `dam_threshold`, else `d = max_damage`);
//!   returns the number of damaged **elements**.
//! * [`MaterialIgfemIterativeStiffnessReduction`]
//!   (`igfem_iterative_stiffness_reduction`, upstream
//!   `MaterialIGFEMIterativeStiffnessReduction<dim> :
//!   MaterialIGFEMSawToothDamage<dim>`): the IGFEM analogue of
//!   `iterative_stiffness_reduction` (XMAT-003, Rots–Invernizzi): internals
//!   `ultimate_strain` (`ε_u`), `damage_step` and `tangent` (`D`);
//!   `updateDamage()` increments the reduction step `k` of every damageable
//!   point of a damaged element (points already at `max_reductions` do not
//!   trigger), `d = 1 − 1/a^k` and `Sc = ε_u (1 − d) E D / ((1 − d) E + D)`
//!   (`E` of the second sub-material), the last step `d = max_damage`.
//!
//! The laws read their damage parameters from their **second sub-material**
//! (upstream `initMaterial`: `prescribed_dam`, `dam_threshold`,
//! `dam_tolerance`, `max_damage`; and `crack_band_width`, `max_reductions`,
//! `reduction_constant`, `Gf`, `E`), a `damage_iterative` /
//! `iterative_stiffness_reduction` law of the same model; the IGFEM model
//! sets them ([`crate::model::SolidMechanicsModelIgfem::refresh_sub_material_constants`]).
//!
//! Their `Sc`, `damage`, `ultimate_strain` and `tangent` start at 0 and are
//! **transferred** from the elements an interface update replaces (upstream
//! `internals_to_transfer`, `SolidMechanicsModelIGFEM::transferInternalValues`
//! and the copy-back of the law's `onElementsAdded`, done by the IGFEM
//! model's event handler, [`crate::transfer`]); after the transfer the
//! stiffness-reduction law recomputes `damage_step` from the damage
//! (`d = max_damage` → `max_reductions`, else the `i < max_reductions` with
//! `1 − 1/a^i = d` within the law's tolerance).
//!
//! Upstream defects (D-003):
//! * UD-1201 (B): `computeStress` resets the maximum before every type (as
//!   `damage_iterative`, UD-573); default: the maximum over every not-ghost
//!   type of one `computeAllStresses`
//!   ([`MaterialIgfemSawToothDamage::equivalent_stress_max_over_types`];
//!   `false` reproduces upstream).
//! * UD-1202 (A): `MaterialDamage::updateEnergies` (after every converged
//!   step) calls the IGFEM elastic `computePotentialEnergy`, which is not
//!   implemented (UD-613): the port does no energy bookkeeping for these
//!   laws and refuses the dissipated energy (`Error::NotImplemented`).
//! * UD-1203 (B, no compat): the unused overload `updateDamage(quad_index,
//!   …)` divides the point index by the number of **elements** to find its
//!   element; the port divides by the points per element.
//! * Serial only (UD-609, D-085): no all-reduce of the maximum or of the
//!   count.

use std::sync::Arc;

use akantu_core::array::ArrayViewMut;
use akantu_core::enums::MatrixType;
use akantu_core::{
    Array, Element, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result, UInt,
};
use akantu_linalg::dense::{Storage, eig};
use akantu_mesh::Mesh;
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec, ModelServices,
};
use akantu_model::model::FeEngineRegistry;
use akantu_parser::param::{ParamDescriptor, ParamValue};
use akantu_solid::material::{DerivedMaterialParams, Material, MaterialBase, MaterialFactory};
use akantu_solid::materials::damage::DamageFields;

use crate::context::IgfemContext;
use crate::material::{
    IgfemMaterial, MaterialIgfemElastic, remove_law_integration_points, resize_law_internals,
};

/// Registered name of the IGFEM saw-tooth damage law.
pub const IGFEM_SAW_TOOTH_DAMAGE: &str = "igfem_saw_tooth_damage";

/// Registered name of the IGFEM iterative stiffness-reduction law.
pub const IGFEM_ITERATIVE_STIFFNESS_REDUCTION: &str = "igfem_iterative_stiffness_reduction";

const NG: GhostType = GhostType::NotGhost;

fn check_len(context: &'static str, expected: usize, got: usize) -> Result<()> {
    if expected == got {
        Ok(())
    } else {
        Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        })
    }
}

fn real_param(param: &dyn Fn(&str) -> Result<ParamValue>, name: &str) -> Result<Real> {
    match param(name)? {
        ParamValue::Real(v) => Ok(v),
        ParamValue::Int(v) => Ok(v as Real),
        other => Err(Error::invalid(format!(
            "IGFEM damage law: parameter {name} of the second sub-material is not a real \
             ({other:?})"
        ))),
    }
}

fn uint_param(param: &dyn Fn(&str) -> Result<ParamValue>, name: &str) -> Result<UInt> {
    match param(name)? {
        ParamValue::Int(v) => UInt::try_from(v).map_err(|_| {
            Error::invalid(format!(
                "IGFEM damage law: parameter {name} of the second sub-material is negative"
            ))
        }),
        other => Err(Error::invalid(format!(
            "IGFEM damage law: parameter {name} of the second sub-material is not an \
             unsigned integer ({other:?})"
        ))),
    }
}

/// Upstream `MaterialIGFEMSawToothDamage<dim>` (`igfem_saw_tooth_damage`).
#[derive(Debug)]
pub struct MaterialIgfemSawToothDamage {
    /// The `igfem_elastic` base (sub-materials, Lamé fields, IGFEM engine).
    pub elastic: MaterialIgfemElastic,
    /// `damage` (history), `damage dissipated energy`, `integral of sigma`
    /// (upstream `MaterialDamage` internals).
    pub damage: DamageFields,
    /// Strength per point (internal `Sc`).
    pub sc: InternalField<Real>,
    /// Normalised equivalent stress per point (`equivalent_stress`).
    pub equivalent_stress: InternalField<Real>,
    /// Damage increment (from the second sub-material).
    pub prescribed_dam: Real,
    /// Relative tolerance below the maximum that still damages.
    pub dam_tolerance: Real,
    /// Damage above which a damaged point jumps to `max_damage`.
    pub dam_threshold: Real,
    /// Maximum damage.
    pub max_damage: Real,
    /// UD-1201 switch: `true` (default) keeps the maximum over every
    /// not-ghost type of one `computeAllStresses`; `false` reproduces
    /// upstream's reset before every type.
    pub equivalent_stress_max_over_types: bool,
    norm_max_equivalent_stress: Real,
    parameters_read: bool,
}

impl MaterialIgfemSawToothDamage {
    /// New law (upstream constructor: `Sc`, `equivalent_stress`, the
    /// damage internals of `MaterialDamage`, all of IGFEM kind;
    /// `internals_to_transfer = {damage, Sc}`).
    ///
    /// # Errors
    /// Bad dimension.
    pub fn new(spec: &LawSpec, mesh: Option<&Mesh>, ctx: Arc<IgfemContext>) -> Result<Self> {
        Self::with_law_type(spec, mesh, ctx, IGFEM_SAW_TOOTH_DAMAGE)
    }

    fn with_law_type(
        spec: &LawSpec,
        mesh: Option<&Mesh>,
        ctx: Arc<IgfemContext>,
        law_type: &str,
    ) -> Result<Self> {
        Ok(Self {
            elastic: MaterialIgfemElastic::with_law_type(spec, mesh, ctx, law_type)?,
            damage: DamageFields::new()?,
            sc: InternalField::new("Sc", 1, 0.0)?,
            equivalent_stress: InternalField::new("equivalent_stress", 1, 0.0)?,
            prescribed_dam: 0.0,
            dam_tolerance: 0.0,
            dam_threshold: 0.0,
            max_damage: 0.0,
            equivalent_stress_max_over_types: true,
            norm_max_equivalent_stress: 0.0,
            parameters_read: false,
        })
    }

    fn dim(&self) -> usize {
        self.elastic.base().spatial_dimension
    }

    /// Upstream `getNormMaxEquivalentStress()`.
    #[must_use]
    pub fn norm_max_equivalent_stress(&self) -> Real {
        self.norm_max_equivalent_stress
    }

    fn own_for_each(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.damage.for_each(f);
        f("Sc", InternalFieldRef::Real(&self.sc));
        f(
            "equivalent_stress",
            InternalFieldRef::Real(&self.equivalent_stress),
        );
    }

    fn own_for_each_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.damage.for_each_mut(f)?;
        f("Sc", InternalFieldMut::Real(&mut self.sc))?;
        f(
            "equivalent_stress",
            InternalFieldMut::Real(&mut self.equivalent_stress),
        )
    }

    /// Upstream `computeNormalizedEquivalentStress(grad_u, type, ghost)`:
    /// `σ_eq = max eig((1 − d)·σ(∇u; λ_q, μ_q)) / Sc` per point.
    ///
    /// # Errors
    /// Missing blocks, size mismatches, eigen-solver failures.
    pub fn compute_normalized_equivalent_stress(
        &mut self,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        let d = self.dim();
        let n = d * d;
        let g = self
            .elastic
            .base()
            .grad_u
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        let nq = g.len() / n;
        let lambda = self.elastic.lambda.get(ty, ghost)?.as_slice().to_vec();
        let mu = self.elastic.mu.get(ty, ghost)?.as_slice().to_vec();
        let dam = self.damage.damage.get(ty, ghost)?.as_slice().to_vec();
        let sc = self.sc.get(ty, ghost)?.as_slice().to_vec();
        for (what, len) in [
            ("lambda", lambda.len()),
            ("mu", mu.len()),
            ("damage", dam.len()),
            ("Sc", sc.len()),
        ] {
            let _ = what;
            check_len(
                "MaterialIGFEMSawToothDamage::computeNormalizedEquivalentStress",
                nq,
                len,
            )?;
        }
        let mut out = vec![0.0; nq];
        let mut sigma = vec![0.0; n];
        for q in 0..nq {
            MaterialIgfemElastic::stress_on_quad(
                d,
                &g[q * n..(q + 1) * n],
                &mut sigma,
                lambda[q],
                mu[q],
            );
            // computeDamageAndStressOnQuad: sigma *= 1 - dam
            let f = 1. - dam[q];
            for v in &mut sigma {
                *v *= f;
            }
            let values = eig(
                &akantu_core::tensor::MatrixX::from_col_major(d, d, sigma.clone())?,
                Storage::Dynamic,
            )?;
            let mut it = values.iter();
            let first = *it
                .next()
                .ok_or_else(|| Error::Numerical("eig: empty spectrum".into()))?;
            // *std::max_element: the first largest
            let max = it.fold(first, |m, &v| if m < v { v } else { m });
            out[q] = max / sc[q];
        }
        let mut e = self.equivalent_stress.get_mut(ty, ghost)?;
        check_len(
            "MaterialIGFEMSawToothDamage::computeNormalizedEquivalentStress: equivalent_stress",
            nq,
            e.len(),
        )?;
        e.as_mut_slice().copy_from_slice(&out);
        Ok(())
    }

    /// Upstream `findMaxNormalizedEquivalentStress(type, ghost)`: not-ghost
    /// blocks raise the maximum with every damageable point (`sub_material
    /// ≠ 0`) whose damage is below `max_damage`.
    ///
    /// # Errors
    /// Missing blocks, size mismatches.
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
        let sub = self.elastic.sub_material.get(ty, NG)?;
        check_len(
            "MaterialIGFEMSawToothDamage::findMaxNormalizedEquivalentStress",
            eq.len(),
            dam.len().min(sub.len()),
        )?;
        for ((&e, &d), &s) in eq.as_slice().iter().zip(dam.as_slice()).zip(sub.as_slice()) {
            if (e > self.norm_max_equivalent_stress && d < self.max_damage) && s != 0 {
                self.norm_max_equivalent_stress = e;
            }
        }
        Ok(())
    }

    /// Upstream `computeStress(type, ghost)`: the IGFEM elastic stress, `σ
    /// *= 1 − d`, the equivalent stress, the maximum.
    fn compute_stress_impl(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.elastic.compute_stress(ctx, ty, ghost)?;
        {
            let n = self.dim() * self.dim();
            let dam = self.damage.damage.get(ty, ghost)?.as_slice().to_vec();
            let mut s = self.elastic.base_mut().stress.get_mut(ty, ghost)?;
            check_len(
                "MaterialIGFEMSawToothDamage::computeStress",
                s.len(),
                dam.len(),
            )?;
            for (sq, &d) in s.as_mut_slice().chunks_exact_mut(n).zip(&dam) {
                let f = 1. - d;
                for v in sq {
                    *v *= f;
                }
            }
        }
        self.compute_normalized_equivalent_stress(ty, ghost)?;
        if !self.equivalent_stress_max_over_types {
            // UD-1201: upstream resets before every type (ghost included)
            self.norm_max_equivalent_stress = 0.;
        }
        self.find_max_normalized_equivalent_stress(ty, ghost)
    }

    /// The tangent `(1 − d)·C` of a block (upstream
    /// `MaterialDamage::computeTangentModuli`).
    fn damaged_tangent(
        elastic: &mut MaterialIgfemElastic,
        damage: &DamageFields,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        elastic.compute_tangent_moduli(ctx, ty, ghost, tangent)?;
        let dam = damage.damage.get(ty, ghost)?;
        check_len(
            "MaterialDamage<MaterialIGFEMElastic>::computeTangentModuli",
            tangent.len(),
            dam.len(),
        )?;
        let nbc = tangent.nb_component();
        for (t, &d) in tangent
            .as_mut_slice()
            .chunks_exact_mut(nbc)
            .zip(dam.as_slice())
        {
            DamageFields::scale_tangent(t, d);
        }
        Ok(())
    }

    fn check_prescribed_dam(&self) -> Result<()> {
        if self.prescribed_dam.is_nan() || self.prescribed_dam <= 0. {
            return Err(Error::invalid(
                "Your prescribed damage must be greater than zero",
            ));
        }
        Ok(())
    }

    /// Upstream `updateDamage()` (saw-tooth, element-wise): see the module
    /// docs. Returns the number of damaged elements.
    ///
    /// # Errors
    /// `prescribed_dam ≤ 0` or NaN (upstream debug assertion), missing
    /// blocks, size mismatches.
    pub fn update_damage(&mut self) -> Result<UInt> {
        self.check_prescribed_dam()?;
        let mut nb_damaged: UInt = 0;
        if self.norm_max_equivalent_stress >= 1. {
            let threshold = (1. - self.dam_tolerance) * self.norm_max_equivalent_stress;
            let engine_nq = |s: &Self, ty| s.elastic.context().engine()?.nb_integration_points(ty);
            for ty in self.elastic.igfem_types(NG) {
                let n_el = self.elastic.filter(ty, NG).len();
                let nq = engine_nq(self, ty)?;
                let eq = self.equivalent_stress.get(ty, NG)?.as_slice().to_vec();
                let sub = self.elastic.sub_material.get(ty, NG)?.as_slice().to_vec();
                let mut dam = self.damage.damage.get_mut(ty, NG)?;
                check_len(
                    "MaterialIGFEMSawToothDamage::updateDamage",
                    n_el * nq,
                    eq.len(),
                )?;
                check_len(
                    "MaterialIGFEMSawToothDamage::updateDamage",
                    eq.len(),
                    sub.len(),
                )?;
                check_len(
                    "MaterialIGFEMSawToothDamage::updateDamage",
                    eq.len(),
                    dam.len(),
                )?;
                let dam = dam.as_mut_slice();
                for e in 0..n_el {
                    let r = e * nq..(e + 1) * nq;
                    let damage_element = r.clone().any(|q| eq[q] >= threshold && sub[q] != 0);
                    if damage_element {
                        nb_damaged += 1;
                        for q in r {
                            if sub[q] != 0 {
                                if dam[q] < self.dam_threshold {
                                    dam[q] += self.prescribed_dam;
                                } else {
                                    dam[q] = self.max_damage;
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(nb_damaged)
    }

    /// Upstream `updateDamage(quad_index, eq_stress, type, ghost)` (unused
    /// upstream): if the point's equivalent stress is `≥ (1 −
    /// dam_tolerance)·max`, every damageable point of its element gets `d
    /// += prescribed_dam` (when the point's damage is below
    /// `dam_threshold`) or `d += max_damage` (as upstream's `+=`); returns
    /// the number of points damaged. The element of the point is `quad_index
    /// / nb_quads` (upstream divides by the number of elements, UD-1203).
    ///
    /// # Errors
    /// `prescribed_dam ≤ 0` or NaN, an index past the block.
    pub fn update_damage_on_quad(
        &mut self,
        quad_index: usize,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<UInt> {
        self.check_prescribed_dam()?;
        let nq = self.elastic.context().engine()?.nb_integration_points(ty)?;
        let eq = self.equivalent_stress.get(ty, ghost)?.as_slice().to_vec();
        let sub = self
            .elastic
            .sub_material
            .get(ty, ghost)?
            .as_slice()
            .to_vec();
        let e = *eq.get(quad_index).ok_or(Error::InvalidIndex {
            what: "quadrature point",
            value: i64::try_from(quad_index).unwrap_or(i64::MAX),
        })?;
        if e < (1. - self.dam_tolerance) * self.norm_max_equivalent_stress {
            return Ok(0);
        }
        let mut dam_g = self.damage.damage.get_mut(ty, ghost)?;
        let dam = dam_g.as_mut_slice();
        check_len("updateDamage(quad)", eq.len(), dam.len())?;
        let own = dam[quad_index];
        let start = (quad_index / nq) * nq;
        let mut damaged = 0;
        for q in start..start + nq {
            if sub[q] != 0 {
                dam[q] += if own < self.dam_threshold {
                    self.prescribed_dam
                } else {
                    self.max_damage
                };
                damaged += 1;
            }
        }
        Ok(damaged)
    }
}

impl DerivedMaterialParams for MaterialIgfemSawToothDamage {
    type Parent = MaterialIgfemElastic;
    fn parent(&self) -> &MaterialIgfemElastic {
        &self.elastic
    }
    fn parent_mut(&mut self) -> &mut MaterialIgfemElastic {
        &mut self.elastic
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        &[]
    }
    fn update_derived(&mut self) -> Result<()> {
        akantu_parser::ParameterRegistry::update_internal_parameters(&mut self.elastic)
    }
    fn derived_id(&self) -> &str {
        &self.elastic.base().id
    }
}

akantu_solid::impl_derived_material_registry!(MaterialIgfemSawToothDamage);

/// Common [`ConstitutiveLaw`] items of the two damage laws (the elastic
/// base's internals first, then the law's own).
macro_rules! igfem_damage_constitutive_law {
    ($ty:ty, $elastic:ident . $($path:ident).*) => {
        impl ConstitutiveLaw for $ty {
            fn name(&self) -> &str {
                &self.$elastic$(.$path)*.base().name
            }
            fn law_id(&self) -> &str {
                &self.$elastic$(.$path)*.base().id
            }
            fn law_type(&self) -> &str {
                &self.$elastic$(.$path)*.base().law_type
            }
            fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
                &self.$elastic$(.$path)*.base().element_filter
            }
            fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
                &mut self.$elastic$(.$path)*.base_mut().element_filter
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
            /// Resize with the IGFEM point counts.
            fn resize_internals(
                &mut self,
                blocks: &[(ElementType, GhostType, usize)],
            ) -> Result<()> {
                let blocks = self.$elastic$(.$path)*.igfem_blocks(blocks)?;
                resize_law_internals(self, &blocks)
            }
            /// Compaction with the IGFEM point counts.
            fn remove_integration_points(
                &mut self,
                new_numbering: &ElementTypeMapArray<Idx>,
                nb_quad: &dyn Fn(ElementType, GhostType) -> usize,
            ) -> Result<()> {
                let ctx = self.$elastic$(.$path)*.context().clone();
                remove_law_integration_points(self, &ctx, new_numbering, nb_quad)
            }
        }
    };
}

impl MaterialIgfemSawToothDamage {
    fn visit(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.elastic.for_each_internal(f);
        self.own_for_each(f);
    }
    fn visit_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.elastic.for_each_internal_mut(f)?;
        self.own_for_each_mut(f)
    }

    /// `initMaterial` of the damage part: the law's own internals resized
    /// (IGFEM point counts), then the elastic base's `initMaterial`.
    fn init_damage(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        if !self.parameters_read {
            return Err(Error::invalid(format!(
                "{}: the damage parameters of the second sub-material were not read (the IGFEM \
                 model reads them before initMaterial)",
                self.elastic.base().name
            )));
        }
        let blocks = self.elastic.base().blocks(ctx);
        let blocks = self.elastic.igfem_blocks(&blocks)?;
        self.own_for_each_mut(&mut |_, f| match f {
            InternalFieldMut::Real(f) => f.resize_blocks(&blocks),
            InternalFieldMut::Int(f) => f.resize_blocks(&blocks),
            InternalFieldMut::UInt(f) => f.resize_blocks(&blocks),
            InternalFieldMut::Bool(f) => f.resize_blocks(&blocks),
        })?;
        self.elastic.init_material(ctx)
    }

    fn all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        if ghost == NG {
            self.norm_max_equivalent_stress = 0.;
        }
        for ty in self.elastic.igfem_types(ghost) {
            let filter = self.elastic.filter(ty, ghost);
            if filter.is_empty() {
                continue;
            }
            self.elastic.update_grad_u(ctx, ty, ghost, &filter)?;
            self.compute_stress_impl(ctx, ty, ghost)?;
        }
        // upstream all-reduces the maximum: serial only (UD-609)
        Ok(())
    }

    #[allow(clippy::type_complexity)] // the hook's block list
    fn stiffness_blocks(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>> {
        let damage = &self.damage;
        self.elastic
            .elemental_stiffness_igfem_with(ctx, ghost, &mut |el, ctx, ty, ghost, t| {
                Self::damaged_tangent(el, damage, ctx, ty, ghost, t)
            })
    }
}

igfem_damage_constitutive_law!(MaterialIgfemSawToothDamage, elastic.);

/// Common [`Material`] items of the two damage laws.
macro_rules! igfem_damage_material_common {
    ($elastic:ident . $($path:ident).*) => {
        fn base(&self) -> &MaterialBase {
            self.$elastic$(.$path)*.base()
        }
        fn base_mut(&mut self) -> &mut MaterialBase {
            self.$elastic$(.$path)*.base_mut()
        }
        fn compute_potential_energy(
            &mut self,
            _ctx: &LawContext<'_>,
            _ty: ElementType,
        ) -> Result<()> {
            Err(Error::NotImplemented(
                "IGFEM damage laws: computePotentialEnergy (MaterialIGFEMElastic: \
                 AKANTU_DEBUG_TO_IMPLEMENT upstream, UD-613)",
            ))
        }
        fn push_wave_speed(&self, _element: Element) -> Result<Real> {
            Err(Error::NotImplemented(
                "IGFEM damage laws: getPushWaveSpeed (Material::AKANTU_TO_IMPLEMENT upstream)",
            ))
        }
        fn shear_wave_speed(&self, _element: Element) -> Result<Real> {
            Err(Error::NotImplemented(
                "IGFEM damage laws: getShearWaveSpeed (Material::AKANTU_TO_IMPLEMENT upstream)",
            ))
        }
        fn tangent_type(&self) -> MatrixType {
            MatrixType::Symmetric
        }
        /// Upstream `MaterialDamage::hasStiffnessMatrixChanged`: always.
        fn has_stiffness_matrix_changed(&self) -> bool {
            true
        }
        /// UD-1202: no energy bookkeeping (upstream's
        /// `MaterialDamage::updateEnergies` needs the IGFEM potential
        /// energy, not implemented, UD-613).
        fn update_energies(&mut self, _ctx: &LawContext<'_>, _ty: ElementType) -> Result<()> {
            Ok(())
        }
        /// UD-1202: the dissipated energy is refused; other ids as the
        /// elastic base.
        fn energy_of(&mut self, ctx: &LawContext<'_>, energy_id: &str) -> Result<Real> {
            if energy_id == "dissipated" {
                return Err(Error::NotImplemented(
                    "IGFEM damage laws: the dissipated energy (MaterialDamage::updateEnergies \
                     needs the IGFEM potential energy, UD-613 / UD-1202)",
                ));
            }
            self.$elastic$(.$path)*.energy_of(ctx, energy_id)
        }
        /// Upstream `onElementsAdded` of the elastic base
        /// (`updateElasticInternals`); the transfer of internals follows in
        /// the IGFEM model's handler ([`crate::transfer`]).
        fn on_elements_added(
            &mut self,
            ctx: &LawContext<'_>,
            elements: &[Element],
        ) -> Result<()> {
            self.$elastic$(.$path)*.on_elements_added(ctx, elements)
        }
        /// `−∫ Bᵀσ` with the damaged stress (IGFEM engine).
        fn assemble_internal_forces_custom(
            &self,
            _fems: &FeEngineRegistry,
            mesh: &Mesh,
            ghost: GhostType,
            f_int: ArrayViewMut<'_, Real>,
        ) -> Option<Result<()>> {
            Some(
                self.$elastic$(.$path)*
                    .assemble_internal_forces_igfem(mesh, ghost, f_int),
            )
        }
        fn as_any(&self) -> Option<&dyn std::any::Any> {
            Some(self)
        }
        fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
            Some(self)
        }
    };
}

impl Material for MaterialIgfemSawToothDamage {
    igfem_damage_material_common!(elastic.);

    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.init_damage(ctx)
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.compute_stress_impl(ctx, ty, ghost)
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        Self::damaged_tangent(&mut self.elastic, &self.damage, ctx, ty, ghost, tangent)
    }

    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        self.all_stresses(ctx, ghost)
    }

    fn elemental_stiffness_custom(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Option<Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>>> {
        Some(self.stiffness_blocks(ctx, ghost))
    }
}

impl IgfemMaterial for MaterialIgfemSawToothDamage {
    fn igfem_base(&self) -> &MaterialIgfemElastic {
        &self.elastic
    }
    fn igfem_base_mut(&mut self) -> &mut MaterialIgfemElastic {
        &mut self.elastic
    }
    fn internals_to_transfer(&self) -> &'static [&'static str] {
        &["damage", "Sc"]
    }
    fn read_sub_material_parameters(
        &mut self,
        param: &dyn Fn(&str) -> Result<ParamValue>,
    ) -> Result<()> {
        self.prescribed_dam = real_param(param, "prescribed_dam")?;
        self.dam_threshold = real_param(param, "dam_threshold")?;
        self.dam_tolerance = real_param(param, "dam_tolerance")?;
        self.max_damage = real_param(param, "max_damage")?;
        self.parameters_read = true;
        Ok(())
    }
}

/// Upstream `MaterialIGFEMIterativeStiffnessReduction<dim>`
/// (`igfem_iterative_stiffness_reduction`).
#[derive(Debug)]
pub struct MaterialIgfemIterativeStiffnessReduction {
    /// The `igfem_saw_tooth_damage` parent.
    pub saw: MaterialIgfemSawToothDamage,
    /// Ultimate strain per point (`ultimate_strain`).
    pub eps_u: InternalField<Real>,
    /// Reduction step per point (`damage_step`).
    pub reduction_step: InternalField<UInt>,
    /// Softening tangent per point (`tangent`).
    pub d: InternalField<Real>,
    /// Fracture energy `Gf` (from the second sub-material).
    pub gf: Real,
    /// Crack band width (from the second sub-material).
    pub crack_band_width: Real,
    /// Total reduction steps (from the second sub-material).
    pub max_reductions: UInt,
    /// Reduction constant `a` (from the second sub-material).
    pub reduction_constant: Real,
    /// Young's modulus `E` of the second sub-material (upstream reads it in
    /// every `updateDamage`).
    pub e: Real,
}

impl MaterialIgfemIterativeStiffnessReduction {
    /// New law (upstream constructor: the parent's, `ultimate_strain`,
    /// `damage_step`, `tangent`; `internals_to_transfer` adds
    /// `ultimate_strain`, `tangent`).
    ///
    /// # Errors
    /// Bad dimension.
    pub fn new(spec: &LawSpec, mesh: Option<&Mesh>, ctx: Arc<IgfemContext>) -> Result<Self> {
        Ok(Self {
            saw: MaterialIgfemSawToothDamage::with_law_type(
                spec,
                mesh,
                ctx,
                IGFEM_ITERATIVE_STIFFNESS_REDUCTION,
            )?,
            eps_u: InternalField::new("ultimate_strain", 1, 0.0)?,
            reduction_step: InternalField::new("damage_step", 1, 0)?,
            d: InternalField::new("tangent", 1, 0.0)?,
            gf: 0.0,
            crack_band_width: 0.0,
            max_reductions: 0,
            reduction_constant: 0.0,
            e: 0.0,
        })
    }

    /// Upstream `getNormMaxEquivalentStress()`.
    #[must_use]
    pub fn norm_max_equivalent_stress(&self) -> Real {
        self.saw.norm_max_equivalent_stress()
    }

    fn visit(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.saw.visit(f);
        f("ultimate_strain", InternalFieldRef::Real(&self.eps_u));
        f("damage_step", InternalFieldRef::UInt(&self.reduction_step));
        f("tangent", InternalFieldRef::Real(&self.d));
    }

    fn visit_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.saw.visit_mut(f)?;
        f("ultimate_strain", InternalFieldMut::Real(&mut self.eps_u))?;
        f(
            "damage_step",
            InternalFieldMut::UInt(&mut self.reduction_step),
        )?;
        f("tangent", InternalFieldMut::Real(&mut self.d))
    }

    /// `1 − 1/a^k` (`std::pow(double, int)` upstream: glibc's `pow`).
    fn damage_of_step(&self, k: UInt) -> Real {
        1. - (1. / akantu_core::glibc_libm::pow(self.reduction_constant, Real::from(k)))
    }

    /// Upstream `updateDamage()` (stiffness reduction, element-wise): see
    /// the module docs. Returns the number of damaged elements.
    ///
    /// # Errors
    /// Missing blocks, size mismatches.
    pub fn update_damage(&mut self) -> Result<UInt> {
        let mut nb_damaged: UInt = 0;
        let s = &self.saw;
        if s.norm_max_equivalent_stress < 1. {
            return Ok(0);
        }
        let threshold = (1. - s.dam_tolerance) * s.norm_max_equivalent_stress;
        let e_mod = self.e;
        for ty in self.saw.elastic.igfem_types(NG) {
            let n_el = self.saw.elastic.filter(ty, NG).len();
            let nq = self
                .saw
                .elastic
                .context()
                .engine()?
                .nb_integration_points(ty)?;
            let eq = self.saw.equivalent_stress.get(ty, NG)?.as_slice().to_vec();
            let sub = self
                .saw
                .elastic
                .sub_material
                .get(ty, NG)?
                .as_slice()
                .to_vec();
            let eps_u = self.eps_u.get(ty, NG)?.as_slice().to_vec();
            let dt = self.d.get(ty, NG)?.as_slice().to_vec();
            let n = n_el * nq;
            for len in [eq.len(), sub.len(), eps_u.len(), dt.len()] {
                check_len(
                    "MaterialIGFEMIterativeStiffnessReduction::updateDamage",
                    n,
                    len,
                )?;
            }
            let mut step = self.reduction_step.get(ty, NG)?.as_slice().to_vec();
            let mut dam = self.saw.damage.damage.get(ty, NG)?.as_slice().to_vec();
            let mut sc = self.saw.sc.get(ty, NG)?.as_slice().to_vec();
            for len in [step.len(), dam.len(), sc.len()] {
                check_len(
                    "MaterialIGFEMIterativeStiffnessReduction::updateDamage",
                    n,
                    len,
                )?;
            }
            for el in 0..n_el {
                let r = el * nq..(el + 1) * nq;
                let mut damage_element = false;
                for q in r.clone() {
                    if eq[q] >= threshold && sub[q] != 0 {
                        // a point at the last step does not trigger
                        if step[q] == self.max_reductions {
                            continue;
                        }
                        damage_element = true;
                    }
                }
                if !damage_element {
                    continue;
                }
                nb_damaged += 1;
                for q in r {
                    if sub[q] == 0 {
                        continue;
                    }
                    step[q] += 1;
                    if step[q] == self.max_reductions {
                        dam[q] = self.saw.max_damage;
                    } else {
                        dam[q] = self.damage_of_step(step[q]);
                        sc[q] = eps_u[q] * (1. - dam[q]) * e_mod * dt[q]
                            / ((1. - dam[q]) * e_mod + dt[q]);
                    }
                }
            }
            self.reduction_step
                .get_mut(ty, NG)?
                .as_mut_slice()
                .copy_from_slice(&step);
            self.saw
                .damage
                .damage
                .get_mut(ty, NG)?
                .as_mut_slice()
                .copy_from_slice(&dam);
            self.saw
                .sc
                .get_mut(ty, NG)?
                .as_mut_slice()
                .copy_from_slice(&sc);
        }
        Ok(nb_damaged)
    }

    /// The end of upstream `onElementsAdded` (after the transfer): the
    /// damage step of every damageable point of the law (both ghost types,
    /// every IGFEM type) recomputed from its damage (`d = max_damage` →
    /// `max_reductions`, else the last `i < max_reductions` with `1 − 1/a^i
    /// = d` within the tolerance; unchanged when none matches).
    ///
    /// # Errors
    /// Missing blocks, size mismatches.
    pub fn recompute_damage_steps(&mut self) -> Result<()> {
        let tol = self.saw.elastic.tolerance;
        let eq = |a: Real, b: Real| akantu_core::math::are_float_equal(a, b, tol);
        let max_damage = self.saw.max_damage;
        let steps: Vec<Real> = (0..self.max_reductions)
            .map(|i| self.damage_of_step(i))
            .collect();
        for ghost in GhostType::ALL {
            for ty in self.saw.elastic.igfem_types(ghost) {
                let sub = self
                    .saw
                    .elastic
                    .sub_material
                    .get(ty, ghost)?
                    .as_slice()
                    .to_vec();
                let dam = self.saw.damage.damage.get(ty, ghost)?.as_slice().to_vec();
                let mut st_g = self.reduction_step.get_mut(ty, ghost)?;
                let st = st_g.as_mut_slice();
                check_len("recompute damage steps", sub.len(), dam.len())?;
                check_len("recompute damage steps", sub.len(), st.len())?;
                for q in 0..sub.len() {
                    if sub[q] == 0 {
                        continue;
                    }
                    if eq(dam[q], max_damage) {
                        st[q] = self.max_reductions;
                    } else {
                        for (i, &val) in (0..self.max_reductions).zip(&steps) {
                            if eq(val, dam[q]) {
                                st[q] = i;
                            }
                        }
                    }
                }
            }
        }
        Ok(())
    }
}

impl DerivedMaterialParams for MaterialIgfemIterativeStiffnessReduction {
    type Parent = MaterialIgfemSawToothDamage;
    fn parent(&self) -> &MaterialIgfemSawToothDamage {
        &self.saw
    }
    fn parent_mut(&mut self) -> &mut MaterialIgfemSawToothDamage {
        &mut self.saw
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        &[]
    }
    fn update_derived(&mut self) -> Result<()> {
        self.saw.update_derived()
    }
    fn derived_id(&self) -> &str {
        &self.saw.elastic.base().id
    }
}

akantu_solid::impl_derived_material_registry!(MaterialIgfemIterativeStiffnessReduction);

igfem_damage_constitutive_law!(MaterialIgfemIterativeStiffnessReduction, saw.elastic);

impl Material for MaterialIgfemIterativeStiffnessReduction {
    igfem_damage_material_common!(saw.elastic);

    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        let blocks = self.saw.elastic.base().blocks(ctx);
        let blocks = self.saw.elastic.igfem_blocks(&blocks)?;
        self.eps_u.resize_blocks(&blocks)?;
        self.reduction_step.resize_blocks(&blocks)?;
        self.d.resize_blocks(&blocks)?;
        self.saw.init_damage(ctx)
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.saw.compute_stress_impl(ctx, ty, ghost)
    }

    fn compute_tangent_moduli(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
        tangent: &mut Array<Real>,
    ) -> Result<()> {
        MaterialIgfemSawToothDamage::damaged_tangent(
            &mut self.saw.elastic,
            &self.saw.damage,
            ctx,
            ty,
            ghost,
            tangent,
        )
    }

    fn compute_all_stresses(&mut self, ctx: &LawContext<'_>, ghost: GhostType) -> Result<()> {
        self.saw.all_stresses(ctx, ghost)
    }

    fn elemental_stiffness_custom(
        &mut self,
        ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Option<Result<Vec<(ElementType, Vec<Idx>, Array<Real>, MatrixType)>>> {
        Some(self.saw.stiffness_blocks(ctx, ghost))
    }
}

impl IgfemMaterial for MaterialIgfemIterativeStiffnessReduction {
    fn igfem_base(&self) -> &MaterialIgfemElastic {
        &self.saw.elastic
    }
    fn igfem_base_mut(&mut self) -> &mut MaterialIgfemElastic {
        &mut self.saw.elastic
    }
    fn internals_to_transfer(&self) -> &'static [&'static str] {
        &["damage", "Sc", "ultimate_strain", "tangent"]
    }
    fn read_sub_material_parameters(
        &mut self,
        param: &dyn Fn(&str) -> Result<ParamValue>,
    ) -> Result<()> {
        self.saw.read_sub_material_parameters(param)?;
        self.crack_band_width = real_param(param, "crack_band_width")?;
        self.max_reductions = uint_param(param, "max_reductions")?;
        self.reduction_constant = real_param(param, "reduction_constant")?;
        self.gf = real_param(param, "Gf")?;
        // upstream reads `E` of the second sub-material in every
        // `updateDamage()`; the port reads it here, i.e. at `init_full` and at
        // each interface update, so a `set_param("E")` on the sub-material
        // between two updates is not seen by `update_damage`
        self.e = real_param(param, "E")?;
        Ok(())
    }
    fn after_transfer(&mut self) -> Result<()> {
        self.recompute_damage_steps()
    }
}

/// Register `igfem_saw_tooth_damage` and
/// `igfem_iterative_stiffness_reduction` (upstream `INSTANTIATE_MATERIAL`).
///
/// # Errors
/// Name already registered.
pub fn register(factory: &mut MaterialFactory, ctx: &Arc<IgfemContext>) -> Result<()> {
    let c = ctx.clone();
    factory.register(
        IGFEM_SAW_TOOTH_DAMAGE,
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            Ok(Box::new(MaterialIgfemSawToothDamage::new(
                spec,
                Some(services.mesh()),
                c.clone(),
            )?) as Box<dyn Material>)
        }),
    )?;
    let c = ctx.clone();
    factory.register(
        IGFEM_ITERATIVE_STIFFNESS_REDUCTION,
        Arc::new(move |spec: &LawSpec, services: &dyn ModelServices| {
            Ok(Box::new(MaterialIgfemIterativeStiffnessReduction::new(
                spec,
                Some(services.mesh()),
                c.clone(),
            )?) as Box<dyn Material>)
        }),
    )
}
