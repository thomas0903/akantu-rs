// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): regression tests
// on the structural FE kernels ported from src/fe_engine/
// element_classes/element_class_{hermite,bernoulli_beam,kirchhoff_shell}_inline_impl.hh,
// shape_structural*, integrator_gauss_inline_impl.hh. Test-only code.

//!
//! * **Finite caches:** a well-shaped DKT triangle of size `1.6e-154` passes every
//!   squared-length criterion but its `B·T` overflows to `NaN`: only the
//!   finite-cache guard refuses it (UD-296);
//! * **Nodal field lengths:** interpolation, gradient and stresses refuse a nodal field
//!   whose length is not exactly the number of mesh nodes (too short **and**
//!   too long) before touching their output, as `FeEngine` does;
//! * **Stale data:** no operation serves data the engine knows to be stale: a
//!   failed `refresh` / event rebuild drops the blocks of the failing ghost
//!   type ("not initialised" afterwards), and every operation that reads a
//!   precomputed block takes the mesh and checks the block first;
//! * **Beam normals:** (UD-343, D-003 A with a legacy switch) a 3-D beam normal
//!   that is clearly not a unit vector orthogonal to the beam axis
//!   (`|n·n − 1| > 1e-10` or `|n·t̂| > 1e-10`, input validation) is refused
//!   by default with an error naming the element; normals inside the
//!   criterion are used raw (bit-identical to the unchecked path);
//!   `StructuralUpstreamCompat::beam3_normal_unchecked` (part of `UPSTREAM`)
//!   reproduces upstream's frames for any finite normal;
//! * **Overflow:** overflowing squared lengths are reported as "too large"
//!   (not "degenerate"); `FeEngine` points structural users to
//!   `StructuralFeEngine`;
//!
//! Tolerances: exact (`==` / bit patterns) unless stated.

mod structural_common;

use akantu_core::{Array, ElementType as E, Error, GhostType, Real};
use akantu_fe::element_class::types::BernoulliBeam3;
use akantu_fe::structural::{
    Coords, EXTRA_NORMAL, StructuralElement, StructuralFeEngine, StructuralUpstreamCompat,
};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler};
use structural_common::{G, mesh};

// ---------------------------------------------------------------------------
// Finite caches: the finite-cache guard is the only barrier for a reachable input
// ---------------------------------------------------------------------------

