// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records the non-local toolbox and the non-local damage
// laws of the reference for the Rust differential tests
// (fixtures/model/non_local/*). The test model `MyModel` and the averaging
// test law follow upstream's test/test_model/test_common/
// test_non_local_toolbox/{my_model.hh, test_material_damage.{hh,cc}}
// (LGPL-3.0-or-later, (c) EPFL/LSMS).
//
// Cases (fixture area model, case ids model/non_local/<case>, format v1):
//   manager_plate_r<radius>
//       upstream MyModel (every point of plate.msh inserted in neighbourhood
//       test_region with the null element index and a running number), base_wf:
//       pair lists (owned list; q1/q2 type, element, num_point, global_num),
//       pair weights (bit-exact, read from the private `pair_weight` with the
//       explicit-instantiation member-access idiom), volumes, jacobians.
//   avg_<mesh>_<wf>
//       SolidMechanicsModel with the averaging test law (`grad_u` →
//       `grad_u non local`, neighbourhood = law name; plate.msh: two laws
//       by physical names), eigen_grad_u = a polynomial of the integration
//       point coordinates (no transcendental function), damage set from the
//       point index before the second averaging; two assembleInternalForces
//       calls (weights recomputed at calls 0 and 1, update_rate 1). Per call:
//       pairs, weights, volumes and every law's `grad_u non local`.
//   law_<keyword>[_<variant>]
//       mazars_non_local (average on Ehat / on damage), plastic_mazars_non_local,
//       and upstream's MaterialMarigoNonLocal (registered here under
//       `marigo_non_local_class`: the keyword `marigo_non_local` allocates the
//       local law, UD-027) on plate.msh, base_wf radius 0.6; a prescribed
//       displacement history (14 steps: loading then unloading) with
//       beforeSolveStep / assembleInternalForces / afterSolveStep(true) per
//       step. Per step: damage, stress, the averaged and the local variable,
//       internal force (and inelastic_strain for the plastic parent).
//   weight_rate_sweep (version 2)
//       upstream's RemoveDamagedWithDamageRateWeightFunction (header-only,
//       registered nowhere upstream, so this TU compiled with the oracle's
//       flags is its only compilation) evaluated through its own
//       operator()(r, q1, q2) over a sweep of radii and distances
//       (0 ... 1.5 R in steps of R/200, plus perturbed values) and four damage rates (below,
//       at and around the default damage_limit 1): the clamp
//       std::max(0., 1. - pow(r*r/R2, 1.)) and pow(alpha, 2.). Black-box
//       diagnostics (D-030: no libm source read): the same alpha through a
//       non-foldable libm pow(alpha, 2.) / pow(x, 1.) call, counted where it
//       differs from alpha*alpha / x.
//
// NL_PROBE_CASES=<id>[,<id>...] restricts the run to the listed cases among
// those that support filtering (today weight_rate_sweep); when it is set the
// version-1 cases are skipped.
//
// Floating point: Eigen cache sizes pinned (D-050) before anything else.

#include "akantu_probe/fixture_writer.hh"

#include "aka_common.hh"
#include "base_weight_function.hh"
#include "communicator.hh"
#include "damaged_weight_function.hh"
#include "integrator_gauss.hh"
#include "material_damage.hh"
#include "material_damage_non_local.hh"
#include "material_marigo_non_local.hh"
#include "material_selector.hh"
#include "mesh.hh"
#include "model.hh"
#include "non_local_manager.hh"
#include "non_local_manager_callback.hh"
#include "non_local_neighborhood.hh"
#include "remove_damaged_weight_function.hh"
#include "remove_damaged_with_damage_rate_weight_function.hh"
#include "shape_lagrange.hh"
#include "solid_mechanics_model.hh"

#include <Eigen/Core>

#include <algorithm>
#include <cmath>
#include <cstdlib>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check
// (added with version 2, the fixture writer refuses to write
// without it).
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
using akantu_probe::FixtureWriter;
using akantu_probe::Json;
namespace fs = std::filesystem;

