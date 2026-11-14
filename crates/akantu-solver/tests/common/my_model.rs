// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS):
// test/test_model/test_common/test_model_solver/test_model_solver_my_model.hh
// (MyModel), test_model_solver.cc / test_model_solver_dynamic.cc (genMesh),
// and the probe-defined non-linear bar of
// oracle/probes/solver_model/solver_model_probe.cc. Test support only.

//! The upstream toy model of `test_model_solver` (a 1-D bar `=\o---o---o-> F`
//! with `E = A = ρ = 1`), statement by statement, so its histories can be
//! compared bit for bit with the oracle (`fixtures/solver_model/*`).
//!
//! The model keeps its own arrays (forces, internal forces, strains,
//! stresses, initial lengths); the DOF family `"disp"` (with derivatives 1
//! and 2 and blocked flags) lives in the DOF manager (D-021). On a
//! distributed bar (local world) internal forces and lumped masses are
//! completed at shared nodes by a slave → master sum followed by a master →
//! slave copy through the running collective scope (the toy's stand-in for
//! upstream's ghost elements and `_user_1` stress synchronisation).

use akantu_core::enums::{MatrixType, SynchronizationTag};
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::{DofManager, SolverCallback};

pub const SEG: ElementType = ElementType::Segment2;

/// `genMesh` of the upstream tests: `nb_nodes` nodes at `n · (1 / (N − 1))`,
/// segments `[n, n + 1]`.
pub fn gen_mesh(nb_nodes: usize) -> Mesh {
    let mut mesh = Mesh::new(1, "bar").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for n in 0..nb_nodes {
            acc.push_node(&[n as f64 * (1.0 / (nb_nodes - 1) as f64)])
                .unwrap();
        }
        let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
        for n in 0..nb_nodes - 1 {
            c.push_tuple(&[n as Idx, n as Idx + 1]);
        }
    }
    mesh
}

/// Stiffness law of the bar.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Law {
    /// Upstream MyModel: `σ = E ε`, `K` assembled once.
    Linear,
    /// Probe-defined: `σ = E (ε + ε³)`, tangent `EA/L (1 + 3ε²)` re-assembled
    /// at every request.
    Cubic,
}

/// Upstream `MyModel` (+ the probe's trace and non-linear variant).
pub struct MyModel {
    pub mesh: Mesh,
    /// Global id of each local node (serial: identity).
    pub global_ids: Vec<i64>,
    pub nb_global_nodes: i64,
    pub nb_dofs: usize,
    pub lumped: bool,
    pub law: Law,
    pub e: Real,
    pub a: Real,
    pub rho: Real,
    pub forces: Vec<Real>,
    pub internal_forces: Vec<Real>,
    pub stresses: Vec<Real>,
    pub strains: Vec<Real>,
    pub initial_lengths: Vec<Real>,
    is_stiffness_assembled: bool,
    is_mass_assembled: bool,
    is_lumped_mass_assembled: bool,
    pub trace: Vec<String>,
    pub tracing: bool,
    /// Injected failure per callback name (rank-specific tests).
    pub fail_at: Option<String>,
    /// Rayleigh damping `C = a M + b K` of the probe's `DampedBar`
    /// (`(a, b)`; `None`: upstream MyModel, `C` not defined).
    pub rayleigh: Option<(Real, Real)>,
    is_damping_assembled: bool,
    /// `true`: the oracle probe's non-linear bar on a distributed mesh
    /// exactly (oracle case
    /// `solver_model/dbltrace_newton_raphson_np2`):
    /// `NonLinearBar::assembleResidual` (`solver_model_probe.cc`) takes the
    /// internal forces of the rank's own elements only, without the
    /// `synchronize(_user_1)` by which upstream's `MyModel` adds the ghost
    /// elements' forces at the shared nodes
    /// (`test_model_solver_my_model.hh:101-103, 260-269, 393-409`).
    /// Upstream's residual gather takes the owned rows
    /// (`dof_synchronizer.cc:121-139`), so the shared node's row then misses
    /// the other rank's element and the probe's bar solves another system
    /// than the serial bar. Default `false`: the completed forces (the
    /// toy's stand-in for upstream `MyModel`'s ghost-element exchange,
    /// module docs).
    pub upstream_partial_forces: bool,
}

