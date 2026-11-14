// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): element-level ports of
// test/test_model/test_structural_mechanics_model/
// test_structural_mechanics_model_{bernoulli_beam_2,bernoulli_beam_3,
// discrete_kirchhoff_triangle_18,bernoulli_beam_dynamics}.cc.

//! The structural model tests with the model replaced by a dense assembly
//! of the port's element matrices (`StructuralFeEngine::element_stiffness`,
//! `consistent_mass`) and a dense solve of the blocked system — the parts
//! that exercise element kernels. Upstream tolerances: `Math::getTolerance()`
//! (default `100 ε`), `1e-12` (DKT), `1e-13 · max` (beam matrices).
//! `TestStructBernoulliDynamic.TestBeamOscilation` (time integration) and the
//! assembled-model plumbing are left to the structural model work package.

mod structural_common;

use akantu_core::{Array, ElementType, Idx, Real};
use akantu_fe::structural::kernels::{
    DktBendingStiffness, tangent_bernoulli_beam_2, tangent_bernoulli_beam_3,
    tangent_discrete_kirchhoff_triangle_18,
};
use structural_common::{G, assemble, element_stiffness, engine, max_abs, mesh, solve_blocked};

const MATH_TOLERANCE: Real = akantu_core::math::DEFAULT_TOLERANCE;

/// TestStructBernoulli2.TestDisplacements (mesh 0, 10, 18 on x; two
/// materials; distributed load replaced by its nodal equivalent as upstream).
#[test]
fn test_struct_bernoulli_2_displacements() {
    let ty = ElementType::BernoulliBeam2;
    let nodes = vec![vec![0., 0.], vec![10., 0.], vec![18., 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![1, 2]];
    let m = mesh(2, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let d = vec![
        tangent_bernoulli_beam_2(3e10, 0.01, 0.0025).to_vec(),
        tangent_bernoulli_beam_2(3e10, 0.01, 0.00128).to_vec(),
    ];
    let k = assemble(3, 3, &conn, &element_stiffness(&fe, &m, ty, &d));
    let (mm, q, l) = (3600., 6000., 10.);
    let mut f = vec![0.0; 9];
    f[2 * 3 + 2] = -mm;
    f[1] = -q * l / 2.;
    f[2] = -q * l * l / 12.;
    f[3 + 1] = -q * l / 2.;
    f[3 + 2] = q * l * l / 12.;
    f[2 * 3] = 3e10 * 0.01 / 18.;
    let blocked = [true, true, true, false, true, false, false, true, false];
    let mut u = vec![0.0; 9];
    solve_blocked(&k, &f, &mut u, &blocked);
    assert!((u[3 + 2] - 5.6 / 4800.).abs() <= MATH_TOLERANCE, "{}", u[5]);
    assert!(
        (u[2 * 3 + 2] + 3.7 / 4800.).abs() <= MATH_TOLERANCE,
        "{}",
        u[8]
    );
    assert!((u[3] - 10. / 18.).abs() <= MATH_TOLERANCE, "{}", u[3]);
}

/// TestStructBernoulli3Static.TestDisplacements (two beams at ±45° in the
/// xy-plane, normals z, unit properties, P = 1 along −z at the free node).
#[test]
fn test_struct_bernoulli_3_static_displacements() {
    let ty = ElementType::BernoulliBeam3;
    // the mesh literal 0.7071067811865476 is FRAC_1_SQRT_2 (same f64)
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let nodes = vec![vec![0., 0., 0.], vec![s, s, 0.], vec![s, -s, 0.]];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 1], vec![0, 2]];
    let normals = vec![vec![0., 0., 1.], vec![0., 0., 1.]];
    let m = mesh(3, ty, &nodes, &conn, Some(&normals));
    let fe = engine(&m);
    let d = vec![tangent_bernoulli_beam_3(1., 1., 1., 1., 1.).to_vec(); 2];
    let k = assemble(3, 6, &conn, &element_stiffness(&fe, &m, ty, &d));
    let mut f = vec![0.0; 18];
    f[2] = -1.;
    let blocked: Vec<bool> = (0..18).map(|i| i >= 6).collect();
    let mut u = vec![0.0; 18];
    solve_blocked(&k, &f, &mut u, &blocked);
    assert!((u[2] + 5. / 48.).abs() <= MATH_TOLERANCE, "vz {}", u[2]);
    assert!(
        (u[4] + 2.0_f64.sqrt() / 8.).abs() <= MATH_TOLERANCE,
        "thy {}",
        u[4]
    );
    assert!(u[5].abs() <= MATH_TOLERANCE, "thz {}", u[5]);
    assert!(u[3].abs() <= MATH_TOLERANCE, "thx {}", u[3]);
}

