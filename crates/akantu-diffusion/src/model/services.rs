// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/diffusion_model/diffusion_model.hh
// (getters used by the laws through `getHandler()`: getDiffusion,
// getFEEngine, getSpatialDimension, …).

//! The read-only model services lent to the diffusion laws for one phase
//! (ARCHITECTURE): the model splits itself into `(&mut laws,
//! &services)`. The DOF arrays are read from the DOF manager (D-021), the
//! flows from the model.

use akantu_core::array_shared::{ArrayRef, SharedArray};
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::ModelServices;
use akantu_model::model::FeEngineRegistry;
use akantu_solver::DofManager;

/// The `<dof_name>` arrays of the DOF manager: `(values, rate)`.
pub(crate) fn dof_arrays<'a>(
    dofs: &'a DofManager,
    dof_name: &str,
) -> (Option<&'a SharedArray<Real>>, Option<&'a SharedArray<Real>>) {
    match dofs.family(dof_name) {
        Ok(f) => (Some(&f.dofs), f.derivatives.get(&1)),
        Err(_) => (None, None),
    }
}

/// Phase-scoped services of a diffusion model.
#[derive(Debug)]
pub(crate) struct DiffusionServices<'a> {
    pub mesh: &'a Mesh,
    pub fems: &'a FeEngineRegistry,
    /// `<dof_name>` values (DOF manager), when registered.
    pub diffusion: Option<&'a SharedArray<Real>>,
    /// `<dof_name>_rate` (DOF manager), when registered.
    pub diffusion_rate: Option<&'a SharedArray<Real>>,
    pub dof_name: &'a str,
    pub external_flow: Option<&'a SharedArray<Real>>,
    pub internal_flow: Option<&'a SharedArray<Real>>,
    pub spatial_dimension: usize,
    pub time_step: Real,
}

impl ModelServices for DiffusionServices<'_> {
    /// The executor of the model's FE engines (D-123): the heat law's
    /// points are independent.
    fn exec(&self) -> &akantu_core::exec::Exec {
        self.fems.exec()
    }

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

    /// `<dof_name>` (the diffusion field), `<dof_name>_rate`,
    /// `external_flow`, `internal_flow`.
    fn nodal_field(&self, name: &str) -> Result<ArrayRef<'_, Real>> {
        let missing = || {
            Error::not_found(
                "nodal field",
                format!("{name} (not allocated: initialise the model first)"),
            )
        };
        let r = if name == self.dof_name {
            self.diffusion
        } else if name.strip_suffix("_rate") == Some(self.dof_name) {
            self.diffusion_rate
        } else {
            match name {
                "external_flow" => self.external_flow,
                "internal_flow" => self.internal_flow,
                _ => return Err(Error::not_found("nodal field", name)),
            }
        };
        r.ok_or_else(missing)?.read()
    }

    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        self.fems.get(name)
    }
}
