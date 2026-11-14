// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_solid_mechanics_model/
// test_materials/test_elastic_materials.cc and test_material_fixtures.hh
// (typed gtests ComputeStress, EnergyDensity, ComputeTangentModuli,
// ComputeCelerity for MaterialElastic<1,2,3>, MaterialElasticOrthotropic<2,3>,
// MaterialElasticLinearAnisotropic<2,3>).

//! Same parameters, strains and tolerances as upstream. Upstream draws one
//! random rotation per run (`std::mt19937` seeded with the gtest seed); the
//! port draws one deterministic random rotation (SplitMix64) — the absolute
//! upstream tolerances (1e-13/1e-14 on stresses up to O(100)) leave no room
//! for the round-off of the test's own rotations over many draws; many
//! rotations with a derived bound are checked in
//! `elastic_family_properties.rs`. The celerity
//! tests of the anisotropic laws compare with upstream's `eig(C)[0]`
//! (`LawOptions::upstream()`, UD-033); the corrected default is tested in
//! `elastic_family_properties.rs`.

mod common;

use akantu_core::tensor::{Matrix, MatrixExt, MatrixX};
use akantu_core::{Element, Real};
use akantu_linalg::dense::{Storage, eig};
use akantu_model::constitutive_law::{LawPhase, LawSpec};
use akantu_parser::{ParamValue, ParameterRegistry};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::anisotropic::MaterialElasticLinearAnisotropic;
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::materials::orthotropic::MaterialElasticOrthotropic;
use akantu_validation::reduce::NanMinMax;
use common::SimplexModel;

/// Random rotations per stress test (upstream: one).
const ROTATIONS: usize = 1;

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

/// Upstream `getDeviatoricStrain` / `getHydrostaticStrain` /
/// `getComposedStrain` (3 × 3, the 2-D tests take the top-left block).
fn composed_strain(intensity: Real) -> [[Real; 3]; 3] {
    let dev = [[0., 1., 2.], [1., 0., 3.], [2., 3., 0.]];
    std::array::from_fn(|i| {
        std::array::from_fn(|j| {
            let h = if i == j { intensity } else { 0. };
            (h + dev[i][j] * intensity) * intensity
        })
    })
}

/// Row-major `D × D` → column-major `Matrix<D, D>`.
fn mat<const D: usize>(rows: &[[Real; 3]; 3]) -> Matrix<D, D> {
    std::array::from_fn(|c| std::array::from_fn(|r| rows[r][c]))
}

/// A small deterministic generator (SplitMix64) for random rotations.
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> Real {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)] // 53-bit mantissa draw
        let v = (z >> 11) as Real / (1u64 << 53) as Real;
        v
    }
}

/// Upstream `getRandomRotation`: a right-handed orthonormal matrix
/// (column-major `D × D`).
fn random_rotation<const D: usize>(rng: &mut Rng) -> Matrix<D, D> {
    let mut v1 = [0.0; 3];
    while v1.iter().map(|x| x * x).sum::<Real>().sqrt() < 1e-9 {
        for x in v1.iter_mut().take(D) {
            *x = rng.next();
        }
    }
    let n = v1.iter().map(|x| x * x).sum::<Real>().sqrt();
    v1.iter_mut().for_each(|x| *x /= n);
    let mut r = [[0.0; 3]; 3]; // columns
    r[0] = v1;
    if D == 2 {
        r[1] = [-v1[1], v1[0], 0.];
    }
    if D == 3 {
        let mut v2 = [rng.next(), rng.next(), rng.next()];
        let d: Real = (0..3).map(|i| v2[i] * v1[i]).sum();
        (0..3).for_each(|i| v2[i] -= d * v1[i]);
        let n = v2.iter().map(|x| x * x).sum::<Real>().sqrt();
        v2.iter_mut().for_each(|x| *x /= n);
        r[1] = v2;
        r[2] = [
            v1[1] * v2[2] - v1[2] * v2[1],
            v1[2] * v2[0] - v1[0] * v2[2],
            v1[0] * v2[1] - v1[1] * v2[0],
        ];
    }
    std::array::from_fn(|c| std::array::from_fn(|i| r[c][i]))
}

/// `Rᵀ A R` (upstream `reverseRotation`).
fn reverse_rotation<const D: usize>(a: &Matrix<D, D>, r: &Matrix<D, D>) -> Matrix<D, D> {
    r.tr_matmul(a).matmul(r)
}

/// `R A Rᵀ` (upstream `applyRotation`).
fn apply_rotation<const D: usize>(a: &Matrix<D, D>, r: &Matrix<D, D>) -> Matrix<D, D> {
    r.matmul(a).matmul(&r.transpose())
}

