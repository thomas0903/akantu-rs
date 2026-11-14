// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): contract tests of
// src/model/common/dof_manager/{dof_manager.cc, dof_manager_default.cc}
// (assembleToGlobalArray periodic rule, getArrayPerDOFs,
// assembleElementalArray*, assembleMatMulDOFsToResidual,
// updateGlobalBlockedDofs/applyBoundary, getNewMatrix(copy)),
// non_linear_solver.cc (split residual of the linear solver) and
// model_solver.cc (initDOFManager from a parser section).

//! Hand-derived checks of DOF-manager assemblies and of the model-solver
//! glue paths the oracle toy does not exercise.

mod common;

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::enums::{
    IntegrationSchemeType, MatrixType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{Array, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_parser::Parser;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType, SolverCallback};

use common::my_model::{MyModel, SEG, gen_mesh};

/// Upstream `assembleToGlobalArray` on a periodic mesh: a periodic slave
/// shares its master's equation and contributes `scale·v·0`; reading per
/// DOF gives the slave its master's value.
#[test]
fn periodic_slaves_alias_and_do_not_contribute() {
    let mut mesh = gen_mesh(3);
    MeshAccessor::new(&mut mesh)
        .add_periodic_pair(2, 0)
        .unwrap();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
    assert_eq!(dm.local_equation_numbers("u").unwrap(), &[0, 1, 0]);
    assert_eq!(dm.local_system_size(), 2);
    dm.assemble_to_residual("u", ArrayView::new(&[1.0, 2.0, 3.0], 1).unwrap(), 2.0)
        .unwrap();
    assert_eq!(dm.residual().values(), [2.0, 4.0]);
    let mut out = Array::try_zeros(0, 1).unwrap();
    let r = dm.residual().values().to_vec();
    dm.get_array_per_dofs("u", &r, &mut out).unwrap();
    assert_eq!(out.as_slice(), &[2.0, 4.0, 2.0]);
}

#[test]
fn elemental_arrays_to_residual_and_lumped_matrix() {
    let mesh = gen_mesh(4); // 3 segments
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    // per element: node-major, component-minor (n0c0 n0c1 n1c0 n1c1)
    let el = Array::from_vec((0..12).map(f64::from).collect(), 4).unwrap();
    dm.assemble_elemental_array_to_residual(
        &mesh,
        "u",
        el.view(),
        SEG,
        GhostType::NotGhost,
        0.5,
        None,
    )
    .unwrap();
    let want = [
        0.0,
        0.5,
        1.0 + 2.0,
        1.5 + 2.5,
        3.0 + 4.0,
        3.5 + 4.5,
        5.0,
        5.5,
    ];
    assert_eq!(dm.residual().values(), want);
    // filtered: elements [2, 0] with two element vectors
    dm.new_lumped_matrix("M").unwrap();
    let el2 = Array::from_vec(vec![1.0, 1.0, 2.0, 2.0, 10.0, 10.0, 20.0, 20.0], 4).unwrap();
    dm.assemble_elemental_array_to_lumped_matrix(
        &mesh,
        "u",
        el2.view(),
        "M",
        SEG,
        GhostType::NotGhost,
        1.0,
        Some(&[2, 0]),
    )
    .unwrap();
    // element 2 (nodes 2, 3) receives the first vector, element 0 (nodes
    // 0, 1) the second
    assert_eq!(
        dm.lumped_matrix("M").unwrap().values,
        [10.0, 10.0, 20.0, 20.0, 1.0, 1.0, 2.0, 2.0]
    );
    // invalid inputs are rejected before any change
    let before = dm.residual().values().to_vec();
    assert!(
        dm.assemble_elemental_array_to_residual(
            &mesh,
            "u",
            el2.view(),
            SEG,
            GhostType::NotGhost,
            1.0,
            Some(&[7])
        )
        .is_err()
    );
    assert!(
        dm.assemble_elemental_array_to_residual(
            &mesh,
            "u",
            el.view(),
            SEG,
            GhostType::NotGhost,
            1.0,
            Some(&[0])
        )
        .is_err()
    );
    let bad = Array::from_vec(vec![1.0; 9], 3).unwrap(); // 3 components for 2 nodes
    assert!(
        dm.assemble_elemental_array_to_residual(
            &mesh,
            "u",
            bad.view(),
            SEG,
            GhostType::NotGhost,
            1.0,
            None
        )
        .is_err()
    );
    assert_eq!(dm.residual().values(), before);
}

/// Upstream `updateGlobalBlockedDofs` / `applyBoundary`: the release is
/// incremented only when the blocked set changes; a matrix unchanged since
/// the last application is re-treated only when the set changed.
#[test]
fn blocked_snapshot_release_and_apply_boundary_rule() {
    let mesh = gen_mesh(3);
    let mut dm = DofManager::new("dm", Communicator::serial());
    let model = MyModel::new(1.0, mesh, false, &mut dm).unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    let mut k = Array::new(4);
    for _ in 0..2 {
        k.push_tuple(&[1.0, -1.0, -1.0, 1.0]);
    }
    dm.assemble_elemental_matrices_to_matrix(
        &model.mesh,
        "K",
        "disp",
        k.view(),
        SEG,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .unwrap();
    dm.update_global_blocked_dofs();
    assert_eq!(dm.global_blocked_dofs_indexes(), &[0]);
    assert_eq!(dm.blocked_release(), 1);
    assert!(dm.has_blocked_dofs_changed());
    dm.apply_boundary("K").unwrap();
    let r1 = dm.matrix_release("K").unwrap();
    assert!(!dm.has_blocked_dofs_changed());
    // unchanged matrix, unchanged set: not re-applied
    dm.update_global_blocked_dofs();
    assert_eq!(dm.blocked_release(), 1);
    dm.apply_boundary("K").unwrap();
    assert_eq!(dm.matrix_release("K").unwrap(), r1);
    // set changed: re-applied on the unchanged matrix
    dm.blocked_dofs_mut("disp").unwrap().as_mut_slice()[2] = true;
    dm.update_global_blocked_dofs();
    assert_eq!(dm.global_blocked_dofs_indexes(), &[0, 2]);
    assert_eq!(dm.blocked_release(), 2);
    dm.apply_boundary("K").unwrap();
    assert_eq!(dm.matrix_release("K").unwrap(), r1 + 1);
    let kk = dm.matrix("K").unwrap();
    assert_eq!(kk.get(2, 2), 1.0);
    assert_eq!(kk.get(1, 2), 0.0);
    assert_eq!(kk.get(1, 1), 2.0);
}

#[test]
fn new_matrix_copy_follows_the_copy_constructor() {
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("u", 3, 1).unwrap();
    dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    {
        let k = dm.matrix_mut("K").unwrap();
        k.add(0, 0, 2.0).unwrap();
        k.add(0, 1, -1.0).unwrap();
        k.add(1, 1, 2.0).unwrap();
    }
    dm.new_matrix_copy("K2", "K").unwrap();
    let (a, b) = (dm.matrix("K").unwrap(), dm.matrix("K2").unwrap());
    assert_eq!(a.coo(), b.coo());
    assert_eq!((b.profile_release(), b.value_release()), (1, 1));
    assert_eq!(dm.declared_matrix_type("K2"), Some(MatrixType::Symmetric));
    assert!(dm.new_matrix_copy("K2", "K").is_err());
    assert!(dm.new_matrix_copy("K3", "missing").is_err());
}

/// A linear spring chain that can split its residual (upstream
/// `canSplitResidual`): `external` = forces, `internal` = −K u.
struct Splittable {
    split: bool,
    calls: Vec<String>,
}

impl SolverCallback for Splittable {
    fn matrix_type(&self, id: &str) -> MatrixType {
        if id == "K" {
            MatrixType::Symmetric
        } else {
            MatrixType::NotDefined
        }
    }
    fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
        self.calls.push(format!("matrix:{id}"));
        if id != "K" {
            return Ok(());
        }
        let k = dm.matrix_mut("K")?;
        k.zero();
        for i in 0..3 {
            k.add(i, i, if i == 2 { 1.0 } else { 2.0 })?;
        }
        k.add(0, 1, -1.0)?;
        k.add(1, 2, -1.0)?;
        Ok(())
    }
    fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
        self.calls.push("residual".into());
        let u = dm.dofs("u")?.as_slice().to_vec();
        let ku = [2.0 * u[0] - u[1], -u[0] + 2.0 * u[1] - u[2], -u[1] + u[2]];
        let r: Vec<Real> = (0..3)
            .map(|i| if i == 2 { 1.0 } else { 0.0 } - ku[i])
            .collect();
        dm.assemble_to_residual("u", ArrayView::new(&r, 1)?, 1.0)
    }
    fn assemble_residual_part(&mut self, dm: &mut DofManager, part: &str) -> Result<()> {
        self.calls.push(format!("residual:{part}"));
        match part {
            "external" => dm.assemble_to_residual("u", ArrayView::new(&[0.0, 0.0, 1.0], 1)?, 1.0),
            "internal" => {
                let u = dm.dofs("u")?.as_slice().to_vec();
                let ku = [2.0 * u[0] - u[1], -u[0] + 2.0 * u[1] - u[2], -u[1] + u[2]];
                dm.assemble_to_residual("u", ArrayView::new(&ku, 1)?, -1.0)
            }
            _ => Ok(()),
        }
    }
    fn can_split_residual(&self) -> bool {
        self.split
    }
}

