// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs tests (BC-001/004/005).
// Behaviour of upstream's `BoundaryCondition::applyBC` ported from
// src/model/boundary_condition_tmpl.hh (Dirichlet: lines 38-90, Neumann:
// 91-190, the 1-D `AKANTU_TO_IMPLEMENT`) as exercised through
// `SolidMechanicsModel::apply_dirichlet_bc` / `apply_neumann_bc`.
//! * Dirichlet on a periodic master whose slave is **outside** the group:
//!   `makeConsistentForPeriodicity(blocked, _or)` and
//!   `makeConsistentForPeriodicity(primal, _id)` must copy the master's
//!   flag and **imposed non-zero value** to the slave (removing only the
//!   `id` copy of the primal makes the test fail).
//! * Neumann in a 1-D model is refused (`AKANTU_TO_IMPLEMENT` upstream).

mod model_common;

use std::path::PathBuf;

use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{Idx, Real};
use akantu_mesh::Mesh;
use akantu_mesh::periodic::make_periodic;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromTraction};
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::{model_with, read_mesh};

/// `test_mesh_periodic/material.dat` (verbatim, as `upstream_periodic.rs`).
const MATERIAL: &str = "material elastic [\n\
\t name = steel\n\
\t rho = 7800\n\
\t E   = 2.1e11\n\
\t nu  = 0.3\n\
]\n";

fn mesh_file(name: &str) -> PathBuf {
    fixtures_root()
        .join("meshes/gmsh-4.13.1/test/test_mesh_utils/test_mesh_periodic")
        .join(format!("{name}.msh"))
}

fn static_model(mesh: Mesh) -> SolidMechanicsModel {
    let mut m = model_with(mesh, MATERIAL);
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    m
}

/// Nodes of an element group (copied out).
fn group_nodes(mesh: &Mesh, group: &str) -> Vec<Idx> {
    mesh.groups()
        .element_group(group)
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

/// Dirichlet `FixedValue(0.37, x)` on the group of the periodic master
/// side of a unit square periodic in x: every periodic slave (not in the
/// group) takes the master's value and blocked flag.
#[test]
fn dirichlet_value_and_flag_reach_the_slave_outside_the_group() {
    let dim = 2;
    let mut mesh = read_mesh(&mesh_file("square"), dim);
    let _ = make_periodic(&mut mesh, SpatialDirection::X, None).unwrap();
    assert!(mesh.is_periodic());

    // the side (x-minus or x-plus) whose nodes are masters; the other side
    // holds their slaves, which are not in the imposed group
    let mut chosen = None;
    for side in ["x-minus", "x-plus"] {
        let nodes = group_nodes(&mesh, side);
        let masters = nodes
            .iter()
            .filter(|&&n| mesh.is_periodic_master(n).unwrap())
            .count();
        let slaves = nodes
            .iter()
            .filter(|&&n| mesh.is_periodic_slave(n).unwrap())
            .count();
        // corner nodes of the square also belong to y groups, but every
        // node of an x side is a master or a slave of the x periodicity
        if masters == nodes.len() {
            assert_eq!(slaves, 0, "{side}");
            chosen = Some((side, nodes));
        }
    }
    let (side, master_nodes) = chosen.expect("one x side holds the masters");
    let slaves: Vec<(Idx, Idx)> = master_nodes
        .iter()
        .flat_map(|&n| mesh.periodic_slaves(n).iter().map(move |&s| (n, s)))
        .collect();
    assert!(!slaves.is_empty());
    for &(_, s) in &slaves {
        assert!(
            !master_nodes.contains(&s),
            "the slave must be outside the group"
        );
    }

    let value: Real = 0.37;
    let mut m = static_model(mesh);
    m.apply_dirichlet_bc(
        &FixedValue {
            value,
            axis: SpatialDirection::X,
        },
        side,
    )
    .unwrap();

    let u = m.displacement().unwrap();
    let b = m.blocked_dofs().unwrap();
    for &(master, slave) in &slaves {
        let (mi, si) = (
            usize::try_from(master).unwrap(),
            usize::try_from(slave).unwrap(),
        );
        // the master carries the imposed value and flag ...
        assert_eq!(
            u.as_slice()[mi * dim].to_bits(),
            value.to_bits(),
            "master {master}"
        );
        assert!(b.as_slice()[mi * dim], "master {master}");
        // ... and so does the slave (`_id` copy of the primal, `_or` of
        // the flags), although it is not part of the group
        assert_eq!(
            u.as_slice()[si * dim].to_bits(),
            value.to_bits(),
            "slave {slave}"
        );
        assert!(b.as_slice()[si * dim], "slave {slave}");
        // the y component was not touched on either node
        for n in [mi, si] {
            assert_eq!(u.as_slice()[n * dim + 1], 0.0);
            assert!(!b.as_slice()[n * dim + 1]);
        }
    }
    // a non-periodic node away from both sides is untouched
    let in_group = |n: usize| {
        slaves
            .iter()
            .any(|&(a, s)| usize::try_from(a).unwrap() == n || usize::try_from(s).unwrap() == n)
    };
    let untouched = (0..u.len()).find(|&n| !in_group(n) && u.as_slice()[n * dim] == 0.0);
    assert!(untouched.is_some());
    for n in 0..u.len() {
        if !in_group(n) {
            assert_eq!(u.as_slice()[n * dim], 0.0, "node {n}");
            assert!(!b.as_slice()[n * dim], "node {n}");
        }
    }
}

/// Upstream `applyBC(NeumannFunctor, group)` on a 1-D model is
/// `AKANTU_TO_IMPLEMENT`: the port returns an error that names the 1-D
/// Neumann case, and `external_force` is left untouched.
#[test]
fn neumann_in_a_one_dimensional_model_is_refused() {
    let mesh = read_mesh(&mesh_file("line"), 1);
    let mut m = static_model(mesh);
    let before = m.external_force().unwrap().as_slice().to_vec();
    let err = m
        .apply_neumann_bc(&FromTraction::new(vec![1.0e9]), "x-plus")
        .expect_err("1-D Neumann boundary conditions are not implemented upstream");
    let text = err.to_string();
    assert!(
        text.contains("Neumann") && text.contains("1-D"),
        "unexpected error: {text}"
    );
    assert_eq!(m.external_force().unwrap().as_slice(), before.as_slice());
}
