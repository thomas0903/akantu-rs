// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the mesh layer: facet-side node-addition events, set-up
// paths breaking the shared node numbering, commit-time adjacency checks and the
// observation on events recorded by a distributed hook during `apply`.
// Ported behaviour: src/mesh/mesh_inline_impl.hh (sendEvent), src/mesh/
// mesh.cc (initMeshFacets), src/mesh_utils/mesh_utils.cc (purifyMesh).

use akantu_comm::phase::Phases;
use akantu_comm::run_local;
use akantu_core::enums::EventPriority;
use akantu_core::{Element, ElementType as T, ElementTypeMapArray, GhostType as G, Idx, Result};
use akantu_mesh::removal::remove_nodes;
use akantu_mesh::utils::{
    SUBELEMENT_TO_ELEMENT, check_facet_mesh_consistency, init_mesh_facets, purify_mesh,
};
use akantu_mesh::{
    DistributedHook, Mesh, MeshAccessor, MeshDataValues, MeshEvent, MeshEventHandler, MeshPart,
    MeshTransaction, SharedMesh,
};

const NG: G = G::NotGhost;

fn mesh_2d(nodes: &[[f64; 2]], blocks: &[(T, &[&[Idx]])]) -> Mesh {
    let mut m = Mesh::new(2, "m").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in nodes {
            a.push_node(x).unwrap();
        }
        for (ty, conns) in blocks {
            for c in *conns {
                a.connectivity_mut(*ty, NG).push_tuple(c);
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// One triangle `[0,1,2]`, one boundary segment `[0,1]` and an extra node 3
/// (the meshes of these tests).
fn triangle_mesh() -> Mesh {
    mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.], [2., 2.]],
        &[(T::Triangle3, &[&[0, 1, 2]]), (T::Segment2, &[&[0, 1]])],
    )
}

/// Consumer tracking nodal state strictly from the event stream: appends on
/// `NodesAdded`, permutes on `NodesRemoved`, and records an error when an
/// event does not fit its own node count.
#[derive(Default)]
struct Shadow {
    nodes: Vec<Idx>,
    errors: Vec<String>,
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
        match event {
            MeshEvent::NodesAdded { nodes, .. } => {
                for &n in nodes {
                    if usize::try_from(n).unwrap() != self.nodes.len() {
                        self.errors
                            .push(format!("added {n} at {}", self.nodes.len()));
                    }
                    self.nodes.push(n);
                }
            }
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                if new_numbering.len() != self.nodes.len() {
                    self.errors.push(format!(
                        "numbering of length {} for {} nodes",
                        new_numbering.len(),
                        self.nodes.len()
                    ));
                    return Ok(());
                }
                let kept = new_numbering.iter().filter(|&&x| x >= 0).count();
                let mut next = vec![0; kept];
                for (old, &new) in new_numbering.iter().enumerate() {
                    if new >= 0 {
                        next[usize::try_from(new).unwrap()] = Idx::try_from(old).unwrap();
                    }
                }
                self.nodes = (0..Idx::try_from(kept).unwrap()).collect();
            }
            _ => {}
        }
        let _ = mesh;
        Ok(())
    }
}

fn nodes_added(n: Idx) -> MeshEvent {
    MeshEvent::NodesAdded {
        nodes: vec![n],
        old_nodes: None,
        origin: "v36".into(),
        extension: None,
    }
}

/// A node appended (event recorded on `Main` only,
/// as documented) then a node removal in a second transaction. Before the
/// fix the facet consumer received a numbering of length 5 after being told
/// about 4 nodes.
#[test]
fn mu2_01_facet_consumer_follows_appends_then_removals_across_transactions() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let tm = d
        .register_consumer("main", EventPriority(60), MeshPart::Main)
        .unwrap();
    let tf = d
        .register_consumer("facets", EventPriority(60), MeshPart::Facets)
        .unwrap();
    let mut sm = Shadow {
        nodes: (0..4).collect(),
        ..Shadow::default()
    };
    let mut sf = Shadow {
        nodes: (0..4).collect(),
        ..Shadow::default()
    };
    let c = d
        .mutate(
            &mut [(&tm, &mut sm), (&tf, &mut sf)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[9., 9.])?;
                tx.record(MeshPart::Main, nodes_added(n));
                Ok(())
            },
            None,
        )
        .unwrap();
    // the transaction recorded the facet twin itself
    let parts: Vec<MeshPart> = c.events.iter().map(|(p, _)| *p).collect();
    assert_eq!(parts, vec![MeshPart::Main, MeshPart::Facets]);
    d.mutate(
        &mut [(&tm, &mut sm), (&tf, &mut sf)],
        |tx| tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1, 3]),
        None,
    )
    .unwrap();
    assert!(sm.errors.is_empty(), "{:?}", sm.errors);
    assert!(sf.errors.is_empty(), "{:?}", sf.errors);
    assert_eq!(sm.nodes.len(), 4);
    assert_eq!(sf.nodes.len(), 4);
}

