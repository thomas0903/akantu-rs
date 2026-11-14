// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): Mesh container queries (counts, element
// type iteration, facet connectivities, makeReady state), MeshAccessor-built
// meshes, ElementTypeMapArray::initialize(mesh, ...), ElementTypeMapArrayFilter,
// Mesh::getNbDataPerElem, the default dumpers of element and node groups
// (ElementGroup / NodeGroup constructors), the group field factories
// (createElementalField / createNodalField / createStridedNodalField /
// createFieldFromAttachedData), Mesh::getGroupDumper, Scotch node reordering
// (MeshPartitionScotch::reorder), weighted Scotch partitioning (edge / vertex
// load callbacks of partitionate), the dblDump debug files of both, and the
// Graphviz output of BoostGraphConverter.
//
// Usage:
//   mesh2_probe <fixtures_root> <upstream_root> <frozen_root> <spec>...
// spec: semicolon-separated key=value list
//   case=<case id below mesh2/>   (required)
//   file=up:<path below the reference root> | fz:<path below the frozen Gmsh
//        meshes root> | gen:<generated mesh name>
//   dim=<1|2|3>
//   ops=<comma list>: queries typemap filter groups_dump kinds_group_dump
//                     reorder weighted graphviz dbldump
//   parts=<n> (weighted)
//
// Elements are recorded as int64 triples (type code, index, ghost type): the
// type code indexes parameters.type_table (Akantu type names), the null
// element is (-1, -1, -1). Files written by upstream (VTU/PVTU, Graphviz,
// Scotch dumps) are recorded byte for byte as uint8 arrays named
// "files/<relative path>" (axis "entry", layout "table"; parameters.files
// lists them in order).

#include "akantu_probe/fixture_writer.hh"

// Akantu first: it configures Eigen (index type) before Eigen is included
#include <aka_common.hh>
#include <aka_random_generator.hh>
#include <mesh.hh>
// same dumper headers as oracle/probes/dump/dump_probe.cc (the elemental
// field templates need the compute/padder specialisations they declare)
#include <dumpable_inline_impl.hh>
#include <dumper_compute.hh>
#include <dumper_element_partition.hh>
#include <dumper_elemental_field.hh>
#include <dumper_homogenizing_field.hh>
#include <dumper_internal_material_field.hh>
#include <dumper_iohelper_paraview.hh>
#include <dumper_nodal_field.hh>
#include <element_group.hh>
#include <element_type_map_filter.hh>
#include <group_manager_inline_impl.hh>
#include <mesh_accessor.hh>
#include <mesh_partition_scotch.hh>
#include <mesh_utils.hh>
#include <node_group.hh>
#include <solid_mechanics_model.hh>

#include <boost_graph_converter.hh>

#include <Eigen/Core>

#include <scotch.h>

#include <algorithm>
#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <fstream>
#include <iostream>
#include <map>
#include <set>
#include <sstream>
#include <string>
#include <unistd.h>
#include <vector>

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "mesh2_probe";
constexpr const char * kProbeVersion = "1";
constexpr Idx kSeed = 1234;

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::map<std::string, std::string> parse_spec(const std::string & spec) {
  std::map<std::string, std::string> kv;
  std::stringstream ss(spec);
  std::string item;
  while (std::getline(ss, item, ';')) {
    auto eq = item.find('=');
    kv[item.substr(0, eq)] = eq == std::string::npos ? "1" : item.substr(eq + 1);
  }
  return kv;
}

/// Element type codes, in order of first use.
struct TypeTable {
  std::vector<std::string> names;
  std::int64_t code(ElementType t) {
    if (t == _not_defined) {
      return -2;
    }
    auto n = type_name(t);
    for (std::size_t i = 0; i < names.size(); ++i) {
      if (names[i] == n) {
        return static_cast<std::int64_t>(i);
      }
    }
    names.push_back(n);
    return static_cast<std::int64_t>(names.size() - 1);
  }
};

std::string block(ElementType t, GhostType g) {
  return type_name(t) + (g == _not_ghost ? "" : "/ghost");
}

std::string ghost_name(GhostType g) { return g == _not_ghost ? "not_ghost" : "ghost"; }

std::string kind_name(ElementKind k) {
  switch (k) {
  case _ek_regular:
    return "regular";
  case _ek_cohesive:
    return "cohesive";
  case _ek_structural:
    return "structural";
  case _ek_not_defined:
    return "not_defined";
  default:
    return "other";
  }
}

std::string dim_name(Int d) { return d == _all_dimensions ? "all" : std::to_string(d); }

template <class T> std::vector<std::int64_t> to_i64(const Array<T> & a) {
  std::vector<std::int64_t> v;
  v.reserve(a.size() * a.getNbComponent());
  for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
    v.push_back(static_cast<std::int64_t>(a.data()[i]));
  }
  return v;
}

std::vector<double> to_f64(const Array<Real> & a) {
  return std::vector<double>(a.data(), a.data() + a.size() * a.getNbComponent());
}

void add_i64(ap::FixtureWriter & w, const std::string & name, std::vector<std::size_t> shape,
             std::vector<std::string> axes, const std::vector<std::int64_t> & v,
             const std::string & layout = "table") {
  w.add(name, shape, axes, layout, v).tolerance_class("exact");
}

