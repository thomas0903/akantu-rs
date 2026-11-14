// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): checks of the lumped
// rotary inertia of 3-D Bernoulli beams with the diagonal
// `LumpedRotaryInertia::IsotropicMax` option (UD-059: the interim rule
// decided, an option since D-059;
// structural_mechanics_model_mass.cc assembleLumpedMassMatrix is the
// upstream counterpart, whose lumped mass is identically zero, UD-005).
// No upstream test. The default exact nodal block (D-059) is checked in
// lumped_rotary_blocks.rs.

//! The element-frame coefficients are the HRZ values `J_b = m L²/78` per
//! bending rotation and `J_t = ρ I_p L/2` for the torsion; with
//! `IsotropicMax` each element contributes the **isotropic** inertia
//! `max(J_t, J_b)` to all three rotational DOFs of both its nodes
//! (frame-independent, diagonal). Checked here:
//!
//! * frame independence: the same shaft along x, (1,1,0), (1,1,1) and
//!   (0.3,−0.5,0.8) has the same lumped mass and the same lumped spectrum
//!   (the former "diagonal of `Rᵀ diag(J) R`"
//!   changed the spectrum by 45 %);
//! * exact nodal values for `J_b > J_t` and `J_t > J_b` on inclined beams,
//!   positivity (also with `I_y = I_z = 0`), total mass;
//! * convergence of the bending and torsional frequencies with mesh
//!   refinement (orders and the regimes of the isotropic choice);
//! * the stable time step: the lumped spectrum's maximum lies below the
//!   consistent one, and the isotropic inertia is entry-wise at least the
//!   former diagonal (so it never shortens the stable step compared with
//!   that rejected rule).

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Idx, Real};
use akantu_mesh::Mesh;
use akantu_structural::{
    LumpedRotaryInertia, StructuralMaterial, StructuralMechanicsModel,
    StructuralModelUpstreamCompat,
};
use structural_model_common::*;

use std::f64::consts::PI;

const NDOF: usize = 6;

/// The corrected default except for the rotary inertia: `IsotropicMax`.
fn iso() -> StructuralModelUpstreamCompat {
    StructuralModelUpstreamCompat {
        lumped_rotary_inertia: LumpedRotaryInertia::IsotropicMax,
        ..StructuralModelUpstreamCompat::default()
    }
}

/// An `IsotropicMax` model of `mesh`.
fn iso_model(mesh: Mesh, id: &str) -> StructuralMechanicsModel {
    StructuralMechanicsModel::with_compat(shared(mesh), None, id, iso()).unwrap()
}

/// Circular steel shaft of radius `r` (`I_y = I_z = π r⁴/4`, `I_p = J`).
fn shaft_material(r: Real) -> StructuralMaterial {
    let i = PI * r.powi(4) / 4.;
    let (e, g) = (2.1e11, 8.1e10);
    StructuralMaterial {
        e,
        a: PI * r * r,
        iz: i,
        iy: i,
        gj: g * 2. * i,
        rho: 7800.,
        ..StructuralMaterial::default()
    }
}

fn cross(a: [Real; 3], b: [Real; 3]) -> [Real; 3] {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn unit(a: [Real; 3]) -> [Real; 3] {
    let n = (a[0] * a[0] + a[1] * a[1] + a[2] * a[2]).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

/// Explicit-lumped model of a straight shaft of `ne` elements, length
/// `len`, along `dir` from `(0.1, −0.2, 0.3)`, clamped at the first node.
fn shaft(
    dir: [Real; 3],
    ne: usize,
    len: Real,
    mat: StructuralMaterial,
) -> StructuralMechanicsModel {
    let e1 = unit(dir);
    let helper = if e1[2].abs() < 0.9 {
        [0., 0., 1.]
    } else {
        [1., 0., 0.]
    };
    let n = unit(cross(e1, helper));
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        ne,
        len,
        &[0.1, -0.2, 0.3],
        &e1,
        Some(&n),
    );
    let mut m = iso_model(mesh, "shaft");
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..NDOF].fill(true);
    m
}

