// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests:
// facet consumers could miss a node addition before receiving a later
// removal map, and an addition event could refer to nodes a consumer had not
// seen. The node event stream a consumer of either mesh part receives must
// describe every change of the shared node set, in order.
// Ported behaviour: src/mesh/mesh_inline_impl.hh (sendEvent),
// src/mesh/mesh.cc (initMeshFacets), src/model/solid_mechanics/
// solid_mechanics_model_cohesive/solid_mechanics_model_cohesive.cc:49-94
// (node events of an insertion), src/model/model_solver/dof_manager.cc
// (onNodesAdded: the listed nodes are the new ones).

use akantu_comm::phase::Phases;
use akantu_comm::run_local;
use akantu_core::enums::EventPriority;
use akantu_core::{ElementType as T, GhostType as G, Idx, Result};
use akantu_mesh::utils::init_mesh_facets;
use akantu_mesh::{
    DistributedHook, Mesh, MeshAccessor, MeshEvent, MeshEventHandler, MeshPart, MeshTransaction,
    SharedMesh,
};

const NG: G = G::NotGhost;

/// One triangle `[0,1,2]`, one boundary segment `[0,1]` and an extra node 3.
fn triangle_mesh() -> Mesh {
    let mut m = Mesh::new(2, "m").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0., 0.], [1., 0.], [0., 1.], [2., 2.]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(T::Triangle3, NG).push_tuple(&[0, 1, 2]);
        a.connectivity_mut(T::Segment2, NG).push_tuple(&[0, 1]);
    }
    m.compute_bounding_box().unwrap();
    m
}

/// Consumer tracking nodal state strictly from the event stream (a nodal
/// field: one value per node). `NodesAdded` must list the next nodes in
/// order and every `old_nodes` entry must be a node it already holds;
/// `NodesRemoved` must map exactly the nodes it holds. Every mismatch is
/// recorded.
#[derive(Default)]
struct Shadow {
    /// Value of every node (`value[n]` = original node id or, for a doubled
    /// node, the value copied from its old node).
    value: Vec<Idx>,
    errors: Vec<String>,
}

impl Shadow {
    fn with_nodes(n: Idx) -> Self {
        Self {
            value: (0..n).collect(),
            errors: Vec::new(),
        }
    }
}

impl MeshEventHandler for Shadow {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority(60)
    }
    fn on_mesh_event(&mut self, mesh: &Mesh, event: &MeshEvent) -> Result<()> {
        let _ = mesh;
        match event {
            MeshEvent::NodesAdded {
                nodes, old_nodes, ..
            } => {
                for (i, &n) in nodes.iter().enumerate() {
                    if usize::try_from(n).ok() != Some(self.value.len()) {
                        self.errors
                            .push(format!("added {n} at {}", self.value.len()));
                    }
                    let v = match old_nodes {
                        Some(old) => {
                            match usize::try_from(old[i]).ok().and_then(|o| self.value.get(o)) {
                                Some(&v) => v,
                                None => {
                                    self.errors.push(format!(
                                        "old node {} of {n} unknown ({} nodes)",
                                        old[i],
                                        self.value.len()
                                    ));
                                    -1
                                }
                            }
                        }
                        None => n,
                    };
                    self.value.push(v);
                }
            }
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                if new_numbering.len() != self.value.len() {
                    self.errors.push(format!(
                        "numbering of length {} for {} nodes",
                        new_numbering.len(),
                        self.value.len()
                    ));
                    return Ok(());
                }
                let kept = new_numbering.iter().filter(|&&x| x >= 0).count();
                let mut next = vec![0; kept];
                for (old, &new) in new_numbering.iter().enumerate() {
                    if new >= 0 {
                        next[usize::try_from(new).unwrap()] = self.value[old];
                    }
                }
                self.value = next;
            }
            _ => {}
        }
        Ok(())
    }
}

fn nodes_added(nodes: Vec<Idx>, old_nodes: Option<Vec<Idx>>) -> MeshEvent {
    MeshEvent::NodesAdded {
        nodes,
        old_nodes,
        origin: "v36c".into(),
        extension: None,
    }
}

/// A shared mesh with a facet mesh and one `Main` and one `Facets` shadow
/// consumer (4 nodes each).
struct Fixture {
    d: SharedMesh,
    tm: akantu_mesh::ConsumerToken,
    tf: akantu_mesh::ConsumerToken,
    sm: Shadow,
    sf: Shadow,
}

