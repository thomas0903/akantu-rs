<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# The performance campaign's development records (16–17 November 2026)

The changes of the performance campaign were measured branch by branch
while they were made. These are development records: each compares a
change with its predecessor, on development machines or on AWS machines
shared with other jobs, often with the port's own `before` built from
another revision than the final campaign's. They are not the campaign's
results. The results are the final campaign's tables in
`docs/BENCHMARKS.md` (data `final/`); where a record and a final table
differ, the final table holds.

The summaries below were written into `docs/BENCHMARKS.md` as the branches
were merged and moved here, unchanged, when the document was rewritten
around the final campaign. Each directory holds the data, commands and
method of its branch:

| Directory | What |
|---|---|
| `assembly/` | sparse assembly, profile, matrix-vector product and solver hand-over |
| `baseline/` | the campaign's starting point: digests, meshes, oracle build, profiles |
| `build-profile/` | the build configuration study |
| `cohesive-check/` | the cohesive insertion check |
| `cohesive-insert/` | the cohesive insertion step |
| `cohesive-threads/` | the cohesive model on threads |
| `contact-geometry/` | contact detection, spatial grid and embedded intersection |
| `distribution/` | distribution, communication layer and the parallel explicit step |
| `exec/` | the execution policy (D-123) |
| `explicit/` | the explicit step: lane kernels, element-block pass, fast kernels |
| `io-python/` | input parser, mesh I/O, dumpers and the Python boundary |
| `laws/` | constitutive laws and the heat law on fixed-size kernels |
| `small-products/` | small dense products and reductions in Eigen's order |
| `sparselu/`, `sparselu-threads/`, `sparselu-nd/` | the Eigen SparseLU translation, its threads, the nested-dissection ordering (D-125) |
| `step-state/` | repeated work in the solid model's step |
| `suite-solid/`, `suite-models/`, `suite-setup/` | the benchmark suites' baselines |
| `threaded-assembly/`, `threaded-assembly-2/`, `threaded-laws/`, `threads-rest/` | matrix assembly, laws and the rest of the step on threads |
| `final/` | the final campaign's data (results) |
| `final-dryrun/` | the final campaign's dry run on a development machine (not results) |

## The explicit step after the lane kernels and the element-block pass (2026-11-16)

Items 1–4 of the list above were worked on with bitwise identical results
at every thread count and SIMD level
(`docs/benchmarks/perf-campaign/explicit/README.md`): the batch kernels
check their ids and connectivity once and run lane kernels (AVX2 at run
time, no FMA; packed for the integrated `BᵀD` and residual, partly or not
for the gradient), `BᵀD` and the finite-deformation residual are integrated as
they are computed, `grad_u −= eigen_grad_u` is applied in the gradient's
lanes, and one elastic law on one element type runs gradient, stress and
integration per cache-sized block of elements. On an AMD EPYC 7B13 at one
thread the step takes 0.74–0.83 (small strain) and 0.52–0.72 (finite
deformation) of its time before this work, and 0.34–0.43 and 0.16–0.26
of the strict C++ time; at 2–8 threads (`AKANTU_NUM_THREADS`) 0.54–0.86. The fast mode
(`AKANTU_PRECISION=fast`) takes 0.84–0.99 of the bitwise step. Items 5 and
6 (per-node solver loops, history copies) remain the serial part of a
threaded step.

## The insertion check (performance campaign, 2026-11-16)

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

### At 2, 4 and 8 ranks

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

## The insertion step and the mesh operations under it (performance campaign, 2026-11-17)

The insertion part of the check (the insertion decision and the mesh
transaction that doubles facets and nodes), and the mesh set-up operations
every cohesive run uses, were profiled and reworked with bitwise identical
results (D-122). Machine: an 8-core AMD EPYC Milan (Zen 3) development
machine, 15 GiB, alone; one process pinned to one core (MPI: rank `r` on
core `r`, MUMPS image); before and after interleaved in rotated order, 5
repetitions; before is `perf/wave2-base` (`0769542f6`), after is
`74e97f334`, both with the default release profile. Raw data, scripts and
profiles: `docs/benchmarks/perf-campaign/cohesive-insert/`.

