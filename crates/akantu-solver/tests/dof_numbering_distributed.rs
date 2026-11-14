// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): expected numbers derived by hand from
// src/model/common/dof_manager/dof_manager.cc:225-247 (countDOFsForNodes),
// :590-626 (updateNodalDOFs), :646-674 (onNodesAdded iterates std::map),
// :768-870 (computeFirstDOFIDs, updateDOFsData, periodic correction,
// _ask_nodes synchronisation) and src/solver/sparse_matrix_aij.cc:52-80
// (applyBoundary), :170-195 (matVecMul, UD-003).

//! Distributed numbering and (distributed
//! assembly/BC context), executed on an in-process 3-rank world.
//!
//! Global 1D bar: nodes g0..g4 at x = 0..4, segments [g0,g1] [g1,g2] [g2,g3]
//! [g3,g4]. Elements 0-1 on rank 0, 2-3 on rank 1, rank 2 is empty.
//!
//! | rank | local nodes (flag) | elements | ghost elements |
//! |---|---|---|---|
//! | 0 | L0=g0, L1=g1, L2=g2 (master), L3=g3 (pure ghost) | [0,1] [1,2] | [2,3] |
//! | 1 | L0=g2 (slave of rank 0), L1=g3, L2=g4, L3=g1 (pure ghost) | [0,1] [1,2] | [3,0] |
//! | 2 | — | — | — |
//!
//! For the numbering example rank 1 additionally declares L2 (g4) a
//! periodic slave of L1 (g3).

use std::time::Duration;

use akantu_comm::{Communicator, Synchronizer, run_local};
use akantu_core::enums::MatrixType;
use akantu_core::{Array, ElementType, GhostType, NodeFlag};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::DofManager;

const SEG: ElementType = ElementType::Segment2;

fn rank_mesh(comm: &Communicator, periodic: bool) -> Mesh {
    let mut mesh = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
    let r = comm.rank();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let (xs, flags): (Vec<f64>, Vec<NodeFlag>) = match r {
            0 => (
                vec![0., 1., 2., 3.],
                vec![
                    NodeFlag::NORMAL,
                    NodeFlag::NORMAL,
                    NodeFlag::MASTER,
                    NodeFlag::PURE_GHOST,
                ],
            ),
            1 => (
                vec![2., 3., 4., 1.],
                vec![
                    NodeFlag::SLAVE,
                    NodeFlag::NORMAL,
                    NodeFlag::NORMAL,
                    NodeFlag::PURE_GHOST,
                ],
            ),
            _ => (vec![], vec![]),
        };
        for (x, f) in xs.iter().zip(&flags) {
            let n = acc.push_node(&[*x]).unwrap();
            acc.nodes_flags_mut().as_mut_slice()[n as usize] = *f;
        }
        let (local, ghost): (&[[i32; 2]], &[[i32; 2]]) = match r {
            0 => (&[[0, 1], [1, 2]], &[[2, 3]]),
            1 => (&[[0, 1], [1, 2]], &[[3, 0]]),
            _ => (&[], &[]),
        };
        for c in local {
            acc.connectivity_mut(SEG, GhostType::NotGhost).push_tuple(c);
        }
        for c in ghost {
            acc.connectivity_mut(SEG, GhostType::Ghost).push_tuple(c);
        }
        let mut sync = Synchronizer::new("nodes", comm.clone());
        match r {
            0 => {
                sync.communications.send.insert(1, vec![2]);
            }
            1 => {
                sync.communications.recv.insert(0, vec![0]);
                acc.set_node_prank(0, 0);
            }
            _ => {}
        }
        acc.synchronizers_mut().node = Some(sync);
        acc.set_distributed();
        if periodic && r == 1 {
            acc.add_periodic_pair(2, 1).unwrap();
        }
    }
    mesh
}

