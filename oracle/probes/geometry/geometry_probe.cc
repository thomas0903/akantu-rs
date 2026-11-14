// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) (src/geometry, parity rows GEOM-001…009): records the
// behaviour of the CGAL-enabled oracle for
//   kernel/*            CGAL::intersection(Segment_3, Triangle_3),
//                       do_intersect(Segment_3, Bbox_3), do_intersect(Ray_3,
//                       Triangle_3), Tetrahedron_3 sides (Simple_cartesian<double>)
//   aabb/*              MeshGeomFactory<dim, type, Triangle<K>, K> + its CGAL
//                       AABB tree: all_intersections / all_intersected_primitives
//                       in traversal order
//   segment_intersector/*  MeshSegmentIntersector<dim, type> results
//   spherical/*         CGAL::intersection(Line_arc_3, Sphere_3) of cgal::Spherical
//                       (counts, multiplicities, to_double of the points)
//   sphere_intersector/*   MeshSphereIntersector<2, _triangle_3> (compile-fixed
//                       copy, oracle/patches/geometry/0001-*, UD-409)
// Inputs are generated deterministically here and recorded with the outputs.
//
// Usage: geometry_probe <fixtures_root> <upstream_root>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>

#include "geom_helper_functions.hh"
#include "mesh_geom_common.hh"
#include "mesh_geom_factory.hh"
#include "mesh_segment_intersector.hh"
#include "mesh_sphere_intersector.hh"
#include "tree_type_helper.hh"

#include <CGAL/Spherical_kernel_intersections.h>
#include <CGAL/intersections.h>

#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <list>
#include <map>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

using K = cgal::Cartesian;
using SK = cgal::Spherical;

