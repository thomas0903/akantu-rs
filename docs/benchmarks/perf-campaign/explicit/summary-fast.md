rounds: 5; orders: after after-fast; after-fast after; after after-fast; after-fast after; after after-fast
cpu: AMD EPYC 7B13; load at round start: 1.90, 1.64, 1.50, 1.36, 1.28
first round 2026-11-16T16:43:23Z, last round 2026-11-16T16:44:34Z

| mesh | type | elements | mode | phase | after | after-fast | after-fast / after (paired) |
|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | step | 63 [63, 64] | 59 [59, 60] | **0.94** [0.93, 0.95] |
| tet4-20k | tetrahedron_4 | 19,120 | ss | fint | 50 [50, 51] | 47 [46, 47] | **0.93** [0.92, 0.93] |
| tet4-20k | tetrahedron_4 | 19,120 | ss | stress | 20 [20, 20] | 16 [15, 16] | **0.78** [0.77, 0.79] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | step | 83 [82, 83] | 78 [77, 78] | **0.94** [0.94, 0.94] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | fint | 67 [67, 68] | 62 [62, 63] | **0.93** [0.92, 0.93] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | stress | 23 [23, 24] | 19 [19, 19] | **0.83** [0.81, 0.83] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | step | 82 [81, 82] | 78 [78, 78] | **0.95** [0.95, 0.96] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | fint | 60 [60, 61] | 57 [57, 58] | **0.94** [0.94, 0.95] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | stress | 29 [28, 29] | 24 [24, 25] | **0.83** [0.82, 0.87] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | step | 109 [109, 109] | 102 [100, 102] | **0.93** [0.92, 0.93] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | fint | 77 [77, 77] | 72 [71, 72] | **0.93** [0.93, 0.94] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | stress | 31 [31, 31] | 26 [26, 26] | **0.84** [0.83, 0.85] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | step | 335 [332, 338] | 332 [331, 335] | **0.99** [0.99, 1.00] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | fint | 235 [234, 236] | 230 [230, 232] | **0.98** [0.98, 0.98] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | stress | 130 [128, 148] | 135 [134, 150] | **1.06** [0.91, 1.15] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | step | 481 [477, 485] | 463 [461, 465] | **0.96** [0.96, 0.97] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | fint | 353 [351, 355] | 334 [333, 339] | **0.95** [0.94, 0.96] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | stress | 141 [138, 145] | 152 [147, 160] | **1.07** [1.05, 1.14] |
| hex8-20k | hexahedron_8 | 19,683 | ss | step | 471 [468, 473] | 446 [445, 448] | **0.95** [0.94, 0.95] |
| hex8-20k | hexahedron_8 | 19,683 | ss | fint | 328 [327, 330] | 302 [301, 303] | **0.92** [0.92, 0.93] |
| hex8-20k | hexahedron_8 | 19,683 | ss | stress | 176 [173, 182] | 215 [208, 216] | **1.22** [1.16, 1.23] |
| hex8-20k | hexahedron_8 | 19,683 | fd | step | 741 [740, 743] | 647 [642, 648] | **0.87** [0.86, 0.88] |
| hex8-20k | hexahedron_8 | 19,683 | fd | fint | 504 [503, 508] | 443 [442, 446] | **0.88** [0.87, 0.88] |
| hex8-20k | hexahedron_8 | 19,683 | fd | stress | 195 [192, 203] | 240 [225, 244] | **1.20** [1.15, 1.27] |
| hex8-200k | hexahedron_8 | 195,112 | ss | step | 448 [443, 458] | 423 [417, 439] | **0.94** [0.92, 0.99] |
| hex8-200k | hexahedron_8 | 195,112 | ss | fint | 339 [337, 348] | 314 [312, 316] | **0.92** [0.91, 0.94] |
| hex8-200k | hexahedron_8 | 195,112 | ss | stress | 202 [196, 208] | 242 [238, 247] | **1.21** [1.14, 1.23] |
| hex8-200k | hexahedron_8 | 195,112 | fd | step | 715 [711, 728] | 599 [598, 609] | **0.84** [0.83, 0.85] |
| hex8-200k | hexahedron_8 | 195,112 | fd | fint | 581 [578, 584] | 470 [465, 477] | **0.81** [0.80, 0.82] |
| hex8-200k | hexahedron_8 | 195,112 | fd | stress | 223 [219, 229] | 270 [258, 283] | **1.21** [1.13, 1.27] |

step fd after-fast/after (median of paired ratios per case): 0.84–0.96 (n=5)
step ss after-fast/after (median of paired ratios per case): 0.94–0.99 (n=5)
