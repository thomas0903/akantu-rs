// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (`docs/ARCHITECTURE.md`) of the solver stack
// ported from Akantu (EPFL/LSMS) src/model/common/{non_linear_solver/,
// time_step_solvers/, dof_manager/} on the upstream MyModel toy
// (test/test_model/test_common/test_model_solver/test_model_solver_my_model.hh)
// distributed over the in-process local world.

//! Collective protocol of every implemented solver, at 1, 2 and 4 ranks of
//! the local world:
//!
//! * **rank-specific failure at each callback** (ARCHITECTURE.md, "Non-linear solvers, time stepping and switches"): the toy fails in
//!   one callback on one rank only (first or last rank); every rank's
//!   `solve_step` returns an error promptly (the failing rank its own
//!   error, the others "failed on another rank"), nothing hangs, the
//!   manager is not poisoned and the next step succeeds on every rank;
//! * **distributed numbering** of a partitioned bar (owned DOFs numbered in
//!   rank order per block, shared DOFs carry their owner's number);
//! * **partition independence**: static Newton–Raphson (linear and cubic
//!   bar), implicit Newmark and explicit central-difference histories
//!   (iteration counts, errors, gathered `u`, `u̇`, `ü`) equal the serial
//!   run's.

mod common;

use akantu_validation::reduce::NanMinMax;
use std::time::{Duration, Instant};

use akantu_comm::{Communicator, Synchronizer, run_local};
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Error, GhostType, Idx, NodeFlag, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};

use common::my_model::{Law, MyModel, SEG};

const TIMEOUT: Duration = Duration::from_secs(20);
const PROMPT: Duration = Duration::from_secs(5);
const N: usize = 17; // 16 elements: 16, 8, 4 per rank at 1, 2, 4 ranks

/// Rank `r` of `p` holds global nodes `[r·e, (r+1)·e]` (`e = (N−1)/p`); the
/// boundary node is master on the lower rank and slave on the upper one.
fn partitioned_bar(comm: &Communicator) -> (Mesh, Vec<i64>) {
    let (r, p) = (comm.rank() as usize, comm.size());
    let e = (N - 1) / p;
    let first = r * e;
    let mut mesh = if p == 1 {
        Mesh::new(1, "bar").unwrap()
    } else {
        Mesh::with_communicator(1, "bar", comm.clone()).unwrap()
    };
    let mut gids = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for i in 0..=e {
            let g = first + i;
            let n = acc.push_node(&[g as f64 * (1.0 / (N - 1) as f64)]).unwrap();
            gids.push(g as i64);
            let flag = if p > 1 && i == 0 && r > 0 {
                NodeFlag::SLAVE
            } else if p > 1 && i == e && r + 1 < p {
                NodeFlag::MASTER
            } else {
                NodeFlag::NORMAL
            };
            acc.nodes_flags_mut().as_mut_slice()[n as usize] = flag;
        }
        let mut c = acc.connectivity_mut(SEG, GhostType::NotGhost);
        for i in 0..e {
            c.push_tuple(&[i as Idx, i as Idx + 1]);
        }
        drop(c);
        if p > 1 {
            let mut sync = Synchronizer::new("nodes", comm.clone());
            if r + 1 < p {
                sync.communications
                    .send
                    .insert(r as i32 + 1, vec![e as Idx]);
            }
            if r > 0 {
                sync.communications.recv.insert(r as i32 - 1, vec![0]);
                acc.set_node_prank(0, r as i32 - 1);
            }
            acc.synchronizers_mut().node = Some(sync);
            acc.set_distributed();
        }
    }
    (mesh, gids)
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Setup {
    StaticNewton,
    StaticLinearSolver,
    StaticCubicNewton,
    ImplicitNewmark,
    ExplicitLumped,
}

