// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): upstream's TestSMMCFixture
// (test/test_model/test_solid_mechanics_model/test_cohesive/
// test_cohesive_fixture.hh, test_cohesive.cc; the gtest suite is registered
// with PARALLEL_LEVEL 1 2) run on 2 ranks for the 2-D and 3-D cases, with the
// partition the port's test uses (`Fixture::create_on` in
// crates/akantu-cohesive/tests/common/mod.rs: the port's RcbPartitioner,
// crates/akantu-mesh/src/partition.rs `rcb`) computed here and given to
// upstream through MeshPartitionMeshData. Derived from the review's
// ud1141_probe.cc, which showed the 2-rank runs of three of these cases bitwise equal to the
// port; this probe records every 2-D / 3-D case with the D-035 fixture writer.
//
// Run under `mpirun -np 2` once per case; every rank writes its own case
//   parallel_test_cohesive/<case>/np2/rank<r>
// <case> = <test>_<types>, test = extrinsic_mode_i, extrinsic_mode_i_finite_def,
// extrinsic_mode_ii, intrinsic_mode_i, intrinsic_mode_ii; types = triangle_3,
// quadrangle_4, triangle_3_quadrangle_4, triangle_6, quadrangle_8,
// triangle_6_quadrangle_8, tetrahedron_4, tetrahedron_10 (upstream's coh_types
// of dimension 2 and 3; the 1-D cases are not run at 2 ranks: upstream's test
// returns at once and upstream refuses the input, UD-1141).
//
// Recorded per rank: the partition (rank 0: one part per element of the mesh
// dimension, in type order), the global surface and group size of the
// fixture, the time steps, per step the return value of checkCohesiveStress
// (extrinsic), the number of not-ghost cohesive elements after the check and
// the five energies after solveStep (getEnergy: global values), and after the
// run the node global ids, the final displacement and velocity (local node
// order), every connectivity of the mesh (not ghost and ghost, in node global
// ids) and upstream's two assertions (checkInsertion with the all-reduced
// count, checkDissipated within 0.5).
//
// Version 2: with PTC_RECORD=boundary the
// probe writes, under the new root
//   parallel_test_cohesive_boundary/<case>/np2/rank<r>
// (the version-1 fixtures above are neither rewritten nor replaced), what
// version 1 does not record: after the run the connectivities of the
// boundary blocks (the regular element types of dimension `dim - 1`, the
// `loading`, `fixed` and `sides` facets of the mesh, which the cohesive
// insertion rewrites when it doubles their nodes; not ghost and ghost, in
// node global ids) and, per element group, its node list (node global ids,
// in the group's order) and its element count per block, with the node
// global ids. Without PTC_RECORD the probe writes the version-1 record
// unchanged.
//
// Usage: parallel_test_cohesive_probe <fixtures_root> <upstream_src_root>
//                                     <frozen_meshes_root> <case>
// The Eigen cache sizes are pinned at the start of main (D-050) and recorded
// with OPENBLAS_CORETYPE / OPENBLAS_NUM_THREADS.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_iterators.hh>
#include <communicator.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_partition_mesh_data.hh>
#include <solid_mechanics_model_cohesive.hh>

#include <algorithm>
#include <cmath>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <limits>
#include <memory>
#include <sstream>
#include <stdexcept>
#include <string>
#include <utility>
#include <vector>

#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "parallel_test_cohesive_probe";
// version 2: the boundary record (PTC_RECORD=boundary)
constexpr const char * kProbeVersion = "2";

/// Version 2: the boundary record instead of the version-1 record.
bool boundary_record() {
  const char * r = std::getenv("PTC_RECORD");
  return r != nullptr and std::string(r) == "boundary";
}
constexpr int kEigenL1 = 16384, kEigenL2 = 524288, kEigenL3 = 524288;
using I64 = std::int64_t;

Int g_rank = 0, g_size = 1;

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

struct TypeCase {
  std::string types;
  std::string mesh;
  Int dim;
  ElementType cohesive_type;
};

