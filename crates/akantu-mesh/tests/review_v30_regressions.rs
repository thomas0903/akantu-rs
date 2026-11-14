// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the mesh layer. Ported
// behaviour: src/mesh/mesh.cc (eraseElements, initMeshFacets),
// src/mesh/mesh_inline_impl.hh (sendEvent<RemovedNodesEvent>),
// src/mesh_utils/mesh_utils.cc (purifyMesh, flipFacets),
// src/mesh/group_manager.cc (createElementGroup on a node group).
//
// This file only uses API that existed before the fix, so that
// every test can be run there: each test asserts the corrected behaviour and
// fails before the fix (the failure observed there is quoted in the test's doc
// comment). Tests needing API added by the fix are in
// `review_v30_new_api.rs`.

use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use akantu_comm::run_local;
use akantu_core::enums::EventPriority;
use akantu_core::{
    Element, ElementType as T, ElementTypeMapArray, GhostType as G, GlobalIdx, Idx, NodeFlag,
    Result, TypeFilter,
};
use akantu_mesh::clusters::compute_clusters;
use akantu_mesh::group::GroupManager;
use akantu_mesh::removal::erase_elements_setup;
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, MESH_TO_MESH_FACET, PHYSICAL_NAMES, SUBELEMENT_TO_ELEMENT,
    flip_facets_local, init_mesh_facets, purify_mesh,
};
use akantu_mesh::{
    Mesh, MeshAccessor, MeshDataValues, MeshEvent, MeshEventHandler, MeshPart, SharedMesh,
};

const NG: G = G::NotGhost;

// ------------------------------------------------------------------ helpers

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

/// One triangle and one boundary segment (the `mu01_*` mesh of Appendix A).
fn triangle_and_segment() -> Mesh {
    mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.]],
        &[(T::Triangle3, &[&[0, 1, 2]]), (T::Segment2, &[&[0, 1]])],
    )
}

