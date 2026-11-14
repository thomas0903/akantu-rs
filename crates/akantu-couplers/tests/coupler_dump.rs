// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Behaviour of the
// couplers' model-level dumpers (CPL-001, CPL-003, CPL-005; D-078) against
// upstream's src/model/model_couplers/coupler_solid_contact_tmpl.hh
// (createNodalField*, dump), coupler_solid_cohesive_contact.cc
// (constructor), coupler_solid_phasefield.cc (constructor,
// dump), src/model/model.cc (Model::setBaseName, addDumpField*, dump) and
// src/model/contact_mechanics/contact_mechanics_model.cc (dumper
// registration, createNodalField*). The byte-for-byte oracle comparisons of
// these dumps are the examples' (EX-001…004, EX-010, EX-011 in
// crates/akantu/tests, EX-042…045 in python/tests/examples).

mod common;

use std::path::{Path, PathBuf};

use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::{CohesiveSurfaceSelector, PhysicalSurfaceSelector};
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_couplers::{CouplerSolidCohesiveContact, CouplerSolidContact, CouplerSolidPhaseField};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use common::*;

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!("akantu-coupler-dump-{}-{name}", std::process::id()));
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Every file name under `dir` (relative, sorted).
fn files(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().display().to_string());
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The `NumberOfComponents` of the data array `name` in the last `.vtu`
/// file of `dir` (the latest dump).
fn components(dir: &Path, name: &str) -> usize {
    let vtu = files(dir)
        .into_iter()
        .rev()
        .find(|f| f.ends_with(".vtu"))
        .expect("a .vtu file");
    let text = std::fs::read(dir.join(vtu)).unwrap();
    let text = String::from_utf8_lossy(&text);
    let tag = text
        .split("<DataArray")
        .find(|t| t.contains(&format!("Name=\"{name}\"")))
        .unwrap_or_else(|| panic!("no data array {name}"));
    let n = tag
        .split("NumberOfComponents=\"")
        .nth(1)
        .and_then(|r| r.split('"').next())
        .unwrap();
    n.parse().unwrap()
}

/// The set-up of `contact_static.rs` (upstream `contact_explicit_static.cc`)
/// after one contact search.
fn hertz() -> CouplerSolidContact {
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
    let mut c = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    c.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    let fix = |axis| FixedValue { value: 0.0, axis };
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::Y), "fixed")
        .unwrap();
    c
}

#[test]
fn solid_contact_blocked_dofs_is_the_contact_models() {
    // type-major resolution, contact first: `blocked_dofs` is a boolean
    // nodal field of both models; the contact model's has one component
    // (upstream `coupler_solid_contact_tmpl.hh:318-328`), the solid's two
    let dir = scratch("blocked");
    let mut c = hertz();
    c.set_directory(&dir).unwrap();
    c.set_base_name("hertz").unwrap();
    c.add_dump_field("blocked_dofs").unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    c.add_dump_field("contact_state").unwrap();
    c.add_dump_field("material_index").unwrap();
    c.dump(None, None).unwrap();
    assert_eq!(components(&dir, "blocked_dofs"), 1);
    assert_eq!(components(&dir, "displacement"), 3);
    assert_eq!(components(&dir, "contact_state"), 1);
    // the solid model's own dumper still resolves its own `blocked_dofs`
    let own = scratch("blocked-solid");
    c.solid.set_directory(&own).unwrap();
    c.solid.add_dump_field("blocked_dofs").unwrap();
    c.solid.dump(None, None).unwrap();
    assert_eq!(components(&own, "blocked_dofs"), 2);
}

#[test]
fn solid_contact_default_series_is_the_contact_models_dumper() {
    let dir = scratch("default");
    let mut c = hertz();
    c.set_directory(&dir).unwrap();
    c.add_dump_field("gaps").unwrap();
    c.dump(None, None).unwrap();
    // upstream: the mesh's default dumper is the contact model's
    // `contact_mechanics`, file base name = the contact model's id
    let f = files(&dir);
    assert!(
        f.iter().any(|n| n.starts_with("contact_mechanics_model")),
        "{f:?}"
    );
    // the contact model's own calls act on the same series
    c.contact.add_dump_field("normals").unwrap();
    c.dump(None, None).unwrap();
    assert_eq!(components(&dir, "normals"), 2);
}

