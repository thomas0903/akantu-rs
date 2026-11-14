// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_plastic_materials.cc, test_material_fixtures.hh}
// (gtest suite test_material: TestPlasticMaterialFixture.ComputeStress).
//! `TestPlasticMaterialFixture<MaterialLinearIsotropicHardening<3>>.
//! ComputeStress` (TEST-144) with **upstream's** rotation: the first
//! `getRandomRotation()` of `std::mt19937` seeded with the gtest random
//! seed 0 (`upstream_material_fixture`, clean-room engine checked bitwise
//! against the oracle by `upstream_elastic_materials.rs`). Same parameters
//! (`E = 1`, `ν = 0`, `σ_y = 1`, `h = 0`), strain paths (100 hydrostatic
//! steps `t·I`, then 100 deviatoric steps `t·getDeviatoricStrain(1)`,
//! `dt = 0.1`), history handling (the deviatoric loop resets the
//! hardening, as upstream), expectations and absolute tolerances (`1e-13`)
//! as upstream, on the J2 point kernel called with explicit history like
//! upstream's named tuples (no thermal stress). The disabled
//! `EnergyDensity` / `ComputeTangentModuli` / `ComputeCelerity` cases are
//! covered by `inelastic_properties.rs`; `test_plastic_materials.rs` runs
//! the same checks with its own (SplitMix64) rotation.

mod upstream_material_fixture;

use akantu_core::Real;
use akantu_core::tensor::MatrixExt;
use akantu_solid::materials::elastic::ElasticConstants;
use akantu_solid::materials::plastic::{
    J2Constants, J2History, ThermalIncrements, j2_small_strain_quad,
};
use upstream_material_fixture::{
    Mat, apply_rotation, deviatoric_strain, friend_material_generator, hydrostatic_strain, lp_inf,
    random_rotation, reverse_rotation,
};

type M3 = Mat<3>;

fn zeros() -> M3 {
    [[0.; 3]; 3]
}

#[test]
fn test_plastic_materials_linear_isotropic_hardening_3d_compute_stress() {
    let (e, nu, sigma_0, h) = (1., 0., 1., 0.);
    let bulk_modulus_k = e / 3. / (1. - 2. * nu);
    let shear_modulus_mu = 0.5 * e / (1. + nu);
    let lambda = nu * e / ((1. + nu) * (1. - 2. * nu));
    let c = J2Constants {
        elastic: ElasticConstants::<3> {
            e,
            lambda,
            mu: shear_modulus_mu,
        },
        h,
        sigma_y: sigma_0,
    };
    let rotation = random_rotation::<3>(&mut friend_material_generator());
    let max_strain = 10.;
    let strain_steps = 100.;
    let dt = max_strain / strain_steps;
    let no_th = ThermalIncrements {
        trial: 0.,
        last: 0.,
    };

    let mut previous_grad_u_rot = zeros();
    let mut previous_sigma_rot = zeros();
    let mut previous_inelastic_strain_rot = zeros();
    let mut iso_hardening = 0.;
    let mut previous_iso_hardening = 0.;
    // hydrostatic loading (should not plastify)
    for i in 0..100 {
        let t = f64::from(i) * dt;
        let grad_u = hydrostatic_strain(t);
        let grad_u_rot = apply_rotation(&grad_u, &rotation);
        let hist = J2History {
            previous_grad_u: &previous_grad_u_rot,
            previous_sigma: &previous_sigma_rot,
            previous_inelastic_strain: &previous_inelastic_strain_rot,
            previous_iso_hardening,
        };
        let (sigma_rot, inelastic_strain_rot) =
            j2_small_strain_quad(&c, &grad_u_rot, &hist, no_th, &mut iso_hardening);
        let sigma = reverse_rotation(&sigma_rot, &rotation);
        let sigma_expected = hydrostatic_strain(t * 3. * bulk_modulus_k);
        let stress_error = lp_inf(&sigma.sub(&sigma_expected));
        assert!(
            stress_error <= 1e-13,
            "hydrostatic step {i}: {stress_error}"
        );
        assert!(lp_inf(&inelastic_strain_rot) <= 1e-13);
        previous_grad_u_rot = grad_u_rot;
        previous_sigma_rot = sigma_rot;
        previous_inelastic_strain_rot = inelastic_strain_rot;
        previous_iso_hardening = iso_hardening;
    }

    // deviatoric loading (should plastify); stress at the onset of plasticity
    let beta = 42_f64.sqrt();
    let t_p = sigma_0 / 2. / shear_modulus_mu / beta;
    let mut previous_sigma = zeros();
    let mut previous_inelastic_strain = zeros();
    let (mut elastic_checked, mut plastic_checked) = (0, 0);
    let mut worst = 0.0_f64;
    for i in 0..100 {
        let t = f64::from(i) * dt;
        let grad_u = deviatoric_strain(t);
        let grad_u_rot = apply_rotation(&grad_u, &rotation);
        let mut iso: Real = 0.;
        let hist = J2History {
            previous_grad_u: &previous_grad_u_rot,
            previous_sigma: &previous_sigma_rot,
            previous_inelastic_strain: &previous_inelastic_strain_rot,
            previous_iso_hardening: 0.,
        };
        let (sigma_rot, inelastic_strain_rot) =
            j2_small_strain_quad(&c, &grad_u_rot, &hist, no_th, &mut iso);
        let sigma = reverse_rotation(&sigma_rot, &rotation);
        let inelastic_strain = reverse_rotation(&inelastic_strain_rot, &rotation);
        let grad_sym = grad_u.add(&grad_u.transpose());
        if t < t_p {
            let sigma_expected = grad_sym.scale(shear_modulus_mu);
            let err = lp_inf(&sigma.sub(&sigma_expected));
            worst = worst.max(err);
            assert!(err <= 1e-13, "elastic step {i}: {err}");
            assert!(lp_inf(&inelastic_strain_rot) <= 1e-13);
            elastic_checked += 1;
        } else if t > t_p + dt {
            // skip the transition from non plastic to plastic
            let delta_lambda_expected = dt / t * previous_sigma.double_dot(&grad_sym) / 2.;
            let delta_inelastic_strain_expected =
                previous_sigma.scale(delta_lambda_expected * 3. / 2. / sigma_0);
            let inelastic_strain_expected =
                delta_inelastic_strain_expected.add(&previous_inelastic_strain);
            let err = lp_inf(&inelastic_strain.sub(&inelastic_strain_expected));
            worst = worst.max(err);
            assert!(err <= 1e-13, "plastic strain step {i}: {err}");
            let delta_sigma_expected = grad_sym
                .scale(0.5 * dt / t)
                .sub(&delta_inelastic_strain_expected)
                .scale(2. * shear_modulus_mu);
            let delta_sigma = sigma.sub(&previous_sigma);
            let err = lp_inf(&delta_sigma_expected.sub(&delta_sigma));
            worst = worst.max(err);
            assert!(err <= 1e-13, "stress increment step {i}: {err}");
            plastic_checked += 1;
        }
        previous_sigma = sigma;
        previous_sigma_rot = sigma_rot;
        previous_grad_u_rot = grad_u_rot;
        previous_inelastic_strain = inelastic_strain;
        previous_inelastic_strain_rot = inelastic_strain_rot;
    }
    eprintln!(
        "J2 3-D ComputeStress: {elastic_checked} elastic + {plastic_checked} plastic steps, worst {worst:e}"
    );
    assert!(elastic_checked >= 1 && plastic_checked > 90);
}
