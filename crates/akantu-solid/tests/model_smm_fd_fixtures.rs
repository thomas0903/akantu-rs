// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). SMM-023
// differential tests against fixtures/model/smm_fd/*
// (oracle/probes/smm/smm_fd_probe.cc, strict reference build, Eigen sparse
// solver): total-Lagrangian finite-deformation assembly
// (src/model/solid_mechanics/material.cc
// assembleInternalForcesFiniteDeformation / assembleStiffnessMatrixNL / L2)
// through the model — static patch tests (plane strain, plane stress,
// 1-D, 3-D), explicit / implicit patch runs (ExplicitFiniteDeformation /
// ImplicitFiniteDeformation of patch_test_linear_elastic_*.cc with a
// genuinely finite-deformation material, UD-210), large-strain dynamics,
// large-load statics, the eigenstrain stiffness rule (UD-208, compat mode),
// and the upstream tests test_solid_mechanics_model_material_large_rotation
// (UD-078) and test_finite_deformation (rotation objectivity). Every
// recorded value bitwise (D-008; fixtures/model/smm_fd/tolerances.json).

mod model_common;

use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromHigherDim, FromSameDim};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_solid::SolidMechanicsModel;
use akantu_solver::ModelSolverOptions;
use akantu_validation::reduce::NanMinMax;
use akantu_validation::{Fixture, canonical_le_bytes};
use model_common::*;
use sha2::{Digest, Sha256};

const NG: GhostType = GhostType::NotGhost;

const ALPHA: [[Real; 4]; 3] = [
    [0.01, 0.02, 0.03, 0.04],
    [0.05, 0.06, 0.07, 0.08],
    [0.09, 0.10, 0.11, 0.12],
];

fn fd(name: &str) -> Fixture {
    case(&format!("smm_fd/{name}"))
}

fn fd_cases(prefix: &str) -> Vec<String> {
    cases_of("smm_fd")
        .into_iter()
        .map(|c| c.trim_start_matches("smm_fd/").to_owned())
        .filter(|c| c.starts_with(prefix))
        .collect()
}

fn pstr(f: &Fixture, key: &str) -> String {
    param(f, key)
        .as_str()
        .unwrap_or_else(|| panic!("{}: {key}", f.case_id()))
        .to_owned()
}

fn dim_of(f: &Fixture) -> usize {
    param(f, "spatial_dimension").as_u64().unwrap() as usize
}

fn element_type(name: &str) -> ElementType {
    name.parse()
        .unwrap_or_else(|_| panic!("element type {name}"))
}

fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// Mesh of the fixture; with recorded `mesh_types`/`groups` (patch cases),
/// upstream's topology after `buildFacets` + `createBoundaryGroupFromGeometry`.
fn mesh_of(f: &Fixture) -> Mesh {
    let dim = dim_of(f);
    let mut mesh = read_mesh(&f.input_path("mesh").unwrap(), dim);
    if let Some(types) = param(f, "mesh_types").as_array() {
        let mut acc = MeshAccessor::new(&mut mesh);
        for t in types {
            let name = t.as_str().unwrap();
            let ty = element_type(name);
            let want = i64s(f, &format!("mesh/connectivity/{name}"));
            let mut c = acc.connectivity_mut(ty, NG);
            if ty.spatial_dimension() == dim {
                let have: Vec<i64> = c.as_slice().iter().map(|&x| i64::from(x)).collect();
                assert_eq!(have, want, "{}: elements of {name}", f.case_id());
                continue;
            }
            c.replace(
                Array::from_vec(
                    want.iter().map(|&x| x as Idx).collect(),
                    ty.nb_nodes_per_element(),
                )
                .unwrap(),
            );
        }
    }
    if let Some(groups) = param(f, "groups").as_array() {
        let existing: Vec<String> = mesh
            .groups()
            .element_groups()
            .map(|g| g.name().to_string())
            .collect();
        for g in groups {
            let name = g["name"].as_str().unwrap();
            if existing.iter().any(|e| e == name) {
                continue;
            }
            let gdim = g["dimension"].as_u64().unwrap() as usize;
            let mut group = mesh.groups_mut().create_element_group(name, gdim).unwrap();
            for t in g["types"].as_array().unwrap() {
                let tn = t.as_str().unwrap();
                let ty = element_type(tn);
                for e in i64s(f, &format!("group/{name}/elements/{tn}")) {
                    group
                        .add(Element::new(ty, e as Idx, NG), None, false)
                        .unwrap();
                }
            }
            for n in i64s(f, &format!("group/{name}/nodes")) {
                group.add_node(n as Idx, false);
            }
        }
    }
    mesh
}

/// Model on the fixture's mesh and material file (default mode of every
/// upstream-defect switch).
fn model_on(f: &Fixture, mesh: Mesh, id: &str) -> SolidMechanicsModel {
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    SolidMechanicsModel::with_parser(akantu_mesh::SharedMesh::new(mesh), None, id, parser).unwrap()
}

