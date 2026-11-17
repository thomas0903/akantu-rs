<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Final campaign: dry run on a development machine (not results)

Every part of the final measurement campaign (`scripts/perf/final/`) run end
to end at its smallest size with 2 repetitions, from `prepare.sh` to the
generated `summary.md`, to prove the chain. **The numbers in these
summaries are not results**: a development machine (a Google Cloud VM, AMD
EPYC 7B13, Zen 3, 8 cores in two L3 domains, 30 GiB, Ubuntu 24.04), the
smallest sizes, two repetitions. They are not comparable with
`docs/BENCHMARKS.md` and must not be quoted.

Revisions: C++ reference `35a85178`; `before` = `perf/final-before`
(`b6f56c2f0`: the library of `59dd2229` with the final benchmark-suite
definitions); `head` = `perf/final-campaign` (`7301353f7`, `perf/wave2-base` at `7a6ad8f42` merged). Date: 2026-11-17.

Per part, the directory holds `summary.md` (the generated tables),
`provenance.txt`, `progress.log` and, where the part has them,
`checks.jsonl` (cohesive checks), `digests.tsv` (part A) and `plan.txt`
(part G). The raw run data stayed on the machine.

## What the dry run shows

Every part ended with status 0: every point complete, every check passed,
the data directory free of host names and home paths.

| part | command (dry) | what ran | checks | wall on the dry machine |
|---|---|---|---|---:|
| A | `run_part.sh A --size dry --reps 2` | 6 configurations (the three 20 k meshes, small strain and finite deformation) × 9 programs, 2 rotated rounds | digests of the 9 programs in the 6 configurations: C++ strict = before = head at 1, 2, 4, 8 threads = the recorded digests; C++ default = its recorded digest; threads and precision of every port run as requested | 1 min |
| A, full size | `run_part.sh A --size full --reps 1` | the ten configurations × 9 programs, 1 round | the same on all ten configurations (90 digests): equal (head build of this branch before the base merge) | 3 min |
| B | `run_part.sh B --size dry --reps 2` | every case of the three suites at `s` × 7 variants, the 12 distributed cases at 1/2/4/8 ranks and 2 × 4, 4 × 2, the Python suite | every case's digest equal across the bitwise variants (before, head at 1, 2, 4, 8 threads; Python before and head); `static_linear_mumps` at P > 1 timed only (its distributed solve does not repeat run to run); threads and precision recorded as requested; `partition_scotch_tet4` skipped by the host build (it runs in the MPI series) | 3 min |
| C | `run_part.sh C --size dry --reps 2` | the six series on h = 0.12 (steps 100 / 800 / 1,000 / 200 / 50 / 800) at 1/2/4/8 ranks, `cpp before head`, 2 × 4 and 4 × 2 | 40 points, 14 bitwise comparisons (energies and insertions at 1 rank: C++ = before = head; SparseLU series at 2, 4, 8 ranks and 2 × 4, 4 × 2: before = head), 134 runs with the planned binding | 21 min |
| D | `run_part.sh D --size dry --reps 2` | SparseLU and MUMPS static solves on h = 0.12, the suite's solver phases; the head's factorisation is not threaded (`sparselu_threaded: false`) | static solutions identical per solver at 1 rank (C++ = before = head); bindings as planned | 3 min |
| E | `run_part.sh E --size dry --reps 2` | memory on h = 0.12, 50 steps, 1/2/4/8 ranks, `cpp before head head-release` | releasing the static solver's factors leaves the energies identical at 1 rank; bindings as planned | 3 min |
| F | `run_part.sh F --size dry --reps 2` | set-up phases on h = 0.12 (1 step), `dist_mesh_tet4` at `s`, the MPI layer (methods A and B, 3 cases each) | as above; method A and B values for raw MPI, before and head | 3 min |
| G | `run_part.sh G --size dry --reps 2` | h = 0.06, 200 steps: port and C++ at 8 ranks, port 4 × 2; the plan printed first (`plan.txt`) | bindings as planned (rank r on core r, 4 × 2 on cores {0,1}, {2,3}, …); the strong-scaling table | 2 min |

What the dry run found and fixed in the tools before these runs: the C++
harness and the port's campaign runs left the ranks unbound inside their
container's cores (the earlier campaigns' method); both now bind one rank
per core (`PERF_MPI_BIND=core`), and the recorder checks it. Open MPI 5
starts every rank on the first core and binds it during start-up, so the
recorder keeps the run's last complete snapshot. The distributed MUMPS solve
of `static_linear_mumps` gives another digest in every run at 4 ranks (two
runs of `before`), as the cohesive MUMPS runs at P > 1 do.

