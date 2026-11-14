// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for the cohesive model ported from src/model/solid_mechanics/
// solid_mechanics_model_cohesive/ (materials/constitutive_laws/
// material_cohesive_{linear,bilinear,linear_uncoupled,exponential}.cc,
// solid_mechanics_model_cohesive.cc): model-level Newton consistency of the
// tangents (finite differences of the assembled internal force against the
// assembled stiffness), the
// staged law selection of new cohesive elements, the law-index map after a
// refused insertion, the bilinear floor after a failed step and the
// committed-maxima lifecycle under the solver's legacy hooks and the damage
// iteration hook; the damage-iteration
// event sent through the model reaches the cohesive override in upstream's
// newest-first order (solid_mechanics_model_event_handler.hh,
// aka_event_handler_manager.hh).

mod common;

use akantu_validation::reduce::NanMinMax;
use std::collections::HashSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{Element, ElementKind, ElementType, GhostType, Real};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawPhase, SelectorChain, SelectorContext,
};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_solid::Material as _;

const NG: GhostType = GhostType::NotGhost;

/// Relative column error of `K` against central differences of the
/// assembled internal force above which a tangent is not the derivative of
/// its traction. Consistent tangents give ≤ 8.2e-9 (h = 1e-11
/// on openings ~1e-6, penalty 1e12: the round-off of `f_int`/h); the
/// defects give 0.14–166 (`docs/NUMERICAL_VALIDATION.md`).
const FD_CONSISTENT: Real = 1e-6;

fn dat(law: &str, params: &str) -> String {
    format!(
        "model solid_mechanics_model_cohesive [
  cohesive_inserter [
    cohesive_surfaces = [insertion]
  ]
  material elastic [
    name = body
    rho = 1e3
    E   = 1e9
    nu  = 0.2
  ]
  material {law} [
    name = insertion
{params}
  ]
]
"
    )
}

const BILINEAR: &str = "    beta = 0.8\n    kappa = 1.5\n    G_c = 10\n    sigma_c = 1e6\n    delta_0 = 1e-7\n    penalty = 1e12\n";
const MIXED: &str =
    "    beta = 0.8\n    kappa = 1.5\n    G_c = 10\n    sigma_c = 1e6\n    penalty = 1e12\n";
const EXPONENTIAL: &str = "    beta = 0.8\n    delta_c = 2e-5\n    sigma_c = 1e6\n";

struct M {
    model: SolidMechanicsModelCohesive,
    dim: usize,
    ct: ElementType,
    /// Sign of the normal's y component (the imposed normal opening is
    /// multiplied by it so that a positive value opens).
    ny: Real,
}

fn static_options() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::Static).with_solver_options(
        akantu_solver::ModelSolverOptions {
            non_linear_solver_type: Some(NonLinearSolverType::Auto),
            sparse_solver_type: Some(SparseSolverType::Eigen),
            ..akantu_solver::ModelSolverOptions::default()
        },
    )
}

/// Static model on `mesh_file` with the law of `dat`; an extrinsic model is
/// opened by a uniform mode-I stress of 1.1 σc through the insertion check.
fn build(
    (mesh_file, dim, ct): (&str, usize, ElementType),
    dat: &str,
    extrinsic: bool,
    configure: impl FnOnce(&mut SolidMechanicsModelCohesive),
) -> M {
    let mesh = common::read_mesh(&common::cohesive_mesh(mesh_file), dim);
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh,
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    configure(&mut model);
    model.init_full(&static_options(), extrinsic).unwrap();
    let mut m = M {
        model,
        dim,
        ct,
        ny: 1.0,
    };
    if extrinsic {
        // σ_yy = 1.1 σc under uniaxial strain
        let (e, nu) = (1e9, 0.2);
        let eyy = 1.1 * 1e6 / e * (1. + nu) * (1. - 2. * nu) / (1. - nu);
        let nodes = m
            .model
            .smm
            .physics
            .base
            .mesh
            .read()
            .unwrap()
            .nodes()
            .as_slice()
            .to_vec();
        let mut u_g = m.model.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for (x, ui) in nodes.chunks(dim).zip(u.chunks_mut(dim)) {
            ui.fill(0.0);
            ui[1] = eyy * x[1];
        }
        drop(u_g);
        m.model.assemble_internal_forces().unwrap();
        let n = m.model.check_cohesive_stress().unwrap();
        assert!(n > 0, "the check opens the interface");
        m.model.smm.displacement_mut().unwrap().fill(0.0);
    }
    let _ = fint(&mut m);
    if let Some(y) = field(&m, "normal").get(1) {
        m.ny = if *y < 0.0 { -1.0 } else { 1.0 };
    }
    m
}

