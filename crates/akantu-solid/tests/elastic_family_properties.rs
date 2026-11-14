// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent
// mathematical checks of the elastic-family laws ported from
// src/model/solid_mechanics/materials/{material_elastic*, material_thermal*,
// material_elastic_linear_anisotropic*, material_elastic_orthotropic*}
// (no upstream counterpart): isotropic limits, symmetry and
// positive-definiteness of tangents, energy derivative = stress, rotation
// covariance, plane-stress consistency, the UD-033 celerity bound, the
// UD-038 thermal eigenstrain and the finite-deformation Cauchy stress.

mod common;

use akantu_core::tensor::{Matrix, MatrixExt, MatrixX, SquareMatrixExt};
use akantu_core::{Element, Real};
use akantu_linalg::dense::{Storage, eig, eigh};
use akantu_model::constitutive_law::{LawPhase, LawSpec};
use akantu_parser::{ParamValue, ParameterRegistry};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::material_tensor::{grad_u_to_f, grad_u_to_green};
use akantu_solid::materials::anisotropic::{
    AnisotropicCore, CelerityMode, MaterialElasticLinearAnisotropic,
};
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::materials::orthotropic::MaterialElasticOrthotropic;
use akantu_validation::reduce::NanMinMax;
use common::{NG, SimplexModel, simplex};

const U: Real = Real::EPSILON / 2.;

fn spec(dim: usize) -> LawSpec {
    LawSpec {
        spatial_dimension: dim,
        option: String::new(),
        id: "m".into(),
    }
}

fn set(m: &mut impl ParameterRegistry, name: &str, v: Real) {
    m.set_param(name, ParamValue::Real(v)).unwrap();
}

struct Rng(u64);
impl Rng {
    fn next(&mut self) -> Real {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)]
        let v = (z >> 11) as Real / (1u64 << 53) as Real;
        v
    }
    fn sym(&mut self) -> Real {
        2. * self.next() - 1.
    }
}

/// Right-handed orthonormal matrix (columns = axes).
fn rotation<const D: usize>(rng: &mut Rng) -> Matrix<D, D> {
    let mut m = [[0.0; D]; D];
    for c in 0..D {
        loop {
            let mut v = [0.0; D];
            for x in &mut v {
                *x = rng.sym();
            }
            for prev in m.iter().take(c) {
                let d: Real = (0..D).map(|i| v[i] * prev[i]).sum();
                (0..D).for_each(|i| v[i] -= d * prev[i]);
            }
            let n = v.iter().map(|x| x * x).sum::<Real>().sqrt();
            if n > 1e-3 {
                m[c] = v.map(|x| x / n);
                break;
            }
        }
    }
    if D == 3 {
        let (a, b) = (m[0], m[1]);
        m[2] = std::array::from_fn(|i| {
            let c = [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ];
            c[i]
        });
    }
    if D == 2 && m[0][0] * m[1][1] - m[0][1] * m[1][0] < 0. {
        m[1] = m[1].map(|x| -x);
    }
    m
}

fn random_grad<const D: usize>(rng: &mut Rng, scale: Real) -> Matrix<D, D> {
    std::array::from_fn(|_| std::array::from_fn(|_| scale * rng.sym()))
}

fn norm_inf(v: &[Real]) -> Real {
    v.iter().fold(0.0, |m: Real, x| m.nan_max(x.abs()))
}

fn empty_model(dim: usize) -> SimplexModel {
    SimplexModel::new(dim, &[], &[])
}

fn elastic<const D: usize>(e: Real, nu: Real, plane_stress: bool) -> MaterialElastic<D> {
    let model = empty_model(D);
    let mut m = MaterialElastic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
    set(&mut m, "E", e);
    set(&mut m, "nu", nu);
    set(&mut m, "rho", 7800.);
    if plane_stress {
        m.set_param("Plane_Stress", ParamValue::Bool(true)).unwrap();
    }
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    m
}

fn elastic_tangent<const D: usize>(m: &MaterialElastic<D>) -> Vec<Real> {
    let v = D * (D + 1) / 2;
    let mut t = vec![0.0; v * v];
    m.tangent_on_quad(&mut t);
    t
}

