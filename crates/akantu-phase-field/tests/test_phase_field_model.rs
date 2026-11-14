// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports of
// test/test_model/test_phase_field_model/*.cc (same inputs, same intent).
//
// * test_phasefield_selector.cc   → `test_phasefield_selector` (TEST-090)
// * test_phase_field_optimal.cc   → `test_phase_field_optimal` (TEST-094)
// * the phase-field half of the coupled one-element tests (the solid half
//   and the coupler belong to akantu-couplers): the strain the solid produces for
//   the upstream loading (all nodes blocked, top `u_y = ε`, so
//   `ε = [[0, 0], [0, ε]]` exactly) is written into the laws' `strain`
//   internal, the phase field is solved as `coupler.solve` /
//   `phase.solveStep()` does, and the damage (interpolated on the
//   quadrature points as `computeDamageOnQuadPoints` does) is checked
//   against the upstream analytical damage with the upstream tolerances:
//   - test_phase_solid_coupling.cc        → `test_phase_solid_coupling_phase_field` (TEST-091)
//   - test_phase_field_anisotropic.cc     → `test_phase_field_anisotropic_phase_field` (TEST-092)
//   - test_phase_field_deviatoric_split.cc → `test_phase_field_deviatoric_split_phase_field` (TEST-093)
//   - test_phase_solid_explicit.cc        → `test_phase_solid_explicit_phase_field` (TEST-096)
//   - test_penalization.cc                → `test_penalization_phase_field` (TEST-098)
//   - test_phase_field_linear.cc          → `test_phase_field_linear_phase_field` (TEST-099)
//
// test_phase_field_optimal_linear.cc (TEST-095) and test_phase_field_tao.cc
// (TEST-100) need the PETSc TAO bound-constrained solver (NLS-007);
// test_multi_material.cc (TEST-097, UNSTABLE upstream) needs the
// coupler with an explicit solid.

mod common;

use akantu_core::enums::SpatialDirection;
use akantu_core::{ElementType, GhostType, Real};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_phase_field::selector::MeshDataPhaseFieldSelector;
use akantu_phase_field::{PhaseFieldModel, PhaseFieldUpstreamCompat};
use akantu_validation::reduce::NanMinMax;
use common::*;

