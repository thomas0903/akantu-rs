// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). UD-374
// (conditions): the friction history
// of the static / implicit couplers of
// src/model/model_couplers/coupler_solid_contact_tmpl.hh (`corrector` →
// `contact->search()` → `savePreviousState`), with the corrected default
// (history committed once per converged step) and the compat switch
// (upstream's save at every Newton iterate), on the Hertz example of
// examples/c++/contact_mechanics_model/hertz with friction.

mod common;

use akantu_contact::{ContactUpstreamCompat, PhysicalSurfaceSelector};
use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_couplers::{CouplerSolidContact, SolidContactCallback, SolidContactCompat};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_model::constitutive_law::ElementDataSelector;
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::{DofManager, ModelSolverOptions};
use common::trace::Traced;
use common::*;

const HERTZ_DAT: &str = include_str!("data/contact/hertz_material.dat");

/// Static Hertz coupler with friction coefficient `mu`; `contact_all`:
/// the contact model's upstream switches (`all()`, incl. upstream's
/// friction moduli), else its corrected defaults (consistent tangent).
fn hertz(mu: Real, contact_all: bool, compat: SolidContactCompat) -> CouplerSolidContact {
    let dat = HERTZ_DAT.replace("mu = 0.0", &format!("mu = {mu}"));
    assert_ne!(dat, HERTZ_DAT);
    let mesh = read_mesh(
        &frozen_mesh("examples/c++/contact_mechanics_model/hertz/hertz.msh"),
        2,
    );
    let parser = Parser::parse_str(&dat, "material.dat").unwrap();
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
    c.init_full(
        &ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Auto),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        }),
    )
    .unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    if contact_all {
        c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    }
    c.set_upstream_compat(compat).unwrap();
    let fix = |axis| FixedValue { value: 0.0, axis };
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::Y), "fixed")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "loading")
        .unwrap();
    c.apply_dirichlet_bc(&fix(SpatialDirection::X), "symmetry")
        .unwrap();
    set_max_iterations(&mut c, 100);
    c
}

fn set_max_iterations(c: &mut CouplerSolidContact, n: i64) {
    c.solid
        .solver
        .non_linear_solver_mut(None)
        .unwrap()
        .set_param("max_iterations", ParamValue::Int(n))
        .unwrap();
}

fn compat(per_iteration_friction_history: bool) -> SolidContactCompat {
    SolidContactCompat {
        per_iteration_friction_history,
        ..SolidContactCompat::default()
    }
}

fn bits(a: &[Real]) -> Vec<u64> {
    a.iter().map(|v| v.to_bits()).collect()
}

/// The `previous_*` history (projections, tangents, tractions, masters).
fn history(c: &CouplerSolidContact) -> Vec<Vec<u64>> {
    let f = &c.contact.fields;
    vec![
        bits(f.previous_projections.as_slice()),
        bits(f.previous_tangents.as_slice()),
        bits(f.previous_tangential_tractions.as_slice()),
        f.previous_master_elements
            .as_slice()
            .iter()
            .map(|e| {
                if e.is_null() {
                    u64::MAX
                } else {
                    e.index() as u64
                }
            })
            .collect(),
    ]
}

/// The current fields the history is taken from (same layout).
fn current(c: &CouplerSolidContact) -> Vec<Vec<u64>> {
    let f = &c.contact.fields;
    let mut masters = vec![u64::MAX; f.projections.len()];
    for e in c.contact.contact_elements() {
        masters[e.slave as usize] = e.master.index() as u64;
    }
    vec![
        bits(f.projections.as_slice()),
        bits(f.tangents.as_slice()),
        bits(f.tangential_tractions.as_slice()),
        masters,
    ]
}

