# Benchmarks

This document holds the measured performance of akantu-rs against the C++
reference (Akantu at `35a85178`), with the machine, date, data and command
behind every number. The short report for the Akantu team,
`docs/reports/02-benchmarks-and-performance.md`, summarises it.

The results of the final campaign (stage 1, parts A to F; four AWS
`m7a.2xlarge`, AMD EPYC 9R14, 8 cores each; 2026-11-17 UTC; port head
`53de1eeef`, port before `b6f56c2f0`, C++ strict install; five repetitions,
medians with [min, max]; data `docs/benchmarks/perf-campaign/final/`):

- On one core the port's explicit step takes 0.29–0.40× the time of the C++
  strict step in small strain and 0.15–0.25× in finite deformation (five
  meshes, three element types), and 0.51–0.72× the time of the port before
  the campaign. At 8 threads it takes 0.20–0.47× its one-thread time.
  Every digest equals the recorded one (part A).
- In the cohesive benchmark (h = 0.12 to 0.04, 1 to 8 ranks, MUMPS in both
  codes) the explicit loop takes 0.22–0.30× upstream's time at 1 to 4 ranks
  and 0.31–0.36× at 8 ranks, and 0.36–0.62× the port's time before the
  campaign. On 8 cores, 2 ranks × 4 threads take 1.05–1.22× the loop time
  of 8 ranks and 4 ranks × 2 threads 0.99–1.06× (part C).
- The static MUMPS solve phase takes 0.79–0.96× upstream's at 1 to 4 ranks
  and 0.91–1.11× at 8 ranks; the port's SparseLU takes 0.74× (h = 0.12) and
  0.91× (h = 0.06) upstream's SparseLU at one thread, 0.45× and 0.26× at 8
  threads (parts C and D).
- With the opt-in nested-dissection ordering (D-125; COLAMD stays the
  default) the port's SparseLU static solve takes 0.305× (h = 0.12) and
  0.170× (h = 0.06) upstream's SparseLU at one thread, 0.412× and 0.186×
  the port with COLAMD (part D, second pass, head `4d974854d`).
- On h = 0.04 (558,875 tetrahedra, static solve and 300 steps) the sum of
  the ranks' memory peaks is 8,965 / 11,013 / 10,400 / 12,133 MiB for the
  port and 11,669 / 13,741 / 13,171 / 15,367 MiB for upstream at 1 / 2 / 4 /
  8 ranks; releasing the static solver's factors brings the port to 7,302 /
  9,464 / 8,873 / 10,716 MiB (part E).
- Set-up: `init_full` takes 0.075–0.19× upstream's time and the mesh
  distribution 0.095–0.11×. The communication layer takes 6.46× raw MPI for
  an 8-byte ping-pong over shared memory and 2.52× for 16 MiB (part F).

The full-size benchmark (the JOSS mesh, 4,392,183 tetrahedra, 5,000 steps
with insertion; AMD EPYC 9R14; 2026-11-17 and 18 UTC; one run per
configuration; data `docs/benchmarks/perf-campaign/final/G/` and
`final/multinode/`; loop = step, check, energies and boundary conditions):

- On one 128-core machine (AWS `m7a.32xlarge`, two sockets; port
  `740a79a36`, part G) the port's run took 0.42–0.51 of upstream's wall
  time and its loop 0.35–0.43 of upstream's at 32, 64 and 128 ranks (835 s
  against 1,948 s at 128 ranks); 64 ranks × 2 threads bring its loop to
  568 s. That machine does not scale from 64 to 128 ranks (the port's loop
  846 s → 835 s, upstream's 1.25× faster), so the ratios at 128 ranks are
  those of a saturated machine.
- Across four 32-core hosts (AWS `m7a.8xlarge`, 128 ranks) the port took
  1.48 and 1.51 times upstream's loop before the change of its collective
  error protocol (1,538 s and 1,536 s against 1,040 s and 1,019 s, two
  sessions on different hosts): it agreed after every phase, 51.6
  collectives per step and rank against upstream's 8.4. With collective
  checks at commit points (D-126, 20.5 collectives per step and rank,
  results unchanged) the final revision `4a8d9d38e` takes 526 s on four
  hosts, 0.51–0.52 of upstream's loop, and 722 s on two hosts against
  upstream's 1,924 s (0.375). On one machine it takes 814 s at 128 ranks
  (835 s before the change).

Sections marked "before the final campaign" are the dated records of 11 to
14 November, measured on other machines with the port as it was then;
their numbers are not compared with the final campaign's.

The summary this document opened with before the final campaign (the
measurements of 11 to 14 November, the port before the performance
campaign), unchanged:

> The main results: on one core the port's explicit step takes 0.31–0.60× the
> time of the C++ step built with the same arithmetic (five meshes, three
> element types). In the cohesive benchmark at 1 to 8 ranks the explicit loop
> takes 0.39–0.59× upstream's time. On the JOSS mesh (4.39 million
> tetrahedra, 5,000 steps with fracture) on one 64-core node the port's run
> took 0.875× upstream's wall time at 64 ranks and 0.851× at 32. The port is
> slower in the mesh distribution (1.26–1.86× at full size) and in the static
> MUMPS solve phase (about 1.1× in the paired series; no gap is established at
> full size). Both codes need about the same memory, most of it MUMPS's; at 64
> ranks the port's largest rank peaks at 1.27× upstream's. No performance work
> was done before parity was reached; none of the slow paths
> listed at the end was worked on.

## Where the numbers come from

Every comparison of the port with the C++ reference was measured on x86_64
cloud machines, with both codes on the same machine in the same session.
The development Mac (an Apple M4 Pro, natively or in its aarch64 Linux VM)
was used for development records only; they are in the appendix and no
conclusion rests on them.

| Machine | CPU | Dates | Measured there | Data |
|---|---|---|---|---|
| 4 × AWS `m7a.2xlarge` (8 cores, one thread per core, one NUMA node, 30.6 GiB visible, Ubuntu 24.04.5, kernel 7.0.0-1014-aws), one part per machine, nothing else running | AMD EPYC 9R14 | 2026-11-17 (sessions 10:33 to 18:59 UTC) | the final campaign, stage 1: machine 1 part C series `reh6`; machine 2 part C (other series); machine 3 parts B, D; machine 4 parts E, F, A | `docs/benchmarks/perf-campaign/final/` |
| AWS `m7a.32xlarge`, 128 cores, one thread per core, two NUMA nodes of 64 cores, 493 GiB visible, Ubuntu 24.04.5, kernel 7.0.0-1014-aws | AMD EPYC 9R14 | 2026-11-17 22:35 to 2026-11-18 03:38 UTC | the final campaign, part G (the JOSS mesh at 128 / 64 / 32 ranks, both codes; the port at 64 × 2, 32 × 4, 16 × 8 threads; port `740a79a36`) | `docs/benchmarks/perf-campaign/final/G/` |
| AWS `m7a.2xlarge` (machine 3 again, same type) | AMD EPYC 9R14 | 2026-11-17 (17:06 to 20:20 UTC) | the final campaign, part D second pass at `4d974854d`: every variant of the first pass and the orderings `head-nd`, `head-auto` | `docs/benchmarks/perf-campaign/final/D-nd/` |
| 4 × AWS `m7a.8xlarge` per session (32 cores, one thread per core, one NUMA node, 123 GiB visible, same image and kernel), one cluster placement group; three sessions on three sets of hosts; and one 128-core machine of part G's description | AMD EPYC 9R14 | 2026-11-18 (04:12 to 19:37 UTC) | the multi-host runs: both codes at 128 ranks on four hosts and 64 on two, the port before and after D-126; the final revision on one machine | `docs/benchmarks/perf-campaign/final/multinode/` |
| AWS `r7a.16xlarge`, one node, 64 cores, one thread per core, one NUMA node, 493 GiB visible | AMD EPYC 9R14 | 2026-11-12, 2026-11-13 | the full-size run (both codes at 64 and 32 ranks, the port at 8); the port's rerun after UD-1280 | `docs/benchmarks/c1-2026-11-12/`, `c1-rerun-2026-11-13/` |
| 8 × AWS `r7a.2xlarge` (8 cores, 64 GiB each), one cluster placement group | AMD EPYC 9R14 | 2026-11-13 | the multi-node runs (the port only, up to 64 ranks) | `docs/benchmarks/mn-2026-11-13/` |
| boat.dev `large` sandbox, 8 vCPU, 15.6 GiB, Ubuntu 24.04, kernel 6.8.0-117 | AMD EPYC 9455 | 2026-11-12, 2026-11-13 | the mid-size rehearsal and the MUMPS series; the weak and strong scaling series at 1–8 ranks | `docs/benchmarks/midsize-2026-11-12/`, `scaling-2026-11-13/` |
| same type | AMD Ryzen 7 9700X | 2026-11-11, 2026-11-12 | the explicit-step kernels against both C++ builds; the C++ baselines on x86_64 | `docs/benchmarks/explicit-step/final/`, `cpp-baseline-x86-2026-11-11/` |
| same type | AMD Ryzen 9 9950X | 2026-11-11 | the explicit-step development series; the serial driver against both C++ builds | `docs/benchmarks/explicit-step/development/`, `serial-driver-2026-11-11/` |
| boat.dev `default` sandbox, 4 vCPU, container 4 CPUs and 6 GiB | AMD Ryzen 9 9950X | 2026-11-14 | MPI latency against raw MPI | `docs/benchmarks/mpi-latency-2026-11-14/` |
| same type | not recorded | 2026-11-12 | the MPI performance gate measured in one job | numbers in this document only |
| development Mac: Apple M4 Pro, 14 cores, 24 GB, natively and in an aarch64 Linux VM | Apple M4 Pro | 2026-11-10, 2026-11-11 | development records only (appendix) | `docs/benchmarks/dev-machine/` |

The final campaign's dates are UTC dates, as its session records give them. Other dates are the project's local dates (UTC+11); the provenance files of the
data record UTC times. boat.dev sandboxes are shared virtual machines. The
upstream JOSS study's
published timings (EPFL's jed cluster, Intel Xeon 8360Y) are committed
verbatim in `benchmarks/cohesive-perf/results/published-jed/`.
`docs/benchmarks/README.md` lists every data directory.

## Method

### The final campaign (2026-11-17)

The final campaign measures the C++ reference, the port before the
performance campaign and the port at the head under test on the same
machine in the same session, for every table of this document. Its driver
is `scripts/perf/final/` (`README.md` gives the parts, the variants, the
checks and the commands). Seven parts (A to G) run independently, each
alone on its machine with its own data directory. Stage 1 (parts A to F)
ran on four AWS `m7a.2xlarge` of the same type; part C was split over two
of them with `--only` (series `reh6` on one, the other series on the
other), which is why it has two data directories, `C/` and `C-reh6/`.

The programs (`scripts/perf/final/README.md`, "Variants"):

| label | what |
|---|---|
| C++ strict (`cpp`) | the C++ reference `35a85178`, strict install, JOSS emulation in every cohesive run |
| C++ default | upstream's flags (part A only) |
| port before (`before`) | the branch `perf/final-before` at `b6f56c2f0` |
| port (`head`) | `53de1eeef`, the default bitwise policy, one thread |
| `head-tN`, ranks × threads | the head with `AKANTU_NUM_THREADS = N` |
| fast | the head with `AKANTU_PRECISION=fast` |
| SparseLU | each code's SparseLU in place of MUMPS |
| factors released | the head with `rust_release_static_solver = true` |

What changed against the earlier campaigns of 11 to 14 November:

- One rank on one core at every rank count. Rank r of a run with T threads
  per rank is bound to the r-th block of T cores, NUMA node by NUMA node
  (`PERF_MPI_BIND=core`), in both codes; serial runs on core 2, N threads
  on cores 0..N−1. `bindings.py` reads every rank's `Cpus_allowed_list`
  from `/proc` during each MPI run and the run counts only if the binding
  is the planned one: 380 + 50 cohesive runs in part C, 150 in D, 80 in E,
  120 in F, all as planned. The earlier AWS runs were not bound and the boat
  runs shared their vCPUs with the system.
- Binaries identified by digest. `prepare.sh` builds every binary before
  any timing and records its SHA-256 (`VARIANT.json`, `CPP.json`); every
  cohesive run records the SHA-256 of what it executed and the point counts
  only if it matches. The port's MPI driver runs from a copy named by its
  SHA-256.
- The variants of a point interleaved and their order rotated per
  repetition, five repetitions of every point, on machines where nothing
  else ran (the driver waits for an idle machine before each group). The
  earlier AWS runs were one run per point.
- The `before` variant. "Port before" is the library of `59dd2229` (the
  public `main`, byte for byte in every crate but `akantu-bench`) with the
  benchmark suites' final case definitions cherry-picked onto it. It is a
  branch because the suites' cases and digests were fixed after `59dd2229`:
  without the cherry-picks the same case name would measure another
  computation in the two revisions.
- The JOSS emulation for C++ throughout: every C++ cohesive run (parts C to
  F) uses the strict install with the `fillNodesToElements()` rebuild
  skipped (`FNTE = joss`), as the JOSS runs did.
- Checks. A group counts only if the binary, the thread count and
  precision each run recorded, the core binding and the state agree: part
  A's state digests equal `docs/benchmarks/explicit-step/final/digests.txt`
  for every bitwise program (90 pairs, 0 differing); every bitwise suite
  variant's digest equals the head's; the per-step energies and insertions
  (17 digits) are identical across the bitwise variants at 1 rank, C++
  included, and across the two revisions in the SparseLU series at every
  rank count. Fast-mode runs are timed, never digested. Every group of
  stage 1 is complete and checked (each `summary.md` gives the counts).

Run-to-run repeatability at more than one rank: with MUMPS, the port's
results are not bit-repeatable at 2 ranks and more. The distributed MUMPS
solve differs in the last digits from run to run (two runs of the same
binary of `static_linear_mumps` at 4 ranks gave two digests in the dry
run), so the campaign times those runs without comparing digests
(`scripts/perf/final/digest-exceptions.txt`). With the port's own SparseLU
the results are bit-repeatable: the per-step energies and insertions of the
SparseLU series (part C, series `eig`) are identical at 1, 2, 4 and 8 ranks
and at 2 × 4 and 4 × 2. Upstream's results at P > 1 vary in the last digits
(the distributed MUMPS solve and `MPI_Waitany` orders).

Times are rank 0's Chrono totals; the loop is `solve_step` plus the
insertion check; core-µs per element and step and loop efficiency are
defined as below. Ratios are paired per repetition, then the median
[min, max]. A ratio column marked "unpaired" divides the medians of two
groups.

### Part G and the multi-host runs (2026-11-17 and 18)

Part G ran the JOSS mesh on one AWS `m7a.32xlarge` (128 cores, two NUMA
nodes of 64 cores) with the driver of stage 1, one run per point, both
codes on the same exported partition per rank count, rank r with T threads
on the r-th block of T cores, NUMA node 0 filled first. The multi-host
runs used the multi-node launcher (`crates/akantu-bench/scripts/mn_cluster.py`,
`mn_session.py`) on four `m7a.8xlarge` (32 cores, one NUMA node each,
started together in one cluster placement group from the same machine
image, Open MPI 5.0.7 over TCP between them), 32 ranks per host in
consecutive blocks, rank 0 on the first host, part G's partitions and
inputs; three sessions on three sets of hosts. The hosts' descriptions
(CPU, cores, NUMA nodes, memory, kernel, transparent huge pages, NUMA
balancing, Docker) are recorded in each run's `status.txt` (sessions 2
and 3) or were read on each host before the session (session 1,
`multinode/s1-port/hosts-record.txt`).

Ranks bind themselves. Open MPI 5.0.7's launcher cannot start more than 10
ranks per host with `--bind-to core` inside a Docker container with the
default seccomp profile: every bound child's `set_mempolicy()` is refused
without `CAP_SYS_NICE`, the warning it sends meets a closed pipe and the
launcher aborts at the eleventh `SIGPIPE`. Part G's first attempt failed
this way at the start of every run. Both codes' launchers therefore start
`mpirun --bind-to none` and every rank binds itself with `taskset` to the
r-th block of T cores of its container
(`benchmarks/cohesive-perf/scripts/in-container/bind-rank.sh`, r the rank's
index on its host): the same cores per rank as `--map-by core --bind-to
core` (`--map-by slot:PE=T`). `docker run --cap-add SYS_NICE` would be the
alternative. The recorded binding of every run is the planned one (part G:
`bindings.py check` over every record, `G/README.md`; the multi-host runs:
`status.txt` `bindings`, `binding_problems`).

Every configuration ran once; the 128-rank port point of part G has a
second sample, and two configurations ran on two sets of hosts (the port
at `740a79a36` and C++ at 128 ranks on four hosts). The full-size tables
give rank 0's phase totals; the loop of the multi-node tables includes the
per-step energies and boundary conditions, the "loop s" of part G's
generated tables does not.

### The earlier campaigns (11–14 November)

Same input. Both codes run on the same machine in the same session and read
the same mesh file. In parallel they use the same Scotch partition: the C++
harness exports it, the port replays it (`rust_partitioner =
file:<partition.csv>`), and every C++ run re-creates it and checks that it
matches. Partitions are reproducible only with `AKA_SEED = 1` and one Scotch
thread (by default Akantu seeds Scotch from the clock, and a multi-threaded
Scotch gave another 2-rank partition in every run). Both codes hand the same
matrix to the same MUMPS 5.7.3 with single-threaded OpenBLAS. Pins in both
images: `OMP_NUM_THREADS = OPENBLAS_NUM_THREADS = SCOTCH_PTHREAD_NUMBER = 1`,
`OPENBLAS_CORETYPE = Haswell`.

The C++ library is built with GCC 14.2, `RelWithDebInfo` (`-O2 -g -DNDEBUG`,
`-flto=auto`, `-frounding-math` from CGAL), no `-march`, by the oracle's
scripts (`docs/ORACLE_BUILD.md`), in two installs. *Strict* adds
`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`, the port's floating-point
operations (no FMA, scalar reductions in Eigen's order). *Default* keeps
upstream's flags; on x86-64 without `-march` the two differ only by Eigen's
SSE2 packet code (on aarch64 the default install also contracts FMA). The
cohesive series use the strict install. In the cohesive driver the default
install showed no measurable difference (paired default / strict medians
0.80–1.24, every range across 1; Ryzen 9 9950X, 2026-11-11,
`docs/benchmarks/serial-driver-2026-11-11/summary.md`); on the explicit-step
step it took 1.01 to 1.51 times the strict install's time (ratio of the step
medians over the ten configurations), which was not profiled.

The port uses the workspace release profile (`opt-level = 3`, `debug = 1`,
no LTO, 16 codegen units, x86-64 baseline, no `target-cpu`), without
fast-math or FMA. `scripts/perf/explicit-step/codegen_check.sh` checked the
generated code (`docs/benchmarks/explicit-step/final/codegen-x86_64.txt`):
no `llvm.fma` and no fast-math flag in the IR of `akantu-fe`,
`akantu-model`, `akantu-solver` and `akantu-cohesive`; ten `nnan`/`ninf`
flags inferred by LLVM in three laws of `akantu-solid` outside the
explicit-step kernels, and the documented `mul_add` exceptions of
`akantu-core`; in the release binaries the only FMA instructions are those
of compiler-builtins' run-time dispatched `fma_with_fma{,4}`. Fat LTO with
one codegen unit gave identical results and no consistent gain.

At one rank the port reproduces the strict build bit for bit: the
explicit-step state digests (FNV-1a of `u`, `v`, `a`, `f_int`, ten
configurations, `digests.txt`) and the whole fragmenting 5,000-step cohesive
run on the mid-size mesh. The serial timings compare the same computation.

A boat sandbox's speed drifts by up to 2.5× within a session (one binary's
static solve took 16.8 s and 5.7 s within an hour). Every boat comparison runs
one job at a time, interleaves the variants, rotates their order per
repetition, uses 3 to 7 repetitions and reports the median with [min, max].
Ratios between the codes are paired: per repetition, then the median
[min, max]. Absolute times of two sandboxes or sessions are not compared.
Checks of single changes on boat could not resolve differences below
10–20 %. The AWS runs are one run per point and give no spread.

Times are rank 0's Chrono totals (`chrono.csv`, written by both drivers). The
explicit loop is `solve_step` (the explicit step) plus
`check_cohesive_stress` (the insertion check: facet stresses, decision,
insertion), in the multi-node tables also the load increments and the
per-step energies. A core-µs per element and step is (`solve_step` + check
totals) × ranks / tetrahedra / steps: the CPU time one element costs per
step, constant under perfect scaling. Loop efficiency is the core-µs at the
reference rank count over the core-µs at P.

The benchmark problem: upstream's driver blocks a rank-local node, so its
problem changes with the partition (UD-349); both codes run with
`blocked_node = global`. The C++ runs skip the `fillNodesToElements()`
rebuild after each insertion, as the JOSS runs did with their patch, through
an interposer in the benchmark executable (`--fnte joss`, bitwise the same
physics). The benchmark writes the energies of every step (4–5 % of the wall
time); the JOSS runs did not. Upstream's runs at P > 1 solve another load
case after their first insertion touching a ghost boundary element (UD-890),
so insertion counts at P > 1 are not compared. The drivers count inserted
elements from the mesh (cohesive elements that are not ghosts, summed over
the ranks); upstream's `checkCohesiveStress()` return value also counts
ghost copies at P > 1 (UD-347). At P > 1 the port's results with MUMPS are not bit-repeatable from run to
run (the distributed MUMPS solve; corrected after the final campaign's dry
run, see above); with the port's SparseLU they are. Upstream's vary in the
last digits (the distributed MUMPS solve and `MPI_Waitany` orders). Upstream's MUMPS memory-retry
path returns a wrong static solution (UD-216); the C++ harness stops such a
run and repeats it, so every C++ timing is conditional on a retry-free
solve.

Tools: the C++ harness `benchmarks/cohesive-perf/`, the port's driver
`crates/akantu-bench` (binary `cohesive_extrinsic`, same input format), the
explicit-step harnesses `scripts/perf/explicit-step/`, each with a README
giving the steps. The campaign and analysis scripts named below are in
`crates/akantu-bench/scripts/`.

## Explicit dynamics

### The explicit step on one core

The explicit step of `examples/c++/solid_mechanics_model/explicit` (unit
cube, `elastic`, small strain (ss) or finite deformation (fd), the ten
configurations of `scripts/perf/explicit-step/`), timed by the Rust harness
`crates/akantu-solid/examples/explicit_step_timing.rs` and its C++ twin.
Final campaign, part A: AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17 UTC,
5 rounds with the program order rotated, one process on core 2 (threads on
cores 0..N−1); data `docs/benchmarks/perf-campaign/final/A/summary.md`.
Nanoseconds per element and step, median [min, max]; ratios paired per
round.

Table "Explicit step (`solve_step`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 145 [145, 145] | 227 [225, 227] | 68.2 [67.8, 68.6] | 47.0 [46.7, 47.3] | 0.32 [0.32, 0.33] | 0.21 [0.21, 0.21] | 0.69 [0.68, 0.69] | 0.47 [0.47, 0.47] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | 162 [161, 163] | 240 [238, 241] | 88.9 [88.5, 90.0] | 63.6 [63.5, 64.1] | 0.40 [0.39, 0.40] | 0.27 [0.26, 0.27] | 0.72 [0.71, 0.72] | 0.55 [0.54, 0.56] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 810 [809, 816] | 1,213 [1,204, 1,214] | 409 [405, 411] | 269 [269, 271] | 0.33 [0.33, 0.33] | 0.22 [0.22, 0.23] | 0.66 [0.66, 0.67] | 0.50 [0.50, 0.51] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 1,251 [1,249, 1,253] | 1,914 [1,908, 1,981] | 548 [546, 555] | 367 [364, 367] | 0.29 [0.29, 0.29] | 0.19 [0.19, 0.19] | 0.67 [0.66, 0.67] | 0.44 [0.44, 0.44] |
| hex8-200k | hexahedron_8 | 195,112 | ss | 1,198 [1,197, 1,217] | 1,862 [1,849, 1,865] | 573 [572, 576] | 380 [379, 384] | 0.32 [0.31, 0.32] | 0.20 [0.20, 0.21] | 0.66 [0.66, 0.67] | 0.48 [0.47, 0.48] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 302 [300, 304] | 390 [385, 391] | 102 [102, 102] | 65.4 [65.1, 65.9] | 0.22 [0.22, 0.22] | 0.17 [0.17, 0.17] | 0.64 [0.64, 0.64] | 0.34 [0.34, 0.34] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | 327 [325, 328] | 412 [407, 412] | 130 [130, 131] | 81.5 [81.1, 81.6] | 0.25 [0.25, 0.25] | 0.20 [0.20, 0.20] | 0.63 [0.62, 0.63] | 0.40 [0.40, 0.40] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 2,245 [2,230, 2,255] | 2,530 [2,524, 2,546] | 706 [703, 709] | 377 [377, 379] | 0.17 [0.17, 0.17] | 0.15 [0.15, 0.15] | 0.53 [0.53, 0.54] | 0.31 [0.31, 0.32] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 3,567 [3,559, 3,595] | 4,053 [4,038, 4,079] | 1,069 [1,063, 1,074] | 541 [539, 543] | 0.15 [0.15, 0.15] | 0.13 [0.13, 0.13] | 0.51 [0.50, 0.51] | 0.30 [0.30, 0.30] |
| hex8-200k | hexahedron_8 | 195,112 | fd | 3,565 [3,550, 3,574] | 4,044 [4,037, 4,065] | 1,087 [1,082, 1,087] | 562 [561, 572] | 0.16 [0.16, 0.16] | 0.14 [0.14, 0.14] | 0.52 [0.52, 0.53] | 0.30 [0.30, 0.31] |

Table "Internal forces (`fint`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 124 [123, 126] | 206 [204, 208] | 58.0 [57.8, 58.1] | 41.3 [40.9, 41.6] | 0.33 [0.32, 0.34] | 0.20 [0.20, 0.20] | 0.71 [0.70, 0.72] | 0.47 [0.46, 0.47] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | 137 [136, 138] | 216 [214, 217] | 71.8 [71.1, 72.2] | 56.9 [56.6, 56.9] | 0.41 [0.41, 0.42] | 0.26 [0.26, 0.26] | 0.79 [0.79, 0.80] | 0.52 [0.52, 0.53] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 649 [648, 655] | 1,048 [1,038, 1,051] | 320 [317, 322] | 220 [219, 221] | 0.34 [0.34, 0.34] | 0.21 [0.21, 0.21] | 0.69 [0.68, 0.70] | 0.49 [0.49, 0.50] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 1,067 [1,051, 1,069] | 1,711 [1,698, 1,715] | 413 [409, 416] | 328 [327, 329] | 0.31 [0.31, 0.31] | 0.19 [0.19, 0.19] | 0.80 [0.79, 0.80] | 0.39 [0.38, 0.39] |
| hex8-200k | hexahedron_8 | 195,112 | ss | 1,021 [1,017, 1,032] | 1,682 [1,669, 1,692] | 439 [438, 441] | 345 [342, 346] | 0.34 [0.33, 0.34] | 0.20 [0.20, 0.21] | 0.78 [0.78, 0.79] | 0.43 [0.42, 0.43] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 281 [280, 283] | 363 [361, 366] | 88.8 [88.7, 89.3] | 59.3 [58.9, 59.5] | 0.21 [0.21, 0.21] | 0.16 [0.16, 0.16] | 0.67 [0.66, 0.67] | 0.32 [0.31, 0.32] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | 292 [291, 294] | 373 [373, 377] | 103 [103, 104] | 72.3 [72.1, 72.4] | 0.25 [0.25, 0.25] | 0.19 [0.19, 0.19] | 0.70 [0.69, 0.70] | 0.35 [0.35, 0.36] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 2,047 [2,040, 2,065] | 2,356 [2,336, 2,359] | 592 [590, 593] | 326 [326, 330] | 0.16 [0.16, 0.16] | 0.14 [0.14, 0.14] | 0.55 [0.55, 0.56] | 0.29 [0.29, 0.29] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 3,319 [3,295, 3,332] | 3,799 [3,787, 3,800] | 868 [866, 880] | 488 [487, 490] | 0.15 [0.15, 0.15] | 0.13 [0.13, 0.13] | 0.56 [0.56, 0.56] | 0.26 [0.26, 0.27] |
| hex8-200k | hexahedron_8 | 195,112 | fd | 3,330 [3,327, 3,350] | 3,829 [3,802, 3,848] | 913 [910, 914] | 526 [523, 534] | 0.16 [0.16, 0.16] | 0.14 [0.14, 0.14] | 0.58 [0.57, 0.59] | 0.27 [0.27, 0.27] |

Table "Stresses (`stress`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port / port before | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 46.5 [46.4, 47.3] | 101 [101, 102] | 24.3 [24.3, 24.5] | 15.9 [15.8, 15.9] | 0.34 [0.34, 0.34] | 0.16 [0.16, 0.16] | 0.65 [0.65, 0.66] | 0.52 [0.51, 0.53] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | 52.8 [52.0, 53.1] | 107 [107, 108] | 30.4 [30.1, 30.5] | 24.6 [24.3, 25.5] | 0.47 [0.46, 0.49] | 0.23 [0.23, 0.24] | 0.81 [0.80, 0.84] | 0.58 [0.57, 0.58] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 269 [268, 270] | 574 [572, 575] | 132 [131, 134] | 124 [123, 139] | 0.46 [0.46, 0.51] | 0.22 [0.21, 0.24] | 0.94 [0.92, 1.1] | 0.49 [0.49, 0.50] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 450 [450, 451] | 1,048 [1,047, 1,050] | 182 [181, 182] | 170 [163, 190] | 0.38 [0.36, 0.42] | 0.16 [0.16, 0.18] | 0.94 [0.90, 1.0] | 0.40 [0.40, 0.41] |
| hex8-200k | hexahedron_8 | 195,112 | ss | 441 [441, 447] | 1,042 [1,041, 1,045] | 195 [195, 196] | 182 [179, 192] | 0.41 [0.40, 0.44] | 0.17 [0.17, 0.18] | 0.93 [0.91, 0.98] | 0.44 [0.44, 0.44] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 63.3 [62.4, 63.5] | 117 [117, 117] | 30.2 [30.2, 30.3] | 19.0 [19.0, 19.1] | 0.30 [0.30, 0.31] | 0.16 [0.16, 0.16] | 0.63 [0.63, 0.63] | 0.48 [0.48, 0.49] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | 68.4 [68.2, 68.8] | 123 [123, 123] | 36.3 [36.1, 37.1] | 27.1 [26.8, 27.5] | 0.39 [0.39, 0.40] | 0.22 [0.22, 0.22] | 0.75 [0.73, 0.76] | 0.53 [0.53, 0.54] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 347 [346, 347] | 651 [650, 652] | 156 [155, 157] | 138 [130, 139] | 0.40 [0.37, 0.40] | 0.21 [0.20, 0.21] | 0.88 [0.83, 0.89] | 0.45 [0.45, 0.45] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 562 [561, 562] | 1,152 [1,146, 1,153] | 229 [228, 230] | 188 [177, 189] | 0.34 [0.32, 0.34] | 0.16 [0.15, 0.16] | 0.82 [0.77, 0.83] | 0.41 [0.41, 0.41] |
| hex8-200k | hexahedron_8 | 195,112 | fd | 617 [616, 619] | 1,217 [1,189, 1,219] | 262 [261, 262] | 192 [189, 202] | 0.31 [0.30, 0.33] | 0.16 [0.16, 0.17] | 0.73 [0.72, 0.77] | 0.42 [0.42, 0.43] |

Table "The port's step at 2, 4 and 8 threads and in fast mode", ns per
element and step:

| mesh | mode | 1 thread | 2 | 4 | 8 | 2 / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1 thread) | fast / bitwise (8) |
|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | ss | 47.0 [46.7, 47.3] | 33.8 [33.7, 34.2] | 25.8 [25.8, 25.9] | 22.1 [21.9, 32.3] | 0.72 [0.71, 0.73] | 0.55 [0.54, 0.55] | 0.47 [0.47, 0.68] | 0.93 [0.93, 0.93] | 0.95 [0.64, 0.95] |
| tet4-200k | ss | 63.6 [63.5, 64.1] | 42.6 [42.0, 42.9] | 24.6 [24.5, 24.7] | 17.6 [17.5, 17.7] | 0.66 [0.66, 0.68] | 0.39 [0.38, 0.39] | 0.28 [0.28, 0.28] | 0.94 [0.93, 0.94] | 0.99 [0.98, 0.99] |
| tet10-20k | ss | 269 [269, 271] | 158 [155, 158] | 90.8 [89.8, 91.2] | 66.4 [66.3, 66.7] | 0.58 [0.57, 0.59] | 0.34 [0.33, 0.34] | 0.25 [0.24, 0.25] | 0.99 [0.98, 0.99] | 1.00 [0.99, 1.0] |
| hex8-20k | ss | 367 [364, 367] | 194 [192, 195] | 117 [116, 117] | 94.6 [94.3, 95.1] | 0.53 [0.52, 0.53] | 0.32 [0.32, 0.32] | 0.26 [0.26, 0.26] | 0.93 [0.93, 0.94] | 1.00 [0.99, 1.00] |
| hex8-200k | ss | 380 [379, 384] | 205 [204, 206] | 138 [138, 138] | 126 [125, 127] | 0.54 [0.54, 0.54] | 0.36 [0.36, 0.36] | 0.33 [0.33, 0.34] | 0.93 [0.93, 0.93] | 1.0 [0.99, 1.0] |
| tet4-20k | fd | 65.4 [65.1, 65.9] | 44.5 [44.2, 44.7] | 32.7 [32.6, 32.8] | 26.1 [26.0, 26.2] | 0.68 [0.68, 0.68] | 0.50 [0.49, 0.50] | 0.40 [0.40, 0.40] | 0.93 [0.92, 0.93] | 0.98 [0.98, 0.99] |
| tet4-200k | fd | 81.5 [81.1, 81.6] | 53.1 [52.7, 53.3] | 29.2 [29.1, 29.3] | 19.0 [18.9, 19.0] | 0.65 [0.65, 0.65] | 0.36 [0.36, 0.36] | 0.23 [0.23, 0.23] | 0.91 [0.90, 0.92] | 0.97 [0.96, 0.97] |
| tet10-20k | fd | 377 [377, 379] | 216 [216, 219] | 122 [122, 122] | 77.7 [77.5, 78.9] | 0.57 [0.57, 0.58] | 0.32 [0.32, 0.32] | 0.21 [0.20, 0.21] | 0.94 [0.94, 0.95] | 0.97 [0.95, 0.97] |
| hex8-20k | fd | 541 [539, 543] | 296 [294, 297] | 158 [157, 167] | 110 [106, 111] | 0.55 [0.54, 0.55] | 0.29 [0.29, 0.31] | 0.20 [0.19, 0.21] | 0.91 [0.90, 0.91] | 0.93 [0.93, 0.96] |
| hex8-200k | fd | 562 [561, 572] | 321 [317, 322] | 175 [173, 176] | 135 [132, 136] | 0.57 [0.55, 0.57] | 0.31 [0.31, 0.31] | 0.24 [0.23, 0.24] | 0.89 [0.87, 0.89] | 0.95 [0.95, 0.98] |

- The step: port / C++ strict 0.29–0.40 (ss) and 0.15–0.25 (fd); port /
  C++ default 0.19–0.27 and 0.13–0.20; port / port before 0.66–0.72 and
  0.51–0.64. The port before took 0.44–0.55 (ss) and 0.30–0.40 (fd) of the
  strict time.
- Threads: at 8 threads the step takes 0.20–0.47 of the one-thread time;
  the largest ratio is the smallest mesh in small strain (tet4-20k, 0.47
  [0.47, 0.68]).