/// Damage interpolated on the quadrature points of `ty` (upstream
/// `computeDamageOnQuadPoints`: `fem.interpolateOnIntegrationPoints(
/// phase.getDamage(), solid_damage, 1, type, ghost)`).
fn damage_on_qpoints(model: &PhaseFieldModel, ty: ElementType) -> Vec<Real> {
    let mesh_h = model.mesh_handle().clone();
    let mesh = mesh_h.read().unwrap();
    let mut out = akantu_core::Array::new(1);
    model
        .fe_engine()
        .unwrap()
        .interpolate_on_integration_points(
            &mesh,
            model.damage().unwrap().view(),
            &mut out,
            1,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
    out.as_slice().to_vec()
}

fn c22(model: &PhaseFieldModel) -> Real {
    let e = param(model, 0, "E");
    let nu = param(model, 0, "nu");
    e * (1. - nu) / ((1. + nu) * (1. - 2. * nu))
}

/// `phase.setPhaseFieldSelector(MeshDataPhaseFieldSelector<std::string>(
/// "physical_names", phase))`.
fn physical_names_selector(model: &mut PhaseFieldModel) {
    model.set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
}

/// test_phasefield_selector.cc: elements 0/1/2 get the phase fields
/// `chocolate` / `chewing-gum` / `candy` from their physical names
/// (`initFull()`: default analysis method, explicit lumped mass).
#[test]
fn test_phasefield_selector() {
    let mut model = model_from_files("phasefield_selector.msh", "phasefield_selector.dat", 1);
    physical_names_selector(&mut model);
    model.init_full(&ModelOptions::default()).unwrap();
    let first = |name: &str| {
        model
            .phase_field_by_name(name)
            .unwrap()
            .base()
            .filter(ElementType::Segment2, GhostType::NotGhost)[0]
    };
    assert_eq!(first("chocolate"), 0);
    assert_eq!(first("chewing-gum"), 1);
    assert_eq!(first("candy"), 2);
    // parameters parsed per law
    assert_eq!(param(&model, 0, "l0"), 2.0);
    assert_eq!(param(&model, 1, "gc"), 1.0);
    assert_eq!(param(&model, 2, "E"), 3.0);
}

/// Exact solution of the discrete problem of test_phase_field_optimal: the
/// phase field with zero strain on a strip of unit height meshed with one
/// row of rectangular bilinear elements, `d = 1` on the left edge. A
/// `y`-independent field is the exact discrete solution (symmetry), so the
/// 2-D system reduces to the 1-D one with, per element of width `h`,
/// `g_c l0/h [[1, −1], [−1, 1]] + g_c/l0 · h/6 [[2, 1], [1, 2]]` (2×2 Gauss
/// integrates both exactly); solved by the Thomas algorithm.
fn optimal_discrete_solution(x: &[Real], gc: Real, l0: Real) -> Vec<Real> {
    let n = x.len();
    let mut diag = vec![0.0; n];
    let mut off = vec![0.0; n - 1];
    for e in 0..n - 1 {
        let h = x[e + 1] - x[e];
        let k = gc * l0 / h;
        let m = gc / l0 * h / 6.;
        diag[e] += k + 2. * m;
        diag[e + 1] += k + 2. * m;
        off[e] += -k + m;
    }
    // d_0 = 1: eliminate the first unknown
    let mut rhs = vec![0.0; n];
    rhs[1] -= off[0];
    let (mut c, mut d) = (vec![0.0; n], vec![0.0; n]);
    let mut out = vec![0.0; n];
    out[0] = 1.0;
    c[1] = if n > 2 { off[1] / diag[1] } else { 0.0 };
    d[1] = rhs[1] / diag[1];
    for i in 2..n {
        let a = off[i - 1];
        let denom = diag[i] - a * c[i - 1];
        c[i] = if i + 1 < n { off[i] / denom } else { 0.0 };
        d[i] = (rhs[i] - a * d[i - 1]) / denom;
    }
    out[n - 1] = d[n - 1];
    for i in (1..n - 1).rev() {
        out[i] = d[i] - c[i] * out[i + 1];
    }
    out
}

/// test_phase_field_optimal.cc: AT2 phase field without strain, `d = 1` on
/// the edge `x = 0` (group `blocked`) of a 1000 × 1 strip, `l0 = 20`; the
/// nodal damage is compared with the optimal profile
/// `cosh((L − x)/l0) / cosh(L/l0)`.
///
/// Upstream only prints the nodes whose relative error exceeds `1e-8` (its
/// `return EXIT_FAILURE` is commented out: linear elements with
/// `h/l0 = 1/20` cannot reach it). The port asserts (a) the nodal damage
/// equals the exact solution of the discrete problem
/// ([`optimal_discrete_solution`]) to `1e-9` in the max norm (`max d = 1`;
/// sparse LU of a system with condition number ≈ 4 l0²/h² ≈ 1.6·10³ and
/// node abscissae of the two rows equal to 3.2·10⁻¹⁰; measured 8.9·10⁻¹⁴:
/// `docs/NUMERICAL_VALIDATION.md`), and (b) where the profile is
/// above `1e-12`, the relative discretisation error against the analytical
/// profile is below the a-priori bound `(x/l0 + 1)·(h/l0)²/12` (the
/// discrete decay rate has a phase error `(h/l0)³/24` per element).
#[test]
fn test_phase_field_optimal() {
    let mut model = model_from_files("test_optimal_damage.msh", "material_optimal.dat", 2);
    init_static(&mut model);
    set_newton(&mut model, 1000, 1e-6, "residual");
    let l0 = param(&model, 0, "l0");
    let gc = param(&model, 0, "gc");
    let big_l = 1000.;
    model
        .apply_dirichlet_bc(
            &FixedValue {
                value: 1.,
                axis: SpatialDirection::X,
            },
            "blocked",
        )
        .unwrap();
    model.solve_step(Some("static")).unwrap();
    model.save_previous_state().unwrap();
    let mesh_h = model.mesh_handle().clone();
    let mesh = mesh_h.read().unwrap();
    let nodes_g = mesh.nodes();
    let nodes = nodes_g.as_slice();
    let damage_g = model.damage().unwrap();
    let damage = damage_g.as_slice();
    // 1-D reduction: the bottom row (y = 0) and the top row (y = 1) have the
    // same abscissae up to the mesh file's rounding (≤ 3.2·10⁻¹⁰)
    let mut bottom: Vec<(Real, usize)> = (0..mesh.nb_nodes())
        .filter(|&n| nodes[2 * n + 1] == 0.0)
        .map(|n| (nodes[2 * n], n))
        .collect();
    let mut top: Vec<(Real, usize)> = (0..mesh.nb_nodes())
        .filter(|&n| nodes[2 * n + 1] == 1.0)
        .map(|n| (nodes[2 * n], n))
        .collect();
    bottom.sort_by(|a, b| a.0.total_cmp(&b.0));
    top.sort_by(|a, b| a.0.total_cmp(&b.0));
    assert_eq!(bottom.len() + top.len(), mesh.nb_nodes());
    assert_eq!(bottom.len(), top.len());
    for (b, t) in bottom.iter().zip(&top) {
        assert!(
            (b.0 - t.0).abs() <= 1e-9,
            "rectangular elements: {} vs {}",
            b.0,
            t.0
        );
    }
    let x: Vec<Real> = bottom
        .iter()
        .zip(&top)
        .map(|(b, t)| 0.5 * (b.0 + t.0))
        .collect();
    let exact_discrete = optimal_discrete_solution(&x, gc, l0);
    let optimal = |x: Real| {
        (x / l0).exp() / ((2. * big_l / l0).exp() + 1.)
            + (-x / l0).exp() * (2. * big_l / l0).exp() / ((2. * big_l / l0).exp() + 1.)
    };
    let h_max = x
        .windows(2)
        .map(|w| w[1] - w[0])
        .fold(0.0, NanMinMax::nan_max);
    let mut worst_discrete: Real = 0.0;
    let mut worst_analytical: Real = 0.0;
    let mut nb_analytical = 0;
    for row in [&bottom, &top] {
        for (j, &(xj, n)) in row.iter().enumerate() {
            let d = damage[n];
            // normwise (max |d| = 1): the profile decays to 4·10⁻²² at x = L,
            // below the LU's absolute rounding
            let e = (d - exact_discrete[j]).abs();
            worst_discrete = worst_discrete.nan_max(e);
            assert!(
                e <= 1e-9,
                "node {n} (x = {xj}): {d} vs discrete {}",
                exact_discrete[j]
            );
            let a = optimal(xj);
            if a < 1e-12 {
                continue;
            }
            nb_analytical += 1;
            let bound = (xj / l0 + 1.) * (h_max / l0).powi(2) / 12.;
            let ea = rel(d, a);
            worst_analytical = worst_analytical.nan_max(ea);
            assert!(
                ea <= bound,
                "node {n} (x = {xj}): {d} vs optimal {a} (bound {bound})"
            );
        }
    }
    assert!(nb_analytical > 1000);
    eprintln!(
        "test_phase_field_optimal: max rel. error vs discrete solution {worst_discrete:e}, vs \
         optimal profile {worst_analytical:e}"
    );
}

/// Uniform one-element loading of the coupled tests: write the strain,
/// solve, return the damage at quadrature point 0.
fn step(model: &mut PhaseFieldModel, strain: &[Real], save: bool) -> Real {
    set_uniform_strain(model, strain);
    model.solve_step(None).unwrap();
    if save {
        model.save_previous_state().unwrap();
    }
    damage_on_qpoints(model, ElementType::Quadrangle4)[0]
}

/// test_phase_solid_coupling.cc (phase-field half): AT2 without split,
/// `ε_yy = 10⁻⁴ s`, 1000 steps; `d = ε² c22 / (g_c/l0 + ε² c22)` and the
/// dissipated energy `d² g_c / (2 l0)` (unit area) to `1e-8` relative.
#[test]
fn test_phase_solid_coupling_phase_field() {
    coupling_one_element("material_coupling.dat");
}

/// test_phase_solid_explicit.cc (phase-field half): the phase field is
/// solved statically after each explicit solid step with the same material
/// file and loading as test_phase_solid_coupling.cc, so its phase-field
/// half is the same check (the explicit solid belongs to akantu-couplers).
#[test]
fn test_phase_solid_explicit_phase_field() {
    coupling_one_element("material_coupling.dat");
}

fn coupling_one_element(dat: &str) {
    let mut model = model_from_files("test_one_element.msh", dat, 2);
    physical_names_selector(&mut model);
    init_static(&mut model);
    let c22 = c22(&model);
    let gc = param(&model, 0, "gc");
    let l0 = param(&model, 0, "l0");
    for s in 0..1000 {
        let eps = 1e-4 * Real::from(s);
        let d = step(&mut model, &uniaxial(2, eps), false);
        let d_an = eps * eps * c22 / (gc / l0 + eps * eps * c22);
        let e_an = d_an * d_an * 0.5 * gc / l0;
        let energy = model.energy("dissipated").unwrap();
        if s == 0 {
            assert_eq!(d, 0.0);
            assert_eq!(energy, 0.0);
            continue;
        }
        assert!(rel(d, d_an) <= 1e-8, "step {s}: damage {d} vs {d_an}");
        assert!(
            rel(energy, e_an) <= 1e-8,
            "step {s}: energy {energy} vs {e_an}"
        );
    }
}

/// test_phase_field_deviatoric_split.cc (phase-field half): AT2 with the
/// volumetric–deviatoric split, loading / unloading into compression /
/// reloading (1500 steps of 10⁻⁵), `savePreviousState` after each step
/// (history); `d = 2(l0/g_c) φ⁺_max / (2(l0/g_c) φ⁺_max + 1)` to `1e-8`
/// relative (upstream tolerance). Upstream's analytical driving energy,
/// `φ⁺ = ½κε² + (5/9)με²` in tension and `(5/9)με²` in compression, is the
/// compat value (UD-387: the 2-D deviator without its out-of-plane
/// component); the corrected default (D-066 (2)) is the plane-strain
/// `φ⁺ = ½κε² + (2/3)με²` and `(2/3)με²`, the potential of the degraded
/// stress.
#[test]
fn test_phase_field_deviatoric_split_phase_field() {
    deviatoric_split_run(Some(PhaseFieldUpstreamCompat::upstream()), 5. / 9.);
    deviatoric_split_run(None, 2. / 3.);
}

fn deviatoric_split_run(compat: Option<PhaseFieldUpstreamCompat>, dev_coefficient: Real) {
    let mut model = model_from_files("test_one_element.msh", "material_deviatoric.dat", 2);
    physical_names_selector(&mut model);
    if let Some(c) = compat {
        model.set_upstream_compat(c);
    }
    init_static(&mut model);
    set_newton(&mut model, 1000, 1e-6, "solution");
    let e = param(&model, 0, "E");
    let nu = param(&model, 0, "nu");
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. + 2. * nu);
    let kappa = lambda + 2. * mu / 3.;
    let gc = param(&model, 0, "gc");
    let l0 = param(&model, 0, "l0");
    let mut max_strain_energy: Real = 0.0;
    let mut checked = 0;
    for s in 0..1500u32 {
        let s_r = Real::from(s);
        let eps = if s < 500 {
            1e-5 * s_r
        } else if s < 1000 {
            (1500. - 2. * s_r) * 1e-5
        } else {
            (3. * s_r - 3500.) * 1e-5
        };
        let plus = if eps > 0. {
            eps * eps * 0.5 * kappa + mu * eps * eps * dev_coefficient
        } else {
            dev_coefficient * eps * eps * mu
        };
        max_strain_energy = max_strain_energy.nan_max(plus);
        let d = step(&mut model, &uniaxial(2, eps), true);
        let d_an = 2. * (l0 / gc) * max_strain_energy / (2. * (l0 / gc) * max_strain_energy + 1.);
        if eps.abs() > 1e-13 {
            assert!(
                rel(d, d_an) <= 1e-8,
                "compat {}: step {s} (ε = {eps}): {d} vs {d_an}",
                compat.is_some()
            );
            checked += 1;
        }
    }
    assert!(checked > 1400);
}