fn linear_solve(split: bool) -> (Vec<Real>, Vec<String>) {
    let mut cb = Splittable {
        split,
        calls: vec![],
    };
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("u", 3, 1).unwrap();
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::Linear),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, &mut cb).unwrap();
    ms.set_integration_scheme(
        "s",
        "u",
        IntegrationSchemeType::PseudoTime,
        SolutionType::NotDefined,
        &cb,
    )
    .unwrap();
    ms.solve_step(&mut cb, None).unwrap();
    (
        ms.dof_manager.dofs("u").unwrap().as_slice().to_vec(),
        cb.calls,
    )
}

/// Upstream `NonLinearSolver::assembleResidual` with a splittable residual
/// and the linear solver: zero residual, `external`, `−K·u` (assembled by
/// the DOF manager over every family), `inertial`; the final residual of
/// `solve_linear` is the `internal` part. Same solution as unsplit.
#[test]
fn linear_solver_split_residual_path() {
    let (u_split, calls_split) = linear_solve(true);
    let (u_plain, calls_plain) = linear_solve(false);
    assert_eq!(u_split, u_plain);
    for (a, b) in u_plain.iter().zip([1.0, 2.0, 3.0]) {
        assert!((a - b).abs() < 1e-14, "{u_plain:?}");
    }
    assert_eq!(
        calls_split,
        // the "inertial" part goes to the integration schemes only
        // (upstream TimeStepSolverDefault::assembleResidual(part))
        ["matrix:K", "residual:external", "residual:internal"]
    );
    assert_eq!(calls_plain, ["matrix:K", "residual", "residual"]);
}