namespace {

const char * const PROBE = "geometry_probe";
const char * const PROBE_VERSION = "1";

struct Rng {
  std::uint64_t s;
  explicit Rng(std::uint64_t seed) : s(seed) {}
  std::uint64_t next() {
    s ^= s >> 12;
    s ^= s << 25;
    s ^= s >> 27;
    return s * 0x2545F4914F6CDD1DULL;
  }
  int below(int n) { return static_cast<int>(next() % static_cast<std::uint64_t>(n)); }
  /// uniform in [0, 1) with 53 random bits
  double unit() { return static_cast<double>(next() >> 11) * 0x1.0p-53; }
  /// a coordinate: random double or one of a few binary-friendly values
  /// (to produce coincidences, collinear and coplanar configurations)
  double coord(double lo, double hi) {
    static const double grid[] = {0., 0.25, 0.5, 0.75, 1., 0.125, 0.375, 0.1, 0.3};
    if (below(3) == 0)
      return lo + (hi - lo) * grid[below(9)];
    return lo + (hi - lo) * unit();
  }
};

void add_f64(ap::FixtureWriter & w, const std::string & name, std::vector<std::size_t> shape,
             std::vector<std::string> axes, const std::vector<double> & v,
             const std::string & layout = "table", const std::string & tol = "exact") {
  w.add(name, shape, axes, layout, v).tolerance_class(tol);
}

void add_i64(ap::FixtureWriter & w, const std::string & name, std::vector<std::size_t> shape,
             std::vector<std::string> axes, const std::vector<std::int64_t> & v,
             const std::string & layout = "table") {
  w.add(name, shape, axes, layout, v).tolerance_class("exact");
}

void push3(std::vector<double> & v, const K::Point_3 & p) {
  v.push_back(p.x());
  v.push_back(p.y());
  v.push_back(p.z());
}

/// kind (0 none, 1 point, 2 segment) and 6 coordinates of an intersection
template <class Variant> void encode(const Variant & var, std::vector<std::int64_t> & kinds,
                                     std::vector<double> & coords) {
  if (const auto * p = boost::get<K::Point_3>(&var)) {
    kinds.push_back(1);
    push3(coords, *p);
    coords.insert(coords.end(), 3, 0.);
  } else if (const auto * s = boost::get<K::Segment_3>(&var)) {
    kinds.push_back(2);
    push3(coords, s->source());
    push3(coords, s->target());
  } else {
    kinds.push_back(-2);
    coords.insert(coords.end(), 6, 0.);
  }
}

K::Point_3 rpoint(Rng & r) { return K::Point_3(r.coord(0, 1), r.coord(0, 1), r.coord(0, 1)); }
K::Point_3 rpoint_plane(Rng & r) { return K::Point_3(r.coord(0, 1), r.coord(0, 1), 0.); }

// ---------------------------------------------------------------------------
void kernel_case(const fs::path & root) {
  ap::FixtureWriter w(root, "geometry/kernel/triangle_segment",
                      "Simple_cartesian<double>: intersection(Segment_3, Triangle_3), "
                      "do_intersect(Segment_3, Bbox_3 of the triangle), do_intersect(Ray_3(source, "
                      "target), Triangle_3), Tetrahedron_3 has_on_bounded_side / has_on_boundary",
                      PROBE, PROBE_VERSION);
  w.add_tag("geometry");
  w.add_tag("cgal_lgpl_kernel");
  Rng r(0x51ED270B27C5A3E9ULL);
  const std::size_t n = 3000;
  std::vector<double> tri, seg, tet, pts, coords;
  std::vector<std::int64_t> kind, bbox_seg, ray_tri, tet_bounded, tet_boundary;
  for (std::size_t i = 0; i < n; ++i) {
    const int mode = r.below(4); // 0: random 3-D; 1: coplanar z = 0; 2: segment
                                 // through vertices/edges; 3: degenerate inputs
    K::Point_3 a, b, c, p, q;
    if (mode == 1) {
      a = rpoint_plane(r), b = rpoint_plane(r), c = rpoint_plane(r);
      p = rpoint_plane(r), q = rpoint_plane(r);
    } else {
      a = rpoint(r), b = rpoint(r), c = rpoint(r);
      p = rpoint(r), q = rpoint(r);
    }
    if (mode == 2) {
      // end point at a vertex or edge midpoint, other end random
      const K::Point_3 cand[] = {a, b, c, CGAL::midpoint(a, b), CGAL::midpoint(b, c)};
      p = cand[r.below(5)];
      if (r.below(2) == 0)
        q = K::Point_3(2 * p.x() - q.x(), 2 * p.y() - q.y(), 2 * p.z() - q.z());
    }
    if (mode == 3) {
      if (r.below(2) == 0)
        q = p; // degenerate segment
      else
        c = CGAL::midpoint(a, b); // degenerate (collinear) triangle
    }
    K::Triangle_3 t(a, b, c);
    K::Segment_3 s(p, q);
    push3(tri, a);
    push3(tri, b);
    push3(tri, c);
    push3(seg, p);
    push3(seg, q);
    try {
      auto res = CGAL::intersection(s, t);
      if (!res) {
        kind.push_back(0);
        coords.insert(coords.end(), 6, 0.);
      } else {
        encode(*res, kind, coords);
      }
    } catch (std::exception &) {
      kind.push_back(-1);
      coords.insert(coords.end(), 6, 0.);
    }
    bbox_seg.push_back(CGAL::do_intersect(s, t.bbox()) ? 1 : 0);
    ray_tri.push_back(CGAL::do_intersect(K::Ray_3(p, q), t) ? 1 : 0);
    K::Point_3 d = rpoint(r);
    K::Point_3 x = (r.below(3) == 0) ? CGAL::midpoint(a, b) : rpoint(r);
    K::Tetrahedron_3 th(a, b, c, d);
    push3(tet, a);
    push3(tet, b);
    push3(tet, c);
    push3(tet, d);
    push3(pts, x);
    tet_bounded.push_back(th.has_on_bounded_side(x) ? 1 : 0);
    tet_boundary.push_back(th.has_on_boundary(x) ? 1 : 0);
  }
  add_f64(w, "input/triangles", {n, 9}, {"sample", "component"}, tri);
  add_f64(w, "input/segments", {n, 6}, {"sample", "component"}, seg);
  add_f64(w, "input/tetrahedra", {n, 12}, {"sample", "component"}, tet);
  add_f64(w, "input/points", {n, 3}, {"sample", "component"}, pts);
  add_i64(w, "intersection/kind", {n}, {"sample"}, kind);
  add_f64(w, "intersection/coords", {n, 6}, {"sample", "component"}, coords);
  add_i64(w, "bbox_segment", {n}, {"sample"}, bbox_seg);
  add_i64(w, "ray_triangle", {n}, {"sample"}, ray_tri);
  add_i64(w, "tetrahedron/bounded", {n}, {"sample"}, tet_bounded);
  add_i64(w, "tetrahedron/boundary", {n}, {"sample"}, tet_boundary);
  w.parameters()["kind_codes"] = ap::Json::raw(R"({"none":0,"point":1,"segment":2,"exception":-1})");
  w.finish();
}

// ---------------------------------------------------------------------------
/// structured triangle mesh [0,1]x[0,ly], alternating diagonals
void triangle_grid(Mesh & mesh, Int nx, Int ny, Real ly) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  for (Int j = 0; j <= ny; ++j)
    for (Int i = 0; i <= nx; ++i) {
      Vector<Real> v(2);
      v(0) = Real(i) / Real(nx);
      v(1) = ly * Real(j) / Real(ny);
      nodes.push_back(v);
    }
  auto id = [&](Int i, Int j) { return j * (nx + 1) + i; };
  auto & conn = acc.getConnectivity(_triangle_3);
  for (Int j = 0; j < ny; ++j)
    for (Int i = 0; i < nx; ++i) {
      Idx a = id(i, j), b = id(i + 1, j), c = id(i + 1, j + 1), d = id(i, j + 1);
      Vector<Idx> t1(3), t2(3);
      if ((i + j) % 2 == 1) {
        t1 << a, b, d;
        t2 << b, c, d;
      } else {
        t1 << a, b, c;
        t2 << a, c, d;
      }
      conn.push_back(t1);
      conn.push_back(t2);
    }
  acc.makeReady();
}

