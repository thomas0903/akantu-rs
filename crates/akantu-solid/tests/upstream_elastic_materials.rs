// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/{test_elastic_materials.cc, test_material_fixtures.hh}
// (gtest suite test_material: TestElasticMaterialFixture.*).
//! The parts of `TestElasticMaterialFixture.*` (TEST-141) that draw random
//! inputs, with **upstream's** rotation: every
//! `FriendMaterial` seeds a `std::mt19937` with the gtest random seed 0 and
//! each typed test builds a fresh material, so the rotation of every test is
//! the first `getRandomRotation()` of `mt19937(0)` (2-D or 3-D), reproduced
//! by `upstream_material_fixture` (clean-room engine, D-030) and checked
//! bitwise against the oracle
//! (`fixtures/model/upstream_tests_2b/material_gtest_random_rotation`).
//!
//! * `ComputeStress` of `MaterialElastic<2|3>` (relative `1e-13` / absolute
//!   `1e-14`), `MaterialElasticOrthotropic<2|3>` and
//!   `MaterialElasticLinearAnisotropic<2|3>` (absolute `1e-13`), same
//!   parameters, strains and frames as upstream;
//! * the anisotropic `ComputeTangentModuli` (`‖tangent − C‖_F ≤ 1e-14`) and
//!   `ComputeCelerity` (`√(eig(C)[0]/ρ)`, `1e-14`, upstream mode UD-033),
//!   whose material frame is the random rotation of `setParams`.
//!
//! The deterministic parts (1-D stress, energy densities, the tangents and
//! celerities of the isotropic and orthotropic laws) are covered by
//! `test_elastic_materials.rs` with upstream's values (no random input);
//! that file also keeps its own (SplitMix64) rotation.

mod common;
mod upstream_material_fixture;

use akantu_core::tensor::MatrixX;
use akantu_core::{Element, Real};
use akantu_linalg::dense::{Storage, eig};
use akantu_model::constitutive_law::{LawPhase, LawSpec};
use akantu_parser::{ParamValue, ParameterRegistry};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::anisotropic::MaterialElasticLinearAnisotropic;
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::materials::orthotropic::MaterialElasticOrthotropic;
use common::SimplexModel;
use upstream_material_fixture::{
    Mat, Mt19937, apply_rotation, composed_strain, friend_material_generator, lp_inf,
    random_rotation, reverse_rotation, sub,
};

fn spec(dim: usize) -> LawSpec {
    LawSpec {
        spatial_dimension: dim,
        option: String::new(),
        id: "material".into(),
    }
}

/// Model without elements (upstream fixture: an empty mesh).
fn empty_model(dim: usize) -> SimplexModel {
    SimplexModel::new(dim, &[], &[])
}

fn set(m: &mut impl ParameterRegistry, name: &str, v: Real) {
    m.set_param(name, ParamValue::Real(v)).unwrap();
}

/// Column-major `n × n` from rows.
fn col_major(rows: &[&[Real]]) -> Vec<Real> {
    let n = rows.len();
    (0..n * n).map(|l| rows[l % n][l / n]).collect()
}

/// The rotation of a fresh `FriendMaterial`.
fn upstream_rotation<const D: usize>() -> Mat<D> {
    random_rotation::<D>(&mut friend_material_generator())
}

// --------------------------------------------------- the random inputs