fn free_dofs(m: &StructuralMechanicsModel) -> Vec<usize> {
    m.blocked_dofs()
        .unwrap()
        .as_slice()
        .iter()
        .enumerate()
        .filter(|(_, b)| !**b)
        .map(|(i, _)| i)
        .collect()
}

/// Lumped and consistent spectra (ascending `ω`) of the free DOFs.
fn spectra(m: &mut StructuralMechanicsModel) -> (Vec<Real>, Vec<Real>) {
    m.assemble_matrix("K").unwrap();
    m.assemble_mass_matrix().unwrap();
    m.assemble_lumped_mass_matrix().unwrap();
    let free = free_dofs(m);
    let k = restrict(&dense(m, "K"), &free);
    let mc = restrict(&dense(m, "M"), &free);
    let lm = m.lumped_mass().unwrap().as_slice().to_vec();
    let nf = free.len();
    let mut ml = vec![vec![0.0; nf]; nf];
    for (a, &i) in free.iter().enumerate() {
        ml[a][a] = lm[i];
    }
    let wl = generalized_eigenvalues(&k, &ml)
        .into_iter()
        .map(Real::sqrt)
        .collect();
    let wc = generalized_eigenvalues(&k, &mc)
        .into_iter()
        .map(Real::sqrt)
        .collect();
    (wl, wc)
}

fn max_rel(a: &[Real], b: &[Real]) -> Real {
    assert_eq!(a.len(), b.len());
    a.iter()
        .zip(b)
        .map(|(x, y)| ((x - y) / y).abs())
        .fold(0.0, Real::max)
}

/// A rigid rotation of a structure must not
/// change its lumped mass or its lumped spectrum. Clamped–free steel shaft,
/// 6 elements, `L/r = 12.5`, `J_b/J_t = 8.0`.
#[test]
fn lumped_spectrum_is_invariant_under_a_rigid_rotation_of_the_structure() {
    let mat = shaft_material(0.02);
    let (ne, len) = (6, 1.5);
    let dirs = [
        ("x", [1., 0., 0.]),
        ("(1,1,0)", [1., 1., 0.]),
        ("(1,1,1)", [1., 1., 1.]),
        ("(0.3,-0.5,0.8)", [0.3, -0.5, 0.8]),
    ];
    let mut base: Option<(Vec<Real>, Vec<Real>, Vec<Real>)> = None;
    for (name, d) in dirs {
        let mut m = shaft(d, ne, len, mat);
        let (wl, wc) = spectra(&mut m);
        let lm = m.lumped_mass().unwrap().as_slice().to_vec();
        // every node's three rotational entries are equal (isotropic)
        for n in 0..=ne {
            let r = &lm[n * NDOF + 3..(n + 1) * NDOF];
            assert!(
                r[0] == r[1] && r[1] == r[2],
                "axis {name}, node {n}: rotary inertia not isotropic: {r:?}"
            );
        }
        match &base {
            None => base = Some((lm, wl, wc)),
            Some((l0, w0, c0)) => {
                // element lengths come from rotated coordinates: a few ulps
                let dl = max_rel(&lm, l0);
                let dw = max_rel(&wl, w0);
                let dc = max_rel(&wc, c0);
                eprintln!(
                    "axis {name}: max rel change vs x: lumped mass {dl:.2e}, lumped spectrum \
                     {dw:.2e}, consistent spectrum {dc:.2e}"
                );
                assert!(dl <= 1e-14, "axis {name}: lumped mass changed by {dl:e}");
                assert!(dw <= 1e-9, "axis {name}: lumped spectrum changed by {dw:e}");
                assert!(
                    dc <= 1e-9,
                    "axis {name}: consistent spectrum changed by {dc:e}"
                );
            }
        }
    }
}

