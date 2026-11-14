// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): contract tests for the collective paths of
// src/model/common/dof_manager/dof_manager_default.cc (solution
// synchronisation, norms, distributed products),
// src/model/common/non_linear_solver/non_linear_solver_petsc.cc:100-200
// (SNES FormFunction/FormJacobian, solve lifecycle). Upstream lets a rank
// throw while its peers wait in a collective; these tests pin the port's
// collective error protocol.

//! Rank-specific failures at every vector-validation and callback boundary
//! of the implemented collective solver entry points,
//! fatal-failure poisoning, operations continued on a scoped
//! worker and typed fatal callback/driver results.
//!
//! Every case runs on a two-rank local world with a **20 s** diagnostic
//! timeout and must complete in well under [`PROMPT`]: an ordinary input or
//! callback error surfaces as an error on **every** rank (the failing
//! rank's own error, "failed on another rank" elsewhere), never as a
//! transport timeout. Cases include zero-work ranks.

mod common;

use std::sync::Arc;
use std::time::{Duration, Instant};

use akantu_comm::phase::{Phases, Severity};
use akantu_comm::{Communicator, run_local};
use akantu_core::enums::MatrixType;
use akantu_core::{Array, Error, FatalKind, GhostType, Idx};
use akantu_solver::{DofManager, NativeNonLinear, NativeSolveContext, NonLinearSolver};
use common::{Log, MockNative, SEG, Springs, newton, two_rank_bar};

const TIMEOUT: Duration = Duration::from_secs(20);
const PROMPT: Duration = Duration::from_secs(2);

/// Run `f` on two local-world ranks; assert that it completed promptly.
fn two_ranks<T: Send>(f: impl Fn(Communicator) -> T + Sync) -> Vec<T> {
    let t0 = Instant::now();
    let out = run_local(2, TIMEOUT, f);
    let dt = t0.elapsed();
    assert!(dt < PROMPT, "took {dt:?}: a rank waited for a timeout");
    out
}

/// `true` for the error a healthy rank reports when another rank failed.
fn remote_failure(e: &Error) -> bool {
    matches!(e, Error::Communication(m) if m.contains("failed on another rank"))
}

/// Outcome summary of one rank: `Ok`, its own error (`"own"`), or the
/// agreed remote failure (`"remote"`).
fn class(r: &akantu_core::Result<impl Sized>, own: impl Fn(&Error) -> bool) -> &'static str {
    match r {
        Ok(_) => "ok",
        Err(e) if remote_failure(e) => "remote",
        Err(e) if own(e) => "own",
        Err(e) => panic!("unexpected error {e:?}"),
    }
}

fn dim_mismatch(e: &Error) -> bool {
    matches!(e, Error::DimensionMismatch { .. })
}

fn native_dm(comm: &Communicator, configure: impl FnOnce(&mut MockNative)) -> DofManager {
    let mut backend = MockNative::new(Arc::default());
    configure(&mut backend);
    DofManager::with_backend("dm", comm.clone(), Box::new(backend))
}

// -------------------------------------------------: one-rank input errors

/// Case 1: one generic scalar per rank; `owned_norm(&[])` on rank
/// 0, `owned_norm(&[1.])` on rank 1. Previously rank 0 returned its
/// dimension error and rank 1 timed out in the sum all-reduce.
#[test]
fn reviewer_1_owned_norm_bad_length_on_one_rank() {
    let out = two_ranks(|comm| {
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_generic_dofs("u", 1, 1).unwrap();
        let v = if comm.rank() == 0 { vec![] } else { vec![1.] };
        let bad = class(&dm.owned_norm(&v), dim_mismatch);
        // Recoverable: the manager stays usable on every rank.
        let good = dm.owned_norm(&[1.0]).unwrap();
        (bad, good, dm.poisoned().is_none())
    });
    assert_eq!(out[0], ("own", 2f64.sqrt(), true));
    assert_eq!(out[1], ("remote", 2f64.sqrt(), true));
}

/// Case 1 with a **zero-work** rank (no DOF): the zero-work rank fails its
/// validation (it passes a value it does not have), or the working rank
/// fails while the zero-work rank passes its valid empty vector.
#[test]
fn owned_norm_with_a_zero_work_rank() {
    for zero_work_fails in [true, false] {
        let out = two_ranks(|comm| {
            let mut dm = DofManager::new("dm", comm.clone());
            let n = usize::from(comm.rank() == 0);
            dm.register_generic_dofs("u", n, 1).unwrap();
            let v: Vec<f64> = match (comm.rank(), zero_work_fails) {
                (1, true) => vec![1.0], // zero-work rank: wrong length
                (1, false) => vec![],
                (_, true) => vec![3.0],
                (_, false) => vec![3.0, 4.0], // working rank: wrong length
            };
            class(&dm.owned_norm(&v), dim_mismatch)
        });
        let expect = if zero_work_fails {
            ["remote", "own"]
        } else {
            ["own", "remote"]
        };
        assert_eq!(out, expect);
    }
}

