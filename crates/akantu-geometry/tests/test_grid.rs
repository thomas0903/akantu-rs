// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_common/test_grid.cc
// (SpatialGrid<Element> of the element barycenters of circle.msh, printed
// and saved as a mesh).

//! Port of `test_grid.cc` (TEST-001, CORE-030) compared with the oracle
//! replay `fixtures/core/common/grid_circle` (oracle/probes/core): read the
//! frozen `circle.msh`, insert every triangle's barycenter into a grid of
//! spacing 0.2 centred on the local bounding box, print it and save it as a
//! mesh. Upstream only prints (no assertion) and writes `grid.msh`; here the
//! printed text, the cells, their contents and neighbours, the bounds and the
//! saved mesh (nodes, connectivity, `tag_1`) must all be identical to the
//! oracle's.

use std::collections::BTreeSet;
use std::path::PathBuf;

use akantu_core::enums::MeshIoType;
use akantu_core::{Element, ElementType, GhostType, Real, TypeFilter};
use akantu_geometry::{SpatialGrid, save_grid_as_mesh};
use akantu_io::read_mesh;
use akantu_mesh::Mesh;
use akantu_validation::{DiffTest, fixtures_root};

fn circle_path() -> PathBuf {
    fixtures_root().join("meshes/gmsh-4.13.1/test/test_common/circle.msh")
}

