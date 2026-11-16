<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The cohesive insertion check: measurements of WP-COHCHECK (2026-11-16)

Raw data of `docs/BENCHMARKS.md`, "The insertion check (performance
campaign, 2026-11-16)".

## Machines

- Confirmation series (the tables): an AWS `m7a.2xlarge` (AMD EPYC 9R14,
  8 cores, 32 GiB, Ubuntu 24.04), timing jobs alone on cores 0–3 (serial
  runs pinned to core 3) while other jobs used cores 4–7; the 8-rank series
  with the whole machine alone.
- Screening (`screening/`, `parallel-bitwise/`): an 8-core AMD EPYC Milan
  machine (Zen 3), quiet, the same method; the source of the larger-mesh
  and MPI tables, named as such there.
- Profiles (`profiles/`): shares only, from frame-pointer builds
  (`-C force-frame-pointers=yes`) on a boat.dev sandbox (serial) and on the
  screening machine (8 ranks), and flat profiles on the AWS machine.

## Binaries

- before: commit `59dd2229` (the branch point on `perf/optimisation`);
  the MPI build adds only commit `d8e01a17` (the driver's per-rank output
  files) on top of it.
- after: serial runs commit `79f0f8f0`; MPI runs commit `c429fbe3`. The one
  commit between them changes only the copy plan's rebuild on a distributed
  mesh (on a serial mesh there is one facet block and the code does the
  same work).
- Built with the default release profile of the branch and toolchain
  1.99.1; serial binaries in a boat sandbox (x86_64 Ubuntu 24.04), MPI
  binaries in the project image `akantu-rs-mumps:dev` (features `mumps`,
  `scotch`).

## Layout

`<series>/<label>/rep<k>/`: `run.json` (variant, binary sha256, start,
load, CPU), `chrono.csv` (rank 0's phase times, seconds and counts),
`chrono_rank<r>.csv` and `insertion_stats_rank<r>.csv` (after binaries
only), `summary.json`, `insertions.csv`, `err.txt` (GNU `time -v`), `exit`.
The repetitions of a series ran interleaved, the order of the variants
rotated per repetition (`scripts/` holds the scripts as run).

| series | input | runs |
|---|---|---|
| `h012-500` | `benchmarks/cohesive-perf/inputs/material-cohesive.dat`, mesh `fixtures/perf_cohesive/meshes/cube-h0.12.msh`, `max_steps = 500`, `energy_precision = 17` | 5 × 2 serial |
| `h012-5000` | the same, full schedule (5000 steps; insertions from step 710) | 5 × 2 serial |
| `screening/h006-ns-200` | the same with the x86_64 `cube-h0.06.msh` (177,328 tetrahedra, sha256 `20523d2d…`), `rust_static_solve = false`, 200 steps; screening machine | 5 × 2 serial |
| `screening/par/np{2,4,8}` | `h012-5000`'s input at 2, 4, 8 MPI ranks (Scotch, MUMPS); screening machine | 5 × 2 each |
| `parallel-bitwise/np{2,4,8}` | `h012-5000`'s input with `output_energy = true` and `rust_sparse_solver = eigen`; screening machine; digests in `digests.txt` (one copy of the identical files kept) | 1–2 × 2 each |

`h012-500` and `h012-5000` ran on the AWS machine; the larger mesh and
every MPI series ran on the screening machine only. The
`out.txt` files and, in `screening/`, `insertions.csv` and `energies.csv`
were not kept.

Summaries: `python3 scripts/analyze.py <series>` (serial) and
`python3 scripts/parana.py <root> <np>` (MPI).

## Repeatability of the MPI runs

The 8-rank runs with the Scotch partition and the MUMPS static solve are
not repeatable run to run with one binary (final cohesive elements 11,598
and 11,609 in two runs of the same binary), so the MPI runs are compared on
time only; bitwise evidence comes from the serial digests and the `par`
tier.
