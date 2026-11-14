// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): the adversarial non-local cases (derived from
// oracle/probes/non_local/non_local_probe.cc)
// promoted to fixtures model/non_local/<case>, plus the damage-band cases of
// UD-557 and the two-law references of UD-558. Same recording helpers, same
// MyModel and averaging test law, which follow upstream's
// test/test_model/test_common/test_non_local_toolbox/{my_model.hh,
// test_material_damage.{hh,cc}} (LGPL-3.0-or-later, (c) EPFL/LSMS).
//
// One case per process (argv[4]) so that a crashing case (update_rate = 0,
// UD-560: the oracle segfaults) cannot take the others down. Case ids
// model/non_local/<case>, format v1.
//
//   mgr_plate_r10 / r0p01 / r0p25   MyModel, base_wf: radius larger than the
//                                   mesh / smaller than any point distance /
//                                   between the inter- and intra-element
//                                   point distances
//   avg_line_r0p1                   1-D segment_2, nodes at decimal tenths
//                                   (inexact distances around R)
//   avg_line_exact_r0p5             1-D segment_2, nodes at multiples of 0.25
//                                   (distances exactly = R)
//   avg_line_mixed_remove_wf        1-D segment_2 + segment_3 (UD-037 in 1-D)
//   avg_mixed3d_damage_wf           3-D hexahedron_8 + pentahedron_6 +
//                                   tetrahedron_4 (UD-037 in 3-D)
//   avg_plate_two_nh                two laws, two neighbourhoods: base_wf
//                                   R 0.3 rate 1 and remove_wf R 0.7 rate 2,
//                                   three calls, damage changing per call
//   avg_plate_rate3                 damage_wf, update_rate 3, four calls
//   avg_plate_rate0                 base_wf, update_rate 0 (UD-560; the
//                                   oracle segfaults: no fixture)
//   avg_plate_dnear1_r0p5 / _r5     damage_wf with damage values close to 1
//                                   (UD-557 conversion domain)
//   avg_plate_dbands_r0p5 / _r100   damage_wf, damages in every band of the
//                                   UD-557 exponent: ordinary, last in-range
//                                   and first out-of-range floor(expb),
//                                   beyond the Int range, ln(1 - x) = +0
//                                   (expb = -inf; R = 100, the default
//                                   radius), the tolerance clamp, D = 1, and
//                                   D = -0.5 (1 - x < 0, expb = NaN)
//   law_ud558_two_mazars            mazars_non_local (Ehat) next to
//                                   mazars_non_local average_on_damage
//   law_ud558_ref_ehat / _damage    the same two-law split with both laws
//                                   averaging Ehat / damage (oracle reference
//                                   of each law in the corrected UD-558
//                                   default)
//   law_ud561_mixed                 mazars_non_local next to the non-local
//                                   Marigo class
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
#include "shape_lagrange.hh"
#include "solid_mechanics_model.hh"

#include <Eigen/Core>

#include <algorithm>
#include <bit>
#include <cmath>
#include <cstdint>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;
using akantu_probe::FixtureWriter;
using akantu_probe::Json;
namespace fs = std::filesystem;

