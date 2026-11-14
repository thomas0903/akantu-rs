// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): scenarios derived from python/py_aka_array.hh
// (views over Array storage), examples/python/.../bi-material.py (views of
// internals and getModel() inside material callbacks), and the upstream
// lifetime hazards recorded as UD-024 / UD-085.

//! Foreign-view contract: retained views are memory-safe
//! through insertion, history restore, owner destruction, slicing and
//! shrink/re-grow; native element borrows and foreign accesses are mutually
//! exclusive (two adversarial programs are rejected); the
//! phase protocol orders concurrent threads. The "foreign" side is Rust
//! here; run under Miri for aliasing and data-race soundness:
//! `cargo +nightly miri test -p akantu-python` (Stacked Borrows) and with
//! `MIRIFLAGS=-Zmiri-tree-borrows`.
//!
//! Every multi-threaded test runs under [`support::watchdog`] and
//! synchronises through [`support::Team`] (timed, broken by a panicking
//! party), so a failure or a deadlock fails the test instead of hanging the
//! binary. The single-threaded tests block on nothing (only uncontended
//! locks, and the protocol refuses instead of waiting).

mod support;

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use support::{Jitter, SpinDeadline, Team, watchdog};

use akantu::core::array_shared::{ForeignRuntime, SharedArray};
use akantu::core::enums::EventPriority;
use akantu::core::{Array, Element, ElementType, Error, GhostType, Idx, Real, Result};
use akantu::mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};
use akantu::model::constitutive_law::{LawContext, LawPhase, LawSpec, ModelServices};
use akantu::solid::{
    ForeignLaw, ForeignLawState, ForeignMaterial, ForeignMaterialMethods, Material,
};
use py11_akantu::foreign_view::{ForeignView, SharedOwner, foreign_callback};
use py11_akantu::scoped::{
    ContextKey, Scoped, ScopedMut, SolverAccess, with_model_services, with_scoped, with_scoped_mut,
    with_solver_access,
};

const SEG: ElementType = ElementType::Segment2;

fn bar() -> SharedMesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [0.0, 1.0, 2.0] {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(SEG, GhostType::NotGhost);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
    }
    SharedMesh::new(m)
}

fn shared(values: &[f64], nbc: usize) -> SharedArray<f64> {
    SharedArray::new(Array::from_vec(values.to_vec(), nbc).unwrap())
}

/// A nodal field consumer standing in for the DOF manager's displacement
/// (upstream `DOFManager::onNodesAdded` → `resizeGlobalArrays` copies the
/// values of the doubled nodes).
struct NodalField {
    u: SharedArray<f64>,
}

impl MeshEventHandler for NodalField {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority::DOF_MANAGER
    }
    fn on_mesh_event(&mut self, _: &Mesh, e: &MeshEvent) -> Result<()> {
        if let MeshEvent::NodesAdded {
            nodes, old_nodes, ..
        } = e
        {
            let mut u = self.u.write()?; // needs the native section
            for (k, _) in nodes.iter().enumerate() {
                let v = match old_nodes {
                    Some(old) => u.get_tuple(usize::try_from(old[k]).unwrap())?[0],
                    None => 0.0,
                };
                u.try_push_tuple(&[v])?;
            }
        }
        Ok(())
    }
}

// ---- lifetime: insertion, history restore, owner destruction --------------