What the profile of the full serial schedule showed at the branch point
(frame-pointer call graph, shares of samples; `profiles/serial-base-5000-*`):
the check 54 % of the run, of which the facet stresses 23 % (their copy
plan rebuilt after every insertion step 5.8 %), the linear law's check
10.7 % and the insertion commit 20 %: the cross-part consistency check of
the facet mesh at each commit 11.5 % (a walk over every adjacency row,
`Vec` per row), the rebuild of the element groups' node lists 3.9 % (a sort
of the 92,624 node entries of the volume group `Body`, after a copy of every
group). At 8 ranks (`profiles/np8-p8-c8-*`, after the serial changes) the
DOF manager's global-number exchange after node doubling took 14.6 % of all
samples: its accessor scanned every DOF of the rank for each node of the
node synchronizer's schemes.

Changes:

- **The cross-part check restricted to what a transaction touched**
  (release builds; `SharedMesh::set_full_consistency_checks`). A
  transaction that starts from a state that passed the check at the
  previous commit reads the adjacency references only in the rows it
  replaced or appended (taken from its journal). Still checked in full:
  the node arrays of both parts (coordinates bit for bit, flags, global
  ids, owner ranks), the index domains, the type, row count and
  components of every adjacency block. Not re-read: the references of
  untouched rows, which were in range at the previous check and whose
  blocks only grew. The full check runs in debug builds (with the debug
  reciprocity check), for the first mutation of a domain, after a set-up
  operation that failed it, for a removal, a snapshot of a part's mesh
  data, a facet mesh created by the transaction, or a transaction that
  replaced more than an eighth of the `element_to_subelement` rows (a
  whole-mesh insertion). The verdicts of both forms are compared on every
  kind of journaled edit by `tests/restricted_consistency_check.rs`.
- **Journals instead of snapshots**: the element groups' node lists
  (rebuilt through a bit set: the same sorted, unique lists) and the node
  synchronizer replaced by the distributed hook are journaled; the group
  manager and the other synchronizers are no longer copied per insertion;
  `CohesiveSynchronizers::element_to_prank` is shared instead of copied
  into the staged synchronizers; journal entries keep the usual dataset
  names without an allocation.
- **The stress copy plan follows an insertion**: the pair that wrote side
  1 of each doubled facet moves to side 0 of the new facet (same source,
  same fused coefficients; new facets follow every existing one), checked
  against the mesh; any other change rebuilds it; debug builds compare
  every patched plan with a rebuilt one. The plan is keyed on a copy of
  the law's filter (compared, not hashed per check).
- **Distributed decision**: one reduction of the flagged owned and ghost
  facets; with none on any rank the two facet-synchronizer exchanges, the
  scan and the order array are skipped (every step before the first
  insertion, step 710 of the serial full schedule, and every later step
  without one).
- **The DOF manager's global-number exchange** (`akantu-solver`, smallest
  change): the accessor indexes the equations of every node once per
  exchange (same equations, same order).
- **The cohesive laws' residual and energies**: the work arrays (normals,
  openings, `(t + t_c) N`, its integral, the elemental residual) are kept
  between steps, the filter and the four energy arrays are read through
  borrows instead of copies.
- **Set-up**: `plan_facets` counts the elements shared by a facet's nodes
  through dense element ids (one pass per facet instead of a scan per
  element and node); `transfer_physical_names` keeps a sorted index per
  coordinate and scans the narrowest window (the x window alone held a
  whole face of a box, so the pre-selection was correct but slow); `fill_from_node_group`
  tests membership through a table instead of a scan of the node list.

The serial benchmark driver (h = 0.12, 23,156 tetrahedra), ms per step:

