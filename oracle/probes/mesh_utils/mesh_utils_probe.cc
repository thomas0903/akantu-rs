// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) for the mesh utilities: facet meshes
// (Mesh::initMeshFacets, MeshUtils::buildAllFacets), in-place boundary
// facets (MeshUtils::buildFacets), element <-> sub-element data written by
// the readers (fillElementToSubElementsData), node -> element CSR and sets,
// clusters and boundary groups, purifyMesh, eraseElements renumbering,
// makePeriodic pairing and makeConsistentForPeriodicity, group iteration.
//
// Usage:
//   mesh_utils_probe <fixtures_root> <upstream_root> <frozen_root> <spec>...
// spec: semicolon-separated key=value list
//   case=<case id below mesh_utils/>   (required)
//   file=up:<path below the reference root> | fz:<path below the frozen
//        Gmsh meshes root> | gen:<generated mesh name> | in:<file of this
//        probe's inputs/ directory> (read from a scratch copy, so that the
//        Diana reader's `material_<name>` side file is written there)
//   dim=<1|2|3>
//   ops=<comma list>: facets csr buildfacets boundary clusters purify erase
//                     periodic periodic_groups iterators
//   verified=<path below the reference root>  (copied as input "verified")
//
// Elements are recorded as int64 triples (type code, index, ghost type):
// the type code indexes parameters.type_table (Akantu type names), the
// null element is (-1, -1, -1) and a default-constructed Element
// (`_not_defined`, 0, `_not_ghost`) has type code -2.

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <aka_csr.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_utils.hh>
#include <node_group.hh>

#include <cstdint>
#include <cstdlib>
#include <filesystem>
#include <iostream>
#include <map>
#include <set>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "mesh_utils_probe";
constexpr const char * kProbeVersion = "1";

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string safe(const std::string & n) {
  std::string out;
  for (char c : n) {
    out += (std::isspace(static_cast<unsigned char>(c)) or c == '/') ? '_' : c;
  }
  return out.empty() ? std::string("_") : out;
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
  void encode(const Element & e, std::vector<std::int64_t> & out) {
    if (e == ElementNull) {
      out.insert(out.end(), {-1, -1, -1});
      return;
    }
    out.push_back(code(e.type));
    out.push_back(static_cast<std::int64_t>(e.element));
    out.push_back(e.ghost_type == _not_ghost ? 0 : 1);
  }
};

std::string block(ElementType t, GhostType g) {
  return type_name(t) + (g == _not_ghost ? "" : "/ghost");
}

std::vector<std::int64_t> to_i64(const Array<Idx> & a) {
  std::vector<std::int64_t> v;
  v.reserve(a.size() * a.getNbComponent());
  for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
    v.push_back(static_cast<std::int64_t>(a.data()[i]));
  }
  return v;
}

void add_i64(ap::FixtureWriter & w, const std::string & name,
             std::vector<std::size_t> shape, std::vector<std::string> axes,
             const std::vector<std::int64_t> & v, const std::string & layout = "table") {
  w.add(name, shape, axes, layout, v).tolerance_class("exact");
}

void record_connectivities(ap::FixtureWriter & w, const std::string & prefix,
                           const Mesh & mesh) {
  ap::Json blocks = ap::Json::array();
  for (auto g : ghost_types) {
    for (auto t : mesh.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & c = mesh.getConnectivity(t, g);
      add_i64(w, prefix + "/connectivity/" + block(t, g),
              {static_cast<std::size_t>(c.size()),
               static_cast<std::size_t>(c.getNbComponent())},
              {"element", "node_local"}, to_i64(c), "elemental");
      blocks.push(block(t, g));
    }
  }
  w.parameters()[prefix + "/blocks"] = blocks;
}

