// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: the review adversarial geometry of curved3d.geo
// without Recombine (meshed with -order 1): tetrahedron_4 volumes and
// triangle_3 facets (case adversarial_curved3d_tri).
// lower body [-1,1]^2 x [-1,0] (4x4x2 layers, master = top face),
// upper body [-0.6,0.6]^2 x [0,0.5] (3x3x1 layers, slave = bottom face). The
// probe bends the lower top into z = -0.15 (x^2 + y^2) and moves the upper
// body.
Point(1) = {-1, -1, -1, 1};
Point(2) = {1, -1, -1, 1};
Line(1) = {1, 2};
Transfinite Line{1} = 5;
s[] = Extrude {0, 2, 0} { Line{1}; Layers{4}; };
v[] = Extrude {0, 0, 1} { Surface{s[1]}; Layers{2}; };
Point(11) = {-0.6, -0.6, 0, 1};
Point(12) = {0.6, -0.6, 0, 1};
Line(11) = {11, 12};
Transfinite Line{11} = 4;
s2[] = Extrude {0, 1.2, 0} { Line{11}; Layers{3}; };
v2[] = Extrude {0, 0, 0.5} { Surface{s2[1]}; Layers{1}; };
Physical Surface("master") = {v[0]};
Physical Surface("slave") = {s2[1]};
Physical Volume("lower") = {v[1]};
Physical Volume("upper") = {v2[1]};
