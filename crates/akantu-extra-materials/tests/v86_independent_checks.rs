// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS). Reproducers and
// independent checks of crates/akantu-extra-materials,
// written from the documented formulas without reading the port (no oracle).
//
// Run with
//   cargo test -p akantu-extra-materials --test v86_independent_checks
// Tests named `xm0N_*` reproduce defects and failed before the fix by
// design (they assert the intended / upstream behaviour); `v86_*` tests are
// independent checks that pass.
//
// Kept as written with two
// adaptations, both marked `adapted` below: `Fe2Config::new` (the
// struct gained the `extra_options` field), and the derivation of `Δε_p[1]`
// in `xm03_*`, which read it back from the port's output assuming the
// per-point composition (`cur[1] − prev[1]`, valid only before the fix);
// the fixed code is checked against the documented flow rule instead
// (the closed-form version is `tests/point_laws.rs::
// viscoplastic_compat_composes_with_the_first_point_previous_inelastic_strain`).

mod common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_core::tensor::Matrix;
use akantu_core::{ElementType, GhostType, Real};
use akantu_extra_materials::brittle::MaterialBrittle;
use akantu_extra_materials::damage_linear::MaterialDamageLinear;
use akantu_extra_materials::orthotropic_damage::{
    MaterialOrthotropicDamageIterative, damaged_stress, one_minus_d,
};
use akantu_extra_materials::viscoplastic::MaterialViscoPlastic;
use akantu_extra_materials::vreepeerlings::MaterialVreePeerlings;
use akantu_extra_materials::{
    ExtraLawOptions, MaterialDamageIterative, MaterialIterativeStiffnessReduction,
    SolidMechanicsModelRve,
};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::FixedValue;
use akantu_model::constitutive_law::{ElementDataSelector, LawSpec};
use akantu_parser::Parser;
use akantu_solid::SolidMechanicsModel;
use common::{data, init_static, model_with, read_mesh, with_iterative, with_law};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

fn spec() -> LawSpec {
    LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "m".into(),
    }
}

fn explicit_model(dat: &str) -> SolidMechanicsModel {
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with(mesh, dat);
    model
        .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
        .unwrap();
    model
}

/// `u = H·X` (`h` row-major), plus `extra` on node `n`.
fn set_displacement(
    model: &mut SolidMechanicsModel,
    h: [[Real; 2]; 2],
    extra: Option<(usize, [Real; 2])>,
) {
    let x: Vec<[Real; 2]> = {
        let mesh = model.physics.base.mesh.read().unwrap();
        mesh.nodes()
            .as_slice()
            .as_chunks::<2>()
            .0
            .iter()
            .map(|c| [c[0], c[1]])
            .collect()
    };
    let mut u = model.displacement_mut().unwrap();
    for (n, p) in x.iter().enumerate() {
        let mut v = [
            h[0][0] * p[0] + h[0][1] * p[1],
            h[1][0] * p[0] + h[1][1] * p[1],
        ];
        if let Some((m, d)) = extra
            && m == n
        {
            v[0] += d[0];
            v[1] += d[1];
        }
        u.as_mut_slice()[2 * n..2 * n + 2].copy_from_slice(&v);
    }
}

fn law<T: 'static>(model: &SolidMechanicsModel) -> &T {
    model
        .material(0)
        .unwrap()
        .as_any()
        .unwrap()
        .downcast_ref::<T>()
        .unwrap()
}

// ------------------------------------------------------------------