/// Executable version of the numbering example of ARCHITECTURE.
#[test]
fn numbering_two_families_three_ranks_periodic_and_insertion() {
    let out = run_local(3, Duration::from_secs(10), |comm| {
        let mut mesh = rank_mesh(&comm, true);
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_nodal_dofs(&mesh, "displacement", 2).unwrap();
        dm.register_nodal_dofs(&mesh, "damage", 1).unwrap();
        let before = (
            dm.local_equation_numbers("displacement").unwrap().to_vec(),
            dm.global_equation_numbers("displacement").unwrap(),
            dm.global_equation_numbers("damage").unwrap(),
            dm.system_size(),
        );
        // Later insertion: rank 1 appends one node (e.g. a doubled cohesive
        // node), ranks 0 and 2 add nothing but take part in the collectives.
        let new_nodes = if comm.rank() == 1 {
            let mut acc = MeshAccessor::new(&mut mesh);
            vec![acc.push_node(&[3.0]).unwrap()]
        } else {
            vec![]
        };
        dm.on_nodes_added(&mesh, &new_nodes).unwrap();
        let after = (
            dm.global_equation_numbers("displacement").unwrap(),
            dm.global_equation_numbers("damage").unwrap(),
            dm.system_size(),
            dm.local_system_size(),
            dm.pure_local_system_size(),
        );
        dm.layout().validate().unwrap();
        (before, after)
    });
    let s = Some;
    // ---- registration of "displacement" (2 components), then "damage" ----
    // rank 0 owns 6 displacement DOFs (exclusive scan 0), rank 1 owns 2
    // (offset 6), rank 2 none (offset 8): total 8.
    let (b0, a0) = &out[0];
    assert_eq!(b0.0, vec![0, 1, 2, 3, 4, 5, 6, 7]);
    assert_eq!(b0.1, vec![s(0), s(1), s(2), s(3), s(4), s(5), None, None]);
    // damage: rank 0 owns 3 (offset 0 from the persistent offset 8).
    assert_eq!(b0.2, vec![s(8), s(9), s(10), None]);
    assert_eq!(b0.3, 12);
    let (b1, a1) = &out[1];
    // rank 1: L0 shared (owner's 4,5), L1 owned 6,7, L2 periodic slave
    // aliases L1's equations (local 2,3), L3 pure ghost (local 4,5, no
    // global number).
    assert_eq!(b1.0, vec![0, 1, 2, 3, 2, 3, 4, 5]);
    assert_eq!(b1.1, vec![s(4), s(5), s(6), s(7), s(6), s(7), None, None]);
    assert_eq!(b1.2, vec![s(10), s(11), s(11), None]);
    assert_eq!(out[2].0.3, 12, "empty rank agrees on the system size");
    // ---- node insertion: families in lexicographic order ("damage" first)
    assert_eq!(a1.1.last().copied().flatten(), Some(12));
    assert_eq!(a1.0[8..], [s(13), s(14)]);
    assert_eq!(a0.0.len(), 8, "rank 0 unchanged");
    for (_, a) in &out {
        assert_eq!(a.2, 15);
    }
    assert_eq!((a1.3, a1.4), (6 + 3 + 3, 2 + 1 + 3));
}

/// Element stiffness of a unit 1D bar element (column-major 2×2).
fn bar_k(n: usize) -> Array<f64> {
    let mut a = Array::new(4);
    for _ in 0..n {
        a.push_tuple(&[1., -1., -1., 1.]);
    }
    a
}

#[test]
fn distributed_assembly_owner_only_diagonal_and_reduced_product() {
    let out = run_local(3, Duration::from_secs(10), |comm| {
        let mesh = rank_mesh(&comm, false);
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        dm.register_blocked_dofs("u").unwrap();
        dm.new_matrix("K", MatrixType::Symmetric).unwrap();
        let ne = mesh.nb_element(SEG, GhostType::NotGhost);
        dm.assemble_elemental_matrices_to_matrix(
            &mesh,
            "K",
            "u",
            bar_k(ne).view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
        .unwrap();
        dm.finalize_matrix("K").unwrap();
        // x = nodal coordinate (linear field): K x vanishes at interior
        // nodes, including the partition node g2, once partial sums are
        // reduced (UD-003 would leave +1 / -1 there).
        let x: Vec<f64> = (0..mesh.nb_nodes())
            .map(|n| mesh.nodes().as_slice()[n])
            .collect();
        let mut y = vec![0.0; dm.local_system_size()];
        dm.mat_vec(&mesh, "K", &x, &mut y, 1.0, 0.0).unwrap();
        let layout = dm.layout().clone();
        let products: Vec<(f64, f64)> = (0..x.len())
            .filter(|&l| layout.global(l).is_some())
            .map(|l| (x[l], y[l]))
            .collect();
        // Block g2 on both copies (owner and slave), apply Dirichlet.
        if let Some(l) = (0..x.len()).find(|&l| x[l] == 2.0) {
            dm.family_mut("u")
                .unwrap()
                .blocked
                .as_mut()
                .unwrap()
                .borrow_mut()
                .as_mut_slice()[l] = true;
        }
        dm.apply_boundary("K").unwrap();
        let g2 = layout.global((0..x.len()).find(|&l| x[l] == 2.0).unwrap_or(usize::MAX));
        let diag = g2.map_or(0.0, |g| dm.matrix("K").unwrap().get(g, g));
        let mut diag_sum = [diag];
        comm.all_reduce(&mut diag_sum, akantu_comm::ReduceOp::Sum)
            .unwrap();
        (products, diag, diag_sum[0])
    });
    for (products, _, diag_sum) in &out {
        for &(x, y) in products {
            let expected = if x == 0.0 {
                -1.0
            } else if x == 4.0 {
                1.0
            } else {
                0.0
            };
            assert_eq!(y, expected, "reduced product at x = {x}");
        }
        assert_eq!(
            *diag_sum, 1.0,
            "the unit diagonal of a shared blocked DOF appears once"
        );
    }
    assert_eq!(
        (out[0].1, out[1].1),
        (1.0, 0.0),
        "owner (rank 0) holds the diagonal"
    );
}
