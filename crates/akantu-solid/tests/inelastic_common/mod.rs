// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test support for the
// inelastic material tests: one law on a one-element simplex model
// (one integration point), driven by prescribed displacement gradients as a
// converged static step of SolidMechanicsModel would (beforeSolveStep,
// computeStress, afterSolveStep).

#![allow(dead_code)] // each test binary uses a subset

use akantu_core::{Array, Element, Real};
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_law::{InternalFieldRef, LawPhase};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_validation::reduce::NanMinMax;

use super::common::{NG, SimplexModel, materials_from_dat_with, simplex};

/// The probe's one-element meshes (same nodes as
/// `oracle/probes/cpp/material_probe.cc`).
#[must_use]
pub fn one_element(dim: usize) -> SimplexModel {
    match dim {
        1 => SimplexModel::new(1, &[vec![0.], vec![1.3]], &[vec![0, 1]]),
        2 => SimplexModel::new(
            2,
            &[vec![0., 0.], vec![1.1, 0.2], vec![0.3, 0.9]],
            &[vec![0, 1, 2]],
        ),
        _ => SimplexModel::new(
            3,
            &[
                vec![0., 0., 0.],
                vec![1.1, 0.1, 0.05],
                vec![0.2, 0.9, 0.1],
                vec![0.1, 0.2, 1.2],
            ],
            &[vec![0, 1, 2, 3]],
        ),
    }
}

/// One law at one quadrature point.
pub struct Point {
    pub dim: usize,
    pub model: SimplexModel,
    pub handler: ConstitutiveLawsHandler<dyn Material>,
}

impl Point {
    /// The first law of `dat` (with `options` set before `initMaterial`).
    pub fn new(dim: usize, dat: &str, options: LawOptions) -> Self {
        let model = one_element(dim);
        let handler = materials_from_dat_with(&model, dat.as_bytes(), Some(options)).unwrap();
        Self {
            dim,
            model,
            handler,
        }
    }

    /// The law.
    pub fn law(&mut self) -> &mut dyn Material {
        self.handler.law_mut(0).unwrap()
    }

    /// Set `grad_u` (row-major `dim × dim`) and compute the stress, without
    /// touching the history (a Newton iterate).
    pub fn eval(&mut self, grad_u_rows: &[Real]) -> akantu_core::Result<()> {
        let d = self.dim;
        let mut col = vec![0.0; d * d];
        for r in 0..d {
            for c in 0..d {
                col[r + c * d] = grad_u_rows[r * d + c];
            }
        }
        let ty = simplex(d);
        let model = &self.model;
        let law = self.handler.law_mut(0).unwrap();
        law.base_mut()
            .grad_u
            .get_mut(ty, NG)
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&col);
        law.compute_stress(&model.ctx(LawPhase::Stress), ty, NG)
    }

    /// A converged step: `beforeSolveStep`, stress at `grad_u_rows`,
    /// `afterSolveStep(true)` (energies).
    pub fn step(&mut self, grad_u_rows: &[Real]) -> akantu_core::Result<()> {
        {
            let model = &self.model;
            let law = self.handler.law_mut(0).unwrap();
            law.before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))?;
        }
        self.eval(grad_u_rows)?;
        let model = &self.model;
        let law = self.handler.law_mut(0).unwrap();
        law.after_solve_step(&model.ctx(LawPhase::AfterSolveStep), true)
    }

    /// The stored stress (`stress`, or the second Piola–Kirchhoff stress
    /// with finite deformation), row-major.
    pub fn stress(&mut self) -> Vec<Real> {
        let d = self.dim;
        let ty = simplex(d);
        let law = self.handler.law_mut(0).unwrap();
        let s = law
            .base()
            .sigma_field()
            .unwrap()
            .get(ty, NG)
            .unwrap()
            .tuple(0)
            .to_vec();
        rows(&s, d)
    }

    /// Tangent (row-major `voigt × voigt`).
    pub fn tangent(&mut self) -> Vec<Real> {
        let d = self.dim;
        let v = d * (d + 1) / 2;
        let ty = simplex(d);
        let model = &self.model;
        let law = self.handler.law_mut(0).unwrap();
        let mut t = Array::zeros(1, v * v);
        law.compute_tangent_moduli(&model.ctx(LawPhase::Tangent), ty, NG, &mut t)
            .unwrap();
        rows(t.as_slice(), v)
    }

    /// Named energy.
    pub fn energy(&mut self, id: &str) -> Real {
        let model = &self.model;
        let law = self.handler.law_mut(0).unwrap();
        law.energy_of(&model.ctx(LawPhase::Energy), id).unwrap()
    }

    /// Potential energy density at the point (not integrated).
    pub fn potential_density(&mut self) -> Real {
        let d = self.dim;
        let ty = simplex(d);
        let model = &self.model;
        let law = self.handler.law_mut(0).unwrap();
        law.compute_potential_energy(&model.ctx(LawPhase::Energy), ty)
            .unwrap();
        law.base().potential_energy.get(ty, NG).unwrap().as_slice()[0]
    }

    /// Values of internal `name` at the point.
    pub fn internal(&mut self, name: &str) -> Vec<Real> {
        let ty = simplex(self.dim);
        let law = self.handler.law_mut(0).unwrap();
        let mut found = None;
        law.for_each_internal(&mut |n, f| {
            if n == name
                && let InternalFieldRef::Real(f) = f
            {
                found = Some(f.get(ty, NG).unwrap().tuple(0).to_vec());
            }
        });
        found.unwrap_or_else(|| panic!("internal {name} missing"))
    }

    /// Scalar internal.
    pub fn scalar(&mut self, name: &str) -> Real {
        self.internal(name)[0]
    }

    /// Wave speeds of the element.
    pub fn celerity(&mut self) -> Real {
        let ty = simplex(self.dim);
        let law = self.handler.law_mut(0).unwrap();
        law.celerity(Element::new(ty, 0, NG)).unwrap()
    }
}

