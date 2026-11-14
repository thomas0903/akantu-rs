// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS). Adversarial
// checks (D-059; structural_mechanics_model_mass.cc is
// the upstream counterpart, whose lumped mass is identically zero),
// written independently of the port; the test of the withdrawn `Upstream`
// rule is dropped, its unit-consistency half kept for the two rules.

#![allow(clippy::needless_range_loop)]

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Idx, Real};
use akantu_fe::structural::StructuralUpstreamCompat;
use akantu_mesh::Mesh;
use akantu_structural::{
    LumpedRotaryInertia, StructuralMaterial, StructuralMechanicsModel,
    StructuralModelUpstreamCompat,
};
use structural_model_common::*;

use std::f64::consts::PI;

const NDOF: usize = 6;
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

fn steel(r: Real) -> StructuralMaterial {
    let i = PI * r.powi(4) / 4.;
    StructuralMaterial {
        e: 2.1e11,
        a: PI * r * r,
        iz: i,
        iy: i,
        gj: 8.1e10 * 2. * i,
        rho: 7800.,
        ..StructuralMaterial::default()
    }
}

fn compat(rule: LumpedRotaryInertia) -> StructuralModelUpstreamCompat {
    StructuralModelUpstreamCompat {
        lumped_rotary_inertia: rule,
        ..StructuralModelUpstreamCompat::default()
    }
}

/// A helix of `ne` elements (every member in its own direction, element
/// length `h`), clamped at node 0.
/// Nodes, connectivity and normals of a mesh.
type MeshData = (Vec<Vec<Real>>, Vec<Vec<Idx>>, Vec<Vec<Real>>);

fn helix(ne: usize, h: Real) -> MeshData {
    let (radius, dtheta): (Real, Real) = (0.15, 0.25);
    // pitch so that every chord has length h
    let chord_xy = 2. * radius * (dtheta / 2.).sin();
    let pitch = (h * h - chord_xy * chord_xy).sqrt();
    let nodes: Vec<Vec<Real>> = (0..=ne)
        .map(|i| {
            let t = i as Real * dtheta;
            vec![radius * t.cos(), radius * t.sin(), pitch * i as Real]
        })
        .collect();
    let conn: Vec<Vec<Idx>> = (0..ne as Idx).map(|i| vec![i, i + 1]).collect();
    let normals: Vec<Vec<Real>> = conn
        .iter()
        .map(|c| {
            let a = &nodes[c[0] as usize];
            let b = &nodes[c[1] as usize];
            let e1 = unit([b[0] - a[0], b[1] - a[1], b[2] - a[2]]);
            unit(cross(e1, [0.3, -0.8, 0.5])).to_vec()
        })
        .collect();
    (nodes, conn, normals)
}

fn model_of(
    mesh: Mesh,
    mat: StructuralMaterial,
    c: StructuralModelUpstreamCompat,
) -> StructuralMechanicsModel {
    let mut m = StructuralMechanicsModel::with_compat(shared(mesh), None, "v127", c).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    m.blocked_dofs_mut().unwrap().as_mut_slice()[..NDOF].fill(true);
    m
}

/// Dense lumped matrix from the model's arrays (translations of
/// `lumped_mass`, full rotary blocks of `rotary_inertia`).
fn dense_lumped(m: &StructuralMechanicsModel) -> Vec<Vec<Real>> {
    let lm = m.lumped_mass().unwrap();
    let j = m.rotary_inertia().unwrap();
    let nn = lm.len();
    let n = nn * NDOF;
    let mut d = vec![vec![0.0; n]; n];
    for k in 0..nn {
        for c in 0..3 {
            d[k * NDOF + c][k * NDOF + c] = lm.tuple(k)[c];
        }
        let b = j.tuple(k);
        for p in 0..3 {
            for q in 0..3 {
                d[k * NDOF + 3 + p][k * NDOF + 3 + q] = b[p * 3 + q];
            }
        }
    }
    d
}

fn free_of(m: &StructuralMechanicsModel) -> Vec<usize> {
    m.blocked_dofs()
        .unwrap()
        .as_slice()
        .iter()
        .enumerate()
        .filter(|(_, b)| !**b)
        .map(|(i, _)| i)
        .collect()
}

