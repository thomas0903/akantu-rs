# Campaign `item1-h012`

```
campaign=item1-h012 h=0.12 steps=1500 ranks=1 2 4 reps=3 cases=cpp rust extra=
date=2026-11-11T20:50:35Z git_head=08cffe1c64add2de7ee2c69329aeee9af6cf01b3 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/frozen-cube-h0.12.msh sha256=6af43648f3445c7b9c2edc07bfa70ce96539877b8ac57bed1a72604b39e28e38
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=50ca1ffff7a468e68826834f7525a9c15c5b5400335da8c158c35caab0454930 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
campaign=item1-h012 h=0.12 steps=1500 ranks=1 reps=1 cases=cppeig rusteig extra=
date=2026-11-11T20:56:54Z git_head=08cffe1c64add2de7ee2c69329aeee9af6cf01b3 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/frozen-cube-h0.12.msh sha256=6af43648f3445c7b9c2edc07bfa70ce96539877b8ac57bed1a72604b39e28e38
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=50ca1ffff7a468e68826834f7525a9c15c5b5400335da8c158c35caab0454930 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 20 (20 complete, 0 failed: )
Tetrahedra: 23156

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 36.2 | 0.697 | 0.464 | 5.86 | 15.1 | 0.903 | 404 | 294 | 710 | 3.123656948 | 664544.6491 | 16280.11061 | 407 | 407 | 378 | — | 407 | 407 |
| cpp | 1 | rep2 | 37.3 | 0.675 | 0.441 | 6.03 | 15.6 | 0.936 | 404 | 294 | 710 | 3.123656948 | 664544.6491 | 16280.11061 | 408 | 408 | 378 | — | 407 | 408 |
| cpp | 1 | rep3 | 35.8 | 0.679 | 0.439 | 5.92 | 14.8 | 0.894 | 404 | 294 | 710 | 3.123656948 | 664544.6491 | 16280.11061 | 407 | 407 | 378 | — | 407 | 407 |
| cppeig | 1 | rep1 | 38 | 0.677 | 2.21 | 6 | 15 | 0.905 | 404 | 294 | 710 | 3.123656656 | 664544.6491 | 16280.11061 | 523 | 523 | 497 | — | 523 | 523 |
| rust | 1 | rep1 | 23 | 0.268 | 0.54 | 2.57 | 10.3 | 0.554 | 404 | 294 | 710 | 3.123656948 | 664544.6491 | 16280.11061 | 365 | 340 | — | 107 / 238 / 365 | 365 | 281 |
| rust | 1 | rep2 | 23.2 | 0.264 | 0.548 | 2.6 | 10.4 | 0.561 | 404 | 294 | 710 | 3.123656948 | 664544.6491 | 16280.11061 | 365 | 281 | — | 107 / 238 / 365 | 365 | 281 |
| rust | 1 | rep3 | 23.1 | 0.264 | 0.519 | 2.59 | 10.3 | 0.558 | 404 | 294 | 710 | 3.123656948 | 664544.6491 | 16280.11061 | 365 | 281 | — | 107 / 238 / 365 | 365 | 281 |
| rusteig | 1 | rep1 | 28.5 | 0.269 | 4.96 | 2.67 | 10.9 | 0.584 | 404 | 294 | 710 | 3.123656656 | 664544.6491 | 16280.11061 | 574 | 574 | — | 107 / 472 / 574 | 574 | 475 |
| cpp | 2 | rep1 | 23.3 | 0.335 | 0.291 | 4.47 | 9.06 | 1.17 | 404 | 294 | 710 | 3.123656911 | 664548.7161 | 16280.11188 | 250 | 494 | 427 | — | 250 | 250 |
| cpp | 2 | rep2 | 23.8 | 0.339 | 0.298 | 4.46 | 9.32 | 1.19 | 404 | 294 | 710 | 3.123656948 | 664548.7161 | 16280.11188 | 250 | 496 | 427 | — | 250 | 250 |
| cpp | 2 | rep3 | 23.3 | 0.333 | 0.29 | 4.39 | 9.14 | 1.17 | 404 | 294 | 710 | 3.123656911 | 664548.7161 | 16280.11188 | 251 | 496 | 427 | — | 251 | 251 |
| rust | 2 | rep1 | 13.3 | 0.153 | 0.334 | 1.81 | 5.64 | 0.644 | 404 | 294 | 710 | 3.123656655 | 664544.6491 | 16280.11061 | 221 | 356 | — | 74.5 / 158 / 221 | 221 | 180 |
| rust | 2 | rep2 | 13.8 | 0.156 | 0.354 | 1.87 | 5.87 | 0.668 | 404 | 294 | 710 | 3.123656908 | 664544.6491 | 16280.11061 | 221 | 353 | — | 74.5 / 158 / 221 | 221 | 178 |
| rust | 2 | rep3 | 13.4 | 0.152 | 0.329 | 1.8 | 5.74 | 0.651 | 404 | 294 | 710 | 3.123656908 | 664544.6491 | 16280.11061 | 220 | 353 | — | 74.3 / 158 / 220 | 220 | 179 |
| cpp | 4 | rep1 | 14.1 | 0.179 | 0.165 | 2.77 | 5.26 | 1.39 | 404 | 293 | 710 | 3.123659283 | 664441.8768 | 16284.15046 | 158 | 616 | 467 | — | 158 | 158 |
| cpp | 4 | rep2 | 13.9 | 0.169 | 0.16 | 2.71 | 5.2 | 1.37 | 404 | 293 | 710 | 3.123659274 | 664441.8768 | 16284.15046 | 158 | 615 | 465 | — | 158 | 158 |
| cpp | 4 | rep3 | 14 | 0.168 | 0.16 | 2.76 | 5.2 | 1.37 | 404 | 293 | 710 | 3.123659274 | 664441.8768 | 16284.15046 | 156 | 614 | 467 | — | 156 | 156 |
| rust | 4 | rep1 | 7.69 | 0.0867 | 0.186 | 1.21 | 3.06 | 0.738 | 404 | 294 | 710 | 3.1236569 | 664544.6491 | 16280.11061 | 135 | 441 | — | 57.1 / 104 / 135 | 135 | 114 |
| rust | 4 | rep2 | 7.07 | 0.0915 | 0.182 | 1.09 | 2.78 | 0.669 | 404 | 294 | 710 | 3.123656908 | 664544.6491 | 16280.11061 | 134 | 439 | — | 57.2 / 104 / 134 | 134 | 114 |
| rust | 4 | rep3 | 8.02 | 0.0867 | 0.182 | 1.27 | 3.21 | 0.773 | 404 | 294 | 710 | 3.123656874 | 664544.6491 | 16280.11061 | 135 | 442 | — | 56.6 / 105 / 135 | 135 | 115 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | 3 | 36.2 | 0.679 | 0.441 | 5.92 | 15.1 | 0.903 | 407 | 407 | 407 | — | — | — |
| cppeig | 1 | 1 | 38 | 0.677 | 2.21 | 6 | 15 | 0.905 | 523 | 523 | 523 | — | — | — |
| rust | 1 | 3 | 23.1 | 0.264 | 0.54 | 2.59 | 10.3 | 0.558 | 365 | 281 | 365 | 107 (107) | 238 (238) | 365 (365) |
| rusteig | 1 | 1 | 28.5 | 0.269 | 4.96 | 2.67 | 10.9 | 0.584 | 574 | 574 | 574 | 107 (107) | 472 (472) | 574 (574) |
| cpp | 2 | 3 | 23.3 | 0.335 | 0.291 | 4.46 | 9.14 | 1.17 | 250 | 496 | 496 | — | — | — |
| rust | 2 | 3 | 13.4 | 0.153 | 0.334 | 1.81 | 5.74 | 0.651 | 221 | 353 | 438 | 74.5 (148) | 158 (313) | 221 (438) |
| cpp | 4 | 3 | 14 | 0.169 | 0.16 | 2.76 | 5.2 | 1.37 | 158 | 615 | 615 | — | — | — |
| rust | 4 | 3 | 7.69 | 0.0867 | 0.182 | 1.21 | 3.06 | 0.738 | 135 | 441 | 522 | 57.1 (218) | 104 (401) | 135 (522) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 1 | 0.637 | 0.389 | 1.23 | 0.438 | 0.687 | 0.896 | 0.69 |
| 2 | 0.575 | 0.456 | 1.15 | 0.405 | 0.629 | 0.88 | 0.713 |
| 4 | 0.551 | 0.513 | 1.13 | 0.44 | 0.588 | 0.855 | 0.717 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust (MUMPS) vs C++ (MUMPS)** — 9 pairs, bitwise 9, insertion steps equal 9, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| 801-1500 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np1: C++ vs C++ (run to run)** — 3 pairs, bitwise 3, insertion steps equal 3, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| 801-1500 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np1: Rust vs Rust (run to run)** — 3 pairs, bitwise 3, insertion steps equal 3, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| 801-1500 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np1: Rust MUMPS vs Rust Eigen** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 1.81e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.84e-15 | 6.75e-13 | 2.89e-11 | 6.96e-15 |
| 801-1500 | 6.79e-07 | 1.91e-11 | 3.34e-10 | 3.89e-11 | 2.25e-11 |
| last | 9.35e-08 | 2.42e-12 | 9.87e-11 | 1.07e-11 | 2.01e-12 |

**np1: C++ MUMPS vs C++ Eigen** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 1.81e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.84e-15 | 6.75e-13 | 2.89e-11 | 6.96e-15 |
| 801-1500 | 6.79e-07 | 1.91e-11 | 3.34e-10 | 3.89e-11 | 2.25e-11 |
| last | 9.35e-08 | 2.42e-12 | 9.87e-11 | 1.07e-11 | 2.01e-12 |

**np1: Rust Eigen vs C++ Eigen** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| 801-1500 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np2: Rust (MUMPS) vs C++ (MUMPS)** — 9 pairs, bitwise 0, insertion steps equal 0, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 4.43e-16 | 1.17e-13 | 3.99e-12 | 5.80e-16 |
| 801-1500 | 6.75e-07 | 6.12e-06 | 7.10e-07 | 3.71e-07 | 8.11e-06 |
| last | 9.40e-08 | 6.12e-06 | 7.80e-08 | 5.97e-09 | 8.11e-06 |

**np2: C++ vs C++ (run to run)** — 3 pairs, bitwise 1, insertion steps equal 3, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 4.31e-16 | 3.94e-14 | 1.80e-12 | 3.48e-16 |
| 801-1500 | 8.73e-08 | 2.46e-12 | 4.29e-11 | 5.01e-12 | 2.89e-12 |
| last | 1.20e-08 | 3.07e-13 | 1.28e-11 | 1.38e-12 | 2.55e-13 |

**np2: Rust vs Rust (run to run)** — 3 pairs, bitwise 1, insertion steps equal 3, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 4.40e-16 | 1.17e-13 | 2.30e-12 | 5.80e-16 |
| 801-1500 | 5.89e-07 | 1.66e-11 | 2.90e-10 | 3.38e-11 | 1.95e-11 |
| last | 8.11e-08 | 2.09e-12 | 8.60e-11 | 9.26e-12 | 1.75e-12 |

np2: distinct C++ partition.csv over the runs: 1

**np4: Rust (MUMPS) vs C++ (MUMPS)** — 9 pairs, bitwise 0, insertion steps equal 0, Δt rel ≤ 6.02e-15

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.08e-15 | 2.04e-13 | 7.95e-12 | 1.04e-15 |
| 801-1500 | 1.02e-06 | 1.55e-04 | 2.48e-04 | 8.42e-06 | 2.00e-04 |
| last | 7.71e-07 | 1.55e-04 | 2.48e-04 | 8.42e-06 | 2.00e-04 |

**np4: C++ vs C++ (run to run)** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 8.54e-16 | 1.72e-13 | 8.76e-12 | 1.04e-15 |
| 801-1500 | 1.91e-08 | 5.38e-13 | 9.61e-12 | 1.10e-12 | 6.35e-13 |
| last | 2.67e-09 | 7.57e-14 | 2.79e-12 | 3.22e-13 | 5.52e-14 |

**np4: Rust vs Rust (run to run)** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 6.02e-15

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.32e-15 | 1.74e-13 | 4.62e-12 | 1.16e-15 |
| 801-1500 | 8.06e-08 | 2.28e-12 | 3.95e-11 | 4.63e-12 | 2.67e-12 |
| last | 1.11e-08 | 2.72e-13 | 1.20e-11 | 1.25e-12 | 2.35e-13 |

np4: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569484616155 |
| cpp | 1 | rep2 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569484616155 |
| cpp | 1 | rep3 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569484616155 |
| cppeig | 1 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656656332388 |
| rust | 1 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569484616155 |
| rust | 1 | rep2 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569484616155 |
| rust | 1 | rep3 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569484616155 |
| rusteig | 1 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656656332388 |
| cpp | 2 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656910838372 |
| cpp | 2 | rep2 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656948420138 |
| cpp | 2 | rep3 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656910838372 |
| rust | 2 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236566548877134 |
| rust | 2 | rep2 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656908272393 |
| rust | 2 | rep3 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656908272393 |
| cpp | 4 | rep1 | 404 | 293 | 710 | 4824 | 4905 | True | True | 3.123659282615499 |
| cpp | 4 | rep2 | 404 | 293 | 710 | 4824 | 4905 | True | True | 3.123659274296954 |
| cpp | 4 | rep3 | 404 | 293 | 710 | 4824 | 4905 | True | True | 3.1236592742772844 |
| rust | 4 | rep1 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236568997406033 |
| rust | 4 | rep2 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.1236569082947456 |
| rust | 4 | rep3 | 404 | 294 | 710 | 4824 | 4905 | True | True | 3.123656873547837 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6368936, "infog9_factor_entries": 6368936, "infog29_factor_entries_effective": 6071094, "infog16_mb_max_estimated": 79, "infog17_mb_sum_estimated": 79, "infog18_mb_max_allocated": 79, "infog19_mb_sum_allocated": 79, "infog21_mb_max_used": 68, "infog22_mb_sum_used": 68, "infog30_mb_max_solve": 70, "infog31_mb_sum_solve": 70, "rinfog1_flops_assembly": 5071588920, "rinfog2_flops_assembly_factorization": 7165098, "rinfog3_flops_elimination": 5071588920, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}
- np1 rep1: {"solver": "eigen", "stats": {}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}
- np1 rep2: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6368936, "infog9_factor_entries": 6368936, "infog29_factor_entries_effective": 6071094, "infog16_mb_max_estimated": 79, "infog17_mb_sum_estimated": 79, "infog18_mb_max_allocated": 79, "infog19_mb_sum_allocated": 79, "infog21_mb_max_used": 68, "infog22_mb_sum_used": 68, "infog30_mb_max_solve": 70, "infog31_mb_sum_solve": 70, "rinfog1_flops_assembly": 5071588920, "rinfog2_flops_assembly_factorization": 7165098, "rinfog3_flops_elimination": 5071588920, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}
- np1 rep3: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6368936, "infog9_factor_entries": 6368936, "infog29_factor_entries_effective": 6071094, "infog16_mb_max_estimated": 79, "infog17_mb_sum_estimated": 79, "infog18_mb_max_allocated": 79, "infog19_mb_sum_allocated": 79, "infog21_mb_max_used": 68, "infog22_mb_sum_used": 68, "infog30_mb_max_solve": 70, "infog31_mb_sum_solve": 70, "rinfog1_flops_assembly": 5071588920, "rinfog2_flops_assembly_factorization": 7165098, "rinfog3_flops_elimination": 5071588920, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}
- np2 rep1: {"solver": "mumps", "stats": {"nb_proc": 2, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6277636, "infog9_factor_entries": 6277636, "infog29_factor_entries_effective": 5993955, "infog16_mb_max_estimated": 62, "infog17_mb_sum_estimated": 120, "infog18_mb_max_allocated": 62, "infog19_mb_sum_allocated": 120, "infog21_mb_max_used": 53, "infog22_mb_sum_used": 102, "infog30_mb_max_solve": 49, "infog31_mb_sum_solve": 94, "rinfog1_flops_assembly": 4729510233, "rinfog2_flops_assembly_factorization": 8696721, "rinfog3_flops_elimination": 4729510233, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 308847, "nnz_max": 154656}}
- np2 rep2: {"solver": "mumps", "stats": {"nb_proc": 2, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6277636, "infog9_factor_entries": 6277636, "infog29_factor_entries_effective": 5993955, "infog16_mb_max_estimated": 62, "infog17_mb_sum_estimated": 120, "infog18_mb_max_allocated": 62, "infog19_mb_sum_allocated": 120, "infog21_mb_max_used": 53, "infog22_mb_sum_used": 102, "infog30_mb_max_solve": 49, "infog31_mb_sum_solve": 94, "rinfog1_flops_assembly": 4729510233, "rinfog2_flops_assembly_factorization": 8696721, "rinfog3_flops_elimination": 4729510233, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 308847, "nnz_max": 154656}}
- np2 rep3: {"solver": "mumps", "stats": {"nb_proc": 2, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6277636, "infog9_factor_entries": 6277636, "infog29_factor_entries_effective": 5993955, "infog16_mb_max_estimated": 62, "infog17_mb_sum_estimated": 120, "infog18_mb_max_allocated": 62, "infog19_mb_sum_allocated": 120, "infog21_mb_max_used": 53, "infog22_mb_sum_used": 102, "infog30_mb_max_solve": 49, "infog31_mb_sum_solve": 94, "rinfog1_flops_assembly": 4729510233, "rinfog2_flops_assembly_factorization": 8696721, "rinfog3_flops_elimination": 4729510233, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 308847, "nnz_max": 154656}}
- np4 rep1: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6154515, "infog9_factor_entries": 6154515, "infog29_factor_entries_effective": 5872257, "infog16_mb_max_estimated": 52, "infog17_mb_sum_estimated": 188, "infog18_mb_max_allocated": 52, "infog19_mb_sum_allocated": 188, "infog21_mb_max_used": 45, "infog22_mb_sum_used": 166, "infog30_mb_max_solve": 29, "infog31_mb_sum_solve": 97, "rinfog1_flops_assembly": 4388070951, "rinfog2_flops_assembly_factorization": 8555622, "rinfog3_flops_elimination": 4388070951, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 317799, "nnz_max": 80208}}
- np4 rep2: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6154515, "infog9_factor_entries": 6154515, "infog29_factor_entries_effective": 5872257, "infog16_mb_max_estimated": 52, "infog17_mb_sum_estimated": 188, "infog18_mb_max_allocated": 52, "infog19_mb_sum_allocated": 188, "infog21_mb_max_used": 43, "infog22_mb_sum_used": 164, "infog30_mb_max_solve": 29, "infog31_mb_sum_solve": 97, "rinfog1_flops_assembly": 4388070951, "rinfog2_flops_assembly_factorization": 8555622, "rinfog3_flops_elimination": 4388070951, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 317799, "nnz_max": 80208}}
- np4 rep3: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_entries_estimated": 6154515, "infog9_factor_entries": 6154515, "infog29_factor_entries_effective": 5872257, "infog16_mb_max_estimated": 52, "infog17_mb_sum_estimated": 188, "infog18_mb_max_allocated": 52, "infog19_mb_sum_allocated": 188, "infog21_mb_max_used": 44, "infog22_mb_sum_used": 165, "infog30_mb_max_solve": 29, "infog31_mb_sum_solve": 97, "rinfog1_flops_assembly": 4388070951, "rinfog2_flops_assembly_factorization": 8555622, "rinfog3_flops_elimination": 4388070951, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 317799, "nnz_max": 80208}}

