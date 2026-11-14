// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): the non-local toolbox and the traction-at-split-node
// contact on distributed meshes (MPI-048…052, TEST-079 / 080). The test laws
// follow upstream's test/test_model/test_common/test_non_local_toolbox/
// {test_material.{hh,cc}, test_material_damage.{hh,cc}} (LGPL-3.0-or-later,
// (c) EPFL/LSMS); the contact scenarios drive the package classes unmodified,
// adding the public NTNBaseContact::initParallel() call that no upstream
// program makes (D-083).
//
// Case ids parallel/<scenario>/np<P>/rank<r> (format v1, probe version 1):
//   nl_region   test_build_neighborhood_parallel's setting: parallel_test.msh,
//               two laws (physical names mat_1 / mat_2) of the averaging test
//               law sharing the neighbourhood test_region (remove_wf, R 0.5,
//               update_rate 1, damage_limit 0.9). Records the distribution,
//               the mesh after initFull (ghost connectivities, counters, node
//               ids / flags), the grid synchronizer schemes, the pair lists,
//               weights and volumes, then two assembleInternalForces: an
//               eigen_grad_u polynomial of the integration-point coordinates
//               (grad_u = -eigen_grad_u) and, before the second call, damage
//               0.95 on the owned points of a band (removed by remove_wf):
//               grad_u non local, the weight internal `damage` after the
//               exchange, weights and volumes.
//   nl_pair     (1 and 2 ranks: upstream aborts on 4, UD-1100)
//               test_pair_computation's setting: pair_test.msh, two laws of
//               the averaging test law with their own neighbourhoods mat_1 /
//               mat_2 (remove_wf, R 0.5), pair lists of both, upstream's
//               brute-force pair check (its criteria) and one averaging pass.
//   nl_mazars   mazars_non_local (K0 1e-4, base_wf R 0.3) on parallel_test.msh,
//               one law, a prescribed displacement history of 10 steps with
//               beforeSolveStep / assembleInternalForces / afterSolveStep(true)
//               per step: damage, Ehat (local and non-local), stress, internal
//               force.
//   nl_mazars_two  (1 rank: upstream aborts on 2 and 4, UD-1100) the same
//               with two mazars_non_local laws (mat_1 average on
//               Ehat, mat_2 average on damage; R 0.3 and 0.25), physical-name
//               selector: two neighbourhoods, two grid synchronizers.
//   nl_mixed    the same history with mat_1 = mazars_non_local (base_wf R 0.3)
//               and mat_2 = elastic (physical-name selector): the cleanup of
//               the ghost elements in no pair removes elastic ghost elements
//               of the distribution's ghost layer (UD-1101).
//   ntn_compression  TestNTNFrictionFixture.TestCompression's setting
//               (ntn_test_compression.msh, slip weakening) on a distributed
//               mesh (vertical strips, distribute_strips) with
//               contact->initParallel(): the filtered schemes, a
//               direct synchronize(_cf_nodal) / (_cf_incr) check with rank-
//               specific values at ghost-element nodes, then 60 explicit steps:
//               contact pressure, normals, slips, displacements, velocities.
//   ntrf_coulomb  NTRFContact (rigid plane y = 0) with NTRFFriction (coulomb,
//               rubin_ampuero) on the slider of ntn_test_compression.msh (the
//               serial probe's ntrf_coulomb setting), vertical strips,
//               initParallel(), 60 steps.
// Later scenarios (the earlier ones unchanged):
//   nl_damage_wf  nl_region with the damage_wf weight function (update_rate
//               1): upstream's DamagedWeightFunction packs nothing for
//               _mnl_weight (UD-1103), so its ghost points are weighed with
//               the ghost elements' own damage (never computed): pairs,
//               weights, volumes, grad_u non local of the two calls (the
//               second after the damage band).
//   nl_mazars_two_strips  (3 ranks) nl_mazars_two on vertical strips
//               (clamp(floor((x + 1) / 2 * np))): with upstream's cleanup the
//               second erasure pass (stale set, UD-1100) erases ghost
//               elements on one rank and upstream completes: the evidence of
//               the multi-pass compat erasure.
//   ntn_update_normals  ntn_compression's setting (no exchange check),
//               initParallel(), 40 steps, then NTNContact::updateNormals(),
//               which starts with synchronize(_cf_nodal):
//               normals, displacement and current position after it.
// Probe version 2 of ntn_compression (the other scenarios and
// the earlier payloads unchanged): ntn_compression also records
// final/external_work_rank, this rank's accumulated work (upstream's private
// NTNContactSolverCallback::external_work) before getExternalWork()'s
// allReduce(_sum): at 8 ranks MPI_Allreduce sums the ranks' works in another
// order than rank order (D-039), and the rank-ordered replay checks the
// rank-order fold of these contributions instead.
// PAR4_PROBE_FULL_LISTS=1 (with PAR4_PROBE_STAGE_ONLY=1, never into the
// fixtures) also writes every pair list and weight array as text to
// <scratch>/full-lists/ (the Rust tests write the same format
// on a digest mismatch).
//
// Ghost filter order (UD-232): upstream appends ghost elements to their law
// in MPI_Waitany completion order (the mesh element synchronizer at the law
// assignment, the grid synchronizers when later neighbourhoods are created
// and in initializeNonLocal). The probe's model subclass rewrites every
// suffix such an exchange appended into the rank-order unpack (synchronizers
// in call order, receive schemes in rank order, scheme order: the admissible
// upstream execution in which messages complete in rank order), right after
// the exchange and before anything reads the filters; the number of moved
// entries is run-dependent and only logged (stderr), the number of rewrites
// is recorded (ghost_filter_rewrites). At that point the moved entries
// hold no state (assignment happens before the laws' internals are sized,
// and in initializeNonLocal before anything is computed on them).
//
// Usage: par4_probe <fixtures_root> <upstream_src_root> <scratch> <scenario>
// Scotch is seeded with RandomGenerator<Idx>::seed(kSeed) on rank 0 before
// the partitioner is built.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_random_generator.hh>
#include <communicator.hh>
#include <element_synchronizer.hh>
#include <grid_synchronizer.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_partition_mesh_data.hh>
#include <mesh_partition_scotch.hh>
#include <solid_mechanics_model.hh>
#include <material_selector.hh>
#include <material_damage.hh>
#include <material_damage_non_local.hh>
#include <base_weight_function.hh>
#include <remove_damaged_weight_function.hh>
#include <damaged_weight_function.hh>
#include <non_local_manager.hh>
#include <non_local_neighborhood.hh>
#include <non_local_neighborhood_base.hh>
#include <neighborhood_base.hh>

#include <ntn_base_contact.hh>
#include <ntn_base_friction.hh>
#include <ntn_contact.hh>
#include <ntn_contact_solvercallback.hh>
#include <ntn_initiation_function.hh>
#include <ntrf_contact.hh>

extern "C" {
#include <scotch.h>
}

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <memory>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

