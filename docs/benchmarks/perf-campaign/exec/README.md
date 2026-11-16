# Execution policy (D-123): measurements

Branch `perf/exec-policy`. Two machines:

- **Report tables**: an AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, 32 GiB),
  Ubuntu 24.04, shared by the campaign's agents through job wrappers:
  runs of 1, 2 and 4 threads on cores 0–3 while builds run on cores 4–7,
  runs of 8 threads with the machine alone.
- **Screening** ("o1"): a cloud VM (AMD EPYC Milan, 8 vCPUs shown as 8 sockets
  of one core, 15 GiB), used alone. Its numbers decided the changes; they
  are not the report's tables.

Release profile of the workspace, one process per run, pinned with
`taskset` (thread `i` on core `i`), the program order rotated per round
(`scripts/perf/explicit-step/timing_rotated.sh`), labels from
`scripts/perf/exec/wrappers.sh` (`AKANTU_NUM_THREADS=N`, cores `0..N-1`).
"base" is the branch point `59dd2229` (one thread, no execution policy).
Meshes: the cubes of `scripts/perf/explicit-step/make_meshes.sh`, made with
gmsh 4.15.2 on macOS (`meshes-SHA256SUMS`). Tables: `summarize_threads.py`
(ns per element and step, median [min, max] over the rounds; speed-up =
base / variant per round, median [min, max]; efficiency = speed-up /
threads; "rest" = step − fint per run: integration scheme, residual,
lumped solve, external work).

## Thread pool: cost of one region (m7a.2xlarge)

`scripts/perf/exec/pool-overhead` (D-123 pool choice): median time of one
region, 2,000 regions per point, 5 rounds, variants interleaved; "overhead"
= region time − serial time / threads. `exec` = `Exec::for_each`;
`rayon-b` = `rayon::ThreadPool::broadcast`; `spin` = a hand-written
reference pool. Nominal work in µs of serial time (column "serial" is the
measured serial time). 2 and 4 threads on cores 0–3 while other jobs built
on cores 4–7 (`pool-own-final-t2-t4.jsonl`, commit `3b6d0257`, the final
pool); 8 threads with the machine alone (`pool-own-t8-interim.jsonl`, the
own pool at commit `0ec6efbf`, before the enter/close protocol and the
block claiming; `pool-rayon.jsonl`: the same series when `exec` was the
`rayon` pool, commit `e1aa744e`). 2026-11-16.

| threads | work (µs) | serial (µs) | `exec` overhead (µs) | `rayon-b` overhead (µs) |
|---|---|---|---|---|
| 2 | 0 | 0.03 | 0.18 | 4.94 |
| 2 | 10 | 9.92 | 0.70 | 4.86 |
| 2 | 30 | 29.66 | 1.39 | 5.45 |
| 2 | 100 | 98.79 | 3.68 | 7.76 |
| 4 | 0 | 0.04 | 0.69 | 6.44 |
| 4 | 10 | 9.91 | 0.73 | 13.68 |
| 4 | 30 | 29.64 | 0.71 | 13.66 |
| 4 | 100 | 98.78 | 0.74 | 13.78 |
| 8 | 0 | 0.04 | 0.84 | 6.99 |
| 8 | 10 | 9.92 | 1.36 | 13.70 |
| 8 | 30 | 29.70 | 1.41 | 13.31 |
| 8 | 100 | 98.92 | 1.51 | 13.80 |

With `exec` = `rayon` (`pool-rayon.jsonl`) the overhead was 4.95–8.58 µs at
2–4 threads and 6.7–10.8 µs at 8.

Regions after a serial stretch (`crates/akantu-core/examples/exec_region_gap.rs`,
o1, 20 µs nominal regions): with workers parking after 500 µs, a region
after a 1–2 ms serial gap took 18.2–31.8 µs against 7.4–8.1 µs of serial
work (the caller paid the futex wake-up); with 5 ms of spinning, 4.7–5.2 µs.

## Explicit step at 1, 2 and 4 threads (m7a.2xlarge)

