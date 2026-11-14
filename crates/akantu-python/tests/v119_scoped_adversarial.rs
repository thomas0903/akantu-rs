// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: adversarial tests, written independently of the port, of the
// `unsafe` sites of crates/akantu-python/src/scoped.rs (ScopedMut Send/Sync,
// the `&mut` dereference in `with_mut`, the lifetime erasure of
// `with_solver_access`), adapted to the execution-context keys (every
// lend names its
// context, every use presents one); run with
// `cargo test -p akantu-python --test v119_scoped_adversarial` and under Miri
// (Stacked and Tree Borrows). No upstream counterpart (akantu-rs design).

use std::cell::Cell;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::rc::Rc;
use std::sync::{Arc, Barrier, Mutex};

use akantu::core::Result;
use akantu::core::enums::MatrixType;
use akantu::solver::DofManager;
use py11_akantu::scoped::{
    ContextKey, ScopedMut, SolverAccess, with_scoped_mut, with_solver_access,
};

/// A panic inside a use ends the use (`ExclusiveUse::drop`): the handle is
/// usable again inside the same lending call and the value is consistent.
#[test]
fn unwinding_out_of_a_use_releases_it() {
    let mut v = vec![0_u32];
    let k = ContextKey::fresh();
    with_scoped_mut(&mut v, k, |h| {
        let r = catch_unwind(AssertUnwindSafe(|| {
            h.with_mut(k, |v| {
                v.push(1);
                panic!("boom inside the use");
            })
        }));
        assert!(r.is_err());
        h.with_mut(k, |v| v.push(2)).unwrap();
    });
    assert_eq!(v, [0, 1, 2]);
}

/// A panic unwinding out of the lending closure closes the scope: a clone
/// kept elsewhere is refused afterwards (no use after the borrow ended).
#[test]
fn unwinding_out_of_the_lending_call_closes_the_scope() {
    let kept: Mutex<Option<ScopedMut<Vec<u32>>>> = Mutex::new(None);
    let mut v = vec![7_u32];
    let k = ContextKey::fresh();
    let r = catch_unwind(AssertUnwindSafe(|| {
        with_scoped_mut(&mut v, k, |h| {
            *kept.lock().unwrap() = Some(h.clone());
            h.with_mut(k, |v| v.push(8)).unwrap();
            panic!("boom in the lending call");
        })
    }));
    assert!(r.is_err());
    v.push(9); // the borrow ended: the owner uses the value again
    let h = kept.lock().unwrap().take().unwrap();
    assert!(!h.is_valid());
    assert!(h.with_mut(k, |v| v.len()).is_err());
    assert_eq!(v, [7, 8, 9]);
}

/// Another thread holding a clone: refused while the lender is inside a use,
/// refused while the scope is open and idle, refused after the scope ended;
/// dropping its clones there never touches the value.
#[test]
fn another_thread_is_refused_in_every_phase() {
    let slot: Arc<Mutex<Option<ScopedMut<Vec<u64>>>>> = Arc::new(Mutex::new(None));
    let b = Arc::new(Barrier::new(2));
    let (s2, b2) = (Arc::clone(&slot), Arc::clone(&b));
    let k = ContextKey::fresh();
    let worker = std::thread::spawn(move || {
        let mut refused = Vec::new();
        for _ in 0..3 {
            b2.wait(); // phase start
            let h = s2.lock().unwrap().clone().unwrap();
            // even with the lend's own key: another thread is refused
            refused.push(h.with_mut(k, |v| v.push(99)).is_err());
            drop(h);
            b2.wait(); // phase end
        }
        refused
    });
    let mut v = vec![1_u64];
    with_scoped_mut(&mut v, k, |h| {
        *slot.lock().unwrap() = Some(h.clone());
        // phase 1: lender inside a use
        h.with_mut(k, |v| {
            b.wait();
            b.wait();
            v.push(2);
        })
        .unwrap();
        // phase 2: scope open, no use in flight
        b.wait();
        b.wait();
    });
    // phase 3: after the scope
    b.wait();
    b.wait();
    assert_eq!(worker.join().unwrap(), [true, true, true]);
    assert_eq!(v, [1, 2]);
}

