// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records element-level reference data computed by Akantu's
// ElementClass / GaussIntegrationElement templates
// (src/fe_engine/element_class*.hh, gauss_integration*.{cc,hh},
// geometrical_element_property.hh) for every element type of the build.
//
// Output: one fixture per element type, fixtures/fe/element_class/<type>/
// (format: docs/NUMERICAL_VALIDATION.md). Usage:
//   element_probe <fixtures_root> [type ...]
//
// The element-class templates are header-only: they are compiled into this
// probe, so the probe must be built with the same floating-point flags as
// the oracle library (oracle/probes/CMakeLists.txt takes care of that).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_types.hh>
#include <element_class.hh>

#include <array>
#include <cmath>
#include <cstdint>
#include <functional>
#include <iostream>
#include <set>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;

namespace {

constexpr const char * kProbe = "element_probe";
constexpr const char * kProbeVersion = "2"; // 2: d2nds2 records

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string kind_name(ElementKind k) {
  switch (k) {
  case _ek_regular:
    return "regular";
  case _ek_cohesive:
    return "cohesive";
  case _ek_structural:
    return "structural";
  default:
    return "unknown";
  }
}

std::vector<std::size_t> sz(std::initializer_list<long> l) {
  std::vector<std::size_t> out;
  for (auto v : l) {
    out.push_back(static_cast<std::size_t>(v));
  }
  return out;
}

/// Row-major copy of an Eigen matrix.
template <class M> void append_row_major(std::vector<double> & out, const M & m) {
  for (Eigen::Index i = 0; i < m.rows(); ++i) {
    for (Eigen::Index j = 0; j < m.cols(); ++j) {
      out.push_back(m(i, j));
    }
  }
}

/// Monomial exponents (a,b,c) with a+b+c <= degree in `dim` variables.
std::vector<std::array<int, 3>> monomials(int dim, int degree) {
  std::vector<std::array<int, 3>> out;
  for (int a = 0; a <= degree; ++a) {
    for (int b = 0; b <= (dim > 1 ? degree - a : 0); ++b) {
      for (int c = 0; c <= (dim > 2 ? degree - a - b : 0); ++c) {
        out.push_back({a, b, c});
      }
    }
  }
  return out;
}

double eval_monomial(const std::array<int, 3> & e, const Vector<Real> & x) {
  double v = 1.;
  for (Eigen::Index d = 0; d < x.size() and d < 3; ++d) {
    for (int k = 0; k < e[static_cast<std::size_t>(d)]; ++k) {
      v *= x(d);
    }
  }
  return v;
}

/* -------------------------------------------------------------------------- */
/* Quadrature orders supported per Gauss integration type                     */
/* -------------------------------------------------------------------------- */
template <ElementType type> constexpr int max_order() {
  constexpr auto git = ElementClassProperty<type>::gauss_integration_type;
  // segment-tensor rules have data for 1..4 points per direction (order 7);
  // triangle/tetrahedron/pentahedron rules up to order 5; point: any.
  return git == _git_segment ? 7 : 5;
}

/* -------------------------------------------------------------------------- */
/* Node natural coordinates (searched on the reference lattice, verified)     */
/* -------------------------------------------------------------------------- */
template <ElementType type> Matrix<Real> node_natural_coordinates() {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  constexpr auto shape =
      GeometricalShape<ElementClassProperty<type>::geometrical_type>::shape;
  Matrix<Real> coords(std::max<Int>(nd, 1), nn);
  coords.setZero();
  if constexpr (nd == 0) {
    return coords;
  } else {
    std::vector<Vector<Real>> candidates;
    std::vector<double> sq{-1., 0., 1.};
    std::vector<double> tri{0., 0.5, 1.};
    std::function<void(Int, Vector<Real> &)> rec = [&](Int d, Vector<Real> & c) {
      if (d == nd) {
        candidates.push_back(c);
        return;
      }
      bool tri_dir = (shape == _gst_triangle) or (shape == _gst_prism and d > 0);
      for (double v : tri_dir ? tri : sq) {
        c(d) = v;
        rec(d + 1, c);
      }
    };
    Vector<Real> c(nd);
    rec(0, c);
    std::vector<int> found(nn, 0);
    for (auto & cand : candidates) {
      if (shape == _gst_triangle) {
        double s = 0.;
        for (Int d = 0; d < nd; ++d) {
          s += cand(d);
        }
        if (s > 1. + 1e-14) {
          continue;
        }
      }
      if (shape == _gst_prism and cand(1) + cand(2) > 1. + 1e-14) {
        continue;
      }
      Vector<Real> N(nn);
      EC::computeShapes(cand, N);
      Int one = -1;
      bool kronecker = true;
      for (Int j = 0; j < nn; ++j) {
        if (std::abs(N(j) - 1.) < 1e-12) {
          if (one >= 0) {
            kronecker = false;
          }
          one = j;
        } else if (std::abs(N(j)) > 1e-12) {
          kronecker = false;
        }
      }
      if (kronecker and one >= 0) {
        coords.col(one) = cand;
        ++found[static_cast<std::size_t>(one)];
      }
    }
    for (Int j = 0; j < nn; ++j) {
      if (found[static_cast<std::size_t>(j)] != 1) {
        throw std::runtime_error(type_name(type) + ": could not locate node " +
                                 std::to_string(j) +
                                 " on the reference lattice");
      }
    }
    return coords;
  }
}

/* -------------------------------------------------------------------------- */
/* Distorted physical elements                                                */
/* -------------------------------------------------------------------------- */
/// Physical coordinates (sdim x nn) of a distorted element: affine map of
/// the reference nodes plus, for `curved`, a smooth non-linear perturbation
/// (exercises the iso-parametric map of quadratic elements).
Matrix<Real> distort(const Matrix<Real> & xi, Int nd, Int sdim, bool curved) {
  // Well-conditioned, non-orthogonal affine part.
  const double A[3][3] = {{1.30, 0.21, -0.17}, {-0.12, 0.92, 0.33}, {0.25, -0.18, 1.11}};
  const double b[3] = {0.37, -1.21, 2.03};
  Matrix<Real> X(sdim, xi.cols());
  for (Eigen::Index n = 0; n < xi.cols(); ++n) {
    for (Int i = 0; i < sdim; ++i) {
      double v = b[i];
      for (Int j = 0; j < nd; ++j) {
        v += A[i][j] * xi(j, n);
      }
      if (curved) {
        double s = 0.;
        for (Int j = 0; j < nd; ++j) {
          s += (j + 1.) * xi(j, n);
        }
        v += 0.05 * std::sin(1.3 * s + 0.4 * i) * (i + 1.) / sdim;
      }
      X(i, n) = v;
    }
  }
  return X;
}

/* -------------------------------------------------------------------------- */
/* Writers shared by all kinds                                                */
/* -------------------------------------------------------------------------- */
template <ElementType type>
void write_topology(ap::FixtureWriter & w) {
  using EC = ElementClass<type>;
  auto & p = w.parameters();
  p["element_type"] = type_name(type);
  p["kind"] = kind_name(EC::getKind());
  p["spatial_dimension"] = static_cast<long long>(EC::getSpatialDimension());
  p["natural_space_dimension"] =
      static_cast<long long>(EC::getNaturalSpaceDimension());
  p["nb_nodes_per_element"] = static_cast<long long>(EC::getNbNodesPerElement());
  p["nb_nodes_per_interpolation_element"] =
      static_cast<long long>(EC::getNbNodesPerInterpolationElement());
  p["default_quadrature_order"] =
      static_cast<long long>(ElementClassProperty<type>::polynomial_degree);
  p["default_nb_quadrature_points"] =
      static_cast<long long>(GaussIntegrationElement<type>::getNbQuadraturePoints());
  p["nb_facet_types"] = static_cast<long long>(EC::getNbFacetTypes());
  ap::Json ftypes = ap::Json::array();
  for (Int t = 0; t < EC::getNbFacetTypes(); ++t) {
    ap::Json f = ap::Json::object();
    if constexpr (EC::getKind() != _ek_structural) {
      f["facet_type"] = type_name(EC::getFacetType(t));
    }
    auto conn = EC::getFacetLocalConnectivityPerElement(t);
    f["nb_facets"] = static_cast<long long>(conn.rows());
    f["nb_nodes_per_facet"] = static_cast<long long>(conn.cols());
    ftypes.push(f);
    std::vector<std::int64_t> c;
    for (Eigen::Index i = 0; i < conn.rows(); ++i) {
      for (Eigen::Index j = 0; j < conn.cols(); ++j) {
        c.push_back(conn(i, j));
      }
    }
    w.add("facet_connectivity/" + std::to_string(t),
          sz({conn.rows(), conn.cols()}), {"facet", "node_local"}, "reference", c)
        .element_type(type_name(type))
        .description("local facet connectivity, facet type index " +
                     std::to_string(t))
        .tolerance_class("exact");
  }
  p["facet_types"] = ftypes;
  if constexpr (EC::getKind() == _ek_regular) {
    p["p1_type"] = type_name(EC::getP1ElementType());
  }
}

/// Quadrature points and weights of order `n`.
template <ElementType type, int n>
void write_quadrature(ap::FixtureWriter & w, std::vector<long long> & nq_out) {
  using GIE = GaussIntegrationElement<type, n>;
  auto pts = GIE::getQuadraturePoints(); // max(1,nd) x nq, column = point
  auto wts = GIE::getWeights();
  const long nq = pts.cols();
  const long nd = ElementClass<type>::getNaturalSpaceDimension();
  std::vector<double> P;
  for (long q = 0; q < nq; ++q) {
    for (long d = 0; d < std::max(nd, 1L); ++d) {
      P.push_back(pts(d, q));
    }
  }
  std::vector<double> W(wts.data(), wts.data() + wts.size());
  const auto o = "order_" + std::to_string(n);
  w.add("quad_points/" + o, sz({nq, std::max(nd, 1L)}), {"quad", "natural_dim"},
        "quadrature", P)
      .element_type(type_name(type))
      .description("Gauss points in natural coordinates (GaussIntegrationElement<type," +
                   std::to_string(n) + ">::getQuadraturePoints)")
      .tolerance_class("kernel");
  w.add("quad_weights/" + o, sz({nq}), {"quad"}, "quadrature", W)
      .element_type(type_name(type))
      .tolerance_class("kernel");
  nq_out.push_back(nq);
}

/// Second natural derivatives at `xi` exactly as upstream's
/// `ElementClassHelper<_ek_regular>::getD2NDS2` evaluates them (dynamic
/// `Matrix<Real>(nd * nd, nn)`, `ElementClass<type>::computeD2NDS2`), appended
/// row-major to `out`; `false` if upstream throws `NotImplementedException`
/// (`AKANTU_TO_IMPLEMENT`) for this interpolation.
template <ElementType type>
bool append_d2nds2(const Vector<Real> & xi, std::vector<double> & out) {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  Matrix<Real> d2nds2(nd * nd, nn);
  d2nds2.setConstant(7.); // overwritten (upstream zero()s first)
  try {
    EC::computeD2NDS2(xi, d2nds2);
  } catch (debug::NotImplementedException &) {
    return false;
  }
  append_row_major(out, d2nds2);
  return true;
}

/// Shapes N [quad, node], natural derivatives dN/ds [quad, natural_dim,
/// node] and, where upstream implements them, second derivatives
/// d2N/ds2 [quad, row, node] (row of upstream's nd*nd x nn matrix) at the
/// order-n Gauss points
/// (Lagrangian interpolations).
template <ElementType type, int n> void write_shapes(ap::FixtureWriter & w) {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  auto pts = GaussIntegrationElement<type, n>::getQuadraturePoints();
  const long nq = pts.cols();
  std::vector<double> Ns, dN, d2N;
  bool d2_implemented = true;
  for (long q = 0; q < nq; ++q) {
    Vector<Real> xi = pts.col(q);
    Vector<Real> N(nn);
    EC::computeShapes(xi, N);
    for (Int j = 0; j < nn; ++j) {
      Ns.push_back(N(j));
    }
    if constexpr (nd > 0) {
      Matrix<Real> dnds(nd, nn);
      EC::computeDNDS(xi, dnds);
      append_row_major(dN, dnds);
    }
    Vector<Real> xi_nd = xi.head(nd);
    d2_implemented = append_d2nds2<type>(xi_nd, d2N) and d2_implemented;
  }
  auto & p = w.parameters();
  if (p.has("d2nds2_implemented") and
      (p["d2nds2_implemented"].dump() == "true") != d2_implemented) {
    throw std::runtime_error(type_name(type) +
                             ": computeD2NDS2 support differs between orders");
  }
  p["d2nds2_implemented"] = d2_implemented;
  const auto o = "order_" + std::to_string(n);
  w.add("shapes/" + o, sz({nq, nn}), {"quad", "node_local"}, "quadrature", Ns)
      .element_type(type_name(type))
      .tolerance_class("kernel");
  if constexpr (nd > 0) {
    w.add("dnds/" + o, sz({nq, nd, nn}), {"quad", "natural_dim", "node_local"},
          "quadrature", dN)
        .element_type(type_name(type))
        .description("dN_j/ds_i at the order-" + std::to_string(n) + " points")
        .tolerance_class("kernel");
  }
  if (d2_implemented) {
    w.add("d2nds2/" + o, sz({nq, nd * nd, nn}), {"quad", "row", "node_local"}, "quadrature",
          d2N)
        .element_type(type_name(type))
        .description("computeD2NDS2 (ElementClassHelper::getD2NDS2 operand types) at the "
                     "order-" + std::to_string(n) + " points: row r of the upstream "
                     "nd*nd x nn matrix")
        .tolerance_class("kernel");
  }
}

template <ElementType type, bool with_shapes, int... ns>
void write_all_orders(ap::FixtureWriter & w, std::integer_sequence<int, ns...>) {
  std::vector<long long> nq;
  (write_quadrature<type, ns + 1>(w, nq), ...);
  if constexpr (with_shapes) {
    (write_shapes<type, ns + 1>(w), ...);
  }
  std::vector<long long> orders{(ns + 1)...};
  w.parameters()["quadrature_orders"] = ap::Json::array_of(orders);
  w.parameters()["nb_quadrature_points_per_order"] = ap::Json::array_of(nq);
}

/// Physical-element data: Jacobians, physical derivatives, quadrature point
/// positions, integrals of monomials, inradius, inverse map, normals.
template <ElementType type, int... ns>
void write_physical(ap::FixtureWriter & w, const std::string & tag,
                    const Matrix<Real> & X, std::integer_sequence<int, ns...>) {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  const Int sdim = X.rows();
  const bool square = (sdim == nd);
  const auto et = type_name(type);
  const auto pre = "physical/" + tag + "/";

  std::vector<double> nodes;
  append_row_major(nodes, X.transpose());
  w.add(pre + "nodes", sz({nn, sdim}), {"node_local", "spatial_dim"}, "reference",
        nodes)
      .element_type(et)
      .description("input: physical node coordinates")
      .tolerance_class("exact");

  // Default order: J, det J, dN/dx, x_q.
  {
    constexpr int n = ElementClassProperty<type>::polynomial_degree;
    auto pts = GaussIntegrationElement<type, n>::getQuadraturePoints();
    const long nq = pts.cols();
    std::vector<double> Jm, detJ, dNdx, xq, nrm;
    for (long q = 0; q < nq; ++q) {
      Vector<Real> xi = pts.col(q);
      Matrix<Real> dnds(nd, nn);
      EC::computeDNDS(xi, dnds);
      Matrix<Real> J = EC::computeJMat(dnds, X);
      append_row_major(Jm, J);
      detJ.push_back(EC::computeJacobian(J));
      Vector<Real> N(nn);
      EC::computeShapes(xi, N);
      Vector<Real> x = X * N;
      for (Int d = 0; d < sdim; ++d) {
        xq.push_back(x(d));
      }
      if (square) {
        Matrix<Real> B(sdim, nn);
        EC::computeShapeDerivatives(J, dnds, B);
        append_row_major(dNdx, B);
      }
    }
    w.add(pre + "jmat", sz({nq, nd, sdim}), {"quad", "natural_dim", "spatial_dim"},
          "quadrature", Jm)
        .element_type(et)
        .description("J = dnds * X^T at the default-order points (computeJMat)")
        .tolerance_class("kernel");
    w.add(pre + "jacobians", sz({nq}), {"quad"}, "quadrature", detJ)
        .element_type(et)
        .description("computeJacobian(J): det J, |J| or |J0 x J1| (embedded)")
        .tolerance_class("kernel");
    w.add(pre + "quad_coords", sz({nq, sdim}), {"quad", "spatial_dim"}, "quadrature",
          xq)
        .element_type(et)
        .tolerance_class("kernel");
    if (square) {
      w.add(pre + "dndx", sz({nq, sdim, nn}), {"quad", "spatial_dim", "node_local"},
            "quadrature", dNdx)
          .element_type(et)
          .description("dN_j/dx_i = J^-1 dnds (computeShapeDerivatives)")
          .tolerance_class("kernel");
    }
    if constexpr (nd >= 1 and nd <= 2) {
      if (sdim == nd + 1) {
        Matrix<Real> normals(sdim, nq);
        Matrix<Real> P = pts;
        EC::computeNormalsOnNaturalCoordinates(P, X, normals);
        append_row_major(nrm, normals.transpose());
        w.add(pre + "normals", sz({nq, sdim}), {"quad", "spatial_dim"}, "quadrature",
              nrm)
            .element_type(et)
            .description("computeNormalsOnNaturalCoordinates at the default-order points")
            .tolerance_class("kernel");
      }
    }
  }

  // Integrals of monomials x^a y^b z^c (a+b+c <= order) for every order.
  auto mono = monomials(sdim, max_order<type>());
  {
    std::vector<std::int64_t> ex;
    for (auto & m : mono) {
      for (Int d = 0; d < sdim; ++d) {
        ex.push_back(m[static_cast<std::size_t>(d)]);
      }
    }
    w.add(pre + "monomials", sz({static_cast<long>(mono.size()), sdim}),
          {"entry", "spatial_dim"}, "table", ex)
        .element_type(et)
        .description("exponents of the integrated monomials")
        .tolerance_class("exact");
  }
  auto integrate = [&](auto order_tag) {
    constexpr int n = decltype(order_tag)::value;
    auto pts = GaussIntegrationElement<type, n>::getQuadraturePoints();
    auto wts = GaussIntegrationElement<type, n>::getWeights();
    std::vector<double> res(mono.size(), 0.);
    for (long q = 0; q < pts.cols(); ++q) {
      Vector<Real> xi = pts.col(q);
      Matrix<Real> dnds(nd, nn);
      EC::computeDNDS(xi, dnds);
      Matrix<Real> J = EC::computeJMat(dnds, X);
      const double dJ = EC::computeJacobian(J);
      Vector<Real> N(nn);
      EC::computeShapes(xi, N);
      Vector<Real> x = X * N;
      for (std::size_t k = 0; k < mono.size(); ++k) {
        res[k] += wts(q) * dJ * eval_monomial(mono[k], x);
      }
    }
    w.add(pre + "integrals/order_" + std::to_string(n), sz({static_cast<long>(res.size())}),
          {"entry"}, "table", res)
        .element_type(et)
        .description("sum_q w_q |J_q| f(x_q) for each monomial, order " +
                     std::to_string(n) + " rule")
        .tolerance_class("kernel");
  };
  (integrate(std::integral_constant<int, ns + 1>{}), ...);

  if (square) {
    // Inradius (characteristic length).
    std::vector<double> r{EC::getInradius(X)};
    w.add(pre + "inradius", {}, {}, "scalar", r)
        .element_type(et)
        .tolerance_class("kernel");
    // Inverse map of the order-2 points and the reference centroid.
    auto pts = GaussIntegrationElement<type, 2>::getQuadraturePoints();
    std::vector<double> xin, xiout, xiref;
    for (long q = 0; q < pts.cols(); ++q) {
      Vector<Real> xi = pts.col(q);
      Vector<Real> N(nn);
      EC::computeShapes(xi, N);
      Vector<Real> x = X * N;
      Vector<Real> xi_back(nd);
      EC::inverseMap(x, X, xi_back);
      for (Int d = 0; d < sdim; ++d) {
        xin.push_back(x(d));
      }
      for (Int d = 0; d < nd; ++d) {
        xiout.push_back(xi_back(d));
        xiref.push_back(xi(d));
      }
    }
    const long np = pts.cols();
    w.add(pre + "inverse_map/x", sz({np, sdim}), {"point", "spatial_dim"}, "table", xin)
        .element_type(et)
        .description("input: physical points (images of the order-2 Gauss points)")
        .tolerance_class("kernel");
    w.add(pre + "inverse_map/xi", sz({np, nd}), {"point", "natural_dim"}, "table", xiout)
        .element_type(et)
        .description("inverseMap(x, X) with default max_iterations=100, tolerance=1e-10")
        .tolerance_class("nonlinear_solve");
    w.add(pre + "inverse_map/xi_exact", sz({np, nd}), {"point", "natural_dim"}, "table",
          xiref)
        .element_type(et)
        .description("natural coordinates the points were generated from")
        .tolerance_class("kernel");
  }
}

/// `contains` on a fixed set of natural points (inside, on the boundary,
/// outside).
template <ElementType type> void write_contains(ap::FixtureWriter & w) {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  if constexpr (nd > 0) {
    const std::vector<double> vals{-1.5, -1.0, -0.5, 0.0, 0.25, 0.5, 1.0, 1.0000001};
    std::vector<double> pts;
    std::vector<bool> in;
    std::vector<std::size_t> idx(nd, 0);
    while (true) {
      Vector<Real> c(nd);
      for (Int d = 0; d < nd; ++d) {
        c(d) = vals[idx[static_cast<std::size_t>(d)]];
        pts.push_back(c(d));
      }
      in.push_back(EC::contains(c));
      Int d = nd - 1;
      while (d >= 0 and ++idx[static_cast<std::size_t>(d)] == vals.size()) {
        idx[static_cast<std::size_t>(d)] = 0;
        --d;
      }
      if (d < 0) {
        break;
      }
    }
    const long np = static_cast<long>(in.size());
    w.add("contains/points", sz({np, nd}), {"point", "natural_dim"}, "table", pts)
        .element_type(type_name(type))
        .tolerance_class("exact");
    w.add_bool("contains/inside", sz({np}), {"point"}, "table", in)
        .element_type(type_name(type))
        .description("GeometricalElement::contains")
        .tolerance_class("exact");
  }
}

/* -------------------------------------------------------------------------- */
/* Probes per kind                                                            */
/* -------------------------------------------------------------------------- */
std::string case_id(ElementType type) {
  return "fe/element_class/" + type_name(type);
}

template <ElementType type> void probe_regular(const std::filesystem::path & root) {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  ap::FixtureWriter w(root, case_id(type),
                      "Element-class reference data of " + type_name(type) +
                          ": reference nodes, facets, Gauss rules of every order, "
                          "shape functions and derivatives, distorted and embedded "
                          "physical elements",
                      kProbe, kProbeVersion);
  w.add_tag("element_class");
  w.add_tag("regular");
  write_topology<type>(w);
  constexpr int maxo = max_order<type>();
  write_all_orders<type, true>(w, std::make_integer_sequence<int, maxo>{});

  auto xi = node_natural_coordinates<type>();
  if constexpr (nd > 0) {
    std::vector<double> c;
    append_row_major(c, xi.transpose());
    w.add("node_natural_coords", sz({nn, nd}), {"node_local", "natural_dim"},
          "reference", c)
        .element_type(type_name(type))
        .description("reference node positions (located where N_j = delta_ij)")
        .tolerance_class("exact");

    // Same-dimension distorted elements.
    write_physical<type>(w, "affine", distort(xi, nd, nd, false),
                         std::make_integer_sequence<int, maxo>{});
    write_physical<type>(w, "curved", distort(xi, nd, nd, true),
                         std::make_integer_sequence<int, maxo>{});
    // Embedded in one more dimension (facet-like use: normals, |J|).
    if constexpr (nd <= 2) {
      write_physical<type>(w, "embedded", distort(xi, nd, nd + 1, true),
                           std::make_integer_sequence<int, maxo>{});
    }
  }
  write_contains<type>(w);
  w.finish();
  std::cout << "  " << case_id(type) << "\n";
}

template <ElementType type> void probe_cohesive(const std::filesystem::path & root) {
  ap::FixtureWriter w(root, case_id(type),
                      "Element-class reference data of the cohesive element " +
                          type_name(type) +
                          ": topology, Gauss rules, shapes of the facet "
                          "interpolation",
                      kProbe, kProbeVersion);
  w.add_tag("element_class");
  w.add_tag("cohesive");
  write_topology<type>(w);
  constexpr int maxo = max_order<type>();
  write_all_orders<type, true>(w, std::make_integer_sequence<int, maxo>{});
  w.finish();
  std::cout << "  " << case_id(type) << "\n";
}

template <ElementType type> void probe_structural(const std::filesystem::path & root) {
  using EC = ElementClass<type>;
  constexpr Int nd = EC::getNaturalSpaceDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  ap::FixtureWriter w(root, case_id(type),
                      "Element-class reference data of the structural element " +
                          type_name(type) +
                          ": Gauss rules and interpolation kernels (shapes N, "
                          "derivatives) on given local coordinates",
                      kProbe, kProbeVersion);
  w.add_tag("element_class");
  w.add_tag("structural");
  write_topology<type>(w);
  constexpr int maxo = max_order<type>();
  write_all_orders<type, false>(w, std::make_integer_sequence<int, maxo>{});
  auto & p = w.parameters();
  p["nb_degree_of_freedom"] = static_cast<long long>(EC::getNbDegreeOfFreedom());
  p["nb_stress_components"] = static_cast<long long>(EC::getNbStressComponents());
  constexpr Int ndnds = EC::interpolation_property::dnds_columns;
  p["dnds_columns"] = static_cast<long long>(ndnds);

  // Node coordinates (spatial_dimension x nn) as passed by ShapeStructural.
  const Int sdim = EC::getSpatialDimension();
  Matrix<Real> X(sdim, nn);
  X.setZero();
  if constexpr (nn == 2) { // beams: length 1.7, oblique
    X(0, 1) = 1.7 * 0.8;
    X(1, 1) = 1.7 * 0.6;
  } else { // DKT triangle in 3D
    X << 0.1, 1.3, 0.2, //
        -0.2, 0.1, 0.9, //
        0.05, 0.3, -0.4;
  }
  std::vector<double> xs;
  append_row_major(xs, X.transpose());
  w.add("structural/nodes", sz({nn, sdim}), {"node_local", "spatial_dim"}, "reference",
        xs)
      .element_type(type_name(type))
      .description("input: node coordinates passed to computeShapes/computeDNDS")
      .tolerance_class("exact");

  constexpr int n = ElementClassProperty<type>::polynomial_degree;
  auto pts = GaussIntegrationElement<type, n>::getQuadraturePoints();
  const long nq = pts.cols();
  std::vector<double> Ns, Bs;
  long nrow = 0, ncol = 0;
  for (long q = 0; q < nq; ++q) {
    Vector<Real> xi = pts.col(q);
    Matrix<Real> dnds(nd, ndnds);
    dnds.setZero();
    EC::computeDNDS(xi, X, dnds);
    append_row_major(Bs, dnds);
    if constexpr (nn == 2) { // beams: Hermite/Lagrange shapes
      Matrix<Real> N(EC::getNbDegreeOfFreedom(), EC::getNbDegreeOfFreedom() * nn);
      N.setZero();
      // computeShapes takes the coordinates in the local (rotated) frame.
      Matrix<Real> xl(1, nn);
      xl << 0., 1.7;
      EC::computeShapes(xi, xl, N);
      nrow = N.rows();
      ncol = N.cols();
      append_row_major(Ns, N);
    }
  }
  w.add("structural/dnds", sz({nq, nd, ndnds}), {"quad", "natural_dim", "component"},
        "quadrature", Bs)
      .element_type(type_name(type))
      .description("computeDNDS(xi, X, dnds) at the default-order points")
      .tolerance_class("kernel");
  if constexpr (nn == 2) {
    w.add("structural/shapes", sz({nq, nrow, ncol}), {"quad", "row", "col"},
          "quadrature", Ns)
        .element_type(type_name(type))
        .description("computeShapes(xi, x_local, N), x_local = [0, 1.7]")
        .tolerance_class("kernel");
  }
  w.finish();
  std::cout << "  " << case_id(type) << "\n";
}

} // namespace

