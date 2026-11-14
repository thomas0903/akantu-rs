// SPDX-License-Identifier: GPL-3.0-or-later
// akantu-rs — GPL-3.0-or-later (D-042) because it links akantu-geometry-cgal;
// it contains no CGAL-derived code. Ported from Akantu (EPFL/LSMS):
// differential tests of src/geometry (MeshGeomFactory + CGAL AABB tree,
// MeshSegmentIntersector, MeshSphereIntersector, the spherical kernel) on
// adversarial inputs, against the oracle fixtures
// fixtures/geometry/adversarial/{spherical_adv, aabb_fan, aabb_square,
// segment_fan, segment_square, sphere_grid10}
// (oracle/probes/geometry/geometry_adversarial_probe.cc).

//! Comparisons go through the validation harness ([`DiffTest`]) with the
//! exact rule of `fixtures/geometry/tolerances.json`, plus the sign of zero
//! of every expected zero (the geometry rule is bitwise, signs of zero
//! included). None of these cases contains NaN.

use akantu_core::{ElementType, GhostType};
use akantu_geometry::index::TriangleIndex;
use akantu_geometry::intersections::TriangleSegmentIntersection;
use akantu_geometry::kernel::{Point3, Ray3, Segment3};
use akantu_geometry::{MeshSegmentIntersector, SegmentIntersectorOptions};
use akantu_geometry_cgal::spherical::{LineArc, SkPoint, SkSphere};
use akantu_geometry_cgal::{AabbTree, MeshSphereIntersector, SphereIntersectorOptions};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_validation::harness::DiffTest;

fn p(v: &[f64]) -> Point3 {
    Point3::new(v[0], v[1], v[2])
}

fn to_usize(v: i64) -> usize {
    usize::try_from(v).unwrap()
}

fn open(test: &str, case: &str) -> DiffTest {
    DiffTest::new(test, &format!("geometry/adversarial/{case}"))
        .unwrap_or_else(|e| panic!("fixture geometry/adversarial/{case}: {e}"))
}

/// Every expected `±0` must be met with the same sign (explicit check; since
/// D-051 the harness's exact rule is sign-strict too). No expected value of
/// these cases is NaN.
fn check_f64_bitwise(dt: &mut DiffTest, quantity: &str, actual: &[f64]) {
    dt.check_f64(quantity, actual);
    let expected = dt.reference_f64(quantity).unwrap().data;
    assert!(
        !expected.iter().any(|x| x.is_nan()),
        "{quantity}: unexpected NaN"
    );
    if expected.len() == actual.len() {
        for (i, (e, a)) in expected.iter().zip(actual).enumerate() {
            if *e == 0.0 {
                assert_eq!(e.to_bits(), a.to_bits(), "{quantity}[{i}]: sign of zero");
            }
        }
    }
}

fn enc(r: &TriangleSegmentIntersection) -> (i64, [f64; 6]) {
    match r {
        TriangleSegmentIntersection::Point(q) => (1, [q.x, q.y, q.z, 0.0, 0.0, 0.0]),
        TriangleSegmentIntersection::Segment(g) => (
            2,
            [
                g.source.x, g.source.y, g.source.z, g.target.x, g.target.y, g.target.z,
            ],
        ),
    }
}

/// The 2-D triangle mesh recorded by the case (`nodes`, `conn`).
fn mesh2(dt: &DiffTest) -> Mesh {
    let nodes = dt.fixture().load_f64("nodes").unwrap().data;
    let conn = dt.fixture().load_i64("conn").unwrap().data;
    let mut m = Mesh::new(2, "mesh").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut m);
        for x in nodes.chunks(2) {
            acc.push_node(x).unwrap();
        }
        let mut c = acc.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for e in conn.chunks(3) {
            let t: Vec<i32> = e.iter().map(|&v| i32::try_from(v).unwrap()).collect();
            c.push_tuple(&t);
        }
    }
    m
}

