// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for the cohesive model: the journaled data-row
// API of `MeshTransaction` (`set_element_data_row`, `set_element_list_row`,
// `push_element_data_rows`, `push_element_list_row`, used by the cohesive
// inserter) must roll back exactly for **any** interleaving with the
// copy-on-write mesh-data snapshot (`data_mut`, also taken by
// `erase_elements`). Ported behaviour: the transactional adjacency edits of
// src/mesh_utils/cohesive_element_inserter.cc / mesh_utils.cc
// (`insertCohesiveElements`, `updateElementalConnectivity`).
//
// Before the fix the rollback restored the snapshot first and then
// replayed every row undo on top of it: a row edit journaled *after* the
// snapshot wrote its intermediate "old" value over the restored data
// (`co04_*` tests below fail there with a fingerprint mismatch).

use akantu_core::{Element, ElementType as T, Error, GhostType as G, Idx};
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, check_facet_mesh_consistency, init_mesh_facets,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshPart, SharedMesh};

const NG: G = G::NotGhost;

/// Two triangles sharing edge (0, 2), with their facet mesh.
fn two_triangles() -> SharedMesh {
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
    SharedMesh::new(m)
}

fn fingerprint(d: &SharedMesh) -> String {
    d.read().unwrap().state_fingerprint()
}

fn tri(i: Idx) -> Element {
    Element::new(T::Triangle3, i, NG)
}

fn seg(i: Idx) -> Element {
    Element::new(T::Segment2, i, NG)
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

fn injected() -> Error {
    Error::invalid("injected failure")
}

/// A `data_mut` snapshot reverses an S2E
/// row, then `set_element_data_row` writes the same row, then the mutation
/// fails. Before the fix the row came back as the intermediate (reversed) value
/// and the fingerprint differed.
#[test]
fn co04_snapshot_then_row_edit_of_the_same_row_rolls_back() {
    let d = two_triangles();
    let before = fingerprint(&d);
    let row0 = s2e_row(&d, tri(0));
    let r = d.mutate(
        &mut [],
        |tx| {
            {
                let data = tx.data_mut(MeshPart::Facets)?;
                let row = data
                    .element_map_mut(SUBELEMENT_TO_ELEMENT)?
                    .element_mut(tri(0))?;
                row.reverse();
            }
            let new_row = [row0[1], row0[1], row0[1]];
            tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &new_row)?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(s2e_row(&d, tri(0)), row0);
    assert_eq!(fingerprint(&d), before);
}

/// The opposite order: a journaled row edit, then a snapshot and a further
/// edit of the same row through `data_mut`.
#[test]
fn co04_row_edit_then_snapshot_rolls_back() {
    let d = two_triangles();
    let before = fingerprint(&d);
    let row0 = s2e_row(&d, tri(0));
    let r = d.mutate(
        &mut [],
        |tx| {
            let new_row = [row0[2], row0[0], row0[1]];
            tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &new_row)?;
            let data = tx.data_mut(MeshPart::Facets)?;
            data.element_map_mut(SUBELEMENT_TO_ELEMENT)?
                .element_mut(tri(0))?
                .reverse();
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(s2e_row(&d, tri(0)), row0);
    assert_eq!(fingerprint(&d), before);
}

/// Row edits on both sides of a snapshot, list rows and appended rows
/// (existing and created blocks / datasets) interleaved with it. Before the fix
/// the list row of `seg(1)` came back as its intermediate (cleared) value
/// and the pre-existing dataset `co04_existing`, removed through the
/// snapshot and re-created by a journaled push, was deleted.
#[test]
fn co04_interleaved_edits_of_every_kind_roll_back() {
    let d = two_triangles();
    d.mutate(
        &mut [],
        |tx| {
            tx.push_element_list_row(
                MeshPart::Facets,
                "co04_existing",
                T::Segment2,
                NG,
                vec![tri(0)],
            )
        },
        None,
    )
    .unwrap();
    let before = fingerprint(&d);
    let r = d.mutate(
        &mut [],
        |tx| {
            // before the snapshot
            tx.set_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                seg(0),
                vec![tri(1), tri(0)],
            )?;
            tx.push_element_data_rows(
                MeshPart::Facets,
                SUBELEMENT_TO_ELEMENT,
                T::Triangle3,
                NG,
                3,
                &[seg(0), seg(1), seg(2)],
            )?;
            tx.push_element_list_row(
                MeshPart::Facets,
                "co04_new_list",
                T::Segment2,
                NG,
                vec![tri(0)],
            )?;
            // snapshot, edits of the same rows through it
            {
                let data = tx.data_mut(MeshPart::Facets)?;
                let e2s = data.element_lists_mut(ELEMENT_TO_SUBELEMENT)?;
                e2s.element_mut(seg(0))?[0].clear();
                e2s.element_mut(seg(1))?[0].clear();
                data.remove_elemental("co04_new_list");
                data.remove_elemental("co04_existing");
            }
            tx.set_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                seg(1),
                vec![tri(1)],
            )?;
            tx.push_element_list_row(
                MeshPart::Facets,
                "co04_existing",
                T::Segment2,
                NG,
                vec![tri(1)],
            )?;
            // after the snapshot
            tx.set_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                seg(0),
                vec![tri(0)],
            )?;
            tx.push_element_data_rows(
                MeshPart::Facets,
                SUBELEMENT_TO_ELEMENT,
                T::Triangle3,
                NG,
                3,
                &[seg(3), seg(4), seg(0)],
            )?;
            tx.push_element_data_rows(
                MeshPart::Facets,
                "co04_new_map",
                T::Segment2,
                NG,
                1,
                &[tri(1)],
            )?;
            tx.push_element_list_row(
                MeshPart::Facets,
                "co04_new_list",
                T::Segment2,
                NG,
                vec![tri(1)],
            )?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    check_facet_mesh_consistency(&d.read().unwrap()).unwrap();
}

/// "Remove, then insert" in one transaction (allowed):
/// `erase_elements` snapshots the data of both parts, then the insertion's
/// journaled adjacency edits follow. Before the fix the rollback left the
/// adjacency corrupted.
#[test]
fn co04_remove_then_row_edits_roll_back() {
    let d = two_triangles();
    let before = fingerprint(&d);
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.erase_elements(MeshPart::Main, &[tri(0)])?;
            // the remaining triangle (now 0) and its first facet
            let row = {
                let fm = tx.mesh().mesh_facets().unwrap();
                fm.data()
                    .element_map(SUBELEMENT_TO_ELEMENT)?
                    .element(tri(0))?
                    .to_vec()
            };
            let rev: Vec<Element> = row.iter().rev().copied().collect();
            tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &rev)?;
            tx.set_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                row[0],
                vec![tri(0), Element::NULL],
            )?;
            tx.push_element_data_rows(
                MeshPart::Facets,
                SUBELEMENT_TO_ELEMENT,
                T::Triangle3,
                NG,
                3,
                &rev,
            )?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    check_facet_mesh_consistency(&d.read().unwrap()).unwrap();
}

