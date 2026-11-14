// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Independent
// mathematical checks of the total-Lagrangian finite-deformation assembly
// (SMM-023; upstream src/model/solid_mechanics/material.cc
// assembleInternalForcesFiniteDeformation / assembleStiffnessMatrixNL / L2):
// consistency of the tangent with finite differences of the internal force,
// objectivity under a large rigid rotation (internal force rotates, energy
// invariant; same intent as test/test_model/test_solid_mechanics_model/
// test_materials/test_finite_deformation.cc), small-strain limit, and the
// eigenstrain rule of the stiffness (UD-208 extended to the L2 part).
// Tolerances: docs/NUMERICAL_VALIDATION.md "Finite-deformation assembly
// (SMM-023)".

mod model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use akantu_mesh::Mesh;
use akantu_model::ModelOptions;
use akantu_solid::SolidMechanicsModel;
use akantu_validation::fixtures_root;
use akantu_validation::reduce::NanMinMax;
use model_common::*;

const TYPES: [&str; 12] = [
    "segment_2",
    "segment_3",
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
];

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

fn material(finite: bool, eigen: Option<&str>) -> String {
    format!(
        "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n  \
         finite_deformation = {}\n{}]\n",
        if finite { "true" } else { "false" },
        eigen
            .map(|e| format!("  eigen_grad_u = {e}\n"))
            .unwrap_or_default()
    )
}

