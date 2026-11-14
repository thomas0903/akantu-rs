// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): regression tests of the
// checked FE-engine API (no upstream
// counterpart — upstream takes fixed-size Eigen outputs and stores the
// elemental interpolation matrices inside the material).

//! `Result`-returning FE APIs validate caller-sized buffers and
//! caller data (output slices, nodal arrays, renumberings, stale meshes)
//! and return errors instead of panicking.
//!
//! An [`akantu_fe::engine::ElementalFieldInterpolation`] is bound to
//! the element ids it was built for; reuse with another filter maps by
//! element id or fails, never silently using another element's matrices.
//!
//! One test group per defect, each reproducing a failing case: stale
//! elemental interpolation (after moved nodes / same-count
//! renumbering), removal numberings of a wrong but divisor length,
//! several components, invalid negative ids; nothing changed on error, in
//! any `(type, ghost)` block, malformed connectivity / coordinate
//! storage through checked queries and facet normals, and the normal
//! kernel with a spatial dimension beyond its scratch capacity.
//!
//! Analytical expectations only (affine 1-D map, Lagrange polynomials).

use akantu_core::{
    Array, Element, ElementKind, ElementType, ElementTypeMapArray, Error, GhostType, Idx,
};
use akantu_fe::FeEngine;
use akantu_fe::element_class::{LagrangeElement, types};
use akantu_fe::kernels;
use akantu_fe::shape::ShapeFamily;
use akantu_mesh::{Mesh, MeshAccessor};

const G: GhostType = GhostType::NotGhost;
const S3: ElementType = ElementType::Segment3;

/// The test mesh: nodes `[0, 1, 0.5, 2, 4, 3]`, segment3 elements
/// `[0, 1, 2]` (on `[0, 1]`) and `[3, 4, 5]` (on `[2, 4]`), plus optional
/// extra elements `[2, 4]·k`-shifted copies to vary the filter lengths.
fn segment3_mesh(extra: usize) -> Mesh {
    let mut mesh = Mesh::new(1, "seg3").unwrap();
    let mut acc = MeshAccessor::new(&mut mesh);
    for x in [0.0, 1.0, 0.5, 2.0, 4.0, 3.0] {
        acc.push_node(&[x]).unwrap();
    }
    for k in 0..extra {
        // element on [5 + 2k, 6 + 2k]
        let a = 5.0 + 2.0 * k as f64;
        for x in [a, a + 1.0, a + 0.5] {
            acc.push_node(&[x]).unwrap();
        }
    }
    let mut c = acc.connectivity_mut(S3, G);
    c.push_tuple(&[0, 1, 2]);
    c.push_tuple(&[3, 4, 5]);
    for k in 0..extra as Idx {
        let b = 6 + 3 * k;
        c.push_tuple(&[b, b + 1, b + 2]);
    }
    drop(c);
    mesh
}

fn engine(mesh: &Mesh) -> FeEngine {
    FeEngine::with_precomputation(
        mesh,
        None,
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "fe",
    )
    .unwrap()
}

fn dimension_mismatch<T: std::fmt::Debug>(r: akantu_core::Result<T>) -> bool {
    matches!(r, Err(Error::DimensionMismatch { .. }))
}

// ---------------------------------------------------------------------------
// Shape and coordinate buffers
// ---------------------------------------------------------------------------

