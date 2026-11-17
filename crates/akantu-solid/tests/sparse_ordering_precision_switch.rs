// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream test:
// the sparse solver's ordering follows the model's precision mode (D-123,
// D-125) when the policy changes between two solves.

//! A static elastic model analysed in precision `Fast` with no ordering set
//! gets the `auto` ordering, which chooses nested dissection on this cube
//! (3,000 unknowns, 3-D). After `set_exec_policy(Bitwise)` the next solve
//! must factorise with COLAMD again: its displacement is bit for bit that of
//! a model that was bitwise from the start, and the solver reports COLAMD.
//! The second solve uses the same matrix values (a linear static problem):
//! the backend factorises again because the factorisation went stale.

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::exec::{ExecPolicy, Precision};
use akantu_core::{ElementType, GhostType, Real};
use akantu_mesh::{Mesh, MeshAccessor, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;

/// An `n³` grid of hexahedra of the unit cube.
fn cube(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "cube").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    a.push_node(&[i as Real * h, j as Real * h, k as Real * h])
                        .unwrap();
                }
            }
        }
        let id = |i: usize, j: usize, k: usize| ((k * (n + 1) + j) * (n + 1) + i) as i32;
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    a.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost)
                        .push_tuple(&[
                            id(i, j, k),
                            id(i + 1, j, k),
                            id(i + 1, j + 1, k),
                            id(i, j + 1, k),
                            id(i, j, k + 1),
                            id(i + 1, j, k + 1),
                            id(i + 1, j + 1, k + 1),
                            id(i, j + 1, k + 1),
                        ]);
                }
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// The ordering the static solver's last analysis used (0 COLAMD, 1 nested
/// dissection).
fn ordering_used(m: &SolidMechanicsModel) -> Real {
    m.solver
        .dof_manager
        .backend()
        .sparse_solver_statistics("J")
        .iter()
        .find(|s| s.0 == "ordering")
        .map(|s| s.1)
        .expect("the eigen solver's statistics")
}

/// Two static solves (the second with twice the load, from a zero
/// displacement) under `first` then `second`; the second solve's
/// displacement as bits and the orderings used.
fn two_solves(first: ExecPolicy, second: ExecPolicy) -> (Vec<u64>, [Real; 2]) {
    let dat = "material elastic [\n  name = a\n  rho = 2500\n  E = 70e9\n  nu = 0.22\n]\n";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut m =
        SolidMechanicsModel::with_parser(SharedMesh::new(cube(9)), None, "smm", parser).unwrap();
    m.set_exec_policy(first).unwrap();
    m.init_full(
        &ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Linear),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        }),
    )
    .unwrap();
    let x: Vec<Real> = m
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let mut blocked = vec![false; x.len()];
    for (n, p) in x.as_chunks::<3>().0.iter().enumerate() {
        if p[0] < 1e-9 {
            blocked[3 * n..3 * n + 3].fill(true);
        }
    }
    m.blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    let load = |m: &mut SolidMechanicsModel, f: Real| {
        let mut ext = m.external_force_mut().unwrap();
        for (n, p) in x.as_chunks::<3>().0.iter().enumerate() {
            if p[0] > 1.0 - 1e-9 {
                ext.as_mut_slice()[3 * n + 2] = f * (1.0 + p[1]);
            }
        }
    };
    load(&mut m, 1e3);
    m.solve_step(None).unwrap();
    let o1 = ordering_used(&m);
    m.set_exec_policy(second).unwrap();
    m.displacement_mut().unwrap().as_mut_slice().fill(0.0);
    load(&mut m, 2e3);
    m.solve_step(None).unwrap();
    let o2 = ordering_used(&m);
    let u = m
        .displacement()
        .unwrap()
        .as_slice()
        .iter()
        .map(|v| v.to_bits())
        .collect();
    (u, [o1, o2])
}

#[test]
fn bitwise_after_fast_uses_colamd_again() {
    let bitwise = ExecPolicy::serial();
    let fast = bitwise.with_precision(Precision::Fast);
    let (reference, orderings) = two_solves(bitwise, bitwise);
    assert_eq!(orderings, [0.0, 0.0]);
    let (switched, orderings) = two_solves(fast, bitwise);
    // the fast solve chose nested dissection, the bitwise one COLAMD
    assert_eq!(orderings, [1.0, 0.0]);
    assert!(
        switched == reference,
        "the bitwise solve after a fast one differs"
    );
    // and the other way: a fast solve after a bitwise one takes `auto`
    let (_, orderings) = two_solves(bitwise, fast);
    assert_eq!(orderings, [0.0, 1.0]);
}
