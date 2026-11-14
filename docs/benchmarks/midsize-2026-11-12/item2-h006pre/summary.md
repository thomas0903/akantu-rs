# Campaign `item2-h006pre`

```
campaign=item2-h006pre h=0.06 steps=300 ranks=8 1 reps=1 cases=cpp rust extra=
date=2026-11-11T23:43:50Z git_head=a00bb5a488db2f7847ae9b738533c190716602e5 dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 4 (4 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 99.5 | 10.1 | 10.5 | 49.2 | 181 | 1.3 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 3.17e+03 | 3.17e+03 | 3.15e+03 | — | 2.27e+03 | 3.17e+03 |
| rust | 1 | rep1 | 53.3 | 2.14 | 11.6 | 20.2 | 84 | 0.587 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 2.85e+03 | 2.66e+03 | — | 614 / 1.85e+03 / 2.85e+03 | 2.66e+03 | 2.23e+03 |
| cpp | 8 | rep1 | 29.1 | 0.824 | 2.72 | 18.1 | 48 | 2.98 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 624 | 4.41e+03 | 4.07e+03 | — | 580 | 624 |
| rust | 8 | rep1 | 14.7 | 0.387 | 2.59 | 10 | 22.4 | 1.46 | 0 | 0 | 0 | 0 | 503448.0238 | 34.65959195 | 605 | 3.48e+03 | — | 128 / 495 / 605 | 605 | 520 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | 1 | 99.5 | 10.1 | 10.5 | 49.2 | 181 | 1.3 | 3.17e+03 | 3.17e+03 | 3.17e+03 | — | — | — |
| rust | 1 | 1 | 53.3 | 2.14 | 11.6 | 20.2 | 84 | 0.587 | 2.85e+03 | 2.66e+03 | 2.85e+03 | 614 (614) | 1.85e+03 (1.85e+03) | 2.85e+03 (2.85e+03) |
| cpp | 8 | 1 | 29.1 | 0.824 | 2.72 | 18.1 | 48 | 2.98 | 624 | 4.41e+03 | 4.41e+03 | — | — | — |
| rust | 8 | 1 | 14.7 | 0.387 | 2.59 | 10 | 22.4 | 1.46 | 605 | 3.48e+03 | 4.01e+03 | 128 (960) | 495 (3.1e+03) | 605 (4.01e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 1 | 0.535 | 0.211 | 1.1 | 0.411 | 0.464 | 0.899 | 0.837 |
| 8 | 0.504 | 0.469 | 0.951 | 0.553 | 0.466 | 0.969 | 0.789 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np8: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 6.18e-15

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 1.16e-14 | 3.76e-13 | 4.67e-11 | 1.16e-14 |
| last | 0 | 1.12e-14 | 4.84e-14 | 4.55e-14 | 1.14e-14 |

np8: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 1 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| cpp | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 8 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 94177702, "infog9_factor_reals": 94177702, "infog29_factor_entries_effective": 91836441, "infog16_mb_max_estimated": 1023, "infog17_mb_sum_estimated": 1023, "infog18_mb_max_allocated": 1023, "infog19_mb_sum_allocated": 1023, "infog21_mb_max_used": 858, "infog22_mb_sum_used": 858, "infog30_mb_max_solve": 1002, "infog31_mb_sum_solve": 1002, "rinfog1_flops_elimination_estimated": 227980874628, "rinfog2_flops_assembly": 163609845, "rinfog3_flops_elimination": 227980874628, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2157831, "nnz_max": 2157831}}
- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 93756205, "infog9_factor_reals": 93777331, "infog29_factor_entries_effective": 91040697, "infog16_mb_max_estimated": 391, "infog17_mb_sum_estimated": 1955, "infog18_mb_max_allocated": 391, "infog19_mb_sum_allocated": 1955, "infog21_mb_max_used": 297, "infog22_mb_sum_used": 1578, "infog30_mb_max_solve": 343, "infog31_mb_sum_solve": 1594, "rinfog1_flops_elimination_estimated": 217552620007, "rinfog2_flops_assembly": 168971544, "rinfog3_flops_elimination": 217552620007, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}

