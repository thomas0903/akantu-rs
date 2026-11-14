// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): SolidMechanicsModelCohesive on a
// distributed mesh (solid_mechanics_model_cohesive{,_parallel}.cc,
// cohesive_element_inserter{,_helper}.cc, global_ids_updater.cc,
// facet_synchronizer.cc, element_synchronizer.cc). Run under `mpirun -np P`
// once per scenario; every rank writes its own case
//   parallel_cohesive/<scenario>/np<P>/rank<r>
// with what that rank holds at the recorded checkpoints: node global ids,
// flags, owner ranks and coordinates (doubled nodes appended), every
// connectivity of the mesh and of the facet mesh, element_to_subelement /
// subelement_to_element of the facet mesh, the send / receive schemes of the
// element, node, facet, cohesive and facet-stress synchronizers (entry
// order fixes the packing order), the inserter's check / insertion facets,
// the facet stresses, every law's element filter (ghost filters: canonical
// rank order, see below) and the cohesive internals (local and ghost
// blocks), the nodal fields, and per step the inserted count and the
// energies (global, MPI_Allreduce, and this rank's contribution).
//
// Meshes are generated (no Gmsh): a Tri3 square [-1, 1]^2 with n = 8 cells
// per side (diagonal (i, j)-(i+1, j+1)) and a Kuhn Tet4 cube [0, 1]^3 with
// n = 4. Partitions are MeshPartitionMeshData rules on the element
// barycentres (recorded as partition/linearized), built so that at 4 ranks
// the crack meets junctions of three ranks and contains a facet that is
// local on a rank owning none of its nodes:
//   2-D, crack x = xc: P = 2: x < xc -> 0, else 1. P = 4: left of the crack
//     y < 0 -> 0, else 1; right y < 0.25 -> 2, else 3, except the cells
//     (ic, 4) and (ic, 6) (ic = first column right of the crack) -> 0. The
//     crack facet between (ic-1, 5) [rank 1] and (ic, 5) [rank 3] is local
//     on rank 1, both its nodes are owned by rank 0; the nodes (xc, 0) and
//     (xc, 0.25) are junctions of ranks {0, 1, 2} and {0, 1, 3}.
//   3-D, crack x = 0.5: P = 2: x < 0.5 -> 0, else 1. P = 4: left y < 0.5 -> 0,
//     else 1; right z < 0.5 -> 2, else 3, except the cells (2, 2, 0) and
//     (2, 2, 2) -> 0: the two crack facets on the face x = 0.5 of cell
//     (1, 2, 1) are local on rank 1 with every node owned by rank 0.
//   2-D at P = 8 (probe version 4; the 2-D scenarios
//     recorded at 8 ranks): left of the crack y < -0.5 -> 0, y < 0 -> 1,
//     y < 0.5 -> 2, else 3; right y < -0.75 -> 4, y < -0.25 -> 5,
//     y < 0.25 -> 6, else 7, except the cells (ic, 4) and (ic, 6) -> 0 as
//     at P = 4. Every rank holds elements; the crack facet between
//     (ic-1, 5) [rank 2] and (ic, 5) [rank 7] has both nodes owned by
//     rank 0, and (xc, 0.25) is a junction of ranks {0, 2, 7}. No 3-D rule
//     at P = 8 (the 3-D scenarios are recorded at 1, 2 and 4 ranks only).
//
// Scenarios (probe version 1):
//   intrinsic_tri       intrinsic cohesive_bilinear on the x = -0.25 line
//                       (inserter bounding_box), explicit, v_x = 0.2 x,
//                       x = 1 blocked in x (test_cohesive_parallel_intrinsic)
//   intrinsic_tet       the same in 3-D on the plane x = 0.5
//                       (test_cohesive_parallel_intrinsic_tetrahedron)
//   extrinsic_tri       extrinsic cohesive_linear, check facets on x = 0
//                       (bounding_box), pulled in x
//                       (test_cohesive_parallel_extrinsic)
//   extrinsic_tet       the same in 3-D on the plane x = 0.5
//                       (test_cohesive_parallel_extrinsic_tetrahedron)
//   extrinsic_igtg_tri  two cohesive laws chosen by an intergranular /
//                       transgranular selector (grain = side of x = 0),
//                       check facets in |x| < 0.2, biaxial pull
//                       (test_cohesive_parallel_extrinsic_IG_TG)
//   facet_stress_tet10  frozen test_cohesive_facet_stress_synchronizer/
//                       tetrahedron.msh (Tet10), extrinsic, bulk stress set
//                       to the linear field of the upstream test, then
//                       checkCohesiveStress: facet stresses of both sides
//                       (test_cohesive_facet_stress_synchronizer)
//
// Scenarios added in probe version 2 (version-1 scenarios
// unchanged):
//   facets_only_tri     extrinsic model of extrinsic_tri (2-D rule, xc = 0),
//                       10 explicit steps (pull, no stress check), then the
//                       not-ghost facets of a T crack inside rank 0 (x = -0.5,
//                       y <= 0 and y = -0.5, x <= -0.5) are flagged on every
//                       rank and CohesiveElementInserter::insertElements(true)
//                       doubles them (ghost flags through _ce_groups), then
//                       20 steps
//   facets_only_tet     the same in 3-D (planes x = 0.25 with y <= 0.5, and
//                       z = 0.5 with x <= 0.25, y <= 0.5)
//   facets_only_shared_tri  as facets_only_tri with the crack on the rank
//                       boundary x = 0 (and y = 0, x <= 0): upstream leaves
//                       the doubled slave nodes without global id / prank and
//                       asserts in NodeSynchronizer::onNodesAdded at 2 and 4
//                       ranks (UD-1043 evidence, not promoted)
//   clusters_tri        Mesh::createClusters(2, "cluster", filter) on the
//                       distributed Tri3 square (filter: cells of row j = 3 and
//                       of the columns i = 2, 5 above it removed) and
//                       createClusters(2, "all"): returned counts, every
//                       cluster group's elements and nodes (MESH-026)
//   fragments_tri       extrinsic cohesive_linear (sigma_c = 1, G_c = 1e-3), the
//                       check facets restricted to the lines x = -0.5, 0, 0.5,
//                       u_x = 0.1 x, v = (x, 0.5 y), assembleInternalForces,
//                       checkCohesiveStress, a FragmentManager (dump_data =
//                       false) and computeAllData() after 0..3 displacement
//                       stages that break one line each (upstream
//                       displaceElements): fragment groups, damage after
//                       _smmc_damage, the global arrays and this rank's
//                       contributions to each all-reduced integral (replicated
//                       integrateFieldOnFragments, checked bitwise against
//                       upstream's own MPI_Allreduce of them) (COH-008)
//
// Scenarios added in probe version 3 (the review's adversarial cases,
// committed; version-1 and -2 payloads unchanged):
//   facets_only_pg_tri  as facets_only_tri with the crack y = -0.5, x <= 0:
//                       it ends on the rank boundary x = 0 (its tip node is
//                       not doubled; only rank 0 doubles nodes, the other
//                       ranks double only the ghost copies of the cracked
//                       facets)
//   facets_only_bnd_tri as facets_only_tri with the single facet x = 0,
//                       -0.5 <= y <= -0.25 on the rank boundary: doubled
//                       without any node on any rank (no NewNodesEvent, the
//                       node schemes stay as they are; the facet-stress
//                       schemes are pruned)
//   clusters_chk_tri    Mesh::createClusters(2, "chk", filter) on the
//                       distributed Tri3 square with a checkerboard filter
//                       (cells (i, j) with i + j even: 32 clusters, many held
//                       as ghosts by other ranks), after the last rank created
//                       an element group of its own (the final
//                       synchronizeGroupNames)
//
// Probe-side patch (as parallel_probe, D-035 / UD-232): upstream appends
// ghost elements to their law's filter while unpacking
// _constitutive_law_id in MPI_Waitany completion order, so with two or more
// neighbours the ghost filter order, hence the order in which ghost
// elements are assembled, is run-dependent. The probe rewrites every law's
// ghost filter (bulk after initFull, cohesive after every insertion: only
// the entries appended by that insertion) into the order a rank-order
// unpack gives (receive schemes in rank order, scheme order), after
// checking that the moved entries hold identical values in every Real
// internal (so the rewrite moves no state).
//
// Usage: parallel_cohesive_probe <fixtures_root> <frozen_meshes_root>
//                                <scratch_dir> <scenario>
// The Eigen cache sizes are pinned at the start of main (D-050) and recorded with OPENBLAS_CORETYPE / OPENBLAS_NUM_THREADS.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <aka_random_generator.hh>
#include <cohesive_element_inserter.hh>
#include <communicator.hh>
#include <element_synchronizer.hh>
#include <facet_synchronizer.hh>
#include <fragment_manager.hh>
#include <material_cohesive.hh>
#include <material_selector_cohesive.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_partition_mesh_data.hh>
#include <node_synchronizer.hh>
#include <solid_mechanics_model_cohesive.hh>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <cstring>
#include <filesystem>
#include <fstream>
#include <functional>
#include <iostream>
#include <map>
#include <set>
#include <sstream>
#include <stdexcept>
#include <string>
#include <tuple>
#include <utility>
#include <vector>

#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

/* -------------------------------------------------------------------------- */
/* Access to the private facet-stress synchronizer (explicit-instantiation    */
/* member pointer: access checking does not apply to explicit instantiation   */
/* arguments; no `#define private public`)                                    */
/* -------------------------------------------------------------------------- */
namespace {
template <typename Tag, typename Tag::type M> struct Rob {
  friend typename Tag::type get(Tag) { return M; }
};
struct FacetStressSyncTag {
  using type = std::unique_ptr<ElementSynchronizer> SolidMechanicsModelCohesive::*;
  friend type get(FacetStressSyncTag);
};
template struct Rob<FacetStressSyncTag, &SolidMechanicsModelCohesive::facet_stress_synchronizer>;

const ElementSynchronizer * facet_stress_synchronizer(SolidMechanicsModelCohesive & model) {
  return (model.*get(FacetStressSyncTag())).get();
}
} // namespace