- Fast mode: 0.89–0.99 of the bitwise step at one thread, 0.93–1.00 at 8
  threads (one range reaches 0.64: tet4-20k ss).
- State digests: 90 program-configuration pairs, none differing.

#### Before the final campaign (Ryzen 7 9700X, 2026-11-12)

The explicit step of `examples/c++/solid_mechanics_model/explicit` (unit
cube, `elastic` with `rho = 2500`, `E = 70e9`, `nu = 0.22`, small strain (ss)
or `finite_deformation = true` (fd), `Δt = 0.8 Δt_crit`, a sine-Gaussian
displacement pulse along x), timed by the Rust harness
`crates/akantu-solid/examples/explicit_step_timing.rs` and its C++ twin
`scripts/perf/explicit-step/explicit_step_timing.cc` against the strict and
the default install. Meshes from `scripts/perf/explicit-step/cube_{tet,hex}.geo`
(gmsh 4.12.1, SHA-256 in `meshes-SHA256SUMS`).

Machine and method: boat.dev `large`, AMD Ryzen 7 9700X, 8 vCPU, 2026-11-12;
7 rounds with the program order rotated, one process per run, 3 warm-up
steps, then each phase over S calls after one untimed call (`timing_rotated.sh`,
`summarize_timing.py`). Nanoseconds per element and step, median [min, max]
over the rounds; ratios: median [min, max] of the per-round paired ratios.
"Port before" is the port before the explicit-step work (same session).
Data: `docs/benchmarks/explicit-step/final/summary.md`.

Table "Explicit step (`solve_step`), ns per element and step":

| mesh | type | elements | mode | C++ strict | C++ default | port before | port | port / C++ strict | port / C++ default | port before / C++ strict |
|---|---|---|---|---|---|---|---|---|---|---|
| tet4-20k | tetrahedron_4 | 19,120 | ss | 109 [82, 118] | 165 [130, 218] | 308 [269, 391] | 51 [42, 67] | 0.50 [0.47, 0.60] | 0.33 [0.23, 0.40] | 3.19 [2.77, 3.59] |
| tet4-20k | tetrahedron_4 | 19,120 | fd | 205 [177, 317] | 271 [224, 333] | 759 [622, 948] | 71 [62, 109] | 0.36 [0.26, 0.37] | 0.28 [0.23, 0.33] | 3.42 [2.84, 3.80] |
| tet4-200k | tetrahedron_4 | 178,772 | ss | 121 [105, 164] | 168 [152, 207] | 336 [301, 454] | 72 [63, 101] | 0.60 [0.54, 0.62] | 0.45 [0.40, 0.49] | 2.79 [2.53, 3.07] |
| tet4-200k | tetrahedron_4 | 178,772 | fd | 223 [206, 281] | 274 [241, 300] | 733 [636, 854] | 97 [88, 116] | 0.42 [0.37, 0.45] | 0.37 [0.34, 0.39] | 3.04 [2.91, 3.28] |
| tet10-20k | tetrahedron_10 | 19,120 | ss | 611 [550, 891] | 809 [718, 1090] | 1817 [1696, 2549] | 356 [280, 511] | 0.57 [0.47, 0.58] | 0.44 [0.36, 0.48] | 2.86 [2.55, 3.08] |
| tet10-20k | tetrahedron_10 | 19,120 | fd | 1578 [1436, 2195] | 1590 [1542, 2373] | 4754 [4511, 6696] | 493 [444, 578] | 0.31 [0.23, 0.34] | 0.31 [0.21, 0.31] | 3.14 [2.81, 3.27] |
| hex8-20k | hexahedron_8 | 19,683 | ss | 953 [912, 1363] | 1265 [1148, 1590] | 3013 [2812, 3734] | 491 [407, 635] | 0.49 [0.44, 0.60] | 0.38 [0.35, 0.50] | 3.08 [2.56, 3.34] |
| hex8-20k | hexahedron_8 | 19,683 | fd | 2507 [2237, 3391] | 2621 [2343, 3487] | 8507 [7335, 10954] | 897 [758, 1078] | 0.34 [0.32, 0.38] | 0.33 [0.27, 0.37] | 3.39 [2.70, 3.68] |
| hex8-200k | hexahedron_8 | 195,112 | ss | 1041 [920, 1274] | 1379 [1134, 1574] | 3623 [3233, 4512] | 513 [449, 755] | 0.49 [0.44, 0.59] | 0.40 [0.34, 0.55] | 3.49 [3.18, 3.67] |
| hex8-200k | hexahedron_8 | 195,112 | fd | 2792 [2349, 3106] | 2885 [2457, 3523] | 10071 [7953, 11157] | 927 [847, 1092] | 0.35 [0.32, 0.39] | 0.33 [0.26, 0.38] | 3.60 [3.17, 3.75] |

- Small strain: 0.49–0.60× the strict build, finite deformation 0.31–0.42×;
  against the default build 0.33–0.45× and 0.28–0.37×. The memory-bound
  case (tet4, 179 k elements, small strain) is the closest to C++.
- The internal-force assembly (`fint`: stresses and the `−∫Bᵀσ` assembly)
  takes 0.25–0.60× the strict time, the stress computation (gradient and
  constitutive law) 0.36–0.59× (`summary.md`).
- Before this work the port took 2.79–3.49× (ss) and 3.04–3.60× (fd) the
  strict time; the current code takes 0.10–0.21× of that.
#### What was optimised

Before the work, a profile (`perf record --call-graph dwarf`, tet4 with
179 k elements, Ryzen 9 9950X, 2026-11-11) showed 42 % of the small-strain
run in a generic emulation of Eigen's evaluation order for the small
fixed-size products of every quadrature point (`redux_tree` 17.7 %,
`product_into` 10.2 %, `product_slices` 7.8 %, `route` 3.0 %), 13 % in page
faults of per-step array allocations and 8 % in zero fills; in finite
deformation the emulation took 39 %. The changes keep every result bit (each
specialised kernel has a property test against the general kernel on ≥ 10⁵
random cases, signed zeros, infinities, NaNs and subnormals included, and
every oracle replay passes bitwise; `docs/NUMERICAL_VALIDATION.md`).

| Change | Measured gain (whole step, ns per element, before → after) |
|---|---|
| fixed-size kernels for the gradient, `D·B` and the integration, same reduction trees, route chosen once per batch | ss tet4-20k 373 → 117, hex8-20k 3,750 → 1,279; gradient kernel ×0.06–0.17 |
| fixed-size finite-deformation residual `(B_L0 + B_L1)ᵀ·S` on stack arrays | fd tet4-20k 594 → 277, hex8-20k 7,209 → 2,609 |
| no per-step temporaries in the stress path | stress ×0.63–0.94 |
| internal-force assembly buffers kept between steps | ss tet4-200k 151 → 95, hex8-20k 793 → 649 |
| straight-line Green strain product `∇uᵀ·∇u` | fd tet4-20k 217 → 139, hex8-20k 2,186 → 1,602 |
| fused finite-deformation residual (no zero fill per point) | fd tet4-20k 139 → 92, hex8-20k 1,602 → 1,110 |
| element scatter without filter copy | scatter ×0.54–0.64 |
| cohesive Cauchy pass with a fixed-size kernel | cohesive step ×0.40 (tet4-20k), ×0.49 (tet4-200k) |

Each row is an interleaved A/B against its predecessor in one session (Ryzen
9 9950X, 2026-11-11, 5 rounds; `docs/benchmarks/explicit-step/development/tables.md`,
made with `scripts/perf/explicit-step/tables.py` from `run_ab.sh` output).
After the work no generic product is left in the step. The small-strain
profile is `fast::gradient` 15 %, `fast::btd` 12 %, the scatter 10 %,
`fast::integrate` 9.6 %, the elastic stress 5.9 % and the eigen-gradient
subtraction 5.4 %; for hex8 in small strain `fast::btd` 22 %, the gradient
14 %, the integration 9.8 % and `memcpy` about 13 %, which is
`save_previous_state` copying `stress`, `grad_u`, `sigma_th` and
`epsilon_th` into their history each step, as upstream's `savePreviousState`
does.

#### The cohesive step and the benchmark driver

The `akantu-cohesive` example `cohesive_step_timing` (the benchmark's
materials: bulk `elastic` with finite deformation, extrinsic
`cohesive_linear`, every interior facet checked, Δt = 0.1 Δt_crit, a tensile
ramp to 30 % of σ_c, no insertion) took ×0.31 of its earlier time in the
step at 20 k, 179 k and 874 k tetrahedra, and the Cauchy-pass kernel a
further ×0.36–0.62 (Ryzen 9 9950X, 2026-11-11). Before insertion the
cohesive step now costs about what the plain finite-deformation step costs
(134–176 against 116–175 ns per element). The insertion check took
×0.76–0.98 of its earlier time; at 871–1,670 ns per element it is the larger
part of the cohesive step.

In the benchmark driver (h = 0.12 frozen mesh, 23,156 tetrahedra, 500 steps
with the static preload, no insertion within them, 5 rotated repetitions;
Ryzen 7 9700X, 2026-11-12; `crates/akantu-bench/scripts/timing_ab.sh`,
`docs/benchmarks/explicit-step/final/driver/`), ms per step:

| material | phase | before [ms/step] | after [ms/step] | after / before (paired) |
|---|---|---|---|---|
| cohesive | `solve_step` | 17.35 [16.84, 17.80] | 2.29 [2.22, 2.42] | 0.132 [0.130, 0.137] |
| cohesive | `check_cohesive_stress` | 9.34 [9.00, 9.67] | 8.80 [8.50, 9.67] | 0.942 [0.926, 1.006] |
| elastic | `solve_step` | 16.95 [16.86, 17.11] | 1.90 [1.83, 1.95] | 0.111 [0.108, 0.115] |

Peak RSS grew from 456 to 464–465 MiB (the assembly buffers kept between
steps). The insertion regime of the driver (294 insertion steps) is replayed
bitwise by the tests but was not timed in this series.

#### What remained in the serial step (as of 2026-11-12)

Ranked by share of the profile; none was worked on before the performance
campaign. Items 1–4, 6 and 7 were worked on in the campaign
(`docs/benchmarks/perf-campaign/explicit/README.md`,
`docs/benchmarks/perf-campaign/step-state/README.md`,
`docs/benchmarks/perf-campaign/cohesive-check/README.md`); the final
campaign's tables above measure the result.

1. `D·B` for many quadrature points (22 % of hex8 ss, about 15 % of tet10):
   fusing `computeBtD` with the integration would avoid writing and reading
   back a per-point array.
2. The gradient (14–15 %): one validation pass over the connectivity instead
   of a checked access per node.
3. The scatter to nodes (9–10 % for tet4): memory-bound random access in
   element order; the order sets each node's summation order and must stay.
4. The eigen-gradient subtraction and the stress loop (5–6 % each).
5. Per-node solver loops (blocked flags copied twice per step,
   `update_global_blocked_dofs` in O(n_dof) every step), more visible on
   hex8.
6. The history copies (`save_previous_state`, about 13 % of hex8 ss) are
   upstream's behaviour. Plain double buffering would change the buffer
   identity that zero-copy Python views hold (D-047); the performance
   campaign removed them without that: a save of a block that is not
   exported is deferred, and becomes an exchange of allocations when the
   writer rewrites the whole block; an exported block is still copied in
   place (`docs/benchmarks/perf-campaign/step-state/`).
7. The insertion check's stress interpolation used the general product per
   point; reworked in the performance campaign (next section).

### Development records of the performance campaign

The changes of the performance campaign (16 and 17 November) were measured
branch by branch while they were made. Those records are development
records: each compares one change with its predecessor, on development
machines or AWS machines shared with other jobs, and the final campaign
replaces them as results. Their summaries, moved from this document, are in
`docs/benchmarks/perf-campaign/README.md`; the data are in the branch
directories it lists.

## The benchmark suites

The port's own benchmark suites (`crates/akantu-bench`: `suite_solid`,
`suite_models`, `suite_setup`, `suite_python.py`), every case at the
baseline's size `m`, the port before against the head, five repetitions
with the variants of a case adjacent and rotated. No suite case has a C++
twin. Final campaign, part B: AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17
UTC; one process on core 2, N threads on cores 0..N−1; the distributed cases
at 1 / 2 / 4 / 8 ranks, rank r on core r, recorded and checked. Seconds of
the phase over its calls, median [min, max]; ratios paired per repetition.
The tables below show the phases named in the cases' descriptions; every
phase of every case is in `docs/benchmarks/perf-campaign/final/B/summary.md`
(the sparse solvers' phases also in part D).

Table "suite_solid, serial cases" (selected phases):

| case | phase | calls | n | before | head | head / before | head 2 threads / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1) | fast / bitwise (8) | digests |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| cohesive_extrinsic_check | check_insertion | 100 | 5 | 4.86 s [4.86 s, 4.88 s] | 1.41 s [1.41 s, 1.42 s] | 0.290 [0.290, 0.291] | 0.570 [0.564, 0.581] | 0.366 [0.363, 0.367] | 0.345 [0.343, 0.345] | 0.996 [0.985, 1.00] | 1.00 [0.998, 1.00] | equal |
| cohesive_extrinsic_check | solve_step | 100 | 5 | 1.17 s [1.16 s, 1.17 s] | 829 ms [823 ms, 840 ms] | 0.712 [0.704, 0.721] | 0.697 [0.691, 0.711] | 0.562 [0.553, 0.570] | 0.513 [0.511, 0.526] | 0.962 [0.947, 0.968] | 0.985 [0.959, 1.00] | equal |
| cohesive_extrinsic_insertion | check_insertion | 16 | 5 | 2.24 s [2.23 s, 2.25 s] | 1.58 s [1.57 s, 1.59 s] | 0.707 [0.699, 0.708] | 0.976 [0.960, 0.982] | 0.955 [0.946, 0.957] | 0.948 [0.942, 0.961] | 1.00 [0.995, 1.01] | 0.998 [0.989, 1.01] | equal |
| cohesive_extrinsic_insertion | solve_step | 16 | 5 | 299 ms [297 ms, 301 ms] | 193 ms [190 ms, 193 ms] | 0.641 [0.640, 0.647] | 0.796 [0.792, 0.802] | 0.684 [0.670, 0.687] | 0.641 [0.631, 0.645] | 0.979 [0.969, 0.998] | 0.980 [0.977, 1.00] | equal |
| cohesive_intrinsic_bilinear | solve_step | 300 | 5 | 6.27 s [6.23 s, 6.28 s] | 3.41 s [3.29 s, 3.46 s] | 0.548 [0.524, 0.551] | 0.576 [0.571, 0.595] | 0.401 [0.386, 0.406] | 0.325 [0.322, 0.339] | 0.990 [0.986, 1.04] | 1.01 [0.982, 1.15] | equal |
| cohesive_intrinsic_exponential | solve_step | 300 | 5 | 7.99 s [7.96 s, 8.08 s] | 5.34 s [5.32 s, 5.39 s] | 0.670 [0.659, 0.674] | 0.785 [0.775, 0.794] | 0.691 [0.687, 0.701] | 0.646 [0.640, 0.655] | 0.999 [0.984, 1.02] | 1.00 [0.992, 1.03] | equal |
| cohesive_intrinsic_implicit | solve_step | 30 | 5 | 9.78 s [9.77 s, 9.83 s] | 2.86 s [2.85 s, 2.90 s] | 0.292 [0.290, 0.297] | 0.986 [0.973, 1.01] | 0.881 [0.873, 0.893] | 0.876 [0.864, 0.883] | 1.00 [0.990, 1.01] | 1.01 [1.01, 1.03] | equal |
| explicit_hex8_fd | solve_step | 30 | 5 | 3.50 s [3.50 s, 3.52 s] | 1.78 s [1.77 s, 1.81 s] | 0.508 [0.506, 0.513] | 0.534 [0.525, 0.540] | 0.307 [0.299, 0.309] | 0.248 [0.245, 0.251] | 0.910 [0.898, 0.915] | 0.986 [0.972, 0.998] | equal |
| explicit_hex8_ss | solve_step | 50 | 5 | 3.13 s [3.12 s, 3.15 s] | 2.07 s [2.05 s, 2.07 s] | 0.658 [0.656, 0.663] | 0.549 [0.547, 0.552] | 0.372 [0.369, 0.373] | 0.342 [0.341, 0.344] | 0.937 [0.936, 0.950] | 1.01 [0.996, 1.01] | equal |
| explicit_marigo | solve_step | 200 | 5 | 4.69 s [4.68 s, 4.71 s] | 2.02 s [2.01 s, 2.03 s] | 0.432 [0.429, 0.433] | 0.571 [0.566, 0.577] | 0.393 [0.387, 0.397] | 0.345 [0.344, 0.347] | 0.973 [0.966, 0.977] | 1.02 [1.01, 1.02] | equal |
| explicit_maxwell | solve_step | 40 | 5 | 6.75 s [6.73 s, 6.78 s] | 654 ms [651 ms, 680 ms] | 0.0965 [0.0964, 0.101] | 0.545 [0.521, 0.547] | 0.364 [0.349, 0.367] | 0.335 [0.322, 0.339] | 0.982 [0.942, 0.992] | 1.02 [1.01, 1.02] | equal |
| explicit_mazars | solve_step | 80 | 5 | 6.87 s [6.84 s, 7.39 s] | 3.47 s [3.46 s, 3.51 s] | 0.504 [0.470, 0.511] | 0.539 [0.536, 0.547] | 0.329 [0.326, 0.336] | 0.224 [0.222, 0.225] | 1.01 [0.998, 1.01] | 0.998 [0.981, 1.18] | equal |
| explicit_mazars_non_local | solve_step | 40 | 5 | 7.40 s [7.39 s, 7.52 s] | 2.06 s [2.05 s, 2.07 s] | 0.277 [0.272, 0.280] | 0.749 [0.731, 0.751] | 0.623 [0.619, 0.631] | 0.376 [0.367, 0.379] | 1.00 [0.988, 1.02] | 1.000 [0.993, 1.01] | equal |
| explicit_multi_material | solve_step | 120 | 5 | 5.35 s [5.34 s, 5.65 s] | 2.68 s [2.68 s, 2.69 s] | 0.501 [0.475, 0.502] | 0.542 [0.535, 0.546] | 0.326 [0.322, 0.329] | 0.232 [0.231, 0.248] | 0.993 [0.993, 1.01] | 1.00 [0.929, 1.01] | equal |
| explicit_neohookean | solve_step | 100 | 5 | 3.03 s [2.97 s, 3.04 s] | 1.33 s [1.29 s, 1.34 s] | 0.440 [0.423, 0.448] | 0.528 [0.527, 0.571] | 0.311 [0.308, 0.331] | 0.222 [0.218, 0.228] | 0.975 [0.913, 1.01] | 1.00 [0.974, 1.06] | equal |
| explicit_orthotropic | solve_step | 200 | 5 | 2.61 s [2.61 s, 2.61 s] | 1.30 s [1.30 s, 1.30 s] | 0.498 [0.497, 0.499] | 0.555 [0.554, 0.574] | 0.339 [0.338, 0.340] | 0.289 [0.284, 0.319] | 0.958 [0.957, 0.970] | 0.987 [0.901, 1.01] | equal |
| explicit_plastic | solve_step | 120 | 5 | 6.64 s [6.55 s, 6.66 s] | 1.38 s [1.38 s, 1.38 s] | 0.208 [0.207, 0.210] | 0.561 [0.559, 0.567] | 0.369 [0.367, 0.370] | 0.328 [0.327, 0.334] | 0.978 [0.974, 0.982] | 0.989 [0.972, 0.992] | equal |
| explicit_sls | solve_step | 120 | 5 | 5.21 s [5.14 s, 5.25 s] | 989 ms [984 ms, 994 ms] | 0.191 [0.188, 0.193] | 0.548 [0.542, 0.599] | 0.362 [0.360, 0.365] | 0.342 [0.339, 0.356] | 0.972 [0.967, 0.985] | 1.00 [0.955, 1.03] | equal |
| explicit_tet10_fd | solve_step | 40 | 5 | 2.91 s [2.91 s, 2.92 s] | 1.58 s [1.57 s, 1.58 s] | 0.542 [0.540, 0.543] | 0.583 [0.580, 0.585] | 0.343 [0.340, 0.345] | 0.281 [0.280, 0.286] | 0.941 [0.938, 0.944] | 0.980 [0.968, 0.985] | equal |
| explicit_tet10_ss | solve_step | 50 | 5 | 2.11 s [2.10 s, 2.13 s] | 1.44 s [1.43 s, 1.44 s] | 0.681 [0.677, 0.683] | 0.614 [0.611, 0.617] | 0.409 [0.406, 0.412] | 0.374 [0.371, 0.375] | 0.971 [0.962, 0.975] | 0.993 [0.991, 0.996] | equal |
| explicit_tet4_fd | solve_step | 150 | 5 | 2.83 s [2.81 s, 2.84 s] | 1.66 s [1.66 s, 1.67 s] | 0.586 [0.586, 0.592] | 0.528 [0.527, 0.530] | 0.306 [0.304, 0.309] | 0.226 [0.226, 0.227] | 0.955 [0.952, 0.958] | 0.993 [0.990, 0.999] | equal |
| explicit_tet4_ss | solve_step | 200 | 5 | 2.62 s [2.61 s, 2.64 s] | 1.76 s [1.75 s, 1.77 s] | 0.670 [0.668, 0.674] | 0.534 [0.532, 0.537] | 0.332 [0.330, 0.336] | 0.269 [0.267, 0.272] | 0.968 [0.964, 0.970] | 1.00 [0.996, 1.25] | equal |
| explicit_thermal | solve_step | 300 | 5 | 2.50 s [2.50 s, 2.51 s] | 1.66 s [1.65 s, 1.66 s] | 0.662 [0.659, 0.665] | 0.530 [0.527, 0.550] | 0.317 [0.316, 0.318] | 0.242 [0.240, 0.246] | 0.967 [0.962, 0.992] | 0.996 [0.977, 1.00] | equal |
| explicit_tri3_fd | solve_step | 200 | 5 | 2.48 s [2.47 s, 2.51 s] | 1.93 s [1.93 s, 1.94 s] | 0.779 [0.769, 0.782] | 0.509 [0.505, 0.510] | 0.304 [0.301, 0.307] | 0.226 [0.224, 0.226] | 0.953 [0.946, 0.977] | 0.984 [0.976, 0.987] | equal |
| explicit_tri3_ss | solve_step | 300 | 5 | 3.26 s [3.25 s, 3.31 s] | 2.16 s [2.09 s, 2.16 s] | 0.653 [0.642, 0.665] | 0.516 [0.511, 0.529] | 0.330 [0.325, 0.342] | 0.278 [0.277, 0.283] | 0.994 [0.982, 1.01] | 0.999 [0.995, 1.01] | equal |
| implicit_dynamic | solve_step | 60 | 5 | 2.66 s [2.66 s, 2.70 s] | 1.06 s [1.06 s, 1.06 s] | 0.396 [0.393, 0.398] | 0.921 [0.916, 0.921] | 0.898 [0.889, 0.904] | 0.889 [0.886, 0.890] | 0.991 [0.988, 0.996] | 0.997 [0.992, 1.000] | equal |
| setup_hex8 | init_explicit | 1 | 5 | 107 ms [107 ms, 108 ms] | 60.2 ms [59.7 ms, 61.1 ms] | 0.562 [0.552, 0.570] | — | — | — | — | — | equal |
| setup_hex8 | stiffness_assembly | 3 | 5 | 3.05 s [3.03 s, 3.08 s] | 447 ms [444 ms, 448 ms] | 0.147 [0.145, 0.148] | — | — | — | — | — | equal |
| setup_tet10 | init_explicit | 1 | 5 | 52.3 ms [52.0 ms, 53.1 ms] | 29.9 ms [29.6 ms, 30.0 ms] | 0.570 [0.562, 0.574] | — | — | — | — | — | equal |
| setup_tet10 | stiffness_assembly | 3 | 5 | 2.35 s [2.34 s, 2.36 s] | 253 ms [251 ms, 256 ms] | 0.107 [0.107, 0.109] | — | — | — | — | — | equal |
| setup_tet4 | init_explicit | 1 | 5 | 125 ms [124 ms, 125 ms] | 83.0 ms [82.3 ms, 83.3 ms] | 0.663 [0.659, 0.670] | — | — | — | — | — | equal |
| setup_tet4 | stiffness_assembly | 3 | 5 | 3.10 s [3.09 s, 3.12 s] | 299 ms [299 ms, 303 ms] | 0.0966 [0.0958, 0.0976] | — | — | — | — | — | equal |
| static_linear_eigen | solve_step | 1 | 5 | 3.05 s [3.04 s, 3.05 s] | 685 ms [683 ms, 693 ms] | 0.225 [0.224, 0.228] | 0.893 [0.887, 0.901] | 0.825 [0.825, 0.830] | 0.789 [0.780, 0.795] | 0.954 [0.942, 0.955] | 1.01 [0.999, 1.01] | equal |
| static_linear_eigen | stiffness_assembly | 3 | 5 | 185 ms [184 ms, 190 ms] | 29.9 ms [29.8 ms, 30.5 ms] | 0.162 [0.158, 0.163] | 0.589 [0.579, 0.595] | 0.331 [0.327, 0.339] | 0.201 [0.198, 0.203] | 0.992 [0.976, 0.999] | 1.00 [0.993, 1.01] | equal |
| static_linear_faer | solve_step | 1 | 5 | 4.97 s [4.95 s, 5.05 s] | 4.81 s [4.79 s, 4.82 s] | 0.965 [0.954, 0.974] | 1.00 [0.997, 1.01] | 1.00 [0.998, 1.02] | 1.00 [0.998, 1.01] | 1.00 [0.999, 1.01] | 1.00 [0.995, 1.03] | equal |
| static_linear_faer | stiffness_assembly | 3 | 5 | 248 ms [243 ms, 249 ms] | 30.1 ms [30.1 ms, 30.9 ms] | 0.122 [0.121, 0.124] | 0.583 [0.567, 0.587] | 0.331 [0.320, 0.335] | 0.198 [0.194, 0.200] | 0.987 [0.960, 0.991] | 1.01 [0.997, 1.79] | equal |
| static_newton_plastic | solve_step | 12 | 5 | 3.02 s [3.01 s, 3.05 s] | 1.64 s [1.63 s, 1.66 s] | 0.541 [0.540, 0.550] | 0.850 [0.834, 0.851] | 0.811 [0.807, 0.838] | 0.816 [0.799, 0.825] | 0.992 [0.980, 0.999] | 0.986 [0.966, 0.993] | equal |

Table "suite_models, serial cases" (selected phases):

| case | phase | calls | n | before | head | head / before | head 2 threads / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1) | fast / bitwise (8) | digests |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| contact_cohesive_quad4 | solve_step | 300 | 5 | 8.65 s [8.59 s, 8.66 s] | 7.55 s [7.54 s, 7.58 s] | 0.873 [0.872, 0.881] | 0.880 [0.877, 0.885] | 0.850 [0.847, 0.854] | 0.842 [0.841, 0.846] | 0.991 [0.985, 0.992] | 1.00 [0.999, 1.02] | equal |
| contact_explicit_quad4 | solve_step | 200 | 5 | 4.51 s [4.50 s, 4.53 s] | 3.37 s [3.36 s, 3.38 s] | 0.747 [0.745, 0.750] | 0.670 [0.667, 0.677] | 0.583 [0.581, 0.587] | 0.570 [0.568, 0.574] | 0.989 [0.974, 0.996] | 1.00 [0.995, 1.01] | equal |
| contact_implicit_static_quad4 | solve_step | 5 | 5 | 2.12 s [2.12 s, 2.18 s] | 710 ms [709 ms, 710 ms] | 0.334 [0.326, 0.335] | 0.947 [0.945, 0.949] | 0.938 [0.935, 0.970] | 0.929 [0.927, 0.935] | 1.03 [1.03, 1.03] | 1.02 [1.01, 1.02] | equal |
| contact_search_quad4 | search | 60 | 5 | 5.39 s [5.39 s, 5.41 s] | 4.00 s [3.99 s, 4.06 s] | 0.740 [0.739, 0.753] | — | — | — | — | — | equal |
| contact_search_tet4 | search | 30 | 5 | 5.10 s [5.06 s, 5.12 s] | 3.09 s [3.08 s, 3.11 s] | 0.606 [0.605, 0.614] | — | — | — | — | — | equal |
| embedded_rebar_tet4 | intersect | 5 | 5 | 7.05 s [7.01 s, 7.15 s] | 87.5 ms [87.4 ms, 88.2 ms] | 0.0124 [0.0122, 0.0126] | 0.998 [0.991, 1.09] | 1.00 [0.988, 1.02] | 0.999 [0.992, 1.03] | 0.999 [0.991, 1.00] | 0.998 [0.969, 0.999] | equal |
| embedded_rebar_tet4 | solve_step | 1 | 5 | 4.21 s [4.20 s, 4.23 s] | 1.06 s [1.06 s, 1.07 s] | 0.253 [0.251, 0.254] | 0.859 [0.845, 0.869] | 0.775 [0.770, 0.786] | 0.732 [0.725, 0.735] | 0.947 [0.938, 0.952] | 0.999 [0.988, 1.01] | equal |
| embedded_rebar_tri3 | intersect | 5 | 5 | 7.82 s [7.82 s, 7.88 s] | 62.9 ms [62.5 ms, 63.4 ms] | 0.00800 [0.00798, 0.00811] | 1.00 [0.995, 1.02] | 1.01 [0.997, 1.01] | 1.01 [1.00, 1.03] | 1.00 [0.985, 1.01] | 0.998 [0.995, 1.00] | equal |
| embedded_rebar_tri3 | solve_step | 1 | 5 | 543 ms [543 ms, 547 ms] | 230 ms [228 ms, 232 ms] | 0.423 [0.420, 0.424] | 0.965 [0.962, 0.975] | 0.966 [0.951, 0.976] | 0.969 [0.959, 0.981] | 0.984 [0.978, 0.996] | 0.996 [0.981, 1.00] | equal |
| heat_explicit_quad4 | solve_step | 120 | 5 | 4.61 s [4.60 s, 4.62 s] | 1.75 s [1.74 s, 1.76 s] | 0.380 [0.377, 0.380] | 0.534 [0.527, 0.537] | 0.394 [0.390, 0.395] | 0.357 [0.356, 0.359] | 1.00 [0.998, 1.01] | 1.00 [0.998, 1.01] | equal |
| heat_explicit_tet4 | solve_step | 100 | 5 | 3.91 s [3.90 s, 3.93 s] | 1.20 s [1.19 s, 1.21 s] | 0.307 [0.306, 0.307] | 0.531 [0.527, 0.533] | 0.343 [0.342, 0.344] | 0.279 [0.277, 0.280] | 0.994 [0.985, 0.997] | 1.01 [1.01, 1.01] | equal |
| heat_implicit_hex8_kvar | solve_step | 5 | 5 | 1.35 s [1.35 s, 1.35 s] | 536 ms [535 ms, 539 ms] | 0.397 [0.396, 0.399] | 0.952 [0.947, 0.958] | 0.947 [0.939, 0.954] | 0.932 [0.928, 0.942] | 0.971 [0.965, 0.977] | 0.985 [0.979, 0.987] | equal |
| heat_static_tri6 | solve_step | 1 | 5 | 4.47 s [4.46 s, 4.48 s] | 1.68 s [1.67 s, 1.70 s] | 0.376 [0.375, 0.379] | 0.991 [0.989, 0.992] | 0.999 [0.993, 1.01] | 0.999 [0.984, 1.01] | 1.01 [0.997, 1.01] | 1.03 [1.01, 1.03] | equal |
| ntn_friction_quad4 | solve_step | 100 | 5 | 3.49 s [3.48 s, 3.53 s] | 2.47 s [2.45 s, 2.50 s] | 0.710 [0.702, 0.716] | 0.645 [0.636, 0.649] | 0.547 [0.542, 0.551] | 0.530 [0.527, 0.535] | 0.984 [0.980, 0.996] | 1.00 [0.994, 1.01] | equal |
| phasefield_staggered_tet4 | phase_solve | 4 | 5 | 141 ms [141 ms, 142 ms] | 58.6 ms [58.4 ms, 59.6 ms] | 0.414 [0.411, 0.421] | 0.984 [0.970, 0.990] | 0.992 [0.970, 1.00] | 0.992 [0.986, 1.01] | 0.987 [0.966, 0.991] | 0.978 [0.971, 0.999] | equal |
| phasefield_staggered_tet4 | solid_solve | 4 | 5 | 2.24 s [2.24 s, 2.24 s] | 487 ms [486 ms, 494 ms] | 0.218 [0.217, 0.221] | 0.972 [0.967, 0.979] | 0.938 [0.930, 0.941] | 0.903 [0.886, 0.907] | 0.955 [0.946, 0.966] | 1.00 [0.999, 1.01] | equal |
| phasefield_staggered_tri3 | phase_solve | 4 | 5 | 2.10 s [2.09 s, 2.10 s] | 746 ms [746 ms, 753 ms] | 0.356 [0.356, 0.360] | 0.931 [0.927, 0.934] | 0.917 [0.908, 0.917] | 0.912 [0.903, 0.924] | 0.977 [0.968, 0.979] | 0.989 [0.975, 0.990] | equal |
| phasefield_staggered_tri3 | solid_solve | 4 | 5 | 11.4 s [11.4 s, 11.4 s] | 2.98 s [2.95 s, 3.00 s] | 0.262 [0.259, 0.263] | 0.894 [0.893, 0.897] | 0.862 [0.858, 0.876] | 0.844 [0.838, 0.861] | 0.968 [0.962, 0.984] | 0.995 [0.984, 1.00] | equal |
| structural_beam3_frame_dynamic | solve_step | 60 | 5 | 5.05 s [5.01 s, 5.06 s] | 2.93 s [2.93 s, 2.94 s] | 0.582 [0.580, 0.585] | 0.993 [0.990, 0.997] | 0.989 [0.985, 0.996] | 0.986 [0.986, 0.988] | 0.998 [0.995, 0.999] | 0.998 [0.993, 1.00] | equal |
| structural_beam3_frame_static | solve_step | 1 | 5 | 5.86 s [5.86 s, 5.87 s] | 1.38 s [1.37 s, 1.38 s] | 0.236 [0.234, 0.236] | 0.761 [0.756, 0.763] | 0.649 [0.644, 0.653] | 0.588 [0.584, 0.594] | 0.893 [0.887, 0.901] | 0.971 [0.965, 1.02] | equal |
| structural_dkt18_plate_static | solve_step | 1 | 5 | 2.64 s [2.61 s, 2.65 s] | 854 ms [850 ms, 856 ms] | 0.323 [0.322, 0.328] | 0.975 [0.972, 0.991] | 0.975 [0.960, 0.988] | 0.982 [0.972, 0.987] | 0.984 [0.979, 0.986] | 0.990 [0.980, 1.00] | equal |

