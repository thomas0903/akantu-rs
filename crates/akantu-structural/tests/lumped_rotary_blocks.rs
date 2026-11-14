// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): checks of the exact nodal
// 3×3 lumped rotary inertia of 3-D Bernoulli beams (D-059, the default
// `LumpedRotaryInertia::Block3x3`) and of the `IsotropicMax` option
// (UD-059, corrected without a compat switch);
// structural_mechanics_model_mass.cc assembleLumpedMassMatrix is the
// upstream counterpart (its lumped mass is identically zero, UD-005).
// No upstream test: independent checks for STRUCT-008, STRUCT-010 and
// STRUCT-011, and the D-059 measurements.

//! * STRUCT-010: the nodal block of a two-element rotated beam equals the
//!   closed form `Σ_e (J_t e₁e₁ᵀ + J_b (e₂e₂ᵀ + e₃e₃ᵀ))` built from the node
//!   coordinates and normals (independent of the FE engine's frames);
//! * STRUCT-011: `E_kin` of a rigid rotation `ω` equals `½ Σ m|v|² +
//!   ½ ωᵀ (Σ Jₙ) ω`;
//! * STRUCT-008: 3-D free torsional vibration of a rotated shaft with
//!   `_explicit_lumped_mass`, step by step against the dense
//!   central-difference recurrence with the closed-form block mass, its
//!   quarter period against the closed form of the lumped chain and
//!   against the dense consistent-mass central-difference run;
//! * the torsional frequency errors D-059 quotes (closed form of the
//!   lumped chain), identical in every orientation; objectivity under a
//!   rigid rotation of the mesh; the stable time step of the two rules;
//! * the block solve with one, two and three blocked rotations at a node
//!   (prescribed accelerations on the blocked ones) against a dense
//!   reference; the definiteness refusal; the 2-D values.
//!
//! Every bound is stated as what it is: a closed-form identity to rounding
//! (with the measured level) or a regression guard (with the measured
//! margin); numbers in `docs/NUMERICAL_VALIDATION.md`.

// dense textbook algorithms read as their formulas with index loops
#![allow(clippy::needless_range_loop)]

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Error, Idx, Real};
use akantu_mesh::Mesh;
use akantu_structural::{
    LumpedRotaryInertia, StructuralMaterial, StructuralMechanicsModel,
    StructuralModelUpstreamCompat,
};
use structural_model_common::*;

use std::f64::consts::PI;

const NDOF: usize = 6;
const EPS: Real = f64::EPSILON;

type V3 = [Real; 3];

fn cross(a: V3, b: V3) -> V3 {
    [
        a[1] * b[2] - a[2] * b[1],
        a[2] * b[0] - a[0] * b[2],
        a[0] * b[1] - a[1] * b[0],
    ]
}

fn dot(a: V3, b: V3) -> Real {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}

fn unit(a: V3) -> V3 {
    let n = dot(a, a).sqrt();
    [a[0] / n, a[1] / n, a[2] / n]
}