namespace {

constexpr const char * kProbe = "parallel_cohesive_probe";
constexpr const char * kProbeVersion = "2";
// The review's adversarial cases (facets_only_pg_tri, facets_only_bnd_tri,
// clusters_chk_tri); the version-1 and -2 scenarios are unchanged.
constexpr const char * kProbeVersion3 = "3";
// The 2-D partition rule at 8 ranks (cases at np8 only; the
// payloads at 1, 2 and 4 ranks are unchanged).
constexpr const char * kProbeVersion4 = "4";
constexpr int kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;
using I64 = std::int64_t;

fs::path g_root, g_frozen, g_scratch;
Int g_rank = 0, g_size = 1;

// Step budgets and loading rates (pre-screened with the serial port, which
// is bitwise with the serial oracle on these laws: the first insertion and
// the opening fall inside the budget).
constexpr int kIntrinsicTriSteps = 100;
constexpr int kIntrinsicTetSteps = 60;
constexpr int kExtrinsicTriSteps = 80;
constexpr int kExtrinsicTetSteps = 80;
constexpr int kIgtgSteps = 120;
constexpr Real kPullRate = 0.5;
constexpr Real kBiaxialRate = 0.1;

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string ghost_name(GhostType g) { return g == _not_ghost ? "not_ghost" : "ghost"; }

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

void add_steps(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
               const std::string & desc) {
  w.add(name, {v.size()}, {"step"}, "table", v).description(desc).tolerance_class("exact");
}

void add_isteps(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
                const std::string & desc) {
  w.add(name, {v.size()}, {"step"}, "table", v).description(desc).tolerance_class("exact");
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
  return "parallel_cohesive/" + scenario + "/np" + std::to_string(g_size) + "/rank" +
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

/// (type code, element, ghost); ElementNull = (-1, -1, -1); a default
/// Element (type _not_defined) = (-2, element, ghost).
void push_element(std::vector<I64> & v, const Element & el, TypeTable & tt) {
  if (el == ElementNull) {
    v.insert(v.end(), {-1, -1, -1});
    return;
  }
  v.push_back(el.type == _not_defined ? -2 : tt.code(el.type));
  v.push_back(el.element);
  v.push_back(el.ghost_type == _not_ghost ? 0 : 1);
}

/* -------------------------------------------------------------------------- */
/* Meshes                                                                     */
/* -------------------------------------------------------------------------- */

/// Tri3 square [-1, 1]^2, n cells per side, cell (i, j) split along the
/// diagonal (i, j)-(i+1, j+1): triangles (n0, n1, n2) and (n0, n2, n3).
void square_tri3(Mesh & mesh, Int n) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  const Real h = 2.0 / n;
  auto id = [&](Int i, Int j) { return j * (n + 1) + i; };
  for (Int j = 0; j <= n; ++j) {
    for (Int i = 0; i <= n; ++i) {
      Vector<Real> x(2);
      x(0) = -1.0 + i * h;
      x(1) = -1.0 + j * h;
      nodes.push_back(x);
    }
  }
  auto & conn = acc.getConnectivity(_triangle_3);
  for (Int j = 0; j < n; ++j) {
    for (Int i = 0; i < n; ++i) {
      Idx n0 = id(i, j), n1 = id(i + 1, j), n2 = id(i + 1, j + 1), n3 = id(i, j + 1);
      Vector<Idx> a(3), b(3);
      a(0) = n0;
      a(1) = n1;
      a(2) = n2;
      b(0) = n0;
      b(1) = n2;
      b(2) = n3;
      conn.push_back(a);
      conn.push_back(b);
    }
  }
  acc.makeReady();
}

/// Kuhn Tet4 cube [0, 1]^3 (as parallel_probe / par_smm).
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
        Idx v[8] = {id(i, j, k),         id(i + 1, j, k),         id(i + 1, j + 1, k),
                    id(i, j + 1, k),     id(i, j, k + 1),         id(i + 1, j, k + 1),
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

/* -------------------------------------------------------------------------- */
/* Partitions                                                                 */
/* -------------------------------------------------------------------------- */

/// 2-D rule (see the file header), crack x = xc on the n = 8 square.
Idx part_2d(const Vector<Real> & b, Real xc) {
  if (g_size == 1) {
    return 0;
  }
  const bool right = b(0) > xc;
  if (g_size == 2) {
    return right ? 1 : 0;
  }
  if (g_size != 4 and g_size != 8) {
    throw std::runtime_error("2-D partition rule defined for 1, 2, 4 and 8 ranks");
  }
  const Real h = 0.25;
  const Int i = Int(std::floor((b(0) + 1.0) / h));
  const Int j = Int(std::floor((b(1) + 1.0) / h));
  const Int ic = Int(std::lround((xc + 1.0) / h));
  if (g_size == 8) {
    // four strips on each side of the
    // crack, the right ones shifted by a quarter of a strip, and the two
    // cells of the 4-rank rule given to rank 0 (see the file header)
    if (not right) {
      return b(1) < -0.5 ? 0 : b(1) < 0.0 ? 1 : b(1) < 0.5 ? 2 : 3;
    }
    if (i == ic and (j == 4 or j == 6)) {
      return 0;
    }
    return b(1) < -0.75 ? 4 : b(1) < -0.25 ? 5 : b(1) < 0.25 ? 6 : 7;
  }
  if (not right) {
    return b(1) < 0.0 ? 0 : 1;
  }
  if (i == ic and (j == 4 or j == 6)) {
    return 0;
  }
  return b(1) < 0.25 ? 2 : 3;
}

/// 3-D rule (see the file header), crack plane x = 0.5 on the n = 4 cube.
Idx part_3d(const Vector<Real> & b) {
  if (g_size == 1) {
    return 0;
  }
  const bool right = b(0) > 0.5;
  if (g_size == 2) {
    return right ? 1 : 0;
  }
  if (g_size != 4) {
    throw std::runtime_error("3-D partition rule defined for 1, 2 and 4 ranks");
  }
  const Real h = 0.25;
  const Int i = Int(std::floor(b(0) / h));
  const Int j = Int(std::floor(b(1) / h));
  const Int k = Int(std::floor(b(2) / h));
  if (not right) {
    return b(1) < 0.5 ? 0 : 1;
  }
  if (i == 2 and j == 2 and (k == 0 or k == 2)) {
    return 0;
  }
  return b(2) < 0.5 ? 2 : 3;
}

/// Octant rule on the box of a read mesh (facet_stress_tet10).
Idx part_box(const Vector<Real> & b, const Vector<Real> & centre) {
  if (g_size == 1) {
    return 0;
  }
  Idx p = b(0) < centre(0) ? 0 : 1;
  if (g_size == 2) {
    return p;
  }
  if (g_size != 4) {
    throw std::runtime_error("box partition rule defined for 1, 2 and 4 ranks");
  }
  return p + (b(1) < centre(1) ? 0 : 2);
}

/// MeshPartitionMeshData from a barycentre rule on rank 0 (P > 1), recorded,
/// then Mesh::distribute (on one process upstream only marks the mesh
/// distributed).
void distribute_rule(Mesh & mesh, ap::FixtureWriter & w,
                     const std::function<Idx(const Vector<Real> &)> & rule,
                     const std::string & rule_text) {
  std::shared_ptr<MeshPartition> partition;
  const Int dim = mesh.getSpatialDimension();
  if (g_size > 1 and g_rank == 0) {
    auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("coh_partition");
    std::vector<I64> lin;
    for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
      auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
      for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
        Vector<Real> b(dim);
        mesh.getBarycenter(Element{type, e, _not_ghost}, b);
        a(e) = rule(b);
        lin.push_back(a(e));
      }
    }
    partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, dim);
    partition->partitionate(g_size);
    std::vector<I64> got;
    for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
      for (auto p : partition->getPartition(type, _not_ghost)) {
        got.push_back(p);
      }
    }
    if (got != lin) {
      throw std::runtime_error("MeshPartitionMeshData changed the prescribed parts");
    }
    add_i(w, "partition/linearized", lin, 1,
          "part of every element of the mesh dimension (types in enum order), barycentre rule");
    w.parameters()["partition/rule"] = rule_text;
  }
  mesh.distribute(partition);
}

/* -------------------------------------------------------------------------- */
/* Records                                                                    */
/* -------------------------------------------------------------------------- */

void record_schemes(ap::FixtureWriter & w, const std::string & prefix,
                    const ElementSynchronizer & s, TypeTable & tt) {
  std::vector<std::string> keys;
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : s.getCommunications().iterateSchemes(sr)) {
      std::vector<I64> v;
      for (auto && el : scheme) {
        push_element(v, el, tt);
      }
      auto key = std::string(sr == _send ? "send" : "recv") + "/" + std::to_string(proc);
      keys.push_back(key);
      add_i(w, prefix + key, v, 3, "(type code, element, ghost) in scheme order");
    }
  }
  w.parameters()[prefix + "keys"] = ap::Json::array_of(keys);
}

void record_node_schemes(ap::FixtureWriter & w, const std::string & prefix, Mesh & mesh) {
  std::vector<std::string> keys;
  const auto & ns = std::as_const(mesh).getNodeSynchronizer();
  for (auto sr : {_send, _recv}) {
    for (auto && [proc, scheme] : ns.getCommunications().iterateSchemes(sr)) {
      auto key = std::string(sr == _send ? "send" : "recv") + "/" + std::to_string(proc);
      keys.push_back(key);
      add_i(w, prefix + key, ints(scheme), 1, "local nodes in scheme order");
    }
  }
  w.parameters()[prefix + "keys"] = ap::Json::array_of(keys);
}

void record_connectivities(ap::FixtureWriter & w, const std::string & prefix, const Mesh & mesh) {
  std::vector<std::string> blocks;
  for (auto gt : ghost_types) {
    for (auto type : mesh.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      const auto & conn = mesh.getConnectivity(type, gt);
      auto key = type_name(type) + "/" + ghost_name(gt);
      blocks.push_back(key);
      add_i(w, prefix + key, ints(conn), conn.getNbComponent(), "connectivity (local node ids)");
    }
  }
  w.parameters()[prefix + "blocks"] = ap::Json::array_of(blocks);
}

/// element_to_subelement (flattened triples + row offsets) and
/// subelement_to_element (triples per row) of every block of the facet mesh.
void record_adjacency(ap::FixtureWriter & w, const std::string & prefix, const Mesh & mf,
                      TypeTable & tt) {
  std::vector<std::string> e2s_blocks, s2e_blocks;
  const auto & e2s_map = mf.getElementToSubelement();
  const auto & s2e_map = mf.getSubelementToElement();
  for (auto gt : ghost_types) {
    for (auto type : e2s_map.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      const auto & e2s = e2s_map(type, gt);
      std::vector<I64> flat, offsets{0};
      for (Idx r = 0; r < e2s.size(); ++r) {
        for (const auto & e : e2s(r)) {
          push_element(flat, e, tt);
        }
        offsets.push_back(I64(flat.size() / 3));
      }
      auto key = type_name(type) + "/" + ghost_name(gt);
      e2s_blocks.push_back(key);
      add_i(w, prefix + "element_to_subelement/" + key, flat, 3,
            "element_to_subelement rows, flattened (type code, element, ghost)");
      add_i(w, prefix + "element_to_subelement_offsets/" + key, offsets, 1,
            "row offsets (in triples) of element_to_subelement");
    }
    for (auto type : s2e_map.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      const auto & s2e = s2e_map(type, gt);
      std::vector<I64> flat;
      for (Idx i = 0; i < s2e.size() * s2e.getNbComponent(); ++i) {
        push_element(flat, s2e.data()[i], tt);
      }
      auto key = type_name(type) + "/" + ghost_name(gt);
      s2e_blocks.push_back(key);
      add_i(w, prefix + "subelement_to_element/" + key, flat, 3,
            "subelement_to_element (rows of nb_component triples)");
      w.parameters()[prefix + "subelement_to_element_components/" + key] =
          static_cast<long long>(s2e.getNbComponent());
    }
  }
  w.parameters()[prefix + "element_to_subelement_blocks"] = ap::Json::array_of(e2s_blocks);
  w.parameters()[prefix + "subelement_to_element_blocks"] = ap::Json::array_of(s2e_blocks);
}

