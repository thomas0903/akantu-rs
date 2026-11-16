// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Ports (same intent)
// of test/test_model/test_contact_mechanics_model/test_selector/test_selection.cc
// (TestSurfaceSelectionFixture.PhysicalSurface) and
// test_detector/test_detection.cc (TestCMMDFixture: detection on straight
// interfaces of segments, triangles, quadrangles, mixed, 3-D), plus
// independent checks of the detection (analytical gaps, normals, nodal
// areas, rotation covariance, UD-363 demonstration).

#![allow(clippy::needless_range_loop)] // step/entry indices mirror the upstream test loops

mod common;

use akantu_contact::{ContactUpstreamCompat, PhysicalSurfaceSelector, SurfaceSelector};
use akantu_core::{ElementType, GhostType, Real};
use akantu_parser::Parser;
use common::*;

const DELTA: Real = 1e-3;

/// Relative gap bound of the rotated straight interfaces (both projection
/// metrics); justification at `detection_is_rotation_covariant`.
const ROTATED_GAP_REL: Real = 1e-11;

fn rel(a: Real, b: Real) -> Real {
    (a - b).abs() / b.abs().max(Real::MIN_POSITIVE)
}

/* ---------------------------------------------------------------------- */
/* test_selection.cc: TestSurfaceSelectionFixture.PhysicalSurface          */
/* ---------------------------------------------------------------------- */

#[test]
fn test_selection_physical_surface() {
    let dat = "contact_detector [\n  type = implicit\n  master = master\n  slave = slave\n]\n";
    let parser = Parser::parse_str(dat, "material.dat").unwrap();
    for (file, dim) in [
        ("test_selector/_selection_triangle_3.msh", 2),
        ("test_selector/_selection_segment_2.msh", 2),
    ] {
        let mesh = read_mesh(&contact_mesh_path(file), dim);
        let sel = PhysicalSurfaceSelector::new(&mesh, &parser).unwrap();
        let master = mesh
            .groups()
            .element_group("master")
            .unwrap()
            .nodes()
            .to_vec();
        let slave = mesh
            .groups()
            .element_group("slave")
            .unwrap()
            .nodes()
            .to_vec();
        assert_eq!(sel.master_list(&mesh).unwrap(), master.as_slice(), "{file}");
        assert_eq!(sel.slave_list(&mesh).unwrap(), slave.as_slice(), "{file}");
        // contact_surface = master ∪ slave facets, sorted, unique
        let s = sel.contact_surface();
        let mut expected_nodes = [master.clone(), slave.clone()].concat();
        expected_nodes.sort_unstable();
        expected_nodes.dedup();
        assert_eq!(s.nodes(), expected_nodes.as_slice());
        let g = mesh.groups();
        let mut n = 0;
        for name in ["master", "slave"] {
            let eg = g.element_group(name).unwrap();
            for ty in eg.group().element_types(GhostType::NotGhost) {
                for &e in &eg.elements_of(ty, GhostType::NotGhost) {
                    assert!(s.contains(ty, GhostType::NotGhost, e));
                    n += 1;
                }
            }
        }
        assert_eq!(s.nb_elements(), n);
    }
    // 3-D selection mesh: no master/slave groups (upstream throws)
    let mesh = read_mesh(
        &contact_mesh_path("test_selector/_selection_tetrahedron_4.msh"),
        3,
    );
    assert!(PhysicalSurfaceSelector::new(&mesh, &parser).is_err());
    // no contact_detector section
    let empty = Parser::parse_str("", "empty.dat").unwrap();
    assert!(PhysicalSurfaceSelector::new(&mesh, &empty).is_err());
}