/// `damage_linear` was designed for a linear softening branch from
/// `(ε_min, Sigc)` to `(ε_max, 0)`: `ε_max = 2Gc/Sigc + ε_min` makes its
/// area equal to `Gc`. Hence: peak stress `Sigc` and work to failure
/// `Sigc·ε_max/2` in uniaxial strain with `ν = 0`. FAILED before the fix
/// (upstream formula `d = (ε̂ − ε_min)/(ε_max − ε_min)·(ε̂/ε_max)`: peak
/// 2.10·Sigc, work 2.71× for `ε_max/ε_min = 5`).
#[test]
fn xm01_damage_linear_is_linear_softening() {
    let (e, sigc, gc) = (1e9, 1e5, 20.);
    let dat = format!(
        "material damage_linear [\n name = m\n rho = 1\n E = {e:e}\n nu = 0\n Sigc = {sigc:e}\n Gc = {gc}\n]\n"
    );
    let mut model = explicit_model(&dat);
    let eps_min = sigc / e;
    let eps_max = 2. * gc / sigc + eps_min;
    let n = 4000;
    let (mut work, mut peak, mut prev) = (0., 0_f64, 0.);
    for i in 1..=n {
        let eps = eps_max * Real::from(i) / Real::from(n);
        set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
        model.assemble_internal_forces().unwrap();
        let s = model
            .material(0)
            .unwrap()
            .base()
            .stress
            .get(T3, NG)
            .unwrap()
            .as_slice()[0];
        work += 0.5 * (s + prev) * (eps_max / Real::from(n));
        prev = s;
        peak = peak.max(s);
    }
    let w_lin = sigc * eps_max / 2.;
    println!(
        "peak/Sigc = {:.4}, W/W_linear = {:.4}",
        peak / sigc,
        work / w_lin
    );
    assert!(
        (peak / sigc - 1.).abs() < 2e-3,
        "peak/Sigc = {}",
        peak / sigc
    );
    assert!(
        (work / w_lin - 1.).abs() < 2e-3,
        "W/W_linear = {}",
        work / w_lin
    );
}

// ------------------------------------------------------------------

fn vree(extra: &str) -> String {
    format!(
        "material vreepeerlings [\n name = m\n rho = 1\n E = 1e9\n nu = 0.25\n Kct = 1\n {extra}\n]\n"
    )
}

/// Damage at rest at `ε̂ = 1.5e-4`, between `Kapaoi = 1e-4` and
/// `0.99·Kapac = 1.98e-4` (`k = 1`: `ε̂ = ε/(1 + ν)`).
fn vree_damage_at_rest(dat: &str) -> Real {
    let mut model = explicit_model(dat);
    let eps = 1.5e-4 * 1.25;
    set_displacement(&mut model, [[eps, 0.], [0., 0.]], None);
    model.assemble_internal_forces().unwrap();
    let m: &MaterialVreePeerlings<2> = law(&model);
    m.damage.damage.get(T3, NG).unwrap().as_slice()[0]
}

/// With the documented default `Crate = 0.0001` (header since 2013,
/// "Kapaoi ≥ Crate") the threshold at rest is `Kapaoi` and
/// `d = 1 − κ_oi/ε̂·(κ_c − ε̂)/(κ_c − κ_oi) = 2/3`. FAILED before the fix with the
/// port's default `Crate = 1` (`κ_dyn = 0.99 κ_c`, no damage).
#[test]
fn xm02_vreepeerlings_default_crate_keeps_kapaoi_active() {
    let want = 1. - 1e-4 / 1.5e-4 * ((2e-4 - 1.5e-4) / (2e-4 - 1e-4));
    let explicit = vree_damage_at_rest(&vree("Crate = 1e-4"));
    assert!(
        (explicit - want).abs() < 1e-9,
        "Crate = 1e-4: d = {explicit}"
    );
    let default = vree_damage_at_rest(&vree(""));
    println!("d (Crate = 1e-4) = {explicit}, d (port default) = {default}");
    assert!(
        (default - want).abs() < 1e-9,
        "default Crate: d = {default}"
    );
}

// ------------------------------------------------------------------

