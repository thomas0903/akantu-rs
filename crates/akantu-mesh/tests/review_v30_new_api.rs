// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the mesh layer that need API added by
// the fix (see also `review_v30_regressions.rs`):
// `mu06_*` facet flips in both modes (3-cycles, 4-cycles, higher order,
// adjacency and orientation), `mu05_*` shared node groups through mesh
// operations and transactions, `mu01_*`/`mu04_*` cross-part invariants after
// removals, `mu08_*`/`mu12_*` complements. Ported behaviour:
// src/mesh_utils/mesh_utils.cc (flipFacets), src/mesh/group_manager.cc,
// src/mesh/element_group.cc, src/mesh/mesh.cc (eraseElements).

use std::panic::{AssertUnwindSafe, catch_unwind};

use akantu_core::{
    Element, ElementType as T, ElementTypeMapArray, GhostType as G, GlobalIdx, Idx, Real,
};
use akantu_mesh::removal::{erase_elements_setup, remove_nodes};
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, FacetFlipMode, SUBELEMENT_TO_ELEMENT, build_facets,
    check_facet_mesh_consistency, flip_facets_local, flip_facets_local_with, init_mesh_facets,
    purify_mesh,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshDataValues, MeshEvent, MeshPart, SharedMesh};

const NG: G = G::NotGhost;

fn mesh(dim: usize, nodes: &[Vec<Real>], blocks: &[(T, Vec<Vec<Idx>>)]) -> Mesh {
    let mut m = Mesh::new(dim, "m").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in nodes {
            a.push_node(x).unwrap();
        }
        for (ty, conns) in blocks {
            for c in conns {
                a.connectivity_mut(*ty, NG).push_tuple(c);
            }
        }
    }
    m.compute_bounding_box().unwrap();
    m
}

/// The facet mesh of `m` (built by `init_mesh_facets`), moved out.
fn facets_of(mut m: Mesh) -> Mesh {
    let _ = init_mesh_facets(&mut m).unwrap();
    let mut f = Mesh::new(m.spatial_dimension(), "f").unwrap();
    std::mem::swap(&mut f, m.mesh_facets_mut().unwrap());
    f
}

/// Two tetrahedra sharing the face (1, 2, 3).
fn two_tets() -> Mesh {
    mesh(
        3,
        &[
            vec![0., 0., 0.],
            vec![1., 0., 0.],
            vec![0., 1., 0.],
            vec![0., 0., 1.],
            vec![1., 1., 1.],
        ],
        &[(T::Tetrahedron4, vec![vec![0, 1, 2, 3], vec![1, 2, 3, 4]])],
    )
}

/// One hexahedron (Akantu node order: bottom face then top face).
fn hexahedron() -> Mesh {
    let mut nodes = Vec::new();
    for z in [0., 1.] {
        for (x, y) in [(0., 0.), (1., 0.), (1., 1.), (0., 1.)] {
            nodes.push(vec![x, y, z]);
        }
    }
    mesh(3, &nodes, &[(T::Hexahedron8, vec![(0..8).collect()])])
}