const std::vector<TypeCase> & type_cases() {
  static const std::vector<TypeCase> v = {
      {"triangle_3", "_cohesive_2d_4_triangle_3.msh", 2, _cohesive_2d_4},
      {"quadrangle_4", "_cohesive_2d_4_quadrangle_4.msh", 2, _cohesive_2d_4},
      {"triangle_3_quadrangle_4", "_cohesive_2d_4_triangle_3_quadrangle_4.msh", 2,
       _cohesive_2d_4},
      {"triangle_6", "_cohesive_2d_6_triangle_6.msh", 2, _cohesive_2d_6},
      {"quadrangle_8", "_cohesive_2d_6_quadrangle_8.msh", 2, _cohesive_2d_6},
      {"triangle_6_quadrangle_8", "_cohesive_2d_6_triangle_6_quadrangle_8.msh", 2,
       _cohesive_2d_6},
      {"tetrahedron_4", "_cohesive_3d_6_tetrahedron_4.msh", 3, _cohesive_3d_6},
      {"tetrahedron_10", "_cohesive_3d_12_tetrahedron_10.msh", 3, _cohesive_3d_12}};
  return v;
}

struct TestCase {
  std::string test; // case prefix
  std::string upstream; // TestSMMCFixture test name
  bool extrinsic;
  bool mode_ii;
  std::string material;
};

const std::vector<TestCase> & test_cases() {
  static const std::vector<TestCase> v = {
      {"extrinsic_mode_i", "ExtrinsicModeI", true, false, "material_0.dat"},
      {"extrinsic_mode_i_finite_def", "ExtrinsicModeIFiniteDef", true, false,
       "material_0_finite_def.dat"},
      {"extrinsic_mode_ii", "ExtrinsicModeII", true, true, "material_0.dat"},
      {"intrinsic_mode_i", "IntrinsicModeI", false, false, "material_1.dat"},
      {"intrinsic_mode_ii", "IntrinsicModeII", false, true, "material_1.dat"}};
  return v;
}