| series | phase | before | after | after / before (paired) |
|---|---|---|---|---|
| 500 steps (no insertion) | `check_cohesive_stress` | 4.744 [4.512, 4.807] | 4.646 [4.477, 4.874] | 0.993 [0.973, 1.014] |
| 500 steps | `solve_step` | 3.400 [3.339, 3.491] | 3.432 [3.366, 3.561] | 1.009 [0.983, 1.026] |
| full schedule (5000 steps, 11,736 cohesive elements) | `check_cohesive_stress` | 10.782 [10.710, 10.849] | 6.056 [5.911, 6.149] | 0.558 [0.552, 0.569] |
| full schedule | of which facet stresses and laws' checks | 6.901 [6.833, 6.936] | 5.105 [4.978, 5.210] | 0.736 [0.727, 0.754] |
| full schedule | of which insertion | 3.879 [3.861, 3.911] | 0.949 [0.930, 0.952] | 0.243 [0.240, 0.246] |
| full schedule | `solve_step` | 7.576 [7.566, 7.635] | 7.606 [7.504, 7.634] | 0.996 [0.987, 1.009] |
| h = 0.06 (177,328 tetrahedra), 200 steps | `check_cohesive_stress` | 54.116 [53.895, 55.468] | 50.870 [49.723, 51.537] | 0.930 [0.919, 0.956] |
| h = 0.06 | of which insertion | 1.522 [1.512, 1.532] | 0.312 [0.306, 0.313] | 0.204 [0.201, 0.206] |

Peak RSS: 463 → 404 MiB (500 steps), 472 → 445 MiB (full schedule), 851
→ 820 MiB (h = 0.06). Every run of both series gave the same insertion
counts; the state digests of the serial runs (energies of every step,
insertions, nodal state of the last step) are identical before and after
(`bitwise/`).

At 2, 4 and 8 ranks (the full schedule with the Scotch partition and the
MUMPS static solve), ms per step of the slowest rank:

| ranks | phase | before | after | after / before (paired) |
|---|---|---|---|---|
| 2 | `check_cohesive_stress` | 12.061 [12.029, 12.209] | 4.059 [4.041, 4.155] | 0.335 [0.332, 0.345] |
| 2 | of which facet stresses | 2.352 [2.299, 2.418] | 1.413 [1.404, 1.477] | 0.597 [0.584, 0.643] |
| 2 | of which insertion | 8.665 [8.641, 8.740] | 1.606 [1.603, 1.633] | 0.185 [0.184, 0.188] |
| 2 | `solve_step` | 5.020 [5.003, 5.078] | 4.893 [4.882, 4.959] | 0.975 [0.962, 0.988] |
| 4 | `check_cohesive_stress` | 6.217 [6.004, 6.316] | 2.378 [2.289, 2.488] | 0.383 [0.375, 0.394] |
| 4 | of which facet stresses | 1.124 [1.037, 1.200] | 0.642 [0.592, 0.699] | 0.578 [0.548, 0.582] |
| 4 | of which insertion | 4.605 [4.495, 4.636] | 1.277 [1.236, 1.323] | 0.277 [0.271, 0.285] |
| 4 | `solve_step` | 3.161 [3.091, 3.242] | 3.075 [3.044, 3.180] | 0.981 [0.963, 0.995] |
| 8 | `check_cohesive_stress` | 3.535 [3.479, 3.611] | 1.624 [1.602, 1.653] | 0.458 [0.444, 0.471] |
| 8 | of which facet stresses | 0.556 [0.540, 0.600] | 0.332 [0.326, 0.344] | 0.598 [0.543, 0.619] |
| 8 | of which insertion | 2.754 [2.708, 2.777] | 1.057 [1.048, 1.078] | 0.387 [0.377, 0.395] |
| 8 | `solve_step` | 2.047 [2.025, 2.108] | 2.005 [1.994, 2.060] | 0.978 [0.955, 1.006] |

The laws' checks did not change (0.96–0.99). The facet-stress part gains
from the copy plan no longer rebuilt after each insertion step. With the
port's own static solve (`rust_sparse_solver = eigen`, which repeats from
run to run) the `energies.csv` (17 digits) and `insertions.csv` of before
and after are identical at 2, 4 and 8 ranks (`bitwise/`); the MUMPS runs
are compared on time only.

The set-up suite (`suite_setup`, size `m`) and the cohesive cases of
`suite_solid`, seconds per phase (every case's digest identical before and
after):