/// Case 2: two-rank bar, nodal `u`; `set_solution(&[])` only on
/// rank 0. Previously rank 1 timed out in the synchronizer's preparation
/// agreement.
#[test]
fn reviewer_2_set_solution_bad_length_on_one_rank() {
    let out = two_ranks(|comm| {
        let mesh = two_rank_bar(&comm);
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        let n = dm.local_system_size();
        let x = if comm.rank() == 0 {
            vec![]
        } else {
            vec![1.0; n]
        };
        let bad = class(&dm.set_solution(&x), dim_mismatch);
        // Usable afterwards: shared rows synchronised from the owner.
        let x: Vec<f64> = (0..n)
            .map(|l| 10.0 * comm.rank() as f64 + l as f64)
            .collect();
        dm.set_solution(&x).unwrap();
        (bad, dm.solution().to_vec())
    });
    assert_eq!(out[0].0, "own");
    assert_eq!(out[1].0, "remote");
    assert_eq!(out[0].1, [0.0, 1.0, 2.0]);
    // g2 (rank 1's local 0) is owned by rank 0: its value (2.0) wins.
    assert_eq!(out[1].1, [2.0, 11.0, 12.0]);
}

/// Case 3: same bar, symmetric `J`; `form_jacobian` with correct
/// zero vectors on both ranks; the callback's `corrector` fails only on
/// rank 0. Previously rank 0 stored the error and rank 1 timed out at
/// finalisation. Now both trampolines return the same nonzero status, the
/// failing rank keeps its own error, and a later evaluation succeeds.
#[test]
fn reviewer_3_native_corrector_failure_on_one_rank() {
    let out = two_ranks(|comm| {
        let mut dm = native_dm(&comm, |_| ());
        let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
        p.setup(&mut dm);
        dm.new_matrix("J", MatrixType::Symmetric).unwrap();
        p.fail_corrector = comm.rank() == 0;
        let x = vec![0.0; dm.local_system_size()];
        let (status, err) = {
            let mut ctx = NativeSolveContext::new(&mut dm, &mut p, "J");
            let s = ctx.form_jacobian(&x);
            (s, ctx.take_error())
        };
        let err = err.map(|e| (remote_failure(&e), e.to_string()));
        p.fail_corrector = false;
        let again = NativeSolveContext::new(&mut dm, &mut p, "J").form_jacobian(&x);
        (status, err, again, dm.poisoned().is_none())
    });
    assert_eq!(out[0].0, 1);
    assert_eq!(out[1].0, 1);
    let (remote0, msg0) = out[0].1.clone().unwrap();
    assert!(
        !remote0 && msg0.contains("injected corrector failure"),
        "{msg0}"
    );
    assert!(
        out[1].1.as_ref().unwrap().0,
        "rank 1: agreed remote failure"
    );
    for o in &out {
        assert_eq!((o.2, o.3), (0, true), "usable after a recoverable failure");
    }
}

// -------------------------------------------: every trampoline boundary

/// Where a trampoline evaluation fails.
#[derive(Clone, Copy, Debug)]
enum Fault {
    FunctionCorrector,
    FunctionResidual,
    FunctionShape,
    FunctionTrialShape,
    JacobianCorrector,
    JacobianAssembly,
    JacobianTrialShape,
    LinearSolveShape,
    NormShape,
}

