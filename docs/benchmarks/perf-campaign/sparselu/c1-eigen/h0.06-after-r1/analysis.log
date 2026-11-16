# Campaign `h0.06-after-r1`

```
campaign=h0.06-after-r1 h=0.06 steps=10 ranks=1 reps=1 cases=cppeig rusteig extra=
date=2026-11-16T11:48:57Z revision=d7d95ae2b8d1759c1ecbddaa855ec4bc8ea52034 dirty=0 partitions=exported
host_cpu=AMD EPYC 9B14 nproc=8 kernel=7.0.0-1017-gcp
mem_total_kib=32858328
mesh=/perf/meshes/cube-h0.06.msh sha256=20523d2dec9a943249bcb6a97b42020997d559ec76f6d26dad9964afa22fa3be elements=188382
cpp_image=akantu-oracle:dev sha256:8d7d4392939c9fa64e0356b1ae8035d582f66b3019e69c03c0a6b7751edf9a6d config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:f021791b74cc2dec1c3788d4b015bb5ba08f66639037bcfbf9ff14e5b72fb40d rust_bin=/target/sparselu-after/release/cohesive_extrinsic sha256=a9b51e80c6e31cf9ad7fd6cee075afe5956d9ad1d4b0d78c63de633e1cf95c8d volume=akantu-rs-mpi-target memory=14g cpus=8 rust_extra=
pin_cpus_from=2
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 2 (2 complete, 0 failed: )
Tetrahedra: 177328

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cppeig | 1 | rep1 | 271 | 15 | 248 | 68.5 | 225 | 1.66 | 0 | 0 | 0 | 0 | 501943.5251 | 0.001268502061 | 7.62e+03 | 7.62e+03 | 7.61e+03 | — | 7.62e+03 | 7.62e+03 |
| rusteig | 1 | rep1 | 219 | 3.27 | 208 | 31.4 | 123 | 0.869 | 0 | 0 | 0 | 0 | 501943.5251 | 0.001268502061 | 5.7e+03 | 5.58e+03 | — | 614 / 4.73e+03 / 5.7e+03 | 5.3e+03 | 5.58e+03 |

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cppeig | 1 | 1 | 271 | 15 | 248 | 68.5 | 225 | 1.66 | 7.62e+03 | 7.62e+03 | 7.62e+03 | — | — | — |
| rusteig | 1 | 1 | 219 | 3.27 | 208 | 31.4 | 123 | 0.869 | 5.7e+03 | 5.58e+03 | 5.7e+03 | 614 (614) | 4.73e+03 (4.73e+03) | 5.7e+03 (5.7e+03) |

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

**np1: Rust Eigen vs C++ Eigen** — 1 pairs, bitwise 1, insertion steps equal 1, Δt rel ≤ 0

| window | ed | ep | ek | ew | et |
|---|---|---|---|---|---|
| 0-800 | 0 | 0 | 0 | 0 | 0 |
| last | 0 | 0 | 0 | 0 | 0 |

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|
| cppeig | 1 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |
| rusteig | 1 | rep1 | 0 | 0 | 0 | 32517 | 32517 | True | True | 0 |

Invariant failures: none

## Rust static solve (solver, matrix, MUMPS statistics)

- np1 rep1: {"solver": "eigen", "stats": {}, "matrix": {"order": 97551, "nnz_sum": 2157831, "nnz_max": 2157831}}

