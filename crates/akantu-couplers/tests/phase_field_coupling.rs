// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Same-intent ports
// (upstream inputs, loadings, step counts and tolerances) of the coupled
// solid–phase-field tests of test/test_model/test_phase_field_model:
// test_phase_solid_coupling.cc (TEST-091), test_phase_field_anisotropic.cc
// (TEST-092), test_phase_field_deviatoric_split.cc (TEST-093),
// test_phase_solid_explicit.cc (TEST-096), test_multi_material.cc
// (TEST-097), test_penalization.cc (TEST-098), test_phase_field_linear.cc
// (TEST-099), through CouplerSolidPhaseField (coupler_solid_phasefield.cc).
// TEST-091 and TEST-099 couple the models upstream with local helper
// functions doing the same staggered pass for a single law; the ports call
// the coupler.

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, GhostType, Real};
use akantu_couplers::{CouplerSolidPhaseField, CouplerSolidPhaseFieldCompat};
use akantu_mesh::SharedMesh;
use akantu_model::ModelOptions;
use akantu_model::constitutive_law::{ElementDataSelector, InternalFieldRef};
use akantu_parser::Parser;
use akantu_parser::param::ParamValue;
use akantu_phase_field::selector::MeshDataPhaseFieldSelector;
use common::*;

const Q4: ElementType = ElementType::Quadrangle4;
const NG: GhostType = GhostType::NotGhost;

fn pf_data(name: &str) -> std::path::PathBuf {
    data_dir().join("test_phase_field_model").join(name)
}

/// Coupler on the upstream mesh and material file; the phase field gets the
/// `physical_names` selector (all upstream tests), the solid too when
/// `material_selector` (test_multi_material).
fn coupler(msh: &str, dat: &str, material_selector: bool) -> CouplerSolidPhaseField {
    let mesh = read_mesh(&pf_data(msh), 2);
    let parser = Parser::parse_file(pf_data(dat)).unwrap();
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .unwrap();
    if material_selector {
        c.solid
            .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    }
    c.phase
        .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    c
}

fn init(c: &mut CouplerSolidPhaseField, solid: AnalysisMethod) {
    c.solid.init_full(&ModelOptions::new(solid)).unwrap();
    c.phase
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
}

fn set_newton(
    nls: &mut dyn akantu_solver::NonLinearSolver,
    max_iterations: i64,
    threshold: f64,
    criterion: &str,
) {
    nls.set_param("max_iterations", ParamValue::Int(max_iterations))
        .unwrap();
    nls.set_param("threshold", ParamValue::Real(threshold))
        .unwrap();
    nls.set_param("convergence_type", ParamValue::Enum(criterion.into()))
        .unwrap();
}

fn solid_newton(c: &mut CouplerSolidPhaseField, it: i64, thr: f64, crit: &str) {
    set_newton(
        c.solid.solver.non_linear_solver_mut(None).unwrap(),
        it,
        thr,
        crit,
    );
}

fn phase_newton(c: &mut CouplerSolidPhaseField, it: i64, thr: f64, crit: &str) {
    set_newton(
        c.phase.solver.non_linear_solver_mut(None).unwrap(),
        it,
        thr,
        crit,
    );
}

/// `applyDisplacement` of the upstream tests: nodes at `y = bottom` fully
/// blocked at 0, nodes at `y = top` blocked with `u = (0, ε)`, the others
/// (four-element and two-element meshes) blocked in x only.
fn apply_displacement(c: &mut CouplerSolidPhaseField, eps: Real, bottom: Real, top: Real) {
    let nodes = c.solid.physics.base.mesh.read().unwrap().nodes().clone();
    let (u, b) = {
        let u = c.solid.displacement().unwrap().as_slice().to_vec();
        let b = c.solid.blocked_dofs().unwrap().as_slice().to_vec();
        (u, b)
    };
    let (mut u, mut b) = (u, b);
    for (n, x) in nodes.as_slice().as_chunks::<2>().0.iter().enumerate() {
        let y = x[1];
        if y == bottom {
            u[2 * n] = 0.;
            u[2 * n + 1] = 0.;
            b[2 * n] = true;
            b[2 * n + 1] = true;
        } else if y == top {
            u[2 * n] = 0.;
            u[2 * n + 1] = eps;
            b[2 * n] = true;
            b[2 * n + 1] = true;
        } else {
            u[2 * n] = 0.;
            b[2 * n] = true;
        }
    }
    c.solid
        .displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u);
    c.solid
        .blocked_dofs_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&b);
}

