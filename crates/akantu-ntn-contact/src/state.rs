// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/ntn_contact/ntn_base_contact.cc (the `SolidMechanicsModel` getters the
// NTN classes call: getMass, getVelocity, getAcceleration, getIncrement,
// getCurrentPosition, getInternalForce, getExternalForce, getTimeStep,
// getF_M2A).

//! Read-only view of the solid mechanics state used by the contact and the
//! friction objects.
//!
//! Upstream the NTN classes keep a reference to the `SolidMechanicsModel` and
//! call its getters whenever they need a nodal field. In Rust the model is
//! borrowed by the solver during a step, so the contact and friction methods
//! receive a [`ModelState`] built either from the model
//! ([`ModelState::of_model`]) or, inside an intercepting solver callback,
//! from the physics and the DOF manager ([`ModelState::of_parts`]).

use std::borrow::Cow;

use akantu_core::array_shared::{ArrayRef, SliceRef};
use akantu_core::error::if_present;
use akantu_core::{Error, Real, Result, SharedArray};
use akantu_mesh::Mesh;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::{DOF_ID, SolidMechanicsPhysics};
use akantu_solver::DofManager;

/// Nodal fields and scalars of the solid mechanics model at one instant.
/// The nodal arrays are guards of the model's exportable arrays (D-047,
/// `docs/ARCHITECTURE.md`): a state must not be held across a
/// callback into foreign code.
#[derive(Debug)]
pub struct ModelState<'a> {
    /// The mesh.
    pub mesh: &'a Mesh,
    /// Spatial dimension of the model.
    pub spatial_dimension: usize,
    /// Time step of the default solver (upstream `getTimeStep()`).
    pub time_step: Real,
    /// Upstream `getF_M2A()` (force-to-acceleration factor, 1 by default).
    pub f_m2a: Real,
    /// Displacement `u` (`nb_nodes × dim`).
    pub displacement: SliceRef<'a, Real>,
    /// Velocity (empty when the analysis is static).
    pub velocity: SliceRef<'a, Real>,
    /// Acceleration (empty when the analysis is static).
    pub acceleration: SliceRef<'a, Real>,
    /// Displacement increment of the step (empty if not allocated).
    pub increment: SliceRef<'a, Real>,
    /// Lumped mass (empty if not assembled).
    pub mass: SliceRef<'a, Real>,
    /// Internal force (stored with the residual sign convention).
    pub internal_force: SliceRef<'a, Real>,
    /// External force.
    pub external_force: SliceRef<'a, Real>,
    /// Blocked DOF flags (empty if not allocated).
    pub blocked_dofs: SliceRef<'a, bool>,
    /// The model's current position `X + u` as cached by its displacement
    /// release (upstream `getCurrentPosition()`); `None`: computed from the
    /// displacement ([`ModelState::current_position`]).
    pub current_position: Option<SliceRef<'a, Real>>,
}

fn derivative(dofs: &DofManager, order: usize) -> Result<SliceRef<'_, Real>> {
    match dofs
        .family(DOF_ID)
        .ok()
        .and_then(|f| f.derivatives.get(&order))
    {
        Some(a) => Ok(a.read()?.into_slice()),
        None => Ok(SliceRef::empty()),
    }
}

/// The values of an optional exportable array, empty if absent.
fn optional<T>(a: Option<&SharedArray<T>>) -> Result<SliceRef<'_, T>> {
    match a {
        Some(a) => Ok(a.read()?.into_slice()),
        None => Ok(SliceRef::empty()),
    }
}

/// The values of a model array, empty if not allocated (`NotFound`).
fn allocated(r: Result<ArrayRef<'_, Real>>) -> Result<SliceRef<'_, Real>> {
    Ok(if_present(r)?.map_or_else(SliceRef::empty, ArrayRef::into_slice))
}

impl<'a> ModelState<'a> {
    /// State from the physics and DOF manager of a running step (see
    /// `akantu_solver::callback::Delegate::split_as`). The model's current
    /// position is refreshed first (upstream `getCurrentPosition()`:
    /// recomputed only when the displacement release changed).
    ///
    /// # Errors
    /// The displacement family does not exist (model not initialised), or
    /// the refresh of an allocated current position fails (wrong sizes).
    pub fn of_parts(
        mesh: &'a Mesh,
        physics: &'a mut SolidMechanicsPhysics,
        dofs: &'a DofManager,
    ) -> Result<Self> {
        // Upstream `getCurrentPosition()`. Only a current position that is
        // not allocated yet (before `init_full`) falls back to `X + u`
        // computed on demand; every other refresh error is propagated.
        let refreshed = if physics.cached_current_position().is_ok() {
            physics.refresh_current_position(dofs)?;
            true
        } else {
            false
        };
        let physics: &'a SolidMechanicsPhysics = physics;
        let family = dofs.family(DOF_ID).map_err(|_| {
            Error::invalid("ModelState: the model has no displacement (call init_full first)")
        })?;
        Ok(Self {
            mesh,
            spatial_dimension: physics.base.spatial_dimension,
            time_step: physics.time_step(),
            f_m2a: physics.f_m2a,
            displacement: family.dofs.read()?.into_slice(),
            velocity: derivative(dofs, 1)?,
            acceleration: derivative(dofs, 2)?,
            increment: optional(family.increment.as_ref())?,
            mass: allocated(physics.mass())?,
            internal_force: allocated(physics.internal_force())?,
            external_force: allocated(physics.external_force())?,
            blocked_dofs: optional(family.blocked.as_ref())?,
            current_position: if refreshed {
                if_present(physics.cached_current_position())?.map(ArrayRef::into_slice)
            } else {
                None
            },
        })
    }

    /// State of a model outside a solve step (`mesh` is the model's mesh,
    /// read-locked by the caller).
    ///
    /// # Errors
    /// See [`ModelState::of_parts`].
    pub fn of_model(mesh: &'a Mesh, model: &'a mut SolidMechanicsModel) -> Result<Self> {
        Self::of_parts(mesh, &mut model.physics, &model.solver.dof_manager)
    }

    /// Number of nodes.
    #[must_use]
    pub fn nb_nodes(&self) -> usize {
        self.mesh.nb_nodes()
    }

    /// Current positions (upstream `getCurrentPosition()`, which returns a
    /// reference): the model's cached array, borrowed, when available,
    /// otherwise `X + u` (the same single addition per component as
    /// upstream's update).
    ///
    /// # Errors
    /// The displacement does not have `nb_nodes × dim` values.
    pub fn current_position(&self) -> Result<Cow<'_, [Real]>> {
        let x = self.mesh.nodes();
        let x = x.as_slice();
        if let Some(cp) = &self.current_position {
            check_len("current position", cp, x.len())?;
            return Ok(Cow::Borrowed(cp));
        }
        check_len(
            "current position: displacement",
            &self.displacement,
            x.len(),
        )?;
        Ok(Cow::Owned(
            x.iter()
                .zip(self.displacement.iter())
                .map(|(a, b)| a + b)
                .collect(),
        ))
    }

    /// Check that a nodal field has `nb_nodes × dim` values.
    ///
    /// # Errors
    /// Wrong length (e.g. the field is not allocated).
    pub fn check_nodal(&self, what: &'static str, field: &[Real]) -> Result<()> {
        check_len(what, field, self.nb_nodes() * self.spatial_dimension)
    }
}

pub(crate) fn check_len<T>(what: &'static str, field: &[T], expected: usize) -> Result<()> {
    if field.len() == expected {
        Ok(())
    } else {
        Err(Error::DimensionMismatch {
            context: what,
            expected,
            got: field.len(),
        })
    }
}