fn omega_max(m: &mut StructuralMechanicsModel) -> Real {
    m.assemble_matrix("K").unwrap();
    m.assemble_lumped_mass_matrix().unwrap();
    let free = free_of(m);
    let k = restrict(&dense(m, "K"), &free);
    let ml = restrict(&dense_lumped(m), &free);
    generalized_eigenvalues(&k, &ml).last().unwrap().sqrt()
}

/// Run `steps` explicit lumped steps from a pseudo-random initial
/// displacement of amplitude 1e-9; returns max |u| / 1e-9.
fn run(m: &mut StructuralMechanicsModel, dt: Real, steps: usize) -> Real {
    m.set_time_step(dt, None).unwrap();
    let free = free_of(m);
    let mut s: u64 = 42;
    {
        let mut u = m.displacement_mut().unwrap();
        let u = u.as_mut_slice();
        for &i in &free {
            s = s
                .wrapping_mul(6364136223846793005)
                .wrapping_add(1442695040888963407);
            u[i] = 1e-9 * (((s >> 11) as Real / (1u64 << 53) as Real) * 2. - 1.);
        }
    }
    let mut worst: Real = 0.0;
    for _ in 0..steps {
        m.solve_step(None).unwrap();
        let u = m.displacement().unwrap();
        let mx = u.as_slice().iter().fold(0.0, |a: Real, v| a.max(v.abs()));
        worst = worst.max(mx / 1e-9);
        if !worst.is_finite() || worst > 1e30 {
            break;
        }
    }
    worst
}

/// Item 2 of the brief: a run at the stable step computed from the block
/// mass (2/ω_max of the dense K, M_lumped with the model's blocks) stays
/// stable; slightly above it diverges; the IsotropicMax step (longer on
/// fine meshes) diverges with Block3x3. Fine helix (h < h* = 4.42 r).
#[test]
fn v127_stable_step_of_the_blocks_is_the_explicit_limit() {
    let r = 0.02;
    let h = 0.05; // h/h* = 0.57
    let (nodes, conn, normals) = helix(16, h);
    let mesh = || {
        mesh_of(
            3,
            ElementType::BernoulliBeam3,
            &nodes,
            &conn,
            Some(&normals),
        )
    };
    let mut b = model_of(mesh(), steel(r), compat(LumpedRotaryInertia::Block3x3));
    let mut iso = model_of(mesh(), steel(r), compat(LumpedRotaryInertia::IsotropicMax));
    let wb = omega_max(&mut b);
    let wi = omega_max(&mut iso);
    let (dtb, dti) = (2. / wb, 2. / wi);
    eprintln!(
        "rotary block: helix 16 × h = {h}: Δt_crit Block3x3 {dtb:.6e}, IsotropicMax {dti:.6e} ({:.3}×)",
        dti / dtb
    );
    let below = run(&mut b, 0.995 * dtb, 4000);
    let mut b2 = model_of(mesh(), steel(r), compat(LumpedRotaryInertia::Block3x3));
    let above = run(&mut b2, 1.005 * dtb, 4000);
    let mut b3 = model_of(mesh(), steel(r), compat(LumpedRotaryInertia::Block3x3));
    let at_iso = run(&mut b3, 0.995 * dti, 4000);
    let mut i2 = model_of(mesh(), steel(r), compat(LumpedRotaryInertia::IsotropicMax));
    let iso_ok = run(&mut i2, 0.995 * dti, 4000);
    eprintln!(
        "rotary block: max|u|/u0: Block3x3 at 0.995 Δt_crit {below:.3e}, at 1.005 Δt_crit {above:.3e}, \
         at 0.995 Δt_iso {at_iso:.3e}; IsotropicMax at 0.995 Δt_iso {iso_ok:.3e}"
    );
    assert!(below < 1e3, "stable below the limit");
    assert!(above > 1e10, "unstable above the limit");
    assert!(
        at_iso > 1e10,
        "IsotropicMax's step is unstable with the blocks"
    );
    assert!(iso_ok < 1e3);
}

