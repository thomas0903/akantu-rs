// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): centralised mesh distribution
// (Mesh::distribute with a seeded MeshPartitionScotch, mesh_utils_distribution,
// element/node info per processor) and SolidMechanicsModel on distributed
// meshes. Run under `mpirun -np P`; every rank writes its own case
//   parallel/<scenario>/np<P>/rank<r>
// with what that rank holds after distribution (connectivities in local
// node ids, node global ids / flags / owner ranks / coordinates, element and
// node synchronizer schemes, ghost counters, groups, mesh data) and, for the
// model scenarios, the nodal fields after the run. Rank 0 also records the
// Scotch partition (linearised parts of the elements of the mesh dimension:
// the input of the Rust PrescribedPartition) and the parts of every type.
//
// Scenarios:
//   distribution_cube_msh   frozen test/test_synchronizer/cube.msh (3-D,
//                           Tetrahedron10 + Triangle6 + Segment3)
//   smm_explicit_cube_n4    structured Tet4 cube (n = 4, Kuhn split), elastic
//                           (E = 1, nu = 0.3, rho = 1), z = 0 clamped,
//                           initial bending displacement, explicit lumped
//                           mass, dt = 0.8 getStableTimeStep, 40 steps
//   smm_static_cube_n3      Tet4 cube n = 3, z = 0 clamped, top load, static
//                           (default solver: MUMPS when parallel; 1 and 2
//                           processes only: MUMPS on 4 is not reproducible)
//   distribution_adv2d the review's adversarial 2-D mesh (adv2d.msh,
//                           written by make_adv2d.py: interleaved Quad4/Tri3,
//                           Seg2 groups incl. an interior crack, Point1 groups
//                           incl. an orphan point on no segment) distributed
//                           with MeshPartitionMeshData and a cell rule
//                           (non-contiguous parts at 2, a triangles-only rank
//                           at 4): records upstream's orphan-element
//                           placement (UD-231)
//
// scenarios (probe version 2; distributed facets and the remaining
// entity synchronizers):
//   facets_cube_msh         cube.msh distributed as distribution_cube_msh, then
//                           Mesh::initMeshFacets (buildAllFacets on a distributed
//                           mesh, FacetSynchronizer, _smmc_facets_conn exchange,
//                           flipFacets of the ghost facets): facet connectivities
//                           per type and ghost type, element_to_subelement,
//                           subelement_to_element, facet synchronizer schemes,
//                           parent mesh_to_mesh_facet
//   facets_cube_n4          the same on the structured Tet4 cube (n = 4)
//   facets_adv2d            the same on distribution_adv2d (2-D mixed Quad4/Tri3)
//   grid_tri_n8             structured Tri3 square (n = 8, h = 1/8) distributed with
//                           a seeded Scotch partition; SpatialGrid<Element> of the
//                           local barycenters (spacing 1.2 R, R = 0.2, centred on
//                           the mesh box), GridSynchronizer(mesh, grid): grid
//                           schemes, the mesh after the new ghost layer, grid
//                           bounds (modern-API port of the stale, unregistered
//                           test_grid_synchronizer.cc)
//   grid_cube_n4            the same on the Tet4 cube n = 4 with R = 0.3
//   facets_hex_prism        (selected explicitly only) UD-251: a hexahedron next
//                           to two prisms on another rank — upstream's
//                           FacetSynchronizer does not complete
//   periodic_<d>d_<dirs>    TestMeshPeriodic (test_mesh_periodic.cc): line /
//                           square / cube.msh distributed (seeded Scotch), then
//                           makePeriodic per direction on the distributed mesh:
//                           nodes (incl. the new pure-ghost periodic nodes), sorted
//                           periodic pairs, periodic and node synchronizer
//                           schemes, makeConsistentForPeriodicity _id and _sum
//                           arrays of the upstream test
//
// scenarios and records (probe version 3):
//   grid_* and periodic_* additionally record the mesh's local and global
//                           bounding boxes before and after the operation
//                           (`mesh/bbox_before`, `mesh/bbox_after`: upstream's
//                           sendEvent<NewNodesEvent> recomputes them);
//                           periodic_* also record the unordered_multimap
//                           iteration order of the periodic pairs and
//                           makeConsistentForPeriodicity _id / _sum of
//                           non-dyadic data
//   v101_facets_{tri,tet}_quadrants  initMeshFacets on a quadrant partition
//                           (facets seen as ghosts of two other ranks)
//   v101_grid_wide          GridSynchronizer, Tri3 n = 8 in four stripes,
//                           R = 0.45 (neighbourhood wider than a rank's halo)
//   v101_grid_adv2d         GridSynchronizer on the mixed Quad4/Tri3 adv2d mesh
//   v101_periodic_{2d_x,2d_xy,3d_xyz}  makePeriodic with a parity partition
//                           (every periodic pair crosses a rank boundary),
//                           non-dyadic makeConsistentForPeriodicity
//   v101_empty_rank_*       (selected explicitly only) a partition leaving
//                           rank 3 empty: upstream's MeshPartition refuses it
//
// Scenarios (probe version 4):
//   distribution_square_data   frozen test_mesh_periodic/square.msh with the
//                           nodal data "shared" (TestMeshPeriodic) and
//                           "node_tag" set on rank 0: every rank's
//                           physical_names, recomputed element_to_subelement
//                           / subelement_to_element and nodal data
//   distribution_square_periodic (selected explicitly only) the same mesh
//                           made periodic in x on rank 0 before the
//                           distribution: upstream aborts at 2 ranks in
//                           MasterNodeInfoPerProc::synchronizePeriodicity
//                           ("double free or corruption", also with x and
//                           y), so it is a reproduction of UD-655 only
//
// records (probe version 4, D-039
// amendment): periodic_* and v101_periodic_* additionally record, for the
// non-dyadic makeConsistentForPeriodicity _sum, the per-rank contributions
// (zero_pure_ghosts, then upstream's own local reduction
// reduceSynchronizeArrayLocal<AddOperation>: the value every rank holds at
// every periodic master copy before the ghost -> owner reduction), the
// exact mask of the run-dependent entries (node, component: >= 2 distinct
// results over the arrival orders of the remote contributions) and the
// _sum with only those entries recorded as 0. Every version-3 payload is
// written unchanged. PARALLEL_PROBE_UNMASKED_SUM=1 (variation study only,
// never forwarded by generate.sh) also writes the unmasked _sum.
//
// Scenarios (probe version 5; the remaining models on distributed
// meshes; every version-1 to 4 payload is written unchanged):
//   clh_marigo_cube_n3      Tet4 cube n = 3 (seeded Scotch), Marigo with a random
//                           threshold (Yd = 1e-7 uniform [0, 1e-7], Sd = 1e-5),
//                           RandomGenerator<Idx> seeded with kSeedLaw * (rank + 1)
//                           before the model: law filters (ghost: canonical rank
//                           order, the UD-232 probe patch; Yd is position-indexed,
//                           see scenario_clh_marigo), Yd of both ghost types after
//                           initFull (upstream's synchronize(_clh_init_cl) has no
//                           registered synchronizer: the ghost Yd are the local
//                           draws, UD-900), then 20 explicit steps from the bending
//                           mode of smm_explicit_cube_n4: displacement, velocity,
//                           damage
//   diffusion_gradient_cube_n3  HeatTransferModel on the same distributed cube, a
//                           non-linear temperature field on every local node,
//                           computeDiffusivityGradU(_not_ghost), then
//                           synchronize(_diffusion_gradient): the law's grad u of
//                           the not-ghost elements and of every ghost element (in
//                           mesh order) before and after the exchange (MPI-043; a
//                           residual assembly is not possible upstream, UD-275)
//   phase_field_square_n8   phase_field_parallel (examples/c++/phase_field_model)
//                           reduced: structured Tri3 square n = 8 (seeded Scotch),
//                           CouplerSolidPhaseField with material_notch.dat, both
//                           models static (one rank: _eigen pinned; more ranks:
//                           the default solver, MUMPS, _eigen is not distributed),
//                           y = 0 / y = 1 blocked in y, x = 0 blocked in x, 3 load
//                           steps of 2e-3 on y = 1, 2 coupler.solve() each: law
//                           filters (ghost: canonical rank order for both models,
//                           checked position-independent), displacement, nodal
//                           damage, the phase field's damage / phi / driving force
//                           of the not-ghost points, per-rank energy contributions
//                           (D-039 amendment) and the reduced energies
//
// Scenario (probe version 5), selected
// explicitly only (it re-parses the static parser's material file):
//   smm_explicit_two_laws_cube_n4  the smm_explicit_cube_n4 run with two
//                           elastic laws, law 0 "small" (rho 1.5, E 2,
//                           nu 0.25) and law 1 "fd" (rho 1, E 1, nu 0.3,
//                           finite_deformation = true), elements chosen by a
//                           2 x 2 x 2 checkerboard of the barycentre
//                           (floor(2 x) + floor(2 y) + floor(2 z)) mod 2:
//                           both laws' ghost filters (canonical rank order,
//                           as smm_explicit_cube_n4), the nodal fields and
//                           energies after 40 steps, and every law's stress,
//                           grad_u (and law 1's piola_kirchhoff_2) on local
//                           and ghost elements after the last step (the
//                           ghost values are the owner's of the last
//                           _smm_stress exchange)
//
// scenarios (probe version 6):
//   pbd_strip_scotch        pstrip.msh (make_pstrip.py: Tri3 strip 24 x 4,
//                           Seg2 boundary groups) made periodic in x on rank 0
//                           BEFORE the distribution, seeded Scotch partition
//                           (MeshPartitionScotch's periodic branch): rank 0's
//                           partition and ghost partitions, the UD-1060
//                           precondition (no rank whose entry of
//                           synchronizePeriodicity's vector is relocated holds
//                           a periodic slave), then, when it holds, every
//                           rank's distribution, physical names, adjacency
//                           and periodic pairs (slave/master, multimap order)
//   pbd_strip_mesh_data     the same with a MeshPartitionMeshData partition
//                           (periodic tweak; at 4 ranks rank 3 receives
//                           missing periodic partners)
//   distribution_sparse_mixed  sparse2d.msh (make_sparse2d.py: sparse, shuffled
//                           node and element tags, Quad4 + Tri3 + Seg2 +
//                           Point1, ten physical groups) with an Int nodal
//                           datum, seeded Scotch, recorded like
//                           distribution_square_data
//
// PARALLEL_PROBE_SCENARIOS (comma-separated scenario names, default: all)
// selects scenarios.
//
// Usage: parallel_probe <fixtures_root> <frozen_meshes_root> <scratch_dir>
//                       [<adv2d.msh>]
// Scotch is seeded with RandomGenerator<Idx>::seed(kSeed) on rank 0 before
// the partitioner is built (upstream: AKA_SEED * (rank + 1)).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_random_generator.hh>
#include <communicator.hh>
#include <element_group.hh>
#include <aka_grid_dynamic.hh>
#include <element_synchronizer.hh>
#include <facet_synchronizer.hh>
#include <grid_synchronizer.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_partition_mesh_data.hh>
#include <mesh_partition_scotch.hh>
#include <mesh_utils.hh>
#include <node_group.hh>
#include <node_synchronizer.hh>
#include <periodic_node_synchronizer.hh>
#include <solid_mechanics_model.hh>
#include <heat_transfer_model.hh>
#include <coupler_solid_phasefield.hh>

extern "C" {
#include <scotch.h>
}

#include <cstdint>
#include <cstring>
#include <functional>
#include <cstdio>
#include <cmath>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <algorithm>
#include <map>
#include <set>
#include <stdexcept>
#include <sstream>
#include <string>
#include <utility>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "parallel_probe";
constexpr const char * kProbeVersion = "1";
// scenarios (the version-1 scenarios are unchanged).
constexpr const char * kProbeVersion2 = "2";
// Grid / periodic scenarios with bounding boxes, non-dyadic
// periodic sums and the multimap order; the adversarial periodic scenarios.
constexpr const char * kProbeVersion3 = "3";
// Scenarios distribution_square_data and
// distribution_square_periodic (the version-1 to 3 scenarios are unchanged).
// and, same version number (both rounds were cut from version 3 and merged
// together; no payload of one depends on the other):
// The periodic scenarios with the per-rank contributions and
// the exact run-dependent mask of the non-dyadic _sum (version-3 payloads
// unchanged).
constexpr const char * kProbeVersion4 = "4";
// The remaining models on distributed meshes (Marigo _clh_init_cl,
// _diffusion_gradient, the phase-field coupler) and the two-law scenario
// smm_explicit_two_laws_cube_n4 (two independent additions under the same
// version, merged; the version-1 to 4 payloads are unchanged).
constexpr const char * kProbeVersion5 = "5";
// Scenarios pbd_strip_scotch, pbd_strip_mesh_data (a mesh
// made periodic before the distribution) and
// distribution_sparse_mixed; earlier payloads unchanged.
constexpr const char * kProbeVersion6 = "6";
// version 7: the scenario
// ud1060_vector_growth (one process): the relocations of
// synchronizePeriodicity's per-rank vector observed on the oracle's
// toolchain, besides the formula ud1060_relocated_ranks
constexpr const char * kProbeVersion7 = "7";
/// Seed of the laws' random fields (rank r: kSeedLaw * (r + 1), upstream's
/// initialize policy for AKANTU_SEED).
constexpr long kSeedLaw = 4242;
// D-050 pin of the oracle's Eigen cache sizes (applied at the start of main;
// recorded in every version-3 case with OPENBLAS_CORETYPE).
constexpr int kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;
constexpr long kSeed = 1234;
using I64 = std::int64_t;

fs::path g_root, g_frozen, g_scratch, g_adv2d;
Int g_rank = 0, g_size = 1;

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string ghost_name(GhostType g) { return g == _not_ghost ? "not_ghost" : "ghost"; }

/// Element type table (codes of the element triples).
struct TypeTable {
  std::vector<std::string> names;
  I64 code(ElementType t) {
    auto n = type_name(t);
    for (std::size_t i = 0; i < names.size(); ++i) {
      if (names[i] == n) {
        return static_cast<I64>(i);
      }
    }
    names.push_back(n);
    return static_cast<I64>(names.size() - 1);
  }
};

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

/// Local and global bounding boxes of the mesh (`bbox_local`, `bbox`):
/// local lower, local upper, global lower, global upper (one row each).
void record_bbox(ap::FixtureWriter & w, Mesh & mesh, const std::string & name) {
  const Int dim = mesh.getSpatialDimension();
  std::vector<double> v;
  for (Int i = 0; i < dim; ++i) v.push_back(mesh.getLocalLowerBounds()(i));
  for (Int i = 0; i < dim; ++i) v.push_back(mesh.getLocalUpperBounds()(i));
  for (Int i = 0; i < dim; ++i) v.push_back(mesh.getLowerBounds()(i));
  for (Int i = 0; i < dim; ++i) v.push_back(mesh.getUpperBounds()(i));
  add_r(w, name, v, dim, "local lower, local upper, global lower, global upper corner");
}

/// Provenance of the D-050 pin (version-3 cases).
void record_pin(ap::FixtureWriter & w) {
  w.parameters()["oracle_pin/eigen_cache_sizes"] = std::to_string(kEigenL1) + " " +
                                                   std::to_string(kEigenL2) + " " +
                                                   std::to_string(kEigenL3);
  const char * coretype = std::getenv("OPENBLAS_CORETYPE");
  w.parameters()["oracle_pin/openblas_coretype"] = std::string(coretype ? coretype : "unset");
  const char * threads = std::getenv("OPENBLAS_NUM_THREADS");
  w.parameters()["oracle_pin/openblas_num_threads"] = std::string(threads ? threads : "unset");
}