int main(int argc, char ** argv) {
  if (argc < 2) {
    std::cerr << "usage: " << argv[0] << " <fixtures_root> [type ...]\n";
    return 2;
  }
  const std::filesystem::path root = argv[1];
  std::set<std::string> only(argv + 2, argv + argc);
  auto want = [&](ElementType t) { return only.empty() or only.count(type_name(t)) > 0; };
  int failures = 0;
  auto guarded = [&](ElementType t, auto && fn) {
    if (not want(t)) {
      return;
    }
    try {
      fn();
    } catch (std::exception & e) {
      std::cerr << "element_probe: " << type_name(t) << " failed: " << e.what() << "\n";
      ++failures;
    }
  };
#define PROBE(kind, t) guarded(t, [&] { probe_##kind<t>(root); })
  std::cout << "element_probe: writing under " << root << "\n";
  PROBE(regular, _point_1);
  PROBE(regular, _segment_2);
  PROBE(regular, _segment_3);
  PROBE(regular, _triangle_3);
  PROBE(regular, _triangle_6);
  PROBE(regular, _quadrangle_4);
  PROBE(regular, _quadrangle_8);
  PROBE(regular, _tetrahedron_4);
  PROBE(regular, _tetrahedron_10);
  PROBE(regular, _pentahedron_6);
  PROBE(regular, _pentahedron_15);
  PROBE(regular, _hexahedron_8);
  PROBE(regular, _hexahedron_20);
#if defined(AKANTU_COHESIVE_ELEMENT)
  PROBE(cohesive, _cohesive_1d_2);
  PROBE(cohesive, _cohesive_2d_4);
  PROBE(cohesive, _cohesive_2d_6);
  PROBE(cohesive, _cohesive_3d_6);
  PROBE(cohesive, _cohesive_3d_12);
  PROBE(cohesive, _cohesive_3d_8);
  PROBE(cohesive, _cohesive_3d_16);
#endif
#if defined(AKANTU_STRUCTURAL_MECHANICS)
  PROBE(structural, _bernoulli_beam_2);
  PROBE(structural, _bernoulli_beam_3);
  PROBE(structural, _discrete_kirchhoff_triangle_18);
#endif
#undef PROBE
  // TODO(igfem): _igfem_segment_3/_igfem_triangle_4/_igfem_triangle_5 need the
  // IGFEM element classes (extra_packages/igfem); not probed yet.
  return failures == 0 ? 0 : 1;
}
