// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): shared helpers of the
// akantu-fe integration tests (no upstream counterpart).

#![allow(dead_code)] // each test binary uses a subset of the helpers

use akantu_core::ElementType;

/// The 13 regular element types (upstream `ElementTypes_t<_ek_regular>`),
/// in enum order.
pub const REGULAR: [ElementType; 13] = [
    ElementType::Point1,
    ElementType::Segment2,
    ElementType::Segment3,
    ElementType::Triangle3,
    ElementType::Triangle6,
    ElementType::Quadrangle4,
    ElementType::Quadrangle8,
    ElementType::Tetrahedron4,
    ElementType::Tetrahedron10,
    ElementType::Pentahedron6,
    ElementType::Pentahedron15,
    ElementType::Hexahedron8,
    ElementType::Hexahedron20,
];

/// The 7 cohesive element types, in enum order.
pub const COHESIVE: [ElementType; 7] = [
    ElementType::Cohesive1d2,
    ElementType::Cohesive2d4,
    ElementType::Cohesive2d6,
    ElementType::Cohesive3d12,
    ElementType::Cohesive3d16,
    ElementType::Cohesive3d6,
    ElementType::Cohesive3d8,
];

/// Upstream `TestElementTypes` (regular types without `_point_1`).
pub fn test_element_types() -> impl Iterator<Item = ElementType> {
    REGULAR.into_iter().filter(|t| *t != ElementType::Point1)
}

/// ULP distance between two finite doubles (0 for equal values, ±0 equal).
pub fn ulp_distance(a: f64, b: f64) -> u64 {
    if a == b {
        return 0;
    }
    let key = |x: f64| {
        let b = x.to_bits() as i64;
        if b < 0 { i64::MIN - b } else { b }
    };
    key(a).abs_diff(key(b))
}

/// `(number of entries that differ bitwise (±0 equal), max ULP distance)`.
pub fn bit_stats(expected: &[f64], actual: &[f64]) -> (usize, u64) {
    assert_eq!(expected.len(), actual.len());
    let mut n = 0;
    let mut m = 0;
    for (e, a) in expected.iter().zip(actual) {
        let d = ulp_distance(*e, *a);
        if d != 0 {
            n += 1;
            m = m.max(d);
        }
    }
    (n, m)
}

use akantu_core::{GhostType, Idx};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::Fixture;

/// Build a mesh from a `mesh_io` golden fixture (node coordinates and the
/// connectivity of every element type recorded by `mesh_probe` from the
/// upstream `.msh` file), e.g. `mesh_io/test_fe_engine/_triangle_6`.
pub fn fixture_mesh(case_id: &str) -> Mesh {
    let fx = Fixture::open_by_id(case_id).expect("mesh fixture");
    let nodes = fx.load_f64("nodes").unwrap();
    let dim = fx
        .manifest()
        .parameters
        .get("spatial_dimension")
        .and_then(|v| v.as_u64())
        .unwrap() as usize;
    let mut mesh = Mesh::new(dim, case_id).unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let n = nodes.shape[0];
        let stride = nodes.shape[1];
        for i in 0..n {
            acc.push_node(&nodes.data[i * stride..i * stride + dim])
                .unwrap();
        }
        for name in fx.quantity_names("connectivity/*") {
            let tname = name.trim_start_matches("connectivity/");
            let ty: ElementType = tname.parse().unwrap();
            let c = fx.load_i64(name).unwrap();
            let mut conn = acc.connectivity_mut(ty, GhostType::NotGhost);
            for row in c.data.chunks(c.shape[1]) {
                let r: Vec<Idx> = row.iter().map(|&v| v as Idx).collect();
                conn.push_tuple(&r);
            }
        }
    }
    mesh
}

/// Mesh fixture of the upstream `test_fe_engine` mesh of `ty`.
pub fn fe_test_mesh(ty: ElementType) -> Mesh {
    fixture_mesh(&format!("mesh_io/test_fe_engine/{}", ty.name()))
}