/// Exact nodal values on an inclined two-element beam, in both regimes:
/// slender elements (`J_b > J_t`: every rotation gets `m L²/78`) and short
/// stocky elements (`J_t > J_b`: every rotation gets `ρ I_p L/2`).
#[test]
fn lumped_values_are_the_isotropic_maximum_in_both_regimes() {
    let dir = unit([0.48, -0.36, 0.8]);
    let normal = unit(cross(dir, [0., 1., 0.]));
    for (case, lens, jt_dominates) in [
        ("slender", [2.0, 3.0], false),
        ("stocky", [0.02, 0.03], true),
    ] {
        let mat = StructuralMaterial {
            e: 1.,
            rho: 2.5,
            a: 0.04,
            iz: 2e-4,
            iy: 6e-4,
            gj: 1.,
            ..StructuralMaterial::default()
        };
        let x0 = [0.3, 0.1, -0.2];
        let mut x = vec![x0.to_vec()];
        let mut s = 0.0;
        for l in lens {
            s += l;
            x.push((0..3).map(|d| x0[d] + s * dir[d]).collect());
        }
        let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
        let normals = vec![normal.to_vec(); 2];
        let mesh = mesh_of(3, ElementType::BernoulliBeam3, &x, &conn, Some(&normals));
        let mut m = iso_model(mesh, "values");
        m.add_material(mat, "").unwrap();
        m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        m.assemble_lumped_mass_matrix().unwrap();
        let lm = m.lumped_mass().unwrap().as_slice().to_vec();
        let ip = mat.iy + mat.iz;
        let el: Vec<(Real, Real, Real)> = lens
            .iter()
            .map(|&l| {
                let mass = mat.rho * mat.a * l;
                let jt = mat.rho * ip * l / 2.;
                let jb = mass * l * l / 78.;
                assert_eq!(jt > jb, jt_dominates, "{case}: regime");
                (mass / 2., jt, jb)
            })
            .collect();
        let want_node = |n: usize| -> (Real, Real) {
            let mut t = 0.0;
            let mut r = 0.0;
            for (e, &(half, jt, jb)) in el.iter().enumerate() {
                if n == e || n == e + 1 {
                    t += half;
                    r += jt.max(jb);
                }
            }
            (t, r)
        };
        for n in 0..3 {
            let (t, r) = want_node(n);
            for d in 0..3 {
                let got = lm[n * NDOF + d];
                assert!(
                    (got - t).abs() <= 4. * f64::EPSILON * t,
                    "{case}: node {n} translation {d}: {got} vs {t}"
                );
                let got = lm[n * NDOF + 3 + d];
                assert!(
                    (got - r).abs() <= 4. * f64::EPSILON * r,
                    "{case}: node {n} rotation {d}: {got} vs {r}"
                );
            }
        }
        // totals: translation = Σ ρAL per direction; rotation = Σ 2 max(J)
        let total_t: Real = (0..3).map(|n| lm[n * NDOF]).sum();
        let want_t: Real = lens.iter().map(|l| mat.rho * mat.a * l).sum();
        assert!((total_t - want_t).abs() <= 1e-15 * want_t * 4., "{case}");
        let total_r: Real = (0..3).map(|n| lm[n * NDOF + 3]).sum();
        let want_r: Real = el.iter().map(|&(_, jt, jb)| 2. * jt.max(jb)).sum();
        assert!((total_r - want_r).abs() <= 1e-15 * want_r * 4., "{case}");
    }
}

/// Positive definite for `ρ A > 0`, also when the section has no polar
/// moment (`I_y = I_z = 0`, `J_t = 0`: the bending value is used for the
/// three rotations).
#[test]
fn lumped_mass_is_positive_without_polar_moment() {
    let mat = StructuralMaterial {
        e: 1.,
        rho: 3.,
        a: 0.2,
        gj: 1.,
        ..StructuralMaterial::default()
    };
    let mut m = shaft([0.2, 0.9, -0.4], 3, 1.2, mat);
    m.assemble_lumped_mass_matrix().unwrap();
    let lm = m.lumped_mass().unwrap().as_slice().to_vec();
    assert!(lm.iter().all(|v| *v > 0.0), "{lm:?}");
    let le: Real = 0.4;
    let jb = mat.rho * mat.a * le * le * le / 78.;
    let tip = &lm[3 * NDOF + 3..4 * NDOF];
    for v in tip {
        assert!((v - jb).abs() <= 4. * f64::EPSILON * jb, "{v} vs {jb}");
    }
}

