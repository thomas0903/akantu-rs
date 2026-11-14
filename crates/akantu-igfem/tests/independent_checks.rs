// SPDX-License-Identifier: LGPL-3.0-or-later
// Independent mathematical checks of the IGFEM FE engine (decision D-007;
// no upstream counterpart). Ported semantics from Akantu (EPFL/LSMS):
// extra_packages/igfem/src/shape_igfem_inline_impl.hh, integrator_gauss_igfem_inline_impl.hh.

//! IGFEM-001/002 checks without an oracle (the package does not compile
//! upstream): partition of unity, reproduction of fields with a kink
//! along the interface (values and gradients), enrichments vanishing at the
//! parent nodes, recovery of physical values at enriched nodes, and
//! quadrature exactness on the sub-elements. Meshes: structured
//! triangulations cut by straight lines with an independent generator
//! (`common/cut.rs`), plus upstream's two test meshes.

mod common;

use akantu_core::{Array, Element, ElementType, GhostType, Idx, Real};
use akantu_igfem::IgfemFeEngine;
use akantu_igfem::element;
use akantu_mesh::Mesh;
use common::cut::{Line, barycentric, cut, polygon_area, structured};

/// Mesh size of the structured meshes of [`cut_meshes`].
const H: Real = 1.0 / 3.0;

const IGFEM_TYPES: [ElementType; 2] = [ElementType::IgfemTriangle4, ElementType::IgfemTriangle5];

/// Several cut meshes: lines in general position (tri5 only) and lines
/// through mesh nodes (tri4 and tri5).
fn cut_meshes() -> Vec<(String, common::cut::CutMesh, Line)> {
    let mut out = Vec::new();
    let (nodes, tris) = structured(6, -1.0, 1.0, -1.0, 1.0);
    for (name, line) in [
        (
            "x=0.137",
            Line {
                a: 1.0,
                b: 0.0,
                c: 0.137,
            },
        ),
        (
            "tilted",
            Line {
                a: 0.83,
                b: 0.31,
                c: -0.123,
            },
        ),
        (
            "steep",
            Line {
                a: -0.2,
                b: 1.7,
                c: 0.4142,
            },
        ),
        // through nodes: the diagonal x = y passes through every (i, i) node
        (
            "diag+eps",
            Line {
                a: 1.0,
                b: -1.0,
                c: 0.05,
            },
        ),
        (
            "x=1/3 (nodes)",
            Line {
                a: 1.0,
                b: 0.0,
                c: 1.0 / 3.0,
            },
        ),
        (
            "x+2y=1/3",
            Line {
                a: 1.0,
                b: 2.0,
                c: 1.0 / 3.0,
            },
        ),
    ] {
        out.push((name.to_owned(), cut(&nodes, &tris, &line, 1e-12), line));
    }
    out
}

fn coords(mesh: &Mesh, el: Element) -> Vec<Real> {
    mesh.extract_nodal_values_from_element(&mesh.nodes(), el)
        .unwrap()
}

fn pt(x: &[Real], n: usize) -> [Real; 2] {
    [x[2 * n], x[2 * n + 1]]
}

#[test]
fn cut_meshes_contain_both_igfem_types() {
    let mut n4 = 0;
    let mut n5 = 0;
    for (_, cm, _) in cut_meshes() {
        n4 += cm
            .mesh
            .nb_element(ElementType::IgfemTriangle4, GhostType::NotGhost);
        n5 += cm
            .mesh
            .nb_element(ElementType::IgfemTriangle5, GhostType::NotGhost);
    }
    assert!(n4 > 0 && n5 > 0, "{n4} {n5}");
}

