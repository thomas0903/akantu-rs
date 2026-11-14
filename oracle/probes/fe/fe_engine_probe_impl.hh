// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records what Akantu's FEEngineTemplate<IntegratorGauss,
// ShapeLagrange, kind> computes on meshes (src/fe_engine/fe_engine_template*,
// integrator_gauss*, shape_lagrange*, shape_cohesive*), for the
// differential tests (crates/akantu-fe/tests/fe_engine_matches_oracle.rs).
//
// Output (format: docs/NUMERICAL_VALIDATION.md):
//   fixtures/fe/engine/<type>           regular types on the upstream
//                                       test/test_fe_engine/<type>.msh meshes
//   fixtures/fe/engine_cohesive/<type>  cohesive types on small generated
//                                       meshes (nodes/connectivity recorded)
// Usage: fe_engine_probe <fixtures_root> <reference_src_root> [type ...]
//
// This header holds the probe bodies; each translation unit fe_engine_probe_*.cc
// instantiates a few element types (one TU for everything exceeds the 4 GiB
// memory cap of the oracle container).

#pragma once

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_types.hh>
#include <dof_manager_default.hh>
#include <element_class.hh>
#include <fe_engine.hh>
#include <integrator_gauss.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <shape_cohesive.hh>
#include <shape_lagrange.hh>

#include <cmath>
#include <cstdint>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;

