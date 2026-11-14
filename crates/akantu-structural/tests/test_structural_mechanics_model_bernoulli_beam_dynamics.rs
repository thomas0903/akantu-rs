// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): port of
// test/test_model/test_structural_mechanics_model/
// test_structural_mechanics_model_bernoulli_beam_dynamics.cc (typed over
// _bernoulli_beam_2 / _bernoulli_beam_3).

//! `TestStructBernoulliDynamic` (typed over the two beams): a simply
//! supported beam of length `L = 2` in 40 elements along x (3-D: normals
//! z), `E = 1e9`, `ρ = 10`, `I = Iz = Iy = A = GJ = 1`, `_implicit_dynamic`,
//! a constant force `F = 1e4` (y) on the middle node.
//!
//! * `TestBeamMatrices`: the assembled `K` and `M` equal the hand-built
//!   element matrices assembled densely, within `1e-13 · max`. The 3-D
//!   reference mass weights the torsion row with `ρ·A` (upstream), so that
//!   case runs with `beam3_torsional_mass_rho_a` (UD-507); a separate test
//!   checks the corrected default (`ρ·(Iy + Iz)` on that row only).
//! * `TestBeamOscilation`: `Δt = 1e-6`, Newton–Raphson (`max_iterations =
//!   100`, `threshold = 1e-8`, `convergence_type = solution`); for 299 steps
//!   the middle-node deflection matches the three-term modal series
//!   `2 F L³/(π⁴ E I) Σ_{n=1,3,5} (1 − cos(n² ω t))/n⁴`,
//!   `ω = π²/L² √(E I/ρ)`, within `1e-6`. (Upstream also writes a CSV file;
//!   the port does not.)

mod structural_model_common;

use akantu_core::enums::AnalysisMethod;
use akantu_core::{ElementType, Real};
use akantu_parser::param::ParamValue;
use akantu_structural::{
    StructuralMaterial, StructuralMechanicsModel, StructuralModelUpstreamCompat,
};
use structural_model_common::*;

const NB_ELEMENT: usize = 40;
const L: Real = 2.0;
const F: Real = 1e4;

fn le() -> Real {
    L / NB_ELEMENT as Real
}

fn material() -> StructuralMaterial {
    StructuralMaterial {
        e: 1e9,
        rho: 10.,
        i: 1.,
        iz: 1.,
        iy: 1.,
        a: 1.,
        gj: 1.,
        ..StructuralMaterial::default()
    }
}

fn ndof(ty: ElementType) -> usize {
    if ty == ElementType::BernoulliBeam2 {
        3
    } else {
        6
    }
}

/// The upstream fixture for `ty`.
fn fixture(ty: ElementType, compat: StructuralModelUpstreamCompat) -> StructuralMechanicsModel {
    let dim = if ty == ElementType::BernoulliBeam2 {
        2
    } else {
        3
    };
    let origin = vec![0.0; dim];
    let mut dir = vec![0.0; dim];
    dir[0] = 1.0;
    let normal = (dim == 3).then_some([0., 0., 1.]);
    let mesh = line_mesh(
        dim,
        ty,
        NB_ELEMENT,
        L,
        &origin,
        &dir,
        normal.as_ref().map(|n| &n[..]),
    );
    let mut model =
        StructuralMechanicsModel::with_compat(shared(mesh), None, ty.name(), compat).unwrap();
    model.add_material(material(), "").unwrap();
    model
        .init_full_with(AnalysisMethod::ImplicitDynamic)
        .unwrap();
    model
        .element_material_mut(ty, G)
        .unwrap()
        .as_mut_slice()
        .fill(0);
    let nd = ndof(ty);
    let nb_nodes = NB_ELEMENT + 1;
    {
        let mut b_g = model.blocked_dofs_mut().unwrap();
        let b = b_g.as_mut_slice();
        b.fill(false);
        b[0] = true; // (0, x)
        b[1] = true; // (0, y)
        b[(nb_nodes - 1) * nd + 1] = true;
        if dim == 3 {
            b[2] = true;
            b[(nb_nodes - 1) * nd + 2] = true;
        }
    }
    {
        let mut f_guard = model.external_force_mut().unwrap();
        let f = f_guard.as_mut_slice();
        f.fill(0.);
        f[(nb_nodes / 2) * nd + 1] = F;
    }
    model
}

