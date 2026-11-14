// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): ports of
// test/test_fe_engine/test_fe_engine_precomputation_structural.cc
// (TestBernoulliB2/B3.PrecomputeRotations, TestDKT18.DISABLED_PrecomputeRotations)
// and of the unregistered test_fe_engine_precomputation_bernoulli_{2,3}.cc.

//! Rotation matrices and shapes precomputed by the structural FE engine on
//! the upstream test meshes (`test/test_fe_engine/_bernoulli_beam_{2,3}.msh`,
//! the DKT patch mesh), upstream tolerances (`Math::setTolerance(1e-15)` on
//! the Frobenius norm of the error).

mod structural_common;

use akantu_core::{ElementType, Real};
use structural_common::{G, engine, mesh};

fn frobenius(a: &[Real], b: &[Real]) -> Real {
    a.iter()
        .zip(b)
        .map(|(x, y)| (x - y) * (x - y))
        .sum::<Real>()
        .sqrt()
}

/// `globalToLocalRotation(theta)` of the upstream test (column-major).
fn global_to_local(theta: Real) -> [Real; 9] {
    let (c, s) = (theta.cos(), theta.sin());
    [c, -s, 0., s, c, 0., 0., 0., 1.]
}

/// TestBernoulliB2.PrecomputeRotations (`_bernoulli_beam_2.msh`: nodes
/// (0, 0), (0.6, 0.8), (1.2, 0); elements (1, 2), (2, 3)).
#[test]
fn test_bernoulli_b2_precompute_rotations() {
    let ty = ElementType::BernoulliBeam2;
    let m = mesh(
        2,
        ty,
        &[vec![0., 0.], vec![0.6, 0.8], vec![1.2, 0.]],
        &[vec![0, 1], vec![1, 2]],
        None,
    );
    let fe = engine(&m);
    let rot = fe.rotations(&m, ty, G).unwrap();
    let a = (4.0_f64 / 3.).atan();
    // upstream zips the rotations with {a, -a, 0}: the mesh has two elements
    for (r, angle) in rot.as_slice().chunks(9).zip([a, -a, 0.]) {
        assert!(frobenius(r, &global_to_local(angle)) <= 1e-15, "{r:?}");
    }
    assert_eq!(rot.len(), 2);
}

/// TestBernoulliB3.PrecomputeRotations (`_bernoulli_beam_3.msh`: (0, 0, 0)
/// → (3, 4, 12), extra normal (−36/65, −48/65, 5/13)).
#[test]
fn test_bernoulli_b3_precompute_rotations() {
    let ty = ElementType::BernoulliBeam3;
    let m = mesh(
        3,
        ty,
        &[vec![0., 0., 0.], vec![3., 4., 12.]],
        &[vec![0, 1]],
        Some(&[vec![-36. / 65., -48. / 65., 5. / 13.]]),
    );
    let fe = engine(&m);
    let reference = [
        [3. / 13., 4. / 13., 12. / 13.],
        [-4. / 5., 3. / 5., 0.],
        [-36. / 65., -48. / 65., 5. / 13.],
    ];
    let mut solution = [0.0; 36];
    for i in 0..3 {
        for j in 0..3 {
            solution[i + 6 * j] = reference[i][j];
            solution[(i + 3) + 6 * (j + 3)] = reference[i][j];
        }
    }
    for r in fe.rotations(&m, ty, G).unwrap().as_slice().chunks(36) {
        assert!(frobenius(r, &solution) <= 1e-15, "{r:?}");
    }
}