impl Fixture {
    fn new(with_facets: bool) -> Self {
        let mut m = triangle_mesh();
        if with_facets {
            let _ = init_mesh_facets(&mut m).unwrap();
        }
        let d = SharedMesh::new(m);
        let tm = d
            .register_consumer("main", EventPriority(60), MeshPart::Main)
            .unwrap();
        let tf = d
            .register_consumer("facets", EventPriority(60), MeshPart::Facets)
            .unwrap();
        Self {
            d,
            tm,
            tf,
            sm: Shadow::with_nodes(4),
            sf: Shadow::with_nodes(4),
        }
    }

    fn mutate(
        &mut self,
        change: impl FnOnce(&mut MeshTransaction<'_>) -> Result<()>,
    ) -> Result<akantu_mesh::CommittedMutation> {
        self.d.mutate(
            &mut [(&self.tm, &mut self.sm), (&self.tf, &mut self.sf)],
            change,
            None,
        )
    }

    /// The mutation is refused with an error containing `reason`: nothing
    /// changed, nothing dispatched, the domain stays usable.
    fn assert_refused(
        &mut self,
        reason: &str,
        change: impl FnOnce(&mut MeshTransaction<'_>) -> Result<()>,
    ) {
        let before = self.d.read().unwrap().state_fingerprint();
        let r = self.mutate(change);
        match &r {
            Err(e) => assert!(e.to_string().contains(reason), "{e}"),
            Ok(_) => panic!("the mutation must be refused: {r:?}"),
        }
        assert_eq!(self.d.read().unwrap().state_fingerprint(), before);
        assert!(self.d.poisoned().unwrap().is_none());
        assert_eq!(self.sm.value, (0..4).collect::<Vec<_>>());
        assert_eq!(self.sf.value, (0..4).collect::<Vec<_>>());
        assert!(self.sm.errors.is_empty() && self.sf.errors.is_empty());
        self.mutate(|_| Ok(())).unwrap();
    }

    fn assert_consistent(&self) {
        let nb_nodes = self.d.read().unwrap().nb_nodes();
        assert!(self.sm.errors.is_empty(), "main: {:?}", self.sm.errors);
        assert!(self.sf.errors.is_empty(), "facets: {:?}", self.sf.errors);
        assert_eq!(self.sm.value.len(), nb_nodes);
        assert_eq!(self.sf.value, self.sm.value);
    }
}

/// A node appended without a `NodesAdded` event, then a node removal in the
/// same transaction: the consumers of both parts would receive a numbering
/// of length 5 after being told about 4 nodes (before the fix the mutation
/// committed and both shadows failed).
#[test]
fn v36c_removal_after_an_unannounced_append_is_refused() {
    let mut f = Fixture::new(true);
    f.assert_refused(
        "NodesRemoved maps 5 nodes, but the consumers know 4",
        |tx| {
            tx.push_node(MeshPart::Main, &[9., 9.])?;
            tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1, 3])
        },
    );
}

/// A node appended and never announced: the consumers would end with 4
/// values for a mesh of 5 nodes.
#[test]
fn v36c_unannounced_append_is_refused() {
    let mut f = Fixture::new(true);
    f.assert_refused("appended without a NodesAdded event", |tx| {
        tx.push_node(MeshPart::Main, &[9., 9.]).map(|_| ())
    });
    // also without a facet mesh (the rule is the same for Main consumers)
    let mut f = Fixture::new(false);
    f.assert_refused("appended without a NodesAdded event", |tx| {
        tx.push_node(MeshPart::Main, &[9., 9.]).map(|_| ())
    });
}

/// `NodesAdded` refers to a node the consumers have not seen: node 5 is
/// announced while node 4 never was.
#[test]
fn v36c_addition_skipping_an_unannounced_node_is_refused() {
    let mut f = Fixture::new(true);
    f.assert_refused("lists node 5 at position 0", |tx| {
        tx.push_node(MeshPart::Main, &[9., 9.])?;
        let n = tx.push_node(MeshPart::Main, &[8., 8.])?;
        tx.record(MeshPart::Main, nodes_added(vec![n], None));
        Ok(())
    });
}

/// `NodesAdded` announcing a node twice, or announcing an existing node.
#[test]
fn v36c_addition_of_an_already_announced_node_is_refused() {
    let mut f = Fixture::new(true);
    f.assert_refused(
        "lists node 4 at position 0, but the consumers know 5",
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[9., 9.])?;
            tx.record(MeshPart::Main, nodes_added(vec![n], None));
            tx.record(MeshPart::Main, nodes_added(vec![n], None));
            Ok(())
        },
    );
    f.assert_refused("lists node 2 at position 0", |tx| {
        tx.push_node(MeshPart::Main, &[9., 9.])?;
        tx.record(MeshPart::Main, nodes_added(vec![2], None));
        Ok(())
    });
}