Table "suite_solid, distributed cases at 1, 2, 4, 8 ranks" (selected
phases; "head ranks x threads" is the head with P ranks × T threads, P × T =
8; its last column divides the medians of two groups, so it is unpaired):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| par_cohesive_extrinsic | check_insertion | 1 | 5 | 2.21 s [2.20 s, 2.21 s] | 1.59 s [1.59 s, 1.59 s] | 0.720 [0.719, 0.726] |  |  |  | equal |
| par_cohesive_extrinsic | check_insertion | 2 | 5 | 1.41 s [1.41 s, 1.42 s] | 923 ms [922 ms, 927 ms] | 0.653 [0.650, 0.656] | 2 x 4 | 903 ms [900 ms, 915 ms] | 2.41 | equal |
| par_cohesive_extrinsic | check_insertion | 4 | 5 | 821 ms [820 ms, 840 ms] | 554 ms [550 ms, 558 ms] | 0.674 [0.659, 0.680] | 4 x 2 | 555 ms [554 ms, 558 ms] | 1.48 | equal |
| par_cohesive_extrinsic | check_insertion | 8 | 5 | 520 ms [511 ms, 521 ms] | 375 ms [363 ms, 383 ms] | 0.723 [0.712, 0.735] |  |  |  | equal |
| par_cohesive_extrinsic | solve_step | 1 | 5 | 302 ms [301 ms, 303 ms] | 192 ms [189 ms, 194 ms] | 0.635 [0.626, 0.642] |  |  |  | equal |
| par_cohesive_extrinsic | solve_step | 2 | 5 | 186 ms [185 ms, 187 ms] | 119 ms [118 ms, 120 ms] | 0.639 [0.635, 0.646] | 2 x 4 | 97.2 ms [93.9 ms, 101 ms] | 1.35 | equal |
| par_cohesive_extrinsic | solve_step | 4 | 5 | 123 ms [123 ms, 124 ms] | 83.1 ms [81.8 ms, 83.8 ms] | 0.675 [0.663, 0.680] | 4 x 2 | 77.6 ms [76.3 ms, 78.4 ms] | 1.08 | equal |
| par_cohesive_extrinsic | solve_step | 8 | 5 | 106 ms [104 ms, 116 ms] | 71.8 ms [67.6 ms, 80.9 ms] | 0.690 [0.626, 0.764] |  |  |  | equal |
| par_explicit_tet4_ss | solve_step | 1 | 5 | 2.59 s [2.55 s, 2.62 s] | 1.75 s [1.75 s, 1.76 s] | 0.680 [0.669, 0.685] |  |  |  | equal |
| par_explicit_tet4_ss | solve_step | 2 | 5 | 1.91 s [1.88 s, 1.95 s] | 1.08 s [1.07 s, 1.09 s] | 0.564 [0.551, 0.574] | 2 x 4 | 917 ms [910 ms, 922 ms] | 0.916 | equal |
| par_explicit_tet4_ss | solve_step | 4 | 5 | 1.60 s [1.59 s, 1.61 s] | 919 ms [912 ms, 927 ms] | 0.576 [0.570, 0.584] | 4 x 2 | 943 ms [922 ms, 947 ms] | 0.942 | equal |
| par_explicit_tet4_ss | solve_step | 8 | 5 | 1.67 s [1.66 s, 1.68 s] | 1.00 s [990 ms, 1.01 s] | 0.600 [0.596, 0.603] |  |  |  | equal |

Table "suite_models, distributed cases at 1, 2, 4, 8 ranks" (selected
phases):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| heat_explicit_tet4 | solve_step | 1 | 5 | 3.89 s [3.89 s, 4.02 s] | 1.19 s [1.19 s, 1.19 s] | 0.305 [0.297, 0.306] |  |  |  | equal |
| heat_explicit_tet4 | solve_step | 2 | 5 | 2.31 s [2.29 s, 2.32 s] | 855 ms [849 ms, 855 ms] | 0.371 [0.366, 0.374] | 2 x 4 | 583 ms [574 ms, 588 ms] | 0.988 | equal |
| heat_explicit_tet4 | solve_step | 4 | 5 | 1.42 s [1.42 s, 1.43 s] | 642 ms [638 ms, 651 ms] | 0.450 [0.446, 0.458] | 4 x 2 | 590 ms [584 ms, 599 ms] | 1.00 | equal |
| heat_explicit_tet4 | solve_step | 8 | 5 | 1.08 s [1.06 s, 1.08 s] | 590 ms [589 ms, 606 ms] | 0.550 [0.545, 0.569] |  |  |  | equal |
| phasefield_staggered_tri3 | phase_solve | 1 | 5 | 2.09 s [2.08 s, 2.09 s] | 749 ms [745 ms, 752 ms] | 0.359 [0.357, 0.360] |  |  |  | equal |
| phasefield_staggered_tri3 | phase_solve | 2 | 5 | 1.86 s [1.78 s, 1.88 s] | 706 ms [701 ms, 713 ms] | 0.380 [0.378, 0.396] | 2 x 4 | 680 ms [678 ms, 683 ms] | 1.03 | equal |
| phasefield_staggered_tri3 | phase_solve | 4 | 5 | 1.78 s [1.74 s, 1.85 s] | 677 ms [674 ms, 678 ms] | 0.380 [0.364, 0.390] | 4 x 2 | 668 ms [668 ms, 675 ms] | 1.01 | equal |
| phasefield_staggered_tri3 | phase_solve | 8 | 5 | 1.61 s [1.59 s, 1.65 s] | 662 ms [660 ms, 666 ms] | 0.412 [0.401, 0.419] |  |  |  | equal |
| phasefield_staggered_tri3 | solid_solve | 1 | 5 | 11.4 s [11.4 s, 11.4 s] | 2.94 s [2.92 s, 3.04 s] | 0.259 [0.257, 0.268] |  |  |  | equal |
| phasefield_staggered_tri3 | solid_solve | 2 | 5 | 11.3 s [11.2 s, 11.3 s] | 3.09 s [3.09 s, 3.14 s] | 0.275 [0.274, 0.278] | 2 x 4 | 2.85 s [2.83 s, 2.89 s] | 0.934 | equal |
| phasefield_staggered_tri3 | solid_solve | 4 | 5 | 11.1 s [11.1 s, 11.2 s] | 3.12 s [3.07 s, 3.14 s] | 0.281 [0.276, 0.282] | 4 x 2 | 2.96 s [2.92 s, 2.98 s] | 0.970 | equal |
| phasefield_staggered_tri3 | solid_solve | 8 | 5 | 10.8 s [10.7 s, 10.9 s] | 3.05 s [3.04 s, 3.09 s] | 0.283 [0.282, 0.284] |  |  |  | equal |

Table "suite_python.py (Python suite)" (selected phases; `missing`: the
case did not exist in the port before):

| case | phase | calls | n | before | head | head / before | digests |
|---|---|---|---|---|---|---|---|
| dirichlet_group_tri3 | python_functor | 20 | 5 | missing | 471 ms [470 ms, 502 ms] | missing | equal |
| dirichlet_group_tri3 | python_group_functor | 200 | 5 | missing | 201 ms [200 ms, 204 ms] | missing | equal |
| dirichlet_tri3 | builtin_functor | 200 | 5 | 58.5 ms [58.3 ms, 58.9 ms] | 56.4 ms [56.4 ms, 57.0 ms] | 0.964 [0.958, 0.975] | equal |
| dirichlet_tri3 | python_functor | 20 | 5 | 579 ms [571 ms, 592 ms] | 470 ms [467 ms, 489 ms] | 0.819 [0.794, 0.841] | equal |
| explicit_tri3 | solve_step | 400 | 5 | 1.85 s [1.84 s, 1.85 s] | 1.16 s [1.15 s, 1.16 s] | 0.626 [0.625, 0.628] | equal |
| explicit_tri3 | solve_step_tiny | 300000 | 5 | 2.94 s [2.85 s, 3.09 s] | 1.84 s [1.83 s, 1.85 s] | 0.627 [0.598, 0.642] | equal |
| fields_tri3 | get_stress | 20000 | 5 | 9.18 ms [9.13 ms, 9.54 ms] | 9.63 ms [9.55 ms, 9.86 ms] | 1.04 [1.01, 1.08] | equal |
| import | import_akantu | 10 | 5 | 1.67 s [1.66 s, 1.67 s] | 1.67 s [1.67 s, 1.67 s] | 1.00 [0.997, 1.01] | equal |
| material_tri3 | solve_step_builtin_law | 400 | 5 | 1.84 s [1.84 s, 1.85 s] | 1.16 s [1.15 s, 1.16 s] | 0.626 [0.624, 0.628] | equal |
| material_tri3 | solve_step_python_law | 400 | 5 | 4.03 s [4.02 s, 4.05 s] | 3.62 s [3.60 s, 3.68 s] | 0.900 [0.892, 0.909] | equal |
| neumann_tri3 | builtin_functor | 2000 | 5 | 870 ms [867 ms, 874 ms] | 894 ms [893 ms, 897 ms] | 1.03 [1.02, 1.03] | equal |
| neumann_tri3 | python_functor | 200 | 5 | 250 ms [249 ms, 252 ms] | 253 ms [252 ms, 254 ms] | 1.01 [1.00, 1.02] | equal |

- Serial `solve_step`, head / before: the explicit cases 0.0965–0.779, the
  explicit cohesive cases 0.548–0.712, the implicit and static cases
  0.225–0.965 (`static_linear_faer` 0.965); in `suite_models` 0.236–0.873.
- Threads (8 / 1, `solve_step`, `suite_solid`): the explicit cases
  0.222–0.376; the implicit and static cases 0.789–1.00 (`static_linear_faer`, whose
  factorisation does not use threads, 1.00).
- Fast / bitwise at one thread: 0.893–1.03 over the cases of the two tables.
- Slower than before (head / before with min > 1): `fields_tri3`
  `get_stress` 1.04 [1.01, 1.08], `neumann_tri3` `builtin_functor` 1.03
  [1.02, 1.03]; and, in `suite_setup` (section "Set-up costs"),
  `erase_purify_tet4` `erase_elements` and `cohesive_insert_tet4`
  `update_insertion_facets`.
- Every bitwise digest of every case equals the head's.

## Static solves and solvers

### The final campaign (part D)

AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17 UTC; data
`docs/benchmarks/perf-campaign/final/D/summary.md`. The head's SparseLU
factorisation uses threads: `prepare.sh` measured the `factorize` phase of
`suite_solid static_linear_eigen` at 4 threads at 0.772 of 1 thread (threshold
0.9), so the threaded variants were timed.

Table "The SparseLU translation against upstream's SparseLU" (the driver's
`static_solve` phase: assembly, analysis, factorisation, solve; 1 rank, 10
steps; s):

| h | DOFs | C++ SparseLU (strict) | port before | port | port / C++ | port / port before | container memory peak (cgroup memory.peak) C++ / before / port MiB |
|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.86 [2.84, 2.87] | 7.59 [7.57, 7.60] | 2.11 [2.11, 2.13] | 0.740 [0.737, 0.743] | 0.279 [0.277, 0.280] | 497 [496, 524] / 561 [560, 567] / 399 [399, 400] |
| 0.12 | port, 2 threads |  |  | 1.70 [1.68, 1.70] | 0.595 [0.587, 0.598] | 0.803 [0.797, 0.807] (against 1 thread) |  |
| 0.12 | port, 4 threads |  |  | 1.41 [1.40, 1.42] | 0.494 [0.489, 0.495] | 0.666 [0.663, 0.669] (against 1 thread) |  |
| 0.12 | port, 8 threads |  |  | 1.27 [1.27, 1.29] | 0.447 [0.443, 0.452] | 0.603 [0.598, 0.613] (against 1 thread) |  |
| 0.06 | 97,551 | 241 [240, 243] | 778 [777, 779] | 219 [218, 221] | 0.909 [0.906, 0.918] | 0.281 [0.281, 0.283] | 7,608 [7,608, 7,608] / 6,918 [6,918, 6,918] / 5,326 [5,326, 5,326] |
| 0.06 | port, 2 threads |  |  | 136 [135, 139] | 0.566 [0.558, 0.577] | 0.622 [0.614, 0.634] (against 1 thread) |  |
| 0.06 | port, 4 threads |  |  | 86.2 [86.0, 86.3] | 0.358 [0.355, 0.359] | 0.393 [0.391, 0.396] (against 1 thread) |  |
| 0.06 | port, 8 threads |  |  | 62.7 [62.6, 63.0] | 0.260 [0.259, 0.261] | 0.286 [0.284, 0.288] (against 1 thread) |  |

Table "MUMPS in the cohesive benchmark" (static solve phase, s; the same
MUMPS 5.7.3 on the same matrix, PORD):

| h | ranks | C++ | port before | port | port / C++ | port / port before |
|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.622 [0.617, 0.628] | 0.792 [0.791, 0.813] | 0.520 [0.517, 0.531] | 0.832 [0.823, 0.861] | 0.658 [0.636, 0.670] |
| 0.12 | 2 | 0.408 [0.406, 0.411] | 0.492 [0.486, 0.497] | 0.376 [0.368, 0.379] | 0.915 [0.898, 0.933] | 0.763 [0.740, 0.769] |
| 0.12 | 4 | 0.227 [0.226, 0.232] | 0.269 [0.266, 0.271] | 0.216 [0.215, 0.218] | 0.949 [0.934, 0.965] | 0.804 [0.799, 0.813] |
| 0.06 | 1 | 12.9 [12.8, 13.0] | 14.0 [14.0, 14.1] | 10.2 [10.1, 10.3] | 0.789 [0.785, 0.798] | 0.727 [0.725, 0.730] |
| 0.06 | 2 | 8.60 [8.57, 8.62] | 9.18 [9.14, 9.21] | 7.39 [7.36, 7.41] | 0.859 [0.858, 0.863] | 0.805 [0.805, 0.806] |
| 0.06 | 4 | 4.59 [4.52, 5.25] | 4.88 [4.81, 5.30] | 4.07 [4.03, 4.53] | 0.889 [0.776, 1.00] | 0.835 [0.826, 0.930] |

Table "suite_solid, the sparse solvers' phases" (selected phases):

| case | phase | calls | n | before | head | head / before | head 2 threads / 1 | 4 / 1 | 8 / 1 | fast / bitwise (1) | fast / bitwise (8) | digests |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_eigen | analysis | 1 | 5 | 42.6 ms [42.1 ms, 43.1 ms] | 47.6 ms [47.1 ms, 47.7 ms] | 1.12 [1.09, 1.13] | 0.951 [0.945, 0.961] | 0.967 [0.964, 0.991] | 0.980 [0.972, 1.00] | — | — | equal |
| static_linear_eigen | back_substitution | 20 | 5 | 210 ms [207 ms, 213 ms] | 134 ms [134 ms, 135 ms] | 0.642 [0.626, 0.648] | 0.990 [0.984, 1.00] | 0.995 [0.993, 1.00] | 1.00 [0.995, 1.01] | — | — | equal |
| static_linear_eigen | factorize | 2 | 5 | 5.53 s [5.51 s, 5.55 s] | 978 ms [970 ms, 988 ms] | 0.176 [0.175, 0.179] | 0.855 [0.842, 0.861] | 0.774 [0.771, 0.783] | 0.734 [0.723, 0.737] | — | — | equal |
| static_linear_eigen | solve_step | 1 | 5 | 3.04 s [3.03 s, 3.07 s] | 684 ms [680 ms, 689 ms] | 0.225 [0.222, 0.227] | 0.889 [0.868, 0.898] | 0.828 [0.817, 0.841] | 0.791 [0.783, 0.794] | — | — | equal |
| static_linear_eigen | stiffness_assembly | 3 | 5 | 189 ms [181 ms, 192 ms] | 29.8 ms [29.6 ms, 29.8 ms] | 0.158 [0.155, 0.164] | 0.596 [0.571, 0.602] | 0.337 [0.330, 0.342] | 0.204 [0.202, 0.205] | — | — | equal |
| static_linear_faer | analysis | 1 | 5 | 49.7 ms [49.4 ms, 50.2 ms] | 55.2 ms [54.9 ms, 55.6 ms] | 1.11 [1.10, 1.12] | 0.996 [0.979, 0.999] | 0.993 [0.983, 1.00] | 0.995 [0.986, 1.00] | — | — | equal |
| static_linear_faer | back_substitution | 20 | 5 | 205 ms [204 ms, 206 ms] | 196 ms [195 ms, 197 ms] | 0.959 [0.952, 0.963] | 1.02 [1.01, 1.02] | 1.01 [1.00, 1.02] | 1.01 [1.00, 1.02] | — | — | equal |
| static_linear_faer | factorize | 2 | 5 | 9.42 s [9.36 s, 9.47 s] | 9.38 s [9.36 s, 9.44 s] | 0.996 [0.988, 1.01] | 1.00 [0.992, 1.02] | 1.00 [0.998, 1.01] | 1.01 [0.994, 1.02] | — | — | equal |
| static_linear_faer | solve_step | 1 | 5 | 5.01 s [4.94 s, 5.04 s] | 4.81 s [4.80 s, 4.89 s] | 0.960 [0.955, 0.990] | 1.01 [0.989, 1.02] | 1.00 [0.984, 1.03] | 1.01 [0.983, 1.03] | — | — | equal |
| static_linear_faer | stiffness_assembly | 3 | 5 | 250 ms [248 ms, 253 ms] | 30.1 ms [30.0 ms, 30.2 ms] | 0.120 [0.119, 0.121] | 0.588 [0.583, 0.591] | 0.331 [0.329, 0.334] | 0.200 [0.199, 0.211] | — | — | equal |

Table "suite_solid static_linear_mumps at 1, 2, 4 ranks" (selected phases;
"not compared (does not repeat)": the distributed MUMPS solve, see
"Method"):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| static_linear_mumps | distribute | 2 | 5 | 78.4 ms [78.2 ms, 79.1 ms] | 31.3 ms [31.3 ms, 31.3 ms] | 0.399 [0.396, 0.400] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | distribute | 4 | 5 | 92.0 ms [91.8 ms, 94.2 ms] | 44.5 ms [44.5 ms, 44.5 ms] | 0.484 [0.473, 0.485] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 1 | 5 | 154 ms [152 ms, 156 ms] | 66.8 ms [66.6 ms, 68.3 ms] | 0.434 [0.428, 0.447] |  |  |  | equal |
| static_linear_mumps | matrix | 2 | 5 | 76.1 ms [75.1 ms, 77.5 ms] | 32.7 ms [32.5 ms, 33.4 ms] | 0.428 [0.419, 0.445] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | matrix | 4 | 5 | 37.4 ms [37.2 ms, 37.6 ms] | 17.6 ms [16.9 ms, 18.0 ms] | 0.474 [0.451, 0.480] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 1 | 5 | 330 ms [327 ms, 337 ms] | 209 ms [209 ms, 212 ms] | 0.634 [0.621, 0.649] |  |  |  | equal |
| static_linear_mumps | solve_step | 2 | 5 | 225 ms [224 ms, 227 ms] | 169 ms [168 ms, 170 ms] | 0.750 [0.741, 0.756] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | solve_step | 4 | 5 | 132 ms [132 ms, 132 ms] | 107 ms [106 ms, 107 ms] | 0.807 [0.801, 0.812] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 1 | 5 | 242 ms [239 ms, 250 ms] | 29.7 ms [29.6 ms, 30.8 ms] | 0.124 [0.119, 0.127] |  |  |  | equal |
| static_linear_mumps | stiffness_assembly | 2 | 5 | 122 ms [119 ms, 125 ms] | 15.3 ms [15.1 ms, 15.4 ms] | 0.127 [0.122, 0.127] |  |  |  | not compared (does not repeat) |
| static_linear_mumps | stiffness_assembly | 4 | 5 | 47.8 ms [47.3 ms, 48.5 ms] | 8.30 ms [8.16 ms, 8.51 ms] | 0.173 [0.171, 0.178] |  |  |  | not compared (does not repeat) |

- The SparseLU translation takes 0.740 (h = 0.12) and 0.909 (h = 0.06) of
  upstream's SparseLU at one thread, 0.279 and 0.281 of the port before;
  at 8 threads 0.447 and 0.260 of upstream's. Its container memory peak is
  399 against 497 MiB (h = 0.12) and 5,326 against 7,608 MiB (h = 0.06).
- MUMPS: port / C++ 0.832 / 0.915 / 0.949 at 1 / 2 / 4 ranks on h = 0.12,
  0.789 / 0.859 / 0.889 on h = 0.06 (the last [0.776, 1.00]).
- The symbolic `analysis` of both sparse backends is slower than before:
  1.12 [1.09, 1.13] (`static_linear_eigen`) and 1.11 [1.10, 1.12]
  (`static_linear_faer`). The `faer` factorisation is unchanged (0.996) and
  does not use threads.

### The nested-dissection ordering (D-125)

The `eigen` solver (the SparseLU translation) takes an opt-in column
ordering (D-125): `nested_dissection`, or `auto`, which takes nested
dissection only when its predicted work is at most 0.8 of COLAMD's and its
predicted size not larger, and otherwise keeps COLAMD. COLAMD stays the default and is the
only bitwise ordering; under another ordering the factors and the last
digits of the solutions differ from the default (rule
`fast_mode/linalg/sparse_lu_ordering`). Under the fast precision without an
ordering set, the solver uses `auto`.

Second pass of part D: AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17
17:06–20:20 UTC (machine 3 again, another session), port head `4d974854d`,
port before `b6f56c2f0`; it repeats every variant of the first pass and adds
`head-nd` (`AKANTU_SPARSE_ORDERING=nested_dissection`) and `head-auto`
(`AKANTU_PRECISION=fast`, no ordering set); 65 of 65 groups complete and
checked, 170 of 170 bindings as planned; data
`docs/benchmarks/perf-campaign/final/D-nd/summary.md`. Its numbers are not
mixed with the first pass's tables above.

Table "The SparseLU translation against upstream's SparseLU" (second pass;
the driver's `static_solve` phase: assembly, analysis, factorisation, solve;
1 rank, 10 steps; s):

| h | DOFs | C++ SparseLU (strict) | port before | port | port / C++ | port / port before | container memory peak (cgroup memory.peak) C++ / before / port MiB |
|---|---|---|---|---|---|---|---|
| 0.12 | 14,472 | 2.85 [2.85, 2.87] | 7.58 [7.58, 7.61] | 2.12 [2.09, 2.12] | 0.740 [0.729, 0.744] | 0.278 [0.276, 0.279] | 497 [497, 497] / 561 [560, 561] / 399 [399, 399] |
| 0.12 | port, 2 threads |  |  | 1.68 [1.66, 1.71] | 0.589 [0.585, 0.596] | 0.797 [0.786, 0.817] (against 1 thread) |  |
| 0.12 | port, 4 threads |  |  | 1.40 [1.39, 1.41] | 0.490 [0.487, 0.494] | 0.663 [0.659, 0.669] (against 1 thread) |  |
| 0.12 | port, 8 threads |  |  | 1.27 [1.27, 1.29] | 0.446 [0.444, 0.452] | 0.603 [0.602, 0.609] (against 1 thread) |  |
| 0.12 | port, nested dissection |  |  | 0.873 [0.857, 0.879] | 0.305 [0.301, 0.307] | 0.412 [0.407, 0.421] (against COLAMD) |  |
| 0.12 | port, fast precision, auto ordering |  |  | 0.880 [0.876, 0.887] | 0.307 [0.306, 0.312] | 0.418 [0.414, 0.420] (against COLAMD) |  |
| 0.06 | 97,551 | 241 [241, 242] | 781 [780, 783] | 219 [218, 220] | 0.910 [0.904, 0.912] | 0.281 [0.279, 0.281] | 7,608 [7,608, 7,608] / 6,918 [6,918, 6,918] / 5,325 [5,325, 5,325] |
| 0.06 | port, 2 threads |  |  | 138 [137, 140] | 0.572 [0.570, 0.579] | 0.630 [0.626, 0.639] (against 1 thread) |  |
| 0.06 | port, 4 threads |  |  | 86.5 [86.4, 86.6] | 0.359 [0.358, 0.359] | 0.394 [0.394, 0.397] (against 1 thread) |  |
| 0.06 | port, 8 threads |  |  | 62.4 [62.3, 62.9] | 0.259 [0.258, 0.261] | 0.285 [0.284, 0.286] (against 1 thread) |  |
| 0.06 | port, nested dissection |  |  | 40.9 [40.9, 41.0] | 0.170 [0.169, 0.170] | 0.186 [0.186, 0.188] (against COLAMD) |  |
| 0.06 | port, fast precision, auto ordering |  |  | 34.5 [34.4, 34.5] | 0.143 [0.143, 0.143] | 0.157 [0.157, 0.158] (against COLAMD) |  |

- At one thread, nested dissection takes 0.873 s (h = 0.12) and 40.9 s
  (h = 0.06): 0.305 and 0.170 of upstream's SparseLU, 0.412 and 0.186 of
  the port with COLAMD. The ordering was not measured with threads in this
  pass.
- `auto` under the fast precision chose nested dissection on both meshes
  (`rust_static_solver_stats.ordering = 1` in every run, `runs.jsonl`; its
  factor L held 4,703,922 entries against COLAMD's 8,222,229 at h = 0.12,
  77,750,118 against 190,221,087 at h = 0.06, `nnz_l`) and took 0.880 s and
  34.5 s: 0.307 and 0.143 of upstream's SparseLU, 0.418 and 0.157 of
  COLAMD.
- The pass's repetition of the first pass's variants agrees with it: the
  port with COLAMD takes 2.12 [2.09, 2.12] s against 2.11 [2.11, 2.13] s in
  the first pass (h = 0.12) and 219 [218, 220] s against 219 [218, 221] s
  (h = 0.06); upstream's SparseLU 2.85 against 2.86 s and 241 against
  241 s. The default path gives the same results at both heads: the
  per-step energies and insertions of every 1-rank and SparseLU run are
  byte-identical to the first pass's, and so are the suite digests of
  `static_linear_eigen`, `static_linear_faer` and `static_linear_mumps` at
  one rank (only the distributed MUMPS runs, which do not repeat, differ).
- Where the ordering loses: on the development machine the DKT plate case
  of `suite_solid`, whose factorisation pivots off the diagonal, took 3.09
  times COLAMD's solve time with nested dissection
  (`docs/benchmarks/perf-campaign/sparselu-nd/README.md`, a development
  record, not a result of this campaign).

### Before the final campaign (11–13 November)

#### The SparseLU translation against upstream's SparseLU

The default direct solver of a build without MUMPS is the port's translation
of Eigen's SparseLU. Like for like: the C++ oracle with
`sparse_solver_type = eigen` (upstream's own SparseLU, strict install, Eigen
cache sizes pinned per D-050) against the port's `eigen` backend, the static
solve of the cohesive benchmark, one run at a time on one boat sandbox (AMD
EPYC 9455, 2026-11-11).

| h | DOFs | C++ MUMPS | C++ SparseLU (strict) | port, SparseLU translation | port / C++ SparseLU |
|---|---|---|---|---|---|
| 0.12 (frozen mesh) | 14,472 | 0.54 s | 3.77 [2.55, 5.16] s, 535 MiB (3 runs) | 5.96 [5.26, 6.04] s, 564 MiB (3 runs) | about 1.6 |
| 0.06 (x86 mesh) | 97,551 | 17.6 s (another sandbox, Ryzen 9 9950X) | 314.6 s, 7.5 GiB (1 run) | 625.2 s, 6.7 GiB (1 run) | about 2.0 |

These numbers are recorded only in this document: the run's output was not
kept, and no data file under `docs/benchmarks/` holds them.

Eigen's SparseLU takes 5.2 min and 7.5 GiB at h = 0.06 in upstream itself,
18× MUMPS's time; the translation adds 1.6× to 2.0×. Meshes of h ≤ 0.06 and
the JOSS mesh need MUMPS even serially. The translation's dense kernel
(`sparselu_gemm`, `akantu-linalg/src/eigen_sparse_lu/dense.rs`) took 18 % of
the samples of the cohesive h = 0.12 run. The `faer` backend's simplicial LU
did not finish the h = 0.06 solve within 2 min (Ryzen 9 9950X sandbox).

The driver's switch `rust_static_solve = false` skips the static preload
(not the upstream problem) for timing the explicit loop. Such a run is
marked in every output (Chrono phase `static_solve_skipped`, a first line in
`energies.csv` and `insertions.csv`, a summary field) and the analysis
scripts keep it apart. Without the preload the cohesive problem inserts
early (640 cohesive elements within 500 steps at h = 0.12), so its costs are
not pre-insertion costs.
#### MUMPS in the cohesive benchmark

The port passes its rank-partial matrix to MUMPS as distributed assembled
input (`ICNTL(18) = 3`), with the right-hand side centralised on rank 0,
and records MUMPS's own statistics. At one rank its static solution is
bitwise the C++ one.

At h = 0.12 (1,500 steps with insertion; boat `large`, AMD EPYC 9455,
2026-11-12; 3 interleaved repetitions, medians;
`docs/benchmarks/midsize-2026-11-12/item1-h012/`) the static solve took
0.441 s (C++) and 0.540 s (port) at one rank with MUMPS, 2.21 s and 4.96 s
with each code's SparseLU; port / C++ 1.23 / 1.15 / 1.13 at 1 / 2 / 4 ranks
with MUMPS (PORD ordering, no workspace retry).

Across the boat series the port's static-solve phase (the same MUMPS on the
same matrix, plus the port's assembly) takes 1.02–1.31× upstream's at 13 of
14 points (paired medians; at h = 0.12 on 1 rank all five pairs in
[1.06, 1.25]) and 0.65× at the fourteenth (h = 0.0476 on 8 ranks), where
upstream's own solve varied from 9.3 to 21.6 s and the port's from 9.7 to
26.9 s. On the mid-size meshes it is 0.76–1.16× (h = 0.04) and
0.91–1.49× (h = 0.06; the 1.49 at 8 ranks varied 1.16–1.85 over the pairs).
At full size no gap is established (next section).

## The cohesive scalability benchmark

Upstream's JOSS scalability test: an extrinsic cohesive cube under shear
(`examples/c++/scalability_test/cohesive_extrinsic.cc`), the input
`material-cohesive.dat`, a static preload solved with MUMPS, then explicit
steps with the shear raised by 1 MPa every 100 steps; insertion starts
around step 700. Meshes come from `benchmarks/cohesive-perf/inputs/cube.geo`
with the oracle image's Gmsh 4.13.1 (`gen-meshes.sh`; statistics in
`benchmarks/cohesive-perf/meshes/`). The same Gmsh generates different
meshes on aarch64 and x86_64 (h = 0.04: 558,455 against 558,875
tetrahedra), so only frozen mesh files give the same input on both
platforms; all comparisons below use the x86_64 meshes.

| h | tetrahedra | DOFs | used in |
|---|---:|---:|---|
| 0.12 (frozen, `fixtures/perf_cohesive/meshes/`) | 23,156 | 14,472 | every series |
| 0.0925 | 47,543 | | scaling series |
| 0.0756 | 89,962 | | scaling series |
| 0.06 | 177,328 | 97,551 | rehearsal, scaling series |
| 0.0476 | 352,996 | | scaling series |
| 0.04 | 558,875 | 293,745 | rehearsal, multi-node |
| 0.032 | 1,118,239 | | multi-node |
| 0.025 | 2,256,352 | | multi-node |
| JOSS `cube.msh` (SHA-256 `5ad845c7…`) | 4,392,183 | 2,203,782 | full size, multi-node |

The JOSS mesh corresponds to h ≈ 0.02 (100 segments per cube edge); the
image's Gmsh does not reproduce it (h = 0.02 gives 4,412,464 tetrahedra), so
the JOSS file itself is used.

Why the C++ runs use the JOSS emulation (C++ reference alone, h = 0.12,
5,000 steps, 1 rank, one run per mode; boat `large`, Ryzen 7 9700X,
2026-11-11; `docs/benchmarks/cpp-baseline-x86-2026-11-11/x86-full-schedule/`):
the event-triggered `fillNodesToElements()` rebuild took 757 s of the 885 s
of `check_cohesive_stress` (86 %, 14,195 calls), and skipping it cut the
check 5.3× (0.177 to 0.033 s per step) with bitwise identical physics (11,732
cohesive elements, E_d = 344.1904 J in both modes). The same baseline
campaign (`x86-baseline/`, h = 0.12 / 0.06 / 0.04 before insertion) shows
upstream's check growing faster than the mesh: check / step at 1 rank
2.2–2.5, 3.6–3.8 and 4.4–4.5.

### Small and mid sizes at 1 to 8 ranks (final campaign, part C)

AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17 UTC, two machines of the same
type (series `reh6` on one, the others on the other); data
`docs/benchmarks/perf-campaign/final/C/summary.md` and
`docs/benchmarks/perf-campaign/final/C-reh6/summary.md`. C++: the strict
install with the JOSS emulation, MUMPS 5.7.3; the port's MPI build
(`mpi,scotch,mumps`); the same mesh and Scotch partition for both; one rank
per core, recorded and checked per run; one container per run; the
variants of a point adjacent and rotated per repetition, the point order
reversed every other repetition; five repetitions (one in series `eig`).
Series:

- `ws`: 500 steps before the first insertion, the ten points of the weak
  and strong studies (the series `ws` of the earlier scaling study);
- `ins`: the insertion regime on h = 0.12, 1,500 steps;
- `full12`: the full schedule on h = 0.12, 5,000 steps;
- `reh6`: the full schedule on h = 0.06, C++ and the head only (port
  before → port is measured by the other series);
- `reh4`: the static solve and 300 pre-insertion steps on h = 0.04;
- `eig`: the port's SparseLU static solve at 1 to 8 ranks, before and head,
  one repetition: the bitwise check of the distributed runs.

Each series has a table "Points" (rank 0's Chrono phases; core-µs per
element and step = (`solve_step` + check) × cores / tetrahedra / steps; loop
efficiency = core-µs at 1 rank / core-µs at P, same code, "missing" where
the series has no 1-rank point on that mesh; peaks of the memory sampler)
and paired ratio tables. "port P x T" is the head with P ranks × T threads.

#### Series `ws`: 500 steps before the first insertion