void record_nodes(ap::FixtureWriter & w, const std::string & prefix, Mesh & mesh) {
  const auto nb_nodes = mesh.getNbNodes();
  std::vector<I64> gids(nb_nodes), flags(nb_nodes), pranks(nb_nodes);
  for (Idx n = 0; n < nb_nodes; ++n) {
    gids[n] = mesh.getNodeGlobalId(n);
    flags[n] = static_cast<I64>(static_cast<std::uint8_t>(mesh.getNodeFlag(n)));
    pranks[n] = mesh.getNodePrank(n);
  }
  add_i(w, prefix + "nodes/global_ids", gids, 1, "getNodeGlobalId per local node");
  add_i(w, prefix + "nodes/flags", flags, 1, "getNodeFlag per local node (NodeFlag bits)");
  add_i(w, prefix + "nodes/pranks", pranks, 1, "getNodePrank per local node (-1: none)");
  add_r(w, prefix + "nodes/coordinates", reals(mesh.getNodes()), mesh.getSpatialDimension(),
        "node coordinates");
  w.parameters()[prefix + "nb_global_nodes"] = static_cast<long long>(mesh.getNbGlobalNodes());
  w.parameters()[prefix + "nb_nodes"] = static_cast<long long>(nb_nodes);
}

void record_bool_map(ap::FixtureWriter & w, const std::string & prefix,
                     const ElementTypeMapArray<bool> & m) {
  std::vector<std::string> blocks;
  for (auto gt : ghost_types) {
    for (auto type : m.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      auto key = type_name(type) + "/" + ghost_name(gt);
      blocks.push_back(key);
      add_i(w, prefix + key, ints(m(type, gt)), 1, "per facet flag");
    }
  }
  w.parameters()[prefix + "blocks"] = ap::Json::array_of(blocks);
}

std::vector<std::string> cohesive_internal_names() {
  return {"opening",          "tractions",   "contact_tractions", "contact_opening",
          "delta max",        "damage",      "normal",            "reversible_energy",
          "total_energy",     "sigma_c_eff", "delta_c_eff",       "insertion_stress",
          "opening_prec",     "delta_max_prev"};
}

std::string key_of(std::string s) {
  for (auto & c : s) {
    if (c == ' ') {
      c = '_';
    }
  }
  return s;
}

/// Every law's element filter (both ghost types) and, for cohesive laws,
/// their internals (both ghost types), facet filter and facet sigma_c.
void record_laws(ap::FixtureWriter & w, const std::string & prefix,
                 SolidMechanicsModelCohesive & model) {
  const Int dim = model.getSpatialDimension();
  std::vector<std::string> names;
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & mat = model.getMaterial(m);
    const auto & cmat = static_cast<const Material &>(mat);
    names.push_back(mat.getName());
    const auto lp = prefix + "law" + std::to_string(m) + "/";
    const auto & filter = cmat.getElementFilter();
    for (auto gt : ghost_types) {
      for (auto type : filter.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        add_i(w, lp + "filter/" + type_name(type) + "/" + ghost_name(gt), ints(filter(type, gt)),
              1, "law element filter (ghost: canonical rank order, probe patch)");
      }
    }
    auto * coh = dynamic_cast<MaterialCohesive *>(&mat);
    if (coh == nullptr) {
      continue;
    }
    std::vector<std::string> recorded;
    for (const auto & internal : cohesive_internal_names()) {
      if (not mat.isInternal<Real>(internal, _ek_cohesive)) {
        continue;
      }
      const auto & field = std::as_const(mat).getInternal<Real>(internal);
      for (auto gt : ghost_types) {
        for (auto type : field.elementTypes(dim, gt, _ek_cohesive)) {
          add_r(w, lp + key_of(internal) + "/" + type_name(type) + "/" + ghost_name(gt),
                reals(field(type, gt)), field(type, gt).getNbComponent(),
                "cohesive internal " + internal);
        }
      }
      recorded.push_back(internal);
    }
    w.parameters()[lp + "cohesive_internals"] = ap::Json::array_of(recorded);
    const auto & ccoh = static_cast<const MaterialCohesive &>(*coh);
    const auto & ff = ccoh.getFacetFilter();
    for (auto gt : ghost_types) {
      for (auto type : ff.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        add_i(w, lp + "facet_filter/" + type_name(type) + "/" + ghost_name(gt), ints(ff(type, gt)),
              1, "facet filter (extrinsic)");
      }
    }
    if (mat.isInternal<Real>("sigma_c", _ek_regular)) {
      const auto & sc = std::as_const(mat).getInternal<Real>("sigma_c");
      for (auto gt : ghost_types) {
        for (auto type : sc.elementTypes(dim - 1, gt, _ek_regular)) {
          add_r(w, lp + "sigma_c/" + type_name(type) + "/" + ghost_name(gt), reals(sc(type, gt)),
                1, "facet strength sigma_c (per facet quadrature point of the facet filter)");
        }
      }
    }
  }
  w.parameters()[prefix + "law_names"] = ap::Json::array_of(names);
}

/// Facet stresses of the facets in `selected` (per block: facet indices,
/// then their quadrature-point values; size policy).
void record_facet_stress(ap::FixtureWriter & w, const std::string & prefix,
                         SolidMechanicsModelCohesive & model,
                         const ElementTypeMapArray<bool> & selected) {
  const Int dim = model.getSpatialDimension();
  const auto & mf = model.getMeshFacets();
  std::vector<std::string> blocks;
  for (auto gt : ghost_types) {
    for (auto type : mf.elementTypes(dim - 1, gt, _ek_regular)) {
      if (mf.getNbElement(type, gt) == 0 or not selected.exists(type, gt)) {
        continue;
      }
      const auto & fs = model.getStressOnFacets(type, gt);
      const auto & sel = selected(type, gt);
      const Idx nbc = fs.getNbComponent();
      const Idx nq = fs.size() / std::max<Idx>(mf.getNbElement(type, gt), 1);
      std::vector<I64> idx;
      std::vector<double> vals;
      for (Idx f = 0; f < sel.size(); ++f) {
        if (not sel(f)) {
          continue;
        }
        idx.push_back(f);
        for (Idx q = 0; q < nq; ++q) {
          for (Idx c = 0; c < nbc; ++c) {
            vals.push_back(fs(f * nq + q, c));
          }
        }
      }
      auto key = type_name(type) + "/" + ghost_name(gt);
      blocks.push_back(key);
      add_i(w, prefix + "facets/" + key, idx, 1, "facets recorded (checked at init)");
      add_r(w, prefix + "values/" + key, vals, nbc,
            "facet_stress of the recorded facets per quadrature point (side 0 then side 1, "
            "dim^2 each)");
      w.parameters()[prefix + "nb_quad/" + key] = static_cast<long long>(nq);
    }
  }
  w.parameters()[prefix + "blocks"] = ap::Json::array_of(blocks);
}

/// Damage of every cohesive law (both ghost types), after `_smmc_damage`.
void record_damage(ap::FixtureWriter & w, const std::string & prefix,
                   SolidMechanicsModelCohesive & model) {
  const Int dim = model.getSpatialDimension();
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & mat = model.getMaterial(m);
    if (dynamic_cast<MaterialCohesive *>(&mat) == nullptr) {
      continue;
    }
    const auto & field = std::as_const(mat).getInternal<Real>("damage");
    for (auto gt : ghost_types) {
      for (auto type : field.elementTypes(dim, gt, _ek_cohesive)) {
        add_r(w, prefix + "law" + std::to_string(m) + "/damage/" + type_name(type) + "/" +
                     ghost_name(gt),
              reals(field(type, gt)), 1, "cohesive damage");
      }
    }
  }
}

void record_facet_material(ap::FixtureWriter & w, const std::string & prefix,
                           SolidMechanicsModelCohesive & model) {
  const Int dim = model.getSpatialDimension();
  const auto & fm = std::as_const(model).getFacetMaterial();
  for (auto gt : ghost_types) {
    for (auto type : fm.elementTypes(dim - 1, gt, _ek_not_defined)) {
      add_i(w, prefix + type_name(type) + "/" + ghost_name(gt), ints(fm(type, gt)), 1,
            "facet_material");
    }
  }
}

/// What a checkpoint records of the topology (size policy: the facet
/// adjacency only where the topology changed).
enum class Level { Light, Conn, Full };

/// Topology and synchronizers at a checkpoint: Light = nodes, synchronizer
/// schemes, check / insertion facets; Conn = + connectivities of the mesh
/// and the facet mesh; Full = + facet-mesh adjacency.
void record_topology(ap::FixtureWriter & w, const std::string & prefix,
                     SolidMechanicsModelCohesive & model, Mesh & mesh, TypeTable & tt,
                     Level level) {
  record_nodes(w, prefix, mesh);
  const auto & mf = mesh.getMeshFacets();
  if (level != Level::Light) {
    record_connectivities(w, prefix + "conn/", mesh);
    record_connectivities(w, prefix + "facets/conn/", mf);
  }
  if (level == Level::Full) {
    record_adjacency(w, prefix + "facets/", mf, tt);
  }
  if (mesh.isDistributed()) {
    record_schemes(w, prefix + "element_scheme/", std::as_const(mesh).getElementSynchronizer(),
                   tt);
    record_node_schemes(w, prefix + "node_scheme/", mesh);
    record_schemes(w, prefix + "facet_scheme/", std::as_const(mf).getElementSynchronizer(), tt);
    record_schemes(w, prefix + "cohesive_scheme/", model.getCohesiveSynchronizer(), tt);
    if (const auto * fss = facet_stress_synchronizer(model)) {
      record_schemes(w, prefix + "facet_stress_scheme/", *fss, tt);
    }
  }
  auto & inserter = model.getElementInserter();
  record_bool_map(w, prefix + "check_facets/", std::as_const(inserter).getCheckFacets());
  record_bool_map(w, prefix + "insertion_facets/",
                  std::as_const(inserter).getInsertionFacetsByElement());
}

void record_fields(ap::FixtureWriter & w, const std::string & prefix,
                   SolidMechanicsModelCohesive & model) {
  const Int dim = model.getSpatialDimension();
  add_r(w, prefix + "model/displacement", reals(model.getDisplacement()), dim, "displacement");
  add_r(w, prefix + "model/velocity", reals(model.getVelocity()), dim, "velocity");
  add_r(w, prefix + "model/acceleration", reals(model.getAcceleration()), dim, "acceleration");
  add_r(w, prefix + "model/internal_force", reals(model.getInternalForce()), dim,
        "internal force");
  add_r(w, prefix + "model/mass", reals(model.getMass()), dim, "lumped mass");
  add_i(w, prefix + "model/blocked_dofs", ints(model.getBlockedDOFs()), dim, "blocked flags");
}

/* -------------------------------------------------------------------------- */
/* Canonical (rank-order) ghost filters                                       */
/* -------------------------------------------------------------------------- */