/// Upstream `initDOFManager(section)`: solvers, non-linear solver
/// parameters, integration schemes and the default solver from the model's
/// `.dat` section.
#[test]
fn init_from_section_builds_solvers_and_schemes() {
    let text = "model model_solver [
  time_step_solver static [
    name = mystatic
    sparse_solver_type = eigen
    non_linear_solver newton_raphson_modified [
      max_iterations = 5
      threshold = 1e-8
      convergence_type = residual
    ]
    integration_scheme pseudo_time [
      name = disp
    ]
  ]
  time_step_solver dynamic [
    sparse_solver_type = faer
    integration_scheme trapezoidal_rule_2 [
      name = disp
      solution_type = acceleration
    ]
  ]
  default_solver = dynamic
]
";
    let parser = Parser::parse_str(text, "solvers.dat").unwrap();
    let section = &parser.root.sub_sections[0];
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(1.0, gen_mesh(5), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    ms.init_from_section(section, &mut model, false).unwrap();
    assert!(ms.has_solver("mystatic") && ms.has_solver("dynamic"));
    assert_eq!(ms.solver(None).unwrap().id, "dynamic");
    let nls = ms.non_linear_solver(Some("mystatic")).unwrap();
    assert_eq!(nls.kind(), NonLinearSolverType::NewtonRaphsonModified);
    assert_eq!(
        nls.get_param("max_iterations").unwrap().as_int().unwrap(),
        5
    );
    assert_eq!(nls.get_param("threshold").unwrap().as_real().unwrap(), 1e-8);
    assert_eq!(
        ms.non_linear_solver(Some("dynamic")).unwrap().kind(),
        NonLinearSolverType::NewtonRaphson
    );
    // `sparse_solver_type = faer` (D-040 extension value) selects faer
    // each Newton solver re-installs its own sparse solver before solving
    assert_eq!(
        ms.dof_manager.backend().sparse_solver_kind("J"),
        Some(SparseSolverType::Faer)
    );
    let tss = ms.solver(Some("dynamic")).unwrap();
    assert_eq!(tss.solution_type("disp"), Some(SolutionType::Acceleration));
    assert_eq!(
        tss.integration_scheme("disp").unwrap().kind(),
        IntegrationSchemeType::TrapezoidalRule2
    );
    assert!(ms.dof_manager.has_matrix("K") && ms.dof_manager.has_matrix("M"));
    assert!(!ms.dof_manager.has_matrix("C"));
    // both solvers run
    ms.set_time_step(0.01, None).unwrap();
    ms.solve_step(&mut model, Some("mystatic")).unwrap();
    assert_eq!(
        ms.dof_manager.backend().sparse_solver_kind("J"),
        Some(SparseSolverType::Eigen)
    );
    ms.solve_step(&mut model, None).unwrap();
    assert_eq!(
        ms.dof_manager.backend().sparse_solver_kind("J"),
        Some(SparseSolverType::Faer)
    );

    // errors: two non-linear solvers, unknown default solver
    let bad = "model m [
  time_step_solver static [
    non_linear_solver newton_raphson [
    ]
    non_linear_solver linear [
    ]
  ]
]
";
    let p = Parser::parse_str(bad, "bad.dat").unwrap();
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(1.0, gen_mesh(3), false, &mut dm).unwrap();
    let mut ms = ModelSolver::new(dm);
    assert!(
        ms.init_from_section(&p.root.sub_sections[0], &mut model, false)
            .is_err()
    );
    let bad = "model m [
  default_solver = nothing
]
";
    let p = Parser::parse_str(bad, "bad2.dat").unwrap();
    assert!(
        ms.init_from_section(&p.root.sub_sections[0], &mut model, false)
            .is_err()
    );
}

