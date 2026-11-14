// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test support for the
// Differential tests against fixtures/model/diffusion/* (generated
// by oracle/probes/diffusion/diffusion_probe.cc) and the ported heat-transfer
// patch tests (test/test_model/patch_tests/patch_test_linear_heat_transfer_*).

#![allow(dead_code)] // shared by several test binaries, each uses a subset

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use akantu_core::enums::{AnalysisMethod, MeshIoType, NonLinearSolverType, SparseSolverType};
use akantu_core::{Array, ElementType, GhostType, Real};
use akantu_diffusion::HeatTransferModel;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;
use akantu_validation::{Fixture, fixtures_root};
use sha2::{Digest, Sha256};

/// Open `fixtures/model/diffusion/<name>` (golden, checksums verified).
pub fn case(name: &str) -> Fixture {
    let dir = fixtures_root().join("model/diffusion").join(name);
    let f = Fixture::open(&dir).unwrap_or_else(|e| panic!("{}: {e}", dir.display()));
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

/// Every case name below `fixtures/model/diffusion` starting with `prefix`.
pub fn cases(prefix: &str) -> Vec<String> {
    let dir = fixtures_root().join("model/diffusion");
    let mut out: Vec<String> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .filter(|e| e.path().join("manifest.json").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(prefix))
                .collect()
        })
        .unwrap_or_default();
    out.sort();
    out
}

pub fn f64s(f: &Fixture, name: &str) -> Vec<f64> {
    f.load_f64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

pub fn i64s(f: &Fixture, name: &str) -> Vec<i64> {
    f.load_i64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

pub fn has(f: &Fixture, name: &str) -> bool {
    f.quantity(name).is_ok()
}

pub fn bools(f: &Fixture, name: &str) -> Vec<bool> {
    let a = f.load(name).unwrap();
    let v = a
        .to_f64()
        .unwrap_or_else(|_| a.to_i64().unwrap().into_iter().map(|x| x as f64).collect());
    v.into_iter().map(|x| x != 0.0).collect()
}

pub fn param<'a>(f: &'a Fixture, key: &str) -> &'a serde_json::Value {
    &f.manifest().parameters[key]
}

pub fn pstr(f: &Fixture, key: &str) -> String {
    param(f, key)
        .as_str()
        .unwrap_or_else(|| panic!("{}: {key}", f.case_id()))
        .to_owned()
}

pub fn dim_of(f: &Fixture) -> usize {
    param(f, "spatial_dimension").as_u64().unwrap() as usize
}

/// Number of entries whose bits differ and the largest relative difference.
pub fn diff_stats(a: &[f64], b: &[f64]) -> (usize, f64) {
    let scale = b.iter().fold(0.0f64, |m, v| m.max(v.abs()));
    let mut n = a.len().abs_diff(b.len());
    let mut worst = 0.0f64;
    for (x, y) in a.iter().zip(b) {
        // D-046 exact rule: identical bits, or default NaNs of either sign
        if !akantu_validation::exact_eq(*y, *x) {
            n += 1;
            worst = worst.max((x - y).abs() / scale.max(f64::MIN_POSITIVE));
        }
    }
    (n, worst)
}

/// Bit identity of `got` with `want`.
pub fn assert_bits_vec(case: &str, what: &str, got: &[f64], want: &[f64]) -> usize {
    let (n, worst) = diff_stats(got, want);
    assert!(
        n == 0,
        "{case}: {what}: {n} of {} values differ (max rel {worst:e})",
        want.len()
    );
    want.len()
}

/// Bit identity of `got` with the recorded quantity `name`.
pub fn assert_bits(f: &Fixture, name: &str, got: &[f64]) -> usize {
    let want = f64s(f, name);
    assert_bits_vec(f.case_id(), name, got, &want)
}