// ---------------------------------------------------- unit tests of the API

/// `set_element_data_row`: commit applies the row, a wrong length is an
/// error (no change), rollback restores it.
#[test]
fn set_element_data_row_commit_error_and_rollback() {
    let d = two_triangles();
    let row0 = s2e_row(&d, tri(0));
    let new_row = vec![row0[2], row0[1], row0[0]];
    // wrong length
    let before = fingerprint(&d);
    let r = d.mutate(
        &mut [],
        |tx| tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &row0[..2]),
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    // rollback
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &new_row)?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    // commit
    d.mutate(
        &mut [],
        |tx| tx.set_element_data_row(MeshPart::Facets, SUBELEMENT_TO_ELEMENT, tri(0), &new_row),
        None,
    )
    .unwrap();
    assert_eq!(s2e_row(&d, tri(0)), new_row);
}

/// `set_element_list_row`: commit and rollback.
#[test]
fn set_element_list_row_commit_and_rollback() {
    let d = two_triangles();
    let list = |d: &SharedMesh| {
        d.read()
            .unwrap()
            .mesh_facets()
            .unwrap()
            .data()
            .element_lists(ELEMENT_TO_SUBELEMENT)
            .unwrap()
            .element(seg(0))
            .unwrap()[0]
            .clone()
    };
    let before = fingerprint(&d);
    let old = list(&d);
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.set_element_list_row(MeshPart::Facets, ELEMENT_TO_SUBELEMENT, seg(0), vec![])?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(list(&d), old);
    assert_eq!(fingerprint(&d), before);
    d.mutate(
        &mut [],
        |tx| {
            tx.set_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                seg(0),
                vec![tri(1), tri(0)],
            )
        },
        None,
    )
    .unwrap();
    assert_eq!(list(&d), vec![tri(1), tri(0)]);
}

