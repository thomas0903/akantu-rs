// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_material_viscoelastic_maxwell/{
// test_material_viscoelastic_maxwell_relaxation.cc,
// material_viscoelastic_maxwell.dat, test_material_viscoelastic_maxwell.geo}
// (registered as `test_material_viscoelasti_maxwell_relaxation`, UNSTABLE).
//! Model-level port of upstream `test_material_viscoelasti_maxwell_relaxation`
//! (TEST-151): the frozen Gmsh mesh of the upstream `add_mesh` target (one
//! `_quadrangle_4`, D-009), `initFull(_static)`, `Δt = 0.1`,
//! `max_steps = ⌊100/Δt + 1⌋`, Newton–Raphson `max_iterations = 200`,
//! `threshold = 1e-7`, `_residual`; for `s = 0..=max_steps` the nodes on the
//! four edges (`Math::are_float_equal` on 0 and 0.001: 8 of the 9 nodes of
//! the 2 × 2 quadrangles, the centre node stays free) are blocked at the
//! biaxial field of the upstream loop (ε ramped over
//! `T = 10` to 0.1, then held), `solveStep()` (an exception is swallowed as
//! upstream; the solver must report `converged`, else `EXIT_FAILURE`), then
//! σ00 of **every quadrature point** is compared with the analytical
//! relaxation `pre·ε/T (E∞ t + λ E_v (1 − e^{−t/λ}))` (ramp) /
//! `pre·ε (E∞ + λ E_v/T (e^{(T−t)/λ} − e^{−t/λ}))` (hold), relative tolerance
//! `1e-6`, as upstream (the relative error of the analytical zero at `t = 0`
//! is `0/0 = NaN`, which upstream lets pass; the port also requires an
//! exactly zero stress there).
//!
//! Upstream marks the test `UNSTABLE`: its law never updates the branch
//! stresses (UD-029, category B), so the stress follows the instantaneous
//! modulus and the test fails. The port's corrected default (exponential
//! recurrence after each converged step) passes the upstream criterion; the
//! upstream mode (`LawOptions::upstream()`) is replayed with the same driver
//! and must fail it, which is the documented upstream behaviour (not
//! hidden: asserted). Measured: default 2.8e-15 worst relative error; upstream
//! mode exact at step 1 (one exponential step from the zero state), then
//! 0.195 at step 2 (the branch stress of step 1 is never committed).

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_parser::ParamValue;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::material::LawOptions;
use akantu_validation::fixtures_root;
use model_common::{init, model_with, read_mesh, set_newton};

/// `material_viscoelastic_maxwell.dat` (verbatim).
const MATERIAL: &str = "material viscoelastic_maxwell [\n\
\t name = anything\n\
\t rho  = 7800     # density\n\
\t Einf = 15e6      # young's modulus\n\
\t nu   = 0.0     # poisson's ratio\n\
\t Eta = [40e6]       # viscosity\n\
         Ev  =[10e6]      # viscous stiffness\n\
\t Plane_Stress = 0\n\
]\n";

const DIM: usize = 2;
const TYPE: ElementType = ElementType::Quadrangle4;

/// Upstream `Math::are_float_equal(x, y)` with the default
/// `Math::tolerance = 1e2·ε`.
fn are_float_equal(x: Real, y: Real) -> bool {
    akantu_core::math::are_float_equal(x, y, akantu_core::math::DEFAULT_TOLERANCE)
}

/// Outcome of the upstream loop.
struct Run {
    /// First failing `(step, relative error)`, if any.
    first_failure: Option<(usize, Real)>,
    /// Largest relative error over the steps that passed.
    worst: Real,
    /// Values compared.
    checked: usize,
}