/// Σ of the parent shapes is 1 at every integration point, the enrichment
/// values lie in `[0, 1]`, and the parent interpolation of the node
/// coordinates is the physical point (|·| ≤ 4 ε: one rounding per term of
/// a 3-term sum of O(1) values).
#[test]
fn partition_of_unity_and_parent_consistency() {
    for (name, cm, _) in cut_meshes() {
        let mesh = &cm.mesh;
        let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
        for ty in IGFEM_TYPES {
            if mesh.nb_element(ty, GhostType::NotGhost) == 0 {
                continue;
            }
            let np = element::nb_parent_nodes(ty).unwrap();
            let nn = ty.nb_nodes_per_interpolation_element();
            let shapes = fe.shapes(mesh, ty, GhostType::NotGhost).unwrap();
            let mut qc = Array::new(2);
            fe.integration_points_coordinates(mesh, &mut qc, ty, GhostType::NotGhost, None)
                .unwrap();
            let nq = fe.nb_integration_points(ty).unwrap();
            for (t, n) in shapes.tuples().enumerate() {
                let sum: Real = n[..np].iter().sum();
                assert!(
                    (sum - 1.0).abs() <= 4.0 * Real::EPSILON,
                    "{name} {ty}: Σ N = {sum}"
                );
                for &v in &n[np..nn] {
                    assert!(
                        (-1e-15..=1.0 + 1e-15).contains(&v),
                        "{name} {ty}: enrichment {v}"
                    );
                }
                let x = coords(
                    mesh,
                    Element::try_new(ty, t / nq, GhostType::NotGhost).unwrap(),
                );
                for d in 0..2 {
                    let interp: Real = (0..np).map(|i| n[i] * x[2 * i + d]).sum();
                    assert!(
                        (interp - qc.tuple(t)[d]).abs() <= 4.0 * Real::EPSILON,
                        "{name} {ty}: parent interpolation {interp} vs {}",
                        qc.tuple(t)[d]
                    );
                }
            }
        }
    }
}

/// A field that is linear on each side of the interface (kink along the
/// line) is reproduced exactly at the integration points (values and
/// gradients), and the enriched-node recovery gives its physical values.
/// Enriched values: `u_e = u(x_e) − Σ N_i(x_e) u_i` (barycentric coordinates
/// computed independently). Tolerance `1e-13`: O(1) data, a few dozen
/// floating-point operations including the inverse maps (Gauss–Newton to
/// machine precision).
#[test]
fn reproduction_of_kinked_fields() {
    for (name, cm, line) in cut_meshes() {
        let mesh = &cm.mesh;
        let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
        let (c0, c1, c2, c3) = (0.7, -1.3, 2.1, 3.7);
        // u = c0 + c1 x + c2 y + c3 |φ|, ∇u = (c1, c2) ± c3 (a, b)
        let u_exact = |x: &[Real]| c0 + c1 * x[0] + c2 * x[1] + c3 * line.phi(x).abs();
        let grad_exact = |x: &[Real]| {
            let s = line.phi(x).signum();
            [c1 + c3 * s * line.a, c2 + c3 * s * line.b]
        };
        let nb_nodes = mesh.nb_nodes();
        let mut u = Array::zeros(nb_nodes, 1);
        for n in 0..cm.nb_standard_nodes {
            u.tuple_mut(n)[0] = u_exact(mesh.nodes().tuple(n));
        }
        // enriched values from any element containing the enriched node
        let nodes = mesh.nodes();
        for ty in IGFEM_TYPES {
            let np = element::nb_parent_nodes(ty).unwrap();
            let conn_g = mesh.connectivity(ty, GhostType::NotGhost).ok();
            let conn: &[Idx] = conn_g.as_ref().map_or(&[][..], |c| c.as_slice());
            for row in conn.chunks(ty.nb_nodes_per_element()) {
                let p: [[Real; 2]; 3] = std::array::from_fn(|i| {
                    let x = nodes.tuple(row[i] as usize);
                    [x[0], x[1]]
                });
                for &en in &row[np..] {
                    let xe = nodes.tuple(en as usize);
                    let l = barycentric(p, [xe[0], xe[1]]);
                    let parent: Real = (0..3).map(|i| l[i] * u.tuple(row[i] as usize)[0]).sum();
                    u.tuple_mut(en as usize)[0] = u_exact(xe) - parent;
                }
            }
        }
        for ty in IGFEM_TYPES {
            if mesh.nb_element(ty, GhostType::NotGhost) == 0 {
                continue;
            }
            let mut uq = Array::new(1);
            fe.interpolate_on_integration_points(
                mesh,
                u.view(),
                &mut uq,
                1,
                ty,
                GhostType::NotGhost,
                None,
            )
            .unwrap();
            let mut gq = Array::new(2);
            fe.gradient_on_integration_points(
                mesh,
                u.view(),
                &mut gq,
                1,
                ty,
                GhostType::NotGhost,
                None,
            )
            .unwrap();
            let mut qc = Array::new(2);
            fe.integration_points_coordinates(mesh, &mut qc, ty, GhostType::NotGhost, None)
                .unwrap();
            for t in 0..uq.len() {
                let x = qc.tuple(t);
                let e = u_exact(x);
                assert!(
                    (uq.tuple(t)[0] - e).abs() <= 1e-13,
                    "{name} {ty}: u {} vs {e}",
                    uq.tuple(t)[0]
                );
                let g = grad_exact(x);
                for d in 0..2 {
                    assert!(
                        (gq.tuple(t)[d] - g[d]).abs() <= 1e-12,
                        "{name} {ty}: ∇u {:?} vs {g:?}",
                        gq.tuple(t)
                    );
                }
            }
            // physical values at the enriched nodes
            let mut real = Array::zeros(nb_nodes, 1);
            fe.interpolate_at_enriched_nodes(mesh, &u, &mut real, ty, GhostType::NotGhost)
                .unwrap();
            let np = element::nb_parent_nodes(ty).unwrap();
            let conn = mesh.connectivity(ty, GhostType::NotGhost).unwrap();
            for row in conn.tuples() {
                for &en in &row[np..] {
                    let xe = nodes.tuple(en as usize);
                    assert!(
                        (real.tuple(en as usize)[0] - u_exact(xe)).abs() <= 1e-13,
                        "{name} {ty}"
                    );
                }
            }
        }
    }
}

