// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: copy of the review adversarial mesh.
// Review adversarial mesh (3-D; meshed with -order 1 → hexahedron_8 /
// quadrangle_4 facets, and with -order 2 → serendipity hexahedron_20 /
// quadrangle_8 for the error-path case quadratic3d):
// lower body [-1,1]^2 x [-1,0] (4x4x2, master = top face),
// upper body [-0.6,0.6]^2 x [0,0.5] (3x3x1, slave = bottom face). The probe
// bends the lower top into z = -0.15 (x^2 + y^2) and moves the upper body.
Mesh.SecondOrderIncomplete = 1;
Point(1) = {-1, -1, -1, 1};
Point(2) = {1, -1, -1, 1};
Line(1) = {1, 2};
Transfinite Line{1} = 5;
s[] = Extrude {0, 2, 0} { Line{1}; Layers{4}; Recombine; };
v[] = Extrude {0, 0, 1} { Surface{s[1]}; Layers{2}; Recombine; };
Point(11) = {-0.6, -0.6, 0, 1};
Point(12) = {0.6, -0.6, 0, 1};
Line(11) = {11, 12};
Transfinite Line{11} = 4;
s2[] = Extrude {0, 1.2, 0} { Line{11}; Layers{3}; Recombine; };
v2[] = Extrude {0, 0, 0.5} { Surface{s2[1]}; Layers{1}; Recombine; };
Physical Surface("master") = {v[0]};
Physical Surface("slave") = {s2[1]};
Physical Volume("lower") = {v[1]};
Physical Volume("upper") = {v2[1]};
