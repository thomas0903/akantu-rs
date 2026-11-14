// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Patch-test replay of
// test/test_model/patch_tests/{patch_test_linear_fixture.hh,
// patch_test_linear_solid_mechanics_fixture.hh,
// patch_test_linear_elastic_{explicit,implicit}.cc,
// patch_test_linear_anisotropic_{explicit,implicit}.cc} against
// fixtures/model/smm/patch_* (oracle/probes/smm/smm_probe.cc), shared by
// model_smm_fixtures.rs (the `default/auto[eigen]` instances, every recorded
// value bitwise), patch_mumps.rs (the `default/auto[mumps]` instances of
// upstream's typed solver list) and patch_petsc.rs (the
// `petsc/auto[petsc]` and `petsc/petsc_snes[petsc]` instances).

// shared by model_smm_fixtures.rs, patch_mumps.rs and patch_petsc.rs, each
// uses a subset
#![allow(dead_code)]
// Index loops mirror the upstream tensor expressions (C_ijkl ε_kl).
#![allow(clippy::needless_range_loop)]

use akantu_comm::Communicator;
use akantu_core::enums::{AnalysisMethod, MeshIoType, NonLinearSolverType, SparseSolverType};
use akantu_core::tensor::MatrixX;
use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FromHigherDim;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::anisotropic::CelerityMode;
use akantu_solver::ModelSolverOptions;
use akantu_validation::reduce::NanMinMax;
use akantu_validation::{Fixture, canonical_le_bytes};
use sha2::{Digest, Sha256};

use crate::model_common::*;

pub const ALPHA: [[Real; 4]; 3] = [
    [0.01, 0.02, 0.03, 0.04],
    [0.05, 0.06, 0.07, 0.08],
    [0.09, 0.10, 0.11, 0.12],
];

pub fn smm(name: &str) -> Fixture {
    case(&format!("smm/{name}"))
}

pub fn smm_cases(prefix: &str) -> Vec<String> {
    cases_of("smm")
        .into_iter()
        .map(|c| c.trim_start_matches("smm/").to_owned())
        .filter(|c| c.starts_with(prefix))
        .collect()
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

pub fn eigen_options(method: AnalysisMethod) -> ModelOptions {
    solver_options(method, SparseSolverType::Eigen)
}

/// `initFull(_analysis_method = method, _solver_options = {_auto, sparse})`
/// of upstream's `TestPatchTestLinear::initModel`.
pub fn solver_options(method: AnalysisMethod, sparse: SparseSolverType) -> ModelOptions {
    solver_options_with(method, NonLinearSolverType::Auto, sparse)
}

/// `initFull(_analysis_method = method, _solver_options = {nls, sparse})`.
pub fn solver_options_with(
    method: AnalysisMethod,
    nls: NonLinearSolverType,
    sparse: SparseSolverType,
) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(nls),
        sparse_solver_type: Some(sparse),
        ..ModelSolverOptions::default()
    })
}

pub fn element_type(name: &str) -> ElementType {
    name.parse()
        .unwrap_or_else(|_| panic!("element type {name}"))
}

/// Mesh of the fixture with upstream's topology after `buildFacets` +
/// `createBoundaryGroupFromGeometry`: the recorded connectivities (the
/// file's elements are a prefix, facets appended) and element groups.
pub fn mesh_of(f: &Fixture) -> Mesh {
    mesh_of_on(f, None)
}

