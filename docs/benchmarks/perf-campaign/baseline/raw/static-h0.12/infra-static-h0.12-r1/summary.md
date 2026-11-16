# Campaign `infra-static-h0.12-r1`

```
campaign=infra-static-h0.12-r1 h=0.12 steps=1 ranks=1 reps=1 cases=cpp cppeig rust rusteig extra=
date=2026-11-16T07:34:16Z revision=e8db954affe881a164cccc96b7f73ced2988bef7 dirty=0 partitions=exported
host_cpu=AMD EPYC 9R14 nproc=4 kernel=<kernel>
mem_total_kib=32135336
mesh=/perf/meshes/frozen-cube-h0.12.msh sha256=6af43648f3445c7b9c2edc07bfa70ce96539877b8ac57bed1a72604b39e28e38 elements=25988
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:0bf8962ad1a6665fb90f3d516ad763d919b2abcad9f91d256c49c6954b5bddaf rust_bin=/target/infra-bench/release/cohesive_extrinsic sha256=e61212bd0a63eb89e365d78114fc05ac578ce160b8f4cc808df1cb34bd14492f volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=2
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 4 (4 complete, 0 failed: )
Tetrahedra: 23156

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 2.85 | 1.08 | 0.951 | 10.5 | 20.6 | 1.34 | 0 | 0 | 0 | 0 | 501943.5235 | 8.493373587e-06 | 408 | 408 | 940 | — | 289 | — |
| cppeig | 1 | rep1 | 5.29 | 1.09 | 3.18 | 10.8 | 25.3 | 1.56 | 0 | 0 | 0 | 0 | 501943.5235 | 8.493373587e-06 | 523 | 523 | 497 | — | 485 | — |
| rust | 1 | rep1 | 2.34 | 0.382 | 1.09 | 6.26 | 18.2 | 1.06 | 0 | 0 | 0 | 0 | 501943.5235 | 8.493373587e-06 | 366 | 341 | — | 109 / 239 / 366 | 341 | — |
| rusteig | 1 | rep1 | 9.28 | 0.377 | 8.05 | 6.31 | 18.1 | 1.06 | 0 | 0 | 0 | 0 | 501943.5235 | 8.493373587e-06 | 580 | 580 | — | 109 / 478 / 580 | 580 | — |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | 1 | 2.85 | 1.08 | 0.951 | 10.5 | 20.6 | 1.34 | 408 | 408 | 408 | — | — | — |
| cppeig | 1 | 1 | 5.29 | 1.09 | 3.18 | 10.8 | 25.3 | 1.56 | 523 | 523 | 523 | — | — | — |
| rust | 1 | 1 | 2.34 | 0.382 | 1.09 | 6.26 | 18.2 | 1.06 | 366 | 341 | 366 | 109 (109) | 239 (239) | 366 (366) |
| rusteig | 1 | 1 | 9.28 | 0.377 | 8.05 | 6.31 | 18.1 | 1.06 | 580 | 580 | 580 | 109 (109) | 478 (478) | 580 (580) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 1 | 0.821 | 0.352 | 1.15 | 0.597 | 0.881 | 0.899 | 0.837 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np1: Rust MUMPS vs Rust Eigen** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 1.81e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.84e-15 | 6.61e-13 | 1.78e-11 | 6.84e-15 |
| last | 0 | 6.84e-15 | 6.61e-13 | 1.78e-11 | 6.84e-15 |

**np1: C++ MUMPS vs C++ Eigen** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 1.81e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.84e-15 | 6.61e-13 | 1.78e-11 | 6.84e-15 |
| last | 0 | 6.84e-15 | 6.61e-13 | 1.78e-11 | 6.84e-15 |

**np1: Rust Eigen vs C++ Eigen** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 0 | 0 | 0 | 4824 | 4824 | True | True | 0 |
| cppeig | 1 | rep1 | 0 | 0 | 0 | 4824 | 4824 | True | True | 0 |
| rust | 1 | rep1 | 0 | 0 | 0 | 4824 | 4824 | True | True | 0 |
| rusteig | 1 | rep1 | 0 | 0 | 0 | 4824 | 4824 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 6368936, "infog9_factor_reals": 6368936, "infog29_factor_entries_effective": 6071094, "infog16_mb_max_estimated": 79, "infog17_mb_sum_estimated": 79, "infog18_mb_max_allocated": 79, "infog19_mb_sum_allocated": 79, "infog21_mb_max_used": 68, "infog22_mb_sum_used": 68, "infog30_mb_max_solve": 70, "infog31_mb_sum_solve": 70, "rinfog1_flops_elimination_estimated": 5071588920, "rinfog2_flops_assembly": 7165098, "rinfog3_flops_elimination": 5071588920, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}
- np1 rep1: {"solver": "eigen", "stats": {}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}

