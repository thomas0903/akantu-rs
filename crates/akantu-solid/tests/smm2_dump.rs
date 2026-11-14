// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). MODEL-009 / SMM-017
// / SMM-001: end-to-end dump differential of SolidMechanicsModel against
// the oracle cases fixtures/model/smm2/dump_* (oracle/probes/smm2): the
// model computes its state itself (explicit / static / finite-deformation
// runs, two materials), its dumpers resolve every model field from the live
// state (upstream solid_mechanics_model_io.cc createNodalFieldReal/Bool,
// createElementalField, onDump), and every file written (ParaView .pvtu,
// .vtu, .pvd; base64 and text modes) must equal the oracle's byte for byte.
// Also the list of files: a dumper that writes a .pvd upstream does not is
// a difference.

mod model_common;

use std::path::{Path, PathBuf};

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_core::{ElementKind, GhostType, Real};
use akantu_io::DumperParaview;
use akantu_io::dumper::MeshSelection;
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromSameDim};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;
use akantu_validation::Fixture;
use model_common::*;

const G: GhostType = GhostType::NotGhost;

fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// Model of the fixture: mesh and material file from its inputs, materials
/// selected by `physical_names` (upstream `MeshDataMaterialSelector`).
fn model_of(f: &Fixture, dim: usize, method: AnalysisMethod) -> SolidMechanicsModel {
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut m = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        parser,
    )
    .unwrap();
    m.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    m.init_full(&eigen_options(method)).unwrap();
    m
}

fn fixed(m: &mut SolidMechanicsModel, value: Real, axis: SpatialDirection, group: &str) {
    m.apply_dirichlet_bc(&FixedValue { value, axis }, group)
        .unwrap();
}

/// The probe's `velocity_y` initial velocity.
fn velocity_y(m: &mut SolidMechanicsModel) {
    let dim = m.spatial_dimension();
    let x: Vec<Real> = {
        let mesh_h = m.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let mut v_g = m.velocity_mut().unwrap();
    let v = v_g.as_mut_slice();
    for (n, vn) in v.chunks_exact_mut(dim).enumerate() {
        let y = x[n * dim + 1];
        vn[0] = 0.01 * y + 0.002;
        vn[1] = -0.005 * y;
        if dim == 3 {
            vn[2] = 0.003 * y - 0.001;
        }
    }
}

fn scratch(name: &str) -> PathBuf {
    let d = PathBuf::from(env!("CARGO_TARGET_TMPDIR"))
        .join("smm2_dump")
        .join(name);
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

fn files_under(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                let rel = p.strip_prefix(base).unwrap();
                let s: Vec<String> = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy().into_owned())
                    .collect();
                out.push(s.join("/"));
            }
        }
    }
    let mut v = Vec::new();
    walk(dir, dir, &mut v);
    v.sort();
    v
}

/// Every file of the oracle's dump tree byte for byte, and the same list
/// of files. Returns the number of bytes compared.
fn compare_tree(f: &Fixture, dir: &Path) -> usize {
    compare_tree_except(f, dir, &[])
}

/// [`compare_tree`] without the byte comparison of the files in `except`
/// (documented differences, checked by the caller; they must still be
/// written by both sides).
fn compare_tree_except(f: &Fixture, dir: &Path, except: &[&str]) -> usize {
    let want: Vec<String> = param(f, "dump_files")
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_owned())
        .collect();
    let got = files_under(dir);
    let mut bytes = 0;
    let mut problems = Vec::new();
    for rel in &want {
        let w = std::fs::read(f.input_path(&format!("dump:{rel}")).unwrap()).unwrap();
        match std::fs::read(dir.join(rel)) {
            Ok(_) if except.contains(&rel.as_str()) => {}
            Ok(g) if g == w => bytes += w.len(),
            Ok(g) => {
                let at = g
                    .iter()
                    .zip(&w)
                    .position(|(a, b)| a != b)
                    .unwrap_or(g.len().min(w.len()));
                let lo = at.saturating_sub(80);
                problems.push(format!(
                    "{rel}: differs at byte {at} (got {} bytes, want {})\n  got:  {:?}\n  want: {:?}",
                    g.len(),
                    w.len(),
                    String::from_utf8_lossy(&g[lo..(at + 80).min(g.len())]),
                    String::from_utf8_lossy(&w[lo..(at + 80).min(w.len())])
                ));
            }
            Err(e) => problems.push(format!("{rel}: not written ({e})")),
        }
    }
    for rel in got.iter().filter(|g| !want.contains(g)) {
        problems.push(format!("{rel}: written by the port, not by the oracle"));
    }
    assert!(
        problems.is_empty(),
        "{}:\n{}",
        f.case_id(),
        problems.join("\n")
    );
    bytes
}

