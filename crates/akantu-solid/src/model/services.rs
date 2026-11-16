// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/
// solid_mechanics_model.hh (getters used by materials through
// `getModel()`: getDisplacement, getFEEngine, getTimeStep, …).

//! The read-only model services lent to the materials for one phase
//! (ARCHITECTURE): the model splits itself into
//! `(&mut laws, &services)` so a law reads the model while it is itself
//! mutably borrowed. The DOF arrays are read from the DOF manager (D-021),
//! the other nodal fields from the model.

use akantu_core::array::ArrayViewMut;
use akantu_core::array_shared::{ArrayRef, SharedArray};
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_fe::FeEngine;
use akantu_mesh::Mesh;
use akantu_model::constitutive_law::ModelServices;
use akantu_model::model::FeEngineRegistry;
use akantu_solver::DofManager;

use super::DOF_ID;

/// Nodal arrays of the DOF family `displacement` (references into the DOF
/// manager), when registered.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct DofArrays<'a> {
    pub displacement: Option<&'a SharedArray<Real>>,
    pub velocity: Option<&'a SharedArray<Real>>,
    pub acceleration: Option<&'a SharedArray<Real>>,
    pub increment: Option<&'a SharedArray<Real>>,
    pub previous: Option<&'a SharedArray<Real>>,
}

impl<'a> DofArrays<'a> {
    pub(crate) fn of(dofs: &'a DofManager) -> Self {
        Self::of_family(dofs.family(DOF_ID).ok())
    }

    /// The arrays of a DOF family's storage (`None`: not registered).
    pub(crate) fn of_family(f: Option<&'a akantu_solver::dof_manager::DofData>) -> Self {
        let Some(f) = f else {
            return Self::default();
        };
        Self {
            displacement: Some(&f.dofs),
            velocity: f.derivatives.get(&1),
            acceleration: f.derivatives.get(&2),
            increment: f.increment.as_ref(),
            previous: f.previous.as_ref(),
        }
    }
}

/// Model-owned nodal arrays.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct OwnArrays<'a> {
    pub external_force: Option<&'a SharedArray<Real>>,
    pub internal_force: Option<&'a SharedArray<Real>>,
    pub mass: Option<&'a SharedArray<Real>>,
    pub current_position: Option<&'a SharedArray<Real>>,
}

/// Phase-scoped services of a solid mechanics model.
#[derive(Debug)]
pub(crate) struct SmmServices<'a> {
    pub mesh: &'a Mesh,
    pub fems: &'a FeEngineRegistry,
    /// Point sources of other element kinds (the IGFEM model's).
    pub extra_points: &'a [super::SharedPointSource],
    pub dofs: DofArrays<'a>,
    pub own: OwnArrays<'a>,
    pub spatial_dimension: usize,
    pub time_step: Real,
}

impl ModelServices for SmmServices<'_> {
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
        if let Some(src) = self.extra_points.iter().find(|s| s.kind() == ty.kind()) {
            return src.nb_integration_points(ty, ghost).unwrap_or(0);
        }
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
        let r = match name {
            "displacement" => self.dofs.displacement,
            "velocity" => self.dofs.velocity,
            "acceleration" => self.dofs.acceleration,
            "increment" | "displacement_increment" => self.dofs.increment,
            "previous_displacement" => self.dofs.previous,
            "external_force" => self.own.external_force,
            "internal_force" => self.own.internal_force,
            "mass" => self.own.mass,
            "current_position" => self.own.current_position,
            _ => return Err(Error::not_found("nodal field", name)),
        };
        r.ok_or_else(missing)?.read()
    }

    fn fe_engine(&self, name: &str) -> Result<&FeEngine> {
        self.fems.get(name)
    }

    /// The model's FE engine writes the gradient straight into the view
    /// (no temporary and no copy per step); same reads
    /// and errors in the same order as the trait's
    /// `gradient_on_integration_points`.
    fn gradient_on_integration_points_view(
        &self,
        field: &str,
        out: ArrayViewMut<'_, Real>,
        nb_component: usize,
        ty: ElementType,
        ghost: GhostType,
        filter: &[Idx],
    ) -> Result<()> {
        let u = self.nodal_field(field)?;
        self.fe_engine("")?.gradient_on_integration_points_view(
            self.mesh,
            u.view(),
            out,
            nb_component,
            ty,
            ghost,
            Some(filter),
        )
    }
}
