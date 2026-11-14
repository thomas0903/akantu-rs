// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): independent checks of extra_packages/
// extra-materials/src/material_damage/material_orthotropic_damage*.

//! Independent checks (D-007) of `orthotropic_damage_iterative`
//! (XMAT-009/010): undamaged and isotropic-damage limits of the
//! Ragueneau stress, objectivity, the damage direction after
//! `updateDamage` (UD-581), and the 2-D tangent against finite differences
//! of the stress at fixed damage.

mod common;

use akantu_core::Real;
use akantu_core::enums::AnalysisMethod;
use akantu_core::math::DEFAULT_TOLERANCE;
use akantu_core::tensor::{Matrix, MatrixExt};
use akantu_core::{ElementType, GhostType};
use akantu_extra_materials::ExtraLawOptions;
use akantu_extra_materials::orthotropic_damage::{
    MaterialOrthotropicDamageIterative, damaged_stress, one_minus_d, rotate_into_computation_frame,
};
use akantu_model::ModelOptions;
use common::{data, model_with, read_mesh, with_iterative, with_law};

const T3: ElementType = ElementType::Triangle3;
const NG: GhostType = GhostType::NotGhost;

fn rot(theta: Real) -> Matrix<2, 2> {
    let (s, c) = theta.sin_cos();
    // column-major: columns (c, s) and (−s, c)
    [[c, s], [-s, c]]
}

fn close(a: &Matrix<2, 2>, b: &Matrix<2, 2>, tol: Real) -> bool {
    let scale = a.norm().max(b.norm()).max(1e-300);
    a.sub(b).norm() <= tol * scale
}

#[test]
fn undamaged_point_returns_the_effective_stress() {
    let sigma: Matrix<2, 2> = [[3e6, 1e6], [1e6, -2e6]];
    let d = [[0.; 2]; 2];
    let (o, s) = one_minus_d(&d, &[[0.; 2]; 2], DEFAULT_TOLERANCE);
    let out = damaged_stress(&sigma, &o, &s, &d, 2., 0.99999);
    assert!(close(&out, &sigma, 1e-15), "{out:?}");
}

#[test]
fn isotropic_damage_scales_deviator_and_sphere() {
    // D = d·I: σ = (1 − d)·dev σ̃ + (1 − min(η d, max))·sph σ̃
    let sigma: Matrix<2, 2> = [[3e6, 1e6], [1e6, -2e6]];
    for (d, eta) in [(0.3, 1.), (0.25, 2.), (0.4, 3.)] {
        let dm = [[d, 0.], [0., d]];
        let (o, s) = one_minus_d(&dm, &rot(0.3), DEFAULT_TOLERANCE);
        let out = damaged_stress(&sigma, &o, &s, &dm, eta, 0.99999);
        let p = (sigma[0][0] + sigma[1][1]) / 2.;
        let k = 1. - f64::min(eta * d, 0.99999);
        let want: Matrix<2, 2> = [
            [(1. - d) * (sigma[0][0] - p) + k * p, (1. - d) * sigma[0][1]],
            [(1. - d) * sigma[1][0], (1. - d) * (sigma[1][1] - p) + k * p],
        ];
        assert!(
            close(&out, &want, 1e-14),
            "d = {d}, η = {eta}: {out:?} vs {want:?}"
        );
    }
}

#[test]
fn stress_is_objective() {
    // σ(R σ̃ Rᵀ, R-rotated damage frame) = R σ(σ̃, frame) Rᵀ
    let sigma: Matrix<2, 2> = [[3e6, 1e6], [1e6, -2e6]];
    let dm = [[0.35, 0.], [0., 0.1]];
    let v = rot(0.2);
    let r = rot(0.9);
    let (o, s) = one_minus_d(&dm, &v, DEFAULT_TOLERANCE);
    let base = damaged_stress(&sigma, &o, &s, &dm, 2., 0.99999);
    let sigma_r = rotate_into_computation_frame(&sigma, &r);
    let v_r = r.matmul(&v);
    let (o_r, s_r) = one_minus_d(&dm, &v_r, DEFAULT_TOLERANCE);
    let rotated = damaged_stress(&sigma_r, &o_r, &s_r, &dm, 2., 0.99999);
    assert!(
        close(&rotated, &rotate_into_computation_frame(&base, &r), 1e-14),
        "{rotated:?}"
    );
}

const DAT: &str = "material orthotropic_damage_iterative [\n name = m\n rho = 1\n E = 1\n nu = 0\n Sc = 0.5\n prescribed_dam = 0.1\n]\n";

