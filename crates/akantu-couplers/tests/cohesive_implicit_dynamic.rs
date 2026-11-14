// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Implicit-dynamic
// solid–cohesive–contact coupling:
// src/model/model_couplers/coupler_solid_cohesive_contact.cc and
// coupler_solid_contact_tmpl.hh in `_implicit_dynamic` mode against the
// oracle cases `fixtures/model/couplers/cohesive_implicit_dynamic_{qs,
// dt0p01}` (probe oracle/probes/couplers/couplers_probe.cc version 2,
// `cohesive_implicit_dynamic`).

//! The `cohesive_static` set-up (cohesive–contact example mesh,
//! intrinsic `cohesive_linear`, `epsilon_n = 1e3`, compression of
//! `1e-3/50` per step, Newton at 100 iterations) in `_implicit_dynamic`
//! with the time steps `1e6` (quasi-static) and `1e-2`, replayed against
//! upstream **iterate by iterate**: every residual, and at every corrector
//! the solution increment and the displacement, of every step until
//! upstream's first Newton failure (a failed solve restores the DOFs, so
//! the end state of such a step says nothing), plus the per-step state
//! (displacement, velocity, acceleration, forces, gaps, pairings) and the
//! Newton outcomes. Upstream-compatible switches as the `cohesive_static`
//! replay: UD-772 (contact stiffness withheld from the solved matrix),
//! UD-374 (per-iterate friction history), UD-034 (symmetric `K` storage),
//! contact `all()`, cohesive laws upstream.

mod common;

use akantu_cohesive::model::CohesiveCallback;
use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::{CohesiveSurfaceSelector, ContactUpstreamCompat};
use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_couplers::{
    CohesiveContactSolverCallback, CouplerSolidCohesiveContact, SolidContactCallback,
    SolidContactCompat,
};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;
use akantu_validation::Fixture;
use common::trace::Traced;
use common::*;

const AREA: &str = "model/couplers";