Table "Points":

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.12 | 1 | C++ | 5 | 18.1 [17.9, 22.2] | 0.966 [0.964, 0.972] | 0.613 [0.611, 1.14] | 8.22 [8.14, 8.38] | 20.5 [20.3, 21.8] | 1.24 [1.24, 1.30] | 100% | 410 [410, 410] | 410 [410, 410] |
| 0.12 | 1 | port before | 5 | 11.5 [11.5, 15.0] | 0.378 [0.375, 0.379] | 0.782 [0.773, 1.10] | 3.63 [3.55, 3.65] | 13.5 [13.4, 13.7] | 0.738 [0.737, 0.747] | 100% | 366 [366, 366] | 366 [366, 366] |
| 0.12 | 1 | port | 5 | 4.84 [4.81, 4.85] | 0.164 [0.163, 0.167] | 0.513 [0.513, 0.522] | 2.52 [2.50, 2.55] | 4.44 [4.36, 4.47] | 0.301 [0.298, 0.301] | 100% | 307 [307, 308] | 307 [307, 308] |
| 0.0925 | 1 | C++ | 5 | 40.4 [40.4, 41.1] | 2.39 [2.38, 2.39] | 1.69 [1.68, 1.69] | 16.8 [16.7, 16.9] | 47.8 [47.6, 49.0] | 1.36 [1.35, 1.39] | 100% | 816 [816, 816] | 816 [816, 816] |
| 0.0925 | 1 | port before | 5 | 24.3 [24.2, 24.3] | 0.794 [0.787, 0.803] | 2.07 [2.06, 2.09] | 7.32 [7.30, 7.43] | 28.4 [28.4, 28.5] | 0.753 [0.751, 0.754] | 100% | 716 [716, 716] | 716 [716, 716] |
| 0.0925 | 1 | port | 5 | 11.2 [11.2, 11.3] | 0.340 [0.338, 0.340] | 1.39 [1.39, 1.40] | 5.32 [5.30, 5.36] | 11.3 [11.3, 11.4] | 0.349 [0.349, 0.351] | 100% | 617 [617, 618] | 617 [617, 618] |
| 0.0925 | 2 | C++ | 5 | 25.4 [25.3, 25.6] | 1.10 [1.10, 1.10] | 1.09 [1.09, 1.09] | 11.5 [11.5, 12.1] | 29.3 [29.0, 29.5] | 1.72 [1.72, 1.73] | 79% | 481 [481, 482] | 959 [959, 960] |
| 0.0925 | 2 | port before | 5 | 13.9 [13.9, 13.9] | 0.445 [0.444, 0.447] | 1.24 [1.23, 1.24] | 5.11 [5.09, 5.13] | 15.3 [15.3, 15.4] | 0.860 [0.858, 0.863] | 88% | 438 [437, 438] | 867 [866, 867] |
| 0.0925 | 2 | port | 5 | 6.98 [6.96, 7.32] | 0.197 [0.195, 0.198] | 0.950 [0.946, 0.953] | 3.60 [3.57, 3.74] | 6.52 [6.46, 7.04] | 0.426 [0.423, 0.453] | 82% | 376 [373, 376] | 744 [743, 746] |
| 0.0756 | 2 | C++ | 5 | 52.7 [52.4, 53.1] | 2.56 [2.56, 2.58] | 2.97 [2.94, 3.45] | 22.0 [21.2, 22.4] | 62.0 [61.6, 62.6] | 1.86 [1.84, 1.89] | missing | 895 [893, 895] | 1,766 [1,765, 1,767] |
| 0.0756 | 2 | port before | 5 | 27.2 [27.2, 27.4] | 0.848 [0.845, 0.848] | 3.33 [3.27, 3.41] | 9.48 [9.45, 9.51] | 29.0 [29.0, 29.2] | 0.856 [0.854, 0.858] | missing | 788 [782, 846] | 1,562 [1,553, 1,644] |
| 0.0756 | 2 | port | 5 | 13.8 [13.7, 13.9] | 0.369 [0.366, 0.372] | 2.53 [2.52, 2.55] | 6.44 [6.42, 6.46] | 12.7 [12.6, 12.9] | 0.425 [0.422, 0.429] | missing | 691 [691, 695] | 1,374 [1,372, 1,377] |
| 0.0756 | 4 | C++ | 5 | 30.9 [30.8, 30.9] | 1.12 [1.12, 1.13] | 1.87 [1.86, 1.90] | 14.5 [14.3, 14.5] | 34.7 [34.6, 34.8] | 2.19 [2.18, 2.19] | missing | 518 [515, 519] | 2,005 [2,003, 2,009] |
| 0.0756 | 4 | port before | 5 | 17.2 [17.1, 17.2] | 0.445 [0.444, 0.446] | 1.99 [1.97, 2.02] | 6.92 [6.87, 6.93] | 18.0 [18.0, 18.1] | 1.11 [1.11, 1.11] | missing | 466 [466, 467] | 1,833 [1,830, 1,836] |
| 0.0756 | 4 | port | 5 | 9.29 [9.20, 9.32] | 0.194 [0.194, 0.196] | 1.67 [1.66, 1.69] | 4.73 [4.70, 4.80] | 8.33 [8.19, 8.36] | 0.581 [0.573, 0.582] | missing | 413 [411, 413] | 1,609 [1,608, 1,611] |
| 0.06 | 1 | C++ | 5 | 195 [194, 198] | 14.0 [13.9, 14.0] | 12.9 [12.8, 13.0] | 62.4 [62.1, 68.3] | 240 [239, 241] | 1.71 [1.70, 1.74] | 100% | 3,178 [3,178, 3,178] | 3,178 [3,178, 3,178] |
| 0.06 | 1 | port before | 5 | 99.0 [98.8, 99.3] | 3.07 [3.07, 3.09] | 14.0 [14.0, 14.1] | 27.2 [27.1, 27.4] | 109 [108, 110] | 0.766 [0.765, 0.771] | 100% | 2,853 [2,853, 2,853] | 2,853 [2,853, 2,853] |
| 0.06 | 1 | port | 5 | 50.8 [50.6, 51.1] | 1.34 [1.34, 1.35] | 10.2 [10.2, 10.3] | 19.9 [19.8, 20.0] | 50.3 [50.1, 50.6] | 0.395 [0.394, 0.398] | 100% | 2,471 [2,471, 2,471] | 2,471 [2,471, 2,471] |
| 0.06 | 2 | C++ | 5 | 113 [112, 114] | 6.25 [5.71, 6.27] | 8.53 [8.50, 8.61] | 41.0 [40.4, 42.2] | 134 [134, 136] | 1.98 [1.96, 2.01] | 86% | 1,875 [1,874, 1,878] | 3,742 [3,740, 3,744] |
| 0.06 | 2 | port before | 5 | 56.3 [56.2, 56.7] | 1.69 [1.69, 1.69] | 9.09 [9.07, 9.19] | 18.3 [18.3, 18.5] | 57.5 [57.3, 57.8] | 0.855 [0.853, 0.861] | 90% | 1,698 [1,696, 1,701] | 3,387 [3,386, 3,392] |
| 0.06 | 2 | port | 5 | 30.1 [29.9, 30.2] | 0.739 [0.737, 0.741] | 7.35 [7.34, 7.39] | 12.5 [12.4, 12.5] | 26.7 [26.4, 26.8] | 0.442 [0.438, 0.443] | 89% | 1,515 [1,513, 1,519] | 3,028 [3,022, 3,037] |
| 0.06 | 2 | port 2 x 4 | 5 | 23.1 [23.1, 23.2] | 0.743 [0.735, 0.745] | 7.12 [7.09, 7.15] | 9.72 [9.66, 9.78] | 16.0 [15.9, 16.2] | 1.16 [1.16, 1.16] | 34% | 1,520 [1,518, 1,527] | 3,036 [3,033, 3,043] |
| 0.06 | 4 | C++ | 5 | 64.3 [63.8, 64.9] | 2.56 [2.53, 2.60] | 4.58 [4.56, 4.63] | 27.7 [27.6, 27.7] | 73.7 [72.9, 74.8] | 2.29 [2.27, 2.31] | 75% | 993 [989, 998] | 3,876 [3,865, 3,887] |
| 0.06 | 4 | port before | 5 | 34.7 [34.6, 34.8] | 0.883 [0.883, 0.892] | 4.87 [4.82, 4.91] | 13.4 [13.2, 13.4] | 35.4 [35.3, 35.5] | 1.10 [1.10, 1.10] | 70% | 907 [902, 908] | 3,514 [3,509, 3,517] |
| 0.06 | 4 | port | 5 | 19.0 [18.9, 19.1] | 0.382 [0.381, 0.390] | 4.03 [4.02, 4.07] | 9.13 [9.09, 9.27] | 16.8 [16.7, 16.9] | 0.586 [0.583, 0.589] | 68% | 811 [807, 813] | 3,128 [3,124, 3,133] |
| 0.06 | 4 | port 4 x 2 | 5 | 18.0 [17.9, 18.3] | 0.383 [0.380, 0.391] | 3.93 [3.89, 4.36] | 9.08 [9.06, 9.15] | 15.0 [15.0, 15.4] | 1.09 [1.08, 1.11] | 36% | 819 [816, 831] | 3,145 [3,141, 3,166] |
| 0.06 | 8 | C++ | 5 | 43.6 [43.3, 44.2] | 1.18 [1.17, 1.20] | 3.05 [3.01, 3.19] | 20.0 [19.6, 20.5] | 48.3 [47.9, 49.2] | 3.09 [3.05, 3.12] | 55% | 640 [613, 683] | 4,465 [4,353, 4,552] |
| 0.06 | 8 | port before | 5 | 27.4 [27.3, 27.5] | 0.492 [0.486, 0.494] | 3.31 [3.23, 3.42] | 14.0 [13.9, 14.0] | 26.6 [26.5, 26.7] | 1.83 [1.83, 1.83] | 42% | 598 [564, 604] | 4,016 [3,972, 4,062] |
| 0.06 | 8 | port | 5 | 16.6 [16.5, 16.7] | 0.220 [0.214, 0.230] | 2.77 [2.76, 2.98] | 9.74 [9.65, 9.88] | 14.8 [14.5, 15.0] | 1.11 [1.09, 1.12] | 36% | 527 [511, 581] | 3,679 [3,535, 3,717] |
| 0.0476 | 8 | C++ | 5 | 91.0 [90.5, 91.8] | 2.78 [2.68, 2.85] | 9.02 [8.52, 9.41] | 38.2 [37.8, 41.2] | 99.3 [98.3, 101] | 3.11 [3.09, 3.21] | missing | 1,317 [1,289, 1,552] | 8,924 [8,719, 9,287] |
| 0.0476 | 8 | port before | 5 | 58.8 [57.3, 63.0] | 0.971 [0.963, 0.980] | 10.9 [9.47, 15.0] | 28.0 [27.7, 28.0] | 52.2 [52.1, 52.3] | 1.82 [1.81, 1.82] | missing | 1,351 [1,193, 1,611] | 8,264 [8,153, 8,818] |
| 0.0476 | 8 | port | 5 | 37.4 [36.9, 41.6] | 0.452 [0.445, 0.459] | 9.69 [9.20, 13.8] | 19.7 [19.6, 19.7] | 29.5 [29.4, 29.6] | 1.11 [1.11, 1.12] | missing | 1,341 [1,088, 1,543] | 7,927 [7,004, 8,538] |

Table "Port / upstream, series ws":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.241 [0.232, 0.244] | 0.306 [0.304, 0.308] | 0.215 [0.204, 0.218] | 0.837 [0.452, 0.853] | 0.170 [0.169, 0.173] | 0.268 [0.217, 0.270] |
| 0.0925 | 1 | 0.258 [0.252, 0.259] | 0.317 [0.314, 0.320] | 0.236 [0.230, 0.239] | 0.824 [0.821, 0.834] | 0.142 [0.142, 0.143] | 0.278 [0.272, 0.279] |
| 0.0925 | 2 | 0.247 [0.244, 0.264] | 0.313 [0.297, 0.324] | 0.222 [0.221, 0.240] | 0.869 [0.866, 0.875] | 0.179 [0.177, 0.179] | 0.274 [0.272, 0.289] |
| 0.0756 | 2 | 0.229 [0.223, 0.231] | 0.294 [0.287, 0.302] | 0.204 [0.201, 0.208] | 0.851 [0.734, 0.867] | 0.144 [0.143, 0.144] | 0.262 [0.258, 0.263] |
| 0.0756 | 4 | 0.266 [0.263, 0.267] | 0.328 [0.327, 0.330] | 0.240 [0.235, 0.241] | 0.889 [0.881, 0.907] | 0.173 [0.172, 0.174] | 0.301 [0.298, 0.302] |
| 0.06 | 1 | 0.233 [0.228, 0.233] | 0.319 [0.292, 0.320] | 0.210 [0.208, 0.211] | 0.790 [0.789, 0.797] | 0.0964 [0.0962, 0.0967] | 0.261 [0.258, 0.262] |
| 0.06 | 2 | 0.222 [0.220, 0.226] | 0.303 [0.296, 0.309] | 0.197 [0.197, 0.200] | 0.861 [0.856, 0.869] | 0.118 [0.118, 0.130] | 0.266 [0.264, 0.270] |
| 0.06 | 4 | 0.257 [0.254, 0.257] | 0.330 [0.329, 0.334] | 0.228 [0.226, 0.230] | 0.877 [0.869, 0.892] | 0.150 [0.149, 0.152] | 0.296 [0.292, 0.297] |
| 0.06 | 8 | 0.357 [0.354, 0.363] | 0.487 [0.474, 0.496] | 0.304 [0.301, 0.309] | 0.922 [0.864, 0.957] | 0.186 [0.183, 0.192] | 0.379 [0.375, 0.385] |
| 0.0476 | 8 | 0.359 [0.346, 0.360] | 0.515 [0.475, 0.522] | 0.297 [0.294, 0.300] | 1.11 [0.977, 1.60] | 0.163 [0.158, 0.171] | 0.414 [0.404, 0.452] |

Table "Port / port before, series ws":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.405 [0.400, 0.409] | 0.695 [0.685, 0.716] | 0.330 [0.321, 0.331] | 0.656 [0.467, 0.675] | 0.434 [0.433, 0.443] | 0.420 [0.321, 0.423] |
| 0.0925 | 1 | 0.464 [0.463, 0.468] | 0.726 [0.714, 0.730] | 0.398 [0.396, 0.401] | 0.674 [0.666, 0.678] | 0.429 [0.423, 0.431] | 0.461 [0.461, 0.464] |
| 0.0925 | 2 | 0.496 [0.492, 0.527] | 0.707 [0.696, 0.731] | 0.426 [0.421, 0.459] | 0.767 [0.762, 0.769] | 0.444 [0.437, 0.444] | 0.504 [0.499, 0.527] |
| 0.0756 | 2 | 0.495 [0.493, 0.502] | 0.678 [0.677, 0.684] | 0.434 [0.433, 0.444] | 0.758 [0.742, 0.780] | 0.435 [0.432, 0.440] | 0.504 [0.503, 0.511] |
| 0.0756 | 4 | 0.524 [0.518, 0.525] | 0.685 [0.683, 0.692] | 0.463 [0.454, 0.464] | 0.838 [0.828, 0.860] | 0.436 [0.435, 0.440] | 0.541 [0.535, 0.543] |
| 0.06 | 1 | 0.515 [0.513, 0.520] | 0.729 [0.726, 0.736] | 0.462 [0.458, 0.467] | 0.726 [0.724, 0.728] | 0.437 [0.436, 0.439] | 0.513 [0.511, 0.516] |
| 0.06 | 2 | 0.515 [0.512, 0.518] | 0.679 [0.678, 0.685] | 0.462 [0.459, 0.467] | 0.808 [0.799, 0.815] | 0.437 [0.437, 0.439] | 0.534 [0.532, 0.536] |
| 0.06 | 4 | 0.533 [0.531, 0.535] | 0.688 [0.682, 0.691] | 0.474 [0.473, 0.477] | 0.832 [0.819, 0.838] | 0.433 [0.427, 0.441] | 0.547 [0.547, 0.549] |
| 0.06 | 8 | 0.605 [0.599, 0.614] | 0.700 [0.690, 0.708] | 0.555 [0.545, 0.564] | 0.855 [0.809, 0.878] | 0.445 [0.435, 0.467] | 0.605 [0.603, 0.613] |
| 0.0476 | 8 | 0.612 [0.612, 0.616] | 0.703 [0.699, 0.710] | 0.566 [0.562, 0.566] | 0.891 [0.671, 1.46] | 0.465 [0.458, 0.475] | 0.636 [0.601, 0.725] |

Table "Port before / upstream, series ws":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.596 [0.568, 0.605] | 0.442 [0.424, 0.449] | 0.662 [0.616, 0.670] | 1.27 [0.968, 1.28] | 0.391 [0.389, 0.392] | 0.640 [0.615, 0.677] |
| 0.0925 | 1 | 0.554 [0.543, 0.557] | 0.438 [0.432, 0.443] | 0.596 [0.579, 0.597] | 1.23 [1.22, 1.23] | 0.332 [0.330, 0.337] | 0.600 [0.591, 0.603] |
| 0.0925 | 2 | 0.500 [0.496, 0.501] | 0.443 [0.421, 0.445] | 0.524 [0.520, 0.527] | 1.13 [1.13, 1.14] | 0.404 [0.403, 0.405] | 0.547 [0.541, 0.548] |
| 0.0756 | 2 | 0.460 [0.453, 0.463] | 0.432 [0.423, 0.446] | 0.470 [0.464, 0.473] | 1.11 [0.989, 1.14] | 0.331 [0.327, 0.331] | 0.516 [0.513, 0.520] |
| 0.0756 | 4 | 0.507 [0.506, 0.509] | 0.478 [0.477, 0.480] | 0.519 [0.517, 0.522] | 1.06 [1.03, 1.08] | 0.396 [0.395, 0.397] | 0.556 [0.554, 0.559] |
| 0.06 | 1 | 0.449 [0.439, 0.454] | 0.438 [0.397, 0.439] | 0.451 [0.449, 0.459] | 1.09 [1.09, 1.10] | 0.220 [0.220, 0.221] | 0.508 [0.499, 0.512] |
| 0.06 | 2 | 0.431 [0.428, 0.436] | 0.447 [0.435, 0.452] | 0.426 [0.426, 0.431] | 1.07 [1.06, 1.08] | 0.270 [0.270, 0.296] | 0.498 [0.495, 0.503] |
| 0.06 | 4 | 0.482 [0.475, 0.484] | 0.483 [0.478, 0.484] | 0.482 [0.473, 0.485] | 1.07 [1.04, 1.07] | 0.345 [0.340, 0.352] | 0.541 [0.535, 0.542] |
| 0.06 | 8 | 0.592 [0.586, 0.600] | 0.696 [0.683, 0.706] | 0.550 [0.541, 0.557] | 1.08 [1.04, 1.12] | 0.416 [0.408, 0.422] | 0.627 [0.621, 0.634] |
| 0.0476 | 8 | 0.585 [0.566, 0.588] | 0.733 [0.679, 0.740] | 0.527 [0.519, 0.530] | 1.28 [1.06, 1.66] | 0.353 [0.341, 0.360] | 0.651 [0.624, 0.692] |

Table "Ranks × threads", series ws:

| h | configuration | loop s | against 8 ranks | against the same ranks, 1 thread |
|---|---|---|---|---|
| 0.06 | 2 ranks x 4 threads | 12.9 [12.8, 12.9] | 1.05 [1.04, 1.06] | 0.658 [0.654, 0.662] |
| 0.06 | 4 ranks x 2 threads | 12.1 [12.0, 12.3] | 0.993 [0.978, 1.000] | 0.933 [0.925, 0.941] |

#### Series `ins`: the insertion regime on h = 0.12, 1,500 steps

Table "Points":

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.12 | 1 | C++ | 5 | 50.0 [49.8, 50.4] | 0.966 [0.964, 0.969] | 0.615 [0.612, 0.616] | 8.37 [8.36, 8.44] | 20.7 [20.6, 21.0] | 1.26 [1.25, 1.27] | 100% | 410 [410, 410] | 410 [410, 410] |
| 0.12 | 1 | port before | 5 | 33.1 [32.9, 33.1] | 0.377 [0.376, 0.378] | 0.782 [0.780, 0.791] | 3.82 [3.75, 3.85] | 14.8 [14.7, 14.8] | 0.802 [0.798, 0.804] | 100% | 366 [366, 366] | 366 [366, 366] |
| 0.12 | 1 | port | 5 | 12.7 [12.7, 12.8] | 0.164 [0.163, 0.165] | 0.514 [0.513, 0.517] | 2.62 [2.61, 2.62] | 4.54 [4.52, 4.57] | 0.309 [0.308, 0.311] | 100% | 308 [307, 308] | 308 [307, 308] |
| 0.12 | 2 | C++ | 5 | 33.1 [33.0, 33.6] | 0.474 [0.473, 0.495] | 0.405 [0.402, 0.411] | 6.02 [6.00, 6.13] | 13.6 [13.5, 13.8] | 1.70 [1.69, 1.71] | 74% | 253 [252, 253] | 500 [499, 500] |
| 0.12 | 2 | port before | 5 | 19.7 [19.7, 19.9] | 0.216 [0.216, 0.217] | 0.492 [0.484, 0.497] | 2.73 [2.70, 2.76] | 8.43 [8.38, 8.44] | 0.963 [0.962, 0.965] | 83% | 221 [221, 222] | 439 [439, 440] |
| 0.12 | 2 | port | 5 | 8.30 [8.27, 8.34] | 0.0973 [0.0968, 0.0975] | 0.372 [0.370, 0.374] | 1.79 [1.78, 1.79] | 2.89 [2.86, 2.90] | 0.403 [0.401, 0.404] | 77% | 194 [194, 194] | 383 [383, 384] |
| 0.12 | 2 | port 2 x 4 | 5 | 5.96 [5.94, 5.99] | 0.0977 [0.0971, 0.0984] | 0.359 [0.359, 0.361] | 1.49 [1.47, 1.51] | 1.62 [1.61, 1.64] | 1.07 [1.07, 1.08] | 29% | 195 [195, 195] | 386 [385, 386] |
| 0.12 | 4 | C++ | 5 | 21.5 [21.3, 21.6] | 0.245 [0.242, 0.248] | 0.228 [0.225, 0.230] | 4.19 [4.18, 4.21] | 8.53 [8.45, 8.55] | 2.20 [2.18, 2.20] | 57% | 160 [159, 160] | 625 [622, 626] |
| 0.12 | 4 | port before | 5 | 12.3 [12.2, 12.3] | 0.120 [0.119, 0.120] | 0.269 [0.265, 0.275] | 2.01 [1.99, 2.04] | 4.99 [4.98, 5.01] | 1.21 [1.21, 1.21] | 66% | 136 [135, 136] | 528 [527, 529] |
| 0.12 | 4 | port | 5 | 5.51 [5.50, 5.53] | 0.0547 [0.0541, 0.0550] | 0.213 [0.212, 0.218] | 1.29 [1.29, 1.30] | 1.79 [1.79, 1.80] | 0.533 [0.532, 0.535] | 58% | 123 [123, 124] | 477 [476, 478] |
| 0.12 | 4 | port 4 x 2 | 5 | 5.25 [5.22, 5.31] | 0.0549 [0.0541, 0.0554] | 0.208 [0.206, 0.217] | 1.26 [1.26, 1.27] | 1.65 [1.63, 1.67] | 1.01 [1.00, 1.02] | 31% | 124 [123, 124] | 478 [477, 480] |
| 0.12 | 8 | C++ | 5 | 16.1 [16.1, 16.2] | 0.137 [0.130, 0.139] | 0.177 [0.168, 0.181] | 3.18 [3.04, 3.21] | 6.20 [6.16, 6.27] | 3.23 [3.19, 3.27] | 39% | 120 [120, 120] | 880 [879, 881] |
| 0.12 | 8 | port before | 5 | 10.2 [10.1, 10.2] | 0.0703 [0.0691, 0.0756] | 0.206 [0.199, 0.211] | 1.96 [1.94, 2.00] | 3.92 [3.86, 3.94] | 2.03 [2.02, 2.04] | 39% | 97.4 [96.7, 101] | 723 [722, 727] |
| 0.12 | 8 | port | 5 | 5.15 [5.12, 5.22] | 0.0311 [0.0308, 0.0317] | 0.171 [0.164, 0.176] | 1.28 [1.26, 1.28] | 1.66 [1.64, 1.72] | 1.01 [1.01, 1.03] | 30% | 93.0 [91.9, 93.5] | 679 [678, 683] |

Table "Port / upstream, series ins":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.245 [0.243, 0.247] | 0.313 [0.310, 0.313] | 0.219 [0.215, 0.221] | 0.840 [0.834, 0.840] | 0.169 [0.169, 0.171] | 0.254 [0.251, 0.255] |
| 0.12 | 2 | 0.237 [0.235, 0.239] | 0.298 [0.291, 0.298] | 0.210 [0.210, 0.213] | 0.913 [0.911, 0.924] | 0.205 [0.197, 0.206] | 0.250 [0.247, 0.252] |
| 0.12 | 4 | 0.243 [0.242, 0.244] | 0.309 [0.307, 0.311] | 0.210 [0.210, 0.212] | 0.945 [0.923, 0.961] | 0.224 [0.221, 0.224] | 0.257 [0.255, 0.259] |
| 0.12 | 8 | 0.314 [0.309, 0.319] | 0.401 [0.392, 0.421] | 0.268 [0.263, 0.279] | 0.961 [0.920, 1.02] | 0.231 [0.224, 0.239] | 0.320 [0.316, 0.326] |

Table "Port / port before, series ins":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.386 [0.383, 0.389] | 0.685 [0.680, 0.697] | 0.308 [0.306, 0.311] | 0.656 [0.649, 0.663] | 0.435 [0.433, 0.438] | 0.384 [0.382, 0.387] |
| 0.12 | 2 | 0.418 [0.417, 0.420] | 0.654 [0.646, 0.661] | 0.343 [0.339, 0.343] | 0.755 [0.744, 0.774] | 0.450 [0.447, 0.451] | 0.421 [0.418, 0.424] |
| 0.12 | 4 | 0.441 [0.440, 0.442] | 0.644 [0.635, 0.651] | 0.359 [0.357, 0.360] | 0.799 [0.771, 0.822] | 0.457 [0.451, 0.462] | 0.450 [0.449, 0.451] |
| 0.12 | 8 | 0.499 [0.495, 0.504] | 0.649 [0.630, 0.653] | 0.425 [0.417, 0.439] | 0.808 [0.789, 0.880] | 0.444 [0.408, 0.451] | 0.508 [0.504, 0.514] |

Table "Ranks × threads", series ins:

| h | configuration | loop s | against 8 ranks | against the same ranks, 1 thread |
|---|---|---|---|---|
| 0.12 | 2 ranks x 4 threads | 4.67 [4.64, 4.70] | 1.06 [1.05, 1.07] | 0.669 [0.661, 0.672] |
| 0.12 | 4 ranks x 2 threads | 4.37 [4.35, 4.41] | 0.996 [0.981, 1.00] | 0.943 [0.938, 0.955] |

#### Series `full12`: the full schedule on h = 0.12, 5,000 steps

Table "Points":

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.12 | 1 | C++ | 5 | 199 [199, 200] | 0.966 [0.964, 0.968] | 0.612 [0.609, 0.618] | 11.0 [10.9, 11.0] | 25.4 [25.2, 25.6] | 1.57 [1.57, 1.58] | 100% | 411 [411, 411] | 411 [411, 411] |
| 0.12 | 1 | port before | 5 | 156 [156, 157] | 0.377 [0.376, 0.377] | 0.785 [0.782, 0.789] | 7.69 [7.67, 7.71] | 20.8 [20.7, 20.8] | 1.23 [1.23, 1.23] | 100% | 366 [366, 366] | 366 [366, 366] |
| 0.12 | 1 | port | 5 | 59.2 [58.9, 59.5] | 0.164 [0.163, 0.166] | 0.512 [0.511, 0.515] | 4.83 [4.82, 4.85] | 6.09 [6.04, 6.12] | 0.472 [0.469, 0.474] | 100% | 345 [343, 346] | 345 [343, 346] |
| 0.12 | 2 | C++ | 5 | 153 [153, 153] | 0.474 [0.474, 0.493] | 0.406 [0.404, 0.408] | 8.42 [8.32, 8.53] | 20.3 [20.1, 20.3] | 2.47 [2.47, 2.49] | 63% | 264 [264, 264] | 522 [521, 522] |
| 0.12 | 2 | port before | 5 | 116 [116, 116] | 0.216 [0.215, 0.216] | 0.487 [0.482, 0.488] | 5.11 [5.10, 5.11] | 16.6 [16.5, 16.6] | 1.87 [1.87, 1.88] | 66% | 221 [221, 222] | 440 [439, 440] |
| 0.12 | 2 | port | 5 | 41.6 [41.4, 41.6] | 0.0970 [0.0964, 0.0977] | 0.371 [0.370, 0.377] | 3.19 [3.18, 3.20] | 4.57 [4.54, 4.58] | 0.671 [0.667, 0.671] | 70% | 215 [214, 215] | 428 [427, 429] |
| 0.12 | 2 | port 2 x 4 | 5 | 31.9 [31.8, 32.1] | 0.0971 [0.0967, 0.0978] | 0.359 [0.358, 0.362] | 2.53 [2.51, 2.54] | 3.30 [3.28, 3.33] | 2.01 [2.01, 2.03] | 23% | 217 [216, 218] | 433 [432, 435] |
| 0.12 | 4 | C++ | 5 | 102 [101, 102] | 0.245 [0.244, 0.246] | 0.225 [0.221, 0.226] | 6.07 [6.06, 6.09] | 12.9 [12.9, 13.0] | 3.28 [3.28, 3.29] | 48% | 199 [198, 199] | 761 [758, 762] |
| 0.12 | 4 | port before | 5 | 69.3 [69.2, 69.4] | 0.119 [0.119, 0.119] | 0.268 [0.266, 0.273] | 3.58 [3.56, 3.58] | 9.42 [9.39, 9.44] | 2.25 [2.24, 2.25] | 55% | 136 [136, 137] | 528 [527, 529] |
| 0.12 | 4 | port | 5 | 28.5 [28.5, 28.8] | 0.0540 [0.0537, 0.0543] | 0.212 [0.211, 0.217] | 2.24 [2.23, 2.25] | 3.11 [3.10, 3.15] | 0.923 [0.923, 0.931] | 51% | 136 [136, 137] | 522 [521, 523] |
| 0.12 | 4 | port 4 x 2 | 5 | 27.6 [27.4, 27.9] | 0.0542 [0.0541, 0.0543] | 0.209 [0.204, 0.211] | 2.11 [2.10, 2.13] | 3.05 [3.01, 3.08] | 1.78 [1.77, 1.80] | 26% | 138 [138, 138] | 527 [526, 529] |
| 0.12 | 8 | C++ | 5 | 82.7 [82.3, 88.0] | 0.135 [0.131, 0.137] | 0.177 [0.173, 0.183] | 4.41 [4.19, 4.57] | 11.0 [10.9, 11.7] | 5.32 [5.22, 5.61] | 29% | 169 [168, 169] | 1,252 [1,249, 1,253] |
| 0.12 | 8 | port before | 5 | 52.4 [52.1, 52.5] | 0.0696 [0.0677, 0.0705] | 0.208 [0.205, 0.222] | 3.08 [3.06, 3.10] | 6.77 [6.74, 6.79] | 3.41 [3.38, 3.41] | 36% | 97.8 [96.4, 100] | 722 [722, 725] |
| 0.12 | 8 | port | 5 | 25.7 [25.7, 25.7] | 0.0311 [0.0307, 0.0353] | 0.171 [0.163, 0.176] | 1.97 [1.96, 1.98] | 2.87 [2.86, 2.89] | 1.67 [1.67, 1.68] | 28% | 96.3 [96.0, 97.7] | 731 [728, 732] |

Table "Port / upstream, series full12":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.299 [0.299, 0.302] | 0.441 [0.439, 0.443] | 0.239 [0.238, 0.242] | 0.838 [0.831, 0.841] | 0.170 [0.169, 0.171] | 0.296 [0.296, 0.299] |
| 0.12 | 2 | 0.271 [0.269, 0.272] | 0.377 [0.374, 0.385] | 0.226 [0.224, 0.227] | 0.915 [0.906, 0.929] | 0.204 [0.197, 0.206] | 0.272 [0.271, 0.273] |
| 0.12 | 4 | 0.281 [0.281, 0.284] | 0.369 [0.367, 0.370] | 0.240 [0.240, 0.244] | 0.952 [0.936, 0.981] | 0.221 [0.220, 0.221] | 0.281 [0.280, 0.283] |
| 0.12 | 8 | 0.315 [0.299, 0.320] | 0.445 [0.433, 0.470] | 0.262 [0.246, 0.263] | 0.988 [0.889, 1.00] | 0.230 [0.226, 0.269] | 0.310 [0.292, 0.312] |

Table "Port / port before, series full12":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.12 | 1 | 0.383 [0.383, 0.385] | 0.629 [0.627, 0.630] | 0.293 [0.291, 0.294] | 0.652 [0.649, 0.658] | 0.435 [0.432, 0.441] | 0.378 [0.378, 0.380] |
| 0.12 | 2 | 0.358 [0.355, 0.359] | 0.624 [0.622, 0.627] | 0.276 [0.273, 0.277] | 0.763 [0.761, 0.774] | 0.450 [0.447, 0.454] | 0.359 [0.356, 0.360] |
| 0.12 | 4 | 0.411 [0.410, 0.415] | 0.626 [0.623, 0.629] | 0.330 [0.329, 0.334] | 0.792 [0.784, 0.816] | 0.453 [0.451, 0.457] | 0.412 [0.411, 0.415] |
| 0.12 | 8 | 0.492 [0.490, 0.494] | 0.642 [0.636, 0.644] | 0.424 [0.422, 0.426] | 0.794 [0.733, 0.859] | 0.449 [0.441, 0.521] | 0.491 [0.488, 0.493] |

Table "Ranks × threads", series full12:

| h | configuration | loop s | against 8 ranks | against the same ranks, 1 thread |
|---|---|---|---|---|
| 0.12 | 2 ranks x 4 threads | 29.1 [29.0, 29.4] | 1.20 [1.20, 1.21] | 0.750 [0.748, 0.756] |
| 0.12 | 4 ranks x 2 threads | 25.8 [25.6, 26.0] | 1.06 [1.06, 1.07] | 0.965 [0.948, 0.974] |

#### Series `reh6`: the full schedule on h = 0.06, 5,000 steps

Table "Points":

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.06 | 1 | C++ | 5 | 1,952 [1,951, 1,974] | 13.9 [13.9, 14.0] | 13.0 [12.9, 13.1] | 82.0 [82.0, 82.5] | 278 [278, 282] | 2.03 [2.03, 2.06] | 100% | 3,180 [3,179, 3,180] | 3,180 [3,179, 3,180] |
| 0.06 | 1 | port | 5 | 498 [498, 500] | 1.34 [1.33, 1.34] | 10.2 [10.2, 10.3] | 35.1 [35.1, 35.2] | 57.0 [56.8, 57.1] | 0.519 [0.518, 0.520] | 100% | 2,622 [2,620, 2,622] | 2,622 [2,620, 2,622] |
| 0.06 | 2 | C++ | 5 | 1,238 [1,233, 1,242] | 6.26 [5.72, 6.27] | 8.60 [8.56, 8.72] | 55.8 [55.5, 56.0] | 175 [174, 176] | 2.60 [2.60, 2.61] | 78% | 1,875 [1,875, 1,879] | 3,743 [3,743, 3,747] |
| 0.06 | 2 | port | 5 | 302 [301, 304] | 0.737 [0.736, 0.739] | 7.40 [7.39, 7.41] | 21.5 [21.5, 21.6] | 34.6 [34.4, 34.8] | 0.633 [0.630, 0.637] | 82% | 1,608 [1,600, 1,610] | 3,187 [3,177, 3,196] |
| 0.06 | 2 | port 2 x 4 | 5 | 223 [222, 223] | 0.740 [0.738, 0.744] | 7.16 [7.14, 7.17] | 17.3 [17.2, 17.3] | 22.9 [22.8, 22.9] | 1.81 [1.81, 1.81] | 29% | 1,617 [1,615, 1,623] | 3,209 [3,200, 3,218] |
| 0.06 | 4 | C++ | 5 | 736 [734, 748] | 2.56 [2.54, 2.61] | 4.62 [4.58, 4.65] | 39.9 [39.7, 40.1] | 98.4 [98.1, 101] | 3.12 [3.11, 3.18] | 65% | 989 [964, 998] | 3,878 [3,749, 3,891] |
| 0.06 | 4 | port | 5 | 200 [200, 201] | 0.381 [0.380, 0.383] | 4.06 [4.03, 4.42] | 15.0 [14.9, 15.1] | 22.4 [22.3, 22.6] | 0.842 [0.839, 0.849] | 62% | 849 [847, 857] | 3,304 [3,299, 3,341] |
| 0.06 | 4 | port 4 x 2 | 5 | 188 [187, 189] | 0.379 [0.378, 0.381] | 3.94 [3.92, 4.17] | 14.5 [14.4, 14.5] | 20.5 [20.4, 20.5] | 1.58 [1.57, 1.58] | 33% | 858 [854, 859] | 3,335 [3,332, 3,350] |
| 0.06 | 8 | C++ | 5 | 496 [491, 502] | 1.19 [1.17, 1.23] | 3.23 [3.02, 3.29] | 26.1 [25.7, 26.3] | 66.2 [65.6, 67.2] | 4.16 [4.12, 4.22] | 49% | 626 [616, 689] | 4,517 [4,438, 4,570] |
| 0.06 | 8 | port | 5 | 175 [174, 176] | 0.220 [0.218, 0.226] | 2.94 [2.89, 3.01] | 13.7 [13.6, 13.8] | 19.2 [19.0, 19.2] | 1.48 [1.47, 1.49] | 35% | 532 [517, 609] | 3,831 [3,812, 3,928] |

