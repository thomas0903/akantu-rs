// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs SolidMechanicsModel scenarios of the remaining
// solid-mechanics rows with the Eigen sparse solver (bit-reproducible by the
// port, D-008) and records them for the Rust differential tests
// (fixtures/model/smm2/*, crates/akantu-solid/tests/smm2_*.rs).
//
// Cases (fixture area model, case ids model/smm2/<case>, format v1):
//   interp_<mesh>      SMM-024: Material::initElementalFieldInterpolation /
//                      interpolateStress / interpolateStressOnFacets
//                      (material.cc:663-735) on the facet quadrature points
//                      (test_interpolate_stress.cc layout), two elastic
//                      materials (alternating by element index), stresses
//                      set to a quadratic field of the quadrature-point
//                      coordinates (not reproduced exactly by the fit)
//   dump_explicit_2d   MODEL-009 / SMM-017 / SMM-001: the model's ParaView
//   dump_static_3d     dumper (registered by the constructor) and extra mesh
//   dump_fd_2d         dumpers, every model field (solid_mechanics_model_io.cc
//                      createNodalFieldReal/Bool, createElementalField),
//                      dump(), dump(step), dump(time, step), dump(name, ...),
//                      setBaseName / setDirectory / setTimeStep; every file
//                      written is recorded byte for byte (inputs role
//                      "dump:<path relative to the dump directory>")
//   local_material     TEST-135 (test_local_material.cc): the upstream test's
//                      user law LocalMaterialDamage (compiled from the
//                      reference tree, unchanged) registered through
//                      MaterialFactory, the test's run (mesh_section_gap.msh,
//                      1100 explicit steps) and its two dumps
//   local_material_live TEST-135 variant:
//                      the same law and run, but a non-uniform initial
//                      displacement makes the first evaluation of the law
//                      non-zero (elastic, damaging and fully damaged points),
//                      so the law's stress arithmetic (incl. UD-684's
//                      mu eps instead of 2 mu eps) is observable; 5 steps
//   dump_padded_2d MODEL-009 / DUMP-029:
//                      padded 2-D "stress" (addDumpFieldTensorToDumper,
//                      StressPadder<2>) of the dump_explicit_2d scenario
//                      (plane strain steel, plane stress alu) in a base64
//                      dumper; the probe decodes the written array and
//                      records only the deterministic entries (in-plane, and
//                      sigma_zz of plane-strain elements, IOHelper's zero
//                      padding; the others are uninitialised memory
//                      upstream, DUMP-029, recorded as 0 with mask 0),
//                      plus every material's grad_u and
//                      stress (padded strain aborts upstream, UD-112)
//   dump_group_2d MODEL-009 group API
//                      (probe version 2): the dump_explicit_2d run with the
//                      model-level group dumper API (model.cc:100-144,
//                      255-339): setGroupDirectory (every group, then one),
//                      setGroupBaseName, addDumpGroupField[Vector] on
//                      boundary and volume groups, getGroupDumper()
//                      .setTimeStep, addDumpGroupField[Vector]ToDumper on a
//                      second (text) dumper of a group, dumpGroup(group),
//                      dumpGroup(group, dumper), dumpGroup() (every element
//                      group), interleaved with model.dump(); every file
//                      byte for byte. One group dump of "steel" right after
//                      a step and before model.dump() records UD-951 (stale
//                      flattened internals). removeDumpGroupField is not
//                      used: a later dump reads a destroyed field (UD-950)
//   event_order SMM-018 (D-077; probe version 2):
//                      the dispatch order of the model's
//                      EventHandlerManager with priorities and
//                      unregistration: user handler A, material a
//                      (registerNewMaterial), handler L (_ehp_lowest), B,
//                      material b, M (_ehp_model), C, then initFull
//                      instantiating material c from the parsed file; the
//                      five events and one model.dump(); then B and M
//                      unregistered, D registered at _ehp_lowest, BeforeDump
//                      and BeginningOfDamageIteration again; a second
//                      unregistration of B (throws). The materials are
//                      MaterialElastic<2> subclasses logging their callbacks
//                      (as the review's probe); the logs are manifest
//                      parameters
//   energy_explicit_3d SMM-010/011/012: getEnergy(id), getEnergy(id,
//   energy_static_3d   element), getEnergy(id, group), getKineticEnergy(type,
//   energy_periodic_2d index), unknown ids; static "external work" over two
//                      load steps (displacement increment and reactions);
//                      periodic slaves excluded (Mesh::makePeriodic)
//
// D-050: Eigen's cache sizes are pinned (setCpuCacheSizes(16384, 524288,
// 524288)) before anything else; recorded in the manifest parameters.
//
// Usage: smm2_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: SMM2_PROBE_CASES=<id>[,<id>...] restricts the run to the
// listed case ids (without the model/smm2/ prefix; unset: every case).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <element_group.hh>
#include <fe_engine.hh>
#include <integrator_gauss.hh>
#include <material.hh>
#include <material_elastic.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_utils.hh>
#include <non_linear_solver.hh>
#include <shape_lagrange.hh>
#include <solid_mechanics_model.hh>
// TEST-135 user law, compiled from the reference tree (CMakeLists.txt)
#include "local_material_damage.hh"
// dumper headers after mesh.hh (as oracle/probes/dump/dump_probe.cc)
#include <dumpable_inline_impl.hh>
#include <dumper_iohelper_paraview.hh>

// after the Akantu headers: they set Eigen's plugin macros first
#include <Eigen/Core>

#include <algorithm>
#include <array>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check
// (required by the fixture writer on x86_64 since then).
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "smm2_probe";
constexpr const char * kProbeVersion = "1";
// Case dump_group_2d (the version-1 cases are unchanged)
constexpr const char * kProbeVersion2 = "2";

using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_frozen, g_work;

bool wanted(const std::string & cid) {
  const char * env = std::getenv("SMM2_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  const std::string prefix = "model/smm2/";
  const auto id = cid.rfind(prefix, 0) == 0 ? cid.substr(prefix.size()) : cid;
  std::stringstream list(env);
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == id) {
      return true;
    }
  }
  return false;
}