/// Upstream's file layout for the comparison (UD-110/113/114: the port
/// corrects the quadrature-field and padded-tensor layout by default;
/// `set_compat_layout` reproduces upstream's bytes).
fn compat_layout(m: &mut SolidMechanicsModel, dumper: &str) {
    m.dumpers
        .dumpable
        .dumper_as::<DumperParaview>(dumper)
        .unwrap()
        .set_compat_layout(true);
}

fn add_extra_dumper(
    m: &mut SolidMechanicsModel,
    name: &str,
    file: &str,
    dim: usize,
    dir: &Path,
    text: bool,
) {
    let d = &mut m.dumpers.dumpable;
    d.register_paraview(name, Some(file), false);
    d.add_dump_mesh_to_dumper(name, MeshSelection::new(Some(dim), G, ElementKind::Regular))
        .unwrap();
    d.set_directory_to_dumper(name, dir).unwrap();
    if text {
        d.set_text_mode_to_dumper(name).unwrap();
    }
    compat_layout(m, name);
}

fn dump_explicit_2d(id: &str, fd: bool) -> usize {
    let f = case(&format!("smm2/{id}"));
    let mut m = model_of(&f, 2, AnalysisMethod::ExplicitLumpedMass);
    fixed(&mut m, 0.0, SpatialDirection::X, "left");
    fixed(&mut m, 0.0, SpatialDirection::Y, "left");
    m.apply_neumann_bc(&FromSameDim::new(vec![1e6, -2e6]), "top")
        .unwrap();
    velocity_y(&mut m);
    let stable = m.stable_time_step().unwrap();
    let dt = 0.5 * stable;
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    // as the probe: the lumped mass exists before "mass" is registered
    m.assemble_mass_lumped().unwrap();

    let out = scratch(id);
    let pv = out.join("paraview");
    m.set_base_name(id).unwrap();
    m.set_directory(&pv).unwrap();
    compat_layout(&mut m, "solid_mechanics_model");
    let fields: &[&str] = if fd {
        &[
            "displacement",
            "velocity",
            "stress",
            "grad_u",
            "Green strain",
            "Von Mises stress",
            "material_index",
        ]
    } else {
        &[
            "displacement",
            "velocity",
            "acceleration",
            "external_force",
            "internal_force",
            "mass",
            "increment",
            "blocked_dofs",
            "material_index",
            "partitions",
            "stress",
            "grad_u",
            "strain",
            "Green strain",
            "principal strain",
            "principal Green strain",
            "Von Mises stress",
            "potential_energy",
        ]
    };
    for name in fields {
        m.add_dump_field(name).unwrap();
    }
    add_extra_dumper(&mut m, "extra", &format!("{id}_extra"), 2, &pv, true);
    for name in [
        "displacement",
        "stress",
        "grad_u",
        "Von Mises stress",
        "external_force",
        "material_index",
        "blocked_dofs",
    ] {
        m.add_dump_field_to_dumper("extra", name).unwrap();
    }
    m.set_time_step(dt, None).unwrap();
    add_extra_dumper(&mut m, "padded", &format!("{id}_padded"), 2, &pv, false);
    m.add_dump_field_vector_to_dumper("padded", "displacement")
        .unwrap();
    m.add_dump_field_vector_to_dumper("padded", "velocity")
        .unwrap();
    m.add_dump_field_tensor_to_dumper("padded", "grad_u")
        .unwrap();

    m.energy("potential").unwrap();
    m.dump(None, None).unwrap();
    m.dump_named("extra", None, None).unwrap();
    for k in 1..=3 {
        m.solve_step(None).unwrap();
        m.energy("potential").unwrap();
        match k {
            1 => {
                m.dump(None, None).unwrap();
                m.dump_named("extra", None, None).unwrap();
                m.dump_named("padded", None, None).unwrap();
            }
            2 => {
                m.dump(None, Some(5)).unwrap();
                m.dump_named("extra", None, Some(7)).unwrap();
            }
            _ => {
                m.dump(Some(0.75), Some(9)).unwrap();
                m.dump_named("padded", Some(0.25), Some(2)).unwrap();
                m.dump_named("extra", Some(1.5), Some(3)).unwrap();
            }
        }
    }
    n += assert_bits(&f, "displacement", m.displacement().unwrap().as_slice());
    n += assert_bits(&f, "velocity", m.velocity().unwrap().as_slice());
    let bytes = compare_tree(&f, &out);
    eprintln!("{id}: {n} state values bitwise, {bytes} dump bytes identical");
    bytes
}

#[test]
fn dump_explicit_2d_matches_the_oracle_byte_for_byte() {
    dump_explicit_2d("dump_explicit_2d", false);
}