namespace {

const char * const PROBE = "oracle/probes/non_local/adversarial/adversarial_probe.cc";
const char * const PROBE_VERSION = "1";
const char * const AREA = "model/non_local/";

/* -------------------------------------------------------------------------- */
template <class Tag, typename Tag::type M> struct Rob {
  friend typename Tag::type get(Tag) { return M; }
};
template <class WF> struct PairWeightTag {
  using type = std::map<GhostType, std::unique_ptr<Array<Real>>>
      NonLocalNeighborhood<WF>::*;
  friend type get(PairWeightTag);
};
template struct Rob<PairWeightTag<BaseWeightFunction>,
                    &NonLocalNeighborhood<BaseWeightFunction>::pair_weight>;
template struct Rob<
    PairWeightTag<RemoveDamagedWeightFunction>,
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
const bool marigo_nl_registered [[maybe_unused]] =
    instantiateMaterial<MaterialMarigoNonLocal>("marigo_non_local_class");

/* -------------------------------------------------------------------------- */
const std::vector<ElementType> & types() {
  static const std::vector<ElementType> t = {
      _segment_2,     _segment_3,    _triangle_3,  _quadrangle_4,
      _tetrahedron_4, _hexahedron_8, _pentahedron_6};
  return t;
}

std::vector<std::string> type_names() {
  std::vector<std::string> names;
  for (auto t : types()) {
    std::stringstream s;
    s << t;
    names.push_back(s.str());
  }
  return names;
}

std::int64_t type_code(ElementType t) {
  auto && ts = types();
  auto it = std::find(ts.begin(), ts.end(), t);
  if (it == ts.end()) {
    throw std::runtime_error("unexpected element type in the probe");
  }
  return it - ts.begin();
}

/// Non-finite values recorded in the current case, per quantity: (NaN, inf)
/// counts (D-046 (4): a golden case with NaN documents them).
std::map<std::string, std::pair<std::size_t, std::size_t>> & nonfinite() {
  static std::map<std::string, std::pair<std::size_t, std::size_t>> m;
  return m;
}

void inventory(const std::string & q, const std::vector<double> & v) {
  std::size_t nan = 0;
  std::size_t inf = 0;
  for (auto x : v) {
    nan += std::isnan(x) ? 1 : 0;
    inf += std::isinf(x) ? 1 : 0;
  }
  if (nan + inf > 0) {
    nonfinite()[q] = {nan, inf};
  }
}

std::string inventory_text() {
  std::size_t nan = 0;
  std::size_t inf = 0;
  for (const auto & [q, c] : nonfinite()) {
    nan += c.first;
    inf += c.second;
  }
  if (nan + inf == 0) {
    return "no NaN and no infinite value is recorded.";
  }
  // D-046 (4) structured inventory (parsed by
  // crates/akantu-validation/tests/oracle_fixtures.rs):
  // "NaN quantities (count of N values): q (n), q (n);"
  std::stringstream s;
  if (nan > 0) {
    s << "NaN quantities (count of " << nan << " values):";
    bool first = true;
    for (const auto & [q, c] : nonfinite()) {
      if (c.first == 0) {
        continue;
      }
      s << (first ? " " : ", ") << q << " (" << c.first << ")";
      first = false;
    }
    s << "; ";
  }
  if (inf > 0) {
    s << "infinite values (count of " << inf << "):";
    bool first = true;
    for (const auto & [q, c] : nonfinite()) {
      if (c.second == 0) {
        continue;
      }
      s << (first ? " " : ", ") << q << " [" << c.second << "]";
      first = false;
    }
    s << "; ";
  }
  s << "every other recorded quantity is finite.";
  return s.str();
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
    w.add<std::int64_t>(prefix + "pairs_" + g, {pairs.size(), 8},
                        {"entry", "component"}, "table", p);
    const auto & weights = pair_weights(nb, gt);
    std::vector<double> wv(weights.data(),
                           weights.data() + weights.size() * weights.getNbComponent());
    inventory(prefix + "weights_" + g, wv);
    w.add<double>(prefix + "weights_" + g, {std::size_t(weights.size()), 2},
                  {"entry", "component"}, "table", wv);
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
    inventory(name + "/" + s.str(), v);
    w.add<double>(name + "/" + s.str(),
                  {std::size_t(a.size()), std::size_t(a.getNbComponent())},
                  {"entry", "component"}, "table", v)
        .element_type(s.str());
  }
}

void write_dat(const fs::path & p, const std::string & text) {
  std::ofstream f(p);
  f << text;
}

