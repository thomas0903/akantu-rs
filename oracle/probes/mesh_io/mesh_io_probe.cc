// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) for the mesh readers/writers beyond plain `Mesh::read` of a .msh
// file (which oracle/probes/cpp/mesh_probe.cc covers): explicit MeshIOType
// (`_miot_gmsh_struct`, `_miot_diana`), nodal and real elemental mesh data
// (`$NodeData`, `$ElementData`, `$ElementNodeData`), the text written by
// `Mesh::write`, the Diana material side file, and reads that fail.
//
// Usage:
//   mesh_io_probe <fixtures_root> <upstream_src_root> <probe_inputs_root>
//                 <frozen_meshes_root> <spec>...
// spec: semicolon-separated key=value list
//   case=<case id below io/mesh_io/>   (required)
//   file=up:<path below the reference root> | in:<path below the inputs root>
//        | fz:<path below the frozen Gmsh meshes root>
//   dim=<1|2|3>  type=<auto|gmsh|gmsh_struct|diana>  write  cwd
// `write` also records the output of Mesh::write (input role
// "oracle_written"); `cwd` copies the input into a scratch directory and
// reads it by its bare name from there (the Diana reader writes
// `material_<name>` next to it, recorded as role "oracle_material_file").

#include "akantu_probe/fixture_writer.hh"

#include <aka_common.hh>
#include <element_group.hh>
#include <mesh.hh>
#include <mesh_data.hh>
#include <node_group.hh>

#include <cstdint>
#include <cstdlib>
#include <cxxabi.h>
#include <filesystem>
#include <iostream>
#include <map>
#include <sstream>
#include <string>
#include <typeinfo>
#include <vector>
// After the Akantu headers: D-050 Eigen cache-size pin check.
#include "akantu_probe/eigen_pin.hh"

using namespace akantu;
namespace ap = akantu_probe;
namespace fs = std::filesystem;

