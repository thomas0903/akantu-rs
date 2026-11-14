// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Oracle probe (not a
// port) (MUTIL-004, UD-131): root cause of upstream's
// facet construction on structural meshes (src/mesh_utils/mesh_utils.cc
// buildFacetsDimension / buildAllFacets / buildFacets,
// src/fe_engine/element_class_structural.hh getFacetTypes).
//
// For every structural element type (and two regular controls) a two-element
// mesh is built with MeshAccessor; the probe records the element-class facet
// data upstream's facet loops read (getAllFacetTypes, the facet local
// connectivity of facet type 0, nb nodes of the facet type) and then runs, each
// in a forked child so that an abort or an uncaught exception is recorded
// instead of ending the probe:
//   init_mesh_facets   Mesh::initMeshFacets() (buildAllFacets(mesh, facets, 0))
//   build_facets       MeshUtils::buildFacets(mesh) (boundary facets in place)
// A child reports success (the element types and counts of the facet mesh, or
// of the mesh after buildFacets), an akantu exception (its info, which holds
// the failing assertion and its source location) or the signal / exit code.
// Output: one JSON document on stdout.

#include <aka_common.hh>
#include <element_class.hh>
#include <mesh.hh>
#include <mesh_accessor.hh>
#include <mesh_utils.hh>

#include <sys/wait.h>
#include <unistd.h>

#include <cstdio>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace akantu;

namespace {

std::string esc(const std::string & s) {
  std::string o;
  for (char c : s) {
    switch (c) {
    case '"':
      o += "\\\"";
      break;
    case '\\':
      o += "\\\\";
      break;
    case '\n':
      o += "\\n";
      break;
    case '\t':
      o += "\\t";
      break;
    default:
      if (static_cast<unsigned char>(c) < 0x20) {
        char b[8];
        std::snprintf(b, sizeof b, "\\u%04x", c);
        o += b;
      } else {
        o += c;
      }
    }
  }
  return o;
}

/// `f()` streamed, or `"throws: <what>"` if it throws.
template <class F> std::string safe(F && f) {
  try {
    std::stringstream o;
    o << f();
    return o.str();
  } catch (debug::Exception & e) {
    return "\"throws: " + esc(e.info()) + "\"";
  } catch (std::exception & e) {
    return "\"throws: " + esc(e.what()) + "\"";
  }
}

std::string name(ElementType t) {
  std::stringstream s;
  s << t;
  return s.str();
}

struct Case {
  std::string id;
  ElementType type;
  Int dim;
  std::vector<Real> nodes;
  std::vector<Idx> conn;
};

void fill(Mesh & mesh, const Case & c) {
  MeshAccessor acc(mesh);
  auto & nodes = acc.getNodes();
  const Int nn = Int(c.nodes.size()) / c.dim;
  nodes.resize(nn);
  for (Int i = 0; i < nn * c.dim; ++i) {
    nodes.data()[i] = c.nodes[i];
  }
  auto & conn = acc.getConnectivity(c.type);
  const Int npe = Mesh::getNbNodesPerElement(c.type);
  conn.resize(Int(c.conn.size()) / npe);
  for (std::size_t i = 0; i < c.conn.size(); ++i) {
    conn.data()[i] = c.conn[i];
  }
  acc.makeReady();
}

/// Element types and counts of `m` (all dimensions, both ghost types).
std::string types_of(const Mesh & m) {
  std::stringstream o;
  o << "[";
  bool first = true;
  for (auto gt : ghost_types) {
    for (auto t : m.elementTypes(_all_dimensions, gt, _ek_not_defined)) {
      o << (first ? "" : ", ") << "{\"type\": \"" << name(t) << "\", \"ghost\": \""
        << (gt == _not_ghost ? "not_ghost" : "ghost")
        << "\", \"nb_element\": " << m.getNbElement(t, gt)
        << ", \"connectivity_components\": " << m.getConnectivity(t, gt).getNbComponent()
        << ", \"nb_nodes_per_element\": "
        << safe([&] { return Mesh::getNbNodesPerElement(t); })
        << ", \"spatial_dimension\": " << safe([&] { return Mesh::getSpatialDimension(t); })
        << "}";
      first = false;
    }
  }
  o << "]";
  return o.str();
}

/// Run `f` in a forked child; the child writes its JSON result into a pipe.
template <class F> std::string forked(F && f) {
  int fd[2];
  if (pipe(fd) != 0) {
    return "{\"outcome\": \"pipe failed\"}";
  }
  std::cout.flush();
  std::cerr.flush();
  pid_t pid = fork();
  if (pid == 0) {
    close(fd[0]);
    std::string out;
    try {
      out = "{\"outcome\": \"ok\", " + f() + "}";
    } catch (debug::Exception & e) {
      out = "{\"outcome\": \"akantu exception\", \"info\": \"" + esc(e.info()) + "\"}";
    } catch (std::exception & e) {
      out = "{\"outcome\": \"std exception\", \"what\": \"" + esc(e.what()) + "\"}";
    }
    auto n = write(fd[1], out.data(), out.size());
    (void)n;
    close(fd[1]);
    _exit(0);
  }
  close(fd[1]);
  std::string buf;
  char b[4096];
  ssize_t n;
  while ((n = read(fd[0], b, sizeof b)) > 0) {
    buf.append(b, std::size_t(n));
  }
  close(fd[0]);
  int status = 0;
  waitpid(pid, &status, 0);
  if (WIFSIGNALED(status)) {
    return "{\"outcome\": \"signal\", \"signal\": " + std::to_string(WTERMSIG(status)) +
           ", \"partial\": \"" + esc(buf) + "\"}";
  }
  if (buf.empty()) {
    return "{\"outcome\": \"no result\", \"exit\": " + std::to_string(WEXITSTATUS(status)) +
           "}";
  }
  return buf;
}

std::string facet_data(ElementType t) {
  std::stringstream o;
  auto fts = Mesh::getAllFacetTypes(t);
  o << "\"facet_types\": [";
  for (Int k = 0; k < fts.size(); ++k) {
    o << (k ? ", " : "") << "\"" << name(fts(k)) << "\"";
  }
  o << "], \"nb_nodes_of_facet_type_0\": "
    << safe([&] { return Mesh::getNbNodesPerElement(fts(0)); })
    << ", \"spatial_dimension_of_facet_type_0\": "
    << safe([&] { return Mesh::getSpatialDimension(fts(0)); });
  auto lc = Mesh::getFacetLocalConnectivity(t, 0);
  o << ", \"facet_local_connectivity_0\": {\"rows\": " << lc.rows()
    << ", \"cols\": " << lc.cols() << ", \"values\": [";
  for (Int i = 0; i < lc.rows(); ++i) {
    o << (i ? ", " : "") << "[";
    for (Int j = 0; j < lc.cols(); ++j) {
      o << (j ? ", " : "") << lc(i, j);
    }
    o << "]";
  }
  o << "]}, \"spatial_dimension_of_type\": " << Mesh::getSpatialDimension(t)
    << ", \"kind\": \"" << Mesh::getKind(t) << "\"";
  return o.str();
}

} // namespace

