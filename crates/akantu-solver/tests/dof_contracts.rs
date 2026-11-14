// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/dof_manager/
// dof_manager.cc:630-643 (resizeGlobalArrays → clearProfile on every
// matrix), dof_manager_default.cc:295-406 (addToProfile, profiled cache
// cleared), src/solver/sparse_matrix_aij_inline_impl.hh:61-75
// (clearProfile), src/model/common/time_step_solvers/time_step_solver.cc:
// 75-86,138-172 (J of the common type of K/M/C), UD-034.

//! Symmetry promotion agreed on all ranks and propagated to J and the
//! solver backend, numbering changes reset every matrix, and checked
//! inputs and capacities on implemented DOF paths.

mod common;

use std::sync::Arc;
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::MatrixType;
use akantu_core::{Array, Error, GhostType, Idx};
use akantu_linalg::{SymmetryPolicy, TermsToAssemble};
use akantu_mesh::MeshAccessor;
use akantu_solver::{AijBackend, DofManager, LinearSystemBackend};
use common::{Log, MockNative, SEG, serial_bar, two_rank_bar};

fn bar_k(n: usize, k: [f64; 4]) -> Array<f64> {
    let mut a = Array::new(4);
    for _ in 0..n {
        a.push_tuple(&k);
    }
    a
}

// ------------------------------------------------------------------

/// Rank 0 assembles an unsymmetric block into the symmetric `K`; rank 1
/// assembles nothing (zero-work rank). After the collective finalisation
/// both ranks hold an unsymmetric `K` **and** an unsymmetric derived `J`,
/// and the backend re-analyses `J` with `sym = 0` on both ranks.
#[test]
fn promotion_is_agreed_on_all_ranks_and_propagated_to_j() {
    for native in [false, true] {
        let out = run_local(2, Duration::from_secs(20), |comm| {
            let log: Log = Arc::default();
            let backend: Box<dyn LinearSystemBackend> = if native {
                Box::new(MockNative::new(Arc::clone(&log)))
            } else {
                Box::new(AijBackend::default())
            };
            let mesh = two_rank_bar(&comm);
            let mut dm = DofManager::with_backend("dm", comm.clone(), backend);
            dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
            dm.new_matrix("K", MatrixType::Symmetric).unwrap();
            dm.new_derived_matrix("J", &["K"]).unwrap();
            assert_eq!(dm.matrix_type("J").unwrap(), MatrixType::Symmetric);
            if comm.rank() == 0 {
                dm.assemble_elemental_matrices_to_matrix(
                    &mesh,
                    "K",
                    "u",
                    bar_k(2, [1.0, -2.0, -1.0, 1.0]).view(),
                    SEG,
                    GhostType::NotGhost,
                    MatrixType::Unsymmetric,
                    None,
                )
                .unwrap();
            }
            let before = (dm.matrix_type("K").unwrap(), dm.matrix_type("J").unwrap());
            dm.finalize_matrix("K").unwrap();
            let after = (dm.matrix_type("K").unwrap(), dm.matrix_type("J").unwrap());
            let mut sym = None;
            if native {
                // The solver backend re-analyses J with the agreed type.
                dm.backend_zero("J").unwrap();
                let one: Vec<f64> = vec![1.0; dm.local_system_size()];
                let mut id = TermsToAssemble::new("u", "u");
                for i in 0..dm.local_system_size() {
                    id.add(i as Idx, i as Idx, 1.0);
                }
                // Unit diagonal on owned rows only (rank-partial input).
                let owned: Vec<bool> = (0..dm.local_system_size())
                    .map(|l| dm.layout().is_owned(l))
                    .collect();
                id.terms.retain(|t| owned[t.i as usize]);
                dm.assemble_preassembled_matrix("J", &id).unwrap();
                dm.finalize_matrix("J").unwrap();
                let mut x = vec![0.0; one.len()];
                dm.solve_linear_system("J", &one, &mut x).unwrap();
                sym = log
                    .lock()
                    .unwrap()
                    .iter()
                    .find(|l| l.starts_with("analysis J"))
                    .cloned();
            }
            (before, after, sym)
        });
        // Rank 0 proposed locally, rank 1 did nothing before agreement.
        assert_eq!(out[0].0.0, MatrixType::Unsymmetric);
        assert_eq!(out[1].0, (MatrixType::Symmetric, MatrixType::Symmetric));
        for (_, after, sym) in &out {
            assert_eq!(*after, (MatrixType::Unsymmetric, MatrixType::Unsymmetric));
            if native {
                assert_eq!(sym.as_deref(), Some("analysis J sym=0"));
            }
        }
    }
}

