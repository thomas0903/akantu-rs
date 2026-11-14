# Campaign `rust-np32-a1`

```
campaign=rust-np32-a1 h=joss steps=5000 ranks=32 reps=1 cases=rust extra=
date=2026-11-13T02:45:18Z revision=7ab86d04235bd99e801991d439cfb262c902069c dirty=0 partitions=/home/<user>/rerun-results/partitions
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
| rust | 32 | rep1 | 3.11e+03 | 3.12 | 338 | 110 | 402 | 3.73 | 471936 | 4114 | 683 | 480.7471655 | 913418.2749 | 567182.6766 | 1.11e+04 | 1.98e+05 | — | 655 / 1.06e+04 / 1.11e+04 | 1.06e+04 | 1.11e+04 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 32 | 1 | 3.11e+03 | 3.12 | 338 | 110 | 402 | 3.73 | 1.11e+04 | 1.98e+05 | 1.99e+05 | 655 (1.79e+04) | 1.06e+04 (1.82e+05) | 1.11e+04 (1.99e+05) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

np32: distinct C++ partition.csv over the runs: 0

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 32 | rep1 | 471936 | 4114 | 683 | 734594 | 947179 | True | True | 480.747165504831 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np32 rep1: {"solver": "mumps", "stats": {"nb_proc": 32, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 7890000000, "infog9_factor_reals": 7891000000, "infog29_factor_entries_effective": 7663000000, "infog16_mb_max_estimated": 10132, "infog17_mb_sum_estimated": 168073, "infog18_mb_max_allocated": 10132, "infog19_mb_sum_allocated": 168073, "infog21_mb_max_used": 6980, "infog22_mb_sum_used": 102946, "infog30_mb_max_solve": 9870, "infog31_mb_sum_solve": 159768, "rinfog1_flops_elimination_estimated": 143991643570511, "rinfog2_flops_assembly": 19782961877, "rinfog3_flops_elimination": 143991643570511, "retries": 0, "icntl14": 35}, "matrix": {"order": 2203782, "nnz_sum": 53089626, "nnz_max": 1677093}}

