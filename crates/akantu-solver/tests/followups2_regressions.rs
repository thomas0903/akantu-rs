// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests of the solver stack ported
// from Akantu (EPFL/LSMS): src/io/parser/parsable.cc (parseParam),
// src/model/common/model_solver.cc (initDOFManager),
// src/model/common/time_step_solvers/time_step_solver_default.cc
// (corrector), on the upstream MyModel toy
// (test/test_model/test_common/test_model_solver/test_model_solver_my_model.hh).

//! Regressions:
//!
//! * item 1: `init_from_section` parses the
//!   `non_linear_solver` sub-section with the parser's permissive flag
//!   (upstream `Parsable::parseParam` consults `Parser::isPermissive()`):
//!   unknown names are an error in strict mode and a warning otherwise;
//! * item 6: the corrector's "previous has to be registered" warning
//!   (upstream `AKANTU_DEBUG_WARNING`) goes through the debugger
//!   (`aka_debug_warning!`) once per corrector call, as upstream, not to
//!   `eprintln!`;
//! * item 7, `g04_*` (DOF-013): nodes appended between two solves
//!   (oracle case `solver_model/new_nodes_event`, x86_64): sizes, equation
//!   numbers, flags, matrices and the next solve bit for bit; UD-652: free
//!   (never assembled) appended nodes, a segmentation fault upstream, solve
//!   with a defined result in the port.

mod common;

use akantu_comm::Communicator;
use akantu_core::debug::{self, DebugLevel};
use akantu_core::enums::{
    IntegrationSchemeType, NonLinearSolverType, SparseSolverType, TimeStepSolverType,
};
use akantu_core::{GhostType, Idx, Real};
use akantu_mesh::MeshAccessor;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};

use common::my_model::{Law, MyModel, SEG, gen_mesh};
use common::oracle::{assert_bits, assert_matrix, case, i64s, input_text, param};

/// Thread-local capture of `log` records (the `log` facade has one global
/// logger per process; records are kept per thread so that tests running
/// in parallel do not see each other's warnings).
mod log_capture {
    use std::cell::RefCell;
    use std::sync::Once;

    thread_local! {
        static RECORDS: RefCell<Option<Vec<(log::Level, String)>>> = const { RefCell::new(None) };
    }

    struct Capture;

    impl log::Log for Capture {
        fn enabled(&self, _: &log::Metadata<'_>) -> bool {
            true
        }
        fn log(&self, record: &log::Record<'_>) {
            RECORDS.with(|c| {
                if let Some(v) = c.borrow_mut().as_mut() {
                    v.push((record.level(), record.args().to_string()));
                }
            });
        }
        fn flush(&self) {}
    }

    static LOGGER: Capture = Capture;
    static INIT: Once = Once::new();

    /// Run `f` and return its result with the records it logged on this
    /// thread.
    pub fn capture<R>(f: impl FnOnce() -> R) -> (R, Vec<(log::Level, String)>) {
        INIT.call_once(|| {
            log::set_logger(&LOGGER).expect("no other logger in this test binary");
            log::set_max_level(log::LevelFilter::Trace);
        });
        RECORDS.with(|c| *c.borrow_mut() = Some(Vec::new()));
        let r = f();
        let records = RECORDS.with(|c| c.borrow_mut().take().unwrap_or_default());
        (r, records)
    }
}

use log_capture::capture;

// ------------------------------------------------------------- item 1

/// A `.dat` file with an unknown parameter in the `non_linear_solver`
/// sub-section. Upstream parses the parameters sorted by name, so `bogus`
/// comes before `max_iterations`.
fn nls_section_text(permissive: bool) -> String {
    format!(
        "permissive_parser = {permissive}
model model_solver [
  time_step_solver static [
    sparse_solver_type = eigen
    non_linear_solver newton_raphson [
      max_iterations = 7
      bogus = 3
    ]
  ]
]
"
    )
}

fn build_from(parser: &Parser) -> akantu_core::Result<ModelSolver> {
    let section = &parser.root.sub_sections[0];
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(1.0, gen_mesh(5), false, &mut dm)?;
    let mut ms = ModelSolver::new(dm);
    ms.init_from_section(section, &mut model, parser.is_permissive())?;
    Ok(ms)
}

