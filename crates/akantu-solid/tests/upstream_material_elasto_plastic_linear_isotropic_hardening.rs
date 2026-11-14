// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_material_elasto_plastic_linear_isotropic_hardening/{
// test_material_elasto_plastic_linear_isotropic_hardening.cc,
// test_material_elasto_plastic_linear_isotropic_hardening.dat,
// test_material_elasto_plastic_linear_isotropic_hardening.geo,
// test_material_standard_linear_isotropic_hardening.verified} (registered as
// `test_material_standard_linear_isotropic_hardening`).
//! Port of upstream `test_material_standard_linear_isotropic_hardening`
//! (TEST-146): a 10 × 1 plane-stress plate (frozen Gmsh mesh of the upstream
//! `add_mesh` target, D-009) of `plastic_linear_isotropic_hardening`
//! (E = 100, ν = 0, σ_y = 10, h = 0), `initFull(_static)`, Newton–Raphson
//! `max_iterations = 300`, `threshold = 1e-5` (criterion: the solver's
//! default), `left` blocked in x, `bottom` in y; for `i = 0..20` the `right`
//! edge is set to `u_x = 0.1 i`, one `solveStep()`, and the line
//! `ε_xx,reaction` is printed with `std::cout.precision(4)` (`%.4g`), where
//! `ε_xx = 0.1 i / 10` and `reaction = −Σ f_int,x` over the `right` node
//! group. The upstream driver compares that output with
//! `test_material_standard_linear_isotropic_hardening.verified` through
//! `diff -biw` (`cmake/akantu_test_driver.sh:158`): same here, line by line,
//! whitespace and case ignored.
//!
//! The `.verified` file is upstream's plane-stress J2 return (UD-137,
//! category B): the 3-D plastic multiplier with an in-plane-truncated flow,
//! off the yield surface, so the perfectly plastic plate carries 10.22
//! instead of `σ_y·L_y = 10`. It is reproduced with `LawOptions::upstream()`.
//! The port's corrected default (consistent plane-stress return) must print
//! the analytical response `min(E ε, σ_y)·L_y` in the same format
//! (independent check, same 4-digit criterion) up to the first plastic step,
//! where the perfectly plastic plate stops converging (UD-732); with a
//! hardening modulus `h = 10` the same driver converges at every step and
//! the plastic return is compared with the analytical linear-hardening
//! response.

mod model_common;

use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, SpatialDirection};
use akantu_core::enums::{NonLinearSolverType, SparseSolverType};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_parser::ParamValue;
use akantu_solid::material::LawOptions;
use akantu_solver::ModelSolverOptions;
use akantu_validation::fixtures_root;
use model_common::{assert_bits, case, i64s, model_with, read_mesh};

/// `test_material_elasto_plastic_linear_isotropic_hardening.dat` (verbatim).
const MATERIAL: &str = "material plastic_linear_isotropic_hardening [\n\
\t name = hardening\n\
\t rho = 1000  # density\n\
\t E   = 100  # young's modulus\n\
\t nu  = 0.0   # poisson's ratio\n\
\t h = 0     # hardening modulus\n\
\t sigma_y = 10  # yielding stress\n\
   \t Plane_Stress = true\n\
]\n";

const VERIFIED: &str =
    include_str!("data/upstream/test_material_standard_linear_isotropic_hardening.verified");

/// C++ `std::ostream << double` with `precision(p)` and the default
/// floatfield, i.e. `printf("%.{p}g")` (finite values).
fn format_g(x: Real, p: usize) -> String {
    assert!(x.is_finite() && p > 0);
    if x == 0. {
        return if x.is_sign_negative() { "-0" } else { "0" }.into();
    }
    // exponent of the value rounded to p significant digits
    let sci = format!("{:.*e}", p - 1, x);
    let (mantissa, exp) = sci.split_once('e').unwrap();
    let exp: i32 = exp.parse().unwrap();
    let strip = |s: &str| -> String {
        if s.contains('.') {
            s.trim_end_matches('0').trim_end_matches('.').to_owned()
        } else {
            s.to_owned()
        }
    };
    if exp < -4 || exp >= p as i32 {
        let sign = if exp < 0 { '-' } else { '+' };
        format!("{}e{sign}{:02}", strip(mantissa), exp.unsigned_abs())
    } else {
        strip(&format!("{:.*}", (p as i32 - 1 - exp) as usize, x))
    }
}

/// `diff -biw` equality: lines compared without whitespace, case folded.
fn same_text_ignoring_whitespace(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase()
            })
            .collect()
    };
    norm(a) == norm(b)
}

