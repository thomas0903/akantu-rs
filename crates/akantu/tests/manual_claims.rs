// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Named
// tests backing the statements of the user manual (docs/manual) about
// upstream behaviour. Each test cites the upstream source it was checked
// against (the code, not the upstream manual, is the oracle):
// src/io/parser/{parser.hh, input_file_parser.hh, algebraic_parser_x3.cc,
// algebraic_parser.hh, parameter_registry_tmpl.hh, parsable.cc},
// src/common/{aka_common.cc, aka_random_generator.hh, aka_array.hh},
// src/model/common/non_linear_solver/non_linear_solver_newton_raphson.cc,
// src/model/model_solver.cc, src/model/solid_mechanics/{material.cc,
// materials/material_thermal.cc, materials/plane_stress_toolbox_tmpl.hh,
// materials/material_elastic_orthotropic.cc,
// materials/material_elastic_linear_anisotropic.cc,
// solid_mechanics_model.cc, solid_mechanics_model_io.cc},
// src/io/mesh_io/mesh_io_msh.cc, src/mesh/group_manager.cc.

use std::path::{Path, PathBuf};

use akantu::core::enums::{
    AnalysisMethod, IntegrationSchemeType, MeshIoType, NonLinearSolverType, TimeStepSolverType,
};
use akantu::core::{Array, ElementType, GhostType};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::ModelOptions;
use akantu::parser::param::ParameterAccess;
use akantu::parser::{ParamValue, Parser, ParserType, RandomDistribution, SearchScope};
use akantu::solid::SolidMechanicsModel;

fn manual(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../docs/manual/src")
        .join(rel)
}

fn square() -> SharedMesh {
    let mut m = Mesh::new(2, "mesh").unwrap();
    akantu::io::read_mesh(&mut m, &manual("meshes/square.msh"), MeshIoType::Auto).unwrap();
    SharedMesh::new(m)
}

fn cube() -> SharedMesh {
    let mut m = Mesh::new(3, "mesh").unwrap();
    akantu::io::read_mesh(&mut m, &manual("meshes/cube.msh"), MeshIoType::Auto).unwrap();
    SharedMesh::new(m)
}

const STEEL: &str = "material elastic [\n name = steel\n rho = 7800\n E = 210e9\n nu = 0.3\n]\n";

fn model_on(
    mesh: SharedMesh,
    input: &str,
    method: AnalysisMethod,
) -> akantu::core::Result<SolidMechanicsModel> {
    let parser = Parser::parse_str(input, "input.dat")?;
    let mut m = SolidMechanicsModel::with_parser(mesh, None, "solid_mechanics_model", parser)?;
    m.init_full(&ModelOptions::new(method))?;
    Ok(m)
}

fn real(v: ParamValue) -> f64 {
    match v {
        ParamValue::Real(x) => x,
        other => panic!("not a real: {other:?}"),
    }
}

/* ---------------------------------------------------------------------- */
/* DOC-001: input-file grammar                                             */
/* ---------------------------------------------------------------------- */

fn scalar(text: &str) -> akantu::core::Result<f64> {
    let p = Parser::parse_str(&format!("x = {text}\n"), "in.dat")?;
    p.root.get_parameter_value::<f64>("x", SearchScope::Current)
}

/// `algebraic_parser_x3.cc:156-164` (`number_def`): functions and
/// constants are tried before parameter names and match a prefix, so a
/// parameter whose name starts with one cannot be referenced in a scalar
/// expression (input-files.md, "Algebraic expressions").
#[test]
fn doc001_names_starting_with_a_function_or_constant() {
    for (name, value) in [
        ("eta", "0.5"),
        ("e1", "2"),
        ("pi_x", "3"),
        ("cost", "4"),
        ("expo", "5"),
    ] {
        let p = Parser::parse_str(
            &format!(
                "{name} = {value}
x = 2 * {name}
"
            ),
            "in.dat",
        )
        .unwrap_or_else(|e| panic!("{name}: the file itself parses: {e}"));
        // defining it is fine, referencing it is a conversion error
        assert!(
            p.root
                .get_parameter_value::<f64>(name, SearchScope::Current)
                .is_ok(),
            "{name}"
        );
        assert!(
            p.root
                .get_parameter_value::<f64>("x", SearchScope::Current)
                .is_err(),
            "{name}: referencing it should fail"
        );
    }
    // the manual's example and its fix
    let p = Parser::parse_str(
        "eta = 0.5\ndamping = 2 * eta\nmy_eta = 0.5\ndamping_ok = 2 * my_eta\n",
        "in.dat",
    )
    .unwrap();
    assert!(
        p.root
            .get_parameter_value::<f64>("damping", SearchScope::Current)
            .is_err()
    );
    assert_eq!(
        p.root
            .get_parameter_value::<f64>("damping_ok", SearchScope::Current)
            .unwrap(),
        1.0
    );
}

/// `algebraic_parser_x3.cc:167-169`: `factor = number *("**" > number)`,
/// left-associative; there is no `^` operator (the upstream manual says
/// `^`). The sign belongs to the number (`-2**2 = 4`).
#[test]
fn doc001_power_operator_is_double_star() {
    assert_eq!(scalar("2**3").unwrap(), 8.0);
    assert_eq!(scalar("2**3**2").unwrap(), 64.0);
    assert_eq!(scalar("-2**2").unwrap(), 4.0);
    assert!(scalar("2^3").is_err());
}

/// `algebraic_parser_x3.cc:49-104`: the function and constant tables.
#[test]
fn doc001_scalar_functions_and_constants() {
    for f in [
        "abs", "acos", "asin", "atan", "ceil", "cos", "cosh", "exp", "floor", "log10", "log",
        "sin", "sinh", "sqrt", "tan", "tanh", "acosh", "asinh", "atanh", "exp2", "expm1", "log1p",
        "log2", "erf", "erfc", "lgamma", "tgamma", "trunc", "round",
    ] {
        // acosh needs x >= 1; every function accepts 1.5 except acos/asin/atanh
        let arg = if matches!(f, "acos" | "asin" | "atanh") {
            "0.5"
        } else {
            "1.5"
        };
        let v = scalar(&format!("{f}({arg})")).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert!(v.is_finite(), "{f}");
    }
    for f in ["pow", "min", "max", "atan2", "fmod", "hypot"] {
        let v = scalar(&format!("{f}(3, 4)")).unwrap_or_else(|e| panic!("{f}: {e}"));
        assert!(v.is_finite(), "{f}");
    }
    assert_eq!(scalar("sqrt(16)").unwrap(), 4.0);
    assert_eq!(scalar("max(2, 5)").unwrap(), 5.0);
    assert_eq!(scalar("hypot(3, 4)").unwrap(), 5.0);
    assert_eq!(scalar("pi").unwrap(), std::f64::consts::PI);
    assert_eq!(scalar("e").unwrap(), std::f64::consts::E);
}

