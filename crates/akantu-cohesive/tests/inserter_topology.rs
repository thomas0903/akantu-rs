// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Topology tests of
// the cohesive inserter (MUTIL-015/016, COH-005).

use std::path::PathBuf;

use akantu_cohesive::CohesiveElementInserter;
use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, GhostType};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_validation::fixtures_root;

fn mesh_path(rel: &str) -> PathBuf {
    fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_cohesive")
        .join(rel)
}

fn read(rel: &str, dim: usize) -> SharedMesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, &mesh_path(rel), MeshIoType::Gmsh).unwrap();
    let shared = SharedMesh::new(mesh);
    shared.setup(akantu_mesh::utils::init_mesh_facets).unwrap();
    shared
}

fn insert_surface(shared: &SharedMesh, surface: &str) -> akantu_cohesive::InsertionResult {
    let mut inserter = {
        let m = shared.read().unwrap();
        CohesiveElementInserter::new(&m, "inserter").unwrap()
    };
    inserter.add_physical_surface(surface);
    {
        let m = shared.read().unwrap();
        inserter.flag_intrinsic_facets(&m).unwrap();
    }
    let mut result = None;
    shared
        .mutate(
            &mut [],
            |tx| {
                result = Some(inserter.insert_elements_in(tx, false)?);
                Ok(())
            },
            None,
        )
        .unwrap();
    let m = shared.read().unwrap();
    inserter.update_insertion_facets(&m).unwrap();
    result.unwrap()
}

fn check(shared: &SharedMesh, cohesive: ElementType, expected: usize) {
    let m = shared.read().unwrap();
    assert_eq!(m.nb_element(cohesive, GhostType::NotGhost), expected);
    m.check_consistency().unwrap();
    akantu_mesh::utils::check_facet_mesh_consistency(&m).unwrap();
}

#[test]
fn intrinsic_insertion_2d_triangle() {
    let shared = read("_cohesive_2d_4_triangle_3.msh", 2);
    let nb_nodes = shared.read().unwrap().nb_nodes();
    let group = {
        let m = shared.read().unwrap();
        m.mesh_facets()
            .unwrap()
            .groups()
            .element_group("insertion")
            .unwrap()
            .elements_of(ElementType::Segment2, GhostType::NotGhost)
            .len()
    };
    let r = insert_surface(&shared, "insertion");
    assert_eq!(r.new_elements.len(), group);
    check(&shared, ElementType::Cohesive2d4, group);
    // a straight line through the domain: one more node than facets is
    // doubled when the line ends on the boundary on both sides
    assert_eq!(
        shared.read().unwrap().nb_nodes(),
        nb_nodes + r.doubled_nodes.len()
    );
    eprintln!("facets {group}, doubled nodes {}", r.doubled_nodes.len());
}