fn sub(a: &[Real], b: &[Real]) -> V3 {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
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

/// A general section (`I_y ≠ I_z`) for the closed-form checks.
fn general_material() -> StructuralMaterial {
    StructuralMaterial {
        e: 3.,
        rho: 2.5,
        a: 0.04,
        iz: 2e-4,
        iy: 6e-4,
        gj: 1.5,
        ..StructuralMaterial::default()
    }
}

fn compat(rule: LumpedRotaryInertia) -> StructuralModelUpstreamCompat {
    StructuralModelUpstreamCompat {
        lumped_rotary_inertia: rule,
        ..StructuralModelUpstreamCompat::default()
    }
}

/// A unit normal orthogonal to the axis `e1`.
fn normal_for(e1: V3, helper: V3) -> V3 {
    unit(cross(e1, helper))
}

/// Explicit-lumped model of `mesh` with one material and `rule`.
fn model(
    mesh: Mesh,
    mat: StructuralMaterial,
    rule: LumpedRotaryInertia,
    id: &str,
) -> StructuralMechanicsModel {
    let mut m =
        StructuralMechanicsModel::with_compat(shared(mesh), None, id, compat(rule)).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    m
}

/// Closed form of the lumped mass from the geometry (independent of the
/// FE engine): per node the translational mass `Σ m/2` and the rotary
/// block `Σ_e J_t e₁e₁ᵀ + J_b (e₂e₂ᵀ + e₃e₃ᵀ)` (`e₁` the axis, `e₃` the
/// normal, `e₂ = e₃ × e₁`), row-major.
fn closed_form(
    nodes: &[Vec<Real>],
    conn: &[Vec<Idx>],
    normals: &[V3],
    mat: &StructuralMaterial,
) -> (Vec<Real>, Vec<[Real; 9]>) {
    let mut trans = vec![0.0; nodes.len()];
    let mut blocks = vec![[0.0; 9]; nodes.len()];
    for (c, &n3) in conn.iter().zip(normals) {
        let (a, b) = (c[0] as usize, c[1] as usize);
        let d = sub(&nodes[b], &nodes[a]);
        let l = dot(d, d).sqrt();
        let e1 = unit(d);
        let e2 = cross(n3, e1);
        let mass = mat.rho * mat.a * l;
        let jt = mat.rho * (mat.iy + mat.iz) * l / 2.;
        let jb = mass * l * l / 78.;
        for n in [a, b] {
            trans[n] += mass / 2.;
            for i in 0..3 {
                for j in 0..3 {
                    blocks[n][i * 3 + j] +=
                        jt * e1[i] * e1[j] + jb * (e2[i] * e2[j] + n3[i] * n3[j]);
                }
            }
        }
    }
    (trans, blocks)
}

/// Dense lumped matrix (all DOFs) of translational masses and rotary
/// blocks.
fn dense_of(trans: &[Real], blocks: &[[Real; 9]]) -> Vec<Vec<Real>> {
    let n = trans.len() * NDOF;
    let mut m = vec![vec![0.0; n]; n];
    for (k, (&t, b)) in trans.iter().zip(blocks).enumerate() {
        for d in 0..3 {
            m[k * NDOF + d][k * NDOF + d] = t;
        }
        for i in 0..3 {
            for j in 0..3 {
                m[k * NDOF + 3 + i][k * NDOF + 3 + j] = b[i * 3 + j];
            }
        }
    }
    m
}

/// Dense lumped matrix of the model's own arrays (`lumped_mass` diagonal,
/// `rotary_inertia` blocks).
fn dense_lumped(m: &StructuralMechanicsModel) -> Vec<Vec<Real>> {
    let lm = m.lumped_mass().unwrap();
    let j = m.rotary_inertia().unwrap();
    let nn = lm.len();
    let trans: Vec<Real> = (0..nn).map(|n| lm.tuple(n)[0]).collect();
    let blocks: Vec<[Real; 9]> = (0..nn).map(|n| j.tuple(n).try_into().unwrap()).collect();
    let mut d = dense_of(&trans, &blocks);
    // the translational entries as stored (each component)
    for n in 0..nn {
        for c in 0..3 {
            d[n * NDOF + c][n * NDOF + c] = lm.tuple(n)[c];
        }
    }
    d
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

/// Lumped spectrum (ascending `ω`) of the free DOFs with the model's
/// block mass.
fn lumped_spectrum(m: &mut StructuralMechanicsModel) -> Vec<Real> {
    m.assemble_matrix("K").unwrap();
    m.assemble_lumped_mass_matrix().unwrap();
    let free = free_dofs(m);
    let k = restrict(&dense(m, "K"), &free);
    let ml = restrict(&dense_lumped(m), &free);
    generalized_eigenvalues(&k, &ml)
        .into_iter()
        .map(Real::sqrt)
        .collect()
}

/// Straight shaft of `ne` elements along `dir` from `(0.1, −0.2, 0.3)`,
/// clamped at the first node; returns the model and its nodes.
fn shaft(
    dir: V3,
    ne: usize,
    len: Real,
    mat: StructuralMaterial,
    rule: LumpedRotaryInertia,
) -> (StructuralMechanicsModel, Vec<Vec<Real>>, V3) {
    let e1 = unit(dir);
    let helper = if e1[2].abs() < 0.9 {
        [0., 0., 1.]
    } else {
        [1., 0., 0.]
    };
    let n = normal_for(e1, helper);
    let origin = [0.1, -0.2, 0.3];
    let mesh = line_mesh(
        3,
        ElementType::BernoulliBeam3,
        ne,
        len,
        &origin,
        &e1,
        Some(&n),
    );
    let nodes: Vec<Vec<Real>> = (0..=ne)
        .map(|i| {
            let s = i as Real * (len / ne as Real);
            (0..3).map(|d| origin[d] + s * e1[d]).collect()
        })
        .collect();
    let mut m = model(mesh, mat, rule, "shaft");
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..NDOF].fill(true);
    (m, nodes, e1)
}

/// The two-element rotated beam of the STRUCT-010 closing test: two
/// members of different directions and normals meeting at node 1.
fn rotated_two_element_beam() -> (Vec<Vec<Real>>, Vec<Vec<Idx>>, Vec<V3>) {
    let nodes = vec![
        vec![0.3, 0.1, -0.2],
        vec![1.26, -0.62, 1.4],
        vec![1.9, 0.75, 2.3],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let normals: Vec<V3> = conn
        .iter()
        .zip([[0.2, 0.9, -0.4], [-0.7, 0.1, 0.5]])
        .map(|(c, h)| normal_for(unit(sub(&nodes[c[1] as usize], &nodes[c[0] as usize])), h))
        .collect();
    (nodes, conn, normals)
}

fn mesh_with(nodes: &[Vec<Real>], conn: &[Vec<Idx>], normals: &[V3]) -> Mesh {
    let n: Vec<Vec<Real>> = normals.iter().map(|v| v.to_vec()).collect();
    mesh_of(3, ElementType::BernoulliBeam3, nodes, conn, Some(&n))
}

/// Rigid rotation `Q` about `(1, 1, 1)/√3` by 50° (Rodrigues).
fn rotate(v: &[Real]) -> V3 {
    let th: Real = 50f64.to_radians();
    let k = [1. / 3f64.sqrt(); 3];
    let (c, s) = (th.cos(), th.sin());
    let kv = dot(k, [v[0], v[1], v[2]]);
    let kxv = cross(k, [v[0], v[1], v[2]]);
    [
        v[0] * c + kxv[0] * s + k[0] * kv * (1. - c),
        v[1] * c + kxv[1] * s + k[1] * kv * (1. - c),
        v[2] * c + kxv[2] * s + k[2] * kv * (1. - c),
    ]
}

/// The matrix of [`rotate`] (columns = images of the basis vectors).
fn rotation_matrix() -> [[Real; 3]; 3] {
    let cols = [
        rotate(&[1., 0., 0.]),
        rotate(&[0., 1., 0.]),
        rotate(&[0., 0., 1.]),
    ];
    let mut q = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            q[i][j] = cols[j][i];
        }
    }
    q
}

// ---------------------------------------------------------------------------
// STRUCT-010: the nodal block in closed form
// ---------------------------------------------------------------------------

