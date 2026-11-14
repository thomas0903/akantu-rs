// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs the upstream structural FE engine
// FEEngineTemplate<IntegratorGauss, ShapeStructural, _ek_structural> on
// small meshes and records the element-level structural kernels for the
// Rust differential tests (fixtures/fe/structural/*).
//
// Recorded per case (raw Akantu arrays: [entry, component] tables whose
// components are the column-major entries of the per-entry matrices):
//   integration_points, integration_weights   default-order Gauss rule
//   rotations          ShapeStructural::getRotations (computeRotationMatrix)
//   jacobians          IntegratorGauss<_ek_structural> det J * w
//   shapes             ShapeStructural N*T (not DKT: upstream computeShapes
//                      is empty and N is uninitialised, UD-053)
//   shapes_derivatives ShapeStructural B*T
//   field/interpolated, field/strain
//                      N(q) * u_e and B(q) * u_e with Tensor3 / dynamic
//                      views computed by the probe: upstream
//                      ShapeStructural::interpolateOnIntegrationPoints and
//                      gradientOnIntegrationPoints read the element values
//                      of element e*nq+q (view stride defect, out of bounds
//                      for the last elements), so the probe evaluates the
//                      intended product with the same operand types
//   tangent            computeTangentModuli<type> formulas (probe copy)
//   btdb               B.transpose() * D * B (assembleStiffnessMatrix<type>)
//   stiffness          FEEngine::integrate of btdb
//   stress             D * B * u (computeStressOnQuad<type>)
//   btd, internal_force_elemental
//                      FEEngine::computeBtD(stress) and its integral
//                      (assembleInternalForce, element part)
//   traction/local     R * T_global (computeForcesByGlobalTractionArray)
//   ntb, line_load     FEEngine::computeNtb(traction/local) and its
//                      integral (computeForcesByLocalTractionArray)
//   ntbn               probe-defined (ShapeStructural::computeNtbN is
//                      AKANTU_TO_IMPLEMENT): the local product of
//                      assembleFieldMatrix, (N^T diag b) N, on N*T
//   mass/shapes, mass/elemental
//                      computeShapesMassOnIntegrationPoints and the element
//                      part of assembleFieldMatrix (rho*A field, order
//                      2*degree, IntegratorGauss::integrate<type, order>)
//
// Usage: structural_probe <fixtures_root> <upstream_src_root>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <aka_math.hh>
#include <fe_engine.hh>
#include <fe_engine_template.hh>
#include <integrator_gauss.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <shape_structural.hh>

#include <cmath>
#include <cstdint>
#include <filesystem>
#include <iostream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
using Json = ap::Json;
using I64 = std::int64_t;