/// `old_nodes` of a doubled node must be a node the consumers already hold
/// (an original node, or one added earlier in the same event): here the old
/// node of node 4 is node 5, announced after it.
#[test]
fn v36c_old_node_announced_later_is_refused() {
    let mut f = Fixture::new(true);
    f.assert_refused("old node 5 of new node 4", |tx| {
        let a = tx.duplicate_node(MeshPart::Main, 0)?;
        let b = tx.duplicate_node(MeshPart::Main, 1)?;
        tx.record(MeshPart::Main, nodes_added(vec![a, b], Some(vec![b, 1])));
        Ok(())
    });
}

/// A `NodesRemoved` recorded directly (the mesh was not renumbered, and the
/// facet consumers would never receive the renumbering of the shared node
/// set): only the transaction's removal path records node removals.
#[test]
fn v36c_directly_recorded_node_removal_is_refused() {
    let mut f = Fixture::new(true);
    f.assert_refused("recorded by the transaction's removal path", |tx| {
        tx.record(
            MeshPart::Main,
            MeshEvent::NodesRemoved {
                nodes: Vec::new(),
                new_numbering: vec![1, 0, 2, 3],
                origin: "direct".into(),
            },
        );
        Ok(())
    });
}

/// A facet consumer registered before the facet mesh exists cannot follow
/// a facet mesh created inside a mutation: here the node appended before
/// the creation is announced on `Main` only (no facet mesh yet, no twin)
/// and the later removal reaches the facet consumer with a numbering of
/// length 5 after 4 nodes. Creating the facet mesh while `Facets`
/// consumers are registered is refused (upstream creates the facet mesh
/// before the facet FE engine, `SolidMechanicsModelCohesive` constructor).
#[test]
fn v36c_facet_mesh_created_under_facet_consumers_is_refused() {
    let mut f = Fixture::new(false);
    f.assert_refused("Facets consumers are registered", |tx| {
        let n = tx.push_node(MeshPart::Main, &[9., 9.])?;
        tx.record(MeshPart::Main, nodes_added(vec![n], None));
        tx.init_mesh_facets()?;
        tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1, 3])
    });
}

/// Without `Facets` consumers, creating the facet mesh inside a mutation
/// after a node addition and before a removal stays valid.
#[test]
fn v36c_facet_mesh_created_without_facet_consumers_commits() {
    let d = SharedMesh::new(triangle_mesh());
    let tm = d
        .register_consumer("main", EventPriority(60), MeshPart::Main)
        .unwrap();
    let mut sm = Shadow::with_nodes(4);
    let c = d
        .mutate(
            &mut [(&tm, &mut sm)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[9., 9.])?;
                tx.record(MeshPart::Main, nodes_added(vec![n], None));
                tx.init_mesh_facets()?;
                tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1, 3])
            },
            None,
        )
        .unwrap();
    assert!(sm.errors.is_empty(), "{:?}", sm.errors);
    assert_eq!(sm.value, vec![0, 1, 2, 4]);
    let parts: Vec<MeshPart> = c.events.iter().map(|(p, _)| *p).collect();
    assert_eq!(
        parts,
        vec![MeshPart::Main, MeshPart::Main, MeshPart::Facets]
    );
    assert_eq!(d.read().unwrap().mesh_facets().unwrap().nb_nodes(), 4);
}

/// The valid streams the rule must keep: appends (doubling chains included:
/// node 5 doubles node 4 of the same event), a removal, appends again — in
/// one transaction and across transactions; both parts receive the same
/// node stream.
#[test]
fn v36c_valid_node_streams_commit_on_both_parts() {
    let mut f = Fixture::new(true);
    f.mutate(|tx| {
        let a = tx.duplicate_node(MeshPart::Main, 0)?;
        let b = tx.duplicate_node(MeshPart::Main, a)?;
        tx.record(MeshPart::Main, nodes_added(vec![a, b], Some(vec![0, a])));
        tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1, 3, 4])?;
        let c = tx.push_node(MeshPart::Main, &[9., 9.])?;
        tx.record(MeshPart::Main, nodes_added(vec![c], None));
        Ok(())
    })
    .unwrap();
    f.assert_consistent();
    assert_eq!(f.sm.value, vec![0, 1, 2, 0, 0, 5]);
    f.mutate(|tx| tx.purify(MeshPart::Main)).unwrap();
    f.assert_consistent();
    // empty additions are fine (zero-work ranks record them too)
    f.mutate(|tx| {
        tx.record(MeshPart::Main, nodes_added(Vec::new(), None));
        Ok(())
    })
    .unwrap();
    f.assert_consistent();
}

