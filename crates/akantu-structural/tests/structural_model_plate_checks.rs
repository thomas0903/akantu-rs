// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): independent checks of the
// structural model's discrete Kirchhoff triangles against
// Kirchhoff plate theory. No upstream counterpart.

//! * Simply supported square plate (`w = 0` on the edges, rotations free)
//!   with a central point load `P`: the centre deflection converges to the
//!   Navier series `w = 4 P a²/(π⁴ D) Σ_{m,n odd} 1/(m² + n²)²`
//!   (`≈ 0.01160 P a²/D`), `D = E t³/(12 (1 − ν²))`, at `O(h² |log h|)`
//!   (point-load singularity).
//! * Thickness scaling (UD-007): with the corrected bending rigidity the
//!   deflection scales like `t⁻³` exactly; upstream's `E t⁴/…` gives `t⁻⁴`.

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Idx, Real};
use akantu_fe::structural::kernels::DktBendingStiffness;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

const TYPE: ElementType = ElementType::DiscreteKirchhoffTriangle18;

/// `n × n` cells on `[0, a]²` (z = 0), each split along a diagonal whose
/// direction alternates like a checkerboard (symmetric about the centre for
/// even `n`). Returns the mesh and the index of the centre node.
fn square_plate(n: usize, a: Real) -> (akantu_mesh::Mesh, usize) {
    let mut nodes = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            nodes.push(vec![
                a * i as Real / n as Real,
                a * j as Real / n as Real,
                0.,
            ]);
        }
    }
    let id = |i: usize, j: usize| (j * (n + 1) + i) as Idx;
    let mut conn: Vec<Vec<Idx>> = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (p00, p10, p01, p11) = (id(i, j), id(i + 1, j), id(i, j + 1), id(i + 1, j + 1));
            if (i + j) % 2 == 0 {
                conn.push(vec![p00, p10, p11]);
                conn.push(vec![p00, p11, p01]);
            } else {
                conn.push(vec![p00, p10, p01]);
                conn.push(vec![p10, p11, p01]);
            }
        }
    }
    let center = (n / 2) * (n + 1) + n / 2;
    (mesh_of(3, TYPE, &nodes, &conn, None), center)
}

/// Centre deflection of the simply supported plate under `p` (downwards).
fn ss_plate_center_deflection(
    n: usize,
    mat: StructuralMaterial,
    p: Real,
    compat: StructuralModelUpstreamCompat,
) -> Real {
    let a = 1.0;
    let (mesh, center) = square_plate(n, a);
    let nb_nodes = (n + 1) * (n + 1);
    let mut m = StructuralMechanicsModel::with_compat(shared(mesh), None, "plate", compat).unwrap();
    m.add_material(mat, "").unwrap();
    m.init_full_with(AnalysisMethod::Static).unwrap();
    {
        let mut b_g = m.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        for k in 0..nb_nodes {
            let (i, j) = (k % (n + 1), k / (n + 1));
            // membrane and drilling DOFs (no stiffness for θz) blocked
            b[k * 6] = true;
            b[k * 6 + 1] = true;
            b[k * 6 + 5] = true;
            if i == 0 || j == 0 || i == n || j == n {
                b[k * 6 + 2] = true;
            }
        }
    }
    m.external_force_mut().unwrap().as_mut_slice()[center * 6 + 2] = -p;
    m.solve_step(None).unwrap();
    -m.displacement().unwrap().as_slice()[center * 6 + 2]
}

/// Navier coefficient `α` of `w = α P a²/D` (centre load, simply
/// supported square plate).
fn navier_alpha() -> Real {
    use std::f64::consts::PI;
    let mut s = 0.0;
    let nmax = 3001;
    for m in (1..=nmax).step_by(2) {
        for n in (1..=nmax).step_by(2) {
            let r = (m * m + n * n) as Real;
            s += 1.0 / (r * r);
        }
    }
    4. / PI.powi(4) * s
}

fn plate_material(t: Real) -> StructuralMaterial {
    StructuralMaterial {
        e: 1.0,
        nu: 0.3,
        t,
        ..StructuralMaterial::default()
    }
}

#[test]
fn simply_supported_plate_converges_to_navier() {
    let alpha = navier_alpha();
    // Timoshenko & Woinowsky-Krieger's table value
    assert!((alpha - 0.0116).abs() < 1e-4, "alpha {alpha}");
    let t = 0.1;
    let mat = plate_material(t);
    let d = mat.e * t * t * t / (12. * (1. - mat.nu * mat.nu));
    let p = 1e-4;
    let exact = alpha * p / d;
    let mut errs = Vec::new();
    for n in [4, 8, 16] {
        let w = ss_plate_center_deflection(n, mat, p, StructuralModelUpstreamCompat::default());
        errs.push(((w - exact) / exact).abs());
    }
    eprintln!("simply supported plate, n = 4, 8, 16: rel errors {errs:?}");
    // the point load makes w ~ r² log r near the load: the centre
    // deflection converges like h² |log h|, i.e. an error ratio of
    // 4 log(h)/log(h/2) ≈ 2.67 (h = 1/4 → 1/8) and 3.0 (1/8 → 1/16) per
    // halving
    assert!(
        errs[0] / errs[1] > 2.5 && errs[1] / errs[2] > 2.5,
        "{errs:?}"
    );
    assert!(errs[2] < 5e-3, "{errs:?}");
}

/// Deflection ratios between `t = 1` and `t = 0.1` (bending only): `t⁻³`
/// with the corrected rigidity (default), `t⁻⁴` with upstream's (UD-007).
#[test]
fn plate_deflection_scales_with_the_cube_of_the_thickness() {
    let p = 1e-4;
    for (bending, power) in [
        (DktBendingStiffness::Corrected, 3),
        (DktBendingStiffness::Legacy, 4),
    ] {
        let compat = StructuralModelUpstreamCompat {
            dkt_bending: bending,
            ..StructuralModelUpstreamCompat::default()
        };
        let w1 = ss_plate_center_deflection(6, plate_material(1.0), p, compat);
        let w01 = ss_plate_center_deflection(6, plate_material(0.1), p, compat);
        let ratio = w01 / w1;
        let want = 10f64.powi(power);
        eprintln!(
            "{bending:?}: w(0.1)/w(1) = {ratio} (rel error {:e})",
            (ratio - want) / want
        );
        assert!(
            ((ratio - want) / want).abs() < 1e-11,
            "{bending:?}: ratio {ratio} vs {want}"
        );
    }
}