namespace {

const char * const PROBE = "oracle/probes/non_local/non_local_probe.cc";
const char * const PROBE_VERSION = "2";

/* -------------------------------------------------------------------------- */
/* Access to NonLocalNeighborhood<WF>::pair_weight (private, no accessor but  */
/* the 6-digit saveWeights): explicit instantiation may name private members. */
/* -------------------------------------------------------------------------- */
template <class Tag, typename Tag::type M> struct Rob {
  friend typename Tag::type get(Tag) { return M; }
};
template <class WF> struct PairWeightTag {
  using type =
      std::map<GhostType, std::unique_ptr<Array<Real>>> NonLocalNeighborhood<WF>::*;
  friend type get(PairWeightTag);
};
template struct Rob<PairWeightTag<BaseWeightFunction>,
                    &NonLocalNeighborhood<BaseWeightFunction>::pair_weight>;
template struct Rob<PairWeightTag<RemoveDamagedWeightFunction>,
                    &NonLocalNeighborhood<RemoveDamagedWeightFunction>::pair_weight>;
template struct Rob<PairWeightTag<DamagedWeightFunction>,
                    &NonLocalNeighborhood<DamagedWeightFunction>::pair_weight>;

template <class WF>
const Array<Real> * weights_of(NonLocalNeighborhoodBase & nb, GhostType gt) {
  auto * n = dynamic_cast<NonLocalNeighborhood<WF> *>(&nb);
  if (n == nullptr) {
    return nullptr;
  }
  auto & m = n->*get(PairWeightTag<WF>{});
  auto it = m.find(gt);
  return it == m.end() ? nullptr : it->second.get();
}

const Array<Real> & pair_weights(NonLocalNeighborhoodBase & nb, GhostType gt) {
  if (auto * w = weights_of<BaseWeightFunction>(nb, gt)) {
    return *w;
  }
  if (auto * w = weights_of<RemoveDamagedWeightFunction>(nb, gt)) {
    return *w;
  }
  if (auto * w = weights_of<DamagedWeightFunction>(nb, gt)) {
    return *w;
  }
  throw std::runtime_error("unknown weight function type");
}

/* -------------------------------------------------------------------------- */
/* Upstream MyModel (test/test_model/test_common/test_non_local_toolbox/     */
/* my_model.hh)                                                               */
/* -------------------------------------------------------------------------- */
class MyModel : public Model, public NonLocalManagerCallback {
  using MyFEEngineType = FEEngineTemplate<IntegratorGauss, ShapeLagrange>;

public:
  MyModel(Mesh & mesh, Int spatial_dimension)
      : Model(mesh, ModelType::_model, spatial_dimension),
        manager(*this, *this) {
    registerFEEngineObject<MyFEEngineType>("FEEngine", mesh, spatial_dimension);
    manager.registerNeighborhood("test_region", "test_region");
    getFEEngine().initShapeFunctions();
    manager.initialize();
  }
  void initModel() override {}
  MatrixType getMatrixType(const ID &) const override { return _mt_not_defined; }
  std::tuple<ID, TimeStepSolverType>
  getDefaultSolverID(const AnalysisMethod & /*method*/) override {
    return std::make_tuple("test", TimeStepSolverType::_static);
  }
  void assembleMatrix(const ID &) override {}
  void assembleLumpedMatrix(const ID &) override {}
  void assembleResidual() override {}
  void onNodesAdded(const Array<Idx> &, const NewNodesEvent &) override {}
  void onNodesRemoved(const Array<Idx> &, const Array<Idx> &,
                      const RemovedNodesEvent &) override {}
  void onElementsAdded(const Array<Element> &, const NewElementsEvent &) override {}
  void onElementsRemoved(const Array<Element> &, const ElementTypeMapArray<Idx> &,
                         const RemovedElementsEvent &) override {}
  void onElementsChanged(const Array<Element> &, const Array<Element> &,
                         const ElementTypeMapArray<Idx> &,
                         const ChangedElementsEvent &) override {}
  void insertIntegrationPointsInNeighborhoods(GhostType ghost_type) override {
    ElementTypeMapArray<Real> quadrature_points_coordinates(
        "quadrature_points_coordinates_tmp_nl", this->id);
    quadrature_points_coordinates.initialize(this->getFEEngine(),
                                             _nb_component = spatial_dimension,
                                             _ghost_type = ghost_type);
    IntegrationPoint q;
    q.ghost_type = ghost_type;
    q.global_num = 0;
    auto & neighborhood = manager.getNeighborhood("test_region");
    for (const auto & type : quadrature_points_coordinates.elementTypes(
             spatial_dimension, ghost_type)) {
      q.type = type;
      auto & quads = quadrature_points_coordinates(type, ghost_type);
      this->getFEEngine().computeIntegrationPointsCoordinates(quads, type,
                                                              ghost_type);
      auto quad_it = quads.cbegin(quads.getNbComponent());
      auto quad_end = quads.cend(quads.getNbComponent());
      q.num_point = 0;
      for (; quad_it != quad_end; ++quad_it) {
        neighborhood.insertIntegrationPoint(q, *quad_it);
        ++q.num_point;
        ++q.global_num;
      }
    }
  }
  void computeNonLocalContribution(GhostType) override {}
  void updateLocalInternal(ElementTypeMapReal &, GhostType, ElementKind) override {}
  void updateNonLocalInternal(ElementTypeMapReal &, GhostType, ElementKind) override {}
  NonLocalManager & getNonLocalManager() { return manager; }

private:
  NonLocalManager manager;
};

/* -------------------------------------------------------------------------- */
/* Averaging test law (upstream TestMaterialDamage): grad_u -> grad_u non    */
/* local in the neighbourhood of the law's name, no non-local stress.        */
/* -------------------------------------------------------------------------- */
template <Int dim>
class ProbeAvgMaterial
    : public MaterialDamageNonLocal<dim, MaterialDamage<dim, MaterialElastic>> {
  using Parent = MaterialDamageNonLocal<dim, MaterialDamage<dim, MaterialElastic>>;

public:
  ProbeAvgMaterial(SolidMechanicsModel & model, const ID & id)
      : Parent(model, id),
        grad_u_nl(this->registerInternal("grad_u non local", dim * dim)) {
    this->is_non_local = true;
  }
  void registerNonLocalVariables() final {
    this->getModel().getNonLocalManager().registerNonLocalVariable(
        this->gradu.getName(), grad_u_nl.getName(), dim * dim);
    this->getModel()
        .getNonLocalManager()
        .getNeighborhood(this->getNeighborhoodName())
        .registerNonLocalVariable(grad_u_nl.getName());
  }
  void computeNonLocalStress(ElementType, GhostType) final {}

private:
  InternalField<Real> & grad_u_nl;
};

const bool probe_avg_registered [[maybe_unused]] =
    instantiateMaterial<ProbeAvgMaterial>("probe_avg_material");
// UD-027: upstream's real non-local Marigo class under a probe keyword
const bool marigo_nl_registered [[maybe_unused]] =
    instantiateMaterial<MaterialMarigoNonLocal>("marigo_non_local_class");

/* -------------------------------------------------------------------------- */
/* Recording helpers                                                          */
/* -------------------------------------------------------------------------- */
std::vector<std::string> type_names() {
  std::vector<std::string> names;
  for (auto t : {_triangle_3, _quadrangle_4, _hexahedron_8, _tetrahedron_4}) {
    std::stringstream s;
    s << t;
    names.push_back(s.str());
  }
  return names;
}

std::int64_t type_code(ElementType t) {
  switch (t) {
  case _triangle_3:
    return 0;
  case _quadrangle_4:
    return 1;
  case _hexahedron_8:
    return 2;
  case _tetrahedron_4:
    return 3;
  default:
    throw std::runtime_error("unexpected element type in the probe");
  }
}

void record_pairs(FixtureWriter & w, const std::string & prefix,
                  NonLocalNeighborhoodBase & nb) {
  for (auto gt : ghost_types) {
    const auto & pairs = nb.getPairLists(gt);
    std::string g = gt == _not_ghost ? "not_ghost" : "ghost";
    std::vector<std::int64_t> p;
    for (const auto & [q1, q2] : pairs) {
      for (const auto * q : {&q1, &q2}) {
        p.push_back(type_code(q->type));
        p.push_back(q->element);
        p.push_back(q->num_point);
        p.push_back(q->global_num);
      }
    }
    w.add<std::int64_t>(prefix + "pairs_" + g, {pairs.size(), 8}, {"entry", "component"},
                     "table", p)
        .description("pair (q1, q2): type code (parameters.type_names), element, "
                     "num_point, global_num of q1 then q2, in upstream pair order");
    const auto & weights = pair_weights(nb, gt);
    std::vector<double> wv(weights.data(),
                           weights.data() + weights.size() * weights.getNbComponent());
    w.add<double>(prefix + "weights_" + g, {std::size_t(weights.size()), 2}, {"entry", "component"},
                  "table", wv)
        .description("pair weights [w(q1<-q2), w(q2<-q1)] (upstream pair_weight)");
  }
}

void record_map(FixtureWriter & w, const std::string & name,
                const ElementTypeMapArray<Real> & map, Int dim,
                GhostType gt = _not_ghost) {
  for (auto type : map.elementTypes(dim, gt)) {
    const auto & a = map(type, gt);
    std::stringstream s;
    s << type;
    std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
    w.add<double>(name + "/" + s.str(), {std::size_t(a.size()), std::size_t(a.getNbComponent())},
                  {"entry", "component"}, "table", v)
        .element_type(s.str());
  }
}

void write_dat(const fs::path & p, const std::string & text) {
  std::ofstream f(p);
  f << text;
}

/* -------------------------------------------------------------------------- */
void case_manager(const fs::path & out, const fs::path & inputs, const fs::path & tmp,
                  const std::string & radius) {
  auto dat = tmp / ("manager_" + radius + ".dat");
  write_dat(dat, "non_local test_region base_wf [\n   radius = " + radius +
                     "\n   weight_function weight_parameter [\n      update_rate = 1\n   ]\n]\n");
  getStaticParser().parse(dat.string());
  std::string id = radius;
  std::replace(id.begin(), id.end(), '.', 'p');
  FixtureWriter w(out, "model/non_local/manager_plate_r" + id,
                  "upstream MyModel on plate.msh, base_wf radius " + radius, PROBE,
                  PROBE_VERSION);
  w.add_input_file("mesh", inputs / "plate.msh",
                   "test/test_model/test_common/test_non_local_toolbox/plate.msh");
  w.add_input_file("material", dat);
  w.parameters()["type_names"] = Json::array_of(type_names());
  w.parameters()["radius"] = std::stod(radius);
  Mesh mesh(2);
  mesh.read((inputs / "plate.msh").string());
  MyModel model(mesh, 2);
  auto & manager = model.getNonLocalManager();
  record_pairs(w, "", manager.getNeighborhood("test_region"));
  record_map(w, "volumes", manager.getVolumes(), 2);
  record_map(w, "jacobians", manager.getJacobians(), 2);
  w.finish();
}

/* -------------------------------------------------------------------------- */
/// eigen_grad_u of point (x, y[, z]): a fixed polynomial (column-major d x d)
Matrix<Real> eigen_of(const Vector<Real> & x, Int dim) {
  Matrix<Real> g(dim, dim);
  for (Int i = 0; i < dim; ++i) {
    for (Int j = 0; j < dim; ++j) {
      Real v = 0.1 * (i + 1) - 0.05 * j;
      for (Int k = 0; k < dim; ++k) {
        v += (0.3 + 0.1 * i - 0.07 * j + 0.02 * k) * x(k) * x((k + i + j) % dim);
      }
      g(i, j) = v;
    }
  }
  return g;
}

void case_avg(const fs::path & out, const fs::path & inputs, const fs::path & tmp,
              const std::string & mesh_file,
              Int dim, const std::string & wf, bool two_laws, Real radius) {
  std::stringstream d;
  std::vector<std::string> laws = two_laws ? std::vector<std::string>{"mat_1", "mat_2"}
                                           : std::vector<std::string>{"body"};
  for (const auto & l : laws) {
    d << "material probe_avg_material [\n  name = " << l
      << "\n  rho = 1\n  E = 1\n  nu = 0\n]\n";
  }
  for (const auto & l : laws) {
    d << "non_local " << l << " " << wf << " [\n  radius = " << radius
      << "\n  weight_function weight_parameter [\n    update_rate = 1\n";
    if (wf == "remove_wf") {
      d << "    damage_limit = 0.9\n";
    }
    d << "  ]\n]\n";
  }
  std::string mesh_id = mesh_file.substr(0, mesh_file.find('.'));
  auto dat = tmp / ("avg_" + mesh_id + "_" + wf + ".dat");
  write_dat(dat, d.str());
  getStaticParser().parse(dat.string());
  FixtureWriter w(out, "model/non_local/avg_" + mesh_id + "_" + wf,
                  "averaging test law on " + mesh_file + " with " + wf, PROBE,
                  PROBE_VERSION);
  w.add_input_file("mesh", inputs / mesh_file);
  w.add_input_file("material", dat);
  w.parameters()["type_names"] = Json::array_of(type_names());
  w.parameters()["spatial_dimension"] = dim;
  w.parameters()["two_laws"] = two_laws;
  w.parameters()["radius"] = radius;
  Mesh mesh(dim);
  mesh.read((inputs / mesh_file).string());
  SolidMechanicsModel model(mesh);
  if (two_laws) {
    auto && sel = std::make_shared<MeshDataMaterialSelector<std::string>>(
        "physical_names", model);
    model.setMaterialSelector(sel);
  }
  model.initFull();
  // eigen_grad_u from the integration point coordinates of each law
  for (auto & mat : model.getMaterials()) {
    const auto & cmat = mat; // the non-const filter accessors are protected
    for (auto type : cmat.getElementFilter().elementTypes(dim, _not_ghost)) {
      const auto & filter = cmat.getElementFilter()(type, _not_ghost);
      if (filter.empty()) {
        continue;
      }
      Array<Real> xq(0, dim);
      model.getFEEngine().computeIntegrationPointsCoordinates(xq, type, _not_ghost,
                                                              filter);
      auto & eg = mat.getInternal<Real>("eigen_grad_u")(type, _not_ghost);
      for (auto && [x, g] : zip(make_view(xq, dim), make_view(eg, dim, dim))) {
        g = eigen_of(x, dim);
      }
    }
  }
  auto & manager = model.getNonLocalManager();
  for (int call = 0; call < 2; ++call) {
    if (call == 1) {
      // damage from the law-local point index (exercises remove_wf/damage_wf)
      for (auto & mat : model.getMaterials()) {
        const auto & cmat = mat;
        for (auto type : cmat.getElementFilter().elementTypes(dim, _not_ghost)) {
          auto & dam = mat.getInternal<Real>("damage")(type, _not_ghost);
          for (Int k = 0; k < dam.size(); ++k) {
            dam(k) = (k % 7 == 3) ? 0.95 : 0.1 * (k % 5);
          }
        }
      }
    }
    model.assembleInternalForces();
    std::string pre = "call" + std::to_string(call) + "/";
    for (const auto & l : laws) {
      record_pairs(w, pre + l + "/", manager.getNeighborhood(l));
    }
    record_map(w, pre + "volumes", manager.getVolumes(), dim);
    for (const auto & mat : model.getMaterials()) {
      for (auto type : mat.getElementFilter().elementTypes(dim, _not_ghost)) {
        const auto & a = mat.getInternal<Real>("grad_u non local")(type, _not_ghost);
        std::stringstream s;
        s << type;
        std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
        w.add<double>(pre + mat.getName() + "/grad_u_non_local/" + s.str(),
                      {std::size_t(a.size()), std::size_t(a.getNbComponent())},
                      {"entry", "component"}, "table", v)
            .element_type(s.str());
      }
    }
  }
  record_map(w, "jacobians", manager.getJacobians(), dim);
  w.finish();
}

/* -------------------------------------------------------------------------- */
void case_law(const fs::path & out, const fs::path & inputs, const fs::path & tmp,
              const std::string & keyword,
              const std::string & variant, const std::string & params,
              const std::string & local_var, const std::string & nl_var) {
  std::string name = "law_" + keyword + (variant.empty() ? "" : "_" + variant);
  auto dat = tmp / (name + ".dat");
  write_dat(dat, "material " + keyword + " [\n  name = body\n" + params +
                     "]\n\nnon_local body base_wf [\n  radius = 0.6\n]\n");
  getStaticParser().parse(dat.string());
  FixtureWriter w(out, "model/non_local/" + name,
                  keyword + " on plate.msh under a prescribed displacement history",
                  PROBE, PROBE_VERSION);
  w.add_input_file("mesh", inputs / "plate.msh",
                   "test/test_model/test_common/test_non_local_toolbox/plate.msh");
  w.add_input_file("material", dat);
  const Int dim = 2;
  Mesh mesh(dim);
  mesh.read((inputs / "plate.msh").string());
  SolidMechanicsModel model(mesh);
  model.initFull();
  std::vector<double> lambdas = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 8, 6, 9, 12};
  std::vector<long long> steps;
  for (std::size_t s = 0; s < lambdas.size(); ++s) {
    steps.push_back(s);
  }
  w.set_steps(steps);
  w.parameters()["lambda"] = Json::array_of(lambdas);
  w.parameters()["displacement"] =
      "u_x = 1e-4 * lambda * (0.6 x + 0.3 y^2), u_y = 1e-4 * lambda * (0.2 x y - 0.1 y)";
  w.parameters()["local_variable"] = local_var;
  w.parameters()["non_local_variable"] = nl_var;
  const auto & nodes = mesh.getNodes();
  auto & u = model.getDisplacement();
  auto & mat = model.getMaterial(0);
  auto type = _quadrangle_4;
  for (std::size_t s = 0; s < lambdas.size(); ++s) {
    Real l = 1e-4 * lambdas[s];
    for (Int n = 0; n < nodes.size(); ++n) {
      Real x = nodes(n, 0);
      Real y = nodes(n, 1);
      u(n, 0) = l * (0.6 * x + 0.3 * y * y);
      u(n, 1) = l * (0.2 * x * y - 0.1 * y);
    }
    // SolidMechanicsModel::beforeSolveStep / afterSolveStep (protected
    // overrides) loop over the materials exactly like this
    for (auto & m : model.getMaterials()) {
      m.beforeSolveStep();
    }
    model.assembleInternalForces();
    for (auto & m : model.getMaterials()) {
      m.afterSolveStep(true);
    }
    std::string pre = "step" + std::to_string(s) + "/";
    auto add = [&](const std::string & qname, const Array<Real> & a,
                   bool nodal = false) {
      std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
      w.add<double>(pre + qname, {std::size_t(a.size()), std::size_t(a.getNbComponent())},
                    {nodal ? "node" : "entry", "component"}, nodal ? "nodal" : "table", v)
          .description(nodal ? "nodal vector" : "per integration point (element * nb_quad + q)");
    };
    add("damage", mat.getInternal<Real>("damage")(type));
    add("stress", mat.getInternal<Real>("stress")(type));
    add("local_variable", mat.getInternal<Real>(local_var)(type));
    add("non_local_variable", mat.getInternal<Real>(nl_var)(type));
    add("grad_u", mat.getInternal<Real>("grad_u")(type));
    if (mat.isInternal<Real>("inelastic_strain", _ek_regular)) {
      add("inelastic_strain", mat.getInternal<Real>("inelastic_strain")(type));
    }
    add("internal_force", model.getInternalForce(), true);
  }
  w.finish();
}

} // namespace