fn ortho<const D: usize>(
    e: [Real; 3],
    nu: [Real; 3],
    g: [Real; 3],
    axes: &Matrix<D, D>,
) -> MaterialElasticOrthotropic<D> {
    let model = empty_model(D);
    let mut m = MaterialElasticOrthotropic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
    m.core.dir_vecs = *axes;
    let names2 = [
        ("E1", e[0]),
        ("E2", e[1]),
        ("nu12", nu[0]),
        ("G12", g[0]),
        ("rho", 2000.),
    ];
    for (n, v) in names2 {
        set(&mut m, n, v);
    }
    if D == 3 {
        for (n, v) in [
            ("E3", e[2]),
            ("nu13", nu[1]),
            ("nu23", nu[2]),
            ("G13", g[1]),
            ("G23", g[2]),
        ] {
            set(&mut m, n, v);
        }
    }
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    m
}

fn aniso<const D: usize>(
    c: &[Vec<Real>],
    axes: &Matrix<D, D>,
    rho: Real,
) -> MaterialElasticLinearAnisotropic<D> {
    let model = empty_model(D);
    let mut m = MaterialElasticLinearAnisotropic::<D>::new(&spec(D), Some(&model.mesh)).unwrap();
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

fn identity<const D: usize>() -> Matrix<D, D> {
    <Matrix<D, D> as SquareMatrixExt<D>>::identity()
}

/// Random symmetric positive-definite Voigt stiffness `B Bᵀ + n·s·I`.
fn random_spd(rng: &mut Rng, v: usize, scale: Real) -> Vec<Vec<Real>> {
    let b: Vec<Vec<Real>> = (0..v)
        .map(|_| (0..v).map(|_| rng.sym()).collect())
        .collect();
    (0..v)
        .map(|i| {
            (0..v)
                .map(|j| {
                    let s: Real = (0..v).map(|k| b[i][k] * b[j][k]).sum();
                    scale * (s + if i == j { 0.5 } else { 0. })
                })
                .collect()
        })
        .collect()
}

// ------------------------------------------------------- isotropic limits

/// Orthotropic constants of an isotropic solid give the isotropic tangent:
/// 3-D = `elastic` (plane strain/3-D Lamé), 2-D = `elastic` with
/// `Plane_Stress` (upstream's 2-D orthotropic constants are plane-stress
/// ones). Bound: both are closed forms of < 30 flops from (E, ν): relative
/// difference ≤ 64 u of the largest entry.
#[test]
fn orthotropic_isotropic_limit_equals_elastic() {
    let (e, nu) = (2.1e11, 0.3);
    let g = e / (2. * (1. + nu));
    let o3 = ortho::<3>([e; 3], [nu; 3], [g; 3], &identity());
    let el3 = elastic::<3>(e, nu, false);
    let (a, b) = (o3.core.c.as_slice().to_vec(), elastic_tangent(&el3));
    let max = norm_inf(&b);
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() <= 64. * U * max, "{x} vs {y}");
    }
    let o2 = ortho::<2>([e, e, 0.], [nu, 0., 0.], [g, 0., 0.], &identity());
    let el2 = elastic::<2>(e, nu, true);
    let (a, b) = (o2.core.c.as_slice().to_vec(), elastic_tangent(&el2));
    let max = norm_inf(&b);
    for (x, y) in a.iter().zip(&b) {
        assert!((x - y).abs() <= 64. * U * max, "{x} vs {y}");
    }
}

/// An isotropic stiffness is invariant under the axis rotation of
/// `elastic_anisotropic`. Bound: each rotated entry is a sum of 81 (3-D) /
/// 16 (2-D) products of a unit-norm rotation and `C'`, γ_81·Σ|terms| with
/// Σ|terms| ≤ 9·max|C'|: ≤ 81·9 u ≈ 8e-14 relative; checked at 1e-12.
#[test]
fn anisotropic_isotropic_limit_is_rotation_invariant() {
    let mut rng = Rng(11);
    let el3 = elastic::<3>(70e9, 0.33, false);
    let t3 = elastic_tangent(&el3);
    let c3: Vec<Vec<Real>> = (0..6)
        .map(|i| (0..6).map(|j| t3[i + j * 6]).collect())
        .collect();
    let el2 = elastic::<2>(70e9, 0.33, false);
    let t2 = elastic_tangent(&el2);
    let c2: Vec<Vec<Real>> = (0..3)
        .map(|i| (0..3).map(|j| t2[i + j * 3]).collect())
        .collect();
    for _ in 0..50 {
        let m = aniso::<3>(&c3, &rotation::<3>(&mut rng), 2700.);
        let max = norm_inf(&t3);
        for (x, y) in m.core.c.as_slice().iter().zip(&t3) {
            assert!((x - y).abs() <= 1e-12 * max);
        }
        let m = aniso::<2>(&c2, &rotation::<2>(&mut rng), 2700.);
        let max = norm_inf(&t2);
        for (x, y) in m.core.c.as_slice().iter().zip(&t2) {
            assert!((x - y).abs() <= 1e-12 * max);
        }
    }
}

