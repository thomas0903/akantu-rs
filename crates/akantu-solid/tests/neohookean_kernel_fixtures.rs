// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Differential test of
// the Neo-Hookean kernels (src/model/solid_mechanics/materials/
// material_finite_deformation/material_neohookean_inline_impl.hh) against the
// oracle's own kernels, recorded through the probe-only keyword
// `neohookean_kernel` (oracle/probes/cpp/material_probe_wpmat2.cc: upstream's
// MaterialNeohookean with the aborting parent calls skipped, UD-026).

//! Per recorded step, from the recorded `grad_u` (and, in plane stress, the
//! recorded `C33`):
//!
//! * `S = μI + (λ ln J − μ)C⁻¹` ([`neohookean_stress`]) is bit-identical to
//!   the oracle's `piola_kirchhoff_2` — the documented stress *is* upstream's
//!   kernel;
//! * the non-condensed tangent ([`neohookean_tangent`]) is bit-identical to
//!   the oracle's tangent (the port's plane-stress tangent is the condensed
//!   one, UD-057; tested by finite differences in
//!   `test_finite_def_materials.rs`);
//! * the oracle's `C33` equals upstream's Newton–Raphson (`|f| ≤ 1e-5`)
//!   replayed here bit for bit, and the port's machine-precision root within
//!   the bound implied by that tolerance;
//! * the oracle's energy equals upstream's (defective, UD-057) kernel
//!   `λ (ln J)² + μ(−ln J + (tr C − dim)/2)/2` (no `C33`), integrated —
//!   evidence for the defect the port corrects;
//! * wave speeds `√((λ + 2μ)/ρ)`, `√(μ/ρ)` through the port's law.

mod common;

use akantu_core::Real;
use akantu_core::glibc_libm;
use akantu_core::tensor::{Matrix, SquareMatrixExt};
use akantu_model::constitutive_law::ModelServices;
use akantu_solid::materials::neohookean::{neohookean_stress, neohookean_tangent, solve_c33};
use akantu_validation::{Fixture, fixtures_root};
use common::{SimplexModel, bitwise_equal_count, simplex};

const E: Real = 1e6;
const NU: Real = 0.45;

fn lame(dim: usize) -> (Real, Real) {
    let nu = if dim == 1 { 0. } else { NU };
    (nu * E / ((1. + nu) * (1. - 2. * nu)), E / (2. * (1. + nu)))
}

/// Row-major `D × D` → column-major matrix.
fn mat<const D: usize>(rows: &[Real]) -> Matrix<D, D> {
    std::array::from_fn(|c| std::array::from_fn(|r| rows[r * D + c]))
}

/// Upstream `C33_NR` solved by `Math::NewtonRaphson(1e-5, 100)` from `x0`.
fn upstream_c33(lambda: Real, mu: Real, g: &Matrix<2, 2>, x0: Real) -> Real {
    let f = akantu_solid::material_tensor::grad_u_to_f(g);
    let c = akantu_solid::material_tensor::right_cauchy(&f);
    let det = glibc_libm::log(c[0][0] * c[1][1] - c[1][0] * c[1][0]);
    let fx = |x: Real| lambda / 2. * (glibc_libm::log(x) + det) + mu * (x - 1.);
    let mut x = x0;
    let mut f_x = fx(x);
    let mut iter = 0;
    while f_x.abs() > 1e-5 && iter < 100 {
        x -= f_x / (lambda / (2. * x) + mu);
        f_x = fx(x);
        iter += 1;
    }
    x
}

/// Upstream's energy kernel (UD-057).
fn upstream_energy<const D: usize>(lambda: Real, mu: Real, g: &Matrix<D, D>) -> Real {
    let f = akantu_solid::material_tensor::grad_u_to_f(g);
    let c = akantu_solid::material_tensor::right_cauchy(&f);
    let j = f.det();
    #[allow(clippy::cast_precision_loss)]
    let dim = D as Real;
    lambda * glibc_libm::pow(glibc_libm::log(j), 2.)
        + mu * (-glibc_libm::log(j) + (c.trace() - dim) / 2.) / 2.
}