fn pf_param(c: &CouplerSolidPhaseField, name: &str) -> Real {
    c.phase
        .phase_field(0)
        .unwrap()
        .get_param(name)
        .unwrap()
        .as_real()
        .unwrap()
}

/// `model.getMaterial(i).getArray<Real>(name, _quadrangle_4)`.
fn material_array(c: &CouplerSolidPhaseField, i: usize, name: &str) -> Vec<Real> {
    let mut out = None;
    c.solid.material(i).unwrap().for_each_internal(&mut |n, f| {
        if n == name
            && let InternalFieldRef::Real(f) = f
        {
            out = Some(f.get(Q4, NG).unwrap().as_slice().to_vec());
        }
    });
    out.unwrap_or_else(|| panic!("material {i} has no internal {name}"))
}

fn stress_yy(c: &CouplerSolidPhaseField) -> Real {
    c.solid
        .material(0)
        .unwrap()
        .base()
        .stress
        .get(Q4, NG)
        .unwrap()
        .as_slice()[3]
}

fn law_previous_damage(c: &CouplerSolidPhaseField) -> Real {
    c.phase
        .phase_field(0)
        .unwrap()
        .base()
        .damage
        .previous(Q4, NG)
        .unwrap()
        .as_slice()[0]
}

fn c22(c: &CouplerSolidPhaseField) -> Real {
    let e = pf_param(c, "E");
    let nu = pf_param(c, "nu");
    e * (1. - nu) / ((1. + nu) * (1. - 2. * nu))
}

/// Exact damage of the penalized AT2 point problem with a uniform field:
/// `−2φ + d(2φ + g_c/l0) + γ min(0, d − d_prev) = 0`.
fn penalized_damage(phi: Real, gc: Real, l0: Real, gamma: Real, d_prev: Real) -> Real {
    let free = 2. * phi / (2. * phi + gc / l0);
    if free >= d_prev {
        free
    } else {
        (2. * phi + gamma * d_prev) / (2. * phi + gc / l0 + gamma)
    }
}

/// test_phase_solid_coupling.cc (TEST-091, solid and phase-field halves):
/// static solid, static AT2 without split, `ε_yy = 10⁻⁴ s`, 1000 steps;
/// damage `ε² c22 / (g_c/l0 + ε² c22)` and stress `c22 ε (1 − d)²` to
/// `1e-8` relative (upstream tolerances), plus the dissipated energy
/// `d² g_c / (2 l0)` (computed but not checked upstream) to `1e-8`.
#[test]
fn test_phase_solid_coupling() {
    one_element_coupling(AnalysisMethod::Static, Some("static"), true);
}

/// test_phase_solid_explicit.cc (TEST-096): explicit lumped solid (time
/// step `0.8 ×` stable), static phase field, the loading of TEST-091;
/// upstream fails only when **both** errors exceed `1e-8` (its `and`); the
/// port checks each error to `1e-8` (stronger, same tolerance).
#[test]
fn test_phase_solid_explicit() {
    one_element_coupling(
        AnalysisMethod::ExplicitLumpedMass,
        Some("explicit_lumped"),
        false,
    );
}