/* -------------------------------------------------------------------------- */
std::vector<double> vec_of(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

void add_vec(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
             const std::string & desc) {
  w.add(name, {v.size()}, {"entry"}, "table", v).description(desc);
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

void common_parameters(ap::FixtureWriter & w) {
  auto & p = w.parameters();
  p["eigen_cache_sizes"] = "setCpuCacheSizes(16384, 524288, 524288) (D-050)";
  p["sparse_solver"] = "eigen";
}

/// Alternating material selector (element index parity), as the dump probe.
class AlternatingSelector : public ConstitutiveLawSelector {
public:
  Idx operator()(const Element & element) override { return element.element % 2; }
};

const char * kTwoDat = "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n"
                       "  nu = 0.3\n]\n"
                       "material elastic [\n  name = alu\n  rho = 2700\n  E = 7e10\n"
                       "  nu = 0.33\n]\n";

const char * kTwoDatPlaneStress =
    "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n]\n"
    "material elastic [\n  name = alu\n  rho = 2700\n  E = 7e10\n  nu = 0.33\n"
    "  Plane_Stress = true\n]\n";

const char * kTwoDatFd =
    "material elastic [\n  name = steel\n  rho = 7800\n  E = 2.1e11\n  nu = 0.3\n"
    "  finite_deformation = true\n]\n"
    "material elastic [\n  name = alu\n  rho = 2700\n  E = 7e10\n  nu = 0.33\n"
    "  finite_deformation = true\n]\n";

/// 3 x 2 quadrangle grid, x in [0, 3], y in [0, 1], interior and top/bottom
/// nodes moved off the regular grid (x = 0 and x = 3 columns match, so
/// makePeriodic(_x) pairs them); checkerboard steel/alu, boundary lines.
const char * kGrid2d = R"MSH($MeshFormat
2.2 0 8
$EndMeshFormat
$PhysicalNames
5
2 1 "steel"
2 2 "alu"
1 3 "left"
1 4 "bottom"
1 5 "top"
$EndPhysicalNames
$Nodes
12
1 0.0 0.0 0.0
2 0.9375 0.0 0.0
3 2.0625 0.0 0.0
4 3.0 0.0 0.0
5 0.0 0.5 0.0
6 1.0625 0.4375 0.0
7 1.9375 0.5625 0.0
8 3.0 0.5 0.0
9 0.0 1.0 0.0
10 1.0625 1.0 0.0
11 1.9375 1.0 0.0
12 3.0 1.0 0.0
$EndNodes
$Elements
14
1 1 2 3 3 1 5
2 1 2 3 3 5 9
3 1 2 4 4 1 2
4 1 2 4 4 2 3
5 1 2 4 4 3 4
6 1 2 5 5 9 10
7 1 2 5 5 10 11
8 1 2 5 5 11 12
9 3 2 1 1 1 2 6 5
10 3 2 2 2 2 3 7 6
11 3 2 1 1 3 4 8 7
12 3 2 2 2 5 6 10 9
13 3 2 1 1 6 7 11 10
14 3 2 2 2 7 8 12 11
$EndElements
)MSH";

/// The checkerboard steel/alu hex8 mesh of smm_probe's review cases (with
/// boundary quad4 groups left/right/top in the main mesh).
const char * kChecker3d = R"MSH($MeshFormat
2.2 0 8
$EndMeshFormat
$PhysicalNames
5
3 1 "steel"
3 2 "alu"
2 3 "left"
2 4 "right"
2 5 "top"
$EndPhysicalNames
$Nodes
36
1 0.0 0.0 0.0
2 0.9375 0.0 0.0
3 1.9375 0.0 0.0
4 3.0 0.0 0.0
5 0.0 0.5 0.0
6 1.0625 0.5 0.0
7 2.0625 0.5 0.0
8 3.0 0.5 0.0
9 0.0 1.0 0.0
10 0.9375 1.0 0.0
11 1.9375 1.0 0.0
12 3.0 1.0 0.0
13 0.0 0.0 0.5
14 1.0625 0.0 0.5
15 2.0625 0.0 0.5
16 3.0 0.0 0.5
17 0.0 0.5 0.5
18 1.125 0.4375 0.53125
19 2.125 0.4375 0.53125
20 3.0 0.5 0.5
21 0.0 1.0 0.5
22 1.0625 1.0 0.5
23 2.0625 1.0 0.5
24 3.0 1.0 0.5
25 0.0 0.0 1.0
26 0.9375 0.0 1.0
27 1.9375 0.0 1.0
28 3.0 0.0 1.0
29 0.0 0.5 1.0
30 1.0625 0.5 1.0
31 2.0625 0.5 1.0
32 3.0 0.5 1.0
33 0.0 1.0 1.0
34 0.9375 1.0 1.0
35 1.9375 1.0 1.0
36 3.0 1.0 1.0
$EndNodes
$Elements
26
1 5 2 1 1 1 2 6 5 13 14 18 17
2 5 2 2 2 2 3 7 6 14 15 19 18
3 5 2 1 1 3 4 8 7 15 16 20 19
4 5 2 2 2 5 6 10 9 17 18 22 21
5 5 2 1 1 6 7 11 10 18 19 23 22
6 5 2 2 2 7 8 12 11 19 20 24 23
7 5 2 2 2 13 14 18 17 25 26 30 29
8 5 2 1 1 14 15 19 18 26 27 31 30
9 5 2 2 2 15 16 20 19 27 28 32 31
10 5 2 1 1 17 18 22 21 29 30 34 33
11 5 2 2 2 18 19 23 22 30 31 35 34
12 5 2 1 1 19 20 24 23 31 32 36 35
13 3 2 3 3 1 13 17 5
14 3 2 4 4 4 8 20 16
15 3 2 3 3 5 17 21 9
16 3 2 4 4 8 12 24 20
17 3 2 3 3 13 25 29 17
18 3 2 4 4 16 20 32 28
19 3 2 3 3 17 29 33 21
20 3 2 4 4 20 24 36 32
21 3 2 5 5 25 26 30 29
22 3 2 5 5 26 27 31 30
23 3 2 5 5 27 28 32 31
24 3 2 5 5 29 30 34 33
25 3 2 5 5 30 31 35 34
26 3 2 5 5 31 32 36 35
$EndElements
)MSH";

/* -------------------------------------------------------------------------- */
/* SMM-024: stress interpolation                                              */
/* -------------------------------------------------------------------------- */
/// Quadratic stress field (component s, material offset folded into s).
Real field(Int s, Real x, Real y, Real z) {
  return (s + 1.) * (1. + 2. * x * x - 3. * x * y + 0.5 * y * y + z * (4. - x)) + 0.125 * s;
}

