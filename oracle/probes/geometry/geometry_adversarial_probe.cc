// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) (src/geometry, parity rows GEOM-001…009): the seven
// adversarial cases of the geometry review, promoted to golden fixtures
// fixtures/geometry/adversarial/*. Derived from the reviewer's
// review_probe.cc: same inputs (same generators and seeds), same upstream Akantu/CGAL calls
// through their public headers, same records; only the output goes through
// akantu_probe::FixtureWriter (one 1-D quantity per record: doubles as f64,
// integers as i32) instead of hex text.
//   kernel_adv      Simple_cartesian<double>: intersection(Segment_3,
//                   Triangle_3), do_intersect(Segment_3, Bbox_3),
//                   do_intersect(Ray_3, Triangle_3), Tetrahedron_3 sides
//   spherical_adv   cgal::Spherical: intersection(Line_arc_3, Sphere_3)
//   aabb_fan/square MeshGeomFactory<2, _triangle_3, Triangle<K>, K> + AABB tree
//   segment_fan/square  MeshSegmentIntersector<2, _triangle_3>
//   sphere_grid10   MeshSphereIntersector<2, _triangle_3> (compile-fixed copy,
//                   oracle/patches/geometry 0001 + 0002, UD-409/UD-414)
//
// Usage: geometry_adversarial_probe <fixtures_root>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>

#include "mesh_geom_common.hh"
#include "mesh_geom_factory.hh"
#include "mesh_segment_intersector.hh"
#include "mesh_sphere_intersector.hh"
#include "tree_type_helper.hh"

#include <CGAL/Spherical_kernel_intersections.h>
#include <CGAL/intersections.h>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <list>
#include <map>
#include <stdexcept>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace fs = std::filesystem;
using K = cgal::Cartesian;
using SK = cgal::Spherical;

namespace {

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
  double unit() { return static_cast<double>(next() >> 11) * 0x1.0p-53; }
  double sym() { return 2. * unit() - 1.; }
  double dyadic(int den, int lo, int hi) { return double(lo + below(hi - lo + 1)) / den; }
};

namespace ap = akantu_probe;

const char * const PROBE = "geometry_adversarial_probe";
const char * const PROBE_VERSION = "1";

/// One fixture case; every record is a 1-D quantity (`entry` axis), exact.
struct Out {
  ap::FixtureWriter w;
  Out(const fs::path & root, const std::string & name, const std::string & description)
      : w(root, "geometry/adversarial/" + name, description, PROBE, PROBE_VERSION) {
    w.add_tag("geometry");
    w.add_tag("adversarial");
    w.add_tag("review_v69");
    w.parameters()["origin"] = std::string(
        "adversarial case of an independent review (its generator review_probe.cc became "
        "oracle/probes/geometry/geometry_adversarial_probe.cc)");
  }
  void d(const std::string & k, const std::vector<double> & v) {
    w.add(k, {v.size()}, {"entry"}, "table", v).tolerance_class("exact");
  }
  void i(const std::string & k, const std::vector<std::int64_t> & v) {
    std::vector<std::int32_t> n;
    n.reserve(v.size());
    for (auto x : v) {
      if (x < INT32_MIN or x > INT32_MAX)
        throw std::runtime_error("value out of i32 range in " + k);
      n.push_back(static_cast<std::int32_t>(x));
    }
    w.add(k, {n.size()}, {"entry"}, "table", n).tolerance_class("exact");
  }
  void finish() { w.finish(); }
};

void push3(std::vector<double> & v, const K::Point_3 & p) {
  v.push_back(p.x());
  v.push_back(p.y());
  v.push_back(p.z());
}