pub fn f64_digest(v: &[Real]) -> String {
    let mut h = Sha256::new();
    for x in v {
        h.update(akantu_validation::canonical_le_bytes(*x));
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn digest_arrays(arrays: &[&[Real]]) -> String {
    let mut h = Sha256::new();
    for a in arrays {
        for x in *a {
            h.update(akantu_validation::canonical_le_bytes(*x));
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Read a Gmsh mesh.
pub fn read_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh).unwrap();
    mesh
}

/// Path of the fixture's mesh: stored with the fixture, or referenced
/// (`frozen:<path>` → `fixtures/meshes/gmsh-*/<path>`, checksum verified).
pub fn mesh_path(f: &Fixture) -> PathBuf {
    if let Ok(p) = f.input_path("mesh") {
        return p;
    }
    let input = f
        .manifest()
        .inputs
        .iter()
        .find(|i| i.role == "mesh")
        .expect("mesh input");
    let src = input.source.as_deref().expect("mesh source");
    let rel = src.strip_prefix("frozen:").expect("frozen mesh reference");
    let meshes = fixtures_root().join("meshes");
    let mut gmsh: Vec<PathBuf> = std::fs::read_dir(&meshes)
        .unwrap()
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name()
                .is_some_and(|n| n.to_string_lossy().starts_with("gmsh-"))
        })
        .collect();
    gmsh.sort();
    let p = gmsh.last().expect("frozen meshes").join(rel);
    let bytes = std::fs::read(&p).unwrap();
    let mut h = Sha256::new();
    h.update(&bytes);
    let got: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(got, input.sha256, "{}: frozen mesh checksum", f.case_id());
    p
}

/// The patch-test mesh with upstream's topology: `buildFacets` +
/// `createBoundaryGroupFromGeometry` by the port, the element groups'
/// node sets checked against the recorded ones.
pub fn patch_mesh(f: &Fixture) -> Mesh {
    let dim = dim_of(f);
    let mut mesh = read_mesh(&mesh_path(f), dim);
    // no model exists yet: nothing to dispatch the facet events to
    let _events = akantu_mesh::utils::build_facets(&mut mesh).unwrap();
    akantu_mesh::clusters::create_boundary_group_from_geometry(&mut mesh).unwrap();
    if let Some(groups) = param(f, "groups").as_array() {
        let want: BTreeSet<i64> = groups
            .iter()
            .flat_map(|g| i64s(f, &format!("group/{}/nodes", g["name"].as_str().unwrap())))
            .collect();
        let got: BTreeSet<i64> = mesh
            .groups()
            .element_groups()
            .flat_map(|g| g.nodes().iter().map(|&n| i64::from(n)).collect::<Vec<_>>())
            .collect();
        assert_eq!(got, want, "{}: boundary node set", f.case_id());
    }
    mesh
}

/// Nodes of every element group of the mesh (sorted, unique).
pub fn group_nodes(mesh: &Mesh) -> Vec<usize> {
    let s: BTreeSet<usize> = mesh
        .groups()
        .element_groups()
        .flat_map(|g| g.nodes().iter().map(|&n| n as usize).collect::<Vec<_>>())
        .collect();
    s.into_iter().collect()
}

pub fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

pub fn method_of(analysis: &str) -> AnalysisMethod {
    match analysis {
        "static" | "square_static" => AnalysisMethod::Static,
        "implicit" | "square_implicit" => AnalysisMethod::ImplicitDynamic,
        _ => AnalysisMethod::ExplicitLumpedMass,
    }
}

/// Heat-transfer model on `mesh` with the fixture's material file, every
/// upstream-defect switch in its default (corrected) mode.
pub fn model_on(f: &Fixture, mesh: Mesh, id: &str) -> HeatTransferModel {
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    HeatTransferModel::with_parser(SharedMesh::new(mesh), None, id, parser).unwrap()
}

/// [`model_on`] for a replay of the unpatched oracle: the solver switch of
/// UD-287 (the generalized-trapezoidal predictor keeps the rate, which the
/// backward-Euler implicit cases reproduce; no other solver or model
/// switch is needed by these cases).
pub fn oracle_model_on(f: &Fixture, mesh: Mesh, id: &str) -> HeatTransferModel {
    let mut m = model_on(f, mesh, id);
    m.dof_manager_mut()
        .set_upstream_compat(akantu_solver::UpstreamCompat {
            generalized_trapezoidal_keeps_rate_in_predictor: true,
            ..akantu_solver::UpstreamCompat::default()
        });
    m
}

/// Set the Newton–Raphson parameters of the default solver (`false` if the
/// solver is not a Newton–Raphson one).
pub fn set_newton(
    model: &mut HeatTransferModel,
    max: i64,
    threshold: Real,
    criterion: &str,
) -> bool {
    let nls = model.solver.non_linear_solver_mut(None).unwrap();
    if nls.as_newton_raphson_mut().is_none() {
        return false;
    }
    nls.set_param("max_iterations", ParamValue::Int(max))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(threshold))
        .unwrap();
    nls.set_param("convergence_type", ParamValue::Enum(criterion.into()))
        .unwrap();
    true
}

pub fn newton_history(model: &HeatTransferModel, its: &mut Vec<i64>, errs: &mut Vec<Real>) {
    let nls = model.solver.non_linear_solver(None).unwrap();
    its.push(nls.nb_iterations() as i64);
    errs.push(nls.error());
}

pub fn element_type(name: &str) -> ElementType {
    name.parse()
        .unwrap_or_else(|_| panic!("element type {name}"))
}

pub fn tname(ty: ElementType) -> String {
    ty.to_string()
}