fn mesh2() -> (&'static str, usize, ElementType) {
    ("_cohesive_2d_4_triangle_3.msh", 2, ElementType::Cohesive2d4)
}

fn mesh3() -> (&'static str, usize, ElementType) {
    (
        "_cohesive_3d_6_tetrahedron_4.msh",
        3,
        ElementType::Cohesive3d6,
    )
}

/// Opening `(tangential x, normal y[, tangential z])`.
fn v(dim: usize, t: Real, n: Real, t2: Real) -> Vec<Real> {
    if dim == 2 { vec![t, n] } else { vec![t, n, t2] }
}

/// Second-side nodes of every cohesive element moved by `-open`.
fn impose(m: &mut M, open: &[Real]) {
    m.model.smm.displacement_mut().unwrap().fill(0.0);
    let conn = m
        .model
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(m.ct, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let nn = m.ct.nb_nodes_per_element();
    let (d, ny) = (m.dim, m.ny);
    let mut u_g = m.model.smm.displacement_mut().unwrap();
    let u = u_g.as_mut_slice();
    for e in conn.chunks(nn) {
        for &n in &e[nn / 2..] {
            for i in 0..d {
                let s = if i == 1 { ny } else { 1.0 };
                u[n as usize * d + i] = -open[i] * s;
            }
        }
    }
}

fn hook(m: &mut M, phase: LawPhase, converged: Option<bool>) {
    let i = m.model.smm.material_index("insertion").unwrap();
    m.model
        .smm
        .with_material(i, phase, |law, ctx| match converged {
            None => law.before_solve_step(ctx),
            Some(c) => law.after_solve_step(ctx, c),
        })
        .unwrap();
}

/// A converged step at `open` through the material hooks.
fn commit(m: &mut M, open: &[Real]) {
    hook(m, LawPhase::BeforeSolveStep, None);
    impose(m, open);
    m.model.assemble_internal_forces().unwrap();
    hook(m, LawPhase::AfterSolveStep, Some(true));
}

fn fint(m: &mut M) -> Vec<Real> {
    m.model.assemble_internal_forces().unwrap();
    m.model.smm.internal_force().unwrap().as_slice().to_vec()
}

fn field(m: &M, name: &str) -> Vec<Real> {
    m.model
        .cohesive_material("insertion")
        .unwrap()
        .array(name, m.ct, NG)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default()
}

fn previous(m: &M, name: &str) -> Vec<Real> {
    m.model
        .cohesive_material("insertion")
        .unwrap()
        .internal(name)
        .unwrap()
        .previous(m.ct, NG)
        .map(|a| a.as_slice().to_vec())
        .unwrap_or_default()
}

/// DOFs of the second-side nodes of cohesive element 0.
fn probe_dofs(m: &M) -> Vec<usize> {
    let conn = m
        .model
        .smm
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .connectivity(m.ct, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let nn = m.ct.nb_nodes_per_element();
    let mut v = Vec::new();
    for &n in &conn[nn / 2..nn] {
        for i in 0..m.dim {
            v.push(n as usize * m.dim + i);
        }
    }
    v
}

/// Worst relative column error of `K` against `−∂f_int/∂u` (central
/// differences, step `h`) over the DOFs of [`probe_dofs`], each evaluation
/// preceded by the trial `before` (if any).
fn fd_error(m: &mut M, open: &[Real], before: Option<&[Real]>, h: Real) -> Real {
    let pre = |m: &mut M| {
        if let Some(b) = before {
            impose(m, b);
            m.model.assemble_internal_forces().unwrap();
        }
    };
    pre(m);
    impose(m, open);
    let _ = fint(m);
    m.model.smm.assemble_stiffness_matrix(true).unwrap();
    let ndof = m.model.smm.displacement().unwrap().as_slice().len();
    let dofs = probe_dofs(m);
    let kcols: Vec<Vec<Real>> = {
        let k = m.model.smm.dof_manager().matrix("K").unwrap();
        dofs.iter()
            .map(|&j| (0..ndof).map(|i| k.get(i as _, j as _)).collect())
            .collect()
    };
    let mut worst = 0.0_f64;
    for (c, &j) in dofs.iter().enumerate() {
        pre(m);
        impose(m, open);
        m.model.smm.displacement_mut().unwrap().as_mut_slice()[j] += h;
        let fp = fint(m);
        pre(m);
        impose(m, open);
        m.model.smm.displacement_mut().unwrap().as_mut_slice()[j] -= h;
        let fm = fint(m);
        let scale = kcols[c]
            .iter()
            .fold(0.0_f64, |a, v| a.nan_max(v.abs()))
            .max(1.0);
        for i in 0..ndof {
            let fd = (fp[i] - fm[i]) / (2. * h);
            worst = worst.nan_max((kcols[c][i] + fd).abs() / scale);
        }
    }
    worst
}

/// `(label, committed opening, trial before, trial opening)`.
type Scenario = (
    &'static str,
    Option<Vec<Real>>,
    Option<Vec<Real>>,
    Vec<Real>,
);

/// The scenarios `v57_fd_newton_consistency`.
fn scenarios(dim: usize) -> Vec<Scenario> {
    vec![
        ("loading mixed", None, None, v(dim, 3e-6, 6e-6, 1e-6)),
        (
            "unloading mixed",
            Some(v(dim, 4e-6, 8e-6, 1e-6)),
            None,
            v(dim, 2e-6, 4e-6, 0.5e-6),
        ),
        (
            "contact, sliding below max",
            Some(v(dim, 0.0, 8e-6, 0.0)),
            None,
            v(dim, 3e-6, -2e-6, 1e-6),
        ),
        (
            "contact, sliding loading",
            Some(v(dim, 0.0, 2e-6, 0.0)),
            None,
            v(dim, 1.2e-5, -2e-6, 2e-6),
        ),
        (
            "contact after overshooting trial",
            Some(v(dim, 0.0, 8e-6, 0.0)),
            Some(v(dim, 0.0, 6e-5, 0.0)),
            v(dim, 3e-6, -2e-6, 1e-6),
        ),
        (
            "contact, sliding past delta_c",
            Some(v(dim, 0.0, 2e-6, 0.0)),
            None,
            v(dim, 4e-5, -2e-6, 0.5e-6),
        ),
        (
            "unloading after overshooting trial",
            Some(v(dim, 4e-6, 8e-6, 1e-6)),
            Some(v(dim, 0.0, 6e-5, 0.0)),
            v(dim, 2e-6, 4e-6, 0.5e-6),
        ),
    ]
}

/// **UD-248**: the assembled stiffness of
/// `cohesive_exponential` is the derivative of its assembled internal force
/// on every scenario (loading, unloading, contact below and
/// past `δ_max`, after overshooting trials), 2-D and 3-D, `β = 0.8`. The
/// upstream tangent (compat) is consistent on loading only (0.14–0.41
/// elsewhere).
#[test]
fn co4_01_exponential_tangent_is_newton_consistent() {
    let _g = common::lock();
    let mut failures = Vec::new();
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        for (label, committed, before, trial) in scenarios(dim) {
            for upstream in [false, true] {
                let mut m = build(
                    mesh,
                    &dat("cohesive_exponential", EXPONENTIAL),
                    false,
                    |md| {
                        md.ext.compat.exponential_tangent = upstream;
                    },
                );
                if let Some(o) = &committed {
                    commit(&mut m, o);
                }
                let err = fd_error(&mut m, &trial, before.as_deref(), 1e-11);
                eprintln!("{dim}-D {label:36} upstream tangent {upstream}: error {err:.2e}");
                let consistent_expected = !upstream || label == "loading mixed";
                if (err <= FD_CONSISTENT) != consistent_expected {
                    failures.push(format!("{dim}-D {label} (upstream {upstream}): {err:e}"));
                }
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

/// The tangent decides the contact
/// branch from the committed damage, as the traction does. Committed
/// intact, then a compression trial with sliding past `δc` (trial damage 1):
/// the linear, bilinear and uncoupled tangents are the derivative of the
/// traction (with the trial damage the tangent drops the penalty: error
/// 166).
#[test]
fn co4_02_tangent_contact_decision_uses_the_committed_damage() {
    let _g = common::lock();
    let mut failures = Vec::new();
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        for (law, params, extrinsic) in [
            ("cohesive_linear", MIXED, true),
            ("cohesive_bilinear", BILINEAR, false),
            ("cohesive_linear_uncoupled", MIXED, true),
        ] {
            let mut m = build(mesh, &dat(law, params), extrinsic, |_| {});
            commit(&mut m, &v(dim, 0.0, 2e-6, 0.0));
            let err = fd_error(&mut m, &v(dim, 4e-5, -2e-6, 0.5e-6), None, 1e-11);
            eprintln!("{dim}-D {law}: contact, sliding past delta_c: error {err:.2e}");
            if err > FD_CONSISTENT {
                failures.push(format!("{dim}-D {law}: {err:e}"));
            }
        }
    }
    assert!(failures.is_empty(), "{failures:?}");
}

// ---------------------------------------------------- staged law selection

/// A selector that records its calls for cohesive elements: law `first` on
/// the first call for an element, `again` on any later call for the same
/// element (a re-selection by the dispatch would therefore change the law),
/// an error at the `fail_at`-th call; regular elements: law 0, facets: 1.
#[derive(Debug)]
struct Recording {
    calls: Arc<AtomicUsize>,
    seen: Arc<Mutex<HashSet<Element>>>,
    first: usize,
    again: usize,
    fail_at: Option<usize>,
}

impl Recording {
    fn new(first: usize, again: usize, fail_at: Option<usize>) -> Self {
        Self {
            calls: Arc::default(),
            seen: Arc::default(),
            first,
            again,
            fail_at,
        }
    }
}

impl ConstitutiveLawSelector for Recording {
    fn select(&self, ctx: &SelectorContext<'_>, el: Element) -> akantu_core::Result<Option<usize>> {
        if el.kind() == ElementKind::Cohesive {
            let k = self.calls.fetch_add(1, Ordering::SeqCst);
            if Some(k) == self.fail_at {
                return Err(akantu_core::Error::invalid("recording selector failure"));
            }
            let fresh = self.seen.lock().unwrap().insert(el);
            return Ok(Some(if fresh { self.first } else { self.again }));
        }
        Ok(Some(
            if el.ty().spatial_dimension() < ctx.mesh.spatial_dimension() {
                1
            } else {
                0
            },
        ))
    }
}

/// `body` (elastic), `insertion` and `second` (`cohesive_bilinear`), `zero`
/// (`cohesive_linear`: no strength when inserted intrinsically, UD-224).
fn four_laws() -> String {
    let base = common::MATERIAL_1.trim_end();
    format!(
        "{}\n  material cohesive_bilinear [\n    name = second\n    beta = 1\n    G_c = 10\n    \
         sigma_c = 1e6\n    delta_0 = 1e-7\n  ]\n  material cohesive_linear [\n    name = zero\n    \
         sigma_c = 1e6\n    G_c = 10\n    beta = 1\n  ]\n]\n",
        &base[..base.len() - 1]
    )
}

fn intrinsic_with(selector: Recording) -> (akantu_mesh::SharedMesh, SolidMechanicsModelCohesive) {
    let (mesh_file, dim, _) = mesh2();
    let mesh = common::read_mesh(&common::cohesive_mesh(mesh_file), dim);
    let parser = Parser::parse_str(&four_laws(), "material.dat").unwrap();
    let mut model = SolidMechanicsModelCohesive::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model_cohesive",
        parser,
    )
    .unwrap();
    model.set_material_selector_chain(SelectorChain {
        selectors: vec![Box::new(selector)],
        fallback_value: 0,
    });
    (mesh, model)
}

fn explicit() -> ModelOptions {
    ModelOptions::new(AnalysisMethod::ExplicitLumpedMass)
}

/// The law of every new cohesive element is
/// selected once, inside the mesh transaction, and the dispatch answers from
/// that staged choice. A selector answering differently on a repeated call
/// is called once per new element and every element goes to its first
/// answer (a dispatch that re-selected would call it twice per element and
/// assign the second answer).
#[test]
fn co4_02_dispatch_answers_from_the_staged_selection() {
    let _g = common::lock();
    let ct = ElementType::Cohesive2d4;
    let sel = Recording::new(1, 2, None);
    let calls = Arc::clone(&sel.calls);
    let (mesh, mut model) = intrinsic_with(sel);
    model.init_full(&explicit(), false).unwrap();
    let n = mesh.read().unwrap().nb_element(ct, NG);
    assert!(n > 0);
    assert_eq!(
        calls.load(Ordering::SeqCst),
        n,
        "one selector call per new cohesive element"
    );
    let map = model.smm.physics.materials.law_index_map();
    for k in 0..n {
        let e = Element::new(ct, akantu_core::try_idx(k).unwrap(), NG);
        assert_eq!(
            map.element_ref(e).unwrap()[0],
            1,
            "cohesive element {k}: the staged (first) answer"
        );
    }
    let second = model.cohesive_material("second").unwrap();
    let in_second = second
        .base()
        .element_filter
        .get(ct, NG)
        .map_or(0, |a| a.len());
    assert_eq!(in_second, 0, "nothing dispatched to the second answer");
}

/// `(type/ghost, length)` of every block of the law-index map.
fn map_lengths(model: &SolidMechanicsModelCohesive) -> Vec<(String, usize)> {
    model
        .smm
        .physics
        .materials
        .law_index_map()
        .as_map()
        .iter_all()
        .map(|(t, g, a)| (format!("{t:?}/{g:?}"), a.len()))
        .collect()
}

/// A refused or failed insertion leaves
/// the law-index map as it was: every block as long as the mesh's element
/// count, after a UD-224 refusal (zero-strength law), a UD-246 refusal
/// (cohesive elements to the bulk law) and a selector error part-way.
#[test]
fn co4_02_law_index_map_follows_the_mesh_after_a_refused_insertion() {
    let _g = common::lock();
    for (label, sel) in [
        ("UD-224 zero strength", Recording::new(3, 3, None)),
        ("UD-246 bulk law", Recording::new(0, 0, None)),
        ("selector error", Recording::new(1, 1, Some(2))),
    ] {
        let (mesh, mut model) = intrinsic_with(sel);
        let before = map_lengths(&model);
        let rev = mesh.revision().unwrap();
        let e = model.init_full(&explicit(), false);
        assert!(e.is_err(), "{label}: refused");
        assert_eq!(mesh.revision().unwrap(), rev, "{label}: mesh unchanged");
        let after = map_lengths(&model);
        assert_eq!(after, before, "{label}: law-index map");
        let m = mesh.read().unwrap();
        for (ty, g, a) in model
            .smm
            .physics
            .materials
            .law_index_map()
            .as_map()
            .iter_all()
        {
            assert_eq!(a.len(), m.nb_element(ty, g), "{label}: {ty:?}/{g:?}");
        }
    }
}

/// After a failed step restored the history
/// the maximal opening of `cohesive_bilinear` is at least `δ₀` (elements
/// inserted after the last commit have the history default 0), as the
/// traction reads it.
#[test]
fn co4_02_bilinear_maximal_opening_is_floored_after_a_failed_step() {
    let _g = common::lock();
    let mut m = build(mesh2(), &dat("cohesive_bilinear", BILINEAR), false, |_| {});
    hook(&mut m, LawPhase::BeforeSolveStep, None);
    impose(&mut m, &v(2, 0.0, 5e-8, 0.0));
    let _ = fint(&mut m);
    hook(&mut m, LawPhase::AfterSolveStep, Some(false));
    let dm = field(&m, "delta max");
    assert!(!dm.is_empty());
    assert!(
        dm.iter().all(|&x| x == 1e-7),
        "delta max after the failed step: {dm:?}"
    );
}

// ------------------------------------------- committed-maxima lifecycle

fn set_legacy_hooks(m: &mut M, legacy: bool) {
    let nls = m.model.smm.solver.non_linear_solver_mut(None).unwrap();
    nls.set_param("convergence_type", ParamValue::Enum("residual".into()))
        .unwrap();
    nls.as_newton_raphson_mut()
        .expect("static Newton-Raphson solver")
        .set_legacy_hooks(legacy);
}

/// A solve step at the imposed opening with every DOF blocked: the residual
/// of the free equations is empty, so Newton converges at its first test
/// (UD-010: under the legacy hooks, without `after_solve_step`).
fn blocked_step(m: &mut M, open: &[Real]) {
    impose(m, open);
    m.model
        .smm
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .fill(true);
    m.model.solve_step(None).unwrap();
    let nls = m.model.smm.solver.non_linear_solver(None).unwrap();
    assert_eq!(
        nls.nb_iterations(),
        0,
        "converged at the first residual test"
    );
}

/// The maxima a converged step reached are committed
/// in both hook modes of the Newton solver. Under the legacy hooks (UD-010
/// compat) a step that converges at its first residual test returns
/// without `after_solve_step`; the cohesive model commits the maxima on the
/// return path of `solve_step`, so the next step unloads from them. With
/// `maxima_committed_at_step_entry` (UD-245 compat) the maxima are saved at
/// the next step entry, as upstream, in both modes.
#[test]
fn co4_03_maxima_are_committed_in_both_hook_modes() {
    let _g = common::lock();
    for (legacy, at_entry) in [(false, false), (true, false), (false, true), (true, true)] {
        let what = format!("legacy hooks {legacy}, maxima at step entry {at_entry}");
        for (law, params, extrinsic, name) in [
            ("cohesive_bilinear", BILINEAR, false, "delta max"),
            ("cohesive_exponential", EXPONENTIAL, false, "delta max"),
            ("cohesive_linear_uncoupled", MIXED, true, "delta_n_max"),
        ] {
            let mut m = build(mesh2(), &dat(law, params), extrinsic, |md| {
                md.ext.compat.maxima_committed_at_step_entry = at_entry;
            });
            set_legacy_hooks(&mut m, legacy);
            blocked_step(&mut m, &v(2, 1e-6, 8e-6, 0.0));
            let reached = field(&m, name);
            assert!(
                reached.iter().any(|&x| x > 1e-6),
                "{what} {law}: the step opens the interface: {reached:?}"
            );
            if at_entry {
                // upstream: committed by the next step entry only
                assert_ne!(
                    previous(&m, name),
                    reached,
                    "{what} {law}: not yet committed"
                );
            } else {
                assert_eq!(previous(&m, name), reached, "{what} {law}: committed");
            }
            // the next step unloads from the maxima of the first
            blocked_step(&mut m, &v(2, 0.5e-6, 4e-6, 0.0));
            assert_eq!(
                field(&m, name),
                reached,
                "{what} {law}: unloading keeps the maxima"
            );
            assert_eq!(
                previous(&m, name),
                reached,
                "{what} {law}: committed maxima"
            );
        }
    }
}

/// The damage-iteration hook (upstream
/// `onDamageIteration` = `savePreviousState`) saves the histories but never
/// commits trial maxima: after a converged step and a standalone overshoot
/// the committed maxima are those of the converged step, the other
/// histories (opening) are saved. With `maxima_committed_at_step_entry`
/// (UD-245 compat) it saves everything, as upstream.
#[test]
fn co4_03_damage_iteration_does_not_commit_trial_maxima() {
    let _g = common::lock();
    for at_entry in [false, true] {
        for (law, params, extrinsic, name) in [
            ("cohesive_bilinear", BILINEAR, false, "delta max"),
            ("cohesive_exponential", EXPONENTIAL, false, "delta max"),
            ("cohesive_linear_uncoupled", MIXED, true, "delta_n_max"),
        ] {
            let mut m = build(mesh2(), &dat(law, params), extrinsic, |md| {
                md.ext.compat.maxima_committed_at_step_entry = at_entry;
            });
            commit(&mut m, &v(2, 1e-6, 8e-6, 0.0));
            let committed = field(&m, name);
            impose(&mut m, &v(2, 0.0, 3e-5, 0.0));
            let _ = fint(&mut m);
            let trial = field(&m, name);
            assert_ne!(
                trial, committed,
                "{law}: the overshoot raises the trial maxima"
            );
            m.model
                .cohesive_material_mut("insertion")
                .unwrap()
                .on_damage_iteration()
                .unwrap();
            assert_eq!(
                previous(&m, "opening"),
                field(&m, "opening"),
                "{law} (at entry {at_entry}): the opening history is saved"
            );
            let expected = if at_entry { &trial } else { &committed };
            assert_eq!(
                &previous(&m, name),
                expected,
                "{law} (at entry {at_entry}): committed maxima after the damage iteration"
            );
        }
    }
}

/// User event handler failing at `onBeginningOfDamageIteration` (O-3 test):
/// the first error stops the dispatch (upstream: the exception propagates
/// out of `EventHandlerManager::sendEvent`), which shows whether the laws
/// were dispatched before or after the handler.
#[derive(Debug)]
struct FailingAtDamageIteration;

impl akantu_solid::model::SolidMechanicsModelEventHandler for FailingAtDamageIteration {
    fn on_beginning_of_damage_iteration(&mut self) -> akantu_core::Result<()> {
        Err(akantu_core::Error::invalid("handler stops the dispatch"))
    }
}

/// `BeginningOfDamageIterationEvent`
/// sent through the model (`SolidMechanicsModel::send_event`, SMM-018)
/// reaches the cohesive law's `onDamageIteration` override (the
/// histories are saved, the trial maxima are not committed), in upstream's
/// order: the laws (each registered as a handler when constructed,
/// `material.cc:53`) and the user handlers form one list dispatched newest
/// first. A handler registered after `init_full` (newer than the laws) runs
/// before them, so its error leaves the cohesive histories unsaved; one
/// registered before `init_full` (older) runs after them, so the histories
/// are saved before its error is returned.
#[test]
fn o3_damage_iteration_event_reaches_the_cohesive_law_newest_first() {
    use akantu_solid::model::SolidMechanicsModelEvent::BeginningOfDamageIteration;
    let _g = common::lock();
    for (law, params, extrinsic, name) in [
        ("cohesive_bilinear", BILINEAR, false, "delta max"),
        ("cohesive_linear_uncoupled", MIXED, true, "delta_n_max"),
    ] {
        // (handler registered before init_full?, expected histories saved)
        for (older_handler, saved) in [(None, true), (Some(false), false), (Some(true), true)] {
            let mut m = build(mesh2(), &dat(law, params), extrinsic, |md| {
                if older_handler == Some(true) {
                    md.smm
                        .register_event_handler(Box::new(FailingAtDamageIteration));
                }
            });
            if older_handler == Some(false) {
                m.model
                    .smm
                    .register_event_handler(Box::new(FailingAtDamageIteration));
            }
            commit(&mut m, &v(2, 1e-6, 8e-6, 0.0));
            let committed = field(&m, name);
            impose(&mut m, &v(2, 0.0, 3e-5, 0.0));
            let _ = fint(&mut m);
            let opening = field(&m, "opening");
            assert_ne!(previous(&m, "opening"), opening, "{law}: trial opening");
            assert_ne!(field(&m, name), committed, "{law}: trial maxima");
            let r = m.model.smm.send_event(BeginningOfDamageIteration);
            assert_eq!(r.is_err(), older_handler.is_some(), "{law}: {r:?}");
            let what = format!("{law} (handler before init_full: {older_handler:?})");
            if saved {
                assert_eq!(previous(&m, "opening"), opening, "{what}: opening saved");
            } else {
                assert_ne!(previous(&m, "opening"), opening, "{what}: dispatch stopped");
            }
            // the cohesive override never commits the trial maxima
            assert_eq!(previous(&m, name), committed, "{what}: committed maxima");
        }
    }
}