/// Finite deformation: the materials' `onDump` (Cauchy stresses) runs
/// before the dump (upstream `Material::onDump`, `BeforeDumpEvent`).
#[test]
fn dump_fd_2d_matches_the_oracle_byte_for_byte() {
    dump_explicit_2d("dump_fd_2d", true);
}

#[test]
fn dump_static_3d_matches_the_oracle_byte_for_byte() {
    let f = case("smm2/dump_static_3d");
    let mut m = model_of(&f, 3, AnalysisMethod::Static);
    set_newton(&mut m, 5, 1e-10, "solution");
    for ax in [
        SpatialDirection::X,
        SpatialDirection::Y,
        SpatialDirection::Z,
    ] {
        fixed(&mut m, 0.0, ax, "left");
    }
    fixed(&mut m, 1e-4, SpatialDirection::X, "right");
    m.apply_neumann_bc(&FromSameDim::new(vec![1e5, -2e5, -3e5]), "top")
        .unwrap();
    let out = scratch("dump_static_3d");
    let pv = out.join("paraview");
    m.set_base_name("dump_static_3d").unwrap();
    m.set_directory(&pv).unwrap();
    compat_layout(&mut m, "solid_mechanics_model");
    for name in [
        "displacement",
        "external_force",
        "internal_force",
        "increment",
        "blocked_dofs",
        "material_index",
        "partitions",
        "stress",
        "grad_u",
        "strain",
        "Green strain",
        "principal strain",
        "principal Green strain",
        "Von Mises stress",
        "potential_energy",
    ] {
        m.add_dump_field(name).unwrap();
    }
    add_extra_dumper(&mut m, "tensor", "dump_static_3d_tensor", 3, &pv, true);
    m.add_dump_field_vector_to_dumper("tensor", "displacement")
        .unwrap();
    for name in ["stress", "strain", "Green strain"] {
        m.add_dump_field_tensor_to_dumper("tensor", name).unwrap();
    }
    m.dump(None, None).unwrap();
    m.solve_step(None).unwrap();
    m.energy("potential").unwrap();
    m.dump(None, None).unwrap();
    m.dump_named("tensor", None, None).unwrap();
    let n = assert_bits(&f, "displacement", m.displacement().unwrap().as_slice());
    let bytes = compare_tree(&f, &out);
    eprintln!("dump_static_3d: {n} values bitwise, {bytes} dump bytes identical");
}

// ------------------------------------------- group dumper API (MODEL-009)

