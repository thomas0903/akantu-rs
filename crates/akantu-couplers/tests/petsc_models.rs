// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_phase_field_model/
// {test_phase_field_optimal_linear.cc (TEST-095), test_phase_field_tao.cc
// (TEST-100)} with their inputs (material_optimal_linear.dat,
// material_linear_tao.dat, test_optimal_damage.msh, test_one_element.msh),
// src/model/common/model_solver.cc:98-121 (initDOFManager(type)).

//! The models on the PETSc DOF manager, `harness = false`: run
//! under `mpirun -np 1/2/4` by the PETSc tier (`scripts/petsc/
//! run-petsc-tier.sh`, features `petsc`; image `akantu-rs-petsc:dev`).
//! Scenarios print one `PASS`/`FAIL` line on rank 0 and `INFO` lines with
//! the measured values; `AKANTU_PETSC_THREAD_LEVEL` selects the MPI thread
//! level as for `petsc_mpi`.
//!
//! * `init_dof_manager_rules` (DOF-015): `init_dof_manager("petsc")`
//!   and `dof_manager_type = petsc` on the phase-field and solid models,
//!   upstream's id and message, UD-1020, the one-process rule of a serial
//!   mesh (refused on every rank of a larger job).
//! * `other_models_on_petsc` (DOF-015): diffusion,
//!   structural and cohesive models with `dof_manager_type = petsc` — the
//!   heat static patch test and upstream's 2-D Bernoulli beam test solved on
//!   PETSc (one process), the cohesive construction and its refused
//!   `set_symmetry_policy`; the serial-mesh refusal at more ranks.
//! * `tao_bounds_after_a_node_removal` (PETSc tier, D-095): TAO bounds
//!   set, a node
//!   removed, the bounded solve refused (recoverable), the bounds set
//!   again, the solve equal (bitwise) to a fresh model's on the final mesh.
//! * `test_phase_field_optimal_linear` (TEST-095, PF-015) and
//!   `test_phase_field_tao` (TEST-100, PF-015, NLS-007): upstream's tests
//!   with their checks and tolerances (serial upstream tests: one process;
//!   at more ranks the scenario checks the refusal of the serial mesh).
//! * `phase_field_tao_oracle` (PF-015, PF-018, NLS-007): the replay of the
//!   oracle records `fixtures/solver/petsc/oracle/pftao/*` (probe
//!   `oracle/probes/pftao`), **bitwise** (damage, energies, loading
//!   history) with every phase-field upstream-compatibility switch and the
//!   DOF manager's UD-422 switch (`petsc_row_oriented_blocks`: the oracle
//!   runs unpatched upstream, which inserts every elemental block
//!   transposed, D-088 (2)), plus the bounds and
//!   irreversibility of every law × split variant.
//! * `coupler_tao_oracle` (INT-004): TEST-100 itself
//!   through `CouplerSolidPhaseField` for every law × split (the records
//!   `fixtures/solver/petsc/oracle/pftao/cpl_tao_*`, probe `pftao` version
//!   2): **bitwise** (per step the nodal damage, both energies, the solid
//!   Newton count and error, the test's check values and digests of the
//!   material stress / damage and the law's strain / phi; the final arrays)
//!   under every upstream switch and UD-422, and again with the corrected
//!   defaults plus only the switches the cell needs (UD-422, and UD-387 for
//!   the volumetric–deviatoric split); the full defaults are run as well and
//!   what differs is printed (`INFO` lines), with the bounds and the
//!   irreversibility of every run checked. The full-defaults run must
//!   differ from the oracle, and so must, for the two-switch
//!   (volumetric–deviatoric) cells, the runs that drop one of the two
//!   switches (each switch is necessary).
//! * `test_phase_field_optimal_linear_distributed` (PF-015,
//!   TEST-095, DOF-015): TEST-095 on a distributed mesh with the PETSc DOF
//!   manager on the mesh's communicator, at 1 / 2 / 4 ranks: with PETSc's
//!   default preconditioner upstream's criterion (1e-5) and the bounds
//!   (bit for bit upstream's one-process result at one rank); with the
//!   partition-independent `pc_type none` the `N`-rank run against the
//!   same run on one process within D-082's distributed-vs-serial class
//!   (1e-14).
//!
//! Isolated cases (`AKANTU_PETSC_CASE=<case>`, one `mpirun` each, run by
//! the PETSc tier): the program holds **no MPI handle of its own** and
//! starts no PETSc session before the case, so the lifetime of the
//! serial-mesh rule's process-wide world is what is tested:
//! `pt02_serial_models_in_sequence` (one process) and
//! `pt02_refusal_recoverable` (2 / 4 processes).

mod common;

use std::path::PathBuf;
use std::time::{Duration, Instant};

use akantu_comm::lifecycle::ThreadLevel;
use akantu_comm::{Communicator, MpiOptions, ReduceOp};
use akantu_core::enums::MeshIoType;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_core::{ElementType, Error, GhostType, Real};
use akantu_couplers::CouplerSolidPhaseField;
use akantu_linalg::petsc;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_model::constitutive_law::InternalFieldRef;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_phase_field::selector::MeshDataPhaseFieldSelector;
use akantu_phase_field::{PhaseFieldModel, PhaseFieldUpstreamCompat};
use akantu_solid::SolidMechanicsModel;
use akantu_solid::material::LawOptions;
use akantu_solver::{BackendKind, ModelSolverOptions};
use akantu_validation::fixture::{oracle_report_line, oracle_values_not_comparable};
use akantu_validation::fixtures_root;

use common::{read_mesh, sha_hex};

type Outcome = std::result::Result<(), String>;

const Q4: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

fn e2s(e: Error) -> String {
    e.to_string()
}

fn thread_level() -> ThreadLevel {
    match std::env::var("AKANTU_PETSC_THREAD_LEVEL").as_deref() {
        Ok("serialized") => ThreadLevel::Serialized,
        Ok("multiple") => ThreadLevel::Multiple,
        _ => ThreadLevel::Funneled,
    }
}

fn world() -> Communicator {
    Communicator::mpi_world(MpiOptions {
        thread_level: thread_level(),
        timeout: Some(Duration::from_secs(600)),
        ..MpiOptions::default()
    })
    .expect("MPI world")
}

/// Upstream's inputs of test/test_model/test_phase_field_model (copied in
/// akantu-phase-field's test data).
fn pf_data(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../akantu-phase-field/tests/data/test_phase_field_model")
        .join(name)
}

/// A phase-field model on an upstream mesh with an input file (text).
fn phase_model(msh: &str, dat_text: &str) -> Result<PhaseFieldModel, String> {
    let mesh = read_mesh(&pf_data(msh), 2);
    let parser = Parser::parse_str(dat_text, "<input>").map_err(e2s)?;
    PhaseFieldModel::with_parser(SharedMesh::new(mesh), None, "phase_field_model", parser)
        .map_err(e2s)
}

fn read_text(name: &str) -> Result<String, String> {
    std::fs::read_to_string(pf_data(name)).map_err(|e| format!("{name}: {e}"))
}

/// `_static` with `_petsc_tao` + `_petsc` (upstream `ModelSolverOptions{…}`).
fn tao_options() -> ModelOptions {
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    o.solver_options.non_linear_solver_type = Some(NonLinearSolverType::PetscTao);
    o.solver_options.sparse_solver_type = Some(SparseSolverType::Petsc);
    o
}

/// Increment bounds of the phase-field TAO solve: `lower = 0`, `upper = 1`,
/// then (`from_damage`) `assemble_to_global_array("damage", d, upper, −1)`
/// — upstream's `SolverVectorPETSc` + `assembleToGlobalArray` sequence —
/// and `setBounds`.
fn phase_bounds(phase: &mut PhaseFieldModel, from_damage: bool) -> Outcome {
    let dm = &phase.solver.dof_manager;
    let n = dm.local_system_size();
    let lower = vec![0.0; n];
    let mut upper = vec![1.0; n];
    if from_damage {
        let d = dm.dofs("damage").map_err(e2s)?.as_slice().to_vec();
        dm.assemble_to_global_array("damage", &d, &mut upper, -1.0)
            .map_err(e2s)?;
    }
    phase
        .solver
        .non_linear_solver_mut(None)
        .map_err(e2s)?
        .as_native_mut()
        .ok_or("not a native solver")?
        .set_bounds(lower, upper)
        .map_err(e2s)
}

fn set(phase: &mut PhaseFieldModel, name: &str, v: ParamValue) -> Outcome {
    phase
        .solver
        .non_linear_solver_mut(None)
        .map_err(e2s)?
        .set_param(name, v)
        .map_err(e2s)
}

fn agree(c: &Communicator, fails: Vec<String>) -> Outcome {
    let mut ok = [u8::from(fails.is_empty())];
    c.all_reduce(&mut ok, ReduceOp::Min).map_err(e2s)?;
    if !fails.is_empty() {
        return Err(fails.join("; "));
    }
    if ok[0] != 1 {
        return Err("failed on another rank".into());
    }
    Ok(())
}

// ------------------------------------------------------------

/// An AT1 law with `irreversibility_type = tao` (inside a model section).
const TAO_LAW: &str = "  phasefield linear no_split [\n    name = plate\n    E = 210.0\n    nu = \
                       0.3\n    gc = 5e-3\n    l0 = 1.\n    irreversibility_type = tao\n  ]\n";

fn tao_material(extra: &str) -> String {
    format!("model phase_field_model [\n{extra}{TAO_LAW}]\n")
}

