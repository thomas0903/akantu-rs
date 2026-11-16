# Performance campaign baseline (2026-11-16)

Where the port stands at `perf/optimisation` 59dd2229 against the C++
reference on the campaign's measurement machine: an AWS `m7a.2xlarge` (AMD
EPYC 9R14, 8 cores, 32 GiB), Ubuntu 24.04. Provenance (binaries, images,
digests): `provenance.txt`. Raw per-run data: `raw/`. The commands that
produced every table: `series.sh` (one command per series, resumable).

Method (`docs/BENCHMARKS.md` "Method", campaign rules section 4): release
profile, one process per run, interleaved variants with the order rotated
per repetition, medians with [min, max] and paired ratios per repetition
(median [min, max]). Serial runs pinned to core 2; the timing jobs ran on
cores 0-3 while other agents' builds used cores 4-7 (the machine's job
scheme from 06:40 UTC), except the scaling series, which had the whole
machine (one exclusive job per repetition). C++: the oracle's strict
install (`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`, the port's
floating-point operations) and, in the explicit-step table, the default
install (upstream's flags).

## Correctness on this machine

| Check | Result |
|---|---|
| Fast tier `ci/run-all.sh --backend docker` at 59dd2229 (image `akantu-rs-mpi:dev`) | lint (fmt, clippy), test (1,105 s), fixtures, parity, licence: pass; shellcheck 0.9.0 and ruff 0.17.3, missing on the host, run in containers: pass (`gate/`) |
| Explicit-step state digests, ten configurations (`explicit-digests.txt`) | port = C++ strict = the recorded digests of `docs/benchmarks/explicit-step/final/digests.txt` in every configuration; C++ default = its recorded digests |
| Static solve h = 0.12, 1 rank, 5 repetitions | port and C++ energies byte-identical with MUMPS and with SparseLU (`raw/static-h0.12/`) |
| Cohesive benchmark h = 0.06, 1 rank, MUMPS, 500 steps | per-step energies (17 digits) byte-identical, port and C++, every repetition (`raw/scaling/`) |
| Meshes | explicit-step meshes and the x86_64 cohesive meshes h = 0.12, 0.06, 0.04 reproduce their recorded SHA-256 (`meshes/`) |

The EPYC 9R14 (AVX-512) changes no bit: the machine can gate bitwise work.

## Explicit step (one core)

`scripts/perf/explicit-step/timing_rotated.sh`, 7 rounds, program order
rotated, ns per element and step of `solve_step` (whole step), median
[min, max] over the rounds; ratios paired per round. `fint` and `stress`
rows: `raw/explicit/summary.md`. Rounds 2026-11-16 07:37–08:11 UTC.

| mesh | mode | C++ strict | C++ default | port | port / C++ strict | port / C++ default | C++ default / strict |
|---|---|---|---|---|---|---|---|
| tet4-20k | ss | 150 [145, 153] | 231 [227, 236] | 71 [69, 72] | 0.47 [0.47, 0.48] | 0.30 [0.30, 0.31] | 1.55 [1.51, 1.57] |
| tet4-20k | fd | 311 [305, 318] | 398 [392, 403] | 107 [103, 112] | 0.34 [0.34, 0.35] | 0.27 [0.26, 0.28] | 1.28 [1.26, 1.29] |
| tet4-200k | ss | 169 [165, 170] | 247 [243, 249] | 94 [92, 95] | 0.56 [0.55, 0.57] | 0.38 [0.38, 0.38] | 1.47 [1.45, 1.49] |
| tet4-200k | fd | 330 [328, 334] | 413 [411, 416] | 136 [134, 141] | 0.41 [0.41, 0.42] | 0.33 [0.32, 0.34] | 1.25 [1.24, 1.26] |
| tet10-20k | ss | 844 [833, 901] | 1249 [1223, 1276] | 426 [423, 452] | 0.51 [0.48, 0.51] | 0.34 [0.34, 0.35] | 1.47 [1.39, 1.48] |
| tet10-20k | fd | 2268 [2244, 2297] | 2560 [2546, 2609] | 724 [717, 762] | 0.32 [0.32, 0.33] | 0.28 [0.28, 0.29] | 1.13 [1.12, 1.14] |
| hex8-20k | ss | 1319 [1282, 1365] | 1952 [1934, 2024] | 580 [567, 640] | 0.45 [0.43, 0.47] | 0.30 [0.29, 0.32] | 1.49 [1.46, 1.51] |
| hex8-20k | fd | 3628 [3623, 3662] | 4149 [4112, 4194] | 1096 [1090, 1142] | 0.30 [0.30, 0.31] | 0.26 [0.26, 0.27] | 1.14 [1.13, 1.15] |
| hex8-200k | ss | 1291 [1272, 1296] | 1941 [1918, 1968] | 615 [603, 630] | 0.48 [0.47, 0.49] | 0.32 [0.31, 0.32] | 1.51 [1.49, 1.54] |
| hex8-200k | fd | 3618 [3592, 3657] | 4129 [4099, 4177] | 1119 [1107, 1132] | 0.31 [0.31, 0.31] | 0.27 [0.27, 0.27] | 1.14 [1.13, 1.15] |

The spreads are 1–8 % of the medians, narrower than on the boat sandboxes
of the earlier series. On this CPU the C++ default install is slower than
the strict one (1.13–1.55): its SSE2 Eigen packets cost more than the
scalar code here.

## Cohesive driver, 1 rank, h = 0.12, 500 steps

`crates/akantu-bench/scripts/timing_ab.sh`, frozen fixture mesh (23,156
tetrahedra), static preload then 500 explicit steps (no insertion), 5
rotated repetitions; C++ strict (static solve MUMPS) against the port's host
build (no MPI: static solve with the port's SparseLU). Rank-0 Chrono
phases; `paired.py`; raw: `raw/driver-s500/`.

| material | phase | unit | C++ strict | port | port / C++ (paired) |
|---|---|---|---|---|---|
| cohesive | `read_mesh` | s | 0.0338 [0.0337, 0.0344] | 0.0189 [0.0187, 0.0190] | 0.56 [0.55, 0.56] |
| cohesive | `init_full` | s | 1.05 [1.04, 1.06] | 0.378 [0.374, 0.381] | 0.36 [0.35, 0.37] |
| cohesive | `static_solve` (different solvers) | s | 0.654 [0.621, 0.689] | 7.91 [7.89, 7.95] | 12.1 [11.5, 12.7] |
| cohesive | `solve_step` | ms/step | 8.17 [8.11, 8.25] | 3.68 [3.59, 3.71] | 0.452 [0.443, 0.453] |
| cohesive | `check_cohesive_stress` | ms/step | 21.6 [20.7, 22.1] | 14.1 [13.8, 14.1] | 0.654 [0.636, 0.668] |
| cohesive | loop | core-µs/el/step | 1.29 [1.25, 1.31] | 0.766 [0.756, 0.771] | 0.598 [0.584, 0.607] |
| cohesive | peak RSS | MiB | 271 | 465 | 1.72 |
| elastic | `init_full` | s | 1.05 [1.04, 1.06] | 0.378 [0.374, 0.379] | 0.36 [0.36, 0.37] |
| elastic | `solve_step` | ms/step | 7.88 [7.70, 7.95] | 3.17 [3.09, 3.27] | 0.405 [0.402, 0.411] |
| elastic | loop | core-µs/el/step | 0.340 [0.333, 0.343] | 0.137 [0.134, 0.141] | 0.405 [0.402, 0.411] |
| elastic | peak RSS | MiB | 271 | 465 | 1.72 |

`dist_mesh` is below 0.1 ms on both sides at one rank. The `energies`
phase is absent: the driver inputs of this series do not write energies.
The peak RSS of the port includes its SparseLU factorisation (the C++ runs
use MUMPS); the static-solve series below compares like for like.

## Cohesive driver, 1 rank, h = 0.12, full schedule (5,000 steps)

The same drivers and inputs with `max_steps = 5000` (upstream's schedule:
the shear grows every 100 steps; the first insertion at step 710).
3 rotated repetitions, one exclusive timing job each (a C++ cohesive run
takes 13 min here; 5 repetitions would have held the timing cores for
about 1.5 h). The C++ runs use upstream's behaviour of rebuilding the
node-to-element map after every insertion pass (`timing_ab.sh` runs the
harness with `AKANTU_RS_FILL_NODES_TO_ELEMENTS=upstream`, not the JOSS
emulation of the scaling series). Raw: `raw/driver-s5000/`.

| material | phase | unit | C++ strict | port | port / C++ (paired) |
|---|---|---|---|---|---|
| cohesive | `init_full` | s | 1.04 [1.04, 1.07] | 0.379 [0.375, 0.381] | 0.36 [0.35, 0.37] |
| cohesive | `static_solve` (different solvers) | s | 0.625 [0.617, 0.775] | 7.89 [7.62, 8.12] | 12.2 [10.2, 13.2] |
| cohesive | `solve_step` | ms/step | 11.0 [10.9, 11.0] | 7.88 [7.73, 7.91] | 0.718 [0.709, 0.721] |
| cohesive | `check_cohesive_stress` | ms/step | 168 [145, 183] | 21.5 [21.1, 21.6] | 0.128 [0.118, 0.146] |
| cohesive | loop | core-µs/el/step | 7.72 [6.73, 8.36] | 1.27 [1.25, 1.27] | 0.164 [0.152, 0.185] |
| cohesive | peak RSS | MiB | 298 | 471 | 1.58 |
| elastic | `solve_step` | ms/step | 7.74 [7.72, 8.01] | 3.10 [3.08, 3.10] | 0.400 [0.385, 0.402] |
| elastic | loop | core-µs/el/step | 0.334 [0.333, 0.346] | 0.134 [0.133, 0.134] | 0.400 [0.385, 0.402] |
| elastic | peak RSS | MiB | 273 | 465 | 1.70 |

Each code repeats its own insertion sequence exactly in every repetition
(C++ 11,739 cohesive elements in 3,545 insertion steps, port 11,736 in
3,547). The two sequences are not the same: the static solves (port
SparseLU, C++ MUMPS) differ in the last bits, and fragmentation amplifies
that; the regime is the same. The C++ insertion check varied with the load
of the other cores (144.8 ms/step at load 1.1 in repetition 1, 167.7 and
182.7 at load 6–7 in repetitions 2 and 3, where other agents' builds ran on
cores 4–7); the port's varied by 2 %. With insertion the port's
`solve_step` costs 0.72× upstream's, against 0.45× before insertion: its
cost per step grows from 3.68 to 7.88 ms with the cohesive elements, the
C++ one from 8.17 to 11.0 ms.

## Static solve, 1 rank, h = 0.12 (14,472 DOFs)

`c1prep_campaign.sh`, one step after the static solve, 5 repetitions, the
four cases rotated; static solutions byte-identical between the codes per
solver. Raw: `raw/static-h0.12/`.

| solver | C++ strict [s] | port [s] | port / C++ (paired) |
|---|---|---|---|
| MUMPS 5.7.3 | 0.621 [0.619, 0.951] | 0.801 [0.799, 1.41] | 1.29 [1.15, 2.27] |
| SparseLU (Eigen 3.4 in C++, the port's translation) | 2.89 [2.88, 3.66] | 7.63 [7.62, 8.64] | 2.64 [2.36, 2.64] |

C++ SparseLU / C++ MUMPS: 4.64 [3.34, 5.89]. `init_full` in the same runs:
C++ 0.968 [0.967, 1.08] s, port 0.377 [0.376, 0.415] s (0.39 [0.35, 0.43]).

## Cohesive benchmark h = 0.06 at 1, 2, 4, 8 ranks

177,328 tetrahedra (x86_64 mesh `20523d2d…`), static solve with MUMPS 5.7.3
on both sides, 500 steps (before the first insertion: none inserted in any
run), C++ strict with the JOSS `fillNodesToElements` emulation, the same
C++ Scotch partition replayed by the port at every rank count
(`partition_matches` in all 9 multi-rank C++ runs), 3 repetitions; one
exclusive job per repetition holding the four points (order reversed in
repetition 2) and the two codes rotated per point; ranks on cores
0..N-1. The full 5,000-step schedule was not run at this size: a 1-rank
C++ run of 500 steps already takes 4–5 min, the schedule's insertion phase
costs more per step, and `scaling_analyze.py` describes pre-insertion
series. Raw: `raw/scaling/` (`infra-scal-s500-summary.md` of
`scaling_analyze.py`, `infra-scal-s500-paired.md` of `paired.py`).

| ranks | phase | unit | C++ strict | port | port / C++ (paired) |
|---|---|---|---|---|---|
| 1 | `read_mesh` | s | 0.251 [0.251, 0.253] | 0.142 [0.142, 0.143] | 0.565 [0.564, 0.568] |
| 1 | `init_full` | s | 14.0 [14.0, 14.0] | 3.11 [3.11, 3.12] | 0.223 [0.222, 0.223] |
| 1 | `static_solve` | s | 13.1 [13.1, 13.1] | 14.3 [14.3, 14.4] | 1.09 [1.09, 1.10] |
| 1 | `solve_step` | ms/step | 63.6 [63.5, 63.6] | 28.2 [28.2, 28.2] | 0.444 [0.443, 0.444] |
| 1 | `check_cohesive_stress` | ms/step | 245 [245, 245] | 110 [110, 111] | 0.450 [0.449, 0.454] |
| 1 | `energies` | ms/step | 22.1 [22.1, 22.1] | 18.2 [18.2, 18.2] | 0.827 [0.825, 0.827] |
| 1 | loop | core-µs/el/step | 1.74 [1.74, 1.74] | 0.781 [0.780, 0.785] | 0.449 [0.448, 0.452] |
| 2 | `dist_mesh` | s | 1.76 [1.76, 1.78] | 0.283 [0.283, 0.284] | 0.160 [0.159, 0.161] |
| 2 | `init_full` | s | 6.29 [5.74, 6.29] | 1.71 [1.71, 1.71] | 0.273 [0.273, 0.298] |
| 2 | `static_solve` | s | 8.74 [8.73, 8.75] | 9.33 [9.32, 9.37] | 1.07 [1.07, 1.07] |
| 2 | `solve_step` | ms/step | 41.6 [41.3, 43.8] | 18.8 [18.8, 18.8] | 0.452 [0.428, 0.454] |
| 2 | `check_cohesive_stress` | ms/step | 137 [135, 137] | 58.3 [58.2, 58.5] | 0.427 [0.425, 0.431] |
| 2 | `energies` | ms/step | 11.9 [11.1, 12.2] | 9.66 [9.66, 9.71] | 0.817 [0.791, 0.874] |
| 2 | loop | core-µs/el/step | 2.01 [1.99, 2.04] | 0.869 [0.868, 0.872] | 0.432 [0.427, 0.436] |
| 4 | `dist_mesh` | s | 1.81 [1.80, 1.81] | 0.325 [0.322, 0.331] | 0.180 [0.179, 0.183] |
| 4 | `init_full` | s | 2.57 [2.57, 2.61] | 0.893 [0.892, 0.893] | 0.347 [0.343, 0.348] |
| 4 | `static_solve` | s | 4.66 [4.66, 4.66] | 4.90 [4.89, 4.95] | 1.05 [1.05, 1.06] |
| 4 | `solve_step` | ms/step | 27.8 [27.7, 28.1] | 13.4 [13.4, 13.4] | 0.484 [0.478, 0.484] |
| 4 | `check_cohesive_stress` | ms/step | 74.5 [74.2, 74.5] | 35.6 [35.6, 35.7] | 0.479 [0.478, 0.480] |
| 4 | `energies` | ms/step | 5.63 [5.52, 5.93] | 5.49 [5.46, 5.51] | 0.979 [0.921, 0.995] |
| 4 | loop | core-µs/el/step | 2.31 [2.30, 2.32] | 1.11 [1.11, 1.11] | 0.480 [0.478, 0.481] |
| 8 | `dist_mesh` | s | 1.89 [1.89, 1.89] | 0.426 [0.426, 0.432] | 0.226 [0.225, 0.229] |
| 8 | `init_full` | s | 1.17 [1.17, 1.17] | 0.473 [0.467, 0.475] | 0.404 [0.401, 0.406] |
| 8 | `static_solve` | s | 3.25 [3.11, 3.29] | 3.27 [3.26, 3.32] | 1.02 [0.99, 1.05] |
| 8 | `solve_step` | ms/step | 19.2 [18.9, 19.2] | 13.1 [13.1, 13.2] | 0.689 [0.682, 0.693] |
| 8 | `check_cohesive_stress` | ms/step | 46.8 [46.5, 46.9] | 25.7 [25.7, 25.8] | 0.551 [0.549, 0.552] |
| 8 | `energies` | ms/step | 4.10 [3.96, 4.44] | 3.69 [3.67, 3.70] | 0.903 [0.831, 0.928] |
| 8 | loop | core-µs/el/step | 2.96 [2.96, 2.98] | 1.75 [1.75, 1.76] | 0.592 [0.588, 0.592] |

`read_mesh` is the same at every rank count (rank 0 reads). Peak memory (the
container's cgroup peak, all ranks together): C++ 3.15 / 3.68 / 3.74 /
4.05 GiB, port 2.84 / 3.35 / 3.42 / 3.75 GiB at 1 / 2 / 4 / 8 ranks (port /
C++ 0.90–0.93).

Strong scaling of the loop (`solve_step` + `check_cohesive_stress`), speed-up
over 1 rank and efficiency (per repetition in brackets):

| ranks | C++ speed-up | C++ efficiency | port speed-up | port efficiency |
|---|---|---|---|---|
| 2 | 1.73 | 0.865 [0.853, 0.872] | 1.80 | 0.899 [0.895, 0.903] |
| 4 | 3.02 | 0.754 [0.752, 0.756] | 2.82 | 0.705 [0.704, 0.710] |
| 8 | 4.69 | 0.587 [0.584, 0.587] | 3.56 | 0.446 [0.444, 0.448] |

The port's `solve_step` does not get faster from 4 to 8 ranks (13.4 → 13.1
ms/step) while upstream's does (27.8 → 19.2): at 8 ranks the port's lead on
the loop shrinks from 0.48 to 0.59 of upstream's core time.

## Profile of the port's cohesive driver (h = 0.12, 500 steps)

`perf record -F 499 --call-graph dwarf` of the host driver (the run of the
table above, core 2), self time by symbol (`profile/`). The run's phases:
static solve 8.1 s, `check_cohesive_stress` 7.0 s, `solve_step` 1.8 s,
`init_full` 0.38 s of 17.1 s. The DWARF call chains did not resolve; the
general product is attributed to the insertion check from the source (its
callers in this run: the facet stress interpolation and, inside the static
solve, the finite-deformation stiffness assembly) and from the phase
balance (the SparseLU symbols account for 39 % of the samples against 47 %
for the static solve; the check's 41 % leaves 22 % beside its own symbols).

| share | symbol | phase |
|---|---|---|
| 33.6 % | `eigen_sparse_lu::dense::sparselu_gemm` | static solve (SparseLU) |
| 21.3 % | `eigen_gebp::product_into` via `akantu_fe::eigen::product_slices` | insertion check: `FeEngine::interpolate_elemental_field_shared` (the stress interpolation to the facets, a general dynamic-size product per element) |
| 8.2 % | `SolidMechanicsModelCohesive::interpolate_stress` | insertion check |
| 7.7 % | `cohesive::laws::linear::Linear::check_insertion` | insertion check |
| 3.4 % | `eigen_sparse_lu::kernels::panel_bmod` | static solve |
| 2.5 % | `finite_deformation::fast::residuals` | step |
| 2.3 % | `eigen_sparse_lu::dense::trsm_unit_lower` | static solve |
| 2.0 % | `material_tensor::s_to_cauchy::<3>` | step |
| 1.4 % | `akantu_fe::eigen::product_slices` | insertion check |
| 1.4 % | `akantu_mesh::utils::plan_facets` | set-up |
| 1.3 % | `fe::engine::fast::gradient::<Tetrahedron4>` | step |
| 1.2 % | `eigen_gebp::route` | insertion check |
| 1.0 % | `DofManager::assemble_elemental_array_local_view` | step |

## Slow paths seen (not worked on: no optimisation in this task)

Bitwise-safe candidates, by share of the profiles above:

1. The insertion check's stress interpolation (21.3 % + 1.4 % + 1.2 % of the
   driver run, about half of `check_cohesive_stress`): a fixed-size kernel
   for the tetrahedron-to-facet interpolation with the same summation order
   as the general product (`BENCHMARKS.md` "What remains", item 7).
2. The SparseLU translation's dense kernel `sparselu_gemm` (33.6 % of the
   driver run; port 2.64× upstream's SparseLU on the same matrix): a
   blocked kernel with Eigen's exact operation order (Eigen's GEBP on
   scalars), checked bitwise against the current kernel.
3. `interpolate_stress` and `check_insertion` (8.2 % and 7.7 %): per-facet
   work of the check.
4. The port's `solve_step` at 8 ranks (h = 0.06: 13.1 ms/step at 8 ranks
   against 13.4 at 4; 34 % of the 8-rank loop; loop efficiency 0.45
   against upstream's 0.59). Not profiled here: a profile of the 8-rank run
   (per-rank time in the ghost synchronisation and the per-node solver
   loops against the element kernels) comes first.
5. The MUMPS path's static solve (1.29× upstream at h = 0.12 with the same
   MUMPS, 1.02–1.09× at h = 0.06): the port's assembly and hand-over around
   the same factorisation.
6. The port's `solve_step` in the insertion regime (h = 0.12 full schedule:
   7.88 ms/step against 3.68 before insertion, 0.72× upstream against 0.45×;
   27 % of the port's loop with insertion): the cohesive elements' share of
   the step (their internal forces and the residual assembly); a profile
   of the 5,000-step run comes first.
7. The `energies` phase at 1–2 ranks (0.82–0.83× upstream, against 0.43–
   0.45× for the rest of the loop; 12 % of the port's 1-rank loop with energies at
   h = 0.06).
8. Peak RSS of the host driver (1.72× upstream at h = 0.12, mostly the
   SparseLU factorisation's storage).

Fast-mode candidates (results change): FMA in the explicit kernels and the
SparseLU dense kernel; a vectorised `sparselu_gemm` with another reduction
order; on this CPU no gain is expected from the C++ default install's SSE2
packets (they are slower than the scalar code here).
