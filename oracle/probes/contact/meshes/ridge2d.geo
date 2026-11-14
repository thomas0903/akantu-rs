// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: copy of the review adversarial mesh.
// Review adversarial mesh (2-D): lower body [-1,1]x[-1,0] (8x2 quads,
// master = top line), upper body [-0.5,0.5]x[0,0.5] (8x1 quads, slave =
// bottom line). The probe bends the lower top into a ridge.
Point(1) = {-1, -1, 0, 1};
Point(2) = {1, -1, 0, 1};
Line(1) = {1, 2};
Transfinite Line{1} = 9;
s[] = Extrude {0, 1, 0} { Line{1}; Layers{2}; Recombine; };
Point(11) = {-0.5, 0, 0, 1};
Point(12) = {0.5, 0, 0, 1};
Line(11) = {11, 12};
Transfinite Line{11} = 9;
s2[] = Extrude {0, 0.5, 0} { Line{11}; Layers{1}; Recombine; };
Physical Line("master") = {s[0]};
Physical Line("slave") = {11};
Physical Surface("lower") = {s[1]};
Physical Surface("upper") = {s2[1]};