void interp_case(const std::string & name, const fs::path & mesh_path, const std::string & source,
                 Int dim, bool extra_build_all_facets) {
  const std::string cid = "model/smm2/interp_" + name;
  if (not wanted(cid)) {
    return;
  }
  auto dp = write_tmp("smm2_interp.dat", kTwoDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(dim);
  mesh.read(mesh_path.string());
  Mesh & mesh_facets = mesh.initMeshFacets("mesh_facets");
  if (extra_build_all_facets) {
    MeshUtils::buildAllFacets(mesh, mesh_facets); // test_interpolate_stress.cc
  }
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(std::make_shared<AlternatingSelector>());
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());

  ap::FixtureWriter w(g_root, cid,
                      "SMM-024 stress interpolation on " + name +
                          ": two elastic materials (alternating), quadratic stress field, "
                          "interpolateStress at the facet quadrature points (element, facet, "
                          "quadrature-point order) and interpolateStressOnFacets",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mesh_path, source);
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["extra_build_all_facets"] = extra_build_all_facets;

  using FacetFE = FEEngineTemplate<IntegratorGauss, ShapeLagrange>;
  model.registerFEEngineObject<FacetFE>("FacetsFEEngine", mesh_facets, dim - 1);
  auto & facets_fe = model.getFEEngine("FacetsFEEngine");
  facets_fe.initShapeFunctions();
  const auto & position = mesh.getNodes();

  Json types = Json::array();
  ElementTypeMapArray<Real> element_quad_facet("element_quad_facet");
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
    std::vector<I64> first(nb_facet);
    const auto & e2s = mesh_facets.getElementToSubelement(type_facet);
    for (Int fct = 0; fct < nb_facet; ++fct) {
      const auto & l = e2s(fct);
      first[fct] = l.empty() ? -1 : I64(l[0].element);
    }
    p["facet_type/" + tname] = fname;
    p["nb_quad_per_facet/" + tname] = static_cast<long long>(nq_facet);
    p["nb_facet_per_element/" + tname] = static_cast<long long>(nfe);
    add_array(w, "facet_points/" + tname, el_q,
              "interpolation points: facet quadrature points per element (element, facet, "
              "point)");
    add_ivec(w, "subelement_to_element/" + tname, s2e, "facet index of each element facet");
    add_ivec(w, "first_neighbour/" + fname, first,
             "element_to_subelement[0] element index of each facet");
    // element quadrature points (model FE engine), stresses set per material
    const Int nq = model.getFEEngine().getNbIntegrationPoints(type);
    Array<Real> quad_elements(nq * nb_element, dim);
    model.getFEEngine().interpolateOnIntegrationPoints(position, quad_elements, dim, type);
    add_array(w, "quad_points/" + tname, quad_elements, "quadrature points of every element");
    facet_stress.alloc(nb_facet, 2 * dim * dim * nq_facet, type_facet, _not_ghost, 0.);
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
      add_array(w, "by_elem/" + std::to_string(m) + "/" + std::to_string(type),
                by_elem(type), "interpolateStress result (all elements, zero outside the filter)");
    }
    ElementTypeMapArray<Real> by_elem2("by_elem2");
    mat.interpolateStressOnFacets(facet_stress, by_elem2);
  }
  for (auto type : mesh_facets.elementTypes(dim - 1, _not_ghost, _ek_regular)) {
    if (facet_stress.exists(type, _not_ghost)) {
      add_array(w, "facet_stress/" + std::to_string(type), facet_stress(type),
                "interpolateStressOnFacets of material 0 then 1 into one zero-initialised map "
                "(stress x 2 sides x facet point per facet)");
    }
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* dumps                                                                       */
/* -------------------------------------------------------------------------- */
void record_dump_tree(ap::FixtureWriter & w, const fs::path & dir) {
  std::vector<fs::path> files;
  for (auto & e : fs::recursive_directory_iterator(dir)) {
    if (e.is_regular_file()) {
      files.push_back(e.path());
    }
  }
  std::sort(files.begin(), files.end());
  Json list = Json::array();
  for (auto & f : files) {
    auto rel = fs::relative(f, dir).generic_string();
    list.push(rel);
    w.add_input_file("dump:" + rel, f, "oracle output");
  }
  w.parameters()["dump_files"] = list;
}

void velocity_y(SolidMechanicsModel & model) {
  const auto & X = model.getMesh().getNodes();
  auto & v = model.getVelocity();
  const auto dim = model.getSpatialDimension();
  for (Int n = 0; n < X.size(); ++n) {
    const Real y = X(n, 1);
    v(n, 0) = 0.01 * y + 0.002;
    v(n, 1) = -0.005 * y;
    if (dim == 3) {
      v(n, 2) = 0.003 * y - 0.001;
    }
  }
}

void linear_velocity(SolidMechanicsModel & model) {
  static const Real beta[3][3] = {{1e-2, -3e-3, 2e-3}, {4e-3, 5e-3, -1e-3}, {-2e-3, 1e-3, 6e-3}};
  auto dim = model.getSpatialDimension();
  const auto & X = model.getMesh().getNodes();
  auto & v = model.getVelocity();
  for (Int n = 0; n < X.size(); ++n) {
    for (Int i = 0; i < dim; ++i) {
      Real s = 0.;
      for (Int j = 0; j < dim; ++j) {
        s += beta[i][j] * X(n, j);
      }
      v(n, i) = s;
    }
  }
}

void dump_explicit_2d(const std::string & id, const char * dat, bool fd) {
  const std::string cid = "model/smm2/" + id;
  if (not wanted(cid)) {
    return;
  }
  auto mp = write_tmp("smm2_grid2d.msh", kGrid2d);
  auto dp = write_tmp("smm2_" + id + ".dat", dat);
  getStaticParser().parse(dp.string());
  Mesh mesh(2);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e6, -2e6}), "top");
  velocity_y(model);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  // the "mass" field can only be registered once the lumped mass exists
  // (otherwise the name falls through to createFieldFromAttachedData,
  // which throws): assemble it first
  model.assembleMassLumped();

  const auto out = g_work / id;
  fs::remove_all(out);
  fs::create_directories(out); // IOHelper creates only the last level
  const auto pv = (out / "paraview").string();
  model.setBaseName(id);
  mesh.setDirectory(pv);
  std::vector<std::string> fields;
  if (fd) {
    fields = {"displacement", "velocity", "stress", "grad_u", "Green strain",
              "Von Mises stress", "material_index"};
  } else {
    fields = {"displacement", "velocity", "acceleration", "external_force",
              "internal_force", "mass", "increment", "blocked_dofs",
              "material_index", "partitions", "stress", "grad_u",
              "strain", "Green strain", "principal strain", "principal Green strain",
              "Von Mises stress", "potential_energy"};
  }
  for (auto & f : fields) {
    model.addDumpField(f);
  }
  // a second ParaView dumper registered before setTimeStep (text mode)
  mesh.registerDumper<DumperParaview>("extra", id + "_extra");
  mesh.addDumpMeshToDumper("extra", mesh, 2, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("extra", pv);
  mesh.setTextModeToDumper("extra");
  for (std::string f : {"displacement", "stress", "grad_u", "Von Mises stress",
                        "external_force", "material_index", "blocked_dofs"}) {
    model.addDumpFieldToDumper("extra", f);
  }
  model.setTimeStep(dt);
  // a third one registered after setTimeStep (base64, padded vectors)
  mesh.registerDumper<DumperParaview>("padded", id + "_padded");
  mesh.addDumpMeshToDumper("padded", mesh, 2, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("padded", pv);
  model.addDumpFieldVectorToDumper("padded", "displacement");
  model.addDumpFieldVectorToDumper("padded", "velocity");
  model.addDumpFieldTensorToDumper("padded", "grad_u");

  ap::FixtureWriter w(g_root, cid,
                      std::string("SMM dump differential (") + (fd ? "finite deformation, " : "") +
                          "explicit, 2-D grid, steel/alu by physical names): the model dumper and "
                          "two extra dumpers, every file byte for byte",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["finite_deformation"] = fd;
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");

  model.getEnergy("potential"); // fills the potential_energy internal
  model.dump();                 // default: step 0
  model.dump("extra");
  for (Int k = 1; k <= 3; ++k) {
    model.solveStep();
    model.getEnergy("potential");
    if (k == 1) {
      model.dump();
      model.dump("extra");
      model.dump("padded");
    } else if (k == 2) {
      model.dump(5);
      model.dump("extra", 7);
    } else {
      model.dump(0.75, 9);
      model.dump("padded", 0.25, 2);
      model.dump("extra", 1.5, 3);
    }
  }
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "velocity", model.getVelocity(), "final velocity");
  record_dump_tree(w, out);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void dump_group_2d() {
  const std::string cid = "model/smm2/dump_group_2d";
  if (not wanted(cid)) {
    return;
  }
  auto mp = write_tmp("smm2_grid2d.msh", kGrid2d);
  auto dp = write_tmp("smm2_dump_group_2d.dat", kTwoDatPlaneStress);
  getStaticParser().parse(dp.string());
  Mesh mesh(2);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e6, -2e6}), "top");
  velocity_y(model);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  model.setTimeStep(dt); // the mesh's default dumper only

  const auto out = g_work / "dump_group_2d";
  fs::remove_all(out);
  fs::create_directories(out); // IOHelper creates only the last level
  const auto pv = (out / "paraview").string();
  // the model's default dumper (mesh only): model.dump() refreshes the
  // flattened material internals (SolidMechanicsModel::onDump)
  model.setBaseName("dump_group_2d_model");
  model.setDirectory((out / "model").string());
  model.setGroupDirectory(pv);                              // every element group
  model.setGroupDirectory((out / "top").string(), "top");   // then one group elsewhere
  model.setGroupBaseName("top_side", "top");
  model.addDumpGroupField("displacement", "top");
  model.addDumpGroupFieldVector("velocity", "top");
  model.addDumpGroupField("external_force", "top");
  model.addDumpGroupField("blocked_dofs", "left");
  model.addDumpGroupFieldVector("displacement", "left");
  model.addDumpGroupField("stress", "steel");
  model.addDumpGroupField("material_index", "steel");
  model.addDumpGroupFieldVector("velocity", "alu");
  model.addDumpGroupField("internal_force", "alu");
  model.getGroupDumper("top").setTimeStep(dt); // a .pvd for this group
  // a second (text) dumper of the group "bottom", registered as the
  // ElementGroup constructor registers its default one
  auto & bottom = mesh.getElementGroup("bottom");
  bottom.registerDumper<DumperParaview>("bottom_text", "bottom_text");
  bottom.addDumpFilteredMeshToDumper("bottom_text", mesh, bottom.getElements(),
                                     bottom.getNodeGroup().getNodes(), _all_dimensions);
  bottom.setDirectoryToDumper("bottom_text", pv);
  bottom.setTextModeToDumper("bottom_text");
  model.addDumpGroupFieldToDumper("bottom_text", "displacement", "bottom", _ek_regular, false);
  model.addDumpGroupFieldVectorToDumper("bottom_text", "velocity", "bottom");

  ap::FixtureWriter w(g_root, cid,
                      "SMM group dump differential (explicit, 2-D grid, steel/alu by physical "
                      "names): the model-level group dumper API, every file byte for byte",
                      kProbe, kProbeVersion2);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["ud951_stale_file"] = std::string("paraview/steel-VTU/steel_0001.vtu");
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");

  model.dump();
  model.dumpGroup("top");
  model.dumpGroup(); // every element group's default dumper
  model.dumpGroup("bottom", "bottom_text");
  for (Int k = 1; k <= 2; ++k) {
    model.solveStep();
    if (k == 1) {
      // UD-951: no model.dump() since the step: the flattened stress is
      // the one of the last model.dump() (step 0)
      model.dumpGroup("steel");
    }
    model.dump();
    model.dumpGroup("top");
    model.dumpGroup("left");
    model.dumpGroup("steel");
    model.dumpGroup("alu");
    model.dumpGroup("bottom", "bottom_text");
  }
  model.dump();
  model.dumpGroup();
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "velocity", model.getVelocity(), "final velocity");
  record_dump_tree(w, out);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* event dispatch order (SMM-018, D-077)                                       */