/// The enrichment functions vanish at the parent nodes and equal `δ` at the
/// enriched nodes; the parent shapes are `δ` at the parent nodes
/// (`compute_shapes_at` at the nodes, the 1e-14 sub-element location test).
/// Queried at the enriched nodes (the case upstream uses, in
/// `computeValuesOnEnrichedNodes`) and at the parent nodes that are
/// corners of a triangular sub-element: a quadrangle sub-element's corner
/// may be rejected by upstream's square containment test (`|ξ| ≤ 1 +
/// DBL_EPSILON`, no `Math::tolerance`) after a Gauss–Newton inverse map
/// that lands a few ulps outside — upstream then fails its "not contained
/// in any element" assertion as well. Tolerance `1e-14`: inverse map of a
/// node of the element.
#[test]
fn shapes_are_kronecker_at_nodes() {
    let mut checked = 0;
    for (name, cm, _) in cut_meshes() {
        let mesh = &cm.mesh;
        let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
        for ty in IGFEM_TYPES {
            let nn = ty.nb_nodes_per_interpolation_element();
            let np = element::nb_parent_nodes(ty).unwrap();
            // nodes of the triangular sub-element 0
            let tri_nodes = element::sub_element_connectivity(ty, 0).unwrap();
            for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
                let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
                let x = coords(mesh, el);
                for k in 0..nn {
                    if k < np && !tri_nodes.contains(&u8::try_from(k).unwrap()) {
                        continue;
                    }
                    let mut n = vec![0.0; nn];
                    fe.compute_shapes_at(mesh, &x[2 * k..2 * k + 2], el, &mut n)
                        .unwrap();
                    for (j, v) in n.iter().enumerate() {
                        let expected = if j == k { 1.0 } else { 0.0 };
                        // a parent node: parent shapes δ, enrichments 0;
                        // an enriched node: its enrichment 1, the others 0
                        // (its parent shapes are its barycentric coordinates)
                        if k < np || j >= np {
                            assert!(
                                (v - expected).abs() <= 1e-14,
                                "{name} {ty} e{e} node {k}: N[{j}] = {v}"
                            );
                        }
                    }
                    checked += 1;
                }
            }
        }
    }
    assert!(checked > 100, "{checked}");
}

