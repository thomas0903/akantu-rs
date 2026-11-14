// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// (`mat01_*`…`mat04_*`, `mat06_*`) on the elastic
// family: src/model/solid_mechanics/materials/material_elastic_orthotropic.cc,
// material_elastic_linear_anisotropic.cc, material_elastic_inline_impl.hh,
// material_thermal.hh; src/model/common/constitutive_laws/
// constitutive_laws_handler_tmpl.hh. The fifth case is tested in
// `akantu-model` (`internal_field::tests`).

//! Each test reproduces a failing case, checks the corrected default
//! against an analytical or finite-difference reference, and checks that the
//! compatibility switch ([`LawOptions::upstream`]) still reproduces the
//! reference behaviour (UD-104, UD-105, UD-109).

mod common;

use akantu_core::tensor::{Matrix, MatrixX};
use akantu_core::{Array, Element, ElementType, Error, Idx, Real};
use akantu_linalg::dense::{Storage, eig, eigh};
use akantu_model::ConstitutiveLawsHandler;
use akantu_model::constitutive_law::{
    ConstitutiveLawSelector, LawPhase, LawSpec, SelectorChain, SelectorContext,
};
use akantu_parser::{ParamValue, ParameterRegistry, ParserType};
use akantu_solid::Material;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::anisotropic::MaterialElasticLinearAnisotropic;
use akantu_solid::materials::elastic::MaterialElastic;
use akantu_solid::materials::orthotropic::MaterialElasticOrthotropic;
use akantu_validation::reduce::NanMinMax;
use common::{NG, SimplexModel, materials_from_dat, materials_from_dat_with, simplex};

/// One unit simplex of dimension `dim`.
fn one_simplex(dim: usize) -> SimplexModel {
    let nodes: Vec<Vec<Real>> = (0..=dim)
        .map(|k| (0..dim).map(|c| if c + 1 == k { 1. } else { 0. }).collect())
        .collect();
    let conn: Vec<Idx> = (0..=dim).map(|k| Idx::try_from(k).unwrap()).collect();
    SimplexModel::new(dim, &nodes, &[conn])
}

fn spec(dim: usize, law_type: &str) -> LawSpec {
    LawSpec {
        spatial_dimension: dim,
        option: String::new(),
        id: format!("m:0:{law_type}"),
    }
}

fn real(m: &dyn Material, name: &str) -> Real {
    match m.get_param(name).unwrap() {
        ParamValue::Real(v) => v,
        v => panic!("{name}: {v:?}"),
    }
}

/// Relative closeness `|a − b| ≤ tol·|b|`.
fn close(a: Real, b: Real, tol: Real) -> bool {
    (a - b).abs() <= tol * b.abs()
}

// ---------------------------------------------------------------

