// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/test/test_geometry/
// test_generation_igfem_mesh_from_sphere_static.cc,
// test_generation_igfem_mesh_from_sphere.cc. Built only with the feature
// `cgal` (the growing-gel mesh generation links the GPL-3.0-or-later crate
// akantu-geometry-cgal, decision D-042).

//! TEST-198 / TEST-199: IGFEM mesh generation from spheres (element counts
//! after each pass, as upstream asserts) plus geometric invariants checked
//! independently: enriched nodes on the spheres and on the parent edges,
//! positive sub-element areas, conservation of the meshed area, node
//! numbering after the clean-up of a moving interface.

mod common;

use akantu_core::{Element, ElementType, GhostType, Real};
use akantu_geometry_cgal::SkSphere;
use akantu_igfem::element;
use akantu_igfem::growing_gel::{GrowingGelPass, MeshIgfemSphericalGrowingGel};
use akantu_mesh::{Mesh, SharedMesh};
use common::cut::polygon_area;
use common::read_msh;

fn counts(mesh: &Mesh) -> [usize; 3] {
    [
        mesh.nb_element(ElementType::Triangle3, GhostType::NotGhost),
        mesh.nb_element(ElementType::IgfemTriangle4, GhostType::NotGhost),
        mesh.nb_element(ElementType::IgfemTriangle5, GhostType::NotGhost),
    ]
}

fn build(
    shared: &SharedMesh,
    gel: &mut MeshIgfemSphericalGrowingGel,
    spheres: &[SkSphere],
) -> GrowingGelPass {
    let mut pass = GrowingGelPass::default();
    shared
        .mutate(
            &mut [],
            |tx| {
                pass = gel.build_igfem_mesh_from_spheres(tx, spheres)?;
                Ok(())
            },
            None,
        )
        .unwrap();
    pass
}

fn sphere(c: [Real; 2], r2: Real) -> SkSphere {
    SkSphere::try_from_f64([c[0], c[1], 0.0], r2).unwrap()
}

/// Invariants of a generated IGFEM mesh (spheres as `(centre, r²)`):
/// * every enriched node is on one of the spheres (`| |x − c|² − r² | ≤
///   8 ε r²`: the exact intersection point rounded to doubles, squared
///   distance with three roundings);
/// * it lies on the parent edge it cuts (barycentric coordinates in
///   `[−1e-15, 1 + 1e-15]` and the third one `|·| ≤ 1e-15`);
/// * every sub-element is counter-clockwise; the sub-element areas sum to
///   the parent area (`1e-15` relative);
/// * the total area is `expected_area`.
fn check_invariants(mesh: &Mesh, spheres: &[([Real; 2], Real)], expected_area: Real) {
    let mut total = 0.0;
    for ty in [
        ElementType::Triangle3,
        ElementType::IgfemTriangle4,
        ElementType::IgfemTriangle5,
    ] {
        for e in 0..mesh.nb_element(ty, GhostType::NotGhost) {
            let el = Element::try_new(ty, e, GhostType::NotGhost).unwrap();
            let x = mesh
                .extract_nodal_values_from_element(&mesh.nodes(), el)
                .unwrap();
            let p = |k: usize| [x[2 * k], x[2 * k + 1]];
            let parent_area = polygon_area(&[p(0), p(1), p(2)]);
            assert!(parent_area > 0.0, "{ty} {e}: clockwise parent");
            total += parent_area;
            if ty == ElementType::Triangle3 {
                continue;
            }
            let np = element::nb_parent_nodes(ty).unwrap();
            let mut sub_sum = 0.0;
            for s in 0..2 {
                let poly: Vec<[Real; 2]> = element::sub_element_connectivity(ty, s)
                    .unwrap()
                    .iter()
                    .map(|&k| p(usize::from(k)))
                    .collect();
                let a = polygon_area(&poly);
                assert!(
                    a > 0.0,
                    "{ty} {e}: sub-element {s} not counter-clockwise ({a})"
                );
                sub_sum += a;
            }
            assert!(
                (sub_sum - parent_area).abs() <= 1e-15 * parent_area.max(1.0),
                "{ty} {e}"
            );
            for k in np..ty.nb_nodes_per_element() {
                let xe = p(k);
                let on_sphere = spheres.iter().any(|(c, r2)| {
                    let d2 = (xe[0] - c[0]).powi(2) + (xe[1] - c[1]).powi(2);
                    (d2 - r2).abs() <= 8.0 * Real::EPSILON * r2.max(1.0)
                });
                assert!(on_sphere, "{ty} {e}: enriched node {xe:?} is on no sphere");
                // on one of the parent edges
                let on_edge = (0..3).any(|i| {
                    let (a, b) = (p(i), p((i + 1) % 3));
                    let cross = (b[0] - a[0]) * (xe[1] - a[1]) - (b[1] - a[1]) * (xe[0] - a[0]);
                    let t = ((xe[0] - a[0]) * (b[0] - a[0]) + (xe[1] - a[1]) * (b[1] - a[1]))
                        / ((b[0] - a[0]).powi(2) + (b[1] - a[1]).powi(2));
                    cross.abs() <= 1e-15 && (-1e-15..=1.0 + 1e-15).contains(&t)
                });
                assert!(
                    on_edge,
                    "{ty} {e}: enriched node {xe:?} not on a parent edge"
                );
            }
        }
    }
    assert!(
        (total - expected_area).abs() <= 1e-14,
        "area {total} vs {expected_area}"
    );
}