/// Each trampoline boundary with a failure on rank 0 or rank 1 only: both
/// ranks return the **same** nonzero status promptly; the failing rank
/// stores its own error, the other the agreed remote failure; nobody is
/// poisoned (ordinary errors are recoverable).
#[test]
fn trampoline_failures_at_every_boundary_are_agreed() {
    use Fault::*;
    for fault in [
        FunctionCorrector,
        FunctionResidual,
        FunctionShape,
        FunctionTrialShape,
        JacobianCorrector,
        JacobianAssembly,
        JacobianTrialShape,
        LinearSolveShape,
        NormShape,
    ] {
        for bad in [0, 1] {
            let out = two_ranks(|comm| {
                let mut dm = native_dm(&comm, |_| ());
                let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
                p.setup(&mut dm);
                dm.new_matrix("J", MatrixType::Symmetric).unwrap();
                let me = comm.rank() == bad;
                let n = dm.local_system_size();
                let short = |ok: usize| if me { ok - 1 } else { ok };
                match fault {
                    FunctionCorrector | JacobianCorrector => p.fail_corrector = me,
                    FunctionResidual => p.fail_residual_at = me.then_some(1),
                    JacobianAssembly => p.fail_assemble_matrix = me,
                    _ => {}
                }
                let mut ctx = NativeSolveContext::new(&mut dm, &mut p, "J");
                let status = match fault {
                    FunctionCorrector | FunctionResidual => {
                        ctx.form_function(&vec![0.0; n], &mut vec![0.0; n])
                    }
                    FunctionShape => ctx.form_function(&vec![0.0; n], &mut vec![0.0; short(n)]),
                    FunctionTrialShape => {
                        ctx.form_function(&vec![0.0; short(n)], &mut vec![0.0; n])
                    }
                    JacobianCorrector | JacobianAssembly => ctx.form_jacobian(&vec![0.0; n]),
                    JacobianTrialShape => ctx.form_jacobian(&vec![0.0; short(n)]),
                    LinearSolveShape => {
                        // A valid Jacobian first (identical on both ranks).
                        assert_eq!(ctx.form_jacobian(&vec![0.0; n]), 0);
                        ctx.linear_solve(&vec![1.0; short(n)], &mut vec![0.0; n])
                    }
                    NormShape => ctx.owned_norm(&vec![1.0; short(n)]).err().unwrap_or(0),
                };
                let err = ctx.take_error();
                let remote = err.as_ref().is_some_and(remote_failure);
                drop(ctx);
                (status, err.is_some(), remote, dm.poisoned().is_none())
            });
            for (rank, o) in out.iter().enumerate() {
                let failing = rank == bad as usize;
                assert_eq!(
                    *o,
                    (1, true, !failing, true),
                    "{fault:?}, failure on rank {bad}: rank {rank}"
                );
            }
        }
    }
}

// -------------------------------------: native solve lifecycle boundaries

/// `NativeNonLinear::solve` with a rank-specific failure in each lifecycle
/// step — `before_solve_step`, `predictor`, native-driver acquisition, a
/// trampoline in the middle of the iterations, `after_solve_step` — fails
/// on **every** rank promptly (previously these were called with `?` and
/// no agreement).
#[test]
fn native_solve_lifecycle_failures_are_agreed() {
    #[derive(Clone, Copy, Debug)]
    enum Step {
        Before,
        Predictor,
        Driver,
        MidSolveResidual,
        After,
    }
    for step in [
        Step::Before,
        Step::Predictor,
        Step::Driver,
        Step::MidSolveResidual,
        Step::After,
    ] {
        for bad in [0, 1] {
            let out = two_ranks(|comm| {
                let me = comm.rank() == bad;
                let mut dm = native_dm(&comm, |b| {
                    b.fail_driver = me && matches!(step, Step::Driver)
                });
                let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
                p.setup(&mut dm);
                match step {
                    Step::Before => p.fail_before_solve_step = me,
                    Step::Predictor => p.fail_predictor = me,
                    Step::MidSolveResidual => p.fail_residual_at = me.then_some(2),
                    Step::After => p.fail_after_solve_step = me,
                    Step::Driver => {}
                }
                let r = NativeNonLinear::new("J").solve(&mut dm, &mut p);
                let remote = r.as_ref().err().is_some_and(remote_failure);
                (r.is_err(), remote, dm.poisoned().is_none())
            });
            for (rank, o) in out.iter().enumerate() {
                let failing = rank == bad as usize;
                assert_eq!(
                    *o,
                    (true, !failing, true),
                    "{step:?}, failure on rank {bad}: rank {rank}"
                );
            }
        }
    }
}

/// The akantu-side Newton (each iteration one collective operation, every
/// callback agreed — the protocol) over the distributed native
/// backend: converges on both ranks; with a residual failure on one rank in
/// the second iteration, both ranks fail promptly.
#[test]
fn newton_iteration_callback_failure_is_agreed() {
    for fail in [false, true] {
        let out = two_ranks(|comm| {
            let mut dm = native_dm(&comm, |_| ());
            let mut p = Springs::new(two_rank_bar(&comm), 4.0, true);
            p.setup(&mut dm);
            p.fail_residual_at = (fail && comm.rank() == 1).then_some(2);
            let r = newton(&mut dm, &mut p);
            let iterations = r.as_ref().ok().copied();
            (
                class(
                    &r,
                    |e| matches!(e, Error::Numerical(m) if m.contains("injected residual")),
                ),
                iterations,
            )
        });
        if fail {
            assert_eq!((out[0].0, out[1].0), ("remote", "own"));
        } else {
            assert_eq!(out[0], out[1], "same iteration count on every rank");
            assert!(out[0].1.is_some_and(|it| it >= 3), "{out:?}");
        }
    }
}