fn one_element_coupling(method: AnalysisMethod, solid_id: Option<&str>, check_energy: bool) {
    let mut c = coupler("test_one_element.msh", "material_coupling.dat", false);
    init(&mut c, method);
    if method == AnalysisMethod::ExplicitLumpedMass {
        let dt = c.solid.stable_time_step().unwrap() * 0.8;
        c.solid.set_time_step(dt, None).unwrap();
    }
    let c22 = c22(&c);
    let gc = pf_param(&c, "gc");
    let l0 = pf_param(&c, "l0");
    for s in 0..1000 {
        let eps = 1e-4 * Real::from(s);
        apply_displacement(&mut c, eps, -0.5, 0.5);
        c.solve(solid_id, Some("static")).unwrap();
        let d = material_array(&c, 0, "damage")[0];
        let sigma = stress_yy(&c);
        let d_an = eps * eps * c22 / (gc / l0 + eps * eps * c22);
        let s_an = c22 * eps * (1. - d_an) * (1. - d_an);
        if s == 0 {
            assert_eq!(d, 0.0);
            assert_eq!(sigma, 0.0);
            continue;
        }
        assert!(rel(d, d_an) <= 1e-8, "step {s}: damage {d} vs {d_an}");
        assert!(
            rel(sigma, s_an) <= 1e-8,
            "step {s}: stress {sigma} vs {s_an}"
        );
        if check_energy {
            let e_an = d_an * d_an * 0.5 * gc / l0;
            let e = c.phase.energy("dissipated").unwrap();
            assert!(rel(e, e_an) <= 1e-8, "step {s}: energy {e} vs {e_an}");
        }
    }
}

/// test_phase_field_deviatoric_split.cc (TEST-093): static solid (Newton
/// residual 1e-6) and AT2 with the volumetric–deviatoric split (Newton
/// solution 1e-6), loading / unloading into compression / reloading (1500
/// steps of 10⁻⁵), `savePreviousState` after each step (history);
/// damage `2(l0/g_c) φ⁺_max / (2(l0/g_c) φ⁺_max + 1)` and the split stress
/// to `1e-8` relative (upstream tolerances, `|ε| > 1e-13`).
///
/// Run twice (UD-387, D-066 (2); same pattern as
/// `akantu-phase-field` `test_phase_field_deviatoric_split_phase_field`):
/// with the single switch `in_block_deviator_energy` the driving energy is
/// upstream's (`5/9 μ ε²`, the value upstream's test encodes); with the
/// corrected default it is the potential of `σ⁺` (`2/3 μ ε²`). The stress
/// formulas are the same in both (`σ⁺`, `σ⁻` are not changed by UD-387).
#[test]
fn test_phase_field_deviatoric_split() {
    deviatoric_split_run(true, 5. / 9.);
    deviatoric_split_run(false, 2. / 3.);
}

fn deviatoric_split_run(in_block_deviator_energy: bool, dev_coefficient: Real) {
    let mut c = coupler("test_one_element.msh", "material_deviatoric.dat", false);
    if in_block_deviator_energy {
        c.phase
            .set_upstream_compat(akantu_phase_field::PhaseFieldUpstreamCompat {
                in_block_deviator_energy: true,
                ..Default::default()
            });
    }
    init(&mut c, AnalysisMethod::Static);
    solid_newton(&mut c, 1000, 1e-6, "residual");
    phase_newton(&mut c, 1000, 1e-6, "solution");
    let e = pf_param(&c, "E");
    let nu = pf_param(&c, "nu");
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. + 2. * nu);
    let kappa = lambda + 2. * mu / 3.;
    let gc = pf_param(&c, "gc");
    let l0 = pf_param(&c, "l0");
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
        apply_displacement(&mut c, eps, -0.5, 0.5);
        let plus = if eps > 0. {
            eps * eps * 0.5 * kappa + mu * eps * eps * dev_coefficient
        } else {
            dev_coefficient * eps * eps * mu
        };
        max_strain_energy = max_strain_energy.max(plus);
        c.solve(Some("static"), Some("static")).unwrap();
        c.phase.save_previous_state().unwrap();
        let d_an = 2. * (l0 / gc) * max_strain_energy / (2. * (l0 / gc) * max_strain_energy + 1.);
        let s_an = if eps < 0. {
            (1. - d_an) * (1. - d_an) * eps * 4. * mu / 3. + eps * (lambda + 2. * mu / 3.)
        } else {
            (lambda + 6. * mu / 3.) * eps * (1. - d_an) * (1. - d_an)
        };
        let d = material_array(&c, 0, "damage")[0];
        let sigma = stress_yy(&c);
        if eps.abs() > 1e-13 {
            assert!(
                rel(d, d_an) <= 1e-8,
                "compat {in_block_deviator_energy}: step {s} (ε = {eps}): damage {d} vs {d_an}"
            );
            assert!(
                rel(sigma, s_an) <= 1e-8,
                "compat {in_block_deviator_energy}: step {s} (ε = {eps}): stress {sigma} vs {s_an}"
            );
            checked += 1;
        }
    }
    assert!(checked > 1400);
}