/// Before the fix the flag was hard-coded to `false`, so
/// the permissive file failed like the strict one ("No parameter named
/// bogus registered in static."); upstream (`parsable.cc:49-62`) warns and
/// keeps parsing.
#[test]
fn dc08_non_linear_solver_section_honours_the_permissive_flag() {
    // strict: the unknown name is an error (upstream AKANTU_EXCEPTION)
    let strict = Parser::parse_str(&nls_section_text(false), "strict.dat").unwrap();
    assert!(!strict.is_permissive());
    let (r, records) = capture(|| build_from(&strict));
    let msg = r.unwrap_err().to_string();
    assert!(
        msg.contains("No parameter named bogus registered in"),
        "{msg}"
    );
    assert!(records.iter().all(|(l, _)| *l != log::Level::Warn));

    // permissive: a warning (upstream AKANTU_DEBUG_WARNING), the known
    // parameters are still set and the solver is built
    let permissive = Parser::parse_str(&nls_section_text(true), "permissive.dat").unwrap();
    assert!(permissive.is_permissive());
    let (r, records) = capture(|| build_from(&permissive));
    let ms = r.unwrap();
    let nls = ms.non_linear_solver(Some("static")).unwrap();
    assert_eq!(
        nls.get_param("max_iterations").unwrap().as_int().unwrap(),
        7
    );
    let warnings: Vec<&String> = records
        .iter()
        .filter(|(l, _)| *l == log::Level::Warn)
        .map(|(_, m)| m)
        .collect();
    assert_eq!(warnings.len(), 1, "{records:?}");
    assert!(
        warnings[0].contains("No parameter named bogus registered in"),
        "{warnings:?}"
    );
}

// ------------------------------------------------------------- item 6

/// `TimeStepSolverDefault::corrector` (`time_step_solver_default.cc:192-197`):
/// a family with an increment but no previous array is skipped with one
/// `AKANTU_DEBUG_WARNING` per corrector call. With the `linear` non-linear
/// solver every step calls the corrector exactly once, so two steps give
/// two warnings with upstream's text, captured from the thread's debugger
/// at `dblWarning`. Before the fix the message went to `eprintln!`.
#[test]
fn item6_missing_previous_warns_once_per_corrector_call() {
    let mut dm = DofManager::new("dm", Communicator::serial());
    let mut model = MyModel::new(-1.0, gen_mesh(4), false, &mut dm).unwrap();
    dm.register_dofs_increment("disp").unwrap();
    assert!(!dm.has_previous_dofs("disp").unwrap());
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::Linear),
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
    let expected = "In order to compute the increment of disp a 'previous' has to be registered";
    for step in 1..=2 {
        let (r, text) = debug::capture(DebugLevel::WARNING, || ms.solve_step(&mut model, None));
        r.unwrap();
        // debugger lines: "{<timestamp>} /!\ <message> (<function>(): <file>:<line>)"
        let warnings: Vec<&str> = text
            .lines()
            .filter_map(|l| l.split_once("} /!\\ ").map(|(_, m)| m))
            .map(|m| m.rsplit_once(" (").map_or(m, |(m, _)| m))
            .collect();
        assert_eq!(warnings, [expected], "step {step}: {text}");
    }
    // the increment was not computed (upstream `continue`)
    assert!(
        ms.dof_manager
            .dofs_increment("disp")
            .unwrap()
            .as_slice()
            .iter()
            .all(|&x| x == 0.0)
    );
}

// ------------------------------------------------------- item 7 /

/// The probe's `new_nodes_case` set-up: the non-linear bar (`NonLinearBar`
/// = `Law::Cubic`), static Newton–Raphson (pseudo-time, Eigen, solution
/// criterion `1e-12`, at most 20 iterations).
fn new_nodes_setup(nb_nodes: usize, force: Real) -> (MyModel, ModelSolver) {
    let mut dm = DofManager::new("model_solver:dof_manager_default", Communicator::serial());
    let mut model = MyModel::new(force, gen_mesh(nb_nodes), false, &mut dm).unwrap();
    model.law = Law::Cubic;
    let mut ms = ModelSolver::new(dm);
    let opts = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Static),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("static", &opts, &mut model).unwrap();
    ms.set_integration_scheme(
        "static",
        "disp",
        IntegrationSchemeType::PseudoTime,
        SolutionType::Displacement,
        &model,
    )
    .unwrap();
    let nls = ms.non_linear_solver_mut(Some("static")).unwrap();
    nls.set_param("max_iterations", ParamValue::Int(20))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(1e-12)).unwrap();
    nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
        .unwrap();
    (model, ms)
}

/// How [`append_nodes`] blocks the new DOFs.
#[derive(Clone, Copy, PartialEq)]
enum NewBlocked {
    /// Every new DOF blocked.
    All,
    /// Only the new end node (the probe's `new_nodes_event`).
    Last,
    /// None (the set of blocked DOFs does not change).
    None,
}

