// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records FE-engine outputs that fe_engine_probe does not replay:
//
//  * consistent field matrices: the elemental matrices
//    FEEngineTemplate::assembleFieldMatrix hands to
//    DOFManager::assembleElementalMatricesToMatrix (captured by overriding
//    that virtual; src/fe_engine/fe_engine_template_tmpl_field.hh:265-400);
//  * lumped field arrays: the elemental arrays assembleFieldLumped hands to
//    DOFManager::assembleElementalArrayLocalArray (row sum / diagonal
//    scaling, fe_engine_template_tmpl_field.hh:33-232);
//  * arbitrary-order integration: IntegratorGauss::integrate<type, n> for
//    n = 1..5 (field and scalar versions, integrator_gauss_inline_impl.hh);
//  * elemental-field interpolation from integration points
//    (FEEngineTemplate::initElementalFieldInterpolationFromIntegrationPoints /
//    interpolateElementalFieldFromIntegrationPoints, shape_functions.cc,
//    shape_functions_inline_impl.hh) with reordered element filters on
//    not-ghost and ghost elements, for the types where upstream is defined.
//
//  * evaluation at real coordinates (FE-024 / FE-027, audit
//    audit gaps): FEEngine::computeShapes, computeShapeDerivatives
//    and interpolate at real points (the integration-point coordinates of
//    fe_engine_probe's inverse-map records), fixtures/fe/engine_real/<type>
//    (probe_real below, written by the same run).
//
// Output: fixtures/fe/engine_extra/<type> on the upstream
// test/test_fe_engine/<type>.msh meshes (ghost elements appended, see below).
// Usage: fe_extra_probe <fixtures_root> <reference_src_root> [type ...]

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
#include <shape_lagrange.hh>

// D-050: the Eigen cache-size pin (after the Akantu
// headers); the fixture writer refuses an unpinned x86_64 run.
#include "akantu_probe/eigen_pin.hh"

#include <algorithm>
#include <cstdint>
#include <filesystem>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;