// ------------------------------------------------- symmetry and SPD tangents

fn assert_spd(t: &[Real], v: usize, what: &str) {
    let max = norm_inf(t);
    for i in 0..v {
        for j in 0..v {
            assert!(
                (t[i + j * v] - t[j + i * v]).abs() <= 1e-12 * max,
                "{what}: tangent not symmetric"
            );
        }
    }
    let ev = eigh(
        &MatrixX::from_col_major(v, v, t.to_vec()).unwrap(),
        Storage::Dynamic,
    )
    .unwrap();
    assert!(ev[0] > 0., "{what}: tangent not positive definite ({ev:?})");
}

#[test]
fn tangents_are_symmetric_positive_definite() {
    let mut rng = Rng(7);
    assert_spd(
        &elastic_tangent(&elastic::<1>(1e9, 0.3, false)),
        1,
        "elastic 1d",
    );
    assert_spd(
        &elastic_tangent(&elastic::<2>(1e9, 0.3, false)),
        3,
        "elastic 2d",
    );
    assert_spd(
        &elastic_tangent(&elastic::<2>(1e9, 0.3, true)),
        3,
        "elastic 2d ps",
    );
    assert_spd(
        &elastic_tangent(&elastic::<3>(1e9, 0.3, false)),
        6,
        "elastic 3d",
    );
    for _ in 0..20 {
        let o = ortho::<3>(
            [1e11, 2e11, 3e11],
            [0.1, 0.2, 0.3],
            [2e10, 3e10, 1e10],
            &rotation::<3>(&mut rng),
        );
        assert_spd(o.core.c.as_slice(), 6, "orthotropic 3d");
        let o = ortho::<2>(
            [1.5e11, 1e10, 0.],
            [0.3, 0., 0.],
            [5e9, 0., 0.],
            &rotation::<2>(&mut rng),
        );
        assert_spd(o.core.c.as_slice(), 3, "orthotropic 2d");
        let c = random_spd(&mut rng, 6, 1e10);
        let a = aniso::<3>(&c, &rotation::<3>(&mut rng), 1000.);
        assert_spd(a.core.c.as_slice(), 6, "anisotropic 3d");
        let c = random_spd(&mut rng, 3, 1e10);
        let a = aniso::<2>(&c, &rotation::<2>(&mut rng), 1000.);
        assert_spd(a.core.c.as_slice(), 3, "anisotropic 2d");
    }
}

// --------------------------------------------- energy derivative = stress

/// `∂W/∂(∇u)_ij` by central differences of `W(∇u) = ½ σ(∇u) : ∇u`. For the
/// (quadratic) linear laws central differences are exact up to round-off
/// `~ u·W/h`; with `h = 1e-4·|∇u|` that is ≲ 1e-11 of `|σ|`; checked at
/// 1e-7 relative to `max|σ|`.
fn check_energy_derivative<const D: usize>(
    stress: &dyn Fn(&Matrix<D, D>) -> Matrix<D, D>,
    energy: &dyn Fn(&Matrix<D, D>, &Matrix<D, D>) -> Real,
    first_pk: &dyn Fn(&Matrix<D, D>, &Matrix<D, D>) -> Matrix<D, D>,
    g: &Matrix<D, D>,
    what: &str,
) {
    let s = stress(g);
    let p = first_pk(g, &s);
    let scale = norm_inf(g.as_flat());
    let h = 1e-4 * scale;
    let pmax = norm_inf(p.as_flat());
    for c in 0..D {
        for r in 0..D {
            let mut gp = *g;
            gp[c][r] += h;
            let mut gm = *g;
            gm[c][r] -= h;
            let wp = energy(&stress(&gp), &gp);
            let wm = energy(&stress(&gm), &gm);
            let d = (wp - wm) / (2. * h);
            assert!(
                (d - p[c][r]).abs() <= 1e-7 * pmax,
                "{what}: dW/dgrad_u({r},{c}) = {d}, stress {}",
                p[c][r]
            );
        }
    }
}