/// Rewrite the ghost entries [from(type), size) of every law's filter whose
/// elements appear in the receive schemes of `sync` into rank order
/// (receive schemes in rank order, scheme order). Every moved entry must
/// hold identical values in every Real internal of the law (checked: the
/// rewrite then moves no state). Returns the number of moved blocks.
long long canonicalize_ghost_filters(SolidMechanicsModelCohesive & model,
                                     const ElementSynchronizer & sync,
                                     const std::map<std::pair<Int, ElementType>, Idx> & from) {
  const auto & law_index = std::as_const(model).getConstitutiveLawByElement();
  auto & local_num = const_cast<ElementTypeMapArray<Idx> &>(
      std::as_const(model).getConstitutiveLawLocalNumbering());
  long long moved_blocks = 0;
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    auto & mat = model.getMaterial(m);
    auto & filter = const_cast<ElementTypeMapArray<Idx> &>(std::as_const(mat).getElementFilter());
    std::map<ElementType, std::vector<Idx>> canon;
    for (auto && [proc, scheme] : sync.getCommunications().iterateSchemes(_recv)) {
      for (auto && el : scheme) {
        if (not law_index.exists(el.type, el.ghost_type) or law_index(el) != m) {
          continue;
        }
        auto it = from.find({m, el.type});
        const Idx start = it == from.end() ? 0 : it->second;
        if (local_num(el) < start) {
          continue;
        }
        canon[el.type].push_back(el.element);
      }
    }
    for (auto && [type, list] : canon) {
      auto it = from.find({m, type});
      const Idx start = it == from.end() ? 0 : it->second;
      auto & f = filter(type, _ghost);
      if (Idx(f.size()) - start != Idx(list.size())) {
        throw std::runtime_error("ghost filter tail and receive schemes disagree (law " +
                                 std::to_string(m) + ")");
      }
      bool changed = false;
      for (Idx i = 0; i < Idx(list.size()); ++i) {
        changed = changed or f(start + i) != list[i];
      }
      if (not changed) {
        continue;
      }
      // validity: every Real internal identical over the moved rows
      auto check_names = cohesive_internal_names();
      for (const auto * bulk : {"stress", "grad_u", "potential_energy", "eigen_grad_u",
                                "sigma_c"}) {
        check_names.emplace_back(bulk);
      }
      for (const auto & id : check_names) {
        if (not mat.isInternal<Real>(id, Mesh::getKind(type))) {
          continue;
        }
        const auto & internal = std::as_const(mat).getInternal<Real>(id);
        if (not internal.exists(type, _ghost)) {
          continue;
        }
        const auto & a = internal(type, _ghost);
        const Idx nbc = a.getNbComponent();
        const Idx per = a.size() / std::max<Idx>(f.size(), 1) * nbc;
        for (Idx i = start + 1; i < Idx(f.size()); ++i) {
          for (Idx c = 0; c < per; ++c) {
            if (a.data()[i * per + c] != a.data()[start * per + c]) {
              throw std::runtime_error("ghost internal " + id +
                                       " differs between the moved entries: the canonical "
                                       "ghost order rewrite would move state");
            }
          }
        }
      }
      for (Idx i = 0; i < Idx(list.size()); ++i) {
        f(start + i) = list[i];
        local_num(Element{type, list[i], _ghost}) = start + i;
      }
      ++moved_blocks;
    }
  }
  return moved_blocks;
}

/// Current ghost filter sizes of every law (start of the entries a coming
/// insertion appends).
std::map<std::pair<Int, ElementType>, Idx> ghost_filter_sizes(SolidMechanicsModelCohesive & model) {
  std::map<std::pair<Int, ElementType>, Idx> out;
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    const auto & filter = std::as_const(model.getMaterial(m)).getElementFilter();
    for (auto type : filter.elementTypes(_all_dimensions, _ghost, _ek_not_defined)) {
      out[{m, type}] = filter(type, _ghost).size();
    }
  }
  return out;
}

/* -------------------------------------------------------------------------- */
/* Energies                                                                   */
/* -------------------------------------------------------------------------- */

/// This rank's contribution to getEnergy(kinetic) (upstream
/// SolidMechanicsModel::getKineticEnergy before its allReduce).
Real local_kinetic(SolidMechanicsModelCohesive & model, Mesh & mesh) {
  const Int dim = model.getSpatialDimension();
  Real ekin = 0.;
  const auto & m = model.getMass();
  const auto & v = model.getVelocity();
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    Real mv2 = 0.;
    if (mesh.isLocalOrMasterNode(n)) {
      for (Int i = 0; i < dim; ++i) {
        if (m(n, i) > std::numeric_limits<Real>::epsilon()) {
          mv2 += v(n, i) * v(n, i) * m(n, i);
        }
      }
    }
    ekin += mv2;
  }
  return ekin * .5;
}

/// This rank's contribution to getEnergy(id) for law energies (the sum over
/// the laws before upstream's allReduce).
Real local_law_energy(SolidMechanicsModelCohesive & model, const std::string & id) {
  Real e = 0.;
  for (Int m = 0; m < model.getNbConstitutiveLaws(); ++m) {
    e += model.getMaterial(m).getEnergy(id);
  }
  return e;
}

/* -------------------------------------------------------------------------- */
/* Selector (IG / TG)                                                         */
/* -------------------------------------------------------------------------- */

/// test_cohesive_parallel_extrinsic_IG_TG's MultiGrainMaterialSelector with
/// the grain of an element = side of x = 0 of its barycentre (instead of
/// the tag_0 mesh data of square.msh).
class GrainSelector : public DefaultMaterialCohesiveSelector {
public:
  GrainSelector(const SolidMechanicsModelCohesive & model, ID tg, ID ig)
      : DefaultMaterialCohesiveSelector(model), tg(std::move(tg)), ig(std::move(ig)),
        cmodel(model), cmesh(model.getMesh()), mesh_facets(model.getMeshFacets()),
        dim(model.getSpatialDimension()) {}

  Int operator()(const Element & element) override {
    if (Mesh::getSpatialDimension(element.type) == dim - 1) {
      const auto & e2s =
          mesh_facets.getElementToSubelement(element.type, element.ghost_type)(element.element);
      const auto & el1 = e2s[0];
      const auto & el2 = e2s[1];
      if (el2 == ElementNull) {
        return cmodel.getMaterialIndex(tg);
      }
      return grain(el1) == grain(el2) ? cmodel.getMaterialIndex(tg) : cmodel.getMaterialIndex(ig);
    }
    return DefaultMaterialCohesiveSelector::operator()(element);
  }

private:
  Int grain(const Element & el) const {
    Vector<Real> b(dim);
    cmesh.getBarycenter(el, b);
    return b(0) < 0. ? 0 : 1;
  }
  ID tg, ig;
  const SolidMechanicsModelCohesive & cmodel;
  const Mesh & cmesh;
  const Mesh & mesh_facets;
  Int dim;
};

/* -------------------------------------------------------------------------- */
/* Scenarios                                                                  */
/* -------------------------------------------------------------------------- */

enum class Loading { IntrinsicVelocity, ExtrinsicPull, Biaxial };

struct Spec {
  std::string name;
  std::string desc;
  Int dim;
  bool extrinsic;
  bool grain_selector;
  std::string dat;
  Loading loading;
  Real rate;
  Real dt_factor;
  int nb_steps;
  std::vector<int> checkpoints; // steps after which the full state is recorded
};

void write_dat(const std::string & dat, const fs::path & path) {
  if (g_rank == 0) {
    std::ofstream f(path);
    f << dat;
  }
  Communicator::getStaticCommunicator().barrier();
  getStaticParser().parse(path.string());
}

/// Coordinate in [-1, 1] of the loading axis (x).
Real loading_coordinate(const Spec & s, Real x) { return s.dim == 2 ? x : 2. * x - 1.; }

