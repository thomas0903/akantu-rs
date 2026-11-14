// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parsable_petsc.hh
// (ParsablePETSc::set / parseSection / exceptIfWrongParameter),
// src/model/common/non_linear_solver/non_linear_solver_petsc.cc:35-60 and
// non_linear_solver_tao.cc:38-70 (akantu_to_petsc_options, registered
// read-only parameters).

//! PETSc option forwarding of the native non-linear solvers (NLS-008,
//! PARSE-010, INT-001) — pure Rust, no PETSc needed: the options are stored
//! in the solver and handed to the backend at each solve.

use akantu_core::enums::NonLinearSolverType;
use akantu_parser::param::{ParamValue, ParameterRegistry};
use akantu_parser::{Parser, ParserType};
use akantu_solver::{NativeNonLinear, NonLinearSolver};

fn section(text: &str) -> akantu_parser::ParserSection {
    let p = Parser::parse_str(text, "<test>").unwrap();
    p.root
        .sub_sections_of(ParserType::NonLinearSolver)
        .next()
        .unwrap()
        .clone()
}

/// Upstream `exceptIfWrongParameter`: akantu's names are refused with the
/// PETSc option to use instead.
#[test]
fn akantu_names_are_refused_with_the_petsc_option() {
    let mut snes = NativeNonLinear::petsc_snes("s");
    for (name, petsc) in [
        ("max_iterations", "snes_max_it"),
        ("threshold", "snes_atol\", \"snes_rtol\" or \"snes_stol"),
        (
            "convergence_type",
            "snes_atol\", \"snes_rtol\" or \"snes_stol",
        ),
    ] {
        let e = snes.set_param(name, ParamValue::Int(3)).unwrap_err();
        let m = e.to_string();
        assert!(
            m.contains(&format!("Cannot set the parameter \"{name}\""))
                && m.contains(&format!("use the PETSc option \"{petsc}\" instead")),
            "{m}"
        );
    }
    let mut tao = NativeNonLinear::petsc_tao("t");
    for (name, petsc) in [
        ("max_iterations", "tao_max_it"),
        ("absolute_threshold", "tao_gatol"),
        ("relative_threshold", "tao_grtol"),
    ] {
        let m = tao
            .set_param(name, ParamValue::Real(1e-3))
            .unwrap_err()
            .to_string();
        assert!(m.contains(&format!("\"{petsc}\"")), "{m}");
    }
    assert!(snes.options().is_empty() && tao.options().is_empty());
}

/// Upstream `ParsablePETSc::set`: other names become `-name value`
/// options; reals use round-trip digits (upstream: 6 significant digits,
/// UD-424, reproduced by the compat format); booleans `true`/`false`.
#[test]
fn other_names_become_petsc_options() {
    let mut snes = NativeNonLinear::petsc_snes("s");
    snes.set_param("snes_rtol", ParamValue::Real(1e-10))
        .unwrap();
    snes.set_param("snes_max_it", ParamValue::Int(10)).unwrap();
    snes.set_param("ksp_type", ParamValue::String("preonly".into()))
        .unwrap();
    snes.set_param("snes_converged_reason", ParamValue::Bool(true))
        .unwrap();
    snes.set_param("snes_atol", ParamValue::Real(0.1234567891))
        .unwrap();
    let opts = snes.options();
    let o: Vec<(&str, &str)> = opts.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
    assert_eq!(
        o,
        [
            ("snes_rtol", "1e-10"),
            ("snes_max_it", "10"),
            ("ksp_type", "preonly"),
            ("snes_converged_reason", "true"),
            ("snes_atol", "0.1234567891"),
        ]
    );
    // UD-424 compat (manager-wide `petsc_six_digit_real_options`).
    let up = snes.options_formatted(true);
    assert_eq!(up[0].1, "1e-10");
    assert_eq!(up[4].1, "0.123457");
    assert!(
        snes.set_param("snes_x", ParamValue::VectorReal(vec![1.0]))
            .is_err(),
        "vectors are not PETSc options"
    );
}

/// The registered read-only parameters of `NonLinearSolverPETSc` are
/// readable (before any solve: zeros / empty reason) and not writable.
#[test]
fn snes_read_only_parameters() {
    let mut snes = NativeNonLinear::petsc_snes("s");
    assert_eq!(
        snes.param_names(),
        [
            "n_iter",
            "convergence_reason",
            "function_norm",
            "solution_norm",
            "update_norm",
            "error"
        ]
    );
    assert!(matches!(
        snes.get_param("n_iter").unwrap(),
        ParamValue::Int(0)
    ));
    assert!(snes.set_param("error", ParamValue::Real(1.0)).is_err());
    assert!(snes.get_param("snes_rtol").is_err());
    assert_eq!(snes.kind(), NonLinearSolverType::PetscSnes);
}