/* -------------------------------------------------------------------------- */
/* Test laws (upstream test_material / test_material_damage)                  */
/* -------------------------------------------------------------------------- */
namespace {
template <Int dim>
class Par4RegionMaterial
    : public MaterialDamageNonLocal<dim, MaterialDamage<dim, MaterialElastic>> {
  using Parent = MaterialDamageNonLocal<dim, MaterialDamage<dim, MaterialElastic>>;

public:
  Par4RegionMaterial(SolidMechanicsModel & model, const ID & id)
      : Parent(model, id), grad_u_nl(this->registerInternal("grad_u non local", dim * dim)) {
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
  void computeNonLocalStress(ElementType /*unused*/, GhostType /*unused*/) final {}
  void computeNonLocalStresses(GhostType /*unused*/) final {}

protected:
  ID getNeighborhoodName() override { return "test_region"; }

private:
  InternalField<Real> & grad_u_nl;
};

template <Int dim>
class Par4DamageMaterial
    : public MaterialDamageNonLocal<dim, MaterialDamage<dim, MaterialElastic>> {
  using Parent = MaterialDamageNonLocal<dim, MaterialDamage<dim, MaterialElastic>>;

public:
  Par4DamageMaterial(SolidMechanicsModel & model, const ID & id)
      : Parent(model, id), grad_u_nl(this->registerInternal("grad_u non local", dim * dim)) {
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
  void computeNonLocalStress(ElementType /*unused*/, GhostType /*unused*/) final {}

private:
  InternalField<Real> & grad_u_nl;
};

const bool par4_region_allocated [[maybe_unused]] =
    instantiateMaterial<Par4RegionMaterial>("par4_region_material");
const bool par4_damage_allocated [[maybe_unused]] =
    instantiateMaterial<Par4DamageMaterial>("par4_damage_material");

/* -------------------------------------------------------------------------- */
/* Access to private / protected members (explicit-instantiation idiom:       */
/* names in the template arguments of an explicit instantiation are exempt    */
/* from access checking, [temp.spec]).                                        */
/* -------------------------------------------------------------------------- */
template <class WF> struct PairWeightTag {
  using type = std::map<GhostType, std::unique_ptr<Array<Real>>> NonLocalNeighborhood<WF>::*;
  friend type get(PairWeightTag);
};
template <class Tag, typename Tag::type M> struct Rob {
  friend typename Tag::type get(Tag) { return M; }
};
template struct Rob<PairWeightTag<BaseWeightFunction>,
                    &NonLocalNeighborhood<BaseWeightFunction>::pair_weight>;
template struct Rob<PairWeightTag<RemoveDamagedWeightFunction>,
                    &NonLocalNeighborhood<RemoveDamagedWeightFunction>::pair_weight>;
template struct Rob<PairWeightTag<DamagedWeightFunction>,
                    &NonLocalNeighborhood<DamagedWeightFunction>::pair_weight>;

struct GridTag {
  using type = std::unique_ptr<GridSynchronizer> NeighborhoodBase::*;
  friend type get(GridTag);
};
template struct Rob<GridTag, &NeighborhoodBase::grid_synchronizer>;

struct GlobalNbhTag {
  using type = std::set<ID> NonLocalManager::*;
  friend type get(GlobalNbhTag);
};
template struct Rob<GlobalNbhTag, &NonLocalManager::global_neighborhoods>;

struct NtnSyncTag {
  using type = std::unique_ptr<ElementSynchronizer> NTNBaseContact::*;
  friend type get(NtnSyncTag);
};
template struct Rob<NtnSyncTag, &NTNBaseContact::synchronizer>;

struct NtnWorkTag {
  using type = Vector<Real, 2> NTNContactSolverCallback::*;
  friend type get(NtnWorkTag);
};
template struct Rob<NtnWorkTag, &NTNContactSolverCallback::external_work>;

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

const Array<Real> * pair_weights(NonLocalNeighborhoodBase & nb, GhostType gt) {
  if (auto * w = weights_of<BaseWeightFunction>(nb, gt)) {
    return w;
  }
  if (auto * w = weights_of<DamagedWeightFunction>(nb, gt)) {
    return w;
  }
  return weights_of<RemoveDamagedWeightFunction>(nb, gt);
}

const GridSynchronizer * grid_of(NonLocalNeighborhoodBase & n) {
  NeighborhoodBase & b = n;
  return (b.*get(GridTag{})).get();
}

/// The manager's global neighbourhood names (namespace-scope helper: inside
/// the model subclass the member ParameterRegistry::get hides the friend).
const std::set<ID> & global_names(NonLocalManager & m) { return m.*get(GlobalNbhTag{}); }

/* -------------------------------------------------------------------------- */
constexpr const char * kProbe = "par4_probe";
constexpr const char * kProbeVersion = "1";
// Ntn_compression with final/external_work_rank
constexpr const char * kProbeVersion2 = "2";
constexpr int kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;
constexpr long kSeed = 1234;
using I64 = std::int64_t;

fs::path g_root, g_src, g_scratch;
Int g_rank = 0, g_size = 1;

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string ghost_name(GhostType g) { return g == _not_ghost ? "not_ghost" : "ghost"; }

void add_i(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
           std::size_t nbc, const std::string & desc) {
  std::size_t rows = nbc == 0 ? 0 : v.size() / nbc;
  if (nbc <= 1) {
    w.add(name, {v.size()}, {"entry"}, "table", v).description(desc).tolerance_class("exact");
  } else {
    w.add(name, {rows, nbc}, {"entry", "component"}, "table", v)
        .description(desc)
        .tolerance_class("exact");
  }
}

void add_r(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
           std::size_t nbc, const std::string & desc) {
  std::size_t rows = nbc == 0 ? 0 : v.size() / nbc;
  w.add(name, {rows, nbc}, {"entry", "component"}, "table", v)
      .description(desc)
      .tolerance_class("exact");
}

template <typename T> std::vector<I64> ints(const Array<T> & a) {
  std::vector<I64> v(a.size() * a.getNbComponent());
  for (std::size_t i = 0; i < v.size(); ++i) {
    v[i] = static_cast<I64>(a.data()[i]);
  }
  return v;
}

std::vector<double> reals(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

std::string case_id(const std::string & scenario) {
  return "parallel/" + scenario + "/np" + std::to_string(g_size) + "/rank" +
         std::to_string(g_rank);
}

void record_pin(ap::FixtureWriter & w) {
  w.parameters()["oracle_pin/eigen_cache_sizes"] = std::to_string(kEigenL1) + " " +
                                                   std::to_string(kEigenL2) + " " +
                                                   std::to_string(kEigenL3);
  const char * coretype = std::getenv("OPENBLAS_CORETYPE");
  w.parameters()["oracle_pin/openblas_coretype"] = std::string(coretype ? coretype : "unset");
  const char * threads = std::getenv("OPENBLAS_NUM_THREADS");
  w.parameters()["oracle_pin/openblas_num_threads"] = std::string(threads ? threads : "unset");
}

void parse_dat(const std::string & dat, const std::string & name) {
  if (g_rank == 0) {
    std::ofstream f(g_scratch / name);
    f << dat;
  }
  Communicator::getStaticCommunicator().barrier();
  getStaticParser().parse((g_scratch / name).string());
}

/// Seeded Scotch partition on rank 0 (as oracle/probes/parallel).
void distribute_scotch(Mesh & mesh, ap::FixtureWriter & w) {
  std::shared_ptr<MeshPartition> partition;
  if (g_size > 1 and g_rank == 0) {
    RandomGenerator<Idx>::seed(kSeed);
    partition = std::make_shared<MeshPartitionScotch>(mesh, mesh.getSpatialDimension());
    partition->partitionate(g_size);
    std::vector<I64> lin;
    for (auto type : mesh.elementTypes(mesh.getSpatialDimension(), _not_ghost, _ek_not_defined)) {
      for (auto p : partition->getPartition(type, _not_ghost)) {
        lin.push_back(p);
      }
    }
    add_i(w, "partition/linearized", lin, 1,
          "Scotch part of every element of the mesh dimension (types in enum order)");
    w.parameters()["scotch/seed"] = static_cast<long long>(kSeed);
    w.parameters()["scotch/version"] = std::to_string(SCOTCH_VERSION) + "." +
                                       std::to_string(SCOTCH_RELEASE) + "." +
                                       std::to_string(SCOTCH_PATCHLEVEL);
    const char * threads = std::getenv("SCOTCH_PTHREAD_NUMBER");
    w.parameters()["scotch/pthread_number"] = std::string(threads ? threads : "unset");
  }
  mesh.distribute(partition);
}

/// Vertical strips of the unit-width split-node meshes (part = floor(np x)
/// of the element barycentre, clamped), so that both sides of the contact
/// interface are on the same ranks (with Scotch the slider and the base land
/// on different ranks and upstream pairs no node at all).
/// Vertical strips: clamp(floor(np * x), 0, np - 1) of the barycentre x on a
/// mesh over [0, 1] (`centred` false), clamp(floor((x + 1) / 2 * np), 0,
/// np - 1) on a mesh over [-1, 1] (`centred` true).
void distribute_strips(Mesh & mesh, ap::FixtureWriter & w, bool centred = false) {
  std::shared_ptr<MeshPartition> partition;
  if (g_size > 1 and g_rank == 0) {
    const Int dim = mesh.getSpatialDimension();
    auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("strip_partition");
    std::vector<I64> lin;
    for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
      auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
      for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
        Vector<Real> b(dim);
        mesh.getBarycenter(Element{type, e, _not_ghost}, b);
        Idx p = centred ? Idx(std::floor((b(0) + 1.) / 2. * Real(g_size)))
                        : Idx(std::floor(b(0) * Real(g_size)));
        a(e) = std::clamp<Idx>(p, 0, g_size - 1);
        lin.push_back(a(e));
      }
    }
    add_i(w, "partition/linearized", lin, 1,
          "strip part of every element of the mesh dimension (types in enum order)");
    w.parameters()["partition/rule"] =
        std::string(centred ? "clamp(floor((barycentre_x + 1) / 2 * np), 0, np - 1)"
                            : "clamp(floor(np * barycentre_x), 0, np - 1)");
    partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, dim);
    partition->partitionate(g_size);
  }
  mesh.distribute(partition);
}

/// Every element block of `dim` (both ghost types): global connectivities;
/// ghost counters of the ghost blocks; nodes: global ids, flags, owner ranks.
void record_mesh(ap::FixtureWriter & w, Mesh & mesh, const std::string & pre, Int dim) {
  MeshAccessor acc(mesh);
  for (auto gt : ghost_types) {
    for (auto type : mesh.elementTypes(dim, gt, _ek_not_defined)) {
      const auto & conn = mesh.getConnectivity(type, gt);
      std::vector<I64> g;
      for (Idx e = 0; e < conn.size(); ++e) {
        for (Idx k = 0; k < conn.getNbComponent(); ++k) {
          g.push_back(mesh.isDistributed() ? I64(mesh.getNodeGlobalId(conn(e, k)))
                                           : I64(conn(e, k)));
        }
      }
      add_i(w, pre + "/conn/" + type_name(type) + "/" + ghost_name(gt), g,
            conn.getNbComponent(), "global connectivity of every element (mesh order)");
      if (gt == _ghost and mesh.isDistributed()) {
        add_i(w, pre + "/ghost_counters/" + type_name(type), ints(acc.getGhostsCounters(type, _ghost)),
              1, "ghost counter of every ghost element");
      }
    }
  }
  std::vector<I64> gid, flags, pranks;
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    gid.push_back(mesh.isDistributed() ? I64(mesh.getNodeGlobalId(n)) : I64(n));
    flags.push_back(I64(std::underlying_type_t<NodeFlag>(mesh.getNodeFlag(n))));
    pranks.push_back(I64(mesh.getNodePrank(n)));
  }
  add_i(w, pre + "/node_global_ids", gid, 1, "global id of every local node");
  add_i(w, pre + "/node_flags", flags, 1, "NodeFlag bits of every local node");
  add_i(w, pre + "/node_pranks", pranks, 1, "owner rank of every local node (-1: none)");
  add_r(w, pre + "/nodes", reals(mesh.getNodes()), mesh.getSpatialDimension(),
        "coordinates of every local node");
}