| case | phase | before [s] | after [s] | after / before (paired) |
|---|---|---|---|---|
| `facets_tet4` | `init_mesh_facets` | 1.753 [1.740, 1.915] | 0.449 [0.447, 0.468] | 0.257 [0.235, 0.264] |
| `facets_hex8` | `init_mesh_facets` | 0.873 [0.848, 0.874] | 0.542 [0.539, 0.554] | 0.629 [0.620, 0.637] |
| `groups_tet4` | `element_group_from_node_group` | 0.160 [0.159, 0.162] | 0.029 [0.029, 0.030] | 0.185 [0.179, 0.189] |
| `cohesive_insert_tet4` | `init_mesh_facets` | 1.715 [1.705, 1.738] | 0.434 [0.430, 0.448] | 0.255 [0.250, 0.258] |
| `cohesive_insert_tet4` | `insert` (every facet) | 3.963 [3.947, 4.161] | 3.479 [3.437, 3.520] | 0.879 [0.830, 0.884] |
| `cohesive_extrinsic_check` | `init` | 1.168 [1.163, 1.177] | 0.536 [0.535, 0.554] | 0.459 [0.456, 0.474] |
| `cohesive_extrinsic_check` | `check_insertion` | 1.372 [1.367, 1.377] | 1.369 [1.344, 1.442] | 0.994 [0.980, 1.053] |
| `cohesive_extrinsic_insertion` | `init` | 0.634 [0.631, 0.635] | 0.273 [0.272, 0.276] | 0.432 [0.429, 0.436] |
| `cohesive_extrinsic_insertion` | `check_insertion` | 1.685 [1.634, 1.774] | 1.391 [1.366, 1.401] | 0.832 [0.787, 0.836] |

What did not pay:

- A fused facet-stress computation and linear-law check per block of
  facets (the stresses read back from the cache): equal to the two passes
  on the driver, 7–10 % slower on `cohesive_extrinsic_check` for blocks of
  64 to 4096 facets; removed.
- A software prefetch of the bulk stress tuple eight pairs ahead in the
  facet-stress loop: −3 % of the check before insertion; not worth new
  `unsafe` in a crate that forbids it.
- Binary searches in `plan_facets`' node rows: slower than the scans
  (`Element::cmp` is a tuple comparison); replaced by the dense-id count.
- An inline node synchronizer in the transaction journal enlarged every
  entry (×1.2 on the whole-mesh insertion); boxed.

What remains (shares of the profiles after the change, not times):

- Serial (`profiles/serial-final-5000-*`, the full schedule): the check is
  39 % of the run, the explicit step 60 %. The facet stresses (18.8 %) are
  bound by gathers of each pair's bulk stress tuple and coefficients
  (random over about 4 MiB) and by writing `facet_stress` (about 26 MB per
  step at h = 0.12, all of it public model state); the linear law's check
  (14.2 %) streams `facet_stress`, normals and tangents. The insertion is
  6.0 % of the run, spread over the subfacet searches (1.6 %), the group
  node lists (0.9 %), the event dispatch and the transaction's
  bookkeeping.
- 8 ranks (`profiles/np8-p8-c9-*`): the insertion is 18.6 % of all
  samples, most of it in the transaction's agreements (one per stage and
  one per dispatched event, consumer and level), which also absorb the
  ranks' imbalance; then the global ids of the doubled nodes (3.2 %), the
  insertion decision (2.9 %), the DOF manager's numbering (2.1 %), the
  node-scheme rebuild (1.3 %). Fewer agreements need a change of the
  transaction protocol (its error paths are gate-tested); the node
  schemes and the global ids could follow the doubled nodes instead of
  being rebuilt over all nodes.

## The cohesive model on threads (performance campaign, 2026-11-17)

The cohesive model's per-step kernels run on the execution policy's
threads (D-123; `docs/ARCHITECTURE.md`, "Execution policy", "The cohesive
model"): the facet stresses and the insertion decision of the check, the
cohesive normals, openings and tractions (`cohesive_linear`,
`cohesive_bilinear`), the cohesive residual and the energy update. For
each rank count the state digests are identical at every thread count and
grain, and equal to the base's at that rank count (the rank counts differ
from each other, as on the base: the insertions follow the partition).
Serial: the h = 0.12 full schedule and h = 0.06 (energies of every step,
insertions, the nodal state of the last step) at 1, 2, 4 and 8 threads,
finest and automatic grain. Distributed, h = 0.12 full schedule with
`rust_sparse_solver = eigen` (energies and insertions): 2 ranks × 1, 2, 4
threads (2 and 4 at both grains), 4 × 1, 2 (2 at both grains), 8 × 1.
The `suite_solid` cohesive cases: the same digests at 1 to 8 threads.
Machine: the AWS `m7a.2xlarge` (AMD
EPYC 9R14, 8 cores), timing on cores 0–3 (`tjob`) or the whole machine
(`xjob`), 5 interleaved repetitions (3 for the MPI series); before is
`perf/wave2-base` at `d404f4c70`, after the branch at `6fb246686`. Raw data and scripts:
`docs/benchmarks/perf-campaign/cohesive-threads/`.