Commit `3b6d0257`, 5 rounds, cores 0–3 (other agents' builds on cores
4–7), 2026-11-16 09:28 UTC (`series-m7a-t124.jsonl`, `summary-m7a-t124.md`).
The 200 k-element meshes and the 8-thread points were measured on o1 only
(below); they are to be repeated on the m7a.2xlarge with the C++
reference. Speed-up = base (branch point, one thread) /
variant, paired per round.

| mesh | mode | phase | base ns/el/step | 1 thread | 2 threads | 4 threads |
|---|---|---|---|---|---|---|
|  tet4-20k  |  ss  |  step  |  69.0 [68.7, 69.2]  |  **0.97** [0.97, 0.99]  |  **1.24** [1.23, 1.25]  |  **1.53** [1.53, 1.54]  |
|  tet4-20k  |  ss  |  fint  |  58.1 [57.7, 58.2]  |  **0.98** [0.98, 0.99]  |  **1.39** [1.39, 1.40]  |  **1.84** [1.83, 1.85]  |
|  tet4-20k  |  ss  |  stress  |  24.3 [24.3, 24.4]  |  **0.98** [0.97, 0.99]  |  **1.81** [1.80, 1.82]  |  **2.26** [2.24, 2.38]  |
|  tet4-20k  |  ss  |  rest  |  11.0 [10.6, 11.2]  |  **0.91** [0.90, 0.97]  |  **0.79** [0.78, 0.81]  |  **0.82** [0.80, 0.84]  |
|  tet4-20k  |  fd  |  step  |  102.7 [102.7, 103.4]  |  **0.98** [0.98, 0.99]  |  **1.41** [1.40, 1.42]  |  **1.86** [1.84, 1.91]  |
|  tet4-20k  |  fd  |  fint  |  88.7 [88.5, 89.4]  |  **1.00** [0.99, 1.00]  |  **1.58** [1.57, 1.60]  |  **2.28** [2.24, 2.28]  |
|  tet4-20k  |  fd  |  stress  |  30.3 [30.1, 30.4]  |  **1.00** [0.99, 1.00]  |  **1.86** [1.85, 1.87]  |  **2.46** [2.40, 2.63]  |
|  tet4-20k  |  fd  |  rest  |  14.0 [13.9, 14.2]  |  **0.90** [0.89, 0.94]  |  **0.84** [0.82, 0.85]  |  **0.87** [0.85, 0.94]  |
|  hex8-20k  |  ss  |  step  |  583.8 [581.8, 584.7]  |  **1.01** [1.01, 1.01]  |  **1.45** [1.45, 1.46]  |  **1.60** [1.59, 1.75]  |
|  hex8-20k  |  ss  |  fint  |  438.6 [435.2, 439.7]  |  **1.02** [1.02, 1.03]  |  **1.81** [1.80, 1.82]  |  **2.14** [2.13, 2.17]  |
|  hex8-20k  |  ss  |  stress  |  185.7 [185.2, 187.1]  |  **0.99** [0.98, 1.00]  |  **1.85** [1.83, 1.87]  |  **2.81** [2.78, 2.88]  |
|  hex8-20k  |  ss  |  rest  |  146.1 [143.0, 147.3]  |  **0.98** [0.94, 1.00]  |  **0.92** [0.89, 0.93]  |  **0.91** [0.90, 1.11]  |
|  hex8-20k  |  fd  |  step  |  1070.1 [1068.7, 1078.5]  |  **0.98** [0.97, 0.99]  |  **1.59** [1.58, 1.60]  |  **2.20** [2.00, 2.22]  |
|  hex8-20k  |  fd  |  fint  |  866.1 [865.2, 876.6]  |  **0.99** [0.98, 1.00]  |  **1.88** [1.87, 1.90]  |  **3.20** [3.19, 3.24]  |
|  hex8-20k  |  fd  |  stress  |  233.1 [231.9, 234.7]  |  **1.00** [0.99, 1.01]  |  **1.90** [1.90, 1.93]  |  **3.13** [3.12, 3.16]  |
|  hex8-20k  |  fd  |  rest  |  203.2 [201.9, 205.3]  |  **0.96** [0.93, 1.00]  |  **0.96** [0.95, 0.97]  |  **0.95** [0.77, 0.95]  |