/// MODEL-009: the model-level group dumper API
/// (upstream `model.cc:100-144, 255-339`) against the probe case
/// `dump_group_2d`, every file byte for byte: `set_all_groups_directory`
/// / `set_group_directory` / `set_group_base_name`,
/// `add_dump_group_field[_vector]` on boundary groups (nodal fields) and
/// volume groups (elemental `stress`, `material_index`),
/// `with_group_dumper` (upstream `getGroupDumper(group).setTimeStep`), a
/// second (text) dumper of a group with
/// `add_dump_group_field[_vector]_to_dumper`, `dump_group`,
/// `dump_group_named` and `dump_all_groups` (every element group, upstream
/// `dumpGroup()`), interleaved with `dump` of the model's default dumper.
/// The group dumpers live in the mesh domain's registry
/// (`SharedMeshDumpers`).
///
/// UD-951 (B): upstream's `dumpGroup` does not refresh the flattened
/// material internals (`onDump` runs only in `SolidMechanicsModel::dump`),
/// so the one group dump of `steel` made after a step and before the next
/// model dump holds the stress of the previous model dump: the oracle's
/// file equals its step-0 file (zero stress), while the port reads the live
/// stress and its file equals the oracle's next (refreshed) `steel` file.
#[test]
fn dump_group_2d_matches_the_oracle_byte_for_byte() {
    use akantu_io::dumper::MeshFilter;
    let id = "dump_group_2d";
    let f = case(&format!("smm2/{id}"));
    let mut m = model_of(&f, 2, AnalysisMethod::ExplicitLumpedMass);
    fixed(&mut m, 0.0, SpatialDirection::X, "left");
    fixed(&mut m, 0.0, SpatialDirection::Y, "left");
    m.apply_neumann_bc(&FromSameDim::new(vec![1e6, -2e6]), "top")
        .unwrap();
    velocity_y(&mut m);
    let stable = m.stable_time_step().unwrap();
    let dt = 0.5 * stable;
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    m.set_time_step(dt, None).unwrap();

    let out = scratch(id);
    let pv = out.join("paraview");
    m.set_base_name("dump_group_2d_model").unwrap();
    m.set_directory(&out.join("model")).unwrap();
    compat_layout(&mut m, "solid_mechanics_model");
    m.set_all_groups_directory(&pv).unwrap();
    m.set_group_directory(&out.join("top"), "top").unwrap();
    m.set_group_base_name("top_side", "top").unwrap();
    m.add_dump_group_field("displacement", "top").unwrap();
    m.add_dump_group_field_vector("velocity", "top").unwrap();
    m.add_dump_group_field("external_force", "top").unwrap();
    m.add_dump_group_field("blocked_dofs", "left").unwrap();
    m.add_dump_group_field_vector("displacement", "left")
        .unwrap();
    m.add_dump_group_field("stress", "steel").unwrap();
    m.add_dump_group_field("material_index", "steel").unwrap();
    m.add_dump_group_field_vector("velocity", "alu").unwrap();
    m.add_dump_group_field("internal_force", "alu").unwrap();
    m.with_group_dumper("top", |d| d.set_time_step(dt))
        .unwrap()
        .unwrap();
    m.with_group_dumpable("bottom", |g| {
        g.register_paraview("bottom_text", Some("bottom_text"), false);
        g.add_dump_mesh_to_dumper(
            "bottom_text",
            MeshSelection::new(None, G, ElementKind::NotDefined)
                .filtered(MeshFilter::ElementGroup("bottom".into())),
        )?;
        g.set_directory_to_dumper("bottom_text", &pv)?;
        g.set_text_mode_to_dumper("bottom_text")
    })
    .unwrap()
    .unwrap();
    m.add_dump_group_field_to_dumper("bottom_text", "displacement", "bottom", false)
        .unwrap();
    m.add_dump_group_field_vector_to_dumper("bottom_text", "velocity", "bottom")
        .unwrap();
    // upstream's file layout (UD-110/113/114) on every group dumper
    for (group, dumper) in [
        ("alu", "paraview_alu"),
        ("bottom", "paraview_bottom"),
        ("bottom", "bottom_text"),
        ("left", "paraview_left"),
        ("steel", "paraview_steel"),
        ("top", "paraview_top"),
    ] {
        m.with_group_dumpable(group, |g| {
            g.dumper_as::<DumperParaview>(dumper)
                .map(|p| p.set_compat_layout(true))
        })
        .unwrap()
        .unwrap();
    }

    m.dump(None, None).unwrap();
    m.dump_group("top").unwrap();
    m.dump_all_groups().unwrap();
    m.dump_group_named("bottom", "bottom_text").unwrap();
    for k in 1..=2 {
        m.solve_step(None).unwrap();
        if k == 1 {
            m.dump_group("steel").unwrap(); // UD-951 (see above)
        }
        m.dump(None, None).unwrap();
        m.dump_group("top").unwrap();
        m.dump_group("left").unwrap();
        m.dump_group("steel").unwrap();
        m.dump_group("alu").unwrap();
        m.dump_group_named("bottom", "bottom_text").unwrap();
    }
    m.dump(None, None).unwrap();
    m.dump_all_groups().unwrap();
    n += assert_bits(&f, "displacement", m.displacement().unwrap().as_slice());
    n += assert_bits(&f, "velocity", m.velocity().unwrap().as_slice());
    let stale = param(&f, "ud951_stale_file").as_str().unwrap().to_owned();
    assert_eq!(stale, "paraview/steel-VTU/steel_0001.vtu");
    let bytes = compare_tree_except(&f, &out, &[stale.as_str()]);
    let oracle = |rel: &str| std::fs::read(f.input_path(&format!("dump:{rel}")).unwrap()).unwrap();
    assert_eq!(
        oracle(&stale),
        oracle("paraview/steel-VTU/steel_0000.vtu"),
        "UD-951: upstream's stale group dump repeats the step-0 stress"
    );
    assert_ne!(
        oracle(&stale),
        oracle("paraview/steel-VTU/steel_0002.vtu"),
        "the refreshed dump at the same state differs (non-zero stress)"
    );
    assert_eq!(
        std::fs::read(out.join(&stale)).unwrap(),
        oracle("paraview/steel-VTU/steel_0002.vtu"),
        "UD-951: the port's group dump reads the live stress"
    );
    eprintln!("{id}: {n} state values bitwise, {bytes} dump bytes identical");
}