fn eigen(method: AnalysisMethod) -> ModelOptions {
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// The probe's set-up in `_implicit_dynamic` with the time step of the
/// fixture and the coupler's switches `compat`; `upstream_k_storage` is the
/// UD-034 compat of the symmetric `K` storage.
fn setup(
    f: &Fixture,
    compat: SolidContactCompat,
    upstream_k_storage: bool,
) -> Box<CouplerSolidCohesiveContact> {
    setup_with(
        f,
        compat,
        upstream_k_storage,
        AnalysisMethod::ImplicitDynamic,
    )
}

/// [`setup`] with the analysis method `method` (the time step of the
/// fixture is set in implicit dynamics only).
fn setup_with(
    f: &Fixture,
    compat: SolidContactCompat,
    upstream_k_storage: bool,
    method: AnalysisMethod,
) -> Box<CouplerSolidCohesiveContact> {
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
    let mut c = Box::new(
        CouplerSolidCohesiveContact::with_parser(
            SharedMesh::new(mesh),
            None,
            "coupler_solid_contact",
            parser,
        )
        .unwrap(),
    );
    let sel = MeshDataMaterialCohesiveSelector {
        default: c.solid.default_cohesive_selector(),
    };
    c.solid.set_material_selector(Box::new(sel));
    c.contact
        .set_surface_selector(Box::new(CohesiveSurfaceSelector::new()))
        .unwrap();
    c.solid.ext.compat = akantu_cohesive::material::CohesiveUpstreamCompat::upstream();
    if upstream_k_storage {
        c.solid
            .set_symmetry_policy(akantu_linalg::SymmetryPolicy::UpstreamUpperSelection)
            .unwrap();
    }
    c.init_full(&eigen(method), false).unwrap();
    c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    c.set_upstream_compat(compat);
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    c.apply_dirichlet_bc(&fix, "sides").unwrap();
    if method == AnalysisMethod::ImplicitDynamic {
        let dt = f64s(f, "input/time_step")[0];
        c.set_time_step(dt, None).unwrap();
    }
    c.solid
        .smm
        .solver
        .non_linear_solver_mut(None)
        .unwrap()
        .set_param("max_iterations", ParamValue::Int(100))
        .unwrap();
    c
}

fn newton(c: &CouplerSolidCohesiveContact) -> (i64, f64) {
    let nls = c.solid.smm.solver.non_linear_solver(None).unwrap();
    let it = match nls.get_param("nb_iterations").unwrap() {
        ParamValue::Int(i) => i,
        o => panic!("{o:?}"),
    };
    let err = match nls.get_param("error").unwrap() {
        ParamValue::Real(e) => e,
        o => panic!("{o:?}"),
    };
    (it, err)
}

/// The per-iterate trace of one step: event codes (1 matrix K, 4 another
/// matrix, 2 residual, 3 corrector), residuals, and at every corrector the
/// solution increment and the displacement.
#[derive(Default)]
struct StepTrace {
    events: Vec<i64>,
    residual: Vec<f64>,
    solution: Vec<f64>,
    displacement: Vec<f64>,
}

/// One loading step through the coupler's callback (`ImplicitDynamic`)
/// with the switches `compat`; `Ok(converged)` and the trace.
fn traced_step(
    c: &mut CouplerSolidCohesiveContact,
    compat: SolidContactCompat,
    standalone: bool,
) -> (bool, StepTrace) {
    let inc: Real = 1e-3 / 50.0;
    for (value, group) in [(-inc, "loading"), (inc, "fixed")] {
        let bc = IncrementValue {
            value,
            axis: SpatialDirection::Y,
        };
        c.apply_dirichlet_bc(&bc, group).unwrap();
    }
    c.forward_new_cohesive_elements().unwrap();
    let mut t = StepTrace::default();
    let contact = &mut c.contact;
    let r = c.solid.smm.solve_step_with(None, |physics, solver, id| {
        let mut cohesive = CohesiveCallback::new(physics);
        let method = AnalysisMethod::ImplicitDynamic;
        let mut coupler_cb;
        let mut standalone_cb;
        let inner = if standalone {
            // UD-622 compat (`need_to_reassemble = false`) as upstream's
            // stand-alone callback
            standalone_cb = CohesiveContactSolverCallback::with_compat(
                &mut cohesive,
                contact,
                method,
                compat,
                true,
            );
            standalone_cb.as_callback()
        } else {
            coupler_cb =
                SolidContactCallback::coupler(&mut cohesive, contact, method).with_compat(compat);
            &mut coupler_cb
        };
        let mut cb = Traced {
            inner,
            on_event: |what: &str,
                       dofs: &akantu_solver::DofManager,
                       _: &SolidContactCallback<'_>| {
                match what {
                    "matrix:K" => t.events.push(1),
                    "residual" => {
                        t.events.push(2);
                        t.residual.extend_from_slice(&dofs.residual().values());
                    }
                    "corrector" => {
                        t.events.push(3);
                        t.solution.extend_from_slice(dofs.solution());
                        let u = dofs.dofs("displacement").unwrap();
                        t.displacement.extend_from_slice(u.as_slice());
                    }
                    w if w.starts_with("matrix:") => t.events.push(4),
                    _ => t.events.push(0),
                }
            },
        };
        solver.solve_step(&mut cb, id)
    });
    (r.is_ok(), t)
}

/// First differing entry of two sequences (bits), with the iterate number
/// for a trace of `width` values per event.
fn first_diff(name: &str, got: &[f64], want: &[f64], width: usize) -> Option<String> {
    if got.len() != want.len() {
        return Some(format!(
            "{name}: {} values ({} events) vs oracle {} ({} events)",
            got.len(),
            got.len() / width.max(1),
            want.len(),
            want.len() / width.max(1)
        ));
    }
    let k = (0..want.len()).find(|&k| !same_bits(got[k], want[k]))?;
    Some(format!(
        "{name}: first difference at event {} entry {}: {:e} vs oracle {:e}",
        k / width,
        k % width,
        got[k],
        want[k]
    ))
}

/// The whole upstream-compatible replay of a fixture: every step's trace
/// and state bitwise, the Newton outcomes. Returns the number of values
/// compared.
fn replay(f: &Fixture, standalone: bool) -> usize {
    replay_with(f, standalone, SolidContactCompat::all())
}

