# Campaign `ct-np8`

```
campaign=ct-np8 h=0.06 steps=500 ranks=8 reps=3 cases=cpp rust extra=
date=2026-11-17T04:52:46Z revision=00ea19c19903ad3891e7cd3199092db9c4d9bee2 dirty=0 partitions=~/baseline/partitions/h0.06
host_cpu=AMD EPYC 9R14 nproc=8 kernel=7.0.0-1017-aws
mem_total_kib=32135336
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be elements=188382
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:0bf8962ad1a6665fb90f3d516ad763d919b2abcad9f91d256c49c6954b5bddaf rust_bin=/target/ct-src/release/cohesive_extrinsic sha256=3fac8323ea32202392c150dd199b44630acd6c9ffa5c392be3e890cfdd81ebcb volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 6 (6 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 8 | rep1 | 43.1 | 1.17 | 3.44 | 19.6 | 47.1 | 3.01 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 636 | 4.49e+03 | 5.28e+03 | — | 622 | 636 |
| cpp | 8 | rep2 | 42.6 | 1.19 | 3.03 | 19.8 | 47.1 | 3.02 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 609 | 4.45e+03 | 4.1e+03 | — | 570 | 609 |
| cpp | 8 | rep3 | 42.6 | 1.15 | 3.02 | 19.6 | 47.1 | 3.01 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 692 | 4.25e+03 | 4.03e+03 | — | 691 | 692 |
| rust | 8 | rep1 | 15.8 | 0.213 | 2.78 | 8.91 | 14 | 1.04 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 524 | 3.71e+03 | — | 128 / 476 / 524 | 502 | 524 |
| rust | 8 | rep2 | 16 | 0.215 | 2.95 | 8.97 | 14.1 | 1.04 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 512 | 3.61e+03 | — | 129 / 462 / 512 | 512 | 512 |
| rust | 8 | rep3 | 15.8 | 0.215 | 2.82 | 8.9 | 14.1 | 1.04 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 534 | 3.54e+03 | — | 129 / 482 / 534 | 513 | 534 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 8 | 3 | 42.6 | 1.17 | 3.03 | 19.6 | 47.1 | 3.01 | 636 | 4.45e+03 | 4.45e+03 | — | — | — |
| rust | 8 | 3 | 15.8 | 0.215 | 2.82 | 8.91 | 14.1 | 1.04 | 524 | 3.61e+03 | 3.61e+03 | 129 (969) | 476 (3.19e+03) | 524 (3.61e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 8 | 0.372 | 0.184 | 0.932 | 0.454 | 0.299 | 0.824 | 0.812 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np8: Rust (MUMPS) vs C++ (MUMPS)** — 9 pairs, bitwise 0, insertion steps equal 9, Δt rel ≤ 1.71e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.33e-14 | 6.25e-13 | 6.50e-11 | 1.17e-14 |
| last | 0 | 1.33e-14 | 1.51e-13 | 2.33e-13 | 1.14e-14 |

**np8: C++ vs C++ (run to run)** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 1.72e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.29e-15 | 3.50e-13 | 2.90e-11 | 3.02e-15 |
| last | 0 | 6.29e-15 | 1.66e-13 | 2.50e-13 | 2.78e-15 |

**np8: Rust vs Rust (run to run)** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 5.64e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.68e-14 | 6.17e-13 | 8.16e-11 | 1.69e-14 |
| last | 0 | 1.49e-14 | 1.01e-13 | 1.11e-13 | 1.64e-14 |

np8: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| cpp | 8 | rep2 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| cpp | 8 | rep3 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep2 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep3 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 95639483, "infog9_factor_reals": 95655215, "infog29_factor_entries_effective": 90811251, "infog16_mb_max_estimated": 347, "infog17_mb_sum_estimated": 2170, "infog18_mb_max_allocated": 347, "infog19_mb_sum_allocated": 2170, "infog21_mb_max_used": 257, "infog22_mb_sum_used": 1629, "infog30_mb_max_solve": 301, "infog31_mb_sum_solve": 1814, "rinfog1_flops_elimination_estimated": 223000182360, "rinfog2_flops_assembly": 168172091, "rinfog3_flops_elimination": 223000182360, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep2: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 94904411, "infog9_factor_reals": 94924910, "infog29_factor_entries_effective": 91612314, "infog16_mb_max_estimated": 304, "infog17_mb_sum_estimated": 2030, "infog18_mb_max_allocated": 304, "infog19_mb_sum_allocated": 2030, "infog21_mb_max_used": 266, "infog22_mb_sum_used": 1535, "infog30_mb_max_solve": 258, "infog31_mb_sum_solve": 1672, "rinfog1_flops_elimination_estimated": 227400532556, "rinfog2_flops_assembly": 171982208, "rinfog3_flops_elimination": 227400532556, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep3: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 94126349, "infog9_factor_reals": 94150211, "infog29_factor_entries_effective": 91142604, "infog16_mb_max_estimated": 361, "infog17_mb_sum_estimated": 1936, "infog18_mb_max_allocated": 361, "infog19_mb_sum_allocated": 1936, "infog21_mb_max_used": 265, "infog22_mb_sum_used": 1509, "infog30_mb_max_solve": 315, "infog31_mb_sum_solve": 1579, "rinfog1_flops_elimination_estimated": 218897197979.99997, "rinfog2_flops_assembly": 162873107, "rinfog3_flops_elimination": 218897197980, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}