The benchmark driver, h = 0.12 (23,156 tetrahedra), full schedule (5000
steps, 11,736 cohesive elements), ms per step, against the base at one
thread:

| threads | `check_cohesive_stress` | facet stresses | laws' checks | insertion | `solve_step` |
|---|---|---|---|---|---|
| base, 1 | 6.430 [6.337, 6.454] | 3.000 [2.942, 3.005] | 2.415 [2.393, 2.435] | 1.012 [0.999, 1.014] | 4.966 [4.951, 4.983] |
| 1 | 6.253 (×0.976 [0.973, 0.978]) | 2.871 (×0.964 [0.957, 0.966]) | 2.376 (×0.981 [0.977, 0.984]) | 1.012 (×1.001 [0.995, 1.006]) | 4.902 (×0.987 [0.977, 0.994]) |
| 2 | 4.055 (×0.633 [0.629, 0.636]) | 1.562 (×0.523) | 1.465 (×0.606) | 1.026 (×1.024) | 3.930 (×0.794 [0.791, 0.797]) |
| 4 | 2.649 (×0.412 [0.411, 0.415]) | 0.794 (×0.266) | 0.807 (×0.333) | 1.046 (×1.034) | 3.252 (×0.654 [0.647, 0.656]) |
| 8 | 2.208 (×0.344 [0.342, 0.345]) | 0.523 (×0.176) | 0.622 (×0.257) | 1.057 (×1.048) | 3.018 (×0.609 [0.603, 0.610]) |

(One series on the whole machine: per repetition the base at one thread
and the branch at 1, 2, 4 and 8 threads, order rotated; ratios paired per
repetition. Peak memory +2.2 to +2.6 % with threads.) Before insertion
(500 steps) at one thread: check ×0.970 [0.952, 0.980] (facet stresses
×0.962, laws' checks ×0.982 [0.965, 0.990]), `solve_step` ×1.000.

The laws' checks at one thread: with the decision on threads beside it,
the serial loop first compiled to scalar code where the base packs two
rows of the effective norm's products into one register (forced inlining
lost the disjointness of the normal-traction output from the stresses it
reads: +18.7 % instructions, 1.05 to 1.07 of the base's time). The normal
traction is now computed into a local array, and the serial loop is
compiled once per dimension (`check_serial_dim::<D>`, fixed sizes): the
packed code is back and the law check takes 0.98 of the base's time (each
lane performs the scalar sequence; the digests are unchanged).

The `suite_solid` cohesive cases (size `m`), seconds per phase, ratio to
the base at one thread (paired):

| case | phase | 1 thread | 2 | 4 | 8 |
|---|---|---|---|---|---|
| `cohesive_extrinsic_check` | `check_insertion` | ×0.982 [0.975, 0.986] | ×0.562 | ×0.348 | ×0.321 |
| `cohesive_extrinsic_check` | `solve_step` | ×1.000 [0.994, 1.005] | ×0.734 | ×0.610 | ×0.542 |
| `cohesive_extrinsic_insertion` | `check_insertion` | ×0.989 [0.979, 0.996] | ×0.952 | ×0.929 | ×0.923 |
| `cohesive_extrinsic_insertion` | `solve_step` | ×0.989 [0.980, 1.026] | ×0.792 | ×0.682 | ×0.629 |

The mid-size mesh (h = 0.06, 177,328 tetrahedra, 500 steps, without the
static preload, the C++ Scotch partitions), 8 cores in every
configuration, slowest rank, ms per step, 3 repetitions:

