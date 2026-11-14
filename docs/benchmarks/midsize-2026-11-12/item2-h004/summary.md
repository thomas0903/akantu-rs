# Campaign `item2-h004`

```
campaign=item2-h004 h=0.04 steps=300 ranks=8 4 2 1 reps=1 cases=cpp rust extra=
date=2026-11-11T23:13:59Z git_head=cddcd62623b668f90a9cd25b13190f5af5111f5d dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.04.msh sha256=1a815017e52ed0861b1cf5ef34b6c393147557f6a903210d106568b453fa3fb6
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 8 (7 complete, 1 failed: cpp/np8/rep1 rc=137)
Tetrahedra: 558875

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 456 | 57.8 | 83.4 | 166 | 718 | 1.58 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 1.17e+04 | 1.17e+04 | 1.26e+04 | — | 8.21e+03 | 1.17e+04 |
| rust | 1 | rep1 | 234 | 7.99 | 75.9 | 70.5 | 286 | 0.637 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 1.03e+04 | 1.03e+04 | — | 1.73e+03 / 7.14e+03 / 1.03e+04 | 7.21e+03 | 1.03e+04 |
| cpp | 2 | rep1 | 267 | 23.7 | 57 | 113 | 399 | 1.83 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 6.61e+03 | 1.28e+04 | 1.33e+04 | — | 4.84e+03 | 6.61e+03 |
| rust | 2 | rep1 | 169 | 4.9 | 66.2 | 58.8 | 183 | 0.866 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 6.54e+03 | 1.25e+04 | — | 954 / 4.99e+03 / 6.53e+03 | 4.99e+03 | 6.54e+03 |
| cpp | 4 | rep1 | 178 | 9.67 | 38.6 | 87.1 | 261 | 2.49 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 3.39e+03 | 1.32e+04 | 1.31e+04 | — | 2.53e+03 | 3.39e+03 |
| rust | 4 | rep1 | 90.2 | 2.96 | 29.3 | 42.4 | 97.8 | 1 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 3.12e+03 | 1.11e+04 | — | 542 / 2.36e+03 / 3.12e+03 | 2.38e+03 | 3.07e+03 |
| rust | 8 | rep1 | 67.9 | 1.7 | 19.7 | 45.5 | 74.2 | 1.71 | 0 | 0 | 0 | 0 | 503017.3467 | 24.7338551 | 2.33e+03 | 1.30e+04 | — | 318 / 1.97e+03 / 2.33e+03 | 2.04e+03 | 2.31e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | 1 | 456 | 57.8 | 83.4 | 166 | 718 | 1.58 | 1.17e+04 | 1.17e+04 | 1.17e+04 | — | — | — |
| rust | 1 | 1 | 234 | 7.99 | 75.9 | 70.5 | 286 | 0.637 | 1.03e+04 | 1.03e+04 | 1.03e+04 | 1.73e+03 (1.73e+03) | 7.14e+03 (7.14e+03) | 1.03e+04 (1.03e+04) |
| cpp | 2 | 1 | 267 | 23.7 | 57 | 113 | 399 | 1.83 | 6.61e+03 | 1.28e+04 | 1.28e+04 | — | — | — |
| rust | 2 | 1 | 169 | 4.9 | 66.2 | 58.8 | 183 | 0.866 | 6.54e+03 | 1.25e+04 | 1.27e+04 | 954 (1.87e+03) | 4.99e+03 (9.61e+03) | 6.53e+03 (1.27e+04) |
| cpp | 4 | 1 | 178 | 9.67 | 38.6 | 87.1 | 261 | 2.49 | 3.39e+03 | 1.32e+04 | 1.32e+04 | — | — | — |
| rust | 4 | 1 | 90.2 | 2.96 | 29.3 | 42.4 | 97.8 | 1 | 3.12e+03 | 1.11e+04 | 1.16e+04 | 542 (2.13e+03) | 2.36e+03 (8.6e+03) | 3.12e+03 (1.16e+04) |
| rust | 8 | 1 | 67.9 | 1.7 | 19.7 | 45.5 | 74.2 | 1.71 | 2.33e+03 | 1.30e+04 | 1.36e+04 | 318 (2.36e+03) | 1.97e+03 (1.07e+04) | 2.33e+03 (1.36e+04) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 1 | 0.514 | 0.138 | 0.91 | 0.424 | 0.398 | 0.88 | 0.88 |
| 2 | 0.635 | 0.207 | 1.16 | 0.521 | 0.459 | 0.989 | 0.977 |
| 4 | 0.507 | 0.306 | 0.759 | 0.486 | 0.375 | 0.921 | 0.845 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np2: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 7.89e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.47e-14 | 1.22e-13 | 3.41e-11 | 1.48e-14 |
| last | 0 | 1.43e-14 | 6.84e-14 | 9.26e-14 | 1.46e-14 |

np2: distinct C++ partition.csv over the runs: 1

**np4: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 2.14e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.04e-14 | 6.42e-13 | 3.49e-12 | 9.86e-15 |
| last | 0 | 1.02e-14 | 2.97e-13 | 3.64e-13 | 9.39e-15 |

np4: distinct C++ partition.csv over the runs: 1

np8: distinct C++ partition.csv over the runs: 0

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |
| rust | 1 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |
| cpp | 2 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |
| rust | 2 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |
| cpp | 4 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |
| rust | 4 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |
| rust | 8 | rep1 | 0 | 0 | 0 | 97915 | 97915 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 466527360, "infog9_factor_reals": 466527360, "infog29_factor_entries_effective": 456602676, "infog16_mb_max_estimated": 5302, "infog17_mb_sum_estimated": 5302, "infog18_mb_max_allocated": 5302, "infog19_mb_sum_allocated": 5302, "infog21_mb_max_used": 4437, "infog22_mb_sum_used": 4437, "infog30_mb_max_solve": 5241, "infog31_mb_sum_solve": 5241, "rinfog1_flops_elimination_estimated": 2415243386213, "rinfog2_flops_assembly": 992698578, "rinfog3_flops_elimination": 2415243386213, "retries": 0, "icntl14": 20}, "matrix": {"order": 293745, "nnz_sum": 6656055, "nnz_max": 6656055}}
- np2 rep1: {"solver": "mumps", "stats": {"nb_proc": 2, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 464039039, "infog9_factor_reals": 464039039, "infog29_factor_entries_effective": 451754889, "infog16_mb_max_estimated": 4016, "infog17_mb_sum_estimated": 7586, "infog18_mb_max_allocated": 4016, "infog19_mb_sum_allocated": 7586, "infog21_mb_max_used": 3089, "infog22_mb_sum_used": 6155, "infog30_mb_max_solve": 3943, "infog31_mb_sum_solve": 7420, "rinfog1_flops_elimination_estimated": 2362876269908, "rinfog2_flops_assembly": 993385140, "rinfog3_flops_elimination": 2362876269908, "retries": 0, "icntl14": 20}, "matrix": {"order": 293745, "nnz_sum": 6729276, "nnz_max": 3365919}}
- np4 rep1: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 440454777, "infog9_factor_reals": 440454777, "infog29_factor_entries_effective": 429219330, "infog16_mb_max_estimated": 1687, "infog17_mb_sum_estimated": 6012, "infog18_mb_max_allocated": 1687, "infog19_mb_sum_allocated": 6012, "infog21_mb_max_used": 1347, "infog22_mb_sum_used": 4863, "infog30_mb_max_solve": 1635, "infog31_mb_sum_solve": 5802, "rinfog1_flops_elimination_estimated": 2021136355528, "rinfog2_flops_assembly": 897799006, "rinfog3_flops_elimination": 2021136355528, "retries": 0, "icntl14": 20}, "matrix": {"order": 293745, "nnz_sum": 6800106, "nnz_max": 1701387}}
- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 446960086, "infog9_factor_reals": 447004968, "infog29_factor_entries_effective": 436671528, "infog16_mb_max_estimated": 1700, "infog17_mb_sum_estimated": 8167, "infog18_mb_max_allocated": 1700, "infog19_mb_sum_allocated": 8167, "infog21_mb_max_used": 1429, "infog22_mb_sum_used": 5896, "infog30_mb_max_solve": 1633, "infog31_mb_sum_solve": 7716, "rinfog1_flops_elimination_estimated": 2093649737680, "rinfog2_flops_assembly": 947952342, "rinfog3_flops_elimination": 2093649737680, "retries": 0, "icntl14": 25}, "matrix": {"order": 293745, "nnz_sum": 6875160, "nnz_max": 862548}}