/// structured tetrahedral mesh of the unit cube, n^3 cubes x 6 tetrahedra
void tet_grid(Mesh & mesh, Int n) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  for (Int k = 0; k <= n; ++k)
    for (Int j = 0; j <= n; ++j)
      for (Int i = 0; i <= n; ++i) {
        Vector<Real> v(3);
        v(0) = Real(i) / Real(n);
        v(1) = Real(j) / Real(n);
        v(2) = Real(k) / Real(n);
        nodes.push_back(v);
      }
  auto id = [&](Int i, Int j, Int k) { return (k * (n + 1) + j) * (n + 1) + i; };
  auto & conn = acc.getConnectivity(_tetrahedron_4);
  for (Int k = 0; k < n; ++k)
    for (Int j = 0; j < n; ++j)
      for (Int i = 0; i < n; ++i) {
        auto v = [&](Int a, Int b, Int c) { return id(i + a, j + b, k + c); };
        Idx p0 = v(0, 0, 0), p7 = v(1, 1, 1);
        const Idx paths[6][2] = {{v(1, 0, 0), v(1, 1, 0)}, {v(1, 0, 0), v(1, 0, 1)},
                                 {v(0, 1, 0), v(1, 1, 0)}, {v(0, 1, 0), v(0, 1, 1)},
                                 {v(0, 0, 1), v(1, 0, 1)}, {v(0, 0, 1), v(0, 1, 1)}};
        for (auto & pth : paths) {
          Vector<Idx> t(4);
          t << p0, pth[0], pth[1], p7;
          conn.push_back(t);
        }
      }
  acc.makeReady();
}

void record_mesh(ap::FixtureWriter & w, const std::string & prefix, const Mesh & mesh,
                 ElementType type) {
  const auto & X = mesh.getNodes();
  std::vector<double> x(X.data(), X.data() + X.size() * X.getNbComponent());
  add_f64(w, prefix + "/nodes", {std::size_t(X.size()), std::size_t(X.getNbComponent())},
          {"node", "spatial_dim"}, x, "nodal");
  const auto & c = mesh.getConnectivity(type);
  std::vector<std::int64_t> cv(c.data(), c.data() + c.size() * c.getNbComponent());
  add_i64(w, prefix + "/connectivity", {std::size_t(c.size()), std::size_t(c.getNbComponent())},
          {"element", "node_local"}, cv, "elemental");
}