fn run(upstream: bool) -> Run {
    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_materials/\
         test_material_viscoelastic_maxwell/test_material_viscoelastic_maxwell.msh",
    );
    let mesh = read_mesh(&path, DIM);
    let x = mesh.nodes().as_slice().to_vec();
    let mut m = model_with(mesh, MATERIAL);
    if upstream {
        m.physics.law_options = Some(LawOptions::upstream());
    }
    init(&mut m, AnalysisMethod::Static);
    let (eps, sim_time, t_ramp, tolerance, time_step) = (0.1, 100., 10., 1e-6, 0.1);
    let (eta, ev) = {
        let law = m.material(0).unwrap();
        match (law.get_param("Eta").unwrap(), law.get_param("Ev").unwrap()) {
            (ParamValue::VectorReal(a), ParamValue::VectorReal(b)) => (a[0], b[0]),
            other => panic!("Eta/Ev: {other:?}"),
        }
    };
    let real = |m: &SolidMechanicsModel, n: &str| match m.material(0).unwrap().get_param(n) {
        Ok(ParamValue::Real(v)) => v,
        other => panic!("{n}: {other:?}"),
    };
    let (einf, nu) = (real(&m, "Einf"), real(&m, "nu"));
    let lambda = eta / ev;
    let pre_mult = 1. / (1. + nu) / (1. - 2. * nu);
    m.set_time_step(time_step, None).unwrap();
    let max_steps = (sim_time / time_step + 1.) as usize;
    set_newton(&mut m, 200, 1e-7, "residual");
    let mut time = 0.;
    let mut out = Run {
        first_failure: None,
        worst: 0.,
        checked: 0,
    };
    for s in 0..=max_steps {
        let epsilon = if time < t_ramp {
            eps * time / t_ramp
        } else {
            eps
        };
        {
            let mut imposed = Vec::new();
            for (n, p) in x.as_chunks::<DIM>().0.iter().enumerate() {
                let v = if are_float_equal(p[0], 0.0) {
                    Some([0., epsilon * p[1]])
                } else if are_float_equal(p[1], 0.0) {
                    Some([epsilon * p[0], 0.])
                } else if are_float_equal(p[0], 0.001) || are_float_equal(p[1], 0.001) {
                    Some([epsilon * p[0], epsilon * p[1]])
                } else {
                    None
                };
                if let Some(v) = v {
                    imposed.push((n, v));
                }
            }
            assert_eq!((imposed.len(), x.len() / DIM), (8, 9), "edge nodes");
            for (n, v) in imposed {
                m.displacement_mut().unwrap().as_mut_slice()[n * DIM..(n + 1) * DIM]
                    .copy_from_slice(&v);
                m.blocked_dofs_mut().unwrap().as_mut_slice()[n * DIM..(n + 1) * DIM].fill(true);
            }
        }
        // upstream: `try { model.solveStep(); } catch (debug::Exception &) {}`
        let _ = m.solve_step(None);
        let nls = m.solver.non_linear_solver(None).unwrap();
        assert!(
            nls.converged(),
            "step {s}: Didn't converge after {} iterations. Error is {}",
            nls.nb_iterations(),
            nls.error()
        );
        let solution = if time < t_ramp {
            pre_mult * eps / t_ramp * (einf * time + lambda * ev * (1. - (-time / lambda).exp()))
        } else {
            pre_mult
                * eps
                * (einf
                    + lambda * ev / t_ramp
                        * (((t_ramp - time) / lambda).exp() - (-time / lambda).exp()))
        };
        let stress = m
            .material(0)
            .unwrap()
            .base()
            .stress
            .get(TYPE, GhostType::NotGhost)
            .unwrap()
            .as_slice()
            .to_vec();
        assert!(!stress.is_empty());
        for (q, sigma) in stress.as_chunks::<{ DIM * DIM }>().0.iter().enumerate() {
            let v = sigma[0];
            assert!(v.is_finite(), "step {s} q {q}: non-finite stress");
            if solution == 0. {
                assert_eq!(v, 0., "step {s} q {q}: stress at the analytical zero");
                continue;
            }
            let rel = ((v - solution) / solution).abs();
            out.checked += 1;
            if rel > tolerance {
                out.first_failure.get_or_insert((s, rel));
            } else {
                out.worst = out.worst.max(rel);
            }
        }
        if out.first_failure.is_some() {
            // upstream returns EXIT_FAILURE at the first failing step
            return out;
        }
        time += time_step;
    }
    out
}

/// The upstream criterion with the port's corrected default (UD-029).
#[test]
fn test_material_viscoelastic_maxwell_relaxation() {
    let r = run(false);
    assert!(
        r.first_failure.is_none(),
        "relative error {:?} > 1e-6",
        r.first_failure
    );
    assert!(r.checked > 4000, "{} values", r.checked);
    eprintln!(
        "Maxwell relaxation (default): {} values, max relative error {:e}",
        r.checked, r.worst
    );
}

/// Upstream mode (UD-029: branch stresses never updated): the upstream
/// criterion fails, as the upstream test does (`UNSTABLE`), **where and with
/// the value the oracle fails**. The pinned oracle running
/// the unmodified upstream program converges at `Time = 0`, `0.1`, `0.2` and then
/// prints `Relative error: 0.194557` and returns `EXIT_FAILURE`, i.e. it
/// fails at step 2 at the first quadrature point compared. The value is
/// printed with the stream's default precision (6 significant digits,
/// `%g`), so the record is not exact: the port's value must lie in the
/// rounding interval of the printed digits, `|rel − 0.194557| ≤ 5e-7`
/// (docs/NUMERICAL_VALIDATION.md, printed oracle value of TEST-151,
/// PROPOSED). Measured: 0.19455744978544992 (x86_64; aarch64
/// bit-identical).
#[test]
fn test_material_viscoelastic_maxwell_relaxation_upstream_mode_fails_as_upstream() {
    let r = run(true);
    let (step, rel) = r
        .first_failure
        .expect("upstream mode unexpectedly meets the upstream criterion");
    eprintln!("Maxwell relaxation (upstream mode): fails at step {step}, relative error {rel:e}");
    // the oracle record: steps 0 and 1 pass, step 2 fails with 0.194557
    const ORACLE_STEP: usize = 2;
    const ORACLE_PRINTED_ERROR: Real = 0.194_557;
    const HALF_UNIT_OF_LAST_PRINTED_DIGIT: Real = 5e-7;
    assert_eq!(step, ORACLE_STEP, "relative error {rel:e}");
    assert!(
        (rel - ORACLE_PRINTED_ERROR).abs() <= HALF_UNIT_OF_LAST_PRINTED_DIGIT,
        "relative error {rel:e}, oracle printed {ORACLE_PRINTED_ERROR}"
    );
}
