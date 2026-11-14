// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test support (contract demonstrations, not parity code): a toy
// linear non-linear-solver and toy 1D physics used to exercise the solver
// ownership contracts.
#![allow(dead_code)] // each test binary uses a subset of the helpers

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::enums::{MatrixType, NonLinearSolverType, TimeStepSolverType};
use akantu_core::{Array, ElementType, GhostType, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::coupling::SolveComponent;
use akantu_parser::param::{ParamValue, ParameterRegistry};
use akantu_solver::{DofManager, ModelSolver, NonLinearSolver, SolverCallback, TimeStepSolver};

/// One linear step with a dense solve (serial only): before → predictor →
/// K → residual → Dirichlet → solve → split → corrector → after.
#[derive(Debug, Default)]
pub struct ToyLinear {
    pub calls: Vec<&'static str>,
}

impl ParameterRegistry for ToyLinear {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        Err(akantu_core::Error::not_found("parameter", name))
    }
    fn set_param(&mut self, name: &str, _: ParamValue) -> Result<()> {
        Err(akantu_core::Error::not_found("parameter", name))
    }
    fn param_names(&self) -> Vec<&'static str> {
        vec![]
    }
}

fn dense_solve(mut a: Vec<f64>, mut b: Vec<f64>, n: usize) -> Vec<f64> {
    for k in 0..n {
        for i in k + 1..n {
            let f = a[i + k * n] / a[k + k * n];
            for j in k..n {
                a[i + j * n] -= f * a[k + j * n];
            }
            b[i] -= f * b[k];
        }
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = b[i];
        for j in i + 1..n {
            s -= a[i + j * n] * x[j];
        }
        x[i] = s / a[i + i * n];
    }
    x
}

impl NonLinearSolver for ToyLinear {
    fn kind(&self) -> NonLinearSolverType {
        NonLinearSolverType::Linear
    }
    fn solve(&mut self, dofs: &mut DofManager, cb: &mut dyn SolverCallback) -> Result<()> {
        cb.before_solve_step(dofs)?;
        cb.predictor(dofs)?;
        cb.assemble_matrix(dofs, "K")?;
        dofs.finalize_matrix("K")?;
        dofs.zero_residual();
        cb.assemble_residual(dofs)?;
        dofs.apply_boundary("K")?;
        let n = dofs.local_system_size();
        let layout = dofs.layout().clone();
        let mut a = vec![0.0; n * n];
        let k = dofs.matrix("K")?;
        let sym = k.matrix_type() == MatrixType::Symmetric;
        let (irn, jcn, v) = k.coo();
        for ((&gi, &gj), &x) in irn.iter().zip(jcn).zip(v) {
            let (i, j) = (layout.local(gi)?, layout.local(gj)?);
            a[i + j * n] += x;
            if sym && i != j {
                a[j + i * n] += x;
            }
        }
        let blocked = dofs.blocked_equations();
        let b: Vec<f64> = dofs
            .residual()
            .values()
            .iter()
            .zip(&blocked)
            .map(|(&r, &bl)| if bl { 0.0 } else { r })
            .collect();
        let x = dense_solve(a, b, n);
        dofs.split_solution_per_dofs(&x)?;
        cb.corrector(dofs)?;
        cb.after_solve_step(dofs, true)?;
        self.calls.push("solve");
        Ok(())
    }
    fn nb_iterations(&self) -> usize {
        1
    }
    fn error(&self) -> f64 {
        0.0
    }
    fn converged(&self) -> bool {
        true
    }
}

pub fn static_solver(id: &str, dt: Real) -> TimeStepSolver {
    let mut tss = TimeStepSolver::new(
        id,
        TimeStepSolverType::Static,
        Box::new(ToyLinear::default()),
    )
    .unwrap();
    tss.time_step = dt;
    tss
}

/// 1D bar n0 - n1 - n2 (unit segments) with a point facet mesh.
pub fn bar_mesh() -> SharedMesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    let mut f = Mesh::new(1, "bar:facets").unwrap();
    {
        let mut a = MeshAccessor::new(&mut f);
        for (i, x) in [0.0, 1.0, 2.0].into_iter().enumerate() {
            a.push_node(&[x]).unwrap();
            a.connectivity_mut(ElementType::Point1, GhostType::NotGhost)
                .push_tuple(&[i32::try_from(i).unwrap()]);
        }
    }
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [0.0, 1.0, 2.0] {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
        drop(c);
        a.set_mesh_facets(f);
    }
    SharedMesh::new(m)
}