/// Two triangles sharing edge (0, 2) and two boundary segments.
fn two_triangles_and_segments() -> Mesh {
    mesh_2d(
        &[[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
        &[
            (T::Triangle3, &[&[0, 1, 2], &[0, 2, 3]]),
            (T::Segment2, &[&[0, 1], &[2, 3]]),
        ],
    )
}

fn e(ty: T, i: Idx) -> Element {
    Element::new(ty, i, NG)
}

/// Element count of the block an element reference denotes: references of
/// the parent's dimension stored in the facet mesh denote parent elements
/// (they are the facet builders' source elements), all others facets.
fn nb_in_domain(parent: &Mesh, el: Element) -> usize {
    let facets = parent.mesh_facets().unwrap();
    if el.ty().spatial_dimension() == parent.spatial_dimension() {
        parent.nb_element(el.ty(), el.ghost_type())
    } else {
        facets.nb_element(el.ty(), el.ghost_type())
    }
}

/// Full check of the facet-mesh adjacency of `parent`: every
/// `element_to_subelement` / `subelement_to_element` block has one row per
/// element of its domain, every reference is in range, and the two maps are
/// reciprocal.
fn check_facet_adjacency(parent: &Mesh) {
    let facets = parent.mesh_facets().expect("facet mesh");
    let e2s = facets.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
    let s2e = facets.data().element_map(SUBELEMENT_TO_ELEMENT).unwrap();
    for g in G::ALL {
        for ty in facets.element_types(TypeFilter::new().ghost(g)) {
            let rows = e2s.get(ty, g).unwrap();
            assert_eq!(rows.len(), facets.nb_element(ty, g), "e2s rows of {ty}");
            for (f, list) in rows.as_slice().iter().enumerate() {
                let facet = Element::try_new(ty, f, g).unwrap();
                for &p in list.iter().filter(|p| !p.is_null()) {
                    assert!(p.idx() < nb_in_domain(parent, p), "{facet} -> {p}");
                    assert!(
                        s2e.element(p).unwrap().contains(&facet),
                        "{p} does not list its facet {facet}"
                    );
                }
            }
        }
        for ty in s2e.element_types(TypeFilter::new().ghost(g)) {
            let rows = s2e.get(ty, g).unwrap();
            let owner = Element::new(ty, 0, g);
            assert_eq!(rows.len(), nb_in_domain(parent, owner), "s2e rows of {ty}");
            for (k, row) in rows.tuples().enumerate() {
                let el = Element::try_new(ty, k, g).unwrap();
                for &f in row.iter().filter(|f| !f.is_null()) {
                    assert!(f.idx() < facets.nb_element(f.ty(), f.ghost_type()));
                    assert!(
                        e2s.element(f).unwrap()[0].contains(&el),
                        "{f} does not list its element {el}"
                    );
                }
            }
        }
    }
    if let Ok(m2f) = parent.data().element_map(MESH_TO_MESH_FACET) {
        for (ty, g, a) in m2f.as_map().iter_all() {
            assert_eq!(a.len(), parent.nb_element(ty, g), "mesh_to_mesh_facet rows");
            for f in a.as_slice().iter().filter(|f| !f.is_null()) {
                assert!(f.idx() < facets.nb_element(f.ty(), f.ghost_type()));
            }
        }
    }
}

fn e2s_rows(m: &Mesh, ty: T) -> Vec<Vec<Element>> {
    m.data()
        .element_lists(ELEMENT_TO_SUBELEMENT)
        .unwrap()
        .get(ty, NG)
        .unwrap()
        .as_slice()
        .to_vec()
}

fn s2e_rows(m: &Mesh, ty: T) -> Vec<Element> {
    m.data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .get(ty, NG)
        .unwrap()
        .as_slice()
        .to_vec()
}

type Log = Arc<Mutex<Vec<(MeshPart, String, Vec<Idx>)>>>;

/// Consumer logging every event it receives with the node numbering of node
/// removals.
struct Recorder {
    part: MeshPart,
    log: Log,
}

impl MeshEventHandler for Recorder {
    fn check_mesh_event(&self, _: &Mesh, _: &MeshEvent) -> Result<()> {
        // test consumer: accepts every event (no capability limits)
        Ok(())
    }
    fn event_priority(&self) -> EventPriority {
        EventPriority(50)
    }
    fn on_mesh_event(&mut self, _: &Mesh, event: &MeshEvent) -> Result<()> {
        let numbering = match event {
            MeshEvent::NodesRemoved { new_numbering, .. } => new_numbering.clone(),
            _ => Vec::new(),
        };
        self.log
            .lock()
            .unwrap()
            .push((self.part, format!("{event:?}"), numbering));
        Ok(())
    }
}

fn fingerprint(d: &SharedMesh) -> String {
    d.read().unwrap().state_fingerprint()
}

// --------------------------------------------------------------------

/// `mu01_*` (Appendix A): erasing the parent's boundary segment 0 must not
/// compact the facet mesh's Segment2 rows (separate index spaces). Before
/// the fix the facet `element_to_subelement` block shrank from 3 rows to 0.
#[test]
fn mu01_parent_boundary_erase_keeps_facet_rows_setup() {
    let mut m = triangle_and_segment();
    let _ = init_mesh_facets(&mut m).unwrap();
    let before_e2s = e2s_rows(m.mesh_facets().unwrap(), T::Segment2);
    let before_pts = e2s_rows(m.mesh_facets().unwrap(), T::Point1);
    let before_s2e = s2e_rows(m.mesh_facets().unwrap(), T::Triangle3);
    let _ = erase_elements_setup(&mut m, &[e(T::Segment2, 0)]).unwrap();
    assert_eq!(m.nb_element(T::Segment2, NG), 0);
    let f = m.mesh_facets().unwrap();
    assert_eq!(f.nb_element(T::Segment2, NG), 3);
    assert_eq!(e2s_rows(f, T::Segment2), before_e2s);
    assert_eq!(e2s_rows(f, T::Point1), before_pts);
    assert_eq!(s2e_rows(f, T::Triangle3), before_s2e);
    check_facet_adjacency(&m);
}

/// `mu01_*` through the journaled transaction (same failure before the fix).
#[test]
fn mu01_parent_boundary_erase_keeps_facet_rows_transaction() {
    let mut m = triangle_and_segment();
    let _ = init_mesh_facets(&mut m).unwrap();
    let before_e2s = e2s_rows(m.mesh_facets().unwrap(), T::Segment2);
    let d = SharedMesh::new(m);
    d.mutate(
        &mut [],
        |tx| tx.erase_elements(MeshPart::Main, &[e(T::Segment2, 0)]),
        None,
    )
    .unwrap();
    let m = d.read().unwrap();
    let f = m.mesh_facets().unwrap();
    assert_eq!(f.nb_element(T::Segment2, NG), 3);
    assert_eq!(e2s_rows(f, T::Segment2), before_e2s);
    check_facet_adjacency(&m);
}

/// Erasing a parent triangle compacts the parent rows kept in the
/// facet mesh (`subelement_to_element` of Triangle3) and remaps the
/// references to triangles; the facet Segment2 references (same type as the
/// parent's boundary segments) are left alone. Before the fix the facet
/// Segment2 references were also renumbered with the parent's numbering.
#[test]
fn mu01_parent_triangle_erase_remaps_parent_references_only() {
    let mut m = two_triangles_and_segments();
    let _ = init_mesh_facets(&mut m).unwrap();
    let f0 = m.mesh_facets().unwrap();
    let old_s2e = s2e_rows(f0, T::Triangle3);
    let old_pts = e2s_rows(f0, T::Point1);
    let old_seg = e2s_rows(f0, T::Segment2);
    // erase triangle 0 and boundary segment 0 at once
    let _ = erase_elements_setup(&mut m, &[e(T::Triangle3, 0), e(T::Segment2, 0)]).unwrap();
    let f = m.mesh_facets().unwrap();
    // facet blocks untouched in size
    assert_eq!(f.nb_element(T::Segment2, NG), 5);
    // triangle 1 moved to 0: its facet row moved with it
    assert_eq!(s2e_rows(f, T::Triangle3), old_s2e[3..6].to_vec());
    // point → segment references (facet domain) are unchanged
    assert_eq!(e2s_rows(f, T::Point1), old_pts);
    // segment → triangle references: T0 removed (NULL), T1 → T0
    let expect: Vec<Vec<Element>> = old_seg
        .iter()
        .map(|l| {
            l.iter()
                .map(|&p| match (p.is_null(), p.index()) {
                    (true, _) => p,
                    (false, 0) => Element::NULL,
                    (false, _) => e(T::Triangle3, 0),
                })
                .collect()
        })
        .collect();
    assert_eq!(e2s_rows(f, T::Segment2), expect);
    // the reciprocal check only holds for surviving references
    for (list, row) in expect.iter().zip(0..) {
        for p in list.iter().filter(|p| !p.is_null()) {
            assert!(
                s2e_rows(f, T::Triangle3)
                    .chunks(3)
                    .nth(p.idx())
                    .unwrap()
                    .contains(&e(T::Segment2, row))
            );
        }
    }
    check_facet_adjacency(&m);
}

/// Other direction: erasing facets renumbers the facet blocks and
/// the facet references (and the parent's `mesh_to_mesh_facet` values) but
/// never the parent's own adjacency data, whose Segment2 references denote
/// the parent's boundary segments. (Passes before the fix too: kept as the
/// regression of the explicit domain rule.)
#[test]
fn mu01_facet_erase_leaves_parent_references() {
    let mut m = two_triangles_and_segments();
    akantu_mesh::utils::fill_element_to_subelements_data(&mut m).unwrap();
    let parent_e2s = e2s_rows(&m, T::Segment2);
    let parent_s2e = s2e_rows(&m, T::Triangle3);
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    d.mutate(
        &mut [],
        |tx| tx.erase_elements(MeshPart::Facets, &[e(T::Segment2, 0)]),
        None,
    )
    .unwrap();
    let m = d.read().unwrap();
    assert_eq!(e2s_rows(&m, T::Segment2), parent_e2s);
    assert_eq!(s2e_rows(&m, T::Triangle3), parent_s2e);
    assert_eq!(m.mesh_facets().unwrap().nb_element(T::Segment2, NG), 4);
}

// --------------------------------------------------------------------

/// `mu02_*` (Appendix A): a permutation-only purify is observable. Before the fix
/// the committed event list was empty and the consumer was never called.
#[test]
fn mu02_permutation_only_purify_is_dispatched() {
    let m = mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.]],
        &[(T::Triangle3, &[&[2, 0, 1]])],
    );
    let d = SharedMesh::new(m);
    let token = d
        .register_consumer("nodal", EventPriority(50), MeshPart::Main)
        .unwrap();
    let log: Log = Arc::default();
    let mut r = Recorder {
        part: MeshPart::Main,
        log: Arc::clone(&log),
    };
    let committed = d
        .mutate(
            &mut [(&token, &mut r)],
            |tx| tx.purify(MeshPart::Main),
            None,
        )
        .unwrap();
    assert_eq!(committed.events.len(), 1);
    let log = log.lock().unwrap().clone();
    assert_eq!(log.len(), 1, "{log:?}");
    assert_eq!(log[0].2, vec![1, 2, 0]);
    assert_eq!(
        d.read()
            .unwrap()
            .connectivity(T::Triangle3, NG)
            .unwrap()
            .as_slice(),
        &[0, 1, 2]
    );
}

