// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) (parity rows EMB-001…005, TEST-125…127, GOLD-008):
// runs Akantu's EmbeddedInterfaceModel compiled from the reference sources
// with the patched oracle variant `embedded` (oracle/patches/embedded,
// D-060, docs/ORACLE_BUILD.md) and writes golden fixtures
// fixtures/model/embedded/<case> through akantu_probe::FixtureWriter.
// Derived from the review's emb_probe.cc: same setups
// for t125/t126/oblique/t127 (those of upstream's
// test/test_model/test_solid_mechanics_model/test_embedded_interface/
// test_embedded_element_matrix.cc, test_embedded_interface_model.cc,
// test_embedded_interface_model_prestress.cc), plus the review's
// adversarial and grazing bars (meshes and bars of
// v82_independent_checks.rs) and a `_triangle_6` background.
//
// Every case has ONE reinforcement law and ONE background element type:
// UD-590 (left unpatched in the variant, as in every released version)
// makes any other configuration undefined behaviour upstream
// (computeBackgroundShapeDerivatives writes the shape derivatives at the
// interface element id, the residual zips the law-wide integrant with the
// per-type shape derivatives and akantu's zip stops at the first range).
//
// Pins: Eigen cache sizes 16384/524288/524288 (D-050; the pinned install
// has the same compile-time defaults), the `_eigen` sparse solver (the
// oracle's `_auto` is MUMPS).
//
// Usage: embedded_probe <case> <data dir> <fixtures root>
//   data dir: copies of the upstream test inputs (meshes, .dat, matrix)

#include "akantu_probe/fixture_writer.hh"

#include "embedded_interface_model.hh"
#include "mesh_accessor.hh"
#include "mesh_geom_common.hh"
#include "non_linear_solver.hh"
#include "sparse_matrix_aij.hh"

#include <CGAL/intersections.h>
#include <Eigen/Core>

#include <array>
#include <cmath>
#include <cstdint>
#include <cstdio>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <functional>
#include <map>
#include <sstream>
#include <stdexcept>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

const char * const PROBE = "embedded_probe";
const char * const PROBE_VERSION = "1";

/// Deterministic pseudo-random numbers in [-1, 1) (64-bit LCG of the review's
/// v82_independent_checks.rs; the Rust replay uses the same).
struct Lcg {
  std::uint64_t s;
  explicit Lcg(std::uint64_t seed) : s(seed) {}
  double next() {
    s = s * 6364136223846793005ULL + 1442695040888963407ULL;
    return static_cast<double>(s >> 11) / static_cast<double>(1ULL << 53) * 2.0 - 1.0;
  }
};

using Bar = std::vector<std::vector<Real>>;

/* -------------------------------------------------------------------------- */
/* meshes (same node/element order as crates/akantu-embedded/tests/common)    */
/* -------------------------------------------------------------------------- */
void name_elements(Mesh & mesh, ElementType type, const std::string & name) {
  auto & names = mesh.getDataPointer<std::string>("physical_names", type, _not_ghost, 1,
                                                  true, false);
  for (auto & n : names) {
    n = name;
  }
}

/// nx × ny cells over [0, lx] × [0, ly], two _triangle_3 per cell (diagonal
/// from the lower left corner).
void grid_triangles(Mesh & mesh, int nx, int ny, Real lx, Real ly) {
  {
    MeshAccessor acc(mesh);
    auto & nodes = acc.getNodes();
    for (int j = 0; j <= ny; ++j) {
      for (int i = 0; i <= nx; ++i) {
        Vector<Real> v(2);
        v(0) = lx * Real(i) / Real(nx);
        v(1) = ly * Real(j) / Real(ny);
        nodes.push_back(v);
      }
    }
    auto id = [&](int i, int j) { return Idx(j * (nx + 1) + i); };
    auto & conn = acc.getConnectivity(_triangle_3);
    for (int j = 0; j < ny; ++j) {
      for (int i = 0; i < nx; ++i) {
        Vector<Idx> t1(3), t2(3);
        t1 << id(i, j), id(i + 1, j), id(i + 1, j + 1);
        t2 << id(i, j), id(i + 1, j + 1), id(i, j + 1);
        conn.push_back(t1);
        conn.push_back(t2);
      }
    }
    acc.makeReady();
  }
  name_elements(mesh, _triangle_3, "concrete");
}