impl MyModel {
    /// Upstream constructor: registers `disp` (+ derivatives 1, 2, blocked
    /// flags), force `F` on the last global node, node 0 blocked, initial
    /// lengths `|x2 − x1|`.
    pub fn new(f: Real, mesh: Mesh, lumped: bool, dm: &mut DofManager) -> Result<Self> {
        let nb_dofs = mesh.nb_nodes();
        let global_ids = (0..nb_dofs as Idx)
            .map(|n| mesh.node_global_id(n))
            .collect();
        Self::with_global_ids(f, mesh, lumped, dm, global_ids, nb_dofs as i64)
    }

    /// Constructor for a distributed bar (explicit global node ids).
    pub fn with_global_ids(
        f: Real,
        mesh: Mesh,
        lumped: bool,
        dm: &mut DofManager,
        global_ids: Vec<i64>,
        nb_global_nodes: i64,
    ) -> Result<Self> {
        let nb_dofs = mesh.nb_nodes();
        let nb_el = mesh.nb_element(SEG, GhostType::NotGhost);
        dm.register_nodal_dofs(&mesh, "disp", 1)?;
        dm.register_dofs_derivative("disp", 1)?;
        dm.register_dofs_derivative("disp", 2)?;
        dm.register_blocked_dofs("disp")?;
        let mut forces = vec![0.0; nb_dofs];
        for n in 0..nb_dofs {
            if global_ids[n] == nb_global_nodes - 1 {
                forces[n] = f;
            }
            if global_ids[n] == 0 {
                dm.blocked_dofs_mut("disp")?.as_mut_slice()[n] = true;
            }
        }
        let mut initial_lengths = Vec::with_capacity(nb_el);
        {
            let c = mesh.connectivity(SEG, GhostType::NotGhost)?;
            let x = mesh.nodes();
            let x = x.as_slice();
            for e in 0..nb_el {
                let t = c.tuple(e);
                let p1 = x[t[0] as usize];
                let p2 = x[t[1] as usize];
                initial_lengths.push((p2 - p1).abs());
            }
        }
        Ok(Self {
            mesh,
            global_ids,
            nb_global_nodes,
            nb_dofs,
            lumped,
            law: Law::Linear,
            e: 1.0,
            a: 1.0,
            rho: 1.0,
            forces,
            internal_forces: vec![0.0; nb_dofs],
            stresses: vec![0.0; nb_el],
            strains: vec![0.0; nb_el],
            initial_lengths,
            is_stiffness_assembled: false,
            is_mass_assembled: false,
            is_lumped_mass_assembled: false,
            trace: Vec::new(),
            tracing: true,
            fail_at: None,
            rayleigh: None,
            is_damping_assembled: false,
            upstream_partial_forces: false,
        })
    }

    fn log(&mut self, s: &str) -> Result<()> {
        if self.tracing {
            self.trace.push(s.to_owned());
        }
        if self.fail_at.as_deref() == Some(s) {
            return Err(Error::Numerical(format!("injected failure in {s}")));
        }
        Ok(())
    }

    /// The trace as the probe writes it (`name\n` per callback).
    pub fn trace_text(&self) -> String {
        self.trace.iter().map(|t| format!("{t}\n")).collect()
    }

    fn connectivity(&self) -> Vec<[usize; 2]> {
        let c = self.mesh.connectivity(SEG, GhostType::NotGhost).unwrap();
        (0..c.len())
            .map(|e| {
                let t = c.tuple(e);
                [t[0] as usize, t[1] as usize]
            })
            .collect()
    }

    fn is_local_or_master(&self, n: usize) -> bool {
        self.mesh.node_flag(n as Idx).is_local_or_master()
    }

    /// Slave → master sum, then master → slave copy of a nodal array
    /// (distributed bars only; through the running collective scope).
    fn complete_shared(&self, dm: &mut DofManager, v: &mut [Real]) -> Result<()> {
        let Some(sync) = self.mesh.synchronizers().node.clone() else {
            return Ok(());
        };
        let mut ph = dm.phases()?;
        sync.slave_reduction_in(
            &mut ph,
            &mut NodalArray { v, add: true },
            SynchronizationTag::User1,
        )?;
        sync.synchronize_in(
            &mut ph,
            &mut NodalArray { v, add: false },
            SynchronizationTag::User1,
        )
    }