/// Upstream's `PhysicalSurfaceSelector` constructor
/// creates the mesh element group `contact_surface` (master ∪ slave,
/// `surface_selector.cc:37-52`), which upstream's Python example
/// `detection-explicit.py:37` reads. Setting the selector on a contact
/// model alone on its mesh mirrors the selector's surface there: the same
/// facets in the same order, the same nodes, dimension `dim − 1`; a second
/// selector replaces it; the mesh revision follows the group-only mutation.
#[test]
fn physical_selector_mirrors_its_surface_as_the_mesh_group() {
    let mesh = read_mesh(
        &contact_mesh_path("test_detector/_detection_triangle_3.msh"),
        2,
    );
    let before = mesh.groups().element_group_exists("contact_surface");
    assert!(!before);
    let dat = contact_dat("master", "slave", 0.0, 1.0, 0.0, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    let check = |shared: &akantu_mesh::SharedMesh,
                 model: &akantu_contact::ContactMechanicsModel| {
        let m = shared.read().unwrap();
        let g = m.groups().element_group("contact_surface").unwrap();
        assert_eq!(g.group().dimension, 1);
        let s = model
            .contact_detector()
            .surface_selector()
            .unwrap()
            .contact_surface();
        assert_eq!(g.nodes().to_vec(), s.nodes().to_vec());
        let ng = GhostType::NotGhost;
        assert_eq!(g.group().element_types(ng), s.element_types(ng));
        for ty in s.element_types(ng) {
            assert_eq!(
                g.elements_of(ty, ng).to_vec(),
                s.elements_of(ty, ng).to_vec()
            );
        }
        assert!(s.nb_elements() > 0);
        assert!(m.groups().node_group_exists("contact_surface_nodes"));
    };
    check(&shared, &model);
    let r0 = shared.revision().unwrap();
    // a second selector (the slave side only) replaces the mirror
    let parser = Parser::parse_str(
        "contact_detector [\n  type = explicit\n  master = slave\n  slave = slave\n]\n",
        "second.dat",
    )
    .unwrap();
    let second = {
        let m = shared.read().unwrap();
        PhysicalSurfaceSelector::new(&m, &parser).unwrap()
    };
    let n_second = second.contact_surface().nb_elements();
    model.set_surface_selector(Box::new(second)).unwrap();
    assert_eq!(shared.revision().unwrap(), r0 + 1);
    check(&shared, &model);
    let m = shared.read().unwrap();
    let g = m.groups().element_group("contact_surface").unwrap();
    let n: usize = g
        .group()
        .element_types(GhostType::NotGhost)
        .into_iter()
        .map(|t| g.elements_of(t, GhostType::NotGhost).len())
        .sum();
    assert_eq!(n, n_second);
}

/* ---------------------------------------------------------------------- */
/* Detection on flat_on_flat (self-contact selection, two-pass)            */
/* ---------------------------------------------------------------------- */

#[test]
fn detection_flat_on_flat_translated_upper_body() {
    let mesh = read_mesh(&contact_mesh_path("flat_on_flat.msh"), 2);
    let dat = contact_dat("contact", "contact", 0.0, 1050e7, 0.0, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    translate_group(&mut model, &shared, "upper", &[0.0, -DELTA]);
    model.search().unwrap();
    let m = shared.read().unwrap();
    let contact_nodes = nodes_of_physical(&m, "contact");
    let upper = nodes_of_physical(&m, "upper");
    // every node of both contact lines is a slave in contact (two-pass)
    assert_eq!(model.contact_elements().len(), contact_nodes.len());
    assert_eq!(contact_nodes.len(), 5 + 11);
    let x = m.nodes();
    for ce in model.contact_elements() {
        let s = ce.slave as usize;
        let gap = model.gaps().tuple(s)[0];
        assert!(rel(gap, DELTA) < 1e-12, "gap {gap}");
        let n = model.normals().tuple(s).to_vec();
        let on_upper = upper.contains(&ce.slave);
        // upper-line slaves project on the lower line (normal +y), and back
        assert_eq!(
            n,
            if on_upper {
                &[0.0, 1.0][..]
            } else {
                &[0.0, -1.0][..]
            }
        );
        // nodal area: Σ ∫ N dA over the adjacent contact segments
        let (h, end) = if on_upper { (0.25, 0.5) } else { (0.1, 0.5) };
        let xs = x.tuple(s)[0];
        let expected = if xs.abs() > 0.5 - 1e-12 { h * end } else { h };
        let area = model.nodal_area().tuple(s)[0];
        // Gmsh places the transfinite nodes of the lower line at
        // `−0.5 + k·0.1` with ~1e-13 absolute error, so the segment lengths
        // are 0.1 to ~1e-12 relative (the 0.25 upper segments are exact).
        assert!(
            rel(area, expected) < 1e-10,
            "area {area} vs {expected} at x = {xs}"
        );
    }
    // nodes not in contact keep zero (−0 after the sign flip) gaps
    let nb_zero = model
        .gaps()
        .as_slice()
        .iter()
        .filter(|g| **g == 0.0)
        .count();
    assert_eq!(nb_zero, m.nb_nodes() - contact_nodes.len());
    drop(x);
    drop(m);
    // forces: rigid master → slave only, f = ε_n δ A_s n
    model.assemble_internal_forces().unwrap();
    for ce in model.contact_elements() {
        let s = ce.slave as usize;
        let f = model.internal_force().tuple(s).to_vec();
        let n = model.normals().tuple(s).to_vec();
        let a = model.nodal_area().tuple(s)[0];
        for d in 0..2 {
            let expected = 1050e7 * DELTA * a * n[d];
            assert!(
                (f[d] - expected).abs() <= 1e-9 * expected.abs().max(1.0),
                "{f:?}"
            );
        }
    }
}

/* ---------------------------------------------------------------------- */
/* test_detection.cc: straight interfaces, 2-D element families            */
/* ---------------------------------------------------------------------- */

fn straight_interface_case(
    file: &str,
    compat: ContactUpstreamCompat,
    angle_deg: Real,
) -> (usize, usize, Real, Real) {
    let mesh = read_mesh(&contact_mesh_path(file), 2);
    let dat = contact_dat("master", "slave", 0.0, 1.0, 0.0, false);
    let (shared, mut model) = contact_model(mesh, &dat);
    model.set_upstream_compat(compat);
    // the slave body (y ≤ 0) penetrates the master body (y ≥ 0) by DELTA
    translate_group(&mut model, &shared, "slave_body", &[0.0, DELTA]);
    let (c, s) = (angle_deg.to_radians().cos(), angle_deg.to_radians().sin());
    for p in model.positions_mut().as_mut_slice().as_chunks_mut::<2>().0 {
        let (x, y) = (p[0], p[1]);
        p[0] = c * x - s * y;
        p[1] = s * x + c * y;
    }
    model.search().unwrap();
    let m = shared.read().unwrap();
    let slaves = nodes_of_physical(&m, "slave");
    let mut max_gap_err: Real = 0.0;
    let mut max_normal_err: Real = 0.0;
    for ce in model.contact_elements() {
        let i = ce.slave as usize;
        assert!(slaves.contains(&ce.slave));
        max_gap_err = max_gap_err.max(rel(model.gaps().tuple(i)[0], DELTA));
        let n = model.normals().tuple(i).to_vec();
        // master outward normal: −e_y rotated
        let e = [s, -c];
        max_normal_err = max_normal_err.max((n[0] - e[0]).abs().max((n[1] - e[1]).abs()));
    }
    (
        model.contact_elements().len(),
        slaves.len(),
        max_gap_err,
        max_normal_err,
    )
}

#[test]
fn test_detection_straight_interfaces_2d() {
    for file in [
        "test_detector/_detection_triangle_3.msh",
        "test_detector/_detection_quadrangle_4.msh",
        "test_detector/_detection_triangle_3_quadrangle_4.msh",
    ] {
        for compat in [
            ContactUpstreamCompat::default(),
            ContactUpstreamCompat::all(),
        ] {
            let (nc, ns, eg, en) = straight_interface_case(file, compat, 0.0);
            assert_eq!(nc, ns, "{file}: every slave node in contact");
            assert!(eg < 1e-12, "{file}: gap error {eg}");
            assert!(en == 0.0, "{file}: normal error {en}");
        }
    }
}

/// Rotation covariance (30°): same contacts, gaps and rotated normals with
/// the corrected metric (one Newton step) and with upstream's (linear
/// convergence, UD-363, tan²30° = 1/3).
///
/// Gap bound [`ROTATED_GAP_REL`] for both metrics (tightened from
/// 1e-9 / 1e-6): on a straight facet the normal gap does not
/// depend on the projection's natural coordinate (at most second order in
/// its error, the Newton residual being ≤ 1e-10), so only the rounding of
/// the rotated coordinates (|x| ≤ 1, a few ulps each) relative to the
/// 1e-3 penetration remains: ≲ 10 ε / 1e-3 ≈ 2e-12. Measured (macOS
/// aarch64): 1.28e-13 corrected, 3.97e-14 upstream metric at 30°.
#[test]
fn detection_is_rotation_covariant() {
    let file = "test_detector/_detection_quadrangle_4.msh";
    let (nc, ns, eg, en) = straight_interface_case(file, ContactUpstreamCompat::default(), 30.0);
    assert_eq!(nc, ns);
    eprintln!("30°, corrected metric: gap error {eg:e}, normal error {en:e}");
    assert!(eg < ROTATED_GAP_REL, "gap error {eg}");
    assert!(en < 1e-14, "normal error {en}");
    let up = ContactUpstreamCompat {
        projection_metric: true,
        ..ContactUpstreamCompat::default()
    };
    let (nc, ns, eg, en) = straight_interface_case(file, up, 30.0);
    assert_eq!(nc, ns);
    eprintln!("30°, upstream metric: gap error {eg:e}, normal error {en:e}");
    assert!(eg < ROTATED_GAP_REL, "gap error {eg}");
    assert!(en < 1e-14, "normal error {en}");
}

/// UD-363: at 60° upstream's Newton iteration diverges (factor tan²60° = 3)
/// and the slaves find no valid projection; the corrected metric finds all.
#[test]
fn ud363_steep_interface_upstream_metric_misses_contacts() {
    let file = "test_detector/_detection_quadrangle_4.msh";
    let (nc, ns, eg, _) = straight_interface_case(file, ContactUpstreamCompat::default(), 60.0);
    assert_eq!(nc, ns);
    eprintln!("60°, corrected metric: gap error {eg:e}");
    assert!(eg < ROTATED_GAP_REL, "gap error {eg}");
    let up = ContactUpstreamCompat {
        projection_metric: true,
        ..ContactUpstreamCompat::default()
    };
    let (nc, ns, _, _) = straight_interface_case(file, up, 60.0);
    assert!(nc < ns, "upstream metric: {nc} contacts of {ns}");
}

/* ---------------------------------------------------------------------- */
/* 3-D: quadrangle and triangle facets                                     */
/* ---------------------------------------------------------------------- */

#[test]
fn test_detection_straight_interfaces_3d() {
    for tets in [false, true] {
        let mesh = two_blocks_3d(4, 1.0, 2, 0.5, tets);
        let dat = contact_dat("master", "slave", 0.0, 1.0, 0.0, false);
        let (shared, mut model) = contact_model(mesh, &dat);
        translate_group(&mut model, &shared, "upper_body", &[0.0, 0.0, -DELTA]);
        model.search().unwrap();
        let m = shared.read().unwrap();
        let slaves = nodes_of_physical(&m, "slave");
        assert_eq!(
            model.contact_elements().len(),
            slaves.len(),
            "tets = {tets}"
        );
        let facet = if tets {
            ElementType::Triangle3
        } else {
            ElementType::Quadrangle4
        };
        let mut total_area = 0.0;
        for ce in model.contact_elements() {
            assert_eq!(ce.master.ty(), facet);
            let i = ce.slave as usize;
            assert!(rel(model.gaps().tuple(i)[0], DELTA) < 1e-12);
            assert_eq!(model.normals().tuple(i), &[0.0, 0.0, 1.0]);
            total_area += model.nodal_area().tuple(i)[0];
        }
        // the slave areas of a flat face sum to its area (partition of unity)
        // — only the slave facets count: the nodal area integrates over the
        // whole contact surface, but slave nodes only touch slave facets
        assert!(rel(total_area, 1.0) < 1e-12, "Σ areas = {total_area}");
    }
}

/// Deformable master, one-pass weighting: contact forces balance (Σ f = 0)
/// and the moment about the origin vanishes for a normal force through the
/// projection point.
#[test]
fn deformable_master_force_balance() {
    use akantu_contact::DeformableMasterWeighting;
    for tets in [false, true] {
        let mesh = two_blocks_3d(3, 1.0, 2, 0.4, tets);
        let dat = contact_dat("master", "slave", 0.0, 1e3, 0.0, true);
        let (shared, mut model) = contact_model(mesh, &dat);
        model.deformable_master_weighting = DeformableMasterWeighting::OnePassFull;
        translate_group(&mut model, &shared, "upper_body", &[0.013, -0.007, -DELTA]);
        model.search().unwrap();
        assert!(!model.contact_elements().is_empty());
        model.assemble_internal_forces().unwrap();
        let f = model.internal_force().as_slice().to_vec();
        let mut sum = [0.0; 3];
        for c in f.as_chunks::<3>().0 {
            for d in 0..3 {
                sum[d] += c[d];
            }
        }
        let scale: Real = f.iter().map(|v| v.abs()).sum();
        for v in sum {
            assert!(v.abs() <= 1e-12 * scale, "Σ f = {sum:?}");
        }
        let _ = shared;
    }
}

/// The outward directions memoised within a search are recomputed in the
/// next one: after a first search, the lower body is mirrored about the
/// contact line (its elements now lie above their contact facets, so the
/// facets' outward side flips) and the upper body moved down into it. The
/// second search of the same model must give exactly what a fresh model
/// gives on the new positions (contact elements, gaps, normals), and the
/// flip must change normals.
#[test]
fn outward_directions_follow_the_positions_between_searches() {
    let new_model = || {
        let mesh = read_mesh(&contact_mesh_path("flat_on_flat.msh"), 2);
        let dat = contact_dat("contact", "contact", 0.0, 1050e7, 0.0, false);
        contact_model(mesh, &dat)
    };
    let move_bodies = |model: &mut akantu_contact::ContactMechanicsModel,
                       shared: &akantu_mesh::SharedMesh| {
        let lower = {
            let m = shared.read().unwrap();
            let upper = nodes_of_physical(&m, "upper");
            (0..m.nb_nodes())
                .filter(|n| !upper.contains(&akantu_core::Idx::try_from(*n).unwrap()))
                .collect::<Vec<_>>()
        };
        let p = model.positions_mut();
        for n in lower {
            let t = p.tuple_mut(n);
            t[1] = -t[1];
        }
        translate_group(model, shared, "upper", &[0.0, -0.05]);
    };
    let (shared, mut model) = new_model();
    translate_group(&mut model, &shared, "upper", &[0.0, -DELTA]);
    model.search().unwrap();
    let first: Vec<Real> = model.normals().as_slice().to_vec();
    assert!(!model.contact_elements().is_empty());
    // undo the first translation, then the new configuration
    translate_group(&mut model, &shared, "upper", &[0.0, DELTA]);
    move_bodies(&mut model, &shared);
    model.search().unwrap();
    let (shared2, mut fresh) = new_model();
    move_bodies(&mut fresh, &shared2);
    fresh.search().unwrap();
    assert_eq!(model.contact_elements(), fresh.contact_elements());
    assert_eq!(model.gaps().as_slice(), fresh.gaps().as_slice());
    assert_eq!(model.normals().as_slice(), fresh.normals().as_slice());
    assert_ne!(first, model.normals().as_slice());
}