/// See the module documentation.
fn init_dof_manager_rules(c: &Communicator) -> Outcome {
    let mut fails = Vec::new();
    let serial = c.size() == 1;
    // Phase field, init_dof_manager("petsc").
    let mut pf = phase_model("test_one_element.msh", &tao_material(""))?;
    match pf.init_dof_manager("petsc") {
        Ok(()) if serial => {
            let dm = &pf.solver.dof_manager;
            if dm.id() != "phase_field_model:dof_manager_petsc"
                || dm.backend().kind() != BackendKind::Petsc
                || !dm.communicator().is_mpi()
            {
                fails.push(format!(
                    "phase field: manager {} kind {:?} mpi {}",
                    dm.id(),
                    dm.backend().kind(),
                    dm.communicator().is_mpi()
                ));
            }
        }
        Ok(()) => fails.push("a serial mesh on several processes was accepted".into()),
        Err(Error::Unsupported(m)) if !serial => {
            if !m.contains("serial (not distributed) mesh runs on MPI_COMM_WORLD") {
                fails.push(format!("refusal text: {m}"));
            }
        }
        Err(e) => fails.push(format!("init_dof_manager(petsc): {e}")),
    }
    // Upstream's message for an unknown type (the factory text appended).
    match pf.init_dof_manager("mumps_like") {
        Err(Error::InvalidArgument(m))
            if m == "To use the solver mumps_like you will have to code it. This is an unknown \
                     solver type.The id \"mumps_like\" is not registered in the \
                     akantu::DOFManager factory." => {}
        other => fails.push(format!("unknown type: {other:?}")),
    }
    if serial {
        // UD-1020: refused once a solver exists.
        if let Err(e) = pf.init_full(&tao_options()) {
            fails.push(format!("init_full on PETSc: {e}"));
        }
        match pf.init_dof_manager("petsc") {
            Err(Error::InvalidArgument(m)) if m.contains("UD-1020") => {}
            other => fails.push(format!("init_dof_manager after init_full: {other:?}")),
        }
        // The model section's dof_manager_type (upstream initDOFManager(section)).
        let text = tao_material("  dof_manager_type = petsc\n");
        match phase_model("test_one_element.msh", &text) {
            Ok(m) if m.solver.dof_manager.backend().kind() == BackendKind::Petsc => {}
            Ok(m) => fails.push(format!(
                "section dof_manager_type = petsc: kind {:?}",
                m.solver.dof_manager.backend().kind()
            )),
            Err(e) => fails.push(format!("section dof_manager_type = petsc: {e}")),
        }
        // Solid model.
        let mesh = read_mesh(&pf_data("test_one_element.msh"), 2);
        let parser = Parser::parse_str(
            "material elastic [\n  name = plate\n  rho = 1\n  E = 210\n  nu = 0.3\n]\n",
            "<input>",
        )
        .map_err(e2s)?;
        let mut smm = SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser)
            .map_err(e2s)?;
        smm.init_dof_manager("petsc").map_err(e2s)?;
        let id = smm.solver.dof_manager.id().to_owned();
        let kind = smm.solver.dof_manager.backend().kind();
        if id != "smm:dof_manager_petsc" || kind != BackendKind::Petsc {
            fails.push(format!("solid: manager {id} kind {kind:?}"));
        }
    }
    if c.rank() == 0 {
        println!(
            "INFO init_dof_manager_rules: ranks={} (serial mesh {})",
            c.size(),
            if serial { "accepted" } else { "refused" }
        );
    }
    agree(c, fails)
}

// ------------------------------------------------------------

/// The serial-mesh rule's refusal (D-092): a model on a serial mesh with
/// the PETSc DOF manager runs on `MPI_COMM_WORLD`, refused on a larger job.
const SERIAL_MESH_RULE: &str = "serial (not distributed) mesh runs on MPI_COMM_WORLD";

/// The construction of a model with `dof_manager_type = petsc`:
/// on one process its DOF manager must be the PETSc one with
/// upstream's id `<id>:dof_manager_petsc`; on several the serial mesh must be
/// refused with the D-092 text. Returns the model when it is usable.
fn on_petsc<M>(
    what: &str,
    serial: bool,
    id: &str,
    built: Result<M, Error>,
    manager: impl Fn(&M) -> (String, BackendKind),
    fails: &mut Vec<String>,
) -> Option<M> {
    match built {
        Ok(m) if serial => {
            let (got, kind) = manager(&m);
            if got != format!("{id}:dof_manager_petsc") || kind != BackendKind::Petsc {
                fails.push(format!("{what}: manager {got} kind {kind:?}"));
                return None;
            }
            Some(m)
        }
        Ok(_) => {
            fails.push(format!(
                "{what}: a serial mesh on several processes was accepted"
            ));
            None
        }
        Err(Error::Unsupported(m)) if !serial && m.contains(SERIAL_MESH_RULE) => None,
        Err(e) => {
            fails.push(format!("{what}: {e}"));
            None
        }
    }
}

/// Upstream's linear heat-transfer static patch test
/// (`patch_test_linear_heat_transfer_static.cc`, TEST-PATCH heat) on a unit
/// square of eight `_triangle_3` (nine nodes, the centre free): the
/// boundary temperature is affine, the solution must be that affine field
/// within upstream's `dofs_tolerance` (1e-10, absolute).
fn diffusion_patch(c: &Communicator, fails: &mut Vec<String>) -> Outcome {
    const ALPHA: [Real; 3] = [0.01, 0.02, 0.03];
    let affine = |x: &[Real]| ALPHA[0] + ALPHA[1] * x[0] + ALPHA[2] * x[1];
    let mut mesh = Mesh::new(2, "pt06_square").map_err(e2s)?;
    {
        let mut a = akantu_mesh::MeshAccessor::new(&mut mesh);
        for j in 0..3 {
            for i in 0..3 {
                a.push_node(&[0.5 * Real::from(i), 0.5 * Real::from(j)])
                    .map_err(e2s)?;
            }
        }
        let mut t = a.connectivity_mut(ElementType::Triangle3, NG);
        for j in 0..2 {
            for i in 0..2 {
                let n = j * 3 + i;
                t.push_tuple(&[n, n + 1, n + 4]);
                t.push_tuple(&[n, n + 4, n + 3]);
            }
        }
    }
    mesh.compute_bounding_box().map_err(e2s)?;
    let text = "model heat_transfer_model [\n  dof_manager_type = petsc\n  constitutive_law \
                heat_diffusion [\n    name = m\n    density = 1\n    capacity = 1\n    \
                conductivity = [[1, 0], [0, 1]]\n  ]\n]\n";
    let parser = Parser::parse_str(text, "<pt06 heat>").map_err(e2s)?;
    let built = akantu_diffusion::HeatTransferModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "pt06_heat",
        parser,
    );
    let Some(mut model) = on_petsc(
        "diffusion",
        c.size() == 1,
        "pt06_heat",
        built,
        |m| {
            (
                m.dof_manager().id().to_owned(),
                m.dof_manager().backend().kind(),
            )
        },
        fails,
    ) else {
        return Ok(());
    };
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .map_err(e2s)?;
    let x = model
        .physics
        .base
        .mesh
        .read()
        .map_err(e2s)?
        .nodes()
        .as_slice()
        .to_vec();
    for n in (0..9).filter(|&n| n != 4) {
        model.blocked_dofs_mut().map_err(e2s)?.as_mut_slice()[n] = true;
        model.temperature_mut().map_err(e2s)?.as_mut_slice()[n] = affine(&x[2 * n..2 * n + 2]);
    }
    // upstream's patch test: Newton, 2 iterations, residual 2e-4
    {
        let nls = model.solver.non_linear_solver_mut(None).map_err(e2s)?;
        if nls.as_newton_raphson_mut().is_some() {
            nls.set_param("max_iterations", ParamValue::Int(2))
                .map_err(e2s)?;
            nls.set_param("threshold", ParamValue::Real(2e-4))
                .map_err(e2s)?;
            nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
                .map_err(e2s)?;
        }
    }
    model.solve_step(None).map_err(e2s)?;
    let t = model.temperature().map_err(e2s)?.as_slice().to_vec();
    let err = (0..9).fold(0.0_f64, |m, n| {
        m.max((t[n] - affine(&x[2 * n..2 * n + 2])).abs())
    });
    println!("INFO pt06 diffusion patch on PETSc: max |T - affine| = {err:e}");
    if err > 1e-10 {
        fails.push(format!(
            "diffusion patch: max |T - affine| = {err:e} > 1e-10"
        ));
    }
    Ok(())
}

/// Upstream's `TestStructBernoulli2.TestDisplacements` (STRUCT, TEST
/// `test_structural_mechanics_model_bernoulli_beam_2.cc`: x = 0, 10, 18,
/// two materials, first node clamped, rollers, the first span's uniform
/// load as nodal equivalents, a moment and an axial force on the last
/// node; static) with the PETSc DOF manager: rotations `5.6/4800`,
/// `−3.7/4800` and the axial displacement `10/18` within upstream's
/// `Math::getTolerance()`.
fn structural_beam(c: &Communicator, fails: &mut Vec<String>) -> Outcome {
    use akantu_structural::{
        StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
    };
    const BEAM: ElementType = ElementType::BernoulliBeam2;
    const NDOF: usize = 3;
    let mut mesh = Mesh::new(2, "pt06_beam").map_err(e2s)?;
    {
        let mut a = akantu_mesh::MeshAccessor::new(&mut mesh);
        for x in [0.0, 10.0, 18.0] {
            a.push_node(&[x, 0.0]).map_err(e2s)?;
        }
        let mut b = a.connectivity_mut(BEAM, NG);
        b.push_tuple(&[0, 1]);
        b.push_tuple(&[1, 2]);
    }
    mesh.compute_bounding_box().map_err(e2s)?;
    let parser = Parser::parse_str(
        "model structural_mechanics_model [\n  dof_manager_type = petsc\n]\n",
        "<pt06 beam>",
    )
    .map_err(e2s)?;
    let built = StructuralMechanicsModel::with_options(
        SharedMesh::new(mesh),
        None,
        "pt06_beam",
        parser,
        StructuralModelUpstreamCompat::default(),
    );
    let Some(mut model) = on_petsc(
        "structural",
        c.size() == 1,
        "pt06_beam",
        built,
        |m| {
            (
                m.dof_manager().id().to_owned(),
                m.dof_manager().backend().kind(),
            )
        },
        fails,
    ) else {
        return Ok(());
    };
    let mut mat = StructuralMaterial {
        e: 3e10,
        i: 0.0025,
        a: 0.01,
        ..StructuralMaterial::default()
    };
    model.add_material(mat, "").map_err(e2s)?;
    mat.i = 0.00128;
    model.add_material(mat, "").map_err(e2s)?;
    model.init_full_with(AnalysisMethod::Static).map_err(e2s)?;
    {
        let mut m = model.element_material_mut(BEAM, NG).map_err(e2s)?;
        m.as_mut_slice()[0] = 0;
        m.as_mut_slice()[1] = 1;
    }
    {
        let mut b_g = model.blocked_dofs_mut().map_err(e2s)?;
        let b = b_g.as_mut_slice();
        b[..NDOF].copy_from_slice(&[true, true, true]);
        b[NDOF..2 * NDOF].copy_from_slice(&[false, true, false]);
        b[2 * NDOF..3 * NDOF].copy_from_slice(&[false, true, false]);
    }
    let (moment, q, l) = (3600.0, 6000.0, 10.0);
    {
        let mut f_g = model.external_force_mut().map_err(e2s)?;
        let f = f_g.as_mut_slice();
        f[2 * NDOF + 2] = -moment;
        f[1] = -q * l / 2.;
        f[2] = -q * l * l / 12.;
        f[NDOF + 1] = -q * l / 2.;
        f[NDOF + 2] = q * l * l / 12.;
        f[2 * NDOF] = mat.e * mat.a / 18.;
    }
    model.solve_step(None).map_err(e2s)?;
    let u = model.displacement().map_err(e2s)?.as_slice().to_vec();
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let got = [u[NDOF + 2], u[2 * NDOF + 2], u[NDOF]];
    let want = [5.6 / 4800., -3.7 / 4800., 10. / 18.];
    println!("INFO pt06 structural beam on PETSc: {got:?} (want {want:?})");
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        if (g - w).abs() > tol {
            fails.push(format!("structural beam: value {k} = {g:e}, want {w:e}"));
        }
    }
    Ok(())
}

