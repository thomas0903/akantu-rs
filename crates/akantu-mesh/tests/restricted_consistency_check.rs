// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Tests of the
// restricted cross-part check of a mesh transaction (performance campaign):
// a transaction that starts from a state that passed the check reads the
// adjacency references only from the rows it replaced or appended. The
// verdicts must be those of the full check (`check_facet_mesh_consistency`,
// still what debug builds run by default) on every edit a transaction can
// make through the journaled row API, and a mesh that was never checked is
// checked in full first. Also the journal of the element groups' node lists
// (`rebuild_element_group_nodes` without a snapshot of every group).

use akantu_core::{Element, ElementType as T, Error, GhostType as G, Idx};
use akantu_mesh::utils::{ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, init_mesh_facets};
use akantu_mesh::{Mesh, MeshAccessor, MeshPart, SharedMesh};

const NG: G = G::NotGhost;

/// Two triangles sharing edge (0, 2), with their facet mesh and an element
/// group `g` on the first triangle whose node list also holds node 3 (a
/// rebuild from the connectivity drops it).
fn two_triangles() -> Mesh {
    let mut m = Mesh::new(2, "m").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [[0., 0.], [1., 0.], [1., 1.], [0., 1.]] {
            a.push_node(&x).unwrap();
        }
        a.connectivity_mut(T::Triangle3, NG).push_tuple(&[0, 1, 2]);
        a.connectivity_mut(T::Triangle3, NG).push_tuple(&[0, 2, 3]);
    }
    m.compute_bounding_box().unwrap();
    let _ = init_mesh_facets(&mut m).unwrap();
    {
        let mut g = m.groups_mut().create_element_group("g", 2).unwrap();
        g.add(tri(0), Some(&[0, 1, 2]), false).unwrap();
        g.add_node(3, false);
    }
    m
}

fn tri(i: Idx) -> Element {
    Element::new(T::Triangle3, i, NG)
}

fn seg(i: Idx) -> Element {
    Element::new(T::Segment2, i, NG)
}

fn fingerprint(d: &SharedMesh) -> String {
    d.read().unwrap().state_fingerprint()
}

fn s2e_row(d: &SharedMesh, el: Element) -> Vec<Element> {
    let m = d.read().unwrap();
    m.mesh_facets()
        .unwrap()
        .data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .element(el)
        .unwrap()
        .to_vec()
}

fn group_nodes(d: &SharedMesh) -> Vec<Idx> {
    let m = d.read().unwrap();
    m.groups()
        .element_group("g")
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

/// A shared mesh whose state passed the check (one committed mutation) with
/// the given check mode.
fn checked(full: bool) -> SharedMesh {
    let d = SharedMesh::new(two_triangles());
    d.set_full_consistency_checks(full).unwrap();
    let row = s2e_row(&d, tri(0));
    d.mutate(
        &mut [],
        |tx| tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &row),
        None,
    )
    .unwrap();
    d
}

type Edit = fn(&mut akantu_mesh::MeshTransaction<'_>) -> akantu_core::Result<()>;

/// Edits through the journaled row API: references out of range in a
/// replaced or an appended row of each dataset, and valid edits.
fn edits() -> Vec<(&'static str, Edit, bool)> {
    vec![
        (
            "subelement_to_element row with a facet outside its block",
            |tx| {
                let row = vec![seg(0), seg(1), seg(42)];
                tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &row)
            },
            false,
        ),
        (
            "element_to_subelement row with an element outside its block",
            |tx| {
                tx.set_element_list_row(
                    MeshPart::Facets,
                    ELEMENT_TO_SUBELEMENT,
                    seg(0),
                    vec![tri(0), tri(9)],
                )
            },
            false,
        ),
        (
            "appended facet whose element_to_subelement row is out of range",
            |tx| {
                let conn = tx
                    .mesh()
                    .mesh_facets()
                    .unwrap()
                    .element_connectivity(seg(0))
                    .unwrap()
                    .to_vec();
                tx.push_element(MeshPart::Facets, T::Segment2, NG, &conn)?;
                tx.push_element_list_row(
                    MeshPart::Facets,
                    ELEMENT_TO_SUBELEMENT,
                    T::Segment2,
                    NG,
                    vec![tri(5)],
                )
            },
            false,
        ),
        (
            "appended facet without its element_to_subelement row",
            |tx| {
                let conn = tx
                    .mesh()
                    .mesh_facets()
                    .unwrap()
                    .element_connectivity(seg(0))
                    .unwrap()
                    .to_vec();
                tx.push_element(MeshPart::Facets, T::Segment2, NG, &conn)
                    .map(|_| ())
            },
            false,
        ),
        (
            "appended facet with a valid row",
            |tx| {
                let conn = tx
                    .mesh()
                    .mesh_facets()
                    .unwrap()
                    .element_connectivity(seg(0))
                    .unwrap()
                    .to_vec();
                let subs = tx
                    .mesh()
                    .mesh_facets()
                    .unwrap()
                    .data()
                    .element_map(SUBELEMENT_TO_ELEMENT)?
                    .element(seg(0))?
                    .iter()
                    .map(|_| Element::NULL)
                    .collect::<Vec<_>>();
                tx.push_element(MeshPart::Facets, T::Segment2, NG, &conn)?;
                tx.push_element_data_rows(
                    MeshPart::Facets,
                    SUBELEMENT_TO_ELEMENT,
                    T::Segment2,
                    NG,
                    subs.len(),
                    &subs,
                )?;
                tx.push_element_list_row(
                    MeshPart::Facets,
                    ELEMENT_TO_SUBELEMENT,
                    T::Segment2,
                    NG,
                    vec![tri(1)],
                )
            },
            true,
        ),
        (
            "reordered subelement_to_element row",
            |tx| {
                let row: Vec<Element> = tx
                    .mesh()
                    .mesh_facets()
                    .unwrap()
                    .data()
                    .element_map(SUBELEMENT_TO_ELEMENT)?
                    .element(tri(1))?
                    .iter()
                    .rev()
                    .copied()
                    .collect();
                tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(1), &row)
            },
            true,
        ),
    ]
}