/// TestStructDKT18.TestDisplacements: Batoz vol. 2 p. 392 patch test
/// (membrane + bending field imposed on the boundary nodes, centre node
/// free except θz).
#[test]
fn test_struct_dkt18_displacements() {
    let ty = ElementType::DiscreteKirchhoffTriangle18;
    let nodes = vec![
        vec![0., 0., 0.],
        vec![40., 0., 0.],
        vec![40., 20., 0.],
        vec![0., 20., 0.],
        vec![15., 15., 0.],
    ];
    let conn: Vec<Vec<Idx>> = vec![vec![0, 4, 1], vec![1, 4, 2], vec![2, 3, 4], vec![0, 3, 4]];
    let m = mesh(3, ty, &nodes, &conn, None);
    let fe = engine(&m);
    let d = vec![
        tangent_discrete_kirchhoff_triangle_18(1., 0.3, 1., DktBendingStiffness::default())
            .to_vec();
        4
    ];
    let k = assemble(5, 6, &conn, &element_stiffness(&fe, &m, ty, &d));
    let mut u = vec![0.0; 30];
    let imposed = [
        [40., 20., -800., -20., 40., 0.],
        [50., 40., -1400., -40., 50., 0.],
        [10., 20., -200., -20., 10., 0.],
    ];
    for (n, v) in imposed.iter().enumerate() {
        u[(n + 1) * 6..(n + 2) * 6].copy_from_slice(v);
    }
    let mut blocked = vec![true; 30];
    for b in &mut blocked[24..29] {
        *b = false;
    }
    solve_blocked(&k, &vec![0.0; 30], &mut u, &blocked);
    let solution = [22.5, 22.5, -337.5, -22.5, 22.5, 0.];
    let err: Real = u[24..30]
        .iter()
        .zip(solution)
        .map(|(a, b)| (a - b) * (a - b))
        .sum::<Real>()
        .sqrt();
    assert!(err <= 1e-12, "{:?}", &u[24..30]);
}

/// Analytical beam element matrices of the dynamics test
/// (`getElementMassMatrix`, `getElementStifnessMatrix`), column-major.
fn analytical_beam_2(l: Real, e: Real, a: Real, i: Real, rho: Real) -> (Vec<Real>, Vec<Real>) {
    let (l2, l3) = (l * l, l * l * l);
    let k = [
        [e * a / l, 0., 0., -e * a / l, 0., 0.],
        [
            0.,
            12. * e * i / l3,
            6. * e * i / l2,
            0.,
            -12. * e * i / l3,
            6. * e * i / l2,
        ],
        [
            0.,
            6. * e * i / l2,
            4. * e * i / l,
            0.,
            -6. * e * i / l2,
            2. * e * i / l,
        ],
        [-e * a / l, 0., 0., e * a / l, 0., 0.],
        [
            0.,
            -12. * e * i / l3,
            -6. * e * i / l2,
            0.,
            12. * e * i / l3,
            -6. * e * i / l2,
        ],
        [
            0.,
            6. * e * i / l2,
            2. * e * i / l,
            0.,
            -6. * e * i / l2,
            4. * e * i / l,
        ],
    ];
    let m = [
        [140., 0., 0., 70., 0., 0.],
        [0., 156., 22. * l, 0., 54., -13. * l],
        [0., 22. * l, 4. * l * l, 0., 13. * l, -3. * l * l],
        [70., 0., 0., 140., 0., 0.],
        [0., 54., 13. * l, 0., 156., -22. * l],
        [0., -13. * l, -3. * l * l, 0., -22. * l, 4. * l * l],
    ];
    let c = rho * a * l / 420.;
    let col = |t: &[[Real; 6]; 6], s: Real| -> Vec<Real> {
        (0..36).map(|x| t[x % 6][x / 6] * s).collect()
    };
    (col(&k, 1.), col(&m, c))
}