/// `algebraic_parser.hh` (Qi vector grammar): constants are tried before
/// functions and unary functions before binary ones (UD-055), so `exp(…)`
/// and `atan2(…)` are rejected inside vectors, `COS(0)` accepted.
#[test]
fn doc001_vector_grammar_quirks() {
    let p = Parser::parse_str(
        "a = [exp(1)]\nb = [atan2(1, 1)]\nc = [COS(0), 2]\nd = [1, 23+2, 5]\n",
        "in.dat",
    )
    .unwrap();
    let v = |n: &str| {
        p.root
            .get_parameter_value::<Vec<f64>>(n, SearchScope::Current)
    };
    assert!(v("a").is_err());
    assert!(v("b").is_err());
    assert_eq!(v("c").unwrap(), [1.0, 2.0]);
    assert_eq!(v("d").unwrap(), [1.0, 25.0, 5.0]);
}

/// `input_file_parser.hh`: a value ends at the end of the line, `#`, `;`,
/// `=` or `\`; `\` continues it. The two-line matrix of the upstream
/// manual (`io.rst:70-71`) is therefore rejected; `;` is no separator.
#[test]
fn doc001_values_end_at_the_end_of_the_line() {
    let manual_example = "user parameters [\n  stress = [[1, 0],\n            [0, 1]]\n]\n";
    assert!(Parser::parse_str(manual_example, "in.dat").is_err());
    let continued = "user parameters [\n  stress = [[1, 0], \\\n            [0, 1]]\n]\n";
    let p = Parser::parse_str(continued, "in.dat").unwrap();
    let m = p
        .user_section()
        .unwrap()
        .get_parameter("stress", SearchScope::Current)
        .unwrap()
        .to_matrix()
        .unwrap();
    assert_eq!((m.rows(), m.cols(), m.at(1, 1)), (2, 2, 1.0));
    assert!(Parser::parse_str("a = 1; b = 2\n", "in.dat").is_err());
    // trailing comments are not part of the value
    let p = Parser::parse_str("a = 1 + 1 # two\n", "in.dat").unwrap();
    assert_eq!(
        p.root
            .get_parameter_value::<f64>("a", SearchScope::Current)
            .unwrap(),
        2.0
    );
}

/// `parser.hh:33-58`: the section types and their order.
#[test]
fn doc001_section_types_are_upstreams() {
    let upstream = [
        "cohesive_inserter",
        "contact",
        "embedded_interface",
        "friction",
        "global",
        "heat",
        "integration_scheme",
        "material",
        "phasefield",
        "constitutive_law",
        "mesh",
        "model",
        "model_solver",
        "neighborhood",
        "neighborhoods",
        "non_linear_solver",
        "non_local",
        "rules",
        "solver",
        "time_step_solver",
        "user",
        "weight_function",
        "contact_detector",
        "contact_resolution",
        "not_defined",
    ];
    let port: Vec<&str> = ParserType::ALL.iter().map(|t| t.name()).collect();
    assert_eq!(port, upstream);
}

/// Sections: case-insensitive type, optional name and option word,
/// anonymous sections, duplicate parameter names refused.
#[test]
fn doc001_sections() {
    let p = Parser::parse_str(
        "MATERIAL elastic opt [\n name = a\n]\nuser [\n x = 1\n]\n",
        "in.dat",
    )
    .unwrap();
    let mat = &p.root.sub_sections[0];
    assert_eq!(mat.section_type, ParserType::Material);
    assert_eq!((mat.name.as_str(), mat.option.as_str()), ("elastic", "opt"));
    assert!(p.root.sub_sections[1].name.starts_with("anonymous_"));
    assert!(Parser::parse_str("a = 1\na = 2\n", "in.dat").is_err());
}

/// `algebraic_parser_x3.cc:134` (`eval`): variables are parameters of the
/// current section or of its ancestors (`_ppsc_current_and_parent_scope`);
/// a sibling section is not visible. Cycles are an error (UD-080).
#[test]
fn doc001_variable_scope_and_cycles() {
    let p = Parser::parse_str(
        "g = 2\nuser a [\n x = 3\n]\nuser b [\n y = 2*g\n z = x\n]\nself = self\n",
        "in.dat",
    )
    .unwrap();
    let b = &p.root.sub_sections[1];
    assert_eq!(
        b.get_parameter_value::<f64>("y", SearchScope::Current)
            .unwrap(),
        4.0
    );
    assert!(
        b.get_parameter_value::<f64>("z", SearchScope::Current)
            .is_err()
    );
    assert!(
        p.root
            .get_parameter_value::<f64>("self", SearchScope::Current)
            .is_err()
    );
}

/// `aka_random_generator.hh:35-37`, `algebraic_parser.hh:466-478`: the
/// distribution names.
#[test]
fn doc001_random_distributions() {
    let cases = [
        ("uniform [1, 2]", RandomDistribution::Uniform(1.0, 2.0)),
        ("exponential [2]", RandomDistribution::Exponential(2.0)),
        ("gamma [1, 2]", RandomDistribution::Gamma(1.0, 2.0)),
        ("weibull [1, 2]", RandomDistribution::Weibull(1.0, 2.0)),
        (
            "extreme_value [1, 2]",
            RandomDistribution::ExtremeValue(1.0, 2.0),
        ),
        ("normal [1, 2]", RandomDistribution::Normal(1.0, 2.0)),
        ("lognormal [1, 2]", RandomDistribution::Lognormal(1.0, 2.0)),
        ("chi_squared [3]", RandomDistribution::ChiSquared(3.0)),
        ("cauchy [1, 2]", RandomDistribution::Cauchy(1.0, 2.0)),
        ("fisher_f [1, 2]", RandomDistribution::FisherF(1.0, 2.0)),
        ("student_t [3]", RandomDistribution::StudentT(3.0)),
    ];
    for (text, want) in cases {
        let p = Parser::parse_str(&format!("r = 10 {text}\n"), "in.dat").unwrap();
        let r = p
            .root
            .get_parameter("r", SearchScope::Current)
            .unwrap()
            .to_random()
            .unwrap();
        assert_eq!((r.base_value, r.distribution), (10.0, Some(want)), "{text}");
    }
}

