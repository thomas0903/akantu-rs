// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential
// tests: replay of every fixtures/model/* case (oracle: model_probe.py on the
// strict reference build), bitwise with the `eigen` backend (D-008).

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{GhostType, Real};
use model_common::*;

/// Static patch tests (`static/patch_*`): affine Dirichlet field on the
/// bounding-box faces, Newton–Raphson (2 iterations, `solution` criterion).
/// The prescribed values are taken from the recorded solution on the
/// blocked DOFs (the probe computes `X Aᵀ + b` with numpy/OpenBLAS).
fn replay_static(name: &str) -> usize {
    let f = case(name);
    let mut model = model_of(&f);
    init(&mut model, AnalysisMethod::Static);
    let blocked = bools(&f, "blocked_dofs");
    let u_ref = f64s(&f, "displacement");
    {
        let mut b = model.blocked_dofs_mut().unwrap();
        b.as_mut_slice().copy_from_slice(&blocked);
        drop(b);
        let mut u = model.displacement_mut().unwrap();
        for (k, &bl) in blocked.iter().enumerate() {
            if bl {
                u.as_mut_slice()[k] = u_ref[k];
            }
        }
    }
    let solver = param(&f, "solver");
    set_newton(
        &mut model,
        solver["max_iterations"].as_i64().unwrap(),
        solver["threshold"].as_f64().unwrap(),
        solver["convergence_type"].as_str().unwrap(),
    );
    model.solve_step(None).unwrap();
    check_static_state(&f, &mut model)
}

/// Loaded cases (`static/loaded_*`): x = xmin face blocked, nodal load on
/// the x = xmax face, Newton–Raphson with the residual criterion.
fn replay_loaded(name: &str) -> usize {
    let f = case(name);
    let mut model = model_of(&f);
    init(&mut model, AnalysisMethod::Static);
    let blocked = bools(&f, "blocked_dofs");
    model
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&blocked);
    let fext = f64s(&f, "external_force");
    model
        .external_force_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&fext);
    let solver = param(&f, "solver");
    set_newton(
        &mut model,
        solver["max_iterations"].as_i64().unwrap(),
        solver["threshold"].as_f64().unwrap(),
        solver["convergence_type"].as_str().unwrap(),
    );
    model.solve_step(None).unwrap();
    check_static_state(&f, &mut model)
}

fn report(f: &akantu_validation::Fixture, name: &str, got: &[f64]) {
    let want = f64s(f, name);
    let (n, w) = diff_stats(got, &want);
    eprintln!(
        "{}: {name}: {n}/{} differ (max rel {w:e})",
        f.case_id(),
        want.len()
    );
}