## Explicit step at 1, 2, 4 and 8 threads (o1 screening)

Commit `3b6d0257`, 5 rounds, the machine alone (`screening-o1/screen-3b6d0257.jsonl`,
`summary-3b6d0257.md`; earlier screens of the branch's intermediate commits
alongside). Same columns, plus 8 threads.

| mesh | mode | phase | base ns/el/step | 1 thread | 2 threads | 4 threads | 8 threads |
|---|---|---|---|---|---|---|---|
|  tet4-20k  |  ss  |  step  |  68.7 [68.2, 69.3]  |  **0.98** [0.97, 0.98]  |  **1.23** [0.38, 1.26]  |  **0.55** [0.47, 0.70]  |  **0.62** [0.56, 0.65]  |
|  tet4-20k  |  ss  |  fint  |  58.3 [58.0, 58.8]  |  **0.98** [0.97, 0.99]  |  **1.37** [0.48, 1.40]  |  **0.62** [0.60, 0.80]  |  **0.85** [0.80, 0.92]  |
|  tet4-20k  |  ss  |  rest  |  10.5 [10.1, 10.7]  |  **0.94** [0.92, 1.05]  |  **0.80** [0.17, 0.82]  |  **0.31** [0.20, 0.42]  |  **0.25** [0.21, 0.27]  |
|  tet4-20k  |  fd  |  step  |  101.2 [100.5, 102.0]  |  **0.99** [0.98, 0.99]  |  **1.12** [0.48, 1.41]  |  **0.84** [0.71, 1.49]  |  **0.86** [0.70, 0.90]  |
|  tet4-20k  |  fd  |  fint  |  88.6 [88.4, 90.5]  |  **1.00** [0.99, 1.02]  |  **1.56** [0.68, 1.60]  |  **0.91** [0.80, 2.27]  |  **1.28** [1.00, 1.33]  |
|  tet4-20k  |  fd  |  rest  |  12.6 [10.5, 13.4]  |  **0.96** [0.74, 0.99]  |  **0.36** [0.16, 0.85]  |  **0.46** [0.37, 0.66]  |  **0.26** [0.23, 0.27]  |
|  tet4-200k  |  ss  |  step  |  88.8 [88.5, 91.4]  |  **0.98** [0.98, 1.00]  |  **0.71** [0.66, 1.19]  |  **0.98** [0.93, 1.07]  |  **1.11** [1.05, 1.17]  |
|  tet4-200k  |  ss  |  fint  |  71.6 [70.4, 72.9]  |  **0.99** [0.97, 1.00]  |  **0.82** [0.73, 1.23]  |  **1.20** [1.14, 1.31]  |  **1.54** [1.47, 1.63]  |
|  tet4-200k  |  ss  |  rest  |  18.2 [17.2, 19.1]  |  **0.99** [0.95, 1.04]  |  **0.64** [0.37, 1.08]  |  **0.53** [0.50, 0.77]  |  **0.52** [0.49, 0.56]  |
|  tet4-200k  |  fd  |  step  |  135.2 [135.1, 138.4]  |  **0.99** [0.98, 0.99]  |  **0.95** [0.87, 1.35]  |  **1.34** [1.27, 1.36]  |  **1.50** [1.41, 1.85]  |
|  tet4-200k  |  fd  |  fint  |  101.4 [101.2, 109.9]  |  **1.00** [0.99, 1.06]  |  **1.18** [1.03, 1.65]  |  **1.75** [1.63, 1.96]  |  **2.16** [2.05, 2.42]  |
|  tet4-200k  |  fd  |  rest  |  33.9 [28.5, 34.2]  |  **0.97** [0.79, 0.99]  |  **0.67** [0.54, 0.97]  |  **0.80** [0.54, 0.90]  |  **0.80** [0.64, 1.09]  |
|  hex8-20k  |  ss  |  step  |  541.3 [536.3, 545.0]  |  **1.01** [1.01, 1.02]  |  **1.24** [1.05, 1.27]  |  **1.35** [1.28, 1.41]  |  **1.49** [1.34, 1.57]  |
|  hex8-20k  |  ss  |  fint  |  404.4 [402.9, 411.2]  |  **1.03** [1.02, 1.04]  |  **1.48** [1.46, 1.88]  |  **2.12** [1.48, 2.96]  |  **3.23** [2.81, 3.66]  |
|  hex8-20k  |  ss  |  rest  |  133.8 [128.0, 138.6]  |  **0.97** [0.91, 1.00]  |  **0.85** [0.43, 0.90]  |  **0.61** [0.52, 0.91]  |  **0.57** [0.47, 0.67]  |
|  hex8-20k  |  fd  |  step  |  1167.9 [1162.8, 1190.2]  |  **0.99** [0.98, 1.00]  |  **1.36** [1.31, 1.62]  |  **1.91** [1.85, 2.16]  |  **2.30** [1.79, 2.41]  |
|  hex8-20k  |  fd  |  fint  |  838.3 [832.4, 914.2]  |  **0.99** [0.99, 1.08]  |  **1.86** [1.80, 2.09]  |  **3.12** [2.92, 3.68]  |  **5.57** [5.47, 6.14]  |
|  hex8-20k  |  fd  |  rest  |  331.6 [253.7, 351.9]  |  **0.99** [0.75, 1.04]  |  **0.77** [0.60, 1.30]  |  **0.92** [0.70, 1.34]  |  **0.92** [0.68, 1.00]  |
|  hex8-200k  |  ss  |  step  |  565.7 [562.5, 584.4]  |  **0.98** [0.97, 0.98]  |  **1.18** [1.14, 1.32]  |  **1.37** [1.15, 1.38]  |  **1.45** [1.43, 1.46]  |
|  hex8-200k  |  ss  |  fint  |  437.0 [435.7, 450.7]  |  **0.98** [0.89, 0.99]  |  **1.22** [1.20, 1.59]  |  **1.63** [1.21, 1.75]  |  **1.86** [1.85, 1.89]  |
|  hex8-200k  |  ss  |  rest  |  126.9 [123.6, 133.7]  |  **0.96** [0.95, 1.40]  |  **0.95** [0.79, 1.07]  |  **0.86** [0.80, 0.98]  |  **0.83** [0.79, 0.84]  |
|  hex8-200k  |  fd  |  step  |  1313.4 [1292.5, 1333.0]  |  **1.01** [1.00, 1.03]  |  **1.55** [1.51, 1.56]  |  **2.27** [2.16, 2.29]  |  **2.71** [2.58, 2.79]  |
|  hex8-200k  |  fd  |  fint  |  1143.6 [1129.7, 1151.0]  |  **1.01** [1.00, 1.02]  |  **1.80** [1.77, 1.90]  |  **2.74** [2.66, 2.80]  |  **4.05** [3.98, 4.11]  |
|  hex8-200k  |  fd  |  rest  |  165.8 [162.7, 189.4]  |  **0.96** [0.96, 1.09]  |  **0.75** [0.69, 0.84]  |  **1.08** [0.83, 1.20]  |  **0.80** [0.76, 0.90]  |