void add_f64(ap::FixtureWriter & w, const std::string & name, std::vector<std::size_t> shape,
             std::vector<std::string> axes, const std::vector<double> & v,
             const std::string & layout = "table") {
  w.add(name, shape, axes, layout, v).tolerance_class("exact");
}

/// Record a file written by upstream as a uint8 array "files/<rel>".
void add_file(ap::FixtureWriter & w, const std::string & rel, const fs::path & path) {
  auto bytes = ap::read_file(path);
  std::vector<std::uint8_t> v(bytes.begin(), bytes.end());
  w.add("files/" + rel, {v.size()}, {"entry"}, "table", v).tolerance_class("exact");
  w.parameters()["files"].push(rel);
}

/// Record every regular file below `dir` (sorted relative paths).
void add_tree(ap::FixtureWriter & w, const std::string & prefix, const fs::path & dir) {
  std::vector<std::string> rels;
  for (const auto & e : fs::recursive_directory_iterator(dir)) {
    if (e.is_regular_file()) {
      rels.push_back(fs::relative(e.path(), dir).generic_string());
    }
  }
  std::sort(rels.begin(), rels.end());
  for (const auto & r : rels) {
    add_file(w, prefix + r, dir / r);
  }
}

void record_connectivities(ap::FixtureWriter & w, const std::string & prefix,
                           const Mesh & mesh) {
  ap::Json blocks = ap::Json::array();
  for (auto g : ghost_types) {
    for (auto t : mesh.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & c = mesh.getConnectivity(t, g);
      add_i64(w, prefix + "/connectivity/" + block(t, g),
              {static_cast<std::size_t>(c.size()), static_cast<std::size_t>(c.getNbComponent())},
              {"element", "node_local"}, to_i64(c), "elemental");
      blocks.push(block(t, g));
      w.parameters()[prefix + "/connectivity_id/" + block(t, g)] = c.getID();
    }
  }
  w.parameters()[prefix + "/blocks"] = blocks;
}

void record_nodes(ap::FixtureWriter & w, const std::string & prefix, const Mesh & mesh) {
  const auto & X = mesh.getNodes();
  add_f64(w, prefix + "/nodes",
          {static_cast<std::size_t>(X.size()), static_cast<std::size_t>(X.getNbComponent())},
          {"node", "spatial_dim"}, to_f64(X), "nodal");
  std::vector<std::int64_t> flags;
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    flags.push_back(static_cast<std::int64_t>(static_cast<std::uint8_t>(mesh.getNodeFlag(n))));
  }
  add_i64(w, prefix + "/flags", {flags.size()}, {"node"}, flags, "nodal");
  w.parameters()[prefix + "/nodes_id"] = X.getID();
}

void record_groups(ap::FixtureWriter & w, const std::string & prefix, const Mesh & mesh) {
  ap::Json eg = ap::Json::array();
  for (const auto & group : mesh.iterateElementGroups()) {
    ap::Json g = ap::Json::object();
    g["name"] = group.getName();
    g["dimension"] = static_cast<long long>(group.getDimension());
    ap::Json types = ap::Json::array();
    for (auto gt : ghost_types) {
      for (auto t : group.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
        const auto & els = group.getElements(t, gt);
        add_i64(w, prefix + "/element_groups/" + group.getName() + "/" + block(t, gt),
                {static_cast<std::size_t>(els.size())}, {"group_member"}, to_i64(els));
        types.push(block(t, gt));
      }
    }
    g["blocks"] = types;
    const auto & nodes = group.getNodeGroup().getNodes();
    add_i64(w, prefix + "/element_groups/" + group.getName() + "/nodes",
            {static_cast<std::size_t>(nodes.size())}, {"group_member"}, to_i64(nodes));
    eg.push(g);
  }
  w.parameters()[prefix + "/element_groups"] = eg;
  ap::Json ng = ap::Json::array();
  for (const auto & group : mesh.iterateNodeGroups()) {
    ng.push(group.getName());
    add_i64(w, prefix + "/node_groups/" + group.getName(),
            {static_cast<std::size_t>(group.getNodes().size())}, {"group_member"},
            to_i64(group.getNodes()));
  }
  w.parameters()[prefix + "/node_groups"] = ng;
}

