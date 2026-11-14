# Campaign `rust-np64-a1`

```
campaign=rust-np64-a1 h=joss steps=5000 ranks=64 reps=1 cases=rust extra=
date=2026-11-13T02:04:57Z revision=7ab86d04235bd99e801991d439cfb262c902069c dirty=0 partitions=/home/<user>/rerun-results/partitions
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
| rust | 64 | rep1 | 2.42e+03 | 1.86 | 324 | 111 | 254 | 5.32 | 471932 | 4114 | 683 | 480.7302066 | 913421.0884 | 567193.3819 | 6.2e+03 | 2.21e+05 | — | 934 / 6.02e+03 / 6.2e+03 | 6.03e+03 | 6.18e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 64 | 1 | 2.42e+03 | 1.86 | 324 | 111 | 254 | 5.32 | 6.2e+03 | 2.21e+05 | 2.25e+05 | 934 (2.07e+04) | 6.02e+03 (2.13e+05) | 6.2e+03 (2.25e+05) |

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
| rust | 64 | rep1 | 471932 | 4114 | 683 | 734594 | 947208 | True | True | 480.7302066154717 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np64 rep1: {"solver": "mumps", "stats": {"nb_proc": 64, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 7880000000, "infog9_factor_reals": 7884000000, "infog29_factor_entries_effective": 7767000000, "infog16_mb_max_estimated": 5618, "infog17_mb_sum_estimated": 174794, "infog18_mb_max_allocated": 5618, "infog19_mb_sum_allocated": 174794, "infog21_mb_max_used": 4374, "infog22_mb_sum_used": 106646, "infog30_mb_max_solve": 5412, "infog31_mb_sum_solve": 162546, "rinfog1_flops_elimination_estimated": 147618072216277, "rinfog2_flops_assembly": 20893380526, "rinfog3_flops_elimination": 147618072216277, "retries": 0, "icntl14": 35}, "matrix": {"order": 2203782, "nnz_sum": 53725776, "nnz_max": 846258}}

