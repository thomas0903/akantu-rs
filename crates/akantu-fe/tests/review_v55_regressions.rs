// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): regression tests
// on the structural FE kernels ported from src/fe_engine/
// element_classes/element_class_{hermite,bernoulli_beam,kirchhoff_shell}_inline_impl.hh,
// shape_structural*, integrator_gauss_inline_impl.hh. Test-only code.

//!
//! * **`fs01_*`** degenerate / non-finite geometry is refused with
//!   `Error::Numerical` naming the element (no `inf`/`NaN` cache), thin
//!   but finite elements are accepted (UD-296);
//! * **`fs02_*`** the 3-D beam θy row is `−dw/ds` by default (nodal,
//!   rigid-rotation, moment virtual-work checks); the legacy switch
//!   reproduces upstream's `+dw/ds` (UD-293, D-003 B);
//! * **`fs03_*`** stale precomputed data (moved nodes, changed connectivity or
//!   `extra_normal`, re-initialised other ghost type) are refused before
//!   the output is touched; `refresh` and mesh events rebuild in full and
//!   drop vanished blocks;
//! * **`fs04_*`** malformed inputs of the public element API are errors, never
//!   panics (Beam2 rotation with 3-D coordinates, short Voigt buffers,
//!   lumped-mass coordinates); `Coords` cannot be forged (`compile_fail`
//!   doc test on `Coords`).
//!
//! The first sections use only API that existed before the fix, so they
//! also run on that version and show the defects there.
//! Tolerances: exact (`==` / bit patterns) unless stated; `1e-15` for the
//! θy values of a closed-form cubic evaluated in a few rounded operations.

mod structural_common;

use akantu_core::{Array, ElementType as E, Error, GhostType, Idx, Real};
use akantu_fe::element_class::types::{
    BernoulliBeam2, BernoulliBeam3, DiscreteKirchhoffTriangle18,
};
use akantu_fe::structural::element::{dkt_basis_change, hermite_dnds, hermite_shapes};
use akantu_fe::structural::{Coords, EXTRA_NORMAL, StructuralElement, StructuralFeEngine};
use akantu_mesh::{Mesh, MeshAccessor};
use structural_common::{G, engine, mesh};

/// Node rows.
type Nodes = Vec<Vec<Real>>;
/// One geometry case: spatial dimension, type, nodes, `extra_normal` rows.
type Case = (usize, E, Nodes, Option<Nodes>);

fn nodes2(v: &[[Real; 2]]) -> Vec<Vec<Real>> {
    v.iter().map(|x| x.to_vec()).collect()
}

fn nodes3(v: &[[Real; 3]]) -> Vec<Vec<Real>> {
    v.iter().map(|x| x.to_vec()).collect()
}

fn refused(m: &Mesh, case: &str) -> String {
    match StructuralFeEngine::with_precomputation(m, None, "v55") {
        Err(Error::Numerical(msg)) => msg,
        Err(other) => panic!("{case}: expected Error::Numerical, got {other:?}"),
        Ok(fe) => {
            let ty = fe.element_types(m, G).next().unwrap();
            panic!(
                "{case}: accepted; jacobians {:?}",
                fe.jacobians(m, ty, G).unwrap().as_slice()
            )
        }
    }
}

const SENTINEL: [Real; 3] = [7.25, -3.5, 11.0];

fn sentinel() -> Array<Real> {
    Array::from_vec(SENTINEL.to_vec(), 3).unwrap()
}

fn untouched(out: &Array<Real>) -> bool {
    out.nb_component() == 3 && out.as_slice() == SENTINEL
}

// ---------------------------------------------------------------------------
// `fs01_*` (API that existed before the fix)
// ---------------------------------------------------------------------------