/* -------------------------------------------------------------------------- */
std::vector<std::string> g_events;

class EventRecorder : public SolidMechanicsModelEventHandler {
public:
  explicit EventRecorder(std::string tag) : tag(std::move(tag)) {}
  void onBeginningSolveStep(const AnalysisMethod & /*m*/) override {
    g_events.push_back(tag + ":beginning_solve_step");
  }
  void onEndSolveStep(const AnalysisMethod & /*m*/) override {
    g_events.push_back(tag + ":end_solve_step");
  }
  void onDump() override { g_events.push_back(tag + ":dump"); }
  void onDamageIteration() override { g_events.push_back(tag + ":damage_iteration"); }
  void onDamageUpdate() override { g_events.push_back(tag + ":damage"); }

private:
  std::string tag;
};

class LoggedElastic : public MaterialElastic<2> {
public:
  LoggedElastic(SolidMechanicsModel & model, const ID & id) : MaterialElastic<2>(model, id) {}
  void onDump() override {
    g_events.push_back("law " + this->getName() + ":dump");
    MaterialElastic<2>::onDump();
  }
  void onDamageIteration() override {
    g_events.push_back("law " + this->getName() + ":damage_iteration");
    MaterialElastic<2>::onDamageIteration();
  }
  void onDamageUpdate() override {
    g_events.push_back("law " + this->getName() + ":damage");
    MaterialElastic<2>::onDamageUpdate();
  }
};

[[maybe_unused]] const bool g_logged_elastic_registered =
    MaterialFactory::getInstance().registerAllocator(
        "logged_elastic",
        [](Int /*dim*/, const ID & /*opt*/, SolidMechanicsModel & model,
           const ID & id) -> std::unique_ptr<Material> {
          return std::make_unique<LoggedElastic>(model, id);
        });

const char * kEventOrderDat =
    "material logged_elastic [\n  name = c\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";