#[test]
fn view_survives_cohesive_insertion_stale_but_safe() {
    let rt = ForeignRuntime::new();
    let mesh = bar();
    let mut field = NodalField {
        u: SharedArray::new(Array::zeros(3, 1)),
    };
    let token = mesh
        .register_consumer("dofs", EventPriority::DOF_MANAGER, MeshPart::Main)
        .unwrap();
    // u = model.getDisplacement(); u[1, 0] = 0.5  (Python, foreign phase)
    let u = ForeignView::new(&mut field.u, &rt, true).unwrap();
    let mut interp = rt.enter().unwrap();
    u.write(&interp, 1, 0, 0.5).unwrap();
    {
        // inserter.checkCohesiveStress(): a binding call = native section.
        let _native = interp.native().unwrap();
        assert_eq!(
            field.u.read().unwrap().as_slice()[1],
            0.5,
            "writes reach Rust"
        );
        let mut handlers: Vec<(&_, &mut dyn MeshEventHandler)> = vec![(&token, &mut field)];
        mesh.mutate(
            &mut handlers,
            |tx| {
                let n = tx.duplicate_node(MeshPart::Main, 1)?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: Some(vec![1]),
                        origin: "inserter".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    }
    // The old view: memory valid, old values, detached, rejected as output.
    assert!(u.is_stale() && !u.is_attached_to(&field.u));
    assert_eq!(u.shape(), (3, 1));
    assert_eq!(u.read(&interp, 1, 0).unwrap(), 0.5);
    assert!(u.require_attached(&field.u).is_err());
    u.write(&interp, 0, 0, 9.0).unwrap(); // reaches the retired block only
    // Re-fetching gives an attached view of the new storage, with the copy.
    let u2 = ForeignView::new(&mut field.u, &rt, true).unwrap();
    assert_eq!(u2.shape(), (4, 1));
    assert!(u2.is_attached_to(&field.u));
    assert_eq!(
        u2.read(&interp, 0, 0).unwrap(),
        0.0,
        "stale write not visible"
    );
    assert_eq!(u2.read(&interp, 3, 0).unwrap(), 0.5, "doubled node copied");
}

#[test]
fn view_stays_attached_through_history_restore() {
    let rt = ForeignRuntime::new();
    // InternalField with history: current + previous of equal size.
    let mut current = SharedArray::new(Array::<f64>::zeros(2, 1));
    let mut previous = SharedArray::new(Array::<f64>::zeros(2, 1));
    let view = ForeignView::new(&mut current, &rt, true).unwrap();
    let mut interp = rt.enter().unwrap();
    view.write(&interp, 0, 0, 0.25).unwrap();
    {
        let _n = interp.native().unwrap();
        // saveCurrentValues
        previous
            .write()
            .unwrap()
            .copy_from(&current.read().unwrap())
            .unwrap();
    }
    view.write(&interp, 0, 0, 0.75).unwrap();
    {
        let _n = interp.native().unwrap();
        // restorePreviousValues: equal-length copy, in place
        current
            .write()
            .unwrap()
            .copy_from(&previous.read().unwrap())
            .unwrap();
    }
    assert!(view.is_attached_to(&current), "restore copies in place");
    assert_eq!(
        view.read(&interp, 0, 0).unwrap(),
        0.25,
        "view sees the restored value"
    );
}

#[test]
fn view_outlives_its_owner() {
    let rt = ForeignRuntime::new();
    struct Model {
        u: SharedArray<f64>,
    }
    let mut model = Model {
        u: shared(&[0.0, 0.0, 3.0], 1),
    };
    let view = ForeignView::new(&mut model.u, &rt, false).unwrap();
    drop(model); // `del model` while Python still holds the array
    assert!(view.is_stale());
    let interp = rt.enter().unwrap();
    assert_eq!(view.read(&interp, 2, 0).unwrap(), 3.0);
    assert!(view.write(&interp, 2, 0, 1.0).is_err(), "read-only view");
}

// ---- adversarial programs -------------------------------------------

/// Adversarial program 1 (retain `a.as_slice()`, write through the view,
/// use the slice). The compile-time form is a `compile_fail` doctest in
/// `src/foreign_view.rs`; here the section is released early so that the
/// program compiles, and the write is rejected at run time.
#[test]
fn reviewer_program_1_slice_then_foreign_write_is_rejected() {
    let rt = ForeignRuntime::new();
    let mut a = SharedArray::new(Array::<f64>::zeros(1, 1));
    let v = ForeignView::new(&mut a, &rt, true).unwrap();
    assert!(
        a.read().is_err(),
        "no native access outside a native section"
    );
    let mut interp = rt.enter().unwrap();
    let section = interp.native().unwrap();
    let s = a.read().unwrap(); // native slice of the exported allocation
    drop(section); // ends the section while `s` is alive: poisoned
    assert!(
        v.write(&interp, 0, 0, 2.0).is_err(),
        "rejected, nothing written"
    );
    assert_eq!(s.as_slice()[0], 0.0, "the native slice is still valid");
    drop(s);
    v.write(&interp, 0, 0, 2.0).unwrap(); // now admitted
    let _n = interp.native().unwrap();
    assert_eq!(a.read().unwrap().as_slice()[0], 2.0);
}

/// Adversarial program 2 (worker holds the owner and writes natively;
/// the main thread writes through an existing view, barrier-unordered).
/// The worker must enter the runtime to obtain native access, so the main
/// thread cannot hold the interpreter at the same time: its write is
/// rejected, and the later admitted write is ordered after the worker's.
#[test]
fn reviewer_program_2_worker_write_vs_retained_view_is_excluded() {
    watchdog(|| {
        let rt = ForeignRuntime::new();
        let owner = Arc::new(SharedOwner::new(SharedArray::new(Array::<f64>::zeros(
            1, 1,
        ))));
        let v = ForeignView::new(&mut owner.try_borrow_mut().unwrap(), &rt, true).unwrap();
        let team = Arc::new(Team::new(2));
        let worker = {
            let (o, rt, team) = (Arc::clone(&owner), rt.clone(), Arc::clone(&team));
            std::thread::spawn(move || {
                let _m = team.member();
                let mut a = o.try_borrow_mut().unwrap();
                let mut interp = rt.enter().unwrap(); // binding entry on the worker
                let _native = interp.native().unwrap();
                team.wait("entered");
                a.write().unwrap().as_mut_slice()[0] = 1.0;
                team.wait("done");
            })
        };
        let _m = team.member();
        team.wait("entered");
        // Main thread: the runtime (GIL stand-in) is held by the worker.
        assert!(
            rt.enter().is_err(),
            "foreign side excluded during native work"
        );
        team.wait("done");
        worker.join().unwrap();
        let interp = rt.enter().unwrap();
        assert_eq!(
            v.read(&interp, 0, 0).unwrap(),
            1.0,
            "ordered after the worker"
        );
        v.write(&interp, 0, 0, 2.0).unwrap();
    });
}

/// Program 2 with the section on the interpreter thread and the native
/// write on a scoped worker whose guard outlives the section: while the
/// guard is alive the runtime is poisoned and foreign accesses are
/// **refused** (not blocked); after its release they are admitted and
/// ordered after the worker's write.
///
/// The worker may use and release its guard only after the refusal has been
/// checked (step `refused`). An earlier version let it release right after
/// the section ended, so the refusal assertion raced with a legitimate
/// release: when the release won, the (correctly) admitted write failed the
/// assertion and the panicking test thread left the worker blocked on a
/// `Barrier`, i.e. `std::thread::scope` waited forever — the intermittent
/// hang of the test binary. That race is now exercised on purpose, with
/// checked invariants, by `worker_guard_release_races_foreign_access_stress`.
#[test]
fn native_guard_on_a_worker_blocks_foreign_writes_until_released() {
    watchdog(|| {
        let rt = ForeignRuntime::new();
        let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
        let v = ForeignView::new(&mut a, &rt, true).unwrap();
        let mut interp = rt.enter().unwrap();
        let team = Team::new(2);
        std::thread::scope(|s| {
            let section = interp.native().unwrap();
            let a = &mut a;
            let team = &team;
            s.spawn(move || {
                let _m = team.member();
                let mut g = a.write().unwrap();
                team.wait("taken");
                team.wait("refused"); // the guard has outlived its section
                g.as_mut_slice()[1] = 5.0;
                drop(g);
                team.wait("released");
            });
            let _m = team.member();
            team.wait("taken");
            drop(section); // section ends, worker guard still alive
            assert!(rt.is_poisoned() && rt.native_guards() == 1);
            assert!(v.write(&interp, 1, 0, 7.0).is_err(), "poisoned: refused");
            assert!(v.read(&interp, 1, 0).is_err(), "reads are refused too");
            team.wait("refused");
            team.wait("released");
            assert!(rt.is_foreign_phase() && !rt.is_poisoned());
            assert_eq!(v.read(&interp, 1, 0).unwrap(), 5.0, "after the release");
            v.write(&interp, 1, 0, 7.0).unwrap();
        });
    });
}

/// Stress regression for the poisoned window (randomised interleavings,
/// bounded by the watchdog). Each iteration: a worker takes one or two
/// native guards (on two arrays exported to the runtime) inside the
/// interpreter thread's section; the section ends; the refusal is checked
/// while the guards are known to be alive; then the worker writes and drops
/// its guards (random order, random delays) **concurrently** with either
/// foreign accesses from the poisoned runtime (mode A) or callback attempts
/// from a new section that adopts the leftover guards (mode B).
///
/// Invariants: every admission happens only after the worker's last guard
/// release and sees the worker's write; the worker's next guard sees the
/// foreign write; nothing blocks (refusals are retried with jitter).
#[test]
fn worker_guard_release_races_foreign_access_stress() {
    watchdog(|| {
        let iters: u32 = if cfg!(miri) { 20 } else { 1500 };
        let rt = ForeignRuntime::new();
        let mut a = SharedArray::new(Array::<f64>::zeros(2, 1));
        let mut b = SharedArray::new(Array::<f64>::zeros(1, 1));
        let va = ForeignView::new(&mut a, &rt, true).unwrap();
        let _vb = ForeignView::new(&mut b, &rt, false).unwrap(); // `b` is exported too
        let mut interp = rt.enter().unwrap();
        let team = Team::new(2);
        // Set by the worker right before it drops its last guard.
        let releasing = AtomicBool::new(false);
        let (mut refusals, mut mode_b) = (0u64, 0u32);
        std::thread::scope(|s| {
            let (a, b, team, releasing) = (&mut a, &b, &team, &releasing);
            s.spawn(move || {
                let _m = team.member();
                let mut rng = Jitter::new(0x5EED_0001);
                for k in 0..iters {
                    team.wait("section open");
                    rng.pause();
                    let mut g = a.write().unwrap();
                    let previous = if k == 0 { 0.0 } else { -f64::from(k - 1) };
                    assert_eq!(
                        g.as_slice()[1],
                        previous,
                        "iteration {k}: the foreign write is visible to the next guard"
                    );
                    let r = rng.coin().then(|| b.read().unwrap());
                    team.wait("taken");
                    team.wait("refused");
                    rng.pause();
                    g.as_mut_slice()[1] = f64::from(k);
                    match r {
                        Some(r) if rng.coin() => {
                            drop(r);
                            rng.pause();
                            releasing.store(true, Ordering::Release);
                            drop(g);
                        }
                        Some(r) => {
                            drop(g);
                            rng.pause();
                            releasing.store(true, Ordering::Release);
                            drop(r);
                        }
                        None => {
                            releasing.store(true, Ordering::Release);
                            drop(g);
                        }
                    }
                    team.wait("iteration end");
                }
            });
            let _m = team.member();
            let mut rng = Jitter::new(0x5EED_0002);
            for k in 0..iters {
                releasing.store(false, Ordering::Relaxed); // ordered by the team lock
                let section = interp.native().unwrap();
                team.wait("section open");
                team.wait("taken");
                rng.pause();
                drop(section); // worker guards alive: poisoned
                assert!(rt.is_poisoned(), "iteration {k}");
                assert!(
                    va.write(&interp, 1, 0, -1.0).is_err() && va.read(&interp, 1, 0).is_err(),
                    "iteration {k}: refused while a worker guard is alive"
                );
                assert!(
                    b.read().is_err(),
                    "iteration {k}: no guard without a section"
                );
                team.wait("refused");
                let want = f64::from(k);
                let deadline = SpinDeadline::start();
                let state = || {
                    format!(
                        "iteration {k}: guards {}, poisoned {}, foreign {}, releasing {}",
                        rt.native_guards(),
                        rt.is_poisoned(),
                        rt.is_foreign_phase(),
                        releasing.load(Ordering::Acquire)
                    )
                };
                if rng.coin() {
                    // Mode A: foreign accesses race the release.
                    loop {
                        rng.pause();
                        if let Ok(x) = va.read(&interp, 1, 0) {
                            assert!(
                                releasing.load(Ordering::Acquire),
                                "{}: admitted before the last guard release",
                                state()
                            );
                            assert_eq!(x, want, "{}: the worker's write is visible", state());
                            va.write(&interp, 1, 0, -want).unwrap();
                            break;
                        }
                        refusals += 1;
                        team.check("poisoned window");
                        deadline.check("foreign access admitted", state);
                    }
                } else {
                    // Mode B: a new section adopts the leftover guards; a
                    // callback is refused until they are all released.
                    mode_b += 1;
                    let _n = interp.native().unwrap();
                    loop {
                        rng.pause();
                        let admitted = foreign_callback(&rt, |py| {
                            assert!(
                                releasing.load(Ordering::Acquire),
                                "{}: callback admitted before the last guard release",
                                state()
                            );
                            let x = va.read(py, 1, 0).unwrap();
                            va.write(py, 1, 0, -want).unwrap();
                            x
                        });
                        if let Ok(x) = admitted {
                            assert_eq!(x, want, "{}: the worker's write is visible", state());
                            break;
                        }
                        refusals += 1;
                        team.check("adopting section");
                        deadline.check("callback admitted", state);
                    }
                }
                assert!(
                    rt.is_foreign_phase() && !rt.is_poisoned() && rt.native_guards() == 0,
                    "{}",
                    state()
                );
                team.wait("iteration end");
            }
        });
        if !cfg!(miri) {
            assert!(mode_b > 0 && mode_b < iters, "both modes exercised");
        }
        eprintln!(
            "worker_guard_release_races_foreign_access_stress: {iters} iterations, \
             {mode_b} adopting sections, {refusals} refusals in the race window"
        );
    });
}

// ---- concurrency and overlap ----------------------------------------------

/// Several threads contend for the runtime; each success performs either a
/// foreign read-modify-write through a retained view or a native
/// increment under a section. All increments are serialised by the
/// protocol: the total is exact and Miri sees no race.
#[test]
fn retained_view_concurrent_access_is_serialised() {
    watchdog(|| {
        let rt = ForeignRuntime::new();
        let owner = Arc::new(SharedOwner::new(SharedArray::new(Array::<f64>::zeros(
            1, 1,
        ))));
        let view =
            Arc::new(ForeignView::new(&mut owner.try_borrow_mut().unwrap(), &rt, true).unwrap());
        let iters = if cfg!(miri) { 4 } else { 200 };
        let handles: Vec<_> = (0..3)
            .map(|t| {
                let (rt, owner, view) = (rt.clone(), Arc::clone(&owner), Arc::clone(&view));
                std::thread::spawn(move || {
                    let mut ok = 0u32;
                    for i in 0..iters {
                        let Ok(mut interp) = rt.enter() else {
                            std::thread::yield_now();
                            continue;
                        };
                        if (i + t) % 2 == 0 {
                            let x = view.read(&interp, 0, 0).unwrap();
                            view.write(&interp, 0, 0, x + 1.0).unwrap();
                        } else {
                            let _n = interp.native().unwrap();
                            let mut a = owner.try_borrow_mut().unwrap();
                            a.write().unwrap().as_mut_slice()[0] += 1.0;
                        }
                        ok += 1;
                    }
                    ok
                })
            })
            .collect();
        let total: u32 = handles.into_iter().map(|h| h.join().unwrap()).sum();
        let interp = rt.enter().unwrap();
        assert_eq!(view.read(&interp, 0, 0).unwrap(), f64::from(total));
    });
}

#[test]
fn native_and_foreign_overlap_is_rejected_both_ways() {
    let rt = ForeignRuntime::new();
    let mut u = shared(&[1.0, 2.0], 1);
    let mut k = shared(&[0.0], 1);
    let uv = ForeignView::new(&mut u, &rt, true).unwrap();
    let mut interp = rt.enter().unwrap();
    let _n = interp.native().unwrap();
    // Native read guard alive → no callback (Python could write `uv`).
    let g = u.read().unwrap();
    let ran = std::cell::Cell::new(false);
    assert!(foreign_callback(&rt, |_| ran.set(true)).is_err());
    assert!(!ran.get(), "the callback did not run");
    drop(g);
    // Native write guard on another exported array: same.
    let _kv = ForeignView::new(&mut k, &rt, false).unwrap();
    let w = k.write().unwrap();
    assert!(foreign_callback(&rt, |_| ()).is_err());
    drop(w);
    // Inside a callback, native guards cannot be taken (foreign phase).
    foreign_callback(&rt, |py| {
        assert!(u.read().is_err() && k.write().is_err());
        uv.write(py, 0, 0, 10.0).unwrap();
    })
    .unwrap();
    assert_eq!(u.read().unwrap().as_slice(), &[10.0, 2.0]);
}

// ---- slicing and shrink/re-grow -------------------------------------------

#[test]
fn sliced_views_share_the_allocation() {
    let rt = ForeignRuntime::new();
    let mut a = SharedArray::new(Array::<f64>::zeros(4, 2));
    let full = ForeignView::new(&mut a, &rt, true).unwrap();
    let mid = full.rows(1..3).unwrap(); // u[1:3, :]
    let last = mid.rows(1..2).unwrap(); // u[1:3][1:2] = u[2:3]
    assert_eq!((mid.shape(), last.shape()), ((2, 2), (1, 2)));
    assert!(full.rows(3..5).is_err() && mid.rows(1..3).is_err());
    let mut interp = rt.enter().unwrap();
    mid.write(&interp, 0, 1, 1.5).unwrap(); // u[1, 1]
    last.write(&interp, 0, 0, 2.5).unwrap(); // u[2, 0]
    assert!(mid.read(&interp, 2, 0).is_err(), "row outside the slice");
    assert!(mid.read(&interp, 0, 2).is_err(), "component out of range");
    let mut out = [0.0; 4];
    mid.copy_to(&interp, &mut out).unwrap();
    assert_eq!(out, [0.0, 1.5, 2.5, 0.0]);
    assert!(mid.read_only().write(&interp, 0, 0, 1.0).is_err());
    mid.fill(&interp, 4.0).unwrap();
    {
        let _n = interp.native().unwrap();
        assert_eq!(
            a.read().unwrap().as_slice(),
            &[0.0, 0.0, 4.0, 4.0, 4.0, 4.0, 0.0, 0.0]
        );
        a.write().unwrap().try_push_tuple(&[9.0, 9.0]).unwrap(); // beyond capacity
    }
    assert!(mid.is_stale() && last.is_stale() && !mid.is_attached_to(&a));
    assert_eq!(
        last.read(&interp, 0, 1).unwrap(),
        4.0,
        "stale slice keeps old values"
    );
}

#[test]
fn shrink_and_regrow_detach_monotonically() {
    let rt = ForeignRuntime::new();
    let mut base = Array::<f64>::new(1);
    base.try_reserve(8).unwrap();
    for x in [1.0, 2.0, 3.0] {
        base.push_tuple(&[x]);
    }
    let mut a = SharedArray::new(base);
    let v = ForeignView::new(&mut a, &rt, true).unwrap();
    let mut interp = rt.enter().unwrap();
    {
        let _n = interp.native().unwrap();
        a.write().unwrap().push_tuple(&[4.0]); // within capacity
    }
    assert!(
        v.is_attached_to(&a),
        "growth within capacity keeps the view"
    );
    {
        let _n = interp.native().unwrap();
        a.write().unwrap().truncate(1); // shrink: detach first
    }
    assert!(v.is_stale() && !v.is_attached_to(&a));
    assert_eq!(
        v.read(&interp, 2, 0).unwrap(),
        3.0,
        "truncated row still readable"
    );
    v.write(&interp, 0, 0, -1.0).unwrap();
    {
        let _n = interp.native().unwrap();
        let mut w = a.write().unwrap();
        w.push_tuple(&[7.0]); // re-grow within the (fresh) capacity
        w.push_tuple(&[8.0]);
        assert_eq!(w.as_slice(), &[1.0, 7.0, 8.0], "stale write not visible");
    }
    assert!(!v.is_attached_to(&a), "staleness is monotone");
    let v2 = ForeignView::new(&mut a, &rt, true).unwrap();
    {
        let _n = interp.native().unwrap();
        a.write().unwrap().swap_remove_tuple(0).unwrap(); // erase
    }
    assert!(v2.is_stale());
    assert_eq!(
        v2.read(&interp, 0, 0).unwrap(),
        1.0,
        "pre-erase values kept"
    );
    let v3 = ForeignView::new(&mut a, &rt, true).unwrap();
    {
        let _n = interp.native().unwrap();
        let src = Array::from_vec(vec![5.0, 6.0], 1).unwrap();
        a.write().unwrap().copy_from(&src).unwrap(); // equal length
    }
    assert!(v3.is_attached_to(&a));
    assert_eq!(v3.read(&interp, 1, 0).unwrap(), 6.0);
}

/// A safe client: a zero-component array `(2, 0)`
/// (point / zero-dimensional FE data) exports shape `(2, 0)` — not
/// `(0, 0)` — so valid row slices work; copies move zero values; growth
/// keeps the view attached; **every** tuple shrink (truncate, resize,
/// shorter copy, clear, erase) detaches it, although no value is removed.
#[test]
fn zero_component_views_keep_their_shape_and_detach_on_shrink() {
    let rt = ForeignRuntime::new();
    let mut a = SharedArray::new(Array::<f64>::zeros(2, 0));
    let v = ForeignView::new(&mut a, &rt, true).unwrap();
    assert_eq!(a.len(), 2);
    assert_eq!(v.shape(), (2, 0), "tuple count survives the export");
    let row = v.rows(0..1).unwrap();
    assert_eq!(row.shape(), (1, 0), "valid array row accepted");
    assert!(v.rows(0..3).is_err(), "beyond the array: rejected");
    let interp = rt.enter().unwrap();
    v.copy_to(&interp, &mut []).unwrap(); // `np.array(view)`: no value
    assert!(v.read(&interp, 0, 0).is_err(), "no component to read");
    drop(interp);
    let mut interp = rt.enter().unwrap();
    {
        let _n = interp.native().unwrap();
        a.write().unwrap().push_tuple(&[]); // growth: (3, 0)
    }
    assert!(v.is_attached_to(&a) && row.is_attached_to(&a));
    {
        let _n = interp.native().unwrap();
        a.write().unwrap().truncate(1); // shrink, no value removed
    }
    assert!(v.is_stale() && !v.is_attached_to(&a), "shrink detaches");
    assert!(!row.is_attached_to(&a), "slices share the lease");
    assert_eq!(v.shape(), (2, 0), "a stale view keeps its shape");
    assert!(v.require_attached(&a).is_err());
    // The other tuple-shrinking operations detach as well.
    type Shrink = fn(&mut akantu::core::array_shared::ArrayMut<'_, f64>);
    let shrinks: [(&str, Shrink); 4] = [
        ("try_resize", |w| w.try_resize(1, 0.0).unwrap()),
        ("copy_from", |w| {
            w.copy_from(&Array::<f64>::zeros(1, 0)).unwrap();
        }),
        ("clear", |w| w.clear()),
        ("swap_remove_tuple", |w| w.swap_remove_tuple(0).unwrap()),
    ];
    for (what, shrink) in shrinks {
        let mut a = SharedArray::new(Array::<f64>::zeros(3, 0));
        let v = ForeignView::new(&mut a, &rt, true).unwrap();
        {
            let _n = interp.native().unwrap();
            shrink(&mut a.write().unwrap());
        }
        assert!(v.is_stale() && !v.is_attached_to(&a), "{what}");
        let w = ForeignView::new(&mut a, &rt, true).unwrap();
        assert_eq!(w.shape(), (a.len(), 0), "{what}: re-export");
    }
    // An equal-length copy (history restore) stays attached.
    let mut a = SharedArray::new(Array::<f64>::zeros(2, 0));
    let v = ForeignView::new(&mut a, &rt, true).unwrap();
    {
        let _n = interp.native().unwrap();
        a.write()
            .unwrap()
            .copy_from(&Array::<f64>::zeros(2, 0))
            .unwrap();
    }
    assert!(v.is_attached_to(&a));
}

// ---- callbacks -------------------------------------------------------------

/// A Python material keeping `self.factor = self.getInternalReal("factor")`
/// and a retained `u = model.getDisplacement()` across calls.
struct PyLawState {
    factor: SharedArray<f64>,
}

struct PyLaw {
    factor: Option<ForeignView<f64>>,
    u: ForeignView<f64>,
}

impl PyLaw {
    /// `initMaterial` (Python, called from a binding = native section).
    fn init(rt: &ForeignRuntime, state: &mut PyLawState, u: ForeignView<f64>) -> Result<Self> {
        let factor = ForeignView::new(&mut state.factor, rt, true)?;
        Ok(Self {
            factor: Some(factor),
            u,
        })
    }

    /// `computeStress` body, running in the foreign phase.
    fn compute_stress(&self, py: &mut akantu::core::array_shared::Interpreter) -> Result<f64> {
        let f = self
            .factor
            .as_ref()
            .ok_or_else(|| Error::invalid("no view"))?;
        let u1 = self.u.read(py, 1, 0)?; // reading a retained model view
        f.write(py, 1, 0, 0.5 * u1)?;
        // A binding call from inside the callback (nested native section).
        let n = py.native()?;
        drop(n);
        Ok(u1)
    }
}

#[test]
fn callbacks_access_retained_views_only_without_native_borrows() {
    let rt = ForeignRuntime::new();
    let mut displacement = shared(&[0.0, 2.0, 0.0], 1);
    let mut state = PyLawState {
        factor: SharedArray::new(Array::zeros(2, 1)),
    };
    let u = ForeignView::new(&mut displacement, &rt, true).unwrap();
    let mut interp = rt.enter().unwrap();
    let law = {
        let _n = interp.native().unwrap();
        PyLaw::init(&rt, &mut state, u).unwrap()
    };
    // model.solveStep(): native section; strains from u, then the law.
    let _n = interp.native().unwrap();
    let grad = displacement.read().unwrap();
    let s = grad.as_slice()[1] - grad.as_slice()[0];
    assert!(
        foreign_callback(&rt, |py| law.compute_stress(py)).is_err(),
        "a native borrow of `displacement` is alive across the callback"
    );
    drop(grad);
    let u1 = foreign_callback(&rt, |py| law.compute_stress(py))
        .unwrap()
        .unwrap();
    assert_eq!((s, u1), (2.0, 2.0));
    assert_eq!(state.factor.read().unwrap().as_slice(), &[0.0, 1.0]);
}

// ---- phase-scoped model handles -----------------------------------

struct Toy(Mesh, Array<Real>);
impl ModelServices for Toy {
    fn spatial_dimension(&self) -> usize {
        1
    }
    fn mesh(&self) -> &Mesh {
        &self.0
    }
    fn time_step(&self) -> Real {
        0.0
    }
    fn nb_integration_points(&self, _: ElementType, _: GhostType) -> usize {
        1
    }
    fn integration_point_coordinates(
        &self,
        ty: ElementType,
        g: GhostType,
        filter: &[Idx],
        out: &mut Array<Real>,
    ) -> Result<()> {
        *out = Array::new(1);
        for &e in filter {
            out.push_tuple(&self.0.barycenter(Element::new(ty, e, g))?);
        }
        Ok(())
    }
    fn nodal_field(&self, name: &str) -> Result<akantu::core::array_shared::ArrayRef<'_, Real>> {
        (name == "displacement")
            .then_some(&self.1)
            .map(Into::into)
            .ok_or_else(|| Error::not_found("field", name))
    }
}

/// A Python law keeping `self.model = self.getModel()` across calls.
#[derive(Debug, Default)]
struct ModelKeepingLaw {
    model: Option<(Scoped<dyn ModelServices>, ContextKey)>,
    late_model_use: Option<String>,
}

impl ForeignMaterialMethods for ModelKeepingLaw {
    fn init_material(&mut self, law: &mut ForeignLaw<'_, '_>) -> Result<()> {
        law.base_init_material()?;
        let key = ContextKey::fresh();
        let dim = with_model_services(law.ctx.services, key, |m| {
            self.model = Some((m.clone(), key));
            m.with(key, ModelServices::spatial_dimension)
        })?;
        assert_eq!(dim, 1);
        Ok(())
    }
    fn compute_stress(
        &mut self,
        _: &mut ForeignLaw<'_, '_>,
        _: ElementType,
        _: GhostType,
    ) -> Result<()> {
        match self
            .model
            .as_ref()
            .map(|(m, key)| m.with(*key, |s| s.time_step()))
        {
            Some(Err(e)) => self.late_model_use = Some(e.to_string()),
            _ => return Err(Error::invalid("a stale model handle must not be usable")),
        }
        Ok(())
    }
    fn push_wave_speed(&self, _: &ForeignLawState, _: Element) -> Result<Real> {
        Ok(1.0)
    }
}

fn clone_mesh(m: &Mesh) -> Mesh {
    let mut c = Mesh::new(m.spatial_dimension(), "copy").unwrap();
    let mut a = MeshAccessor::new(&mut c);
    for t in m.nodes().tuples() {
        a.push_node(t).unwrap();
    }
    for tup in m.connectivity(SEG, GhostType::NotGhost).unwrap().tuples() {
        a.connectivity_mut(SEG, GhostType::NotGhost).push_tuple(tup);
    }
    c
}

#[test]
fn callbacks_cannot_keep_the_model() {
    let mesh = bar();
    let toy = Toy(clone_mesh(&mesh.read().unwrap()), Array::zeros(3, 1));
    let spec = LawSpec {
        spatial_dimension: 1,
        option: String::new(),
        id: "py".into(),
    };
    let mut m =
        ForeignMaterial::new(&spec, "py_law", Box::new(ModelKeepingLaw::default())).unwrap();
    m.state
        .params
        .set_from_input("rho", akantu::parser::param::ParamValue::Real(1.0))
        .unwrap();
    m.element_filter_mut()
        .as_map_mut()
        .get_or_insert_with(SEG, GhostType::NotGhost, || Array::new(1))
        .append(&Array::from_vec(vec![0, 1], 1).unwrap())
        .unwrap();
    m.init_material(&LawContext {
        phase: LawPhase::Init,
        services: &toy,
    })
    .unwrap();
    m.compute_stress(
        &LawContext {
            phase: LawPhase::Stress,
            services: &toy,
        },
        SEG,
        GhostType::NotGhost,
    )
    .unwrap();
}

#[test]
fn escaped_model_handle_reports_the_phase_error() {
    let mesh = bar();
    let toy = Toy(clone_mesh(&mesh.read().unwrap()), Array::zeros(3, 1));
    let key = ContextKey::fresh();
    let kept = with_model_services(&toy, key, |h| {
        assert!(h.is_valid());
        h
    });
    assert!(!kept.is_valid());
    assert!(
        kept.with(key, |m| m.spatial_dimension())
            .unwrap_err()
            .to_string()
            .contains("outside the callback")
    );
}

#[test]
fn concurrent_or_reentrant_owner_access_fails_promptly() {
    watchdog(|| {
        let owner = Arc::new(SharedOwner::new(SharedArray::new(Array::<f64>::zeros(
            4, 1,
        ))));
        let team = Arc::new(Team::new(2));
        let native = {
            let (o, team) = (Arc::clone(&owner), Arc::clone(&team));
            std::thread::spawn(move || {
                let _m = team.member();
                let mut a = o.try_borrow_mut().unwrap(); // native computation
                team.wait("started");
                team.wait("release");
                a.write().unwrap().as_mut_slice()[0] = 1.0; // not exported: no section needed
            })
        };
        let _m = team.member();
        team.wait("started");
        // Another thread (or a re-entrant Python call) tries to export a view.
        assert!(
            owner.try_borrow_mut().is_err(),
            "already borrowed, no aliasing, no blocking"
        );
        team.wait("release");
        native.join().unwrap();
        let rt = ForeignRuntime::new();
        let v = ForeignView::new(&mut owner.try_borrow_mut().unwrap(), &rt, false).unwrap();
        let interp = rt.enter().unwrap();
        assert_eq!(v.read(&interp, 0, 0).unwrap(), 1.0);
    });
}

/// Handles under concurrency: a use nested inside an in-flight use on
/// another thread, while the lending call is returning, is **refused**
/// promptly; the lending call returns once the in-flight use has ended.
/// (With the former `RwLock<bool>` the nested read — and even `is_valid` —
/// queued behind the scope end's pending write lock, which itself waited
/// for the outer read: a deadlock.)
#[test]
fn nested_scoped_use_during_scope_end_is_refused_not_blocked() {
    watchdog(|| {
        let value = String::from("model");
        let team = Team::new(2);
        let use_ended = AtomicBool::new(false);
        let outcome = std::thread::scope(|s| {
            let (team, use_ended) = (&team, &use_ended);
            let key = ContextKey::fresh();
            let user = with_scoped(&value, key, |h| {
                let user = s.spawn(move || {
                    let _m = team.member();
                    let r = h.with(key, |v| {
                        team.wait("in flight");
                        // The lending call is returning: wait until it has
                        // closed the scope (it then waits for this use).
                        let deadline = SpinDeadline::start();
                        while h.is_valid() {
                            team.check("closing");
                            deadline.check("scope closed", || "still open".to_owned());
                            std::thread::yield_now();
                        }
                        let nested = h.with(key, |w| w.len());
                        use_ended.store(true, Ordering::Relaxed);
                        (v.clone(), nested.is_err())
                    });
                    (r, h.with(key, |_| ()).is_err())
                });
                let _m = team.member();
                team.wait("in flight");
                user
            });
            assert!(
                use_ended.load(Ordering::Relaxed),
                "the scope end waited for the in-flight use"
            );
            user.join().unwrap()
        });
        let (r, later_refused) = outcome;
        assert_eq!(r.unwrap(), ("model".to_owned(), true), "nested use refused");
        assert!(later_refused, "uses after the scope end are refused");
    });
}

/// Stress for phase-scoped handles: three threads keep using (sometimes
/// nested) clones of a handle while the lending call returns at a random
/// moment, after which the lent value is freed (a use admitted after the
/// scope end would be a use-after-free under Miri). Every use either sees
/// the value or is refused; nothing blocks.
#[test]
fn scoped_uses_racing_the_scope_end_stress() {
    watchdog(|| {
        let iters: u64 = if cfg!(miri) { 8 } else { 300 };
        for k in 0..iters {
            let value = Box::new(k);
            let team = Team::new(4);
            let team = &team;
            std::thread::scope(move |s| {
                let mut rng = Jitter::new(k);
                let key = ContextKey::fresh();
                with_scoped(&*value, key, |h| {
                    for t in 0..3 {
                        let h = h.clone();
                        let mut rng = Jitter::new(k * 8 + t + 1);
                        s.spawn(move || {
                            let _m = team.member();
                            team.wait("start");
                            let deadline = SpinDeadline::start();
                            loop {
                                rng.pause();
                                let r = h.with(key, |v| {
                                    rng.pause();
                                    let nested = rng.coin().then(|| h.with(key, |w| *w));
                                    (*v, nested)
                                });
                                match r {
                                    Ok((v, nested)) => {
                                        assert_eq!(v, k);
                                        if let Some(Ok(w)) = nested {
                                            assert_eq!(w, k);
                                        }
                                    }
                                    Err(_) => break,
                                }
                                team.check("using");
                                deadline.check("scope closed", || format!("iteration {k}"));
                            }
                            assert!(
                                h.with(key, |_| ()).is_err() && !h.is_valid(),
                                "iteration {k}: closing is permanent"
                            );
                        });
                    }
                    let _m = team.member();
                    team.wait("start");
                    rng.pause();
                });
                drop(value); // users may still be running: they must be refused
            });
        }
    });
}

// ---- exclusive callback handles (solver callbacks, Python laws) ----

/// `ScopedMut`: a `&mut` lent for one callback is usable (and mutates the
/// value) only while the lending call runs, one use at a time, and refused
/// afterwards; a handle stored beyond the call is inert.
#[test]
fn scoped_mut_is_exclusive_and_dies_with_the_lending_call() {
    let mut value = vec![1.0_f64, 2.0];
    let mut stored: Option<ScopedMut<Vec<f64>>> = None;
    let key = ContextKey::fresh();
    let nested = with_scoped_mut(&mut value, key, |h| {
        stored = Some(h.clone());
        h.with_mut(key, |v| v.push(3.0)).unwrap();
        // a nested use (re-entrant callback) is refused, not aliased
        let nested = h.with_mut(key, |v| {
            h.with_mut(key, |w| w.len()).is_err() && v.len() == 3
        });
        assert!(h.is_valid());
        nested.unwrap()
    });
    assert!(nested, "nested use refused");
    assert_eq!(value, vec![1.0, 2.0, 3.0]);
    let h = stored.unwrap();
    assert!(!h.is_valid());
    let err = h.with_mut(key, |v| v.len()).unwrap_err();
    assert!(err.to_string().contains("outside the callback"), "{err}");
}

/// `ScopedMut` uses on another thread are refused while the lending call
/// runs (the lent value is never touched off the lending thread), so the
/// handle can be `Send + Sync` without `T: Send`.
#[test]
fn scoped_mut_refuses_other_threads() {
    watchdog(|| {
        let mut value = std::rc::Rc::new(5_u32); // !Send on purpose
        let key = ContextKey::fresh();
        let refused = with_scoped_mut(&mut value, key, |h| {
            let h2 = h.clone();
            let r = std::thread::scope(|s| {
                s.spawn(move || h2.with_mut(key, |v| **v).unwrap_err().to_string())
                    .join()
                    .unwrap()
            });
            assert_eq!(h.with_mut(key, |v| **v).unwrap(), 5);
            r
        });
        assert!(refused.contains("another thread"), "{refused}");
    });
}

#[derive(Debug, Default)]
struct FakeSolve {
    calls: Vec<String>,
}

impl SolverAccess for FakeSolve {
    fn matrix_type(&self, _matrix_id: &str) -> Result<akantu::core::enums::MatrixType> {
        Ok(akantu::core::enums::MatrixType::Symmetric)
    }
    fn assemble_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.calls.push(format!("K:{matrix_id}"));
        Ok(())
    }
    fn assemble_lumped_matrix(&mut self, matrix_id: &str) -> Result<()> {
        self.calls.push(format!("M:{matrix_id}"));
        Ok(())
    }
    fn assemble_residual(&mut self) -> Result<()> {
        self.calls.push("r".into());
        Ok(())
    }
    fn predictor(&mut self) -> Result<()> {
        Ok(())
    }
    fn corrector(&mut self) -> Result<()> {
        Ok(())
    }
    fn before_solve_step(&mut self) -> Result<()> {
        Ok(())
    }
    fn after_solve_step(&mut self, _converged: bool) -> Result<()> {
        Ok(())
    }
    fn dofs(&mut self) -> Result<&mut akantu::solver::DofManager> {
        Err(Error::invalid("no DOF manager in this fake"))
    }
}

/// `with_solver_access`: a trait object borrowing the running solve (here a
/// local) is reachable through the stored handle during the call only.
#[test]
fn solver_access_handle_reaches_the_running_solve_only_during_the_call() {
    let mut solve = FakeSolve::default();
    let mut stored = None;
    let key = ContextKey::fresh();
    with_solver_access(&mut solve, key, |h| {
        stored = Some(h.clone());
        h.with_mut(key, |a| a.assemble_matrix("K"))
            .unwrap()
            .unwrap();
        h.with_mut(key, |a| a.assemble_residual()).unwrap().unwrap();
        assert!(h.with_mut(key, |a| a.dofs().is_err()).unwrap());
    });
    assert_eq!(solve.calls, ["K:K", "r"]);
    let h = stored.unwrap();
    assert!(h.with_mut(key, |a| a.assemble_residual()).is_err());
    assert_eq!(solve.calls.len(), 2);
}

// ---- execution contexts -------------------------------

/// Context keys are never reused: two lends never share one.
#[test]
fn context_keys_are_unique() {
    let keys: Vec<ContextKey> = (0..64).map(|_| ContextKey::fresh()).collect();
    for (i, a) in keys.iter().enumerate() {
        assert!(keys[i + 1..].iter().all(|b| a != b));
    }
}

/// A use presenting another execution context's key — the shape of the
/// main greenlet using a handle while the callback's greenlet is suspended
/// inside the lending call, with the lending call's stack copied away — is
/// refused like another thread: it never touches the lent value, also when
/// it is nested inside a use of the lending context, and the lend goes on:
/// the lending context's own uses still work afterwards.
#[test]
fn scoped_mut_refuses_another_execution_context() {
    let mut value = vec![1_u32];
    let (lend, other) = (ContextKey::fresh(), ContextKey::fresh());
    with_scoped_mut(&mut value, lend, |h| {
        let err = h.with_mut(other, |v| v.push(99)).unwrap_err().to_string();
        assert!(err.contains("another execution context"), "{err}");
        assert!(h.is_valid());
        h.with_mut(lend, |v| {
            v.push(2);
            let nested = h.with_mut(other, |w| w.push(98)).unwrap_err().to_string();
            assert!(nested.contains("another execution context"), "{nested}");
        })
        .unwrap();
        h.with_mut(lend, |v| v.push(3)).unwrap();
    });
    assert_eq!(value, [1, 2, 3]);
}

/// The same for the shared lends (`Scoped`: law-state `&` lends and the
/// model services lent to Python law callbacks).
#[test]
fn scoped_refuses_another_execution_context() {
    let value = String::from("lent");
    let (lend, other) = (ContextKey::fresh(), ContextKey::fresh());
    let n = with_scoped(&value, lend, |h| {
        let err = h.with(other, |v| v.len()).unwrap_err().to_string();
        assert!(err.contains("another execution context"), "{err}");
        h.with(lend, |v| {
            assert!(h.with(other, |_| ()).is_err(), "nested, other context");
            v.len()
        })
        .unwrap()
    });
    assert_eq!(n, 4);
    let mesh = bar();
    let toy = Toy(clone_mesh(&mesh.read().unwrap()), Array::zeros(3, 1));
    let dim = with_model_services(&toy, lend, |h| {
        assert!(h.with(other, |m| m.spatial_dimension()).is_err());
        h.with(lend, |m| m.spatial_dimension()).unwrap()
    });
    assert_eq!(dim, 1);
}

static LENT_ACCESS: std::sync::Mutex<Option<ScopedMut<dyn SolverAccess>>> =
    std::sync::Mutex::new(None);

/// Execution contexts at the protocol level: solve access lent from a stack local
/// (`DelegateAccess` / `CallbackAccess` in the bindings) and reachable
/// through shared state (`ModelHandle.access`) while the lending call is
/// alive. Another context that finds it there is refused and the local is
/// not touched; the lending context reaches it; after the call every
/// context is refused.
#[test]
fn solver_access_lent_from_the_stack_refuses_another_context() {
    let (lend, other) = (ContextKey::fresh(), ContextKey::fresh());
    {
        let mut solve = FakeSolve::default();
        with_solver_access(&mut solve, lend, |h| {
            *LENT_ACCESS.lock().unwrap() = Some(h);
            let found = LENT_ACCESS.lock().unwrap().clone().unwrap();
            let err = found
                .with_mut(other, |a| a.assemble_residual())
                .unwrap_err()
                .to_string();
            assert!(err.contains("another execution context"), "{err}");
            found
                .with_mut(lend, |a| a.assemble_matrix("K"))
                .unwrap()
                .unwrap();
        });
        assert_eq!(solve.calls, ["K:K"]);
    }
    let kept = LENT_ACCESS.lock().unwrap().take().unwrap();
    for key in [lend, other] {
        assert!(kept.with_mut(key, |a| a.assemble_residual()).is_err());
    }
}
