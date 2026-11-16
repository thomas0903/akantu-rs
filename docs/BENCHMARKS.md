# Benchmarks

This document holds the measured performance of akantu-rs against the C++
reference (Akantu at `35a85178`), with the machine, date, data and command
behind every number. The short report for the Akantu team,
`docs/reports/02-benchmarks-and-performance.md`, summarises it.

The main results: on one core the port's explicit step takes 0.31–0.60× the
time of the C++ step built with the same arithmetic (five meshes, three
element types). In the cohesive benchmark at 1 to 8 ranks the explicit loop
takes 0.39–0.59× upstream's time. On the JOSS mesh (4.39 million
tetrahedra, 5,000 steps with fracture) on one 64-core node the port's run
took 0.875× upstream's wall time at 64 ranks and 0.851× at 32. The port is
slower in the mesh distribution (1.26–1.86× at full size) and in the static
MUMPS solve phase (about 1.1× in the paired series; no gap is established at
full size). Both codes need about the same memory, most of it MUMPS's; at 64
ranks the port's largest rank peaks at 1.27× upstream's. No performance work
was done before parity was reached; none of the slow paths
listed at the end was worked on.

## Where the numbers come from

Every comparison of the port with the C++ reference was measured on x86_64
cloud machines, with both codes on the same machine in the same session.
The development Mac (an Apple M4 Pro, natively or in its aarch64 Linux VM)
was used for development records only; they are in the appendix and no
conclusion rests on them.

| Machine | CPU | Dates | Measured there | Data |
|---|---|---|---|---|
| AWS `r7a.16xlarge`, one node, 64 cores, one thread per core, one NUMA node, 493 GiB visible | AMD EPYC 9R14 | 2026-11-12, 2026-11-13 | the full-size run (both codes at 64 and 32 ranks, the port at 8); the port's rerun after UD-1280 | `docs/benchmarks/c1-2026-11-12/`, `c1-rerun-2026-11-13/` |
| 8 × AWS `r7a.2xlarge` (8 cores, 64 GiB each), one cluster placement group | AMD EPYC 9R14 | 2026-11-13 | the multi-node runs (the port only, up to 64 ranks) | `docs/benchmarks/mn-2026-11-13/` |
| boat.dev `large` sandbox, 8 vCPU, 15.6 GiB, Ubuntu 24.04, kernel 6.8.0-117 | AMD EPYC 9455 | 2026-11-12, 2026-11-13 | the mid-size rehearsal and the MUMPS series; the weak and strong scaling series at 1–8 ranks | `docs/benchmarks/midsize-2026-11-12/`, `scaling-2026-11-13/` |
| same type | AMD Ryzen 7 9700X | 2026-11-11, 2026-11-12 | the explicit-step kernels against both C++ builds; the C++ baselines on x86_64 | `docs/benchmarks/explicit-step/final/`, `cpp-baseline-x86-2026-11-11/` |
| same type | AMD Ryzen 9 9950X | 2026-11-11 | the explicit-step development series; the serial driver against both C++ builds | `docs/benchmarks/explicit-step/development/`, `serial-driver-2026-11-11/` |
| boat.dev `default` sandbox, 4 vCPU, container 4 CPUs and 6 GiB | AMD Ryzen 9 9950X | 2026-11-14 | MPI latency against raw MPI | `docs/benchmarks/mpi-latency-2026-11-14/` |
| same type | not recorded | 2026-11-12 | the MPI performance gate measured in one job | numbers in this document only |
| development Mac: Apple M4 Pro, 14 cores, 24 GB, natively and in an aarch64 Linux VM | Apple M4 Pro | 2026-11-10, 2026-11-11 | development records only (appendix) | `docs/benchmarks/dev-machine/` |