/// Zero-length beams, a normal parallel to the axis, collinear or repeated
/// DKT nodes, underflowed geometry, non-finite inputs, a zero normal and an
/// underflowed binormal: refused at precomputation with `Error::Numerical`
/// naming the element.
#[test]
fn fs01_degenerate_geometry_is_refused() {
    let z3 = Some(vec![vec![0., 0., 1.]]);
    let cases: Vec<(&str, usize, E, Nodes, Option<Nodes>)> = vec![
        (
            "zero beam2",
            2,
            E::BernoulliBeam2,
            nodes2(&[[1., 1.], [1., 1.]]),
            None,
        ),
        (
            "NaN beam2",
            2,
            E::BernoulliBeam2,
            nodes2(&[[0., 0.], [Real::NAN, 1.]]),
            None,
        ),
        (
            "inf beam2",
            2,
            E::BernoulliBeam2,
            nodes2(&[[0., 0.], [Real::INFINITY, 0.]]),
            None,
        ),
        (
            "underflowed beam2",
            2,
            E::BernoulliBeam2,
            nodes2(&[[0., 0.], [1e-160, 0.]]),
            None,
        ),
        (
            "zero beam3",
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [0.; 3]]),
            z3.clone(),
        ),
        (
            "parallel normal",
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [1., 0., 0.]]),
            Some(vec![vec![1., 0., 0.]]),
        ),
        (
            "zero normal",
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [1., 0., 0.]]),
            Some(vec![vec![0., 0., 0.]]),
        ),
        (
            "underflowed binormal",
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [1., 0., 0.]]),
            Some(vec![vec![1., 1e-160, 0.]]),
        ),
        (
            "NaN normal",
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [1., 0., 0.]]),
            Some(vec![vec![0., Real::NAN, 1.]]),
        ),
        (
            "NaN beam3",
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [1., Real::NAN, 0.]]),
            z3.clone(),
        ),
        (
            "collinear dkt",
            3,
            E::DiscreteKirchhoffTriangle18,
            nodes3(&[[0.; 3], [1., 0., 0.], [2., 0., 0.]]),
            None,
        ),
        (
            "underflow dkt",
            3,
            E::DiscreteKirchhoffTriangle18,
            nodes3(&[[0.; 3], [1., 0., 0.], [2., 1e-200, 0.]]),
            None,
        ),
        (
            "repeated dkt node",
            3,
            E::DiscreteKirchhoffTriangle18,
            nodes3(&[[0.; 3], [1., 0., 0.], [0.; 3]]),
            None,
        ),
        (
            "zero first dkt edge",
            3,
            E::DiscreteKirchhoffTriangle18,
            nodes3(&[[0.; 3], [0.; 3], [0., 1., 0.]]),
            None,
        ),
        (
            "inf dkt",
            3,
            E::DiscreteKirchhoffTriangle18,
            nodes3(&[[0.; 3], [1., 0., 0.], [0., Real::INFINITY, 0.]]),
            None,
        ),
    ];
    for (case, dim, ty, nodes, normals) in cases {
        let conn: Vec<Vec<Idx>> = vec![(0..ty.nb_nodes_per_element() as Idx).collect()];
        let m = mesh(dim, ty, &nodes, &conn, normals.as_deref());
        let msg = refused(&m, case);
        assert!(msg.contains("element 0"), "{case}: {msg}");
        assert!(msg.contains(ty.name()), "{case}: {msg}");
    }
}

/// Thin but finite triangles and short beams are accepted (no conditioning
/// policy): the cache is finite with positive Jacobians.
#[test]
fn fs01_thin_finite_elements_are_accepted() {
    let cases: Vec<Case> = vec![
        (
            3,
            E::DiscreteKirchhoffTriangle18,
            nodes3(&[[0.; 3], [1., 0., 0.], [2., 1e-12, 0.]]),
            None,
        ),
        (
            2,
            E::BernoulliBeam2,
            nodes2(&[[0., 0.], [1e-100, 0.]]),
            None,
        ),
        (
            3,
            E::BernoulliBeam3,
            nodes3(&[[0.; 3], [0., 1e-100, 0.]]),
            Some(vec![vec![0., 0., 1.]]),
        ),
    ];
    for (dim, ty, nodes, normals) in cases {
        let conn: Vec<Vec<Idx>> = vec![(0..ty.nb_nodes_per_element() as Idx).collect()];
        let m = mesh(dim, ty, &nodes, &conn, normals.as_deref());
        let fe = engine(&m);
        let j = fe.jacobians(&m, ty, G).unwrap().as_slice();
        assert!(j.iter().all(|v| v.is_finite() && *v > 0.), "{ty:?}: {j:?}");
        for a in [
            fe.rotations(&m, ty, G),
            fe.shapes(&m, ty, G),
            fe.shapes_derivatives(&m, ty, G),
        ] {
            assert!(
                a.unwrap().as_slice().iter().all(|v| v.is_finite()),
                "{ty:?}"
            );
        }
    }
}