template <class Sync>
void record_schemes(ap::FixtureWriter & w, const Sync & sync, const std::string & pre) {
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : sync.getCommunications().iterateSchemes(sr)) {
      std::vector<I64> v;
      for (auto && el : scheme) {
        v.push_back(I64(el.type));
        v.push_back(I64(el.element));
        v.push_back(I64(el.ghost_type));
      }
      add_i(w, pre + "/" + (sr == _send ? "send" : "recv") + "/" + std::to_string(proc), v, 3,
            "scheme entries (type enum, element, ghost type), scheme order");
    }
  }
}

/* -------------------------------------------------------------------------- */
/* Model with the rank-order rewrite of the ghost filters (UD-232)            */
/* -------------------------------------------------------------------------- */
class ProbeSMM : public SolidMechanicsModel {
public:
  using SolidMechanicsModel::SolidMechanicsModel;
  I64 moved{0};
  I64 rewrites{0};

protected:
  using Snapshot = std::map<ElementType, std::vector<Idx>>;

  Snapshot snapshot() {
    Snapshot s;
    const auto & law_index = std::as_const(*this).getConstitutiveLawByElement();
    for (auto type : law_index.elementTypes(_all_dimensions, _ghost, _ek_not_defined)) {
      const auto & a = law_index(type, _ghost);
      s[type] = std::vector<Idx>(a.data(), a.data() + a.size());
    }
    return s;
  }

  std::vector<std::map<ElementType, Idx>> filter_sizes() {
    std::vector<std::map<ElementType, Idx>> out(this->getNbMaterials());
    for (Int m = 0; m < this->getNbMaterials(); ++m) {
      const auto & f = std::as_const(this->getMaterial(m)).getElementFilter();
      for (auto type : f.elementTypes(_all_dimensions, _ghost, _ek_not_defined)) {
        out[m][type] = f(type, _ghost).size();
      }
    }
    return out;
  }

  void canonicalize(const Snapshot & before, const std::vector<std::map<ElementType, Idx>> & sizes,
                    bool with_element_synchronizer) {
    auto & mesh = this->getMesh();
    if (not mesh.isDistributed()) {
      return;
    }
    const auto & law_index = std::as_const(*this).getConstitutiveLawByElement();
    std::vector<std::map<ElementType, std::vector<Idx>>> canon(this->getNbMaterials());
    std::set<Element> placed;
    auto visit = [&](const auto & comms) {
      for (auto && [proc, scheme] : comms.iterateSchemes(_recv)) {
        for (auto && el : scheme) {
          if (el.ghost_type != _ghost or not law_index.exists(el.type, _ghost)) {
            continue;
          }
          auto it = before.find(el.type);
          Idx old = -1;
          if (it != before.end() and el.element < Idx(it->second.size())) {
            old = it->second[el.element];
          }
          Idx now = law_index(el);
          if (old != -1 or now == -1 or placed.count(el) > 0) {
            continue;
          }
          placed.insert(el);
          canon[now][el.type].push_back(el.element);
        }
      }
    };
    if (this->isNonLocal()) {
      auto & mgr = this->getNonLocalManager();
      for (const auto & name : global_names(mgr)) {
        if (const auto * g = grid_of(mgr.getNeighborhood(name))) {
          visit(g->getCommunications());
        }
      }
    }
    if (with_element_synchronizer) {
      visit(std::as_const(mesh).getElementSynchronizer().getCommunications());
    }
    auto & local_num = const_cast<ElementTypeMapArray<Idx> &>(
        std::as_const(*this).getConstitutiveLawLocalNumbering());
    for (Int m = 0; m < this->getNbMaterials(); ++m) {
      auto & filter = const_cast<ElementTypeMapArray<Idx> &>(
          std::as_const(this->getMaterial(m)).getElementFilter());
      for (auto type : filter.elementTypes(_all_dimensions, _ghost, _ek_not_defined)) {
        auto & f = filter(type, _ghost);
        Idx start = 0;
        if (auto it = sizes[m].find(type); it != sizes[m].end()) {
          start = it->second;
        }
        std::vector<Idx> suffix(f.data() + start, f.data() + f.size());
        auto & list = canon[m][type];
        std::vector<Idx> a = suffix, b = list;
        std::sort(a.begin(), a.end());
        std::sort(b.begin(), b.end());
        if (a != b) {
          throw std::runtime_error("canonicalize: appended ghost entries and receive schemes "
                                   "disagree for law " + std::to_string(m));
        }
        for (Idx i = 0; i < Idx(list.size()); ++i) {
          if (f(start + i) != list[i]) {
            ++moved;
          }
          f(start + i) = list[i];
          local_num(Element{type, list[i], _ghost}) = start + i;
        }
      }
    }
    ++rewrites;
  }

  void assignConstitutiveLawToElements(const ElementTypeMapArray<Idx> * filter) override {
    auto before = snapshot();
    auto sizes = filter_sizes();
    SolidMechanicsModel::assignConstitutiveLawToElements(filter);
    canonicalize(before, sizes, true);
  }

  void initializeNonLocal() override {
    auto before = snapshot();
    auto sizes = filter_sizes();
    SolidMechanicsModel::initializeNonLocal();
    canonicalize(before, sizes, false);
  }
};

