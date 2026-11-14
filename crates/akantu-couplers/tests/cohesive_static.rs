// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Static
// solid–cohesive–contact coupling:
// src/model/model_couplers/coupler_solid_cohesive_contact.cc and
// coupler_solid_contact_tmpl.hh in `_static` mode against the oracle case
// `fixtures/model/couplers/cohesive_static` (probe
// oracle/probes/couplers/couplers_probe.cc, `cohesive_static`).

mod common;

use std::collections::BTreeMap;

use akantu_cohesive::model::CohesiveCallback;
use akantu_cohesive::selector::MeshDataMaterialCohesiveSelector;
use akantu_contact::{CohesiveSurfaceSelector, ContactUpstreamCompat};
use akantu_core::Real;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType, SpatialDirection};
use akantu_couplers::{CouplerSolidCohesiveContact, SolidContactCallback, SolidContactCompat};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, IncrementValue};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solver::ModelSolverOptions;
use akantu_validation::Fixture;
use common::trace::{Traced, triplets};
use common::*;

const AREA: &str = "model/couplers";

/// The fixture: `CPL_COHESIVE_STATIC_DIR` (a staged probe output, for
/// development) or the committed golden.
fn fixture() -> Fixture {
    match std::env::var("CPL_COHESIVE_STATIC_DIR") {
        Ok(d) => Fixture::open(std::path::Path::new(&d)).unwrap(),
        Err(_) => cases(AREA, "cohesive_static").remove(0),
    }
}

fn eigen_static() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::Static).with_solver_options(ModelSolverOptions {
        non_linear_solver_type: Some(NonLinearSolverType::Auto),
        sparse_solver_type: Some(SparseSolverType::Eigen),
        ..ModelSolverOptions::default()
    })
}

/// The probe's setup: example mesh and material (`epsilon_n = 1e3`),
/// intrinsic cohesive elements (upstream cohesive laws, UD-224: zero
/// strength), cohesive surface selector, `_static`, sides fixed in x,
/// Newton at 100 iterations; contact upstream-compatible (`all()`); the
/// coupler's switches `compat`.
fn setup(
    f: &Fixture,
    compat: SolidContactCompat,
    upstream_k_storage: bool,
) -> CouplerSolidCohesiveContact {
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
    if upstream_k_storage {
        // UD-034 compat: upstream keeps K symmetric and adds only the global
        // upper triangle of the unsymmetric cohesive tangent (the port
        // promotes K to unsymmetric storage by default). Without it the
        // replay departs at Newton iteration 2.
        c.solid
            .set_symmetry_policy(akantu_linalg::SymmetryPolicy::UpstreamUpperSelection)
            .unwrap();
    }
    c.init_full(&eigen_static(), false).unwrap();
    c.contact.set_upstream_compat(ContactUpstreamCompat::all());
    c.set_upstream_compat(compat);
    let fix = FixedValue {
        value: 0.0,
        axis: SpatialDirection::X,
    };
    c.apply_dirichlet_bc(&fix, "sides").unwrap();
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

/// One recorded callback event: its code (1 matrix K, 2 residual, 3
/// corrector) and the recorded quantities (matrices as dense 36 × 36,
/// symmetric storage mirrored).
type Event = (i64, BTreeMap<String, Vec<f64>>);

fn dense(irn: &[i64], jcn: &[i64], a: &[f64], n: usize) -> Vec<f64> {
    let mut d = vec![0.0; n * n];
    let mut stored = vec![false; n * n];
    for ((&i, &j), &v) in irn.iter().zip(jcn).zip(a) {
        let (i, j) = (i as usize, j as usize);
        d[i + j * n] += v;
        stored[i + j * n] = true;
    }
    // symmetric storage (one triangle): mirror
    for i in 0..n {
        for j in 0..n {
            if stored[i + j * n] && !stored[j + i * n] {
                d[j + i * n] = d[i + j * n];
            }
        }
    }
    d
}

/// One step of the probe's loading through the coupler's callback with the
/// switches `compat`, recording every callback event when `record`.
fn traced_step(
    c: &mut CouplerSolidCohesiveContact,
    compat: SolidContactCompat,
    record: bool,
) -> (bool, Vec<Event>) {
    let inc: Real = 1e-3 / 50.0;
    let up = IncrementValue {
        value: -inc,
        axis: SpatialDirection::Y,
    };
    let down = IncrementValue {
        value: inc,
        axis: SpatialDirection::Y,
    };
    c.apply_dirichlet_bc(&up, "loading").unwrap();
    c.apply_dirichlet_bc(&down, "fixed").unwrap();
    c.forward_new_cohesive_elements().unwrap();
    let mut events: Vec<Event> = Vec::new();
    let contact = &mut c.contact;
    let r = c.solid.smm.solve_step_with(None, |physics, solver, id| {
        let mut cohesive = CohesiveCallback::new(physics);
        let mut inner =
            SolidContactCallback::coupler(&mut cohesive, contact, AnalysisMethod::Static)
                .with_compat(compat);
        let mut cb = Traced {
            inner: &mut inner,
            on_event: |what: &str,
                       dofs: &akantu_solver::DofManager,
                       cbk: &SolidContactCallback<'_>| {
                if !record {
                    return;
                }
                let mut q = BTreeMap::new();
                let code = match what {
                    "matrix:K" => {
                        let (i, j, a) = triplets(dofs, "K");
                        q.insert("K".to_owned(), dense(&i, &j, &a, 36));
                        1
                    }
                    "residual" => {
                        q.insert("residual".to_owned(), dofs.residual().values().to_vec());
                        2
                    }
                    "corrector" => {
                        let (i, j, a) = triplets(dofs, "J");
                        q.insert("J".to_owned(), dense(&i, &j, &a, 36));
                        q.insert("solution".to_owned(), dofs.solution().to_vec());
                        let ct = &*cbk.contact;
                        q.insert("positions".to_owned(), ct.positions().as_slice().to_vec());
                        q.insert("gaps".to_owned(), ct.gaps().as_slice().to_vec());
                        q.insert("normals".to_owned(), ct.normals().as_slice().to_vec());
                        q.insert(
                            "contact_elements".to_owned(),
                            ct.contact_elements()
                                .iter()
                                .flat_map(|e| [e.slave as f64, e.master.index() as f64])
                                .collect(),
                        );
                        3
                    }
                    _ => 0,
                };
                events.push((code, q));
            },
        };
        solver.solve_step(&mut cb, id)
    });
    (r.is_ok(), events)
}

