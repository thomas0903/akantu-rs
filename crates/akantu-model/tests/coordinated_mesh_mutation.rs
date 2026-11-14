// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): event order and payloads from
// src/model/solid_mechanics/solid_mechanics_model_cohesive/
// solid_mechanics_model_cohesive.cc:49-94 (elements event before nodes
// event, participation by global counts) and :504-546 (onNodesAdded copies
// old-node values), src/mesh_utils/cohesive_element_inserter_helper.cc
// (facet events first), src/mesh/mesh.cc:113-150, element_group.cc:176-202.

//! Coordinated mutation of one mesh
//! shared by two models (each with its own FE engine stand-in and a real
//! `DofManager`), a facet FE engine, a dumper and a Python handle, with a
//! toy 1D facet-doubling insertion.
//!
//! * Every failure before commit — after the group rebuild, after
//!   successful hook edits on a peer, in the schedule/count stage, for an
//!   invalid facet event — restores the **complete** mesh state (compared
//!   through `Mesh::state_fingerprint`) on every rank; hooks get explicit
//!   prepare/commit/abort.
//! * No collective (bounding boxes included) starts before the
//!   agreement that makes it safe; rank-specific failures and re-entry
//!   inside communicating consumers (the DOF manager's numbering
//!   collectives) complete on all ranks long before the diagnostic timeout.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use akantu_comm::phase::Phases;
use akantu_comm::{Communicator, Synchronizer, run_local};
use akantu_core::enums::EventPriority;
use akantu_core::{ElementType, Error, GhostType, Idx, NodeFlag, Result};
use akantu_mesh::{
    ConsumerToken, DistributedHook, Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart,
    MeshTransaction, SharedMesh,
};
use akantu_solver::DofManager;

type Log = Arc<Mutex<Vec<String>>>;

/// Generous diagnostic timeout: every test asserts completion far below it.
const TIMEOUT: Duration = Duration::from_secs(20);
const PROMPT: Duration = Duration::from_secs(3);

/// 1D bar n0 - n1 - n2 (segments e0, e1) with a point facet mesh and an
/// element group "right" = {e1}.
fn bar(comm: Communicator, distributed: bool, facets: bool) -> Mesh {
    let mut m = Mesh::with_communicator(1, "bar", comm.clone()).unwrap();
    let mut f = Mesh::with_communicator(1, "bar:facets", comm).unwrap();
    {
        let mut a = MeshAccessor::new(&mut f);
        for (i, x) in [0.0, 1.0, 2.0].into_iter().enumerate() {
            a.push_node(&[x]).unwrap();
            a.connectivity_mut(ElementType::Point1, GhostType::NotGhost)
                .push_tuple(&[i32::try_from(i).unwrap()]);
        }
    }
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [0.0, 1.0, 2.0] {
            a.push_node(&[x]).unwrap();
        }
        let mut c = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[1, 2]);
        drop(c);
        if facets {
            a.set_mesh_facets(f);
        }
        if distributed {
            a.set_distributed(); // stage 2 (synchronizers/global ids) runs
        }
    }
    let mut g = m.groups_mut().create_element_group("right", 1).unwrap();
    g.elements
        .as_map_mut()
        .get_or_insert_with(ElementType::Segment2, GhostType::NotGhost, || {
            akantu_core::Array::new(1).into()
        })
        .borrow_mut()
        .push_value(1);
    g.node_group.set_nodes(vec![1, 2]);
    m.compute_bounding_box().unwrap();
    m
}

/// Toy extrinsic insertion at node `n` (upstream order of events: facets,
/// then elements, then nodes). `insert = false` records empty events (a
/// zero-work rank).
fn insert_at(tx: &mut MeshTransaction<'_>, n: Idx, insert: bool, fail_midway: bool) -> Result<()> {
    let (mut facets, mut cohesive, mut new_nodes, mut old_nodes) = (vec![], vec![], vec![], vec![]);
    if insert {
        let n2 = tx.duplicate_node(MeshPart::Main, n)?;
        if tx.mesh().mesh_facets().is_some() {
            facets.push(tx.push_element(
                MeshPart::Facets,
                ElementType::Point1,
                GhostType::NotGhost,
                &[n2],
            )?);
        }
        if fail_midway {
            return Err(Error::Mesh("injected topology failure".into()));
        }
        let right = akantu_core::Element::new(ElementType::Segment2, 1, GhostType::NotGhost);
        tx.set_element_nodes(MeshPart::Main, right, &[n2, 2])?;
        cohesive.push(tx.push_element(
            MeshPart::Main,
            ElementType::Cohesive1d2,
            GhostType::NotGhost,
            &[n, n2],
        )?);
        new_nodes.push(n2);
        old_nodes.push(n);
    }
    if tx.mesh().mesh_facets().is_some() {
        tx.record(
            MeshPart::Facets,
            MeshEvent::ElementsAdded {
                elements: facets,
                constitutive_law: None,
                origin: "inserter".into(),
                extension: None,
            },
        );
    }
    tx.record(
        MeshPart::Main,
        MeshEvent::ElementsAdded {
            elements: cohesive,
            constitutive_law: Some(vec![0]),
            origin: "inserter".into(),
            extension: None,
        },
    );
    tx.record(
        MeshPart::Main,
        MeshEvent::NodesAdded {
            nodes: new_nodes,
            old_nodes: Some(old_nodes),
            origin: "inserter".into(),
            extension: None,
        },
    );
    Ok(())
}

