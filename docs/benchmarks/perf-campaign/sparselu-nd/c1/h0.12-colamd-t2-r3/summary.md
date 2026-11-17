# Campaign `h0.12-colamd-t2-r3`

```
campaign=h0.12-colamd-t2-r3 h=0.12 steps=10 ranks=1 reps=1 cases=rusteig extra=
date=2026-11-17T10:23:46Z revision=00cd1031efca56d15bfe80239f29600b73457529 dirty=0 partitions=exported
host_cpu=AMD EPYC 9B14 nproc=8 kernel=7.0.0-1017-gcp
mem_total_kib=32858328
mesh=/perf/meshes/frozen-cube-h0.12.msh sha256=6af43648f3445c7b9c2edc07bfa70ce96539877b8ac57bed1a72604b39e28e38 elements=25988
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:f021791b74cc2dec1c3788d4b015bb5ba08f66639037bcfbf9ff14e5b72fb40d rust_bin=/target/nd-head/release/cohesive_extrinsic sha256=7c89425826bb8b24f03b9e6e5252c3ce91a87d48a27ff533e3e08ee1323e1397 volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=0 rust_threads=2
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 1 (1 complete, 0 failed: )
Tetrahedra: 23156

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rusteig | 1 | rep1 | 2.27 | 0.171 | 1.63 | 2.21 | 5.8 | 0.346 | 0 | 0 | 0 | 0 | 501943.5347 | 0.003051392725 | 428 | 428 | — | 108 / 362 / 428 | 428 | — |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rusteig | 1 | 1 | 2.27 | 0.171 | 1.63 | 2.21 | 5.8 | 0.346 | 428 | 428 | 428 | 108 (108) | 362 (362) | 428 (428) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rusteig | 1 | rep1 | 0 | 0 | 0 | 4824 | 4824 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "eigen", "stats": {"ordering": 0, "nnz_l": 8222229, "nnz_u": 8222229, "off_diagonal_pivots": 0}, "matrix": {"order": 14472, "nnz_sum": 299727, "nnz_max": 299727}}