/// `test_generation_igfem_mesh_from_sphere_static.cc`: two spheres on
/// `test_geometry_triangle.msh` give 0 `_triangle_3`, 1 `_igfem_triangle_4`
/// and 1 `_igfem_triangle_5`.
#[test]
fn test_generation_igfem_mesh_from_sphere_static() {
    let mesh = read_msh(2, "test_geometry_triangle.msh", "mesh");
    let shared = SharedMesh::new(mesh);
    let mut gel = MeshIgfemSphericalGrowingGel::new(&shared.read().unwrap()).unwrap();
    gel.init(&shared.read().unwrap()).unwrap();
    let spheres = [
        sphere([0.0, 1.0], 0.2 * 0.2),
        sphere([1.0, 0.0], 0.4999999999),
    ];
    let pass = build(&shared, &mut gel, &spheres);
    let m = shared.read().unwrap();
    assert_eq!(counts(&m), [0, 1, 1], "{pass:?}");
    check_invariants(
        &m,
        &[([0.0, 1.0], 0.2 * 0.2), ([1.0, 0.0], 0.4999999999)],
        0.5,
    );
    assert_eq!(m.nb_nodes(), 4 + pass.new_nodes);
}

/// `test_generation_igfem_mesh_from_sphere.cc`: a growing sphere (radius
/// factors 1, 1.5, 2, 2.1, 3.5 of `r² = 0.4999999999 / 4`) moves the
/// interface through the two triangles; element counts after each pass as
/// upstream asserts. Each pass removes the previous enriched nodes, so the
/// node count is always `4 + new nodes of the pass` and the enriched nodes
/// follow the standard ones.
#[test]
fn test_generation_igfem_mesh_from_sphere() {
    let mesh = read_msh(2, "test_geometry_triangle.msh", "mesh_gel2");
    let shared = SharedMesh::new(mesh);
    let mut gel = MeshIgfemSphericalGrowingGel::new(&shared.read().unwrap()).unwrap();
    gel.init(&shared.read().unwrap()).unwrap();
    let r2 = 0.4999999999 / 4.0;
    let base = [sphere([1.0, 0.0], r2)];
    let expected: [(Real, [usize; 3]); 5] = [
        (1.0, [1, 0, 1]),
        (1.5, [1, 0, 1]),
        (2.0, [1, 1, 0]),
        (2.1, [0, 0, 2]),
        (3.5, [1, 0, 1]),
    ];
    for (k, (fact, want)) in expected.into_iter().enumerate() {
        let spheres = if k == 0 {
            base.to_vec()
        } else {
            MeshIgfemSphericalGrowingGel::inflated(&base, fact).unwrap()
        };
        let pass = build(&shared, &mut gel, &spheres);
        let m = shared.read().unwrap();
        assert_eq!(counts(&m), want, "factor {fact}: {pass:?}");
        assert_eq!(m.nb_nodes(), 4 + pass.new_nodes, "factor {fact}");
        // r² · f · f with the doubles of the exact Quotient product
        let r2f = r2 * fact * fact;
        check_invariants(&m, &[([1.0, 0.0], r2f)], 0.5);
        // every node is referenced (old enriched nodes were removed)
        let mut used = vec![false; m.nb_nodes()];
        for ty in [
            ElementType::Triangle3,
            ElementType::IgfemTriangle4,
            ElementType::IgfemTriangle5,
        ] {
            if let Ok(c) = m.connectivity(ty, GhostType::NotGhost) {
                for &n in c.as_slice() {
                    used[usize::try_from(n).unwrap()] = true;
                }
            }
        }
        assert!(used.iter().all(|&u| u), "factor {fact}: unreferenced node");
    }
}