/* -------------------------------------------------------------------------- */
/* -------------------------------------------------------------------------- */
/* weight_rate_sweep */
/* -------------------------------------------------------------------------- */
bool filtered() {
  const char * env = std::getenv("NL_PROBE_CASES");
  return env != nullptr && *env != '\0';
}

bool wanted(const std::string & id) {
  if (not filtered()) {
    return true;
  }
  std::stringstream list(std::getenv("NL_PROBE_CASES"));
  std::string item;
  while (std::getline(list, item, ',')) {
    if (item == id) {
      return true;
    }
  }
  return false;
}

void case_weight_rate(const fs::path & out, const fs::path & inputs, const fs::path & tmp) {
  const std::string id = "weight_rate_sweep";
  if (not wanted(id)) {
    return;
  }
  auto dat = tmp / "weight_rate_sweep.dat";
  write_dat(dat, "non_local test_region base_wf [\n   radius = 0.5"
                 "\n   weight_function weight_parameter [\n      update_rate = 1\n   ]\n]\n");
  getStaticParser().parse(dat.string());
  FixtureWriter w(out, "model/non_local/" + id,
                  "RemoveDamagedWithDamageRateWeightFunction::operator() over a radius / "
                  "distance / damage-rate sweep (header-only upstream class, compiled with the "
                  "oracle's flags; G8-01)",
                  PROBE, PROBE_VERSION);
  w.add_input_file("mesh", inputs / "plate.msh",
                   "test/test_model/test_common/test_non_local_toolbox/plate.msh");
  w.add_input_file("material", dat);
  Mesh mesh(2);
  mesh.read((inputs / "plate.msh").string());
  MyModel model(mesh, 2);
  auto & manager = model.getNonLocalManager();
  RemoveDamagedWithDamageRateWeightFunction wf(manager);
  // the internal the constructor registered (init()); operator() reads
  // damage-rate(q2.type, q2.ghost_type)(q2.global_num)
  auto & rate = manager.registerWeightFunctionInternal("damage-rate");
  const std::vector<Real> rates = {0., 0.5, 1., 1. - 1e-12};
  rate.alloc(rates.size(), 1, _triangle_3, _not_ghost);
  for (std::size_t g = 0; g < rates.size(); ++g) {
    rate(_triangle_3, _not_ghost)(g) = rates[g];
  }
  const std::vector<Real> radii = {0.5, 0.51, 0.6, 1.2, 2., 1. / 3., 0.1, 7.3, 1e-3, 123.456};
  IntegrationPoint q1;
  IntegrationPoint q2;
  q1.type = q2.type = _triangle_3;
  q1.ghost_type = q2.ghost_type = _not_ghost;
  q1.global_num = 1000; // never equal to q2's
  // non-foldable exponents for the black-box libm diagnostics
  volatile Real two = 2.;
  volatile Real one = 1.;
  std::vector<double> rv, Rv, wv;
  std::int64_t n_pow2_ne_mul = 0, n_pow1_ne_x = 0, n_points = 0;
  std::vector<double> pow2_ne_alpha;
  for (auto R : radii) {
    wf.setRadius(R);
    const Real R2 = R * R;
    std::vector<Real> distances;
    for (Int k = 0; k <= 300; ++k) {
      Real t = Real(k) / 200.;
      distances.push_back(R * t);
      distances.push_back(R * t * (1. + 1e-9 * std::sin(0.37 * k + 0.1)));
    }
    distances.push_back(R);
    distances.push_back(std::nextafter(R, 0.));
    distances.push_back(std::nextafter(R, 2. * R + 1.));
    for (auto r : distances) {
      rv.push_back(r);
      Rv.push_back(R);
      for (std::size_t g = 0; g < rates.size(); ++g) {
        q2.global_num = g;
        wv.push_back(wf(r, q1, q2));
      }
      const Real x = r * r / R2;
      Real alpha = std::max(0., 1. - x);
      ++n_points;
      if (std::pow(alpha, two) != alpha * alpha) {
        ++n_pow2_ne_mul;
        pow2_ne_alpha.push_back(alpha);
      }
      if (std::pow(x, one) != x) {
        ++n_pow1_ne_x;
      }
    }
  }
  // same integration point: 1
  q1.global_num = 0;
  const std::vector<double> same = {wf(0.25, q1, q1)};
  const auto n = rv.size();
  w.add<double>("r", {n}, {"entry"}, "table", rv).description("distance r");
  w.add<double>("radius", {n}, {"entry"}, "table", Rv).description("setRadius(R)");
  w.add<double>("weight", {n, rates.size()}, {"entry", "component"}, "table", wv)
      .description("RemoveDamagedWithDamageRateWeightFunction::operator()(r, q1, q2), q2 of "
                   "damage rate damage_rate[component] (q1 != q2)");
  w.add<double>("weight_same_point", {1}, {"entry"}, "table", same)
      .description("operator()(0.25, q, q) (same global_num) with R = the last radius");
  w.add<double>("damage_rate", {rates.size()}, {"entry"}, "table", rates)
      .description("damage-rate internal values (damage_limit default 1)");
  if (not pow2_ne_alpha.empty()) {
    w.add<double>("diagnostic/pow2_ne_mul_alpha", {pow2_ne_alpha.size()}, {"entry"}, "table",
                  pow2_ne_alpha)
        .description("alpha where libm pow(alpha, 2.) (non-foldable call) != alpha * alpha");
  }
  auto & p = w.parameters();
  p["damage_limit"] = 1.;
  p["diagnostic/points"] = n_points;
  p["diagnostic/libm_pow2_ne_mul"] = n_pow2_ne_mul;
  p["diagnostic/libm_pow1_ne_x"] = n_pow1_ne_x;
  w.set_notes("operator() is upstream's inline code compiled in this TU with the oracle's flags "
              "(the class has no .cc and no registration upstream). The diagnostics are "
              "black-box libm calls through a volatile exponent (not the code under test).");
  w.finish();
}