/// `compute_shapes_at` at the integration-point coordinates reproduces the
/// precomputed shapes (two independent code paths: precomputation maps the
/// sub-element rule points, `compute_shapes_at` inverse-maps the physical
/// point into the parent and the containing sub-element). Parent shapes
/// and triangle sub-elements: `1e-14` (one exact Gauss–Newton step of an
/// affine map). Quadrangle sub-elements: the inverse map stops at
/// upstream's tolerance `1e-10` on the physical residual, so the natural
/// coordinates are only known to `1e-10 / σ_min(J)` (`σ_min` the smallest
/// singular value of the sub-element Jacobian at the point) and the
/// bilinear shapes (|∂N/∂ξ| ≤ 1/2 per direction) to `2 · 1e-10 / σ_min`.
#[test]
fn shapes_at_points_match_precomputed_shapes() {
    for (name, cm, _) in cut_meshes() {
        let mesh = &cm.mesh;
        let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
        for ty in IGFEM_TYPES {
            if mesh.nb_element(ty, GhostType::NotGhost) == 0 {
                continue;
            }
            let nn = ty.nb_nodes_per_interpolation_element();
            let np = element::nb_parent_nodes(ty).unwrap();
            let nq = fe.nb_integration_points(ty).unwrap();
            let block = fe.block(mesh, ty, GhostType::NotGhost).unwrap();
            let n1 = block.sub_rules[0].nb_points();
            let quad_sub = element::sub_element_type(ty, 1).unwrap() == ElementType::Quadrangle4;
            let shapes = fe.shapes(mesh, ty, GhostType::NotGhost).unwrap();
            let mut qc = Array::new(2);
            fe.integration_points_coordinates(mesh, &mut qc, ty, GhostType::NotGhost, None)
                .unwrap();
            for t in 0..qc.len() {
                let (e, q) = (t / nq, t % nq);
                let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
                let mut n = vec![0.0; nn];
                fe.compute_shapes_at(mesh, qc.tuple(t), el, &mut n).unwrap();
                let tol_enr = if quad_sub && q >= n1 {
                    let x = coords(mesh, el);
                    let xi = block.points.as_slice()[2 * q..2 * q + 2].to_vec();
                    2.0 * 1e-10
                        / quad_sigma_min(&x, element::sub_element_connectivity(ty, 1).unwrap(), &xi)
                } else {
                    1e-14
                };
                for (j, (a, b)) in n.iter().zip(shapes.tuple(t)).enumerate() {
                    let tol = if j < np { 1e-14 } else { tol_enr };
                    assert!(
                        (a - b).abs() <= tol,
                        "{name} {ty} point {t}: {n:?} vs {:?} (tol {tol:e})",
                        shapes.tuple(t)
                    );
                }
                assert!(fe.contains(mesh, qc.tuple(t), el).unwrap());
            }
        }
    }
}