/// Compat policy (UD-034 upstream selection): no promotion anywhere.
#[test]
fn upstream_selection_policy_never_promotes() {
    let out = run_local(2, Duration::from_secs(20), |comm| {
        let mesh = two_rank_bar(&comm);
        let mut dm = DofManager::with_backend(
            "dm",
            comm.clone(),
            Box::new(AijBackend::with_policy(
                SymmetryPolicy::UpstreamUpperSelection,
            )),
        );
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        dm.new_matrix("K", MatrixType::Symmetric).unwrap();
        dm.assemble_elemental_matrices_to_matrix(
            &mesh,
            "K",
            "u",
            bar_k(2, [1.0, -2.0, -1.0, 1.0]).view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Unsymmetric,
            None,
        )
        .unwrap();
        dm.finalize_matrix("K").unwrap();
        dm.matrix_type("K").unwrap()
    });
    assert!(out.iter().all(|t| *t == MatrixType::Symmetric));
}

// ------------------------------------------------------------------

fn coo(dm: &DofManager, id: &str) -> (Vec<i64>, Vec<i64>, Vec<f64>) {
    let (i, j, a) = dm.matrix(id).unwrap().coo();
    (i.to_vec(), j.to_vec(), a.to_vec())
}

/// Terms-only, unassembled and elemental matrices across a node insertion
/// with a reconnection: every matrix loses its profile/values and takes the
/// new order (upstream `clearProfile` on every matrix); re-assembly gives
/// exactly the COO arrays of a fresh assembly on the new mesh — no obsolete
/// coupling, no stale first-insertion order.
#[test]
fn numbering_change_resets_every_matrix_profile() {
    let mut mesh = serial_bar(2); // n0 - n1 - n2
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    for (id, ty) in [
        ("T", MatrixType::Unsymmetric),
        ("U", MatrixType::Symmetric),
        ("E", MatrixType::Symmetric),
    ] {
        dm.new_matrix(id, ty).unwrap();
    }
    let mut t = TermsToAssemble::new("u", "u");
    t.add(0, 0, 7.0);
    dm.assemble_preassembled_matrix("T", &t).unwrap();
    let k = [1.0, -1.0, -1.0, 1.0];
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "E",
        "u",
        bar_k(2, k).view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .unwrap();
    assert_eq!(dm.matrix("E").unwrap().get(1, 2), -1.0);
    // Insertion with reconnection: n3 duplicates n2's position, segment 1
    // becomes [n1, n3] (n2 isolated).
    let n3 = {
        let mut acc = MeshAccessor::new(&mut mesh);
        let n3 = acc.push_node(&[2.0]).unwrap();
        acc.connectivity_mut(SEG, GhostType::NotGhost)
            .as_mut_slice()[2..4]
            .copy_from_slice(&[1, n3]);
        n3
    };
    dm.on_nodes_added(&mesh, &[n3]).unwrap();
    for id in ["T", "U", "E"] {
        let m = dm.matrix(id).unwrap();
        assert_eq!((m.size(), m.nnz()), (4, 0), "{id} reset to the new order");
    }
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "E",
        "u",
        bar_k(2, k).view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .unwrap();
    let mut fresh = DofManager::new("fresh", Communicator::serial());
    fresh.register_nodal_dofs(&mesh, "u", 1).unwrap();
    fresh.new_matrix("E", MatrixType::Symmetric).unwrap();
    fresh
        .assemble_elemental_matrices_to_matrix(
            &mesh,
            "E",
            "u",
            bar_k(2, k).view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
        .unwrap();
    assert_eq!(coo(&dm, "E"), coo(&fresh, "E"));
    assert_eq!(
        dm.matrix("E").unwrap().get(1, 2),
        0.0,
        "obsolete coupling gone"
    );
    // A later registration also resets every matrix (upstream registerDOFs
    // → resizeGlobalArrays).
    dm.register_generic_dofs("lambda", 1, 1).unwrap();
    assert_eq!(dm.matrix("E").unwrap().nnz(), 0);
    assert_eq!(dm.matrix("E").unwrap().size(), 5);
}

/// The same reset reaches native backends, for every matrix.
#[test]
fn numbering_change_resets_native_matrices() {
    let log: Log = Arc::default();
    let mut mesh = serial_bar(1);
    let mut dm = DofManager::with_backend(
        "dm",
        Communicator::serial(),
        Box::new(MockNative::new(Arc::clone(&log))),
    );
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    dm.new_matrix("M", MatrixType::Symmetric).unwrap();
    let n = MeshAccessor::new(&mut mesh).push_node(&[2.0]).unwrap();
    log.lock().unwrap().clear();
    dm.on_nodes_added(&mesh, &[n]).unwrap();
    assert_eq!(*log.lock().unwrap(), ["reset K 3", "reset M 3"]);
}

// ------------------------------------------------------------------

/// Invalid pre-assembled terms (positive out-of-range, negative) are
/// errors before any backend change, never panics.
#[test]
fn invalid_terms_are_errors_without_mutation() {
    let mesh = serial_bar(0); // one node: a one-DOF family
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    dm.new_matrix("K", MatrixType::Unsymmetric).unwrap();
    for (i, j) in [(1, 0), (0, 1), (-1, 0), (0, -5)] {
        let mut t = TermsToAssemble::new("u", "u");
        t.add(0, 0, 3.0); // a valid term first: must not be inserted alone
        t.add(i, j, 1.0);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dm.assemble_preassembled_matrix("K", &t)
        }));
        let err = r.expect("no panic").unwrap_err();
        assert!(
            matches!(err, Error::OutOfRange { .. } | Error::InvalidIndex { .. }),
            "{err:?}"
        );
        assert_eq!(dm.matrix("K").unwrap().nnz(), 0, "no partial insertion");
    }
    let mut t = TermsToAssemble::new("u", "missing");
    t.add(0, 0, 1.0);
    assert!(dm.assemble_preassembled_matrix("K", &t).is_err());
}

