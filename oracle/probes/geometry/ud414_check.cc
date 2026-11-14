// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs. Oracle check (not a port) for UD-414: built against the
// sphere-intersector header with ONLY the compile fixes of
// oracle/patches/geometry/0001-*.patch, it shows that upstream's
// constructData() leaves new_node_per_elem empty (Array::clear() =
// resize(0)) and that the first new node then trips the bounds assertion.
// Prints one line: "size_after_constructData=<n> exception=<0|1> what=<msg>".
#include <aka_common.hh>
#include <mesh.hh>

#include "mesh_geom_common.hh"
#include "mesh_sphere_intersector.hh"

#include <iostream>
#include <list>

using namespace akantu;

int main(int argc, char * argv[]) {
  if (argc != 2) {
    std::cerr << "usage: ud414_check <test_geometry_triangle.msh>\n";
    return 2;
  }
  int fake_argc = 1;
  char ** fake_argv = argv;
  initialize(fake_argc, fake_argv);
  debug::setDebugLevel(dblError);
  Mesh mesh(2);
  mesh.read(argv[1]);
  MeshSphereIntersector<2, _triangle_3> si(mesh);
  si.constructData();
  const auto n = si.getNewNodePerElem().size();
  int exc = 0;
  std::string what;
  try {
    std::list<cgal::Spherical::Sphere_3> l;
    l.emplace_back(cgal::Spherical::Point_3(0, 1, 0), 0.2 * 0.2);
    si.computeMeshQueryListIntersectionPoint(l, mesh.getNbNodes());
  } catch (std::exception & e) {
    exc = 1;
    what = e.what();
    for (auto & c : what)
      if (c == '\n')
        c = ' ';
  }
  std::cout << "size_after_constructData=" << n << " exception=" << exc << " what=" << what.substr(0, 200)
            << std::endl;
  finalize();
  return 0;
}
