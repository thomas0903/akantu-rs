# Final campaign, part A: the explicit step on one core

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T04:43:50Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Rounds: 2 (program order rotated per round: cpp-strict cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8; cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8 cpp-strict).

Table "Explicit step (`solve_step`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 164 [164, 165] | 250 [249, 252] | 80.5 [80.4, 80.6] | 58.3 [58.2, 58.5] | 0.35 [0.35, 0.36] | 0.23 [0.23, 0.23] | 0.72 [0.72, 0.73] | 0.49 [0.49, 0.49] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 907 [904, 911] | 1,336 [1,333, 1,339] | 466 [464, 468] | 289 [288, 290] | 0.32 [0.32, 0.32] | 0.22 [0.22, 0.22] | 0.62 [0.62, 0.62] | 0.51 [0.51, 0.51] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 1,344 [1,344, 1,344] | 1,970 [1,963, 1,977] | 634 [632, 636] | 369 [369, 369] | 0.27 [0.27, 0.27] | 0.19 [0.19, 0.19] | 0.58 [0.58, 0.58] | 0.47 [0.47, 0.47] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 387 [386, 387] | 457 [456, 458] | 123 [123, 123] | 74.9 [74.6, 75.3] | 0.19 [0.19, 0.19] | 0.16 [0.16, 0.16] | 0.61 [0.61, 0.61] | 0.32 [0.32, 0.32] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 2,742 [2,730, 2,753] | 2,983 [2,977, 2,990] | 836 [836, 836] | 407 [407, 408] | 0.15 [0.15, 0.15] | 0.14 [0.14, 0.14] | 0.49 [0.49, 0.49] | 0.30 [0.30, 0.31] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 4,413 [4,411, 4,416] | 4,870 [4,858, 4,882] | 1,510 [1,507, 1,513] | 616 [616, 616] | 0.14 [0.14, 0.14] | 0.13 [0.13, 0.13] | 0.41 [0.41, 0.41] | 0.34 [0.34, 0.34] |

Table "Internal forces (`fint`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 143 [142, 144] | 228 [227, 228] | 67.8 [67.5, 68.2] | 50.9 [50.7, 51.0] | 0.36 [0.35, 0.36] | 0.22 [0.22, 0.22] | 0.75 [0.75, 0.75] | 0.47 [0.47, 0.48] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 740 [735, 745] | 1,163 [1,162, 1,164] | 364 [363, 364] | 236 [235, 237] | 0.32 [0.31, 0.32] | 0.20 [0.20, 0.20] | 0.65 [0.65, 0.65] | 0.49 [0.49, 0.50] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 1,139 [1,138, 1,141] | 1,768 [1,764, 1,771] | 483 [481, 485] | 331 [330, 331] | 0.29 [0.29, 0.29] | 0.19 [0.19, 0.19] | 0.68 [0.68, 0.69] | 0.42 [0.42, 0.43] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 360 [358, 362] | 434 [433, 435] | 106 [106, 107] | 67.9 [67.6, 68.2] | 0.19 [0.19, 0.19] | 0.16 [0.16, 0.16] | 0.64 [0.64, 0.64] | 0.30 [0.29, 0.30] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 2,543 [2,528, 2,559] | 2,826 [2,823, 2,828] | 697 [697, 698] | 350 [350, 350] | 0.14 [0.14, 0.14] | 0.12 [0.12, 0.12] | 0.50 [0.50, 0.50] | 0.27 [0.27, 0.28] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 4,241 [4,235, 4,247] | 4,699 [4,699, 4,699] | 1,302 [1,300, 1,304] | 575 [574, 576] | 0.14 [0.14, 0.14] | 0.12 [0.12, 0.12] | 0.44 [0.44, 0.44] | 0.31 [0.31, 0.31] |