/// random query segments in the box [lo, hi]^dim (z = 0 in 2-D), plus
/// segments along mesh lines and through nodes
std::vector<K::Segment_3> queries(Rng & r, Int dim, std::size_t n, double lo, double hi,
                                  Int grid) {
  std::vector<K::Segment_3> q;
  auto pt = [&]() {
    const double z = (dim == 3) ? r.coord(lo, hi) : 0.;
    return K::Point_3(r.coord(lo, hi), r.coord(lo, hi), z);
  };
  for (std::size_t i = 0; i < n; ++i) {
    const int mode = r.below(5);
    K::Point_3 a = pt(), b = pt();
    if (mode == 0) {
      // along a grid line
      const double c = Real(r.below(grid + 1)) / Real(grid);
      if (r.below(2) == 0)
        a = K::Point_3(a.x(), c, a.z()), b = K::Point_3(b.x(), c, b.z());
      else
        a = K::Point_3(c, a.y(), a.z()), b = K::Point_3(c, b.y(), b.z());
    } else if (mode == 1) {
      // through a node
      const double cx = Real(r.below(grid + 1)) / Real(grid);
      const double cy = Real(r.below(grid + 1)) / Real(grid);
      const double cz = (dim == 3) ? Real(r.below(grid + 1)) / Real(grid) : 0.;
      b = K::Point_3(2 * cx - a.x(), 2 * cy - a.y(), 2 * cz - a.z());
    } else if (mode == 2) {
      // short segment (often inside one element)
      b = K::Point_3(a.x() + 0.01 * r.unit(), a.y() + 0.01 * r.unit(),
                     (dim == 3) ? a.z() + 0.01 * r.unit() : 0.);
    }
    q.emplace_back(a, b);
  }
  return q;
}

void push_segments(std::vector<double> & v, const std::vector<K::Segment_3> & q) {
  for (auto & s : q) {
    push3(v, s.source());
    push3(v, s.target());
  }
}

// ---------------------------------------------------------------------------
template <Int dim, ElementType type>
void aabb_case(const fs::path & root, const std::string & id, Mesh & mesh, Int grid,
               std::uint64_t seed) {
  ap::FixtureWriter w(root, "geometry/aabb/" + id,
                      "MeshGeomFactory<dim, type, Triangle<K>, K> primitives and CGAL AABB "
                      "tree traversal order (all_intersections of segments, "
                      "all_intersected_primitives of rays)",
                      PROBE, PROBE_VERSION);
  w.add_tag("geometry");
  w.add_tag("cgal_gpl_aabb_tree");
  record_mesh(w, "input", mesh, type);
  MeshGeomFactory<dim, type, Triangle<K>, K> factory(mesh);
  factory.constructData();
  // primitive list
  std::vector<double> prims;
  std::vector<std::int64_t> pids;
  for (auto & t : factory.getPrimitiveList()) {
    for (int k = 0; k < 3; ++k)
      push3(prims, t.vertex(k));
    pids.push_back(t.id());
  }
  add_f64(w, "primitives/triangles", {pids.size(), 9}, {"entry", "component"}, prims);
  add_i64(w, "primitives/ids", {pids.size()}, {"entry"}, pids);
  const auto & tree = factory.getTree();
  using result_type =
      typename IntersectionTypeHelper<TreeTypeHelper<Triangle<K>, K>, K::Segment_3>::intersection_type;
  Rng r(seed);
  auto q = queries(r, dim, 300, -0.2, 1.2, grid);
  std::vector<double> qv;
  push_segments(qv, q);
  std::vector<std::int64_t> offsets{0}, ids, kinds, ray_offsets{0}, ray_ids;
  std::vector<double> coords;
  for (auto & s : q) {
    std::list<result_type> res;
    tree.all_intersections(s, std::back_inserter(res));
    for (auto & x : res) {
      ids.push_back(x->second);
      encode(x->first, kinds, coords);
    }
    offsets.push_back(static_cast<std::int64_t>(ids.size()));
    std::list<UInt> hit;
    tree.all_intersected_primitives(K::Ray_3(s.source(), s.target()), std::back_inserter(hit));
    for (auto h : hit)
      ray_ids.push_back(h);
    ray_offsets.push_back(static_cast<std::int64_t>(ray_ids.size()));
  }
  add_f64(w, "input/queries", {q.size(), 6}, {"sample", "component"}, qv);
  add_i64(w, "all_intersections/offsets", {offsets.size()}, {"entry"}, offsets, "ragged_offsets");
  w.add("all_intersections/ids", {ids.size()}, {"entry"}, "ragged_values", ids)
      .tolerance_class("exact")
      .ragged("all_intersections/offsets", "sample");
  w.add("all_intersections/kinds", {kinds.size()}, {"entry"}, "ragged_values", kinds)
      .tolerance_class("exact")
      .ragged("all_intersections/offsets", "sample");
  add_f64(w, "all_intersections/coords", {kinds.size(), 6}, {"entry", "component"}, coords);
  add_i64(w, "rays/offsets", {ray_offsets.size()}, {"entry"}, ray_offsets, "ragged_offsets");
  w.add("rays/ids", {ray_ids.size()}, {"entry"}, "ragged_values", ray_ids)
      .tolerance_class("exact")
      .ragged("rays/offsets", "sample");
  w.parameters()["dim"] = ap::Json(static_cast<double>(dim));
  w.set_notes(type == _tetrahedron_4 ? "UD-407: upstream maps _tetrahedron_4 to the triangle "
                                       "helper (one flat triangle (x,y,0) per element)"
                                     : "");
  w.finish();
}