/// The upstream `hasDOFsDerivatives` / auxiliary-array registrations.
#[test]
fn auxiliary_arrays_and_increment_in_the_corrector() {
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(-1.0, gen_mesh(4), false, &mut dm).unwrap();
    dm.register_dofs_previous("disp").unwrap();
    dm.register_dofs_increment("disp").unwrap();
    assert!(dm.register_dofs_previous("disp").is_err());
    assert!(dm.register_dofs_increment("disp").is_err());
    assert!(dm.register_dofs_derivative("disp", 1).is_err());
    assert!(dm.register_blocked_dofs("disp").is_err());
    assert!(dm.has_dofs_derivative("disp", 2).unwrap());
    assert!(!dm.has_dofs_derivative("disp", 3).unwrap());
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &opts, &mut model).unwrap();
    ms.set_integration_scheme(
        "s",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::Displacement,
        &model,
    )
    .unwrap();
    ms.solve_step(&mut model, None).unwrap();
    let dm = &ms.dof_manager;
    // displacement-type solution: increment = solution of the last solve
    assert_eq!(
        dm.dofs_increment("disp").unwrap().as_slice(),
        dm.dofs_solution("disp").unwrap().as_slice()
    );
    // previous saved by the predictor (zero state)
    assert!(
        dm.previous_dofs("disp")
            .unwrap()
            .as_slice()
            .iter()
            .all(|&x| x == 0.0)
    );
    let u: Vec<f64> = dm.dofs("disp").unwrap().as_slice().to_vec();
    let x: Vec<f64> = (0..4).map(|i| -(i as f64) / 3.0).collect();
    for (a, b) in u.iter().zip(&x) {
        assert!((a - b).abs() < 1e-14);
    }
    let _ = Idx::default();
    let _ = Mesh::new(1, "unused");
}