void event_order_case() {
  const std::string cid = "model/smm2/event_order";
  if (not wanted(cid)) {
    return;
  }
  auto dp = write_tmp("smm2_event_order.dat", kEventOrderDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(2);
  {
    MeshAccessor acc(mesh);
    auto & nodes = acc.getNodes();
    for (auto && p : std::vector<std::array<Real, 2>>{{0, 0}, {1, 0}, {1, 1}, {0, 1}}) {
      nodes.push_back(Vector<Real, 2>{p[0], p[1]});
    }
    auto & conn = acc.getConnectivity(_triangle_3);
    conn.push_back(Vector<Idx, 3>{0, 1, 2});
    conn.push_back(Vector<Idx, 3>{0, 2, 3});
    acc.makeReady();
  }
  SolidMechanicsModel model(mesh);
  EventRecorder A("A"), L("L"), B("B"), M("M"), C("C"), D("D");
  auto & ehm = static_cast<EventHandlerManager<SolidMechanicsModelEventHandler> &>(model);
  ehm.registerEventHandler(A);
  model.registerNewMaterial("a", "logged_elastic", "");
  ehm.registerEventHandler(L, _ehp_lowest);
  ehm.registerEventHandler(B);
  model.registerNewMaterial("b", "logged_elastic", "");
  ehm.registerEventHandler(M, _ehp_model);
  ehm.registerEventHandler(C);
  for (auto && name : {"a", "b"}) {
    auto & mat = model.getMaterial(name);
    mat.set("rho", 1.);
    mat.set("E", 1.);
    mat.set("nu", 0.3);
  }
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());

  ap::FixtureWriter w(g_root, cid,
                      "SolidMechanicsModel event dispatch order with priorities and "
                      "unregistration (EventHandlerManager, D-077)",
                      kProbe, kProbeVersion2);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  std::vector<std::string> names;
  for (Int i = 0; i < model.getNbMaterials(); ++i) {
    names.push_back(model.getMaterial(i).getName());
  }
  p["materials"] = Json::array_of(names);
  auto record = [&](const std::string & key) {
    p[key] = Json::array_of(g_events);
    g_events.clear();
  };
  g_events.clear();
  AnalysisMethod method = _explicit_lumped_mass; // the events take a non-const reference
  ehm.sendEvent(SolidMechanicsModelEvent::BeforeSolveStepEvent(method));
  record("events/1/beginning_solve_step");
  ehm.sendEvent(SolidMechanicsModelEvent::AfterSolveStepEvent(method));
  record("events/1/end_solve_step");
  ehm.sendEvent(SolidMechanicsModelEvent::BeforeDumpEvent());
  record("events/1/dump");
  ehm.sendEvent(SolidMechanicsModelEvent::BeginningOfDamageIterationEvent());
  record("events/1/damage_iteration");
  ehm.sendEvent(SolidMechanicsModelEvent::AfterDamageEvent());
  record("events/1/damage");
  const auto out = g_work / "event_order";
  fs::remove_all(out);
  fs::create_directories(out);
  model.setDirectory(out.string());
  model.dump();
  record("events/1/model_dump");

  ehm.unregisterEventHandler(B);
  ehm.unregisterEventHandler(M);
  ehm.registerEventHandler(D, _ehp_lowest);
  ehm.sendEvent(SolidMechanicsModelEvent::BeforeDumpEvent());
  record("events/2/dump");
  ehm.sendEvent(SolidMechanicsModelEvent::BeginningOfDamageIterationEvent());
  record("events/2/damage_iteration");
  bool threw = false;
  try {
    ehm.unregisterEventHandler(B);
  } catch (debug::Exception & /*e*/) {
    threw = true;
  }
  p["unregister_again_throws"] = threw;
  add_ivec(w, "nb_materials", {static_cast<I64>(model.getNbMaterials())},
           "number of materials (a, b, c)");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void dump_static_3d() {
  const std::string cid = "model/smm2/dump_static_3d";
  if (not wanted(cid)) {
    return;
  }
  auto mp = write_tmp("smm2_checker3d.msh", kChecker3d);
  auto dp = write_tmp("smm2_dump_static_3d.dat", kTwoDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(3);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 5);
  solver.set("threshold", 1e-10);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _z), "left");
  model.applyBC(BC::Dirichlet::FixedValue(1e-4, _x), "right");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e5, -2e5, -3e5}), "top");

  const auto out = g_work / "dump_static_3d";
  fs::remove_all(out);
  fs::create_directories(out); // IOHelper creates only the last level
  const auto pv = (out / "paraview").string();
  model.setBaseName("dump_static_3d");
  mesh.setDirectory(pv);
  for (std::string f :
       {"displacement", "external_force", "internal_force", "increment", "blocked_dofs",
        "material_index", "partitions", "stress", "grad_u", "strain", "Green strain",
        "principal strain", "principal Green strain", "Von Mises stress", "potential_energy"}) {
    model.addDumpField(f);
  }
  mesh.registerDumper<DumperParaview>("tensor", "dump_static_3d_tensor");
  mesh.addDumpMeshToDumper("tensor", mesh, 3, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("tensor", pv);
  mesh.setTextModeToDumper("tensor");
  model.addDumpFieldVectorToDumper("tensor", "displacement");
  model.addDumpFieldTensorToDumper("tensor", "stress");
  model.addDumpFieldTensorToDumper("tensor", "strain");
  model.addDumpFieldTensorToDumper("tensor", "Green strain");

  ap::FixtureWriter w(g_root, cid,
                      "SMM dump differential (static, checkerboard steel/alu hex8 mesh): the "
                      "model dumper before and after a static solve, a text-mode tensor dumper; "
                      "every file byte for byte",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  w.parameters()["spatial_dimension"] = 3LL;
  model.dump();
  model.solveStep();
  add_ivec(w, "newton/nb_iterations", {Int(solver.get("nb_iterations"))}, "its");
  model.getEnergy("potential");
  model.dump();
  model.dump("tensor");
  add_array(w, "displacement", model.getDisplacement(), "u");
  record_dump_tree(w, out);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* energies                                                                    */
/* -------------------------------------------------------------------------- */
std::vector<Element> volume_elements(const Mesh & mesh, Int dim) {
  std::vector<Element> v;
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    for (Int e = 0; e < mesh.getNbElement(type); ++e) {
      v.push_back(Element{type, e, _not_ghost});
    }
  }
  return v;
}

void energy_explicit_3d() {
  const std::string cid = "model/smm2/energy_explicit_3d";
  if (not wanted(cid)) {
    return;
  }
  auto mp = write_tmp("smm2_checker3d.msh", kChecker3d);
  auto dp = write_tmp("smm2_energy_explicit_3d.dat", kTwoDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(3);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _z), "left");
  Matrix<Real> sigma(3, 3);
  for (Int i = 0; i < 3; ++i) {
    for (Int j = 0; j < 3; ++j) {
      sigma(i, j) = 1e6 * (1 + i) + 3e5 * j - 2e5 * (i == j);
    }
  }
  model.applyBC(BC::Neumann::FromHigherDim(sigma), "right");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e5, -2e5, -3e5}), "top");
  linear_velocity(model);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  model.setTimeStep(dt);

  ap::FixtureWriter w(g_root, cid,
                      "SMM-010/011/012 energy accessors (explicit, checkerboard steel/alu hex8): "
                      "totals, per group, per element, unknown ids, every step",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 3LL;
  const Int steps = 6;
  p["steps"] = static_cast<long long>(steps);
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  std::vector<double> ek, ep, ew, ewn, eks, eka, ebogus, ebogus_g, u_all, v_all;
  auto snap = [&]() {
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    ew.push_back(model.getEnergy("external work"));
    ewn.push_back(model.getEnergy("external work new"));
    eks.push_back(model.getEnergy("kinetic", "steel"));
    eka.push_back(model.getEnergy("kinetic", "alu"));
    ebogus.push_back(model.getEnergy("bogus"));
    ebogus_g.push_back(model.getEnergy("bogus", "alu"));
    for (auto x : vec_of(model.getDisplacement())) u_all.push_back(x);
    for (auto x : vec_of(model.getVelocity())) v_all.push_back(x);
  };
  snap();
  for (Int s = 0; s < steps; ++s) {
    model.solveStep();
    snap();
  }
  add_vec(w, "energy/kinetic", ek, "getEnergy(\"kinetic\") per step (step 0 = initial)");
  add_vec(w, "energy/potential", ep, "getEnergy(\"potential\")");
  add_vec(w, "energy/external_work", ew, "getEnergy(\"external work\")");
  add_vec(w, "energy/external_work_new", ewn, "getEnergy(\"external work new\")");
  add_vec(w, "energy/kinetic_steel", eks, "getEnergy(\"kinetic\", \"steel\")");
  add_vec(w, "energy/kinetic_alu", eka, "getEnergy(\"kinetic\", \"alu\")");
  add_vec(w, "energy/bogus", ebogus, "getEnergy(\"bogus\") (unknown id: 0 per material)");
  add_vec(w, "energy/bogus_alu", ebogus_g, "getEnergy(\"bogus\", \"alu\")");
  add_vec(w, "displacement", u_all, "u per step");
  add_vec(w, "velocity", v_all, "v per step");
  std::vector<double> ek_el, ebogus_el;
  for (auto & el : volume_elements(mesh, 3)) {
    ek_el.push_back(model.getEnergy("kinetic", el));
    ebogus_el.push_back(model.getEnergy("bogus", el));
  }
  add_vec(w, "energy/kinetic_element", ek_el,
          "getEnergy(\"kinetic\", element) of every hex8 at the last step");
  add_vec(w, "energy/bogus_element", ebogus_el, "getEnergy(\"bogus\", element)");
  // getEnergy(id, type, index) is [[deprecated]] upstream (the Element
  // overload is the interface); called through the Element overload here
  add_vec(w, "energy/kinetic_type_index",
          {model.getEnergy("kinetic", Element{_hexahedron_8, 3, _not_ghost}),
           model.getEnergy("bogus", Element{_hexahedron_8, 7, _not_ghost})},
          "getEnergy(\"kinetic\", {_hexahedron_8, 3}), getEnergy(\"bogus\", {_hexahedron_8, 7})");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void energy_static_3d() {
  const std::string cid = "model/smm2/energy_static_3d";
  if (not wanted(cid)) {
    return;
  }
  auto mp = write_tmp("smm2_checker3d.msh", kChecker3d);
  auto dp = write_tmp("smm2_energy_static_3d.dat", kTwoDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(3);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _static, _solver_options = eigen_options());
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 5);
  solver.set("threshold", 1e-10);
  solver.set("convergence_type", SolveConvergenceCriteria::_solution);
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _z), "left");
  model.applyBC(BC::Dirichlet::FixedValue(1e-4, _x), "right");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e5, -2e5, -3e5}), "top");

  ap::FixtureWriter w(g_root, cid,
                      "SMM-011 static external work over two load steps (displacement increment "
                      "on free DOFs, reactions on blocked DOFs; checkerboard steel/alu hex8)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  w.parameters()["spatial_dimension"] = 3LL;
  std::vector<double> ew, ewn, ep, u_all, du_all, fi_all, fe_all;
  std::vector<I64> its;
  for (Int step = 0; step < 2; ++step) {
    if (step == 1) {
      model.applyBC(BC::Dirichlet::IncrementValue(1e-4, _x), "right");
      model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e5, -2e5, -3e5}), "top");
    }
    model.solveStep();
    its.push_back(Int(solver.get("nb_iterations")));
    ew.push_back(model.getEnergy("external work"));
    ewn.push_back(model.getEnergy("external work new"));
    ep.push_back(model.getEnergy("potential"));
    for (auto x : vec_of(model.getDisplacement())) u_all.push_back(x);
    for (auto x : vec_of(model.getIncrement())) du_all.push_back(x);
    for (auto x : vec_of(model.getInternalForce())) fi_all.push_back(x);
    for (auto x : vec_of(model.getExternalForce())) fe_all.push_back(x);
  }
  add_ivec(w, "newton/nb_iterations", its, "per load step");
  add_vec(w, "energy/external_work", ew, "getEnergy(\"external work\") after each load step");
  add_vec(w, "energy/external_work_new", ewn, "getEnergy(\"external work new\")");
  add_vec(w, "energy/potential", ep, "getEnergy(\"potential\")");
  add_vec(w, "displacement", u_all, "u after each load step");
  add_vec(w, "increment", du_all, "displacement increment after each load step");
  add_vec(w, "internal_force", fi_all, "f_int after each load step");
  add_vec(w, "external_force", fe_all, "f_ext after each load step");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