/// Closing test, STRUCT-010: the lumped rotary blocks of a two-element
/// rotated beam (members of different directions and normals) equal the
/// closed form built from the coordinates and normals, to rounding; the
/// per-DOF lumped mass holds the translations and the block diagonals, the
/// DOF manager's lumped `M` the same values and the block couplings the
/// off-diagonal entries (bitwise).
#[test]
fn lumped_block_of_a_two_element_rotated_beam_equals_the_closed_form() {
    let (nodes, conn, normals) = rotated_two_element_beam();
    let mat = general_material();
    let mut m = model(
        mesh_with(&nodes, &conn, &normals),
        mat,
        LumpedRotaryInertia::Block3x3,
        "two",
    );
    assert_eq!(m.lumped_rotary_inertia(), LumpedRotaryInertia::Block3x3);
    m.assemble_lumped_mass_matrix().unwrap();
    let (trans, blocks) = closed_form(&nodes, &conn, &normals, &mat);
    let lm = m.lumped_mass().unwrap().as_slice().to_vec();
    let j = m.rotary_inertia().unwrap().as_slice().to_vec();
    assert_eq!(j.len(), nodes.len() * 9);
    let mut worst: Real = 0.0;
    for n in 0..nodes.len() {
        let b = &j[n * 9..(n + 1) * 9];
        let scale = blocks[n].iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        for k in 0..9 {
            let err = (b[k] - blocks[n][k]).abs() / scale;
            worst = worst.max(err);
            // closed-form identity to rounding (measured 3.4 ε)
            assert!(
                err <= 16. * EPS,
                "node {n} entry {k}: {} vs {} (rel {err:e})",
                b[k],
                blocks[n][k]
            );
        }
        for p in 0..3 {
            for q in 0..3 {
                assert_eq!(b[p * 3 + q].to_bits(), b[q * 3 + p].to_bits(), "symmetric");
            }
            // the per-DOF mass holds the block diagonal
            assert_eq!(lm[n * NDOF + 3 + p].to_bits(), b[p * 3 + p].to_bits());
            let t = lm[n * NDOF + p];
            assert!((t - trans[n]).abs() <= 4. * EPS * trans[n], "translation");
        }
    }
    eprintln!(
        "STRUCT-010 closed form: worst relative entry error {worst:.2e} ({:.1} ε)",
        worst / EPS
    );
    // the element blocks are not diagonal: the members are rotated
    assert!(j[1].abs() > 1e-3 * j[0].abs() && j[9 + 5].abs() > 1e-3 * j[9 + 4].abs());
    // the DOF manager's lumped M: the per-DOF mass and the couplings
    let dm = m.dof_manager();
    assert_eq!(
        dm.lumped_matrix("M").unwrap().values.as_slice(),
        lm.as_slice()
    );
    let c = dm
        .lumped_block_couplings("M")
        .expect("block couplings of M");
    assert_eq!(
        (c.family(), c.first_component(), c.nb_blocks()),
        ("displacement", 3, 3)
    );
    for n in 0..nodes.len() {
        let b = &j[n * 9..(n + 1) * 9];
        assert_eq!(&c.off_diagonal()[n * 3..n * 3 + 3], &[b[1], b[2], b[5]]);
        assert_eq!(
            &c.equations()[n * 3..n * 3 + 3],
            &[n * NDOF + 3, n * NDOF + 4, n * NDOF + 5]
        );
    }
}

// ---------------------------------------------------------------------------
// STRUCT-011: kinetic energy with the blocks
// ---------------------------------------------------------------------------

/// Closing test, STRUCT-011: the kinetic energy of a rigid rotation `ω`
/// (rotations `ω` at every node, translations `ω × (x − c)`) equals
/// `½ Σ mₙ |vₙ|² + ½ ωᵀ (Σ Jₙ) ω` with the closed-form blocks; with the
/// translations at rest it is `½ ωᵀ (Σ Jₙ) ω`. Closed-form identity to
/// rounding (measured ≤ 1.9e-16 relative; bound 16 ε).
#[test]
fn kinetic_energy_of_a_rigid_rotation_is_half_omega_j_omega() {
    let (mut nodes, mut conn, mut normals) = rotated_two_element_beam();
    // a third, branching member: three different frames at node 1
    nodes.push(vec![0.4, 1.7, 0.9]);
    conn.push(vec![1, 3]);
    normals.push(normal_for(
        unit(sub(&nodes[3], &nodes[1])),
        [0.3, -0.2, 1.0],
    ));
    let mat = general_material();
    let (trans, blocks) = closed_form(&nodes, &conn, &normals, &mat);
    let omega = [0.7, -1.3, 0.45];
    let centre = [0.2, 0.5, -0.1];
    let j_total: Vec<Real> = (0..9).map(|k| blocks.iter().map(|b| b[k]).sum()).collect();
    let wjw: Real = (0..3)
        .map(|i| {
            (0..3)
                .map(|j| omega[i] * j_total[i * 3 + j] * omega[j])
                .sum::<Real>()
        })
        .sum();
    for with_translation in [false, true] {
        let mut m = model(
            mesh_with(&nodes, &conn, &normals),
            mat,
            LumpedRotaryInertia::Block3x3,
            "rigid",
        );
        let mut want = 0.5 * wjw;
        {
            let mut v_g = m.velocity_mut().unwrap();
            let v = v_g.as_mut_slice();
            for (n, x) in nodes.iter().enumerate() {
                v[n * NDOF + 3..n * NDOF + 6].copy_from_slice(&omega);
                if with_translation {
                    let vt = cross(omega, sub(x, &centre));
                    v[n * NDOF..n * NDOF + 3].copy_from_slice(&vt);
                    want += 0.5 * trans[n] * dot(vt, vt);
                }
            }
        }
        // the lumped branch needs the lumped M (UD-510)
        m.assemble_lumped_mass_matrix().unwrap();
        let ek = m.kinetic_energy().unwrap();
        let rel = (ek - want).abs() / want;
        eprintln!(
            "STRUCT-011 rigid rotation (translations {with_translation}): E_kin {ek:.15e}, closed form {want:.15e}, rel {rel:.1e}"
        );
        assert!(rel <= 16. * EPS, "E_kin {ek} vs {want} (rel {rel:e})");
        // the diagonal-only sum differs (the off-diagonal terms matter)
        let diag_only = 0.5
            * m.lumped_mass()
                .unwrap()
                .as_slice()
                .iter()
                .zip(m.velocity().unwrap().as_slice())
                .map(|(a, b)| a * b * b)
                .sum::<Real>();
        assert!(
            (diag_only - want).abs() > 1e-3 * want,
            "{diag_only} vs {want}"
        );
    }
}

// ---------------------------------------------------------------------------
// STRUCT-008: explicit lumped torsional vibration
// ---------------------------------------------------------------------------