namespace {

const char * const kProbe = "oracle/probes/structural/structural_probe.cc";
const char * const kProbeVersion = "1";
std::filesystem::path g_root;
std::filesystem::path g_up;

using StructFE = FEEngineTemplate<IntegratorGauss, ShapeStructural, _ek_structural>;

template <class T> std::vector<T> vec_of(const Array<T> & a) {
  return std::vector<T>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

void add_matrix(ap::FixtureWriter & w, const std::string & name, const Matrix<Real> & m,
                const std::string & desc) {
  std::vector<double> v(m.data(), m.data() + m.size());
  w.add(name, {static_cast<std::size_t>(m.cols()), static_cast<std::size_t>(m.rows())},
        {"entry", "component"}, "table", v)
      .description(desc);
}

/// Material parameters of one element (StructuralMaterial subset).
struct Mat {
  Real E, A, I, Iz, Iy, GJ, rho, t, nu;
};

struct Case {
  std::string id;
  std::string description;
  ElementType type;
  Int dim;
  std::vector<std::vector<Real>> nodes;
  std::vector<std::vector<Idx>> conn;
  std::vector<std::vector<Real>> normals; // beam 3 only
  std::vector<Mat> mats;                  // one per element
  std::string source;                     // upstream mesh (provenance) or ""
};

/// Deterministic input values (no RNG dependency).
Real pseudo(Int i, Int j, Real scale) {
  return scale * std::sin(0.7 * Real(i) + 1.3 * Real(j) + 0.25);
}

template <ElementType type> void run_case(const Case & c) {
  constexpr Int ndof = ElementClass<type>::getNbDegreeOfFreedom();
  constexpr Int ns = ElementClass<type>::getNbStressComponents();
  constexpr Int nn = ElementClass<type>::getNbNodesPerElement();
  constexpr Int nc = ndof * nn;
  constexpr Int degree = ElementClassProperty<type>::polynomial_degree;
  constexpr bool has_shapes = type != _discrete_kirchhoff_triangle_18;
  const Int dim = c.dim;

  Mesh mesh(dim);
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  nodes.resize(c.nodes.size());
  for (std::size_t i = 0; i < c.nodes.size(); ++i) {
    for (Int d = 0; d < dim; ++d) {
      nodes(i, d) = c.nodes[i][d];
    }
  }
  mesh.addConnectivityType(type);
  auto & conn = acc.getConnectivity(type);
  conn.resize(c.conn.size());
  for (std::size_t e = 0; e < c.conn.size(); ++e) {
    for (Int k = 0; k < nn; ++k) {
      conn(e, k) = c.conn[e][k];
    }
  }
  acc.makeReady();
  const auto ne = mesh.getNbElement(type);
  if (not c.normals.empty()) {
    auto & normals = mesh.getDataPointer<Real>("extra_normal", type, _not_ghost, dim);
    for (Idx e = 0; e < ne; ++e) {
      for (Int d = 0; d < dim; ++d) {
        normals(e, d) = c.normals[e][d];
      }
    }
  }

  StructFE fem(mesh, dim, "structural_probe_fem");
  const auto & shape = fem.getShapeFunctions();
  const auto & integ = fem.getIntegrator();
  const auto nq = fem.getNbIntegrationPoints(type);

  ap::FixtureWriter w(g_root, c.id, c.description, kProbe, kProbeVersion);
  w.add_tag("fe");
  w.add_tag("structural");
  if (not c.source.empty()) {
    w.add_input_file("mesh", g_up / c.source, "upstream:" + c.source);
  }
  w.parameters()["spatial_dimension"] = static_cast<long long>(dim);
  w.parameters()["element_type"] = std::to_string(type);
  w.parameters()["nb_quadrature_points"] = static_cast<long long>(nq);

  // inputs
  add_array(w, "nodes", mesh.getNodes(), "input: node coordinates");
  {
    std::vector<I64> v;
    for (auto && row : c.conn) {
      for (auto n : row) {
        v.push_back(n);
      }
    }
    w.add("connectivity", {c.conn.size(), static_cast<std::size_t>(nn)}, {"element", "node_local"},
          "elemental", v)
        .element_type(std::to_string(type))
        .description("input: connectivity");
  }
  if (not c.normals.empty()) {
    add_array(w, "extra_normal", mesh.getData<Real>("extra_normal", type),
              "input: elemental data extra_normal");
  }
  {
    std::vector<double> m;
    for (auto && mt : c.mats) {
      m.insert(m.end(), {mt.E, mt.A, mt.I, mt.Iz, mt.Iy, mt.GJ, mt.rho, mt.t, mt.nu});
    }
    w.add("material", {c.mats.size(), 9}, {"element", "component"}, "elemental", m)
        .components({"E", "A", "I", "Iz", "Iy", "GJ", "rho", "t", "nu"})
        .description("input: StructuralMaterial per element");
  }

  // precomputed data
  add_matrix(w, "integration_points", fem.getIntegrationPoints(type),
             "natural coordinates of the default-order Gauss points (one entry per point)");
  {
    auto wts = integ.template getIntegrationWeights<type, degree>();
    w.add("integration_weights", {static_cast<std::size_t>(wts.size())}, {"entry"}, "table",
          std::vector<double>(wts.data(), wts.data() + wts.size()))
        .description("Gauss weights");
  }
  add_array(w, "rotations", shape.getRotations(type),
            "ShapeStructural rotation matrices (ndof x ndof per element, column-major)");
  add_array(w, "jacobians", integ.getJacobians(type), "det J * w per quadrature point");
  const auto & B = fem.getShapesDerivatives(type);
  add_array(w, "shapes_derivatives", B, "B*T per quadrature point (nb_stress x ndof*nn)");
  if (has_shapes) {
    add_array(w, "shapes", fem.getShapes(type), "N*T per quadrature point (ndof x ndof*nn)");
  }

  // nodal field and element values
  Array<Real> u(mesh.getNbNodes(), ndof);
  for (Idx i = 0; i < u.size(); ++i) {
    for (Int d = 0; d < ndof; ++d) {
      u(i, d) = pseudo(i, d, 0.01 * (d + 1));
    }
  }
  add_array(w, "field/nodal", u, "input: nodal displacement-rotation field");
  Array<Real> u_el(0, nc);
  FEEngine::extractNodalToElementField(mesh, u, u_el, type);

  // N(q) * u_e and B(q) * u_e (intended semantics, Tensor3 / dynamic views)
  auto apply = [&](const Array<Real> & M, Int rows, const std::string & name,
                   const std::string & desc) {
    Array<Real> out(ne * nq, rows);
    auto M_it = make_view(M, rows, nc, nq).begin();
    auto u_it = make_view(u_el, nc, 1).begin();
    auto o_it = make_view(out, rows, 1, nq).begin();
    for (Idx e = 0; e < ne; ++e, ++M_it, ++u_it, ++o_it) {
      auto && Mq = *M_it;
      auto && ue = *u_it;
      auto && oq = *o_it;
      for (Int q = 0; q < nq; ++q) {
        oq(q) = Mq(q) * ue;
      }
    }
    add_array(w, name, out, desc);
  };
  apply(B, ns, "field/strain", "B(q) * u_e (probe, dynamic Tensor3 slices)");
  if (has_shapes) {
    apply(fem.getShapes(type), ndof, "field/interpolated",
          "N(q) * u_e (probe, dynamic Tensor3 slices)");
  }

  // tangent moduli (computeTangentModuli<type> formulas, upstream variant)
  Array<Real> tangent(ne * nq, ns * ns);
  tangent.zero();
  {
    auto D_it = tangent.begin(ns, ns);
    for (Idx e = 0; e < ne; ++e) {
      const auto & m = c.mats[e];
      for (Int q = 0; q < nq; ++q, ++D_it) {
        auto & D = *D_it;
        if constexpr (type == _bernoulli_beam_2) {
          D(0, 0) = m.E * m.A;
          D(1, 1) = m.E * m.I;
        } else if constexpr (type == _bernoulli_beam_3) {
          D(0, 0) = m.E * m.A;
          D(1, 1) = m.E * m.Iz;
          D(2, 2) = m.E * m.Iy;
          D(3, 3) = m.GJ;
        } else {
          Matrix<Real, 3, 3> Dm{{1., m.nu, 0.}, {m.nu, 1., 0.}, {0., 0., (1. - m.nu) / 2.}};
          Dm *= m.E * m.t / (1. - m.nu * m.nu);
          D.zero();
          D.template block<3, 3>(0, 0) = Dm;
          D.template block<3, 3>(3, 3) = Dm * Math::pow<3>(m.t) / 12.;
        }
      }
    }
  }
  add_array(w, "tangent", tangent,
            "computeTangentModuli<type> (upstream formulas; DKT bending block D t^3/12, UD-007)");

  // B^T D B and its integral (assembleStiffnessMatrix<type>)
  Array<Real> btdb(ne * nq, nc * nc);
  for (auto && data :
       zip(make_view(B, ns, nc), make_view(tangent, ns, ns), make_view(btdb, nc, nc))) {
    auto & Bq = std::get<0>(data);
    auto & D = std::get<1>(data);
    auto & BtDB = std::get<2>(data);
    BtDB = Bq.transpose() * D * Bq;
  }
  add_array(w, "btdb", btdb, "B.transpose() * D * B per quadrature point");
  Array<Real> stiffness(ne, nc * nc);
  fem.integrate(btdb, stiffness, nc * nc, type);
  add_array(w, "stiffness", stiffness, "element stiffness: FEEngine::integrate(btdb)");

  // stresses, B^T sigma, internal force (computeStressOnQuad, assembleInternalForce)
  Array<Real> sigma(ne * nq, ns);
  for (auto && data : zip(make_view(tangent, ns, ns), make_view(B, ns, nc),
                          repeat_n(make_view(u_el, nc), nq), make_view(sigma, ns))) {
    auto && D = std::get<0>(data);
    auto && Bq = std::get<1>(data);
    auto && ue = std::get<2>(data);
    auto && DBu = std::get<3>(data);
    DBu = D * Bq * ue;
  }
  add_array(w, "stress", sigma, "D * B * u per quadrature point");
  Array<Real> btd(ne * nq, nc);
  fem.computeBtD(sigma, btd, type, _not_ghost, empty_filter);
  add_array(w, "btd", btd, "FEEngine::computeBtD(stress)");
  Array<Real> fint(0, nc);
  fem.integrate(btd, fint, nc, type);
  add_array(w, "internal_force_elemental", fint, "FEEngine::integrate(btd)");

  // tractions (computeForcesByGlobalTractionArray / ...LocalTractionArray)
  Array<Real> t_global(ne * nq, ndof);
  for (Idx i = 0; i < t_global.size(); ++i) {
    for (Int d = 0; d < ndof; ++d) {
      t_global(i, d) = pseudo(i + 3, d + 5, 10. * (d + 1));
    }
  }
  add_array(w, "traction/global", t_global, "input: global tractions per quadrature point");
  Array<Real> t_local(ne * nq, ndof);
  {
    auto R_it = shape.getRotations(type).begin(ndof, ndof);
    auto Te_it = t_global.begin(ndof);
    auto te_it = t_local.begin(ndof);
    for (Idx e = 0; e < ne; ++e, ++R_it) {
      for (Int q = 0; q < nq; ++q, ++Te_it, ++te_it) {
        *te_it = *R_it * *Te_it;
      }
    }
  }
  add_array(w, "traction/local", t_local, "R * T_global per quadrature point");
  if (has_shapes) {
    Array<Real> ntb(ne * nq, nc);
    fem.computeNtb(t_local, ntb, type, _not_ghost, empty_filter);
    add_array(w, "ntb", ntb, "FEEngine::computeNtb(traction/local)");
    Array<Real> load(ne, nc);
    fem.integrate(ntb, load, nc, type);
    add_array(w, "line_load", load, "FEEngine::integrate(ntb)");

    // (N^T diag b) N on N*T (probe-defined, assembleFieldMatrix local product)
    Array<Real> bw(ne * nq, ndof);
    for (Idx i = 0; i < bw.size(); ++i) {
      for (Int d = 0; d < ndof; ++d) {
        bw(i, d) = 1. + 0.5 * std::cos(Real(i) + 0.3 * d);
      }
    }
    add_array(w, "ntbn/b", bw, "input: weights b per quadrature point");
    Array<Real> ntbn(ne * nq, nc * nc);
    for (auto && [mat, N, rho] : zip(make_view(ntbn, nc, nc), make_view(fem.getShapes(type), ndof, nc),
                                     make_view(bw, ndof))) {
      Matrix<Real> Nt = N.transpose();
      for (Int d = 0; d < Nt.cols(); ++d) {
        Nt(d) *= rho(d);
      }
      mat = Nt * N;
    }
    add_array(w, "ntbn", ntbn, "(N^T diag b) N on N*T (assembleFieldMatrix local product)");

    // consistent mass (assembleFieldMatrix with ShapesForMassHelper<_ek_structural>)
    constexpr Int mass_degree = 2 * degree;
    auto mpoints = integ.template getIntegrationPoints<type, mass_degree>();
    const Int nqm = mpoints.cols();
    Array<Real> mshapes(0, ns * nc);
    shape.template computeShapesMassOnIntegrationPoints<type>(mesh.getNodes(), mpoints, mshapes,
                                                              _not_ghost);
    add_matrix(w, "mass/integration_points", mpoints, "Gauss points of order 2 * degree");
    add_array(w, "mass/shapes", mshapes, "computeShapesMassOnIntegrationPoints (nb_stress rows)");
    Array<Real> field(ne * nqm, ns);
    for (Idx e = 0; e < ne; ++e) {
      for (Int q = 0; q < nqm; ++q) {
        for (Int d = 0; d < ns; ++d) {
          field(e * nqm + q, d) = c.mats[e].rho * c.mats[e].A;
        }
      }
    }
    add_array(w, "mass/field", field, "input: rho * A per mass quadrature point");
    Array<Real> local(ne * nqm, nc * nc);
    for (auto && [mat, N, rho] :
         zip(make_view(local, nc, nc), make_view(mshapes, ns, nc), make_view(field, ns))) {
      Matrix<Real> Nt = N.transpose();
      for (Int d = 0; d < Nt.cols(); ++d) {
        Nt(d) *= rho(d);
      }
      mat = Nt * N;
    }
    Array<Real> mass(ne, nc * nc);
    integ.template integrate<type, mass_degree>(local, mass, nc * nc, _not_ghost);
    add_array(w, "mass/elemental", mass, "element consistent mass");
  }
  w.finish();
  std::cout << "  " << c.id << std::endl;
}

void run(const Case & c) {
  switch (c.type) {
  case _bernoulli_beam_2:
    run_case<_bernoulli_beam_2>(c);
    break;
  case _bernoulli_beam_3:
    run_case<_bernoulli_beam_3>(c);
    break;
  case _discrete_kirchhoff_triangle_18:
    run_case<_discrete_kirchhoff_triangle_18>(c);
    break;
  default:
    throw std::runtime_error("unsupported type");
  }
}

Mat beam_mat(Real E, Real A, Real I, Real rho) { return {E, A, I, I, 0.7 * I, 0.4 * E * I, rho, 0., 0.}; }

std::vector<Real> unit_normal(const std::vector<Real> & a, const std::vector<Real> & b,
                              const std::vector<Real> & helper) {
  // n = normalize((b - a) x helper), orthogonal to the beam axis
  Real x[3] = {b[0] - a[0], b[1] - a[1], b[2] - a[2]};
  Real n[3] = {x[1] * helper[2] - x[2] * helper[1], x[2] * helper[0] - x[0] * helper[2],
               x[0] * helper[1] - x[1] * helper[0]};
  Real l = std::sqrt(n[0] * n[0] + n[1] * n[1] + n[2] * n[2]);
  return {n[0] / l, n[1] / l, n[2] / l};
}

std::vector<Case> cases() {
  std::vector<Case> cs;
  // beam 2: test_fe_engine mesh (inclined +-atan(4/3))
  cs.push_back({"fe/structural/bernoulli_beam_2_fe_engine",
                "_bernoulli_beam_2 on test/test_fe_engine/_bernoulli_beam_2.msh",
                _bernoulli_beam_2, 2, {{0, 0}, {0.6, 0.8}, {1.2, 0}}, {{0, 1}, {1, 2}}, {},
                {beam_mat(3e10, 0.01, 0.0025, 2500.), beam_mat(2.1e11, 0.02, 0.00128, 7800.)},
                "test/test_fe_engine/_bernoulli_beam_2.msh"});
  // beam 2: model test mesh (TestStructBernoulli2 materials)
  cs.push_back({"fe/structural/bernoulli_beam_2_model",
                "_bernoulli_beam_2 on test/test_model/test_structural_mechanics_model/_bernoulli_beam_2.msh",
                _bernoulli_beam_2, 2, {{0, 0}, {10, 0}, {18, 0}}, {{0, 1}, {1, 2}}, {},
                {beam_mat(3e10, 0.01, 0.0025, 1.), beam_mat(3e10, 0.01, 0.00128, 1.)},
                "test/test_model/test_structural_mechanics_model/_bernoulli_beam_2.msh"});
  // beam 2: generated, irregular lengths and orientations (incl. reversed)
  {
    Case c{"fe/structural/bernoulli_beam_2_inclined",
           "_bernoulli_beam_2, generated chain with irregular inclined elements",
           _bernoulli_beam_2, 2,
           {{0.1, -0.3}, {1.37, 0.41}, {0.93, 2.2}, {-1.15, 1.7}, {-0.4, -2.05}, {3.3, -1.1}},
           {{0, 1}, {1, 2}, {3, 2}, {3, 4}, {4, 5}, {5, 0}}, {}, {}, ""};
    for (Int e = 0; e < 6; ++e) {
      c.mats.push_back(beam_mat(1e7 * (1 + e), 0.3 + 0.1 * e, 0.01 * (e + 2), 3. + e));
    }
    cs.push_back(c);
  }
  // beam 3: TestBernoulliB3 (fe engine) mesh and normal
  cs.push_back({"fe/structural/bernoulli_beam_3_fe_engine",
                "_bernoulli_beam_3 on test/test_fe_engine/_bernoulli_beam_3.msh, extra_normal "
                "(-36/65, -48/65, 5/13)",
                _bernoulli_beam_3, 3, {{0, 0, 0}, {3, 4, 12}}, {{0, 1}},
                {{-36. / 65, -48. / 65, 5. / 13}}, {{1., 1., 1., 1., 1., 1., 1., 0., 0.}},
                "test/test_fe_engine/_bernoulli_beam_3.msh"});
  // beam 3: model test mesh, normals z
  cs.push_back({"fe/structural/bernoulli_beam_3_model",
                "_bernoulli_beam_3 on test/test_model/test_structural_mechanics_model/_bernoulli_beam_3.msh, normals (0, 0, 1)",
                _bernoulli_beam_3, 3,
                {{0, 0, 0}, {0.7071067811865476, 0.7071067811865476, 0}, {0.7071067811865476, -0.7071067811865476, 0}},
                {{0, 1}, {0, 2}}, {{0, 0, 1}, {0, 0, 1}},
                {{1., 1., 1., 1., 1., 1., 1., 0., 0.}, {1., 1., 1., 1., 1., 1., 1., 0., 0.}},
                "test/test_model/test_structural_mechanics_model/_bernoulli_beam_3.msh"});
  // beam 3: generated 3-D oriented beams, normals orthogonal to the axes
  {
    Case c{"fe/structural/bernoulli_beam_3_oriented",
           "_bernoulli_beam_3, generated 3-D oriented beams with orthogonal unit normals",
           _bernoulli_beam_3, 3,
           {{0.2, -0.1, 0.3}, {1.4, 0.7, -0.5}, {0.6, 2.1, 1.3}, {-1.2, 0.4, 2.2}, {0.9, -1.6, -0.8}},
           {{0, 1}, {1, 2}, {2, 3}, {3, 0}, {4, 2}}, {}, {}, ""};
    const std::vector<std::vector<Real>> helpers = {
        {0, 0, 1}, {1, 0, 0}, {0.3, -0.8, 0.5}, {0, 1, 0}, {-0.6, 0.2, 0.7}};
    for (Int e = 0; e < 5; ++e) {
      c.normals.push_back(unit_normal(c.nodes[c.conn[e][0]], c.nodes[c.conn[e][1]], helpers[e]));
      c.mats.push_back({2e5 * (1 + e), 0.5 + 0.1 * e, 0., 0.02 * (e + 1), 0.03 * (e + 2),
                        700. * (e + 1), 7.8 + e, 0., 0.});
    }
    cs.push_back(c);
  }
  // DKT: model test mesh (Batoz patch, z = 0)
  {
    Case c{"fe/structural/discrete_kirchhoff_triangle_18_model",
           "_discrete_kirchhoff_triangle_18 on test/test_model/test_structural_mechanics_model/"
           "_discrete_kirchhoff_triangle_18.msh (E = 1, t = 1, nu = 0.3)",
           _discrete_kirchhoff_triangle_18, 3,
           {{0, 0, 0}, {40, 0, 0}, {40, 20, 0}, {0, 20, 0}, {15, 15, 0}},
           {{0, 4, 1}, {1, 4, 2}, {2, 3, 4}, {0, 3, 4}}, {}, {},
           "test/test_model/test_structural_mechanics_model/_discrete_kirchhoff_triangle_18.msh"};
    for (Int e = 0; e < 4; ++e) {
      c.mats.push_back({1., 0., 0., 0., 0., 0., 0., 1., 0.3});
    }
    cs.push_back(c);
  }
  // DKT: generated distorted triangles in tilted / warped 3-D positions, t != 1
  {
    Case c{"fe/structural/discrete_kirchhoff_triangle_18_distorted",
           "_discrete_kirchhoff_triangle_18, generated distorted triangles in 3-D (t != 1)",
           _discrete_kirchhoff_triangle_18, 3,
           {{0.1, 0.05, 0.2}, {1.3, -0.2, 0.45}, {0.4, 0.9, -0.3}, {1.6, 1.1, 0.8}, {-0.7, 0.6, 0.1},
            {0.9, 2.0, 0.35}},
           {{0, 1, 2}, {1, 3, 2}, {4, 0, 2}, {2, 3, 5}, {4, 2, 5}}, {}, {}, ""};
    for (Int e = 0; e < 5; ++e) {
      c.mats.push_back({2.1e3 * (1 + e), 0., 0., 0., 0., 0., 0., 0.05 + 0.03 * e, 0.2 + 0.05 * e});
    }
    cs.push_back(c);
  }
  return cs;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 3) {
    std::cerr << "usage: structural_probe <fixtures_root> <upstream_src_root>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    for (auto && c : cases()) {
      run(c);
    }
  } catch (std::exception & e) {
    std::cerr << "structural_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