/// Upstream DOF classification predicates (`isLocalOrMasterDOF`,
/// `isSlaveDOF`, `isPureGhostDOF`, `getDOFFlag`) on a two-rank bar with a
/// shared node, and the global ↔ local equation maps.
#[test]
fn dof_classification_predicates() {
    let out = akantu_comm::run_local(2, std::time::Duration::from_secs(20), |comm| {
        let mesh = common::two_rank_bar(&comm);
        let mut dm = DofManager::new("dm", comm.clone());
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        (0..dm.local_system_size())
            .map(|l| {
                (
                    dm.is_local_or_master_dof(l).unwrap(),
                    dm.is_slave_dof(l).unwrap(),
                    dm.is_pure_ghost_dof(l).unwrap(),
                    dm.local_to_global_equation_number(l),
                    dm.dof_flag(l).unwrap().bits(),
                )
            })
            .collect::<Vec<_>>()
    });
    // rank 0: g0, g1, g2 (master) — rank 1: g2 (slave), g3, g4
    assert_eq!(
        out[0][2],
        (
            true,
            false,
            false,
            Some(2),
            akantu_core::NodeFlag::MASTER.bits()
        )
    );
    assert_eq!(
        out[1][0],
        (
            false,
            true,
            false,
            Some(2),
            akantu_core::NodeFlag::SLAVE.bits()
        )
    );
    assert_eq!(out[1][2], (true, false, false, Some(4), 0));
    let dm = DofManager::new("dm", Communicator::serial());
    assert!(dm.dof_flag(0).is_err());
    assert!(!dm.has_global_equation_number(0));
}