/* -------------------------------------------------------------------------- */
/* Non-local records                                                          */
/* -------------------------------------------------------------------------- */
void record_laws(ap::FixtureWriter & w, ProbeSMM & model, const std::string & pre) {
  const auto & law_index = std::as_const(model).getConstitutiveLawByElement();
  for (auto type : law_index.elementTypes(_all_dimensions, _ghost, _ek_regular)) {
    add_i(w, pre + "/law_index/" + type_name(type) + "/ghost", ints(law_index(type, _ghost)), 1,
          "law of every ghost element (-1: none)");
  }
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    const auto & f = std::as_const(model.getMaterial(m)).getElementFilter();
    for (auto gt : ghost_types) {
      for (auto type : f.elementTypes(_all_dimensions, gt, _ek_regular)) {
        add_i(w, pre + "/law" + std::to_string(m) + "/filter/" + type_name(type) + "/" +
                     ghost_name(gt),
              ints(f(type, gt)), 1, "law element filter (ghost: rank-order rewrite, UD-232)");
      }
    }
  }
}

/// SHA-256 of the little-endian bytes of `v` (int64 or binary64 values).
template <typename T> std::string digest(const std::vector<T> & v) {
  static_assert(sizeof(T) == 8);
  std::string bytes(reinterpret_cast<const char *>(v.data()), v.size() * sizeof(T));
  return ap::Sha256::of(bytes);
}

/// Pair lists and pair weights of a neighbourhood. The lists are long (R 0.5
/// holds ~10^5 pairs), so the fixture records, per list, the number of pairs
/// and the SHA-256 of the int64 little-endian array of the pairs (q1, q2),
/// each (type enum, ghost, element, num_point, global_num), in list order,
/// and the SHA-256 of the binary64 array of the weights (w0, w1): an exact
/// comparison (the Rust tests hash the same bytes).
/// PAR4_PROBE_FULL_LISTS=1: `<scratch>/full-lists/<case>/<pre>_<what>_<ghost>.txt`
/// (`/` of `pre` as `_`), one entry per line: pairs as the ten integers of
/// the digest, weights as the two hexadecimal bit patterns.
void write_full_list(const std::string & pre, const std::string & what, const std::string & g,
                     const std::vector<std::string> & lines) {
  const char * on = std::getenv("PAR4_PROBE_FULL_LISTS");
  if (on == nullptr or std::string(on) != "1") {
    return;
  }
  std::string p = pre;
  std::replace(p.begin(), p.end(), '/', '_');
  auto dir = g_scratch / "full-lists" / ("np" + std::to_string(g_size) + "_rank" + std::to_string(g_rank));
  fs::create_directories(dir);
  std::ofstream f(dir / (p + "_" + what + "_" + g + ".txt"));
  for (const auto & l : lines) {
    f << l << "\n";
  }
}

std::string hex_bits(double v) {
  std::uint64_t b;
  std::memcpy(&b, &v, sizeof b);
  std::ostringstream o;
  o << std::hex << b;
  return o.str();
}

void record_neighborhood(ap::FixtureWriter & w, ProbeSMM & model, const std::string & name,
                         const std::string & pre, bool with_pairs) {
  auto & mgr = model.getNonLocalManager();
  auto & nb = mgr.getNeighborhood(name);
  for (auto gt : ghost_types) {
    const auto g = ghost_name(gt);
    if (with_pairs) {
      std::vector<I64> p;
      for (auto && [q1, q2] : nb.getPairLists(gt)) {
        for (const auto * q : {&q1, &q2}) {
          p.push_back(I64(q->type));
          p.push_back(I64(q->ghost_type));
          p.push_back(I64(q->element));
          p.push_back(I64(q->num_point));
          p.push_back(I64(q->global_num));
        }
      }
      add_i(w, pre + "/pairs/" + g + "/count", {I64(nb.getPairLists(gt).size())}, 1,
            "number of pairs of the list");
      w.parameters()[pre + "/pairs/" + g + "/sha256"] = digest(p);
      std::vector<std::string> lines;
      for (std::size_t k = 0; k + 10 <= p.size(); k += 10) {
        std::ostringstream o;
        for (std::size_t i = 0; i < 10; ++i) {
          o << (i ? " " : "") << p[k + i];
        }
        lines.push_back(o.str());
      }
      write_full_list(pre, "pairs", g, lines);
    }
    if (const auto * wts = pair_weights(nb, gt)) {
      const auto r = reals(*wts);
      w.parameters()[pre + "/weights/" + g + "/sha256"] = digest(r);
      std::vector<std::string> lines;
      for (std::size_t k = 0; k + 2 <= r.size(); k += 2) {
        lines.push_back(hex_bits(r[k]) + " " + hex_bits(r[k + 1]));
      }
      write_full_list(pre, "weights", g, lines);
    }
  }
  if (with_pairs) {
    if (const auto * gs = grid_of(nb)) {
      record_schemes(w, *gs, pre + "/grid");
    }
  }
}

void record_map(ap::FixtureWriter & w, const std::string & pre, const ElementTypeMapReal & m,
                Int dim) {
  for (auto gt : ghost_types) {
    for (auto type : m.elementTypes(dim, gt, _ek_regular)) {
      add_r(w, pre + "/" + type_name(type) + "/" + ghost_name(gt), reals(m(type, gt)),
            m(type, gt).getNbComponent(), "per integration point (element * nb_quad + q)");
    }
  }
}

/// Quantity `<pre>/law<m>/<id>/<type>/<ghost>` (spaces of `id` become `_`:
/// fixture names hold no whitespace).
void record_internal(ap::FixtureWriter & w, ProbeSMM & model, const std::string & id,
                     const std::string & pre, Int dim, bool ghosts = false) {
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & mat = model.getMaterial(m);
    if (not mat.isInternal<Real>(id, _ek_regular)) {
      continue;
    }
    const auto & internal = std::as_const(mat).getInternal<Real>(id);
    for (auto gt : ghost_types) {
      if (gt == _ghost and not ghosts) {
        continue;
      }
      for (auto type : internal.elementTypes(dim, gt, _ek_regular)) {
        const auto & a = internal(type, gt);
        std::string qid = id;
        std::replace(qid.begin(), qid.end(), ' ', '_');
        add_r(w, pre + "/law" + std::to_string(m) + "/" + qid + "/" + type_name(type) + "/" +
                     ghost_name(gt),
              reals(a), a.getNbComponent(), id + " per law-local element and point");
      }
    }
  }
}

/// eigen_grad_u of every owned point of every law: a polynomial of the
/// point coordinates (no transcendental function), grad_u = -eigen_grad_u
/// after computeAllStresses with u = 0.
void set_eigen_grad_u(ProbeSMM & model, Mesh & mesh, Int dim) {
  ElementTypeMapReal coords("par4_coords");
  coords.initialize(mesh, _nb_component = dim, _spatial_dimension = dim, _with_nb_element = true);
  model.getFEEngine().computeIntegrationPointsCoordinates(coords);
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & mat = model.getMaterial(m);
    const auto & filter = std::as_const(mat).getElementFilter();
    for (auto type : filter.elementTypes(dim, _not_ghost, _ek_regular)) {
      auto & eg = const_cast<Array<Real> &>(
          std::as_const(mat).getInternal<Real>("eigen_grad_u")(type, _not_ghost));
      const auto & f = filter(type, _not_ghost);
      const Idx nq = model.getFEEngine().getNbIntegrationPoints(type, _not_ghost);
      const auto & x = coords(type, _not_ghost);
      for (Idx k = 0; k < f.size(); ++k) {
        for (Idx q = 0; q < nq; ++q) {
          const Real X = x(f(k) * nq + q, 0), Y = x(f(k) * nq + q, 1);
          for (Int i = 0; i < dim; ++i) {
            for (Int j = 0; j < dim; ++j) {
              eg(k * nq + q, i * dim + j) =
                  (i + 1) * X + (j + 2) * Y * Y + 0.5 * X * Y + 0.25 * (i - j) * X * X;
            }
          }
        }
      }
    }
  }
}