/// The cohesive model accepts `dof_manager_type = petsc` through the solid
/// model's section lookup (type `solid_mechanics_model_cohesive`), and its
/// `set_symmetry_policy` (UD-034's compat switch of the AIJ back-end) is
/// refused on that manager, which is kept.
fn cohesive_construction(c: &Communicator, fails: &mut Vec<String>) -> Outcome {
    let mut mesh = Mesh::new(2, "pt06_two_quads").map_err(e2s)?;
    {
        let mut a = akantu_mesh::MeshAccessor::new(&mut mesh);
        for p in [[0., 0.], [1., 0.], [1., 1.], [0., 1.], [0., 2.], [1., 2.]] {
            a.push_node(&p).map_err(e2s)?;
        }
        let mut q = a.connectivity_mut(Q4, NG);
        q.push_tuple(&[0, 1, 2, 3]);
        q.push_tuple(&[3, 2, 5, 4]);
    }
    mesh.compute_bounding_box().map_err(e2s)?;
    let parser = Parser::parse_str(
        "model solid_mechanics_model_cohesive [\n  dof_manager_type = petsc\n]\n",
        "<pt06 cohesive>",
    )
    .map_err(e2s)?;
    let built = akantu_cohesive::SolidMechanicsModelCohesive::with_parser(
        SharedMesh::new(mesh),
        None,
        "pt06_cohesive",
        parser,
    );
    let manager = |m: &akantu_cohesive::SolidMechanicsModelCohesive| {
        let dm = &m.smm.solver.dof_manager;
        (dm.id().to_owned(), dm.backend().kind())
    };
    let Some(mut model) = on_petsc(
        "cohesive",
        c.size() == 1,
        "pt06_cohesive",
        built,
        manager,
        fails,
    ) else {
        return Ok(());
    };
    match model.set_symmetry_policy(akantu_linalg::SymmetryPolicy::UpstreamUpperSelection) {
        Err(Error::InvalidArgument(m)) if m.contains("AIJ back-end") && m.contains("Petsc") => {}
        other => fails.push(format!("cohesive set_symmetry_policy on PETSc: {other:?}")),
    }
    if manager(&model).1 != BackendKind::Petsc {
        fails.push("cohesive: the PETSc manager was replaced".into());
    }
    Ok(())
}

/// Diffusion, structural and cohesive models on the
/// PETSc DOF manager (`dof_manager_type = petsc` in the model section,
/// accepted through feature unification): at one process the diffusion
/// static patch test and upstream's 2-D Bernoulli beam test on PETSc with
/// upstream's criteria, the cohesive construction and its
/// `set_symmetry_policy` refusal; at several processes the serial-mesh
/// refusal (D-092) of each.
fn other_models_on_petsc(c: &Communicator) -> Outcome {
    let mut fails = Vec::new();
    diffusion_patch(c, &mut fails)?;
    structural_beam(c, &mut fails)?;
    cohesive_construction(c, &mut fails)?;
    if c.rank() == 0 {
        println!(
            "INFO other_models_on_petsc: ranks={} ({})",
            c.size(),
            if c.size() == 1 {
                "solved on PETSc"
            } else {
                "serial meshes refused"
            }
        );
    }
    agree(c, fails)
}

// ------------------------------------------------------------

/// The bounds refusal text (`NativeNonLinear::invalidate_bounds`).
const STALE_BOUNDS: &str = "node removal renumbered the local equations";

/// TEST-095's phase-field model on `test_optimal_damage.msh`, the PETSc
/// DOF manager and `petsc_tao` (GPCG, `gatol` 1e-9), on `shared` under `id`.
fn optimal_linear_on(shared: &SharedMesh, id: &str) -> Result<PhaseFieldModel, String> {
    let parser =
        Parser::parse_str(&read_text("material_optimal_linear.dat")?, "<input>").map_err(e2s)?;
    let mut phase = PhaseFieldModel::with_parser(shared.clone(), None, id, parser).map_err(e2s)?;
    phase.init_dof_manager("petsc").map_err(e2s)?;
    phase.init_full(&tao_options()).map_err(e2s)?;
    set(&mut phase, "tao_type", ParamValue::String("gpcg".into()))?;
    set(&mut phase, "tao_gatol", ParamValue::Real(1e-9))?;
    phase
        .apply_dirichlet_bc(
            &FixedValue {
                value: 1.,
                axis: SpatialDirection::X,
            },
            "blocked",
        )
        .map_err(e2s)?;
    Ok(phase)
}

/// The PETSc-tier test (D-095):
/// the variable bounds of a `petsc_tao` solver name local equations, so a
/// node removal invalidates them. On TEST-095's phase-field model with one
/// orphan node: bounds set, the orphan removed (`purify`, which also
/// renumbers the kept nodes by first appearance), the bounded solve refused
/// (recoverable, naming the cause, the model unchanged); the bounds set
/// again, the solve converges and its damage equals, bit for bit, a fresh
/// model's on the final mesh with the same bounds. One process (the
/// serial-mesh rule refuses the PETSc manager of a serial mesh on more).
fn tao_bounds_after_a_node_removal(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        if c.rank() == 0 {
            println!("INFO tao_bounds_after_a_node_removal: one-process scenario");
        }
        return agree(c, Vec::new());
    }
    let mut fails = Vec::new();
    let mut mesh = read_mesh(&pf_data("test_optimal_damage.msh"), 2);
    let nb_nodes = mesh.nb_nodes();
    akantu_mesh::MeshAccessor::new(&mut mesh)
        .push_node(&[1e3, 1e3])
        .map_err(e2s)?;
    mesh.compute_bounding_box().map_err(e2s)?;
    let shared = SharedMesh::new(mesh);
    let mut phase = optimal_linear_on(&shared, "phase_field_model")?;
    phase_bounds(&mut phase, false)?;
    let before = phase.solver.dof_manager.local_system_size();
    phase
        .mutate_mesh(|tx| tx.purify(akantu_mesh::MeshPart::Main), None)
        .map_err(e2s)?;
    let after = phase.solver.dof_manager.local_system_size();
    if shared.read().map_err(e2s)?.nb_nodes() != nb_nodes || after + 1 != before {
        fails.push(format!(
            "purify: {} nodes, system {before} -> {after}",
            shared.read().map_err(e2s)?.nb_nodes()
        ));
    }
    let d0 = phase.damage().map_err(e2s)?.as_slice().to_vec();
    match phase.solve_step(Some("static")) {
        Err(e) if e.fatal_kind().is_none() && e.to_string().contains(STALE_BOUNDS) => {}
        other => fails.push(format!("bounded solve after the removal: {other:?}")),
    }
    if phase.damage().map_err(e2s)?.as_slice() != &d0[..] {
        fails.push("the refused solve changed the damage".into());
    }
    phase_bounds(&mut phase, false)?;
    phase
        .solve_step(Some("static"))
        .and_then(|()| phase.save_previous_state())
        .map_err(e2s)?;
    let d = phase.damage().map_err(e2s)?.as_slice().to_vec();
    // a fresh model on the final mesh, the same bounds
    let mut fresh = optimal_linear_on(&shared, "phase_field_model")?;
    phase_bounds(&mut fresh, false)?;
    fresh
        .solve_step(Some("static"))
        .and_then(|()| fresh.save_previous_state())
        .map_err(e2s)?;
    let want = fresh.damage().map_err(e2s)?.as_slice().to_vec();
    let differ = d
        .iter()
        .zip(&want)
        .filter(|(a, b)| a.to_bits() != b.to_bits())
        .count();
    let max = d
        .iter()
        .zip(&want)
        .fold(0.0_f64, |m, (a, b)| m.max((a - b).abs()));
    println!(
        "INFO tao_bounds_after_a_node_removal: system {before} -> {after}; damage vs a fresh \
         model: {differ} of {} values differ (max {max:e})",
        d.len()
    );
    if d.len() != want.len() || differ != 0 {
        fails.push(format!(
            "damage after the removal and new bounds vs a fresh model: {differ} of {} values \
             differ (max {max:e}), lengths {} / {}",
            d.len(),
            d.len(),
            want.len()
        ));
    }
    agree(c, fails)
}

/// The DOF manager's UD-422 compat switch (`petsc_row_oriented_blocks`:
/// every elemental block inserted transposed, as upstream's
/// `SparseMatrixPETSc::addValues` does) for the oracle replays:
/// the phase-field blocks are symmetric only up to round-off, so
/// without it the port's PETSc `J` is the transpose of upstream's in each
/// bitwise-asymmetric pair and GPCG's noise-level line search turns that
/// into 1-ulp result differences. Set on the new manager, after
/// `init_dof_manager("petsc")`.
fn ud422_compat(phase: &mut PhaseFieldModel) {
    let mut compat = phase.solver.dof_manager.upstream_compat();
    compat.petsc_row_oriented_blocks = true;
    phase.solver.dof_manager.set_upstream_compat(compat);
}

// ------------------------------------------------------------ TEST-095

/// `computeOptimalDamage(x, l)` of upstream's test.
fn optimal_damage(x: Real, l: Real) -> Real {
    let inside = x > -2. * l && x < 2. * l;
    if inside {
        (1. - x.abs() / (2. * l)).powi(2)
    } else {
        0.
    }
}

/// TEST-095 setup: the model after upstream's single static step
/// (`upstream`: every phase-field upstream-compatibility switch on, as the
/// oracle replays need).
fn optimal_linear_model(upstream: bool) -> Result<(PhaseFieldModel, Outcome), String> {
    let mut phase = phase_model(
        "test_optimal_damage.msh",
        &read_text("material_optimal_linear.dat")?,
    )?;
    if upstream {
        phase.set_upstream_compat(PhaseFieldUpstreamCompat::upstream());
    }
    phase.init_dof_manager("petsc").map_err(e2s)?;
    if upstream {
        ud422_compat(&mut phase);
    }
    phase.init_full(&tao_options()).map_err(e2s)?;
    set(&mut phase, "tao_type", ParamValue::String("gpcg".into()))?;
    set(&mut phase, "tao_gatol", ParamValue::Real(1e-9))?;
    set(
        &mut phase,
        "tao_monitor",
        ParamValue::String("stdout".into()),
    )?;
    phase_bounds(&mut phase, false)?;
    phase
        .apply_dirichlet_bc(
            &FixedValue {
                value: 1.,
                axis: SpatialDirection::X,
            },
            "blocked",
        )
        .map_err(e2s)?;
    let r = phase
        .solve_step(Some("static"))
        .and_then(|()| phase.save_previous_state())
        .map_err(e2s);
    Ok((phase, r))
}