/// A two-entry shapes buffer for a segment3 panicked in
/// `compute_shapes_at`. Short, long and exact buffers.
#[test]
fn compute_shapes_at_validates_the_output_length() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let e = Element::new(S3, 0, G);
    let r = std::panic::catch_unwind(|| {
        let mut short = [0.0; 2];
        fe.compute_shapes_at(&mesh, &[0.25], e, &mut short)
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
    let mut long = [7.0; 4];
    assert!(dimension_mismatch(fe.compute_shapes_at(
        &mesh,
        &[0.25],
        e,
        &mut long
    )));
    assert_eq!(long, [7.0; 4], "nothing written on error");
    // x = 0.25 on [0, 1] (mid node 0.5): ξ = −0.5;
    // N = ((ξ−1)ξ/2, (ξ+1)ξ/2, 1−ξ²) = (0.375, −0.125, 0.75)
    let mut n = [0.0; 3];
    fe.compute_shapes_at(&mesh, &[0.25], e, &mut n).unwrap();
    assert_eq!(n, [0.375, -0.125, 0.75]);
}

/// `compute_shape_derivatives_at` checks `sdim · nn` before any work.
#[test]
fn compute_shape_derivatives_at_validates_the_output_length() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let e = Element::new(S3, 0, G);
    let r = std::panic::catch_unwind(|| {
        let mut short = [0.0; 2];
        fe.compute_shape_derivatives_at(&mesh, &[0.25], e, &mut short)
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
    let mut long = [0.0; 6];
    assert!(dimension_mismatch(fe.compute_shape_derivatives_at(
        &mesh,
        &[0.25],
        e,
        &mut long
    )));
    // dN/dξ(−0.5) = (ξ−½, ξ+½, −2ξ) = (−1, 0, 1); dx/dξ = ½ ⇒ dN/dx = (−2, 0, 2)
    let mut b = [9.0; 3];
    fe.compute_shape_derivatives_at(&mesh, &[0.25], e, &mut b)
        .unwrap();
    assert_eq!(b, [-2.0, 0.0, 2.0]);
}

/// `interpolate` checks the result length and the nodal array.
#[test]
fn interpolate_validates_the_output_and_the_nodal_field() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let e = Element::new(S3, 1, G);
    // two-component field (x, −2x)
    let mut nodal = Array::new(2);
    for x in mesh.nodes().as_slice() {
        nodal.push_tuple(&[*x, -2.0 * x]);
    }
    let r = std::panic::catch_unwind(|| {
        let mut short = [0.0; 1];
        fe.interpolate(&mesh, &[3.5], nodal.view(), e, &mut short)
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
    let mut long = [0.0; 3];
    assert!(dimension_mismatch(fe.interpolate(
        &mesh,
        &[3.5],
        nodal.view(),
        e,
        &mut long
    )));
    // nodal array with too few tuples
    let few = Array::from_vec(vec![0.0; 4], 2).unwrap();
    let mut out = [0.0; 2];
    assert!(dimension_mismatch(fe.interpolate(
        &mesh,
        &[3.5],
        few.view(),
        e,
        &mut out
    )));
    fe.interpolate(&mesh, &[3.5], nodal.view(), e, &mut out)
        .unwrap();
    assert!((out[0] - 3.5).abs() <= 1e-15 && (out[1] + 7.0).abs() <= 2e-15);
}

/// The natural-coordinate buffer of `inverse_map` and invalid
/// elements are errors.
#[test]
fn inverse_map_validates_its_buffers_and_element() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let e = Element::new(S3, 0, G);
    assert!(dimension_mismatch(fe.inverse_map(
        &mesh,
        &[0.25],
        e,
        &mut [0.0; 2]
    )));
    assert!(dimension_mismatch(fe.inverse_map(
        &mesh,
        &[0.25, 0.0],
        e,
        &mut [0.0; 1]
    )));
    assert!(matches!(
        fe.inverse_map(&mesh, &[0.25], Element::new(S3, 2, G), &mut [0.0; 1]),
        Err(Error::OutOfRange { .. })
    ));
    assert!(
        fe.inverse_map(&mesh, &[0.25], Element::NULL, &mut [0.0; 1])
            .is_err()
    );
}

/// Public `Result`-returning kernels validate their buffers.
#[test]
fn kernels_validate_their_buffers() {
    use types::{Quadrangle4, Segment2, Triangle6};
    // computeD2NDS2: nd² × nn values
    let mut d2 = [0.0; 15];
    assert!(dimension_mismatch(Quadrangle4::compute_d2nds2(
        &[0.0, 0.0],
        &mut d2
    )));
    let mut d2 = [0.0; 1];
    assert!(dimension_mismatch(Segment2::compute_d2nds2(
        &[0.0],
        &mut d2
    )));
    assert!(matches!(
        Triangle6::compute_d2nds2(&[0.0, 0.0], &mut [0.0; 24]),
        Err(Error::NotImplemented(_))
    ));
    // normals: x must be sdim × nn, n must be nb_points × sdim
    let x = [0.0, 0.0, 1.0, 0.0]; // segment (0,0)–(1,0)
    let pts = [0.0, 0.5];
    let mut n = [0.0; 4];
    kernels::normals_on_natural::<Segment2>(&pts, &x, 2, false, &mut n).unwrap();
    assert_eq!(n, [0.0, -1.0, 0.0, -1.0]);
    assert!(dimension_mismatch(kernels::normals_on_natural::<Segment2>(
        &pts,
        &x,
        2,
        false,
        &mut [0.0; 3]
    )));
    assert!(dimension_mismatch(kernels::normals_on_natural::<Segment2>(
        &pts,
        &x[..3],
        2,
        false,
        &mut n
    )));
    assert!(dimension_mismatch(kernels::normals_on_natural::<Segment2>(
        &pts, &x, 3, false, &mut n
    )));
}

/// Batch operations on a mesh that changed after
/// `init_shape_functions` (events not forwarded) fail instead of indexing
/// the precomputed arrays out of bounds.
#[test]
fn stale_precomputed_data_is_an_error() {
    let small = segment3_mesh(0);
    let fe = engine(&small);
    let big = segment3_mesh(2);
    let u = Array::from_vec(vec![0.0; big.nb_nodes()], 1).unwrap();
    let mut out = Array::new(1);
    let r = std::panic::catch_unwind(|| {
        let mut out = Array::new(1);
        fe.interpolate_on_integration_points(&big, u.view(), &mut out, 1, S3, G, None)
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
    assert!(dimension_mismatch(fe.gradient_on_integration_points(
        &big,
        u.view(),
        &mut out,
        1,
        S3,
        G,
        None
    )));
    let mut xq = Array::new(1);
    assert!(dimension_mismatch(fe.integration_points_coordinates(
        &big,
        &mut xq,
        S3,
        G,
        Some(&[3])
    )));
}

/// `on_elements_removed` validates the caller's numbering
/// before modifying anything.
#[test]
fn invalid_renumbering_is_rejected_without_changes() {
    let mesh = segment3_mesh(1); // 3 elements
    let mut fe = engine(&mesh);
    let before = fe.jacobians(S3, G).unwrap().as_slice().to_vec();
    let shapes_before = fe.shapes(S3, G).unwrap().as_slice().to_vec();
    let numbering = |v: &[Idx]| {
        let mut m = ElementTypeMapArray::new("numbering");
        let a = m.alloc(0, 1, S3, G, 0).unwrap();
        for &x in v {
            a.push_value(x);
        }
        m
    };
    for bad in [
        &[0, 1, 2, 3][..], // wrong length: not a whole number of element blocks
        &[5, -1, 0][..],   // new index beyond the kept elements
        &[0, 0, -1][..],   // duplicated new index
        &[-1, 2, 0][..],   // 2 ≥ 2 kept elements
    ] {
        assert!(
            fe.on_elements_removed(&numbering(bad)).is_err(),
            "{bad:?} accepted"
        );
        assert_eq!(fe.jacobians(S3, G).unwrap().as_slice(), before.as_slice());
        assert_eq!(
            fe.shapes(S3, G).unwrap().as_slice(),
            shapes_before.as_slice()
        );
    }
    // a permutation is a valid renumbering (applied twice = identity)
    fe.on_elements_removed(&numbering(&[0, 2, 1])).unwrap();
    fe.on_elements_removed(&numbering(&[0, 2, 1])).unwrap();
    assert_eq!(fe.jacobians(S3, G).unwrap().as_slice(), before.as_slice());
    // a valid removal still works: drop element 1
    fe.on_elements_removed(&numbering(&[0, -1, 1])).unwrap();
    let nq = fe.nb_integration_points(S3, G).unwrap();
    let after = fe.jacobians(S3, G).unwrap().as_slice();
    assert_eq!(&after[..nq], &before[..nq]);
    assert_eq!(&after[nq..], &before[2 * nq..]);
}

// ---------------------------------------------------------------------------
// Filtered caches
// ---------------------------------------------------------------------------

/// One interpolation point per element: `targets[e]`.
fn points(mesh: &Mesh, targets: &[f64]) -> ElementTypeMapArray<f64> {
    assert_eq!(targets.len(), mesh.nb_element(S3, G));
    let mut p = ElementTypeMapArray::new("points");
    let a = p.alloc(0, 1, S3, G, 0.0).unwrap();
    for &t in targets {
        a.push_value(t);
    }
    p
}

fn filter(ids: &[Idx]) -> ElementTypeMapArray<Idx> {
    let mut f = ElementTypeMapArray::new("filter");
    let a = f.alloc(0, 1, S3, G, 0).unwrap();
    for &e in ids {
        a.push_value(e);
    }
    f
}

/// Quadrature-point field `f(x) = x` for the filtered elements, in filter
/// order (a material internal).
fn affine_field(fe: &FeEngine, mesh: &Mesh, ids: Option<&[Idx]>) -> ElementTypeMapArray<f64> {
    let mut xq = Array::new(1);
    fe.integration_points_coordinates(mesh, &mut xq, S3, G, ids)
        .unwrap();
    let mut f = ElementTypeMapArray::new("field");
    let a = f.alloc(0, 1, S3, G, 0.0).unwrap();
    for &x in xq.as_slice() {
        a.push_value(x);
    }
    f
}

fn run(
    fe: &FeEngine,
    mesh: &Mesh,
    interp: &akantu_fe::engine::ElementalFieldInterpolation,
    ids: Option<&[Idx]>,
) -> akantu_core::Result<Vec<f64>> {
    let field = affine_field(fe, mesh, ids);
    let flt = ids.map(filter);
    let mut out = ElementTypeMapArray::new("out");
    fe.interpolate_elemental_field(mesh, interp, &field, &mut out, G, flt.as_ref())?;
    Ok(out.get(S3, G).unwrap().as_slice().to_vec())
}

fn close(a: f64, b: f64) -> bool {
    (a - b).abs() <= 4.0 * f64::EPSILON * b.abs().max(1.0)
}

/// Built for all elements (no filter), used
/// with filter `[1]`: element 1 must get **3.5** (it got 2.5 = element 0's
/// fit at element 1's target before the fix).
#[test]
fn all_to_subset_maps_by_element_id() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let interp = fe
        .init_elemental_field_interpolation(&mesh, &points(&mesh, &[0.25, 3.5]), None)
        .unwrap();
    assert_eq!(interp.elements(S3, G), Some(&[0, 1][..]));
    let out = run(&fe, &mesh, &interp, Some(&[1])).unwrap();
    assert_eq!(out[0], 0.0, "element 0 not written");
    assert!(close(out[1], 3.5), "element 1: {}", out[1]);
}

/// A subset initialisation cannot serve another element (before the fix,
/// initialised with `[0]` and used with `[1]` it gave 2.5).
#[test]
fn missing_element_ids_are_rejected() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let pts = points(&mesh, &[0.25, 3.5]);
    let interp = fe
        .init_elemental_field_interpolation(&mesh, &pts, Some(&filter(&[0])))
        .unwrap();
    assert_eq!(interp.elements(S3, G), Some(&[0][..]));
    assert!(matches!(
        run(&fe, &mesh, &interp, Some(&[1])),
        Err(Error::InvalidArgument(_))
    ));
    // no filter = all elements, element 1 missing as well
    assert!(run(&fe, &mesh, &interp, None).is_err());
    // the initialised element still works
    let out = run(&fe, &mesh, &interp, Some(&[0])).unwrap();
    assert!(close(out[0], 0.25));
}

/// Reordered filters (at initialisation and/or at use) and filters
/// of different lengths map every element to its own matrices.
#[test]
fn reordered_and_resized_filters_map_by_element_id() {
    let mesh = segment3_mesh(2); // elements on [0,1], [2,4], [5,6], [7,8]
    let fe = engine(&mesh);
    let targets = [0.25, 3.5, 5.75, 7.125];
    let pts = points(&mesh, &targets);
    for init in [None, Some(&[3, 1, 0, 2][..]), Some(&[2, 0, 3, 1, 2][..])] {
        let flt = init.map(filter);
        let interp = fe
            .init_elemental_field_interpolation(&mesh, &pts, flt.as_ref())
            .unwrap();
        for use_ids in [
            None,
            Some(&[1, 0][..]),
            Some(&[3][..]),
            Some(&[2, 3, 0, 1][..]),
            Some(&[0, 2][..]),
        ] {
            let out = run(&fe, &mesh, &interp, use_ids).unwrap();
            let all: Vec<usize> = (0..targets.len()).collect();
            let ids: Vec<usize> = use_ids.map_or(all, |v| v.iter().map(|&e| e as usize).collect());
            for (e, t) in targets.iter().enumerate() {
                if ids.contains(&e) {
                    assert!(
                        close(out[e], *t),
                        "init {init:?} use {use_ids:?}: element {e}: {}",
                        out[e]
                    );
                } else {
                    assert_eq!(
                        out[e], 0.0,
                        "init {init:?} use {use_ids:?}: element {e} written"
                    );
                }
            }
        }
    }
}

/// Field sizes must follow the requested filter, and types that were
/// not initialised are errors.
#[test]
fn interpolation_inputs_are_validated() {
    let mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    let interp = fe
        .init_elemental_field_interpolation(&mesh, &points(&mesh, &[0.25, 3.5]), None)
        .unwrap();
    // field sized for two elements, filter of one
    let field = affine_field(&fe, &mesh, None);
    let mut out = ElementTypeMapArray::new("out");
    assert!(dimension_mismatch(fe.interpolate_elemental_field(
        &mesh,
        &interp,
        &field,
        &mut out,
        G,
        Some(&filter(&[1]))
    )));
    // ghost block not initialised (but present in the mesh)
    let mut mesh_g = segment3_mesh(0);
    {
        let mut acc = MeshAccessor::new(&mut mesh_g);
        acc.connectivity_mut(S3, GhostType::Ghost)
            .push_tuple(&[0, 1, 2]);
    }
    let fe_g = engine(&mesh_g);
    // built for the not-ghost block of the same mesh (same revision) only
    let interp_g = fe_g
        .init_elemental_field_interpolation(
            &mesh_g,
            &points(&mesh_g, &[0.25, 3.5]),
            Some(&filter(&[0, 1])),
        )
        .unwrap();
    let mut field_g = ElementTypeMapArray::new("field");
    field_g.alloc(2, 1, S3, GhostType::Ghost, 0.0).unwrap();
    assert!(matches!(
        fe_g.interpolate_elemental_field(
            &mesh_g,
            &interp_g,
            &field_g,
            &mut out,
            GhostType::Ghost,
            None
        ),
        Err(Error::InvalidArgument(m)) if m.contains("not initialised")
    ));
    // malformed interpolation points
    let mut bad = ElementTypeMapArray::new("points");
    bad.alloc(3, 1, S3, G, 0.0).unwrap();
    assert!(dimension_mismatch(
        fe.init_elemental_field_interpolation(&mesh, &bad, None)
    ));
}

// ---------------------------------------------------------------------------
// Stale data, numberings, malformed storage, normals
// ---------------------------------------------------------------------------

/// Interpolate `field` with `interp` into an output pre-filled with the
/// sentinel 7 (so that "nothing written" is observable).
fn run_into_sentinel(
    fe: &FeEngine,
    mesh: &Mesh,
    interp: &akantu_fe::engine::ElementalFieldInterpolation,
    field: &ElementTypeMapArray<f64>,
    ids: &[Idx],
) -> (akantu_core::Result<()>, Vec<f64>) {
    let mut out = ElementTypeMapArray::new("out");
    out.alloc(mesh.nb_element(S3, G), 1, S3, G, 7.0).unwrap();
    let r = fe.interpolate_elemental_field(mesh, interp, field, &mut out, G, Some(&filter(ids)));
    (r, out.get(S3, G).unwrap().as_slice().to_vec())
}

/// Stale interpolation (moved nodes, same element ids): built at
/// revision 1 for targets `[0.25, 3.5]`; nodes 4 and 5 move so that element
/// 1 becomes `[2, 6]` (mid node 4); the engine itself is re-initialised.
/// The stale object returned `Ok` with 5.0000000000000036 instead of 3.5;
/// it must now be refused before anything is written, and a rebuilt object
/// gives 3.5.
#[test]
fn fe2_01_stale_interpolation_after_moved_nodes_is_rejected() {
    let mut mesh = segment3_mesh(0);
    let mut fe = engine(&mesh);
    let pts = points(&mesh, &[0.25, 3.5]);
    let interp = fe
        .init_elemental_field_interpolation(&mesh, &pts, None)
        .unwrap();
    assert_eq!(interp.mesh_revision(), mesh.revision());
    let built_at = mesh.revision();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let mut x = acc.nodes_mut();
        let x = x.as_mut_slice();
        x[4] = 6.0;
        x[5] = 4.0;
    }
    assert_ne!(mesh.revision(), built_at);
    fe.init_shape_functions(&mesh, G).unwrap();
    let field = affine_field(&fe, &mesh, Some(&[1]));
    let (r, out) = run_into_sentinel(&fe, &mesh, &interp, &field, &[1]);
    assert!(
        matches!(&r, Err(Error::InvalidArgument(m)) if m.contains("revision")),
        "{r:?}"
    );
    assert_eq!(out, [7.0, 7.0], "nothing written on error");
    // rebuilt for the current revision: f(3.5) = 3.5 on the moved element
    let rebuilt = fe
        .init_elemental_field_interpolation(&mesh, &pts, None)
        .unwrap();
    let (r, out) = run_into_sentinel(&fe, &mesh, &rebuilt, &field, &[1]);
    r.unwrap();
    assert_eq!(out[0], 7.0);
    assert!(close(out[1], 3.5), "{}", out[1]);
}

/// Same-count element renumbering (the two elements swap their
/// connectivities; every element id still exists) is refused as well.
#[test]
fn fe2_01_stale_interpolation_after_same_count_renumbering_is_rejected() {
    let mut mesh = segment3_mesh(0);
    let mut fe = engine(&mesh);
    let interp = fe
        .init_elemental_field_interpolation(&mesh, &points(&mesh, &[0.25, 3.5]), None)
        .unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let mut c = acc.connectivity_mut(S3, G);
        let c = c.as_mut_slice();
        c.copy_from_slice(&[3, 4, 5, 0, 1, 2]);
    }
    assert_eq!(mesh.nb_element(S3, G), 2);
    fe.init_shape_functions(&mesh, G).unwrap();
    let field = affine_field(&fe, &mesh, Some(&[0, 1]));
    let (r, out) = run_into_sentinel(&fe, &mesh, &interp, &field, &[0, 1]);
    assert!(
        matches!(&r, Err(Error::InvalidArgument(m)) if m.contains("revision")),
        "{r:?}"
    );
    assert_eq!(out, [7.0, 7.0], "nothing written on error");
    // rebuilt with the targets of the renumbered elements
    let rebuilt = fe
        .init_elemental_field_interpolation(&mesh, &points(&mesh, &[3.5, 0.25]), None)
        .unwrap();
    let (r, out) = run_into_sentinel(&fe, &mesh, &rebuilt, &field, &[0, 1]);
    r.unwrap();
    assert!(close(out[0], 3.5) && close(out[1], 0.25), "{out:?}");
}