/// damage 0.95 on the owned points with |x - 0.1| < 0.15 (a band crossing the
/// partition boundaries), 0 elsewhere.
void set_damage_band(ProbeSMM & model, Mesh & mesh, Int dim) {
  ElementTypeMapReal coords("par4_coords_d");
  coords.initialize(mesh, _nb_component = dim, _spatial_dimension = dim, _with_nb_element = true);
  model.getFEEngine().computeIntegrationPointsCoordinates(coords);
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    auto & mat = model.getMaterial(m);
    const auto & filter = std::as_const(mat).getElementFilter();
    for (auto type : filter.elementTypes(dim, _not_ghost, _ek_regular)) {
      auto & d = const_cast<Array<Real> &>(
          std::as_const(mat).getInternal<Real>("damage")(type, _not_ghost));
      const auto & f = filter(type, _not_ghost);
      const Idx nq = model.getFEEngine().getNbIntegrationPoints(type, _not_ghost);
      const auto & x = coords(type, _not_ghost);
      for (Idx k = 0; k < f.size(); ++k) {
        for (Idx q = 0; q < nq; ++q) {
          const Real X = x(f(k) * nq + q, 0);
          d(k * nq + q) = std::abs(X - 0.1) < 0.15 ? 0.95 : 0.;
        }
      }
    }
  }
}

/// Upstream test_pair_computation's computePairs + its checks (criteria of
/// the upstream program); returns "pass" or the failing criterion.
std::string pair_check(ProbeSMM & model, Mesh & mesh, const std::vector<std::string> & nbhs,
                       ap::FixtureWriter & w) {
  const Int dim = 2;
  ElementTypeMapReal quad_coords("quad_coords");
  quad_coords.initialize(mesh, _nb_component = dim, _spatial_dimension = dim,
                         _with_nb_element = true);
  model.getFEEngine().computeIntegrationPointsCoordinates(quad_coords);
  using PairList = std::vector<std::pair<IntegrationPoint, IntegrationPoint>>;
  std::array<PairList, 2> pair_list;
  const Real R = 0.5;
  IntegrationPoint q1, q2;
  q1.ghost_type = _not_ghost;
  for (auto type_1 : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    q1.type = type_1;
    auto nb_elements_1 = mesh.getNbElement(type_1, _not_ghost);
    auto nb_quads_1 = model.getFEEngine().getNbIntegrationPoints(type_1);
    auto coord_it_1 = quad_coords(q1.type, q1.ghost_type).begin(dim);
    for (Int e_1 = 0; e_1 < nb_elements_1; ++e_1) {
      q1.element = e_1;
      auto mat_index_1 = model.getMaterialByElement(q1.type, q1.ghost_type).begin()[q1.element];
      for (Int q_1 = 0; q_1 < nb_quads_1; ++q_1) {
        q1.global_num = nb_quads_1 * e_1 + q_1;
        q1.num_point = q_1;
        Vector<Real> c1 = coord_it_1[q1.global_num];
        for (auto ghost_type_2 : ghost_types) {
          q2.ghost_type = ghost_type_2;
          for (auto type_2 : mesh.elementTypes(dim, ghost_type_2, _ek_regular)) {
            q2.type = type_2;
            auto nb_elements_2 = mesh.getNbElement(type_2, ghost_type_2);
            auto nb_quads_2 = model.getFEEngine().getNbIntegrationPoints(type_2);
            auto coord_it_2 = quad_coords(q2.type, q2.ghost_type).begin(dim);
            for (Int e_2 = 0; e_2 < nb_elements_2; ++e_2) {
              q2.element = e_2;
              auto mat_index_2 =
                  model.getMaterialByElement(q2.type, q2.ghost_type).begin()[q2.element];
              for (Int q_2 = 0; q_2 < nb_quads_2; ++q_2) {
                q2.global_num = nb_quads_2 * e_2 + q_2;
                q2.num_point = q_2;
                Vector<Real> c2 = coord_it_2[q2.global_num];
                auto distance = c1.distance(c2);
                if (mat_index_1 != mat_index_2) {
                  continue;
                }
                if (distance <= R + Math::getTolerance() &&
                    (q2.ghost_type == _ghost ||
                     (q2.ghost_type == _not_ghost && q1.global_num <= q2.global_num))) {
                  pair_list[q2.ghost_type].push_back(std::make_pair(q1, q2));
                }
              }
            }
          }
        }
      }
    }
  }
  auto & mgr = model.getNonLocalManager();
  std::size_t ng = 0, gg = 0;
  for (const auto & n : nbhs) {
    ng += mgr.getNeighborhood(n).getPairLists(_not_ghost).size();
    gg += mgr.getNeighborhood(n).getPairLists(_ghost).size();
  }
  add_i(w, "pair_check/counts",
        {I64(ng), I64(gg), I64(pair_list[0].size()), I64(pair_list[1].size())}, 1,
        "grid not-ghost pairs, grid ghost pairs, brute-force not-ghost, brute-force ghost");
  if (ng != pair_list[0].size() or gg != pair_list[1].size()) {
    return "The number of pairs is not correct";
  }
  for (const auto & n : nbhs) {
    for (auto gt : ghost_types) {
      for (const auto & p : mgr.getNeighborhood(n).getPairLists(gt)) {
        if (std::find(pair_list[gt].begin(), pair_list[gt].end(), p) == pair_list[gt].end()) {
          return "The pairs are not correct";
        }
      }
    }
  }
  return "pass";
}

/* -------------------------------------------------------------------------- */
void scenario_nl_region() {
  const std::string scenario = "nl_region";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "test_build_neighborhood_parallel setting: two averaging test laws sharing "
                      "the remove_wf neighbourhood test_region on parallel_test.msh, rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  record_pin(w);
  const Int dim = 2;
  const auto mesh_file =
      g_src / "test/test_model/test_common/test_non_local_toolbox/parallel_test.msh";
  w.add_input_file("mesh", mesh_file,
                   "upstream:test/test_model/test_common/test_non_local_toolbox/parallel_test.msh");
  Mesh mesh(dim);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  distribute_scotch(mesh, w);
  record_mesh(w, mesh, "distributed", dim);
  parse_dat("material par4_region_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n"
            "material par4_region_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n"
            "non_local test_region remove_wf [\n  radius = 0.5\n"
            "  weight_function weight_parameter [\n    update_rate = 1\n"
            "    damage_limit = 0.9\n  ]\n]\n",
            "nl_region.dat");
  ProbeSMM model(mesh);
  auto selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  model.setMaterialSelector(selector);
  model.initFull();
  record_mesh(w, mesh, "init", dim);
  record_laws(w, model, "init");
  record_neighborhood(w, model, "test_region", "init/test_region", true);
  auto & mgr = model.getNonLocalManager();
  record_map(w, "init/volumes", mgr.getVolumes(), dim);
  record_map(w, "init/jacobians", mgr.getJacobians(), dim);
  set_eigen_grad_u(model, mesh, dim);
  model.assembleInternalForces();
  record_internal(w, model, "grad_u", "call0", dim);
  record_internal(w, model, "grad_u non local", "call0", dim);
  record_neighborhood(w, model, "test_region", "call0/test_region", false);
  record_map(w, "call0/volumes", mgr.getVolumes(), dim);
  add_r(w, "call0/internal_force", reals(model.getInternalForce()), dim, "internal force");
  set_damage_band(model, mesh, dim);
  model.assembleInternalForces();
  record_internal(w, model, "grad_u non local", "call1", dim);
  record_internal(w, model, "damage", "call1", dim);
  record_neighborhood(w, model, "test_region", "call1/test_region", false);
  record_map(w, "call1/volumes", mgr.getVolumes(), dim);
  // the number of moved entries is run-dependent (MPI_Waitany order): log only
  std::cerr << "[par4_probe] " << scenario << " rank " << g_rank << ": ghost_filter_moved "
            << model.moved << "\n";
  w.parameters()["ghost_filter_rewrites"] = static_cast<long long>(model.rewrites);
  w.finish();
}