/// What a traced step records.
#[derive(Debug, Default)]
struct StepTrace {
    /// `previous_*` (projections, tangents, tractions) after each
    /// corrector's search.
    history: Vec<Vec<Vec<u64>>>,
    /// The history sources after each residual assembly (projections and
    /// tangents of the last search, tractions of the force assembly).
    after_residual: Vec<Vec<Vec<u64>>>,
    /// `previous_*` (projections, tangents, tractions) at each residual
    /// assembly (UD-1291: the first one follows the predictor's search).
    history_at_residual: Vec<Vec<Vec<u64>>>,
    /// Displacement after each corrector.
    u: Vec<Vec<Real>>,
}

impl StepTrace {
    /// `‖u_k − u_{k−1}‖` of the Newton iterates (the prescribed DOFs move
    /// in the first iteration only).
    fn increments(&self) -> Vec<Real> {
        self.u
            .windows(2)
            .map(|w| {
                w[1].iter()
                    .zip(&w[0])
                    .map(|(a, b)| (a - b) * (a - b))
                    .sum::<Real>()
                    .sqrt()
            })
            .collect()
    }
}

/// One increment of `inc` on `loading` along `axis` through the coupler's
/// callback with the coupler's switches, recording every corrector and
/// residual event.
fn traced_step(
    c: &mut CouplerSolidContact,
    axis: SpatialDirection,
    inc: Real,
) -> (bool, StepTrace) {
    c.apply_dirichlet_bc(&IncrementValue { value: inc, axis }, "loading")
        .unwrap();
    let compat = c.upstream_compat();
    let mut tr = StepTrace::default();
    let contact = &mut c.contact;
    let r = c.solid.solve_step_with(None, |physics, solver, id| {
        let mut inner = SolidContactCallback::coupler(physics, contact, AnalysisMethod::Static)
            .with_compat(compat);
        let mut cb = Traced {
            inner: &mut inner,
            on_event: |what: &str, dofs: &DofManager, cbk: &SolidContactCallback<'_>| {
                let f = &cbk.contact.fields;
                match what {
                    "corrector" => {
                        tr.history.push(vec![
                            bits(f.previous_projections.as_slice()),
                            bits(f.previous_tangents.as_slice()),
                            bits(f.previous_tangential_tractions.as_slice()),
                        ]);
                        tr.u.push(dofs.dofs("displacement").unwrap().as_slice().to_vec());
                    }
                    "residual" => {
                        tr.after_residual.push(vec![
                            bits(f.projections.as_slice()),
                            bits(f.tangents.as_slice()),
                            bits(f.tangential_tractions.as_slice()),
                        ]);
                        tr.history_at_residual.push(vec![
                            bits(f.previous_projections.as_slice()),
                            bits(f.previous_tangents.as_slice()),
                            bits(f.previous_tangential_tractions.as_slice()),
                        ]);
                    }
                    _ => {}
                }
            },
        };
        solver.solve_step(&mut cb, id)
    });
    (r.is_ok(), tr)
}

/// Three compression steps of −1e-4 (the contact zone is established).
fn compressed(mu: Real, contact_all: bool, per_iteration: bool) -> CouplerSolidContact {
    let mut c = hertz(mu, contact_all, compat(per_iteration));
    for s in 0..3 {
        let (ok, _) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
        assert!(ok, "compression step {s} failed");
    }
    c
}

fn state_codes(c: &CouplerSolidContact) -> Vec<i32> {
    c.contact
        .contact_elements()
        .iter()
        .map(|e| {
            akantu_contact::contact_state_code(c.contact.contact_state().tuple(e.slave as usize)[0])
        })
        .collect()
}

/// Corrected default: inside a step every corrector searches without
/// touching the history, which is the state committed by the previous
/// converged step; a converged step commits its final search and tractions.
#[test]
fn ud374_history_is_committed_once_per_converged_step() {
    let mut c = compressed(0.1, false, false);
    for s in 3..8 {
        let committed = history(&c);
        let (ok, tr) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
        assert!(ok, "step {s} failed");
        assert!(tr.history.len() >= 2, "step {s}: one iteration only");
        for (k, h) in tr.history.iter().enumerate() {
            assert_eq!(
                h[..],
                committed[..3],
                "step {s}, iteration {k}: history moved"
            );
        }
        assert_eq!(history(&c), current(&c), "step {s}: commit");
        assert!(
            c.contact
                .fields
                .previous_tangential_tractions
                .as_slice()
                .iter()
                .any(|&t| t != 0.0),
            "step {s}: non-zero traction history expected"
        );
    }
}

