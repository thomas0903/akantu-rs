// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Model-level checks of
// the documented Neo-Hookean law (MAT-025/026; upstream
// src/model/solid_mechanics/materials/material_finite_deformation/
// material_neohookean*, which raises NotImplemented, UD-026) through the
// total-Lagrangian assembly (SMM-023; material.cc
// assembleInternalForcesFiniteDeformation / assembleStiffnessMatrixNL / L2).
// TEST-142 note: independent checks, no oracle.

//! 1. **Affine field.** For `u = H·X` the deformation gradient, hence the
//!    first Piola–Kirchhoff stress `P = F S`, is uniform, so the assembled
//!    internal force vanishes at every interior node (`∫ ∇N_a dV = 0`) and,
//!    for the affine test fields `v = e_i ⊗ e_j · X`, the virtual work gives
//!    `Σ_a f_{a,i} X_{a,j} = −V P_ij` exactly (`f_int = −∫ ∇N·P`). The `P`
//!    recovered from the assembled forces is compared with the closed form
//!    `S = μ I + (λ ln J − μ) C⁻¹` of the module documentation, computed
//!    here independently.
//! 2. **Tangent.** The assembled stiffness (material + geometric part) is
//!    the derivative of the assembled internal force: `K v` against the
//!    central difference of `f_int` at a genuinely nonlinear state, in
//!    plane strain, plane stress (condensed tangent, `C33` solved at every
//!    evaluation) and 3-D.
//! 3. **Objectivity.** A large rigid rotation of the deformed state rotates
//!    the internal force and leaves the energy unchanged.
//!
//! Tolerances: docs/NUMERICAL_VALIDATION.md.

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use akantu_mesh::Mesh;
use akantu_model::ModelOptions;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use model_common::*;

const E: Real = 1e6;
const NU: Real = 0.45;

fn ty(name: &str) -> ElementType {
    format!("_{name}").parse().unwrap()
}

/// Patch-test mesh `_<type>.msh` (unit square / cube).
fn patch_mesh(name: &str) -> Mesh {
    let f = fixtures_root()
        .join("model/smm")
        .join(format!("lumped_mass_{name}"))
        .join(format!("inputs/_{name}.msh"));
    read_mesh(&f, ty(name).spatial_dimension())
}

fn model(name: &str, plane_stress: bool) -> SolidMechanicsModel {
    let dat = format!(
        "material neohookean [\n  name = rubber\n  rho = 1000\n  E = {E}\n  nu = {NU}\n{}]\n",
        if plane_stress {
            "  Plane_Stress = true\n"
        } else {
            ""
        }
    );
    let mut m = model_with(patch_mesh(name), &dat);
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    m
}

fn nodes(m: &SolidMechanicsModel) -> Vec<Real> {
    m.physics
        .base
        .mesh
        .read()
        .unwrap()
        .nodes()
        .as_slice()
        .to_vec()
}

fn norm(v: &[Real]) -> Real {
    v.iter().map(|x| x * x).sum::<Real>().sqrt()
}

fn internal_force_at(m: &mut SolidMechanicsModel, u: &[Real]) -> Vec<Real> {
    m.displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(u);
    m.assemble_internal_forces().unwrap();
    m.internal_force().unwrap().as_slice().to_vec()
}

/// `n × n` matrices stored row-major `a[i][j]`.
type M3 = [[Real; 3]; 3];

fn mat_mul(a: &M3, b: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| (0..3).map(|k| a[i][k] * b[k][j]).sum()))
}

fn transpose(a: &M3) -> M3 {
    std::array::from_fn(|i| std::array::from_fn(|j| a[j][i]))
}

fn det(a: &M3) -> Real {
    a[0][0] * (a[1][1] * a[2][2] - a[1][2] * a[2][1])
        - a[0][1] * (a[1][0] * a[2][2] - a[1][2] * a[2][0])
        + a[0][2] * (a[1][0] * a[2][1] - a[1][1] * a[2][0])
}

fn inverse(a: &M3) -> M3 {
    let d = det(a);
    let c = |i: usize, j: usize| {
        let (r0, r1) = ((i + 1) % 3, (i + 2) % 3);
        let (c0, c1) = ((j + 1) % 3, (j + 2) % 3);
        a[r0][c0] * a[r1][c1] - a[r0][c1] * a[r1][c0]
    };
    // inverse = adjugate / det, adj(i, j) = cofactor(j, i)
    std::array::from_fn(|i| std::array::from_fn(|j| c(j, i) / d))
}

/// Closed-form `P = F S` (3-D or plane strain: `F33 = 1`) of the documented
/// law, `S = μ I + (λ ln J − μ) C⁻¹`, `λ`, `μ` the 3-D Lamé constants.
fn pk1_closed_form(h: &M3) -> M3 {
    let lambda = NU * E / ((1. + NU) * (1. - 2. * NU));
    let mu = E / (2. * (1. + NU));
    let f: M3 =
        std::array::from_fn(|i| std::array::from_fn(|j| h[i][j] + if i == j { 1. } else { 0. }));
    let c = mat_mul(&transpose(&f), &f);
    let ci = inverse(&c);
    let ln_j = det(&f).ln();
    let s: M3 = std::array::from_fn(|i| {
        std::array::from_fn(|j| mu * if i == j { 1. } else { 0. } + (lambda * ln_j - mu) * ci[i][j])
    });
    mat_mul(&f, &s)
}