#[test]
fn energy_derivative_equals_stress() {
    let mut rng = Rng(3);
    // small strain: P = σ (symmetric), W = ½ σ : ∇u
    let small = |_: &Matrix<3, 3>, s: &Matrix<3, 3>| *s;
    let el = elastic::<3>(2e11, 0.3, false);
    let ortho3 = ortho::<3>(
        [1e11, 2e11, 3e11],
        [0.1, 0.2, 0.3],
        [2e10, 3e10, 1e10],
        &rotation::<3>(&mut rng),
    );
    let c = random_spd(&mut rng, 6, 1e10);
    let aniso3 = aniso::<3>(&c, &rotation::<3>(&mut rng), 1000.);
    for _ in 0..10 {
        let g = random_grad::<3>(&mut rng, 1e-3);
        check_energy_derivative::<3>(
            &|g| {
                let mut s = [[0.0; 3]; 3];
                el.stress_on_quad(g, 0., &mut s);
                s
            },
            &|s, g| MaterialElastic::<3>::energy_on_quad(s, g),
            &small,
            &g,
            "elastic 3d",
        );
        for (core, what) in [
            (&ortho3.core, "orthotropic 3d"),
            (&aniso3.core, "anisotropic 3d"),
        ] {
            check_energy_derivative::<3>(
                &|g| {
                    let mut s = [[0.0; 3]; 3];
                    core.stress_on_quad(g, &mut s).unwrap();
                    s
                },
                &|s, g| AnisotropicCore::<3>::energy_on_quad(s, g),
                &small,
                &g,
                what,
            );
        }
        // 2-D plane stress
        let ps = elastic::<2>(2e11, 0.3, true);
        let g2 = random_grad::<2>(&mut rng, 1e-3);
        check_energy_derivative::<2>(
            &|g| {
                let mut s = [[0.0; 2]; 2];
                ps.stress_on_quad(g, 0., &mut s);
                s
            },
            &|s, g| MaterialElastic::<2>::energy_on_quad(s, g),
            &|_, s| *s,
            &g2,
            "elastic 2d plane stress",
        );
        // St-Venant–Kirchhoff: W = ½ S(E) : E, ∂W/∂∇u = P = F·S
        let g = random_grad::<3>(&mut rng, 5e-2);
        check_energy_derivative::<3>(
            &|g| {
                let mut s = [[0.0; 3]; 3];
                el.stress_on_quad(&grad_u_to_green(g), 0., &mut s);
                s
            },
            &|s, g| MaterialElastic::<3>::energy_on_quad(s, &grad_u_to_green(g)),
            &|g, s| grad_u_to_f(g).matmul(s),
            &g,
            "St-Venant-Kirchhoff 3d",
        );
    }
}

// ------------------------------------------------------ rotation covariance

/// `σ(R ∇u Rᵀ) = R σ(∇u) Rᵀ` for the isotropic law. Bound: two products of
/// 3-term sums on each side plus the kernel (< 10 roundings): ≤ 64 u of
/// `max|σ|`.
#[test]
fn isotropic_stress_is_rotation_covariant() {
    let mut rng = Rng(5);
    let el = elastic::<3>(2e11, 0.3, false);
    for _ in 0..50 {
        let r = rotation::<3>(&mut rng);
        let g = random_grad::<3>(&mut rng, 1e-3);
        let mut s = [[0.0; 3]; 3];
        el.stress_on_quad(&g, 0., &mut s);
        let gr = r.matmul(&g).matmul(&r.transpose());
        let mut sr = [[0.0; 3]; 3];
        el.stress_on_quad(&gr, 0., &mut sr);
        let expected = r.matmul(&s).matmul(&r.transpose());
        let max = norm_inf(s.as_flat());
        for (a, b) in sr.as_flat().iter().zip(expected.as_flat()) {
            assert!((a - b).abs() <= 64. * U * max * 4., "{a} vs {b}");
        }
    }
}