void run_case(const Spec & s) {
  const auto cid = case_id(s.name);
  ap::FixtureWriter w(g_root, cid, s.desc, kProbe, g_size == 8 ? kProbeVersion4 : kProbeVersion);
  w.add_tag("parallel");
  w.add_tag("cohesive");
  record_pin(w);
  auto & p = w.parameters();
  p["rank"] = static_cast<long long>(g_rank);
  p["nb_ranks"] = static_cast<long long>(g_size);
  p["scenario"] = s.name;
  p["extrinsic"] = s.extrinsic;
  p["grain_selector"] = s.grain_selector;
  p["loading_rate"] = s.rate;
  p["time_step_factor"] = s.dt_factor;
  p["nb_steps"] = static_cast<long long>(s.nb_steps);
  p["material_dat"] = s.dat;
  const auto dat = g_scratch / (s.name + "-np" + std::to_string(g_size) + ".dat");
  write_dat(s.dat, dat);
  Mesh mesh(s.dim);
  if (g_rank == 0) {
    if (s.dim == 2) {
      square_tri3(mesh, 8);
    } else {
      cube_tet4(mesh, 4);
    }
  }
  if (s.dim == 2) {
    const Real xc = s.name == "intrinsic_tri" ? -0.25 : 0.0;
    p["crack_x"] = xc;
    distribute_rule(mesh, w, [xc](const Vector<Real> & b) { return part_2d(b, xc); },
                    "2-D rule of the probe header, xc = " + std::to_string(xc));
  } else {
    distribute_rule(mesh, w, [](const Vector<Real> & b) { return part_3d(b); },
                    "3-D rule of the probe header");
  }
  TypeTable tt;
  SolidMechanicsModelCohesive model(mesh);
  if (s.grain_selector) {
    model.setMaterialSelector(
        std::make_shared<GrainSelector>(model, "TG_cohesive", "IG_cohesive"));
  }
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = s.extrinsic);
  // canonical ghost filters: bulk (element synchronizer) and the cohesive
  // elements of an intrinsic insertion (cohesive synchronizer)
  long long moved = 0;
  if (mesh.isDistributed()) {
    moved += canonicalize_ghost_filters(model, std::as_const(mesh).getElementSynchronizer(), {});
    moved += canonicalize_ghost_filters(model, model.getCohesiveSynchronizer(), {});
  }
  const Real stable = model.getStableTimeStep();
  const Real dt = stable * s.dt_factor;
  model.setTimeStep(dt);
  add_r(w, "model/time_step", {stable, dt}, 1, "getStableTimeStep, time_step_factor * it");
  // boundary conditions and initial velocity
  {
    auto & x = mesh.getNodes();
    auto & blocked = model.getBlockedDOFs();
    auto & v = model.getVelocity();
    const Real eps = std::numeric_limits<Real>::epsilon();
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      const Real X = loading_coordinate(s, x(n, 0));
      switch (s.loading) {
      case Loading::IntrinsicVelocity:
        if (std::abs(X - 1.) < eps) {
          blocked(n, 0) = true;
        }
        v(n, 0) = s.rate * X;
        break;
      case Loading::ExtrinsicPull:
        if (std::abs(X) > 0.99) {
          blocked(n, 0) = true;
        }
        if (s.dim == 2 and std::abs(x(n, 1)) > 0.99) {
          blocked(n, 1) = true;
        }
        v(n, 0) = s.rate * X;
        break;
      case Loading::Biaxial:
        if (std::abs(x(n, 1)) > 0.99) {
          blocked(n, 1) = true;
        }
        if (std::abs(x(n, 0)) > 0.99) {
          blocked(n, 0) = true;
        }
        v(n, 1) = s.rate * x(n, 1);
        v(n, 0) = s.rate * x(n, 0);
        break;
      }
    }
  }
  record_topology(w, "init/", model, mesh, tt, s.extrinsic ? Level::Conn : Level::Full);
  record_laws(w, "init/", model);
  record_facet_material(w, "init/facet_material/", model);
  // the facets checked at init (their stresses are recorded at the first
  // insertion)
  ElementTypeMapArray<bool> checked_at_init("checked_at_init");
  {
    const auto & cf = std::as_const(model.getElementInserter()).getCheckFacets();
    for (auto gt : ghost_types) {
      for (auto type : cf.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        const auto & src = cf(type, gt);
        auto & dst = checked_at_init.alloc(src.size(), 1, type, gt, false);
        for (Idx i = 0; i < src.size(); ++i) {
          dst(i) = src(i);
        }
      }
    }
  }
  std::vector<I64> inserted, nb_coh_local, nb_coh_ghost;
  std::vector<double> ek, ep, ed, er, ec, lek, lep, led, ler, lec;
  int first_insertion = -1;
  Real dispy = 0.;
  const Real VI = s.rate; // Biaxial: loading_rate * 2 * 0.5 (upstream)
  for (int step = 1; step <= s.nb_steps; ++step) {
    if (s.extrinsic) {
      auto & x = mesh.getNodes();
      auto & u = model.getDisplacement();
      auto & v = model.getVelocity();
      if (s.loading == Loading::ExtrinsicPull) {
        for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
          const Real X = loading_coordinate(s, x(n, 0));
          if (std::abs(X) > 0.99) {
            u(n, 0) += s.rate * dt * X;
          }
        }
      } else if (s.loading == Loading::Biaxial) {
        dispy += VI * dt;
        for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
          if (x(n, 1) > 0.99) {
            u(n, 1) = dispy;
            v(n, 1) = VI;
          }
          if (x(n, 1) < -0.99) {
            u(n, 1) = -dispy;
            v(n, 1) = -VI;
          }
          if (x(n, 0) > 0.99) {
            u(n, 0) = dispy;
            v(n, 0) = VI;
          }
          if (x(n, 0) < -0.99) {
            u(n, 0) = -dispy;
            v(n, 0) = -VI;
          }
        }
      }
      const auto before = ghost_filter_sizes(model);
      const auto n_ins = model.checkCohesiveStress();
      inserted.push_back(n_ins);
      if (n_ins > 0 and mesh.isDistributed()) {
        moved += canonicalize_ghost_filters(model, model.getCohesiveSynchronizer(), before);
      }
      if (n_ins > 0 and first_insertion < 0) {
        first_insertion = step;
        // the facet stresses that decided the first insertion (computed by
        // checkCohesiveStress before the insertion; the arrays grew with the
        // new facets)
        record_facet_stress(w, "ins1/facet_stress/", model, checked_at_init);
        record_topology(w, "ins1/", model, mesh, tt, Level::Light);
        record_laws(w, "ins1/", model);
        record_facet_material(w, "ins1/facet_material/", model);
      }
    }
    I64 nl = 0, ng = 0;
    for (auto type : mesh.elementTypes(s.dim, _not_ghost, _ek_cohesive)) {
      nl += mesh.getNbElement(type, _not_ghost);
    }
    for (auto type : mesh.elementTypes(s.dim, _ghost, _ek_cohesive)) {
      ng += mesh.getNbElement(type, _ghost);
    }
    nb_coh_local.push_back(nl);
    nb_coh_ghost.push_back(ng);
    model.solveStep();
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    ed.push_back(model.getEnergy("dissipated"));
    er.push_back(model.getEnergy("reversible"));
    ec.push_back(model.getEnergy("cohesive contact"));
    lek.push_back(local_kinetic(model, mesh));
    lep.push_back(local_law_energy(model, "potential"));
    led.push_back(local_law_energy(model, "dissipated"));
    ler.push_back(local_law_energy(model, "reversible"));
    lec.push_back(local_law_energy(model, "cohesive contact"));
    if (std::find(s.checkpoints.begin(), s.checkpoints.end(), step) != s.checkpoints.end()) {
      const auto prefix = "s" + std::to_string(step) + "/";
      // the topology changes only by insertion: the facet adjacency at the
      // last checkpoint of an extrinsic run (intrinsic: at init)
      const bool last = step == s.nb_steps;
      record_topology(w, prefix, model, mesh, tt,
                      s.extrinsic and last ? Level::Full : Level::Light);
      record_laws(w, prefix, model);
      record_fields(w, prefix, model);
    }
  }
  p["first_insertion_step"] = static_cast<long long>(first_insertion);
  // run-dependent by nature (upstream's MPI_Waitany unpack order): logged,
  // never recorded in the case
  std::cerr << "GHOST_FILTER_REORDERED " << s.name << " np" << g_size << " rank" << g_rank
            << " blocks " << moved << "\n";
  p["ghost_filter_order"] = std::string(
      "canonical rank order (probe patch): bulk ghost filters after initFull, cohesive ghost "
      "filters after each insertion (entries appended by it), receive schemes in rank order; "
      "the number of reordered blocks (run-dependent upstream) is logged as "
      "GHOST_FILTER_REORDERED");
  std::vector<long long> cps(s.checkpoints.begin(), s.checkpoints.end());
  p["checkpoints"] = ap::Json::array_of(cps);
  if (s.extrinsic) {
    add_isteps(w, "steps/inserted", inserted, "checkCohesiveStress() per step (global count)");
  }
  add_isteps(w, "steps/nb_cohesive_local", nb_coh_local,
             "not-ghost cohesive elements on this rank before the solve");
  add_isteps(w, "steps/nb_cohesive_ghost", nb_coh_ghost,
             "ghost cohesive elements on this rank before the solve");
  add_steps(w, "steps/energy/kinetic", ek, "getEnergy(kinetic) after solveStep (MPI_Allreduce)");
  add_steps(w, "steps/energy/potential", ep, "getEnergy(potential) (MPI_Allreduce)");
  add_steps(w, "steps/energy/dissipated", ed, "getEnergy(dissipated) (MPI_Allreduce)");
  add_steps(w, "steps/energy/reversible", er, "getEnergy(reversible) (MPI_Allreduce)");
  add_steps(w, "steps/energy/cohesive_contact", ec, "getEnergy(cohesive contact) (MPI_Allreduce)");
  add_steps(w, "steps/local_energy/kinetic", lek, "this rank's kinetic contribution");
  add_steps(w, "steps/local_energy/potential", lep, "sum of the laws' potential energy (local)");
  add_steps(w, "steps/local_energy/dissipated", led, "sum of the laws' dissipated energy (local)");
  add_steps(w, "steps/local_energy/reversible", ler, "sum of the laws' reversible energy (local)");
  add_steps(w, "steps/local_energy/cohesive_contact", lec,
            "sum of the laws' cohesive contact energy (local)");
  // _smmc_damage over the cohesive synchronizer (MPI-039): ghost damage
  // takes the owner's value
  if (mesh.isDistributed()) {
    model.getCohesiveSynchronizer().synchronizeOnce(model, SynchronizationTag::_smmc_damage);
    record_damage(w, "after_smmc_damage/", model);
  }
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

/// test_cohesive_facet_stress_synchronizer: linear bulk stress, facet
/// stresses of both sides after checkCohesiveStress.
Real stress_function(Real constant, Real x, Real y, Real z) {
  return constant + 2. * x + 3. * y + 4 * z;
}