/// Hand-built element mass matrix (row-major), torsion row weighted with
/// `torsion_rho` (upstream: `ρ·A`).
fn element_mass(ty: ElementType, m: &StructuralMaterial, l: Real, torsion: Real) -> Vec<Vec<Real>> {
    let c = m.rho * m.a * l / 420.;
    let ll = l * l;
    let rows: Vec<Vec<Real>> = if ty == ElementType::BernoulliBeam2 {
        vec![
            vec![140., 0., 0., 70., 0., 0.],
            vec![0., 156., 22. * l, 0., 54., -13. * l],
            vec![0., 22. * l, 4. * ll, 0., 13. * l, -3. * ll],
            vec![70., 0., 0., 140., 0., 0.],
            vec![0., 54., 13. * l, 0., 156., -22. * l],
            vec![0., -13. * l, -3. * ll, 0., -22. * l, 4. * ll],
        ]
    } else {
        #[rustfmt::skip]
        let r = vec![
            vec![140., 0., 0., 0., 0., 0., 70., 0., 0., 0., 0., 0.],
            vec![0., 156., 0., 0., 0., 22. * l, 0., 54., 0., 0., 0., -13. * l],
            vec![0., 0., 156., 0., -22. * l, 0., 0., 0., 54., 0., 13. * l, 0.],
            vec![0., 0., 0., 140., 0., 0., 0., 0., 0., 70., 0., 0.],
            vec![0., 0., -22. * l, 0., 4. * ll, 0., 0., 0., -13. * l, 0., -3. * ll, 0.],
            vec![0., 22. * l, 0., 0., 0., 4. * ll, 0., 13. * l, 0., 0., 0., -3. * ll],
            vec![70., 0., 0., 0., 0., 0., 140., 0., 0., 0., 0., 0.],
            vec![0., 54., 0., 0., 0., 13. * l, 0., 156., 0., 0., 0., -22. * l],
            vec![0., 0., 54., 0., -13. * l, 0., 0., 0., 156., 0., 22. * l, 0.],
            vec![0., 0., 0., 70., 0., 0., 0., 0., 0., 140., 0., 0.],
            vec![0., 0., 13. * l, 0., -3. * ll, 0., 0., 0., 22. * l, 0., 4. * ll, 0.],
            vec![0., -13. * l, 0., 0., 0., -3. * ll, 0., -22. * l, 0., 0., 0., 4. * ll],
        ];
        r
    };
    let torsion_scale = torsion / (m.rho * m.a);
    rows.into_iter()
        .enumerate()
        .map(|(i, row)| {
            row.into_iter()
                .enumerate()
                .map(|(j, v)| {
                    let t = ty == ElementType::BernoulliBeam3 && (i % 6 == 3) && (j % 6 == 3);
                    if t { c * v * torsion_scale } else { c * v }
                })
                .collect()
        })
        .collect()
}