#[test]
fn solid_contact_set_time_step_gives_the_dumper_no_time_step() {
    // upstream's couplers do not override `ModelSolver::setTimeStep`: no
    // dumper time step, so no `.pvd`
    let dir = scratch("pvd");
    let mut c = hertz();
    c.set_directory(&dir).unwrap();
    c.add_dump_field("gaps").unwrap();
    c.set_time_step(1e-6, None).unwrap();
    c.dump(None, None).unwrap();
    c.dump(None, None).unwrap();
    assert!(
        !files(&dir).iter().any(|n| n.ends_with(".pvd")),
        "{:?}",
        files(&dir)
    );
    assert_eq!(c.solid.time_step(None).unwrap(), 1e-6);
    assert_eq!(c.solid.law_time_step(), 1e-6);
    // Nor did the solid model's own series get one (a
    // coupler `set_time_step` that set the solid's dumper is caught here)
    let own = scratch("pvd-solid");
    c.solid.set_directory(&own).unwrap();
    c.solid.add_dump_field_vector("displacement").unwrap();
    c.solid.dump(None, None).unwrap();
    c.solid.dump(None, None).unwrap();
    assert!(
        !files(&own).iter().any(|n| n.ends_with(".pvd")),
        "{:?}",
        files(&own)
    );
}

#[test]
fn solid_cohesive_contact_set_time_step_gives_the_dumper_no_time_step() {
    // the cohesive coupler's case: its series is the
    // cohesive solid's default dumper, which must get no time step either
    let dir = scratch("pvd-cohesive");
    let mut c = cohesive_contact();
    c.set_directory(&dir).unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    c.set_time_step(1e-6, None).unwrap();
    c.dump(None, None).unwrap();
    c.dump(None, None).unwrap();
    assert!(
        !files(&dir).iter().any(|n| n.ends_with(".pvd")),
        "{:?}",
        files(&dir)
    );
    assert_eq!(c.solid.smm.time_step(None).unwrap(), 1e-6);
}

#[test]
fn solid_part_calls_reach_the_coupler_series() {
    // Upstream's `solid.setTimeStep` gives the time step
    // to the mesh's default dumper (the contact model's, the coupler's
    // series: `e3.pvd` of case E3) and `solid.addDumpFieldVector`
    // registers the solid's field there (E4); the coupler's
    // `solid_set_time_step` / `solid_add_dump_field` (the Python solid
    // part's routes) do the same
    let dir = scratch("solid-part");
    let mut c = hertz();
    c.set_directory(&dir).unwrap();
    c.set_base_name("e3").unwrap();
    c.add_dump_field("gaps").unwrap();
    c.solid_add_dump_field("displacement", true).unwrap();
    // the solid resolves the name: a contact-only field is refused
    assert!(c.solid_add_dump_field("normals", true).is_err());
    c.solid_set_time_step(1e-6, None).unwrap();
    c.dump(None, None).unwrap();
    c.dump(None, None).unwrap();
    let f = files(&dir);
    assert!(f.iter().any(|n| n == "e3.pvd"), "{f:?}");
    assert_eq!(components(&dir, "displacement"), 3);
    assert_eq!(c.solid.time_step(None).unwrap(), 1e-6);
}

#[test]
fn solid_contact_refusals_are_errors_never_silent() {
    let mut c = hertz();
    // a name neither part resolves (UD-683)
    assert!(c.add_dump_field("no_such_field").is_err());
    assert!(
        c.add_dump_field_to_dumper("no_such_dumper", "gaps")
            .is_err()
    );
    // a solid field the coupler registered on the contact model's dumper is
    // resolved by the coupler's dump, refused by the contact model alone
    let dir = scratch("foreign");
    c.set_directory(&dir).unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    assert!(c.contact.dump(None, None).is_err());
    c.dump(None, None).unwrap();
}