/// Global ids of the periodic masters whose `_sum` upstream accumulates
/// from two or more remote ranks. The owner receives the contributions of
/// `reduceSynchronizeArray` in `SynchronizerImpl::communicateOnce`
/// (`synchronizer_impl_tmpl.hh:124-145`, `Communicator::waitAny`), i.e. in
/// arrival order; with at most one remote contribution the sum is
/// deterministic. This structural rule (probe version 3) is a superset of
/// the run-dependent masters: a zero contribution, or contributions that
/// commute exactly, leave the sum deterministic (the
/// exact set is `run_dependent_masters`, version 4). Every rank returns the
/// same set (all-gather).
std::set<Idx> order_dependent_masters(Mesh & mesh) {
  const auto & comms = std::as_const(mesh).getPeriodicNodeSynchronizer().getCommunications();
  std::map<Idx, Int> count;
  for (auto && [proc, scheme] : comms.iterateSchemes(_send)) {
    for (auto node : scheme) {
      ++count[node];
    }
  }
  std::vector<Idx> mine;
  for (auto && [node, c] : count) {
    if (c >= 2) {
      mine.push_back(mesh.getNodeGlobalId(node));
    }
  }
  const auto & comm = Communicator::getStaticCommunicator();
  Array<Int> sizes(g_size, 1);
  sizes.set(0);
  sizes(g_rank) = Int(mine.size());
  comm.allGather(sizes);
  Int total = 0, offset = 0;
  for (Int p = 0; p < g_size; ++p) {
    if (p < g_rank) offset += sizes(p);
    total += sizes(p);
  }
  std::set<Idx> out;
  if (total == 0) {
    return out;
  }
  Array<Idx> all(total, 1);
  all.set(-1);
  for (std::size_t i = 0; i < mine.size(); ++i) {
    all(offset + Int(i)) = mine[i];
  }
  comm.allGatherV(all, sizes);
  for (Int i = 0; i < total; ++i) {
    out.insert(all(i));
  }
  return out;
}

/// Upstream's private `PeriodicNodeSynchronizer::reduceSynchronizeArrayLocal
/// <AddOperation, Real>` (`periodic_node_synchronizer_tmpl.hh`: local slaves
/// -> masters, `slaves_list` packed before `masters_list` is unpacked), the
/// local part of `reduceSynchronizeArrayPBC`, called on the mesh's own
/// synchronizer without modifying the reference: names in the arguments of
/// an explicit instantiation are exempt from access checking
/// ([temp.spec], C++20), and the friend defined by the instantiated
/// class returns the member pointer.
using LocalReduceFn = void (PeriodicNodeSynchronizer::*)(Array<Real> &) const;
struct LocalReduceTag {
  friend LocalReduceFn local_reduce_member(LocalReduceTag);
};
template <LocalReduceFn F> struct LocalReduceAccess {
  friend LocalReduceFn local_reduce_member(LocalReduceTag) { return F; }
};
template struct LocalReduceAccess<
    &PeriodicNodeSynchronizer::reduceSynchronizeArrayLocal<AddOperation, Real>>;

std::uint64_t real_bits(Real x) {
  std::uint64_t b = 0;
  static_assert(sizeof(b) == sizeof(x));
  std::memcpy(&b, &x, sizeof(b));
  return b;
}

/// Per-rank contributions of upstream's distributed `_sum`: `zero_pure_ghosts`
/// (a lambda of `Mesh::makeConsistentForPeriodicity`, `mesh_periodic.cc:465-478`,
/// not callable: same flag rule) followed by upstream's own
/// `reduceSynchronizeArrayLocal<AddOperation>`. At every periodic master copy
/// this is what the rank sends to the owner in the ghost -> owner reduction
/// of `reduceSynchronizeArray` (or, on the owner, the value the remote
/// contributions are added to).
Array<Real> periodic_sum_contributions(Mesh & mesh, const Array<Real> & a) {
  Array<Real> loc = a;
  const auto remote = NodeFlag::_periodic | NodeFlag::_pure_ghost | NodeFlag::_slave;
  const auto ghost = NodeFlag::_periodic | NodeFlag::_pure_ghost;
  const auto slave = NodeFlag::_periodic | NodeFlag::_slave;
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    const auto f = mesh.getNodeFlag(n) & remote;
    if (f == slave or f == ghost) {
      for (Int c = 0; c < loc.getNbComponent(); ++c) {
        loc(n, c) = Real();
      }
    }
  }
  const auto & sync = std::as_const(mesh).getPeriodicNodeSynchronizer();
  (sync.*local_reduce_member(LocalReduceTag{}))(loc);
  return loc;
}

/// The run-dependent masters of upstream's distributed `_sum`, per component
/// (D-039 amendment). The owner of a periodic master
/// adds the remote contributions in `Communicator::waitAny` arrival order
/// (`SynchronizerImpl::communicateOnce`, `synchronizer_impl_tmpl.hh:124-145`;
/// slave reduction: the owner receives on its send schemes, every other rank
/// sends its receive-scheme entries). Every rank contributes, for every node
/// of its periodic schemes, a row (kind, global id, rank, other rank, values):
/// kind 0 = the owner's own value and a contributor it expects (send scheme
/// toward `other`), kind 1 = a contribution to the owner `other` (receive
/// scheme from `other`); the rows are all-gathered. A master's component is
/// run-dependent iff the fold `own + c_q1 + c_q2 + ...` over every order of
/// its remote contributions gives >= 2 distinct results (bitwise). Every rank
/// returns the same sets. Throws if the schemes are not symmetric.
std::vector<std::set<Idx>> run_dependent_masters(Mesh & mesh, const Array<Real> & loc) {
  const Int nbc = loc.getNbComponent();
  const auto & comms = std::as_const(mesh).getPeriodicNodeSynchronizer().getCommunications();
  const Int row = 4 + nbc;
  std::vector<Real> mine;
  auto push = [&](Real kind, Idx node, Int other) {
    mine.push_back(kind);
    mine.push_back(Real(mesh.getNodeGlobalId(node)));
    mine.push_back(Real(g_rank));
    mine.push_back(Real(other));
    for (Int c = 0; c < nbc; ++c) {
      mine.push_back(loc(node, c));
    }
  };
  for (auto && [proc, scheme] : comms.iterateSchemes(_send)) {
    for (auto node : scheme) {
      push(0., node, proc);
    }
  }
  for (auto && [proc, scheme] : comms.iterateSchemes(_recv)) {
    for (auto node : scheme) {
      push(1., node, proc);
    }
  }
  const auto & comm = Communicator::getStaticCommunicator();
  Array<Int> sizes(g_size, 1);
  sizes.set(0);
  sizes(g_rank) = Int(mine.size());
  comm.allGather(sizes);
  Int total = 0, offset = 0;
  for (Int p = 0; p < g_size; ++p) {
    if (p < g_rank) offset += sizes(p);
    total += sizes(p);
  }
  std::vector<std::set<Idx>> out(nbc);
  if (total == 0) {
    return out;
  }
  Array<Real> all(total, 1);
  all.set(0.);
  for (std::size_t i = 0; i < mine.size(); ++i) {
    all(offset + Int(i)) = mine[i];
  }
  comm.allGatherV(all, sizes);
  struct Master {
    Int owner = -1;
    std::vector<Real> own;
    std::set<Int> expected;
    std::map<Int, std::vector<Real>> remote;
    std::map<Int, Int> remote_to;
  };
  std::map<Idx, Master> masters;
  for (Int r = 0; r < total / row; ++r) {
    const Real * v = &all(r * row);
    const auto gid = Idx(v[1]);
    const auto rank = Int(v[2]);
    const auto other = Int(v[3]);
    std::vector<Real> vals(v + 4, v + row);
    auto & m = masters[gid];
    if (v[0] == 0.) {
      if ((m.owner != -1 and m.owner != rank) or
          (not m.own.empty() and m.own != vals)) {
        throw std::runtime_error("run_dependent_masters: inconsistent owner rows of gid " +
                                 std::to_string(gid));
      }
      m.owner = rank;
      m.own = vals;
      m.expected.insert(other);
    } else {
      if (m.remote.count(rank) != 0) {
        throw std::runtime_error("run_dependent_masters: two contributions of rank " +
                                 std::to_string(rank) + " to gid " + std::to_string(gid));
      }
      m.remote[rank] = vals;
      m.remote_to[rank] = other;
    }
  }
  for (auto && [gid, m] : masters) {
    std::set<Int> senders;
    for (auto && [q, to] : m.remote_to) {
      senders.insert(q);
      if (to != m.owner) {
        throw std::runtime_error("run_dependent_masters: rank " + std::to_string(q) +
                                 " contributes gid " + std::to_string(gid) + " to rank " +
                                 std::to_string(to) + ", owner " + std::to_string(m.owner));
      }
    }
    if (m.owner == -1 or senders != m.expected) {
      throw std::runtime_error("run_dependent_masters: periodic schemes not symmetric at gid " +
                               std::to_string(gid));
    }
    std::vector<Int> order(senders.begin(), senders.end());
    if (order.size() > 8) {
      throw std::runtime_error("run_dependent_masters: too many contributors");
    }
    for (Int c = 0; c < nbc; ++c) {
      std::set<std::uint64_t> results;
      auto perm = order;
      do {
        Real s = m.own[c];
        for (auto q : perm) {
          s += m.remote[q][c];
        }
        results.insert(real_bits(s));
      } while (std::next_permutation(perm.begin(), perm.end()));
      if (results.size() > 1) {
        out[c].insert(gid);
      }
    }
  }
  return out;
}

/// Version-4 records of the non-dyadic `_sum` (D-039
/// amendment): the per-rank contributions (`periodic_sum_contributions`), the
/// exact mask of the run-dependent entries (node, component; a node takes its
/// value from the master copy with its global id, or from its local master)
/// and the `_sum` with only those entries recorded as 0. `sum` is the
/// unmasked result of makeConsistentForPeriodicity(_sum) on `a`.
void record_periodic_sum_run_dependent(ap::FixtureWriter & w, Mesh & mesh,
                                       const std::string & prefix, const Array<Real> & a,
                                       const Array<Real> & sum) {
  const auto nb = mesh.getNbNodes();
  const Int nbc = a.getNbComponent();
  const auto contrib = periodic_sum_contributions(mesh, a);
  add_r(w, prefix + "nondyadic_sum_contributions", reals(contrib), nbc,
        "zero_pure_ghosts then upstream's reduceSynchronizeArrayLocal<AddOperation> (local "
        "slaves -> masters in multimap order): at every periodic master copy, this rank's "
        "contribution to the ghost -> owner reduction (on the owner: the value the remote "
        "contributions are added to)");
  const auto dependent = run_dependent_masters(mesh, contrib);
  std::vector<I64> mask(nb * nbc, 0);
  for (Idx n = 0; n < nb; ++n) {
    for (Int c = 0; c < nbc; ++c) {
      if (dependent[c].count(mesh.getNodeGlobalId(n)) != 0) {
        mask[n * nbc + c] = 1;
      }
    }
  }
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    for (Int c = 0; c < nbc; ++c) {
      if (dependent[c].count(mesh.getNodeGlobalId(master)) != 0) {
        mask[slave * nbc + c] = 1;
      }
    }
  }
  Array<Real> deterministic = sum;
  I64 nb_masked = 0;
  for (Idx n = 0; n < nb; ++n) {
    for (Int c = 0; c < nbc; ++c) {
      if (mask[n * nbc + c] != 0) {
        deterministic(n, c) = 0.;
        ++nb_masked;
      }
    }
  }
  add_i(w, prefix + "nondyadic_sum_run_dependent", mask, nbc,
        "1: the entry's _sum comes from a master whose remote contributions give >= 2 distinct "
        "results over their arrival orders (run-dependent upstream, D-039 amendment; recorded "
        "as 0 in nondyadic_after_sum_deterministic)");
  add_r(w, prefix + "nondyadic_after_sum_deterministic", reals(deterministic), nbc,
        "makeConsistentForPeriodicity _sum (entries of nondyadic_sum_run_dependent recorded "
        "as 0)");
  std::set<Idx> masters;
  for (Int c = 0; c < nbc; ++c) {
    std::vector<long long> gids(dependent[c].begin(), dependent[c].end());
    w.parameters()[prefix + "nondyadic_run_dependent_gids/" + std::to_string(c)] =
        ap::Json::array_of(gids);
    masters.insert(dependent[c].begin(), dependent[c].end());
  }
  w.parameters()[prefix + "nondyadic_run_dependent_masters"] =
      static_cast<long long>(masters.size());
  w.parameters()[prefix + "nondyadic_run_dependent_entries"] = static_cast<long long>(nb_masked);
  const char * unmasked = std::getenv("PARALLEL_PROBE_UNMASKED_SUM");
  if (unmasked != nullptr and std::string(unmasked) == "1") {
    // variation study only (never forwarded by generate.sh / run-in-container.sh)
    add_r(w, prefix + "nondyadic_after_sum_unmasked", reals(sum), nbc,
          "variation study: makeConsistentForPeriodicity _sum, unmasked (run-dependent)");
    w.parameters()[prefix + "variation_study"] = true;
  }
}

/// The unordered_multimap master -> slaves in its iteration order (equal
/// keys are adjacent), and makeConsistentForPeriodicity _id / _sum of
/// non-dyadic data f(gid) = (1 / (3 + gid), 0.1 (gid + 1))
/// (several slaves per master make the sum order-dependent). Version
/// 3: the `_sum` values of nodes that take their value from a master with
/// >= 2 remote contributors (`order_dependent_masters`: the master itself on
/// every rank and its local slaves) are recorded as 0 and flagged in
/// `nondyadic_sum_order_dependent` (a superset). Version 4 adds the per-rank
/// contributions and masks only the run-dependent entries
/// (`record_periodic_sum_run_dependent`).
void record_periodic_nondyadic(ap::FixtureWriter & w, Mesh & mesh, const std::string & prefix) {
  std::vector<I64> raw;
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    raw.push_back(master);
    raw.push_back(slave);
  }
  add_i(w, prefix + "multimap_order", raw, 2,
        "(master, slave) local pairs in the iteration order of the unordered_multimap");
  const auto nb = mesh.getNbNodes();
  Array<Real> a(nb, 2);
  for (Idx i = 0; i < nb; ++i) {
    auto g = mesh.getNodeGlobalId(i);
    a(i, 0) = 1. / (3. + g);
    a(i, 1) = 0.1 * (g + 1);
  }
  add_r(w, prefix + "nondyadic_input", reals(a), 2, "1 / (3 + gid), 0.1 (gid + 1)");
  Array<Real> id = a;
  mesh.makeConsistentForPeriodicity(id, ReduceOperation::_id);
  add_r(w, prefix + "nondyadic_after_id", reals(id), 2, "makeConsistentForPeriodicity _id");
  Array<Real> sum = a;
  mesh.makeConsistentForPeriodicity(sum, ReduceOperation::_sum);
  const Array<Real> sum_unmasked = sum;
  const auto dependent = order_dependent_masters(mesh);
  std::vector<I64> mask(nb, 0);
  for (Idx n = 0; n < nb; ++n) {
    if (dependent.count(mesh.getNodeGlobalId(n)) != 0) {
      mask[n] = 1;
    }
  }
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    if (dependent.count(mesh.getNodeGlobalId(master)) != 0) {
      mask[slave] = 1;
    }
  }
  I64 nb_masked = 0;
  for (Idx n = 0; n < nb; ++n) {
    if (mask[n] != 0) {
      sum(n, 0) = 0.;
      sum(n, 1) = 0.;
      ++nb_masked;
    }
  }
  add_i(w, prefix + "nondyadic_sum_order_dependent", mask, 1,
        "1: the node's _sum comes from a master accumulated from >= 2 remote ranks in arrival "
        "order (structural rule of probe version 3, a superset of the run-dependent entries "
        "nondyadic_sum_run_dependent; recorded as 0 in nondyadic_after_sum)");
  add_r(w, prefix + "nondyadic_after_sum", reals(sum), 2,
        "makeConsistentForPeriodicity _sum (entries of nondyadic_sum_order_dependent recorded "
        "as 0)");
  w.parameters()[prefix + "nondyadic_order_dependent_masters"] =
      static_cast<long long>(dependent.size());
  w.parameters()[prefix + "nondyadic_order_dependent_nodes"] = static_cast<long long>(nb_masked);
  record_periodic_sum_run_dependent(w, mesh, prefix, a, sum_unmasked);
}