#[test]
fn spherical_adversarial_matches_the_oracle() {
    let mut dt = open("spherical_adversarial_matches_the_oracle", "spherical_adv");
    let arcs = dt.fixture().load_f64("arcs").unwrap().data;
    let sph = dt.fixture().load_f64("spheres").unwrap().data;
    let n = arcs.len() / 6;
    assert_eq!(n, 3000);
    let (mut count, mut mult, mut points) = (Vec::new(), Vec::new(), Vec::new());
    let mut tangent = 0;
    for i in 0..n {
        let a = SkPoint::try_from_f64([arcs[6 * i], arcs[6 * i + 1], arcs[6 * i + 2]]).unwrap();
        let b = SkPoint::try_from_f64([arcs[6 * i + 3], arcs[6 * i + 4], arcs[6 * i + 5]]).unwrap();
        let s =
            SkSphere::try_from_f64([sph[4 * i], sph[4 * i + 1], sph[4 * i + 2]], sph[4 * i + 3])
                .unwrap();
        let res = LineArc::from_points(&a, &b)
            .unwrap()
            .intersect_sphere(&s)
            .unwrap();
        let mut m = [0i64; 2];
        let mut q = [0.0f64; 6];
        for (k, (pt, mu)) in res.iter().enumerate() {
            m[k] = i64::from(*mu);
            q[3 * k..3 * k + 3].copy_from_slice(&pt.to_double().unwrap());
        }
        tangent += usize::from(m[0] == 2);
        count.push(i64::try_from(res.len()).unwrap());
        mult.extend_from_slice(&m);
        points.extend_from_slice(&q);
    }
    dt.check_i64("count", &count);
    dt.check_i64("mult", &mult);
    check_f64_bitwise(&mut dt, "points", &points);
    eprintln!("spherical_adv: {n} samples, {tangent} tangent");
    dt.assert_pass();
}

fn aabb(test: &str, case: &str) {
    let mut dt = open(test, case);
    let mesh = mesh2(&dt);
    let si = MeshSegmentIntersector::<AabbTree>::new(
        &mesh,
        ElementType::Triangle3,
        SegmentIntersectorOptions::upstream(),
    )
    .unwrap();
    let tree = AabbTree::build(si.factory().primitive_list()).unwrap();
    let q = dt.fixture().load_f64("queries").unwrap().data;
    let (mut status, mut off, mut ids, mut kinds, mut coords, mut roff, mut rids) = (
        Vec::new(),
        vec![0i64],
        Vec::new(),
        Vec::new(),
        Vec::new(),
        vec![0i64],
        Vec::new(),
    );
    let mut multi = 0;
    for k in 0..q.len() / 6 {
        let s = Segment3::new(p(&q[6 * k..]), p(&q[6 * k + 3..]));
        let mut got = Vec::new();
        let st = i64::from(tree.all_intersections(&s, &mut got).is_err());
        if st == 1 {
            got.clear(); // the probe clears the partial result on an exception
        }
        status.push(st);
        multi += usize::from(got.len() > 1);
        for (x, id) in &got {
            ids.push(i64::from(*id));
            let (kk, c) = enc(x);
            kinds.push(kk);
            coords.extend_from_slice(&c);
        }
        off.push(i64::try_from(ids.len()).unwrap());
        let mut rays = Vec::new();
        tree.all_intersected_primitives(&Ray3::new(s.source, s.target), &mut rays);
        rids.extend(rays.into_iter().map(i64::from));
        roff.push(i64::try_from(rids.len()).unwrap());
    }
    dt.check_i64("status", &status);
    dt.check_i64("off", &off);
    dt.check_i64("ids", &ids);
    dt.check_i64("kinds", &kinds);
    check_f64_bitwise(&mut dt, "coords", &coords);
    dt.check_i64("roff", &roff);
    dt.check_i64("rids", &rids);
    eprintln!("{case}: {} queries ({multi} with >1 hit)", status.len());
    dt.assert_pass();
}

#[test]
fn aabb_fan_adversarial_matches_the_oracle() {
    aabb("aabb_fan_adversarial_matches_the_oracle", "aabb_fan");
}

#[test]
fn aabb_square_adversarial_matches_the_oracle() {
    aabb("aabb_square_adversarial_matches_the_oracle", "aabb_square");
}