/// nl_region with the damage_wf weight function (UD-1103).
void scenario_nl_damage_wf() {
  const std::string scenario = "nl_damage_wf";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "nl_region with the damage_wf weight function: two averaging test laws "
                      "sharing the neighbourhood test_region on parallel_test.msh, rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  record_pin(w);
  const Int dim = 2;
  const auto mesh_file =
      g_src / "test/test_model/test_common/test_non_local_toolbox/parallel_test.msh";
  w.add_input_file("mesh", mesh_file,
                   "upstream:test/test_model/test_common/test_non_local_toolbox/parallel_test.msh");
  Mesh mesh(dim);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  distribute_scotch(mesh, w);
  record_mesh(w, mesh, "distributed", dim);
  parse_dat("material par4_region_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n"
            "material par4_region_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n"
            "non_local test_region damage_wf [\n  radius = 0.5\n"
            "  weight_function weight_parameter [\n    update_rate = 1\n  ]\n]\n",
            "nl_damage_wf.dat");
  ProbeSMM model(mesh);
  auto selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  model.setMaterialSelector(selector);
  model.initFull();
  record_mesh(w, mesh, "init", dim);
  record_laws(w, model, "init");
  record_neighborhood(w, model, "test_region", "init/test_region", true);
  auto & mgr = model.getNonLocalManager();
  record_map(w, "init/volumes", mgr.getVolumes(), dim);
  set_eigen_grad_u(model, mesh, dim);
  model.assembleInternalForces();
  record_internal(w, model, "grad_u non local", "call0", dim);
  record_neighborhood(w, model, "test_region", "call0/test_region", false);
  record_map(w, "call0/volumes", mgr.getVolumes(), dim);
  set_damage_band(model, mesh, dim);
  model.assembleInternalForces();
  record_internal(w, model, "grad_u non local", "call1", dim);
  record_internal(w, model, "damage", "call1", dim);
  record_neighborhood(w, model, "test_region", "call1/test_region", false);
  record_map(w, "call1/volumes", mgr.getVolumes(), dim);
  add_r(w, "call1/internal_force", reals(model.getInternalForce()), dim, "internal force");
  std::cerr << "[par4_probe] " << scenario << " rank " << g_rank << ": ghost_filter_moved "
            << model.moved << "\n";
  w.parameters()["ghost_filter_rewrites"] = static_cast<long long>(model.rewrites);
  w.finish();
}

void scenario_nl_pair() {
  const std::string scenario = "nl_pair";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "test_pair_computation setting: two averaging test laws with their own "
                      "remove_wf neighbourhoods mat_1 / mat_2 on pair_test.msh, rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  record_pin(w);
  const Int dim = 2;
  const auto mesh_file =
      g_src / "test/test_model/test_common/test_non_local_toolbox/pair_test.msh";
  w.add_input_file("mesh", mesh_file,
                   "upstream:test/test_model/test_common/test_non_local_toolbox/pair_test.msh");
  Mesh mesh(dim);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  distribute_scotch(mesh, w);
  record_mesh(w, mesh, "distributed", dim);
  std::string wf = "remove_wf [\n  radius = 0.5\n  weight_function weight_parameter [\n"
                   "    update_rate = 1\n    damage_limit = 0.9\n  ]\n]\n";
  parse_dat("material par4_damage_material [\n  name = mat_1\n  rho = 1\n  E = 1\n  nu = 0\n]\n"
            "material par4_damage_material [\n  name = mat_2\n  rho = 1\n  E = 1\n  nu = 0\n]\n"
            "non_local mat_1 " + wf + "non_local mat_2 " + wf,
            "nl_pair.dat");
  ProbeSMM model(mesh);
  auto selector =
      std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
  model.setMaterialSelector(selector);
  model.initFull();
  record_mesh(w, mesh, "init", dim);
  record_laws(w, model, "init");
  record_neighborhood(w, model, "mat_1", "init/mat_1", true);
  record_neighborhood(w, model, "mat_2", "init/mat_2", true);
  w.parameters()["pair_check"] = pair_check(model, mesh, {"mat_1", "mat_2"}, w);
  set_eigen_grad_u(model, mesh, dim);
  model.assembleInternalForces();
  record_internal(w, model, "grad_u non local", "call0", dim);
  record_map(w, "call0/volumes", model.getNonLocalManager().getVolumes(), dim);
  // the number of moved entries is run-dependent (MPI_Waitany order): log only
  std::cerr << "[par4_probe] " << scenario << " rank " << g_rank << ": ghost_filter_moved "
            << model.moved << "\n";
  w.parameters()["ghost_filter_rewrites"] = static_cast<long long>(model.rewrites);
  w.finish();
}

void scenario_nl_mazars(int variant) {
  // variant 3: nl_mazars_two on vertical strips
  const bool strips = variant == 3;
  const bool two = variant == 1 or strips, mixed = variant == 2;
  const std::string scenario = strips ? "nl_mazars_two_strips"
                               : two  ? "nl_mazars_two"
                               : mixed ? "nl_mixed"
                                       : "nl_mazars";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      std::string("mazars_non_local on a distributed parallel_test.msh under a "
                                  "prescribed displacement history (") +
                          (two ? "two non-local laws" : mixed ? "mat_1 non-local, mat_2 elastic" : "one law") +
                          "), rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  record_pin(w);
  const Int dim = 2;
  const auto mesh_file =
      g_src / "test/test_model/test_common/test_non_local_toolbox/parallel_test.msh";
  w.add_input_file("mesh", mesh_file,
                   "upstream:test/test_model/test_common/test_non_local_toolbox/parallel_test.msh");
  Mesh mesh(dim);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  if (strips) {
    distribute_strips(mesh, w, true);
  } else {
    distribute_scotch(mesh, w);
  }
  record_mesh(w, mesh, "distributed", dim);
  const std::string elastic = "  rho = 1\n  E = 1e9\n  nu = 0.2\n  K0 = 1e-4\n";
  if (mixed) {
    parse_dat("material mazars_non_local [\n  name = mat_1\n" + elastic + "]\n"
              "material elastic [\n  name = mat_2\n  rho = 1\n  E = 1e9\n  nu = 0.2\n]\n"
              "non_local mat_1 base_wf [\n  radius = 0.3\n]\n",
              "nl_mixed.dat");
  } else if (two) {
    parse_dat("material mazars_non_local [\n  name = mat_1\n" + elastic + "]\n"
              "material mazars_non_local [\n  name = mat_2\n" + elastic +
                  "  average_on_damage = true\n]\n"
                  "non_local mat_1 base_wf [\n  radius = 0.3\n]\n"
                  "non_local mat_2 base_wf [\n  radius = 0.25\n]\n",
              "nl_mazars_two.dat");
  } else {
    parse_dat("material mazars_non_local [\n  name = body\n" + elastic + "]\n"
              "non_local body base_wf [\n  radius = 0.3\n]\n",
              "nl_mazars.dat");
  }
  ProbeSMM model(mesh);
  if (two or mixed) {
    auto selector =
        std::make_shared<MeshDataMaterialSelector<std::string>>("physical_names", model);
    model.setMaterialSelector(selector);
  }
  model.initFull();
  record_mesh(w, mesh, "init", dim);
  record_laws(w, model, "init");
  for (Int m = 0; m < model.getNbMaterials(); ++m) {
    const auto name = model.getMaterial(m).getName();
    if (mixed and m == 1) {
      continue;
    }
    record_neighborhood(w, model, name, "init/" + name, true);
  }
  std::vector<double> lambdas = {1, 2, 3, 4, 5, 6, 7, 8, 6, 9};
  w.parameters()["lambda"] = ap::Json::array_of(lambdas);
  w.parameters()["displacement"] =
      "u_x = 1e-4 * lambda * (0.6 x + 0.3 y^2), u_y = 1e-4 * lambda * (0.2 x y - 0.1 y)";
  const auto & nodes = mesh.getNodes();
  auto & u = model.getDisplacement();
  for (std::size_t s = 0; s < lambdas.size(); ++s) {
    Real l = 1e-4 * lambdas[s];
    for (Idx n = 0; n < nodes.size(); ++n) {
      Real x = nodes(n, 0), y = nodes(n, 1);
      u(n, 0) = l * (0.6 * x + 0.3 * y * y);
      u(n, 1) = l * (0.2 * x * y - 0.1 * y);
    }
    for (auto & m : model.getMaterials()) {
      m.beforeSolveStep();
    }
    model.assembleInternalForces();
    for (auto & m : model.getMaterials()) {
      m.afterSolveStep(true);
    }
    const std::string pre = "step" + std::to_string(s);
    record_internal(w, model, "damage", pre, dim);
    record_internal(w, model, "non_local_variable", pre, dim);
    add_r(w, pre + "/internal_force", reals(model.getInternalForce()), dim, "internal force");
    if (s + 1 == lambdas.size()) {
      record_internal(w, model, "Ehat", pre, dim);
      record_internal(w, model, "stress", pre, dim);
    }
  }
  // the number of moved entries is run-dependent (MPI_Waitany order): log only
  std::cerr << "[par4_probe] " << scenario << " rank " << g_rank << ": ghost_filter_moved "
            << model.moved << "\n";
  w.parameters()["ghost_filter_rewrites"] = static_cast<long long>(model.rewrites);
  w.finish();
}