/// One quadratic tetrahedron: corners 0-3, mid-edge nodes (01, 12, 20, 03,
/// 13, 23) at the midpoints.
fn tetra10() -> Mesh {
    let c = [[0., 0., 0.], [1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    let mut nodes: Vec<Vec<Real>> = c.iter().map(|x| x.to_vec()).collect();
    for (a, b) in [(0, 1), (1, 2), (2, 0), (0, 3), (1, 3), (2, 3)] {
        nodes.push((0..3).map(|d| (c[a][d] + c[b][d]) / 2.0).collect());
    }
    mesh(3, &nodes, &[(T::Tetrahedron10, vec![(0..10).collect()])])
}

/// Remote global connectivities = the local ones (no global ids: global =
/// local) except `edit`.
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

fn sub(a: &[Real], b: &[Real]) -> [Real; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}

/// Normal of the facet given by its first three nodes.
fn normal(m: &Mesh, conn: &[Idx]) -> [Real; 3] {
    let x = |n: Idx| m.nodes().tuple(usize::try_from(n).unwrap()).to_vec();
    let (u, v) = (sub(&x(conn[1]), &x(conn[0])), sub(&x(conn[2]), &x(conn[0])));
    [
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ]
}

fn dot(a: [Real; 3], b: [Real; 3]) -> Real {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn e2s(f: &Mesh, ty: T, k: usize) -> Vec<Element> {
    f.data()
        .element_lists(ELEMENT_TO_SUBELEMENT)
        .unwrap()
        .get(ty, NG)
        .unwrap()
        .as_slice()[k]
        .clone()
}

fn s2e(f: &Mesh, ty: T, k: usize) -> Vec<Element> {
    f.data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .get(ty, NG)
        .unwrap()
        .tuple(k)
        .to_vec()
}

/// Flip facet `k` of `ty` with remote order `remote` in both modes and
/// check: the corrected connectivity equals `remote` (global = local
/// here) with the remote orientation; the compatibility one equals
/// `upstream`; the adjacency is inverted iff `inverted`, identically in
/// both modes.
fn check_flip(f: &Mesh, ty: T, k: usize, remote: &[Idx], upstream: &[Idx], inverted: bool) {
    let r = remote_of(f, ty, &[(k, remote.to_vec())]);
    let (e_before, s_before) = (e2s(f, ty, k), s2e(f, ty, k));
    for mode in [FacetFlipMode::RemoteOrder, FacetFlipMode::UpstreamInverse] {
        let mut g = Mesh::new(f.spatial_dimension(), "copy").unwrap();
        // fresh copy through the fingerprint-stable builder of the caller
        g.clone_from_test(f);
        flip_facets_local_with(&mut g, &r, NG, mode).unwrap();
        let conn = g.connectivity(ty, NG).unwrap().tuple(k).to_vec();
        match mode {
            FacetFlipMode::RemoteOrder => {
                assert_eq!(conn, remote, "corrected order of {ty} {k}");
                assert_eq!(normal(&g, &conn), normal(&g, remote));
            }
            FacetFlipMode::UpstreamInverse => {
                assert_eq!(conn, upstream, "upstream order of {ty} {k}");
                // same orientation class as the remote order
                assert!(dot(normal(&g, &conn), normal(&g, remote)) > 0.0);
            }
        }
        let (e_after, s_after) = (e2s(&g, ty, k), s2e(&g, ty, k));
        if inverted {
            if e_before[1].is_null() {
                assert_eq!(e_after, e_before);
            } else {
                assert_eq!((e_after[0], e_after[1]), (e_before[1], e_before[0]));
            }
            assert_eq!((s_after[0], s_after[1]), (s_before[1], s_before[0]));
            assert_eq!(s_after[2..], s_before[2..]);
        } else {
            assert_eq!(e_after, e_before);
            assert_eq!(s_after, s_before);
        }
    }
}

/// Test-only deep copy (`Mesh` is not `Clone`): rebuilt from its arrays.
trait CloneFromTest {
    fn clone_from_test(&mut self, other: &Mesh);
}

impl CloneFromTest for Mesh {
    fn clone_from_test(&mut self, other: &Mesh) {
        let mut m = Mesh::new(other.spatial_dimension(), other.id()).unwrap();
        {
            let mut a = MeshAccessor::new(&mut m);
            for x in other.nodes().tuples() {
                a.push_node(x).unwrap();
            }
            for (ty, g, c) in other.connectivities().as_map().iter_all() {
                a.connectivity_mut(ty, g).replace(c.borrow().clone());
            }
        }
        for name in other.data().elemental_names() {
            m.data_mut()
                .insert_elemental(name, other.data().elemental(name).unwrap().clone());
        }
        *self = m;
    }
}

fn position(f: &Mesh, ty: T, nodes: &[Idx]) -> usize {
    f.connectivity(ty, NG)
        .unwrap()
        .tuples()
        .position(|t| t == nodes)
        .unwrap_or_else(|| panic!("no {ty} facet {nodes:?}"))
}

/// The C++ probe facet (local `[0,2,1]`, remote `[2,1,0]`, upstream
/// result `[1,0,2]`) in both modes; a 3-cycle of the shared face (no
/// inversion) and a reflection (inversion, neighbours swapped).
#[test]
fn mu06_triangle_facets_both_modes() {
    let f = facets_of(two_tets());
    let k = position(&f, T::Triangle3, &[0, 2, 1]);
    // remote [2,1,0] is a rotation of [0,2,1]: no inversion
    check_flip(&f, T::Triangle3, k, &[2, 1, 0], &[1, 0, 2], false);
    let shared = position(&f, T::Triangle3, &[1, 2, 3]);
    assert!(!e2s(&f, T::Triangle3, shared)[1].is_null());
    // 3-cycle
    check_flip(&f, T::Triangle3, shared, &[2, 3, 1], &[3, 1, 2], false);
    // reflection (a transposition is its own inverse)
    check_flip(&f, T::Triangle3, shared, &[1, 3, 2], &[1, 3, 2], true);
}

/// 4-cycles of a quadrangle facet (rotation: no inversion; the two
/// modes differ) and a reflection (inversion; identical in both modes).
#[test]
fn mu06_quadrangle_facets_both_modes() {
    let f = facets_of(hexahedron());
    let k = 0;
    let l: Vec<Idx> = f
        .connectivity(T::Quadrangle4, NG)
        .unwrap()
        .tuple(k)
        .to_vec();
    let (a, b, c, d) = (l[0], l[1], l[2], l[3]);
    check_flip(&f, T::Quadrangle4, k, &[b, c, d, a], &[d, a, b, c], false);
    check_flip(&f, T::Quadrangle4, k, &[c, d, a, b], &[c, d, a, b], false);
    check_flip(&f, T::Quadrangle4, k, &[a, d, c, b], &[a, d, c, b], true);
}

/// Higher-order facets — a rotation of a Triangle6 facet moves the
/// mid-edge nodes with the corners in the corrected mode; upstream applies
/// the inverse rotation to all six nodes.
#[test]
fn mu06_higher_order_facets_both_modes() {
    let f = facets_of(tetra10());
    let k = 0;
    let l: Vec<Idx> = f.connectivity(T::Triangle6, NG).unwrap().tuple(k).to_vec();
    let rot = [l[1], l[2], l[0], l[4], l[5], l[3]];
    let inv = [l[2], l[0], l[1], l[5], l[3], l[4]];
    check_flip(&f, T::Triangle6, k, &rot, &inv, false);
    let refl = [l[0], l[2], l[1], l[5], l[4], l[3]];
    check_flip(&f, T::Triangle6, k, &refl, &refl, true);
}

/// The default entry point is the corrected mode; a
/// zero-component adjacency block is an error (no panic), state unchanged.
#[test]
fn mu06_default_is_remote_order_and_bad_shapes_fail_cleanly() {
    let mut f = facets_of(two_tets());
    let k = position(&f, T::Triangle3, &[1, 2, 3]);
    let r = remote_of(&f, T::Triangle3, &[(k, vec![2, 3, 1])]);
    flip_facets_local(&mut f, &r, NG).unwrap();
    assert_eq!(
        f.connectivity(T::Triangle3, NG).unwrap().tuple(k),
        &[2, 3, 1]
    );

    let mut f = facets_of(two_tets());
    let n = f.nb_element(T::Triangle3, NG);
    let mut bad: ElementTypeMapArray<Vec<Element>> = ElementTypeMapArray::new("bad");
    bad.alloc(n, 0, T::Triangle3, NG, Vec::new()).unwrap();
    f.data_mut()
        .insert_elemental(ELEMENT_TO_SUBELEMENT, MeshDataValues::ElementList(bad));
    let r = remote_of(&f, T::Triangle3, &[(k, vec![1, 3, 2])]);
    let before = f.state_fingerprint();
    let out = catch_unwind(AssertUnwindSafe(|| flip_facets_local(&mut f, &r, NG)));
    assert!(out.expect("no panic").is_err());
    assert_eq!(f.state_fingerprint(), before);
}

// -------------------------------------------------------------------

fn bar() -> Mesh {
    mesh(
        1,
        &[vec![0.], vec![1.], vec![2.], vec![3.]],
        &[(T::Segment2, vec![vec![0, 1], vec![1, 2], vec![2, 3]])],
    )
}

/// `createElementGroupFromNodeGroup` shares the node group; a node
/// removal renumbers the shared list exactly once; a rolled-back
/// transaction restores the shared state; rename and destruction rules.
#[test]
fn mu05_shared_node_group_through_mesh_operations() {
    let mut m = bar();
    m.groups_mut()
        .create_node_group("p")
        .unwrap()
        .set_nodes(vec![1, 2, 3]);
    m.create_element_group_from_node_group("e", "p", 1).unwrap();
    assert_eq!(
        m.groups()
            .element_group("e")
            .unwrap()
            .elements_of(T::Segment2, NG),
        &[1, 2]
    );
    // node 0 removed: 1,2,3 → 0,1,2 once (applied twice it would be wrong)
    let _ = erase_elements_setup(&mut m, &[Element::new(T::Segment2, 0, NG)]).unwrap();
    let _ = remove_nodes(&mut m, &[-1, 0, 1, 2], "test").unwrap();
    assert_eq!(
        m.groups().node_group("p").unwrap().node_list(),
        vec![0, 1, 2]
    );
    assert_eq!(m.groups().element_group("e").unwrap().nodes(), &[0, 1, 2]);
    // rename keeps the identity
    m.groups_mut().rename_node_group("p", "q").unwrap();
    m.groups_mut().node_group_mut("q").unwrap().add(9, false);
    assert_eq!(
        m.groups().element_group("e").unwrap().nodes(),
        &[0, 1, 2, 9]
    );
    {
        let q = m.groups_mut().node_group_mut("q").unwrap();
        let mut v = q.node_list();
        v.pop();
        q.set_nodes(v);
    }
    // rollback restores the shared state
    let d = SharedMesh::new(m);
    let before = d.read().unwrap().state_fingerprint();
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.groups_mut(MeshPart::Main)?
                .element_group_mut("e")?
                .add_node(7, false);
            Err(akantu_core::Error::invalid("injected"))
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(d.read().unwrap().state_fingerprint(), before);
    let m = d.read().unwrap();
    assert_eq!(
        m.groups().node_group("q").unwrap().node_list(),
        vec![0, 1, 2]
    );
}

// --------------------------------------------------------- /

/// After mixed-dimension removals in both directions and a
/// purify, the cross-part invariants hold (shared nodes, index domains,
/// adjacency rows and references).
#[test]
fn mu01_mu04_cross_part_invariants_after_removals() {
    let m = mesh(
        2,
        &[
            vec![0., 0.],
            vec![1., 0.],
            vec![1., 1.],
            vec![0., 1.],
            vec![2., 0.],
        ],
        &[
            (
                T::Triangle3,
                vec![vec![0, 1, 2], vec![0, 2, 3], vec![1, 4, 2]],
            ),
            (T::Segment2, vec![vec![0, 1], vec![2, 3], vec![1, 4]]),
        ],
    );
    let d = SharedMesh::new(m);
    d.mutate(
        &mut [],
        |tx| {
            tx.init_mesh_facets()?;
            tx.erase_elements(
                MeshPart::Main,
                &[
                    Element::new(T::Segment2, 1, NG),
                    Element::new(T::Triangle3, 2, NG),
                ],
            )?;
            tx.erase_elements(MeshPart::Facets, &[Element::new(T::Point1, 4, NG)])?;
            tx.erase_elements(MeshPart::Main, &[Element::new(T::Segment2, 1, NG)])?;
            tx.purify(MeshPart::Main)
        },
        None,
    )
    .unwrap();
    let m = d.read().unwrap();
    check_facet_mesh_consistency(&m).unwrap();
    let f = m.mesh_facets().unwrap();
    assert_eq!(f.nb_nodes(), m.nb_nodes());
    assert_eq!(m.nb_element(T::Triangle3, NG), 2);
    assert_eq!(m.nb_element(T::Segment2, NG), 1);
}

/// An edit that would desynchronise the parts is caught by the
/// commit-time invariant check (here: facet adjacency rows dropped by
/// hand), and rolled back.
#[test]
fn mu04_cross_part_invariant_is_checked_before_commit() {
    let mut m = mesh(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![0., 1.]],
        &[(T::Triangle3, vec![vec![0, 1, 2]])],
    );
    let _ = init_mesh_facets(&mut m).unwrap();
    let d = SharedMesh::new(m);
    let before = d.read().unwrap().state_fingerprint();
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.data_mut(MeshPart::Facets)?
                .element_lists_mut(ELEMENT_TO_SUBELEMENT)?
                .get_mut(T::Segment2, NG)?
                .clear();
            Ok(())
        },
        None,
    );
    assert!(r.is_err(), "{r:?}");
    assert_eq!(d.read().unwrap().state_fingerprint(), before);
}