/// [`replay`] with the coupler's switches `compat`.
fn replay_with(f: &Fixture, standalone: bool, compat: SolidContactCompat) -> usize {
    let cid = f.case_id().to_owned();
    let mut c = setup(f, compat, true);
    let steps = usize::try_from(f.manifest().parameters["steps_run"].as_i64().unwrap()).unwrap();
    let converged = i64s(f, "newton/converged");
    let iterations = i64s(f, "newton/iterations");
    let errors = f64s(f, "newton/error");
    let nb_contact = i64s(f, "nb_contact_elements");
    assert_eq!(converged.len(), steps);
    let mut n = 0;
    for s in 0..steps {
        let (ok, t) = traced_step(&mut c, compat, standalone);
        let pre = format!("trace/step_{s}");
        assert_eq!(
            t.events,
            i64s(f, &format!("{pre}/events")),
            "{cid} step {s}: callback events"
        );
        let width = 36;
        for (name, got) in [
            ("residual", &t.residual),
            ("solution", &t.solution),
            ("displacement", &t.displacement),
        ] {
            let q = format!("{pre}/{name}");
            let want = if f.manifest().quantities.iter().any(|x| x.name == q) {
                f64s(f, &q)
            } else {
                Vec::new()
            };
            if let Some(d) = first_diff(name, got, &want, width) {
                panic!("{cid} step {s}: {d}");
            }
            n += want.len();
        }
        assert_eq!(ok, converged[s] == 1, "{cid} step {s}: Newton outcome");
        let (it, err) = newton(&c);
        assert_eq!(it, iterations[s], "{cid} step {s}: iterations");
        assert!(
            same_bits(err, errors[s]),
            "{cid} step {s}: error {err:e} vs {:e}",
            errors[s]
        );
        assert_eq!(
            i64::try_from(c.contact.contact_elements().len()).unwrap(),
            nb_contact[s],
            "{cid} step {s}: contact elements"
        );
        let smm = &c.solid.smm;
        for (name, got) in [
            (
                "displacement",
                smm.displacement().unwrap().as_slice().to_vec(),
            ),
            ("velocity", smm.velocity().unwrap().as_slice().to_vec()),
            (
                "acceleration",
                smm.acceleration().unwrap().as_slice().to_vec(),
            ),
            (
                "internal_force",
                smm.internal_force().unwrap().as_slice().to_vec(),
            ),
            (
                "contact_force",
                c.contact.internal_force().as_slice().to_vec(),
            ),
            ("gaps", c.contact.gaps().as_slice().to_vec()),
        ] {
            let want = f64s(f, &format!("step_{s}/{name}"));
            assert_bits(&cid, &format!("step_{s}/{name}"), &got, &want);
            n += want.len();
        }
    }
    n
}

/// Quasi-static time step (`Δt = 1e6`): the port replays upstream's
/// implicit-dynamic run iterate by iterate until upstream's first Newton
/// failure (the second step: UD-772 withholds the contact stiffness from
/// the solved matrix).
#[test]
fn cohesive_implicit_dynamic_quasi_static_matches_the_oracle_iterate_by_iterate() {
    let f = cases(AREA, "cohesive_implicit_dynamic_qs").remove(0);
    let n = replay(&f, false);
    eprintln!("cohesive_implicit_dynamic_qs: {n} values bitwise");
}

/// Dynamic time step (`Δt = 1e-2`): upstream's Newton fails at the
/// second step as the port's; the iterates are bitwise equal up to there.
#[test]
fn cohesive_implicit_dynamic_dt0p01_matches_the_oracle_iterate_by_iterate() {
    let f = cases(AREA, "cohesive_implicit_dynamic_dt0p01").remove(0);
    let n = replay(&f, false);
    eprintln!("cohesive_implicit_dynamic_dt0p01: {n} values bitwise");
}

/// CPL-004: the stand-alone
/// `CohesiveContactSolverCallback` (upstream switches, UD-622's
/// `need_to_reassemble = false` included) in implicit dynamics gives
/// upstream's coupler run iterate by iterate, both time steps (upstream's
/// stand-alone callback runs the coupler's logic; UD-622 has no reachable
/// effect with cohesive laws). This replaces the static-limit bound of the
/// stand-alone callback's first step (`1e-8`, margin 1.8) as the
/// evidence of its implicit-dynamic path.
#[test]
fn standalone_callback_implicit_dynamic_matches_the_oracle_iterate_by_iterate() {
    for case in [
        "cohesive_implicit_dynamic_qs",
        "cohesive_implicit_dynamic_dt0p01",
    ] {
        let f = cases(AREA, case).remove(0);
        let n = replay(&f, true);
        eprintln!("{case} (stand-alone callback): {n} values bitwise");
    }
}