/* -------------------------------------------------------------------------- */
void case_manager(const fs::path & out, const fs::path & nl_inputs,
                  const fs::path & tmp, const std::string & radius,
                  const std::string & id) {
  auto dat = tmp / ("mgr_" + id + ".dat");
  write_dat(dat, "non_local test_region base_wf [\n   radius = " + radius +
                     "\n   weight_function weight_parameter [\n      "
                     "update_rate = 1\n   ]\n]\n");
  getStaticParser().parse(dat.string());
  FixtureWriter w(out, std::string(AREA) + "mgr_plate_" + id,
                  "upstream MyModel on plate.msh, base_wf radius " + radius, PROBE,
                  PROBE_VERSION);
  w.add_input_file("mesh", nl_inputs / "plate.msh",
                   "test/test_model/test_common/test_non_local_toolbox/plate.msh");
  w.add_input_file("material", dat);
  w.parameters()["type_names"] = Json::array_of(type_names());
  Mesh mesh(2);
  mesh.read((nl_inputs / "plate.msh").string());
  MyModel model(mesh, 2);
  auto & manager = model.getNonLocalManager();
  record_pairs(w, "", manager.getNeighborhood("test_region"));
  record_map(w, "volumes", manager.getVolumes(), 2);
  record_map(w, "jacobians", manager.getJacobians(), 2);
  w.finish();
}

/* -------------------------------------------------------------------------- */
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

/// damage of law-local point k at call c (c >= 1), pattern by name
Real damage_of(const std::string & pattern, Int k, Int c) {
  if (pattern == "probe") {
    return (k % 7 == 3) ? 0.95 : 0.1 * (k % 5);
  }
  if (pattern == "shift") {
    Int m = k + c;
    return (m % 7 == 3) ? 0.95 : 0.1 * (m % 5);
  }
  if (pattern == "bands") {
    // UD-557 exponent bands (see the header); the two bit patterns are the
    // last damage with floor(expb) inside the Int range (2147483459) and the
    // first one beyond it (2147483839): no damage gives floor(expb) =
    // 2^31 - 1 (achievable expb values are ~380 apart there)
    static const std::vector<Real> d = {
        0.,
        0.3,
        0.99,
        1. - 1e-4,
        std::bit_cast<Real>(std::uint64_t{0x3fefffb4f9d6acc3}),
        std::bit_cast<Real>(std::uint64_t{0x3fefffb4f9d6acc4}),
        1. - 3e-5,
        1. - 1e-6,
        1. - 1e-8,
        1. - 1e-10,
        1. - 1e-11,
        1. - 1e-12,
        1.,
        -0.5};
    return d[k % d.size()];
  }
  if (pattern == "near1") {
    static const std::vector<Real> d = {0.,         0.5,        0.99,
                                        1. - 1e-4,  1. - 3e-5,  1. - 1e-6,
                                        1. - 1e-9,  1. - 1e-12, 1.};
    return d[k % d.size()];
  }
  throw std::runtime_error("unknown damage pattern " + pattern);
}

struct Law {
  std::string name;
  std::string wf;
  std::string radius;
  Int rate;
};