/// The existing static fixtures were solved by the oracle's `auto` sparse
/// solver, which is MUMPS in the oracle build (`AKANTU_IMPLICIT_SOLVER =
/// Mumps+PETSc`): the solved state is compared with the fixture's reviewed
/// `linear_solve` rules, while everything that does not depend on the
/// factorisation is bitwise — the stiffness (assembled from the prescribed
/// state), the external force, the iteration count, and the internal
/// forces / gradients / stresses recomputed from the **oracle's**
/// displacement (assembly given `u`).
fn check_static_state(
    f: &akantu_validation::Fixture,
    model: &mut akantu_solid::SolidMechanicsModel,
) -> usize {
    if std::env::var("SMM_DIAG").is_ok() {
        report(f, "displacement", model.displacement().unwrap().as_slice());
        report(
            f,
            "internal_force",
            model.internal_force().unwrap().as_slice(),
        );
    }
    let mut n = 0;
    let nls = model.solver.non_linear_solver(None).unwrap();
    assert_eq!(
        nls.nb_iterations() as u64,
        param(f, "nb_iterations").as_u64().unwrap(),
        "{}: nb_iterations",
        f.case_id()
    );
    assert!(nls.converged(), "{}: not converged", f.case_id());
    let k = model.dof_manager().matrix("K").unwrap();
    let (r, c, v) = k.coo();
    let rows: Vec<i64> = r.to_vec();
    let cols: Vec<i64> = c.to_vec();
    assert_eq!(rows, i64s(f, "K/rows"), "{}: K rows", f.case_id());
    assert_eq!(cols, i64s(f, "K/cols"), "{}: K cols", f.case_id());
    n += 2 * rows.len();
    n += assert_bits(f, "K/values", v);
    let fext = model.external_force().unwrap().as_slice().to_vec();
    n += assert_bits(f, "external_force", &fext);
    // solved state: reviewed tolerance rules of fixtures/model/tolerances.json
    let mut t = akantu_validation::DiffTest::with_fixture(
        "smm_static_solved_state",
        case(f.case_id().trim_start_matches("model/")),
    )
    .unwrap();
    t.check_f64("displacement", model.displacement().unwrap().as_slice());
    let fint = model.internal_force().unwrap().as_slice().to_vec();
    t.check_f64("internal_force", &fint);
    let res: Vec<Real> = fext.iter().zip(&fint).map(|(a, b)| a + b).collect();
    t.check_f64("residual", &res);
    let dim = model.spatial_dimension();
    let types = {
        let mesh = model.physics.base.mesh.read().unwrap();
        types_of(&mesh, dim)
    };
    for &ty in &types {
        t.check_f64(
            &format!("grad_u/{}", tname(ty)),
            &law_tensor(model, "grad_u", ty),
        );
        t.check_f64(
            &format!("stress/{}", tname(ty)),
            &law_tensor(model, "stress", ty),
        );
    }
    if let Err(e) = t.finish() {
        panic!("{e}");
    }
    // assembly given the oracle's u: bitwise
    let u_ref = f64s(f, "displacement");
    model
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u_ref);
    model.assemble_internal_forces().unwrap();
    n += assert_bits(
        f,
        "internal_force",
        model.internal_force().unwrap().as_slice(),
    );
    let fint = model.internal_force().unwrap().as_slice().to_vec();
    let res: Vec<Real> = fext.iter().zip(&fint).map(|(a, b)| a + b).collect();
    n += assert_bits(f, "residual", &res);
    for &ty in &types {
        n += assert_bits(
            f,
            &format!("grad_u/{}", tname(ty)),
            &law_tensor(model, "grad_u", ty),
        );
        n += assert_bits(
            f,
            &format!("stress/{}", tname(ty)),
            &law_tensor(model, "stress", ty),
        );
    }
    n
}

#[test]
fn static_patch_tests_bitwise() {
    let cases: Vec<String> = cases_of("static")
        .into_iter()
        .filter(|c| c.contains("/patch_"))
        .collect();
    assert_eq!(cases.len(), 4, "{cases:?}");
    let mut total = 0;
    for c in &cases {
        total += replay_static(c);
    }
    eprintln!(
        "static patch tests: {} cases, {total} values bitwise",
        cases.len()
    );
}

#[test]
fn loaded_cases_bitwise() {
    let cases: Vec<String> = cases_of("static")
        .into_iter()
        .filter(|c| c.contains("/loaded_"))
        .collect();
    assert_eq!(cases.len(), 2, "{cases:?}");
    let mut total = 0;
    for c in &cases {
        total += replay_loaded(c);
    }
    eprintln!(
        "loaded cases: {} cases, {total} values bitwise",
        cases.len()
    );
}

#[allow(dead_code)]
fn unused(_: GhostType) {}

/// Explicit central-difference runs (`explicit/*`): initial `u`, `v` from
/// the recorded step 0, `dt = 0.5 · getStableTimeStep()` (checked against
/// the recorded `dt`), every step's `u`, `v`, `a`, `f_int` and energies
/// bitwise (the lumped solver has no factorisation).
fn replay_explicit(name: &str) -> usize {
    let f = case(name);
    let mut model = model_of(&f);
    init(&mut model, AnalysisMethod::ExplicitLumpedMass);
    let sh = shape(&f, "displacement");
    let (steps, per) = (sh[0], sh[1] * sh[2]);
    let u_all = f64s(&f, "displacement");
    let v_all = f64s(&f, "velocity");
    model
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u_all[..per]);
    model
        .velocity_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&v_all[..per]);
    let dt = 0.5 * model.stable_time_step().unwrap();
    // `time[1] = 1 · dt` (exact npy value; the manifest's JSON `dt` is not
    // parsed correctly rounded by serde_json without `float_roundtrip`)
    let t1 = f64s(&f, "time")[1];
    assert!(
        akantu_validation::exact_eq(t1, dt),
        "{name}: dt {dt} vs {t1}"
    ); // D-046
    model.set_time_step(dt, None).unwrap();
    let (mut u, mut v, mut a, mut fi, mut ek, mut ep) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    let mut snap = |m: &mut akantu_solid::SolidMechanicsModel| {
        u.extend_from_slice(m.displacement().unwrap().as_slice());
        v.extend_from_slice(m.velocity().unwrap().as_slice());
        a.extend_from_slice(m.acceleration().unwrap().as_slice());
        fi.extend_from_slice(m.internal_force().unwrap().as_slice());
        ek.push(m.energy("kinetic").unwrap());
        ep.push(m.energy("potential").unwrap());
    };
    snap(&mut model);
    for _ in 1..steps {
        model.solve_step(None).unwrap();
        snap(&mut model);
    }
    let mut n = 0;
    n += assert_bits(&f, "displacement", &u);
    n += assert_bits(&f, "velocity", &v);
    n += assert_bits(&f, "acceleration", &a);
    n += assert_bits(&f, "internal_force", &fi);
    n += assert_bits(&f, "energy/kinetic", &ek);
    n += assert_bits(&f, "energy/potential", &ep);
    n
}