/// `parser_input_files.cc:82` (`permissive_parser`), `aka_common.cc:113`
/// (`seed`, read from the root section only).
#[test]
fn doc001_global_parameters() {
    let p = Parser::parse_str("permissive_parser = true\nseed = 123456\n", "in.dat").unwrap();
    assert!(p.is_permissive());
    assert_eq!(
        p.root
            .get_parameter("seed", SearchScope::Current)
            .unwrap()
            .to_int::<i64>(),
        123_456
    );
    assert!(
        !Parser::parse_str("a = 1\n", "in.dat")
            .unwrap()
            .is_permissive()
    );
}

/* ---------------------------------------------------------------------- */
/* DOC-002: solver configuration                                           */
/* ---------------------------------------------------------------------- */

/// `non_linear_solver_newton_raphson.cc:68-84`: defaults and access flags.
#[test]
fn doc002_newton_raphson_parameters() {
    let m = model_on(square(), STEEL, AnalysisMethod::Static).unwrap();
    let nls = m.solver.non_linear_solver(None).unwrap();
    assert_eq!(nls.kind(), NonLinearSolverType::NewtonRaphson);
    assert_eq!(nls.get_param("threshold").unwrap(), ParamValue::Real(1e-10));
    assert_eq!(
        nls.get_param("max_iterations").unwrap(),
        ParamValue::Int(10)
    );
    assert_eq!(
        nls.get_param("convergence_type").unwrap(),
        ParamValue::Enum("solution".into())
    );
    assert_eq!(
        nls.get_param("force_linear_recompute").unwrap(),
        ParamValue::Bool(true)
    );
    assert_eq!(
        nls.get_param("verbose_iteration").unwrap(),
        ParamValue::Bool(false)
    );
    for p in ["threshold", "convergence_type", "max_iterations"] {
        assert_eq!(
            nls.param_info(p).unwrap().access,
            ParameterAccess::PARSMOD,
            "{p}"
        );
    }
    for p in ["force_linear_recompute", "verbose_iteration"] {
        assert_eq!(
            nls.param_info(p).unwrap().access,
            ParameterAccess::MODIFIABLE,
            "{p}"
        );
    }
    for p in ["error", "nb_iterations", "converged"] {
        assert_eq!(
            nls.param_info(p).unwrap().access,
            ParameterAccess::READABLE,
            "{p}"
        );
    }
}

/// One degree of freedom `m ü + k u = 0` (generic DOF family `u`).
struct Oscillator {
    m: f64,
    k: f64,
}

impl akantu::solver::SolverCallback for Oscillator {
    fn matrix_type(&self, id: &str) -> akantu::core::enums::MatrixType {
        if id == "C" {
            akantu::core::enums::MatrixType::NotDefined
        } else {
            akantu::core::enums::MatrixType::Symmetric
        }
    }
    fn assemble_matrix(
        &mut self,
        dm: &mut akantu::solver::DofManager,
        id: &str,
    ) -> akantu::core::Result<()> {
        let value = match id {
            "K" => self.k,
            "M" => self.m,
            _ => return Ok(()),
        };
        let a = dm.matrix_mut(id)?;
        a.zero();
        a.add(0, 0, value)
    }
    fn assemble_residual(
        &mut self,
        dm: &mut akantu::solver::DofManager,
    ) -> akantu::core::Result<()> {
        let u = dm.dofs("u")?.as_slice()[0];
        let r = Array::from_vec(vec![-self.k * u], 1)?;
        dm.assemble_to_residual("u", r.view(), 1.0)
    }
}

/// `(u, u̇, ü)` after one step of `scheme` from `(u0, v0)` with the
/// consistent `ü0 = −k u0 / m`.
fn newmark_step(
    scheme: IntegrationSchemeType,
    osc: &mut Oscillator,
    dt: f64,
    u0: f64,
    v0: f64,
) -> [f64; 3] {
    use akantu::comm::Communicator;
    use akantu::core::enums::SparseSolverType;
    use akantu::solver::{DofManager, ModelSolver, ModelSolverOptions, SolutionType};
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("u", 1, 1).unwrap();
    dm.register_dofs_derivative("u", 1).unwrap();
    dm.register_dofs_derivative("u", 2).unwrap();
    let mut ms = ModelSolver::new(dm);
    let options = ModelSolverOptions {
        time_step_solver_type: Some(TimeStepSolverType::Dynamic),
        non_linear_solver_type: Some(NonLinearSolverType::NewtonRaphson),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    };
    ms.get_new_solver("s", &options, osc).unwrap();
    ms.set_integration_scheme("s", "u", scheme, SolutionType::Displacement, osc)
        .unwrap();
    ms.set_time_step(dt, None).unwrap();
    ms.dof_manager.dofs_mut("u").unwrap().as_mut_slice()[0] = u0;
    ms.dof_manager
        .dofs_derivative_mut("u", 1)
        .unwrap()
        .as_mut_slice()[0] = v0;
    ms.dof_manager
        .dofs_derivative_mut("u", 2)
        .unwrap()
        .as_mut_slice()[0] = -osc.k * u0 / osc.m;
    ms.solve_step(osc, None).unwrap();
    let dm = &ms.dof_manager;
    [
        dm.dofs("u").unwrap().as_slice()[0],
        dm.dofs_derivative("u", 1).unwrap().as_slice()[0],
        dm.dofs_derivative("u", 2).unwrap().as_slice()[0],
    ]
}

/// Textbook Newmark step (β, γ) of `m ü + k u = 0`.
fn textbook_newmark(
    beta: f64,
    gamma: f64,
    osc: &Oscillator,
    dt: f64,
    u0: f64,
    v0: f64,
) -> [f64; 3] {
    let a0 = -osc.k * u0 / osc.m;
    let u_pred = u0 + dt * v0 + dt * dt * (0.5 - beta) * a0;
    let v_pred = v0 + dt * (1.0 - gamma) * a0;
    let a1 = -osc.k * u_pred / (osc.m + beta * dt * dt * osc.k);
    [u_pred + beta * dt * dt * a1, v_pred + gamma * dt * a1, a1]
}