// -------------------------------: other implemented collective entries

/// Distributed `mat_vec`: (a) the backend fails locally **before** it
/// reaches the collective reduction on one rank — the reduction runs as
/// phases of the same scope, so the healthy rank's reduction agreement and
/// the failing rank's closing agreement meet and both fail; (b) a bad
/// vector length on one rank is agreed before any communication.
#[test]
fn distributed_mat_vec_failures_are_agreed() {
    for (case, bad) in [("backend", 0), ("backend", 1), ("length", 0), ("length", 1)] {
        let out = two_ranks(|comm| {
            let me = comm.rank() == bad;
            let mesh = two_rank_bar(&comm);
            let mut dm = native_dm(&comm, |b| b.fail_mat_vec = me && case == "backend");
            dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
            dm.new_matrix("K", MatrixType::Symmetric).unwrap();
            let mut k = Array::new(4);
            k.push_tuple(&[1., -1., -1., 1.]);
            k.push_tuple(&[1., -1., -1., 1.]);
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                "u",
                k.view(),
                SEG,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                None,
            )
            .unwrap();
            dm.finalize_matrix("K").unwrap();
            let n = dm.local_system_size();
            let nx = if me && case == "length" { n + 1 } else { n };
            let mut y = vec![0.0; n];
            let r = dm.mat_vec(&mesh, "K", &vec![1.0; nx], &mut y, 1.0, 0.0);
            let bad_class = class(&r, |e| {
                matches!(e, Error::Backend { .. } | Error::DimensionMismatch { .. })
            });
            // Usable afterwards with a healthy backend path (length case).
            let healthy = (case == "length").then(|| {
                let x: Vec<f64> = (0..n)
                    .map(|l| comm.rank() as f64 * 2.0 + l as f64)
                    .collect();
                let mut y = vec![0.0; n];
                dm.mat_vec(&mesh, "K", &x, &mut y, 1.0, 0.0).map(|()| y)
            });
            (bad_class, healthy.map(|h| h.unwrap()))
        });
        for (rank, (c, _)) in out.iter().enumerate() {
            let expect = if rank == bad as usize {
                "own"
            } else {
                "remote"
            };
            assert_eq!(*c, expect, "{case} on rank {bad}: rank {rank}");
        }
        if case == "length" {
            // u = x along the bar: K·u = [-1, 0, 0 | 0, 0, 1] (the shared
            // node's row is complete on both copies after the reduction).
            assert_eq!(out[0].1.as_deref(), Some(&[-1.0, 0.0, 0.0][..]));
            assert_eq!(out[1].1.as_deref(), Some(&[0.0, 0.0, 1.0][..]));
        }
    }
}

/// `solve_linear_system` (bad right-hand side / solution length on one
/// rank) and `finalize_matrix` (matrix missing on one rank) agree their
/// local validation before the backend communicates.
#[test]
fn solve_and_finalize_validation_is_agreed() {
    for (case, bad) in [
        ("rhs", 0),
        ("solution", 1),
        ("finalize", 0),
        ("finalize", 1),
    ] {
        let out = two_ranks(|comm| {
            let me = comm.rank() == bad;
            let mut dm = native_dm(&comm, |_| ());
            let p = Springs::new(two_rank_bar(&comm), 4.0, false);
            p.setup(&mut dm);
            let n = dm.local_system_size();
            if case == "finalize" {
                if !me {
                    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
                }
                return class(&dm.finalize_matrix("K"), |e| {
                    matches!(e, Error::NotFound { .. })
                });
            }
            dm.new_matrix("J", MatrixType::Symmetric).unwrap();
            let mut id = akantu_linalg::TermsToAssemble::new("u", "u");
            for l in 0..n {
                if dm.layout().is_owned(l) {
                    id.add(l as Idx, l as Idx, 1.0);
                }
            }
            dm.assemble_preassembled_matrix("J", &id).unwrap();
            dm.finalize_matrix("J").unwrap();
            let nr = if me && case == "rhs" { n - 1 } else { n };
            let nx = if me && case == "solution" { n + 2 } else { n };
            let mut x = vec![0.0; nx];
            class(
                &dm.solve_linear_system("J", &vec![1.0; nr], &mut x),
                dim_mismatch,
            )
        });
        for (rank, c) in out.iter().enumerate() {
            let expect = if rank == bad as usize {
                "own"
            } else {
                "remote"
            };
            assert_eq!(*c, expect, "{case} on rank {bad}: rank {rank}");
        }
    }
}

