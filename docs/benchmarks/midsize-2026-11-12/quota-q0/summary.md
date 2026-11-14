# Campaign `quota-q0`

```
campaign=quota-q0 h=0.06 steps=300 ranks=8 reps=1 cases=rust extra=
date=2026-11-11T23:47:49Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
campaign=quota-q0 h=0.06 steps=300 ranks=8 reps=1 cases=rust extra=
date=2026-11-11T23:48:36Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
campaign=quota-q0 h=0.06 steps=300 ranks=8 reps=1 cases=rust extra=
date=2026-11-11T23:49:24Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=0
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 3 (3 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | rep1 | 14.1 | 0.345 | 2.58 | 9.4 | 21.9 | 1.41 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 571 | 3.51e+03 | — | 127 / 462 / 571 | 571 | 486 |
| rust | 8 | rep1.again-1794440925 | 14.2 | 0.394 | 2.48 | 9.61 | 21.9 | 1.42 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 558 | 3.27e+03 | — | 127 / 455 / 558 | 558 | 480 |
| rust | 8 | rep1.again-1794440971 | 14 | 0.362 | 2.46 | 9.27 | 21.9 | 1.41 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 596 | 3.33e+03 | — | 127 / 486 / 596 | 596 | 516 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | 3 | 14.1 | 0.362 | 2.48 | 9.4 | 21.9 | 1.41 | 571 | 3.33e+03 | 3.99e+03 | 127 (957) | 462 (3.09e+03) | 571 (3.99e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np8: Rust vs Rust (run to run)** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 1.63e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.04e-14 | 6.17e-13 | 4.24e-12 | 1.04e-14 |
| last | 0 | 9.83e-15 | 2.12e-13 | 2.71e-13 | 1.01e-14 |

np8: distinct C++ partition.csv over the runs: 0

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1.again-1794440925 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1.again-1794440971 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 97014411, "infog9_factor_reals": 97027378, "infog29_factor_entries_effective": 93956508, "infog16_mb_max_estimated": 333, "infog17_mb_sum_estimated": 2235, "infog18_mb_max_allocated": 333, "infog19_mb_sum_allocated": 2235, "infog21_mb_max_used": 284, "infog22_mb_sum_used": 1710, "infog30_mb_max_solve": 286, "infog31_mb_sum_solve": 1877, "rinfog1_flops_elimination_estimated": 241986446687, "rinfog2_flops_assembly": 181032009, "rinfog3_flops_elimination": 241986446687, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep1.again-1794440925: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 96268874, "infog9_factor_reals": 96297791, "infog29_factor_entries_effective": 93489003, "infog16_mb_max_estimated": 304, "infog17_mb_sum_estimated": 1849, "infog18_mb_max_allocated": 304, "infog19_mb_sum_allocated": 1849, "infog21_mb_max_used": 283, "infog22_mb_sum_used": 1455, "infog30_mb_max_solve": 258, "infog31_mb_sum_solve": 1488, "rinfog1_flops_elimination_estimated": 236567995212.99997, "rinfog2_flops_assembly": 160625363, "rinfog3_flops_elimination": 236567995213, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep1.again-1794440971: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 93756205, "infog9_factor_reals": 93777331, "infog29_factor_entries_effective": 91040697, "infog16_mb_max_estimated": 391, "infog17_mb_sum_estimated": 1955, "infog18_mb_max_allocated": 391, "infog19_mb_sum_allocated": 1955, "infog21_mb_max_used": 297, "infog22_mb_sum_used": 1548, "infog30_mb_max_solve": 343, "infog31_mb_sum_solve": 1594, "rinfog1_flops_elimination_estimated": 217552620007, "rinfog2_flops_assembly": 168990877, "rinfog3_flops_elimination": 217552620007, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}