    /// Upstream `assembleLumpedMass`.
    pub fn assemble_lumped_mass(&mut self, dm: &mut DofManager) -> Result<()> {
        dm.lumped_matrix_mut("M")?.zero();
        let mut m = Array::try_zeros(self.nb_dofs, 1)?;
        let mut m_all_el = Array::new(2);
        let x = self.mesh.nodes().as_slice().to_vec();
        for [n1, n2] in self.connectivity() {
            let l = (x[n2] - x[n1]).abs();
            let m_n = self.rho * self.a * l / 2.0;
            m_all_el.push_tuple(&[m_n, m_n]);
        }
        DofManager::assemble_elemental_array_local_array(
            &self.mesh,
            m_all_el.view(),
            &mut m,
            SEG,
            GhostType::NotGhost,
            1.0,
            None,
        )?;
        self.complete_shared(dm, m.as_mut_slice())?;
        dm.assemble_to_lumped_matrix("disp", m.view(), "M", 1.0)?;
        self.is_lumped_mass_assembled = true;
        Ok(())
    }

    /// Upstream `assembleMass`: `m = ρAL/6 [[2, 1], [1, 2]]`.
    pub fn assemble_mass(&mut self, dm: &mut DofManager) -> Result<()> {
        dm.zero_matrix("M")?;
        let mut m_all_el = Array::new(4);
        let x = self.mesh.nodes().as_slice().to_vec();
        for [n1, n2] in self.connectivity() {
            let l = (x[n2] - x[n1]).abs();
            let s = self.rho * self.a * l / 6.0;
            m_all_el.push_tuple(&[2.0 * s, s, s, 2.0 * s]); // 1·s exact
        }
        dm.assemble_elemental_matrices_to_matrix(
            &self.mesh,
            "M",
            "disp",
            m_all_el.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )?;
        self.is_mass_assembled = true;
        Ok(())
    }

    /// Upstream `assembleStiffness` (linear) / the probe's tangent (cubic).
    pub fn assemble_stiffness(&mut self, dm: &mut DofManager) -> Result<()> {
        dm.zero_matrix("K")?;
        let mut k_all_el = Array::new(4);
        let x = self.mesh.nodes().as_slice().to_vec();
        let u = dm.dofs("disp")?.as_slice().to_vec();
        for (e, [n1, n2]) in self.connectivity().into_iter().enumerate() {
            let k = match self.law {
                Law::Linear => {
                    let l = (x[n2] - x[n1]).abs();
                    self.e * self.a / l
                }
                Law::Cubic => {
                    let l = self.initial_lengths[e];
                    let strain = (u[n2] - u[n1]) / l;
                    self.e * self.a / l * (1.0 + 3.0 * strain * strain)
                }
            };
            // upstream `k_el = k; k_el *= EA/L` with k = [[1, -1], [-1, 1]] (±1·x exact)
            k_all_el.push_tuple(&[k, -k, -k, k]);
        }
        dm.assemble_elemental_matrices_to_matrix(
            &self.mesh,
            "K",
            "disp",
            k_all_el.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )?;
        self.is_stiffness_assembled = true;
        Ok(())
    }

    /// The probe's `DampedBar::assembleMatrix("C")` (solver_model probe v3):
    /// `c = a m + b k` entry by entry with `m = {2, 1} (ρAL/6)` and
    /// `k = {1, −1} (EA/L)`, in the probe's operation order.
    pub fn assemble_damping(&mut self, dm: &mut DofManager, a: Real, b: Real) -> Result<()> {
        dm.zero_matrix("C")?;
        let mut c_all_el = Array::new(4);
        let x = self.mesh.nodes().as_slice().to_vec();
        for [n1, n2] in self.connectivity() {
            let l = (x[n2] - x[n1]).abs();
            let s = self.rho * self.a * l / 6.0;
            let m_d = 2.0 * s;
            let m_o = 1.0 * s;
            let k = self.e * self.a / l;
            let k_o = -k; // upstream `-1. * k`: bitwise equal
            let d = a * m_d + b * k;
            let o = a * m_o + b * k_o;
            c_all_el.push_tuple(&[d, o, o, d]);
        }
        dm.assemble_elemental_matrices_to_matrix(
            &self.mesh,
            "C",
            "disp",
            c_all_el.view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )?;
        self.is_damping_assembled = true;
        Ok(())
    }

