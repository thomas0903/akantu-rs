// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): runs SolidMechanicsModelCohesive scenarios and records
// the model state for the Rust differential tests (fixtures/model/cohesive/*).
//
// Cases (fixture area model, case ids model/cohesive/<case>, format v1):
//   fixture_<extrinsic|intrinsic>_mode_<i|ii>_<types>
//       TestSMMCFixture (test/test_model/test_solid_mechanics_model/
//       test_cohesive/test_cohesive_fixture.hh, test_cohesive.cc) on the
//       frozen meshes, serial (the gtest calls mesh.distribute(), which on
//       one process only marks the mesh distributed; the probe runs the
//       plain serial path the Rust model implements). Per step: number of
//       cohesive elements after checkCohesiveStress, energies (kinetic,
//       potential, dissipated, reversible, cohesive contact) after the
//       solve; SHA-256 of displacement || velocity || acceleration every
//       25 steps and at the last; at the
//       end: nodal state, internal force, lumped mass, mesh and facet mesh
//       topology (connectivities, element_to_subelement,
//       subelement_to_element), cohesive internals, facet_material.
//   implicit_static_<law>_triangle_6
//       probe-defined from test_cohesive_intrinsic_impl (legacy, never built
//       upstream): implicit.msh, intrinsic insertion in the band
//       0.9 < y < 1.1 (inserter bounding_box), static Newton-Raphson with the
//       Eigen sparse solver (max_iterations 100, threshold 1e-5, residual),
//       200 load increments of the top displacement; per step iterations,
//       error, top reaction, opening of quadrature point 0, state digest;
//       final displacement, internal force, K (COO, storage order) and the
//       cohesive internals. An upstream exception is recorded as
//       `upstream_error` with the steps completed before it.
//   buildfragments
//       test_cohesive_buildfragments (Math::setTolerance(1e-14), restored
//       afterwards; seed 1 as its material.dat): 200 explicit steps with
//       checkCohesiveStress and FragmentManager::computeAllData every step;
//       per step inserted elements, number of fragments, total mass; final
//       fragment data (masses, centres, velocities, inertia moments,
//       principal directions, elements per fragment, fragment elements).
//   random_extrinsic_mode_i_triangle_3
//       ExtrinsicModeI with sigma_c = 1e6 uniform [-1e5, 1e5] (seed 7):
//       random facet strengths (D-006 RNG).
//   fixture_extrinsic_mode_i_finite_def_<types>
//       TestSMMCFixture ExtrinsicModeIFiniteDef (material_0_finite_def.dat,
//       finite-deformation bulk), every type of dimension 2 and 3 (upstream
//       skips 1-D); recorded as the fixture_* cases.
//   adv_a_tet4_multilaw_mixed, adv_b_tri6_cyclic
//       Adversarial explicit scenarios, reconstructed
//       from the review's parameters: A = 3-D tet4, no cohesive_inserter
//       section (every interior facet checkable), two laws chosen by
//       MeshDataMaterialCohesiveSelector (cohesive_linear with random
//       sigma_c, cohesive_linear_uncoupled on the `insertion` surface),
//       mixed-mode strain; B = 2-D tri6, every facet checkable, random
//       sigma_c, contact after breaking, loading / unloading into compression
//       / reloading. Per step inserted elements, cohesive count, energies;
//       final nodal state, topology, facet_material, every law's filter and
//       internals.
//   adv_c_implicit_mixed_<law>_kappa2
//       Adversarial implicit scenarios: the implicit
//       case with the top x also blocked and moved by ratio * the y
//       increment (mixed mode), beta = 1, kappa = 2 (unsymmetric cohesive
//       tangent). A step where Newton does not converge is recorded
//       (iterations, error, state) and ends the case.
//   adv_v57_implicit_bilinear_contact_reversal,
//   adv_v57_implicit_bilinear_standalone_retry
//       Adversarial implicit scenarios:
//       implicit.msh, intrinsic cohesive_bilinear (sigma_c 1, G_c 0.25,
//       delta_0 0.01, beta 1, kappa 2, penalty 10), top x moved by 0.3 * the
//       y increment. contact_reversal: 30 steps up, 50 down into
//       compression, 60 up past the previous maximum. standalone_retry: 45
//       steps up; after step 15 a standalone assembleInternalForces at
//       u_y + 0.3 (then u restored); step 28 first attempted with
//       max_iterations = 1 (fails), then retried with 100. Per solve
//       attempt: step, converged flag, iterations, error, top reactions,
//       displacement digest; the standalone reactions; final displacement,
//       internal force, blocked flags, K (COO), cohesive internals, mesh.
//   va3_facets_only_triangle_3, va3_ig_tg_rules_zones, va3_ig_tg_zone_btop,
//   va3_friction_prescribed_quadrangle_4
//       insertElements(only_double_facets
//       = true) during an explicit run with the nodal arrays of the doubled
//       nodes; the upstream example cohesive_extrinsic_ig_tg
//       (MaterialCohesiveRulesSelector, cohesive_zones) and its variant with
//       addPhysicalVolume("btop"); cohesive_linear_friction under a prescribed
//       rigid motion (contact with stick, slip and reverse slip, failure,
//       contact after breaking). Eigen's cache sizes are pinned in main()
//       (D-050) and recorded with the OpenBLAS pinning in these cases.
//   v92_ig_tg_zones_update_after_insertion, v92_ig_tg_rules_reversed_key,
//   v92_ig_tg_rules_both_keys, v92_implicit_uncoupled_prestressed_reversal
//       updateAutomaticInsertion with
//       cohesive_zones after the first insertion (upstream throws: UD-808;
//       the reviewer's probe as an oracle case); the example with
//       ig_cohesive listed first so that the default cohesive law differs
//       from the reversed rule's law, and with both keys of the interface
//       pair mapped to different laws (rules lookup order); an
//       implicit cohesive_linear_uncoupled case whose elements are inserted
//       by the stress check at a uniform pre-strain before the first solve
//       (UD-244 compat tangent).
//   cmf_friction_prescribed_tetrahedron_4, cmf_implicit_friction_prestressed_sliding
//       cohesive_linear_friction in
//       3-D under a prescribed rigid motion (explicit; contact with stick,
//       slip, a change of the sliding direction, reverse stick and reverse
//       slip, a change of the penetration, failure, contact after breaking:
//       UD-249's residual sliding update); an implicit static friction case
//       (implicit.msh, insertion by the stress check at a pre-strain,
//       closing into penalty contact, then sliding forward, back and
//       forward again at constant top height: the friction tangent, UD-247)
//       with the history of `delta max` activated by the probe after
//       initFull (UD-028, category A: upstream's implicit friction reads
//       delta_max.previous, which the library never activates; the probe
//       applies the defined behaviour without patching the library).
//
// Usage: cohesive_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>
// Environment: COH_PROBE_CASES=<id>[,<id>...] restricts the run.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <aka_random_generator.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_utils.hh>
#include <non_linear_solver.hh>
#include <fragment_manager.hh>
#include <material_selector_cohesive.hh>
#include <solid_mechanics_model_cohesive.hh>
#include <sparse_matrix_aij.hh>

#include <Eigen/Core>

#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <map>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "cohesive_probe";
constexpr const char * kProbeVersion = "1";

using I64 = std::int64_t;
using Json = ap::Json;

fs::path g_root, g_up, g_frozen;

bool wanted(const std::string & id) {
  const char * env = std::getenv("COH_PROBE_CASES");
  if (env == nullptr || *env == '\0') {
    return true;
  }
  std::stringstream list(env);
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == id) {
      return true;
    }
  }
  return false;
}