fn segment(test: &str, case: &str) {
    let mut dt = open(test, case);
    let mesh = mesh2(&dt);
    let si = MeshSegmentIntersector::<AabbTree>::new(
        &mesh,
        ElementType::Triangle3,
        SegmentIntersectorOptions::upstream(),
    )
    .unwrap();
    let q = dt.fixture().load_f64("queries").unwrap().data;
    let sizes = dt.fixture().load_i64("batch_sizes").unwrap().data;
    let mut res = Mesh::new(2, "interface_mesh").unwrap();
    let (mut assoc, mut status, mut after) = (Vec::new(), Vec::new(), Vec::new());
    let mut start = 0;
    for &n in &sizes {
        let n = to_usize(n);
        let mut st = 0;
        for k in start..start + n {
            let s = Segment3::new(p(&q[6 * k..]), p(&q[6 * k + 3..]));
            match si.compute_segments(&mesh, &s) {
                Ok(segs) => {
                    for (g, e) in &segs {
                        if !g.is_degenerate() {
                            assoc.push(i64::from(*e));
                        }
                    }
                    si.compute_intersection_query(&mesh, &s, &mut res).unwrap();
                }
                Err(_) => {
                    st = 1;
                    break;
                }
            }
        }
        start += n;
        status.push(st);
        after.push(
            i64::try_from(res.nb_element(ElementType::Segment2, GhostType::NotGhost)).unwrap(),
        );
    }
    let rn = res.nodes();
    let rnodes: Vec<f64> = (0..rn.len()).flat_map(|i| rn.tuple(i).to_vec()).collect();
    let rconn: Vec<i64> = match res.connectivity(ElementType::Segment2, GhostType::NotGhost) {
        Ok(c) => (0..c.len())
            .flat_map(|e| c.tuple(e).iter().map(|&v| i64::from(v)).collect::<Vec<_>>())
            .collect(),
        Err(_) => Vec::new(),
    };
    dt.check_i64("status", &status);
    dt.check_i64("after", &after);
    check_f64_bitwise(&mut dt, "rnodes", &rnodes);
    dt.check_i64("rconn", &rconn);
    dt.check_i64("assoc", &assoc);
    eprintln!("{case}: {} result elements", assoc.len());
    dt.assert_pass();
}

#[test]
fn segment_fan_adversarial_matches_the_oracle() {
    segment("segment_fan_adversarial_matches_the_oracle", "segment_fan");
}

#[test]
fn segment_square_adversarial_matches_the_oracle() {
    segment(
        "segment_square_adversarial_matches_the_oracle",
        "segment_square",
    );
}

#[test]
fn sphere_grid10_adversarial_matches_the_oracle() {
    let mut dt = open(
        "sphere_grid10_adversarial_matches_the_oracle",
        "sphere_grid10",
    );
    let fx = dt.fixture();
    let sph = fx.load_f64("spheres").unwrap().data;
    let sizes = fx.load_i64("run_sizes").unwrap().data;
    let app_off = fx.load_i64("app_off").unwrap().data;
    let appended = fx.load_f64("appended").unwrap().data;
    let nb_old = fx.load_i64("nb_old").unwrap().data;
    let mut mesh = mesh2(&dt);
    let mut si = MeshSphereIntersector::new(
        &mesh,
        ElementType::Triangle3,
        SphereIntersectorOptions::upstream(),
    )
    .unwrap();
    let (mut nb_nodes, mut status, mut ip_off, mut ip, mut nn) =
        (Vec::new(), Vec::new(), vec![0i64], Vec::new(), Vec::new());
    let mut s0 = 0;
    for run in 0..sizes.len() {
        // Points appended to the mesh before this run (IGFEM pattern).
        let (a0, a1) = (to_usize(app_off[run]), to_usize(app_off[run + 1]));
        {
            let mut acc = MeshAccessor::new(&mut mesh);
            for x in appended[2 * a0..2 * a1].chunks(2) {
                acc.push_node(x).unwrap();
            }
        }
        nb_nodes.push(i64::try_from(mesh.nb_nodes()).unwrap());
        let n = to_usize(sizes[run]);
        let list: Vec<SkSphere> = (s0..s0 + n)
            .map(|k| {
                SkSphere::try_from_f64([sph[4 * k], sph[4 * k + 1], sph[4 * k + 2]], sph[4 * k + 3])
                    .unwrap()
            })
            .collect();
        s0 += n;
        si.construct_data(&mesh, GhostType::NotGhost).unwrap();
        let res = si.compute_mesh_query_list_intersection_point(
            &mesh,
            &list,
            i32::try_from(nb_old[run]).unwrap(),
        );
        status.push(i64::from(res.is_err()));
        let pts = si.intersection_points();
        ip.extend((0..pts.len()).flat_map(|i| pts.tuple(i).to_vec()));
        ip_off.push(i64::try_from(ip.len() / 2).unwrap());
        let npe = si.new_node_per_elem();
        nn.extend((0..npe.len()).flat_map(|e| {
            npe.tuple(e)
                .iter()
                .map(|&v| i64::from(v))
                .collect::<Vec<_>>()
        }));
    }
    dt.check_i64("nb_nodes", &nb_nodes);
    dt.check_i64("status", &status);
    dt.check_i64("ip_off", &ip_off);
    check_f64_bitwise(&mut dt, "ip", &ip);
    dt.check_i64("nn", &nn);
    eprintln!(
        "sphere_grid10: {} runs, {} points",
        sizes.len(),
        ip.len() / 2
    );
    dt.assert_pass();
}
