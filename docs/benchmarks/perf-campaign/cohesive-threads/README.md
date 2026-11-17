<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The cohesive model on threads: measurements (performance campaign, 2026-11-17)

Raw data of `docs/benchmarks/perf-campaign/README.md` (moved from
`docs/BENCHMARKS.md`), "The cohesive model on threads".

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
- before (`base`): `perf/wave2-base` at `d404f4c70`; after: the branch at
  `6fb246686` (the series `r3-*`, the digests `*-6fb24668.txt`; later
  commits change documents and tests only). The C++ comparisons
  (`cpp1`, `c1`) ran with the branch at `ac3b3179`. Default release
  profile, toolchain 1.99.1; host binaries (`scripts/build.sh`), MPI
  binaries in the MUMPS image with `mpi,scotch,mumps`.

## Series

| directory | what |
|---|---|
| `series/r3-h012-5000` | driver, h = 0.12 full schedule: per repetition the base at one thread and the branch at 1, 2, 4, 8 threads (whole machine) |
| `series/r3-h012-500` | 500 steps (no insertion), the base and the branch at one thread |
| `series/r3-suite-r*` | `suite_solid --case cohesive_extrinsic_check,cohesive_extrinsic_insertion --size m`, the base at one thread and the branch at 1, 2, 4, 8 threads (one directory per repetition) |
| `series/r3-mid` | h = 0.06, 500 steps, `rust_static_solve = false`, the C++ partitions: 8 × 1, 4 × 2, 2 × 4, 1 × 8 and the base at 8 × 1 |
| `series/var500` | the one-thread law check's variants, 500 steps: `head` (`9bcb0526`, the loop outlined, packed norms), `B` (the same loop inlined into the check), `E` (the loop per dimension, as committed in `6fb246686`); 7 repetitions, the last 4 beside builds on the other cores |
| `series/cpp1` | C++ strict and the port, one rank, h = 0.12, 500 steps (`timing_ab.sh`) |
| `series/c1` | C++ strict and the port at 8 ranks, h = 0.06, 500 steps, MUMPS (`c1prep_campaign.sh`) |
| `digests/` | state digests: serial (`dig.sh`, 1 / 2 / 4 / 8 threads at the finest and the automatic grain) and MPI (`mpirt.sh` with `in-5000-eigen.dat`, ranks × threads, both grains for the threaded runs, the base rebuilt) |
| `profiles/` | 1-thread profile of the base (full schedule), an 8-rank call-graph profile of the branch, and `onethread-instructions-*` (`perf record -e instructions:u`, the base and `9bcb0526`: the law check's instructions, `perf stat` totals) |

`<series>/<label>/rep<k>/`: `run.json` (threads, cores, binary sha256,
start, load, CPU), `chrono*.csv`, `insertion_stats_rank*.csv`,
`summary*.json`, `insertions.csv`, `err.txt` (GNU `time -v`), `exit`.

## Scripts

`scripts/` (paths relative to the working directory of the machine):
`abt.sh` (serial driver runs with a thread count and a core set),
`suitet.sh` (suite runs with `--threads`), `mpirt.sh` (ranks × threads),
`dig.sh`, `digs.sh`, `mpidig.sh` (digests), `master3.sh` (the C++
series), `master5.sh` and `master6.sh` (the final series, digests and
gates), `var.sh` (the variants' builds), `asmstat.sh` (packed and scalar
operations of the law check's functions in the cohesive crate's object
code), `pf.sh` (profiles), `anat.py`, `sum.py` (tables; the suite's five
directories merged into one with `rep<k>` names first).