void record_input(ap::FixtureWriter & w, const Mesh & mesh) {
  const auto & X = mesh.getNodes();
  std::vector<double> x(X.data(), X.data() + X.size() * X.getNbComponent());
  w.add("input/nodes",
        {static_cast<std::size_t>(X.size()), static_cast<std::size_t>(X.getNbComponent())},
        {"node", "spatial_dim"}, "nodal", x)
      .tolerance_class("exact");
  record_connectivities(w, "input", mesh);
  ap::Json tables = ap::Json::object();
  if (mesh.hasData("physical_names")) {
    const auto & names = mesh.getData<std::string>("physical_names");
    for (auto g : ghost_types) {
      for (auto t : names.elementTypes(_all_dimensions, g, _ek_not_defined)) {
        const auto & a = names(t, g);
        std::vector<std::string> table;
        std::vector<std::int64_t> codes;
        for (Idx i = 0; i < a.size(); ++i) {
          auto it = std::find(table.begin(), table.end(), a(i));
          if (it == table.end()) {
            table.push_back(a(i));
            it = table.end() - 1;
          }
          codes.push_back(it - table.begin());
        }
        tables["input/physical_names/" + block(t, g)] = ap::Json::array_of(table);
        add_i64(w, "input/physical_names/" + block(t, g), {codes.size()}, {"element"},
                codes, "elemental");
      }
    }
  }
  w.parameters()["string_tables"] = tables;
  w.parameters()["bounds_lower"] = ap::Json::array_of(std::vector<double>(
      mesh.getLowerBounds().data(), mesh.getLowerBounds().data() + mesh.getSpatialDimension()));
  w.parameters()["bounds_upper"] = ap::Json::array_of(std::vector<double>(
      mesh.getUpperBounds().data(), mesh.getUpperBounds().data() + mesh.getSpatialDimension()));
}

void record_e2s(ap::FixtureWriter & w, TypeTable & tt, const std::string & prefix,
                const Mesh & mesh) {
  if (not mesh.hasData("element_to_subelement")) {
    return;
  }
  const auto & data = mesh.getData<std::vector<Element>>("element_to_subelement");
  ap::Json blocks = ap::Json::array();
  for (auto g : ghost_types) {
    for (auto t : data.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & a = data(t, g);
      std::vector<std::int64_t> offsets{0};
      std::vector<std::int64_t> values;
      for (Idx i = 0; i < a.size(); ++i) {
        for (const auto & e : a(i)) {
          tt.encode(e, values);
        }
        offsets.push_back(static_cast<std::int64_t>(values.size() / 3));
      }
      auto n = prefix + "/element_to_subelement/" + block(t, g);
      add_i64(w, n + "/offsets", {offsets.size()}, {"entry"}, offsets);
      add_i64(w, n + "/values", {values.size() / 3, 3}, {"entry", "component"}, values);
      blocks.push(block(t, g));
    }
  }
  w.parameters()[prefix + "/element_to_subelement/blocks"] = blocks;
}

void record_s2e(ap::FixtureWriter & w, TypeTable & tt, const std::string & prefix,
                const Mesh & mesh) {
  if (not mesh.hasData("subelement_to_element")) {
    return;
  }
  const auto & data = mesh.getData<Element>("subelement_to_element");
  ap::Json blocks = ap::Json::array();
  for (auto g : ghost_types) {
    for (auto t : data.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & a = data(t, g);
      std::vector<std::int64_t> values;
      for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
        tt.encode(a.data()[i], values);
      }
      add_i64(w, prefix + "/subelement_to_element/" + block(t, g),
              {static_cast<std::size_t>(a.size()),
               static_cast<std::size_t>(a.getNbComponent()), 3},
              {"element", "facet", "component"}, values, "elemental");
      blocks.push(block(t, g));
    }
  }
  w.parameters()[prefix + "/subelement_to_element/blocks"] = blocks;
}

void record_element_data(ap::FixtureWriter & w, TypeTable & tt, const std::string & prefix,
                         const Mesh & mesh, const std::string & name) {
  if (not mesh.hasData(name)) {
    return;
  }
  const auto & data = mesh.getData<Element>(name);
  ap::Json blocks = ap::Json::array();
  for (auto g : ghost_types) {
    for (auto t : data.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & a = data(t, g);
      std::vector<std::int64_t> values;
      for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
        tt.encode(a.data()[i], values);
      }
      add_i64(w, prefix + "/" + name + "/" + block(t, g),
              {static_cast<std::size_t>(a.size()), 3}, {"element", "component"}, values,
              "elemental");
      blocks.push(block(t, g));
    }
  }
  w.parameters()[prefix + "/" + name + "/blocks"] = blocks;
}