/// UD-105: the orthotropic update performs the parent
/// update by default — the parsed `eigen_grad_u` reaches the internal (and
/// thus `computeAllStresses`), and a parameter update after an assembled
/// tangent flags the stiffness as changed. Upstream (switch off before
/// `initMaterial`) drops both. Case in 2-D (`E1 = 100, E2 = 10,
/// ν12 = 0.2, G12 = 5`, `eigen_grad_u = [[0.01, 0], [0, 0]]`, C11 from
/// 100.40160642570282 to 200.4008016032064 after `E1 = 200`), and a 3-D
/// analogue.
#[test]
fn mat01_orthotropic_parent_update_2d_and_3d() {
    for dim in [2, 3] {
        let ty = simplex(dim);
        let (dat, eigen_rows, c11_before, c11_after): (&str, Vec<Vec<Real>>, Real, Real) = if dim
            == 2
        {
            (
                "material elastic_orthotropic [\n name = o\n E1 = 100\n E2 = 10\n nu12 = 0.2\n G12 = 5\n eigen_grad_u = [[0.01, 0], [0, 0]]\n]\n",
                vec![vec![0.01, 0.], vec![0., 0.]],
                100.401_606_425_702_82,
                200.400_801_603_206_4,
            )
        } else {
            // C11 = E1 (1 − ν23 ν32) Γ, ν32 = ν23 E3/E2, ν21 = ν12 E2/E1,
            // ν31 = ν13 E3/E1 (evaluated below with the law's formula).
            let c11 = |e1: Real| {
                let (e2, e3, nu12, nu13, nu23) = (10., 20., 0.2, 0.1, 0.25);
                let nu21 = nu12 * e2 / e1;
                let nu31 = nu13 * e3 / e1;
                let nu32 = nu23 * e3 / e2;
                let gamma =
                    1. / (1. - nu12 * nu21 - nu23 * nu32 - nu31 * nu13 - 2. * nu21 * nu32 * nu13);
                e1 * (1. - nu23 * nu32) * gamma
            };
            (
                "material elastic_orthotropic [\n name = o\n E1 = 100\n E2 = 10\n E3 = 20\n nu12 = 0.2\n nu13 = 0.1\n nu23 = 0.25\n G12 = 5\n G13 = 4\n G23 = 3\n eigen_grad_u = [[0.01, 0.003, 0], [0, 0, 0], [0, 0, -0.002]]\n]\n",
                vec![
                    vec![0.01, 0.003, 0.],
                    vec![0., 0., 0.],
                    vec![0., 0., -0.002],
                ],
                c11(100.),
                c11(200.),
            )
        };
        // column-major internal layout M(r, c) at r + dim·c
        let mut eigen = vec![0.0; dim * dim];
        for (r, row) in eigen_rows.iter().enumerate() {
            for (c, v) in row.iter().enumerate() {
                eigen[r + dim * c] = *v;
            }
        }
        let mut model = one_simplex(dim);
        model.gradient = Some(vec![0.0; dim * dim]);
        let v = dim * (dim + 1) / 2;

        // corrected default
        let mut h = materials_from_dat(&model, dat.as_bytes()).unwrap();
        let law = h.law_mut(0).unwrap();
        assert_eq!(
            law.base().eigen_grad_u.get(ty, NG).unwrap().tuple(0),
            &eigen[..],
            "{dim}-D: parsed eigen_grad_u applied after init"
        );
        // computeAllStresses with ∇u = 0: σ = C : sym(−eigen_grad_u) (unit
        // axes: C = C', engineering-shear Voigt product evaluated here)
        law.compute_all_stresses(&model.ctx(LawPhase::Stress), NG)
            .unwrap();
        let sigma = law.base().stress.get(ty, NG).unwrap().tuple(0).to_vec();
        let pairs: &[(usize, usize)] = if dim == 2 {
            &[(0, 0), (1, 1), (0, 1)]
        } else {
            &[(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)]
        };
        let strain: Vec<Real> = pairs
            .iter()
            .map(|&(i, j)| {
                let e = -0.5 * (eigen[i + dim * j] + eigen[j + dim * i]);
                if i == j { e } else { 2. * e }
            })
            .collect();
        let cij = |a: usize, b: usize| real(law, &format!("C{}{}", a.min(b) + 1, a.max(b) + 1));
        let smax = sigma.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
        assert!(smax > 0.0, "{dim}-D: the eigen strain acts");
        for (big_i, &(i, j)) in pairs.iter().enumerate() {
            let expected: Real = (0..v).map(|big_j| cij(big_i, big_j) * strain[big_j]).sum();
            for (r, c) in [(i, j), (j, i)] {
                assert!(
                    (sigma[r + dim * c] - expected).abs() <= 1e-14 * smax,
                    "{dim}-D: σ({r},{c}) = {} vs {expected}",
                    sigma[r + dim * c]
                );
            }
        }
        assert_eq!(real(law, "C11"), c11_before);
        let mut t = Array::zeros(1, v * v);
        law.compute_tangent_moduli(&model.ctx(LawPhase::Tangent), ty, NG, &mut t)
            .unwrap();
        assert!(!law.has_stiffness_matrix_changed());
        law.set_param("E1", ParamValue::Real(200.)).unwrap();
        assert!(
            law.has_stiffness_matrix_changed(),
            "{dim}-D: stiffness flagged changed after the update"
        );
        assert!(law.has_matrix_changed("K"));
        assert_eq!(real(law, "C11"), c11_after);

        // upstream (switch off before initMaterial)
        let mut h =
            materials_from_dat_with(&model, dat.as_bytes(), Some(LawOptions::upstream())).unwrap();
        let law = h.law_mut(0).unwrap();
        assert!(
            law.base()
                .eigen_grad_u
                .get(ty, NG)
                .unwrap()
                .as_slice()
                .iter()
                .all(|&x| x == 0.0),
            "{dim}-D upstream: eigen_grad_u dropped"
        );
        law.compute_tangent_moduli(&model.ctx(LawPhase::Tangent), ty, NG, &mut t)
            .unwrap();
        law.set_param("E1", ParamValue::Real(200.)).unwrap();
        assert!(
            !law.has_stiffness_matrix_changed(),
            "{dim}-D upstream: flag not reset"
        );
        assert_eq!(real(law, "C11"), c11_after);
    }
}

// ---------------------------------------------------------------

/// A non-symmetric rotation (Rodrigues, axis (1, 2, 3)/√14, 0.7 rad); the
/// columns are the material axes `n1, n2, n3 = n1 × n2`.
fn nonsymmetric_rotation() -> [[Real; 3]; 3] {
    let l = 14.0_f64.sqrt();
    let a = [1. / l, 2. / l, 3. / l];
    let (s, c) = 0.7_f64.sin_cos();
    // R(i, j) = c δij + s ε_ikj a_k + (1 − c) a_i a_j, stored as columns
    let k = [[0., a[2], -a[1]], [-a[2], 0., a[0]], [a[1], -a[0], 0.]]; // [a]× columns
    let mut cols = [[0.0; 3]; 3];
    for j in 0..3 {
        for i in 0..3 {
            let d = if i == j { 1. } else { 0. };
            cols[j][i] = c * d + s * k[j][i] + (1. - c) * a[i] * a[j];
        }
    }
    // n3 = n1 × n2 in floating point (right-handed within rounding)
    let (n1, n2) = (cols[0], cols[1]);
    cols[2] = [
        n1[1] * n2[2] - n1[2] * n2[1],
        n1[2] * n2[0] - n1[0] * n2[2],
        n1[0] * n2[1] - n1[1] * n2[0],
    ];
    cols
}

