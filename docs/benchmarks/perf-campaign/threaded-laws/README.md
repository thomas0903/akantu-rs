<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# More constitutive laws, the heat model and the phase-field model on threads

Branch `perf/threaded-laws` (from `perf/wave2-base` c9eef15b8). Every
result keeps every bit at every thread count (D-122, D-123): the suite
digests of the 48 cases with a digest (`suite_solid`, `suite_models`, size
`s`) are the same for the base and the head at 1 and 4 threads
(`digests/`), and every timed run below at size `m` (5 repetitions, 1 to
8 threads) has its case's single digest (column "digest" of the tables in
`series/`).

## What changed

- Laws that declare `Material::independent_points()` and run their point
  loops on the model's executor (`material::points_exec`): orthotropic and
  anisotropic elasticity, neo-Hookean (stress and plane-stress `C33`),
  linear isotropic hardening plasticity, Drucker–Prager, Mazars (both
  parents), Marigo, the local loop of the non-local Mazars and Marigo laws,
  anisotropic damage (experimental and corrected models), Maxwell, the
  standard linear solid, the phase-field solid law (stress and tangent),
  and in the extra materials `visco_plastic`, `damage_linear` (per-point
  thresholds) and `ve_stiffness_prop`. The per-step hooks that run every
  step go with them (plastic work, dissipated energy of the damage laws,
  Maxwell branch update and energies, thermal refresh). The list with the
  reason each law is independent, and why the others stay serial, is in
  `docs/ARCHITECTURE.md`, "Execution policy".
- Error path: the state after a failing point equals the serial loop's at
  every thread count. Laws that compute on copies of their blocks write
  them back only when every point succeeded; neo-Hookean finds the first
  point with `J ≤ 0` with a read-only pass of the kernel's own predicate,
  runs the points before it on the threads and the rest serially; the
  plane-stress `C33` solve writes a copy and commits the points before the
  first failure.
- Heat transfer model: `exec_policy` / `set_exec_policy` (FE engines and
  DOF manager), the heat law's diffusivity and `D∇u` loops on the
  executor, the internal flow scattered to the nodes through a cached
  `NodeScatter` (the serial order per node). Phase-field model:
  `exec_policy` / `set_exec_policy` (its FE kernels on threads).
- One-thread path: the Maxwell Voigt helpers `inline(always)` and its
  point kernels `inline`; the phase-field and SLS stress loops in part
  functions with slice parameters (the closure version had lost the
  inlining of the point kernels: Maxwell 4.5 % slower, SLS kernel 17 %
  of the step against 15 %, phase-field internal forces 6.8 % slower in
  the first series; `profiles/head-896d0f46`, `profiles/head-73e10205`).

## Correctness evidence

- Thread-invariance tests (every nodal array, every internal field of
  every law with its previous values, the energies; 1, 2, 3, 4 and 8
  threads, finest and automatic grain; 2-D triangles and quadrangles, 3-D
  tetrahedra and hexahedra): `crates/akantu-solid/tests/exec_threads.rs`
  (one test per law, error-path tests for Mazars, neo-Hookean and its
  plane-stress variant and anisotropic damage: a NaN displacement at the
  centre node, the error message and the state compared),
  `crates/akantu-extra-materials/tests/exec_threads.rs`,
  `crates/akantu-diffusion/tests/exec_threads_explicit.rs` (explicit heat
  step, constant and temperature-dependent conductivity),
  `crates/akantu-couplers/tests/exec_threads_phase_field.rs` (staggered
  solid / phase-field step).
- Mutants (`mutants.py`, results in `mutants.log`): a mutant per threaded
  loop that makes a point depend on its predecessor (or its position) in
  its part, and one that drops the neo-Hookean pre-check; each must make
  its test fail. All are caught. The first SLS mutant survived because the
  dissipated energy stayed zero in that test's configuration (relaxation
  time far below the time step); the case now uses `Eta = 3e5` and checks
  that the dissipated energy moves, and the mutant is caught.
- Suite digests at size `s`, base and head, 1 and 4 threads:
  `digests/size-s-af38c786.txt` (48 cases, all equal).

