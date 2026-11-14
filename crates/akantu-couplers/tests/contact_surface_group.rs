// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). The mesh group
// `contact_surface` of the surface selectors (surface_selector.cc:37-52,
// 69-98: PhysicalSurfaceSelector creates it in the mesh, the cohesive
// selectors in the facet mesh and add the facets of every new cohesive
// element) set through the couplers.

mod common;

use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::selector::ContactSurface;
use akantu_contact::{CohesiveSurfaceSelector, PhysicalSurfaceSelector};
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::{GhostType, Real};
use akantu_couplers::{CouplerSolidCohesiveContact, CouplerSolidContact};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use common::*;

/// The group `contact_surface` of `mesh` equals `surface` (facets in
/// order, nodes, dimension 1).
fn assert_mirrors(mesh: &Mesh, surface: &ContactSurface) {
    let g = mesh.groups().element_group("contact_surface").unwrap();
    assert_eq!(g.group().dimension, 1);
    assert_eq!(g.nodes().to_vec(), surface.nodes().to_vec());
    for ghost in GhostType::ALL {
        let types = surface.element_types(ghost);
        let mut group_types = g.group().element_types(ghost);
        group_types.retain(|&t| !g.elements_of(t, ghost).is_empty());
        assert_eq!(group_types, types, "{ghost:?}");
        for ty in types {
            assert_eq!(
                g.elements_of(ty, ghost).to_vec(),
                surface.elements_of(ty, ghost).to_vec()
            );
        }
    }
}

#[test]
fn solid_contact_coupler_mirrors_the_physical_surface() {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/hertz.msh"),
        2,
    );
    let parser = Parser::parse_str(
        include_str!("data/contact/hertz_material.dat"),
        "material.dat",
    )
    .unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let shared = SharedMesh::new(mesh);
    let mut c =
        CouplerSolidContact::with_parser(shared.clone(), None, "coupler_solid_contact", parser)
            .unwrap();
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    c.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    // the contact model alone cannot write it (the solid model is a
    // consumer of the mesh): through the coupler it does
    c.set_surface_selector(Box::new(selector)).unwrap();
    let s = c
        .contact
        .contact_detector()
        .surface_selector()
        .unwrap()
        .contact_surface()
        .clone();
    assert!(s.nb_elements() > 0);
    assert_mirrors(&shared.read().unwrap(), &s);
    // the models follow the revision of the group-only mutation
    c.solve_step(None).unwrap();
}

#[test]
fn cohesive_contact_coupler_mirrors_the_inserted_facets() {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
        2,
    );
    let parser = Parser::parse_str(
        include_str!("data/contact/material-cohesive.dat"),
        "material-cohesive.dat",
    )
    .unwrap();
    let shared = SharedMesh::new(mesh);
    let mut c = CouplerSolidCohesiveContact::with_parser(
        shared.clone(),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    // created empty in the facet mesh (upstream: replace_group = true)
    {
        let m = shared.read().unwrap();
        let fm = m.mesh_facets().unwrap();
        let g = fm.groups().element_group("contact_surface").unwrap();
        assert_eq!(g.nb_nodes(), 0);
        assert!(g.group().is_empty());
        assert!(!m.groups().element_group_exists("contact_surface"));
    }
    c.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
        .unwrap();
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    c.apply_dirichlet_bc(&fix, "sides").unwrap();
    let dt = c.stable_time_step().unwrap() * 0.1;
    c.set_time_step(dt, None).unwrap();
    // the example's loading until the first insertion
    let inc = 1e-3 / Real::from(2500u32);
    let mut inserted = 0;
    for _ in 0..2500 {
        for (group, v) in [("loading", inc), ("fixed", -inc)] {
            let bc = IncrementValue {
                value: v,
                axis: SpatialDirection::Y,
            };
            c.apply_dirichlet_bc(&bc, group).unwrap();
        }
        c.solve_step(None).unwrap();
        inserted += c.check_cohesive_stress().unwrap();
        if inserted > 0 {
            break;
        }
    }
    assert!(inserted > 0);
    let s = c
        .contact
        .contact_detector()
        .surface_selector()
        .unwrap()
        .contact_surface()
        .clone();
    // both facets of every new cohesive element
    assert_eq!(s.nb_elements(), 2 * inserted);
    let m = shared.read().unwrap();
    assert_mirrors(m.mesh_facets().unwrap(), &s);
}
