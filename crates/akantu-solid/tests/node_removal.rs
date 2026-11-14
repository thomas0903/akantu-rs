// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Node removal
// (SMM-015, UD-209): node removal between two solves of a live
// SolidMechanicsModel equals a fresh model on the mesh after the removal
// with the state transferred (upstream solid_mechanics_model.cc:752-783,
// dof_manager.cc:901-902); serial, and on 1/2/4 ranks of the local world
// with the removal on a rank boundary (the same scenarios run on real MPI
// in `mpi_smm.rs`).

mod node_removal_common;
mod par_smm;

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use node_removal_common::{Change, Method, Outcome};

fn check(o: Outcome) {
    if let Err(e) = o {
        panic!("{e}");
    }
}

#[test]
fn static_removal_equals_a_fresh_model() {
    check(node_removal_common::serial_equivalence(
        Method::Static,
        Change::EraseCornerAndPurify,
        1,
    ));
}

#[test]
fn explicit_removal_equals_a_fresh_model() {
    check(node_removal_common::serial_equivalence(
        Method::Explicit,
        Change::EraseCornerAndPurify,
        6,
    ));
}

/// Unused nodes (blocked in the live model) removed by `purify` alone: no
/// element event in the mutation.
#[test]
fn explicit_removal_of_unused_nodes_equals_a_fresh_model() {
    check(node_removal_common::serial_equivalence(
        Method::Explicit,
        Change::PurifyUnused,
        6,
    ));
}

#[test]
fn implicit_dynamic_removal_equals_a_fresh_model() {
    check(node_removal_common::serial_equivalence(
        Method::ImplicitDynamic,
        Change::EraseCornerAndPurify,
        3,
    ));
}

