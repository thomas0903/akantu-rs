# Campaign `quota-q8`

```
campaign=quota-q8 h=0.06 steps=300 ranks=8 reps=1 cases=rust extra=
date=2026-11-11T23:47:24Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
campaign=quota-q8 h=0.06 steps=300 ranks=8 reps=1 cases=rust extra=
date=2026-11-11T23:48:12Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
campaign=quota-q8 h=0.06 steps=300 ranks=8 reps=1 cases=rust extra=
date=2026-11-11T23:49:00Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 3 (3 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | rep1 | 14.5 | 0.349 | 2.54 | 10.2 | 22.1 | 1.46 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 611 | 3.32e+03 | — | 125 / 500 / 611 | 611 | 526 |
| rust | 8 | rep1.again-1794440900 | 15.4 | 0.365 | 3.18 | 10.6 | 22.5 | 1.49 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 605 | 3.77e+03 | — | 127 / 495 / 605 | 605 | 521 |
| rust | 8 | rep1.again-1794440948 | 14 | 0.357 | 2.38 | 9.49 | 22 | 1.42 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 601 | 3.87e+03 | — | 127 / 490 / 600 | 601 | 516 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | 3 | 14.5 | 0.357 | 2.54 | 10.2 | 22.1 | 1.46 | 605 | 3.77e+03 | 4e+03 | 127 (958) | 495 (3.1e+03) | 605 (4e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np8: Rust vs Rust (run to run)** — 3 pairs, bitwise 0, insertion steps equal 3, Δt rel ≤ 5.67e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 8.12e-16 | 1.23e-13 | 1.13e-11 | 9.28e-16 |
| last | 0 | 1.16e-16 | 9.51e-14 | 1.05e-13 | 4.64e-16 |

np8: distinct C++ partition.csv over the runs: 0

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| rust | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1.again-1794440900 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1.again-1794440948 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 93756205, "infog9_factor_reals": 93777331, "infog29_factor_entries_effective": 91040697, "infog16_mb_max_estimated": 391, "infog17_mb_sum_estimated": 1955, "infog18_mb_max_allocated": 391, "infog19_mb_sum_allocated": 1955, "infog21_mb_max_used": 297, "infog22_mb_sum_used": 1584, "infog30_mb_max_solve": 343, "infog31_mb_sum_solve": 1594, "rinfog1_flops_elimination_estimated": 217552620007, "rinfog2_flops_assembly": 168977582, "rinfog3_flops_elimination": 217552620007, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep1.again-1794440900: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 93756205, "infog9_factor_reals": 93777331, "infog29_factor_entries_effective": 91040697, "infog16_mb_max_estimated": 391, "infog17_mb_sum_estimated": 1955, "infog18_mb_max_allocated": 391, "infog19_mb_sum_allocated": 1955, "infog21_mb_max_used": 297, "infog22_mb_sum_used": 1589, "infog30_mb_max_solve": 343, "infog31_mb_sum_solve": 1594, "rinfog1_flops_elimination_estimated": 217552620007, "rinfog2_flops_assembly": 168976827, "rinfog3_flops_elimination": 217552620007, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep1.again-1794440948: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 93756205, "infog9_factor_reals": 93777331, "infog29_factor_entries_effective": 91040697, "infog16_mb_max_estimated": 391, "infog17_mb_sum_estimated": 1955, "infog18_mb_max_allocated": 391, "infog19_mb_sum_allocated": 1955, "infog21_mb_max_used": 297, "infog22_mb_sum_used": 1564, "infog30_mb_max_solve": 343, "infog31_mb_sum_solve": 1594, "rinfog1_flops_elimination_estimated": 217552620007, "rinfog2_flops_assembly": 168993538, "rinfog3_flops_elimination": 217552620007, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}