/// Build the toy and its solver on `comm` (sinusoidal initial state for the
/// dynamic setups, computed locally from the global coordinate).
fn build(comm: &Communicator, setup: Setup) -> (MyModel, ModelSolver) {
    let (mesh, gids) = partitioned_bar(comm);
    let x: Vec<f64> = mesh.nodes().as_slice().to_vec();
    let mut dm = DofManager::new("dm", comm.clone());
    let lumped = setup == Setup::ExplicitLumped;
    let force = match setup {
        Setup::StaticCubicNewton => -2.0,
        Setup::StaticNewton | Setup::StaticLinearSolver => -10.0,
        _ => 0.0,
    };
    let mut model =
        MyModel::with_global_ids(force, mesh, lumped, &mut dm, gids.clone(), N as i64).unwrap();
    if setup == Setup::StaticCubicNewton {
        model.law = Law::Cubic;
    }
    let dynamic = matches!(setup, Setup::ImplicitNewmark | Setup::ExplicitLumped);
    if dynamic {
        let mut b_g = dm.blocked_dofs_mut("disp").unwrap();
        let b = b_g.as_mut_slice();
        for (k, g) in gids.iter().enumerate() {
            b[k] = *g == 0 || *g == N as i64 - 1;
        }
        let k = 2.0 * std::f64::consts::PI / 0.2;
        let u: Vec<f64> = x.iter().map(|&xi| 0.01 * (k * xi).cos()).collect();
        let v: Vec<f64> = x.iter().map(|&xi| k * 0.01 * (k * xi).sin()).collect();
        drop(b_g);
        dm.dofs_mut("disp")
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&u);
        dm.dofs_derivative_mut("disp", 1)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&v);
    }
    let mut ms = ModelSolver::new(dm);
    let (tss, nls) = match setup {
        Setup::StaticNewton | Setup::StaticCubicNewton => (
            TimeStepSolverType::Static,
            NonLinearSolverType::NewtonRaphson,
        ),
        Setup::StaticLinearSolver => (TimeStepSolverType::Static, NonLinearSolverType::Linear),
        Setup::ImplicitNewmark => (
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
        ),
        Setup::ExplicitLumped => (
            TimeStepSolverType::DynamicLumped,
            NonLinearSolverType::Lumped,
        ),
    };
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(tss),
        non_linear_solver_type: Some(nls),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, &mut model).unwrap();
    let (scheme, st) = match setup {
        Setup::ImplicitNewmark => (
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
        ),
        Setup::ExplicitLumped => (
            IntegrationSchemeType::CentralDifference,
            SolutionType::Acceleration,
        ),
        _ => (IntegrationSchemeType::PseudoTime, SolutionType::NotDefined),
    };
    ms.set_integration_scheme("s", "disp", scheme, st, &model)
        .unwrap();
    ms.set_time_step(0.001, None).unwrap();
    if nls == NonLinearSolverType::NewtonRaphson {
        ms.non_linear_solver_mut(None)
            .unwrap()
            .set_param("max_iterations", ParamValue::Int(20))
            .unwrap();
    }
    (model, ms)
}

/// Histories of `steps` solve steps: per step `(iterations, error)` and the
/// final `(u, u̇, ü)` keyed by global node id.
type Histories = (Vec<(usize, Real)>, Vec<(i64, [Real; 3])>);

fn run(comm: &Communicator, setup: Setup, steps: usize) -> Histories {
    let (mut model, mut ms) = build(comm, setup);
    let mut hist = Vec::new();
    for _ in 0..steps {
        ms.solve_step(&mut model, None).unwrap();
        let nls = ms.non_linear_solver(None).unwrap();
        hist.push((nls.nb_iterations(), nls.error()));
    }
    let dm = &ms.dof_manager;
    let u = dm.dofs("disp").unwrap();
    let u = u.as_slice();
    let v = dm.dofs_derivative("disp", 1).unwrap();
    let v = v.as_slice();
    let a = dm.dofs_derivative("disp", 2).unwrap();
    let a = a.as_slice();
    let state = model
        .global_ids
        .iter()
        .enumerate()
        .map(|(k, &g)| (g, [u[k], v[k], a[k]]))
        .collect();
    (hist, state)
}

fn on_ranks<T: Send>(p: usize, f: impl Fn(Communicator) -> T + Sync) -> Vec<T> {
    let t0 = Instant::now();
    let out = run_local(p, TIMEOUT, f);
    let dt = t0.elapsed();
    assert!(
        dt < PROMPT,
        "{p} ranks took {dt:?}: a rank waited for a timeout"
    );
    out
}

