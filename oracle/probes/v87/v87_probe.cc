// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle
// probe (not a port): adversarial cases for SMM-024 (upstream
// src/model/solid_mechanics/material.cc initElementalFieldInterpolation,
// interpolateStress, interpolateStressOnFacets), written
// after oracle/probes/smm2/smm2_probe.cc (same helpers, same field, same
// fixture format). Cases (case ids model/v87/<case>, committed under
// fixtures/model/v87):
//   interp_mixed_linear_2d     triangle_3 + quadrangle_4 in one mesh (shared
//                              segment_2 facets between the two types);
//                              material 1 holds only odd quadrangles (its
//                              triangle filter is empty), material 2 holds no
//                              element at all
//   interp_mixed_quadratic_2d  triangle_6 + quadrangle_8 (segment_3 facets),
//                              same selection
//   interp_block_3d            tetrahedron_4 patch mesh, material 1 = the
//                              first third of the element indices (one
//                              contiguous block, so interface facets between
//                              the two materials are a minority)
//   interp_reinit_2d           triangle_6 patch mesh: init + interpolate on
//                              the facet points, then a second
//                              initElementalFieldInterpolation with one
//                              point per element (the first quadrature
//                              point): records whether upstream throws
//
// Usage: v87_probe <fixtures_root> <upstream_src_root>

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <fe_engine.hh>
#include <integrator_gauss.hh>
#include <material.hh>
#include <mesh.hh>
#include <mesh_utils.hh>
#include <non_linear_solver.hh>
#include <shape_lagrange.hh>
#include <solid_mechanics_model.hh>

#include <Eigen/Core>

#include <algorithm>
#include <array>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iomanip>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "v87_probe";
constexpr const char * kProbeVersion = "1";
using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_work;