int main(int argc, char * argv[]) {
  initialize(argc, argv);
  const std::vector<Case> cases = {
      {"bernoulli_beam_2", _bernoulli_beam_2, 2, {0, 0, 1, 0, 2, 0.5}, {0, 1, 1, 2}},
      {"bernoulli_beam_3", _bernoulli_beam_3, 3, {0, 0, 0, 1, 0, 0, 2, 0.5, 0.25}, {0, 1, 1, 2}},
      {"discrete_kirchhoff_triangle_18",
       _discrete_kirchhoff_triangle_18,
       3,
       {0, 0, 0, 1, 0, 0, 1, 1, 0, 0, 1, 0},
       {0, 1, 2, 0, 2, 3}},
      {"control_segment_2", _segment_2, 1, {0, 1, 2}, {0, 1, 1, 2}},
      {"control_triangle_3", _triangle_3, 2, {0, 0, 1, 0, 1, 1, 0, 1}, {0, 1, 2, 0, 2, 3}},
  };
  std::cout << "{\"cases\": [";
  bool first = true;
  for (const auto & c : cases) {
    std::cout << (first ? "" : ",") << "\n {\"id\": \"" << c.id << "\", \"type\": \""
              << name(c.type) << "\", \"mesh_dimension\": " << c.dim << ", ";
    try {
      std::cout << facet_data(c.type);
    } catch (debug::Exception & e) {
      std::cout << "\"facet_data_error\": \"" << esc(e.info()) << "\"";
    } catch (std::exception & e) {
      std::cout << "\"facet_data_error\": \"" << esc(e.what()) << "\"";
    }
    first = false;
    {
      Mesh mesh(c.dim, c.id);
      fill(mesh, c);
      std::cout << ", \"mesh_types\": " << types_of(mesh);
    }
    auto init = forked([&]() {
      Mesh mesh(c.dim, c.id);
      fill(mesh, c);
      auto & facets = mesh.initMeshFacets();
      return std::string("\"facet_mesh_types\": ") + types_of(facets);
    });
    auto boundary = forked([&]() {
      Mesh mesh(c.dim, c.id);
      fill(mesh, c);
      MeshUtils::buildFacets(mesh);
      return std::string("\"mesh_types_after\": ") + types_of(mesh);
    });
    std::cout << ",\n  \"init_mesh_facets\": " << init << ",\n  \"build_facets\": " << boundary
              << "}";
  }
  std::cout << "\n]}\n";
  finalize();
  return 0;
}