/// Rotating the material axes rotates the stress with the strain. With the
/// axes `n_j` = columns of `Q`: by default the axes are the material
/// directions (`σ_Q(Q ∇u Qᵀ) = Q σ_I(∇u) Qᵀ`); upstream rotates with `Qᵀ`
/// (`σ_Q(Qᵀ ∇u Q) = Qᵀ σ_I(∇u) Q`, UD-104, see
/// `LawOptions::material_axes_as_directions`). Bound: the
/// rotated stiffness carries the 81-term rotation error (see
/// `anisotropic_isotropic_limit_is_rotation_invariant`), the strain and
/// stress rotations add 3-term sums: checked at 1e-12 of `max|σ|`.
#[test]
fn anisotropic_stress_is_frame_covariant() {
    let mut rng = Rng(9);
    let c = random_spd(&mut rng, 6, 1e10);
    let (e, nu, g) = ([1e11, 2e11, 3e11], [0.1, 0.2, 0.3], [2e10, 3e10, 1e10]);
    let base = aniso::<3>(&c, &identity(), 1000.);
    let obase = ortho::<3>(e, nu, g, &identity());
    for as_directions in [false, true] {
        for _ in 0..30 {
            let q = rotation::<3>(&mut rng);
            let mut rotated = aniso::<3>(&c, &q, 1000.);
            let mut orot = ortho::<3>(e, nu, g, &q);
            for law in [&mut rotated as &mut dyn Material, &mut orot] {
                law.options_mut().unwrap().material_axes_as_directions = as_directions;
                law.update_internal_parameters().unwrap();
            }
            // the frame rotation that maps material to global components
            let r = if as_directions { q } else { q.transpose() };
            let gm = random_grad::<3>(&mut rng, 1e-3);
            let gq = r.matmul(&gm).matmul(&r.transpose());
            for (b, rc) in [(&base.core, &rotated.core), (&obase.core, &orot.core)] {
                let mut s = [[0.0; 3]; 3];
                b.stress_on_quad(&gm, &mut s).unwrap();
                let mut sq = [[0.0; 3]; 3];
                rc.stress_on_quad(&gq, &mut sq).unwrap();
                let expected = r.matmul(&s).matmul(&r.transpose());
                let max = norm_inf(s.as_flat());
                for (a, x) in sq.as_flat().iter().zip(expected.as_flat()) {
                    assert!((a - x).abs() <= 1e-12 * max, "{as_directions}: {a} vs {x}");
                }
            }
        }
    }
}

/// UD-104 in one picture: orthotropic E1 ≫ E2 with `n1 = (0.8, 0.6)`: the
/// default is stiff along `n1`, upstream along `(0.8, −0.6)`.
#[test]
fn material_axes_convention_directional_stiffness() {
    let young = |m: &MaterialElasticOrthotropic<2>, d: [Real; 2]| {
        let s = m.core.c.inverse().unwrap();
        let sv = [d[0] * d[0], d[1] * d[1], d[0] * d[1]];
        let mut e = 0.0;
        for i in 0..3 {
            for j in 0..3 {
                e += sv[i] * s.at(i, j) * sv[j];
            }
        }
        1. / e
    };
    let axes = [[0.8, 0.6], [-0.6, 0.8]];
    let mut m = ortho::<2>([1.5e11, 1e10, 0.], [0.3, 0., 0.], [5e9, 0., 0.], &axes);
    assert!(
        (young(&m, [0.8, 0.6]) - 1.5e11).abs() <= 1e-9 * 1.5e11,
        "axes as directions (default)"
    );
    m.set_options(LawOptions::upstream()).unwrap();
    assert!(
        (young(&m, [0.8, -0.6]) - 1.5e11).abs() <= 1e-9 * 1.5e11,
        "upstream convention"
    );
}

/// Stress = tangent · engineering strain (Voigt), for every law.
#[test]
fn stress_equals_tangent_times_engineering_strain() {
    fn check<const D: usize>(t: &[Real], s: &Matrix<D, D>, g: &Matrix<D, D>) {
        let v = D * (D + 1) / 2;
        let tab = akantu_core::voigt::voigt_table(D).unwrap();
        let ev: Vec<Real> = (0..v)
            .map(|k| {
                let (i, j) = tab.vec(k);
                tab.factor(k) * 0.5 * (g[j][i] + g[i][j])
            })
            .collect();
        let max = norm_inf(s.as_flat());
        for big in 0..v {
            let (i, j) = tab.vec(big);
            let sv: Real = (0..v).map(|k| t[big + k * v] * ev[k]).sum();
            assert!((sv - s[j][i]).abs() <= 64. * U * max.max(1e-300) * 4.);
        }
    }
    let mut rng = Rng(13);
    for _ in 0..20 {
        let el = elastic::<3>(2e11, 0.3, false);
        let g = random_grad::<3>(&mut rng, 1e-3);
        let mut s = [[0.0; 3]; 3];
        el.stress_on_quad(&g, 0., &mut s);
        check::<3>(&elastic_tangent(&el), &s, &g);
        let ps = elastic::<2>(2e11, 0.3, true);
        let g2 = random_grad::<2>(&mut rng, 1e-3);
        let mut s2 = [[0.0; 2]; 2];
        ps.stress_on_quad(&g2, 0., &mut s2);
        check::<2>(&elastic_tangent(&ps), &s2, &g2);
        let c = random_spd(&mut rng, 6, 1e10);
        let a = aniso::<3>(&c, &rotation::<3>(&mut rng), 1000.);
        let mut sa = [[0.0; 3]; 3];
        a.core.stress_on_quad(&g, &mut sa).unwrap();
        check::<3>(a.core.c.as_slice(), &sa, &g);
    }
}

