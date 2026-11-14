// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports of
// test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_insertion/
// test_cohesive_insertion_along_physical_surfaces.cc and test_group_update.cc.

mod common;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{ElementKind, GhostType, TypeFilter};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_parser::Parser;

const INPUT: &str = include_str!("data/insertion_input_file.dat");

#[test]
fn test_cohesive_insertion_along_physical_surfaces() {
    let _g = common::lock();
    let mesh = common::read_mesh(
        &common::cohesive_mesh("test_cohesive_insertion/3d_spherical_inclusion.msh"),
        3,
    );
    let parser = Parser::parse_str(INPUT, "input_file.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    model.set_math_tolerance(1e-15);
    let default = model.default_cohesive_selector();
    model.set_material_selector(Box::new(MeshDataMaterialCohesiveSelector { default }));
    model
        .init_full(
            &ModelOptions::new(AnalysisMethod::ExplicitLumpedMass),
            false,
        )
        .unwrap();
    let m = mesh.read().unwrap();
    let mut total = 0;
    for ty in m.element_types(
        TypeFilter::new()
            .dim(3)
            .ghost(GhostType::NotGhost)
            .kind(ElementKind::Cohesive),
    ) {
        let facet_type = ty.facet_type(0).unwrap();
        for name in ["interface", "coh1", "coh2", "coh3", "coh4", "coh5"] {
            let expected = m
                .groups()
                .element_group(name)
                .unwrap()
                .elements_of(facet_type, GhostType::NotGhost)
                .len();
            let mat = model.cohesive_material(name).unwrap();
            let inserted = akantu_solid::Material::base(mat)
                .element_filter
                .get(ty, GhostType::NotGhost)
                .map_or(0, |f| f.len());
            assert_eq!(inserted, expected, "surface {name}");
            total += inserted;
        }
    }
    assert!(total > 0);
}

#[test]
fn test_cohesive_group_update() {
    let _g = common::lock();
    let mesh = common::read_mesh(&common::cohesive_mesh("test_cohesive_insertion/bar.msh"), 2);
    let dat = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("tests/data/group_update_material.dat"),
    )
    .unwrap();
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static), true)
        .unwrap();
    model
        .smm
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    model.update_automatic_insertion().unwrap();
    let dt = model.stable_time_step().unwrap() * 0.1;
    model.set_time_step(dt, Some("explicit_lumped")).unwrap();
    let n_steps = (6.0e-6 / dt) as usize;
    model
        .smm
        .apply_dirichlet_bc(
            &FixedValue {
                value: 0.,
                axis: SpatialDirection::Y,
            },
            "YBlocked",
        )
        .unwrap();
    let (lo, hi) = {
        let m = mesh.read().unwrap();
        (m.lower_bounds()[0], m.upper_bounds()[0])
    };
    let vel = 1e5 * (hi - lo) / 2.;
    let left = IncrementValue {
        value: -vel * dt,
        axis: SpatialDirection::X,
    };
    let right = IncrementValue {
        value: vel * dt,
        axis: SpatialDirection::X,
    };
    model.smm.apply_dirichlet_bc(&left, "left").unwrap();
    model.smm.apply_dirichlet_bc(&right, "right").unwrap();
    for _ in 0..n_steps {
        model.smm.apply_dirichlet_bc(&left, "left").unwrap();
        model.smm.apply_dirichlet_bc(&right, "right").unwrap();
        model.check_cohesive_stress().unwrap();
        model.solve_step(Some("explicit_lumped")).unwrap();
    }
    // the intent of the upstream test: element groups follow the doubled
    // nodes (every node of a group's elements is in its node group)
    let m = mesh.read().unwrap();
    for g in m.groups().element_groups() {
        let nodes: std::collections::HashSet<_> = g.nodes().iter().copied().collect();
        for ty in m.element_types(TypeFilter::new().ghost(GhostType::NotGhost)) {
            for &e in &g.elements_of(ty, GhostType::NotGhost) {
                let el = akantu_core::Element::new(ty, e, GhostType::NotGhost);
                for n in &m.element_connectivity(el).unwrap() {
                    assert!(
                        nodes.contains(n),
                        "group {}: node {n} of {el} missing",
                        g.name()
                    );
                }
            }
        }
    }
    eprintln!(
        "group_update: {n_steps} steps, {} cohesive elements",
        model.ext.stats.inserted
    );
}