/// A communicating callback that opens its **own** scope (the old
/// `sync.slave_reduction` style) inside a trampoline is refused locally
/// instead of desynchronising the collectives — on both ranks, or on one
/// rank only — and the refusal is agreed: same status everywhere, no hang.
#[test]
fn independent_scope_inside_a_callback_is_refused_not_hung() {
    for only_rank in [None, Some(0), Some(1)] {
        let out = two_ranks(|comm| {
            let mut dm = native_dm(&comm, |_| ());
            let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
            p.setup(&mut dm);
            p.independent_scope = only_rank.is_none_or(|r| r == comm.rank());
            let n = dm.local_system_size();
            let mut ctx = NativeSolveContext::new(&mut dm, &mut p, "J");
            let status = ctx.form_function(&vec![0.0; n], &mut vec![0.0; n]);
            let err = ctx.take_error().map(|e| e.to_string());
            (status, err)
        });
        for (status, err) in &out {
            assert_eq!(*status, 1, "{only_rank:?}");
            let err = err.as_deref().unwrap();
            assert!(
                err.contains("already active") || err.contains("failed on another rank"),
                "{err}"
            );
        }
    }
}

// ------------------------------------------------------------------

/// The native mock with an **atomic** injected
/// promotion failure on rank 1; rank 0 assembles an unsymmetric block into
/// symmetric `K`, rank 1 assembles nothing. Rank 0 promoted irreversibly,
/// rank 1 did not: the failure is agreed as fatal, **both** managers are
/// poisoned and return `Error::Fatal { kind: Agreed }`, and later use
/// (`backend_zero`, `finalize_matrix`) is refused on both.
#[test]
fn agreed_fatal_promotion_poisons_every_rank() {
    let out = two_ranks(|comm| {
        let mesh = two_rank_bar(&comm);
        let mut dm = native_dm(&comm, |b| b.fail_promote = comm.rank() == 1);
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        dm.new_matrix("K", MatrixType::Symmetric).unwrap();
        if comm.rank() == 0 {
            let mut k = Array::new(4);
            k.push_tuple(&[1.0, -2.0, -1.0, 1.0]);
            k.push_tuple(&[1.0, -2.0, -1.0, 1.0]);
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                "u",
                k.view(),
                SEG,
                GhostType::NotGhost,
                MatrixType::Unsymmetric,
                None,
            )
            .unwrap();
        }
        let err = dm.finalize_matrix("K").unwrap_err();
        (
            err.fatal_kind(),
            dm.poison_kind(),
            dm.backend_zero("K").map_err(|e| e.fatal_kind()),
            dm.finalize_matrix("K").map_err(|e| e.fatal_kind()),
            dm.matrix_type("K").unwrap(),
        )
    });
    let agreed = Some(FatalKind::Agreed);
    for o in &out {
        assert_eq!((o.0, o.1), (agreed, agreed));
        assert_eq!(o.2, Err(agreed), "later backend use refused");
        assert_eq!(o.3, Err(agreed), "later collectives refused locally");
    }
    // The inconsistent state that makes this fatal.
    assert_eq!(out[0].4, MatrixType::Unsymmetric);
    assert_eq!(out[1].4, MatrixType::Symmetric);
}

/// A matrix reset failing on one rank **after** a committed numbering
/// change (registration, node addition) is fatal: agreed, every rank's
/// manager is poisoned (fatal path).
#[test]
fn failed_reset_after_a_committed_numbering_poisons_every_rank() {
    for growth in ["registration", "nodes_added"] {
        let out = two_ranks(|comm| {
            let mut mesh = two_rank_bar(&comm);
            // Every reset fails on rank 0; the first registration resets
            // nothing (no matrix yet).
            let mut failing = native_dm(&comm, |b| b.fail_reset = comm.rank() == 0);
            failing.register_nodal_dofs(&mesh, "u", 1).unwrap();
            failing.new_matrix("K", MatrixType::Symmetric).unwrap();
            let r = if growth == "registration" {
                failing.register_generic_dofs("lambda", 1, 1)
            } else {
                let n = akantu_mesh::MeshAccessor::new(&mut mesh)
                    .push_node(&[9.0])
                    .unwrap();
                failing.on_nodes_added(&mesh, &[n])
            };
            let kind = r.unwrap_err().fatal_kind();
            (
                kind,
                failing.poison_kind(),
                failing.backend_zero("K").is_err(),
            )
        });
        for o in &out {
            assert_eq!(
                *o,
                (Some(FatalKind::Agreed), Some(FatalKind::Agreed), true),
                "{growth}"
            );
        }
    }
}