fn check_case<const D: usize>(fx: &Fixture) -> (usize, usize) {
    let id = fx.case_id().to_owned();
    let plane_stress = id.contains("plane_stress");
    let (lambda, mu) = lame(D);
    let model = SimplexModel::from_msh(D, &fx.input_path("mesh").unwrap());
    let grad_u = fx.load_f64("grad_u").unwrap();
    let pk2 = fx.load_f64("internal/piola_kirchhoff_2").unwrap();
    let tangent = fx.load_f64("tangent").unwrap();
    let energy = fx.load_f64("energy/potential").unwrap();
    let c33 = fx.load_f64("internal/third_axis_deformation").ok();
    let n = D * D;
    let v = D * (D + 1) / 2;
    let nsteps = grad_u.shape[0];
    let (mut compared, mut identical) = (0, 0);
    let mut x_prev = 1.;
    for k in 0..nsteps {
        let g = mat::<D>(&grad_u.data[k * n..(k + 1) * n]);
        let x = c33.as_ref().map_or(1., |c| c.data[k]);
        if plane_stress {
            let g2: &Matrix<2, 2> = akantu_core::tensor::as_matrix(g.as_flattened()).unwrap();
            let xu = upstream_c33(lambda, mu, g2, x_prev);
            assert!(
                akantu_validation::exact_eq(x, xu), // D-046
                "{id} step {k}: upstream C33 replay: {xu:e} vs oracle {x:e}"
            );
            let xr = solve_c33(lambda, mu, g2, x_prev).unwrap();
            // |f| <= 1e-5 at the oracle's root, f' >= mu
            assert!(
                (xr - x).abs() <= 1e-5 / mu,
                "{id} step {k}: C33 {xr} vs {x}"
            );
            x_prev = x;
        }
        // stress (pk2 internal: column-major tuples)
        let s = neohookean_stress::<D>(lambda, mu, &g, x).unwrap();
        let want = &pk2.data[k * n..(k + 1) * n];
        compared += n;
        identical += bitwise_equal_count(want, s.as_flattened());
        // non-condensed tangent (fixture: row-major [row, col])
        let mut t = vec![0.; v * v];
        neohookean_tangent::<D>(lambda, mu, &g, x, &mut t).unwrap();
        let rows = common::col_major_to_rows(&t, v);
        compared += v * v;
        identical += bitwise_equal_count(&tangent.data[k * v * v..(k + 1) * v * v], &rows);
        // upstream energy kernel, integrated over the element
        let psi = upstream_energy::<D>(lambda, mu, &g);
        let e = model
            .integrate_scalar(&[psi], simplex(D), common::NG, &[0])
            .unwrap();
        compared += 1;
        identical += usize::from(akantu_validation::exact_eq(energy.data[k], e)); // D-046
    }
    assert_eq!(
        identical, compared,
        "{id}: {identical}/{compared} bit-identical"
    );
    (compared, identical)
}

#[test]
fn neohookean_kernels_match_oracle_bitwise() {
    let dir = fixtures_root().join("material").join("neohookean_kernel");
    let mut total = 0;
    let mut ncases = 0;
    let mut entries: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .map(|e| e.path())
        .filter(|p| p.join("manifest.json").exists())
        .collect();
    entries.sort();
    for p in entries {
        let fx = Fixture::open(&p).unwrap();
        let dim = fx.manifest().parameters["spatial_dimension"]
            .as_i64()
            .unwrap();
        let (c, _) = match dim {
            1 => check_case::<1>(&fx),
            2 => check_case::<2>(&fx),
            _ => check_case::<3>(&fx),
        };
        total += c;
        ncases += 1;
    }
    assert!(ncases >= 7);
    eprintln!("neohookean_kernel: {ncases} cases, {total}/{total} values bit-identical");
}

/// Wave speeds recorded by the probe vs the port's `neohookean` law.
#[test]
fn neohookean_wave_speeds_match_oracle() {
    for dim in [1usize, 2, 3] {
        let case = if dim == 1 {
            "1d_uniaxial"
        } else if dim == 2 {
            "2d_composed"
        } else {
            "3d_composed"
        };
        let fx = Fixture::open(
            &fixtures_root()
                .join("material/neohookean_kernel")
                .join(case),
        )
        .unwrap();
        let (lambda, mu) = lame(dim);
        let push = fx.load_f64("wave_speed/push").unwrap();
        let shear = fx.load_f64("wave_speed/shear").unwrap();
        let want_p = ((lambda + 2. * mu) / 1000.).sqrt();
        let want_s = (mu / 1000.).sqrt();
        assert!(
            push.data
                .iter()
                .all(|&x| akantu_validation::exact_eq(x, want_p)), // D-046
            "{case} push"
        );
        assert!(
            shear
                .data
                .iter()
                .all(|&x| akantu_validation::exact_eq(x, want_s)), // D-046
            "{case} shear"
        );
    }
}