// ---------------------------------------------------------------------------
template <Int dim, ElementType type>
void segment_intersector_case(const fs::path & root, const std::string & id, Mesh & mesh,
                              const std::vector<std::vector<K::Segment_3>> & batches,
                              const std::string & notes) {
  ap::FixtureWriter w(root, "geometry/segment_intersector/" + id,
                      "MeshSegmentIntersector<dim, type>: _segment_2 result mesh (nodes, "
                      "connectivity, associated_element, physical_names) for batches of "
                      "query segments (buildResultFromQueryList per physical name)",
                      PROBE, PROBE_VERSION);
  w.add_tag("geometry");
  w.add_tag("segment_intersector");
  record_mesh(w, "input", mesh, type);
  Mesh result(dim, "interface_mesh");
  // UD-072 work-around (as EmbeddedInterfaceIntersector does): the result
  // mesh needs its _segment_2 connectivity before the first query.
  result.addConnectivityType(_segment_2, _not_ghost);
  result.addConnectivityType(_segment_2, _ghost);
  result.getElementalData<Element>("associated_element").alloc(0, 1, _segment_2);
  result.getElementalData<std::string>("physical_names").alloc(0, 1, _segment_2);
  MeshSegmentIntersector<dim, type> intersector(mesh, result);
  std::vector<double> qv;
  std::vector<std::int64_t> batch_sizes;
  std::vector<std::int64_t> after_batch;
  ap::Json names = ap::Json::array();
  for (std::size_t b = 0; b < batches.size(); ++b) {
    const std::string name = "batch_" + std::to_string(b);
    names.push(name);
    intersector.setPhysicalName(name);
    std::list<K::Segment_3> l(batches[b].begin(), batches[b].end());
    intersector.buildResultFromQueryList(l);
    push_segments(qv, batches[b]);
    batch_sizes.push_back(static_cast<std::int64_t>(batches[b].size()));
    after_batch.push_back(result.getNbElement(_segment_2));
  }
  add_f64(w, "input/queries", {qv.size() / 6, 6}, {"sample", "component"}, qv);
  add_i64(w, "input/batch_sizes", {batch_sizes.size()}, {"entry"}, batch_sizes);
  const auto & X = result.getNodes();
  std::vector<double> x(X.data(), X.data() + X.size() * X.getNbComponent());
  add_f64(w, "result/nodes", {std::size_t(X.size()), std::size_t(dim)}, {"node", "spatial_dim"},
          x, "nodal");
  const auto & c = result.getConnectivity(_segment_2);
  std::vector<std::int64_t> cv(c.data(), c.data() + c.size() * c.getNbComponent());
  add_i64(w, "result/connectivity", {std::size_t(c.size()), 2}, {"element", "node_local"}, cv,
          "elemental");
  const auto & ae = result.getData<Element>("associated_element", _segment_2);
  const auto & pn = result.getData<std::string>("physical_names", _segment_2);
  std::vector<std::int64_t> ael, pnc;
  for (Idx e = 0; e < ae.size(); ++e) {
    ael.push_back(ae(e).element);
    pnc.push_back(std::stoll(pn(e).substr(6)));
  }
  add_i64(w, "result/associated_element", {ael.size()}, {"element"}, ael, "elemental");
  add_i64(w, "result/physical_names", {pnc.size()}, {"element"}, pnc, "elemental");
  add_i64(w, "result/nb_elements_after_batch", {after_batch.size()}, {"entry"}, after_batch);
  w.parameters()["dim"] = ap::Json(static_cast<double>(dim));
  w.parameters()["string_tables"]["physical_names"] = names;
  w.set_notes(notes);
  w.finish();
}