fn tag(e: &MeshEvent) -> &'static str {
    match e {
        MeshEvent::NodesAdded { .. } => "nodes",
        MeshEvent::ElementsAdded { .. } => "elements",
        _ => "other",
    }
}

/// Generic logging consumer (FE engines, dumper, Python wrapper).
struct Logger {
    name: &'static str,
    prio: EventPriority,
    log: Log,
    fail: bool,
    fail_on: Option<&'static str>,
    reenter: Option<SharedMesh>,
    reentry_result: Option<Result<()>>,
}

impl MeshEventHandler for Logger {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        self.prio
    }
    fn on_mesh_event(&mut self, mesh: &Mesh, e: &MeshEvent) -> Result<()> {
        self.log
            .lock()
            .unwrap()
            .push(format!("{}:{}:{}", self.name, tag(e), mesh.nb_nodes()));
        if let Some(m) = &self.reenter {
            // A Python callback trying to mutate during dispatch.
            self.reentry_result = Some(m.mutate(&mut [], |_| Ok(()), None).map(|_| ()));
        }
        if self.fail && matches!(e, MeshEvent::NodesAdded { .. }) {
            return Err(Error::Numerical("injected consumer failure".into()));
        }
        if self.fail_on == Some(tag(e)) {
            return Err(Error::Numerical("injected consumer failure".into()));
        }
        Ok(())
    }
}

/// How the model's DOF level misbehaves (rank-specific injections).
#[derive(Clone, Copy, PartialEq, Eq)]
enum DofFault {
    None,
    /// Attempt a re-entrant mutation, then return its error without
    /// running the (communicating) DOF update.
    Reenter,
    /// Pass an invalid node list to the DOF manager (preflight failure
    /// inside the communicating consumer).
    InvalidNodes,
}

/// Model stand-in: **one** consumer reacting at three levels, like an
/// upstream model whose FE engine (9), DOF manager (20) and own fields (94)
/// are separate handlers. Its fields: a nodal DOF family owned by a real
/// `DofManager` (copied from old nodes for the cohesive model, zero for
/// the other) and one value per cohesive element. The DOF level
/// communicates (numbering collectives) through the mutation's phases.
struct Model {
    name: &'static str,
    copy_old: bool,
    dofs: DofManager,
    field: String,
    cohesive_state: Vec<f64>,
    log: Log,
    fault: DofFault,
    mesh: Option<SharedMesh>,
    reentry: Option<String>,
}