/// Voigt index of `(i, j)` (`[11, 22, 33, 23, 13, 12]`).
fn voigt3(i: usize, j: usize) -> usize {
    if i == j { i } else { 6 - i - j }
}

/// Independent rank-four rotation of an engineering-Voigt stiffness:
/// `C_ijkl = Σ Q_ia Q_jb Q_kc Q_ld C'_abcd` with `Q(i, a) = q(i, a)`.
fn rotate_rank_four(c_prime: &[[Real; 6]; 6], q: &dyn Fn(usize, usize) -> Real) -> [[Real; 6]; 6] {
    let mut out = [[0.0; 6]; 6];
    for (big_i, row) in out.iter_mut().enumerate() {
        let (i, j) = [(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)][big_i];
        for (big_j, x) in row.iter_mut().enumerate() {
            let (k, l) = [(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)][big_j];
            let mut s = 0.0;
            for a in 0..3 {
                for b in 0..3 {
                    for cc in 0..3 {
                        for d in 0..3 {
                            s += q(i, a)
                                * q(j, b)
                                * q(k, cc)
                                * q(l, d)
                                * c_prime[voigt3(a, b)][voigt3(cc, d)];
                        }
                    }
                }
            }
            *x = s;
        }
    }
    out
}

/// Directional Young's modulus `1/(σ_d · S · σ_d)` of a Voigt stiffness for
/// uniaxial stress along the unit vector `d`.
fn directional_young(c: &MatrixX, d: [Real; 3]) -> Real {
    let s = c.inverse().unwrap();
    let sv = [
        d[0] * d[0],
        d[1] * d[1],
        d[2] * d[2],
        d[1] * d[2],
        d[0] * d[2],
        d[0] * d[1],
    ];
    let mut e = 0.0;
    for i in 0..6 {
        for j in 0..6 {
            e += sv[i] * s.at(i, j) * sv[j];
        }
    }
    1. / e
}

fn assert_voigt_close(c: &MatrixX, expected: &[[Real; 6]; 6], what: &str) {
    let max = expected
        .iter()
        .flatten()
        .fold(0.0_f64, |m, x| m.nan_max(x.abs()));
    for (i, row) in expected.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            assert!(
                (c.at(i, j) - x).abs() <= 1e-12 * max,
                "{what}: C({i},{j}) = {} vs {x}",
                c.at(i, j)
            );
        }
    }
}

/// UD-104: by default the axes `n1.n3` are the material
/// directions, for a non-symmetric 3-D rotation and distinct moduli: the
/// directional Young's modulus along `n_a` is `E_a`, and the whole rotated
/// stiffness equals an independent rank-four rotation with `R` (columns
/// `n_a`) — for `elastic_orthotropic` and for a fully populated
/// `elastic_anisotropic` `C'`. Upstream rotates with `Rᵀ` (stiffness `E_a`
/// along the `a`-th row of `R`).
#[test]
fn mat02_axes_are_material_directions_3d_nonsymmetric_rotation() {
    let model = SimplexModel::new(3, &[], &[]);
    let r = nonsymmetric_rotation();
    let rm = |i: usize, a: usize| r[a][i];
    assert!(
        (0..3).any(|i| (0..3).any(|j| (rm(i, j) - rm(j, i)).abs() > 0.1)),
        "the rotation is not symmetric"
    );
    let young = [150e9, 10e9, 40e9];
    let build_ortho = |options: LawOptions| {
        let mut m = MaterialElasticOrthotropic::<3>::new(
            &spec(3, "elastic_orthotropic"),
            Some(&model.mesh),
        )
        .unwrap();
        m.options = options;
        m.core.dir_vecs = r;
        for (n, v) in [
            ("E1", young[0]),
            ("E2", young[1]),
            ("E3", young[2]),
            ("nu12", 0.25),
            ("nu13", 0.2),
            ("nu23", 0.3),
            ("G12", 5e9),
            ("G13", 6e9),
            ("G23", 4e9),
            ("rho", 1600.),
        ] {
            m.set_param_from_input(n, ParamValue::Real(v)).unwrap();
        }
        m.init_material(&model.ctx(LawPhase::Init)).unwrap();
        m
    };
    let m = build_ortho(LawOptions::default());
    let c_prime: [[Real; 6]; 6] =
        std::array::from_fn(|i| std::array::from_fn(|j| m.core.c_prime.at(i, j)));
    for (a, (n, &ea)) in r.iter().zip(&young).enumerate() {
        let e = directional_young(&m.core.c, *n);
        assert!(close(e, ea, 1e-12), "E along n{} = {e}", a + 1);
    }
    assert_voigt_close(
        &m.core.c,
        &rotate_rank_four(&c_prime, &rm),
        "orthotropic, R",
    );
    let up = build_ortho(LawOptions::upstream());
    for (a, &ea) in young.iter().enumerate() {
        let row = [rm(a, 0), rm(a, 1), rm(a, 2)];
        let e = directional_young(&up.core.c, row);
        assert!(close(e, ea, 1e-12), "upstream: E along row {a} = {e}");
    }
    assert!(
        !close(directional_young(&up.core.c, r[0]), young[0], 0.1),
        "upstream is not stiff along n1"
    );
    assert_voigt_close(
        &up.core.c,
        &rotate_rank_four(&c_prime, &|i, a| rm(a, i)),
        "orthotropic upstream, Rᵀ",
    );

    // fully populated anisotropic C' (symmetric positive definite)
    let mut full = [[0.0; 6]; 6];
    for (i, row) in full.iter_mut().enumerate() {
        for (j, x) in row.iter_mut().enumerate() {
            let (a, b) = (i.min(j), i.max(j));
            #[allow(clippy::cast_precision_loss)] // small indices
            let off = 1e9 * (1. + a as Real + 0.5 * b as Real) / (1. + (a + b) as Real);
            *x = if i == j { 1e11 / (1. + i as Real) } else { off };
        }
    }
    for options in [LawOptions::default(), LawOptions::upstream()] {
        let mut a = MaterialElasticLinearAnisotropic::<3>::new(
            &spec(3, "elastic_anisotropic"),
            Some(&model.mesh),
        )
        .unwrap();
        a.options = options;
        a.core.dir_vecs = r;
        for (i, row) in full.iter().enumerate() {
            for (j, x) in row.iter().enumerate().skip(i) {
                a.set_param_from_input(&format!("C{}{}", i + 1, j + 1), ParamValue::Real(*x))
                    .unwrap();
            }
        }
        a.init_material(&model.ctx(LawPhase::Init)).unwrap();
        let expected = if options.material_axes_as_directions {
            rotate_rank_four(&full, &rm)
        } else {
            rotate_rank_four(&full, &|i, k| rm(k, i))
        };
        assert_voigt_close(
            &a.core.c,
            &expected,
            &format!(
                "anisotropic, as_directions = {}",
                options.material_axes_as_directions
            ),
        );
    }
}

