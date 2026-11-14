// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records what the core utilities of src/common produce —
// SpatialGrid (aka_grid_dynamic.hh, incl. a replay of test/test_common/
// test_grid.cc), BBox (aka_bbox.hh, incl. the boxes upstream flags non-empty
// although their bounds are inverted), Math::reduce / NewtonRaphson / pow /
// are_float_equal / intersects / is_in_range (aka_math*), printMemorySize /
// to_lower / trim (aka_common_inline_impl.hh), getVersion (aka_config.cc),
// CSR (aka_csr.hh) and CircularArray (aka_circular_array*.hh).
//
// Usage: core_probe <fixtures_root> <circle.msh>
// Writes the cases core/common/<name>. Built and run by
// oracle/probes/core/generate.sh inside the oracle image against the strict
// install (header templates are instantiated here, so the probe uses the
// strict floating-point flags).

#include "akantu_probe/fixture_writer.hh"

#include <aka_array.hh>
#include <aka_bbox.hh>
#include <aka_circular_array.hh>
#include <aka_common.hh>
#include <aka_csr.hh>
#include <aka_grid_dynamic.hh>
#include <aka_math.hh>
#include <communicator.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>

#include <cctype>
#include <cmath>
#include <cstdint>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <unistd.h>
#include <iostream>
#include <limits>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;

