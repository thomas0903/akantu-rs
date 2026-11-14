// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the mesh
// events and handlers (MESH-015; upstream src/mesh/mesh_events.hh,
// mesh.cc sendEvent, aka_event_handler_manager.hh) and of the global-data
// update hook (MESH-033; mesh_global_data_updater.hh, Mesh::updateGlobalData).
//
// Upstream semantics checked: every event carries its list, its origin and
// (removals) the old → new numbering; handlers are notified in ascending
// priority order (mesh 5 < FE engine 9 < synchronizer 10 < DOF manager 20 <
// model 94); events are delivered in the order they were recorded (upstream
// `sendEvent` dispatches each event when it is called), each to every
// handler before the next event; in serial the global event
// counts are the local list sizes (upstream `updateGlobalData` without an
// updater).

use std::sync::{Arc, Mutex};

use akantu_core::enums::EventPriority;
use akantu_core::{Element, ElementType, GhostType, Idx, Result};
use akantu_mesh::global_data_updater::{MeshGlobalDataUpdater, update_global_data};
use akantu_mesh::removal::{erase_elements_setup, remove_nodes};
use akantu_mesh::utils::purify_mesh;
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, SharedMesh};

const S2: ElementType = ElementType::Segment2;
const NG: GhostType = GhostType::NotGhost;

/// Bar of `n` segments, nodes at 0..=n.
fn bar(n: usize) -> Mesh {
    let mut m = Mesh::new(1, "bar").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for i in 0..=n {
            #[allow(clippy::cast_precision_loss)] // small
            a.push_node(&[i as f64]).unwrap();
        }
        for i in 0..n {
            let i = Idx::try_from(i).unwrap();
            a.connectivity_mut(S2, NG).push_tuple(&[i, i + 1]);
        }
        a.make_ready().unwrap();
    }
    m
}

fn kind(e: &MeshEvent) -> &'static str {
    match e {
        MeshEvent::NodesAdded { .. } => "NodesAdded",
        MeshEvent::NodesRemoved { .. } => "NodesRemoved",
        MeshEvent::ElementsAdded { .. } => "ElementsAdded",
        MeshEvent::ElementsRemoved { .. } => "ElementsRemoved",
        MeshEvent::ElementsChanged { .. } => "ElementsChanged",
        MeshEvent::MeshIsDistributed { .. } => "MeshIsDistributed",
        _ => "other",
    }
}

/// Handler recording `(name, event kind, origin)`.
struct Recorder {
    name: &'static str,
    priority: EventPriority,
    log: Arc<Mutex<Vec<(String, String, String)>>>,
}

impl MeshEventHandler for Recorder {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        self.priority
    }
    fn on_mesh_event(&mut self, _: &Mesh, e: &MeshEvent) -> Result<()> {
        let origin = match e {
            MeshEvent::NodesAdded { origin, .. }
            | MeshEvent::NodesRemoved { origin, .. }
            | MeshEvent::ElementsAdded { origin, .. }
            | MeshEvent::ElementsRemoved { origin, .. }
            | MeshEvent::ElementsChanged { origin, .. }
            | MeshEvent::MeshIsDistributed { origin } => origin.clone(),
            _ => String::new(),
        };
        self.log
            .lock()
            .unwrap()
            .push((self.name.into(), kind(e).into(), origin));
        Ok(())
    }
}

