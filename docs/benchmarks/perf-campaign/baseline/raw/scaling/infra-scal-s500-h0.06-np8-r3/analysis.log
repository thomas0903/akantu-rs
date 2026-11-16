# Campaign `infra-scal-s500-h0.06-np8-r3`

```
campaign=infra-scal-s500-h0.06-np8-r3 h=0.06 steps=500 ranks=8 reps=1 cases=cpp rust extra=
date=2026-11-16T10:08:40Z revision=38c6eabccf4c6d8e4f70b3a17ddcc6fa20ac6097 dirty=0 partitions=/home/ubuntu/w/infra/data/scaling/infra-scal-s500-parts/h0.06
host_cpu=AMD EPYC 9R14 nproc=8 kernel=<kernel>
mem_total_kib=32135336
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be elements=188382
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:0bf8962ad1a6665fb90f3d516ad763d919b2abcad9f91d256c49c6954b5bddaf rust_bin=/target/infra-bench/release/cohesive_extrinsic sha256=e61212bd0a63eb89e365d78114fc05ac578ce160b8f4cc808df1cb34bd14492f volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 2 (2 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 8 | rep1 | 42.4 | 1.17 | 3.29 | 19.2 | 46.5 | 2.96 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 624 | 4.4e+03 | 4.05e+03 | — | 599 | 624 |
| rust | 8 | rep1 | 26.4 | 0.467 | 3.26 | 13.2 | 25.7 | 1.75 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 593 | 3.33e+03 | — | 130 / 482 / 593 | 593 | 508 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 8 | 1 | 42.4 | 1.17 | 3.29 | 19.2 | 46.5 | 2.96 | 624 | 4.4e+03 | 4.4e+03 | — | — | — |
| rust | 8 | 1 | 26.4 | 0.467 | 3.26 | 13.2 | 25.7 | 1.75 | 593 | 3.33e+03 | 4.01e+03 | 130 (979) | 482 (3.1e+03) | 593 (4.01e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 8 | 0.623 | 0.401 | 0.99 | 0.689 | 0.552 | 0.951 | 0.757 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np8: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 1.77e-16

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.38e-14 | 4.51e-13 | 4.05e-11 | 1.38e-14 |
| last | 0 | 1.30e-14 | 2.40e-14 | 1.85e-14 | 1.35e-14 |

np8: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 93756205, "infog9_factor_reals": 93777331, "infog29_factor_entries_effective": 91040697, "infog16_mb_max_estimated": 391, "infog17_mb_sum_estimated": 1955, "infog18_mb_max_allocated": 391, "infog19_mb_sum_allocated": 1955, "infog21_mb_max_used": 297, "infog22_mb_sum_used": 1584, "infog30_mb_max_solve": 343, "infog31_mb_sum_solve": 1594, "rinfog1_flops_elimination_estimated": 217552620007, "rinfog2_flops_assembly": 168974710, "rinfog3_flops_elimination": 217552620007, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}