/// The intersection factor is applied exactly (`squared_radius · f · f` in
/// `Quotient<MP_Float>`, upstream `computeMeshQueryListIntersectionPoint(list,
/// inf_fact)`).
#[test]
fn inflation_is_exact() {
    let s = sphere([0.25, 0.5], 0.1);
    let i = MeshIgfemSphericalGrowingGel::inflated(std::slice::from_ref(&s), 1.5).unwrap();
    let expected = s.squared_radius.mul_f64(1.5).unwrap().mul_f64(1.5).unwrap();
    assert!(i[0].squared_radius.eq_value(&expected).unwrap());
}

/// A mesh with an unsupported element type of the mesh dimension is
/// refused at `init` (upstream "Not ready for mesh type").
#[test]
fn unsupported_types_are_refused() {
    let mut mesh = Mesh::new(2, "q").unwrap();
    {
        let mut acc = akantu_mesh::MeshAccessor::new(&mut mesh);
        for p in [[0.0, 0.0], [1.0, 0.0], [1.0, 1.0], [0.0, 1.0]] {
            acc.push_node(&p).unwrap();
        }
        acc.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost)
            .push_tuple(&[0, 1, 2, 3]);
    }
    let mut gel = MeshIgfemSphericalGrowingGel::new(&mesh).unwrap();
    assert!(gel.init(&mesh).is_err());
    assert!(MeshIgfemSphericalGrowingGel::new(&Mesh::new(3, "m3").unwrap()).is_err());
}

/// IGFEM-003 (fails if `set_radius` computes `r² = new_position`
/// instead of `new_position²`): upstream's
/// `IGFEMEnrichment::moveInterface(new_position, domain)` through
/// [`SphericalEnrichment`] gives `test_generation_igfem_mesh_from_sphere`'s
/// counts for each radius factor (`r = f·√(0.4999999999 / 4)`), both as the
/// first move on a mesh without enriched nodes and as one move of the
/// sequence 1, 1.5, 2, 2.1, 3.5 (each pass removes the previous enriched
/// nodes), plus the geometric invariants of the generated mesh.
#[test]
fn spherical_enrichment_move_interface_gives_test_199_counts() {
    use akantu_igfem::enrichment::SphericalEnrichment;
    let r2: Real = 0.4999999999 / 4.0;
    let r0 = r2.sqrt();
    let expected: [(Real, [usize; 3]); 5] = [
        (1.0, [1, 0, 1]),
        (1.5, [1, 0, 1]),
        (2.0, [1, 1, 0]),
        (2.1, [0, 0, 2]),
        (3.5, [1, 0, 1]),
    ];
    let new_enrichment = |shared: &SharedMesh| {
        let mut e = SphericalEnrichment::new(&shared.read().unwrap()).unwrap();
        e.initialize(&shared.read().unwrap()).unwrap();
        e.register_geometry_object(vec![sphere([1.0, 0.0], r2)], "gel")
            .unwrap();
        e
    };
    let mv = |shared: &SharedMesh, e: &mut SphericalEnrichment, f: Real, domain: &str| {
        let mut pass = GrowingGelPass::default();
        shared
            .mutate(
                &mut [],
                |tx| {
                    pass = e.move_interface(tx, f * r0, domain)?;
                    Ok(())
                },
                None,
            )
            .unwrap();
        pass
    };
    // each factor as the first move on a mesh without enriched nodes
    for &(fact, want) in &expected {
        let shared = SharedMesh::new(read_msh(2, "test_geometry_triangle.msh", "mesh_first"));
        let mut e = new_enrichment(&shared);
        assert_eq!(e.nb_enriched_nodes(), 0);
        let pass = mv(&shared, &mut e, fact, "");
        let m = shared.read().unwrap();
        assert_eq!(counts(&m), want, "factor {fact} (first move): {pass:?}");
        assert_eq!(m.nb_nodes(), 4 + pass.new_nodes, "factor {fact}");
        // the radius is applied squared: r² = (f r₀)²
        let got = e.geometry("gel").unwrap()[0]
            .squared_radius
            .to_double()
            .unwrap();
        assert!(
            (got - (fact * r0) * (fact * r0)).abs() <= 1e-15,
            "factor {fact}: r² = {got}"
        );
        check_invariants(&m, &[([1.0, 0.0], got)], 0.5);
    }
    // the sequence on one mesh (explicit domain name)
    let shared = SharedMesh::new(read_msh(2, "test_geometry_triangle.msh", "mesh_seq"));
    let mut e = new_enrichment(&shared);
    for &(fact, want) in &expected {
        let pass = mv(&shared, &mut e, fact, "gel");
        let m = shared.read().unwrap();
        assert_eq!(counts(&m), want, "factor {fact} (sequence): {pass:?}");
        assert_eq!(m.nb_nodes(), 4 + pass.new_nodes, "factor {fact}");
    }
    // an unregistered domain is refused, the mesh is left as it was
    let before = counts(&shared.read().unwrap());
    let mut tried = false;
    let r = shared.mutate(
        &mut [],
        |tx| {
            tried = true;
            e.move_interface(tx, 0.3, "nope").map(|_| ())
        },
        None,
    );
    assert!(tried && r.is_err());
    assert_eq!(counts(&shared.read().unwrap()), before);
}