void record_strings(ap::FixtureWriter & w, const std::string & prefix, const Mesh & mesh,
                    const std::string & name) {
  if (not mesh.hasData(name)) {
    return;
  }
  const auto & data = mesh.getData<std::string>(name);
  auto & tables = w.parameters()["string_tables"];
  for (auto g : ghost_types) {
    for (auto t : data.elementTypes(_all_dimensions, g, _ek_not_defined)) {
      const auto & a = data(t, g);
      std::vector<std::string> table;
      std::vector<std::int64_t> codes;
      for (Idx i = 0; i < a.size(); ++i) {
        auto it = std::find(table.begin(), table.end(), a(i));
        if (it == table.end()) {
          table.push_back(a(i));
          it = table.end() - 1;
        }
        codes.push_back(it - table.begin());
      }
      auto q = prefix + "/" + name + "/" + block(t, g);
      tables[q] = ap::Json::array_of(table);
      add_i64(w, q, {codes.size()}, {"element"}, codes, "elemental");
    }
  }
}

/// Every element group (elements in stored order, both ghost types) and
/// node group of `mesh`, names and dimensions in iteration order.
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
        add_i64(w, prefix + "/element_groups/" + safe(group.getName()) + "/" + block(t, gt),
                {static_cast<std::size_t>(els.size())}, {"group_member"}, to_i64(els));
        types.push(block(t, gt));
      }
    }
    g["blocks"] = types;
    g["node_group"] = group.getNodeGroup().getName();
    const auto & nodes = group.getNodeGroup().getNodes();
    add_i64(w, prefix + "/element_groups/" + safe(group.getName()) + "/nodes",
            {static_cast<std::size_t>(nodes.size())}, {"group_member"}, to_i64(nodes));
    eg.push(g);
  }
  w.parameters()[prefix + "/element_groups"] = eg;
  ap::Json ng = ap::Json::array();
  for (const auto & group : mesh.iterateNodeGroups()) {
    ng.push(group.getName());
  }
  w.parameters()[prefix + "/node_groups"] = ng;
}

void record_csr(ap::FixtureWriter & w, TypeTable & tt, const std::string & prefix,
                CSR<Element> & csr) { // non-const: upstream const getRow does not compile
  std::vector<std::int64_t> offsets{0};
  std::vector<std::int64_t> values;
  for (Idx r = 0; r < csr.getNbRows(); ++r) {
    for (const auto & e : csr.getRow(r)) {
      tt.encode(e, values);
    }
    offsets.push_back(static_cast<std::int64_t>(values.size() / 3));
  }
  add_i64(w, prefix + "/offsets", {offsets.size()}, {"entry"}, offsets);
  add_i64(w, prefix + "/values", {values.size() / 3, 3}, {"entry", "component"}, values);
}

/// Generated meshes exercising element kinds the reference test meshes do
/// not contain (cohesive elements between doubled nodes, mixed kinds).
void generate(Mesh & mesh, const std::string & name) {
  MeshAccessor acc(mesh);
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
  if (name == "cohesive_1d") {
    // 0 --s0-- 1 | 2 --s1-- 3, cohesive point pair (1, 2)
    push({0.});
    push({1.});
    push({1.});
    push({2.});
    conn(_segment_2, {0, 1});
    conn(_segment_2, {2, 3});
    conn(_cohesive_1d_2, {1, 2});
  } else if (name == "cohesive_2d") {
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
  } else if (name == "cohesive_3d") {
    // two tetrahedra sharing the face x = 0 (doubled nodes 1,2,3 / 5,6,7)
    push({-1., 0., 0.});
    push({0., 0., 0.});
    push({0., 1., 0.});
    push({0., 0., 1.});
    push({1., 0., 0.});
    push({0., 0., 0.});
    push({0., 1., 0.});
    push({0., 0., 1.});
    conn(_tetrahedron_4, {0, 1, 2, 3});
    conn(_tetrahedron_4, {4, 6, 5, 7});
    conn(_cohesive_3d_6, {1, 2, 3, 5, 6, 7});
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
  } else {
    throw std::runtime_error("unknown generated mesh " + name);
  }
  acc.makeReady();
}

