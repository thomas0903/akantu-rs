// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): flows from src/model/model_couplers/
// coupler_solid_phasefield.cc:40-60,410-425 (staggered, independent
// solvers), coupler_solid_contact*.cc / cohesive_contact_solvercallback.cc
// (monolithic, shared DOF manager; contact on the facet mesh),
// src/model/common/solver_callback.hh:113-166 (InterceptSolverCallback) and
// python/py_model.cc:155 (solveStep(callback, solver_id)).

//! Staggered and monolithic coupling ownership, and the Python intercept
//! callback delegating to the model without a second mutable borrow, with
//! toy physics.

mod common;

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::enums::MatrixType;
use akantu_core::{ElementType, GhostType, Real, Result};
use akantu_linalg::TermsToAssemble;
use akantu_mesh::SharedMesh;
use akantu_model::coupling::{Monolithic, SolveComponent, Staggered};
use akantu_solver::callback::{Delegate, InterceptCallback};
use akantu_solver::{DofManager, ModelSolver, SolverCallback};
use common::{BarPhysics, Component, bar_mesh, bar_model, static_solver};

/// Phase-field stand-in with its **own** DOF manager: nodal damage solving
/// `(1 + g_n) d_n = g_n` for a driving energy `g` transferred from the solid.
#[derive(Debug)]
struct PhasePhysics {
    driving: Vec<Real>,
}

impl SolverCallback for PhasePhysics {
    fn assemble_matrix(&mut self, dofs: &mut DofManager, id: &str) -> Result<()> {
        dofs.backend_zero(id)?;
        let mut t = TermsToAssemble::new("damage", "damage");
        for (n, g) in self.driving.iter().enumerate() {
            let n = i32::try_from(n).unwrap();
            t.add(n, n, 1.0 + g);
        }
        dofs.assemble_preassembled_matrix(id, &t)
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        let d = dofs.dofs("damage")?.as_slice().to_vec();
        let r: Vec<Real> = self
            .driving
            .iter()
            .zip(&d)
            .map(|(g, d)| g - (1.0 + g) * d)
            .collect();
        dofs.assemble_to_residual("damage", ArrayView::new(&r, 1)?, 1.0)
    }
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        let f = dofs.family_mut("damage")?;
        let s = f.solution.as_slice().to_vec();
        for (d, x) in f.dofs.borrow_mut().as_mut_slice().iter_mut().zip(s) {
            *d += x;
        }
        Ok(())
    }
}

fn phase_model(mesh: &SharedMesh) -> Component<PhasePhysics> {
    let mut dofs = DofManager::new("phase", Communicator::serial());
    dofs.register_nodal_dofs(&mesh.read().unwrap(), "damage", 1)
        .unwrap();
    dofs.new_matrix("K", MatrixType::Symmetric).unwrap();
    let mut solver = ModelSolver::new(dofs);
    solver
        .insert_solver("phase_static", static_solver("phase_static", 1.0))
        .unwrap();
    Component {
        physics: PhasePhysics {
            driving: vec![0.0; 3],
        },
        solver,
    }
}

/// Strain energy per element → nodal driving energy (owned → owned).
fn strain_to_phase(
    solid: &Component<BarPhysics>,
    phase: &mut Component<PhasePhysics>,
) -> Result<()> {
    let w = solid.physics.energies(&solid.solver.dof_manager);
    phase.physics.driving = vec![w[0], w[0] + w[1], w[1]];
    Ok(())
}

/// Nodal damage → element damage of the solid.
fn damage_to_solid(
    phase: &Component<PhasePhysics>,
    solid: &mut Component<BarPhysics>,
) -> Result<()> {
    let d = phase.solver.dof_manager.dofs("damage")?;
    let d = d.as_slice();
    solid.physics.damage = vec![0.5 * (d[0] + d[1]), 0.5 * (d[1] + d[2])];
    Ok(())
}

#[test]
fn staggered_components_keep_independent_solvers() {
    let mesh = bar_mesh();
    let mut c = Staggered {
        first: bar_model(&mesh, "solid_static", 0.1),
        second: phase_model(&mesh),
    };
    // Independent solver ids, time steps and DOF managers.
    assert_eq!(c.first.solver.time_step(Some("solid_static")).unwrap(), 0.1);
    assert_eq!(
        c.second.solver.time_step(Some("phase_static")).unwrap(),
        1.0
    );
    assert!(c.first.solver.time_step(Some("phase_static")).is_err());
    assert!(c.first.solver.dof_manager.dofs("damage").is_err());
    assert!(c.second.solver.dof_manager.dofs("displacement").is_err());
    c.first
        .solver
        .solver_mut(Some("solid_static"))
        .unwrap()
        .time_step = 0.05;

    c.solve(
        Some("solid_static"),
        Some("phase_static"),
        strain_to_phase,
        damage_to_solid,
    )
    .unwrap();
    // Step 1 (no damage yet): two springs k = 4 in series, F = 1.
    let u = c
        .first
        .solver
        .dof_manager
        .dofs("displacement")
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(u, [0.0, 0.25, 0.5]);
    let w = 0.5 * 4.0 * 0.25f64.powi(2);
    let g = [w, 2.0 * w, w];
    let d: Vec<f64> = g.iter().map(|g| g / (1.0 + g)).collect();
    assert_eq!(
        c.second
            .solver
            .dof_manager
            .dofs("damage")
            .unwrap()
            .as_slice(),
        d.as_slice()
    );
    assert_eq!(c.first.physics.damage, vec![0.5 * (d[0] + d[1]); 2]);

    // Step 2 uses the transferred damage.
    c.solve(
        Some("solid_static"),
        Some("phase_static"),
        strain_to_phase,
        damage_to_solid,
    )
    .unwrap();
    let k_prev = 4.0 * (1.0 - 0.5 * (d[0] + d[1])).powi(2);
    let u2 = c
        .first
        .solver
        .dof_manager
        .dofs("displacement")
        .unwrap()
        .as_slice()[2];
    assert!(
        (u2 - 2.0 / k_prev).abs() < 1e-12,
        "second solid step solved with step-1 damage"
    );
}