**The m7a.2xlarge and o1 disagree on the small tet4 mesh**: the m7a.2xlarge gives 1.24× (2 threads)
and 1.53× (4 threads) on tet4-20k small strain with spreads of ±1 %, o1
gives 1.23× and 0.55× with spreads from 0.38 to 1.26. o1 is a VM whose 8
vCPUs are shown as 8 sockets; its host placement is unknown, and on it the
serial part of the step ("rest") becomes 2–4 times slower while the
workers spin, which points at vCPUs sharing physical cores. The
m7a.2xlarge (8 cores, no SMT) is authoritative.

## Bitwise evidence

`scripts/perf/explicit-step/digests.sh 30` (FNV-1a of `u`, `v`, `a`,
`f_int` after 30 steps), base against the head at 1, 2, 4 and 8 threads,
tet4-20k, hex8-20k, tet4-200k, hex8-200k, small strain and finite
deformation: identical for every commit measured, on the m7a.2xlarge
(`digests-m7a-3b6d0257.txt`; also `e1aa744e`, `0ec6efbf`, `fe18d3a0`,
`5c7543bb`) and on o1 (`screening-o1/digests-*.txt`). Before the fix of
`e1aa744e` the head differed from base at every thread count (the
integration scheme updated a third of the equations); the digests caught
it.