void scenario_facet_stress_tet10() {
  const std::string name = "facet_stress_tet10";
  ap::FixtureWriter w(g_root, case_id(name),
                      "test_cohesive_facet_stress_synchronizer: linear bulk stress, facet stresses "
                      "of both sides after checkCohesiveStress (Tet10, rank view)",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  w.add_tag("cohesive");
  record_pin(w);
  const std::string dat_text =
      "material elastic [\n  name = steel\n  rho = 1e3\n  E = 1e3\n  nu = 0.001\n]\n"
      "material cohesive_linear [\n  name = cohesive\n  beta = 1\n  G_c = 100\n"
      "  sigma_c = 100\n]\n";
  w.parameters()["material_dat"] = dat_text;
  write_dat(dat_text, g_scratch / "facet_stress_tet10.dat");
  Math::setTolerance(1.e-11);
  Mesh mesh(3);
  const auto file = g_frozen /
                    "test/test_model/test_solid_mechanics_model/test_cohesive/"
                    "test_parallel_cohesive/test_cohesive_facet_stress_synchronizer/"
                    "tetrahedron.msh";
  Vector<Real> centre(3);
  if (g_rank == 0) {
    mesh.read(file.string());
    w.add_input_file("mesh", file,
                     "frozen:fixtures/meshes/gmsh-4.13.1/test/test_model/"
                     "test_solid_mechanics_model/test_cohesive/test_parallel_cohesive/"
                     "test_cohesive_facet_stress_synchronizer/tetrahedron.msh");
    for (Int i = 0; i < 3; ++i) {
      centre(i) = 0.5 * (mesh.getLowerBounds()(i) + mesh.getUpperBounds()(i));
    }
    w.parameters()["box_centre"] = ap::Json::array_of(
        std::vector<double>{centre(0), centre(1), centre(2)});
  }
  distribute_rule(mesh, w, [&centre](const Vector<Real> & b) { return part_box(b, centre); },
                  "box rule: x < cx -> 0 else 1; at 4 ranks + 2 for y >= cy (box centre of "
                  "the read mesh)");
  TypeTable tt;
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  long long moved = 0;
  if (mesh.isDistributed()) {
    moved += canonicalize_ghost_filters(model, std::as_const(mesh).getElementSynchronizer(), {});
  }
  std::cerr << "GHOST_FILTER_REORDERED " << name << " np" << g_size << " rank" << g_rank
            << " blocks " << moved << "\n";
  const ElementType type = _tetrahedron_10;
  auto & position = mesh.getNodes();
  const Int nq = model.getFEEngine().getNbIntegrationPoints(type);
  {
    const GhostType gt = _not_ghost; // as the upstream test: local stresses only
    const Idx ne = mesh.getNbElement(type, gt);
    Array<Real> quad(ne * nq, 3);
    model.getFEEngine().interpolateOnIntegrationPoints(position, quad, 3, type, gt);
    auto & stress = const_cast<Array<Real> &>(model.getMaterial(0).getStress(type, gt));
    // the law's stress array follows its filter; the filter of the single
    // bulk law holds every element in order on both ghost types
    const auto & filter = static_cast<const Material &>(model.getMaterial(0)).getElementFilter(type, gt);
    for (Idx k = 0; k < filter.size(); ++k) {
      const Idx e = filter(k);
      for (Int q = 0; q < nq; ++q) {
        for (Int i = 0; i < 3; ++i) {
          for (Int j = 0; j < 3; ++j) {
            const Int a = std::min(i, j), b = std::max(i, j);
            stress(k * nq + q, i * 3 + j) =
                stress_function(a * 3 + b, quad(e * nq + q, 0), quad(e * nq + q, 1),
                                quad(e * nq + q, 2));
          }
        }
      }
    }
  }
  const auto n_ins = model.checkCohesiveStress();
  w.parameters()["inserted"] = static_cast<long long>(n_ins);
  record_topology(w, "check/", model, mesh, tt, Level::Conn);
  // every facet (the upstream test checks the checked facets of both sides)
  ElementTypeMapArray<bool> all("all_facets");
  {
    const auto & mf = mesh.getMeshFacets();
    for (auto gt : ghost_types) {
      for (auto type : mf.elementTypes(2, gt, _ek_regular)) {
        all.alloc(mf.getNbElement(type, gt), 1, type, gt, true);
      }
    }
  }
  record_facet_stress(w, "check/facet_stress/", model, all);
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

/* -------------------------------------------------------------------------- */
/* Probe version 2 */
/* -------------------------------------------------------------------------- */

constexpr int kFacetsOnlyPre = 10;
constexpr int kFacetsOnlyPost = 20;

/// Crack of the facets-only scenario `name` (see the file header): every
/// node of the facet on the plane(s). Exact comparisons (grid coordinates).
bool facets_only_flagged(const std::string & name, const std::vector<Vector<Real>> & x) {
  auto all = [&](auto && pred) {
    return std::all_of(x.begin(), x.end(), [&](const Vector<Real> & p) { return pred(p); });
  };
  if (name == "facets_only_tri") {
    return all([](auto && p) { return p(0) == -0.5 and p(1) <= 0.; }) or
           all([](auto && p) { return p(1) == -0.5 and p(0) <= -0.5; });
  }
  if (name == "facets_only_tet") {
    return all([](auto && p) { return p(0) == 0.25 and p(1) <= 0.5; }) or
           all([](auto && p) { return p(2) == 0.5 and p(0) <= 0.25 and p(1) <= 0.5; });
  }
  // probe version 3:
  // facets_only_pg_tri: y = -0.5 for x <= 0 (the crack ends on the rank
  // boundary x = 0; only rank 0 doubles nodes);
  // facets_only_bnd_tri: the single facet x = 0, -0.5 <= y <= -0.25 on
  // the rank boundary (no node doubled: facet-stress scheme pruning)
  if (name == "facets_only_pg_tri") {
    return all([](auto && p) { return p(1) == -0.5 and p(0) <= 0.; });
  }
  if (name == "facets_only_bnd_tri") {
    return all([](auto && p) { return p(0) == 0. and p(1) >= -0.5 and p(1) <= -0.25; });
  }
  if (name == "facets_only_shared_tri") {
    return all([](auto && p) { return p(0) == 0.; }) or
           all([](auto && p) { return p(1) == 0. and p(0) <= 0.; });
  }
  throw std::runtime_error("unknown facets-only scenario " + name);
}

void run_facets_only(const std::string & name) {
  const Int dim = name == "facets_only_tet" ? 3 : 2;
  const std::string extrinsic_mat =
      "  material elastic [\n    name = steel\n    rho = 1e3\n    E = 1e3\n    nu = 0.001\n  ]\n"
      "  material cohesive_linear [\n    name = cohesive\n    beta = 1\n    G_c = 100\n"
      "    sigma_c = 100\n  ]\n";
  const std::string box = dim == 2 ? "[[-0.1, 0.1], [-10, 10]]" : "[[0.45, 0.55], [-10, 10], [-10, 10]]";
  const std::string dat_text =
      "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    bounding_box = " + box +
      "\n  ]\n" + extrinsic_mat + "]\n";
  ap::FixtureWriter w(g_root, case_id(name),
                      "insertElements(true) (facets only) on a distributed mesh after explicit "
                      "steps, then more steps (rank view)",
                      kProbe,
                      (name == "facets_only_pg_tri" or name == "facets_only_bnd_tri")
                          ? kProbeVersion3
                          : kProbeVersion);
  w.add_tag("parallel");
  w.add_tag("cohesive");
  record_pin(w);
  auto & p = w.parameters();
  p["rank"] = static_cast<long long>(g_rank);
  p["nb_ranks"] = static_cast<long long>(g_size);
  p["scenario"] = name;
  p["material_dat"] = dat_text;
  p["steps_before"] = static_cast<long long>(kFacetsOnlyPre);
  p["steps_after"] = static_cast<long long>(kFacetsOnlyPost);
  write_dat(dat_text, g_scratch / (name + "-np" + std::to_string(g_size) + ".dat"));
  Mesh mesh(dim);
  if (g_rank == 0) {
    if (dim == 2) {
      square_tri3(mesh, 8);
    } else {
      cube_tet4(mesh, 4);
    }
  }
  if (dim == 2) {
    distribute_rule(mesh, w, [](const Vector<Real> & b) { return part_2d(b, 0.); },
                    "2-D rule of the probe header, xc = 0");
  } else {
    distribute_rule(mesh, w, [](const Vector<Real> & b) { return part_3d(b); },
                    "3-D rule of the probe header");
  }
  TypeTable tt;
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  long long moved = 0;
  if (mesh.isDistributed()) {
    moved += canonicalize_ghost_filters(model, std::as_const(mesh).getElementSynchronizer(), {});
  }
  std::cerr << "GHOST_FILTER_REORDERED " << name << " np" << g_size << " rank" << g_rank
            << " blocks " << moved << "\n";
  const Real stable = model.getStableTimeStep();
  const Real dt = stable * 0.1;
  model.setTimeStep(dt);
  add_r(w, "model/time_step", {stable, dt}, 1, "getStableTimeStep, 0.1 * it");
  const Real rate = kPullRate;
  auto X_of = [&](Real x) { return dim == 2 ? x : 2. * x - 1.; };
  {
    auto & x = mesh.getNodes();
    auto & blocked = model.getBlockedDOFs();
    auto & v = model.getVelocity();
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      const Real X = X_of(x(n, 0));
      if (std::abs(X) > 0.99) {
        blocked(n, 0) = true;
      }
      if (dim == 2 and std::abs(x(n, 1)) > 0.99) {
        blocked(n, 1) = true;
      }
      v(n, 0) = rate * X;
    }
  }
  record_topology(w, "init/", model, mesh, tt, Level::Conn);
  record_laws(w, "init/", model);
  std::vector<double> ek, ep, lek, lep;
  const int nb_steps = kFacetsOnlyPre + kFacetsOnlyPost;
  for (int step = 1; step <= nb_steps; ++step) {
    {
      auto & x = mesh.getNodes();
      auto & u = model.getDisplacement();
      for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
        const Real X = X_of(x(n, 0));
        if (std::abs(X) > 0.99) {
          u(n, 0) += rate * dt * X;
        }
      }
    }
    model.solveStep();
    ek.push_back(model.getEnergy("kinetic"));
    ep.push_back(model.getEnergy("potential"));
    lek.push_back(local_kinetic(model, mesh));
    lep.push_back(local_law_energy(model, "potential"));
    if (step == kFacetsOnlyPre) {
      const auto & mf = mesh.getMeshFacets();
      auto & inserter = model.getElementInserter();
      auto & flags = inserter.getInsertionFacetsByElement();
      I64 flagged = 0;
      for (auto type : mf.elementTypes(dim - 1, _not_ghost, _ek_regular)) {
        const auto & conn = mf.getConnectivity(type, _not_ghost);
        auto & f = flags(type, _not_ghost);
        for (Idx e = 0; e < conn.size(); ++e) {
          std::vector<Vector<Real>> xs;
          for (Idx k = 0; k < conn.getNbComponent(); ++k) {
            Vector<Real> pt(dim);
            for (Int i = 0; i < dim; ++i) {
              pt(i) = mf.getNodes()(conn(e, k), i);
            }
            xs.push_back(pt);
          }
          if (facets_only_flagged(name, xs)) {
            f(e) = true;
            ++flagged;
          }
        }
      }
      add_i(w, "fo/flagged_local", {flagged}, 1, "not-ghost facets flagged on this rank");
      const auto ret = inserter.insertElements(true);
      add_i(w, "fo/returned", {static_cast<I64>(ret)}, 1, "insertElements(true) return value");
      record_topology(w, "fo/", model, mesh, tt, Level::Full);
      record_laws(w, "fo/", model);
      record_fields(w, "fo/", model);
    }
  }
  const auto prefix = "s" + std::to_string(nb_steps) + "/";
  record_topology(w, prefix, model, mesh, tt, Level::Light);
  record_laws(w, prefix, model);
  record_fields(w, prefix, model);
  std::vector<long long> cps{nb_steps};
  p["checkpoints"] = ap::Json::array_of(cps);
  add_steps(w, "steps/energy/kinetic", ek, "getEnergy(kinetic) after solveStep (MPI_Allreduce)");
  add_steps(w, "steps/energy/potential", ep, "getEnergy(potential) (MPI_Allreduce)");
  add_steps(w, "steps/local_energy/kinetic", lek, "this rank's kinetic contribution");
  add_steps(w, "steps/local_energy/potential", lep, "sum of the laws' potential energy (local)");
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

/// clusters_tri filter: the cells of row j = 3 and, above it, of the
/// columns i = 2 and 5 of the n = 8 square are removed.
bool clusters_filter(const Vector<Real> & b) {
  const Int i = Int(std::floor((b(0) + 1.0) / 0.25));
  const Int j = Int(std::floor((b(1) + 1.0) / 0.25));
  return not(j == 3 or (j >= 4 and (i == 2 or i == 5)));
}

class BarycentreFilter : public GroupManager::ClusteringFilter {
public:
  explicit BarycentreFilter(const Mesh & mesh) : mesh(mesh) {}
  bool operator()(const Element & el) const override {
    Vector<Real> b(mesh.getSpatialDimension());
    mesh.getBarycenter(el, b);
    return clusters_filter(b);
  }

private:
  const Mesh & mesh;
};

/// Every element group of `gm` whose name starts with `prefix`: its
/// elements per (type, ghost) in group order, its nodes, and the names.
void record_groups(ap::FixtureWriter & w, const std::string & key, GroupManager & gm,
                   const std::string & prefix) {
  std::vector<std::string> names;
  for (auto & group : gm.iterateElementGroups()) {
    const auto & name = group.getName();
    if (name.rfind(prefix, 0) != 0) {
      continue;
    }
    names.push_back(name);
    std::vector<std::string> blocks;
    for (auto gt : ghost_types) {
      for (auto type : group.elementTypes(_ghost_type = gt, _element_kind = _ek_not_defined)) {
        auto bk = type_name(type) + "/" + ghost_name(gt);
        blocks.push_back(bk);
        add_i(w, key + name + "/elements/" + bk, ints(group.getElements(type, gt)), 1,
              "group elements (group order)");
      }
    }
    w.parameters()[key + name + "/blocks"] = ap::Json::array_of(blocks);
    w.parameters()[key + name + "/dimension"] = static_cast<long long>(group.getDimension());
    add_i(w, key + name + "/nodes", ints(group.getNodeGroup().getNodes()), 1,
          "node group (local ids, group order)");
  }
  w.parameters()[key + "names"] = ap::Json::array_of(names);
}

void scenario_clusters_tri() {
  const std::string name = "clusters_tri";
  ap::FixtureWriter w(g_root, case_id(name),
                      "Mesh::createClusters on the distributed Tri3 square (rank view)", kProbe,
                      kProbeVersion);
  w.add_tag("parallel");
  w.add_tag("mesh");
  record_pin(w);
  w.parameters()["rank"] = static_cast<long long>(g_rank);
  w.parameters()["nb_ranks"] = static_cast<long long>(g_size);
  Mesh mesh(2);
  if (g_rank == 0) {
    square_tri3(mesh, 8);
  }
  distribute_rule(mesh, w, [](const Vector<Real> & b) { return part_2d(b, 0.); },
                  "2-D rule of the probe header, xc = 0");
  const auto n = mesh.createClusters(2, "cluster", BarycentreFilter(mesh));
  add_i(w, "clusters/cluster/returned", {static_cast<I64>(n)}, 1,
        "createClusters(2, cluster, filter)");
  record_groups(w, "clusters/cluster/", mesh, "cluster_");
  const auto n_all = mesh.createClusters(2, "all");
  add_i(w, "clusters/all/returned", {static_cast<I64>(n_all)}, 1, "createClusters(2, all)");
  record_groups(w, "clusters/all/", mesh, "all_");
  std::vector<std::string> egs, ngs;
  for (auto & g : mesh.iterateElementGroups()) {
    egs.push_back(g.getName());
  }
  for (auto & g : mesh.iterateNodeGroups()) {
    ngs.push_back(g.getName());
  }
  w.parameters()["groups/element_groups"] = ap::Json::array_of(egs);
  w.parameters()["groups/node_groups"] = ap::Json::array_of(ngs);
  w.finish();
}

/// clusters_chk_tri (probe version 3): the cells (i, j) of the
/// n = 8 square with i + j even are kept (a checkerboard: every kept cell is
/// a cluster of two triangles, many of them held as ghosts by other ranks);
/// the last rank creates an element group of its own first (the distributed
/// createClusters ends with synchronizeGroupNames).
bool chk_filter(const Vector<Real> & b) {
  const Int i = Int(std::floor((b(0) + 1.0) / 0.25));
  const Int j = Int(std::floor((b(1) + 1.0) / 0.25));
  return (i + j) % 2 == 0;
}

class ChkFilter : public GroupManager::ClusteringFilter {
public:
  explicit ChkFilter(const Mesh & mesh) : mesh(mesh) {}
  bool operator()(const Element & el) const override {
    Vector<Real> b(mesh.getSpatialDimension());
    mesh.getBarycenter(el, b);
    return chk_filter(b);
  }

private:
  const Mesh & mesh;
};

void scenario_clusters_chk_tri() {
  const std::string name = "clusters_chk_tri";
  ap::FixtureWriter w(g_root, case_id(name),
                      "Mesh::createClusters with a checkerboard filter on the distributed Tri3 "
                      "square, a group of the last rank only (rank view)",
                      kProbe, kProbeVersion3);
  w.add_tag("parallel");
  w.add_tag("mesh");
  record_pin(w);
  w.parameters()["rank"] = static_cast<long long>(g_rank);
  w.parameters()["nb_ranks"] = static_cast<long long>(g_size);
  Mesh mesh(2);
  if (g_rank == 0) {
    square_tri3(mesh, 8);
  }
  distribute_rule(mesh, w, [](const Vector<Real> & b) { return part_2d(b, 0.); },
                  "2-D rule of the probe header, xc = 0");
  if (g_rank == g_size - 1) {
    mesh.createElementGroup("last_rank_only", 2);
  }
  const auto n = mesh.createClusters(2, "chk", ChkFilter(mesh));
  add_i(w, "clusters/chk/returned", {static_cast<I64>(n)}, 1, "createClusters(2, chk, filter)");
  record_groups(w, "clusters/chk/", mesh, "chk_");
  std::vector<std::string> egs, ngs;
  for (auto & g : mesh.iterateElementGroups()) {
    egs.push_back(g.getName());
  }
  for (auto & g : mesh.iterateNodeGroups()) {
    ngs.push_back(g.getName());
  }
  w.parameters()["groups/element_groups"] = ap::Json::array_of(egs);
  w.parameters()["groups/node_groups"] = ap::Json::array_of(ngs);
  w.finish();
}

/// test_cohesive_parallel_buildfragments' displaceElements: the nodes of
/// every element (both ghost types) whose barycentre x is below (above)
/// the limit move by `amount` in x, each node once.
void displace_elements(SolidMechanicsModelCohesive & model, Real limit, bool below, Real amount) {
  auto & mesh = model.getMesh();
  const Int dim = mesh.getSpatialDimension();
  auto & u = model.getDisplacement();
  std::vector<bool> displaced(mesh.getNbNodes(), false);
  Vector<Real> b(dim);
  for (auto gt : ghost_types) {
    for (auto type : mesh.elementTypes(dim, gt, _ek_regular)) {
      const auto & conn = mesh.getConnectivity(type, gt);
      for (Idx e = 0; e < conn.size(); ++e) {
        mesh.getBarycenter(Element{type, e, gt}, b);
        if (not(below ? b(0) < limit : b(0) > limit)) {
          continue;
        }
        for (Idx k = 0; k < conn.getNbComponent(); ++k) {
          auto node = conn(e, k);
          if (not displaced[node]) {
            u(node, 0) += amount;
            displaced[node] = true;
          }
        }
      }
    }
  }
}

/// upstream FragmentManager::integrateFieldOnFragments before its
/// allReduce (same loops and operations): this rank's contribution.
Array<Real> local_fragment_integral(SolidMechanicsModelCohesive & model, FragmentManager & fm,
                                    const ElementTypeMapArray<Real> & field, Int nbc,
                                    const ElementTypeMapArray<Real> & rho) {
  const Int dim = model.getSpatialDimension();
  const auto nb_fragment = fm.getNbFragment();
  Array<Real> output(nb_fragment, nbc);
  output.zero();
  auto output_begin = make_view(output, nbc).begin();
  auto & fe = model.getFEEngine();
  for (auto & group : fm.iterateElementGroups()) {
    const auto & el_list = group.getElements();
    const auto fragment_index = std::stoul(group.getName().substr(std::string("fragment").size() + 1));
    for (auto type : el_list.elementTypes(dim, _not_ghost, _ek_regular)) {
      const auto nq = fe.getNbIntegrationPoints(type);
      const auto & density_array = rho(type);
      const auto & field_array = field(type);
      const auto & elements = el_list(type);
      Array<Real> integration_array(elements.size() * nq, nbc);
      auto field_array_begin = make_view(field_array, nq, nbc).begin();
      auto density_array_begin = make_view(density_array, nq).begin();
      for (auto && data : enumerate(make_view(integration_array, nq, nbc))) {
        auto global_el = elements(std::get<0>(data));
        auto & int_array = std::get<1>(data);
        int_array = field_array_begin[global_el];
        const auto & density_vector = density_array_begin[global_el];
        for (Int i = 0; i < nq; ++i) {
          for (Int j = 0; j < nbc; ++j) {
            int_array(i, j) *= density_vector(i);
          }
        }
      }
      Array<Real> integrated_array(elements.size(), nbc);
      fe.integrate(integration_array, integrated_array, nbc, type, _not_ghost, elements);
      Vector<Real> zeros = Vector<Real>::Zero(nbc);
      output_begin[fragment_index] = zeros;
      for (auto && data : make_view(integrated_array, nbc)) {
        output_begin[fragment_index] += data;
      }
    }
  }
  return output;
}

void require_same(const std::string & what, const Array<Real> & a, const Array<Real> & b) {
  if (a.size() * a.getNbComponent() != b.size() * b.getNbComponent()) {
    throw std::runtime_error(what + ": sizes differ");
  }
  for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
    if (std::memcmp(a.data() + i, b.data() + i, sizeof(Real)) != 0) {
      throw std::runtime_error(what + ": the replicated contributions do not reduce to "
                                      "upstream's value (entry " + std::to_string(i) + ")");
    }
  }
}

