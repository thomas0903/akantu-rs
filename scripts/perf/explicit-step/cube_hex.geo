// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs project tooling (not ported from Akantu): explicit-step benchmark
// mesh, unit cube of n x n x n _hexahedron_8. gmsh -3 -setnumber n N
DefineConstant[ n = {27, Name "n"} ];
General.NumThreads = 1;
Point(1) = {0, 0, 0, 1};
l[] = Extrude {1, 0, 0} { Point{1}; Layers{n}; };
s[] = Extrude {0, 1, 0} { Line{l[1]}; Layers{n}; Recombine; };
v[] = Extrude {0, 0, 1} { Surface{s[1]}; Layers{n}; Recombine; };
Physical Volume("body") = {v[1]};