/// Reassembly after a material change replaces the couplings (no
/// accumulation of the old ones) and follows the new density.
#[test]
fn v127_reassembly_replaces_the_couplings() {
    let (nodes, conn, normals) = helix(4, 0.3);
    let mesh = mesh_of(
        3,
        ElementType::BernoulliBeam3,
        &nodes,
        &conn,
        Some(&normals),
    );
    let mut m = model_of(mesh, steel(0.02), compat(LumpedRotaryInertia::Block3x3));
    m.assemble_lumped_mass_matrix().unwrap();
    let off0 = m
        .dof_manager()
        .lumped_block_couplings("M")
        .unwrap()
        .off_diagonal()
        .to_vec();
    m.material_mut(0).unwrap().rho *= 3.0;
    m.assemble_lumped_mass_matrix().unwrap();
    let off1 = m
        .dof_manager()
        .lumped_block_couplings("M")
        .unwrap()
        .off_diagonal()
        .to_vec();
    let j = m.rotary_inertia().unwrap();
    let mut worst: Real = 0.0;
    for n in 0..j.len() {
        let b = j.tuple(n);
        for (k, idx) in [1usize, 2, 5].iter().enumerate() {
            assert_eq!(off1[n * 3 + k], b[*idx]);
            if off0[n * 3 + k] != 0.0 {
                worst = worst.max((off1[n * 3 + k] / off0[n * 3 + k] - 3.0).abs());
            }
        }
    }
    eprintln!("rotary block: reassembly after ρ×3: couplings ratio 3 to {worst:.1e}");
    assert!(worst <= 1e-14);
}

/// Under the UD-343 compat (`beam3_normal_unchecked`) with a skew normal
/// the FE frame is not orthogonal; record what Block3x3 then assembles
/// against the axis-only closed form `J_t aaᵀ + J_b (I − aaᵀ)`.
#[test]
fn v127_block_with_a_skew_normal_under_ud343_compat() {
    let nodes = vec![vec![0., 0., 0.], vec![0.8, 0.3, -0.2]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1]];
    let a = unit([0.8, 0.3, -0.2]);
    let skew = vec![vec![0.2, 0.9, 0.5]]; // not orthogonal to a, not unit
    let mat = StructuralMaterial {
        e: 3.,
        rho: 2.5,
        a: 0.04,
        iz: 2e-4,
        iy: 6e-4,
        gj: 1.5,
        ..StructuralMaterial::default()
    };
    let c = StructuralModelUpstreamCompat {
        fe: StructuralUpstreamCompat {
            beam3_normal_unchecked: true,
            ..StructuralUpstreamCompat::default()
        },
        lumped_rotary_inertia: LumpedRotaryInertia::Block3x3,
        ..StructuralModelUpstreamCompat::default()
    };
    let mesh = mesh_of(3, ElementType::BernoulliBeam3, &nodes, &conn, Some(&skew));
    let mut m = StructuralMechanicsModel::with_compat(shared(mesh), None, "skew", c).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
        .unwrap();
    let r = m.assemble_lumped_mass_matrix();
    let l = (0.8f64 * 0.8 + 0.09 + 0.04).sqrt();
    let jt = mat.rho * (mat.iy + mat.iz) * l / 2.;
    let jb = mat.rho * mat.a * l * l * l / 78.;
    match r {
        Ok(()) => {
            let b = m.rotary_inertia().unwrap().tuple(0).to_vec();
            let mut worst: Real = 0.0;
            for i in 0..3 {
                for j in 0..3 {
                    let id = if i == j { 1. } else { 0. };
                    let want = jt * a[i] * a[j] + jb * (id - a[i] * a[j]);
                    worst = worst.max((b[i * 3 + j] - want).abs() / jb);
                }
            }
            eprintln!(
                "rotary block: skew normal (UD-343 compat): Block3x3 vs axis-only closed form {worst:.3e} of J_b"
            );
        }
        Err(e) => eprintln!("rotary block: skew normal (UD-343 compat): refused: {e}"),
    }
}