/// UD-017, `newmark-beta.cc:64-66,120,150` and `newmark-beta.hh:159-177`:
/// Akantu's Newmark `alpha`/`beta` are not the textbook β/γ: textbook
/// β = `alpha·beta`, γ = `beta`. Checked on the presets' parameters and on
/// one step of each implicit preset against the closed-form textbook
/// update (`docs/manual/src/solver-configuration.md`).
#[test]
fn doc002_newmark_parameters_are_akantus() {
    use IntegrationSchemeType as T;
    use akantu::solver::new_integration_scheme;
    // (scheme, Akantu alpha, Akantu beta, textbook β)
    let presets = [
        (T::CentralDifference, 0.0, 0.5, 0.0),
        (T::FoxGoodwin, 1.0 / 6.0, 0.5, 1.0 / 12.0),
        (T::TrapezoidalRule2, 0.5, 0.5, 0.25),
        (T::LinearAcceleration, 1.0 / 3.0, 0.5, 1.0 / 6.0),
        (T::NewmarkBeta, 0.0, 0.0, 0.0),
    ];
    for (scheme, alpha, beta, textbook_beta) in presets {
        let s = new_integration_scheme(scheme, "u", false).unwrap();
        let a = real(s.get_param("alpha").unwrap());
        let b = real(s.get_param("beta").unwrap());
        assert_eq!((a, b), (alpha, beta), "{scheme:?}");
        // halving is exact: the products are the correctly rounded β
        assert_eq!(a * b, textbook_beta, "{scheme:?}");
    }
    // one step: m = 2, k = 50 (ω = 5), Δt = 0.1, u0 = 0.01, v0 = 0.3
    let (dt, u0, v0) = (0.1, 0.01, 0.3);
    for (scheme, alpha, beta, textbook_beta) in &presets[1..4] {
        let mut osc = Oscillator { m: 2.0, k: 50.0 };
        let port = newmark_step(*scheme, &mut osc, dt, u0, v0);
        let want = textbook_newmark(*textbook_beta, *beta, &osc, dt, u0, v0);
        // a 1 × 1 linear system solved by one Newton iteration in a
        // different operation order: a few ulps (bound justified in
        // docs/NUMERICAL_VALIDATION.md)
        let rel: Vec<f64> = port
            .iter()
            .zip(want)
            .map(|(p, w)| ((p - w) / w).abs())
            .collect();
        println!("{scheme:?}: port {port:?}, textbook {want:?}, relative differences {rel:?}");
        for (i, (p, w)) in port.iter().zip(want).enumerate() {
            assert!(
                (p - w).abs() <= 1e-13 * w.abs(),
                "{scheme:?} component {i}: port {p:e}, textbook {w:e}"
            );
        }
        // the misreading "Akantu alpha = textbook β" gives another step
        let misread = textbook_newmark(*alpha, *beta, &osc, dt, u0, v0);
        assert!(
            (port[0] - misread[0]).abs() > 1e-6 * misread[0].abs(),
            "{scheme:?}: the test does not distinguish β = alpha"
        );
    }
}

/// A model section with a static solver and the material (materials are
/// read from the model section when the file has one).
fn model_section(body: &str) -> String {
    format!("model solid_mechanics_model [\n{body}\n{STEEL}]\n")
}

fn solver_section(nls_body: &str) -> String {
    model_section(&format!(
        " time_step_solver static [\n  non_linear_solver newton_raphson [\n{nls_body}\n  ]\n ]"
    ))
}

fn error_of(input: &str, method: AnalysisMethod) -> String {
    match model_on(square(), input, method) {
        Ok(_) => panic!("accepted: {input}"),
        Err(e) => e.to_string(),
    }
}

/// `parameter_registry_tmpl.hh:114-119` (`setAuto` requires `_pat_parsable`)
/// and `parsable.cc:49-62` (permissive covers unknown names only): the
/// upstream manual's `force_linear_recompute` / `verbose_iteration`
/// input-file parameters are rejected.
#[test]
fn doc002_modifiable_parameters_are_not_parsable() {
    for p in ["force_linear_recompute = false", "verbose_iteration = true"] {
        for permissive in ["", "permissive_parser = true\n"] {
            let input = format!("{permissive}{}", solver_section(p));
            let e = error_of(&input, AnalysisMethod::Static);
            assert!(e.contains("is not parsable"), "{p}: {e}");
        }
    }
    // the parsable ones are accepted
    let input = solver_section("threshold = 1e-8\nmax_iterations = 3\nconvergence_type = residual");
    let m = model_on(square(), &input, AnalysisMethod::Static).unwrap();
    let nls = m.solver.non_linear_solver(Some("static")).unwrap();
    assert_eq!(nls.get_param("max_iterations").unwrap(), ParamValue::Int(3));
    // unknown names: error, warning when permissive
    let input = solver_section("bogus = 1");
    let e = error_of(&input, AnalysisMethod::Static);
    assert!(e.contains("bogus"), "{e}");
    // upstream (`parsable.cc:49-62`) only warns about an unknown name when
    // the file sets `permissive_parser`: the model is built and the known
    // parameters of the section are still applied
    let input = format!(
        "permissive_parser = true\n{}",
        solver_section("bogus = 1\nmax_iterations = 4")
    );
    let m = model_on(square(), &input, AnalysisMethod::Static).unwrap();
    let nls = m.solver.non_linear_solver(Some("static")).unwrap();
    assert_eq!(nls.get_param("max_iterations").unwrap(), ParamValue::Int(4));
}

/// `model.cc` / `solid_mechanics_model.cc` (`initDOFManager`): only the
/// `default` DOF manager exists without PETSc.
#[test]
fn doc002_dof_manager_type() {
    let ok = model_section(" dof_manager_type = default");
    assert!(model_on(square(), &ok, AnalysisMethod::Static).is_ok());
    let e = error_of(
        &model_section(" dof_manager_type = petsc"),
        AnalysisMethod::Static,
    );
    // upstream's message of a build without PETSc, quoted by the page
    assert!(
        e.contains("To use the solver petsc you will have to code it"),
        "{e}"
    );
}

