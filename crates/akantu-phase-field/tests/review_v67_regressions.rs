// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// for src/model/phase_field/phasefields/phasefield_linear.cc (initPhaseField,
// computeDrivingForce) and phasefield_linear_inline_impl.hh:
//
// * `pfm01_*` / UD-385: AT1 (`linear`) with `irreversibility_type =
//   penalization` and a non-positive recovery penalty
//   `ρ_rec = g_c/l0·9(diam/l0 − 2)/(64 tol_rec) ≤ 0` (mesh diagonal
//   `diam ≤ 2 l0`) is refused at initialisation;
// * `pfm02_*` / UD-388: AT1 with penalisation fully unloaded (φ = 0) without a
//   damage Dirichlet condition: the corrected tangent (active one-sided
//   derivative `γ·[d ≤ d_prev]`) keeps `K` positive definite and Newton
//   lands on the exact discrete solution `d = d_prev − (8/9)·tol_ir²`.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_model::ModelOptions;
use akantu_phase_field::{EnergySplit, EnergySplitKind, PhaseFieldUpstreamCompat};
use akantu_validation::reduce::NanMinMax;
use common::*;

/// `n × n` squares of the unit square, two triangles each; interior nodes
/// shifted (distorted mesh).
fn tri_mesh(n: usize) -> Mesh {
    let mut mesh = Mesh::new(2, "tris").unwrap();
    {
        let mut a = MeshAccessor::new(&mut mesh);
        for j in 0..=n {
            for i in 0..=n {
                let (x, y) = (i as Real / n as Real, j as Real / n as Real);
                let interior = i > 0 && i < n && j > 0 && j < n;
                let s = if interior {
                    0.09 * ((i * 3 + j * 5) % 7) as Real / 7.
                } else {
                    0.
                };
                a.push_node(&[x + s / n as Real, y - 0.5 * s / n as Real])
                    .unwrap();
            }
        }
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        let id = |i: usize, j: usize| (i + j * (n + 1)) as Idx;
        for j in 0..n {
            for i in 0..n {
                c.push_tuple(&[id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                c.push_tuple(&[id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
            }
        }
    }
    mesh.compute_bounding_box().unwrap();
    mesh
}

fn at1(l0: Real, irreversibility: &str) -> String {
    format!(
        "phasefield linear [\n name = b\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = {l0:?}\n \
         irreversibility_type = {irreversibility}\n irreversibility_tol = 2e-2\n \
         recovery_tol = 2e-2\n]\n"
    )
}

fn init(model: &mut akantu_phase_field::PhaseFieldModel) -> akantu_core::Result<()> {
    model.init_full(&ModelOptions::new(AnalysisMethod::Static))
}

/// Unit square (`diam = √2`), `l0 = 1` → `ρ_rec < 0`; `l0 = √2/2`
/// → `diam/l0 = 2` exactly, `ρ_rec = 0`: both refused at initialisation
/// with penalisation; a slightly smaller `l0` (`ρ_rec > 0`) is accepted.
#[test]
fn pfm01_at1_penalization_nonpositive_recovery_penalty_refused() {
    let diam = Real::sqrt(2.0);
    for l0 in [1.0, diam / 2.0, 10.0] {
        let mut model = model_with(tri_mesh(1), &at1(l0, "penalization"));
        let err = init(&mut model).expect_err("ρ_rec ≤ 0 with penalization must be refused");
        let msg = err.to_string();
        assert!(
            msg.contains("recovery penalty") && msg.contains("UD-385"),
            "l0 = {l0}: unexpected error: {msg}"
        );
    }
    let mut model = model_with(
        tri_mesh(1),
        &at1(diam / 2.0 * (1.0 - 1e-12), "penalization"),
    );
    init(&mut model).expect("ρ_rec > 0 is accepted");
}

/// The compat switch restores upstream's acceptance (the oracle case
/// `rev_at1_negative_rho_rec` replays bit for bit with it, including
/// upstream's two diverging Newton solves).
#[test]
fn pfm01_compat_switch_accepts_nonpositive_recovery_penalty() {
    let compat = PhaseFieldUpstreamCompat {
        accept_nonpositive_recovery_penalty: true,
        ..PhaseFieldUpstreamCompat::default()
    };
    for c in [compat, PhaseFieldUpstreamCompat::upstream()] {
        let mut model = model_with(tri_mesh(1), &at1(1.0, "penalization"));
        model.set_upstream_compat(c);
        init(&mut model).unwrap();
        assert_eq!(model.upstream_compat(), c);
        // The law carries the model's switches (read-only accessor)
        assert_eq!(model.phase_field(0).unwrap().base().compat(), c);
    }
}

/// `history` and `tao` never use `ρ_rec` (the upstream tests with
/// `l0 = 1` on the unit element use `history`), and AT2 has no recovery
/// penalty: no refusal.
#[test]
fn pfm01_refusal_only_for_at1_penalization() {
    for irr in ["history", "tao"] {
        let mut model = model_with(tri_mesh(1), &at1(1.0, irr));
        init(&mut model).unwrap_or_else(|e| panic!("{irr}: {e}"));
    }
    let at2 = "phasefield quadratic [\n name = b\n E = 210\n nu = 0.3\n gc = 5e-3\n l0 = 1.0\n \
               irreversibility_type = penalization\n]\n";
    let mut model = model_with(tri_mesh(1), at2);
    init(&mut model).unwrap();
}

/// UD-388: AT1 + penalisation, uniform tension then zero strain,
/// no damage boundary condition. Step 1 (`d > d_prev = 0`, penalties
/// inactive): `d = 1 − 3 g_c/(16 l0 φ)` with `φ = (λ/2 + μ) ε²`. Step 2
/// (`φ = 0`): the corrected tangent `γ·[d ≤ d_prev]` keeps `K` definite and
/// the exact discrete solution is uniform, `d = d_prev − 3g_c/(8 l0 γ) =
/// d_prev − (8/9)·tol²`. Upstream's `[d < d_prev]` leaves `K = ∫Bᵀ(¾g_c l0)B`
/// singular at the first iterate (structured mesh: sparse LU refuses it;
/// distorted meshes: Newton diverges or wanders).
#[test]
fn pfm02_at1_penalization_full_unload_converges_to_exact_solution() {
    let tol: Real = 2e-2;
    let (gc, l0, eps) = (5e-3, 0.05, 2e-2);
    let s = EnergySplit::new(EnergySplitKind::NoSplit, 210., 0.3, false);
    let phi = (0.5 * s.lambda + s.mu) * eps * eps;
    let d1_exact = 1.0 - 3.0 * gc / (16.0 * l0 * phi);
    for n in [1usize, 2, 3, 4] {
        let mut model = model_with(tri_mesh(n), &at1(l0, "penalization"));
        init(&mut model).unwrap();
        set_newton(&mut model, 100, 1e-13, "solution");
        set_uniform_strain(&mut model, &[0., 0., 0., eps]);
        model.solve_step(None).unwrap();
        let d1 = model.damage().unwrap().as_slice().to_vec();
        let mut err1: Real = 0.0;
        for &d in &d1 {
            err1 = err1.nan_max((d - d1_exact).abs());
            assert!(
                (d - d1_exact).abs() <= 1e-12,
                "n = {n}: step 1: {d} vs {d1_exact}"
            );
        }
        model.save_previous_state().unwrap();
        set_uniform_strain(&mut model, &[0.; 4]);
        model
            .solve_step(None)
            .unwrap_or_else(|e| panic!("n = {n}: full unload: {e}"));
        let nls = model.solver.non_linear_solver(None).unwrap();
        let it = nls.get_param("nb_iterations").unwrap().as_int().unwrap();
        assert!(it <= 3, "n = {n}: {it} Newton iterations");
        let mut err2: Real = 0.0;
        for (node, (&d, &dp)) in model
            .damage()
            .unwrap()
            .as_slice()
            .iter()
            .zip(&d1)
            .enumerate()
        {
            let want = dp - 8. / 9. * tol * tol;
            err2 = err2.nan_max((d - want).abs());
            assert!(
                (d - want).abs() <= 1e-12,
                "n = {n}, node {node}: {d} vs d_prev − 8/9 tol² = {want}"
            );
        }
        eprintln!(
            "tri_mesh({n}): step 1 max |d − d1| = {err1:e}; full unload: {it} iterations, \
             max |d − (d_prev − 8/9 tol²)| = {err2:e}"
        );
    }
}

/// With `at1_strict_irreversibility_indicator` (upstream) the
/// structured 2-triangle mesh reproduces upstream's singular tangent.
#[test]
fn pfm02_compat_switch_reproduces_singular_tangent() {
    let compat = PhaseFieldUpstreamCompat {
        at1_strict_irreversibility_indicator: true,
        ..PhaseFieldUpstreamCompat::default()
    };
    let mut model = model_with(tri_mesh(1), &at1(0.05, "penalization"));
    model.set_upstream_compat(compat);
    init(&mut model).unwrap();
    set_newton(&mut model, 100, 1e-13, "solution");
    set_uniform_strain(&mut model, &[0., 0., 0., 2e-2]);
    model.solve_step(None).unwrap();
    model.save_previous_state().unwrap();
    set_uniform_strain(&mut model, &[0.; 4]);
    let err = model
        .solve_step(None)
        .expect_err("upstream's tangent is singular");
    assert!(err.to_string().contains("singular"), "{err}");
}

/// `pfm02_*` on the inputs of the oracle case `rev_at1_full_unload_quadrangle_4`
/// (upstream patch mesh `_quadrangle_4.msh`, AT1 penalisation, uniform
/// `ε_yy = 2e-2` then zero strain, no damage BC), where upstream's Newton
/// diverges (error 2.0752587082923272e18 after 100 iterations, replayed bit
/// for bit in compat mode by `phase_field_fixtures_bitwise`). Corrected
/// default: both steps converge to the closed forms of
/// `pfm02_at1_penalization_full_unload_converges_to_exact_solution`.
#[test]
fn pfm02_oracle_case_inputs_full_unload_exact() {
    let dir = akantu_validation::fixtures_root()
        .join("model/phase_field/rev_at1_full_unload_quadrangle_4/inputs");
    let mesh = read_mesh(&dir.join("_quadrangle_4.msh"), 2);
    let dat = std::fs::read_to_string(dir.join("rev_at1_full_unload_quadrangle_4.dat")).unwrap();
    let mut model = model_with(mesh, &dat);
    init(&mut model).unwrap();
    set_newton(&mut model, 100, 1e-13, "solution");
    let (gc, l0, tol, eps) = (5e-3, 0.05, 2e-2, 2e-2);
    let s = EnergySplit::new(EnergySplitKind::NoSplit, 210., 0.3, false);
    let phi = (0.5 * s.lambda + s.mu) * eps * eps;
    set_uniform_strain(&mut model, &[0., 0., 0., eps]);
    model.solve_step(None).unwrap();
    let d1 = model.damage().unwrap().as_slice().to_vec();
    let mut err1: Real = 0.0;
    for &d in &d1 {
        let want = 1.0 - 3.0 * gc / (16.0 * l0 * phi);
        err1 = err1.nan_max((d - want).abs());
        assert!((d - want).abs() <= 1e-12, "step 1: {d} vs {want}");
    }
    model.save_previous_state().unwrap();
    set_uniform_strain(&mut model, &[0.; 4]);
    model.solve_step(None).unwrap();
    let mut err2: Real = 0.0;
    for (&d, &dp) in model.damage().unwrap().as_slice().iter().zip(&d1) {
        let want = dp - 8. / 9. * tol * tol;
        err2 = err2.nan_max((d - want).abs());
        assert!((d - want).abs() <= 1e-12, "step 2: {d} vs {want}");
    }
    eprintln!("oracle-case inputs: step 1 max error {err1:e}, full unload max error {err2:e}");
}
