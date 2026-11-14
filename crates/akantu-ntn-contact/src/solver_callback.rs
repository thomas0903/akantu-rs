// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/{ntn_contact_solvercallback.hh, ntn_contact_solvercallback.cc}.

//! Solver callback adding the contact and friction forces to the residual of
//! an explicit step (upstream `NTNContactSolverCallback`, TASN-009).
//!
//! It intercepts the solid mechanics model's callback: on every residual
//! assembly it accumulates the work of the previous contact/friction forces
//! over the step increment, lets the model assemble its residual, computes
//! the contact pressure, the slip and the friction traction, and assembles
//! both global force arrays into the residual. Before/after a step it saves
//! (restores on non-convergence) the global force arrays.

use akantu_core::enums::{AnalysisMethod, SynchronizationTag};
use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;
use akantu_model::ModelOptions;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::model::{DOF_ID, SolidMechanicsPhysics};
use akantu_solver::callback::{Delegate, InterceptCallback};

use crate::TasnUpstreamCompat;
use crate::contact::{NtnBaseContact, dot};
use crate::factory::initialize_ntn_friction;
use crate::friction::NtnFriction;
use crate::state::ModelState;
use crate::sync_array::SynchronizedArray;

/// Upstream `NTNContactSolverCallback`.
#[derive(Debug)]
pub struct NtnContactSolverCallback {
    contact: NtnBaseContact,
    friction: NtnFriction,
    external_work: [Real; 2],
    external_work_release: i64,
}

fn physics_of<'a, 'b>(
    model: &'a mut Delegate<'b>,
) -> Result<(
    &'a mut SolidMechanicsPhysics,
    &'a mut akantu_solver::DofManager,
)> {
    model.split_as::<SolidMechanicsPhysics>().ok_or_else(|| {
        Error::invalid("NTNContactSolverCallback intercepts a SolidMechanicsModel only")
    })
}

impl NtnContactSolverCallback {
    /// Upstream constructor `NTNContactSolverCallback(solid, slave, master,
    /// surface_normal_dir, time_step_factor)`: a node-to-node contact
    /// (`contact`), `initFull(_explicit_lumped_mass)`, lumped mass, surface
    /// pair, normals, lumped boundaries, time step = stable time step ×
    /// `time_step_factor`, impedance, and the friction of the first
    /// `friction` section of the model's input file.
    ///
    /// # Errors
    /// Any of these steps fails.
    pub fn new(
        solid: &mut SolidMechanicsModel,
        slave: &str,
        master: &str,
        surface_normal_dir: usize,
        time_step_factor: Real,
        compat: TasnUpstreamCompat,
    ) -> Result<Self> {
        let mesh_h = solid.physics.base.mesh.clone();
        let mut contact = {
            let mesh = mesh_h.read()?;
            NtnBaseContact::new_ntn(&mesh, solid.spatial_dimension(), "contact", compat)?
        };
        solid.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))?;
        solid.assemble_mass_lumped()?;
        {
            let mesh = mesh_h.read()?;
            let state = ModelState::of_model(&mesh, &mut *solid)?;
            contact.add_surface_pair(&state, slave, master, surface_normal_dir)?;
            contact.update_normals(&state)?;
            contact.update_lumped_boundary(&mesh)?;
        }
        let stable_time_step = solid.stable_time_step()?;
        let time_step = stable_time_step * time_step_factor;
        solid.set_time_step(time_step, None)?;
        let friction = {
            let mesh = mesh_h.read()?;
            let state = ModelState::of_model(&mesh, &mut *solid)?;
            contact.update_impedance(&state)?;
            // the state's guards borrow the model: end them before the
            // parser is read (explicit guard `Drop`)
            drop(state);
            initialize_ntn_friction(&contact, mesh.nb_nodes(), &solid.physics.base.parser)?
        };
        Ok(Self {
            contact,
            friction,
            external_work: [0.0, 0.0],
            external_work_release: -1,
        })
    }

    /// Callback around an existing contact and friction (no upstream
    /// counterpart; the contact must be set up for `model`).
    #[must_use]
    pub fn from_parts(contact: NtnBaseContact, friction: NtnFriction) -> Self {
        Self {
            contact,
            friction,
            external_work: [0.0, 0.0],
            external_work_release: -1,
        }
    }

    /// The contact (upstream `getContact`).
    #[must_use]
    pub fn contact(&self) -> &NtnBaseContact {
        &self.contact
    }

    /// Mutable contact.
    pub fn contact_mut(&mut self) -> &mut NtnBaseContact {
        &mut self.contact
    }

    /// The friction (upstream `getFriction`).
    #[must_use]
    pub fn friction(&self) -> &NtnFriction {
        &self.friction
    }

    /// Mutable friction.
    pub fn friction_mut(&mut self) -> &mut NtnFriction {
        &mut self.friction
    }

    /// Contact and friction, both mutable.
    pub fn parts_mut(&mut self) -> (&mut NtnBaseContact, &mut NtnFriction) {
        (&mut self.contact, &mut self.friction)
    }

    /// Upstream `getContactPressure()`.
    #[must_use]
    pub fn contact_pressure(&self) -> &SynchronizedArray<Real> {
        self.contact.contact_pressure()
    }

    /// Upstream `getNbContactNodes()`.
    #[must_use]
    pub fn nb_contact_nodes(&self) -> usize {
        self.contact.nb_contact_nodes()
    }

    /// Accumulated work `[contact, friction]` of the contact and friction
    /// forces on this rank (upstream's `external_work` member; on one rank
    /// upstream `getExternalWork()`, see
    /// [`NtnContactSolverCallback::global_external_work`]).
    #[must_use]
    pub fn external_work(&self) -> [Real; 2] {
        self.external_work
    }

    /// Upstream `getExternalWork()`: the work summed over the ranks of a
    /// distributed mesh (`allReduce(_sum)`, rank-ordered here, D-039;
    /// collective there); [`NtnContactSolverCallback::external_work`] is
    /// this rank's part.
    ///
    /// # Errors
    /// Communication errors.
    pub fn global_external_work(&self, model: &SolidMechanicsModel) -> Result<[Real; 2]> {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let mut w = self.external_work;
        crate::parallel::sum_reals(&mesh, Ok(()), &mut w, "external_work")?;
        Ok(w)
    }

    fn work_increment(&self, mesh: &Mesh, increment: &[Real]) -> Result<[Real; 2]> {
        let dim = self.contact.spatial_dimension();
        let f_cont = self.contact.global_contact_pressure().as_slice();
        let f_fric = self.friction.global_friction_traction().as_slice();
        let nb = mesh.nb_nodes();
        crate::state::check_len("getExternalWorkIncrement: increment", increment, nb * dim)?;
        crate::state::check_len("getExternalWorkIncrement: contact force", f_cont, nb * dim)?;
        crate::state::check_len("getExternalWorkIncrement: friction force", f_fric, nb * dim)?;
        let mut work = [0.0, 0.0];
        for n in 0..nb {
            let idx = akantu_core::Idx::try_from(n).map_err(|_| Error::InvalidIndex {
                what: "node",
                value: n as i64,
            })?;
            let counted = mesh.is_local_or_master_node(idx)? && !mesh.is_periodic_slave(idx)?;
            let count_node = if counted { 1.0 } else { 0.0 };
            let inc = &increment[n * dim..(n + 1) * dim];
            work[0] -= count_node * dot(&f_cont[n * dim..(n + 1) * dim], inc);
            work[1] -= count_node * dot(&f_fric[n * dim..(n + 1) * dim], inc);
        }
        Ok(work)
    }

    /// Upstream `getExternalWorkIncrement()`: `−Σ f·Δu` of the current global
    /// contact and friction forces over the current increment.
    ///
    /// # Errors
    /// Increment not allocated or inconsistent sizes.
    pub fn external_work_increment(&self, model: &SolidMechanicsModel) -> Result<[Real; 2]> {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let increment = model.increment()?;
        self.work_increment(&mesh, increment.as_slice())
    }

    /// One explicit step of `model` with the contact (upstream
    /// `model.solveStep(callback, solver_id)`).
    ///
    /// # Errors
    /// Solver or contact failure.
    pub fn solve_step(
        &mut self,
        model: &mut SolidMechanicsModel,
        solver_id: Option<&str>,
    ) -> Result<()> {
        model.solve_step_intercepted(self, solver_id)
    }
}