Table "Port / upstream, series reh6":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.06 | 1 | 0.255 [0.252, 0.256] | 0.428 [0.426, 0.429] | 0.205 [0.202, 0.205] | 0.787 [0.785, 0.793] | 0.0961 [0.0956, 0.0963] | 0.255 [0.252, 0.256] |
| 0.06 | 2 | 0.243 [0.242, 0.244] | 0.385 [0.385, 0.387] | 0.198 [0.196, 0.199] | 0.860 [0.850, 0.864] | 0.118 [0.117, 0.129] | 0.244 [0.243, 0.245] |
| 0.06 | 4 | 0.270 [0.267, 0.272] | 0.377 [0.374, 0.378] | 0.227 [0.224, 0.230] | 0.881 [0.867, 0.960] | 0.149 [0.146, 0.150] | 0.272 [0.269, 0.274] |
| 0.06 | 8 | 0.356 [0.351, 0.359] | 0.528 [0.521, 0.531] | 0.288 [0.285, 0.292] | 0.910 [0.881, 0.995] | 0.185 [0.178, 0.194] | 0.352 [0.349, 0.356] |

Table "Ranks × threads", series reh6:

| h | configuration | loop s | against 8 ranks | against the same ranks, 1 thread |
|---|---|---|---|---|
| 0.06 | 2 ranks x 4 threads | 201 [201, 201] | 1.22 [1.21, 1.23] | 0.716 [0.711, 0.720] |
| 0.06 | 4 ranks x 2 threads | 175 [174, 175] | 1.06 [1.06, 1.07] | 0.931 [0.930, 0.940] |

#### Series `reh4`: the static solve and 300 steps on h = 0.04

Table "Points":

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.04 | 1 | C++ | 5 | 519 [518, 520] | 73.6 [73.5, 73.7] | 82.0 [81.8, 82.0] | 198 [198, 200] | 875 [871, 878] | 1.92 [1.91, 1.93] | 100% | 11,670 [11,669, 11,670] | 11,670 [11,669, 11,670] |
| 0.04 | 1 | port before | 5 | 268 [268, 268] | 10.4 [10.4, 10.4] | 84.4 [84.3, 84.6] | 89.4 [89.1, 89.6] | 353 [352, 354] | 0.791 [0.790, 0.792] | 100% | 10,271 [10,271, 10,271] | 10,271 [10,271, 10,271] |
| 0.04 | 1 | port | 5 | 159 [159, 160] | 5.45 [5.43, 5.45] | 70.3 [70.1, 70.3] | 65.5 [65.4, 65.8] | 178 [178, 179] | 0.436 [0.436, 0.438] | 100% | 8,965 [8,965, 8,965] | 8,965 [8,965, 8,965] |
| 0.04 | 2 | C++ | 5 | 297 [297, 301] | 32.0 [32.0, 32.0] | 57.7 [57.7, 57.8] | 126 [125, 127] | 475 [473, 487] | 2.15 [2.15, 2.19] | 89% | 6,961 [6,961, 6,962] | 13,741 [13,740, 13,744] |
| 0.04 | 2 | port before | 5 | 149 [149, 149] | 5.70 [5.69, 5.75] | 51.0 [51.0, 51.3] | 59.2 [58.7, 59.6] | 184 [183, 184] | 0.868 [0.866, 0.871] | 91% | 5,884 [5,875, 5,885] | 11,347 [11,338, 11,348] |
| 0.04 | 2 | port | 5 | 101 [101, 102] | 2.89 [2.88, 2.92] | 52.6 [52.5, 52.7] | 40.2 [39.9, 40.3] | 91.8 [91.5, 92.5] | 0.473 [0.471, 0.475] | 92% | 5,568 [5,551, 5,577] | 11,016 [10,986, 11,025] |
| 0.04 | 2 | port 2 x 4 | 5 | 86.0 [85.9, 86.0] | 2.88 [2.87, 2.89] | 51.6 [51.4, 51.6] | 33.1 [33.0, 33.2] | 51.2 [51.0, 51.5] | 1.20 [1.20, 1.21] | 36% | 5,568 [5,567, 5,574] | 11,016 [11,014, 11,023] |
| 0.04 | 4 | C++ | 5 | 158 [158, 159] | 11.8 [11.5, 12.0] | 27.8 [27.6, 28.8] | 81.6 [81.3, 81.8] | 256 [253, 260] | 2.41 [2.40, 2.44] | 80% | 3,488 [3,383, 3,510] | 13,115 [13,105, 13,157] |
| 0.04 | 4 | port before | 5 | 89.6 [89.3, 91.2] | 2.90 [2.89, 2.91] | 28.4 [28.3, 30.1] | 43.8 [43.7, 43.8] | 112 [111, 112] | 1.11 [1.11, 1.12] | 71% | 3,126 [3,042, 3,130] | 11,688 [11,663, 11,723] |
| 0.04 | 4 | port | 5 | 58.1 [56.3, 58.3] | 1.45 [1.44, 1.46] | 26.9 [25.1, 27.1] | 29.7 [29.6, 29.7] | 55.8 [55.8, 55.9] | 0.612 [0.611, 0.613] | 71% | 2,697 [2,693, 2,767] | 10,377 [10,352, 10,396] |
| 0.04 | 4 | port 4 x 2 | 5 | 55.0 [54.0, 55.6] | 1.44 [1.43, 1.45] | 25.9 [24.8, 26.5] | 30.1 [29.8, 30.1] | 48.8 [48.6, 49.0] | 1.13 [1.13, 1.13] | 39% | 2,717 [2,715, 2,775] | 10,433 [10,361, 10,440] |
| 0.04 | 8 | C++ | 5 | 103 [102, 104] | 5.09 [5.01, 5.32] | 18.2 [17.9, 20.6] | 58.4 [57.3, 59.4] | 158 [157, 161] | 3.10 [3.09, 3.15] | 62% | 2,419 [2,066, 2,884] | 14,760 [14,467, 15,038] |
| 0.04 | 8 | port before | 5 | 67.5 [67.0, 69.1] | 1.58 [1.57, 1.59] | 19.0 [18.6, 20.6] | 43.9 [43.8, 44.5] | 82.1 [82.0, 82.2] | 1.80 [1.80, 1.81] | 44% | 1,966 [1,868, 2,226] | 13,611 [12,929, 13,654] |
| 0.04 | 8 | port | 5 | 45.2 [44.6, 46.3] | 0.775 [0.767, 0.786] | 17.7 [17.0, 18.8] | 30.9 [30.8, 31.0] | 47.0 [46.8, 47.3] | 1.12 [1.11, 1.12] | 39% | 2,054 [1,700, 2,085] | 11,735 [11,199, 12,138] |

Table "Port / upstream, series reh4":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.04 | 1 | 0.227 [0.226, 0.228] | 0.330 [0.330, 0.330] | 0.204 [0.203, 0.204] | 0.857 [0.855, 0.858] | 0.0740 [0.0738, 0.0741] | 0.307 [0.306, 0.308] |
| 0.04 | 2 | 0.219 [0.216, 0.220] | 0.320 [0.316, 0.320] | 0.193 [0.190, 0.194] | 0.911 [0.909, 0.913] | 0.0902 [0.0899, 0.0912] | 0.340 [0.338, 0.342] |
| 0.04 | 4 | 0.254 [0.250, 0.256] | 0.364 [0.363, 0.365] | 0.218 [0.215, 0.221] | 0.959 [0.906, 0.977] | 0.123 [0.119, 0.127] | 0.367 [0.355, 0.368] |
| 0.04 | 8 | 0.358 [0.355, 0.363] | 0.529 [0.518, 0.542] | 0.296 [0.291, 0.299] | 0.970 [0.862, 1.02] | 0.152 [0.147, 0.157] | 0.443 [0.429, 0.444] |

Table "Port / port before, series reh4":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---|---|---|---|---|---|---|
| 0.04 | 1 | 0.551 [0.550, 0.553] | 0.732 [0.731, 0.738] | 0.505 [0.504, 0.506] | 0.832 [0.831, 0.833] | 0.523 [0.522, 0.524] | 0.595 [0.594, 0.596] |
| 0.04 | 2 | 0.544 [0.543, 0.545] | 0.678 [0.673, 0.687] | 0.501 [0.500, 0.504] | 1.03 [1.03, 1.03] | 0.507 [0.502, 0.513] | 0.682 [0.681, 0.682] |
| 0.04 | 4 | 0.549 [0.549, 0.551] | 0.678 [0.677, 0.679] | 0.499 [0.498, 0.501] | 0.941 [0.886, 0.955] | 0.500 [0.495, 0.504] | 0.647 [0.627, 0.652] |
| 0.04 | 8 | 0.618 [0.616, 0.621] | 0.700 [0.697, 0.707] | 0.572 [0.571, 0.575] | 0.951 [0.851, 0.983] | 0.491 [0.487, 0.499] | 0.674 [0.654, 0.685] |

Table "Ranks × threads", series reh4:

| h | configuration | loop s | against 8 ranks | against the same ranks, 1 thread |
|---|---|---|---|---|
| 0.04 | 2 ranks x 4 threads | 25.2 [25.2, 25.4] | 1.08 [1.07, 1.09] | 0.637 [0.635, 0.642] |
| 0.04 | 4 ranks x 2 threads | 23.6 [23.6, 23.7] | 1.01 [1.01, 1.01] | 0.922 [0.919, 0.923] |

#### Series `eig`: the port's SparseLU at 1 to 8 ranks (one repetition)

Table "Points":

| h | ranks | code | n | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| 0.12 | 1 | port before SparseLU | 1 | 39.9 | 0.376 | 7.55 | 3.75 | 14.9 | 0.806 | 100% | 579 | 579 |
| 0.12 | 1 | port SparseLU | 1 | 14.4 | 0.163 | 2.15 | 2.61 | 4.56 | 0.310 | 100% | 419 | 419 |
| 0.12 | 2 | port before SparseLU | 1 | 26.4 | 0.216 | 7.22 | 2.73 | 8.42 | 0.963 | 84% | 445 | 617 |
| 0.12 | 2 | port SparseLU | 1 | 9.92 | 0.0968 | 1.98 | 1.77 | 2.90 | 0.403 | 77% | 331 | 471 |
| 0.12 | 2 | port SparseLU 2 x 4 | 1 | 6.89 | 0.0973 | 1.29 | 1.48 | 1.62 | 1.07 | 29% | 341 | 482 |
| 0.12 | 4 | port before SparseLU | 1 | 19.2 | 0.119 | 7.15 | 2.00 | 5.00 | 1.21 | 67% | 366 | 680 |
| 0.12 | 4 | port SparseLU | 1 | 7.19 | 0.0543 | 1.86 | 1.30 | 1.80 | 0.535 | 58% | 300 | 570 |
| 0.12 | 4 | port SparseLU 4 x 2 | 1 | 6.51 | 0.0539 | 1.44 | 1.27 | 1.66 | 1.01 | 31% | 295 | 567 |
| 0.12 | 8 | port before SparseLU | 1 | 16.9 | 0.0706 | 6.98 | 1.93 | 3.90 | 2.01 | 40% | 356 | 850 |
| 0.12 | 8 | port SparseLU | 1 | 6.77 | 0.0308 | 1.81 | 1.25 | 1.67 | 1.01 | 31% | 278 | 739 |

#### Weak and strong scaling

Efficiency = core-µs per element and step at the study's first point over
the core-µs at P, same code (medians):

| study | ranks | code | efficiency (core-µs at the first point / at P) |
|---|---|---|---|
| weak A (≈ 23 k tets per rank) | 2 / 4 / 8 | port | 0.71 / 0.52 / 0.27 |
| weak A (≈ 23 k tets per rank) | 2 / 4 / 8 | port before | 0.86 / 0.67 / 0.40 |
| weak A (≈ 23 k tets per rank) | 2 / 4 / 8 | upstream | 0.72 / 0.57 / 0.40 |

| study | ranks | code | efficiency (core-µs at the first point / at P) |
|---|---|---|---|
| weak B (≈ 45 k tets per rank) | 2 / 4 / 8 | port | 0.82 / 0.60 / 0.31 |
| weak B (≈ 45 k tets per rank) | 2 / 4 / 8 | port before | 0.88 / 0.69 / 0.41 |
| weak B (≈ 45 k tets per rank) | 2 / 4 / 8 | upstream | 0.73 / 0.59 / 0.44 |

| study | ranks | code | efficiency (core-µs at the first point / at P) |
|---|---|---|---|
| strong, h = 0.06, before insertion | 2 / 4 / 8 | port | 0.89 / 0.68 / 0.36 |
| strong, h = 0.06, before insertion | 2 / 4 / 8 | port before | 0.90 / 0.70 / 0.42 |
| strong, h = 0.06, before insertion | 2 / 4 / 8 | upstream | 0.86 / 0.75 / 0.55 |

| study | ranks | code | efficiency (core-µs at the first point / at P) |
|---|---|---|---|
| strong, h = 0.12, with insertion | 2 / 4 / 8 | port | 0.77 / 0.58 / 0.30 |
| strong, h = 0.12, with insertion | 2 / 4 / 8 | port before | 0.83 / 0.66 / 0.39 |
| strong, h = 0.12, with insertion | 2 / 4 / 8 | upstream | 0.74 / 0.57 / 0.39 |

#### What the part C tables show

- The loop, port / upstream: 0.22–0.30 at 1 to 4 ranks and 0.31–0.36 at 8
  ranks over the series `ws`, `ins`, `full12`, `reh6` and `reh4`; the wall
  time 0.24–0.37 at 1 to 4 ranks and 0.31–0.44 at 8.
- The loop, port / port before: 0.36–0.62 (series `ws`, `ins`, `full12`,
  `reh4`), highest at 8 ranks in every series (0.49–0.62).
- `solve_step`, port / upstream: 0.29–0.44 at 1 to 4 ranks, 0.40–0.53 at 8;
  the check 0.19–0.24 at 1 to 4 ranks, 0.26–0.30 at 8.
- Strong scaling on h = 0.06 before insertion at 8 ranks: loop efficiency
  0.36 (port), 0.42 (port before), 0.55 (upstream). The port's lead over
  upstream is smallest at 8 ranks in every series.
- Ranks × threads on 8 cores: 2 × 4 takes 1.05 / 1.06 / 1.20 / 1.22 / 1.08
  times the loop of 8 ranks (`ws`, `ins`, `full12`, `reh6`, `reh4`), 4 × 2
  0.993 / 0.996 / 1.06 / 1.06 / 1.01.
- The static solve phase, port / upstream: 0.79–0.96 at 1 to 4 ranks,
  0.91–1.11 at 8 (h = 0.0476 at 8 ranks: 1.11 [0.977, 1.60]).
- The benchmark's 8 cores are one node of 8 cores; these series say nothing
  about larger rank counts or several nodes.

### Before the final campaign (boat.dev sandboxes, 12–13 November)

Two series on one boat `large` sandbox (AMD EPYC 9455, 8 vCPU, one NUMA
node, 2026-11-13), the port against the strict C++ install
with the JOSS emulation, MUMPS in both. Series `ws`: 500 steps, before the
first insertion on every mesh (no run inserted), ten points: weak study A
(about 23 k tetrahedra per rank: h = 0.12 on 1 rank, 0.0925 on 2, 0.0756 on
4, 0.06 on 8), weak study B (about 45 k per rank: 0.0925 on 1 to 0.0476 on
8) and the strong study on h = 0.06. Series `ins`: the insertion regime on
the h = 0.12 mesh, 1,500 steps, 1 / 2 / 4 / 8 ranks. Five repetitions, each
running every point once (point order reversed every other repetition), the
two codes adjacent per point with their order rotated; one container per run
(14 GiB, no swap; the port's container `--cpus 8`, the C++ one without
quota). At 8 ranks every vCPU runs a rank, with no core left for the system.
Made with `scaling_series.sh`; tables by `scaling_analyze.py`
(`docs/benchmarks/scaling-2026-11-13/ws-summary.md`, `ins-summary.md`).

The per-point tables (phases, core-µs per element and step, static solve,
`init_full` and peak memory of both codes, median [min, max]) are in
`ws-summary.md` ("Points") and `ins-summary.md`.

Table "Port / upstream, series ws" (paired: the two runs of each repetition;
median [min, max]):

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---:|---|---|---|---|---|---|
| 0.12 | 1 | 0.539 [0.516, 0.57] | 0.454 [0.42, 0.491] | 0.564 [0.549, 0.596] | 1.22 [1.06, 1.25] | 0.395 [0.391, 0.427] | 0.592 [0.576, 0.625] |
| 0.0925 | 1 | 0.452 [0.443, 0.47] | 0.424 [0.413, 0.467] | 0.453 [0.449, 0.487] | 1.1 [0.886, 1.28] | 0.34 [0.324, 0.349] | 0.513 [0.492, 0.531] |
| 0.0925 | 2 | 0.511 [0.46, 0.536] | 0.483 [0.422, 0.511] | 0.511 [0.452, 0.556] | 1.13 [0.544, 1.2] | 0.427 [0.337, 0.532] | 0.552 [0.471, 0.591] |
| 0.0756 | 2 | 0.427 [0.407, 0.442] | 0.474 [0.375, 0.513] | 0.425 [0.385, 0.435] | 1.09 [0.884, 1.17] | 0.355 [0.309, 0.406] | 0.482 [0.463, 0.518] |
| 0.0756 | 4 | 0.482 [0.393, 0.532] | 0.508 [0.419, 0.603] | 0.474 [0.384, 0.508] | 1.13 [0.975, 1.53] | 0.512 [0.461, 0.654] | 0.532 [0.47, 0.58] |
| 0.06 | 1 | 0.385 [0.352, 0.423] | 0.443 [0.388, 0.535] | 0.363 [0.334, 0.42] | 1.03 [0.901, 1.28] | 0.236 [0.215, 0.323] | 0.443 [0.401, 0.484] |
| 0.06 | 2 | 0.442 [0.382, 0.45] | 0.462 [0.427, 0.604] | 0.399 [0.359, 0.457] | 1.11 [0.94, 1.28] | 0.356 [0.313, 0.478] | 0.519 [0.461, 0.53] |
| 0.06 | 4 | 0.448 [0.4, 0.465] | 0.512 [0.41, 0.574] | 0.408 [0.366, 0.458] | 1.02 [0.957, 1.17] | 0.395 [0.362, 0.681] | 0.498 [0.453, 0.53] |
| 0.06 | 8 | 0.544 [0.5, 0.586] | 0.763 [0.577, 0.783] | 0.482 [0.471, 0.518] | 1.1 [0.861, 1.4] | 0.61 [0.509, 0.777] | 0.564 [0.518, 0.629] |
| 0.0476 | 8 | 0.513 [0.508, 0.598] | 0.775 [0.727, 0.915] | 0.434 [0.427, 0.531] | 0.65 [0.449, 2.91] | 0.44 [0.3, 0.54] | 0.578 [0.481, 0.75] |

At h = 0.0476 on 8 ranks the port's medians are over six repetitions and
upstream's over five (its first was the excluded UD-216 run, repeated once
more for both codes), so the paired ratio is the like-for-like figure.

Table "Port / upstream, series ins":

| h | ranks | loop | `solve_step` | check | static solve | `init_full` | wall |
|---|---:|---|---|---|---|---|---|
| 0.12 | 1 | 0.581 [0.511, 0.59] | 0.482 [0.459, 0.514] | 0.602 [0.526, 0.614] | 1.13 [0.734, 1.62] | 0.413 [0.373, 0.441] | 0.603 [0.536, 0.627] |
| 0.12 | 2 | 0.593 [0.515, 0.635] | 0.512 [0.469, 0.555] | 0.625 [0.533, 0.666] | 1.31 [0.854, 2.34] | 0.49 [0.451, 0.778] | 0.611 [0.531, 0.655] |
| 0.12 | 4 | 0.574 [0.554, 0.606] | 0.572 [0.537, 0.575] | 0.575 [0.552, 0.622] | 1.08 [0.864, 1.69] | 0.522 [0.509, 0.593] | 0.586 [0.561, 0.616] |
| 0.12 | 8 | 0.547 [0.529, 0.8] | 0.657 [0.57, 0.938] | 0.534 [0.489, 0.731] | 1.23 [0.969, 1.52] | 0.591 [0.514, 0.747] | 0.538 [0.501, 0.767] |

Table "Weak and strong scaling, boat" (core-µs/el/step medians; weak
efficiency E_w = core-µs at 1 rank / at P; strong efficiency = speed-up /
ranks; medians, then the median [min, max] of the per-repetition values;
full tables: `ws-summary.md` "Weak study `A`", "Weak study `B`", "Strong
study h = 0.06", `ins-summary.md` "Strong study h = 0.12"):

| Study | Ranks | Port | Upstream |
|---|---:|---|---|
| weak A (≈ 23 k tets per rank) | 2 / 4 / 8 | E_w 0.78 / 0.51 / 0.28; per rep. at 8: 0.305 [0.225, 0.455] | E_w 0.76 / 0.48 / 0.30; per rep. at 8: 0.303 [0.22, 0.442] |
| weak B (≈ 45 k tets per rank) | 2 / 4 / 8 | E_w 0.68 / 0.56 / 0.34; per rep. at 8: 0.353 [0.215, 0.37] | E_w 0.67 / 0.58 / 0.37; per rep. at 8: 0.342 [0.276, 0.449] |
| strong, h = 0.06, before insertion | 2 / 4 / 8 | 88 / 65 / 33 %; per rep. at 8: 0.327 [0.292, 0.435] | 92 / 82 / 51 %; per rep. at 8: 0.528 [0.422, 0.591] |
| strong, h = 0.12, with insertion | 2 / 4 / 8 | 72 / 51 / 26 %; per rep. at 8: 0.266 [0.249, 0.491] | 70 / 50 / 37 %; per rep. at 8: 0.363 [0.239, 0.466] |

Reading:

- Before insertion the port's loop takes 0.39–0.54× upstream's time (the
  step 0.42–0.78×, the check 0.36–0.56×), `init_full` 0.24–0.61×, the wall
  0.44–0.59×. In the insertion regime the loop takes 0.55–0.59×.
- The port's explicit step loses its lead as ranks are added: 0.44×
  upstream's at 1 rank, 0.46× at 2, 0.51× at 4, 0.76× at 8 on h = 0.06. That
  fits a per-step parallel part that costs about the same in both codes
  while the port's serial step is cheaper; no per-rank timing confirms it.
  The check keeps most of its lead (0.48× at 8 ranks).
- Weak scaling: both codes lose about the same share, a quarter already at
  2 ranks; the cause (ghost elements, the exchange of every step, vCPUs
  sharing a physical core or its caches) is not measured. In strong scaling
  the port's serial loop is 2.7× faster (131 against 359 ms per step on
  h = 0.06) and at 8 ranks still takes 0.54× upstream's time.
- The machine drifted during the series: repetitions 1, 2 and the extra
  repetition 6 ran fast, 3–5 slow; the larger meshes took up to 1.4–2.15×
  longer per step in the slow repetitions, for both codes. The paired ratios
  stay in their ranges, but the efficiencies at 4 and 8 ranks depend on the
  machine's state. Repetitions 1 and 2 alone give weak A at 8 ranks E_w =
  0.41–0.46 (port) and 0.41–0.44 (upstream), and the strong study at 8 ranks
  42–43 % and 54–59 %. Neither set is the undisturbed machine, and whether
  the 8-rank loss is a fixed per-step cost is not settled.
- In the insertion series the 8-rank runs vary by about 2× in both codes
  (2,894 tetrahedra per rank, and the machine sped up during the series).
#### The mid-size rehearsal

The full 5,000-step schedule on h = 0.06 and a static solve plus 300 steps
on h = 0.04, both codes, 1 / 2 / 4 / 8 ranks (boat `large`, AMD EPYC 9455,
8 vCPU, 15.6 GiB, 2026-11-12; `c1prep_campaign.sh`, tables by
`c1prep_analyze.py`; `docs/benchmarks/midsize-2026-11-12/item2-h006/`,
`item2-h004/`). h = 0.06: one repetition at 1 and 2 ranks, the median of two
at 4 and 8; h = 0.04: one repetition.

Table "h = 0.06, full schedule" (the mesh fragments: first insertion at
step 712, about 51.5 k cohesive elements, nodes 32.5 k → 65 k, 3,977
insertion steps; loop efficiency = core-µs at 1 rank / core-µs at P):

| ranks | code | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | loop efficiency | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | C++ | 1,600 | 11.1 | 12.3 | 63.4 | 233 | 1.67 | 100 % | 3,180 | 3,180 |
| 1 | Rust | 1,090 | 2.42 | 13.0 | 45.0 | 154 | 1.12 | 100 % | 2,850 | 2,850 |
| 2 | C++ | 1,040 | 4.37 | 10.4 | 47.9 | 145 | 2.18 | 77 % | 1,870 | 3,740 |
| 2 | Rust | 748 | 1.49 | 9.55 | 26.1 | 112 | 1.56 | 72 % | 1,690 | 3,370 |
| 4 | C++ | 637 | 2.05 | 5.18 | 32.5 | 86.5 | 2.69 | 62 % | 979 | 3,790 |
| 4 | Rust | 430 | 0.762 | 4.71 | 17.7 | 62.4 | 1.81 | 62 % | 880 | 3,430 |
| 8 | C++ | 393 | 0.888 | 2.87 | 19.8 | 52.3 | 3.25 | 51 % | 634 | 4,450 |
| 8 | Rust | 289 | 0.442 | 4.27 | 13.9 | 39.6 | 2.41 | 46 % | 565 | 4,140 |

Port / C++ at 1 / 2 / 4 / 8 ranks: wall 0.68 / 0.72 / 0.68 / 0.74, explicit
step 0.71 / 0.55 / 0.54 / 0.70, check 0.66 / 0.77 / 0.72 / 0.76,
`init_full` 0.22 / 0.34 / 0.37 / 0.50, static solve 1.06 / 0.92 / 0.91 /
1.49, largest-rank peak 0.90 / 0.90 / 0.90 / 0.89. At one rank the two runs
are bitwise identical over all 5,000 steps (51,528 cohesive elements,
E_d = 447.0954309862559 J). The counts at P > 1 are written as measured;
they differ by up to 1.6 % between runs and are not a correctness criterion
(upstream's distributed runs leave the serial problem through UD-890; these
port runs predate the load-placement correction UD-1280).

Table "h = 0.04, static solve + 300 pre-insertion steps" (293,745 DOFs; no
insertion within 300 steps):

| ranks | code | wall s | init_full s | static solve s | `solve_step` ms/step | check ms/step | core-µs/el/step | largest rank peak MiB | Σ rank peaks MiB |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | C++ | 456 | 57.8 | 83.4 | 166 | 718 | 1.58 | 11,666 | 11,666 |
| 1 | Rust | 234 | 7.99 | 75.9 | 70.5 | 286 | 0.637 | 10,265 | 10,265 |
| 2 | C++ | 267 | 23.7 | 57.0 | 113 | 399 | 1.83 | 6,608 | 12,786 |
| 2 | Rust | 169 | 4.90 | 66.2 | 58.8 | 183 | 0.866 | 6,535 | 12,697 |
| 4 | C++ | 178 | 9.67 | 38.6 | 87.1 | 261 | 2.49 | 3,393 | 13,180 |
| 4 | Rust | 90.2 | 2.96 | 29.3 | 42.4 | 97.8 | 1.00 | 3,124 | 11,647 |
| 8 | C++ | killed by the 14 GiB container limit in the static solve (45.5 s; the host has 15.6 GiB) |  |  |  |  |  |  | > 14,336 |
| 8 | Rust | 67.9 | 1.70 | 19.7 | 45.5 | 74.2 | 1.71 | 2,326 | 13,559 |

Port / C++ at 1 / 2 / 4 ranks: wall 0.51 / 0.63 / 0.51, explicit step 0.42 /
0.52 / 0.49, check 0.40 / 0.46 / 0.37, `init_full` 0.14 / 0.21 / 0.31,
static solve 0.91 / 1.16 / 0.76, largest-rank peak 0.88 / 0.99 / 0.92.
MUMPS (PORD at every P, no retry): allocation `INFOG(19)` 5,302 / 7,586 /
6,012 / 8,167 MB summed at 1 / 2 / 4 / 8 ranks, elimination flops 2.42e12 /
2.36e12 / 2.02e12 / 2.09e12.

On the same mesh before any insertion (300 steps, 1 and 8 ranks) the port
costs 0.45 / 0.49 of upstream per element and step; the full schedule
multiplies the cost by ×1.29 / ×1.09 for upstream and ×1.91 / ×1.65 for the
port. The port's lead per element and step shrinks from about 2.1× to about
1.4× once the mesh fragments (its explicit step with about 52 k cohesive
elements and doubled nodes, and the check after insertions); this was not
profiled. The CPU quota of the port's container (`--cpus 8`, upstream's has
none) cost the port's explicit step about 8 % at 8 ranks (10.2 against
9.4 ms per step, 3 alternated pairs).

### The full-size run

#### Final campaign: one 128-core machine (part G, 2026-11-17/18)

