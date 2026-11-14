// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port): records what Akantu's Mesh::read (src/io/mesh_io/mesh_io_msh.cc)
// produces for a mesh file: nodes, connectivities per element type, node
// and element groups, elemental mesh data (tags, physical names).
//
// Usage:
//   mesh_probe <fixtures_root> <reference_src_root> <relpath[:dim]> ...
// e.g. mesh_probe fixtures /akantu-src test/test_fe_engine/_triangle_3.msh
// Case id: mesh_io/<relpath without leading "test/" and ".msh">.
// Without ":dim" the spatial dimension is the largest element dimension
// found in the file (the mesh is read a first time with dimension 3).

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_data.hh>
#include <node_group.hh>

#include <cstdint>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;

namespace {

constexpr const char * kProbe = "mesh_probe";
constexpr const char * kProbeVersion = "1";

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

/// Group names come from Gmsh physical names and may contain spaces or
/// slashes; quantity names must not (the original names are kept in the
/// manifest parameters).
std::string safe_name(const std::string & n) {
  std::string out;
  for (char c : n) {
    out += (std::isspace(static_cast<unsigned char>(c)) or c == '/') ? '_' : c;
  }
  return out.empty() ? std::string("_") : out;
}

std::string ghost_name(GhostType g) { return g == _not_ghost ? "not_ghost" : "ghost"; }

std::string case_id_for(std::string rel) {
  if (rel.rfind("test/", 0) == 0) {
    rel = rel.substr(5);
  }
  if (rel.size() > 4 and rel.substr(rel.size() - 4) == ".msh") {
    rel = rel.substr(0, rel.size() - 4);
  }
  std::string out;
  for (char c : rel) {
    out += (std::isalnum(static_cast<unsigned char>(c)) or c == '_' or c == '-' or
            c == '.' or c == '+' or c == '/')
               ? c
               : '_';
  }
  return "mesh_io/" + out;
}

template <class T> std::vector<std::int64_t> to_i64(const Array<T> & a) {
  std::vector<std::int64_t> v;
  v.reserve(static_cast<std::size_t>(a.size() * a.getNbComponent()));
  for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
    v.push_back(static_cast<std::int64_t>(a.data()[i]));
  }
  return v;
}

Int max_element_dimension(const std::filesystem::path & file) {
  Mesh probe_mesh(3, "dim_probe");
  probe_mesh.read(file.string());
  Int dim = 0;
  for (auto type : probe_mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    dim = std::max(dim, Mesh::getSpatialDimension(type));
  }
  return std::max<Int>(dim, 1);
}