/// The fallible public element kernels refuse degenerate geometry instead
/// of returning `inf`/`NaN`.
#[test]
fn fs01_public_kernels_refuse_degenerate_geometry() {
    let numerical = |r: akantu_core::Result<()>, case: &str| {
        assert!(matches!(r, Err(Error::Numerical(_))), "{case}: {r:?}");
    };
    let zero2 = [1., 1., 1., 1.];
    let x = Coords::new(&zero2, 2, 2).unwrap();
    let mut r9 = [0.; 9];
    numerical(
        BernoulliBeam2::compute_rotation_matrix(x, &[], &mut r9),
        "beam2 rotation",
    );
    let mut n18 = [0.; 18];
    numerical(
        BernoulliBeam2::compute_shapes(&[0.2], x, &mut n18),
        "beam2 shapes",
    );
    let mut d6 = [0.; 6];
    numerical(
        BernoulliBeam2::compute_dnds(&[0.2], x, &mut d6),
        "beam2 dnds",
    );
    assert!(matches!(hermite_shapes(0.2, x), Err(Error::Numerical(_))));
    assert!(matches!(hermite_dnds(0.2, x), Err(Error::Numerical(_))));

    let beam = [0., 0., 0., 1., 0., 0.];
    let xb = Coords::new(&beam, 3, 2).unwrap();
    let mut r36 = [0.; 36];
    numerical(
        BernoulliBeam3::compute_rotation_matrix(xb, &[2., 0., 0.], &mut r36),
        "beam3 parallel normal",
    );
    numerical(
        BernoulliBeam3::compute_rotation_matrix(xb, &[0., 0., 0.], &mut r36),
        "beam3 zero normal",
    );
    numerical(
        BernoulliBeam3::compute_rotation_matrix(xb, &[0., Real::INFINITY, 0.], &mut r36),
        "beam3 inf normal",
    );

    let tri = [0., 0., 0., 1., 0., 0., 2., 0., 0.];
    let xt = Coords::new(&tri, 3, 3).unwrap();
    assert!(matches!(dkt_basis_change(xt), Err(Error::Numerical(_))));
    numerical(
        DiscreteKirchhoffTriangle18::compute_rotation_matrix(xt, &[], &mut r36),
        "dkt rotation",
    );
    let mut d42 = [0.; 42];
    numerical(
        DiscreteKirchhoffTriangle18::compute_dnds(&[0.2, 0.2], xt, &mut d42),
        "dkt dnds",
    );
}

// ---------------------------------------------------------------------------
// `fs02_*` (API before the fix; default engine = corrected θy row)
// ---------------------------------------------------------------------------

fn unit_x_beam3() -> Mesh {
    mesh(
        3,
        E::BernoulliBeam3,
        &nodes3(&[[0.; 3], [1., 0., 0.]]),
        &[vec![0, 1]],
        Some(&[vec![0., 0., 1.]]),
    )
}

/// Rigid rotation about y (`w = −x`, `θy = 1`) on a unit x-axis beam with
/// normal z (`R = I`): the interpolated θy is +1 at every point.
#[test]
fn fs02_rigid_rotation_interpolates_theta_y() {
    let m = unit_x_beam3();
    let fe = engine(&m);
    let u = Array::from_vec(vec![0., 0., 0., 0., 1., 0., 0., 0., -1., 0., 1., 0.], 6).unwrap();
    let mut out = Array::new(1);
    fe.interpolate_on_integration_points(&m, u.view(), &mut out, 6, E::BernoulliBeam3, G, None)
        .unwrap();
    for v in out.as_slice().chunks(6) {
        assert!((v[4] - 1.).abs() <= 1e-15, "θy {v:?}");
        // w = −x at the point, the other components vanish
        assert_eq!([v[0], v[1], v[3], v[5]], [0.; 4], "{v:?}");
    }
}

/// A constant local unit θy moment density on a unit beam does virtual work
/// `∫ m θy ds = 1` on the rigid rotation above.
#[test]
fn fs02_unit_moment_line_load_virtual_work() {
    let m = unit_x_beam3();
    let fe = engine(&m);
    let ty = E::BernoulliBeam3;
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let mut tr = vec![0.; nq * 6];
    for q in 0..nq {
        tr[q * 6 + 4] = 1.;
    }
    let tr = Array::from_vec(tr, 6).unwrap();
    let mut f = Array::new(1);
    fe.line_load_local(&m, tr.view(), &mut f, ty, G).unwrap();
    let u = [0., 0., 0., 0., 1., 0., 0., 0., -1., 0., 1., 0.];
    let work: Real = f.as_slice().iter().zip(u).map(|(a, b)| a * b).sum();
    assert!((work - 1.).abs() <= 1e-15, "virtual work {work}");
}

/// The θy row interpolates its own nodal values: at ξ = −1 (+1) the
/// coefficient of θy₁ (θy₂) is exactly 1 and every other θy-row
/// coefficient is 0.
#[test]
fn fs02_theta_y_row_is_nodal() {
    let x = [0.3, -0.2, 0.5, 1.9, 0.4, -0.7];
    let xc = Coords::new(&x, 3, 2).unwrap();
    for (xi, node_col) in [(-1., 4), (1., 10)] {
        let mut n = [0.; 72];
        BernoulliBeam3::compute_shapes(&[xi], xc, &mut n).unwrap();
        for c in 0..12 {
            let want = if c == node_col { 1. } else { 0. };
            assert_eq!(n[4 + 6 * c], want, "ξ = {xi}, column {c}");
        }
    }
}