## Measurements

Machine: Google Cloud, AMD EPYC 9B45 (Zen 5), 8 cores, one thread per
core, Ubuntu 24.04. Interleaved, rotated series (`scripts/perf/suite/run_ab.sh`),
size `m`, 5 repetitions, one process per run; one thread pinned to core 2,
2 threads to cores 2–3, 4 to 0–3, 8 to 0–7 (`AKANTU_NUM_THREADS` and
`--threads`). Base: `perf/wave2-base` c9eef15b8 at one thread. Head:
af38c786d. Ratios: median [min, max] of the per-repetition ratios to the
base at one thread. Raw data and full tables:
`series/final-af38c786/{solid,models}{/,.md}` (command, date and binaries
in the `meta.*.json` files); the first series (head 896d0f46, before the
one-thread fixes) in `series/first-896d0f46`, the one-thread check of the
fixes in `series/one-thread-73e10205`.

| case | phase | base, 1 thread (s) | head, 1 thread | 2 threads | 4 threads | 8 threads |
|---|---|---|---|---|---|---|
| `explicit_marigo` | solve_step | 2.296 s [2.255 s, 2.440 s] | 0.982 [0.966, 0.993] | 1.018 [0.967, 1.038] | 0.897 [0.843, 0.921] | 0.856 [0.834, 0.893] |
| `explicit_maxwell` | solve_step | 1.600 s [1.524 s, 1.627 s] | 0.987 [0.972, 1.026] | 0.823 [0.819, 0.861] | 0.754 [0.732, 0.779] | 0.720 [0.705, 0.750] |
| `explicit_mazars` | solve_step | 2.940 s [2.878 s, 2.990 s] | 0.988 [0.965, 1.001] | 0.552 [0.543, 0.555] | 0.355 [0.352, 0.363] | 0.252 [0.249, 0.260] |
| `explicit_mazars_non_local` | solve_step | 4.171 s [4.149 s, 4.351 s] | 0.998 [0.992, 1.009] | 0.953 [0.946, 0.958] | 0.932 [0.926, 0.938] | 0.919 [0.912, 0.929] |
| `explicit_multi_material` | solve_step | 2.590 s [2.537 s, 2.684 s] | 0.994 [0.964, 1.005] | 0.582 [0.568, 0.593] | 0.395 [0.383, 0.402] | 0.303 [0.297, 0.309] |
| `explicit_neohookean` | solve_step | 1.413 s [1.382 s, 1.449 s] | 0.945 [0.916, 0.963] | 0.597 [0.579, 0.609] | 0.394 [0.385, 0.397] | 0.317 [0.304, 0.318] |
| `explicit_orthotropic` | solve_step | 1.378 s [1.374 s, 1.446 s] | 1.017 [1.014, 1.026] | 0.710 [0.685, 0.714] | 0.572 [0.557, 0.590] | 0.519 [0.514, 0.531] |
| `explicit_plastic` | solve_step | 2.725 s [2.661 s, 2.912 s] | 1.006 [0.993, 1.021] | 0.862 [0.789, 0.865] | 0.765 [0.724, 0.792] | 0.744 [0.711, 0.762] |
| `explicit_sls` | solve_step | 2.580 s [2.464 s, 2.688 s] | 1.018 [1.001, 1.067] | 0.803 [0.776, 0.832] | 0.712 [0.671, 0.728] | 0.653 [0.639, 0.698] |
| `explicit_thermal` | solve_step | 1.841 s [1.809 s, 1.919 s] | 1.004 [0.988, 1.010] | 0.710 [0.694, 0.721] | 0.565 [0.544, 0.576] | 0.487 [0.473, 0.505] |
| `heat_explicit_quad4` | solve_step | 1.262 s [1.241 s, 1.306 s] | 0.988 [0.986, 1.015] | 0.635 [0.613, 0.644] | 0.539 [0.524, 0.564] | 0.507 [0.485, 0.515] |
| `heat_explicit_tet4` | solve_step | 997.39 ms [989.05 ms, 1.006 s] | 1.005 [0.998, 1.017] | 0.594 [0.588, 0.627] | 0.414 [0.408, 0.422] | 0.361 [0.358, 0.368] |
| `phasefield_staggered_tet4` | solid_solve | 1.299 s [1.294 s, 1.313 s] | 0.998 [0.991, 1.002] | 0.999 [0.996, 1.007] | 1.000 [0.989, 1.005] | 1.006 [0.996, 1.023] |
| `phasefield_staggered_tet4` | phase_solve | 75.72 ms [75.30 ms, 76.83 ms] | 1.007 [0.998, 1.013] | 1.000 [0.990, 1.009] | 0.988 [0.975, 0.999] | 0.994 [0.983, 1.018] |
| `phasefield_staggered_tet4` | internal_forces | 1.42 ms [1.41 ms, 1.50 ms] | 0.993 [0.943, 1.001] | 0.632 [0.614, 0.649] | 0.567 [0.533, 0.572] | 0.554 [0.532, 0.574] |
| `phasefield_staggered_tri3` | solid_solve | 6.733 s [6.723 s, 6.768 s] | 1.004 [1.000, 1.016] | 0.993 [0.992, 1.002] | 1.002 [0.996, 1.008] | 1.002 [0.997, 1.013] |
| `phasefield_staggered_tri3` | phase_solve | 1.218 s [1.213 s, 1.227 s] | 1.005 [1.001, 1.020] | 0.982 [0.976, 0.997] | 1.012 [1.008, 1.015] | 0.996 [0.987, 1.018] |
| `phasefield_staggered_tri3` | internal_forces | 7.34 ms [7.33 ms, 7.39 ms] | 1.020 [1.004, 1.054] | 0.564 [0.555, 0.600] | 0.419 [0.417, 0.427] | 0.447 [0.443, 0.457] |