/// A transport failure (rank 1 never joins the norm; short local-world
/// timeout) poisons the observing rank with the **transport** kind — the
/// runtime's abort signal — and that rank never communicates again.
#[test]
fn transport_failure_poisons_the_observing_rank() {
    let out = run_local(2, Duration::from_millis(300), |comm| {
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_generic_dofs("u", 1, 1).unwrap();
        if comm.rank() == 0 {
            let e = dm.owned_norm(&[1.0]).unwrap_err();
            let again = dm.owned_norm(&[1.0]).unwrap_err();
            (e.fatal_kind(), dm.poison_kind(), again.fatal_kind())
        } else {
            (None, dm.poison_kind(), None)
        }
    });
    let t = Some(FatalKind::Transport);
    assert_eq!(out[0], (t, t, t));
    assert_eq!(out[1], (None, None, None));
}

/// Serial sanity: the collective protocol costs nothing observable on one
/// rank and keeps every result (also through the native trampolines).
#[test]
fn serial_paths_are_unchanged() {
    let mut dm = DofManager::with_backend(
        "dm",
        Communicator::serial(),
        Box::new(MockNative::new(Log::default())),
    );
    let mut p = Springs::new(common::serial_bar(4), 4.0, false);
    p.setup(&mut dm);
    NativeNonLinear::new("J").solve(&mut dm, &mut p).unwrap();
    let u_g = dm.dofs("u").unwrap();
    let u = u_g.as_slice();
    for (x, u) in [0.0, 1.0, 2.0, 3.0, 4.0].iter().zip(u) {
        assert!((u - x).abs() < 1e-10);
    }
    assert!(dm.backend().has_matrix("J"));
    drop(u_g);
    assert!(dm.phases().is_err(), "no collective operation running");
}

// ------------------------------------------------------------------

/// The scope-loan choreography at the DOF-manager level: a callback
/// continues the running operation on a **scoped worker** (the manager is
/// `Send`; its scope is parked between uses and resumed by each step on
/// the calling thread). Rank 1's worker also opens an independent scope on
/// the communicator — refused, because the manager's scope is live — and
/// the step agreement on the worker reports both failures on both ranks.
/// No timeout, nothing poisoned, the manager stays usable.
#[test]
fn reviewer_ar4_01_operation_continued_on_a_worker_is_one_scope() {
    fn send<T: Send>() {}
    send::<DofManager>(); // idle-owner mobility is preserved
    let out = two_ranks(|comm| {
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_generic_dofs("u", 1, 1).unwrap();
        let r: akantu_core::Result<()> = dm.collective("outer", |dm| {
            std::thread::scope(|s| {
                s.spawn(|| {
                    // The owner thread is blocked in `join`; the worker is
                    // the only thread using the scope.
                    assert_eq!(comm.active_scope().as_deref(), Some("dof:dm:outer"));
                    let local = if comm.rank() == 0 {
                        Err(Error::invalid("local callback failure"))
                    } else {
                        let mut nested = Phases::new(&comm, "independent-child");
                        assert!(nested.is_refused(), "the missing enforcement");
                        nested.agree("child", Ok(()), Severity::Recoverable)
                    };
                    dm.step("callback:done", |_| local)
                })
                .join()
                .unwrap()
            })
        });
        let later = dm.owned_norm(&[1.0]);
        (
            r.map_err(|e| e.to_string()),
            dm.poison_kind(),
            later.ok(),
            comm.active_scope(),
        )
    });
    assert!(
        out[0]
            .0
            .as_ref()
            .unwrap_err()
            .contains("local callback failure")
    );
    assert!(out[1].0.as_ref().unwrap_err().contains("already active"));
    for o in &out {
        assert_eq!((o.1, o.2, o.3.as_deref()), (None, Some(2f64.sqrt()), None));
    }
}