/// n × n cells over [0, 1]², two _triangle_6 per cell (corners as
/// grid_triangles; mid-edge nodes 3, 4, 5 between corners (0,1), (1,2), (2,0))
/// on the (2n + 1)² lattice.
void grid_triangles6(Mesh & mesh, int n) {
  {
    MeshAccessor acc(mesh);
    auto & nodes = acc.getNodes();
    const int g = 2 * n + 1;
    for (int j = 0; j < g; ++j) {
      for (int i = 0; i < g; ++i) {
        Vector<Real> v(2);
        v(0) = Real(i) / Real(2 * n);
        v(1) = Real(j) / Real(2 * n);
        nodes.push_back(v);
      }
    }
    auto id = [&](int i, int j) { return Idx(j * g + i); };
    auto & conn = acc.getConnectivity(_triangle_6);
    for (int j = 0; j < n; ++j) {
      for (int i = 0; i < n; ++i) {
        const int a = 2 * i, b = 2 * j;
        Vector<Idx> t1(6), t2(6);
        t1 << id(a, b), id(a + 2, b), id(a + 2, b + 2), id(a + 1, b), id(a + 2, b + 1),
            id(a + 1, b + 1);
        t2 << id(a, b), id(a + 2, b + 2), id(a, b + 2), id(a + 1, b + 1), id(a + 1, b + 2),
            id(a, b + 1);
        conn.push_back(t1);
        conn.push_back(t2);
      }
    }
    acc.makeReady();
  }
  name_elements(mesh, _triangle_6, "concrete");
}

/// n³ cubes over [0, l]³, six positively oriented _tetrahedron_4 per cube
/// (Kuhn subdivision along the main diagonal).
void grid_tetrahedra(Mesh & mesh, int n, Real l) {
  {
    MeshAccessor acc(mesh);
    auto & nodes = acc.getNodes();
    std::vector<std::array<Real, 3>> coords;
    for (int k = 0; k <= n; ++k) {
      for (int j = 0; j <= n; ++j) {
        for (int i = 0; i <= n; ++i) {
          std::array<Real, 3> x{l * Real(i) / Real(n), l * Real(j) / Real(n),
                                l * Real(k) / Real(n)};
          Vector<Real> v(3);
          v(0) = x[0];
          v(1) = x[1];
          v(2) = x[2];
          nodes.push_back(v);
          coords.push_back(x);
        }
      }
    }
    auto id = [&](int i, int j, int k) { return Idx((k * (n + 1) + j) * (n + 1) + i); };
    const int perms[6][3] = {{0, 1, 2}, {0, 2, 1}, {1, 0, 2}, {1, 2, 0}, {2, 0, 1}, {2, 1, 0}};
    auto & conn = acc.getConnectivity(_tetrahedron_4);
    for (int k = 0; k < n; ++k) {
      for (int j = 0; j < n; ++j) {
        for (int i = 0; i < n; ++i) {
          for (auto & p : perms) {
            int b[3] = {i, j, k};
            std::array<Idx, 4> tet{};
            tet[0] = id(b[0], b[1], b[2]);
            for (int s = 0; s < 3; ++s) {
              b[p[s]] += 1;
              tet[s + 1] = id(b[0], b[1], b[2]);
            }
            auto x = [&](Idx t) { return coords[std::size_t(t)]; };
            auto a = x(tet[0]), bb = x(tet[1]), cc = x(tet[2]), d = x(tet[3]);
            Real e1[3] = {bb[0] - a[0], bb[1] - a[1], bb[2] - a[2]};
            Real e2[3] = {cc[0] - a[0], cc[1] - a[1], cc[2] - a[2]};
            Real e3[3] = {d[0] - a[0], d[1] - a[1], d[2] - a[2]};
            Real det = e1[0] * (e2[1] * e3[2] - e2[2] * e3[1]) -
                       e1[1] * (e2[0] * e3[2] - e2[2] * e3[0]) +
                       e1[2] * (e2[0] * e3[1] - e2[1] * e3[0]);
            if (det < 0.) {
              std::swap(tet[2], tet[3]);
            }
            Vector<Idx> t(4);
            t << tet[0], tet[1], tet[2], tet[3];
            conn.push_back(t);
          }
        }
      }
    }
    acc.makeReady();
  }
  name_elements(mesh, _tetrahedron_4, "concrete");
}