/// Bound of [`penalized_damage`] checks (relative,
/// tightened from `1e-8`). The residual is
/// piecewise linear in `d`; on both load paths below every compared step
/// makes at least one Newton iteration (`test_penalization_phase_field`
/// uses the solution criterion; in `test_phase_field_anisotropic_phase_field`
/// only the uncompared `ε = 0` step makes none), so the free branch is
/// solved exactly up to rounding and an iteration stopped in the penalized
/// branch leaves `|d − d*| ≤ threshold / (2φ + g_c/l0 + γ) ≤ 1e-6 / 5·10⁵ =
/// 2·10⁻¹²` absolute, i.e. `≤ 4·10⁻¹²` relative since `d ≥ 0.58` there
/// (unloading from the maximum load). Measured (printed by the tests):
/// `≤ 8.8·10⁻¹⁵` (docs/NUMERICAL_VALIDATION.md).
const EXACT_PENALIZED_REL: Real = 1e-11;

/// Exact damage of the penalized AT2 point problem with a uniform field
/// (no gradient): `−2φ + d(2φ + g_c/l0) + γ min(0, d − d_prev) = 0`.
fn penalized_damage(phi: Real, gc: Real, l0: Real, gamma: Real, d_prev: Real) -> Real {
    let free = 2. * phi / (2. * phi + gc / l0);
    if free >= d_prev {
        free
    } else {
        (2. * phi + gamma * d_prev) / (2. * phi + gc / l0 + gamma)
    }
}