Table "Stresses (`stress`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 52.5 [52.2, 52.8] | 112 [112, 112] | 28.9 [28.7, 29.1] | 20.4 [20.2, 20.6] | 0.39 [0.39, 0.39] | 0.18 [0.18, 0.18] | 0.71 [0.70, 0.71] | 0.55 [0.55, 0.55] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 306 [306, 307] | 635 [634, 636] | 165 [164, 165] | 137 [135, 138] | 0.45 [0.44, 0.45] | 0.22 [0.21, 0.22] | 0.83 [0.82, 0.83] | 0.54 [0.54, 0.54] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 491 [489, 493] | 1,040 [1,036, 1,045] | 219 [218, 219] | 181 [178, 184] | 0.37 [0.36, 0.37] | 0.17 [0.17, 0.18] | 0.83 [0.82, 0.84] | 0.45 [0.44, 0.45] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 70.8 [70.6, 71.0] | 131 [131, 131] | 34.8 [34.7, 34.9] | 23.5 [23.2, 23.8] | 0.33 [0.33, 0.34] | 0.18 [0.18, 0.18] | 0.68 [0.67, 0.68] | 0.49 [0.49, 0.49] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 376 [373, 379] | 708 [706, 710] | 187 [185, 188] | 158 [155, 161] | 0.42 [0.41, 0.43] | 0.22 [0.22, 0.23] | 0.85 [0.83, 0.87] | 0.50 [0.49, 0.50] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 666 [659, 673] | 1,218 [1,217, 1,220] | 288 [287, 289] | 212 [211, 213] | 0.32 [0.31, 0.32] | 0.17 [0.17, 0.17] | 0.74 [0.73, 0.74] | 0.43 [0.43, 0.44] |

Table "The port's step at 2, 4 and 8 threads (`AKANTU_NUM_THREADS`, cores 0..N-1) and in fast mode (`AKANTU_PRECISION=fast`)", ns per element and step; ratios paired per round:

| mesh | mode | 1 thread | 2 | 4 | 8 | 2 / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1 thread) | fast / bitwise (8) |
|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | ss | 58.3 [58.2, 58.5] | 34.5 [34.2, 34.9] | 25.1 [24.7, 25.6] | 33.1 [32.9, 33.4] | 0.59 [0.58, 0.60] | 0.43 [0.42, 0.44] | 0.57 [0.56, 0.57] | 0.93 [0.92, 0.93] | 0.99 [0.98, 1.00] |
| tet10-20k | ss | 289 [288, 290] | 178 [176, 181] | 121 [119, 123] | 126 [126, 126] | 0.62 [0.61, 0.62] | 0.42 [0.41, 0.43] | 0.43 [0.43, 0.44] | 0.98 [0.98, 0.99] | 1.0 [0.99, 1.0] |
| hex8-20k | ss | 369 [369, 369] | 205 [205, 205] | 145 [140, 149] | 107 [105, 109] | 0.56 [0.56, 0.56] | 0.39 [0.38, 0.40] | 0.29 [0.29, 0.30] | 0.92 [0.92, 0.92] | 0.96 [0.94, 0.97] |
| tet4-20k | fd | 74.9 [74.6, 75.3] | 44.3 [44.0, 44.6] | 32.5 [32.0, 32.9] | 41.6 [41.5, 41.7] | 0.59 [0.58, 0.60] | 0.43 [0.42, 0.44] | 0.56 [0.55, 0.56] | 0.93 [0.93, 0.94] | 0.95 [0.94, 0.97] |
| tet10-20k | fd | 407 [407, 408] | 241 [240, 242] | 154 [153, 155] | 145 [145, 146] | 0.59 [0.59, 0.59] | 0.38 [0.38, 0.38] | 0.36 [0.36, 0.36] | 0.96 [0.95, 0.96] | 0.96 [0.95, 0.96] |
| hex8-20k | fd | 616 [616, 616] | 328 [327, 328] | 203 [203, 204] | 146 [146, 146] | 0.53 [0.53, 0.53] | 0.33 [0.33, 0.33] | 0.24 [0.24, 0.24] | 0.81 [0.80, 0.81] | 0.87 [0.84, 0.91] |

State digests (`digests.tsv`, FNV-1a of `u`, `v`, `a`, `f_int` after the recorded n steps): 54 program-configuration pairs, 0 differing; every bitwise program (C++ strict, port before, port at 1, 2, 4, 8 threads) equal to the recorded digest of `docs/benchmarks/explicit-step/final/digests.txt` in every configuration; fast mode not compared.

Progress log: first line 2026-11-17T04:43:52Z, last line 2026-11-17T04:44:45Z (5 lines).