void case_avg(const fs::path & out, const fs::path & mesh_path, const fs::path & tmp,
              const std::string & case_name, Int dim, const std::vector<Law> & laws,
              bool by_physical_names, Int ncalls, const std::string & pattern) {
  nonfinite().clear();
  std::stringstream d;
  for (const auto & l : laws) {
    d << "material probe_avg_material [\n  name = " << l.name
      << "\n  rho = 1\n  E = 1\n  nu = 0\n]\n";
  }
  for (const auto & l : laws) {
    d << "non_local " << l.name << " " << l.wf << " [\n  radius = " << l.radius
      << "\n  weight_function weight_parameter [\n    update_rate = " << l.rate
      << "\n";
    if (l.wf == "remove_wf") {
      d << "    damage_limit = 0.9\n";
    }
    d << "  ]\n]\n";
  }
  auto dat = tmp / (case_name + ".dat");
  write_dat(dat, d.str());
  getStaticParser().parse(dat.string());
  FixtureWriter w(out, std::string(AREA) + case_name,
                  "adversarial averaging case " + case_name, PROBE, PROBE_VERSION);
  w.add_input_file("mesh", mesh_path);
  w.add_input_file("material", dat);
  w.parameters()["type_names"] = Json::array_of(type_names());
  w.parameters()["spatial_dimension"] = dim;
  w.parameters()["by_physical_names"] = by_physical_names;
  w.parameters()["calls"] = ncalls;
  w.parameters()["damage_pattern"] = pattern;
  std::vector<std::string> law_names;
  for (const auto & l : laws) {
    law_names.push_back(l.name);
  }
  w.parameters()["laws"] = Json::array_of(law_names);
  Mesh mesh(dim);
  mesh.read(mesh_path.string());
  SolidMechanicsModel model(mesh);
  if (by_physical_names) {
    auto && sel = std::make_shared<MeshDataMaterialSelector<std::string>>(
        "physical_names", model);
    model.setMaterialSelector(sel);
  }
  model.initFull();
  for (auto & mat : model.getMaterials()) {
    const auto & cmat = mat;
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
  for (Int call = 0; call < ncalls; ++call) {
    if (call >= 1) {
      for (auto & mat : model.getMaterials()) {
        const auto & cmat = mat;
        for (auto type : cmat.getElementFilter().elementTypes(dim, _not_ghost)) {
          auto & dam = mat.getInternal<Real>("damage")(type, _not_ghost);
          for (Int k = 0; k < dam.size(); ++k) {
            dam(k) = damage_of(pattern, k, call);
          }
        }
      }
    }
    model.assembleInternalForces();
    std::string pre = "call" + std::to_string(call) + "/";
    for (const auto & l : laws) {
      record_pairs(w, pre + l.name + "/", manager.getNeighborhood(l.name));
    }
    record_map(w, pre + "volumes", manager.getVolumes(), dim);
    for (const auto & mat : model.getMaterials()) {
      for (auto type : mat.getElementFilter().elementTypes(dim, _not_ghost)) {
        const auto & a = mat.getInternal<Real>("grad_u non local")(type, _not_ghost);
        std::stringstream s;
        s << type;
        std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
        inventory(pre + mat.getName() + "/grad_u_non_local/" + s.str(), v);
        w.add<double>(pre + mat.getName() + "/grad_u_non_local/" + s.str(),
                      {std::size_t(a.size()), std::size_t(a.getNbComponent())},
                      {"entry", "component"}, "table", v)
            .element_type(s.str());
      }
    }
  }
  record_map(w, "jacobians", manager.getJacobians(), dim);
  if (pattern == "near1" or pattern == "bands") {
    w.set_notes(
        "UD-557 (decision D-062 (3)), D-046 (4): damage_wf with prescribed "
        "damages close to 1" +
        std::string(pattern == "bands" ? " and D = -0.5" : "") +
        " (damage_of pattern '" + pattern +
        "', from call 1). Upstream converts expb = 2 ln 0.51 / ln(1 - 0.49 "
        "R_t/R_init) with Int expb_floor = std::floor(expb); for floor(expb) >= "
        "2^31 (1 - D <~ 3.58e-5), for expb = -inf (ln(1 - x) rounded to +0: "
        "R >= 3.74 and 1 - D in [~1.5e-7/R^2, 1.06e-8]) and for expb = NaN "
        "(1 - x < 0, D = -0.5) the conversion is undefined and this x86_64 "
        "oracle (cvttsd2si) gives INT_MIN: b = -2^31, so the raw weight "
        "alpha^b is +inf for alpha < 1 - ~1e-8, huge but finite for alpha "
        "closer to 1 (R = 100: alpha = 1 - r^2/R^4), +inf for pairs outside the "
        "support (pow(0, b)) and 1 at r = 0; volumes containing such a weight "
        "are infinite, the normalised weights w * (1 / inf) = inf * 0 are NaN "
        "and so are the averages they enter. Call 0 (no damage) is finite. "
        "Recorded: " +
        inventory_text() +
        " Every NaN is the platform's default quiet NaN (x86_64: "
        "0xfff8000000000000), compared under D-046. Port: "
        "NonLocalOptions::weight_arithmetic = WeightArithmetic::UpstreamX86_64 "
        "(also in NonLocalOptions::upstream()) replays these values; the "
        "corrected default (WeightArithmetic::Corrected) is finite here.");
  }
  w.finish();
}

/* -------------------------------------------------------------------------- */
/// two non-local laws on plate.msh (mat_1, mat_2 by physical names) under
/// the prescribed displacement history; records every law's damage,
/// stress, local/non-local variables and the internal force per step. An
/// exception is recorded in parameters["exception"].
void case_law_pair(const fs::path & out, const fs::path & nl_inputs,
                   const fs::path & tmp, const std::string & case_name,
                   const std::string & mat1, const std::string & mat2) {
  auto dat = tmp / (case_name + ".dat");
  write_dat(dat, mat1 + mat2 +
                     "\nnon_local mat_1 base_wf [\n  radius = 0.6\n]\n"
                     "non_local mat_2 base_wf [\n  radius = 0.6\n]\n");
  getStaticParser().parse(dat.string());
  FixtureWriter w(out, std::string(AREA) + case_name,
                  "two non-local laws on plate.msh: " + case_name, PROBE,
                  PROBE_VERSION);
  w.add_input_file("mesh", nl_inputs / "plate.msh",
                   "test/test_model/test_common/test_non_local_toolbox/plate.msh");
  w.add_input_file("material", dat);
  const Int dim = 2;
  Mesh mesh(dim);
  mesh.read((nl_inputs / "plate.msh").string());
  SolidMechanicsModel model(mesh);
  auto && sel = std::make_shared<MeshDataMaterialSelector<std::string>>(
      "physical_names", model);
  model.setMaterialSelector(sel);
  std::vector<double> lambdas = {1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 8, 6, 9, 12};
  w.parameters()["lambda"] = Json::array_of(lambdas);
  std::string stage = "initFull";
  try {
    model.initFull();
    std::vector<long long> steps;
    for (std::size_t s = 0; s < lambdas.size(); ++s) {
      steps.push_back(s);
    }
    w.set_steps(steps);
    const auto & nodes = mesh.getNodes();
    auto & u = model.getDisplacement();
    auto type = _quadrangle_4;
    for (std::size_t s = 0; s < lambdas.size(); ++s) {
      Real l = 1e-4 * lambdas[s];
      for (Int n = 0; n < nodes.size(); ++n) {
        Real x = nodes(n, 0);
        Real y = nodes(n, 1);
        u(n, 0) = l * (0.6 * x + 0.3 * y * y);
        u(n, 1) = l * (0.2 * x * y - 0.1 * y);
      }
      for (auto & m : model.getMaterials()) {
        m.beforeSolveStep();
      }
      stage = "assembleInternalForces step " + std::to_string(s);
      model.assembleInternalForces();
      for (auto & m : model.getMaterials()) {
        m.afterSolveStep(true);
      }
      std::string pre = "step" + std::to_string(s) + "/";
      for (auto & mat : model.getMaterials()) {
        for (const auto * q :
             {"damage", "stress", "Ehat", "non_local_variable", "Y", "Y non local"}) {
          if (not mat.isInternal<Real>(q, _ek_regular)) {
            continue;
          }
          const auto & a = mat.getInternal<Real>(q)(type);
          std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
          std::string qn = q;
          std::replace(qn.begin(), qn.end(), ' ', '_');
          w.add<double>(pre + mat.getName() + "/" + qn,
                        {std::size_t(a.size()), std::size_t(a.getNbComponent())},
                        {"entry", "component"}, "table", v);
        }
      }
      const auto & f = model.getInternalForce();
      std::vector<double> v(f.data(), f.data() + f.size() * f.getNbComponent());
      w.add<double>(pre + "internal_force",
                    {std::size_t(f.size()), std::size_t(f.getNbComponent())},
                    {"node", "component"}, "nodal", v);
    }
  } catch (std::exception & e) {
    w.parameters()["exception"] = std::string(e.what());
    w.parameters()["exception_stage"] = stage;
  }
  w.finish();
}

} // namespace

