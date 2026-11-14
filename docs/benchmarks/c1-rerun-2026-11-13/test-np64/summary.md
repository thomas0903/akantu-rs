# Campaign `test-np64`

```
campaign=test-np64 h=joss steps=20 ranks=64 reps=1 cases=rust extra=
date=2026-11-13T01:56:13Z revision=7ab86d04235bd99e801991d439cfb262c902069c dirty=0 partitions=/home/<user>/rerun-results/partitions
host_cpu=AMD EPYC 9R14 nproc=64 kernel=7.0.0-1014-aws
mem_total_kib=517095532
mesh=/perf/meshes/cube-joss.msh sha256=5ad845c731ea461d96c503d05f1278419de05840d9c69686297413bb0d86a420 elements=4485451
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=480g cpus=unlimited
rust_image=akantu-rs-bench:c1 sha256:100806c10f2b15ea5062fa1eadfe354859ff2e8ac33c6cb5d0828814cebbad1a rust_bin=/usr/local/bin/cohesive_extrinsic sha256=b321054d7bbf39f15b9ce4b091575e925ad5aa8250c0f36988cbdbc903bda708 volume=c1-rust memory=480g cpus=64 rust_extra=rust_cohesive_compat=upstream rust_doubled_node_load=follows_elements
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 1 (1 complete, 0 failed: )
Tetrahedra: 4392183

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 64 | rep1 | 519 | 2.01 | 350 | 80.3 | 121 | 2.93 | 0 | 0 | 0 | 0 | 501943.5232 | 0.001415124892 | 7.69e+03 | 2.28e+05 | — | 943 / 7.39e+03 / 7.67e+03 | 7.43e+03 | 7.65e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 64 | 1 | 519 | 2.01 | 350 | 80.3 | 121 | 2.93 | 7.69e+03 | 2.28e+05 | 2.30e+05 | 943 (2.07e+04) | 7.39e+03 (2.17e+05) | 7.67e+03 (2.30e+05) |

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
| rust | 64 | rep1 | 0 | 0 | 0 | 734594 | 734594 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np64 rep1: {"solver": "mumps", "stats": {"nb_proc": 64, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 8023000000, "infog9_factor_reals": 8026000000, "infog29_factor_entries_effective": 7890000000, "infog16_mb_max_estimated": 6723, "infog17_mb_sum_estimated": 177264, "infog18_mb_max_allocated": 6723, "infog19_mb_sum_allocated": 177264, "infog21_mb_max_used": 5450, "infog22_mb_sum_used": 107594, "infog30_mb_max_solve": 6540, "infog31_mb_sum_solve": 165904, "rinfog1_flops_elimination_estimated": 155811586021582, "rinfog2_flops_assembly": 21677118642, "rinfog3_flops_elimination": 155811586021582, "retries": 0, "icntl14": 35}, "matrix": {"order": 2203782, "nnz_sum": 53725776, "nnz_max": 846258}}