/// Smallest singular value of the bilinear map's Jacobian at `xi` of the
/// quadrangle with nodes `conn` (local) of the element coordinates `x`.
fn quad_sigma_min(x: &[Real], conn: &[u8], xi: &[Real]) -> Real {
    let (s, t) = (xi[0], xi[1]);
    // dN/dξ, dN/dη of the bilinear quadrangle (nodes (-1,-1),(1,-1),(1,1),(-1,1))
    let dns = [
        -(1.0 - t) / 4.0,
        (1.0 - t) / 4.0,
        (1.0 + t) / 4.0,
        -(1.0 + t) / 4.0,
    ];
    let dnt = [
        -(1.0 - s) / 4.0,
        -(1.0 + s) / 4.0,
        (1.0 + s) / 4.0,
        (1.0 - s) / 4.0,
    ];
    let mut j = [0.0; 4];
    for (i, &k) in conn.iter().enumerate() {
        let p = pt(x, usize::from(k));
        j[0] += dns[i] * p[0];
        j[1] += dns[i] * p[1];
        j[2] += dnt[i] * p[0];
        j[3] += dnt[i] * p[1];
    }
    // σ_min² = (tr − sqrt(tr² − 4 det²)) / 2 of JᵀJ
    let tr = j[0] * j[0] + j[1] * j[1] + j[2] * j[2] + j[3] * j[3];
    let det = j[0] * j[3] - j[1] * j[2];
    ((tr - (tr * tr - 4.0 * det * det).max(0.0).sqrt()) / 2.0).sqrt()
}

/// Quadrature on the sub-elements: the weights × Jacobians of the points of
/// each sub-element sum to its area (shoelace formula), and a linear field
/// integrates exactly (`∫ x = area · x̄` per sub-element, `x̄` the
/// polygon centroid). Tolerance `32 ε h²` absolute (`h = 1/3` the mesh
/// size): each `det J · w` is a 2 × 2 determinant of entries `O(h)` (a
/// few rounded products and one cancelling difference) and the
/// quadrangle's area is a sum of four of them; the shoelace reference has
/// the same order of error.
#[test]
fn sub_element_quadrature_exactness() {
    for (name, cm, _) in cut_meshes() {
        let mesh = &cm.mesh;
        let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
        let mut total = 0.0;
        for ty in IGFEM_TYPES {
            if mesh.nb_element(ty, GhostType::NotGhost) == 0 {
                continue;
            }
            let nq = fe.nb_integration_points(ty).unwrap();
            let n1 = fe.block(mesh, ty, GhostType::NotGhost).unwrap().sub_rules[0].nb_points();
            let jac = fe.jacobians(mesh, ty, GhostType::NotGhost).unwrap();
            let mut qc = Array::new(2);
            fe.integration_points_coordinates(mesh, &mut qc, ty, GhostType::NotGhost, None)
                .unwrap();
            for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
                let x = coords(mesh, Element::try_new(ty, e, GhostType::NotGhost).unwrap());
                for s in 0..2 {
                    let conn = element::sub_element_connectivity(ty, s).unwrap();
                    let poly: Vec<[Real; 2]> =
                        conn.iter().map(|&k| pt(&x, usize::from(k))).collect();
                    let area = polygon_area(&poly);
                    assert!(
                        area > 0.0,
                        "{name} {ty}: sub-element {s} not counter-clockwise"
                    );
                    let range = if s == 0 { 0..n1 } else { n1..nq };
                    let a: Real = range.clone().map(|q| jac.as_slice()[e * nq + q]).sum();
                    assert!(
                        (a - area).abs() <= 32.0 * Real::EPSILON * H * H,
                        "{name} {ty} e{e} s{s}: {a} vs {area}"
                    );
                    // ∫ x over the sub-element: exact for a linear integrand
                    let ix: Real = range
                        .map(|q| jac.as_slice()[e * nq + q] * qc.tuple(e * nq + q)[0])
                        .sum();
                    let cx = polygon_centroid_x(&poly);
                    assert!(
                        (ix - area * cx).abs() <= 32.0 * Real::EPSILON * H * H,
                        "{name} {ty}: ∫x {ix} vs {}",
                        area * cx
                    );
                    total += a;
                }
            }
        }
        // add the regular triangles: the cut mesh covers [-1, 1]²
        let t3 = mesh.nb_element(ElementType::Triangle3, GhostType::NotGhost);
        for e in 0..t3 {
            let x = coords(
                mesh,
                Element::try_new(ElementType::Triangle3, e, GhostType::NotGhost).unwrap(),
            );
            total += polygon_area(&[pt(&x, 0), pt(&x, 1), pt(&x, 2)]);
        }
        assert!((total - 4.0).abs() <= 1e-13, "{name}: area {total}");
    }
}