void energy_periodic_2d() {
  const std::string cid = "model/smm2/energy_periodic_2d";
  if (not wanted(cid)) {
    return;
  }
  auto mp = write_tmp("smm2_grid2d.msh", kGrid2d);
  auto dp = write_tmp("smm2_energy_periodic_2d.dat", kTwoDat);
  getStaticParser().parse(dp.string());
  Mesh mesh(2);
  mesh.read(mp.string());
  mesh.makePeriodic(_x);
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "bottom");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e6, -2e6}), "top");
  velocity_y(model);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  model.setTimeStep(dt);

  ap::FixtureWriter w(g_root, cid,
                      "SMM-010/011 periodic-slave exclusion: explicit on the 2-D grid made "
                      "periodic in x (makePeriodic(_x)), energies every step",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  const Int steps = 6;
  p["steps"] = static_cast<long long>(steps);
  std::vector<I64> slaves;
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    slaves.push_back(mesh.isPeriodicSlave(n) ? 1 : 0);
  }
  add_ivec(w, "periodic_slave", slaves, "Mesh::isPeriodicSlave per node");
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  model.assembleMassLumped();
  add_array(w, "mass", model.getMass(), "lumped mass (periodic-consistent)");
  std::vector<double> ek, ep, ew, ewn, eks, u_all, v_all;
  auto snap = [&]() {
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    ew.push_back(model.getEnergy("external work"));
    ewn.push_back(model.getEnergy("external work new"));
    eks.push_back(model.getEnergy("kinetic", "steel"));
    for (auto x : vec_of(model.getDisplacement())) u_all.push_back(x);
    for (auto x : vec_of(model.getVelocity())) v_all.push_back(x);
  };
  snap();
  for (Int s = 0; s < steps; ++s) {
    model.solveStep();
    snap();
  }
  add_vec(w, "energy/kinetic", ek, "getEnergy(\"kinetic\") per step");
  add_vec(w, "energy/potential", ep, "getEnergy(\"potential\")");
  add_vec(w, "energy/external_work", ew, "getEnergy(\"external work\")");
  add_vec(w, "energy/external_work_new", ewn, "getEnergy(\"external work new\")");
  add_vec(w, "energy/kinetic_steel", eks, "getEnergy(\"kinetic\", \"steel\")");
  add_vec(w, "displacement", u_all, "u per step");
  add_vec(w, "velocity", v_all, "v per step");
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* TEST-135: user-defined material through the factory                        */
/* -------------------------------------------------------------------------- */
/// MaterialFactory::registerAllocator("local_damage", ...) as
/// test_local_material.cc; once per process (the factory refuses a second
/// registration of the same id).
void register_local_damage() {
  static bool done = false;
  if (done) {
    return;
  }
  MaterialFactory::getInstance().registerAllocator(
      "local_damage",
      [](Int, const ID &, SolidMechanicsModel & model,
         const ID & id) -> std::unique_ptr<Material> {
        return std::make_unique<LocalMaterialDamage>(model, id);
      });
  done = true;
}