/// Outcome of the upstream program.
struct Output {
    /// Standard output (one `ε_xx,reaction` line per converged step).
    text: String,
    /// Reactions of the converged steps.
    reactions: Vec<Real>,
    /// Newton iterations of the converged steps.
    iterations: Vec<usize>,
    /// Newton errors of the converged steps.
    errors: Vec<Real>,
    /// First step whose `solveStep()` failed (upstream rethrows: the test
    /// fails) with the solver's error.
    failed: Option<Failure>,
    /// Final displacement and internal force.
    final_state: Vec<(&'static str, Vec<Real>)>,
}

/// A `solveStep()` that failed.
#[derive(Debug)]
struct Failure {
    /// Load step (`i` of the upstream loop).
    step: usize,
    /// The solver's error message.
    message: String,
    /// Newton iterations done.
    iterations: usize,
    /// Newton error at the end.
    error: Real,
}

/// The material file with the hardening modulus `h` instead of upstream's
/// `h = 0` (every other line verbatim).
fn material_with_hardening(h: Real) -> String {
    let line = "\t h = 0     # hardening modulus\n";
    assert_eq!(MATERIAL.matches(line).count(), 1);
    MATERIAL.replace(line, &format!("\t h = {h:?}     # hardening modulus\n"))
}

/// The upstream program (`eigen`: the Eigen sparse solver pinned, as the
/// oracle replay; upstream uses the default solver).
fn run_with(options: Option<LawOptions>, eigen: bool) -> Output {
    run_material(MATERIAL, options, eigen)
}

/// The upstream program on the material file `material`.
fn run_material(material: &str, options: Option<LawOptions>, eigen: bool) -> Output {
    const DIM: usize = 2;
    let (u_increment, steps) = (0.1, 20);
    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_materials/\
         test_material_elasto_plastic_linear_isotropic_hardening/\
         test_material_elasto_plastic_linear_isotropic_hardening.msh",
    );
    let mesh = read_mesh(&path, DIM);
    let mut model = model_with(mesh, material);
    model.physics.law_options = options;
    let mut o = ModelOptions::new(AnalysisMethod::Static);
    if eigen {
        o = o.with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Auto),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        });
    }
    model.init_full(&o).unwrap();
    {
        let nls = model.solver.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(300))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-5)).unwrap();
    }
    let fixed = |value: Real, axis| FixedValue { value, axis };
    model
        .apply_dirichlet_bc(&fixed(0., SpatialDirection::X), "left")
        .unwrap();
    model
        .apply_dirichlet_bc(&fixed(0., SpatialDirection::Y), "bottom")
        .unwrap();
    let edge_nodes: Vec<usize> = {
        let mesh = model.physics.base.mesh.read().unwrap();
        let g = mesh.groups().element_group("right").unwrap();
        g.nodes().iter().map(|&n| n as usize).collect()
    };
    assert!(!edge_nodes.is_empty());
    let mut out = Output {
        text: String::new(),
        reactions: Vec::new(),
        iterations: Vec::new(),
        errors: Vec::new(),
        failed: None,
        final_state: Vec::new(),
    };
    for i in 0..steps {
        model
            .apply_dirichlet_bc(
                &fixed(i as Real * u_increment, SpatialDirection::X),
                "right",
            )
            .unwrap();
        if let Err(e) = model.solve_step(None) {
            let nls = model.solver.non_linear_solver(None).unwrap();
            out.failed = Some(Failure {
                step: i,
                message: e.to_string(),
                iterations: nls.nb_iterations(),
                error: nls.error(),
            });
            return out;
        }
        let strainxx = i as Real * u_increment / 10.;
        let residual_g = model.internal_force().unwrap();
        let residual = residual_g.as_slice();
        let mut reaction = 0.;
        for &n in &edge_nodes {
            reaction -= residual[n * DIM];
        }
        drop(residual_g);
        out.text += &format!("{},{}\n", format_g(strainxx, 4), format_g(reaction, 4));
        out.reactions.push(reaction);
        let nls = model.solver.non_linear_solver(None).unwrap();
        out.iterations.push(nls.nb_iterations());
        out.errors.push(nls.error());
    }
    out.final_state = vec![
        (
            "final/displacement",
            model.displacement().unwrap().as_slice().to_vec(),
        ),
        (
            "final/internal_force",
            model.internal_force().unwrap().as_slice().to_vec(),
        ),
    ];
    out
}

/// The upstream program with the default solver.
fn run(options: Option<LawOptions>) -> Output {
    run_with(options, false)
}

#[test]
fn format_g_matches_the_cpp_stream() {
    for (x, s) in [
        (0., "0"),
        (0.01, "0.01"),
        (0.1, "0.1"),
        (10., "10"),
        (10.219_8, "10.22"),
        (10.194_9, "10.19"),
        (1.234_56e-5, "1.235e-05"),
        (123_456., "1.235e+05"),
        (9.999_99, "10"),
    ] {
        assert_eq!(format_g(x, 4), s, "{x}");
    }
}