impl InterceptCallback for NtnContactSolverCallback {
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        {
            let (physics, dofs) = physics_of(model)?;
            let release = physics.displacement_release();
            if self.external_work_release != release {
                let mesh_h = physics.base.mesh.clone();
                let mesh = mesh_h.read()?;
                let increment = dofs
                    .family(DOF_ID)
                    .ok()
                    .and_then(|f| f.increment.as_ref())
                    .ok_or_else(|| Error::invalid("NTN contact: no displacement increment"))?;
                let w = self.work_increment(&mesh, increment.read()?.as_slice())?;
                self.external_work[0] += w[0];
                self.external_work[1] += w[1];
                self.external_work_release = release;
            }
        }
        // the solid mechanics residual
        model.assemble_residual()?;
        let (physics, dofs) = physics_of(model)?;
        let mesh_h = physics.base.mesh.clone();
        // upstream `computeContactPressure` starts with
        // `synchronize(_cf_nodal)`, `updateSlip` with `synchronize(_cf_incr)`
        // (nothing on one rank, refused on a distributed mesh
        // without init_parallel: UD-904)
        self.contact
            .synchronize_nodal_in(SynchronizationTag::CfNodal, physics, dofs)?;
        let mesh = mesh_h.read()?;
        {
            let state = ModelState::of_parts(&mesh, physics, dofs)?;
            self.contact.compute_contact_pressure(&state)?;
        }
        drop(mesh);
        self.contact
            .synchronize_nodal_in(SynchronizationTag::CfIncr, physics, dofs)?;
        let mesh = mesh_h.read()?;
        {
            let state = ModelState::of_parts(&mesh, physics, dofs)?;
            self.friction.update_slip(&self.contact, &state)?;
            self.friction
                .compute_friction_traction(&self.contact, &state)?;
        }
        self.contact.assemble_global_contact_pressure(&mesh)?;
        self.friction
            .assemble_global_friction_traction(&self.contact, &mesh)?;
        dofs.assemble_to_residual(DOF_ID, self.contact.global_contact_pressure().view(), 1.0)?;
        dofs.assemble_to_residual(DOF_ID, self.friction.global_friction_traction().view(), 1.0)?;
        Ok(())
    }

    fn before_solve_step(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        model.before_solve_step()?;
        self.friction.save_previous_state();
        self.contact.save_previous_state();
        Ok(())
    }

    fn after_solve_step(&mut self, model: &mut Delegate<'_>, converged: bool) -> Result<()> {
        model.after_solve_step(converged)?;
        if !converged {
            self.friction.restore_previous_state();
            self.contact.restore_previous_state();
        }
        Ok(())
    }
}