/// 2-D plane stress = 3-D state with the free out-of-plane strain
/// `ε_zz = −ν/(1 − ν)(ε_xx + ε_yy)`, whose `σ_zz` vanishes.
#[test]
fn plane_stress_matches_3d_with_free_thickness() {
    let mut rng = Rng(17);
    let (e, nu) = (2.1e11, 0.3);
    let ps = elastic::<2>(e, nu, true);
    let el3 = elastic::<3>(e, nu, false);
    for _ in 0..20 {
        let g2 = random_grad::<2>(&mut rng, 1e-3);
        let mut s2 = [[0.0; 2]; 2];
        ps.stress_on_quad(&g2, 0., &mut s2);
        let ezz = -nu / (1. - nu) * (g2[0][0] + g2[1][1]);
        let g3: Matrix<3, 3> = [
            [g2[0][0], g2[0][1], 0.],
            [g2[1][0], g2[1][1], 0.],
            [0., 0., ezz],
        ];
        let mut s3 = [[0.0; 3]; 3];
        el3.stress_on_quad(&g3, 0., &mut s3);
        let max = norm_inf(s2.as_flat());
        assert!(s3[2][2].abs() <= 1e-12 * max, "sigma_zz = {}", s3[2][2]);
        for c in 0..2 {
            for r in 0..2 {
                assert!((s3[c][r] - s2[c][r]).abs() <= 1e-12 * max);
            }
        }
    }
}

// ---------------------------------------------------------------- UD-033

/// Largest eigenvalue of the acoustic tensor `Q_ik(n) = C_ijkl n_j n_l`
/// built from the Voigt stiffness (engineering shear convention).
fn acoustic_max(c: &MatrixX, dim: usize, n: &[Real]) -> Real {
    let t = akantu_core::voigt::voigt_table(dim).unwrap();
    // full C_ijkl from the Voigt matrix (mat(i, j) for i == j or the
    // symmetric Voigt index otherwise)
    let voigt = |i: usize, j: usize| {
        let k = t.mat(i, j);
        if k < dim * (dim + 1) / 2 {
            k
        } else {
            t.mat(j, i)
        }
    };
    let mut q = MatrixX::zeros(dim, dim);
    for i in 0..dim {
        for k in 0..dim {
            let mut s = 0.0;
            for j in 0..dim {
                for l in 0..dim {
                    s += c.at(voigt(i, j), voigt(k, l)) * n[j] * n[l];
                }
            }
            *q.at_mut(i, k) = s;
        }
    }
    *eigh(&q, Storage::Dynamic).unwrap().last().unwrap()
}