/// test_phase_field_anisotropic.cc (TEST-092): reads
/// `material_penalization.dat` (AT2, no split, penalization, `l0 = 1`,
/// `tol_ir = 1e-4`; UD-383), static solid and phase field (Newton residual
/// 1e-6 both), load / unload / reload in steps of 10⁻⁴; upstream fails
/// only for relative errors above `1e3` (kept); the port adds the exact
/// penalized solution of the point problem (`1e-8` relative) and the
/// no-split stress `c22 ε (1 − d)²` with the computed damage (`1e-8`).
#[test]
fn test_phase_field_anisotropic() {
    let mut c = coupler("test_one_element.msh", "material_penalization.dat", false);
    init(&mut c, AnalysisMethod::Static);
    solid_newton(&mut c, 1000, 1e-6, "residual");
    phase_newton(&mut c, 1000, 1e-6, "residual");
    let e = pf_param(&c, "E");
    let nu = pf_param(&c, "nu");
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let mu = e / (2. + 2. * nu);
    let gc = pf_param(&c, "gc");
    let l0 = pf_param(&c, "l0");
    let tol = pf_param(&c, "irreversibility_tol");
    let gamma = gc / l0 * (1. / (tol * tol) - 1.);
    let c22 = c22(&c);
    let mut analytical_damage: Real = 0.0;
    for s in 0..1500u32 {
        let s_r = Real::from(s);
        let eps = if s < 500 {
            1e-4 * s_r
        } else if s < 1000 {
            (1500. - 2. * s_r) * 1e-4
        } else {
            (3. * s_r - 3500.) * 1e-4
        };
        apply_displacement(&mut c, eps, -0.5, 0.5);
        let d_prev = law_previous_damage(&c);
        c.solve(Some("static"), Some("static")).unwrap();
        c.phase.save_previous_state().unwrap();
        let plus = if eps > 0. {
            eps * eps * (0.5 * lambda + mu)
        } else {
            2. * eps * eps * mu / 3.
        };
        let new_damage = 2. * (l0 / gc) * plus / (2. * (l0 / gc) * plus + 1.);
        analytical_damage = analytical_damage.max(new_damage);
        let s_an = if eps < 0. {
            (1. - analytical_damage) * (1. - analytical_damage) * eps * 4. * mu / 3.
                + eps * (lambda + 2. * mu / 3.)
        } else {
            (lambda + 2. * mu) * eps * (1. - analytical_damage) * (1. - analytical_damage)
        };
        let d = material_array(&c, 0, "damage")[0];
        let sigma = stress_yy(&c);
        if eps.abs() > 1e-13 {
            assert!(
                rel(d, analytical_damage) <= 1e3,
                "step {s}: upstream damage check"
            );
            assert!(rel(sigma, s_an) <= 1e3, "step {s}: upstream stress check");
            let exact = penalized_damage(0.5 * c22 * eps * eps, gc, l0, gamma, d_prev);
            assert!(
                rel(d, exact) <= 1e-8,
                "step {s} (ε = {eps}): {d} vs exact {exact}"
            );
            let s_ex = c22 * eps * (1. - d) * (1. - d);
            assert!(
                rel(sigma, s_ex) <= 1e-8,
                "step {s} (ε = {eps}): {sigma} vs {s_ex}"
            );
        }
    }
}

