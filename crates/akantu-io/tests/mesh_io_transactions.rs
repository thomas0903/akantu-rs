// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests of
// the transactional mesh reads, each built from a failing case; the
// related upstream code is src/mesh/mesh.cc
// (`Mesh::read`, `makeReady`, `computeBoundingBox`).

//! Committing a read must not be followed by a fallible
//! collective. Before the fix the reader filled the mesh and then ran the
//! bounding-box all-reduce over the mesh communicator: with a missing or
//! failed peer the read returned `Error::Communication` with a populated,
//! partially finalised target that could not even be read again.
//!
//! Upstream `Mesh::computeBoundingBox` (`mesh.cc:415-434`) reduces only for
//! distributed meshes, and a read target is never distributed: a read is a
//! local operation (rank 0 reads, then distributes — ARCHITECTURE).
//! The port now computes the box while staging and commits with moves only
//! (`mesh_io/staging.rs`), so there is no bounding-box collective left that
//! could fail on one rank. The tests below use two-rank in-process worlds
//! whose peer never joins any collective (absent or failing on its own
//! input): each would time out, with the target already modified, if the
//! read communicated. They check the complete state with
//! `Mesh::state_fingerprint` (coordinates, flags, global ids,
//! connectivities, groups, data, bounding boxes, revision).

use std::path::PathBuf;
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, Error, GhostType};
use akantu_io::{ReadOptions, read_mesh_from_str, read_mesh_with};
use akantu_mesh::{BBox, Mesh};
use akantu_validation::fixtures_root;

/// A string input: two nodes and one physical segment.
const EDGE: &str = "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n\
                    $PhysicalNames\n1\n1 1 \"edge\"\n$EndPhysicalNames\n\
                    $Nodes\n2\n1 0 0 0\n2 1 0 0\n$EndNodes\n\
                    $Elements\n1\n1 1 2 1 1 1 2\n$EndElements\n";

fn no_side_file() -> ReadOptions {
    ReadOptions {
        write_diana_material_file: false,
        ..ReadOptions::default()
    }
}

fn diana_3d() -> PathBuf {
    fixtures_root().join("io/mesh_io/diana/diana_3d/inputs/diana_3d.diana")
}

/// Box of the mesh's node coordinates.
fn node_box(m: &Mesh) -> BBox {
    let mut b = BBox::empty(m.spatial_dimension());
    for n in 0..m.nb_nodes() {
        b.add_point(m.nodes().tuple(n));
    }
    b
}