/// `ScopedMut<T>` is `Send + Sync` without `T: Send`: moving a handle of a
/// non-`Send` value (`Rc`) to another thread must never let that thread
/// touch the value (refcount races would be reported by Miri).
#[test]
fn non_send_payload_never_leaves_the_lending_thread() {
    let mut rc = Rc::new(Cell::new(1_i32));
    let k = ContextKey::fresh();
    with_scoped_mut(&mut rc, k, |h| {
        let h2 = h.clone();
        let refused = std::thread::spawn(move || {
            let r = h2.with_mut(k, |rc| {
                let c = Rc::clone(rc); // would race on the refcount
                c.set(c.get() + 100);
            });
            let h3 = h2.clone();
            drop(h2);
            drop(h3);
            r.is_err()
        })
        .join()
        .unwrap();
        assert!(refused);
        h.with_mut(k, |rc| {
            let c = Rc::clone(rc);
            c.set(c.get() + 1);
        })
        .unwrap();
    });
    assert_eq!(rc.get(), 2);
    assert_eq!(Rc::strong_count(&rc), 1);
}

/// Lending a field of the lent value from inside a use (a nested phase, as
/// a Python law callback inside a solver callback): the inner handle works,
/// the outer one is refused while its use is in flight, the inner one is
/// refused once returned out of its scope; Miri checks the reborrows.
#[test]
fn nested_lend_of_a_sub_borrow() {
    let mut pair = (vec![1_u8], vec![10_u8]);
    // the nested lend belongs to the nested callback's own context
    let (k, k2) = (ContextKey::fresh(), ContextKey::fresh());
    let escaped = with_scoped_mut(&mut pair, k, |h| {
        let inner = h
            .with_mut(k, |p| {
                with_scoped_mut(&mut p.0, k2, |h2| {
                    h2.with_mut(k2, |a| a.push(2)).unwrap();
                    assert!(h.with_mut(k, |_| ()).is_err(), "outer in use");
                    assert!(h2.with_mut(k, |_| ()).is_err(), "inner: outer key");
                    p.1.push(11); // the rest of the outer value, disjoint
                    h2.clone()
                })
            })
            .unwrap();
        assert!(
            inner.with_mut(k2, |a| a.push(3)).is_err(),
            "inner scope ended"
        );
        h.with_mut(k, |p| p.0.push(4)).unwrap();
        inner
    });
    assert!(!escaped.is_valid());
    assert_eq!(pair, (vec![1, 2, 4], vec![10, 11]));
}

/// A `SolverAccess` borrowing a short-lived local (the erased lifetime): a
/// handle escaping into a global is refused after the call, while the
/// local is mutated and dropped by its owner.
struct ShortAccess<'a> {
    log: &'a mut Vec<String>,
}

impl SolverAccess for ShortAccess<'_> {
    fn matrix_type(&self, _: &str) -> Result<MatrixType> {
        Ok(MatrixType::Symmetric)
    }
    fn assemble_matrix(&mut self, id: &str) -> Result<()> {
        self.log.push(format!("K:{id}"));
        Ok(())
    }
    fn assemble_lumped_matrix(&mut self, id: &str) -> Result<()> {
        self.log.push(format!("M:{id}"));
        Ok(())
    }
    fn assemble_residual(&mut self) -> Result<()> {
        self.log.push("r".into());
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
    fn after_solve_step(&mut self, _: bool) -> Result<()> {
        Ok(())
    }
    fn dofs(&mut self) -> Result<&mut DofManager> {
        Err(akantu::core::Error::invalid("no DOF manager in this test"))
    }
}

static ESCAPED: Mutex<Option<ScopedMut<dyn SolverAccess>>> = Mutex::new(None);

#[test]
fn escaped_solver_access_after_its_borrow_ended() {
    let mut log = vec!["start".to_owned()];
    let k = ContextKey::fresh();
    {
        let mut acc = ShortAccess { log: &mut log };
        with_solver_access(&mut acc, k, |h| {
            *ESCAPED.lock().unwrap() = Some(h.clone());
            h.with_mut(k, |a| a.assemble_matrix("K")).unwrap().unwrap();
            // nested use from inside a use (a callback re-entering): refused
            let nested = h.with_mut(k, |a| {
                let inner = ESCAPED.lock().unwrap().clone().unwrap();
                let refused = inner.with_mut(k, |b| b.assemble_residual()).is_err();
                a.assemble_lumped_matrix("M").unwrap();
                refused
            });
            assert!(nested.unwrap());
        });
    }
    log.push("owner".into());
    drop(log); // the borrowed local is gone
    let h = ESCAPED.lock().unwrap().take().unwrap();
    assert!(h.with_mut(k, |a| a.assemble_residual()).is_err());
}