/// Two ranks, distributed mesh: a distributed hook that appends a node
/// during `apply` without announcing it (rank 1 only) — refused on every
/// rank (agreed), nothing changed; an announced one is dispatched to both
/// parts after the change's events.
#[test]
fn v36c_hook_appends_follow_the_same_rule() {
    struct Hook {
        announce: bool,
    }
    impl DistributedHook for Hook {
        fn prepare(&mut self, _: &Mesh, _: &[(MeshPart, MeshEvent)]) -> Result<()> {
            Ok(())
        }
        fn apply(
            &mut self,
            tx: &mut MeshTransaction<'_>,
            _: &[(MeshPart, MeshEvent)],
            _: &mut Phases,
        ) -> Result<()> {
            let rank = tx.mesh().communicator().rank();
            if self.announce {
                let n = tx.push_node(MeshPart::Main, &[7., 7.])?;
                tx.record(MeshPart::Main, nodes_added(vec![n], None));
            } else {
                // identical schedules on both ranks; rank 1 appends a node
                // its event does not list
                if rank == 1 {
                    tx.push_node(MeshPart::Main, &[7., 7.])?;
                }
                tx.record(MeshPart::Main, nodes_added(Vec::new(), None));
            }
            Ok(())
        }
    }
    let out = run_local(2, std::time::Duration::from_secs(20), |comm| {
        let mut m = Mesh::with_communicator(2, "m", comm).unwrap();
        let mut facets = Mesh::with_communicator(2, "m:facets", m.communicator().clone()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut facets);
            for x in [[0., 0.], [1., 0.], [0., 1.]] {
                a.push_node(&x).unwrap();
            }
            a.connectivity_mut(T::Segment2, NG).push_tuple(&[0, 1]);
        }
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [[0., 0.], [1., 0.], [0., 1.]] {
                a.push_node(&x).unwrap();
            }
            a.connectivity_mut(T::Triangle3, NG).push_tuple(&[0, 1, 2]);
            a.set_mesh_facets(facets);
            a.set_distributed();
        }
        let d = SharedMesh::new(m);
        let tm = d
            .register_consumer("main", EventPriority(60), MeshPart::Main)
            .unwrap();
        let tf = d
            .register_consumer("facets", EventPriority(60), MeshPart::Facets)
            .unwrap();
        let mut sm = Shadow::with_nodes(3);
        let mut sf = Shadow::with_nodes(3);
        let before = d.read().unwrap().state_fingerprint();
        let refused = d
            .mutate(
                &mut [(&tm, &mut sm), (&tf, &mut sf)],
                |tx| {
                    tx.record(MeshPart::Main, nodes_added(Vec::new(), None));
                    Ok(())
                },
                Some(&mut Hook { announce: false }),
            )
            .err()
            .map(|e| e.to_string());
        let unchanged = d.read().unwrap().state_fingerprint() == before;
        let not_poisoned = d.poisoned().unwrap().is_none();
        let c = d
            .mutate(
                &mut [(&tm, &mut sm), (&tf, &mut sf)],
                |tx| {
                    let n = tx.push_node(MeshPart::Main, &[3., 3.])?;
                    tx.record(MeshPart::Main, nodes_added(vec![n], None));
                    Ok(())
                },
                Some(&mut Hook { announce: true }),
            )
            .unwrap();
        let parts: Vec<MeshPart> = c.events.iter().map(|(p, _)| *p).collect();
        (
            refused,
            unchanged,
            not_poisoned,
            parts,
            sm.value.len(),
            sf.value.len(),
            sm.errors.len() + sf.errors.len(),
        )
    });
    for (rank, (refused, unchanged, not_poisoned, parts, nm, nf, errors)) in
        out.into_iter().enumerate()
    {
        let e = refused.expect("refused on every rank");
        if rank == 1 {
            assert!(e.contains("appended without a NodesAdded event"), "{e}");
        } else {
            assert!(e.contains("another rank"), "{e}");
        }
        assert!(unchanged && not_poisoned);
        assert_eq!(
            parts,
            vec![
                MeshPart::Main,
                MeshPart::Facets,
                MeshPart::Main,
                MeshPart::Facets
            ]
        );
        assert_eq!((nm, nf, errors), (5, 5, 0));
    }
}