/// Errors of the group dumper API (upstream throws): unknown groups and
/// dumpers, a field removed that was never added, a duplicate field;
/// a removed field can be added again.
#[test]
fn group_dumper_api_errors() {
    let f = case("smm2/dump_explicit_2d");
    let mut m = model_of(&f, 2, AnalysisMethod::ExplicitLumpedMass);
    assert!(
        m.add_dump_group_field("displacement", "no_such_group")
            .is_err()
    );
    assert!(m.dump_group("no_such_group").is_err());
    assert!(m.dump_group_named("top", "no_such_dumper").is_err());
    assert!(m.remove_dump_group_field("displacement", "top").is_err());
    assert!(
        m.add_dump_group_field_to_dumper("no_such_dumper", "displacement", "top", false)
            .is_err()
    );
    assert!(m.set_group_base_name("x", "no_such_group").is_err());
    m.add_dump_group_field("displacement", "top").unwrap();
    assert!(m.add_dump_group_field("displacement", "top").is_err());
    m.remove_dump_group_field("displacement", "top").unwrap();
    m.add_dump_group_field("displacement", "top").unwrap();
}

// ------------------------------------------------- registration (UD-683)

/// The model of `dump_explicit_2d` after `init_full` (explicit, lumped
/// mass not yet assembled), dumping into `dir` in upstream's layout.
fn explicit_2d_model(dir: &Path) -> SolidMechanicsModel {
    let f = case("smm2/dump_explicit_2d");
    let mut m = model_of(&f, 2, AnalysisMethod::ExplicitLumpedMass);
    m.set_directory(dir).unwrap();
    m.set_base_name("reg").unwrap();
    compat_layout(&mut m, "solid_mechanics_model");
    m
}

/// UD-683, category A: a dump field name that can never
/// be resolved — neither a model field, nor an internal of a material, nor
/// numeric elemental mesh data — is refused when it is registered, as
/// upstream (`addDumpGroupFieldToDumper` → `createFieldFromAttachedData<Int>`
/// throws), and the dumper stays usable. Before the fix the name was
/// recorded and every later dump of that dumper failed without writing
/// anything.
#[test]
fn ud683_never_resolvable_dump_field_is_refused_at_registration() {
    let dir = scratch("ud683_refused");
    let mut m = explicit_2d_model(&dir);
    m.add_dump_field("displacement").unwrap();
    for name in ["no_such_field", "connectivity", "physical_names"] {
        let e = m.add_dump_field(name).unwrap_err();
        assert!(e.to_string().contains("UD-683"), "{name}: {e}");
        assert!(m.add_dump_field_vector(name).is_err(), "{name}");
        assert!(
            m.add_dump_field_to_dumper("solid_mechanics_model", name)
                .is_err(),
            "{name}"
        );
    }
    // an internal is only known once the materials exist
    let mesh = read_mesh(
        &case("smm2/dump_explicit_2d").input_path("mesh").unwrap(),
        2,
    );
    let parser = akantu_parser::Parser::parse_file(
        case("smm2/dump_explicit_2d")
            .input_path("material")
            .unwrap(),
    )
    .unwrap();
    let mut early =
        SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "early", parser).unwrap();
    assert!(early.add_dump_field("stress").is_err(), "before init_full");
    // the dumper is still usable: two dumps write their files
    m.dump(None, None).unwrap();
    m.dump(None, None).unwrap();
    let files = files_under(&dir);
    assert!(
        files.iter().filter(|f| f.ends_with(".vtu")).count() == 2,
        "{files:?}"
    );
}

/// UD-683, category C (kept): a known model array that does not exist yet
/// (`mass` before the first lumped-mass assembly) is accepted at
/// registration (upstream refuses it) and read from the live model at
/// each dump: a dump before the assembly is an error of that dump only,
/// `remove_dump_field` recovers the dumper, and once the mass exists the
/// field is written.
#[test]
fn ud683_model_array_allocated_later_is_resolved_at_dump_time() {
    let dir = scratch("ud683_deferred");
    let mut m = explicit_2d_model(&dir);
    m.add_dump_field("displacement").unwrap();
    m.add_dump_field("mass").unwrap();
    let e = m.dump(None, None).unwrap_err();
    assert!(e.to_string().contains("mass"), "{e}");
    m.remove_dump_field("mass").unwrap();
    m.dump(None, None).unwrap();
    m.assemble_mass_lumped().unwrap();
    m.add_dump_field("mass").unwrap();
    m.dump(None, None).unwrap();
    let vtus: Vec<String> = files_under(&dir)
        .into_iter()
        .filter(|f| f.ends_with(".vtu"))
        .collect();
    assert_eq!(vtus.len(), 2, "{vtus:?}");
    let last = std::fs::read_to_string(dir.join(vtus.last().unwrap())).unwrap();
    assert!(
        last.contains("Name=\"mass\""),
        "mass written once it exists"
    );
}

// ------------------------------------------------- layouts

