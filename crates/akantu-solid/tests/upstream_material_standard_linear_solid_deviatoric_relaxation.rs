// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_material_viscoelastic/{
// test_material_standard_linear_solid_deviatoric_relaxation.cc,
// test_material_standard_linear_solid_deviatoric_relaxation_tension.cc,
// material_standard_linear_solid_deviatoric_relaxation.dat,
// test_material_standard_linear_solid_deviatoric_relaxation.geo}.
//! Model-level ports of upstream `test_material_standard_linear_solid_
//! deviatoric_relaxation` (TEST-149) and `…_tension` (TEST-150): the frozen
//! Gmsh mesh of the upstream `add_mesh` target (one `_quadrangle_4`, D-009),
//! `SolidMechanicsModel::initFull()` (explicit lumped mass), the lumped mass
//! assembled, `Δt = 0.1·getStableTimeStep()`, `max_steps = ⌊25/Δt⌋`; every
//! step imposes the displacement on every node (`(εy, εx)` for the shear
//! test, `εX` for the tension test, ε ramped over `T = 10` to `0.001`, then
//! held), calls `assembleInternalForces()` and compares **every quadrature
//! point** with the analytical relaxation, relative tolerance `1e-7` (shear:
//! σ01 and σ10) / `1e-5` (tension: σ11), as upstream (`EXIT_FAILURE`
//! otherwise). The tension test calls `assembleInternalForces()` and
//! `setToSteadyState()` once before the loop, as upstream.
//!
//! Upstream's driver never solves: the history of `sls_deviatoric` advances
//! inside `computeStress` (UD-063, category B). The faithful replay
//! therefore runs the law in its upstream mode (`LawOptions::upstream()`).
//! The port's corrected default (committed state: repeated evaluations in a
//! step are idempotent, the state advances at a converged step) is driven
//! with the same loop, each `assembleInternalForces()` bracketed by the
//! materials' `beforeSolveStep` / `afterSolveStep(true)` — one converged
//! step per time step, which is what upstream's per-evaluation advancement
//! stands for — and must meet the same tolerances.
//!
//! The relative error is upstream's `|(σ − σ_ref)/σ_ref|`; its only
//! analytical zero (`t = 0`, zero strain) gives `0/0 = NaN`, which upstream's
//! `>` comparison lets pass: the port additionally requires an exactly zero
//! stress there.

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_model::constitutive_law::LawPhase;
use akantu_parser::ParamValue;
use akantu_solid::SolidMechanicsModel;
use akantu_solid::material::LawOptions;
use akantu_validation::fixtures_root;
use model_common::{init, model_with, read_mesh};

/// `material_standard_linear_solid_deviatoric_relaxation.dat` (verbatim).
const MATERIAL: &str = "material sls_deviatoric [\n\
\t name = anything\n\
\t rho = 7800     # density\n\
\t E   = 15      # young's modulus\n\
\t nu  = 0.25     # poisson's ratio\n\
\t Eta = 10       # viscosity\n\
         Ev  = 5      # viscous stiffness\n\
\t Plane_Stress = 0\n\
]\n";

const DIM: usize = 2;
const TYPE: ElementType = ElementType::Quadrangle4;

/// How the material history advances.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    /// `LawOptions::upstream()`: history advanced inside `computeStress`
    /// (upstream driver as is).
    Upstream,
    /// Port default: committed state, one converged step per time step.
    Committed,
}

fn model(mode: Mode) -> SolidMechanicsModel {
    let path = fixtures_root().join(
        "meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/test_materials/\
         test_material_viscoelastic/test_material_standard_linear_solid_deviatoric_relaxation.msh",
    );
    let mesh = read_mesh(&path, DIM);
    let mut m = model_with(mesh, MATERIAL);
    if mode == Mode::Upstream {
        m.physics.law_options = Some(LawOptions::upstream());
    }
    // upstream `model.initFull()`: default analysis method
    init(&mut m, AnalysisMethod::ExplicitLumpedMass);
    m.assemble_mass_lumped().unwrap();
    m
}

fn get(m: &SolidMechanicsModel, name: &str) -> Real {
    match m.material(0).unwrap().get_param(name).unwrap() {
        ParamValue::Real(x) => x,
        other => panic!("{name}: {other:?}"),
    }
}