/// The observability of a permutation is agreed across ranks — the
/// zero-work peer (identity numbering, nothing removed) receives its
/// identity numbering exactly once too. Before the fix neither rank's consumer
/// was called.
#[test]
fn mu02_permutation_is_agreed_with_a_zero_work_peer() {
    let out = run_local(2, Duration::from_secs(20), |comm| {
        let conn: &[Idx] = if comm.rank() == 0 {
            &[2, 0, 1]
        } else {
            &[0, 1, 2]
        };
        let mut m = Mesh::with_communicator(2, "m", comm.clone()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in [[0., 0.], [1., 0.], [0., 1.]] {
                a.push_node(&x).unwrap();
            }
            a.connectivity_mut(T::Triangle3, NG).push_tuple(conn);
        }
        let d = SharedMesh::new(m);
        let token = d
            .register_consumer("nodal", EventPriority(50), MeshPart::Main)
            .unwrap();
        let log: Log = Arc::default();
        let mut r = Recorder {
            part: MeshPart::Main,
            log: Arc::clone(&log),
        };
        d.mutate(
            &mut [(&token, &mut r)],
            |tx| tx.purify(MeshPart::Main),
            None,
        )
        .unwrap();
        log.lock().unwrap().clone()
    });
    assert_eq!(out[0].len(), 1);
    assert_eq!(out[0][0].2, vec![1, 2, 0]);
    assert_eq!(out[1].len(), 1, "zero-work peer");
    assert_eq!(out[1][0].2, vec![0, 1, 2]);
}