int main(int argc, char * argv[]) {
  Eigen::setCpuCacheSizes(16384, 524288, 524288); // D-050
  if (argc < 4) {
    std::cerr << "usage: non_local_probe <fixtures root> <probe dir> <scratch dir>\n";
    return 2;
  }
  fs::path root = argv[1];
  fs::path inputs = fs::path(argv[2]) / "inputs";
  fs::path tmp = argv[3];
  fs::create_directories(tmp);
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    case_weight_rate(root, inputs, tmp);
    if (filtered()) {
      finalize();
      return 0;
    }
    case_manager(root, inputs, tmp, "0.51");
    case_manager(root, inputs, tmp, "0.5");
    for (const auto * wf : {"base_wf", "remove_wf", "damage_wf"}) {
      case_avg(root, inputs, tmp, "plate.msh", 2, wf, true, 0.5);
      case_avg(root, inputs, tmp, "mixed.msh", 2, wf, false, 1.2); // UD-037: equal global_num across types within R
      case_avg(root, inputs, tmp, "cube_hex8.msh", 3, wf, false, 0.6);
    }
    const std::string elastic = "  rho = 1\n  E = 1e9\n  nu = 0.2\n";
    case_law(root, inputs, tmp, "mazars_non_local", "", elastic + "  K0 = 1e-4\n",
             "Ehat", "non_local_variable");
    case_law(root, inputs, tmp, "mazars_non_local", "average_on_damage",
             elastic + "  K0 = 1e-4\n  average_on_damage = true\n", "damage",
             "non_local_variable");
    case_law(root, inputs, tmp, "plastic_mazars_non_local", "",
             elastic + "  K0 = 1e-4\n  sigma_y = 8e4\n  h = 1e8\n", "Ehat",
             "non_local_variable");
    case_law(root, inputs, tmp, "marigo_non_local_class", "",
             elastic + "  Yd = 2\n  Sd = 50\n", "Y", "Y non local");
  } catch (std::exception & e) {
    std::cerr << "non_local_probe: " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
