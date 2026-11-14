// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Implicit-dynamic
// coupled solves (CPL-002 / CPL-004): upstream's
// `CouplerSolidContact` (coupler_solid_contact_tmpl.hh, `case
// _implicit_dynamic` in `corrector` and `assembleStiffnessMatrix`, `M` from
// the solid) and `CohesiveContactSolverCallback` (`solid.assembleMass()`).
// No oracle run: independent checks only.

//! Implicit-dynamic runs of the solid–contact coupler and of the
//! stand-alone callback on the Hertz setup of
//! `examples/c++/contact_mechanics_model/hertz/contact_explicit_static.cc`
//! (and, for the cohesive callback, on the `cohesive_static` fixture
//! setup). Independent checks, none of which needs an oracle:
//!
//! * **equation of motion**: after every converged step
//!   `M a = f_ext + f_int + f_contact` on the free DOFs (the arrays hold the
//!   forces with the sign convention of the residual `r = f_ext + f_int +
//!   f_contact − M a`), with `M` assembled by the solid itself
//!   (`assemble_mass`, not through the callback under test). A coupler that
//!   does not forward `M` solves the static equation instead and misses
//!   this by `‖M a‖` (the mutant `"M" => Ok(())` fails here);
//! * **quasi-static limit**: a time step `10⁶` times the period gives the
//!   static solution;
//! * the stand-alone callback is the coupler's callback (same arithmetic:
//!   bitwise equal trajectories).

mod common;

use akantu_cohesive::model::CohesiveCallback;
use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::{CohesiveSurfaceSelector, ContactUpstreamCompat, PhysicalSurfaceSelector};
use akantu_core::Real;
use akantu_core::enums::{
    AnalysisMethod, MatrixType, NonLinearSolverType, SparseSolverType, SpatialDirection,
};
use akantu_couplers::contact::{CallbackFlavour, SolidContactCallback};
use akantu_couplers::{
    CohesiveContactSolverCallback, CouplerSolidCohesiveContact, CouplerSolidContact,
    SolidContactCompat,
};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolverOptions};
use common::*;

const HERTZ_DAT: &str = include_str!("data/contact/hertz_material.dat");

/// A time step so large (the Hertz setup has `E = 1e3`, `ρ = 1`, element
/// size ~ 0.1…1: periods well below 10) that `M/(β Δt²)` is a ~1e-12 relative
/// perturbation of `K`.
const QUASI_STATIC_DT: Real = 1e6;
/// A time step of the order of the dynamics (inertia visible: ~1e-3
/// relative difference from the static solution).
const DYNAMIC_DT: Real = 0.1;

fn hertz(method: AnalysisMethod, dt: Real) -> CouplerSolidContact {
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/hertz.msh"),
        2,
    );
    let parser = Parser::parse_str(HERTZ_DAT, "material.dat").unwrap();
    let selector = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
    let mut c = CouplerSolidContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    c.solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    c.init_full(&ModelOptions::new(method)).unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    c.contact
        .set_upstream_compat(ContactUpstreamCompat::default());
    let fix = |axis| FixedValue { value: 0.0, axis };
    for (axis, group) in [
        (SpatialDirection::X, "fixed"),
        (SpatialDirection::Y, "fixed"),
        (SpatialDirection::X, "loading"),
        (SpatialDirection::X, "symmetry"),
    ] {
        c.apply_dirichlet_bc(&fix(axis), group).unwrap();
    }
    if method == AnalysisMethod::ImplicitDynamic {
        c.set_time_step(dt, None).unwrap();
    }
    c
}

fn load(c: &mut CouplerSolidContact) -> akantu_core::Result<()> {
    let inc = IncrementValue {
        value: -1e-4,
        axis: SpatialDirection::Y,
    };
    c.apply_dirichlet_bc(&inc, "loading")
}

/// One step through the coupler.
fn coupler_step(c: &mut CouplerSolidContact) -> akantu_core::Result<()> {
    load(c)?;
    c.solve_step(None)
}

/// One step through the stand-alone flavour of the callback on the plain
/// solid physics (the configuration of `CohesiveContactSolverCallback`
/// without the cohesive part).
fn standalone_step(c: &mut CouplerSolidContact) -> akantu_core::Result<()> {
    load(c)?;
    let contact = &mut c.contact;
    c.solid.solve_step_with(None, |physics, solver, id| {
        let mut cb = SolidContactCallback::new(
            physics,
            contact,
            AnalysisMethod::ImplicitDynamic,
            CallbackFlavour::StandAlone {
                upstream_stiffness: false,
            },
            SolidContactCompat::default(),
        );
        solver.solve_step(&mut cb, id)
    })
}