/// The clean-room engine, the uniform draws and both rotations against the
/// oracle's `std::mt19937` / `std::uniform_real_distribution<Real>` /
/// transcribed `getRandomRotation` (black-box data), bitwise.
#[test]
fn material_gtest_random_inputs_match_the_oracle() {
    let dir = akantu_validation::fixtures_root()
        .join("model/upstream_tests_2b/material_gtest_random_rotation");
    let f = akantu_validation::Fixture::open(&dir).unwrap();
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    let raw = f.load_i64("engine/raw").unwrap().data;
    let mut g = Mt19937::new(0);
    let got: Vec<i64> = (0..raw.len()).map(|_| i64::from(g.next_u32())).collect();
    assert_eq!(got, raw, "mt19937(0) outputs");
    let uni = f.load_f64("engine/uniform").unwrap().data;
    let mut g = Mt19937::new(0);
    let mut d = akantu_core::random::distributions::UniformRealDistribution::new(0., 1.);
    for (i, want) in uni.iter().enumerate() {
        let x = d.sample(&mut g);
        assert_eq!(
            x.to_bits(),
            want.to_bits(),
            "uniform draw {i}: {x} vs {want}"
        );
    }
    let r2 = upstream_rotation::<2>();
    let r3 = upstream_rotation::<3>();
    let flat2: Vec<Real> = r2.iter().flatten().copied().collect();
    let flat3: Vec<Real> = r3.iter().flatten().copied().collect();
    for (name, got) in [("rotation/2", flat2), ("rotation/3", flat3)] {
        let want = f.load_f64(name).unwrap().data;
        assert_eq!(got.len(), want.len());
        for (k, (a, b)) in got.iter().zip(&want).enumerate() {
            assert_eq!(a.to_bits(), b.to_bits(), "{name}[{k}]: {a} vs {b}");
        }
    }
    // the standard's conformance value ([rand.predef]): 10000th output of a
    // default-seeded (5489) engine
    let mut g = Mt19937::new(5489);
    let mut x = 0;
    for _ in 0..10_000 {
        x = g.next_u32();
    }
    assert_eq!(x, 4_123_659_995);
}

// --------------------------------------------------------------- elastic