/// Compat (UD-374, upstream): the history saved by the search of iteration
/// k is the state after iteration k − 1 (its search and its force
/// assembly), as upstream's `savePreviousState` at the start of every
/// `search()` (shown on the oracle by).
#[test]
fn ud374_compat_saves_the_history_at_every_iterate() {
    let mut c = compressed(0.1, false, true);
    let (ok, tr) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
    assert!(ok);
    assert!(tr.history.len() >= 3);
    // residual events: one before the loop, then one after every corrector
    for k in 1..tr.history.len() {
        assert_eq!(tr.history[k], tr.after_residual[k], "iteration {k}");
    }
}

/// A failed step does not commit: the next attempt starts from the history
/// of the last converged step.
#[test]
fn ud374_failed_step_keeps_the_committed_history() {
    let mut c = compressed(0.1, false, false);
    let committed = history(&c);
    set_max_iterations(&mut c, 1);
    let (ok, _) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
    assert!(!ok, "one Newton iteration should not converge");
    assert_eq!(
        history(&c),
        committed,
        "a failed step committed its history"
    );
}

/// UD-1291 with the UD-374 default (the friction history committed once
/// per converged step): the search before the first residual of a step
/// keeps the history, in both orders. After a failed step (whose last
/// iterate searched and assembled, but committed nothing) the retry's
/// first residual and every corrector see the history of the last
/// converged step. A predictor search that saved the history
/// (`search()` instead of `search_keeping_history()`) would make the
/// failed iterate's state the history of the retry (mutant M-HIST-1 of
/// the UD-1291 fix); one that committed after searching would zero the traction
/// history (M-HIST-2, also caught by
/// `ud374_history_is_committed_once_per_converged_step`).
#[test]
fn ud1291_predictor_search_keeps_the_committed_history() {
    for search_in_corrector_only in [false, true] {
        let mut c = hertz(
            0.1,
            false,
            SolidContactCompat {
                search_in_corrector_only,
                ..SolidContactCompat::default()
            },
        );
        for s in 0..3 {
            let (ok, _) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
            assert!(ok, "compression step {s} failed");
        }
        let committed = history(&c);
        set_max_iterations(&mut c, 1);
        let (ok, failed) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
        assert!(!ok, "one Newton iteration should not converge");
        assert_ne!(
            failed.after_residual.last().map(|h| &h[..]),
            Some(&committed[..3]),
            "the failed iterate should differ from the committed state"
        );
        set_max_iterations(&mut c, 100);
        let (ok, tr) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
        assert!(ok, "the retry failed");
        assert!(tr.history.len() >= 2, "one iteration only");
        for (k, h) in tr.history_at_residual.iter().enumerate() {
            assert_eq!(
                h[..],
                committed[..3],
                "search_in_corrector_only = {search_in_corrector_only}: residual {k} of the \
                 retry: the history is not the last converged step's"
            );
        }
        for (k, h) in tr.history.iter().enumerate() {
            assert_eq!(
                h[..],
                committed[..3],
                "search_in_corrector_only = {search_in_corrector_only}: corrector {k} of the retry"
            );
        }
        assert_eq!(history(&c), current(&c), "the retry commits");
    }
}