/// Dense copy of the DOF manager's matrix `id` (COO entries summed,
/// symmetric storage mirrored).
fn dense(dofs: &DofManager, id: &str) -> (usize, Vec<Real>) {
    let k = dofs.matrix(id).unwrap();
    let n = usize::try_from(k.size()).unwrap();
    let mut d = vec![0.0; n * n];
    let sym = k.matrix_type() == MatrixType::Symmetric;
    for ((i, j), v) in k
        .irn_one_based()
        .into_iter()
        .zip(k.jcn_one_based())
        .zip(k.values())
    {
        let (i, j) = (
            usize::try_from(i - 1).unwrap(),
            usize::try_from(j - 1).unwrap(),
        );
        d[i * n + j] += v;
        if sym && i != j {
            d[j * n + i] += v;
        }
    }
    (n, d)
}

/// `(‖M a − (f_ext + f_int + f_contact)‖, ‖M a‖, ‖f_int‖)` over the free
/// DOFs, `M` the consistent mass matrix.
fn equation_of_motion(
    mass: &(usize, Vec<Real>),
    a: &[Real],
    f_ext: &[Real],
    f_int: &[Real],
    f_contact: &[Real],
    blocked: &[bool],
) -> (Real, Real, Real) {
    let (n, m) = mass;
    assert_eq!(*n, a.len());
    let (mut r2, mut ma2, mut f2) = (0.0, 0.0, 0.0);
    for i in 0..*n {
        if blocked[i] {
            continue;
        }
        let ma: Real = (0..*n).map(|j| m[i * n + j] * a[j]).sum();
        let r = ma - (f_ext[i] + f_int[i] + f_contact[i]);
        r2 += r * r;
        ma2 += ma * ma;
        f2 += f_int[i].powi(2);
    }
    (r2.sqrt(), ma2.sqrt(), f2.sqrt())
}

/// Equation of motion of the coupler after the last step.
fn hertz_motion(c: &mut CouplerSolidContact) -> (Real, Real, Real) {
    // `M` from the solid itself, not through the callback under test
    c.solid.assemble_mass().unwrap();
    let mass = dense(&c.solid.solver.dof_manager, "M");
    equation_of_motion(
        &mass,
        c.solid.acceleration().unwrap().as_slice(),
        c.external_force().unwrap().as_slice(),
        c.solid.internal_force().unwrap().as_slice(),
        c.contact.internal_force().as_slice(),
        c.solid.blocked_dofs().unwrap().as_slice(),
    )
}

/// CPL-002: with a time step `10⁶` times the dynamics the implicit-dynamic
/// coupled run (the Newton loop is also the active-set loop, contact
/// stiffness in `K`, `M` from the solid) equals the static coupled run: the
/// same contact sets and the displacements within `1e-8` of `max|u|`
/// (displacement scale `1e-4…1.2e-3`; measured `1.26e-10`). The bound is
/// an empirical regression guard (margin ×80), not a stopping-criterion
/// bound: the criterion envelope is
/// `≈ 2e-6`, both Newton loops converge well past it.
#[test]
fn coupler_implicit_dynamic_quasi_static_limit_equals_static() {
    let mut st = hertz(AnalysisMethod::Static, 0.0);
    let mut dy = hertz(AnalysisMethod::ImplicitDynamic, QUASI_STATIC_DT);
    for s in 0..12 {
        coupler_step(&mut st).unwrap_or_else(|e| panic!("static step {s}: {e}"));
        coupler_step(&mut dy).unwrap_or_else(|e| panic!("dynamic step {s}: {e}"));
        assert_eq!(
            st.contact.contact_elements().len(),
            dy.contact.contact_elements().len(),
            "step {s}: contact sets"
        );
        let a = st.displacement().unwrap().as_slice().to_vec();
        let b = dy.displacement().unwrap().as_slice().to_vec();
        let scale = a.iter().fold(0.0f64, |m, x| m.max(x.abs()));
        let diff = a
            .iter()
            .zip(&b)
            .fold(0.0f64, |m, (x, y)| m.max((x - y).abs()));
        assert!(diff <= 1e-8 * scale, "step {s}: {:e}", diff / scale);
    }
    assert!(!dy.contact.contact_elements().is_empty());
}