/// `model_solver.cc` (`initDOFManager(section)`): `default_solver` must
/// name a created solver; an integration scheme needs a DOF name; at most
/// one non-linear solver per time-step solver.
#[test]
fn doc002_section_errors() {
    let e = error_of(
        &model_section(" default_solver = nope"),
        AnalysisMethod::Static,
    );
    assert!(e.contains("nope"), "{e}");
    let unnamed = model_section(
        " time_step_solver dynamic [\n  integration_scheme trapezoidal_rule_2 [\n  ]\n ]",
    );
    let e = error_of(&unnamed, AnalysisMethod::ImplicitDynamic);
    assert!(e.contains("No degree of freedom name"), "{e}");
    let two = model_section(
        " time_step_solver static [\n  non_linear_solver newton_raphson [\n  ]\n  non_linear_solver linear [\n  ]\n ]",
    );
    let e = error_of(&two, AnalysisMethod::Static);
    assert!(e.contains("More than one non linear solver"), "{e}");
}

/// `solid_mechanics_model.cc` (`getDefaultSolverID`,
/// `getDefaultSolverOptions`): default solver per analysis method.
#[test]
fn doc002_default_solvers() {
    let cases = [
        (
            AnalysisMethod::Static,
            "static",
            TimeStepSolverType::Static,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::PseudoTime,
        ),
        (
            AnalysisMethod::ImplicitDynamic,
            "implicit",
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::TrapezoidalRule2,
        ),
        (
            AnalysisMethod::ExplicitLumpedMass,
            "explicit_lumped",
            TimeStepSolverType::DynamicLumped,
            NonLinearSolverType::Lumped,
            IntegrationSchemeType::CentralDifference,
        ),
        (
            AnalysisMethod::ExplicitConsistentMass,
            "explicit",
            TimeStepSolverType::Dynamic,
            NonLinearSolverType::NewtonRaphson,
            IntegrationSchemeType::CentralDifference,
        ),
    ];
    for (method, id, tss, nls, scheme) in cases {
        let m = model_on(square(), STEEL, method).unwrap();
        let s = m.solver.solver(None).unwrap();
        assert_eq!(s.id, id, "{method:?}");
        assert_eq!(s.kind, tss, "{method:?}");
        assert_eq!(s.non_linear_solver.kind(), nls, "{method:?}");
        assert_eq!(
            s.integration_scheme("displacement").unwrap().kind(),
            scheme,
            "{method:?}"
        );
    }
}

/// `model.cc` (`getParserSection`): a section with `name = x` belongs to
/// the model with id `x` only; without `name` to every model of the type.
#[test]
fn doc002_model_section_name() {
    // another model's section: ignored (its invalid default_solver too),
    // the top-level material is used
    let other =
        format!("model solid_mechanics_model [\n name = other\n default_solver = nope\n]\n{STEEL}");
    let m = model_on(square(), &other, AnalysisMethod::Static).unwrap();
    assert!(m.material_by_name("steel").is_ok());
    let mine = model_section(" name = solid_mechanics_model\n default_solver = nope");
    let e = error_of(&mine, AnalysisMethod::Static);
    assert!(e.contains("nope"), "{e}");
}

/// UD-194: `residual_mass_wgh` is refused by default.
#[test]
fn doc002_residual_mass_wgh_is_refused() {
    use akantu::core::enums::SpatialDirection::X;
    use akantu::model::boundary_condition::FixedValue;
    let input = solver_section("convergence_type = residual_mass_wgh");
    let mut m = model_on(square(), &input, AnalysisMethod::Static).unwrap();
    m.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: X,
        },
        "left",
    )
    .unwrap();
    let e = m.solve_step(None).unwrap_err();
    assert!(matches!(e, akantu::core::Error::NotImplemented(_)), "{e:?}");
}

/* ---------------------------------------------------------------------- */
/* DOC-003: material sections                                              */
/* ---------------------------------------------------------------------- */

/// `constitutive_laws_handler_tmpl.hh`: a material without `name` is an
/// error; materials come from the model section when there is one.
#[test]
fn doc003_material_sections() {
    assert!(
        model_on(
            square(),
            "material elastic [\n rho = 1\n E = 1\n nu = 0.3\n]\n",
            AnalysisMethod::Static
        )
        .is_err()
    );
    let both = format!(
        "model solid_mechanics_model [\n material elastic [\n  name = inside\n  rho = 1\n  E = 1\n  nu = 0.3\n ]\n]\n{STEEL}"
    );
    let m = model_on(square(), &both, AnalysisMethod::Static).unwrap();
    assert_eq!(m.nb_materials(), 1);
    assert!(m.material_by_name("inside").is_ok());
    assert!(m.material_by_name("steel").is_err());
    let top = model_on(square(), STEEL, AnalysisMethod::Static).unwrap();
    assert!(top.material_by_name("steel").is_ok());
}

/// `material.cc:44-51`, `material_thermal.cc:38-45`,
/// `plane_stress_toolbox_tmpl.hh:44`: defaults of the elastic law.
#[test]
fn doc003_elastic_defaults() {
    let m = model_on(
        square(),
        "material elastic [\n name = m\n]\n",
        AnalysisMethod::Static,
    )
    .unwrap();
    let law = m.material(0).unwrap();
    assert_eq!(law.get_param("rho").unwrap(), ParamValue::Real(0.0));
    assert_eq!(law.get_param("E").unwrap(), ParamValue::Real(0.0));
    assert_eq!(law.get_param("nu").unwrap(), ParamValue::Real(0.5));
    assert_eq!(law.get_param("alpha").unwrap(), ParamValue::Real(0.0));
    assert_eq!(law.get_param("delta_T").unwrap(), ParamValue::Real(0.0));
    assert_eq!(
        law.get_param("Plane_Stress").unwrap(),
        ParamValue::Bool(false)
    );
    assert_eq!(
        law.get_param("finite_deformation").unwrap(),
        ParamValue::Bool(false)
    );
    // derived parameters are readable only
    let m = model_on(square(), STEEL, AnalysisMethod::Static).unwrap();
    let law = m.material(0).unwrap();
    let mu = real(law.get_param("mu").unwrap());
    assert!((mu - 210e9 / 2.6).abs() <= 1e-6 * mu);
    assert_eq!(
        law.param_info("mu").unwrap().access,
        ParameterAccess::READABLE
    );
}