/// Upstream `saveAssociatedNodes` (DOF-017): a lumped vector `nodes` with
/// the global node id of every nodal DOF, written as Matrix Market.
#[test]
fn save_associated_nodes_writes_global_ids() {
    let mesh = gen_mesh(3);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 2).unwrap();
    let dir = std::env::temp_dir().join(format!("akantu-solver-nodes-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("nodes");
    dm.save_associated_nodes(&mesh, &path).unwrap();
    assert_eq!(
        dm.lumped_matrix("nodes").unwrap().values,
        [0.0, 0.0, 1.0, 1.0, 2.0, 2.0]
    );
    let text = std::fs::read_to_string(dir.join("nodes.mtx")).unwrap();
    assert!(
        text.starts_with("%%MatrixMarket matrix coordinate real general\n6 1 6\n"),
        "{text}"
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// A bar with an unsymmetric element stiffness assembled into a `K`
/// declared symmetric (UD-034 default policy: promotion agreed at
/// finalisation and propagated to the derived `J`) and a symmetric
/// consistent mass. With the default `MatrixAddPolicy` (mirror fix)
/// `J = c_K K + c_M M` is the exact Jacobian and the linear step converges
/// in two Newton iterations (solution criterion); upstream's add drops the
/// mirror of `M` in the unsymmetric `J`, so Newton converges only
/// linearly (or not at all).
#[test]
fn promoted_stiffness_propagates_to_j_and_mass_is_mirrored() {
    struct Unsym;
    const KE: [f64; 4] = [3.0, -1.0, -2.0, 1.5]; // column-major [[3, -2], [-1, 1.5]]
    impl SolverCallback for Unsym {
        fn matrix_type(&self, id: &str) -> MatrixType {
            if id == "C" {
                MatrixType::NotDefined
            } else {
                MatrixType::Symmetric
            }
        }
        fn assemble_matrix(&mut self, dm: &mut DofManager, id: &str) -> Result<()> {
            let mesh = gen_mesh(4);
            let (ke, ty) = match id {
                "K" => (KE, MatrixType::Unsymmetric),
                "M" => ([2.0, 1.0, 1.0, 2.0], MatrixType::Symmetric),
                _ => return Ok(()),
            };
            dm.zero_matrix(id)?;
            let mut el = Array::new(4);
            for _ in 0..3 {
                el.push_tuple(&ke);
            }
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                id,
                "u",
                el.view(),
                SEG,
                GhostType::NotGhost,
                ty,
                None,
            )
        }
        fn assemble_residual(&mut self, dm: &mut DofManager) -> Result<()> {
            let u = dm.dofs("u")?.as_slice().to_vec();
            let mut r = vec![0.0, 0.0, 0.0, 1.0];
            for e in 0..3 {
                let (a, b) = (u[e], u[e + 1]);
                r[e] -= KE[0] * a + KE[2] * b;
                r[e + 1] -= KE[1] * a + KE[3] * b;
            }
            dm.assemble_to_residual("u", ArrayView::new(&r, 1)?, 1.0)
        }
    }
    let run = |policy: akantu_solver::MatrixAddPolicy| {
        let mesh = gen_mesh(4);
        let mut backend = akantu_solver::AijBackend::default();
        backend.set_matrix_add_policy(policy);
        let mut dm = DofManager::with_backend("dm", Communicator::serial(), Box::new(backend));
        dm.register_nodal_dofs(&mesh, "u", 1).unwrap();
        dm.register_dofs_derivative("u", 1).unwrap();
        dm.register_dofs_derivative("u", 2).unwrap();
        dm.register_blocked_dofs("u").unwrap();
        dm.blocked_dofs_mut("u").unwrap().as_mut_slice()[0] = true;
        let mut cb = Unsym;
        let mut ms = ModelSolver::new(dm);
        let opts = ModelSolverOptions {
            time_step_solver_type: Some(TimeStepSolverType::Dynamic),
            non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        };
        ms.get_new_solver("d", &opts, &mut cb).unwrap();
        ms.set_integration_scheme(
            "d",
            "u",
            IntegrationSchemeType::TrapezoidalRule2,
            SolutionType::Displacement,
            &cb,
        )
        .unwrap();
        ms.set_time_step(0.5, None).unwrap();
        ms.non_linear_solver_mut(None)
            .unwrap()
            .set_param("max_iterations", akantu_parser::param::ParamValue::Int(50))
            .unwrap();
        let r = ms.solve_step(&mut cb, None);
        let n = ms.non_linear_solver(None).unwrap().nb_iterations();
        let ty = (
            ms.dof_manager.matrix_type("K").unwrap(),
            ms.dof_manager.matrix_type("J").unwrap(),
        );
        (r.is_ok(), n, ty)
    };
    let (ok, n, ty) = run(akantu_solver::MatrixAddPolicy::MirrorSymmetricSource);
    assert_eq!(ty, (MatrixType::Unsymmetric, MatrixType::Unsymmetric));
    assert!(
        ok && n == 2,
        "exact Jacobian: converged {ok} in {n} iterations"
    );
    let (ok_up, n_up, _) = run(akantu_solver::MatrixAddPolicy::Upstream);
    println!("mirror fix: {n} iterations; upstream add: converged {ok_up} after {n_up} iterations");
    assert!(!ok_up || n_up > 2, "upstream add policy unexpectedly exact");
}
