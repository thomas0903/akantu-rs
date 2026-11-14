# Campaign `rust-np8-a1`

```
campaign=rust-np8-a1 h=joss steps=5000 ranks=8 reps=1 cases=rust extra=
date=2026-11-12T11:01:26Z revision=8ec21eaf9a280484d8d138034d49f6ec5326663c dirty=0 partitions=/home/<user>/c1-results/partitions
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
| rust | 8 | rep1 | 1.10e+04 | 12.8 | 831 | 249 | 1.66e+03 | 3.47 | 475636 | 4114 | 683 | 474.3632595 | 896256.3069 | 1817697.397 | 3.25e+04 | 1.69e+05 | — | 1.88e+03 / 3.02e+04 / 3.25e+04 | 3.02e+04 | 3.25e+04 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | 1 | 1.10e+04 | 12.8 | 831 | 249 | 1.66e+03 | 3.47 | 3.25e+04 | 1.69e+05 | 1.70e+05 | 1.88e+03 (1.46e+04) | 3.02e+04 (1.50e+05) | 3.25e+04 (1.70e+05) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

np8: distinct C++ partition.csv over the runs: 0

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | rep1 | 475636 | 4114 | 683 | 734594 | 942892 | True | True | 474.36325945666607 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 8099000000, "infog9_factor_reals": 8099000000, "infog29_factor_entries_effective": 7949000000, "infog16_mb_max_estimated": 28687, "infog17_mb_sum_estimated": 139310, "infog18_mb_max_allocated": 28687, "infog19_mb_sum_allocated": 139310, "infog21_mb_max_used": 28273, "infog22_mb_sum_used": 107226, "infog30_mb_max_solve": 27921, "infog31_mb_sum_solve": 133754, "rinfog1_flops_elimination_estimated": 154108254587226, "rinfog2_flops_assembly": 18843041852, "rinfog3_flops_elimination": 154108254587226, "retries": 0, "icntl14": 25}, "matrix": {"order": 2203782, "nnz_sum": 52023552, "nnz_max": 6531900}}