/// TEST-095: `test_phase_field_optimal_linear.cc` with upstream's check
/// (`|d − (1 − |x|/(2 l0))²| ≤ 1e-5` at every node).
fn test_phase_field_optimal_linear(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        // A serial upstream test: the serial mesh is refused on several
        // processes (init_dof_manager_rules).
        return Ok(());
    }
    let (phase, solved) = optimal_linear_model(false)?;
    solved?;
    let l0 = phase
        .phase_field(0)
        .map_err(e2s)?
        .get_param("l0")
        .map_err(e2s)?
        .as_real()
        .map_err(e2s)?;
    let mesh = phase.mesh_handle().read().map_err(e2s)?;
    let nodes = mesh.nodes().as_slice().to_vec();
    let damage = phase.damage().map_err(e2s)?.as_slice().to_vec();
    let mut worst: Real = 0.0;
    let mut fails = Vec::new();
    for (n, d) in damage.iter().enumerate() {
        let x = nodes[2 * n];
        let e = (optimal_damage(x, l0) - d).abs();
        worst = worst.max(e);
        if e > 1e-5 {
            fails.push(format!(
                "node {n} (x = {x}): {d} vs {}",
                optimal_damage(x, l0)
            ));
        }
    }
    println!(
        "INFO test_phase_field_optimal_linear: {} nodes, max |d - optimal| {worst:e} (upstream \
         tolerance 1e-5), min d {:e}, max d {:e}",
        damage.len(),
        damage.iter().copied().fold(Real::INFINITY, Real::min),
        damage.iter().copied().fold(Real::NEG_INFINITY, Real::max)
    );
    fails.truncate(5);
    agree(c, fails)
}

/// D-082's distributed-vs-serial class (`par_phase_field::ENVELOPE`,
/// `par_laws::VS_SERIAL`): a run on a mesh distributed over `N` processes
/// against the same run on one process, per global node, relative to the
/// one-process array's maximum — a regression guard with its measured margin
/// (`docs/NUMERICAL_VALIDATION.md`), not an error bound.
const VS_ONE_RANK: Real = 1e-14;

/// One distributed TEST-095 run (see
/// [`test_phase_field_optimal_linear_distributed`]).
struct DistributedRun {
    /// Damage by global node id (the owners' values), identical on every
    /// rank of the communicator.
    damage: Vec<Real>,
    /// [nodes beyond upstream's 1e-5, nodes outside [0, 1], global ids out
    /// of range, global ids without exactly one owner], summed over the
    /// ranks.
    counts: [u64; 4],
    /// max |d − (1 − |x|/(2 l0))²| over the ranks.
    worst_optimal: Real,
    /// TAO iterations and final gradient norm (rank 0's report; the solve
    /// is collective).
    iterations: usize,
    gnorm: Real,
    /// Set-up deviations (manager kind / communicator).
    fails: Vec<String>,
}

/// TEST-095 on `test_optimal_damage.msh` read on rank 0 of `c` and
/// distributed over `c` with the bisection partitioner; the PETSc DOF
/// manager on the mesh's communicator, the replay's switches (every
/// phase-field switch but UD-004, which concerns ghost elements only, plus
/// UD-422), GPCG with `tao_gatol 1e-9` (upstream's settings). With
/// `pc_none`, the TAO solver's private option `pc_type none` (the port's
/// option API, upstream `getNonLinearSolver().set("pc_type", "none")`):
/// `TaoSetFromOptions` reads the private database, and GPCG's
/// `TaoSetFromOptions_GPCG` calls `KSPSetFromOptions` on its inner NASH KSP
/// (no options prefix), so the inner solves run without a preconditioner,
/// whose result does not depend on the partition — PETSc's default is
/// ILU(0) on one process and block Jacobi on several.
fn distributed_optimal_linear_run(
    c: &Communicator,
    n_ref: usize,
    pc_none: bool,
) -> Result<DistributedRun, String> {
    let mut m = Mesh::with_communicator(2, "mesh", c.clone()).map_err(e2s)?;
    if c.rank() == 0 {
        akantu_io::read_mesh(
            &mut m,
            &pf_data("test_optimal_damage.msh"),
            MeshIoType::Gmsh,
        )
        .map_err(e2s)?;
    }
    m.compute_bounding_box().map_err(e2s)?;
    let mut part = RcbPartitioner;
    let _ = distribute_with(&mut m, Some(&mut part as &mut dyn MeshPartitioner), 0).map_err(e2s)?;
    let parser =
        Parser::parse_str(&read_text("material_optimal_linear.dat")?, "<input>").map_err(e2s)?;
    let mut phase =
        PhaseFieldModel::with_parser(SharedMesh::new(m), None, "phase_field_model", parser)
            .map_err(e2s)?;
    let mut compat = PhaseFieldUpstreamCompat::upstream();
    compat.ghost_gradient_into_not_ghost = false;
    phase.set_upstream_compat(compat);
    phase.init_dof_manager("petsc").map_err(e2s)?;
    let mut fails = Vec::new();
    {
        let dm = &phase.solver.dof_manager;
        if dm.backend().kind() != BackendKind::Petsc
            || !dm.communicator().is_mpi()
            || dm.communicator().size() != c.size()
        {
            fails.push(format!(
                "distributed mesh: manager kind {:?}, MPI {}, {} ranks (want PETSc on the mesh's \
                 {} ranks)",
                dm.backend().kind(),
                dm.communicator().is_mpi(),
                dm.communicator().size(),
                c.size()
            ));
        }
    }
    ud422_compat(&mut phase);
    phase.init_full(&tao_options()).map_err(e2s)?;
    set(&mut phase, "tao_type", ParamValue::String("gpcg".into()))?;
    set(&mut phase, "tao_gatol", ParamValue::Real(1e-9))?;
    if pc_none {
        set(&mut phase, "pc_type", ParamValue::String("none".into()))?;
    }
    phase_bounds(&mut phase, false)?;
    phase
        .apply_dirichlet_bc(
            &FixedValue {
                value: 1.,
                axis: SpatialDirection::X,
            },
            "blocked",
        )
        .map_err(e2s)?;
    // Converged to `tao_gatol` or an error (`Error::NotConverged`).
    phase.solve_step(Some("static")).map_err(e2s)?;
    let (iterations, gnorm) = {
        let s = phase.solver.non_linear_solver_mut(None).map_err(e2s)?;
        let it = s.nb_iterations();
        let g = s
            .as_native_mut()
            .and_then(|n| n.report().map(|r| r.relative_norm))
            .unwrap_or(Real::NAN);
        (it, g)
    };
    let l0 = phase
        .phase_field(0)
        .map_err(e2s)?
        .get_param("l0")
        .map_err(e2s)?
        .as_real()
        .map_err(e2s)?;
    let damage = phase.damage().map_err(e2s)?.as_slice().to_vec();
    let mesh = phase.mesh_handle().read().map_err(e2s)?;
    let nodes = mesh.nodes().as_slice().to_vec();
    // The damage by global id (owners' values; zeros added elsewhere) and
    // the number of owners of every global id.
    let mut full = vec![0.0_f64; n_ref];
    let mut owners = vec![0.0_f64; n_ref];
    let mut counts = [0_u64; 4];
    let mut worst_optimal = [0.0_f64];
    for (n, &d) in damage.iter().enumerate() {
        let idx = akantu_core::Idx::try_from(n).map_err(|e| e.to_string())?;
        if !mesh.node_flag(idx).is_local_or_master() {
            continue;
        }
        let gid = usize::try_from(mesh.node_global_id(idx)).unwrap_or(usize::MAX);
        if gid >= full.len() {
            counts[2] += 1;
            continue;
        }
        full[gid] = d;
        owners[gid] += 1.0;
        let e = (optimal_damage(nodes[2 * n], l0) - d).abs();
        worst_optimal[0] = worst_optimal[0].max(e);
        counts[0] += u64::from(e.is_nan() || e > 1e-5);
        counts[1] += u64::from(!(0. ..=1.).contains(&d));
    }
    drop(mesh);
    c.all_reduce(&mut full, ReduceOp::Sum).map_err(e2s)?;
    c.all_reduce(&mut owners, ReduceOp::Sum).map_err(e2s)?;
    counts[3] = u64::try_from(owners.iter().filter(|&&k| k != 1.0).count()).unwrap_or(u64::MAX);
    let mut summed = [counts[0], counts[1], counts[2]];
    c.all_reduce(&mut summed, ReduceOp::Sum).map_err(e2s)?;
    counts[..3].copy_from_slice(&summed);
    c.all_reduce(&mut worst_optimal, ReduceOp::Max)
        .map_err(e2s)?;
    Ok(DistributedRun {
        damage: full,
        counts,
        worst_optimal: worst_optimal[0],
        iterations,
        gnorm,
        fails,
    })
}

fn damage_digest(d: &[Real]) -> u64 {
    d.iter().fold(0xcbf2_9ce4_8422_2325_u64, |h, x| {
        akantu_comm::phase::digest_bytes(h, &x.to_bits().to_le_bytes())
    })
}