/// Upstream `material_viscoplastic.cc` never advances
/// `previous_inelastic_strain_it`: every point gets `ε_p,prev[0] + Δε_p[q]`.
/// `ExtraLawOptions::upstream()` is documented to reproduce UD-583. FAILED before
/// the fix (the compat mode reads `ε_p,prev[q]`).
#[test]
fn xm03_viscoplastic_compat_reads_first_point_previous_inelastic_strain() {
    let dat = "material visco_plastic [\n name = m\n rho = 1\n E = 1e9\n nu = 0.2\n sigma_y = 1e6\n h = 1e8\n rate = 2\n edot0 = 0.1\n ts = 0.5\n]\n";
    let mut model = explicit_model(dat);
    with_law(&mut model, 0, |l, _| {
        l.as_any_mut()
            .unwrap()
            .downcast_mut::<MaterialViscoPlastic<2>>()
            .unwrap()
            .extra_options = ExtraLawOptions::upstream();
        Ok(())
    });
    // non-affine field: the two elements get different stresses and flows
    let (mut committed_sigma, mut committed_iso) = (Vec::new(), Vec::new());
    for k in 1..=3 {
        let s = Real::from(k) * 1e-3;
        set_displacement(&mut model, [[s, 0.], [0., 0.]], Some((2, [0.4 * s, 0.])));
        model.assemble_internal_forces().unwrap();
        if k < 3 {
            with_law(&mut model, 0, |l, _| l.save_previous_state());
            // Adapted: the state the next evaluation reads
            let m: &MaterialViscoPlastic<2> = law(&model);
            committed_sigma = akantu_solid::Material::base(&m.elastic)
                .stress
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .to_vec();
            committed_iso = m
                .plastic
                .iso_hardening
                .get(T3, NG)
                .unwrap()
                .as_slice()
                .to_vec();
        }
    }
    // Adapted: Δε_p[1] from the documented flow rule (m = 2) applied to
    // point 1's current stress and hardening before the last evaluation
    // (= the state committed at step 2), instead of `cur[1] − prev[1]`
    // (which presupposes the per-point composition this test refutes).
    let m: &MaterialViscoPlastic<2> = law(&model);
    let prev = m
        .plastic
        .inelastic_strain
        .previous(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    let cur = m
        .plastic
        .inelastic_strain
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    assert_ne!(
        prev[0..4],
        prev[4..8],
        "need different committed inelastic strains"
    );
    let sig1 = &committed_sigma[4..8];
    let norm = sig1.iter().map(|x| x * x).sum::<Real>().sqrt();
    let f = 0.5 * 0.1 * norm / ((1e6 + committed_iso[1]) * (1e6 + committed_iso[1]));
    let delta1: Vec<Real> = (0..4).map(|i| f * sig1[i]).collect();
    let upstream1: Vec<Real> = (0..4).map(|i| prev[i] + delta1[i]).collect();
    println!(
        "port ε_p[1] = {:?}, upstream ε_p[1] = {upstream1:?}",
        &cur[4..8]
    );
    for i in 0..4 {
        assert!(
            (cur[4 + i] - upstream1[i]).abs() <= 1e-15 * upstream1[i].abs().max(1e-30),
            "component {i}: port {} vs upstream {}",
            cur[4 + i],
            upstream1[i]
        );
    }
}

// ------------------------------------------------------------------

/// Upstream `advanceASR` casts materials 0/1 with
/// `dynamic_cast<MaterialDamageIterative<2>&>`, which accepts the derived
/// `iterative_stiffness_reduction`. FAILED before the fix (exact-type check).
#[test]
fn xm05_rve_advance_asr_accepts_derived_iterative_laws() {
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(1);
    let dat = std::fs::read_to_string(data("test_material_FE2/mesoscale_materials.dat"))
        .unwrap()
        .replacen(
            "material damage_iterative [\n\t name = paste",
            "material iterative_stiffness_reduction [\n\t name = paste\n\t Gf = 100\n\t crack_band_width = 0.01",
            1,
        );
    assert!(dat.contains("iterative_stiffness_reduction"));
    let parser = Parser::parse_str(&dat, "mesoscale.dat").unwrap();
    let mesh = read_mesh(&data("test_material_FE2/one_inclusion.msh"), 2);
    let mut rve = SolidMechanicsModelRve::new(mesh, false, 400, parser, "rve").unwrap();
    rve.model
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));
    rve.init_full().unwrap();
    rve.apply_boundary_conditions(&[[0.; 2]; 2]).unwrap();
    let r = rve.advance_asr(&[[0.02, 0.], [0., 0.02]]);
    assert!(r.is_ok(), "{r:?}");
}