// ---------------------------------------------------------------------------
void spherical_case(const fs::path & root) {
  ap::FixtureWriter w(root, "geometry/spherical/line_arc_sphere",
                      "cgal::Spherical (Spherical_kernel_3 over Quotient<MP_Float>): "
                      "intersection(Line_arc_3, Sphere_3) counts, multiplicities and "
                      "to_double of the solutions",
                      PROBE, PROBE_VERSION);
  w.add_tag("geometry");
  w.add_tag("cgal_gpl_spherical_kernel");
  Rng r(0xA0761D6478BD642FULL);
  const std::size_t n = 3000;
  std::vector<double> arcs, spheres, points;
  std::vector<std::int64_t> count, mult;
  for (std::size_t i = 0; i < n; ++i) {
    const int mode = r.below(4); // 0 random; 1 binary-fraction tangents; 2 2-D; 3 through centre
    double p[3], q[3], c[3], r2;
    for (int k = 0; k < 3; ++k) {
      p[k] = r.coord(-1, 1);
      q[k] = r.coord(-1, 1);
      c[k] = r.coord(-1, 1);
    }
    r2 = r.coord(0.01, 1.5);
    if (mode == 1) {
      // horizontal arc y = 0.25·k touching the sphere of radius 0.125·m
      const double y0 = 0.25 * r.below(5) - 0.5, rad = 0.125 * (1 + r.below(4));
      p[1] = q[1] = y0;
      p[2] = q[2] = c[2] = 0.;
      c[1] = y0 + (r.below(2) ? rad : -rad);
      p[0] = c[0] - 0.5 - 0.25 * r.below(3);
      q[0] = c[0] + 0.5 + 0.25 * r.below(3);
      r2 = rad * rad;
    } else if (mode == 2) {
      p[2] = q[2] = c[2] = 0.;
    } else if (mode == 3) {
      for (int k = 0; k < 3; ++k)
        q[k] = 2 * c[k] - p[k];
    }
    if (p[0] == q[0] && p[1] == q[1] && p[2] == q[2])
      q[0] += 0.5;
    arcs.insert(arcs.end(), {p[0], p[1], p[2], q[0], q[1], q[2]});
    spheres.insert(spheres.end(), {c[0], c[1], c[2], r2});
    SK::Point_3 a(p[0], p[1], p[2]), b(q[0], q[1], q[2]);
    SK::Line_3 l(a, b);
    SK::Line_arc_3 arc(l, a, b);
    SK::Sphere_3 s(SK::Point_3(c[0], c[1], c[2]), r2);
    using pair_type = std::pair<SK::Circular_arc_point_3, unsigned>;
    std::list<boost::variant<pair_type>> res;
    CGAL::intersection(arc, s, std::back_inserter(res));
    count.push_back(static_cast<std::int64_t>(res.size()));
    int k = 0;
    for (auto & v : res) {
      const auto & pr = boost::get<pair_type>(v);
      mult.push_back(pr.second);
      points.insert(points.end(), {CGAL::to_double(pr.first.x()), CGAL::to_double(pr.first.y()),
                                   CGAL::to_double(pr.first.z())});
      ++k;
    }
    for (; k < 2; ++k) {
      mult.push_back(0);
      points.insert(points.end(), 3, 0.);
    }
  }
  add_f64(w, "input/arcs", {n, 6}, {"sample", "component"}, arcs);
  add_f64(w, "input/spheres", {n, 4}, {"sample", "component"}, spheres);
  add_i64(w, "count", {n}, {"sample"}, count);
  add_i64(w, "multiplicity", {n, 2}, {"sample", "entry"}, mult);
  add_f64(w, "points", {n, 2, 3}, {"sample", "entry", "component"}, points);
  w.finish();
}