/// All precomputed arrays of `(ty, G)`, for unchanged-state comparisons.
fn caches(fe: &FeEngine, ty: ElementType) -> Vec<Vec<f64>> {
    vec![
        fe.shapes(ty, G).unwrap().as_slice().to_vec(),
        fe.shapes_derivatives(ty, G).unwrap().as_slice().to_vec(),
        fe.jacobians(ty, G).unwrap().as_slice().to_vec(),
    ]
}

fn numbering_of(blocks: &[(ElementType, &[Idx])], nb_component: usize) -> ElementTypeMapArray<Idx> {
    let mut m = ElementTypeMapArray::new("numbering");
    for &(ty, v) in blocks {
        m.as_map_mut()
            .insert(ty, G, Array::from_vec(v.to_vec(), nb_component).unwrap())
            .unwrap();
    }
    m
}

/// Removal numberings: three segment3 elements (`nq = 2`, six
/// cached tuples). `[0]` and `[0, -1]` divide the tuple count and were
/// accepted (the second truncated every cache to three tuples, not a whole
/// number of two-point blocks); `[0, 1, 2, 3, 4, 5]` matches the tuple
/// count. All must be rejected with nothing changed, as must a numbering
/// with several components and negative ids other than −1.
#[test]
fn fe2_02_wrong_numbering_lengths_and_shapes_are_rejected() {
    let mesh = segment3_mesh(1); // 3 elements
    let mut fe = engine(&mesh);
    let nq = fe.nb_integration_points(S3, G).unwrap();
    assert_eq!((nq, fe.jacobians(S3, G).unwrap().len()), (2, 6));
    let before = caches(&fe, S3);
    let bad_scalar: [&[Idx]; 5] = [
        &[0],                // divisor of the tuple count (6 / 1)
        &[0, -1],            // divisor (6 / 2): truncated to 3 tuples before the fix
        &[0, 1, 2, 3, 4, 5], // the tuple count, not the element count
        &[0, 1, 2, 3],       // not a divisor
        &[0, -2, 1],         // invalid negative id (removed elements are -1)
    ];
    for bad in bad_scalar {
        let r = fe.on_elements_removed(&numbering_of(&[(S3, bad)], 1));
        assert!(r.is_err(), "{bad:?} accepted");
        assert_eq!(caches(&fe, S3), before, "{bad:?} changed the caches");
    }
    // three entries, but as one tuple of three components
    let r = fe.on_elements_removed(&numbering_of(&[(S3, &[0, 1, 2])], 3));
    assert!(dimension_mismatch(r));
    // two tuples of three components (six entries, also the tuple count)
    let r = fe.on_elements_removed(&numbering_of(&[(S3, &[0, 1, 2, -1, -1, -1])], 3));
    assert!(dimension_mismatch(r));
    assert_eq!(caches(&fe, S3), before);
    // an empty numbering leaves the type unchanged (upstream `continue`)
    fe.on_elements_removed(&numbering_of(&[(S3, &[])], 1))
        .unwrap();
    assert_eq!(caches(&fe, S3), before);
    // a valid removal keeps whole blocks: kept · nq tuples (UD-103)
    fe.on_elements_removed(&numbering_of(&[(S3, &[1, -1, 0])], 1))
        .unwrap();
    let after = caches(&fe, S3);
    for (a, b) in after.iter().zip(&before) {
        let per = b.len() / 3; // values per element block
        assert_eq!(a.len(), 2 * per);
        assert_eq!(&a[..per], &b[2 * per..], "new 0 = old 2");
        assert_eq!(&a[per..], &b[..per], "new 1 = old 0");
    }
}