/// Torsional fundamental of a clamped–free shaft with the isotropic
/// lumped inertia. Fine meshes (`h² < 39 I_p/A`, `J_t ≥ J_b`): exact HRZ
/// torsion, `O(h²)` convergence from below. Coarse meshes (`J_b > J_t`):
/// the torsional inertia is over-estimated by `J_b/J_t = A h²/(39 I_p)`
/// and the frequency is low by about `√(J_t/J_b)` (documented, recorded).
#[test]
fn torsional_frequency_convergence_with_the_isotropic_inertia() {
    let r = 0.02;
    let mat = shaft_material(r);
    let len = 1.5;
    let ip = mat.iy + mat.iz;
    let w_t = PI / (2. * len) * (mat.gj / (mat.rho * ip)).sqrt();
    let h_switch = (39. * ip / mat.a).sqrt();
    // isolate the torsion: every DOF blocked except θx along an x shaft
    let torsion = |ne: usize| -> Real {
        let mut m = shaft([1., 0., 0.], ne, len, mat);
        {
            let mut b_g = m.blocked_dofs_mut().unwrap();
            let b = b_g.as_mut_slice();
            for (k, v) in b.iter_mut().enumerate() {
                *v = k < NDOF || k % NDOF != 3;
            }
        }
        spectra(&mut m).0[0]
    };
    let mut prev: Option<Real> = None;
    for ne in [24, 48, 96] {
        let h = len / ne as Real;
        assert!(h < h_switch, "fine regime");
        let w = torsion(ne);
        let err = (w - w_t) / w_t;
        eprintln!(
            "torsion, {ne} elements (h/h* = {:.3}): rel error {err:+.3e}",
            h / h_switch
        );
        assert!(err < 0.0, "lumped torsion from below: {err:e}");
        if let Some(p) = prev {
            let ratio = p / err;
            assert!(
                (3.8..4.2).contains(&ratio),
                "O(h²): ratio {ratio} ({p:e} → {err:e})"
            );
        }
        prev = Some(err);
    }
    for ne in [3, 6, 12] {
        let h = len / ne as Real;
        let w = torsion(ne);
        let el = mat.rho * mat.a * h;
        let ratio = (el * h * h / 78.) / (mat.rho * ip * h / 2.);
        eprintln!(
            "torsion, {ne} elements (h/h* = {:.3}, J_b/J_t = {ratio:.3}): ω = {w:.6e}, rel error \
             {:+.3e}",
            h / h_switch,
            (w - w_t) / w_t
        );
        assert!(ratio > 1.0 && w < w_t);
    }
}