#[test]
fn distributed_numbering_partitioned_bar_1_2_4_ranks() {
    for p in [1, 2, 4] {
        let out = on_ranks(p, |comm| {
            let (mesh, gids) = partitioned_bar(&comm);
            let mut dm = DofManager::new("dm", comm.clone());
            dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
            let globals = dm.global_equation_numbers("u").unwrap();
            (gids, globals, dm.system_size(), dm.pure_local_system_size())
        });
        let mut owned_total = 0;
        for (gids, globals, size, owned) in &out {
            assert_eq!(*size, 2 * N as i64, "{p} ranks: system size");
            owned_total += owned;
            // contiguous partitions, masters on the lower rank: the global
            // equations follow the global node order (components innermost)
            for (k, g) in gids.iter().enumerate() {
                for c in 0..2 {
                    assert_eq!(
                        globals[2 * k + c],
                        Some(2 * g + c as i64),
                        "{p} ranks: node {g}"
                    );
                }
            }
        }
        assert_eq!(owned_total, 2 * N, "{p} ranks: every DOF owned once");
    }
}

fn gather(states: Vec<Histories>) -> (Vec<(usize, Real)>, Vec<[Real; 3]>) {
    let hist = states[0].0.clone();
    for s in &states {
        assert_eq!(s.0.len(), hist.len());
        for (a, b) in s.0.iter().zip(&hist) {
            assert_eq!(a.0, b.0, "iteration counts are common knowledge");
            assert_eq!(a.1.to_bits(), b.1.to_bits(), "errors are common knowledge");
        }
    }
    let mut g = vec![None; N];
    for s in states {
        for (gid, val) in s.1 {
            let slot = &mut g[gid as usize];
            if let Some(prev) = slot {
                let prev: [Real; 3] = *prev;
                for c in 0..3 {
                    assert_eq!(
                        prev[c].to_bits(),
                        val[c].to_bits(),
                        "shared node {gid} differs between ranks"
                    );
                }
            }
            *slot = Some(val);
        }
    }
    (hist, g.into_iter().map(Option::unwrap).collect())
}

/// `exact`: bit identity expected (no distributed matrix–vector product in
/// the run). Otherwise (implicit Newmark: `r −= M·ü` through the corrected
/// distributed product, UD-003) the distributed path accumulates every row
/// in place like the serial one, but a shared row adds the other
/// ranks' partial sums to the owner's `β·y + own partial` in rank order,
/// where the serial path adds the terms one by one in its own storage
/// order, and the multi-rank storage order differs too. Per
/// product a row differs by at most `γ_{k+1}(|β y| + Σ|α a x|)` (`k ≤ 3`
/// stored terms per row of the bar: ≤ 4u relative), ~3 products per step,
/// mapped through `J⁻¹` with `κ₂(J) ≈ κ₂(M) ≈ 3` (`J ≈ 4/Δt²·M`):
/// ≲ 4e-15 per step, accumulating at most linearly (non-dissipative
/// scheme). Rule (NUMERICAL_VALIDATION):
/// `max|Δ| ≤ 1e-13 · max|field|` per component for the 5-step test,
/// scaled with the step count for longer runs (`1e-13 · steps / 5`),
/// with identical iteration counts. Measured: 2.7e-16 (5 steps) … 2.4e-15
/// (400 steps).
fn assert_partition_independent(setup: Setup, steps: usize, exact: bool) {
    // long histories are not failure tests: no promptness bound
    let ranks = |p: usize| {
        if steps > 50 {
            run_local(p, Duration::from_secs(300), |c| run(&c, setup, steps))
        } else {
            on_ranks(p, |c| run(&c, setup, steps))
        }
    };
    let serial = gather(ranks(1));
    for p in [2, 4] {
        let par = gather(ranks(p));
        assert_eq!(
            par.0.iter().map(|h| h.0).collect::<Vec<_>>(),
            serial.0.iter().map(|h| h.0).collect::<Vec<_>>(),
            "{setup:?}: iteration counts at {p} ranks"
        );
        let mut worst = 0.0f64;
        let mut differ = 0;
        let scale: Vec<f64> = (0..3)
            .map(|c| serial.1.iter().fold(0.0f64, |m, v| m.nan_max(v[c].abs())))
            .collect();
        for (a, b) in par.1.iter().zip(&serial.1) {
            for c in 0..3 {
                if a[c] != b[c] {
                    differ += 1;
                    worst = worst.nan_max((a[c] - b[c]).abs() / scale[c].max(f64::MIN_POSITIVE));
                }
            }
        }
        let errs_equal = par.0.iter().zip(&serial.0).all(|(a, b)| a.1 == b.1);
        println!(
            "{setup:?} at {p} ranks: {differ} of {} state values differ from serial (max rel {worst:e}); errors equal: {errs_equal}",
            3 * N
        );
        // The contiguous partition reproduces the serial numbering, every
        // shared-node sum keeps the serial order (left element, then right)
        // and the centralised solve sees the serial triplets in rank order,
        // so the runs are expected to be identical.
        if exact {
            assert_eq!(differ, 0, "{setup:?} at {p} ranks");
            assert!(errs_equal, "{setup:?} at {p} ranks: errors");
        } else {
            let bound = 1e-13 * (steps as f64 / 5.0).max(1.0);
            assert!(
                worst <= bound,
                "{setup:?} at {p} ranks, {steps} steps: max rel {worst:e} > {bound:e}"
            );
        }
    }
}