/// Everything the rank holds after distribution.
void record_distribution(ap::FixtureWriter & w, Mesh & mesh, TypeTable & tt) {
  auto & p = w.parameters();
  p["rank"] = static_cast<long long>(g_rank);
  p["nb_ranks"] = static_cast<long long>(g_size);
  p["nb_global_nodes"] = static_cast<long long>(mesh.getNbGlobalNodes());
  p["is_distributed"] = mesh.isDistributed();
  const auto nb_nodes = mesh.getNbNodes();
  std::vector<I64> gids(nb_nodes), flags(nb_nodes), pranks(nb_nodes);
  for (Idx n = 0; n < nb_nodes; ++n) {
    gids[n] = mesh.getNodeGlobalId(n);
    flags[n] = static_cast<I64>(static_cast<std::uint8_t>(mesh.getNodeFlag(n)));
    pranks[n] = mesh.getNodePrank(n);
  }
  add_i(w, "nodes/global_ids", gids, 1, "getNodeGlobalId per local node");
  add_i(w, "nodes/flags", flags, 1, "getNodeFlag per local node (NodeFlag bits)");
  add_i(w, "nodes/pranks", pranks, 1, "getNodePrank per local node (-1: none)");
  add_r(w, "nodes/coordinates", reals(mesh.getNodes()), mesh.getSpatialDimension(),
        "node coordinates");
  std::vector<std::string> blocks;
  for (auto gt : ghost_types) {
    for (auto type : mesh.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      auto key = type_name(type) + "/" + ghost_name(gt);
      blocks.push_back(key);
      const auto & conn = mesh.getConnectivity(type, gt);
      add_i(w, "conn/" + key, ints(conn), conn.getNbComponent(),
            "connectivity (local node ids)");
      if (gt == _ghost) {
        MeshAccessor acc(mesh);
        const auto & gc = acc.getGhostsCounters(type, _ghost);
        add_i(w, "ghosts_counters/" + key, ints(gc), 1, "ghost counters");
      }
      for (auto tag : {std::string("tag_0"), std::string("tag_1")}) {
        if (mesh.template hasData<Int>(tag, type, GhostType(gt))) {
          add_i(w, "mesh_data/" + tag + "/" + key, ints(mesh.getData<Int>(tag)(type, gt)), 1,
                "elemental Int mesh data");
        }
      }
    }
  }
  p["blocks"] = ap::Json::array_of(blocks);
  // element synchronizer
  std::vector<std::string> esched;
  const auto & es = std::as_const(mesh).getElementSynchronizer();
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : es.getCommunications().iterateSchemes(sr)) {
      std::vector<I64> v;
      for (auto && el : scheme) {
        v.push_back(tt.code(el.type));
        v.push_back(el.element);
        v.push_back(el.ghost_type == _not_ghost ? 0 : 1);
      }
      auto key = std::string(sr == _send ? "send" : "recv") + "/" + std::to_string(proc);
      esched.push_back(key);
      add_i(w, "element_scheme/" + key, v, 3, "(type code, element, ghost) in scheme order");
    }
  }
  p["element_schemes"] = ap::Json::array_of(esched);
  std::vector<std::string> nsched;
  const auto & ns = std::as_const(mesh).getNodeSynchronizer();
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : ns.getCommunications().iterateSchemes(sr)) {
      auto key = std::string(sr == _send ? "send" : "recv") + "/" + std::to_string(proc);
      nsched.push_back(key);
      add_i(w, "node_scheme/" + key, ints(scheme), 1, "local nodes in scheme order");
    }
  }
  p["node_schemes"] = ap::Json::array_of(nsched);
  // groups
  std::vector<std::string> egroups, ngroups;
  for (auto & eg : mesh.iterateElementGroups()) {
    egroups.push_back(eg.getName());
    p["element_group_dimension/" + eg.getName()] = static_cast<long long>(eg.getDimension());
    for (auto gt : ghost_types) {
      for (auto type : eg.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        add_i(w, "element_group/" + eg.getName() + "/" + type_name(type) + "/" + ghost_name(gt),
              ints(eg.getElements(type, gt)), 1, "group elements");
      }
    }
  }
  for (auto & ng : mesh.iterateNodeGroups()) {
    ngroups.push_back(ng.getName());
    add_i(w, "node_group/" + ng.getName(), ints(ng.getNodes()), 1, "group nodes");
  }
  p["element_groups"] = ap::Json::array_of(egroups);
  p["node_groups"] = ap::Json::array_of(ngroups);
  p["type_table"] = ap::Json::array_of(tt.names);
}

/// Record a partition computed on rank 0 (linearised parts of the mesh
/// dimension, parts of every partitioned type).
void record_partition(ap::FixtureWriter & w, Mesh & mesh, MeshPartition & partition,
                      const std::string & what) {
  std::vector<I64> lin;
  for (auto type : mesh.elementTypes(mesh.getSpatialDimension(), _not_ghost, _ek_not_defined)) {
    for (auto p : partition.getPartition(type, _not_ghost)) {
      lin.push_back(p);
    }
  }
  add_i(w, "partition/linearized", lin, 1,
        what + " part of every element of the mesh dimension (types in enum order)");
  std::vector<std::string> ptypes;
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    if (partition.hasPartitions(type, _not_ghost)) {
      ptypes.push_back(type_name(type));
      add_i(w, "partition/parts/" + type_name(type),
            ints(partition.getPartition(type, _not_ghost)), 1, "parts of every element");
    }
  }
  w.parameters()["partition/types"] = ap::Json::array_of(ptypes);
}

/// Seeded Scotch partition on rank 0 (P > 1), recorded, then distribute.
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
    std::vector<std::string> ptypes;
    for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      if (partition->hasPartitions(type, _not_ghost)) {
        ptypes.push_back(type_name(type));
        add_i(w, "partition/parts/" + type_name(type),
              ints(partition->getPartition(type, _not_ghost)), 1, "parts of every element");
      }
    }
    w.parameters()["partition/types"] = ap::Json::array_of(ptypes);
    w.parameters()["scotch/seed"] = static_cast<long long>(kSeed);
    w.parameters()["scotch/version"] = std::to_string(SCOTCH_VERSION) + "." +
                                       std::to_string(SCOTCH_RELEASE) + "." +
                                       std::to_string(SCOTCH_PATCHLEVEL);
    w.parameters()["scotch/strategy"] = std::string("");
    const char * threads = std::getenv("SCOTCH_PTHREAD_NUMBER");
    w.parameters()["scotch/pthread_number"] = std::string(threads ? threads : "unset");
  }
  mesh.distribute(partition);
}

/// Structured Tet4 cube (Kuhn split), as crates/akantu-solid/tests/par_smm.
void cube_tet4(Mesh & mesh, Int n) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  const Real h = 1.0 / n;
  auto id = [&](Int i, Int j, Int k) { return (k * (n + 1) + j) * (n + 1) + i; };
  for (Int k = 0; k <= n; ++k) {
    for (Int j = 0; j <= n; ++j) {
      for (Int i = 0; i <= n; ++i) {
        Vector<Real> x(3);
        x(0) = i * h;
        x(1) = j * h;
        x(2) = k * h;
        nodes.push_back(x);
      }
    }
  }
  auto & conn = acc.getConnectivity(_tetrahedron_4);
  const int kuhn[6][4] = {{0, 1, 2, 6}, {0, 2, 3, 6}, {0, 3, 7, 6},
                          {0, 7, 4, 6}, {0, 4, 5, 6}, {0, 5, 1, 6}};
  for (Int k = 0; k < n; ++k) {
    for (Int j = 0; j < n; ++j) {
      for (Int i = 0; i < n; ++i) {
        Idx v[8] = {id(i, j, k),         id(i + 1, j, k),     id(i + 1, j + 1, k),
                    id(i, j + 1, k),     id(i, j, k + 1),     id(i + 1, j, k + 1),
                    id(i + 1, j + 1, k + 1), id(i, j + 1, k + 1)};
        for (auto & t : kuhn) {
          Vector<Idx> c(4);
          for (int q = 0; q < 4; ++q) {
            c(q) = v[t[q]];
          }
          conn.push_back(c);
        }
      }
    }
  }
  acc.makeReady();
}

void write_material() {
  if (g_rank == 0) {
    std::ofstream f(g_scratch / "material.dat");
    f << "material elastic [\n  name = steel\n  rho = 1\n  E = 1\n  nu = 0.3\n]\n";
  }
  Communicator::getStaticCommunicator().barrier();
  getStaticParser().parse((g_scratch / "material.dat").string());
}

void scenario_distribution() {
  const auto scenario = std::string("distribution_cube_msh");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::distribute of cube.msh with a seeded Scotch partition, rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  Mesh mesh(3);
  const auto file = g_frozen / "test/test_synchronizer/cube.msh";
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file, "frozen:fixtures/meshes/gmsh-4.13.1/test/test_synchronizer/cube.msh");
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  w.finish();
}

void setup_cube_bc(SolidMechanicsModel & model, Mesh & mesh, bool initial_u, bool top_load) {
  auto & blocked = model.getBlockedDOFs();
  auto & u = model.getDisplacement();
  auto & f = model.getExternalForce();
  const auto & x = mesh.getNodes();
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    const Real X = x(n, 0), Y = x(n, 1), Z = x(n, 2);
    if (Z == 0.0) {
      blocked(n, 0) = blocked(n, 1) = blocked(n, 2) = true;
    }
    if (initial_u) {
      u(n, 0) = 1e-3 * Z * Z * (1.0 + 0.3 * Y);
      u(n, 1) = 5e-4 * Z * (0.5 - X);
    }
    if (top_load and Z == 1.0) {
      f(n, 0) = 1e-3 * (1.0 + Y);
      f(n, 2) = -2e-3;
    }
  }
}

/// Probe-side patch (documented, NUMERICAL_VALIDATION "Mesh distribution
/// and parallel SMM"): upstream appends ghost elements to their law's
/// filter while unpacking `_constitutive_law_id` in MPI_Waitany completion
/// order (`constitutive_laws_handler_tmpl.hh:493-505`,
/// `synchronizer_impl_tmpl.hh:235`), so with two or more neighbours the
/// ghost filter order — hence the order in which ghost elements are
/// assembled into nodal forces — varies from run to run (two oracle runs at
/// 4 ranks differ in the last bits). The probe rewrites every law's ghost
/// filter (and the law-local numbering) into the order a **rank-order**
/// unpack gives: receive schemes in rank order, elements in scheme order.
/// Validity condition (D-035, UD-232): the rewrite moves
/// no state only if every ghost internal still holds its default; checked
/// below for the Real internals of the recorded laws (all defaults are 0
/// for the elastic law with the default eigen_grad_u), otherwise the probe
/// fails. Returns the checked internal names.
std::vector<std::string> check_ghost_internals_at_defaults(SolidMechanicsModel & model) {
  std::set<std::string> checked;
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & mat = model.getMaterial(m);
    for (const auto * id : {"stress", "eigen_grad_u", "grad_u", "potential_energy", "delta_T",
                            "sigma_th", "epsilon_th", "piola_kirchhoff_2", "green_strain"}) {
      if (not mat.isInternal<Real>(id, _ek_regular)) {
        continue;
      }
      const auto & internal = std::as_const(mat).getInternal<Real>(id);
      for (auto type : internal.elementTypes(_ghost_type = _ghost)) {
        const auto & a = internal(type, _ghost);
        for (Idx i = 0; i < Idx(a.size() * a.getNbComponent()); ++i) {
          if (a.data()[i] != 0.) {
            throw std::runtime_error(std::string("ghost internal ") + id +
                                     " is not at its default: the canonical ghost order "
                                     "rewrite would move state");
          }
        }
      }
      checked.insert(id);
    }
  }
  return {checked.begin(), checked.end()};
}

void canonicalize_ghost_filters(SolidMechanicsModel & model, Mesh & mesh) {
  if (not mesh.isDistributed()) {
    return;
  }
  const auto & es = std::as_const(mesh).getElementSynchronizer();
  const auto & law_index = std::as_const(model).getConstitutiveLawByElement();
  auto & local_num = const_cast<ElementTypeMapArray<Idx> &>(
      std::as_const(model).getConstitutiveLawLocalNumbering());
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & mat = model.getMaterial(m);
    auto & filter = const_cast<ElementTypeMapArray<Idx> &>(std::as_const(mat).getElementFilter());
    std::map<ElementType, std::vector<Idx>> canon;
    for (auto && [proc, scheme] : es.getCommunications().iterateSchemes(_recv)) {
      for (auto && el : scheme) {
        if (law_index(el) == m) {
          canon[el.type].push_back(el.element);
        }
      }
    }
    for (auto && [type, list] : canon) {
      auto & f = filter(type, _ghost);
      if (Idx(f.size()) != Idx(list.size())) {
        throw std::runtime_error("ghost filter and receive schemes disagree");
      }
      for (Idx i = 0; i < Idx(list.size()); ++i) {
        f(i) = list[i];
        local_num(Element{type, list[i], _ghost}) = i;
      }
    }
  }
}

void record_ghost_filters(ap::FixtureWriter & w, SolidMechanicsModel & model) {
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    const auto & filter = std::as_const(model.getMaterial(m)).getElementFilter();
    for (auto gt : ghost_types) {
      for (auto type : filter.elementTypes(_ghost_type = gt, _element_kind = _ek_not_defined)) {
        add_i(w, "law" + std::to_string(m) + "/filter/" + type_name(type) + "/" + ghost_name(gt),
              ints(filter(type, gt)), 1, "law element filter (ghost: canonical rank order)");
      }
    }
  }
}

void record_model(ap::FixtureWriter & w, SolidMechanicsModel & model, Mesh & mesh, bool dynamic) {
  add_r(w, "model/displacement", reals(model.getDisplacement()), 3, "displacement");
  if (dynamic) {
    add_r(w, "model/velocity", reals(model.getVelocity()), 3, "velocity");
    add_r(w, "model/acceleration", reals(model.getAcceleration()), 3, "acceleration");
    add_r(w, "model/mass", reals(model.getMass()), 3, "lumped mass");
  }
  add_r(w, "model/internal_force", reals(model.getInternalForce()), 3, "internal force");
  (void)mesh;
}

void scenario_explicit() {
  const auto scenario = std::string("smm_explicit_cube_n4");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Explicit SMM on a distributed Tet4 cube (seeded Scotch), rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 4);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  const auto checked = check_ghost_internals_at_defaults(model);
  canonicalize_ghost_filters(model, mesh);
  record_ghost_filters(w, model);
  w.parameters()["ghost_filter_order"] = std::string("canonical rank order (probe patch)");
  w.parameters()["ghost_filter_reorder_validity"] =
      std::string("valid: every ghost value of the Real internals listed in "
                  "ghost_internals_checked is at its default (0) before the rewrite, so the "
                  "reorder moves no state (D-035, UD-232)");
  w.parameters()["ghost_internals_checked"] = ap::Json::array_of(checked);
  setup_cube_bc(model, mesh, true, false);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.8 * stable;
  model.setTimeStep(dt);
  std::vector<double> ek, ep;
  for (int s = 0; s < 40; ++s) {
    model.solveStep();
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
  }
  w.add("model/time_step", {2}, {"entry"}, "table", std::vector<double>{stable, dt})
      .description("getStableTimeStep, 0.8 * it")
      .tolerance_class("exact");
  w.add("model/kinetic_energy", {ek.size()}, {"step"}, "table", ek)
      .description("getEnergy(kinetic) after each step (MPI_Allreduce)")
      .tolerance_class("exact");
  w.add("model/potential_energy", {ep.size()}, {"step"}, "table", ep)
      .description("getEnergy(potential) after each step (MPI_Allreduce)")
      .tolerance_class("exact");
  record_model(w, model, mesh, true);
  w.finish();
}

/// Two-law selection of smm_explicit_two_laws_cube_n4 (local elements only:
/// ghosts receive their owner's law through _constitutive_law_id).
class CheckerboardSelector : public ConstitutiveLawSelector {
public:
  explicit CheckerboardSelector(const Mesh & mesh) : mesh(mesh) {}
  Idx operator()(const Element & element) override {
    Vector<Real> b(3);
    mesh.getBarycenter(element, b);
    Int k = 0;
    for (Int d = 0; d < 3; ++d) {
      k += static_cast<Int>(std::floor(2. * b(d)));
    }
    return ((k % 2) + 2) % 2;
  }

private:
  const Mesh & mesh;
};

void record_law_internals(ap::FixtureWriter & w, SolidMechanicsModel & model) {
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & mat = model.getMaterial(m);
    for (const auto * id : {"stress", "grad_u", "piola_kirchhoff_2"}) {
      if (not mat.isInternal<Real>(id, _ek_regular)) {
        continue;
      }
      const auto & internal = std::as_const(mat).getInternal<Real>(id);
      for (auto gt : ghost_types) {
        for (auto type : internal.elementTypes(_ghost_type = gt)) {
          add_r(w, "law" + std::to_string(m) + "/" + id + "/" + type_name(type) + "/" + ghost_name(gt),
                reals(internal(type, gt)), internal(type, gt).getNbComponent(),
                std::string("law ") + std::to_string(m) + " " + id +
                    " per quadrature point after the last step (ghost: canonical filter order)");
        }
      }
    }
  }
}