/// A finite affine deformation (stretch, shear, compression; `det F > 0`).
fn affine_h(dim: usize) -> M3 {
    let mut h = [[0.0; 3]; 3];
    let full = [
        [0.15, 0.08, -0.05],
        [-0.03, -0.10, 0.06],
        [0.04, 0.02, 0.12],
    ];
    for i in 0..dim {
        for j in 0..dim {
            h[i][j] = full[i][j];
        }
    }
    h
}

/// Affine field `u = H X` on every 2-D (plane strain) and 3-D patch type:
/// interior internal forces vanish, the virtual-work identity recovers the
/// closed-form `P` (reaction resultants of a homogeneous stress).
#[test]
fn affine_field_interior_forces_vanish_and_virtual_work_recovers_pk1() {
    for name in [
        "triangle_3",
        "triangle_6",
        "quadrangle_4",
        "quadrangle_8",
        "tetrahedron_4",
        "tetrahedron_10",
        "hexahedron_8",
        "hexahedron_20",
        "pentahedron_6",
        "pentahedron_15",
    ] {
        let mut m = model(name, false);
        let dim = m.spatial_dimension();
        let x = nodes(&m);
        // the patch is a box [lo, hi]^dim (unit square / cube for most)
        let bounds: Vec<(Real, Real)> = (0..dim)
            .map(|c| {
                x.iter()
                    .skip(c)
                    .step_by(dim)
                    .fold((Real::MAX, Real::MIN), |(l, h), &v| (l.min(v), h.max(v)))
            })
            .collect();
        let volume: Real = bounds.iter().map(|(l, h)| h - l).product();
        let h = affine_h(dim);
        let u: Vec<Real> = x
            .chunks_exact(dim)
            .flat_map(|xi| (0..dim).map(move |i| (0..dim).map(|j| h[i][j] * xi[j]).sum::<Real>()))
            .collect();
        let f = internal_force_at(&mut m, &u);
        let fmax = f.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
        // interior nodes (strictly inside the unit box)
        let mut interior = 0;
        let mut imax = 0.0_f64;
        for (xi, fi) in x.chunks_exact(dim).zip(f.chunks_exact(dim)) {
            if xi
                .iter()
                .zip(&bounds)
                .all(|(&v, &(l, h))| v > l + 1e-12 && v < h - 1e-12)
            {
                interior += 1;
                imax = fi.iter().fold(imax, |a, b| a.max(b.abs()));
            }
        }
        assert!(interior > 0, "{name}: no interior node");
        assert!(
            imax <= 1e-12 * fmax,
            "{name}: interior |f| = {imax:e} vs max |f| = {fmax:e}"
        );
        // P_ij = −(1/V) Σ_a f_{a,i} X_{a,j}
        let p = pk1_closed_form(&h);
        let pmax = (0..dim)
            .flat_map(|i| (0..dim).map(move |j| (i, j)))
            .fold(0.0_f64, |a, (i, j)| a.max(p[i][j].abs()));
        let mut worst = 0.0_f64;
        for i in 0..dim {
            for j in 0..dim {
                let recovered: Real = -x
                    .chunks_exact(dim)
                    .zip(f.chunks_exact(dim))
                    .map(|(xa, fa)| fa[i] * xa[j])
                    .sum::<Real>()
                    / volume;
                worst = worst.max((recovered - p[i][j]).abs() / pmax);
            }
        }
        eprintln!("affine {name}: interior {imax:e} / {fmax:e}, P mismatch {worst:e}");
        assert!(
            worst < 1e-12,
            "{name}: recovered P differs by {worst:e} (relative)"
        );
    }
}

/// Deterministic pseudo-random values in `[-1, 1)` (LCG).
fn noise(n: usize, seed: u64) -> Vec<Real> {
    let mut s = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..n)
        .map(|_| {
            s = s
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            ((s >> 11) as Real / (1u64 << 53) as Real) * 2.0 - 1.0
        })
        .collect()
}

/// A smooth nonlinear field of amplitude `a` (as `model_finite_deformation`).
fn nonlinear_field(m: &SolidMechanicsModel, a: Real, seed: u64) -> Vec<Real> {
    let dim = m.spatial_dimension();
    let x = nodes(m);
    let z = noise(x.len(), seed);
    let mut u = vec![0.0; x.len()];
    for (n, (ui, xi)) in u.chunks_exact_mut(dim).zip(x.chunks_exact(dim)).enumerate() {
        for c in 0..dim {
            let mut v = 0.0;
            for (k, &xk) in xi.iter().enumerate() {
                v += (0.3 + 0.1 * (c + k) as Real) * xk + 0.2 * xk * xk * (c as Real - 0.5);
            }
            ui[c] = a * (v + 0.05 * z[n * dim + c]);
        }
    }
    u
}

