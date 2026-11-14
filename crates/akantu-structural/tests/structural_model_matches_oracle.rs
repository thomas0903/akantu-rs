// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS).
// Differential tests against fixtures/model/structural/* (oracle probe
// oracle/probes/structural_model/structural_model_probe.cc, strict x86_64
// reference build, Eigen sparse solver pinned, three identical runs; probe
// version 2 pins Eigen's cache sizes, D-050).

//! Every case is rebuilt from its recorded inputs and replayed with
//!
//! * [`StructuralModelUpstreamCompat::UPSTREAM`] — every recorded value is
//!   compared **bitwise** (D-008, D-051): the assembled `K` / `M` (COO
//!   coordinates in storage order, then the values), the external forces of
//!   the line loads, the final displacements, velocities, accelerations,
//!   internal forces and generalised stresses, the per-step series (node
//!   displacement, kinetic and potential energies, Newton iterations), the
//!   full displacement at selected steps, the static energies, and the
//!   kinetic energy of an explicit-lumped analysis before its first step
//!   with the consistent `M` it uses (UD-510);
//! * for each upstream-compatibility switch S (UD-293, UD-343, UD-007,
//!   UD-507) and for the rotary-inertia option `IsotropicMax` (UD-059 is
//!   corrected without a switch, D-059 amendment; the
//!   option must change no compared group): every upstream switch except S
//!   — the replay must differ from
//!   the fixture in **exactly** the quantity groups the case declares for S
//!   ([`expected`]; no declaration means bitwise identical). Flipping any
//!   single correction back to upstream behaviour, or leaking a correction
//!   into another quantity, fails;
//! * the corrected default — exactly the union of the per-switch
//!   declarations.
//!
//! **Documented differences** (UD-005: upstream's lumped mass is
//! identically zero, `node2 = conn(0)`; no compat switch):
//! `lumped/mass`, `lumped/kinetic_energy` and
//! `pre/kinetic_energy_after_lumped` record zeros upstream. The replay
//! checks those records exactly and the port's corrected values against
//! their definition (positive lumped mass, `½ Σ m v²`); they are counted
//! separately and never as bitwise comparisons.
//! Value counts include only compared values: COO coordinates are checked
//! for equality and counted apart.

mod structural_model_common;

use std::collections::BTreeMap;

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{Array, ElementType, Idx};
use akantu_fe::structural::kernels::DktBendingStiffness;
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::ModelOptions;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;
use akantu_structural::{
    LumpedRotaryInertia, StructuralMaterial, StructuralMechanicsModel,
    StructuralModelUpstreamCompat,
};
use akantu_validation::{Fixture, fixtures_root};
use structural_model_common::G;