// ------------------------------------------------------ independent checks

/// Rots–Invernizzi saw-tooth: after each reduction `k` the point
/// `(Sc_k/E_k, Sc_k)` lies on the linear softening line `σ = D(ε_u − ε)`
/// (the envelope whose area is `G_f/h`), with `E_k = (1 − d_k)E`.
#[test]
fn v86_stiffness_reduction_peaks_on_the_softening_line() {
    let dat = std::fs::read_to_string(data(
        "test_material_damage/material_stiffness_reduction.dat",
    ))
    .unwrap();
    let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
    let mut model = model_with(mesh, &dat);
    init_static(&mut model);
    for (value, axis, group) in [
        (0., akantu_core::enums::SpatialDirection::X, "left"),
        (0., akantu_core::enums::SpatialDirection::Y, "bottom"),
        (2., akantu_core::enums::SpatialDirection::Y, "top"),
    ] {
        model
            .apply_dirichlet_bc(&FixedValue { value, axis }, group)
            .unwrap();
    }
    let e = 60e9;
    let mut checked = 0;
    loop {
        model.solve_step(None).unwrap();
        let nb = with_iterative(&mut model, 0, |m, ctx| m.update_damage(ctx));
        let l: &MaterialIterativeStiffnessReduction<2> = law(&model);
        let dam = l
            .parent
            .damage
            .damage
            .get(T3, NG)
            .unwrap()
            .as_slice()
            .to_vec();
        let sc = l.parent.sc.get(T3, NG).unwrap().as_slice().to_vec();
        let eu = l.eps_u.get(T3, NG).unwrap().as_slice().to_vec();
        let dd = l.d.get(T3, NG).unwrap().as_slice().to_vec();
        for q in 0..dam.len() {
            if dam[q] == 0. || dam[q] >= l.parent.max_damage {
                continue;
            }
            let ek = (1. - dam[q]) * e;
            let line = dd[q] * (eu[q] - sc[q] / ek);
            let rel = (line - sc[q]).abs() / sc[q];
            assert!(
                rel < 1e-9,
                "q {q}: d = {}, Sc = {}, line {line}, rel {rel:e}",
                dam[q],
                sc[q]
            );
            checked += 1;
        }
        if nb == 0 {
            break;
        }
    }
    assert_eq!(checked, 18);
}

/// For an isotropic elastic `C`, the asymmetric parts of `(S⊗S):C` and of
/// `O⊗(C:O)/tr O` cancel (`λ O⊗I` each), so the damaged 2-D tangent is
/// symmetric for any damage frame.
#[test]
fn v86_orthotropic_2d_tangent_symmetric() {
    let mut m = MaterialOrthotropicDamageIterative::<2>::new(&spec(), None).unwrap();
    m.elastic.thermal.e = 30e9;
    m.elastic.thermal.nu = 0.2;
    akantu_parser::ParameterRegistry::update_internal_parameters(&mut m).unwrap();
    for (d, theta) in [([0.3, 0.1], 0.0), ([0.45, 0.05], 0.7), ([0.2, 0.6], -1.1)] {
        let (s, c) = Real::sin_cos(theta);
        let v: Matrix<2, 2> = [[c, s], [-s, c]];
        let dm: Matrix<2, 2> = [[d[0], 0.], [0., d[1]]];
        let mut t = [0.; 9];
        m.elastic.tangent_on_quad(&mut t);
        m.damaged_tangent_on_quad(&mut t, &dm, &v);
        for i in 0..3 {
            for j in 0..i {
                let (a, b) = (t[i + 3 * j], t[j + 3 * i]);
                assert!((a - b).abs() <= 1e-12 * t[0].abs(), "({i},{j}) {a} vs {b}");
            }
        }
    }
}