/// Contact part living on the **facet mesh**: a penalty spring on the
/// facet node at x = 2, assembled into the shared displacement family.
#[derive(Debug)]
struct FacetPenalty {
    mesh: SharedMesh,
    k: Real,
    hooks: Vec<&'static str>,
}

impl FacetPenalty {
    fn nodes(&self) -> Vec<i32> {
        let m = self.mesh.read().unwrap();
        let f = m.mesh_facets().unwrap();
        let c = f
            .connectivity(ElementType::Point1, GhostType::NotGhost)
            .unwrap();
        c.as_slice()
            .iter()
            .copied()
            .filter(|&n| f.nodes().as_slice()[n as usize] == 2.0)
            .collect()
    }
}

impl SolverCallback for FacetPenalty {
    fn assemble_matrix(&mut self, dofs: &mut DofManager, id: &str) -> Result<()> {
        self.hooks.push("contact:K");
        let mut t = TermsToAssemble::new("displacement", "displacement");
        for n in self.nodes() {
            t.add(n, n, self.k);
        }
        dofs.assemble_preassembled_matrix(id, &t)
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.hooks.push("contact:r");
        let u = dofs.dofs("displacement")?.as_slice().to_vec();
        let mut r = vec![0.0; u.len()];
        for n in self.nodes() {
            r[n as usize] = -self.k * u[n as usize];
        }
        dofs.assemble_to_residual("displacement", ArrayView::new(&r, 1)?, 1.0)
    }
}

/// Combined physics of a monolithic coupler (upstream coupler overrides).
#[derive(Debug)]
struct SolidContact {
    solid: BarPhysics,
    contact: FacetPenalty,
}

impl SolverCallback for SolidContact {
    fn assemble_matrix(&mut self, dofs: &mut DofManager, id: &str) -> Result<()> {
        self.solid.assemble_matrix(dofs, id)?;
        self.contact.assemble_matrix(dofs, id)
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.solid.assemble_residual(dofs)?;
        self.contact.assemble_residual(dofs)
    }
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.solid.corrector(dofs)
    }
}

#[test]
fn monolithic_coupler_shares_one_dof_manager() {
    let mesh = bar_mesh();
    let solid = BarPhysics::new(mesh.clone(), 4.0, 1.0);
    let mut dofs = DofManager::new("coupler", Communicator::serial());
    solid.register(&mut dofs).unwrap();
    let mut solver = ModelSolver::new(dofs);
    solver
        .insert_solver("static", static_solver("static", 1.0))
        .unwrap();
    let contact = FacetPenalty {
        mesh: mesh.clone(),
        k: 2.0,
        hooks: vec![],
    };
    let mut c = Monolithic {
        solver,
        physics: SolidContact { solid, contact },
    };
    c.solve_step(Some("static")).unwrap();
    // Bar (series stiffness 2) in parallel with the contact spring 2.
    let u = c
        .solver
        .dof_manager
        .dofs("displacement")
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(u, [0.0, 0.125, 0.25]);
    assert_eq!(c.physics.contact.hooks, ["contact:K", "contact:r"]);
    assert_eq!(
        c.solver.dof_manager.dof_ids(),
        ["displacement"],
        "one family, one manager"
    );
}

/// Python `InterceptSolverCallback` subclass stand-in: adds a load after
/// calling `super().assembleResidual()`.
struct ExtraLoad {
    extra: Real,
    log: Vec<&'static str>,
}

impl InterceptCallback for ExtraLoad {
    fn assemble_residual(&mut self, model: &mut Delegate<'_>) -> Result<()> {
        self.log.push("user:before-super");
        model.assemble_residual()?; // super(): the model's own callback
        self.log.push("user:after-super");
        let extra = [0.0, 0.0, self.extra];
        model
            .dofs()
            .assemble_to_residual("displacement", ArrayView::new(&extra, 1)?, 1.0)
    }
}

#[test]
fn intercept_callback_delegates_without_reborrowing_the_model() {
    let mesh = bar_mesh();
    let mut model = bar_model(&mesh, "static", 1.0);
    let mut user = ExtraLoad {
        extra: 0.5,
        log: vec![],
    };
    // Disjoint borrows: the solver drives `Intercept { user, physics }`.
    model
        .solver
        .solve_step_intercepted(&mut model.physics, &mut user, None)
        .unwrap();
    let u = model
        .solver
        .dof_manager
        .dofs("displacement")
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(u, [0.0, 0.375, 0.75]);
    assert_eq!(user.log, ["user:before-super", "user:after-super"]);
    assert_eq!(
        model.physics.hooks,
        [
            "before",
            "assemble_matrix:K",
            "assemble_residual",
            "after:true"
        ],
        "lifecycle hooks reach the model exactly once"
    );
}