fn law_previous_damage(model: &PhaseFieldModel) -> Real {
    model
        .phase_field(0)
        .unwrap()
        .base()
        .damage
        .previous(ElementType::Quadrangle4, GhostType::NotGhost)
        .unwrap()
        .as_slice()[0]
}

/// test_phase_field_anisotropic.cc (phase-field half). Upstream reads
/// `material_penalization.dat` (AT2, **no split**, penalization,
/// `l0 = 1`, `tol_ir = 1e-4`) although its analytical damage assumes the
/// volumetric–deviatoric split, and fails only for relative errors above
/// `1e3`; the port keeps that check (same intent, same tolerance) and adds
/// the exact solution of the penalized point problem the model solves
/// ([`penalized_damage`], [`EXACT_PENALIZED_REL`]).
#[test]
fn test_phase_field_anisotropic_phase_field() {
    let mut model = model_from_files("test_one_element.msh", "material_penalization.dat", 2);
    physical_names_selector(&mut model);
    init_static(&mut model);
    set_newton(&mut model, 1000, 1e-6, "residual");
    let e = param(&model, 0, "E");
    let nu = param(&model, 0, "nu");
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. + 2. * nu);
    let gc = param(&model, 0, "gc");
    let l0 = param(&model, 0, "l0");
    let tol: Real = param(&model, 0, "irreversibility_tol");
    let gamma = gc / l0 * (1. / (tol * tol) - 1.);
    let c22 = c22(&model);
    let mut analytical_damage: Real = 0.0;
    let mut max_rel: Real = 0.0;
    for s in 0..1500u32 {
        let s_r = Real::from(s);
        let eps = if s < 500 {
            1e-4 * s_r
        } else if s < 1000 {
            (1500. - 2. * s_r) * 1e-4
        } else {
            (3. * s_r - 3500.) * 1e-4
        };
        let d_prev = law_previous_damage(&model);
        let d = step(&mut model, &uniaxial(2, eps), true);
        let plus = if eps > 0. {
            eps * eps * (0.5 * lambda + mu)
        } else {
            2. * eps * eps * mu / 3.
        };
        let new_damage = 2. * (l0 / gc) * plus / (2. * (l0 / gc) * plus + 1.);
        analytical_damage = analytical_damage.nan_max(new_damage);
        if eps.abs() > 1e-13 {
            assert!(rel(d, analytical_damage) <= 1e3, "step {s}: upstream check");
            let exact = penalized_damage(0.5 * c22 * eps * eps, gc, l0, gamma, d_prev);
            max_rel = max_rel.max(rel(d, exact));
            assert!(
                rel(d, exact) <= EXACT_PENALIZED_REL,
                "step {s} (ε = {eps}): {d} vs exact {exact}"
            );
        }
    }
    eprintln!(
        "test_phase_field_anisotropic_phase_field: max relative error vs the exact penalized damage {max_rel:e}"
    );
}

