// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_plastic_materials.cc (typed gtest ComputeStress for
// MaterialLinearIsotropicHardening<3>; the DISABLED EnergyDensity /
// ComputeTangentModuli / ComputeCelerity are covered by
// inelastic_properties.rs) and the material-point intent of
// test_material_elasto_plastic_linear_isotropic_hardening.

//! Same parameters (`E = 1`, `ν = 0`, `σ_y = 1`, `h = 0`), strain paths
//! (hydrostatic then deviatoric `t·getDeviatoricStrain`, 100 steps of 0.1),
//! expectations and absolute tolerances (1e-13) as upstream, on the J2 point
//! kernel called with explicit history like upstream's named tuples
//! (no thermal stress). Upstream draws one random rotation (`std::mt19937`);
//! the port uses one deterministic rotation (SplitMix64).

mod common;
mod inelastic_common;

use akantu_core::Real;
use akantu_core::tensor::{Matrix, MatrixExt};
use akantu_solid::materials::elastic::ElasticConstants;
use akantu_solid::materials::plastic::{
    J2Constants, J2History, ThermalIncrements, j2_small_strain_quad,
};
use akantu_validation::reduce::NanMinMax;
use inelastic_common::{Rng, random_rotation};

type M3 = Matrix<3, 3>;

fn m3(rows: &[Real]) -> M3 {
    std::array::from_fn(|c| std::array::from_fn(|r| rows[r * 3 + c]))
}

fn to_rows(m: &M3) -> Vec<Real> {
    (0..9).map(|k| m[k % 3][k / 3]).collect()
}

/// `R A Rᵀ` (upstream `applyRotation`).
fn apply_rotation(a: &M3, r: &M3) -> M3 {
    r.matmul(a).matmul(&r.transpose())
}

/// `Rᵀ A R` (upstream `reverseRotation`).
fn reverse_rotation(a: &M3, r: &M3) -> M3 {
    r.tr_matmul(a).matmul(r)
}

fn lp_inf(a: &M3) -> Real {
    to_rows(a).iter().fold(0., |m: Real, x| m.nan_max(x.abs()))
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
    let rotation = m3(&random_rotation(3, &mut Rng(0x5eed)));
    let max_strain = 10.;
    let strain_steps = 100.;
    let dt = max_strain / strain_steps;
    let no_th = ThermalIncrements {
        trial: 0.,
        last: 0.,
    };

    let mut previous_grad_u_rot = M3::zeros();
    let mut previous_sigma_rot = M3::zeros();
    let mut previous_inelastic_strain_rot = M3::zeros();
    let mut iso_hardening = 0.;
    let mut previous_iso_hardening = 0.;
    // hydrostatic loading (should not plastify)
    for i in 0..100 {
        let t = f64::from(i) * dt;
        let grad_u = m3(&[t, 0., 0., 0., t, 0., 0., 0., t]);
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
        let k3 = t * 3. * bulk_modulus_k;
        let sigma_expected = m3(&[k3, 0., 0., 0., k3, 0., 0., 0., k3]);
        assert!(
            lp_inf(&sigma.sub(&sigma_expected)) < 1e-13,
            "hydrostatic step {i}"
        );
        assert!(lp_inf(&inelastic_strain_rot) < 1e-13);
        previous_grad_u_rot = grad_u_rot;
        previous_sigma_rot = sigma_rot;
        previous_inelastic_strain_rot = inelastic_strain_rot;
        previous_iso_hardening = iso_hardening;
    }

    // deviatoric loading (should plastify); stress at the onset of plasticity
    let beta = 42_f64.sqrt();
    let t_p = sigma_0 / 2. / shear_modulus_mu / beta;
    let dev = |t: Real| m3(&[0., t, 2. * t, t, 0., 3. * t, 2. * t, 3. * t, 0.]);
    let mut previous_sigma = M3::zeros();
    let mut previous_inelastic_strain = M3::zeros();
    let mut checked = 0;
    for i in 0..100 {
        let t = f64::from(i) * dt;
        let grad_u = dev(t);
        let grad_u_rot = apply_rotation(&grad_u, &rotation);
        let mut iso = 0.;
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
            assert!(
                lp_inf(&sigma.sub(&sigma_expected)) < 1e-13,
                "elastic step {i}"
            );
            assert!(lp_inf(&inelastic_strain_rot) < 1e-13);
        } else if t > t_p + dt {
            // skip the transition from non plastic to plastic
            let delta_lambda_expected = dt / t * previous_sigma.double_dot(&grad_sym) / 2.;
            let delta_inelastic_strain_expected =
                previous_sigma.scale(delta_lambda_expected * 3. / 2. / sigma_0);
            let inelastic_strain_expected =
                delta_inelastic_strain_expected.add(&previous_inelastic_strain);
            assert!(
                lp_inf(&inelastic_strain.sub(&inelastic_strain_expected)) < 1e-13,
                "plastic strain step {i}"
            );
            let delta_sigma_expected = grad_sym
                .scale(0.5 * dt / t)
                .sub(&delta_inelastic_strain_expected)
                .scale(2. * shear_modulus_mu);
            let delta_sigma = sigma.sub(&previous_sigma);
            assert!(
                lp_inf(&delta_sigma_expected.sub(&delta_sigma)) < 1e-13,
                "stress increment step {i}"
            );
            checked += 1;
        }
        previous_sigma = sigma;
        previous_sigma_rot = sigma_rot;
        previous_grad_u_rot = grad_u_rot;
        previous_inelastic_strain = inelastic_strain;
        previous_inelastic_strain_rot = inelastic_strain_rot;
    }
    assert!(checked > 90);
}