/// test_penalization.cc (TEST-098): four elements (uniform `ε_yy` from the
/// solid: bottom blocked, top `u_y = ε`, interior nodes blocked in x),
/// static solid (Newton residual 1e-8) and AT2 with penalization (Newton
/// solution 1e-6), loading to `5·10⁻³` then unloading in steps of 10⁻⁵,
/// `savePreviousState` after each step; upstream compares damage and
/// stress with the irreversible analytical solution to `1e-3` (kept); the
/// port adds the exact penalized solution at every quadrature point of the
/// material (`1e-8` relative).
#[test]
fn test_penalization() {
    let mut c = coupler("test_four_elements.msh", "material_penalization.dat", false);
    init(&mut c, AnalysisMethod::Static);
    solid_newton(&mut c, 1000, 1e-8, "residual");
    phase_newton(&mut c, 1000, 1e-6, "solution");
    let c22 = c22(&c);
    let gc = pf_param(&c, "gc");
    let l0 = pf_param(&c, "l0");
    let tol = pf_param(&c, "irreversibility_tol");
    let gamma = gc / l0 * (1. / (tol * tol) - 1.);
    let mut eps: Real = 0.0;
    let mut analytical_damage: Real = 0.0;
    for s in 0..1000 {
        if s < 500 {
            eps += 1e-5;
        } else {
            eps -= 1e-5;
        }
        apply_displacement(&mut c, eps, -0.5, 0.5);
        let d_prev = law_previous_damage(&c);
        c.solve(Some("static"), Some("static")).unwrap();
        c.phase.save_previous_state().unwrap();
        let new_damage = eps * eps * c22 / (gc / l0 + eps * eps * c22);
        analytical_damage = analytical_damage.max(new_damage);
        let s_an = c22 * eps * (1. - analytical_damage) * (1. - analytical_damage);
        let d0 = material_array(&c, 0, "damage")[0];
        let sigma = stress_yy(&c);
        if eps.abs() > 1e-13 {
            assert!(
                rel(d0, analytical_damage) <= 1e-3,
                "step {s}: upstream damage check"
            );
            assert!(rel(sigma, s_an) <= 1e-3, "step {s}: upstream stress check");
            let exact = penalized_damage(0.5 * c22 * eps * eps, gc, l0, gamma, d_prev);
            for d in material_array(&c, 0, "damage") {
                assert!(
                    rel(d, exact) <= 1e-8,
                    "step {s} (ε = {eps}): {d} vs exact {exact}"
                );
            }
        }
    }
}

/// test_phase_field_linear.cc (TEST-099): static solid (Newton residual
/// 1e-9, 100 iterations), AT1 (`linear`, history), `ε_yy = 10⁻⁴ s`;
/// damage `max(0, 1 − 3g_c/(8 l0 ε² c22))` (absolute error) and stress
/// `c22 ε (1 − d)²` (relative error) with upstream's tolerances (`1e-5`
/// while the analytical damage is below `1e-8`, else `1e-8`); plus the
/// dissipated energy `3 g_c d / (8 l0)` (computed, not checked upstream)
/// to `1e-8` relative once `d > 0`.
#[test]
fn test_phase_field_linear() {
    let mut c = coupler("test_one_element.msh", "material_linear.dat", false);
    init(&mut c, AnalysisMethod::Static);
    solid_newton(&mut c, 100, 1e-9, "residual");
    let c22 = c22(&c);
    let gc = pf_param(&c, "gc");
    let l0 = pf_param(&c, "l0");
    for s in 1..1000 {
        let eps = 1e-4 * Real::from(s);
        apply_displacement(&mut c, eps, -0.5, 0.5);
        c.solve(Some("static"), None).unwrap();
        let d_an = (1. - 3. * gc / (8. * l0 * eps * eps * c22)).max(0.);
        let s_an = c22 * eps * (1. - d_an) * (1. - d_an);
        let tol = if d_an < 1e-8 { 1e-5 } else { 1e-8 };
        let d = material_array(&c, 0, "damage")[0];
        let sigma = stress_yy(&c);
        assert!((d - d_an).abs() <= tol, "step {s}: damage {d} vs {d_an}");
        assert!(
            rel(sigma, s_an) <= tol,
            "step {s}: stress {sigma} vs {s_an}"
        );
        if d_an > 0. {
            let e_an = d_an * 3. * gc / (8. * l0);
            let e = c.phase.energy("dissipated").unwrap();
            assert!(rel(e, e_an) <= 1e-8, "step {s}: energy {e} vs {e_an}");
        }
    }
}