impl MeshEventHandler for Model {
    fn check_mesh_event(&self, mesh: &Mesh, e: &MeshEvent) -> Result<()> {
        // forwards to the DOF manager it dispatches to
        self.dofs.check_mesh_event(mesh, e)
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority::MODEL
    }
    fn event_priorities(&self) -> Vec<EventPriority> {
        vec![
            EventPriority::FE_ENGINE,
            EventPriority::DOF_MANAGER,
            EventPriority::MODEL,
        ]
    }
    fn on_mesh_event(&mut self, mesh: &Mesh, e: &MeshEvent) -> Result<()> {
        self.on_mesh_event_at(mesh, e, EventPriority::MODEL)
    }
    fn on_mesh_event_phased(
        &mut self,
        mesh: &Mesh,
        e: &MeshEvent,
        level: EventPriority,
        phases: &mut Phases,
    ) -> Result<()> {
        let who = match level {
            EventPriority::FE_ENGINE => format!("fe:{}", self.name),
            EventPriority::DOF_MANAGER => format!("dofs:{}", self.name),
            _ => self.name.to_string(),
        };
        self.log
            .lock()
            .unwrap()
            .push(format!("{who}:{}:{}", tag(e), mesh.nb_nodes()));
        match (level, e) {
            (EventPriority::DOF_MANAGER, MeshEvent::NodesAdded { nodes, .. }) => match self.fault {
                DofFault::Reenter => {
                    let m = self.mesh.as_ref().expect("mesh handle");
                    let err = m.mutate(&mut [], |_| Ok(()), None).map(|_| ()).unwrap_err();
                    self.reentry = Some(err.to_string());
                    Err(err)
                }
                DofFault::InvalidNodes => self.dofs.on_nodes_added_phased(mesh, &[1000], phases),
                DofFault::None => self.dofs.on_nodes_added_phased(mesh, nodes, phases),
            },
            (EventPriority::DOF_MANAGER, _) => {
                self.dofs
                    .on_mesh_event_phased(mesh, e, EventPriority::DOF_MANAGER, phases)
            }
            (EventPriority::MODEL, MeshEvent::ElementsAdded { elements, .. }) => {
                self.cohesive_state.extend(elements.iter().map(|_| 1.0));
                Ok(())
            }
            (
                EventPriority::MODEL,
                MeshEvent::NodesAdded {
                    nodes,
                    old_nodes: Some(old),
                    ..
                },
            ) if self.copy_old => {
                // DOF arrays were resized at level 20; copy old → new
                // (upstream SolidMechanicsModelCohesive::onNodesAdded).
                let mut u = self.dofs.dofs_mut(&self.field)?;
                for (&nn, &o) in nodes.iter().zip(old) {
                    let v = u.tuple(o as usize).to_vec();
                    u.tuple_mut(nn as usize).copy_from_slice(&v);
                }
                Ok(())
            }
            _ => Ok(()),
        }
    }
}

/// Distributed hook stand-in (upstream `CohesiveMeshGlobalDataUpdater`):
/// numbers the new nodes globally with an exclusive scan **through the
/// phase API**, edits flags, owner ranks, synchronizer schemes, groups and
/// mesh data through the journaled transaction, and stages external state.
#[derive(Default)]
struct Hook {
    fail_prepare: bool,
    fail_apply: bool,
    prepared: u32,
    committed: u32,
    aborted: u32,
    /// External state staged by `prepare`, made permanent by `commit`.
    staged: Option<usize>,
    permanent: usize,
}

impl DistributedHook for Hook {
    fn prepare(&mut self, _: &Mesh, events: &[(MeshPart, MeshEvent)]) -> Result<()> {
        self.prepared += 1;
        if self.fail_prepare {
            return Err(Error::Mesh("injected hook prepare failure".into()));
        }
        self.staged = Some(events.len());
        Ok(())
    }
    fn apply(
        &mut self,
        tx: &mut MeshTransaction<'_>,
        events: &[(MeshPart, MeshEvent)],
        phases: &mut Phases,
    ) -> Result<()> {
        // node events of the shared node set: read on Main (the Facets
        // twin repeats them)
        let new_nodes: Vec<Idx> = events
            .iter()
            .filter(|(p, _)| *p == MeshPart::Main)
            .filter_map(|(_, e)| match e {
                MeshEvent::NodesAdded { nodes, .. } => Some(nodes.clone()),
                _ => None,
            })
            .flatten()
            .collect();
        let n = u64::try_from(new_nodes.len()).unwrap();
        let offset = phases.run("hook:global_ids", Ok(n), |c, n| {
            Ok(Ok(c.exclusive_scan_sum_u64(n)?))
        })?;
        for (k, &node) in new_nodes.iter().enumerate() {
            let gid = 100 + i64::try_from(offset).unwrap() + i64::try_from(k).unwrap();
            tx.set_node_global_id(MeshPart::Main, node, gid)?;
        }
        tx.set_node_flag(MeshPart::Main, 0, NodeFlag::MASTER)?;
        tx.set_node_prank(MeshPart::Main, 1, Some(1))?;
        let comm = tx.mesh().communicator().clone();
        let mut sync = Synchronizer::new("cohesive-nodes", comm);
        sync.communications.send.insert(0, new_nodes.clone());
        tx.synchronizers_mut(MeshPart::Main)?.node = Some(sync);
        tx.groups_mut(MeshPart::Main)?.create_node_group("hook")?;
        tx.ghosts_counters_mut(MeshPart::Main)?;
        if self.fail_apply {
            return Err(Error::Mesh("injected hook apply failure".into()));
        }
        Ok(())
    }
    fn commit(&mut self) {
        self.committed += 1;
        self.permanent += self.staged.take().unwrap_or(0);
    }
    fn abort(&mut self) {
        self.aborted += 1;
        self.staged = None;
    }
}