/* -------------------------------------------------------------------------- */
/* Split-node contact                                                         */
/* -------------------------------------------------------------------------- */
fs::path ntn_dir() { return g_src / "extra_packages/traction-at-split-node-contact/test/test_ntn_model"; }

void record_contact(ap::FixtureWriter & w, const std::string & pre, NTNBaseContact & c, Mesh & mesh) {
  std::vector<I64> s, sg;
  for (Idx i = 0; i < c.getSlaves().getArray().size(); ++i) {
    auto n = c.getSlaves()(i);
    s.push_back(n);
    sg.push_back(mesh.isDistributed() ? mesh.getNodeGlobalId(n) : n);
  }
  add_i(w, pre + "/slaves", s, 1, "local ids of the contact nodes");
  add_i(w, pre + "/slaves_global", sg, 1, "global ids of the contact nodes");
  if (auto * ntn = dynamic_cast<NTNContact *>(&c)) {
    std::vector<I64> m, mg;
    for (Idx i = 0; i < ntn->getMasters().getArray().size(); ++i) {
      auto n = ntn->getMasters()(i);
      m.push_back(n);
      mg.push_back(mesh.isDistributed() ? mesh.getNodeGlobalId(n) : n);
    }
    add_i(w, pre + "/masters", m, 1, "local ids of the masters");
    add_i(w, pre + "/masters_global", mg, 1, "global ids of the masters");
  }
}

void record_state(ap::FixtureWriter & w, const std::string & pre, SolidMechanicsModel & model,
                  NTNBaseContact & c, NTNBaseFriction & f) {
  const Int dim = model.getSpatialDimension();
  add_r(w, pre + "/displacement", reals(model.getDisplacement()), dim, "every local node");
  add_r(w, pre + "/velocity", reals(model.getVelocity()), dim, "every local node");
  add_r(w, pre + "/contact_pressure", reals(c.getContactPressure().getArray()), dim, "contact");
  add_r(w, pre + "/normals", reals(c.getNormals().getArray()), dim, "contact");
  add_i(w, pre + "/is_in_contact", ints(c.getIsInContact().getArray()), 1, "contact");
  add_r(w, pre + "/friction_traction", reals(f.getFrictionTraction().getArray()), dim, "friction");
  add_r(w, pre + "/slip", reals(f.getSlip().getArray()), 1, "friction");
  add_i(w, pre + "/is_sticking", ints(f.getIsSticking().getArray()), 1, "friction");
}

/// Rank-specific values at every node, synchronize(tag) through the contact's
/// registry, record the arrays (u, current position, v; or the increment).
void exchange_check(ap::FixtureWriter & w, SolidMechanicsModel & model, NTNBaseContact & c,
                    Mesh & mesh) {
  const Int dim = model.getSpatialDimension();
  auto & u = model.getDisplacement();
  auto & v = model.getVelocity();
  auto & inc = model.getIncrement();
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    const Real g = mesh.isDistributed() ? Real(mesh.getNodeGlobalId(n)) : Real(n);
    for (Int d = 0; d < dim; ++d) {
      // owned values depend on the global node only; other copies carry a
      // rank-specific offset that the exchange must overwrite
      const bool owned = not mesh.isDistributed() or mesh.isLocalOrMasterNode(n);
      const Real off = owned ? 0. : 1000. * (g_rank + 1);
      u(n, d) = 1e-3 * (g + 0.5 * d) + off;
      v(n, d) = 2e-3 * (g - 0.25 * d) + off;
      inc(n, d) = 3e-3 * (g + 0.125 * d) + off;
    }
  }
  // the model's current position follows u at the next getCurrentPosition
  c.getSynchronizerRegistry().synchronize(SynchronizationTag::_cf_nodal);
  add_r(w, "exchange/cf_nodal/displacement", reals(u), dim, "after synchronize(_cf_nodal)");
  add_r(w, "exchange/cf_nodal/velocity", reals(v), dim, "after synchronize(_cf_nodal)");
  add_r(w, "exchange/cf_nodal/current_position", reals(model.getCurrentPosition()), dim,
        "getCurrentPosition() after synchronize(_cf_nodal)");
  c.getSynchronizerRegistry().synchronize(SynchronizationTag::_cf_incr);
  add_r(w, "exchange/cf_incr/increment", reals(inc), dim, "after synchronize(_cf_incr)");
  u.zero();
  v.zero();
  inc.zero();
}

void scenario_ntn_compression() {
  const std::string scenario = "ntn_compression";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "TestNTNFrictionFixture.TestCompression setting on a distributed mesh with "
                      "NTNBaseContact::initParallel(), rank view",
                      kProbe, kProbeVersion2);
  w.add_tag("parallel");
  record_pin(w);
  const auto mesh_file = ntn_dir() / "ntn_test_compression.msh";
  const auto mat_file = ntn_dir() / "data/material_ntn_friction_slip_weakening.dat";
  w.add_input_file("mesh", mesh_file,
                   "upstream:extra_packages/traction-at-split-node-contact/test/test_ntn_model/"
                   "ntn_test_compression.msh");
  w.add_input_file("material", mat_file,
                   "upstream:extra_packages/traction-at-split-node-contact/test/test_ntn_model/"
                   "data/material_ntn_friction_slip_weakening.dat");
  Mesh mesh(2);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  distribute_strips(mesh, w);
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  Real time_step_factor = getUserParser().getParameter("time_step_factor");
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, time_step_factor);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  record_mesh(w, mesh, "init", 2);
  record_contact(w, "contact", *contact, mesh);
  contact->initParallel();
  if (auto & sync = contact.get()->*get(NtnSyncTag{})) {
    record_schemes(w, *sync, "contact/filtered");
  }
  add_r(w, "contact/normals_init", reals(contact->getNormals().getArray()), 2,
        "normals after the solver callback's updateNormals (before initParallel)");
  add_r(w, "contact/impedance", reals(contact->getImpedance().getArray()), 1, "impedance");
  add_r(w, "contact/lumped_boundary_slaves",
        reals(contact->getLumpedBoundarySlaves().getArray()), 1, "lumped boundary");
  exchange_check(w, model, *contact, mesh);
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_right");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "slider_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "slider_right");
  Vector<Real> trac = {0, -1e6};
  Real time_step = model.getTimeStep();
  Real period = 5e-5;
  Vector<Real> trac_nul = {0, 0};
  const Int nb_steps = 60;
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{time_step});
  for (Int s = 0; s < nb_steps; ++s) {
    if ((s * time_step) < (period / 2)) {
      Vector<Real> trac_t = trac * std::sin(s * time_step / period * 2 * M_PI);
      model.applyBC(BC::Neumann::FromTraction(trac_t), "slider_top");
    } else {
      model.applyBC(BC::Neumann::FromTraction(trac_nul), "slider_top");
    }
    model.solveStep(solver);
    if (s % 20 == 19) {
      record_state(w, "step" + std::to_string(s), model, *contact, *friction);
    }
  }
  const auto & rank_work = solver.*get(NtnWorkTag{});
  add_r(w, "final/external_work_rank", {rank_work(0), rank_work(1)}, 2,
        "this rank's accumulated work before the allReduce (probe version 2)");
  auto work = solver.getExternalWork();
  add_r(w, "final/external_work", {work[0], work[1]}, 2, "allReduce(_sum) of the work");
  add_r(w, "final/global_contact_pressure", reals(contact->getGlobalContactPressure()), 2,
        "every local node");
  w.finish();
}

