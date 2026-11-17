# Final campaign, part A: the explicit step on one core

**Dry run on a development machine: the smallest sizes and 2 repetitions, a check of the campaign's chain from the build to the tables. These numbers are not results and are not comparable with the published tables.**

- Machine: AMD EPYC 7B13, 8 cores, kernel 7.0.0-1017-gcp.
- Sessions (UTC): 2026-11-17T06:39:20Z.
- Revisions: C++ reference `35a851780d0b`, port before `b6f56c2f0b24`, port head `7301353f76df`.
- Size `dry`, repetitions requested: 2. Provenance: `provenance.txt`; every command: `commands.log`; progress: `progress.log`.

Groups: 3 planned, 3 complete and checked (only these are tabulated).

Rounds used: 2 of 2 planned; program order rotated per round: cpp-strict cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8; cpp-strict cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8.

Table "Explicit step (`solve_step`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 165 [164, 166] | 251 [250, 251] | 80.9 [80.4, 81.4] | 57.6 [57.6, 57.6] | 0.35 [0.35, 0.35] | 0.23 [0.23, 0.23] | 0.71 [0.71, 0.72] | 0.49 [0.49, 0.49] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 967 [908, 1,026] | 1,339 [1,337, 1,342] | 464 [463, 465] | 288 [287, 289] | 0.30 [0.28, 0.32] | 0.21 [0.21, 0.22] | 0.62 [0.62, 0.62] | 0.48 [0.45, 0.51] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 1,338 [1,337, 1,340] | 1,969 [1,967, 1,971] | 636 [635, 637] | 370 [370, 370] | 0.28 [0.28, 0.28] | 0.19 [0.19, 0.19] | 0.58 [0.58, 0.58] | 0.48 [0.47, 0.48] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 388 [388, 389] | 459 [457, 460] | 122 [122, 123] | 74.7 [74.6, 74.9] | 0.19 [0.19, 0.19] | 0.16 [0.16, 0.16] | 0.61 [0.61, 0.61] | 0.32 [0.32, 0.32] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 2,764 [2,754, 2,774] | 2,981 [2,979, 2,982] | 836 [834, 837] | 408 [408, 408] | 0.15 [0.15, 0.15] | 0.14 [0.14, 0.14] | 0.49 [0.49, 0.49] | 0.30 [0.30, 0.30] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 4,498 [4,497, 4,498] | 4,950 [4,943, 4,956] | 1,511 [1,505, 1,516] | 615 [615, 616] | 0.14 [0.14, 0.14] | 0.12 [0.12, 0.12] | 0.41 [0.41, 0.41] | 0.34 [0.33, 0.34] |

Table "Internal forces (`fint`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 143 [143, 143] | 228 [226, 229] | 68.1 [67.8, 68.4] | 50.9 [50.9, 51.0] | 0.36 [0.36, 0.36] | 0.22 [0.22, 0.23] | 0.75 [0.74, 0.75] | 0.48 [0.48, 0.48] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 754 [740, 767] | 1,166 [1,163, 1,169] | 361 [361, 362] | 235 [234, 235] | 0.31 [0.31, 0.32] | 0.20 [0.20, 0.20] | 0.65 [0.65, 0.65] | 0.48 [0.47, 0.49] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 1,138 [1,137, 1,140] | 1,769 [1,765, 1,773] | 484 [483, 485] | 329 [328, 330] | 0.29 [0.29, 0.29] | 0.19 [0.19, 0.19] | 0.68 [0.68, 0.68] | 0.43 [0.42, 0.43] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 362 [362, 362] | 435 [435, 435] | 106 [106, 106] | 68.0 [67.8, 68.1] | 0.19 [0.19, 0.19] | 0.16 [0.16, 0.16] | 0.64 [0.64, 0.64] | 0.29 [0.29, 0.29] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 2,539 [2,528, 2,551] | 2,828 [2,828, 2,828] | 700 [699, 701] | 352 [351, 353] | 0.14 [0.14, 0.14] | 0.12 [0.12, 0.12] | 0.50 [0.50, 0.50] | 0.28 [0.27, 0.28] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 4,152 [4,152, 4,152] | 4,636 [4,636, 4,637] | 1,303 [1,299, 1,307] | 576 [575, 576] | 0.14 [0.14, 0.14] | 0.12 [0.12, 0.12] | 0.44 [0.44, 0.44] | 0.31 [0.31, 0.31] |

