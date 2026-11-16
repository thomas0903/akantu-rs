<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The cohesive insertion step: measurements (performance campaign, 2026-11-17)

Raw data of `docs/BENCHMARKS.md`, "The insertion step and the mesh
operations under it".

## Machine and method

- An 8-core AMD EPYC Milan (Zen 3) development machine, 15 GiB, Ubuntu
  24.04, alone (no other job during the series). Serial runs pinned to core
  3 (`taskset`), suite runs to core 2, MPI runs in the project image
  `akantu-rs-mumps:dev` with rank `r` bound to core `r`.
- Before and after interleaved, order rotated per repetition, 5
  repetitions (`scripts/final.sh`). Medians [min, max] over the
  repetitions; ratios are the median [min, max] of the per-repetition
  paired ratios (`scripts/analyze.py`, `scripts/sum.py`).
- before: `perf/wave2-base` at `0769542f6`; after: `perf/cohesive-insert`
  at `74e97f334` (it contains `0769542f6`). Both built with the default
  release profile, toolchain 1.99.1; serial binaries on the host
  (`scripts/build.sh`), MPI binaries in the MUMPS image with the features
  `mumps,scotch` (`scripts/build-par.sh`).

## Series

| directory | input | runs |
|---|---|---|
| `series/h012-500` | `benchmarks/cohesive-perf/inputs/material-cohesive.dat` with `max_steps = 500`, `energy_precision = 17`, mesh `fixtures/perf_cohesive/meshes/cube-h0.12.msh` (23,156 tetrahedra) | 5 × 2 serial |
| `series/h012-5000` | the same, full schedule (5000 steps, insertions from step 710) | 5 × 2 serial |
| `series/h006-ns-200` | the same with the x86_64 `cube-h0.06.msh` (177,328 tetrahedra), `rust_static_solve = false`, 200 steps | 5 × 2 serial |
| `series/par/np{2,4,8}` | `h012-5000`'s input at 2, 4, 8 ranks (Scotch partition, MUMPS static solve) | 5 × 2 each |
| `series/suite-setup` | `suite_setup --case facets_tet4,facets_hex8,groups_tet4,cohesive_insert_tet4 --size m` | 5 × 2 |
| `series/suite-solid` | `suite_solid --case cohesive_extrinsic_check,cohesive_extrinsic_insertion --size m` | 5 × 2 |
| `bitwise/` | state digests: serial (energies of every step, insertions, nodal state of the last step) and MPI at 2, 4, 8 ranks with `rust_sparse_solver = eigen` (`energies.csv`, `insertions.csv`) | 1 × 2 each |

`<series>/<label>/rep<k>/` holds `run.json` (binary sha256, start, load,
CPU), `chrono.csv` / `chrono_rank<r>.csv` (phase times, seconds and
counts), `insertion_stats_rank<r>.csv` (the split of the check),
`summary.json`, `insertions.csv`, `err.txt` (GNU `time -v`) and `exit`;
`out.txt` and the empty `diag/` directories were not kept. The MPI runs
with the MUMPS static solve do not repeat bit for bit from run to run (also
on the base commit), so they are compared on time only; the bitwise
evidence at P > 1 is `bitwise/`.

## Profiles

`profiles/`: `perf record -g` with frame-pointer builds
(`-C force-frame-pointers=yes`), shares of samples, not times:
`serial-base-5000-*` (the branch point) and `serial-c3-5000-*` (after the
restricted check, the group journal, the copy-plan update and a fused
facet-stress and law check that was later removed) and
`serial-final-5000-*` (the final commit) of the full serial schedule; `np8-p8-c8-*` and
`np8-p8-c9-*` (whole machine, 8 ranks, before and after the
global-number fix); `setup-*` (flat, the set-up suite's facet and insertion
cases; `setup-facets-insert-binary-search-flat.txt` is the first
`plan_facets` attempt, slower than the base, see "What did not pay" in
`docs/BENCHMARKS.md`).

## Scripts

`scripts/` holds the scripts as run (paths relative to the working
directory `~/w/insert/` of the machine): `final.sh` (the series),
`ab.sh` (one serial repetition), `par-ab.sh`, `par-run.sh` (MPI),
`suite.sh` (suite A/B), `dig.sh` and `parbit.sh` (bitwise digests),
`prof.sh`, `prof8.sh` (profiles), `analyze.py`, `sum.py` (tables).
