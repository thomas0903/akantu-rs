// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_mesh_utils/test_mesh_partitionate/
// {test_mesh_partitionate_mesh_data.cc, test_mesh_partitionate_scotch.cc}.

//! Upstream `test_mesh_partitionate_mesh_data`: `quad.msh` (frozen), 8 parts
//! assigned by the element barycentre's x coordinate (`round(8x)` when within
//! 10 ε of an integer, else `floor(8x)`), partitioned with
//! `MeshPartitionMeshData`; every element keeps its assigned part (the
//! upstream dumps are not reproduced). The Scotch variant
//! (`test_mesh_partitionate_scotch`: 8 parts of `triangle.msh`, a dump and a
//! node reordering, no assertion) runs here with the bisection partitioner
//! for the partition invariants; Scotch itself is checked against the oracle
//! in `parallel_oracle.rs` (feature `scotch`).

use akantu_core::enums::MeshIoType;
use akantu_core::{Element, ElementTypeMapArray, GhostType, Real, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_with};
use akantu_mesh::Mesh;
use akantu_mesh::partition::{MeshDataPartition, MeshPartitioner, RcbPartitioner};

fn read(rel: &str) -> Mesh {
    let mut m = Mesh::new(2, "mesh").unwrap();
    let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_partitionate")
        .join(rel);
    let opts = ReadOptions {
        fill_unnamed_physical_groups: false,
        write_diana_material_file: false,
    };
    read_mesh_with(&mut m, &path, MeshIoType::Auto, &opts).unwrap();
    m
}

#[test]
fn test_mesh_partitionate_mesh_data() {
    let m = read("quad.msh");
    let nb_partitions = 8usize;
    let mut mapping = ElementTypeMapArray::new("partition");
    for ty in m.element_types(TypeFilter::new().dim(2)) {
        let n = m.nb_element(ty, GhostType::NotGhost);
        let a = mapping.alloc(n, 1, ty, GhostType::NotGhost, 0).unwrap();
        for i in 0..n {
            let b = m
                .barycenter(Element::try_new(ty, i, GhostType::NotGhost).unwrap())
                .unwrap();
            let real_proc = b[0] * nb_partitions as Real;
            a.as_mut_slice()[i] = if (real_proc - real_proc.round()).abs() < 10.0 * Real::EPSILON {
                real_proc.round() as i32
            } else {
                real_proc.floor() as i32
            };
        }
    }
    let reference = mapping.clone();
    let p = MeshDataPartition::new(mapping)
        .partition(&m, nb_partitions)
        .unwrap();
    for ty in m.element_types(TypeFilter::new().dim(2)) {
        assert_eq!(
            p.partition(ty).unwrap(),
            reference.get(ty, GhostType::NotGhost).unwrap().as_slice(),
            "Incorrect partitioning ({ty})"
        );
    }
}

#[test]
fn test_mesh_partitionate_scotch_invariants_with_bisection() {
    let m = read("triangle.msh");
    let p = RcbPartitioner.partition(&m, 8).unwrap();
    let lin = p.linearized();
    assert_eq!(lin.len(), m.nb_element_filtered(TypeFilter::new().dim(2)));
    for part in 0..8 {
        assert!(lin.contains(&part), "part {part} is empty");
    }
    // every lower-dimensional element has a part and ghost parts exclude it
    for ty in m.element_types(TypeFilter::new()) {
        let parts = p.partition(ty).unwrap();
        let ghosts = p.ghost_partition(ty).unwrap();
        for (e, &own) in parts.iter().enumerate() {
            assert!((0..8).contains(&own));
            assert!(!ghosts.row(e).contains(&own));
        }
    }
}
