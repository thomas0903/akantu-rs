// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential tests
// of the mesh readers against the oracle fixtures `fixtures/mesh_io/*`
// (every `test/**/*.msh` of the reference read by Akantu's `Mesh::read`,
// plus the frozen Gmsh-generated meshes, D-009).

mod common;

use akantu_core::enums::MeshIoType;
use akantu_io::ReadOptions;
use akantu_validation::Fixture;

/// Committed `fixtures/mesh_io` cases; external cases are not counted
/// (they are compared only when materialised).
const COMMITTED_MESH_IO_CASES: usize = 218;

/// Every `mesh_io` fixture: connectivities, node coordinates (bitwise),
/// bounds, mesh data (`tag_*`, `physical_names`), node and element groups
/// equal the oracle's. Committed fixtures must all be present; external
/// fixtures that are not materialised locally are counted, listed with
/// their regeneration command and skipped (`common::CaseSet`).
#[test]
fn every_mesh_io_fixture_matches_the_oracle() {
    let test = "every_mesh_io_fixture_matches_the_oracle";
    let cases = common::CaseSet::new(&common::mesh_io_cases());
    cases.check_and_announce(test);
    assert!(
        cases.committed() >= COMMITTED_MESH_IO_CASES,
        "expected ≥ {COMMITTED_MESH_IO_CASES} committed mesh_io fixtures, found {}",
        cases.committed()
    );
    // Oracle parity: upstream leaves `msh_block_*` groups empty (UD-045).
    let opts = ReadOptions {
        fill_unnamed_physical_groups: false,
        write_diana_material_file: false,
    };
    let mut failures = Vec::new();
    for fx in &cases.materialized {
        let mesh = common::read_fixture_mesh(fx, &opts, MeshIoType::Auto);
        if let Err(e) = common::compare_with_fixture(test, fx.dir(), &mesh) {
            failures.push(format!("{}:\n{e}", fx.case_id()));
        }
    }
    eprintln!(
        "{test}: {} of {} cases compared ({} external skipped)",
        cases.materialized.len(),
        cases.len(),
        cases.external_missing.len()
    );
    assert!(
        failures.is_empty(),
        "{} of {} cases differ:\n{}",
        failures.len(),
        cases.materialized.len(),
        failures.join("\n\n")
    );
}

/// The comparison is not vacuous: one-ulp coordinate changes, a swapped
/// connectivity entry, a changed tag, a renamed group and a missing group
/// node are all detected.
#[test]
fn fixture_comparison_detects_differences() {
    use akantu_core::{ElementType, GhostType};
    use akantu_mesh::{MeshAccessor, MeshDataValues};
    let dir = akantu_validation::fixtures_root().join("mesh_io/test_io/test_dumper/test_dumper");
    let fx = Fixture::open(&dir).unwrap();
    let opts = ReadOptions::default();
    let fresh = || common::read_fixture_mesh(&fx, &opts, MeshIoType::Gmsh);
    let name = "fixture_comparison_detects_differences";
    common::compare_with_fixture(name, &dir, &fresh()).unwrap();

    let mut m = fresh();
    {
        let mut acc = MeshAccessor::new(&mut m);
        let mut nodes = acc.nodes_mut();
        let x = &mut nodes.as_mut_slice()[5];
        *x = f64::from_bits(x.to_bits() + 1);
    }
    assert!(
        common::compare_with_fixture(name, &dir, &m).is_err(),
        "1 ulp"
    );

    let mut m = fresh();
    {
        let mut acc = MeshAccessor::new(&mut m);
        let mut c = acc.connectivity_mut(ElementType::Hexahedron8, GhostType::NotGhost);
        c.as_mut_slice().swap(0, 1);
    }
    assert!(
        common::compare_with_fixture(name, &dir, &m).is_err(),
        "connectivity"
    );

    let mut m = fresh();
    if let Ok(MeshDataValues::Int(t)) = m.data_mut().elemental_mut("tag_1") {
        t.get_mut(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap()
            .as_mut_slice()[3] += 1;
    }
    assert!(common::compare_with_fixture(name, &dir, &m).is_err(), "tag");

    let mut m = fresh();
    if let Ok(MeshDataValues::String(t)) = m.data_mut().elemental_mut("physical_names") {
        t.get_mut(ElementType::Quadrangle4, GhostType::NotGhost)
            .unwrap()
            .as_mut_slice()[0] = "Top".into();
    }
    assert!(
        common::compare_with_fixture(name, &dir, &m).is_err(),
        "physical name"
    );

    let mut m = fresh();
    {
        let g = m.groups_mut().element_group_mut("Top").unwrap();
        let mut v = g.node_group.node_list();
        v.pop();
        g.node_group.set_nodes(v);
    }
    assert!(
        common::compare_with_fixture(name, &dir, &m).is_err(),
        "group nodes"
    );

    let mut m = fresh();
    m.groups_mut().create_element_group("extra", 2).unwrap();
    assert!(
        common::compare_with_fixture(name, &dir, &m).is_err(),
        "extra group"
    );
}