/// [`mesh_of`] on the communicator `comm` (`None`: the default one).
pub fn mesh_of_on(f: &Fixture, comm: Option<&Communicator>) -> Mesh {
    let dim = dim_of(f);
    let mut mesh = match comm {
        None => read_mesh(&f.input_path("mesh").unwrap(), dim),
        Some(c) => {
            let mut m = Mesh::with_communicator(dim, "mesh", c.clone()).unwrap();
            akantu_io::read_mesh(&mut m, &f.input_path("mesh").unwrap(), MeshIoType::Gmsh).unwrap();
            m
        }
    };
    if let Some(types) = param(f, "mesh_types").as_array() {
        let mut acc = MeshAccessor::new(&mut mesh);
        for t in types {
            let name = t.as_str().unwrap();
            let ty = element_type(name);
            let want = i64s(f, &format!("mesh/connectivity/{name}"));
            let nnpe = ty.nb_nodes_per_element();
            let mut c = acc.connectivity_mut(ty, GhostType::NotGhost);
            let have: Vec<i64> = c.as_slice().iter().map(|&x| i64::from(x)).collect();
            if ty.spatial_dimension() == dim {
                assert_eq!(have, want, "{}: elements of {name}", f.case_id());
                continue;
            }
            // upstream buildFacets rebuilds the facet connectivities (the
            // facets read from the file are replaced, same count, other
            // order/orientation): take upstream's facets wholesale
            if !have.is_empty() {
                assert_eq!(
                    want.len(),
                    have.len(),
                    "{}: number of {name} facets",
                    f.case_id()
                );
            }
            c.replace(Array::from_vec(want.iter().map(|&x| x as Idx).collect(), nnpe).unwrap());
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
                        .add(Element::new(ty, e as Idx, GhostType::NotGhost), None, false)
                        .unwrap();
                }
            }
            for n in i64s(f, &format!("group/{name}/nodes")) {
                group.add_node(n as Idx, false);
            }
        }
        let names: Vec<String> = mesh
            .groups()
            .element_groups()
            .map(|g| g.name().to_string())
            .collect();
        let want: Vec<String> = groups
            .iter()
            .map(|g| g["name"].as_str().unwrap().to_owned())
            .collect();
        assert_eq!(names, want, "{}: element groups", f.case_id());
    }
    mesh
}

/// Model on the fixture's material file in the **default (corrected)
/// mode** of every upstream-defect switch, enabling per case only the
/// switches the case reproduces: the anisotropic patch
/// cases take their time step from the upstream celerity (UD-033, first
/// eigenvalue of Eigen's real Schur solver) and rotate the 3-D stiffness
/// with upstream's transposed axes (UD-104; `n1 = [-1, 1, 0]`, … in
/// `material_anisotropic_3.dat`); no SMM case needs a solver
/// (`UpstreamCompat`) or model (`SmmUpstreamCompat`) switch.
pub fn model_on(f: &Fixture, mesh: Mesh, id: &str) -> SolidMechanicsModel {
    let parser = akantu_parser::Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut model =
        SolidMechanicsModel::with_parser(akantu_mesh::SharedMesh::new(mesh), None, id, parser)
            .unwrap();
    if param(f, "law").as_str() == Some("anisotropic") {
        model.physics.law_options = Some(LawOptions {
            celerity: CelerityMode::UpstreamFirstEigenvalue,
            material_axes_as_directions: false,
            ..LawOptions::default()
        });
    }
    model
}

/// `setLinearDOF` of the upstream patch-test fixture (sequential sums).
pub fn set_linear_dof(dof: &mut [Real], coord: &[Real]) {
    for (i, d) in dof.iter_mut().enumerate() {
        *d = ALPHA[i][0];
        for (j, x) in coord.iter().enumerate() {
            *d += ALPHA[i][j + 1] * x;
        }
    }
}