/// String Gmsh path: rank 0 of a two-rank
/// world reads alone (the peer never takes part; 20 ms collective
/// timeout). It failed with "timed out … waiting for collective
/// AllReduce #0 from rank 1" after installing 2 nodes, the segment, the
/// mesh data and both groups. Now it succeeds without communicating and
/// equals a serial read, bounding boxes included.
#[test]
fn mio2_01_string_read_on_one_rank_never_communicates() {
    let comm = Communicator::local_world(2, Duration::from_millis(20)).remove(0);
    let mut m = Mesh::with_communicator(1, "m", comm).unwrap();
    read_mesh_from_str(
        &mut m,
        "comm.msh",
        EDGE,
        MeshIoType::Gmsh,
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(m.nb_nodes(), 2);
    assert_eq!(m.nb_element(ElementType::Segment2, GhostType::NotGhost), 1);
    assert_eq!(m.groups().element_group("edge").unwrap().nodes(), &[0, 1]);
    assert_eq!(
        m.groups().node_group("edge_nodes").unwrap().node_list(),
        vec![0, 1]
    );
    let expected = BBox {
        lower: vec![0.0],
        upper: vec![1.0],
    };
    assert_eq!((m.bbox(), m.local_bbox()), (&expected, &expected));
    m.check_consistency().unwrap();

    let mut serial = Mesh::new(1, "m").unwrap();
    read_mesh_from_str(
        &mut serial,
        "comm.msh",
        EDGE,
        MeshIoType::Gmsh,
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(m.state_fingerprint(), serial.state_fingerprint());
}

/// File Diana path (committed `diana_3d`
/// input, side file disabled): the failed read left 30 nodes, 2 element
/// groups and 3 node groups behind. Now it succeeds alone on rank 0 and
/// equals the serial read.
#[test]
fn mio2_01_file_read_on_one_rank_never_communicates() {
    let comm = Communicator::local_world(2, Duration::from_millis(20)).remove(0);
    let mut m = Mesh::with_communicator(3, "m", comm).unwrap();
    read_mesh_with(&mut m, &diana_3d(), MeshIoType::Diana, &no_side_file()).unwrap();
    assert_eq!(m.nb_nodes(), 30);
    assert_eq!(m.groups().element_groups().count(), 2);
    assert_eq!((m.bbox(), m.local_bbox()), (&node_box(&m), &node_box(&m)));
    m.check_consistency().unwrap();

    let mut serial = Mesh::new(3, "m").unwrap();
    read_mesh_with(&mut serial, &diana_3d(), MeshIoType::Diana, &no_side_file()).unwrap();
    assert_eq!(m.state_fingerprint(), serial.state_fingerprint());
}

/// Transactional reads in a running two-rank world: both ranks read their own input at
/// the same time and rank 1's read fails (malformed node line). Rank 1's
/// mesh is untouched — complete state, revision included — and the failure
/// cannot strand rank 0 in a bounding-box collective after it committed:
/// rank 0 completes with the full mesh, for both public read paths.
#[test]
fn mio2_01_peer_failure_cannot_leave_a_partial_mesh() {
    let broken = EDGE.replace("2 1 0 0\n", "2 1 zero 0\n");
    let results = run_local(2, Duration::from_millis(200), |comm| {
        let rank = comm.rank();
        let dim = if rank == 0 { 3 } else { 1 };
        let mut from_file = Mesh::with_communicator(dim, "m", comm.clone()).unwrap();
        let mut from_str = Mesh::with_communicator(1, "s", comm).unwrap();
        let before = (from_file.state_fingerprint(), from_str.state_fingerprint());
        let (file_read, str_read) = if rank == 0 {
            (
                read_mesh_with(
                    &mut from_file,
                    &diana_3d(),
                    MeshIoType::Diana,
                    &no_side_file(),
                )
                .map(|_| ()),
                read_mesh_from_str(
                    &mut from_str,
                    "ok.msh",
                    EDGE,
                    MeshIoType::Gmsh,
                    &ReadOptions::default(),
                )
                .map(|_| ()),
            )
        } else {
            (
                read_mesh_from_str(
                    &mut from_file,
                    "bad.msh",
                    &broken,
                    MeshIoType::Gmsh,
                    &ReadOptions::default(),
                )
                .map(|_| ()),
                read_mesh_from_str(
                    &mut from_str,
                    "bad.msh",
                    &broken,
                    MeshIoType::Gmsh,
                    &ReadOptions::default(),
                )
                .map(|_| ()),
            )
        };
        let after = (from_file.state_fingerprint(), from_str.state_fingerprint());
        (
            rank,
            file_read,
            str_read,
            before,
            after,
            from_file.nb_nodes(),
        )
    });
    for (rank, file_read, str_read, before, after, nb_nodes) in results {
        if rank == 0 {
            file_read.unwrap();
            str_read.unwrap();
            assert_eq!(nb_nodes, 30);
            assert_ne!(after.0, before.0);
            assert_ne!(after.1, before.1);
        } else {
            for r in [file_read, str_read] {
                assert!(matches!(r, Err(Error::Parse { .. })), "{r:?}");
            }
            assert_eq!(after, before, "rank 1: a failed read changed the mesh");
        }
    }
}

/// After a failed read the same target can be read again (before
/// the fix a failed final step left it non-empty, refused by the reader's
/// empty-target check).
#[test]
fn mio2_01_failed_read_can_be_retried() {
    let comm = Communicator::local_world(2, Duration::from_millis(20)).remove(0);
    let mut m = Mesh::with_communicator(1, "m", comm).unwrap();
    let before = m.state_fingerprint();
    let broken = EDGE.replace("1 1 2 1 1 1 2", "1 1 2 1 1 1 3");
    let e = read_mesh_from_str(
        &mut m,
        "x.msh",
        &broken,
        MeshIoType::Gmsh,
        &ReadOptions::default(),
    )
    .unwrap_err();
    assert!(!matches!(e, Error::Communication(_)), "{e}");
    assert_eq!(m.state_fingerprint(), before);
    read_mesh_from_str(
        &mut m,
        "x.msh",
        EDGE,
        MeshIoType::Gmsh,
        &ReadOptions::default(),
    )
    .unwrap();
    assert_eq!(m.nb_nodes(), 2);
}

/// Reading a mesh with no nodes reports upstream's empty
/// bounds (±`DBL_MAX`, `aka_bbox.hh:45-49,125-126`), not ±∞.
#[test]
fn empty_read_reports_upstream_empty_bounds() {
    let msh =
        "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n0\n$EndNodes\n$Elements\n0\n$EndElements\n";
    let mut mesh = Mesh::new(3, "m").unwrap();
    read_mesh_from_str(
        &mut mesh,
        "empty.msh",
        msh,
        MeshIoType::Gmsh,
        &no_side_file(),
    )
    .expect("an empty mesh reads");
    let b = mesh.bbox();
    assert!(b.lower.iter().all(|&l| l.to_bits() == f64::MAX.to_bits()));
    assert!(
        b.upper
            .iter()
            .all(|&u| u.to_bits() == (-f64::MAX).to_bits())
    );
}