fn elastic_stress<const D: usize>(e: Real, nu: Real, relative: bool, tol: Real) {
    let model = empty_model(D);
    let mut m = MaterialElastic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
    set(&mut m, "E", e);
    set(&mut m, "nu", nu);
    set(&mut m, "rho", 2.);
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    let k = e / (3. * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let r = upstream_rotation::<D>();
    let grad_u: Mat<D> = composed_strain::<D>(1.);
    let grad_u_rot = apply_rotation(&grad_u, &r);
    let mut sigma_rot = [[0.0; D]; D];
    m.stress_on_quad(&grad_u_rot, 0., &mut sigma_rot);
    let sigma = reverse_rotation(&sigma_rot, &r);
    let strain: Mat<D> =
        std::array::from_fn(|c| std::array::from_fn(|i| 0.5 * (grad_u[c][i] + grad_u[i][c])));
    let tr: Real = (0..D).map(|i| strain[i][i]).sum();
    // upstream: 2μ dev(ε) + (σ_th + D·K)·I (dev with tr/3; D·K = K·tr ε)
    let expected: Mat<D> = std::array::from_fn(|c| {
        std::array::from_fn(|i| {
            let id = if i == c { 1. } else { 0. };
            2. * mu * (strain[c][i] - 1. / 3. * tr * id) + (0. + D as Real * k) * id
        })
    });
    let err = lp_inf(&sub(&sigma, &expected));
    let err = if relative {
        err / lp_inf(&expected)
    } else {
        err
    };
    eprintln!("elastic {D}-D ComputeStress: error {err:e} (tolerance {tol:e})");
    assert!(err <= tol, "dim {D}: stress error {err}");
}

/// `TestElasticMaterialFixture<MaterialElastic<2>>.ComputeStress`.
#[test]
fn test_elastic_materials_elastic_2d_compute_stress() {
    elastic_stress::<2>(1., 0.3, true, 1e-13);
}

/// `TestElasticMaterialFixture<MaterialElastic<3>>.ComputeStress`.
#[test]
fn test_elastic_materials_elastic_3d_compute_stress() {
    elastic_stress::<3>(1., 0.3, false, 1e-14);
}

// ----------------------------------------------- orthotropic, anisotropic

/// Upstream's `ComputeStress` of the orthotropic / anisotropic laws:
/// material axes = columns of the rotation, strain in the material frame,
/// stress compared in the material frame with `C'·ε_voigt` (`1e-13`).
fn rotated_stress_check<const D: usize, M>(
    m: &M,
    r: &Mat<D>,
    stress: impl Fn(&M, &Mat<D>, &mut Mat<D>),
    c_material: &[Real],
    intensity: Real,
    what: &str,
) {
    let v = D * (D + 1) / 2;
    let grad_u: Mat<D> = composed_strain::<D>(intensity);
    let grad_u_rot = reverse_rotation(&grad_u, r);
    let mut sigma_rot = [[0.0; D]; D];
    stress(m, &grad_u_rot, &mut sigma_rot);
    let sigma = apply_rotation(&sigma_rot, r);
    let t = akantu_core::voigt::voigt_table(D).unwrap();
    let eps: Mat<D> =
        std::array::from_fn(|c| std::array::from_fn(|i| (grad_u[c][i] + grad_u[i][c]) / 2.));
    let ev: Vec<Real> = (0..v)
        .map(|big| {
            let (i, j) = t.vec(big);
            t.factor(big) * eps[j][i]
        })
        .collect();
    let sv: Vec<Real> = (0..v)
        .map(|i| (0..v).map(|k| c_material[i + k * v] * ev[k]).sum())
        .collect();
    let mut expected = [[0.0; D]; D];
    for (big, s) in sv.iter().enumerate() {
        let (i, j) = t.vec(big);
        expected[j][i] = *s;
        expected[i][j] = *s;
    }
    let err = lp_inf(&sub(&sigma, &expected));
    eprintln!("{what} {D}-D ComputeStress: error {err:e} (tolerance 1e-13)");
    assert!(err <= 1e-13, "{what} dim {D}: stress error {err}");
}

struct Ortho {
    e: [Real; 3],
    nu: [Real; 3], // nu12, nu13, nu23
    g: [Real; 3],  // G12, G13, G23
    rho: Real,
}

impl Ortho {
    /// Upstream's `C_expected` in the material frame (Voigt, column-major).
    fn c_expected(&self, dim: usize) -> Vec<Real> {
        let [e1, e2, e3] = self.e;
        let [nu12, nu13, nu23] = self.nu;
        let [g12, g13, g23] = self.g;
        let nu21 = nu12 * e2 / e1;
        if dim == 2 {
            let gamma = 1. / (1. - nu12 * nu21);
            return col_major(&[
                &[gamma * e1, gamma * e1 * nu21, 0.],
                &[gamma * e1 * nu21, gamma * e2, 0.],
                &[0., 0., g12],
            ]);
        }
        let nu31 = nu13 * e3 / e1;
        let nu32 = nu23 * e3 / e2;
        let gamma = 1. / (1. - nu12 * nu21 - nu23 * nu32 - nu31 * nu13 - 2. * nu21 * nu32 * nu13);
        let c11 = gamma * e1 * (1. - nu23 * nu32);
        let c22 = gamma * e2 * (1. - nu13 * nu31);
        let c33 = gamma * e3 * (1. - nu12 * nu21);
        let c12 = gamma * e1 * (nu21 + nu31 * nu23);
        let c13 = gamma * e1 * (nu31 + nu21 * nu32);
        let c23 = gamma * e2 * (nu32 + nu12 * nu31);
        col_major(&[
            &[c11, c12, c13, 0., 0., 0.],
            &[c12, c22, c23, 0., 0., 0.],
            &[c13, c23, c33, 0., 0., 0.],
            &[0., 0., 0., g23, 0., 0.],
            &[0., 0., 0., 0., g13, 0.],
            &[0., 0., 0., 0., 0., g12],
        ])
    }

    /// Upstream `setParams` + the test's `*dir_vecs[j] = rotation(j)` and
    /// `updateInternalParameters()` (upstream mode: UD-104/UD-105).
    fn build<const D: usize>(&self, axes: &Mat<D>) -> MaterialElasticOrthotropic<D> {
        let model = empty_model(D);
        let mut m = MaterialElasticOrthotropic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
        *m.options_mut().unwrap() = LawOptions::upstream();
        m.core.dir_vecs = *axes;
        for (name, v) in [
            ("E1", self.e[0]),
            ("E2", self.e[1]),
            ("nu12", self.nu[0]),
            ("G12", self.g[0]),
            ("rho", self.rho),
        ] {
            set(&mut m, name, v);
        }
        if D == 3 {
            for (name, v) in [
                ("E3", self.e[2]),
                ("nu13", self.nu[1]),
                ("nu23", self.nu[2]),
                ("G13", self.g[1]),
                ("G23", self.g[2]),
            ] {
                set(&mut m, name, v);
            }
        }
        m.init_material(&model.ctx(LawPhase::Init)).unwrap();
        m
    }
}

fn orthotropic_stress<const D: usize>(o: &Ortho) {
    let r = upstream_rotation::<D>();
    let m = o.build::<D>(&r);
    rotated_stress_check::<D, _>(
        &m,
        &r,
        |m, g, s| m.core.stress_on_quad(g, s).unwrap(),
        &o.c_expected(D),
        2.,
        "orthotropic",
    );
}

/// `TestElasticMaterialFixture<MaterialElasticOrthotropic<2>>.ComputeStress`.
#[test]
fn test_elastic_materials_orthotropic_2d_compute_stress() {
    orthotropic_stress::<2>(&Ortho {
        e: [1., 2., 0.],
        nu: [0.1, 0., 0.],
        g: [2., 0., 0.],
        rho: 2.5,
    });
}

/// `TestElasticMaterialFixture<MaterialElasticOrthotropic<3>>.ComputeStress`.
#[test]
fn test_elastic_materials_orthotropic_3d_compute_stress() {
    orthotropic_stress::<3>(&Ortho {
        e: [1., 2., 3.],
        nu: [0.1, 0.2, 0.3],
        g: [2., 3., 1.],
        rho: 2.3,
    });
}

/// Upstream `setParams` of `MaterialElasticLinearAnisotropic<D>`: `C'`,
/// `ρ`, and the material axes = columns of the random rotation.
fn anisotropic<const D: usize>(
    c: &[&[Real]],
    axes: &Mat<D>,
    rho: Real,
) -> MaterialElasticLinearAnisotropic<D> {
    let model = empty_model(D);
    let mut m = MaterialElasticLinearAnisotropic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
    *m.options_mut().unwrap() = LawOptions::upstream();
    m.core.dir_vecs = *axes;
    for (i, row) in c.iter().enumerate() {
        for (j, &cij) in row.iter().enumerate().skip(i) {
            set(&mut m, &format!("C{}{}", i + 1, j + 1), cij);
        }
    }
    set(&mut m, "rho", rho);
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    m
}

fn anisotropic_suite<const D: usize>(c: &[&[Real]], rho: Real, intensity: Real) {
    let v = D * (D + 1) / 2;
    let r = upstream_rotation::<D>();
    // ComputeStress (a fresh material)
    let m = anisotropic::<D>(c, &r, rho);
    rotated_stress_check::<D, _>(
        &m,
        &r,
        |m, g, s| m.core.stress_on_quad(g, s).unwrap(),
        &col_major(c),
        intensity,
        "anisotropic",
    );
    // ComputeTangentModuli (a fresh material, same rotation): the law's C
    let m = anisotropic::<D>(c, &r, rho);
    let mut t = vec![0.0; v * v];
    m.core.tangent_on_quad(&mut t);
    let tangent_error = t
        .iter()
        .zip(m.core.c.as_slice())
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<Real>()
        .sqrt();
    assert!(tangent_error <= 1e-14, "tangent error {tangent_error}");
    // ComputeCelerity: √(eig(C)[0]/ρ) (upstream mode, UD-033)
    let expected = {
        let ev = eig(
            &MatrixX::from_col_major(v, v, m.core.c.as_slice().to_vec()).unwrap(),
            // the law's `C` is a dynamic matrix upstream (as in
            // test_elastic_materials.rs)
            Storage::Dynamic,
        )
        .unwrap();
        (ev[0] / rho).sqrt()
    };
    let celerity = m.celerity(Element::NULL).unwrap();
    assert!(
        (celerity - expected).abs() <= 1e-14,
        "celerity {celerity} vs {expected}"
    );
}

/// `TestElasticMaterialFixture<MaterialElasticLinearAnisotropic<2>>.{ComputeStress,
/// ComputeTangentModuli, ComputeCelerity}`.
#[test]
fn test_elastic_materials_anisotropic_2d_random_frame() {
    anisotropic_suite::<2>(
        &[&[1.0, 0.3, 0.4], &[0.3, 2.0, 0.1], &[0.4, 0.1, 1.5]],
        2.7,
        1.,
    );
}

/// `TestElasticMaterialFixture<MaterialElasticLinearAnisotropic<3>>.{ComputeStress,
/// ComputeTangentModuli, ComputeCelerity}`.
#[test]
fn test_elastic_materials_anisotropic_3d_random_frame() {
    anisotropic_suite::<3>(
        &[
            &[1.0, 0.3, 0.4, 0.3, 0.2, 0.1],
            &[0.3, 2.0, 0.1, 0.2, 0.3, 0.2],
            &[0.4, 0.1, 1.5, 0.1, 0.4, 0.3],
            &[0.3, 0.2, 0.1, 2.4, 0.1, 0.4],
            &[0.2, 0.3, 0.4, 0.1, 0.9, 0.1],
            &[0.1, 0.2, 0.3, 0.4, 0.1, 1.2],
        ],
        2.9,
        2.,
    );
}