// ---------------------------------------------------------------

/// One-element thermoelastic law (`E = 2.1e11`, `ν = 0.3`, `α = 1.2e-5`,
/// `ΔT = 50`, `ρ = 7800`).
fn thermal_law(
    model: &SimplexModel,
    plane_stress: bool,
    finite: bool,
    alpha: Real,
    options: LawOptions,
) -> ConstitutiveLawsHandler<dyn Material> {
    let dat = format!(
        "material elastic [\n name = s\n E = 2.1e11\n nu = 0.3\n alpha = {alpha:e}\n delta_T = 50\n rho = 7800\n{}{}]\n",
        if plane_stress {
            " Plane_Stress = true\n"
        } else {
            ""
        },
        if finite {
            " finite_deformation = true\n"
        } else {
            ""
        }
    );
    materials_from_dat_with(model, dat.as_bytes(), Some(options)).unwrap()
}

/// Energy density (whole-field path), stress (PK2 with finite deformation)
/// and thermal stress at the gradient `g` (column-major); asserts that the
/// per-element path gives the same density, bit for bit.
fn evaluate(
    law: &mut dyn Material,
    model: &SimplexModel,
    ty: ElementType,
    g: &[Real],
) -> (Real, Vec<Real>, Real) {
    law.base_mut()
        .grad_u
        .get_mut(ty, NG)
        .unwrap()
        .tuple_mut(0)
        .copy_from_slice(g);
    let ctx = model.ctx(LawPhase::Energy);
    law.compute_stress(&ctx, ty, NG).unwrap();
    law.compute_potential_energy(&ctx, ty).unwrap();
    let w = law.base().potential_energy.get(ty, NG).unwrap().tuple(0)[0];
    let by_element = law
        .potential_energy_by_element(&ctx, Element::new(ty, 0, NG))
        .unwrap();
    assert_eq!(by_element[0].to_bits(), w.to_bits(), "per-element path");
    let s = law
        .base()
        .sigma_field()
        .unwrap()
        .get(ty, NG)
        .unwrap()
        .tuple(0)
        .to_vec();
    let mut sigma_th = 0.0;
    law.for_each_internal(&mut |n, f| {
        if n == "sigma_th"
            && let akantu_model::constitutive_law::InternalFieldRef::Real(f) = f
        {
            sigma_th = f.get(ty, NG).unwrap().tuple(0)[0];
        }
    });
    (w, s, sigma_th)
}

/// Central difference of the energy density with respect to every
/// component of `∇u` (step `h`).
fn energy_gradient(
    law: &mut dyn Material,
    model: &SimplexModel,
    ty: ElementType,
    g: &[Real],
    h: Real,
) -> Vec<Real> {
    (0..g.len())
        .map(|c| {
            let mut gp = g.to_vec();
            gp[c] += h;
            let mut gm = g.to_vec();
            gm[c] -= h;
            let (wp, _, _) = evaluate(law, model, ty, &gp);
            let (wm, _, _) = evaluate(law, model, ty, &gm);
            (wp - wm) / (2. * h)
        })
        .collect()
}

/// `P = F·S` (column-major), `F = I + ∇u`: the stress conjugate to `∇u`.
fn first_piola(dim: usize, g: &[Real], s: &[Real]) -> Vec<Real> {
    let f = |i: usize, k: usize| g[i + dim * k] + if i == k { 1. } else { 0. };
    let mut p = vec![0.0; dim * dim];
    for j in 0..dim {
        for i in 0..dim {
            p[i + dim * j] = (0..dim).map(|k| f(i, k) * s[k + dim * j]).sum();
        }
    }
    p
}