/// Generated meshes (built through MeshAccessor, as upstream tests do).
void generate(Mesh & mesh, const std::string & name) {
  MeshAccessor acc(mesh);
  if (name == "accessor_quads") {
    // MeshAccessor resizeNodes / resizeConnectivity / getConnectivity /
    // makeReady, as test_model_solver's genMesh: 2 x 1 quadrangles + a
    // boundary segment, then makeReady.
    acc.resizeNodes(6);
    auto & X = acc.getNodes();
    const Real xs[6][2] = {{0., 0.}, {1., 0.}, {2., 0.}, {0., 1.}, {1., 1.}, {2., 1.}};
    for (Idx n = 0; n < 6; ++n) {
      X(n, 0) = xs[n][0];
      X(n, 1) = xs[n][1];
    }
    acc.resizeConnectivity(2, _quadrangle_4);
    auto & c = acc.getConnectivity(_quadrangle_4);
    const Idx q[2][4] = {{0, 1, 4, 3}, {1, 2, 5, 4}};
    for (Idx e = 0; e < 2; ++e) {
      for (Idx k = 0; k < 4; ++k) {
        c(e, k) = q[e][k];
      }
    }
    acc.resizeConnectivity(1, _segment_2);
    auto & s = acc.getConnectivity(_segment_2);
    s(0, 0) = 0;
    s(0, 1) = 2;
    acc.makeReady();
    return;
  }
  auto & nodes = acc.getNodes();
  auto push = [&](std::initializer_list<Real> x) {
    Vector<Real> v(mesh.getSpatialDimension());
    Int i = 0;
    for (auto c : x) {
      v(i++) = c;
    }
    nodes.push_back(v);
  };
  auto conn = [&](ElementType t, std::initializer_list<Idx> c) {
    Vector<Idx> v(c.size());
    Int i = 0;
    for (auto n : c) {
      v(i++) = n;
    }
    acc.getConnectivity(t).push_back(v);
  };
  if (name == "cohesive_2d") {
    // two triangles on each side of x = 1, doubled edge nodes 1/4, 2/5
    push({0., 0.});
    push({1., 0.});
    push({1., 1.});
    push({2., 0.});
    push({1., 0.});
    push({1., 1.});
    conn(_triangle_3, {0, 1, 2});
    conn(_triangle_3, {4, 3, 5});
    conn(_cohesive_2d_4, {1, 2, 4, 5});
  } else if (name == "mixed_2d_kinds") {
    // quadrangle + triangle sharing an edge + a hanging segment and point
    push({0., 0.});
    push({1., 0.});
    push({1., 1.});
    push({0., 1.});
    push({2., 0.5});
    push({3., 0.5});
    conn(_quadrangle_4, {0, 1, 2, 3});
    conn(_triangle_3, {1, 4, 2});
    conn(_segment_2, {4, 5});
    conn(_point_1, {5});
  } else if (name == "beams_2d") {
    // structural kind: two Bernoulli beams
    push({0., 0.});
    push({1., 0.});
    push({2., 1.});
    conn(_bernoulli_beam_2, {0, 1});
    conn(_bernoulli_beam_2, {1, 2});
  } else {
    throw std::runtime_error("unknown generated mesh " + name);
  }
  acc.makeReady();
}

const std::vector<Int> kDims = {_all_dimensions, 0, 1, 2, 3};
const std::vector<ElementKind> kKinds = {_ek_not_defined, _ek_regular, _ek_cohesive,
                                         _ek_structural};

/* -------------------------------------------------------------------------- */
/* queries: MESH-001...004                                                     */
/* -------------------------------------------------------------------------- */
void op_queries(ap::FixtureWriter & w, TypeTable & tt, Mesh & mesh) {
  auto & p = w.parameters();
  p["mesh/id"] = mesh.getID();
  p["mesh/spatial_dimension"] = static_cast<long long>(mesh.getSpatialDimension());
  p["mesh/nb_nodes"] = static_cast<long long>(mesh.getNbNodes());
  p["mesh/nb_global_nodes"] = static_cast<long long>(mesh.getNbGlobalNodes());
  p["mesh/is_distributed"] = mesh.isDistributed();
  p["mesh/is_periodic"] = mesh.isPeriodic();
  p["mesh/is_mesh_facets"] = mesh.isMeshFacets();
  auto d = mesh.getSpatialDimension();
  auto vec = [&](const Vector<Real> & v) {
    return std::vector<double>(v.data(), v.data() + d);
  };
  add_f64(w, "bbox/lower", {static_cast<std::size_t>(d)}, {"spatial_dim"},
          vec(mesh.getLowerBounds()));
  add_f64(w, "bbox/upper", {static_cast<std::size_t>(d)}, {"spatial_dim"},
          vec(mesh.getUpperBounds()));
  add_f64(w, "bbox/local_lower", {static_cast<std::size_t>(d)}, {"spatial_dim"},
          vec(mesh.getLocalLowerBounds()));
  add_f64(w, "bbox/local_upper", {static_cast<std::size_t>(d)}, {"spatial_dim"},
          vec(mesh.getLocalUpperBounds()));
  record_nodes(w, "mesh", mesh);
  record_connectivities(w, "mesh", mesh);
  p["mesh/connectivities_id"] = mesh.getConnectivities().getID();

  // element type iteration (enum order) and counts per (dim, ghost, kind)
  for (auto g : ghost_types) {
    for (auto dd : kDims) {
      for (auto k : kKinds) {
        std::vector<std::int64_t> codes;
        for (auto t : mesh.elementTypes(dd, g, k)) {
          codes.push_back(tt.code(t));
        }
        auto key = dim_name(dd) + "/" + ghost_name(g) + "/" + kind_name(k);
        add_i64(w, "element_types/" + key, {codes.size()}, {"entry"}, codes);
        // upstream asserts dim <= 3 || dim == -1 while _all_dimensions is
        // INT_MAX: with debug checks (oracle) the all-dimensions count throws
        try {
          p["nb_element/" + key] = static_cast<long long>(mesh.getNbElement(dd, g, k));
        } catch (std::exception & e) {
          p["nb_element/" + key] = std::string("exception: ") + e.what();
        }
      }
    }
  }
  // getNbElement(type, ghost): every type of the mesh plus absent ones (0)
  std::vector<std::int64_t> codes;
  std::vector<std::int64_t> counts;
  for (auto g : ghost_types) {
    for (auto t : {_point_1, _segment_2, _segment_3, _triangle_3, _triangle_6, _quadrangle_4,
                   _quadrangle_8, _tetrahedron_4, _tetrahedron_10, _pentahedron_6,
                   _pentahedron_15, _hexahedron_8, _hexahedron_20, _cohesive_2d_4,
                   _bernoulli_beam_2}) {
      codes.push_back(tt.code(t));
      codes.push_back(g == _not_ghost ? 0 : 1);
      counts.push_back(static_cast<std::int64_t>(mesh.getNbElement(t, g)));
    }
  }
  add_i64(w, "nb_element_type/keys", {codes.size() / 2, 2}, {"entry", "component"}, codes);
  add_i64(w, "nb_element_type/values", {counts.size()}, {"entry"}, counts);

  // getFacetConnectivity(element, t) for every element and facet type
  for (auto g : ghost_types) {
    for (auto t : mesh.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      if (Mesh::getKind(t) == _ek_structural) {
        continue; // no facets
      }
      auto nb_facet_types = Mesh::getNbFacetTypes(t);
      for (Idx ft = 0; ft < nb_facet_types; ++ft) {
        std::vector<std::int64_t> v;
        std::size_t rows = 0;
        std::size_t cols = 0;
        for (Idx e = 0; e < mesh.getNbElement(t, g); ++e) {
          auto fc = mesh.getFacetConnectivity(Element{t, e, g}, ft);
          rows = static_cast<std::size_t>(fc.rows());
          cols = static_cast<std::size_t>(fc.cols());
          for (Idx r = 0; r < fc.rows(); ++r) {
            for (Idx c = 0; c < fc.cols(); ++c) {
              v.push_back(static_cast<std::int64_t>(fc(r, c)));
            }
          }
        }
        add_i64(w, "facet_connectivity/" + block(t, g) + "/" + std::to_string(ft),
                {static_cast<std::size_t>(mesh.getNbElement(t, g)), rows, cols},
                {"element", "facet", "node_local"}, v, "elemental");
      }
    }
  }
  record_groups(w, "mesh", mesh);
}

