// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_mazars_non_local{.hh,.cc,_tmpl.hh},
// material_von_mises_mazars_non_local.{hh,cc}, material_damage_non_local.hh,
// and the non-local branch of material_mazars_inline_impl.hh
// (computeStressOnQuad with is_non_local, computeDamageAndStressOnQuad).

//! `mazars_non_local` (MAT-014) and `plastic_mazars_non_local` (MAT-016):
//! Mazars damage with a non-local equivalent strain (default) or a
//! non-local damage (`average_on_damage = true`).
//!
//! * **Local pass** (`computeStress`): the local Mazars kernel with
//!   `is_non_local = true` ([`super::super::mazars`]): parent stress,
//!   `∇u −= ε_p` (plastic parent), principal strains, `ε̂` stored in
//!   `Ehat`, the local damage only when `average_on_damage`; the stress is
//!   not damaged and `∇u` stays decremented.
//! * **Averaging**: `Ehat` (or `damage`) → `non_local_variable`.
//! * **Non-local pass** (`computeNonLocalStress`, upstream
//!   `computeDamageAndStressOnQuad` with the averaged variable substituted):
//!   - average on `ε̂`: principal strains recomputed from the stored
//!     `∇u − ε_p` (same embedding as the local pass), damage update with
//!     `ε̂ := ε̂_nl` (`d = min(max(d_new, d), 1)`), `σ *= 1 − d`;
//!   - average on `d`: `σ *= 1 − d_nl`, the law's `damage` internal keeps
//!     the local value;
//!   - then `∇u += ε_p`.
//!
//! Parameter `average_on_damage` (bool, default **false**, parsable and
//! modifiable: upstream `registerParam` default overrides the member
//! default). The local law's `Ehat` internal (UD-067) is the averaged
//! internal.

use akantu_core::tensor::{Matrix, MatrixExt, SquareMatrixExt, as_matrix, as_matrix_mut};
use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec,
};
use akantu_model::non_local::NonLocalVariable;
use akantu_parser::param::{ParamDescriptor, ParamKind, ParamValue, ParameterAccess};

use super::MaterialNonLocal;
use crate::material::{DerivedMaterialParams, Material, MaterialParams};
use crate::materials::damage::DamageParent;
use crate::materials::elastic::MaterialElastic;
use crate::materials::mazars::{
    MaterialMazars, mazars_damage_on_quad, mazars_strain_3d, principal_strains_and_ehat3,
};
use crate::materials::plastic::MaterialLinearIsotropicHardening;

/// Name of the averaged internal (upstream `non_local_variable`).
pub const NON_LOCAL_VARIABLE: &str = "non_local_variable";

/// Upstream `MaterialMazarsNonLocal<dim, Parent>`.
#[derive(Clone, Debug)]
pub struct MaterialMazarsNonLocal<const DIM: usize, P> {
    /// The local Mazars law (with `is_non_local = true`).
    pub inner: MaterialMazars<DIM, P>,
    /// The averaged variable (`non_local_variable`, one component).
    pub non_local_variable: InternalField<Real>,
}

/// `mazars_non_local`.
pub type MaterialMazarsNonLocalElastic<const DIM: usize> =
    MaterialMazarsNonLocal<DIM, MaterialElastic<DIM>>;
/// `plastic_mazars_non_local` (upstream `MaterialVonMisesMazarsNonLocal`).
pub type MaterialVonMisesMazarsNonLocal<const DIM: usize> =
    MaterialMazarsNonLocal<DIM, MaterialLinearIsotropicHardening<DIM>>;

impl<const DIM: usize> MaterialMazarsNonLocalElastic<DIM> {
    /// New `mazars_non_local` law.
    ///
    /// # Errors
    /// Law construction errors.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut inner = MaterialMazars::<DIM, MaterialElastic<DIM>>::new(spec, mesh)?;
        inner.parent.base_mut().law_type = "mazars_non_local".into();
        Self::wrap(inner)
    }
}

impl<const DIM: usize> MaterialVonMisesMazarsNonLocal<DIM> {
    /// New `plastic_mazars_non_local` law.
    ///
    /// # Errors
    /// Law construction errors.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut inner =
            MaterialMazars::<DIM, MaterialLinearIsotropicHardening<DIM>>::new(spec, mesh)?;
        inner.parent.base_mut().law_type = "plastic_mazars_non_local".into();
        Self::wrap(inner)
    }
}