/// CPL-002 / CPL-004 (kills the mutant `"M" => Ok(())` in the callback):
/// with a dynamic time step every converged implicit-dynamic step of the
/// coupler **and** of the stand-alone callback satisfies the equation of
/// motion `M a = f_ext + f_int + f_contact` (residual `≤ 1e-8` of the
/// solid's internal force norm; measured `8.1e-10`: an empirical
/// regression guard, margin ×12, not
/// a stopping-criterion bound, whose envelope is `≈ 1e-5`; if it trips
/// without a code change, look at the Newton iteration count first) with
/// the inertia visible (`‖M a‖` ≥ `1e-5` of the internal
/// force, observed `~1e-3`; the trajectory differs from the static one),
/// the contact set is non-empty, and the two flavours are bitwise equal.
#[test]
fn implicit_dynamic_coupler_and_standalone_callback_satisfy_the_equation_of_motion() {
    let mut a = hertz(AnalysisMethod::ImplicitDynamic, DYNAMIC_DT);
    let mut b = hertz(AnalysisMethod::ImplicitDynamic, DYNAMIC_DT);
    let mut st = hertz(AnalysisMethod::Static, 0.0);
    for s in 0..6 {
        coupler_step(&mut a).unwrap_or_else(|e| panic!("coupler step {s}: {e}"));
        standalone_step(&mut b).unwrap_or_else(|e| panic!("stand-alone step {s}: {e}"));
        coupler_step(&mut st).unwrap();
        for (name, c) in [("coupler", &mut a), ("stand-alone", &mut b)] {
            let (r, ma, f) = hertz_motion(c);
            assert!(f > 0.0, "{name} step {s}: no force");
            assert!(
                r <= 1e-8 * f,
                "{name} step {s}: ‖M a − f‖ = {r:e} (‖M a‖ = {ma:e}, ‖f‖ = {f:e})"
            );
            assert!(
                ma >= 1e-5 * f,
                "{name} step {s}: no inertia (‖M a‖ = {ma:e}, ‖f‖ = {f:e})"
            );
        }
        assert!(s == 0 || !a.contact.contact_elements().is_empty());
        let (x, y) = (a.displacement().unwrap(), b.displacement().unwrap());
        assert!(
            x.as_slice()
                .iter()
                .zip(y.as_slice())
                .all(|(p, q)| p.to_bits() == q.to_bits()),
            "step {s}: the callbacks differ"
        );
        // the trajectory is genuinely dynamic
        let u_st = st.displacement().unwrap();
        let scale = u_st.as_slice().iter().fold(0.0f64, |m, v| m.max(v.abs()));
        let diff = u_st
            .as_slice()
            .iter()
            .zip(x.as_slice())
            .fold(0.0f64, |m, (p, q)| m.max((p - q).abs()));
        assert!(
            diff >= 1e-5 * scale,
            "step {s}: static-like {:e}",
            diff / scale
        );
    }
}

// ---------------------------------------------------------------- cohesive

/// Boxed: the coupler is a large value (the test threads have 2 MiB stacks).
fn cohesive_coupler(method: AnalysisMethod, dt: Real) -> Box<CouplerSolidCohesiveContact> {
    let f = cases("model/couplers", "cohesive_static").remove(0);
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut c = CouplerSolidCohesiveContact::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_contact",
        parser,
    )
    .unwrap();
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    c.solid.ext.compat = akantu_cohesive::material::CohesiveUpstreamCompat::upstream();
    let options = ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    });
    c.init_full(&options, false).unwrap();
    c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    c.set_upstream_compat(SolidContactCompat::default());
    c.apply_dirichlet_bc(
        &FixedValue {
            value: 0.0,
            axis: SpatialDirection::X,
        },
        "sides",
    )
    .unwrap();
    if method == AnalysisMethod::ImplicitDynamic {
        c.set_time_step(dt, None).unwrap();
    }
    c.solid
        .smm
        .solver
        .non_linear_solver_mut(None)
        .unwrap()
        .set_param("max_iterations", ParamValue::Int(100))
        .unwrap();
    Box::new(c)
}