/// `material_elastic_orthotropic.cc:36-51`,
/// `material_elastic_linear_anisotropic.cc:40-60`: moduli default to 0,
/// axes to the global axes.
#[test]
fn doc003_anisotropic_defaults() {
    let m = model_on(
        cube(),
        "material elastic_orthotropic [\n name = o\n rho = 1\n E1 = 1\n E2 = 1\n E3 = 1\n \
         nu12 = 0.1\n nu13 = 0.1\n nu23 = 0.1\n G12 = 1\n G13 = 1\n G23 = 1\n]\n\
         material elastic_anisotropic [\n name = a\n rho = 1\n C11 = 2\n C22 = 2\n C33 = 2\n \
         C44 = 1\n C55 = 1\n C66 = 1\n]\n",
        AnalysisMethod::Static,
    )
    .unwrap();
    let a = m.material_by_name("a").unwrap();
    assert_eq!(a.get_param("C12").unwrap(), ParamValue::Real(0.0));
    for (n, axis) in [
        ("n1", [1.0, 0.0, 0.0]),
        ("n2", [0.0, 1.0, 0.0]),
        ("n3", [0.0, 0.0, 1.0]),
    ] {
        assert_eq!(
            a.get_param(n).unwrap(),
            ParamValue::VectorReal(axis.to_vec()),
            "{n}"
        );
    }
    let o = m.material_by_name("o").unwrap();
    assert_eq!(o.get_param("E3").unwrap(), ParamValue::Real(1.0));
    // 2-D orthotropic laws have no third modulus
    assert!(
        model_on(
            square(),
            "material elastic_orthotropic [\n name = o\n rho = 1\n E1 = 1\n E2 = 1\n E3 = 1\n]\n",
            AnalysisMethod::Static
        )
        .is_err()
    );
}

/// `parsable.cc:49-62`: unknown material parameters are errors, warnings
/// when permissive; not-parsable ones are errors in both modes.
#[test]
fn doc003_unknown_and_readonly_parameters() {
    let unknown = "material elastic [\n name = a\n E = 1\n nu = 0.3\n Young = 1\n]\n";
    assert!(model_on(square(), unknown, AnalysisMethod::Static).is_err());
    let permissive = format!("permissive_parser = true\n{unknown}");
    assert!(model_on(square(), &permissive, AnalysisMethod::Static).is_ok());
    let readonly =
        "permissive_parser = true\nmaterial elastic [\n name = a\n E = 1\n nu = 0.3\n mu = 1\n]\n";
    assert!(model_on(square(), readonly, AnalysisMethod::Static).is_err());
}

/* ---------------------------------------------------------------------- */
/* DOC-004: command-line and environment options                           */
/* ---------------------------------------------------------------------- */

/// `aka_common.cc:73-91`: the four options, their types and defaults, and
/// `_remove_parsed`.
#[test]
fn doc004_akantu_options() {
    use akantu::parser::cli::{
        ArgumentParser, DEFAULT_DEBUG_LEVEL, ParseFlags, ParseOutcome, register_akantu_options,
    };
    let mut p = ArgumentParser::new();
    register_akantu_options(&mut p).unwrap();
    let mut argv: Vec<String> = [
        "prog",
        "-x",
        "--aka_print_backtrace",
        "--aka_debug_level",
        "7",
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let out = p
        .parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)
        .unwrap();
    assert_eq!(out, ParseOutcome::Parsed);
    assert_eq!(argv, ["prog", "-x"]);
    assert_eq!(p.get_int("aka_debug_level").unwrap(), 7);
    assert!(p.get_bool("aka_print_backtrace").unwrap());
    assert_eq!(p.get_string("aka_input_file").unwrap(), "");
    assert!(!p.has("aka_seed"));
    assert_eq!(DEFAULT_DEBUG_LEVEL, 4); // dblWarning

    let mut q = ArgumentParser::new();
    register_akantu_options(&mut q).unwrap();
    let mut argv = vec!["prog".to_string(), "--help".to_string()];
    assert_eq!(
        q.parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)
            .unwrap(),
        ParseOutcome::HelpRequested
    );
    let mut bad = ArgumentParser::new();
    register_akantu_options(&mut bad).unwrap();
    let mut argv = vec!["prog".to_string(), "--aka_seed".to_string()];
    assert!(
        bad.parse_argv(&mut argv, ParseFlags::REMOVE_PARSED, true)
            .is_err()
    );
}

/// `aka_common.cc:104-117`: `AKA_SEED` > `--aka_seed` > input `seed` >
/// time, times `rank + 1`.
#[test]
fn doc004_seed_policy() {
    use akantu::core::random::initial_seed;
    assert_eq!(initial_seed(Some("7"), Some(20), Some(30), 40, 0), 7);
    assert_eq!(initial_seed(Some("7"), None, None, 40, 3), 28);
    assert_eq!(initial_seed(None, Some(20), Some(30), 40, 1), 40);
    assert_eq!(initial_seed(None, None, Some(30), 40, 0), 30);
    assert_eq!(initial_seed(None, None, None, 40, 0), 40);
    assert_eq!(initial_seed(Some(" 12abc"), None, None, 0, 0), 12); // atol
}

/* ---------------------------------------------------------------------- */
/* DOC-005: meshes                                                         */
/* ---------------------------------------------------------------------- */

/// `mesh_io_msh.cc:395-409, 913-916`, `group_manager.cc:128`: Gmsh tags,
/// physical names, groups and their `<name>_nodes` node groups.
#[test]
fn doc005_gmsh_mesh_data_and_groups() {
    let mesh = square();
    let m = mesh.read().unwrap();
    let ty = ElementType::Quadrangle4;
    assert_eq!(
        m.data()
            .ints("tag_0")
            .unwrap()
            .get(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        [5, 5, 5, 5]
    );
    assert_eq!(
        m.data()
            .ints("tag_1")
            .unwrap()
            .get(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice(),
        [10, 10, 10, 10]
    );
    assert_eq!(
        m.data()
            .strings("physical_names")
            .unwrap()
            .get(ty, GhostType::NotGhost)
            .unwrap()
            .as_slice()[0],
        "body"
    );
    for g in ["left", "right", "bottom", "top", "body"] {
        assert!(m.groups().element_group_exists(g), "{g}");
        assert!(m.groups().node_group_exists(&format!("{g}_nodes")), "{g}");
    }
}

/// D-027 / UD-049: no Abaqus reader; binary MSH refused; format detection
/// by extension (`mesh_io.cc`, `getMeshIO`).
#[test]
fn doc005_formats() {
    use akantu::io::{ReadOptions, detect_format, read_mesh_from_str};
    let mut m = Mesh::new(2, "m").unwrap();
    assert!(
        read_mesh_from_str(
            &mut m,
            "a.inp",
            "",
            MeshIoType::Abaqus,
            &ReadOptions::default()
        )
        .is_err()
    );
    let binary = "$MeshFormat\n2.2 1 8\n$EndMeshFormat\n";
    let mut m = Mesh::new(2, "m").unwrap();
    assert!(matches!(
        read_mesh_from_str(
            &mut m,
            "b.msh",
            binary,
            MeshIoType::Gmsh,
            &ReadOptions::default()
        ),
        Err(akantu::core::Error::Unsupported(_))
    ));
    assert_eq!(detect_format(Path::new("x.msh")).unwrap(), MeshIoType::Gmsh);
    assert_eq!(
        detect_format(Path::new("x.diana")).unwrap(),
        MeshIoType::Diana
    );
    assert!(detect_format(Path::new("x.inp")).is_err());
}

/* ---------------------------------------------------------------------- */
/* DOC-006 / DOC-008: dumpers                                              */
/* ---------------------------------------------------------------------- */

fn scratch(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "akantu-manual-claims-{name}-{}",
        std::process::id()
    ));
    let _ = std::fs::remove_dir_all(&d);
    d
}