fn model(name: &str, finite: bool, eigen: Option<&str>) -> SolidMechanicsModel {
    let mut m = model_with(patch_mesh(name), &material(finite, eigen));
    m.init_full(&ModelOptions::new(AnalysisMethod::Static))
        .unwrap();
    m
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

/// A smooth, genuinely nonlinear displacement field of amplitude `a`
/// (stretch + shear + bending-like quadratic terms + noise).
fn set_field(m: &mut SolidMechanicsModel, a: Real, seed: u64) {
    let dim = m.spatial_dimension();
    let x = nodes(m);
    let z = noise(x.len(), seed);
    let mut u = m.displacement_mut().unwrap();
    let u = u.as_mut_slice();
    for (n, (ui, xi)) in u.chunks_exact_mut(dim).zip(x.chunks_exact(dim)).enumerate() {
        for c in 0..dim {
            let mut v = 0.0;
            for (k, &xk) in xi.iter().enumerate() {
                v += (0.3 + 0.1 * (c + k) as Real) * xk + 0.2 * xk * xk * (c as Real - 0.5);
            }
            ui[c] = a * (v + 0.05 * z[n * dim + c]);
        }
    }
}

fn norm(v: &[Real]) -> Real {
    v.iter().map(|x| x * x).sum::<Real>().sqrt()
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

fn internal_force_at(m: &mut SolidMechanicsModel, u: &[Real]) -> Vec<Real> {
    m.displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(u);
    m.assemble_internal_forces().unwrap();
    m.internal_force().unwrap().as_slice().to_vec()
}

/// Central difference `−(f(u + h v) − f(u − h v)) / 2h` (`f_int = −∫ Bᵀ…`).
fn fd_tangent(m: &mut SolidMechanicsModel, v: &[Real], h: Real) -> Vec<Real> {
    let u0 = m.displacement().unwrap().as_slice().to_vec();
    let up: Vec<Real> = u0.iter().zip(v).map(|(u, v)| u + h * v).collect();
    let um: Vec<Real> = u0.iter().zip(v).map(|(u, v)| u - h * v).collect();
    let fp = internal_force_at(m, &up);
    let fm = internal_force_at(m, &um);
    internal_force_at(m, &u0);
    fp.iter()
        .zip(&fm)
        .map(|(p, q)| -(p - q) / (2.0 * h))
        .collect()
}

/// Relative mismatch of the assembled tangent and the central difference
/// of the internal force at a nonlinear state of amplitude `a`.
fn tangent_mismatch(name: &str, a: Real, eigen: Option<&str>) -> Real {
    let mut m = model(name, true, eigen);
    set_field(&mut m, a, 7);
    m.assemble_internal_forces().unwrap();
    m.assemble_stiffness_matrix(true).unwrap();
    let n = m.displacement().unwrap().len() * m.spatial_dimension();
    let v = noise(n, 11);
    let kv = k_times(&m, &v);
    // h·|v| ~ 1e-6 of the displacement scale: truncation O(h²) ≪ rounding
    let fd = fd_tangent(&mut m, &v, 1e-6 * a.max(1e-3));
    let diff: Vec<Real> = kv.iter().zip(&fd).map(|(x, y)| x - y).collect();
    norm(&diff) / norm(&kv)
}

#[test]
fn fd_tangent_is_the_derivative_of_the_internal_force() {
    for name in TYPES {
        let rel = tangent_mismatch(name, 0.1, None);
        eprintln!("tangent {name}: {rel:e}");
        assert!(rel < 1e-8, "{name}: |K v − FD| / |K v| = {rel:e}");
    }
}

/// With `eigen_grad_u`, the default mode evaluates the `L2` part at
/// `∇u − ε⁰` (the strain the internal force uses): the tangent stays the
/// derivative of the internal force. Upstream evaluates `B_L1` at `∇u`
/// (UD-208), which is not the derivative (reproduced in compat mode).
#[test]
fn fd_tangent_with_eigen_grad_u_default_and_upstream_compat() {
    let eigen = Some("[[0.01, 0.002], [0.003, -0.004]]");
    for name in ["triangle_3", "quadrangle_8"] {
        let rel = tangent_mismatch(name, 0.1, eigen);
        assert!(rel < 1e-8, "{name}: default mode, rel = {rel:e}");
        let mut m = model(name, true, eigen);
        m.physics
            .upstream_compat
            .stiffness_grad_u_ignores_eigen_grad_u = true;
        set_field(&mut m, 0.1, 7);
        m.assemble_internal_forces().unwrap();
        m.assemble_stiffness_matrix(true).unwrap();
        let n = m.displacement().unwrap().len() * 2;
        let v = noise(n, 11);
        let kv = k_times(&m, &v);
        let fd = fd_tangent(&mut m, &v, 1e-7);
        let diff: Vec<Real> = kv.iter().zip(&fd).map(|(x, y)| x - y).collect();
        let rel_up = norm(&diff) / norm(&kv);
        eprintln!("eigen {name}: default {rel:e} upstream {rel_up:e}");
        assert!(
            rel_up > 1e-4,
            "{name}: upstream mode should be off by O(|ε⁰|), rel = {rel_up:e}"
        );
    }
}

/// Rotation matrix (column-major `dim × dim`) by `theta` about axis x
/// (3-D) or z (2-D).
fn rotation(dim: usize, theta: Real) -> Vec<Real> {
    let (c, s) = (theta.cos(), theta.sin());
    match dim {
        2 => vec![c, s, -s, c],
        3 => vec![1.0, 0.0, 0.0, 0.0, c, s, 0.0, -s, c],
        _ => unreachable!(),
    }
}

fn rotate(r: &[Real], dim: usize, v: &[Real]) -> Vec<Real> {
    (0..dim)
        .map(|i| (0..dim).map(|k| r[i + k * dim] * v[k]).sum())
        .collect()
}

/// Objectivity (test_finite_deformation.cc intent, every 2-D/3-D type):
/// with `u_R = R (X + u) − X`, the internal force rotates
/// (`f(u_R) = R f(u)` per node) and the potential energy is unchanged.
#[test]
fn internal_force_rotates_and_energy_is_invariant_under_large_rotation() {
    for name in TYPES.iter().filter(|n| !n.starts_with("segment")) {
        let mut m = model(name, true, None);
        let dim = m.spatial_dimension();
        set_field(&mut m, 0.05, 3);
        let u = m.displacement().unwrap().as_slice().to_vec();
        let f0 = internal_force_at(&mut m, &u);
        let e0 = m.energy("potential").unwrap();
        let x = nodes(&m);
        for theta in [std::f64::consts::FRAC_PI_4, 2.5] {
            let r = rotation(dim, theta);
            let mut ur = vec![0.0; u.len()];
            for ((o, ui), xi) in ur
                .chunks_exact_mut(dim)
                .zip(u.chunks_exact(dim))
                .zip(x.chunks_exact(dim))
            {
                let p: Vec<Real> = xi.iter().zip(ui).map(|(a, b)| a + b).collect();
                let rp = rotate(&r, dim, &p);
                for c in 0..dim {
                    o[c] = rp[c] - xi[c];
                }
            }
            let fr = internal_force_at(&mut m, &ur);
            let er = m.energy("potential").unwrap();
            let rf0: Vec<Real> = f0
                .chunks_exact(dim)
                .flat_map(|fi| rotate(&r, dim, fi))
                .collect();
            let diff: Vec<Real> = fr.iter().zip(&rf0).map(|(a, b)| a - b).collect();
            let rel = norm(&diff) / norm(&f0);
            assert!(
                rel < 1e-12,
                "{name}, θ = {theta}: |f(u_R) − R f(u)| / |f| = {rel:e}"
            );
            let rel_e = (er - e0).abs() / e0.abs();
            eprintln!("objectivity {name} {theta}: f {rel:e} e {rel_e:e}");
            assert!(rel_e < 1e-12, "{name}, θ = {theta}: energy {e0} vs {er}");
            // and the rotated state is not trivially the same state
            assert!(
                norm(&fr.iter().zip(&f0).map(|(a, b)| a - b).collect::<Vec<_>>())
                    > 1e-3 * norm(&f0)
            );
        }
    }
}

/// A rigid rotation alone is stress free: zero internal force (the
/// small-strain path would not be).
#[test]
fn rigid_rotation_is_stress_free() {
    for name in [
        "triangle_6",
        "quadrangle_4",
        "tetrahedron_10",
        "hexahedron_8",
    ] {
        let mut m = model(name, true, None);
        let dim = m.spatial_dimension();
        let r = rotation(dim, 1.0);
        let x = nodes(&m);
        let u: Vec<Real> = x
            .chunks_exact(dim)
            .flat_map(|xi| {
                let rx = rotate(&r, dim, xi);
                (0..dim).map(move |c| rx[c] - xi[c]).collect::<Vec<_>>()
            })
            .collect();
        let f = internal_force_at(&mut m, &u);
        let fmax = f.iter().fold(0.0_f64, |a, b| a.nan_max(b.abs()));
        eprintln!("rigid {name}: {fmax:e}");
        // E · |u| · h^{dim−2}… scale: E = 2.1e11, |∇u| ~ 1 → rounding ~ E·ε
        assert!(fmax < 2.1e11 * 1e-13, "{name}: max |f_int| = {fmax:e}");
        assert!(m.energy("potential").unwrap().abs() < 2.1e11 * 1e-26 + 1e-3);
    }
}

/// Small-strain limit: at `u = 0` the finite-deformation stiffness is the
/// small-strain stiffness (`B_L1 = 0`, `S = 0`); for a displacement of
/// amplitude `a` the internal forces agree to `O(a)` relatively.
#[test]
fn small_strain_limit_equals_the_linear_path() {
    for name in TYPES {
        let mut fd = model(name, true, None);
        let mut lin = model(name, false, None);
        fd.assemble_internal_forces().unwrap();
        lin.assemble_internal_forces().unwrap();
        fd.assemble_stiffness_matrix(true).unwrap();
        lin.assemble_stiffness_matrix(true).unwrap();
        let n = fd.displacement().unwrap().len() * fd.spatial_dimension();
        let v = noise(n, 5);
        let (kf, kl) = (k_times(&fd, &v), k_times(&lin, &v));
        let diff: Vec<Real> = kf.iter().zip(&kl).map(|(a, b)| a - b).collect();
        let rel = norm(&diff) / norm(&kl);
        eprintln!("K(0) {name}: {rel:e}");
        assert!(rel < 1e-13, "{name}: K(0) FD vs linear, rel = {rel:e}");
        for a in [1e-4, 1e-6] {
            set_field(&mut fd, a, 9);
            set_field(&mut lin, a, 9);
            fd.assemble_internal_forces().unwrap();
            lin.assemble_internal_forces().unwrap();
            let ff = fd.internal_force().unwrap().as_slice().to_vec();
            let fl = lin.internal_force().unwrap().as_slice().to_vec();
            let diff: Vec<Real> = ff.iter().zip(&fl).map(|(x, y)| x - y).collect();
            let rel = norm(&diff) / norm(&fl);
            eprintln!("limit {name} {a}: {rel:e}");
            // the geometric nonlinearity is O(|∇u|) ~ a (field gradient ≲ 1.5 a)
            assert!(rel < 5.0 * a, "{name}, a = {a}: rel = {rel:e}");
            assert!(rel > 0.0 || a < 1e-12);
        }
    }
}