/// UD-1291 on the Hertz setup: the load increments act on the `loading`
/// edge only, which no contact node belongs to, so the predictor moves no
/// contact node and the search before the first residual finds exactly the
/// previous step's last search: both orders give the same run bit for bit,
/// with either friction history. With the per-iterate history this also
/// shows that the predictor's search does not advance the history (a
/// predictor search that saved it, mutant M2a, makes the
/// orders differ there).
#[test]
fn ud1291_search_order_is_inert_when_the_predictor_moves_no_contact_node() {
    for per_iteration in [false, true] {
        let run = |search_in_corrector_only: bool| {
            let mut c = hertz(
                0.1,
                false,
                SolidContactCompat {
                    per_iteration_friction_history: per_iteration,
                    search_in_corrector_only,
                    ..SolidContactCompat::default()
                },
            );
            let mut out = Vec::new();
            for s in 0..6 {
                let (ok, tr) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
                assert!(ok, "step {s}");
                out.push((
                    bits(c.displacement().unwrap().as_slice()),
                    bits(c.contact.internal_force().as_slice()),
                    tr.history_at_residual,
                    tr.u.len(),
                ));
            }
            out
        };
        assert!(
            run(false) == run(true),
            "per_iteration_friction_history = {per_iteration}: the orders differ"
        );
    }
}

/// Order estimates `p_k = ln(e_{k+1}/e_k) / ln(e_k/e_{k−1})` of a sequence
/// of increment norms, above the rounding floor.
fn orders(e: &[Real], floor: Real) -> Vec<Real> {
    (1..e.len().saturating_sub(1))
        .filter(|&k| e[k + 1] > floor)
        .map(|k| (e[k + 1] / e[k]).ln() / (e[k] / e[k - 1]).ln())
        .collect()
}

/// Corrected default with the consistent friction tangent: compression
/// steps in which edge slaves of the Hertz contact slip converge
/// superlinearly in 4 iterations (UD-374 + UD-369 corrections together;
/// with the nodal areas recomputed at every search the tangent lacks
/// `∂A/∂u`, so the order is 1.4–1.8 rather than 2). Under the compat
/// switch the per-iterate history redefines the friction residual at every
/// iteration: the iteration converges to a different state, in which no
/// slave slips (each iterate's trial slip is the last correction only),
/// and that state is not the solution of the increments.
#[test]
fn slipping_step_converges_superlinearly_with_the_step_history() {
    let mut d = compressed(0.1, false, false);
    let mut p = compressed(0.1, false, true);
    let mut slipping_steps = 0;
    let mut nb_before = d.contact.contact_elements().len();
    for s in 3..10 {
        let (ok, tr) = traced_step(&mut d, SpatialDirection::Y, -1e-4);
        assert!(ok, "default: step {s} failed");
        let states = state_codes(&d);
        let inc = tr.increments();
        let rel: Vec<Real> = inc.iter().map(|v| v / inc[0]).collect();
        let o = orders(&rel, 1e-12);
        eprintln!("default step {s}: states {states:?}, |du_k|/|du_1| = {rel:?}, orders {o:?}");
        // slipping steps without an active-set change (a new contact
        // element makes the first correction a jump, not a Newton step)
        let same_set = states.len() == nb_before;
        nb_before = states.len();
        if states.contains(&2) && same_set {
            slipping_steps += 1;
            let last = o
                .iter()
                .rev()
                .take(2)
                .copied()
                .fold(Real::NEG_INFINITY, Real::max);
            // superlinear (measured orders 1.37…1.84, 4 iterations): not
            // exactly quadratic because the coupler recomputes the nodal
            // areas at every search and the contact tangent has no ∂A/∂u
            // (docs/NUMERICAL_VALIDATION.md)
            assert!(
                last > 1.2 && tr.u.len() <= 4,
                "default step {s}: order {last}, {} iterations ({rel:?})",
                tr.u.len()
            );
        }
        let (okp, trp) = traced_step(&mut p, SpatialDirection::Y, -1e-4);
        assert!(okp, "compat: step {s} failed");
        let states_p = state_codes(&p);
        eprintln!(
            "compat step {s}: states {states_p:?}, {} iterations",
            trp.u.len()
        );
        assert!(
            !states_p.contains(&2),
            "compat: the per-iterate history let a slave slip"
        );
    }
    assert!(
        slipping_steps >= 3,
        "{slipping_steps} slipping steps in the default run"
    );
    let ud_g = d.displacement().unwrap();
    let ud = ud_g.as_slice();
    let up_g = p.displacement().unwrap();
    let up = up_g.as_slice();
    let num: Real = ud
        .iter()
        .zip(up)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<Real>()
        .sqrt();
    let den: Real = ud.iter().map(|a| a * a).sum::<Real>().sqrt();
    eprintln!("|u_default - u_compat|/|u_default| = {:e}", num / den);
    assert!(num / den > 1e-6, "the two histories gave the same solution");
}