/// One triangle (plane strain),
/// `E = 210 GPa, ν = 0.3, α = 1.2e-5, ΔT = 50`, zero gradient: σ11 =
/// −315 MPa. The centred difference of the energy density at
/// `∇u11 = ±1e-7` is σ11 by default (stored elastic energy); upstream's
/// `½ σ:∇u` gives half of it (−157.5 MPa).
#[test]
fn mat03_reviewer_case_plane_strain() {
    let model = one_simplex(2);
    let ty = simplex(2);
    let zero = [0.0; 4];
    for (options, expected_slope) in [
        (LawOptions::default(), -315e6),
        (LawOptions::upstream(), -157.5e6),
    ] {
        let mut h = thermal_law(&model, false, false, 1.2e-5, options);
        let law = h.law_mut(0).unwrap();
        let (_, s, _) = evaluate(law, &model, ty, &zero);
        assert!(close(s[0], -315e6, 1e-12), "σ11 = {}", s[0]);
        let d = energy_gradient(law, &model, ty, &zero, 1e-7);
        // W is quadratic in ∇u: the centred difference is exact up to the
        // rounding of W (|W| ≈ 2.8e5 J/m³, ulp ≈ 6e-11, / 2h ≈ 3e-4 Pa).
        assert!(
            close(d[0], expected_slope, 1e-8),
            "{options:?}: dW/d∇u11 = {}",
            d[0]
        );
    }
}

/// UD-109: at a fixed non-zero temperature the default
/// energy density is a potential of the computed stress — the centred
/// difference of `W` with respect to every component of `∇u` equals σ
/// (small strain) or `P = F·S` (St-Venant–Kirchhoff, PK2 `S`), in 1-D,
/// 2-D plane strain, 2-D plane stress and 3-D, through the whole-field and
/// per-element paths. Upstream's `½ σ:∇u` fails it by `½ σ_th·I` (`½ σ_th·F`
/// with finite deformation). Bound: `W` is quadratic (small strain) or quartic (finite) in
/// `∇u`; with `h = 1e-7` the truncation of the quartic case is
/// `O(h²·E) ≈ 2e-3 Pa` and the rounding `≈ ulp(W)/h ≈ 1e-3 Pa`, against
/// stresses ≥ 1e8 Pa: checked at `1e-8·max|σ|`.
#[test]
fn mat03_thermal_energy_is_a_potential_of_the_stress() {
    let gradients: [&[Real]; 3] = [
        &[1.3e-3],
        &[1.1e-3, -4e-4, 2.5e-4, -7e-4],
        &[9e-4, 2e-4, -3e-4, -1e-4, -6e-4, 4e-4, 5e-4, 1.5e-4, 2e-3],
    ];
    for dim in 1..=3 {
        let ty = simplex(dim);
        let model = one_simplex(dim);
        let g = gradients[dim - 1];
        let plane_stress_cases: &[bool] = if dim == 2 { &[false, true] } else { &[false] };
        for &plane_stress in plane_stress_cases {
            for finite in [false, true] {
                // finite deformation: larger strains so that P ≠ σ matters
                let g: Vec<Real> = g
                    .iter()
                    .map(|x| if finite { 20. * x } else { *x })
                    .collect();
                let what = format!("{dim}-D ps={plane_stress} finite={finite}");
                let mut hd =
                    thermal_law(&model, plane_stress, finite, 1.2e-5, LawOptions::default());
                let law = hd.law_mut(0).unwrap();
                let (w, s, sigma_th) = evaluate(law, &model, ty, &g);
                assert!(sigma_th < 0.0, "{what}: thermal stress active");
                assert!(w > 0.0, "{what}: stored energy positive, W = {w}");
                let expected = if finite {
                    first_piola(dim, &g, &s)
                } else {
                    s.clone()
                };
                let max = expected.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
                let d = energy_gradient(law, &model, ty, &g, 1e-7);
                for (c, (a, b)) in d.iter().zip(&expected).enumerate() {
                    assert!(
                        (a - b).abs() <= 1e-8 * max,
                        "{what}: dW/d∇u[{c}] = {a} vs {b}"
                    );
                }
                // upstream: the derivative misses ½ σ_th·I (small strain),
                // ½ σ_th·F (finite: d(½S:E) = (S − ½σ_th·I):dE)
                let mut hu =
                    thermal_law(&model, plane_stress, finite, 1.2e-5, LawOptions::upstream());
                let lu = hu.law_mut(0).unwrap();
                let (wu, su, _) = evaluate(lu, &model, ty, &g);
                assert_eq!(su, s, "{what}: stress independent of the energy switch");
                assert_ne!(wu, w, "{what}");
                let du = energy_gradient(lu, &model, ty, &g, 1e-7);
                let f11 = if finite { 1. + g[0] } else { 1. };
                assert!(
                    ((du[0] - expected[0]) - (-0.5 * sigma_th * f11)).abs() <= 1e-8 * max,
                    "{what}: upstream slope {} = {} − ½σ_th·F11",
                    du[0],
                    expected[0]
                );
            }
        }
    }
}