/// Every edit gets the same verdict from the restricted and the full
/// check, a refused one leaves the mesh as it was, and the restricted
/// check is taken after a checked commit.
#[test]
fn restricted_and_full_checks_agree_on_the_journaled_edits() {
    for (what, edit, valid) in edits() {
        let mut verdicts = Vec::new();
        for full in [true, false] {
            let d = checked(full);
            let before = fingerprint(&d);
            let r = d.mutate(&mut [], edit, None);
            assert_eq!(r.is_ok(), valid, "{what} (full check: {full}): {r:?}");
            if !valid {
                assert_eq!(fingerprint(&d), before, "{what}: rolled back");
            }
            verdicts.push(r.map(|_| ()).map_err(|e| e.to_string()));
        }
        assert_eq!(verdicts[0].is_ok(), verdicts[1].is_ok(), "{what}");
    }
}

/// A mesh that was never checked (shared as built) takes the full check at
/// its first mutation: an inconsistency in a row the mutation does not
/// touch is reported, and keeps being reported (no checked state).
#[test]
fn an_unchecked_mesh_is_checked_in_full_first() {
    let mut m = two_triangles();
    // break an untouched row before sharing
    m.mesh_facets_mut()
        .unwrap()
        .data_mut()
        .element_map_mut(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .element_mut(tri(1))
        .unwrap()[0] = seg(77);
    let d = SharedMesh::new(m);
    d.set_full_consistency_checks(false).unwrap();
    for _ in 0..2 {
        let row = s2e_row(&d, tri(0));
        let r = d.mutate(
            &mut [],
            |tx| tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &row),
            None,
        );
        let e = r.expect_err("the broken untouched row is found");
        assert!(e.to_string().contains("outside its block"), "{e}");
    }
}

fn injected() -> Error {
    Error::invalid("injected failure")
}

/// The node lists rebuilt by `rebuild_element_group_nodes` are journaled
/// (no snapshot of the group manager) and roll back, also when the group
/// manager is snapshotted after the rebuild in the same transaction; a
/// commit keeps the sorted, unique nodes of the group's elements.
#[test]
fn rebuilt_group_node_lists_roll_back() {
    for snapshot_after in [false, true] {
        let d = SharedMesh::new(two_triangles());
        let before = group_nodes(&d);
        assert_eq!(before, vec![0, 1, 2, 3]);
        let fp = fingerprint(&d);
        let r = d.mutate(
            &mut [],
            |tx| {
                tx.rebuild_element_group_nodes(MeshPart::Main)?;
                if snapshot_after {
                    tx.groups_mut(MeshPart::Main)?
                        .element_group_mut("g")?
                        .add_node(1, false);
                }
                Err(injected())
            },
            None,
        );
        assert!(r.is_err());
        assert_eq!(group_nodes(&d), before, "snapshot after: {snapshot_after}");
        assert_eq!(fingerprint(&d), fp, "snapshot after: {snapshot_after}");
    }
    let d = SharedMesh::new(two_triangles());
    d.mutate(
        &mut [],
        |tx| tx.rebuild_element_group_nodes(MeshPart::Main),
        None,
    )
    .unwrap();
    assert_eq!(group_nodes(&d), vec![0, 1, 2]);
}

/// The node synchronizer replaced by `replace_node_synchronizer` is
/// journaled and rolls back, also when the synchronizers are snapshotted
/// after the replacement in the same transaction; a commit keeps the new
/// one.
#[test]
fn replaced_node_synchronizer_rolls_back() {
    for snapshot_after in [false, true] {
        let d = SharedMesh::new(two_triangles());
        assert!(d.read().unwrap().synchronizers().node.is_none());
        let r = d.mutate(
            &mut [],
            |tx| {
                let comm = tx.mesh().communicator().clone();
                tx.replace_node_synchronizer(
                    MeshPart::Main,
                    Some(akantu_comm::Synchronizer::new("nodes".to_owned(), comm)),
                )?;
                if snapshot_after {
                    tx.synchronizers_mut(MeshPart::Main)?;
                }
                Err(injected())
            },
            None,
        );
        assert!(r.is_err());
        assert!(
            d.read().unwrap().synchronizers().node.is_none(),
            "snapshot after: {snapshot_after}"
        );
    }
    let d = SharedMesh::new(two_triangles());
    d.mutate(
        &mut [],
        |tx| {
            let comm = tx.mesh().communicator().clone();
            tx.replace_node_synchronizer(
                MeshPart::Main,
                Some(akantu_comm::Synchronizer::new("nodes".to_owned(), comm)),
            )
        },
        None,
    )
    .unwrap();
    assert!(d.read().unwrap().synchronizers().node.is_some());
}
