// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/model_couplers/
// coupler_solid_phasefield.cc:40-60 (independent solid and phase-field
// models, no shared DOF manager), :410-425 (solve: solid->solveStep(solid_id),
// strain transfer, phase->solveStep(phase_id), damage transfer),
// coupler_solid_contact*.cc and cohesive_contact_solvercallback.cc
// (shared displacement DOF manager, combined callback).

//! Coupling contracts (D-021 revised).
//!
//! Two coupling styles exist upstream and both are first-class here:
//!
//! * [`Staggered`] — **independently owned component models**, each with
//!   its own `ModelSolver` (own DOF manager, own named time-step solvers
//!   and time steps). The coupler owns the components by value and solves
//!   them in sequence with explicit field transfers between them
//!   (solid–phase-field: `solve_step(solid_id)` → strain transfer →
//!   `solve_step(phase_id)` → damage transfer). Transfers receive `&A` and
//!   `&mut B` — disjoint borrows of the coupler's fields.
//! * [`Monolithic`] — **one** `ModelSolver` (shared DOF manager) and a
//!   physics value `P: SolverCallback` that combines several physics parts
//!   (solid–contact, solid–cohesive–contact where the contact part lives on
//!   the facet mesh but assembles into the same displacement family). Part
//!   physics own no DOF manager; their fields are read and written through
//!   the coupler's DOF manager, received as a callback parameter.
//!
//! Component handles: `coupler.first` / `coupler.second` (Python
//! `getSolidMechanicsModel()`, `getPhaseFieldModel()` return borrowed
//! handles valid while the coupler lives; for a monolithic coupler the
//! handle pairs the part physics with the shared solver).

use akantu_core::Result;
use akantu_solver::{ModelSolver, SolverCallback};

/// Something that can advance one step with one of its own solvers.
pub trait SolveComponent {
    /// Solve one step with solver `solver_id` (`None` = default).
    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()>;
}

/// Staggered coupling of two independently solved components.
#[derive(Debug)]
pub struct Staggered<A, B> {
    /// First component (e.g. the solid model).
    pub first: A,
    /// Second component (e.g. the phase-field model).
    pub second: B,
}

impl<A: SolveComponent, B: SolveComponent> Staggered<A, B> {
    /// One staggered step: `first.solve_step(first_id)`, transfer to the
    /// second component, `second.solve_step(second_id)`, transfer back
    /// (upstream `CouplerSolidPhaseField::solve`).
    pub fn solve(
        &mut self,
        first_id: Option<&str>,
        second_id: Option<&str>,
        mut to_second: impl FnMut(&A, &mut B) -> Result<()>,
        mut to_first: impl FnMut(&B, &mut A) -> Result<()>,
    ) -> Result<()> {
        self.first.solve_step(first_id)?;
        to_second(&self.first, &mut self.second)?;
        self.second.solve_step(second_id)?;
        to_first(&self.second, &mut self.first)
    }
}

/// Monolithic coupling: one solver (shared DOF manager) driving a combined
/// physics callback.
#[derive(Debug)]
pub struct Monolithic<P> {
    /// Shared solver.
    pub solver: ModelSolver,
    /// Combined physics (a struct of parts implementing [`SolverCallback`]).
    pub physics: P,
}

impl<P: SolverCallback> SolveComponent for Monolithic<P> {
    fn solve_step(&mut self, solver_id: Option<&str>) -> Result<()> {
        self.solver.solve_step(&mut self.physics, solver_id)
    }
}
