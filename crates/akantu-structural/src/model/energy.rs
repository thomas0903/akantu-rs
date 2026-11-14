// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/structural_mechanics/
// structural_mechanics_model.cc (getKineticEnergy, getPotentialEnergy,
// getEnergy).

//! Energies (STRUCT-011).
//!
//! Each public method is one collective operation of the DOF manager whose
//! local part is agreed before the rank reduction (as the solid model), so
//! a failure on one rank surfaces as an error on every rank. Sums follow
//! upstream's accumulation order.

use akantu_comm::ReduceOp;
use akantu_core::{Array, Real, Result};
use akantu_mesh::Mesh;
use akantu_solver::DofManager;

use super::{
    DOF_ID, StructuralMechanicsModel, StructuralMechanicsPhysics, check_nodal, not_allocated,
};

/// `allReduce(op)` of one scalar through the running collective operation
/// of `dm`; the local value (or the local error) is agreed first.
fn all_reduce_in(dm: &mut DofManager, local: Result<Real>, op: ReduceOp) -> Result<Real> {
    let mut ph = dm.phases()?;
    ph.run_collective("structural:all_reduce", local.map(|x| [x]), |c, mut s| {
        c.all_reduce(&mut s, op)?;
        Ok(s[0])
    })
}

/// Eigen `a.dot(b)` of two dynamic vectors (non-vectorised redux: first
/// product, then sequential additions).
fn dot(a: &[Real], b: &[Real]) -> Real {
    let mut it = a.iter().zip(b);
    let Some((a0, b0)) = it.next() else {
        return 0.0;
    };
    let mut r = a0 * b0;
    for (x, y) in it {
        r += x * y;
    }
    r
}

fn local_or_master(mesh: &Mesh, n: usize) -> Result<bool> {
    Ok(mesh
        .node_flag(akantu_core::try_idx(n)?)
        .is_local_or_master())
}

/// Outcome of the local kinetic-energy computation.
enum Kinetic {
    /// Local sum (to be reduced).
    Sum(Real),
    /// No mass matrix of any kind: upstream returns `0` at once (no
    /// reduction).
    NoMass,
}

impl StructuralMechanicsPhysics {
    /// Local part of upstream `getKineticEnergy()`
    /// (`structural_mechanics_model.cc:578-600`).
    ///
    /// * lumped `M` registered: assemble it, then `Σ_n Σ_i v_i² m_i` over
    ///   the local-or-master nodes (every DOF, rotations included); with
    ///   the D-059 blocks (`Block3x3` in 3-D) each node adds
    ///   `2·Σ_{i<j} J_ij ω_i ω_j`, so the rotations contribute `ωᵀ J ω`;
    /// * else consistent `M` registered: assemble it, `Mv`, then
    ///   `Σ_n v_n·(Mv)_n` over the local-or-master nodes;
    /// * else no mass.
    ///
    /// As upstream, the integration scheme of every dynamic solver creates
    /// the sparse `M` when it is set (`TimeStepSolver::setIntegrationScheme`;
    /// `_explicit_lumped_mass` included), while the lumped `M` exists only
    /// after the first lumped assembly (the first step, or
    /// [`StructuralMechanicsModel::assemble_lumped_mass_matrix`]): before
    /// it, an explicit-lumped analysis reports the consistent-mass energy
    /// (UD-510).
    fn kinetic_energy_local(&mut self, dofs: &mut DofManager) -> Result<Kinetic> {
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let nn = mesh.nb_nodes();
        let ndof = self.nb_degree_of_freedom;
        let mut ekin = 0.0;
        if dofs.has_lumped_matrix("M") {
            dofs.step("structural:assemble_lumped_matrix:M", |dm| {
                self.assemble_lumped_mass_matrix(dm)
            })?;
            let m = self
                .mass
                .as_ref()
                .ok_or_else(|| akantu_core::Error::invalid("The lumped mass is not allocated."))?;
            let v = dofs
                .dofs_derivative(DOF_ID, 1)
                .map_err(|_| not_allocated("velocity"))?;
            check_nodal("getKineticEnergy: lumped mass", m, nn, ndof)?;
            check_nodal("getKineticEnergy: velocity", &*v, nn, ndof)?;
            // D-059 blocks (Block3x3 in 3-D): ωᵀJω adds the off-diagonal
            // terms 2·Σ_{i<j} J_ij ω_i ω_j to the diagonal sum
            let couplings = if self.compat.lumped_rotary_inertia
                == super::LumpedRotaryInertia::Block3x3
                && ndof == 6
            {
                let j = self
                    .rotary_inertia
                    .as_ref()
                    .ok_or_else(|| not_allocated("rotary_inertia"))?;
                check_nodal("getKineticEnergy: rotary inertia", j, nn, 9)?;
                Some(j.as_slice())
            } else {
                None
            };
            for n in 0..nn {
                if !local_or_master(&mesh, n)? {
                    continue;
                }
                let mut mv2 = 0.0;
                for i in 0..ndof {
                    let vi = v.as_slice()[n * ndof + i];
                    mv2 += vi * vi * m.as_slice()[n * ndof + i];
                }
                if let Some(j) = couplings {
                    let b = &j[n * 9..(n + 1) * 9];
                    let w = &v.as_slice()[n * 6 + 3..n * 6 + 6];
                    let off = b[1] * (w[0] * w[1]) + b[2] * (w[0] * w[2]) + b[5] * (w[1] * w[2]);
                    mv2 += 2. * off;
                }
                ekin += mv2;
            }
        } else if dofs.has_matrix("M") {
            dofs.step("structural:assemble_matrix:M", |dm| {
                self.assemble_mass_matrix(dm)
            })?;
            dofs.finalize_matrix("M")?;
            // the copy (needed by the product below) ends the guard before
            // `dofs` is borrowed mutably (explicit guard `Drop`)
            let v = {
                let v = dofs
                    .dofs_derivative(DOF_ID, 1)
                    .map_err(|_| not_allocated("velocity"))?;
                check_nodal("getKineticEnergy: velocity", &*v, nn, ndof)?;
                v.as_slice().to_vec()
            };
            let mut mv = Array::try_zeros(nn, ndof)?;
            dofs.assemble_mat_mul_vect_to_array(DOF_ID, "M", &v, &mut mv, 1.0)?;
            for n in 0..nn {
                if local_or_master(&mesh, n)? {
                    ekin += dot(
                        &v[n * ndof..(n + 1) * ndof],
                        &mv.as_slice()[n * ndof..(n + 1) * ndof],
                    );
                }
            }
        } else {
            return Ok(Kinetic::NoMass);
        }
        Ok(Kinetic::Sum(ekin))
    }

