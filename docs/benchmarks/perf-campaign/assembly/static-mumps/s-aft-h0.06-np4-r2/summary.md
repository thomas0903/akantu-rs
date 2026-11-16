# Campaign `s-aft-h0.06-np4-r2`

```
campaign=s-aft-h0.06-np4-r2 h=0.06 steps=10 ranks=4 reps=1 cases=rust cpp extra=
date=2026-11-16T10:27:14Z revision=d208746c0af89de6ada373eda2703b5709a25134 dirty=0 partitions=~/baseline/partitions/h0.06
host_cpu=INTEL(R) XEON(R) PLATINUM 8581C CPU @ 2.30GHz nproc=8 kernel=7.0.0-1017-gcp
mem_total_kib=32857312
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be elements=188382
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:6abe147f9f2a0ef937f329218c0edf53e3ef17bcdce35923bedfe886d2e0a1f2 rust_bin=/target/assembly/release/cohesive_extrinsic sha256=651c74543556d592441c9b9bcdf1adcadded334628d8c84ae9c10e71c5bdff20 volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 2 (2 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 4 | rep1 | 10.1 | 1.82 | 4.26 | 28.8 | 65.8 | 2.13 | 0 | 0 | 0 | 0 | 501943.5251 | 0.001268502061 | 992 | 3.87e+03 | 3.72e+03 | — | 787 | 992 |
| rust | 4 | rep1 | 7.25 | 0.841 | 4.58 | 14.7 | 36.3 | 1.15 | 0 | 0 | 0 | 0 | 501943.5251 | 0.001268502061 | 938 | 3.39e+03 | — | 201 / 695 / 938 | 890 | 777 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 4 | 1 | 10.1 | 1.82 | 4.26 | 28.8 | 65.8 | 2.13 | 992 | 3.87e+03 | 3.87e+03 | — | — | — |
| rust | 4 | 1 | 7.25 | 0.841 | 4.58 | 14.7 | 36.3 | 1.15 | 938 | 3.39e+03 | 3.58e+03 | 201 (792) | 695 (2.63e+03) | 938 (3.58e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 4 | 0.716 | 0.462 | 1.07 | 0.51 | 0.551 | 0.946 | 0.877 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np4: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 1.13e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 5.10e-15 | 2.75e-13 | 1.97e-11 | 5.10e-15 |
| last | 0 | 4.87e-15 | 2.45e-13 | 1.75e-11 | 4.87e-15 |

np4: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 4 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 4 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np4 rep1: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 98330439, "infog9_factor_reals": 98330439, "infog29_factor_entries_effective": 95354766, "infog16_mb_max_estimated": 430, "infog17_mb_sum_estimated": 1520, "infog18_mb_max_allocated": 430, "infog19_mb_sum_allocated": 1520, "infog21_mb_max_used": 315, "infog22_mb_sum_used": 1205, "infog30_mb_max_solve": 404, "infog31_mb_sum_solve": 1415, "rinfog1_flops_elimination_estimated": 248248380423, "rinfog2_flops_assembly": 182244397, "rinfog3_flops_elimination": 248248380423, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2225178, "nnz_max": 558030}}