#[test]
fn explicit_cases_bitwise() {
    let cases = cases_of("explicit");
    assert_eq!(cases.len(), 2, "{cases:?}");
    let mut total = 0;
    for c in &cases {
        total += replay_explicit(c);
    }
    eprintln!(
        "explicit cases: {} cases, {total} values bitwise",
        cases.len()
    );
}

/// Tensor-layout sentinels (`layout/*`): the value `1000 k + 10 r + c + 1`
/// written at buffer component `r + c·dim` of tuple `k` of material 0's
/// stress reads back as `M(r, c)`; both the raw buffer and the transposed
/// tensor match the fixture exactly.
#[test]
fn layout_sentinels_exact() {
    let cases = cases_of("layout");
    assert_eq!(cases.len(), 2, "{cases:?}");
    for c in &cases {
        let f = case(c);
        let mut model = model_of(&f);
        init(&mut model, AnalysisMethod::Static);
        let dim = model.spatial_dimension();
        let types = {
            let mesh = model.physics.base.mesh.read().unwrap();
            types_of(&mesh, dim)
        };
        for ty in types {
            let law = model.material_mut(0).unwrap();
            let mut buf = law
                .base_mut()
                .stress
                .get_mut(ty, GhostType::NotGhost)
                .unwrap();
            for (k, t) in buf.as_mut_slice().chunks_exact_mut(dim * dim).enumerate() {
                for r in 0..dim {
                    for col in 0..dim {
                        t[r + col * dim] = 1000.0 * k as f64 + 10.0 * r as f64 + col as f64 + 1.0;
                    }
                }
            }
            let raw = buf.as_slice().to_vec();
            assert_bits(&f, &format!("stress_buffer/{}", tname(ty)), &raw);
            drop(buf);
            assert_bits(
                &f,
                &format!("stress/{}", tname(ty)),
                &law_tensor(&model, "stress", ty),
            );
        }
    }
}