    /// Local part of upstream `getPotentialEnergy()`: assemble `K` (if
    /// needed), `Ku`, then `Σ_n (u_n·(Ku)_n) · [local or master]`.
    fn potential_energy_local(&mut self, dofs: &mut DofManager) -> Result<Real> {
        dofs.step("structural:assemble_stiffness", |dm| {
            self.assemble_stiffness_matrix(dm)
        })?;
        dofs.finalize_matrix("K")?;
        let mesh_h = self.base.mesh.clone();
        let mesh = mesh_h.read()?;
        let nn = mesh.nb_nodes();
        let ndof = self.nb_degree_of_freedom;
        // the copy (needed by the product below) ends the guard before
        // `dofs` is borrowed mutably (explicit guard `Drop`)
        let u = {
            let u = dofs
                .dofs(DOF_ID)
                .map_err(|_| not_allocated("displacement"))?;
            check_nodal("getPotentialEnergy: displacement", &*u, nn, ndof)?;
            u.as_slice().to_vec()
        };
        let mut ku = Array::try_zeros(nn, ndof)?;
        dofs.assemble_mat_mul_vect_to_array(DOF_ID, "K", &u, &mut ku, 1.0)?;
        let mut epot = 0.0;
        for n in 0..nn {
            let w = if local_or_master(&mesh, n)? { 1.0 } else { 0.0 };
            epot += dot(
                &u[n * ndof..(n + 1) * ndof],
                &ku.as_slice()[n * ndof..(n + 1) * ndof],
            ) * w;
        }
        Ok(epot)
    }
}

impl StructuralMechanicsModel {
    /// Upstream `getKineticEnergy()`: `½ Σ m v²` with the lumped mass (all
    /// DOFs; with the D-059 rotary blocks `½ (Σ m v² + Σₙ ωₙᵀ Jₙ ωₙ)` over
    /// translations and rotations), else `½ vᵀ M v` with the consistent
    /// mass, summed over the ranks; `0` when the model has no mass matrix
    /// (static analyses).
    ///
    /// The lumped branch is taken once a lumped `M` exists; every dynamic
    /// integration scheme creates the sparse `M` when it is set (as
    /// upstream), so an `_explicit_lumped_mass` analysis reports the
    /// consistent-mass energy until its first lumped assembly (first step or
    /// [`StructuralMechanicsModel::assemble_lumped_mass_matrix`]), bitwise
    /// as upstream (UD-510).
    ///
    /// # Errors
    /// Mass assembly errors; velocity not allocated with a mass matrix.
    pub fn kinetic_energy(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        let e = self
            .solver
            .dof_manager
            .collective("structural:kinetic_energy", |dm| {
                match p.kinetic_energy_local(dm) {
                    Ok(Kinetic::NoMass) => Ok(None),
                    Ok(Kinetic::Sum(x)) => all_reduce_in(dm, Ok(x), ReduceOp::Sum).map(Some),
                    Err(e) => all_reduce_in(dm, Err(e), ReduceOp::Sum).map(Some),
                }
            })?;
        Ok(e.map_or(0.0, |e| e / 2.))
    }

    /// Upstream `getPotentialEnergy()`: `½ uᵀ K u` (rotations included),
    /// summed over the ranks; `K` is assembled if needed.
    ///
    /// # Errors
    /// Stiffness assembly errors.
    pub fn potential_energy(&mut self) -> Result<Real> {
        let p = &mut self.physics;
        let e = self
            .solver
            .dof_manager
            .collective("structural:potential_energy", |dm| {
                let local = p.potential_energy_local(dm);
                all_reduce_in(dm, local, ReduceOp::Sum)
            })?;
        Ok(e / 2.)
    }

    /// Upstream `getEnergy(id)`: `"kinetic"`, `"potential"`; any other id
    /// returns `0` (upstream defines no other energy for this model, e.g.
    /// no external work; UD-510).
    ///
    /// # Errors
    /// See [`StructuralMechanicsModel::kinetic_energy`] and
    /// [`StructuralMechanicsModel::potential_energy`].
    pub fn energy(&mut self, energy_id: &str) -> Result<Real> {
        match energy_id {
            "kinetic" => self.kinetic_energy(),
            "potential" => self.potential_energy(),
            _ => Ok(0.0),
        }
    }
}