// --------------------------------------------------------------------

/// Triangle + unused node 3, with its facet mesh, and recorders on both
/// parts.
fn unused_node_domain() -> (SharedMesh, Vec<akantu_mesh::ConsumerToken>, Log) {
    let mut m = mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.], [5., 5.]],
        &[(T::Triangle3, &[&[0, 1, 2]])],
    );
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let tokens = vec![
        d.register_consumer("main", EventPriority(50), MeshPart::Main)
            .unwrap(),
        d.register_consumer("facets", EventPriority(50), MeshPart::Facets)
            .unwrap(),
    ];
    (d, tokens, Log::default())
}

/// Mirroring a parent node removal into the facet mesh records and
/// dispatches the facet node event (after the parent's, same numbering).
/// Before the fix the commit held only `(Main, NodesRemoved(1))` and the Facets
/// consumer was called zero times.
#[test]
fn mu03_mirrored_node_removal_reaches_facet_consumers() {
    let (d, t, log) = unused_node_domain();
    let mut main = Recorder {
        part: MeshPart::Main,
        log: Arc::clone(&log),
    };
    let mut facets = Recorder {
        part: MeshPart::Facets,
        log: Arc::clone(&log),
    };
    let committed = d
        .mutate(
            &mut [(&t[0], &mut main), (&t[1], &mut facets)],
            |tx| tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1]),
            None,
        )
        .unwrap();
    let parts: Vec<MeshPart> = committed.events.iter().map(|(p, _)| *p).collect();
    assert_eq!(parts, vec![MeshPart::Main, MeshPart::Facets]);
    let log = log.lock().unwrap().clone();
    assert_eq!(log.len(), 2, "{log:?}");
    assert_eq!(log[0].0, MeshPart::Main);
    assert_eq!(log[1].0, MeshPart::Facets);
    assert_eq!(log[1].2, vec![0, 1, 2, -1]);
    let m = d.read().unwrap();
    assert_eq!(m.mesh_facets().unwrap().nb_nodes(), 3);
}