std::vector<double> vec_of(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

std::vector<std::uint8_t> vec_of(const Array<bool> & a) {
  std::vector<std::uint8_t> v(a.size() * a.getNbComponent());
  for (std::size_t i = 0; i < v.size(); ++i) {
    v[i] = a.data()[i] ? 1 : 0;
  }
  return v;
}

std::string digest(const std::vector<const Array<Real> *> & arrays) {
  ap::Sha256 h;
  for (const auto * a : arrays) {
    h.update(a->data(), a->size() * a->getNbComponent() * sizeof(double));
  }
  return h.hex();
}

void add_array(ap::FixtureWriter & w, const std::string & name, const Array<Real> & a,
               const std::string & desc) {
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", vec_of(a))
      .description(desc);
}

void add_bools(ap::FixtureWriter & w, const std::string & name, const Array<bool> & a,
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

/// Element encoded as (type name, index, ghost) with the type name's
/// position in the case parameter `element_type_names`; null = (-1, -1, -1).
struct ElementCodec {
  std::vector<std::string> names;
  void push(std::vector<I64> & out, const Element & e) {
    if (e == ElementNull || e.type == _not_defined) {
      out.insert(out.end(), {-1, -1, -1});
      return;
    }
    auto n = std::to_string(e.type);
    I64 k = 0;
    for (; k < I64(names.size()); ++k) {
      if (names[k] == n) {
        break;
      }
    }
    if (k == I64(names.size())) {
      names.push_back(n);
    }
    out.insert(out.end(), {k, I64(e.element), I64(e.ghost_type == _ghost ? 1 : 0)});
  }
};

void record_connectivities(ap::FixtureWriter & w, const Mesh & mesh, const std::string & prefix,
                           Json & types_out) {
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    const auto & c = mesh.getConnectivity(type);
    std::vector<I64> v(c.size() * c.getNbComponent());
    for (std::size_t i = 0; i < v.size(); ++i) {
      v[i] = c.data()[i];
    }
    auto name = std::to_string(type);
    types_out.push(name);
    w.add(prefix + "connectivity/" + name,
          {static_cast<std::size_t>(c.size()), static_cast<std::size_t>(c.getNbComponent())},
          {"element", "node_local"}, "elemental", v)
        .element_type(name)
        .description("connectivity after the run (cohesive elements and doubled facets appended)");
  }
}

/// Facet-mesh adjacency: element_to_subelement (ragged, flattened triples +
/// offsets) and subelement_to_element (triples per row) for every type.
void record_adjacency(ap::FixtureWriter & w, const Mesh & mesh_facets, ElementCodec & codec) {
  auto & p = w.parameters();
  Json e2s_types = Json::array();
  Json s2e_types = Json::array();
  for (auto type : mesh_facets.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    auto name = std::to_string(type);
    if (mesh_facets.getData<std::vector<Element>>("element_to_subelement").exists(type)) {
      const auto & e2s = mesh_facets.getElementToSubelement(type);
      std::vector<I64> flat, offsets{0};
      for (const auto & row : e2s) {
        for (const auto & e : row) {
          codec.push(flat, e);
        }
        offsets.push_back(I64(flat.size() / 3));
      }
      e2s_types.push(name);
      add_ivec(w, "facets/element_to_subelement/" + name, flat,
               "element_to_subelement rows: flattened (type index, element, ghost) triples");
      add_ivec(w, "facets/element_to_subelement_offsets/" + name, offsets,
               "row offsets (in triples) of facets/element_to_subelement");
    }
  }
  for (auto type : mesh_facets.getData<Element>("subelement_to_element")
                       .elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    auto name = std::to_string(type);
    const auto & s2e = mesh_facets.getSubelementToElement(type);
    std::vector<I64> flat;
    for (Int i = 0; i < s2e.size() * s2e.getNbComponent(); ++i) {
      codec.push(flat, s2e.data()[i]);
    }
    s2e_types.push(name);
    w.add("facets/subelement_to_element/" + name,
          {static_cast<std::size_t>(s2e.size()), static_cast<std::size_t>(s2e.getNbComponent()), 3},
          {"element", "facet", "component"}, "table", flat)
        .description("subelement_to_element rows (type index, element, ghost)");
  }
  p["facets/element_to_subelement_types"] = e2s_types;
  p["facets/subelement_to_element_types"] = s2e_types;
}

/// Cohesive internals of material `name` (raw arrays, not-ghost); `extra`
/// lists law-specific internals recorded in addition (new cases only).
void record_cohesive_internals(ap::FixtureWriter & w, SolidMechanicsModelCohesive & model,
                               const std::string & name,
                               const std::vector<std::string> & extra = {}) {
  auto & mat = model.getMaterial(name);
  const auto dim = model.getSpatialDimension();
  const auto & cmat = static_cast<const Material &>(mat);
  for (auto type : cmat.getElementFilter().elementTypes(dim, _not_ghost, _ek_cohesive)) {
    auto tname = std::to_string(type);
    const auto & f = cmat.getElementFilter(type);
    std::vector<I64> fv(f.data(), f.data() + f.size());
    add_ivec(w, "material/" + name + "/filter/" + tname, fv, "element filter");
    std::vector<std::string> internals{
        "opening", "tractions", "contact_tractions", "contact_opening", "delta max", "damage",
        "normal", "reversible_energy", "total_energy", "sigma_c_eff", "delta_c_eff",
        "insertion_stress"};
    internals.insert(internals.end(), extra.begin(), extra.end());
    for (const auto & internal : internals) {
      if (not mat.isInternal<Real>(internal, _ek_cohesive)) {
        continue;
      }
      std::string key = internal;
      for (auto & c : key) {
        if (c == ' ') {
          c = '_';
        }
      }
      add_array(w, "material/" + name + "/" + key + "/" + tname,
                mat.getArray<Real>(internal, type), std::string("internal ") + internal);
    }
  }
}

/// Upstream `StrainIncrement` (test_cohesive_fixture.hh).
class StrainIncrement : public BC::Functor {
public:
  StrainIncrement(const Matrix<Real> & strain, BC::Axis dir) : strain_inc(strain), dir(dir) {}
  void operator()(Idx /*node*/, VectorProxy<bool> & flags, VectorProxy<Real> & primal,
                  const VectorProxy<const Real> & coord) {
    if (std::abs(coord(dir)) < 1e-8) {
      return;
    }
    flags.set(true);
    primal += strain_inc * coord;
  }
  static const BC::Functor::Type type = BC::Functor::_dirichlet;

private:
  Matrix<Real> strain_inc;
  BC::Axis dir;
};

struct FixtureSpec {
  ElementType cohesive_type;
  std::string mesh;         // file name in the frozen test_cohesive directory
  std::string types;        // id suffix
  bool extrinsic;
  bool mode_ii;
  std::string material;     // .dat in upstream test_cohesive
  std::string id;
  bool random{false};
  std::string law_text{}; // replaces the cohesive material section when set
};

/// A case whose upstream run raised: the error is the golden result.
void error_case(const std::string & id, const std::string & desc, const std::string & error) {
  ap::FixtureWriter w(g_root, "model/cohesive/" + id, desc + " (upstream raises)", kProbe,
                      kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.parameters()["upstream_error"] = error;
  add_ivec(w, "upstream_error_marker", {1}, "the upstream run raised upstream_error");
  w.finish();
  std::cout << "  model/cohesive/" << id << " (upstream error: " << error << ")" << std::endl;
}

/// TestSMMCFixture scenario.
void fixture_case(const FixtureSpec & s) {
  const std::string cid = "model/cohesive/" + s.id;
  if (not wanted(s.id)) {
    return;
  }
  const auto dim = Mesh::getSpatialDimension(s.cohesive_type);
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  std::string dat_text;
  {
    std::ifstream in(tdir / s.material);
    std::stringstream ss;
    ss << in.rdbuf();
    dat_text = ss.str();
  }
  fs::path dat = tdir / s.material;
  fs::path tmp_dat;
  if (not s.law_text.empty()) {
    auto start = dat_text.find("  material cohesive_");
    auto end = dat_text.find("]", start);
    dat_text.replace(start, end - start + 1, s.law_text);
    tmp_dat = fs::temp_directory_path() / ("cohesive_probe_" + s.id + ".dat");
    std::ofstream out(tmp_dat);
    out << dat_text;
    dat = tmp_dat;
  }
  if (s.random) {
    // sigma_c with a uniform perturbation (probe-defined)
    auto pos = dat_text.find("sigma_c = 1e6");
    dat_text.replace(pos, std::string("sigma_c = 1e6").size(),
                     "sigma_c = 1e6 uniform [-1e5, 1e5]");
    tmp_dat = fs::temp_directory_path() / ("cohesive_probe_" + s.id + ".dat");
    std::ofstream out(tmp_dat);
    out << dat_text;
    dat = tmp_dat;
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);

  Mesh mesh(dim);
  mesh.read((mdir / s.mesh).string());
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = s.extrinsic);
  const Real stable_dt = model.getStableTimeStep();
  const Real dt = stable_dt * 0.01;
  model.setTimeStep(dt);

  Real surface = 1.;
  Int group_size = 1;
  if (dim > 1) {
    auto facet_type = mesh.getFacetType(s.cohesive_type);
    auto & fe_engine = model.getFEEngineBoundary();
    const auto & group = mesh.getElementGroup("insertion");
    group_size = group.size(_ghost_type = _not_ghost);
    const auto & elements = group.getElements(facet_type);
    Array<Real> ones(fe_engine.getNbIntegrationPoints(facet_type) * group_size);
    ones.set(1.);
    surface = fe_engine.integrate(ones, facet_type, _not_ghost, elements);
  }

  auto & mat_el = model.getMaterial("body");
  auto & mat_co = model.getMaterial("insertion");
  Real sigma_c = mat_co.get("sigma_c");
  Real E = mat_el.get("E");
  Real nu = mat_el.get("nu");
  auto direction = dim == 1 ? _x : _y;
  auto length = mesh.getUpperBounds()(direction) - mesh.getLowerBounds()(direction);
  Matrix<Real> strain;
  Real speed{0.};
  Real init_factor{1. - 1e-5};
  Real inc_factor{2e-2};
  UInt nb_steps{0};
  if (not s.mode_ii) {
    speed = mat_el.getPushWaveSpeed(Element());
    nb_steps = length / speed / model.getTimeStep();
    if (dim == 1) {
      strain = Matrix<Real>{{1.}};
    } else if (dim == 2) {
      strain = Matrix<Real>{{-nu, 0.}, {0., 1. - nu}};
      strain *= (1. + nu);
    } else {
      strain = Matrix<Real>{{-nu, 0., 0.}, {0., 1., 0.}, {0., 0., -nu}};
    }
    strain *= sigma_c / E;
  } else {
    try {
      speed = mat_el.getShearWaveSpeed(Element());
    } catch (...) {
      speed = mat_el.getPushWaveSpeed(Element());
    }
    nb_steps = 2 * length / 2. / speed / model.getTimeStep();
    if (dim > 1) {
      model.applyBC(BC::Dirichlet::FlagOnly(_y), "sides");
    }
    if (dim > 2) {
      model.applyBC(BC::Dirichlet::FlagOnly(_z), "sides");
    }
    Real beta = mat_co.get("beta");
    if (dim == 1) {
      strain = Matrix<Real>{{1.}};
    } else if (dim == 2) {
      strain = Matrix<Real>{{0., 1.}, {0., 0.}};
      strain *= (1. + nu);
    } else {
      strain = Matrix<Real>{{0., 1., 0.}, {0., 0., 0.}, {0., 0., 0.}};
      strain *= (1. + nu);
    }
    strain *= 2 * beta * beta * sigma_c / E;
    inc_factor = 0.005;
  }
  {
    Matrix<Real> init = init_factor * strain;
    for (auto && [pos, disp] :
         zip(make_view(mesh.getNodes(), dim), make_view(model.getDisplacement(), dim))) {
      disp = init * pos;
    }
  }
  Matrix<Real> inc = (1. / 300) * (inc_factor * strain);

  ap::FixtureWriter w(g_root, cid,
                      "TestSMMCFixture " + std::string(s.extrinsic ? "Extrinsic" : "Intrinsic") +
                          (s.mode_ii ? "ModeII" : "ModeI") + " (" + s.types + ")" +
                          (s.random ? " with random sigma_c" : ""),
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mdir / s.mesh,
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/" + s.mesh);
  if (s.random or not s.law_text.empty()) {
    w.add_input_file("material", dat, "probe-defined from upstream test_cohesive/" + s.material);
  } else {
    w.add_input_file("material", dat, "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/" + s.material);
  }
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["cohesive_type"] = std::to_string(s.cohesive_type);
  p["extrinsic"] = s.extrinsic;
  p["mode_ii"] = s.mode_ii;
  p["random_seed"] = 7LL;
  p["stable_time_step"] = stable_dt;
  p["time_step"] = dt;
  p["nb_steps"] = static_cast<long long>(nb_steps);
  p["surface"] = surface;
  p["group_size"] = static_cast<long long>(group_size);
  p["init_factor"] = init_factor;
  p["inc_factor"] = inc_factor;
  add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), 0.01 * getStableTimeStep()");
  add_array(w, "initial_displacement", model.getDisplacement(), "displacement after setInitialCondition");
  add_bools(w, "initial_blocked_dofs", model.getBlockedDOFs(), "blocked flags before the steps");
  if (s.extrinsic) {
    // initial facet stress check data
    Json t = Json::array();
    const auto & mesh_facets = mesh.getMeshFacets();
    for (auto ft : mesh_facets.elementTypes(dim - 1)) {
      add_array(w, "init/tangents/" + std::to_string(ft), model.getTangents(ft), "facet tangents");
      add_array(w, "init/normals/" + std::to_string(ft),
                model.getFEEngine("FacetsFEEngine").getNormalsOnIntegrationPoints(ft),
                "facet normals (FacetsFEEngine)");
      add_array(w, "init/sigma_c/" + std::to_string(ft),
                mat_co.getArray<Real>("sigma_c", ft), "facet strengths (FacetRandomInternalField)");
      t.push(std::to_string(ft));
    }
    p["facet_types"] = t;
  }

  std::vector<I64> nb_cohesive, inserted, digest_steps;
  std::vector<double> ekin, epot, edis, erev, econ;
  Json digests = Json::array();
  StrainIncrement functor(inc, dim == 1 ? _x : _y);
  for (UInt step = 0; step < nb_steps; ++step) {
    model.applyBC(functor, "loading");
    model.applyBC(functor, "fixed");
    I64 n = 0;
    if (s.extrinsic) {
      n = model.checkCohesiveStress();
    }
    inserted.push_back(n);
    nb_cohesive.push_back(mesh.getNbElement(s.cohesive_type));
    model.solveStep();
    ekin.push_back(model.getEnergy("kinetic"));
    epot.push_back(model.getEnergy("potential"));
    edis.push_back(model.getEnergy("dissipated"));
    erev.push_back(model.getEnergy("reversible"));
    econ.push_back(model.getEnergy("cohesive contact"));
    if (step % 25 == 0 or step + 1 == nb_steps) {
      digest_steps.push_back(I64(step));
      digests.push(digest({&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
    }
  }
  add_ivec(w, "steps/digest_steps", digest_steps, "steps whose state digest is recorded (every 25th and the last)");
  add_ivec(w, "steps/inserted", inserted, "checkCohesiveStress() return value per step");
  add_ivec(w, "steps/nb_cohesive", nb_cohesive, "number of cohesive elements after the check");
  add_vec(w, "steps/energy/kinetic", ekin, "getEnergy(kinetic) after each solveStep");
  add_vec(w, "steps/energy/potential", epot, "getEnergy(potential) after each solveStep");
  add_vec(w, "steps/energy/dissipated", edis, "getEnergy(dissipated) after each solveStep");
  add_vec(w, "steps/energy/reversible", erev, "getEnergy(reversible) after each solveStep");
  add_vec(w, "steps/energy/cohesive_contact", econ, "getEnergy(cohesive contact) after each solveStep");
  p["step_digests"] = digests;
  p["step_digests_definition"] =
      "SHA-256 of displacement || velocity || acceleration (binary64 LE) after the solveStep of "
      "each step of steps/digest_steps";
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "velocity", model.getVelocity(), "final velocity");
  add_array(w, "acceleration", model.getAcceleration(), "final acceleration");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  add_array(w, "mass", model.getMass(), "final lumped mass");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "final blocked flags");
  add_array(w, "mesh/nodes", mesh.getNodes(), "final node coordinates (doubled nodes appended)");
  Json types = Json::array();
  record_connectivities(w, mesh, "mesh/", types);
  p["mesh_types"] = types;
  const auto & mesh_facets = mesh.getMeshFacets();
  Json ftypes = Json::array();
  record_connectivities(w, mesh_facets, "facets/", ftypes);
  p["facet_mesh_types"] = ftypes;
  ElementCodec codec;
  record_adjacency(w, mesh_facets, codec);
  record_cohesive_internals(w, model, "insertion");
  for (auto ft : mesh_facets.elementTypes(dim - 1)) {
    const auto & fm = model.getFacetMaterial(ft);
    std::vector<I64> v(fm.data(), fm.data() + fm.size());
    add_ivec(w, "facet_material/" + std::to_string(ft), v, "facet_material");
  }
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  if (not tmp_dat.empty()) {
    fs::remove(tmp_dat);
  }
  std::cout << "  " << cid << std::endl;
}

ModelSolverOptions eigen_options() {
  ModelSolverOptions so;
  so.non_linear_solver_type = NonLinearSolverType::_auto;
  so.sparse_solver_type = SparseSolverType::_eigen;
  return so;
}

/// COO of an AIJ matrix (0-based, storage order).
void record_aij(ap::FixtureWriter & w, const std::string & prefix, SolidMechanicsModel & model,
                const ID & id) {
  if (not model.getDOFManager().hasMatrix(id)) {
    return;
  }
  auto & A = aka::as_type<SparseMatrixAIJ>(model.getDOFManager().getMatrix(id));
  const auto nnz = A.getNbNonZero();
  std::vector<I64> rows(nnz), cols(nnz);
  std::vector<double> vals(nnz);
  for (Int k = 0; k < nnz; ++k) {
    rows[k] = A.getIRN()(k) - 1;
    cols[k] = A.getJCN()(k) - 1;
    vals[k] = A.getA()(k);
  }
  w.parameters()[prefix + "/symmetric"] = A.getMatrixType() == _symmetric;
  add_ivec(w, prefix + "/rows", rows, "SparseMatrixAIJ::getIRN - 1 (storage order)");
  add_ivec(w, prefix + "/cols", cols, "SparseMatrixAIJ::getJCN - 1 (storage order)");
  add_vec(w, prefix + "/values", vals, "SparseMatrixAIJ::getA (storage order)");
}

void implicit_case(const std::string & law, const std::string & law_params) {
  const std::string id = "implicit_static_" + law + "_triangle_6";
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  fs::path dat = fs::temp_directory_path() / ("cohesive_probe_" + id + ".dat");
  {
    std::ofstream out(dat);
    out << "model solid_mechanics_model_cohesive [\n"
        << "  cohesive_inserter [\n    bounding_box = [[-10, 10], [0.9, 1.1]]\n  ]\n"
        << "  material elastic [\n    name = steel\n    rho = 1\n    E = 6\n    nu = 0.0\n  ]\n"
        << "  material cohesive_" << law << " [\n    name = cohesive\n    sigma_c = 1\n"
        << law_params << "  ]\n]\n";
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  Mesh mesh(2);
  mesh.read((tdir / "test_cohesive_intrinsic_impl/implicit.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  ap::FixtureWriter w(g_root, cid,
                      "static implicit cohesive_" + law +
                          " (probe-defined from test_cohesive_intrinsic_impl), Eigen sparse solver",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_tag("implicit");
  w.add_input_file("mesh", tdir / "test_cohesive_intrinsic_impl/implicit.msh",
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "test_cohesive_intrinsic_impl/implicit.msh");
  w.add_input_file("material", dat, "probe-defined");
  auto & p = w.parameters();
  p["law"] = law;
  p["max_step"] = 200LL;
  p["random_seed"] = 7LL;
  std::vector<I64> its;
  std::vector<double> errs, resids, openings;
  Json digests = Json::array();
  std::string error;
  try {
    model.initFull(SolidMechanicsModelCohesiveOptions(_static, eigen_options(), false));
    auto & boundary = model.getBlockedDOFs();
    auto & position = mesh.getNodes();
    auto & displacement = model.getDisplacement();
    const auto nb_nodes = mesh.getNbNodes();
    const Real tol = Math::getTolerance();
    for (Int n = 0; n < nb_nodes; ++n) {
      if (std::abs(position(n, 1)) < tol) {
        boundary(n, 1) = true;
        displacement(n, 1) = 0.0;
      }
      if ((std::abs(position(n, 0)) < tol) && (position(n, 1) < 1.1)) {
        boundary(n, 0) = true;
        displacement(n, 0) = 0.0;
      }
      if ((std::abs(position(n, 0) - 1) < tol) && (std::abs(position(n, 1) - 1) < tol)) {
        boundary(n, 0) = true;
        displacement(n, 0) = 0.0;
      }
      if (std::abs(position(n, 1) - 2) < tol) {
        boundary(n, 1) = true;
      }
    }
    p["nb_cohesive"] = static_cast<long long>(mesh.getNbElement(_cohesive_2d_6));
    auto & solver = model.getNonLinearSolver();
    solver.set("max_iterations", 100);
    solver.set("threshold", 1e-5);
    solver.set("convergence_type", SolveConvergenceCriteria::_residual);
    const auto & mat_coh = dynamic_cast<const MaterialCohesive &>(model.getMaterial(1));
    const Real increment = 3. / 200;
    for (Int step = 0; step < 200; ++step) {
      for (Int n = 0; n < nb_nodes; ++n) {
        if (std::abs(position(n, 1) - 2) < tol) {
          displacement(n, 1) += increment;
        }
      }
      model.solveStep();
      its.push_back(Int(solver.get("nb_iterations")));
      errs.push_back(Real(solver.get("error")));
      const auto & residual = model.getInternalForce();
      Real resid = 0;
      for (Int n = 0; n < nb_nodes; ++n) {
        if (std::abs(position(n, 1) - 2.) / 2. < tol) {
          resid += residual(n, 1);
        }
      }
      resids.push_back(resid);
      openings.push_back(mat_coh.getOpening(_cohesive_2d_6)(0, 1));
      digests.push(digest({&model.getDisplacement()}));
    }
  } catch (std::exception & e) {
    error = e.what();
  }
  if (not error.empty()) {
    p["upstream_error"] = error;
  }
  add_ivec(w, "steps/nb_iterations", its, "Newton iterations per step");
  add_vec(w, "steps/error", errs, "Newton error per step");
  add_vec(w, "steps/top_reaction", resids, "sum of internal_force(n, 1) over the nodes at y = 2");
  add_vec(w, "steps/opening_0_y", openings, "opening(0, 1) of the cohesive material");
  p["step_digests"] = digests;
  p["step_digests_definition"] = "SHA-256 of the displacement after each solveStep";
  if (error.empty()) {
    add_array(w, "displacement", model.getDisplacement(), "final displacement");
    add_array(w, "internal_force", model.getInternalForce(), "final internal force");
    add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "blocked flags");
    record_aij(w, "K", model, "K");
    record_cohesive_internals(w, model, "cohesive");
    add_array(w, "mesh/nodes", mesh.getNodes(), "final node coordinates");
    Json types = Json::array();
    record_connectivities(w, mesh, "mesh/", types);
    p["mesh_types"] = types;
  }
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << (error.empty() ? "" : " (upstream error: " + error + ")")
            << std::endl;
}

void buildfragments_case() {
  const std::string id = "buildfragments";
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path tdir =
      g_up / "test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_buildfragments";
  const fs::path mdir = g_frozen /
                        "test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_buildfragments";
  const Real old_tol = Math::getTolerance();
  Math::setTolerance(1e-14);
  getStaticParser().parse((tdir / "material.dat").string());
  RandomGenerator<Idx>::seed(1);
  const Int spatial_dimension = 2;
  const Int max_steps = 200;
  Real strain_rate = 1.e5;
  Real L = 0.03;
  Mesh mesh(spatial_dimension);
  mesh.read((mdir / "mesh.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  Real stable = model.getStableTimeStep();
  Real time_step = stable * 0.05;
  model.setTimeStep(time_step);
  Real disp_increment = strain_rate * L / 2. * time_step;
  model.assembleMassLumped();
  auto & velocity = model.getVelocity();
  const auto & position = mesh.getNodes();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    velocity(n, 0) = strain_rate * position(n, 0);
  }
  model.applyBC(BC::Dirichlet::FixedValue(0, _x), "Left_side");
  model.applyBC(BC::Dirichlet::FixedValue(0, _x), "Right_side");
  FragmentManager fragment_manager(model, false);
  ap::FixtureWriter w(g_root, cid, "test_cohesive_buildfragments with per-step fragment data",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mdir / "mesh.msh",
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "test_cohesive_buildfragments/mesh.msh");
  w.add_input_file("material", tdir / "material.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "test_cohesive_buildfragments/material.dat");
  auto & p = w.parameters();
  p["math_tolerance"] = 1e-14;
  p["random_seed"] = 1LL;
  p["max_steps"] = static_cast<long long>(max_steps);
  add_vec(w, "time_step", {stable, time_step}, "getStableTimeStep(), 0.05 * getStableTimeStep()");
  std::vector<I64> inserted, nb_fragments;
  std::vector<double> total_mass;
  for (Int s = 1; s <= max_steps; ++s) {
    inserted.push_back(model.checkCohesiveStress());
    model.solveStep();
    model.applyBC(BC::Dirichlet::IncrementValue(-disp_increment, _x), "Left_side");
    model.applyBC(BC::Dirichlet::IncrementValue(disp_increment, _x), "Right_side");
    fragment_manager.computeAllData();
    nb_fragments.push_back(fragment_manager.getNbFragment());
    const auto & fm = fragment_manager.getMass();
    Real tm = 0.;
    for (Int f = 0; f < fm.size(); ++f) {
      tm += fm(f, 0);
    }
    total_mass.push_back(tm);
  }
  add_ivec(w, "steps/inserted", inserted, "checkCohesiveStress() per step");
  add_ivec(w, "steps/nb_fragments", nb_fragments, "getNbFragment() after computeAllData");
  add_vec(w, "steps/total_mass", total_mass, "sum of getMass()(f, 0) (sequential)");
  add_array(w, "fragments/mass", fragment_manager.getMass(), "final fragment masses");
  add_array(w, "fragments/center_of_mass", fragment_manager.getCenterOfMass(), "final centres");
  add_array(w, "fragments/velocity", fragment_manager.getVelocity(), "final velocities");
  add_array(w, "fragments/moments_of_inertia", fragment_manager.getMomentsOfInertia(),
            "final principal moments");
  add_array(w, "fragments/principal_directions", fragment_manager.getPrincipalDirections(),
            "final principal directions");
  {
    const auto & ne = fragment_manager.getNbElementsPerFragment();
    std::vector<I64> v(ne.data(), ne.data() + ne.size());
    add_ivec(w, "fragments/nb_elements", v, "getNbElementsPerFragment()");
  }
  ElementCodec codec;
  {
    std::vector<I64> flat, offsets{0};
    for (Int f = 0; f < fragment_manager.getNbFragment(); ++f) {
      auto & g = fragment_manager.getElementGroup("fragment_" + std::to_string(f));
      for (auto type : g.elementTypes(spatial_dimension, _not_ghost, _ek_not_defined)) {
        for (auto e : g.getElements(type)) {
          codec.push(flat, Element{type, e, _not_ghost});
        }
      }
      offsets.push_back(I64(flat.size() / 3));
    }
    add_ivec(w, "fragments/elements", flat, "fragment_<i> elements (type-major), triples");
    add_ivec(w, "fragments/elements_offsets", offsets, "offsets (triples) per fragment");
  }
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "velocity", model.getVelocity(), "final velocity");
  record_cohesive_internals(w, model, "cohesive");
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  Math::setTolerance(old_tol);
  std::cout << "  " << cid << std::endl;
}

/// Explicit adversarial scenario.
struct ExplicitSpec {
  std::string id;
  std::string desc;
  ElementType cohesive_type;
  std::string mesh;                     // frozen test_cohesive mesh
  std::string dat;                      // full material file
  bool mesh_data_selector{false};       // MeshDataMaterialCohesiveSelector
  std::vector<double> strain_rows;      // dim x dim, row-major
  double strain_scale{1.};              // strain = rows * scale
  double init_factor{1.};               // u0 = init_factor * strain * X
  double inc_factor{1.};                // inc = inc_factor * strain
  std::vector<std::pair<double, int>> schedule; // (k, steps): StrainIncrement(k * inc)
  double dt_factor{0.05};
  long long seed{0};
  std::vector<std::string> laws;        // cohesive materials recorded
};

void explicit_case(const ExplicitSpec & s) {
  const std::string cid = "model/cohesive/" + s.id;
  if (not wanted(s.id)) {
    return;
  }
  const auto dim = Mesh::getSpatialDimension(s.cohesive_type);
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  fs::path dat = fs::temp_directory_path() / ("cohesive_probe_" + s.id + ".dat");
  {
    std::ofstream out(dat);
    out << s.dat;
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(s.seed);
  Mesh mesh(dim);
  mesh.read((mdir / s.mesh).string());
  SolidMechanicsModelCohesive model(mesh);
  if (s.mesh_data_selector) {
    model.setMaterialSelector(std::make_shared<MeshDataMaterialCohesiveSelector>(model));
  }
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  const Real stable_dt = model.getStableTimeStep();
  const Real dt = stable_dt * s.dt_factor;
  model.setTimeStep(dt);
  Matrix<Real> strain(dim, dim);
  for (Int i = 0; i < dim; ++i) {
    for (Int j = 0; j < dim; ++j) {
      strain(i, j) = s.strain_rows[i * dim + j];
    }
  }
  strain *= s.strain_scale;
  {
    Matrix<Real> init = s.init_factor * strain;
    for (auto && [pos, disp] :
         zip(make_view(mesh.getNodes(), dim), make_view(model.getDisplacement(), dim))) {
      disp = init * pos;
    }
  }
  Matrix<Real> inc = s.inc_factor * strain;

  ap::FixtureWriter w(g_root, cid, s.desc, kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_tag("adversarial");
  w.add_input_file("mesh", mdir / s.mesh,
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/" + s.mesh);
  w.add_input_file("material", dat, "probe-defined (review V48 adversarial case)");
  auto & p = w.parameters();
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["cohesive_type"] = std::to_string(s.cohesive_type);
  p["mesh_data_selector"] = s.mesh_data_selector;
  Json rows = Json::array();
  for (auto v : s.strain_rows) {
    rows.push(v);
  }
  p["strain_rows"] = rows;
  p["strain_scale"] = s.strain_scale;
  p["init_factor"] = s.init_factor;
  p["inc_factor"] = s.inc_factor;
  Json sched = Json::array();
  for (const auto & [k, n] : s.schedule) {
    Json e = Json::array();
    e.push(k);
    e.push(static_cast<long long>(n));
    sched.push(e);
  }
  p["schedule"] = sched;
  p["time_step_factor"] = s.dt_factor;
  p["random_seed"] = s.seed;
  Json laws = Json::array();
  for (const auto & l : s.laws) {
    laws.push(l);
  }
  p["laws"] = laws;
  add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), time_step_factor * it");
  add_array(w, "initial_displacement", model.getDisplacement(), "init_factor * strain * X");
  {
    Json t = Json::array();
    const auto & mesh_facets = mesh.getMeshFacets();
    for (auto ft : mesh_facets.elementTypes(dim - 1)) {
      add_array(w, "init/tangents/" + std::to_string(ft), model.getTangents(ft), "facet tangents");
      add_array(w, "init/normals/" + std::to_string(ft),
                model.getFEEngine("FacetsFEEngine").getNormalsOnIntegrationPoints(ft),
                "facet normals (FacetsFEEngine)");
      for (const auto & l : s.laws) {
        auto & mat = model.getMaterial(l);
        if (mat.isInternal<Real>("sigma_c", _ek_regular) and
            mat.getInternal<Real>("sigma_c").exists(ft)) {
          add_array(w, "init/sigma_c/" + l + "/" + std::to_string(ft),
                    mat.getArray<Real>("sigma_c", ft), "facet strengths of the law");
        }
      }
      t.push(std::to_string(ft));
    }
    p["facet_types"] = t;
  }
  std::vector<I64> nb_cohesive, inserted, digest_steps;
  std::vector<double> ekin, epot, edis, erev, econ;
  Json digests = Json::array();
  UInt step = 0;
  for (const auto & [k, n] : s.schedule) {
    StrainIncrement functor(k * inc, _y);
    for (int i = 0; i < n; ++i, ++step) {
      model.applyBC(functor, "loading");
      model.applyBC(functor, "fixed");
      inserted.push_back(model.checkCohesiveStress());
      nb_cohesive.push_back(mesh.getNbElement(s.cohesive_type));
      model.solveStep();
      ekin.push_back(model.getEnergy("kinetic"));
      epot.push_back(model.getEnergy("potential"));
      edis.push_back(model.getEnergy("dissipated"));
      erev.push_back(model.getEnergy("reversible"));
      econ.push_back(model.getEnergy("cohesive contact"));
      if (step % 25 == 0) {
        digest_steps.push_back(I64(step));
        digests.push(
            digest({&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
      }
    }
  }
  p["nb_steps"] = static_cast<long long>(step);
  add_ivec(w, "steps/digest_steps", digest_steps, "steps whose state digest is recorded (every 25th)");
  add_ivec(w, "steps/inserted", inserted, "checkCohesiveStress() return value per step");
  add_ivec(w, "steps/nb_cohesive", nb_cohesive, "number of cohesive elements after the check");
  add_vec(w, "steps/energy/kinetic", ekin, "getEnergy(kinetic) after each solveStep");
  add_vec(w, "steps/energy/potential", epot, "getEnergy(potential) after each solveStep");
  add_vec(w, "steps/energy/dissipated", edis, "getEnergy(dissipated) after each solveStep");
  add_vec(w, "steps/energy/reversible", erev, "getEnergy(reversible) after each solveStep");
  add_vec(w, "steps/energy/cohesive_contact", econ, "getEnergy(cohesive contact) after each solveStep");
  p["step_digests"] = digests;
  p["step_digests_definition"] =
      "SHA-256 of displacement || velocity || acceleration (binary64 LE) after the solveStep of "
      "each step of steps/digest_steps";
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "velocity", model.getVelocity(), "final velocity");
  add_array(w, "acceleration", model.getAcceleration(), "final acceleration");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  add_array(w, "mass", model.getMass(), "final lumped mass");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "final blocked flags");
  add_array(w, "mesh/nodes", mesh.getNodes(), "final node coordinates (doubled nodes appended)");
  Json types = Json::array();
  record_connectivities(w, mesh, "mesh/", types);
  p["mesh_types"] = types;
  const auto & mesh_facets = mesh.getMeshFacets();
  Json ftypes = Json::array();
  record_connectivities(w, mesh_facets, "facets/", ftypes);
  p["facet_mesh_types"] = ftypes;
  ElementCodec codec;
  record_adjacency(w, mesh_facets, codec);
  for (const auto & l : s.laws) {
    record_cohesive_internals(w, model, l,
                              {"delta_n_max", "delta_t_max", "damage_n", "damage_t"});
  }
  for (auto ft : mesh_facets.elementTypes(dim - 1)) {
    const auto & fm = model.getFacetMaterial(ft);
    std::vector<I64> v(fm.data(), fm.data() + fm.size());
    add_ivec(w, "facet_material/" + std::to_string(ft), v, "facet_material");
  }
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

/// Implicit mixed-mode adversarial scenario.
void implicit_mixed_case(const std::string & id, const std::string & law_text, Real ux_ratio,
                         Int max_step) {
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  fs::path dat = fs::temp_directory_path() / ("cohesive_probe_" + id + ".dat");
  {
    std::ofstream out(dat);
    out << "model solid_mechanics_model_cohesive [\n"
        << "  cohesive_inserter [\n    bounding_box = [[-10, 10], [0.9, 1.1]]\n  ]\n"
        << "  material elastic [\n    name = steel\n    rho = 1\n    E = 6\n    nu = 0.0\n  ]\n"
        << law_text << "]\n";
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  Mesh mesh(2);
  mesh.read((tdir / "test_cohesive_intrinsic_impl/implicit.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  ap::FixtureWriter w(g_root, cid,
                      "static implicit mixed-mode cohesive case (review V48 adversarial), Eigen",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_tag("implicit");
  w.add_tag("adversarial");
  w.add_input_file("mesh", tdir / "test_cohesive_intrinsic_impl/implicit.msh",
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "test_cohesive_intrinsic_impl/implicit.msh");
  w.add_input_file("material", dat, "probe-defined (review V48 adversarial case)");
  auto & p = w.parameters();
  p["ux_ratio"] = ux_ratio;
  p["max_step"] = static_cast<long long>(max_step);
  p["random_seed"] = 7LL;
  std::vector<I64> its;
  std::vector<double> errs, rx, ry;
  Json digests = Json::array();
  model.initFull(SolidMechanicsModelCohesiveOptions(_static, eigen_options(), false));
  auto & boundary = model.getBlockedDOFs();
  auto & position = mesh.getNodes();
  auto & displacement = model.getDisplacement();
  const auto nb_nodes = mesh.getNbNodes();
  const Real tol = Math::getTolerance();
  for (Int n = 0; n < nb_nodes; ++n) {
    if (std::abs(position(n, 1)) < tol) {
      boundary(n, 1) = true;
      displacement(n, 1) = 0.0;
    }
    if ((std::abs(position(n, 0)) < tol) && (position(n, 1) < 1.1)) {
      boundary(n, 0) = true;
      displacement(n, 0) = 0.0;
    }
    if ((std::abs(position(n, 0) - 1) < tol) && (std::abs(position(n, 1) - 1) < tol)) {
      boundary(n, 0) = true;
      displacement(n, 0) = 0.0;
    }
    if (std::abs(position(n, 1) - 2) < tol) {
      boundary(n, 0) = true;
      boundary(n, 1) = true;
    }
  }
  p["nb_cohesive"] = static_cast<long long>(mesh.getNbElement(_cohesive_2d_6));
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 100);
  solver.set("threshold", 1e-5);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  const Real increment = 3. / 200;
  Int failed_step = -1;
  for (Int step = 0; step < max_step; ++step) {
    for (Int n = 0; n < nb_nodes; ++n) {
      if (std::abs(position(n, 1) - 2) < tol) {
        displacement(n, 1) += increment;
        displacement(n, 0) += ux_ratio * increment;
      }
    }
    bool converged = true;
    try {
      model.solveStep();
    } catch (debug::NLSNotConvergedException &) {
      converged = false;
    }
    its.push_back(Int(solver.get("nb_iterations")));
    errs.push_back(Real(solver.get("error")));
    const auto & residual = model.getInternalForce();
    Real sx = 0, sy = 0;
    for (Int n = 0; n < nb_nodes; ++n) {
      if (std::abs(position(n, 1) - 2.) / 2. < tol) {
        sx += residual(n, 0);
        sy += residual(n, 1);
      }
    }
    rx.push_back(sx);
    ry.push_back(sy);
    digests.push(digest({&model.getDisplacement()}));
    if (not converged) {
      failed_step = step;
      break;
    }
  }
  p["upstream_nonconverged_step"] = static_cast<long long>(failed_step);
  add_ivec(w, "steps/nb_iterations", its, "Newton iterations per step");
  add_vec(w, "steps/error", errs, "Newton error per step (last iteration)");
  add_vec(w, "steps/top_reaction_x", rx, "sum of internal_force(n, 0) over the nodes at y = 2");
  add_vec(w, "steps/top_reaction_y", ry, "sum of internal_force(n, 1) over the nodes at y = 2");
  p["step_digests"] = digests;
  p["step_digests_definition"] = "SHA-256 of the displacement after each solveStep";
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "blocked flags");
  record_aij(w, "K", model, "K");
  record_cohesive_internals(w, model, "cohesive");
  add_array(w, "mesh/nodes", mesh.getNodes(), "final node coordinates");
  Json types = Json::array();
  record_connectivities(w, mesh, "mesh/", types);
  p["mesh_types"] = types;
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid
            << (failed_step < 0 ? "" : " (Newton not converged at step " +
                                           std::to_string(failed_step) + ")")
            << std::endl;
}

// ---- adversarial implicit cases (adopted by) --------
struct V57Phase {
  Real sign;
  Int steps;
  /// top x increment in units of the y increment; NaN:
  /// sign * ux_ratio (the review cases)
  Real x{std::numeric_limits<Real>::quiet_NaN()};
};

void record_pinning(ap::FixtureWriter & w);

/// Options of the review variant of v57_case (defaults: the original review cases).
struct V57Options {
  /// > 0: extrinsic model; after the boundary conditions the displacement
  /// is set to u_y = prestress_strain * y (uniform strain), the internal
  /// forces assembled and checkCohesiveStress() inserts the cohesive
  /// elements before the first solve (no insertion during the solves, so
  /// the stiffness profile is built once, cf. UD-197)
  Real prestress_strain{0.};
  /// law-specific internals recorded in addition
  std::vector<std::string> extra_internals{};
  /// manifest description (default: the original one)
  std::string desc{};
  /// activate the history of the internal "delta max" after initFull
  /// (UD-028 defined behaviour; required by implicit cohesive_linear_friction,
  /// whose traction reads delta_max.previous)
  bool delta_max_history{false};
  /// manifest description of the material input (default: the original one)
  std::string material_source{};
  /// x blocked on the left side only below the band (y < 0.9) and not at
  /// (1, 1): the interface (and the doubled nodes, which copy the blocked
  /// flags) can slide
  bool interface_free_x{false};
};

/// The implicit friction case (parameters pre-screened on
/// the port: upstream's Newton converges at every step with k_f = 1, while
/// its tangent has no friction stiffness in sustained contact, UD-247).
const std::string CMF_IMPLICIT_LAW =
    "  material cohesive_linear_friction [\n    name = cohesive\n    sigma_c = 1\n"
    "    G_c = 1.2\n    beta = 1\n    kappa = 2\n    penalty = 10\n    mu = 0.1\n"
    "    penalty_for_friction = 1\n  ]\n";
const std::vector<V57Phase> CMF_IMPLICIT_SCHEDULE = {
    {1., 10, 0.}, {-1., 40, 0.}, {0., 20, 1.}, {0., 25, -1.}, {0., 15, 1.}, {1., 30, 0.}};

void v57_case(const std::string & id, const std::string & law_text, Real ux_ratio,
              const std::vector<V57Phase> & schedule, Int standalone_at, Int fail_at,
              const V57Options & opt = V57Options{}) {
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  fs::path dat = fs::temp_directory_path() / ("cohesive_probe_" + id + ".dat");
  {
    std::ofstream out(dat);
    out << "model solid_mechanics_model_cohesive [\n"
        << "  cohesive_inserter [\n    bounding_box = [[-10, 10], [0.9, 1.1]]\n  ]\n"
        << "  material elastic [\n    name = steel\n    rho = 1\n    E = 6\n    nu = 0.0\n  ]\n"
        << law_text << "]\n";
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  Mesh mesh(2);
  mesh.read((tdir / "test_cohesive_intrinsic_impl/implicit.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  const bool prestress = opt.prestress_strain > 0.;
  ap::FixtureWriter w(g_root, cid,
                      opt.desc.empty() ? std::string("review V57 adversarial implicit case")
                                       : opt.desc,
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_tag("implicit");
  w.add_tag("adversarial");
  w.add_input_file("mesh", tdir / "test_cohesive_intrinsic_impl/implicit.msh",
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "test_cohesive_intrinsic_impl/implicit.msh");
  w.add_input_file("material", dat,
                   not opt.material_source.empty() ? opt.material_source
                   : prestress ? "probe-defined (review V92 G92-05)"
                               : "probe-defined (review V57 adversarial case)");
  auto & p = w.parameters();
  p["ux_ratio"] = ux_ratio;
  p["random_seed"] = 7LL;
  p["standalone_at"] = static_cast<long long>(standalone_at);
  p["fail_at"] = static_cast<long long>(fail_at);
  if (prestress) {
    record_pinning(w);
    p["prestress_strain"] = opt.prestress_strain;
  }
  Json sched = Json::array();
  for (const auto & ph : schedule) {
    Json e = Json::array();
    e.push(ph.sign);
    e.push(static_cast<long long>(ph.steps));
    if (not std::isnan(ph.x)) {
      e.push(ph.x);
    }
    sched.push(e);
  }
  p["schedule"] = sched;
  std::vector<I64> its, att_step, att_conv;
  std::vector<double> errs, rx, ry, sa;
  Json digests = Json::array();
  model.initFull(SolidMechanicsModelCohesiveOptions(_static, eigen_options(), prestress));
  if (opt.delta_max_history) {
    model.getMaterial("cohesive").getInternal<Real>("delta max").initializeHistory();
    p["probe_delta_max_history"] = true;
  }
  auto & boundary = model.getBlockedDOFs();
  auto & position = mesh.getNodes();
  auto & displacement = model.getDisplacement();
  const auto nb_nodes = mesh.getNbNodes();
  const Real tol = Math::getTolerance();
  if (opt.interface_free_x) {
    p["interface_free_x"] = true;
  }
  const Real y_left = opt.interface_free_x ? 0.9 : 1.1;
  for (Int n = 0; n < nb_nodes; ++n) {
    if (std::abs(position(n, 1)) < tol) {
      boundary(n, 1) = true;
      displacement(n, 1) = 0.0;
    }
    if ((std::abs(position(n, 0)) < tol) && (position(n, 1) < y_left)) {
      boundary(n, 0) = true;
      displacement(n, 0) = 0.0;
    }
    if (not opt.interface_free_x and (std::abs(position(n, 0) - 1) < tol) and
        (std::abs(position(n, 1) - 1) < tol)) {
      boundary(n, 0) = true;
      displacement(n, 0) = 0.0;
    }
    if (std::abs(position(n, 1) - 2) < tol) {
      boundary(n, 0) = true;
      boundary(n, 1) = true;
    }
  }
  if (prestress) {
    for (Int n = 0; n < nb_nodes; ++n) {
      displacement(n, 1) = opt.prestress_strain * position(n, 1);
    }
    model.assembleInternalForces();
    I64 inserted = model.checkCohesiveStress();
    p["prestress_inserted"] = static_cast<long long>(inserted);
    add_array(w, "prestress/internal_force", model.getInternalForce(),
              "internal force assembled at the pre-strain, after the insertion "
              "(array resized to the doubled nodes)");
  }
  p["nb_cohesive"] = static_cast<long long>(mesh.getNbElement(_cohesive_2d_6));
  auto & solver = model.getNonLinearSolver();
  solver.set("max_iterations", 100);
  solver.set("threshold", 1e-5);
  solver.set("convergence_type", SolveConvergenceCriteria::_residual);
  const Real increment = 3. / 200;
  auto record = [&](Int step, bool converged) {
    att_step.push_back(step);
    att_conv.push_back(converged ? 1 : 0);
    its.push_back(Int(solver.get("nb_iterations")));
    errs.push_back(Real(solver.get("error")));
    const auto & residual = model.getInternalForce();
    Real sx = 0, sy = 0;
    for (Int n = 0; n < nb_nodes; ++n) {
      if (std::abs(position(n, 1) - 2.) / 2. < tol) {
        sx += residual(n, 0);
        sy += residual(n, 1);
      }
    }
    rx.push_back(sx);
    ry.push_back(sy);
    digests.push(digest({&model.getDisplacement()}));
  };
  auto solve = [&]() {
    try {
      model.solveStep();
      return true;
    } catch (debug::NLSNotConvergedException &) {
      return false;
    }
  };
  Int failed_step = -1;
  Int step = 0;
  bool stop = false;
  for (const auto & ph : schedule) {
    for (Int k = 0; k < ph.steps and not stop; ++k, ++step) {
      for (Int n = 0; n < nb_nodes; ++n) {
        if (std::abs(position(n, 1) - 2) < tol) {
          displacement(n, 1) += ph.sign * increment;
          if (std::isnan(ph.x)) {
            displacement(n, 0) += ph.sign * ux_ratio * increment;
          } else {
            displacement(n, 0) += ph.x * increment;
          }
        }
      }
      if (step == fail_at) {
        solver.set("max_iterations", 1);
        bool c = solve();
        record(step, c);
        solver.set("max_iterations", 100);
      }
      bool c = solve();
      record(step, c);
      if (not c) {
        failed_step = step;
        stop = true;
        break;
      }
      if (step == standalone_at) {
        Array<Real> saved(displacement, "saved");
        for (Int n = 0; n < nb_nodes; ++n) {
          if (std::abs(position(n, 1) - 2) < tol) {
            displacement(n, 1) += 0.3;
          }
        }
        model.assembleInternalForces();
        const auto & residual = model.getInternalForce();
        Real sx = 0, sy = 0;
        for (Int n = 0; n < nb_nodes; ++n) {
          if (std::abs(position(n, 1) - 2.) / 2. < tol) {
            sx += residual(n, 0);
            sy += residual(n, 1);
          }
        }
        sa.push_back(sx);
        sa.push_back(sy);
        displacement.copy(saved);
      }
    }
    if (stop) {
      break;
    }
  }
  p["upstream_nonconverged_step"] = static_cast<long long>(failed_step);
  add_ivec(w, "attempts/step", att_step, "step index of each solve attempt");
  add_ivec(w, "attempts/converged", att_conv, "1 if the attempt converged");
  add_ivec(w, "attempts/nb_iterations", its, "Newton iterations per attempt");
  add_vec(w, "attempts/error", errs, "Newton error per attempt");
  add_vec(w, "attempts/top_reaction_x", rx, "sum of internal_force(n, 0) at y = 2");
  add_vec(w, "attempts/top_reaction_y", ry, "sum of internal_force(n, 1) at y = 2");
  add_vec(w, "standalone/top_reaction", sa, "standalone assembleInternalForces at the overshoot");
  p["attempt_digests"] = digests;
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  add_bools(w, "blocked_dofs", model.getBlockedDOFs(), "blocked flags");
  record_aij(w, "K", model, "K");
  record_cohesive_internals(w, model, "cohesive", opt.extra_internals);
  add_array(w, "mesh/nodes", mesh.getNodes(), "final node coordinates");
  Json types = Json::array();
  record_connectivities(w, mesh, "mesh/", types);
  p["mesh_types"] = types;
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid
            << (failed_step < 0 ? "" : " (Newton not converged at step " +
                                           std::to_string(failed_step) + ")")
            << std::endl;
}

// ---------------------------------------------------------------------------
// cases va3_*.
// D-050: Eigen's cache sizes are pinned in main(); every va3 case records the
// sizes Eigen reports and the OpenBLAS pinning of its environment.
// ---------------------------------------------------------------------------

void record_pinning(ap::FixtureWriter & w) {
  auto & p = w.parameters();
  Json c = Json::array();
  c.push(static_cast<long long>(Eigen::l1CacheSize()));
  c.push(static_cast<long long>(Eigen::l2CacheSize()));
  c.push(static_cast<long long>(Eigen::l3CacheSize()));
  p["eigen_cache_sizes"] = c;
  const char * ct = std::getenv("OPENBLAS_CORETYPE");
  const char * nt = std::getenv("OPENBLAS_NUM_THREADS");
  p["openblas_coretype"] = std::string(ct == nullptr ? "" : ct);
  p["openblas_num_threads"] = std::string(nt == nullptr ? "" : nt);
}

void add_ints(ap::FixtureWriter & w, const std::string & name, const Array<Idx> & a,
              const std::string & desc) {
  std::vector<I64> v(a.size() * a.getNbComponent());
  for (std::size_t i = 0; i < v.size(); ++i) {
    v[i] = a.data()[i];
  }
  w.add(name, {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
        {"entry", "component"}, "table", v)
      .description(desc);
}

/// Topology of mesh and facet mesh with the adjacency (as the fixture cases).
void record_topology(ap::FixtureWriter & w, Mesh & mesh, const std::string & prefix,
                     ElementCodec & codec) {
  auto & p = w.parameters();
  add_array(w, prefix + "mesh/nodes", mesh.getNodes(), "node coordinates (doubled nodes appended)");
  Json types = Json::array();
  record_connectivities(w, mesh, prefix + "mesh/", types);
  p[prefix + "mesh_types"] = types;
  const auto & mesh_facets = mesh.getMeshFacets();
  Json ftypes = Json::array();
  record_connectivities(w, mesh_facets, prefix + "facets/", ftypes);
  p[prefix + "facet_mesh_types"] = ftypes;
  if (prefix.empty()) {
    record_adjacency(w, mesh_facets, codec);
  }
}

/// insertion/check flags of the inserter for every facet type.
void record_flags(ap::FixtureWriter & w, SolidMechanicsModelCohesive & model,
                  const std::string & prefix) {
  auto & inserter = model.getElementInserter();
  const auto dim = model.getSpatialDimension();
  for (auto ft : model.getMesh().getMeshFacets().elementTypes(dim - 1)) {
    add_bools(w, prefix + "check_facets/" + std::to_string(ft), inserter.getCheckFacets(ft),
              "CohesiveElementInserter::getCheckFacets");
    add_bools(w, prefix + "insertion_facets/" + std::to_string(ft),
              inserter.getInsertionFacets(ft), "CohesiveElementInserter::getInsertionFacets");
  }
}

/// The nodal arrays upstream onNodesAdded copies to doubled nodes.
void record_nodal_state(ap::FixtureWriter & w, SolidMechanicsModelCohesive & model,
                        const std::string & prefix) {
  add_array(w, prefix + "displacement", model.getDisplacement(), "displacement");
  add_array(w, prefix + "velocity", model.getVelocity(), "velocity");
  add_array(w, prefix + "acceleration", model.getAcceleration(), "acceleration");
  add_array(w, prefix + "previous_displacement", model.getPreviousDisplacement(),
            "previous displacement (DOF manager previous)");
  add_array(w, prefix + "increment", model.getIncrement(), "displacement increment");
  add_array(w, prefix + "solution", model.getDOFManager().getSolution("displacement"),
            "DOFManager::getSolution(displacement)");
  add_bools(w, prefix + "blocked_dofs", model.getBlockedDOFs(), "blocked flags");
  add_array(w, prefix + "mass", model.getMass(), "lumped mass");
}

/// va3_facets_only_triangle_3: TestSMMCFixture ExtrinsicModeI set-up on the
/// frozen tri3 mesh, initial strain 0.5 x the mode-I strain, 10 explicit
/// steps without stress check, then insertElements(only_double_facets = true)
/// on the checkable facets (the `insertion` surface), then 30 more steps.
void va3_facets_only_case() {
  const std::string id = "va3_facets_only_triangle_3";
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path tdir = g_up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  getStaticParser().parse((tdir / "material_0.dat").string());
  RandomGenerator<Idx>::seed(7);
  Mesh mesh(2);
  mesh.read((mdir / "_cohesive_2d_4_triangle_3.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  const Real stable_dt = model.getStableTimeStep();
  const Real dt = stable_dt * 0.01;
  model.setTimeStep(dt);
  auto & mat_el = model.getMaterial("body");
  auto & mat_co = model.getMaterial("insertion");
  Real sigma_c = mat_co.get("sigma_c");
  Real E = mat_el.get("E");
  Real nu = mat_el.get("nu");
  Matrix<Real> strain{{-nu, 0.}, {0., 1. - nu}};
  strain *= (1. + nu);
  strain *= sigma_c / E;
  {
    Matrix<Real> init = 0.5 * strain;
    for (auto && [pos, disp] :
         zip(make_view(mesh.getNodes(), 2), make_view(model.getDisplacement(), 2))) {
      disp = init * pos;
    }
  }
  Matrix<Real> inc = (1. / 300) * (2e-2 * strain);
  ap::FixtureWriter w(g_root, cid,
                      "insertElements(only_double_facets = true) during an explicit run "
                      "(TestSMMCFixture ExtrinsicModeI set-up, triangle_3)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mdir / "_cohesive_2d_4_triangle_3.msh",
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "_cohesive_2d_4_triangle_3.msh");
  w.add_input_file("material", tdir / "material_0.dat",
                   "upstream:test/test_model/test_solid_mechanics_model/test_cohesive/material_0.dat");
  record_pinning(w);
  auto & p = w.parameters();
  p["random_seed"] = 7LL;
  p["init_factor"] = 0.5;
  p["inc_factor"] = 2e-2;
  p["steps_before"] = 10LL;
  p["steps_after"] = 30LL;
  add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), 0.01 * it");
  StrainIncrement functor(inc, _y);
  std::vector<double> ekin, epot;
  Json digests = Json::array();
  for (Int step = 0; step < 10; ++step) {
    model.applyBC(functor, "loading");
    model.applyBC(functor, "fixed");
    model.solveStep();
    ekin.push_back(model.getEnergy("kinetic"));
    epot.push_back(model.getEnergy("potential"));
  }
  auto & inserter = model.getElementInserter();
  for (auto ft : mesh.getMeshFacets().elementTypes(1)) {
    inserter.getInsertionFacets(ft).copy(inserter.getCheckFacets(ft));
  }
  record_flags(w, model, "before/");
  add_array(w, "before/displacement", model.getDisplacement(), "displacement before the insertion");
  const auto nb_returned = inserter.insertElements(true);
  add_ivec(w, "facets_only/returned", {I64(nb_returned)},
           "CohesiveElementInserter::insertElements(true) return value");
  add_ivec(w, "facets_only/nb_cohesive", {I64(mesh.getNbElement(_cohesive_2d_4))},
           "cohesive elements after the insertion");
  record_flags(w, model, "after/");
  record_nodal_state(w, model, "after/");
  ElementCodec codec;
  record_topology(w, mesh, "", codec);
  for (Int step = 0; step < 30; ++step) {
    model.applyBC(functor, "loading");
    model.applyBC(functor, "fixed");
    model.solveStep();
    ekin.push_back(model.getEnergy("kinetic"));
    epot.push_back(model.getEnergy("potential"));
    digests.push(digest({&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
  }
  add_vec(w, "steps/energy/kinetic", ekin, "getEnergy(kinetic) after each solveStep (40 steps)");
  add_vec(w, "steps/energy/potential", epot, "getEnergy(potential) after each solveStep");
  p["step_digests"] = digests;
  p["step_digests_definition"] =
      "SHA-256 of displacement || velocity || acceleration after each step following the "
      "insertion";
  record_nodal_state(w, model, "final/");
  add_array(w, "final/internal_force", model.getInternalForce(), "final internal force");
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  std::cout << "  " << cid << std::endl;
}

/// Upstream examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg
/// `Velocity` functor.
class IgTgVelocity : public BC::Dirichlet::DirichletFunctor {
public:
  IgTgVelocity(SolidMechanicsModel & model, Real vel, BC::Axis ax)
      : DirichletFunctor(ax), model(model), vel(vel) {
    disp = vel * model.getTimeStep();
  }
  inline void operator()(Idx node, VectorProxy<bool> & /*flags*/, VectorProxy<Real> & disp,
                         const VectorProxy<const Real> & coord) override {
    Real sign = std::signbit(coord(axis)) ? -1. : 1.;
    disp(axis) += sign * this->disp;
    model.getVelocity()(node, axis) = sign * vel;
  }

private:
  SolidMechanicsModel & model;
  Real vel, disp;
};

/// Variants of the upstream example cohesive_extrinsic_ig_tg (va3_ig_tg_*,
/// v92_ig_tg_*). Defaults: the example as written.
struct IgTgVariant {
  /// material.dat without the cohesive_zones line, addPhysicalVolume("btop")
  bool zone_btop{false};
  /// material.dat with ig_cohesive listed before tg_cohesive (the default
  /// cohesive law, the first cohesive one, is then ig_cohesive)
  bool ig_first{false};
  /// rules (side 1, side 2, law) replacing the example's when not empty
  std::vector<std::vector<std::string>> rules{};
  /// manifest description (default: the va3 descriptions)
  std::string desc{};
};

/// The example's material.dat with the variant's edits (`tmp_dat` set when
/// a probe-defined copy is written).
fs::path ig_tg_material(const std::string & id, const IgTgVariant & v, fs::path & tmp_dat) {
  const fs::path edir = g_up / "examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg";
  fs::path dat = edir / "material.dat";
  if (not v.zone_btop and not v.ig_first) {
    return dat;
  }
  std::ifstream in(dat);
  std::stringstream ss;
  ss << in.rdbuf();
  std::string text = ss.str();
  if (v.zone_btop) {
    const std::string line = "     cohesive_zones = [btop, bbottom]\n";
    auto pos = text.find(line);
    if (pos == std::string::npos) {
      throw std::runtime_error("ig_tg material.dat: cohesive_zones line not found");
    }
    text.erase(pos, line.size());
  }
  if (v.ig_first) {
    const auto tg = text.find("   material cohesive_linear [\n   \t  name = tg_cohesive");
    const auto ig = text.find("   material cohesive_linear [\n      name = ig_cohesive");
    const auto end = text.rfind("]\n");
    if (tg == std::string::npos or ig == std::string::npos or not(tg < ig and ig < end)) {
      throw std::runtime_error("ig_tg material.dat: cohesive sections not found");
    }
    const std::string tg_block = text.substr(tg, ig - tg); // ends with "   ]\n\n"
    const std::string ig_block = text.substr(ig, end - ig); // ends with "   ]\n"
    text = text.substr(0, tg) + ig_block + "\n" + tg_block.substr(0, tg_block.size() - 1) + "]\n";
  }
  tmp_dat = fs::temp_directory_path() / ("cohesive_probe_" + id + ".dat");
  std::ofstream out(tmp_dat);
  out << text;
  return tmp_dat;
}

/// The example's MaterialCohesiveRules, or the variant's.
MaterialCohesiveRules ig_tg_rules(const IgTgVariant & v) {
  if (v.rules.empty()) {
    return MaterialCohesiveRules{{{"btop", "bbottom"}, "tg_cohesive"},
                                 {{"btop", "btop"}, "ig_cohesive"},
                                 {{"bbottom", "bbottom"}, "ig_cohesive"}};
  }
  MaterialCohesiveRules rules;
  for (const auto & r : v.rules) {
    rules[std::make_pair(r.at(0), r.at(1))] = r.at(2);
  }
  return rules;
}

/// va3_ig_tg_rules_zones (the upstream example: cohesive_zones = [btop,
/// bbottom], MaterialCohesiveRulesSelector), va3_ig_tg_zone_btop (the
/// example's material without cohesive_zones, addPhysicalVolume("btop")) and
/// the rules variants v92_ig_tg_*.
void va3_ig_tg_case(const std::string & id, const IgTgVariant & v) {
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const bool zone_btop = v.zone_btop;
  const fs::path mfile =
      g_frozen / "examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/square.msh";
  fs::path tmp_dat;
  const fs::path dat = ig_tg_material(id, v, tmp_dat);
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  const Int spatial_dimension = 2;
  const Int max_steps = 1000;
  Mesh mesh(spatial_dimension);
  mesh.read(mfile.string());
  SolidMechanicsModelCohesive model(mesh);
  MaterialCohesiveRules rules = ig_tg_rules(v);
  auto cohesive_material_selector = std::make_shared<MaterialCohesiveRulesSelector>(model, rules);
  auto bulk_material_selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  auto && current_selector = model.getMaterialSelector();
  cohesive_material_selector->setFallback(bulk_material_selector);
  bulk_material_selector->setFallback(current_selector);
  model.setMaterialSelector(cohesive_material_selector);
  if (zone_btop) {
    model.getElementInserter().addPhysicalVolume("btop");
  }
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  const Real stable_dt = model.getStableTimeStep();
  Real time_step = stable_dt * 0.05;
  model.setTimeStep(time_step);
  model.assembleMassLumped();
  auto & position = mesh.getNodes();
  auto & velocity = model.getVelocity();
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "top");
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "right");
  Real loading_rate = 0.1;
  Real VI = loading_rate * 2 * 0.5;
  for (auto && [pos, vel] :
       zip(make_view(position, spatial_dimension), make_view(velocity, spatial_dimension))) {
    vel = loading_rate * pos;
  }
  IgTgVelocity vely(model, VI, _y);
  IgTgVelocity velx(model, VI, _x);

  const std::string desc =
      not v.desc.empty() ? v.desc
      : zone_btop        ? "upstream example cohesive_extrinsic_ig_tg with "
                           "addPhysicalVolume(btop) instead of cohesive_zones"
                         : "upstream example cohesive_extrinsic_ig_tg "
                           "(MaterialCohesiveRulesSelector, cohesive_zones)";
  ap::FixtureWriter w(g_root, cid, desc, kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mfile,
                   "frozen:examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/"
                   "square.msh");
  std::string edits;
  if (zone_btop) {
    edits = "cohesive_zones line removed";
  }
  if (v.ig_first) {
    edits += std::string(edits.empty() ? "" : "; ") + "ig_cohesive listed before tg_cohesive";
  }
  w.add_input_file("material", dat,
                   edits.empty() ? "upstream:examples/c++/solid_mechanics_cohesive_model/"
                                   "cohesive_extrinsic_ig_tg/material.dat"
                                 : "probe-defined from upstream:examples/c++/"
                                   "solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/"
                                   "material.dat (" + edits + ")");
  record_pinning(w);
  auto & p = w.parameters();
  p["random_seed"] = 7LL;
  p["max_steps"] = static_cast<long long>(max_steps);
  p["zone_btop"] = zone_btop;
  p["loading_rate"] = loading_rate;
  // review variants only (the va3 manifests are unchanged)
  if (v.ig_first) {
    p["ig_first"] = true;
  }
  if (not v.rules.empty()) {
    Json jr = Json::array();
    for (const auto & r : v.rules) {
      Json e = Json::array();
      for (const auto & x : r) {
        e.push(x);
      }
      jr.push(e);
    }
    p["rules"] = jr;
  }
  add_vec(w, "time_step", {stable_dt, time_step}, "getStableTimeStep(), 0.05 * it");
  const auto & mesh_facets = mesh.getMeshFacets();
  Json ft_names = Json::array();
  for (auto ft : mesh_facets.elementTypes(spatial_dimension - 1)) {
    const auto & fm = model.getFacetMaterial(ft);
    add_ints(w, "init/facet_material/" + std::to_string(ft), fm, "facet_material after initFull");
    ft_names.push(std::to_string(ft));
  }
  p["facet_types"] = ft_names;
  record_flags(w, model, "init/");
  for (auto type : mesh.elementTypes(spatial_dimension, _not_ghost, _ek_regular)) {
    add_ints(w, "init/material_index/" + std::to_string(type), model.getMaterialByElement(type),
             "law index of each bulk element (getMaterialByElement)");
  }
  std::vector<I64> inserted, nb_cohesive;
  std::vector<double> ekin, epot, edis, erev, econ;
  Json digests = Json::array();
  bool recorded_first = false;
  for (Int s = 1; s <= max_steps; ++s) {
    model.applyBC(vely, "top");
    model.applyBC(vely, "bottom");
    model.applyBC(velx, "left");
    model.applyBC(velx, "right");
    I64 n = model.checkCohesiveStress();
    inserted.push_back(n);
    nb_cohesive.push_back(I64(mesh.getNbElement(_cohesive_2d_4)));
    if (n > 0 and not recorded_first) {
      recorded_first = true;
      p["first_insertion_step"] = static_cast<long long>(s);
      record_nodal_state(w, model, "first_insertion/");
    }
    model.solveStep();
    ekin.push_back(model.getEnergy("kinetic"));
    epot.push_back(model.getEnergy("potential"));
    edis.push_back(model.getEnergy("dissipated"));
    erev.push_back(model.getEnergy("reversible"));
    econ.push_back(model.getEnergy("cohesive contact"));
    if (s % 50 == 0) {
      digests.push(digest({&model.getDisplacement(), &model.getVelocity(), &model.getAcceleration()}));
    }
  }
  add_ivec(w, "steps/inserted", inserted, "checkCohesiveStress() return value per step");
  add_ivec(w, "steps/nb_cohesive", nb_cohesive, "cohesive elements after the check");
  add_vec(w, "steps/energy/kinetic", ekin, "getEnergy(kinetic) after each solveStep");
  add_vec(w, "steps/energy/potential", epot, "getEnergy(potential) after each solveStep");
  add_vec(w, "steps/energy/dissipated", edis, "getEnergy(dissipated) after each solveStep");
  add_vec(w, "steps/energy/reversible", erev, "getEnergy(reversible) after each solveStep");
  add_vec(w, "steps/energy/cohesive_contact", econ, "getEnergy(cohesive contact) after each solveStep");
  p["step_digests"] = digests;
  p["step_digests_definition"] =
      "SHA-256 of displacement || velocity || acceleration after the solveStep of every 50th step";
  record_nodal_state(w, model, "final/");
  add_array(w, "final/internal_force", model.getInternalForce(), "final internal force");
  ElementCodec codec;
  record_topology(w, mesh, "", codec);
  record_flags(w, model, "final/");
  for (auto ft : mesh_facets.elementTypes(spatial_dimension - 1)) {
    add_ints(w, "final/facet_material/" + std::to_string(ft), model.getFacetMaterial(ft),
             "facet_material at the end");
  }
  for (auto type : mesh.elementTypes(spatial_dimension, _not_ghost, _ek_not_defined)) {
    add_ints(w, "final/material_index/" + std::to_string(type), model.getMaterialByElement(type),
             "law index of each element at the end (getMaterialByElement)");
  }
  for (const auto * law : {"tg_cohesive", "ig_cohesive"}) {
    record_cohesive_internals(w, model, law);
  }
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  if (not tmp_dat.empty()) {
    fs::remove(tmp_dat);
  }
  std::cout << "  " << cid << std::endl;
}

/// va3_friction_prescribed_quadrangle_4: cohesive_linear_friction on the
/// frozen quad4 mesh, inserted by a uniform tension of 2 sigma_c, then a
/// prescribed rigid motion (dx, dy) of the upper half (every DOF blocked,
/// explicit steps): partial opening, contact with sliding (stick, slip),
/// reverse sliding (stick, reverse slip), forward again, complete failure,
/// contact after breaking with sliding.
void va3_friction_case() {
  const std::string id = "va3_friction_prescribed_quadrangle_4";
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  fs::path dat = fs::temp_directory_path() / ("cohesive_probe_" + id + ".dat");
  {
    std::ofstream out(dat);
    out << "model solid_mechanics_model_cohesive [\n"
        << "  cohesive_inserter [\n    cohesive_surfaces = [insertion]\n  ]\n"
        << "  material elastic [\n    name = body\n    rho = 1e3\n    E = 1e9\n    nu = 0.2\n  ]\n"
        << "  material cohesive_linear_friction [\n    name = insertion\n    beta = 1\n"
        << "    G_c = 10\n    sigma_c = 1e6\n    penalty = 1e12\n    mu = 0.3\n"
        << "    penalty_for_friction = 1e12\n    contact_after_breaking = true\n  ]\n]\n";
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  Mesh mesh(2);
  mesh.read((mdir / "_cohesive_2d_4_quadrangle_4.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  const Real stable_dt = model.getStableTimeStep();
  const Real dt = stable_dt * 0.05;
  model.setTimeStep(dt);
  auto & u = model.getDisplacement();
  const auto & X = mesh.getNodes();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    u(n, 0) = 0.;
    u(n, 1) = 2. * 1e6 / 1e9 * X(n, 1);
  }
  model.assembleInternalForces();
  const I64 nb_inserted = model.checkCohesiveStress();
  // nodes of the bulk elements above the interface (barycentre y > 0)
  std::vector<bool> top(mesh.getNbNodes(), false);
  for (auto type : mesh.elementTypes(2, _not_ghost, _ek_regular)) {
    const auto & conn = mesh.getConnectivity(type);
    const auto nn = conn.getNbComponent();
    for (Int e = 0; e < conn.size(); ++e) {
      Real yb = 0.;
      for (Int k = 0; k < nn; ++k) {
        yb += X(conn(e, k), 1);
      }
      if (yb > 0.) {
        for (Int k = 0; k < nn; ++k) {
          top[conn(e, k)] = true;
        }
      }
    }
  }
  model.getBlockedDOFs().set(true);
  // (dx, dy) path, delta_c,eff = 2 G_c / (2 sigma_c) = 1e-5
  std::vector<std::pair<Real, Real>> path;
  auto ramp = [&path](Real x1, Real y1, int n) {
    auto [x0, y0] = path.empty() ? std::pair<Real, Real>{0., 0.} : path.back();
    for (int k = 1; k <= n; ++k) {
      Real f = Real(k) / Real(n);
      path.emplace_back(x0 + (x1 - x0) * f, y0 + (y1 - y0) * f);
    }
  };
  ramp(0., 3e-6, 6);
  ramp(1e-7, 3e-6, 1);
  ramp(1e-7, -2e-6, 5);
  ramp(2e-6, -2e-6, 19);
  ramp(5e-7, -2e-6, 15);
  ramp(1.5e-6, -2e-6, 10);
  ramp(1.5e-6, 1.5e-5, 10);
  ramp(1.5e-6, -1e-6, 10);
  ramp(4e-6, -1e-6, 10);
  ap::FixtureWriter w(g_root, cid,
                      "cohesive_linear_friction under a prescribed rigid motion: contact with "
                      "stick, slip and reverse slip, failure, contact after breaking",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mdir / "_cohesive_2d_4_quadrangle_4.msh",
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "_cohesive_2d_4_quadrangle_4.msh");
  w.add_input_file("material", dat, "probe-defined (cohesive_linear_friction case)");
  record_pinning(w);
  auto & p = w.parameters();
  p["random_seed"] = 7LL;
  Json jp = Json::array();
  for (auto && [x, y] : path) {
    Json e = Json::array();
    e.push(x);
    e.push(y);
    jp.push(e);
  }
  p["path"] = jp;
  add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), 0.05 * it");
  add_ivec(w, "insertion/inserted", {nb_inserted}, "checkCohesiveStress() at 2 sigma_c");
  std::vector<double> edis, erev, econ, tr, ctr, ff, rs, op, dmg, dmx;
  auto & mat = model.getMaterial("insertion");
  for (auto && [x, y] : path) {
    for (Int n = 0; n < mesh.getNbNodes(); ++n) {
      u(n, 0) = top[n] ? x : 0.;
      u(n, 1) = top[n] ? y : 0.;
    }
    model.solveStep();
    edis.push_back(model.getEnergy("dissipated"));
    erev.push_back(model.getEnergy("reversible"));
    econ.push_back(model.getEnergy("cohesive contact"));
    auto q0 = [&](const std::string & name, std::vector<double> & out) {
      const auto & a = mat.getArray<Real>(name, _cohesive_2d_4);
      for (Int c = 0; c < a.getNbComponent(); ++c) {
        out.push_back(a(0, c));
      }
    };
    q0("tractions", tr);
    q0("contact_tractions", ctr);
    q0("friction_force", ff);
    q0("residual_sliding", rs);
    q0("opening", op);
    q0("damage", dmg);
    q0("delta max", dmx);
  }
  add_vec(w, "steps/energy/dissipated", edis, "getEnergy(dissipated) after each step");
  add_vec(w, "steps/energy/reversible", erev, "getEnergy(reversible) after each step");
  add_vec(w, "steps/energy/cohesive_contact", econ, "getEnergy(cohesive contact) after each step");
  add_vec(w, "steps/q0/tractions", tr, "tractions of quadrature point 0 (2 per step)");
  add_vec(w, "steps/q0/contact_tractions", ctr, "contact tractions of point 0 (2 per step)");
  add_vec(w, "steps/q0/friction_force", ff, "friction force of point 0 (2 per step)");
  add_vec(w, "steps/q0/residual_sliding", rs, "residual sliding of point 0");
  add_vec(w, "steps/q0/opening", op, "opening of point 0 (2 per step)");
  add_vec(w, "steps/q0/damage", dmg, "damage of point 0");
  add_vec(w, "steps/q0/delta_max", dmx, "delta max of point 0");
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  ElementCodec codec;
  record_topology(w, mesh, "", codec);
  record_cohesive_internals(w, model, "insertion", {"residual_sliding", "friction_force"});
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

// ---------------------------------------------------------------------------
// cmf_* cases.
// ---------------------------------------------------------------------------

/// cmf_friction_prescribed_tetrahedron_4: cohesive_linear_friction on the
/// frozen tet4 mesh (insertion surface y = 0, normal +-y), inserted by a
/// uniform tension of 2 sigma_c, then a prescribed rigid motion (dx, dy, dz)
/// of the upper half (every DOF blocked, explicit steps): partial opening,
/// penetration, sliding along x (stick, slip), along z with x held (a change
/// of the sliding direction, the norm grows), back along x (reverse stick,
/// reverse slip in the norm: UD-249), a smaller penetration with reverse
/// sliding, a diagonal reload, complete failure, contact after breaking with
/// sliding. Same material as va3_friction_prescribed_quadrangle_4.
void cmf_friction_3d_case() {
  const std::string id = "cmf_friction_prescribed_tetrahedron_4";
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path mdir = g_frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  fs::path dat = fs::temp_directory_path() / ("cohesive_probe_" + id + ".dat");
  {
    std::ofstream out(dat);
    out << "model solid_mechanics_model_cohesive [\n"
        << "  cohesive_inserter [\n    cohesive_surfaces = [insertion]\n  ]\n"
        << "  material elastic [\n    name = body\n    rho = 1e3\n    E = 1e9\n    nu = 0.2\n  ]\n"
        << "  material cohesive_linear_friction [\n    name = insertion\n    beta = 1\n"
        << "    G_c = 10\n    sigma_c = 1e6\n    penalty = 1e12\n    mu = 0.3\n"
        << "    penalty_for_friction = 1e12\n    contact_after_breaking = true\n  ]\n]\n";
  }
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  Mesh mesh(3);
  mesh.read((mdir / "_cohesive_3d_6_tetrahedron_4.msh").string());
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  const Real stable_dt = model.getStableTimeStep();
  const Real dt = stable_dt * 0.05;
  model.setTimeStep(dt);
  auto & u = model.getDisplacement();
  const auto & X = mesh.getNodes();
  for (Int n = 0; n < mesh.getNbNodes(); ++n) {
    u(n, 0) = 0.;
    u(n, 1) = 2. * 1e6 / 1e9 * X(n, 1);
    u(n, 2) = 0.;
  }
  model.assembleInternalForces();
  const I64 nb_inserted = model.checkCohesiveStress();
  // nodes of the bulk elements above the interface (barycentre y > 0)
  std::vector<bool> top(mesh.getNbNodes(), false);
  for (auto type : mesh.elementTypes(3, _not_ghost, _ek_regular)) {
    const auto & conn = mesh.getConnectivity(type);
    const auto nn = conn.getNbComponent();
    for (Int e = 0; e < conn.size(); ++e) {
      Real yb = 0.;
      for (Int k = 0; k < nn; ++k) {
        yb += X(conn(e, k), 1);
      }
      if (yb > 0.) {
        for (Int k = 0; k < nn; ++k) {
          top[conn(e, k)] = true;
        }
      }
    }
  }
  model.getBlockedDOFs().set(true);
  // (dx, dy, dz) path; delta_c,eff = 1e-5, tau_max = 0.3 * 1e12 * |dy|
  struct P3 {
    Real x, y, z;
  };
  std::vector<P3> path;
  auto ramp = [&path](Real x1, Real y1, Real z1, int n) {
    P3 a = path.empty() ? P3{0., 0., 0.} : path.back();
    for (int k = 1; k <= n; ++k) {
      Real f = Real(k) / Real(n);
      path.push_back({a.x + (x1 - a.x) * f, a.y + (y1 - a.y) * f, a.z + (z1 - a.z) * f});
    }
  };
  ramp(0., 3e-6, 0., 6);
  ramp(1e-7, 3e-6, 0., 1);
  ramp(1e-7, -2e-6, 0., 5);
  ramp(2e-6, -2e-6, 0., 19);
  ramp(2e-6, -2e-6, 1.5e-6, 12);
  ramp(6e-7, -2e-6, 1.5e-6, 14);
  ramp(6e-7, -1.2e-6, 4e-7, 8);
  ramp(1.6e-6, -1.2e-6, 1.2e-6, 10);
  ramp(1.6e-6, 1.5e-5, 1.2e-6, 10);
  ramp(1.6e-6, -1e-6, 1.2e-6, 10);
  ramp(4e-6, -1e-6, 3e-6, 10);
  ap::FixtureWriter w(g_root, cid,
                      "cohesive_linear_friction in 3-D under a prescribed rigid motion: contact "
                      "with stick, slip, a change of the sliding direction, reverse stick and "
                      "reverse slip, failure, contact after breaking",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mdir / "_cohesive_3d_6_tetrahedron_4.msh",
                   "frozen:test/test_model/test_solid_mechanics_model/test_cohesive/"
                   "_cohesive_3d_6_tetrahedron_4.msh");
  w.add_input_file("material", dat, "probe-defined (3-D cohesive_linear_friction case)");
  record_pinning(w);
  auto & p = w.parameters();
  p["random_seed"] = 7LL;
  Json jp = Json::array();
  for (auto && q : path) {
    Json e = Json::array();
    e.push(q.x);
    e.push(q.y);
    e.push(q.z);
    jp.push(e);
  }
  p["path"] = jp;
  add_vec(w, "time_step", {stable_dt, dt}, "getStableTimeStep(), 0.05 * it");
  add_ivec(w, "insertion/inserted", {nb_inserted}, "checkCohesiveStress() at 2 sigma_c");
  std::vector<double> edis, erev, econ, tr, ctr, ff, rs, op, dmg, dmx;
  auto & mat = model.getMaterial("insertion");
  for (auto && q : path) {
    for (Int n = 0; n < mesh.getNbNodes(); ++n) {
      u(n, 0) = top[n] ? q.x : 0.;
      u(n, 1) = top[n] ? q.y : 0.;
      u(n, 2) = top[n] ? q.z : 0.;
    }
    model.solveStep();
    edis.push_back(model.getEnergy("dissipated"));
    erev.push_back(model.getEnergy("reversible"));
    econ.push_back(model.getEnergy("cohesive contact"));
    auto q0 = [&](const std::string & name, std::vector<double> & out) {
      const auto & a = mat.getArray<Real>(name, _cohesive_3d_6);
      for (Int c = 0; c < a.getNbComponent(); ++c) {
        out.push_back(a(0, c));
      }
    };
    q0("tractions", tr);
    q0("contact_tractions", ctr);
    q0("friction_force", ff);
    q0("residual_sliding", rs);
    q0("opening", op);
    q0("damage", dmg);
    q0("delta max", dmx);
  }
  add_vec(w, "steps/energy/dissipated", edis, "getEnergy(dissipated) after each step");
  add_vec(w, "steps/energy/reversible", erev, "getEnergy(reversible) after each step");
  add_vec(w, "steps/energy/cohesive_contact", econ, "getEnergy(cohesive contact) after each step");
  add_vec(w, "steps/q0/tractions", tr, "tractions of quadrature point 0 (3 per step)");
  add_vec(w, "steps/q0/contact_tractions", ctr, "contact tractions of point 0 (3 per step)");
  add_vec(w, "steps/q0/friction_force", ff, "friction force of point 0 (3 per step)");
  add_vec(w, "steps/q0/residual_sliding", rs, "residual sliding of point 0");
  add_vec(w, "steps/q0/opening", op, "opening of point 0 (3 per step)");
  add_vec(w, "steps/q0/damage", dmg, "damage of point 0");
  add_vec(w, "steps/q0/delta_max", dmx, "delta max of point 0");
  add_array(w, "displacement", model.getDisplacement(), "final displacement");
  add_array(w, "internal_force", model.getInternalForce(), "final internal force");
  ElementCodec codec;
  record_topology(w, mesh, "", codec);
  record_cohesive_internals(w, model, "insertion", {"residual_sliding", "friction_force"});
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  fs::remove(dat);
  std::cout << "  " << cid << std::endl;
}

// ---------------------------------------------------------------------------
// v92_ig_tg_zones_update_after_insertion, a zone-update probe as an oracle
// case (UD-808).
// ---------------------------------------------------------------------------

/// The upstream example cohesive_extrinsic_ig_tg as written, with
/// updateAutomaticInsertion() before any insertion (succeeds) and after the
/// step of the first insertion: upstream's limitCheckFacets reads
/// physical_names of the cohesive neighbour of the doubled facets and
/// throws; the message is the golden result (`upstream_error`), with the
/// state up to it.
void v92_zone_update_case() {
  const std::string id = "v92_ig_tg_zones_update_after_insertion";
  const std::string cid = "model/cohesive/" + id;
  if (not wanted(id)) {
    return;
  }
  const fs::path mfile =
      g_frozen / "examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/square.msh";
  fs::path tmp_dat;
  const fs::path dat = ig_tg_material(id, IgTgVariant{}, tmp_dat);
  getStaticParser().parse(dat.string());
  RandomGenerator<Idx>::seed(7);
  const Int spatial_dimension = 2;
  const Int max_steps = 1000;
  Mesh mesh(spatial_dimension);
  mesh.read(mfile.string());
  SolidMechanicsModelCohesive model(mesh);
  MaterialCohesiveRules rules = ig_tg_rules(IgTgVariant{});
  auto cohesive_material_selector = std::make_shared<MaterialCohesiveRulesSelector>(model, rules);
  auto bulk_material_selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  auto && current_selector = model.getMaterialSelector();
  cohesive_material_selector->setFallback(bulk_material_selector);
  bulk_material_selector->setFallback(current_selector);
  model.setMaterialSelector(cohesive_material_selector);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  const Real stable_dt = model.getStableTimeStep();
  Real time_step = stable_dt * 0.05;
  model.setTimeStep(time_step);
  model.assembleMassLumped();

  ap::FixtureWriter w(g_root, cid,
                      "upstream example cohesive_extrinsic_ig_tg: updateAutomaticInsertion "
                      "before and after the first insertion (cohesive_zones; review V92 CG-01)",
                      kProbe, kProbeVersion);
  w.add_tag("model");
  w.add_tag("cohesive");
  w.add_input_file("mesh", mfile,
                   "frozen:examples/c++/solid_mechanics_cohesive_model/cohesive_extrinsic_ig_tg/"
                   "square.msh");
  w.add_input_file("material", dat,
                   "upstream:examples/c++/solid_mechanics_cohesive_model/"
                   "cohesive_extrinsic_ig_tg/material.dat");
  record_pinning(w);
  auto & p = w.parameters();
  p["random_seed"] = 7LL;
  p["max_steps"] = static_cast<long long>(max_steps);
  add_vec(w, "time_step", {stable_dt, time_step}, "getStableTimeStep(), 0.05 * it");
  record_flags(w, model, "init/");
  // before any insertion: succeeds
  model.updateAutomaticInsertion();
  p["update_before_insertion"] = std::string("ok");
  record_flags(w, model, "update_before/");

  auto & position = mesh.getNodes();
  auto & velocity = model.getVelocity();
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "top");
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "right");
  Real loading_rate = 0.1;
  Real VI = loading_rate * 2 * 0.5;
  p["loading_rate"] = loading_rate;
  for (auto && [pos, vel] :
       zip(make_view(position, spatial_dimension), make_view(velocity, spatial_dimension))) {
    vel = loading_rate * pos;
  }
  IgTgVelocity vely(model, VI, _y);
  IgTgVelocity velx(model, VI, _x);
  std::vector<I64> inserted, nb_cohesive;
  std::vector<double> ekin, epot;
  Int s = 1;
  for (; s <= max_steps; ++s) {
    model.applyBC(vely, "top");
    model.applyBC(vely, "bottom");
    model.applyBC(velx, "left");
    model.applyBC(velx, "right");
    I64 n = model.checkCohesiveStress();
    inserted.push_back(n);
    nb_cohesive.push_back(I64(mesh.getNbElement(_cohesive_2d_4)));
    model.solveStep();
    ekin.push_back(model.getEnergy("kinetic"));
    epot.push_back(model.getEnergy("potential"));
    if (n > 0) {
      break;
    }
  }
  p["first_insertion_step"] = static_cast<long long>(s);
  p["physical_names_has_cohesive_type"] =
      mesh.getData<std::string>("physical_names").exists(_cohesive_2d_4, _not_ghost);
  add_ivec(w, "steps/inserted", inserted, "checkCohesiveStress() return value per step");
  add_ivec(w, "steps/nb_cohesive", nb_cohesive, "cohesive elements after the check");
  add_vec(w, "steps/energy/kinetic", ekin, "getEnergy(kinetic) after each solveStep");
  add_vec(w, "steps/energy/potential", epot, "getEnergy(potential) after each solveStep");
  record_nodal_state(w, model, "after_insertion/");
  record_flags(w, model, "after_insertion/");
  ElementCodec codec;
  record_topology(w, mesh, "", codec);
  // after the first insertion: upstream throws in limitCheckFacets
  std::string error;
  try {
    model.updateAutomaticInsertion();
  } catch (std::exception & e) {
    error = e.what();
  }
  if (error.empty()) {
    throw std::runtime_error(id + ": updateAutomaticInsertion did not throw");
  }
  p["upstream_error"] = error;
  // the flags as the exception left them (limitCheckFacets modifies them in
  // place up to the facet that throws)
  record_flags(w, model, "after_error/");
  Json names = Json::array();
  for (auto & n : codec.names) {
    names.push(n);
  }
  p["element_type_names"] = names;
  w.finish();
  if (not tmp_dat.empty()) {
    fs::remove(tmp_dat);
  }
  std::cout << "  " << cid << " (upstream error: " << error << ")" << std::endl;
}

} // namespace

int main(int argc, char * argv[]) {
  if (argc < 4) {
    std::cerr << "usage: cohesive_probe <fixtures_root> <upstream_src_root> <frozen_meshes_root>\n";
    return 2;
  }
  // D-050: host-independent Eigen blocking (Eigen's own defaults; recorded
  // in the provenance of the va3_* cases)
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  g_root = argv[1];
  g_up = argv[2];
  g_frozen = argv[3];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    struct T {
      ElementType ct;
      std::string mesh, types;
    };
    const std::vector<T> types = {
        {_cohesive_1d_2, "_cohesive_1d_2_segment_2.msh", "segment_2"},
        {_cohesive_2d_4, "_cohesive_2d_4_triangle_3.msh", "triangle_3"},
        {_cohesive_2d_4, "_cohesive_2d_4_quadrangle_4.msh", "quadrangle_4"},
        {_cohesive_2d_4, "_cohesive_2d_4_triangle_3_quadrangle_4.msh", "triangle_3_quadrangle_4"},
        {_cohesive_2d_6, "_cohesive_2d_6_triangle_6.msh", "triangle_6"},
        {_cohesive_2d_6, "_cohesive_2d_6_quadrangle_8.msh", "quadrangle_8"},
        {_cohesive_2d_6, "_cohesive_2d_6_triangle_6_quadrangle_8.msh", "triangle_6_quadrangle_8"},
        {_cohesive_3d_6, "_cohesive_3d_6_tetrahedron_4.msh", "tetrahedron_4"},
        {_cohesive_3d_12, "_cohesive_3d_12_tetrahedron_10.msh", "tetrahedron_10"}};
    for (const auto & t : types) {
      for (bool extrinsic : {true, false}) {
        for (bool mode_ii : {false, true}) {
          FixtureSpec s{t.ct, t.mesh, t.types, extrinsic, mode_ii,
                        extrinsic ? "material_0.dat" : "material_1.dat",
                        std::string("fixture_") + (extrinsic ? "extrinsic" : "intrinsic") +
                            "_mode_" + (mode_ii ? "ii" : "i") + "_" + t.types};
          fixture_case(s);
        }
      }
    }
    FixtureSpec r{_cohesive_2d_4, "_cohesive_2d_4_triangle_3.msh", "triangle_3", true, false,
                  "material_0.dat", "random_extrinsic_mode_i_triangle_3", true};
    fixture_case(r);
    const std::vector<std::pair<std::string, std::string>> laws = {
        {"linear_fatigue",
         "  material cohesive_linear_fatigue [\n    name = insertion\n    beta = 1\n    G_c = 10\n"
         "    sigma_c = 1e6\n    delta_f = 4e-5\n    fatigue_ratio = 0.5\n  ]"},
        {"linear_friction",
         "  material cohesive_linear_friction [\n    name = insertion\n    beta = 1\n    G_c = 10\n"
         "    sigma_c = 1e6\n    penalty = 1e12\n    mu = 0.3\n    penalty_for_friction = 1e10\n"
         "    contact_after_breaking = true\n  ]"},
        {"linear_uncoupled",
         "  material cohesive_linear_uncoupled [\n    name = insertion\n    beta = 1\n    G_c = 10\n"
         "    sigma_c = 1e6\n    roughness = 0.8\n  ]"},
        {"linear_kappa",
         "  material cohesive_linear [\n    name = insertion\n    beta = 1\n    kappa = 2\n    G_c = 10\n"
         "    sigma_c = 1e6\n    penalty = 1e12\n    max_quad_stress_insertion = true\n  ]"}};
    for (const auto & [law, text] : laws) {
      for (bool mode_ii : {false, true}) {
        for (const auto & t : std::vector<T>{types[1], types[7]}) {
          FixtureSpec ls{t.ct, t.mesh, t.types, true, mode_ii, "material_0.dat",
                         "law_" + law + "_mode_" + (mode_ii ? "ii" : "i") + "_" + t.types};
          ls.law_text = text;
          if (not wanted(ls.id)) {
            continue;
          }
          try {
            fixture_case(ls);
          } catch (std::exception & e) {
            error_case(ls.id, "TestSMMCFixture scenario with cohesive_" + law, e.what());
          }
        }
      }
    }
    // ExtrinsicModeIFiniteDef on every type of dimension >= 2
    for (const auto & t : types) {
      if (Mesh::getSpatialDimension(t.ct) < 2) {
        continue;
      }
      FixtureSpec s{t.ct, t.mesh, t.types, true, false, "material_0_finite_def.dat",
                    "fixture_extrinsic_mode_i_finite_def_" + t.types};
      fixture_case(s);
    }
    // Adversarial scenarios
    {
      const std::string body = "  material elastic [\n    name = body\n    rho = 1e3\n"
                               "    E = 1e9\n    nu = 0.2\n  ]\n";
      const Real nu = 0.2;
      ExplicitSpec a;
      a.id = "adv_a_tet4_multilaw_mixed";
      a.desc = "3-D mixed mode, every facet checkable, two cohesive laws chosen by "
               "MeshDataMaterialCohesiveSelector (review V48 case A)";
      a.cohesive_type = _cohesive_3d_6;
      a.mesh = "_cohesive_3d_6_tetrahedron_4.msh";
      a.dat = "model solid_mechanics_model_cohesive [\n" + body +
              "  material cohesive_linear [\n    name = coh_default\n    beta = 1\n"
              "    kappa = 2\n    G_c = 10\n    sigma_c = 1e6 uniform [-2e5, 2e5]\n"
              "    penalty = 1e12\n  ]\n"
              "  material cohesive_linear_uncoupled [\n    name = insertion\n    beta = 1\n"
              "    G_c = 10\n    sigma_c = 0.8e6\n    roughness = 0.8\n  ]\n]\n";
      a.mesh_data_selector = true;
      a.strain_rows = {-nu, 0.5, 0., 0., 1., 0., 0., 0., -nu};
      a.strain_scale = 1e6 / 1e9;
      a.init_factor = 0.75;
      a.inc_factor = 0.3 / 200.;
      a.schedule = {{1., 200}};
      a.dt_factor = 0.05;
      a.seed = 11;
      a.laws = {"coh_default", "insertion"};
      explicit_case(a);
      ExplicitSpec b;
      b.id = "adv_b_tri6_cyclic";
      b.desc = "2-D tri6, every facet checkable, random sigma_c, contact after breaking, "
               "loading / unloading into compression / reloading (review V48 case B)";
      b.cohesive_type = _cohesive_2d_6;
      b.mesh = "_cohesive_2d_6_triangle_6.msh";
      b.dat = "model solid_mechanics_model_cohesive [\n" + body +
              "  material cohesive_linear [\n    name = coh\n    beta = 1\n    kappa = 2\n"
              "    G_c = 10\n    sigma_c = 1e6 uniform [-1e5, 1e5]\n    penalty = 1e11\n"
              "    contact_after_breaking = true\n  ]\n]\n";
      b.strain_rows = {-nu, 0.3, 0., 1. - nu};
      b.strain_scale = (1. + nu) * 1e6 / 1e9;
      b.init_factor = 0.85;
      b.inc_factor = 0.25 / 120.;
      b.schedule = {{1., 120}, {-2., 120}, {2., 120}};
      b.dt_factor = 0.05;
      b.seed = 5;
      b.laws = {"coh"};
      explicit_case(b);
    }
    implicit_mixed_case("adv_c_implicit_mixed_linear_kappa2",
                        "  material cohesive_linear [\n    name = cohesive\n    sigma_c = 1\n"
                        "    delta_c = 0.5\n    beta = 1\n    kappa = 2\n    penalty = 10\n  ]\n",
                        0.5, 200);
    implicit_mixed_case("adv_c_implicit_mixed_bilinear_kappa2",
                        "  material cohesive_bilinear [\n    name = cohesive\n    sigma_c = 1\n"
                        "    G_c = 0.25\n    delta_0 = 0.01\n    beta = 1\n    kappa = 2\n"
                        "    penalty = 10\n  ]\n",
                        0.3, 100);
    implicit_case("linear", "    delta_c = 0.5\n    G_c = 0.25\n    penalty = 10\n");
    implicit_case("bilinear", "    G_c = 0.25\n    delta_0 = 0.01\n    penalty = 10\n");
    implicit_case("exponential", "    delta_c = 0.5\n");
    v57_case("adv_v57_implicit_bilinear_contact_reversal",
             "  material cohesive_bilinear [\n    name = cohesive\n    sigma_c = 1\n"
             "    G_c = 0.25\n    delta_0 = 0.01\n    beta = 1\n    kappa = 2\n"
             "    penalty = 10\n  ]\n",
             0.3, {{1., 30}, {-1., 50}, {1., 60}}, -1, -1);
    v57_case("adv_v57_implicit_bilinear_standalone_retry",
             "  material cohesive_bilinear [\n    name = cohesive\n    sigma_c = 1\n"
             "    G_c = 0.25\n    delta_0 = 0.01\n    beta = 1\n    kappa = 2\n"
             "    penalty = 10\n  ]\n",
             0.3, {{1., 45}}, 15, 28);
    buildfragments_case();
    // verification audit gaps
    va3_facets_only_case();
    va3_ig_tg_case("va3_ig_tg_rules_zones", IgTgVariant{});
    va3_ig_tg_case("va3_ig_tg_zone_btop", IgTgVariant{true});
    va3_friction_case();
    v92_zone_update_case();
    {
      IgTgVariant reversed;
      reversed.ig_first = true;
      reversed.desc = "upstream example cohesive_extrinsic_ig_tg with ig_cohesive listed "
                      "first (default cohesive law ig_cohesive; the btop-bbottom facets get "
                      "tg_cohesive only by the reversed rule key)";
      va3_ig_tg_case("v92_ig_tg_rules_reversed_key", reversed);
      IgTgVariant both;
      both.ig_first = true;
      both.rules = {{"btop", "bbottom", "ig_cohesive"},
                    {"bbottom", "btop", "tg_cohesive"},
                    {"btop", "btop", "ig_cohesive"},
                    {"bbottom", "bbottom", "ig_cohesive"}};
      both.desc = "upstream example cohesive_extrinsic_ig_tg with ig_cohesive listed first "
                  "and both keys of the interface pair mapped to different laws "
                  "((btop, bbottom) -> ig, (bbottom, btop) -> tg): the lookup order";
      va3_ig_tg_case("v92_ig_tg_rules_both_keys", both);
    }
    v57_case("v92_implicit_uncoupled_prestressed_reversal",
             "  material cohesive_linear_uncoupled [\n    name = cohesive\n    sigma_c = 1\n"
             "    G_c = 1.2\n    beta = 1\n    kappa = 2\n    roughness = 0.8\n"
             "    penalty = 10\n  ]\n",
             0., {{1., 30}, {-1., 66}, {1., 40}, {-1., 60}}, -1, -1,
             V57Options{0.2, {"delta_n_max", "delta_t_max", "damage_n", "damage_t"},
                        "implicit static cohesive_linear_uncoupled (review V92 G92-05): "
                        "extrinsic insertion by checkCohesiveStress at a uniform pre-strain "
                        "before the first solve, then mode-I loading, unloading into "
                        "compression (penalty contact), reloading below the previous "
                        "maximum and unloading into compression again: the final state "
                        "penetrates with partial shear damage, where upstream's tangent "
                        "differs from the derivative of the traction (UD-244) (Eigen)"});
    cmf_friction_3d_case();
    {
      V57Options o;
      o.prestress_strain = 0.2;
      o.extra_internals = {"residual_sliding", "friction_force"};
      o.delta_max_history = true;
      o.interface_free_x = true;
      o.material_source = "probe-defined (implicit cohesive_linear_friction case)";
      o.desc = "implicit static cohesive_linear_friction: extrinsic insertion by "
               "checkCohesiveStress at a uniform "
               "pre-strain before the first solve, the history of delta max activated by "
               "the probe after initFull (UD-028 A), x free at the interface, then mode-I "
               "opening, closing into "
               "penalty contact, sliding forward, back and forward again at constant top "
               "height (stick, slip, reverse stick, reverse slip: the friction tangent, "
               "UD-247, and the residual sliding update, UD-249) and reopening (Eigen)";
      v57_case("cmf_implicit_friction_prestressed_sliding", CMF_IMPLICIT_LAW, 0.,
               CMF_IMPLICIT_SCHEDULE, -1, -1, o);
    }
  } catch (std::exception & e) {
    std::cerr << "cohesive_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
