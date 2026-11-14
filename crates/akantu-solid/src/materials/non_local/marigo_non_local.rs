// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/materials/
// material_damage/material_marigo_non_local.{hh,cc},
// material_damage_non_local.hh, and the non-local branch of
// material_marigo_inline_impl.hh (computeStressOnQuad with is_non_local,
// computeDamageAndStressOnQuad).

//! `marigo_non_local` (MAT-012): Marigo damage with a non-local energy
//! release rate.
//!
//! * **Local pass** (`computeStress`): the local Marigo kernel with
//!   `is_non_local = true` ([`crate::materials::marigo`]): elastic stress
//!   and `Y` (with `damage_in_y` / `Yc limit`), no damage update.
//! * **Averaging**: `Y` → `Y non local`.
//! * **Non-local pass** (`computeNonLocalStress`, upstream
//!   `computeDamageAndStressOnQuad` with `Y := Y non local`):
//!   `F = Y_nl − Yd − Sd·d_prev`; if `F > 0`: `d = (Y_nl − Yd)/Sd`;
//!   `d = std::max(d, d_prev)`, `d = std::min(d, 1)`; `σ *= 1 − d`.
//!
//! Upstream registers the local law under this keyword (UD-027, see
//! [`super`]); this type is upstream's otherwise unreachable
//! `MaterialMarigoNonLocal<dim>`.

use akantu_core::tensor::{Matrix, as_matrix_mut};
use akantu_core::{
    ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx, Real, Result,
};
use akantu_model::InternalField;
use akantu_model::constitutive_law::{
    ConstitutiveLaw, InternalFieldMut, InternalFieldRef, LawContext, LawSpec,
};
use akantu_model::non_local::NonLocalVariable;
use akantu_parser::param::ParamDescriptor;

use super::MaterialNonLocal;
use crate::material::{DerivedMaterialParams, Material, MaterialParams};
use crate::materials::marigo::MaterialMarigo;

/// Name of the averaged internal (upstream `Ynl`).
pub const Y_NON_LOCAL: &str = "Y non local";

/// Upstream `MaterialMarigoNonLocal<dim>`.
#[derive(Clone, Debug)]
pub struct MaterialMarigoNonLocal<const DIM: usize> {
    /// The local Marigo law (with `is_non_local = true`).
    pub inner: MaterialMarigo<DIM>,
    /// `Y non local`.
    pub y_non_local: InternalField<Real>,
}

impl<const DIM: usize> MaterialMarigoNonLocal<DIM> {
    /// New law (upstream constructor: `Y non local` registered,
    /// `is_non_local = true`).
    ///
    /// # Errors
    /// Law construction errors.
    pub fn new(spec: &LawSpec, mesh: Option<&akantu_mesh::Mesh>) -> Result<Self> {
        let mut inner = MaterialMarigo::<DIM>::new(spec, mesh)?;
        inner.elastic.base_mut().law_type = "marigo_non_local".into();
        inner.elastic.base_mut().is_non_local = true;
        Ok(Self {
            inner,
            y_non_local: InternalField::new(Y_NON_LOCAL, 1, 0.0)?,
        })
    }

    /// Upstream `computeNonLocalStress(type, ghost)` (module docs).
    ///
    /// # Errors
    /// Size inconsistencies.
    pub fn compute_non_local_stress(&mut self, ty: ElementType, ghost: GhostType) -> Result<()> {
        let n = DIM * DIM;
        let ynl = self.y_non_local.get(ty, ghost)?.clone();
        let yd = self.inner.yd.get(ty, ghost)?.clone();
        let mut sigma = self
            .inner
            .elastic
            .base()
            .sigma_field()?
            .get(ty, ghost)?
            .clone();
        let (mut dam, prev_dam) = {
            let (c, p) = self
                .inner
                .damage
                .damage
                .current_and_previous_mut(ty, ghost)?;
            (c.clone(), p.clone())
        };
        let nq = dam.len();
        if [sigma.len(), ynl.len(), yd.len(), prev_dam.len()]
            .iter()
            .any(|&l| l != nq)
        {
            return Err(Error::DimensionMismatch {
                context: "MaterialMarigoNonLocal::computeNonLocalStress",
                expected: nq,
                got: ynl.len().min(sigma.len()),
            });
        }
        for (q, s) in sigma.as_mut_slice().chunks_exact_mut(n).enumerate() {
            let s: &mut Matrix<DIM, DIM> = as_matrix_mut(s)?;
            let d = &mut dam.as_mut_slice()[q];
            *d = self.inner.damage_and_stress_on_quad(
                s,
                *d,
                prev_dam.as_slice()[q],
                ynl.as_slice()[q],
                yd.as_slice()[q],
            );
        }
        self.inner
            .elastic
            .base_mut()
            .sigma_field_mut()?
            .get_mut(ty, ghost)?
            .copy_from(&sigma)?;
        self.inner
            .damage
            .damage
            .get_mut(ty, ghost)?
            .copy_from(&dam)?;
        Ok(())
    }
}

impl<const DIM: usize> DerivedMaterialParams for MaterialMarigoNonLocal<DIM> {
    type Parent = MaterialMarigo<DIM>;
    fn parent(&self) -> &Self::Parent {
        &self.inner
    }
    fn parent_mut(&mut self) -> &mut Self::Parent {
        &mut self.inner
    }
    fn derived_params(&self) -> &'static [ParamDescriptor<Self>] {
        &[]
    }
    fn update_derived(&mut self) -> Result<()> {
        akantu_parser::ParameterRegistry::update_internal_parameters(&mut self.inner)
    }
    fn derived_id(&self) -> &str {
        &MaterialParams::params_base(&self.inner).id
    }
}

crate::impl_derived_material_registry!(MaterialMarigoNonLocal<DIM>, const DIM);

impl<const DIM: usize> ConstitutiveLaw for MaterialMarigoNonLocal<DIM> {
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
        f(Y_NON_LOCAL, InternalFieldRef::Real(&self.y_non_local));
    }
    fn for_each_internal_mut(
        &mut self,
        f: &mut dyn FnMut(&str, InternalFieldMut<'_>) -> Result<()>,
    ) -> Result<()> {
        self.inner.for_each_internal_mut(f)?;
        f(Y_NON_LOCAL, InternalFieldMut::Real(&mut self.y_non_local))
    }
}

impl<const DIM: usize> Material for MaterialMarigoNonLocal<DIM> {
    super::forward_local_material!(inner);

    /// The local law's `initMaterial`, then `Y non local` sized like the
    /// others (registered after `Yd`: no effect on the random draws).
    fn init_material(&mut self, ctx: &LawContext<'_>) -> Result<()> {
        self.inner.init_material(ctx)?;
        let blocks = self.inner.elastic.base().blocks(ctx);
        self.y_non_local.resize_blocks(&blocks)
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

impl<const DIM: usize> MaterialNonLocal for MaterialMarigoNonLocal<DIM> {
    fn neighborhood_name(&self) -> String {
        self.inner.name().to_owned()
    }

    /// Upstream `registerNonLocalVariables`: `Y` → `Y non local`.
    fn non_local_variables(&self) -> Vec<NonLocalVariable> {
        vec![NonLocalVariable {
            local: "Y".into(),
            non_local: Y_NON_LOCAL.into(),
            nb_component: 1,
        }]
    }

    fn compute_non_local_stresses(
        &mut self,
        _ctx: &LawContext<'_>,
        ghost: GhostType,
    ) -> Result<()> {
        for ty in self.inner.elastic.base().filter_types(ghost) {
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