/// test_penalization.cc (phase-field half): AT2 with penalization on four
/// elements (the solid gives a uniform `ε_yy`), loading to `5·10⁻³` then
/// unloading (steps of 10⁻⁵); upstream compares with the irreversible
/// analytical damage `max_t d(ε_t)` to `1e-3` (the penalty lets the damage
/// decrease by `O(1/γ)`); the port keeps that check and adds the exact
/// penalized solution ([`penalized_damage`], [`EXACT_PENALIZED_REL`]).
#[test]
fn test_penalization_phase_field() {
    let mut model = model_from_files("test_four_elements.msh", "material_penalization.dat", 2);
    physical_names_selector(&mut model);
    init_static(&mut model);
    set_newton(&mut model, 1000, 1e-6, "solution");
    let c22 = c22(&model);
    let gc = param(&model, 0, "gc");
    let l0 = param(&model, 0, "l0");
    let tol: Real = param(&model, 0, "irreversibility_tol");
    let gamma = gc / l0 * (1. / (tol * tol) - 1.);
    let mut eps: Real = 0.0;
    let mut analytical_damage: Real = 0.0;
    let mut max_rel: Real = 0.0;
    for s in 0..1000 {
        if s < 500 {
            eps += 1e-5;
        } else {
            eps -= 1e-5;
        }
        let d_prev = law_previous_damage(&model);
        set_uniform_strain(&mut model, &uniaxial(2, eps));
        model.solve_step(None).unwrap();
        model.save_previous_state().unwrap();
        let dq = damage_on_qpoints(&model, ElementType::Quadrangle4);
        let new_damage = eps * eps * c22 / (gc / l0 + eps * eps * c22);
        analytical_damage = analytical_damage.nan_max(new_damage);
        if eps.abs() > 1e-13 {
            let exact = penalized_damage(0.5 * c22 * eps * eps, gc, l0, gamma, d_prev);
            for &d in &dq {
                assert!(
                    rel(d, analytical_damage) <= 1e-3,
                    "step {s}: {d} vs {analytical_damage}"
                );
                max_rel = max_rel.max(rel(d, exact));
                assert!(
                    rel(d, exact) <= EXACT_PENALIZED_REL,
                    "step {s} (ε = {eps}): {d} vs exact {exact}"
                );
            }
        }
    }
    eprintln!(
        "test_penalization_phase_field: max relative error vs the exact penalized damage {max_rel:e}"
    );
}