// ------------------------------------------------------------------

/// An element removal followed by an element addition in the same
/// transaction (each event relative to the state before its operation).
#[test]
fn mu08_element_removal_then_addition() {
    let m = mesh(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![1., 1.], vec![0., 1.]],
        &[(T::Triangle3, vec![vec![0, 1, 2], vec![0, 2, 3]])],
    );
    let d = SharedMesh::new(m);
    let c = d
        .mutate(
            &mut [],
            |tx| {
                tx.erase_elements(MeshPart::Main, &[Element::new(T::Triangle3, 0, NG)])?;
                let e = tx.push_element(MeshPart::Main, T::Triangle3, NG, &[0, 1, 2])?;
                tx.record(
                    MeshPart::Main,
                    MeshEvent::ElementsAdded {
                        elements: vec![e],
                        constitutive_law: None,
                        origin: "test".into(),
                        extension: None,
                    },
                );
                Ok(())
            },
            None,
        )
        .unwrap();
    assert_eq!(c.events.len(), 2);
    assert_eq!(d.read().unwrap().nb_element(T::Triangle3, NG), 2);
}

// ------------------------------------------------------------------

/// Every public set-up mutator advances the revision exactly once.
#[test]
fn mu12_setup_mutators_advance_the_revision_once() {
    let once = |what: &str, m: &mut Mesh, op: &dyn Fn(&mut Mesh)| {
        let r0 = m.revision();
        op(m);
        assert_eq!(m.revision(), r0 + 1, "{what}");
    };
    let mut m = mesh(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![0., 1.], vec![4., 4.]],
        &[
            (T::Triangle3, vec![vec![2, 0, 1]]),
            (T::Segment2, vec![vec![0, 1]]),
        ],
    );
    once("erase_elements_setup", &mut m, &|m| {
        let _ = erase_elements_setup(m, &[Element::new(T::Segment2, 0, NG)]).unwrap();
    });
    once("purify_mesh", &mut m, &|m| {
        let _ = purify_mesh(m).unwrap();
    });
    once("build_facets", &mut m, &|m| {
        let _ = build_facets(m).unwrap();
    });
    let mut f = facets_of(two_tets());
    let k = position(&f, T::Triangle3, &[1, 2, 3]);
    let r = remote_of(&f, T::Triangle3, &[(k, vec![1, 3, 2])]);
    once("flip_facets_local_with", &mut f, &|f| {
        flip_facets_local_with(f, &r, NG, FacetFlipMode::UpstreamInverse).unwrap();
    });
    let src = two_tets();
    let mut target = akantu_mesh::utils::new_facet_mesh(&src, "t").unwrap();
    once("build_all_facets", &mut target, &|t| {
        let _ = akantu_mesh::utils::build_all_facets(&src, t, 3, 0).unwrap();
    });
    let mut target = akantu_mesh::utils::new_facet_mesh(&src, "t").unwrap();
    once("build_facets_dimension", &mut target, &|t| {
        let _ = akantu_mesh::utils::build_facets_dimension(&src, t, false, 3).unwrap();
    });
    once("build_facets_dimension_in_place", &mut target, &|t| {
        let _ = akantu_mesh::utils::build_facets_dimension_in_place(t, false, 2).unwrap();
    });
}