#[test]
fn ud033_kelvin_bound_is_a_conservative_wave_speed() {
    let mut rng = Rng(23);
    // (a) bound of every sampled acoustic speed, orthotropic and anisotropic
    for _ in 0..10 {
        let c = random_spd(&mut rng, 6, 1e10);
        let a = aniso::<3>(&c, &rotation::<3>(&mut rng), 1000.);
        let o = ortho::<3>(
            [1e11, 2e11, 3e11],
            [0.1, 0.2, 0.3],
            [2e10, 3e10, 1e10],
            &rotation::<3>(&mut rng),
        );
        for core in [&a.core, &o.core] {
            let bound = core.kelvin_celerity(1000.).unwrap();
            for _ in 0..500 {
                let mut n = [rng.sym(), rng.sym(), rng.sym()];
                let l = n.iter().map(|x| x * x).sum::<Real>().sqrt();
                n.iter_mut().for_each(|x| *x /= l);
                let speed = (acoustic_max(&core.c, 3, &n) / 1000.).sqrt();
                assert!(speed <= bound * (1. + 1e-12), "{speed} > {bound}");
            }
        }
    }
    // (b) isotropic: √((3λ + 2μ)/ρ) ≥ P-wave speed √((λ + 2μ)/ρ)
    let el = elastic::<3>(2e11, 0.3, false);
    let t = elastic_tangent(&el);
    let c: Vec<Vec<Real>> = (0..6)
        .map(|i| (0..6).map(|j| t[i + j * 6]).collect())
        .collect();
    let mut a = aniso::<3>(&c, &identity(), 7800.);
    let kelvin = a.celerity(Element::NULL).unwrap();
    let (l, mu) = (el.lambda(), el.mu());
    assert!((kelvin - ((3. * l + 2. * mu) / 7800.).sqrt()).abs() <= 1e-12 * kelvin);
    assert!(kelvin >= el.push_wave_speed(Element::NULL).unwrap());
    // (d) compatibility mode: √(eig(C)[0]/ρ), Eigen's unsorted order
    a.options_mut().unwrap().celerity = CelerityMode::UpstreamFirstEigenvalue;
    let e0 = eig(&a.core.c, Storage::Dynamic).unwrap()[0];
    assert_eq!(a.celerity(Element::NULL).unwrap(), (e0 / 7800.).sqrt());
    // (c) uncoupled orthotropy C_ii = 1,
    // C44 = C55 = 0.1, C66 = 10, ρ = 1: the plain Voigt maximum (10) is
    // below the acoustic Rayleigh quotient 10.5 at n = a = (1,1,0)/√2; the
    // Kelvin bound (λ_max = 2·C66 = 20) is above it.
    let mut c = vec![vec![0.0; 6]; 6];
    for (i, row) in c.iter_mut().enumerate().take(3) {
        row[i] = 1.;
    }
    c[3][3] = 0.1;
    c[4][4] = 0.1;
    c[5][5] = 10.;
    let m = aniso::<3>(&c, &identity(), 1.);
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let q = acoustic_max(&m.core.c, 3, &[s, s, 0.]);
    assert!(q >= 10.5 - 1e-12);
    let voigt_max = *eigh(&m.core.c, Storage::Dynamic).unwrap().last().unwrap();
    assert!(voigt_max < q, "the plain Voigt maximum is not a bound");
    assert!(m.core.kelvin_celerity(1.).unwrap() >= q.sqrt());
    // (e) 1-D: all conventions give √(C11/ρ)
    let mut a1 = aniso::<1>(&[vec![105.]], &[[-1.]], 1.6);
    let k = a1.celerity(Element::NULL).unwrap();
    a1.options_mut().unwrap().celerity = CelerityMode::UpstreamFirstEigenvalue;
    assert_eq!(k, a1.celerity(Element::NULL).unwrap());
    assert_eq!(k, (105.0_f64 / 1.6).sqrt());
}

// ---------------------------------------------------------------- UD-038

/// Run the thermal pre-step of a one-element law and return
/// `(epsilon_th, sigma_th, σ)` for the gradient `g`.
fn thermal_run<const D: usize>(
    options: LawOptions,
    plane_stress: bool,
    g: &Matrix<D, D>,
) -> (Real, Real, Vec<Real>) {
    let nodes: Vec<Vec<Real>> = match D {
        1 => vec![vec![0.], vec![1.]],
        2 => vec![vec![0., 0.], vec![1., 0.], vec![0., 1.]],
        _ => vec![
            vec![0., 0., 0.],
            vec![1., 0., 0.],
            vec![0., 1., 0.],
            vec![0., 0., 1.],
        ],
    };
    let conn: Vec<akantu_core::Idx> = (0..=D as akantu_core::Idx).collect();
    let model = SimplexModel::new(D, &nodes, &[conn]);
    let dat = format!(
        "material elastic [\n name = s\n E = 2e11\n nu = 0.3\n alpha = 1.2e-5\n delta_T = 80\n{}]\n",
        if plane_stress {
            " Plane_Stress = true\n"
        } else {
            ""
        }
    );
    let mut h = common::materials_from_dat(&model, dat.as_bytes()).unwrap();
    let law = h.law_mut(0).unwrap();
    *law.options_mut().unwrap() = options;
    let ty = simplex(D);
    law.base_mut()
        .grad_u
        .get_mut(ty, NG)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(g.as_flat());
    law.compute_stress(&model.ctx(LawPhase::Stress), ty, NG)
        .unwrap();
    let mut eps = None;
    let mut sig = None;
    law.for_each_internal(&mut |n, f| {
        if let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f {
            let v = f.get(ty, NG).unwrap().as_slice()[0];
            match n {
                "epsilon_th" => eps = Some(v),
                "sigma_th" => sig = Some(v),
                _ => {}
            }
        }
    });
    let s = law.base().stress.get(ty, NG).unwrap().as_slice().to_vec();
    (eps.unwrap(), sig.unwrap(), s)
}