Readings:

- One thread: every case within about 2 % of the base. Two medians are
  above 1 with a range that excludes it: `explicit_orthotropic` 1.017
  [1.014, 1.026] and the phase-field tri3 internal forces 1.020 [1.004,
  1.054] (4 calls of 7 ms). The orthotropic law's code is the same in both
  series and measured 0.982 [0.973, 1.002] in the first one; its kernel
  symbol takes 15.0 % (base) and 15.8 % (head) of the profile
  (`profiles/*/explicit_orthotropic.t1.flat.txt`), most likely a
  code-layout effect of the other changes in the binary. `explicit_sls`
  1.018 [1.001, 1.067]: its kernel share equals the base's after the fix
  (10.3 % against 10.1 %); the spread comes from the page faults of its
  per-step allocations.
- Scaling: laws whose per-point work dominates scale to 0.25–0.32 of the
  base at 8 threads (Mazars, neo-Hookean, three laws in one mesh). The
  others are bound by serial work in the calling thread: at 8 threads the
  workers spin while the caller copies the law's blocks before the loop and
  back after it, and allocates and frees per-step temporaries (page faults
  in the kernel: `do_user_addr_fault`, `zap_present_ptes`, the page
  allocator; `profiles/final-af38c786/*.t8.main.txt`). This is the case of
  Marigo (0.86), plastic (0.74), Maxwell (0.72) and SLS (0.65); the per-step
  state work that keeps these blocks between steps removes most of it.
  Marigo also computes its potential energy densities serially every step
  (`elastic_damage_energy_densities`). Non-local Mazars: the neighbourhood
  average, serial, dominates (0.92).
- Heat explicit step: 0.36 (tet4) and 0.51 (quad4) at 8 threads; the FE
  kernels (`fast::btd`, `fast::gradient`, `fast::integrate`) and the law's
  `D∇u` are on threads; what stays serial is the first-order
  predictor / corrector of the solver crate and the memory traffic of the
  quad4 case (160 801 nodes, one value each).
- Phase field: the staggered step is the two sparse direct solves (serial
  sparse LU, unchanged); the solid model's internal forces (the
  phase-field law's stress) reach 0.45–0.55 at 4–8 threads.