void scenario_explicit_two_laws() {
  const auto scenario = std::string("smm_explicit_two_laws_cube_n4");
  if (g_rank == 0) {
    std::ofstream f(g_scratch / "material_two_laws.dat");
    f << "material elastic [\n  name = small\n  rho = 1.5\n  E = 2\n  nu = 0.25\n]\n\n"
         "material elastic [\n  name = fd\n  rho = 1\n  E = 1\n  nu = 0.3\n  "
         "finite_deformation = true\n]\n";
  }
  Communicator::getStaticCommunicator().barrier();
  getStaticParser().parse((g_scratch / "material_two_laws.dat").string());
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Explicit SMM with two laws (one finite-deformation) on a distributed Tet4 "
                      "cube (seeded Scotch), rank view",
                      kProbe, kProbeVersion5);
  w.add_tag("parallel");
  record_pin(w);
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 4);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  SolidMechanicsModel model(mesh);
  model.setMaterialSelector(std::make_shared<CheckerboardSelector>(mesh));
  model.initFull(_analysis_method = _explicit_lumped_mass);
  if (model.getNbConstitutiveLaws() != 2 or model.getMaterial(0).isFiniteDeformation() or
      not model.getMaterial(1).isFiniteDeformation()) {
    throw std::runtime_error("two laws, law 1 in finite deformation, expected");
  }
  const auto checked = check_ghost_internals_at_defaults(model);
  canonicalize_ghost_filters(model, mesh);
  record_ghost_filters(w, model);
  w.parameters()["material_selector"] =
      std::string("law (floor(2 x) + floor(2 y) + floor(2 z)) mod 2 of the barycentre");
  w.parameters()["ghost_filter_order"] = std::string("canonical rank order (probe patch)");
  w.parameters()["ghost_filter_reorder_validity"] =
      std::string("valid: every ghost value of the Real internals listed in "
                  "ghost_internals_checked is at its default (0) before the rewrite, so the "
                  "reorder moves no state (D-035, UD-232)");
  w.parameters()["ghost_internals_checked"] = ap::Json::array_of(checked);
  setup_cube_bc(model, mesh, true, false);
  const Real stable = model.getStableTimeStep();
  const Real dt = 0.8 * stable;
  model.setTimeStep(dt);
  std::vector<double> ek, ep;
  for (int s = 0; s < 40; ++s) {
    model.solveStep();
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
  }
  w.add("model/time_step", {2}, {"entry"}, "table", std::vector<double>{stable, dt})
      .description("getStableTimeStep, 0.8 * it")
      .tolerance_class("exact");
  w.add("model/kinetic_energy", {ek.size()}, {"step"}, "table", ek)
      .description("getEnergy(kinetic) after each step (MPI_Allreduce)")
      .tolerance_class("exact");
  w.add("model/potential_energy", {ep.size()}, {"step"}, "table", ep)
      .description("getEnergy(potential) after each step (MPI_Allreduce)")
      .tolerance_class("exact");
  record_model(w, model, mesh, true);
  record_law_internals(w, model);
  w.finish();
  // restore the static parser of the other scenarios
  write_material();
}

void scenario_static() {
  const auto scenario = std::string("smm_static_cube_n3");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Static SMM on a distributed Tet4 cube (seeded Scotch), rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 3);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _static);
  const auto checked = check_ghost_internals_at_defaults(model);
  canonicalize_ghost_filters(model, mesh);
  record_ghost_filters(w, model);
  w.parameters()["ghost_filter_order"] = std::string("canonical rank order (probe patch)");
  w.parameters()["ghost_filter_reorder_validity"] =
      std::string("valid: every ghost value of the Real internals listed in "
                  "ghost_internals_checked is at its default (0) before the rewrite, so the "
                  "reorder moves no state (D-035, UD-232)");
  w.parameters()["ghost_internals_checked"] = ap::Json::array_of(checked);
  setup_cube_bc(model, mesh, false, true);
  model.solveStep();
  w.add("model/potential_energy", {1}, {"entry"}, "table",
        std::vector<double>{model.getEnergy("potential")})
      .description("getEnergy(potential)")
      .tolerance_class("solver");
  record_model(w, model, mesh, false);
  w.finish();
}

/// Review cell rule of the adversarial mesh: part of the cell (i, j)
/// holding an element of type `type`, modulo the number of parts.
Idx adv2d_part(Int i, Int j, ElementType type) {
  Idx p = 0;
  if (i <= 1) {
    p = 0;
  } else if (i == 2) {
    p = 1;
  } else if (i == 3) {
    p = j == 0 ? 1 : 2;
  } else if (i == 4) {
    p = 2;
  } else {
    p = type == _quadrangle_4 ? 2 : 3;
  }
  return p % g_size;
}

void scenario_adv2d() {
  const auto scenario = std::string("distribution_adv2d");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::distribute of review V50's adversarial 2-D mesh (orphan point, "
                      "interleaved Quad4/Tri3) with MeshPartitionMeshData, rank view",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  Mesh mesh(2);
  std::shared_ptr<MeshPartition> partition;
  if (g_rank == 0) {
    mesh.read(g_adv2d.string());
    w.add_input_file("mesh", g_adv2d, "probe:oracle/probes/parallel/adv2d.msh");
    if (g_size > 1) {
      auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("adv2d_partition");
      for (auto type : mesh.elementTypes(2, _not_ghost, _ek_not_defined)) {
        auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
        for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
          Vector<Real> b(2);
          mesh.getBarycenter(Element{type, e, _not_ghost}, b);
          a(e) = adv2d_part(Int(std::floor(b(0))), Int(std::floor(b(1))), type);
        }
      }
      partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, 2);
      partition->partitionate(g_size);
      record_partition(w, mesh, *partition, "MeshPartitionMeshData (cell rule)");
      w.parameters()["partition/rule"] =
          std::string("cell (i, j): i <= 1 -> 0, i = 2 -> 1, i = 3 -> (j = 0 ? 1 : 2), "
                      "i = 4 -> 2, i = 5 -> 2 (Quad4) / 3 (Tri3); modulo nb_ranks");
    }
  }
  mesh.distribute(partition);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  w.finish();
}

/* ---------------------------------------------------------------------- */
/* Distributed facets, grid and periodic synchronizers */
/* ---------------------------------------------------------------------- */

/// (type code, element, ghost) of an element; ElementNull = (-1, -1, -1),
/// a default Element() (type _not_defined, UD-128) = (-2, element, ghost).
void push_element(std::vector<I64> & v, const Element & el, TypeTable & tt) {
  if (el == ElementNull) {
    v.insert(v.end(), {-1, -1, -1});
    return;
  }
  v.push_back(el.type == _not_defined ? -2 : tt.code(el.type));
  v.push_back(el.element);
  v.push_back(el.ghost_type == _not_ghost ? 0 : 1);
}

/// Send/receive schemes of an element synchronizer under `prefix`.
std::vector<std::string> record_element_schemes(ap::FixtureWriter & w,
                                                const Communications<Element> & comms,
                                                const std::string & prefix, TypeTable & tt) {
  std::vector<std::string> keys;
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : comms.iterateSchemes(sr)) {
      std::vector<I64> v;
      for (auto && el : scheme) {
        push_element(v, el, tt);
      }
      auto key = std::string(sr == _send ? "send" : "recv") + "/" + std::to_string(proc);
      keys.push_back(key);
      add_i(w, prefix + "/" + key, v, 3, "(type code, element, ghost) in scheme order");
    }
  }
  return keys;
}

std::vector<std::string> record_node_schemes(ap::FixtureWriter & w,
                                             const Communications<Idx> & comms,
                                             const std::string & prefix) {
  std::vector<std::string> keys;
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : comms.iterateSchemes(sr)) {
      auto key = std::string(sr == _send ? "send" : "recv") + "/" + std::to_string(proc);
      keys.push_back(key);
      add_i(w, prefix + "/" + key, ints(scheme), 1, "local nodes in scheme order");
    }
  }
  return keys;
}

/// The facet mesh of a distributed mesh after Mesh::initMeshFacets.
void record_facets(ap::FixtureWriter & w, Mesh & mesh, TypeTable & tt) {
  auto & p = w.parameters();
  auto & facets = mesh.getMeshFacets();
  p["facets/is_distributed"] = facets.isDistributed();
  std::vector<std::string> blocks;
  for (auto gt : ghost_types) {
    for (auto type : facets.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      auto key = type_name(type) + "/" + ghost_name(gt);
      blocks.push_back(key);
      const auto & conn = facets.getConnectivity(type, gt);
      add_i(w, "facets/conn/" + key, ints(conn), conn.getNbComponent(),
            "facet connectivity (local node ids)");
    }
  }
  p["facets/blocks"] = ap::Json::array_of(blocks);
  std::vector<std::string> e2s_blocks, s2e_blocks;
  const auto & e2s = facets.getElementToSubelement();
  for (auto gt : ghost_types) {
    for (auto type : e2s.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      auto key = type_name(type) + "/" + ghost_name(gt);
      e2s_blocks.push_back(key);
      std::vector<I64> len, v;
      const auto & rows = e2s(type, gt);
      for (Idx i = 0; i < Idx(rows.size()); ++i) {
        const auto & row = rows(i);
        len.push_back(static_cast<I64>(row.size()));
        for (auto && el : row) {
          push_element(v, el, tt);
        }
      }
      add_i(w, "facets/e2s_len/" + key, len, 1, "element_to_subelement row lengths");
      add_i(w, "facets/e2s/" + key, v, 3, "element_to_subelement rows, concatenated");
    }
  }
  const auto & s2e = facets.getSubelementToElement();
  for (auto gt : ghost_types) {
    for (auto type : s2e.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      auto key = type_name(type) + "/" + ghost_name(gt);
      s2e_blocks.push_back(key);
      const auto & a = s2e(type, gt);
      std::vector<I64> v;
      for (Idx i = 0; i < Idx(a.size() * a.getNbComponent()); ++i) {
        push_element(v, a.data()[i], tt);
      }
      add_i(w, "facets/s2e/" + key, v, 3, "subelement_to_element (row-major, every slot)");
      p["facets/s2e_nb_component/" + key] = static_cast<long long>(a.getNbComponent());
    }
  }
  p["facets/e2s_blocks"] = ap::Json::array_of(e2s_blocks);
  p["facets/s2e_blocks"] = ap::Json::array_of(s2e_blocks);
  if (facets.isDistributed()) {
    auto keys = record_element_schemes(
        w, std::as_const(facets).getElementSynchronizer().getCommunications(),
        "facets/scheme", tt);
    p["facets/schemes"] = ap::Json::array_of(keys);
  }
  if (mesh.hasData("mesh_to_mesh_facet")) {
    const auto & m2f = mesh.getData<Element>("mesh_to_mesh_facet");
    std::vector<std::string> mblocks;
    for (auto gt : ghost_types) {
      for (auto type : m2f.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        auto key = type_name(type) + "/" + ghost_name(gt);
        mblocks.push_back(key);
        std::vector<I64> v;
        const auto & a = m2f(type, gt);
        for (Idx i = 0; i < Idx(a.size()); ++i) {
          push_element(v, a(i), tt);
        }
        add_i(w, "m2f/" + key, v, 3, "parent mesh_to_mesh_facet");
      }
    }
    p["m2f_blocks"] = ap::Json::array_of(mblocks);
  }
  p["type_table"] = ap::Json::array_of(tt.names);
}

void scenario_facets_cube_msh() {
  const auto scenario = std::string("facets_cube_msh");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::initMeshFacets on cube.msh distributed with a seeded Scotch "
                      "partition (FacetSynchronizer), rank view",
                      kProbe, kProbeVersion2);
  w.add_tag("parallel");
  Mesh mesh(3);
  const auto file = g_frozen / "test/test_synchronizer/cube.msh";
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file,
                     "frozen:fixtures/meshes/gmsh-4.13.1/test/test_synchronizer/cube.msh");
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  mesh.initMeshFacets();
  record_facets(w, mesh, tt);
  w.finish();
}

void scenario_facets_cube_n4() {
  const auto scenario = std::string("facets_cube_n4");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::initMeshFacets on the distributed Tet4 cube n = 4 (seeded "
                      "Scotch), rank view",
                      kProbe, kProbeVersion2);
  w.add_tag("parallel");
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 4);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  mesh.initMeshFacets();
  record_facets(w, mesh, tt);
  w.finish();
}

void scenario_facets_adv2d() {
  const auto scenario = std::string("facets_adv2d");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::initMeshFacets on review V50's adversarial 2-D mesh distributed "
                      "with MeshPartitionMeshData (cell rule), rank view",
                      kProbe, kProbeVersion2);
  w.add_tag("parallel");
  Mesh mesh(2);
  std::shared_ptr<MeshPartition> partition;
  if (g_rank == 0) {
    mesh.read(g_adv2d.string());
    w.add_input_file("mesh", g_adv2d, "probe:oracle/probes/parallel/adv2d.msh");
    if (g_size > 1) {
      auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("adv2d_partition");
      for (auto type : mesh.elementTypes(2, _not_ghost, _ek_not_defined)) {
        auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
        for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
          Vector<Real> b(2);
          mesh.getBarycenter(Element{type, e, _not_ghost}, b);
          a(e) = adv2d_part(Int(std::floor(b(0))), Int(std::floor(b(1))), type);
        }
      }
      partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, 2);
      partition->partitionate(g_size);
      record_partition(w, mesh, *partition, "MeshPartitionMeshData (cell rule)");
    }
  }
  mesh.distribute(partition);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  mesh.initMeshFacets();
  record_facets(w, mesh, tt);
  w.finish();
}

/// Structured Tri3 square [0,1]^2 (n x n cells, each split along the
/// diagonal (i,j)-(i+1,j+1)), as crates/akantu-mesh/tests/par_common.
void square_tri3(Mesh & mesh, Int n) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  const Real h = 1.0 / n;
  auto id = [&](Int i, Int j) { return j * (n + 1) + i; };
  for (Int j = 0; j <= n; ++j) {
    for (Int i = 0; i <= n; ++i) {
      Vector<Real> x(2);
      x(0) = i * h;
      x(1) = j * h;
      nodes.push_back(x);
    }
  }
  auto & conn = acc.getConnectivity(_triangle_3);
  for (Int j = 0; j < n; ++j) {
    for (Int i = 0; i < n; ++i) {
      Vector<Idx> a(3), b(3);
      a(0) = id(i, j);
      a(1) = id(i + 1, j);
      a(2) = id(i + 1, j + 1);
      b(0) = id(i, j);
      b(1) = id(i + 1, j + 1);
      b(2) = id(i, j + 1);
      conn.push_back(a);
      conn.push_back(b);
    }
  }
  acc.makeReady();
}

/// GridSynchronizer on a distributed mesh (modern-API port of the stale
/// test_grid_synchronizer.cc: grid of the local barycenters, spacing 1.2 R,
/// centre of the mesh box).
void run_grid(ap::FixtureWriter & w, Mesh & mesh, Real radius) {
  const Int dim = mesh.getSpatialDimension();
  const Vector<Real> lower = mesh.getLowerBounds();
  const Vector<Real> upper = mesh.getUpperBounds();
  Vector<Real> center = 0.5 * (upper + lower);
  Vector<Real> spacing(dim);
  for (Int i = 0; i < dim; ++i) {
    spacing(i) = radius * 1.2;
  }
  SpatialGrid<Element> grid(dim, spacing, center);
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
    for (Idx e = 0; e < mesh.getNbElement(type, _not_ghost); ++e) {
      Element el{type, e, _not_ghost};
      Vector<Real> b(dim);
      mesh.getBarycenter(el, b);
      grid.insert(el, b);
    }
  }
  std::vector<double> bounds;
  for (Int i = 0; i < dim; ++i) {
    bounds.push_back(grid.getLowerBounds()(i));
  }
  for (Int i = 0; i < dim; ++i) {
    bounds.push_back(grid.getUpperBounds()(i));
  }
  add_r(w, "grid/bounds", bounds, dim, "SpatialGrid lower bounds, upper bounds");
  std::vector<double> c(center.data(), center.data() + dim);
  add_r(w, "grid/center", c, dim, "grid centre (0.5 (upper + lower) of the mesh box)");
  w.parameters()["grid/radius"] = radius;
  record_bbox(w, mesh, "mesh/bbox_before");
  GridSynchronizer gs(mesh, grid, "grid_synchronizer");
  record_bbox(w, mesh, "mesh/bbox_after");
  record_pin(w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  auto keys = record_element_schemes(w, std::as_const(gs).getCommunications(), "grid_scheme", tt);
  w.parameters()["grid_schemes"] = ap::Json::array_of(keys);
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
}