fn case(name: &str) -> Fixture {
    let dir = fixtures_root().join("model/structural").join(name);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

/// The solve-replay cases. The dump cases `dump_*`
/// are not solve replays: `structural_dump_set_time_step.rs`
/// replays them.
fn case_names() -> Vec<String> {
    let dir = fixtures_root().join("model/structural");
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .filter(|e| e.path().join("manifest.json").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| !n.starts_with("dump_"))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

fn f64s(f: &Fixture, name: &str) -> Vec<f64> {
    f.load_f64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

fn i64s(f: &Fixture, name: &str) -> Vec<i64> {
    f.load_i64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

fn shape(f: &Fixture, name: &str) -> Vec<usize> {
    f.load_f64(name)
        .map(|x| x.shape)
        .or_else(|_| f.load_i64(name).map(|x| x.shape))
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
}

fn bools(f: &Fixture, name: &str) -> Vec<bool> {
    let a = f.load(name).unwrap();
    let v = a
        .to_f64()
        .unwrap_or_else(|_| a.to_i64().unwrap().into_iter().map(|x| x as f64).collect());
    v.into_iter().map(|x| x != 0.0).collect()
}

fn has(f: &Fixture, name: &str) -> bool {
    f.quantity(name).is_ok()
}

fn param<'a>(f: &'a Fixture, key: &str) -> &'a serde_json::Value {
    &f.manifest().parameters[key]
}

/// Comparison record of one replay: per quantity `(values, differing,
/// max rel)`, the COO coordinates checked, the documented differences
/// checked, and hard failures (wrong documented records, shape errors).
#[derive(Default)]
struct Replay {
    quantities: BTreeMap<String, (usize, usize, f64)>,
    coordinates: usize,
    documented: usize,
    errors: Vec<String>,
}

/// Quantity group of a recorded name (per-step and per-type suffixes
/// folded).
fn group(name: &str) -> String {
    if let Some(rest) = name.strip_suffix("/values") {
        return rest.to_owned();
    }
    if name.starts_with("series/displacement_step_") {
        return "series/displacement_step".to_owned();
    }
    if name.starts_with("final/stress/") {
        return "final/stress".to_owned();
    }
    name.to_owned()
}

impl Replay {
    fn bits(&mut self, what: &str, got: &[f64], want: &[f64]) {
        if got.len() != want.len() {
            self.errors.push(format!(
                "{what}: {} values vs {} recorded",
                got.len(),
                want.len()
            ));
            return;
        }
        let scale = want.iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let mut n = 0;
        let mut worst = 0.0f64;
        for (x, y) in got.iter().zip(want) {
            if x.to_bits() != y.to_bits() {
                n += 1;
                worst = worst.max((x - y).abs() / scale.max(f64::MIN_POSITIVE));
            }
        }
        let e = self.quantities.entry(group(what)).or_default();
        e.0 += want.len();
        e.1 += n;
        e.2 = e.2.max(worst);
    }

    fn check(&mut self, f: &Fixture, name: &str, got: &[f64]) {
        let want = f64s(f, name);
        self.bits(name, got, &want);
    }

    /// Mark a group as differing without values (COO profile, iterations).
    fn differs(&mut self, what: &str, n: usize) {
        let e = self.quantities.entry(what.to_owned()).or_default();
        e.0 += n;
        e.1 += n;
    }

    fn compared(&self) -> usize {
        self.quantities.values().map(|v| v.0).sum()
    }

    /// Groups with at least one differing value (sorted).
    fn differing(&self) -> Vec<String> {
        self.quantities
            .iter()
            .filter(|(_, v)| v.1 > 0)
            .map(|(g, _)| g.clone())
            .collect()
    }

    fn report(&self, case: &str, what: &str) {
        for (g, v) in &self.quantities {
            if v.1 > 0 {
                eprintln!(
                    "{case} [{what}]: {g}: {} of {} differ (max rel {:e})",
                    v.1, v.0, v.2
                );
            }
        }
    }
}

fn element_type(name: &str) -> ElementType {
    name.parse()
        .unwrap_or_else(|_| panic!("element type {name}"))
}

fn types_of(f: &Fixture) -> Vec<ElementType> {
    param(f, "mesh_types")
        .as_array()
        .unwrap()
        .iter()
        .map(|t| element_type(t.as_str().unwrap()))
        .collect()
}

/// The probe records upstream's `AnalysisMethod` enumerator value
/// (`std::to_string` of the unscoped enum).
fn method_of(f: &Fixture) -> AnalysisMethod {
    match param(f, "analysis_method").as_str().unwrap() {
        "0" => AnalysisMethod::Static,
        "1" => AnalysisMethod::ImplicitDynamic,
        "2" => AnalysisMethod::ExplicitLumpedMass,
        "3" => AnalysisMethod::ExplicitConsistentMass,
        other => panic!("{}: analysis method {other}", f.case_id()),
    }
}

/// The recorded mesh (nodes and connectivities); for upstream meshes the
/// reader must give the same arrays.
fn mesh_of(f: &Fixture) -> Mesh {
    let dim = param(f, "spatial_dimension").as_u64().unwrap() as usize;
    let mut mesh = Mesh::new(dim, "structural_oracle").unwrap();
    {
        let nodes = f64s(f, "mesh/nodes");
        let mut acc = MeshAccessor::new(&mut mesh);
        for x in nodes.chunks(dim) {
            acc.push_node(x).unwrap();
        }
        for ty in types_of(f) {
            let name = format!("mesh/connectivity/{}", ty.name());
            let c = i64s(f, &name);
            let nn = shape(f, &name)[1];
            let mut block = acc.connectivity_mut(ty, G);
            for row in c.chunks(nn) {
                let r: Vec<Idx> = row.iter().map(|&v| v as Idx).collect();
                block.push_tuple(&r);
            }
        }
    }
    if let Ok(path) = f.input_path("mesh") {
        let mut read = Mesh::new(dim, "read").unwrap();
        akantu_io::mesh_io::read_mesh(&mut read, &path, akantu_core::enums::MeshIoType::GmshStruct)
            .unwrap();
        assert_eq!(read.nodes().as_slice(), mesh.nodes().as_slice());
        for ty in types_of(f) {
            assert_eq!(
                read.connectivity(ty, G).unwrap().as_slice(),
                mesh.connectivity(ty, G).unwrap().as_slice()
            );
        }
    }
    for ty in types_of(f) {
        let name = format!("extra_normal/{}", ty.name());
        if has(f, &name) {
            let v = f64s(f, &name);
            mesh.data_mut()
                .reals_mut_or_insert(akantu_fe::structural::EXTRA_NORMAL)
                .unwrap()
                .as_map_mut()
                .replace(ty, G, Array::from_vec(v, dim).unwrap().into());
        }
    }
    mesh
}

fn materials_of(f: &Fixture) -> Vec<StructuralMaterial> {
    f64s(f, "materials")
        .chunks(9)
        .map(|m| StructuralMaterial {
            e: m[0],
            a: m[1],
            i: m[2],
            iz: m[3],
            iy: m[4],
            gj: m[5],
            rho: m[6],
            t: m[7],
            nu: m[8],
        })
        .collect()
}

fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// The model of a fixture after `initFull` and the input assignment
/// (element materials, normals), before the boundary conditions.
fn model_of(
    f: &Fixture,
    compat: StructuralModelUpstreamCompat,
) -> Result<StructuralMechanicsModel, String> {
    let mut m = StructuralMechanicsModel::with_compat(
        akantu_mesh::SharedMesh::new(mesh_of(f)),
        None,
        f.case_id().rsplit('/').next().unwrap(),
        compat,
    )
    .map_err(|e| e.to_string())?;
    for mat in materials_of(f) {
        m.add_material(mat, "").unwrap();
    }
    m.init_full(&eigen_options(method_of(f)))
        .map_err(|e| e.to_string())?;
    for ty in types_of(f) {
        let em: Vec<u32> = i64s(f, &format!("element_material/{}", ty.name()))
            .into_iter()
            .map(|v| v as u32)
            .collect();
        m.element_material_mut(ty, G)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&em);
        assert_eq!(
            param(f, &format!("nb_quadrature_points/{}", ty.name()))
                .as_u64()
                .unwrap() as usize,
            m.fe_engine().nb_integration_points(ty, G).unwrap()
        );
    }
    Ok(m)
}

fn check_aij(c: &mut Replay, f: &Fixture, m: &StructuralMechanicsModel, prefix: &str, id: &str) {
    let key = format!("{prefix}/values");
    if !has(f, &key) {
        return;
    }
    let a = match m.dof_manager().matrix(id) {
        Ok(a) => a,
        Err(e) => {
            c.errors.push(format!("matrix {id}: {e}"));
            return;
        }
    };
    let (r, cl, v) = a.coo();
    let (rows, cols) = (
        i64s(f, &format!("{prefix}/rows")),
        i64s(f, &format!("{prefix}/cols")),
    );
    if r.to_vec() != rows || cl.to_vec() != cols {
        c.differs(&format!("{prefix}/profile"), rows.len());
        return;
    }
    c.coordinates += rows.len() + cols.len();
    if param(f, &format!("{prefix}/symmetric")).as_bool()
        != Some(a.matrix_type() == akantu_core::enums::MatrixType::Symmetric)
    {
        c.errors.push(format!("{prefix}: matrix type"));
    }
    c.check(f, &key, v);
}

/// `½ (Σ m v² + Σₙ ωₙᵀ Jₙ ωₙ off-diagonal)` of the port's lumped mass and
/// rotary blocks (documented UD-005 difference; D-059 blocks in 3-D with
/// the default `Block3x3`, diagonal blocks otherwise).
fn lumped_kinetic_energy(m: &StructuralMechanicsModel) -> f64 {
    structural_model_common::lumped_kinetic_energy_reference(m)
}

/// Replay one fixture (`Err`: refused at construction / initialisation).
fn replay(f: &Fixture, compat: StructuralModelUpstreamCompat) -> Result<Replay, String> {
    let mut c = Replay::default();
    let mut m = model_of(f, compat)?;
    let id = f.case_id().to_owned();
    if has(f, "pre/kinetic_energy") {
        // UD-510: explicit-lumped analysis, kinetic energy before any step:
        // upstream has a sparse M and C (no K, no lumped M) after initFull
        let p = &f.manifest().parameters;
        let dm = m.dof_manager();
        for (key, got) in [
            ("pre/has_matrix_M", dm.has_matrix("M")),
            ("pre/has_matrix_K", dm.has_matrix("K")),
            ("pre/has_matrix_C", dm.has_matrix("C")),
            ("pre/has_lumped_M", dm.has_lumped_matrix("M")),
        ] {
            if p[key].as_bool() != Some(got) {
                c.errors
                    .push(format!("{key}: port {got}, upstream {}", p[key]));
            }
        }
        let v = f64s(f, "pre/velocity");
        m.velocity_mut().unwrap().as_mut_slice().copy_from_slice(&v);
        let ek = m.energy("kinetic").map_err(|e| e.to_string())?;
        c.bits("pre/kinetic_energy", &[ek], &f64s(f, "pre/kinetic_energy"));
        if p["pre/has_lumped_M_after"].as_bool() != Some(m.dof_manager().has_lumped_matrix("M")) {
            c.errors.push("pre/has_lumped_M_after".into());
        }
        check_aij(&mut c, f, &m, "pre/M", "M");
        // documented difference (UD-005): upstream's lumped mass is zero
        if f64s(f, "pre/kinetic_energy_after_lumped") != [0.0] {
            c.errors.push(format!(
                "{id}: upstream lumped kinetic energy not zero (UD-005)"
            ));
        }
        m.assemble_lumped_mass_matrix().map_err(|e| e.to_string())?;
        let ek2 = m.energy("kinetic").map_err(|e| e.to_string())?;
        let want = lumped_kinetic_energy(&m);
        if !(ek2 > 0.0 && (ek2 - want).abs() <= 1e-13 * want) {
            c.errors
                .push(format!("{id}: port lumped kinetic energy {ek2} vs {want}"));
        }
        c.documented += 1;
        return Ok(c);
    }
    if has(f, "lumped/mass") {
        // documented difference (UD-005): upstream's lumped mass is
        // identically zero (node2 = conn(0)); the port's is the corrected
        // mass (values: structural_model_dynamics_checks.rs,
        // lumped_rotary_inertia.rs)
        let up = f64s(f, "lumped/mass");
        if !up.iter().all(|v| *v == 0.0) || f64s(f, "lumped/kinetic_energy") != [0.0] {
            c.errors
                .push(format!("{id}: upstream lumped records not zero (UD-005)"));
        }
        m.assemble_lumped_mass_matrix().map_err(|e| e.to_string())?;
        let lm = m.lumped_mass().unwrap();
        if lm.len() * lm.nb_component() != up.len() || !lm.as_slice().iter().all(|v| *v > 0.0) {
            c.errors
                .push(format!("{id}: port lumped mass not positive"));
        }
        let v = f64s(f, "lumped/velocity");
        m.velocity_mut().unwrap().as_mut_slice().copy_from_slice(&v);
        let ek = m.energy("kinetic").map_err(|e| e.to_string())?;
        let want = lumped_kinetic_energy(&m);
        if !(ek > 0.0 && (ek - want).abs() <= 1e-13 * want) {
            c.errors
                .push(format!("{id}: port lumped kinetic energy {ek} vs {want}"));
        }
        c.documented += up.len() + 1;
        return Ok(c);
    }
    let nb_nodes = m.displacement().unwrap().len();
    {
        let b = bools(f, "input/blocked_dofs");
        m.blocked_dofs_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&b);
        let u = f64s(f, "input/displacement");
        m.displacement_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&u);
    }
    if has(f, "loads/traction_global") {
        let ty = types_of(f)[0];
        let ndof = m.nb_degree_of_freedom();
        let g = Array::from_vec(f64s(f, "loads/traction_global"), ndof).unwrap();
        m.external_force_mut().unwrap().zero();
        m.compute_forces_by_global_traction_array(&g, ty)
            .map_err(|e| e.to_string())?;
        c.check(
            f,
            "loads/external_force_global",
            m.external_force().unwrap().as_slice(),
        );
        let l = Array::from_vec(f64s(f, "loads/traction_local"), ndof).unwrap();
        m.compute_forces_by_local_traction_array(&l, ty)
            .map_err(|e| e.to_string())?;
        if has(f, "loads/external_force_after_local") {
            // the probe adds nodal loads after the line loads: with equal
            // line loads the recorded total is used as is; otherwise the
            // port's line loads plus the recorded nodal increments
            let after = f64s(f, "loads/external_force_after_local");
            let got = m.external_force().unwrap().as_slice().to_vec();
            c.bits("loads/external_force_after_local", &got, &after);
            let total = f64s(f, "input/external_force");
            let same = got
                .iter()
                .zip(&after)
                .all(|(a, b)| a.to_bits() == b.to_bits());
            let fe: Vec<f64> = if same {
                total
            } else {
                got.iter()
                    .zip(total.iter().zip(&after))
                    .map(|(g, (t, a))| g + (t - a))
                    .collect()
            };
            m.external_force_mut()
                .unwrap()
                .as_mut_slice()
                .copy_from_slice(&fe);
        } else {
            c.check(
                f,
                "input/external_force",
                m.external_force().unwrap().as_slice(),
            );
        }
    } else {
        let fe = f64s(f, "input/external_force");
        m.external_force_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&fe);
    }
    let method = method_of(f);
    let dynamic = method != AnalysisMethod::Static;
    if id.ends_with("_dynamic") && !id.contains("/v76_") {
        // TestStructBernoulliDynamic::TestBeamOscilation solver settings
        let nls = m.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(100))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-8)).unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
    }
    if has(f, "input/time_step") {
        // the exact recorded value (the JSON parameter is informative only:
        // serde_json does not round-trip every double)
        m.set_time_step(f64s(f, "input/time_step")[0], None)
            .unwrap();
    }
    if has(f, "input/velocity") {
        let v = f64s(f, "input/velocity");
        m.velocity_mut().unwrap().as_mut_slice().copy_from_slice(&v);
    }
    let nb_steps = param(f, "nb_steps").as_u64().unwrap() as usize;
    let series_node = param(f, "series/node").as_u64().map(|v| v as usize);
    let series_dof = param(f, "series/dof").as_u64().unwrap_or(0) as usize;
    let ndof = m.nb_degree_of_freedom();
    let (mut series, mut ek, mut ep, mut iters) = (Vec::new(), Vec::new(), Vec::new(), Vec::new());
    for s in 1..=nb_steps {
        m.solve_step(None).map_err(|e| e.to_string())?;
        if method != AnalysisMethod::ExplicitLumpedMass {
            iters.push(m.non_linear_solver_mut(None).unwrap().nb_iterations() as i64);
        }
        if dynamic {
            if let Some(node) = series_node {
                series.push(m.displacement().unwrap().as_slice()[node * ndof + series_dof]);
            }
            ek.push(m.energy("kinetic").unwrap());
            ep.push(m.energy("potential").unwrap());
            let key = format!("series/displacement_step_{s}");
            if has(f, &key) {
                c.check(f, &key, m.displacement().unwrap().as_slice());
            }
        }
    }
    assert_eq!(m.displacement().unwrap().len(), nb_nodes);
    check_aij(&mut c, f, &m, "K", "K");
    check_aij(&mut c, f, &m, "M", "M");
    c.check(
        f,
        "final/displacement",
        m.displacement().unwrap().as_slice(),
    );
    c.check(
        f,
        "final/internal_force",
        m.internal_force().unwrap().as_slice(),
    );
    if dynamic {
        c.check(f, "final/velocity", m.velocity().unwrap().as_slice());
        c.check(
            f,
            "final/acceleration",
            m.acceleration().unwrap().as_slice(),
        );
    }
    for ty in types_of(f) {
        let key = format!("final/stress/{}", ty.name());
        if has(f, &key) {
            c.check(f, &key, m.stress(ty, G).unwrap().as_slice());
        }
    }
    if has(f, "series/nb_iterations") {
        let want = i64s(f, "series/nb_iterations");
        let n = iters.iter().zip(&want).filter(|(a, b)| a != b).count()
            + iters.len().abs_diff(want.len());
        let e = c
            .quantities
            .entry("series/nb_iterations".into())
            .or_default();
        e.0 += want.len();
        e.1 += n;
    }
    if dynamic {
        if series_node.is_some() {
            c.check(f, "series/node_displacement", &series);
        }
        c.check(f, "series/kinetic_energy", &ek);
        c.check(f, "series/potential_energy", &ep);
    } else {
        c.check(f, "energy/potential", &[m.energy("potential").unwrap()]);
        c.check(f, "energy/kinetic", &[m.energy("kinetic").unwrap()]);
    }
    Ok(c)
}