fn polygon_centroid_x(p: &[[Real; 2]]) -> Real {
    let a = polygon_area(p);
    let mut cx = 0.0;
    for i in 0..p.len() {
        let (x0, x1) = (p[i], p[(i + 1) % p.len()]);
        cx += (x0[0] + x1[0]) * (x0[0] * x1[1] - x1[0] * x0[1]);
    }
    cx / (6.0 * a)
}

/// `integrate_scalar` (sorted pairwise sum), `integrate_element` and
/// `integrate_on_integration_points` agree with the per-element integrals
/// (UD-604: every point is scaled).
#[test]
fn integration_variants_agree() {
    let (_, cm, _) = cut_meshes().into_iter().nth(1).unwrap();
    let mesh = &cm.mesh;
    let fe = IgfemFeEngine::with_precomputation(mesh, Some(2), "fe").unwrap();
    for ty in IGFEM_TYPES {
        let ne = mesh.nb_element(ty, GhostType::NotGhost);
        if ne == 0 {
            continue;
        }
        let nq = fe.nb_integration_points(ty).unwrap();
        let f: Vec<Real> = (0..ne * nq).map(|i| 1.0 + (i as Real) * 0.01).collect();
        let fa = Array::from_vec(f.clone(), 1).unwrap();
        let mut per = Array::new(1);
        fe.integrate(mesh, fa.view(), &mut per, 1, ty, GhostType::NotGhost, None)
            .unwrap();
        for e in 0..ne {
            let v = fe
                .integrate_element(mesh, &f[e * nq..(e + 1) * nq], ty, e, GhostType::NotGhost)
                .unwrap();
            assert!((v - per.as_slice()[e]).abs() <= 1e-16);
        }
        let mut onq = Array::new(1);
        fe.integrate_on_integration_points(
            mesh,
            fa.view(),
            &mut onq,
            1,
            ty,
            GhostType::NotGhost,
            None,
        )
        .unwrap();
        let jac = fe.jacobians(mesh, ty, GhostType::NotGhost).unwrap();
        for ((o, fv), j) in onq.as_slice().iter().zip(&f).zip(jac.as_slice()) {
            assert_eq!(*o, fv * j);
        }
        let mut copy = per.as_slice().to_vec();
        let total = fe
            .integrate_scalar(mesh, fa.view(), ty, GhostType::NotGhost, None)
            .unwrap();
        assert_eq!(total, akantu_core::math::reduce(&mut copy));
        // filtered integration
        let filter: Vec<akantu_core::Idx> = vec![0];
        let mut one = Array::new(1);
        fe.integrate(
            mesh,
            Array::from_vec(f[..nq].to_vec(), 1).unwrap().view(),
            &mut one,
            1,
            ty,
            GhostType::NotGhost,
            Some(&filter),
        )
        .unwrap();
        assert_eq!(one.as_slice()[0], per.as_slice()[0]);
    }
}

/// Stale precomputed data is refused after a mesh change, and the engine
/// works again after `init_shape_functions`.
#[test]
fn stale_data_is_refused() {
    let (_, mut cm, _) = cut_meshes().into_iter().next().unwrap();
    let mut fe = IgfemFeEngine::with_precomputation(&cm.mesh, Some(2), "fe").unwrap();
    let ty = ElementType::IgfemTriangle5;
    assert!(fe.shapes(&cm.mesh, ty, GhostType::NotGhost).is_ok());
    {
        let _acc = akantu_mesh::MeshAccessor::new(&mut cm.mesh);
    }
    assert!(fe.shapes(&cm.mesh, ty, GhostType::NotGhost).is_err());
    fe.init_shape_functions(&cm.mesh, GhostType::NotGhost)
        .unwrap();
    assert!(fe.shapes(&cm.mesh, ty, GhostType::NotGhost).is_ok());
}