template <class Variant>
void encode(const Variant & var, std::vector<std::int64_t> & kinds, std::vector<double> & coords) {
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

K::Point_3 lerp(const K::Point_3 & a, const K::Point_3 & b, double t) {
  return K::Point_3(a.x() + t * (b.x() - a.x()), a.y() + t * (b.y() - a.y()),
                    a.z() + t * (b.z() - a.z()));
}
K::Point_3 scale(const K::Point_3 & a, double s) {
  return K::Point_3(a.x() * s, a.y() * s, a.z() * s);
}

// ---------------------------------------------------------------------------
// R1: Simple_cartesian<double> kernel with adversarial inputs.
void kernel_adv(const fs::path & out) {
  Out o(out, "kernel_adv",
        "Simple_cartesian<double> on adversarial inputs (8 modes: mixed magnitudes 2^+-80, one "
        "scale 2^+-500 per sample, exactly coplanar tilted plane, segments along edges, through "
        "vertices, slivers, nearly parallel, 1e+-150 scaling): intersection(Segment_3, Triangle_3), "
        "do_intersect(Segment_3, Bbox_3), do_intersect(Ray_3, Triangle_3), Tetrahedron_3 sides");
  Rng r(0x7A3C1D2E9F00B451ULL);
  const int n = 4000;
  std::vector<double> tri, seg, coords, tet, pts;
  std::vector<std::int64_t> kind, bbox_seg, ray_tri, tb, tbd, modes;
  for (int i = 0; i < n; ++i) {
    const int mode = i % 8;
    modes.push_back(mode);
    K::Point_3 a, b, c, p, q;
    auto rp = [&]() { return K::Point_3(r.sym(), r.sym(), r.sym()); };
    auto dy = [&]() { return K::Point_3(r.dyadic(64, -64, 64), r.dyadic(64, -64, 64), 0.); };
    switch (mode) {
    case 0: { // mixed magnitudes inside one sample
      auto mp = [&]() {
        return K::Point_3(r.sym() * std::ldexp(1., r.below(161) - 80),
                          r.sym() * std::ldexp(1., r.below(161) - 80),
                          r.sym() * std::ldexp(1., r.below(161) - 80));
      };
      a = mp(), b = mp(), c = mp(), p = mp(), q = mp();
      break;
    }
    case 1: { // one huge/tiny scale per sample, segment through the interior
      const double s = std::ldexp(1. + r.unit(), r.below(1001) - 500);
      a = scale(rp(), s), b = scale(rp(), s), c = scale(rp(), s);
      K::Point_3 g = CGAL::centroid(a, b, c);
      K::Vector_3 nrm = CGAL::cross_product(b - a, c - a);
      const double t = r.unit();
      p = g + t * nrm, q = g - (1. - t) * nrm;
      break;
    }
    case 2: { // exactly coplanar tilted plane z = (1 - x - 2y)/4, dyadic
      auto pl = [&]() {
        double x = r.dyadic(64, -64, 64), y = r.dyadic(64, -64, 64);
        return K::Point_3(x, y, (1. - x - 2. * y) / 4.);
      };
      a = pl(), b = pl(), c = pl(), p = pl(), q = pl();
      if (r.below(2) == 0) { p = lerp(a, b, 0.5); q = lerp(a, c, 0.5) ; }
      break;
    }
    case 3: { // segment along an edge (sub/over-lapping)
      a = dy(), b = dy(), c = dy();
      if (r.below(2)) { a = K::Point_3(a.x(), a.y(), r.dyadic(8, -8, 8)); }
      const double ts[] = {-0.5, 0., 0.25, 0.5, 1., 1.5, 2.};
      p = lerp(a, b, ts[r.below(7)]);
      q = lerp(a, b, ts[r.below(7)]);
      break;
    }
    case 4: { // through vertices
      a = rp(), b = rp(), c = rp();
      const K::Point_3 cand[] = {a, b, c};
      p = cand[r.below(3)];
      q = (r.below(2) == 0) ? cand[r.below(3)] : rp();
      if (r.below(2) == 0) q = K::Point_3(2 * p.x() - q.x(), 2 * p.y() - q.y(), 2 * p.z() - q.z());
      break;
    }
    case 5: { // sliver triangles
      a = rp(), b = rp();
      K::Point_3 m = CGAL::midpoint(a, b);
      const double eps = std::ldexp(1., -40 - r.below(20));
      c = K::Point_3(std::nextafter(m.x(), 2.), m.y() + (r.below(2) ? eps : 0.), m.z());
      K::Point_3 g = CGAL::centroid(a, b, c);
      p = K::Point_3(g.x(), g.y(), g.z() + 1.), q = K::Point_3(g.x(), g.y(), g.z() - 1.);
      if (r.below(2)) { p = lerp(a, b, -0.25); q = lerp(a, b, 1.25); }
      break;
    }
    case 6: { // nearly parallel / touching the plane z = 0
      a = dy(), b = dy(), c = dy();
      const double tiny[] = {0., 4.9e-324, -4.9e-324, 1e-300, -1e-300, 0x1p-52, -0x1p-52};
      p = K::Point_3(r.dyadic(64, -64, 64), r.dyadic(64, -64, 64), tiny[r.below(7)]);
      q = K::Point_3(r.dyadic(64, -64, 64), r.dyadic(64, -64, 64), tiny[r.below(7)]);
      break;
    }
    default: { // non-power-of-two scaling
      const double s = (r.below(2) ? 1e150 : 1e-150) * (1. + r.unit());
      a = scale(rp(), s), b = scale(rp(), s), c = scale(rp(), s);
      p = scale(rp(), s), q = scale(rp(), s);
      if (r.below(2)) { K::Point_3 g = CGAL::centroid(a, b, c); p = lerp(g, q, -1.); }
    }
    }
    K::Triangle_3 t(a, b, c);
    K::Segment_3 s(p, q);
    push3(tri, a); push3(tri, b); push3(tri, c);
    push3(seg, p); push3(seg, q);
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
    K::Point_3 d = (mode == 2 || mode == 6) ? K::Point_3(a.x(), a.y(), a.z() + 0.5) : rp();
    if (mode == 1 || mode == 7) d = scale(rp(), std::abs(a.x()) + 1.);
    K::Point_3 x;
    switch (r.below(4)) {
    case 0: x = CGAL::midpoint(a, b); break;
    case 1: x = CGAL::centroid(a, b, c); break;
    case 2: x = a; break;
    default: x = lerp(a, d, r.unit());
    }
    K::Tetrahedron_3 th(a, b, c, d);
    push3(tet, d);
    push3(pts, x);
    tb.push_back(th.has_on_bounded_side(x) ? 1 : 0);
    tbd.push_back(th.has_on_boundary(x) ? 1 : 0);
  }
  o.i("modes", modes);
  o.d("tri", tri);
  o.d("seg", seg);
  o.i("kind", kind);
  o.d("coords", coords);
  o.i("bbox_seg", bbox_seg);
  o.i("ray_tri", ray_tri);
  o.d("tet_d", tet);
  o.d("tet_x", pts);
  o.i("tet_bounded", tb);
  o.i("tet_boundary", tbd);
  o.w.parameters()["kind_codes"] =
      ap::Json::raw(R"({"none":0,"point":1,"segment":2,"exception":-1})");
  {
    // D-046 (4): document every NaN of the case (which quantity, where, why).
    std::size_t nan_values = 0, nan_samples = 0;
    std::map<std::int64_t, int> per_mode;
    for (std::size_t s = 0; s < kind.size(); ++s) {
      int k = 0;
      for (int j = 0; j < 6; ++j)
        k += std::isnan(coords[6 * s + j]) ? 1 : 0;
      nan_values += static_cast<std::size_t>(k);
      if (k > 0) {
        ++nan_samples;
        ++per_mode[modes[s]];
      }
    }
    for (double v : tri) if (std::isnan(v)) throw std::runtime_error("NaN input");
    for (double v : seg) if (std::isnan(v)) throw std::runtime_error("NaN input");
    if (nan_values > 0) {
      std::string m;
      for (const auto & [md, c] : per_mode)
        m += (m.empty() ? "" : ", ") + std::string("mode ") + std::to_string(md) + ": " +
             std::to_string(c);
      o.w.set_notes(
          "NaN (decision D-046 (4)): the quantity coords holds " + std::to_string(nan_values) +
          " NaN values in " + std::to_string(nan_samples) + " samples (" + m +
          "; mode 1 = one scale 2^-500..2^500 per sample, mode 5 = sliver triangles, mode 7 = "
          "1e+-150 scaling). They are oracle behaviour: CGAL's Simple_cartesian<double> "
          "construction of the intersection point (Line_3/Plane_3 and Segment_3/Triangle_3 "
          "formulas, no overflow check) overflows to +-inf on these magnitudes and then evaluates "
          "inf - inf, inf / inf or 0 * inf; the port evaluates the same operations. The NaN is "
          "the platform's default NaN (x86_64: 0xfff8000000000000, aarch64: "
          "0x7ff8000000000000); no other quantity and no input contains NaN.");
    }
  }
  o.finish();
}

// ---------------------------------------------------------------------------
// R2: spherical kernel, near tangency, magnitudes, exact end points.
void spherical_adv(const fs::path & out) {
  Out o(out, "spherical_adv",
        "cgal::Spherical intersection(Line_arc_3, Sphere_3) near tangency (r^2 exact +- 1-2 ulps), "
        "at scales 2^+-40 and 1e+-12, end points exactly on the sphere, lines through the centre, "
        "arcs inside or touching from inside: counts, multiplicities, to_double of the points");
  Rng r(0x3C6EF372FE94F82BULL);
  const int n = 3000;
  std::vector<double> arcs, spheres, points;
  std::vector<std::int64_t> count, mult, modes;
  for (int i = 0; i < n; ++i) {
    const int mode = i % 6;
    modes.push_back(mode);
    double p[3], q[3], c[3], r2;
    for (int k = 0; k < 3; ++k) {
      p[k] = r.sym();
      q[k] = r.sym();
      c[k] = r.sym();
    }
    r2 = 0.01 + r.unit();
    if (mode == 0 || mode == 1) { // near tangency (dyadic / non-dyadic distance)
      const double y0 = r.dyadic(8, -8, 8);
      const double dist = (mode == 0) ? r.dyadic(16, 1, 16) : 0.1 * (1 + r.below(9));
      c[0] = r.dyadic(16, -16, 16);
      c[1] = y0 + (r.below(2) ? dist : -dist);
      c[2] = 0.;
      p[1] = q[1] = y0;
      p[2] = q[2] = 0.;
      p[0] = c[0] - 0.75 - r.unit();
      q[0] = c[0] + 0.75 + r.unit();
      if (r.below(3) == 0) { // tilt the line slightly in z
        q[2] = std::ldexp(1., -30 - r.below(20)) * (r.below(2) ? 1 : -1);
      }
      r2 = dist * dist;
      const int k = r.below(5) - 2;
      for (int j = 0; j < (k < 0 ? -k : k); ++j)
        r2 = std::nextafter(r2, k < 0 ? 0. : 10.);
    } else if (mode == 2) { // magnitudes
      const double s = r.below(2) ? std::ldexp(1., r.below(81) - 40) : (r.below(2) ? 1e12 : 1e-12);
      for (int k = 0; k < 3; ++k) {
        p[k] *= s;
        q[k] *= s;
        c[k] *= s;
      }
      r2 *= s * s;
    } else if (mode == 3) { // end points exactly on the sphere (3-4-5)
      const double sc = r.dyadic(8, 1, 16);
      c[0] = r.dyadic(8, -8, 8), c[1] = r.dyadic(8, -8, 8), c[2] = 0.;
      p[0] = c[0] + 3 * sc, p[1] = c[1] + 4 * sc, p[2] = 0.;
      const int w = r.below(3);
      if (w == 0) { q[0] = c[0] - 3 * sc, q[1] = c[1] - 4 * sc; }
      else if (w == 1) { q[0] = c[0] + 4 * sc, q[1] = c[1] - 3 * sc; }
      else { q[0] = c[0], q[1] = c[1]; }
      q[2] = 0.;
      r2 = 25 * sc * sc;
    } else if (mode == 4) { // through the centre, 3-D
      for (int k = 0; k < 3; ++k) q[k] = 2 * c[k] - p[k];
    } else { // arc inside the sphere / touching from inside
      r2 = 4.;
      for (int k = 0; k < 3; ++k) { c[k] = 0.; p[k] *= 0.5; q[k] *= 0.5; }
      if (r.below(2)) { p[0] = 2.; p[1] = 0.; p[2] = 0.; }
    }
    if (p[0] == q[0] && p[1] == q[1] && p[2] == q[2]) q[0] += 0.5;
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
  o.i("modes", modes);
  o.d("arcs", arcs);
  o.d("spheres", spheres);
  o.i("count", count);
  o.i("mult", mult);
  o.d("points", points);
  o.finish();
}

// ---------------------------------------------------------------------------
/// fan of m triangles (centre, rim_i, rim_{i+1}) — every reference point
/// (vertex 0) is the centre — plus `dup` duplicated elements.
void fan_mesh(Mesh & mesh, int m, int dup) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  Vector<Real> v(2);
  v(0) = 0.5, v(1) = 0.5;
  nodes.push_back(v);
  for (int i = 0; i < m; ++i) {
    const double th = 2. * M_PI * double(i) / double(m);
    v(0) = 0.5 + 0.5 * std::cos(th);
    v(1) = 0.5 + 0.5 * std::sin(th);
    nodes.push_back(v);
  }
  auto & conn = acc.getConnectivity(_triangle_3);
  for (int i = 0; i < m; ++i) {
    Vector<Idx> t(3);
    t << 0, 1 + i, 1 + (i + 1) % m;
    conn.push_back(t);
  }
  for (int i = 0; i < dup; ++i) {
    Vector<Idx> t(3);
    t << 0, 1 + (3 * i) % m, 1 + (3 * i + 1) % m;
    conn.push_back(t);
  }
  acc.makeReady();
}

void square_grid(Mesh & mesh, int nx) { // [0,1]^2, all diagonals the same way
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  for (int j = 0; j <= nx; ++j)
    for (int i = 0; i <= nx; ++i) {
      Vector<Real> v(2);
      v(0) = Real(i) / Real(nx);
      v(1) = Real(j) / Real(nx);
      nodes.push_back(v);
    }
  auto id = [&](int i, int j) { return j * (nx + 1) + i; };
  auto & conn = acc.getConnectivity(_triangle_3);
  for (int j = 0; j < nx; ++j)
    for (int i = 0; i < nx; ++i) {
      Vector<Idx> t1(3), t2(3);
      t1 << id(i, j), id(i + 1, j), id(i + 1, j + 1);
      t2 << id(i, j), id(i + 1, j + 1), id(i, j + 1);
      conn.push_back(t1);
      conn.push_back(t2);
    }
  acc.makeReady();
}

void record_mesh(Out & o, const Mesh & mesh) {
  const auto & X = mesh.getNodes();
  o.d("nodes", std::vector<double>(X.data(), X.data() + X.size() * X.getNbComponent()));
  const auto & c = mesh.getConnectivity(_triangle_3);
  std::vector<std::int64_t> cv(c.data(), c.data() + c.size() * c.getNbComponent());
  o.i("conn", cv);
}

std::vector<K::Segment_3> fan_queries(Rng & r, int m, int n) {
  std::vector<K::Segment_3> q;
  for (int i = 0; i < n; ++i) {
    const int mode = i % 4;
    K::Point_3 a, b;
    if (mode == 0) { // through the centre
      const double th = 2. * M_PI * r.unit();
      const double l1 = 0.7 * r.unit(), l2 = 0.7 * r.unit();
      a = K::Point_3(0.5 + l1 * std::cos(th), 0.5 + l1 * std::sin(th), 0.);
      b = K::Point_3(0.5 - l2 * std::cos(th), 0.5 - l2 * std::sin(th), 0.);
    } else if (mode == 1) { // along a spoke (from the centre to a rim node, exact)
      const int k = r.below(m);
      const double th = 2. * M_PI * double(k) / double(m);
      a = K::Point_3(0.5, 0.5, 0.);
      b = K::Point_3(0.5 + 0.5 * std::cos(th), 0.5 + 0.5 * std::sin(th), 0.);
      if (r.below(2)) std::swap(a, b);
    } else if (mode == 2) { // random
      a = K::Point_3(1.4 * r.unit() - 0.2, 1.4 * r.unit() - 0.2, 0.);
      b = K::Point_3(1.4 * r.unit() - 0.2, 1.4 * r.unit() - 0.2, 0.);
    } else { // through two rim nodes (chord along an outer edge) or a diagonal
      const int k = r.below(m);
      const double t0 = 2. * M_PI * double(k) / double(m), t1 = 2. * M_PI * double(k + 1) / double(m);
      a = K::Point_3(0.5 + 0.5 * std::cos(t0), 0.5 + 0.5 * std::sin(t0), 0.);
      b = K::Point_3(0.5 + 0.5 * std::cos(t1), 0.5 + 0.5 * std::sin(t1), 0.);
      if (r.below(2)) { a = K::Point_3(0., 0., 0.); b = K::Point_3(1., 1., 0.); }
    }
    q.emplace_back(a, b);
  }
  return q;
}

// R3: AABB traversal order with all-equal reference points / ties.
void aabb_adv(const fs::path & out, const std::string & id, Mesh & mesh,
              const std::vector<K::Segment_3> & q) {
  Out o(out, "aabb_" + id,
        "MeshGeomFactory<2, _triangle_3, Triangle<K>, K> AABB tree on the " + id +
            " mesh: all_intersections (traversal order, objects) and all_intersected_primitives "
            "of rays; all-equal or tied reference points in every nth_element call");
  record_mesh(o, mesh);
  MeshGeomFactory<2, _triangle_3, Triangle<K>, K> factory(mesh);
  factory.constructData();
  const auto & tree = factory.getTree();
  using result_type =
      typename IntersectionTypeHelper<TreeTypeHelper<Triangle<K>, K>, K::Segment_3>::intersection_type;
  std::vector<double> qv, coords;
  std::vector<std::int64_t> off{0}, ids, kinds, roff{0}, rids, status;
  for (auto & s : q) {
    push3(qv, s.source());
    push3(qv, s.target());
    std::list<result_type> res;
    std::int64_t st = 0;
    try {
      tree.all_intersections(s, std::back_inserter(res));
    } catch (std::exception &) {
      st = 1;
      res.clear();
    }
    status.push_back(st);
    for (auto & x : res) {
      ids.push_back(x->second);
      encode(x->first, kinds, coords);
    }
    off.push_back(static_cast<std::int64_t>(ids.size()));
    std::list<UInt> hit;
    tree.all_intersected_primitives(K::Ray_3(s.source(), s.target()), std::back_inserter(hit));
    for (auto h : hit) rids.push_back(h);
    roff.push_back(static_cast<std::int64_t>(rids.size()));
  }
  o.d("queries", qv);
  o.i("status", status);
  o.i("off", off);
  o.i("ids", ids);
  o.i("kinds", kinds);
  o.d("coords", coords);
  o.i("roff", roff);
  o.i("rids", rids);
  o.finish();
}

// R4: upstream MeshSegmentIntersector on the same meshes (segmentPairsLess).
void segment_adv(const fs::path & out, const std::string & id, Mesh & mesh,
                 const std::vector<std::vector<K::Segment_3>> & batches) {
  Out o(out, "segment_" + id,
        "MeshSegmentIntersector<2, _triangle_3> on the " + id +
            " mesh (shared-edge duplicates: segmentPairsLess and the libstdc++ set, UD-408): "
            "result nodes, connectivity order, associated elements, per-batch status");
  record_mesh(o, mesh);
  Mesh result(2, "interface_mesh");
  result.addConnectivityType(_segment_2, _not_ghost);
  result.addConnectivityType(_segment_2, _ghost);
  result.getElementalData<Element>("associated_element").alloc(0, 1, _segment_2);
  result.getElementalData<std::string>("physical_names").alloc(0, 1, _segment_2);
  MeshSegmentIntersector<2, _triangle_3> intersector(mesh, result);
  std::vector<double> qv;
  std::vector<std::int64_t> sizes, after, status;
  for (std::size_t b = 0; b < batches.size(); ++b) {
    intersector.setPhysicalName("batch_" + std::to_string(b));
    std::list<K::Segment_3> l(batches[b].begin(), batches[b].end());
    std::int64_t st = 0;
    try {
      intersector.buildResultFromQueryList(l);
    } catch (std::exception &) {
      st = 1;
    }
    status.push_back(st);
    for (auto & s : batches[b]) { push3(qv, s.source()); push3(qv, s.target()); }
    sizes.push_back(static_cast<std::int64_t>(batches[b].size()));
    after.push_back(result.getNbElement(_segment_2));
  }
  o.d("queries", qv);
  o.i("batch_sizes", sizes);
  o.i("status", status);
  o.i("after", after);
  const auto & X = result.getNodes();
  o.d("rnodes", std::vector<double>(X.data(), X.data() + X.size() * X.getNbComponent()));
  const auto & c = result.getConnectivity(_segment_2);
  o.i("rconn", std::vector<std::int64_t>(c.data(), c.data() + c.size() * 2));
  const auto & ae = result.getData<Element>("associated_element", _segment_2);
  std::vector<std::int64_t> ael;
  for (Idx e = 0; e < ae.size(); ++e) ael.push_back(ae(e).element);
  o.i("assoc", ael);
  o.finish();
}

// R5: MeshSphereIntersector (patched copy 0001+0002), adversarial runs,
// including the IGFEM pattern (pending points appended to the mesh, then a
// query with nb_old_nodes < nb_nodes).
struct SphereRun {
  std::vector<std::array<double, 4>> spheres;
  bool append_previous; // append the previous run's intersection points
  bool nb_old_is_original;
};

void sphere_adv(const fs::path & out, const std::string & id, Mesh & mesh,
                const std::vector<SphereRun> & runs) {
  Out o(out, "sphere_" + id,
        "MeshSphereIntersector<2, _triangle_3> (compile-fixed copy, patches 0001+0002) on a grid "
        "with nodes at i/10: spheres through nodes and 5e-11/2e-12/1e-13 off (UD-411), tangents "
        "(UD-412), the IGFEM pattern (pending points appended, nb_old_nodes < node count), "
        "concentric spheres, an element crossed too often (UD-413)");
  record_mesh(o, mesh);
  const Int nb_original = mesh.getNbNodes();
  MeshSphereIntersector<2, _triangle_3> si(mesh);
  std::vector<double> sph, ip, appended;
  std::vector<std::int64_t> run_sizes, status, ip_off{0}, nn, nb_old_v, nb_nodes_v, app_off{0};
  std::vector<double> prev;
  for (auto & run : runs) {
    if (run.append_previous) {
      MeshAccessor acc(mesh);
      auto & nodes = acc.getNodes();
      for (std::size_t k = 0; k + 1 < prev.size(); k += 2) {
        Vector<Real> v(2);
        v(0) = prev[k], v(1) = prev[k + 1];
        nodes.push_back(v);
        appended.push_back(prev[k]);
        appended.push_back(prev[k + 1]);
      }
    }
    app_off.push_back(static_cast<std::int64_t>(appended.size() / 2));
    std::list<SK::Sphere_3> l;
    for (auto & s : run.spheres) {
      l.emplace_back(SK::Point_3(s[0], s[1], s[2]), s[3]);
      sph.insert(sph.end(), s.begin(), s.end());
    }
    run_sizes.push_back(static_cast<std::int64_t>(run.spheres.size()));
    si.constructData();
    const Int nb_old = run.nb_old_is_original ? nb_original : mesh.getNbNodes();
    nb_old_v.push_back(nb_old);
    nb_nodes_v.push_back(mesh.getNbNodes());
    std::int64_t st = 0;
    try {
      si.computeMeshQueryListIntersectionPoint(l, nb_old);
    } catch (std::exception &) {
      st = 1;
    }
    status.push_back(st);
    const auto & pts = si.getIntersectionPoints();
    prev.clear();
    for (Idx i = 0; i < pts.size(); ++i)
      for (Int k = 0; k < 2; ++k) {
        ip.push_back(pts(i, k));
        prev.push_back(pts(i, k));
      }
    ip_off.push_back(static_cast<std::int64_t>(ip.size() / 2));
    const auto & npe = si.getNewNodePerElem();
    for (Idx e = 0; e < npe.size(); ++e)
      for (Int k = 0; k < npe.getNbComponent(); ++k) nn.push_back(npe(e, k));
  }
  o.d("spheres", sph);
  o.i("run_sizes", run_sizes);
  o.i("app_off", app_off);
  o.d("appended", appended);
  o.i("nb_old", nb_old_v);
  o.i("nb_nodes", nb_nodes_v);
  o.i("status", status);
  o.i("ip_off", ip_off);
  o.d("ip", ip);
  o.i("nn", nn);
  o.finish();
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc != 2) {
    std::cerr << "usage: geometry_adversarial_probe <fixtures_root>\n";
    return 2;
  }
  const fs::path out = argv[1];
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  debug::setDebugLevel(dblError);

  kernel_adv(out);
  spherical_adv(out);
  {
    Rng r(0x1F83D9ABFB41BD6BULL);
    Mesh fan(2, "fan");
    fan_mesh(fan, 24, 9);
    auto q = fan_queries(r, 24, 240);
    aabb_adv(out, "fan", fan, q);
    std::vector<std::vector<K::Segment_3>> batches;
    for (int b = 0; b < 4; ++b) batches.push_back(fan_queries(r, 24, 50));
    segment_adv(out, "fan", fan, batches);
    Mesh sq(2, "square");
    square_grid(sq, 16);
    std::vector<K::Segment_3> qs;
    for (int i = 0; i < 240; ++i) {
      const int mode = i % 3;
      if (mode == 0) { // along grid lines / diagonals (shared edges)
        const double c = double(r.below(17)) / 16.;
        if (r.below(2)) qs.emplace_back(K::Point_3(-0.1, c, 0.), K::Point_3(1.1, c, 0.));
        else qs.emplace_back(K::Point_3(0., 0., 0.), K::Point_3(1., 1., 0.));
      } else if (mode == 1) { // through nodes
        const double cx = double(r.below(17)) / 16., cy = double(r.below(17)) / 16.;
        const double dx = r.sym() * 0.3, dy = r.sym() * 0.3;
        qs.emplace_back(K::Point_3(cx - dx, cy - dy, 0.), K::Point_3(cx + dx, cy + dy, 0.));
      } else {
        qs.emplace_back(K::Point_3(1.4 * r.unit() - 0.2, 1.4 * r.unit() - 0.2, 0.),
                        K::Point_3(1.4 * r.unit() - 0.2, 1.4 * r.unit() - 0.2, 0.));
      }
    }
    aabb_adv(out, "square", sq, qs);
    std::vector<std::vector<K::Segment_3>> sb;
    for (int b = 0; b < 3; ++b)
      sb.emplace_back(qs.begin() + 80 * b, qs.begin() + 80 * (b + 1));
    segment_adv(out, "square", sq, sb);
  }
  {
    Mesh grid(2, "grid");
    square_grid(grid, 10); // nodes at i/10 (not dyadic)
    const double c5 = 0.5;
    std::vector<SphereRun> runs = {
        // through nodes (0.7,0.5), (0.3,0.5), (0.5,0.7), (0.5,0.3)
        {{{c5, c5, 0., 0.2 * 0.2}}, false, false},
        // 5e-11, 2e-12, 1e-13 away from those nodes (UD-411 1e-10 vs 1e-12)
        {{{c5, c5, 0., (0.2 + 5e-11) * (0.2 + 5e-11)}}, false, false},
        {{{c5, c5, 0., (0.2 + 2e-12) * (0.2 + 2e-12)}}, false, false},
        {{{c5, c5, 0., (0.2 + 1e-13) * (0.2 + 1e-13)}}, false, false},
        // tangent to the horizontal edge y = 0.5 at x = 0.55 (UD-412)
        {{{0.55, 0.75, 0., 0.0625}}, false, false},
        // tangent at a node from outside: centre (0.5, 0.75), r = 0.25 -> (0.5, 0.5)
        {{{0.5, 0.75, 0., 0.0625}}, false, false},
        // IGFEM pattern: a sphere, then the pending points appended and the same
        // sphere (plus a slightly bigger one) with nb_old_nodes = original count
        {{{0.43, 0.37, 0., 0.0729}}, false, false},
        {{{0.43, 0.37, 0., 0.0729}, {0.43, 0.37, 0., 0.0729 * (1. + 1e-11)}}, true, true},
        // two concentric spheres in one run (pending-point reuse and numbering)
        {{{0.21, 0.66, 0., 0.04}, {0.21, 0.66, 0., 0.04 * (1. + 3e-11)}}, false, false},
        // many small spheres crossing the same element (UD-413)
        {{{0.05, 0.05, 0., 0.0016}, {0.051, 0.05, 0., 0.0017}, {0.05, 0.052, 0., 0.0018}}, false,
         false},
    };
    sphere_adv(out, "grid10", grid, runs);
  }
  finalize();
  return 0;
}