// ------------------------------------------- deviation regressions (UD-*)

/// UD-118: in-place `buildFacets` does not leave the mesh marked
/// as a facet mesh of itself, so a later node removal still compacts its
/// flags and global ids (upstream skips them for facet meshes).
#[test]
fn ud118_build_facets_does_not_mark_the_mesh_as_facet_mesh() {
    let mut m = mesh(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![0., 1.], vec![5., 5.]],
        &[(T::Triangle3, vec![vec![0, 1, 2]])],
    );
    MeshAccessor::new(&mut m)
        .nodes_global_ids_mut()
        .as_mut_slice()
        .copy_from_slice(&[10, 11, 12, 13]);
    let _ = build_facets(&mut m).unwrap();
    assert!(!m.is_mesh_facets());
    let _ = remove_nodes(&mut m, &[0, 1, 2, -1], "test").unwrap();
    assert_eq!(m.nodes_flags().len(), 3);
    assert_eq!(m.nodes_global_ids().unwrap().as_slice(), &[10, 11, 12]);
}

/// UD-119: more facets than slots in
/// `fillElementToSubElementsData` is an error and nothing is installed
/// (upstream dereferences `end()` in unchecked builds).
#[test]
fn ud119_too_many_facets_is_an_error_and_the_mesh_is_unchanged() {
    let mut m = mesh(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![0., 1.]],
        &[
            (T::Triangle3, vec![vec![0, 1, 2]]),
            (
                T::Segment2,
                vec![vec![0, 1], vec![1, 2], vec![2, 0], vec![1, 0]],
            ),
        ],
    );
    let before = m.state_fingerprint();
    assert!(akantu_mesh::utils::fill_element_to_subelements_data(&mut m).is_err());
    assert_eq!(m.state_fingerprint(), before);
}