/// `_igfem_segment_3` (ELEM-024) in a 1-D mesh: a field linear on each side
/// of an interface point `x_c` is reproduced exactly (values and
/// gradients), Σ of the parent shapes is 1, and the Jacobians of the two
/// sub-segments sum to their lengths. Mesh: `[0, 1]` in 5 segments, the
/// third one cut at `x_c = 0.5 + 0.0371` (enriched node appended).
#[test]
fn igfem_segment_3_reproduction() {
    let xc = 0.5371;
    let mut mesh = Mesh::new(1, "seg").unwrap();
    {
        let mut acc = akantu_mesh::MeshAccessor::new(&mut mesh);
        for i in 0..=5 {
            acc.push_node(&[f64::from(i) / 5.0]).unwrap();
        }
        let enr = acc.push_node(&[xc]).unwrap();
        let mut c2 = acc.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
        for e in [0, 1, 3, 4] {
            c2.push_tuple(&[e, e + 1]);
        }
        drop(c2);
        acc.connectivity_mut(ElementType::IgfemSegment3, GhostType::NotGhost)
            .push_tuple(&[2, 3, enr]);
    }
    let fe = IgfemFeEngine::with_precomputation(&mesh, Some(1), "fe").unwrap();
    let ty = ElementType::IgfemSegment3;
    assert_eq!(fe.nb_integration_points(ty).unwrap(), 2);
    let (s0, s1) = (2.0, -1.5);
    let u_exact = |x: Real| {
        if x < xc {
            0.3 + s0 * (x - xc)
        } else {
            0.3 + s1 * (x - xc)
        }
    };
    let mut u = Array::zeros(mesh.nb_nodes(), 1);
    for n in 0..6 {
        u.tuple_mut(n)[0] = u_exact(mesh.nodes().tuple(n)[0]);
    }
    // enriched value: physical value minus the parent interpolation
    let (x2, x3) = (0.4, 0.6);
    let parent = u.tuple(2)[0] * (x3 - xc) / (x3 - x2) + u.tuple(3)[0] * (xc - x2) / (x3 - x2);
    u.tuple_mut(6)[0] = u_exact(xc) - parent;
    let mut uq = Array::new(1);
    fe.interpolate_on_integration_points(
        &mesh,
        u.view(),
        &mut uq,
        1,
        ty,
        GhostType::NotGhost,
        None,
    )
    .unwrap();
    let mut gq = Array::new(1);
    fe.gradient_on_integration_points(&mesh, u.view(), &mut gq, 1, ty, GhostType::NotGhost, None)
        .unwrap();
    let mut qc = Array::new(1);
    fe.integration_points_coordinates(&mesh, &mut qc, ty, GhostType::NotGhost, None)
        .unwrap();
    for q in 0..2 {
        let x = qc.tuple(q)[0];
        assert!((uq.tuple(q)[0] - u_exact(x)).abs() <= 1e-15, "{x}");
        let g = if x < xc { s0 } else { s1 };
        assert!(
            (gq.tuple(q)[0] - g).abs() <= 1e-13,
            "{x}: {} vs {g}",
            gq.tuple(q)[0]
        );
    }
    // sub-segment midpoints and lengths
    assert!((qc.tuple(0)[0] - (x2 + xc) / 2.0).abs() <= 1e-16);
    assert!((qc.tuple(1)[0] - (xc + x3) / 2.0).abs() <= 1e-16);
    let jac = fe.jacobians(&mesh, ty, GhostType::NotGhost).unwrap();
    assert!((jac.as_slice()[0] - (xc - x2)).abs() <= 1e-16);
    assert!((jac.as_slice()[1] - (x3 - xc)).abs() <= 1e-16);
    for n in fe.shapes(&mesh, ty, GhostType::NotGhost).unwrap().tuples() {
        assert!((n[0] + n[1] - 1.0).abs() <= 2.0 * Real::EPSILON);
    }
}