void scenario_grid_tri_n8() {
  const auto scenario = std::string("grid_tri_n8");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "GridSynchronizer (R = 0.2) on a distributed Tri3 square n = 8 (seeded "
                      "Scotch), rank view after the new ghost layer",
                      kProbe, kProbeVersion3);
  w.add_tag("parallel");
  Mesh mesh(2);
  if (g_rank == 0) {
    square_tri3(mesh, 8);
  }
  distribute_scotch(mesh, w);
  run_grid(w, mesh, 0.2);
  w.finish();
}

void scenario_grid_cube_n4() {
  const auto scenario = std::string("grid_cube_n4");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "GridSynchronizer (R = 0.3) on the distributed Tet4 cube n = 4 (seeded "
                      "Scotch), rank view after the new ghost layer",
                      kProbe, kProbeVersion3);
  w.add_tag("parallel");
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 4);
  }
  distribute_scotch(mesh, w);
  run_grid(w, mesh, 0.3);
  w.finish();
}

/// TestMeshPeriodic (test_mesh_periodic_fixture.hh + test_mesh_periodic.cc)
/// on `dim` with the first `dim_periodic` directions periodic.
void scenario_periodic(Int dim, Int dim_periodic) {
  const std::string dirs = std::string("xyz").substr(0, dim_periodic);
  const auto scenario = "periodic_" + std::to_string(dim) + "d_" + dirs;
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "TestMeshPeriodic: makePeriodic on a distributed mesh (seeded Scotch), "
                      "periodic synchronizer and makeConsistentForPeriodicity, rank view",
                      kProbe, kProbeVersion4);
  w.add_tag("parallel");
  Mesh mesh(dim);
  const std::string name = dim == 1 ? "line.msh" : dim == 2 ? "square.msh" : "cube.msh";
  const auto file = g_frozen / "test/test_mesh_utils/test_mesh_periodic" / name;
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file,
                     "frozen:fixtures/meshes/gmsh-4.13.1/test/test_mesh_utils/"
                     "test_mesh_periodic/" + name);
    std::vector<ID> groups;
    const char * all[] = {"x-minus", "x-plus", "y-minus", "y-plus", "z-minus", "z-plus"};
    for (Int i = 0; i < 2 * dim_periodic; ++i) {
      groups.push_back(all[i]);
    }
    auto && shared = mesh.getNodalData<Real>("shared");
    shared.resize(mesh.getNbNodes());
    shared.set(0.);
    for (auto group : groups) {
      for (auto node : mesh.getElementGroup(group).getNodeGroup().getNodes()) {
        shared(node) += 1.;
      }
    }
  }
  distribute_scotch(mesh, w);
  record_bbox(w, mesh, "mesh/bbox_before");
  for (Int s = 0; s < dim_periodic; ++s) {
    mesh.makePeriodic(SpatialDirection(s));
  }
  record_bbox(w, mesh, "mesh/bbox_after");
  record_pin(w);
  w.parameters()["is_periodic"] = mesh.isPeriodic();
  TypeTable tt;
  record_distribution(w, mesh, tt);
  std::vector<I64> pairs;
  std::vector<std::pair<Idx, Idx>> sorted;
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    sorted.emplace_back(master, slave);
  }
  std::sort(sorted.begin(), sorted.end());
  for (auto && [m, s] : sorted) {
    pairs.push_back(m);
    pairs.push_back(s);
  }
  add_i(w, "periodic/pairs", pairs, 2, "(master, slave) local node pairs, sorted");
  auto keys = record_node_schemes(
      w, std::as_const(mesh).getPeriodicNodeSynchronizer().getCommunications(),
      "periodic_scheme");
  w.parameters()["periodic_schemes"] = ap::Json::array_of(keys);
  // MakeConsistentForPeriodicityIdentity
  Array<Real> id_array(mesh.getNbNodes(), 1);
  id_array.set(10.);
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    if (mesh.isPeriodicSlave(n)) {
      id_array(n) = -1.;
    }
  }
  add_r(w, "periodic/identity_input", reals(id_array), 1, "10, -1 on periodic slaves");
  mesh.makeConsistentForPeriodicity(id_array, ReduceOperation::_id);
  add_r(w, "periodic/identity_output", reals(id_array), 1, "after makeConsistent _id");
  // MakeConsistentForPeriodicitySum
  Array<Real> sum_array(mesh.getNbNodes(), 1);
  sum_array.set(100.);
  std::set<Idx> masters;
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    masters.insert(master);
  }
  const auto & shared = mesh.getNodalData<Real>("shared");
  for (auto master : masters) {
    if (mesh.isLocalOrMasterNode(master)) {
      sum_array(master) = 100. / std::pow(2, shared(master));
    }
  }
  add_r(w, "periodic/sum_input", reals(sum_array), 1,
        "100, 100 / 2^shared on local-or-master periodic masters");
  mesh.makeConsistentForPeriodicity(sum_array, ReduceOperation::_id);
  add_r(w, "periodic/sum_after_id", reals(sum_array), 1, "after makeConsistent _id");
  mesh.makeConsistentForPeriodicity(sum_array, ReduceOperation::_sum);
  add_r(w, "periodic/sum_output", reals(sum_array), 1, "after makeConsistent _id then _sum");
  record_periodic_nondyadic(w, mesh, "periodic/");
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

/// UD-251 probe (only when selected explicitly): a hexahedron next to two
/// prisms (prisms on rank 0, the hexahedron on rank 1): the ranks' local
/// facet types differ ({triangle, quadrangle} vs {quadrangle}), so upstream's
/// per-type synchronous announcements of the FacetSynchronizer are never all
/// received (expected: no completion; run with a short timeout).
void scenario_facets_hex_prism() {
  const auto scenario = std::string("facets_hex_prism");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "UD-251: FacetSynchronizer with different local facet types per rank",
                      kProbe, kProbeVersion2);
  w.add_tag("parallel");
  Mesh mesh(3);
  std::shared_ptr<MeshPartition> partition;
  if (g_rank == 0) {
    MeshAccessor acc(mesh);
    auto & nodes = acc.getNodes();
    for (Int k = 0; k <= 1; ++k) {
      for (Int j = 0; j <= 1; ++j) {
        for (Int i = 0; i <= 2; ++i) {
          Vector<Real> x(3);
          x(0) = i;
          x(1) = j;
          x(2) = k;
          nodes.push_back(x);
        }
      }
    }
    auto id = [](Int i, Int j, Int k) { return (k * 2 + j) * 3 + i; };
    auto & prisms = acc.getConnectivity(_pentahedron_6);
    Vector<Idx> a(6), b(6);
    a << id(0, 0, 0), id(1, 0, 0), id(1, 1, 0), id(0, 0, 1), id(1, 0, 1), id(1, 1, 1);
    b << id(0, 0, 0), id(1, 1, 0), id(0, 1, 0), id(0, 0, 1), id(1, 1, 1), id(0, 1, 1);
    prisms.push_back(a);
    prisms.push_back(b);
    auto & hexes = acc.getConnectivity(_hexahedron_8);
    Vector<Idx> h(8);
    h << id(1, 0, 0), id(2, 0, 0), id(2, 1, 0), id(1, 1, 0), id(1, 0, 1), id(2, 0, 1),
        id(2, 1, 1), id(1, 1, 1);
    hexes.push_back(h);
    acc.makeReady();
    if (g_size > 1) {
      auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("hex_prism_partition");
      mapping->alloc(2, 1, _pentahedron_6, _not_ghost).set(0);
      mapping->alloc(1, 1, _hexahedron_8, _not_ghost).set(1 % g_size);
      partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, 3);
      partition->partitionate(g_size);
    }
  }
  mesh.distribute(partition);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  std::cerr << "parallel_probe (rank " << g_rank << "): facets_hex_prism initMeshFacets...\n";
  mesh.initMeshFacets();
  std::cerr << "parallel_probe (rank " << g_rank << "): facets_hex_prism done\n";
  record_facets(w, mesh, tt);
  w.finish();
}

/* ---------------------------------------------------------------------- */
/* Adversarial scenarios (probe version 3) */
/* adversarial partitions, committed as fixtures (the empty-rank ones are  */
/* selected explicitly only: upstream refuses the partition).              */
/* ---------------------------------------------------------------------- */

/// Partition every element of the mesh dimension by a rule on its
/// barycentre; recorded like the other mesh-data partitions.
template <class Rule>
std::shared_ptr<MeshPartition> v101_partition(ap::FixtureWriter & w, Mesh & mesh, Rule && rule,
                                              const std::string & what) {
  std::shared_ptr<MeshPartition> partition;
  if (g_rank != 0 or g_size == 1) {
    return partition;
  }
  const Int dim = mesh.getSpatialDimension();
  auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("v101_partition");
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
    auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
    for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
      Vector<Real> b(dim);
      mesh.getBarycenter(Element{type, e, _not_ghost}, b);
      a(e) = rule(b, type);
    }
  }
  partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, dim);
  partition->partitionate(g_size);
  record_partition(w, mesh, *partition, "MeshPartitionMeshData (V101 rule)");
  w.parameters()["partition/rule"] = what;
  return partition;
}

Idx v101_quadrant(const Vector<Real> & b) {
  Idx a = b(0) > 0.5 ? 1 : 0;
  Idx c = b(1) > 0.5 ? 1 : 0;
  return (a + 2 * c) % g_size;
}

/// Parity colouring of the octants (2-D: quadrants): neighbours across every
/// periodic direction are on different ranks at 2 and 4 ranks.
Idx v101_parity(const Vector<Real> & b, Int dim) {
  Idx s = 0;
  Idx a = b(0) > 0.5 ? 1 : 0;
  for (Int i = 0; i < dim; ++i) s += b(i) > 0.5 ? 1 : 0;
  return ((s % 2) + 2 * a) % g_size;
}

/// Ranks 0..min(3, P) - 1 get the elements; at P = 4 rank 3 is empty.
Idx v101_three_stripes(const Vector<Real> & b) {
  Idx p = Idx(std::floor(b(0) * 3.));
  p = std::min<Idx>(p, 2);
  return p % std::min<Idx>(g_size, 3);
}

Idx v101_four_stripes(const Vector<Real> & b) {
  Idx p = Idx(std::floor(b(0) * 4.));
  p = std::min<Idx>(p, 3);
  return p % g_size;
}

#define V101_WRITER_V(w, scenario, what, version)                                      \
  ap::FixtureWriter w(g_root, case_id(scenario), std::string("Review V101: ") + (what),  \
                      kProbe, version);                                                  \
  w.add_tag("parallel");                                                                 \
  w.add_tag("review-v101")
#define V101_WRITER(w, scenario, what) V101_WRITER_V(w, scenario, what, kProbeVersion3)

/// Facets on a partition with facets shared by three ranks' ghosts.
void v101_scenario_facets(const std::string & scenario, Int dim, Int n) {
  V101_WRITER(w, scenario, "initMeshFacets, quadrant partition (facets seen as ghosts of "
                           "two other ranks)");
  Mesh mesh(dim);
  if (g_rank == 0) {
    if (dim == 2) square_tri3(mesh, n); else cube_tet4(mesh, n);
  }
  auto partition = v101_partition(
      w, mesh, [&](auto && b, auto) { return v101_quadrant(b); }, "quadrant (x > .5) + 2 (y > .5)");
  mesh.distribute(partition);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  record_bbox(w, mesh, "mesh/bbox_before");
  mesh.initMeshFacets();
  record_facets(w, mesh, tt);
  record_bbox(w, mesh, "mesh/bbox_after");
  record_pin(w);
  w.finish();
}

/// Rank 3 empty at 4 ranks: what = facets | grid | periodic (upstream's
/// MeshPartition refuses the partition: no fixture is promoted).
void v101_scenario_empty_rank(const std::string & what) {
  const auto scenario = "v101_empty_rank_" + what;
  V101_WRITER(w, scenario, "partition with an empty rank (4 ranks), " + what);
  Mesh mesh(2);
  if (g_rank == 0) square_tri3(mesh, 6);
  auto partition = v101_partition(
      w, mesh, [&](auto && b, auto) { return v101_three_stripes(b); },
      "three stripes floor(3 x) mod min(P, 3)");
  mesh.distribute(partition);
  TypeTable tt;
  record_bbox(w, mesh, "mesh/bbox_before");
  if (what == "facets") {
    record_distribution(w, mesh, tt);
    mesh.initMeshFacets();
    record_facets(w, mesh, tt);
  } else if (what == "grid") {
    run_grid(w, mesh, 0.2);
  } else {
    mesh.makePeriodic(_x);
    mesh.makePeriodic(_y);
    record_distribution(w, mesh, tt);
  }
  record_bbox(w, mesh, "mesh/bbox_after");
  w.finish();
}

/// Periodic pairs across rank boundaries in 1 / 2 / 3 directions (parity
/// partition), with non-dyadic data in makeConsistentForPeriodicity.
void v101_scenario_periodic(Int dim, Int ndir, Int n) {
  const std::string dirs = std::string("xyz").substr(0, ndir);
  const auto scenario = "v101_periodic_" + std::to_string(dim) + "d_" + dirs;
  V101_WRITER_V(w, scenario, "makePeriodic across rank boundaries (parity partition)",
                kProbeVersion4);
  Mesh mesh(dim);
  if (g_rank == 0) {
    if (dim == 2) square_tri3(mesh, n); else cube_tet4(mesh, n);
  }
  auto partition = v101_partition(
      w, mesh, [&](auto && b, auto) { return v101_parity(b, dim); },
      "parity of the octant ((sum (x_i > .5)) % 2 + 2 (x > .5)) mod P");
  mesh.distribute(partition);
  record_bbox(w, mesh, "mesh/bbox_before");
  for (Int s = 0; s < ndir; ++s) {
    mesh.makePeriodic(SpatialDirection(s));
  }
  record_bbox(w, mesh, "mesh/bbox_after");
  record_pin(w);
  w.parameters()["is_periodic"] = mesh.isPeriodic();
  TypeTable tt;
  record_distribution(w, mesh, tt);
  std::vector<I64> pairs;
  std::vector<std::pair<Idx, Idx>> sorted;
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) sorted.emplace_back(master, slave);
  std::sort(sorted.begin(), sorted.end());
  for (auto && [m, s] : sorted) { pairs.push_back(m); pairs.push_back(s); }
  add_i(w, "periodic/pairs", pairs, 2, "(master, slave) local node pairs, sorted");
  auto keys = record_node_schemes(
      w, std::as_const(mesh).getPeriodicNodeSynchronizer().getCommunications(), "periodic_scheme");
  w.parameters()["periodic_schemes"] = ap::Json::array_of(keys);
  record_periodic_nondyadic(w, mesh, "periodic/");
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

/// Grid neighbourhood wider than a rank's halo (stripes of width 1/4,
/// R = 0.45), and the grid synchronizer on the mixed Quad4/Tri3 adv2d mesh.
void v101_scenario_grid(const std::string & which) {
  const auto scenario = "v101_grid_" + which;
  V101_WRITER(w, scenario, "GridSynchronizer: " + which);
  Mesh mesh(2);
  std::shared_ptr<MeshPartition> partition;
  Real radius = 0.45;
  if (which == "wide") {
    if (g_rank == 0) square_tri3(mesh, 8);
    partition = v101_partition(
        w, mesh, [&](auto && b, auto) { return v101_four_stripes(b); },
        "four stripes floor(4 x) mod P");
  } else {
    radius = 0.6;
    if (g_rank == 0) {
      mesh.read(g_adv2d.string());
      w.add_input_file("mesh", g_adv2d, "probe:oracle/probes/parallel/adv2d.msh");
      if (g_size > 1) {
        auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("adv2d_partition");
        for (auto type : mesh.elementTypes(2, _not_ghost, _ek_not_defined)) {
          auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
          for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
            Vector<Real> b(2);
            mesh.getBarycenter(Element{type, e, _not_ghost}, b);
            a(e) = adv2d_part(Int(std::floor(b(0))), Int(std::floor(b(1))), type);
          }
        }
        partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, 2);
        partition->partitionate(g_size);
        record_partition(w, mesh, *partition, "MeshPartitionMeshData (cell rule)");
      }
    }
  }
  mesh.distribute(partition);
  run_grid(w, mesh, radius);
  w.finish();
}