/// UD-120: ghost facets get one ghost copy (upstream leaves the
/// counter array empty and indexes it on erasure); erasing a facet whose
/// counter is 2 decrements it, the next erasure removes it; the event of
/// the first erasure changes nothing and is not dispatched.
#[test]
fn ud120_ghost_facet_counters() {
    let m = mesh(
        3,
        &[
            vec![0., 0., 0.],
            vec![1., 0., 0.],
            vec![0., 1., 0.],
            vec![0., 0., 1.],
        ],
        &[],
    );
    let mut m = m;
    MeshAccessor::new(&mut m)
        .connectivity_mut(T::Tetrahedron4, G::Ghost)
        .push_tuple(&[0, 1, 2, 3]);
    let _ = init_mesh_facets(&mut m).unwrap();
    let f = m.mesh_facets().unwrap();
    let nb = f.nb_element(T::Triangle3, G::Ghost);
    assert_eq!(nb, 4);
    assert_eq!(
        f.ghosts_counters()
            .get(T::Triangle3, G::Ghost)
            .unwrap()
            .as_slice(),
        &[1, 1, 1, 1]
    );
    let d = SharedMesh::new(m);
    let facet = Element::new(T::Triangle3, 0, G::Ghost);
    let c = d
        .mutate(
            &mut [],
            |tx| {
                tx.ghosts_counters_mut(MeshPart::Facets)?
                    .get_mut(T::Triangle3, G::Ghost)?
                    .as_mut_slice()[0] = 2;
                tx.erase_elements(MeshPart::Facets, &[facet])
            },
            None,
        )
        .unwrap();
    assert!(c.events.is_empty(), "{:?}", c.events);
    let c = d
        .mutate(
            &mut [],
            |tx| tx.erase_elements(MeshPart::Facets, &[facet]),
            None,
        )
        .unwrap();
    assert_eq!(c.events.len(), 1);
    let m = d.read().unwrap();
    let f = m.mesh_facets().unwrap();
    assert_eq!(f.nb_element(T::Triangle3, G::Ghost), 3);
    check_facet_mesh_consistency(&m).unwrap();
}

