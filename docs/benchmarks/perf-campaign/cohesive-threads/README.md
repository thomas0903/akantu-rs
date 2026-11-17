<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The cohesive model on threads: measurements (performance campaign, 2026-11-17)

Raw data of `docs/BENCHMARKS.md`, "The cohesive model on threads".

## Machine and method

- The AWS `m7a.2xlarge` (AMD EPYC 9R14, 8 cores, Ubuntu 24.04). Timing
  under the machine's job wrappers: one process (or one MPI job) at a time
  on cores 0–3, builds on cores 4–7; runs with more than 4 cores (8
  threads, 8 ranks, ranks × threads) with the whole machine alone.
- One thread pinned to core 2; `N` threads pinned to `N` cores
  (`AKANTU_NUM_THREADS = N`); MPI ranks in the project image
  `akantu-rs-mumps:dev`, rank `r` on its own `T` cores (`mpirun --map-by
  slot:PE=T --bind-to core`, `AKANTU_NUM_THREADS = T`).
- Variants interleaved, order rotated per repetition; medians [min, max];
  ratios are the median [min, max] of the per-repetition paired ratios to
  the base at one thread (`scripts/anat.py`, `scripts/sum.py`).
- before (`base`): `perf/wave2-base` at `d404f4c70`. The thread series
  (2, 4, 8 threads; ranks × threads) ran with commit `6adbc387` (the
  threaded kernels as in the final commit); the one-thread series with the
  final commit `ac3b3179` (its serial code: the check's original loop in a
  function of its own). Default release profile, toolchain 1.99.1; host
  binaries (`scripts/build.sh`), MPI binaries in the MUMPS image with
  `mpi,scotch,mumps`.

## Series

| directory | what |
|---|---|
| `series/h012-5000` | driver, h = 0.12 full schedule, base and 1 / 2 / 4 threads |
| `series/h012-5000-t8` | the same, base and 8 threads (whole machine) |
| `series/f-h012-5000` | the same, base and the final commit at one thread |
| `series/h012-500-t1c` | 500 steps (no insertion), base and the final commit at one thread |
| `series/suite-solid*`, `series/suite1-r*`, `series/f-suite1-r*` | `suite_solid --case cohesive_extrinsic_check,cohesive_extrinsic_insertion --size m` at 1 / 2 / 4 / 8 threads |
| `series/mid`, `series/f-mid` | h = 0.06, 500 steps, `rust_static_solve = false`, the C++ partitions: 8 × 1, 4 × 2, 2 × 4, 1 × 8 and the base at 8 × 1 and 1 × 1 |
| `series/cpp1` | C++ strict and the port, one rank, h = 0.12, 500 steps (`timing_ab.sh`) |
| `series/c1` | C++ strict and the port at 8 ranks, h = 0.06, 500 steps, MUMPS (`c1prep_campaign.sh`) |
| `digests/` | state digests: serial (`dig.sh`, finest grain, 1 / 2 / 4 / 8 threads) and MPI (`mpidig.sh`, eigen static solve) |
| `profiles/` | 1-thread profile of the base (full schedule) and an 8-rank call-graph profile of the branch |

`<series>/<label>/rep<k>/`: `run.json` (threads, cores, binary sha256,
start, load, CPU), `chrono*.csv`, `insertion_stats_rank*.csv`,
`summary*.json`, `insertions.csv`, `err.txt` (GNU `time -v`), `exit`.

## Scripts

`scripts/` (paths relative to the working directory of the machine):
`abt.sh` (serial driver runs with a thread count and a core set),
`suitet.sh` (suite runs with `--threads`), `mpirt.sh` (ranks × threads),
`dig.sh`, `digs.sh`, `mpidig.sh` (digests), `master2.sh`, `master3.sh`,
`final.sh` (the series as run), `anat.py`, `sum.py` (tables).