/// Reinforcement (primitive) mesh: every bar is a polyline of `_segment_2`
/// (own nodes, no sharing between bars), all named `name`.
void reinforcement(Mesh & rm, Int dim, const std::vector<Bar> & bars,
                   const std::string & name = "reinforcement") {
  std::vector<std::array<Idx, 2>> segs;
  for (auto && bar : bars) {
    const Idx first = rm.getNbNodes();
    for (auto && p : bar) {
      Vector<Real> v(dim);
      for (Int i = 0; i < dim; ++i) {
        v(i) = p[i];
      }
      rm.getNodes().push_back(v);
    }
    for (Idx k = 0; k + 1 < Idx(bar.size()); ++k) {
      segs.push_back({first + k, first + k + 1});
    }
  }
  rm.addConnectivityType(_segment_2);
  for (auto && s : segs) {
    rm.getConnectivity(_segment_2).push_back(Vector<Idx>({s[0], s[1]}));
  }
  rm.getElementalData<std::string>("physical_names").alloc(Int(segs.size()), 1, _segment_2);
  auto & names = rm.getData<std::string>("physical_names")(_segment_2);
  for (Int k = 0; k < Int(segs.size()); ++k) {
    names(k) = name;
  }
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

/* -------------------------------------------------------------------------- */
/* fixture output                                                             */
/* -------------------------------------------------------------------------- */
struct Out {
  ap::FixtureWriter w;
  Out(const fs::path & root, const std::string & name, const std::string & description)
      : w(root, "model/embedded/" + name, description, PROBE, PROBE_VERSION) {
    w.add_tag("embedded");
    w.add_tag("oracle_variant");
    w.parameters()["oracle_variant"] = std::string("embedded");
    const char * sha = std::getenv("EMBEDDED_VARIANT_SHA256");
    w.parameters()["oracle_variant_patches_sha256"] = std::string(sha ? sha : "unknown");
    w.parameters()["oracle_variant_patches"] = std::string(
        "oracle/patches/embedded/0001…0008 (docs/ORACLE_BUILD.md; D-060)");
    w.parameters()["eigen_cache_sizes_runtime_pin"] = std::string(
        "Eigen::setCpuCacheSizes(16384, 524288, 524288) at the start of main (D-050; the "
        "cache-size static is a preemptible global of libakantu, so the library's SparseLU "
        "uses it)");
    auto env = [](const char * n) {
      const char * v = std::getenv(n);
      return std::string(n) + "=" + (v ? v : "unset");
    };
    w.parameters()["blas_environment"] = env("OPENBLAS_CORETYPE") + " " +
                                         env("OPENBLAS_NUM_THREADS") + " " +
                                         env("OMP_NUM_THREADS");
  }
  void f64(const std::string & k, const std::vector<std::size_t> & shape,
           const std::vector<std::string> & axes, const std::string & layout,
           const std::vector<double> & v) {
    w.add(k, shape, axes, layout, v).tolerance_class("exact");
  }
  void i32(const std::string & k, const std::vector<std::size_t> & shape,
           const std::vector<std::string> & axes, const std::string & layout,
           const std::vector<std::int32_t> & v) {
    w.add(k, shape, axes, layout, v).tolerance_class("exact");
  }
};

std::int32_t to_i32(Idx x) {
  if (x < INT32_MIN or x > INT32_MAX) {
    throw std::runtime_error("index out of i32 range");
  }
  return static_cast<std::int32_t>(x);
}

void dump_interface(Out & o, EmbeddedInterfaceModel & model, Int dim) {
  auto & im = model.getInterfaceMesh();
  const auto & X = im.getNodes();
  std::vector<double> nodes(X.data(), X.data() + X.size() * X.getNbComponent());
  o.f64("interface/nodes", {std::size_t(X.size()), std::size_t(dim)}, {"node", "component"},
        "table", nodes);
  std::vector<std::int32_t> conn, bg;
  std::int32_t bg_type = -1;
  if (im.getConnectivities().exists(_segment_2, _not_ghost)) {
    const auto & c = im.getConnectivity(_segment_2);
    const auto & assoc = model.getInterfaceAssociatedElements(_segment_2);
    for (Int e = 0; e < c.size(); ++e) {
      conn.push_back(to_i32(c(e, 0)));
      conn.push_back(to_i32(c(e, 1)));
      bg.push_back(to_i32(assoc(e).element));
      bg_type = std::int32_t(assoc(e).type);
    }
  }
  o.i32("interface/connectivity", {conn.size() / 2, 2}, {"element", "node_local"}, "table",
        conn);
  o.i32("interface/background_element", {bg.size()}, {"element"}, "table", bg);
  std::ostringstream t;
  if (bg_type >= 0) {
    t << ElementType(bg_type);
  }
  o.w.parameters()["interface_background_type"] = t.str();
}

void dump_k(Out & o, EmbeddedInterfaceModel & model) {
  model.assembleStiffnessMatrix(true);
  // const access: entries outside the profile read as 0 (symmetric storage
  // answered for both triangles)
  const auto & K =
      dynamic_cast<const SparseMatrixAIJ &>(model.getDOFManager().getMatrix("K"));
  const Int n = model.getDOFManager().getSystemSize();
  std::vector<std::int32_t> rows, cols;
  std::vector<double> vals;
  for (Int i = 0; i < n; ++i) {
    for (Int j = 0; j < n; ++j) {
      const Real v = K(i, j);
      if (v != 0.) {
        rows.push_back(to_i32(i));
        cols.push_back(to_i32(j));
        vals.push_back(v);
      }
    }
  }
  o.i32("K/rows", {rows.size()}, {"nnz"}, "matrix", rows);
  o.i32("K/cols", {cols.size()}, {"nnz"}, "matrix", cols);
  o.f64("K/values", {vals.size()}, {"nnz"}, "matrix", vals);
  o.w.parameters()["K_size"] = static_cast<long long>(n);
}

void dump_nodal(Out & o, const std::string & k, const Array<Real> & a) {
  std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
  o.f64(k, {std::size_t(a.size()), std::size_t(a.getNbComponent())}, {"node", "component"},
        "nodal", v);
}

void dump_energies(Out & o, EmbeddedInterfaceModel & model) {
  o.f64("energy/potential", {1}, {"entry"}, "table", {model.getEnergy("potential")});
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & law = model.getMaterial(m);
    o.f64("energy/law/" + law.getName(), {1}, {"entry"}, "table",
          {law.getEnergy("potential")});
  }
}