/// UD-121: `makePeriodic` pairs in the given list order (no fuzzy
/// sort): with two right nodes at the same projected position the first
/// listed one is matched; widely different coordinate scales are handled
/// deterministically.
#[test]
fn ud121_periodic_pairing_follows_the_given_order() {
    use akantu_core::enums::SpatialDirection;
    use akantu_mesh::periodic::make_periodic_lists;
    let mut m = mesh(
        2,
        &[
            vec![0., 0.5],
            vec![1., 0.5],
            vec![1., 0.5],
            vec![1e-300, 1e300],
        ],
        &[(T::Segment2, vec![vec![0, 1], vec![1, 3]])],
    );
    let _ = make_periodic_lists(&mut m, SpatialDirection::X, &[0], &[2, 1]).unwrap();
    assert_eq!(m.periodic_master(2), Some(0));
    assert_eq!(m.periodic_master(1), None);
    let mut n = mesh(
        2,
        &[
            vec![0., 0.5],
            vec![1., 0.5],
            vec![1., 0.5],
            vec![1e-300, 1e300],
        ],
        &[(T::Segment2, vec![vec![0, 1], vec![1, 3]])],
    );
    let _ = make_periodic_lists(&mut n, SpatialDirection::X, &[0], &[1, 2]).unwrap();
    assert_eq!(n.periodic_master(1), Some(0));
    assert_eq!(n.periodic_master(2), None);
}