/* -------------------------------------------------------------------------- */
/* typemap: MESH-017...019                                                     */
/* -------------------------------------------------------------------------- */
template <class T>
void record_typemap(ap::FixtureWriter & w, TypeTable & tt, const std::string & name,
                    const ElementTypeMapArray<T> & a) {
  std::vector<std::int64_t> blocks;
  ap::Json ids = ap::Json::array();
  std::vector<double> values;
  for (auto g : ghost_types) {
    for (auto t : a.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & arr = a(t, g);
      blocks.push_back(tt.code(t));
      blocks.push_back(g == _not_ghost ? 0 : 1);
      blocks.push_back(static_cast<std::int64_t>(arr.size()));
      blocks.push_back(static_cast<std::int64_t>(arr.getNbComponent()));
      ids.push(arr.getID());
      for (Idx i = 0; i < arr.size() * arr.getNbComponent(); ++i) {
        values.push_back(static_cast<double>(arr.data()[i]));
      }
    }
  }
  add_i64(w, "typemap/" + name + "/blocks", {blocks.size() / 4, 4},
          {"entry", "component"}, blocks);
  add_f64(w, "typemap/" + name + "/values", {values.size()}, {"entry"}, values);
  w.parameters()["typemap/" + name + "/ids"] = ids;
  w.parameters()["typemap/" + name + "/id"] = a.getID();
}

void op_typemap(ap::FixtureWriter & w, TypeTable & tt, Mesh & mesh) {
  {
    ElementTypeMapArray<Real> a("default", "probe");
    a.initialize(mesh);
    record_typemap(w, tt, "default", a);
  }
  {
    ElementTypeMapArray<Real> a("nbelem", "probe");
    a.initialize(mesh, _nb_component = 3, _with_nb_element = true, _default_value = 2.5);
    record_typemap(w, tt, "nbelem", a);
    // re-initialisation of existing blocks without _with_nb_element: size 0
    a.initialize(mesh, _nb_component = 3);
    record_typemap(w, tt, "nbelem_reinit", a);
  }
  {
    ElementTypeMapArray<Int> a("alldims_npe", "probe");
    a.initialize(mesh, _nb_component = 2, _spatial_dimension = _all_dimensions,
                 _with_nb_element = true, _with_nb_nodes_per_element = true,
                 _default_value = Int(-7));
    record_typemap(w, tt, "alldims_npe", a);
  }
  {
    ElementTypeMapArray<Idx> a("facets_regular", "probe");
    a.initialize(mesh, _spatial_dimension = Int(mesh.getSpatialDimension() - 1),
                 _ghost_type = _not_ghost, _element_kind = _ek_regular,
                 _with_nb_element = true, _default_value = Idx(4));
    record_typemap(w, tt, "facets_regular", a);
  }
  {
    ElementTypeMapArray<Real> a("functor", "probe");
    a.initialize(
        mesh,
        _nb_component =
            [](ElementType t, GhostType g) -> Int {
              return Mesh::getNbNodesPerElement(t) + (g == _ghost ? 1 : 0);
            },
        _spatial_dimension = _all_dimensions, _with_nb_element = true);
    record_typemap(w, tt, "functor", a);
  }
  {
    // element filter: every second element of each type of the mesh dimension
    ElementTypeMapArray<Idx> filter("filter", "probe");
    for (auto t : mesh.elementTypes(mesh.getSpatialDimension(), _not_ghost, _ek_not_defined)) {
      auto & f = filter.alloc(0, 1, t, _not_ghost);
      for (Idx e = 0; e < mesh.getNbElement(t); e += 2) {
        f.push_back(e);
      }
    }
    ElementTypeMapArray<Real> a("filtered", "probe");
    a.initialize(mesh, _element_filter = &filter, _with_nb_element = true,
                 _nb_component = 2, _default_value = 1.5);
    record_typemap(w, tt, "filtered", a);
  }
}