/// Facet-side node additions within one transaction: append then remove.
#[test]
fn mu2_01_facet_consumer_follows_append_and_removal_in_one_transaction() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let tm = d
        .register_consumer("main", EventPriority(60), MeshPart::Main)
        .unwrap();
    let tf = d
        .register_consumer("facets", EventPriority(60), MeshPart::Facets)
        .unwrap();
    let mut sm = Shadow {
        nodes: (0..4).collect(),
        ..Shadow::default()
    };
    let mut sf = Shadow {
        nodes: (0..4).collect(),
        ..Shadow::default()
    };
    let c = d
        .mutate(
            &mut [(&tm, &mut sm), (&tf, &mut sf)],
            |tx| {
                let n = tx.push_node(MeshPart::Main, &[9., 9.])?;
                tx.record(MeshPart::Main, nodes_added(n));
                let o = tx.duplicate_node(MeshPart::Main, 0)?;
                tx.record(MeshPart::Main, nodes_added(o));
                tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1, 3, 4])
            },
            None,
        )
        .unwrap();
    let parts: Vec<MeshPart> = c.events.iter().map(|(p, _)| *p).collect();
    assert_eq!(
        parts,
        vec![
            MeshPart::Main,
            MeshPart::Facets,
            MeshPart::Main,
            MeshPart::Facets,
            MeshPart::Main,
            MeshPart::Facets
        ]
    );
    assert!(sm.errors.is_empty(), "{:?}", sm.errors);
    assert!(sf.errors.is_empty(), "{:?}", sf.errors);
    assert_eq!(sf.nodes.len(), 5);
}

/// A node event recorded on `Facets` by the caller would duplicate
/// the automatic twin; it is an invalid event (the mutation rolls back).
#[test]
fn mu2_01_caller_recorded_facet_node_event_is_rejected() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let before = d.read().unwrap().state_fingerprint();
    let r = d.mutate(
        &mut [],
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[9., 9.])?;
            tx.record(MeshPart::Facets, nodes_added(n));
            Ok(())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(d.read().unwrap().state_fingerprint(), before);
}

/// `remove_nodes` / `purify_mesh` on the facet mesh
/// of a set-up mesh are rejected (they returned `Ok` and broke the shared
/// numbering).
#[test]
fn mu2_03_setup_node_renumbering_of_a_facet_mesh_is_rejected() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    let before = m.state_fingerprint();
    assert!(remove_nodes(m.mesh_facets_mut().unwrap(), &[1, 2, 0, 3], "p").is_err());
    assert!(purify_mesh(m.mesh_facets_mut().unwrap()).is_err());
    assert_eq!(m.state_fingerprint(), before);
    check_facet_mesh_consistency(&m).unwrap();
    // through the domain's set-up path
    let d = SharedMesh::new(m);
    assert!(
        d.setup(|m| remove_nodes(m.mesh_facets_mut().unwrap(), &[1, 2, 0, 3], "p"))
            .is_err()
    );
    // the domain stays usable
    d.mutate(&mut [], |_| Ok(()), None).unwrap();
}

/// A set-up operation that breaks the shared numbering by other
/// means (raw accessor) is rejected before the revision step.
#[test]
fn mu2_03_setup_checks_the_facet_mesh_consistency() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let rev = d.read().unwrap().revision();
    let r = d.setup(|m| {
        let f = m.mesh_facets_mut().unwrap();
        MeshAccessor::new(f).push_node(&[5., 5.]).unwrap();
        Ok(akantu_mesh::MeshEvents(Vec::new()))
    });
    assert!(r.is_err());
    // no revision step for the rejected operation (the accessor bumped the
    // facet mesh's own counter only)
    assert_eq!(d.read().unwrap().revision(), rev);
}