/// UD-1291: which of these replays need
/// `search_in_corrector_only`. The Newmark predictor moves every free node,
/// the crack faces included, so with the corrected order (every other
/// switch upstream's) the quasi-static replay departs from upstream at the
/// first residual of step 1, through the coupler's callback and through
/// the stand-alone callback alike (the first step starts from rest:
/// nothing moves in its predictor). The `Δt = 1e-2` replay stays bitwise:
/// up to upstream's failure at its second step the predictor search finds
/// the corrector's last contact set. A switch ignored by either callback
/// fails this test or the replays above.
#[test]
fn ud1291_only_the_quasi_static_replay_needs_the_switch() {
    let corrected = SolidContactCompat {
        search_in_corrector_only: false,
        ..SolidContactCompat::all()
    };
    for standalone in [false, true] {
        let f = cases(AREA, "cohesive_implicit_dynamic_qs").remove(0);
        let r = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            replay_with(&f, standalone, corrected)
        }));
        let msg = match r {
            Ok(n) => panic!("standalone = {standalone}: still bitwise ({n} values)"),
            Err(e) => e
                .downcast_ref::<String>()
                .cloned()
                .unwrap_or_else(|| "non-string panic".into()),
        };
        assert!(
            msg.contains("step 1: residual: first difference at event 0 "),
            "standalone = {standalone}: {msg}"
        );
        let f = cases(AREA, "cohesive_implicit_dynamic_dt0p01").remove(0);
        let n = replay_with(&f, standalone, corrected);
        eprintln!(
            "standalone = {standalone}: qs departs at step 1, event 0; dt0p01 bitwise ({n} values)"
        );
    }
}

/// Corrected default (contact stiffness solved, UD-772): with the
/// quasi-static time step the run departs from the static one from the
/// second step on (2.2e-2 relative), and the fifth step does not converge,
/// because the Newmark predictor extrapolates the previous increment
/// (`u + Δt·v + …` with `v ≈ 2Δu/Δt` after a step from rest): the Newton
/// solve of this path-dependent problem (intrinsic cohesive law, penalty
/// contact) starts beyond the static path and converges to another
/// equilibrium branch. Restarting the predictor from rest before every step
/// (velocity and acceleration zeroed, the static start) gives the static
/// run at every step and every step converges. The bound `1e-6` separates
/// "the static branch" (observed ≤ 1.8e-8: two Newton solutions with
/// different iteration histories; the mass term is `M/(βΔt²)`, about 1e-12
/// of `M`) from "another branch" (≥ 2.2e-2); it is no accuracy claim
/// (`docs/NUMERICAL_VALIDATION.md`, PROPOSED).
#[test]
fn quasi_static_departure_comes_from_the_newmark_predictor() {
    let f = cases(AREA, "cohesive_implicit_dynamic_qs").remove(0);
    let fst = cases(AREA, "cohesive_static").remove(0);
    let compat = SolidContactCompat::default();
    let inc: Real = 1e-3 / 50.0;
    let mut out = Vec::new();
    for restart in [false, true] {
        let mut st = setup_with(&fst, compat, false, AnalysisMethod::Static);
        let mut dy = setup(&f, compat, false);
        let mut diffs = Vec::new();
        for _ in 0..5 {
            for c in [&mut st, &mut dy] {
                for (value, group) in [(-inc, "loading"), (inc, "fixed")] {
                    let bc = IncrementValue {
                        value,
                        axis: SpatialDirection::Y,
                    };
                    c.apply_dirichlet_bc(&bc, group).unwrap();
                }
                c.forward_new_cohesive_elements().unwrap();
            }
            if restart {
                dy.solid
                    .smm
                    .velocity_mut()
                    .unwrap()
                    .as_mut_slice()
                    .fill(0.0);
                dy.solid
                    .smm
                    .acceleration_mut()
                    .unwrap()
                    .as_mut_slice()
                    .fill(0.0);
            }
            st.solve_step(None).unwrap();
            let converged = dy.solve_step(None).is_ok();
            let us = st.solid.smm.displacement().unwrap().as_slice().to_vec();
            let ud = dy.solid.smm.displacement().unwrap().as_slice().to_vec();
            let scale = us.iter().fold(0.0f64, |m, x| m.max(x.abs()));
            let d = us
                .iter()
                .zip(&ud)
                .fold(0.0f64, |m, (a, b)| m.max((a - b).abs()))
                / scale;
            diffs.push((converged, d));
        }
        out.push(diffs);
    }
    eprintln!("predictor as upstream: {:?}", out[0]);
    eprintln!("predictor restarted from rest: {:?}", out[1]);
    // as upstream's scheme: the first step (from rest) is static, then the
    // run leaves the static branch and the fifth step fails
    assert!(out[0][0].0 && out[0][0].1 <= 1e-6, "{:?}", out[0][0]);
    for (s, &(ok, d)) in out[0].iter().enumerate().skip(1) {
        assert!(d >= 1e-2, "step {s}: {d:e}");
        assert_eq!(ok, s < 4, "step {s}: converged");
    }
    // restarted from rest: the static branch at every step
    for (s, &(ok, d)) in out[1].iter().enumerate() {
        assert!(ok && d <= 1e-6, "restarted step {s}: converged {ok}, {d:e}");
    }
}