/// `displacement(n, ·) = f(ε, X_n)` on every node.
fn impose(m: &mut SolidMechanicsModel, x: &[Real], f: impl Fn(&[Real]) -> [Real; DIM]) {
    let mut u_g = m.displacement_mut().unwrap();
    let u = u_g.as_mut_slice();
    for (un, xn) in u
        .as_chunks_mut::<DIM>()
        .0
        .iter_mut()
        .zip(x.as_chunks::<DIM>().0)
    {
        un.copy_from_slice(&f(xn));
    }
}

/// One evaluation of the upstream loop: `assembleInternalForces()`, as a
/// converged step of the materials in [`Mode::Committed`].
fn evaluate(m: &mut SolidMechanicsModel, mode: Mode) {
    if mode == Mode::Committed {
        m.with_material(0, LawPhase::BeforeSolveStep, |l, c| l.before_solve_step(c))
            .unwrap();
    }
    m.assemble_internal_forces().unwrap();
    if mode == Mode::Committed {
        m.with_material(0, LawPhase::AfterSolveStep, |l, c| {
            l.after_solve_step(c, true)
        })
        .unwrap();
    }
}

/// Every quadrature point's stress (column-major `DIM × DIM` tuples).
fn stresses(m: &SolidMechanicsModel) -> Vec<[Real; DIM * DIM]> {
    let s = m
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(TYPE, GhostType::NotGhost)
        .unwrap();
    s.as_slice().as_chunks::<{ DIM * DIM }>().0.to_vec()
}

/// Upstream's check of one component: `|(σ − ref)/ref| > tol` fails (NaN
/// passes), plus an exact zero at the analytical zero.
fn check(v: Real, solution: Real, tol: Real, what: &str) -> Real {
    assert!(v.is_finite(), "{what}: non-finite stress {v}");
    if solution == 0. {
        assert_eq!(v, 0., "{what}: stress at the analytical zero");
        return 0.;
    }
    let rel = ((v - solution) / solution).abs();
    assert!(
        rel <= tol,
        "{what}: Relative error: {rel} (tolerance {tol})"
    );
    rel
}