/// `want(name)`: default selection (every scenario when none is selected);
/// `explicit_only(name)`: selected by name.
void v101_dispatch(const std::function<bool(const std::string &)> & want,
                   const std::function<bool(const std::string &)> & explicit_only) {
  if (want("v101_facets_tri_quadrants")) v101_scenario_facets("v101_facets_tri_quadrants", 2, 4);
  if (want("v101_facets_tet_quadrants")) v101_scenario_facets("v101_facets_tet_quadrants", 3, 4);
  for (auto what : {std::string("facets"), std::string("grid"), std::string("periodic")}) {
    if (explicit_only("v101_empty_rank_" + what)) v101_scenario_empty_rank(what);
  }
  if (want("v101_periodic_2d_x")) v101_scenario_periodic(2, 1, 4);
  if (want("v101_periodic_2d_xy")) v101_scenario_periodic(2, 2, 4);
  if (want("v101_periodic_3d_xyz")) v101_scenario_periodic(3, 3, 2);
  if (want("v101_grid_wide")) v101_scenario_grid("wide");
  if (not g_adv2d.empty() and want("v101_grid_adv2d")) v101_scenario_grid("adv2d");
}

/* ---------------------------------------------------------------------- */
/* probe version 4 */
/* ---------------------------------------------------------------------- */

/// Pin provenance of the version-4 cases (an `oracle_pins` object; the same
/// pins as record_pin, recorded under the keys of the first generation of
/// these cases).
ap::Json oracle_pins() {
  ap::Json j = ap::Json::object();
  j["eigen_cpu_cache_sizes"] = "setCpuCacheSizes(" + std::to_string(kEigenL1) + ", " +
                               std::to_string(kEigenL2) + ", " + std::to_string(kEigenL3) +
                               ") at the start of main";
  const char * ct = std::getenv("OPENBLAS_CORETYPE");
  const char * nt = std::getenv("OPENBLAS_NUM_THREADS");
  j["OPENBLAS_CORETYPE"] = ct != nullptr ? ct : "";
  j["OPENBLAS_NUM_THREADS"] = nt != nullptr ? nt : "";
  return j;
}

/// physical_names, element_to_subelement / subelement_to_element (recomputed
/// by fillElementToSubElementsData on every rank) per block.
void record_mesh_data_g20(ap::FixtureWriter & w, Mesh & mesh, TypeTable & tt) {
  auto & p = w.parameters();
  std::vector<std::string> names_blocks, e2s_blocks, s2e_blocks;
  for (auto gt : ghost_types) {
    for (auto type : mesh.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      auto key = type_name(type) + "/" + ghost_name(gt);
      if (mesh.template hasData<std::string>("physical_names", type, GhostType(gt))) {
        const auto & a = mesh.getData<std::string>("physical_names")(type, gt);
        std::vector<std::string> v(a.begin(), a.end());
        p["physical_names/" + key] = ap::Json::array_of(v);
        names_blocks.push_back(key);
      }
      if (mesh.template hasData<std::vector<Element>>("element_to_subelement", type,
                                                       GhostType(gt))) {
        const auto & a = mesh.getElementToSubelement(type, gt);
        std::vector<I64> counts, flat;
        for (Idx i = 0; i < a.size(); ++i) {
          counts.push_back(static_cast<I64>(a(i).size()));
          for (const auto & el : a(i)) {
            push_element(flat, el, tt);
          }
        }
        add_i(w, "element_to_subelement/" + key + "/counts", counts, 1,
              "number of elements per entry");
        add_i(w, "element_to_subelement/" + key + "/elements", flat, 3,
              "(type code, element, ghost) per listed element, entry by entry; null "
              "elements (-1, -1, -1)");
        e2s_blocks.push_back(key);
      }
      if (mesh.template hasData<Element>("subelement_to_element", type, GhostType(gt))) {
        const auto & a = mesh.getSubelementToElement(type, gt);
        std::vector<I64> flat;
        // raw storage: the scalar iterator of a multi-component Array throws
        for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
          push_element(flat, a.data()[i], tt);
        }
        p["subelement_to_element/" + key + "/nb_component"] =
            static_cast<long long>(a.getNbComponent());
        add_i(w, "subelement_to_element/" + key + "/elements", flat, 3,
              "(type code, element, ghost) per component, entry by entry; null elements "
              "(-1, -1, -1)");
        s2e_blocks.push_back(key);
      }
    }
  }
  p["physical_names_blocks"] = ap::Json::array_of(names_blocks);
  p["element_to_subelement_blocks"] = ap::Json::array_of(e2s_blocks);
  p["subelement_to_element_blocks"] = ap::Json::array_of(s2e_blocks);
}

/// square.msh on rank 0 (frozen test_mesh_periodic mesh).
std::filesystem::path read_square_on_root(Mesh & mesh, ap::FixtureWriter & w) {
  const auto file = g_frozen / "test/test_mesh_utils/test_mesh_periodic/square.msh";
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file,
                     "frozen:fixtures/meshes/gmsh-4.13.1/test/test_mesh_utils/"
                     "test_mesh_periodic/square.msh");
  }
  return file;
}

void scenario_square_data() {
  const auto scenario = std::string("distribution_square_data");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::distribute of square.msh with nodal data, seeded Scotch: "
                      "physical_names, recomputed element/facet adjacency, nodal data",
                      kProbe, kProbeVersion4);
  w.add_tag("parallel");
  w.parameters()["oracle_pins"] = oracle_pins();
  Mesh mesh(2);
  read_square_on_root(mesh, w);
  if (g_rank == 0) {
    // TestMeshPeriodic::SetUp's "shared" counter and an Int tag
    auto & shared = mesh.getNodalData<Real>("shared");
    shared.resize(mesh.getNbNodes());
    shared.set(0.);
    for (const auto * g : {"x-minus", "x-plus", "y-minus", "y-plus"}) {
      for (auto node : mesh.getElementGroup(g).getNodeGroup().getNodes()) {
        shared(node) += 1.;
      }
    }
    auto & tag = mesh.getNodalData<Int>("node_tag", 2);
    tag.resize(mesh.getNbNodes());
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      tag(n, 0) = (7 * n) % 11;
      tag(n, 1) = -n;
    }
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  record_mesh_data_g20(w, mesh, tt);
  add_r(w, "nodal_data/shared", reals(mesh.getNodalData<Real>("shared")), 1,
        "nodal data \"shared\" (Real) in local node order");
  add_i(w, "nodal_data/node_tag", ints(mesh.getNodalData<Int>("node_tag")), 2,
        "nodal data \"node_tag\" (Int, 2 components) in local node order");
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

void scenario_square_periodic() {
  // reproduction of UD-655 only (never generates a golden): upstream aborts
  // in MasterNodeInfoPerProc::synchronizePeriodicity ("double free or
  // corruption") at 2 ranks, periodic in x as well as in x and y
  const auto scenario = std::string("distribution_square_periodic");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::distribute of square.msh made periodic in x on rank 0, "
                      "seeded Scotch: periodic pairs of every rank",
                      kProbe, kProbeVersion4);
  w.add_tag("parallel");
  w.parameters()["oracle_pins"] = oracle_pins();
  Mesh mesh(2);
  read_square_on_root(mesh, w);
  // periodic in x only: with x and y, upstream's distribution corrupts the
  // heap (MasterNodeInfoPerProc::synchronizePeriodicity, "double free or
  // corruption", UD-655)
  if (g_rank == 0) {
    mesh.makePeriodic(_x);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  auto & p = w.parameters();
  p["is_periodic"] = mesh.isPeriodic();
  std::vector<I64> pairs, gpairs, slaves_of;
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    if (mesh.isPeriodicSlave(n)) {
      auto m = mesh.getPeriodicMaster(n);
      pairs.insert(pairs.end(), {n, m});
      gpairs.insert(gpairs.end(), {mesh.getNodeGlobalId(n), mesh.getNodeGlobalId(m)});
    }
    if (mesh.isPeriodicMaster(n)) {
      slaves_of.push_back(n);
      std::vector<I64> s;
      for (auto sl : mesh.getPeriodicSlaves(n)) {
        s.push_back(sl);
      }
      slaves_of.push_back(static_cast<I64>(s.size()));
      slaves_of.insert(slaves_of.end(), s.begin(), s.end());
    }
  }
  add_i(w, "periodic/pairs", pairs, 2, "(local slave, local master), slaves increasing");
  add_i(w, "periodic/global_pairs", gpairs, 2, "the same pairs in global node ids");
  add_i(w, "periodic/slaves_of_masters", slaves_of, 1,
        "per local master (increasing): master, number of slaves, the slaves in "
        "getPeriodicSlaves order (unordered_multimap iteration)");
  w.finish();
}

/* ---------------------------------------------------------------------- */
/* probe version 6 */
/* ---------------------------------------------------------------------- */

/// Ghost partitions (getGhostPartitionCSR) of every partitioned type, rank 0.
void record_ghost_partitions(ap::FixtureWriter & w, Mesh & mesh, MeshPartition & partition) {
  const auto & csr_map = partition.getGhostPartitionCSR();
  std::vector<std::string> types;
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    if (not partition.hasPartitions(type, _not_ghost)) {
      continue;
    }
    types.push_back(type_name(type));
    const auto & csr = csr_map(type, _not_ghost);
    std::vector<I64> counts, values;
    for (Idx r = 0; r < csr.getNbRows(); ++r) {
      counts.push_back(csr.getNbCols(r));
      // const CSR: begin/end (upstream's const getRow does not compile)
      for (auto q = csr.begin(r); q != csr.end(r); ++q) {
        values.push_back(*q);
      }
    }
    add_i(w, "partition/ghost_counts/" + type_name(type), counts, 1,
          "number of ghost parts per element (getGhostPartitionCSR row sizes)");
    add_i(w, "partition/ghost_parts/" + type_name(type), values, 1,
          "ghost parts of every element, rows concatenated");
  }
  w.parameters()["partition/ghost_types"] = ap::Json::array_of(types);
}

/// UD-1060: ranks whose entry of MasterNodeInfoPerProc::synchronizePeriodicity's
/// std::vector<Array<Idx>> is relocated by a later emplace_back: entries
/// 0 .. 2^k - 1 for the largest power of two 2^k < nb_ranks, the pattern
/// observed black-box with the oracle image's g++ 14.2.0 (capacities 1, 2,
/// 4, 4, 8,...; review sandbox/vecgrowth.txt, D-030: the standard does
/// not fix the growth policy; re-observe it if the oracle's toolchain
/// changes). Akantu's Array move leaves the source owning its buffer,
/// so a relocated entry that holds periodic pairs is freed while still in
/// use and freed again by the vector's destructor (heap corruption, UD-655).
std::vector<Int> ud1060_relocated_ranks(Int nb_ranks) {
  std::vector<Int> out;
  Int top = 1;
  while (2 * top < nb_ranks) top *= 2;
  for (Int p = 0; nb_ranks > 1 and p < top; ++p) out.push_back(p);
  return out;
}

/// The relocations of a
/// std::vector filled by `nb_ranks` successive emplace_back calls without
/// reserve, observed on this toolchain (D-030: black-box observation, not
/// the library's source) with an element type whose move constructor is
/// noexcept (Akantu's Array, the element type of
/// MasterNodeInfoPerProc::synchronizePeriodicity's vector, is relocated by
/// moves): per entry, the number of times it was moved by a later
/// emplace_back.
std::vector<I64> ud1060_observed_relocations(Int nb_ranks) {
  static std::vector<I64> * counts = nullptr;
  struct Entry {
    Int id;
    explicit Entry(Int i) : id(i) {}
    Entry(Entry && o) noexcept : id(o.id) { ++(*counts)[o.id]; }
    Entry(const Entry &) = delete;
    Entry & operator=(const Entry &) = delete;
    Entry & operator=(Entry &&) = delete;
    ~Entry() = default;
  };
  std::vector<I64> local(nb_ranks, 0);
  counts = &local;
  {
    std::vector<Entry> v;
    for (Int p = 0; p < nb_ranks; ++p) {
      v.emplace_back(p);
    }
  }
  counts = nullptr;
  return local;
}

/// Scenario `ud1060_vector_growth` (one process): for 1 to 16 ranks, the
/// observed relocation count of every entry (ud1060_observed_relocations)
/// next to the formula's relocated entries (ud1060_relocated_ranks); the
/// probe fails if they disagree on which entries move.
void scenario_ud1060_vector_growth() {
  ap::FixtureWriter w(g_root, case_id("ud1060_vector_growth"),
                      "UD-1060 / PP-01 (b): relocations of a std::vector filled by n "
                      "emplace_back calls (noexcept moves), observed on the oracle's toolchain "
                      "for n = 1 .. 16, and the formula of ud1060_relocated_ranks",
                      kProbe, kProbeVersion7);
  w.add_tag("parallel");
  w.parameters()["oracle_pins"] = oracle_pins();
  w.parameters()["compiler"] = std::string(__VERSION__);
#if defined(__GLIBCXX__)
  w.parameters()["libstdcxx_date"] = static_cast<long long>(__GLIBCXX__);
#endif
  for (Int n = 1; n <= 16; ++n) {
    auto observed = ud1060_observed_relocations(n);
    std::vector<I64> formula(n, 0);
    for (auto r : ud1060_relocated_ranks(n)) {
      formula[r] = 1;
    }
    for (Int p = 0; p < n; ++p) {
      if ((observed[p] > 0) != (formula[p] > 0)) {
        throw std::runtime_error("ud1060_vector_growth: the formula disagrees with the observed "
                                 "relocations at " + std::to_string(n) + " ranks, entry " +
                                 std::to_string(p));
      }
    }
    add_i(w, "observed/relocations_n" + std::to_string(n), observed, 1,
          "times each entry was moved by a later emplace_back (n entries)");
    add_i(w, "formula/relocated_n" + std::to_string(n), formula, 1,
          "1 for the entries ud1060_relocated_ranks(n) names, else 0");
  }
  w.finish();
  std::cout << "  " << case_id("ud1060_vector_growth") << std::endl;
}

/// Rank 0, before the distribution: the ranks that will hold each periodic
/// slave node (local and ghost elements of every type, from the partition
/// and its ghost partitions) and whether upstream's distribution is
/// memory-safe for this partition (no relocated rank holds a slave). The
/// result is broadcast; every rank records it.
/// Number of periodic slave nodes every rank will hold under `partition`
/// (rank 0, before the distribution).
std::vector<I64> ud1060_slave_holders(Mesh & mesh, MeshPartition & partition, Int nb_parts) {
  std::vector<std::set<Idx>> held(nb_parts);
  const auto & csr_map = partition.getGhostPartitionCSR();
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    if (not partition.hasPartitions(type, _not_ghost)) continue;
    const auto & parts = partition.getPartition(type, _not_ghost);
    const auto & csr = csr_map(type, _not_ghost);
    const auto & conn = mesh.getConnectivity(type, _not_ghost);
    const auto nnpe = conn.getNbComponent();
    for (Idx e = 0; e < conn.size(); ++e) {
      std::vector<Int> ranks{Int(parts(e))};
      for (auto q = csr.begin(e); q != csr.end(e); ++q) ranks.push_back(Int(*q));
      for (Idx k = 0; k < nnpe; ++k) {
        auto n = conn(e, k);
        if (mesh.isPeriodicSlave(n)) {
          for (auto r : ranks) held[r].insert(n);
        }
      }
    }
  }
  std::vector<I64> out;
  for (auto & h : held) out.push_back(static_cast<I64>(h.size()));
  return out;
}

bool ud1060_safe(const std::vector<I64> & held, Int nb_parts) {
  for (auto r : ud1060_relocated_ranks(nb_parts)) {
    if (held[r] != 0) return false;
  }
  return true;
}

bool record_ud1060_precondition(ap::FixtureWriter & w, Mesh & mesh, MeshPartition * partition) {
  Int safe = 1;
  std::vector<I64> holders;  // (rank, number of slave nodes held)
  if (g_rank == 0 and partition != nullptr) {
    auto held = ud1060_slave_holders(mesh, *partition, g_size);
    safe = ud1060_safe(held, g_size) ? 1 : 0;
    for (Int r = 0; r < g_size; ++r) {
      holders.push_back(r);
      holders.push_back(held[r]);
    }
  }
  Communicator::getStaticCommunicator().broadcast(safe, 0);
  std::vector<I64> relocated;
  for (auto r : ud1060_relocated_ranks(g_size)) relocated.push_back(r);
  w.parameters()["ud1060/relocated_ranks"] = ap::Json::array_of(relocated);
  w.parameters()["ud1060/upstream_memory_safe"] = safe == 1;
  if (g_rank == 0 and partition != nullptr) {
    add_i(w, "ud1060/slave_holders", holders, 2,
          "(rank, number of periodic slave nodes the rank will hold) before the distribution");
  }
  return safe == 1;
}