/// test_multi_material.cc (TEST-097, UNSTABLE upstream, no assertion
/// upstream): two elements with the materials / phase fields `plate`
/// (element 0) and `rigid` (element 1), explicit lumped solid (`0.8 ×`
/// stable step), static phase field, 1000 steps of `ε = 10⁻⁴ s`
/// (`coupler.solve()` with the default solvers). The port runs three modes
/// and checks what upstream leaves implicit:
///
/// * corrected (default): each material's damage is the nodal damage
///   interpolated at **its own** element's quadrature points (and so is
///   each phase field's own `damage` internal);
/// * coupler compat only (UD-382; phase field corrected): `rigid`
///   (element 1) reads the first entries of the unfiltered array, i.e.
///   element 0's damage, and the phase field's `damage` internal is the
///   mesh-indexed array;
/// * full upstream compat (UD-382 and the phase field's UD-377): the
///   phase-field Newton–Raphson stalls at step 1 (its residual reads the
///   other element's damage, so `K` is not its Jacobian) — the default
///   solver (10 iterations, 1e-10) reports non-convergence, which is
///   upstream's `test_multi_material` failure (UNSTABLE).
#[test]
fn test_multi_material() {
    #[derive(Clone, Copy, PartialEq, Debug)]
    enum Mode {
        Corrected,
        CouplerCompat,
        Upstream,
    }
    for mode in [Mode::Corrected, Mode::CouplerCompat, Mode::Upstream] {
        let mut c = coupler("test_two_element.msh", "material_multiple.dat", true);
        match mode {
            Mode::Corrected => {}
            Mode::CouplerCompat => c.compat = CouplerSolidPhaseFieldCompat::upstream(),
            Mode::Upstream => c.set_upstream_compat(true),
        }
        init(&mut c, AnalysisMethod::ExplicitLumpedMass);
        let dt = c.solid.stable_time_step().unwrap() * 0.8;
        c.solid.set_time_step(dt, None).unwrap();
        assert_eq!(c.solid.material(0).unwrap().name(), "plate");
        assert_eq!(c.solid.material(1).unwrap().name(), "rigid");
        let mut differs = 0;
        for s in 0..1000 {
            let eps = 1e-4 * Real::from(s);
            apply_displacement(&mut c, eps, -1., 1.);
            let r = c.solve(None, None);
            if mode == Mode::Upstream && s == 1 {
                let e = r.expect_err("upstream compat: the phase-field solve must stall");
                assert!(
                    matches!(e, akantu_core::Error::NotConverged { niter: 10, .. }),
                    "{e}"
                );
                break;
            }
            r.unwrap_or_else(|e| panic!("{mode:?}, step {s}: {e}"));
            // nodal damage interpolated on each element (4 points each)
            let mesh = c.phase.mesh_handle().read().unwrap();
            let fe = c.phase.fe_engine().unwrap();
            let mut all = akantu_core::Array::<Real>::new(1);
            fe.interpolate_on_integration_points(
                &mesh,
                c.phase.damage().unwrap().view(),
                &mut all,
                1,
                Q4,
                NG,
                None,
            )
            .unwrap();
            drop(mesh);
            let all = all.as_slice().to_vec();
            let plate = material_array(&c, 0, "damage");
            let rigid = material_array(&c, 1, "damage");
            let pd = c
                .phase
                .phase_field(1)
                .unwrap()
                .base()
                .damage
                .get(Q4, NG)
                .unwrap();
            let pd = pd.as_slice().to_vec();
            assert_bits("multi_material", "plate damage", &plate, &all[0..4]);
            if mode == Mode::Corrected {
                assert_bits("multi_material", "rigid damage", &rigid, &all[4..8]);
                assert_bits(
                    "multi_material",
                    "rigid phase-field damage",
                    &pd,
                    &all[4..8],
                );
                if all[0..4] != all[4..8] {
                    differs += 1;
                }
            } else {
                assert_bits(
                    "multi_material",
                    "rigid damage (UD-382)",
                    &rigid,
                    &all[0..4],
                );
                assert_bits(
                    "multi_material",
                    "rigid phase-field damage (UD-382)",
                    &pd,
                    &all,
                );
            }
            // (no bound check: the Q4 discretisation of AT2 has no discrete
            // maximum principle, values slightly below 0 occur)
            for d in plate.iter().chain(&rigid) {
                assert!(d.is_finite() && *d < 1.0, "{mode:?}, step {s}: {d}");
            }
        }
        if mode == Mode::Corrected {
            assert!(
                differs > 900,
                "the two elements' damages should differ ({differs})"
            );
        }
    }
}