/* -------------------------------------------------------------------------- */
/* filter: MESH-020, getNbDataPerElem (MESH-027)                               */
/* -------------------------------------------------------------------------- */
void op_filter(ap::FixtureWriter & w, TypeTable & tt, Mesh & mesh) {
  constexpr Int nb_comp = 2;
  constexpr Int nb_item = 3;
  ElementTypeMapArray<Real> data("data", "probe");
  ElementTypeMap<Int> nb_data_per_elem;
  for (auto t : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    auto nb = mesh.getNbElement(t);
    auto & a = data.alloc(nb * nb_item, nb_comp, t, _not_ghost);
    for (Idx e = 0; e < nb; ++e) {
      for (Int q = 0; q < nb_item; ++q) {
        for (Int c = 0; c < nb_comp; ++c) {
          a(e * nb_item + q, c) = Real(1000 * e + 10 * q + c) + 0.25 * Real(tt.code(t));
        }
      }
    }
    nb_data_per_elem(nb_comp * nb_item, t, _not_ghost);
  }
  auto nbd = mesh.getNbDataPerElem(data);
  {
    std::vector<std::int64_t> v;
    for (auto t : nbd.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      v.push_back(tt.code(t));
      v.push_back(nbd(t));
    }
    add_i64(w, "nb_data_per_elem/data", {v.size() / 2, 2}, {"entry", "component"}, v);
  }
  if (mesh.hasData("tag_0")) {
    auto nbt = mesh.getNbDataPerElem(mesh.getData<Int>("tag_0"));
    std::vector<std::int64_t> v;
    for (auto t : nbt.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      v.push_back(tt.code(t));
      v.push_back(nbt(t));
    }
    add_i64(w, "nb_data_per_elem/tag_0", {v.size() / 2, 2}, {"entry", "component"}, v);
  }
  ap::Json groups = ap::Json::array();
  for (const auto & group : mesh.iterateElementGroups()) {
    groups.push(group.getName());
    for (int with_nbd = 0; with_nbd < 2; ++with_nbd) {
      const auto & filter_els = group.getElements();
      std::unique_ptr<ElementTypeMapArrayFilter<Real>> f;
      if (with_nbd != 0) {
        f = std::make_unique<ElementTypeMapArrayFilter<Real>>(data, filter_els,
                                                              nb_data_per_elem);
      } else {
        f = std::make_unique<ElementTypeMapArrayFilter<Real>>(data, filter_els);
      }
      std::vector<std::int64_t> meta;
      std::vector<double> values;
      for (auto t : f->elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
        auto af = (*f)(t, _not_ghost);
        meta.push_back(tt.code(t));
        meta.push_back(af.size());
        meta.push_back(af.getNbComponent());
        auto it = af.begin_reinterpret(nb_comp, af.size());
        auto end = af.end_reinterpret(nb_comp, af.size());
        for (; it != end; ++it) {
          auto v = *it;
          for (Int c = 0; c < nb_comp; ++c) {
            values.push_back(v(c));
          }
        }
      }
      auto base = "filter/" + group.getName() + (with_nbd != 0 ? "/nbd" : "/plain");
      add_i64(w, base + "/meta", {meta.size() / 3, 3}, {"entry", "component"}, meta);
      add_f64(w, base + "/values", {values.size()}, {"entry"}, values);
    }
  }
  w.parameters()["filter/groups"] = groups;
}