#[allow(clippy::too_many_arguments)]
fn analytical_beam_3(
    l: Real,
    e: Real,
    a: Real,
    iz: Real,
    iy: Real,
    gj: Real,
    rho: Real,
) -> (Vec<Real>, Vec<Real>) {
    let a1 = e * a / l;
    let (b1, b2, b3, b4) = (
        12. * e * iz / l / l / l,
        6. * e * iz / l / l,
        4. * e * iz / l,
        2. * e * iz / l,
    );
    let (c1, c2, c3, c4) = (
        12. * e * iy / l / l / l,
        6. * e * iy / l / l,
        4. * e * iy / l,
        2. * e * iy / l,
    );
    let d1 = gj / l;
    let k: [[Real; 12]; 12] = [
        [a1, 0., 0., 0., 0., 0., -a1, 0., 0., 0., 0., 0.],
        [0., b1, 0., 0., 0., b2, 0., -b1, 0., 0., 0., b2],
        [0., 0., c1, 0., -c2, 0., 0., 0., -c1, 0., -c2, 0.],
        [0., 0., 0., d1, 0., 0., 0., 0., 0., -d1, 0., 0.],
        [0., 0., -c2, 0., c3, 0., 0., 0., c2, 0., c4, 0.],
        [0., b2, 0., 0., 0., b3, 0., -b2, 0., 0., 0., b4],
        [-a1, 0., 0., 0., 0., 0., a1, 0., 0., 0., 0., 0.],
        [0., -b1, 0., 0., 0., -b2, 0., b1, 0., 0., 0., -b2],
        [0., 0., -c1, 0., c2, 0., 0., 0., c1, 0., c2, 0.],
        [0., 0., 0., -d1, 0., 0., 0., 0., 0., d1, 0., 0.],
        [0., 0., -c2, 0., c4, 0., 0., 0., c2, 0., c3, 0.],
        [0., b2, 0., 0., 0., b4, 0., -b2, 0., 0., 0., b3],
    ];
    let ll = l * l;
    let m: [[Real; 12]; 12] = [
        [140., 0., 0., 0., 0., 0., 70., 0., 0., 0., 0., 0.],
        [0., 156., 0., 0., 0., 22. * l, 0., 54., 0., 0., 0., -13. * l],
        [0., 0., 156., 0., -22. * l, 0., 0., 0., 54., 0., 13. * l, 0.],
        [0., 0., 0., 140., 0., 0., 0., 0., 0., 70., 0., 0.],
        [
            0.,
            0.,
            -22. * l,
            0.,
            4. * ll,
            0.,
            0.,
            0.,
            -13. * l,
            0.,
            -3. * ll,
            0.,
        ],
        [
            0.,
            22. * l,
            0.,
            0.,
            0.,
            4. * ll,
            0.,
            13. * l,
            0.,
            0.,
            0.,
            -3. * ll,
        ],
        [70., 0., 0., 0., 0., 0., 140., 0., 0., 0., 0., 0.],
        [0., 54., 0., 0., 0., 13. * l, 0., 156., 0., 0., 0., -22. * l],
        [0., 0., 54., 0., -13. * l, 0., 0., 0., 156., 0., 22. * l, 0.],
        [0., 0., 0., 70., 0., 0., 0., 0., 0., 140., 0., 0.],
        [
            0.,
            0.,
            13. * l,
            0.,
            -3. * ll,
            0.,
            0.,
            0.,
            22. * l,
            0.,
            4. * ll,
            0.,
        ],
        [
            0.,
            -13. * l,
            0.,
            0.,
            0.,
            -3. * ll,
            0.,
            -22. * l,
            0.,
            0.,
            0.,
            4. * ll,
        ],
    ];
    let c = rho * a * l / 420.;
    let col = |t: &[[Real; 12]; 12], s: Real| -> Vec<Real> {
        (0..144).map(|x| t[x % 12][x / 12] * s).collect()
    };
    (col(&k, 1.), col(&m, c))
}

