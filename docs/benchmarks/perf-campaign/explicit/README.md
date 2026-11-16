# Explicit step: lane kernels, element-block pass, fast kernels

Branch `perf/explicit-step` (from `perf/wave2-base` `1fe1e551`). Machine: a
Google Cloud VM, AMD EPYC 7B13 (Zen 3: AVX2 and FMA, no AVX-512), 8 cores
(one thread per core) in two L3 domains (cores 0–3 and 4–7, 32 MiB each),
30 GiB, Ubuntu 24.04, used by one person; no builds while a series ran.
Release profile of the workspace, toolchain 1.99.1, default target CPU
(the AVX2 code paths are chosen at run time). Meshes: the explicit-step
meshes of `scripts/perf/explicit-step/make_meshes.sh` (SHA-256 equal to the
recorded ones). 2026-11-16.

Programs ("labels"):

| label | what |
|---|---|
| `before` | the port at `59dd2229` (`perf/optimisation` before the campaign's explicit-step work: no execution policy) |
| `base` | the branch point `1fe1e551` (threads of the execution policy, D-123) |
| `after` | this branch (`cac5d2cd` for the one-thread, thread and suite series; `8534c3f6` for the fast-mode series, which differs only in the fast-mode gradient) |
| `cpp-strict` | the C++ reference harness, strict build (`~/baseline/cpp-bin`, oracle image) |

Method: `scripts/perf/explicit-step/timing_rotated.sh` (one process per
run, program order rotated per round, 3 warm-up steps, phases `step`,
`fint`, `stress`), summaries by `summarize_timing.py` (ns per element and
step, median [min, max] over the rounds; ratios: median [min, max] of the
per-round paired ratios). How the thread count was set: every run of
every series sets it through the environment variable
`AKANTU_NUM_THREADS` (the harness and the suites read the process default
policy; the suites' `--threads` option only records a number and was not
used); unset (one thread) in the one-thread, fast-mode and suite series.
Serial runs pinned to core 2; `N` threads (`AKANTU_NUM_THREADS=N`) on
cores `0..N−1` (`scripts/perf/exec/wrappers.sh`). Raw data: `t1.jsonl`,
`threads.jsonl`, `fast.jsonl`; tables: `summary-t1.md`,
`summary-threads.md`, `summary-fast.md`; suite series: `suite-solid/`,
`suite-models/`, `summary-suite-*.md`.

## What changed

1. **Bounds checks.** The gradient, `BᵀD`, integration and
   finite-deformation residual kernels check their element ids (one
   vectorisable maximum over the filter) and the connectivity of their
   elements (one pass over the rows of consecutive ids) once per batch and
   then index without per-value checks (`chunks_exact` / `as_chunks`,
   fixed-size arrays; no `unsafe`). The element scatter of the DOF manager
   indexes each node once. A batch with a bad id or node runs the general
   path, which reports the error as before.
2. **No per-point arrays.** `BᵀD` and the total-Lagrangian residual are
   integrated over each element as they are computed
   (`FeEngine::compute_btd_integrate`, `residuals_integrate`): every point
   value is the one the separate kernels store and enters the element's
   accumulators in the order the GEMV integration reads it back.
3. **Element-block pass.** For one elastic law on one element type of a
   mesh without ghosts (the explicit-step benchmarks, the suite's
   `explicit_*_ss/fd` cases), `assemble_internal_forces` runs, in one
   threaded region, per block of about 192 KiB of per-element data: the
   gradient minus `eigen_grad_u`, the thermal pre-step, the stress (second
   Piola–Kirchhoff stress with finite deformation) and its integral; the
   scatter follows. The plans (`FeEngine::{gradient_plan,
   btd_integrate_plan}`, `assembly::ResidualPlan`) check a filter once and
   run the kernels on element ranges. `grad_u`, the stress fields and the
   thermal fields receive the values of the separate passes.
4. **Lane kernels.** Kernel bodies over `exec::Lanes4` (four lanes, every
   operation per lane, no FMA) compiled for AVX2 at run time through
   `exec::with_simd` (no `unsafe` in `akantu-fe` and `akantu-solid`; the
   dispatch is one site of `akantu-core`). Gradient: one lane per row of
   `∇u`; integrated `BᵀD`: one lane per row of `D`; integrated residual:
   one lane per node of a group of four (component fixed per vector, so the
   `B_L0` selections are the same in every lane) — in a screening run this
   layout took the hex8-20k finite-deformation step from 0.87 to 0.60 of
   the branch point's time (lanes over the component: 0.87).
   What the compiler makes of them (`objdump` of the AVX2 instances, x86-64
   release build): the integrated `BᵀD` (39 packed, 1 scalar arithmetic
   instructions) and the integrated residual (103 packed, 1 scalar) run
   packed AVX2 code; the gradient without subtraction about half packed
   (tetrahedra and hexahedra: 63 / 135 packed, as many scalar); the
   gradient with the `eigen_grad_u` subtraction, the instance the
   element-block pass runs, compiles to scalar AVX instructions only
   (`vmulsd` / `vaddsd`). A version that the compiler packs (the
   subtraction's operand taken per point instead of sliced per entry) gave
   the same step time on tet4 and hex8 (1.00–1.01) and a slower one on
   tet10 small strain (internal forces 1.09 [1.03, 1.11]), so the scalar
   version stays (`i1.jsonl`, `summary-i1.md`: `after` = `8534c3f6`, `i1`
   = the packed variant, 5 rounds); its cost is the shape-derivative stream and the gather of
   the nodal values, not the arithmetic.
5. **Filter copies.** The internal-force assembly reads the law's filter
   through its guard; the FE batch calls take the filter itself (no
   `Vec<usize>` of element ids; one is built only without a filter).
6. **Fast mode** (`Precision::Fast`, opt-in): the gradient, the integrated
   `BᵀD`, the integrated residual and the integration evaluate their sums
   as chains of fused multiply-adds (`exec::with_fma`).

Bitwise evidence: the harness digests (FNV-1a of `u`, `v`, `a`, `f_int`)
of the ten configurations equal the recorded ones (port = C++ strict,
`../baseline/explicit-digests.txt`) for `before`, `base` and `after` at 1,
2, 4 and 8 threads (`AKANTU_NUM_THREADS`, `digest_check.sh`;
`digests-after.txt`, 120 lines;
`digests-final.txt`: `8534c3f6`, 40 lines), and the 14 suite cases of
the suite series have equal digests before and after (`summary-suite-*.md`).
Every new kernel has a property test against the general product path at
every SIMD level the CPU supports (signed zeros, infinities, NaNs with and
without payload, subnormals, thread counts 1–4).

## One thread, ten configurations (7 rounds, `summary-t1.md`)

`step`, ns per element and step:

| mesh | mode | C++ strict | before | base | after | after / base | after / before | after / C++ strict |
|---|---|---|---|---|---|---|---|---|
| tet4-20k | ss | 165 [164, 166] | 80 [80, 80] | 75 [75, 75] | 62 [62, 63] | 0.83 [0.83, 0.84] | 0.78 [0.78, 0.79] | 0.38 [0.38, 0.38] |
| tet4-200k | ss | 187 [186, 198] | 106 [105, 107] | 102 [101, 103] | 81 [81, 82] | 0.80 [0.80, 0.81] | 0.77 [0.76, 0.78] | 0.43 [0.41, 0.44] |
| tet10-20k | ss | 897 [893, 902] | 464 [464, 468] | 449 [445, 451] | 333 [332, 337] | 0.74 [0.74, 0.76] | 0.72 [0.71, 0.72] | 0.37 [0.37, 0.38] |
| hex8-20k | ss | 1324 [1317, 1334] | 629 [625, 635] | 600 [597, 604] | 470 [465, 475] | 0.78 [0.78, 0.79] | 0.75 [0.74, 0.75] | 0.35 [0.35, 0.36] |
| hex8-200k | ss | 1303 [1296, 1310] | 604 [599, 613] | 586 [579, 611] | 444 [442, 450] | 0.76 [0.72, 0.77] | 0.74 [0.72, 0.74] | 0.34 [0.34, 0.35] |
| tet4-20k | fd | 387 [386, 393] | 121 [121, 122] | 114 [113, 114] | 82 [81, 82] | 0.72 [0.72, 0.72] | 0.67 [0.67, 0.68] | 0.21 [0.21, 0.21] |
| tet4-200k | fd | 421 [417, 457] | 162 [160, 162] | 153 [152, 168] | 108 [108, 109] | 0.71 [0.64, 0.71] | 0.67 [0.66, 0.68] | 0.26 [0.24, 0.26] |
| tet10-20k | fd | 2735 [2719, 2768] | 829 [823, 837] | 800 [792, 807] | 479 [475, 480] | 0.60 [0.59, 0.60] | 0.58 [0.57, 0.58] | 0.17 [0.17, 0.18] |
| hex8-20k | fd | 4390 [4269, 4462] | 1388 [1386, 1468] | 1326 [1317, 1398] | 735 [733, 744] | 0.56 [0.53, 0.56] | 0.53 [0.50, 0.53] | 0.17 [0.16, 0.17] |
| hex8-200k | fd | 4343 [4318, 4420] | 1447 [1442, 1452] | 1373 [1367, 1402] | 709 [708, 730] | 0.52 [0.51, 0.53] | 0.49 [0.49, 0.50] | 0.16 [0.16, 0.17] |

The internal-force phase (`fint`) went to 0.66–0.80 (small strain) and
0.47–0.67 (finite deformation) of the branch point; the stress phase as
the harness times it (the laws' separate passes, not the block pass) to
0.82–0.95. The rest of the step (step − fint: integration scheme,
residual, lumped solve, external work and the history copies of
`save_previous_state`) did not change (for example tet4-200k ss 19 / 21 ns,
hex8-20k ss 147 / 143 ns, base / after).

## Threads (5 rounds, `summary-threads.md`)

`step`, paired ratio after / base at the same thread count
(`AKANTU_NUM_THREADS=N`, wrappers of `scripts/perf/exec/wrappers.sh`, cores
`0..N−1`; the absolute
times are in the summary; during the session cores 0–3 were slower than
cores 4–7 for the same run, 45 against 29 ns per element and step for
tet4-20k small strain at 4 threads, which widens some ranges):

| mesh | mode | 2 threads | 4 threads | 8 threads |
|---|---|---|---|---|
| tet4-20k | ss | 0.77 [0.76, 0.82] | 0.76 [0.64, 0.82] | 0.70 [0.45, 1.02] |
| tet4-200k | ss | 0.80 [0.79, 1.16] | 0.83 [0.77, 0.96] | 0.75 [0.72, 0.77] |
| tet10-20k | ss | 0.74 [0.56, 0.82] | 0.86 [0.60, 0.94] | 0.80 [0.80, 1.36] |
| hex8-20k | ss | 0.76 [0.75, 0.78] | 0.77 [0.72, 0.80] | 0.77 [0.73, 1.02] |
| hex8-200k | ss | 0.80 [0.71, 0.81] | 0.70 [0.59, 0.72] | 0.71 [0.68, 0.73] |
| tet4-20k | fd | 0.80 [0.80, 1.43] | 0.74 [0.65, 0.77] | 0.84 [0.79, 0.93] |
| tet4-200k | fd | 0.75 [0.73, 1.07] | 0.86 [0.81, 0.91] | 0.80 [0.79, 0.83] |
| tet10-20k | fd | 0.64 [0.64, 0.71] | 0.76 [0.71, 0.78] | 0.77 [0.72, 0.78] |
| hex8-20k | fd | 0.60 [0.60, 0.61] | 0.61 [0.59, 0.65] | 0.69 [0.67, 0.70] |
| hex8-200k | fd | 0.54 [0.53, 0.56] | 0.56 [0.54, 0.58] | 0.62 [0.61, 0.62] |

Thread scaling of `after` against its own one-thread time (medians of the
two series, not paired): hex8-200k fd 709 → 422 / 300 / 235 ns (1.7× / 2.4×
/ 3.0× at 2 / 4 / 8 threads), tet4-200k ss 81 → 61 / 57 / 43 ns. What
limits it now, from a profile at 4 threads (`AKANTU_NUM_THREADS=4`, hex8-20k
and tet4-200k small
strain, `perf record` on all threads): the serial part of the step. The
history copies of `save_previous_state` (`memcpy`) take about a third of
the wall time on hex8-20k and a quarter on tet4-200k at 4 threads; the
integration scheme, residual and lumped-solve loops a few per cent each.
Threading those loops with parts of 16,384 equations (instead of 262,144)
made the rest faster but the next internal-force pass slower (the
displacement written by other cores): step 1.09× slower on tet4-20k, 0.96
on hex8-20k at 4 threads (`AKANTU_NUM_THREADS=4` on cores 4–7, two
alternating runs of 5 repetitions each, a screening measurement); not
kept.

## Fast mode (5 rounds, `summary-fast.md`)

`step` of `after` with `AKANTU_PRECISION=fast` against the bitwise
default, one thread:

| mesh | ss | fd |
|---|---|---|
| tet4-20k | 0.94 [0.93, 0.95] | 0.94 [0.94, 0.94] |
| tet4-200k | 0.95 [0.95, 0.96] | 0.93 [0.92, 0.93] |
| tet10-20k | 0.99 [0.99, 1.00] | 0.96 [0.96, 0.97] |
| hex8-20k | 0.95 [0.94, 0.95] | 0.87 [0.86, 0.88] |
| hex8-200k | 0.94 [0.92, 0.99] | 0.84 [0.83, 0.85] |

Measured differences (`crates/akantu-solid/tests/fast_mode_gradient.rs`,
cubes of 750 tetrahedra and 125 hexahedra, 20 steps): largest
|fast − bitwise| / max|bitwise| of `u`, `v`, `a`, `f_int` 2.7e-15 on this
CPU and 2.2e-15 on aarch64; the rule `fast_mode/solid/explicit_fma_kernels`
allows 1e-12 (`fixtures/fast_mode/solid/tolerances.json`). The gain is
small because FMA only shortens the dependency chains of kernels whose
cost is mostly memory traffic and data movement; the largest is in the
finite-deformation residual.

## Benchmark suites (5 repetitions, `summary-suite-*.md`)

`solve_step` of the suite cases that run the explicit kernels, after /
base (paired, median [min, max]), size `m`, one thread
(`AKANTU_NUM_THREADS` unset) on core 2:

| case | after / base |
|---|---|
| `explicit_tet4_ss` | 0.80 [0.79, 0.82] |
| `explicit_tet4_fd` | 0.72 [0.72, 0.72] |
| `explicit_tet10_ss` | 0.74 [0.74, 0.75] |
| `explicit_tet10_fd` | 0.58 [0.58, 0.59] |
| `explicit_hex8_ss` | 0.77 [0.77, 0.78] |
| `explicit_hex8_fd` | 0.57 [0.57, 0.58] |
| `explicit_tri3_ss` | 0.78 [0.78, 0.79] |
| `explicit_tri3_fd` | 0.95 [0.95, 0.96] |
| `explicit_thermal` | 0.78 [0.77, 0.79] |
| `explicit_multi_material` (three laws: separate passes) | 0.94 [0.92, 0.99] |
| `explicit_plastic` (separate passes) | 0.96 [0.96, 0.97] |
| `cohesive_extrinsic_check` (bulk and cohesive laws: separate passes) | 0.78 [0.78, 0.79] |
| `heat_explicit_tet4` (suite_models) | 0.93 [0.92, 0.94] |
| `contact_explicit_quad4` (suite_models) | 0.95 [0.94, 0.95] |

Digests equal for all 14 cases. The 2-D finite-deformation case gains
least; three nodes per triangle fill three of the four node lanes of the
residual kernel.

## Not done, and why

- **The gradient** is the largest kernel left (20–25 % of the small-strain
  step). On hexahedra it is bound by the stream of shape derivatives
  (1.5 KiB per element); on tetrahedra by the gather of the nodal
  displacements. Other lane layouts (one lane per column of `∇u`, four
  elements per vector) do not reduce the number of loads; four elements per
  vector would need a transposed shape-derivative layout.
- **Lanes over nodes for `BᵀD`** (as in the residual) measured 1.11× slower
  on tet4-20k: transposing `B` per point costs more than the broadcasts it
  saves (reverted).
- **AVX-512**: three-component lanes do not fill 512-bit vectors; no
  instance was written (this CPU has no AVX-512).
- **The scatter** (17 % of tet4-200k small strain) is a random-access
  read-modify-write of `f_int` in element order; the order is part of the
  results.
- **The history copies** of `save_previous_state` (13 % of hex8 small
  strain at one thread, the main serial cost with threads) belong to
  upstream's state handling; avoiding them needs double buffering.

Fast-mode candidates not done: an order-free scatter (per-thread partial
`f_int` summed afterwards; the scatter is 17 % of tet4 small strain at one
thread and the main threaded cost there); a stress kernel and history
handling in single passes with the block pass; vectorised reductions in the
integration scheme's energy terms.
