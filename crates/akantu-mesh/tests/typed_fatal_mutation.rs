// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): contract tests for the collective error
// paths of src/mesh/mesh.cc:105-150 (sendEvent) and
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc:49-94 (CohesiveMeshGlobalDataUpdater
// as a distributed hook). Upstream lets a rank throw while its peers wait;
// these tests pin the port's handling of typed fatal results.

//! The mesh domain: a rank-specific
//! `Error::Fatal { kind: Agreed }` returned by the caller's topology
//! closure, by the distributed hook's `prepare`/`apply` or by a consumer is
//! agreed as fatal on every rank (each rank's domain is poisoned with the
//! agreed kind and returns `Error::Fatal { kind: Agreed }`), never
//! downgraded to a recoverable rollback on the peers; ordinary errors at
//! the same boundaries stay recoverable before commit. A
//! `Error::Fatal { kind: Transport }` keeps its kind and takes the abort
//! path: the reporting rank attempts no further agreement on the
//! communicator.

use std::time::{Duration, Instant};

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::EventPriority;
use akantu_core::{ElementType, Error, FatalKind, GhostType, Result};
use akantu_mesh::{
    DistributedHook, Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, MeshTransaction,
    SharedMesh,
};

const TIMEOUT: Duration = Duration::from_secs(20);
const PROMPT: Duration = Duration::from_secs(2);

/// 1D bar n0 - n1 - n2; distributed (so the hook stage runs) or not.
fn bar(comm: Communicator, distributed: bool) -> Mesh {
    let mut m = Mesh::with_communicator(1, "bar", comm).unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [0.0, 1.0, 2.0] {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
        drop(c);
        if distributed {
            a.set_distributed();
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// The error injected at one boundary: ordinary, or typed fatal.
fn injected(kind: Option<FatalKind>, what: &str) -> Error {
    let e = Error::Numerical(format!("injected {what} failure"));
    match kind {
        Some(k) => Error::fatal(k, e),
        None => e,
    }
}

/// Add one node and record its event.
fn add_node(tx: &mut MeshTransaction<'_>) -> Result<()> {
    let n = tx.push_node(MeshPart::Main, &[9.0])?;
    tx.record(
        MeshPart::Main,
        MeshEvent::NodesAdded {
            nodes: vec![n],
            old_nodes: None,
            origin: "test".into(),
            extension: None,
        },
    );
    Ok(())
}

/// Distributed hook with injectable `prepare`/`apply` failures.
#[derive(Default)]
struct Hook {
    prepare: Option<Option<FatalKind>>,
    apply: Option<Option<FatalKind>>,
}

impl DistributedHook for Hook {
    fn prepare(&mut self, _: &Mesh, _: &[(MeshPart, MeshEvent)]) -> Result<()> {
        match self.prepare {
            Some(kind) => Err(injected(kind, "hook prepare")),
            None => Ok(()),
        }
    }
    fn apply(
        &mut self,
        _: &mut MeshTransaction<'_>,
        _: &[(MeshPart, MeshEvent)],
        _: &mut akantu_comm::phase::Phases,
    ) -> Result<()> {
        match self.apply {
            Some(kind) => Err(injected(kind, "hook apply")),
            None => Ok(()),
        }
    }
}

/// Consumer with an injectable failure (a model propagating an unusable
/// dependent owner's error).
struct Consumer(Option<Option<FatalKind>>);

impl MeshEventHandler for Consumer {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority(50)
    }
    fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        match self.0 {
            Some(kind) => Err(injected(kind, "consumer")),
            None => Ok(()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Boundary {
    Topology,
    HookPrepare,
    HookApply,
    Consumer,
}

/// Outcome of one injected mutation on a rank: the returned error's fatal
/// kind, the domain's poison kind, whether reads are refused, the message.
type Outcome = (Option<FatalKind>, Option<FatalKind>, bool, String);

/// Run one mutation on a distributed bar with a failure of class `kind`
/// injected at `at` on rank `bad` only.
fn mutate_with(comm: &Communicator, at: Boundary, bad: i32, kind: Option<FatalKind>) -> Outcome {
    mutate_timed(comm, at, bad, kind).0
}

/// [`mutate_with`], also returning the time this rank spent in the
/// mutation itself. The set-up (mesh, bounding box, consumer) is excluded
/// and followed by a barrier, so the measured interval starts with both
/// ranks at the mutation and holds only the step under test.
fn mutate_timed(
    comm: &Communicator,
    at: Boundary,
    bad: i32,
    kind: Option<FatalKind>,
) -> (Outcome, Duration) {
    let me = comm.rank() == bad;
    let fail = |b: Boundary| (me && at == b).then_some(kind);
    let mesh = SharedMesh::new(bar(comm.clone(), true));
    let token = mesh
        .register_consumer("model", EventPriority(50), MeshPart::Main)
        .unwrap();
    let mut consumer = Consumer(fail(Boundary::Consumer));
    let mut hook = Hook {
        prepare: fail(Boundary::HookPrepare),
        apply: fail(Boundary::HookApply),
    };
    let topology = fail(Boundary::Topology);
    comm.barrier().unwrap();
    let t0 = Instant::now();
    let r = mesh.mutate(
        &mut [(&token, &mut consumer)],
        |tx| {
            add_node(tx)?;
            match topology {
                Some(kind) => Err(injected(kind, "topology")),
                None => Ok(()),
            }
        },
        Some(&mut hook),
    );
    let elapsed = t0.elapsed();
    let e = r.unwrap_err();
    (
        (
            e.fatal_kind(),
            mesh.poison_kind().unwrap(),
            mesh.read().is_err(),
            e.to_string(),
        ),
        elapsed,
    )
}

/// A typed fatal result at every mutation boundary — before
/// commit (topology closure, hook prepare, hook apply) and after
/// (consumer) — on either rank is agreed fatal: every rank returns
/// `Error::Fatal { kind: Agreed }` and every rank's domain is poisoned
/// with the agreed kind and refuses reads.
#[test]
fn typed_fatal_results_poison_every_rank_at_every_boundary() {
    for at in [
        Boundary::Topology,
        Boundary::HookPrepare,
        Boundary::HookApply,
        Boundary::Consumer,
    ] {
        for bad in [0, 1] {
            let t0 = Instant::now();
            let out = run_local(2, TIMEOUT, |comm| {
                mutate_with(&comm, at, bad, Some(FatalKind::Agreed))
            });
            assert!(t0.elapsed() < PROMPT, "{at:?}: {:?}", t0.elapsed());
            let a = Some(FatalKind::Agreed);
            for (rank, o) in out.iter().enumerate() {
                assert_eq!(
                    (o.0, o.1, o.2),
                    (a, a, true),
                    "{at:?}, bad {bad}: rank {rank}"
                );
                if rank == usize::try_from(bad).unwrap() {
                    assert!(o.3.contains("injected"), "own error kept: {}", o.3);
                } else {
                    assert!(o.3.contains("failed on another rank (fatal"), "{}", o.3);
                }
            }
        }
    }
}

/// Control: ordinary errors before commit stay recoverable (rolled back,
/// not poisoned) on every rank; after commit they are fatal as before.
#[test]
fn ordinary_errors_keep_their_classification() {
    for (at, expected) in [
        (Boundary::Topology, None),
        (Boundary::HookPrepare, None),
        (Boundary::HookApply, None),
        (Boundary::Consumer, Some(FatalKind::Agreed)),
    ] {
        let out = run_local(2, TIMEOUT, |comm| mutate_with(&comm, at, 1, None));
        for o in &out {
            assert_eq!(
                (o.0, o.1, o.2),
                (expected, expected, expected.is_some()),
                "{at:?}"
            );
        }
    }
}

/// World timeout of the abort-path test. The peer observes the transport
/// failure through it, and the set-up (whose bounding box is an
/// all-reduce) runs under it too: ten times the 300 ms that a loaded
/// 4-vCPU sandbox once exceeded in the set-up.
const ABORT_WORLD: Duration = Duration::from_secs(3);
/// Deadline of the reporting rank's abort path, measured around the
/// mutation alone ([`mutate_timed`]). It is well below [`ABORT_WORLD`], so
/// an abort path that waits for the timeout (e.g. one more agreement)
/// fails it, and far above the few milliseconds the path takes.
const ABORT_PROMPT: Duration = Duration::from_secs(1);

/// A typed **transport**-fatal result (topology closure or consumer) keeps
/// its kind and takes the abort path: the reporting rank returns at once —
/// it attempts no further agreement — poisoned with the transport kind and
/// its communicator broken; its peer, still waiting in the agreement,
/// observes the transport failure itself (diagnostic timeout).
#[test]
fn typed_transport_fatal_results_take_the_abort_path() {
    const _: () = assert!(ABORT_PROMPT.as_millis() * 3 <= ABORT_WORLD.as_millis());
    for at in [Boundary::Topology, Boundary::Consumer] {
        let out = run_local(2, ABORT_WORLD, |comm| {
            let (o, elapsed) = mutate_timed(&comm, at, 0, Some(FatalKind::Transport));
            (
                o.0,
                o.1,
                comm.broken().is_some(),
                elapsed < ABORT_PROMPT,
                o.3.contains("timed out"),
            )
        });
        let t = Some(FatalKind::Transport);
        assert_eq!(
            out[0],
            (t, t, true, true, false),
            "{at:?}: never communicated"
        );
        assert_eq!(
            (out[1].0, out[1].1, out[1].2, out[1].4),
            (t, t, true, true),
            "{at:?}"
        );
    }
}

// ------------------------------------------------------------------

/// Hook or consumer that first runs an **internal** agreement through the
/// mutation's scope (failing on rank `first`, with severity `inner`), then
/// returns a late result — an error detected during its cleanup — on rank
/// `bad` (ordinary, or typed fatal of the given kind). Nothing communicates
/// after the internal agreement.
#[derive(Clone, Copy)]
struct Late {
    first: i32,
    inner: akantu_comm::phase::Severity,
    bad: i32,
    late: Option<FatalKind>,
}

impl Late {
    fn run(&self, ph: &mut akantu_comm::phase::Phases) -> Result<()> {
        let rank = ph.rank_size().0;
        let local = if rank == self.first {
            Err(Error::invalid("first recoverable failure"))
        } else {
            Ok(())
        };
        let _ = ph.agree("inner", local, self.inner);
        if rank == self.bad {
            Err(injected(self.late, "late cleanup"))
        } else {
            Ok(())
        }
    }
}

impl DistributedHook for Late {
    fn prepare(&mut self, _: &Mesh, _: &[(MeshPart, MeshEvent)]) -> Result<()> {
        Ok(())
    }
    fn apply(
        &mut self,
        _: &mut MeshTransaction<'_>,
        _: &[(MeshPart, MeshEvent)],
        ph: &mut akantu_comm::phase::Phases,
    ) -> Result<()> {
        self.run(ph)
    }
}

impl MeshEventHandler for Late {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority(50)
    }
    fn on_mesh_event(&mut self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        Ok(())
    }
    fn on_mesh_event_phased(
        &mut self,
        _: &Mesh,
        _: &MeshEvent,
        _: EventPriority,
        ph: &mut akantu_comm::phase::Phases,
    ) -> Result<()> {
        self.run(ph)
    }
}

/// One mutation with the late result in the hook (`in_hook`) or in the
/// consumer: `(returned fatal kind, poison kind, reads refused,
/// communicator broken)`.
fn late_mutation(
    comm: &Communicator,
    in_hook: bool,
    late: Late,
) -> (Option<FatalKind>, Option<FatalKind>, bool, bool) {
    let mesh = SharedMesh::new(bar(comm.clone(), true));
    let token = mesh
        .register_consumer("model", EventPriority(50), MeshPart::Main)
        .unwrap();
    let mut consumer: Box<dyn MeshEventHandler> = if in_hook {
        Box::new(Consumer(None))
    } else {
        Box::new(late)
    };
    let mut hook: Box<dyn DistributedHook> = if in_hook {
        Box::new(late)
    } else {
        Box::new(Hook::default())
    };
    let r = mesh.mutate(&mut [(&token, &mut *consumer)], add_node, Some(&mut *hook));
    (
        r.unwrap_err().fatal_kind(),
        mesh.poison_kind().unwrap(),
        mesh.read().is_err(),
        comm.broken().is_some(),
    )
}

/// A hook (before commit) or a consumer (after commit) whose
/// internal agreement already failed recoverably, then returns a typed
/// fatal result on one rank. The late result is classified by the scope
/// without communicating: a transport-kind result, or an agreed-kind result
/// that can no longer be agreed (the peers already returned), takes the
/// abort path on the reporting rank — `Transport` returned and poisoned,
/// communicator broken. The peers keep the classification of the failure
/// they observed: recoverable rollback before commit, agreed poisoning
/// after commit. Ordinary late errors keep the existing classification.
/// Every combination of failing ranks; nothing waits for a timeout.
#[test]
fn late_results_after_an_internal_failure_are_classified() {
    use akantu_comm::phase::Severity;
    let t = Some(FatalKind::Transport);
    let a = Some(FatalKind::Agreed);
    for in_hook in [true, false] {
        for late in [None, a, t] {
            for (first, bad) in [(0, 0), (0, 1), (1, 0), (1, 1)] {
                let spec = Late {
                    first,
                    inner: Severity::Recoverable,
                    bad,
                    late,
                };
                let t0 = Instant::now();
                let out = run_local(2, TIMEOUT, |comm| late_mutation(&comm, in_hook, spec));
                assert!(t0.elapsed() < PROMPT, "no rank waited for a timeout");
                // The class every rank observed from the failed internal
                // agreement: recoverable before commit, agreed after.
                let observed = if in_hook {
                    (None, None, false, false)
                } else {
                    (a, a, true, false)
                };
                for (rank, o) in out.iter().enumerate() {
                    let reporting = rank == usize::try_from(bad).unwrap();
                    let expected = if reporting && late.is_some() {
                        (t, t, true, true)
                    } else {
                        observed
                    };
                    assert_eq!(
                        *o, expected,
                        "hook {in_hook}, late {late:?}, first {first}, bad {bad}: rank {rank}"
                    );
                }
            }
        }
    }
}

/// Control: after an internal agreement that failed **fatally**
/// (agreed on every rank), a late agreed-kind result is already common
/// knowledge and stays `Agreed`; a late transport-kind result still takes
/// the abort path on the reporting rank.
#[test]
fn late_results_after_an_agreed_fatal_internal_failure() {
    use akantu_comm::phase::Severity;
    let t = Some(FatalKind::Transport);
    let a = Some(FatalKind::Agreed);
    for in_hook in [true, false] {
        for (late, reporting_expected) in [(a, (a, a, true, false)), (t, (t, t, true, true))] {
            let spec = Late {
                first: 1,
                inner: Severity::Fatal,
                bad: 0,
                late,
            };
            let out = run_local(2, TIMEOUT, |comm| late_mutation(&comm, in_hook, spec));
            assert_eq!(out[0], reporting_expected, "hook {in_hook}, late {late:?}");
            assert_eq!(out[1], (a, a, true, false), "hook {in_hook}, late {late:?}");
        }
    }
}