/// `implicit_dynamic/patch_triangle_6_plastic` (replayed since
/// `plastic_linear_isotropic_hardening` is registered on the model): 5 Newmark (trapezoidal) steps of a 2-D (plane
/// strain) `_triangle_6` patch with the J2 law, `x = xmin` blocked, initial
/// `v_y = 0.05 (x − xmin)`, `Δt = 1e−4`, Newton–Raphson (residual
/// criterion, 1e−10, ≤ 50 iterations). The law runs in its upstream
/// compatibility mode (`LawOptions::upstream()`: UD-137 truncated 2-D
/// return, UD-138 trial hardening of earlier Newton iterations), the
/// configuration the oracle ran; the model is in its default mode.
///
/// The oracle solved with MUMPS (`auto` in that build, probe v3) and the
/// port with the Eigen `SparseLU` translation, so solved states are not
/// bitwise (as for `static/*`): bitwise are the consistent mass matrix and
/// its profile; exact are the blocked DOFs and the Newton iteration counts
/// of every step (2-D plasticity onset at step 2, 14–21 iterations, linear
/// contraction); `u`, `v`, `a` and the energies of every step are compared
/// with the reviewed rule of `fixtures/model/tolerances.json`
/// (`model/implicit_dynamic/patch_triangle_6_plastic`, rel 1e−10 of the
/// step maximum)
/// and the final solver errors with its `newton/error` rule (abs 1e−7 N,
/// also reviewed). Measured per step, relative to the step maximum (the
/// rule's `max_abs_per_step` scale): `u` ≤ 4.3e−16,
/// `v` ≤ 1.44e−14, `a` ≤ 2.9e−14, energies ≤ 7.5e−16; final Newton errors
/// within 4.8e−10 N absolute of the oracle's (2.8e−11 … 4.8e−10 N; step 1's
/// oracle residual, 1.06e−9 N, is rounding noise). Solver acceptance on the port's own residual
/// histories is not run here: the Newton solver exposes no per-iteration
/// norm history yet (follow-up); every step converged (else `solve_step`
/// fails).
#[test]
fn implicit_dynamic_plastic_patch_replay() {
    let f = case("implicit_dynamic/patch_triangle_6_plastic");
    let dim = param(&f, "spatial_dimension").as_u64().unwrap() as usize;
    let mut model = model_of(&f);
    model.physics.law_options = Some(akantu_solid::material::LawOptions::upstream());
    init(&mut model, AnalysisMethod::ImplicitDynamic);
    let x: Vec<Real> = {
        let mesh = model.physics.base.mesh.read().unwrap();
        mesh.nodes().as_slice().to_vec()
    };
    let nn = x.len() / dim;
    let xmin = (0..nn).map(|n| x[n * dim]).fold(Real::INFINITY, Real::min);
    {
        let mut b = model.blocked_dofs_mut().unwrap();
        for n in 0..nn {
            if (x[n * dim] - xmin).abs() < 1e-10 {
                for c in 0..dim {
                    b.as_mut_slice()[n * dim + c] = true;
                }
            }
        }
    }
    let blocked: Vec<bool> = model.blocked_dofs_mut().unwrap().as_slice().to_vec();
    assert_eq!(blocked, bools(&f, "blocked_dofs"));
    {
        let mut v = model.velocity_mut().unwrap();
        for n in 0..nn {
            v.as_mut_slice()[n * dim + 1] = 0.05 * (x[n * dim] - xmin);
        }
    }
    model.set_time_step(1e-4, None).unwrap();
    set_newton(&mut model, 50, 1e-10, "residual");
    model.assemble_mass().unwrap();
    let mut n = 0;
    {
        let m = model.dof_manager().matrix("M").unwrap();
        let (r, c, v) = m.coo();
        assert_eq!(r.to_vec(), i64s(&f, "M/rows"));
        assert_eq!(c.to_vec(), i64s(&f, "M/cols"));
        n += assert_bits(&f, "M/values", v);
    }
    let steps = shape(&f, "displacement")[0];
    let (mut u, mut v, mut a, mut ek, mut ep, mut its, mut errs) = (
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
        Vec::new(),
    );
    for _ in 0..steps {
        model.solve_step(None).unwrap();
        u.extend_from_slice(model.displacement().unwrap().as_slice());
        v.extend_from_slice(model.velocity().unwrap().as_slice());
        a.extend_from_slice(model.acceleration().unwrap().as_slice());
        ek.push(model.energy("kinetic").unwrap());
        ep.push(model.energy("potential").unwrap());
        let nls = model.solver.non_linear_solver(None).unwrap();
        assert!(nls.converged());
        its.push(nls.nb_iterations() as i64);
        errs.push(nls.error());
    }
    for (name, got) in [
        ("displacement", &u),
        ("velocity", &v),
        ("acceleration", &a),
        ("energy/kinetic", &ek),
        ("energy/potential", &ep),
        ("newton/error", &errs),
    ] {
        report(&f, name, got);
    }
    // exact: iteration counts (compared directly: checking the convergence
    // quantities through `DiffTest` runs solver acceptance, which needs the
    // per-iteration residual histories)
    assert_eq!(its, i64s(&f, "newton/nb_iterations"), "iterations");
    // fixtures/model/tolerances.json `model/implicit_dynamic/*` newton/error
    // rule (abs 1e-7 N), applied directly for the same reason
    for (k, (e, r)) in errs.iter().zip(f64s(&f, "newton/error")).enumerate() {
        assert!((e - r).abs() <= 1e-7, "step {}: error {e} vs {r}", k + 1);
    }
    let mut t =
        akantu_validation::DiffTest::with_fixture("smm_implicit_dynamic_plastic", f).unwrap();
    for (name, got) in [
        ("displacement", &u),
        ("velocity", &v),
        ("acceleration", &a),
        ("energy/kinetic", &ek),
        ("energy/potential", &ep),
    ] {
        t.check_f64(name, got);
        n += got.len();
    }
    if let Err(e) = t.finish() {
        panic!("{e}");
    }
    eprintln!(
        "implicit_dynamic/patch_triangle_6_plastic: {n} values within the rules (M bitwise), \
         iterations {its:?}"
    );
}