struct Sim {
    mesh: SharedMesh,
    log: Log,
    a: Model,
    b: Model,
    facet_fe: Logger,
    dumper: Logger,
    python: Logger,
    tokens: Vec<ConsumerToken>,
}

fn sim_with(comm: Communicator, distributed: bool, facets: bool) -> Sim {
    let mesh = SharedMesh::new(bar(comm.clone(), distributed, facets));
    let log: Log = Arc::default();
    let logger = |name, prio| Logger {
        name,
        prio,
        log: Arc::clone(&log),
        fail: false,
        fail_on: None,
        reenter: None,
        reentry_result: None,
    };
    let model = |name, copy_old, field: &str| {
        let mut dofs = DofManager::new(name, comm.clone());
        dofs.register_nodal_dofs(&mesh.read().unwrap(), field, 1)
            .unwrap();
        let mut u = dofs.dofs_mut(field).unwrap();
        u.as_mut_slice().copy_from_slice(&[10.0, 11.0, 12.0]);
        drop(u);
        Model {
            name,
            copy_old,
            dofs,
            field: field.into(),
            cohesive_state: vec![],
            log: Arc::clone(&log),
            fault: DofFault::None,
            mesh: Some(mesh.clone()),
            reentry: None,
        }
    };
    let levels = [
        EventPriority::FE_ENGINE,
        EventPriority::DOF_MANAGER,
        EventPriority::MODEL,
    ];
    let a = model("cohesive", true, "displacement");
    let b = model("heat", false, "temperature");
    // Registration order = creation order (identical on every rank).
    let tokens = vec![
        mesh.register_multilevel_consumer("model:cohesive", &levels, MeshPart::Main)
            .unwrap(),
        mesh.register_consumer("fe:facets", EventPriority::FE_ENGINE, MeshPart::Facets)
            .unwrap(),
        mesh.register_multilevel_consumer("model:heat", &levels, MeshPart::Main)
            .unwrap(),
        mesh.register_consumer("dumper", EventPriority(200), MeshPart::Main)
            .unwrap(),
        mesh.register_consumer("python", EventPriority(250), MeshPart::Main)
            .unwrap(),
    ];
    Sim {
        a,
        b,
        facet_fe: logger("fe:facets", EventPriority::FE_ENGINE),
        dumper: logger("dumper", EventPriority(200)),
        python: logger("python", EventPriority(250)),
        log,
        mesh,
        tokens,
    }
}

fn sim(comm: Communicator) -> Sim {
    let distributed = comm.size() > 1;
    sim_with(comm, distributed, true)
}

