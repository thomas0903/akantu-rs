rounds: 5; orders: after i1; i1 after; after i1; i1 after; after i1
cpu: AMD EPYC 7B13; load at round start: 2.33, 2.13, 1.95, 1.74, 1.63
first round 2026-11-16T18:32:11Z, last round 2026-11-16T18:32:53Z

| mesh | type | elements | mode | phase | after | i1 | i1 / after (paired) |
|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | step | 63 [63, 64] | 65 [63, 67] | **1.02** [0.99, 1.06] |
| tet4-20k | tetrahedron_4 | 19,120 | ss | fint | 50 [50, 51] | 51 [50, 51] | **1.01** [0.99, 1.02] |
| tet4-20k | tetrahedron_4 | 19,120 | ss | stress | 20 [20, 20] | 20 [20, 21] | **1.02** [1.00, 1.02] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | step | 82 [81, 82] | 82 [82, 83] | **1.01** [1.00, 1.01] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | fint | 60 [60, 60] | 61 [60, 64] | **1.01** [1.00, 1.06] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | stress | 29 [28, 29] | 29 [29, 30] | **1.00** [1.00, 1.04] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | step | 110 [109, 110] | 110 [109, 111] | **1.00** [0.99, 1.02] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | fint | 77 [76, 77] | 77 [77, 78] | **1.00** [0.99, 1.01] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | stress | 31 [31, 31] | 31 [31, 31] | **1.00** [0.99, 1.01] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | step | 336 [332, 392] | 351 [349, 354] | **1.05** [0.89, 1.06] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | fint | 235 [234, 240] | 255 [247, 259] | **1.09** [1.03, 1.11] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | stress | 136 [128, 142] | 138 [131, 141] | **0.98** [0.94, 1.10] |
| hex8-20k | hexahedron_8 | 19,683 | ss | step | 472 [466, 475] | 469 [466, 481] | **1.01** [0.99, 1.01] |
| hex8-20k | hexahedron_8 | 19,683 | ss | fint | 327 [326, 328] | 323 [321, 333] | **0.99** [0.98, 1.02] |
| hex8-20k | hexahedron_8 | 19,683 | ss | stress | 182 [172, 184] | 181 [174, 191] | **1.01** [0.95, 1.06] |
| hex8-20k | hexahedron_8 | 19,683 | fd | step | 743 [737, 769] | 748 [738, 774] | **1.01** [0.97, 1.04] |
| hex8-20k | hexahedron_8 | 19,683 | fd | fint | 505 [504, 507] | 506 [498, 528] | **1.00** [0.99, 1.05] |
| hex8-20k | hexahedron_8 | 19,683 | fd | stress | 193 [192, 202] | 203 [192, 214] | **1.02** [1.00, 1.12] |
| hex8-200k | hexahedron_8 | 195,112 | ss | step | 449 [448, 459] | 452 [449, 455] | **1.00** [0.98, 1.01] |
| hex8-200k | hexahedron_8 | 195,112 | ss | fint | 340 [337, 349] | 343 [339, 348] | **1.01** [0.97, 1.02] |
| hex8-200k | hexahedron_8 | 195,112 | ss | stress | 209 [199, 225] | 208 [200, 212] | **1.00** [0.91, 1.05] |

step fd i1/after (median of paired ratios per case): 1.00–1.01 (n=2)
step ss i1/after (median of paired ratios per case): 1.00–1.05 (n=5)
