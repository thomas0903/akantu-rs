// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression test:
// StructuralMechanicsModel::setTimeStep
// (src/model/structural_mechanics/structural_mechanics_model.cc:130-134)
// gives the time step to the mesh's default dumper only, not to every
// registered dumper; replayed against
// fixtures/model/structural/dump_set_time_step
// (oracle/probes/structural_model/structural_model_probe.cc
// `dump_set_time_step_case`, probe version 3).

//! STRUCT-013: `set_time_step` and the dumpers.
//!
//! Upstream's `StructuralMechanicsModel::setTimeStep` calls
//! `mesh.getDumper().setTimeStep(dt)`, which is what activates the time
//! description (`.pvd`) of the default dumper (`DumperIOHelper::
//! setTimeStep`). A dumper registered next to it keeps no time step and
//! writes no `.pvd`. Before the fix the port gave the time step to every
//! registered dumper, so an extra dumper registered before
//! `set_time_step` wrote a `.pvd` that upstream does not write; both tests
//! below fail on that code.

mod structural_model_common;

use std::path::{Path, PathBuf};

use akantu_core::enums::{AnalysisMethod, MeshIoType, NonLinearSolverType, SparseSolverType};
use akantu_core::{ElementKind, ElementType, GhostType, Real};
use akantu_io::DumperParaview;
use akantu_io::dumper::MeshSelection;
use akantu_mesh::Mesh;
use akantu_model::ModelOptions;
use akantu_solver::ModelSolverOptions;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use akantu_validation::{Fixture, fixtures_root};
use structural_model_common::{G, shared, upstream_mesh_path};

const ID: &str = "dump_set_time_step";
const TYPE: ElementType = ElementType::BernoulliBeam2;
const NDOF: usize = 3;

fn eigen_options() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("structural_dump_set_time_step")
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

/// The probe's scenario: TestStructBernoulli2's set-up with model id `ID`,
/// the default dumper with the structural fields and an extra text-mode
/// dumper registered **before** `set_time_step`, dumps before and after
/// one static step with every upstream overload. Returns the model after
/// the last dump.
fn run(mesh_path: &Path, out: &Path) -> StructuralMechanicsModel {
    let mut mesh = Mesh::new(2, "structural_mesh").unwrap();
    akantu_io::mesh_io::read_mesh(&mut mesh, mesh_path, MeshIoType::GmshStruct).unwrap();
    let mut model = StructuralMechanicsModel::with_compat(
        shared(mesh),
        None,
        ID,
        StructuralModelUpstreamCompat::UPSTREAM,
    )
    .unwrap();
    for i in [0.0025, 0.00128] {
        let mat = StructuralMaterial {
            e: 3e10,
            a: 0.01,
            i,
            ..StructuralMaterial::default()
        };
        model.add_material(mat, "").unwrap();
    }
    model.init_full(&eigen_options()).unwrap();
    model
        .element_material_mut(TYPE, G)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&[0, 1]);
    {
        let mut b_g = model.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b[..NDOF].copy_from_slice(&[true, true, true]);
        b[NDOF + 1] = true;
        b[2 * NDOF + 1] = true;
    }
    {
        let (m, q, l): (Real, Real, Real) = (3600.0, 6000.0, 10.0);
        let mut f_g = model.external_force_mut().unwrap();
        let f = f_g.as_mut_slice();
        f[2 * NDOF + 2] = -m;
        f[1] = -q * l / 2.;
        f[2] = -q * l * l / 12.;
        f[NDOF + 1] = -q * l / 2.;
        f[NDOF + 2] = q * l * l / 12.;
        f[2 * NDOF] = 3e10 * 0.01 / 18.;
    }

    let pv = out.join("paraview");
    model.set_base_name(ID).unwrap();
    model.set_directory(&pv).unwrap();
    model.add_dump_field_vector("displacement").unwrap();
    model.add_dump_field("rotation").unwrap();
    model.add_dump_field_vector("force").unwrap();
    model.add_dump_field("momentum").unwrap();
    model.add_dump_field("internal_force").unwrap();
    model.add_dump_field("internal_momentum").unwrap();
    model.add_dump_field("blocked_dofs").unwrap();
    model.add_dump_field("stress").unwrap();
    {
        let d = &mut model.dumpers.dumpable;
        d.register_paraview("extra", Some(&format!("{ID}_extra")), false);
        d.add_dump_mesh_to_dumper(
            "extra",
            MeshSelection::new(Some(2), GhostType::NotGhost, ElementKind::Structural),
        )
        .unwrap();
        d.set_directory_to_dumper("extra", &pv).unwrap();
        d.set_text_mode_to_dumper("extra").unwrap();
        // upstream file layout (UD-110/113/114)
        for n in ["structural_mechanics_model", "extra"] {
            d.dumper_as::<DumperParaview>(n)
                .unwrap()
                .set_compat_layout(true);
        }
    }
    model
        .add_dump_field_to_dumper("extra", "displacement")
        .unwrap();
    model
        .add_dump_field_to_dumper("extra", "internal_force")
        .unwrap();
    model.set_time_step(0.125, None).unwrap();

    model.dump(None, None).unwrap();
    model.dump_named("extra", None, None).unwrap();
    model.solve_step(None).unwrap();
    model.dump(None, None).unwrap();
    model.dump_named("extra", None, None).unwrap();
    model.dump(None, Some(5)).unwrap();
    model.dump_named("extra", None, Some(7)).unwrap();
    model.dump(Some(0.75), Some(9)).unwrap();
    model.dump_named("extra", Some(1.5), Some(3)).unwrap();
    model
}

/// Independent of the fixture: only the default dumper writes a time
/// description; the extra dumper registered before `set_time_step` writes
/// its `.pvtu` / `.vtu` files but no `.pvd`.
#[test]
fn only_the_default_dumper_gets_the_time_step() {
    let out = scratch("independent");
    let _ = run(&upstream_mesh_path("_bernoulli_beam_2.msh"), &out);
    let files = tree(&out);
    let pvd: Vec<&String> = files.iter().filter(|f| f.ends_with(".pvd")).collect();
    assert_eq!(
        pvd,
        vec![&format!("paraview/{ID}.pvd")],
        "only the default dumper writes a .pvd (upstream setTimeStep); files: {files:?}"
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
/// and the state after the step bitwise.
#[test]
fn dump_set_time_step_matches_the_oracle_byte_for_byte() {
    let dir = fixtures_root().join("model/structural").join(ID);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    let out = scratch("oracle");
    let model = run(&f.input_path("mesh").unwrap(), &out);

    let mut n = 0;
    for (name, got) in [
        (
            "final/displacement",
            model.displacement().unwrap().as_slice().to_vec(),
        ),
        (
            "final/internal_force",
            model.internal_force().unwrap().as_slice().to_vec(),
        ),
        (
            "final/stress/_bernoulli_beam_2",
            model.stress(TYPE, G).unwrap().as_slice().to_vec(),
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
        "the oracle writes the default dumper's .pvd: {want:?}"
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