/// The oracle's events of step `s` in the same representation.
fn oracle_events(f: &Fixture, s: usize) -> Vec<Event> {
    let pre = format!("trace/step_{s}");
    let codes = i64s(f, &format!("{pre}/events"));
    let mut out = Vec::new();
    for (e, &code) in codes.iter().enumerate() {
        let p = format!("{pre}/e{e}");
        let mut q = BTreeMap::new();
        match code {
            1 => {
                let d = dense(
                    &i64s(f, &format!("{p}/K/irn")),
                    &i64s(f, &format!("{p}/K/jcn")),
                    &f64s(f, &format!("{p}/K/a")),
                    36,
                );
                q.insert("K".to_owned(), d);
            }
            2 => {
                q.insert("residual".to_owned(), f64s(f, &format!("{p}/residual")));
            }
            3 => {
                let d = dense(
                    &i64s(f, &format!("{p}/J/irn")),
                    &i64s(f, &format!("{p}/J/jcn")),
                    &f64s(f, &format!("{p}/J/a")),
                    36,
                );
                q.insert("J".to_owned(), d);
                for w in ["solution", "positions", "gaps", "normals"] {
                    q.insert(w.to_owned(), f64s(f, &format!("{p}/{w}")));
                }
                q.insert(
                    "contact_elements".to_owned(),
                    i64s(f, &format!("{p}/contact_elements"))
                        .iter()
                        .map(|&v| v as f64)
                        .collect(),
                );
            }
            _ => {}
        }
        out.push((code, q));
    }
    out
}

/// First differing (event, quantity, entries differing / entries) of two
/// traces, or `None` when they are bitwise identical.
fn first_difference(got: &[Event], expected: &[Event]) -> Option<String> {
    for (e, ((gc, gq), (ec, eq))) in got.iter().zip(expected).enumerate() {
        if gc != ec {
            return Some(format!("event {e}: code {gc} vs {ec}"));
        }
        for (name, ev) in eq {
            let gv = &gq[name];
            if gv.len() != ev.len() || count_bits(gv, ev) != ev.len() {
                let first = (0..ev.len().min(gv.len())).find(|&i| !same_bits(gv[i], ev[i]));
                return Some(format!(
                    "event {e} (code {ec}) {name}: {} of {} entries differ; first at {first:?}: {:?} vs {:?}",
                    ev.len() - count_bits(gv, ev),
                    ev.len(),
                    first.map(|i| gv[i]),
                    first.map(|i| ev[i])
                ));
            }
        }
    }
    if got.len() != expected.len() {
        return Some(format!("{} events vs {}", got.len(), expected.len()));
    }
    None
}