/// Dense central difference `u⁺ = u + Δt v + Δt²/2 a`, `M a⁺ = −K u⁺`,
/// `v⁺ = v + Δt/2 (a + a⁺)` on the free DOFs (Cholesky of `M` once),
/// started like the model: at rest with `a₀ = 0` (the stored acceleration;
/// the first step then gives `u₁ = u₀`).
struct DenseCd {
    k: Vec<Vec<Real>>,
    l: Vec<Vec<Real>>,
    u: Vec<Real>,
    v: Vec<Real>,
    a: Vec<Real>,
    dt: Real,
}

impl DenseCd {
    fn new(k: Vec<Vec<Real>>, m: &[Vec<Real>], u0: Vec<Real>, dt: Real) -> Self {
        let l = cholesky(m);
        let n = u0.len();
        Self {
            k,
            l,
            u: u0,
            v: vec![0.0; n],
            a: vec![0.0; n],
            dt,
        }
    }
    fn solve(&self, b: &[Real]) -> Vec<Real> {
        let n = b.len();
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut s = b[i];
            for j in 0..i {
                s -= self.l[i][j] * y[j];
            }
            y[i] = s / self.l[i][i];
        }
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut s = y[i];
            for j in i + 1..n {
                s -= self.l[j][i] * x[j];
            }
            x[i] = s / self.l[i][i];
        }
        x
    }
    fn step(&mut self) {
        let dt = self.dt;
        for i in 0..self.u.len() {
            self.u[i] += dt * self.v[i] + dt * dt / 2. * self.a[i];
        }
        let r: Vec<Real> = mat_vec(&self.k, &self.u).into_iter().map(|x| -x).collect();
        let a_new = self.solve(&r);
        for i in 0..self.u.len() {
            self.v[i] += dt / 2. * (self.a[i] + a_new[i]);
        }
        self.a = a_new;
    }
}

/// Time of the first downward zero crossing of `f` (linear interpolation).
fn crossing(prev: Real, cur: Real, t: Real, dt: Real) -> Option<Real> {
    (prev > 0.0 && cur <= 0.0).then(|| t - dt * cur / (cur - prev))
}

