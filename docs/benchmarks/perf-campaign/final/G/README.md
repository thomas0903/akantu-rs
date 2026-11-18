<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Part G: the full-size run on one 128-core machine

The JOSS mesh (4,392,183 tetrahedra), 5,000 steps with insertion, both
codes at 128, 64 and 32 ranks and the port at 64 × 2, 32 × 4 and 16 × 8
threads, one run per point, on one machine: AWS `m7a.32xlarge`, AMD EPYC
9R14, 128 cores, one thread per core, two NUMA nodes of 64 cores, 493 GiB
visible, Ubuntu 24.04.5, kernel 7.0.0-1014-aws, Docker 29.1.3, Open MPI
5.0.7; nothing else running (`provenance.txt`).

- Sessions (UTC): 2026-11-17 22:35 to 2026-11-18 03:18 (every point), and
  03:18 to 03:38 (the 128-rank port point repeated, below).
- Revisions: C++ reference `35a851780d0b` (strict install, JOSS
  emulation); port head `740a79a36` (driver SHA-256 `90d301c9…`); the
  driver checkout `007900416` in the first session and `6b769694c` in the
  second (`provenance.txt`). The `before` variant has no point in part G.
- Placement `fill`: rank r with T threads on the r-th block of T cores,
  NUMA node 0 filled before node 1. The 32- and 64-rank runs therefore use
  one NUMA node (one socket's memory), the 128-rank and the 128-core hybrid
  runs both (the NUMA column of `summary.md`).
- Both codes replay the same Scotch partition per rank count, exported
  first by the C++ harness (`AKA_SEED = 1`, one Scotch thread).
- The 8-rank point of the default list was not planned (13 groups: four
  partition exports and nine runs, `plan.txt`).

Every rank binds itself to its cores
(`benchmarks/cohesive-perf/scripts/in-container/bind-rank.sh`, started by
`mpirun --bind-to none`). A first attempt on the same machine (2026-11-17,
22:04 to 22:13 UTC, launchers of `740a79a36`) exported the partitions and
then failed at the start of every run, because Open MPI 5.0.7's
`--bind-to core` cannot start more than 10 ranks per host in a Docker
container with the default seccomp profile (`set_mempolicy` refused
without `CAP_SYS_NICE`); its data is not kept.

## The first 128-rank sample of the port

The first run of the point `128:head` (2026-11-17, 22:35 to 22:54 UTC)
completed and was correct, but its binding record was written while 12 of
the 128 ranks were exiting (their rank not readable from `/proc`); the
check of that day's `bindings.py` failed and the group was repeated in a
second session with the recorder fix of `6b769694c` (keep the last
snapshot in which every rank is known). The table's 128-rank port run is
the repeated one. The first sample is kept as a second sample
(`first-128head/runs.jsonl`, `first-128head/*.bindings.json`): loop 825.5 s
against the table run's 835.3 s, wall 1,116 s against 1,174 s (static
solve 271 s against 319 s) (`../multinode/table.md`). `checks.jsonl` keeps
the failed check of the first sample, so `summary.md` counts "1 points
failed"; the repeated group passed (`groups.jsonl`, last record of the
point: `done`), and only `done` groups are tabulated.

## Bindings

`scripts/perf/final/bindings.py check` at this revision, over every
recorded binding of part G (`bindings/`), the first sample and the final
one-machine run of `../multinode/one-machine-final/`:

    python3 -I scripts/perf/final/bindings.py check FILE NP THREADS "$(seq -s, 0 127)"

Every one is as planned (exit 0): the nine runs of the table, the first
128-rank sample (the final check matches its 12 unreadable ranks by their
cores) and the final one-machine run.

## Files

As parts A to F (`../README.md`): `summary.md`, `provenance.txt`,
`plan.txt`, `plan.jsonl`, `groups.jsonl`, `sessions.jsonl`, `checks.jsonl`,
`index.jsonl`, `progress.log`, `commands.log` copied by
`scripts/perf/final/extract.py`, which also wrote `runs.jsonl`; and

- `bindings/`: each run's binding record (the input of the check above);
- `first-128head/`: the first 128-rank sample (`extract.py` over its run
  directory) and its binding record;
- `profile/`: the port (`740a79a36`) at 64 and 128 ranks, 2,000 steps, on
  the same machine after part G (2026-11-18, 03:38 to 03:58 UTC): system-
  wide `perf` cycle samples by shared object (`perf-np*.dso.txt`) and by
  symbol (`perf-np*.symbols.txt`, the first 140 lines, columns padded with
  two spaces), the campaign logs of the two runs.

`summary.md` was regenerated with `python3 scripts/perf/final/summarize.py
DATA/G --out summary.md`; it differs from the summary written at the end of
the second session only in the progress log's last line (the log gained
its "part G end" line after that summary was written). Its "loop s" column
is `solve_step` + check; `docs/BENCHMARKS.md` and `../multinode/table.md`
also give the loop with the energies and boundary conditions (the driver's
`loop_time_s`).

Profile shares (cycles, all processes of the machine):

| | 64 ranks | 128 ranks |
|---|---:|---:|
| the port's driver | 68.96 % | 50.99 % |
| kernel | 13.08 % | 20.95 % |
| libc | 13.12 % | 15.03 % |
| Open MPI (`libopen-pal`, `libmpi`) | 4.45 % | 12.42 % |
| `write_pairs` (insertion check) | 20.53 % | 15.27 % |
| `Linear::check_serial_dim` | 13.20 % | 10.69 % |
| `__sched_yield` | 2.10 % | 3.96 % |

Left out, as in parts A to F: the partitions (469 MB), the per-run
directories (logs, per-rank records, memory samples, energies and
insertions) and the profiles' `perf.data`.
