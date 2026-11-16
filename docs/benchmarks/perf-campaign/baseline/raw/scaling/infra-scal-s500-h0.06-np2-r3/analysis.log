# Campaign `infra-scal-s500-h0.06-np2-r3`

```
campaign=infra-scal-s500-h0.06-np2-r3 h=0.06 steps=500 ranks=2 reps=1 cases=cpp rust extra=
date=2026-11-16T10:03:59Z revision=38c6eabccf4c6d8e4f70b3a17ddcc6fa20ac6097 dirty=0 partitions=~/w/infra/data/scaling/infra-scal-s500-parts/h0.06
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
| cpp | 2 | rep1 | 114 | 5.74 | 8.73 | 41.6 | 135 | 1.99 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 1.87e+03 | 3.74e+03 | 3.68e+03 | — | 1.49e+03 | 1.87e+03 |
| rust | 2 | rep1 | 57.4 | 1.71 | 9.32 | 18.8 | 58.3 | 0.869 | 0 | 0 | 0 | 0 | 509067.8293 | 226.6157324 | 1.7e+03 | 3.2e+03 | — | 348 / 1.22e+03 / 1.7e+03 | 1.51e+03 | 1.41e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 2 | 1 | 114 | 5.74 | 8.73 | 41.6 | 135 | 1.99 | 1.87e+03 | 3.74e+03 | 3.74e+03 | — | — | — |
| rust | 2 | 1 | 57.4 | 1.71 | 9.32 | 18.8 | 58.3 | 0.869 | 1.7e+03 | 3.2e+03 | 3.39e+03 | 348 (694) | 1.22e+03 (2.42e+03) | 1.7e+03 (3.39e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 2 | 0.504 | 0.298 | 1.07 | 0.452 | 0.431 | 0.91 | 0.856 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np2: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 2.32e-16 | 9.14e-14 | 1.43e-12 | 3.48e-16 |
| last | 0 | 0 | 4.01e-15 | 1.73e-15 | 1.16e-16 |

np2: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 2 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 2 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np2 rep1: {"solver": "mumps", "stats": {"nb_proc": 2, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 98332894, "infog9_factor_reals": 98332894, "infog29_factor_entries_effective": 95201568, "infog16_mb_max_estimated": 875, "infog17_mb_sum_estimated": 1628, "infog18_mb_max_allocated": 875, "infog19_mb_sum_allocated": 1628, "infog21_mb_max_used": 684, "infog22_mb_sum_used": 1336, "infog30_mb_max_solve": 854, "infog31_mb_sum_solve": 1582, "rinfog1_flops_elimination_estimated": 251172618466, "rinfog2_flops_assembly": 178168764, "rinfog3_flops_elimination": 251172618466, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2191536, "nnz_max": 1096746}}