/// TEST-095 on a **distributed** mesh
/// with the PETSc DOF manager — `init_dof_manager("petsc")` keeps the
/// mesh's MPI communicator, so the bounded TAO solve runs on the
/// distributed PETSc system. Upstream's own program is serial and the
/// oracle probe has no distributed case. Three comparisons:
///
/// 1. **PETSc's default preconditioner** (what a user gets): upstream's own
///    criterion of TEST-095, `|d − (1 − |x|/(2 l0))²| ≤ 1e-5` at every
///    node, and `0 ≤ d ≤ 1`, at 1 / 2 / 4 ranks; at one rank (the same
///    distributed code path on an MPI communicator of one rank) bit for bit
///    upstream's one-process result (oracle record `optimal_linear`). At
///    2 / 4 ranks the difference from that record is printed, not bounded:
///    PETSc's default preconditioner of GPCG's inner KSP depends on the
///    number of processes (ILU(0) on one, block Jacobi with ILU(0) blocks
///    on several), so the truncated inner solves differ and so does the
///    converged minimiser, within the solver's tolerances — which is what
///    upstream's criterion judges.
/// 2. **Partition-independent preconditioner** (`pc_type none`): the run
///    on `N` ranks against the same run on a one-process communicator
///    (rank 0 of `c`, split off) per global node within D-082's
///    distributed-vs-serial class [`VS_ONE_RANK`] (1e-14 relative to the
///    maximum; bit for bit at one rank), plus upstream's criterion and the
///    bounds.
/// 3. The option is applied: the `pc_type none` result differs from the
///    default-preconditioner result on the same partition.
fn test_phase_field_optimal_linear_distributed(c: &Communicator) -> Outcome {
    let rec = oracle_record("optimal_linear")?.ok_or("oracle record optimal_linear missing")?;
    let want = bits(&rec["result"]["damage"])?;
    let n = want.len();
    // D-121: only check (1)'s one-rank bitwise comparison with the record
    // depends on the environment; the other checks run everywhere.
    let compare_values =
        pftao_comparability(c, "test_phase_field_optimal_linear_distributed").is_none();
    let mut fails = Vec::new();
    let checks = |what: &str, r: &DistributedRun, fails: &mut Vec<String>| {
        fails.extend(r.fails.iter().map(|f| format!("{what}: {f}")));
        if r.counts.iter().any(|&k| k > 0) {
            fails.push(format!(
                "{what}: {} nodes beyond upstream's 1e-5, {} outside [0, 1], {} global ids out \
                 of range, {} global ids without exactly one owner (max |d - optimal| {:e})",
                r.counts[0], r.counts[1], r.counts[2], r.counts[3], r.worst_optimal
            ));
        }
    };
    // (1) PETSc's default preconditioner.
    let def = distributed_optimal_linear_run(c, n, false)?;
    checks("default preconditioner", &def, &mut fails);
    let (differ_def, rel_def) = compare(&def.damage, &want);
    if compare_values && c.size() == 1 && differ_def != 0 {
        fails.push(format!(
            "default preconditioner at 1 rank: {differ_def} damages differ from upstream's \
             one-process result (max rel {rel_def:e}; want bit for bit)"
        ));
    }
    // (2) `pc_type none` on N ranks and on a one-process communicator.
    let none = distributed_optimal_linear_run(c, n, true)?;
    checks("pc_type none", &none, &mut fails);
    let sub = c
        .split(if c.rank() == 0 { Some(0) } else { None }, 0)
        .map_err(e2s)?;
    let mut one = vec![0.0_f64; n];
    let mut one_stats = [0.0_f64; 2];
    if let Some(s) = sub {
        let r = distributed_optimal_linear_run(&s, n, true)?;
        checks("pc_type none, one process", &r, &mut fails);
        one = r.damage;
        one_stats = [
            f64::from(u32::try_from(r.iterations).unwrap_or(u32::MAX)),
            r.gnorm,
        ];
    }
    c.all_reduce(&mut one, ReduceOp::Sum).map_err(e2s)?;
    c.all_reduce(&mut one_stats, ReduceOp::Sum).map_err(e2s)?;
    let (differ_none, rel_none) = compare(&none.damage, &one);
    let (differ_none_rec, rel_none_rec) = compare(&none.damage, &want);
    if rel_none.is_nan() || rel_none > VS_ONE_RANK || (c.size() == 1 && differ_none != 0) {
        fails.push(format!(
            "pc_type none at {} rank(s) vs one process: {differ_none} of {n} damages differ, max \
             rel {rel_none:e} (bound {VS_ONE_RANK:e}, bit for bit at one rank)",
            c.size()
        ));
    }
    // (3) The option reached the inner KSP.
    let (differ_opt, rel_opt) = compare(&none.damage, &def.damage);
    if differ_opt == 0 {
        fails.push(
            "pc_type none gives the default preconditioner's result bit for bit: the option \
             did not reach GPCG's inner KSP"
                .into(),
        );
    }
    if c.rank() == 0 {
        println!(
            "INFO test_phase_field_optimal_linear_distributed: ranks={} default preconditioner: \
             {differ_def} of {n} damages differ from upstream's one-process result, max rel \
             {rel_def:e} (bit for bit required at one rank; printed only at more), digest \
             {:016x}, TAO {} iterations, gnorm {:e}, max |d - optimal| {:e} (upstream 1e-5)",
            c.size(),
            damage_digest(&def.damage),
            def.iterations,
            def.gnorm,
            def.worst_optimal
        );
        println!(
            "INFO test_phase_field_optimal_linear_distributed: ranks={} pc_type none vs the same \
             run on one process: {differ_none} of {n} differ, max rel {rel_none:e} (bound \
             {VS_ONE_RANK:e}, margin x{:.1}); digest {:016x}; TAO {} iterations, gnorm {:e} (one \
             process: {} iterations, gnorm {:e}); max |d - optimal| {:e} (upstream 1e-5); vs \
             upstream's ILU(0) record: {differ_none_rec} differ, max rel {rel_none_rec:e}; vs the \
             default preconditioner on this partition: {differ_opt} differ, max rel {rel_opt:e}",
            c.size(),
            if rel_none > 0.0 {
                VS_ONE_RANK / rel_none
            } else {
                f64::INFINITY
            },
            damage_digest(&none.damage),
            none.iterations,
            none.gnorm,
            one_stats[0],
            one_stats[1],
            none.worst_optimal
        );
    }
    agree(c, fails)
}

// ------------------------------------------------------------ TEST-100

fn pf_param(c: &CouplerSolidPhaseField, name: &str) -> Result<Real, String> {
    c.phase
        .phase_field(0)
        .map_err(e2s)?
        .get_param(name)
        .map_err(e2s)?
        .as_real()
        .map_err(e2s)
}

fn material_array(c: &CouplerSolidPhaseField, i: usize, name: &str) -> Result<Vec<Real>, String> {
    let mut out = None;
    c.solid
        .material(i)
        .map_err(e2s)?
        .for_each_internal(&mut |n, f| {
            if n == name
                && let InternalFieldRef::Real(f) = f
            {
                out = f.get(Q4, NG).ok().map(|a| a.as_slice().to_vec());
            }
        });
    out.ok_or_else(|| format!("material {i} has no internal {name}"))
}

/// `applyDisplacement` of upstream's test: every node blocked, `u = 0` at
/// `y = −0.5`, `u = (0, ε)` elsewhere.
fn apply_displacement(c: &mut CouplerSolidPhaseField, eps: Real) -> Outcome {
    let nodes = c
        .solid
        .physics
        .base
        .mesh
        .read()
        .map_err(e2s)?
        .nodes()
        .as_slice()
        .to_vec();
    let mut u = c.solid.displacement().map_err(e2s)?.as_slice().to_vec();
    let mut b = c.solid.blocked_dofs().map_err(e2s)?.as_slice().to_vec();
    for n in 0..nodes.len() / 2 {
        u[2 * n] = 0.;
        u[2 * n + 1] = if nodes[2 * n + 1] == -0.5 { 0. } else { eps };
        b[2 * n] = true;
        b[2 * n + 1] = true;
    }
    c.solid
        .displacement_mut()
        .map_err(e2s)?
        .as_mut_slice()
        .copy_from_slice(&u);
    c.solid
        .blocked_dofs_mut()
        .map_err(e2s)?
        .as_mut_slice()
        .copy_from_slice(&b);
    Ok(())
}

/// TEST-100: `test_phase_field_tao.cc` through the coupler (the test's
/// local helpers are the coupler's staggered pass, as the port of
/// test_phase_field_linear.cc): `-tao_type gpcg`, `-tao_gatol 1e-9` in
/// PETSc's global database before `initDOFManager("petsc")`, bounds `[0, 1]`
/// then `[0, 1 − d]` after every step, 1000 steps; upstream's checks
/// (damage and stress at the first quadrature point vs the analytical AT1
/// solution, `1e-8`, `1e-5` while the analytical damage is below `1e-8`).
fn test_phase_field_tao(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(());
    }
    let mesh = read_mesh(&pf_data("test_one_element.msh"), 2);
    let parser =
        Parser::parse_str(&read_text("material_linear_tao.dat")?, "<input>").map_err(e2s)?;
    let mut cp = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .map_err(e2s)?;
    cp.solid
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .map_err(e2s)?;
    {
        let nls = cp.solid.solver.non_linear_solver_mut(None).map_err(e2s)?;
        nls.set_param("max_iterations", ParamValue::Int(100))
            .map_err(e2s)?;
        nls.set_param("threshold", ParamValue::Real(1e-9))
            .map_err(e2s)?;
        nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
            .map_err(e2s)?;
    }
    cp.phase
        .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    petsc::set_global_option(c, "tao_type", "gpcg").map_err(e2s)?;
    petsc::set_global_option(c, "tao_gatol", "1e-9").map_err(e2s)?;
    cp.phase.init_dof_manager("petsc").map_err(e2s)?;
    cp.phase.init_full(&tao_options()).map_err(e2s)?;
    phase_bounds(&mut cp.phase, false)?;
    let e = pf_param(&cp, "E")?;
    let nu = pf_param(&cp, "nu")?;
    let c22 = e * (1. - nu) / ((1. + nu) * (1. - 2. * nu));
    let gc = pf_param(&cp, "gc")?;
    let l0 = pf_param(&cp, "l0")?;
    let mut analytical_damage: Real = 0.;
    let (mut worst_d, mut worst_s) = (0.0_f64, 0.0_f64);
    let mut fails = Vec::new();
    for s in 0..1000_i32 {
        let axial_strain = if s < 500 {
            1e-5 * Real::from(s)
        } else {
            Real::from(1000 - s) * 1e-5
        };
        apply_displacement(&mut cp, axial_strain)?;
        cp.solve(Some("static"), None).map_err(e2s)?;
        phase_bounds(&mut cp.phase, true)?;
        let dam = 1. - 3. * gc / (8. * l0 * axial_strain * axial_strain * c22);
        analytical_damage = dam.max(analytical_damage);
        let analytical_sigma =
            c22 * axial_strain * (1. - analytical_damage) * (1. - analytical_damage);
        let stress = cp
            .solid
            .material(0)
            .map_err(e2s)?
            .base()
            .stress
            .get(Q4, NG)
            .map_err(e2s)?
            .as_slice()[3];
        let damage = material_array(&cp, 0, "damage")?[0];
        let error_stress = (analytical_sigma - stress).abs() / analytical_sigma;
        let error_damage = (analytical_damage - damage).abs();
        let tol = if analytical_damage < 1e-8 { 1e-5 } else { 1e-8 };
        if s > 0 {
            worst_d = worst_d.max(error_damage);
            worst_s = worst_s.max(error_stress);
        }
        if (error_damage > tol || error_stress > tol) && s > 0 {
            fails.push(format!(
                "step {s} (strain {axial_strain}): error damage {error_damage:e}, error stress \
                 {error_stress:e}"
            ));
            break;
        }
    }
    println!(
        "INFO test_phase_field_tao: 1000 steps, max error damage {worst_d:e}, max relative \
         error stress {worst_s:e}, final analytical damage {analytical_damage:e}"
    );
    agree(c, fails)
}

// ------------------------------------------------------------ oracle