struct EveryThird : public GroupManager::ClusteringFilter {
  bool operator()(const Element & el) const override { return el.element % 3 != 0; }
};

void load(Mesh & mesh, const fs::path & file, const std::string & gen) {
  if (gen.empty()) {
    mesh.read(file.string());
  } else {
    generate(mesh, gen);
  }
}

void probe(const fs::path & root, const fs::path & up_root, const fs::path & fz_root,
           const std::string & spec) {
  auto kv = parse_spec(spec);
  const auto cid = "mesh_utils/" + kv.at("case");
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
  } else if (where == "in") {
    file = fs::path(PROBE_INPUTS_DIR) / rel;
  } else {
    throw std::runtime_error("bad file prefix in " + file_spec);
  }
  const Int dim = std::stoi(kv.count("dim") ? kv["dim"] : "3");
  std::set<std::string> ops;
  {
    std::stringstream ss(kv.count("ops") ? kv["ops"] : "");
    std::string op;
    while (std::getline(ss, op, ',')) {
      ops.insert(op);
    }
  }

  ap::FixtureWriter w(root, cid,
                      "Mesh utilities on " + file_spec + " (dimension " +
                          std::to_string(dim) + "): " +
                          (kv.count("ops") ? kv["ops"] : std::string("")),
                      kProbe, kProbeVersion);
  w.add_tag("mesh_utils");
  if (gen.empty()) {
    w.add_input_file("mesh", file,
                     (where == "up"   ? std::string("upstream:")
                      : where == "in" ? std::string("probe-input:oracle/probes/mesh_utils/inputs/")
                                      : std::string("frozen:fixtures/meshes/gmsh-4.13.1/")) +
                         rel);
  }
  if (kv.count("verified")) {
    w.add_input_file("verified", up_root / kv["verified"], "upstream:" + kv["verified"]);
  }
  auto & p = w.parameters();
  p["source"] = file_spec;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["ops"] = kv.count("ops") ? kv["ops"] : std::string("");
  TypeTable tt;

  Mesh mesh(dim);
  if (where == "in") {
    // scratch copy: the Diana reader writes `material_<name>` next to it
    const auto scratch = fs::temp_directory_path() / ("mesh_utils_probe_in_" + kv.at("case"));
    fs::remove_all(scratch);
    fs::create_directories(scratch.parent_path());
    fs::create_directories(scratch);
    fs::copy_file(file, scratch / file.filename());
    const auto old_cwd = fs::current_path();
    fs::current_path(scratch);
    load(mesh, file.filename(), gen);
    fs::current_path(old_cwd);
    fs::remove_all(scratch);
  } else {
    load(mesh, file, gen);
  }
  record_input(w, mesh);
  // what Mesh::read leaves (fillElementToSubElementsData)
  record_e2s(w, tt, "read", mesh);
  record_s2e(w, tt, "read", mesh);
  if (ops.count("iterators")) {
    record_groups(w, "read", mesh);
  }

  if (ops.count("csr")) {
    CSR<Element> csr;
    MeshUtils::buildNode2Elements(mesh, csr);
    record_csr(w, tt, "csr", csr);
    auto first = *mesh.elementTypes(dim, _not_ghost, _ek_not_defined).begin();
    CSR<Idx> csr_t;
    MeshUtils::buildNode2ElementsElementTypeMap(mesh, csr_t, first);
    std::vector<std::int64_t> offsets{0};
    std::vector<std::int64_t> values;
    for (Idx r = 0; r < csr_t.getNbRows(); ++r) {
      for (auto e : csr_t.getRow(r)) {
        values.push_back(e);
      }
      offsets.push_back(static_cast<std::int64_t>(values.size()));
    }
    p["csr_type/type"] = type_name(first);
    add_i64(w, "csr_type/offsets", {offsets.size()}, {"entry"}, offsets);
    add_i64(w, "csr_type/values", {values.size()}, {"entry"}, values);
    mesh.fillNodesToElements();
    std::vector<std::int64_t> s_off{0};
    std::vector<std::int64_t> s_val;
    for (Idx n = 0; n < mesh.getNbNodes(); ++n) {
      for (const auto & e : mesh.getAssociatedElements(n)) {
        tt.encode(e, s_val);
      }
      s_off.push_back(static_cast<std::int64_t>(s_val.size() / 3));
    }
    add_i64(w, "nodes_to_elements/offsets", {s_off.size()}, {"entry"}, s_off);
    add_i64(w, "nodes_to_elements/values", {s_val.size() / 3, 3}, {"entry", "component"},
            s_val);
  }

  if (ops.count("facets")) {
    const auto & mf = mesh.initMeshFacets();
    record_connectivities(w, "facets", mf);
    record_e2s(w, tt, "facets", mf);
    record_s2e(w, tt, "facets", mf);
    record_strings(w, "facets", mf, "physical_names");
    record_groups(w, "facets", mf);
    record_element_data(w, tt, "parent", mesh, "mesh_to_mesh_facet");
  }

  if (ops.count("buildfacets")) {
    Mesh m2(dim, "m2");
    load(m2, file, gen);
    MeshUtils::buildFacets(m2);
    record_connectivities(w, "build_facets", m2);
    record_e2s(w, tt, "build_facets", m2);
    record_s2e(w, tt, "build_facets", m2);
    if (ops.count("boundary")) {
      auto nb = m2.createBoundaryGroupFromGeometry();
      p["boundary/nb_clusters"] = static_cast<long long>(nb);
      record_groups(w, "boundary", m2);
    }
  } else if (ops.count("boundary")) {
    Mesh m2(dim, "m2");
    load(m2, file, gen);
    auto nb = m2.createBoundaryGroupFromGeometry();
    p["boundary/nb_clusters"] = static_cast<long long>(nb);
    record_groups(w, "boundary", m2);
  }

  if (ops.count("clusters")) {
    Mesh m3(dim, "m3");
    load(m3, file, gen);
    auto nb = m3.createClusters(dim, "cluster", EveryThird());
    p["clusters/nb_clusters"] = static_cast<long long>(nb);
    p["clusters/filter"] = "element index % 3 != 0";
    record_groups(w, "clusters", m3);
  }

  if (ops.count("purify")) {
    Mesh m4(dim, "m4");
    load(m4, file, gen);
    MeshUtils::purifyMesh(m4);
    const auto & X = m4.getNodes();
    std::vector<double> x(X.data(), X.data() + X.size() * X.getNbComponent());
    w.add("purify/nodes",
          {static_cast<std::size_t>(X.size()), static_cast<std::size_t>(X.getNbComponent())},
          {"node", "spatial_dim"}, "nodal", x)
        .tolerance_class("exact");
    record_connectivities(w, "purify", m4);
  }

  if (ops.count("erase")) {
    Mesh m5(dim, "m5");
    load(m5, file, gen);
    Array<Element> to_erase;
    for (auto t : m5.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
      auto nb = m5.getNbElement(t);
      // every third element and the last one
      for (Idx e = 0; e < nb; e += 3) {
        to_erase.push_back(Element{t, e, _not_ghost});
      }
      if (nb > 0 and (nb - 1) % 3 != 0) {
        to_erase.push_back(Element{t, nb - 1, _not_ghost});
      }
    }
    std::vector<std::int64_t> list;
    for (const auto & e : to_erase) {
      tt.encode(e, list);
    }
    add_i64(w, "erase/elements", {list.size() / 3, 3}, {"entry", "component"}, list);
    m5.eraseElements(to_erase);
    record_connectivities(w, "erase", m5);
  }

  if (ops.count("periodic") or ops.count("periodic_groups")) {
    const bool by_groups = ops.count("periodic_groups") != 0;
    const std::string dirs = kv.count("periodic") ? kv["periodic"] : "x";
    Mesh m6(dim, "m6");
    load(m6, file, gen);
    for (char c : dirs) {
      auto d = c == 'x' ? _x : (c == 'y' ? _y : _z);
      if (by_groups) {
        std::string a = std::string(1, c) + "-minus";
        std::string b = std::string(1, c) + "-plus";
        m6.makePeriodic(d, a, b);
      } else {
        m6.makePeriodic(d);
      }
    }
    p["periodic/directions"] = dirs;
    p["periodic/is_periodic"] = m6.isPeriodic();
    std::vector<std::pair<Idx, Idx>> pairs;
    for (const auto & [m, s] : m6.getPeriodicMasterSlaves()) {
      pairs.emplace_back(m, s);
    }
    std::sort(pairs.begin(), pairs.end());
    std::vector<std::int64_t> pv;
    for (const auto & [m, s] : pairs) {
      pv.push_back(m);
      pv.push_back(s);
    }
    add_i64(w, "periodic/pairs", {pairs.size(), 2}, {"entry", "component"}, pv);
    std::vector<std::pair<Idx, Idx>> sm;
    for (const auto & [s, m] : m6.getPeriodicSlaveMaster()) {
      sm.emplace_back(s, m);
    }
    std::sort(sm.begin(), sm.end());
    std::vector<std::int64_t> smv;
    for (const auto & [s, m] : sm) {
      smv.push_back(s);
      smv.push_back(m);
    }
    add_i64(w, "periodic/slave_master", {sm.size(), 2}, {"entry", "component"}, smv);
    std::vector<std::int64_t> flags;
    for (Idx n = 0; n < m6.getNbNodes(); ++n) {
      flags.push_back(static_cast<std::int64_t>(static_cast<std::uint8_t>(m6.getNodeFlag(n))));
    }
    add_i64(w, "periodic/flags", {flags.size()}, {"node"}, flags, "nodal");
    // upstream test_mesh_periodic MakeConsistentForPeriodicitySum: masters
    // hold 100 / 2^shared, _id then _sum must give 100 on local nodes
    Array<Real> shared(m6.getNbNodes(), 1, 0.);
    for (char c : dirs) {
      for (const auto & g : {std::string(1, c) + "-minus", std::string(1, c) + "-plus"}) {
        if (not m6.elementGroupExists(g)) {
          continue;
        }
        for (auto node : m6.getElementGroup(g).getNodeGroup().getNodes()) {
          shared(node) += 1.;
        }
      }
    }
    Array<Real> values(m6.getNbNodes(), 1, 100.);
    std::set<Idx> masters;
    for (const auto & ms : m6.getPeriodicMasterSlaves()) {
      masters.insert(ms.first);
    }
    for (auto master : masters) {
      if (m6.isLocalOrMasterNode(master)) {
        values(master) = 100. / std::pow(2, shared(master));
      }
    }
    std::vector<double> before(values.data(), values.data() + values.size());
    w.add("periodic/sum_input", {before.size()}, {"node"}, "nodal", before)
        .tolerance_class("exact");
    m6.makeConsistentForPeriodicity(values, ReduceOperation::_id);
    std::vector<double> after_id(values.data(), values.data() + values.size());
    w.add("periodic/after_id", {after_id.size()}, {"node"}, "nodal", after_id)
        .tolerance_class("exact");
    m6.makeConsistentForPeriodicity(values, ReduceOperation::_sum);
    std::vector<double> after_sum(values.data(), values.data() + values.size());
    w.add("periodic/after_sum", {after_sum.size()}, {"node"}, "nodal", after_sum)
        .tolerance_class("exact");
  }

  p["type_table"] = ap::Json::array_of(tt.names);
  w.finish();
  std::cout << "  " << cid << "\n";
}

} // namespace

int main(int argc, char ** argv) {
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
      std::cerr << "mesh_utils_probe: " << argv[i] << " failed: " << e.what() << "\n";
      ++failures;
    }
  }
  finalize();
  return failures == 0 ? 0 : 1;
}