namespace {

constexpr const char * kProbe = "core_probe";
// version 2: adds the cases grid_extent_2d, grid_extent_3d,
// grid_signed_zero_2d, bbox_emptiness and debug_output; the code of the
// version-1 cases is unchanged.
constexpr const char * kProbeVersion = "2";

using i64 = std::int64_t;

/// Deterministic pseudo-random doubles in [lo, hi) (64-bit LCG, the
/// constants of Knuth's MMIX; the values are inputs recorded in the
/// fixtures, so the generator itself is not part of the contract).
struct Lcg {
  std::uint64_t s;
  explicit Lcg(std::uint64_t seed) : s(seed) {}
  double next(double lo, double hi) {
    s = s * 6364136223846793005ULL + 1442695040888963407ULL;
    double u = double(s >> 11) * (1.0 / 9007199254740992.0);
    return lo + (hi - lo) * u;
  }
};

std::string printself_str(const SpatialGrid<Element> & g) {
  std::stringstream ss;
  ss << g << std::endl; // test_grid.cc: std::cout << grid << std::endl
  return ss.str();
}

template <class T> std::string printself_any(const T & g) {
  std::stringstream ss;
  g.printself(ss);
  return ss.str();
}

/// Cells (ids, ragged contents), neighbours and saveAsMesh arrays of a grid.
template <class T, class F>
void write_grid(ap::FixtureWriter & w, const SpatialGrid<T> & grid, Int dim,
                F && item_index) {
  std::vector<i64> ids, offsets{0}, items, neighbors;
  for (auto && cell_id : grid) {
    for (Int d = 0; d < dim; ++d) {
      ids.push_back(cell_id.getID(d));
    }
    const auto & cell = grid.getCell(cell_id);
    for (auto && it : cell) {
      items.push_back(item_index(it));
    }
    offsets.push_back(i64(items.size()));
    auto cid = cell_id; // neighbors() is non-const upstream
    for (auto && n : cid.neighbors()) {
      for (Int d = 0; d < dim; ++d) {
        neighbors.push_back(n.getID(d));
      }
    }
  }
  auto nb_cells = offsets.size() - 1;
  std::size_t nb_neigh = 1;
  for (Int d = 0; d < dim; ++d) {
    nb_neigh *= 3;
  }
  w.add<i64>("cells/ids", {nb_cells, std::size_t(dim)}, {"entry", "spatial_dim"},
             "table", ids)
      .description("cell ids in SpatialGrid iteration order (std::map)");
  w.add<i64>("cells/offsets", {offsets.size()}, {"entry"}, "ragged_offsets",
             offsets);
  w.add<i64>("cells/items", {items.size()}, {"group_member"}, "ragged_values", items)
      .ragged("cells/offsets", "entry")
      .description("items of each cell in insertion order");
  w.add<i64>("cells/neighbors", {nb_cells, nb_neigh, std::size_t(dim)},
             {"entry", "point", "spatial_dim"}, "table", neighbors)
      .description("CellID::neighbors() of every cell");
  const auto & lo = grid.getLowerBounds();
  const auto & up = grid.getUpperBounds();
  std::vector<double> lov(lo.data(), lo.data() + dim),
      upv(up.data(), up.data() + dim);
  w.add<double>("lower_bounds", {std::size_t(dim)}, {"spatial_dim"}, "table", lov);
  w.add<double>("upper_bounds", {std::size_t(dim)}, {"spatial_dim"}, "table", upv);

  // saveAsMesh into an empty mesh
  Mesh mesh(dim, "save");
  grid.saveAsMesh(mesh);
  ElementType type = dim == 1 ? _segment_2 : (dim == 2 ? _quadrangle_4 : _hexahedron_8);
  const auto & nodes = mesh.getNodes();
  std::vector<double> nv(nodes.data(), nodes.data() + nodes.size() * dim);
  w.add<double>("mesh/nodes", {std::size_t(nodes.size()), std::size_t(dim)},
                {"node", "spatial_dim"}, "nodal", nv);
  const auto & conn = mesh.getConnectivity(type);
  std::vector<i64> cv(conn.data(), conn.data() + conn.size() * conn.getNbComponent());
  w.add<i64>("mesh/connectivity",
             {std::size_t(conn.size()), std::size_t(conn.getNbComponent())},
             {"element", "node_local"}, "elemental", cv)
      .element_type(dim == 1 ? "_segment_2" : (dim == 2 ? "_quadrangle_4" : "_hexahedron_8"));
  const auto & tags = mesh.getData<Int>("tag_1", type);
  std::vector<i64> tv(tags.data(), tags.data() + tags.size());
  w.add<i64>("mesh/tag_1", {tv.size()}, {"element"}, "elemental", tv);
}

/* ------------------------------------------------------------------------ */
void case_grid_circle(const std::string & root, const std::string & mesh_path) {
  ap::FixtureWriter w(root, "core/common/grid_circle",
                      "test/test_common/test_grid.cc replay: SpatialGrid<Element> "
                      "of the element barycenters of circle.msh, spacing 0.2, "
                      "centered on the local bounding box; printself and saveAsMesh",
                      kProbe, kProbeVersion);
  const Int spatial_dimension = 2;
  Mesh circle(spatial_dimension);
  circle.read(mesh_path);
  w.add_input_reference("mesh", "fixtures/meshes/gmsh-4.13.1/test/test_common/circle.msh",
                        mesh_path);

  const auto & l = circle.getLocalLowerBounds();
  const auto & u = circle.getLocalUpperBounds();
  Vector<Real> s{.2, .2};
  Vector<Real> c = (u + l) / 2;
  SpatialGrid<Element> grid(spatial_dimension, s, c);

  std::vector<double> barys;
  Vector<Real> bary(spatial_dimension);
  Element el;
  el.ghost_type = _not_ghost;
  std::vector<std::string> types;
  for (const auto & type : circle.elementTypes(spatial_dimension)) {
    Int nb_element = circle.getNbElement(type);
    el.type = type;
    std::stringstream ts;
    ts << type;
    types.push_back(ts.str());
    for (Int e = 0; e < nb_element; ++e) {
      el.element = e;
      circle.getBarycenter(el, bary);
      barys.push_back(bary(0));
      barys.push_back(bary(1));
      grid.insert(el, bary);
    }
  }
  w.parameters()["element_types"] = ap::Json::array_of(types);
  w.parameters()["printself"] = printself_str(grid);
  w.parameters()["type_name"] = "akantu::Element";
  std::vector<double> cv{c(0), c(1)};
  w.add<double>("center", {2}, {"spatial_dim"}, "table", cv);
  w.add<double>("barycenters", {barys.size() / 2, 2}, {"element", "spatial_dim"},
                "elemental", barys)
      .element_type(types.empty() ? "" : types.front());
  write_grid(w, grid, spatial_dimension, [](const Element & e) { return i64(e.element); });
  w.finish();
}

/* ------------------------------------------------------------------------ */
void case_grid_points(const std::string & root, Int dim) {
  std::string name = "core/common/grid_points_" + std::to_string(dim) + "d";
  ap::FixtureWriter w(root, name,
                      "SpatialGrid<Int> of pseudo-random and cell-boundary points "
                      "(non-dyadic spacing and center): cell ids, cell order, "
                      "neighbours, bounds, printself, saveAsMesh",
                      kProbe, kProbeVersion);
  const double sp[3] = {0.3, 0.7, 0.45};
  const double ce[3] = {0.1, -0.25, 1. / 3.};
  Vector<Real> spacing(dim), center(dim);
  for (Int d = 0; d < dim; ++d) {
    spacing(d) = sp[d];
    center(d) = ce[d];
  }
  SpatialGrid<Int> grid(dim, spacing, center);
  Lcg g(1234 + dim);
  std::vector<double> pts;
  const int nb_random = 60;
  for (int p = 0; p < nb_random; ++p) {
    for (Int d = 0; d < dim; ++d) {
      pts.push_back(g.next(-2.0, 2.0));
    }
  }
  // points exactly on cell boundaries (center + k * spacing), and -0.0
  for (int k = -3; k <= 3; ++k) {
    for (Int d = 0; d < dim; ++d) {
      pts.push_back(ce[d] + k * sp[d]);
    }
  }
  for (Int d = 0; d < dim; ++d) {
    pts.push_back(-0.0);
  }
  auto nb_pts = pts.size() / dim;
  std::vector<i64> pids;
  Vector<Real> x(dim);
  for (std::size_t p = 0; p < nb_pts; ++p) {
    for (Int d = 0; d < dim; ++d) {
      x(d) = pts[p * dim + d];
    }
    auto id = grid.getCellID(x);
    for (Int d = 0; d < dim; ++d) {
      pids.push_back(id.getID(d));
    }
    grid.insert(Int(p), x);
  }
  w.add<double>("points", {nb_pts, std::size_t(dim)}, {"point", "spatial_dim"},
                "table", pts)
      .description("inserted positions (inputs), item = point index");
  w.add<double>("spacing", {std::size_t(dim)}, {"spatial_dim"}, "table",
                std::vector<double>(sp, sp + dim));
  w.add<double>("center", {std::size_t(dim)}, {"spatial_dim"}, "table",
                std::vector<double>(ce, ce + dim));
  w.add<i64>("point_cell_ids", {nb_pts, std::size_t(dim)}, {"point", "spatial_dim"},
             "table", pids);
  {
    std::stringstream ss;
    grid.printself(ss);
    w.parameters()["printself"] = ss.str();
  }
  {
    std::stringstream ss;
    grid.printself(ss, 3);
    w.parameters()["printself_indent3"] = ss.str();
  }
  w.parameters()["type_name"] = "int";
  write_grid(w, grid, dim, [](const Int & i) { return i64(i); });
  w.finish();
}

/* ------------------------------------------------------------------------ */
/// SpatialGrid<Int> of explicit points (same quantities as grid_points_*).
void case_grid_set(const std::string & root, const std::string & name,
                   const std::string & description, Int dim,
                   const std::vector<double> & sp, const std::vector<double> & ce,
                   const std::vector<double> & pts) {
  ap::FixtureWriter w(root, "core/common/" + name, description, kProbe,
                      kProbeVersion);
  Vector<Real> spacing(dim), center(dim);
  for (Int d = 0; d < dim; ++d) {
    spacing(d) = sp[d];
    center(d) = ce[d];
  }
  SpatialGrid<Int> grid(dim, spacing, center);
  auto nb_pts = pts.size() / dim;
  std::vector<i64> pids;
  Vector<Real> x(dim);
  for (std::size_t p = 0; p < nb_pts; ++p) {
    for (Int d = 0; d < dim; ++d) {
      x(d) = pts[p * dim + d];
    }
    auto id = grid.getCellID(x);
    for (Int d = 0; d < dim; ++d) {
      pids.push_back(id.getID(d));
    }
    grid.insert(Int(p), x);
  }
  w.add<double>("points", {nb_pts, std::size_t(dim)}, {"point", "spatial_dim"},
                "table", pts)
      .description("inserted positions (inputs), item = point index");
  w.add<double>("spacing", {std::size_t(dim)}, {"spatial_dim"}, "table", sp);
  w.add<double>("center", {std::size_t(dim)}, {"spatial_dim"}, "table", ce);
  w.add<i64>("point_cell_ids", {nb_pts, std::size_t(dim)}, {"point", "spatial_dim"},
             "table", pids);
  {
    std::stringstream ss;
    grid.printself(ss);
    w.parameters()["printself"] = ss.str();
  }
  {
    std::stringstream ss;
    grid.printself(ss, 3);
    w.parameters()["printself_indent3"] = ss.str();
  }
  w.parameters()["type_name"] = "int";
  write_grid(w, grid, dim, [](const Int & i) { return i64(i); });
  w.finish();
}

/// Adversarial grid (non-integral bounding-box extents in directions >= 1,
/// so printself's ceil is discriminated) and a lower bound and mesh
/// nodes equal to -0.0: negative spacing, center -0.0).
void case_grid_sets(const std::string & root) {
  case_grid_set(root, "grid_extent_2d",
                "SpatialGrid<Int> whose bounding box has non-integral extents "
                "(3.0000000000000004 cells) in both directions: printself "
                "nb_cells = prod ceil(extent)",
                2, {0.2, 0.2}, {0., 0.},
                {-0.3, -0.3, 0.1, 0.1, 0.05, -0.35, -0.25, 0.15});
  case_grid_set(root, "grid_extent_3d",
                "SpatialGrid<Int> whose bounding box has non-integral extents "
                "below an integer (6.999999999999999, 6, 5.999999999999999 "
                "cells): printself nb_cells = prod ceil(extent)",
                3, {0.1, 0.3, 0.7}, {0.05, -0.1, 0.2},
                {-0.31, -0.44, -1.3, 0.27, 0.91, 1.95, 0., 0., 0.});
  case_grid_set(root, "grid_signed_zero_2d",
                "SpatialGrid<Int> with a negative spacing and center -0.0: the "
                "lower bound and saveAsMesh node coordinates contain -0.0",
                2, {-0.25, 0.5}, {-0., -0.},
                {-0.1, 0.2, 0.3, -0.7, -0., 0., 0.6, 1.2});
}

/* ------------------------------------------------------------------------ */
void case_bbox(const std::string & root) {
  ap::FixtureWriter w(root, "core/common/bbox",
                      "BBox: construction from points, emptiness, size, center, "
                      "contains, per-direction and full intersects, clamped "
                      "intersection, printself",
                      kProbe, kProbeVersion);
  const Int dim = 2;
  std::vector<std::vector<std::vector<double>>> sets = {
      {},                                     // empty
      {{0., 0.}, {2., 1.}},                   // A
      {{1., -1.}, {3., 0.5}},                 // overlaps A
      {{2., 1.}},                             // touches A at a corner
      {{5., 5.}, {6., 7.}},                   // disjoint
      {{1., 5.}, {3., 6.}},                   // overlaps A in x only
      {{0.1, 0.2}, {0.3, 0.7}, {-0.1, 0.4}},  // non-dyadic
      {{-0., 0.}, {0., -0.}},                 // signed zeros
  };
  std::vector<BBox> boxes;
  std::vector<double> lower, upper, size, center;
  std::vector<i64> empty;
  ap::Json prints = ap::Json::array();
  std::vector<i64> set_offsets{0};
  std::vector<double> set_points;
  for (auto && s : sets) {
    BBox b(dim);
    for (auto && p : s) {
      Vector<Real> v{p[0], p[1]};
      b += v;
      set_points.push_back(p[0]);
      set_points.push_back(p[1]);
    }
    set_offsets.push_back(i64(set_points.size() / 2));
    for (Int d = 0; d < dim; ++d) {
      lower.push_back(b.getLowerBounds()(d));
      upper.push_back(b.getUpperBounds()(d));
      size.push_back(b.size(SpatialDirection(d)));
    }
    Vector<Real> c(dim);
    b.getCenter(c);
    center.push_back(c(0));
    center.push_back(c(1));
    empty.push_back(b.empty() ? 1 : 0);
    prints.push(printself_any(b));
    boxes.push_back(b);
  }
  auto nb = boxes.size();
  w.add<i64>("sets/offsets", {set_offsets.size()}, {"entry"}, "table", set_offsets)
      .description("box b is built from the points sets/points[offsets[b]..offsets[b+1]]");
  w.add<double>("sets/points", {set_points.size() / 2, 2}, {"point", "spatial_dim"},
                "table", set_points);
  w.add<double>("boxes/lower", {nb, 2}, {"entry", "spatial_dim"}, "table", lower);
  w.add<double>("boxes/upper", {nb, 2}, {"entry", "spatial_dim"}, "table", upper);
  w.add<double>("boxes/size", {nb, 2}, {"entry", "spatial_dim"}, "table", size);
  w.add<double>("boxes/center", {nb, 2}, {"entry", "spatial_dim"}, "table", center);
  w.add<i64>("boxes/empty", {nb}, {"entry"}, "table", empty);
  w.parameters()["printself"] = prints;

  std::vector<i64> inter, inter_dir, inter_empty;
  std::vector<double> il, iu;
  for (auto && a : boxes) {
    for (auto && b : boxes) {
      inter.push_back(a.intersects(b) ? 1 : 0);
      for (Int d = 0; d < dim; ++d) {
        inter_dir.push_back(a.intersects(b, SpatialDirection(d)) ? 1 : 0);
      }
      auto i = a.intersection(b);
      inter_empty.push_back(i.empty() ? 1 : 0);
      for (Int d = 0; d < dim; ++d) {
        il.push_back(i.getLowerBounds()(d));
        iu.push_back(i.getUpperBounds()(d));
      }
    }
  }
  w.add<i64>("pairs/intersects", {nb, nb}, {"entry", "sample"}, "table", inter);
  w.add<i64>("pairs/intersects_dir", {nb, nb, 2}, {"entry", "sample", "spatial_dim"},
             "table", inter_dir);
  w.add<i64>("pairs/intersection_empty", {nb, nb}, {"entry", "sample"}, "table",
             inter_empty);
  w.add<double>("pairs/intersection_lower", {nb, nb, 2},
                {"entry", "sample", "spatial_dim"}, "table", il);
  w.add<double>("pairs/intersection_upper", {nb, nb, 2},
                {"entry", "sample", "spatial_dim"}, "table", iu);

  std::vector<double> pts = {0., 0., 2., 1., 2.0000000000000004, 1., 1., 0.5,
                             -1e-300, 0., 0.3, 0.7, 0.1, 0.2, 7., 7.};
  std::vector<i64> contains;
  for (auto && b : boxes) {
    for (std::size_t p = 0; p < pts.size() / 2; ++p) {
      Vector<Real> v{pts[2 * p], pts[2 * p + 1]};
      contains.push_back(b.contains(v) ? 1 : 0);
    }
  }
  w.add<double>("contains/points", {pts.size() / 2, 2}, {"point", "spatial_dim"},
                "table", pts);
  w.add<i64>("contains/result", {nb, pts.size() / 2}, {"entry", "point"}, "table",
             contains);
  // reset() then printself
  BBox r = boxes[1];
  r.reset();
  w.parameters()["printself_after_reset"] = printself_any(r);
  w.finish();
}

/* ------------------------------------------------------------------------ */
/// Boxes upstream flags non-empty (`empty_ = false`) while
/// their bounds stay the inverted empty-box values +-DBL_MAX in some
/// direction, i.e. the cases where upstream's flag and the port's derived
/// emptiness differ, and the queries on them.
void case_bbox_emptiness(const std::string & root) {
  ap::FixtureWriter w(root, "core/common/bbox_emptiness",
                      "BBox built by setLowerBounds/setUpperBounds with a margin on "
                      "an empty box (ContactDetector::constructBoundingBox, empty "
                      "node list), from an empty SpatialGrid (GridSynchronizer), "
                      "from NaN points, by allSum of an empty box; queries against "
                      "ordinary boxes and a box of infinite extent",
                      kProbe, kProbeVersion);
  const Int dim = 2;
  std::vector<BBox> boxes;
  std::vector<std::string> kinds;
  // 0-4: the constructions
  {
    // contact_detector_inline_impl.hh:72-88 with an empty node list
    BBox b(dim);
    const Real max_bb = 0.3;
    Vector<Real> lower_bound = b.getLowerBounds().array() - max_bb;
    Vector<Real> upper_bound = b.getUpperBounds().array() + max_bb;
    b.setLowerBounds(lower_bound);
    b.setUpperBounds(upper_bound);
    boxes.push_back(b);
    kinds.emplace_back("contact_margin_empty_list");
  }
  {
    // grid_synchronizer.cc:47-57 on a grid without cells
    Vector<Real> spacing{0.2, 0.3}, center{0.1, -0.2};
    SpatialGrid<Int> grid(dim, spacing, center);
    BBox b(dim);
    b.setLowerBounds(grid.getLowerBounds() - grid.getSpacing());
    b.setUpperBounds(grid.getUpperBounds() + grid.getSpacing());
    boxes.push_back(b);
    kinds.emplace_back("empty_grid");
  }
  {
    BBox b(dim);
    b += Vector<Real>{NAN, NAN};
    boxes.push_back(b);
    kinds.emplace_back("nan_point");
  }
  {
    BBox b(dim);
    b += Vector<Real>{NAN, 1.};
    boxes.push_back(b);
    kinds.emplace_back("nan_x_point");
  }
  {
    BBox e(dim);
    boxes.push_back(e.allSum(Communicator::getStaticCommunicator()));
    kinds.emplace_back("allsum_of_empty");
  }
  // 5-9: ordinary boxes (built with +=), 10: an empty box
  std::vector<std::vector<std::vector<double>>> sets = {
      {{0., 0.}, {2., 1.}},
      {{1., -1.}, {3., 0.5}},
      {{-1e300, -1e300}, {1e300, 1e300}},
      {{-0., 0.}, {0., -0.}},
      {{0.1, 0.2}, {0.3, 0.7}, {-0.1, 0.4}},
      {},
  };
  std::vector<double> set_points;
  std::vector<i64> set_offsets{0};
  for (auto && st : sets) {
    BBox b(dim);
    for (auto && p : st) {
      b += Vector<Real>{p[0], p[1]};
      set_points.push_back(p[0]);
      set_points.push_back(p[1]);
    }
    set_offsets.push_back(i64(set_points.size() / 2));
    boxes.push_back(b);
    kinds.emplace_back(st.empty() ? "empty" : "points");
  }
  auto nb = boxes.size();
  std::vector<double> lower, upper, size, center;
  std::vector<i64> empty;
  ap::Json prints = ap::Json::array();
  for (auto && b : boxes) {
    for (Int d = 0; d < dim; ++d) {
      lower.push_back(b.getLowerBounds()(d));
      upper.push_back(b.getUpperBounds()(d));
      size.push_back(b.size(SpatialDirection(d)));
    }
    Vector<Real> c(dim);
    b.getCenter(c);
    center.push_back(c(0));
    center.push_back(c(1));
    empty.push_back(b.empty() ? 1 : 0);
    prints.push(printself_any(b));
  }
  ap::Json jk = ap::Json::array();
  for (auto && k : kinds) {
    jk.push(k);
  }
  w.parameters()["kinds"] = jk;
  w.parameters()["max_bb"] = 0.3;
  w.add<i64>("sets/offsets", {set_offsets.size()}, {"entry"}, "table", set_offsets)
      .description("box 5 + s is built from sets/points[offsets[s]..offsets[s+1]]");
  w.add<double>("sets/points", {set_points.size() / 2, 2}, {"point", "spatial_dim"},
                "table", set_points);
  w.add<double>("boxes/lower", {nb, 2}, {"entry", "spatial_dim"}, "table", lower);
  w.add<double>("boxes/upper", {nb, 2}, {"entry", "spatial_dim"}, "table", upper);
  w.add<double>("boxes/size", {nb, 2}, {"entry", "spatial_dim"}, "table", size);
  w.add<double>("boxes/center", {nb, 2}, {"entry", "spatial_dim"}, "table", center);
  w.add<i64>("boxes/empty", {nb}, {"entry"}, "table", empty)
      .description("upstream empty() flag (the port derives emptiness: differs "
                   "for boxes 0-4 by design)");
  w.parameters()["printself"] = prints;

  std::vector<i64> inter, inter_dir, inter_empty;
  std::vector<double> il, iu;
  for (auto && a : boxes) {
    for (auto && b : boxes) {
      inter.push_back(a.intersects(b) ? 1 : 0);
      for (Int d = 0; d < dim; ++d) {
        inter_dir.push_back(a.intersects(b, SpatialDirection(d)) ? 1 : 0);
      }
      auto i = a.intersection(b);
      inter_empty.push_back(i.empty() ? 1 : 0);
      for (Int d = 0; d < dim; ++d) {
        il.push_back(i.getLowerBounds()(d));
        iu.push_back(i.getUpperBounds()(d));
      }
    }
  }
  w.add<i64>("pairs/intersects", {nb, nb}, {"entry", "sample"}, "table", inter);
  w.add<i64>("pairs/intersects_dir", {nb, nb, 2}, {"entry", "sample", "spatial_dim"},
             "table", inter_dir);
  w.add<i64>("pairs/intersection_empty", {nb, nb}, {"entry", "sample"}, "table",
             inter_empty);
  w.add<double>("pairs/intersection_lower", {nb, nb, 2},
                {"entry", "sample", "spatial_dim"}, "table", il);
  w.add<double>("pairs/intersection_upper", {nb, nb, 2},
                {"entry", "sample", "spatial_dim"}, "table", iu);

  const double inf = std::numeric_limits<double>::infinity();
  const double dmax = std::numeric_limits<double>::max();
  std::vector<double> pts = {0., 0., 1., 0.5, 0.2, 0.5, dmax, -dmax, -dmax, dmax,
                             inf, inf, -inf, 0., NAN, 0., 1e300, -1e300};
  std::vector<i64> contains;
  for (auto && b : boxes) {
    for (std::size_t p = 0; p < pts.size() / 2; ++p) {
      Vector<Real> v{pts[2 * p], pts[2 * p + 1]};
      contains.push_back(b.contains(v) ? 1 : 0);
    }
  }
  w.add<double>("contains/points", {pts.size() / 2, 2}, {"point", "spatial_dim"},
                "table", pts);
  w.add<i64>("contains/result", {nb, pts.size() / 2}, {"entry", "point"}, "table",
             contains);

  // a box of infinite extent: only intersects / intersects(dir) are recorded
  // (intersection() with a flagged-non-empty inverted box would call
  // std::clamp with lo > hi, a precondition violation)
  BBox f(dim);
  f += Vector<Real>{-inf, -inf};
  f += Vector<Real>{inf, inf};
  std::vector<i64> fi, fi_dir;
  for (auto && b : boxes) {
    fi.push_back(f.intersects(b) ? 1 : 0);
    fi.push_back(b.intersects(f) ? 1 : 0);
    for (Int d = 0; d < dim; ++d) {
      fi_dir.push_back(f.intersects(b, SpatialDirection(d)) ? 1 : 0);
      fi_dir.push_back(b.intersects(f, SpatialDirection(d)) ? 1 : 0);
    }
  }
  w.add<i64>("infinite/intersects", {nb, 2}, {"entry", "component"}, "table", fi)
      .components({"inf_with_box", "box_with_inf"})
      .description("box [-inf, inf]^2 against every box, both orders");
  w.add<i64>("infinite/intersects_dir", {nb, 2, 2},
             {"entry", "spatial_dim", "component"}, "table", fi_dir)
      .components({"inf_with_box", "box_with_inf"});
  w.finish();
}

/* ------------------------------------------------------------------------ */
struct SqrtFunctor : public Math::NewtonRaphsonFunctor<Real> {
  explicit SqrtFunctor(Real a) : NewtonRaphsonFunctor("sqrt"), a(a) {}
  Real f(const Real & x) const override { return x * x - a; }
  Real f_prime(const Real & x) const override { return 2. * x; }
  Real a;
};

struct CubicFunctor : public Math::NewtonRaphsonFunctor<Real> {
  explicit CubicFunctor(Real b) : NewtonRaphsonFunctor("cubic"), b(b) {}
  Real f(const Real & x) const override { return x * x * x - 2. * x - b; }
  Real f_prime(const Real & x) const override { return 3. * x * x - 2.; }
  Real b;
};

void case_math(const std::string & root) {
  ap::FixtureWriter w(root, "core/common/math",
                      "Math::reduce (sort + pairwise sum), NewtonRaphson, pow<p>, "
                      "pow(x, p), are_float_equal, intersects, is_in_range",
                      kProbe, kProbeVersion);
  // reduce
  Lcg g(99);
  std::vector<i64> offsets{0};
  std::vector<double> inputs, results;
  std::vector<int> lengths = {1, 2, 3, 4, 5, 6, 7, 8, 9, 15, 16, 17, 31, 100, 1001};
  for (int n : lengths) {
    Array<Real> a(n, 1);
    for (int i = 0; i < n; ++i) {
      double v = g.next(-1.0, 1.0);
      if (i % 3 == 0) {
        v *= 1e-8; // mixed magnitudes: the pairwise order is observable
      }
      a(i) = v;
      inputs.push_back(v);
    }
    offsets.push_back(i64(inputs.size()));
    results.push_back(Math::reduce(a));
  }
  w.add<i64>("reduce/offsets", {offsets.size()}, {"sample"}, "ragged_offsets", offsets);
  w.add<double>("reduce/inputs", {inputs.size()}, {"entry"}, "ragged_values", inputs)
      .ragged("reduce/offsets", "sample");
  w.add<double>("reduce/results", {results.size()}, {"sample"}, "table", results);

  // Newton-Raphson: x^2 - a (tol 1e-12, 100 its, x0 = 1) and a cubic
  std::vector<double> nr_a = {2., 3., 10., 0.5, 1e-3, 1234.5678};
  std::vector<double> nr_x;
  std::vector<i64> nr_threw;
  for (double a : nr_a) {
    Math::NewtonRaphson<Real> nr(1e-12, 100);
    try {
      nr_x.push_back(nr.solve(SqrtFunctor(a), 1.));
      nr_threw.push_back(0);
    } catch (debug::Exception &) {
      nr_x.push_back(NAN);
      nr_threw.push_back(1);
    }
  }
  std::vector<double> cub_b = {1., -0.5, 7.25};
  for (double b : cub_b) {
    Math::NewtonRaphson<Real> nr(1e-5, 100); // neo-Hookean C33 settings
    try {
      nr_x.push_back(nr.solve(CubicFunctor(b), 2.));
      nr_threw.push_back(0);
    } catch (debug::Exception &) {
      nr_x.push_back(NAN);
      nr_threw.push_back(1);
    }
  }
  // iteration limit reached (assertion active in the strict build)
  {
    Math::NewtonRaphson<Real> nr(0., 3);
    try {
      nr_x.push_back(nr.solve(SqrtFunctor(2.), 1.));
      nr_threw.push_back(0);
    } catch (debug::Exception &) {
      nr_x.push_back(NAN);
      nr_threw.push_back(1);
    }
  }
  w.add<double>("newton/sqrt_a", {nr_a.size()}, {"sample"}, "table", nr_a);
  w.add<double>("newton/cubic_b", {cub_b.size()}, {"sample"}, "table", cub_b);
  w.add<double>("newton/x", {nr_x.size()}, {"sample"}, "table", nr_x)
      .description("6 sqrt solves (tol 1e-12, 100 its, x0 1), 3 cubic solves "
                   "x^3-2x-b (tol 1e-5, 100 its, x0 2), 1 solve limited to 3 "
                   "iterations with tol 0 (NaN when it threw)");
  w.add<i64>("newton/threw", {nr_threw.size()}, {"sample"}, "table", nr_threw);

  // pow<p>(x) and pow(x, p)
  std::vector<double> px = {1.1, -0.7, 3.000000000000001, 1e-3, 123.456};
  std::vector<double> pv;
  for (double x : px) {
    pv.push_back(Math::pow<0>(x));
    pv.push_back(Math::pow<1>(x));
    pv.push_back(Math::pow<2>(x));
    pv.push_back(Math::pow<3>(x));
    pv.push_back(Math::pow<4>(x));
    pv.push_back(Math::pow<7>(x));
    pv.push_back(Math::pow(x, 5));
    pv.push_back(Math::pow(x, 9));
  }
  w.add<double>("pow/x", {px.size()}, {"sample"}, "table", px);
  w.add<double>("pow/values", {px.size(), 8}, {"sample", "component"}, "table", pv)
      .components({"p0", "p1", "p2", "p3", "p4", "p7", "rt5", "rt9"});

  // are_float_equal with the default tolerance, then 1e-15
  std::vector<double> pairs = {1., 1. + 2.2e-16,  1., 1. + 1e-13,  1., 1. + 3e-14,
                               0., 1e-14,        0., 3e-14,       1e20, 1e20 * (1 + 2e-14),
                               -5., -5.0000000000001, 1e-300, -1e-300};
  std::vector<i64> eq;
  w.parameters()["default_tolerance"] = Math::getTolerance();
  for (std::size_t p = 0; p < pairs.size() / 2; ++p) {
    eq.push_back(Math::are_float_equal(pairs[2 * p], pairs[2 * p + 1]) ? 1 : 0);
  }
  Math::setTolerance(1e-15);
  for (std::size_t p = 0; p < pairs.size() / 2; ++p) {
    eq.push_back(Math::are_float_equal(pairs[2 * p], pairs[2 * p + 1]) ? 1 : 0);
  }
  Math::setTolerance(1e2 * std::numeric_limits<Real>::epsilon());
  w.add<double>("float_equal/pairs", {pairs.size() / 2, 2}, {"sample", "component"},
                "table", pairs);
  w.add<i64>("float_equal/result", {2, pairs.size() / 2}, {"entry", "sample"},
             "table", eq)
      .description("row 0: default tolerance 1e2*eps, row 1: tolerance 1e-15");

  // intersects / is_in_range
  std::vector<double> iv = {0., 1., 1., 2.,  0., 1., 1.5, 2.,  0., 1., -1., 0.,
                            0., 1., -1., -0.5, 2., 1., 0., 3.,  0., 0., 0., 0.};
  std::vector<i64> ir, rr;
  for (std::size_t k = 0; k < iv.size() / 4; ++k) {
    ir.push_back(Math::intersects(iv[4 * k], iv[4 * k + 1], iv[4 * k + 2], iv[4 * k + 3]) ? 1 : 0);
    rr.push_back(Math::is_in_range(iv[4 * k], iv[4 * k + 2], iv[4 * k + 3]) ? 1 : 0);
  }
  w.add<double>("intervals/values", {iv.size() / 4, 4}, {"sample", "component"},
                "table", iv)
      .components({"x_min", "x_max", "y_min", "y_max"});
  w.add<i64>("intervals/intersects", {ir.size()}, {"sample"}, "table", ir);
  w.add<i64>("intervals/is_in_range", {rr.size()}, {"sample"}, "table", rr)
      .description("is_in_range(x_min, y_min, y_max)");
  w.finish();
}

/* ------------------------------------------------------------------------ */
void case_strings(const std::string & root) {
  ap::FixtureWriter w(root, "core/common/strings",
                      "printMemorySize<T>, to_lower, trim, trim(c), getVersion, "
                      "AKANTU_VERSION",
                      kProbe, kProbeVersion);
  std::vector<i64> sizes = {0, 1, 2, 100, 127, 128, 129, 1000, 1024, 131071, 131072,
                            131073, 1048575, 1048576, 3000000, 134217727,
                            134217728, 2000000000};
  ap::Json mem_real = ap::Json::array(), mem_char = ap::Json::array(),
           mem_uint = ap::Json::array();
  for (auto s : sizes) {
    mem_real.push(printMemorySize<Real>(UInt(s)));
    mem_char.push(printMemorySize<char>(UInt(s)));
    mem_uint.push(printMemorySize<UInt>(UInt(s)));
  }
  w.add<i64>("memory/sizes", {sizes.size()}, {"sample"}, "table", sizes);
  w.parameters()["memory_real"] = mem_real;
  w.parameters()["memory_char"] = mem_char;
  w.parameters()["memory_uint"] = mem_uint;
  std::vector<std::string> strs = {"  Hello World \t", "\v\f\r\nX\v", "ALLCAPS_123",
                                   "", "   ", "\"quoted\"", "\"\"x\"\"", "MiXeD.CaSe"};
  ap::Json in = ap::Json::array(), low = ap::Json::array(), tr = ap::Json::array(),
           trq = ap::Json::array();
  for (auto && s : strs) {
    in.push(s);
    low.push(to_lower(s));
    tr.push(trim(s));
    trq.push(trim(s, '"'));
  }
  w.parameters()["strings"] = in;
  w.parameters()["to_lower"] = low;
  w.parameters()["trim"] = tr;
  w.parameters()["trim_quote"] = trq;
  w.parameters()["version"] = std::string(getVersion());
  w.parameters()["version_number"] = AKANTU_VERSION;
  w.parameters()["version_major"] = AKANTU_VERSION_MAJOR;
  w.parameters()["version_minor"] = AKANTU_VERSION_MINOR;
  w.parameters()["version_patch"] = AKANTU_VERSION_PATCH;
  w.parameters()["version_prerelease"] = std::string(AKANTU_VERSION_PRERELEASE);
  w.finish();
}

/* ------------------------------------------------------------------------ */
void case_csr_circular(const std::string & root) {
  ap::FixtureWriter w(root, "core/common/csr_circular",
                      "CSR<Idx> two-pass build with interleaved insertions and "
                      "empty rows; CircularArray<Real> logical order over steps",
                      kProbe, kProbeVersion);
  std::vector<i64> counts = {3, 0, 2, 5, 0, 1, 0};
  CSR<Idx> csr(Int(counts.size()));
  for (std::size_t r = 0; r < counts.size(); ++r) {
    for (i64 k = 0; k < counts[r]; ++k) {
      ++csr.rowOffset(Idx(r));
    }
  }
  csr.countToCSR();
  std::vector<i64> after_count(csr.getRowsOffset().data(),
                               csr.getRowsOffset().data() + counts.size() + 1);
  csr.resizeCols();
  csr.beginInsertions();
  // interleaved insertion order: round robin over the rows
  std::vector<i64> order_rows, order_pos;
  std::vector<i64> left = counts;
  Idx val = 100;
  bool any = true;
  while (any) {
    any = false;
    for (std::size_t r = 0; r < counts.size(); ++r) {
      if (left[r] > 0) {
        order_rows.push_back(i64(r));
        order_pos.push_back(csr.insertInRow(Idx(r), val++));
        --left[r];
        any = true;
      }
    }
  }
  csr.endInsertions();
  std::vector<i64> offsets(csr.getRowsOffset().data(),
                           csr.getRowsOffset().data() + counts.size() + 1);
  std::vector<i64> values(csr.getRows().data(),
                          csr.getRows().data() + csr.getRows().size());
  std::vector<i64> nb_cols;
  for (Int r = 0; r < csr.getNbRows(); ++r) {
    nb_cols.push_back(csr.getNbCols(r));
  }
  w.add<i64>("csr/counts", {counts.size()}, {"entry"}, "table", counts);
  w.add<i64>("csr/offsets_after_count", {after_count.size()}, {"entry"}, "table",
             after_count);
  w.add<i64>("csr/insert_rows", {order_rows.size()}, {"entry"}, "table", order_rows);
  w.add<i64>("csr/insert_positions", {order_pos.size()}, {"entry"}, "table", order_pos)
      .description("return value of insertInRow; inserted values are 100, 101, ...");
  w.add<i64>("csr/offsets", {offsets.size()}, {"entry"}, "table", offsets);
  w.add<i64>("csr/values", {values.size()}, {"entry"}, "table", values);
  w.add<i64>("csr/nb_cols", {nb_cols.size()}, {"entry"}, "table", nb_cols);

  CircularArray<Real> ca(4, 2, 0., "ca");
  for (Int i = 0; i < 4; ++i) {
    for (Int j = 0; j < 2; ++j) {
      ca(i, j) = 10. * i + j + 0.5;
    }
  }
  std::vector<double> seen;
  for (int step = 0; step < 6; ++step) {
    for (Int i = 0; i < 4; ++i) {
      for (Int j = 0; j < 2; ++j) {
        seen.push_back(ca(i, j));
      }
    }
    ca.makeStep();
  }
  w.add<double>("circular/values", {6, 4, 2}, {"step", "entry", "component"}, "table",
                seen)
      .description("logical tuples (i, j) before each of 6 makeStep calls; "
                   "initial (i, j) = 10 i + j + 0.5");
  w.finish();
}

/* ------------------------------------------------------------------------ */
/// Debug messages contain the process id (`<pid>`) and a time stamp
/// (`{microseconds}`), which differ between runs: both are masked (`<PID>`,
/// `{TS}`) so that the three runs of the determinism check are identical.
std::string mask_message(const std::string & s) {
  std::string out;
  std::size_t i = 0;
  while (i < s.size()) {
    char open = s[i];
    if (open == '<' or open == '{') {
      char close = open == '<' ? '>' : '}';
      std::size_t j = i + 1;
      while (j < s.size() and std::isdigit(static_cast<unsigned char>(s[j]))) {
        ++j;
      }
      if (j > i + 1 and j < s.size() and s[j] == close) {
        out += open;
        out += (open == '<') ? "PID" : "TS";
        out += close;
        i = j + 1;
        continue;
      }
    }
    out += s[i];
    ++i;
  }
  return out;
}

/// Upstream's user-visible debug output —
/// Array<T>::printself at several debug levels (address value masked as
/// ADDR), AKANTU_DEBUG_* / AKANTU_ERROR / AKANTU_EXCEPTION messages with
/// prefixes, locations, parallel contexts and module filter, testLevel,
/// printBacktrace layouts, setLogFile.
void case_debug_output(const std::string & root) {
  ap::FixtureWriter w(root, "core/common/debug_output",
                      "aka_error debug output: Array::printself per debug level, "
                      "AKANTU_DEBUG_* messages (prefixes, locations, parallel "
                      "context, module filter), testLevel, printBacktrace layout, "
                      "setLogFile; process ids and time stamps masked",
                      kProbe, kProbeVersion);
  auto & dbg = debug::debugger;
  const DebugLevel saved = debug::getDebugLevel();

  // 1. Array<T>::printself
  ap::Json arrays = ap::Json::array();
  auto record = [&](const std::string & name, const auto & arr, DebugLevel level,
                    int indent, int precision) {
    debug::setDebugLevel(level);
    std::stringstream ss;
    ss.precision(precision);
    arr.printself(ss, indent);
    debug::setDebugLevel(saved);
    std::string t = ss.str();
    const std::string key = " + address        : ";
    auto p = t.find(key);
    if (p != std::string::npos) {
      auto b = p + key.size();
      t.replace(b, t.find('\n', b) - b, "ADDR");
    }
    ap::Json j = ap::Json::object();
    j["name"] = name;
    j["level"] = int(level);
    j["indent"] = indent;
    j["precision"] = precision;
    j["text"] = t;
    arrays.push(j);
  };
  Array<Real> real(4, 2, "real");
  const double inf = std::numeric_limits<double>::infinity();
  std::vector<double> rv = {1. / 3., -0., 2.5e-7, 1e20, NAN, -inf, 123456789., 1e-5};
  for (Int i = 0; i < 4; ++i) {
    for (Int j = 0; j < 2; ++j) {
      real(i, j) = rv[2 * i + j];
    }
  }
  record("real", real, dblTest, 0, 6);
  record("real", real, dblTest, 0, 10);
  record("real", real, dblTest, 3, 6);
  record("real", real, dblDump, 0, 6);
  record("real", real, dblWarning, 0, 6);
  Array<Int> ints(3, 1, "ints");
  ints(0) = 3;
  ints(1) = -4;
  ints(2) = 5;
  record("ints", ints, dblTest, 0, 6);
  Array<UInt> uints(2, 3, "uints");
  for (Int k = 0; k < 6; ++k) {
    uints(k / 3, k % 3) = UInt(k);
  }
  uints(1, 2) = std::numeric_limits<UInt>::max();
  record("uints", uints, dblTest, 0, 6);
  Array<Element> elements(1, 1, "elements");
  elements(0) = Element{_triangle_3, 0, _not_ghost};
  record("elements", elements, dblTest, 0, 6);
  Array<Real> empty(0, 3, "empty");
  record("empty", empty, dblTest, 0, 6);
  Array<Real> pushed(0, 2, "pushed");
  for (int k = 0; k < 3; ++k) {
    pushed.push_back(Vector<Real>{Real(k), Real(k) + 0.5});
  }
  record("pushed", pushed, dblTest, 0, 6);
  w.parameters()["printself"] = arrays;

  // 2. messages on a string stream
  std::stringstream ms;
  dbg.setOutStream(ms);
  dbg.setParallelContext(3, 12);
  debug::setDebugLevel(dblInfo);
  AKANTU_DEBUG_WARNING("value " << 3 << " too large");
  AKANTU_DEBUG_INFO("plain info");
  AKANTU_DEBUG_TRACE("hidden trace");
  AKANTU_DEBUG_IN();
  debug::setDebugLevel(dblDump);
  AKANTU_DEBUG_TRACE("visible trace");
  AKANTU_DEBUG_IN();
  AKANTU_DEBUG_OUT();
  AKANTU_DEBUG(dblAccessory, "accessory " << 9);
  debug::setDebugLevel(dblWarning);
  std::string what, info;
  try {
    AKANTU_EXCEPTION("boom " << 1);
  } catch (debug::Exception & e) {
    what = e.what();
    info = e.info();
  }
  try {
    AKANTU_SILENT_EXCEPTION("quiet");
  } catch (debug::Exception &) {
  }
  try {
    AKANTU_ERROR("fatal " << 2);
  } catch (debug::CriticalError &) {
  }
  // module filter: this file's module is the literal AKANTU_MODULE (not
  // defined for files outside the library)
  dbg.addModuleToDebug("mesh");
  debug::setDebugLevel(dblDump);
  AKANTU_DEBUG_INFO("filtered out");
  AKANTU_DEBUG(dblCritical, "critical ignores the filter");
  dbg.removeModuleToDebug("mesh");
  debug::setDebugLevel(dblWarning);
  ap::Json contexts = ap::Json::array();
  const std::vector<std::pair<int, int>> rs = {{0, 1}, {1, 2}, {7, 10}, {3, 12}, {5, 101}};
  for (auto && [r, sz] : rs) {
    dbg.setParallelContext(r, sz);
    std::stringstream cs;
    dbg.setOutStream(cs);
    AKANTU_DEBUG_WARNING("ctx");
    std::string t = mask_message(cs.str());
    contexts.push(t.substr(0, t.find("{TS}")));
  }
  dbg.setOutStream(ms);
  dbg.setParallelContext(0, 1); // as initialize on one rank
  w.parameters()["messages"] = mask_message(ms.str());
  w.parameters()["message_module"] = std::string(AKANTU_DEBUG_MODULE);
  w.parameters()["exception_what"] = what;
  w.parameters()["exception_info"] = info;
  w.parameters()["contexts"] = contexts;
  w.parameters()["contexts_rank_size"] = ap::Json::array_of(std::vector<int>{0, 1, 1, 2, 7, 10, 3, 12, 5, 101});

  // 3. testLevel(level, module) per debugger level and module filter
  const std::vector<int> levels = {0, 1, 2, 3, 4, 5, 6, 8, 9, 42, 100, 1337};
  const std::vector<int> dlevels = {0, 1, 4, 5, 8, 100, 1337};
  std::vector<i64> tl;
  for (int dl : dlevels) {
    debug::setDebugLevel(DebugLevel(dl));
    for (int filter = 0; filter < 2; ++filter) {
      if (filter == 1) {
        dbg.addModuleToDebug("core");
      }
      for (const char * m : {"core", "mesh"}) {
        for (int l : levels) {
          tl.push_back(dbg.testLevel(DebugLevel(l), m) ? 1 : 0);
        }
      }
      if (filter == 1) {
        dbg.removeModuleToDebug("core");
      }
    }
  }
  debug::setDebugLevel(saved);
  w.add<i64>("test_level/levels", {levels.size()}, {"sample"}, "table",
             std::vector<i64>(levels.begin(), levels.end()));
  w.add<i64>("test_level/debugger_levels", {dlevels.size()}, {"entry"}, "table",
             std::vector<i64>(dlevels.begin(), dlevels.end()));
  w.add<i64>("test_level/result", {dlevels.size(), 2, 2, levels.size()},
             {"entry", "component", "point", "sample"}, "table", tl)
      .description("testLevel(levels[s], module) for debugger level "
                   "debugger_levels[e], filter c (0: none, 1: {core}), module p "
                   "(0: core, 1: mesh)");

  // 4. printBacktrace layouts (written to std::cerr)
  ap::Json bts = ap::Json::array();
  for (std::size_t n : {std::size_t(0), std::size_t(1), std::size_t(10)}) {
    std::vector<std::string> frames;
    for (std::size_t k = 1; k <= n; ++k) {
      frames.push_back("frame" + std::to_string(k));
    }
    std::stringstream es;
    auto * old = std::cerr.rdbuf(es.rdbuf());
    debug::printBacktrace(frames);
    std::cerr.rdbuf(old);
    bts.push(es.str());
  }
  w.parameters()["backtraces"] = bts;

  // 5. setLogFile (kept until the end of the program: upstream closes the
  // file in the debugger's destructor)
  {
    auto path = std::filesystem::temp_directory_path() /
                ("core_probe_log_" + std::to_string(getpid()) + ".txt");
    dbg.setLogFile(path.string());
    AKANTU_DEBUG_WARNING("to the log file");
    dbg.getOutputStream().flush();
    std::ifstream in(path);
    std::stringstream content;
    content << in.rdbuf();
    w.parameters()["log_file"] = mask_message(content.str());
    std::filesystem::remove(path);
  }
  w.finish();
}

} // namespace

int main(int argc, char ** argv) {
  if (argc != 3) {
    std::cerr << "usage: core_probe <fixtures_root> <circle.msh>\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  std::string root = argv[1];
  case_grid_circle(root, argv[2]);
  for (Int d = 1; d <= 3; ++d) {
    case_grid_points(root, d);
  }
  case_grid_sets(root);
  case_bbox(root);
  case_bbox_emptiness(root);
  case_math(root);
  case_strings(root);
  case_csr_circular(root);
  case_debug_output(root); // last: leaves the debugger writing to a log file
  finalize();
  return 0;
}