/// Hand-built element stiffness matrix (row-major).
fn element_stiffness(ty: ElementType, m: &StructuralMaterial, l: Real) -> Vec<Vec<Real>> {
    let (e, a) = (m.e, m.a);
    if ty == ElementType::BernoulliBeam2 {
        let i = m.i;
        let (l2, l3) = (l * l, l * l * l);
        return vec![
            vec![e * a / l, 0., 0., -e * a / l, 0., 0.],
            vec![
                0.,
                12. * e * i / l3,
                6. * e * i / l2,
                0.,
                -12. * e * i / l3,
                6. * e * i / l2,
            ],
            vec![
                0.,
                6. * e * i / l2,
                4. * e * i / l,
                0.,
                -6. * e * i / l2,
                2. * e * i / l,
            ],
            vec![-e * a / l, 0., 0., e * a / l, 0., 0.],
            vec![
                0.,
                -12. * e * i / l3,
                -6. * e * i / l2,
                0.,
                12. * e * i / l3,
                -6. * e * i / l2,
            ],
            vec![
                0.,
                6. * e * i / l2,
                2. * e * i / l,
                0.,
                -6. * e * i / l2,
                4. * e * i / l,
            ],
        ];
    }
    let a1 = e * a / l;
    let b1 = 12. * e * m.iz / l / l / l;
    let b2 = 6. * e * m.iz / l / l;
    let b3 = 4. * e * m.iz / l;
    let b4 = 2. * e * m.iz / l;
    let c1 = 12. * e * m.iy / l / l / l;
    let c2 = 6. * e * m.iy / l / l;
    let c3 = 4. * e * m.iy / l;
    let c4 = 2. * e * m.iy / l;
    let d1 = m.gj / l;
    #[rustfmt::skip]
    let k = vec![
        vec![a1, 0., 0., 0., 0., 0., -a1, 0., 0., 0., 0., 0.],
        vec![0., b1, 0., 0., 0., b2, 0., -b1, 0., 0., 0., b2],
        vec![0., 0., c1, 0., -c2, 0., 0., 0., -c1, 0., -c2, 0.],
        vec![0., 0., 0., d1, 0., 0., 0., 0., 0., -d1, 0., 0.],
        vec![0., 0., -c2, 0., c3, 0., 0., 0., c2, 0., c4, 0.],
        vec![0., b2, 0., 0., 0., b3, 0., -b2, 0., 0., 0., b4],
        vec![-a1, 0., 0., 0., 0., 0., a1, 0., 0., 0., 0., 0.],
        vec![0., -b1, 0., 0., 0., -b2, 0., b1, 0., 0., 0., -b2],
        vec![0., 0., -c1, 0., c2, 0., 0., 0., c1, 0., c2, 0.],
        vec![0., 0., 0., -d1, 0., 0., 0., 0., 0., d1, 0., 0.],
        vec![0., 0., -c2, 0., c4, 0., 0., 0., c2, 0., c3, 0.],
        vec![0., b2, 0., 0., 0., b4, 0., -b2, 0., 0., 0., b3],
    ];
    k
}

/// Dense assembly of one element matrix per element of the line mesh.
fn assemble_line(nd: usize, me: &[Vec<Real>]) -> Vec<Vec<Real>> {
    let n = (NB_ELEMENT + 1) * nd;
    let mut a = vec![vec![0.0; n]; n];
    for e in 0..NB_ELEMENT {
        let (n1, n2) = (e, e + 1);
        for i in 0..nd {
            for j in 0..nd {
                a[n1 * nd + i][n1 * nd + j] += me[i][j];
                a[n2 * nd + i][n2 * nd + j] += me[nd + i][nd + j];
                a[n1 * nd + i][n2 * nd + j] += me[i][nd + j];
                a[n2 * nd + i][n1 * nd + j] += me[nd + i][j];
            }
        }
    }
    a
}

fn beam_matrices(ty: ElementType, compat: StructuralModelUpstreamCompat, torsion: Real) {
    let mut model = fixture(ty, compat);
    model.assemble_matrix("M").unwrap();
    model.assemble_matrix("K").unwrap();
    let k = dense(&model, "K");
    let m = dense(&model, "M");
    let mat = material();
    let nd = ndof(ty);
    let ka = assemble_line(nd, &element_stiffness(ty, &mat, le()));
    let ma = assemble_line(nd, &element_mass(ty, &mat, le(), torsion));
    let tol = 1e-13;
    let (ka_max, ma_max) = (max_abs(&ka), max_abs(&ma));
    for i in 0..ka.len() {
        for j in 0..ka.len() {
            assert!(
                (ka[i][j] - k[i][j]).abs() <= tol * ka_max,
                "K({i},{j}): {} vs {}",
                k[i][j],
                ka[i][j]
            );
            assert!(
                (ma[i][j] - m[i][j]).abs() <= tol * ma_max,
                "M({i},{j}): {} vs {}",
                m[i][j],
                ma[i][j]
            );
        }
    }
}