impl<const DIM: usize, P: DamageParent<DIM>> MaterialMazarsNonLocal<DIM, P> {
    fn wrap(mut inner: MaterialMazars<DIM, P>) -> Result<Self> {
        inner.parent.base_mut().is_non_local = true;
        // upstream registerParam("average_on_damage", damage_in_compute_stress, false, …)
        inner.damage_in_compute_stress = false;
        Ok(Self {
            inner,
            non_local_variable: InternalField::new(NON_LOCAL_VARIABLE, 1, 0.0)?,
        })
    }

    /// `average_on_damage`.
    #[must_use]
    pub fn average_on_damage(&self) -> bool {
        self.inner.damage_in_compute_stress
    }

    /// Upstream `computeNonLocalStress(type, ghost)` (module docs).
    ///
    /// # Errors
    /// Size inconsistencies, eigen-solver failure.
    pub fn compute_non_local_stress(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let n = DIM * DIM;
        let m = &mut self.inner;
        let el = m.elastic_constants();
        let mode = m.strain_mode();
        let params = m.params;
        let on_damage = m.damage_in_compute_stress;
        let alpha = m.parent.elastic().thermal.alpha;
        let delta_t = m.parent.elastic().delta_t.get(ty, ghost)?.clone();
        let inel = m
            .parent
            .inelastic_strain()
            .map(|f| f.get(ty, ghost).map(|a| akantu_core::Array::clone(&a)))
            .transpose()?;
        let k0 = m.k0.get(ty, ghost)?.clone();
        let nlv = self.non_local_variable.get(ty, ghost)?.clone();
        let mut dam = m.damage.damage.get(ty, ghost)?.clone();
        let base = m.parent.base();
        let mut grad_u = base.grad_u.get(ty, ghost)?.clone();
        let mut sigma = base.sigma_field()?.get(ty, ghost)?.clone();
        let nq = grad_u.len();
        if [sigma.len(), k0.len(), dam.len(), nlv.len(), delta_t.len()]
            .iter()
            .any(|&l| l != nq)
            || inel.as_ref().is_some_and(|a| a.len() != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialMazarsNonLocal::computeNonLocalStress",
                expected: nq,
                got: nlv.len().min(sigma.len()),
            });
        }
        for q in 0..nq {
            let r = q * n..(q + 1) * n;
            let g: &mut Matrix<DIM, DIM> = as_matrix_mut(&mut grad_u.as_mut_slice()[r.clone()])?;
            let p: Option<&Matrix<DIM, DIM>> = inel
                .as_ref()
                .map(|a| as_matrix(&a.as_slice()[r.clone()]))
                .transpose()?;
            let d = if on_damage {
                nlv.as_slice()[q]
            } else {
                let inel_trace = p.map_or(0., SquareMatrixExt::trace);
                let e3 = mazars_strain_3d(g, inel_trace, delta_t.as_slice()[q] * alpha, mode);
                let (fdiag, _) = principal_strains_and_ehat3(&e3)?;
                let dq = &mut dam.as_mut_slice()[q];
                *dq = mazars_damage_on_quad(
                    &params,
                    &el,
                    nlv.as_slice()[q],
                    k0.as_slice()[q],
                    &fdiag,
                    *dq,
                );
                *dq
            };
            let f = 1. - d;
            for s in &mut sigma.as_mut_slice()[r.clone()] {
                *s *= f;
            }
            if let Some(p) = p {
                *g = g.add(p);
            }
        }
        let base = m.parent.base_mut();
        base.grad_u.get_mut(ty, ghost)?.copy_from(&grad_u)?;
        base.sigma_field_mut()?
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        if !on_damage {
            m.damage.damage.get_mut(ty, ghost)?.copy_from(&dam)?;
        }
        Ok(())
    }
}

impl<const DIM: usize, P> MaterialMazarsNonLocal<DIM, P>
where
    P: 'static,
    MaterialMazars<DIM, P>: akantu_parser::ParameterRegistry,
{
    const PARAMS: &'static [ParamDescriptor<Self>] = &[ParamDescriptor::new(
        "average_on_damage",
        ParamKind::Bool,
        ParameterAccess::PARSMOD,
        "Is D the non local variable",
        |s| ParamValue::Bool(s.inner.damage_in_compute_stress),
        Some(|s, v| match v {
            ParamValue::Bool(b) => {
                s.inner.damage_in_compute_stress = b;
                Ok(())
            }
            other => Err(Error::invalid(format!(
                "average_on_damage expects a bool, got {other:?}"
            ))),
        }),
    )];
}

