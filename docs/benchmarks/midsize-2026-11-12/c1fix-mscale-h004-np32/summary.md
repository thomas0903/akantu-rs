# Campaign `mscale-h004-np32`

```
campaign=mscale-h004-np32 h=0.04 steps=2 ranks=32 reps=1 cases=rust extra=
date=2026-11-12T03:12:16Z revision=ec3f00ae04de08a2608278372a976c0b7cebdf9b dirty=0 partitions=exported
host_cpu=AMD Ryzen 9 9950X 16-Core Processor nproc=8 kernel=6.8.0-117-generic
mem_total_kib=16375776
mesh=/perf/meshes/cube-h0.04.msh sha256=1a815017e52ed0861b1cf5ef34b6c393147557f6a903210d106568b453fa3fb6 elements=582313
cpp_image=akantu-oracle:dev sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5 config=strict fnte=joss memory=14g cpus=unlimited
rust_image=akantu-rs-mumps:dev sha256:c1648e0311dff04a952672de693fff32dfb82148b06fa310d223e5c337c5578a rust_bin=/target/c1/release/cohesive_extrinsic sha256=9d2756e830495f910f4165825626d1d435f081e32b784ceea87d70350cab2629 volume=akantu-rs-mpi-target memory=14g cpus=8
threads: OMP_NUM_THREADS=1 OPENBLAS_NUM_THREADS=1 SCOTCH_PTHREAD_NUMBER=1 OPENBLAS_CORETYPE=Haswell (image ENV) AKA_SEED=1
```

Runs: 1 (0 complete, 1 failed: rust/np32/rep1 rc=137)
Tetrahedra: None

## Runs

Memory in MiB. `run peak rank` / `run peak total`: sampler, whole run (exact up to the sampling interval). `Rust model / static / loop`: the driver's own records, largest rank (VmRSS before the static solve, VmHWM at its end, VmHWM of the loop after a reset). `~static`, `~loop`: the sampler split at the approximate boundary (largest rank).

| case | np | rep | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | coh. el. | ins. steps | first | E_d | E_p | E_k | run peak rank | run peak total | C++ container peak | Rust model / static / loop | ~static | ~loop |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|

## Medians per case and rank count

Memory in MiB (see Runs). `run peak total`: largest sum of the ranks' VmRSS over the samples (can miss sub-interval transients); `Σ rank peaks`: sum of the ranks' VmHWM (an upper bound of the simultaneous total). Rust columns: the driver's own records (largest rank; sum over the ranks in parentheses).

| case | np | n | wall s | init_full s | static_solve s | solve_step ms/step | check ms/step | core-µs/el/step | run peak rank | run peak total | Σ rank peaks | Rust model | Rust static HWM | Rust loop HWM |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|

## Rust / C++ (medians)

| np | wall | init_full | static_solve | solve_step | check | run peak rank | run peak total |
|---|---|---|---|---|---|---|---|

## Energy histories and insertions

Max relative difference per column (ed, ep, ek, ew, et) over the step windows; `bitwise` = identical energies.csv text.

## Counts and oracle-free invariants per run

Invariants: owned nodes summed over the ranks = global nodes at the end (every node owned once); regular elements summed = tetrahedra of the mesh; global nodes at the end = start + the nodes doubled by insertion (≥ start).

| case | np | rep | coh. el. | ins. steps | first | nodes start | nodes end | owned sum = end | regular = tets | E_d final |
|---|---|---|---|---|---|---|---|---|---|---|

Invariant failures: none