/// The periodic state of a distributed mesh (rank view).
void record_periodic_state(ap::FixtureWriter & w, Mesh & mesh) {
  auto & p = w.parameters();
  p["is_periodic"] = mesh.isPeriodic();
  std::vector<I64> pairs, gpairs, raw;
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    if (mesh.isPeriodicSlave(n)) {
      auto m = mesh.getPeriodicMaster(n);
      pairs.insert(pairs.end(), {n, m});
      gpairs.insert(gpairs.end(), {mesh.getNodeGlobalId(n), mesh.getNodeGlobalId(m)});
    }
  }
  add_i(w, "periodic/slave_master", pairs, 2, "(local slave, local master), slaves increasing");
  add_i(w, "periodic/global_slave_master", gpairs, 2, "the same pairs in global node ids");
  std::vector<std::pair<Idx, Idx>> sorted;
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    sorted.emplace_back(master, slave);
  }
  std::sort(sorted.begin(), sorted.end());
  std::vector<I64> ms;
  for (auto && [m, s] : sorted) {
    ms.push_back(m);
    ms.push_back(s);
  }
  add_i(w, "periodic/pairs", ms, 2, "(master, slave) local node pairs, sorted");
  for (auto && [master, slave] : mesh.getPeriodicMasterSlaves()) {
    raw.push_back(master);
    raw.push_back(slave);
  }
  add_i(w, "periodic/multimap_order", raw, 2,
        "(master, slave) local pairs in the iteration order of the unordered_multimap");
}

/// pstrip.msh (next to adv2d.msh) on rank 0, made periodic in x there.
void read_pstrip_periodic_on_root(Mesh & mesh, ap::FixtureWriter & w) {
  const auto file = g_adv2d.parent_path() / "pstrip.msh";
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file, "probe:oracle/probes/parallel/pstrip.msh");
    mesh.makePeriodic(_x);
    w.parameters()["root_periodic_pairs_before"] =
        static_cast<long long>(mesh.getPeriodicMasterSlaves().size());
  }
}

/// Record everything after the (upstream-safe) distribution of a periodic
/// mesh.
void record_pbd_after(ap::FixtureWriter & w, Mesh & mesh) {
  TypeTable tt;
  record_distribution(w, mesh, tt);
  record_mesh_data_g20(w, mesh, tt);
  record_periodic_state(w, mesh);
  record_bbox(w, mesh, "mesh/bbox");
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
}

/// Scotch seed of pbd_strip_scotch: the default seed. Its partitions of the
/// 24 x 4 strip at 2 and 4 parts are memory-safe upstream (UD-1060: no
/// relocated rank holds a periodic slave; PARALLEL_PROBE_PBD_SEED_SCAN
/// prints the precondition for other seeds and meshes).
constexpr long kPbdSeed = kSeed;

/// PART-002/003/008: the strip made periodic in x on the root,
/// partitioned by a seeded Scotch (MeshPartitionScotch::partitionate's
/// periodic branch: tweakConnectivity before the dual graph and
/// fillPartitionInformation, restoreConnectivity after), then distributed
/// when upstream is memory-safe for the partition (UD-1060).
void scenario_pbd_strip_scotch() {
  const auto scenario = std::string("pbd_strip_scotch");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "pstrip.msh made periodic in x on rank 0, seeded Scotch partition "
                      "(periodic branch), then Mesh::distribute: rank view",
                      kProbe, kProbeVersion6);
  w.add_tag("parallel");
  w.add_tag("periodic-before-distribution");
  w.parameters()["oracle_pins"] = oracle_pins();
  Mesh mesh(2);
  read_pstrip_periodic_on_root(mesh, w);
  // diagnostic only (never forwarded by generate.sh, writes nothing):
  // PARALLEL_PROBE_PBD_SEED_SCAN=N prints the UD-1060 precondition of the
  // Scotch partitions at 2 and 4 parts for the seeds kSeed .. kSeed + N - 1
  // (PARALLEL_PROBE_PBD_SCAN_MESHES: comma-separated mesh files next to
  // adv2d.msh to scan instead of pstrip.msh)
  if (const char * scan = std::getenv("PARALLEL_PROBE_PBD_SEED_SCAN")) {
    if (g_rank == 0) {
      std::vector<std::string> files{"pstrip.msh"};
      if (const char * list = std::getenv("PARALLEL_PROBE_PBD_SCAN_MESHES")) {
        files.clear();
        std::stringstream ss(list);
        for (std::string f; std::getline(ss, f, ',');) files.push_back(f);
      }
      for (auto & f : files) {
        Mesh m(2);
        m.read((g_adv2d.parent_path() / f).string());
        m.makePeriodic(_x);
        for (long s = kSeed; s < kSeed + std::atol(scan); ++s) {
          std::cout << "pbd " << f << " seed " << s;
          for (Int np : {2, 4}) {
            RandomGenerator<Idx>::seed(s);
            MeshPartitionScotch part(m, m.getSpatialDimension());
            part.partitionate(np);
            auto held = ud1060_slave_holders(m, part, np);
            std::cout << " np" << np << (ud1060_safe(held, np) ? " safe [" : " unsafe [");
            for (auto h : held) std::cout << " " << h;
            std::cout << " ]";
          }
          std::cout << std::endl;
        }
      }
    }
    return;
  }
  std::shared_ptr<MeshPartition> partition;
  if (g_size > 1 and g_rank == 0) {
    RandomGenerator<Idx>::seed(kPbdSeed);
    partition = std::make_shared<MeshPartitionScotch>(mesh, mesh.getSpatialDimension());
    partition->partitionate(g_size);
    record_partition(w, mesh, *partition, "Scotch (periodic branch)");
    record_ghost_partitions(w, mesh, *partition);
    w.parameters()["scotch/seed"] = static_cast<long long>(kPbdSeed);
    w.parameters()["scotch/version"] = std::to_string(SCOTCH_VERSION) + "." +
                                       std::to_string(SCOTCH_RELEASE) + "." +
                                       std::to_string(SCOTCH_PATCHLEVEL);
    const char * threads = std::getenv("SCOTCH_PTHREAD_NUMBER");
    w.parameters()["scotch/pthread_number"] = std::string(threads ? threads : "unset");
    // the connectivity is restored after partitionate (mesh_partition_scotch.cc:370)
    std::vector<I64> conn = ints(mesh.getConnectivity(_triangle_3, _not_ghost));
    add_i(w, "partition/root_connectivity_after", conn, 3,
          "rank 0's Triangle3 connectivity after partitionate (restored)");
  }
  const bool safe = record_ud1060_precondition(w, mesh, partition.get());
  w.parameters()["distributed"] = safe;
  if (safe) {
    mesh.distribute(partition);
    record_pbd_after(w, mesh);
  }
  w.finish();
}

/// The prescribed partition of pbd_strip_mesh_data (elements by barycentre
/// on [0, 24] x [0, 4]): P = 2: rank 0 = 6 < x < 18, rank 1 = the rest
/// (both periodic faces); P = 4: rank 0 = [6, 12), rank 1 = [12, 18),
/// rank 3 = [18, 23) (next to the slave face x = 24 without touching it:
/// its ghosts hold slaves whose masters it lacks) and x < 6 above y = 2,
/// rank 2 = [23, 24] and x < 6 below y = 2 (the master face is split
/// between ranks 2 and 3, so the periodic tweak changes ghost parts). Ranks
/// 0 (and 1 at P = 4) hold no periodic slave (UD-1060).
Idx pbd_strip_part(Real x, Real y) {
  if (g_size == 2) return (x > 6. and x < 18.) ? 0 : 1;
  if (x < 6.) return y > 2. ? 3 : 2;
  if (x < 12.) return 0;
  if (x < 18.) return 1;
  if (x < 23.) return 3;
  return 2;
}

/// PART-002/005/008: the same strip with a MeshPartitionMeshData
/// partition (the periodic tweak of mesh_partition_mesh_data.cc:60, 106).
void scenario_pbd_strip_mesh_data() {
  const auto scenario = std::string("pbd_strip_mesh_data");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "pstrip.msh made periodic in x on rank 0, MeshPartitionMeshData "
                      "partition (periodic tweak), then Mesh::distribute: rank view",
                      kProbe, kProbeVersion6);
  w.add_tag("parallel");
  w.add_tag("periodic-before-distribution");
  w.parameters()["oracle_pins"] = oracle_pins();
  Mesh mesh(2);
  read_pstrip_periodic_on_root(mesh, w);
  std::shared_ptr<MeshPartition> partition;
  if (g_size > 1 and g_rank == 0) {
    auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("pbd_partition");
    for (auto type : mesh.elementTypes(2, _not_ghost, _ek_not_defined)) {
      auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
      for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
        Vector<Real> b(2);
        mesh.getBarycenter(Element{type, e, _not_ghost}, b);
        a(e) = pbd_strip_part(b(0), b(1));
      }
    }
    partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, 2);
    partition->partitionate(g_size);
    record_partition(w, mesh, *partition, "MeshPartitionMeshData (periodic tweak)");
    record_ghost_partitions(w, mesh, *partition);
    w.parameters()["partition/rule"] =
        std::string("barycentre (x, y): P = 2: (6 < x < 18) ? 0 : 1; P = 4: x < 6 -> (y > 2 ? "
                    "3 : 2), < 12 -> 0, < 18 -> 1, < 23 -> 3, else 2");
  }
  const bool safe = record_ud1060_precondition(w, mesh, partition.get());
  w.parameters()["distributed"] = safe;
  if (safe) {
    mesh.distribute(partition);
    record_pbd_after(w, mesh);
  }
  w.finish();
}

/// INT-005: sparse2d.msh (sparse, shuffled Gmsh node and element
/// tags, Quad4 + Tri3 + Seg2 + Point1, ten physical groups) with an Int
/// nodal datum, seeded Scotch: every rank's distribution, physical names,
/// recomputed adjacency and nodal data.
void scenario_sparse_mixed() {
  const auto scenario = std::string("distribution_sparse_mixed");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Mesh::distribute of sparse2d.msh (sparse node/element tags, four "
                      "element types, ten groups) with a nodal datum, seeded Scotch: rank view",
                      kProbe, kProbeVersion6);
  w.add_tag("parallel");
  w.parameters()["oracle_pins"] = oracle_pins();
  Mesh mesh(2);
  const auto file = g_adv2d.parent_path() / "sparse2d.msh";
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file, "probe:oracle/probes/parallel/sparse2d.msh");
    auto & tag = mesh.getNodalData<Int>("node_tag", 2);
    tag.resize(mesh.getNbNodes());
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      tag(n, 0) = (5 * n) % 13;
      tag(n, 1) = 100 - n;
    }
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  record_mesh_data_g20(w, mesh, tt);
  add_i(w, "nodal_data/node_tag", ints(mesh.getNodalData<Int>("node_tag")), 2,
        "nodal data \"node_tag\" (Int, 2 components) in local node order");
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

} // namespace

/* -------------------------------------------------------------------------- */
/* probe version 5                                                          */
/* -------------------------------------------------------------------------- */
void parse_dat(const std::string & dat, const std::string & name) {
  if (g_rank == 0) {
    std::ofstream f(g_scratch / name);
    f << dat;
  }
  Communicator::getStaticCommunicator().barrier();
  getStaticParser().parse((g_scratch / name).string());
}

/// The `Real` internal `id` of `law` per block (law-local order).
template <class Law>
void record_internal(ap::FixtureWriter & w, Law & law, const std::string & id,
                     const std::string & prefix) {
  const auto & internal = std::as_const(law).template getInternal<Real>(id);
  for (auto gt : ghost_types) {
    for (auto type : internal.elementTypes(_ghost_type = gt)) {
      const auto & a = internal(type, gt);
      add_r(w, prefix + "/" + type_name(type) + "/" + ghost_name(gt), reals(a),
            a.getNbComponent(), id + " per law-local element and quadrature point");
    }
  }
}

/// MPI-041: Marigo's random threshold on a distributed mesh.
///
/// The canonical rewrite of the ghost filters (UD-232, see
/// check_ghost_internals_at_defaults) is valid here although the ghost `Yd`
/// are not at a default: upstream draws them in `initMaterial` **after** the
/// `_constitutive_law_id` unpack, position by position of the ghost block
/// (flat storage order), so the value of law-local position l is the l-th
/// ghost draw whatever element arrived there; the rewrite gives position l
/// to the canonical (rank-order) element l, which is the port's rule. No
/// other state depends on the ghost `Yd`: ghost stresses arrive with
/// `_smm_stress` and the laws compute stresses of not-ghost elements only.
/// The other internals of the law are checked at their defaults (0).
void scenario_clh_marigo() {
  const auto scenario = std::string("clh_marigo_cube_n3");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "Marigo (random Yd) on a distributed Tet4 cube: _clh_init_cl and an "
                      "explicit run, rank view",
                      kProbe, kProbeVersion5);
  w.add_tag("parallel");
  record_pin(w);
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 3);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  parse_dat("material marigo [\n  name = m\n  rho = 1\n  E = 1\n  nu = 0.3\n"
            "  Yd = 1e-7 uniform [0, 1e-7]\n  Sd = 1e-5\n]\n",
            "material_marigo.dat");
  const long seed = kSeedLaw * (g_rank + 1);
  RandomGenerator<Idx>::seed(seed);
  w.parameters()["random/seed"] = static_cast<long long>(seed);
  SolidMechanicsModel model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass);
  auto checked = check_ghost_internals_at_defaults(model);
  for (const auto * id : {"damage", "Y"}) {
    const auto & internal = std::as_const(model.getMaterial(0)).getInternal<Real>(id);
    for (auto type : internal.elementTypes(_ghost_type = _ghost)) {
      const auto & a = internal(type, _ghost);
      for (Idx i = 0; i < Idx(a.size() * a.getNbComponent()); ++i) {
        if (a.data()[i] != 0.) {
          throw std::runtime_error(std::string("ghost internal ") + id + " is not 0");
        }
      }
    }
    checked.emplace_back(id);
  }
  canonicalize_ghost_filters(model, mesh);
  record_ghost_filters(w, model);
  w.parameters()["ghost_filter_order"] = std::string("canonical rank order (probe patch)");
  w.parameters()["ghost_filter_reorder_validity"] =
      std::string("valid: the internals listed in ghost_internals_checked are at their default "
                  "(0) and the ghost Yd are position-indexed draws (scenario_clh_marigo)");
  w.parameters()["ghost_internals_checked"] = ap::Json::array_of(checked);
  record_internal(w, model.getMaterial(0), "Yd", "law0/Yd");
  setup_cube_bc(model, mesh, true, false);
  const Real dt = 0.8 * model.getStableTimeStep();
  model.setTimeStep(dt);
  for (int s = 0; s < 20; ++s) {
    model.solveStep();
  }
  w.add("model/time_step", {1}, {"entry"}, "table", std::vector<double>{dt})
      .description("0.8 * getStableTimeStep")
      .tolerance_class("exact");
  add_r(w, "model/displacement", reals(model.getDisplacement()), 3, "displacement after 20 steps");
  add_r(w, "model/velocity", reals(model.getVelocity()), 3, "velocity after 20 steps");
  record_internal(w, model.getMaterial(0), "damage", "law0/damage");
  w.finish();
}