/// UD-124: `copyElementGroup` copies the nodes too (upstream
/// leaves the copy's node group empty); the copy owns its node group (no
/// sharing with the source).
#[test]
fn ud124_copy_element_group_copies_the_nodes() {
    let mut g = akantu_mesh::GroupManager::default();
    g.create_element_group("a", 1)
        .unwrap()
        .add(Element::new(T::Segment2, 0, NG), Some(&[3, 4]), false)
        .unwrap();
    g.copy_element_group("a", "b").unwrap();
    assert_eq!(g.element_group("b").unwrap().nodes(), &[3, 4]);
    assert_eq!(g.element_group("b").unwrap().node_group.name(), "b_nodes");
    g.node_group_mut("b_nodes").unwrap().add(5, false);
    assert_eq!(g.element_group("a").unwrap().nodes(), &[3, 4]);
}

/// UD-127: node → element sets are computed on demand right after
/// a read (upstream returns the empty cache of a freshly read mesh).
#[test]
fn ud127_node_to_element_sets_after_read() {
    use akantu_core::enums::MeshIoType;
    let text = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n3\n1 0 0 0\n2 1 0 0\n3 0 1 0\n$EndNodes\n\
                $Elements\n1\n1 2 2 1 1 1 2 3\n$EndElements\n";
    let mut m = Mesh::new(2, "read").unwrap();
    akantu_io::read_mesh_from_str(
        &mut m,
        "t.msh",
        text,
        MeshIoType::Gmsh,
        &akantu_io::ReadOptions::default(),
    )
    .unwrap();
    let sets = akantu_mesh::utils::fill_nodes_to_elements(&m, None).unwrap();
    let t = Element::new(T::Triangle3, 0, NG);
    for n in 0..3 {
        assert_eq!(
            akantu_mesh::utils::associated_elements(&sets, n).unwrap(),
            &[t]
        );
    }
}
