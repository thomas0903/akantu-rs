// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports of the
// upstream mesh-utility tests (same intent, same names):
// test/test_mesh_utils/test_buildfacets/test_buildfacets.cc (+ .verified),
// test/test_mesh_utils/test_purify_mesh.cc,
// test/test_mesh_utils/test_mesh_iterators.cc,
// test/test_mesh_utils/test_mesh_periodic/test_mesh_periodic.cc (mesh part),
// test/test_fe_engine/test_facet_element_mapping.cc,
// test/test_fe_engine/test_mesh_boundary.cc.
//
// The input meshes are the copies kept with the oracle fixtures
// (`fixtures/mesh_utils/<case>/inputs/`), read with the akantu-io readers.

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::path::PathBuf;

use akantu_core::enums::{MeshIoType, ReduceOperation, SpatialDirection};
use akantu_core::{Array, Element, ElementType, GhostType, Idx, TypeFilter};
use akantu_io::{ReadOptions, read_mesh_with, write_mesh};
use akantu_mesh::Mesh;
use akantu_mesh::periodic::{make_consistent_for_periodicity_real, make_periodic, periodic_pairs};
use akantu_mesh::utils::{
    ELEMENT_TO_SUBELEMENT, SUBELEMENT_TO_ELEMENT, fill_element_to_subelements_data,
    init_mesh_facets, purify_mesh,
};
use akantu_validation::Fixture;

fn input(case: &str, role: &str) -> PathBuf {
    Fixture::open_by_id(&format!("mesh_utils/{case}"))
        .unwrap()
        .input_path(role)
        .unwrap()
}

/// `Mesh::read` (upstream parity options) + `fillElementToSubElementsData`.
fn read(case: &str, dim: usize) -> Mesh {
    let mut m = Mesh::new(dim, "mesh").unwrap();
    let opts = ReadOptions {
        fill_unnamed_physical_groups: false,
        write_diana_material_file: false,
    };
    read_mesh_with(&mut m, &input(case, "mesh"), MeshIoType::Auto, &opts).unwrap();
    fill_element_to_subelements_data(&mut m).unwrap();
    m
}

// ------------------------------------------------------------ test_buildfacets

/// `print_connection(name, type, list)` of the upstream fixture.
fn print_connection(out: &mut String, name: &str, ty: ElementType, rows: &[Vec<Element>]) {
    let _ = writeln!(out, "{name}");
    for (i, row) in rows.iter().enumerate() {
        let _ = write!(out, "{ty} {i} connected to ");
        for e in row {
            let _ = write!(out, "{e}, ");
        }
        let _ = writeln!(out, " ");
    }
}

fn e2s_rows(f: &Mesh, ty: ElementType) -> Vec<Vec<Element>> {
    f.data()
        .element_lists(ELEMENT_TO_SUBELEMENT)
        .unwrap()
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .to_vec()
}

fn s2e_rows(f: &Mesh, ty: ElementType) -> Vec<Vec<Element>> {
    let a = f
        .data()
        .element_map(SUBELEMENT_TO_ELEMENT)
        .unwrap()
        .get(ty, GhostType::NotGhost)
        .unwrap();
    a.tuples().map(<[Element]>::to_vec).collect()
}

/// Body of `TYPED_TEST(TestBuildFacetFixture, Buildfacets)`.
fn buildfacets_output(type1: ElementType, type2: Option<ElementType>) -> String {
    let dim = type1.spatial_dimension();
    let name = format!("{type1}{}", type2.map_or(String::new(), |t| t.to_string()));
    let mut mesh = read(&format!("buildfacets/{name}"), dim);
    let _ = init_mesh_facets(&mut mesh).unwrap();
    let f = mesh.mesh_facets().unwrap();
    let mut types_facet: BTreeSet<ElementType> =
        type1.facet_types().unwrap().iter().copied().collect();
    if let Some(t2) = type2 {
        types_facet.extend(t2.facet_types().unwrap().iter().copied());
    }
    let mut out = String::new();
    for &tf in &types_facet {
        print_connection(&mut out, "ElementToSubelement1", tf, &e2s_rows(f, tf));
    }
    let first = *types_facet.iter().next().unwrap();
    let type_subfacet = first.facet_type(0).unwrap();
    print_connection(
        &mut out,
        "ElementToSubelement2",
        type_subfacet,
        &e2s_rows(f, type_subfacet),
    );
    if dim == 3 {
        let sub2 = type_subfacet.facet_type(0).unwrap();
        print_connection(&mut out, "ElementToSubelement3", sub2, &e2s_rows(f, sub2));
    }
    out.push('\n');
    print_connection(&mut out, "SubelementToElement1", type1, &s2e_rows(f, type1));
    if let Some(t2) = type2 {
        print_connection(&mut out, "SubelementToElement1", t2, &s2e_rows(f, t2));
    }
    for &tf in &types_facet {
        print_connection(&mut out, "SubelementToElement2", tf, &s2e_rows(f, tf));
    }
    if dim == 3 {
        print_connection(
            &mut out,
            "SubelementToElement3",
            type_subfacet,
            &s2e_rows(f, type_subfacet),
        );
    }
    out
}

