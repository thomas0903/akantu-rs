<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# What still ran on one thread in the threaded explicit step

Branch `perf/threads-rest` (from `perf/wave2-base` 7a6ad8f42). The
question: when the explicit step runs on 8 threads, how much of its wall
time does the calling thread spend alone while the workers wait, in which
functions, and can that work run on the threads too, bitwise at every
thread count (D-122, D-123)?

## Method

- Profiles: `perf record --call-graph fp` (frame-pointer builds of the
  suites, `RUSTFLAGS="-C force-frame-pointers=yes"`, profiles only) of
  every `explicit_*` case of `suite_solid` and the `heat_explicit_*` and
  `phasefield_staggered_*` cases of `suite_models`, size `m`, 8 threads on
  cores 0–7, 5 s per case. A sample of the calling thread under
  `solve_step` is serial when none of its frames is in the thread pool
  (`SpinPool::run`, `exec::pool::run_parts`, `exec::spin`): the calling
  thread works alone. Its share of the calling thread's samples in the
  step is the share of the 8-thread wall time that is serial. Per case,
  the self and inclusive functions of the serial samples and the first
  akantu frame above the samples without a symbol (libc's `memset` /
  `memmove`): `profiles/base-7a6ad8f4/`, `profiles/head-85d4d74d/`,
  `profiles/head-2188f0e2/` (finite-deformation cases).
- Timings: interleaved, rotated series (`scripts/perf/suite/run_ab.sh`),
  size `m`, 5 repetitions (7 for the one-thread checks), one process per
  run; 1 thread pinned to core 2, 8 threads to cores 0–7. Ratios: median
  [min, max] of the per-repetition ratios to the base at one thread.
  Machine: Google Cloud, AMD EPYC 9B45 (Zen 5), 8 cores, one thread per
  core, Ubuntu 24.04.
- Every timed run has its case's single digest (column "digest" of the
  tables in `series/`), and the suite digests at size `s` are equal for
  the base and the head at 1, 3 and 8 threads.

## The per-equation threshold (262,144 equations before)

The solver's per-equation loops (Newmark-β predictor and corrector,
lumped solve, residual scatter, solution gather) ran on threads only from
262,144 equations, because an earlier screening had found them 4 to 21 %
slower with threads on the explicit cases. Screening with the part size
read from an environment variable (a build of the tree of b6b3f5aa3, not
committed; `series/threshold-screen*`), 8 threads, step time relative to
parts of 16,384 equations:

| case | 262,144 | 65,536 | 4,096 |
|---|---|---|---|
| `explicit_tet4_ss` | 1.121 | 1.122 | 0.975 |
| `explicit_tri3_ss` | 1.388 | 1.171 | 0.975 |
| `explicit_hex8_ss` | 1.100 | 1.009 | 1.000 |