    /// Upstream `assembleResidualInternal(_not_ghost)` (law-dependent).
    fn assemble_residual_internal(&mut self, dm: &DofManager) -> Result<()> {
        let u = dm.dofs("disp")?;
        let u = u.as_slice();
        let mut f_el = Array::new(2);
        for (e, [n1, n2]) in self.connectivity().into_iter().enumerate() {
            let l = self.initial_lengths[e];
            let strain = (u[n2] - u[n1]) / l;
            let stress = match self.law {
                Law::Linear => self.e * strain,
                Law::Cubic => self.e * (strain + strain * strain * strain),
            };
            self.strains[e] = strain;
            self.stresses[e] = stress;
            let f_n = self.a * stress;
            f_el.push_tuple(&[-f_n, f_n]);
        }
        let mut fi = Array::from_vec(std::mem::take(&mut self.internal_forces), 1)?;
        let r = DofManager::assemble_elemental_array_local_array(
            &self.mesh,
            f_el.view(),
            &mut fi,
            SEG,
            GhostType::NotGhost,
            1.0,
            None,
        );
        self.internal_forces = fi.as_slice().to_vec();
        r
    }

    /// Upstream `getPotentialEnergy`.
    pub fn potential_energy(&self, dm: &mut DofManager) -> Result<Real> {
        let mut res = 0.0;
        if !self.lumped {
            let u = dm.dofs("disp")?.as_slice().to_vec();
            res = self.mul_vect_mat_vect(dm, &u, "K", &u)?;
        } else {
            for ((strain, stress), l) in self
                .strains
                .iter()
                .zip(&self.stresses)
                .zip(&self.initial_lengths)
            {
                res += strain * stress * self.a * l;
            }
            res = all_reduce_sum(dm, res)?;
        }
        Ok(res / 2.0)
    }

    /// Upstream `getKineticEnergy`.
    pub fn kinetic_energy(&self, dm: &mut DofManager) -> Result<Real> {
        let mut res = 0.0;
        let v = dm.dofs_derivative("disp", 1)?.as_slice().to_vec();
        if !self.lumped {
            res = self.mul_vect_mat_vect(dm, &v, "M", &v)?;
        } else {
            let m = dm.lumped_matrix("M")?.values.clone();
            for (node, (vi, mi)) in v.iter().zip(&m).enumerate() {
                if self.is_local_or_master(node) {
                    res += mi * vi * vi;
                }
            }
            res = all_reduce_sum(dm, res)?;
        }
        Ok(res / 2.0)
    }

    /// Upstream `getExternalWorkIncrement`.
    pub fn external_work_increment(&self, dm: &mut DofManager, time_step: Real) -> Result<Real> {
        let mut res = 0.0;
        let v = dm.dofs_derivative("disp", 1)?.as_slice().to_vec();
        let b = dm.blocked_dofs("disp")?.as_slice().to_vec();
        for node in 0..v.len() {
            if self.is_local_or_master(node) {
                res += (if b[node] {
                    -self.internal_forces[node]
                } else {
                    self.forces[node]
                }) * v[node];
            }
        }
        res = all_reduce_sum(dm, res)?;
        Ok(res * time_step)
    }

    /// Upstream `mulVectMatVect`: `Σ x · (A y)` over local-or-master nodes.
    pub fn mul_vect_mat_vect(
        &self,
        dm: &mut DofManager,
        x: &[Real],
        a_id: &str,
        y: &[Real],
    ) -> Result<Real> {
        let mut ay = Array::try_zeros(self.nb_dofs, 1)?;
        dm.assemble_mat_mul_vect_to_array("disp", a_id, y, &mut ay, 1.0)?;
        let mut res = 0.0;
        for (n, (yi, xi)) in ay.as_slice().iter().zip(x).enumerate() {
            res += xi * yi * if self.is_local_or_master(n) { 1.0 } else { 0.0 };
        }
        all_reduce_sum(dm, res)
    }
}

/// `allReduce(_sum)` of one scalar (a collective of the DOF manager's
/// communicator; serial: identity).
pub fn all_reduce_sum(dm: &mut DofManager, x: Real) -> Result<Real> {
    if dm.communicator().size() == 1 {
        return Ok(x);
    }
    dm.collective("all_reduce_sum", |dm| {
        let mut ph = dm.phases()?;
        ph.run_collective("sum", Ok([x]), |c, mut s| {
            c.all_reduce(&mut s, akantu_comm::ReduceOp::Sum)?;
            Ok(s[0])
        })
    })
}