/// `K v` with the assembled stiffness (symmetric storage: one triangle).
fn k_times(m: &SolidMechanicsModel, v: &[Real]) -> Vec<Real> {
    let k = m.dof_manager().matrix("K").unwrap();
    let eq = &m
        .dof_manager()
        .family("displacement")
        .unwrap()
        .local_equation_numbers;
    let mut dof_of = vec![0usize; eq.len()];
    for (d, &e) in eq.iter().enumerate() {
        dof_of[e as usize] = d;
    }
    let sym = k.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
    let (r, c, a) = k.coo();
    let mut out = vec![0.0; v.len()];
    for ((&i, &j), &x) in r.iter().zip(c).zip(a) {
        let (i, j) = (dof_of[i as usize], dof_of[j as usize]);
        out[i] += x * v[j];
        if sym && i != j {
            out[j] += x * v[i];
        }
    }
    out
}

/// The assembled tangent is the derivative of the assembled internal force
/// (central differences, `h·|v|` ≈ 1e-7 of the displacement scale), plane
/// strain, plane stress and 3-D.
#[test]
fn assembled_tangent_is_the_derivative_of_the_internal_force() {
    for (name, plane_stress) in [
        ("triangle_3", false),
        ("triangle_6", false),
        ("quadrangle_8", false),
        ("triangle_3", true),
        ("quadrangle_4", true),
        ("tetrahedron_4", false),
        ("tetrahedron_10", false),
        ("hexahedron_8", false),
    ] {
        let mut m = model(name, plane_stress);
        let u = nonlinear_field(&m, 0.1, 7);
        internal_force_at(&mut m, &u);
        m.assemble_stiffness_matrix(true).unwrap();
        let v = noise(u.len(), 11);
        let kv = k_times(&m, &v);
        let h = 1e-7;
        let up: Vec<Real> = u.iter().zip(&v).map(|(a, b)| a + h * b).collect();
        let um: Vec<Real> = u.iter().zip(&v).map(|(a, b)| a - h * b).collect();
        let fp = internal_force_at(&mut m, &up);
        let fm = internal_force_at(&mut m, &um);
        let fd: Vec<Real> = fp
            .iter()
            .zip(&fm)
            .map(|(p, q)| -(p - q) / (2. * h))
            .collect();
        let diff: Vec<Real> = kv.iter().zip(&fd).map(|(a, b)| a - b).collect();
        let rel = norm(&diff) / norm(&kv);
        eprintln!("tangent {name} plane_stress={plane_stress}: {rel:e}");
        assert!(
            rel < 1e-8,
            "{name} (plane stress {plane_stress}): |K v − FD| / |K v| = {rel:e}"
        );
    }
}

/// Objectivity: `u_R = R (X + u) − X` rotates the internal force node by
/// node and keeps the energy (θ = 1 rad and 2.5 rad).
#[test]
fn internal_force_rotates_and_energy_is_invariant_under_rotation() {
    for name in [
        "triangle_6",
        "quadrangle_8",
        "tetrahedron_10",
        "hexahedron_8",
    ] {
        let mut m = model(name, false);
        let dim = m.spatial_dimension();
        let u = nonlinear_field(&m, 0.05, 3);
        let f0 = internal_force_at(&mut m, &u);
        let e0 = m.energy("potential").unwrap();
        let x = nodes(&m);
        for theta in [1.0_f64, 2.5] {
            let (c, s) = (theta.cos(), theta.sin());
            let rot = |v: &[Real]| -> Vec<Real> {
                if dim == 2 {
                    vec![c * v[0] - s * v[1], s * v[0] + c * v[1]]
                } else {
                    vec![v[0], c * v[1] - s * v[2], s * v[1] + c * v[2]]
                }
            };
            let ur: Vec<Real> = u
                .chunks_exact(dim)
                .zip(x.chunks_exact(dim))
                .flat_map(|(ui, xi)| {
                    let p: Vec<Real> = xi.iter().zip(ui).map(|(a, b)| a + b).collect();
                    let rp = rot(&p);
                    (0..dim).map(move |k| rp[k] - xi[k]).collect::<Vec<_>>()
                })
                .collect();
            let fr = internal_force_at(&mut m, &ur);
            let er = m.energy("potential").unwrap();
            let rf0: Vec<Real> = f0.chunks_exact(dim).flat_map(&rot).collect();
            let diff: Vec<Real> = fr.iter().zip(&rf0).map(|(a, b)| a - b).collect();
            let rel = norm(&diff) / norm(&f0);
            let rel_e = (er - e0).abs() / e0.abs();
            eprintln!("objectivity {name} θ={theta}: f {rel:e} e {rel_e:e}");
            assert!(
                rel < 1e-12,
                "{name} θ = {theta}: |f(u_R) − R f(u)| / |f| = {rel:e}"
            );
            assert!(rel_e < 1e-12, "{name} θ = {theta}: energy {e0} vs {er}");
        }
    }
}