Dates are the project's local dates (UTC+11); the provenance files of the
data record UTC times. boat.dev sandboxes are shared virtual machines. The
upstream JOSS study's
published timings (EPFL's jed cluster, Intel Xeon 8360Y) are committed
verbatim in `benchmarks/cohesive-perf/results/published-jed/`.
`docs/benchmarks/README.md` lists every data directory.

## Method

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
ghost copies at P > 1 (UD-347). At P > 1 the port's results are identical
from run to run with a fixed partition and solver, while upstream's vary in
the last digits (the distributed MUMPS solve and `MPI_Waitany` orders). Upstream's MUMPS memory-retry
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

### What was optimised

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

### The cohesive step and the benchmark driver

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

### What remains in the serial step

Ranked by share of the profile; none was worked on.

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
   upstream's behaviour; avoiding them needs double buffering, which would
   change the buffer identity that zero-copy Python views hold (D-047).
7. The insertion check's stress interpolation used the general product per
   point; reworked in the performance campaign (next section).

### The explicit step after the lane kernels and the element-block pass (2026-11-16)

Items 1–4 of the list above were worked on with bitwise identical results
at every thread count and SIMD level
(`docs/benchmarks/perf-campaign/explicit/README.md`): the batch kernels
check their ids and connectivity once and run lane kernels (AVX2 at run
time, no FMA), `BᵀD` and the finite-deformation residual are integrated as
they are computed, `grad_u −= eigen_grad_u` is applied in the gradient's
lanes, and one elastic law on one element type runs gradient, stress and
integration per cache-sized block of elements. On an AMD EPYC 7B13 at one
thread the step takes 0.74–0.83 (small strain) and 0.52–0.72 (finite
deformation) of its time before this work, and 0.34–0.43 and 0.16–0.26
of the strict C++ time; at 2–8 threads 0.54–0.86. The fast mode
(`AKANTU_PRECISION=fast`) takes 0.84–0.99 of the bitwise step. Items 5 and
6 (per-node solver loops, history copies) remain the serial part of a
threaded step.

### The insertion check (performance campaign, 2026-11-16)

`check_cohesive_stress` (facet stresses, the laws' insertion decision, the
insertion) was profiled and reworked with bitwise identical results
(D-122). Machine, unless a table says otherwise: an AWS `m7a.2xlarge` (AMD EPYC
9R14, 8 cores, 32 GiB), one process pinned to one core of the four timing
cores (other jobs on the other four), before and after interleaved in
rotated order, 5 repetitions; "before" is the branch point `59dd2229`, both
binaries built with the default release profile on the same toolchain.
Raw data, commands and profiles: `docs/benchmarks/perf-campaign/cohesive-check/`.

What the profile showed (branch point, h = 0.12 driver, full schedule;
shares of all samples of a flat profile on the AWS machine, attributed to
callers with frame-pointer call graphs on a boat.dev development VM;
`profiles/aws-base-flat.txt`, `profiles/base-5000.txt`): the elemental interpolation's two small products
per element through the general product path, 31 % (the second product of
the linear tetrahedron at its facets' 3 × 4 points takes Eigen's `gebp`
route); the copy of the interpolated values into `facet_stress`, 14 %, and
its plan rebuilt with a sort after every insertion step, 3.6 %; the linear
law's check, 9 %; the mesh transaction's consistency check at each
insertion commit, 8 %.

Changes:

- the facet stresses are computed directly from each bulk element's stress
  tuple for every (facet, side) destination of the copy plan, in the
  plan's order (`akantu_fe::engine::PointInterpolation`): the by-element
  buffer (20 MB on this mesh) is no longer written and read per step (the
  first check still allocates it, and it stays allocated, so the peak
  memory is unchanged); the element's coefficients once per pair;
- the elemental interpolation selects both products' routes once per block
  and evaluates them with direct indexing (coefficient-based route, and the
  depth-1 `gebp` route as `+0 + (+0 + t)`), property-tested bitwise
  against the general product;
- the copy plan after an insertion is ordered with a last-writer table
  instead of a sort, and reads each facet's first neighbour once;
- the linear law's effective norm uses fixed-size loops for 2-D and 3-D;
- the mesh consistency check keeps the last block size it looked up.

Before insertion (h = 0.12 frozen mesh, 23,156 tetrahedra, 500 steps after
the static preload), ms per step:

| phase | before | after | after / before (paired) |
|---|---|---|---|
| `check_cohesive_stress` | 13.74 [13.65, 13.87] | 4.62 [4.62, 4.64] | 0.336 [0.333, 0.339] |
| `solve_step` | 3.63 [3.62, 3.65] | 3.50 [3.49, 3.51] | 0.965 [0.955, 0.968] |
| loop | 17.41 [17.32, 17.50] | 8.13 [8.12, 8.15] | 0.467 [0.464, 0.469] |

The check went from 593 to 200 ns per element and step. The explicit step
was not changed; its 3.5 % gain is consistent with the check no longer
writing and reading the by-element buffer every step (not proven).

In the insertion regime (the same mesh, the full schedule: 5000 steps,
insertions from step 710, 11,736 cohesive elements at the end), ms per step:

| phase | before | after | after / before (paired) |
|---|---|---|---|
| `check_cohesive_stress` | 21.29 [21.19, 21.40] | 11.32 [11.31, 11.83] | 0.533 [0.530, 0.553] |
| `solve_step` | 7.82 [7.80, 7.91] | 7.76 [7.74, 7.79] | 0.992 [0.978, 0.994] |
| loop | 29.13 [29.02, 29.27] | 19.08 [19.06, 19.63] | 0.655 [0.654, 0.671] |

The check went from 919 to 489 ns per element and step; every run of both
series gave the same energies, displacement norms and insertion counts.

A larger mesh (the x86_64 `cube-h0.06.msh`, 177,328 tetrahedra, without
the static preload, `rust_static_solve = false`, 200 steps; insertions start
at step 133, 16 cohesive elements at step 200), measured on the screening
machine (8-core AMD EPYC Milan, quiet, same method), ms per step:

| phase | before | after | after / before (paired) |
|---|---|---|---|
| `check_cohesive_stress` | 111.34 [110.51, 113.34] | 52.65 [52.44, 53.76] | 0.472 [0.465, 0.486] |
| `solve_step` | 28.31 [27.98, 28.76] | 28.46 [28.03, 28.67] | 1.002 [0.992, 1.015] |

628 → 297 ns per element and step.

The check's split after the change (the after binary's
`insertion_stats_rank0.csv`), ms per step: before insertion 2.51 stress
interpolation and 2.12 law checks; over the full schedule 4.80 stress
interpolation (with the copy plan rebuilt after every insertion step),
2.42 law checks and 4.08 insertion.