At 2 and 4 threads parts of 4,096 equations are also the fastest (tri3:
0.713 against 0.949 for 262,144 at 4 threads; plastic 0.699 against
0.738). Wake-up of the pool is not the cause: with `AKANTU_EXEC_SPIN_US=0`
(workers parked at once) parts of 4,096 give the same gain (tet4 0.880,
tri3 0.719 against 262,144; spinning: 0.864, 0.701). The earlier losses
came from the arrays these loops read being written just before by
serial code on the calling thread (the scatter and the kernels, serial at
the time): its cache held them, and the workers fetched them from it.
With the scatter and the kernels on threads, the parts read what their
own cores wrote. The threshold is now 4,096 equations
(`integration_scheme::EQUATIONS_PER_PART`, also the DOF manager's) and
2,048 nodes for the external-work terms.

## What changed

Threaded (each with a thread-invariance test at 1, 2, 3, 4 and 8
threads, finest and automatic grain, and a mutant the test catches:
`mutants.py`, `mutants.log`: 17 mutants, all caught, one of them added
after review):

- `akantu-solver`: the per-equation loops from 4,096 equations; the
  generalized-trapezoidal predictor and corrector (heat model); the
  increment update; the schemes' store and restore copies; the blocked-DOF
  scan (per-part lists appended in part order).
- `akantu-fe`: the checks of the plans and kernels (element ids within the
  block, connectivity within the nodes, ids within the shape derivatives,
  consecutive ids, per-element rows): an integer maximum, a logical or or a
  logical and per part, then over the parts.
- `akantu-model`: the non-local weighted average from 5 threads (per
  point, its contributions in pair order from a per-point table built
  once with the pair list; below 5 threads the serial half-pair loop is
  faster, see "After review"); the deferred history save of a block reached through
  `InternalField::get_mut_with` (a copy in parts).
- `akantu-solid`: the non-local Mazars damage after the average; the
  energy densities of Marigo and Mazars; the stress block of the damage
  laws' parent kernel through `get_mut_with`; Mazars' block copies, into
  buffers kept from one step to the next with threads (fresh zeroed
  vectors of a block's size come from the heap once glibc's mmap threshold
  has grown past it, and `calloc` zeroes them on the calling thread: 6 % of
  the 8-thread Mazars step); the finite-deformation check of the material
  filter (the largest id per part; the serial scan only reports the first
  element out of range).

One thread keeps its serial code in each place (`Array::clone`, the serial
maximum, the serial scans).

## Results

Base 7a6ad8f42, head 85d4d74d (`series/final-85d4d74d/`); the
finite-deformation cases also with 2188f0e2 (`series/fd-2188f0e2/`, the
filter check). Step: phase `solve_step` (phase-field: its three phases).
Serial: share of the 8-thread wall time.

| case | base 1 thread | head 1 thread | base 8 threads | head 8 threads | serial base → head |
|---|---|---|---|---|---|
| `explicit_tet4_ss` | 1.286 s | 1.011 [1.005, 1.017] | 0.373 | 0.270 | 43.5 → 10.6 % |
| `explicit_tet4_fd` | 1.291 s | 1.007 [1.005, 1.011] | 0.267 | 0.206 | 40.0 → 9.8 % |
| `explicit_tet10_ss` | 0.993 s | 1.020 [1.007, 1.027] | 0.469 | 0.390 | 43.3 → 8.0 % |
| `explicit_tet10_fd` | 1.126 s | 1.013 [1.004, 1.018] | 0.333 | 0.280 | 41.7 → 7.6 % |
| `explicit_hex8_ss` | 1.525 s | 1.007 [1.003, 1.019] | 0.387 | 0.341 | 27.5 → 5.6 % |
| `explicit_hex8_fd` | 1.312 s | 1.009 [1.006, 1.013] | 0.272 | 0.245 | 25.9 → 6.1 % |
| `explicit_tri3_ss` | 1.508 s | 1.030 [0.810, 1.038] | 0.492 | 0.269 | 69.9 → 17.7 % |
| `explicit_tri3_fd` | 1.456 s | 1.015 [1.009, 1.017] | 0.343 | 0.203 | 65.7 → 20.6 % |
| `explicit_plastic` | 0.988 s | 0.998 [0.996, 1.005] | 0.388 | 0.350 | 22.0 → 7.0 % |
| `explicit_marigo` | 1.523 s | 1.008 [1.003, 1.011] | 0.415 | 0.322 | 44.5 → 14.5 % |
| `explicit_mazars` | 2.677 s | 1.001 [0.987, 1.008] | 0.241 | 0.213 | 33.7 → 11.5 % |
| `explicit_sls` | 0.774 s | 0.983 [0.944, 1.024] | 0.399 | 0.335 | 25.2 → 6.4 % |
| `explicit_maxwell` | 0.488 s | 1.006 [1.001, 1.009] | 0.378 | 0.347 | 21.0 → 8.9 % |
| `explicit_neohookean` | 1.059 s | 0.998 [0.992, 1.009] | 0.251 | 0.224 | 34.2 → 7.4 % |
| `explicit_orthotropic` | 1.004 s | 1.004 [0.977, 1.064] | 0.346 | 0.279 | 36.7 → 8.1 % |
| `explicit_thermal` | 1.219 s | 1.002 [1.000, 1.011] | 0.335 | 0.231 | 48.5 → 9.0 % |
| `explicit_multi_material` | 1.996 s | 1.005 [1.000, 1.043] | 0.260 | 0.228 | 29.5 → 10.0 % |
| `explicit_mazars_non_local` | 1.663 s | 0.989 [0.988, 0.991] | 0.809 | 0.367 | 94.5 → 8.5 % |
| `heat_explicit_tet4` | 0.894 s | 0.993 [0.992, 0.995] | 0.352 | 0.284 | 27.4 → 3.8 % |
| `heat_explicit_quad4` | 1.193 s | 0.998 [0.988, 1.004] | 0.485 | 0.416 | 26.7 → 5.5 % |
| `phasefield_staggered_tri3` solid / phase / forces | 1.825 s / 0.524 s / 4.80 ms | 1.002 / 0.997 / 0.993 | 0.998 / 0.955 / 0.419 | 0.982 / 0.937 / 0.372 | 99.1 → 99.0 % |
| `phasefield_staggered_tet4` solid / phase / forces | 0.325 s / 46.7 ms / 1.10 ms | 0.994 / 0.993 / 1.000 | 0.946 / 0.911 / 0.519 | 0.937 / 0.907 / 0.523 | 98.1 → 97.9 % |

With the finite-deformation filter check on threads (2188f0e2,
`series/fd-2188f0e2/`, `profiles/head-2188f0e2/`):

| case | head 1 thread | base 8 threads | head 8 threads | serial |
|---|---|---|---|---|
| `explicit_tri3_fd` | 1.015 [1.013, 1.017] | 0.342 | 0.198 | 16.2 % |
| `explicit_tet4_fd` | 1.007 [1.003, 1.009] | 0.268 | 0.200 | 9.1 % |
| `explicit_tet10_fd` | 1.007 [1.005, 1.012] | 0.333 | 0.280 | 8.1 % |
| `explicit_hex8_fd` | 1.009 [1.004, 1.010] | 0.272 | 0.242 | 5.8 % |

## After review: 2 and 4 threads, the non-local average (base 7365a08f4)

The branch was merged with `perf/wave2-base` 7365a08f4; the series below
compare the merged head with that base, size `m`, cores as above (2
threads on cores 2–3, 3 on 1–3, 4 on 0–3, 5 on 0–4, 8 on 0–7).

Non-local Mazars (`series/round/non-local-final/`, head e8061c92, 5
repetitions; ratios to the base at one thread). The per-point gather of
the average visits every pair twice: at 2 threads it made the step 1.085
(the base's serial average 0.886), so it now runs from 5 threads, where
it beats the serial average (the serial average / the gather at 4
threads 0.599 / 0.633, 5 threads 0.572 / 0.534, 6 threads 0.555 /
0.463, 8 threads 0.534 / 0.366: `series/round/cut4`, `cut5`, `cut68`,
throwaway builds that moved the cutoff):

| threads | 1 | 2 | 3 | 4 | 5 | 8 |
|---|---|---|---|---|---|---|
| base 7365a08f4 | 1 (1.671 s) | 0.885 | 0.858 | 0.834 | 0.830 | 0.815 |
| head | 0.986 | 0.721 | 0.641 | 0.599 | 0.539 | 0.371 |

Every other case at 2 and 4 threads (`series/round/t2t4/`, head
f8babbe8, whose code on these cases equals the final head's; 3
repetitions; median ratios to the base at one thread). No step is slower
than the base at the same thread count. Sub-phases within a few per cent
of the base: the SLS internal forces at 2 threads (0.547 against 0.528,
its step 0.547 against 0.568) and the phase-field tet4 phases at 2
threads (solid solve 0.979 against 0.973: the sparse solve of the base,
not changed here).

| case | phase | base, 2 threads | head, 2 threads | base, 4 threads | head, 4 threads |
|---|---|---|---|---|---|
| explicit_hex8_fd | solve_step | 0.559 | 0.542 | 0.317 | 0.290 |
| explicit_hex8_fd | internal_forces | 0.520 | 0.518 | 0.266 | 0.262 |
| explicit_hex8_ss | solve_step | 0.600 | 0.573 | 0.430 | 0.391 |
| explicit_hex8_ss | internal_forces | 0.543 | 0.546 | 0.369 | 0.371 |
| explicit_marigo | solve_step | 0.634 | 0.581 | 0.497 | 0.411 |
| explicit_marigo | internal_forces | 0.539 | 0.529 | 0.305 | 0.292 |
| explicit_maxwell | solve_step | 0.567 | 0.553 | 0.410 | 0.390 |
| explicit_maxwell | internal_forces | 0.557 | 0.548 | 0.393 | 0.386 |
| explicit_mazars | solve_step | 0.548 | 0.535 | 0.346 | 0.325 |
| explicit_mazars | internal_forces | 0.526 | 0.523 | 0.311 | 0.305 |
| explicit_multi_material | solve_step | 0.560 | 0.540 | 0.355 | 0.328 |
| explicit_multi_material | internal_forces | 0.522 | 0.520 | 0.303 | 0.294 |
| explicit_neohookean | solve_step | 0.585 | 0.567 | 0.363 | 0.341 |
| explicit_neohookean | internal_forces | 0.531 | 0.527 | 0.281 | 0.279 |
| explicit_orthotropic | solve_step | 0.604 | 0.581 | 0.423 | 0.375 |
| explicit_orthotropic | internal_forces | 0.540 | 0.544 | 0.310 | 0.305 |
| explicit_plastic | solve_step | 0.609 | 0.583 | 0.440 | 0.410 |
| explicit_plastic | internal_forces | 0.541 | 0.532 | 0.306 | 0.304 |
| explicit_sls | solve_step | 0.568 | 0.547 | 0.449 | 0.359 |
| explicit_sls | internal_forces | 0.528 | 0.547 | 0.338 | 0.326 |
| explicit_tet10_fd | solve_step | 0.648 | 0.604 | 0.400 | 0.346 |
| explicit_tet10_fd | internal_forces | 0.572 | 0.568 | 0.303 | 0.291 |
| explicit_tet10_ss | solve_step | 0.719 | 0.662 | 0.536 | 0.465 |
| explicit_tet10_ss | internal_forces | 0.619 | 0.613 | 0.408 | 0.396 |
| explicit_tet4_fd | solve_step | 0.557 | 0.520 | 0.339 | 0.282 |
| explicit_tet4_fd | internal_forces | 0.515 | 0.498 | 0.286 | 0.261 |
| explicit_tet4_ss | solve_step | 0.586 | 0.529 | 0.419 | 0.329 |
| explicit_tet4_ss | internal_forces | 0.528 | 0.501 | 0.328 | 0.285 |
| explicit_thermal | solve_step | 0.566 | 0.512 | 0.387 | 0.298 |
| explicit_thermal | internal_forces | 0.519 | 0.491 | 0.311 | 0.265 |
| explicit_tri3_fd | solve_step | 0.582 | 0.506 | 0.406 | 0.284 |
| explicit_tri3_fd | internal_forces | 0.482 | 0.463 | 0.274 | 0.241 |
| explicit_tri3_ss | solve_step | 0.628 | 0.511 | 0.514 | 0.327 |
| explicit_tri3_ss | internal_forces | 0.481 | 0.433 | 0.308 | 0.240 |
| heat_explicit_quad4 | solve_step | 0.631 | 0.589 | 0.549 | 0.484 |
| heat_explicit_tet4 | solve_step | 0.595 | 0.563 | 0.406 | 0.352 |
| phasefield_staggered_tet4 | solid_solve | 0.973 | 0.979 | 0.969 | 0.972 |
| phasefield_staggered_tet4 | phase_solve | 0.986 | 0.990 | 0.997 | 0.994 |
| phasefield_staggered_tet4 | internal_forces | 0.642 | 0.651 | 0.580 | 0.575 |
| phasefield_staggered_tri3 | solid_solve | 0.955 | 0.952 | 0.998 | 0.970 |
| phasefield_staggered_tri3 | phase_solve | 0.955 | 0.952 | 0.940 | 0.939 |
| phasefield_staggered_tri3 | internal_forces | 0.567 | 0.538 | 0.478 | 0.447 |

## One thread

The one-thread medians of the elastic cases are 0.5 to 3 % above the
base in the final series, and they move between builds whose code on
their one-thread path is the same. Across the commits of the branch,
explicit Mazars at one thread measured 1.006 (b6b3f5aa), 1.015
(4db9886a), 1.006 (2ecf3c90), 1.014 (4a90c1ad), 1.011 (9d3bf6ed) and
1.016 (c866d99a) (`series/one-thread-bisect/`); 2ecf3c90 changes only
FE checks that Mazars runs serially as before. Between c866d99a and
85d4d74d, which differ only in the FE checks, the elastic tri3 case went
from 1.005 to 1.022 (`series/one-thread-85d4d74d/`). Fixed-work profiles
put the extra samples in functions whose machine code is identical in
both builds (the same instructions apart from addresses):
`mazars_damage_on_quad` +24 % in one Mazars build, the Newmark-β
corrector +48 % and the solution gather +55 % in the tri3 build
(`profiles/one-thread/`: fixed work, 3 repetitions, two runs per build;
labels: `base` 7a6ad8f4, `head2` c866d99a, `head3` ce03eaad, `head`
85d4d74d, `al-*` built with every function aligned to 64 bytes).

With every function aligned to 64 bytes
(`-C llvm-args=-align-all-functions=6`, base and head built the same
way), the head equals the base. Ratios to the unaligned base at one
thread, aligned head against aligned base: tri3 1.005 against 1.007,
tri3 finite-deformation 0.998 against 1.003, tet10 1.004 against 1.015,
tet4 0.999 against 0.999, Mazars 1.006 against 1.006
(`series/aligned-functions-85d4d74d/`); the total samples of fixed-work
profiles of Mazars and multi-material differ by 0.1 %
(`series/aligned-functions/`, `profiles/one-thread/al-*`; `al-head` is
c866d99a, `al-head3` ce03eaad, `al-head4` 85d4d74d). The differences
come from code placement; the code is not slower. Two one-thread costs
that were real are fixed on the branch: the per-element connectivity
check called the threaded maximum once per element (multi-material
0.8 %), and Mazars' copies at one thread use `Array::clone` again.

## What limits the rest

- Serial by the bitwise contract: the external-work increment's sum (one
  fold in node order; 0.7 to 3 % of the 8-thread step, most in the tri3
  cases).
- Zeroing and copies in libc on the calling thread (`memset`, `memmove`,
  about 7 to 10 % in most cases): the residual zeroed before each
  assembly (`zero_residual`), the zeroing of the nodal arrays that the
  internal-force scatter fills (`scatter_block`), the lumped
  `r += M∘a` buffer, the blocked flags copied per step (`Vec<bool>`),
  and in the damage laws the scratch loads and the block guards
  (`scratch::load`, `get_mut`). Each is one pass over a nodal or
  per-point array; together they are what a parallel zeroing / copy with
  its first touch on the workers would take next.
- Mazars and multi-material: `elastic_family_thermal_work` (serial, 0.5
  to 2 %).
- Non-local Marigo damage after the average (cheap, not in a suite case);
  the laws left serial for the reasons in `docs/ARCHITECTURE.md`
  (anisotropic damage, `damage_linear`, the iterative laws, brittle,
  `vreepeerlings`): none is in a suite case.
- Phase field: the two sparse direct solves are serial (the sparse LU
  factorization, `panel_bmod`, `dfs_kernel` and the dense tiles: 98 to 99 %
  of the step), outside the files of this branch.