/* -------------------------------------------------------------------------- */
/* groups_dump: MESH-022, MESH-023, MESH-027                                   */
/* -------------------------------------------------------------------------- */
void op_groups_dump(ap::FixtureWriter & w, TypeTable & tt, Mesh & mesh, const fs::path & scratch) {
  (void)tt;
  auto out = scratch / "groups_dump";
  fs::create_directories(out);
  auto & p = w.parameters();
  // default dumpers of every element group (name, file name, default flag)
  ap::Json names = ap::Json::array();
  for (auto & group : mesh.iterateElementGroups()) {
    names.push(group.getName());
    p["group_dumpers/" + group.getName() + "/default"] = group.getDefaultDumperName();
  }
  p["group_dumpers/element_groups"] = names;
  std::cerr << "[groups_dump] plain" << std::endl;
  // 1) plain default dump of each element group (positions + connectivity)
  for (auto & group : mesh.iterateElementGroups()) {
    group.setDirectory((out / "plain").string());
    group.dump();
  }
  std::cerr << "[groups_dump] mixed" << std::endl;
  // 2) a mixed-dimension group (_all_dimensions filtered mesh)
  Int dim = mesh.getSpatialDimension();
  auto & mixed = mesh.createElementGroup("mixed", dim);
  for (auto t : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    for (Idx e = 0; e < mesh.getNbElement(t); e += 3) {
      mixed.add(Element{t, e, _not_ghost}, true);
    }
  }
  mixed.optimize();
  mixed.setDirectory((out / "mixed").string());
  mixed.dump();
  std::cerr << "[groups_dump] fields" << std::endl;
  // 3) fields from the factories on the group of the mesh dimension
  std::string vol;
  std::string facet;
  for (auto & group : mesh.iterateElementGroups()) {
    if (group.getName() == "mixed") {
      continue;
    }
    if (group.getDimension() == dim and vol.empty()) {
      vol = group.getName();
    }
    if (group.getDimension() == dim - 1 and facet.empty()) {
      facet = group.getName();
    }
  }
  p["group_dumpers/vol"] = vol;
  p["group_dumpers/facet"] = facet;
  Array<Real> disp(mesh.getNbNodes(), dim);
  for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
    for (Int c = 0; c < dim; ++c) {
      disp(n, c) = mesh.getNodes()(n, c) * Real(c + 1) + 0.5;
    }
  }
  if (not vol.empty()) {
    auto & g = mesh.getElementGroup(vol);
    g.setDirectory((out / "fields").string());
    g.addDumpFieldExternal("tag_0",
                           mesh.createFieldFromAttachedData<Int>("tag_0", vol, _ek_regular));
    g.addDumpFieldExternal("disp", mesh.createNodalField(&disp, vol, 3));
    g.addDumpFieldExternal("disp_1", mesh.createStridedNodalField(&disp, vol, 1, 1, 0));
    g.dump();
  }
  if (not facet.empty()) {
    auto & g = mesh.getElementGroup(facet);
    g.setDirectory((out / "fields").string());
    g.addDumpFieldExternal(
        "tag_0", mesh.createElementalField<Int, Vector<Int>, dumpers::ElementalField>(
                     mesh.getData<Int>("tag_0"), facet, dim - 1, _ek_regular));
    g.addDumpFieldExternal("disp", mesh.createNodalField(&disp, facet));
    // getGroupDumper(dumper, group) + setBaseName through it
    mesh.getGroupDumper("paraview_" + facet, facet).setBaseName(facet + "_via_mesh");
    g.dump();
  }
  std::cerr << "[groups_dump] all" << std::endl;
  // 4) whole mesh ("all") dumper on the mesh itself
  mesh.registerDumper<DumperParaview>("mesh_all", "mesh_all", true);
  mesh.setDirectoryToDumper("mesh_all", (out / "all").string());
  mesh.addDumpMeshToDumper("mesh_all", mesh, dim, _not_ghost, _ek_regular);
  mesh.addDumpFieldExternalToDumper(
      "mesh_all", "tag_0", mesh.createFieldFromAttachedData<Int>("tag_0", "all", _ek_regular));
  mesh.addDumpFieldExternalToDumper("mesh_all", "disp", mesh.createNodalField(&disp, "all", 3));
  mesh.getGroupDumper("mesh_all", "all").setBaseName("mesh_all_via_group_dumper");
  mesh.dump("mesh_all");
  // 5) renamed groups are not probed: GroupManager::renameGroup binds
  // `auto && group_ptr = std::move(it->second)` (a reference into the map
  // node), erases the node (destroying the group) and then moves from the
  // dangling reference, a use after free (UD-671; segfault observed here)
  std::cerr << "[groups_dump] nodes" << std::endl;
  // 6) node groups: default dumper with the "positions" field
  auto & ng = mesh.createNodeGroup("corner_nodes");
  for (Idx n = 0; n < mesh.getNbNodes(); n += 4) {
    ng.add(n);
  }
  p["group_dumpers/corner_nodes/default"] = ng.getDefaultDumperName();
  ng.setDirectory((out / "nodes").string());
  ng.dump();
  if (not vol.empty()) {
    auto & vng = mesh.getNodeGroup(vol + "_nodes"); // createElementGroup names it so
    vng.setDirectory((out / "nodes").string());
    vng.dump();
  }
  add_tree(w, "groups_dump/", out);
}

/* -------------------------------------------------------------------------- */
/* kinds_group_dump: MESH-022, MESH-023 on groups of every element kind         */
/* -------------------------------------------------------------------------- */
// Review adversarial mode `kinds_group_dump`: the
// default dumper of an element group holding every element of the mesh
// (cohesive, structural, segment and point elements of the generated meshes)
// added in reverse order, dumped before and after `optimize`, and the default
// dumper of a node group filled in reverse order. Upstream's default element
// group selection is every kind (`addDumpFilteredMesh(..., _all_dimensions)`,
// `_ek_not_defined`).
void op_kinds_group_dump(ap::FixtureWriter & w, Mesh & mesh, const fs::path & scratch,
                         const std::string & name) {
  auto out = scratch / "kinds";
  fs::create_directories(out); // IOHelper does not create it
  auto & g = mesh.createElementGroup("all_" + name, mesh.getSpatialDimension());
  for (auto t : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    for (Idx e = mesh.getNbElement(t) - 1; e >= 0; --e) { // reverse order
      g.add(Element{t, e, _not_ghost}, true);
    }
  }
  g.setDirectory(out.string());
  g.dump();
  g.optimize();
  g.dump();
  auto & ng = mesh.createNodeGroup("odd_" + name);
  for (Idx n = mesh.getNbNodes() - 1; n >= 0; n -= 2) {
    ng.add(n);
  }
  ng.setDirectory(out.string());
  ng.dump();
  auto & p = w.parameters();
  p["kinds/element_group"] = g.getName();
  p["kinds/element_group_nodes"] = static_cast<long long>(g.getNodeGroup().size());
  p["kinds/node_group"] = ng.getName();
  p["kinds/node_group_size"] = static_cast<long long>(ng.size());
  add_tree(w, "kinds/", out);
}