/// A communicating callback running on a scoped worker communicates
/// through the resumed scope ([`DofManager::phases`]) while the owner
/// thread's raw operations stay refused; the operation completes.
#[test]
fn communicating_callback_on_a_worker_uses_the_resumed_scope() {
    let out = two_ranks(|comm| {
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_generic_dofs("u", 1, 1).unwrap();
        dm.collective("outer", |dm| {
            let sum = std::thread::scope(|s| {
                let h = s.spawn(|| {
                    dm.step("callback", |d| {
                        d.phases()?.run("sum", Ok(()), |c, ()| {
                            let mut v = [1u64];
                            c.all_reduce(&mut v, akantu_comm::ReduceOp::Sum)?;
                            Ok(Ok(v[0]))
                        })
                    })
                });
                h.join().unwrap()
            })?;
            let mut v = [1u64];
            let raw = comm.all_reduce(&mut v, akantu_comm::ReduceOp::Sum);
            Ok((sum, raw.is_err()))
        })
    });
    for o in out {
        assert_eq!(o.unwrap(), (2, true));
    }
}

// ------------------------------------------------------------------

/// The double-completion case and its siblings: rank 0 alone returns an
/// `Error::Fatal { kind: Agreed }` (a callback propagating an unusable
/// dependent owner's error) from a `step`, from the operation body (the
/// closing agreement) or as an already computed outcome
/// (`agree_outcome`, the native driver's boundary). Every rank returns
/// `Error::Fatal { kind: Agreed }`, every rank's manager is poisoned and
/// refuses the next collective locally. An ordinary error at the same
/// boundaries stays recoverable.
#[test]
fn reviewer_ar4_02_typed_fatal_callback_results_poison_every_rank() {
    for path in ["step", "closing", "outcome"] {
        for typed in [true, false] {
            let out = two_ranks(|comm| {
                let mut dm = DofManager::new("dm", comm.clone());
                dm.register_generic_dofs("u", 1, 1).unwrap();
                let local = || -> akantu_core::Result<()> {
                    if comm.rank() == 0 {
                        let e = Error::invalid("fatal callback");
                        Err(if typed {
                            Error::fatal(FatalKind::Agreed, e)
                        } else {
                            e
                        })
                    } else {
                        Ok(())
                    }
                };
                let result: akantu_core::Result<()> = dm.collective("outer", |d| match path {
                    "step" => d.step("callback", |_| local()),
                    "closing" => local(),
                    _ => d.agree_outcome("native_outcome", local()),
                });
                let next = dm.owned_norm(&[1.0]);
                (
                    result.unwrap_err().fatal_kind(),
                    dm.poison_kind(),
                    next.map_err(|e| e.fatal_kind()),
                )
            });
            for (rank, o) in out.iter().enumerate() {
                if typed {
                    let a = Some(FatalKind::Agreed);
                    assert_eq!(*o, (a, a, Err(a)), "{path}: rank {rank}");
                } else {
                    assert_eq!(*o, (None, None, Ok(2f64.sqrt())), "{path}: rank {rank}");
                }
            }
        }
    }
}

/// Typed fatal results through the native solve paths, on either rank:
/// a lifecycle callback (`predictor`), a trampoline callback
/// (`corrector`, inside `form_function`/`form_jacobian`) and the native
/// driver's own outcome. Every rank returns `Error::Fatal { kind: Agreed }`
/// and every rank's manager is poisoned; a second solve is refused
/// locally on both.
#[test]
fn typed_fatal_native_solve_results_poison_every_rank() {
    for path in ["predictor", "corrector", "driver_outcome"] {
        for bad in [0, 1] {
            let out = two_ranks(|comm| {
                let me = comm.rank() == bad;
                let mut dm = native_dm(&comm, |b| {
                    if me && path == "driver_outcome" {
                        b.fatal_outcome = Some(FatalKind::Agreed);
                    }
                });
                let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
                p.setup(&mut dm);
                p.fatal = Some(FatalKind::Agreed);
                p.fail_predictor = me && path == "predictor";
                p.fail_corrector = me && path == "corrector";
                let r = NativeNonLinear::new("J").solve(&mut dm, &mut p);
                let again = NativeNonLinear::new("J").solve(&mut dm, &mut p);
                (
                    r.unwrap_err().fatal_kind(),
                    dm.poison_kind(),
                    again.unwrap_err().fatal_kind(),
                )
            });
            let a = Some(FatalKind::Agreed);
            for (rank, o) in out.iter().enumerate() {
                assert_eq!(*o, (a, a, a), "{path}, bad rank {bad}: rank {rank}");
            }
        }
    }
}