/// Upstream-compatible replay (UD-772 contact stiffness withheld, UD-374
/// per-iterate history, UD-034 symmetric K storage, contact `all()`,
/// cohesive laws upstream): every step until upstream's Newton failure,
/// the callback trace of steps 0 and 1 (every event bitwise: K, J,
/// residual, solution, detector positions, gaps, normals, pairings) and the
/// composition of K. Every quantity of the manifest is compared, except the
/// contact model's own DOF-manager matrix (evidence of UD-772: the port has
/// no such matrix).
#[test]
fn cohesive_static_fixture_upstream_compat() {
    replay_cohesive_static(SolidContactCompat::all());
}

/// UD-1291: the same replay with the corrected search order (every other
/// switch upstream's) is bitwise too: the predictor of these static steps
/// moves only the prescribed nodes (`loading` and `fixed`), none of which
/// is on the crack (intrinsic cohesive elements, inserted at
/// initialisation), so the search before the first residual finds the
/// previous step's last search. The case therefore does not need
/// `search_in_corrector_only`; the implicit quasi-static case does
/// (`cohesive_implicit_dynamic.rs`).
#[test]
fn cohesive_static_fixture_corrected_search_order() {
    replay_cohesive_static(SolidContactCompat {
        search_in_corrector_only: false,
        ..SolidContactCompat::all()
    });
}

fn replay_cohesive_static(compat: SolidContactCompat) {
    let f = fixture();
    let cid = format!("{} ({compat:?})", f.case_id());
    let mut c = setup(&f, compat, true);
    let used: std::cell::RefCell<std::collections::BTreeSet<String>> = Default::default();
    let cmp = |name: &str, got: &[f64]| {
        assert_bits(&cid, name, got, &f64s(&f, name));
        used.borrow_mut().insert(name.to_owned());
    };
    let steps_run = f.manifest().parameters["steps_run"].as_i64().unwrap() as usize;
    assert_eq!(i64s(&f, "cohesive_elements")[0], {
        use akantu_model::Model;
        let m = c.solid.smm.mesh().read().unwrap();
        m.nb_element(
            akantu_core::ElementType::Cohesive2d4,
            akantu_core::GhostType::NotGhost,
        ) as i64
    });
    used.borrow_mut().insert("cohesive_elements".into());
    let digests = read_input_text(&f, "digests");
    let mut lines = digests.lines();
    let (mut conv, mut its, mut errs, mut nbs, mut pens) = (vec![], vec![], vec![], vec![], vec![]);
    for s in 0..steps_run {
        let (ok, got) = traced_step(&mut c, compat, s <= 1);
        if s <= 1 {
            let expected = oracle_events(&f, s);
            assert_eq!(
                first_difference(&got, &expected),
                None,
                "{cid}: trace of step {s}"
            );
            for name in f.quantity_names(&format!("trace/step_{s}/*")) {
                used.borrow_mut().insert(name.to_owned());
            }
        }
        let (it, err) = newton(&c);
        conv.push(i64::from(ok));
        its.push(it);
        errs.push(err);
        nbs.push(c.contact.contact_elements().len() as i64);
        pens.push(
            c.contact
                .gaps()
                .as_slice()
                .iter()
                .fold(0.0f64, |m, g| m.max(*g)),
        );
        let u = c.solid.smm.displacement().unwrap().as_slice().to_vec();
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
        cmp(&format!("{pre}/displacement"), &u);
        cmp(
            &format!("{pre}/internal_force"),
            c.solid.smm.internal_force().unwrap().as_slice(),
        );
        cmp(
            &format!("{pre}/contact_force"),
            c.contact.internal_force().as_slice(),
        );
        cmp(&format!("{pre}/gaps"), c.contact.gaps().as_slice());
        cmp(&format!("{pre}/normals"), c.contact.normals().as_slice());
        cmp(&format!("{pre}/areas"), c.contact.nodal_area().as_slice());
        cmp(
            &format!("{pre}/positions"),
            c.contact.positions().as_slice(),
        );
        let pairs: Vec<i64> = c
            .contact
            .contact_elements()
            .iter()
            .flat_map(|e| [i64::from(e.slave), e.master.index() as i64])
            .collect();
        assert_eq!(
            pairs,
            i64s(&f, &format!("{pre}/contact_elements")),
            "{cid}: {pre} pairs"
        );
        used.borrow_mut().insert(format!("{pre}/contact_elements"));
    }
    assert_eq!(conv, i64s(&f, "newton/converged"), "{cid}: converged");
    assert_eq!(its, i64s(&f, "newton/iterations"), "{cid}: iterations");
    assert_eq!(
        nbs,
        i64s(&f, "nb_contact_elements"),
        "{cid}: contact elements"
    );
    for n in [
        "newton/converged",
        "newton/iterations",
        "nb_contact_elements",
    ] {
        used.borrow_mut().insert(n.into());
    }
    cmp("newton/error", &errs);
    cmp("max_penetration", &pens);
    // K composition: the coupler's assembleStiffnessMatrix() under UD-772
    // compat is the solid stiffness alone
    let ks = coupler_k(&mut c, compat);
    cmp("K/coupler_after_coupler_assembly", &k_sums(&ks));
    let (irn, jcn, a) = &ks;
    assert_eq!(irn, &i64s(&f, "K/coupler/irn"), "{cid}: K rows");
    assert_eq!(jcn, &i64s(&f, "K/coupler/jcn"), "{cid}: K columns");
    cmp("K/coupler/a", a);
    used.borrow_mut().insert("K/coupler/irn".into());
    used.borrow_mut().insert("K/coupler/jcn".into());
    let ks_solid = solid_k(&mut c);
    cmp("K/coupler_after_solid_only_assembly", &k_sums(&ks_solid));
    // evidence only (UD-772): the contact model's own, never solved matrix
    used.borrow_mut()
        .insert("K/contact_dof_manager_after_coupler_assembly".into());
    let all: Vec<String> = f
        .quantity_names("")
        .into_iter()
        .map(str::to_owned)
        .collect();
    let unused: Vec<&String> = all.iter().filter(|n| !used.borrow().contains(*n)).collect();
    assert!(
        unused.is_empty(),
        "{cid}: quantities never compared: {unused:?}"
    );
    eprintln!(
        "{cid}: {steps_run} steps (Newton {its:?}, converged {conv:?}), {} quantities compared bitwise",
        used.borrow().len()
    );
}