/// TestDKT18.DISABLED_PrecomputeRotations only prints the rotations
/// upstream; ported with its intent: on the DKT patch mesh every rotation
/// is block-diagonal with two copies of an orthonormal `P`, `det P = +1`,
/// whose first row is the first edge direction and whose last row is the
/// unit normal of the (z = 0) triangle.
#[test]
fn test_dkt18_precompute_rotations() {
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    let nodes = vec![
        vec![0., 0., 0.],
        vec![40., 0., 0.],
        vec![40., 20., 0.],
        vec![0., 20., 0.],
        vec![15., 15., 0.],
    ];
    let conn = vec![vec![0, 4, 1], vec![1, 4, 2], vec![2, 3, 4], vec![0, 3, 4]];
    let m = mesh(3, ty, &nodes, &conn, None);
    let fe = engine(&m);
    for (r, c) in fe
        .rotations(&m, ty, G)
        .unwrap()
        .as_slice()
        .chunks(36)
        .zip(&conn)
    {
        let p = |i: usize, j: usize| r[i + 6 * j];
        for i in 0..3 {
            for j in 0..3 {
                assert_eq!(p(i, j), p(i + 3, j + 3));
                assert_eq!(p(i, j + 3), 0.);
                assert_eq!(p(i + 3, j), 0.);
                let d: Real = (0..3).map(|k| p(i, k) * p(j, k)).sum();
                assert!((d - if i == j { 1. } else { 0. }).abs() < 1e-15);
            }
        }
        let det = p(0, 0) * (p(1, 1) * p(2, 2) - p(1, 2) * p(2, 1))
            - p(0, 1) * (p(1, 0) * p(2, 2) - p(1, 2) * p(2, 0))
            + p(0, 2) * (p(1, 0) * p(2, 1) - p(1, 1) * p(2, 0));
        assert!((det - 1.).abs() < 1e-15);
        let (a, b) = (&nodes[c[0] as usize], &nodes[c[1] as usize]);
        let len = ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2)).sqrt();
        assert!((p(0, 0) - (b[0] - a[0]) / len).abs() < 1e-15);
        assert!((p(0, 1) - (b[1] - a[1]) / len).abs() < 1e-15);
        assert_eq!((p(2, 0), p(2, 1)), (0., 0.));
        assert_eq!(p(2, 2).abs(), 1.);
    }
}

/// test_fe_engine_precomputation_bernoulli_2.cc (not registered upstream):
/// rotations of the three elements (0,1), (1,2), (0,2) and the axial row of
/// the shapes at the Gauss points (`axialReference`, `Math::are_vector_equal`
/// with the default tolerance).
#[test]
fn test_fe_engine_precomputation_bernoulli_2() {
    let ty = ElementType::BernoulliBeam2;
    let m = mesh(
        2,
        ty,
        &[vec![0., 0.], vec![3. / 5., 4. / 5.], vec![2. * 3. / 5., 0.]],
        &[vec![0, 1], vec![1, 2], vec![0, 2]],
        None,
    );
    let fe = engine(&m);
    let a = (4.0_f64 / 3.).atan();
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    for (r, angle) in fe
        .rotations(&m, ty, G)
        .unwrap()
        .as_slice()
        .chunks(9)
        .zip([a, -a, 0.])
    {
        for (x, y) in r.iter().zip(global_to_local(angle)) {
            assert!((x - y).abs() <= tol, "{r:?}");
        }
    }
    // testBending(shape_functions, 0, axialReference): xq = -1/sqrt(3), then
    // alternating sign for each successive quadrature point; the axial row of
    // N·T in the local basis is the row of N rotated by T, i.e. for line 0
    // [c L0, s L0, 0, c L1, s L1, 0].
    let shapes = fe.shapes(&m, ty, G).unwrap().as_slice();
    let mut xq = -1. / 3.0_f64.sqrt();
    for (k, nt) in shapes.chunks(18).enumerate() {
        let angle = [a, -a, 0.][k / 2];
        let (c, s) = (angle.cos(), angle.sin());
        let (l0, l1) = ((1. - xq) / 2., (1. + xq) / 2.);
        let reference = [c * l0, s * l0, 0., c * l1, s * l1, 0.];
        for (col, want) in reference.iter().enumerate() {
            assert!((nt[3 * col] - want).abs() <= tol, "point {k}");
        }
        xq = -xq;
    }
}

/// test_fe_engine_precomputation_bernoulli_3.cc (not registered upstream):
/// rotations of a 3-D beam with two (identical) normals pushed.
#[test]
fn test_fe_engine_precomputation_bernoulli_3() {
    let ty = ElementType::BernoulliBeam3;
    let n = vec![-36. / 65., -48. / 65., 5. / 13.];
    let m = mesh(
        3,
        ty,
        &[vec![0., 0., 0.], vec![3., 4., 12.]],
        &[vec![0, 1]],
        Some(std::slice::from_ref(&n)),
    );
    let fe = engine(&m);
    let r = fe.rotations(&m, ty, G).unwrap().as_slice();
    let tol = akantu_core::math::DEFAULT_TOLERANCE;
    let reference = [
        [3. / 13., 4. / 13., 12. / 13.],
        [-4. / 5., 3. / 5., 0.],
        [n[0], n[1], n[2]],
    ];
    for i in 0..6 {
        for j in 0..6 {
            let want = if (i < 3) == (j < 3) {
                reference[i % 3][j % 3]
            } else {
                0.
            };
            assert!((r[i + 6 * j] - want).abs() <= tol);
        }
    }
}