/// A callback (or the native driver) returning `Error::Fatal { kind:
/// Transport }` keeps the transport kind and takes the abort path: the
/// reporting rank never attempts another agreement on the communicator
/// (it returns at once, poisoned with the transport kind, the
/// communicator broken), while its peer — still waiting in the step's
/// agreement — observes the transport failure itself (short diagnostic
/// timeout).
#[test]
fn typed_transport_fatal_results_take_the_abort_path() {
    for path in ["step", "driver_outcome"] {
        let out = run_local(2, Duration::from_millis(300), |comm| {
            let t0 = Instant::now();
            let me = comm.rank() == 0;
            let r = if path == "step" {
                let mut dm = DofManager::new("dm", comm.clone());
                dm.register_generic_dofs("u", 1, 1).unwrap();
                let r: akantu_core::Result<()> = dm.collective("outer", |d| {
                    d.step("callback", |_| {
                        if me {
                            Err(Error::fatal(
                                FatalKind::Transport,
                                Error::Communication("dependent owner lost its peers".into()),
                            ))
                        } else {
                            Ok(())
                        }
                    })
                });
                (r.unwrap_err(), dm.poison_kind())
            } else {
                let mut dm = native_dm(&comm, |b| {
                    if me {
                        b.fatal_outcome = Some(FatalKind::Transport);
                    }
                });
                let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
                p.setup(&mut dm);
                let r = NativeNonLinear::new("J").solve(&mut dm, &mut p);
                (r.unwrap_err(), dm.poison_kind())
            };
            let fast = t0.elapsed() < Duration::from_millis(200);
            (
                r.0.fatal_kind(),
                r.1,
                comm.broken().is_some(),
                fast,
                r.0.to_string().contains("timed out"),
            )
        });
        let t = Some(FatalKind::Transport);
        assert_eq!(
            out[0],
            (t, t, true, true, false),
            "{path}: no agreement attempted"
        );
        assert_eq!(out[1].0, t, "{path}");
        assert_eq!((out[1].1, out[1].2, out[1].4), (t, true, true), "{path}");
    }
}

// ------------------------------------------------------------------

/// A trampoline callback fails first (the corrector inside
/// `form_function`, on rank `first`, ordinary or typed agreed-fatal), then
/// the native driver of rank `bad` returns a late outcome while tearing down
/// the failed solve (ordinary, agreed-fatal or transport-fatal), without
/// any further trampoline or collective. The stored callback error keeps
/// its diagnostic but no longer hides a more urgent driver fatal kind:
///
/// * a transport-fatal driver outcome, or an agreed-fatal one after a
///   recoverable callback failure (no longer agreeable: the peers already
///   returned), takes the abort path on the reporting rank — `Transport`
///   returned and stored as the poison kind (upgraded from an earlier
///   agreed poisoning), communicator broken;
/// * an agreed-fatal driver outcome after an agreed-fatal callback failure
///   stays `Agreed`;
/// * an ordinary driver error keeps the callback's classification and
///   diagnostic priority.
///
/// The peers keep the class of the callback failure they observed. Every
/// combination of failing ranks; nothing waits for a timeout.
#[test]
fn late_driver_outcomes_keep_the_most_urgent_fatal_kind() {
    let t = Some(FatalKind::Transport);
    let a = Some(FatalKind::Agreed);
    for callback in [None, a] {
        for late in [None, a, t] {
            for (first, bad) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let out = two_ranks(|comm| {
                    let mut dm = native_dm(&comm, |b| {
                        if comm.rank() == bad {
                            b.late_outcome = Some(late);
                        }
                    });
                    let mut p = Springs::new(two_rank_bar(&comm), 4.0, false);
                    p.setup(&mut dm);
                    p.fatal = callback;
                    p.fail_corrector = comm.rank() == first;
                    let e = NativeNonLinear::new("J")
                        .solve(&mut dm, &mut p)
                        .unwrap_err();
                    (
                        (e.fatal_kind(), dm.poison_kind(), comm.broken().is_some()),
                        e.to_string(),
                    )
                });
                let observed = (callback, callback, false);
                let escalated = match (callback, late) {
                    (_, Some(FatalKind::Transport)) | (None, Some(FatalKind::Agreed)) => {
                        (t, t, true)
                    }
                    _ => observed,
                };
                for (rank, (o, msg)) in out.iter().enumerate() {
                    let reporting = rank == usize::try_from(bad).unwrap();
                    let ctx = format!(
                        "callback {callback:?}, late {late:?}, first {first}, bad {bad}: rank {rank}"
                    );
                    assert_eq!(*o, if reporting { escalated } else { observed }, "{ctx}");
                    // The callback's diagnostic is kept on every rank.
                    let diag = if rank == usize::try_from(first).unwrap() {
                        "injected corrector failure"
                    } else {
                        "failed on another rank"
                    };
                    assert!(msg.contains(diag), "{ctx}: {msg}");
                }
            }
        }
    }
}