void write_file(const std::string & path, const std::string & text) {
  std::ofstream f(path);
  f << text;
  if (not f) {
    throw std::runtime_error("cannot write " + path);
  }
}

/// Input file with a bulk law and ONE reinforcement law (`reinforcement`).
std::string dat(Real e_bulk, Real nu_bulk, Real e, Real area, Real pre) {
  std::ostringstream s;
  s.precision(17);
  s << "material elastic [\n name = concrete\n E = " << e_bulk << "\n nu = " << nu_bulk
    << "\n]\n"
    << "material reinforcement elastic [\n name = reinforcement\n E = " << e
    << "\n area = " << area << "\n pre_stress = " << pre << "\n]\n";
  return s.str();
}

/// Random nodal displacement field (LCG, amplitude 1e-3) on every dof.
void random_field(EmbeddedInterfaceModel & model, std::uint64_t seed) {
  Lcg rng(seed);
  auto & u = model.getDisplacement();
  Real * d = u.data();
  for (Int i = 0; i < u.size() * u.getNbComponent(); ++i) {
    d[i] = 1e-3 * rng.next();
  }
}

/* -------------------------------------------------------------------------- */
/* review bars */
/* -------------------------------------------------------------------------- */
std::vector<Bar> bars_3d() {
  const Real third = 1.0 / 3.0;
  const Real vtx[3] = {third, 2.0 * third, third};
  const Real dirn[3] = {0.6, -0.48, 0.64};
  return {
      {{0.0, 0.0, 0.0}, {1.0, 1.0, 1.0}},                 // diag_edge
      {{0.05, 0.1, third}, {0.93, 0.77, third}},          // in_face
      {{vtx[0] - 0.3 * dirn[0], vtx[1] - 0.3 * dirn[1], vtx[2] - 0.3 * dirn[2]},
       {vtx[0] + 0.35 * dirn[0], vtx[1] + 0.35 * dirn[1], vtx[2] + 0.35 * dirn[2]}},
                                                          // thru_vertex
      {{-0.3, 0.4, 0.5}, {0.6, 0.55, 0.45}},              // outside
      {{0.21, 0.05, 0.03}, {0.26, 0.07, 0.04}},           // inside
      {{0.0, 0.0, 0.0}, {1.0, 0.0, 0.0}},                 // bnd_edge
      {{0.0, 0.13, 0.91}, {0.0, 0.88, 0.07}},             // bnd_face
      {{0.9, 0.1, 0.2}, {2.0 * third, third, 2.0 * third}, {0.15, 0.95, 0.8}}, // polyline
      {{0.07, 0.11, 0.0}, {0.89, 0.71, 0.0}},             // in the face z = 0 (UD-407)
  };
}