/// Bending fundamentals of a simply supported slender 3-D beam (axial and
/// torsion DOFs blocked) with the isotropic lumped inertia, in the regime
/// `J_b > J_t` (`h² > 39 I_p/A`): HRZ bending values, `O(h²)` convergence
/// from below in both planes.
#[test]
fn bending_frequency_convergence_with_the_isotropic_inertia() {
    let mat = StructuralMaterial {
        e: 1.0,
        rho: 1.0,
        a: 0.01,
        iz: 1e-5,
        iy: 4e-6,
        gj: 3e-6,
        ..StructuralMaterial::default()
    };
    let len = 10.0;
    let h_switch = (39. * (mat.iy + mat.iz) / mat.a).sqrt();
    let omega = |ei: Real| (PI / len).powi(2) * (ei / (mat.rho * mat.a)).sqrt();
    let (wy, wz) = (omega(mat.e * mat.iy), omega(mat.e * mat.iz));
    let mut prev: Option<(Real, Real)> = None;
    for ne in [8, 16, 32] {
        let h = len / ne as Real;
        assert!(h > h_switch, "J_b > J_t regime: h = {h}, h* = {h_switch}");
        let mesh = line_mesh(
            3,
            ElementType::BernoulliBeam3,
            ne,
            len,
            &[0., 0., 0.],
            &[1., 0., 0.],
            Some(&[0., 0., 1.]),
        );
        let mut m = iso_model(mesh, "bending");
        m.add_material(mat, "").unwrap();
        m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        {
            let mut b_g = m.blocked_dofs_mut().unwrap();
            let b = b_g.as_mut_slice();
            for n in 0..=ne {
                b[n * NDOF] = true;
                b[n * NDOF + 3] = true;
            }
            for n in [0, ne] {
                b[n * NDOF + 1] = true;
                b[n * NDOF + 2] = true;
            }
        }
        let wl = spectra(&mut m).0;
        let ey = (wl[0] - wy) / wy;
        let ez = wl
            .iter()
            .map(|w| (w - wz) / wz)
            .min_by(|a, b| a.abs().total_cmp(&b.abs()))
            .unwrap();
        eprintln!("bending, {ne} elements: rel errors x–z {ey:+.3e}, x–y {ez:+.3e}");
        assert!(ey < 0.0 && ez < 0.0, "from below");
        if let Some((py, pz)) = prev {
            for (p, e) in [(py, ey), (pz, ez)] {
                let ratio = p / e;
                assert!((3.5..4.5).contains(&ratio), "O(h²): ratio {ratio}");
            }
        }
        prev = Some((ey, ez));
    }
}

/// Stable time step: on a 3-D frame in general orientation the largest
/// lumped frequency lies below the consistent one, and the isotropic
/// inertia is entry-wise at least the former frame-dependent diagonal
/// `Σ_k R_ki² J_k` (so, by the Rayleigh quotient, its `ω_max` is not larger:
/// the stable step is never shortened).
#[test]
fn isotropic_inertia_never_shortens_the_stable_time_step() {
    let nodes = vec![
        vec![0., 0., 0.],
        vec![1.1, 0.4, 0.9],
        vec![2.0, -0.3, 1.7],
        vec![1.3, 1.6, 2.4],
        vec![0.2, 2.2, 1.1],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2], vec![1, 3], vec![3, 4], vec![2, 3]];
    let helpers = [
        [0.2, 0.9, -0.4],
        [0.7, -0.1, 0.5],
        [-0.3, 0.2, 0.9],
        [0.6, 0.6, -0.5],
        [0.1, -0.8, 0.3],
    ];
    let normals: Vec<Vec<Real>> = conn
        .iter()
        .zip(helpers)
        .map(|(c, h)| {
            let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
            unit(cross([b[0] - a[0], b[1] - a[1], b[2] - a[2]], h)).to_vec()
        })
        .collect();
    let mat = shaft_material(0.05);
    let mesh: Mesh = mesh_of(
        3,
        ElementType::BernoulliBeam3,
        &nodes,
        &conn,
        Some(&normals),
    );
    let mut m = iso_model(mesh, "frame");
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..NDOF].fill(true);
    let (wl, wc) = spectra(&mut m);
    let (lmax, cmax) = (*wl.last().unwrap(), *wc.last().unwrap());
    eprintln!(
        "frame: ω_max lumped {lmax:.6e}, consistent {cmax:.6e}, stable-step ratio {:.3}",
        cmax / lmax
    );
    assert!(lmax < cmax, "lumped ω_max {lmax} ≥ consistent {cmax}");
    // former diagonal Σ_k R_ki² J_k (local diag (J_t, J_b, J_b))
    let lm = m.lumped_mass().unwrap().as_slice().to_vec();
    let rot = m
        .rotations(ElementType::BernoulliBeam3, G)
        .unwrap()
        .as_slice()
        .to_vec();
    let mut former = vec![0.0; nodes.len() * 3];
    for (e, c) in conn.iter().enumerate() {
        let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
        let l = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2) + (b[2] - a[2]).powi(2)).sqrt();
        let mass = mat.rho * mat.a * l;
        let j = [
            mat.rho * (mat.iy + mat.iz) * l / 2.,
            mass * l * l / 78.,
            mass * l * l / 78.,
        ];
        let r = &rot[e * NDOF * NDOF..(e + 1) * NDOF * NDOF];
        for i in 0..3 {
            let ji: Real = (0..3).map(|k| r[k + i * NDOF].powi(2) * j[k]).sum();
            for &n in c {
                former[n as usize * 3 + i] += ji;
            }
        }
    }
    for n in 0..nodes.len() {
        for i in 0..3 {
            let iso = lm[n * NDOF + 3 + i];
            let old = former[n * 3 + i];
            assert!(
                iso >= old * (1. - 1e-14),
                "node {n} rotation {i}: isotropic {iso} < former {old}"
            );
        }
    }
}