/// A renumbering of the model's DOF manager (a family registered after the
/// lumped assembly) drops the couplings while keeping the lumped diagonal;
/// the next lumped assembly must restore them (reuse-path condition).
#[test]
fn v127_renumbering_restores_the_couplings() {
    let (nodes, conn, normals) = helix(4, 0.3);
    let mesh = mesh_of(
        3,
        ElementType::BernoulliBeam3,
        &nodes,
        &conn,
        Some(&normals),
    );
    let mut m = model_of(mesh, steel(0.02), compat(LumpedRotaryInertia::Block3x3));
    m.assemble_lumped_mass_matrix().unwrap();
    let off0 = m
        .dof_manager()
        .lumped_block_couplings("M")
        .unwrap()
        .off_diagonal()
        .to_vec();
    m.dof_manager_mut()
        .register_generic_dofs("v127_extra", 1, 1)
        .unwrap();
    assert!(m.dof_manager().lumped_block_couplings("M").is_none());
    m.assemble_lumped_mass_matrix().unwrap();
    let off1 = m
        .dof_manager()
        .lumped_block_couplings("M")
        .map(|c| c.off_diagonal().to_vec());
    eprintln!(
        "rotary block: renumbering: couplings restored = {}",
        off1.as_deref() == Some(&off0[..])
    );
    assert_eq!(off1.as_deref(), Some(&off0[..]));
}

/// From `v127_upstream_2d_rule_depends_on_the_unit_system` without its half on
/// the withdrawn `Upstream` rule. The lumped rotary inertia is
/// unit-consistent: the same clamped–free 2-D cantilever in (N, m, kg, s)
/// and in (N, mm, t, s) has the same angular frequencies ω_max and ω₁ to
/// 1e-9 relative (consistent unit systems give the same
/// eigenvalues up to rounding), for both rules.
#[test]
fn lumped_rotary_inertia_is_unit_consistent() {
    // (ω_max, ω_min) of the clamped–free cantilever (2-D: the lumped
    // matrix is diagonal)
    let spectrum =
        |rule: LumpedRotaryInertia, len: Real, mat: StructuralMaterial| -> (Real, Real) {
            let mesh = line_mesh(
                2,
                ElementType::BernoulliBeam2,
                8,
                len,
                &[0., 0.],
                &[1., 0.],
                None,
            );
            let mut m =
                StructuralMechanicsModel::with_compat(shared(mesh), None, "units", compat(rule))
                    .unwrap();
            m.add_material(mat, "").unwrap();
            m.init_full_with(AnalysisMethod::ExplicitLumpedMass)
                .unwrap();
            m.blocked_dofs_mut().unwrap().as_mut_slice()[..3].fill(true);
            m.assemble_matrix("K").unwrap();
            m.assemble_lumped_mass_matrix().unwrap();
            let free = free_of(&m);
            let k = restrict(&dense(&m, "K"), &free);
            let lm = m.lumped_mass().unwrap().as_slice().to_vec();
            let ml: Vec<Vec<Real>> = free
                .iter()
                .map(|&i| {
                    free.iter()
                        .map(|&j| if i == j { lm[i] } else { 0.0 })
                        .collect()
                })
                .collect();
            let w = generalized_eigenvalues(&k, &ml);
            (w.last().unwrap().sqrt(), w[0].sqrt())
        };
    let si = StructuralMaterial {
        e: 2.1e11,
        a: 1e-3,
        i: 1e-7,
        rho: 7800.,
        ..StructuralMaterial::default()
    };
    let mm = StructuralMaterial {
        e: 2.1e5,
        a: 1e3,
        i: 1e5,
        rho: 7.8e-9,
        ..StructuralMaterial::default()
    };
    for rule in [
        LumpedRotaryInertia::Block3x3,
        LumpedRotaryInertia::IsotropicMax,
    ] {
        let (a_max, a_min) = spectrum(rule, 2.0, si);
        let (b_max, b_min) = spectrum(rule, 2000.0, mm);
        assert!(
            (b_max / a_max - 1.).abs() < 1e-9 && (b_min / a_min - 1.).abs() < 1e-9,
            "{rule:?}: ω_max SI {a_max:e} vs mm {b_max:e}, ω₁ SI {a_min:e} vs mm {b_min:e}"
        );
    }
}