#[test]
fn solid_contact_own_dumper_by_name() {
    // upstream registers the coupler's own `coupler_solid_contact` dumper
    // (file = the coupler's id), never the default once the parts exist
    let dir = scratch("own");
    let mut c = hertz();
    c.set_directory_to_dumper("coupler_solid_contact", &dir)
        .unwrap();
    c.add_dump_field_to_dumper("coupler_solid_contact", "gaps")
        .unwrap();
    c.add_dump_field_vector_to_dumper("coupler_solid_contact", "displacement")
        .unwrap();
    c.dump_named("coupler_solid_contact", None, None).unwrap();
    let f = files(&dir);
    assert!(
        f.iter().any(|n| n.starts_with("coupler_solid_contact")),
        "{f:?}"
    );
    assert_eq!(components(&dir, "displacement"), 3);
}

/// The set-up of `cohesive_contact.rs` (upstream
/// `cohesive_contact_explicit_dynamic.cc`), before any step.
fn cohesive_contact() -> CouplerSolidCohesiveContact {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/cohesive-contact.msh"),
        2,
    );
    let parser = Parser::parse_str(
        include_str!("data/contact/material-cohesive.dat"),
        "material-cohesive.dat",
    )
    .unwrap();
    let mut c = CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    c.init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass), true)
        .unwrap();
    c
}

#[test]
fn solid_cohesive_contact_default_series_is_the_cohesive_solid_models() {
    // upstream: the contact model lives on the facet mesh, so the mesh's
    // default dumper stays the cohesive solid model's (file = its id
    // `solid_mechanics_model_cohesive`); the coupler's fields are resolved
    // type-major, contact first, as for `CouplerSolidContact`
    let dir = scratch("cohesive");
    let mut c = cohesive_contact();
    c.set_directory(&dir).unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    c.add_dump_field("gaps").unwrap();
    c.add_dump_field("blocked_dofs").unwrap();
    c.add_dump_field("material_index").unwrap();
    c.dump(None, None).unwrap();
    let f = files(&dir);
    assert!(
        f.iter()
            .any(|n| n.starts_with("solid_mechanics_model_cohesive")),
        "{f:?}"
    );
    assert_eq!(components(&dir, "displacement"), 3);
    assert_eq!(components(&dir, "gaps"), 1);
    assert_eq!(components(&dir, "blocked_dofs"), 1);
    // the cohesive solid model's own calls act on the same series
    c.solid.smm.add_dump_field_vector("velocity").unwrap();
    c.dump(None, None).unwrap();
    assert_eq!(components(&dir, "velocity"), 3);
    // the coupler's own dumper is reachable by name
    let own = scratch("cohesive-own");
    c.set_directory_to_dumper("coupler_solid_cohesive_contact", &own)
        .unwrap();
    c.add_dump_field_to_dumper("coupler_solid_cohesive_contact", "gaps")
        .unwrap();
    c.dump_named("coupler_solid_cohesive_contact", None, None)
        .unwrap();
    assert!(
        files(&own)
            .iter()
            .any(|n| n.starts_with("coupler_solid_contact")),
        "{:?}",
        files(&own)
    );
    assert!(c.add_dump_field("no_such_field").is_err());
}

#[test]
fn cohesive_solid_dump_reads_the_contact_fields() {
    // Upstream's `getSolidMechanicsModelCohesive().dump()`
    // after `coupler.addDumpField("gaps")` writes the mesh's default dumper
    // (the cohesive solid's) with the contact field; the solid model's own
    // dump refuses the field it cannot resolve, `dump_solid` (the Python
    // part's route) reads it from the contact model
    let dir = scratch("cohesive-solid-dump");
    let mut c = cohesive_contact();
    c.set_directory(&dir).unwrap();
    c.add_dump_field("gaps").unwrap();
    assert!(c.solid.dump(None, None).is_err());
    c.dump_solid(None, None, None).unwrap();
    assert_eq!(components(&dir, "gaps"), 1);
    c.dump(None, None).unwrap();
    let f = files(&dir);
    assert_eq!(
        f.iter().filter(|n| n.ends_with(".pvtu")).count(),
        2,
        "{f:?}"
    );
}

fn phase_coupler() -> CouplerSolidPhaseField {
    let dat = data_dir().join("test_phase_field_model/material_coupling.dat");
    let msh = data_dir().join("test_phase_field_model/test_one_element.msh");
    let mesh = read_mesh(&msh, 2);
    let parser = Parser::parse_file(dat).unwrap();
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .unwrap();
    c.solid
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    c.phase
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    c
}