/// UD-109 temperature-only reference: at zero strain the default energy is
/// `g(T) = ½ ε_th:C:ε_th` (1-D `½E(αΔT)²`, plane stress `E/(1 − ν)(αΔT)²`,
/// plane strain and 3-D `(9/2)K(αΔT)²`); at the stress-free free expansion
/// (`∇u = αΔT·I`) it vanishes; in plane strain the in-plane free expansion
/// `(1 + ν)αΔT` leaves the constraint energy `½E(αΔT)²`.
#[test]
fn mat03_thermal_energy_reference_states() {
    let (e, nu, adt) = (2.1e11, 0.3, 1.2e-5 * 50.);
    let k = e / (3. * (1. - 2. * nu));
    for (dim, plane_stress, g_t) in [
        (1, false, 0.5 * e * adt * adt),
        (2, true, e / (1. - nu) * adt * adt),
        (2, false, 4.5 * k * adt * adt),
        (3, false, 4.5 * k * adt * adt),
    ] {
        let ty = simplex(dim);
        let model = one_simplex(dim);
        let mut h = thermal_law(&model, plane_stress, false, 1.2e-5, LawOptions::default());
        let law = h.law_mut(0).unwrap();
        let what = format!("{dim}-D ps={plane_stress}");
        let (w0, _, _) = evaluate(law, &model, ty, &vec![0.0; dim * dim]);
        assert!(close(w0, g_t, 1e-14), "{what}: W(0) = {w0} vs g(T) = {g_t}");
        // free expansion ∇u = αΔT·I: stress-free, no stored energy (1-D,
        // plane stress, 3-D); plane strain: in-plane (1 + ν)αΔT
        let diag = if dim == 2 && !plane_stress {
            (1. + nu) * adt
        } else {
            adt
        };
        let mut g = vec![0.0; dim * dim];
        for i in 0..dim {
            g[i + dim * i] = diag;
        }
        let (w, s, _) = evaluate(law, &model, ty, &g);
        let smax = s.iter().fold(0.0_f64, |m, x| m.nan_max(x.abs()));
        assert!(smax <= 1e-12 * e * adt, "{what}: stress-free, |σ| = {smax}");
        let expected = if dim == 2 && !plane_stress {
            0.5 * e * adt * adt
        } else {
            0.0
        };
        assert!(
            (w - expected).abs() <= 1e-10 * g_t,
            "{what}: W(free expansion) = {w} vs {expected}"
        );
    }
}

/// Without thermal stress (`α = 0`) both energy conventions give
/// upstream's `½ σ:∇u` bit for bit (so the elastic and finite-deformation
/// fixtures are unaffected by the default).
#[test]
fn mat03_no_thermal_stress_is_bitwise_upstream() {
    let g: [Real; 9] = [9e-4, 2e-4, -3e-4, -1e-4, -6e-4, 4e-4, 5e-4, 1.5e-4, 2e-3];
    for finite in [false, true] {
        let model = one_simplex(3);
        let ty = simplex(3);
        let mut hd = thermal_law(&model, false, finite, 0.0, LawOptions::default());
        let mut hu = thermal_law(&model, false, finite, 0.0, LawOptions::upstream());
        let (wd, sd, th) = evaluate(hd.law_mut(0).unwrap(), &model, ty, &g);
        let (wu, su, _) = evaluate(hu.law_mut(0).unwrap(), &model, ty, &g);
        assert_eq!(th, 0.0);
        assert_eq!(sd, su);
        assert_eq!(wd.to_bits(), wu.to_bits(), "finite = {finite}");
        let s: &Matrix<3, 3> = akantu_core::tensor::as_matrix(&sd).unwrap();
        let gm: &Matrix<3, 3> = akantu_core::tensor::as_matrix(&g).unwrap();
        if !finite {
            assert_eq!(
                wd.to_bits(),
                MaterialElastic::<3>::energy_on_quad(s, gm).to_bits()
            );
        }
    }
}

// ---------------------------------------------------------------

/// Selector: law 0 for element 0; element 1 gets law 1 iff
/// element 0 already has law 0 in the handler's map.
#[derive(Debug)]
struct DependsOnFirst;

impl ConstitutiveLawSelector for DependsOnFirst {
    fn select(
        &self,
        ctx: &SelectorContext<'_>,
        element: Element,
    ) -> akantu_core::Result<Option<usize>> {
        if element.idx() == 0 {
            return Ok(Some(0));
        }
        let previous = ctx
            .law_index
            .get(element.ty(), element.ghost_type())?
            .as_slice()[0];
        Ok(Some(if previous == 0 { 1 } else { 0 }))
    }
}

/// Element 1 asks for a law index that does not exist.
#[derive(Debug)]
struct FailsOnSecond;

impl ConstitutiveLawSelector for FailsOnSecond {
    fn select(
        &self,
        _ctx: &SelectorContext<'_>,
        element: Element,
    ) -> akantu_core::Result<Option<usize>> {
        Ok(Some(if element.idx() == 0 { 1 } else { 7 }))
    }
}