| ranks × threads | check | of which insertion | `solve_step` | step | step / base 8 × 1 |
|---|---|---|---|---|---|
| base 8 × 1 | 16.081 | 2.521 | 10.319 | 26.372 | 1 |
| 8 × 1 | 16.145 | 2.578 | 10.270 | 26.388 | 1.001 [0.998, 1.001] |
| 4 × 2 | 17.475 | 2.895 | 11.088 | 28.557 | 1.086 [1.083, 1.087] |
| 2 × 4 | 19.234 | 4.070 | 13.525 | 32.759 | 1.242 [1.241, 1.258] |
| 1 × 8 | 17.820 | 3.063 | 17.496 | 35.290 | 1.338 [1.336, 1.343] |

Against the C++ strict build (the branch at `ac3b3179`, before the
per-dimension serial loop): at one rank and one thread (h = 0.12, 500
steps, 3 repetitions) the port's check takes 4.69 against 20.7 ms per
step (C++ / port 4.4) and its step 2.56 against 8.15 (3.19); at 8 ranks
(h = 0.06, 500 steps, the MUMPS static solve, `c1prep_campaign.sh`, the
port at 8 × 1) the check 14.1 against 47.1 ms (port / C++ 0.299), the
step 8.91 against 19.6 (0.453), 1.04 against 3.01 core-µs per element and
step (0.344).

What limits the scaling:

- The insertion stays serial (the transaction, the inserter's searches,
  the flags and parameters in filter order): about 1 ms per step at
  h = 0.12, half of the check at 8 threads, and the whole difference
  between the threads in the insertion-heavy suite case.
- The facet stresses write `facet_stress` (about 26 MB per step at
  h = 0.12) and the law check streams it with the normals and tangents:
  memory traffic; 0.176 at 8 threads.
- The solve: the bulk residual is threaded by the base; the cohesive
  part scales; what remains is serial per-node and per-equation work
  below the solver's part sizes and the bulk Cauchy stresses after the
  step (`akantu-solid`, not part of this work): 0.609 at 8 threads.
- Ranks against threads at 8 cores (mid-size): ranks win (8 × 1 is 1.34
  times faster than 1 × 8) because everything above stays serial within
  a rank while it is split between ranks; 4 × 2 is within 10 % of 8 × 1.
- At 8 ranks the transaction's agreements (one per stage and per
  dispatched event, consumer and level, with the ranks' waits) are about
  8 % of all samples of a call-graph profile (`profiles/np8-*`); fewer
  agreements need a change of the transaction protocol (`akantu-mesh`).

## Distribution and the communication layer (performance campaign, 2026-11-17)

Measured on an 8-vCPU AMD EPYC Milan development machine, before
(`24f73b94e`) and after (`818a43195`), interleaved, 5 repetitions; every
digest identical at 1, 2, 4 and 8 ranks. Details, raw data and what
remains: `docs/benchmarks/perf-campaign/distribution/README.md`.

| Case | Before | After | Ratio |
|---|---|---|---|
| Partition (Scotch with the dual graph), 1,296,000 Tet4, 2 / 8 ranks | 6.51 / 7.46 s | 1.92 / 2.65 s | 0.30 / 0.37 |
| Distribution, 1,296,000 Tet4, 2 / 8 ranks | 8.48 / 11.72 s | 2.84 / 3.26 s | 0.34 / 0.28 |
| Explicit step, 162,000 Tet4, 2 / 8 ranks | 13.2 / 8.2 ms | 10.1 / 5.7 ms | 0.88 / 0.79 |
| Two energies, 162,000 Tet4, 1 rank | 17.4 ms | 14.7 ms | 0.79 |
| `all_reduce` of 2 reals, 8 ranks | 10.6 µs | 6.2 µs | 0.52 |
| Explicit step of a 32-triangle model, serial communicator | 8.1 µs | 5.1 µs | 0.63 |

The part of the distributed explicit step that does not shrink with the
rank count is memory traffic (four independent one-rank runs at once each
slow by 1.5 to 1.7 times) and, on that machine, unequal vCPU speed under
load; the communication and its bookkeeping were about 20 % of rank 0's
step at 4 ranks before the changes.
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
