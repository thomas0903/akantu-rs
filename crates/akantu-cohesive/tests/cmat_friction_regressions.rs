// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests of
// the friction fixes for `cohesive_linear_friction`, ported from
// src/model/solid_mechanics/solid_mechanics_model_cohesive/materials/
// constitutive_laws/material_cohesive_linear_friction.cc: the residual
// sliding update against the play operator under stick → slip → reverse
// stick → reverse slip → reload cycles (UD-249, against a closed-form
// play operator), 2-D and 3-D, also with a change of the sliding
// direction; model-level Newton consistency of the implicit tangent
// (UD-247); trial idempotence
// of implicit friction (UD-247); zero tangential opening in contact
// (UD-225).

mod common;

use akantu_validation::reduce::NanMinMax;

use akantu_cohesive::SolidMechanicsModelCohesive;
use akantu_core::enums::{AnalysisMethod, NonLinearSolverType, SparseSolverType};
use akantu_core::{ElementType, GhostType, Real};
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::LawPhase;
use akantu_parser::Parser;

const NG: GhostType = GhostType::NotGhost;

/// Model-level Newton-consistency threshold (`FD_CONSISTENT`,
/// `docs/NUMERICAL_VALIDATION.md`: consistent tangents ≤ 8.2e-9, defects 0.14–166).
const FD_CONSISTENT: Real = 1e-6;

/// `CLOSED_FORM` (`independent_checks.rs`): 1e-13 of the scale.
const CLOSED_FORM: Real = 1e-13;

const MU: Real = 0.3;
const PENALTY: Real = 1e12;

fn dat(kf: Real, extra: &str) -> String {
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
  material cohesive_linear_friction [
    name = insertion
    beta = 0.8
    kappa = 1.5
    G_c = 10
    sigma_c = 1e6
    penalty = {PENALTY:e}
    mu = {MU}
    penalty_for_friction = {kf:e}
{extra}  ]
]
"
    )
}

/// The input of [`dat`] for another linear-family law `law` (the friction
/// keywords only for `cohesive_linear_friction`, with k_f = 1e11).
fn dat_law(law: &str, extra: &str) -> String {
    if law == "cohesive_linear_friction" {
        return dat(1e11, extra);
    }
    dat(1e11, extra)
        .replace(
            "material cohesive_linear_friction [",
            &format!("material {law} ["),
        )
        .replace(&format!("    mu = {MU}\n"), "")
        .replace(&format!("    penalty_for_friction = {:e}\n", 1e11), "")
}