/// Element filters and connectivities are validated before indexing.
#[test]
fn invalid_filters_and_connectivity_are_errors_without_mutation() {
    let mut mesh = serial_bar(2);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    let k = [1.0, -1.0, -1.0, 1.0];
    for filter in [vec![0, 2], vec![-1]] {
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                "u",
                bar_k(filter.len(), k).view(),
                SEG,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                Some(&filter),
            )
        }));
        assert!(r.expect("no panic").is_err(), "filter {filter:?}");
        assert_eq!(dm.matrix("K").unwrap().nnz(), 0, "profile untouched");
    }
    // A connectivity node without DOFs (added to the mesh, not to the
    // family): error before the backend is touched.
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let n = acc.push_node(&[3.0]).unwrap();
        acc.connectivity_mut(SEG, GhostType::NotGhost)
            .push_tuple(&[2, n]);
    }
    let err = dm
        .assemble_elemental_matrices_to_matrix(
            &mesh,
            "K",
            "u",
            bar_k(3, k).view(),
            SEG,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
        .unwrap_err();
    assert!(matches!(err, Error::OutOfRange { .. }), "{err:?}");
    assert_eq!(dm.matrix("K").unwrap().nnz(), 0);
}

/// A failed node addition (invalid node, missing group) leaves every
/// family, the layout and the matrices unchanged; registrations with
/// invalid input add nothing.
#[test]
fn growth_failures_preserve_families() {
    let mut mesh = serial_bar(2);
    mesh.groups_mut().create_element_group("right", 1).unwrap();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    dm.register_generic_dofs("lambda", 1, 1).unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    let layout = dm.layout().clone();
    let eqs = dm.local_equation_numbers("u").unwrap().to_vec();
    for bad in [vec![3], vec![-1], vec![0, 99]] {
        assert!(dm.on_nodes_added(&mesh, &bad).is_err(), "{bad:?}");
        assert_eq!(dm.layout(), &layout);
        assert_eq!(dm.local_equation_numbers("u").unwrap(), &eqs[..]);
        assert_eq!(dm.dofs("u").unwrap().len(), 3);
        assert_eq!(dm.dof_ids(), ["u", "lambda"]);
    }
    assert!(
        dm.register_nodal_dofs_on_group(&mesh, "v", 1, "nope")
            .is_err()
    );
    assert!(dm.register_nodal_dofs(&mesh, "u", 1).is_err(), "duplicate");
    assert_eq!(dm.dof_ids(), ["u", "lambda"]);
    assert_eq!(dm.layout(), &layout);
    // A valid addition afterwards still works.
    let n = MeshAccessor::new(&mut mesh).push_node(&[3.0]).unwrap();
    dm.on_nodes_added(&mesh, &[n]).unwrap();
    assert_eq!(dm.local_system_size(), layout.nb_local() + 2);
}

/// Rank-specific growth failure (invalid node on rank 1 only): agreed in
/// the preflight, so both ranks fail promptly and keep their families.
#[test]
fn rank_specific_growth_failure_is_agreed() {
    let t0 = std::time::Instant::now();
    let out = run_local(2, Duration::from_secs(20), |comm| {
        let mesh = two_rank_bar(&comm);
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        let before = dm.layout().clone();
        let nodes: Vec<Idx> = if comm.rank() == 1 { vec![42] } else { vec![] };
        let r = dm.on_nodes_added(&mesh, &nodes);
        (r.is_err(), dm.layout() == &before, dm.poisoned().is_none())
    });
    assert!(t0.elapsed() < Duration::from_secs(5));
    assert!(out.iter().all(|o| *o == (true, true, true)));
}