void record_fragments(ap::FixtureWriter & w, const std::string & prefix,
                      SolidMechanicsModelCohesive & model, FragmentManager & fm) {
  const Int dim = model.getSpatialDimension();
  auto & mesh = model.getMesh();
  const auto & comm = Communicator::getStaticCommunicator();
  auto & fe = model.getFEEngine();
  add_i(w, prefix + "nb_fragment", {static_cast<I64>(fm.getNbFragment())}, 1,
        "getNbFragment (global)");
  add_r(w, prefix + "mass", reals(fm.getMass()), dim, "getMass (all-reduced)");
  add_r(w, prefix + "center_of_mass", reals(fm.getCenterOfMass()), dim, "getCenterOfMass");
  add_r(w, prefix + "velocity", reals(fm.getVelocity()), dim, "getVelocity");
  add_r(w, prefix + "moments_of_inertia", reals(fm.getMomentsOfInertia()), dim,
        "getMomentsOfInertia (eig of the all-reduced integrals)");
  add_r(w, prefix + "principal_directions", reals(fm.getPrincipalDirections()), dim * dim,
        "getPrincipalDirections");
  add_i(w, prefix + "nb_elements_per_fragment", ints(fm.getNbElementsPerFragment()), 1,
        "getNbElementsPerFragment (all-reduced)");
  record_groups(w, prefix + "groups/", fm, "fragment_");
  record_damage(w, prefix + "damage/", model);
  // this rank's contributions (replicated integrateFieldOnFragments)
  ElementTypeMapArray<Real> rho("rho"), unit("unit"), quad("quad"), vel("vel"), moments("moments");
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_regular)) {
    const auto ne = mesh.getNbElement(type);
    const auto nq = fe.getNbIntegrationPoints(type);
    auto & r = rho.alloc(ne * nq, 1, type, _not_ghost);
    const auto & mat_indexes = model.getMaterialByElement(type);
    for (Idx e = 0; e < ne; ++e) {
      for (Idx q = 0; q < nq; ++q) {
        r(e * nq + q) = model.getMaterial(mat_indexes(e)).getRho();
      }
    }
    unit.alloc(ne * nq, dim, type, _not_ghost, 1.);
    auto & qc = quad.alloc(ne * nq, dim, type, _not_ghost);
    fe.interpolateOnIntegrationPoints(mesh.getNodes(), qc, dim, type, _not_ghost);
    auto & vq = vel.alloc(ne * nq, dim, type, _not_ghost);
    fe.interpolateOnIntegrationPoints(model.getVelocity(), vq, dim, type, _not_ghost);
    moments.alloc(ne * nq, dim * dim, type, _not_ghost, 1.);
  }
  auto l_mass = local_fragment_integral(model, fm, unit, dim, rho);
  auto l_x = local_fragment_integral(model, fm, quad, dim, rho);
  auto l_v = local_fragment_integral(model, fm, vel, dim, rho);
  // moments about the (global) centres, upstream's group/centre zip
  {
    const auto & mass_centers = fm.getCenterOfMass();
    for (auto && data : zip(fm.iterateElementGroups(), make_view(mass_centers, dim))) {
      const auto & el_list = std::get<0>(data).getElements();
      auto & mass_center = std::get<1>(data);
      for (auto type : el_list.elementTypes(dim, _not_ghost, _ek_regular)) {
        auto nq = fe.getNbIntegrationPoints(type);
        auto & moments_coords_array = moments(type);
        const auto & quad_coordinates_array = quad(type);
        const auto & el_list_array = el_list(type);
        auto moments_begin = moments_coords_array.begin(dim, dim);
        auto quad_coordinates_begin = quad_coordinates_array.begin(dim);
        Vector<Real> relative_coords(dim);
        for (Int el = 0; el < el_list_array.size(); ++el) {
          auto global_el = el_list_array(el);
          for (Int q = 0; q < nq; ++q) {
            auto global_q = global_el * nq + q;
            auto && moments_matrix = moments_begin[global_q];
            const auto & quad_coord_vector = quad_coordinates_begin[global_q];
            relative_coords = quad_coord_vector;
            relative_coords -= mass_center;
            moments_matrix = relative_coords * relative_coords.transpose();
            Real trace = moments_matrix.trace();
            moments_matrix =
                -1. * moments_matrix + trace * Matrix<Real>::Identity(dim, dim);
          }
        }
      }
    }
  }
  auto l_moments = local_fragment_integral(model, fm, moments, dim * dim, rho);
  // self-check: upstream's MPI_Allreduce of the replicated contributions
  // gives upstream's arrays bitwise
  {
    Array<Real> m = l_mass;
    comm.allReduce(m, SynchronizerOperation::_sum);
    require_same(prefix + "mass", m, fm.getMass());
    for (auto & [what, local, ref] :
         std::vector<std::tuple<std::string, Array<Real> *, const Array<Real> *>>{
             {"center_of_mass", &l_x, &fm.getCenterOfMass()},
             {"velocity", &l_v, &fm.getVelocity()}}) {
      Array<Real> c = *local;
      comm.allReduce(c, SynchronizerOperation::_sum);
      // upstream computeCenterOfMass / computeVelocity: entry-wise division
      for (auto && pair : zip(make_view(m), make_view(c))) {
        std::get<1>(pair) /= std::get<0>(pair);
      }
      require_same(prefix + what, c, *ref);
    }
    Array<Real> im = l_moments;
    comm.allReduce(im, SynchronizerOperation::_sum);
    Array<Real> values(fm.getNbFragment(), dim), dirs(fm.getNbFragment(), dim * dim);
    auto im_it = im.begin(dim, dim);
    auto v_it = values.begin(dim);
    auto d_it = dirs.begin(dim, dim);
    for (UInt f = 0; f < fm.getNbFragment(); ++f, ++im_it, ++v_it, ++d_it) {
      im_it->eig(*v_it, *d_it);
    }
    require_same(prefix + "moments_of_inertia", values, fm.getMomentsOfInertia());
    require_same(prefix + "principal_directions", dirs, fm.getPrincipalDirections());
  }
  add_r(w, prefix + "local/mass", reals(l_mass), dim, "this rank's integral of rho (before the allReduce)");
  add_r(w, prefix + "local/mass_center", reals(l_x), dim,
        "this rank's integral of rho x (before the allReduce and the division)");
  add_r(w, prefix + "local/velocity", reals(l_v), dim,
        "this rank's integral of rho v (before the allReduce and the division)");
  add_r(w, prefix + "local/inertia", reals(l_moments), dim * dim,
        "this rank's integral of rho (tr(r r^T) I - r r^T) (before the allReduce)");
  std::vector<I64> l_nb(fm.getNbFragment(), 0);
  for (auto & group : fm.iterateElementGroups()) {
    const auto idx = std::stoul(group.getName().substr(std::string("fragment").size() + 1));
    for (auto type : group.getElements().elementTypes(dim, _not_ghost, _ek_regular)) {
      l_nb[idx] += group.getElements(type, _not_ghost).size();
    }
  }
  add_i(w, prefix + "local/nb_elements", l_nb, 1, "this rank's not-ghost bulk elements per fragment");
}