impl<const DIM: usize, P> DerivedMaterialParams for MaterialMazarsNonLocal<DIM, P>
where
    P: 'static,
    MaterialMazars<DIM, P>: akantu_parser::ParameterRegistry + MaterialParams,
{
    type Parent = MaterialMazars<DIM, P>;
    fn parent(&self) -> &Self::Parent {
        &self.inner
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.inner
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        Self::PARAMS
    }
    fn update_derived(&mut self) -> Result<()> {
        akantu_parser::ParameterRegistry::update_internal_parameters(&mut self.inner)
    }
    fn derived_id(&self) -> &str {
        &MaterialParams::params_base(&self.inner).id
    }
}

crate::impl_derived_material_registry!(MaterialMazarsNonLocalElastic<DIM>, const DIM);
crate::impl_derived_material_registry!(MaterialVonMisesMazarsNonLocal<DIM>, const DIM);

impl<const DIM: usize, P> ConstitutiveLaw for MaterialMazarsNonLocal<DIM, P>
where
    P: DamageParent<DIM>,
    MaterialMazars<DIM, P>: Material,
    Self: akantu_parser::Parsable,
{
    fn name(&self) -> &str {
        self.inner.name()
    }
    fn law_id(&self) -> &str {
        self.inner.law_id()
    }
    fn law_type(&self) -> &str {
        self.inner.law_type()
    }
    fn element_filter(&self) -> &ElementTypeMapArray<Idx> {
        self.inner.element_filter()
    }
    fn element_filter_mut(&mut self) -> &mut ElementTypeMapArray<Idx> {
        self.inner.element_filter_mut()
    }
    fn for_each_internal(&self, f: &mut dyn FnMut(&str, InternalFieldRef<'_>)) {
        self.inner.for_each_internal(f);
        f(
            NON_LOCAL_VARIABLE,
            InternalFieldRef::Real(&self.non_local_variable),
        );
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.inner.for_each_internal_mut(f)?;
        f(
            NON_LOCAL_VARIABLE,
            InternalFieldMut::Real(&mut self.non_local_variable),
        )
    }
}

impl<const DIM: usize, P> Material for MaterialMazarsNonLocal<DIM, P>
where
    P: DamageParent<DIM> + 'static,
    MaterialMazars<DIM, P>: Material,
    Self: akantu_parser::Parsable,
{
    super::forward_local_material!(inner);

    /// The local law's `initMaterial`, then the non-local internal sized
    /// like the others (registered last: no effect on the random draws).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.inner.init_material(ctx)?;
        let blocks = self.inner.parent.base().blocks(ctx);
        self.non_local_variable.resize_blocks(&blocks)
    }

    fn compute_stress(
        &mut self,
        ctx: &LawContext<'_>,
        ty: ElementType,
        ghost: GhostType,
    ) -> Result<()> {
        self.inner.compute_stress(ctx, ty, ghost)
    }

    fn as_non_local(&self) -> Option<&dyn MaterialNonLocal> {
        Some(self)
    }

    fn as_non_local_mut(&mut self) -> Option<&mut dyn MaterialNonLocal> {
        Some(self)
    }

    fn as_any(&self) -> Option<&dyn std::any::Any> {
        Some(self)
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn std::any::Any> {
        Some(self)
    }
}

impl<const DIM: usize, P> MaterialNonLocal for MaterialMazarsNonLocal<DIM, P>
where
    P: DamageParent<DIM>,
    MaterialMazars<DIM, P>: Material,
    Self: akantu_parser::Parsable,
{
    fn neighborhood_name(&self) -> String {
        self.inner.name().to_owned()
    }

    /// Upstream `registerNonLocalVariables`: `damage` (if
    /// `average_on_damage`) or `Ehat` → `non_local_variable`.
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        let local = if self.inner.damage_in_compute_stress {
            "damage"
        } else {
            "Ehat"
        };
        vec![NonLocalVariable {
            local: local.into(),
            non_local: NON_LOCAL_VARIABLE.into(),
            nb_component: 1,
        }]
    }

    /// Upstream `MaterialDamageNonLocal::computeNonLocalStresses`: every
    /// non-empty type of the filter (law dimension).
    fn compute_non_local_stresses(
        &mut self,
        _ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<()> {
        for ty in self.inner.parent.base().filter_types(ghost) {
            if ty.kind() != ElementKind::Regular
                || self.inner.element_filter().get(ty, ghost)?.is_empty()
            {
                continue;
            }
            self.compute_non_local_stress(ty, ghost)?;
        }
        Ok(())
    }
}
