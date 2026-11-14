// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/
// phase_field_element_filter.hh.

//! [`PhaseFieldElementFilter`] (PF-011): the clustering filter of upstream
//! `GroupManager::createClusters` that keeps the elements that are not
//! completely broken (used by the phase-field notch example to count
//! fragments).

use std::collections::HashSet;

use akantu_core::{Element, Error, GhostType, Real, Result, TypeFilter};

use crate::model::PhaseFieldModel;

/// Upstream `PhaseFieldElementFilter(model, max_damage = 1)`: an element
/// passes when at least one of its quadrature points has a damage (the
/// law's `damage` internal, as last computed) strictly below `max_damage`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PhaseFieldElementFilter {
    /// Damage threshold (`IsUnbrokenFunctor`: `x < max_damage`).
    pub max_damage: Real,
}

impl Default for PhaseFieldElementFilter {
    fn default() -> Self {
        Self { max_damage: 1.0 }
    }
}

impl PhaseFieldElementFilter {
    /// Filter with the given threshold.
    #[must_use]
    pub fn new(max_damage: Real) -> Self {
        Self { max_damage }
    }

    /// Upstream `operator()(element)`.
    ///
    /// # Errors
    /// An element without a phase field, or whose quadrature points are
    /// outside the law's `damage` block (upstream debug assertion "This
    /// quadrature point is out of range").
    pub fn is_unbroken(&self, model: &PhaseFieldModel, element: Element) -> Result<bool> {
        let law_index = model.phase_field_by_element().element_ref(element)?[0];
        let law_index = akantu_core::decode_idx(law_index)?
            .ok_or_else(|| Error::invalid(format!("{element} has no phase field")))?;
        let local =
            akantu_core::try_to_usize(model.phase_field_local_numbering().element(element)?[0])?;
        let law = model.phase_field(law_index)?;
        let nq = model
            .fe_engine()?
            .nb_integration_points(element.ty(), element.ghost_type())?;
        let damage = law.base().damage.get(element.ty(), element.ghost_type())?;
        let start = nq * local;
        let values = damage
            .as_slice()
            .get(start..start + nq)
            .ok_or(Error::OutOfRange {
                what: "PhaseFieldElementFilter: quadrature point",
                index: start,
                len: damage.len(),
            })?;
        Ok(values.iter().any(|&x| x < self.max_damage))
    }

    /// The unbroken not-ghost elements of the model dimension, to build the
    /// predicate of [`akantu_mesh::clusters::create_clusters`]
    /// (`|e| set.contains(&e)`) before mutating the mesh's groups.
    pub fn unbroken_elements(&self, model: &PhaseFieldModel) -> Result<HashSet<Element>> {
        let mesh_h = model.mesh_handle().clone();
        let mesh = mesh_h.read()?;
        let mut out = HashSet::new();
        let filter = TypeFilter {
            spatial_dimension: Some(model.spatial_dimension()),
            ghost_type: GhostType::NotGhost,
            kind: Some(akantu_core::ElementKind::Regular),
        };
        for e in mesh.elements(filter) {
            if self.is_unbroken(model, e)? {
                out.insert(e);
            }
        }
        Ok(out)
    }
}