/// Column-major tuple → row-major.
#[must_use]
pub fn rows(tuple: &[Real], n: usize) -> Vec<Real> {
    super::common::col_major_to_rows(tuple, n)
}

/// `max |a_i − b_i|`.
#[must_use]
pub fn max_abs_diff(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y).abs())
        .fold(0., NanMinMax::nan_max)
}

/// `max |a_i|`.
#[must_use]
pub fn max_abs(a: &[Real]) -> Real {
    a.iter().fold(0., |m, x| m.nan_max(x.abs()))
}

/// A small deterministic generator (SplitMix64) in `[0, 1)`.
pub struct Rng(pub u64);
impl Rng {
    pub fn next(&mut self) -> Real {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)] // 53-bit mantissa draw
        let v = (z >> 11) as Real / (1u64 << 53) as Real;
        v
    }
    /// Uniform in `[-1, 1)`.
    pub fn sym(&mut self) -> Real {
        2. * self.next() - 1.
    }
}

/// A random rotation (row-major `dim × dim`, right-handed).
#[must_use]
pub fn random_rotation(dim: usize, rng: &mut Rng) -> Vec<Real> {
    let mut r = vec![0.0; dim * dim];
    match dim {
        1 => r[0] = 1.,
        2 => {
            let t = std::f64::consts::PI * rng.sym();
            let (s, c) = t.sin_cos();
            r.copy_from_slice(&[c, -s, s, c]);
        }
        _ => {
            // unit quaternion
            let q = [rng.sym(), rng.sym(), rng.sym(), rng.sym()];
            let n = q.iter().map(|x| x * x).sum::<Real>().sqrt();
            let [w, x, y, z] = q.map(|v| v / n);
            r.copy_from_slice(&[
                1. - 2. * (y * y + z * z),
                2. * (x * y - z * w),
                2. * (x * z + y * w),
                2. * (x * y + z * w),
                1. - 2. * (x * x + z * z),
                2. * (y * z - x * w),
                2. * (x * z - y * w),
                2. * (y * z + x * w),
                1. - 2. * (x * x + y * y),
            ]);
        }
    }
    r
}

/// Row-major `a · b` (`n × n`).
#[must_use]
pub fn matmul(a: &[Real], b: &[Real], n: usize) -> Vec<Real> {
    let mut c = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            c[i * n + j] = (0..n).map(|k| a[i * n + k] * b[k * n + j]).sum();
        }
    }
    c
}

/// Row-major transpose.
#[must_use]
pub fn transpose(a: &[Real], n: usize) -> Vec<Real> {
    let mut t = vec![0.0; n * n];
    for i in 0..n {
        for j in 0..n {
            t[j * n + i] = a[i * n + j];
        }
    }
    t
}