pub fn state_digest(model: &SolidMechanicsModel) -> String {
    let mut h = Sha256::new();
    for a in [
        model.displacement().unwrap(),
        model.velocity().unwrap(),
        model.acceleration().unwrap(),
    ] {
        for x in a.as_slice() {
            // D-046: default NaNs canonicalised (no recorded digest changes)
            h.update(canonical_le_bytes(*x));
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

pub fn f64_digest(v: &[Real]) -> String {
    let mut h = Sha256::new();
    for x in v {
        h.update(canonical_le_bytes(*x));
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Matrix `id`: full COO arrays when recorded, else the digests.
pub fn check_matrix(f: &Fixture, model: &SolidMechanicsModel, id: &str) -> usize {
    check_matrix_as(f, model, id, id)
}

/// [`check_matrix`] of matrix `matrix` recorded under the prefix `id`.
pub fn check_matrix_as(f: &Fixture, model: &SolidMechanicsModel, id: &str, matrix: &str) -> usize {
    let has_param = !param(f, &format!("{id}/nnz")).is_null();
    if !has_param {
        assert!(
            !model.dof_manager().has_matrix(matrix)
                || model
                    .dof_manager()
                    .matrix(matrix)
                    .map(|m| m.nnz())
                    .unwrap_or(0)
                    == 0,
            "{}: matrix {id} exists in the port only",
            f.case_id()
        );
        return 0;
    }
    let m = model.dof_manager().matrix(matrix).unwrap();
    let (r, c, v) = m.coo();
    let rows: Vec<i64> = r.to_vec();
    let cols: Vec<i64> = c.to_vec();
    let mut h = Sha256::new();
    for x in rows.iter().chain(&cols) {
        h.update(x.to_le_bytes());
    }
    let coords: String = h.finalize().iter().map(|b| format!("{b:02x}")).collect();
    assert_eq!(
        coords,
        pstr(f, &format!("{id}/coords_sha256")),
        "{}: {id} coords",
        f.case_id()
    );
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
    if has(f, &format!("{id}/values")) {
        assert_eq!(rows, i64s(f, &format!("{id}/rows")));
        assert_eq!(cols, i64s(f, &format!("{id}/cols")));
        assert_bits(f, &format!("{id}/values"), v);
    }
    3 * v.len()
}

/// Material `m` internals (raw column-major buffers) and filters.
pub fn check_internals(f: &Fixture, model: &SolidMechanicsModel, m: usize, prefix: &str) -> usize {
    let law = model.material(m).unwrap();
    let mut n = 0;
    for ty in law.base().filter_types(GhostType::NotGhost) {
        if ty.spatial_dimension() != model.spatial_dimension() {
            continue;
        }
        let t = tname(ty);
        let s = law.base().stress.get(ty, GhostType::NotGhost).unwrap();
        let s = s.as_slice();
        let g = law.base().grad_u.get(ty, GhostType::NotGhost).unwrap();
        let g = g.as_slice();
        let key = format!("{prefix}stress/{t}/sha256");
        if param(f, &key).is_string() {
            assert_eq!(f64_digest(s), pstr(f, &key), "{}: {key}", f.case_id());
            let key = format!("{prefix}grad_u/{t}/sha256");
            assert_eq!(f64_digest(g), pstr(f, &key), "{}: {key}", f.case_id());
            n += s.len() + g.len();
        } else {
            n += assert_bits(f, &format!("{prefix}stress/{t}"), s);
            n += assert_bits(f, &format!("{prefix}grad_u/{t}"), g);
        }
        let filter: Vec<i64> = law
            .base()
            .element_filter
            .get(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .iter()
            .map(|&x| i64::from(x))
            .collect();
        assert_eq!(
            filter,
            i64s(f, &format!("{prefix}filter/{t}")),
            "{}: filter",
            f.case_id()
        );
    }
    n
}

pub fn set_newton(
    model: &mut SolidMechanicsModel,
    max: i64,
    threshold: Real,
    criterion: &str,
) -> bool {
    let nls = model.solver.non_linear_solver_mut(None).unwrap();
    if nls.as_newton_raphson_mut().is_none() {
        return false;
    }
    crate::model_common::set_newton(model, max, threshold, criterion);
    true
}

pub fn newton_history(model: &SolidMechanicsModel, its: &mut Vec<i64>, errs: &mut Vec<Real>) {
    let nls = model.solver.non_linear_solver(None).unwrap();
    its.push(nls.nb_iterations() as i64);
    errs.push(nls.error());
}

/* ---------------------------------------------------------------------- */
/* patch tests                                                             */
/* ---------------------------------------------------------------------- */

/// Solver configuration of a patch replay: one entry of upstream's typed
/// list `solver_options` (`patch_test_linear_solid_mechanics_fixture.hh:170-190`).
#[derive(Clone)]
pub enum PatchSolver {
    /// `{DOFManagerDefault, (auto, eigen)}` on the default communicator: the
    /// configuration of the oracle fixtures, every recorded value bitwise.
    Eigen,
    /// `{DOFManagerDefault, (auto, mumps)}` (upstream: MUMPS builds), the
    /// mesh — hence the DOF manager — on `comm` (an MPI communicator: the
    /// MUMPS library needs one; upstream runs `patch_test_linear` on one
    /// process). The values that do not depend on the linear solves (blocked
    /// DOFs, initial displacement, time steps, `K`, `M`, lumped mass) and
    /// every value of the explicit lumped-mass cases (no sparse solve) are
    /// still compared bit for bit with the Eigen fixture; the solution of the
    /// static and implicit cases is checked with upstream's own criteria
    /// (`checkAll`, `E_kin`), MUMPS and Eigen solves differing by rounding.
    Mumps(Communicator),
    /// `{DOFManagerPETSc, (auto, petsc)}` (`snes = false`: the model's
    /// Newton–Raphson, or the lumped solver of the explicit scheme, on the
    /// PETSc DOF manager, the linear solves by a KSP with upstream's
    /// `pc_type cholesky`, `ksp_rtol 1e-30`) and `{DOFManagerPETSc,
    /// (petsc_snes, petsc)}` (`snes = true`: PETSc SNES with upstream's
    /// `snes_max_it 10`, `snes_rtol 1e-10`, `snes_atol 2e-5`, `snes_stol
    /// 1e-10`; AnisotropicStatic `snes_max_it 2`, `snes_rtol 1e-10`) of
    /// upstream's PETSc builds (`initDOFManager("petsc")` in `SetUp`). The
    /// mesh is serial: the DOF manager runs on the MPI world of one process
    /// (D-092). The values that no sparse solve produces are compared bit
    /// for bit with the Eigen fixture as for `Mumps` (the matrices `K`, `M`
    /// have no COO access on the PETSc backend, as upstream's
    /// `SparseMatrixPETSc`); every value of the explicit lumped-mass cases of
    /// `auto[petsc]` too (no sparse solve).
    Petsc { snes: bool },
}

/// What a replay measured besides its assertions.
#[derive(Debug, Default)]
pub struct PatchReport {
    /// Values compared bit for bit with the fixture.
    pub bitwise: usize,
    /// Sparse solver installed for `J` (`None`: no sparse solver, as in the
    /// explicit lumped-mass scheme).
    pub sparse_kind: Option<SparseSolverType>,
    /// Newton–Raphson iterations of every solve (empty without Newton).
    pub iterations: Vec<i64>,
    /// The oracle's (Eigen) iteration counts of the same case.
    pub reference_iterations: Vec<i64>,
    /// Entries of the final displacement whose bits differ from the oracle's
    /// Eigen run, and the largest difference relative to its maximum.
    pub displacement_diff: (usize, Real),
    /// Final kinetic energy (dynamic cases).
    pub kinetic_energy: Option<Real>,
    /// Kind of the model's non-linear solver.
    pub nls_kind: Option<NonLinearSolverType>,
    /// Kind of the DOF manager's backend.
    pub backend: Option<akantu_solver::backend::BackendKind>,
}

/// The `default/auto[eigen]` replay of fixture case `name`: every recorded
/// value bitwise plus upstream's checks; returns the number of values
/// compared bit for bit.
pub fn replay_patch(name: &str) -> usize {
    replay_patch_with(name, &PatchSolver::Eigen).bitwise
}

/// Replay of fixture case `name` with the solver configuration `solver`
/// (see [`PatchSolver`] for what is bitwise and what is upstream's
/// criterion).
pub fn replay_patch_with(name: &str, solver: &PatchSolver) -> PatchReport {
    let f = smm(name);
    let dim = dim_of(&f);
    let ty = element_type(&pstr(&f, "element_type"));
    let analysis = pstr(&f, "analysis");
    let method = match analysis.as_str() {
        "static" => AnalysisMethod::Static,
        "explicit" => AnalysisMethod::ExplicitLumpedMass,
        _ => AnalysisMethod::ImplicitDynamic,
    };
    let (comm, sparse, snes) = match solver {
        PatchSolver::Eigen => (None, SparseSolverType::Eigen, false),
        PatchSolver::Mumps(c) => (Some(c), SparseSolverType::Mumps, false),
        PatchSolver::Petsc { snes } => (None, SparseSolverType::Petsc, *snes),
    };
    let petsc = sparse == SparseSolverType::Petsc;
    let nls = if snes {
        NonLinearSolverType::PetscSnes
    } else {
        NonLinearSolverType::Auto
    };
    // values computed from the solution: bitwise with the Eigen fixture only
    // when no sparse solve of another backend produced them (SNES solves
    // the explicit scheme too)
    let exact = sparse == SparseSolverType::Eigen
        || (method == AnalysisMethod::ExplicitLumpedMass && !snes);
    let mesh = mesh_of_on(&f, comm);
    let mut model = model_on(&f, mesh, &pstr(&f, "model_id"));
    if petsc {
        // upstream SetUp: `model->initDOFManager("petsc")` (serial mesh: the
        // MPI world of one process, D-092)
        model.init_dof_manager("petsc").unwrap();
    }
    model
        .init_full(&solver_options_with(method, nls, sparse))
        .unwrap();
    let mut n = 0;
    // applyBC: every node of every element group, blocked at the affine field
    {
        let mesh_h = model.physics.base.mesh.clone();
        let mesh = mesh_h.read().unwrap();
        let x = mesh.nodes().as_slice().to_vec();
        let nodes: Vec<usize> = mesh
            .groups()
            .element_groups()
            .flat_map(|g| g.nodes().iter().map(|&n| n as usize).collect::<Vec<_>>())
            .collect();
        drop(mesh);
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
    let blocked = bools(&f, "blocked_dofs");
    assert_eq!(
        model.blocked_dofs().unwrap().as_slice(),
        &blocked[..],
        "{name}: blocked"
    );
    n += assert_bits(
        &f,
        "initial_displacement",
        model.displacement().unwrap().as_slice(),
    );
    // probe settings: max_iterations 3 / threshold 1e-6 / solution, except
    // the AnisotropicStatic cases (recorded: 2 / 1e-10 / residual)
    let anisotropic_static = !param(&f, "newton_max_iterations").is_null();
    let newton = if snes {
        // upstream initModel (`_petsc_snes`), then AnisotropicStatic's own
        set_snes(&mut model, anisotropic_static);
        false
    } else if !anisotropic_static {
        set_newton(&mut model, 3, 1e-6, "solution")
    } else {
        set_newton(
            &mut model,
            param(&f, "newton_max_iterations").as_i64().unwrap(),
            param(&f, "newton_threshold").as_f64().unwrap(),
            &pstr(&f, "newton_convergence_type"),
        )
    };
    if !snes {
        assert_eq!(
            newton,
            param(&f, "newton").as_bool().unwrap(),
            "{name}: solver kind"
        );
    }
    if petsc && newton {
        // upstream initModel (DOFManagerPETSc, Newton–Raphson): the sparse
        // solver's `pc_type cholesky`, `ksp_rtol 1e-30` (formatted as
        // ParsablePETSc's stream does)
        let dofs = model.dof_manager_mut();
        for (k, v) in [("pc_type", "cholesky"), ("ksp_rtol", "1e-30")] {
            dofs.backend_mut()
                .and_then(|b| b.set_solver_option("J", k, v))
                .unwrap_or_else(|e| panic!("{name}: {k}: {e}"));
        }
    }
    let converged = |model: &SolidMechanicsModel, step: usize| {
        if newton {
            let nls = model.solver.non_linear_solver(None).unwrap();
            assert!(nls.converged(), "{name}: solve {step} did not converge");
        }
    };
    let (mut its, mut errs) = (Vec::new(), Vec::new());
    let mut kinetic_energy = None;
    if method == AnalysisMethod::Static {
        model
            .solve_step(None)
            .unwrap_or_else(|e| panic!("{name}: solve 1: {e}"));
        converged(&model, 1);
        newton_history(&model, &mut its, &mut errs);
    } else {
        let stable = model.stable_time_step().unwrap();
        let dt = 0.8 * stable;
        n += assert_bits(&f, "time_step", &[stable, dt]);
        model.set_time_step(dt, None).unwrap();
        {
            let mesh_h = model.physics.base.mesh.clone();
            let mesh = mesh_h.read().unwrap();
            let x = mesh.nodes().as_slice().to_vec();
            drop(mesh);
            let mut u = model.displacement_mut().unwrap();
            let u = u.as_mut_slice();
            for (ui, xi) in u.chunks_exact_mut(dim).zip(x.chunks_exact(dim)) {
                set_linear_dof(ui, xi);
            }
        }
        let digests = param(&f, "step_digests").as_array().unwrap().clone();
        for (s, d) in digests.iter().enumerate() {
            model
                .solve_step(None)
                .unwrap_or_else(|e| panic!("{name}: solve {}: {e}", s + 1));
            converged(&model, s + 1);
            if exact {
                assert_eq!(
                    state_digest(&model),
                    d.as_str().unwrap(),
                    "{name}: state after step {} differs",
                    s + 1
                );
            }
            if newton || snes {
                newton_history(&model, &mut its, &mut errs);
            }
        }
        let ek = model.energy("kinetic").unwrap();
        kinetic_energy = Some(ek);
        if exact {
            n += digests.len() * 3 * model.displacement().unwrap().len() * dim;
            n += assert_bits(&f, "energy/kinetic_final", &[ek]);
            n += assert_bits(&f, "velocity", model.velocity().unwrap().as_slice());
            n += assert_bits(&f, "acceleration", model.acceleration().unwrap().as_slice());
        }
    }
    let reference_iterations = if newton {
        i64s(&f, "newton/nb_iterations")
    } else {
        Vec::new()
    };
    if newton && exact {
        assert_eq!(its, reference_iterations, "{name}: iterations");
        n += assert_bits(&f, "newton/error", &errs);
    }
    let displacement_diff = diff_stats(
        model.displacement().unwrap().as_slice(),
        &f64s(&f, "displacement"),
    );
    if exact {
        n += assert_bits(&f, "displacement", model.displacement().unwrap().as_slice());
        n += assert_bits(
            &f,
            "internal_force",
            model.internal_force().unwrap().as_slice(),
        );
        n += check_internals(&f, &model, 0, "");
    }
    if !petsc {
        // the PETSc backend has no COO access (upstream SparseMatrixPETSc)
        n += check_matrix(&f, &model, "K");
        n += check_matrix(&f, &model, "M");
    }
    if has(&f, "mass") {
        n += assert_bits(&f, "mass", model.mass().unwrap().as_slice());
    }
    // checkForces: f_ext from FromHigherDim(σ of quadrature point 0)
    let sigma: Vec<Real> = model
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()[..dim * dim]
        .to_vec();
    model.external_force_mut().unwrap().zero();
    if dim > 1 {
        let groups: Vec<String> = {
            let mesh = model.physics.base.mesh.read().unwrap();
            mesh.groups()
                .element_groups()
                .map(|g| g.name().to_string())
                .collect()
        };
        let functor = FromHigherDim::new(MatrixX::from_col_major(dim, dim, sigma).unwrap());
        for g in groups {
            model.apply_neumann_bc(&functor, &g).unwrap();
        }
    } else {
        let mut fe = model.external_force_mut().unwrap();
        let fe = fe.as_mut_slice();
        fe[0] = -sigma[0];
        fe[1] = sigma[0];
    }
    if exact {
        n += assert_bits(
            &f,
            "external_force_neumann",
            model.external_force().unwrap().as_slice(),
        );
    }
    // the upstream test's own assertions (same intent as the C++ test)
    check_all(&f, &model, ty);
    if let Some(ek) = kinetic_energy {
        let tol = if analysis == "explicit" { 1e-16 } else { 1e-13 };
        if pstr(&f, "law") != "anisotropic" || analysis == "explicit" {
            assert!(ek.abs() < tol, "{name}: E_kin = {ek}");
        }
    }
    PatchReport {
        bitwise: n,
        sparse_kind: model.dof_manager().backend().sparse_solver_kind("J"),
        iterations: its,
        reference_iterations,
        displacement_diff,
        kinetic_energy,
        nls_kind: model.solver.non_linear_solver(None).ok().map(|s| s.kind()),
        backend: Some(model.dof_manager().backend().kind()),
    }
}

/// Upstream `initModel`'s SNES options (`snes_max_it 10`, `snes_rtol 1e-10`,
/// `snes_atol 2e-5`, `snes_stol 1e-10`), then AnisotropicStatic's
/// (`snes_max_it 2`, `snes_rtol 1e-10`).
pub fn set_snes(model: &mut SolidMechanicsModel, anisotropic_static: bool) {
    use akantu_parser::ParamValue::{Int, Real as R};
    let nls = model.solver.non_linear_solver_mut(None).unwrap();
    let mut opts = vec![
        ("snes_max_it", Int(10)),
        ("snes_rtol", R(1e-10)),
        ("snes_atol", R(2e-5)),
        ("snes_stol", R(1e-10)),
    ];
    if anisotropic_static {
        opts.extend([("snes_max_it", Int(2)), ("snes_rtol", R(1e-10))]);
    }
    for (k, v) in opts {
        nls.set_param(k, v)
            .unwrap_or_else(|e| panic!("SNES option {k}: {e}"));
    }
}

/// The class of two backward-stable direct solves of the same matrix
/// (NUMERICAL_VALIDATION, `mumps_mpi_solver`): equal Newton–Raphson
/// iteration counts and the final displacement within `1e-12` of its
/// maximum against the oracle's Eigen run. `None` when
/// it holds or the case has no Newton solve.
pub fn direct_solver_class(r: &PatchReport) -> Option<String> {
    if r.reference_iterations.is_empty() {
        return None;
    }
    if r.iterations != r.reference_iterations {
        return Some(format!(
            "Newton iterations {:?}, Eigen oracle {:?}",
            r.iterations, r.reference_iterations
        ));
    }
    if r.displacement_diff.1.is_nan() || r.displacement_diff.1 > 1e-12 {
        return Some(format!(
            "u vs the Eigen oracle: max rel {:.3e} > 1e-12",
            r.displacement_diff.1
        ));
    }
    None
}

/* ---------------------------------------------------------------------- */
/* upstream checkAll (patch_test_linear_fixture.hh,                         */
/* patch_test_linear_solid_mechanics_fixture.hh, anisotropic tests)         */
/* ---------------------------------------------------------------------- */

/// `C[i][j][k][l]` of `patch_test_linear_anisotropic_{explicit,implicit}.cc`.
pub fn anisotropic_c(dim: usize) -> Vec<Vec<Vec<Vec<Real>>>> {
    let c3: [[[[Real; 3]; 3]; 3]; 3] = [
        [
            [
                [112.93753505, 1.85842452538e-10, -4.47654358027e-10],
                [1.85847317471e-10, 54.2334345331, -3.69840984824],
                [-4.4764768395e-10, -3.69840984824, 56.848605217],
            ],
            [
                [1.85847781609e-10, 25.429294233, -3.69840984816],
                [25.429294233, 3.31613847493e-10, -8.38797920011e-11],
                [-3.69840984816, -8.38804581349e-11, -1.97875715813e-10],
            ],
            [
                [-4.47654358027e-10, -3.69840984816, 28.044464917],
                [-3.69840984816, 2.09374961813e-10, 9.4857455224e-12],
                [28.044464917, 9.48308098714e-12, -2.1367885239e-10],
            ],
        ],
        [
            [
                [1.85847781609e-10, 25.429294233, -3.69840984816],
                [25.429294233, 3.31613847493e-10, -8.38793479119e-11],
                [-3.69840984816, -8.38795699565e-11, -1.97876381947e-10],
            ],
            [
                [54.2334345331, 3.31617400207e-10, 2.09372075233e-10],
                [3.3161562385e-10, 115.552705733, -3.15093728886e-10],
                [2.09372075233e-10, -3.15090176173e-10, 54.2334345333],
            ],
            [
                [-3.69840984824, -8.38795699565e-11, 9.48219280872e-12],
                [-8.38795699565e-11, -3.1509195253e-10, 25.4292942335],
                [9.48441325477e-12, 25.4292942335, 3.69840984851],
            ],
        ],
        [
            [
                [-4.47653469848e-10, -3.69840984816, 28.044464917],
                [-3.69840984816, 2.09374073634e-10, 9.48752187924e-12],
                [28.044464917, 9.48552347779e-12, -2.1367885239e-10],
            ],
            [
                [-3.69840984824, -8.3884899027e-11, 9.48219280872e-12],
                [-8.3884899027e-11, -3.150972816e-10, 25.4292942335],
                [9.48041645188e-12, 25.4292942335, 3.69840984851],
            ],
            [
                [56.848605217, -1.97875493768e-10, -2.13681516925e-10],
                [-1.97877270125e-10, 54.2334345333, 3.69840984851],
                [-2.13683293282e-10, 3.69840984851, 112.93753505],
            ],
        ],
    ];
    let mut c = vec![vec![vec![vec![0.0; dim]; dim]; dim]; dim];
    match dim {
        1 => c[0][0][0][0] = 105.092023,
        2 => {
            c[0][0][0][0] = 112.93753504999995;
            c[1][1][1][1] = 112.93753504999995;
            c[0][0][1][1] = 51.618263849999984;
            c[1][1][0][0] = 51.618263849999984;
            for (i, j, k, l) in [(0, 1, 0, 1), (1, 0, 0, 1), (0, 1, 1, 0), (1, 0, 1, 0)] {
                c[i][j][k][l] = 22.814123549999987;
            }
        }
        _ => {
            for i in 0..3 {
                for j in 0..3 {
                    for k in 0..3 {
                        for l in 0..3 {
                            c[i][j][k][l] = c3[i][j][k][l];
                        }
                    }
                }
            }
        }
    }
    c
}

pub fn inf_norm(v: impl IntoIterator<Item = Real>) -> Real {
    v.into_iter().fold(0.0, |a, x| a.nan_max(x.abs()))
}

/// Upstream `checkAll` (elastic) / the anisotropic tests' checks, with the
/// upstream tolerances: DOFs (1e-10 absolute), gradient (1e-8 relative),
/// stresses against the analytical constitutive response (1e-9; 1e-11
/// anisotropic), force balance after the Neumann load (1e-8, elastic only)
/// and the final kinetic energy (explicit 1e-16, implicit elastic 1e-13).
pub fn check_all(f: &Fixture, model: &SolidMechanicsModel, ty: ElementType) {
    let dim = model.spatial_dimension();
    let law_kind = pstr(f, "law");
    let anisotropic = law_kind == "anisotropic";
    let x = model
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    let u = model.displacement().unwrap();
    let u = u.as_slice();
    let mut ref_dof = vec![0.0; dim];
    for (ui, xi) in u.chunks_exact(dim).zip(x.chunks_exact(dim)) {
        set_linear_dof(&mut ref_dof, xi);
        let e = inf_norm(ui.iter().zip(&ref_dof).map(|(a, b)| a - b));
        assert!(e < 1e-10, "{}: checkDOFs {e}", f.case_id());
    }
    let grad = |r: usize, c: usize| ALPHA[r][c + 1];
    let law = model.material(0).unwrap();
    for g in law
        .base()
        .grad_u
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .chunks_exact(dim * dim)
    {
        let e = inf_norm((0..dim * dim).map(|k| g[k] - grad(k % dim, k / dim)))
            / inf_norm(g.iter().copied());
        assert!(e < 1e-8, "{}: checkGradient {e}", f.case_id());
    }
    let strain = |i: usize, j: usize| (grad(i, j) + grad(j, i)) / 2.;
    let presult: Vec<Real> = if anisotropic {
        let c = anisotropic_c(dim);
        (0..dim * dim)
            .map(|k| {
                let (i, j) = (k % dim, k / dim);
                let mut s = 0.;
                for kk in 0..dim {
                    for l in 0..dim {
                        s += c[i][j][kk][l] * strain(kk, l);
                    }
                }
                s
            })
            .collect()
    } else {
        let num = |n: &str| match law.get_param(n).unwrap() {
            akantu_parser::ParamValue::Real(v) => v,
            other => panic!("{n}: {other:?}"),
        };
        let (nu, e) = (num("nu"), num("E"));
        let plane_strain = !f.case_id().ends_with("plane_stress");
        let mut lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
        let mu = e / (2. * (1. + nu));
        if !plane_strain {
            lambda = nu * e / (1. - nu * nu);
        }
        let trace: Real = (0..dim).map(|i| strain(i, i)).sum();
        (0..dim * dim)
            .map(|k| {
                let (i, j) = (k % dim, k / dim);
                if dim == 1 {
                    e * strain(i, j)
                } else {
                    (if i == j { lambda * trace } else { 0. }) + 2. * mu * strain(i, j)
                }
            })
            .collect()
    };
    // upstream result_tolerance: 1e-11 for the anisotropic dynamic tests,
    // 1e-9 for AnisotropicStatic and the elastic tests
    let tol = if anisotropic && pstr(f, "analysis") != "static" {
        1e-11
    } else {
        1e-9
    };
    for s in law
        .base()
        .stress
        .get(ty, GhostType::NotGhost)
        .unwrap()
        .as_slice()
        .chunks_exact(dim * dim)
    {
        let e = inf_norm(s.iter().zip(&presult).map(|(a, b)| a - b))
            / inf_norm(presult.iter().copied());
        assert!(e < tol, "{}: checkResults {e}", f.case_id());
    }
    if !anisotropic {
        let fi = model.internal_force().unwrap();
        let fi = fi.as_slice();
        let fe = model.external_force().unwrap();
        let fe = fe.as_slice();
        let fmax = inf_norm(fi.iter().copied());
        let total = inf_norm((0..dim).map(|d| fi.iter().skip(d).step_by(dim).sum::<Real>()));
        assert!(
            total / fmax < 1e-8,
            "{}: checkForces total {}",
            f.case_id(),
            total / fmax
        );
        for (a, b) in fi.iter().zip(fe) {
            assert!(
                (a + b).abs() / fmax < 1e-8,
                "{}: checkForces f_int + f_ext",
                f.case_id()
            );
        }
    }
}

pub fn replay_patch_family(prefix: &str) -> (usize, usize) {
    let cases = smm_cases(prefix);
    assert!(!cases.is_empty(), "no case {prefix}*");
    let mut total = 0;
    for c in &cases {
        total += replay_patch(c);
    }
    (cases.len(), total)
}