/// The coupler's `assembleStiffnessMatrix()` (outside a solve: the solid
/// stiffness reassembled, plus the contact stiffness unless withheld), as
/// triplets of the coupler's `K`.
fn coupler_k(
    c: &mut CouplerSolidCohesiveContact,
    compat: SolidContactCompat,
) -> (Vec<i64>, Vec<i64>, Vec<f64>) {
    use akantu_solver::SolverCallback;
    let akantu_solid::SolidMechanicsModel {
        physics, solver, ..
    } = &mut c.solid.smm;
    let mut cohesive = CohesiveCallback::new(physics);
    let mut cb =
        SolidContactCallback::coupler(&mut cohesive, &mut c.contact, AnalysisMethod::Static)
            .with_compat(compat);
    solver
        .dof_manager
        .collective("test:K", |dm| {
            cb.assemble_matrix(dm, "K")?;
            dm.finalize_matrix("K")
        })
        .unwrap();
    triplets(&solver.dof_manager, "K")
}

/// `solid.assembleStiffnessMatrix(true)` into the coupler's `K`.
fn solid_k(c: &mut CouplerSolidCohesiveContact) -> (Vec<i64>, Vec<i64>, Vec<f64>) {
    use akantu_couplers::contact::SolidPart;
    let akantu_solid::SolidMechanicsModel {
        physics, solver, ..
    } = &mut c.solid.smm;
    let mut cohesive = CohesiveCallback::new(physics);
    solver
        .dof_manager
        .collective("test:Ks", |dm| {
            cohesive.assemble_solid_stiffness(dm, true)?;
            dm.finalize_matrix("K")
        })
        .unwrap();
    triplets(&solver.dof_manager, "K")
}

/// Upstream's `(nnz, Σ a, Σ |a|)` of a matrix (sequential sums in storage
/// order, as the probe).
fn k_sums(k: &(Vec<i64>, Vec<i64>, Vec<f64>)) -> Vec<f64> {
    let mut s = 0.0;
    let mut sa = 0.0;
    for v in &k.2 {
        s += v;
        sa += v.abs();
    }
    vec![k.2.len() as f64, s, sa]
}