/// The perfect DKT triangle `(0,0,0), (s,0,0), (0,s,0)` with
/// `s = 1.6e-154` has positive normal squared edges (`s² ≈ 2.6e-308`) and a
/// positive finite `det J`, but `B·T` holds `NaN` (upstream publishes 36 of
/// 324 non-finite entries, oracle case `tiny_dkt`). The precomputation must
/// refuse it through the finite-cache guard (`Error::Numerical` naming the
/// `B·T` entry and the element) in both compatibility modes, and store
/// nothing.
#[test]
fn fs2_01_tiny_dkt_is_refused_by_the_finite_cache_guard() {
    let ty = E::DiscreteKirchhoffTriangle18;
    let s = 1.6e-154;
    let m = mesh(
        3,
        ty,
        &[vec![0., 0., 0.], vec![s, 0., 0.], vec![0., s, 0.]],
        &[vec![0, 1, 2]],
        None,
    );
    for compat in [
        StructuralUpstreamCompat::default(),
        StructuralUpstreamCompat::UPSTREAM,
    ] {
        let mut fe = StructuralFeEngine::new_with_compat(&m, None, "v60", compat).unwrap();
        match fe.init_shape_functions(&m, G) {
            Err(Error::Numerical(msg)) => {
                assert!(msg.contains("non-finite B·T entry"), "{compat:?}: {msg}");
                assert!(
                    msg.contains("element 0 of _discrete_kirchhoff_triangle_18:_not_ghost"),
                    "{compat:?}: {msg}"
                );
            }
            other => panic!("{compat:?}: expected Error::Numerical, got {other:?}"),
        }
        // nothing stored
        assert!(fe.check_up_to_date(&m, ty, G).is_err(), "{compat:?}");
        assert!(fe.quadrature_rule(ty, G).is_err(), "{compat:?}");
        assert!(fe.check_up_to_date(&m, ty, GhostType::Ghost).is_err());
        assert!(
            StructuralFeEngine::with_precomputation_compat(&m, None, "v60", compat).is_err(),
            "{compat:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// Nodal field lengths: nodal field length checked before the output is touched
// ---------------------------------------------------------------------------

const SENTINEL: [Real; 3] = [7.25, -3.5, 11.0];

fn sentinel() -> Array<Real> {
    Array::from_vec(SENTINEL.to_vec(), 3).unwrap()
}

fn untouched(out: &Array<Real>) -> bool {
    out.nb_component() == 3 && out.as_slice() == SENTINEL
}

/// On a 3-node beam-2 mesh, a nodal field with 2 or 4 nodes
/// is refused with `Error::DimensionMismatch` by interpolation, gradient
/// and stresses, and the output keeps its sentinel (before the fix the
/// short field resized and partially wrote the output, then failed with
/// `OutOfRange`; the long field was accepted). The exact length passes.
#[test]
fn fs2_02_nodal_field_length_is_checked_before_the_output() {
    let ty = E::BernoulliBeam2;
    let m = mesh(
        2,
        ty,
        &[vec![0., 0.], vec![1., 0.], vec![2., 0.]],
        &[vec![0, 1], vec![1, 2]],
        None,
    );
    let fe = structural_common::engine(&m);
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let d = Array::from_vec([1., 0., 0., 1.].repeat(2 * nq), 4).unwrap();
    for nodes in [0usize, 2, 4, 3] {
        let u = Array::from_vec(vec![0.5; nodes * 3], 3).unwrap();
        for op in ["gradient", "interpolate", "stresses"] {
            let mut out = sentinel();
            let r = match op {
                "gradient" => {
                    fe.gradient_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
                }
                "interpolate" => {
                    fe.interpolate_on_integration_points(&m, u.view(), &mut out, 3, ty, G, None)
                }
                _ => fe.compute_stresses(&m, d.view(), u.view(), &mut out, ty, G),
            };
            if nodes == 3 {
                r.unwrap();
                assert_eq!(out.len(), 2 * nq, "{op}");
            } else {
                assert!(
                    matches!(r, Err(Error::DimensionMismatch { expected: 3, got, .. }) if got == nodes),
                    "{op} with {nodes} nodes: {r:?}"
                );
                assert!(untouched(&out), "{op} with {nodes} nodes: output changed");
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Stale data: failed rebuilds drop the affected blocks
// ---------------------------------------------------------------------------

/// Beam-2 mesh with one not-ghost element `[0, 1]` and one ghost element
/// `[2, 3]` (independent nodes).
fn beam2_with_independent_ghost() -> Mesh {
    let ty = E::BernoulliBeam2;
    let mut m = Mesh::new(2, "v60_ghost").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for x in [[0., 0.], [1., 0.], [2., 0.], [3., 0.]] {
            acc.push_node(&x).unwrap();
        }
        acc.connectivity_mut(ty, G).push_tuple(&[0, 1]);
        acc.connectivity_mut(ty, GhostType::Ghost)
            .push_tuple(&[2, 3]);
    }
    m
}

/// The ghost element collapses, the event rebuild fails in
/// the ghost type. Before the fix the ghost blocks kept their old data and
/// `element_stiffness(…, Ghost)` returned `Ok` from them; now the ghost
/// blocks are dropped (every read reports "not initialised"), the
/// not-ghost blocks are rebuilt and fresh, and a later successful refresh
/// restores the ghost type.
#[test]
fn fs2_03_failed_event_rebuild_drops_the_failing_ghost_type() {
    let ty = E::BernoulliBeam2;
    let gh = GhostType::Ghost;
    let mut m = beam2_with_independent_ghost();
    let mut fe = structural_common::engine(&m);
    let nq = fe.nb_integration_points(ty, gh).unwrap();
    let d = Array::from_vec([1., 0., 0., 1.].repeat(nq), 4).unwrap();
    // collapse the ghost element only
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[6..8].copy_from_slice(&[2., 0.]);
    let r = fe.on_mesh_event(
        &m,
        &MeshEvent::MeshIsDistributed {
            origin: "v60".into(),
        },
    );
    assert!(matches!(r, Err(Error::Numerical(_))), "{r:?}");
    // the not-ghost type was rebuilt
    fe.check_up_to_date(&m, ty, G).unwrap();
    let mut k = Array::new(1);
    fe.element_stiffness(&m, d.view(), &mut k, ty, G).unwrap();
    // the ghost type has no data any more
    let msg = format!("{:?}", fe.check_up_to_date(&m, ty, gh).unwrap_err());
    assert!(msg.contains("no precomputed data"), "{msg}");
    assert!(msg.contains("rebuild failed"), "{msg}");
    let mut k = sentinel();
    assert!(fe.element_stiffness(&m, d.view(), &mut k, ty, gh).is_err());
    assert!(untouched(&k));
    assert!(fe.rotations(&m, ty, gh).is_err());
    assert!(fe.shapes(&m, ty, gh).is_err());
    assert!(fe.shapes_derivatives(&m, ty, gh).is_err());
    assert!(fe.jacobians(&m, ty, gh).is_err());
    assert!(fe.quadrature_rule(ty, gh).is_err());
    // repaired mesh: refresh restores the ghost type (it stays initialised)
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[6..8].copy_from_slice(&[3., 0.]);
    fe.refresh(&m).unwrap();
    fe.check_up_to_date(&m, ty, gh).unwrap();
    let fresh = structural_common::engine(&m);
    assert_eq!(
        fe.shapes_derivatives(&m, ty, gh).unwrap().as_slice(),
        fresh.shapes_derivatives(&m, ty, gh).unwrap().as_slice()
    );
}

// ---------------------------------------------------------------------------
// Stale data: every operation that reads a precomputed block checks it first
// ---------------------------------------------------------------------------

/// Run every public operation of `fe` that reads a precomputed block of
/// `(ty, G)` (except the mesh-reading field operations covered by
/// `review_v55_regressions.rs::fs03_*`) against `m`; return the operations
/// that succeeded and those that changed their output while failing.
fn block_operations(
    fe: &StructuralFeEngine,
    m: &Mesh,
    ty: E,
) -> (Vec<&'static str>, Vec<&'static str>) {
    let nq = fe.nb_integration_points(ty, G).unwrap();
    let ne = m.nb_element(ty, G);
    let (ndof, ns) = match ty {
        E::BernoulliBeam2 => (3, 2),
        _ => (6, 4),
    };
    let zeros = |n: usize, c: usize| Array::from_vec(vec![0.25; n * c], c).unwrap();
    let (mut ok, mut touched) = (Vec::new(), Vec::new());
    let mut record = |name: &'static str, r: akantu_core::Result<()>, out: &Array<Real>| match r {
        Ok(()) => ok.push(name),
        Err(e) => {
            assert!(matches!(e, Error::InvalidArgument(_)), "{name}: {e:?}");
            if !untouched(out) {
                touched.push(name);
            }
        }
    };
    let sentinel_unchanged = sentinel();
    record(
        "rotations",
        fe.rotations(m, ty, G).map(|_| ()),
        &sentinel_unchanged,
    );
    record(
        "shapes",
        fe.shapes(m, ty, G).map(|_| ()),
        &sentinel_unchanged,
    );
    record(
        "shapes_derivatives",
        fe.shapes_derivatives(m, ty, G).map(|_| ()),
        &sentinel_unchanged,
    );
    record(
        "jacobians",
        fe.jacobians(m, ty, G).map(|_| ()),
        &sentinel_unchanged,
    );
    record(
        "precomputed",
        fe.precomputed(m).map(|_| ()),
        &sentinel_unchanged,
    );
    let mut out = sentinel();
    let r = fe.integrate(m, zeros(ne * nq, 2).view(), &mut out, 2, ty, G, None);
    record("integrate", r, &out);
    let mut out = sentinel();
    let r = fe.compute_btd(m, zeros(ne * nq, ns).view(), &mut out, ty, G, None);
    record("compute_btd", r, &out);
    let mut out = sentinel();
    let r = fe.compute_btdb(m, zeros(ne * nq, ns * ns).view(), &mut out, ty, G, None);
    record("compute_btdb", r, &out);
    let mut out = sentinel();
    let r = fe.compute_ntb(m, zeros(ne * nq, ndof).view(), &mut out, ty, G, None);
    record("compute_ntb", r, &out);
    let mut out = sentinel();
    let r = fe.compute_ntbn(m, zeros(ne * nq, ndof).view(), &mut out, ty, G, None);
    record("compute_ntbn", r, &out);
    let mut out = sentinel();
    let r = fe.element_stiffness(m, zeros(ne * nq, ns * ns).view(), &mut out, ty, G);
    record("element_stiffness", r, &out);
    let mut out = sentinel();
    let r = fe.line_load_local(m, zeros(ne * nq, ndof).view(), &mut out, ty, G);
    record("line_load_local", r, &out);
    let mut out = sentinel();
    let r = fe.tractions_to_local(m, zeros(ne * nq, ndof).view(), &mut out, ty, G);
    record("tractions_to_local", r, &out);
    (ok, touched)
}

const ALL_BLOCK_OPERATIONS: usize = 13;

/// After a mesh mutation that was not forwarded
/// (moved node: new revision; edited `extra_normal`: same revision, no
/// event), **every** operation that reads a precomputed block refuses with
/// `Error::InvalidArgument` before touching its output (before the fix the
/// operations without a mesh argument served the stale blocks); after
/// `refresh` they all succeed.
#[test]
fn fs2_03_block_operations_refuse_stale_blocks() {
    // moved node (beam 2-D)
    let ty = E::BernoulliBeam2;
    let mut m = mesh(2, ty, &[vec![0., 0.], vec![1., 0.]], &[vec![0, 1]], None);
    let mut fe = structural_common::engine(&m);
    let (ok, touched) = block_operations(&fe, &m, ty);
    assert_eq!(ok.len(), ALL_BLOCK_OPERATIONS, "{ok:?}");
    assert!(touched.is_empty());
    MeshAccessor::new(&mut m).nodes_mut().as_mut_slice()[2] = 2.;
    let (ok, touched) = block_operations(&fe, &m, ty);
    assert!(ok.is_empty(), "served stale blocks: {ok:?}");
    assert!(touched.is_empty(), "output changed: {touched:?}");
    fe.refresh(&m).unwrap();
    assert_eq!(block_operations(&fe, &m, ty).0.len(), ALL_BLOCK_OPERATIONS);

    // edited extra_normal (beam 3-D): no revision change, no event
    let ty = E::BernoulliBeam3;
    let mut m = mesh(
        3,
        ty,
        &[vec![0., 0., 0.], vec![1., 0., 0.]],
        &[vec![0, 1]],
        Some(&[vec![0., 0., 1.]]),
    );
    let mut fe = structural_common::engine(&m);
    assert_eq!(block_operations(&fe, &m, ty).0.len(), ALL_BLOCK_OPERATIONS);
    let rev = m.revision();
    m.data_mut()
        .reals_mut(EXTRA_NORMAL)
        .unwrap()
        .get_mut(ty, G)
        .unwrap()
        .as_mut_slice()
        .copy_from_slice(&[0., 1., 0.]);
    assert_eq!(m.revision(), rev);
    let (ok, touched) = block_operations(&fe, &m, ty);
    assert!(ok.is_empty(), "served stale blocks: {ok:?}");
    assert!(touched.is_empty(), "output changed: {touched:?}");
    fe.refresh(&m).unwrap();
    assert_eq!(block_operations(&fe, &m, ty).0.len(), ALL_BLOCK_OPERATIONS);
}

// ---------------------------------------------------------------------------
// Beam normals: admissible 3-D beam normals (UD-343)
// ---------------------------------------------------------------------------

fn x_axis_beam3(normal: [Real; 3]) -> Mesh {
    mesh(
        3,
        E::BernoulliBeam3,
        &[vec![0., 0., 0.], vec![1., 0., 0.]],
        &[vec![0, 1]],
        Some(&[normal.to_vec()]),
    )
}

/// Non-unit (`(0,0,2)`, `(0,0,0.5)`), unit but skew
/// (`(0.6,0,0.8)`) and nearly parallel (`(1,1e-8,1e-8)`, `(1,1e-150,0)`)
/// normals of an x-axis beam, and normals just outside the criterion, are
/// refused by default (precomputation: `Error::InvalidArgument` naming the
/// element; before the fix they were accepted with a non-orthogonal `R`).
/// Normals just inside the criterion are accepted.
#[test]
fn fs2_04_inadmissible_normals_are_refused_by_default() {
    for n in [
        [0., 0., 2.],
        [0., 0., 0.5],
        [0.6, 0., 0.8],
        [1., 1e-8, 1e-8],
        [1., 1e-150, 0.],
        [2e-10, 0., 1.],
        [0., 0., 1. + 2e-10],
        [0., 0., 1. - 2e-10],
    ] {
        let m = x_axis_beam3(n);
        match StructuralFeEngine::with_precomputation(&m, None, "v60") {
            Err(Error::InvalidArgument(msg)) => {
                assert!(msg.contains("extra_normal"), "{n:?}: {msg}");
                assert!(
                    msg.contains("element 0 of _bernoulli_beam_3:_not_ghost"),
                    "{n:?}: {msg}"
                );
            }
            other => panic!("{n:?}: expected Error::InvalidArgument, got {other:?}"),
        }
    }
    for n in [[5e-11, 0., 1.], [0., 0., 1. + 4e-11], [0., 1., 0.]] {
        let m = x_axis_beam3(n);
        StructuralFeEngine::with_precomputation(&m, None, "v60").unwrap();
    }
}

/// The legacy switch reproduces upstream on inadmissible normals: an
/// x-axis beam with `n = (0,0,2)` gets the (non-rotation) frame
/// `R = diag(1, 0.5, 0.5)` in both diagonal blocks (oracle case
/// `nonunit_normal`), oracle-independent closed form:
/// `Pg = [x, x × n, n] = diag(1, −2, 2)`, `R = diag(1, −1, 1)·Pg⁻¹`.
#[test]
fn fs2_04_legacy_switch_reproduces_upstream_on_a_non_unit_normal() {
    let ty = E::BernoulliBeam3;
    let m = x_axis_beam3([0., 0., 2.]);
    let mut want = [0.; 36];
    for (i, v) in [1., 0.5, 0.5, 1., 0.5, 0.5].into_iter().enumerate() {
        want[i + 6 * i] = v;
    }
    // kernel level: default refuses, the switch alone reproduces upstream
    let x = [0., 0., 0., 1., 0., 0.];
    let xc = Coords::new(&x, 3, 2).unwrap();
    let mut r = [0.; 36];
    assert!(matches!(
        BernoulliBeam3::compute_rotation_matrix(xc, &[0., 0., 2.], &mut r),
        Err(Error::InvalidArgument(_))
    ));
    let unchecked = StructuralUpstreamCompat {
        beam3_normal_unchecked: true,
        ..StructuralUpstreamCompat::default()
    };
    BernoulliBeam3::compute_rotation_matrix_with(xc, &[0., 0., 2.], unchecked, &mut r).unwrap();
    assert_eq!(r, want);
    // engine level, UPSTREAM compat
    let fe = StructuralFeEngine::with_precomputation_compat(
        &m,
        None,
        "v60",
        StructuralUpstreamCompat::UPSTREAM,
    )
    .unwrap();
    assert_eq!(fe.rotations(&m, ty, G).unwrap().as_slice(), want);
    // the default engine refuses the same mesh
    assert!(matches!(
        StructuralFeEngine::with_precomputation(&m, None, "v60"),
        Err(Error::InvalidArgument(_))
    ));
}

/// Admissible normals are used raw, exactly as upstream: for normals
/// inside the criterion (exact unit normals, normals off by a few ulps,
/// and normals at `±4e-11` from unit length / orthogonality) the default
/// rotation is bit-identical to the unchecked (upstream) one.
#[test]
fn fs2_04_admissible_normals_keep_their_bits() {
    let unchecked = StructuralUpstreamCompat {
        beam3_normal_unchecked: true,
        ..StructuralUpstreamCompat::default()
    };
    let s = std::f64::consts::FRAC_1_SQRT_2;
    let cases: [([Real; 6], [Real; 3]); 5] = [
        ([0., 0., 0., 1., 0., 0.], [0., 0., 1.]),
        ([0., 0., 0., 1., 0., 0.], [0., 0., 1. + 4e-11]),
        ([0., 0., 0., 1., 0., 0.], [4e-11, -1., 0.]),
        ([0.2, -0.1, 0.3, 1.2, 0.9, 0.3], [-s, s, 0.]),
        (
            [0., 0., 0., 3., 4., 12.],
            [-36. / 65., -48. / 65., 5. / 13.],
        ),
    ];
    for (x, n) in cases {
        let xc = Coords::new(&x, 3, 2).unwrap();
        let (mut a, mut b) = ([0.; 36], [0.; 36]);
        BernoulliBeam3::compute_rotation_matrix(xc, &n, &mut a).unwrap();
        BernoulliBeam3::compute_rotation_matrix_with(xc, &n, unchecked, &mut b).unwrap();
        let bits = |v: &[Real]| v.iter().map(|x| x.to_bits()).collect::<Vec<_>>();
        assert_eq!(bits(&a), bits(&b), "{n:?}");
    }
}

// ---------------------------------------------------------------------------
// Overflow: error messages
// ---------------------------------------------------------------------------

/// Squared lengths that overflow to `inf` are refused with a
/// message saying so ("too large … overflows"), not "degenerate" (beam 2-D
/// and 3-D, DKT edge; and a huge but finite beam normal whose binormal
/// overflows is not called "parallel").
#[test]
fn fs2_06_overflow_is_not_called_degenerate() {
    let huge_beam3 = Some(vec![vec![0., 0., 1.]]);
    for (dim, ty, nodes, normals) in [
        (
            2,
            E::BernoulliBeam2,
            vec![vec![0., 0.], vec![1e200, 0.]],
            None,
        ),
        (
            3,
            E::BernoulliBeam3,
            vec![vec![0., 0., 0.], vec![1e200, 0., 0.]],
            huge_beam3,
        ),
        (
            3,
            E::DiscreteKirchhoffTriangle18,
            vec![vec![0., 0., 0.], vec![1e155, 0., 0.], vec![0., 1e155, 0.]],
            None,
        ),
        (
            3,
            E::BernoulliBeam3,
            vec![vec![0., 0., 0.], vec![1., 0., 0.]],
            Some(vec![vec![0., 0., 1e300]]),
        ),
    ] {
        let conn = vec![(0..ty.nb_nodes_per_element() as akantu_core::Idx).collect()];
        let m = mesh(dim, ty, &nodes, &conn, normals.as_deref());
        match StructuralFeEngine::with_precomputation(&m, None, "v60") {
            Err(Error::Numerical(msg)) => {
                assert!(
                    msg.contains("too large") && msg.contains("overflows"),
                    "{msg}"
                );
                assert!(
                    !msg.contains("degenerate") && !msg.contains("parallel"),
                    "{msg}"
                );
                assert!(msg.contains("element 0 of"), "{msg}");
            }
            other => panic!("{ty:?}: expected Error::Numerical, got {other:?}"),
        }
    }
}

/// `FeEngine` refuses structural shape functions and
/// points to `StructuralFeEngine` (it pointed to a work package before).
#[test]
fn fs2_06_fe_engine_points_to_the_structural_engine() {
    use akantu_core::ElementKind;
    use akantu_fe::FeEngine;
    use akantu_fe::shape::ShapeFamily;
    let m = mesh(
        2,
        E::BernoulliBeam2,
        &[vec![0., 0.], vec![1., 0.]],
        &[vec![0, 1]],
        None,
    );
    for (kind, family) in [
        (ElementKind::Structural, ShapeFamily::Structural),
        (ElementKind::Regular, ShapeFamily::Structural),
        (ElementKind::Structural, ShapeFamily::Lagrange),
    ] {
        match FeEngine::new(&m, None, kind, family, "v60") {
            Err(e) => {
                let msg = format!("{e}");
                assert!(msg.contains("StructuralFeEngine"), "{msg}");
                assert!(!msg.contains("WP-STRUCT"), "{msg}");
            }
            Ok(_) => panic!("{kind:?} {family:?}: accepted"),
        }
    }
}
