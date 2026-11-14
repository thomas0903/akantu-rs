# Campaign `item2-h006`

```
campaign=item2-h006 h=0.06 steps=5000 ranks=8 4 2 reps=2 cases=cpp rust extra=
date=2026-11-11T20:59:17Z git_head=cddcd62623b668f90a9cd25b13190f5af5111f5d dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
campaign=item2-h006 h=0.06 steps=5000 ranks=1 reps=1 cases=cpp rust extra=
date=2026-11-11T22:28:57Z git_head=cddcd62623b668f90a9cd25b13190f5af5111f5d dirty=0
host_cpu=AMD EPYC 9455 48-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16374768
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin_sha256=19a3fdfafeac96c71463dab4d9955f2e4ad4a8fbb62d188c2d41874ff3a47896 cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 12 (12 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 1.6e+03 | 11.1 | 12.3 | 63.4 | 233 | 1.67 | 51528 | 3977 | 712 | 447.095431 | 1582409.853 | 7493246.983 | 3.18e+03 | 3.18e+03 | 3.15e+03 | — | 2.15e+03 | 3.18e+03 |
| rust | 1 | rep1 | 1.09e+03 | 2.42 | 13 | 45 | 154 | 1.12 | 51528 | 3977 | 712 | 447.095431 | 1582409.853 | 7493246.983 | 2.85e+03 | 2.8e+03 | — | 614 / 1.85e+03 / 2.85e+03 | 1.88e+03 | 2.8e+03 |
| cpp | 2 | rep1 | 1.04e+03 | 4.37 | 10.4 | 47.9 | 145 | 2.18 | 51719 | 3977 | 712 | 447.9681279 | 1585723.583 | 7506143.342 | 1.87e+03 | 3.74e+03 | 3.68e+03 | — | 1.36e+03 | 1.87e+03 |
| rust | 2 | rep1 | 748 | 1.49 | 9.55 | 26.1 | 112 | 1.56 | 51581 | 3977 | 712 | 448.011837 | 1580322.195 | 7523113.221 | 1.69e+03 | 3.21e+03 | — | 345 / 1.2e+03 / 1.69e+03 | 1.28e+03 | 1.68e+03 |
| cpp | 4 | rep1 | 584 | 2.23 | 5.7 | 29.7 | 78.8 | 2.45 | 52152 | 3977 | 712 | 454.3090218 | 1593090.926 | 7797728.072 | 997 | 3.84e+03 | 3.83e+03 | — | 777 | 997 |
| cpp | 4 | rep2 | 691 | 1.86 | 4.65 | 35.4 | 94.3 | 2.92 | 52176 | 3977 | 712 | 454.8528975 | 1593175.599 | 7795169.894 | 962 | 3.74e+03 | 3.6e+03 | — | 728 | 962 |
| rust | 4 | rep1 | 414 | 0.713 | 4.56 | 16.8 | 60.3 | 1.74 | 51538 | 3977 | 712 | 447.5232738 | 1577676.639 | 7799591.289 | 858 | 3.15e+03 | — | 200 / 614 / 858 | 649 | 769 |
| rust | 4 | rep2 | 447 | 0.81 | 4.87 | 18.6 | 64.5 | 1.88 | 51559 | 3977 | 712 | 447.3928955 | 1578076.756 | 7784435.518 | 902 | 3.19e+03 | — | 200 / 658 / 902 | 692 | 790 |
| cpp | 8 | rep1 | 379 | 0.889 | 2.99 | 19 | 50.3 | 3.13 | 52166 | 3976 | 712 | 455.8551483 | 1621492.218 | 7661251.647 | 623 | 4.42e+03 | 4.07e+03 | — | 525 | 623 |
| cpp | 8 | rep2 | 406 | 0.887 | 2.75 | 20.6 | 54.2 | 3.37 | 52184 | 3976 | 712 | 455.8365448 | 1621780.055 | 7662336.013 | 646 | 4.47e+03 | 4.13e+03 | — | 555 | 646 |
| rust | 8 | rep1 | 280 | 0.451 | 5.09 | 12.8 | 39 | 2.34 | 51342 | 3977 | 712 | 444.4506794 | 1584713.135 | 7496110.673 | 551 | 3.75e+03 | — | 127 / 448 / 551 | 523 | 486 |
| rust | 8 | rep2 | 297 | 0.434 | 3.46 | 15.1 | 40.2 | 2.49 | 51340 | 3977 | 712 | 444.804002 | 1584116.874 | 7514358.174 | 578 | 3.91e+03 | — | 125 / 473 / 578 | 501 | 511 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | 1 | 1.6e+03 | 11.1 | 12.3 | 63.4 | 233 | 1.67 | 3.18e+03 | 3.18e+03 | 3.18e+03 | — | — | — |
| rust | 1 | 1 | 1.09e+03 | 2.42 | 13 | 45 | 154 | 1.12 | 2.85e+03 | 2.8e+03 | 2.85e+03 | 614 (614) | 1.85e+03 (1.85e+03) | 2.85e+03 (2.85e+03) |
| cpp | 2 | 1 | 1.04e+03 | 4.37 | 10.4 | 47.9 | 145 | 2.18 | 1.87e+03 | 3.74e+03 | 3.74e+03 | — | — | — |
| rust | 2 | 1 | 748 | 1.49 | 9.55 | 26.1 | 112 | 1.56 | 1.69e+03 | 3.21e+03 | 3.37e+03 | 345 (689) | 1.2e+03 (2.4e+03) | 1.69e+03 (3.37e+03) |
| cpp | 4 | 2 | 637 | 2.05 | 5.18 | 32.5 | 86.5 | 2.69 | 979 | 3.79e+03 | 3.79e+03 | — | — | — |
| rust | 4 | 2 | 430 | 0.762 | 4.71 | 17.7 | 62.4 | 1.81 | 880 | 3.17e+03 | 3.43e+03 | 200 (785) | 636 (2.46e+03) | 880 (3.43e+03) |
| cpp | 8 | 2 | 393 | 0.888 | 2.87 | 19.8 | 52.3 | 3.25 | 634 | 4.45e+03 | 4.45e+03 | — | — | — |
| rust | 8 | 2 | 289 | 0.442 | 4.27 | 13.9 | 39.6 | 2.41 | 565 | 3.83e+03 | 4.14e+03 | 126 (956) | 461 (3.23e+03) | 565 (4.14e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 1 | 0.679 | 0.219 | 1.06 | 0.709 | 0.661 | 0.898 | 0.881 |
| 2 | 0.719 | 0.34 | 0.918 | 0.546 | 0.771 | 0.902 | 0.858 |
| 4 | 0.675 | 0.372 | 0.911 | 0.544 | 0.721 | 0.898 | 0.835 |
| 8 | 0.735 | 0.498 | 1.49 | 0.702 | 0.757 | 0.89 | 0.861 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| 801-1500 | 0 | 0 | 0 | 0 | 0 |
| 1501-end | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

**np2: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 5.71e-16 | 3.07e-13 | 2.85e-12 | 5.80e-16 |
| 801-1500 | 6.37e-10 | 5.62e-07 | 2.53e-05 | 1.97e-06 | 6.55e-07 |
| 1501-end | 0.0041 | 0.00341 | 0.00226 | 0.00202 | 0.0116 |
| last | 9.76e-05 | 0.00341 | 0.00226 | 0.00202 | 0.0116 |

np2: distinct C++ partition.csv over the runs: 1

**np4: Rust (MUMPS) vs C++ (MUMPS)** — 4 pairs, bitwise 0, insertion steps equal 4, Δt rel ≤ 1.15e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 9.99e-15 | 1.75e-13 | 1.05e-11 | 8.12e-15 |
| 801-1500 | 6.32e-10 | 2.30e-05 | 4.15e-05 | 7.42e-07 | 2.64e-05 |
| 1501-end | 0.0173 | 0.00987 | 0.0017 | 0.00158 | 0.0481 |
| last | 0.0164 | 0.00973 | 0.0017 | 0.00158 | 0.0481 |

**np4: C++ vs C++ (run to run)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 5.71e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 7.32e-15 | 9.77e-14 | 5.73e-12 | 4.75e-15 |
| 801-1500 | 6.54e-10 | 7.17e-14 | 3.93e-12 | 4.87e-13 | 6.50e-15 |
| 1501-end | 0.0024 | 3.04e-04 | 3.28e-04 | 2.80e-04 | 1.28e-05 |
| last | 0.0012 | 5.31e-05 | 3.28e-04 | 2.80e-04 | 1.28e-05 |

**np4: Rust vs Rust (run to run)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 1.02e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 3.36e-15 | 2.04e-13 | 1.23e-11 | 3.60e-15 |
| 801-1500 | 1.32e-10 | 1.47e-14 | 2.34e-13 | 1.10e-13 | 3.83e-15 |
| 1501-end | 7.58e-04 | 6.21e-04 | 0.00194 | 0.00166 | 5.02e-05 |
| last | 2.91e-04 | 2.54e-04 | 0.00194 | 0.00166 | 5.02e-05 |

np4: distinct C++ partition.csv over the runs: 1

**np8: Rust (MUMPS) vs C++ (MUMPS)** — 4 pairs, bitwise 0, insertion steps equal 0, Δt rel ≤ 1.07e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.96e-15 | 7.23e-13 | 3.59e-11 | 7.42e-15 |
| 801-1500 | 7.22e-10 | 2.84e-05 | 1.56e-04 | 2.72e-05 | 3.60e-05 |
| 1501-end | 0.0368 | 0.0238 | 0.0218 | 0.0165 | 0.108 |
| last | 0.025 | 0.0232 | 0.0217 | 0.0165 | 0.108 |

**np8: C++ vs C++ (run to run)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 5.07e-14

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 6.96e-15 | 4.38e-13 | 2.06e-11 | 7.31e-15 |
| 801-1500 | 6.77e-10 | 1.84e-14 | 4.69e-12 | 3.28e-13 | 7.89e-15 |
| 1501-end | 7.37e-04 | 5.85e-04 | 2.06e-04 | 2.02e-04 | 0.00104 |
| last | 4.08e-05 | 1.77e-04 | 1.42e-04 | 2.02e-04 | 7.09e-04 |

**np8: Rust vs Rust (run to run)** — 1 pairs, bitwise 0, insertion steps equal 1, Δt rel ≤ 1.64e-13

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 5.11e-15 | 9.90e-13 | 3.52e-11 | 3.48e-15 |
| 801-1500 | 6.42e-10 | 3.15e-14 | 3.47e-12 | 1.94e-13 | 3.60e-15 |
| 1501-end | 0.00122 | 5.83e-04 | 0.00243 | 0.00206 | 5.97e-05 |
| last | 7.94e-04 | 3.76e-04 | 0.00243 | 0.00206 | 5.97e-05 |

np8: distinct C++ partition.csv over the runs: 1

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 51528 | 3977 | 712 | 32517 | 65090 | True | True | 447.0954309862559 |
| rust | 1 | rep1 | 51528 | 3977 | 712 | 32517 | 65090 | True | True | 447.0954309862559 |
| cpp | 2 | rep1 | 51719 | 3977 | 712 | 32517 | 65280 | True | True | 447.9681279426992 |
| rust | 2 | rep1 | 51581 | 3977 | 712 | 32517 | 65178 | True | True | 448.01183697258983 |
| cpp | 4 | rep1 | 52152 | 3977 | 712 | 32517 | 65838 | True | True | 454.3090218493551 |
| cpp | 4 | rep2 | 52176 | 3977 | 712 | 32517 | 65862 | True | True | 454.85289754324515 |
| rust | 4 | rep1 | 51538 | 3977 | 712 | 32517 | 65311 | True | True | 447.52327379501037 |
| rust | 4 | rep2 | 51559 | 3977 | 712 | 32517 | 65360 | True | True | 447.39289546964176 |
| cpp | 8 | rep1 | 52166 | 3976 | 712 | 32517 | 65822 | True | True | 455.85514830583566 |
| cpp | 8 | rep2 | 52184 | 3976 | 712 | 32517 | 65845 | True | True | 455.8365448090809 |
| rust | 8 | rep1 | 51342 | 3977 | 712 | 32517 | 65003 | True | True | 444.4506794072844 |
| rust | 8 | rep2 | 51340 | 3977 | 712 | 32517 | 64991 | True | True | 444.80400200661063 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 94177702, "infog9_factor_reals": 94177702, "infog29_factor_entries_effective": 91836441, "infog16_mb_max_estimated": 1023, "infog17_mb_sum_estimated": 1023, "infog18_mb_max_allocated": 1023, "infog19_mb_sum_allocated": 1023, "infog21_mb_max_used": 858, "infog22_mb_sum_used": 858, "infog30_mb_max_solve": 1002, "infog31_mb_sum_solve": 1002, "rinfog1_flops_elimination_estimated": 227980874628, "rinfog2_flops_assembly": 163609845, "rinfog3_flops_elimination": 227980874628, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2157831, "nnz_max": 2157831}}
- np2 rep1: {"solver": "mumps", "stats": {"nb_proc": 2, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 98332894, "infog9_factor_reals": 98332894, "infog29_factor_entries_effective": 95201568, "infog16_mb_max_estimated": 875, "infog17_mb_sum_estimated": 1628, "infog18_mb_max_allocated": 875, "infog19_mb_sum_allocated": 1628, "infog21_mb_max_used": 684, "infog22_mb_sum_used": 1336, "infog30_mb_max_solve": 854, "infog31_mb_sum_solve": 1582, "rinfog1_flops_elimination_estimated": 251172618466, "rinfog2_flops_assembly": 178168764, "rinfog3_flops_elimination": 251172618466, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2191536, "nnz_max": 1096746}}
- np4 rep1: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 94259855, "infog9_factor_reals": 94259855, "infog29_factor_entries_effective": 91854306, "infog16_mb_max_estimated": 343, "infog17_mb_sum_estimated": 1324, "infog18_mb_max_allocated": 343, "infog19_mb_sum_allocated": 1324, "infog21_mb_max_used": 284, "infog22_mb_sum_used": 1100, "infog30_mb_max_solve": 317, "infog31_mb_sum_solve": 1219, "rinfog1_flops_elimination_estimated": 218740415679, "rinfog2_flops_assembly": 157555194, "rinfog3_flops_elimination": 218740415679, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2225178, "nnz_max": 558030}}
- np4 rep2: {"solver": "mumps", "stats": {"nb_proc": 4, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 96650227, "infog9_factor_reals": 96650227, "infog29_factor_entries_effective": 91728864, "infog16_mb_max_estimated": 417, "infog17_mb_sum_estimated": 1526, "infog18_mb_max_allocated": 417, "infog19_mb_sum_allocated": 1526, "infog21_mb_max_used": 304, "infog22_mb_sum_used": 1138, "infog30_mb_max_solve": 392, "infog31_mb_sum_solve": 1425, "rinfog1_flops_elimination_estimated": 225725827582, "rinfog2_flops_assembly": 153978896, "rinfog3_flops_elimination": 225725827582, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2225178, "nnz_max": 558030}}
- np8 rep1: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 95797857, "infog9_factor_reals": 95820970, "infog29_factor_entries_effective": 93173841, "infog16_mb_max_estimated": 288, "infog17_mb_sum_estimated": 2136, "infog18_mb_max_allocated": 288, "infog19_mb_sum_allocated": 2136, "infog21_mb_max_used": 230, "infog22_mb_sum_used": 1563, "infog30_mb_max_solve": 244, "infog31_mb_sum_solve": 1777, "rinfog1_flops_elimination_estimated": 236688862847, "rinfog2_flops_assembly": 169977107, "rinfog3_flops_elimination": 236688862847, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}
- np8 rep2: {"solver": "mumps", "stats": {"nb_proc": 8, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 95639483, "infog9_factor_reals": 95655215, "infog29_factor_entries_effective": 90811251, "infog16_mb_max_estimated": 347, "infog17_mb_sum_estimated": 2170, "infog18_mb_max_allocated": 347, "infog19_mb_sum_allocated": 2170, "infog21_mb_max_used": 267, "infog22_mb_sum_used": 1642, "infog30_mb_max_solve": 301, "infog31_mb_sum_solve": 1814, "rinfog1_flops_elimination_estimated": 223000182360, "rinfog2_flops_assembly": 168146760, "rinfog3_flops_elimination": 223000182360, "retries": 0, "icntl14": 25}, "matrix": {"order": 97551, "nnz_sum": 2260803, "nnz_max": 284016}}