/// ntn_compression's setting without the exchange check, 40 steps, then
/// NTNContact::updateNormals() (it starts with synchronize(_cf_nodal)).
void scenario_ntn_update_normals() {
  const std::string scenario = "ntn_update_normals";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "TestNTNFrictionFixture.TestCompression setting on a distributed mesh with "
                      "NTNBaseContact::initParallel(), 40 steps, then NTNContact::updateNormals(), "
                      "rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  record_pin(w);
  const auto mesh_file = ntn_dir() / "ntn_test_compression.msh";
  const auto mat_file = ntn_dir() / "data/material_ntn_friction_slip_weakening.dat";
  w.add_input_file("mesh", mesh_file,
                   "upstream:extra_packages/traction-at-split-node-contact/test/test_ntn_model/"
                   "ntn_test_compression.msh");
  w.add_input_file("material", mat_file,
                   "upstream:extra_packages/traction-at-split-node-contact/test/test_ntn_model/"
                   "data/material_ntn_friction_slip_weakening.dat");
  Mesh mesh(2);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  distribute_strips(mesh, w);
  getStaticParser().parse(mat_file.string());
  SolidMechanicsModel model(mesh);
  Real time_step_factor = getUserParser().getParameter("time_step_factor");
  NTNContactSolverCallback solver(model, "slider_bottom", "base_top", 1, time_step_factor);
  auto contact = solver.getContact();
  auto friction = solver.getFriction();
  record_contact(w, "contact", *contact, mesh);
  contact->initParallel();
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_right");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "slider_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "slider_right");
  Vector<Real> trac = {0, -1e6};
  Real time_step = model.getTimeStep();
  Real period = 5e-5;
  Vector<Real> trac_nul = {0, 0};
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{time_step});
  for (Int s = 0; s < 40; ++s) {
    if ((s * time_step) < (period / 2)) {
      Vector<Real> trac_t = trac * std::sin(s * time_step / period * 2 * M_PI);
      model.applyBC(BC::Neumann::FromTraction(trac_t), "slider_top");
    } else {
      model.applyBC(BC::Neumann::FromTraction(trac_nul), "slider_top");
    }
    model.solveStep(solver);
    if (s % 20 == 19) {
      record_state(w, "step" + std::to_string(s), model, *contact, *friction);
    }
  }
  contact->updateNormals();
  add_r(w, "update_normals/normals", reals(contact->getNormals().getArray()), 2,
        "normals after updateNormals() (synchronize(_cf_nodal) first)");
  add_r(w, "update_normals/displacement", reals(model.getDisplacement()), 2,
        "every local node, after updateNormals()");
  add_r(w, "update_normals/current_position", reals(model.getCurrentPosition()), 2,
        "every local node, after updateNormals()");
  w.finish();
}

/// Probe copy of NTNContactSolverCallback::assembleResidual for an NTRF
/// contact (as oracle/probes/ntn_contact case_ntrf).
class NtrfCallback : public InterceptSolverCallback {
public:
  NtrfCallback(SolidMechanicsModel & solid, NTNBaseContact & contact, NTNBaseFriction & friction)
      : InterceptSolverCallback(solid), solid(solid), contact(contact), friction(friction) {}
  void assembleResidual() override {
    auto && dof_manager = solid.getDOFManager();
    aka::as_type<SolverCallback>(solid).assembleResidual();
    contact.computeContactPressure();
    friction.updateSlip();
    friction.computeFrictionTraction();
    contact.assembleGlobalContactPressure();
    friction.assembleGlobalFrictionTraction();
    dof_manager.assembleToResidual("displacement", contact.getGlobalContactPressure(), 1);
    dof_manager.assembleToResidual("displacement", friction.getGlobalFrictionTraction(), 1);
  }
  void beforeSolveStep() override {
    aka::as_type<SolverCallback>(solid).beforeSolveStep();
    friction.savePreviousState();
    contact.savePreviousState();
  }
  void afterSolveStep(bool converged = true) override {
    aka::as_type<SolverCallback>(solid).afterSolveStep(converged);
  }

private:
  SolidMechanicsModel & solid;
  NTNBaseContact & contact;
  NTNBaseFriction & friction;
};

void scenario_ntrf() {
  const std::string scenario = "ntrf_coulomb";
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "NTRFContact (rigid plane y = 0) with NTRFFriction (coulomb) on the slider of "
                      "ntn_test_compression.msh, distributed, initParallel(), rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  record_pin(w);
  const auto mesh_file = ntn_dir() / "ntn_test_compression.msh";
  w.add_input_file("mesh", mesh_file,
                   "upstream:extra_packages/traction-at-split-node-contact/test/test_ntn_model/"
                   "ntn_test_compression.msh");
  Mesh mesh(2);
  if (g_rank == 0) {
    mesh.read(mesh_file.string());
  }
  distribute_strips(mesh, w);
  parse_dat("friction coulomb rubin_ampuero [\n mu = 0.2\n t_star = 2e-5\n]\n"
            "material elastic [\n  name = slider\n  rho = 1200\n  nu = 0.33\n  E = 7.98e9\n"
            "  Plane_Stress = 1\n]\n"
            "material elastic [\n  name = base\n  rho = 1200\n  nu = 0.33\n  E = 7.98e9\n"
            "  Plane_Stress = 1\n]\n",
            "ntrf.dat");
  SolidMechanicsModel model(mesh);
  auto contact = std::make_shared<NTRFContact>(model);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  model.assembleMassLumped();
  contact->setReferencePoint(0., 0., 0.);
  contact->setNormal(0., 1., 0.);
  contact->addSurface("slider_bottom");
  Real dt = model.getStableTimeStep() * 0.1;
  model.setTimeStep(dt);
  contact->updateImpedance();
  auto friction = initializeNTNFriction(*contact);
  record_mesh(w, mesh, "init", 2);
  record_contact(w, "contact", *contact, mesh);
  contact->initParallel();
  if (auto & sync = contact.get()->*get(NtnSyncTag{})) {
    record_schemes(w, *sync, "contact/filtered");
  }
  exchange_check(w, model, *contact, mesh);
  NtrfCallback cb(model, *contact, *friction);
  model.applyBC(BC::Dirichlet::FlagOnly(_y), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_bottom");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_left");
  model.applyBC(BC::Dirichlet::FlagOnly(_x), "base_right");
  model.applyBC(BC::Neumann::FromTraction(Vector<Real>{0., -1e6}), "slider_top");
  w.add("time_step", {1}, {"entry"}, "table", std::vector<double>{dt});
  for (Int s = 0; s < 60; ++s) {
    if (s >= 40) {
      model.applyBC(BC::Neumann::FromTraction(Vector<Real>{4e3, 0.}), "slider_top");
    }
    model.solveStep(cb);
    if (s % 20 == 19) {
      record_state(w, "step" + std::to_string(s), model, *contact, *friction);
    }
  }
  w.finish();
}

} // namespace

int main(int argc, char * argv[]) {
  Eigen::setCpuCacheSizes(kEigenL1, kEigenL2, kEigenL3);
  if (argc < 5) {
    std::cerr << "usage: par4_probe <fixtures_root> <upstream_src_root> <scratch> <scenario>\n";
    return 2;
  }
  g_root = argv[1];
  g_src = argv[2];
  g_scratch = argv[3];
  const std::string scenario = argv[4];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  const auto & comm = Communicator::getStaticCommunicator();
  g_rank = comm.whoAmI();
  g_size = comm.getNbProc();
  try {
    if (scenario == "nl_region") {
      scenario_nl_region();
    } else if (scenario == "nl_pair") {
      scenario_nl_pair();
    } else if (scenario == "nl_mazars") {
      scenario_nl_mazars(0);
    } else if (scenario == "nl_mazars_two") {
      scenario_nl_mazars(1);
    } else if (scenario == "nl_mixed") {
      scenario_nl_mazars(2);
    } else if (scenario == "ntn_compression") {
      scenario_ntn_compression();
    } else if (scenario == "ntrf_coulomb") {
      scenario_ntrf();
    } else if (scenario == "nl_damage_wf") {
      scenario_nl_damage_wf();
    } else if (scenario == "nl_mazars_two_strips") {
      scenario_nl_mazars(3);
    } else if (scenario == "ntn_update_normals") {
      scenario_ntn_update_normals();
    } else {
      std::cerr << "par4_probe: unknown scenario " << scenario << "\n";
      return 2;
    }
  } catch (std::exception & e) {
    std::cerr << "par4_probe (rank " << g_rank << ", " << scenario << "): " << e.what() << "\n";
    return 1;
  }
  finalize();
  return 0;
}