/// An error after the mirrored removal rolls both parts back and no
/// consumer is called.
#[test]
fn mu03_mirrored_node_removal_rolls_back() {
    let (d, t, log) = unused_node_domain();
    let before = fingerprint(&d);
    let mut main = Recorder {
        part: MeshPart::Main,
        log: Arc::clone(&log),
    };
    let mut facets = Recorder {
        part: MeshPart::Facets,
        log: Arc::clone(&log),
    };
    let r = d.mutate(
        &mut [(&t[0], &mut main), (&t[1], &mut facets)],
        |tx| {
            tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1])?;
            Err(akantu_core::Error::invalid("injected"))
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    assert!(log.lock().unwrap().is_empty());
}

// --------------------------------------------------------------------

/// A facet-only node renumbering on a mirrored mesh is rejected
/// (shared node numbering). Before the fix `[1,2,0]` committed and permuted the
/// facet nodes only.
#[test]
fn mu04_facet_only_node_renumbering_is_rejected() {
    let mut m = triangle_and_segment();
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let before = fingerprint(&d);
    assert!(
        d.mutate(
            &mut [],
            |tx| tx.remove_nodes(MeshPart::Facets, &[1, 2, 0]),
            None
        )
        .is_err()
    );
    assert!(
        d.mutate(&mut [], |tx| tx.purify(MeshPart::Facets), None)
            .is_err()
    );
    assert_eq!(fingerprint(&d), before);
}

/// Node appends and flag changes go to the shared node set (both
/// parts). Before the fix `push_node(Main)` left the facet mesh one node short
/// and a facet flag change was not seen by the parent.
#[test]
fn mu04_node_appends_and_flags_are_shared() {
    let mut m = triangle_and_segment();
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    d.mutate(
        &mut [],
        |tx| {
            let n = tx.push_node(MeshPart::Main, &[2.0, 2.0])?;
            tx.record(
                MeshPart::Main,
                MeshEvent::NodesAdded {
                    nodes: vec![n],
                    old_nodes: None,
                    origin: "test".into(),
                    extension: None,
                },
            );
            tx.set_node_flag(MeshPart::Facets, 0, NodeFlag::MASTER)
        },
        None,
    )
    .unwrap();
    let m = d.read().unwrap();
    let f = m.mesh_facets().unwrap();
    assert_eq!(f.nb_nodes(), 4);
    assert_eq!(f.nodes().as_slice(), m.nodes().as_slice());
    assert_eq!(m.node_flag(0), NodeFlag::MASTER);
    assert_eq!(f.node_flag(0), NodeFlag::MASTER);
}

// --------------------------------------------------------------------

/// An element group created on an existing node group shares it
/// (upstream `ElementGroup` stores `NodeGroup &`). C++ probe: `p=[0,1]`,
/// `e` on `p`, append 2 to `p` → `e.getNbNodes() == 3`. Before the fix the
/// element group held a copy (2 nodes).
#[test]
fn mu05_element_group_on_node_group_shares_it() {
    let mut g = GroupManager::default();
    g.create_node_group("p").unwrap().set_nodes(vec![0, 1]);
    g.create_element_group_on_node_group("e", 1, "p").unwrap();
    g.node_group_mut("p").unwrap().add(2, false);
    assert_eq!(g.element_group("e").unwrap().nodes(), &[0, 1, 2]);
    assert_eq!(g.element_group("e").unwrap().nb_nodes(), 3);
    // reverse direction
    g.element_group_mut("e").unwrap().add_node(3, false);
    assert_eq!(g.node_group("p").unwrap().node_list(), vec![0, 1, 2, 3]);
    // listed once, nothing hidden
    let names: Vec<String> = g.node_groups().map(|n| n.name().to_owned()).collect();
    assert_eq!(names, vec!["p".to_owned()]);
    // D9: destroying a referenced node group is refused
    assert!(g.destroy_node_group("p").is_err());
    assert_eq!(g.element_group("e").unwrap().nodes(), &[0, 1, 2, 3]);
}

// --------------------------------------------------------------------

/// Facet mesh of one tetrahedron (four Triangle3 facets, not ghost).
fn tetra_facets() -> Mesh {
    let mut m = Mesh::new(3, "t").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [
            [0.0, 0.0, 0.0],
            [1.0, 0.0, 0.0],
            [0.0, 1.0, 0.0],
            [0.0, 0.0, 1.0],
        ] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(T::Tetrahedron4, NG)
            .push_tuple(&[0, 1, 2, 3]);
    }
    let _ = init_mesh_facets(&mut m).unwrap();
    let mut f = Mesh::new(3, "f").unwrap();
    std::mem::swap(&mut f, m.mesh_facets_mut().unwrap());
    f
}