namespace fe_probe {

inline constexpr const char * kProbe = "fe_engine_probe";
inline constexpr const char * kProbeVersion = "1";

inline std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

inline std::vector<std::size_t> sz(std::initializer_list<long> l) {
  std::vector<std::size_t> out;
  for (auto v : l) {
    out.push_back(static_cast<std::size_t>(v));
  }
  return out;
}

/// Row-major copy of an Array of per-tuple column-major (rows x cols)
/// matrices: [tuple, row, col].
inline std::vector<double> tuples_row_major(const Array<Real> & a, Int rows, Int cols) {
  std::vector<double> out;
  out.reserve(a.size() * rows * cols);
  for (Int t = 0; t < a.size(); ++t) {
    for (Int r = 0; r < rows; ++r) {
      for (Int c = 0; c < cols; ++c) {
        out.push_back(a(t, r + c * rows));
      }
    }
  }
  return out;
}

inline std::vector<double> flat(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

/// Deterministic, exactly representable test values in [-2, 2).
inline double pseudo(long k) { return double((k * 37 + 11) % 64) / 16. - 2.; }

/// Smooth nonlinear nodal test field with two components.
inline Array<Real> nodal_field(const Mesh & mesh) {
  const auto dim = mesh.getSpatialDimension();
  const auto & X = mesh.getNodes();
  Array<Real> u(X.size(), 2, "u");
  for (Int n = 0; n < X.size(); ++n) {
    double s0 = 0., s1 = 0.;
    for (Int d = 0; d < dim; ++d) {
      s0 += (d + 1.) * X(n, d);
      s1 += X(n, d) * X(n, d);
    }
    u(n, 0) = std::sin(1.3 * s0) + 0.5 * s1;
    u(n, 1) = std::cos(0.7 * s0) - s0 * s1;
  }
  return u;
}

/// rho field functor of the lumped-mass test: rho(d, q) = 1.5 + 0.25 d +
/// 0.01 e (the Rust test uses the same expression).
inline void rho_functor(Matrix<Real> & rho, const Element & el) {
  for (Int d = 0; d < rho.rows(); ++d) {
    for (Int q = 0; q < rho.cols(); ++q) {
      rho(d, q) = 1.5 + 0.25 * d + 0.01 * el.element;
    }
  }
}

using FEMRegular = FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_regular>;
using FEMCohesive = FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_cohesive>;

/* -------------------------------------------------------------------------- */
/* Regular elements on the upstream test meshes                               */
/* -------------------------------------------------------------------------- */
template <ElementType type>
void probe_regular(const std::filesystem::path & root, const std::filesystem::path & src) {
  using EC = ElementClass<type>;
  constexpr Int dim = EC::getSpatialDimension();
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  const auto et = type_name(type);
  const auto rel = "test/test_fe_engine/" + et + ".msh";
  const auto file = src / rel;

  Mesh mesh(dim);
  mesh.read(file.string());
  FEMRegular fem(mesh, dim, "my_fem");
  fem.initShapeFunctions(_not_ghost);

  ap::FixtureWriter w(root, "fe/engine/" + et,
                      "FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_regular> on the "
                      "upstream test mesh " + rel + ": precomputed shapes, derivatives, "
                      "jacobians, interpolation, gradients, integrals, BtD, BtDB, Ntb, "
                      "NtbN, inverse map, inradius, lumped mass, facet normals",
                      kProbe, kProbeVersion);
  w.add_tag("fe_engine");
  w.add_input_reference("mesh", "upstream:" + rel, file);
  auto & p = w.parameters();
  p["element_type"] = et;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["mesh"] = "mesh_io/test_fe_engine/" + et;

  const auto nb_element = mesh.getNbElement(type);
  const auto nq = fem.getNbIntegrationPoints(type);
  p["nb_element"] = static_cast<long long>(nb_element);
  p["nb_quadrature_points"] = static_cast<long long>(nq);
  const long ne = nb_element;
  const long q = nq;

  {
    const auto & Q = fem.getIntegrationPoints(type);
    std::vector<double> P;
    for (Int k = 0; k < Q.cols(); ++k) {
      for (Int d = 0; d < std::max<Int>(nd, 1); ++d) {
        P.push_back(Q(d, k));
      }
    }
    w.add("integration_points", sz({q, std::max<long>(nd, 1)}), {"quad", "natural_dim"},
          "quadrature", P)
        .element_type(et)
        .tolerance_class("kernel");
  }
  w.add("shapes", sz({ne, q, nn}), {"element", "quad", "node_local"}, "quadrature",
        flat(fem.getShapes(type)))
      .element_type(et).ghost_type("not_ghost").tolerance_class("kernel");
  w.add("shapes_derivatives", sz({ne, q, dim, nn}),
        {"element", "quad", "spatial_dim", "node_local"}, "quadrature",
        tuples_row_major(fem.getShapesDerivatives(type), dim, nn))
      .element_type(et).ghost_type("not_ghost")
      .description("dN_j/dx_i (J^-1 dnds), [element, quad, i, j]")
      .tolerance_class("kernel");
  w.add("jacobians", sz({ne, q}), {"element", "quad"}, "quadrature",
        flat(fem.getIntegrator().getJacobians(type)))
      .element_type(et).ghost_type("not_ghost")
      .description("det J * w per quadrature point").tolerance_class("kernel");

  // nodal field, interpolation, gradient, integration
  auto u = nodal_field(mesh);
  w.add("field/nodal", sz({u.size(), 2}), {"node", "component"}, "nodal", flat(u))
      .description("input: nodal test field").tolerance_class("exact");
  Array<Real> uq(0, 2);
  fem.interpolateOnIntegrationPoints(u, uq, 2, type);
  w.add("field/interpolated", sz({ne, q, 2}), {"element", "quad", "component"}, "quadrature",
        flat(uq))
      .element_type(et).tolerance_class("kernel");
  Array<Real> grad(0, 2 * dim);
  fem.gradientOnIntegrationPoints(u, grad, 2, type);
  w.add("field/gradient", sz({ne, q, 2, dim}), {"element", "quad", "row", "col"},
        "quadrature", tuples_row_major(grad, 2, dim))
      .element_type(et).description("du_i/dx_j, [element, quad, i, j]")
      .tolerance_class("kernel");
  {
    Array<Int> filter;
    for (Int e = 0; e < nb_element; e += 2) {
      filter.push_back(e);
    }
    std::vector<std::int64_t> fi;
    for (Int k = 0; k < filter.size(); ++k) {
      fi.push_back(filter(k));
    }
    w.add("filter", sz({filter.size()}), {"entry"}, "table", fi)
        .description("input: element filter (every other element)")
        .tolerance_class("exact");
    Array<Real> gradf(0, 2 * dim);
    fem.gradientOnIntegrationPoints(u, gradf, 2, type, _not_ghost, filter);
    w.add("field/gradient_filtered", sz({filter.size(), q, 2, dim}),
          {"element", "quad", "row", "col"}, "quadrature", tuples_row_major(gradf, 2, dim))
        .element_type(et).tolerance_class("kernel");
    Array<Real> uqf(0, 2);
    fem.interpolateOnIntegrationPoints(u, uqf, 2, type, _not_ghost, filter);
    w.add("field/interpolated_filtered", sz({filter.size(), q, 2}),
          {"element", "quad", "component"}, "quadrature", flat(uqf))
        .element_type(et).tolerance_class("kernel");
    Array<Real> intf(0, 2);
    fem.integrate(uqf, intf, 2, type, _not_ghost, filter);
    w.add("field/integrated_filtered", sz({filter.size(), 2}), {"element", "component"},
          "elemental", flat(intf))
        .element_type(et).tolerance_class("kernel");
  }
  Array<Real> intu(0, 2);
  fem.integrate(uq, intu, 2, type);
  w.add("field/integrated", sz({ne, 2}), {"element", "component"}, "elemental", flat(intu))
      .element_type(et).tolerance_class("kernel");
  {
    Array<Real> u0(uq.size(), 1);
    for (Int k = 0; k < uq.size(); ++k) {
      u0(k) = uq(k, 0);
    }
    std::vector<double> s{fem.integrate(u0, type)};
    w.add("field/integral_scalar", {}, {}, "scalar", s)
        .description("integrate(f, type): Math::reduce of the per-element integrals")
        .tolerance_class("kernel");
  }

  Array<Real> xq(0, dim);
  fem.computeIntegrationPointsCoordinates(xq, type);
  w.add("quad_coords", sz({ne, q, dim}), {"element", "quad", "spatial_dim"}, "quadrature",
        flat(xq))
      .element_type(et).tolerance_class("kernel");

  // Operators on a spread subset of the elements (size policy), which also
  // exercises the filtered code paths.
  Array<Int> op_filter;
  {
    std::set<Int> picked;
    for (Int k = 0; k < 5; ++k) {
      picked.insert(static_cast<Int>((static_cast<long>(nb_element) * k) / 5));
    }
    std::vector<std::int64_t> fi;
    for (auto e : picked) {
      op_filter.push_back(e);
      fi.push_back(e);
    }
    w.add("op_filter", sz({static_cast<long>(fi.size())}), {"entry"}, "table", fi)
        .description("input: element filter of the BtD/BtDB/Ntb/NtbN records")
        .tolerance_class("exact");
  }
  const long nf = op_filter.size();
  // BtDB (order 4) on at most two elements: (dim nn)^2 per point
  Array<Int> btdb_filter;
  {
    std::vector<std::int64_t> fi;
    for (Int k = 0; k < std::min<Int>(2, op_filter.size()); ++k) {
      btdb_filter.push_back(op_filter(k));
      fi.push_back(op_filter(k));
    }
    w.add("btdb_filter", sz({static_cast<long>(fi.size())}), {"entry"}, "table", fi)
        .description("input: element filter of the order-4 BtDB record")
        .tolerance_class("exact");
  }
  const long nb4 = btdb_filter.size();
  // BtD with D = dim x dim per point
  {
    Array<Real> D(nf * q, dim * dim);
    for (Int k = 0; k < D.size() * D.getNbComponent(); ++k) {
      D.data()[k] = pseudo(k);
    }
    w.add("btd/input", sz({nf, q, dim * dim}), {"element", "quad", "component"},
          "quadrature", flat(D))
        .description("input: D, column-major dim x dim per point").tolerance_class("exact");
    Array<Real> BtD(0, dim * nn);
    fem.computeBtD(D, BtD, type, _not_ghost, op_filter);
    w.add("btd", sz({nf, q, dim, nn}), {"element", "quad", "row", "col"}, "quadrature",
          tuples_row_major(BtD, dim, nn))
        .element_type(et).tolerance_class("kernel");
  }
  // BtDB order 4 (Voigt tangent) and order 2
  {
    constexpr Int ts = VoigtHelper<dim>::size;
    Array<Real> D(nb4 * q, ts * ts);
    auto Dv = make_view(D, ts, ts);
    long k = 0;
    for (auto && m : Dv) {
      for (Int i = 0; i < ts; ++i) {
        for (Int j = i; j < ts; ++j) {
          m(i, j) = m(j, i) = pseudo(k++) + (i == j ? 4. : 0.);
        }
      }
    }
    w.add("btdb/input", sz({nb4, q, ts * ts}), {"element", "quad", "component"},
          "quadrature", flat(D))
        .description("input: symmetric Voigt tangent per point (column-major)")
        .tolerance_class("exact");
    Array<Real> BtDB(0, dim * nn * dim * nn);
    fem.computeBtDB(D, BtDB, 4, type, _not_ghost, btdb_filter);
    w.add("btdb", sz({nb4, q, dim * nn, dim * nn}), {"element", "quad", "row", "col"},
          "quadrature", tuples_row_major(BtDB, dim * nn, dim * nn))
        .element_type(et).tolerance_class("kernel");
    Array<Real> D2(nf * q, dim * dim);
    for (Int kk = 0; kk < D2.size() * D2.getNbComponent(); ++kk) {
      D2.data()[kk] = pseudo(3 * kk + 1);
    }
    w.add("btdb2/input", sz({nf, q, dim * dim}), {"element", "quad", "component"},
          "quadrature", flat(D2))
        .description("input: dim x dim D per point (column-major)").tolerance_class("exact");
    Array<Real> BtDB2(0, nn * nn);
    fem.computeBtDB(D2, BtDB2, 2, type, _not_ghost, op_filter);
    w.add("btdb2", sz({nf, q, nn, nn}), {"element", "quad", "row", "col"}, "quadrature",
          tuples_row_major(BtDB2, nn, nn))
        .element_type(et).tolerance_class("kernel");
  }
  // Ntb (2 dofs) and NtbN (scalar)
  {
    Array<Real> b(nf * q, 2);
    for (Int kk = 0; kk < b.size() * 2; ++kk) {
      b.data()[kk] = pseudo(5 * kk + 2);
    }
    w.add("ntb/input", sz({nf, q, 2}), {"element", "quad", "component"}, "quadrature",
          flat(b))
        .tolerance_class("exact");
    Array<Real> Ntb(0, 2 * nn);
    fem.computeNtb(b, Ntb, type, _not_ghost, op_filter);
    w.add("ntb", sz({nf, q, 2, nn}), {"element", "quad", "row", "col"}, "quadrature",
          tuples_row_major(Ntb, 2, nn))
        .element_type(et).tolerance_class("kernel");
    Array<Real> s(nf * q, 1);
    for (Int kk = 0; kk < s.size(); ++kk) {
      s(kk) = pseudo(7 * kk + 3);
    }
    w.add("ntbn/input", sz({nf, q}), {"element", "quad"}, "quadrature", flat(s))
        .tolerance_class("exact");
    Array<Real> NtbN(0, nn * nn);
    fem.computeNtbN(s, NtbN, type, _not_ghost, op_filter);
    w.add("ntbn", sz({nf, q, nn, nn}), {"element", "quad", "row", "col"}, "quadrature",
          tuples_row_major(NtbN, nn, nn))
        .element_type(et).tolerance_class("kernel");
  }
  // inradius, inverse map, contains
  {
    std::vector<double> r;
    std::vector<double> xi;
    std::vector<bool> in;
    for (Int e = 0; e < nb_element; ++e) {
      Element el{type, e, _not_ghost};
      r.push_back(fem.getElementInradius(el));
      for (Int k = 0; k < nq; ++k) {
        Vector<Real> x(dim);
        for (Int d = 0; d < dim; ++d) {
          x(d) = xq(e * nq + k, d);
        }
        Vector<Real> nat(dim);
        fem.inverseMap(x, e, type, nat);
        for (Int d = 0; d < dim; ++d) {
          xi.push_back(nat(d));
        }
        in.push_back(fem.contains(x, e, type));
      }
    }
    w.add("inradius", sz({ne}), {"element"}, "elemental", r)
        .element_type(et).tolerance_class("kernel");
    w.add("inverse_map/xi", sz({ne, q, dim}), {"element", "quad", "natural_dim"},
          "quadrature", xi)
        .element_type(et).description("inverseMap of quad_coords")
        .tolerance_class("nonlinear_solve");
    w.add_bool("inverse_map/contains", sz({ne, q}), {"element", "quad"}, "quadrature", in)
        .element_type(et).tolerance_class("exact");
  }
  // lumped mass (row sum or diagonal scaling), assembled on the nodes
  {
    DOFManagerDefault dof_manager(mesh, "probe_dof_manager");
    Array<Real> lumped(mesh.getNbNodes(), 2, 0.);
    fem.assembleFieldLumped(rho_functor, lumped, dof_manager, type, _not_ghost);
    w.add("lumped_mass", sz({lumped.size(), 2}), {"node", "component"}, "nodal",
          flat(lumped))
        .description("assembleFieldLumped with rho(d, q) = 1.5 + 0.25 d + 0.01 e, assembled "
                     "by DOFManager::assembleElementalArrayLocalArray")
        .tolerance_class("kernel");
  }
  // facet normals (facet engine of dimension dim - 1)
  if constexpr (dim > 1) {
    FEMRegular fem_b(mesh, dim - 1, "boundary");
    fem_b.initShapeFunctions(_not_ghost);
    fem_b.computeNormalsOnIntegrationPoints(_not_ghost);
    std::vector<std::string> facets;
    for (auto ft : mesh.elementTypes(dim - 1, _not_ghost, _ek_regular)) {
      const auto & normals = fem_b.getNormalsOnIntegrationPoints(ft);
      const auto fname = type_name(ft);
      facets.push_back(fname);
      const long nfe = mesh.getNbElement(ft);
      const long nfq = fem_b.getNbIntegrationPoints(ft);
      w.add("normals/" + fname, sz({nfe, nfq, dim}), {"element", "quad", "spatial_dim"},
            "quadrature", flat(normals))
          .element_type(fname).tolerance_class("kernel");
      w.add("facet_jacobians/" + fname, sz({nfe, nfq}), {"element", "quad"}, "quadrature",
            flat(fem_b.getIntegrator().getJacobians(ft)))
          .element_type(fname).tolerance_class("kernel");
    }
    p["facet_types"] = ap::Json::array_of(facets);
  }
  w.finish();
  std::cout << "  fe/engine/" << et << "\n";
}

/* -------------------------------------------------------------------------- */
/* Cohesive elements on generated meshes                                      */
/* -------------------------------------------------------------------------- */
/// Natural node coordinates of the facet interpolation (nn x max(nd,1)).
template <ElementType facet> std::vector<std::vector<double>> facet_nodes() {
  switch (facet) {
  case _point_1:
    return {{0.}};
  case _segment_2:
    return {{-1.}, {1.}};
  case _segment_3:
    return {{-1.}, {1.}, {0.}};
  case _triangle_3:
    return {{0., 0.}, {1., 0.}, {0., 1.}};
  case _triangle_6:
    return {{0., 0.}, {1., 0.}, {0., 1.}, {.5, 0.}, {.5, .5}, {0., .5}};
  case _quadrangle_4:
    return {{-1., -1.}, {1., -1.}, {1., 1.}, {-1., 1.}};
  case _quadrangle_8:
    return {{-1., -1.}, {1., -1.}, {1., 1.}, {-1., 1.},
            {0., -1.},  {1., 0.},  {0., 1.}, {-1., 0.}};
  default:
    throw std::runtime_error("no facet nodes");
  }
}

/// Physical position of facet node xi in dimension sdim, element e, side s.
inline std::vector<double> cohesive_position(const std::vector<double> & xi, Int sdim, Int e,
                                      Int side, Int n) {
  const double A[3][2] = {{1.10, 0.20}, {-0.15, 0.95}, {0.30, -0.25}};
  std::vector<double> x(sdim, 0.);
  for (Int i = 0; i < sdim; ++i) {
    double v = 0.4 * i + 1.7 * e;
    for (std::size_t j = 0; j < xi.size() and sdim > 1; ++j) {
      v += A[i][j] * xi[j];
    }
    double s = 0.;
    for (std::size_t j = 0; j < xi.size(); ++j) {
      s += (j + 1.) * xi[j];
    }
    v += 0.04 * std::sin(1.1 * s + 0.3 * i);
    // the minus side is opened by a node-dependent offset
    if (side == 1) {
      v += 0.01 * (1. + i) + 0.002 * n;
    }
    x[i] = v;
  }
  return x;
}

template <ElementType type>
void probe_cohesive(const std::filesystem::path & root) {
  using EC = ElementClass<type>;
  constexpr Int sdim = EC::getSpatialDimension();
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  constexpr auto facet = ElementClassExtraGeometryProperties<type>::facet_type[0];
  const auto et = type_name(type);

  Mesh mesh(sdim);
  MeshAccessor acc(mesh);
  auto & conn = acc.getConnectivity(type);
  const auto xi = facet_nodes<facet>();
  constexpr Int ne = 2;
  for (Int e = 0; e < ne; ++e) {
    Vector<Idx> c(2 * nn);
    for (Int side = 0; side < 2; ++side) {
      for (Int n = 0; n < nn; ++n) {
        auto x = cohesive_position(nd == 0 ? std::vector<double>{} : xi[n], sdim, e, side, n);
        Vector<Real> node(sdim);
        for (Int d = 0; d < sdim; ++d) {
          node(d) = x[d];
        }
        c(side * nn + n) = acc.getNodes().size();
        acc.getNodes().push_back(node);
      }
    }
    conn.push_back(c);
  }
  acc.makeReady();

  FEMCohesive fem(mesh, sdim, "cohesive_fem");
  fem.initShapeFunctions(_not_ghost);

  ap::FixtureWriter w(root, "fe/engine_cohesive/" + et,
                      "FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_cohesive> on a "
                      "generated two-element mesh of " + et +
                          ": shapes, natural derivatives, mid-surface jacobians, "
                          "mean/opening interpolation, integrals, normals",
                      kProbe, kProbeVersion);
  w.add_tag("fe_engine");
  w.add_tag("cohesive");
  auto & p = w.parameters();
  p["element_type"] = et;
  p["spatial_dimension"] = static_cast<long long>(sdim);
  const auto & nodes = mesh.getNodes();
  w.add("mesh/nodes", sz({nodes.size(), sdim}), {"node", "spatial_dim"}, "nodal", flat(nodes))
      .description("input: generated node coordinates").tolerance_class("exact");
  std::vector<std::int64_t> cv(conn.data(), conn.data() + conn.size() * conn.getNbComponent());
  w.add("mesh/connectivity", sz({conn.size(), conn.getNbComponent()}),
        {"element", "node_local"}, "elemental", cv)
      .element_type(et).description("input: connectivity (side + nodes, then side -)")
      .tolerance_class("exact");

  const long q = fem.getNbIntegrationPoints(type);
  w.add("shapes", sz({ne, q, nn}), {"element", "quad", "node_local"}, "quadrature",
        flat(fem.getShapes(type)))
      .element_type(et).tolerance_class("kernel");
  if constexpr (nd > 0) {
    w.add("shapes_derivatives", sz({ne, q, nd, nn}),
          {"element", "quad", "natural_dim", "node_local"}, "quadrature",
          tuples_row_major(fem.getShapesDerivatives(type), nd, nn))
        .element_type(et).description("natural dN/ds of the cohesive interpolation")
        .tolerance_class("kernel");
  }
  w.add("jacobians", sz({ne, q}), {"element", "quad"}, "quadrature",
        flat(fem.getIntegrator().getJacobians(type)))
      .element_type(et).tolerance_class("kernel");

  auto u = nodal_field(mesh);
  w.add("field/nodal", sz({u.size(), 2}), {"node", "component"}, "nodal", flat(u))
      .tolerance_class("exact");
  Array<Real> mean(0, 2), opening(0, 2);
  fem.getShapeFunctions()
      .interpolateOnIntegrationPoints<type, CohesiveReduceFunctionMean>(u, mean, 2);
  fem.getShapeFunctions()
      .interpolateOnIntegrationPoints<type, CohesiveReduceFunctionOpening>(u, opening, 2);
  w.add("field/mean", sz({ne, q, 2}), {"element", "quad", "component"}, "quadrature",
        flat(mean))
      .element_type(et).tolerance_class("kernel");
  w.add("field/opening", sz({ne, q, 2}), {"element", "quad", "component"}, "quadrature",
        flat(opening))
      .element_type(et).tolerance_class("kernel");
  Array<Real> intf(0, 2);
  fem.integrate(opening, intf, 2, type);
  w.add("field/integrated_opening", sz({ne, 2}), {"element", "component"}, "elemental",
        flat(intf))
      .element_type(et).tolerance_class("kernel");
  {
    Array<Real> o0(opening.size(), 1);
    for (Int k = 0; k < opening.size(); ++k) {
      o0(k) = opening(k, 0);
    }
    std::vector<double> s{fem.integrate(o0, type)};
    w.add("field/integral_scalar", {}, {}, "scalar", s).tolerance_class("kernel");
  }
  if constexpr (sdim > 1) {
    Array<Real> normals(0, sdim);
    Array<Int> all;
    for (Int e = 0; e < ne; ++e) {
      all.push_back(e);
    }
    fem.getShapeFunctions()
        .computeNormalsOnIntegrationPoints<type, CohesiveReduceFunctionMean>(
            mesh.getNodes(), normals, _not_ghost, all);
    w.add("normals", sz({ne, q, sdim}), {"element", "quad", "spatial_dim"}, "quadrature",
          flat(normals))
        .element_type(et).description("computeNormalsOnIntegrationPoints<type, Mean>(nodes)")
        .tolerance_class("kernel");
  }
  w.finish();
  std::cout << "  fe/engine_cohesive/" << et << "\n";
}

} // namespace fe_probe