struct NodalArray<'a> {
    v: &'a mut [Real],
    add: bool,
}

impl akantu_comm::DataAccessor<Idx> for NodalArray<'_> {
    fn nb_data(&self, n: &[Idx], _: SynchronizationTag) -> usize {
        n.len() * 8
    }
    fn pack_data(
        &self,
        b: &mut akantu_comm::CommunicationBuffer,
        n: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &i in n {
            b.pack(&self.v[i as usize]);
        }
        Ok(())
    }
    fn unpack_data(
        &mut self,
        b: &mut akantu_comm::CommunicationBuffer,
        n: &[Idx],
        _: SynchronizationTag,
    ) -> Result<()> {
        for &i in n {
            let x: Real = b.unpack()?;
            if self.add {
                self.v[i as usize] += x;
            } else {
                self.v[i as usize] = x;
            }
        }
        Ok(())
    }
}

impl SolverCallback for MyModel {
    fn matrix_type(&self, id: &str) -> MatrixType {
        if id == "C" && self.rayleigh.is_none() {
            MatrixType::NotDefined
        } else {
            MatrixType::Symmetric
        }
    }

    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        self.log(&format!("assemble_matrix:{id}"))?;
        match id {
            "K" => {
                if !self.is_stiffness_assembled || self.law == Law::Cubic {
                    self.assemble_stiffness(dm)?;
                }
                Ok(())
            }
            "M" => {
                if !self.is_mass_assembled {
                    self.assemble_mass(dm)?;
                }
                Ok(())
            }
            "C" => match self.rayleigh {
                Some((a, b)) if !self.is_damping_assembled => self.assemble_damping(dm, a, b),
                _ => Ok(()),
            },
            _ => Err(Error::invalid(format!(
                "This solver does not know what to do with a matrix {id}"
            ))),
        }
    }

    fn assemble_lumped_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        self.log(&format!("assemble_lumped_matrix:{id}"))?;
        if id == "M" {
            if !self.is_lumped_mass_assembled {
                self.assemble_lumped_mass(dm)?;
            }
            Ok(())
        } else {
            Err(Error::invalid(format!(
                "This solver does not know what to do with a matrix {id}"
            )))
        }
    }

    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        self.log("assemble_residual")?;
        let forces = Array::from_vec(self.forces.clone(), 1)?;
        dm.assemble_to_residual("disp", forces.view(), 1.0)?;
        self.internal_forces.iter_mut().for_each(|f| *f = 0.0);
        self.assemble_residual_internal(dm)?;
        if !self.upstream_partial_forces {
            let mut fi = std::mem::take(&mut self.internal_forces);
            let r = self.complete_shared(dm, &mut fi);
            self.internal_forces = fi;
            r?;
        }
        let fi = Array::from_vec(self.internal_forces.clone(), 1)?;
        dm.assemble_to_residual("disp", fi.view(), -1.0)
    }

    fn assemble_residual_part(&mut self, _: &mut DofManager, part: &str) -> Result<()> {
        self.log(&format!("assemble_residual:{part}"))
    }

    fn predictor(&mut self, _: &mut DofManager) -> Result<()> {
        self.log("predictor")
    }

    fn corrector(&mut self, _: &mut DofManager) -> Result<()> {
        self.log("corrector")
    }

    fn before_solve_step(&mut self, _: &mut DofManager) -> Result<()> {
        self.log("before_solve_step")
    }

    fn after_solve_step(&mut self, _: &mut DofManager, converged: bool) -> Result<()> {
        self.log(&format!("after_solve_step:{converged}"))
    }

    /// Upstream MyModel has no `initSolver` override; the port's toy only
    /// injects a failure here (not traced: solve traces are unchanged).
    fn init_solver(
        &mut self,
        _: &mut DofManager,
        _: akantu_core::enums::TimeStepSolverType,
    ) -> Result<()> {
        if self.fail_at.as_deref() == Some("init_solver") {
            return Err(Error::Numerical("injected failure in init_solver".into()));
        }
        Ok(())
    }
}