/// UD-582 (documented): the 3-D tangent is the undamaged elastic tangent;
/// quantify its distance to the exact tangent of the stress map.
#[test]
fn v86_orthotropic_3d_tangent_is_undamaged() {
    let spec3 = LawSpec {
        spatial_dimension: 3,
        option: String::new(),
        id: "m".into(),
    };
    let mut m = MaterialOrthotropicDamageIterative::<3>::new(&spec3, None).unwrap();
    m.elastic.thermal.e = 10.;
    m.elastic.thermal.nu = 0.2;
    akantu_parser::ParameterRegistry::update_internal_parameters(&mut m).unwrap();
    let mut c = [0.; 36];
    m.elastic.tangent_on_quad(&mut c);
    let mut t = c;
    let dm: Matrix<3, 3> = [[0.4, 0., 0.], [0., 0.1, 0.], [0., 0., 0.2]];
    let v: Matrix<3, 3> = [[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]];
    m.damaged_tangent_on_quad(&mut t, &dm, &v);
    assert_eq!(t, c, "3-D tangent unchanged (UD-582)");
    // ∂σ11/∂ε11 of the stress map vs C11
    let (o, s) = one_minus_d(&dm, &v, DEFAULT_TOLERANCE);
    let (lambda, mu) = (m.elastic.lambda(), m.elastic.mu());
    // Adapted: the original code kept verbatim (diagonal fill)
    #[allow(clippy::needless_range_loop)]
    let sig = |e11: Real| {
        let mut st = [[0.; 3]; 3];
        for i in 0..3 {
            st[i][i] = lambda * e11;
        }
        st[0][0] += 2. * mu * e11;
        damaged_stress(&st, &o, &s, &dm, m.eta, m.max_damage)[0][0]
    };
    let exact = (sig(1e-3) - sig(-1e-3)) / 2e-3;
    println!("3-D ∂σ11/∂ε11 = {exact}, tangent C11 = {}", c[0]);
    assert!(exact < 0.8 * c[0]);
}

/// `damage_iterative`: σ_eq = max eig((1 − d) C:∇u)/Sc for pure shear
/// (eigenvalues ±2μγ) and a rotated uniaxial state.
#[test]
fn v86_damage_iterative_equivalent_stress_closed_form() {
    let mut m = MaterialDamageIterative::<2>::new(&spec(), None).unwrap();
    m.elastic.thermal.e = 12e9;
    m.elastic.thermal.nu = 0.3;
    akantu_parser::ParameterRegistry::update_internal_parameters(&mut m).unwrap();
    let (lambda, mu) = (m.elastic.lambda(), m.elastic.mu());
    let gamma = 2e-4;
    let shear: Matrix<2, 2> = [[0., gamma], [gamma, 0.]];
    let got = m.equivalent_stress_on_quad(&shear, 0.3, 5e6).unwrap();
    let want = 0.7 * 2. * mu * gamma / 5e6;
    assert!((got - want).abs() <= 1e-14 * want, "{got} vs {want}");
    // uniaxial strain ε along n = (cos t, sin t): eigenvalues (λ+2μ)ε, λε
    let (s, c) = Real::sin_cos(0.4);
    let eps = 1e-4;
    let g: Matrix<2, 2> = [[eps * c * c, eps * c * s], [eps * c * s, eps * s * s]];
    let got = m.equivalent_stress_on_quad(&g, 0.0, 1e6).unwrap();
    let want = (lambda + 2. * mu) * eps / 1e6;
    assert!((got - want).abs() <= 1e-13 * want, "{got} vs {want}");
}