/// IGFEM-003: upstream's `registerGeometryObject` /
/// `unRegisterGeometryObject`: the first registered domain is the default
/// (the empty domain name resolves to it), unregistering makes the first
/// remaining domain (name order) the default, an unknown domain is refused
/// (upstream: debug assertion), registering a domain twice is refused.
#[test]
fn spherical_enrichment_geometry_registry_and_default_domain() {
    use akantu_igfem::enrichment::SphericalEnrichment;
    let mesh = read_msh(2, "test_geometry_triangle.msh", "mesh_reg");
    let mut e = SphericalEnrichment::new(&mesh).unwrap();
    // nothing registered: the default (empty) domain does not exist
    assert!(e.geometry("").is_err());
    assert!(e.geometry("a").is_err());
    let g = |r2: Real| vec![sphere([0.0, 0.0], r2)];
    e.register_geometry_object(g(1.0), "b").unwrap();
    e.register_geometry_object(g(2.0), "c").unwrap();
    e.register_geometry_object(g(3.0), "a").unwrap();
    let r2 = |e: &SphericalEnrichment, d: &str| {
        e.geometry(d).unwrap()[0]
            .squared_radius
            .to_double()
            .unwrap()
    };
    // the first registered domain ("b") is the default
    assert_eq!(r2(&e, ""), 1.0);
    assert_eq!(e.geometry_snapshot().default_domain(), "b");
    assert_eq!((r2(&e, "a"), r2(&e, "b"), r2(&e, "c")), (3.0, 1.0, 2.0));
    // a domain registered twice is refused and does not change the registry
    assert!(e.register_geometry_object(g(9.0), "c").is_err());
    assert_eq!(r2(&e, "c"), 2.0);
    // an unknown domain is refused
    assert!(e.unregister_geometry_object("nope").is_err());
    assert!(e.geometry("nope").is_err());
    // removing the default: the first remaining domain in name order
    e.unregister_geometry_object("b").unwrap();
    assert!(e.geometry("b").is_err());
    assert_eq!(e.geometry_snapshot().default_domain(), "a");
    assert_eq!(r2(&e, ""), 3.0);
    // removing a non-default domain also re-picks the first in name order
    e.unregister_geometry_object("c").unwrap();
    assert_eq!(e.geometry_snapshot().default_domain(), "a");
    // the last domain: nothing remains, the default name is stale
    e.unregister_geometry_object("a").unwrap();
    assert!(e.geometry("").is_err());
    assert!(e.unregister_geometry_object("a").is_err());
}