/// The bit patterns of a record's `{"bits": [...]}`.
fn bits(v: &serde_json::Value) -> Result<Vec<f64>, String> {
    v.get("bits")
        .and_then(serde_json::Value::as_array)
        .ok_or("no bits")?
        .iter()
        .map(|b| {
            let s = b.as_str().ok_or("bits entry")?;
            u64::from_str_radix(s, 16)
                .map(f64::from_bits)
                .map_err(|e| e.to_string())
        })
        .collect()
}

/// `(differing values, max relative difference)`.
fn compare(got: &[f64], want: &[f64]) -> (usize, f64) {
    if got.len() != want.len() {
        return (usize::MAX, f64::INFINITY);
    }
    let scale = want
        .iter()
        .fold(0.0_f64, |m, x| m.max(x.abs()))
        .max(f64::MIN_POSITIVE);
    got.iter().zip(want).fold((0, 0.0), |(n, m), (a, b)| {
        (
            n + usize::from(a.to_bits() != b.to_bits()),
            m.max((a - b).abs() / scale),
        )
    })
}

/// The environment of the TAO oracle records (the pinned x86_64 oracle,
/// D-050); no other platform has records (D-121).
const PFTAO_RECORD_ENV: &str = "x86_64-haswell";

/// D-121: `None` when this process compares values with the TAO records;
/// otherwise the reason, after printing the scenario's report line (rank 0).
fn pftao_comparability(c: &Communicator, scenario: &str) -> Option<String> {
    let not_comparable = oracle_values_not_comparable(PFTAO_RECORD_ENV);
    if c.rank() == 0 {
        println!(
            "{}",
            oracle_report_line(scenario, PFTAO_RECORD_ENV, not_comparable.as_deref())
        );
    }
    not_comparable
}

fn oracle_record(case: &str) -> Result<Option<serde_json::Value>, String> {
    let path = fixtures_root().join(format!(
        "solver/petsc/oracle/pftao/{case}.{PFTAO_RECORD_ENV}.json"
    ));
    if !path.exists() {
        return Ok(None);
    }
    let text = std::fs::read_to_string(&path).map_err(|e| e.to_string())?;
    serde_json::from_str(&text)
        .map(Some)
        .map_err(|e| e.to_string())
}

/// The material text of a law × split variant (the probe's).
fn variant_material(law: &str, split: &str) -> String {
    let rec = if law == "linear" {
        "    recovery_tol = 0.001\n"
    } else {
        ""
    };
    format!(
        "model phase_field_model [\n  phasefield {law} {split} [\n    name = plate\n    E = 210.0\n    \
         nu = 0.3\n    gc = 5e-3\n    l0 = 1.\n    Plane_Stress = false\n    irreversibility_type = \
         tao\n    irreversibility_tol = 0.001\n{rec}  ]\n]\n"
    )
}

/// The probe's TEST-100 phase-field half: per step `(strain, nodal damage,
/// dissipated energy)`.
type Trace = (Vec<Real>, Vec<Real>, Vec<Real>);

fn tao_variant_run(c: &Communicator, law: &str, split: &str) -> Result<Trace, String> {
    let text = if law == "linear" && split == "no_split" {
        read_text("material_linear_tao.dat")?
    } else {
        variant_material(law, split)
    };
    let mut phase = phase_model("test_one_element.msh", &text)?;
    phase.set_upstream_compat(PhaseFieldUpstreamCompat::upstream());
    phase.set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    petsc::set_global_option(c, "tao_type", "gpcg").map_err(e2s)?;
    petsc::set_global_option(c, "tao_gatol", "1e-9").map_err(e2s)?;
    phase.init_dof_manager("petsc").map_err(e2s)?;
    ud422_compat(&mut phase);
    phase.init_full(&tao_options()).map_err(e2s)?;
    phase_bounds(&mut phase, false)?;
    let (mut strains, mut damages, mut energies) = (Vec::new(), Vec::new(), Vec::new());
    for s in 0..1000_i32 {
        let eps = if s < 500 {
            1e-5 * Real::from(s)
        } else {
            Real::from(1000 - s) * 1e-5
        };
        {
            let law = phase.phase_field_mut(0).map_err(e2s)?;
            let mut st = law.base_mut().strain.get_mut(Q4, NG).map_err(e2s)?;
            for t in st.as_mut_slice().as_chunks_mut::<4>().0 {
                *t = [0., 0., 0., eps];
            }
        }
        phase
            .solve_step(None)
            .map_err(|e| format!("step {s}: {e}"))?;
        phase_bounds(&mut phase, true)?;
        strains.push(eps);
        damages.extend_from_slice(phase.damage().map_err(e2s)?.as_slice());
        energies.push(phase.energy("dissipated").map_err(e2s)?);
    }
    Ok((strains, damages, energies))
}

/// Oracle replays (bitwise) and the bounds / irreversibility of every
/// variant.
fn phase_field_tao_oracle(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(());
    }
    // D-121: outside the records' environment the runs, their bounds and
    // irreversibility are checked and the differences printed, not asserted.
    let compare_values = pftao_comparability(c, "phase_field_tao_oracle").is_none();
    let mut fails = Vec::new();
    let mut missing = Vec::new();
    // TEST-095.
    match oracle_record("optimal_linear")? {
        None => missing.push("optimal_linear"),
        Some(rec) => {
            let (phase, solved) = optimal_linear_model(true)?;
            solved?;
            let got = phase.damage().map_err(e2s)?.as_slice().to_vec();
            let want = bits(&rec["result"]["damage"])?;
            let (n, rel) = compare(&got, &want);
            println!(
                "INFO oracle optimal_linear: {} nodal damages, {n} differing bits, max rel {rel:e}",
                want.len()
            );
            // Bitwise (D-088 (2)).
            if compare_values && n != 0 {
                fails.push(format!(
                    "optimal_linear: {n} damages differ from the oracle (max rel {rel:e})"
                ));
            }
        }
    }
    for law in ["linear", "quadratic"] {
        for split in ["no_split", "volumetric_deviatoric"] {
            let case = format!("tao_{law}_{split}");
            let (strains, damages, energies) = tao_variant_run(c, law, split)?;
            // Independent checks: 0 ≤ d ≤ 1, irreversible (non-decreasing
            // at every node, also while unloading).
            let nn = damages.len() / strains.len().max(1);
            let mut bad = 0;
            for k in 0..strains.len() {
                for i in 0..nn {
                    let d = damages[k * nn + i];
                    let prev = if k == 0 {
                        0.
                    } else {
                        damages[(k - 1) * nn + i]
                    };
                    if !(0. ..=1.).contains(&d) || d < prev {
                        bad += 1;
                    }
                }
            }
            if bad > 0 {
                fails.push(format!("{case}: {bad} bound / irreversibility violations"));
            }
            match oracle_record(&case)? {
                None => missing.push("tao variant"),
                Some(rec) => {
                    let r = &rec["result"];
                    let want_d = bits(&r["damage"])?;
                    let first = damages
                        .iter()
                        .zip(&want_d)
                        .position(|(a, b)| a.to_bits() != b.to_bits())
                        .map(|k| (k / nn.max(1), damages[k], want_d[k]));
                    println!(
                        "INFO oracle {case}: first differing damage (step, port, oracle) {first:?}"
                    );
                    let (nd, rd) = compare(&damages, &want_d);
                    let (ne, re) = compare(&energies, &bits(&r["energy"])?);
                    let (ns, _) = compare(&strains, &bits(&r["strain_yy"])?);
                    println!(
                        "INFO oracle {case}: {} steps; damage {nd} differing bits (max rel {rd:e}), \
                         energy {ne} (max rel {re:e}); final damage {:?}",
                        strains.len(),
                        &damages[damages.len().saturating_sub(nn)..]
                    );
                    // Bitwise: damage, energies and the loading history
                    // (D-088 (2)).
                    if compare_values && (ns != 0 || nd != 0 || ne != 0) {
                        fails.push(format!(
                            "{case}: damage max rel {rd:e} ({nd} differ), energy {re:e} ({ne}), \
                             strain {ns} values differ"
                        ));
                    }
                }
            }
        }
    }
    if !missing.is_empty() {
        fails.push(format!("oracle records missing: {missing:?}"));
    }
    agree(c, fails)
}

// ------------------------------------------------------------

/// The law × split cells of TEST-100 through the coupler (records
/// `cpl_tao_<law>_<split>`).
const CPL_TAO_CELLS: [(&str, &str); 4] = [
    ("linear", "no_split"),
    ("linear", "volumetric_deviatoric"),
    ("quadratic", "no_split"),
    ("quadratic", "volumetric_deviatoric"),
];

/// The switches of a coupled TAO run.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum CplMode {
    /// Every upstream switch (coupler, phase field, solid laws) and the DOF
    /// manager's UD-422 switch.
    Upstream,
    /// The corrected defaults plus only what the cell needs: UD-422 and, for
    /// the volumetric–deviatoric split, UD-387 (`in_block_deviator_energy`).
    Needed,
    /// Every default corrected, UD-422 off.
    Defaults,
    /// Leave-one-out of a two-switch cell: `Needed`
    /// without UD-422 (UD-387 kept for the volumetric–deviatoric split).
    NeededWithoutUd422,
    /// Leave-one-out: `Needed` without UD-387 (UD-422 kept).
    NeededWithoutUd387,
}