std::string file_sha256(const fs::path & p) {
  std::ifstream in(p, std::ios::binary);
  if (not in) {
    throw std::runtime_error("cannot read " + p.string());
  }
  std::stringstream ss;
  ss << in.rdbuf();
  auto s = ss.str();
  ap::Sha256 h;
  h.update(s.data(), s.size());
  return h.hex();
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

void add_i(ap::FixtureWriter & w, const std::string & name, const std::vector<I64> & v,
           std::size_t nbc, const std::string & desc) {
  if (nbc <= 1) {
    w.add(name, {v.size()}, {"entry"}, "table", v).description(desc).tolerance_class("exact");
  } else {
    w.add(name, {v.size() / nbc, nbc}, {"entry", "component"}, "table", v)
        .description(desc)
        .tolerance_class("exact");
  }
}

void add_r(ap::FixtureWriter & w, const std::string & name, const std::vector<double> & v,
           std::size_t nbc, const std::string & desc) {
  w.add(name, {nbc == 0 ? 0 : v.size() / nbc, nbc}, {"entry", "component"}, "table", v)
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

/// The port's RcbPartitioner (crates/akantu-mesh/src/partition.rs `rcb`):
/// barycentres (node coordinates summed in local node order, divided by the
/// node count) of every not-ghost element of the mesh dimension in type order,
/// longest extent (first strict maximum), sort by (coordinate, index), cut at
/// len * (nb / 2) / nb, recurse.
void rcb(std::vector<std::pair<std::vector<Real>, Idx>> & items, std::size_t lo, std::size_t hi,
         Idx first, Idx nb, std::vector<Idx> & out) {
  if (nb <= 1 or hi - lo <= 1) {
    for (auto i = lo; i < hi; ++i) {
      out[items[i].second] = first;
    }
    return;
  }
  auto d = items[lo].first.size();
  std::size_t axis = 0;
  Real best = -std::numeric_limits<Real>::infinity();
  for (std::size_t k = 0; k < d; ++k) {
    Real mn = std::numeric_limits<Real>::infinity();
    Real mx = -std::numeric_limits<Real>::infinity();
    for (auto i = lo; i < hi; ++i) {
      mn = std::min(mn, items[i].first[k]);
      mx = std::max(mx, items[i].first[k]);
    }
    if (mx - mn > best) {
      best = mx - mn;
      axis = k;
    }
  }
  std::sort(items.begin() + lo, items.begin() + hi, [axis](auto && a, auto && b) {
    if (a.first[axis] != b.first[axis]) {
      return a.first[axis] < b.first[axis];
    }
    return a.second < b.second;
  });
  Idx left = nb / 2;
  std::size_t cut = (hi - lo) * left / nb;
  rcb(items, lo, lo + cut, first, left, out);
  rcb(items, lo + cut, hi, first + left, nb - left, out);
}

/// RCB partition of the mesh read on rank 0 (nullptr elsewhere); the parts
/// in type order are returned in `parts`.
std::shared_ptr<MeshPartition> rcb_partition(Mesh & mesh, Int dim, std::vector<I64> & parts) {
  if (g_rank != 0) {
    return nullptr;
  }
  std::vector<std::pair<std::vector<Real>, Idx>> items;
  const auto & nodes = mesh.getNodes();
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
    const auto & conn = mesh.getConnectivity(type, _not_ghost);
    for (Idx e = 0; e < conn.size(); ++e) {
      std::vector<Real> c(dim, 0.);
      for (Idx n = 0; n < conn.getNbComponent(); ++n) {
        for (Int k = 0; k < dim; ++k) {
          c[k] += nodes(conn(e, n), k);
        }
      }
      for (Int k = 0; k < dim; ++k) {
        c[k] /= Real(conn.getNbComponent());
      }
      items.emplace_back(c, Idx(items.size()));
    }
  }
  std::vector<Idx> p(items.size(), 0);
  rcb(items, 0, items.size(), 0, g_size, p);
  auto mapping = std::make_shared<ElementTypeMapArray<Idx>>("rcb_partition");
  Idx k = 0;
  for (auto type : mesh.elementTypes(dim, _not_ghost, _ek_not_defined)) {
    auto & a = mapping->alloc(mesh.getNbElement(type), 1, type, _not_ghost);
    for (Idx e = 0; e < mesh.getNbElement(type); ++e) {
      a(e) = p[k++];
      parts.push_back(a(e));
    }
  }
  auto partition = std::make_shared<MeshPartitionMeshData>(mesh, mapping, dim);
  partition->partitionate(g_size);
  return partition;
}

/// upstream's StrainIncrement (test_cohesive_fixture.hh)
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

void run_case(const fs::path & root, const fs::path & up, const fs::path & frozen,
              const TestCase & tc, const TypeCase & ty) {
  const std::string name = tc.test + "_" + ty.types;
  const bool boundary = boundary_record();
  const std::string cid = std::string(boundary ? "parallel_test_cohesive_boundary/"
                                               : "parallel_test_cohesive/") +
                          name + "/np" + std::to_string(g_size) + "/rank" +
                          std::to_string(g_rank);
  const fs::path tdir = up / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const fs::path mdir = frozen / "test/test_model/test_solid_mechanics_model/test_cohesive";
  const Int dim = ty.dim;

  ap::FixtureWriter w(root, cid,
                      "TestSMMCFixture." + tc.upstream + " (" + ty.types + ") on " +
                          std::to_string(g_size) +
                          " ranks, the port's RCB partition (PARALLEL_LEVEL 2), rank view" +
                          (boundary ? "; boundary record (version 2): boundary blocks and "
                                      "element groups after the run"
                                    : ""),
                      kProbe, kProbeVersion);
  w.add_tag("parallel");
  w.add_tag("cohesive");
  record_pin(w);
  w.set_parallel(g_size, g_rank, "rcb (the port's RcbPartitioner rule, MeshPartitionMeshData)",
                 g_rank == 0 ? "partition/rcb" : "");

  // SetUp()
  auto mesh = std::make_unique<Mesh>(dim);
  std::vector<I64> parts;
  if (g_rank == 0) {
    mesh->read((mdir / ty.mesh).string());
  }
  auto partition = rcb_partition(*mesh, dim, parts);
  mesh->distribute(partition);
  getStaticParser().parse((tdir / tc.material).string());

  // createModel()
  auto model = std::make_unique<SolidMechanicsModelCohesive>(*mesh);
  model->initFull(_analysis_method = _explicit_lumped_mass, _is_extrinsic = tc.extrinsic);
  const Real stable_dt = model->getStableTimeStep();
  const Real time_step = stable_dt * 0.01;
  model->setTimeStep(time_step);
  Real surface = 0;
  Int group_size = 0;
  {
    auto facet_type = mesh->getFacetType(ty.cohesive_type);
    auto & fe_engine = model->getFEEngineBoundary();
    const auto & group = mesh->getElementGroup("insertion");
    group_size = group.size(_ghost_type = _not_ghost);
    const auto & elements = group.getElements(facet_type);
    Array<Real> ones(fe_engine.getNbIntegrationPoints(facet_type) * group_size);
    ones.set(1.);
    surface = fe_engine.integrate(ones, facet_type, _not_ghost, elements);
    mesh->getCommunicator().allReduce(surface, SynchronizerOperation::_sum);
    mesh->getCommunicator().allReduce(group_size, SynchronizerOperation::_sum);
  }

  // testModeI() / testModeII() up to the steps
  auto & mat_el = model->getMaterial("body");
  auto & mat_co = model->getMaterial("insertion");
  const Real sigma_c = mat_co.get("sigma_c");
  const Real E = mat_el.get("E");
  const Real nu = mat_el.get("nu");
  const auto direction = _y;
  const auto length = mesh->getUpperBounds()(direction) - mesh->getLowerBounds()(direction);
  UInt nb_steps = 0;
  Matrix<Real> strain;
  Real inc_factor = 2e-2;
  if (not tc.mode_ii) {
    auto speed = mat_el.getPushWaveSpeed(Element());
    nb_steps = length / speed / model->getTimeStep();
    if (dim == 2) {
      strain = Matrix<Real>{{-nu, 0.}, {0., 1. - nu}};
      strain *= (1. + nu);
    } else {
      strain = Matrix<Real>{{-nu, 0., 0.}, {0., 1., 0.}, {0., 0., -nu}};
    }
    strain *= sigma_c / E;
  } else {
    Real speed;
    try {
      speed = mat_el.getShearWaveSpeed(Element());
    } catch (...) {
      speed = mat_el.getPushWaveSpeed(Element());
    }
    nb_steps = 2 * length / 2. / speed / model->getTimeStep();
    model->applyBC(BC::Dirichlet::FlagOnly(_y), "sides");
    if (dim > 2) {
      model->applyBC(BC::Dirichlet::FlagOnly(_z), "sides");
    }
    const Real beta = mat_co.get("beta");
    if (dim == 2) {
      strain = Matrix<Real>{{0., 1.}, {0., 0.}};
    } else {
      strain = Matrix<Real>{{0., 1., 0.}, {0., 0., 0.}, {0., 0., 0.}};
    }
    strain *= (1. + nu);
    strain *= 2 * beta * beta * sigma_c / E;
    inc_factor = 0.005;
  }
  // setInitialCondition((1 - 1e-5) * strain)
  {
    Matrix<Real> init = (1 - 1e-5) * strain;
    for (auto && data :
         zip(make_view(mesh->getNodes(), dim), make_view(model->getDisplacement(), dim))) {
      std::get<1>(data) = init * std::get<0>(data);
    }
  }
  // steps(inc_factor * strain)
  Matrix<Real> inc = inc_factor * strain;
  StrainIncrement functor((1. / 300) * inc, _y);
  std::vector<I64> inserted, nb_cohesive;
  std::vector<double> ekin, epot, edis, erev, econ;
  for (UInt s = 0; s < nb_steps; ++s) {
    model->applyBC(functor, "loading");
    model->applyBC(functor, "fixed");
    I64 n = 0;
    if (tc.extrinsic) {
      n = model->checkCohesiveStress();
    }
    inserted.push_back(n);
    nb_cohesive.push_back(mesh->getNbElement(ty.cohesive_type, _not_ghost));
    model->solveStep();
    ekin.push_back(model->getEnergy("kinetic"));
    epot.push_back(model->getEnergy("potential"));
    edis.push_back(model->getEnergy("dissipated"));
    erev.push_back(model->getEnergy("reversible"));
    econ.push_back(model->getEnergy("cohesive contact"));
  }
  // checkInsertion / checkDissipated (upstream's assertions, evaluated)
  auto nb_coh_all = mesh->getNbElement(ty.cohesive_type);
  mesh->getCommunicator().allReduce(nb_coh_all, SynchronizerOperation::_sum);
  const Real G_c = mat_co.get("G_c");
  const Real edis_final = model->getEnergy("dissipated");

  auto & p = w.parameters();
  p["test"] = tc.upstream;
  p["types"] = ty.types;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["cohesive_type"] = type_name(ty.cohesive_type);
  p["extrinsic"] = tc.extrinsic;
  p["mode_ii"] = tc.mode_ii;
  p["mesh"] = "fixtures/meshes/gmsh-4.13.1/test/test_model/test_solid_mechanics_model/"
              "test_cohesive/" +
              ty.mesh;
  p["mesh_sha256"] = file_sha256(mdir / ty.mesh);
  p["material"] = "test/test_model/test_solid_mechanics_model/test_cohesive/" + tc.material;
  p["material_sha256"] = file_sha256(tdir / tc.material);
  p["rank"] = static_cast<long long>(g_rank);
  p["nb_ranks"] = static_cast<long long>(g_size);
  p["nb_steps"] = static_cast<long long>(nb_steps);
  p["surface"] = surface;
  p["group_size"] = static_cast<long long>(group_size);
  p["nb_cohesive_global"] = static_cast<long long>(nb_coh_all);
  p["check_insertion"] = (nb_coh_all == group_size);
  p["check_dissipated"] = (std::abs(surface * G_c - edis_final) <= 5e-1);
  if (boundary) {
    p["record"] = "boundary";
    if (g_rank == 0) {
      // the manifest's parallel.partition_quantity (set_parallel above)
      add_i(w, "partition/rcb", parts, 1,
            "part of every not-ghost element of the mesh dimension of the read mesh, in type "
            "order (the port's RcbPartitioner rule, given to upstream by MeshPartitionMeshData)");
    }
    std::vector<I64> gids(mesh->getNbNodes());
    for (Idx n = 0; n < mesh->getNbNodes(); ++n) {
      gids[n] = mesh->getNodeGlobalId(n);
    }
    add_i(w, "nodes/global_id", gids, 1, "node global ids after the run (local node order)");
    ap::Json blocks = ap::Json::array();
    for (auto gt : ghost_types) {
      for (auto type : mesh->elementTypes(dim - 1, gt, _ek_regular)) {
        const auto & conn = mesh->getConnectivity(type, gt);
        std::vector<I64> g(conn.size() * conn.getNbComponent());
        for (std::size_t i = 0; i < g.size(); ++i) {
          g[i] = mesh->getNodeGlobalId(conn.data()[i]);
        }
        const std::string key =
            std::string(gt == _ghost ? "ghost" : "not_ghost") + "/" + type_name(type);
        blocks.push(key);
        add_i(w, "boundary/" + key + "/connectivity_gids", g, conn.getNbComponent(),
              "boundary block connectivity after the run, in node global ids");
      }
    }
    p["boundary_blocks"] = blocks;
    ap::Json groups = ap::Json::array();
    for (auto && group : mesh->iterateElementGroups()) {
      const std::string gname = group.getName();
      groups.push(gname);
      const auto & nodes = group.getNodeGroup().getNodes();
      std::vector<I64> g(nodes.size());
      for (std::size_t i = 0; i < g.size(); ++i) {
        g[i] = mesh->getNodeGlobalId(nodes(i));
      }
      add_i(w, "groups/" + gname + "/node_gids", g, 1,
            "the group's node list after the run, in node global ids (group order)");
      std::vector<I64> counts;
      ap::Json gblocks = ap::Json::array();
      for (auto gt : ghost_types) {
        for (auto type : group.elementTypes(_ghost_type = gt)) {
          gblocks.push(std::string(gt == _ghost ? "ghost" : "not_ghost") + "/" +
                       type_name(type));
          counts.push_back(static_cast<I64>(group.getElements(type, gt).size()));
        }
      }
      add_i(w, "groups/" + gname + "/nb_elements", counts, 1,
            "the group's element count per block (blocks in groups_blocks)");
      p["groups_blocks"][gname] = gblocks;
    }
    p["groups"] = groups;
    w.finish();
    model.reset();
    mesh.reset();
    if (g_rank == 0) {
      std::cout << "  " << cid << " (boundary record)" << std::endl;
    }
    return;
  }
  if (g_rank == 0) {
    add_i(w, "partition/rcb", parts, 1,
          "part of every not-ghost element of the mesh dimension of the read mesh, in type order "
          "(the port's RcbPartitioner rule, given to upstream by MeshPartitionMeshData)");
  }
  add_r(w, "time_step", {stable_dt, time_step}, 1,
        "getStableTimeStep() (global), 0.01 * getStableTimeStep()");
  add_r(w, "surface", {surface}, 1,
        "integral of 1 over the not-ghost insertion facets, all-reduced (fixture's surface)");
  add_isteps(w, "steps/inserted", inserted,
             "checkCohesiveStress() return value per step (0 for the intrinsic tests)");
  add_isteps(w, "steps/nb_cohesive", nb_cohesive,
             "not-ghost cohesive elements on this rank after the check");
  add_steps(w, "steps/energy/kinetic", ekin, "getEnergy(kinetic) after each solveStep (global)");
  add_steps(w, "steps/energy/potential", epot,
            "getEnergy(potential) after each solveStep (global)");
  add_steps(w, "steps/energy/dissipated", edis,
            "getEnergy(dissipated) after each solveStep (global)");
  add_steps(w, "steps/energy/reversible", erev,
            "getEnergy(reversible) after each solveStep (global)");
  add_steps(w, "steps/energy/cohesive_contact", econ,
            "getEnergy(cohesive contact) after each solveStep (global)");
  {
    std::vector<I64> gids(mesh->getNbNodes());
    for (Idx n = 0; n < mesh->getNbNodes(); ++n) {
      gids[n] = mesh->getNodeGlobalId(n);
    }
    add_i(w, "nodes/global_id", gids, 1, "node global ids after the run (local node order)");
    const auto & u = model->getDisplacement();
    const auto & v = model->getVelocity();
    add_r(w, "displacement", std::vector<double>(u.data(), u.data() + u.size() * dim), dim,
          "final displacement (local node order)");
    add_r(w, "velocity", std::vector<double>(v.data(), v.data() + v.size() * dim), dim,
          "final velocity (local node order)");
  }
  ap::Json types = ap::Json::array();
  for (auto gt : ghost_types) {
    for (auto type : mesh->elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      if (Mesh::getSpatialDimension(type) != dim and Mesh::getKind(type) != _ek_cohesive) {
        continue;
      }
      const auto & conn = mesh->getConnectivity(type, gt);
      std::vector<I64> g(conn.size() * conn.getNbComponent());
      for (std::size_t i = 0; i < g.size(); ++i) {
        g[i] = mesh->getNodeGlobalId(conn.data()[i]);
      }
      const std::string key =
          std::string(gt == _ghost ? "ghost" : "not_ghost") + "/" + type_name(type);
      types.push(key);
      add_i(w, "mesh/" + key + "/connectivity_gids", g, conn.getNbComponent(),
            "connectivity after the run in node global ids (cohesive elements appended)");
    }
  }
  p["mesh_blocks"] = types;
  w.finish();
  model.reset();
  mesh.reset();
  if (g_rank == 0) {
    std::cout << "  " << cid << " (" << nb_steps << " steps, " << nb_coh_all
              << " cohesive elements)" << std::endl;
  }
}

} // namespace