std::vector<Bar> bars_2d() {
  return {
      {{0.0, 0.0}, {1.0, 1.0}},   // diag_edge
      {{0.0, 1.0}, {1.0, 0.0}},   // anti_diag
      {{0.1, 0.5}, {0.9, 0.5}},   // grid_line
      {{-0.2, 0.3}, {0.55, 0.62}}, // outside
      {{0.03, 0.87}, {0.97, 0.11}}, // oblique
      {{0.30, 0.05}, {0.33, 0.07}}, // inside
  };
}

/// Case `grazing`: through the vertex (1/3, 2/3, 1/3) of the Kuhn
/// grid at the angle delta to the grid plane z = 1/3.
Bar grazing_3d(Real delta) {
  const Real third = 1.0 / 3.0;
  const Real c[3] = {third, 2.0 * third, third};
  const Real n = std::sqrt(0.36 + 0.64 + delta * delta);
  const Real d[3] = {0.6 / n, -0.8 / n, delta / n};
  return {{c[0] - 0.3 * d[0], c[1] - 0.3 * d[1], c[2] - 0.3 * d[2]},
          {c[0] + 0.35 * d[0], c[1] + 0.35 * d[1], c[2] + 0.35 * d[2]}};
}

/// Case `v82_2d_grazing_report`: through the vertex (0.5, 0.5) of the
/// 4 × 4 grid at the angle delta to the direction dir0.
Bar grazing_2d(Real delta, Real d0x, Real d0y) {
  const Real c[2] = {0.5, 0.5};
  const Real e[2] = {d0x - delta * d0y, d0y + delta * d0x};
  const Real n = std::sqrt(e[0] * e[0] + e[1] * e[1]);
  const Real d[2] = {e[0] / n, e[1] / n};
  return {{c[0] - 0.3 * d[0], c[1] - 0.3 * d[1]}, {c[0] + 0.35 * d[0], c[1] + 0.35 * d[1]}};
}

/// Bars as a parameters string (17 significant digits).
std::string bars_text(const std::vector<Bar> & bars) {
  std::ostringstream s;
  s.precision(17);
  for (auto && b : bars) {
    s << "[";
    for (auto && p : b) {
      s << "(";
      for (std::size_t i = 0; i < p.size(); ++i) {
        s << (i ? ", " : "") << p[i];
      }
      s << ")";
    }
    s << "]";
  }
  return s.str();
}

/// A prescribed-field case: interface, K, f_int and energies under the LCG
/// field `seed` (`seed == 0`: u = 0, pre-stress at rest). No solve.
void field_case(Out & o, Mesh & mesh, Int dim, const std::vector<Bar> & bars,
                const std::string & dat_text, std::uint64_t seed) {
  Mesh rm(dim, "reinforcement_mesh");
  reinforcement(rm, dim, bars);
  o.w.parameters()["bars"] = bars_text(bars);
  o.w.parameters()["material_dat"] = dat_text;
  o.w.parameters()["field_seed"] = static_cast<long long>(seed);
  EmbeddedInterfaceModel model(mesh, rm, dim);
  try {
    model.initFull(EmbeddedInterfaceModelOptions(_static, eigen_options()));
  } catch (std::exception & e) {
    // what upstream computes: an exception (e.g. CGAL_error() of the
    // Segment_3/Triangle_3 intersection on a degenerate flat triangle,
    // UD-407); recorded as the expected outcome
    std::string what = e.what();
    const bool cgal = what.find("CGAL ERROR") != std::string::npos;
    o.w.expect_error(cgal ? "cgal_error" : "exception", cgal ? "" : what);
    o.w.parameters()["upstream_exception"] = what;
    return;
  }
  dump_interface(o, model, dim);
  if (seed != 0) {
    random_field(model, seed);
  }
  model.assembleInternalForces();
  dump_nodal(o, "f_int", model.getInternalForce());
  dump_energies(o, model);
  dump_k(o, model);
}

} // namespace

