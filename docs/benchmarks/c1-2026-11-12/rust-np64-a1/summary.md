# Campaign `rust-np64-a1`

```
campaign=rust-np64-a1 h=joss steps=5000 ranks=64 reps=1 cases=rust extra=
date=2026-11-12T07:38:57Z revision=8ec21eaf9a280484d8d138034d49f6ec5326663c dirty=0 partitions=/home/<user>/c1-results/partitions
host_cpu=AMD EPYC 9R14 nproc=64 kernel=7.0.0-1014-aws
mem_total_kib=517095528
mesh=/perf/meshes/cube-joss.msh sha256=5ad845c731ea461d96c503d05f1278419de05840d9c69686297413bb0d86a420 elements=4485451
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=480g cpus=unlimited
rust_image=akantu-rs-bench:c1 sha256:6d408fc0d7779a1fb0fb9172f453711edc5ddffe31904d1e7a325e56beb265b4 rust_bin=/usr/local/bin/cohesive_extrinsic sha256=9d2756e830495f910f4165825626d1d435f081e32b784ceea87d70350cab2629 volume=c1-rust memory=480g cpus=64
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 1 (1 complete, 0 failed: )
Tetrahedra: 4392183

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 64 | rep1 | 2.39e+03 | 1.91 | 305 | 108 | 255 | 5.29 | 476343 | 4114 | 683 | 476.0931863 | 896157.5958 | 1842332.267 | 6.28e+03 | 2.32e+05 | — | 941 / 6.11e+03 / 6.28e+03 | 6.12e+03 | 6.28e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 64 | 1 | 2.39e+03 | 1.91 | 305 | 108 | 255 | 5.29 | 6.28e+03 | 2.32e+05 | 2.33e+05 | 941 (2.06e+04) | 6.11e+03 (2.21e+05) | 6.28e+03 (2.33e+05) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

np64: distinct C++ partition.csv over the runs: 0

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 64 | rep1 | 476343 | 4114 | 683 | 734594 | 943827 | True | True | 476.09318629787293 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np64 rep1: {"solver": "mumps", "stats": {"nb_proc": 64, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 7986000000, "infog9_factor_reals": 7988000000, "infog29_factor_entries_effective": 7862000000, "infog16_mb_max_estimated": 6155, "infog17_mb_sum_estimated": 182823, "infog18_mb_max_allocated": 6155, "infog19_mb_sum_allocated": 182823, "infog21_mb_max_used": 4109, "infog22_mb_sum_used": 110220, "infog30_mb_max_solve": 5966, "infog31_mb_sum_solve": 171543, "rinfog1_flops_elimination_estimated": 149097762529726.03, "rinfog2_flops_assembly": 20760055994, "rinfog3_flops_elimination": 149097762529726, "retries": 0, "icntl14": 35}, "matrix": {"order": 2203782, "nnz_sum": 53725776, "nnz_max": 846258}}