## What limits the scaling

- The per-element kernels of tet4 small strain stream several arrays per
  element for few operations. Kernel phases of the harness at 1 / 2 / 4 /
  8 threads on o1, tet4-200k, ns per element: gradient 20.1 / 10.9 / 8.5 /
  7.7, `BᵀD` 11.8 / 7.4 / 7.0 / 8.0, integration 10.3 / 9.2 / 7.8 / 8.2
  (`BᵀD` reads the stress and the shape derivatives and writes 12 values
  per element, about 270 bytes per element, 48 MB per call: 24 GB/s at one
  thread already). These kernels are bandwidth-bound; hex8 and finite
  deformation (more operations per byte) scale to 3–5.6× in fint.
- The rest of the step (Newmark-β updates, residual assembly, lumped solve,
  external work) stays serial below 262,144 equations per part: threading
  it with parts of 16,384 equations made it 2–3 times slower on o1 (short
  streaming loops whose data then lives in other cores' caches). On the m7a.2xlarge it
  runs at 0.79–0.96 of its one-thread speed with threads, because its inputs
  (the internal forces) were written by the workers.
- A sandbox profile (Ryzen 9 9950X, shares only) of tet4-200k at 8 threads
  spent about 20 % of all samples in workers waiting for the next region,
  i.e. in the serial stretches.
- `perf stat` counters on the m7a.2xlarge were not taken; those of the
  sandbox were disturbed by a concurrent test run and are not reported.

What would raise it: threading the residual and integration-scheme loops
with the caller's own block (node ranges that match the scatter's), so that
the data stays where the scatter wrote it; fusing the gradient, the stress
update and `BᵀD` into one pass per element block (one region, the
intermediate arrays stay in cache); removing the per-call copies of the
law filters and the `Vec<usize>` of element ids (2.4–7.8 % of the samples).

## One-thread overhead (m7a.2xlarge)

The head at one thread against the branch point: step 0.97–1.01, fint
0.98–1.02, stress 0.98–1.00 (all four 20 k cases); the rest 0.90–0.98,
about 1 ns per element of 11–15 on tet4. Not within noise for the rest:
listed as an open problem. Before the part kernels became separate
functions (`5c7543bb`) the head was 0.71–0.90 at one thread (the closures
lost the no-alias guarantee of function parameters).

## Wait policy (o1, review fixes)