fn on_ranks(f: impl Fn(Communicator) -> Outcome + Sync) {
    // 3 and 8 ranks too (from 7 ranks on some ranks hold no corner
    // element: ranks without the removed node, empty ranks)
    for n in [1, 2, 3, 4, 8] {
        let out = run_local(n, Duration::from_secs(120), &f);
        for (r, o) in out.into_iter().enumerate() {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

#[test]
fn explicit_removal_on_a_rank_boundary_equals_a_fresh_model() {
    on_ranks(|c| node_removal_common::rank_boundary_equivalence(c, Method::Explicit, 4));
}

#[test]
fn static_removal_on_a_rank_boundary_equals_a_fresh_model() {
    on_ranks(|c| node_removal_common::rank_boundary_equivalence(c, Method::Static, 1));
}

#[test]
fn implicit_dynamic_removal_on_a_rank_boundary_equals_a_fresh_model() {
    on_ranks(|c| node_removal_common::rank_boundary_equivalence(c, Method::ImplicitDynamic, 2));
}

/// An IGFEM-shaped update in one mutation — the elements using the old
/// node erased, the node removed with a renumbering, a new node added, new
/// elements using it — goes through the solid model and its DOF manager:
/// equal to a fresh model on the final mesh, bitwise. This validates the
/// DOF path of a removal followed by an addition, not upstream's IGFEM
/// order (nodes added first, removed and renumbered last: two mutations).
#[test]
fn igfem_shaped_update_equals_a_fresh_model() {
    check(node_removal_common::igfem_shaped_equivalence(4));
}

/// A rank-specific refusal of the rank-boundary removal rolls it back on
/// every rank; the same removal then commits.
#[test]
fn rank_specific_refusal_rolls_back_on_every_rank() {
    on_ranks(node_removal_common::rollback_when_one_rank_refuses);
}

/// UD-1072: an element erased on its owner while its ghost copies are kept
/// is refused by the distributed removal hook, rolled back on every rank.
#[test]
fn inconsistent_element_erasure_is_refused() {
    on_ranks(node_removal_common::inconsistent_element_erasure_is_refused);
}

/// The variable bounds of a native (`petsc_tao`) solver
/// name local equations and are not moved by a renumbering; after a node
/// removal the next bounded solve is refused (agreed, recoverable, naming
/// the cause) until the bounds are set again. Without PETSc the solve then
/// stops at the native driver: the refusal comes before it.
#[test]
fn tao_bounds_are_invalidated_by_a_node_removal() {
    use akantu_core::enums::AnalysisMethod;
    use akantu_mesh::{MeshPart, SharedMesh};
    use akantu_model::ModelOptions;
    use akantu_solver::NativeNonLinear;
    const CAUSE: &str = "node removal renumbered the local equations";
    let shared = SharedMesh::new(node_removal_common::scrambled_square(4, 3, true));
    let mut m = node_removal_common::model_on(&shared).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    m.solver.solver_mut(None).unwrap().non_linear_solver =
        Box::new(NativeNonLinear::petsc_tao("tao"));
    let set = |m: &mut akantu_solid::SolidMechanicsModel| {
        let n = m.solver.dof_manager.layout().nb_local();
        m.solver
            .non_linear_solver_mut(None)
            .unwrap()
            .as_native_mut()
            .unwrap()
            .set_bounds(vec![-1.0; n], vec![1.0; n])
            .unwrap();
        n
    };
    let n_before = set(&mut m);
    let e = m.solve_step(None).expect_err("no native backend here");
    assert!(!format!("{e}").contains(CAUSE), "{e}");
    m.mutate_mesh(|tx| tx.purify(MeshPart::Main), None).unwrap();
    assert!(m.solver.dof_manager.layout().nb_local() < n_before);
    let e = m.solve_step(None).expect_err("stale bounds refused");
    assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
    assert!(format!("{e}").contains(CAUSE), "{e}");
    // set again: no longer refused for the bounds (stops at the driver)
    set(&mut m);
    let e = m.solve_step(None).expect_err("no native backend here");
    assert!(!format!("{e}").contains(CAUSE), "{e}");
}

/// beside the bounds test, same set-up: the
/// UD-1022 check of a bounded `petsc_tao` solve (D-088) also refuses a NaN
/// bound, which no comparison admits (upstream hands it to TAO), with the
/// same agreed, recoverable `Unsupported`, before the native driver; ±inf
/// bounds are admitted (the solve then stops at the driver, which needs
/// PETSc).
#[test]
fn tao_nan_bounds_are_refused_by_the_ud1022_check() {
    use akantu_core::enums::AnalysisMethod;
    use akantu_mesh::SharedMesh;
    use akantu_model::ModelOptions;
    use akantu_solver::NativeNonLinear;
    let shared = SharedMesh::new(node_removal_common::scrambled_square(4, 3, true));
    let mut m = node_removal_common::model_on(&shared).unwrap();
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    m.solver.solver_mut(None).unwrap().non_linear_solver =
        Box::new(NativeNonLinear::petsc_tao("tao"));
    let set = |m: &mut akantu_solid::SolidMechanicsModel, lo: f64, up: f64, at: usize| {
        let n = m.solver.dof_manager.layout().nb_local();
        let (mut lower, mut upper) = (vec![f64::NEG_INFINITY; n], vec![f64::INFINITY; n]);
        lower[at] = lo;
        upper[at] = up;
        m.solver
            .non_linear_solver_mut(None)
            .unwrap()
            .as_native_mut()
            .unwrap()
            .set_bounds(lower, upper)
            .unwrap();
    };
    for (lo, up) in [(f64::NAN, 1.0), (-1.0, f64::NAN)] {
        set(&mut m, lo, up, 3);
        let e = m.solve_step(None).expect_err("NaN bound refused");
        assert!(e.fatal_kind().is_none(), "recoverable: {e:?}");
        assert!(matches!(e, akantu_core::Error::Unsupported(_)), "{e:?}");
        let text = format!("{e}");
        assert!(
            text.contains("UD-1022") && text.contains("on 1 owned row(s)"),
            "{text}"
        );
    }
    // ±inf everywhere: admitted (stops at the native driver here)
    set(&mut m, f64::NEG_INFINITY, f64::INFINITY, 3);
    let e = m.solve_step(None).expect_err("no native backend here");
    assert!(!format!("{e}").contains("UD-1022"), "{e}");
}

/// At 1 / 2 / 4 ranks: the corner cell erased in two
/// calls of one mutation is refused before the commit, agreed on every
/// rank, rolled back; the one-call erasure then commits.
#[test]
fn two_erasures_of_a_block_are_refused_on_every_rank() {
    on_ranks(node_removal_common::two_erasures_of_a_block_are_refused);
}

/// At 1 / 2 / 4 ranks: an element added and erased again
/// in one mutation (on rank 0) is refused before the commit, agreed on
/// every rank, rolled back.
#[test]
fn added_then_erased_is_refused_on_every_rank() {
    on_ranks(node_removal_common::added_then_erased_is_refused);
}

/// At 1 / 2 / 4 ranks: after an erasure that moves kept
/// elements on a rank boundary, every ghost receives its owner's
/// barycentre and every slave its master's coordinates and global id
/// through the renumbered synchronizers.
#[test]
fn synchronizers_after_a_middle_erasure() {
    on_ranks(node_removal_common::synchronizers_after_a_middle_erasure);
}
