# Campaign `mscale-h005-np32`

```
campaign=mscale-h005-np32 h=0.05 steps=2 ranks=32 reps=1 cases=rust extra=
date=2026-11-12T03:16:40Z revision=ec3f00ae04de08a2608278372a976c0b7cebdf9b dirty=0 partitions=exported
host_cpu=AMD Ryzen 9 9950X 16-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16375776
mesh=/perf/meshes/cube-h0.05.msh sha256=f6b1e2bf7606bc823328e6069eae3fcdf0aed19207a4941181fd6bf7bd91f9a0 elements=294077
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin=/target/c1/release/cohesive_extrinsic sha256=9d2756e830495f910f4165825626d1d435f081e32b784ceea87d70350cab2629 volume=akantu-rs-mpi-target memory=14g cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 1 (1 complete, 0 failed: )
Tetrahedra: 279131

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 32 | rep1 | 17.5 | 1.05 | 9.7 | 68.3 | 95 | 18.7 | 0 | 0 | 0 | 0 | 501943.5235 | 2.899202035e-05 | 365 | 9.55e+03 | — | 162 / 325 / 365 | 330 | 335 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 32 | 1 | 17.5 | 1.05 | 9.7 | 68.3 | 95 | 18.7 | 365 | 9.55e+03 | 9.82e+03 | 162 (2.47e+03) | 325 (8.59e+03) | 365 (9.81e+03) |

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
| rust | 32 | rep1 | 0 | 0 | 0 | 50234 | 50234 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np32 rep1: {"solver": "mumps", "stats": {"nb_proc": 32, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 168620220, "infog9_factor_reals": 168809845, "infog29_factor_entries_effective": 164351127, "infog16_mb_max_estimated": 256, "infog17_mb_sum_estimated": 6409, "infog18_mb_max_allocated": 256, "infog19_mb_sum_allocated": 6409, "infog21_mb_max_used": 211, "infog22_mb_sum_used": 4895, "infog30_mb_max_solve": 170, "infog31_mb_sum_solve": 3698, "rinfog1_flops_elimination_estimated": 497613399203.99994, "rinfog2_flops_assembly": 344669999, "rinfog3_flops_elimination": 497613399204, "retries": 0, "icntl14": 35}, "matrix": {"order": 150702, "nnz_sum": 3673176, "nnz_max": 117351}}

