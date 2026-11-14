// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phase_field_model.hh
// (getters used by the phase-field laws through `handler`: getDamage,
// getFEEngine, getInternalForce, getMesh).

//! The read-only model services lent to the phase-field laws for one phase
//! (ARCHITECTURE): the laws read the mesh, the FE engine
//! `PhaseFieldFEEngine` and the nodal damage while they are mutably
//! borrowed.

use akantu_core::array_shared::ArrayRef;
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result, SharedArray};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::ModelServices;
use akantu_model::model::FeEngineRegistry;
use akantu_solver::DofManager;

use super::DOF_ID;

/// Phase-scoped services of a phase-field model.
#[derive(Debug)]
pub(crate) struct PfServices<'a> {
    pub mesh: &'a Mesh,
    pub fems: &'a FeEngineRegistry,
    pub damage: Option<&'a SharedArray<Real>>,
    pub previous_damage: Option<&'a SharedArray<Real>>,
    pub external_force: Option<&'a Array<Real>>,
    pub internal_force: Option<&'a SharedArray<Real>>,
    pub spatial_dimension: usize,
    pub time_step: Real,
}

impl<'a> PfServices<'a> {
    /// Services reading the DOF family `damage` of `dofs`.
    pub(crate) fn new(
        mesh: &'a Mesh,
        fems: &'a FeEngineRegistry,
        dofs: &'a DofManager,
        external_force: Option<&'a Array<Real>>,
        internal_force: Option<&'a SharedArray<Real>>,
        spatial_dimension: usize,
        time_step: Real,
    ) -> Self {
        let family = dofs.family(DOF_ID).ok();
        Self {
            mesh,
            fems,
            damage: family.map(|f| &f.dofs),
            previous_damage: family.and_then(|f| f.previous.as_ref()),
            external_force,
            internal_force,
            spatial_dimension,
            time_step,
        }
    }
}

impl ModelServices for PfServices<'_> {
    fn spatial_dimension(&self) -> usize {
        self.spatial_dimension
    }

    fn mesh(&self) -> &Mesh {
        self.mesh
    }

    fn time_step(&self) -> Real {
        self.time_step
    }

    fn nb_integration_points(&self, ty: ElementType, ghost: GhostType) -> usize {
        self.fems
            .get("")
            .and_then(|fe| fe.nb_integration_points(ty, ghost))
            .unwrap_or(0)
    }

    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        let fe = self.fems.get("")?;
        fe.interpolate_on_integration_points(
            self.mesh,
            self.mesh.nodes().view(),
            out,
            self.mesh.spatial_dimension(),
            ty,
            ghost,
            Some(filter),
        )
    }

    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
        let missing = || {
            Error::not_found(
                "nodal field",
                format!("{name} (not allocated: initialise the model first)"),
            )
        };
        // the DOF family's arrays and the internal force are exportable
        // (guards, D-047); the external force is plain
        match name {
            "damage" => self.damage.ok_or_else(missing)?.read(),
            "previous_damage" => self.previous_damage.ok_or_else(missing)?.read(),
            "external_force" => Ok(self.external_force.ok_or_else(missing)?.into()),
            "internal_force" => self.internal_force.ok_or_else(missing)?.read(),
            _ => Err(Error::not_found("nodal field", name)),
        }
    }

    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        self.fems.get(name)
    }
}