/* -------------------------------------------------------------------------- */
/* reorder / weighted partition: MUTIL-014                                     */
/* -------------------------------------------------------------------------- */
std::ofstream & null_stream() {
  static std::ofstream s("/dev/null");
  return s;
}

void op_reorder(ap::FixtureWriter & w, TypeTable & tt, Mesh & mesh, const fs::path & scratch,
                bool dbldump) {
  (void)tt;
  record_nodes(w, "reorder/before", mesh);
  record_connectivities(w, "reorder/before", mesh);
  record_groups(w, "reorder/before", mesh);
  auto dir = scratch / "reorder";
  fs::create_directories(dir);
  auto cwd = fs::current_path();
  fs::current_path(dir);
  RandomGenerator<Idx>::seed(kSeed);
  {
    MeshPartitionScotch part(mesh, mesh.getSpatialDimension());
    // the constructor seeds Scotch (SCOTCH_randomSeed); the reset makes every
    // call of this multi-case process behave as in a fresh process (the
    // port's shim does the same)
    SCOTCH_randomReset();
    if (dbldump) {
      debug::debugger.setOutStream(null_stream());
      debug::DebugLevelContext ctx(dblDump);
      part.reorder();
    } else {
      part.reorder();
    }
  }
  debug::debugger.setOutStream(std::cerr);
  fs::current_path(cwd);
  record_nodes(w, "reorder/after", mesh);
  record_connectivities(w, "reorder/after", mesh);
  record_groups(w, "reorder/after", mesh);
  w.parameters()["reorder/nb_global_nodes"] = static_cast<long long>(mesh.getNbGlobalNodes());
  if (dbldump) {
    add_tree(w, "reorder_dump/", dir);
  }
  w.parameters()["scotch/seed"] = static_cast<long long>(kSeed);
  w.parameters()["scotch/version"] = std::to_string(SCOTCH_VERSION) + "." +
                                     std::to_string(SCOTCH_RELEASE) + "." +
                                     std::to_string(SCOTCH_PATCHLEVEL);
  const char * threads = std::getenv("SCOTCH_PTHREAD_NUMBER");
  w.parameters()["scotch/pthread_number"] = std::string(threads ? threads : "unset");
}

void op_weighted(ap::FixtureWriter & w, TypeTable & tt, Mesh & mesh, const fs::path & scratch,
                 Int nb_parts, bool dbldump) {
  auto dir = scratch / "weighted";
  fs::create_directories(dir);
  auto cwd = fs::current_path();
  fs::current_path(dir);
  // edge load: 1 + (index sum mod 4); vertex load: 1 + (index mod 3), +5 for
  // the first type of the mesh dimension (types in enum order)
  auto edge = [](const Element & a, const Element & b) -> Int {
    return 1 + Int((a.element + b.element) % 4);
  };
  auto vertex = [](const Element & a) -> Int { return 1 + Int(a.element % 3); };
  std::vector<std::int64_t> parts;
  for (int weighted = 0; weighted < 2; ++weighted) {
    RandomGenerator<Idx>::seed(kSeed);
    MeshPartitionScotch part(mesh, mesh.getSpatialDimension());
    SCOTCH_randomReset(); // as in op_reorder
    if (weighted != 0) {
      if (dbldump) {
        debug::debugger.setOutStream(null_stream());
        debug::DebugLevelContext ctx(dblDump);
        part.partitionate(nb_parts, edge, vertex);
      } else {
        part.partitionate(nb_parts, edge, vertex);
      }
      debug::debugger.setOutStream(std::cerr);
    } else {
      part.partitionate(nb_parts);
    }
    std::vector<std::int64_t> v;
    for (auto t : mesh.elementTypes(mesh.getSpatialDimension(), _not_ghost, _ek_not_defined)) {
      const auto & pa = part.getPartition(t, _not_ghost);
      for (Idx e = 0; e < pa.size(); ++e) {
        v.push_back(static_cast<std::int64_t>(pa(e)));
      }
    }
    add_i64(w, weighted != 0 ? "partition/weighted" : "partition/unit", {v.size()}, {"element"},
            v, "elemental");
  }
  fs::current_path(cwd);
  if (dbldump) {
    add_tree(w, "weighted_dump/", dir);
  }
  (void)tt;
  w.parameters()["partition/nb_parts"] = static_cast<long long>(nb_parts);
  w.parameters()["scotch/seed"] = static_cast<long long>(kSeed);
  w.parameters()["scotch/version"] = std::to_string(SCOTCH_VERSION) + "." +
                                     std::to_string(SCOTCH_RELEASE) + "." +
                                     std::to_string(SCOTCH_PATCHLEVEL);
  const char * threads = std::getenv("SCOTCH_PTHREAD_NUMBER");
  w.parameters()["scotch/pthread_number"] = std::string(threads ? threads : "unset");
}