/// MPI-043: the `_diffusion_gradient` exchange of the heat-transfer model.
void scenario_diffusion_gradient() {
  const auto scenario = std::string("diffusion_gradient_cube_n3");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "HeatTransferModel on a distributed Tet4 cube: the law gradient of the "
                      "not-ghost elements and synchronize(_diffusion_gradient), rank view",
                      kProbe, kProbeVersion5);
  w.add_tag("parallel");
  record_pin(w);
  Mesh mesh(3);
  if (g_rank == 0) {
    cube_tet4(mesh, 3);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  parse_dat("model heat_transfer_model [\n  constitutive_law heat_diffusion [\n    name = mat\n"
            "    density = 2\n    capacity = 3\n"
            "    conductivity = [[2.0, 0.3, 0.1], [0.2, 1.5, 0.2], [0.1, 0.4, 1.0]]\n  ]\n]\n",
            "heat.dat");
  HeatTransferModel model(mesh);
  model.initFull(_analysis_method = _static);
  auto & temperature = model.getTemperature();
  const auto & x = mesh.getNodes();
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    const Real X = x(n, 0), Y = x(n, 1), Z = x(n, 2);
    temperature(n) = 1. + 2. * X + 3. * Y * Y + 5. * X * Z;
  }
  add_r(w, "model/temperature", reals(temperature), 1, "temperature (every local node)");
  auto & law = model.getConstitutiveLaw(0);
  law.computeDiffusivityGradU(_not_ghost);
  const auto & grad = std::as_const(law).getInternal<Real>("∇u");
  const auto & local = std::as_const(model).getConstitutiveLawLocalNumbering();
  auto record_ghost = [&](const std::string & name) {
    for (auto type : mesh.elementTypes(3, _ghost)) {
      std::vector<double> v;
      if (grad.exists(type, _ghost)) {
        const auto & a = grad(type, _ghost);
        // getElementFilter(type, ghost) is protected upstream: the rule of the
        // model's FE engine gives the points per element
        const Idx nq = model.getFEEngine().getNbIntegrationPoints(type, _ghost);
        for (Idx e = 0; e < mesh.getNbElement(type, _ghost); ++e) {
          const auto l = local(Element{type, e, _ghost});
          for (Idx q = 0; q < Idx(nq * a.getNbComponent()); ++q) {
            v.push_back(a.data()[l * nq * a.getNbComponent() + q]);
          }
        }
        add_r(w, name + "/" + type_name(type), v, a.getNbComponent(),
              "grad u of every ghost element (mesh order) and quadrature point");
      }
    }
  };
  for (auto type : mesh.elementTypes(3, _not_ghost)) {
    add_r(w, "law0/grad_u/" + type_name(type) + "/not_ghost", reals(grad(type, _not_ghost)), 3,
          "grad u per law-local element and quadrature point (computeDiffusivityGradU)");
  }
  record_ghost("law0/grad_u_ghost_before");
  model.synchronize(SynchronizationTag::_diffusion_gradient);
  record_ghost("law0/grad_u_ghost_after");
  w.finish();
}

/// Canonical rank-order ghost filters of every law of a law handler (the
/// UD-232 probe patch of canonicalize_ghost_filters, any model) after a
/// check that every ghost block of every Real internal of the laws is
/// position-independent (all its values equal), so that the rewrite moves
/// no state. Returns the checked internal names.
template <class Model_>
std::vector<std::string> canonicalize_law_ghosts(Model_ & model, Mesh & mesh,
                                                 const std::vector<std::string> & internals) {
  std::vector<std::string> checked;
  if (not mesh.isDistributed()) {
    return checked;
  }
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & law = model.getConstitutiveLaw(m);
    for (const auto & id : internals) {
      if (not law.template isInternal<Real>(id, _ek_regular)) {
        continue;
      }
      const auto & internal = std::as_const(law).template getInternal<Real>(id);
      for (auto type : internal.elementTypes(_ghost_type = _ghost)) {
        const auto & a = internal(type, _ghost);
        const auto nbc = a.getNbComponent();
        for (Idx i = 0; i < Idx(a.size()); ++i) {
          for (Idx c = 0; c < nbc; ++c) {
            if (a(i, c) != a(0, c)) {
              throw std::runtime_error("ghost internal " + id +
                                       " is position-dependent: the canonical ghost order "
                                       "rewrite would move state");
            }
          }
        }
      }
      checked.push_back(id);
    }
  }
  const auto & es = std::as_const(mesh).getElementSynchronizer();
  const auto & law_index = std::as_const(model).getConstitutiveLawByElement();
  auto & local_num = const_cast<ElementTypeMapArray<Idx> &>(
      std::as_const(model).getConstitutiveLawLocalNumbering());
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & law = model.getConstitutiveLaw(m);
    auto & filter = const_cast<ElementTypeMapArray<Idx> &>(std::as_const(law).getElementFilter());
    std::map<ElementType, std::vector<Idx>> canon;
    for (auto && [proc, scheme] : es.getCommunications().iterateSchemes(_recv)) {
      for (auto && el : scheme) {
        if (law_index(el) == m) {
          canon[el.type].push_back(el.element);
        }
      }
    }
    for (auto && [type, list] : canon) {
      auto & f = filter(type, _ghost);
      if (Idx(f.size()) != Idx(list.size())) {
        throw std::runtime_error("ghost filter and receive schemes disagree");
      }
      for (Idx i = 0; i < Idx(list.size()); ++i) {
        f(i) = list[i];
        local_num(Element{type, list[i], _ghost}) = i;
      }
    }
  }
  std::sort(checked.begin(), checked.end());
  checked.erase(std::unique(checked.begin(), checked.end()), checked.end());
  return checked;
}

template <class Model_>
void record_law_filters(ap::FixtureWriter & w, Model_ & model, const std::string & prefix) {
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    const auto & filter = std::as_const(model.getConstitutiveLaw(m)).getElementFilter();
    for (auto gt : ghost_types) {
      for (auto type : filter.elementTypes(_ghost_type = gt, _element_kind = _ek_not_defined)) {
        add_i(w, prefix + std::to_string(m) + "/filter/" + type_name(type) + "/" + ghost_name(gt),
              ints(filter(type, gt)), 1, "law element filter (ghost: canonical rank order)");
      }
    }
  }
}

/// Structured Tri3 square [0, 1]^2 (two triangles per cell), as
/// crates/akantu-couplers/tests/par_phase_field.
void square_tri3_ac(Mesh & mesh, Int n) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  const Real h = 1.0 / n;
  auto id = [&](Int i, Int j) { return j * (n + 1) + i; };
  for (Int j = 0; j <= n; ++j) {
    for (Int i = 0; i <= n; ++i) {
      Vector<Real> x(2);
      x(0) = i * h;
      x(1) = j * h;
      nodes.push_back(x);
    }
  }
  auto & conn = acc.getConnectivity(_triangle_3);
  for (Int j = 0; j < n; ++j) {
    for (Int i = 0; i < n; ++i) {
      Vector<Idx> t1(3), t2(3);
      t1(0) = id(i, j);
      t1(1) = id(i + 1, j);
      t1(2) = id(i + 1, j + 1);
      t2(0) = id(i, j);
      t2(1) = id(i + 1, j + 1);
      t2(2) = id(i, j + 1);
      conn.push_back(t1);
      conn.push_back(t2);
    }
  }
  acc.makeReady();
}

/// MPI-044…047: the solid–phase-field coupler on a distributed mesh
/// (phase_field_parallel reduced).
void scenario_phase_field_square() {
  const auto scenario = std::string("phase_field_square_n8");
  ap::FixtureWriter w(g_root, case_id(scenario),
                      "CouplerSolidPhaseField (material_notch.dat) on a distributed Tri3 square, "
                      "3 load steps x 2 staggered solves, rank view",
                      kProbe, kProbeVersion5);
  w.add_tag("parallel");
  record_pin(w);
  Mesh mesh(2);
  if (g_rank == 0) {
    square_tri3_ac(mesh, 8);
  }
  distribute_scotch(mesh, w);
  TypeTable tt;
  record_distribution(w, mesh, tt);
  parse_dat("model solid_mechanics_model [\nmaterial phasefield [\n\t name = plate\n\t rho = 1.\n"
            "\t E = 210.0\n\t nu = 0.3\n\t eta = 0\n\t Plane_Stress = false\n]\n]\n\n"
            "model phase_field_model [\nphasefield quadratic [\n      name = plate\n"
            "      l0 = 0.2\n      gc = 2.7e-3\n      E  = 210.0\n      nu = 0.3\n]\n]\n",
            "material_notch.dat");
  CouplerSolidPhaseField coupler(mesh);
  auto & model = coupler.getSolidMechanicsModel();
  auto & phase = coupler.getPhaseFieldModel();
  if (g_size == 1) {
    ModelSolverOptions so;
    so.non_linear_solver_type = NonLinearSolverType::_auto;
    so.sparse_solver_type = SparseSolverType::_eigen;
    model.initFull(_analysis_method = _static, _solver_options = so);
    phase.initFull(_analysis_method = _static, _solver_options = so);
    w.parameters()["sparse_solver_type"] = std::string("eigen");
  } else {
    model.initFull(_analysis_method = _static);
    phase.initFull(_analysis_method = _static);
    w.parameters()["sparse_solver_type"] =
        std::string("default (MUMPS; _eigen is not distributed)");
  }
  const std::vector<std::string> smm_internals{
      "stress", "eigen_grad_u", "grad_u", "potential_energy", "damage", "epsilon_th",
      "sigma_th", "delta_T"};
  const std::vector<std::string> pf_internals{
      "g_c",    "damage",         "grad_d",        "phi",
      "strain", "driving_force",  "driving_energy", "damage_energy",
      "damage_energy_density", "dissipated_energy"};
  auto c1 = canonicalize_law_ghosts(model, mesh, smm_internals);
  auto c2 = canonicalize_law_ghosts(phase, mesh, pf_internals);
  record_law_filters(w, model, "solid_law");
  record_law_filters(w, phase, "phase_law");
  w.parameters()["ghost_filter_order"] = std::string("canonical rank order (probe patch)");
  w.parameters()["ghost_filter_reorder_validity"] =
      std::string("valid: every ghost block of the listed internals is position-independent "
                  "(all values equal) before the rewrite (D-035, UD-232)");
  w.parameters()["solid_ghost_internals_checked"] = ap::Json::array_of(c1);
  w.parameters()["phase_ghost_internals_checked"] = ap::Json::array_of(c2);
  auto & blocked = model.getBlockedDOFs();
  const auto & x = mesh.getNodes();
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    if (x(n, 1) == 0. or x(n, 1) == 1.) {
      blocked(n, 1) = true;
    }
    if (x(n, 0) == 0.) {
      blocked(n, 0) = true;
    }
  }
  std::vector<double> edis_local, edis, epot_local, epot;
  for (int s = 0; s < 3; ++s) {
    auto & u = model.getDisplacement();
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      if (x(n, 1) == 1.) {
        u(n, 1) += 2e-3;
      }
    }
    for (int k = 0; k < 2; ++k) {
      coupler.solve();
    }
    Real el = 0.;
    for (Int m = 0; m < phase.getNbConstitutiveLaws(); ++m) {
      el += phase.getConstitutiveLaw(m).getEnergy("dissipated");
    }
    edis_local.push_back(el);
    edis.push_back(phase.getEnergy("dissipated"));
    Real pl = 0.;
    for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
      pl += model.getMaterial(m).getEnergy("potential");
    }
    epot_local.push_back(pl);
    epot.push_back(model.getEnergy("potential"));
  }
  w.add("energy/dissipated_local", {edis_local.size()}, {"step"}, "table", edis_local)
      .description("sum over the phase-field laws of getEnergy(dissipated) on this rank (the "
                   "rank's contribution, D-039 amendment)")
      .tolerance_class("exact");
  w.add("energy/dissipated", {edis.size()}, {"step"}, "table", edis)
      .description("PhaseFieldModel::getEnergy(dissipated) (MPI_Allreduce)")
      .tolerance_class("exact");
  w.add("energy/potential_local", {epot_local.size()}, {"step"}, "table", epot_local)
      .description("sum over the materials of getEnergy(potential) on this rank")
      .tolerance_class("exact");
  w.add("energy/potential", {epot.size()}, {"step"}, "table", epot)
      .description("SolidMechanicsModel::getEnergy(potential) (MPI_Allreduce)")
      .tolerance_class("exact");
  add_r(w, "solid/displacement", reals(model.getDisplacement()), 2, "displacement after 3 steps");
  add_r(w, "phase/damage", reals(phase.getDamage()), 1, "nodal damage after 3 steps");
  for (const auto * id : {"damage", "phi", "driving_force"}) {
    const auto & internal = std::as_const(phase.getConstitutiveLaw(0)).getInternal<Real>(id);
    for (auto type : internal.elementTypes(_ghost_type = _not_ghost)) {
      add_r(w, std::string("phase_law0/") + id + "/" + type_name(type) + "/not_ghost",
            reals(internal(type, _not_ghost)), internal(type, _not_ghost).getNbComponent(),
            std::string(id) + " per law-local element and quadrature point");
    }
  }
  w.finish();
}

int main(int argc, char * argv[]) {
  // D-050 pin (the run-time pin: the x86_64 oracle host is the unpinned
  // build); recorded in the version-3 cases (record_pin)
  Eigen::setCpuCacheSizes(kEigenL1, kEigenL2, kEigenL3);
  if (argc < 4) {
    std::cerr << "usage: parallel_probe <fixtures_root> <frozen_meshes_root> <scratch>\n";
    return 2;
  }
  g_root = argv[1];
  g_frozen = argv[2];
  g_scratch = argv[3];
  if (argc > 4) {
    g_adv2d = argv[4];
  }
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  const auto & comm = Communicator::getStaticCommunicator();
  g_rank = comm.whoAmI();
  g_size = comm.getNbProc();
  try {
    const char * only = std::getenv("PARALLEL_PROBE_SCENARIOS");
    const std::string selected = only ? std::string(",") + only + "," : std::string();
    auto want = [&](const std::string & name) {
      return selected.empty() or selected.find("," + name + ",") != std::string::npos;
    };
    write_material();
    if (want("distribution_cube_msh")) {
      scenario_distribution();
    }
    if (want("smm_explicit_cube_n4")) {
      scenario_explicit();
    }
    // MUMPS' parallel solution on 4 processes is not reproducible run to
    // run (last bits; observed with two runs): the static scenario is
    // recorded on 1 and 2 processes only.
    if (g_size <= 2 and want("smm_static_cube_n3")) {
      scenario_static();
    }
    if (not g_adv2d.empty() and want("distribution_adv2d")) {
      scenario_adv2d();
    }
    if (want("facets_cube_msh")) {
      scenario_facets_cube_msh();
    }
    if (want("facets_cube_n4")) {
      scenario_facets_cube_n4();
    }
    if (not g_adv2d.empty() and want("facets_adv2d")) {
      scenario_facets_adv2d();
    }
    // UD-251 probe: only when selected explicitly (upstream does not
    // complete on more than one rank)
    if (not selected.empty() and want("facets_hex_prism")) {
      scenario_facets_hex_prism();
    }
    if (want("grid_tri_n8")) {
      scenario_grid_tri_n8();
    }
    if (want("grid_cube_n4")) {
      scenario_grid_cube_n4();
    }
    for (auto [d, pd] : std::vector<std::pair<Int, Int>>{{1, 1}, {2, 1}, {2, 2}, {3, 1}, {3, 2}, {3, 3}}) {
      const std::string name = "periodic_" + std::to_string(d) + "d_" + std::string("xyz").substr(0, pd);
      if (want(name)) {
        scenario_periodic(d, pd);
      }
    }
    v101_dispatch(want, [&](const std::string & n) { return not selected.empty() and want(n); });
    // Probe version 4
    if (want("distribution_square_data")) {
      scenario_square_data();
    }
    // only when selected explicitly (it
    // replaces the static parser's materials for its run)
    if (not selected.empty() and want("smm_explicit_two_laws_cube_n4")) {
      scenario_explicit_two_laws();
    }
    // UD-655 reproduction: only when selected explicitly (upstream aborts)
    if (not selected.empty() and want("distribution_square_periodic")) {
      scenario_square_periodic();
    }
    // Probe version 6
    if (not g_adv2d.empty() and want("pbd_strip_scotch")) {
      scenario_pbd_strip_scotch();
    }
    if (not g_adv2d.empty() and want("pbd_strip_mesh_data")) {
      scenario_pbd_strip_mesh_data();
    }
    if (not g_adv2d.empty() and want("distribution_sparse_mixed")) {
      scenario_sparse_mixed();
    }
    // one process, only when selected
    // explicitly (a toolchain observation)
    if (g_size == 1 and not selected.empty() and want("ud1060_vector_growth")) {
      scenario_ud1060_vector_growth();
    }
    // Probe version 5
    if (want("clh_marigo_cube_n3")) {
      scenario_clh_marigo();
    }
    if (want("diffusion_gradient_cube_n3")) {
      scenario_diffusion_gradient();
    }
    if (want("phase_field_square_n8")) {
      scenario_phase_field_square();
    }
  } catch (std::exception & e) {
    std::cerr << "parallel_probe (rank " << g_rank << "): " << e.what() << "\n";
    return 1;
  }
  finalize();
  return 0;
}