/// Upstream mode: the output matches the `.verified` file (`diff -biw`).
#[test]
fn test_material_standard_linear_isotropic_hardening() {
    let out = run(Some(LawOptions::upstream()));
    assert!(out.failed.is_none(), "solveStep failed: {:?}", out.failed);
    eprintln!("{}", out.text);
    assert!(
        same_text_ignoring_whitespace(&out.text, VERIFIED),
        "output differs from test_material_standard_linear_isotropic_hardening.verified:\n{}",
        out.text
    );
    eprintln!(
        "reactions (upstream mode): {:?}\nNewton iterations: {:?}",
        out.reactions, out.iterations
    );
}

/// Corrected default (UD-137, consistent plane-stress return) under the
/// upstream driver (UD-732): steps 0…10 (elastic, then first yield at
/// `ε = σ_y/E = 0.1`) print the analytical response `min(E ε, σ_y)·L_y`,
/// compared as upstream; at step 11 the plate flows plastically with
/// `h = 0` and Newton with the law's elastic tangent (UD-079, as upstream)
/// is a fixed-point iteration of rate `1 − E_t/E = 1` on the plastic mode:
/// it does not reach `1e-5` in upstream's 300 iterations (measured 3.3e-4;
/// 2.2e-5 after 20 000 iterations), so upstream's program would rethrow
/// `NLSNotConvergedException`. Upstream's off-surface return keeps an
/// apparent hardening (10.19 → 10.22) that makes the same iteration
/// converge. Pinned so that a change of either behaviour is noticed: the
/// printed steps, the failing step, its iteration count and its final error
/// (bitwise). The plastic return itself is checked by the `h = 10` run
/// below.
#[test]
fn test_material_standard_linear_isotropic_hardening_corrected_return() {
    let out = run(None);
    let (e, sigma_y, ly) = (100., 10., 1.);
    let expected: String = (0..=10)
        .map(|i| {
            let strain = i as Real * 0.1 / 10.;
            let sigma: Real = (e * strain).min(sigma_y);
            format!("{},{}\n", format_g(strain, 4), format_g(sigma * ly, 4))
        })
        .collect();
    eprintln!("{}", out.text);
    eprintln!(
        "reactions (default): {:?}\nNewton iterations: {:?}\nfailure: {:?}",
        out.reactions, out.iterations, out.failed
    );
    assert!(
        same_text_ignoring_whitespace(&out.text, &expected),
        "corrected return differs from the perfectly plastic response:\n{}",
        out.text
    );
    let failure = out.failed.expect("UD-732: step 11 unexpectedly converged");
    assert_eq!(failure.step, 11, "{failure:?}");
    assert!(failure.message.contains("did not converge"), "{failure:?}");
    // the failure itself, pinned: all 300 iterations,
    // final error 3.2774050543695377e-4, bit-identical on x86_64 and
    // aarch64
    assert_eq!(failure.iterations, 300, "{failure:?}");
    assert_eq!(
        failure.error.to_bits(),
        3.277_405_054_369_537_7e-4_f64.to_bits(),
        "{failure:?}"
    );
}