/// test_phase_field_linear.cc (phase-field half): AT1 (`linear`, history,
/// no `savePreviousState`), `ε_yy = 10⁻⁴ s`;
/// `d = max(0, 1 − 3g_c/(8 l0 ε² c22))` with the upstream absolute
/// tolerance (`1e-5` while `d < 1e-8`, else `1e-8`), and the dissipated
/// energy `3 g_c d / (8 l0)` to `1e-8` relative once `d > 0`.
#[test]
fn test_phase_field_linear_phase_field() {
    let mut model = model_from_files("test_one_element.msh", "material_linear.dat", 2);
    physical_names_selector(&mut model);
    init_static(&mut model);
    let c22 = c22(&model);
    let gc = param(&model, 0, "gc");
    let l0 = param(&model, 0, "l0");
    let mut damaged = 0;
    for s in 0..1000 {
        let eps = 1e-4 * Real::from(s);
        let d = step(&mut model, &uniaxial(2, eps), false);
        let d_an = (1. - 3. * gc / (8. * l0 * eps * eps * c22)).max(0.);
        let tol = if d_an < 1e-8 { 1e-5 } else { 1e-8 };
        if s > 0 {
            assert!((d_an - d).abs() <= tol, "step {s}: {d} vs {d_an}");
        }
        if d_an > 1e-8 {
            damaged += 1;
            let e_an = d_an * 3. * gc / (8. * l0);
            let energy = model.energy("dissipated").unwrap();
            assert!(
                rel(energy, e_an) <= 1e-8,
                "step {s}: energy {energy} vs {e_an}"
            );
        }
    }
    assert!(
        damaged > 500,
        "the AT1 threshold is crossed early in the history"
    );
}