/// Closing test, STRUCT-008: 3-D free torsional vibration of a clamped–free
/// steel shaft along `(0.3, −0.5, 0.8)` (12 elements) with
/// `_explicit_lumped_mass`, started at rest from the twist `θ(s) =
/// θ₀ sin(πs/2L) t̂` (the first torsional mode of the lumped chain):
///
/// * every step equals the dense central-difference recurrence with the
///   closed-form block mass (measured ≤ 5.2e-15 of the amplitude; guard
///   1e-12);
/// * the motion stays a pure twist (translations and rotation components
///   normal to the axis ≤ 1e-12 θ₀; measured 1.8e-15);
/// * the first zero crossing of the tip twist is at `Δt/2 + π/(2ω̃)`,
///   `sin(ω̃Δt/2) = ω_h Δt/2`, with the lumped chain's
///   `ω_h = (2c/h) sin(π/(4N))` (central difference on one mode started
///   with `a₀ = 0`: `u_n = A cos(ω̃ (n − ½) Δt)`; linear interpolation of
///   the crossing, measured ≤ 2.1e-10, guard 1e-6);
/// * against the dense consistent-mass central-difference run (same
///   initial twist, its single-mode closed form checked the same way) the
///   period is longer by the two discretisation errors (lumped
///   −0.0714 %, consistent +0.0714 %: ratio 1.001427, the closed-form
///   ratio `ω_c/ω_h` to 2e-6).
#[test]
fn explicit_lumped_torsional_vibration_of_a_rotated_shaft() {
    let (ne, len, r) = (12, 1.5, 0.02);
    let mat = shaft_material(r);
    let (mut m, nodes, t) = shaft(
        [0.3, -0.5, 0.8],
        ne,
        len,
        mat,
        LumpedRotaryInertia::Block3x3,
    );
    m.assemble_matrix("K").unwrap();
    m.assemble_mass_matrix().unwrap();
    m.assemble_lumped_mass_matrix().unwrap();
    let free = free_dofs(&m);
    let k = restrict(&dense(&m, "K"), &free);
    let mc = restrict(&dense(&m, "M"), &free);
    let normals = vec![normal_for(t, [0., 0., 1.]); ne];
    let conn: Vec<Vec<Idx>> = (0..ne as Idx).map(|i| vec![i, i + 1]).collect();
    let (trans, blocks) = closed_form(&nodes, &conn, &normals, &mat);
    let ml = restrict(&dense_of(&trans, &blocks), &free);
    let wl: Vec<Real> = generalized_eigenvalues(&k, &ml)
        .into_iter()
        .map(Real::sqrt)
        .collect();
    let wc: Vec<Real> = generalized_eigenvalues(&k, &mc)
        .into_iter()
        .map(Real::sqrt)
        .collect();
    let dt = 0.1 * 2. / wl.last().unwrap().max(*wc.last().unwrap());
    m.set_time_step(dt, None).unwrap();
    let theta0 = 1e-3;
    let mut u0 = vec![0.0; nodes.len() * NDOF];
    for n in 0..nodes.len() {
        let s = len * n as Real / ne as Real;
        let phi = theta0 * (PI * s / (2. * len)).sin();
        for d in 0..3 {
            u0[n * NDOF + 3 + d] = phi * t[d];
        }
    }
    m.displacement_mut()
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&u0);
    let u0f: Vec<Real> = free.iter().map(|&i| u0[i]).collect();
    let mut lumped = DenseCd::new(k.clone(), &ml, u0f.clone(), dt);
    let mut consistent = DenseCd::new(k, &mc, u0f, dt);
    let tip = nodes.len() - 1;
    let twist = |u: &[Real]| dot([u[tip * NDOF + 3], u[tip * NDOF + 4], u[tip * NDOF + 5]], t);
    let tip_free = |u: &[Real]| -> Real {
        // free index of the tip's rotations
        let base = free.iter().position(|&g| g == tip * NDOF + 3).unwrap();
        dot([u[base], u[base + 1], u[base + 2]], t)
    };
    let (mut prev_m, mut prev_c) = (twist(&u0), theta0);
    let (mut t_m, mut t_c) = (None, None);
    let (mut worst_step, mut worst_pure): (Real, Real) = (0.0, 0.0);
    let mut time = 0.0;
    for _ in 0..2000 {
        m.solve_step(None).unwrap();
        lumped.step();
        consistent.step();
        time += dt;
        let um = m.displacement().unwrap().as_slice().to_vec();
        for (j, &g) in free.iter().enumerate() {
            worst_step = worst_step.max((um[g] - lumped.u[j]).abs() / theta0);
        }
        for n in 0..nodes.len() {
            let w = [um[n * NDOF + 3], um[n * NDOF + 4], um[n * NDOF + 5]];
            let along = dot(w, t);
            let normal = (0..3)
                .map(|d| (w[d] - along * t[d]).abs())
                .fold(0.0, Real::max);
            let tr = um[n * NDOF..n * NDOF + 3]
                .iter()
                .fold(0.0_f64, |s, v| s.max(v.abs()));
            worst_pure = worst_pure.max(normal.max(tr) / theta0);
        }
        let (cm, cc) = (twist(&um), tip_free(&consistent.u));
        if t_m.is_none() {
            t_m = crossing(prev_m, cm, time, dt);
        }
        if t_c.is_none() {
            t_c = crossing(prev_c, cc, time, dt);
        }
        if t_m.is_some() && t_c.is_some() {
            break;
        }
        (prev_m, prev_c) = (cm, cc);
    }
    let (t_m, t_c) = (
        t_m.expect("lumped crossing"),
        t_c.expect("consistent crossing"),
    );
    // closed forms of the lumped and consistent chains (θ = π/(2N))
    let c = (mat.gj / (mat.rho * (mat.iy + mat.iz))).sqrt();
    let h = len / ne as Real;
    let th = PI / (2. * ne as Real);
    let w_lumped = 2. * c / h * (th / 2.).sin();
    let w_consistent = c / h * (6. * (1. - th.cos()) / (2. + th.cos())).sqrt();
    // u_n = A cos(ω̃ (n − ½) Δt): first zero at Δt/2 + π/(2ω̃)
    let quarter = |w: Real| dt / 2. + PI / (2. * (2. / dt * (w * dt / 2.).asin()));
    let (q_l, q_c) = (quarter(w_lumped), quarter(w_consistent));
    let (e_l, e_c) = ((t_m - q_l) / q_l, (t_c - q_c) / q_c);
    let w_exact = c * PI / (2. * len);
    eprintln!(
        "STRUCT-008: {} steps, model vs dense lumped CD ≤ {worst_step:.1e} θ₀, pure twist ≤ \
         {worst_pure:.1e} θ₀; quarter period lumped {t_m:.9e} (closed form rel {e_l:+.1e}), \
         consistent {t_c:.9e} (rel {e_c:+.1e}); T_lumped/T_consistent = {:.6}; ω_h/ω = {:.6}, \
         ω_c/ω = {:.6}",
        (time / dt).round(),
        t_m / t_c,
        w_lumped / w_exact,
        w_consistent / w_exact
    );
    assert!(
        worst_step <= 1e-12,
        "model vs dense recurrence {worst_step:e}"
    );
    assert!(worst_pure <= 1e-12, "pure twist {worst_pure:e}");
    assert!(e_l.abs() <= 1e-6 && e_c.abs() <= 1e-6, "{e_l:e} {e_c:e}");
    // lumped −0.0714 %, consistent +0.0714 % of ω: the periods differ by both
    let want_ratio = w_consistent / w_lumped;
    let ratio_dev = ((t_m / t_c) / want_ratio - 1.).abs();
    eprintln!("STRUCT-008: period ratio vs closed form ω_c/ω_h: {ratio_dev:.3e}");
    assert!(ratio_dev <= 2e-6);
    assert!((w_lumped / w_exact - 1. + 7.14e-4).abs() < 5e-7);
}

// ---------------------------------------------------------------------------
// D-059 measurements
// ---------------------------------------------------------------------------

/// The torsional frequencies D-059 quotes (−1.14 % / −0.29 % / −0.07 % at
/// 3 / 6 / 12 elements, steel shaft `r = 0.02`, `L = 1.5`): on a shaft
/// along x with the torsion isolated, the lumped fundamental is the closed
/// form of the lumped chain `ω_h/ω = (4N/π) sin(π/(4N))` (−1.1384 %,
/// −0.2853 %, −0.0714 %) to the eigen-solver's rounding (measured ≤ 8.9e-15;
/// identity bound 1e-12); in the full spectrum of the same shaft along
/// (1,1,1) and (0.3,−0.5,0.8) the same frequency is present to the
/// objectivity level (measured ≤ 7.8e-15; bound 1e-10). The interim
/// `IsotropicMax` gives −82.5 % / −64.8 % / −29.4 % (recorded).
#[test]
fn torsional_frequency_errors_of_d059() {
    let (len, r) = (1.5, 0.02);
    let mat = shaft_material(r);
    let ip = mat.iy + mat.iz;
    let w_t = PI / (2. * len) * (mat.gj / (mat.rho * ip)).sqrt();
    let torsion_x = |ne: usize, rule| -> Real {
        let (mut m, _, _) = shaft([1., 0., 0.], ne, len, mat, rule);
        {
            let mut b_g = m.blocked_dofs_mut().unwrap();
            for (k, v) in b_g.as_mut_slice().iter_mut().enumerate() {
                *v = k < NDOF || k % NDOF != 3;
            }
        }
        lumped_spectrum(&mut m)[0]
    };
    for (ne, quoted) in [(3, -1.14e-2), (6, -0.29e-2), (12, -0.07e-2)] {
        let n = ne as Real;
        let closed = (4. * n / PI) * (PI / (4. * n)).sin();
        let w = torsion_x(ne, LumpedRotaryInertia::Block3x3);
        let err = w / w_t - 1.;
        let iso = torsion_x(ne, LumpedRotaryInertia::IsotropicMax) / w_t - 1.;
        let id = (w / w_t - closed).abs() / closed;
        eprintln!(
            "torsion, {ne} elements: Block3x3 {:+.4} % (closed form {:+.4} %, rel {id:.1e}), \
             IsotropicMax {:+.2} %",
            100. * err,
            100. * (closed - 1.),
            100. * iso
        );
        assert!(id <= 1e-12, "closed form of the lumped chain: {id:e}");
        // the quoted value, to half a unit of its last digit (0.01 %)
        assert!((err - quoted).abs() <= 0.5e-4, "{err} vs quoted {quoted}");
        assert!(iso < -0.2, "IsotropicMax is the interim rule");
        for dir in [[1., 1., 1.], [0.3, -0.5, 0.8]] {
            let (mut m, _, _) = shaft(dir, ne, len, mat, LumpedRotaryInertia::Block3x3);
            let wl = lumped_spectrum(&mut m);
            let near = wl
                .iter()
                .map(|x| (x - w).abs() / w)
                .fold(Real::INFINITY, Real::min);
            eprintln!("  axis {dir:?}: torsional frequency in the full spectrum to {near:.1e}");
            assert!(
                near <= 1e-10,
                "axis {dir:?}, {ne} elements: torsion {near:e}"
            );
        }
    }
}