/// Every affected `(type, ghost)` block is validated before any is
/// modified — a valid segment2 numbering is not applied when the segment3
/// numbering of the same event is invalid (`[0]` for two two-point
/// segment3 elements: a divisor of the four cached tuples, accepted as one
/// four-tuple block before the fix while segment2 was renumbered).
#[test]
fn fe2_02_no_block_changes_when_another_block_is_invalid() {
    let mut mesh = segment3_mesh(0);
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        let mut c = acc.connectivity_mut(ElementType::Segment2, G);
        c.push_tuple(&[0, 1]);
        c.push_tuple(&[3, 4]);
    }
    let mut fe = engine(&mesh);
    let (s2, s3) = (caches(&fe, ElementType::Segment2), caches(&fe, S3));
    let ev = numbering_of(&[(ElementType::Segment2, &[-1, 0]), (S3, &[0])], 1);
    assert!(dimension_mismatch(fe.on_elements_removed(&ev)));
    assert_eq!(caches(&fe, ElementType::Segment2), s2);
    assert_eq!(caches(&fe, S3), s3);
    // both valid: both applied
    let ev = numbering_of(&[(ElementType::Segment2, &[-1, 0]), (S3, &[0, -1])], 1);
    fe.on_elements_removed(&ev).unwrap();
    assert_eq!(
        fe.jacobians(ElementType::Segment2, G).unwrap().len(),
        fe.nb_integration_points(ElementType::Segment2, G).unwrap()
    );
    assert_eq!(fe.jacobians(S3, G).unwrap().len(), 2);
}