void scenario_fragments_tri() {
  const std::string name = "fragments_tri";
  const std::string dat_text =
      "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    bounding_box = "
      "[[-10, 10], [-10, 10]]\n  ]\n"
      "  material elastic [\n    name = bulk\n    rho = 1e3\n    E = 1e3\n    nu = 0.1\n  ]\n"
      "  material cohesive_linear [\n    name = cohesive\n    beta = 1\n    G_c = 1e-3\n"
      "    sigma_c = 1\n  ]\n]\n";
  ap::FixtureWriter w(g_root, case_id(name),
                      "FragmentManager::computeAllData on a distributed mesh broken line by line "
                      "(rank view)",
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  w.add_tag("cohesive");
  record_pin(w);
  auto & p = w.parameters();
  p["rank"] = static_cast<long long>(g_rank);
  p["nb_ranks"] = static_cast<long long>(g_size);
  p["material_dat"] = dat_text;
  write_dat(dat_text, g_scratch / (name + "-np" + std::to_string(g_size) + ".dat"));
  Mesh mesh(2);
  if (g_rank == 0) {
    square_tri3(mesh, 8);
  }
  distribute_rule(mesh, w, [](const Vector<Real> & b) { return part_2d(b, 0.); },
                  "2-D rule of the probe header, xc = 0");
  TypeTable tt;
  SolidMechanicsModelCohesive model(mesh);
  model.initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = true);
  long long moved = 0;
  if (mesh.isDistributed()) {
    moved += canonicalize_ghost_filters(model, std::as_const(mesh).getElementSynchronizer(), {});
  }
  const Int dim = 2;
  // check facets: the lines x = -0.5, 0, 0.5 only
  {
    const auto & mf = mesh.getMeshFacets();
    auto & inserter = model.getElementInserter();
    for (auto gt : ghost_types) {
      for (auto type : mf.elementTypes(dim - 1, gt, _ek_regular)) {
        auto & cf = inserter.getCheckFacets(type, gt);
        const auto & conn = mf.getConnectivity(type, gt);
        for (Idx f = 0; f < cf.size(); ++f) {
          bool on_line = false;
          for (Real line : {-0.5, 0., 0.5}) {
            bool all = true;
            for (Idx k = 0; k < conn.getNbComponent(); ++k) {
              all = all and mf.getNodes()(conn(f, k), 0) == line;
            }
            on_line = on_line or all;
          }
          if (not on_line) {
            cf(f) = false;
          }
        }
      }
    }
  }
  {
    auto & x = mesh.getNodes();
    auto & u = model.getDisplacement();
    auto & v = model.getVelocity();
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      u(n, 0) = x(n, 0) * 0.1;
      v(n, 0) = x(n, 0);
      v(n, 1) = 0.5 * x(n, 1);
    }
  }
  model.assembleInternalForces();
  const auto before = ghost_filter_sizes(model);
  const auto n_ins = model.checkCohesiveStress();
  add_i(w, "ins/inserted", {static_cast<I64>(n_ins)}, 1, "checkCohesiveStress() (global count)");
  if (n_ins > 0 and mesh.isDistributed()) {
    moved += canonicalize_ghost_filters(model, model.getCohesiveSynchronizer(), before);
  }
  std::cerr << "GHOST_FILTER_REORDERED " << name << " np" << g_size << " rank" << g_rank
            << " blocks " << moved << "\n";
  record_topology(w, "ins/", model, mesh, tt, Level::Light);
  record_laws(w, "ins/", model);
  FragmentManager fm(model, false);
  const std::vector<std::tuple<Real, bool, Real>> stages = {
      {-0.5, true, -0.01}, {0.5, false, 0.01}, {0., true, -0.01}};
  for (std::size_t stage = 1; stage <= stages.size() + 1; ++stage) {
    if (stage > 1) {
      auto [limit, below, amount] = stages[stage - 2];
      displace_elements(model, limit, below, amount);
      model.assembleInternalForces();
    }
    fm.computeAllData();
    record_fragments(w, "frag/s" + std::to_string(stage) + "/", model, fm);
  }
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
}

std::vector<Spec> specs() {
  const std::string intrinsic_mat =
      "  material elastic [\n    name = bulk\n    rho = 1e3\n    E = 1e3\n    nu = 0.1\n  ]\n"
      "  material cohesive_bilinear [\n    name = cohesive\n    sigma_c = 1\n    beta = 1.5\n"
      "    G_c = 1\n    delta_0 = 0.1\n  ]\n";
  const std::string extrinsic_mat =
      "  material elastic [\n    name = steel\n    rho = 1e3\n    E = 1e3\n    nu = 0.001\n  ]\n"
      "  material cohesive_linear [\n    name = cohesive\n    beta = 1\n    G_c = 100\n"
      "    sigma_c = 100\n  ]\n";
  const std::string igtg_mat =
      "  material elastic [\n    name = steel\n    rho = 1e3\n    E = 1e3\n    nu = 0.0\n  ]\n"
      "  material cohesive_linear [\n    name = TG_cohesive\n    beta = 0\n    G_c = 10\n"
      "    sigma_c = 100\n  ]\n"
      "  material cohesive_linear [\n    name = IG_cohesive\n    beta = 0\n    G_c = 10\n"
      "    sigma_c = 20\n  ]\n";
  auto model_section = [](const std::string & box, const std::string & mats) {
    return "model solid_mechanics_model_cohesive [\n  cohesive_inserter [\n    bounding_box = " +
           box + "\n  ]\n" + mats + "]\n";
  };
  std::vector<Spec> out;
  out.push_back({"intrinsic_tri",
                 "intrinsic cohesive_bilinear on x = -0.25 (Tri3 n = 8), explicit, v_x = 0.2 x, "
                 "rank view",
                 2, false, false,
                 model_section("[[-0.26, -0.24], [-10, 10]]", intrinsic_mat),
                 Loading::IntrinsicVelocity, 0.2, 0.8, kIntrinsicTriSteps,
                 {kIntrinsicTriSteps / 2, kIntrinsicTriSteps}});
  out.push_back({"intrinsic_tet",
                 "intrinsic cohesive_bilinear on x = 0.5 (Kuhn Tet4 n = 4), explicit, "
                 "v_x = 0.2 (2x - 1), rank view",
                 3, false, false,
                 model_section("[[0.49, 0.51], [-10, 10], [-10, 10]]", intrinsic_mat),
                 Loading::IntrinsicVelocity, 0.2, 0.8, kIntrinsicTetSteps,
                 {kIntrinsicTetSteps / 2, kIntrinsicTetSteps}});
  out.push_back({"extrinsic_tri",
                 "extrinsic cohesive_linear, check facets on x = 0 (Tri3 n = 8), pulled in x, "
                 "rank view",
                 2, true, false, model_section("[[-0.1, 0.1], [-10, 10]]", extrinsic_mat),
                 Loading::ExtrinsicPull, kPullRate, 0.1, kExtrinsicTriSteps,
                 {kExtrinsicTriSteps}});
  out.push_back({"extrinsic_tet",
                 "extrinsic cohesive_linear, check facets on x = 0.5 (Kuhn Tet4 n = 4), pulled "
                 "in x, rank view",
                 3, true, false,
                 model_section("[[0.45, 0.55], [-10, 10], [-10, 10]]", extrinsic_mat),
                 Loading::ExtrinsicPull, kPullRate, 0.1, kExtrinsicTetSteps,
                 {kExtrinsicTetSteps}});
  out.push_back({"extrinsic_igtg_tri",
                 "extrinsic, two cohesive laws by grain (IG on x = 0, TG elsewhere), check "
                 "facets in |x| < 0.2 (Tri3 n = 8), biaxial pull, rank view",
                 2, true, true, model_section("[[-0.2, 0.2], [-10, 10]]", igtg_mat),
                 Loading::Biaxial, kBiaxialRate, 0.1, kIgtgSteps,
                 {kIgtgSteps}});
  return out;
}

} // namespace

int main(int argc, char * argv[]) {
  Eigen::setCpuCacheSizes(kEigenL1, kEigenL2, kEigenL3);
  if (argc < 5) {
    std::cerr << "usage: parallel_cohesive_probe <fixtures_root> <frozen_meshes_root> <scratch> "
                 "<scenario>\n";
    return 2;
  }
  g_root = argv[1];
  g_frozen = argv[2];
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
    bool found = false;
    if (scenario == "facet_stress_tet10") {
      scenario_facet_stress_tet10();
      found = true;
    }
    if (scenario == "facets_only_tri" or scenario == "facets_only_tet" or
        scenario == "facets_only_shared_tri" or scenario == "facets_only_pg_tri" or
        scenario == "facets_only_bnd_tri") {
      run_facets_only(scenario);
      found = true;
    }
    if (scenario == "clusters_chk_tri") {
      scenario_clusters_chk_tri();
      found = true;
    }
    if (scenario == "clusters_tri") {
      scenario_clusters_tri();
      found = true;
    }
    if (scenario == "fragments_tri") {
      scenario_fragments_tri();
      found = true;
    }
    for (const auto & s : specs()) {
      if (s.name == scenario) {
        run_case(s);
        found = true;
      }
    }
    if (not found) {
      std::cerr << "unknown scenario " << scenario << "\n";
      return 2;
    }
  } catch (std::exception & e) {
    std::cerr << "parallel_cohesive_probe (rank " << g_rank << ", " << scenario
              << "): " << e.what() << "\n";
    return 1;
  }
  finalize();
  return 0;
}