Remaining shares in the full schedule after the change (shares of a
frame-pointer call-graph profile on a boat.dev development VM, not times;
`profiles/src2-5000.txt`): stress interpolation 26.5 % of the run
(the fused loop streams `facet_stress`, 20 MB on this mesh, about the
memory traffic of writing it), the law check 12 %, the insertion 19 %, of
which the mesh consistency check 11.6 % and the inserter's topology work
the rest; the explicit step 41 %.

#### At 2, 4 and 8 ranks

The full schedule on h = 0.12 with the Scotch partition and the MUMPS static
solve (project image `akantu-rs-mumps:dev`), every rank on its own core.
Every rank now writes its phase times (`chrono_rank<r>.csv`) and the split
of its check (`insertion_stats_rank<r>.csv`). Screening on an 8-core EPYC
Milan machine (5 interleaved repetitions; the slowest rank), ms per step:

| ranks | check before | check after | after / before (paired) | of which insertion (before → after) |
|---|---|---|---|---|
| 2 | 17.01 | 11.81 | 0.695 [0.685, 0.716] | 8.85 → 8.71 |
| 4 | 8.28 | 5.89 | 0.713 [0.692, 0.717] | 4.53 → 4.46 |
| 8 | 4.56 | 3.37 | 0.737 [0.722, 0.771] | 2.62 → 2.62 |

- Per rank: the check's total and its interpolation and insertion parts
  are the same on every rank to within 1 %; only the law checks differ
  between ranks (0.25–0.28 ms at 8 ranks, 0.21–0.23 after). The ranks
  therefore wait in the synchronisations that close each part (the
  facet-stress exchange at the end of the interpolation, the insertion
  decision), and that wait is counted in those parts; with the cheaper
  local work the interpolation part went from 1.63 to 0.51 ms at 8 ranks.
- The insertion part is now the larger half of the check at every rank
  count, and it does not shrink with the change: the insertion decision
  runs on every step (two facet-synchronizer exchanges, a scan and a
  reduction, an order map of all facets, an agreement), and every
  insertion step runs the distributed transaction. At 2 ranks the insertion
  part (8.85 ms) is about twice the serial insertion phase (4.08 ms). An
  8-rank call-graph profile of the insertion part on the 8-core EPYC Milan
  development machine (shares, not times): MPI and kernel time
  31 %, the mesh consistency check 27 %, the DOF manager's global-number
  exchange 11 %.
- A first version of the change was slower at 8 ranks (×1.05): the copy
  plan's first-neighbour table was kept for one facet block only, and the
  not-ghost and ghost facet blocks alternate on a distributed mesh. Fixed
  before these measurements (one table per block).
- The MPI runs with the MUMPS static solve are not repeatable run to run
  (the final cohesive count varied by 11 between two runs of one binary at
  8 ranks), so they are compared on time only. With the port's own static
  solve (`rust_sparse_solver = eigen`) the runs are repeatable (two runs of
  each binary at 2 ranks) and the before and after `energies.csv` (17
  digits) and `insertions.csv` are identical at 2, 4 and 8 ranks
  (`parallel-bitwise/`).

