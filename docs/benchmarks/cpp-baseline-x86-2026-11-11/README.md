# C++ reference baselines on x86_64 (2026-11-11)

The C++ reference alone (the harness of `benchmarks/cohesive-perf/` against
the default install, MUMPS static solve) on one x86_64 Linux virtual machine
(boat.dev `large`, AMD Ryzen 7 9700X, 8 cores, 15 GiB, Ubuntu 24.04, kernel 6.8), 1 / 2 / 4
ranks, `AKA_SEED = 1`, one Scotch thread, upstream's local-node rule,
interleaved repetitions. Each run carries its build record.

| Directory | What |
|---|---|
| `x86-baseline/` | before insertion: h = 0.12 and 0.06, 500 steps, 3 repetitions; h = 0.04, 300 steps, 2 repetitions; cohesive and elastic materials. 48 valid runs and one attempt stopped on upstream's MUMPS retry path (UD-216) and repeated |
| `x86-full-schedule/` | h = 0.12, 5,000 steps, 1 rank, one run in each mode: upstream's behaviour and the JOSS emulation (`--fnte joss`) |

Files per directory: `timmings_<material>_<config>_h<h>.csv` (upstream's
format), `runs.jsonl` (one record per run: Chrono phases, environment,
binary and build-record digests, memory peak), `provenance.json`,
`collection.json`. Tables: copy the directory to
`benchmarks/cohesive-perf/results/` and run `python3 -I
benchmarks/cohesive-perf/scripts/analyze.py <directory name>`.

The h = 0.06 and 0.04 meshes are the x86_64 Gmsh meshes (177,328 and 558,875
tetrahedra), not the aarch64 ones. The machine is a shared virtual machine:
fit for per-phase costs, not for scaling studies.