/// SHA-256 of the values' little-endian bytes, a default NaN of either sign
/// hashed as `0x7ff8000000000000` (decision D-046; every digest recorded on
/// aarch64 is unchanged).
fn f64_digest(v: &[Real]) -> String {
    let mut h = Sha256::new();
    for x in v {
        h.update(canonical_le_bytes(*x));
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Digest of u, v, a as the probe's `digest` (raw bytes in that order),
/// default NaNs canonicalised (D-046).
fn state_digest(model: &SolidMechanicsModel) -> String {
    state_digest_with(model, canonical_le_bytes)
}

fn state_digest_with(model: &SolidMechanicsModel, bytes: fn(Real) -> [u8; 8]) -> String {
    let mut h = Sha256::new();
    for a in [
        model.displacement().unwrap(),
        model.velocity().unwrap(),
        model.acceleration().unwrap(),
    ] {
        for x in a.as_slice() {
            h.update(bytes(*x));
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Matrix `id` against the record under `prefix`: full COO when recorded,
/// else the digests.
fn check_matrix(f: &Fixture, model: &SolidMechanicsModel, id: &str, prefix: &str) -> usize {
    if param(f, &format!("{prefix}/nnz")).is_null() {
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
    let mut h = Sha256::new();
    for x in r.iter().chain(c) {
        h.update(x.to_le_bytes());
    }
    let coords: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        coords,
        pstr(f, &format!("{prefix}/coords_sha256")),
        "{}: {prefix} coords",
        f.case_id()
    );
    if has(f, &format!("{prefix}/values")) {
        assert_eq!(r.to_vec(), i64s(f, &format!("{prefix}/rows")));
        assert_eq!(c.to_vec(), i64s(f, &format!("{prefix}/cols")));
        assert_bits(f, &format!("{prefix}/values"), v);
    }
    assert_eq!(
        f64_digest(v),
        pstr(f, &format!("{prefix}/values_sha256")),
        "{}: {prefix} values",
        f.case_id()
    );
    assert_eq!(
        param(f, &format!("{prefix}/symmetric")).as_bool().unwrap(),
        m.matrix_type() == akantu_core::enums::MatrixType::Symmetric,
        "{}: {prefix} symmetry",
        f.case_id()
    );
    3 * v.len()
}

/// Material 0 internals: stress, grad_u, piola_kirchhoff_2, filter.
fn check_internals(f: &Fixture, model: &SolidMechanicsModel, prefix: &str) -> usize {
    let law = model.material(0).unwrap();
    let mut n = 0;
    for ty in law.base().filter_types(NG) {
        if ty.spatial_dimension() != model.spatial_dimension() {
            continue;
        }
        let t = tname(ty);
        let b = law.base();
        n += assert_bits(
            f,
            &format!("{prefix}stress/{t}"),
            b.stress.get(ty, NG).unwrap().as_slice(),
        );
        n += assert_bits(
            f,
            &format!("{prefix}grad_u/{t}"),
            b.grad_u.get(ty, NG).unwrap().as_slice(),
        );
        let pk2 = b.piola_kirchhoff_2.as_ref().expect("finite deformation");
        n += assert_bits(
            f,
            &format!("{prefix}piola_kirchhoff_2/{t}"),
            pk2.get(ty, NG).unwrap().as_slice(),
        );
        let filter: Vec<i64> = b
            .element_filter
            .get(ty, NG)
            .unwrap()
            .as_slice()
            .iter()
            .map(|&x| i64::from(x))
            .collect();
        assert_eq!(filter, i64s(f, &format!("{prefix}filter/{t}")));
    }
    n
}

/// `Material::onDump` (Cauchy stresses), then the stress internals.
fn check_cauchy(f: &Fixture, model: &mut SolidMechanicsModel) -> usize {
    model.material_mut(0).unwrap().on_dump().unwrap();
    let law = model.material(0).unwrap();
    let mut n = 0;
    for ty in law.base().filter_types(NG) {
        if ty.spatial_dimension() != model.spatial_dimension() {
            continue;
        }
        n += assert_bits(
            f,
            &format!("cauchy/stress/{}", tname(ty)),
            law.base().stress.get(ty, NG).unwrap().as_slice(),
        );
    }
    n
}

/// Recorded Newton parameters, applied when the solver is Newton–Raphson;
/// returns whether it is.
fn apply_newton(f: &Fixture, model: &mut SolidMechanicsModel) -> bool {
    let newton = model
        .solver
        .non_linear_solver_mut(None)
        .unwrap()
        .as_newton_raphson_mut()
        .is_some();
    if newton && !param(f, "newton_max_iterations").is_null() {
        set_newton(
            model,
            param(f, "newton_max_iterations").as_i64().unwrap(),
            param(f, "newton_threshold").as_f64().unwrap(),
            &pstr(f, "newton_convergence_type"),
        );
    }
    newton
}

fn newton_history(model: &SolidMechanicsModel, its: &mut Vec<i64>, errs: &mut Vec<Real>) {
    let nls = model.solver.non_linear_solver(None).unwrap();
    its.push(nls.nb_iterations() as i64);
    errs.push(nls.error());
}

fn nodes_of(model: &SolidMechanicsModel) -> Vec<Real> {
    model
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

/// `setLinearDOF` of the upstream patch-test fixture (sequential sums).
fn set_linear_dof(dof: &mut [Real], coord: &[Real]) {
    for (i, d) in dof.iter_mut().enumerate() {
        *d = ALPHA[i][0];
        for (j, x) in coord.iter().enumerate() {
            *d += ALPHA[i][j + 1] * x;
        }
    }
}

/* ---------------------------------------------------------------------- */
/* patch tests                                                             */
/* ---------------------------------------------------------------------- */

fn patch_method(f: &Fixture) -> AnalysisMethod {
    match pstr(f, "analysis").as_str() {
        "static" => AnalysisMethod::Static,
        "explicit" => AnalysisMethod::ExplicitLumpedMass,
        _ => AnalysisMethod::ImplicitDynamic,
    }
}

/// Model of a patch case initialised with `method`, the boundary-group
/// nodes blocked at the affine field (upstream `applyBC`).
fn patch_model(f: &Fixture, method: AnalysisMethod) -> SolidMechanicsModel {
    let dim = dim_of(f);
    let mut model = model_on(f, mesh_of(f), &pstr(f, "model_id"));
    model.init_full(&eigen_options(method)).unwrap();
    {
        let x = nodes_of(&model);
        let nodes: Vec<usize> = {
            let mesh = model.physics.base.mesh.read().unwrap();
            mesh.groups()
                .element_groups()
                .flat_map(|g| g.nodes().iter().map(|&n| n as usize).collect::<Vec<_>>())
                .collect()
        };
        let mut b_g = model.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        for &nd in &nodes {
            b[nd * dim..(nd + 1) * dim].fill(true);
        }
        drop(b_g);
        let mut u = model.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        for &nd in &nodes {
            set_linear_dof(
                &mut u[nd * dim..(nd + 1) * dim],
                &x[nd * dim..(nd + 1) * dim],
            );
        }
    }
    model
}

/// Every node at the affine field (the patch tests' initial state).
fn set_affine_field(model: &mut SolidMechanicsModel, dim: usize) {
    let x = nodes_of(model);
    for (u, xi) in model
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .chunks_exact_mut(dim)
        .zip(x.chunks_exact(dim))
    {
        set_linear_dof(u, xi);
    }
}

/// Dynamic patch start (probe `patch_case`): `dt = 0.8 getStableTimeStep()`,
/// then every node at the affine field; returns `[stable, dt]`.
fn start_dynamic_patch(model: &mut SolidMechanicsModel, dim: usize) -> [Real; 2] {
    let stable = model.stable_time_step().unwrap();
    let dt = 0.8 * stable;
    model.set_time_step(dt, None).unwrap();
    set_affine_field(model, dim);
    [stable, dt]
}

/// Oracle patch cases documented as diverging (D-046 (4)): the
/// case and the first step whose state (u, v, a) holds NaN. The probe's
/// patch-test time step is beyond the stability limit of the hex20
/// finite-deformation tangent
/// (`fd_explicit_patch_time_step_against_the_stability_limit`); the case
/// manifest's `notes` and NUMERICAL_VALIDATION "Default-NaN convention
/// (D-046)" document it. Such a case carries no patch-test (at rest)
/// evidence.
const DIVERGED_PATCH_CASES: [(&str, usize); 1] = [("explicit_patch_hexahedron_20", 49)];

/// How a patch replay ends.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PatchEnd {
    /// The final displacement is the affine field (the patch-test intent,
    /// checked NaN-propagating).
    AtRest,
    /// The documented divergence of [`DIVERGED_PATCH_CASES`]: the state
    /// holds NaN from `first_nan_step` on (on the free DOFs, default NaNs).
    Diverged { first_nan_step: usize },
}

/// Result of [`replay_patch`].
#[derive(Clone, Copy, Debug)]
struct PatchReplay {
    /// Non-NaN values compared bit for bit (per-step state digests and
    /// recorded quantities); default NaNs matched under D-046 are not
    /// counted.
    bitwise: usize,
    /// Steps whose state holds NaN (diverged cases only; their digests
    /// compare the non-NaN entries bitwise and the NaNs under D-046).
    nan_steps: usize,
    end: PatchEnd,
}

/// Largest `|u − affine|` over every DOF of `u` (the patch-test field of
/// the node coordinates `x`), **NaN-propagating**: a NaN anywhere gives
/// NaN, which fails every `< tol` check. `f64::max` would skip it
/// (the diverged hex20 case gave 0 with 21 NaN entries).
fn affine_deviation(u: &[Real], x: &[Real], dim: usize) -> Real {
    let mut worst: Real = 0.0;
    let mut want = vec![0.0; dim];
    for (ui, xi) in u.chunks_exact(dim).zip(x.chunks_exact(dim)) {
        set_linear_dof(&mut want, xi);
        for (a, b) in ui.iter().zip(&want) {
            worst = worst.nan_max((a - b).abs());
        }
    }
    worst
}

/// The patch-test criterion of a case at rest: the final displacement is
/// finite and equal to the affine field within `1e-10`.
fn at_rest(u: &[Real], x: &[Real], dim: usize) -> Result<(), String> {
    if u.len() != x.len() {
        return Err(format!("displacement length {} ≠ {}", u.len(), x.len()));
    }
    if let Some(k) = u.iter().position(|v| !v.is_finite()) {
        return Err(format!("non-finite displacement u[{k}] = {}", u[k]));
    }
    let worst = affine_deviation(u, x, dim);
    if worst < 1e-10 {
        Ok(())
    } else {
        Err(format!("|u − affine| = {worst:e}"))
    }
}

fn assert_at_rest(name: &str, u: &[Real], x: &[Real], dim: usize) {
    if let Err(e) = at_rest(u, x, dim) {
        panic!("{name}: {e}");
    }
}

fn replay_patch(name: &str) -> PatchReplay {
    let f = fd(name);
    let dim = dim_of(&f);
    let method = patch_method(&f);
    let mut model = patch_model(&f, method);
    let diverges_at = DIVERGED_PATCH_CASES
        .iter()
        .find(|(c, _)| *c == name)
        .map(|&(_, s)| s);
    let mut n = 0;
    let mut first_nan = None;
    let mut nan_steps = 0;
    assert_eq!(
        model.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs")[..]
    );
    n += assert_bits(
        &f,
        "initial_displacement",
        model.displacement().unwrap().as_slice(),
    );
    let newton = apply_newton(&f, &mut model);
    assert_eq!(newton, param(&f, "newton").as_bool().unwrap());
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    if method == AnalysisMethod::Static {
        model.solve_step(None).unwrap();
        newton_history(&model, &mut its, &mut errs);
    } else {
        n += assert_bits(&f, "time_step", &start_dynamic_patch(&mut model, dim));
        let digests = param(&f, "step_digests").as_array().unwrap().clone();
        for (s, d) in digests.iter().enumerate() {
            let step = s + 1;
            model.solve_step(None).unwrap();
            assert_eq!(
                state_digest(&model),
                d.as_str().unwrap(),
                "{name}: state after step {step}"
            );
            if newton {
                newton_history(&model, &mut its, &mut errs);
            }
            let guards = [
                model.displacement().unwrap(),
                model.velocity().unwrap(),
                model.acceleration().unwrap(),
            ];
            let state = [
                guards[0].as_slice(),
                guards[1].as_slice(),
                guards[2].as_slice(),
            ];
            let nans: usize = state
                .iter()
                .map(|a| a.iter().filter(|v| v.is_nan()).count())
                .sum();
            let entries: usize = state.iter().map(|a| a.len()).sum();
            if diverges_at.is_none() {
                assert!(
                    state.iter().all(|a| a.iter().all(|v| v.is_finite())),
                    "{name}: non-finite state after step {step} (no divergence documented)"
                );
            }
            if nans > 0 {
                first_nan.get_or_insert(step);
                nan_steps += 1;
            } else {
                assert!(
                    first_nan.is_none(),
                    "{name}: NaN disappeared at step {step}"
                );
            }
            // the digest compared every entry: the non-NaN ones bit for
            // bit, the NaNs under D-046
            n += entries - nans;
        }
        let ek = model.energy("kinetic").unwrap();
        n += assert_bits(&f, "energy/kinetic_final", &[ek]);
        n += assert_bits(&f, "velocity", model.velocity().unwrap().as_slice());
        n += assert_bits(&f, "acceleration", model.acceleration().unwrap().as_slice());
    }
    if newton {
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
    n += assert_bits(
        &f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    let ep = model.energy("potential").unwrap();
    n += assert_bits(&f, "energy/potential", &[ep]);
    n += check_internals(&f, &model, "");
    n += check_matrix(&f, &model, "K", "K");
    if has(&f, "mass") {
        n += assert_bits(&f, "mass", model.mass().unwrap().as_slice());
    }
    n += check_cauchy(&f, &mut model);
    // the patch-test intent: the affine field is the finite-deformation
    // equilibrium too (static: exact up to the solver; dynamics: at rest)
    let x = nodes_of(&model);
    let u_g = model.displacement().unwrap();
    let u = u_g.as_slice();
    let end = match diverges_at {
        None => {
            assert_at_rest(name, u, &x, dim);
            PatchEnd::AtRest
        }
        Some(documented) => {
            // Asserted as diverged, never as at rest
            assert_eq!(
                first_nan,
                Some(documented),
                "{name}: first step with NaN in (u, v, a)"
            );
            assert!(
                affine_deviation(u, &x, dim).is_nan(),
                "{name}: the at-rest measure must see the NaNs"
            );
            let blocked_g = model.blocked_dofs().unwrap();
            let blocked = blocked_g.as_slice();
            for (q, a) in [
                ("displacement", u),
                ("velocity", model.velocity().unwrap().as_slice()),
                ("acceleration", model.acceleration().unwrap().as_slice()),
            ] {
                for (k, (v, b)) in a.iter().zip(blocked).enumerate() {
                    assert_eq!(
                        v.is_nan(),
                        !b,
                        "{name}: {q}[{k}]: NaN exactly on the free DOFs"
                    );
                    assert!(
                        !v.is_nan() || akantu_validation::exact::is_default_nan(*v),
                        "{name}: {q}[{k}] = {:#018x} is not a default NaN",
                        v.to_bits()
                    );
                }
            }
            // the blocked DOFs keep the affine field exactly
            let mut want = vec![0.0; dim];
            for (node, (ui, xi)) in u.chunks_exact(dim).zip(x.chunks_exact(dim)).enumerate() {
                set_linear_dof(&mut want, xi);
                for (j, (a, w)) in ui.iter().zip(&want).enumerate() {
                    if blocked[node * dim + j] {
                        assert_eq!(a.to_bits(), w.to_bits(), "{name}: blocked u[{node}][{j}]");
                    }
                }
            }
            PatchEnd::Diverged {
                first_nan_step: documented,
            }
        }
    };
    PatchReplay {
        bitwise: n,
        nan_steps,
        end,
    }
}

/// [`replay_patch`] of a case that must end at rest (every static and
/// implicit patch case); returns its bitwise value count.
fn replay_patch_at_rest(name: &str) -> usize {
    let r = replay_patch(name);
    assert_eq!(r.end, PatchEnd::AtRest, "{name}");
    assert_eq!(r.nan_steps, 0, "{name}");
    r.bitwise
}

/// Replays every case of the family; a failing case does not stop the
/// others (an early stop hid the later cases): all failures are
/// reported together at the end. Returns the cases at rest, the diverged
/// cases (name, first NaN step) and the bitwise value count.
fn replay_family(prefix: &str) -> (Vec<String>, Vec<(String, usize)>, usize) {
    let cases = fd_cases(prefix);
    assert!(!cases.is_empty(), "no {prefix}* cases");
    let mut n = 0;
    let (mut at_rest, mut diverged) = (Vec::new(), Vec::new());
    let mut failures = Vec::new();
    for c in &cases {
        match std::panic::catch_unwind(|| replay_patch(c)) {
            Ok(r) => {
                n += r.bitwise;
                match r.end {
                    PatchEnd::AtRest => {
                        eprintln!("{c}: at rest; {} values bitwise", r.bitwise);
                        at_rest.push(c.clone());
                    }
                    PatchEnd::Diverged { first_nan_step } => {
                        eprintln!(
                            "{c}: diverged (documented) from step {first_nan_step}: {} non-NaN \
                             values bitwise, {} NaN steps compared under D-046; not at rest",
                            r.bitwise, r.nan_steps
                        );
                        diverged.push((c.clone(), first_nan_step));
                    }
                }
            }
            Err(e) => {
                let msg = e
                    .downcast_ref::<String>()
                    .cloned()
                    .or_else(|| e.downcast_ref::<&str>().map(|m| (*m).to_owned()))
                    .unwrap_or_else(|| "non-string panic".to_owned());
                failures.push(format!("{c}: {msg}"));
            }
        }
    }
    assert!(
        failures.is_empty(),
        "{} of {} {prefix}* cases failed:\n{}",
        failures.len(),
        cases.len(),
        failures.join("\n")
    );
    (at_rest, diverged, n)
}

#[test]
fn fd_static_patch_bitwise() {
    let (at_rest, diverged, n) = replay_family("static_patch_");
    assert_eq!(at_rest.len(), 16, "12 types + 4 plane-stress twins");
    assert!(diverged.is_empty());
    eprintln!("{} cases at rest, {n} values bitwise", at_rest.len());
}

#[test]
fn fd_explicit_patch_bitwise() {
    let (at_rest, diverged, n) = replay_family("explicit_patch_");
    // 12 cases: 11 at rest, the documented diverged one is not counted
    // among them
    assert_eq!(at_rest.len(), 11);
    assert!(!at_rest.iter().any(|c| c == "explicit_patch_hexahedron_20"));
    assert_eq!(diverged, [("explicit_patch_hexahedron_20".to_owned(), 49)]);
    eprintln!(
        "{} cases at rest, {} diverged (documented), {n} values bitwise",
        at_rest.len(),
        diverged.len()
    );
}

/// The at-rest measure propagates NaN. A finite patch
/// state at rest passes; the same state with one NaN entry (or an
/// infinite one) fails, wherever the entry sits. With `f64::max` in
/// [`affine_deviation`] the NaN cases would pass.
#[test]
fn fd_patch_at_rest_check_is_not_satisfied_by_nan() {
    let f = fd("explicit_patch_hexahedron_8");
    let dim = dim_of(&f);
    let mut model = patch_model(&f, AnalysisMethod::ExplicitLumpedMass);
    set_affine_field(&mut model, dim);
    let x = nodes_of(&model);
    let u = model.displacement().unwrap().as_slice().to_vec();
    assert_eq!(affine_deviation(&u, &x, dim), 0.0);
    assert_at_rest("affine", &u, &x, dim);
    for k in [0, u.len() / 2, u.len() - 1] {
        for bad in [Real::NAN, -Real::NAN, Real::INFINITY] {
            let mut v = u.clone();
            v[k] = bad;
            let dev = affine_deviation(&v, &x, dim);
            let passes = dev < 1e-10;
            assert!(!passes, "entry {k} = {bad}: deviation {dev}");
            assert!(
                at_rest(&v, &x, dim).is_err(),
                "entry {k} = {bad}: accepted as at rest"
            );
        }
    }
}

/// Quantities of `explicit_patch_hexahedron_20` holding NaN in the oracle
/// record (aarch64: all `0x7ff8000000000000`), with their NaN counts
/// (documented in the case manifest's `notes` and
/// NUMERICAL_VALIDATION).
const HEX20_EXPLICIT_NAN_QUANTITIES: [(&str, usize); 9] = [
    ("displacement", 21),
    ("velocity", 21),
    ("acceleration", 21),
    ("internal_force", 243),
    ("energy/kinetic_final", 1),
    ("energy/potential", 1),
    ("grad_u/_hexahedron_20", 576),
    ("piola_kirchhoff_2/_hexahedron_20", 576),
    ("cauchy/stress/_hexahedron_20", 576),
];

/// D-046. The oracle run of `explicit_patch_hexahedron_20`
/// diverges: the state (u, v, a) is finite up to step 48 and holds NaN
/// from step 49 on (every free DOF); the port reproduces each step's
/// digest. On x86_64 the NaNs carry the sign bit (SSE's default NaN): the
/// test sets every NaN of the final state to each platform's default NaN
/// and checks that the D-046 digest and exact comparison accept both while
/// a raw-bit digest (the comparison before D-046) rejects the x86_64 one.
#[test]
fn fd_explicit_patch_hexahedron_20_diverges_from_step_49() {
    let f = fd("explicit_patch_hexahedron_20");
    let dim = dim_of(&f);
    assert_eq!(patch_method(&f), AnalysisMethod::ExplicitLumpedMass);
    let mut model = patch_model(&f, AnalysisMethod::ExplicitLumpedMass);
    let ts = start_dynamic_patch(&mut model, dim);
    assert_bits(&f, "time_step", &ts);
    let x = nodes_of(&model);
    let digests = param(&f, "step_digests").as_array().unwrap().clone();
    assert_eq!(digests.len(), 100);
    let mut first_nan = None;
    let mut deviation = Vec::new();
    for (s, d) in digests.iter().enumerate() {
        model.solve_step(None).unwrap();
        assert_eq!(state_digest(&model), d.as_str().unwrap(), "step {}", s + 1);
        let guards = [
            model.displacement().unwrap(),
            model.velocity().unwrap(),
            model.acceleration().unwrap(),
        ];
        let state = [
            guards[0].as_slice(),
            guards[1].as_slice(),
            guards[2].as_slice(),
        ];
        let nan = state.iter().any(|a| a.iter().any(|v| v.is_nan()));
        // growth of the deviation from the affine equilibrium (NaN from
        // the first NaN step on: the measure propagates NaN)
        let dev = affine_deviation(state[0], &x, dim);
        assert_eq!(dev.is_nan(), state[0].iter().any(|v| v.is_nan()));
        if (s + 1) % 4 == 0 || (44..=50).contains(&(s + 1)) {
            if nan {
                eprintln!("step {:3}: NaN", s + 1);
            } else {
                eprintln!("step {:3}: max |u - affine| = {dev:e}", s + 1);
            }
        }
        deviation.push(dev);
        if nan && first_nan.is_none() {
            first_nan = Some(s + 1);
        }
        assert_eq!(
            nan,
            first_nan.is_some(),
            "NaN disappeared at step {}",
            s + 1
        );
    }
    assert_eq!(first_nan, Some(49), "first step with NaN in (u, v, a)");
    assert_eq!(
        DIVERGED_PATCH_CASES,
        [("explicit_patch_hexahedron_20", 49)],
        "documented divergence"
    );
    // The final state is not accepted as at rest
    let u_final = model.displacement().unwrap().as_slice().to_vec();
    assert!(affine_deviation(&u_final, &x, dim).is_nan());
    assert!(
        at_rest(&u_final, &x, dim).is_err(),
        "the diverged state passed the at-rest check"
    );
    // The deviation grows by the linearised amplification factor of the
    // central-difference scheme, |g| = r + sqrt(r² − 1), r = (ω dt)²/2 − 1,
    // for the tangent's ω_max (fd_explicit_patch_time_step_against_the_
    // stability_limit). Measured over steps 12..32, in the linear regime
    // (deviation 1e-13…1e-5); measured agreement 4e-7 relative (aarch64),
    // 1e-3 leaves room for the decaying subdominant modes.
    let text = std::fs::read_to_string(f.input_path("material").unwrap()).unwrap();
    let omega_dt = omega_dt_at_affine_state(&f, &text);
    let r = omega_dt * omega_dt / 2.0 - 1.0;
    let g = r + (r * r - 1.0).sqrt();
    let measured = (deviation[31] / deviation[11]).powf(1.0 / 20.0);
    eprintln!("omega_max dt = {omega_dt:.6}: |g| = {g:.6}, measured growth {measured:.6} per step");
    assert!((measured / g - 1.0).abs() < 1e-3, "{measured} vs {g}");
    // every NaN of the final state is the platform's default NaN, on the
    // free DOFs only (the boundary nodes stay blocked at the affine field)
    let blocked = model.blocked_dofs().unwrap().as_slice().to_vec();
    for a in [
        model.displacement().unwrap().as_slice(),
        model.velocity().unwrap().as_slice(),
        model.acceleration().unwrap().as_slice(),
    ] {
        for (v, b) in a.iter().zip(&blocked) {
            assert_eq!(v.is_nan(), !b);
            assert!(!v.is_nan() || akantu_validation::exact::is_default_nan(*v));
        }
    }
    // the oracle record: NaN exactly in the documented quantities
    for q in f.manifest().quantities.iter().filter(|q| q.dtype == "f64") {
        let nans = f64s(&f, &q.name).iter().filter(|v| v.is_nan()).count();
        let want = HEX20_EXPLICIT_NAN_QUANTITIES
            .iter()
            .find(|(n, _)| *n == q.name)
            .map_or(0, |(_, c)| *c);
        assert_eq!(nans, want, "NaN count of {}", q.name);
    }
    let notes = f.manifest().notes.as_deref().unwrap_or("");
    for (q, _) in HEX20_EXPLICIT_NAN_QUANTITIES {
        assert!(notes.contains(q), "manifest notes must name {q}");
    }
    // Both platforms' patterns, whichever platform runs the test: every NaN
    // of the state set to x86_64's negative default NaN, then to aarch64's
    // positive one. The D-046 digest and exact comparison accept both; a
    // raw-bit digest (the comparison before D-046) accepts only the
    // aarch64 pattern of the golden record.
    let last = digests.last().unwrap().as_str().unwrap();
    for (pattern, bits) in [
        (
            "x86_64",
            akantu_validation::exact::NEGATIVE_DEFAULT_NAN_BITS,
        ),
        ("aarch64", akantu_validation::exact::DEFAULT_NAN_BITS),
    ] {
        let set = |a: &mut [Real]| {
            for v in a.iter_mut().filter(|v| v.is_nan()) {
                *v = Real::from_bits(bits);
            }
        };
        set(model.displacement_mut().unwrap().as_mut_slice());
        set(model.velocity_mut().unwrap().as_mut_slice());
        set(model.acceleration_mut().unwrap().as_mut_slice());
        assert_eq!(state_digest(&model), last, "D-046 digest, {pattern} NaNs");
        assert_eq!(
            state_digest_with(&model, Real::to_le_bytes) == last,
            pattern == "aarch64",
            "raw-bit digest, {pattern} NaNs"
        );
        for (q, got) in [
            ("displacement", model.displacement().unwrap().as_slice()),
            ("velocity", model.velocity().unwrap().as_slice()),
            ("acceleration", model.acceleration().unwrap().as_slice()),
        ] {
            assert!(got.iter().any(|v| v.to_bits() == bits), "{q}");
            assert_bits(&f, q, got);
        }
    }
}

/// Dense block of the assembled matrix `id` on the free DOFs (`free`:
/// local DOF indices), from its COO in global equation numbers (one rank);
/// a symmetric matrix stores one triangle.
fn free_block(model: &SolidMechanicsModel, id: &str, free: &[usize]) -> Vec<Vec<Real>> {
    let dm = model.dof_manager();
    let eq = dm.global_equation_numbers("displacement").unwrap();
    let mut pos = std::collections::HashMap::new();
    for (k, &l) in free.iter().enumerate() {
        pos.insert(eq[l].unwrap(), k);
    }
    let m = dm.matrix(id).unwrap();
    let sym = m.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
    let (r, c, v) = m.coo();
    let mut a = vec![vec![0.0; free.len()]; free.len()];
    for ((ri, ci), x) in r.iter().zip(c).zip(v) {
        if let (Some(&i), Some(&j)) = (pos.get(ri), pos.get(ci)) {
            a[i][j] += x;
            if sym && i != j {
                a[j][i] += x;
            }
        }
    }
    a
}

/// Largest eigenvalue of `M⁻¹ K` (lumped, positive `M`): the largest
/// eigenvalue of the symmetric `M^{-1/2} K M^{-1/2}`.
fn lambda_max_generalized(k: &[Vec<Real>], m: &[Real]) -> Real {
    let n = m.len();
    let mut a = akantu_core::tensor::MatrixX::zeros(n, n);
    for i in 0..n {
        for j in 0..n {
            *a.at_mut(i, j) = k[i][j] / (m[i].sqrt() * m[j].sqrt());
        }
    }
    let ev = akantu_linalg::dense::eigh(&a, akantu_linalg::dense::Storage::Dynamic).unwrap();
    ev.into_iter().fold(Real::MIN, NanMinMax::nan_max)
}

/// `ω_max · dt` of the linearised central-difference scheme about the
/// affine state of a patch case (stable iff `≤ 2`): tangent stiffness
/// assembled by the port at every node on the affine field (`material`:
/// the case's material text, possibly edited), lumped mass and `dt` from
/// the oracle record.
fn omega_dt_at_affine_state(f: &Fixture, material: &str) -> Real {
    let dim = dim_of(f);
    let parser = akantu_parser::Parser::parse_str(material, "material.dat").unwrap();
    let mut model = SolidMechanicsModel::with_parser(
        akantu_mesh::SharedMesh::new(mesh_of(f)),
        None,
        &pstr(f, "model_id"),
        parser,
    )
    .unwrap();
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    set_affine_field(&mut model, dim);
    model.assemble_internal_forces().unwrap();
    model.assemble_stiffness_matrix(true).unwrap();
    let blocked = bools(f, "blocked_dofs");
    let free: Vec<usize> = (0..blocked.len()).filter(|&i| !blocked[i]).collect();
    let k = free_block(&model, "K", &free);
    let mass = f64s(f, "mass");
    let m: Vec<Real> = free.iter().map(|&i| mass[i]).collect();
    let dt = f64s(f, "time_step")[1];
    lambda_max_generalized(&k, &m).sqrt() * dt
}

/// Why the oracle's `explicit_patch_hexahedron_20` diverges. The
/// explicit patch cases use upstream's patch-test time step
/// `dt = 0.8 getStableTimeStep()`; `getStableTimeStep` is the small-strain
/// wave-speed estimate. The central-difference scheme linearised about the
/// affine equilibrium is stable iff `ω_max dt ≤ 2`, `ω_max²` the largest
/// eigenvalue of `M⁻¹ K_t` on the free DOFs. For every type the
/// small-strain stiffness is within the limit; with the genuinely
/// finite-deformation material the tangent at the pre-strained affine
/// state (|∇u| up to 0.12) is stiffer, and only `_hexahedron_20` crosses
/// the limit: rounding noise grows geometrically until the state overflows
/// to NaN (step 49, `fd_explicit_patch_hexahedron_20_diverges_from_step_49`).
#[test]
fn fd_explicit_patch_time_step_against_the_stability_limit() {
    let mut beyond = Vec::new();
    for name in fd_cases("explicit_patch_") {
        let f = fd(&name);
        let text = std::fs::read_to_string(f.input_path("material").unwrap()).unwrap();
        assert!(text.contains("finite_deformation = true"), "{name}");
        let small = text.replace("finite_deformation = true", "finite_deformation = false");
        let fd_dt = omega_dt_at_affine_state(&f, &text);
        let ss_dt = omega_dt_at_affine_state(&f, &small);
        eprintln!(
            "{name}: omega_max dt = {fd_dt:.6} (finite deformation), {ss_dt:.6} (small strain)"
        );
        assert!(ss_dt <= 2.0, "{name}: small strain beyond the limit");
        if fd_dt > 2.0 {
            beyond.push(name);
        }
    }
    assert_eq!(beyond, ["explicit_patch_hexahedron_20"]);
}

/// Element families (one test each, so the 100-step implicit and the
/// dynamic replays run in parallel in the fast tier).
const FAMILIES: [&str; 6] = [
    "segment",
    "triangle",
    "quadrangle",
    "tetrahedron",
    "hexahedron",
    "pentahedron",
];

fn replay_family_of(prefix: &str, family: &str, replay: fn(&str) -> usize) -> (usize, usize) {
    let cases: Vec<String> = fd_cases(prefix)
        .into_iter()
        .filter(|c| c[prefix.len()..].starts_with(family))
        .collect();
    assert_eq!(
        cases.len(),
        2,
        "{prefix}{family}_*: 2 cases (orders 1 and 2)"
    );
    let n = cases.iter().map(|c| replay(c)).sum();
    (cases.len(), n)
}

macro_rules! per_family {
    ($($test:ident: $prefix:literal, $family:literal, $replay:path;)*) => {$(
        #[test]
        fn $test() {
            assert!(FAMILIES.contains(&$family));
            let (c, n) = replay_family_of($prefix, $family, $replay);
            eprintln!("{}{}: {c} cases, {n} values bitwise", $prefix, $family);
        }
    )*};
}

per_family! {
    fd_implicit_patch_segment_bitwise: "implicit_patch_", "segment", replay_patch_at_rest;
    fd_implicit_patch_triangle_bitwise: "implicit_patch_", "triangle", replay_patch_at_rest;
    fd_implicit_patch_quadrangle_bitwise: "implicit_patch_", "quadrangle", replay_patch_at_rest;
    fd_implicit_patch_tetrahedron_bitwise: "implicit_patch_", "tetrahedron", replay_patch_at_rest;
    fd_implicit_patch_hexahedron_bitwise: "implicit_patch_", "hexahedron", replay_patch_at_rest;
    fd_implicit_patch_pentahedron_bitwise: "implicit_patch_", "pentahedron", replay_patch_at_rest;
    fd_dynamic_implicit_segment_bitwise: "dynamic_implicit_", "segment", replay_dynamic;
    fd_dynamic_implicit_triangle_bitwise: "dynamic_implicit_", "triangle", replay_dynamic;
    fd_dynamic_implicit_quadrangle_bitwise: "dynamic_implicit_", "quadrangle", replay_dynamic;
    fd_dynamic_implicit_tetrahedron_bitwise: "dynamic_implicit_", "tetrahedron", replay_dynamic;
    fd_dynamic_implicit_hexahedron_bitwise: "dynamic_implicit_", "hexahedron", replay_dynamic;
    fd_dynamic_implicit_pentahedron_bitwise: "dynamic_implicit_", "pentahedron", replay_dynamic;
}

#[test]
fn fd_case_inventory() {
    let all = cases_of("smm_fd");
    assert_eq!(all.len(), 82, "{all:?}");
    assert_eq!(fd_cases("mixed_quad8_tri6_").len(), 3);
    for prefix in ["implicit_patch_", "dynamic_implicit_", "dynamic_explicit_"] {
        assert_eq!(fd_cases(prefix).len(), 12, "{prefix}");
    }
}

/* ---------------------------------------------------------------------- */
/* x = xmin blocked: large-strain dynamics and large-load statics          */
/* ---------------------------------------------------------------------- */

fn xmin_xmax(x: &[Real], dim: usize) -> (Real, Real) {
    let mut lo = x[0];
    let mut hi = x[0];
    for p in x.chunks_exact(dim) {
        lo = lo.min(p[0]);
        hi = hi.max(p[0]);
    }
    (lo, hi)
}

fn block_xmin(model: &mut SolidMechanicsModel) {
    let dim = model.spatial_dimension();
    let x = nodes_of(model);
    let (xmin, _) = xmin_xmax(&x, dim);
    let mut b = model.blocked_dofs_mut().unwrap();
    let b = b.as_mut_slice();
    for (n, p) in x.chunks_exact(dim).enumerate() {
        if (p[0] - xmin).abs() < 1e-10 {
            b[n * dim..(n + 1) * dim].fill(true);
        }
    }
}

/// The probe's non-affine initial displacement (sequential sums).
fn initial_field(model: &mut SolidMechanicsModel, amplitude: Real) {
    const C: [[Real; 3]; 3] = [[0.5, 0.3, -0.2], [-0.4, 0.6, 0.1], [0.2, -0.1, 0.4]];
    let dim = model.spatial_dimension();
    let x = nodes_of(model);
    let (xmin, _) = xmin_xmax(&x, dim);
    let mut u = model.displacement_mut().unwrap();
    let u = u.as_mut_slice();
    for (ui, p) in u.chunks_exact_mut(dim).zip(x.chunks_exact(dim)) {
        for i in 0..dim {
            let mut s = C[i][0];
            for j in 1..dim {
                s += C[i][j] * p[j];
            }
            ui[i] = amplitude * (p[0] - xmin) * s;
        }
    }
}

fn replay_dynamic(name: &str) -> usize {
    let f = fd(name);
    let implicit = pstr(&f, "analysis") == "implicit";
    let method = if implicit {
        AnalysisMethod::ImplicitDynamic
    } else {
        AnalysisMethod::ExplicitLumpedMass
    };
    let mut model = model_on(&f, mesh_of(&f), "solid_mechanics_model");
    model.init_full(&eigen_options(method)).unwrap();
    block_xmin(&mut model);
    initial_field(&mut model, param(&f, "amplitude").as_f64().unwrap());
    let newton = apply_newton(&f, &mut model);
    assert_eq!(newton, param(&f, "newton").as_bool().unwrap());
    let stable = model.stable_time_step().unwrap();
    let dt = if implicit { 1e-4 } else { 0.5 * stable };
    model.set_time_step(dt, None).unwrap();
    let mut n = assert_bits(&f, "time_step", &[stable, dt]);
    assert_eq!(
        model.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs")[..]
    );
    n += assert_bits(
        &f,
        "initial_displacement",
        model.displacement().unwrap().as_slice(),
    );
    let (mut ek, mut ep, mut ew) = (Vec::new(), Vec::new(), Vec::new());
    let mut snap = |m: &mut SolidMechanicsModel| {
        ek.push(m.energy("kinetic").unwrap());
        ep.push(m.energy("potential").unwrap());
        ew.push(m.energy("external work").unwrap());
    };
    snap(&mut model);
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    let digests = param(&f, "step_digests").as_array().unwrap().clone();
    for (s, d) in digests.iter().enumerate() {
        model.solve_step(None).unwrap();
        if newton {
            newton_history(&model, &mut its, &mut errs);
        }
        assert_eq!(
            state_digest(&model),
            d.as_str().unwrap(),
            "{name}: state after step {}",
            s + 1
        );
        snap(&mut model);
    }
    let dim = model.spatial_dimension();
    n += digests.len() * 3 * model.displacement().unwrap().len() * dim;
    n += assert_bits(&f, "energy/kinetic", &ek);
    n += assert_bits(&f, "energy/potential", &ep);
    n += assert_bits(&f, "energy/external_work", &ew);
    if newton {
        assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
    n += assert_bits(&f, "velocity", model.velocity().unwrap().as_slice());
    n += assert_bits(&f, "acceleration", model.acceleration().unwrap().as_slice());
    n += assert_bits(
        &f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    n += check_internals(&f, &model, "");
    n += check_matrix(&f, &model, "K", "K");
    n += check_matrix(&f, &model, "M", "M");
    if has(&f, "mass") {
        n += assert_bits(&f, "mass", model.mass().unwrap().as_slice());
    }
    n += check_cauchy(&f, &mut model);
    // genuinely finite deformation: the quadratic part ½ ∇uᵀ∇u of the Green
    // strain is not negligible against the linear part at the final state
    let law = model.material(0).unwrap();
    let (mut quad, mut lin): (Real, Real) = (0.0, 0.0);
    for ty in law.base().filter_types(NG) {
        for g in law
            .base()
            .grad_u
            .get(ty, NG)
            .unwrap()
            .as_slice()
            .chunks_exact(dim * dim)
        {
            for i in 0..dim {
                for j in 0..dim {
                    let q: Real = (0..dim).map(|k| g[k + i * dim] * g[k + j * dim]).sum();
                    quad = quad.nan_max(0.5 * q.abs());
                    lin = lin.nan_max((0.5 * (g[i + j * dim] + g[j + i * dim])).abs());
                }
            }
        }
    }
    assert!(
        quad > 1e-3 * lin,
        "{name}: max |½ ∇uᵀ∇u| = {quad:e} vs max |ε| = {lin:e}"
    );
    n
}

#[test]
fn fd_dynamic_explicit_bitwise() {
    let cases = fd_cases("dynamic_explicit_");
    assert_eq!(cases.len(), 12);
    let n: usize = cases.iter().map(|c| replay_dynamic(c)).sum();
    eprintln!("{} cases, {n} values bitwise", cases.len());
}

/// Static solve with the probe's load; `compat` sets the UD-208 switch.
fn loaded_model(f: &Fixture, compat: bool) -> SolidMechanicsModel {
    let dim = dim_of(f);
    let mut model = model_on(f, mesh_of(f), "solid_mechanics_model");
    model
        .physics
        .upstream_compat
        .stiffness_grad_u_ignores_eigen_grad_u = compat;
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    block_xmin(&mut model);
    let x = nodes_of(&model);
    let (_, xmax) = xmin_xmax(&x, dim);
    let load = f64s(f, "nodal_load");
    let mut fe_g = model.external_force_mut().unwrap();
    let fe = fe_g.as_mut_slice();
    for (n, p) in x.chunks_exact(dim).enumerate() {
        if (p[0] - xmax).abs() < 1e-10 {
            fe[n * dim..(n + 1) * dim].copy_from_slice(&load);
        }
    }
    drop(fe_g);
    apply_newton(f, &mut model);
    model.solve_step(None).unwrap();
    model
}

fn replay_loaded(name: &str, compat: bool) -> usize {
    let f = fd(name);
    let mut model = loaded_model(&f, compat);
    let mut n = 0;
    assert_eq!(
        model.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs")[..]
    );
    n += assert_bits(
        &f,
        "external_force",
        model.external_force().unwrap().as_slice(),
    );
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    newton_history(&model, &mut its, &mut errs);
    assert_eq!(its, i64s(&f, "newton/nb_iterations"), "{name}: iterations");
    n += assert_bits(&f, "newton/error", &errs);
    n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
    n += assert_bits(
        &f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    let ep = model.energy("potential").unwrap();
    n += assert_bits(&f, "energy/potential", &[ep]);
    n += check_internals(&f, &model, "");
    n += check_matrix(&f, &model, "K", "K");
    if has(&f, "after_stiffness/K/nnz") || !param(&f, "after_stiffness/K/nnz").is_null() {
        model.assemble_stiffness_matrix(true).unwrap();
        n += check_internals(&f, &model, "after_stiffness/");
        n += check_matrix(&f, &model, "K", "after_stiffness/K");
    }
    n += check_cauchy(&f, &mut model);
    // the solution balances the load: |f_ext + f_int| on free DOFs ≪ |f_ext|
    let b = model.blocked_dofs().unwrap().as_slice().to_vec();
    let fe = model.external_force().unwrap().as_slice().to_vec();
    let fi = model.internal_force().unwrap().as_slice().to_vec();
    let res = fe
        .iter()
        .zip(&fi)
        .zip(&b)
        .filter(|(_, b)| !**b)
        .fold(0.0_f64, |a, ((e, i), _)| a.nan_max((e + i).abs()));
    let scale = fe.iter().fold(0.0_f64, |a, x| a.nan_max(x.abs()));
    assert!(
        res < 1e-6 * scale,
        "{name}: residual {res:e} vs load {scale:e}"
    );
    n
}

#[test]
fn fd_loaded_static_bitwise() {
    let cases = fd_cases("loaded_static_");
    assert_eq!(cases.len(), 12);
    let n: usize = cases.iter().map(|c| replay_loaded(c, false)).sum();
    eprintln!("{} cases, {n} values bitwise", cases.len());
}

/// UD-208 extended to the finite-deformation `L2` part: upstream evaluates
/// `B_L1` (and stores `grad_u`) at `∇u`, not `∇u − ε⁰`. Compat mode replays
/// the oracle bitwise (Newton history, solution, `K` and `grad_u` after the
/// standalone stiffness assembly). Default mode stores `∇u − ε⁰` (bitwise
/// the value of the last stress computation) and converges to the same
/// equilibrium within the solver tolerance.
#[test]
fn fd_eigenstrain_stiffness_upstream_compat_and_default() {
    let name = "eigenstrain_stiffness_quadrangle_4";
    let n = replay_loaded(name, true);
    assert!(n > 0);
    let f = fd(name);
    let mut model = loaded_model(&f, false);
    let u_oracle = f64s(&f, "displacement");
    let u = model.displacement().unwrap().as_slice().to_vec();
    let scale = u_oracle.iter().fold(0.0_f64, |a, x| a.nan_max(x.abs()));
    let diff = u
        .iter()
        .zip(&u_oracle)
        .fold(0.0_f64, |a, (x, y)| a.nan_max((x - y).abs()));
    // both runs converge to |r| < 1e-9 |r0|: same equilibrium
    assert!(
        diff < 1e-8 * scale,
        "default vs oracle displacement {diff:e}"
    );
    let before = model
        .material(0)
        .unwrap()
        .base()
        .grad_u
        .get(ElementType::Quadrangle4, NG)
        .unwrap()
        .clone();
    model.assemble_stiffness_matrix(true).unwrap();
    let law = model.material(0).unwrap();
    let after = law.base().grad_u.get(ElementType::Quadrangle4, NG).unwrap();
    for (a, b) in after.as_slice().iter().zip(before.as_slice()) {
        assert_eq!(a.to_bits(), b.to_bits(), "default: grad_u = ∇u − ε⁰");
    }
    // and the oracle's stored ∇u differs from it by ε⁰
    let oracle_after = f64s(&f, "after_stiffness/grad_u/_quadrangle_4");
    let eigen = law
        .base()
        .eigen_grad_u
        .get(ElementType::Quadrangle4, NG)
        .unwrap();
    for ((o, a), e) in oracle_after
        .iter()
        .zip(after.as_slice())
        .zip(eigen.as_slice())
    {
        assert!(
            (o - (a + e)).abs() < 1e-8,
            "oracle ∇u vs default ∇u − ε⁰ + ε⁰"
        );
    }
}

/* ---------------------------------------------------------------------- */
/* upstream tests                                                          */
/* ---------------------------------------------------------------------- */

/// test_solid_mechanics_model_material_large_rotation.cc (UNSTABLE
/// upstream; UD-078: the test's non-const `M(i, j)` aborts outside the
/// profile and its mass-centre loop reads past the end — the probe uses
/// the const accessor and the in-range loop).
#[test]
fn test_solid_mechanics_model_material_large_rotation() {
    let f = fd("large_rotation_tetrahedron_4");
    let mut model = model_on(&f, mesh_of(&f), "solid_mechanics_model");
    model
        .init_full(&eigen_options(AnalysisMethod::ImplicitDynamic))
        .unwrap();
    set_newton(&mut model, 100, 1e-4, "residual");
    model.assemble_mass().unwrap();
    // sum_j M(i, j) in equation numbering, upstream loop order
    let m = model.dof_manager().matrix("M").unwrap();
    let size = model.dof_manager().local_system_size();
    let mut dense = vec![0.0; size * size];
    let sym = m.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
    let (r, c, v) = m.coo();
    for ((&i, &j), &x) in r.iter().zip(c).zip(v) {
        let (i, j) = (i as usize, j as usize);
        dense[i * size + j] = x;
        if sym {
            dense[j * size + i] = x;
        }
    }
    let mut mass = vec![0.0; size];
    for i in 0..size {
        for j in 0..size {
            mass[i] += dense[i * size + j];
        }
    }
    let mut center = [0.0; 3];
    let mut total = 0.0;
    for i in 0..size / 3 {
        for j in 0..3 {
            center[j] += mass[i * 3 + j];
            total += mass[i * 3 + j];
        }
    }
    for c in &mut center {
        *c /= total / 3.;
    }
    let dt = model.time_step(None).unwrap();
    model.solve_step(None).unwrap();
    let mut n = assert_bits(&f, "mass_row_sums", &mass);
    n += assert_bits(&f, "mass_center", &center);
    n += assert_bits(&f, "total_mass", &[total]);
    n += assert_bits(&f, "time_step", &[dt]);
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    newton_history(&model, &mut its, &mut errs);
    assert_eq!(its, i64s(&f, "newton/nb_iterations"));
    n += assert_bits(&f, "newton/error", &errs);
    n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
    n += assert_bits(
        &f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    n += check_matrix(&f, &model, "M", "M");
    n += check_matrix(&f, &model, "K", "K");
    n += check_internals(&f, &model, "");
    // the test's intent: the total mass is ρ·V (unit cube, ρ = 7800) per
    // direction
    assert!(
        (total / 3.0 - 7800.0).abs() < 1e-9 * 7800.0,
        "total {total}"
    );
    eprintln!("large rotation: {n} values bitwise");
}

/// test_finite_deformation.cc (TestFiniteDeformation.NotUnit): the internal
/// force of a rigidly rotated state is the rotated internal force.
#[test]
fn test_finite_deformation_not_unit() {
    let f = fd("rotation_tetrahedron_4");
    let mut model = model_on(&f, mesh_of(&f), "solid_mechanics_model");
    model
        .init_full(&eigen_options(AnalysisMethod::Static))
        .unwrap();
    let alpha: [[Real; 4]; 3] = [
        [0.00, 0.02, 0.03, 0.04],
        [0.00, 0.06, 0.07, 0.08],
        [0.00, 0.10, 0.11, 0.12],
    ];
    let x = nodes_of(&model);
    let impose = |m: &mut SolidMechanicsModel| {
        m.blocked_dofs_mut().unwrap().as_mut_slice().fill(true);
        let mut u = m.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        u.fill(0.0);
        for (ui, p) in u
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(x.as_chunks::<3>().0)
        {
            for i in 0..3 {
                ui[i] += alpha[i][0];
            }
            for (k, &pk) in p.iter().enumerate() {
                for i in 0..3 {
                    ui[i] += alpha[i][1 + k] * pk;
                }
            }
        }
    };
    impose(&mut model);
    model.solve_step(None).unwrap();
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    newton_history(&model, &mut its, &mut errs);
    assert_eq!(its, i64s(&f, "first/newton/nb_iterations"));
    let mut n = assert_bits(&f, "first/newton/error", &errs);
    let u0 = model.displacement().unwrap().as_slice().to_vec();
    let f0 = model.internal_force().unwrap().as_slice().to_vec();
    n += assert_bits(&f, "first/displacement", &u0);
    n += assert_bits(&f, "first/internal_force", &f0);
    // u = R (X + u) − X with the recorded R (host cos/sin of the oracle)
    let r = f64s(&f, "rotation");
    impose(&mut model);
    {
        let mut u = model.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        for (ui, p) in u
            .as_chunks_mut::<3>()
            .0
            .iter_mut()
            .zip(x.as_chunks::<3>().0)
        {
            let xp: Vec<Real> = (0..3).map(|i| p[i] + ui[i]).collect();
            let mut rx = [0.0; 3];
            akantu_fe::eigen::product_slices(
                akantu_fe::eigen::Dims::dynamic(3, 3),
                akantu_fe::eigen::Dims::new(3, 1, false, true),
                &r,
                &xp,
                &mut rx,
            )
            .unwrap();
            for i in 0..3 {
                ui[i] = rx[i] - p[i];
            }
        }
    }
    n += assert_bits(
        &f,
        "rotated_displacement",
        model.displacement().unwrap().as_slice(),
    );
    model.solve_step(None).unwrap();
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    newton_history(&model, &mut its, &mut errs);
    assert_eq!(its, i64s(&f, "newton/nb_iterations"));
    n += assert_bits(&f, "newton/error", &errs);
    let u = model.displacement().unwrap().as_slice().to_vec();
    let fi = model.internal_force().unwrap().as_slice().to_vec();
    n += assert_bits(&f, "displacement", &u);
    n += assert_bits(&f, "internal_force", &fi);
    n += check_internals(&f, &model, "");
    n += check_matrix(&f, &model, "K", "K");
    n += check_cauchy(&f, &mut model);
    // the upstream assertions (same tolerances)
    for (node, p) in x.as_chunks::<3>().0.iter().enumerate() {
        let d0 = &u0[node * 3..node * 3 + 3];
        let d = &u[node * 3..node * 3 + 3];
        let refdis: Vec<Real> = (0..3)
            .map(|i| alpha[i][0] + (0..3).map(|k| alpha[i][1 + k] * p[k]).sum::<Real>())
            .collect();
        let dist = |a: &[Real], b: &[Real]| {
            a.iter()
                .zip(b)
                .map(|(x, y)| (x - y) * (x - y))
                .sum::<Real>()
                .sqrt()
        };
        assert!(dist(&refdis, d0) < 1e-14);
        let rot = |v: &[Real]| -> Vec<Real> {
            (0..3)
                .map(|i| (0..3).map(|k| r[i + 3 * k] * v[k]).sum())
                .collect()
        };
        let xp: Vec<Real> = (0..3).map(|i| p[i] + d0[i]).collect();
        let want: Vec<Real> = rot(&xp).iter().zip(p).map(|(a, b)| a - b).collect();
        assert!(dist(d, &want) < 1e-14);
        let rf0 = rot(&f0[node * 3..node * 3 + 3]);
        assert!(dist(&fi[node * 3..node * 3 + 3], &rf0) < 1e-5);
    }
    eprintln!("rotation: {n} values bitwise");
}

/* ---------------------------------------------------------------------- */
/* two materials on a mixed quad8 / tri6 mesh                              */
/* ---------------------------------------------------------------------- */

/// Every material's internals per type of its filter (empty types
/// included, as recorded): stress, grad_u, piola_kirchhoff_2 (finite
/// deformation), filter.
fn check_internals_all(f: &Fixture, model: &SolidMechanicsModel, prefix: &str) -> usize {
    let mut n = 0;
    for m in 0..model.nb_materials() {
        let law = model.material(m).unwrap();
        let b = law.base();
        let mp = format!("{prefix}m{m}/");
        let types: Vec<String> = b.filter_types(NG).into_iter().map(tname).collect();
        let want: Vec<String> = param(f, &format!("{mp}types"))
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t.as_str().unwrap().to_owned())
            .collect();
        assert_eq!(types, want, "{}: {mp} filter types", f.case_id());
        let fd = param(f, &format!("{mp}finite_deformation"))
            .as_bool()
            .unwrap();
        assert_eq!(b.finite_deformation, fd, "{}: {mp}", f.case_id());
        for ty in b.filter_types(NG) {
            let t = tname(ty);
            n += assert_bits(
                f,
                &format!("{mp}stress/{t}"),
                b.stress.get(ty, NG).unwrap().as_slice(),
            );
            n += assert_bits(
                f,
                &format!("{mp}grad_u/{t}"),
                b.grad_u.get(ty, NG).unwrap().as_slice(),
            );
            if fd {
                let pk2 = b.piola_kirchhoff_2.as_ref().expect("finite deformation");
                n += assert_bits(
                    f,
                    &format!("{mp}piola_kirchhoff_2/{t}"),
                    pk2.get(ty, NG).unwrap().as_slice(),
                );
            }
            let filter: Vec<i64> = b
                .element_filter
                .get(ty, NG)
                .unwrap()
                .as_slice()
                .iter()
                .map(|&x| i64::from(x))
                .collect();
            assert_eq!(filter, i64s(f, &format!("{mp}filter/{t}")), "{mp}{t}");
        }
    }
    n
}

fn replay_mixed(name: &str) -> usize {
    let f = fd(name);
    let linear = pstr(&f, "non_linear_solver") == "linear";
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let mut model = model_on(&f, mesh, "solid_mechanics_model");
    model.set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    let options =
        ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(if linear {
                NonLinearSolverType::Linear
            } else {
                NonLinearSolverType::Auto
            }),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        });
    model.init_full(&options).unwrap();
    // upstream's `_linear` is the Newton–Raphson class without a
    // convergence loop (`non_linear_solver_newton_raphson.cc`); Newton
    // parameters are recorded (and applied) for the Newton cases only
    assert!(apply_newton(&f, &mut model), "{name}: Newton-Raphson class");
    assert_eq!(
        param(&f, "newton_max_iterations").is_null(),
        linear,
        "{name}: solver"
    );
    for (group, axis) in [
        ("left", SpatialDirection::X),
        ("left", SpatialDirection::Y),
        ("bottom", SpatialDirection::Y),
    ] {
        model
            .apply_dirichlet_bc(&FixedValue { value: 0.0, axis }, group)
            .unwrap();
    }
    assert_eq!(
        model.blocked_dofs().unwrap().as_slice(),
        &bools(&f, "blocked_dofs")[..],
        "{name}: blocked"
    );
    let lambdas = f64s(&f, "load_factors");
    assert_eq!(lambdas, [0.5, 1.0]);
    let mut n = 0;
    for (k, &l) in lambdas.iter().enumerate() {
        model.external_force_mut().unwrap().as_mut_slice().fill(0.0);
        model
            .apply_neumann_bc(&FromSameDim::new(vec![l * 1e8, l * 2e8]), "right")
            .unwrap();
        // upstream Matrix<Real>{{0, l·3e7}, {l·3e7, l·−2e7}}, column-major
        let sigma = vec![0.0, l * 3e7, l * 3e7, l * -2e7];
        model
            .apply_neumann_bc(
                &FromHigherDim::new(MatrixX::from_col_major(2, 2, sigma).unwrap()),
                "top",
            )
            .unwrap();
        model.solve_step(None).unwrap();
        let sp = format!("step{}/", k + 1);
        if !linear {
            let nls = model.solver.non_linear_solver(None).unwrap();
            assert_eq!(
                vec![nls.nb_iterations() as i64],
                i64s(&f, &format!("{sp}newton/nb_iterations")),
                "{name}: {sp} iterations"
            );
            n += assert_bits(&f, &format!("{sp}newton/error"), &[nls.error()]);
        }
        n += assert_bits(
            &f,
            &format!("{sp}external_force"),
            model.external_force().unwrap().as_slice(),
        );
        n += assert_bits(
            &f,
            &format!("{sp}displacement"),
            model.displacement().unwrap().as_slice(),
        );
        n += assert_bits(
            &f,
            &format!("{sp}internal_force"),
            model.internal_force().unwrap().as_slice(),
        );
        // full COO in storage order: the profile order of this case
        n += check_matrix(&f, &model, "K", &format!("{sp}K"));
        n += check_internals_all(&f, &model, &sp);
    }
    for m in 0..model.nb_materials() {
        model.material_mut(m).unwrap().on_dump().unwrap();
        let law = model.material(m).unwrap();
        for ty in law.base().filter_types(NG) {
            n += assert_bits(
                &f,
                &format!("cauchy/m{m}/stress/{}", tname(ty)),
                law.base().stress.get(ty, NG).unwrap().as_slice(),
            );
        }
    }
    let ep = model.energy("potential").unwrap();
    n += assert_bits(&f, "energy/potential", &[ep]);
    n
}

/// Two materials on a mixed mesh (regression test with its own oracle
/// cases): one `quadrangle_8` (material `a`, finite deformation) and
/// two `triangle_6` (material `b`: finite deformation, or small strain in
/// `_fd_lin`) by physical name, Neumann loads in two steps, static
/// Newton–Raphson or the `_linear` solver. Every material's filter has an
/// empty type: upstream's FD stiffness still registers that type's
/// connectivity in `K`'s profile, so `K`'s COO order (compared in full,
/// storage order) and, through the AIJ mat-vec of the `_linear` residual,
/// the step-2 solution depend on processing empty filters as upstream
/// (before the fix: `K` rows/cols permuted, `u` off by up to 8.9e-15
/// relative). Everything bitwise.
#[test]
fn fd_mixed_two_material_bitwise() {
    let cases = fd_cases("mixed_quad8_tri6_");
    assert_eq!(cases.len(), 3, "{cases:?}");
    let n: usize = cases.iter().map(|c| replay_mixed(c)).sum();
    eprintln!("{} cases, {n} values bitwise", cases.len());
}