// ---------------------------------------------------------------------------
void sphere_intersector_case(const fs::path & root, const std::string & id, Mesh & mesh,
                             const std::vector<std::vector<std::array<double, 4>>> & runs,
                             const std::string & notes) {
  ap::FixtureWriter w(root, "geometry/sphere_intersector/" + id,
                      "MeshSphereIntersector<2, _triangle_3> (compile-fixed copy, UD-409): per "
                      "run constructData() then computeMeshQueryListIntersectionPoint(spheres, "
                      "nb_nodes); intersection_points and new_node_per_elem",
                      PROBE, PROBE_VERSION);
  w.add_tag("geometry");
  w.add_tag("sphere_intersector");
  record_mesh(w, "input", mesh, _triangle_3);
  MeshSphereIntersector<2, _triangle_3> si(mesh);
  std::vector<double> sph, ip;
  std::vector<std::int64_t> run_sizes, status, ip_offsets{0}, nn;
  for (auto & run : runs) {
    std::list<SK::Sphere_3> l;
    for (auto & s : run) {
      l.emplace_back(SK::Point_3(s[0], s[1], s[2]), s[3]);
      sph.insert(sph.end(), s.begin(), s.end());
    }
    run_sizes.push_back(static_cast<std::int64_t>(run.size()));
    si.constructData();
    std::int64_t st = 0;
    try {
      si.computeMeshQueryListIntersectionPoint(l, mesh.getNbNodes());
    } catch (std::exception &) {
      st = 1; // AKANTU_DEBUG_ASSERT (UD-413) or another exception
    }
    status.push_back(st);
    const auto & pts = si.getIntersectionPoints();
    for (Idx i = 0; i < pts.size(); ++i)
      for (Int k = 0; k < 2; ++k)
        ip.push_back(pts(i, k));
    ip_offsets.push_back(static_cast<std::int64_t>(ip.size() / 2));
    const auto & npe = si.getNewNodePerElem();
    for (Idx e = 0; e < npe.size(); ++e)
      for (Int k = 0; k < npe.getNbComponent(); ++k)
        nn.push_back(npe(e, k));
  }
  const std::size_t ne = std::size_t(mesh.getNbElement(_triangle_3));
  add_f64(w, "input/spheres", {sph.size() / 4, 4}, {"sample", "component"}, sph);
  add_i64(w, "input/run_sizes", {run_sizes.size()}, {"entry"}, run_sizes);
  add_i64(w, "status", {status.size()}, {"entry"}, status);
  add_i64(w, "intersection_points/offsets", {ip_offsets.size()}, {"entry"}, ip_offsets,
          "ragged_offsets");
  add_f64(w, "intersection_points/values", {ip.size() / 2, 2}, {"entry", "component"}, ip);
  add_i64(w, "new_node_per_elem", {runs.size(), ne, 5}, {"sample", "element", "component"}, nn);
  // oracle/patches/geometry/0001-* (compile fixes, UD-409) and 0002-* (zero
  // fill, UD-414), applied to probe-local header copies; names + sha256.
  const char * patches = std::getenv("GEOMETRY_PROBE_PATCHES");
  w.parameters()["probe_local_patches"] = std::string(patches ? patches : "unknown");
  w.set_notes(notes);
  w.finish();
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc != 3) {
    std::cerr << "usage: geometry_probe <fixtures_root> <upstream_root>\n";
    return 2;
  }
  const fs::path root = argv[1];
  const fs::path up = argv[2];
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  debug::setDebugLevel(dblError);

  kernel_case(root);
  spherical_case(root);

  // AABB traversal: triangle grid (alternating diagonals: many equal
  // reference points), upstream triangle test mesh, tetrahedron grid
  // (upstream flat-triangle primitives, UD-407).
  {
    Mesh grid(2, "grid");
    triangle_grid(grid, 9, 7, 0.8);
    aabb_case<2, _triangle_3>(root, "triangle_grid", grid, 9, 0x1D8E4E27C47D124FULL);
    Mesh tets(3, "tets");
    tet_grid(tets, 3);
    aabb_case<3, _tetrahedron_4>(root, "tetrahedron_grid", tets, 3, 0x9FB21C651E98DF25ULL);
  }

  // Segment intersector.
  {
    Mesh tri(2, "mesh");
    tri.read((up / "test/test_geometry/test_geometry_triangle.msh").string());
    std::vector<std::vector<K::Segment_3>> b0 = {
        {K::Segment_3(K::Point_3(0, 0.25, 0), K::Point_3(1, 0.25, 0)),
         K::Segment_3(K::Point_3(0.25, 0, 0), K::Point_3(0.25, 1, 0))},
        {K::Segment_3(K::Point_3(0.1, 0.33, 0), K::Point_3(0.1, 0.67, 0))},
        {K::Segment_3(K::Point_3(0, 0, 0), K::Point_3(0.5, 0.5, 0)),
         K::Segment_3(K::Point_3(1, 0, 0), K::Point_3(0, 1, 0)),
         K::Segment_3(K::Point_3(-1, -1, 0), K::Point_3(2, 2, 0))}};
    segment_intersector_case<2, _triangle_3>(
        root, "triangle_test_mesh", tri, b0,
        "upstream test_segment_intersection_triangle_3 queries (+ shared-edge/diagonal ones); "
        "UD-072 work-around: result connectivity created before the queries");
    Mesh grid(2, "grid");
    triangle_grid(grid, 9, 7, 0.8);
    Rng r(0xC2B2AE3D27D4EB4FULL);
    std::vector<std::vector<K::Segment_3>> bg;
    for (int b = 0; b < 4; ++b)
      bg.push_back(queries(r, 2, 60, -0.2, 1.2, 9));
    segment_intersector_case<2, _triangle_3>(root, "triangle_grid", grid, bg,
                                             "UD-408: element order from the libstdc++ set with "
                                             "segmentPairsLess and the AABB traversal order");
    Mesh tet(3, "mesh");
    tet.read((up / "test/test_geometry/test_geometry_tetrahedron.msh").string());
    std::vector<std::vector<K::Segment_3>> b3 = {
        {K::Segment_3(K::Point_3(0, 0, 0), K::Point_3(1, 1, 1))},
        {K::Segment_3(K::Point_3(0.05, 0.05, 0.05), K::Point_3(0.06, 0.06, 0.06))},
        {K::Segment_3(K::Point_3(0.1, 0.1, 0.1), K::Point_3(0.9, 0.9, 0.9))},
        {K::Segment_3(K::Point_3(1, 0, 0), K::Point_3(0, 1, 0))}};
    segment_intersector_case<3, _tetrahedron_4>(
        root, "tetrahedron_test_mesh", tet, b3,
        "upstream test_segment_intersection_tetrahedron_4 queries; UD-407 (flat-triangle "
        "primitives) and UD-072 work-around");
    Mesh tets(3, "tets");
    tet_grid(tets, 3);
    Rng r3(0x165667B19E3779F9ULL);
    std::vector<std::vector<K::Segment_3>> bt;
    for (int b = 0; b < 3; ++b)
      bt.push_back(queries(r3, 3, 40, -0.2, 1.2, 3));
    segment_intersector_case<3, _tetrahedron_4>(root, "tetrahedron_grid", tets, bt,
                                                "UD-407, UD-408");
  }

  // Sphere intersector (compile-fixed copy).
  {
    Mesh tri(2, "mesh");
    tri.read((up / "test/test_geometry/test_geometry_triangle.msh").string());
    std::vector<std::vector<std::array<double, 4>>> runs = {
        {{0., 1., 0., 0.2 * 0.2}, {1., 0., 0., 0.4999999999}},
        {{0.25, -0.125, 0., 0.125 * 0.125}},
        {{0.5, 0.5, 0., 0.09}},
        {{0., 0., 0., 0.5}, {0.5, 0.25, 0., 0.01}}};
    sphere_intersector_case(root, "triangle_test_mesh", tri, runs,
                            "spheres of the disabled (#if 0) block of "
                            "test_segment_intersection_triangle_3 + tangent and node cases; "
                            "UD-409..413");
    Mesh grid(2, "grid");
    triangle_grid(grid, 12, 9, 0.75);
    Rng r(0x27D4EB2F165667C5ULL);
    std::vector<std::vector<std::array<double, 4>>> gr;
    for (int k = 0; k < 40; ++k) {
      std::vector<std::array<double, 4>> run;
      const int m = 1 + r.below(3);
      for (int j = 0; j < m; ++j) {
        const double rad = 0.08 + 0.3 * r.unit();
        run.push_back({r.coord(0, 1), r.coord(0, 0.75), 0., rad * rad});
      }
      gr.push_back(run);
    }
    sphere_intersector_case(root, "triangle_grid", grid, gr, "UD-410..413");
  }

  finalize();
  return 0;
}
