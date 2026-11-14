# Campaign `test-np64`

```
campaign=test-np64 h=joss steps=20 ranks=64 reps=1 cases=rust cpp extra=
date=2026-11-12T07:19:08Z revision=8ec21eaf9a280484d8d138034d49f6ec5326663c dirty=0 partitions=exported
host_cpu=AMD EPYC 9R14 nproc=64 kernel=7.0.0-1014-aws
mem_total_kib=517095528
mesh=/perf/meshes/cube-joss.msh sha256=5ad845c731ea461d96c503d05f1278419de05840d9c69686297413bb0d86a420 elements=4485451
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=480g cpus=unlimited
rust_image=akantu-rs-bench:c1 sha256:6d408fc0d7779a1fb0fb9172f453711edc5ddffe31904d1e7a325e56beb265b4 rust_bin=/usr/local/bin/cohesive_extrinsic sha256=9d2756e830495f910f4165825626d1d435f081e32b784ceea87d70350cab2629 volume=c1-rust memory=480g cpus=64
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 2 (2 complete, 0 failed: )
Tetrahedra: 4392183

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 64 | rep1 | 441 | 7.16 | 324 | 92.3 | 229 | 4.69 | 0 | 0 | 0 | 0 | 501943.5232 | 0.001415124892 | 6.35e+03 | 2.42e+05 | 2.19e+05 | — | 6.07e+03 | 6.35e+03 |
| rust | 64 | rep1 | 463 | 1.94 | 295 | 78 | 122 | 2.91 | 0 | 0 | 0 | 0 | 501943.5232 | 0.001415124892 | 5.95e+03 | 2.20e+05 | — | 937 / 5.75e+03 / 5.95e+03 | 5.76e+03 | 5.92e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 64 | 1 | 441 | 7.16 | 324 | 92.3 | 229 | 4.69 | 6.35e+03 | 2.42e+05 | 2.42e+05 | — | — | — |
| rust | 64 | 1 | 463 | 1.94 | 295 | 78 | 122 | 2.91 | 5.95e+03 | 2.20e+05 | 2.23e+05 | 937 (2.06e+04) | 5.75e+03 (2.10e+05) | 5.95e+03 (2.23e+05) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 64 | 1.05 | 0.271 | 0.91 | 0.845 | 0.53 | 0.937 | 0.905 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np64: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 7.00e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.55e-14 | 9.36e-13 | 3.74e-12 | 1.55e-14 |
| last | 0 | 1.51e-14 | 9.14e-13 | 1.73e-12 | 1.51e-14 |

np64: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 64 | rep1 | 0 | 0 | 0 | 734594 | 734594 | True | True | 0 |
| rust | 64 | rep1 | 0 | 0 | 0 | 734594 | 734594 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np64 rep1: {"solver": "mumps", "stats": {"nb_proc": 64, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 7835000000, "infog9_factor_reals": 7837000000, "infog29_factor_entries_effective": 7728000000, "infog16_mb_max_estimated": 5462, "infog17_mb_sum_estimated": 171623, "infog18_mb_max_allocated": 5462, "infog19_mb_sum_allocated": 171623, "infog21_mb_max_used": 3721, "infog22_mb_sum_used": 108124, "infog30_mb_max_solve": 5279, "infog31_mb_sum_solve": 160406, "rinfog1_flops_elimination_estimated": 141730459044871.03, "rinfog2_flops_assembly": 20030784023, "rinfog3_flops_elimination": 141730459044871, "retries": 0, "icntl14": 35}, "matrix": {"order": 2203782, "nnz_sum": 53725776, "nnz_max": 846258}}