/// MESH-015: priority order across handlers (registered model first, FE
/// engine second) and recorded event order (nodes, then elements).
#[test]
fn handlers_follow_priorities_and_events_their_recorded_order() {
    let d = SharedMesh::new(bar(2));
    let log = Arc::new(Mutex::new(Vec::new()));
    let t_model = d
        .register_consumer("model", EventPriority::MODEL, MeshPart::Main)
        .unwrap();
    let t_fe = d
        .register_consumer("fe", EventPriority::FE_ENGINE, MeshPart::Main)
        .unwrap();
    let mut model = Recorder {
        name: "model",
        priority: EventPriority::MODEL,
        log: log.clone(),
    };
    let mut fe = Recorder {
        name: "fe",
        priority: EventPriority::FE_ENGINE,
        log: log.clone(),
    };
    let committed = d
        .mutate(
            &mut [(&t_model, &mut model), (&t_fe, &mut fe)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[3.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test:nodes".into(),
                        extension: None,
                    },
                );
                let e = tx.push_element(MeshPart::Main, S2, NG, &[2, n])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: None,
                        origin: "test:elements".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    let got = log.lock().unwrap().clone();
    let want: Vec<(String, String, String)> = [
        ("fe", "NodesAdded", "test:nodes"),
        ("model", "NodesAdded", "test:nodes"),
        ("fe", "ElementsAdded", "test:elements"),
        ("model", "ElementsAdded", "test:elements"),
    ]
    .iter()
    .map(|(a, b, c)| ((*a).into(), (*b).into(), (*c).into()))
    .collect();
    assert_eq!(got, want);
    // the committed events, with their lists
    let kinds: Vec<&str> = committed.events.iter().map(|(_, e)| kind(e)).collect();
    assert_eq!(kinds, ["NodesAdded", "ElementsAdded"]);
    match &committed.events[0].1 {
        MeshEvent::NodesAdded { nodes, .. } => assert_eq!(nodes, &vec![3]),
        e => panic!("{e:?}"),
    }
    match &committed.events[1].1 {
        MeshEvent::ElementsAdded { elements, .. } => {
            assert_eq!(elements, &vec![Element::new(S2, 2, NG)]);
        }
        e => panic!("{e:?}"),
    }
    let m = d.read().unwrap();
    assert_eq!((m.nb_nodes(), m.nb_element(S2, NG)), (4, 3));
    assert_eq!(m.upper_bounds(), &[3.0], "box updated before dispatch");
}

/// MESH-015: removal events carry upstream's numberings (`RemovedElements
/// Event::new_numbering`: holes filled from the end; `RemovedNodesEvent`:
/// old → new, −1 removed) and their emitters.
#[test]
fn removal_events_carry_upstream_numberings() {
    let mut m = bar(4);
    let ev = erase_elements_setup(&mut m, &[Element::new(S2, 1, NG)]).unwrap();
    match ev {
        MeshEvent::ElementsRemoved {
            elements,
            new_numbering,
            ..
        } => {
            assert_eq!(elements, vec![Element::new(S2, 1, NG)]);
            // upstream eraseElements: removed = -1, holes filled by the last
            assert_eq!(
                new_numbering.get(S2, NG).unwrap().as_slice(),
                &[0, -1, 2, 1]
            );
        }
        e => panic!("{e:?}"),
    }
    // every node is still used (0-1, 3-4, 2-3): purify renumbers in order
    // of first appearance (0, 1, 3, 4, 2) and removes nothing
    let evs = purify_mesh(&mut m).unwrap();
    match &evs.events()[0] {
        MeshEvent::NodesRemoved {
            nodes,
            new_numbering,
            origin,
        } => {
            assert!(nodes.is_empty());
            assert_eq!(new_numbering, &vec![0, 1, 4, 2, 3]);
            assert_eq!(origin, "MeshUtils::purifyMesh");
        }
        e => panic!("{e:?}"),
    }
    let mut m = bar(2);
    let evs = remove_nodes(&mut m, &[1, 0, 2], "test:permutation").unwrap();
    match &evs.events()[0] {
        MeshEvent::NodesRemoved {
            nodes,
            new_numbering,
            origin,
        } => {
            assert!(nodes.is_empty(), "a permutation removes nothing");
            assert_eq!(new_numbering, &vec![1, 0, 2]);
            assert_eq!(origin, "test:permutation");
        }
        e => panic!("{e:?}"),
    }
    assert_eq!(m.connectivity(S2, NG).unwrap().as_slice(), &[1, 0, 0, 2]);
    assert_eq!(m.nodes().as_slice(), &[1.0, 0.0, 2.0]);
}

/// MESH-033: `Mesh::updateGlobalData` (no updater: local list sizes;
/// upstream base class: (0, 0)) and, in a serial mutation without a hook,
/// every recorded event dispatched with its local list.
#[test]
fn global_data_update_defaults() {
    struct Base;
    impl MeshGlobalDataUpdater for Base {}
    let nodes: Vec<Idx> = vec![7, 8];
    let elements = vec![Element::new(S2, 0, NG)];
    assert_eq!(update_global_data(None, &nodes, &elements).unwrap(), (2, 1));
    assert_eq!(
        update_global_data(Some(&mut Base), &nodes, &elements).unwrap(),
        (0, 0)
    );
    // serial staged mutation, no distributed hook: the events go out as
    // recorded (global count = local count); a node event with an empty
    // list is dropped (global count 0, as upstream's inserter sends no
    // event when updateGlobalData returns 0)
    let d = SharedMesh::new(bar(1));
    let committed = d
        .mutate(
            &mut [],
            |tx| {
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![],
                        old_nodes: None,
                        origin: "empty".into(),
                        extension: None,
                    },
                );
                let n = tx.push_node(MeshPart::Main, &[2.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "one".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    let origins: Vec<String> = committed
        .events
        .iter()
        .map(|(_, e)| match e {
            MeshEvent::NodesAdded { origin, .. } => origin.clone(),
            other => format!("{other:?}"),
        })
        .collect();
    assert_eq!(origins, ["one"]);
}