/// Parse the ascii `<DataArray ... Name="X" ...>` blocks of a text VTU:
/// `name -> (components, values)`.
fn ascii_arrays(text: &str) -> std::collections::BTreeMap<String, (usize, Vec<f64>)> {
    let mut out = std::collections::BTreeMap::new();
    let mut rest = text;
    while let Some(i) = rest.find("<DataArray") {
        rest = &rest[i..];
        let end_tag = rest.find('>').unwrap();
        let tag = &rest[..end_tag];
        let attr = |k: &str| {
            tag.find(&format!("{k}=\"")).map(|p| {
                let s = &tag[p + k.len() + 2..];
                s[..s.find('"').unwrap()].to_owned()
            })
        };
        let name = attr("Name").unwrap_or_default();
        let ncomp = attr("NumberOfComponents")
            .and_then(|s| s.parse().ok())
            .unwrap_or(1);
        let body_end = rest.find("</DataArray>").unwrap();
        let body = &rest[end_tag + 1..body_end];
        let vals: Vec<f64> = body
            .split_whitespace()
            .filter_map(|t| t.parse().ok())
            .collect();
        out.insert(name, (ncomp, vals));
        rest = &rest[body_end..];
    }
    out
}

/// Two explicit steps of the `dump_explicit_2d` scenario, then one dump of
/// every quadrature / derived field through a text-mode dumper in
/// upstream's (compat) or the corrected default layout.
fn layout_run(compat: bool) -> std::collections::BTreeMap<String, (usize, Vec<f64>)> {
    let f = case("smm2/dump_explicit_2d");
    let mut m = model_of(&f, 2, AnalysisMethod::ExplicitLumpedMass);
    fixed(&mut m, 0.0, SpatialDirection::X, "left");
    fixed(&mut m, 0.0, SpatialDirection::Y, "left");
    m.apply_neumann_bc(&FromSameDim::new(vec![1e6, -2e6]), "top")
        .unwrap();
    let dt = 0.5 * m.stable_time_step().unwrap();
    m.set_time_step(dt, None).unwrap();
    for _ in 0..2 {
        m.solve_step(None).unwrap();
    }
    let dir = scratch(if compat {
        "layout_compat"
    } else {
        "layout_default"
    });
    {
        let d = &mut m.dumpers.dumpable;
        d.register_paraview("t", Some("t"), false);
        d.add_dump_mesh_to_dumper("t", MeshSelection::new(Some(2), G, ElementKind::Regular))
            .unwrap();
        d.set_directory_to_dumper("t", &dir).unwrap();
        d.set_text_mode_to_dumper("t").unwrap();
        d.dumper_as::<DumperParaview>("t")
            .unwrap()
            .set_compat_layout(compat);
    }
    for name in [
        "displacement",
        "stress",
        "grad_u",
        "strain",
        "Green strain",
        "principal strain",
        "Von Mises stress",
        "material_index",
    ] {
        m.add_dump_field_to_dumper("t", name).unwrap();
    }
    m.add_dump_field_vector_to_dumper("t", "velocity").unwrap();
    m.dump_named("t", None, None).unwrap();
    let vtu = files_under(&dir)
        .into_iter()
        .find(|n| n.ends_with(".vtu"))
        .unwrap();
    ascii_arrays(&std::fs::read_to_string(dir.join(vtu)).unwrap())
}

/// The dump differentials compare upstream's (compat) layout; the corrected
/// default layout carries the same values with the zero padding removed
/// (same order, never more components), so the byte comparisons evidence
/// the default layout too (adopted from its reproducer).
#[test]
fn default_layout_is_the_compat_layout_without_padding() {
    let compat = layout_run(true);
    let default = layout_run(false);
    let mut problems = Vec::new();
    for (name, (nc, vc)) in &compat {
        let Some((nd, vd)) = default.get(name) else {
            problems.push(format!("{name}: only in the compat layout"));
            continue;
        };
        let nzc: Vec<f64> = vc.iter().copied().filter(|x| *x != 0.0).collect();
        let nzd: Vec<f64> = vd.iter().copied().filter(|x| *x != 0.0).collect();
        if nzc.is_empty() && name != "material_index" {
            problems.push(format!("{name}: no non-zero value (vacuous comparison)"));
        }
        if nzc != nzd {
            problems.push(format!("{name}: the non-zero values differ"));
        }
        if nd > nc {
            problems.push(format!("{name}: more components in the default layout"));
        }
    }
    for name in default.keys() {
        if !compat.contains_key(name) {
            problems.push(format!("{name}: only in the default layout"));
        }
    }
    assert!(compat.len() >= 9, "arrays: {:?}", compat.keys());
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

// ------------------------------------------------- padded 2-D tensors

/// Standard base64 decoding (white space and `=` skipped).
fn base64_decode(s: &str) -> Vec<u8> {
    let val = |c: u8| -> Option<u32> {
        match c {
            b'A'..=b'Z' => Some(u32::from(c - b'A')),
            b'a'..=b'z' => Some(u32::from(c - b'a') + 26),
            b'0'..=b'9' => Some(u32::from(c - b'0') + 52),
            b'+' => Some(62),
            b'/' => Some(63),
            _ => None,
        }
    };
    let (mut buf, mut bits, mut out) = (0u32, 0u32, Vec::new());
    for v in s.bytes().filter_map(val) {
        buf = ((buf << 6) | v) & 0x00FF_FFFF;
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            out.push(((buf >> bits) & 0xFF) as u8);
        }
    }
    out
}