#[test]
fn phase_field_default_series_is_named_after_the_phase_field_model() {
    // upstream: the phase-field model registers the mesh's default dumper
    // last (file base name = its id), filled by the solid's fields
    let dir = scratch("pf-default");
    let mut c = phase_coupler();
    c.set_directory(&dir).unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    c.dump(None, None).unwrap();
    let f = files(&dir);
    assert!(
        f.iter().any(|n| n.starts_with("phase_field_model")),
        "{f:?}"
    );
    assert!(!f.iter().any(|n| n.starts_with("solid_mechanics_model")));
}

#[test]
fn phase_field_solid_and_coupler_calls_share_one_series() {
    // upstream EX-042: `solid.setBaseName`, `solid.addDumpField*`,
    // `solid.dump()`, then `model.dump()` continue one series
    let dir = scratch("pf-shared");
    let mut c = phase_coupler();
    c.solid.set_directory(&dir).unwrap();
    c.solid.set_base_name("plate").unwrap();
    c.solid.add_dump_field_vector("displacement").unwrap();
    c.solid.dump(None, None).unwrap();
    c.add_dump_field("damage").unwrap();
    c.dump(None, None).unwrap();
    let f = files(&dir);
    assert!(f.iter().any(|n| n.contains("plate_0001")), "{f:?}");
    assert_eq!(f.iter().filter(|n| n.ends_with(".pvtu")).count(), 2);
    // integer nodal fields: the coupler has none (it asks the solid's
    // real and boolean nodal fields and its elemental fields only)
    assert!(c.add_dump_field("no_such_field").is_err());
}

#[test]
fn phase_part_series_never_overwrites_the_coupler_series() {
    // The phase-field model's own default dumper (the
    // port's models own their dumpers) has its own base name, so
    // `coupler.dump()` then `phase.dump()` keep both files (the same base
    // name overwrote the coupler's `_0000` files); upstream writes one
    // series `_0000`, `_0001` (UD-982)
    let dir = scratch("pf-distinct");
    let mut c = phase_coupler();
    c.set_directory(&dir).unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    c.phase.set_directory(&dir).unwrap();
    c.phase.add_dump_field("damage").unwrap();
    c.dump(None, None).unwrap();
    c.phase.dump(None, None).unwrap();
    let f = files(&dir);
    assert!(
        f.iter().any(|n| n == "phase_field_model_0000.pvtu"),
        "{f:?}"
    );
    assert!(
        f.iter().any(|n| n == "phase_field_model_part_0000.pvtu"),
        "{f:?}"
    );
    assert_eq!(f.iter().filter(|n| n.ends_with(".pvtu")).count(), 2);
}

#[test]
fn test_contact_coupling_dumps_through_the_contact_model() {
    // TEST-215 (test_contact_coupling.cc:51-61): the driver registers its
    // fields through the coupler and dumps through the contact model;
    // upstream's mesh default dumper is the contact model's, so that dump
    // writes every coupler-registered field (the solid's included)
    let dir = scratch("test215");
    let mut c = hertz();
    c.set_directory(&dir).unwrap();
    c.set_base_name("coupling").unwrap();
    c.add_dump_field_vector("displacement").unwrap();
    c.add_dump_field("blocked_dofs").unwrap();
    c.add_dump_field("external_force").unwrap();
    c.add_dump_field("internal_force").unwrap();
    c.add_dump_field("grad_u").unwrap();
    c.add_dump_field("stress").unwrap();
    c.solve_step(None).unwrap();
    // the contact model alone cannot read the solid's fields …
    assert!(c.contact.dump(None, None).is_err());
    // … the coupler's view of `contact.dump()` does
    c.dump_contact(None, None, None).unwrap();
    let f = files(&dir);
    assert!(f.iter().any(|n| n.starts_with("coupling")), "{f:?}");
    assert_eq!(components(&dir, "displacement"), 3);
    assert_eq!(components(&dir, "blocked_dofs"), 1);
    assert_eq!(components(&dir, "external_force"), 2);
    assert_eq!(components(&dir, "stress"), 4);
}