/// Committed cases: 12 + 8 (adversarial cases).
const EXPECTED_CASES: usize = 20;

/// The upstream-compatibility switches: name and "correct this one".
type Switch = (&'static str, fn(&mut StructuralModelUpstreamCompat));
const SWITCHES: [Switch; 5] = [
    ("UD-293", |c| c.fe.beam3_theta_y_legacy = false),
    ("UD-343", |c| c.fe.beam3_normal_unchecked = false),
    ("UD-007", |c| c.dkt_bending = DktBendingStiffness::Corrected),
    ("UD-507", |c| c.beam3_torsional_mass_rho_a = false),
    // D-059 (not a switch): the rotary-inertia option
    // `IsotropicMax` of the lumped mass, a documented UD-005 difference
    // (never compared bitwise): no compared group may change; the lumped
    // cases check the port's values against their definition (positivity,
    // the kinetic energy with the nodal blocks)
    ("IsotropicMax", |c| {
        c.lumped_rotary_inertia = LumpedRotaryInertia::IsotropicMax;
    }),
];

/// Declared effect of correcting `ud` alone on `case` (sorted groups;
/// empty = bitwise identical to the oracle).
fn expected(case: &str, ud: &str) -> &'static [&'static str] {
    // a static solve downstream of a changed load or stiffness
    const STATIC_STATE: [&str; 4] = [
        "energy/potential",
        "final/displacement",
        "final/internal_force",
        "final/stress",
    ];
    match (case, ud) {
        // UD-293: θy row of N·T (distributed θy moments of the line loads);
        // neither B, K nor the consistent mass involve the row
        ("bernoulli_beam_3_line_loads", "UD-293") => &[
            "energy/potential",
            "final/displacement",
            "final/internal_force",
            "final/stress",
            "input/external_force",
            "loads/external_force_global",
        ],
        ("v76_frame3d_general", "UD-293") => &[
            "energy/potential",
            "final/displacement",
            "final/internal_force",
            "final/stress",
            "loads/external_force_after_local",
            "loads/external_force_global",
        ],
        // UD-007: DKT bending block (t ≠ 1); every DOF imposed: the
        // displacement is an input
        ("discrete_kirchhoff_triangle_18_static_t05" | "v76_dkt_tilted_imposed", "UD-007") => &[
            "K",
            "energy/potential",
            "final/internal_force",
            "final/stress",
        ],
        ("mixed_beam_shell_static" | "v76_dkt_beam_tilted", "UD-007") => &[
            "K",
            STATIC_STATE[0],
            STATIC_STATE[1],
            STATIC_STATE[2],
            STATIC_STATE[3],
        ],
        // UD-507: torsion row of the 3-D beam consistent mass. The committed
        // dynamic case never excites torsion (zero torsional load and
        // velocity: the decoupled torsional block changes no other value)
        ("bernoulli_beam_3_dynamic", "UD-507") => &["M"],
        ("v76_beam3_dynamic_general", "UD-507") => &[
            "M",
            "final/acceleration",
            "final/displacement",
            "final/internal_force",
            "final/stress",
            "final/velocity",
            "series/displacement_step",
            "series/kinetic_energy",
            "series/node_displacement",
            "series/potential_energy",
        ],
        // explicit consistent mass: the Newton solve of the acceleration
        // meets its threshold after a different number of iterations
        ("v76_beam3_explicit_consistent_general", "UD-507") => &[
            "M",
            "final/acceleration",
            "final/displacement",
            "final/internal_force",
            "final/stress",
            "final/velocity",
            "series/displacement_step",
            "series/kinetic_energy",
            "series/nb_iterations",
            "series/node_displacement",
            "series/potential_energy",
        ],
        // the consistent-branch kinetic energy before the first step
        ("v76_beam3_lumped_pre_energy", "UD-507") => &["pre/M", "pre/kinetic_energy"],
        _ => &[],
    }
}