/// The base64 `Float64` data array `name` of a VTU file: (components,
/// values), the IOHelper header (`payload bytes + 4`) checked.
fn vtu_base64_array(file: &Path, name: &str) -> (usize, Vec<f64>) {
    let text = std::fs::read_to_string(file).unwrap();
    let at = text
        .find(&format!("Name=\"{name}\""))
        .unwrap_or_else(|| panic!("{}: no array {name}", file.display()));
    let tag_begin = text[..at].rfind("<DataArray").unwrap();
    let tag_end = at + text[at..].find('>').unwrap();
    let tag = &text[tag_begin..tag_end];
    assert!(tag.contains("format=\"binary\"") && tag.contains("type=\"Float64\""));
    let nc = tag
        .find("NumberOfComponents=\"")
        .map(|p| {
            let s = &tag[p + 20..];
            s[..s.find('"').unwrap()].parse().unwrap()
        })
        .unwrap_or(1);
    let body_end = tag_end + text[tag_end..].find("</DataArray>").unwrap();
    let bytes = base64_decode(&text[tag_end + 1..body_end]);
    let header = i32::from_le_bytes(bytes[..4].try_into().unwrap());
    assert_eq!(
        usize::try_from(header).unwrap(),
        bytes.len(),
        "IOHelper header"
    );
    let (words, rest) = bytes[4..].as_chunks::<8>();
    assert!(rest.is_empty(), "payload of whole doubles");
    let values = words.iter().map(|b| f64::from_le_bytes(*b)).collect();
    (nc, values)
}