/// Uniaxial tension along y (the principal stress direction is e_y): after
/// one `updateDamage` the damage tensor in the computation frame is
/// `0.1·e_y⊗e_y` (default) — upstream stores the stress-frame eigenvectors
/// and damages e_x (UD-581).
#[test]
fn update_damage_follows_the_principal_stress_direction() {
    for (opts, want_yy) in [
        (ExtraLawOptions::default(), true),
        (ExtraLawOptions::upstream(), false),
    ] {
        let mesh = read_mesh(&data("test_material_damage/two_elements.msh"), 2);
        let mut model = model_with(mesh, DAT);
        model
            .init_full(&ModelOptions::new(AnalysisMethod::ExplicitLumpedMass))
            .unwrap();
        with_law(&mut model, 0, |l, _| {
            l.as_any_mut()
                .unwrap()
                .downcast_mut::<MaterialOrthotropicDamageIterative<2>>()
                .unwrap()
                .extra_options = opts;
            Ok(())
        });
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
            for (n, p) in x.as_chunks::<2>().0.iter().enumerate() {
                u.as_mut_slice()[2 * n + 1] = 1.0 * p[1]; // ε_yy = 1 → σ_yy = 1
            }
        }
        model.assemble_internal_forces().unwrap();
        let nb = with_iterative(&mut model, 0, |m, ctx| {
            assert!(m.norm_max_equivalent_stress() >= 1.);
            m.update_damage(ctx)
        });
        assert_eq!(nb, 1);
        let m = model
            .material(0)
            .unwrap()
            .as_any()
            .unwrap()
            .downcast_ref::<MaterialOrthotropicDamageIterative<2>>()
            .unwrap();
        let (ty, q) = m.q_max().unwrap();
        assert_eq!(ty, T3);
        let d: Matrix<2, 2> = *akantu_core::tensor::as_matrix(
            &m.damage.get(T3, NG).unwrap().as_slice()[4 * q..4 * q + 4],
        )
        .unwrap();
        let v: Matrix<2, 2> = *akantu_core::tensor::as_matrix(
            &m.damage_dir_vecs.get(T3, NG).unwrap().as_slice()[4 * q..4 * q + 4],
        )
        .unwrap();
        let dc = rotate_into_computation_frame(&d, &v);
        let want: Matrix<2, 2> = if want_yy {
            [[0., 0.], [0., 0.1]]
        } else {
            [[0.1, 0.], [0., 0.]]
        };
        assert!(close(&dc, &want, 1e-12), "{dc:?}");
    }
}

/// The 2-D tangent of the reference vs central finite differences of the
/// stress map `ε ↦ σ(C:ε, D)` at fixed damage (linear: differences exact up
/// to rounding).
#[test]
fn tangent_2d_against_finite_differences() {
    let spec = akantu_model::constitutive_law::LawSpec {
        spatial_dimension: 2,
        option: String::new(),
        id: "m".into(),
    };
    let mut m = MaterialOrthotropicDamageIterative::<2>::new(&spec, None).unwrap();
    m.elastic.thermal.e = 10.;
    m.elastic.thermal.nu = 0.2;
    akantu_parser::ParameterRegistry::update_internal_parameters(&mut m).unwrap();
    let (lambda, mu) = (m.elastic.lambda(), m.elastic.mu());
    let c_eps = |e: [Real; 3]| -> Matrix<2, 2> {
        // Voigt strain (ε11, ε22, γ12)
        let tr = e[0] + e[1];
        [
            [lambda * tr + 2. * mu * e[0], mu * e[2]],
            [mu * e[2], lambda * tr + 2. * mu * e[1]],
        ]
    };
    let mut report = Vec::new();
    for (d, theta) in [([0.3, 0.3], 0.0), ([0.4, 0.1], 0.0), ([0.4, 0.1], 0.5)] {
        let dm: Matrix<2, 2> = [[d[0], 0.], [0., d[1]]];
        let v = rot(theta);
        let (o, s) = one_minus_d(&dm, &v, DEFAULT_TOLERANCE);
        let sigma = |e: [Real; 3]| damaged_stress(&c_eps(e), &o, &s, &dm, m.eta, m.max_damage);
        let mut t = [0.; 9];
        let mut c = [0.; 9];
        m.elastic.tangent_on_quad(&mut c);
        t.copy_from_slice(&c);
        m.damaged_tangent_on_quad(&mut t, &dm, &v);
        let mut max_rel: Real = 0.;
        for j in 0..3 {
            let h = 1e-3;
            let mut ep = [0.; 3];
            let mut em = [0.; 3];
            ep[j] = h;
            em[j] = -h;
            let (sp, sm) = (sigma(ep), sigma(em));
            let col = [
                (sp[0][0] - sm[0][0]) / (2. * h),
                (sp[1][1] - sm[1][1]) / (2. * h),
                (sp[1][0] - sm[1][0]) / (2. * h),
            ];
            for i in 0..3 {
                let rel = (t[i + 3 * j] - col[i]).abs() / c[0];
                max_rel = max_rel.max(rel);
            }
        }
        report.push((d, theta, max_rel));
    }
    // the reference 2-D formula is the exact tangent of the stress map, for
    // aligned and rotated damage frames (measured ≤ 4e-16; the map is
    // linear, so central differences are exact up to rounding: 1e-12)
    for &(d, theta, rel) in &report {
        println!("D = {d:?}, θ = {theta}: max |T − ∂σ/∂ε| / C11 = {rel:e}");
        assert!(rel < 1e-12, "{report:?}");
    }
}

/// XMAT-009, XMAT-010: the 1-D branch of upstream's
/// `computeTangentModuliOnQuad` is `tangent *= 1 − dam(0, 0)` (the damage
/// direction and `η` do not enter), for the elastic tangent of the 1-D
/// law. No model test reaches it (all use a 2-D mesh).
#[test]
fn tangent_1d_is_the_elastic_tangent_times_one_minus_d11() {
    let spec = akantu_model::constitutive_law::LawSpec {
        spatial_dimension: 1,
        option: String::new(),
        id: "m".into(),
    };
    let mut m = MaterialOrthotropicDamageIterative::<1>::new(&spec, None).unwrap();
    m.elastic.thermal.e = 10.;
    m.elastic.thermal.nu = 0.;
    akantu_parser::ParameterRegistry::update_internal_parameters(&mut m).unwrap();
    let mut c = [0.; 1];
    m.elastic.tangent_on_quad(&mut c);
    assert!(c[0] > 0.);
    for d11 in [0., 0.3, 0.75, 0.99] {
        let mut t = c;
        m.damaged_tangent_on_quad(&mut t, &[[d11]], &[[1.]]);
        assert_eq!(t[0], c[0] * (1. - d11), "d11 = {d11}");
        // the damage direction is irrelevant in 1-D
        let mut t2 = c;
        m.damaged_tangent_on_quad(&mut t2, &[[d11]], &[[-1.]]);
        assert_eq!(t2[0], t[0]);
    }
}