/// Upstream `test_buildfacets` (gtest, 13 typed cases): the printed
/// element ↔ sub-element connections of every mesh equal the
/// `test_buildfacets_gtest.verified` output (confirmed against the strict
/// oracle by O1), line for line.
#[test]
fn test_buildfacets() {
    use ElementType as T;
    let types: [(T, Option<T>); 13] = [
        (T::Triangle3, None),
        (T::Triangle6, None),
        (T::Quadrangle4, None),
        (T::Quadrangle8, None),
        (T::Tetrahedron10, None),
        (T::Hexahedron8, None),
        (T::Hexahedron20, None),
        (T::Pentahedron6, None),
        (T::Pentahedron15, None),
        (T::Quadrangle4, Some(T::Triangle3)),
        (T::Quadrangle8, Some(T::Triangle6)),
        (T::Hexahedron8, Some(T::Pentahedron6)),
        (T::Hexahedron20, Some(T::Pentahedron15)),
    ];
    let verified = std::fs::read_to_string(input("buildfacets/_triangle_3", "verified")).unwrap();
    // the body of each test: lines between "[ RUN      ]" and "[       OK ]"
    let mut sections: Vec<String> = Vec::new();
    let mut current: Option<String> = None;
    for line in verified.lines() {
        if line.starts_with("[ RUN      ]") {
            current = Some(String::new());
        } else if line.starts_with("[       OK ]") {
            sections.push(current.take().unwrap());
        } else if let Some(c) = current.as_mut() {
            c.push_str(line);
            c.push('\n');
        }
    }
    assert_eq!(sections.len(), 13);
    for (i, ((t1, t2), expected)) in types.iter().zip(&sections).enumerate() {
        let got = buildfacets_output(*t1, *t2);
        assert_eq!(
            &got, expected,
            "TestBuildFacetFixture/{i} ({t1} {t2:?}) differs"
        );
    }
}

// ---------------------------------------------------------- test_purify_mesh

