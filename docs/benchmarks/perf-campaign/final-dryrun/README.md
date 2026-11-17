<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Final campaign: dry run on a development machine (not results)

Every part of the final measurement campaign (`scripts/perf/final/`) run end
to end at its smallest size with 2 repetitions, from `prepare.sh` to the
generated `summary.md`, to prove the chain. **The numbers in these
summaries are not results**: a development machine (a Google Cloud VM, AMD
EPYC 7B13, Zen 3, 8 cores in two L3 domains, one NUMA node, 30 GiB, Ubuntu
24.04), the smallest sizes, two repetitions. They are not comparable with
`docs/BENCHMARKS.md` and must not be quoted.

Revisions: C++ reference `35a85178`; `before` = `perf/final-before`
(`b6f56c2f0`: the library of `59dd2229` with the final benchmark-suite
definitions); `head` = `perf/final-campaign` at `7301353f7` (with
`perf/wave2-base` `7a6ad8f42`); the campaign scripts of this directory's
commit. Date: 2026-11-17.

Per part, the directory holds `summary.md` (the generated tables),
`plan.txt` (the plan the part printed before it ran), `groups.jsonl` (the
outcome of every group), `sessions.jsonl`, `provenance.txt`,
`progress.log` and, where the part has them, `checks.jsonl` (cohesive
checks) and `digests.tsv` (part A). The raw run data stayed on the machine.

## What the dry run shows

Every part ended with status 0: every planned group complete and checked,
every binary the one its `VARIANT.json` / `CPP.json` names, every MPI run
bound as planned, the data directory free of host names and home paths.

| part | command (dry) | groups | what ran | checks | wall on the dry machine |
|---|---|---:|---|---|---:|
| A | `run_part.sh A --size dry --reps 2` | 3 | 6 configurations (the three 20 k meshes, small strain and finite deformation) × 9 programs, 2 rotated rounds | digests of the 9 programs in the 6 configurations: C++ strict = before = head at 1, 2, 4, 8 threads = the recorded digests; C++ default = its recorded digest; threads and precision of every port run as requested | 1 min |
| B | `run_part.sh B --size dry --reps 2` | 224 | every serial case of the three suites at `s` (threads and fast mode for the model cases only), the 12 distributed cases at 1/2/4/8 ranks (2 × 4, 4 × 2 for the five threaded ones), the Python suite | every case's digest equal across the bitwise variants; `static_linear_mumps` at P > 1 timed only; threads and precision as requested; every MPI run's binding as planned (184 records) | 7 min |
| C | `run_part.sh C --size dry --reps 2` | 40 | the six series on h = 0.12 (steps 100 / 800 / 1,000 / 200 / 50 / 800) at 1/2/4/8 ranks, `cpp before head` (`reh6`: `cpp head`), 2 × 4 and 4 × 2 | the binary of every run; energies and insertions at 1 rank C++ = before = head; SparseLU series at 2, 4, 8 ranks and 2 × 4, 4 × 2: before = head; bindings as planned | 21 min |
| D | `run_part.sh D --size dry --reps 2` | 18 | SparseLU and MUMPS static solves on h = 0.12, the suite's solver phases; the head's factorisation measured as not threaded (4 threads / 1 thread = 0.99) | static solutions identical per solver at 1 rank (C++ = before = head) | 3 min |
| E | `run_part.sh E --size dry --reps 2` | 8 | memory on h = 0.12, 50 steps, 1/2/4/8 ranks, `cpp before head head-release` | releasing the static solver's factors leaves the energies identical at 1 rank | 3 min |
| F | `run_part.sh F --size dry --reps 2` | 38 | set-up phases on h = 0.12 (1 step), `dist_mesh_tet4` at `s`, the MPI layer (methods A and B, 3 cases each; method B one group per pair) | as above | 4 min |
| G | `run_part.sh G --size dry --reps 2` | 8 | h = 0.06, 200 steps: port and C++ at 8 ranks, port 4 × 2; the plan printed first (`plan.txt`) | bindings as planned (rank r on core r; 4 × 2 on cores {0,1}, {2,3}, …); the strong-scaling table | 2 min |

## The breaking experiments of the review, repeated

On the same machine, with the fixed scripts (data under a scratch root, not
kept):