/// `TestStructBernoulliDynamic/0.TestBeamMatrices` (`_bernoulli_beam_2`).
#[test]
fn test_struct_bernoulli_dynamic_test_beam_matrices_bernoulli_beam_2() {
    let m = material();
    beam_matrices(
        ElementType::BernoulliBeam2,
        StructuralModelUpstreamCompat::default(),
        m.rho * m.a,
    );
}

/// `TestStructBernoulliDynamic/1.TestBeamMatrices` (`_bernoulli_beam_3`;
/// the reference matrix encodes upstream's torsion row, UD-507).
#[test]
fn test_struct_bernoulli_dynamic_test_beam_matrices_bernoulli_beam_3() {
    let m = material();
    let compat = StructuralModelUpstreamCompat {
        beam3_torsional_mass_rho_a: true,
        ..StructuralModelUpstreamCompat::default()
    };
    beam_matrices(ElementType::BernoulliBeam3, compat, m.rho * m.a);
}

/// Corrected default of the 3-D beam mass (UD-507): only the torsion row
/// changes, to `ρ·(Iy + Iz)·l/420·[140, 70]`.
#[test]
fn beam_matrices_bernoulli_beam_3_corrected_torsional_mass() {
    let m = material();
    beam_matrices(
        ElementType::BernoulliBeam3,
        StructuralModelUpstreamCompat::default(),
        m.rho * (m.iy + m.iz),
    );
}

/// Three-term modal series of upstream (`analytical_solution`).
fn analytical_solution(time: Real, l: Real, rho: Real, e: Real, i: Real, f: Real) -> Real {
    use std::f64::consts::PI;
    let omega = PI * PI / l / l * (e * i / rho).sqrt();
    let mut sum = 0.;
    for n in (1..=5).step_by(2) {
        let n = n as Real;
        sum += (1. - (n * n * omega * time).cos()) / n.powi(4);
    }
    2. * f * l.powi(3) / PI.powi(4) / e / i * sum
}

fn beam_oscillation(ty: ElementType, compat: StructuralModelUpstreamCompat) {
    let mut model = fixture(ty, compat);
    let time_step = 1e-6;
    model.set_time_step(time_step, None).unwrap();
    {
        let nls = model.non_linear_solver_mut(None).unwrap();
        nls.set_param("max_iterations", ParamValue::Int(100))
            .unwrap();
        nls.set_param("threshold", ParamValue::Real(1e-8)).unwrap();
        nls.set_param("convergence_type", ParamValue::Enum("solution".into()))
            .unwrap();
    }
    let nd = ndof(ty);
    let nb_nodes = NB_ELEMENT + 1;
    let node_to_print = nb_nodes / 2;
    model.displacement_mut().unwrap().as_mut_slice().fill(0.);
    let mat = material();
    let tol = 1e-6;
    for s in 1..300 {
        model.solve_step(None).unwrap();
        let time = s as Real * time_step;
        let da = analytical_solution(time, L, mat.rho, mat.e, mat.iy, F);
        let d = model.displacement().unwrap().as_slice()[node_to_print * nd + 1];
        assert!((d - da).abs() <= tol, "step {s}: {d} vs {da}");
    }
}

/// `TestStructBernoulliDynamic/0.TestBeamOscilation` (`_bernoulli_beam_2`).
#[test]
fn test_struct_bernoulli_dynamic_test_beam_oscilation_bernoulli_beam_2() {
    beam_oscillation(
        ElementType::BernoulliBeam2,
        StructuralModelUpstreamCompat::default(),
    );
}

/// `TestStructBernoulliDynamic/1.TestBeamOscilation` (`_bernoulli_beam_3`,
/// corrected default; the torsion DOFs are not excited).
#[test]
fn test_struct_bernoulli_dynamic_test_beam_oscilation_bernoulli_beam_3() {
    beam_oscillation(
        ElementType::BernoulliBeam3,
        StructuralModelUpstreamCompat::default(),
    );
}

/// Same in upstream mode.
#[test]
fn test_struct_bernoulli_dynamic_test_beam_oscilation_bernoulli_beam_3_upstream_compat() {
    beam_oscillation(
        ElementType::BernoulliBeam3,
        StructuralModelUpstreamCompat::UPSTREAM,
    );
}