/* -------------------------------------------------------------------------- */
int main(int argc, char * argv[]) {
  Eigen::setCpuCacheSizes(16384, 524288, 524288); // D-050
  if (argc < 5) {
    std::cerr << "usage: adversarial_probe <fixtures root> <inputs dir> "
                 "<scratch dir> <case>\n";
    return 2;
  }
  fs::path root = argv[1];
  fs::path in = argv[2];
  fs::path nl_in = argv[2];
  fs::path tmp = argv[3];
  std::string c = argv[4];
  fs::create_directories(tmp);
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  try {
    auto plate = nl_in / "plate.msh";
    if (c == "mgr_plate_r10") {
      case_manager(root, nl_in, tmp, "10", "r10");
    } else if (c == "mgr_plate_r0p01") {
      case_manager(root, nl_in, tmp, "0.01", "r0p01");
    } else if (c == "mgr_plate_r0p25") {
      case_manager(root, nl_in, tmp, "0.25", "r0p25");
    } else if (c == "avg_line_r0p1") {
      case_avg(root, in / "line.msh", tmp, c, 1, {{"body", "base_wf", "0.1", 1}},
               false, 2, "probe");
    } else if (c == "avg_line_exact_r0p5") {
      case_avg(root, in / "line_exact.msh", tmp, c, 1,
               {{"body", "base_wf", "0.5", 1}}, false, 2, "probe");
    } else if (c == "avg_line_mixed_remove_wf") {
      case_avg(root, in / "line_mixed.msh", tmp, c, 1,
               {{"body", "remove_wf", "0.6", 1}}, false, 2, "probe");
    } else if (c == "avg_mixed3d_damage_wf") {
      case_avg(root, in / "mixed3d.msh", tmp, c, 3,
               {{"body", "damage_wf", "1.5", 1}}, false, 2, "probe");
    } else if (c == "avg_plate_two_nh") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "base_wf", "0.3", 1}, {"mat_2", "remove_wf", "0.7", 2}},
               true, 3, "shift");
    } else if (c == "avg_plate_rate3") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "damage_wf", "0.5", 3}, {"mat_2", "damage_wf", "0.5", 3}},
               true, 4, "shift");
    } else if (c == "avg_plate_rate0") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "base_wf", "0.5", 0}, {"mat_2", "base_wf", "0.5", 0}},
               true, 2, "probe");
    } else if (c == "avg_plate_dnear1_r0p5") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "damage_wf", "0.5", 1}, {"mat_2", "damage_wf", "0.5", 1}},
               true, 2, "near1");
    } else if (c == "avg_plate_dnear1_r5") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "damage_wf", "5", 1}, {"mat_2", "damage_wf", "5", 1}},
               true, 2, "near1");
    } else if (c == "avg_plate_dbands_r0p5") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "damage_wf", "0.5", 1}, {"mat_2", "damage_wf", "0.5", 1}},
               true, 2, "bands");
    } else if (c == "avg_plate_dbands_r100") {
      case_avg(root, plate, tmp, c, 2,
               {{"mat_1", "damage_wf", "100", 1}, {"mat_2", "damage_wf", "100", 1}},
               true, 2, "bands");
    } else if (c == "law_ud558_two_mazars") {
      const std::string el = "  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n";
      case_law_pair(root, nl_in, tmp, c,
                    "material mazars_non_local [\n  name = mat_1\n" + el + "]\n",
                    "material mazars_non_local [\n  name = mat_2\n" + el +
                        "  average_on_damage = true\n]\n");
    } else if (c == "law_ud558_ref_ehat") {
      const std::string el = "  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n";
      case_law_pair(root, nl_in, tmp, c,
                    "material mazars_non_local [\n  name = mat_1\n" + el + "]\n",
                    "material mazars_non_local [\n  name = mat_2\n" + el + "]\n");
    } else if (c == "law_ud558_ref_damage") {
      const std::string el = "  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n";
      case_law_pair(root, nl_in, tmp, c,
                    "material mazars_non_local [\n  name = mat_1\n" + el +
                        "  average_on_damage = true\n]\n",
                    "material mazars_non_local [\n  name = mat_2\n" + el +
                        "  average_on_damage = true\n]\n");
    } else if (c == "law_ud561_mixed") {
      const std::string el = "  rho = 1\n  E = 1e9\n  nu = 0.2\n";
      case_law_pair(root, nl_in, tmp, c,
                    "material mazars_non_local [\n  name = mat_1\n" + el +
                        "  K0 = 1e-4\n]\n",
                    "material marigo_non_local_class [\n  name = mat_2\n" + el +
                        "  Yd = 2\n  Sd = 50\n]\n");
    } else {
      std::cerr << "unknown case " << c << "\n";
      return 2;
    }
  } catch (std::exception & e) {
    std::cerr << "adversarial_probe " << c << ": " << e.what() << std::endl;
    return 1;
  }
  finalize();
  return 0;
}
