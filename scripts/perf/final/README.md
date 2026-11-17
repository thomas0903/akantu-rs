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
| `run_part.sh` | one part, unattended and resumable |
| `summarize.py` | a part's data directory → its `summary.md` (run at the end of every part) |
| `lib.sh` | shared helpers (provenance, scrubbing, wrappers) |
| `digest-exceptions.txt` | suite cases whose digest definition changed between the two port revisions |

The dry run of every part on a development machine (not results) is in
`docs/benchmarks/perf-campaign/final-dryrun/`, with the time each part took
there and the estimates for the real campaign.

## Variants

| label | what |
|---|---|
| `cpp-strict`, `cpp` | the C++ reference `35a85178`, oracle install `install-strict` (`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`, the port's floating-point operations) |
| `cpp-default` | the same with upstream's flags (part A only, as in the document) |
| `before` | the port before the campaign: `--before REV` of `prepare.sh`; use `91cb54c64` (the library of `59dd2229` = public `main`, plus the benchmark suites and the baseline scripts; its `cohesive_extrinsic` and `explicit_step_timing` sources are those of `59dd2229`) |
| `head` | the port at the revision under test (`--head REV`), default bitwise policy, one thread |
| `head-t2`, `-t4`, `-t8` | `head` with `AKANTU_NUM_THREADS = N`, threads on cores 0..N−1; in the cohesive and MPI parts `head-tN` at P ranks is P ranks × N threads (`mpirun --map-by slot:PE=N --bind-to core`) |
| `head-fast`, `head-fast-t8` | `head` with `AKANTU_PRECISION=fast` (compared only through its tolerance rules, never digested as bitwise) |
| `head-eig`, `before-eig`, `cppeig` | the static solve with each code's SparseLU instead of MUMPS |
| `head-release` | `head` with `rust_release_static_solver = true` (the static solver's factors released after the preload) |

Variants are directories prepared by `prepare.sh`, never hard-coded:
`run_part.sh --before DIR --head DIR` takes any two prepared directories.

## What a fresh machine needs before `prepare.sh`

An 8-core x86_64 Ubuntu 24.04 host (an AWS `m7a.2xlarge`, 32 GiB; part G an
`r7a.16xlarge`, 64 cores, 512 GiB) with, as on the campaign's machines:

1. Docker, git, Python 3 with `venv` and `pip` (network access to PyPI for
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
scripts/perf/final/prepare.sh --before 91cb54c64 --head <REV> [--joss-mesh cube.msh]
```

`prepare.sh` checks the oracle installs and the cohesive harness builds,
copies or builds the C++ explicit-step harness, checks the meshes (SHA-256
against the recorded files) and generates the scaling meshes h = 0.0925,
0.0756, 0.0476 with the oracle image's Gmsh, builds the raw-MPI baseline,
and per variant: the host binaries (`explicit_step_timing`, the three
suites), the MPI binaries in the images (`cohesive_extrinsic` and
`suite_solid` with `mpi,scotch,mumps`, the suites with `mpi,scotch`,
`mpi_bench`), a Python environment with the extension; `VARIANT.json`
records revision, toolchain, image ids, binary digests and whether the
head's SparseLU factorisation uses threads (part D times threads only then).

## Running a part

```sh
nohup scripts/perf/final/run_part.sh C --reps 5 --size full > C.out 2>&1 &
tail -f ~/final/data/C/progress.log
```

Every point (the variants of one configuration in one repetition, their
order rotated by the repetition) writes its directory and a marker when it
is complete; calling the same command again skips complete points and
repeats incomplete ones (after a reboot, a stopped host, an interrupted
ssh). Exit status 0 only if every point is complete and every check
passed. Then `summarize.py` writes `~/final/data/<PART>/summary.md`, and
every text file of the data directory is scrubbed (data root written
`<root>`, home `~`, host name `<host>`) and checked for host names and
addresses. Copy the data directory into
`docs/benchmarks/perf-campaign/final/` and run
`cargo test -p akantu-validation --test public_tree_guard`.

Method (COMMON.md section 4, `docs/BENCHMARKS.md` "Method"): nothing is
built while a part runs; one part at a time on a machine; serial runs on
core 2, N threads or ranks on cores 0..N−1 (one per physical core); the
variants of a point interleaved, their order rotated per repetition; at
least 5 repetitions in parts A to F (`--reps`); medians with [min, max];
ratios paired per repetition. Both codes read the same mesh file and, at
P > 1, the same Scotch partition (exported by the C++ harness, replayed by
the port).

Checks that stop a point from counting (the point is repeated on the next
call; the part exits 1):

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
  `rust_exec` in `summary.json`) equal to the requested ones (a `before`
  build records none: it has one thread and the bitwise path);
- a C++ run that took upstream's MUMPS retry path (UD-216; the harness
  reports it, `status.txt` outcome `ud216`): the whole point is repeated,
  at most twice;
- fast-mode runs are timed, never digested.

## The parts

| part | series (`--only`) | what | full settings |
|---|---|---|---|
| A | `digests`, `rounds` | the ten configurations of `scripts/perf/explicit-step/` (`timing_rotated.sh`), programs `cpp-strict cpp-default before head head-t2 head-t4 head-t8 head-fast head-fast-t8`, phases `step`, `fint`, `stress` | 5 rounds |
| B | `solid`, `models`, `setup`, `mpi`, `python` | every suite case at size `m` (`before head head-t2 head-t4 head-t8 head-fast head-fast-t8`); the distributed cases at 1/2/4/8 ranks (`before head`, and `head` 2 × 4, 4 × 2); `suite_python.py` (`before head`) | 5 repetitions |
| C | `ws`, `ins`, `full12`, `reh6`, `reh4`, `eig` | the cohesive benchmark: series `ws` (500 steps, ten points of the weak and strong studies), `ins` (h = 0.12, 1,500 steps), `full12` (h = 0.12, 5,000 steps), `reh6` (h = 0.06, 5,000 steps), `reh4` (h = 0.04, 300 steps), each at 1/2/4/8 ranks with `cpp before head` and `head` 2 × 4, 4 × 2 on the strong mesh; `eig`: the SparseLU static solve at 1–8 ranks, `before-eig head-eig` (the bitwise check of the distributed runs, one repetition) | 5 repetitions |
| D | `eig1`, `mumps`, `suite` | the static solve with SparseLU (`cppeig before-eig head-eig`, threads if the head has a threaded factorisation) on h = 0.12 and 0.06 at 1 rank; MUMPS (`cpp before head`) at 1/2/4 ranks; `suite_solid` `static_linear_{eigen,faer}` phases (analysis, factorize, back substitution) and `static_linear_mumps` at 1/2/4 ranks | 5 repetitions |
| E | `mem4` | memory on h = 0.04 (static solve + 300 steps) at 1/2/4/8 ranks: `cpp before head head-release`; per-rank peaks (sampler), sums, container peak, the port's records, `INFOG(19)` | 5 repetitions |
| F | `setup`, `dist`, `mpilayer` | set-up phases (`read_mesh`, `dist_mesh`, `init_full`, `before_step`, static solve; 1 step on h = 0.06 and 0.04, 1/2/4/8 ranks, `cpp before head`); `dist_mesh_tet4` at size `l` (1,296,000 Tet4) at 2/4/8 ranks; the MPI layer against raw MPI (methods A and B of `docs/benchmarks/mpi-latency-2026-11-14/`) | 5 repetitions |
| G | `full` | the JOSS mesh, 5,000 steps, one 64-core node: `--points "64:head,cpp 32:head,cpp 32x2:head 8:head"` (in this order; partitions exported first) | 1 run per point |

Part G memory per run (the 2026-11-12 run, container peaks): port 206 GiB
at 64 ranks, 186 GiB at 32, 166 GiB at 8; C++ 192 GiB at 64, 194 GiB at
32; 32 × 2 threads as 32 ranks. One run at a time, `--memory 480g` on the
512 GiB node.