#[test]
fn ud038_thermal_eigenstrain_and_free_expansion() {
    let (alpha, dt, nu) = (1.2e-5, 80., 0.3);
    let free3: Matrix<3, 3> =
        std::array::from_fn(|c| std::array::from_fn(|r| if r == c { alpha * dt } else { 0. }));
    let (eps, sig, s) = thermal_run::<3>(LawOptions::default(), false, &free3);
    assert_eq!(eps, dt * alpha, "corrected 3-D eigenstrain");
    // free expansion by the corrected eigenstrain: zero stress
    assert!(norm_inf(&s) <= 1e-9 * sig.abs(), "{s:?}");
    let (eps_up, sig_up, s_up) = thermal_run::<3>(LawOptions::upstream(), false, &free3);
    assert_eq!(eps_up, dt * alpha * (1. + nu));
    assert_eq!(
        (sig_up, s_up),
        (sig, s),
        "the stress does not depend on the convention"
    );
    // fully constrained: σ = σ_th I = −3KαΔT I
    let (_, sig, s) = thermal_run::<3>(LawOptions::default(), false, &[[0.; 3]; 3]);
    let k = 2e11 / (3. * (1. - 2. * nu));
    assert!((sig + 3. * k * alpha * dt).abs() <= 1e-12 * sig.abs());
    assert_eq!(s[0], sig);
    // 2-D plane strain keeps the reduced (1 + ν) eigenstrain in both modes;
    // plane stress uses αΔT
    for o in [LawOptions::default(), LawOptions::upstream()] {
        let (eps, _, _) = thermal_run::<2>(o, false, &[[0.; 2]; 2]);
        assert_eq!(eps, dt * alpha * (1. + nu));
        let (eps, _, _) = thermal_run::<2>(o, true, &[[0.; 2]; 2]);
        assert_eq!(eps, dt * alpha);
        let (eps, _, _) = thermal_run::<1>(o, false, &[[0.]]);
        assert_eq!(eps, dt * alpha);
    }
}

// ------------------------------------------------- finite deformation

/// `onDump` in finite deformation: `σ = J⁻¹ F S Fᵀ` with `S` the PK2 stress
/// computed by the law (St-Venant–Kirchhoff).
#[test]
fn finite_deformation_cauchy_stress() {
    let nodes = vec![
        vec![0., 0., 0.],
        vec![1., 0., 0.],
        vec![0., 1., 0.],
        vec![0., 0., 1.],
    ];
    let model = SimplexModel::new(3, &nodes, &[vec![0, 1, 2, 3]]);
    let dat =
        b"material elastic [\n name = s\n E = 1e6\n nu = 0.3\n finite_deformation = true\n]\n";
    let mut h = common::materials_from_dat(&model, dat).unwrap();
    let law = h.law_mut(0).unwrap();
    let ty = simplex(3);
    let mut rng = Rng(29);
    let g = random_grad::<3>(&mut rng, 0.1);
    law.base_mut()
        .grad_u
        .get_mut(ty, NG)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(g.as_flat());
    law.compute_stress(&model.ctx(LawPhase::Stress), ty, NG)
        .unwrap();
    assert!(
        law.base()
            .stress
            .get(ty, NG)
            .unwrap()
            .as_slice()
            .iter()
            .all(|&v| v == 0.),
        "stress untouched before onDump"
    );
    let s = law
        .base()
        .piola_kirchhoff_2
        .as_ref()
        .unwrap()
        .get(ty, NG)
        .unwrap()
        .as_slice()
        .to_vec();
    law.on_dump().unwrap();
    let sigma = law.base().stress.get(ty, NG).unwrap().as_slice().to_vec();
    let f = grad_u_to_f(&g);
    let j = f.det();
    let s: Matrix<3, 3> = *akantu_core::tensor::as_matrix(&s).unwrap();
    let expected = f.matmul(&s).matmul(&f.transpose()).scale(1. / j);
    let max = norm_inf(expected.as_flat());
    for (a, b) in sigma.iter().zip(expected.as_flat()) {
        assert!((a - b).abs() <= 1e-13 * max);
    }
}