/// With upstream's friction moduli as well (full upstream behaviour: the
/// configuration of the oracle case `contact_static_friction`, UD-001
/// variant), the per-iterate history does not converge at the second
/// compression step (100 iterations), while the step history with the same
/// moduli converges at that step.
#[test]
fn upstream_friction_moduli_with_per_iterate_history_fail_at_step_1() {
    let mut c = hertz(0.3, true, SolidContactCompat::upstream());
    let (ok0, _) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
    assert!(ok0, "step 0");
    let (ok1, tr) = traced_step(&mut c, SpatialDirection::Y, -1e-4);
    assert!(!ok1, "step 1 converged in {} iterations", tr.u.len());
    let mut s = hertz(0.3, true, compat(false));
    for k in 0..2 {
        let (ok, _) = traced_step(&mut s, SpatialDirection::Y, -1e-4);
        assert!(ok, "step history, step {k}");
    }
}

/// The oracle case `contact_static_friction` (UD-001 zero-init variant:
/// upstream's friction is defined there): Hertz with `mu = 0.3` through the
/// static coupler, full upstream behaviour (contact `all()`, the coupler's
/// `upstream()`: per-iterate history, search in the corrector only),
/// compression steps until upstream's Newton failure: every step's state
/// and the digests of the residual, the solution, the gaps and the
/// tangential tractions at every callback event replayed bitwise.
#[test]
fn contact_static_friction_fixture_upstream_compat() {
    replay_contact_static_friction(SolidContactCompat::upstream());
}

/// UD-1291: the same replay with the corrected search order (the
/// per-iterate history kept): bitwise too, because the predictor moves no
/// contact node, so the search before the first residual finds the
/// previous step's last search. (That this search does not advance the
/// per-iterate history is shown by
/// `ud1291_search_order_is_inert_when_the_predictor_moves_no_contact_node`,
/// which kills mutant M2a; this replay does not.)
#[test]
fn contact_static_friction_fixture_corrected_search_order() {
    replay_contact_static_friction(SolidContactCompat {
        search_in_corrector_only: false,
        ..SolidContactCompat::upstream()
    });
}