/// `solid_mechanics_model_io.cc:134-173` and `createElementalField`: every
/// field name of the manual's table is accepted and dumped (explicit
/// analysis, after one step so that the lumped mass exists).
#[test]
fn doc006_smm_field_names() {
    let mut m = model_on(square(), STEEL, AnalysisMethod::ExplicitLumpedMass).unwrap();
    let dt = m.stable_time_step().unwrap();
    m.set_time_step(0.5 * dt, None).unwrap();
    m.solve_step(None).unwrap();
    let dir = scratch("fields");
    m.set_directory(&dir).unwrap();
    for f in [
        "displacement",
        "velocity",
        "acceleration",
        "external_force",
        "internal_force",
        "mass",
        "increment",
        "blocked_dofs",
        "partitions",
        "material_index",
        "stress",
        "grad_u",
        "eigen_grad_u",
        "strain",
        "Green strain",
        "principal strain",
        "principal Green strain",
        "Von Mises stress",
        "potential_energy",
        "sigma_th",
        "tag_0",
    ] {
        m.add_dump_field(f).unwrap_or_else(|e| panic!("{f}: {e}"));
    }
    m.add_dump_field_vector("displacement").ok(); // already registered: either way
    m.dump(None, None).unwrap();
    let pvtu = std::fs::read_to_string(dir.join("solid_mechanics_model_0000.pvtu")).unwrap();
    for f in [
        "displacement",
        "mass",
        "Von Mises stress",
        "material_index",
        "tag_0",
    ] {
        assert!(pvtu.contains(&format!("Name=\"{f}\"")), "{f}");
    }
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `solid_mechanics_model_io.cc:143-148`: `force` and `residual` raise.
#[test]
fn doc006_renamed_fields() {
    let mut m = model_on(square(), STEEL, AnalysisMethod::Static).unwrap();
    let e = m.add_dump_field("force").unwrap_err().to_string();
    assert!(e.contains("renamed in 'external_force'"), "{e}");
    let e = m.add_dump_field("residual").unwrap_err().to_string();
    assert!(e.contains("replaced by 'internal_force'"), "{e}");
    // string mesh data has no VTK representation and an unknown name can
    // never be resolved: both refused at registration, as upstream
    // (UD-683)
    assert!(m.add_dump_field("physical_names").is_err());
    assert!(m.add_dump_field("no_such_field").is_err());
    // a known model array is accepted (read when the model dumps)
    m.add_dump_field("mass").unwrap();
}

/// Default dumper: ParaView, base name = model id, `.pvd` once the time
/// step is known (upstream `setTimeStep` forwards it to the dumper).
#[test]
fn doc006_default_dumper_layout() {
    let mut m = model_on(square(), STEEL, AnalysisMethod::ExplicitLumpedMass).unwrap();
    let dir = scratch("layout");
    m.set_directory(&dir).unwrap();
    m.add_dump_field("displacement").unwrap();
    m.dump(None, None).unwrap();
    assert!(dir.join("solid_mechanics_model_0000.pvtu").exists());
    assert!(
        dir.join("solid_mechanics_model-VTU/solid_mechanics_model_0000.vtu")
            .exists()
    );
    assert!(!dir.join("solid_mechanics_model.pvd").exists());
    m.set_time_step(1e-7, None).unwrap();
    m.dump(None, None).unwrap();
    assert!(dir.join("solid_mechanics_model.pvd").exists());
    std::fs::remove_dir_all(&dir).unwrap();
}

/// `dumper_iohelper_paraview.hh` (`directory = "./paraview"`) and
/// `solid_mechanics_model.cc` (default dumper named after the model id):
/// without `set_directory` the files go to `./paraview` relative to the
/// working directory. The working directory is process-global, so the
/// test runs itself in a child process inside a scratch directory.
#[test]
fn doc006_default_dump_directory() {
    const CHILD: &str = "AKANTU_MANUAL_CLAIMS_DUMP_CHILD";
    if let Some(dir) = std::env::var_os(CHILD) {
        // child: dump from inside the scratch directory
        std::env::set_current_dir(&dir).unwrap();
        let mut m = model_on(square(), STEEL, AnalysisMethod::ExplicitLumpedMass).unwrap();
        m.add_dump_field("displacement").unwrap();
        m.dump(None, None).unwrap();
        return;
    }
    let dir = scratch("default-directory");
    std::fs::create_dir_all(&dir).unwrap();
    let out = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "doc006_default_dump_directory",
            "--exact",
            "--test-threads=1",
        ])
        .env(CHILD, &dir)
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "child test failed: {}\n{}{}",
        out.status,
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(
        dir.join("paraview/solid_mechanics_model_0000.pvtu")
            .exists()
    );
    assert!(
        dir.join("paraview/solid_mechanics_model-VTU/solid_mechanics_model_0000.vtu")
            .exists()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/* ---------------------------------------------------------------------- */
/* DOC-009: solid mechanics model                                          */
/* ---------------------------------------------------------------------- */

/// `solid_mechanics_model.cc:642-699`: `getEnergy(id)` and
/// `getEnergy(id, group)` are reduced over the ranks, `getEnergy(id,
/// element)` is local (solid-mechanics.md, "Energies"). Two in-process
/// ranks, static stretch of the unit square.
#[test]
fn doc009_energy_reductions() {
    use akantu::comm::{Communicator, run_local};
    use akantu::core::enums::SpatialDirection::{X, Y};
    use akantu::core::{Element, ElementType as E};
    use akantu::mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
    use akantu::model::boundary_condition::FixedValue;
    let run = |comm: Communicator| -> akantu::core::Result<(f64, f64, f64)> {
        let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone())?;
        if comm.rank() == 0 {
            akantu::io::read_mesh(&mut mesh, &manual("meshes/square.msh"), MeshIoType::Auto)?;
        }
        let mut partitioner = RcbPartitioner;
        // no consumer is registered yet: the returned events are not needed
        let _events = distribute_with(
            &mut mesh,
            Some(&mut partitioner as &mut dyn MeshPartitioner),
            0,
        )?;
        let nb_local = mesh.nb_element(E::Quadrangle4, GhostType::NotGhost);
        let parser = Parser::parse_str(STEEL, "material.dat")?;
        let mut m = SolidMechanicsModel::with_parser(SharedMesh::new(mesh), None, "smm", parser)?;
        m.init_full(&ModelOptions::new(AnalysisMethod::Static))?;
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: X,
            },
            "left",
        )?;
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: Y,
            },
            "bottom",
        )?;
        m.apply_dirichlet_bc(
            &FixedValue {
                value: 0.01,
                axis: X,
            },
            "right",
        )?;
        m.solve_step(None)?;
        let mut local = 0.0;
        for e in 0..nb_local {
            let el = Element::try_new(E::Quadrangle4, e, GhostType::NotGhost)?;
            local += m.energy_of_element("potential", el)?;
        }
        Ok((
            m.energy("potential")?,
            m.energy_of_group("potential", "body")?,
            local,
        ))
    };
    let results: Vec<_> = run_local(2, std::time::Duration::from_secs(60), run)
        .into_iter()
        .collect::<Result<_, _>>()
        .unwrap();
    let exact = 0.5 * 210e9 / 0.91 * 1e-4; // plane strain, ε = 1 %, V = 1
    let local_sum: f64 = results.iter().map(|r| r.2).sum();
    for (total, group, local) in &results {
        // reduced: every rank gets the global value
        assert!((total - exact).abs() <= 1e-12 * exact, "{total} vs {exact}");
        assert!((group - exact).abs() <= 1e-12 * exact, "{group} vs {exact}");
        // per element: local values only (both ranks own elements)
        assert!(*local > 0.0 && *local < 0.9 * exact, "{local} vs {exact}");
    }
    assert!((local_sum - exact).abs() <= 1e-12 * exact);
}

