// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression test:
// PhaseFieldModel::setTimeStep
// (src/model/phase_field/phase_field_model.cc:309-313) gives the time step
// to the model's own dumper `phase_field` only, not to every registered
// dumper; replayed against fixtures/model/phase_field/dump_set_time_step
// (oracle/probes/phase_field/phase_field_probe.cc `dump_set_time_step_case`).

//! PF-012: `set_time_step` and the dumpers.
//!
//! Upstream's `PhaseFieldModel::setTimeStep` calls
//! `mesh.getDumper("phase_field").setTimeStep(dt)`, which is what activates
//! the time description (`.pvd`) of that dumper (`DumperIOHelper::
//! setTimeStep`). A dumper registered next to it (before or after the
//! call) keeps no time step and writes no `.pvd`. Before the fix the port
//! gave the time step to every registered dumper, so an extra dumper
//! registered before `set_time_step` wrote a `.pvd` that upstream does not
//! write; both tests below fail on that code.

mod common;

use std::path::{Path, PathBuf};

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{ElementKind, GhostType};
use akantu_io::DumperParaview;
use akantu_io::dumper::MeshSelection;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_phase_field::{PhaseFieldModel, PhaseFieldUpstreamCompat};
use akantu_solver::ModelSolverOptions;
use akantu_validation::{Fixture, fixtures_root};
use common::{read_mesh, set_uniform_strain, uniaxial};

const ID: &str = "dump_set_time_step";

fn eigen_options() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("pf_dump_set_time_step")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Every file below `dir`, relative, sorted (as the probe's
/// `record_dump_tree`).
fn tree(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        if let Ok(rd) = std::fs::read_dir(d) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_dir() {
                    walk(base, &p, out);
                } else {
                    let rel = p.strip_prefix(base).unwrap();
                    out.push(rel.to_string_lossy().replace('\\', "/"));
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

/// The probe's scenario: the model dumper `phase_field` (base name `ID`)
/// and an extra text-mode dumper registered **before** `set_time_step`,
/// dumps before and after one static step with every upstream overload.
/// Returns the model (for the state checks) after the last dump.
fn run(mesh_path: &Path, material: &str, out: &Path) -> PhaseFieldModel {
    let mesh = read_mesh(mesh_path, 2);
    let parser = Parser::parse_str(material, "material.dat").unwrap();
    let mut model =
        PhaseFieldModel::with_parser(SharedMesh::new(mesh), None, "phase_field_model", parser)
            .unwrap();
    model.set_upstream_compat(PhaseFieldUpstreamCompat::upstream());
    model.init_full(&eigen_options()).unwrap();
    let pv = out.join("paraview");
    model.set_base_name(ID).unwrap();
    model.set_directory(&pv).unwrap();
    for f in [
        "damage",
        "external_force",
        "internal_force",
        "blocked_dofs",
        "partitions",
    ] {
        model.add_dump_field(f).unwrap();
    }
    {
        let d = &mut model.dumpers.dumpable;
        d.register_paraview("extra", Some(&format!("{ID}_extra")), false);
        d.add_dump_mesh_to_dumper(
            "extra",
            MeshSelection::new(Some(2), GhostType::NotGhost, ElementKind::Regular),
        )
        .unwrap();
        d.set_directory_to_dumper("extra", &pv).unwrap();
        d.set_text_mode_to_dumper("extra").unwrap();
        // upstream file layout (UD-110/113/114)
        for n in ["phase_field", "extra"] {
            d.dumper_as::<DumperParaview>(n)
                .unwrap()
                .set_compat_layout(true);
        }
    }
    model.add_dump_field_to_dumper("extra", "damage").unwrap();
    model
        .add_dump_field_to_dumper("extra", "internal_force")
        .unwrap();
    model.set_time_step(0.125, None).unwrap();

    model.dump(None, None).unwrap();
    model.dump_named("extra", None, None).unwrap();
    set_uniform_strain(&mut model, &uniaxial(2, 3e-3));
    model.solve_step(Some("static")).unwrap();
    model.dump(None, None).unwrap();
    model.dump_named("extra", None, None).unwrap();
    model.dump(None, Some(5)).unwrap();
    model.dump_named("extra", None, Some(7)).unwrap();
    model.dump(Some(0.75), Some(9)).unwrap();
    model.dump_named("extra", Some(1.5), Some(3)).unwrap();
    model
}

/// Independent of the fixture: only the `phase_field` dumper writes a
/// time description; the extra dumper registered before `set_time_step`
/// writes its `.pvtu`/`.vtu` files but no `.pvd`.
#[test]
fn only_the_phase_field_dumper_gets_the_time_step() {
    let mesh = fixtures_root().join(
        "model/phase_field/patch_quadrangle_4_linear_voldev_penalization/inputs/_quadrangle_4.msh",
    );
    let dat =
        "phasefield quadratic [\n name = body\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 0.2\n]\n";
    let out = scratch("independent");
    let _ = run(&mesh, dat, &out);
    let files = tree(&out);
    let pvd: Vec<&String> = files.iter().filter(|f| f.ends_with(".pvd")).collect();
    assert_eq!(
        pvd,
        vec![&format!("paraview/{ID}.pvd")],
        "only the phase_field dumper writes a .pvd (upstream setTimeStep); files: {files:?}"
    );
    assert!(
        files
            .iter()
            .any(|f| f.starts_with(&format!("paraview/{ID}_extra")) && f.ends_with(".pvtu")),
        "the extra dumper still dumps: {files:?}"
    );
}

/// Byte-for-byte replay of the oracle case: the same file list (the
/// decisive part: no `dump_set_time_step_extra.pvd`) and identical bytes,
/// and the nodal damage / internal force after the step bitwise.
#[test]
fn dump_set_time_step_matches_the_oracle_byte_for_byte() {
    let dir = fixtures_root().join("model/phase_field").join(ID);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    let material = std::fs::read_to_string(f.input_path("material").unwrap()).unwrap();
    let out = scratch("oracle");
    let model = run(&f.input_path("mesh").unwrap(), &material, &out);

    let mut n = 0;
    for (name, got) in [
        ("damage", model.damage().unwrap().as_slice().to_vec()),
        (
            "internal_force",
            model.internal_force().unwrap().as_slice().to_vec(),
        ),
    ] {
        let want = f.load_f64(name).unwrap().data;
        assert_eq!(got.len(), want.len(), "{name}: length");
        for (k, (a, b)) in got.iter().zip(&want).enumerate() {
            assert_eq!(a.to_bits(), b.to_bits(), "{name}[{k}]: {a:e} vs {b:e}");
        }
        n += want.len();
    }

    let want: Vec<String> = f.manifest().parameters["dump_files"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    assert!(
        want.iter().any(|w| w.ends_with(&format!("{ID}.pvd"))),
        "the oracle writes the phase_field .pvd: {want:?}"
    );
    assert!(
        !want.iter().any(|w| w.ends_with(&format!("{ID}_extra.pvd"))),
        "the oracle writes no .pvd for the extra dumper: {want:?}"
    );
    assert_eq!(tree(&out), want, "dumped file list");
    let mut bytes = 0;
    for rel in &want {
        let w = std::fs::read(f.input_path(&format!("dump:{rel}")).unwrap()).unwrap();
        let g = std::fs::read(out.join(rel)).unwrap_or_else(|e| panic!("{rel}: {e}"));
        assert!(g == w, "{rel}: differs from the oracle");
        bytes += w.len();
    }
    eprintln!(
        "dump_set_time_step: {n} values bitwise, {} files / {bytes} bytes identical",
        want.len()
    );
}