fn replay_contact_static_friction(compat: SolidContactCompat) {
    let f = cases("model/couplers", "contact_static_friction").remove(0);
    let cid = format!("{} ({compat:?})", f.case_id());
    let v = &f.manifest().parameters["oracle_variant"];
    assert_eq!(
        v["defect"].as_str(),
        Some("UD-001"),
        "{cid}: oracle variant label"
    );
    let mesh = read_mesh(&f.input_path("mesh").unwrap(), 2);
    let parser = Parser::parse_file(f.input_path("material").unwrap()).unwrap();
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
    c.init_full(
        &ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Auto),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..ModelSolverOptions::default()
        }),
    )
    .unwrap();
    c.contact.set_surface_selector(Box::new(selector)).unwrap();
    c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    c.set_upstream_compat(compat).unwrap();
    let fix = |axis| FixedValue { value: 0.0, axis };
    for (axis, group) in [
        (SpatialDirection::X, "fixed"),
        (SpatialDirection::Y, "fixed"),
        (SpatialDirection::X, "loading"),
        (SpatialDirection::X, "symmetry"),
    ] {
        c.apply_dirichlet_bc(&fix(axis), group).unwrap();
    }
    set_max_iterations(&mut c, 100);
    let steps_run = f.manifest().parameters["steps_run"].as_i64().unwrap() as usize;
    let expected_events = read_input_text(&f, "events");
    let mut events = String::new();
    let digests = read_input_text(&f, "digests");
    let mut lines = digests.lines();
    let (mut conv, mut its, mut errs, mut nbs) = (vec![], vec![], vec![], vec![]);
    let mut compared = 0;
    for s in 0..steps_run {
        c.apply_dirichlet_bc(
            &IncrementValue {
                value: -1e-4,
                axis: SpatialDirection::Y,
            },
            "loading",
        )
        .unwrap();
        let contact = &mut c.contact;
        let ev = &mut events;
        let r = c.solid.solve_step_with(None, |physics, solver, id| {
            let mut inner = SolidContactCallback::coupler(physics, contact, AnalysisMethod::Static)
                .with_compat(compat);
            let mut cb = Traced {
                inner: &mut inner,
                on_event: |what: &str, dofs: &DofManager, cbk: &SolidContactCallback<'_>| {
                    ev.push_str(&format!("{s} {what}"));
                    match what {
                        "residual" => {
                            ev.push_str(&format!(" {}", sha_hex(&dofs.residual().values())))
                        }
                        "corrector" => {
                            let ct = &*cbk.contact;
                            ev.push_str(&format!(
                                " {} {} {} {}",
                                sha_hex(dofs.solution()),
                                sha_hex(ct.gaps().as_slice()),
                                sha_hex(ct.tangential_tractions().as_slice()),
                                ct.contact_elements().len()
                            ));
                        }
                        _ => {}
                    }
                    ev.push('\n');
                },
            };
            solver.solve_step(&mut cb, id)
        });
        let nls = c.solid.solver.non_linear_solver(None).unwrap();
        let ParamValue::Int(it) = nls.get_param("nb_iterations").unwrap() else {
            panic!()
        };
        let ParamValue::Real(err) = nls.get_param("error").unwrap() else {
            panic!()
        };
        conv.push(i64::from(r.is_ok()));
        its.push(it);
        errs.push(err);
        nbs.push(c.contact.contact_elements().len() as i64);
        let u = c.displacement().unwrap().as_slice().to_vec();
        assert_eq!(
            lines.next().unwrap(),
            format!(
                "{s} {} {} {}",
                sha_hex(&u),
                sha_hex(c.contact.internal_force().as_slice()),
                sha_hex(c.contact.gaps().as_slice())
            ),
            "{cid}: step {s}: digests"
        );
        let pre = format!("step_{s}");
        for (what, got) in [
            ("displacement", u.as_slice()),
            ("contact_force", c.contact.internal_force().as_slice()),
            (
                "tangential_force",
                c.contact.fields.tangential_force.borrow().as_slice(),
            ),
            ("gaps", c.contact.gaps().as_slice()),
            (
                "tangential_tractions",
                c.contact.tangential_tractions().as_slice(),
            ),
            ("positions", c.contact.positions().as_slice()),
        ] {
            let name = format!("{pre}/{what}");
            let e = f64s(&f, &name);
            assert_bits(&cid, &name, got, &e);
            compared += e.len();
        }
    }
    // the per-event digests, line by line (first difference reported)
    for (k, (g, e)) in events.lines().zip(expected_events.lines()).enumerate() {
        assert_eq!(g, e, "{cid}: callback event {k}");
    }
    assert_eq!(
        events.lines().count(),
        expected_events.lines().count(),
        "{cid}: number of callback events"
    );
    assert_eq!(conv, i64s(&f, "newton/converged"), "{cid}: converged");
    assert_eq!(its, i64s(&f, "newton/iterations"), "{cid}: iterations");
    assert_eq!(
        nbs,
        i64s(&f, "nb_contact_elements"),
        "{cid}: contact elements"
    );
    assert_bits(&cid, "newton/error", &errs, &f64s(&f, "newton/error"));
    eprintln!(
        "{cid}: {steps_run} steps (Newton {its:?}, converged {conv:?}), {} callback events and {compared} values bitwise",
        events.lines().count()
    );
}