/// Padded 2-D stress and strain through the model's dumpers
/// (oracle case `fixtures/model/smm2/dump_padded_2d`): the
/// `dump_explicit_2d` scenario (plane-strain steel, plane-stress alu) after
/// two explicit steps.
/// - `stress` (`add_dump_field_tensor_to_dumper`, upstream `StressPadder<2>`
///   then the homogeniser, compat layout = a 9 × 9 block per element): every
///   deterministic entry bitwise (in-plane stresses, plane-strain
///   `σzz = ν(σxx + σyy)` with the element's `nu` read from the live
///   model, IOHelper's zero padding); the entries upstream never writes
///   (out-of-plane shears, plane-stress `σzz`: uninitialised memory,
///   DUMP-029 / UD-111) must hold the port's corrected value `+0`.
/// - `strain` padded: upstream aborts at registration (UD-112), so the
///   port's result (default layout, 9 components per element) is compared
///   with an independent recomputation from the oracle's `grad_u`:
///   `ε = (∇u + ∇uᵀ)/2`, `εzz = ν/(ν−1)(εxx + εyy)` for plane stress and 0
///   for plane strain, averaged over the quadrature points.
#[test]
fn dump_padded_2d_matches_the_oracle() {
    let f = case("smm2/dump_padded_2d");
    let mut m = model_of(&f, 2, AnalysisMethod::ExplicitLumpedMass);
    fixed(&mut m, 0.0, SpatialDirection::X, "left");
    fixed(&mut m, 0.0, SpatialDirection::Y, "left");
    m.apply_neumann_bc(&FromSameDim::new(vec![1e6, -2e6]), "top")
        .unwrap();
    velocity_y(&mut m);
    let stable = m.stable_time_step().unwrap();
    let dt = 0.5 * stable;
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    m.set_time_step(dt, None).unwrap();
    for _ in 0..2 {
        m.solve_step(None).unwrap();
    }
    let ty = akantu_core::ElementType::Quadrangle4;
    let nb_mat = m.nb_materials();
    // per-material grad_u and stress (law order) bitwise, filters equal
    let mut el_of = Vec::new(); // (material, filter, grad_u)
    for mi in 0..nb_mat {
        let law = m.material(mi).unwrap();
        let filter: Vec<i64> = law
            .base()
            .element_filter
            .get(ty, G)
            .unwrap()
            .as_slice()
            .iter()
            .map(|&e| i64::from(e))
            .collect();
        assert_eq!(filter, i64s(&f, &format!("material/{mi}/filter")));
        let g = law.base().grad_u.get(ty, G).unwrap().as_slice().to_vec();
        n += assert_bits(&f, &format!("material/{mi}/grad_u"), &g);
        n += assert_bits(
            &f,
            &format!("material/{mi}/stress"),
            law.base().stress.get(ty, G).unwrap().as_slice(),
        );
        el_of.push((mi, filter, f64s(&f, &format!("material/{mi}/grad_u"))));
    }

    let dir = scratch("dump_padded_2d");
    add_extra_dumper(&mut m, "pstress", "dump_padded_2d_pstress", 2, &dir, false);
    m.add_dump_field_tensor_to_dumper("pstress", "stress")
        .unwrap();
    {
        let d = &mut m.dumpers.dumpable;
        d.register_paraview("pstrain", Some("dump_padded_2d_pstrain"), false);
        d.add_dump_mesh_to_dumper(
            "pstrain",
            MeshSelection::new(Some(2), G, ElementKind::Regular),
        )
        .unwrap();
        d.set_directory_to_dumper("pstrain", &dir).unwrap();
    }
    m.add_dump_field_tensor_to_dumper("pstrain", "strain")
        .unwrap();
    m.dump_named("pstress", None, None).unwrap();
    m.dump_named("pstrain", None, None).unwrap();

    // padded stress against the oracle
    let (nc, got) = vtu_base64_array(
        &dir.join("dump_padded_2d_pstress-VTU/dump_padded_2d_pstress_0000.vtu"),
        "stress",
    );
    assert_eq!(nc, 9);
    assert_eq!(
        param(&f, "padded_stress_components").as_u64(),
        Some(9),
        "oracle components"
    );
    let want = f64s(&f, "padded_stress/values");
    let mask = i64s(&f, "padded_stress/mask");
    assert_eq!(got.len(), want.len(), "padded stress length");
    let (mut det, mut undef) = (0, 0);
    for (k, ((g, w), m)) in got.iter().zip(&want).zip(&mask).enumerate() {
        if *m == 1 {
            assert_eq!(
                g.to_bits(),
                w.to_bits(),
                "padded stress [{k}]: {g:e} vs {w:e}"
            );
            det += 1;
        } else {
            assert_eq!(
                g.to_bits(),
                0,
                "padded stress [{k}] (undefined upstream): {g:e}"
            );
            undef += 1;
        }
    }
    // the plane-strain sigma_zz entries are among the compared ones and
    // non-zero (the padder path is exercised, not only zeros)
    let mats = i64s(&f, "material_by_element");
    let szz: Vec<f64> = (0..mats.len())
        .filter(|&e| param(&f, &format!("material/{}/plane_stress", mats[e])) == false)
        .map(|e| got[e * 81 + 20])
        .collect();
    assert!(!szz.is_empty() && szz.iter().all(|&x| x != 0.0), "{szz:?}");

    // padded strain against the independent recomputation
    let (nc, strain) = vtu_base64_array(
        &dir.join("dump_padded_2d_pstrain-VTU/dump_padded_2d_pstrain_0000.vtu"),
        "strain",
    );
    assert_eq!(nc, 9);
    let nb_el = mats.len();
    assert_eq!(strain.len(), nb_el * 9);
    let mut checked = 0;
    for (mi, filter, g) in &el_of {
        let nu = param(&f, &format!("material/{mi}/nu")).as_f64().unwrap();
        let plane_stress = param(&f, &format!("material/{mi}/plane_stress"))
            .as_bool()
            .unwrap();
        let nq = g.len() / 4 / filter.len().max(1);
        for (l, &e) in filter.iter().enumerate() {
            let mut res = [0.0_f64; 9];
            for q in 0..nq {
                let gq = &g[(l * nq + q) * 4..(l * nq + q + 1) * 4];
                let exy = 0.5 * (gq[2] + gq[1]);
                let (exx, eyy) = (gq[0], gq[3]);
                let ezz = if plane_stress {
                    nu / (nu - 1.) * (exx + eyy)
                } else {
                    0.0
                };
                let block = [exx, exy, 0.0, exy, eyy, 0.0, 0.0, 0.0, ezz];
                for (r, b) in res.iter_mut().zip(block) {
                    *r += b;
                }
            }
            let e = usize::try_from(e).unwrap();
            for (c, r) in res.iter().enumerate() {
                let want = r / nq as f64;
                let got = strain[e * 9 + c];
                assert_eq!(
                    got.to_bits(),
                    want.to_bits(),
                    "padded strain element {e} component {c}: {got:e} vs {want:e}"
                );
                checked += 1;
            }
        }
    }
    eprintln!(
        "dump_padded_2d: {n} state values bitwise; padded stress {det} deterministic entries \
         bitwise, {undef} undefined-upstream entries = +0; padded strain {checked} entries = \
         recomputation"
    );
}