impl Sim {
    /// The coordinator (a coupler, the Python simulation object, …):
    /// borrows every consumer disjointly and runs the staged mutation.
    fn mutate(
        &mut self,
        change: impl FnOnce(&mut MeshTransaction<'_>) -> Result<()>,
        hook: Option<&mut dyn DistributedHook>,
        omit_heat_model: bool,
    ) -> Result<u64> {
        let t = &self.tokens;
        let mut handlers: Vec<(&ConsumerToken, &mut dyn MeshEventHandler)> = vec![
            (&t[0], &mut self.a),
            (&t[1], &mut self.facet_fe),
            (&t[3], &mut self.dumper),
            (&t[4], &mut self.python),
        ];
        if !omit_heat_model {
            handlers.insert(2, (&t[2], &mut self.b));
        }
        self.mesh
            .mutate(&mut handlers, change, hook)
            .map(|c| c.revision)
    }

    fn insert(&mut self, insert: bool, fail_topology: bool, omit_heat_model: bool) -> Result<u64> {
        let mut hook = Hook::default();
        self.mutate(
            |tx| insert_at(tx, 1, insert, fail_topology),
            Some(&mut hook),
            omit_heat_model,
        )
    }

    fn fingerprint(&self) -> String {
        self.mesh.read().unwrap().state_fingerprint()
    }

    fn dof_state(&self) -> (usize, usize, i64) {
        (
            self.a.dofs.dofs("displacement").unwrap().len(),
            self.a.dofs.local_system_size(),
            self.a.dofs.system_size(),
        )
    }
}

#[test]
fn insertion_updates_all_consumers_in_upstream_order() {
    let mut s = sim(Communicator::serial());
    let rev = s.insert(true, false, false).unwrap();
    assert_eq!(rev, 1);
    let log = s.log.lock().unwrap().clone();
    assert_eq!(
        log,
        [
            "fe:facets:elements:4",
            // elements event before nodes event; ascending priority, and at
            // equal priority the most recently registered consumer first
            // (upstream `std::lower_bound` insertion,
            // fe:heat before fe:cohesive).
            "fe:heat:elements:4",
            "fe:cohesive:elements:4",
            "dofs:heat:elements:4",
            "dofs:cohesive:elements:4",
            "heat:elements:4",
            "cohesive:elements:4",
            "dumper:elements:4",
            "python:elements:4",
            "fe:heat:nodes:4",
            "fe:cohesive:nodes:4",
            "dofs:heat:nodes:4",
            "dofs:cohesive:nodes:4",
            "heat:nodes:4",
            "cohesive:nodes:4",
            "dumper:nodes:4",
            "python:nodes:4",
            // The facet mesh's twin of the node event
            "fe:facets:nodes:4",
        ]
    );
    // DOF managers (priority 20) renumbered; model fields copied at 94.
    assert_eq!(
        s.a.dofs.dofs("displacement").unwrap().as_slice(),
        &[10.0, 11.0, 12.0, 11.0]
    );
    assert_eq!(
        s.b.dofs.dofs("temperature").unwrap().as_slice(),
        &[10.0, 11.0, 12.0, 0.0]
    );
    assert_eq!((s.a.dofs.system_size(), s.b.dofs.system_size()), (4, 4));
    assert_eq!(s.a.cohesive_state, vec![1.0]);
    let mesh = s.mesh.read().unwrap();
    assert_eq!(
        mesh.connectivity(ElementType::Segment2, GhostType::NotGhost)
            .unwrap()
            .tuple(1),
        &[3, 2]
    );
    assert_eq!(
        mesh.groups().element_group("right").unwrap().nodes(),
        &[2, 3],
        "groups rebuilt"
    );
    assert_eq!(
        mesh.mesh_facets().unwrap().nb_nodes(),
        4,
        "facet mesh shares numbering"
    );
    drop(mesh);
    for t in &s.tokens {
        s.mesh.check_current(t).unwrap();
    }
}

#[test]
fn missing_consumer_and_topology_failure_leave_the_mesh_unchanged() {
    let mut s = sim(Communicator::serial());
    let before = s.fingerprint();
    let err = s.insert(true, false, true).unwrap_err();
    assert!(err.to_string().contains("model:heat"), "{err}");
    let err = s.insert(true, true, false).unwrap_err();
    assert!(err.to_string().contains("injected topology failure"));
    assert_eq!(s.fingerprint(), before, "complete state restored");
    assert!(s.log.lock().unwrap().is_empty(), "no consumer saw an event");
    assert_eq!(s.mesh.revision().unwrap(), 0);
    s.insert(true, false, false).unwrap();
}

/// A distributed-flagged serial mesh: the
/// topology duplicates node 1 and reconnects, the group rebuild runs, then
/// the hook fails. Everything — connectivity, nodes, **group node lists**,
/// bounding boxes, flags, ids, synchronizers — is restored; the hook is
/// aborted; the domain stays usable.
#[test]
fn hook_failure_after_group_rebuild_restores_complete_state() {
    for (fail_prepare, fail_apply) in [(true, false), (false, true)] {
        let mut s = sim_with(Communicator::serial(), true, true);
        let before = s.fingerprint();
        let mut hook = Hook {
            fail_prepare,
            fail_apply,
            ..Hook::default()
        };
        let err = s
            .mutate(|tx| insert_at(tx, 1, true, false), Some(&mut hook), false)
            .unwrap_err();
        assert!(err.to_string().contains("injected hook"), "{err}");
        assert_eq!(s.fingerprint(), before, "complete state restored");
        let mesh = s.mesh.read().unwrap();
        assert_eq!(
            mesh.groups().element_group("right").unwrap().nodes(),
            &[1, 2]
        );
        assert!(mesh.groups().node_group("hook").is_err());
        drop(mesh);
        assert_eq!((hook.prepared, hook.committed), (1, 0));
        assert_eq!(
            hook.aborted,
            u32::from(fail_apply),
            "abort after a successful prepare"
        );
        assert!(hook.staged.is_none() && hook.permanent == 0);
        assert_eq!(s.mesh.revision().unwrap(), 0);
        assert!(s.mesh.poisoned().unwrap().is_none());
        // The domain is fully usable afterwards.
        let mut ok = Hook::default();
        s.mutate(|tx| insert_at(tx, 1, true, false), Some(&mut ok), false)
            .unwrap();
        // 4 events: the 3 recorded by the change plus the facet twin of
        // its node event
        assert_eq!((ok.committed, ok.aborted, ok.permanent), (1, 0, 4));
        let mesh = s.mesh.read().unwrap();
        assert_eq!(mesh.node_global_id(3), 100);
        assert_eq!(mesh.node_flag(0), NodeFlag::MASTER);
        assert_eq!(mesh.node_prank(1), 1);
        assert!(mesh.synchronizers().node.is_some());
    }
}

/// Rank 0's hook succeeds (global ids through a phase, flags,
/// owner ranks, synchronizer schemes, groups), rank 1's hook fails after
/// its edits: both ranks restore their complete state and abort.
#[test]
fn remote_hook_edits_are_rolled_back_when_a_peer_fails() {
    let t0 = Instant::now();
    let out = run_local(2, TIMEOUT, |comm| {
        let mut s = sim(comm.clone());
        let before = s.fingerprint();
        let dofs_before = s.dof_state();
        let mut hook = Hook {
            fail_apply: comm.rank() == 1,
            ..Hook::default()
        };
        let r = s.mutate(
            |tx| insert_at(tx, 1, comm.rank() == 0, false),
            Some(&mut hook),
            false,
        );
        (
            r.is_err(),
            s.fingerprint() == before,
            s.dof_state() == dofs_before,
            (hook.prepared, hook.committed, hook.aborted),
            s.mesh.poisoned().unwrap().is_none(),
            s.log.lock().unwrap().is_empty(),
        )
    });
    assert!(t0.elapsed() < PROMPT, "{:?}", t0.elapsed());
    for o in &out {
        assert_eq!(*o, (true, true, true, (1, 0, 1), true, true));
    }
}

/// Count stage: ranks recording different event schedules
/// — same number of events but a different kind, or a different number —
/// fail at the descriptor agreement, before any count collective, and
/// restore their complete state.
#[test]
fn schedule_mismatch_rolls_back_on_all_ranks() {
    for variant in 0..2 {
        let t0 = Instant::now();
        let out = run_local(2, TIMEOUT, |comm| {
            let mut s = sim(comm.clone());
            let before = s.fingerprint();
            let mut hook = Hook::default();
            let r = s.mutate(
                |tx| {
                    insert_at(tx, 1, true, false)?;
                    let empty_nodes = MeshEvent::NodesAdded {
                        nodes: vec![],
                        old_nodes: None,
                        origin: "extra".into(),
                        extension: None,
                    };
                    match (variant, comm.rank()) {
                        // Same number of events, different kind of the last.
                        (0, 0) => tx.record(MeshPart::Main, empty_nodes),
                        (0, _) => tx.record(
                            MeshPart::Main,
                            MeshEvent::MeshIsDistributed {
                                origin: "extra".into(),
                            },
                        ),
                        // A different number of events.
                        (_, 0) => tx.record(MeshPart::Main, empty_nodes),
                        _ => {}
                    }
                    Ok(())
                },
                Some(&mut hook),
                false,
            );
            (
                r.map_err(|e| e.to_string()),
                s.fingerprint() == before,
                hook.prepared,
            )
        });
        assert!(t0.elapsed() < PROMPT);
        for (r, same, prepared) in &out {
            let err = r.as_ref().unwrap_err();
            assert!(err.contains("disagree"), "{err}");
            assert!(*same, "complete state restored");
            assert_eq!(*prepared, 0, "no hook phase started");
        }
    }
}

/// Rank 0's topology closure fails, rank 1
/// adds a node and records NodesAdded. The failure is agreed before the
/// bounding-box all-reduce: no "all_reduce length mismatch", no timeout.
#[test]
fn topology_failure_on_one_rank_never_reaches_the_bbox_collective() {
    let t0 = Instant::now();
    let out = run_local(2, TIMEOUT, |comm| {
        let mut s = sim(comm.clone());
        let before = s.fingerprint();
        let mut hook = Hook::default();
        let r = s.mutate(
            |tx| {
                if tx.mesh().communicator().rank() == 0 {
                    return Err(Error::Mesh("injected rank-0 topology error".into()));
                }
                let n = tx.push_node(MeshPart::Main, &[5.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "rank 1".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            Some(&mut hook),
            false,
        );
        (r.map_err(|e| e.to_string()), s.fingerprint() == before)
    });
    assert!(t0.elapsed() < PROMPT);
    assert!(out[0].0.as_ref().unwrap_err().contains("injected rank-0"));
    let e1 = out[1].0.as_ref().unwrap_err();
    assert!(e1.contains("failed on another rank"), "{e1}");
    assert!(!e1.contains("length mismatch") && !e1.contains("timed out"));
    assert!(out[0].1 && out[1].1);
}

/// A facet event on a mesh without a facet mesh is rejected in the
/// topology stage (previously an early dispatch error that bypassed
/// poisoning) and rolled back.
#[test]
fn facet_event_without_facet_mesh_rolls_back() {
    let mut s = sim_with(Communicator::serial(), false, false);
    let before = s.fingerprint();
    let err = s
        .mutate(
            |tx| {
                insert_at(tx, 1, true, false)?;
                tx.record(
                    MeshPart::Facets,
                    MeshEvent::ElementsAdded {
                        elements: vec![],
                        constitutive_law: None,
                        origin: "bad".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
            false,
        )
        .unwrap_err();
    assert!(err.to_string().contains("facet"), "{err}");
    assert_eq!(s.fingerprint(), before);
    assert!(s.mesh.poisoned().unwrap().is_none());
}

/// Facet dispatch: the facet FE consumer fails on rank 1
/// during the facet event: every rank is poisoned, promptly.
#[test]
fn facet_consumer_failure_poisons_all_ranks() {
    let t0 = Instant::now();
    let out = run_local(2, TIMEOUT, |comm| {
        let mut s = sim(comm.clone());
        s.facet_fe.fail_on = (comm.rank() == 1).then_some("elements");
        let r = s.insert(comm.rank() == 0, false, false);
        (r.is_err(), s.mesh.poisoned().unwrap().is_some())
    });
    assert!(t0.elapsed() < PROMPT);
    assert!(out.iter().all(|o| *o == (true, true)));
}

#[test]
fn consumer_failure_poisons_and_reentry_fails_promptly() {
    let mut s = sim(Communicator::serial());
    s.dumper.reenter = Some(s.mesh.clone());
    s.python.fail = true;
    let err = s.insert(true, false, false).unwrap_err();
    // A failure after commit is the explicit fatal signal (agreed:
    // every rank is poisoned), wrapping the consumer's own error.
    match &err {
        Error::Fatal { kind, source } => {
            assert_eq!(*kind, akantu_core::FatalKind::Agreed);
            assert!(matches!(**source, Error::Numerical(_)), "{source:?}");
        }
        e => panic!("expected a fatal error, got {e:?}"),
    }
    assert_eq!(
        s.mesh.poison_kind().unwrap(),
        Some(akantu_core::FatalKind::Agreed)
    );
    let reentry = s.dumper.reentry_result.take().unwrap();
    assert!(reentry.unwrap_err().to_string().contains("re-entrant"));
    assert!(s.mesh.read().is_err(), "poisoned domain refuses reads");
    assert!(s.mesh.check_current(&s.tokens[0]).is_err());
    assert!(
        s.insert(true, false, false).is_err(),
        "and further mutations"
    );
}

/// Re-entry inside a **communicating** consumer. On rank 0 the
/// model's DOF level tries a re-entrant mutation (refused locally, before
/// any collective) and returns that error; on rank 1 the DOF manager runs
/// its phase-structured numbering. Both ranks fail at the same agreement
/// and are poisoned — well before the 20 s diagnostic timeout.
#[test]
fn reentry_inside_a_communicating_consumer_is_bounded() {
    let t0 = Instant::now();
    let out = run_local(2, TIMEOUT, |comm| {
        let mut s = sim(comm.clone());
        if comm.rank() == 0 {
            s.a.fault = DofFault::Reenter;
        }
        let r = s.insert(comm.rank() == 0, false, false);
        (
            r.map_err(|e| e.to_string()),
            s.mesh.poisoned().unwrap().is_some(),
            s.a.reentry.clone(),
        )
    });
    assert!(t0.elapsed() < PROMPT, "{:?}", t0.elapsed());
    assert!(out.iter().all(|(r, poisoned, _)| r.is_err() && *poisoned));
    assert!(out[0].2.as_deref().unwrap().contains("re-entrant"));
    assert!(out[0].0.as_ref().unwrap_err().contains("re-entrant"));
}

/// Rank-specific failure inside a communicating consumer: rank 1's
/// DOF manager rejects an invalid node list in its preflight; the failure
/// is agreed before any numbering collective; both ranks are poisoned
/// promptly and no DOF manager is half-renumbered.
#[test]
fn rank_specific_failure_inside_a_communicating_consumer_is_bounded() {
    let t0 = Instant::now();
    let out = run_local(2, TIMEOUT, |comm| {
        let mut s = sim(comm.clone());
        let dofs_before = s.dof_state();
        if comm.rank() == 1 {
            s.a.fault = DofFault::InvalidNodes;
        }
        let r = s.insert(true, false, false);
        (
            r.is_err(),
            s.mesh.poisoned().unwrap().is_some(),
            s.dof_state() == dofs_before,
        )
    });
    assert!(t0.elapsed() < PROMPT);
    assert!(out.iter().all(|o| *o == (true, true, true)));
}

#[test]
fn live_reader_blocks_mutation_without_deadlock() {
    let mut s = sim(Communicator::serial());
    let reader = s.mesh.clone();
    let guard = reader.read().unwrap(); // e.g. a dumper thread mid-write
    let err = s.insert(true, false, false).unwrap_err();
    assert!(err.to_string().contains("borrowed"), "{err}");
    drop(guard);
    s.insert(true, false, false).unwrap();
}

#[test]
fn two_ranks_zero_work_rank_participates_and_failures_are_collective() {
    for fail_on_rank1 in [false, true] {
        let t0 = Instant::now();
        let out = run_local(2, TIMEOUT, |comm| {
            let mut s = sim(comm.clone());
            s.python.fail = fail_on_rank1 && comm.rank() == 1;
            let mut hook = Hook::default();
            let r = s.mutate(
                |tx| insert_at(tx, 1, comm.rank() == 0, false),
                Some(&mut hook),
                false,
            );
            let log = s.log.lock().unwrap().clone();
            (
                r.is_ok(),
                log,
                s.mesh.poisoned().unwrap().is_some(),
                hook.committed,
            )
        });
        assert!(t0.elapsed() < PROMPT);
        if fail_on_rank1 {
            assert!(
                out.iter().all(|(ok, _, poisoned, _)| !ok && *poisoned),
                "both ranks poisoned"
            );
        } else {
            assert!(
                out.iter()
                    .all(|(ok, _, _, committed)| *ok && *committed == 1)
            );
            // Rank 1 inserted nothing but dispatches the same events (global
            // counts are non-zero), so collective consumers stay matched.
            assert_eq!(out[1].1.len(), out[0].1.len());
            assert!(out[1].1.iter().all(|l| l.ends_with(":3")));
        }
    }
}

// ------------------------------------------------------------------

/// Mesh case: rank 0 enters an empty mutation, rank 1
/// never participates (a transport-failure injection). Rank 0 times out in
/// the **preflight** agreement — an early exit that previously bypassed the
/// poison handling: its domain is now poisoned with the transport kind,
/// reads are refused and the error is the explicit fatal signal. Rank 1,
/// which observed nothing, is untouched.
#[test]
fn preflight_transport_failure_poisons_the_observing_rank() {
    use akantu_core::FatalKind;
    let out = run_local(2, Duration::from_millis(300), |comm| {
        let mesh = SharedMesh::new(bar(comm.clone(), true, false));
        let err = (comm.rank() == 0).then(|| {
            mesh.mutate(&mut [], |_| Ok(()), None)
                .unwrap_err()
                .fatal_kind()
        });
        (err, mesh.poison_kind().unwrap(), mesh.read().is_err())
    });
    assert_eq!(
        out[0],
        (
            Some(Some(FatalKind::Transport)),
            Some(FatalKind::Transport),
            true
        )
    );
    assert_eq!(out[1], (None, None, false));
}

/// A transport failure in the **topology** agreement (rank 1's
/// topology closure outlives the local-world timeout; the other early exit
/// that bypassed the poison handling): every rank that observed it is
/// poisoned (transport kind) and refuses reads.
#[test]
fn topology_transport_failure_poisons_every_observing_rank() {
    use akantu_core::FatalKind;
    let out = run_local(2, Duration::from_millis(300), |comm| {
        let mesh = SharedMesh::new(bar(comm.clone(), true, false));
        let slow = comm.rank() == 1;
        let err = mesh
            .mutate(
                &mut [],
                |tx| {
                    if slow {
                        std::thread::sleep(Duration::from_millis(700));
                    }
                    tx.push_node(MeshPart::Main, &[9.0]).map(|_| ())
                },
                None,
            )
            .unwrap_err();
        let kind = mesh.poison_kind().unwrap();
        (err.fatal_kind(), kind, mesh.read().is_err())
    });
    for (err, kind, refused) in &out {
        assert_eq!(*err, Some(FatalKind::Transport));
        assert_eq!(*kind, Some(FatalKind::Transport));
        assert!(*refused);
    }
}