// ---------------------------------------------------------------------------
// `fs03_*` (API before the fix: refusals)
// ---------------------------------------------------------------------------

fn unit_x_beam2() -> Mesh {
    mesh(
        2,
        E::BernoulliBeam2,
        &nodes2(&[[0., 0.], [1., 0.]]),
        &[vec![0, 1]],
        None,
    )
}

/// Same element / node counts, moved node: every operation that reads the
/// mesh is refused before touching its output (before the fix the engine
/// returned strains `[2, 0, 2, 0]` instead of `[1, 0, 1, 0]`).
#[test]
fn fs03_moved_nodes_are_refused_before_output() {
    let ty = E::BernoulliBeam2;
    let mut m = unit_x_beam2();
    let fe = engine(&m);
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[2] = 2.;
    let u = Array::from_vec(vec![0., 0., 0., 2., 0., 0.], 3).unwrap();
    let mut out = sentinel();
    assert!(
        fe.gradient_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
            .is_err()
    );
    assert!(untouched(&out));
    assert!(
        fe.interpolate_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
            .is_err()
    );
    assert!(untouched(&out));
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let d = Array::from_vec([1., 0., 0., 1.].repeat(nq), 4).unwrap();
    assert!(
        fe.compute_stresses(&m, d.view(), u.view(), &mut out, ty, G)
            .is_err()
    );
    assert!(untouched(&out));
    assert!(fe.mass_shapes(&m, &mut out, ty, G).is_err());
    assert!(untouched(&out));
}

/// Rotating the beam to a length-two y-axis beam (same counts): the
/// consistent mass is refused (before the fix it was all zero, from the
/// cached x-axis rotations).
#[test]
fn fs03_rotated_mesh_consistent_mass_is_refused() {
    let ty = E::BernoulliBeam2;
    let mut m = unit_x_beam2();
    let fe = engine(&m);
    MeshAccessor::new(&mut m)
        .nodes_mut()
        .as_mut_slice()
        .copy_from_slice(&[0., 0., 0., 2.]);
    let nq = akantu_fe::gauss_rule(ty, fe.integration_orders().mass_order(ty))
        .unwrap()
        .nb_points();
    let rho = Array::from_vec(vec![1.; nq * 2], 2).unwrap();
    let mut out = sentinel();
    assert!(fe.consistent_mass(&m, rho.view(), &mut out, ty, G).is_err());
    assert!(untouched(&out));
}

/// Same-count connectivity change: refused.
#[test]
fn fs03_changed_connectivity_is_refused() {
    let ty = E::BernoulliBeam2;
    let mut m = mesh(
        2,
        ty,
        &nodes2(&[[0., 0.], [1., 0.], [1., 3.]]),
        &[vec![0, 1]],
        None,
    );
    let fe = engine(&m);
    MeshAccessor::new(&mut m)
        .connectivity_mut(ty, G)
        .as_mut_slice()[1] = 2;
    let u = Array::from_vec(vec![0.; 9], 3).unwrap();
    let mut out = sentinel();
    assert!(
        fe.gradient_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
            .is_err()
    );
    assert!(untouched(&out));
}

fn set_normals(m: &mut Mesh, ty: E, ghost: GhostType, n: Vec<Real>) {
    m.data_mut()
        .reals_mut_or_insert(EXTRA_NORMAL)
        .unwrap()
        .as_map_mut()
        .replace(ty, ghost, Array::from_vec(n, 3).unwrap().into());
}

/// A changed `extra_normal` (mesh data: the mesh revision does not move)
/// makes the 3-D beam frames stale: refused.
#[test]
fn fs03_changed_extra_normal_is_refused() {
    let ty = E::BernoulliBeam3;
    let mut m = unit_x_beam3();
    let fe = engine(&m);
    let rev = m.revision();
    set_normals(&mut m, ty, G, vec![0., 1., 0.]);
    assert_eq!(m.revision(), rev, "mesh data do not advance the revision");
    let u = Array::from_vec(vec![0.; 12], 6).unwrap();
    let mut out = sentinel();
    assert!(
        fe.gradient_on_integration_points(&m, u.view(), &mut out, 6, ty, G, None)
            .is_err()
    );
    assert!(untouched(&out));
    assert!(
        fe.interpolate_on_integration_points(&m, u.view(), &mut out, 6, ty, G, None)
            .is_err()
    );
    assert!(untouched(&out));
    let nq = akantu_fe::gauss_rule(ty, fe.integration_orders().mass_order(ty))
        .unwrap()
        .nb_points();
    let rho = Array::from_vec(vec![1.; nq * 4], 4).unwrap();
    assert!(fe.consistent_mass(&m, rho.view(), &mut out, ty, G).is_err());
    assert!(untouched(&out));
    // removing the data is a change as well
    m.data_mut().remove_elemental(EXTRA_NORMAL);
    assert!(
        fe.gradient_on_integration_points(&m, u.view(), &mut out, 6, ty, G, None)
            .is_err()
    );
}