/// Upstream `test_purify_mesh`: after `purifyMesh` the mesh has 21 nodes
/// and 4 `_quadrangle_8`; the mesh is written (`purify_mesh_after.msh`)
/// and, read back, is unchanged.
#[test]
fn test_purify_mesh() {
    let mut mesh = read("purify/purify_mesh", 2);
    let before = mesh.nb_nodes();
    let ev = purify_mesh(&mut mesh).unwrap();
    assert_eq!(
        mesh.nb_nodes(),
        21,
        "the purified mesh does not contain the good number of nodes"
    );
    assert_eq!(
        mesh.nb_element(ElementType::Quadrangle8, GhostType::NotGhost),
        4
    );
    assert_eq!(ev.events().len(), 1);
    mesh.check_consistency().unwrap();
    assert!(before > 21);
    let dir = std::env::temp_dir().join(format!("akantu-purify-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let out = dir.join("purify_mesh_after.msh");
    write_mesh(&mesh, &out, MeshIoType::Gmsh).unwrap();
    let mut back = Mesh::new(2, "back").unwrap();
    read_mesh_with(&mut back, &out, MeshIoType::Gmsh, &ReadOptions::default()).unwrap();
    assert_eq!(back.nodes().as_slice(), mesh.nodes().as_slice());
    assert_eq!(
        back.connectivity(ElementType::Quadrangle8, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        mesh.connectivity(ElementType::Quadrangle8, GhostType::NotGhost)
            .unwrap()
            .as_slice()
    );
    let _ = std::fs::remove_dir_all(dir);
}

// ------------------------------------------------------- test_mesh_iterators

/// Upstream `test_mesh_iterators`: iterate the element groups (name,
/// dimension), the node groups and the enumerated element groups of
/// `iterators_mesh.msh`; the listing equals the oracle's (fixture
/// `mesh_utils/frozen/iterators_mesh`, parameters `read/*`).
#[test]
fn test_mesh_iterators() {
    let mesh = read("frozen/iterators_mesh", 3);
    let mut out = String::from("ElementGroups\n");
    for g in mesh.groups().element_groups() {
        let _ = writeln!(out, "{} {}", g.name(), g.dimension);
    }
    out.push_str("NodeGroups\n");
    for g in mesh.groups().node_groups() {
        let _ = writeln!(out, "{}", g.name());
    }
    out.push_str("enumerate(ElementGroups)\n");
    for (i, g) in mesh.groups().element_groups().enumerate() {
        let _ = writeln!(out, "{i} {}", g.name());
    }
    let fx = Fixture::open_by_id("mesh_utils/frozen/iterators_mesh").unwrap();
    let p = &fx.manifest().parameters;
    let mut expected = String::from("ElementGroups\n");
    let groups = p["read/element_groups"].as_array().unwrap();
    for g in groups {
        let _ = writeln!(
            expected,
            "{} {}",
            g["name"].as_str().unwrap(),
            g["dimension"]
        );
    }
    expected.push_str("NodeGroups\n");
    for n in p["read/node_groups"].as_array().unwrap() {
        let _ = writeln!(expected, "{}", n.as_str().unwrap());
    }
    expected.push_str("enumerate(ElementGroups)\n");
    for (i, g) in groups.iter().enumerate() {
        let _ = writeln!(expected, "{i} {}", g["name"].as_str().unwrap());
    }
    assert_eq!(out, expected);
    assert!(!groups.is_empty());
}

// -------------------------------------------------------- test_mesh_periodic

fn periodic_mesh(dim: usize, dim_periodic: usize) -> Mesh {
    let case = match dim {
        1 => "periodic/line_x",
        2 => "periodic/square_x",
        _ => "periodic/cube_x",
    };
    let mut m = read(case, dim);
    for d in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ]
    .into_iter()
    .take(dim_periodic)
    {
        let _ = make_periodic(&mut m, d, None).unwrap();
    }
    m
}

const PERIODIC_TYPES: [(usize, usize); 6] = [(1, 1), (2, 1), (2, 2), (3, 1), (3, 2), (3, 3)];

/// Upstream `TestMeshPeriodic.SimpleMakePeriodic` (6 typed cases): the mesh
/// is periodic, and (dump-field check of the fixture) no slave is the
/// master of another node.
#[test]
fn test_mesh_periodic_simple_make_periodic() {
    for (dim, dp) in PERIODIC_TYPES {
        let m = periodic_mesh(dim, dp);
        assert!(m.is_periodic(), "({dim}, {dp})");
        let pairs = periodic_pairs(&m);
        assert!(!pairs.is_empty());
        let masters: BTreeSet<Idx> = pairs.iter().map(|p| p.0).collect();
        for (_, s) in &pairs {
            assert!(
                !masters.contains(s),
                "({dim}, {dp}): {s} is slave and master"
            );
        }
    }
}

/// Upstream `MakeConsistentForPeriodicityIdentity`: slaves set to −1 get
/// their master's value 10 back.
#[test]
fn test_mesh_periodic_make_consistent_identity() {
    for (dim, dp) in PERIODIC_TYPES {
        let m = periodic_mesh(dim, dp);
        let mut v = Array::filled(m.nb_nodes(), 1, 10.0);
        for n in 0..m.nb_nodes() {
            if m.is_periodic_slave(Idx::try_from(n).unwrap()).unwrap() {
                v.as_mut_slice()[n] = -1.0;
            }
        }
        make_consistent_for_periodicity_real(&m, &mut v, ReduceOperation::Id).unwrap();
        assert!(v.as_slice().iter().all(|&x| x == 10.0), "({dim}, {dp})");
    }
}

/// Upstream `MakeConsistentForPeriodicitySum`: masters hold
/// `100 / 2^shared` (shared = number of periodic boundary groups of the
/// node); `_id` then `_sum` gives 100 on every local or master node
/// (upstream tolerance 1e-7; the values here are exact dyadic sums).
#[test]
fn test_mesh_periodic_make_consistent_sum() {
    for (dim, dp) in PERIODIC_TYPES {
        let m = periodic_mesh(dim, dp);
        let n = m.nb_nodes();
        let mut shared = vec![0.0f64; n];
        for g in [
            "x-minus", "x-plus", "y-minus", "y-plus", "z-minus", "z-plus",
        ]
        .into_iter()
        .take(2 * dp)
        {
            for &node in m.groups().element_group(g).unwrap().nodes().as_slice() {
                shared[usize::try_from(node).unwrap()] += 1.0;
            }
        }
        let mut v = Array::filled(n, 1, 100.0);
        let masters: BTreeSet<Idx> = periodic_pairs(&m).iter().map(|p| p.0).collect();
        for &ma in &masters {
            if m.is_local_or_master_node(ma).unwrap() {
                let u = usize::try_from(ma).unwrap();
                v.as_mut_slice()[u] = 100.0 / 2f64.powf(shared[u]);
            }
        }
        make_consistent_for_periodicity_real(&m, &mut v, ReduceOperation::Id).unwrap();
        make_consistent_for_periodicity_real(&m, &mut v, ReduceOperation::Sum).unwrap();
        for (i, &x) in v.as_slice().iter().enumerate() {
            if m.is_local_or_master_node(Idx::try_from(i).unwrap())
                .unwrap()
            {
                assert!((x - 100.0).abs() <= 1e-7, "({dim}, {dp}) node {i}: {x}");
            }
        }
    }
}

// ------------------------------------------- test_facet_element_mapping (fe)

/// Upstream `test_facet_element_mapping` (print-only upstream): the
/// element ↔ sub-element data the reader leaves on `cube_physical_names`
/// is printed for both ghost types; here the printed relations are checked
/// to be mutually consistent (each facet lists the tetrahedra that list
/// it, without `ElementNull` entries, at most two per facet).
#[test]
fn test_facet_element_mapping() {
    let mesh = read("frozen/cube_physical_names", 3);
    let e2s = mesh.data().element_lists(ELEMENT_TO_SUBELEMENT).unwrap();
    let s2e = mesh.data().element_map(SUBELEMENT_TO_ELEMENT).unwrap();
    let mut nb_links = 0;
    for ty in mesh.element_types(TypeFilter::new().dim(2)) {
        let rows = e2s.get(ty, GhostType::NotGhost).unwrap();
        assert_eq!(rows.len(), mesh.nb_element(ty, GhostType::NotGhost));
        for (f, list) in rows.as_slice().iter().enumerate() {
            let facet = Element::new(ty, Idx::try_from(f).unwrap(), GhostType::NotGhost);
            assert!(!list.is_empty() && list.len() <= 2, "{facet}: {list:?}");
            for &el in list {
                assert!(!el.is_null());
                assert!(
                    s2e.element(el).unwrap().contains(&facet),
                    "{el} does not list {facet}"
                );
                nb_links += 1;
            }
        }
    }
    let mut back = 0;
    for ty in mesh.element_types(TypeFilter::new().dim(3)) {
        let a = s2e.get(ty, GhostType::NotGhost).unwrap();
        assert_eq!(a.nb_component(), ty.nb_facets_per_element());
        for (e, row) in a.tuples().enumerate() {
            let el = Element::new(ty, Idx::try_from(e).unwrap(), GhostType::NotGhost);
            for &f in row.iter().filter(|f| !f.is_null()) {
                assert!(e2s.element(f).unwrap()[0].contains(&el));
                back += 1;
            }
        }
    }
    assert_eq!(nb_links, back);
    assert!(nb_links > 0);
}

/// Upstream `test_mesh_boundary`: `cube_physical_names` advertises
/// boundary element groups of dimension 2, printed through the group
/// manager.
#[test]
fn test_mesh_boundary() {
    let mesh = read("frozen/cube_physical_names", 3);
    let nb = mesh.groups().nb_element_groups(Some(2));
    assert!(nb > 0, "No boundary detected!");
    let text = mesh.groups().describe(0);
    assert!(text.starts_with("GroupManager ["));
    assert_eq!(
        text.matches("ElementGroup [").count(),
        mesh.groups().nb_element_groups(None)
    );
}