/// Objectivity: a rigid rotation `Q` of a 3-D frame in general orientation
/// (nodes and normals) maps every nodal block to `Q J Qᵀ` (measured 3.4 ε
/// of the block's largest entry; identity bound 64 ε) and leaves the
/// lumped spectrum unchanged to the eigen-solver's level (measured 2.0e-12
/// relative; guard 1e-9, the bound of the isotropic rule's test).
#[test]
fn lumped_blocks_and_spectrum_are_objective() {
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
    let normals: Vec<V3> = conn
        .iter()
        .zip(helpers)
        .map(|(c, h)| normal_for(unit(sub(&nodes[c[1] as usize], &nodes[c[0] as usize])), h))
        .collect();
    let mat = shaft_material(0.05);
    let run = |nodes: &[Vec<Real>], normals: &[V3]| -> (Vec<Real>, Vec<Real>) {
        let mut m = model(
            mesh_with(nodes, &conn, normals),
            mat,
            LumpedRotaryInertia::Block3x3,
            "frame",
        );
        m.blocked_dofs_mut().unwrap().as_mut_slice()[..NDOF].fill(true);
        let w = lumped_spectrum(&mut m);
        (m.rotary_inertia().unwrap().as_slice().to_vec(), w)
    };
    let (j0, w0) = run(&nodes, &normals);
    let rn: Vec<Vec<Real>> = nodes.iter().map(|x| rotate(x).to_vec()).collect();
    let rnorm: Vec<V3> = normals.iter().map(|x| rotate(x)).collect();
    let (j1, w1) = run(&rn, &rnorm);
    let q = rotation_matrix();
    let mut worst_block: Real = 0.0;
    for n in 0..nodes.len() {
        let b = &j0[n * 9..(n + 1) * 9];
        let scale = b.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        for i in 0..3 {
            for j in 0..3 {
                let mut qjq = 0.0;
                for p in 0..3 {
                    for s in 0..3 {
                        qjq += q[i][p] * b[p * 3 + s] * q[j][s];
                    }
                }
                worst_block = worst_block.max((j1[n * 9 + i * 3 + j] - qjq).abs() / scale);
            }
        }
    }
    let dw = w0
        .iter()
        .zip(&w1)
        .map(|(a, b)| ((a - b) / a).abs())
        .fold(0.0, Real::max);
    eprintln!(
        "objectivity: blocks Q J Qᵀ to {worst_block:.1e} ({:.1} ε), spectrum {dw:.1e}",
        worst_block / EPS
    );
    assert!(worst_block <= 64. * EPS, "blocks {worst_block:e}");
    assert!(dw <= 1e-9, "spectrum {dw:e}");
}