The tables above use the baseline build. Builds with `-C target-cpu`
(`x86-64-v2`, `-v3`, `-v4`, `znver4`), LTO, one codegen unit,
`panic = "abort"` and PGO give the same bits (section "Build
configuration").

## Static solves and solvers

### The SparseLU translation against upstream's SparseLU

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

### MUMPS in the cohesive benchmark

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

### Small and mid sizes at 1 to 8 ranks

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

### The mid-size rehearsal

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

### Comparison with upstream's published data

Upstream's JOSS study ran the same mesh and the same 5,000-step schedule
with insertion (v5.0.4 Release with the JOSS patch, five runs per core
count) on EPFL's jed cluster (Intel Xeon 8360Y, two 36-core sockets per
node). Its timing files are committed verbatim in
`benchmarks/cohesive-perf/results/published-jed/`; the medians per core
count are in `docs/manual/src/appendix/joss.md`. jed: medians of the v5.0.4
cohesive file at the three core counts nearest to the full-size points.

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
cores on (several nodes) and the paper's elastic series at full size were
not run.

### Multi-node runs

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

### Point-to-point latency against raw MPI

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

The build configurations of the release profile and the code-generation
flags, timed against the workspace release profile (`opt-level = 3`, no
LTO, 16 codegen units, x86-64 baseline) and checked for bit identity.
Machine: an AWS `m7a.2xlarge` (AMD EPYC 9R14, Zen 4, 8 cores, 32 GiB),
2026-11-16, every run pinned to one core inside the machine's timing
slot (builds of other jobs ran on the other four cores), 5 interleaved
repetitions in rotated order; paired ratio variant / base, median
[min, max]. Data, method and the full tables:
`docs/benchmarks/perf-campaign/build-profile/`.

Table "Build configurations, paired ratio to the release profile":

| configuration | tet4 ss step | tet4 fd step | hex8 ss step | hex8 fd step | driver explicit step | driver insertion check | driver static solve | driver wall | facet set-up |
|---|---|---|---|---|---|---|---|---|---|
| `cgu1` | 1.098 [1.074, 1.103] | 1.061 [1.057, 1.073] | 1.019 [1.013, 1.031] | 0.999 [0.996, 1.009] | 1.034 [1.016, 1.037] | 0.971 [0.960, 0.988] | 0.980 [0.974, 0.993] | 0.981 [0.978, 0.993] | 0.984 [0.983, 0.984] |
| `thin` | 0.991 [0.986, 1.013] | 0.997 [0.987, 1.003] | 0.999 [0.982, 1.006] | 0.996 [0.991, 1.002] | 1.003 [0.997, 1.010] | 0.999 [0.991, 1.012] | 0.996 [0.988, 1.016] | 0.998 [0.994, 1.013] | 0.987 [0.987, 0.989] |
| `fat` | 1.101 [1.085, 1.112] | 1.072 [1.056, 1.080] | 1.025 [1.021, 1.032] | 1.007 [1.000, 1.024] | 1.036 [1.032, 1.060] | 0.948 [0.941, 0.970] | 0.975 [0.969, 0.989] | 0.970 [0.967, 0.981] | 0.968 [0.966, 0.970] |
| `fat+cgu1` | 1.088 [1.077, 1.108] | 1.073 [1.060, 1.083] | 1.012 [0.984, 1.019] | 1.012 [0.992, 1.015] | 1.031 [1.027, 1.056] | 1.043 [1.030, 1.046] | 0.986 [0.978, 1.014] | 1.012 [1.008, 1.028] | 0.973 [0.971, 0.977] |
| `v3` | 0.964 [0.960, 1.022] | 0.977 [0.969, 0.994] | 0.964 [0.959, 0.965] | 0.965 [0.934, 0.968] | 0.972 [0.949, 0.974] | 0.970 [0.958, 0.987] | 0.995 [0.973, 1.017] | 0.984 [0.973, 0.997] | 1.052 [1.051, 1.089] |
| `v4` | 1.010 [0.983, 1.081] | 1.011 [1.006, 1.022] | 1.367 [1.356, 1.382] | 1.193 [1.177, 1.198] | 0.991 [0.987, 1.012] | 0.935 [0.932, 0.958] | 0.992 [0.985, 1.002] | 0.972 [0.966, 0.983] | 1.052 [1.050, 1.055] |
| `znver4` | 0.968 [0.930, 0.998] | 0.981 [0.968, 1.014] | 0.962 [0.945, 0.968] | 0.967 [0.939, 0.974] | 1.011 [0.969, 1.029] | 0.940 [0.922, 0.964] | 1.023 [1.014, 1.030] | 0.984 [0.979, 1.000] | 1.029 [1.027, 1.031] |
| `abort` | 0.996 [0.978, 1.004] | 0.995 [0.986, 1.002] | 0.996 [0.981, 1.000] | 1.003 [1.002, 1.005] | 0.995 [0.994, 1.000] | 0.978 [0.972, 0.992] | 0.997 [0.991, 1.031] | 0.988 [0.984, 1.015] | 0.977 [0.971, 1.029] |
| `pgo2-base` | 0.950 [0.938, 0.963] | 0.962 [0.956, 0.974] | 0.981 [0.975, 1.011] | 0.975 [0.973, 0.980] | 0.854 [0.845, 0.866] | 0.887 [0.852, 0.906] | 0.957 [0.934, 0.992] | 0.917 [0.914, 0.922] | 0.958 [0.955, 0.966] |
| `pgo2-fat` | 0.936 [0.921, 0.952] | 0.952 [0.943, 0.955] | 0.981 [0.974, 1.019] | 0.966 [0.960, 0.981] | 0.843 [0.834, 0.862] | 0.847 [0.811, 0.867] | 0.968 [0.953, 0.983] | 0.904 [0.882, 0.923] | 1.033 [1.031, 1.041] |
| `base@mimalloc` | 1.040 [1.020, 1.070] | 1.019 [1.010, 1.030] | 1.052 [1.027, 1.062] | 1.100 [1.089, 1.106] | 1.008 [0.952, 1.035] | 1.003 [0.990, 1.045] | 1.015 [0.995, 1.026] | 1.006 [0.996, 1.033] | 0.958 [0.949, 0.963] |
| `nobc` | 0.899 [0.880, 0.920] | 0.938 [0.934, 0.949] | 0.975 [0.971, 1.026] | 0.992 [0.981, 1.005] | 0.938 [0.919, 0.958] | 0.999 [0.995, 1.006] | 0.997 [0.994, 1.002] | 0.997 [0.986, 0.999] | 1.001 [0.994, 1.005] |