/// Corrected defaults (UD-772: contact stiffness in the solved `K`; UD-374
/// history committed per step; UD-034 promotion of `K`) on the same setup:
/// the Newton tangent is now the derivative of the coupled residual, so
/// every step converges in a few iterations (upstream: 13, 2, 2, 2, then a
/// failure after 100 iterations at step 4), the run passes upstream's
/// failure step, and the coupler's `K` is the solid stiffness plus the
/// contact stiffness.
#[test]
fn cohesive_static_corrected_default_converges() {
    let f = fixture();
    let compat = SolidContactCompat::default();
    let mut c = setup(&f, compat, false);
    let mut its = Vec::new();
    for s in 0..10 {
        let (ok, _) = traced_step(&mut c, compat, false);
        let (it, err) = newton(&c);
        assert!(
            ok,
            "step {s}: Newton failed ({it} iterations, error {err:e})"
        );
        its.push(it);
        assert!(
            !c.contact.contact_elements().is_empty(),
            "step {s}: the crack faces must stay in contact"
        );
    }
    // first increment: 12 iterations (the contact set builds up from the
    // stress-free state); then 2 per step (upstream compat: 13, 2, 2, 2,
    // failure after 100 at step 4)
    assert!(
        its[0] <= 13 && its[1..].iter().all(|&i| i <= 3),
        "corrected default: Newton iterations {its:?}"
    );
    let kc = coupler_k(&mut c, compat);
    let ksol = solid_k(&mut c);
    let (abs_c, abs_s) = (k_sums(&kc)[2], k_sums(&ksol)[2]);
    assert!(
        abs_c > abs_s,
        "the coupled K carries the contact stiffness: Σ|K| {abs_c:e} vs solid {abs_s:e}"
    );
    eprintln!(
        "corrected default: Newton iterations {its:?}; Σ|K| coupled {abs_c:e}, solid {abs_s:e}"
    );
}

/// The coupled static replay departs from upstream at Newton iteration 2
/// when the port's default UD-034 storage (K promoted to unsymmetric when
/// the unsymmetric cohesive tangent is assembled) is kept: the first
/// differing quantity is `K` (the root cause; the contact
/// search is bitwise on identical positions).
#[test]
fn cohesive_static_departs_at_iteration_2_without_the_ud034_compat() {
    let f = fixture();
    let compat = SolidContactCompat::all();
    let mut c = setup(&f, compat, false);
    let (_, got) = traced_step(&mut c, compat, true);
    let expected = oracle_events(&f, 0);
    let d = first_difference(&got, &expected).expect("expected a difference");
    assert!(
        d.starts_with("event 4 (code 1) K:"),
        "first difference: {d}"
    );
    assert_eq!(first_difference(&got[..4], &expected[..4]), None);
}

/// UD-622 (latent upstream): the stand-alone callback's
/// `need_to_reassemble = false` (compat flag `upstream_stiffness`) has no
/// effect with a `SolidMechanicsModelCohesive`: its cohesive laws report a
/// changed stiffness (upstream `Material::hasStiffnessMatrixChanged()` is
/// `true` for them), so `K` is zeroed and reassembled either way.
#[test]
fn ud622_has_no_reachable_effect_with_cohesive_laws() {
    use akantu_couplers::CohesiveContactSolverCallback;
    use akantu_solver::SolverCallback;
    let f = fixture();
    let compat = SolidContactCompat::default();
    let mut c = setup(&f, compat, false);
    for _ in 0..3 {
        let (ok, _) = traced_step(&mut c, compat, false);
        assert!(ok);
    }
    let mut ks = Vec::new();
    for upstream_stiffness in [false, true, true] {
        let akantu_solid::SolidMechanicsModel {
            physics, solver, ..
        } = &mut c.solid.smm;
        let mut cohesive = CohesiveCallback::new(physics);
        let mut cb = CohesiveContactSolverCallback::with_compat(
            &mut cohesive,
            &mut c.contact,
            AnalysisMethod::Static,
            compat,
            upstream_stiffness,
        );
        solver
            .dof_manager
            .collective("test:K", |dm| {
                cb.as_callback().assemble_matrix(dm, "K")?;
                dm.finalize_matrix("K")
            })
            .unwrap();
        ks.push(triplets(&solver.dof_manager, "K"));
    }
    assert_eq!(ks[0], ks[1], "need_to_reassemble = false changed K");
    assert_eq!(ks[1], ks[2], "K accumulated over two assemblies");
}