fn two_triangles_handler(
    model: &SimplexModel,
    selector: Box<dyn ConstitutiveLawSelector>,
) -> ConstitutiveLawsHandler<dyn Material> {
    let mut h = ConstitutiveLawsHandler::<dyn Material>::new(
        "solid_mechanics_model",
        &model.mesh,
        2,
        ParserType::Material,
    )
    .unwrap();
    let f = common::factory();
    for name in ["a", "b"] {
        h.register_new_law(&f, name, "elastic", "", model).unwrap();
    }
    h.selector = SelectorChain {
        selectors: vec![selector],
        fallback_value: 0,
    };
    h
}

/// A selector sees the assignments made earlier in the
/// same pass (upstream writes `constitutive_law_index(element)` inside the
/// selection loop). Two-triangle case: upstream sequence `[0, 1]`
/// (the old snapshot gave `[0, 0]`). On a selector error the first pass
/// stops: earlier elements keep their law, no element is added to a law.
#[test]
fn mat04_selector_sees_earlier_assignments() {
    let model = SimplexModel::new(
        2,
        &[vec![0., 0.], vec![1., 0.], vec![0., 1.], vec![1., 1.]],
        &[vec![0, 1, 2], vec![1, 3, 2]],
    );
    let tri = simplex(2);
    let mut h = two_triangles_handler(&model, Box::new(DependsOnFirst));
    h.assign_laws_to_elements(&model.mesh, None).unwrap();
    assert_eq!(h.law_index_map().get(tri, NG).unwrap().as_slice(), &[0, 1]);
    assert_eq!(
        h.local_numbering_map().get(tri, NG).unwrap().as_slice(),
        &[0, 0]
    );
    for (law, element) in [(0, 0), (1, 1)] {
        assert_eq!(
            h.law(law)
                .unwrap()
                .element_filter()
                .get(tri, NG)
                .unwrap()
                .as_slice(),
            &[element]
        );
    }

    let mut h = two_triangles_handler(&model, Box::new(FailsOnSecond));
    let err = h.assign_laws_to_elements(&model.mesh, None).unwrap_err();
    assert!(matches!(err, Error::OutOfRange { index: 7, .. }), "{err:?}");
    assert_eq!(h.law_index_map().get(tri, NG).unwrap().as_slice(), &[1, -1]);
    for law in 0..2 {
        assert!(
            h.law(law)
                .unwrap()
                .element_filter()
                .get(tri, NG)
                .unwrap()
                .is_empty()
        );
    }
}

// ---------------------------------------------------------------

/// The Kelvin celerity is computed from the eigenvalue
/// cached at the parameter update (no per-query eigen-solve): it equals the
/// freshly computed `√(λ_max(D·C·D)/ρ)` bit for bit, ignores a direct change
/// of `C` until the next update (as upstream's cached `eigC`), follows every
/// update (moduli, axes) and the current density. The Kelvin bound is
/// invariant under a change of material axes (Mandel rotation), upstream's
/// first unsorted eigenvalue is not.
#[test]
fn mat06_celerity_uses_the_cached_kelvin_eigenvalue() {
    let model = SimplexModel::new(3, &[], &[]);
    let fresh = |m: &MaterialElasticOrthotropic<3>| {
        *eigh(&m.core.kelvin_stiffness(), Storage::Dynamic)
            .unwrap()
            .last()
            .unwrap()
    };
    let mut m =
        MaterialElasticOrthotropic::<3>::new(&spec(3, "elastic_orthotropic"), Some(&model.mesh))
            .unwrap();
    for (n, v) in [
        ("E1", 150e9),
        ("E2", 10e9),
        ("E3", 40e9),
        ("nu12", 0.25),
        ("nu13", 0.2),
        ("nu23", 0.3),
        ("G12", 5e9),
        ("G13", 6e9),
        ("G23", 4e9),
        ("rho", 1600.),
    ] {
        m.set_param_from_input(n, ParamValue::Real(v)).unwrap();
    }
    m.init_material(&model.ctx(LawPhase::Init)).unwrap();
    let el = Element::NULL;
    let lmax = fresh(&m);
    assert_eq!(m.core.kelvin_lambda_max(), Some(lmax));
    let c0 = m.celerity(el).unwrap();
    assert_eq!(c0.to_bits(), (lmax / 1600.).sqrt().to_bits());
    // a direct change of C is not seen before the next update (cached)
    let saved = m.core.c.clone();
    m.core.c = MatrixX::zeros(6, 6);
    assert_eq!(m.celerity(el).unwrap().to_bits(), c0.to_bits());
    m.core.c = saved;
    // a modulus update refreshes the cache
    m.set_param("E1", ParamValue::Real(300e9)).unwrap();
    let lmax2 = fresh(&m);
    assert!(lmax2 > lmax);
    assert_eq!(
        m.celerity(el).unwrap().to_bits(),
        (lmax2 / 1600.).sqrt().to_bits()
    );
    // the density is read at query time
    m.set_param("rho", ParamValue::Real(2500.)).unwrap();
    assert_eq!(
        m.celerity(el).unwrap().to_bits(),
        (lmax2 / 2500.).sqrt().to_bits()
    );
    // an axis update refreshes both caches: Kelvin bound invariant,
    // upstream's first unsorted eigenvalue of C changes
    let eig0 = m.core.eig_c.clone();
    let r = nonsymmetric_rotation();
    for (k, n) in ["n1", "n2", "n3"].iter().enumerate() {
        m.set_param_from_input(n, ParamValue::VectorReal(r[k].to_vec()))
            .unwrap();
    }
    m.update_internal_parameters().unwrap();
    assert_eq!(m.core.eig_c, eig(&m.core.c, Storage::Dynamic).unwrap());
    assert_ne!(m.core.eig_c, eig0);
    let lmax3 = m.core.kelvin_lambda_max().unwrap();
    assert_eq!(lmax3, fresh(&m));
    assert!(close(lmax3, lmax2, 1e-12), "{lmax3} vs {lmax2}");
    // upstream compatibility: √(eigC[0]/ρ) from the cached eigenvalues
    m.set_options(LawOptions::upstream()).unwrap();
    assert_eq!(
        m.celerity(el).unwrap().to_bits(),
        (m.core.eig_c[0] / 2500.).sqrt().to_bits()
    );
    // the same holds for elastic_anisotropic (shared core)
    let mut a = MaterialElasticLinearAnisotropic::<2>::new(
        &spec(2, "elastic_anisotropic"),
        Some(&SimplexModel::new(2, &[], &[]).mesh),
    )
    .unwrap();
    for (n, v) in [
        ("C11", 5.),
        ("C12", 1.),
        ("C22", 3.),
        ("C33", 2.),
        ("rho", 2.),
    ] {
        a.set_param(n, ParamValue::Real(v)).unwrap();
    }
    let lk = *eigh(&a.core.kelvin_stiffness(), Storage::Dynamic)
        .unwrap()
        .last()
        .unwrap();
    assert_eq!(
        a.celerity(el).unwrap().to_bits(),
        (lk / 2.).sqrt().to_bits()
    );
}