/// Append `nb_new` nodes at `x_last + k · 0.1` (and, when `connect`, the
/// segments `[n − 1, n]` extending the bar), tell the DOF manager
/// (upstream `NewNodesEvent`) and resize the model's arrays as the probe
/// does (MyModel does not react to mesh events).
fn append_nodes(
    model: &mut MyModel,
    ms: &mut ModelSolver,
    nb_new: usize,
    connect: bool,
    blocked: NewBlocked,
) {
    let n0 = model.mesh.nb_nodes();
    let x_last = model.mesh.nodes().as_slice()[n0 - 1];
    let mut new = Vec::new();
    {
        let mut acc = MeshAccessor::new(&mut model.mesh);
        for k in 1..=nb_new {
            acc.push_node(&[x_last + k as Real * 0.1]).unwrap();
            new.push((n0 - 1 + k) as Idx);
            if connect {
                acc.connectivity_mut(SEG, GhostType::NotGhost)
                    .push_tuple(&[(n0 - 2 + k) as Idx, (n0 - 1 + k) as Idx]);
            }
        }
    }
    ms.dof_manager.on_nodes_added(&model.mesh, &new).unwrap();
    let n = n0 + nb_new;
    model.nb_dofs = n;
    model.forces.resize(n, 0.0);
    model.internal_forces.resize(n, 0.0);
    if connect {
        let x = model.mesh.nodes().as_slice().to_vec();
        for k in 1..=nb_new {
            model
                .initial_lengths
                .push((x[n0 - 1 + k] - x[n0 - 2 + k]).abs());
            model.strains.push(0.0);
            model.stresses.push(0.0);
        }
    }
    let mut b_g = ms.dof_manager.blocked_dofs_mut("disp").unwrap();
    let b = b_g.as_mut_slice();
    match blocked {
        NewBlocked::All => b[n0..].iter_mut().for_each(|x| *x = true),
        NewBlocked::Last => b[n - 1] = true,
        NewBlocked::None => {}
    }
}

/// Upstream `record_layout` of the probe.
fn layout(dm: &DofManager) -> (i64, i64, i64, Vec<i64>, Vec<i64>, Vec<i64>) {
    let local: Vec<i64> = dm
        .local_equation_numbers("disp")
        .unwrap()
        .iter()
        .map(|&l| i64::from(l))
        .collect();
    let global = (0..dm.local_system_size())
        .map(|l| dm.local_to_global_equation_number(l).unwrap_or(-1))
        .collect();
    let flags = (0..dm.local_system_size())
        .map(|l| i64::from(dm.dof_flag(l).unwrap().bits()))
        .collect();
    (
        dm.system_size(),
        dm.local_system_size() as i64,
        dm.pure_local_system_size() as i64,
        local,
        global,
        flags,
    )
}

fn assert_layout(f: &akantu_validation::Fixture, prefix: &str, dm: &DofManager) {
    let (system, local_size, pure, local, global, flags) = layout(dm);
    let p = |k: &str| param(f, &format!("{prefix}/{k}")).as_i64().unwrap();
    assert_eq!(system, p("system_size"), "{prefix}: system size");
    assert_eq!(local_size, p("local_system_size"), "{prefix}: local size");
    assert_eq!(
        pure,
        p("pure_local_system_size"),
        "{prefix}: pure local size"
    );
    assert_eq!(local, i64s(f, &format!("{prefix}/local_equation_numbers")));
    assert_eq!(
        global,
        i64s(f, &format!("{prefix}/global_equation_numbers"))
    );
    assert_eq!(flags, i64s(f, &format!("{prefix}/dof_flags")));
}

/// DOF-013: `solver_model/new_nodes_event` (solver_model
/// probe v3, x86_64): solve, append two nodes and two segments + the
/// nodes-added event, block the new end node, solve again — sizes, equation numbers, flags, the
/// cleared profiles, both solves (iterations, error, displacement,
/// residual, callback traces) and the final `K`/`J` (COO, releases) bit for
/// bit.
#[test]
fn g04_new_nodes_event_matches_oracle() {
    let f = case("new_nodes_event");
    let n = param(&f, "nb_nodes").as_u64().unwrap() as usize;
    let nb_new = param(&f, "nb_new_nodes").as_u64().unwrap() as usize;
    let force = param(&f, "force").as_f64().unwrap();
    let (mut model, mut ms) = new_nodes_setup(n, force);
    let mut values = 0;
    let mut solve = |prefix: &str, model: &mut MyModel, ms: &mut ModelSolver| {
        model.trace.clear();
        ms.solve_step(model, Some("static")).unwrap();
        let nls = ms.non_linear_solver(Some("static")).unwrap();
        assert_eq!(
            nls.get_param("nb_iterations").unwrap().as_int().unwrap(),
            param(&f, &format!("{prefix}/nb_iterations"))
                .as_i64()
                .unwrap(),
            "{prefix}: iterations"
        );
        assert_eq!(
            nls.converged(),
            param(&f, &format!("{prefix}/converged")).as_bool().unwrap()
        );
        let err = nls.get_param("error").unwrap().as_real().unwrap();
        values += assert_bits(&f, &format!("{prefix}/error"), &[err]);
        let dm = &ms.dof_manager;
        values += assert_bits(
            &f,
            &format!("{prefix}/displacement"),
            dm.dofs("disp").unwrap().as_slice(),
        );
        values += assert_bits(&f, &format!("{prefix}/residual"), &dm.residual().values());
        assert_eq!(
            model.trace_text(),
            input_text(&f, &format!("{prefix}_hooks")),
            "{prefix}: callback trace"
        );
    };
    solve("solve_1", &mut model, &mut ms);
    assert_layout(&f, "before", &ms.dof_manager);
    append_nodes(&mut model, &mut ms, nb_new, true, NewBlocked::Last);
    assert_layout(&f, "after_event", &ms.dof_manager);
    let dm = &ms.dof_manager;
    assert!(dm.has_matrix("K"));
    for id in ["K", "J"] {
        assert_eq!(
            dm.matrix(id).unwrap().nnz() as i64,
            param(&f, &format!("after_event/{id}_nnz"))
                .as_i64()
                .unwrap(),
            "{id} profile after the event"
        );
    }
    solve("solve_2", &mut model, &mut ms);
    for id in ["K", "J"] {
        values += assert_matrix(
            &f,
            &format!("matrix/{id}"),
            ms.dof_manager.matrix(id).unwrap(),
        );
    }
    println!("new_nodes_event: {values} values bit-identical");
}