/// `ω_max` (and the stable step `Δt = 2/ω_max`) of the steel shaft along x
/// for the two rules: the maximum over the four decoupled sub-systems
/// (axial, torsion, the two bending planes). On coarse meshes (`h > h* =
/// √(39 I_p/A) = 4.42 r`) `Block3x3` and `IsotropicMax` have the same
/// `ω_max` (the axial wave governs: identical sub-system, bitwise); on fine
/// meshes the interim rule's inflated bending inertia gives it the longer
/// step (measured 1.391× at 24 and 2.405× at 48 elements, 1.4× /
/// 2.4×; guard: more than 1.3×).
/// The lumped `ω_max` stays below the consistent one (stable step at least
/// the consistent one's).
#[test]
fn stable_time_step_of_the_two_rules() {
    let (len, r) = (1.5, 0.02);
    let mat = shaft_material(r);
    let h_star = (39. * (mat.iy + mat.iz) / mat.a).sqrt();
    // sub-systems: free components per node (along x: u | θx | v, θz | w, θy)
    let subsystems: [&[usize]; 4] = [&[0], &[3], &[1, 5], &[2, 4]];
    let w_max = |ne: usize, rule: LumpedRotaryInertia, consistent: bool| -> Real {
        let mut best: Real = 0.0;
        for comps in subsystems {
            let (mut m, _, _) = shaft([1., 0., 0.], ne, len, mat, rule);
            {
                let mut b_g = m.blocked_dofs_mut().unwrap();
                for (k, v) in b_g.as_mut_slice().iter_mut().enumerate() {
                    *v = k < NDOF || !comps.contains(&(k % NDOF));
                }
            }
            let w = if consistent {
                m.assemble_matrix("K").unwrap();
                m.assemble_mass_matrix().unwrap();
                let free = free_dofs(&m);
                let k = restrict(&dense(&m, "K"), &free);
                let mc = restrict(&dense(&m, "M"), &free);
                generalized_eigenvalues(&k, &mc).last().unwrap().sqrt()
            } else {
                *lumped_spectrum(&mut m).last().unwrap()
            };
            best = best.max(w);
        }
        best
    };
    for ne in [3, 6, 12, 24, 48] {
        let h = len / ne as Real;
        let b = w_max(ne, LumpedRotaryInertia::Block3x3, false);
        let i = w_max(ne, LumpedRotaryInertia::IsotropicMax, false);
        let c = w_max(ne, LumpedRotaryInertia::Block3x3, true);
        eprintln!(
            "stable step, {ne} elements (h/h* = {:.3}): Δt = 2/ω_max: Block3x3 {:.4e}, \
             IsotropicMax {:.4e} ({:.3}×), consistent {:.4e}",
            h / h_star,
            2. / b,
            2. / i,
            b / i,
            2. / c
        );
        assert!(b < c, "lumped ω_max below the consistent one");
        if h > h_star {
            assert_eq!(b.to_bits(), i.to_bits(), "coarse mesh: same ω_max");
        } else {
            assert!(
                b > 1.3 * i,
                "fine mesh: the interim rule has the longer step"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// block solve with blocked rotations
// ---------------------------------------------------------------------------

/// The explicit lumped solve with one, two and three blocked rotations at
/// the branching node of a 3-D frame, nonzero prescribed accelerations on
/// the blocked rotations and loads on every free DOF: after one step from
/// rest the free accelerations are `M_FF⁻¹ (f_F − M_FB a_B)` with the dense
/// closed-form block mass (blocked rows and columns removed, the remaining
/// block solved), the blocked ones unchanged. Dense-solve identity to
/// rounding (measured ≤ 2.4 ε; bound 64 ε of the largest acceleration).
#[test]
fn block_solve_honours_one_two_and_three_blocked_rotations() {
    let (mut nodes, mut conn, mut normals) = rotated_two_element_beam();
    nodes.push(vec![0.4, 1.7, 0.9]);
    conn.push(vec![1, 3]);
    normals.push(normal_for(
        unit(sub(&nodes[3], &nodes[1])),
        [0.3, -0.2, 1.0],
    ));
    let mat = general_material();
    let (trans, blocks) = closed_form(&nodes, &conn, &normals, &mat);
    let full = dense_of(&trans, &blocks);
    let ndofs = nodes.len() * NDOF;
    let masks: [&[usize]; 7] = [&[3], &[4], &[5], &[3, 5], &[4, 5], &[3, 4], &[3, 4, 5]];
    for mask in masks {
        let mut m = model(
            mesh_with(&nodes, &conn, &normals),
            mat,
            LumpedRotaryInertia::Block3x3,
            "blocked",
        );
        m.set_time_step(1e-6, None).unwrap();
        let mut blocked = vec![false; ndofs];
        blocked[..NDOF].fill(true);
        for &c in mask {
            blocked[NDOF + c] = true;
            // a blocked rotation of node 2 as well (an end node's block)
            blocked[2 * NDOF + c] = true;
        }
        m.blocked_dofs_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&blocked);
        let f: Vec<Real> = (0..ndofs)
            .map(|i| {
                if blocked[i] {
                    0.0
                } else {
                    ((i * 5 + 1) % 7) as Real - 2.5
                }
            })
            .collect();
        let a_b: Vec<Real> = (0..ndofs)
            .map(|i| {
                if blocked[i] && i >= NDOF {
                    0.3 + 0.1 * (i % 4) as Real
                } else {
                    0.0
                }
            })
            .collect();
        m.external_force_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&f);
        m.acceleration_mut()
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&a_b);
        m.solve_step(None).unwrap();
        let acc = m.acceleration().unwrap().as_slice().to_vec();
        let free: Vec<usize> = (0..ndofs).filter(|&i| !blocked[i]).collect();
        let mff = restrict(&full, &free);
        let rhs: Vec<Real> = free
            .iter()
            .map(|&i| f[i] - (0..ndofs).map(|j| full[i][j] * a_b[j]).sum::<Real>())
            .collect();
        let l = cholesky(&mff);
        let n = free.len();
        let mut y = vec![0.0; n];
        for i in 0..n {
            let mut s = rhs[i];
            for j in 0..i {
                s -= l[i][j] * y[j];
            }
            y[i] = s / l[i][i];
        }
        let mut x = vec![0.0; n];
        for i in (0..n).rev() {
            let mut s = y[i];
            for j in i + 1..n {
                s -= l[j][i] * x[j];
            }
            x[i] = s / l[i][i];
        }
        let scale = x.iter().fold(0.0_f64, |s, v| s.max(v.abs()));
        let worst = free
            .iter()
            .zip(&x)
            .map(|(&g, &want)| (acc[g] - want).abs() / scale)
            .fold(0.0, Real::max);
        eprintln!(
            "blocked rotations {mask:?} at nodes 1 and 2: free accelerations to {:.1} ε",
            worst / EPS
        );
        assert!(worst <= 64. * EPS, "mask {mask:?}: {worst:e}");
        for i in (0..ndofs).filter(|&i| blocked[i]) {
            assert_eq!(
                acc[i].to_bits(),
                a_b[i].to_bits(),
                "blocked DOF {i} keeps its acceleration"
            );
        }
    }
}

/// The free rotary sub-block must be positive definite: collinear beams
/// without polar moment (`J_t = 0`) have no inertia about their axis, so an
/// inclined straight beam is refused with `Block3x3` (its diagonal entries
/// are all positive), accepted with `IsotropicMax`; along x with `θx`
/// blocked the free sub-block `(θy, θz)` is regular and accepted, and
/// unblocking `θx` afterwards is refused on the reuse path.
#[test]
fn singular_free_rotary_blocks_are_refused() {
    let mat = StructuralMaterial {
        e: 1.,
        rho: 3.,
        a: 0.2,
        gj: 1.,
        ..StructuralMaterial::default()
    };
    let (mut m, _, _) = shaft([0.2, 0.9, -0.4], 3, 1.2, mat, LumpedRotaryInertia::Block3x3);
    match m.assemble_lumped_mass_matrix() {
        Err(Error::Numerical(msg)) => assert!(msg.contains("positive definite"), "{msg}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    let lm_diag_positive = {
        // the refused block has positive diagonal entries (the plain
        // per-DOF check would accept it)
        let (_, nodes, t) = shaft([0.2, 0.9, -0.4], 3, 1.2, mat, LumpedRotaryInertia::Block3x3);
        let normals = vec![normal_for(t, [0., 0., 1.]); 3];
        let conn: Vec<Vec<Idx>> = (0..3).map(|i| vec![i, i + 1]).collect();
        let (_, blocks) = closed_form(&nodes, &conn, &normals, &mat);
        blocks
            .iter()
            .all(|b| b[0] > 0.0 && b[4] > 0.0 && b[8] > 0.0)
    };
    assert!(lm_diag_positive);
    let (mut iso, _, _) = shaft(
        [0.2, 0.9, -0.4],
        3,
        1.2,
        mat,
        LumpedRotaryInertia::IsotropicMax,
    );
    iso.assemble_lumped_mass_matrix().unwrap();
    let (mut mx, _, _) = shaft([1., 0., 0.], 3, 1.2, mat, LumpedRotaryInertia::Block3x3);
    {
        let mut b_g = mx.blocked_dofs_mut().unwrap();
        for (k, v) in b_g.as_mut_slice().iter_mut().enumerate() {
            if k % NDOF == 3 {
                *v = true;
            }
        }
    }
    mx.assemble_lumped_mass_matrix().unwrap();
    mx.blocked_dofs_mut().unwrap().as_mut_slice()[3 * NDOF + 3] = false;
    assert!(matches!(
        mx.assemble_lumped_mass_matrix(),
        Err(Error::Numerical(_))
    ));
    assert!(mx.solve_step(None).is_err());
}

/// The accept side of `ROTARY_BLOCK_DEFINITENESS` (1e-12):
/// a single element along x has the nodal block `diag(J_t, J_b,
/// J_b)` with `J_t/J_b = 39 (Iy + Iz)/(A L²)`, so its first `LDLᵀ` pivot
/// relative to the largest diagonal entry is exactly that ratio. A valid
/// element of ratio 1e-10 is accepted (a threshold raised to 1e-6 would
/// refuse it), one of ratio 1e-13 is refused.
#[test]
fn rotary_block_definiteness_threshold_accepts_1e10_and_refuses_1e13() {
    let (len, a) = (1.2, 0.2);
    let with_ratio = |ratio: Real| StructuralMaterial {
        e: 1.,
        rho: 3.,
        a,
        iy: ratio * a * len * len / 39.,
        gj: 1.,
        ..StructuralMaterial::default()
    };
    // accepted: 1e-10, and the model holds the block it was asked for
    let mat = with_ratio(1e-10);
    let (mut m, _, _) = shaft([1., 0., 0.], 1, len, mat, LumpedRotaryInertia::Block3x3);
    m.assemble_lumped_mass_matrix().unwrap();
    let j = m.rotary_inertia().unwrap().tuple(0).to_vec();
    let ratio = j[0] / j[4];
    eprintln!("definiteness threshold: accepted block, J_t/J_b = {ratio:e}");
    assert!((ratio / 1e-10 - 1.).abs() <= 64. * EPS, "{ratio:e}");
    // refused: 1e-13, with the conditioning message
    let mat = with_ratio(1e-13);
    let (mut m, _, _) = shaft([1., 0., 0.], 1, len, mat, LumpedRotaryInertia::Block3x3);
    match m.assemble_lumped_mass_matrix() {
        Err(Error::Numerical(msg)) => assert!(msg.contains("positive definite"), "{msg}"),
        other => panic!("expected a refusal, got {other:?}"),
    }
    // the same element with IsotropicMax is accepted (J_b on the torsion)
    let (mut iso, _, _) = shaft([1., 0., 0.], 1, len, mat, LumpedRotaryInertia::IsotropicMax);
    iso.assemble_lumped_mass_matrix().unwrap();
}

// ---------------------------------------------------------------------------
// 2-D
// ---------------------------------------------------------------------------

/// 2-D: one rotation, nothing to couple: `Block3x3` and `IsotropicMax` give
/// `J_b` (bitwise the same mass), a one-component `rotary_inertia` and no
/// block couplings.
#[test]
fn two_d_rules_have_one_rotation_and_no_couplings() {
    let nodes = vec![vec![0., 0.], vec![3., 4.], vec![3., 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let mat = StructuralMaterial {
        e: 1.,
        rho: 5.,
        a: 3.,
        i: 1.,
        ..StructuralMaterial::default()
    };
    let run = |rule| {
        let mesh = mesh_of(2, ElementType::BernoulliBeam2, &nodes, &conn, None);
        let mut m =
            StructuralMechanicsModel::with_compat(shared(mesh), None, "two_d", compat(rule))
                .unwrap();
        m.add_material(mat, "").unwrap();
        m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
            .unwrap();
        m.assemble_lumped_mass_matrix().unwrap();
        assert!(m.dof_manager().lumped_block_couplings("M").is_none());
        let j = m.rotary_inertia().unwrap();
        assert_eq!(j.nb_component(), 1);
        let lm = m.lumped_mass().unwrap().as_slice().to_vec();
        for n in 0..3 {
            assert_eq!(j.tuple(n)[0].to_bits(), lm[n * 3 + 2].to_bits());
        }
        lm
    };
    let b = run(LumpedRotaryInertia::Block3x3);
    let i = run(LumpedRotaryInertia::IsotropicMax);
    assert_eq!(
        b.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        i.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    let m1 = 5. * 3. * 5.;
    assert!((b[2] - m1 * 25. / 78.).abs() <= 4. * EPS * b[2]);
}