// ------------------------------------------------------- UD-108 (kept)

/// UD-108 (category C, kept): the `eigen_grad_u`
/// parameter is copied into the **owned** blocks of the internal only
/// (upstream `Material::updateInternalParameters`); ghost blocks keep their
/// values. Normal residuals are unaffected because ghost stresses are
/// received from their owners (`SolidMechanicsModel::assembleInternalForces`);
/// a ghost-local constitutive re-evaluation would need this field
/// initialised or exchanged first (model contract).
#[test]
fn ud108_eigen_grad_u_parameter_reaches_owned_elements_only() {
    let tri = simplex(2);
    let mut b = akantu_solid::MaterialBase::new(&spec(2, "elastic"), "elastic").unwrap();
    for g in akantu_core::GhostType::ALL {
        b.element_filter.alloc(1, 1, tri, g, 0).unwrap();
    }
    b.eigen_grad_u
        .resize_blocks(&[(tri, NG, 1), (tri, akantu_core::GhostType::Ghost, 1)])
        .unwrap();
    *b.eigen_grad_u_param.at_mut(0, 0) = 0.01;
    b.update_internal_parameters().unwrap();
    assert_eq!(
        b.eigen_grad_u.get(tri, NG).unwrap().tuple(0),
        &[0.01, 0., 0., 0.]
    );
    assert_eq!(
        b.eigen_grad_u
            .get(tri, akantu_core::GhostType::Ghost)
            .unwrap()
            .tuple(0),
        &[0., 0., 0., 0.]
    );
}

// -------------------------------------------------

/// Switching options after the tangent was assembled
/// changes the stiffness, so the law must report the stiffness (and `K`) as
/// changed — also when switching to upstream compat mode, whose own
/// orthotropic update does not reset the flag (UD-105).
#[test]
fn mat2_01_set_options_after_assembly_flags_the_stiffness() {
    for dim in [2usize, 3] {
        let dat = if dim == 2 {
            "material elastic_orthotropic [\n name = o\n E1 = 100\n E2 = 10\n nu12 = 0.2\n G12 = 5\n n1 = [0.8, 0.6]\n n2 = [-0.6, 0.8]\n]\n"
        } else {
            "material elastic_orthotropic [\n name = o\n E1 = 100\n E2 = 10\n E3 = 20\n nu12 = 0.2\n nu13 = 0.1\n nu23 = 0.25\n G12 = 5\n G13 = 4\n G23 = 3\n]\n"
        };
        let mut model = one_simplex(dim);
        model.gradient = Some(vec![0.0; dim * dim]);
        let v = dim * (dim + 1) / 2;
        let ty = simplex(dim);
        let mut h = materials_from_dat(&model, dat.as_bytes()).unwrap();
        let law = h.law_mut(0).unwrap();
        let mut t = Array::zeros(1, v * v);
        law.compute_tangent_moduli(&model.ctx(LawPhase::Tangent), ty, NG, &mut t)
            .unwrap();
        assert!(!law.has_stiffness_matrix_changed(), "{dim}-D: assembled");
        law.set_options(LawOptions::upstream()).unwrap();
        assert!(
            law.has_stiffness_matrix_changed(),
            "{dim}-D: stiffness flagged changed after switching options"
        );
        assert!(law.has_matrix_changed("K"), "{dim}-D: K flagged changed");
        // and back to the corrected defaults
        law.compute_tangent_moduli(&model.ctx(LawPhase::Tangent), ty, NG, &mut t)
            .unwrap();
        law.set_options(LawOptions::default()).unwrap();
        assert!(law.has_stiffness_matrix_changed(), "{dim}-D: flagged again");
    }
}