/* -------------------------------------------------------------------------- */
/* graphviz: MUTIL-017                                                         */
/* -------------------------------------------------------------------------- */
void op_graphviz(ap::FixtureWriter & w, Mesh & mesh, const fs::path & scratch) {
  mesh.initMeshFacets();
  auto dir = scratch / "graphviz";
  fs::create_directories(dir);
  BoostGraphConverter converter(mesh);
  converter.toGraphviz((dir / "graph.dot").string());
  add_file(w, "graph.dot", dir / "graph.dot");
}

void probe(const fs::path & root, const fs::path & up_root, const fs::path & fz_root,
           const std::string & spec) {
  auto kv = parse_spec(spec);
  const auto cid = "mesh2/" + kv.at("case");
  const auto file_spec = kv.at("file");
  const auto colon = file_spec.find(':');
  const auto where = file_spec.substr(0, colon);
  const auto rel = file_spec.substr(colon + 1);
  fs::path file;
  std::string gen;
  if (where == "up") {
    file = up_root / rel;
  } else if (where == "fz") {
    file = fz_root / rel;
  } else if (where == "gen") {
    gen = rel;
  } else {
    throw std::runtime_error("bad file spec " + file_spec);
  }
  const Int dim = std::stoi(kv.at("dim"));
  std::set<std::string> ops;
  {
    std::stringstream ss(kv.at("ops"));
    std::string op;
    while (std::getline(ss, op, ',')) {
      ops.insert(op);
    }
  }
  ap::FixtureWriter w(root, cid, "mesh2 probe case " + kv.at("case"), kProbe,
                      kProbeVersion);
  w.parameters()["files"] = ap::Json::array();
  w.parameters()["spec"] = spec;
  if (gen.empty()) {
    w.add_input_reference("mesh", file_spec, file);
  }
  std::string sname = kv.at("case");
  std::replace(sname.begin(), sname.end(), '/', '_');
  auto scratch =
      fs::temp_directory_path() / ("mesh2-" + std::to_string(::getpid()) + "-" + sname);
  fs::remove_all(scratch);
  fs::create_directories(scratch);

  TypeTable tt;
  auto load = [&](Mesh & m) {
    if (gen.empty()) {
      m.read(file.string());
    } else {
      generate(m, gen);
    }
  };
  const bool dbldump = ops.count("dbldump") != 0;
  // An upstream exception is recorded as the expected outcome of the case
  // (quantities recorded before it stay in the fixture).
  try {
    if (ops.count("queries")) {
      Mesh m(dim, "mesh2_" + kv.at("case").substr(kv.at("case").rfind('/') + 1));
      load(m);
      op_queries(w, tt, m);
    }
    if (ops.count("typemap")) {
      Mesh m(dim, "typemap");
      load(m);
      op_typemap(w, tt, m);
    }
    if (ops.count("filter")) {
      Mesh m(dim, "filter");
      load(m);
      op_filter(w, tt, m);
    }
    if (ops.count("groups_dump")) {
      Mesh m(dim, "groups");
      load(m);
      op_groups_dump(w, tt, m, scratch);
    }
    if (ops.count("kinds_group_dump")) {
      if (gen.empty()) {
        throw std::runtime_error("kinds_group_dump needs a generated mesh");
      }
      Mesh m(dim, "kinds_" + gen);
      load(m);
      op_kinds_group_dump(w, m, scratch, gen);
    }
    if (ops.count("reorder")) {
      Mesh m(dim, "reorder");
      load(m);
      op_reorder(w, tt, m, scratch, dbldump);
    }
    if (ops.count("weighted")) {
      Mesh m(dim, "weighted");
      load(m);
      op_weighted(w, tt, m, scratch, kv.count("parts") ? std::stoi(kv.at("parts")) : 3, dbldump);
    }
    if (ops.count("graphviz")) {
      Mesh m(dim, "graphviz");
      load(m);
      op_graphviz(w, m, scratch);
    }
  } catch (std::exception & e) {
    w.expect_error("exception", e.what());
    std::cout << "  " << cid << ": upstream exception: " << e.what() << "\n";
  }
  w.parameters()["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
  fs::remove_all(scratch);
  std::cout << "  " << cid << "\n";
}

} // namespace

int main(int argc, char ** argv) {
  // D-050: host-independent Eigen blocking in every probe
  Eigen::setCpuCacheSizes(16384, 524288, 524288);
  if (argc < 5) {
    std::cerr << "usage: " << argv[0]
              << " <fixtures_root> <upstream_root> <frozen_root> <spec>...\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  int failures = 0;
  for (int i = 4; i < argc; ++i) {
    try {
      probe(argv[1], argv[2], argv[3], argv[i]);
    } catch (std::exception & e) {
      std::cerr << "mesh2_probe: " << argv[i] << " failed: " << e.what() << "\n";
      ++failures;
    }
  }
  finalize();
  return failures == 0 ? 0 : 1;
}