fn remote_of(f: &Mesh, ty: T, edit: &[(usize, Vec<Idx>)]) -> ElementTypeMapArray<GlobalIdx> {
    let conn = f.connectivity(ty, NG).unwrap();
    let mut r = ElementTypeMapArray::new("remote");
    let a = r.alloc(conn.len(), conn.nb_component(), ty, NG, 0).unwrap();
    for (d, &s) in a.as_mut_slice().iter_mut().zip(conn.as_slice()) {
        *d = GlobalIdx::from(s);
    }
    for (k, v) in edit {
        for (d, &s) in a.tuple_mut(*k).iter_mut().zip(v) {
            *d = GlobalIdx::from(s);
        }
    }
    r
}

/// `mu06_*` (category B): by default the flipped facet's global connectivity
/// equals the remote one. C++ probe: local `[0,2,1]`, remote `[2,1,0]` →
/// upstream (inverse permutation) `[1,0,2]`; before the fix the Rust default
/// reproduced `[1,0,2]`.
#[test]
fn mu06_flip_default_matches_the_remote_order() {
    let mut f = tetra_facets();
    // find the facet [0,2,1]
    let conn = f.connectivity(T::Triangle3, NG).unwrap().clone();
    let k = conn
        .tuples()
        .position(|t| t == [0, 2, 1])
        .expect("tetra facet [0,2,1]");
    let remote = remote_of(&f, T::Triangle3, &[(k, vec![2, 1, 0])]);
    flip_facets_local(&mut f, &remote, NG).unwrap();
    assert_eq!(
        f.connectivity(T::Triangle3, NG).unwrap().tuple(k),
        &[2, 1, 0]
    );
}

// --------------------------------------------------------------------

/// An invalid ghost counter is an error before the decrement, not an
/// overflow panic; the transaction rolls back. Before the fix the debug build
/// panicked with "attempt to subtract with overflow".
#[test]
fn mu07_invalid_ghost_counter_is_an_error_not_a_panic() {
    let mut m = Mesh::new(2, "g").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0., 0.], [1., 0.], [0., 1.]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(T::Triangle3, G::Ghost)
            .push_tuple(&[0, 1, 2]);
    }
    let d = SharedMesh::new(m);
    let before = fingerprint(&d);
    let r = catch_unwind(AssertUnwindSafe(|| {
        d.mutate(
            &mut [],
            |tx| {
                tx.ghosts_counters_mut(MeshPart::Main)?.alloc(
                    1,
                    1,
                    T::Triangle3,
                    G::Ghost,
                    Idx::MIN,
                )?;
                tx.erase_elements(MeshPart::Main, &[Element::new(T::Triangle3, 0, G::Ghost)])
            },
            None,
        )
    }));
    let r = r.expect("no panic");
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    assert!(d.poisoned().unwrap().is_none());
}

/// A zero-component `element_to_subelement` block makes
/// `compute_clusters` fail, not panic. Before the fix: index out of bounds.
#[test]
fn mu07_malformed_adjacency_in_clusters_is_an_error() {
    let mut m = triangle_and_segment();
    let _ = init_mesh_facets(&mut m).unwrap();
    let mut facets = Mesh::new(2, "f").unwrap();
    std::mem::swap(&mut facets, m.mesh_facets_mut().unwrap());
    let mut bad: ElementTypeMapArray<Vec<Element>> = ElementTypeMapArray::new("e2s");
    bad.alloc(3, 0, T::Segment2, NG, Vec::new()).unwrap();
    facets
        .data_mut()
        .insert_elemental(ELEMENT_TO_SUBELEMENT, MeshDataValues::ElementList(bad));
    let r = catch_unwind(AssertUnwindSafe(|| {
        compute_clusters(&m, &facets, 2, &|_| true)
    }));
    assert!(r.expect("no panic").is_err());
}

// --------------------------------------------------------------------