/// Beam-2 mesh with one not-ghost and one ghost element.
fn beam2_with_ghost() -> Mesh {
    let ty = E::BernoulliBeam2;
    let mut m = Mesh::new(2, "v55_ghost").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for x in [[0., 0.], [1., 0.], [1., 2.]] {
            acc.push_node(&x).unwrap();
        }
        acc.connectivity_mut(ty, G).push_tuple(&[0, 1]);
        acc.connectivity_mut(ty, GhostType::Ghost)
            .push_tuple(&[1, 2]);
    }
    m
}

/// Re-initialising the not-ghost blocks after a node move must not bless
/// the ghost blocks as fresh.
#[test]
fn fs03_reinitialising_one_ghost_type_does_not_bless_the_other() {
    let ty = E::BernoulliBeam2;
    let mut m = beam2_with_ghost();
    let mut fe = engine(&m);
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[5] = 3.;
    fe.init_shape_functions(&m, G).unwrap();
    let u = Array::from_vec(vec![0.1, 0.2, 0.3, 0.4, 0.5, 0.6, 0.7, 0.8, 0.9], 3).unwrap();
    let mut out = sentinel();
    assert!(
        fe.gradient_on_integration_points(&m, u.view(), &mut out, 3, ty, GhostType::Ghost, None)
            .is_err()
    );
    assert!(untouched(&out));
    let mut fresh = Array::new(1);
    engine(&m)
        .gradient_on_integration_points(&m, u.view(), &mut fresh, 3, ty, G, None)
        .unwrap();
    fe.gradient_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
        .unwrap();
    assert_eq!(bits(out.as_slice()), bits(fresh.as_slice()));
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

// ---------------------------------------------------------------------------
// `fs02_*`: legacy switch (new API)
// ---------------------------------------------------------------------------

/// The legacy engine reproduces upstream's `+dw/ds` row: on the rigid
/// rotation it interpolates the exact negation of the corrected θy
/// (measured `[-0.9999999999999998, -0.9999999999999999]`), every
/// other component bit-identical; the moment load's work is `−1`.
#[test]
fn fs02_legacy_switch_reproduces_upstream_theta_y() {
    use akantu_fe::structural::StructuralUpstreamCompat;
    let ty = E::BernoulliBeam3;
    let m = unit_x_beam3();
    let corrected = engine(&m);
    let legacy = StructuralFeEngine::with_precomputation_compat(
        &m,
        None,
        "legacy",
        StructuralUpstreamCompat::UPSTREAM,
    )
    .unwrap();
    assert!(legacy.upstream_compat().beam3_theta_y_legacy);
    assert!(!corrected.upstream_compat().beam3_theta_y_legacy);
    let u = Array::from_vec(vec![0., 0., 0., 0., 1., 0., 0., 0., -1., 0., 1., 0.], 6).unwrap();
    let (mut a, mut b) = (Array::new(1), Array::new(1));
    corrected
        .interpolate_on_integration_points(&m, u.view(), &mut a, 6, ty, G, None)
        .unwrap();
    legacy
        .interpolate_on_integration_points(&m, u.view(), &mut b, 6, ty, G, None)
        .unwrap();
    assert_eq!(
        b.as_slice().chunks(6).map(|v| v[4]).collect::<Vec<_>>(),
        [-0.9999999999999998, -0.9999999999999999]
    );
    for (x, y) in a.as_slice().chunks(6).zip(b.as_slice().chunks(6)) {
        assert_eq!(x[4].to_bits(), (-y[4]).to_bits());
        for c in [0, 1, 2, 3, 5] {
            assert_eq!(x[c].to_bits(), y[c].to_bits());
        }
    }
    // element level: legacy row = exact negation of the corrected row
    let xs = [0.3, -0.2, 0.5, 1.9, 0.4, -0.7];
    let xc = Coords::new(&xs, 3, 2).unwrap();
    for xi in [-1., -0.4, 0.1, 0.77, 1.] {
        let (mut nc, mut nl) = ([0.; 72], [0.; 72]);
        BernoulliBeam3::compute_shapes_with(
            &[xi],
            xc,
            StructuralUpstreamCompat::default(),
            &mut nc,
        )
        .unwrap();
        BernoulliBeam3::compute_shapes_with(&[xi], xc, StructuralUpstreamCompat::UPSTREAM, &mut nl)
            .unwrap();
        for i in 0..72 {
            // the four set coefficients of the θy row (columns w₁, θy₁, w₂, θy₂)
            let flipped = i % 6 == 4 && [2, 4, 8, 10].contains(&(i / 6));
            let want = if flipped { -nl[i] } else { nl[i] };
            assert_eq!(nc[i].to_bits(), want.to_bits(), "ξ = {xi}, entry {i}");
        }
    }
    // consistent mass uses the first nb_stress rows only: identical
    let nq = akantu_fe::gauss_rule(ty, corrected.integration_orders().mass_order(ty))
        .unwrap()
        .nb_points();
    let rho = Array::from_vec(vec![2.5; nq * 4], 4).unwrap();
    corrected
        .consistent_mass(&m, rho.view(), &mut a, ty, G)
        .unwrap();
    legacy
        .consistent_mass(&m, rho.view(), &mut b, ty, G)
        .unwrap();
    assert_eq!(bits(a.as_slice()), bits(b.as_slice()));
}

// ---------------------------------------------------------------------------
// `fs03_*`: refresh, events, vanished blocks, transactional rebuild (new API)
// ---------------------------------------------------------------------------

/// Every precomputed array of `a` and `b` bit-identical for `(ty, ghost)`.
fn same_block(m: &Mesh, a: &StructuralFeEngine, b: &StructuralFeEngine, ty: E, ghost: GhostType) {
    assert_eq!(
        bits(a.rotations(m, ty, ghost).unwrap().as_slice()),
        bits(b.rotations(m, ty, ghost).unwrap().as_slice())
    );
    assert_eq!(
        bits(a.jacobians(m, ty, ghost).unwrap().as_slice()),
        bits(b.jacobians(m, ty, ghost).unwrap().as_slice())
    );
    assert_eq!(
        bits(a.shapes(m, ty, ghost).unwrap().as_slice()),
        bits(b.shapes(m, ty, ghost).unwrap().as_slice())
    );
    assert_eq!(
        bits(a.shapes_derivatives(m, ty, ghost).unwrap().as_slice()),
        bits(b.shapes_derivatives(m, ty, ghost).unwrap().as_slice())
    );
}

/// `refresh` after moved nodes / changed normals: the engine equals a
/// freshly built one (bitwise) and the operations work again (strains
/// `[1, 0, 1, 0]`).
#[test]
fn fs03_refresh_rebuilds_in_full() {
    let ty = E::BernoulliBeam2;
    let mut m = unit_x_beam2();
    let mut fe = engine(&m);
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[2] = 2.;
    assert!(fe.check_up_to_date(&m, ty, G).is_err());
    fe.refresh(&m).unwrap();
    fe.check_up_to_date(&m, ty, G).unwrap();
    assert_eq!(fe.mesh_revision(), m.revision());
    same_block(&m, &fe, &engine(&m), ty, G);
    let u = Array::from_vec(vec![0., 0., 0., 2., 0., 0.], 3).unwrap();
    let mut out = Array::new(1);
    fe.gradient_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
        .unwrap();
    assert_eq!(out.as_slice(), [1., 0., 1., 0.]);

    let ty = E::BernoulliBeam3;
    let mut m = unit_x_beam3();
    let mut fe = engine(&m);
    set_normals(&mut m, ty, G, vec![0., 1., 0.]);
    assert!(fe.check_up_to_date(&m, ty, G).is_err());
    fe.refresh(&m).unwrap();
    same_block(&m, &fe, &engine(&m), ty, G);
    fe.check_up_to_date(&m, ty, G).unwrap();
}

/// Mesh events rebuild every initialised block in full (elements added to
/// one type also refresh the other ghost type, whose shared node moved),
/// matching a freshly built engine bitwise.
#[test]
fn fs03_mesh_events_rebuild_every_block() {
    use akantu_core::Element;
    use akantu_mesh::{MeshEvent, MeshEventHandler};
    let ty = E::BernoulliBeam2;
    let mut m = beam2_with_ghost();
    let mut fe = engine(&m);
    {
        let mut acc = MeshAccessor::new(&mut m);
        acc.nodes_mut().as_mut_slice()[3] = 0.5; // shared node 1 moves
        let n = acc.push_node(&[2., 2.]).unwrap();
        acc.connectivity_mut(ty, G).push_tuple(&[2, n]);
    }
    let event = MeshEvent::ElementsAdded {
        elements: vec![Element::new(ty, 1, G)],
        constitutive_law: None,
        origin: "v55".into(),
        extension: None,
    };
    fe.on_mesh_event(&m, &event).unwrap();
    let fresh = engine(&m);
    for ghost in [G, GhostType::Ghost] {
        fe.check_up_to_date(&m, ty, ghost).unwrap();
        same_block(&m, &fe, &fresh, ty, ghost);
    }
    assert_eq!(fe.rotations(&m, ty, G).unwrap().len(), 2);
}

/// A rebuild drops the blocks whose type vanished (here: a distributed
/// mesh without DKT elements); an engine that never initialised a ghost
/// type does not create it on refresh.
#[test]
fn fs03_vanished_blocks_are_removed() {
    use akantu_mesh::{MeshEvent, MeshEventHandler};
    let mut m = unit_x_beam3();
    {
        let mut acc = MeshAccessor::new(&mut m);
        acc.push_node(&[0., 1., 0.]).unwrap();
        acc.connectivity_mut(E::DiscreteKirchhoffTriangle18, G)
            .push_tuple(&[0, 1, 2]);
    }
    let mut fe = StructuralFeEngine::new(&m, None, "v55").unwrap();
    fe.init_shape_functions(&m, G).unwrap();
    assert!(fe.shapes(&m, E::DiscreteKirchhoffTriangle18, G).is_ok());
    let beams_only = unit_x_beam3();
    let event = MeshEvent::MeshIsDistributed {
        origin: "v55".into(),
    };
    fe.on_mesh_event(&beams_only, &event).unwrap();
    let ty = E::DiscreteKirchhoffTriangle18;
    assert!(fe.shapes(&beams_only, ty, G).is_err());
    assert!(fe.rotations(&beams_only, ty, G).is_err());
    assert!(fe.jacobians(&beams_only, ty, G).is_err());
    assert!(fe.shapes_derivatives(&beams_only, ty, G).is_err());
    assert!(fe.quadrature_rule(ty, G).is_err());
    assert!(fe.check_up_to_date(&beams_only, ty, G).is_err());
    fe.check_up_to_date(&beams_only, E::BernoulliBeam3, G)
        .unwrap();
    // the ghost type was never initialised: still absent
    assert!(
        fe.rotations(&beams_only, E::BernoulliBeam3, GhostType::Ghost)
            .is_err()
    );
}

/// A failed `init_shape_functions` changes nothing: the blocks keep their
/// data and their old stamps, so they are refused as stale rather than
/// blessed (no partial commit). A failed `refresh` (event rebuild) drops
/// every block of the failing ghost type instead (see
/// `review_v60_regressions.rs`).
#[test]
fn fs03_failed_rebuild_is_transactional() {
    let mut m = unit_x_beam3();
    {
        let mut acc = MeshAccessor::new(&mut m);
        acc.push_node(&[0., 1., 0.]).unwrap();
        acc.connectivity_mut(E::DiscreteKirchhoffTriangle18, G)
            .push_tuple(&[0, 1, 2]);
    }
    let mut fe = engine(&m);
    let rev = m.revision();
    // collapse the triangle (node 2 onto node 0): the beam is unaffected
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[6..9].copy_from_slice(&[0., 0., 0.]);
    assert!(matches!(
        fe.init_shape_functions(&m, G),
        Err(Error::Numerical(_))
    ));
    // nothing committed: the blocks keep their old stamps (refused as built
    // at the old revision, not "missing", not blessed)
    assert_eq!(fe.mesh_revision(), rev);
    for ty in [E::BernoulliBeam3, E::DiscreteKirchhoffTriangle18] {
        let msg = format!("{:?}", fe.check_up_to_date(&m, ty, G).unwrap_err());
        assert!(
            msg.contains(&format!("precomputed at mesh revision {rev}")),
            "{msg}"
        );
        assert!(fe.rotations(&m, ty, G).is_err());
    }
    // the event rebuild drops the failing ghost type
    assert!(matches!(fe.refresh(&m), Err(Error::Numerical(_))));
    for ty in [E::BernoulliBeam3, E::DiscreteKirchhoffTriangle18] {
        let msg = format!("{:?}", fe.check_up_to_date(&m, ty, G).unwrap_err());
        assert!(msg.contains("no precomputed data"), "{msg}");
        assert!(fe.rotations(&m, ty, G).is_err());
    }
}

// ---------------------------------------------------------------------------
// `fs04_*`: public element API (new API)
// ---------------------------------------------------------------------------

#[test]
fn fs04_coords_invariants_and_accessors() {
    let v = [1., 2., 3., 4., 5.];
    let c = Coords::new(&v, 2, 2).unwrap();
    assert_eq!((c.rows(), c.cols(), c.data()), (2, 2, &v[..4]));
    assert!(matches!(
        Coords::new(&v, 3, 2),
        Err(Error::DimensionMismatch { .. })
    ));
    assert!(matches!(
        Coords::new(&v, usize::MAX, 2),
        Err(Error::DimensionMismatch { .. })
    ));
}

/// Beam-2 rotation requires exactly two coordinate rows (before the fix,
/// 3-D nodes gave a non-orthogonal `diag(1/√2, 1/√2, 1)`).
#[test]
fn fs04_beam2_rotation_requires_two_rows() {
    let x = [0., 0., 0., 1., 0., 1.];
    let mut r = [0.; 9];
    assert!(matches!(
        BernoulliBeam2::compute_rotation_matrix(Coords::new(&x, 3, 2).unwrap(), &[], &mut r),
        Err(Error::DimensionMismatch { .. })
    ));
    assert_eq!(r, [0.; 9], "nothing written");
    let x1 = [0., 1.];
    assert!(
        BernoulliBeam2::compute_rotation_matrix(Coords::new(&x1, 1, 2).unwrap(), &[], &mut r)
            .is_err()
    );
}

/// Short buffers and wrong dimensions through the exported APIs: errors,
/// never panics.
#[test]
fn fs04_short_buffers_are_errors() {
    fn voigt<T: StructuralElement>() {
        let (nd, nc) = (
            T::NATURAL_DIM * T::DNDS_COLUMNS,
            T::NB_STRESS * T::NB_ELEMENT_DOFS,
        );
        let dn = vec![0.5; nd];
        let mut b = vec![0.; nc];
        assert!(T::arrange_in_voigt(&dn, &mut b).is_ok());
        assert!(matches!(
            T::arrange_in_voigt(&dn[..nd - 1], &mut b),
            Err(Error::DimensionMismatch { .. })
        ));
        assert!(matches!(
            T::arrange_in_voigt(&[], &mut b),
            Err(Error::DimensionMismatch { .. })
        ));
        assert!(matches!(
            T::arrange_in_voigt(&dn, &mut b[..nc - 1]),
            Err(Error::DimensionMismatch { .. })
        ));
        assert!(matches!(
            T::arrange_in_voigt(&dn, &mut []),
            Err(Error::DimensionMismatch { .. })
        ));
    }
    voigt::<BernoulliBeam2>();
    voigt::<BernoulliBeam3>();
    voigt::<DiscreteKirchhoffTriangle18>();

    let beam = [0., 0., 0., 1., 2., 3.];
    let xb = Coords::new(&beam, 3, 2).unwrap();
    let mut r36 = [0.; 36];
    for n in [&[0., 0.][..], &[0., 0., 1., 0.][..], &[][..]] {
        assert!(matches!(
            BernoulliBeam3::compute_rotation_matrix(xb, n, &mut r36),
            Err(Error::DimensionMismatch { .. })
        ));
    }
    assert!(BernoulliBeam3::compute_rotation_matrix(xb, &[0., 0., 1.], &mut r36[..35]).is_err());
    let mut n71 = [0.; 71];
    assert!(BernoulliBeam3::compute_shapes(&[0.1], xb, &mut n71).is_err());
    assert!(BernoulliBeam3::compute_shapes(&[], xb, &mut [0.; 72]).is_err());
    assert!(BernoulliBeam3::compute_dnds(&[0.1], xb, &mut [0.; 5]).is_err());
    let tri = [0., 0., 0., 1., 0., 0., 0., 1., 0.];
    let xt2 = Coords::new(&tri, 3, 2).unwrap();
    assert!(dkt_basis_change(xt2).is_err());
    assert!(
        DiscreteKirchhoffTriangle18::compute_dnds(
            &[0.2],
            Coords::new(&tri, 3, 3).unwrap(),
            &mut [0.; 42]
        )
        .is_err()
    );
}

/// `beam_lumped_mass_contributions` requires two coordinate vectors of the
/// same length 2 or 3 (it silently truncated with `min` before).
#[test]
fn fs04_lumped_mass_coordinates_contract() {
    use akantu_fe::structural::kernels::beam_lumped_mass_contributions as lumped;
    assert_eq!(lumped(&[0., 0.], &[3., 4.], 2., 0.5).unwrap(), (2.5, 1.25));
    for (a, b) in [
        (&[0., 0.][..], &[3., 4., 12.][..]),
        (&[0., 0., 0.][..], &[3., 4.][..]),
        (&[0.][..], &[1.][..]),
        (&[0.; 4][..], &[1.; 4][..]),
        (&[][..], &[][..]),
    ] {
        assert!(
            matches!(lumped(a, b, 1., 1.), Err(Error::DimensionMismatch { .. })),
            "{a:?} {b:?}"
        );
    }
    assert!(matches!(
        lumped(&[0., Real::NAN], &[1., 1.], 1., 1.),
        Err(Error::Numerical(_))
    ));
    assert!(matches!(
        lumped(&[0., 0.], &[1., 1.], Real::INFINITY, 1.),
        Err(Error::Numerical(_))
    ));
}