/// CPL-007: unequal numbers of materials and phase fields are refused by
/// default (upstream debug assertion) and accepted in compat mode (release
/// behaviour, UD-619).
#[test]
fn damage_transfer_law_count_check() {
    let mut c = coupler("test_one_element.msh", "material_coupling.dat", false);
    init(&mut c, AnalysisMethod::Static);
    c.phase
        .register_new_phase_field("extra", "quadratic", "")
        .unwrap();
    let e = c
        .compute_damage_on_quad_points(GhostType::NotGhost)
        .unwrap_err();
    assert!(e.to_string().contains("should be equal"), "{e}");
    c.compat = CouplerSolidPhaseFieldCompat {
        skip_law_count_check: true,
        ..CouplerSolidPhaseFieldCompat::default()
    };
    c.compute_damage_on_quad_points(GhostType::NotGhost)
        .unwrap();
}

/// CPL-006: the strain written into the phase field is
/// `(∇u + ∇uᵀ)/2 − ε_th I` of the solid's quadrature points, through each
/// law's filter.
#[test]
fn strain_transfer_is_the_symmetric_gradient() {
    let mut c = coupler("test_four_elements.msh", "material_coupling.dat", false);
    init(&mut c, AnalysisMethod::Static);
    // a non-uniform displacement field: u = (x² y, x − y³)
    let nodes = c.solid.physics.base.mesh.read().unwrap().nodes().clone();
    {
        let mut u = c.solid.displacement_mut().unwrap();
        for (uv, x) in u
            .as_mut_slice()
            .as_chunks_mut::<2>()
            .0
            .iter_mut()
            .zip(nodes.as_slice().as_chunks::<2>().0)
        {
            uv[0] = x[0] * x[0] * x[1];
            uv[1] = x[0] - x[1] * x[1] * x[1];
        }
    }
    c.solid.assemble_internal_forces().unwrap();
    c.compute_strain_on_quad_points(GhostType::NotGhost)
        .unwrap();
    let gu = material_array(&c, 0, "grad_u");
    let strain = c
        .phase
        .phase_field(0)
        .unwrap()
        .base()
        .strain
        .get(Q4, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_eq!(gu.len(), strain.len());
    assert_eq!(gu.len(), 4 * 4 * 4);
    for (g, s) in gu.as_chunks::<4>().0.iter().zip(strain.as_chunks::<4>().0) {
        assert_eq!(s[0], g[0]);
        assert_eq!(s[3], g[3]);
        assert_eq!(s[1], (g[1] + g[2]) / 2.);
        assert_eq!(s[2], s[1]);
        assert!(g[1] != g[2], "test field must have a skew part");
    }
}

/// An AT1 penalisation step at **zero
/// strain** through the coupler, with the corrected defaults (UD-388's
/// tangent `γ·[d ≤ d_prev]`; upstream cannot run it: its `[d < d_prev]`
/// leaves `K` singular at the first iterate). Upstream's four-element mesh
/// and `applyDisplacement` (x blocked everywhere), so the strain is exactly
/// `ε = diag(0, ε_yy)` whatever the damage; AT1 (`linear`) with
/// penalisation (`irreversibility_tol = recovery_tol = 2e-2`, `l0 = 0.05`),
/// no damage boundary condition. Step 1 (`ε_yy = 2e-2`, `d > d_prev = 0`,
/// both penalties inactive): `d = 1 − 3 g_c/(16 l0 φ)`, `φ = c22 ε²/2`.
/// Step 2 (`ε = 0`, `φ = 0`): the exact discrete solution is uniform, `d =
/// d_prev − 3 g_c/(8 l0 γ) = d_prev − (8/9)·tol²`, reached in at most three
/// Newton iterations: the model-level closed forms of
/// `akantu-phase-field/tests/review_v67_regressions.rs::
/// pfm02_at1_penalization_full_unload_converges_to_exact_solution`, here
/// with the strain transferred by the coupler. Bounds: `1e-12` absolute,
/// as the model-level test (the closed forms are evaluated in the same
/// arithmetic, the Newton tolerance is `1e-13` on the solution).
#[test]
fn at1_penalization_zero_strain_step_through_the_coupler() {
    let dat = "model solid_mechanics_model [\n material phasefield [\n  name = plate\n  rho = 1.\n  \
               E = 210.0\n  nu = 0.3\n  Plane_Stress = false\n ]\n]\n\
               model phase_field_model [\n phasefield linear [\n  name = plate\n  E = 210.0\n  \
               nu = 0.3\n  gc = 5e-3\n  l0 = 0.05\n  Plane_Stress = false\n  \
               irreversibility_type = penalization\n  irreversibility_tol = 2e-2\n  \
               recovery_tol = 2e-2\n ]\n]\n";
    let mesh = read_mesh(&pf_data("test_four_elements.msh"), 2);
    let parser = Parser::parse_str(dat, "at1_penalization.dat").unwrap();
    let mut c = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        parser,
    )
    .unwrap();
    c.phase
        .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));
    init(&mut c, AnalysisMethod::Static);
    solid_newton(&mut c, 100, 1e-9, "residual");
    phase_newton(&mut c, 100, 1e-13, "solution");
    let (gc, l0, tol) = (
        pf_param(&c, "gc"),
        pf_param(&c, "l0"),
        pf_param(&c, "irreversibility_tol"),
    );
    let eps = 2e-2;
    let phi = 0.5 * c22(&c) * eps * eps;
    let d1_exact = 1.0 - 3.0 * gc / (16.0 * l0 * phi);
    assert!(d1_exact > 0.5 && d1_exact < 1.0, "{d1_exact}");
    // step 1: tension
    apply_displacement(&mut c, eps, -0.5, 0.5);
    c.solve(Some("static"), Some("static")).unwrap();
    let d1 = c.phase.damage().unwrap().as_slice().to_vec();
    for (n, &d) in d1.iter().enumerate() {
        assert!(
            (d - d1_exact).abs() <= 1e-12,
            "step 1, node {n}: {d} vs {d1_exact}"
        );
    }
    c.phase.save_previous_state().unwrap();
    // step 2: zero strain
    apply_displacement(&mut c, 0.0, -0.5, 0.5);
    c.solve(Some("static"), Some("static"))
        .unwrap_or_else(|e| panic!("zero-strain AT1 penalisation step: {e}"));
    let it = c
        .phase
        .solver
        .non_linear_solver(None)
        .unwrap()
        .get_param("nb_iterations")
        .unwrap()
        .as_int()
        .unwrap();
    assert!(it <= 3, "{it} Newton iterations at zero strain");
    let mut worst: Real = 0.0;
    for (n, (&d, &dp)) in c
        .phase
        .damage()
        .unwrap()
        .as_slice()
        .iter()
        .zip(&d1)
        .enumerate()
    {
        let want = dp - 8.0 / 9.0 * tol * tol;
        worst = worst.max((d - want).abs());
        assert!(
            (d - want).abs() <= 1e-12,
            "step 2, node {n}: {d} vs d_prev − 8/9 tol² = {want}"
        );
    }
    eprintln!(
        "AT1 penalisation through the coupler: step 1 d = {d1_exact}; zero strain: {it} Newton \
         iterations, max |d − (d_prev − 8/9 tol²)| = {worst:e}"
    );
}