/// Removals compose within one transaction (each numbering relative
/// to the state before its operation). Before the fix: "ElementsRemoved:
/// numbering is not a dense injective map onto the 0 entities".
#[test]
fn mu08_repeated_element_removals_compose() {
    let m = mesh_2d(
        &[[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
        &[(T::Triangle3, &[&[0, 1, 2], &[0, 2, 3]])],
    );
    let d = SharedMesh::new(m);
    let c = d
        .mutate(
            &mut [],
            |tx| {
                tx.erase_elements(MeshPart::Main, &[e(T::Triangle3, 0)])?;
                tx.erase_elements(MeshPart::Main, &[e(T::Triangle3, 0)])
            },
            None,
        )
        .unwrap();
    // replaying the events from the initial size reaches the final one
    let mut n = 2usize;
    for (_, ev) in &c.events {
        let MeshEvent::ElementsRemoved { new_numbering, .. } = ev else {
            panic!("{ev:?}")
        };
        let num = new_numbering.get(T::Triangle3, NG).unwrap().as_slice();
        assert_eq!(num.len(), n);
        n = num.iter().filter(|&&x| x >= 0).count();
    }
    assert_eq!(c.events.len(), 2);
    assert_eq!(n, 0);
    assert_eq!(d.read().unwrap().nb_element(T::Triangle3, NG), 0);
}

/// Repeated node removals and a removal followed by an addition (and
/// another removal) compose. Before the fix the first removal's numbering was
/// validated against the final node count and the mutation failed.
#[test]
fn mu08_node_removals_and_additions_compose() {
    let m = mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.], [7., 7.], [8., 8.]],
        &[(T::Triangle3, &[&[0, 1, 2]])],
    );
    let d = SharedMesh::new(m);
    let c = d
        .mutate(
            &mut [],
            |tx| {
                tx.remove_nodes(MeshPart::Main, &[0, 1, 2, 3, -1])?;
                tx.remove_nodes(MeshPart::Main, &[0, 1, 2, -1])?;
                let n = tx.push_node(MeshPart::Main, &[9.0, 9.0])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::NodesAdded {
                        nodes: vec![n],
                        old_nodes: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                tx.purify(MeshPart::Main)
            },
            None,
        )
        .unwrap();
    // replay node counts
    let mut n = 5usize;
    for (_, ev) in &c.events {
        match ev {
            MeshEvent::NodesRemoved { new_numbering, .. } => {
                assert_eq!(new_numbering.len(), n);
                n = new_numbering.iter().filter(|&&x| x >= 0).count();
            }
            MeshEvent::NodesAdded { nodes, .. } => n += nodes.len(),
            other => panic!("{other:?}"),
        }
    }
    assert_eq!(n, 3);
    assert_eq!(d.read().unwrap().nb_nodes(), 3);
}

// --------------------------------------------------------------------

/// Facet mesh of the two-triangle square and the remote connectivities of
/// every edge reversed.
fn square_facets_reversed() -> (Mesh, ElementTypeMapArray<GlobalIdx>) {
    let mut m = mesh_2d(
        &[[0., 0.], [1., 0.], [1., 1.], [0., 1.]],
        &[(T::Triangle3, &[&[0, 1, 2], &[0, 2, 3]])],
    );
    let _ = init_mesh_facets(&mut m).unwrap();
    let mut f = Mesh::new(2, "f").unwrap();
    std::mem::swap(&mut f, m.mesh_facets_mut().unwrap());
    let edits: Vec<(usize, Vec<Idx>)> = f
        .connectivity(T::Segment2, NG)
        .unwrap()
        .tuples()
        .enumerate()
        .map(|(k, t)| (k, vec![t[1], t[0]]))
        .collect();
    let remote = remote_of(&f, T::Segment2, &edits);
    (f, remote)
}

/// Missing adjacency rows are detected while planning; the mesh is
/// unchanged on error. Before the fix the connectivity was rewritten before the
/// `OutOfRange` error.
#[test]
fn mu09_flip_validates_before_any_change() {
    let (mut f, remote) = square_facets_reversed();
    f.data_mut()
        .element_lists_mut(ELEMENT_TO_SUBELEMENT)
        .unwrap()
        .get_mut(T::Segment2, NG)
        .unwrap()
        .clear();
    let before = f.state_fingerprint();
    assert!(flip_facets_local(&mut f, &remote, NG).is_err());
    assert_eq!(f.state_fingerprint(), before);
}