int main(int argc, char * argv[]) {
  // D-050: host-independent Eigen blocking, before any Akantu call
  Eigen::setCpuCacheSizes(kEigenL1, kEigenL2, kEigenL3);
  if (argc < 5) {
    std::cerr << "usage: parallel_test_cohesive_probe <fixtures_root> <upstream_src_root> "
                 "<frozen_meshes_root> <case>\n";
    return 2;
  }
  const fs::path root = argv[1];
  const fs::path up = argv[2];
  const fs::path frozen = argv[3];
  const std::string wanted = argv[4];
  int aka_argc = 1;
  char * aka_argv[] = {argv[0], nullptr};
  char ** aka_argv_p = aka_argv;
  initialize(aka_argc, aka_argv_p);
  debug::setDebugLevel(dblError);
  const auto & comm = Communicator::getStaticCommunicator();
  g_rank = comm.whoAmI();
  g_size = comm.getNbProc();
  int rc = 1;
  try {
    for (const auto & tc : test_cases()) {
      for (const auto & ty : type_cases()) {
        if (tc.test + "_" + ty.types == wanted) {
          run_case(root, up, frozen, tc, ty);
          rc = 0;
        }
      }
    }
    if (rc != 0) {
      std::cerr << "unknown case " << wanted << "\n";
    }
  } catch (std::exception & e) {
    std::cerr << "rank " << g_rank << ": " << e.what() << "\n";
    rc = 1;
  }
  finalize();
  return rc;
}