/// What a coupled TAO run records per step (the probe's `cpl_tao_*`
/// record) and after the last step.
#[derive(Default)]
struct CplTrace {
    strain: Vec<Real>,
    damage: Vec<Real>,
    energy: Vec<Real>,
    potential: Vec<Real>,
    solid_iterations: Vec<i64>,
    solid_error: Vec<Real>,
    stress_yy_q0: Vec<Real>,
    material_damage_q0: Vec<Real>,
    /// Material stress, material damage, law strain, law phi.
    digests: [Vec<String>; 4],
    finals: Vec<(&'static str, Vec<Real>)>,
}

const CPL_DIGESTS: [&str; 4] = [
    "material_stress",
    "material_damage",
    "law_strain",
    "law_phi",
];

fn law_array(c: &CouplerSolidPhaseField, l: usize, name: &str) -> Result<Vec<Real>, String> {
    let mut out = None;
    c.phase
        .phase_field(l)
        .map_err(e2s)?
        .for_each_internal(&mut |n, f| {
            if n == name
                && let InternalFieldRef::Real(f) = f
            {
                out = f.get(Q4, NG).ok().map(|a| a.as_slice().to_vec());
            }
        });
    out.ok_or_else(|| format!("law {l} has no internal {name}"))
}

/// The probe's material file of a cell (upstream's material_linear_tao.dat
/// for AT1 no split; otherwise the solid half of that file followed by
/// [`variant_material`]).
fn coupled_material(law: &str, split: &str) -> Result<String, String> {
    if law == "linear" && split == "no_split" {
        return read_text("material_linear_tao.dat");
    }
    Ok(format!(
        "model solid_mechanics_model [\n  material phasefield {split} [\n    name = plate\n    rho \
         = 1.\n    E = 210.0\n    nu = 0.3\n    Plane_Stress = false\n  ]\n]\n\n{}",
        variant_material(law, split)
    ))
}

fn eigen_static() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// TEST-100 through the coupler as the probe's `cpl_tao_*` case: the solid
/// static on the Eigen sparse solver (the probe pins it, D-035) with the
/// test's Newton (100 iterations, 1e-9, residual), the phase field on the
/// PETSc DOF manager with TAO (`-tao_type gpcg`, `-tao_gatol 1e-9` in the
/// global database before `init_dof_manager("petsc")`), the test's
/// `applyDisplacement`, 1000 steps of `coupler.solve("static", default)`
/// and the bounds `[0, 1 − d]` after every step.
fn coupled_tao_run(
    c: &Communicator,
    split: &str,
    mode: CplMode,
    material: &str,
) -> Result<CplTrace, String> {
    let mesh = read_mesh(&pf_data("test_one_element.msh"), 2);
    let parser = Parser::parse_str(material, "<input>").map_err(e2s)?;
    let mut cp = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .map_err(e2s)?;
    match mode {
        CplMode::Upstream => {
            cp.set_upstream_compat(true);
            cp.solid.physics.law_options = Some(LawOptions::upstream());
        }
        CplMode::Needed | CplMode::NeededWithoutUd422 => {
            cp.phase.set_upstream_compat(PhaseFieldUpstreamCompat {
                in_block_deviator_energy: split == "volumetric_deviatoric",
                ..PhaseFieldUpstreamCompat::default()
            });
        }
        CplMode::Defaults | CplMode::NeededWithoutUd387 => {}
    }
    cp.solid.init_full(&eigen_static()).map_err(e2s)?;
    {
        let nls = cp
            .solid
            .solver
            .non_linear_solver_mut(Some("static"))
            .map_err(e2s)?;
        nls.set_param("max_iterations", ParamValue::Int(100))
            .map_err(e2s)?;
        nls.set_param("threshold", ParamValue::Real(1e-9))
            .map_err(e2s)?;
        nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
            .map_err(e2s)?;
    }
    cp.phase
        .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    petsc::set_global_option(c, "tao_type", "gpcg").map_err(e2s)?;
    petsc::set_global_option(c, "tao_gatol", "1e-9").map_err(e2s)?;
    cp.phase.init_dof_manager("petsc").map_err(e2s)?;
    if matches!(
        mode,
        CplMode::Upstream | CplMode::Needed | CplMode::NeededWithoutUd387
    ) {
        ud422_compat(&mut cp.phase);
    }
    cp.phase.init_full(&tao_options()).map_err(e2s)?;
    phase_bounds(&mut cp.phase, false)?;
    let mut t = CplTrace::default();
    for s in 0..1000_i32 {
        let eps = if s < 500 {
            1e-5 * Real::from(s)
        } else {
            Real::from(1000 - s) * 1e-5
        };
        apply_displacement(&mut cp, eps)?;
        cp.solve(Some("static"), None)
            .map_err(|e| format!("step {s}: {e}"))?;
        phase_bounds(&mut cp.phase, true)?;
        t.strain.push(eps);
        t.damage
            .extend_from_slice(cp.phase.damage().map_err(e2s)?.as_slice());
        t.energy.push(cp.phase.energy("dissipated").map_err(e2s)?);
        t.potential.push(cp.solid.energy("potential").map_err(e2s)?);
        let nls = cp
            .solid
            .solver
            .non_linear_solver(Some("static"))
            .map_err(e2s)?;
        match (
            nls.get_param("nb_iterations").map_err(e2s)?,
            nls.get_param("error").map_err(e2s)?,
        ) {
            (ParamValue::Int(i), ParamValue::Real(e)) => {
                t.solid_iterations.push(i);
                t.solid_error.push(e);
            }
            other => return Err(format!("solid Newton state: {other:?}")),
        }
        let stress = material_array(&cp, 0, "stress")?;
        let mdam = material_array(&cp, 0, "damage")?;
        t.stress_yy_q0.push(stress[3]);
        t.material_damage_q0.push(mdam[0]);
        t.digests[0].push(sha_hex(&stress));
        t.digests[1].push(sha_hex(&mdam));
        t.digests[2].push(sha_hex(&law_array(&cp, 0, "strain")?));
        t.digests[3].push(sha_hex(&law_array(&cp, 0, "phi")?));
    }
    t.finals = vec![
        (
            "displacement",
            cp.solid.displacement().map_err(e2s)?.as_slice().to_vec(),
        ),
        (
            "internal_force",
            cp.solid.internal_force().map_err(e2s)?.as_slice().to_vec(),
        ),
        (
            "damage",
            cp.phase.damage().map_err(e2s)?.as_slice().to_vec(),
        ),
        ("material_stress", material_array(&cp, 0, "stress")?),
        ("material_grad_u", material_array(&cp, 0, "grad_u")?),
        ("material_damage", material_array(&cp, 0, "damage")?),
        ("law_damage", law_array(&cp, 0, "damage")?),
        ("law_strain", law_array(&cp, 0, "strain")?),
        ("law_phi", law_array(&cp, 0, "phi")?),
    ];
    Ok(t)
}

/// The record's integers.
fn ints(v: &serde_json::Value) -> Result<Vec<i64>, String> {
    v.as_array()
        .ok_or("not an array")?
        .iter()
        .map(|x| x.as_i64().ok_or_else(|| "not an integer".to_owned()))
        .collect()
}

fn strs(v: &serde_json::Value) -> Result<Vec<String>, String> {
    v.as_array()
        .ok_or("not an array")?
        .iter()
        .map(|x| {
            x.as_str()
                .map(str::to_owned)
                .ok_or_else(|| "not a string".to_owned())
        })
        .collect()
}

/// `(quantity, differing values, max relative difference)` of the
/// quantities of a coupled run that differ from the record (empty: bit for
/// bit).
type CplDiff = Vec<(String, usize, f64)>;

/// Every quantity of a coupled run against the record ([`CplDiff`]), and
/// the first step at which the nodal damage differs.
fn compare_cpl(t: &CplTrace, r: &serde_json::Value) -> Result<(CplDiff, Option<usize>), String> {
    let mut out = Vec::new();
    let mut real = |name: &str, got: &[f64], want: &[f64]| {
        let (n, rel) = compare(got, want);
        if n != 0 {
            out.push((name.to_owned(), n, rel));
        }
    };
    real("strain_yy", &t.strain, &bits(&r["strain_yy"])?);
    let want_d = bits(&r["damage"])?;
    real("damage", &t.damage, &want_d);
    real("energy", &t.energy, &bits(&r["energy"])?);
    real(
        "potential_energy",
        &t.potential,
        &bits(&r["potential_energy"])?,
    );
    real("solid_error", &t.solid_error, &bits(&r["solid_error"])?);
    real("stress_yy_q0", &t.stress_yy_q0, &bits(&r["stress_yy_q0"])?);
    real(
        "material_damage_q0",
        &t.material_damage_q0,
        &bits(&r["material_damage_q0"])?,
    );
    for (name, got) in &t.finals {
        real(&format!("final/{name}"), got, &bits(&r["final"][name])?);
    }
    let want_it = ints(&r["solid_iterations"])?;
    if t.solid_iterations != want_it {
        let n = t
            .solid_iterations
            .iter()
            .zip(&want_it)
            .filter(|(a, b)| a != b)
            .count();
        out.push(("solid_iterations".into(), n.max(1), f64::NAN));
    }
    for (k, name) in CPL_DIGESTS.iter().enumerate() {
        let want = strs(&r["digests"][name])?;
        if t.digests[k] != want {
            let n = t.digests[k]
                .iter()
                .zip(&want)
                .filter(|(a, b)| a != b)
                .count();
            out.push((format!("digests/{name}"), n.max(1), f64::NAN));
        }
    }
    let nn = 4;
    let first = t
        .damage
        .iter()
        .zip(&want_d)
        .position(|(a, b)| a.to_bits() != b.to_bits())
        .map(|k| k / nn);
    Ok((out, first))
}

/// Bounds `0 ≤ d ≤ 1` and irreversibility (non-decreasing at every node).
fn bound_violations(damage: &[Real], nn: usize) -> usize {
    let steps = damage.len() / nn;
    let mut bad = 0;
    for k in 0..steps {
        for i in 0..nn {
            let d = damage[k * nn + i];
            let prev = if k == 0 { 0. } else { damage[(k - 1) * nn + i] };
            if !(0. ..=1.).contains(&d) || d < prev {
                bad += 1;
            }
        }
    }
    bad
}

/// The coupled TEST-100 records replayed (see the module
/// documentation).
fn coupler_tao_oracle(c: &Communicator) -> Outcome {
    if c.size() > 1 {
        return Ok(());
    }
    // D-121: outside the records' environment the records' integrity, every
    // run's bounds and irreversibility are checked and the differences
    // printed; whether a run matches or differs from the record is not
    // asserted.
    let compare_values = pftao_comparability(c, "coupler_tao_oracle").is_none();
    let mut fails = Vec::new();
    let mut missing = Vec::new();
    for (law, split) in CPL_TAO_CELLS {
        let case = format!("cpl_tao_{law}_{split}");
        let Some(rec) = oracle_record(&case)? else {
            missing.push(case);
            continue;
        };
        let r = &rec["result"];
        if r["status"].as_str() != Some("ok") || r["steps"].as_i64() != Some(1000) {
            fails.push(format!("{case}: the record is not a complete run"));
            continue;
        }
        let material = r["material_text"]
            .as_str()
            .ok_or("no material_text")?
            .to_owned();
        if material != coupled_material(law, split)? {
            fails.push(format!(
                "{case}: the recorded material is not the port's text"
            ));
        }
        // The full defaults, and for the two-switch
        // (volumetric–deviatoric) cells each leave-one-out run, must differ
        // from the oracle: each switch of `Needed` is necessary
        let mut modes = vec![CplMode::Upstream, CplMode::Needed, CplMode::Defaults];
        if split == "volumetric_deviatoric" {
            modes.extend([CplMode::NeededWithoutUd422, CplMode::NeededWithoutUd387]);
        }
        for mode in modes {
            let t = coupled_tao_run(c, split, mode, &material)
                .map_err(|e| format!("{case} ({mode:?}): {e}"))?;
            let bad = bound_violations(&t.damage, 4);
            if bad > 0 {
                fails.push(format!(
                    "{case} ({mode:?}): {bad} bound / irreversibility violations"
                ));
            }
            let (diff, first) = compare_cpl(&t, r)?;
            let final_d = &t.damage[t.damage.len().saturating_sub(4)..];
            if diff.is_empty() {
                println!(
                    "INFO oracle {case} ({mode:?}): 1000 steps bit for bit (damage, energies, \
                     solid Newton, check values, 4 x 1000 digests, 9 final arrays); final damage \
                     {final_d:?}"
                );
            } else {
                println!(
                    "INFO oracle {case} ({mode:?}): first differing damage step {first:?}; \
                     differing (quantity, values, max rel): {diff:?}; final damage {final_d:?}"
                );
            }
            // Bitwise under the upstream switches and with only the needed
            // ones; the full defaults and the leave-one-out runs must
            // differ (otherwise a switch of `Needed` is unnecessary and the
            // replay's claim stale), what differs is printed above.
            let must_match = matches!(mode, CplMode::Upstream | CplMode::Needed);
            if compare_values && must_match && !diff.is_empty() {
                fails.push(format!(
                    "{case} ({mode:?}): not bit for bit: first damage step {first:?}, {diff:?}"
                ));
            }
            if compare_values && !must_match && diff.is_empty() {
                fails.push(format!(
                    "{case} ({mode:?}): bit for bit without a switch of `Needed`: the switch is \
                     unnecessary"
                ));
            }
        }
        // Observation (upstream against itself): the coupled phase field
        // against the phase-only record of the same cell.
        if let Some(alone) = oracle_record(&format!("tao_{law}_{split}"))? {
            let (n, rel) = compare(&bits(&r["damage"])?, &bits(&alone["result"]["damage"])?);
            println!(
                "INFO oracle {case}: against the phase-only record tao_{law}_{split}: {n} of \
                 4000 nodal damages differ (max rel {rel:e})"
            );
        }
    }
    if !missing.is_empty() {
        fails.push(format!("oracle records missing: {missing:?}"));
    }
    agree(c, fails)
}

// ------------------------------------------------------------ isolated cases

/// Rank and size of this process from Open MPI's launcher
/// (`OMPI_COMM_WORLD_RANK` / `OMPI_COMM_WORLD_SIZE`): the isolated cases
/// must know them without holding any MPI handle.
fn launcher_rank_size() -> Result<(usize, usize), String> {
    let get = |k: &str| -> Result<usize, String> {
        std::env::var(k)
            .map_err(|_| format!("{k} not set (run the case under Open MPI's mpirun)"))?
            .parse::<usize>()
            .map_err(|e| format!("{k}: {e}"))
    };
    Ok((get("OMPI_COMM_WORLD_RANK")?, get("OMPI_COMM_WORLD_SIZE")?))
}

/// One process: serial-mesh PETSc models one
/// after another in a program that holds no MPI handle of its own and has
/// no PETSc session before the first model — TEST-100 in upstream's order
/// (`tao_type`, `tao_gatol` set in PETSc's global database before the first
/// PETSc use: queued, applied at PETSc's initialisation, which the
/// phase-field model's `init_dof_manager("petsc")` triggers), then TEST-095,
/// then the models of `init_dof_manager_rules`, each with upstream's checks;
/// then an ordinary MPI use. Before the fix, dropping the first model
/// finalised MPI (and PETSc) for the rest of the process.
fn serial_models_in_sequence() -> Outcome {
    let serial = Communicator::serial();
    test_phase_field_tao(&serial).map_err(|e| format!("TEST-100 (first model): {e}"))?;
    test_phase_field_optimal_linear(&serial)
        .map_err(|e| format!("TEST-095 (after the first model was dropped): {e}"))?;
    init_dof_manager_rules(&serial).map_err(|e| format!("init_dof_manager_rules: {e}"))?;
    let w = Communicator::mpi_world(MpiOptions::default()).map_err(e2s)?;
    let mut one = [1_u64];
    w.all_reduce(&mut one, ReduceOp::Sum).map_err(e2s)?;
    if w.size() != 1 || one[0] != 1 {
        return Err(format!("a one-process case, run at {} processes", w.size()));
    }
    println!(
        "INFO pt02_serial_models_in_sequence: three serial PETSc phases (TEST-100, TEST-095, \
         the PETSc models) one after another, then an MPI all-reduce: ok"
    );
    Ok(())
}

/// 2 / 4 processes: the serial-mesh refusal is
/// ordinary and recoverable, in a program that holds no MPI handle of its
/// own. (1) Only the last process asks for the PETSc manager of a serial
/// mesh (its first request: refused at once, nothing collective); (2) every
/// process asks (refused everywhere with the same text naming the job size;
/// the model keeps its default manager); (3) an ordinary MPI use afterwards
/// (`Communicator::mpi_world` and an all-reduce). Before the fix the lone
/// first request duplicated the world (it paired with the others' later
/// request) and the refusal finalised MPI.
fn refusal_recoverable() -> Outcome {
    let (rank, size) = launcher_rank_size()?;
    if size < 2 {
        return Err("a case for 2 or more processes".into());
    }
    let want = format!(
        "serial (not distributed) mesh runs on MPI_COMM_WORLD, which has {size} processes here"
    );
    let mut fails = Vec::new();
    let request = |fails: &mut Vec<String>, what: &str| -> Outcome {
        let mut pf = phase_model("test_one_element.msh", &tao_material(""))?;
        match pf.init_dof_manager("petsc") {
            Err(Error::Unsupported(m)) if m.contains(&want) => {}
            other => fails.push(format!("{what}: {other:?}")),
        }
        if pf.solver.dof_manager.backend().kind() == BackendKind::Petsc {
            fails.push(format!("{what}: the refused model has a PETSc manager"));
        }
        Ok(())
    };
    if rank + 1 == size {
        request(&mut fails, "first request, last process only")?;
    }
    request(&mut fails, "request on every process")?;
    let w = Communicator::mpi_world(MpiOptions::default()).map_err(e2s)?;
    let mut v = [1_u64, u64::from(fails.is_empty())];
    w.all_reduce(&mut v, ReduceOp::Sum).map_err(e2s)?;
    let size_u = u64::try_from(size).map_err(|e| e.to_string())?;
    if v[0] != size_u {
        fails.push(format!(
            "MPI all-reduce after the refusals: {} (want {size})",
            v[0]
        ));
    }
    if w.rank() == 0 {
        println!(
            "INFO pt02_refusal_recoverable: {size} processes; refusals then an MPI all-reduce: \
             sum {}, {} of {size} processes without a deviation",
            v[0], v[1]
        );
    }
    if !fails.is_empty() {
        return Err(fails.join("; "));
    }
    if v[1] != size_u {
        return Err("failed on another process".into());
    }
    Ok(())
}

/// Run isolated case `name` (`AKANTU_PETSC_CASE`): no MPI world of the
/// program and no PETSc session before the case; `CASE PASS <name>` /
/// `CASE FAIL <name>` printed by the process of launcher rank 0, the exit
/// code 0 on success; `CASE HANG` (exit 75) after the watchdog's limit.
fn isolated_case(name: &str) -> i32 {
    let case = name.to_owned();
    std::thread::spawn(move || {
        std::thread::sleep(Duration::from_secs(600));
        println!("CASE HANG {case}: no completion within 600 s");
        std::process::exit(75);
    });
    let t0 = Instant::now();
    let body: fn() -> Outcome = match name {
        "pt02_serial_models_in_sequence" => serial_models_in_sequence,
        "pt02_refusal_recoverable" => refusal_recoverable,
        _ => {
            eprintln!(
                "petsc_models: unknown AKANTU_PETSC_CASE {name} (pt02_serial_models_in_sequence, \
                 pt02_refusal_recoverable)"
            );
            return 2;
        }
    };
    let out = std::panic::catch_unwind(body).unwrap_or_else(|_| Err("panicked".into()));
    let rank = launcher_rank_size().map_or(0, |(r, _)| r);
    if let Err(e) = &out {
        println!("rank {rank}: CASE {name}: {e}");
    }
    if rank == 0 || out.is_err() {
        println!(
            "CASE {} {name} ({:.2} s)",
            if out.is_ok() { "PASS" } else { "FAIL" },
            t0.elapsed().as_secs_f64()
        );
    }
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    i32::from(out.is_err())
}

type Scenario = (&'static str, fn(&Communicator) -> Outcome);

fn main() {
    if let Ok(case) = std::env::var("AKANTU_PETSC_CASE") {
        std::process::exit(isolated_case(&case));
    }
    std::thread::spawn(|| {
        std::thread::sleep(Duration::from_secs(1500));
        eprintln!("petsc_models: watchdog expired");
        std::process::exit(70);
    });
    let control = world();
    let work = world();
    let rank = control.rank();
    // PETSc's session is tied to its first user's communicator (finalised
    // with its last handle; a re-initialised PETSc 3.22 cannot find TAO
    // types): start it on `work`, alive for the whole run.
    if let Err(e) = akantu_solver::DofManager::of_type("petsc", "petsc_session", work.clone()) {
        eprintln!("petsc_models: PETSc session: {e}");
        std::process::exit(1);
    }
    let scenarios: [Scenario; 8] = [
        ("init_dof_manager_rules", init_dof_manager_rules),
        ("other_models_on_petsc", other_models_on_petsc),
        (
            "tao_bounds_after_a_node_removal",
            tao_bounds_after_a_node_removal,
        ),
        (
            "test_phase_field_optimal_linear",
            test_phase_field_optimal_linear,
        ),
        (
            "test_phase_field_optimal_linear_distributed",
            test_phase_field_optimal_linear_distributed,
        ),
        ("test_phase_field_tao", test_phase_field_tao),
        ("phase_field_tao_oracle", phase_field_tao_oracle),
        ("coupler_tao_oracle", coupler_tao_oracle),
    ];
    let only = std::env::var("AKANTU_PETSC_ONLY").ok();
    let mut failed = 0;
    for (name, f) in scenarios {
        if only.as_deref().is_some_and(|o| !name.contains(o)) {
            continue;
        }
        let t0 = Instant::now();
        let out = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| f(&work)))
            .unwrap_or_else(|_| Err("panicked".into()));
        let mut ok = [u8::from(out.is_ok())];
        control.all_reduce(&mut ok, ReduceOp::Min).expect("control");
        if ok[0] == 0 {
            failed += 1;
        }
        if let Err(e) = &out {
            println!("rank {rank}: {name}: {e}");
        }
        if rank == 0 {
            let v = if ok[0] == 1 { "PASS" } else { "FAIL" };
            println!("{v} {name} ({:.2} s)", t0.elapsed().as_secs_f64());
        }
    }
    if rank == 0 {
        println!("petsc_models: failed={failed}");
    }
    drop(work);
    drop(control);
    let _ = akantu_comm::mpi_lifecycle().shutdown();
    std::process::exit(i32::from(failed > 0));
}