/// Element types of dimension `dim` (not ghost, regular kind).
pub fn types_of(mesh: &Mesh, dim: usize) -> Vec<ElementType> {
    mesh.element_types(akantu_core::TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: GhostType::NotGhost,
        kind: Some(akantu_core::ElementKind::Regular),
    })
    .collect()
}

/// Matrix `id`: digests always, full COO arrays when recorded.
pub fn check_matrix(f: &Fixture, model: &HeatTransferModel, id: &str) -> usize {
    let exists = param(f, &format!("{id}/exists")).as_bool().unwrap_or(false);
    if !exists {
        assert!(
            !model.dof_manager().has_matrix(id)
                || model.dof_manager().matrix(id).map(|m| m.nnz()).unwrap_or(0) == 0,
            "{}: matrix {id} exists in the port only",
            f.case_id()
        );
        return 0;
    }
    let m = model.dof_manager().matrix(id).unwrap();
    let (r, c, v) = m.coo();
    let rows: Vec<i64> = r.to_vec();
    let cols: Vec<i64> = c.to_vec();
    let mut h = Sha256::new();
    for x in rows.iter().chain(&cols) {
        h.update(x.to_le_bytes());
    }
    let coords: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        param(f, &format!("{id}/nnz")).as_u64().unwrap() as usize,
        v.len(),
        "{}: {id} nnz",
        f.case_id()
    );
    assert_eq!(
        coords,
        pstr(f, &format!("{id}/coords_sha256")),
        "{}: {id} coords",
        f.case_id()
    );
    if has(f, &format!("{id}/values")) {
        assert_eq!(
            rows,
            i64s(f, &format!("{id}/rows")),
            "{}: {id} rows",
            f.case_id()
        );
        assert_eq!(
            cols,
            i64s(f, &format!("{id}/cols")),
            "{}: {id} cols",
            f.case_id()
        );
        assert_bits(f, &format!("{id}/values"), v);
    }
    assert_eq!(
        f64_digest(v),
        pstr(f, &format!("{id}/values_sha256")),
        "{}: {id} values",
        f.case_id()
    );
    assert_eq!(
        param(f, &format!("{id}/symmetric")).as_bool().unwrap(),
        m.matrix_type() == akantu_core::enums::MatrixType::Symmetric,
        "{}: {id} symmetry",
        f.case_id()
    );
    3 * v.len()
}

/// Lumped capacity per DOF.
pub fn check_lumped(f: &Fixture, model: &HeatTransferModel) -> usize {
    let exists = param(f, "lumped_M/exists").as_bool().unwrap_or(false);
    if !exists {
        assert!(
            !model.dof_manager().has_lumped_matrix("M"),
            "{}: lumped M exists in the port only",
            f.case_id()
        );
        return 0;
    }
    let m = model.lumped_capacity().unwrap();
    assert_eq!(
        f64_digest(m.as_slice()),
        pstr(f, "lumped_M/sha256"),
        "{}: lumped M digest",
        f.case_id()
    );
    if has(f, "lumped_M") {
        assert_bits(f, "lumped_M", m.as_slice());
    }
    m.len()
}

/// Law `law` internals (raw buffers) and filters, full arrays or digests.
pub fn check_internals(f: &Fixture, model: &HeatTransferModel, law: usize, prefix: &str) -> usize {
    let l = model.law(law).unwrap();
    let mut n = 0;
    for ty in l.base().filter_types(GhostType::NotGhost) {
        let t = tname(ty);
        for internal in ["∇u", "D∇u", "diffusivity"] {
            let a_g = l
                .base()
                .internal(internal)
                .unwrap()
                .get(ty, GhostType::NotGhost)
                .unwrap();
            let a = a_g.as_slice();
            let key = format!("{prefix}{internal}/{t}");
            if param(f, &format!("{key}/sha256")).is_string() {
                assert_eq!(
                    f64_digest(a),
                    pstr(f, &format!("{key}/sha256")),
                    "{}: {key}",
                    f.case_id()
                );
                n += a.len();
            } else {
                n += assert_bits(f, &key, a);
            }
        }
        let filter: Vec<i64> = l
            .base()
            .filter(ty, GhostType::NotGhost)
            .iter()
            .map(|&x| i64::from(x))
            .collect();
        let key = format!("{prefix}filter/{t}");
        if has(f, &key) {
            assert_eq!(filter, i64s(f, &key), "{}: {key}", f.case_id());
        } else {
            let mut h = Sha256::new();
            for x in &filter {
                h.update(x.to_le_bytes());
            }
            let d: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
            assert_eq!(
                d,
                pstr(f, &format!("{key}/sha256")),
                "{}: {key}",
                f.case_id()
            );
        }
    }
    n
}

/// Copy of an array's values.
pub fn values(a: &Array<Real>) -> Vec<Real> {
    a.as_slice().to_vec()
}