#[test]
fn partition_independence_static_newton() {
    assert_partition_independent(Setup::StaticNewton, 1, true);
}

#[test]
fn partition_independence_static_cubic_newton() {
    assert_partition_independent(Setup::StaticCubicNewton, 1, true);
}

#[test]
fn partition_independence_static_linear_solver() {
    assert_partition_independent(Setup::StaticLinearSolver, 1, true);
}

#[test]
fn partition_independence_implicit_newmark() {
    assert_partition_independent(Setup::ImplicitNewmark, 5, false);
}

/// Growth of the partition difference with the step count
/// (at most linear); the bound scales as `1e-13 · steps / 5`.
#[test]
fn partition_independence_implicit_newmark_400_steps() {
    assert_partition_independent(Setup::ImplicitNewmark, 400, false);
}

#[test]
fn partition_independence_explicit_central_difference() {
    assert_partition_independent(Setup::ExplicitLumped, 20, true);
}

fn remote_failure(e: &Error) -> bool {
    matches!(e, Error::Communication(m) if m.contains("failed on another rank"))
}

/// The callbacks the toy receives in one step of `setup` (serial trace).
fn callbacks(setup: Setup) -> Vec<String> {
    let comm = Communicator::serial();
    let (mut model, mut ms) = build(&comm, setup);
    ms.solve_step(&mut model, None).unwrap();
    let mut names = model.trace.clone();
    names.sort();
    names.dedup();
    names
}

/// ARCHITECTURE.md, "Non-linear solvers, time stepping and switches" failure matrix: one rank fails in one callback; returns one row per
/// (callback, ranks, failing rank).
fn failure_matrix(setup: Setup) -> Vec<String> {
    let mut rows = Vec::new();
    for cb in callbacks(setup) {
        for p in [1usize, 2, 4] {
            let bads = if p == 1 { vec![0] } else { vec![0, p - 1] };
            for bad in bads {
                let cbn = cb.clone();
                let out = on_ranks(p, move |comm| {
                    let (mut model, mut ms) = build(&comm, setup);
                    if comm.rank() as usize == bad {
                        model.fail_at = Some(cbn.clone());
                    }
                    let first = ms.solve_step(&mut model, None);
                    model.fail_at = None;
                    let poisoned = ms.dof_manager.poisoned().is_some();
                    // the operation failed everywhere and nothing is
                    // poisoned: the next step succeeds on every rank
                    let second = ms.solve_step(&mut model, None);
                    (first, poisoned, second)
                });
                for (rank, (first, poisoned, second)) in out.iter().enumerate() {
                    let e = first.as_ref().err().unwrap_or_else(|| {
                        panic!("{setup:?}/{cb}/{p} ranks: rank {rank} succeeded although rank {bad} failed")
                    });
                    if rank == bad {
                        assert!(
                            e.to_string().contains("injected failure"),
                            "{setup:?}/{cb}: failing rank's own error, got {e}"
                        );
                    } else {
                        assert!(remote_failure(e), "{setup:?}/{cb}: rank {rank} got {e}");
                    }
                    assert!(
                        !poisoned,
                        "{setup:?}/{cb}: ordinary failure poisoned the manager"
                    );
                    assert!(
                        second.is_ok(),
                        "{setup:?}/{cb}/{p}: next step on rank {rank}: {second:?}"
                    );
                }
                rows.push(format!(
                    "{setup:?} | {cb} | {p} | {bad} | all ranks fail, next step ok"
                ));
            }
        }
    }
    rows
}