namespace fe_extra {

inline constexpr const char * kProbe = "fe_extra_probe";
inline constexpr const char * kProbeVersion = "1";

/// Elemental matrices are recorded for at most this many elements (the
/// computation is independent per element; keeps every case < 1 MiB).
inline constexpr Int kMaxMassElements = 24;

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

inline std::vector<double> flat(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

/// Row-major copy of the first `n` tuples of an Array of per-tuple
/// column-major (rows x cols) matrices: [tuple, row, col].
inline std::vector<double> tuples_row_major(const Array<Real> & a, Int rows, Int cols,
                                            Int n) {
  std::vector<double> out;
  out.reserve(n * rows * cols);
  for (Int t = 0; t < n; ++t) {
    for (Int r = 0; r < rows; ++r) {
      for (Int c = 0; c < cols; ++c) {
        out.push_back(a(t, r + c * rows));
      }
    }
  }
  return out;
}

/// Same as tuples_row_major for a flat copy of `n` tuples.
inline std::vector<double> row_major(const std::vector<double> & a, Int rows, Int cols,
                                     Int n) {
  std::vector<double> out;
  out.reserve(n * rows * cols);
  for (Int t = 0; t < n; ++t) {
    for (Int r = 0; r < rows; ++r) {
      for (Int c = 0; c < cols; ++c) {
        out.push_back(a[t * rows * cols + r + c * rows]);
      }
    }
  }
  return out;
}

/// Deterministic, exactly representable test values in [-2, 2).
inline double pseudo(long k) { return double((k * 37 + 11) % 64) / 16. - 2.; }

/// rho(d, q) = 1.5 + 0.25 d + 0.01 e + 0.125 q (the Rust test evaluates the
/// same expression in the same order).
inline void rho_functor(Matrix<Real> & rho, const Element & el) {
  for (Int d = 0; d < rho.rows(); ++d) {
    for (Int q = 0; q < rho.cols(); ++q) {
      rho(d, q) = 1.5 + 0.25 * d + 0.01 * el.element + 0.125 * q;
    }
  }
}

/// DOF manager that records what the FE engine hands to it.
class CaptureDOFManager : public DOFManagerDefault {
public:
  CaptureDOFManager(Mesh & mesh, const ID & id) : DOFManagerDefault(mesh, id) {}

  void assembleElementalMatricesToMatrix(const ID & matrix_id, const ID & dof_id,
                                         const Array<Real> & elementary_mat,
                                         ElementType type, GhostType ghost_type,
                                         const MatrixType & elemental_matrix_type,
                                         const Array<Int> & filter_elements) override {
    matrices.assign(elementary_mat.data(),
                    elementary_mat.data() + elementary_mat.size() * elementary_mat.getNbComponent());
    matrices_size = elementary_mat.size();
    matrices_nb_component = elementary_mat.getNbComponent();
    matrix_type = elemental_matrix_type;
    DOFManagerDefault::assembleElementalMatricesToMatrix(
        matrix_id, dof_id, elementary_mat, type, ghost_type, elemental_matrix_type,
        filter_elements);
  }

  void assembleElementalArrayLocalArray(const Array<Real> & elementary_vect,
                                        Array<Real> & array_assembeled, ElementType type,
                                        GhostType ghost_type, Real scale_factor,
                                        const Array<Int> & filter_elements) override {
    arrays.assign(elementary_vect.data(),
                  elementary_vect.data() + elementary_vect.size() * elementary_vect.getNbComponent());
    arrays_size = elementary_vect.size();
    arrays_nb_component = elementary_vect.getNbComponent();
    array_scale = scale_factor;
    DOFManager::assembleElementalArrayLocalArray(elementary_vect, array_assembeled, type,
                                                 ghost_type, scale_factor,
                                                 filter_elements);
  }

  std::vector<double> matrices;
  Int matrices_size{0};
  Int matrices_nb_component{0};
  std::vector<double> arrays;
  Int arrays_size{0};
  Int arrays_nb_component{0};
  MatrixType matrix_type{_mt_not_defined};
  Real array_scale{0.};
};

using FEMRegular = FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_regular>;

/// Arbitrary-order integration with the order-`n` rule.
template <ElementType type, Int n>
void probe_order(ap::FixtureWriter & w, FEMRegular & fem, const Mesh & mesh,
                 const std::string & et) {
  const Int ne = mesh.getNbElement(type);
  auto points = fem.getIntegrator().template getIntegrationPoints<type, n>();
  const Int nq = points.cols();
  const Int nbdof = 2;
  Array<Real> f(ne * nq, nbdof, "f");
  for (Int k = 0; k < ne * nq * nbdof; ++k) {
    f.data()[k] = pseudo(k + 3 * n);
  }
  const std::string pre = "order_" + std::to_string(n) + "/";
  w.add(pre + "field", sz({ne, nq, nbdof}), {"element", "quad", "component"}, "quadrature",
        flat(f))
      .element_type(et)
      .description("input: quadrature field at the order-n points")
      .tolerance_class("exact");
  Array<Real> intf(0, nbdof);
  fem.getIntegrator().template integrate<type, n>(f, intf, nbdof, _not_ghost);
  w.add(pre + "integrated", sz({ne, nbdof}), {"element", "component"}, "elemental",
        flat(intf))
      .element_type(et)
      .description("IntegratorGauss::integrate<type, n>(f, intf, 2, _not_ghost)")
      .tolerance_class("kernel");
  Array<Real> f0(ne * nq, 1, "f0");
  for (Int k = 0; k < ne * nq; ++k) {
    f0(k) = f(k, 0);
  }
  std::vector<double> s{fem.getIntegrator().template integrate<type, n>(f0, _not_ghost)};
  w.add(pre + "integral_scalar", {}, {}, "scalar", s)
      .description("IntegratorGauss::integrate<type, n>(component 0): Math::reduce of the "
                   "per-element integrals")
      .tolerance_class("kernel");
}

/// Upstream defines the elemental-field interpolation for these types only
/// (UD-096: out-of-bounds/uninitialised for hexahedra and pentahedra,
/// AKANTU_TO_IMPLEMENT for _hexahedron_20).
template <ElementType type> constexpr bool elemental_supported() {
  return type == _segment_2 or type == _segment_3 or type == _triangle_3 or
         type == _triangle_6 or type == _quadrangle_4 or type == _quadrangle_8 or
         type == _tetrahedron_4 or type == _tetrahedron_10;
}

/// Version of the fe/engine_real cases (probe_real).
inline constexpr const char * kRealProbeVersion = "1";

/// FE-024: shapes, physical shape derivatives and the interpolation of a
/// nodal field at real points (FEEngine::computeShapes(real_coords, ...),
/// computeShapeDerivatives(real_coords, ...), interpolate(real_coords, ...);
/// shape_lagrange_inline_impl.hh: inverse map, then the natural-coordinate
/// kernels). Points: the integration-point coordinates of a spread subset
/// of at most kMaxMassElements elements of the upstream test mesh.
template <ElementType type>
void probe_real(const std::filesystem::path & root, const std::filesystem::path & src) {
  using EC = ElementClass<type>;
  constexpr Int dim = EC::getSpatialDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  constexpr Int nc = 2;
  const auto et = type_name(type);
  const auto rel = "test/test_fe_engine/" + et + ".msh";
  const auto file = src / rel;
  Mesh mesh(dim);
  mesh.read(file.string());
  const Int ne = mesh.getNbElement(type);
  FEMRegular fem(mesh, dim, "my_fem");
  fem.initShapeFunctions(_not_ghost);
  const Int nq = fem.getNbIntegrationPoints(type);
  Array<Real> xq(0, dim);
  fem.computeIntegrationPointsCoordinates(xq, type);
  std::vector<std::int64_t> elements;
  const Int nsel = std::min(ne, kMaxMassElements);
  for (Int i = 0; i < nsel; ++i) {
    elements.push_back(static_cast<std::int64_t>(ne <= kMaxMassElements ? i : (i * ne) / nsel));
  }
  Array<Real> field(mesh.getNbNodes(), nc);
  for (Int k = 0; k < field.size() * nc; ++k) {
    field.data()[k] = pseudo(5 * k + 1);
  }
  std::vector<double> points, shapes, derivatives, interpolated;
  for (auto e : elements) {
    const Element el{type, Idx(e), _not_ghost};
    const Matrix<Real> nodal = mesh.extractNodalValuesFromElement(field, el);
    for (Int k = 0; k < nq; ++k) {
      Vector<Real> x(dim);
      for (Int d = 0; d < dim; ++d) {
        x(d) = xq(Int(e) * nq + k, d);
        points.push_back(x(d));
      }
      Vector<Real> n(nn);
      fem.computeShapes(x, Int(e), type, n);
      for (Int a = 0; a < nn; ++a) {
        shapes.push_back(n(a));
      }
      Matrix<Real> b(dim, nn);
      fem.computeShapeDerivatives(x, Int(e), type, b);
      for (Int d = 0; d < dim; ++d) {
        for (Int a = 0; a < nn; ++a) {
          derivatives.push_back(b(d, a));
        }
      }
      Vector<Real> out(nc);
      fem.interpolate(x, nodal, out, el);
      for (Int c = 0; c < nc; ++c) {
        interpolated.push_back(out(c));
      }
    }
  }
  const auto ns = static_cast<long>(elements.size());
  ap::FixtureWriter w(root, "fe/engine_real/" + et,
                      "FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_regular> on the "
                      "upstream test mesh " + rel + ": computeShapes, computeShapeDerivatives "
                      "and interpolate at real points (integration-point coordinates of a "
                      "spread subset of the elements)",
                      kProbe, kRealProbeVersion);
  w.add_tag("fe_engine");
  w.add_tag("wp-lib-small");
  w.add_input_reference("mesh", "upstream:" + rel, file);
  auto & p = w.parameters();
  p["element_type"] = et;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["mesh"] = "mesh_io/test_fe_engine/" + et;
  p["nb_element"] = static_cast<long long>(ne);
  p["nb_quadrature_points"] = static_cast<long long>(nq);
  p["nodal_field"] = "pseudo(5 k + 1), k = flat index of the nb_nodes x 2 nodal array";
  w.add("elements", sz({ns}), {"element"}, "table", elements)
      .description("mesh element indices of the evaluated elements")
      .tolerance_class("exact");
  w.add("points", sz({ns, nq, dim}), {"element", "quad", "spatial_dim"}, "quadrature", points)
      .description("real points: computeIntegrationPointsCoordinates of the element")
      .tolerance_class("exact");
  w.add("nodal_field", sz({field.size(), nc}), {"node", "component"}, "nodal", flat(field))
      .tolerance_class("exact");
  w.add("shapes", sz({ns, nq, nn}), {"element", "quad", "node_local"}, "quadrature", shapes)
      .element_type(et).description("computeShapes(real_coords, element, type, shapes)")
      .tolerance_class("nonlinear_solve");
  w.add("shape_derivatives", sz({ns, nq, dim, nn}),
        {"element", "quad", "spatial_dim", "node_local"}, "quadrature", derivatives)
      .element_type(et)
      .description("computeShapeDerivatives(real_coords, element, type, B), B(d, a) = dN_a/dx_d")
      .tolerance_class("nonlinear_solve");
  w.add("interpolate", sz({ns, nq, nc}), {"element", "quad", "component"}, "quadrature",
        interpolated)
      .element_type(et)
      .description("interpolate(real_coords, extractNodalValuesFromElement(field, el), out, el)")
      .tolerance_class("nonlinear_solve");
  w.finish();
  std::cout << "  fe/engine_real/" << et << "\n";
}

template <ElementType type>
void probe_extra(const std::filesystem::path & root, const std::filesystem::path & src) {
  using EC = ElementClass<type>;
  constexpr Int dim = EC::getSpatialDimension();
  constexpr Int nn = EC::getNbNodesPerInterpolationElement();
  const auto et = type_name(type);
  const auto rel = "test/test_fe_engine/" + et + ".msh";
  const auto file = src / rel;

  Mesh mesh(dim);
  mesh.read(file.string());
  const Int ne = mesh.getNbElement(type);

  // Ghost elements: copies of the connectivity of not-ghost elements
  // (ne - 1, 0, ne / 2) so that the ghost element engine has valid geometry.
  std::vector<Int> ghost_sources{ne - 1, 0, ne / 2};
  {
    MeshAccessor acc(mesh);
    auto & conn = mesh.getConnectivity(type, _not_ghost);
    auto & gconn = acc.getConnectivity(type, _ghost);
    gconn.resize(0);
    for (auto e : ghost_sources) {
      Vector<Idx> c(conn.getNbComponent());
      for (Int k = 0; k < conn.getNbComponent(); ++k) {
        c(k) = conn(e, k);
      }
      gconn.push_back(c);
    }
  }
  const Int nbg = mesh.getNbElement(type, _ghost);

  FEMRegular fem(mesh, dim, "my_fem");
  fem.initShapeFunctions(_not_ghost);
  fem.initShapeFunctions(_ghost);

  ap::FixtureWriter w(root, "fe/engine_extra/" + et,
                      "FEEngineTemplate<IntegratorGauss, ShapeLagrange, _ek_regular> on the "
                      "upstream test mesh " + rel + " (+3 ghost copies): elemental consistent "
                      "field matrices and lumped arrays (as handed to the DOF manager), "
                      "arbitrary-order integration (orders 1-5), elemental-field "
                      "interpolation from integration points (filtered, ghost)",
                      kProbe, kProbeVersion);
  w.add_tag("fe_engine");
  w.add_tag("wp-verify-1");
  w.add_input_reference("mesh", "upstream:" + rel, file);
  auto & p = w.parameters();
  p["element_type"] = et;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["mesh"] = "mesh_io/test_fe_engine/" + et;
  p["nb_element"] = static_cast<long long>(ne);
  p["nb_ghost_element"] = static_cast<long long>(nbg);
  p["nb_quadrature_points"] = static_cast<long long>(fem.getNbIntegrationPoints(type));

  {
    std::vector<std::int64_t> g;
    const auto & gconn = mesh.getConnectivity(type, _ghost);
    for (Int e = 0; e < gconn.size(); ++e) {
      for (Int k = 0; k < gconn.getNbComponent(); ++k) {
        g.push_back(gconn(e, k));
      }
    }
    w.add("ghost_connectivity", sz({nbg, gconn.getNbComponent()}), {"element", "node_local"},
          "table", g)
        .description("input: connectivity of the ghost elements (copies of not-ghost "
                     "elements ne-1, 0, ne/2)")
        .tolerance_class("exact");
  }

  // consistent field matrix (nb_dof = spatial dimension, the solid-mechanics
  // mass) and lumped arrays (same dof count)
  {
    const Int nbdof = dim;
    CaptureDOFManager dof_manager(mesh, "probe_dof_manager");
    Array<Real> dofs(mesh.getNbNodes(), nbdof, 0., "u");
    dof_manager.registerDOFs("u", dofs, _dst_nodal);
    dof_manager.getNewMatrix("M", _symmetric);
    fem.assembleFieldMatrix(rho_functor, "M", "u", dof_manager, type, _not_ghost);
    const Int lmat = nbdof * nn;
    if (dof_manager.matrices_size != ne or dof_manager.matrices_nb_component != lmat * lmat) {
      throw std::runtime_error("unexpected captured elemental matrix layout");
    }
    const Int nrec = std::min(ne, kMaxMassElements);
    p["field_matrix_nb_dof"] = static_cast<long long>(nbdof);
    p["field_matrix_recorded_elements"] = static_cast<long long>(nrec);
    p["field_matrix_type"] = dof_manager.matrix_type == _symmetric ? "symmetric" : "other";
    w.add("field_matrix/elemental", sz({nrec, lmat, lmat}), {"element", "row", "col"},
          "elemental", row_major(dof_manager.matrices, lmat, lmat, nrec))
        .element_type(et)
        .description("assembleFieldMatrix(rho, 'M', 'u' (nb_dof = dim), type): the "
                     "elemental matrices passed to assembleElementalMatricesToMatrix, "
                     "first min(ne, 24) elements, [element, row, col] (node-major dofs)")
        .tolerance_class("kernel");

    CaptureDOFManager dof_manager_l(mesh, "probe_dof_manager_lumped");
    Array<Real> lumped(mesh.getNbNodes(), nbdof, 0.);
    fem.assembleFieldLumped(rho_functor, lumped, dof_manager_l, type, _not_ghost);
    if (dof_manager_l.arrays_size != ne or dof_manager_l.arrays_nb_component != nbdof * nn) {
      throw std::runtime_error("unexpected captured elemental array layout");
    }
    p["lumped_scale_factor"] = dof_manager_l.array_scale;
    w.add("lumped/elemental", sz({ne, nn, nbdof}), {"element", "node_local", "component"},
          "elemental", dof_manager_l.arrays)
        .element_type(et)
        .description("assembleFieldLumped(rho, lumped (nb_dof = dim), type): the elemental "
                     "arrays passed to assembleElementalArrayLocalArray")
        .tolerance_class("kernel");
    w.add("lumped/assembled", sz({lumped.size(), nbdof}), {"node", "component"}, "nodal",
          flat(lumped))
        .description("the assembled lumped array (element-order scatter by the DOF "
                     "manager; DOF-007 obligation)")
        .tolerance_class("kernel");
  }

  // arbitrary-order integration
  probe_order<type, 1>(w, fem, mesh, et);
  probe_order<type, 2>(w, fem, mesh, et);
  probe_order<type, 3>(w, fem, mesh, et);
  probe_order<type, 4>(w, fem, mesh, et);
  probe_order<type, 5>(w, fem, mesh, et);

  // elemental-field interpolation from integration points
  if constexpr (elemental_supported<type>()) {
    const Int nq = fem.getNbIntegrationPoints(type);
    const Int nip = 2;
    const Int nbc = 2;
    ElementTypeMapArray<Real> points("points", "probe");
    ElementTypeMapArray<Idx> filter("filter", "probe");
    ElementTypeMapArray<Real> field("field", "probe");
    ElementTypeMapArray<Real> result("result", "probe");
    std::vector<Int> nf_filter;
    {
      std::vector<Int> f{ne - 1, 1};
      if (ne / 2 != 1 and ne / 2 != ne - 1) {
        f.push_back(ne / 2);
      }
      if (ne > 2) {
        f.push_back(0);
      }
      auto & a = filter.alloc(0, 1, type, _not_ghost);
      for (auto e : f) {
        a.push_back(e);
      }
      auto & g = filter.alloc(0, 1, type, _ghost);
      g.push_back(Idx(nbg - 1));
      g.push_back(Idx(0));
    }
    const auto & X = mesh.getNodes();
    for (auto gt : ghost_types) {
      const auto & conn = mesh.getConnectivity(type, gt);
      const Int nel = conn.size();
      auto & pts = points.alloc(nel * nip, dim, type, gt);
      for (Int e = 0; e < nel; ++e) {
        for (Int i = 0; i < nip; ++i) {
          // point i: weighted mean of the first two nodes, weights (1+i)/4 and
          // (3-i)/4, plus 1/8 of the third node offset (inside or near the
          // element; the fit is polynomial, any point is valid)
          for (Int d = 0; d < dim; ++d) {
            Real x0 = X(conn(e, 0), d);
            Real x1 = X(conn(e, 1), d);
            Real x2 = X(conn(e, std::min<Int>(2, nn - 1)), d);
            pts(e * nip + i, d) =
                (1. + i) / 4. * x0 + (3. - i) / 4. * x1 + 0.125 * (x2 - x0);
          }
        }
      }
      const auto & flt = filter(type, gt);
      auto & fld = field.alloc(flt.size() * nq, nbc, type, gt);
      for (Int k = 0; k < fld.size() * nbc; ++k) {
        fld.data()[k] = pseudo(k + (gt == _ghost ? 101 : 7));
      }
      result.alloc(nel * nip, nbc, type, gt, 0.);
    }
    ElementTypeMapArray<Real> itp_mats("itp_mats", "probe");
    ElementTypeMapArray<Real> inv_mats("inv_mats", "probe");
    fem.initElementalFieldInterpolationFromIntegrationPoints(points, itp_mats, inv_mats,
                                                             &filter);
    for (auto gt : ghost_types) {
      fem.interpolateElementalFieldFromIntegrationPoints(field, itp_mats, inv_mats, result,
                                                         gt, &filter);
    }
    for (auto gt : ghost_types) {
      const std::string g = gt == _ghost ? "ghost" : "not_ghost";
      const auto & flt = filter(type, gt);
      const Int nf = flt.size();
      std::vector<std::int64_t> fi;
      for (Int k = 0; k < nf; ++k) {
        fi.push_back(flt(k));
      }
      w.add("elemental/" + g + "/filter", sz({nf}), {"entry"}, "table", fi)
          .description("input: element filter (reordered subset)")
          .tolerance_class("exact");
      const auto & pts = points(type, gt);
      w.add("elemental/" + g + "/points", sz({pts.size(), dim}), {"point", "spatial_dim"},
            "table", flat(pts))
          .description("input: nip = 2 interpolation points per element (all elements)")
          .tolerance_class("exact");
      w.add("elemental/" + g + "/field", sz({nf, nq, nbc}), {"entry", "quad", "component"},
            "quadrature", flat(field(type, gt)))
          .description("input: field at the integration points of the filtered elements")
          .tolerance_class("exact");
      w.add("elemental/" + g + "/inv_matrices", sz({nf, nq, nq}), {"entry", "row", "col"},
            "elemental", tuples_row_major(inv_mats(type, gt), nq, nq, nf))
          .element_type(et).ghost_type(g)
          .description("inverse of the basis matrix at the integration points (Eigen "
                       "fixed-size inverse), per filter entry")
          .tolerance_class("kernel");
      w.add("elemental/" + g + "/itp_matrices", sz({nf, nip, nq}), {"entry", "row", "col"},
            "elemental", tuples_row_major(itp_mats(type, gt), nip, nq, nf))
          .element_type(et).ghost_type(g)
          .description("basis values at the interpolation points, per filter entry")
          .tolerance_class("kernel");
      std::vector<double> res;
      const auto & r = result(type, gt);
      for (Int k = 0; k < nf; ++k) {
        for (Int i = 0; i < nip; ++i) {
          for (Int c = 0; c < nbc; ++c) {
            res.push_back(r(flt(k) * nip + i, c));
          }
        }
      }
      w.add("elemental/" + g + "/result", sz({nf, nip, nbc}), {"entry", "point", "component"},
            "elemental", res)
          .element_type(et).ghost_type(g)
          .description("interpolateElementalFieldFromIntegrationPoints at the points of the "
                       "filtered elements, in filter order")
          .tolerance_class("kernel");
    }
    p["elemental_nb_interpolation_points"] = static_cast<long long>(nip);
  }
  w.finish();
  std::cout << "  fe/engine_extra/" << et << "\n";
  probe_real<type>(root, src);
}

} // namespace fe_extra