/// Upstream `ParsablePETSc::parseSection`: every parameter of the section
/// (sorted by name, raw value) becomes an option; a refused or read-only
/// name fails the whole section before any option is stored.
#[test]
fn sections_become_options() {
    let mut snes = NativeNonLinear::petsc_snes("s");
    let s = section(
        "non_linear_solver petsc_snes [\n  snes_type = newtonls\n  ksp_type = preonly\n  pc_type = lu\n  snes_rtol = 1e-8\n]\n",
    );
    assert!(matches!(snes.parse_options_section(&s), Some(Ok(()))));
    let opts = snes.options();
    let names: Vec<&str> = opts.iter().map(|(k, _)| k.as_str()).collect();
    assert_eq!(names, ["ksp_type", "pc_type", "snes_rtol", "snes_type"]);
    assert_eq!(opts[2].1, "1e-8");
    for bad in [
        "non_linear_solver petsc_snes [\n  ksp_type = cg\n  threshold = 1e-3\n]\n",
        "non_linear_solver petsc_snes [\n  ksp_type = cg\n  n_iter = 3\n]\n",
    ] {
        let mut snes = NativeNonLinear::petsc_snes("s");
        assert!(matches!(
            snes.parse_options_section(&section(bad)),
            Some(Err(_))
        ));
        assert!(snes.options().is_empty(), "no option stored on failure");
    }
    // Generic native solvers keep the generic parsing.
    let mut generic = NativeNonLinear::new("J");
    assert!(generic.parse_options_section(&s).is_none());
}

/// Bounds belong to TAO (upstream `NonLinearSolverTAO::setBounds`).
#[test]
fn bounds_only_for_tao() {
    let mut snes = NativeNonLinear::petsc_snes("s");
    assert!(snes.set_bounds(vec![0.0], vec![1.0]).is_err());
    let mut tao = NativeNonLinear::petsc_tao("t");
    assert!(tao.set_bounds(vec![0.0, 0.0], vec![1.0]).is_err());
    assert!(tao.set_bounds(vec![0.0], vec![1.0]).is_ok());
    assert_eq!(tao.kind(), NonLinearSolverType::PetscTao);
}

/// Upstream `DOFManagerFactory`: `default` exists everywhere; `petsc` only
/// with the feature (and an MPI communicator).
#[test]
fn dof_manager_factory_names() {
    use akantu_comm::Communicator;
    use akantu_solver::DofManager;
    assert!(DofManager::of_type("default", "dm", Communicator::serial()).is_ok());
    assert!(DofManager::of_type("mumps", "dm", Communicator::serial()).is_err());
    // Without the feature `petsc` is not registered; with it, a serial
    // (non-MPI) communicator is refused.
    assert!(DofManager::of_type("petsc", "dm", Communicator::serial()).is_err());
}

/// Upstream `NonLinearSolverTAO` registers `max_iterations` (10),
/// `threshold` (1e-10) and `convergence_type` (`_solution`) as
/// `_pat_parsmod` (`non_linear_solver_tao.cc:68-76`): readable through `get`
/// with those values, refused by `set` and by sections
/// (`exceptIfWrongParameter` comes first), unchanged after a refusal.
#[test]
fn tao_readable_parameters() {
    let mut tao = NativeNonLinear::petsc_tao("t");
    assert_eq!(
        tao.param_names(),
        ["max_iterations", "threshold", "convergence_type"]
    );
    assert_eq!(
        tao.get_param("max_iterations").unwrap(),
        ParamValue::Int(10)
    );
    assert_eq!(tao.get_param("threshold").unwrap(), ParamValue::Real(1e-10));
    assert_eq!(
        tao.get_param("convergence_type").unwrap(),
        ParamValue::Enum("solution".into())
    );
    let info = tao.param_info("threshold").unwrap();
    assert_eq!(info.access, akantu_parser::param::ParameterAccess::PARSMOD);
    for (name, v) in [
        ("max_iterations", ParamValue::Int(3)),
        ("threshold", ParamValue::Real(1e-3)),
        ("convergence_type", ParamValue::Enum("residual".into())),
    ] {
        let m = tao.set_param(name, v).unwrap_err().to_string();
        assert!(
            m.contains(&format!(
                "Cannot set the parameter \"{name}\" on object \"t\""
            )),
            "{m}"
        );
    }
    let s = section("non_linear_solver petsc_tao [\n  tao_type = gpcg\n  threshold = 1e-3\n]\n");
    assert!(matches!(tao.parse_options_section(&s), Some(Err(_))));
    assert_eq!(
        tao.get_param("max_iterations").unwrap(),
        ParamValue::Int(10)
    );
    assert_eq!(tao.get_param("threshold").unwrap(), ParamValue::Real(1e-10));
    assert!(tao.options().is_empty());
    assert!(
        tao.get_param("n_iter").is_err(),
        "SNES-only read-only parameter"
    );
}

/// Upstream's `TAONotConvergedException` text
/// (`non_linear_solver_tao.hh:107-115`: reals through a default
/// `std::stringstream`, the reason as its integer).
#[test]
fn tao_not_converged_message() {
    let r = akantu_solver::NativeReport {
        reason: -2,
        max_iterations: 1,
        atol: 1e-9,
        rtol: 1e-8,
        stol: 0.0,
        ..Default::default()
    };
    assert_eq!(
        r.tao_not_converged_message(1),
        "The TAO solver did not converge after 1 iterations (reason: -2, max_iterations: 1, \
         atol: 1e-09, rtol: 1e-08, ttol: 0)"
    );
}
