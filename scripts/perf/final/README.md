<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The final measurement campaign

The C++ reference, the port before the performance campaign and the port at
a head under test, measured on the same machine in the same session, for
every table of `docs/BENCHMARKS.md` and every claim of the performance
campaign. Seven independent parts (A to G); each builds nothing, runs alone
on its machine, writes its own data directory and can run on a different
machine of the same type than the other parts.

| File | What |
|---|---|
| `prepare.sh` | builds every binary of every variant once (before any timing) |
| `plan.py` | what a part runs: every group in execution order with its variants and expected time (`plan.jsonl`); read by the runner, `--plan-only` and the summaries |
| `run_part.sh` | one part, unattended and resumable: executes the plan |
| `summarize.py` | a part's data directory → its `summary.md` (run at the end of every part) |
| `lib.sh` | shared helpers (provenance, scrubbing, wrappers) |
| `digest-exceptions.txt` | suite cases not digest-compared: definition changed between the revisions, or not repeatable at a rank count |
| `bindings.py` | records and checks the rank → cores binding of every MPI run |
| `g-estimates.txt` | part G's expected memory and time per run, with their basis |

The dry run of every part on a development machine (not results) is in
`docs/benchmarks/perf-campaign/final-dryrun/`, with the time each part took
there and the estimates for the real campaign.

## Variants