/// UD-652 (category A): upstream clears every matrix profile on a
/// nodes-added event and `SparseMatrixAIJ::clearProfile` sets the size to
/// 0; the size then grows only with assembled entries, so appended nodes no
/// element touches leave `J` smaller than the residual and
/// `SparseSolverEigen::solve` reads out of bounds (segmentation fault of
/// the solver_model probe, first `new_nodes_event` attempt). The port keeps
/// the matrices at the system size; the free DOFs have no entry (not even a
/// diagonal for the blocked flag), so the next solve is refused with an
/// ordinary error (structurally singular `J`) instead of undefined
/// behaviour, and the manager stays consistent (sizes, equation numbers).
#[test]
fn ud652_free_appended_nodes_are_refused_not_undefined() {
    let (mut model, mut ms) = new_nodes_setup(11, -0.5);
    ms.solve_step(&mut model, Some("static")).unwrap();
    append_nodes(&mut model, &mut ms, 2, false, NewBlocked::All);
    assert_eq!(ms.dof_manager.system_size(), 13);
    let err = ms.solve_step(&mut model, Some("static")).unwrap_err();
    assert!(
        matches!(&err, akantu_core::Error::LinearSolver(m) if m.contains("STRUCTURALLY SINGULAR")),
        "{err:?}"
    );
    assert_eq!(ms.dof_manager.matrix("J").unwrap().size(), 13);
    assert_eq!(
        ms.dof_manager.local_equation_numbers("disp").unwrap(),
        (0..13).collect::<Vec<Idx>>().as_slice()
    );
}

/// UD-653 (category A): after a nodes-added event upstream's
/// `DOFManager::updateGlobalBlockedDofs` returns early when the set of
/// blocked equations did not change, before resizing `global_blocked_dofs`,
/// so `SparseMatrixAIJ::applyBoundary` reads beyond its end at the next
/// implicit solve (`AssertException` "The value at position [11] is out of
/// range" in the oracle's strict build, `aka_array_tmpl.hh:413`; an
/// out-of-bounds read otherwise). The port sizes its blocked flags with the
/// layout, so the extension with free new nodes solves: the unloaded new
/// segments carry no force, hence `u11 = u12 = u10`, and the original nodes
/// keep their equilibrium, within the Newton tolerance (solution criterion
/// `1e-12` on the increment; each check uses `1e-10` relative to `|u10|`,
/// two orders above the stopping threshold, see
/// `docs/NUMERICAL_VALIDATION.md`).
#[test]
fn ud653_unchanged_blocked_set_after_new_nodes_solves() {
    let (mut model, mut ms) = new_nodes_setup(11, -0.5);
    ms.solve_step(&mut model, Some("static")).unwrap();
    let before = ms.dof_manager.dofs("disp").unwrap().as_slice().to_vec();
    append_nodes(&mut model, &mut ms, 2, true, NewBlocked::None);
    ms.solve_step(&mut model, Some("static")).unwrap();
    assert!(ms.non_linear_solver(Some("static")).unwrap().converged());
    let u = ms.dof_manager.dofs("disp").unwrap().as_slice().to_vec();
    let scale = before[10].abs();
    assert!(scale > 0.0);
    for (i, (a, b)) in u[..11].iter().zip(&before).enumerate() {
        assert!((a - b).abs() <= 1e-10 * scale, "node {i}: {a} vs {b}");
    }
    for i in [11, 12] {
        assert!(
            (u[i] - u[10]).abs() <= 1e-10 * scale,
            "node {i}: {} vs {}",
            u[i],
            u[10]
        );
    }
}