fn cohesive_load(c: &mut CouplerSolidCohesiveContact) {
    let inc: Real = 1e-3 / 50.0;
    for (value, group) in [(-inc, "loading"), (inc, "fixed")] {
        c.apply_dirichlet_bc(
            &IncrementValue {
                value,
                axis: SpatialDirection::Y,
            },
            group,
        )
        .unwrap();
    }
    c.forward_new_cohesive_elements().unwrap();
}

/// One step of the stand-alone `CohesiveContactSolverCallback`.
fn cohesive_standalone_step(c: &mut CouplerSolidCohesiveContact) -> akantu_core::Result<()> {
    cohesive_load(c);
    let contact = &mut c.contact;
    c.solid.smm.solve_step_with(None, |physics, solver, id| {
        let mut cohesive = CohesiveCallback::new(physics);
        let mut cb = CohesiveContactSolverCallback::new(
            &mut cohesive,
            contact,
            AnalysisMethod::ImplicitDynamic,
        );
        solver.solve_step(cb.as_callback(), id)
    })
}

fn cohesive_u(c: &CouplerSolidCohesiveContact) -> Vec<Real> {
    c.solid.smm.displacement().unwrap().as_slice().to_vec()
}

fn rel_diff(a: &[Real], b: &[Real]) -> Real {
    let scale = a.iter().fold(0.0f64, |m, x| m.max(x.abs()));
    a.iter()
        .zip(b)
        .fold(0.0f64, |m, (x, y)| m.max((x - y).abs()))
        / scale
}

/// CPL-004 (kills the same mutant on the cohesive path): the stand-alone
/// `CohesiveContactSolverCallback` in implicit dynamics (its `"M"` is
/// `solid.assembleMass()` of the cohesive model) on the `cohesive_static`
/// setup gives the coupler's trajectory bitwise, and
/// * with the quasi-static time step, four steps, the callbacks bitwise
///   equal in each. The comparison with upstream is
///   `cohesive_implicit_dynamic.rs`: both callbacks
///   replay upstream's implicit-dynamic runs iterate by iterate (the
///   static-limit bound `1e-8` of the first step that stood here, margin
///   1.8, was dropped), and the departure from the
///   static run from the second step on is the Newmark predictor's
///   (`quasi_static_departure_comes_from_the_newmark_predictor`);
/// * with a dynamic time step (`Δt = 0.01`; the second step does not
///   converge, reported) the first step differs from the static one by the
///   inertia (`≥ 1e-6` relative; a callback that does not forward `M`
///   returns the static solution, `~1e-12`).
#[test]
fn cohesive_standalone_callback_implicit_dynamic_matches_coupler_and_static_limit() {
    let mut a = cohesive_coupler(AnalysisMethod::ImplicitDynamic, QUASI_STATIC_DT);
    let mut b = cohesive_coupler(AnalysisMethod::ImplicitDynamic, QUASI_STATIC_DT);
    for s in 0..4 {
        cohesive_load(&mut a);
        a.solve_step(None)
            .unwrap_or_else(|e| panic!("coupler step {s}: {e}"));
        cohesive_standalone_step(&mut b).unwrap_or_else(|e| panic!("stand-alone step {s}: {e}"));
        assert!(!a.contact.contact_elements().is_empty(), "step {s}");
        let (ua, ub) = (cohesive_u(&a), cohesive_u(&b));
        assert!(
            ua.iter().zip(&ub).all(|(p, q)| p.to_bits() == q.to_bits()),
            "step {s}: the callbacks differ"
        );
    }
    // a dynamic time step: the inertia is visible in the first step
    let mut st = cohesive_coupler(AnalysisMethod::Static, 0.0);
    let mut a = cohesive_coupler(AnalysisMethod::ImplicitDynamic, 1e-2);
    let mut b = cohesive_coupler(AnalysisMethod::ImplicitDynamic, 1e-2);
    cohesive_load(&mut st);
    st.solve_step(None).unwrap();
    cohesive_load(&mut a);
    a.solve_step(None).unwrap();
    cohesive_standalone_step(&mut b).unwrap();
    let (ua, ub, us) = (cohesive_u(&a), cohesive_u(&b), cohesive_u(&st));
    assert!(ua.iter().zip(&ub).all(|(p, q)| p.to_bits() == q.to_bits()));
    let d = rel_diff(&us, &ua);
    assert!(d >= 1e-6, "no inertia: static vs dynamic {d:e}");
}