/// Upstream `test_material_standard_linear_solid_deviatoric_relaxation`.
fn shear_relaxation(mode: Mode) -> (usize, Real) {
    let (t_ramp, eps, sim_time, time_factor, tolerance) = (10., 0.001, 25., 0.1, 1e-7);
    let mut m = model(mode);
    let (eta, ev, einf, nu) = (
        get(&m, "Eta"),
        get(&m, "Ev"),
        get(&m, "Einf"),
        get(&m, "nu"),
    );
    let ginf = einf / (2. * (1. + nu));
    let g = ev / (2. * (1. + nu));
    let g0 = g + ginf;
    let gamma = g / g0;
    let tau = eta / ev;
    let gammainf = ginf / g0;
    let x = {
        let mesh = m.physics.base.mesh.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let time_step = m.stable_time_step().unwrap() * time_factor;
    m.set_time_step(time_step, None).unwrap();
    let max_steps = (sim_time / time_step) as i64;
    assert!(max_steps > 1000, "{max_steps} steps");
    let mut worst = 0.0_f64;
    let mut checked = 0;
    for s in 0..=max_steps {
        let time = s as Real * time_step;
        let epsilon = if time < t_ramp {
            eps * time / t_ramp
        } else {
            eps
        };
        impose(&mut m, &x, |p| [epsilon * p[1], epsilon * p[0]]);
        evaluate(&mut m, mode);
        let solution = if time < t_ramp {
            2. * g0 * eps / t_ramp * (gammainf * time + gamma * tau * (1. - (-time / tau).exp()))
        } else {
            2. * g0
                * eps
                * (gammainf
                    + gamma * tau / t_ramp * (((t_ramp - time) / tau).exp() - (-time / tau).exp()))
        };
        let st = stresses(&m);
        assert!(!st.is_empty());
        for (q, sigma) in st.iter().enumerate() {
            // column-major: (0, 1) at 2, (1, 0) at 1
            for (v, c) in [(sigma[2], "01"), (sigma[1], "10")] {
                let what = format!("{mode:?} step {s} q {q} σ{c}");
                worst = worst.max(check(v, solution, tolerance, &what));
                checked += 1;
            }
        }
    }
    (checked, worst)
}

/// Upstream `test_material_standard_linear_solid_deviatoric_relaxation_tension`.
fn tension_relaxation(mode: Mode) -> (usize, Real) {
    let (t_ramp, eps, sim_time, time_factor, tolerance) = (10., 0.001, 25., 0.1, 1e-5);
    let mut m = model(mode);
    // upstream: assembleInternalForces() and setToSteadyState() at zero
    // displacement, before the loop
    m.assemble_internal_forces().unwrap();
    m.with_material(0, LawPhase::Stress, |l, c| {
        l.set_all_to_steady_state(c, GhostType::NotGhost)
    })
    .unwrap();
    let (eta, ev, einf, e0) = (get(&m, "Eta"), get(&m, "Ev"), get(&m, "Einf"), get(&m, "E"));
    let (kpa, mu) = (get(&m, "kapa"), get(&m, "mu"));
    let gamma = ev / e0;
    let gammainf = einf / e0;
    let tau = eta / ev;
    let x = {
        let mesh = m.physics.base.mesh.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let time_step = m.stable_time_step().unwrap() * time_factor;
    m.set_time_step(time_step, None).unwrap();
    let max_steps = (sim_time / time_step) as i64;
    assert!(max_steps > 1000, "{max_steps} steps");
    let mut worst = 0.0_f64;
    let mut checked = 0;
    for s in 0..=max_steps {
        let time = s as Real * time_step;
        let epsilon = if time < t_ramp {
            eps * time / t_ramp
        } else {
            eps
        };
        impose(&mut m, &x, |p| [epsilon * p[0], epsilon * p[1]]);
        evaluate(&mut m, mode);
        let epskk = DIM as Real * eps;
        let solution = if time < t_ramp {
            2. * mu * (eps - epskk / 3.) / t_ramp
                * (gammainf * time + gamma * tau * (1. - (-time / tau).exp()))
                + gammainf * kpa * epskk * time / t_ramp
        } else {
            2. * mu
                * (eps - epskk / 3.)
                * (gammainf
                    + gamma * tau / t_ramp * (((t_ramp - time) / tau).exp() - (-time / tau).exp()))
                + gammainf * kpa * epskk
        };
        for (q, sigma) in stresses(&m).iter().enumerate() {
            let what = format!("{mode:?} step {s} q {q} σ11");
            worst = worst.max(check(sigma[3], solution, tolerance, &what));
            checked += 1;
        }
    }
    (checked, worst)
}

#[test]
fn test_material_standard_linear_solid_deviatoric_relaxation() {
    let (n, worst) = shear_relaxation(Mode::Upstream);
    eprintln!("SLS shear relaxation (upstream mode): {n} values, max relative error {worst:e}");
}

#[test]
fn test_material_standard_linear_solid_deviatoric_relaxation_committed_state() {
    let (n, worst) = shear_relaxation(Mode::Committed);
    eprintln!("SLS shear relaxation (committed state): {n} values, max relative error {worst:e}");
}

#[test]
fn test_material_standard_linear_solid_deviatoric_relaxation_tension() {
    let (n, worst) = tension_relaxation(Mode::Upstream);
    eprintln!("SLS tension relaxation (upstream mode): {n} values, max relative error {worst:e}");
}

#[test]
fn test_material_standard_linear_solid_deviatoric_relaxation_tension_committed_state() {
    let (n, worst) = tension_relaxation(Mode::Committed);
    eprintln!("SLS tension relaxation (committed state): {n} values, max relative error {worst:e}");
}

/// The two modes really differ on upstream's bare driver (UD-063): two
/// `assembleInternalForces()` at the same displacement advance the history
/// twice in upstream mode (the stress relaxes) and are idempotent in the
/// committed-state default. Guards against the mode switch being ignored.
#[test]
fn ud063_bare_driver_advances_only_in_upstream_mode() {
    for mode in [Mode::Upstream, Mode::Committed] {
        let mut m = model(mode);
        let x = {
            let mesh = m.physics.base.mesh.read().unwrap();
            mesh.nodes().as_slice().to_vec()
        };
        m.set_time_step(0.5, None).unwrap();
        impose(&mut m, &x, |p| [1e-3 * p[1], 1e-3 * p[0]]);
        m.assemble_internal_forces().unwrap();
        let first = stresses(&m);
        m.assemble_internal_forces().unwrap();
        let second = stresses(&m);
        let same = first
            .iter()
            .flatten()
            .zip(second.iter().flatten())
            .all(|(a, b)| a.to_bits() == b.to_bits());
        assert_eq!(
            same,
            mode == Mode::Committed,
            "{mode:?}: {first:?} vs {second:?}"
        );
    }
}