void local_material_case() {
  const std::string cid = "model/smm2/local_material";
  if (not wanted(cid)) {
    return;
  }
  const auto tdir = g_up / "test/test_model/test_solid_mechanics_model/test_materials";
  const auto mesh_path =
      g_frozen / "test/test_model/test_solid_mechanics_model/test_materials/mesh_section_gap.msh";
  getStaticParser().parse((tdir / "material.dat").string());
  const Int max_steps = 1100;
  const Int spatial_dimension = 2;
  Mesh mesh(spatial_dimension);
  mesh.read(mesh_path.string());
  register_local_damage();
  SolidMechanicsModel model(mesh);
  model.initFull();

  const auto out = g_work / "local_material";
  fs::remove_all(out);
  fs::create_directories(out);
  mesh.setDirectory((out / "paraview").string());
  model.addDumpField("damage");
  model.addDumpField("strain");
  model.addDumpField("stress");
  model.addDumpFieldVector("displacement");
  model.addDumpFieldVector("external_force");
  model.addDumpFieldVector("internal_force");
  model.dump();

  Real time_step = model.getStableTimeStep();
  model.setTimeStep(time_step / 2.5);
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "Fixed");
  Matrix<Real> stress = Matrix<Real, 2, 2>::Identity() * 5e7;
  model.applyBC(BC::Neumann::FromHigherDim(stress), "Traction");

  ap::FixtureWriter w(g_root, cid,
                      "TEST-135 test_local_material: the test's LocalMaterialDamage registered "
                      "through MaterialFactory, 1100 explicit steps, the test's dumps",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mesh_path,
                   "frozen:test/test_model/test_solid_mechanics_model/test_materials/"
                   "mesh_section_gap.msh");
  w.add_input_file("material", tdir / "material.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/test_materials/"
                   "material.dat");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["steps"] = static_cast<long long>(max_steps);
  add_vec(w, "time_step", {time_step, time_step / 2.5}, "getStableTimeStep(), time step used");
  auto & mat = dynamic_cast<LocalMaterialDamage &>(model.getMaterial("concrete"));
  p["law_type"] = "local_damage";
  const auto type = _triangle_6;
  for (Int s = 0; s < max_steps; ++s) {
    model.solveStep();
    if (s < 3) {
      const auto k = std::to_string(s + 1);
      add_array(w, "step_" + k + "/damage", mat.getDamage(type), "damage after the step");
      add_array(w, "step_" + k + "/stress", mat.getStress(type), "stress after the step");
      add_array(w, "step_" + k + "/displacement", model.getDisplacement(), "u");
    }
  }
  model.dump();
  add_array(w, "damage", mat.getDamage(type), "final damage");
  add_array(w, "stress", mat.getStress(type), "final stress");
  add_array(w, "displacement", model.getDisplacement(), "final u");
  add_array(w, "velocity", model.getVelocity(), "final v");
  record_dump_tree(w, out);
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* TEST-135 variant: live stresses in the user law */
/* -------------------------------------------------------------------------- */
void local_material_live_case() {
  const std::string cid = "model/smm2/local_material_live";
  if (not wanted(cid)) {
    return;
  }
  const auto tdir = g_up / "test/test_model/test_solid_mechanics_model/test_materials";
  const auto mesh_path =
      g_frozen / "test/test_model/test_solid_mechanics_model/test_materials/mesh_section_gap.msh";
  getStaticParser().parse((tdir / "material.dat").string());
  const Int max_steps = 5;
  const Int spatial_dimension = 2;
  Mesh mesh(spatial_dimension);
  mesh.read(mesh_path.string());
  register_local_damage();
  SolidMechanicsModel model(mesh);
  model.initFull();
  // initial displacement u_x = 8e-3 x^2 + 5e-3 y^2, u_y = -3e-3 x y: strains
  // 0 ... 1.6e-2 over x in [0, 1], i.e. Y below Yd (elastic), between Yd and
  // Yd + Sd (damaging) and above (fully damaged) at the first evaluation
  {
    const auto & X = mesh.getNodes();
    auto & u = model.getDisplacement();
    for (Int n = 0; n < X.size(); ++n) {
      const Real x = X(n, 0);
      const Real y = X(n, 1);
      u(n, 0) = 8e-3 * x * x + 5e-3 * y * y;
      u(n, 1) = -3e-3 * x * y;
    }
  }
  Real time_step = model.getStableTimeStep();
  model.setTimeStep(time_step / 2.5);
  model.applyBC(BC::Dirichlet::FixedValue(0.0, _x), "Fixed");
  Matrix<Real> stress = Matrix<Real, 2, 2>::Identity() * 5e7;
  model.applyBC(BC::Neumann::FromHigherDim(stress), "Traction");

  ap::FixtureWriter w(g_root, cid,
                      "TEST-135 variant: the test's LocalMaterialDamage (factory) with a "
                      "non-uniform initial displacement, so that its first evaluation gives "
                      "non-zero stresses; 5 explicit steps",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mesh_path,
                   "frozen:test/test_model/test_solid_mechanics_model/test_materials/"
                   "mesh_section_gap.msh");
  w.add_input_file("material", tdir / "material.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/test_materials/"
                   "material.dat");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["steps"] = static_cast<long long>(max_steps);
  p["initial_displacement"] = "u_x = 8e-3 x^2 + 5e-3 y^2, u_y = -3e-3 x y (before applyBC)";
  add_vec(w, "time_step", {time_step, time_step / 2.5}, "getStableTimeStep(), time step used");
  add_array(w, "initial_displacement", model.getDisplacement(), "u after applyBC, before step 1");
  auto & mat = dynamic_cast<LocalMaterialDamage &>(model.getMaterial("concrete"));
  p["law_type"] = "local_damage";
  const auto type = _triangle_6;
  for (Int s = 0; s < max_steps; ++s) {
    model.solveStep();
    const auto k = std::to_string(s + 1);
    add_array(w, "step_" + k + "/grad_u", mat.getGradU(type), "grad_u after the step");
    add_array(w, "step_" + k + "/damage", mat.getDamage(type), "damage after the step");
    add_array(w, "step_" + k + "/stress", mat.getStress(type), "stress after the step");
    add_array(w, "step_" + k + "/internal_force", model.getInternalForce(), "f_int");
    add_array(w, "step_" + k + "/displacement", model.getDisplacement(), "u");
    add_array(w, "step_" + k + "/velocity", model.getVelocity(), "v");
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/* -------------------------------------------------------------------------- */
/* padded 2-D stress through the model's createElementalField */
/* -------------------------------------------------------------------------- */
std::vector<unsigned char> base64_decode(const std::string & s) {
  auto val = [](char c) -> int {
    if (c >= 'A' and c <= 'Z') {
      return c - 'A';
    }
    if (c >= 'a' and c <= 'z') {
      return c - 'a' + 26;
    }
    if (c >= '0' and c <= '9') {
      return c - '0' + 52;
    }
    if (c == '+') {
      return 62;
    }
    if (c == '/') {
      return 63;
    }
    return -1; // '=', white space
  };
  std::vector<unsigned char> out;
  unsigned buf = 0;
  int bits = 0;
  for (char c : s) {
    const int v = val(c);
    if (v < 0) {
      continue;
    }
    buf = ((buf << 6) | static_cast<unsigned>(v)) & 0xFFFFFFu;
    bits += 6;
    if (bits >= 8) {
      bits -= 8;
      out.push_back(static_cast<unsigned char>((buf >> bits) & 0xFFu));
    }
  }
  return out;
}

/// The base64 Float64 DataArray `name` of a VTU file: (components, values).
std::pair<Int, std::vector<double>> read_vtu_array(const fs::path & file, const std::string & name) {
  std::ifstream f(file, std::ios::binary);
  std::stringstream ss;
  ss << f.rdbuf();
  const auto text = ss.str();
  const auto key = "Name=\"" + name + "\"";
  const auto at = text.find(key);
  if (at == std::string::npos) {
    throw std::runtime_error(file.string() + ": no DataArray " + name);
  }
  const auto tag_begin = text.rfind("<DataArray", at);
  const auto tag_end = text.find('>', at);
  const auto tag = text.substr(tag_begin, tag_end - tag_begin);
  if (tag.find("format=\"binary\"") == std::string::npos or
      tag.find("type=\"Float64\"") == std::string::npos) {
    throw std::runtime_error(file.string() + ": " + name + " is not a base64 Float64 array");
  }
  Int nc = 1;
  const auto nca = tag.find("NumberOfComponents=\"");
  if (nca != std::string::npos) {
    nc = std::stoi(tag.substr(nca + 20));
  }
  const auto body_end = text.find("</DataArray>", tag_end);
  const auto bytes = base64_decode(text.substr(tag_end + 1, body_end - tag_end - 1));
  if (bytes.size() < 4 or (bytes.size() - 4) % 8 != 0) {
    throw std::runtime_error(file.string() + ": " + name + ": malformed payload");
  }
  std::int32_t header = 0;
  std::memcpy(&header, bytes.data(), 4);
  if (static_cast<std::size_t>(header) != bytes.size()) { // IOHelper: payload + 4
    throw std::runtime_error(file.string() + ": " + name + ": unexpected header");
  }
  std::vector<double> v((bytes.size() - 4) / 8);
  std::memcpy(v.data(), bytes.data() + 4, v.size() * 8);
  return {nc, v};
}

void dump_padded_2d() {
  const std::string cid = "model/smm2/dump_padded_2d";
  if (not wanted(cid)) {
    return;
  }
  const std::string id = "dump_padded_2d";
  auto mp = write_tmp("smm2_grid2d.msh", kGrid2d);
  auto dp = write_tmp("smm2_" + id + ".dat", kTwoDatPlaneStress);
  getStaticParser().parse(dp.string());
  Mesh mesh(2);
  mesh.read(mp.string());
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model));
  model.initFull(_analysis_method = _explicit_lumped_mass, _solver_options = eigen_options());
  model.applyBC(BC::Dirichlet::FixedValue(0., _x), "left");
  model.applyBC(BC::Dirichlet::FixedValue(0., _y), "left");
  model.applyBC(BC::Neumann::FromSameDim(Vector<Real>{1e6, -2e6}), "top");
  velocity_y(model);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.5 * stable;
  model.setTimeStep(dt);
  for (Int k = 0; k < 2; ++k) {
    model.solveStep();
  }

  // the dump goes to a scratch directory: the written array holds
  // uninitialised entries (DUMP-029), only the decoded deterministic
  // entries are recorded
  const auto out = g_work / id;
  fs::remove_all(out);
  fs::create_directories(out);
  mesh.registerDumper<DumperParaview>("pstress", id + "_pstress");
  mesh.addDumpMeshToDumper("pstress", mesh, 2, _not_ghost, _ek_regular);
  mesh.setDirectoryToDumper("pstress", out.string());
  model.addDumpFieldTensorToDumper("pstress", "stress");
  model.dump("pstress");
  const auto vtu = out / (id + "_pstress-VTU") / (id + "_pstress_0000.vtu");
  auto [nc, values] = read_vtu_array(vtu, "stress");

  ap::FixtureWriter w(g_root, cid,
                      "padded 2-D stress (StressPadder<2>) of the dump_explicit_2d scenario: "
                      "deterministic entries of the dumped array, materials' grad_u and stress",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("smm2");
  w.add_input_file("mesh", mp, "generated:smm2_probe.cc");
  w.add_input_file("material", dp, "generated:smm2_probe.cc");
  common_parameters(w);
  auto & p = w.parameters();
  p["spatial_dimension"] = 2LL;
  p["steps"] = 2LL;
  p["padded_stress_components"] = static_cast<long long>(nc);
  add_vec(w, "time_step", {stable, dt}, "getStableTimeStep(), time step used");
  const auto type = _quadrangle_4;
  const auto nb_el = mesh.getNbElement(type, _not_ghost);
  // IOHelper writes each element's padded 3 x 3 tensor as a 9 x 9 block of
  // 9-component tuples (UD-110/113: tuple j holds column j of the tensor in
  // its first 3 components, the rest is IOHelper's zero padding)
  constexpr std::size_t kBlock = 81;
  if (nc != 9 or values.size() != static_cast<std::size_t>(nb_el) * kBlock) {
    throw std::runtime_error(cid + ": unexpected padded stress shape: " + std::to_string(nc) +
                             " components, " + std::to_string(values.size()) + " values, " +
                             std::to_string(nb_el) + " elements");
  }
  const auto & mat_idx = model.getMaterialByElement()(type, _not_ghost);
  std::vector<I64> mask(values.size(), 1);
  std::vector<I64> el_mat;
  for (Idx e = 0; e < nb_el; ++e) {
    const auto & m = model.getMaterial(mat_idx(e));
    const bool plane_stress = static_cast<bool>(m.getParam("Plane_Stress"));
    el_mat.push_back(mat_idx(e));
    // tensor entry (i, j) is at offset 9 j + i of the element's block:
    // StressPadder<2> never writes (2,0) (2,1) (0,2) (1,2), nor (2,2) for
    // plane stress (uninitialised Eigen storage); every other entry is the
    // in-plane stress, the plane-strain sigma_zz or IOHelper's zero padding
    for (std::size_t c : {2, 11, 18, 19}) {
      mask[e * kBlock + c] = 0;
    }
    if (plane_stress) {
      mask[e * kBlock + 20] = 0;
    }
  }
  for (std::size_t k = 0; k < values.size(); ++k) {
    if (mask[k] == 0) {
      values[k] = 0.;
    }
  }
  w.add("padded_stress/values", {static_cast<std::size_t>(nb_el) * 9, 9}, {"entry", "component"},
        "table", values)
      .description("decoded VTU array 'stress' (9 tuples of 9 components per element, "
                   "homogenised); entries with mask 0 are uninitialised upstream and recorded "
                   "as 0");
  w.add("padded_stress/mask", {static_cast<std::size_t>(nb_el) * 9, 9}, {"entry", "component"},
        "table", mask)
      .description("1: deterministic entry (in-plane, plane-strain sigma_zz, zero padding)");
  add_ivec(w, "material_by_element", el_mat, "getMaterialByElement()(_quadrangle_4)");
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & mat = model.getMaterial(m);
    const auto ms = std::to_string(m);
    p["material/" + ms + "/nu"] = Real(mat.getParam("nu"));
    p["material/" + ms + "/plane_stress"] = static_cast<bool>(mat.getParam("Plane_Stress"));
    const auto & filter = static_cast<const Material &>(mat).getElementFilter(type, _not_ghost);
    add_ivec(w, "material/" + ms + "/filter",
             std::vector<I64>(filter.data(), filter.data() + filter.size()),
             "element filter (_quadrangle_4)");
    add_array(w, "material/" + ms + "/grad_u", mat.getGradU(type), "grad_u (material order)");
    add_array(w, "material/" + ms + "/stress", mat.getStress(type), "stress (material order)");
  }
  w.finish();
  std::cout << "  " << cid << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  // D-050: host-independent Eigen blocking (Eigen's own defaults)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (argc < 4) {
    std::cerr << "usage: smm2_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>\n";
    return 2;
  }
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  g_work = fs::temp_directory_path() / "smm2_probe_work";
  fs::remove_all(g_work);
  fs::create_directories(g_work);
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    const auto data = g_up / "test/test_model/patch_tests/data";
    for (auto type : {_triangle_3, _triangle_6, _quadrangle_4, _quadrangle_8, _tetrahedron_4,
                      _tetrahedron_10}) {
      const auto tname = std::to_string(type);
      interp_case(tname.substr(1), data / (tname + ".msh"),
                  "upstream:test/test_model/patch_tests/data/" + tname + ".msh",
                  Mesh::getSpatialDimension(type), false);
    }
    const auto interp_mesh =
        g_frozen / "test/test_model/test_solid_mechanics_model/test_materials/interpolation.msh";
    interp_case("interpolation_msh", interp_mesh,
                "frozen:test/test_model/test_solid_mechanics_model/test_materials/"
                "interpolation.msh",
                3, false);
    dump_explicit_2d("dump_explicit_2d", kTwoDatPlaneStress, false);
    dump_explicit_2d("dump_fd_2d", kTwoDatFd, true);
    dump_group_2d();
    dump_static_3d();
    energy_explicit_3d();
    energy_static_3d();
    energy_periodic_2d();
    local_material_case();
    local_material_live_case();
    event_order_case();
    dump_padded_2d();
  } catch (std::exception & e) {
    std::cerr << "smm2_probe: " << e.what() << std::endl;
    return 1;
  }
  fs::remove_all(g_work);
  finalize();
  return 0;
}
