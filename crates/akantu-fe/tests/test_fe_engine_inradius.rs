// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_fe_engine/test_fe_engine_inradius.cc

//! `TestFEMFixture.Inradius`: the element characteristic length of every
//! element of the upstream test meshes matches upstream's tabulated values
//! (6 significant digits, hence the upstream tolerance `1e-6`), both through
//! the static coordinate overload and through the element overload.

mod common;

use akantu_core::{Element, ElementKind, GhostType};
use akantu_fe::engine::element_inradius_of;
use akantu_fe::{FeEngine, shape::ShapeFamily};

#[test]
fn test_fe_engine_inradius() {
    for ty in common::test_element_types() {
        let mesh = common::fe_test_mesh(ty);
        let fe = FeEngine::with_precomputation(
            &mesh,
            None,
            ElementKind::Regular,
            ShapeFamily::Lagrange,
            "my_fem",
        )
        .unwrap();
        let path = format!(
            "{}/tests/data/inradius/{}.csv",
            env!("CARGO_MANIFEST_DIR"),
            ty.name()
        );
        let text = std::fs::read_to_string(&path).unwrap();
        let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
        let mut reference = vec![f64::NAN; conn.len()];
        for line in text.lines().filter(|l| !l.trim().is_empty()) {
            let (id, r) = line.split_once(',').unwrap();
            reference[id.trim().parse::<usize>().unwrap()] = r.trim().parse().unwrap();
        }
        let dim = mesh.spatial_dimension();
        let nodes = mesh.nodes();
        for (e, c) in conn.tuples().enumerate() {
            let mut x = vec![0.0; dim * c.len()];
            for (s, &n) in c.iter().enumerate() {
                x[s * dim..(s + 1) * dim].copy_from_slice(nodes.tuple(n as usize));
            }
            let r_coor = element_inradius_of(ty, &x);
            let r_elem = fe
                .element_inradius(&mesh, Element::new(ty, e as i32, GhostType::NotGhost))
                .unwrap();
            assert_eq!(r_coor.to_bits(), r_elem.to_bits());
            assert!(
                (r_elem - reference[e]).abs() <= 1e-6,
                "{}: element {e}: {r_elem} vs {}",
                ty.name(),
                reference[e]
            );
        }
    }
}