fn lp_inf<const D: usize>(a: &Matrix<D, D>) -> Real {
    a.as_flat()
        .iter()
        .fold(0.0, |m: Real, v| m.nan_max(v.abs()))
}

fn sub<const D: usize>(a: &Matrix<D, D>, b: &Matrix<D, D>) -> Matrix<D, D> {
    a.sub(b)
}

/// Frobenius norm of the difference of two column-major `n × n` slices.
fn diff_norm(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<Real>()
        .sqrt()
}

fn diff_inf(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .fold(0.0, |m: Real, (x, y)| m.nan_max((x - y).abs()))
}

/// Column-major `n × n` from rows.
fn col_major(rows: &[&[Real]]) -> Vec<Real> {
    let n = rows.len();
    (0..n * n).map(|l| rows[l % n][l / n]).collect()
}

// ---------------------------------------------------------------- elastic

fn elastic<const D: usize>(e: Real, nu: Option<Real>, rho: Real) -> MaterialElastic<D> {
    let model = empty_model(D);
    let mut m = MaterialElastic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
    set(&mut m, "E", e);
    if let Some(nu) = nu {
        set(&mut m, "nu", nu);
    }
    set(&mut m, "rho", rho);
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    m
}

#[test]
fn test_elastic_materials_elastic_1d() {
    let (e, rho) = (3., 2.);
    let m = elastic::<1>(e, None, rho);
    // ComputeStress
    let mut sigma = [[0.0]];
    m.stress_on_quad(&[[2.]], 2., &mut sigma);
    assert!((sigma[0][0] - (e * 2. + 2.)).abs() < 1e-14);
    // EnergyDensity
    let epot = MaterialElastic::<1>::energy_on_quad(&[[2.]], &[[2.]]);
    assert!((epot - 2.).abs() < 1e-14);
    // ComputeTangentModuli
    let mut t = [0.0];
    m.tangent_on_quad(&mut t);
    assert!((t[0] - e).abs() < 1e-14);
    // ComputeCelerity
    let c = m.celerity(Element::NULL).unwrap();
    assert!((c - (e / rho).sqrt()).abs() < 1e-14);
}