/// A zero-component Triangle3 `subelement_to_element`
/// block committed; now the component count must equal the number of
/// facets per element.
#[test]
fn mu2_05_subelement_to_element_component_count_is_checked() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    check_facet_mesh_consistency(&m).unwrap();
    {
        let f = m.mesh_facets_mut().unwrap();
        let mut s2e = ElementTypeMapArray::<Element>::new("s2e");
        s2e.alloc(1, 0, T::Triangle3, NG, Element::NULL).unwrap();
        f.data_mut()
            .insert_elemental(SUBELEMENT_TO_ELEMENT, MeshDataValues::Element(s2e));
    }
    let e = check_facet_mesh_consistency(&m).unwrap_err();
    assert!(e.to_string().contains("components"), "{e}");
}

/// Debug builds: `subelement_to_element` and
/// `element_to_subelement` must be reciprocal for regular elements.
#[cfg(debug_assertions)]
#[test]
fn mu2_05_adjacency_reciprocity_is_checked_in_debug_builds() {
    let mut m = triangle_mesh();
    let _ = init_mesh_facets(&mut m).unwrap();
    {
        let f = m.mesh_facets_mut().unwrap();
        let s2e = f.data_mut().element_map_mut(SUBELEMENT_TO_ELEMENT).unwrap();
        let row = s2e.get_mut(T::Triangle3, NG).unwrap();
        // swap the triangle's first two facets for an unrelated one: every
        // facet still exists, but facet 0's e2s row does not list the
        // triangle twice and the swapped-in facet list is wrong
        let s = row.as_mut_slice();
        let f0 = s[0];
        s[0] = Element::new(T::Segment2, 0, NG);
        s[1] = f0;
        s[2] = Element::new(T::Segment2, 0, NG);
    }
    // facet 0 is listed (its e2s contains the triangle): build an e2s
    // row that does not contain the triangle for segment 2
    {
        let f = m.mesh_facets_mut().unwrap();
        let e2s = f
            .data_mut()
            .element_lists_mut(akantu_mesh::utils::ELEMENT_TO_SUBELEMENT)
            .unwrap();
        let rows = e2s.get_mut(T::Segment2, NG).unwrap();
        rows.as_mut_slice()[0] = vec![Element::NULL];
    }
    let e = check_facet_mesh_consistency(&m).unwrap_err();
    assert!(e.to_string().contains("does not list"), "{e}");
}

/// Events recorded by a distributed hook during `apply`
/// are kept (they were dropped) and their schedule is agreed.
#[test]
fn v36_events_recorded_by_a_distributed_hook_are_dispatched() {
    struct Hook;
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
            let n = tx.push_node(MeshPart::Main, &[7., 7.])?;
            tx.record(MeshPart::Main, nodes_added(n));
            Ok(())
        }
    }
    let out = run_local(2, std::time::Duration::from_secs(20), |comm| {
        let mut m = Mesh::with_communicator(2, "m", comm).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [[0., 0.], [1., 0.], [0., 1.]] {
                a.push_node(&x).unwrap();
            }
            a.connectivity_mut(T::Triangle3, NG).push_tuple(&[0, 1, 2]);
            a.set_distributed();
        }
        let d = SharedMesh::new(m);
        let t = d
            .register_consumer("main", EventPriority(60), MeshPart::Main)
            .unwrap();
        let mut s = Shadow {
            nodes: (0..3).collect(),
            ..Shadow::default()
        };
        let mut h = Hook;
        let c = d
            .mutate(
                &mut [(&t, &mut s)],
                |tx| {
                    let n = tx.push_node(MeshPart::Main, &[3., 3.])?;
                    tx.record(MeshPart::Main, nodes_added(n));
                    Ok(())
                },
                Some(&mut h),
            )
            .unwrap();
        (c.events.len(), s.nodes.len(), s.errors.len())
    });
    for (nb_events, nodes, errors) in out {
        assert_eq!((nb_events, nodes, errors), (2, 5, 0));
    }
}
