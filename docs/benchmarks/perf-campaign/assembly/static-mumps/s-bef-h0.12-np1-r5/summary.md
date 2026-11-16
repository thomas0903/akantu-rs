# Campaign `s-bef-h0.12-np1-r5`

```
campaign=s-bef-h0.12-np1-r5 h=0.12 steps=10 ranks=1 reps=1 cases=rust extra=
date=2026-11-16T10:31:33Z revision=0223e03add8d32bde8187dcf37905259c59b22d8 dirty=0 partitions=/home/ubuntu/baseline/partitions/h0.06
host_cpu=INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz nproc=8 kernel=7.0.0-1017-gcp
mem_total_kib=32857312
mesh=/perf/meshes/frozen-cube-h0.12.msh sha256=6af43648f3445c7b9c2edc07bfa70ce96539877b8ac57bed1a72604b39e28e38 elements=25988
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:6abe147f9f2a0ef937f329218c0edf53e3ef17bcdce35923bedfe886d2e0a1f2 rust_bin=/target/assembly-base/release/cohesive_extrinsic sha256=887882894c1b05c787f5c3e644da6f03338c3ad217b668273b9db939b62cf7ba volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 1 (1 complete, 0 failed: )
Tetrahedra: 23156

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 1 | rep1 | 2.07 | 0.385 | 0.846 | 4.56 | 13.8 | 0.794 | 0 | 0 | 0 | 0 | 501943.5347 | 0.003051392725 | 366 | 277 | — | 108 / 239 / 366 | 366 | — |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 1 | 1 | 2.07 | 0.385 | 0.846 | 4.56 | 13.8 | 0.794 | 366 | 277 | 366 | 108 (108) | 239 (239) | 366 (366) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 1 | rep1 | 0 | 0 | 0 | 4824 | 4824 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 6368936, "infog9_factor_reals": 6368936, "infog29_factor_entries_effective": 6071094, "infog16_mb_max_estimated": 79, "infog17_mb_sum_estimated": 79, "infog18_mb_max_allocated": 79, "infog19_mb_sum_allocated": 79, "infog21_mb_max_used": 68, "infog22_mb_sum_used": 68, "infog30_mb_max_solve": 70, "infog31_mb_sum_solve": 70, "rinfog1_flops_elimination_estimated": 5071588920, "rinfog2_flops_assembly": 7165098, "rinfog3_flops_elimination": 5071588920, "retries": 0, "icntl14": 20}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}