#[test]
fn test_grid() {
    let mut t = DiffTest::new("test_grid", "core/common/grid_circle").unwrap();
    let mut seen = BTreeSet::new();
    let spatial_dimension = 2;
    let mut circle = Mesh::new(spatial_dimension, "mesh").unwrap();
    read_mesh(&mut circle, &circle_path(), MeshIoType::Gmsh).unwrap();

    let l = circle.local_bbox().lower.clone();
    let u = circle.local_bbox().upper.clone();
    let s = [0.2, 0.2];
    let c: Vec<Real> = u.iter().zip(&l).map(|(u, l)| (u + l) / 2.0).collect();
    let mut grid: SpatialGrid<Element> =
        SpatialGrid::with_spacing(spatial_dimension, &s, &c).unwrap();

    let mut barys = Vec::new();
    let types: Vec<ElementType> = circle
        .element_types(TypeFilter::new().dim(spatial_dimension))
        .collect();
    for &ty in &types {
        for e in 0..circle.nb_element(ty, GhostType::NotGhost) {
            let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
            let bary = circle.barycenter(el).unwrap();
            barys.extend_from_slice(&bary);
            grid.insert(el, &bary).unwrap();
        }
    }
    let p = t.fixture().manifest().parameters.clone();
    let ref_types: Vec<String> = p["element_types"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert_eq!(
        types.iter().map(ToString::to_string).collect::<Vec<_>>(),
        ref_types
    );
    let mut f64q = |t: &mut DiffTest, q: &str, a: &[f64]| {
        seen.insert(q.to_owned());
        t.check_f64(q, a);
    };
    f64q(&mut t, "center", &c);
    f64q(&mut t, "barycenters", &barys);
    f64q(&mut t, "lower_bounds", grid.lower_bounds());
    f64q(&mut t, "upper_bounds", grid.upper_bounds());

    // `std::cout << grid << std::endl`
    let printed = format!("{}\n", grid.print_self("akantu::Element", 0));
    assert_eq!(printed, p["printself"].as_str().unwrap());

    let mut ids = Vec::new();
    let mut offsets = vec![0i64];
    let mut items = Vec::new();
    let mut neighbors = Vec::new();
    for (id, cell) in grid.cells() {
        ids.extend(id.ids().iter().map(|&i| i64::from(i)));
        items.extend(cell.iter().map(|e| i64::from(e.index())));
        offsets.push(i64::try_from(items.len()).unwrap());
        for n in id.neighbors() {
            neighbors.extend(n.ids().iter().map(|&i| i64::from(i)));
        }
    }
    let mut i64q = |t: &mut DiffTest, q: &str, a: &[i64]| {
        seen.insert(q.to_owned());
        t.check_i64(q, a);
    };
    i64q(&mut t, "cells/ids", &ids);
    i64q(&mut t, "cells/offsets", &offsets);
    i64q(&mut t, "cells/items", &items);
    i64q(&mut t, "cells/neighbors", &neighbors);

    // `grid.saveAsMesh(mesh)` into a new mesh
    let mut mesh = Mesh::new(spatial_dimension, "save").unwrap();
    save_grid_as_mesh(&grid, &mut mesh).unwrap();
    let ty = ElementType::Quadrangle4;
    let nodes = mesh.nodes().as_slice().to_vec();
    let conn: Vec<i64> = mesh
        .connectivity(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .iter()
        .map(|&i| i64::from(i))
        .collect();
    let tags: Vec<i64> = mesh
        .data()
        .ints("tag_1")
        .unwrap()
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .iter()
        .map(|&i| i64::from(i))
        .collect();
    i64q(&mut t, "mesh/connectivity", &conn);
    i64q(&mut t, "mesh/tag_1", &tags);
    seen.insert("mesh/nodes".to_owned());
    t.check_f64("mesh/nodes", &nodes);
    mesh.check_consistency().unwrap();
    // the exact rule ignores the sign of zero: bounds and saved nodes are
    // also compared bit for bit
    for (q, mine) in [
        ("lower_bounds", grid.lower_bounds()),
        ("upper_bounds", grid.upper_bounds()),
        ("mesh/nodes", nodes.as_slice()),
    ] {
        let r = t.reference_f64(q).unwrap().data;
        assert_eq!(r.len(), mine.len(), "{q} length");
        for (k, (a, b)) in r.iter().zip(mine).enumerate() {
            // D-046 (bit identity, or default NaNs of either sign)
            assert!(
                akantu_validation::exact_eq(*a, *b),
                "{q}[{k}] bits: oracle {a:e}, port {b:e}"
            );
        }
    }

    let all: BTreeSet<String> = t
        .fixture()
        .quantity_names("*")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let missing: Vec<_> = all.difference(&seen).collect();
    assert!(missing.is_empty(), "unchecked quantities: {missing:?}");
    t.assert_pass();
}

/// `save_grid_as_mesh` appends to a mesh that already has nodes and
/// elements of the grid's element type (upstream `getDataPointer` sizes
/// `tag_1` to the existing elements, with value 0).
#[test]
fn save_grid_appends_to_an_existing_mesh() {
    let mut grid: SpatialGrid<u8> = SpatialGrid::with_spacing(2, &[1.0, 1.0], &[0.0, 0.0]).unwrap();
    grid.insert(0, &[0.5, 0.5]).unwrap();
    grid.insert(1, &[-0.5, 2.5]).unwrap();
    let mut mesh = Mesh::new(2, "m").unwrap();
    save_grid_as_mesh(&grid, &mut mesh).unwrap();
    save_grid_as_mesh(&grid, &mut mesh).unwrap();
    let ty = ElementType::Quadrangle4;
    assert_eq!(mesh.nb_nodes(), 16);
    let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
    assert_eq!(conn.tuple(2), &[8, 11, 9, 10]);
    let tags = mesh.data().ints("tag_1").unwrap();
    assert_eq!(
        tags.get(ty, GhostType::NotGhost).unwrap().as_slice(),
        &[0, 1, 0, 1]
    );
    // dimension mismatch: refused, mesh unchanged
    let mut m3 = Mesh::new(3, "m3").unwrap();
    assert!(save_grid_as_mesh(&grid, &mut m3).is_err());
    assert_eq!(m3.nb_nodes(), 0);
}

/// D-048: the grid re-exported by `akantu-geometry` is
/// the core grid with the access upstream's `GridSynchronizer` and
/// `ContactDetector` use (per-direction cell id, cell ids, ranges of cells,
/// neighbours, cell contents).
#[test]
fn reexported_grid_exposes_the_synchronizer_and_contact_access() {
    use akantu_geometry::CellId;
    let el = |i| Element::new(ElementType::Triangle3, i, GhostType::NotGhost);
    let mut g: SpatialGrid<Element> =
        SpatialGrid::with_spacing(2, &[0.5, 0.5], &[0.0, 0.0]).unwrap();
    g.insert(el(0), &[0.1, 0.1]).unwrap();
    g.insert(el(1), &[0.6, 0.1]).unwrap();
    g.insert(el(2), &[-0.4, -0.4]).unwrap();
    // grid synchroniser: getCellID(Real, Int) on box bounds, then the range
    let first = CellId::new(vec![
        g.cell_index(-0.1, 0).unwrap(),
        g.cell_index(0.0, 1).unwrap(),
    ])
    .unwrap();
    let last = CellId::new(vec![
        g.cell_index(0.7, 0).unwrap(),
        g.cell_index(0.2, 1).unwrap(),
    ])
    .unwrap();
    let sent: Vec<Element> = g
        .cells_in_range(&first, &last)
        .unwrap()
        .flat_map(|(_, items)| items.iter().copied())
        .collect();
    assert_eq!(sent, vec![el(0), el(1)]);
    // contact detector: getCellID(position), neighbors(), getCell(neighbor)
    let id = g.cell_id(&[0.2, 0.2]).unwrap();
    let near: Vec<Element> = id.neighbors().flat_map(|n| g.cell(&n).to_vec()).collect();
    assert_eq!(near, vec![el(2), el(0), el(1)]);
}