std::vector<double> vec_of(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

void add_ivec(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
              const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

fs::path write_tmp(const std::string & name, const std::string & text) {
  auto p = g_work / name;
  std::ofstream f(p, std::ios::binary | std::ios::trunc);
  f << text;
  return p;
}

/// smm2_probe's quadratic field.
Real field(Int s, Real x, Real y, Real z) {
  return (s + 1.) * (1. + 2. * x * x - 3. * x * y + 0.5 * y * y + z * (4. - x)) + 0.125 * s;
}

const char * kTwoDat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n"
                       "  nu = 0.3\n]\n"
                       "material elastic [\n  name = alu\n  rho = 2700\n  E = 7e10\n"
                       "  nu = 0.33\n]\n";

const char * kThreeDat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n"
                         "  nu = 0.3\n]\n"
                         "material elastic [\n  name = alu\n  rho = 2700\n  E = 7e10\n"
                         "  nu = 0.33\n]\n"
                         "material elastic [\n  name = unused\n  rho = 1000\n  E = 1e9\n"
                         "  nu = 0.25\n]\n";

/// Material 1 for odd quadrangles (linear or quadratic), 0 otherwise;
/// material 2 never selected.
class MixedSelector : public ConstitutiveLawSelector {
public:
  Idx operator()(const Element & element) override {
    const bool quad = element.type == _quadrangle_4 || element.type == _quadrangle_8;
    return (quad && element.element % 2 == 1) ? 1 : 0;
  }
};

/// Material 1 for the first third of the element indices.
class BlockSelector : public ConstitutiveLawSelector {
public:
  explicit BlockSelector(Int n) : n(n) {}
  Idx operator()(const Element & element) override {
    return 3 * element.element < n ? 1 : 0;
  }
  Int n;
};

/// 4 x 3 node grid on [0, 3] x [0, 2] (two interior nodes moved), 3 x 2
/// cells; the cells of column 0 are split into two triangles each, the
/// others are quadrangles. With `quadratic`, straight-edge mid-side nodes
/// (gmsh orders: triangle_6 = corners, m01, m12, m20; quadrangle_8 =
/// corners, m01, m12, m23, m30). No boundary elements.
std::string mixed_msh(bool quadratic) {
  std::vector<std::array<double, 2>> nodes;
  auto grid = [](int i, int j) { return j * 4 + i; };
  for (int j = 0; j < 3; ++j) {
    for (int i = 0; i < 4; ++i) {
      double x = i, y = j;
      if (i == 1 && j == 1) {
        x = 1.125;
        y = 0.9375;
      }
      if (i == 2 && j == 1) {
        x = 1.875;
        y = 1.0625;
      }
      nodes.push_back({x, y});
    }
  }
  std::map<std::pair<int, int>, int> mid;
  auto midpoint = [&](int a, int b) {
    auto key = std::make_pair(std::min(a, b), std::max(a, b));
    auto it = mid.find(key);
    if (it != mid.end()) {
      return it->second;
    }
    nodes.push_back({(nodes[a][0] + nodes[b][0]) / 2., (nodes[a][1] + nodes[b][1]) / 2.});
    int id = int(nodes.size()) - 1;
    mid[key] = id;
    return id;
  };
  std::vector<std::vector<int>> tris, quads;
  for (int j = 0; j < 2; ++j) {
    tris.push_back({grid(0, j), grid(1, j), grid(1, j + 1)});
    tris.push_back({grid(0, j), grid(1, j + 1), grid(0, j + 1)});
    for (int i = 1; i < 3; ++i) {
      quads.push_back({grid(i, j), grid(i + 1, j), grid(i + 1, j + 1), grid(i, j + 1)});
    }
  }
  if (quadratic) {
    for (auto & t : tris) {
      int a = t[0], b = t[1], c = t[2];
      t.push_back(midpoint(a, b));
      t.push_back(midpoint(b, c));
      t.push_back(midpoint(c, a));
    }
    for (auto & q : quads) {
      int a = q[0], b = q[1], c = q[2], d = q[3];
      q.push_back(midpoint(a, b));
      q.push_back(midpoint(b, c));
      q.push_back(midpoint(c, d));
      q.push_back(midpoint(d, a));
    }
  }
  std::ostringstream o;
  o << std::setprecision(17);
  o << "$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$Nodes\n" << nodes.size() << "\n";
  for (std::size_t n = 0; n < nodes.size(); ++n) {
    o << n + 1 << " " << nodes[n][0] << " " << nodes[n][1] << " 0\n";
  }
  o << "$EndNodes\n$Elements\n" << tris.size() + quads.size() << "\n";
  int e = 1;
  const int ttype = quadratic ? 9 : 2, qtype = quadratic ? 16 : 3;
  for (auto & t : tris) {
    o << e++ << " " << ttype << " 2 1 1";
    for (auto n : t) {
      o << " " << n + 1;
    }
    o << "\n";
  }
  for (auto & q : quads) {
    o << e++ << " " << qtype << " 2 1 1";
    for (auto n : q) {
      o << " " << n + 1;
    }
    o << "\n";
  }
  o << "$EndElements\n";
  return o.str();
}

enum class Selection { mixed, block };

/// One interpolation case (smm2_probe interp_case layout, extended to
/// several volume types sharing a facet type and to N materials).
void interp_case(const std::string & name, const fs::path & mesh_path, const std::string & source,
                 Int dim, Selection selection, const char * dat, bool reinit) {
  const std::string cid = "model/v87/interp_" + name;
  auto dp = write_tmp("v87_interp.dat", dat);
  getStaticParser().parse(dp.string());
  Mesh mesh(dim);
  mesh.read(mesh_path.string());
  Mesh & mesh_facets = mesh.initMeshFacets("mesh_facets");
  SolidMechanicsModel model(mesh);
  Int nb_elem_total = 0;
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    nb_elem_total += mesh.getNbElement(type);
  }
  if (selection == Selection::mixed) {
    model.setMaterialSelector(std::make_shared<MixedSelector>());
  } else {
    model.setMaterialSelector(std::make_shared<BlockSelector>(nb_elem_total));
  }
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());

  ap::FixtureWriter w(g_root, cid,
                      "V87 adversarial SMM-024 case " + name +
                          ": stresses set to smm2_probe's quadratic field, interpolateStress "
                          "at the facet quadrature points and interpolateStressOnFacets",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("v87");
  w.add_input_file("mesh", mesh_path, source);
  w.add_input_file("material", dp, "generated:v87_probe.cc");
  auto & p = w.parameters();
  p["eigen_cache_sizes"] = "setCpuCacheSizes(16384, 524288, 524288) (D-050)";
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["selection"] = selection == Selection::mixed ? "mixed" : "block";
  p["nb_materials"] = static_cast<long long>(model.getNbMaterials());

  using FacetFE = FEEngineTemplate<IntegratorGauss, ShapeLagrange>;
  model.registerFEEngineObject<FacetFE>("FacetsFEEngine", mesh_facets, dim - 1);
  auto & facets_fe = model.getFEEngine("FacetsFEEngine");
  facets_fe.initShapeFunctions();
  const auto & position = mesh.getNodes();

  Json types = Json::array();
  std::vector<ElementType> vtypes;
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    vtypes.push_back(type);
  }
  auto type_index = [&](ElementType t) -> I64 {
    auto it = std::find(vtypes.begin(), vtypes.end(), t);
    return it == vtypes.end() ? -1 : I64(it - vtypes.begin());
  };
  ElementTypeMapArray<Real> element_quad_facet("element_quad_facet");
  ElementTypeMapArray<Real> first_points("first_points");
  ElementTypeMapArray<Real> facet_stress("facet_stress");
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    const auto tname = std::to_string(type);
    types.push(tname);
    const auto type_facet = Mesh::getFacetType(type);
    const auto fname = std::to_string(type_facet);
    const Int nb_element = mesh.getNbElement(type);
    const Int nb_facet = mesh_facets.getNbElement(type_facet);
    const Int nq_facet = facets_fe.getNbIntegrationPoints(type_facet);
    Array<Real> quad_facets(nq_facet * nb_facet, dim);
    facets_fe.interpolateOnIntegrationPoints(position, quad_facets, dim, type_facet);
    const auto & f2e = mesh_facets.getSubelementToElement(type);
    const Int nfe = f2e.getNbComponent();
    auto & el_q = element_quad_facet.alloc(nb_element * nfe * nq_facet, dim, type, _not_ghost);
    std::vector<I64> s2e(nb_element * nfe);
    for (Int el = 0; el < nb_element; ++el) {
      for (Int f = 0; f < nfe; ++f) {
        const auto global_facet = f2e(el, f).element;
        s2e[el * nfe + f] = global_facet;
        for (Int q = 0; q < nq_facet; ++q) {
          for (Int s = 0; s < dim; ++s) {
            el_q(el * nfe * nq_facet + f * nq_facet + q, s) =
                quad_facets(global_facet * nq_facet + q, s);
          }
        }
      }
    }
    std::vector<I64> first(nb_facet), second(nb_facet);
    const auto & e2s = mesh_facets.getElementToSubelement(type_facet);
    for (Int fct = 0; fct < nb_facet; ++fct) {
      const auto & l = e2s(fct);
      first[fct] = l.empty() ? -1 : I64(l[0].element);
      second[fct] = l.size() < 2 || l[1] == ElementNull ? -1 : I64(l[1].element);
    }
    p["facet_type/" + tname] = fname;
    p["nb_quad_per_facet/" + tname] = static_cast<long long>(nq_facet);
    p["nb_facet_per_element/" + tname] = static_cast<long long>(nfe);
    add_array(w, "facet_points/" + tname, el_q, "facet quadrature points per element");
    add_ivec(w, "subelement_to_element/" + tname, s2e, "facet index of each element facet");
    if (not facet_stress.exists(type_facet, _not_ghost)) {
      std::vector<I64> first_type(nb_facet);
      for (Int fct = 0; fct < nb_facet; ++fct) {
        const auto & l = e2s(fct);
        first_type[fct] = l.empty() ? -1 : type_index(l[0].type);
      }
      add_ivec(w, "first_neighbour/" + fname, first, "element index of element_to_subelement[0]");
      add_ivec(w, "first_neighbour_type/" + fname, first_type,
               "position in the `types` list of element_to_subelement[0]'s type");
      add_ivec(w, "second_neighbour/" + fname, second,
               "element index of element_to_subelement[1] (-1: none)");
      facet_stress.alloc(nb_facet, 2 * dim * dim * nq_facet, type_facet, _not_ghost, 0.);
    }
    const Int nq = model.getFEEngine().getNbIntegrationPoints(type);
    Array<Real> quad_elements(nq * nb_element, dim);
    model.getFEEngine().interpolateOnIntegrationPoints(position, quad_elements, dim, type);
    add_array(w, "quad_points/" + tname, quad_elements, "quadrature points of every element");
    auto & fp = first_points.alloc(nb_element, dim, type, _not_ghost);
    for (Int el = 0; el < nb_element; ++el) {
      for (Int s = 0; s < dim; ++s) {
        fp(el, s) = quad_elements(el * nq, s);
      }
    }
    for (Int m = 0; m < Int(model.getNbMaterials()); ++m) {
      auto & mat = model.getMaterial(m);
      const auto & filter = static_cast<const Material &>(mat).getElementFilter(type);
      auto & stress = const_cast<Array<Real> &>(mat.getStress(type));
      for (Int l = 0; l < filter.size(); ++l) {
        const auto e = filter(l);
        for (Int q = 0; q < nq; ++q) {
          const Real x = quad_elements(e * nq + q, 0);
          const Real y = quad_elements(e * nq + q, 1);
          const Real z = dim == 3 ? quad_elements(e * nq + q, 2) : 0.;
          for (Int s = 0; s < dim * dim; ++s) {
            stress(l * nq + q, s) = field(s + 10 * m, x, y, z);
          }
        }
      }
      std::vector<I64> fv(filter.data(), filter.data() + filter.size());
      add_ivec(w, "filter/" + std::to_string(m) + "/" + tname, fv, "material element filter");
      add_array(w, "stress/" + std::to_string(m) + "/" + tname, stress,
                "material stress set to field(s + 10 m, x)");
    }
  }
  p["types"] = types;

  for (Int m = 0; m < Int(model.getNbMaterials()); ++m) {
    auto & mat = model.getMaterial(m);
    mat.initElementalFieldInterpolation(element_quad_facet);
    ElementTypeMapArray<Real> by_elem("by_elem");
    mat.interpolateStress(by_elem);
    for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
      add_array(w, "by_elem/" + std::to_string(m) + "/" + std::to_string(type), by_elem(type),
                "interpolateStress result (all elements, zero outside the filter)");
    }
    ElementTypeMapArray<Real> by_elem2("by_elem2");
    mat.interpolateStressOnFacets(facet_stress, by_elem2);
  }
  for (auto type : mesh_facets.elementTypes(dim - 1, _not_ghost, _ek_regular)) {
    if (facet_stress.exists(type, _not_ghost)) {
      add_array(w, "facet_stress/" + std::to_string(type), facet_stress(type),
                "interpolateStressOnFacets of every material in index order into one "
                "zero-initialised map");
    }
  }

  if (reinit) {
    // second initialisation of material 0 with one point per element
    auto & mat = model.getMaterial(0);
    std::string outcome = "ok";
    try {
      mat.initElementalFieldInterpolation(first_points);
      ElementTypeMapArray<Real> by_elem("by_elem_reinit");
      mat.interpolateStress(by_elem);
      for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
        add_array(w, "reinit/by_elem/" + std::to_string(type), by_elem(type),
                  "interpolateStress after the second initialisation");
      }
    } catch (debug::Exception & e) {
      outcome = std::string("threw: ") + e.what();
    } catch (std::exception & e) {
      outcome = std::string("threw (std): ") + e.what();
    }
    p["reinit_outcome"] = outcome;
    std::cout << "  reinit: " << outcome.substr(0, 200) << std::endl;
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  Eigen::setCpuCacheSizes(16384, 524288, 524288); // D-050
  if (argc < 3) {
    std::cerr << "usage: v87_probe <fixtures_root> <upstream_src_root>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  g_work = fs::temp_directory_path() / "v87_probe_work";
  fs::remove_all(g_work);
  fs::create_directories(g_work);
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  int rc = 0;
  const auto data = g_up / "test/test_model/patch_tests/data";
  auto run = [&](auto && f) {
    try {
      f();
    } catch (std::exception & e) {
      std::cerr << "v87_probe: " << e.what() << std::endl;
      rc = 1;
    }
  };
  run([&] {
    auto msh = write_tmp("mixed_linear.msh", mixed_msh(false));
    interp_case("mixed_linear_2d", msh, "generated:v87_probe.cc mixed_msh(false)", 2,
                Selection::mixed, kThreeDat, false);
  });
  run([&] {
    auto msh = write_tmp("mixed_quadratic.msh", mixed_msh(true));
    interp_case("mixed_quadratic_2d", msh, "generated:v87_probe.cc mixed_msh(true)", 2,
                Selection::mixed, kThreeDat, false);
  });
  run([&] {
    interp_case("block_3d", data / "_tetrahedron_4.msh",
                "upstream:test/test_model/patch_tests/data/_tetrahedron_4.msh", 3,
                Selection::block, kTwoDat, false);
  });
  run([&] {
    interp_case("reinit_2d", data / "_triangle_6.msh",
                "upstream:test/test_model/patch_tests/data/_triangle_6.msh", 2,
                Selection::block, kTwoDat, true);
  });
  fs::remove_all(g_work);
  finalize();
  return rc;
}