struct M {
    model: SolidMechanicsModelCohesive,
    dim: usize,
    ct: ElementType,
    /// Sign of the normal's y component.
    ny: Real,
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

fn options(method: AnalysisMethod) -> ModelOptions {
    if method == AnalysisMethod::Static {
        ModelOptions::new(AnalysisMethod::Static).with_solver_options(
            akantu_solver::ModelSolverOptions {
                non_linear_solver_type: Some(NonLinearSolverType::Auto),
                sparse_solver_type: Some(SparseSolverType::Eigen),
                ..akantu_solver::ModelSolverOptions::default()
            },
        )
    } else {
        ModelOptions::new(method)
    }
}

/// Extrinsic model on `mesh` with `dat`, opened by a uniform mode-I stress
/// of 1.1 σc through the insertion check.
fn build(
    (mesh_file, dim, ct): (&str, usize, ElementType),
    dat: &str,
    method: AnalysisMethod,
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
    model.init_full(&options(method), true).unwrap();
    let mut m = M {
        model,
        dim,
        ct,
        ny: 1.0,
    };
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
    {
        let mut u_g = m.model.smm.displacement_mut().unwrap();
        let u = u_g.as_mut_slice();
        for (x, ui) in nodes.chunks(dim).zip(u.chunks_mut(dim)) {
            ui.fill(0.0);
            ui[1] = eyy * x[1];
        }
    }
    m.model.assemble_internal_forces().unwrap();
    assert!(m.model.check_cohesive_stress().unwrap() > 0);
    m.model.smm.displacement_mut().unwrap().fill(0.0);
    let _ = fint(&mut m);
    if let Some(y) = field(&m, "normal").get(1) {
        m.ny = if *y < 0.0 { -1.0 } else { 1.0 };
    }
    m
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

/// The entry of the next solve step: `cohesive_linear_friction` commits
/// its histories (`delta max`, `residual_sliding`) at step entry only (it
/// has no committed maxima, UD-245 not applied to it), so the trials of
/// a step after [`commit`]s start with it.
fn next_step(m: &mut M) {
    hook(m, LawPhase::BeforeSolveStep, None);
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

/// The first `nc` values of internal `name`.
fn q0(m: &M, name: &str, nc: usize) -> Vec<Real> {
    field(m, name)[..nc].to_vec()
}

fn norm(a: &[Real]) -> Real {
    a.iter().map(|x| x * x).sum::<Real>().sqrt()
}

/// A prescribed path of tangential openings (`dim − 1` components): linear
/// ramps from the origin through the corners, `(corner, steps)` each, with
/// the arithmetic of the closed-form reference.
fn path(corners: &[(Vec<Real>, u32)]) -> Vec<Vec<Real>> {
    let mut out: Vec<Vec<Real>> = Vec::new();
    let mut from = vec![0.0; corners[0].0.len()];
    for (to, steps) in corners {
        for k in 1..=*steps {
            let f = Real::from(k) / Real::from(*steps);
            out.push(from.iter().zip(to).map(|(a, b)| a + (b - a) * f).collect());
        }
        from.clone_from(to);
    }
    out
}

/// Result of [`friction_cycle`]: worst deviation from the play operator
/// and from a scalar transcription of upstream's update (both relative to
/// `τ_max`), first step of each deviation, steps per friction branch.
#[derive(Debug, Default)]
struct Cycle {
    worst_closed_form: Real,
    first_closed_form: Option<usize>,
    worst_upstream: Real,
    first_upstream: Option<usize>,
    stick: usize,
    slip: usize,
    reverse_stick: usize,
    reverse_slip: usize,
}

/// Drives point 0 of every cohesive element of an explicit model along the
/// tangential `path` at the penetration `dn`, one committed step per entry,
/// and compares the friction force and residual sliding (a) with the play
/// operator `s_n = clamp(s_{n−1}, x_n − r, x_n + r)`, `τ_n = k_f (x_n −
/// s_n)`, `r = τ_max/k_f`, `x = ‖Δt‖` (the exact solution of the scalar
/// rate-independent penalty friction law, any path) and (b) with upstream's
/// update `τ = sign(d)·min(k_f|d|, τ_max)`, `s = x − |τ|/k_f`, `d = x − s`,
/// chained from its own state.
fn friction_cycle(
    mesh: (&str, usize, ElementType),
    kf: Real,
    dn: Real,
    tangential: &[Vec<Real>],
    upstream: bool,
) -> Cycle {
    let mut m = build(
        mesh,
        &dat(kf, ""),
        AnalysisMethod::ExplicitLumpedMass,
        |md| {
            md.ext.compat.friction_residual_sliding_abs = upstream;
        },
    );
    let dim = m.dim;
    let tau_max = MU * PENALTY * dn.abs();
    let r = tau_max / kf;
    let (mut s_cf, mut s_up) = (0.0_f64, 0.0_f64);
    let mut c = Cycle::default();
    for (step, t) in tangential.iter().enumerate() {
        let open = if dim == 2 {
            v(2, t[0], dn, 0.0)
        } else {
            v(3, t[0], dn, t[1])
        };
        commit(&mut m, &open);
        // the law's view: on penetration `opening` holds the tangential
        // part; its norm in the law's reduction order (bitwise the law's x)
        let o = q0(&m, "opening", dim);
        let x = akantu_cohesive::vecops::norm_fixed(&o, dim);
        let ct = q0(&m, "contact_tractions", dim);
        assert!(
            (norm(&ct) / PENALTY - dn.abs()).abs() <= 1e-12 * dn.abs(),
            "step {step}: penetration"
        );
        let f = q0(&m, "friction_force", dim);
        // deviation of the force from `τ Δt/x` (the law's operation order)
        // and of the residual sliding, relative to τ_max
        let dev = |tau: Real, s_ref: Real, s: Real| {
            (0..dim)
                .map(|i| (f[i] - tau * o[i] / x).abs())
                .fold((s - s_ref).abs() * kf, NanMinMax::nan_max)
                / tau_max
        };
        let s = q0(&m, "residual_sliding", 1)[0];
        // (a) play operator
        let s_prev_cf = s_cf;
        s_cf = s_cf.max(x - r).min(x + r);
        let tau_cf = kf * (x - s_cf);
        let e_cf = dev(tau_cf, s_cf, s);
        if e_cf > CLOSED_FORM && c.first_closed_form.is_none() {
            c.first_closed_form = Some(step);
        }
        c.worst_closed_form = c.worst_closed_form.nan_max(e_cf);
        // branch of the closed form
        let d = x - s_prev_cf;
        match (d >= 0.0, kf * d.abs() < tau_max) {
            (true, true) => c.stick += 1,
            (true, false) => c.slip += 1,
            (false, true) => c.reverse_stick += 1,
            (false, false) => c.reverse_slip += 1,
        }
        // (b) upstream's update, chained from its own state
        let du = x - s_up;
        let mut tau_up = (kf * du.abs()).min(tau_max);
        if du < 0. {
            tau_up = -tau_up;
        }
        s_up = x - tau_up.abs() / kf;
        let e_up = dev(tau_up, s_up, s);
        if e_up > CLOSED_FORM && c.first_upstream.is_none() {
            c.first_upstream = Some(step);
        }
        c.worst_upstream = c.worst_upstream.nan_max(e_up);
    }
    c
}

/// The cycle of (μ 0.3, penalty and k_f 1e12, Δn = −2e-6,
/// τ_max = 6e5, r = 6e-7): tangential 0 → 2e-6 → 2e-7 → 1.2e-6 along one
/// direction (3-D: a fixed oblique direction, components 0.8 and 0.6),
/// 48 committed steps.
fn v92_cycle(dim: usize) -> Vec<Vec<Real>> {
    let dir: Vec<Real> = if dim == 2 { vec![1.0] } else { vec![0.8, 0.6] };
    let at = |x: Real| dir.iter().map(|d| d * x).collect::<Vec<_>>();
    path(&[(at(2e-6), 20), (at(2e-7), 18), (at(1.2e-6), 10)])
}

/// **UD-249**: with the corrected update the explicit model
/// follows the play operator through stick, slip, reverse stick, reverse
/// slip and reloading (2-D, and 3-D with a fixed oblique tangential
/// direction) within the closed-form bound (1e-13 τ_max; 9.7e-17 τ_max); the compat
/// switch reproduces upstream's update bitwise along the whole cycle and
/// leaves the closed form at the first reverse stick (step 26, worst
/// 1.17 τ_max).
#[test]
fn ud249_stick_slip_reverse_cycle_follows_the_play_operator() {
    let _g = common::lock();
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        let tangential = v92_cycle(dim);
        assert_eq!(tangential.len(), 48);
        let d = friction_cycle(mesh, 1e12, -2e-6, &tangential, false);
        eprintln!("{dim}-D default: {d:?}");
        assert_eq!(d.first_closed_form, None, "{dim}-D default: {d:?}");
        assert!(d.stick > 0 && d.slip > 0 && d.reverse_stick > 0 && d.reverse_slip > 0);
        assert_eq!(
            d.first_upstream,
            Some(26),
            "{dim}-D default differs from upstream"
        );
        let u = friction_cycle(mesh, 1e12, -2e-6, &tangential, true);
        eprintln!("{dim}-D upstream: {u:?}");
        assert_eq!(
            u.worst_upstream, 0.0,
            "{dim}-D compat: bitwise upstream's update"
        );
        assert_eq!(u.first_closed_form, Some(26), "{dim}-D compat");
        assert!(u.worst_closed_form > 1.0, "{dim}-D compat: {u:?}");
    }
}

/// **UD-249**, 3-D with a change of the sliding direction: slip along x,
/// then along z with x held (‖Δt‖ grows again), then back along x to a
/// smaller norm (reverse stick and reverse slip in the norm), then a
/// diagonal reload. The law is scalar in `x = ‖Δt‖` (upstream's modelling
/// choice, its rest point follows the direction of `Δt`), so the play
/// operator on the norm is its exact solution for any path: the corrected
/// update follows it within the closed-form bound; the friction force stays parallel to
/// `Δt`.
#[test]
fn ud249_direction_change_in_3d_follows_the_play_operator_on_the_norm() {
    let _g = common::lock();
    let tangential = path(&[
        (vec![1.5e-6, 0.0], 12),
        (vec![1.5e-6, 1.5e-6], 12),
        (vec![0.3e-6, 1.5e-6], 12),
        (vec![0.3e-6, 0.4e-6], 12),
        (vec![1.2e-6, 1.0e-6], 12),
    ]);
    let d = friction_cycle(mesh3(), 1e12, -2e-6, &tangential, false);
    eprintln!("3-D direction change, default: {d:?}");
    assert_eq!(d.first_closed_form, None, "{d:?}");
    assert!(d.stick > 0 && d.slip > 0 && d.reverse_stick > 0 && d.reverse_slip > 0);
    let u = friction_cycle(mesh3(), 1e12, -2e-6, &tangential, true);
    eprintln!("3-D direction change, upstream: {u:?}");
    assert_eq!(u.worst_upstream, 0.0, "compat: bitwise upstream's update");
    assert!(
        u.first_closed_form.is_some(),
        "compat leaves the closed form"
    );
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

/// `(label, committed openings, trial before, trial opening, upstream's
/// tangent consistent there)`.
type Scenario = (
    &'static str,
    Vec<Vec<Real>>,
    Option<Vec<Real>>,
    Vec<Real>,
    bool,
);

/// Friction scenarios (`v57_fd_newton_consistency`, k_f 1e11,
/// τ_max = 6e5 at Δn = −2e-6, r = 6e-6) and the friction branches from a
/// committed slip (‖Δt‖ = 2e-5 in 3-D, s = 1.4e-5; 1.6e-5 in 2-D, s = 1e-5):
/// forward stick, reverse stick (at Δn = −2.5e-6, r = 7.5e-6), reverse
/// slip; 2-D and 3-D.
fn scenarios(dim: usize) -> Vec<Scenario> {
    vec![
        (
            "loading mixed",
            vec![],
            None,
            v(dim, 3e-6, 6e-6, 1e-6),
            true,
        ),
        (
            "unloading mixed",
            vec![v(dim, 4e-6, 8e-6, 1e-6)],
            None,
            v(dim, 2e-6, 4e-6, 0.5e-6),
            true,
        ),
        (
            "contact, sliding below max (stick)",
            vec![v(dim, 0.0, 8e-6, 0.0)],
            None,
            v(dim, 3e-6, -2e-6, 1e-6),
            false,
        ),
        (
            "contact, sliding loading (slip)",
            vec![v(dim, 0.0, 2e-6, 0.0)],
            None,
            v(dim, 1.2e-5, -2e-6, 2e-6),
            false,
        ),
        (
            "contact after overshooting trial",
            vec![v(dim, 0.0, 8e-6, 0.0)],
            Some(v(dim, 0.0, 6e-5, 0.0)),
            v(dim, 3e-6, -2e-6, 1e-6),
            false,
        ),
        (
            "contact, sliding past delta_c",
            vec![v(dim, 0.0, 2e-6, 0.0)],
            None,
            v(dim, 4e-5, -2e-6, 0.5e-6),
            true,
        ),
        (
            "unloading after overshooting trial",
            vec![v(dim, 4e-6, 8e-6, 1e-6)],
            Some(v(dim, 0.0, 6e-5, 0.0)),
            v(dim, 2e-6, 4e-6, 0.5e-6),
            true,
        ),
        (
            "committed slip, forward stick",
            vec![v(dim, 0.0, -2e-6, 0.0), v(dim, 1.6e-5, -2e-6, 1.2e-5)],
            None,
            v(dim, 1.4e-5, -2e-6, 1.05e-5),
            false,
        ),
        (
            "committed slip, reverse stick",
            vec![v(dim, 0.0, -2e-6, 0.0), v(dim, 1.6e-5, -2e-6, 1.2e-5)],
            None,
            v(dim, 0.8e-5, -2.5e-6, 1.0e-5),
            false,
        ),
        (
            "committed slip, reverse slip",
            vec![v(dim, 0.0, -2e-6, 0.0), v(dim, 1.6e-5, -2e-6, 1.2e-5)],
            None,
            v(dim, 2.4e-6, -2e-6, 3.2e-6),
            false,
        ),
    ]
}

/// **UD-247**: the assembled stiffness of
/// implicit `cohesive_linear_friction` is the derivative of its assembled
/// internal force on every scenario (0.30–3.9 in contact), 2-D tri3
/// and 3-D tet4. Upstream's tangent (compat) is not consistent in contact
/// with friction (it is where the friction force is zero or constant:
/// open, and sliding past δc where the slip force has no Δn dependence…
/// see the scenario flags).
#[test]
fn ud247_friction_tangent_is_newton_consistent() {
    let _g = common::lock();
    let mut failures = Vec::new();
    let mut worst: Real = 0.0;
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        for (label, committed, before, trial, upstream_consistent) in scenarios(dim) {
            for upstream in [false, true] {
                let mut m = build(mesh, &dat(1e11, ""), AnalysisMethod::Static, |md| {
                    md.ext.compat.friction_upstream_implicit = upstream;
                });
                for o in &committed {
                    commit(&mut m, o);
                }
                next_step(&mut m);
                if label.starts_with("committed slip") {
                    // the committed slip left a residual sliding
                    let s_p = q0(&m, "residual_sliding", 1)[0];
                    assert!(s_p > 9e-6, "{dim}-D {label}: committed s {s_p:e}");
                }
                let err = fd_error(&mut m, &trial, before.as_deref(), 1e-11);
                eprintln!("{dim}-D {label:38} upstream tangent {upstream}: error {err:.2e}");
                if !upstream {
                    worst = worst.max(err);
                }
                let expected = !upstream || upstream_consistent;
                if (err <= FD_CONSISTENT) != expected {
                    failures.push(format!("{dim}-D {label} (upstream {upstream}): {err:e}"));
                }
            }
        }
    }
    eprintln!("ud247 model-level friction tangent: worst default error {worst:.2e}");
    assert!(failures.is_empty(), "{failures:?}");
}

/// **UD-1160** (D-099 (1)) at the model level: a point
/// failed past δc by a committed opening (Δn = 3e-5 > δc,eff = 2 G_c/σc,eff
/// ≈ 1.8e-5, inserted at 1.1 σc), then trials below `δ_max` in the next
/// step: open below δc and between δc and `δ_max`, compressed without
/// `contact_after_breaking` (no contact) and with it (the penalty and, for
/// the friction law, sticking friction). For `cohesive_linear`,
/// `cohesive_linear_fatigue` and `cohesive_linear_friction` (k_f 1e11),
/// tri3 and tet4, the default stiffness is the derivative of the internal
/// force (column error ≤ [`FD_CONSISTENT`]); with upstream's
/// negative secant (the switch `tangent_failed_secant` alone) it is not
/// (measured 1.0 open and 0.11 in contact after breaking
/// for the friction law).
#[test]
fn ud1160_failed_point_stiffness_is_newton_consistent() {
    let _g = common::lock();
    let mut failures = Vec::new();
    let mut worst: Real = 0.0;
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        let trials: [(&str, Vec<Real>, bool); 4] = [
            ("open, below delta_c", v(dim, 2e-6, 1e-5, 1e-6), false),
            (
                "open, between delta_c and delta_max",
                v(dim, 2e-6, 2.5e-5, 1e-6),
                false,
            ),
            (
                "compressed, no contact after breaking",
                v(dim, 3e-6, -2e-6, 1e-6),
                false,
            ),
            ("contact after breaking", v(dim, 3e-6, -2e-6, 1e-6), true),
        ];
        for law in [
            "cohesive_linear",
            "cohesive_linear_fatigue",
            "cohesive_linear_friction",
        ] {
            for (label, trial, cab) in &trials {
                for upstream in [false, true] {
                    let extra = if *cab {
                        "    contact_after_breaking = true\n"
                    } else {
                        ""
                    };
                    let mut m = build(mesh, &dat_law(law, extra), AnalysisMethod::Static, |md| {
                        md.ext.compat.tangent_failed_secant = upstream;
                    });
                    commit(&mut m, &v(dim, 0.0, 3e-5, 0.0));
                    next_step(&mut m);
                    let dc = q0(&m, "delta_c_eff", 1)[0];
                    let dm = q0(&m, "delta max", 1)[0];
                    assert!(
                        dm > dc,
                        "{dim}-D {law}: failed past delta_c ({dm:e}, {dc:e})"
                    );
                    let err = fd_error(&mut m, trial, None, 1e-11);
                    assert_eq!(
                        q0(&m, "damage", 1)[0],
                        1.0,
                        "{dim}-D {law} {label}: failed point"
                    );
                    let contact = q0(&m, "contact_tractions", dim).iter().any(|c| *c != 0.0);
                    assert_eq!(contact, *cab, "{dim}-D {law} {label}: contact");
                    eprintln!(
                        "{dim}-D {law:26} {label:38} upstream secant {upstream}: error {err:.2e}"
                    );
                    if !upstream {
                        worst = worst.max(err);
                    }
                    if (err <= FD_CONSISTENT) == upstream {
                        failures.push(format!(
                            "{dim}-D {law} {label} (upstream {upstream}): {err:e}"
                        ));
                    }
                }
            }
        }
    }
    eprintln!("ud1160 model-level stiffness after failure: worst default error {worst:.2e}");
    assert!(failures.is_empty(), "{failures:?}");
}

/// **UD-247** at the model level (scenario `v57_ud247_friction_trial_
/// dependence`): committed intact, a compression trial with sliding, a
/// trial past complete failure, the compression trial again. By default
/// the second evaluation is bitwise the first (internal force and every
/// friction internal); upstream (compat) loses the contact and the
/// friction after the overshooting trial.
#[test]
fn ud247_implicit_friction_trial_is_idempotent() {
    let _g = common::lock();
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        for upstream in [false, true] {
            let mut m = build(mesh, &dat(1e11, ""), AnalysisMethod::Static, |md| {
                md.ext.compat.friction_upstream_implicit = upstream;
            });
            commit(&mut m, &v(dim, 1e-6, 4e-6, 0.0));
            next_step(&mut m);
            let a = v(dim, 3e-6, -2e-6, 1e-6);
            let state = |m: &mut M| {
                let mut s: Vec<u64> = fint(m).iter().map(|x| x.to_bits()).collect();
                for name in [
                    "tractions",
                    "contact_tractions",
                    "friction_force",
                    "residual_sliding",
                    "damage",
                    "delta max",
                ] {
                    s.extend(field(m, name).iter().map(|x| x.to_bits()));
                }
                s
            };
            impose(&mut m, &a);
            let s1 = state(&mut m);
            let ff1 = field(&m, "friction_force");
            impose(&mut m, &v(dim, 0.0, 6e-5, 0.0));
            let _ = fint(&mut m);
            impose(&mut m, &a);
            let s2 = state(&mut m);
            let ff2 = field(&m, "friction_force");
            assert!(
                ff1.iter().any(|x| *x != 0.0),
                "{dim}-D: friction in contact"
            );
            if upstream {
                assert_ne!(
                    s1, s2,
                    "{dim}-D compat: the trial depends on the previous one"
                );
                assert!(
                    ff2.iter().all(|x| *x == 0.0),
                    "{dim}-D compat: contact lost"
                );
            } else {
                assert_eq!(s1, s2, "{dim}-D default: trial A after B");
            }
        }
    }
}

