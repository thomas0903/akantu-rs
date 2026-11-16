# Campaign `s-aft-h0.06-np1-r3`

```
campaign=s-aft-h0.06-np1-r3 h=0.06 steps=10 ranks=1 reps=1 cases=cpp rust extra=
date=2026-11-16T10:27:47Z revision=d208746c0af89de6ada373eda2703b5709a25134 dirty=0 partitions=/home/ubuntu/baseline/partitions/h0.06
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
| cpp | 1 | rep1 | 27.4 | 9.7 | 10.9 | 77 | 225 | 1.71 | 0 | 0 | 0 | 0 | 501943.5251 | 0.001268502061 | 3.17e+03 | 3.17e+03 | 3.15e+03 | — | 2.28e+03 | 3.17e+03 |
| rust | 1 | rep1 | 17.3 | 2.79 | 10.2 | 38.2 | 126 | 0.925 | 0 | 0 | 0 | 0 | 501943.5251 | 0.001268502061 | 2.9e+03 | 2.9e+03 | — | 614 / 1.9e+03 / 2.9e+03 | 2.38e+03 | 2.9e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | 1 | 27.4 | 9.7 | 10.9 | 77 | 225 | 1.71 | 3.17e+03 | 3.17e+03 | 3.17e+03 | — | — | — |
| rust | 1 | 1 | 17.3 | 2.79 | 10.2 | 38.2 | 126 | 0.925 | 2.9e+03 | 2.9e+03 | 2.9e+03 | 614 (614) | 1.9e+03 (1.9e+03) | 2.9e+03 (2.9e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|
| 1 | 0.631 | 0.287 | 0.94 | 0.496 | 0.558 | 0.914 | 0.914 |

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust (MUMPS) vs C++ (MUMPS)** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cpp | 1 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rust | 1 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "mumps", "stats": {"nb_proc": 1, "sym": 2, "infog7_ordering": 4, "infog3_factor_reals_estimated": 94177702, "infog9_factor_reals": 94177702, "infog29_factor_entries_effective": 91836441, "infog16_mb_max_estimated": 1023, "infog17_mb_sum_estimated": 1023, "infog18_mb_max_allocated": 1023, "infog19_mb_sum_allocated": 1023, "infog21_mb_max_used": 858, "infog22_mb_sum_used": 858, "infog30_mb_max_solve": 1002, "infog31_mb_sum_solve": 1002, "rinfog1_flops_elimination_estimated": 227980874628, "rinfog2_flops_assembly": 163609845, "rinfog3_flops_elimination": 227980874628, "retries": 0, "icntl14": 20}, "matrix": {"order": 97551, "nnz_sum": 2157831, "nnz_max": 2157831}}