/// Corrected default (UD-137) with a hardening modulus for which the
/// upstream driver converges (`h = 10`): the plastic
/// return itself is checked against the analytical response.
///
/// Derivation. With `ν = 0`, `left` blocked in x, `bottom` in y and the
/// top edge free, the exact solution of the plate is uniaxial and uniform:
/// `σ_xx = σ`, every other stress component zero (plane stress), strain
/// `ε = u_right/L_x` with `L_x = 10`, i.e. `ε = 0.01 i` at step `i`. J2 with
/// linear isotropic hardening: yield function `|σ| − (σ_y + h p)`, where
/// for uniaxial stress the equivalent plastic strain `p` equals the axial
/// plastic strain `ε_p` (isochoric flow `ε_p·diag(1, −½, −½)`:
/// `√(⅔ ε_p:ε_p) = |ε_p|`). Elastic while `E ε ≤ σ_y`: `σ = E ε`. Once
/// yielding (monotonic loading), `σ = E (ε − ε_p) = σ_y + h ε_p` gives
/// `ε_p = (E ε − σ_y)/(E + h)` and
/// `σ = (σ_y + h ε)/(1 + h/E)` (`= σ_y` at `ε = σ_y/E`, slope
/// `E_t = E h/(E + h)`). The reaction of the `right` edge is
/// `σ·L_y` with `L_y = 1`. Values (`E = 100`, `σ_y = 10`, `h = 10`): steps
/// 0…10 `10 ε·100`, i.e. 0, 1, …, 10; steps 11…19 `(10 + 0.1 i)/1.1`,
/// i.e. 10.0909…, …, 10.8181….
///
/// Criterion: every step converges; each reaction within `1e-4` relative of
/// the analytical value (step 0: exactly 0), and the printed lines equal
/// the analytical ones at upstream's 4-digit resolution. The deviation is
/// Newton's stopping error (NUMERICAL_VALIDATION "TEST-146
/// hardening run", PROPOSED): the default `solution` criterion stops when
/// the Euclidean norm of the correction on the free DOFs is below `1e-5`,
/// and with the law's elastic tangent (UD-079) the plastic mode contracts
/// by `ρ = E/(E + h) = 10/11` per iteration, so the displacement error
/// left is about `ρ/(1 − ρ)·1e-5 = 1e-4`; over `L_x = 10` with
/// `E_t ≈ 9.09` that is ≈ 9e-5 on a reaction of ≈ 10, i.e. ≈ 1e-5
/// relative (measured ≤ 7.7e-6, this test's output).
#[test]
fn test_material_standard_linear_isotropic_hardening_corrected_return_hardening() {
    let (e, sigma_y, h, ly) = (100., 10., 10., 1.);
    let out = run_material(&material_with_hardening(h), None, false);
    eprintln!("{}", out.text);
    eprintln!(
        "reactions (default, h = 10): {:?}\nNewton iterations: {:?}\nerrors: {:?}",
        out.reactions, out.iterations, out.errors
    );
    assert!(out.failed.is_none(), "solveStep failed: {:?}", out.failed);
    assert_eq!(out.reactions.len(), 20);
    let analytical = |strain: Real| -> Real {
        let sigma = if e * strain <= sigma_y {
            e * strain
        } else {
            (sigma_y + h * strain) / (1. + h / e)
        };
        sigma * ly
    };
    let mut expected = String::new();
    let mut worst: Real = 0.;
    for (i, &reaction) in out.reactions.iter().enumerate() {
        let strain = i as Real * 0.1 / 10.;
        let r = analytical(strain);
        expected += &format!("{},{}\n", format_g(strain, 4), format_g(r, 4));
        if r == 0. {
            assert_eq!(reaction, 0., "step {i}: reaction at zero strain");
            continue;
        }
        let rel = ((reaction - r) / r).abs();
        worst = worst.max(rel);
        assert!(
            rel <= 1e-4,
            "step {i}: reaction {reaction} vs analytical {r} (relative {rel:e})"
        );
    }
    eprintln!("h = 10: worst relative deviation from the analytical response {worst:e}");
    assert!(
        same_text_ignoring_whitespace(&out.text, &expected),
        "corrected return (h = 10) differs from the linear-hardening response:\n{}",
        out.text
    );
    // the plastic steps really are plastic: past the elastic line E ε
    for i in 11..20 {
        let strain = i as Real * 0.1 / 10.;
        assert!(out.reactions[i] < 0.95 * e * strain * ly, "step {i}");
    }
}

/// Upstream mode with the Eigen sparse solver pinned: the oracle replay of
/// the upstream program (`fixtures/model/upstream_tests_2b/
/// elasto_plastic_linear_isotropic_hardening`, x86_64, same mesh and
/// material file) bitwise — printed reactions, Newton iteration counts and
/// errors of the 20 steps, final displacement and internal force.
#[test]
fn test_material_standard_linear_isotropic_hardening_oracle_bitwise() {
    let out = run_with(Some(LawOptions::upstream()), true);
    assert!(out.failed.is_none(), "solveStep failed: {:?}", out.failed);
    let f = case("upstream_tests_2b/elasto_plastic_linear_isotropic_hardening");
    let strains: Vec<Real> = (0..20).map(|i| i as Real * 0.1 / 10.).collect();
    let mut n = assert_bits(&f, "step/strain_xx", &strains);
    n += assert_bits(&f, "step/reaction", &out.reactions);
    let its: Vec<i64> = out.iterations.iter().map(|&i| i as i64).collect();
    assert_eq!(its, i64s(&f, "step/nb_iterations"), "Newton iterations");
    n += its.len();
    n += assert_bits(&f, "step/error", &out.errors);
    for (name, got) in &out.final_state {
        n += assert_bits(&f, name, got);
    }
    assert!(
        same_text_ignoring_whitespace(&out.text, VERIFIED),
        "Eigen-pinned output differs from the .verified file:\n{}",
        out.text
    );
    eprintln!("elasto-plastic plate: {n} values bitwise equal to the oracle");
}