/// Malformed storage: the segment3 connectivity of a valid
/// engine's mesh is replaced through the public accessor by a one-element,
/// two-component array; `compute_shapes_at` with a correctly sized output
/// panicked at `Block::node` (len 2, index 2). Also: node coordinates with
/// the wrong number of components.
#[test]
fn fe2_03_malformed_connectivity_is_an_error_in_checked_queries() {
    let mut mesh = segment3_mesh(0);
    let fe = engine(&mesh);
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        acc.connectivity_mut(S3, G)
            .replace(Array::from_vec(vec![0, 1], 2).unwrap());
    }
    let e = Element::new(S3, 0, G);
    let r = std::panic::catch_unwind(|| {
        let mut n = [7.0; 3];
        let r = fe.compute_shapes_at(&mesh, &[0.25], e, &mut n);
        (r, n)
    });
    let (r, n) = r.expect("must not panic");
    assert!(dimension_mismatch(r));
    assert_eq!(n, [7.0; 3], "nothing written on error");
    let mut d = [7.0; 3];
    assert!(dimension_mismatch(fe.compute_shape_derivatives_at(
        &mesh,
        &[0.25],
        e,
        &mut d
    )));
    let mut xi = [0.0];
    assert!(dimension_mismatch(fe.inverse_map(
        &mesh,
        &[0.25],
        e,
        &mut xi
    )));
    // node coordinates of the wrong width for a 1-D mesh
    let mut mesh = segment3_mesh(0);
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        acc.nodes_mut()
            .replace(Array::from_vec(vec![0.0; 6], 2).unwrap());
    }
    let mut n = [7.0; 3];
    let r = std::panic::catch_unwind(|| {
        let mut n = [7.0; 3];
        fe.compute_shapes_at(&mesh, &[0.25], e, &mut n)
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
    assert!(dimension_mismatch(fe.compute_shapes_at(
        &mesh,
        &[0.25],
        e,
        &mut n
    )));
    assert_eq!(n, [7.0; 3]);
}

/// Malformed storage: a 2-D mesh with the facet segment2
/// `(0,0)→(1,0)` and its dimension-1 engine; the connectivity is replaced
/// by `[0]` (one component). `compute_normals_for` panicked in
/// `Block::gather` (len 1, index 1).
#[test]
fn fe2_03_malformed_facet_connectivity_is_an_error_in_normals() {
    let s2 = ElementType::Segment2;
    let mut mesh = Mesh::new(2, "facet").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        acc.push_node(&[0.0, 0.0]).unwrap();
        acc.push_node(&[1.0, 0.0]).unwrap();
        acc.connectivity_mut(s2, G).push_tuple(&[0, 1]);
    }
    let fe = FeEngine::with_precomputation(
        &mesh,
        Some(1),
        ElementKind::Regular,
        ShapeFamily::Lagrange,
        "facets",
    )
    .unwrap();
    let mut normals = Array::new(2);
    fe.compute_normals_for(&mesh, mesh.nodes().view(), &mesh, &mut normals, s2, G)
        .unwrap();
    assert_eq!(normals.tuple(0), &[0.0, -1.0]);
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        acc.connectivity_mut(s2, G)
            .replace(Array::from_vec(vec![0], 1).unwrap());
    }
    let r = std::panic::catch_unwind(|| {
        let mut normals = Array::new(2);
        fe.compute_normals_for(&mesh, mesh.nodes().view(), &mesh, &mut normals, s2, G)
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
}

/// Normal kernel: `normals_on_natural::<Tetrahedron4>` with
/// `sdim = 4` (= natural dimension + 1, correctly sized buffers) sliced 12
/// values out of the 9-value tangent scratch.
#[test]
fn fe2_04_normal_kernel_rejects_dimensions_beyond_three() {
    let r = std::panic::catch_unwind(|| {
        kernels::normals_on_natural::<types::Tetrahedron4>(
            &[0.1; 3],
            &[0.0; 16],
            4,
            false,
            &mut [0.0; 4],
        )
    });
    assert!(dimension_mismatch(r.expect("must not panic")));
    // the same for every 3-D element type
    let r = kernels::normals_on_natural::<types::Hexahedron8>(
        &[0.1; 3],
        &[0.0; 32],
        4,
        true,
        &mut [0.0; 4],
    );
    assert!(dimension_mismatch(r));
    // supported dimensions still work: triangle facet in 3-D
    let x = [0.0, 0.0, 0.0, 1.0, 0.0, 0.0, 0.0, 1.0, 0.0];
    let mut n = [0.0; 3];
    kernels::normals_on_natural::<types::Triangle3>(&[0.25, 0.25], &x, 3, false, &mut n).unwrap();
    assert_eq!(n, [0.0, 0.0, 1.0]);
}

/// An absurd spatial dimension is rejected with an
/// error — the diagnostic is built without `sdim * sdim` overflowing (this
/// panicked in debug builds) — and the output is left untouched.
#[test]
fn gf2_01_huge_shape_derivative_dimension_is_an_error_not_a_panic() {
    use akantu_fe::{eigen::Dims, kernels};
    let n = 1usize << (usize::BITS / 2);
    let mut out = [7.0; 4];
    for rows in [4usize, n, usize::MAX] {
        let r = kernels::shape_derivatives(&[], &[], Dims::dynamic(rows, 0), &mut out);
        match r {
            Err(Error::DimensionMismatch { expected, got, .. }) => {
                assert_eq!((expected, got), (3, rows));
            }
            other => panic!("rows = {rows}: expected DimensionMismatch, got {other:?}"),
        }
        assert_eq!(out, [7.0; 4], "rows = {rows}: output untouched");
    }
    // An in-range dimension with a short Jacobian still reports the length.
    match kernels::shape_derivatives(&[0.0; 3], &[], Dims::dynamic(2, 3), &mut out) {
        Err(Error::DimensionMismatch { expected, got, .. }) => assert_eq!((expected, got), (4, 3)),
        other => panic!("expected DimensionMismatch, got {other:?}"),
    }
}

/// The public Jacobian / normal helpers
/// reject malformed sizes with an error instead of panicking.
#[test]
fn gf2_03_public_fe_helpers_reject_malformed_sizes() {
    use akantu_fe::kernels;
    for (j, nd, sdim) in [
        (&[][..], 2usize, 2usize),
        (&[1.0, 0.0, 0.0][..], 2, 2),
        (&[1.0; 5][..], 2, 3),
        (&[1.0; 16][..], 4, 4),
        (&[][..], usize::MAX, usize::MAX),
    ] {
        assert!(
            matches!(
                kernels::jacobian(j, nd, sdim),
                Err(Error::DimensionMismatch { .. })
            ),
            "jacobian(len {}, {nd}, {sdim})",
            j.len()
        );
    }
    assert_eq!(kernels::jacobian(&[2.0, 0.0, 0.0, 3.0], 2, 2).unwrap(), 6.0);
    assert_eq!(kernels::jacobian(&[], 0, 2).unwrap(), 0.0);
    let mut n = [9.0; 3];
    assert!(kernels::normal_from_tangents(&[1.0], 2, &mut n).is_err());
    assert!(kernels::normal_from_tangents(&[1.0; 5], 3, &mut n).is_err());
    assert!(kernels::normal_from_tangents(&[1.0; 6], 3, &mut n[..2]).is_err());
    assert_eq!(n, [9.0; 3], "nothing written on error");
    kernels::normal_from_tangents(&[1.0; 6], 4, &mut n).unwrap(); // untouched, as upstream
    assert_eq!(n, [9.0; 3]);
}