void probe_mesh(const std::filesystem::path & root, const std::filesystem::path & src_root,
                const std::string & spec) {
  auto colon = spec.rfind(':');
  std::string rel = spec;
  Int dim = -1;
  if (colon != std::string::npos) {
    rel = spec.substr(0, colon);
    dim = std::stoi(spec.substr(colon + 1));
  }
  const auto file = src_root / rel;
  const bool auto_dim = dim < 0;
  if (auto_dim) {
    dim = max_element_dimension(file);
  }

  Mesh mesh(dim);
  mesh.read(file.string());

  const auto cid = case_id_for(rel) + (auto_dim ? "" : "_dim" + std::to_string(dim));
  ap::FixtureWriter w(root, cid,
                      "Mesh::read of upstream mesh " + rel + " (spatial dimension " +
                          std::to_string(dim) + ")",
                      kProbe, kProbeVersion);
  w.add_tag("mesh_io");
  w.add_input_file("mesh", file, "upstream:" + rel);
  auto & p = w.parameters();
  p["source"] = rel;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["spatial_dimension_from"] = auto_dim ? "largest element dimension" : "explicit";
  p["nb_nodes"] = static_cast<long long>(mesh.getNbNodes());

  // Nodes.
  const auto & nodes = mesh.getNodes();
  std::vector<double> X(nodes.data(), nodes.data() + nodes.size() * nodes.getNbComponent());
  w.add("nodes", {static_cast<std::size_t>(nodes.size()),
                  static_cast<std::size_t>(nodes.getNbComponent())},
        {"node", "spatial_dim"}, "nodal", X)
      .description("node coordinates in file order")
      .tolerance_class("exact");
  std::vector<double> lo(mesh.getLowerBounds().data(),
                         mesh.getLowerBounds().data() + dim);
  std::vector<double> hi(mesh.getUpperBounds().data(),
                         mesh.getUpperBounds().data() + dim);
  w.add("bounds/lower", {static_cast<std::size_t>(dim)}, {"spatial_dim"}, "table", lo)
      .tolerance_class("exact");
  w.add("bounds/upper", {static_cast<std::size_t>(dim)}, {"spatial_dim"}, "table", hi)
      .tolerance_class("exact");

  // Connectivities and elemental mesh data per type.
  ap::Json types = ap::Json::array();
  ap::Json string_tables = ap::Json::object();
  ap::Json skipped = ap::Json::array();
  const MeshData & mesh_data = mesh; // Mesh derives from MeshData
  for (auto ghost : ghost_types) {
    for (auto type : mesh.elementTypes(_all_dimensions, ghost, _ek_not_defined)) {
      const auto tn = type_name(type);
      const auto suffix = tn + (ghost == _not_ghost ? "" : "/ghost");
      const auto & conn = mesh.getConnectivity(type, ghost);
      ap::Json t = ap::Json::object();
      t["type"] = tn;
      t["ghost_type"] = ghost_name(ghost);
      t["nb_elements"] = static_cast<long long>(conn.size());
      types.push(t);
      w.add("connectivity/" + suffix,
            {static_cast<std::size_t>(conn.size()),
             static_cast<std::size_t>(conn.getNbComponent())},
            {"element", "node_local"}, "elemental", to_i64(conn))
          .element_type(tn)
          .ghost_type(ghost_name(ghost))
          .tolerance_class("exact");

      for (auto && name : mesh_data.getTagNames(type, ghost)) {
        const auto code = mesh_data.getTypeCode(name);
        const auto qname = "mesh_data/" + safe_name(name) + "/" + suffix;
        auto add_int = [&](auto tag) {
          using T = decltype(tag);
          const auto & a = mesh_data.template getElementalDataArray<T>(name, type, ghost);
          w.add(qname,
                {static_cast<std::size_t>(a.size()),
                 static_cast<std::size_t>(a.getNbComponent())},
                {"element", "component"}, "elemental", to_i64(a))
              .element_type(tn)
              .ghost_type(ghost_name(ghost))
              .tolerance_class("exact");
        };
        switch (code) {
        case MeshDataTypeCode::_int:
          add_int(Int{});
          break;
        case MeshDataTypeCode::_uint:
          add_int(UInt{});
          break;
        case MeshDataTypeCode::_bool:
          add_int(bool{});
          break;
        case MeshDataTypeCode::_real: {
          const auto & a = mesh_data.getElementalDataArray<Real>(name, type, ghost);
          std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
          w.add(qname,
                {static_cast<std::size_t>(a.size()),
                 static_cast<std::size_t>(a.getNbComponent())},
                {"element", "component"}, "elemental", v)
              .element_type(tn)
              .ghost_type(ghost_name(ghost))
              .tolerance_class("exact");
          break;
        }
        case MeshDataTypeCode::_std_string: {
          // Strings are stored as codes into a per-name string table kept in
          // the manifest parameters (npy has no portable string dtype here).
          const auto & a =
              mesh_data.getElementalDataArray<std::string>(name, type, ghost);
          std::map<std::string, std::int64_t> index;
          std::vector<std::string> table;
          std::vector<std::int64_t> codes;
          for (Idx i = 0; i < a.size(); ++i) {
            const auto & s = a(i);
            auto it = index.find(s);
            if (it == index.end()) {
              it = index.emplace(s, static_cast<std::int64_t>(table.size())).first;
              table.push_back(s);
            }
            codes.push_back(it->second);
          }
          string_tables[name + "/" + suffix] = ap::Json::array_of(table);
          w.add(qname, {codes.size()}, {"element"}, "elemental", codes)
              .element_type(tn)
              .ghost_type(ghost_name(ghost))
              .description("codes into parameters.string_tables[\"" + name + "/" +
                           suffix + "\"]")
              .tolerance_class("exact");
          break;
        }
        default:
          skipped.push(name + "/" + suffix);
          break;
        }
      }
    }
  }
  p["element_types"] = types;
  if (not string_tables.is_null()) {
    p["string_tables"] = string_tables;
  }
  if (not skipped.is_null()) {
    p["skipped_mesh_data"] = skipped;
  }

  // Node groups (member order as stored by Akantu).
  ap::Json ngroups = ap::Json::array();
  for (auto & group : mesh.iterateNodeGroups()) {
    const auto & n = group.getNodes();
    ap::Json ng = ap::Json::object();
    ng["name"] = group.getName();
    ng["quantity"] = "node_groups/" + safe_name(group.getName());
    ngroups.push(ng);
    w.add("node_groups/" + safe_name(group.getName()), {static_cast<std::size_t>(n.size())},
          {"group_member"}, "table", to_i64(n))
        .ordering("group_member", "unordered")
        .tolerance_class("exact");
  }
  p["node_groups"] = ngroups;

  // Element groups: element ids per type, plus the group's node set.
  ap::Json egroups = ap::Json::array();
  for (auto & group : mesh.iterateElementGroups()) {
    ap::Json g = ap::Json::object();
    g["name"] = group.getName();
    g["quantity_prefix"] = "element_groups/" + safe_name(group.getName());
    g["dimension"] = static_cast<long long>(group.getDimension());
    ap::Json gt = ap::Json::array();
    for (auto ghost : ghost_types) {
      for (auto type : group.elementTypes(_all_dimensions, ghost, _ek_not_defined)) {
        const auto & els = group.getElements(type, ghost);
        const auto suffix = type_name(type) + (ghost == _not_ghost ? "" : "/ghost");
        gt.push(suffix);
        w.add("element_groups/" + safe_name(group.getName()) + "/" + suffix,
              {static_cast<std::size_t>(els.size())}, {"group_member"}, "table",
              to_i64(els))
            .element_type(type_name(type))
            .ghost_type(ghost_name(ghost))
            .ordering("group_member", "unordered")
            .tolerance_class("exact");
      }
    }
    g["types"] = gt;
    const auto & gn = group.getNodeGroup().getNodes();
    w.add("element_groups/" + safe_name(group.getName()) + "/nodes",
          {static_cast<std::size_t>(gn.size())}, {"group_member"}, "table", to_i64(gn))
        .ordering("group_member", "unordered")
        .tolerance_class("exact");
    egroups.push(g);
  }
  p["element_groups"] = egroups;
  w.finish();
  std::cout << "  " << cid << "\n";
}

} // namespace

int main(int argc, char ** argv) {
  if (argc < 4) {
    std::cerr << "usage: " << argv[0]
              << " <fixtures_root> <reference_src_root> <relpath[:dim]>...\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  int failures = 0;
  for (int i = 3; i < argc; ++i) {
    try {
      probe_mesh(argv[1], argv[2], argv[i]);
    } catch (std::exception & e) {
      std::cerr << "mesh_probe: " << argv[i] << " failed: " << e.what() << "\n";
      ++failures;
    }
  }
  finalize();
  return failures == 0 ? 0 : 1;
}
