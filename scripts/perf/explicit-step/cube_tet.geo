// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling (not ported from Akantu): explicit-step benchmark
// mesh, unit cube of unstructured tetrahedra (order 1: _tetrahedron_4,
// order 2: _tetrahedron_10). gmsh -3 -setnumber h H -setnumber order O
DefineConstant[ h = {0.05, Name "h"}, order = {1, Name "order"} ];
SetFactory("OpenCASCADE");
General.NumThreads = 1;
Mesh.MeshSizeMin = h;
Mesh.MeshSizeMax = h;
Mesh.Algorithm3D = 1;
Mesh.ElementOrder = order;
Box(1) = {0, 0, 0, 1, 1, 1};
Physical Volume("body") = {1};
