<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The final campaign: data (stage 1, parts A to F)

The records of the final measurement campaign from which the tables of
`docs/BENCHMARKS.md` are copied. The driver, its method and its checks are
described in `scripts/perf/final/README.md`.

- Machines: four AWS `m7a.2xlarge` of the same type (AMD EPYC 9R14, 8
  cores, one thread per core, one NUMA node, Ubuntu 24.04.5, kernel
  7.0.0-1014-aws), one session each, nothing else running.
- Date: 2026-11-17, sessions from 10:33 to 18:59 UTC.
- Revisions: C++ reference `35a851780d0b` (strict install, JOSS emulation in
  the cohesive runs), port before `b6f56c2f0` (branch `perf/final-before`),
  port head `53de1eeef`.
- Size `full`, 5 repetitions (one in part C's series `eig`).
- Every group of every part is complete and passed its checks; each
  `summary.md` gives the counts.

| Directory | Part | Machine | Session (UTC) |
|---|---|---|---|
| `A/` | the explicit step on one core, threads, fast mode | 4 | 15:53 to 16:03 |
| `B/` | the benchmark suites, serial and at 1 to 8 ranks, Python | 3 | 10:42 to 13:14 |
| `C/` | the cohesive benchmark, series `ws`, `ins`, `full12`, `reh4`, `eig` | 2 | 10:42 to 17:55 |
| `C-reh6/` | the cohesive benchmark, series `reh6` | 1 | 10:33 to 18:58 |
| `D/` | static solves: SparseLU, MUMPS, the sparse solvers' phases | 3 | 13:14 to 16:06 |
| `E/` | memory on h = 0.04 | 4 | 10:42 to 14:07 |
| `F/` | set-up, distribution, the MPI layer | 4 | 14:07 to 15:53 |

Part C was split over two machines with `--only` to fit the day: `C/` holds
every series but `reh6`, `C-reh6/` holds `reh6`. Each directory's plan has
only its own series. The two summaries were regenerated from the raw data
with `summarize.py` after a change that lists the other directory's series
as "not planned in this data directory" where the first summaries printed
them as "missing", and that puts a blank line between the weak and strong
scaling tables. No number changed: the regenerated summaries differ from
the first ones only in those lines and in the progress log's last line
(the raw log gained one line when the first summary was written). The other
parts' summaries are the ones written at the end of each part.

## What each directory holds

Copied as written by the driver:

- `summary.md`: the part's tables (copied into `docs/BENCHMARKS.md`);
- `provenance.txt`: machine, images, toolchains, binaries' SHA-256,
  settings;
- `plan.txt`, `plan.jsonl`: every group in execution order;
- `groups.jsonl`: every group's outcome and session; `sessions.jsonl`:
  session start, boot id, CPU, kernel;
- `checks.jsonl` (parts C to F): the checks of the cohesive runs; `digests.tsv`: part
  A's state digests;
- `index.jsonl` (parts C to F): every run with its duration; `progress.log`,
  `commands.log`: progress and every command run;
- `rounds/` (part A): the timing harness's records of the five rounds;
- `mpilayer/` (part F): the MPI layer's tables (`separate.tsv`,
  `calibrated.txt`, `perf-lines.txt`).

Written by `scripts/perf/final/extract.py` from the run directories, with
nothing computed:

- `runs.jsonl` (parts C to F): one line per cohesive run: rank 0's Chrono
  totals, the run's status, the keys of `summary.json` the tables use, the
  memory sampler's peak per process, the SHA-256 of the binary that ran,
  the recorded core binding, and the SHA-256 of `energies.csv` and
  `insertions.csv`;
- `suite-records.tsv` (parts B, D, F): one line per result line of the
  suites (directory, variant, ranks, case, repetition, phase, calls,
  seconds, elements, threads, precision, digest); `suite-bindings.tsv`:
  the recorded binding of every MPI suite run.

## What was left out

Left out of the repository: the meshes and partitions, the per-run
directories (`energies.csv` and `insertions.csv`, `chrono.csv`,
`summary.json`, the memory samples, logs and traces), the suites' raw JSON
lines (their content is in `suite-records.tsv`) and the copies of the
programs. The whole of stage 1's raw data exists outside the repository; the
summaries can be regenerated from it with
`python3 scripts/perf/final/summarize.py DATA/<PART> --out FILE`, and the
compact files with `extract.py DATA/<PART> OUTDIR`.

## Still to come

Measured separately, not yet in this revision; their places in
`docs/BENCHMARKS.md` are marked `PLACEHOLDER`:

- part D's SparseLU series at `4d974854d` with the nested-dissection
  ordering (`head-nd`) and the `auto` ordering (`head-auto`);
- part G: the JOSS mesh on one 128-core node at 128, 64 and 32 ranks, both
  codes;
- the multi-node run: 128 ranks over 4 hosts, the port only.