/// TestStructBernoulliDynamic.TestBeamMatrices (beam_2 and beam_3): the
/// assembled stiffness and consistent mass of a 40-element beam of length 2
/// equal the assembled analytical element matrices to `1e-13 · max`.
fn beam_matrices(ty: ElementType) {
    let (nb_element, len) = (40usize, 2.0);
    let le = len / nb_element as Real;
    let dim = ty.spatial_dimension();
    let ndof = if dim == 2 { 3 } else { 6 };
    let nodes: Vec<Vec<Real>> = (0..=nb_element)
        .map(|i| {
            let mut x = vec![0.0; dim];
            x[0] = i as Real * le;
            x
        })
        .collect();
    let conn: Vec<Vec<Idx>> = (0..nb_element)
        .map(|i| vec![i as Idx, i as Idx + 1])
        .collect();
    let normals: Vec<Vec<Real>> = vec![vec![0., 0., 1.]; nb_element];
    let m = mesh(dim, ty, &nodes, &conn, (dim == 3).then_some(&normals[..]));
    let fe = engine(&m);
    let (e, rho) = (1e9, 10.);
    let (d, ns, (ke, me)) = if dim == 2 {
        (
            tangent_bernoulli_beam_2(e, 1., 1.).to_vec(),
            2,
            analytical_beam_2(le, e, 1., 1., rho),
        )
    } else {
        (
            tangent_bernoulli_beam_3(e, 1., 1., 1., 1.).to_vec(),
            4,
            analytical_beam_3(le, e, 1., 1., 1., 1., rho),
        )
    };
    let k = assemble(
        nb_element + 1,
        ndof,
        &conn,
        &element_stiffness(&fe, &m, ty, &vec![d; nb_element]),
    );
    let nqm = akantu_fe::gauss_rule(ty, fe.integration_orders().mass_order(ty))
        .unwrap()
        .nb_points();
    let field = Array::from_vec(vec![rho * 1.; nb_element * nqm * ns], ns).unwrap();
    let mut me_port = Array::new(1);
    fe.consistent_mass(&m, field.view(), &mut me_port, ty, G)
        .unwrap();
    let mats: Vec<Vec<Real>> = me_port
        .as_slice()
        .chunks(me_port.nb_component())
        .map(<[Real]>::to_vec)
        .collect();
    let mass = assemble(nb_element + 1, ndof, &conn, &mats);
    let ka = assemble(nb_element + 1, ndof, &conn, &vec![ke; nb_element]);
    let ma = assemble(nb_element + 1, ndof, &conn, &vec![me; nb_element]);
    let (kmax, mmax) = (max_abs(&ka), max_abs(&ma));
    for ((a, b), (c, dd)) in ka.iter().zip(&k).zip(ma.iter().zip(&mass)) {
        assert!((a - b).abs() <= 1e-13 * kmax, "K {a} vs {b}");
        assert!((c - dd).abs() <= 1e-13 * mmax, "M {c} vs {dd}");
    }
}

#[test]
fn test_struct_bernoulli_dynamic_beam_matrices_bernoulli_beam_2() {
    beam_matrices(ElementType::BernoulliBeam2);
}

#[test]
fn test_struct_bernoulli_dynamic_beam_matrices_bernoulli_beam_3() {
    beam_matrices(ElementType::BernoulliBeam3);
}