/* ---------------------------------------------------------------------- */
/* DOC-017: basic types                                                    */
/* ---------------------------------------------------------------------- */

/// `aka_array.hh:329-342`, `aka_array_tmpl.hh:429-443`: `clear` empties,
/// `zero` zeroes, `erase` moves the last tuple into the hole.
#[test]
fn doc017_array_semantics() {
    let mut a = Array::from_vec(vec![1, 2, 3, 4, 5, 6], 2).unwrap();
    a.swap_remove_tuple(0).unwrap();
    assert_eq!(a.as_slice(), [5, 6, 3, 4]);
    a.zero();
    assert_eq!((a.len(), a.as_slice()), (2, &[0, 0, 0, 0][..]));
    a.clear();
    assert!(a.is_empty());
}

/// UD-042 and D-061: `find` semantics (basic-types.md). The index of the
/// first match with upstream's comparison: `==` for the generic
/// `Array<T>::find` (`aka_array_tmpl.hh:639-645`), `|a − v| ≤ ε` (absolute,
/// `ε = DBL_EPSILON`) for `Array<Real>` (`aka_array.cc:35-44`, which
/// returns `size − index`: `find_upstream`); an error for a non-empty array
/// with several components (upstream's scalar iterator throws,
/// `aka_array_tmpl.hh:832-840`).
#[test]
fn doc017_array_find_semantics() {
    let ints = Array::from_vec(vec![7, 8, 9], 1).unwrap();
    assert_eq!(ints.find(&9).unwrap(), Some(2)); // generic upstream: index 2 too
    assert_eq!(ints.find_upstream(&9).unwrap(), 2);
    assert_eq!(ints.find(&7).unwrap(), Some(0));
    assert_eq!(ints.find(&42).unwrap(), None);
    assert_eq!(ints.find_upstream(&42).unwrap(), -1);
    let reals = Array::from_vec(vec![1.0, 2.0, 0.1 + 0.2, 3.0], 1).unwrap();
    assert_eq!(reals.find(&2.0).unwrap(), Some(1));
    assert_eq!(reals.find_upstream(&2.0).unwrap(), 3); // upstream: size − index
    // |(0.1 + 0.2) − 0.3| = 5.6e-17 ≤ DBL_EPSILON: found, as upstream (at
    // distance 2 from the end there)
    assert!(((0.1 + 0.2) - 0.3_f64).abs() <= f64::EPSILON);
    assert_eq!(reals.find(&0.3).unwrap(), Some(2));
    assert_eq!(reals.find_upstream(&0.3).unwrap(), 2);
    assert_eq!(reals.find_real(0.3), Some(2)); // shorthand of `find`
    // the bound is absolute: 1e-300 matches 0, 1 + 2ε does not match 1
    let near = Array::from_vec(vec![0.0, 1.0 + 2.0 * f64::EPSILON], 1).unwrap();
    assert_eq!(near.find(&1e-300).unwrap(), Some(0));
    assert_eq!(near.find(&1.0).unwrap(), None);
    // ±0 are equal; NaN and ∞ are never found (`∞ − ∞` is NaN)
    let specials = Array::from_vec(vec![-0.0, f64::NAN, f64::INFINITY], 1).unwrap();
    assert_eq!(specials.find(&0.0).unwrap(), Some(0));
    assert_eq!(specials.find(&f64::NAN).unwrap(), None);
    assert_eq!(specials.find(&f64::INFINITY).unwrap(), None);
    // several components: an error (upstream throws), except on an empty
    // array (upstream's check `nb_component · size == size` holds);
    // `find_tuple` for tuples
    let pairs = Array::from_vec(vec![1, 2, 3, 4], 2).unwrap();
    assert!(pairs.find(&3).is_err());
    assert_eq!(Array::<i32>::new(2).find(&3).unwrap(), None);
    assert_eq!(pairs.find_tuple(&[3, 4]), Some(1));
    assert_eq!(pairs.find_tuple(&[3]), None);
}