/// UD-577 default: the cubic fracture stress is increasing above `E_0`
/// (derivative discriminant < 0) and continuous with `S_0` within 0.07 %.
#[test]
fn v86_brittle_cubic_increasing_and_continuous() {
    let m = MaterialBrittle::<2>::new(&spec(), None).unwrap();
    let mut prev = m.fracture_stress(27e3 + 1.);
    assert!(((prev - 157e6) / 157e6).abs() < 7e-4);
    let mut r = 27e3 + 1.;
    while r < 1e6 {
        r *= 1.01;
        let f = m.fracture_stress(r);
        assert!(f > prev, "r = {r}: {f} <= {prev}");
        prev = f;
    }
}

/// `damage_linear` UD-575/576 sanity: the equivalent strain of a symmetric
/// strain is the norm of its positive principal part.
#[test]
fn v86_damage_linear_equivalent_strain() {
    let m = MaterialDamageLinear::<2>::new(&spec(), None).unwrap();
    let g: Matrix<2, 2> = [[3e-4, 1e-4], [1e-4, -2e-4]];
    // eigenvalues of [[3, 1], [1, −2]]·1e-4: (1 ± √29)/2·1e-4
    let lmax = (1. + 29f64.sqrt()) / 2. * 1e-4;
    let got = m.equivalent_strain(&g).unwrap();
    assert!((got - lmax).abs() <= 1e-15, "{got} vs {lmax}");
}

// ------------------------------------------------------------------

/// UD-584 compat: upstream `sigma(i, j) = sigma(j, i) = voigt_stress(I) +
/// (i == j) * sigma_th` multiplies `NaN` by `0` off the diagonal, so **every**
/// component is NaN (UD-584: "every macro stress is NaN"). FAILED before the fix
/// (`fe2.rs`: `acc + if i == j { th } else { 0. }` leaves the shear finite).
#[test]
fn xm03b_fe2_compat_every_component_nan() {
    use akantu_extra_materials::{Fe2Config, MaterialFE2, register_material_fe2};
    use akantu_mesh::SharedMesh;
    use akantu_model::constitutive_law::{LawPhase, SelectorChain};
    akantu_core::random::RandomGenerator::<akantu_core::random::IdxSlot>::seed(1);
    let p = Parser::parse_file(data("test_material_FE2/material.dat")).unwrap();
    let mesh = read_mesh(&data("test_material_FE2/square.msh"), 2);
    let mut model = SolidMechanicsModel::with_parser(
        SharedMesh::new(mesh),
        None,
        "solid_mechanics_model",
        p.clone(),
    )
    .unwrap();
    akantu_extra_materials::register_extra_materials(&mut model.physics.factory).unwrap();
    // Adapted: `Fe2Config::new` (new `extra_options` field)
    register_material_fe2(
        &mut model.physics.factory,
        Fe2Config::new(p, Some(data("test_material_FE2"))),
    )
    .unwrap();
    model.set_material_selector_chain(SelectorChain {
        selectors: Vec::new(),
        fallback_value: 3,
    });
    model
        .init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    let fe2 = model.material_index("FE2_mat").unwrap();
    model
        .with_material(fe2, LawPhase::Init, |law, _| {
            law.as_any_mut()
                .unwrap()
                .downcast_mut::<MaterialFE2<2>>()
                .unwrap()
                .extra_options = ExtraLawOptions::upstream();
            Ok(())
        })
        .unwrap();
    // a shear macro gradient
    let n = model.displacement_mut().unwrap().len();
    let x: Vec<Real> = model
        .physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec();
    {
        let mut u = model.displacement_mut().unwrap();
        for k in 0..n {
            u.as_mut_slice()[2 * k] = 1e-4 * x[2 * k + 1];
        }
    }
    let _ = model.assemble_internal_forces();
    let s = model
        .material(fe2)
        .unwrap()
        .base()
        .stress
        .get(T3, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    println!(
        "compat macro stress (first point, column-major): {:?}",
        &s[0..4]
    );
    assert!(s[0].is_nan() && s[3].is_nan(), "diagonal NaN");
    assert!(
        s[1].is_nan() && s[2].is_nan(),
        "shear: upstream NaN, port {} {}",
        s[1],
        s[2]
    );
}