The JOSS mesh, 5,000 steps with insertion, on one AWS `m7a.32xlarge` (AMD
EPYC 9R14, 128 cores, one thread per core, two NUMA nodes of 64 cores,
493 GiB visible), 2026-11-17 22:35 to 2026-11-18 03:38 UTC. Both codes at
128, 64 and 32 ranks, the port also at 64 × 2, 32 × 4 and 16 × 8 threads
on all 128 cores; C++: the strict install with the JOSS emulation, MUMPS
5.7.3; port: `740a79a36` (driver `90d301c9…`). The same Scotch partition
per rank count, exported first by the C++ harness; every rank bound to its
own cores (section "Method"), NUMA node 0 filled first: the 32- and
64-rank runs use one socket, the 128-rank and hybrid runs both. One run
per point. Data and the first 128-rank sample:
`docs/benchmarks/perf-campaign/final/G/` (`README.md`). The tables below
are those of `G/summary.md` as generated; their "loop s" is `solve_step`
plus the check. The loop with the per-step energies and boundary
conditions (the driver's loop, as in the multi-node tables) is in
`multinode/table.md` and in the section "Multi-node runs".

Table "Strong scaling" (rank 0's phases; core-µs per element and step = (`solve_step` + check) × cores / tetrahedra / steps; loop efficiency against 32 cores of the same code):

| code | ranks x threads | cores | placement, NUMA nodes | wall s | read_mesh | dist_mesh | init_full | static solve | `solve_step` ms/step | check ms/step | energies ms/step | loop s | core-µs/el/step | loop efficiency |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 32 x 1 | 32 | fill, 0 | 1,806 | 2.52 | 11.4 | 1.76 | 448 | 115 | 142 | 10.2 | 1,285 | 1.87 | 100% |
| port | 64 x 1 | 64 | fill, 0 | 1,204 | 2.43 | 11.5 | 0.881 | 337 | 76.9 | 86.0 | 6.32 | 815 | 2.37 | 79% |
| port | 128 x 1 | 128 | fill, 0,1 | 1,174 | 2.46 | 12.1 | 0.544 | 319 | 74.9 | 83.8 | 8.41 | 793 | 4.62 | 40% |
| port | 64 x 2 | 128 | fill, 0,1 | 849 | 2.49 | 11.9 | 0.680 | 262 | 50.4 | 58.1 | 5.12 | 543 | 3.16 | 59% |
| port | 32 x 4 | 128 | fill, 0,1 | 896 | 2.47 | 11.2 | 1.29 | 276 | 54.2 | 60.8 | 5.24 | 575 | 3.35 | 56% |
| port | 16 x 8 | 128 | fill, 0,1 | 1,532 | 2.50 | 11.1 | 2.78 | 641 | 91.0 | 74.8 | 8.09 | 829 | 4.83 | 39% |
| C++ | 32 x 1 | 32 | fill, 0 | 4,297 | 15.0 | 66.3 | 16.7 | 381 | 199 | 533 | 30.0 | 3,659 | 5.33 | 100% |
| C++ | 64 x 1 | 64 | fill, 0 | 2,848 | 18.0 | 85.7 | 7.55 | 299 | 134 | 326 | 26.0 | 2,300 | 6.70 | 80% |
| C++ | 128 x 1 | 128 | fill, 0,1 | 2,298 | 15.7 | 86.5 | 4.16 | 239 | 87.7 | 246 | 55.5 | 1,671 | 9.74 | 55% |

Table "Phases" (rank 0's Chrono totals, s; per step = total / 5,000):

| code | ranks | wall s | read_mesh | dist_mesh | init_full | static solve | before_step | `solve_step` s (ms/step) | check s (ms/step) | energies s (ms/step) | loop s | core-µs/el/step |
|---|---|---|---|---|---|---|---|---|---|---|---|---|
| port | 128 | 1,174 | 2.46 | 12.1 | 0.544 | 319 | 0.468 | 374 (74.9) | 419 (83.8) | 42.1 (8.41) | 793 | 4.62 |
| C++ | 128 | 2,298 | 15.7 | 86.5 | 4.16 | 239 | 1.27 | 439 (87.7) | 1,232 (246) | 277 (55.5) | 1,671 | 9.74 |
| port | 64 | 1,204 | 2.43 | 11.5 | 0.881 | 337 | 0.890 | 385 (76.9) | 430 (86.0) | 31.6 (6.32) | 815 | 2.37 |
| port 64 x 2 | 64 | 849 | 2.49 | 11.9 | 0.680 | 262 | 0.606 | 252 (50.4) | 291 (58.1) | 25.6 (5.12) | 543 | 3.16 |
| C++ | 64 | 2,848 | 18.0 | 85.7 | 7.55 | 299 | 2.33 | 669 (134) | 1,631 (326) | 130 (26.0) | 2,300 | 6.70 |
| port | 32 | 1,806 | 2.52 | 11.4 | 1.76 | 448 | 1.56 | 574 (115) | 711 (142) | 51.2 (10.2) | 1,285 | 1.87 |
| port 32 x 4 | 32 | 896 | 2.47 | 11.2 | 1.29 | 276 | 1.15 | 271 (54.2) | 304 (60.8) | 26.2 (5.24) | 575 | 3.35 |
| C++ | 32 | 4,297 | 15.0 | 66.3 | 16.7 | 381 | 4.12 | 995 (199) | 2,664 (533) | 150 (30.0) | 3,659 | 5.33 |
| port 16 x 8 | 16 | 1,532 | 2.50 | 11.1 | 2.78 | 641 | 2.48 | 455 (91.0) | 374 (74.8) | 40.4 (8.09) | 829 | 4.83 |

Table "Port / upstream at the same rank count" (single runs):

| ranks | wall | `init_full` | static solve | `dist_mesh` | `solve_step` | check | energies | loop | largest rank peak | total peak |
|---|---|---|---|---|---|---|---|---|---|---|
| 128 | 0.511 | 0.131 | 1.34 | 0.140 | 0.853 | 0.340 | 0.152 | 0.475 | 0.988 | 1.01 |
| 64 | 0.423 | 0.117 | 1.13 | 0.134 | 0.575 | 0.264 | 0.243 | 0.354 | 1.03 | 0.929 |
| 32 | 0.420 | 0.106 | 1.18 | 0.172 | 0.577 | 0.267 | 0.341 | 0.351 | 1.24 | 0.993 |

Table "Memory" (GiB; sampler: VmHWM of every rank; container: the cgroup's `memory.peak`; static: the port's VmHWM at the end of the static solve):

| code | ranks | largest rank | total (sampler) | container | static: largest rank / total | per-rank peak min / median / max |
|---|---|---|---|---|---|---|
| port | 128 | 4.53 | 321 | 206 | 4.53 / 319 | 2.05 / 2.41 / 4.53 |
| C++ | 128 | 4.59 | 318 | 217 | — | 2.10 / 2.44 / 4.59 |
| port | 64 | 5.89 | 218 | 196 | 5.88 / 216 | 2.57 / 3.21 / 5.89 |
| port 64 x 2 | 64 | 6.44 | 197 | 176 | 6.38 / 192 | 2.33 / 2.91 / 6.44 |
| C++ | 64 | 5.70 | 235 | 213 | — | 2.74 / 3.48 / 5.70 |
| port | 32 | 10.9 | 187 | 183 | 10.8 / 183 | 4.39 / 5.58 / 10.9 |
| port 32 x 4 | 32 | 8.05 | 178 | 175 | 7.91 / 171 | 4.01 / 5.41 / 8.05 |
| C++ | 32 | 8.77 | 188 | 184 | — | 4.36 / 5.36 / 8.77 |
| port 16 x 8 | 16 | 17.6 | 171 | 170 | 17.2 / 164 | 7.37 / 10.3 / 17.6 |

Table "MUMPS" (the port's records; `INFOG` in MB, flops = `RINFOG(3)`):

| ranks | ordering `INFOG(7)` | factor entries `INFOG(29)` | allocated sum / largest rank `INFOG(19)` / `(18)` | used sum / largest `INFOG(22)` / `(21)` | elimination flops | retries | matrix entries sum / largest rank |
|---|---|---|---|---|---|---|---|
| 128 (port) | 4 | 8.14e+09 | 199,175 / 3,766 | 122,002 / 2,139 | 1.67e+14 | 0 | 54,624,147 / 435,060 |
| 64 (port) | 4 | 7.86e+09 | 182,002 / 5,639 | 107,667 / 3,988 | 1.52e+14 | 0 | 53,725,776 / 846,258 |
| 64 (port 64 x 2) | 4 | 7.55e+09 | 158,306 / 6,017 | 101,194 / 5,298 | 1.33e+14 | 0 | 53,725,776 / 846,258 |
| 32 (port) | 4 | 7.97e+09 | 174,331 / 11,183 | 106,523 / 7,547 | 1.58e+14 | 0 | 53,089,626 / 1,677,093 |
| 32 (port 32 x 4) | 4 | 7.51e+09 | 155,575 / 7,569 | 97,472 / 5,150 | 1.31e+14 | 0 | 53,089,626 / 1,677,093 |
| 16 (port 16 x 8) | 4 | 8.12e+09 | 152,678 / 16,879 | 101,682 / 13,533 | 1.66e+14 | 0 | 52,508,430 / 3,297,309 |

Reading (single runs; ratios from `multinode/table.md`):

- At the same rank count the port's run took 0.511, 0.423 and 0.420 of
  upstream's wall time at 128, 64 and 32 ranks, and its loop with the
  energies 0.429, 0.348 and 0.351 (835 s against 1,948 s, 846 s against
  2,430 s, 1,336 s against 3,809 s): `solve_step` 0.853 / 0.575 / 0.577,
  the check 0.340 / 0.264 / 0.267, the energies 0.152 / 0.243 / 0.341. The
  static solve took 1.34 / 1.13 / 1.18 times upstream's, `init_full`
  0.106–0.131 and the mesh distribution 0.134–0.172.
- The machine does not scale from 64 to 128 ranks: the port's loop goes
  from 846 s to 835 s, upstream's from 2,430 s to 1,948 s (1.25×). The
  128-rank runs add the second socket; per element and step the port's
  step and check cost 2.37 core-µs at 64 ranks and 4.62 at 128 (upstream
  6.70 and 9.74). Across hosts the same 128 ranks run faster in both codes
  (section "Multi-node runs"): the ratios at 128 ranks above are measured
  on a saturated machine.
- Ranks × threads on all 128 cores: 64 × 2 takes 568 s (loop with the
  energies), 32 × 4 601 s and 16 × 8 870 s, against 835 s for 128
  single-thread ranks; 64 × 2 and 32 × 4 peak at 176 and 175 GiB in the
  container against 206 GiB.
- Memory: container peaks 206 / 196 / 183 GiB for the port and 217 / 213 /
  184 GiB for upstream at 128 / 64 / 32 ranks; the largest rank 0.988 /
  1.03 / 1.24 times upstream's.
- The repetition: the first 128-rank sample of the port (correct; repeated
  because its binding record was incomplete, `G/README.md`) took 825.5 s
  in the loop, 1,116 s in all and 271 s in the static solve, against
  835.3 s, 1,174 s and 319 s in the table's run.
- The final revision (`4a8d9d38e`, with D-126) on a 128-core machine of the
  same description, 2026-11-18 19:18–19:37 UTC, 128 ranks, part G's
  command (`multinode/one-machine-final/`): loop 814 s, 0.975 of the
  table's 835 s and 0.986 of the first sample's 826 s; `solve_step` and the
  check within 1.3 %, the energies 29.9 s against 42.1 s (one reduction for
  the four energies).
- A profile of the port at 64 and 128 ranks on that machine (2,000 steps,
  system-wide cycles, `G/profile/`): the driver's own code 69.0 % and
  51.0 % of the cycles, the kernel 13.1 % and 21.0 %, libc 13.1 % and
  15.0 %, Open MPI 4.5 % and 12.4 %.

#### Before the final campaign (AWS `r7a.16xlarge`, 2026-11-12)

The JOSS mesh, 5,000 steps with insertion, on one AWS `r7a.16xlarge`
(2026-11-12). Both codes ran in one container per run (Open MPI 5.0.7, TCP
and shared memory, no binding), memory limit 480 GiB without swap, CPU
quota 64 for the port's container. C++: the harness against the strict
install (binary `5b85f310…`), JOSS emulation, `_auto` = MUMPS 5.7.3. Port:
`cohesive_extrinsic` release with `mpi,scotch,mumps` (binary `9d2756e8…`).
Same partition at each rank count. One run per point. The C++ run at 8
ranks was skipped by the session's time limit (2.35 h left against an estimate of 2.49 h). Every table here is printed
by `python3 -I crates/akantu-bench/scripts/c1_report.py tables
docs/benchmarks/c1-2026-11-12` from the committed files alone.

Table "Phases" (rank 0's Chrono totals, s; per step = total / 5,000):

| code | ranks | wall s | read_mesh | dist_mesh | init_full | static solve | before_step | `solve_step` s (ms/step) | check s (ms/step) | energies s (ms/step) | loop s | core-µs/el/step |
|---|---:|---:|---:|---:|---:|---:|---:|---|---|---|---:|---:|
| Rust | 64 | 2,387 | 5.48 | 151 | 1.91 | 305 | 2.93 | 541 (108) | 1,274 (255) | 101 (20.2) | 1,916 | 5.29 |
| C++ | 64 | 2,729 | 15.6 | 80.9 | 7.42 | 228 | 2.3 | 659 (132) | 1,591 (318) | 140 (28) | 2,390 | 6.56 |
| Rust | 32 | 3,221 | 5.67 | 83.5 | 3.12 | 356 | 4.71 | 561 (112) | 2,088 (418) | 114 (22.9) | 2,764 | 3.86 |
| C++ | 32 | 3,787 | 15.3 | 66.4 | 17.3 | 353 | 4.17 | 894 (179) | 2,296 (459) | 137 (27.3) | 3,326 | 4.65 |
| Rust | 8 | 10,967 | 5.41 | 32.1 | 12.8 | 831 | 16.4 | 1,244 (249) | 8,278 (1,656) | 543 (109) | 10,065 | 3.47 |

Table "Port / upstream at the same rank count" (ratio of the single runs;
the static solve of the 20-step test runs in brackets, same binaries and
partition):

| ranks | wall | `init_full` | static solve | `dist_mesh` | `solve_step` | check | energies | loop | largest rank peak | total peak |
|---:|---:|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 64 | 0.875 | 0.26 | 1.34 (0.91) | 1.86 | 0.82 | 0.80 | 0.72 | 0.80 | 1.27 | 1.07 |
| 32 | 0.851 | 0.18 | 1.01 | 1.26 | 0.63 | 0.91 | 0.84 | 0.83 | 0.93 | 0.96 |

Strong scaling (single runs): the port's loop gained 3.64× from 8 to 32
ranks (91 %) and 5.25× to 64 (66 %), its wall 3.40× (85 %) and 4.59× (57 %);
from 32 to 64 ranks the port's loop gained 1.44× (72 %), upstream's 1.39×
(70 %). The static solve scaled 2.73× from 8 to 64 ranks (34 %).

Table "Pre-insertion costs at 64 ranks" (the 20-step test runs, no insertion
within them, against the 5,000-step averages):

| code | static solve s: test / timed | `solve_step` ms/step: test / timed | check ms/step: test / timed | core-µs/el/step: test / timed |
|---|---|---|---|---|
| Rust | 295 / 305 | 78.0 / 108.2 | 122 / 255 | 2.91 / 5.29 |
| C++ | 324 / 228 | 92.3 / 131.8 | 229 / 318 | 4.69 / 6.56 |

A rank holds 67,944–69,215 tetrahedra and 11,492–21,310 ghosts at 64 ranks,
135,978–138,627 and 17,571–33,714 at 32, 546,628–551,256 and 42,348–46,693
at 8.

Reading:

- At 64 and 32 ranks the port's loop took 0.80× and 0.83× upstream's (step
  0.82× and 0.63×, check 0.80× and 0.91×, energies 0.72× and 0.84×),
  `init_full` 0.26× and 0.18×.
- The static solve: both codes hand the same matrix (53.7 M entries summed
  over the ranks at 64) to the same MUMPS. Upstream's own static solve took
  324 s in the 20-step test and 228 s in the timed run (same binary, input
  and partition: ×1.42 between two runs), the port's 295 and 305 s. The
  timed pair's 1.34 lies inside one code's run-to-run variation, the test
  pair gives 0.91, and at 32 ranks the codes agree within 1 %. No
  static-solve gap is established at full size; the paired boat series put
  the phase at about 1.1×, which these single runs can neither confirm nor
  exclude.
- The insertion check is the largest cost in both codes: 75 % of the port's
  wall at 8 ranks (1,656 ms per step), 65 % at 32, 53 % at 64 (upstream 61 %
  and 58 %). It costs 3.02, 3.04 and 3.71 core-µs per element and step at 8,
  32 and 64 ranks and scales well (99 % from 8 to 32 ranks, 81 % to 64).
  With insertion it costs about twice its pre-insertion cost (64 ranks: 3.71
  against about 1.8 core-µs in the 20-step test).
- The explicit step scales worst: 249, 112 and 108 ms per step at 8, 32 and
  64 ranks (0.45, 0.82 and 1.58 core-µs; upstream 179 and 132 ms at 32 and
  64). The ghost layer grows from 8 % of a rank's tetrahedra at 8 ranks to
  24 % at 64, the explicit step assembles the ghost elements' contributions
  too, and at 64 ranks a rank holds 0 to 19,833 cohesive elements (median
  7,658) while every step waits for the slowest rank at the ghost exchange.
  There are no per-rank timings, so these facts fit the data but are not a
  proven cause. Before any insertion the 64-rank step already costs
  1.14 core-µs per element (78 ms per step), against 0.65 at 8 ranks with
  the same elements per rank on boat: the parallel overhead is there without
  insertion.
- The mesh distribution from rank 0: the port's grows by about 2.1 s per
  added rank (32, 84 and 151 s at 8, 32 and 64 ranks), upstream's from 66 to
  81 s (32 to 64 ranks). The 20-step test gives 148 s for the port and 79 s
  for upstream at 64 ranks.
- Shares of the wall time: the static solve 8–13 %, the mesh distribution
  up to 6 %, the per-step energies 4–5 %.

Counts and energies at step 5,000 are in `c1_report.py`'s output and the
run summaries. They are not a correctness result: the
port's runs of this day used upstream's placement of the accumulated load of
doubled nodes, which depends on the partition (corrected later, UD-1280),
and upstream's distributed runs leave the serial problem (UD-890).

The port's rerun after UD-1280 (2026-11-13, 64 and 32 ranks, same host type
and partitions, `rust_doubled_node_load = follows_elements`;
`docs/benchmarks/c1-rerun-2026-11-13/`) took 2,415 s and 3,107 s; its phases
are in the multi-node table below, its correctness results in
`docs/NUMERICAL_VALIDATION.md` and `docs/MPI_VALIDATION.md`. Its numbers are
printed by `results_report.py` (output `docs/benchmarks/results-2026-11-13.txt`,
checked by `crates/akantu-bench/scripts/tests/test_results_report.py`).

An estimate made beforehand from the boat runs (matched work per rank)
put the loop's cost per element and step at 0.35–0.58 of what was measured;
at 64 ranks the miss splits into ×1.70 before insertion and ×1.10 for the
insertion. One rank on a sandbox does not show the parallel overhead of a
64-rank run.
#### Comparison with upstream's published data

Upstream's JOSS study ran the same mesh and the same 5,000-step schedule
with insertion (v5.0.4 Release with the JOSS patch, five runs per core
count, seven from 288 cores) on EPFL's jed cluster (Intel Xeon 8360Y, two
36-core sockets per node). Its timing files are committed verbatim in
`benchmarks/cohesive-perf/results/published-jed/`; the medians per core
count are in `docs/manual/src/appendix/joss.md`.

Final campaign (part G and the multi-host runs; `multinode/table.md`):

| run | runs | step + check s | core-µs/el/step |
|---|---|---|---|
| jed, 9 cores | 5 | 9,445.2 [9,353.2, 9,839.5] | 3.87 |
| jed, 18 cores | 5 | 4,871.9 [4,808.0, 5,009.9] | 3.99 |
| jed, 36 cores | 5 | 2,686.8 [2,632.6, 2,739.0] | 4.40 |
| jed, 72 cores | 5 | 1,829.2 [1,748.2, 1,868.0] | 6.00 |
| jed, 144 cores | 5 | 888.1 [880.9, 1,008.3] | 5.82 |
| jed, 288 cores | 7 | 953.2 [515.8, 1,213.4] | 12.50 |
| jed, 432 cores | 7 | 1,095.4 [946.5, 1,162.7] | 21.55 |
| jed, 576 cores | 7 | 908.4 [509.8, 1,054.2] | 23.83 |
| jed, 648 cores | 7 | 940.0 [873.6, 1,086.8] | 27.74 |
| jed, 720 cores | 7 | 988.7 [887.4, 1,096.7] | 32.41 |
| port 740a79a36, 32, one 128-core machine (`G/full-hjoss-np32-r1-head`) | 1 | 1,285.0 | 1.87 |
| port 740a79a36, 64, one 128-core machine (`G/full-hjoss-np64-r1-head`) | 1 | 814.6 | 2.37 |
| port 740a79a36, 128, one 128-core machine (`G/full-hjoss-np128-r1-head`) | 1 | 793.2 | 4.62 |
| port 4a8d9d38e, 128, one 128-core machine (`one-machine-final/run`) | 1 | 784.3 | 4.57 |
| port 740a79a36, 64, 2 hosts x 32 (`s1-port/joss64x2-a1`) | 1 | 953.2 | 2.78 |
| port 4a8d9d38e, 64, 2 hosts x 32 (`s3-final64/joss64x2-a1`) | 1 | 698.2 | 2.03 |
| port 740a79a36, 128, 4 hosts x 32 (`s1-port/joss128x4-a1`) | 1 | 1,320.8 | 7.70 |
| port 6cdb1b601, 128, 4 hosts x 32 (`s2-after/joss128x4-a1`) | 1 | 458.1 | 2.67 |
| port 4a8d9d38e, 128, 4 hosts x 32 (`s3-final128/joss128x4-a1`) | 1 | 510.9 | 2.98 |
| C++, 32, one 128-core machine (`G/full-hjoss-np32-r1-cpp`) | 1 | 3,658.9 | 5.33 |
| C++, 64, one 128-core machine (`G/full-hjoss-np64-r1-cpp`) | 1 | 2,299.9 | 6.70 |
| C++, 128, one 128-core machine (`G/full-hjoss-np128-r1-cpp`) | 1 | 1,670.9 | 9.74 |
| C++, 64, 2 hosts x 32 (`s1-cpp64/joss64x2-a1`) | 1 | 1,785.2 | 5.20 |
| C++, 128, 4 hosts x 32 (`s1-cpp/joss128x4-a1`) | 1 | 935.9 | 5.45 |
| C++, 128, 4 hosts x 32 (`s2-cpp/joss128x4-a1`) | 1 | 923.5 | 5.38 |

- jed's step + check falls to 888 s at 144 cores (two nodes) and stays
  between 908 and 1,095 s (medians) from 288 to 720 cores: the reference
  build of the paper stops scaling from 144 cores on.
- The port's step + check at 128 ranks: 793 s (part G) and 784 s (final
  revision) on one machine, 511 s across four hosts at the final revision
  (458 s at `6cdb1b601`); upstream `35a85178` 1,671 s on one machine and
  924–936 s across four hosts. Per element and step: 4.57 core-µs for the
  final revision on one machine and 2.98 across four hosts, 9.74 and
  5.38–5.45 for upstream, 5.82 for jed at 144 cores.
- What cannot be compared, as below: absolute times of another processor
  (Ice Lake against Genoa), another interconnect, another build (v5.0.4
  Release) and another year; medians of five to seven runs on jed against
  single runs here; upstream's distributed runs leave the serial problem
  in both studies (UD-890). The one 128-core machine is saturated at 128
  ranks (above), and the four-host runs measure the hosts' memory systems
  as much as the network ("Multi-node runs").

Before the final campaign (the runs of 12 November; jed: medians of the
v5.0.4 cohesive file at the three core counts nearest to the full-size
points):

|  | jed, v5.0.4 (9 / 36 / 72 cores) | port (8 / 32 / 64 ranks) | upstream `35a85178` (— / 32 / 64 ranks) |
|---|---|---|---|
| step + check, core-µs/el/step | 3.87 / 4.40 / 6.00 | 3.47 / 3.86 / 5.29 | — / 4.65 / 6.56 |
| `solve_step` alone | 0.96 / 1.06 / 1.59 | 0.45 / 0.82 / 1.58 | — / 1.30 / 1.92 |
| check alone | 2.93 / 3.35 / 4.46 | 3.02 / 3.04 / 3.71 | — / 3.35 / 4.64 |
| loop efficiency, ×4 ranks (9 → 36; 8 → 32) | 88 % | 90 % | — |
| loop efficiency, ×2 ranks (36 → 72; 32 → 64) | 73 % | 73 % | 71 % |
| loop efficiency, ×8 ranks (9 → 72; 8 → 64) | 65 % | 66 % | — |
| static solve s | 309 / 130 / 108 | 831 / 356 / 305 | — / 353 / 228 |
| `init_full` s | 68.4 / 10.1 / 4.5 | 12.8 / 3.12 / 1.91 | — / 17.3 / 7.42 |
| `dist_mesh` s | 67 / 88 / 80 | 32 / 84 / 151 | — / 66 / 81 |

What can be compared:

- The shape of the loop's strong scaling: over the same rank factors the
  efficiency is 90 / 73 / 66 % for the port and 71 % for upstream from 32 to
  64 ranks (one run per point), against 88 / 73 / 65 % on jed (medians of
  five runs).
- The cost per element and step on the same mesh, as an order of magnitude:
  the AWS values lie within −12 % to +9 % of jed's at the neighbouring core
  count (port 0.88–0.90×, upstream 1.06–1.09×).

What cannot be compared: absolute times as a statement about a code or a
machine. The processors (Ice Lake, 2021, against Genoa, 2023), a dedicated
two-socket node against 64 cores of a shared cloud host, the builds, the
rank counts and the number of runs all differ, and in both studies upstream's
distributed runs depart from the serial problem (UD-890). Whether a jed run
took upstream's MUMPS retry path (UD-216) cannot be checked from the
published files. The static solve
took 2.1–2.8× jed's at the neighbouring core counts; jed's MUMPS ordering
and BLAS are not recorded (here PORD with OpenBLAS). jed's points from 144
cores on (several nodes) were not run before the final campaign (above:
128 ranks on four hosts); the paper's elastic series at full size was not
run.

### Multi-node runs

#### Final campaign: four and two 32-core hosts (2026-11-18)

The JOSS mesh, 5,000 steps with insertion, at 128 ranks on four hosts and
64 ranks on two, both codes, the port before and after the change of its
collective error protocol (D-126). Every host an AWS `m7a.8xlarge` (AMD
EPYC 9R14, 32 cores, one thread per core, one NUMA node, 123 GiB visible);
the hosts of a session in one cluster placement group; Open MPI 5.0.7 over
TCP between the hosts; one container per host, 32 ranks per host, each
bound to its own core, rank 0 on the first host (`mn_cluster.py`, ssh
backend, `bind = self`). Both codes replay part G's partitions; the
drivers' default settings; MUMPS static solve; C++ as in part G. One run
per configuration, three sessions on three sets of hosts (UTC):

- session 1 (04:12–06:23): the port at `740a79a36` (part G's driver
  binary) at 128 ranks on four hosts and 64 on two, then C++ at the same;
- session 2 (14:10–15:25), one run after the other on the same four hosts:
  the port after the first round of D-126 (`6cdb1b601`), the port at
  `740a79a36`, C++;
- session 3 (18:23–18:56): the port at the final revision `4a8d9d38e` at
  128 ranks on four hosts and 64 on two;
- and the final revision on one 128-core machine (19:18–19:37), part G's
  128-rank point.

Data, sessions, binaries and commands:
`docs/benchmarks/perf-campaign/final/multinode/` (`README.md`); every
table here is printed by `python3 -I table.py` there (`table.md`) from the
committed run records and part G's `runs.jsonl`. Times are rank 0's Chrono
totals in seconds; init = read_mesh + dist_mesh + `init_full` + initial
conditions + static solve + before_step; the loop is `solve_step` + check +
energies + boundary conditions (the driver's `loop_time_s`, as in the
tables of 13 November below).

Table "Loop by configuration" (s):

| ranks, machines | port 740a79a36 | port 6cdb1b601 | port 4a8d9d38e | C++ |
|---|---|---|---|---|
| 128, one 128-core machine | 835.3 | — | 814.3 | 1,948.3 |
| 128, four 32-core hosts (session 1) | 1,537.7 | — | — | 1,040.2 |
| 128, four 32-core hosts (session 2) | 1,535.8 | 484.7 | — | 1,019.3 |
| 128, four 32-core hosts (session 3) | — | — | 526.4 | — |
| 64, one 128-core machine | 846.3 | — | — | 2,430.0 |
| 64, two 32-core hosts (sessions 1, 3) | 1,046.4 | — | 722.3 | 1,923.7 |
| 32, one 128-core machine | 1,336.2 | — | — | 3,809.0 |

Table "Every run" (s; peak memory: one machine, the container's cgroup
peak; across hosts, the largest sampled sum of the ranks' resident memory
on one host / the sum of those peaks over the hosts):

| run | session | code | ranks, placement | wall | init | dist_mesh | static solve | solve_step | check | energies | step + check | loop | cohesive elements | peak GiB |
|---|---|---|---|---|---|---|---|---|---|---|---|---|---|---|
| G/full-hjoss-np128-r1-cpp | G | C++ | 128, one 128-core machine | 2,298 | 346.3 | 86.5 | 238.7 | 438.7 | 1,232.2 | 277.3 | 1,670.9 | 1,948.3 | 488,968 | 217.2 (container) |
| G/full-hjoss-np128-r1-head | G | port 740a79a36 | 128, one 128-core machine | 1,174 | 335.1 | 12.1 | 319.5 | 374.4 | 418.8 | 42.1 | 793.2 | 835.3 | 475,470 | 206.4 (container) |
| G/full-hjoss-np16-r1-head-t8 | G | port 740a79a36 | 16 x 8 threads, one 128-core machine | 1,532 | 659.6 | 11.1 | 640.8 | 455.0 | 374.2 | 40.4 | 829.2 | 869.7 | 475,781 | 170.0 (container) |
| G/full-hjoss-np32-r1-cpp | G | C++ | 32, one 128-core machine | 4,297 | 483.1 | 66.3 | 381.1 | 994.5 | 2,664.4 | 150.1 | 3,658.9 | 3,809.0 | 482,745 | 184.0 (container) |
| G/full-hjoss-np32-r1-head | G | port 740a79a36 | 32, one 128-core machine | 1,806 | 465.4 | 11.4 | 448.2 | 574.1 | 710.9 | 51.2 | 1,285.0 | 1,336.2 | 476,040 | 182.9 (container) |
| G/full-hjoss-np32-r1-head-t4 | G | port 740a79a36 | 32 x 4 threads, one 128-core machine | 896 | 291.6 | 11.2 | 275.5 | 270.9 | 304.0 | 26.2 | 575.0 | 601.2 | 475,980 | 174.9 (container) |
| G/full-hjoss-np64-r1-cpp | G | C++ | 64, one 128-core machine | 2,848 | 412.5 | 85.7 | 299.0 | 668.6 | 1,631.3 | 130.0 | 2,299.9 | 2,430.0 | 485,698 | 213.0 (container) |
| G/full-hjoss-np64-r1-head | G | port 740a79a36 | 64, one 128-core machine | 1,204 | 352.6 | 11.5 | 336.9 | 384.6 | 430.0 | 31.6 | 814.6 | 846.3 | 476,352 | 196.1 (container) |
| G/full-hjoss-np64-r1-head-t2 | G | port 740a79a36 | 64 x 2 threads, one 128-core machine | 849 | 277.8 | 11.9 | 262.1 | 251.9 | 290.6 | 25.6 | 542.5 | 568.1 | 476,315 | 176.2 (container) |
| G/first-128head | G, first sample | port 740a79a36 | 128, one 128-core machine | 1,116 | 286.6 | 12.1 | 271.1 | 368.2 | 415.2 | 42.2 | 783.3 | 825.5 | 475,439 | 195.8 (container) |
| one-machine-final/run | 4 | port 4a8d9d38e | 128, one 128-core machine | 1,098 | 280.0 | 12.0 | 264.6 | 369.5 | 414.8 | 29.9 | 784.3 | 814.3 | 475,439 | 200.5 (container) |
| s1-port/joss128x4-a1 | 1 | port 740a79a36 | 128, 4 hosts x 32 | 1,854 | 314.6 | 11.5 | 300.0 | 484.0 | 836.8 | 216.9 | 1,320.8 | 1,537.7 | 475,478 | 55.8 / 205.8 (hosts) |
| s1-port/joss64x2-a1 | 1 | port 740a79a36 | 64, 2 hosts x 32 | 1,352 | 303.0 | 11.1 | 288.2 | 404.3 | 548.9 | 93.1 | 953.2 | 1,046.4 | 476,207 | 108.9 / 213.0 (hosts) |
| s1-cpp/joss128x4-a1 | 1 | C++ | 128, 4 hosts x 32 | 1,429 | 387.1 | 72.3 | 298.2 | 259.3 | 676.6 | 104.3 | 935.9 | 1,040.2 | 489,002 | 61.2 / 216.2 (hosts) |
| s1-cpp64/joss64x2-a1 | 1 | C++ | 64, 2 hosts x 32 | 2,317 | 390.5 | 67.6 | 301.3 | 516.2 | 1,269.0 | 138.5 | 1,785.2 | 1,923.7 | 485,671 | 110.3 / 216.0 (hosts) |
| s2-after/joss128x4-a1 | 2 | port 6cdb1b601 | 128, 4 hosts x 32 | 875 | 388.0 | 11.4 | 373.6 | 179.3 | 278.9 | 26.5 | 458.1 | 484.7 | 475,352 | 62.0 / 203.6 (hosts) |
| s2-before/joss128x4-a1 | 2 | port 740a79a36 | 128, 4 hosts x 32 | 1,977 | 439.1 | 11.7 | 424.2 | 479.7 | 839.6 | 216.5 | 1,319.3 | 1,535.8 | 475,467 | 56.6 / 202.0 (hosts) |
| s2-cpp/joss128x4-a1 | 2 | C++ | 128, 4 hosts x 32 | 1,467 | 445.5 | 71.0 | 358.2 | 257.3 | 666.2 | 95.8 | 923.5 | 1,019.3 | 488,998 | 64.2 / 233.9 (hosts) |
| s3-final128/joss128x4-a1 | 3 | port 4a8d9d38e | 128, 4 hosts x 32 | 877 | 348.5 | 11.3 | 334.2 | 221.9 | 289.1 | 15.4 | 510.9 | 526.4 | 475,506 | 62.3 / 201.5 (hosts) |
| s3-final64/joss64x2-a1 | 3 | port 4a8d9d38e | 64, 2 hosts x 32 | 994 | 268.8 | 11.1 | 254.0 | 323.0 | 375.2 | 24.0 | 698.2 | 722.3 | 476,327 | 103.1 / 194.5 (hosts) |

Table "Port / upstream, same configuration":

| ratio (single runs) | wall | init | static solve | solve_step | check | energies | step + check | loop |
|---|---|---|---|---|---|---|---|---|
| 128, one machine: port 740a79a36 / C++ | 0.511 | 0.967 | 1.338 | 0.853 | 0.340 | 0.152 | 0.475 | 0.429 |
| 128, one machine: port 4a8d9d38e / C++ (other session) | 0.478 | 0.808 | 1.108 | 0.842 | 0.337 | 0.108 | 0.469 | 0.418 |
| 64, one machine: port 740a79a36 / C++ | 0.423 | 0.855 | 1.127 | 0.575 | 0.264 | 0.243 | 0.354 | 0.348 |
| 32, one machine: port 740a79a36 / C++ | 0.420 | 0.963 | 1.176 | 0.577 | 0.267 | 0.341 | 0.351 | 0.351 |
| 128, four hosts, session 1: port 740a79a36 / C++ | 1.298 | 0.813 | 1.006 | 1.866 | 1.237 | 2.079 | 1.411 | 1.478 |
| 128, four hosts, session 2: port 740a79a36 / C++ | 1.348 | 0.986 | 1.184 | 1.864 | 1.260 | 2.259 | 1.429 | 1.507 |
| 128, four hosts, session 2: port 6cdb1b601 / C++ | 0.596 | 0.871 | 1.043 | 0.697 | 0.419 | 0.277 | 0.496 | 0.476 |
| 128, four hosts: port 4a8d9d38e (session 3) / C++ (session 2) | 0.598 | 0.782 | 0.933 | 0.862 | 0.434 | 0.161 | 0.553 | 0.516 |
| 128, four hosts: port 4a8d9d38e (session 3) / C++ (session 1) | 0.614 | 0.900 | 1.121 | 0.856 | 0.427 | 0.148 | 0.546 | 0.506 |
| 64, two hosts, session 1: port 740a79a36 / C++ | 0.584 | 0.776 | 0.956 | 0.783 | 0.433 | 0.673 | 0.534 | 0.544 |
| 64, two hosts: port 4a8d9d38e (session 3) / C++ (session 1) | 0.429 | 0.688 | 0.843 | 0.626 | 0.296 | 0.174 | 0.391 | 0.375 |

Table "Across hosts / one machine, same code and binary":

| ratio (single runs) | wall | init | static solve | solve_step | check | energies | step + check | loop |
|---|---|---|---|---|---|---|---|---|
| port 740a79a36, 128: four hosts (session 1) / one machine | 1.579 | 0.939 | 0.939 | 1.293 | 1.998 | 5.157 | 1.665 | 1.841 |
| port 740a79a36, 128: four hosts (session 2) / one machine | 1.683 | 1.310 | 1.328 | 1.281 | 2.005 | 5.146 | 1.663 | 1.839 |
| port 740a79a36, 64: two hosts / one machine | 1.123 | 0.859 | 0.855 | 1.051 | 1.276 | 2.945 | 1.170 | 1.236 |
| port 4a8d9d38e, 128: four hosts (session 3) / one machine | 0.799 | 1.245 | 1.263 | 0.601 | 0.697 | 0.515 | 0.651 | 0.646 |
| C++, 128: four hosts (session 1) / one machine | 0.622 | 1.118 | 1.249 | 0.591 | 0.549 | 0.376 | 0.560 | 0.534 |
| C++, 128: four hosts (session 2) / one machine | 0.638 | 1.286 | 1.500 | 0.587 | 0.541 | 0.346 | 0.553 | 0.523 |
| C++, 64: two hosts / one machine | 0.814 | 0.947 | 1.008 | 0.772 | 0.778 | 1.065 | 0.776 | 0.792 |

Table "The collective checks at commit points (D-126)":

| ratio (single runs) | wall | init | static solve | solve_step | check | energies | step + check | loop |
|---|---|---|---|---|---|---|---|---|
| 128, four hosts, session 2: port 6cdb1b601 / port 740a79a36 | 0.442 | 0.884 | 0.881 | 0.374 | 0.332 | 0.123 | 0.347 | 0.316 |
| 128, four hosts: port 4a8d9d38e (session 3) / port 740a79a36 (session 2) | 0.444 | 0.794 | 0.788 | 0.463 | 0.344 | 0.071 | 0.387 | 0.343 |
| 64, two hosts: port 4a8d9d38e (session 3) / port 740a79a36 (session 1) | 0.735 | 0.887 | 0.881 | 0.799 | 0.684 | 0.258 | 0.732 | 0.690 |
| 128, one machine: port 4a8d9d38e / port 740a79a36 | 0.935 | 0.836 | 0.828 | 0.987 | 0.991 | 0.711 | 0.989 | 0.975 |
| 128, one machine: port 4a8d9d38e / port 740a79a36, first sample | 0.984 | 0.977 | 0.976 | 1.004 | 0.999 | 0.710 | 1.001 | 0.986 |

Table "Two sessions on different hosts of the same type":

| ratio (single runs) | wall | init | static solve | solve_step | check | energies | step + check | loop |
|---|---|---|---|---|---|---|---|---|
| port 740a79a36: session 2 / session 1 | 1.066 | 1.396 | 1.414 | 0.991 | 1.003 | 0.998 | 0.999 | 0.999 |
| C++: session 2 / session 1 | 1.026 | 1.151 | 1.201 | 0.992 | 0.985 | 0.918 | 0.987 | 0.980 |

Reading:

- Before D-126 the port lost across hosts. Its loop took 1,538 s
  (session 1) and 1,536 s (session 2) at 128 ranks on four hosts, 1.84
  times its 835 s on one machine, while upstream's fell from 1,948 s to
  1,040 s and 1,019 s: the port took 1.48 and 1.51 times upstream's loop.
  Across four hosts the port's energies took 5.2 times and its check 2.0
  times their one-machine time. At 64 ranks on two hosts the port took
  1,046 s (1.24 times its one-machine loop), upstream 1,924 s (0.79).
- The cause, measured on a small four-host cluster
  (`docs/benchmarks/perf-campaign/net/`): the port's collective error
  protocol agreed after every phase, 51.6 collectives per step and rank
  against upstream's 8.4 (8 ranks, h = 0.06), each with a linear
  all-gather of the protocol header; across hosts a collective costs about
  one round trip, so their number decided. D-126 agrees at commit points
  and before exchanges instead: 20.5 collectives per step and rank, the
  same results (bitwise, Eigen static solve), and after a failure the same
  persistent state on every rank as the every-phase mode (section "MPI
  layer").
- After D-126: in session 2 the first round of the change (`6cdb1b601`)
  took 485 s, 0.316 of the port before the change on the same hosts and
  0.476 of upstream. The final revision (session 3, other hosts) took
  526 s on four hosts, 0.516 of upstream's loop of session 2 and 0.506 of
  session 1, and 722 s on two hosts, 0.375 of upstream's 1,924 s. Across
  four hosts its loop is 0.646 of its loop on one machine (814 s);
  upstream's is 0.523–0.534 of its own.
- The final revision against the first round: `solve_step` 222 s against
  179 s (the fix round's commit points before every persistent change,
  D-126), energies 15 s against 27 s (one reduction for the four
  energies); one run each on different hosts.
- Repeatability: the same binary on two sets of hosts of the same type gave
  loops within 0.1 % (port at `740a79a36`) and 2.0 % (C++); the static
  solve and the set-up varied more (port 1.41 and 1.40 times between the
  sessions).
- What the comparison of one machine with four hosts measures: more than
  the network. On one machine 128 ranks share two sockets' memory system,
  on four hosts 32 ranks share each host's; upstream's loop gains 1.87
  times from one machine to four hosts at 128 ranks, and its serial phases
  on rank 0 run faster on the 32-core hosts too (read_mesh 0.80,
  dist_mesh 0.84, `init_full` 0.66 of the one-machine times,
  `multinode/compare.txt`). Ratios of the two codes on the same machines
  and in the same session are the comparable figures.
- Correctness (`multinode/compare.txt`): every multi-host run that has a
  one-machine run of the same binary and rank count passes
  `mn_compare.py` (placement, partition, distribution, static matrix,
  MUMPS settings, and for C++ the driver, input, binding and phase
  repetitions); their energies and insertions differ from the first
  insertions on, as two runs with MUMPS's parallel factorisation do on one
  machine, and the cohesive element counts by at most 0.03 %.

#### Before the final campaign (8 × AWS `r7a.2xlarge`, 2026-11-13)

The port only, on 8 AWS `r7a.2xlarge` in one cluster placement group
(2026-11-13): one MPI job across the hosts (`mn_cluster.py`, SSH backend),
one container per host limited to 56 GiB and cores 0–7, every rank bound to
a core, ranks in consecutive blocks per host, rank 0 on the head host. Binary
`b321054d…` with `rust_cohesive_compat = upstream` and
`rust_doubled_node_load = follows_elements`, MUMPS static solve; 1,500 steps
for the scaling runs (first insertion at steps 678–746), 5,000 for the JOSS
runs. One run per point. Every number is printed by `python3 -I
crates/akantu-bench/scripts/results_report.py smoke multinode` from
`docs/benchmarks/mn-2026-11-13/` and `c1-rerun-2026-11-13/`. "init" is
read_mesh + dist_mesh + `init_full` + initial conditions + static solve +
before_step; the loop adds the boundary-condition increments and the
per-step energies to the step and the check.

Before the timed runs, a smoke session checked the launcher: with the Eigen
static solve, the 8-rank run on 8 hosts (one rank per host) wrote
`energies.csv` and `insertions.csv` byte-identical to the same run on one
host (1,500 steps, 416 cohesive elements on h = 0.12). With MUMPS the two
insert the same elements and their energies differ by at most 1.0e-14 of the
step's largest energy.

Table "The JOSS mesh across eight hosts against one node" (same binary and
partition; one host = the `r7a.16xlarge` rerun runs; s):

| ranks | hosts | wall s | init s | dist_mesh | static solve | `solve_step` | check | energies | loop s |
|---:|---|---:|---:|---:|---:|---:|---:|---:|---:|
| 64 | 1 | 2,415 | 484 | 149.4 | 324.5 | 553.4 | 1,271.0 | 101.5 | 1,926 |
| 64 | 8 (8 per host) | 2,679 | 467 | 196.9 | 258.9 | 565.7 | 1,455.8 | 188.9 | 2,210 |
| 64 | ratio 8 / 1 | 1.11 | 0.96 | 1.32 | 0.80 | 1.02 | 1.15 | 1.86 | 1.15 |
| 32 | 1 | 3,107 | 434 | 83.5 | 337.6 | 549.4 | 2,011.0 | 108.4 | 2,669 |
| 32 | 8 (4 per host) | 3,072 | 380 | 82.1 | 285.8 | 512.6 | 2,038.7 | 138.9 | 2,690 |
| 32 | ratio 8 / 1 | 0.99 | 0.88 | 0.98 | 0.85 | 0.93 | 1.01 | 1.28 | 1.01 |

At 64 ranks, eight per host, the loop took 15 % longer across the eight hosts
(the check 1.15×, the step 1.02×), the per-step energies (global sums every
step) 1.86×, the mesh distribution 1.32×, the whole run 1.11×. At 32 ranks,
four per host, the loop and the wall are within 1 % of the single node's.
Both pairs ran on the same partition file; for the 64-rank pair the
comparator also found the per-rank counts, the static matrix and MUMPS's
set-up identical, for the 32-rank pair the per-rank summaries are not kept
and that check was not made.
The static solve was shorter on the eight hosts in both pairs, while two
single-node solves of the same binary on the same partition already differ
by 8 %. The machines (eight 8-core instances against one 64-core instance)
and the binding also differ, so these ratios measure more than the network.

Table "h = 0.04, the same 8 ranks on 1, 2, 4 and 8 hosts" (one partition;
2,402 cohesive elements in every run):

| hosts | ranks per host | wall s | init s | static solve | `solve_step` | check | energies | loop s | loop / 1 host | `solve_step` / 1 host | check / 1 host | static / 1 host | peak GiB per host |
|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| 1 | 8 | 266 | 31 | 21.1 | 64.5 | 153.3 | 17.2 | 235 | 1.00 | 1.00 | 1.00 | 1.00 | 12.9 |
| 2 | 4 | 205 | 25 | 18.9 | 42.9 | 119.9 | 16.7 | 180 | 0.76 | 0.67 | 0.78 | 0.90 | 6.0 |
| 4 | 2 | 178 | 24 | 18.7 | 34.7 | 103.1 | 15.8 | 154 | 0.65 | 0.54 | 0.67 | 0.89 | 3.8 |
| 8 | 1 | 169 | 23 | 17.5 | 31.5 | 100.6 | 14.1 | 146 | 0.62 | 0.49 | 0.66 | 0.83 | 2.3 |

Spreading the same ranks over more hosts made the run shorter: on eight
hosts the loop took 0.62× the one-host time (step 0.49×, check 0.66×). On one
host the eight ranks share one instance's memory system and caches; which
resource sets the one-host time is not measured.

Table "h = 0.04, strong scaling" (8 ranks per host; S against 8 ranks,
efficiency = S × 8 / P):

| ranks | hosts | tetrahedra per rank | loop s | loop S (eff.) | `solve_step` S (eff.) | check S (eff.) | static solve S (eff.) | wall S (eff.) | `dist_mesh` s | `init_full` s |
|---:|---:|---:|---:|---|---|---|---|---|---:|---:|
| 8 | 1 | 69,859 | 235.0 | 1.00 (100 %) | 1.00 (100 %) | 1.00 (100 %) | 1.00 (100 %) | 1.00 (100 %) | 6.0 | 1.45 |
| 16 | 2 | 34,929 | 150.6 | 1.56 (78 %) | 1.40 (70 %) | 1.71 (86 %) | 1.81 (91 %) | 1.55 (77 %) | 7.5 | 0.77 |
| 32 | 4 | 17,464 | 130.2 | 1.80 (45 %) | 1.48 (37 %) | 2.29 (57 %) | 1.69 (42 %) | 1.73 (43 %) | 9.9 | 0.44 |
| 64 | 8 | 8,732 | 182.9 | 1.28 (16 %) | 1.03 (13 %) | 1.86 (23 %) | 1.55 (19 %) | 1.25 (16 %) | 14.5 | 0.29 |

At 64 ranks (8,732 tetrahedra per rank) the loop is slower than at 32, and
the explicit step as slow as at 8 ranks. The check scales best. The mesh
distribution grows with the rank count, `init_full` shrinks.

Table "Weak scaling" (about 70,000 tetrahedra per rank, 8 ranks per host;
E_w = loop time per step at 8 ranks / at P):

| ranks | hosts | mesh | tetrahedra per rank | cohesive elements (first insertion) | `solve_step` ms/step | check ms/step | loop ms/step | E_w | core-µs/el/step | static solve s | peak GiB per host |
|---:|---:|---|---:|---|---:|---:|---:|---:|---:|---:|---:|
| 8 | 1 | h = 0.04 | 69,859 | 2,402 (705) | 43.0 | 102.2 | 156.6 | 1.00 | 2.08 | 21.1 | 12.9 |
| 16 | 2 | h = 0.032 | 69,890 | 3,137 (678) | 54.9 | 114.1 | 184.9 | 0.85 | 2.42 | 47.5 | 17.1 |
| 32 | 4 | h = 0.025 | 70,511 | 3,855 (746) | 70.6 | 138.7 | 232.4 | 0.67 | 2.97 | 101.5 | 26.9 |
| 64 | 8 | JOSS | 68,628 | 6,350 (683) | 94.2 | 166.5 | 298.3 | 0.53 | 3.80 | 230.8 | 28.6 |

The step time grows with the rank count at nearly constant elements per rank
(E_w 0.85, 0.67, 0.53 at 16, 32, 64 ranks). The four meshes insert different
numbers of cohesive elements, and the per-phase totals do not separate the
steps before and after the first insertion. These runs show that one MPI job
of the port runs across eight hosts at up to 64 ranks; they say nothing about
other interconnects or beyond 64 ranks.

## Memory

### The final campaign (part E)

h = 0.04 (558,875 tetrahedra), the static solve and 300 steps, 1 / 2 / 4 /
8 ranks, five repetitions; AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17 UTC;
data `docs/benchmarks/perf-campaign/final/E/summary.md`. Peaks of the
sampler (`memsample.py`, VmHWM of every rank every 0.25 s); container = the
cgroup's `memory.peak`; the port's own records in `summary.json`; the last
column = (Σ rank peaks − the port's `INFOG(19)`) / tetrahedra (the C++ row
subtracts the port's `INFOG(19)` too: upstream records no MUMPS
statistics). "Port, factors released" is the head with
`rust_release_static_solver = true`.

Table "Memory, h = 0.04":

| ranks | code | n | largest rank MiB | Σ rank peaks MiB | container MiB | static: largest / Σ (port) MiB | port model before static: largest / Σ MiB | MUMPS INFOG(19) MB | besides MUMPS, bytes per tetrahedron |
|---|---|---|---|---|---|---|---|---|---|
| 1 | C++ | 5 | 11,669 [11,669, 11,669] | 11,669 [11,669, 11,669] | 11,659 [11,659, 11,687] | — | — | 5302 | 12,408 |
| 1 | port before | 5 | 10,271 [10,270, 10,271] | 10,271 [10,270, 10,271] | 10,269 [10,269, 10,287] | 7,149 / 7,149 | 1,733 / 1,733 | 5302 | 9,783 |
| 1 | port | 5 | 8,965 [8,964, 8,965] | 8,965 [8,964, 8,965] | 8,960 [8,960, 8,960] | 7,302 / 7,302 | 1,732 / 1,732 | 5302 | 7,334 |
| 1 | port, factors released | 5 | 7,302 [7,302, 7,303] | 7,302 [7,302, 7,303] | 7,294 [7,294, 7,294] | 7,302 / 7,302 | 1,732 / 1,732 | 5302 | 4,214 |
| 2 | C++ | 5 | 6,961 [6,961, 6,962] | 13,741 [13,740, 13,747] | 13,707 [13,707, 13,715] | — | — | 7586 | 12,207 |
| 2 | port before | 5 | 5,885 [5,884, 5,892] | 11,352 [11,347, 11,359] | 11,323 [11,318, 11,330] | 4,333 / 8,249 | 955 / 1,878 | 7586 | 7,725 |
| 2 | port | 5 | 5,566 [5,557, 5,576] | 11,013 [10,992, 11,023] | 10,982 [10,958, 10,992] | 4,790 / 9,458 | 955 / 1,876 | 7586 | 7,090 |
| 2 | port, factors released | 5 | 4,792 [4,790, 4,801] | 9,464 [9,459, 9,474] | 9,430 [9,425, 9,439] | 4,792 / 9,464 | 955 / 1,878 | 7586 | 4,183 |
| 4 | C++ | 5 | 3,489 [3,381, 3,652] | 13,171 [13,117, 13,673] | 13,045 [12,991, 13,548] | — | — | 6900 | 12,365 |
| 4 | port before | 5 | 3,022 [2,999, 3,123] | 11,728 [11,661, 11,747] | 11,639 [11,570, 11,657] | 2,252 / 8,673 | 545 / 2,138 | 6900 | 9,658 |
| 4 | port | 5 | 2,709 [2,687, 2,952] | 10,400 [10,339, 11,043] | 10,302 [10,239, 10,946] | 2,590 / 9,547 | 542 / 2,139 | 6900 | 7,167 |
| 4 | port, factors released | 5 | 2,419 [2,315, 2,604] | 8,873 [8,837, 9,436] | 8,773 [8,738, 9,338] | 2,315 / 8,873 | 542 / 2,139 | 6900 | 4,301 |
| 8 | C++ | 5 | 2,199 [2,154, 2,699] | 15,367 [13,976, 15,465] | 15,037 [13,633, 15,134] | — | — | 8051 | 14,426 |
| 8 | port before | 5 | 2,245 [1,889, 2,439] | 13,938 [12,328, 14,463] | 13,697 [12,078, 14,212] | 1,504 / 9,913 | 313 / 2,373 | 8051 | 11,744 |
| 8 | port | 5 | 1,843 [1,682, 2,228] | 12,133 [11,452, 12,277] | 11,878 [11,197, 12,023] | 2,086 / 10,802 | 323 / 2,389 | 8051 | 8,359 |
| 8 | port, factors released | 5 | 1,614 [1,553, 2,134] | 10,716 [10,358, 11,170] | 10,466 [10,105, 10,918] | 1,857 / 11,170 | 323 / 2,388 | 8051 | 5,700 |

- Σ rank peaks, port / C++ (ratio of the medians): 0.768 / 0.801 / 0.790 /
  0.790 at 1 / 2 / 4 / 8 ranks; with the factors released 0.626 / 0.689 /
  0.674 / 0.697.
- MUMPS's `INFOG(19)` is 5,302 to 8,051 MB of these totals. Besides MUMPS
  the port needs 7,090–8,359 bytes per tetrahedron, upstream 12,207–14,426,
  and the port with the factors released 4,183–5,700.
- The largest rank at 8 ranks: 1,843 MiB (port), 2,199 MiB (C++).

### Before the final campaign (12–13 November)

Table "Memory" of the full-size run (GiB; sampler = `memsample.py`, every
0.25 s, VmRSS / VmHWM of every rank; container = the cgroup's `memory.peak`;
static = the sampler's peaks up to the end of the static solve; the port's
own records of VmRSS before the static solve):

| code | ranks | largest rank | total (sampler) | container | static: largest rank / total | per-rank peak min / median / max | port model before static: largest / sum |
|---|---:|---:|---:|---:|---|---|---|
| Rust | 64 | 6.13 | 227.0 | 206.2 | 5.98 / 216.3 | 2.74 / 3.30 / 6.13 | 0.92 / 20.1 |
| C++ | 64 | 4.84 | 212.0 | 191.7 | 4.42 / 188.9 | 2.81 / 3.17 / 4.84 | — |
| Rust | 32 | 10.42 | 189.0 | 185.5 | 10.00 / 174.2 | 4.36 / 5.16 / 10.42 | 0.64 / 17.5 |
| C++ | 32 | 11.24 | 197.9 | 194.2 | 10.57 / 175.5 | 4.52 / 5.64 / 11.24 | — |
| Rust | 8 | 31.78 | 165.3 | 165.9 | 29.45 / 146.4 | 15.49 / 19.97 / 31.78 | 1.83 / 14.3 |

Table "MUMPS" of the full-size run (the port's records; upstream hands MUMPS
the same matrix; `INFOG` in MB, flops = `RINFOG(3)`):

| ranks | ordering `INFOG(7)` | factor entries `INFOG(29)` | allocated sum / largest rank `INFOG(19)` / `(18)` | used sum / largest `INFOG(22)` / `(21)` | elimination flops | rate (flops / static-solve wall) | retries | matrix entries sum / largest rank |
|---:|---:|---:|---|---|---:|---:|---:|---|
| 64 (20-step test) | 4 | 7.73e+09 | 171,623 / 5,462 | 108,124 / 3,721 | 1.42e+14 | 481 GF/s | 0 | 53,725,776 / 846,258 |
| 64 | 4 | 7.86e+09 | 182,823 / 6,155 | 110,220 / 4,109 | 1.49e+14 | 489 GF/s | 0 | 53,725,776 / 846,258 |
| 32 | 4 | 7.67e+09 | 162,775 / 10,689 | 101,796 / 7,918 | 1.4e+14 | 394 GF/s | 0 | 53,089,626 / 1,677,093 |
| 8 | 4 | 7.95e+09 | 139,310 / 28,687 | 107,226 / 28,273 | 1.54e+14 | 185 GF/s | 0 | 52,023,552 / 6,531,900 |

- MUMPS dominates: its summed allocation `INFOG(19)` is 170 GiB of the
  port's 227 GiB at 64 ranks and 130 of 165 GiB at 8, and it stays allocated
  through the loop in both codes (upstream keeps its static solver too).
  Whether anything uses the factors after the static solve was not checked.
  MUMPS options that reduce memory (out-of-core factors `ICNTL(22)`, other
  orderings, block low-rank `ICNTL(35)`) are reachable through
  `MumpsOptions`; they are not upstream's defaults and were not measured.
- The port's largest rank at 64 ranks peaks at 6.13 GiB against 4.84 GiB
  (1.27×) and at 1.86× the median rank (upstream 1.53×). It is most likely
  the rank holding MUMPS's largest allocation (`INFOG(18)` 6,155 MB =
  5.7 GiB; its growth in the static solve, 5.5 GiB, is the largest of the
  ranks). Upstream records no MUMPS statistics, so the difference is not
  explained.
- Mesh, facets and model before the static solve: 14.3 GiB summed at 8 ranks
  (3.5 KB per tetrahedron), 20.1 GiB at 64 (4.9 KB). A two-point fit of the
  per-rank records (about 58 MiB per process plus 3.04 KiB per tetrahedron
  or ghost) puts about 2 GiB of the difference on the ghost layers and about
  3 GiB on the per-process memory of 64 processes.

Table "Memory besides MUMPS" (mid-size rehearsal; L = (sum of the ranks'
whole-run VmHWM − `INFOG(19)`) / tetrahedra, bytes per tetrahedron;
h = 0.04 before insertion, h = 0.06 full schedule):

| P | port h0.04 pre | port h0.06 full | C++ h0.04 pre | C++ h0.06 full | port model only (before the static solve, h0.04) |
|---|---|---|---|---|---|
| 1 | 9.77e+03 | 1.11e+04 | 1.24e+04 | 1.3e+04 | 3.25e+03 |
| 2 | 1.02e+04 | 1.08e+04 | 1.04e+04 | 1.29e+04 | 3.51e+03 |
| 4 | 1.11e+04 | 1.23e+04 | 1.4e+04 | 1.44e+04 | 3.99e+03 |
| 8 | 1.08e+04 | 1.23e+04 | — | 1.41e+04 | 4.43e+03 |

C++ at 8 ranks on h = 0.04 was killed by the 14 GiB container limit during
the static solve, which puts its L above 1.23e4 bytes. Besides MUMPS both
codes hold about 10–14 KB per tetrahedron at their peak, linear in the mesh
size (three sizes). Of that, both allocate about 3 GiB (5.7 KB per
tetrahedron) in `before_step` (`initNewSolver(_explicit_lumped_mass)`,
step-0 energies, stable time step: 25.0 s in C++ and 25.9 s in the port at
h = 0.04 and 1 rank), and the fragmenting schedule adds about 10 %. The
port's peaks were 0.86–0.99× upstream's at every mid-size point.

Table "MUMPS allocation beyond 8 ranks" (`INFOG(19)` MB summed over the
ranks, boat sandbox, 2026-11-12; excess = M(P) − M(8) in GB, ratio =
M(P) / M(8); `docs/benchmarks/midsize-2026-11-12/c1fix-mscale-*`):

| mesh | P = 8 | 16 | 32 | 64 | excess 16 / 32 / 64 GB | ratio 16 / 32 / 64 |
|---|---|---|---|---|---|---|
| h0.12 (14,472 DOFs) | 446 | 1,484 | 2,818 | 10,744 | 1.04 / 2.37 / 10.3 | 3.33 / 6.32 / 24.1 |
| h0.06 (97,551 DOFs) | 1,765 | 3,224 | 4,395 | 12,982 | 1.46 / 2.63 / 11.2 | 1.83 / 2.49 / 7.36 |
| h0.05 (150,702 DOFs) | 3,657 | 4,731 | 6,409 | — | 1.07 / 2.75 / — | 1.29 / 1.75 / — |
| h0.04 (293,745 DOFs) | 8,167 | — | — | — | — / — / — | — / — / — |

The summed allocation grows with the rank count by an excess that is about
the same on meshes 10× apart in DOFs. At full size it was 139 GB at 8 ranks
and 183 GB at 64 (table "MUMPS" above).

## MPI layer

### Point-to-point latency against raw MPI (final campaign, part F)

The methods of `docs/benchmarks/mpi-latency-2026-11-14/`: method A launches
each case separately, method B runs `mpi_bench calibrated` in one job. AWS
`m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17 UTC, Open MPI 5.0.7; data
`docs/benchmarks/perf-campaign/final/F/summary.md` (method B's table is
there; its backend ratios at 8 B, 1 MiB and 16 MiB subtract a calibration
larger than or close to the measured time and are not used here).

Table "Method A, separate launches" (µs, median [min, max]; ping-pong: half
round trip; halo: one exchange with every other rank, slowest rank):

| Transport | Case | Raw MPI (C) | akantu-comm before | akantu-comm head | head / raw, paired | before / raw | head / before |
|---|---|---|---|---|---|---|---|
| shared memory | halo, 2 ranks, 8 KiB | 1.73 [1.71, 1.76] | 3.70 [3.66, 3.73] | 3.44 [3.40, 3.46] | 1.99 [1.95, 2.00] | 2.14 [2.08, 2.16] | 0.928 [0.924, 0.940] |
| shared memory | halo, 4 ranks, 8 KiB | 5.23 [5.18, 5.27] | 10.5 [10.5, 10.7] | 9.94 [9.85, 10.0] | 1.89 [1.89, 1.93] | 2.02 [1.99, 2.06] | 0.940 [0.936, 0.948] |
| shared memory | ping-pong 8 B | 0.130 [0.130, 0.130] | 0.860 [0.850, 0.870] | 0.840 [0.830, 0.840] | 6.46 [6.38, 6.46] | 6.62 [6.54, 6.69] | 0.966 [0.965, 0.977] |
| shared memory | ping-pong 64 KiB | 8.54 [8.49, 8.55] | 15.0 [15.0, 15.1] | 13.3 [13.3, 13.3] | 1.56 [1.55, 1.57] | 1.76 [1.75, 1.77] | 0.885 [0.881, 0.889] |
| shared memory | ping-pong 1 MiB | 118 [117, 118] | 218 [218, 220] | 186 [186, 186] | 1.58 [1.57, 1.58] | 1.85 [1.85, 1.87] | 0.851 [0.845, 0.852] |
| shared memory | ping-pong 16 MiB | 2,093 [2,086, 2,098] | 7,078 [7,012, 7,111] | 5,278 [5,252, 5,325] | 2.52 [2.51, 2.54] | 3.38 [3.36, 3.40] | 0.746 [0.743, 0.752] |
| TCP | halo, 2 ranks, 8 KiB | 13.1 [13.0, 13.2] | 16.4 [16.1, 16.6] | 16.2 [16.0, 16.3] | 1.23 [1.23, 1.24] | 1.26 [1.22, 1.27] | 0.984 [0.967, 1.01] |
| TCP | halo, 4 ranks, 8 KiB | 32.6 [32.1, 32.6] | 43.1 [43.0, 43.4] | 42.3 [42.2, 42.4] | 1.30 [1.30, 1.32] | 1.32 [1.32, 1.35] | 0.980 [0.975, 0.983] |
| TCP | ping-pong 8 B | 8.24 [7.98, 8.31] | 9.35 [9.32, 9.37] | 9.35 [9.33, 9.39] | 1.13 [1.12, 1.18] | 1.13 [1.12, 1.17] | 1.00 [0.999, 1.00] |
| TCP | ping-pong 64 KiB | 40.3 [40.3, 41.3] | 48.5 [48.3, 49.1] | 46.6 [46.5, 46.7] | 1.16 [1.13, 1.16] | 1.20 [1.17, 1.22] | 0.963 [0.948, 0.964] |
| TCP | ping-pong 1 MiB | 157 [156, 160] | 258 [258, 259] | 228 [227, 228] | 1.45 [1.43, 1.46] | 1.64 [1.62, 1.66] | 0.881 [0.876, 0.884] |
| TCP | ping-pong 16 MiB | 3,127 [3,040, 3,152] | 8,041 [7,881, 8,101] | 6,496 [6,391, 6,527] | 2.08 [2.06, 2.10] | 2.57 [2.57, 2.59] | 0.810 [0.802, 0.811] |

- Over shared memory the head takes 6.46× raw MPI for an 8-byte ping-pong,
  1.56–1.58× for 64 KiB and 1 MiB, 2.52× for 16 MiB and 1.89–1.99× for the
  8 KiB halos; over TCP 1.13–2.08×.
- Head / before: 0.746 for 16 MiB and 0.851 for 1 MiB over shared memory;
  0.928–0.966 for the halos and the 8-byte ping-pong.

### Collectives across hosts (D-126)

The port's collective error protocol (D-018) agreed on every rank's
outcome after every phase: one all-reduce of status, phase and digest per
phase, carried by the header of a checked collective (one `MPI_Iallgather`,
Open MPI's linear non-blocking algorithm). On one machine an agreement
costs microseconds; across hosts about a round trip to the farthest host.
Measured on a four-host cluster of 8-core machines
(`docs/benchmarks/perf-campaign/net/README.md`; 8 ranks, h = 0.06,
2,000 steps):

- per step and rank the port made 51.6 collectives (41.8 of them
  agreements), upstream 8.4; one agreement cost 2.9 µs with 8 ranks on one
  host and 472 µs with 8 ranks on four hosts;
- without the agreements (a diagnostic build) the port's results were
  bitwise the same and its loop across four hosts fell from 88.8 s to
  43.4 s.

D-126 (`docs/DECISIONS.md`) keeps an agreement where the ranks decide
together or are about to depend on each other: at commit points, before
every point-to-point exchange and before every persistent change of an
operation; purely local outcomes in between are deferred to the next
agreement, and every checked collective's header carries the protocol
status. From 64 ranks the header goes by gather and broadcast (254
messages per agreement at 128 ranks against 16,147 for the all-gather).
The mode is `AKANTU_COLLECTIVE_CHECKS` (`commit-points`, the default, or
`every-phase`), the header exchange `AKANTU_COLLECTIVE_HEADER`
([`docs/TESTING.md`](TESTING.md)). Results are bitwise unchanged in both
modes; after a failure every rank's persistent state is the one the
every-phase mode leaves; a peer's local failure becomes visible at the
next agreement instead of at the end of the failed phase.

The benchmark's step then makes 20.5 collectives per step and rank
(upstream 8.4). At full size (section "Multi-node runs"): 128 ranks on
four 32-core hosts took 1,538 s and 1,536 s in the loop before the change
(upstream 1,040 s and 1,019 s) and 526 s at the final revision; 64 ranks on
two hosts 1,046 s before and 722 s after (upstream 1,924 s). On one host
the change is small: 814 s against 835 s at 128 ranks on the 128-core
machine; the shared-memory suite cases of `net/README.md`, section 7, are
within 1.5 % of the first round of the change.

### Before the final campaign (2026-11-14)

#### Point-to-point latency against raw MPI

`crates/akantu-comm/examples/mpi_bench.rs` (release, feature `mpi`) against
the raw-MPI C program `scripts/mpi/bench_baseline.c` (`mpicc -O2`) on x86_64:
boat.dev `default` sandbox, 4 vCPU of an AMD Ryzen 9 9950X host, image
`akantu-rs-mpi:dev` (Open MPI 5.0.7, Rust 1.99.1), container 4 CPUs and
6 GiB, 2026-11-14. The image disables Open MPI's
single-copy mechanism, so shared memory copies in and out of a shared buffer.
Files, commands and binary digests: `docs/benchmarks/mpi-latency-2026-11-14/`.
Ping-pong: half round trip; halo: one exchange with every other rank,
slowest rank.

Table "Method A, separate launches" (the two programs adjacent per case,
order alternated, 5 repetitions; iterations 20,000 (8 B), 5,000 (64 KiB and
the halos), 500 (1 MiB), 30 (16 MiB), 10 % warm-up; µs, median [min, max];
ratio = median [min, max] of the per-repetition ratios; the last column is
the development Mac's series from the appendix, shown for reference only and
not compared with the x86_64 numbers):

| Transport | Case | Raw MPI (C) | akantu-comm | akantu / raw, paired | Mac, after / raw |
|---|---|---|---|---|---|
| shared memory | ping-pong 8 B | 0.09 [0.09, 0.16] | 0.59 [0.58, 0.61] | 6.44 [3.75, 6.78] | 6.5× |
| shared memory | ping-pong 64 KiB | 2.45 [2.43, 2.48] | 5.27 [5.20, 12.1] | 2.17 [2.11, 4.94] | 3.6× |
| shared memory | ping-pong 1 MiB | 26.3 [26.1, 27.2] | 92.1 [89.7, 120.5] | 3.50 [3.44, 4.52] | 3.7× |
| shared memory | ping-pong 16 MiB | 1,212 [1,079, 1,563] | 5,994 [5,732, 6,720] | 4.76 [4.30, 5.54] | 3.6–4.1× |
| shared memory | halo, 2 ranks, 8 KiB | 0.77 [0.71, 2.08] | 2.06 [1.89, 2.29] | 2.59 [0.97, 3.23] | 1.9× |
| shared memory | halo, 4 ranks, 8 KiB | 2.53 [2.32, 4.79] | 8.90 [6.32, 12.8] | 2.85 [2.05, 4.30] | 2.3–3.5× |
| TCP | ping-pong 8 B | 2.25 [2.09, 3.46] | 2.80 [2.72, 3.88] | 1.28 [0.89, 1.41] | 1.3× |
| TCP | ping-pong 64 KiB | 11.1 [10.7, 15.6] | 14.3 [13.8, 20.0] | 1.29 [1.24, 1.76] | 1.6× |
| TCP | ping-pong 1 MiB | 60.9 [49.7, 77.0] | 117.5 [116.2, 158.9] | 2.06 [1.72, 2.36] | 2.5× |
| TCP | ping-pong 16 MiB | 1,890 [1,850, 2,236] | 6,630 [6,462, 7,332] | 3.52 [3.11, 3.63] | 2.0× |
| TCP | halo, 2 ranks, 8 KiB | 3.80 [3.61, 5.41] | 4.91 [4.88, 5.00] | 1.32 [0.91, 1.35] | 1.9× |
| TCP | halo, 4 ranks, 8 KiB | 10.6 [9.79, 20.8] | 15.7 [14.0, 21.5] | 1.42 [1.03, 1.62] | 1.3× |

Method B runs raw MPI (through the `mpi` crate) and akantu-comm in
interleaved blocks of one job (`mpi_bench calibrated`, 9 blocks, 3
repetitions), plus a calibration block (one `send` and `receive_vec` to
self: the API's buffer work without MPI). The MPI test tier gates its
backend ratio (akantu − calibration) / raw at ≤ 8. Over shared memory it was
3.35 (8 B), 1.18 (64 KiB), 0.65 (1 MiB), 1.04 (16 MiB), 2.61 and 2.34 (the 2-
and 4-rank halos); over TCP 1.07–1.38 (`calibrated.txt`).

- A small message over shared memory costs about 0.5 µs more than raw MPI
  (0.59 against 0.09 µs), about 0.3 µs of it the API's buffer work. Over
  TCP the small-message overhead is 1.2–1.3×.
- Large messages over shared memory take 3.5× (1 MiB) and 4.8× (16 MiB) the
  raw time in separate launches, 2.1× and 3.5× over TCP. In one job they
  take 9.9–11.2× raw at 1 MiB and 6.4–8.4× at 16 MiB, nearly all of it
  buffer work: the calibration alone takes
  287 µs for 1 MiB.
- 8 KiB halo exchanges take 2.3–2.9× raw over shared memory and 1.2–1.4×
  over TCP.
- The cost is one packing copy per send (the payload goes into an owned
  buffer, so a send left behind at a deadline never refers to user memory),
  and a zeroed buffer and an unpacking copy per receive (the matching engine
  receives into a buffer of the matched length). The rest of a small
  message's extra cost, about 0.2 µs, is the matching engine, locks and
  pacing, with two `MPI_Improbe` calls per waiting key and poll. Buffer reuse would remove
  the per-message allocations; it was not done.

The difference between the two methods is the allocator's state. A
measurement in one job (boat.dev `default`, 4 vCPU, CPU model not recorded,
2026-11-12, medians of 30 runs, shared memory) found that an akantu rank
takes about 320 minor page faults per 16 MiB message standalone (freed
blocks reused) and about 12,000 per operation in a job that also runs raw
MPI (every block returned to the system and faulted in again); the raw C
program takes about 140. In that measurement the backend ratio was 0.82
(16 MiB), 0.57 (1 MiB) and 1.61 (4-rank halo).

An idle wait costs 40–60 ms of thread CPU per 2 s on x86_64 (the MPI tier's
timing checks, `docs/TESTING.md`): the progress loop spins 64 polls, yields
for 20 ms, then sleeps 250 µs per poll (`crates/akantu-comm/src/pacing.rs`).

In a whole run, the per-step energies (global sums every step) cost 20.3 ms
per step at 64 ranks on one node and 37.8 ms across eight hosts. No other
measurement isolates the exchanges' share of a simulation.

## Set-up costs

### The final campaign (parts F and B)

The cohesive driver, one step, on h = 0.06 and 0.04 at 1 / 2 / 4 / 8 ranks,
five repetitions; AWS `m7a.2xlarge`, AMD EPYC 9R14, 2026-11-17 UTC; data
`docs/benchmarks/perf-campaign/final/F/summary.md`.

Table "Set-up costs" (rank 0's Chrono phases, s):

| h | ranks | code | read_mesh s | dist_mesh s | init_full s | before_step s | static solve s |
|---|---|---|---|---|---|---|---|
| 0.06 | 1 | C++ | 0.248 [0.246, 0.251] | 0.000358 [0.000357, 0.000469] | 13.9 [13.9, 13.9] | 4.02 [4.02, 4.06] | 12.9 [12.8, 12.9] |
| 0.06 | 1 | port before | 0.140 [0.137, 0.143] | 0.00000218 [0.00000186, 0.00000242] | 3.07 [3.06, 3.07] | 4.38 [4.34, 4.41] | 14.1 [14.0, 14.1] |
| 0.06 | 1 | port | 0.0898 [0.0890, 0.0908] | 0.00000215 [0.00000201, 0.00000248] | 1.34 [1.33, 1.35] | 1.44 [1.43, 1.46] | 10.2 [10.2, 10.3] |
| 0.06 | 2 | C++ | 0.248 [0.247, 0.250] | 1.76 [1.76, 1.78] | 6.24 [6.24, 6.26] | 1.93 [1.90, 1.97] | 8.58 [8.52, 8.63] |
| 0.06 | 2 | port before | 0.141 [0.140, 0.144] | 0.279 [0.274, 0.283] | 1.69 [1.69, 1.70] | 2.10 [2.09, 2.13] | 9.25 [9.19, 9.29] |
| 0.06 | 2 | port | 0.0905 [0.0898, 0.0908] | 0.179 [0.177, 0.179] | 0.741 [0.731, 0.744] | 0.681 [0.673, 0.696] | 7.47 [7.40, 7.51] |
| 0.06 | 4 | C++ | 0.248 [0.247, 0.249] | 1.80 [1.80, 1.81] | 2.55 [2.54, 2.59] | 0.980 [0.963, 0.985] | 4.65 [4.61, 4.98] |
| 0.06 | 4 | port before | 0.142 [0.139, 0.143] | 0.315 [0.312, 0.325] | 0.882 [0.878, 0.889] | 1.03 [1.02, 1.04] | 4.88 [4.83, 5.31] |
| 0.06 | 4 | port | 0.0886 [0.0878, 0.0898] | 0.173 [0.172, 0.175] | 0.385 [0.380, 0.390] | 0.331 [0.320, 0.333] | 4.06 [3.99, 4.08] |
| 0.06 | 8 | C++ | 0.247 [0.245, 0.252] | 1.89 [1.88, 1.91] | 1.17 [1.16, 1.18] | 0.492 [0.491, 0.498] | 3.26 [3.10, 3.30] |
| 0.06 | 8 | port before | 0.141 [0.140, 0.143] | 0.417 [0.406, 0.421] | 0.488 [0.485, 0.498] | 0.512 [0.507, 0.527] | 3.27 [3.21, 3.40] |
| 0.06 | 8 | port | 0.0892 [0.0887, 0.0917] | 0.179 [0.178, 0.187] | 0.218 [0.214, 0.224] | 0.167 [0.166, 0.169] | 2.81 [2.58, 2.94] |
| 0.04 | 1 | C++ | 0.838 [0.829, 0.842] | 0.00113 [0.00112, 0.00113] | 73.5 [73.5, 73.6] | 14.9 [14.9, 15.0] | 81.8 [81.6, 81.8] |
| 0.04 | 1 | port before | 0.480 [0.467, 0.484] | 0.00000202 [0.00000187, 0.00000232] | 10.4 [10.4, 10.4] | 15.9 [15.7, 16.0] | 84.4 [84.3, 84.8] |
| 0.04 | 1 | port | 0.280 [0.276, 0.281] | 0.00000207 [0.00000181, 0.00000220] | 5.48 [5.42, 5.52] | 5.23 [5.21, 5.31] | 70.3 [70.0, 70.5] |
| 0.04 | 2 | C++ | 0.843 [0.823, 0.847] | 5.95 [5.87, 5.97] | 32.0 [31.9, 32.0] | 7.11 [7.10, 7.20] | 57.7 [57.6, 57.9] |
| 0.04 | 2 | port before | 0.474 [0.470, 0.494] | 1.18 [1.13, 1.20] | 5.69 [5.68, 5.70] | 7.66 [7.60, 7.73] | 51.2 [51.0, 51.3] |
| 0.04 | 2 | port | 0.278 [0.277, 0.292] | 0.637 [0.628, 0.644] | 2.91 [2.90, 2.92] | 2.53 [2.52, 2.57] | 52.9 [52.9, 53.2] |
| 0.04 | 4 | C++ | 0.829 [0.821, 0.843] | 5.93 [5.90, 5.95] | 11.5 [11.3, 11.9] | 3.67 [3.64, 3.68] | 27.9 [27.7, 29.8] |
| 0.04 | 4 | port before | 0.473 [0.469, 0.479] | 1.38 [1.37, 1.42] | 2.89 [2.88, 2.90] | 3.78 [3.74, 3.80] | 28.5 [28.4, 29.5] |
| 0.04 | 4 | port | 0.280 [0.277, 0.281] | 0.603 [0.599, 0.613] | 1.45 [1.44, 1.47] | 1.22 [1.22, 1.23] | 25.5 [25.2, 27.3] |
| 0.04 | 8 | C++ | 0.834 [0.826, 0.850] | 6.07 [6.04, 6.95] | 5.12 [5.05, 5.17] | 1.94 [1.92, 1.96] | 19.7 [17.7, 21.1] |
| 0.04 | 8 | port before | 0.472 [0.464, 0.484] | 2.01 [1.95, 2.05] | 1.58 [1.57, 1.60] | 1.98 [1.97, 1.99] | 19.0 [18.4, 19.6] |
| 0.04 | 8 | port | 0.279 [0.279, 0.283] | 0.643 [0.638, 0.653] | 0.779 [0.764, 0.780] | 0.667 [0.651, 0.682] | 18.4 [16.7, 24.5] |

Table "Set-up, port / upstream" (paired):

| h | ranks | read_mesh | dist_mesh | init_full | before_step | static solve |
|---|---|---|---|---|---|---|
| 0.06 | 1 | 0.362 [0.354, 0.366] | 0.00562 [0.00459, 0.00694] | 0.0965 [0.0959, 0.0969] | 0.359 [0.356, 0.361] | 0.795 [0.792, 0.797] |
| 0.06 | 2 | 0.363 [0.363, 0.368] | 0.101 [0.101, 0.102] | 0.119 [0.117, 0.119] | 0.351 [0.350, 0.358] | 0.870 [0.862, 0.873] |
| 0.06 | 4 | 0.358 [0.353, 0.363] | 0.0963 [0.0953, 0.0970] | 0.151 [0.147, 0.152] | 0.337 [0.325, 0.345] | 0.876 [0.813, 0.881] |
| 0.06 | 8 | 0.363 [0.359, 0.363] | 0.0951 [0.0939, 0.0995] | 0.187 [0.182, 0.193] | 0.339 [0.336, 0.344] | 0.851 [0.785, 0.950] |
| 0.04 | 1 | 0.334 [0.329, 0.339] | 0.00183 [0.00161, 0.00197] | 0.0745 [0.0737, 0.0751] | 0.349 [0.349, 0.355] | 0.859 [0.858, 0.862] |
| 0.04 | 2 | 0.337 [0.328, 0.346] | 0.107 [0.106, 0.109] | 0.0910 [0.0906, 0.0913] | 0.356 [0.354, 0.361] | 0.918 [0.914, 0.922] |
| 0.04 | 4 | 0.337 [0.332, 0.339] | 0.102 [0.101, 0.103] | 0.125 [0.122, 0.130] | 0.333 [0.332, 0.337] | 0.913 [0.846, 0.965] |
| 0.04 | 8 | 0.336 [0.332, 0.338] | 0.106 [0.0933, 0.106] | 0.152 [0.151, 0.153] | 0.343 [0.332, 0.351] | 0.893 [0.850, 1.39] |

Table "Distribution" (`suite_setup` `dist_mesh_tet4`, size `l`: 1,296,000
Tet4; the port only):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | distribute | 2 | 5 | 7.35 s [7.31 s, 7.36 s] | 2.09 s [2.08 s, 2.11 s] | 0.285 [0.284, 0.287] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 4 | 5 | 8.37 s [8.33 s, 8.39 s] | 2.34 s [2.34 s, 2.35 s] | 0.280 [0.279, 0.281] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 8 | 5 | 10.5 s [10.5 s, 10.6 s] | 2.70 s [2.69 s, 2.70 s] | 0.256 [0.255, 0.257] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 2 | 5 | 1.52 s [1.49 s, 1.54 s] | 941 ms [933 ms, 947 ms] | 0.619 [0.613, 0.627] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 4 | 5 | 1.52 s [1.51 s, 1.53 s] | 945 ms [941 ms, 946 ms] | 0.622 [0.619, 0.623] |  |  |  | equal |
| dist_mesh_tet4 | mesh | 8 | 5 | 1.51 s [1.50 s, 1.51 s] | 941 ms [938 ms, 949 ms] | 0.625 [0.624, 0.629] |  |  |  | equal |
| dist_mesh_tet4 | partition | 2 | 5 | 5.51 s [5.51 s, 5.52 s] | 1.48 s [1.47 s, 1.50 s] | 0.268 [0.267, 0.272] |  |  |  | equal |
| dist_mesh_tet4 | partition | 4 | 5 | 5.87 s [5.86 s, 5.88 s] | 1.81 s [1.81 s, 1.82 s] | 0.309 [0.308, 0.310] |  |  |  | equal |
| dist_mesh_tet4 | partition | 8 | 5 | 6.31 s [6.30 s, 6.32 s] | 2.22 s [2.21 s, 2.23 s] | 0.352 [0.351, 0.353] |  |  |  | equal |

Table "suite_setup, serial cases" (part B, selected phases; the port only):

| case | phase | calls | n | before | head | head / before | digests |
|---|---|---|---|---|---|---|---|
| cohesive_insert_tet4 | init_mesh_facets | 1 | 5 | 1.81 s [1.81 s, 1.82 s] | 514 ms [514 ms, 519 ms] | 0.284 [0.282, 0.287] | equal |
| cohesive_insert_tet4 | insert | 1 | 5 | 4.09 s [4.08 s, 4.13 s] | 3.60 s [3.57 s, 3.65 s] | 0.880 [0.864, 0.884] | equal |
| cohesive_insert_tet4 | update_insertion_facets | 1 | 5 | 402 µs [391 µs, 452 µs] | 998 µs [611 µs, 1.26 ms] | 2.49 [1.35, 3.21] | equal |
| dump_paraview_ascii_tet4 | dump | 3 | 5 | 2.71 s [2.71 s, 2.72 s] | 857 ms [853 ms, 873 ms] | 0.316 [0.314, 0.322] | equal |
| dump_paraview_tet4 | dump | 15 | 5 | 3.02 s [3.02 s, 3.06 s] | 1.44 s [1.44 s, 1.45 s] | 0.477 [0.471, 0.478] | equal |
| dump_text_tet4 | dump | 30 | 5 | 2.22 s [2.20 s, 2.23 s] | 507 ms [506 ms, 508 ms] | 0.228 [0.228, 0.230] | equal |
| erase_purify_tet4 | erase_elements | 1 | 5 | 10.5 ms [10.5 ms, 10.7 ms] | 11.1 ms [10.9 ms, 11.2 ms] | 1.06 [1.03, 1.06] | equal |
| facets_hex8 | init_mesh_facets | 1 | 5 | 953 ms [952 ms, 966 ms] | 653 ms [651 ms, 656 ms] | 0.686 [0.676, 0.688] | equal |
| facets_tet4 | init_mesh_facets | 1 | 5 | 1.80 s [1.80 s, 1.80 s] | 507 ms [505 ms, 526 ms] | 0.282 [0.280, 0.292] | equal |
| groups_tet4 | element_group_from_node_group | 1 | 5 | 161 ms [160 ms, 162 ms] | 30.5 ms [30.3 ms, 31.2 ms] | 0.190 [0.187, 0.194] | equal |
| msh22_read_hex8 | read | 1 | 5 | 173 ms [172 ms, 174 ms] | 98.6 ms [97.9 ms, 99.5 ms] | 0.571 [0.568, 0.576] | equal |
| msh22_read_tet4 | read | 1 | 5 | 141 ms [140 ms, 143 ms] | 83.9 ms [83.2 ms, 84.2 ms] | 0.594 [0.588, 0.598] | equal |
| msh41_read_tet4 | read | 1 | 5 | 113 ms [113 ms, 114 ms] | 71.8 ms [71.2 ms, 72.8 ms] | 0.638 [0.629, 0.646] | equal |
| msh_write_tet4 | write | 1 | 5 | 54.2 ms [53.5 ms, 56.4 ms] | 24.7 ms [24.3 ms, 24.9 ms] | 0.454 [0.438, 0.463] | equal |
| parser_chain | evaluate_last | 5 | 5 | 1.97 s [1.96 s, 1.97 s] | 33.7 µs [33.2 µs, 33.8 µs] | 0.0000171 [0.0000168, 0.0000172] | equal |
| parser_dat | evaluate | 200 | 5 | 2.92 s [2.91 s, 2.95 s] | 2.06 s [2.05 s, 2.10 s] | 0.705 [0.700, 0.718] | equal |
| partition_rcb_tet4 | partition | 1 | 5 | 78.5 ms [78.1 ms, 78.7 ms] | 62.4 ms [62.3 ms, 62.4 ms] | 0.795 [0.791, 0.799] | equal |
| periodic_tet4 | make_periodic | 1 | 5 | 6.29 ms [6.27 ms, 6.30 ms] | 6.27 ms [6.26 ms, 6.29 ms] | 0.999 [0.993, 1.00] | equal |
| py_twin_explicit_tri3 | solve_step | 400 | 5 | 1.84 s [1.84 s, 1.84 s] | 1.16 s [1.16 s, 1.16 s] | 0.630 [0.629, 0.631] | equal |

Table "suite_setup, distributed cases at 1, 2, 4, 8 ranks" (part B, selected
phases):

| case | phase | ranks | n | before | head | head / before | head ranks x threads | time | / head on as many cores (unpaired) | digests |
|---|---|---|---|---|---|---|---|---|---|---|
| dist_mesh_tet4 | distribute | 2 | 5 | 816 ms [809 ms, 820 ms] | 274 ms [273 ms, 275 ms] | 0.337 [0.333, 0.338] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 4 | 5 | 932 ms [926 ms, 941 ms] | 344 ms [343 ms, 345 ms] | 0.368 [0.366, 0.372] |  |  |  | equal |
| dist_mesh_tet4 | distribute | 8 | 5 | 1.09 s [1.08 s, 1.10 s] | 427 ms [425 ms, 427 ms] | 0.390 [0.389, 0.395] |  |  |  | equal |
| dist_mesh_tet4 | partition | 2 | 5 | 676 ms [675 ms, 678 ms] | 197 ms [197 ms, 197 ms] | 0.291 [0.290, 0.292] |  |  |  | equal |
| dist_mesh_tet4 | partition | 4 | 5 | 755 ms [754 ms, 766 ms] | 276 ms [275 ms, 277 ms] | 0.365 [0.361, 0.366] |  |  |  | equal |
| dist_mesh_tet4 | partition | 8 | 5 | 837 ms [836 ms, 843 ms] | 355 ms [355 ms, 357 ms] | 0.425 [0.422, 0.427] |  |  |  | equal |
| explicit_step_dist_tet4 | distribute | 2 | 5 | 817 ms [815 ms, 825 ms] | 274 ms [274 ms, 275 ms] | 0.336 [0.332, 0.337] | 2 x 4 | 274 ms [274 ms, 275 ms] | 0.641 | equal |
| explicit_step_dist_tet4 | distribute | 4 | 5 | 936 ms [933 ms, 937 ms] | 344 ms [343 ms, 345 ms] | 0.368 [0.366, 0.370] | 4 x 2 | 344 ms [342 ms, 345 ms] | 0.805 | equal |
| explicit_step_dist_tet4 | distribute | 8 | 5 | 1.09 s [1.08 s, 1.10 s] | 428 ms [427 ms, 432 ms] | 0.393 [0.389, 0.396] |  |  |  | equal |
| explicit_step_dist_tet4 | energies | 1 | 5 | 3.14 s [3.12 s, 3.14 s] | 631 ms [631 ms, 635 ms] | 0.202 [0.201, 0.203] |  |  |  | equal |
| explicit_step_dist_tet4 | energies | 2 | 5 | 1.59 s [1.58 s, 1.59 s] | 321 ms [321 ms, 323 ms] | 0.202 [0.202, 0.204] | 2 x 4 | 321 ms [318 ms, 325 ms] | 3.18 | equal |
| explicit_step_dist_tet4 | energies | 4 | 5 | 853 ms [851 ms, 855 ms] | 163 ms [162 ms, 163 ms] | 0.191 [0.190, 0.192] | 4 x 2 | 163 ms [162 ms, 163 ms] | 1.61 | equal |
| explicit_step_dist_tet4 | energies | 8 | 5 | 523 ms [521 ms, 527 ms] | 101 ms [97.0 ms, 101 ms] | 0.193 [0.186, 0.193] |  |  |  | equal |
| explicit_step_dist_tet4 | partition | 2 | 5 | 675 ms [672 ms, 683 ms] | 197 ms [197 ms, 197 ms] | 0.291 [0.288, 0.294] | 2 x 4 | 197 ms [196 ms, 197 ms] | 0.552 | equal |
| explicit_step_dist_tet4 | partition | 4 | 5 | 757 ms [755 ms, 759 ms] | 276 ms [275 ms, 276 ms] | 0.365 [0.363, 0.366] | 4 x 2 | 277 ms [275 ms, 278 ms] | 0.777 | equal |
| explicit_step_dist_tet4 | partition | 8 | 5 | 838 ms [835 ms, 843 ms] | 357 ms [356 ms, 360 ms] | 0.425 [0.423, 0.429] |  |  |  | equal |
| explicit_step_dist_tet4 | solve_step | 1 | 5 | 1.30 s [1.29 s, 1.31 s] | 874 ms [867 ms, 882 ms] | 0.677 [0.663, 0.678] |  |  |  | equal |
| explicit_step_dist_tet4 | solve_step | 2 | 5 | 969 ms [944 ms, 1.14 s] | 541 ms [539 ms, 543 ms] | 0.558 [0.472, 0.571] | 2 x 4 | 456 ms [453 ms, 464 ms] | 0.911 | equal |
| explicit_step_dist_tet4 | solve_step | 4 | 5 | 802 ms [798 ms, 807 ms] | 463 ms [459 ms, 468 ms] | 0.579 [0.572, 0.582] | 4 x 2 | 474 ms [471 ms, 479 ms] | 0.947 | equal |
| explicit_step_dist_tet4 | solve_step | 8 | 5 | 835 ms [834 ms, 848 ms] | 501 ms [495 ms, 502 ms] | 0.597 [0.583, 0.601] |  |  |  | equal |
| ghost_elements_tet4 | distribute | 2 | 5 | 819 ms [809 ms, 823 ms] | 274 ms [274 ms, 275 ms] | 0.336 [0.333, 0.339] |  |  |  | equal |
| ghost_elements_tet4 | distribute | 4 | 5 | 933 ms [927 ms, 937 ms] | 345 ms [344 ms, 347 ms] | 0.370 [0.367, 0.374] |  |  |  | equal |
| ghost_elements_tet4 | distribute | 8 | 5 | 1.08 s [1.08 s, 1.10 s] | 427 ms [425 ms, 431 ms] | 0.392 [0.390, 0.397] |  |  |  | equal |
| ghost_elements_tet4 | partition | 2 | 5 | 676 ms [673 ms, 684 ms] | 197 ms [196 ms, 197 ms] | 0.290 [0.288, 0.292] |  |  |  | equal |
| ghost_elements_tet4 | partition | 4 | 5 | 755 ms [754 ms, 764 ms] | 277 ms [276 ms, 279 ms] | 0.367 [0.362, 0.369] |  |  |  | equal |
| ghost_elements_tet4 | partition | 8 | 5 | 837 ms [834 ms, 855 ms] | 356 ms [354 ms, 357 ms] | 0.424 [0.418, 0.428] |  |  |  | equal |
| ghost_elements_tet4 | synchronize | 1 | 5 | 0.240 µs [0.230 µs, 0.260 µs] | 0.250 µs [0.240 µs, 0.260 µs] | 1.04 [0.923, 1.13] |  |  |  | equal |
| ghost_elements_tet4 | synchronize | 2 | 5 | 779 ms [774 ms, 804 ms] | 666 ms [651 ms, 674 ms] | 0.851 [0.839, 0.855] |  |  |  | equal |
| ghost_elements_tet4 | synchronize | 4 | 5 | 872 ms [861 ms, 888 ms] | 748 ms [727 ms, 758 ms] | 0.847 [0.834, 0.870] |  |  |  | equal |
| ghost_elements_tet4 | synchronize | 8 | 5 | 889 ms [877 ms, 893 ms] | 754 ms [752 ms, 758 ms] | 0.849 [0.846, 0.860] |  |  |  | equal |
| ghost_nodes_tet4 | distribute | 2 | 5 | 811 ms [806 ms, 818 ms] | 274 ms [273 ms, 275 ms] | 0.339 [0.334, 0.341] |  |  |  | equal |
| ghost_nodes_tet4 | distribute | 4 | 5 | 931 ms [929 ms, 937 ms] | 344 ms [343 ms, 345 ms] | 0.369 [0.367, 0.371] |  |  |  | equal |
| ghost_nodes_tet4 | distribute | 8 | 5 | 1.09 s [1.09 s, 1.11 s] | 427 ms [426 ms, 429 ms] | 0.392 [0.382, 0.394] |  |  |  | equal |
| ghost_nodes_tet4 | partition | 2 | 5 | 675 ms [671 ms, 681 ms] | 197 ms [196 ms, 198 ms] | 0.292 [0.289, 0.295] |  |  |  | equal |
| ghost_nodes_tet4 | partition | 4 | 5 | 758 ms [755 ms, 760 ms] | 276 ms [275 ms, 277 ms] | 0.365 [0.364, 0.365] |  |  |  | equal |
| ghost_nodes_tet4 | partition | 8 | 5 | 841 ms [834 ms, 857 ms] | 357 ms [354 ms, 357 ms] | 0.424 [0.417, 0.428] |  |  |  | equal |
| ghost_nodes_tet4 | reduce_synchronize | 1 | 5 | 76.2 ms [75.9 ms, 76.3 ms] | 77.0 ms [76.9 ms, 77.3 ms] | 1.01 [1.01, 1.02] |  |  |  | equal |
| ghost_nodes_tet4 | reduce_synchronize | 2 | 5 | 310 ms [303 ms, 322 ms] | 278 ms [263 ms, 278 ms] | 0.897 [0.817, 0.916] |  |  |  | equal |
| ghost_nodes_tet4 | reduce_synchronize | 4 | 5 | 341 ms [336 ms, 359 ms] | 295 ms [283 ms, 299 ms] | 0.834 [0.830, 0.882] |  |  |  | equal |
| ghost_nodes_tet4 | reduce_synchronize | 8 | 5 | 377 ms [372 ms, 386 ms] | 307 ms [300 ms, 336 ms] | 0.807 [0.795, 0.895] |  |  |  | equal |
| reduce_scalars | all_reduce_2 | 1 | 5 | 114 ms [113 ms, 118 ms] | 60.3 ms [59.6 ms, 63.2 ms] | 0.527 [0.507, 0.557] |  |  |  | equal |
| reduce_scalars | all_reduce_2 | 2 | 5 | 268 ms [258 ms, 271 ms] | 164 ms [160 ms, 168 ms] | 0.604 [0.592, 0.652] |  |  |  | equal |
| reduce_scalars | all_reduce_2 | 4 | 5 | 486 ms [484 ms, 494 ms] | 274 ms [271 ms, 276 ms] | 0.564 [0.548, 0.568] |  |  |  | equal |
| reduce_scalars | all_reduce_2 | 8 | 5 | 1.03 s [1.01 s, 1.14 s] | 561 ms [551 ms, 625 ms] | 0.540 [0.507, 0.606] |  |  |  | equal |
| reduce_scalars | all_reduce_4096 | 1 | 5 | 151 ms [151 ms, 152 ms] | 154 ms [153 ms, 154 ms] | 1.01 [1.01, 1.02] |  |  |  | equal |
| reduce_scalars | all_reduce_4096 | 2 | 5 | 468 ms [465 ms, 470 ms] | 471 ms [469 ms, 472 ms] | 1.01 [1.000, 1.02] |  |  |  | equal |
| reduce_scalars | all_reduce_4096 | 4 | 5 | 1.10 s [1.09 s, 1.54 s] | 1.11 s [1.10 s, 1.15 s] | 0.999 [0.719, 1.05] |  |  |  | equal |
| reduce_scalars | all_reduce_4096 | 8 | 5 | 7.47 s [7.36 s, 7.54 s] | 7.32 s [7.21 s, 7.53 s] | 0.994 [0.975, 1.01] |  |  |  | equal |

- Port / upstream: `init_full` 0.075–0.187, `dist_mesh` 0.095–0.107,
  `read_mesh` 0.33–0.36, `before_step` 0.33–0.36, the static solve
  0.795–0.918 (1-rank `dist_mesh` is a no-op in both codes).
- `dist_mesh_tet4` at size `l`, head / before: 0.256–0.285 for
  `distribute`; the head's `distribute` takes 2.09 / 2.34 / 2.70 s at
  2 / 4 / 8 ranks.
- Slower than before: `cohesive_insert_tet4` `update_insertion_facets`
  2.49 [1.35, 3.21] (402 µs → 998 µs) and `erase_purify_tet4`
  `erase_elements` 1.06 [1.03, 1.06]; at 1 rank `ghost_nodes_tet4`
  `reduce_synchronize` and `reduce_scalars` `all_reduce_4096` 1.01 [1.01,
  1.02].

### Before the final campaign (12–13 November)

- `init_full` (mesh facets, model and cohesive set-up): the port takes
  0.14–0.50× upstream's time on the mid-size meshes, 0.24–0.61× in the
  scaling series and 0.18–0.26× at full size (1.91 against 7.42 s at 64
  ranks). On the development machine (appendix), upstream's grew faster
  than the mesh at one rank (∝ N^1.3–1.45), an observation about upstream
  alone that no conclusion of this document rests on.
- The mesh distribution (`dist_mesh`): rank 0 reads and distributes the mesh
  in both codes. The port's grows with the rank count: 32 / 84 / 151 s at
  8 / 32 / 64 ranks at full size (upstream 66 / 81 s at 32 / 64), 6.0 →
  14.5 s from 8 to 64 ranks on h = 0.04 across hosts.
- `read_mesh` of the JOSS mesh: 5.4–5.7 s for the port, 15.3–15.6 s for
  upstream.
- `before_step` (lumped mass, step-0 energies, stable time step): 2.9 s
  (port) and 2.3 s (upstream) at 64 ranks; about 25 s for both at h = 0.04
  on one rank.

## Build configuration

The build configurations of the release profile were studied in the
performance campaign on an AWS `m7a.2xlarge` (2026-11-16) and not adopted;
the summary moved to `docs/benchmarks/perf-campaign/README.md`, the data
and tables are in `docs/benchmarks/perf-campaign/build-profile/`. Every
measurement of this document uses the workspace release profile
(section "Method").

## Known slow paths

From the final campaign's tables (2026-11-17 UTC, AWS `m7a.2xlarge`, data
`docs/benchmarks/perf-campaign/final/`).

| Slow path | Measurement | Where |
|---|---|---|
| The parallel overhead at 8 ranks | loop port / upstream 0.31–0.36 at 8 ranks against 0.22–0.30 at 1 to 4; loop efficiency at 8 ranks on h = 0.06 before insertion 0.36 (port), 0.42 (port before), 0.55 (upstream) | part C |
| Threads against ranks on 8 cores | 2 ranks × 4 threads take 1.05–1.22× the loop of 8 ranks, 4 × 2 0.993–1.06× | part C |
| The static MUMPS solve at 8 ranks | port / upstream 0.91–1.11 at 8 ranks (1.11 [0.977, 1.60] on h = 0.0476), 0.79–0.96 at 1 to 4 | parts C, D |
| Threads in implicit and static solves | 8 threads / 1: 0.789–1.00 of the `solve_step` time of the implicit and static cases of `suite_solid`; the `faer` backend 1.00 | part B |
| The symbolic analysis of the sparse backends | head / before 1.12 [1.09, 1.13] (`eigen`), 1.11 [1.10, 1.12] (`faer`) | part D |
| `update_insertion_facets` | head / before 2.49 [1.35, 3.21] (402 µs → 998 µs per call, `cohesive_insert_tet4`) | part B |
| Small messages of the communication layer | 6.46× raw MPI for an 8-byte ping-pong over shared memory; 2.52× for 16 MiB | part F |
| The mesh distribution from rank 0 | the head's `distribute` of 1,296,000 Tet4 grows from 2.09 s at 2 ranks to 2.70 s at 8 (0.095–0.107× upstream's `dist_mesh` in the cohesive driver) | part F |
| Collectives across hosts | 20.5 collectives per step and rank after D-126 against upstream's 8.4 (51.6 before); across four hosts the final revision's loop is 0.646 of its one-machine loop, upstream's 0.523–0.534 | net round, multi-host runs |
| 128 ranks on one 128-core machine | the loop from 64 to 128 ranks: port 846 → 835 s, upstream 2,430 → 1,948 s; step and check 2.37 → 4.62 core-µs per element and step (port) | part G |

Limits of the measurements:

- Stage 1 of the final campaign ran on 8-core machines. Part G and the
  multi-host runs give 32 to 128 ranks on one 128-core machine and on four
  32-core hosts, one run per configuration; nothing beyond 128 ranks or
  four hosts was measured.
- The suites have no C++ twin; their tables compare the port with itself.
- No hardware counters and no per-rank timings: the causes of the parallel
  overheads are not established.

### As published before the final campaign

Each is measured; none was worked on unless the row says so.

| Slow path | Measurement | Where |
|---|---|---|
| The insertion check after insertions start | the largest phase at full size in both codes: 53 % of the port's wall at 64 ranks, 75 % at 8; about twice its pre-insertion cost per element and step; profiled and reworked in the performance campaign (`docs/benchmarks/perf-campaign/README.md`, "The insertion check"); the full-size run was not repeated | full-size run |
| The port's lead shrinks once the mesh fragments | per element and step from about 2.1× to about 1.4× on h = 0.06 (full schedule ×1.91 / ×1.65 for the port at 1 / 8 ranks against ×1.29 / ×1.09 for upstream); not profiled | mid-size rehearsal |
| The explicit step's parallel overhead | 0.45 → 0.82 → 1.58 core-µs per element and step at 8 → 32 → 64 ranks, 1.14 before insertion at 64; the port's step goes from 0.44× upstream's at 1 rank to 0.76× at 8 and 0.82× at 64 | scaling series, full-size run |
| The mesh distribution from rank 0 | grows by about 2.1 s per added rank at full size; 1.86× upstream's at 64 ranks | full-size run |
| The static solve phase | 1.02–1.31× upstream's at 13 of 14 paired points (same MUMPS, plus the port's assembly) | scaling series |
| The largest rank's memory | 1.27× upstream's at 64 ranks; MUMPS's allocation stays through the loop in both codes | full-size run |
| The SparseLU translation | 1.6–2.0× upstream's SparseLU | static solves |
| MPI buffer copies | large messages 3.5–4.8× raw MPI over shared memory, 6.4–11.2× in one job | MPI layer |
| The shipped C++ build on the explicit-step kernels | the default install takes 1.01 to 1.51 times the strict one's step time there; the port is compared with both | explicit step |

Limits of the measurements:

- One run per point on AWS, so no spread at full size; upstream's static
  solve varied by 1.42× between two runs.
- Boat sandboxes are shared virtual machines; at 8 ranks every vCPU runs a
  rank.
- No per-rank timings and no hardware counters: the causes given for the
  parallel overheads fit the data and are not proven.
- Not measured: the C++ full-size run at 8 ranks, the elastic series at full
  size, more than 64 ranks, other interconnects, a weak-scaling series on a
  dedicated node.

## What remains

From the final campaign's tables:

- The 8-rank loop: the port's loop efficiency at 8 ranks (0.36 on h = 0.06)
  is below upstream's (0.55), and its lead over upstream is smallest there
  in every series of part C. At full size on one 128-core machine the
  port's loop is 0.35 of upstream's at 32 and 64 ranks and 0.43 at 128,
  where the machine stops scaling (part G).
- Collectives across hosts: after D-126 the port makes 20.5 collectives
  per step and rank against upstream's 8.4, mostly the agreements before
  point-to-point exchanges and the commit points of the step; its
  four-host loop gains less over its one-machine loop (0.646) than
  upstream's (0.523–0.534). The ideas left are listed in
  `docs/benchmarks/perf-campaign/net/README.md`, section 6, and
  `docs/reports/04-possible-improvements.md`.
- Threads inside a rank do not replace ranks on these machines: 2 × 4 and
  4 × 2 take 0.993–1.22× the loop of 8 ranks.
- The static solve phase: the port's assembly and MUMPS hand-over are
  0.79–0.96× upstream's at 1 to 4 ranks; at 8 ranks the ratio reaches 1 and
  its range is wide. Without MUMPS, the opt-in nested-dissection
  ordering of the SparseLU translation (D-125) takes 0.186–0.412× the
  default ordering's static solve at one thread; it was not measured with
  threads.
- Three phases are slower than before the campaign: the sparse backends'
  symbolic analysis (1.11–1.12×), `update_insertion_facets` (2.49×) and
  `erase_elements` (1.06×); the Python `get_stress` and built-in functor
  cases 1.03–1.04×.
- The communication layer's fixed cost per message (6.46× raw MPI at 8
  bytes over shared memory).
- Not done in the campaign: GPU kernels and distributed mesh input
  (`docs/reports/04-possible-improvements.md`, P4.3 and P4.6).

## Appendix: development-machine records

These were measured on the development Mac (Apple M4 Pro: 10 performance and
4 efficiency cores, 24 GB), natively or in its aarch64 Linux VM (OrbStack,
Docker containers with a CPU and memory limit), on 2026-11-10 and
2026-11-11, while the port was being written. They are records of the
development. No conclusion of this document rests on them, and they are not
compared with the x86_64 numbers.

Mesh set-up paths (native, rustc 1.99.1, release; `cargo run --release -p
akantu-mesh --example facet_scaling`): structured cubes of n³ cells split
into 6 tetrahedra; one warm-up and 5 repetitions, medians.
`init_mesh_facets` took 0.0162, 0.1423 and 1.2337 s on 3,072, 24,576 and
196,608 tetrahedra (×8.8 and ×8.7 per ×8 elements), erasing a third of the
elements 0.0069, 0.0505 and 0.3915 s; the node-to-element map and the facet
fills scale about ×8. The remaining per-facet allocation is the
`Vec<Element>` row of `element_to_subelement`, as upstream's
`std::vector<Element>`. No x86_64 measurement of these paths exists.

MPI latency (the VM, container 4 CPUs, 6 GB, Open MPI 5.0.7 with single
copy): after the back-off and packing changes of the communication layer,
akantu-comm took 6.5× raw MPI for an 8-byte ping-pong over shared memory
(about 0.45 µs fixed cost), 3.6× for 64 KiB, 3.7× for 1 MiB, 3.6–4.1× for
16 MiB, 1.9× and 2.3–3.5× for the 2- and 4-rank halos; over TCP 1.3–2.5×.
Before the back-off change a large message paid 100 µs sleeps per poll after
1,024 polls (40–800× raw); before the bulk packing the per-value packing
limited large payloads to about 2.2 GB/s. The x86_64 series above replaces
these numbers.

MUMPS sizing (the VM, container 4 CPUs, 6 GB; `crates/akantu-linalg/examples/mumps_scaling.rs`,
raw data `fixtures/solver/mumps/scaling-20261111.tsv`): on 3-D Laplacian and
linear-elasticity systems of up to 216,000 DOFs at 1 / 2 / 4 ranks the
summed MUMPS memory grew 35–40 % from 1 to 4 ranks while the largest rank's
fell about 2.7×, and re-solves with existing factors were 100–200× cheaper
than the factorisation. The full-size runs measured what this sized.

C++ cohesive baselines (the VM, a `RelWithDebInfo` build with Akantu's
assertions on and FMA contraction; `docs/benchmarks/dev-machine/cpp-baseline-m4-2026-11-10/`,
tables by `benchmarks/cohesive-perf/scripts/analyze.py`): before insertion
upstream's check already exceeded its step (check / step 2.0, 3.3 and 4.0 at
h = 0.12, 0.06, 0.04), `init_full` grew as N^1.3–1.45, the static MUMPS
solve as N^1.8 in time and N^1.2 in memory at one rank, and h = 0.03 (689 k
DOFs) was killed at 12 GB during the static solve. On the full schedule at
h = 0.12 the node-to-element rebuild took about 85 % of the check (×7.3 with
the JOSS emulation). The x86_64 baselines and the JOSS-mesh runs replace
these numbers; a per-element comparison of this build with jed's published
data gave a ratio of 2.27 between two different configurations, which says
nothing about the hardware.