/// **UD-225** (A) at the model level: a committed slip, then the
/// tangential opening brought back to zero exactly in contact — upstream's
/// friction force is 0/0 = NaN there (and the internal force with it); the
/// port's is zero, the internal force finite, the residual sliding follows
/// the corrected update `s = −τ/k_f = min(s_p, r)`.
#[test]
fn ud225_zero_tangential_opening_in_contact_is_finite() {
    let _g = common::lock();
    for mesh in [mesh2(), mesh3()] {
        let dim = mesh.1;
        let kf = 1e12;
        let mut m = build(
            mesh,
            &dat(kf, ""),
            AnalysisMethod::ExplicitLumpedMass,
            |_| {},
        );
        commit(&mut m, &v(dim, 0.0, -2e-6, 0.0));
        commit(&mut m, &v(dim, 1.5e-6, -2e-6, 0.5e-6));
        let s_p = q0(&m, "residual_sliding", 1)[0];
        let r = MU * PENALTY * 2e-6 / kf;
        assert!(s_p > r, "{dim}-D: committed slip ({s_p:e})");
        commit(&mut m, &v(dim, 0.0, -2e-6, 0.0));
        let f = fint(&mut m);
        assert!(
            f.iter().all(|x| x.is_finite()),
            "{dim}-D: internal force finite"
        );
        let ff = q0(&m, "friction_force", dim);
        assert!(ff.iter().all(|x| *x == 0.0), "{dim}-D: {ff:?}");
        let s = q0(&m, "residual_sliding", 1)[0];
        // d = −s_p, |d| k_f > τ_max: reverse slip, τ = −τ_max, s = τ_max/k_f
        assert!(
            (s - r).abs() <= CLOSED_FORM * r,
            "{dim}-D: s {s:e} vs {r:e}"
        );
    }
}