| label | what |
|---|---|
| `cpp-strict`, `cpp` | the C++ reference `35a85178`, oracle install `install-strict` (`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`, the port's floating-point operations) |
| `cpp-default` | the same with upstream's flags (part A only, as in the document) |
| `before` | the port before the campaign: `--before REV` of `prepare.sh`; use the branch `perf/final-before` (`b6f56c2f0`): the library of `59dd2229` (= the public `main`, byte for byte in every crate but `akantu-bench`), the cohesive driver and explicit-step harness of `59dd2229`, and the benchmark suites with their final case definitions (`91cb54c64` plus the suite-only commits `73b8999d5`, `6743820cb`, `82325c03a`, `9f3149c5b`, `0a94566bb`, `f373ab7ce`, `50fe7e996`, cherry-picked; `26d0f8365` needs a library function of the campaign and is left out: `explicit_step_dist_tet4` of `before` does not report the agreement counts). What differs from the head's suites is the execution-policy plumbing (`--threads`, `--grain`) and cases added later (they run without `before`) |
| `head` | the port at the revision under test (`--head REV`), default bitwise policy, one thread |
| `head-t2`, `-t4`, `-t8` | `head` with `AKANTU_NUM_THREADS = N`, threads on cores 0..N−1; in the cohesive and MPI parts `head-tN` at P ranks is P ranks × N threads (`mpirun --map-by slot:PE=N --bind-to core`) |
| `head-fast`, `head-fast-t8` | `head` with `AKANTU_PRECISION=fast` (compared only through its tolerance rules, never digested as bitwise) |
| `head-eig`, `before-eig`, `cppeig` | the static solve with each code's SparseLU instead of MUMPS |
| `head-release` | `head` with `rust_release_static_solver = true` (the static solver's factors released after the preload) |

Variants are directories prepared by `prepare.sh`, never hard-coded:
`run_part.sh --before DIR --head DIR` takes any two prepared directories.

## What a fresh machine needs before `prepare.sh`

An 8-core x86_64 Ubuntu 24.04 host (an AWS `m7a.2xlarge`, 32 GiB; part G an
`m7a.32xlarge`, 128 cores, 512 GiB, or the `r7a.32xlarge` with 1 TiB) with,
as on the campaign's machines:

1. Docker, git, Python 3 with `pip` (`python3-pip`; `python3-venv` is not needed) (network access to PyPI for
   `maturin`, `numpy`, `scipy`), GNU `time`, `taskset`, `lscpu`, `flock`;
   rustup for the user (the toolchain of `rust-toolchain.toml` installs
   itself).
2. The project images `akantu-rs-mpi:dev` and `akantu-rs-mumps:dev`
   (`docker/`, `docs/TESTING.md`), and the oracle image `akantu-oracle:dev`
   (`oracle/docker/`).
3. The Docker volume `akantu-oracle-build` with `install-strict` and
   `install-default` (`docs/ORACLE_BUILD.md`, both with `READY`): about 3 h
   of build at `-j2` on 8 cores if it has to be made; copy it from a machine
   that has it (`docker run -v akantu-oracle-build:/b ... tar`).
4. Optional, saves time: the volume `akantu-perf-cohesive` with the C++
   cohesive harness builds (`/perf/builds/{strict,default}`; otherwise
   `prepare.sh` builds them with `benchmarks/cohesive-perf/scripts/build.sh`)
   and `~/baseline/` (`cpp-bin/` explicit-step harness, `meshes/explicit/`,
   `meshes/cohesive/`, `partitions/h0.06/`); without `~/baseline/` the
   explicit-step meshes need gmsh 4.12.1 (`make_meshes.sh`; their SHA-256 is
   checked) and the partitions are exported by the campaign.
5. Part G only: the JOSS mesh `cube.msh` (SHA-256 `5ad845c7…`,
   `benchmarks/cohesive-perf/meshes/cube-joss.json`), passed to
   `prepare.sh --joss-mesh FILE`.
6. A clone of this repository at this branch or later (the driver
   checkout), with both revisions in its history.

Disk: about 25 GB for the two variants' binaries, build trees removed
after each build (`--keep-targets` keeps them).

```sh
git clone -b perf/final-campaign <repository> akantu-rs && cd akantu-rs
scripts/perf/final/prepare.sh --before perf/final-before --head <REV> [--joss-mesh cube.msh]
```

`prepare.sh` checks the oracle installs and the cohesive harness builds,
copies or builds the C++ explicit-step harness, checks the meshes (SHA-256
against the recorded files) and generates the scaling meshes h = 0.0925,
0.0756, 0.0476 with the oracle image's Gmsh, builds the raw-MPI baseline,
and per variant: the host binaries (`explicit_step_timing`, the three
suites), the MPI binaries in the images (`cohesive_extrinsic` and
`suite_solid` with `mpi,scotch,mumps`, the suites with `mpi,scotch`,
`mpi_bench`), a Python environment with the extension; `VARIANT.json`
records revision, toolchain, image ids, the SHA-256 of every binary and of
the Python extension, and whether the variant's SparseLU factorisation uses
threads, measured (`sparselu_probe`: the `factorize` phase of
`suite_solid static_linear_eigen` at size `m` at 1 thread and at 4, the
faster of two alternated runs each; threaded below 0.9. On the development
machine the threaded factorisation of `perf/sparse-lu-threads` gave 0.784,
the head and before 1.00); part D times the threaded variants only then
(`run_part.sh --sparselu-threads true|false` overrides it).

## Running a part

```sh
scripts/perf/final/run_part.sh C --plan-only                 # groups, runs, expected time per series
nohup scripts/perf/final/run_part.sh C --reps 5 --size full > C.out 2>&1 &
tail -f ~/final/data/C/progress.log
```

`--plan-only` works for every part without running anything (and without
prepared binaries: the suites' case lists then come from `plan.py`);
`FINAL_PLAN_CORES=128 FINAL_PLAN_MEMORY=481g` plans for another machine.
`--variants` keeps only the variants matching its patterns (`cpp,head`,
`before,head*`), `--only` selects series: a part can be cut down or split
over machines without editing it.

The part first checks every binary of both variant directories against
their `VARIANT.json`, the C++ binaries against `CPP.json`, and copies the
port's MPI driver into the Docker volume under a path named by its SHA-256
(`/target/final/sha/<sha>/`), which is what every cohesive run executes;
any mismatch stops the part (exit 2). It then writes its plan
(`plan.jsonl`, `plan.txt`) and executes it group by group: a group is the
variants of one configuration in one repetition, in the order they run
(rotated by the repetition). Before each group the machine must be idle (no
`final-*` container, less than half a core busy over a second; it waits up
to 10 minutes, then the part stops). Every outcome (`done`, `check_failed`,
`failed`, `skipped`) goes to `groups.jsonl` with the session
(`sessions.jsonl`: UTC start, boot id, CPU model); a `done` group leaves a
marker, and calling the same command again repeats every other group
(after a reboot, a stopped host, an interrupted ssh). Exit status 0 only if
every group is complete and every check passed. Then `summarize.py` writes
`~/final/data/<PART>/summary.md` (only `done` groups are tabulated; every
planned point has its row, "missing", "check failed", "failed" or "skipped"
when it has no data; a ‡ marks a median whose repetitions come from more
than one session, and the summary says when the host changed), and
every text file of the data directory is scrubbed (data root written
`<root>`, home `~`, host name `<host>`) and checked for host names and
addresses. Copy the data directory into
`docs/benchmarks/perf-campaign/final/` and run
`cargo test -p akantu-validation --test public_tree_guard`.

Method (COMMON.md section 4, `docs/BENCHMARKS.md` "Method"): nothing is
built while a part runs; one part at a time on a machine; serial runs on
core 2, N threads on cores 0..N−1; the cohesive runs of both codes (parts
C to G) bound one rank per core (`PERF_MPI_BIND=core` of the campaign
tools), rank r of a run with T threads per rank on the r-th block of T
cores of the host's cores ordered NUMA node by NUMA node (`fin_core_order`
in `lib.sh`: ranks fill one NUMA node before the next, the threads of a
rank sit on adjacent cores of one node); the suites' MPI runs likewise
inside a container restricted to cores 0..P×T−1; the
variants of a point interleaved, their order rotated per repetition; at
least 5 repetitions in parts A to F (`--reps`); medians with [min, max];
ratios paired per repetition. Both codes read the same mesh file and, at
P > 1, the same Scotch partition (exported by the C++ harness, replayed by
the port).

Checks that stop a group from counting (it is repeated on the next call;
the part exits 1):

- the binary that ran: every cohesive run records the SHA-256 of what it
  executed (the port: `binary.sha256`, C++: `run.json`), compared with
  `VARIANT.json` / `CPP.json`;
- state digests: part A, the FNV-1a digest of `u`, `v`, `a`, `f_int` of
  every bitwise program (C++ strict, before, head at 1/2/4/8 threads) equal
  to the recorded `docs/benchmarks/explicit-step/final/digests.txt`
  (C++ default to its own recorded digest); the suites, every bitwise
  variant's digest per case equal to `head`'s (`before` too, except the
  cases of `digest-exceptions.txt`); the cohesive parts, `energies.csv`
  and `insertions.csv` (17 digits) identical across the bitwise variants
  at 1 rank (C++ included: the same MUMPS or SparseLU on the same matrix)
  and in the SparseLU series at every rank count (series `eig` of part C);
- the thread count and precision recorded in every result line (the
  harness's meta line, the suites' `threads` / `precision`, the driver's
  `rust_exec` in `summary.json`; all three are the process default policy,
  read from the environment) equal to the requested ones (a `before` build
  records none: it has one thread and the bitwise path); the per-step
  energies and insertions must exist for every bitwise variant;
- a C++ run that took upstream's MUMPS retry path (UD-216; the harness
  reports it, `status.txt` outcome `ud216`): the whole point is repeated,
  at most twice;
- the core binding of every MPI run (cohesive runs and the suites'
  distributed cases): `bindings.py` reads each rank's `Cpus_allowed_list`
  from `/proc` every 50 ms during the run and keeps the last complete
  snapshot (Open MPI binds a rank during its start-up; rank from
  `OMPI_COMM_WORLD_RANK`), and the check requires the planned binding above
  (`*.bindings.txt`: rank → cores, NUMA node); a suite run shorter than 2 s
  (the dry sizes) is marked "not recorded";
- fast-mode runs are timed, never digested; cases whose result does not
  repeat from run to run at a rank count (the distributed MUMPS solve:
  `static_linear_mumps` at P > 1, the cohesive MUMPS runs at P > 1) are
  timed only (`digest-exceptions.txt`, lines starting with `!`).

## The parts

| part | series (`--only`) | what | full settings |
|---|---|---|---|
| A | `digests`, `rounds` | the ten configurations of `scripts/perf/explicit-step/` (`timing_rotated.sh`), programs `cpp-strict cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8`, phases `step`, `fint`, `stress` | 5 rounds |
| B | `solid`, `models`, `setup`, `mpi`, `python` | every serial suite case at size `m`: `before head`, and `head-t2 head-t4 head-t8 head-fast head-fast-t8` only for the cases whose timed phases run threaded or fast-mode code (the model cases of `suite_solid` and `suite_models`; not the set-up suite, the `setup_*` cases or the geometric contact search: `plan.py`, `threaded_case`); the distributed cases at 1/2/4/8 ranks (`before head`, and `head` 2 × 4, 4 × 2 for the five whose timed phases run threaded model code); `suite_python.py` (`before head`) | 5 repetitions |
| C | `ws`, `ins`, `full12`, `reh6`, `reh4`, `eig` | the cohesive benchmark: series `ws` (500 steps, ten points of the weak and strong studies), `ins` (h = 0.12, 1,500 steps), `full12` (h = 0.12, 5,000 steps), `reh6` (h = 0.06, 5,000 steps; `cpp head` only, before → head being measured by the other series), `reh4` (h = 0.04, 300 steps), each at 1/2/4/8 ranks with `cpp before head` and `head` 2 × 4, 4 × 2 on the strong mesh; `eig`: the SparseLU static solve at 1–8 ranks, `before-eig head-eig` (the bitwise check of the distributed runs, one repetition) | 5 repetitions |
| D | `eig1`, `mumps`, `suite` | the static solve with SparseLU (`cppeig before-eig head-eig`, threads if the head has a threaded factorisation) on h = 0.12 and 0.06 at 1 rank; MUMPS (`cpp before head`) at 1/2/4 ranks; `suite_solid` `static_linear_{eigen,faer}` phases (analysis, factorize, back substitution) and `static_linear_mumps` at 1/2/4 ranks | 5 repetitions |
| E | `mem4` | memory on h = 0.04 (static solve + 300 steps) at 1/2/4/8 ranks: `cpp before head head-release`; per-rank peaks (sampler), sums, container peak, the port's records, `INFOG(19)` | 5 repetitions |
| F | `setup`, `dist`, `mpilayer` | set-up phases (`read_mesh`, `dist_mesh`, `init_full`, `before_step`, static solve; 1 step on h = 0.06 and 0.04, 1/2/4/8 ranks, `cpp before head`); `dist_mesh_tet4` at size `l` (1,296,000 Tet4) at 2/4/8 ranks; the MPI layer against raw MPI (methods A and B of `docs/benchmarks/mpi-latency-2026-11-14/`) | 5 repetitions |
| G | `full` | the JOSS mesh, 5,000 steps, one 128-core node: `--points "128:head 128:cpp 64:head 64:cpp 32:head 32:cpp 64x2:head 32x4:head 16x8:head 8:head"` (the default, in this order; the partitions of every rank count exported first by the C++ harness, replayed by every run); any other list of `RANKS[xTHREADS]:VARIANT[,VARIANT]`; `--placement fill` (default: one NUMA node filled before the next, so 64 ranks run on one socket) or `spread` (the same number of ranks on every node: 64 ranks as 32 + 32) | 1 run per point |

### Expected machine time

`run_part.sh <PART> --plan-only` prints it per series (`plan.py`; the basis
of every estimate is written there). At full settings, 5 repetitions, on an
8-core Zen 4 machine (part G: one 128-core node, one run per point):

| part | expected | of which |
|---|---:|---|
| A | 0.2 h | digests 1 min, 5 rounds 10 min |
| B | 4.4 h | `solid` 2.3 h, `models` 0.9 h, `setup` 0.1 h, `mpi` 1.0 h, `python` 0.1 h |
| C | 15.4 h | `ws` 2.3 h, `ins` 0.5 h, `full12` 1.6 h, `reh6` 7.9 h, `reh4` 3.1 h, `eig` 0.1 h |
| D | 2.6 h | `eig1` 1.9 h, `mumps` 0.3 h, `suite` 0.3 h (+0.8 h if the head's factorisation uses threads) |
| E | 3.7 h | |
| F | 1.9 h | `setup` 1.4 h, `dist` 0.2 h, `mpilayer` 0.3 h |
| A to F | 28.2 h | |
| G | 7.0 h | below |

### Part G

Before its first run, part G prints its plan (`DATA/G/plan.txt`; without
running anything: `run_part.sh G --plan-only`, and for another machine
`FINAL_PLAN_CORES=128 FINAL_PLAN_MEMORY=481g run_part.sh G --plan-only`):
the steps in their order, the cores of each run, the expected container
memory peak and wall time from `g-estimates.txt` (where every figure's
basis is written), the cumulative machine time. For the default points on
a 128-core node it expects 7.0 h of runs:

| step | cores | expected peak | expected wall |
|---|---:|---:|---:|
| partition exports, 128 / 64 / 32 / 16 / 8 ranks | | | 10 min |
| port, 128 ranks | 128 | 250 GiB | 21 min |
| C++, 128 ranks | 128 | 235 GiB | 38 min |
| port, 64 ranks | 64 | 206 GiB | 26 min |
| C++, 64 ranks | 64 | 192 GiB | 45 min |
| port, 32 ranks | 32 | 186 GiB | 33 min |
| C++, 32 ranks | 32 | 194 GiB | 1.05 h |
| port, 64 × 2 threads | 128 | 206 GiB | 22 min |
| port, 32 × 4 threads | 128 | 186 GiB | 22 min |
| port, 16 × 8 threads | 128 | 175 GiB | 35 min |
| port, 8 ranks | 8 | 166 GiB | 1.72 h |

- Memory: the 64-, 32- and 8-rank figures are the container peaks of the
  2026-11-12 run (port 206.2 / 185.5 / 165.9 GiB, C++ 191.7 / 194.2 GiB).
  128 ranks were never run: MUMPS's summed allocation grew 139 → 163 → 183
  GB from 8 to 32 to 64 ranks, and its excess over 8 ranks about doubles
  per doubling of the ranks on the smaller meshes, so +40 GB from 64 to 128,
  plus about 6 GiB of per-process memory and ghost layers: about 250 GiB
  (port) and 235 GiB (C++). 512 GiB is enough for the C++ run unless its
  allocation grows four times faster than that; the port's 128-rank run
  comes first and measures it (the two codes' peaks were within 8 % of
  each other at 32 and 64 ranks). Hybrids hold the memory of their rank
  count. The limit is `--memory` (94 % of the machine by default, 481g on
  512 GiB); an out-of-memory kill is recorded (outcome `oom`), not repeated;
  move to the `r7a.32xlarge` (1 TiB) if the port's 128-rank peak exceeds
  400 GiB.
- Order: the runs go one at a time, so the machine time is their sum in any
  order; the order removes waste: every partition is exported once, first;
  the shortest full-size run (the port at 128 ranks) runs first and
  measures the 128-rank memory before the C++ run pays for it; the longest
  and least needed run (8 ranks, 1.7 h) runs last, so that a short budget
  drops it rather than a comparison.
- Time: the port's runs are the 2026-11-12 phases scaled by the campaign's
  measured head / before ratios (check ×0.45, explicit step ×0.95, energies
  ×0.8, distribution ×0.33, `init_full` ×0.5, static solve unchanged); 128
  ranks with a loop efficiency of 0.65 from 64 ranks; the hybrids with the
  loop ×0.8 (64 × 2), ×0.6 (32 × 4), ×0.5 (16 × 8) of their rank count's run.
  The dry run measured 2 × 4 threads at 1.95× and 4 × 2 at 1.30× the loop
  of 8 single-thread ranks on a small mesh, so the hybrid estimates are
  optimistic; the 16 × 8 run takes at most as long as the 8-rank run.
- Its summary is the strong-scaling table: per configuration (ranks ×
  threads) the wall time, the phases, the core-µs per element and step and
  the loop efficiency against the smallest core count of the same code,
  then the port / upstream ratios at equal rank counts, memory (largest
  rank, total, container, per-rank min / median / max) and MUMPS's
  statistics.