fn names_checked() -> Vec<String> {
    let names = case_names();
    assert_eq!(
        names.len(),
        EXPECTED_CASES,
        "fixtures/model/structural: {names:?} (oracle/probes/structural_model/generate.sh)"
    );
    names
}

#[test]
fn structural_model_matches_oracle() {
    let names = names_checked();
    let (mut total, mut coords, mut documented, mut failures) = (0, 0, 0, Vec::new());
    for name in &names {
        let f = case(name);
        match replay(&f, StructuralModelUpstreamCompat::UPSTREAM) {
            Ok(r) => {
                if !r.differing().is_empty() || !r.errors.is_empty() {
                    r.report(name, "upstream");
                    failures.push(format!(
                        "{name} [upstream]: differs in {:?}; errors {:?}",
                        r.differing(),
                        r.errors
                    ));
                }
                eprintln!(
                    "{name} [upstream]: {} values bitwise, {} COO coordinates, {} documented \
                     UD-005 values",
                    r.compared(),
                    r.coordinates,
                    r.documented
                );
                total += r.compared();
                coords += r.coordinates;
                documented += r.documented;
            }
            Err(e) => failures.push(format!("{name} [upstream]: refused: {e}")),
        }
        // corrected default: exactly the union of the per-switch effects
        let mut union: Vec<&str> = SWITCHES
            .iter()
            .flat_map(|(ud, _)| expected(name, ud).iter().copied())
            .collect();
        union.sort_unstable();
        union.dedup();
        match replay(&f, StructuralModelUpstreamCompat::default()) {
            Ok(r) => {
                r.report(name, "default");
                if r.differing() != union || !r.errors.is_empty() {
                    failures.push(format!(
                        "{name} [default]: expected {union:?}, got {:?}; errors {:?}",
                        r.differing(),
                        r.errors
                    ));
                }
            }
            Err(e) => failures.push(format!("{name} [default]: refused: {e}")),
        }
    }
    eprintln!(
        "structural model oracle replays: {} cases, {total} values bitwise (upstream mode), \
         {coords} COO coordinates equal, {documented} documented UD-005 values checked",
        names.len()
    );
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

/// One switch corrected alone: exactly the declared groups differ.
fn switch_alone(ud: &str) {
    let (_, correct) = SWITCHES.iter().find(|(n, _)| *n == ud).unwrap();
    let mut compat = StructuralModelUpstreamCompat::UPSTREAM;
    correct(&mut compat);
    let (mut exercised, mut failures) = (0, Vec::new());
    for name in &names_checked() {
        let f = case(name);
        let want = expected(name, ud);
        match replay(&f, compat) {
            Ok(r) => {
                r.report(name, &format!("{ud} corrected"));
                if r.differing() != want || !r.errors.is_empty() {
                    failures.push(format!(
                        "{name} [{ud} corrected]: expected {want:?}, got {:?}; errors {:?}",
                        r.differing(),
                        r.errors
                    ));
                }
            }
            Err(e) => failures.push(format!("{name} [{ud} corrected]: refused: {e}")),
        }
        if !want.is_empty() {
            exercised += 1;
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
    eprintln!("{ud} corrected alone: {exercised} case(s) differ exactly as declared");
    // UD-343 refuses inadmissible normals; every fixture normal is
    // admissible, so no case may change (model-level refusal:
    // review_v76_regressions.rs::st04_*; kernel-level oracle cases with
    // inadmissible normals: fixtures/fe/structural)
    // the rotary-inertia option changes only the lumped mass, a documented
    // UD-005 difference that no case compares bitwise
    // (lumped_rotary_blocks.rs checks it)
    if ud != "UD-343" && ud != "IsotropicMax" {
        assert!(exercised > 0, "{ud}: no case exercises the correction");
    }
}

#[test]
fn ud293_corrected_alone_changes_exactly_the_declared_quantities() {
    switch_alone("UD-293");
}

#[test]
fn ud343_corrected_alone_changes_nothing_on_admissible_normals() {
    switch_alone("UD-343");
}

#[test]
fn ud007_corrected_alone_changes_exactly_the_declared_quantities() {
    switch_alone("UD-007");
}

#[test]
fn ud507_corrected_alone_changes_exactly_the_declared_quantities() {
    switch_alone("UD-507");
}

#[test]
fn isotropic_max_option_changes_no_compared_quantity() {
    switch_alone("IsotropicMax");
}