fn elastic_stress_test<const D: usize>(e: Real, nu: Real, rel: bool, tol: Real) {
    let m = elastic::<D>(e, Some(nu), 2.);
    let k = e / (3. * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let mut rng = Rng(0x5eed + D as u64);
    for _ in 0..ROTATIONS {
        let r = random_rotation::<D>(&mut rng);
        let grad_u: Matrix<D, D> = mat(&composed_strain(1.));
        let grad_u_rot = apply_rotation(&grad_u, &r);
        let mut sigma_rot = [[0.0; D]; D];
        m.stress_on_quad(&grad_u_rot, 0., &mut sigma_rot);
        let sigma = reverse_rotation(&sigma_rot, &r);
        let strain: Matrix<D, D> =
            std::array::from_fn(|c| std::array::from_fn(|i| 0.5 * (grad_u[c][i] + grad_u[i][c])));
        let tr: Real = (0..D).map(|i| strain[i][i]).sum();
        #[allow(clippy::cast_precision_loss)]
        let d = D as Real;
        // upstream: 2μ dev(ε) + (σ_th + d·K)·I, dev with tr/3, d·K = K·tr(ε)
        let expected: Matrix<D, D> = std::array::from_fn(|c| {
            std::array::from_fn(|i| {
                let id = if i == c { 1. } else { 0. };
                2. * mu * (strain[c][i] - 1. / 3. * tr * id) + (0. + d * k) * id
            })
        });
        let err = lp_inf(&sub(&sigma, &expected));
        let err = if rel { err / lp_inf(&expected) } else { err };
        assert!(err < tol, "dim {D}: stress error {err}");
    }
}

#[test]
fn test_elastic_materials_elastic_2d() {
    let (e, nu, rho) = (1., 0.3, 2.);
    elastic_stress_test::<2>(e, nu, true, 1e-13);
    let mut m = elastic::<2>(e, Some(nu), rho);
    // EnergyDensity
    let epot = MaterialElastic::<2>::energy_on_quad(
        &mat(&[[1., 2., 0.], [2., 4., 0.], [0.; 3]]),
        &mat(&[[1., 0., 0.], [0., 1., 0.], [0.; 3]]),
    );
    assert!((epot - 2.5).abs() < 1e-14);
    // ComputeTangentModuli: plane strain
    let mut t = [0.0; 9];
    m.tangent_on_quad(&mut t);
    let f = e / ((1. + nu) * (1. - 2. * nu));
    let sol: Vec<Real> = col_major(&[
        &[1. - nu, nu, 0.],
        &[nu, 1. - nu, 0.],
        &[0., 0., (1. - 2. * nu) / 2.],
    ])
    .into_iter()
    .map(|v| v * f)
    .collect();
    assert!(diff_inf(&t, &sol) < 1e-14);
    // plane stress (upstream sets plane_stress and calls updateInternalParameters)
    m.set_param("Plane_Stress", ParamValue::Bool(true)).unwrap();
    m.tangent_on_quad(&mut t);
    let f = e / (1. - nu * nu);
    let sol: Vec<Real> = col_major(&[&[1., nu, 0.], &[nu, 1., 0.], &[0., 0., (1. - nu) / 2.]])
        .into_iter()
        .map(|v| v * f)
        .collect();
    assert!(diff_inf(&t, &sol) < 1e-14);
    // ComputeCelerity
    let m = elastic::<2>(e, Some(nu), rho);
    let k = e / (3. * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let sol = ((k + 4. / 3. * mu) / rho).sqrt();
    assert!((m.push_wave_speed(Element::NULL).unwrap() - sol).abs() < 1e-14);
    assert!((m.celerity(Element::NULL).unwrap() - sol).abs() < 1e-14);
    assert!((m.shear_wave_speed(Element::NULL).unwrap() - (mu / rho).sqrt()).abs() < 1e-14);
}

#[test]
fn test_elastic_materials_elastic_3d() {
    let (e, nu, rho) = (1., 0.3, 2.);
    elastic_stress_test::<3>(e, nu, false, 1e-14);
    let m = elastic::<3>(e, Some(nu), rho);
    let epot = MaterialElastic::<3>::energy_on_quad(
        &mat(&[[1., 2., 3.], [2., 4., 5.], [3., 5., 6.]]),
        &mat(&[[1., 0., 0.], [0., 1., 0.], [0., 0., 1.]]),
    );
    assert!((epot - 5.5).abs() < 1e-14);
    let mut t = [0.0; 36];
    m.tangent_on_quad(&mut t);
    let a = (1. - 2. * nu) / 2.;
    let rows: [[Real; 6]; 6] = [
        [1. - nu, nu, nu, 0., 0., 0.],
        [nu, 1. - nu, nu, 0., 0., 0.],
        [nu, nu, 1. - nu, 0., 0., 0.],
        [0., 0., 0., a, 0., 0.],
        [0., 0., 0., 0., a, 0.],
        [0., 0., 0., 0., 0., a],
    ];
    let f = e / ((1. + nu) * (1. - 2. * nu));
    let sol: Vec<Real> = col_major(&rows.iter().map(|r| &r[..]).collect::<Vec<_>>())
        .into_iter()
        .map(|v| v * f)
        .collect();
    assert!(diff_norm(&t, &sol) < 1e-14);
    let k = e / (3. * (1. - 2. * nu));
    let mu = e / (2. * (1. + nu));
    let sol = ((k + 4. / 3. * mu) / rho).sqrt();
    assert!((m.push_wave_speed(Element::NULL).unwrap() - sol).abs() < 1e-14);
    assert!((m.celerity(Element::NULL).unwrap() - sol).abs() < 1e-14);
    assert!((m.shear_wave_speed(Element::NULL).unwrap() - (mu / rho).sqrt()).abs() < 1e-14);
}

// ------------------------------------------------------------ orthotropic

struct Ortho {
    e: [Real; 3],
    nu: [Real; 3], // nu12, nu13, nu23
    g: [Real; 3],  // G12, G13, G23
    rho: Real,
}

impl Ortho {
    /// Upstream's `C_expected` in the material frame (Voigt).
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

    fn build<const D: usize>(&self, axes: &Matrix<D, D>) -> MaterialElasticOrthotropic<D> {
        let model = empty_model(D);
        let mut m = MaterialElasticOrthotropic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
        *m.options_mut().unwrap() = LawOptions::upstream();
        // upstream writes `*dir_vecs[j]` directly (a `set` per axis would
        // run the axis check with the other axes still canonical)
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

fn identity<const D: usize>() -> Matrix<D, D> {
    std::array::from_fn(|c| std::array::from_fn(|i| if i == c { 1. } else { 0. }))
}

/// Upstream `testComputeStress` of the orthotropic/anisotropic laws:
/// material axes = columns of a random rotation, strain given in the
/// material frame, compared in the material frame with `C'·ε_voigt`.
fn rotated_stress_check<const D: usize, M: Material>(
    build: impl Fn(&Matrix<D, D>) -> M,
    stress: impl Fn(&M, &Matrix<D, D>, &mut Matrix<D, D>),
    c_material: &[Real],
    intensity: Real,
) {
    let v = D * (D + 1) / 2;
    let mut rng = Rng(0xa115 + D as u64);
    for _ in 0..ROTATIONS {
        let r = random_rotation::<D>(&mut rng);
        let m = build(&r);
        let grad_u: Matrix<D, D> = mat(&composed_strain(intensity));
        let grad_u_rot = reverse_rotation(&grad_u, &r);
        let mut sigma_rot = [[0.0; D]; D];
        stress(&m, &grad_u_rot, &mut sigma_rot);
        let sigma = apply_rotation(&sigma_rot, &r);
        let t = akantu_core::voigt::voigt_table(D).unwrap();
        let eps: Matrix<D, D> =
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
        assert!(err < 1e-13, "dim {D}: stress error {err}");
    }
}

fn first_eigenvalue(c: &[Real], v: usize, storage: Storage) -> Real {
    eig(&MatrixX::from_col_major(v, v, c.to_vec()).unwrap(), storage).unwrap()[0]
}

fn orthotropic_suite<const D: usize>(o: &Ortho) {
    let v = D * (D + 1) / 2;
    let c_exp = o.c_expected(D);
    rotated_stress_check::<D, _>(
        |r| o.build::<D>(r),
        |m, g, s| m.core.stress_on_quad(g, s).unwrap(),
        &c_exp,
        2.,
    );
    let m = o.build::<D>(&identity::<D>());
    // EnergyDensity
    let (sigma, eps) = if D == 2 {
        ([[1., 2., 0.], [2., 4., 0.], [0.; 3]], 2.5)
    } else {
        ([[1., 2., 3.], [2., 4., 5.], [3., 5., 6.]], 5.5)
    };
    let epot = akantu_solid::materials::anisotropic::AnisotropicCore::<D>::energy_on_quad(
        &mat(&sigma),
        &identity::<D>(),
    );
    assert!((epot - eps).abs() < 1e-14);
    // ComputeTangentModuli (axes = canonical basis)
    let mut t = vec![0.0; v * v];
    m.core.tangent_on_quad(&mut t);
    assert!(diff_norm(&t, &c_exp) < 1e-14);
    // ComputeCelerity (upstream: eig of the fixed-size C_expected)
    let expected = (first_eigenvalue(&c_exp, v, Storage::Fixed) / o.rho).sqrt();
    let c = m.celerity(Element::NULL).unwrap();
    assert!((c - expected).abs() < 1e-14, "{c} vs {expected}");
}

#[test]
fn test_elastic_materials_orthotropic_2d() {
    orthotropic_suite::<2>(&Ortho {
        e: [1., 2., 0.],
        nu: [0.1, 0., 0.],
        g: [2., 0., 0.],
        rho: 2.5,
    });
}

#[test]
fn test_elastic_materials_orthotropic_3d() {
    orthotropic_suite::<3>(&Ortho {
        e: [1., 2., 3.],
        nu: [0.1, 0.2, 0.3],
        g: [2., 3., 1.],
        rho: 2.3,
    });
}

// ------------------------------------------------------------ anisotropic

fn anisotropic<const D: usize>(
    c: &[&[Real]],
    axes: &Matrix<D, D>,
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
    let c_mat = col_major(c);
    rotated_stress_check::<D, _>(
        |r| anisotropic::<D>(c, r, rho),
        |m, g, s| m.core.stress_on_quad(g, s).unwrap(),
        &c_mat,
        intensity,
    );
    let mut rng = Rng(0xc0ffee + D as u64);
    let m = anisotropic::<D>(c, &random_rotation::<D>(&mut rng), rho);
    let (sigma, e) = if D == 2 {
        ([[1., 2., 0.], [2., 4., 0.], [0.; 3]], 2.5)
    } else {
        ([[1., 2., 3.], [2., 4., 5.], [3., 5., 6.]], 5.5)
    };
    let epot = akantu_solid::materials::anisotropic::AnisotropicCore::<D>::energy_on_quad(
        &mat(&sigma),
        &identity::<D>(),
    );
    assert!((epot - e).abs() < 1e-14);
    // ComputeTangentModuli: the tangent is the law's (rotated) C
    let mut t = vec![0.0; v * v];
    m.core.tangent_on_quad(&mut t);
    assert!(diff_norm(&t, m.core.c.as_slice()) < 1e-14);
    // ComputeCelerity: eig(C)[0] of the law's C
    let expected = (first_eigenvalue(m.core.c.as_slice(), v, Storage::Dynamic) / rho).sqrt();
    assert!((m.celerity(Element::NULL).unwrap() - expected).abs() < 1e-14);
}

#[test]
fn test_elastic_materials_anisotropic_2d() {
    anisotropic_suite::<2>(
        &[&[1.0, 0.3, 0.4], &[0.3, 2.0, 0.1], &[0.4, 0.1, 1.5]],
        2.7,
        1.,
    );
}

#[test]
fn test_elastic_materials_anisotropic_3d() {
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