/// A late invalid row (after valid flips) fails without any change,
/// and a dataset of the wrong type is an error, not a silent skip. Before
/// the fix both calls returned `Ok`.
#[test]
fn mu09_late_invalid_row_and_wrong_dataset_type_are_errors() {
    let (mut f, remote) = square_facets_reversed();
    let last = f.nb_element(T::Segment2, NG) - 1;
    f.data_mut()
        .element_lists_mut(ELEMENT_TO_SUBELEMENT)
        .unwrap()
        .get_mut(T::Segment2, NG)
        .unwrap()
        .as_mut_slice()[last] = vec![e(T::Triangle3, 1)];
    let before = f.state_fingerprint();
    assert!(flip_facets_local(&mut f, &remote, NG).is_err());
    assert_eq!(f.state_fingerprint(), before);

    let (mut f, remote) = square_facets_reversed();
    let mut ints: ElementTypeMapArray<i32> = ElementTypeMapArray::new("x");
    ints.alloc(f.nb_element(T::Segment2, NG), 1, T::Segment2, NG, 0)
        .unwrap();
    f.data_mut()
        .insert_elemental(SUBELEMENT_TO_ELEMENT, MeshDataValues::Int(ints.into()));
    let before = f.state_fingerprint();
    assert!(flip_facets_local(&mut f, &remote, NG).is_err());
    assert_eq!(f.state_fingerprint(), before);
}

// --------------------------------------------------------------------

/// Public set-up mutators advance the revision exactly once. Before
/// the fix `removal::remove_nodes` and `flip_facets_local` left it
/// unchanged, `init_mesh_facets` too, and `SharedMesh::setup(purify_mesh)`
/// advanced it twice.
#[test]
fn mu12_public_mutators_advance_the_revision_once() {
    let mut m = mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.], [3., 3.]],
        &[(T::Triangle3, &[&[0, 1, 2]])],
    );
    let r0 = m.revision();
    let _ = akantu_mesh::removal::remove_nodes(&mut m, &[0, 1, 2, -1], "test").unwrap();
    assert_eq!(m.revision(), r0 + 1, "remove_nodes");
    let r0 = m.revision();
    let _ = init_mesh_facets(&mut m).unwrap();
    assert_eq!(m.revision(), r0 + 1, "init_mesh_facets");

    let (mut f, remote) = square_facets_reversed();
    let r0 = f.revision();
    flip_facets_local(&mut f, &remote, NG).unwrap();
    assert_eq!(f.revision(), r0 + 1, "flip_facets_local");

    let m = mesh_2d(
        &[[0., 0.], [1., 0.], [0., 1.]],
        &[(T::Triangle3, &[&[2, 0, 1]])],
    );
    let d = SharedMesh::new(m);
    let r0 = d.read().unwrap().revision();
    d.setup(purify_mesh).unwrap();
    assert_eq!(d.read().unwrap().revision(), r0 + 1, "setup(purify_mesh)");
}

// ---------------------------------------------------------------------- D13

/// Triangle + segment with physical names and an `Int` dataset named
/// `mesh_to_mesh_facet`.
fn with_wrong_m2f_type() -> Mesh {
    let mut m = triangle_and_segment();
    let data = m.data_mut();
    let names = data.strings_mut_or_insert(PHYSICAL_NAMES).unwrap();
    names
        .alloc(1, 1, T::Triangle3, NG, "body".to_owned())
        .unwrap();
    names
        .alloc(1, 1, T::Segment2, NG, "edge".to_owned())
        .unwrap();
    data.ints_mut_or_insert(MESH_TO_MESH_FACET)
        .unwrap()
        .alloc(1, 1, T::Segment2, NG, 0)
        .unwrap();
    m
}

/// D13: the transaction variant of `initMeshFacets` rejects a pre-existing
/// `mesh_to_mesh_facet` of another type, as the set-up variant does. Before
/// the fix it was silently replaced.
#[test]
fn d13_transaction_init_mesh_facets_rejects_a_wrong_m2f_type() {
    assert!(init_mesh_facets(&mut with_wrong_m2f_type()).is_err());
    let d = SharedMesh::new(with_wrong_m2f_type());
    let before = fingerprint(&d);
    assert!(d.mutate(&mut [], |tx| tx.init_mesh_facets(), None).is_err());
    assert_eq!(fingerprint(&d), before);
}