Two `before` details: the suites' case definitions were changed after the
public `main` (digests extended to the timed work, the implicit model cases
pinned to SparseLU), so `before` is a branch with those suite commits on
the library of `59dd2229` (`scripts/perf/final/README.md`, "Variants"); and
`explicit_step_dist_tet4` of `before` lacks the agreement counts (the commit
that added them needs a library function of the campaign).

## Estimated wall time at full settings

On an 8-core Zen 4 machine (AWS `m7a.2xlarge`, AMD EPYC 9R14) for parts A
to F with 5 repetitions, on a 128-core node for part G with one run per
point. Basis: for part A, one full round measured on the development
machine (122 s for the ten configurations and nine programs, 72 s for the
full digest check; Zen 4 is not slower); for parts B to F, the run times
of the earlier series on Zen 4 / Zen 5 machines (the suite baselines, the
scaling series, the mid-size rehearsal, the static-solve series in
`docs/benchmarks/`), with the head's measured ratios to `before` for the
port's runs, plus the per-run overhead seen in the dry run (about 3 s per
cohesive run: containers, copies, the analysis); for part G,
`scripts/perf/final/g-estimates.txt`.

| part | series | per repetition | full part (5 rep.) | basis |
|---|---|---:|---:|---|
| A | digests, rounds | 2 min (+1 min digests once) | 0.2 h | measured: one full round 122 s, digests 72 s |
| B | serial suites (7 variants) | 44 min | 3.7 h | `before` 533 s of phases per repetition (baseline, size `m`), `head` ≈ 0.7×, threads and fast ≈ 0.6×, ~0.5 s per process |
| B | distributed cases | 16 min | 1.3 h | 120 container runs of 3–10 s |
| B | Python | 1 min | 0.1 h | |
| C | `ws` | 30 min | 2.5 h | scaling series walls (C++ 787 s, port 389 s per repetition), head ×0.75, 32 runs |
| C | `ins` | 7 min | 0.6 h | scaling series `ins` walls |
| C | `full12` | 20 min | 1.7 h | C++ 5,000 steps with the JOSS emulation ≈ 230 s at 1 rank; port from the insertion-step tables |
| C | `reh6` | 2.3 h | 11.5 h | mid-size rehearsal walls (C++ 3,670 s, port 2,557 s over 1/2/4/8 ranks), head ×0.62 |
| C | `reh4` | 38 min | 3.2 h | mid-size rehearsal h = 0.04 walls, C++ at 8 ranks ≈ 150 s (it no longer hits a 14 GiB limit) |
| C | `eig` | 8 min (once) | 0.15 h | one repetition |
| D | `eig1`, `mumps`, `suite` | 32 min | 2.7 h | h = 0.06 SparseLU: C++ 249 s, before 822 s, head 209 s per repetition |
| E | `mem4` | 47 min | 3.9 h | as `reh4` plus the `head-release` run |
| F | `setup`, `dist`, `mpilayer` | 30 min | 2.5 h | 1-step runs on h = 0.06 and 0.04; `dist_mesh_tet4` at `l`; methods A and B |
| G | `full` | | 7.0 h | `g-estimates.txt` (128-core plan in `scripts/perf/final/README.md`) |

Parts A to F: about 34 machine-hours. If the head's SparseLU factorisation
uses threads (`VARIANT.json`), part D adds about 0.8 h.

### Proposed split of parts A to F over four 8-core machines

Each line is one machine, its parts run one after the other
(`run_part.sh … --only` selects series of a part; every part keeps its own
data directory):

| machine | commands | expected |
|---|---|---:|
| 1 | `run_part.sh C --only reh6` | 11.5 h |
| 2 | `run_part.sh C --only ws,ins,full12,reh4,eig`; `run_part.sh A` | 8.5 h |
| 3 | `run_part.sh B`; `run_part.sh D` | 7.8 h |
| 4 | `run_part.sh E`; `run_part.sh F` | 6.4 h |

Machine 1 sets the elapsed time: the full 5,000-step schedule on
h = 0.06 at 1 to 8 ranks for three codes takes 2.3 h per repetition (the
1-rank C++ run alone 27 min). With `--reps 3` for this series only it
takes 6.9 h, at the cost of the five-repetition rule; with a fifth machine
the series `ws` and `full12` can move to it (machine 2: 4.3 h).

Two runs of the same part on two machines with `--only` (for example `C`
split as above) write two data directories `C`; give one of them
`--data` elsewhere and summarise each.