namespace {

constexpr const char * kProbe = "mesh_io_probe";
constexpr const char * kProbeVersion = "1";

std::string type_name(ElementType t) {
  std::stringstream ss;
  ss << t;
  return ss.str();
}

std::string safe_name(const std::string & n) {
  std::string out;
  for (char c : n) {
    out += (std::isspace(static_cast<unsigned char>(c)) or c == '/') ? '_' : c;
  }
  return out.empty() ? std::string("_") : out;
}

std::string demangle(const char * n) {
  int status = 0;
  char * d = abi::__cxa_demangle(n, nullptr, nullptr, &status);
  std::string out = (status == 0 and d != nullptr) ? d : n;
  std::free(d); // NOLINT
  return out;
}

std::string first_line(const std::string & s) {
  auto p = s.find('\n');
  auto l = s.substr(0, p);
  return l.size() > 160 ? l.substr(0, 160) : l;
}

template <class T> std::vector<std::int64_t> to_i64(const Array<T> & a) {
  std::vector<std::int64_t> v;
  for (Idx i = 0; i < a.size() * a.getNbComponent(); ++i) {
    v.push_back(static_cast<std::int64_t>(a.data()[i]));
  }
  return v;
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

MeshIOType io_type(const std::string & t) {
  if (t == "gmsh") {
    return _miot_gmsh;
  }
  if (t == "gmsh_struct") {
    return _miot_gmsh_struct;
  }
  if (t == "diana") {
    return _miot_diana;
  }
  return _miot_auto;
}

void record_mesh(ap::FixtureWriter & w, const Mesh & mesh) {
  const auto dim = mesh.getSpatialDimension();
  auto & p = w.parameters();
  p["nb_nodes"] = static_cast<long long>(mesh.getNbNodes());
  const auto & nodes = mesh.getNodes();
  std::vector<double> X(nodes.data(), nodes.data() + nodes.size() * nodes.getNbComponent());
  w.add("nodes",
        {static_cast<std::size_t>(nodes.size()),
         static_cast<std::size_t>(nodes.getNbComponent())},
        {"node", "spatial_dim"}, "nodal", X)
      .tolerance_class("exact");

  ap::Json types = ap::Json::array();
  ap::Json string_tables = ap::Json::object();
  ap::Json skipped = ap::Json::array();
  ap::Json nodal_flags = ap::Json::object();
  const MeshData & md = mesh;
  for (auto type : mesh.elementTypes(_all_dimensions, _not_ghost, _ek_not_defined)) {
    const auto tn = type_name(type);
    const auto & conn = mesh.getConnectivity(type, _not_ghost);
    ap::Json t = ap::Json::object();
    t["type"] = tn;
    t["ghost_type"] = "not_ghost";
    t["nb_elements"] = static_cast<long long>(conn.size());
    types.push(t);
    w.add("connectivity/" + tn,
          {static_cast<std::size_t>(conn.size()),
           static_cast<std::size_t>(conn.getNbComponent())},
          {"element", "node_local"}, "elemental", to_i64(conn))
        .element_type(tn)
        .ghost_type("not_ghost")
        .tolerance_class("exact");
    for (auto && name : md.getTagNames(type, _not_ghost)) {
      const auto qname = "mesh_data/" + safe_name(name) + "/" + tn;
      switch (md.getTypeCode(name)) {
      case MeshDataTypeCode::_int: {
        const auto & a = md.getElementalDataArray<Int>(name, type, _not_ghost);
        w.add(qname,
              {static_cast<std::size_t>(a.size()),
               static_cast<std::size_t>(a.getNbComponent())},
              {"element", "component"}, "elemental", to_i64(a))
            .element_type(tn)
            .ghost_type("not_ghost")
            .tolerance_class("exact");
        break;
      }
      case MeshDataTypeCode::_real: {
        const auto & a = md.getElementalDataArray<Real>(name, type, _not_ghost);
        std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
        w.add(qname,
              {static_cast<std::size_t>(a.size()),
               static_cast<std::size_t>(a.getNbComponent())},
              {"element", "component"}, "elemental", v)
            .element_type(tn)
            .ghost_type("not_ghost")
            .tolerance_class("exact");
        nodal_flags[name] = md.getElementalData<Real>(name).isNodal();
        break;
      }
      case MeshDataTypeCode::_std_string: {
        const auto & a = md.getElementalDataArray<std::string>(name, type, _not_ghost);
        std::map<std::string, std::int64_t> index;
        std::vector<std::string> table;
        std::vector<std::int64_t> codes;
        for (Idx i = 0; i < a.size(); ++i) {
          auto it = index.find(a(i));
          if (it == index.end()) {
            it = index.emplace(a(i), static_cast<std::int64_t>(table.size())).first;
            table.push_back(a(i));
          }
          codes.push_back(it->second);
        }
        string_tables[name + "/" + tn] = ap::Json::array_of(table);
        w.add(qname, {codes.size()}, {"element"}, "elemental", codes)
            .element_type(tn)
            .ghost_type("not_ghost")
            .tolerance_class("exact");
        break;
      }
      default:
        skipped.push(name + "/" + tn);
      }
    }
  }
  p["element_types"] = types;
  p["string_tables"] = string_tables;
  p["skipped_mesh_data"] = skipped;
  p["elemental_real_is_nodal"] = nodal_flags;

  // Nodal data (MeshData::getTagNames() lists the nodal datasets).
  ap::Json nodal = ap::Json::array();
  for (auto && name : md.getTagNames()) {
    if (md.getTypeCode(name, MeshDataType::_nodal) != MeshDataTypeCode::_real) {
      continue;
    }
    const auto & a = md.getNodalData<Real>(name);
    std::vector<double> v(a.data(), a.data() + a.size() * a.getNbComponent());
    w.add("nodal_data/" + safe_name(name),
          {static_cast<std::size_t>(a.size()), static_cast<std::size_t>(a.getNbComponent())},
          {"node", "component"}, "nodal", v)
        .tolerance_class("exact");
    nodal.push(name);
  }
  p["nodal_data"] = nodal;

  ap::Json ngroups = ap::Json::array();
  for (auto & group : mesh.iterateNodeGroups()) {
    ap::Json ng = ap::Json::object();
    ng["name"] = group.getName();
    ngroups.push(ng);
    w.add("node_groups/" + safe_name(group.getName()),
          {static_cast<std::size_t>(group.getNodes().size())}, {"group_member"}, "table",
          to_i64(group.getNodes()))
        .ordering("group_member", "unordered")
        .tolerance_class("exact");
  }
  p["node_groups"] = ngroups;
  ap::Json egroups = ap::Json::array();
  for (auto & group : mesh.iterateElementGroups()) {
    ap::Json g = ap::Json::object();
    g["name"] = group.getName();
    g["dimension"] = static_cast<long long>(group.getDimension());
    ap::Json gt = ap::Json::array();
    for (auto ghost : ghost_types) {
      for (auto type : group.elementTypes(_all_dimensions, ghost, _ek_not_defined)) {
        const auto & els = group.getElements(type, ghost);
        const auto suffix = type_name(type) + (ghost == _not_ghost ? "" : "/ghost");
        gt.push(suffix);
        w.add("element_groups/" + safe_name(group.getName()) + "/" + suffix,
              {static_cast<std::size_t>(els.size())}, {"group_member"}, "table", to_i64(els))
            .element_type(type_name(type))
            .ghost_type(ghost == _not_ghost ? "not_ghost" : "ghost")
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
  (void)dim;
}

void probe(const fs::path & root, const fs::path & up_root, const fs::path & in_root,
           const fs::path & fz_root, const std::string & spec) {
  auto kv = parse_spec(spec);
  const auto cid = "io/mesh_io/" + kv.at("case");
  const auto file_spec = kv.at("file");
  const auto colon = file_spec.find(':');
  const auto where = file_spec.substr(0, colon);
  const auto rel = file_spec.substr(colon + 1);
  const fs::path base = where == "up" ? up_root : where == "fz" ? fz_root : in_root;
  if (where != "up" and where != "in" and where != "fz") {
    throw std::runtime_error("bad file prefix in " + file_spec);
  }
  const fs::path file = base / rel;
  const Int dim = std::stoi(kv.count("dim") ? kv["dim"] : "3");
  const auto type = kv.count("type") ? kv["type"] : "auto";

  ap::FixtureWriter w(root, cid,
                      "Mesh::read(" + rel + ", " + type + ") in dimension " +
                          std::to_string(dim) + (kv.count("write") ? " + Mesh::write" : ""),
                      kProbe, kProbeVersion);
  w.add_tag("mesh_io");
  w.add_input_file("mesh", file,
                   (where == "up"   ? "upstream:"
                    : where == "fz" ? "frozen:fixtures/meshes/gmsh-4.13.1/"
                                    : "probe-input:oracle/probes/mesh_io/inputs/") +
                       rel);
  auto & p = w.parameters();
  p["source"] = file_spec;
  p["spatial_dimension"] = static_cast<long long>(dim);
  p["mesh_io_type"] = type;

  fs::path scratch = fs::temp_directory_path() /
                     ("mesh_io_probe_" + std::to_string(std::hash<std::string>{}(cid)));
  fs::remove_all(scratch);
  fs::create_directories(scratch);
  fs::path read_path = file;
  const auto old_cwd = fs::current_path();
  if (kv.count("cwd")) {
    fs::copy_file(file, scratch / file.filename());
    fs::current_path(scratch);
    read_path = file.filename();
  }

  Mesh mesh(dim);
  bool ok = true;
  try {
    mesh.read(read_path.string(), io_type(type));
  } catch (std::exception & e) {
    ok = false;
    w.expect_error(demangle(typeid(e).name()), first_line(e.what()));
    p["read_error"] = std::string(e.what());
  }
  fs::current_path(old_cwd);
  if (ok) {
    record_mesh(w, mesh);
    if (kv.count("cwd")) {
      auto mat = scratch / ("material_" + file.filename().string());
      if (fs::exists(mat)) {
        w.add_input_file("oracle_material_file", mat, "written by the Diana reader");
      }
    }
    if (kv.count("write")) {
      auto out = scratch / "oracle_written.msh";
      try {
        mesh.write(out.string(), io_type(type == "auto" ? "gmsh" : type));
        w.add_input_file("oracle_written", out, "Mesh::write output");
      } catch (std::exception & e) {
        p["write_error"] = std::string(e.what());
        p["write_error_kind"] = demangle(typeid(e).name());
      }
    }
  }
  fs::remove_all(scratch);
  w.finish();
  std::cout << "  " << cid << (ok ? "" : " (read error)") << "\n";
}

} // namespace

int main(int argc, char ** argv) {
  if (argc < 6) {
    std::cerr << "usage: " << argv[0]
              << " <fixtures_root> <upstream_root> <inputs_root> <frozen_root> <spec>...\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  int failures = 0;
  for (int i = 5; i < argc; ++i) {
    try {
      probe(argv[1], argv[2], argv[3], argv[4], argv[i]);
    } catch (std::exception & e) {
      std::cerr << "mesh_io_probe: " << argv[i] << " failed: " << e.what() << "\n";
      ++failures;
    }
  }
  finalize();
  return failures == 0 ? 0 : 1;
}