/// Solid part: bar of modulus `e`, element stiffness `e (1 - d_e)^2`,
/// node 0 fixed, force `force` at node 2. Owns no DOF manager.
#[derive(Debug)]
pub struct BarPhysics {
    pub mesh: SharedMesh,
    pub e: Real,
    pub force: Real,
    pub damage: Vec<Real>,
    pub hooks: Vec<String>,
}

impl BarPhysics {
    pub fn new(mesh: SharedMesh, e: Real, force: Real) -> Self {
        Self {
            mesh,
            e,
            force,
            damage: vec![0.0; 2],
            hooks: vec![],
        }
    }
    pub fn register(&self, dofs: &mut DofManager) -> Result<()> {
        dofs.register_nodal_dofs(&*self.mesh.read()?, "displacement", 1)?;
        dofs.register_blocked_dofs("displacement")?;
        dofs.family_mut("displacement")?
            .blocked
            .as_mut()
            .unwrap()
            .borrow_mut()
            .as_mut_slice()[0] = true;
        dofs.new_matrix("K", MatrixType::Symmetric)
    }
    fn ke(&self) -> Array<f64> {
        let mut k = Array::new(4);
        for d in &self.damage {
            let s = self.e * (1.0 - d) * (1.0 - d);
            k.push_tuple(&[s, -s, -s, s]);
        }
        k
    }
    /// Strain energy density per element from the current displacement.
    pub fn energies(&self, dofs: &DofManager) -> Vec<Real> {
        let u = dofs.dofs("displacement").unwrap();
        let u = u.as_slice();
        (0..2)
            .map(|e| 0.5 * self.e * (u[e + 1] - u[e]).powi(2))
            .collect()
    }
}

impl SolverCallback for BarPhysics {
    fn assemble_matrix(&mut self, dofs: &mut DofManager, id: &str) -> Result<()> {
        self.hooks.push(format!("assemble_matrix:{id}"));
        let mesh = self.mesh.read()?;
        dofs.backend_zero(id)?;
        dofs.assemble_elemental_matrices_to_matrix(
            &mesh,
            id,
            "displacement",
            self.ke().view(),
            ElementType::Segment2,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
    }
    fn assemble_residual(&mut self, dofs: &mut DofManager) -> Result<()> {
        self.hooks.push("assemble_residual".into());
        // r = f_ext - K u (internal forces of the current state).
        let u = dofs.dofs("displacement")?.as_slice().to_vec();
        let mut f = vec![0.0, 0.0, self.force];
        for (e, d) in self.damage.iter().enumerate() {
            let s = self.e * (1.0 - d) * (1.0 - d) * (u[e + 1] - u[e]);
            f[e] += s;
            f[e + 1] -= s;
        }
        dofs.assemble_to_residual("displacement", ArrayView::new(&f, 1)?, 1.0)
    }
    fn corrector(&mut self, dofs: &mut DofManager) -> Result<()> {
        let f = dofs.family_mut("displacement")?;
        let sol = f.solution.as_slice().to_vec();
        for (u, s) in f.dofs.borrow_mut().as_mut_slice().iter_mut().zip(sol) {
            *u += s;
        }
        Ok(())
    }
    fn before_solve_step(&mut self, _: &mut DofManager) -> Result<()> {
        self.hooks.push("before".into());
        Ok(())
    }
    fn after_solve_step(&mut self, _: &mut DofManager, converged: bool) -> Result<()> {
        self.hooks.push(format!("after:{converged}"));
        Ok(())
    }
}

/// A complete component model: physics + its own solver (D-021 split).
#[derive(Debug)]
pub struct Component<P> {
    pub physics: P,
    pub solver: ModelSolver,
}

impl<P: SolverCallback> SolveComponent for Component<P> {
    fn solve_step(&mut self, id: Option<&str>) -> Result<()> {
        self.solver.solve_step(&mut self.physics, id)
    }
}

pub fn bar_model(mesh: &SharedMesh, solver_id: &str, dt: Real) -> Component<BarPhysics> {
    let physics = BarPhysics::new(mesh.clone(), 4.0, 1.0);
    let mut dofs = DofManager::new("solid", Communicator::serial());
    physics.register(&mut dofs).unwrap();
    let mut solver = ModelSolver::new(dofs);
    solver
        .insert_solver(solver_id, static_solver(solver_id, dt))
        .unwrap();
    Component { physics, solver }
}