/// Fine regime (`h² < 39 I_p/A`, every rotation gets `J_t = ρ I_p h/2`):
/// the isotropic inertia adds a distributed rotary inertia `ρ I_p` to the
/// bending rotations, so the lumped bending frequencies converge to the
/// Rayleigh-beam value `ω_R = ω_EB / √(1 + k² I_p/A)` of a simply
/// supported beam, not to the Euler–Bernoulli `ω_EB`: a consistency error
/// `≈ −k² I_p/(2A)` (here −4.38e-4, the size of the rotary-inertia effect
/// Euler–Bernoulli theory neglects). Measured: the distance to `ω_R`
/// shrinks by ≈ 16 per halving of `h` (−2.0e-7, −1.3e-8, …: the uniform
/// nodal masses of a sinusoidal mode; asserted as at least 12), while the
/// distance to `ω_EB` stays at the Rayleigh offset.
#[test]
fn bending_frequencies_in_the_fine_regime_converge_to_the_rayleigh_limit() {
    let r = 0.02;
    let mat = shaft_material(r);
    let len = 1.5;
    let ip = mat.iy + mat.iz;
    let h_switch = (39. * ip / mat.a).sqrt();
    let k = PI / len;
    let w_eb = k * k * (mat.e * mat.iy / (mat.rho * mat.a)).sqrt();
    let w_r = w_eb / (1. + k * k * ip / mat.a).sqrt();
    let mut prev: Option<Real> = None;
    for ne in [24, 48] {
        let h = len / ne as Real;
        assert!(h < h_switch, "fine regime");
        let mesh = line_mesh(
            3,
            ElementType::BernoulliBeam3,
            ne,
            len,
            &[0., 0., 0.],
            &[1., 0., 0.],
            Some(&[0., 0., 1.]),
        );
        let mut m = iso_model(mesh, "rayleigh");
        m.add_material(mat, "").unwrap();
        m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        {
            let mut b_g = m.blocked_dofs_mut().unwrap();
            let b = b_g.as_mut_slice();
            for n in 0..=ne {
                b[n * NDOF] = true;
                b[n * NDOF + 3] = true;
            }
            for n in [0, ne] {
                b[n * NDOF + 1] = true;
                b[n * NDOF + 2] = true;
            }
        }
        let w = spectra(&mut m).0[0];
        let (e_eb, e_r) = ((w - w_eb) / w_eb, (w - w_r) / w_r);
        eprintln!(
            "bending, fine regime, {ne} elements (h/h* = {:.3}): rel error vs ω_EB {e_eb:+.3e}, \
             vs ω_R {e_r:+.3e} (ω_R/ω_EB − 1 = {:+.3e})",
            h / h_switch,
            w_r / w_eb - 1.
        );
        assert!(e_r < 0.0, "below ω_R: {e_r:e}");
        // the limit is ω_R, not ω_EB
        let offset = w_r / w_eb - 1.;
        assert!(
            (e_eb - offset).abs() <= 1e-3 * offset.abs(),
            "error vs ω_EB {e_eb:e} should approach the Rayleigh offset {offset:e}"
        );
        if let Some(p) = prev {
            let ratio = p / e_r;
            assert!(ratio >= 12.0, "convergence to ω_R: ratio {ratio}");
        }
        prev = Some(e_r);
    }
}
