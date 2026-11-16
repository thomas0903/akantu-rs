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

FULL_SCHEDULE_PLACEHOLDER

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

SCALING_PLACEHOLDER

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
4. The MUMPS path's static solve (1.29× upstream with the same MUMPS): the
   port's assembly and hand-over around the same factorisation.
5. Peak RSS of the host driver (1.72× upstream at h = 0.12, mostly the
   SparseLU factorisation's storage).

Fast-mode candidates (results change): FMA in the explicit kernels and the
SparseLU dense kernel; a vectorised `sparselu_gemm` with another reduction
order; on this CPU no gain is expected from the C++ default install's SSE2
packets (they are slower than the scalar code here).