/// `push_element_data_rows`: existing block, created block, created dataset;
/// a component mismatch and a ragged row length are errors; rollback removes
/// what was created.
#[test]
fn push_element_data_rows_blocks_datasets_and_rollback() {
    let d = two_triangles();
    let before = fingerprint(&d);
    for bad in [
        // ragged
        (3usize, vec![seg(0), seg(1)]),
        // components of the existing block are 3
        (2usize, vec![seg(0), seg(1)]),
        // zero components
        (0usize, vec![]),
    ] {
        let r = d.mutate(
            &mut [],
            |tx| {
                tx.push_element_data_rows(
                    MeshPart::Facets,
                    SUBELEMENT_TO_ELEMENT,
                    T::Triangle3,
                    NG,
                    bad.0,
                    &bad.1,
                )
            },
            None,
        );
        assert!(r.is_err(), "{bad:?}");
        assert_eq!(fingerprint(&d), before);
    }
    let push_all = |tx: &mut akantu_mesh::MeshTransaction<'_>| -> akantu_core::Result<()> {
        tx.push_element_data_rows(
            MeshPart::Facets,
            SUBELEMENT_TO_ELEMENT,
            T::Triangle3,
            NG,
            3,
            &[seg(0), seg(1), seg(2)],
        )?;
        // created block of an existing dataset
        tx.push_element_data_rows(
            MeshPart::Facets,
            SUBELEMENT_TO_ELEMENT,
            T::Quadrangle4,
            NG,
            4,
            &[seg(0), seg(1), seg(2), seg(3)],
        )?;
        // created dataset
        tx.push_element_data_rows(
            MeshPart::Facets,
            "co04_created",
            T::Segment2,
            NG,
            2,
            &[tri(0), tri(1)],
        )
    };
    let r = d.mutate(
        &mut [],
        |tx| {
            push_all(tx)?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    // commit (the mesh-consistency check of the topology stage would reject
    // the extra S2E rows, so the commit uses a dataset of its own)
    d.mutate(
        &mut [],
        |tx| {
            tx.push_element_data_rows(
                MeshPart::Facets,
                "co04_created",
                T::Segment2,
                NG,
                2,
                &[tri(0), tri(1)],
            )
        },
        None,
    )
    .unwrap();
    let m = d.read().unwrap();
    let a = m
        .mesh_facets()
        .unwrap()
        .data()
        .element_map("co04_created")
        .unwrap();
    assert_eq!(
        a.get(T::Segment2, NG).unwrap().as_slice(),
        &[tri(0), tri(1)]
    );
}

/// `push_element_list_row`: existing block, created block and dataset,
/// rollback.
#[test]
fn push_element_list_row_blocks_datasets_and_rollback() {
    let d = two_triangles();
    let before = fingerprint(&d);
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.push_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                T::Segment2,
                NG,
                vec![tri(0)],
            )?;
            tx.push_element_list_row(
                MeshPart::Facets,
                ELEMENT_TO_SUBELEMENT,
                T::Point1,
                NG,
                vec![seg(0)],
            )?;
            tx.push_element_list_row(MeshPart::Facets, "co04_list", T::Segment2, NG, vec![tri(1)])?;
            Err(injected())
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
    d.mutate(
        &mut [],
        |tx| {
            tx.push_element_list_row(
                MeshPart::Facets,
                "co04_list",
                T::Segment2,
                NG,
                vec![tri(1), tri(0)],
            )
        },
        None,
    )
    .unwrap();
    let m = d.read().unwrap();
    let a = m
        .mesh_facets()
        .unwrap()
        .data()
        .element_lists("co04_list")
        .unwrap();
    assert_eq!(
        a.get(T::Segment2, NG).unwrap().as_slice(),
        &[vec![tri(1), tri(0)]]
    );
    // a dataset of another type is an error
    drop(m);
    let before = fingerprint(&d);
    let r = d.mutate(
        &mut [],
        |tx| {
            tx.push_element_list_row(
                MeshPart::Facets,
                SUBELEMENT_TO_ELEMENT,
                T::Triangle3,
                NG,
                vec![seg(0)],
            )
        },
        None,
    );
    assert!(r.is_err());
    assert_eq!(fingerprint(&d), before);
}