Table "Stresses (`stress`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 52.4 [52.1, 52.6] | 111 [111, 112] | 28.8 [28.7, 28.9] | 20.0 [19.9, 20.0] | 0.38 [0.38, 0.38] | 0.18 [0.18, 0.18] | 0.69 [0.69, 0.70] | 0.55 [0.55, 0.55] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 307 [307, 307] | 636 [633, 639] | 165 [164, 165] | 140 [136, 144] | 0.45 [0.44, 0.47] | 0.22 [0.21, 0.23] | 0.85 [0.83, 0.87] | 0.54 [0.53, 0.54] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 488 [487, 489] | 1,042 [1,037, 1,047] | 220 [220, 220] | 186 [182, 189] | 0.38 [0.37, 0.39] | 0.18 [0.17, 0.18] | 0.84 [0.83, 0.86] | 0.45 [0.45, 0.45] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 71.3 [71.2, 71.4] | 130 [129, 131] | 34.9 [34.8, 34.9] | 23.5 [23.5, 23.6] | 0.33 [0.33, 0.33] | 0.18 [0.18, 0.18] | 0.67 [0.67, 0.68] | 0.49 [0.49, 0.49] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 374 [374, 375] | 707 [706, 707] | 189 [188, 190] | 151 [151, 152] | 0.40 [0.40, 0.41] | 0.21 [0.21, 0.21] | 0.80 [0.80, 0.81] | 0.50 [0.50, 0.51] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 674 [674, 675] | 1,229 [1,229, 1,229] | 287 [287, 287] | 208 [204, 213] | 0.31 [0.30, 0.32] | 0.17 [0.17, 0.17] | 0.72 [0.71, 0.74] | 0.43 [0.43, 0.43] |

Table "The port's step at 2, 4 and 8 threads (`AKANTU_NUM_THREADS`, cores 0..N-1) and in fast mode (`AKANTU_PRECISION=fast`)", ns per element and step; ratios paired per round; the thread count and precision each run recorded are those of the process default policy, read from the environment, checked against the request:

| mesh | mode | 1 thread | 2 | 4 | 8 | 2 / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1 thread) | fast / bitwise (8) |
|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | ss | 57.6 [57.6, 57.6] | 50.3 [34.3, 66.3] | 32.3 [26.2, 38.4] | 33.6 [32.9, 34.4] | 0.87 [0.60, 1.2] | 0.56 [0.45, 0.67] | 0.58 [0.57, 0.60] | 0.94 [0.93, 0.96] | 0.98 [0.97, 0.99] |
| tet10-20k | ss | 288 [287, 289] | 178 [177, 179] | 120 [119, 120] | 126 [126, 126] | 0.62 [0.61, 0.62] | 0.42 [0.41, 0.42] | 0.44 [0.44, 0.44] | 0.99 [0.99, 1.00] | 0.99 [0.98, 0.99] |
| hex8-20k | ss | 370 [370, 370] | 206 [206, 207] | 138 [137, 139] | 104 [101, 106] | 0.56 [0.56, 0.56] | 0.37 [0.37, 0.38] | 0.28 [0.27, 0.29] | 0.93 [0.92, 0.93] | 0.99 [0.97, 1.0] |
| tet4-20k | fd | 74.7 [74.6, 74.9] | 64.4 [44.2, 84.5] | 39.6 [32.5, 46.6] | 42.3 [41.4, 43.2] | 0.86 [0.59, 1.1] | 0.53 [0.44, 0.62] | 0.57 [0.56, 0.58] | 0.95 [0.95, 0.95] | 0.96 [0.94, 0.97] |
| tet10-20k | fd | 408 [408, 408] | 242 [241, 242] | 155 [153, 157] | 144 [142, 145] | 0.59 [0.59, 0.59] | 0.38 [0.38, 0.38] | 0.35 [0.35, 0.36] | 0.96 [0.95, 0.96] | 0.97 [0.96, 0.98] |
| hex8-20k | fd | 615 [615, 616] | 327 [325, 329] | 202 [201, 202] | 177 [146, 208] | 0.53 [0.53, 0.53] | 0.33 [0.33, 0.33] | 0.29 [0.24, 0.34] | 0.82 [0.81, 0.83] | 0.71 [0.61, 0.82] |

State digests (`digests.tsv`, FNV-1a of `u`, `v`, `a`, `f_int` after the recorded n steps): 54 program-configuration pairs, 0 differing; every bitwise program (C++ strict, port before, port at 1, 2, 4, 8 threads) equal to the recorded digest of `docs/benchmarks/explicit-step/final/digests.txt` in every configuration; fast mode not compared.

Progress log: first line 2026-11-17T06:39:22Z, last line 2026-11-17T06:40:19Z (6 lines).