int main(int argc, char * argv[]) {
  // D-050 pin (the cache-size static is a preemptible global of libakantu,
  // so this also governs the library's SparseLU). Diagnostics only:
  // EMBEDDED_PROBE_EIGEN_CACHE="l1 l2 l3" overrides it (never for fixtures).
  long l1 = 16384, l2 = 524288, l3 = 524288;
  if (const char * c = std::getenv("EMBEDDED_PROBE_EIGEN_CACHE")) {
    std::istringstream(c) >> l1 >> l2 >> l3;
  }
  Eigen::setCpuCacheSizes(l1, l2, l3);
  if (argc < 4) {
    std::fprintf(stderr, "usage: embedded_probe <case> <data dir> <fixtures root>\n");
    return 2;
  }
  const std::string which = argv[1];
  const std::string data = argv[2];
  const fs::path root = argv[3];
  debug::setDebugLevel(dblError);

  // generated cases write their input next to the probe's working directory
  auto init_dat = [&](const std::string & text) {
    const std::string path = "embedded_probe_" + which + ".dat";
    write_file(path, text);
    initialize(path, argc, argv);
  };

  if (which == "t125") {
    Out o(root, which,
          "TEST-125 setup (test_embedded_element_matrix.cc): one horizontal bar y = 0.4 in "
          "triangle.msh, interface mesh and stiffness");
    initialize(data + "/embedded_element.dat", argc, argv);
    Mesh mesh(2);
    mesh.read(data + "/triangle.msh");
    Mesh rm(2, "reinforcement_mesh");
    reinforcement(rm, 2, {{{0, 0.4}, {1, 0.4}}});
    EmbeddedInterfaceModel model(mesh, rm, 2);
    model.initFull(EmbeddedInterfaceModelOptions(_static, eigen_options()));
    dump_interface(o, model, 2);
    dump_k(o, model);
    o.w.finish();
  } else if (which == "t126" or which == "oblique") {
    Out o(root, which,
          which == "t126"
              ? "TEST-126 setup (test_embedded_interface_model.cc): bar y = 0.5 in "
                "embedded_mesh.msh, static solve; interface, K, u, f_int, energies"
              : "TEST-126 mesh and loads with the oblique bar (0.07, 0.13)-(0.91, 0.83) "
                "(review V82, EM-10)");
    initialize(data + "/material.dat", argc, argv);
    Math::setTolerance(1e-7); // as the upstream test (before the model)
    Mesh mesh(2);
    mesh.read(data + "/embedded_mesh.msh");
    Mesh rm(2, "reinforcement_mesh");
    if (which == "t126") {
      reinforcement(rm, 2, {{{0, 0.5}, {1, 0.5}}});
    } else {
      reinforcement(rm, 2, {{{0.07, 0.13}, {0.91, 0.83}}});
    }
    EmbeddedInterfaceModel model(mesh, rm, 2);
    model.initFull(EmbeddedInterfaceModelOptions(_static, eigen_options()));
    dump_interface(o, model, 2);
    auto & nodes = mesh.getNodes();
    auto & forces = model.getExternalForce();
    auto & bound = model.getBlockedDOFs();
    forces(2, 0) = -250;
    forces(5, 0) = -500;
    forces(8, 0) = -250;
    for (Int i = 0; i < mesh.getNbNodes(); i++) {
      if (Math::are_float_equal(nodes(i, 0), 0.)) {
        bound(i, 0) = true;
      }
      if (Math::are_float_equal(nodes(i, 1), 0.)) {
        bound(i, 1) = true;
      }
    }
    model.solveStep();
    dump_k(o, model);
    dump_nodal(o, "u", model.getDisplacement());
    model.assembleInternalForces();
    dump_nodal(o, "f_int", model.getInternalForce());
    dump_energies(o, model);
    o.w.finish();
  } else if (which == "t127") {
    Out o(root, which,
          "TEST-127 setup (test_embedded_interface_model_prestress.cc): pre-stressed bars of "
          "embedded_mesh_prestress*.msh, static solve; interface, u, f_int, energies");
    initialize(data + "/prestress.dat", argc, argv);
    Math::setTolerance(1e-6);
    Mesh mesh(2);
    mesh.read(data + "/embedded_mesh_prestress.msh");
    Mesh rm(2, "reinforcement_mesh");
    rm.read(data + "/embedded_mesh_prestress_reinforcement.msh");
    EmbeddedInterfaceModel model(mesh, rm, 2);
    model.initFull(EmbeddedInterfaceModelOptions(_static, eigen_options()));
    dump_interface(o, model, 2);
    model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "XBlocked");
    model.applyBC(BC::Dirichlet::FixedValue(0.0, _y), "YBlocked");
    model.solveStep();
    dump_nodal(o, "u", model.getDisplacement());
    model.assembleInternalForces();
    dump_nodal(o, "f_int", model.getInternalForce());
    dump_energies(o, model);
    o.w.finish();
  } else if (which == "prestress_2d_rest") {
    Out o(root, which,
          "2-D pre-stress at rest: review V82's 2-D adversarial bars (one law, sigma_p = 3.5) in "
          "a 4 x 4 grid of _triangle_3, u = 0; interface, f_int (pre-stress nodal forces), K");
    const auto text = dat(0., 0., 2.0e4, 0.01, 3.5);
    init_dat(text);
    Mesh mesh(2);
    grid_triangles(mesh, 4, 4, 1.0, 1.0);
    field_case(o, mesh, 2, bars_2d(), text, 0);
    o.w.finish();
  } else if (which == "adversarial_2d") {
    Out o(root, which,
          "Review V82's 2-D adversarial bars (one law) in a 4 x 4 grid of _triangle_3 under the "
          "LCG displacement field (seed 11), bulk E = 0; interface, f_int, K, energies");
    const auto text = dat(0., 0., 2.0e4, 0.01, 0.);
    init_dat(text);
    Mesh mesh(2);
    grid_triangles(mesh, 4, 4, 1.0, 1.0);
    field_case(o, mesh, 2, bars_2d(), text, 11);
    o.w.finish();
  } else if (which == "grazing_2d_1e-4" or which == "grazing_2d_1e-10") {
    const Real delta = which == "grazing_2d_1e-4" ? 1e-4 : 1e-10;
    Out o(root, which,
          "Review V82 EM-01, 2-D: bars through the vertex (0.5, 0.5) of a 4 x 4 grid of "
          "_triangle_3 at a grazing angle to the horizontal grid line and to the diagonal "
          "edges, LCG field (seed 3), bulk E = 0");
    o.w.parameters()["delta"] = delta;
    const auto text = dat(0., 0., 2.0e4, 0.01, 0.);
    init_dat(text);
    Mesh mesh(2);
    grid_triangles(mesh, 4, 4, 1.0, 1.0);
    field_case(o, mesh, 2, {grazing_2d(delta, 1.0, 0.0), grazing_2d(delta, 1.0, 1.0)}, text, 3);
    o.w.finish();
  } else if (which == "triangle6_oblique") {
    Out o(root, which,
          "_triangle_6 background (2 x 2 cells over the unit square): an oblique bar and a bar "
          "along the grid line x = 0.5, LCG field (seed 13), bulk E = 0");
    const auto text = dat(0., 0., 2.1e4, 0.02, 1.5);
    init_dat(text);
    Mesh mesh(2);
    grid_triangles6(mesh, 2);
    field_case(o, mesh, 2, {{{0.07, 0.13}, {0.91, 0.83}}, {{0.5, 0.05}, {0.5, 0.95}}}, text, 13);
    o.w.finish();
  } else if (which == "adversarial_3d") {
    Out o(root, which,
          "Review V82's 3-D adversarial bars (EM-03; one law) and a bar in the face z = 0 in a "
          "Kuhn grid of 3^3 cubes (_tetrahedron_4) under the LCG field (seed 7), bulk E = 0 "
          "(upstream: one flat triangle per tetrahedron, UD-407)");
    const auto text = dat(0., 0., 2.0e4, 0.01, 0.);
    init_dat(text);
    Mesh mesh(3);
    grid_tetrahedra(mesh, 3, 1.0);
    field_case(o, mesh, 3, bars_3d(), text, 7);
    o.w.finish();
  } else if (which == "prestress_3d_rest") {
    Out o(root, which,
          "3-D pre-stress at rest: review V82's 3-D adversarial bars (one law, sigma_p = 2) in "
          "a Kuhn grid of 3^3 cubes, u = 0 (upstream: UD-407)");
    const auto text = dat(0., 0., 2.0e4, 0.01, 2.0);
    init_dat(text);
    Mesh mesh(3);
    grid_tetrahedra(mesh, 3, 1.0);
    field_case(o, mesh, 3, bars_3d(), text, 0);
    o.w.finish();
  } else if (which == "face_z0_3d") {
    Out o(root, which,
          "A bar in the boundary face z = 0 of a Kuhn grid of 3^3 cubes (_tetrahedron_4) under the "
          "LCG field (seed 7), bulk E = 0: upstream (one flat triangle per tetrahedron, UD-407) "
          "counts it once per tetrahedron whose projected first three nodes cover it");
    const auto text = dat(0., 0., 2.0e4, 0.01, 0.);
    init_dat(text);
    Mesh mesh(3);
    grid_tetrahedra(mesh, 3, 1.0);
    field_case(o, mesh, 3, {{{0.07, 0.11, 0.0}, {0.89, 0.71, 0.0}}}, text, 7);
    o.w.finish();
  } else if (which == "grazing_3d_1e-4" or which == "grazing_3d_1e-10") {
    const Real delta = which == "grazing_3d_1e-4" ? 1e-4 : 1e-10;
    Out o(root, which,
          "Review V82 EM-01, 3-D: a bar through the vertex (1/3, 2/3, 1/3) of a Kuhn grid of "
          "3^3 cubes at a grazing angle to the plane z = 1/3, LCG field (seed 5), bulk E = 0 "
          "(upstream: UD-407)");
    o.w.parameters()["delta"] = delta;
    const auto text = dat(0., 0., 2.0e4, 0.01, 0.);
    init_dat(text);
    Mesh mesh(3);
    grid_tetrahedra(mesh, 3, 1.0);
    field_case(o, mesh, 3, {grazing_3d(delta)}, text, 5);
    o.w.finish();
  } else if (which == "faces_grazing_3d") {
    // UD-601: does CGAL (Simple_cartesian<double>, the kernel of upstream's
    // intersector) miss the vertex hits of the grazing bars as the port's
    // LGPL Intersections_3 does? intersection(Segment_3, Triangle_3) of each
    // bar with the four faces (a,b,c), (b,c,d), (c,d,a), (d,a,b) of every
    // tetrahedron of the Kuhn grid (akantu-geometry's factory order).
    using K = cgal::Cartesian;
    init_dat(dat(0., 0., 1., 1., 0.));
    Out o(root, which,
          "CGAL intersection(Segment_3, Triangle_3) in Simple_cartesian<double> of review V82's "
          "grazing bars (delta 1e-4, 1e-10, 1e-12 through the vertex (1/3, 2/3, 1/3)) with the "
          "four faces of every tetrahedron of the Kuhn grid of 3^3 cubes (UD-601)");
    Mesh mesh(3);
    grid_tetrahedra(mesh, 3, 1.0);
    const auto & X = mesh.getNodes();
    const auto & C = mesh.getConnectivity(_tetrahedron_4);
    std::vector<double> queries, coords;
    std::vector<std::int32_t> kind;
    auto pt = [&](Idx n) { return K::Point_3(X(n, 0), X(n, 1), X(n, 2)); };
    for (Real delta : {1e-4, 1e-10, 1e-12}) {
      auto bar = grazing_3d(delta);
      K::Segment_3 seg(K::Point_3(bar[0][0], bar[0][1], bar[0][2]),
                       K::Point_3(bar[1][0], bar[1][1], bar[1][2]));
      for (auto && p : bar) {
        queries.insert(queries.end(), p.begin(), p.end());
      }
      for (Int e = 0; e < C.size(); ++e) {
        auto a = pt(C(e, 0)), b = pt(C(e, 1)), c = pt(C(e, 2)), d = pt(C(e, 3));
        for (auto && tri : {K::Triangle_3(a, b, c), K::Triangle_3(b, c, d),
                            K::Triangle_3(c, d, a), K::Triangle_3(d, a, b)}) {
          std::array<double, 6> v{};
          std::int32_t k = 0;
          if (auto res = CGAL::intersection(seg, tri)) {
            if (const auto * q = boost::get<K::Point_3>(&*res)) {
              k = 1;
              v = {q->x(), q->y(), q->z(), 0., 0., 0.};
            } else if (const auto * sg = boost::get<K::Segment_3>(&*res)) {
              k = 2;
              v = {sg->source().x(), sg->source().y(), sg->source().z(),
                   sg->target().x(), sg->target().y(), sg->target().z()};
            }
          }
          kind.push_back(k);
          coords.insert(coords.end(), v.begin(), v.end());
        }
      }
    }
    o.f64("queries", {queries.size() / 6, 6}, {"entry", "component"}, "table", queries);
    o.i32("kind", {kind.size()}, {"entry"}, "table", kind);
    o.f64("coords", {coords.size() / 6, 6}, {"entry", "component"}, "table", coords);
    o.w.parameters()["kind_codes"] = std::string("0 no intersection, 1 point, 2 segment");
    o.w.parameters()["layout"] = std::string(
        "entry = (query q, tetrahedron e, face f) at q * 4 * nb_tetrahedra + 4 * e + f");
    o.w.finish();
  } else {
    std::fprintf(stderr, "unknown case %s\n", which.c_str());
    return 2;
  }
  finalize();
  return 0;
}