- No release-profile setting gains: one codegen unit and fat LTO make the
  explicit step 6–10 % slower on tet4, thin LTO changes nothing, and fat
  LTO gains 2–5 % only in the insertion check, the static solve and the
  mesh set-up. The clean build of the three timing binaries takes 1.3×
  (thin LTO or one codegen unit) to 2.5× (fat LTO) as long. The workspace
  profile stays as it is.
- `x86-64-v3` and `znver4` take 0.96–0.98 of the explicit-step time and
  0.98 of the cohesive driver's wall time, but 1.03–1.05 of the facet
  set-up's.
  `x86-64-v4` is 1.19–1.37 times slower on hex8: LLVM's generic v4 cost
  model vectorises the strided loads of `fast::gradient` with AVX-512
  gathers and scatters, which are slow on AMD Zen 4/5 (the hex8 `stress`
  phase takes twice the time). `znver4` (the Zen 4 cost model) does not
  slow down.
- PGO, trained on cases disjoint from the measured ones, gains most: the
  cohesive driver 0.92 (0.90 with fat LTO), its insertion check 0.85–0.89,
  its explicit step 0.84–0.85, the explicit-step harness 0.95–0.98, the
  untrained mesh set-up 0.96. A training set without the cohesive
  material made the insertion check 1.35 times slower: PGO helps only
  where the training covers the code.
- `panic = "abort"` changes nothing measurable (0.98–1.00). mimalloc and
  jemalloc (preloaded) slow the explicit step by 2–10 %; mimalloc lowers
  the driver's peak RSS by 11 % and speeds up element removal (0.65).
- Every configuration gives the same bits: the CI `test` and `release`
  tiers pass when built with `x86-64-v2`, `-v3`, `-v4` and `znver4`, the
  `release` tier with fat or thin LTO and one codegen unit; the
  explicit-step digests, the 5,000-step fragmenting cohesive run and the
  71 cases of the benchmark suites are identical for the configurations
  checked, PGO included (list in the study's README). `f64::min`/`max` give the same bits at every level; only
  the choice between two NaN operands can differ, as it already does
  between debug and release builds. The manual states which levels keep
  the results (`docs/manual/src/getting-started.md`).
- Bounds checks: removing those of `fast::gradient`, `fast::btd` and the
  internal-force scatter by hand made the tet4 explicit step 0.90 (small
  strain) and 0.94 (finite deformation) and the driver's explicit step
  0.94, with the same results. The kernels with the most checks are
  listed with their restructuring in the study's README.

## Known slow paths

Each is measured; none was worked on unless the row says so.

| Slow path | Measurement | Where |
|---|---|---|
| The insertion check after insertions start | the largest phase at full size in both codes: 53 % of the port's wall at 64 ranks, 75 % at 8; about twice its pre-insertion cost per element and step; profiled and reworked in the performance campaign (explicit dynamics, "The insertion check"); the full-size run was not repeated | full-size run |
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