`crates/akantu-core/examples/exec_region_gap.rs THREADS 4000`: 4,000
back-to-back regions of 4,000 items (about 5 µs of serial work), region
time median [min, max] over 5 rounds of the per-round p50, p99 and maximum
(µs), the two builds alternating. "Before": commit `4addb652` (5 ms spin,
no yield); "after": commit `427eb987` (default spin 5 ms, 0 when the
threads outnumber the cores; the caller's wait yields after 2048 polls).
Raw data: `wait-policy-o1.jsonl`.

| threads / cores | build | spin | p50 | p99 | max |
|---|---|---|---|---|---|
| 4 / 4 | before | 5 ms | 3.7 [3.6, 3.9] | 4.5 [4.2, 4.8] | 17.9 [13.5, 36.6] |
| 4 / 4 | after | 5 ms (default) | 3.9 [3.6, 4.3] | 4.4 [4.1, 5.0] | 16.4 [14.2, 58.8] |
| 4 / 4 | after | 0 | 10.8 [8.7, 12.4] | 21.5 [16.7, 22.0] | 35.4 [31.3, 69.3] |
| 16 / 8 | before | 5 ms | 8.6 [8.4, 9.1] | 16.6 [15.1, 17.8] | 6017 [5012, 6020] |
| 16 / 8 | after | 0 (default) | 20.1 [18.4, 23.3] | 81.3 [71.1, 100.3] | 197 [186, 218] |
| 16 / 8 | after | 5 ms (forced) | 7.8 [6.7, 8.6] | 15.9 [13.9, 17.4] | 6019 [6018, 6023] |

With more threads than cores, spinning workers delay a worker descheduled
inside a region by a scheduler slice (6 ms); parking at once bounds the
worst region at 0.2 ms for a higher median. With threads ≤ cores the
default is unchanged.

`pool-overhead` at commit `427eb987` on o1 (2/4/8 threads, 5 rounds,
`pool-o1-427eb987.jsonl`): overhead above `work / threads` 0.18–11.3 µs
for the own pool against 7.7–15.2 µs for `rayon::ThreadPool::broadcast`.

## Second review: one thread and the rest phase (EPYC 7B13)

An 8-core AMD EPYC 7B13 machine, threads pinned to cores 0–3, 7 rotated
rounds of 30 steps, median [min, max] of the per-round values in ns per
element and step; "rest" is step − fint. Raw data and the summary table:
`review-b/`.

**One thread against the branch point** (`f1-1124a0a1.jsonl`,
`f1-summary.md`): base `59dd2229` against the head after the error of
`gather_part` became lazy (`1124a0a1`), speed-up base/head.

| mesh | mode | step | fint | stress | rest |
|---|---|---|---|---|---|
| tet4-20k | ss | 1.00 [0.99, 1.01] | 0.99 [0.98, 1.00] | 0.99 [0.97, 1.00] | 1.03 [0.99, 1.14] |
| tet4-20k | fd | 1.00 [0.99, 1.00] | 1.00 [0.99, 1.00] | 0.99 [0.98, 1.01] | 1.02 [0.96, 1.04] |
| hex8-20k | ss | 1.00 [0.94, 1.00] | 1.00 [0.97, 1.00] | 1.00 [0.96, 1.01] | 1.00 [0.86, 1.02] |
| hex8-20k | fd | 1.00 [1.00, 1.06] | 1.00 [1.00, 1.00] | 1.00 [1.00, 1.01] | 1.01 [0.99, 1.21] |

Every phase is at 1.00 within the noise: the one-thread loss of the rest
measured on m7a (0.90–0.98, section above) came from the `Error` that
`gather_part` built for every equation (`ok_or` evaluates its argument).

**The rest phase at 2 and 4 threads** (`f3-153890e1.jsonl`): before = `1124a0a1`,
after = `153890e1` (the inverse scatter and the external work only when the
batch is split, the inverse table checked against a numbering generation,
a reused buffer for the work terms); ratio = before/after, median of the
paired per-round ratios.

| mesh | mode | 2 thr before | 2 thr after | ratio | 4 thr before | 4 thr after | ratio |
|---|---|---|---|---|---|---|---|
| tet4-20k | ss | 12.5 [12.0, 32.9] | 11.9 [11.3, 34.2] | 1.01 | 17.8 [11.5, 27.5] | 11.7 [−0.4, 17.9] | 1.06 |
| tet4-20k | fd | 15.5 [−13.2, 40.7] | 39.2 [14.0, 39.5] | 1.03 | 24.4 [15.4, 30.3] | 23.7 [23.3, 29.3] | 1.03 |
| hex8-20k | ss | 217.7 [154.7, 220.2] | 214.5 [149.2, 219.4] | 1.02 | 171.7 [157.2, 196.6] | 175.4 [169.4, 198.2] | 1.00 |
| hex8-20k | fd | 371.4 [285.2, 374.6] | 340.3 [278.9, 367.2] | 1.04 | 296.0 [270.6, 335.3] | 293.7 [268.6, 331.1] | 0.99 |

The one-thread rest of the base on the same rounds: tet4 ss 12.7, tet4
fd 15.8, hex8 ss 149.9, hex8 fd 359.4. These series are noisy: the load
average was about 2 at the start of the rounds (other jobs on cores 4–7),
and on tet4 the rest is the difference of two numbers 4–10 times larger,
so single rounds go negative. Read the paired ratios, not the medians: the
change gains 0–6 % of the rest and loses nothing measurable; the step
ratios are 1.00–1.01. On hex8 the rest at 2 and 4 threads stays above the
one-thread base (150 and 359): the residual and the integration scheme
are not threaded and the data the threaded scatter wrote is read back
from another core's cache (section "What limits the scaling").