- **The wrong binary.** `run_part.sh D --head <the before directory>`: the
  runs labelled `head` executed the before driver (SHA-256 `4d1f175d…`, its
  copy in the volume under that SHA-256) and the provenance names before's
  revision for `head`: label and binary agree (before the fix the same
  command ran the real head, `8d92af1a…`, under before's revision). A variant
  directory whose `mpi/cohesive_extrinsic` was replaced by another build:
  the part stops before any run ("mpi/cohesive_extrinsic: SHA-256 8d92af1a…
  differs from VARIANT.json 4d1f175d…", exit 2). The driver's copy in the
  Docker volume overwritten with another build: the part stops ("the copy
  of before's driver in the volume … has SHA-256 8d92af1a…", exit 2). Every
  cohesive run also compares the SHA-256 it recorded with the expected one.
- **A fake digest.** A head whose `suite_solid` rewrites the digest of
  `explicit_tet4_ss`: that group fails its check, the summary's row reads
  "check failed" in every column (no timing printed), "29 of 30 groups
  tabulated", the part exits 1.
- **Deleted runs.** The records of the 4-rank groups of part E removed: the
  four rows stay in the table with "missing" in every column.
- **Kill and resume.** Part E (2 repetitions) killed after 5 of 8 groups,
  resumed by the same command: the second session ran the 3 missing groups;
  the rows whose two repetitions came from different sessions carry ‡, the
  8-rank rows (both repetitions in the first session) do not; the summary
  lists the two sessions with their boot id and CPU model.
- **A permuted core list.** The binding check of a recorded 4-rank run
  against the planned list passes, against the list with cores 0 and 1
  exchanged it fails ("rank 0: cores [0], planned [1]").
- **A busy machine.** A busy loop on one core when a part starts: the idle
  check waits, then the part stops ("the machine is not idle (busy=1.03
  cores …)", exit 1).
- **Threaded SparseLU, both ways.** The probe of `prepare.sh` on the head
  and before: 4 threads take 1.00 of the 1-thread `factorize` time, not
  threaded; on `suite_solid` built from `perf/sparse-lu-threads`: 0.784,
  threaded (at 2 threads that branch gave 0.858, too close to a threshold,
  hence 4 threads and 0.9).

## Estimated machine time at full settings

`run_part.sh <PART> --plan-only` (`plan.py`, where the basis of every
figure is written: the walls of the earlier series on Zen 4 / Zen 5
machines, the head's ratios to `before`, about 4 s of overhead per
cohesive run as in the dry run, part A's full round measured here at
122 s, part G from `g-estimates.txt`). An 8-core Zen 4 machine for parts
A to F with 5 repetitions, a 128-core node for part G:

| part | series | expected |
|---|---|---:|
| A | digests, 5 rounds | 0.2 h |
| B | `solid` 2.3 h, `models` 0.9 h, `setup` 0.1 h, `mpi` 1.0 h, `python` 0.1 h | 4.4 h |
| C | `ws` 2.3 h, `ins` 0.5 h, `full12` 1.6 h, `reh6` (C++ and head only) 7.9 h, `reh4` 3.1 h, `eig` 0.1 h | 15.4 h |
| D | `eig1` 1.9 h, `mumps` 0.3 h, `suite` 0.3 h (+0.8 h if the head's factorisation is threaded) | 2.6 h |
| E | `mem4` | 3.7 h |
| F | `setup` 1.4 h, `dist` 0.2 h, `mpilayer` 0.3 h | 1.9 h |
| A to F | | 28.2 h |
| G | partitions, then 10 runs (plan in `scripts/perf/final/README.md`) | 7.0 h |

The budget cuts of the review: `reh6` without `before` (11.5 → 7.9 h),
the suites' threads and fast-mode variants only for the cases whose timed
phases run such code (part B 5.1 → 4.4 h). `--variants` cuts a part further
without editing it (for example `--variants cpp,head`).

### Proposed split of parts A to F over four 8-core machines

| machine | commands | expected |
|---|---|---:|
| 1 | `run_part.sh C --only reh6` | 7.9 h |
| 2 | `run_part.sh C --only ws,ins,full12,reh4,eig --data <other root>` | 7.5 h |
| 3 | `run_part.sh B`; `run_part.sh D` | 7.0 h |
| 4 | `run_part.sh E`; `run_part.sh F`; `run_part.sh A` | 5.8 h |

Two calls of one part with different `--only` on two machines write two
data directories `C` (give one of them another `--data` root) and are
summarised separately; a resumed part marks the medians that combine
sessions (‡), and the summary shows a change of host.