#[test]
fn rank_specific_failures_static_newton() {
    let rows = failure_matrix(Setup::StaticNewton);
    assert!(rows.len() >= 5 * 5, "{rows:?}");
    rows.iter().for_each(|r| println!("{r}"));
}

#[test]
fn rank_specific_failures_static_linear_solver() {
    let rows = failure_matrix(Setup::StaticLinearSolver);
    assert!(rows.len() >= 5 * 5, "{rows:?}");
    rows.iter().for_each(|r| println!("{r}"));
}

#[test]
fn rank_specific_failures_implicit_newmark() {
    let rows = failure_matrix(Setup::ImplicitNewmark);
    assert!(rows.len() >= 6 * 5, "{rows:?}");
    rows.iter().for_each(|r| println!("{r}"));
}

#[test]
fn rank_specific_failures_explicit_lumped() {
    let rows = failure_matrix(Setup::ExplicitLumped);
    assert!(rows.len() >= 6 * 5, "{rows:?}");
    rows.iter().for_each(|r| println!("{r}"));
}

/// `get_new_solver` is one agreed collective operation.
/// The model's `init_solver` fails on one rank only (first or last, at 1,
/// 2 and 4 ranks): every rank returns an error promptly (the failing rank
/// its own, the others "failed on another rank"), no rank registers the
/// solver, the default-solver id (set before validation, upstream quirk
/// UD-201) is set identically everywhere, nothing is poisoned, and the
/// retried construction and a solve then succeed on every rank. Before
/// the fix rank 0 returned `Ok` and its next solve timed out
/// (`Fatal(Transport)`, poisoned manager).
#[test]
fn rank_specific_init_solver_failure_is_agreed() {
    for p in [1usize, 2, 4] {
        let bads = if p == 1 { vec![0] } else { vec![0, p - 1] };
        for bad in bads {
            let out = on_ranks(p, move |comm| {
                let (mesh, gids) = partitioned_bar(&comm);
                let mut dm = DofManager::new("dm", comm.clone());
                let mut model =
                    MyModel::with_global_ids(-10.0, mesh, false, &mut dm, gids, N as i64).unwrap();
                let mut ms = ModelSolver::new(dm);
                if comm.rank() as usize == bad {
                    model.fail_at = Some("init_solver".into());
                }
                let opts = ModelSolverOptions {
                    time_step_solver_type: Some(TimeStepSolverType::Static),
                    non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
                    sparse_solver_type: Some(SparseSolverType::Eigen),
                    ..ModelSolverOptions::default()
                };
                let first = ms.get_new_solver("s", &opts, &mut model);
                let registered = ms.has_solver("s");
                let default = ms.has_default_solver();
                let poisoned = ms.dof_manager.poisoned().is_some();
                model.fail_at = None;
                let retry = ms.get_new_solver("s", &opts, &mut model);
                let solved = retry.as_ref().ok().map(|()| {
                    ms.set_integration_scheme(
                        "s",
                        "disp",
                        IntegrationSchemeType::PseudoTime,
                        SolutionType::NotDefined,
                        &model,
                    )
                    .and_then(|()| ms.solve_step(&mut model, None))
                });
                (first, registered, default, poisoned, retry, solved)
            });
            for (rank, (first, registered, default, poisoned, retry, solved)) in
                out.iter().enumerate()
            {
                let e = first.as_ref().err().unwrap_or_else(|| {
                    panic!("{p} ranks: rank {rank} succeeded although rank {bad} failed")
                });
                if rank == bad {
                    assert!(e.to_string().contains("injected failure"), "own error: {e}");
                } else {
                    assert!(remote_failure(e), "rank {rank}: {e}");
                }
                assert!(!registered, "{p}/{bad}: rank {rank} registered the solver");
                assert!(default, "{p}/{bad}: default id set on every rank (UD-201)");
                assert!(!poisoned, "{p}/{bad}: rank {rank} poisoned");
                assert!(retry.is_ok(), "{p}/{bad}: retry on rank {rank}: {retry:?}");
                assert!(
                    matches!(solved, Some(Ok(()))),
                    "{p}/{bad}: solve on rank {rank}: {solved:?}"
                );
            }
            println!("init_solver | {p} | {bad} | all ranks fail, nothing registered, retry ok");
        }
    }
}
