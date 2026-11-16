<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Distribution, communication layer and the parallel explicit step (W2-DIST)

Machine: an 8-vCPU development virtual machine, AMD EPYC Milan (Zen 3),
15 GiB, Ubuntu 24.04, Docker image `akantu-rs-scotch:dev`; one MPI rank per
vCPU (`mpirun --bind-to core --map-by core`), `SCOTCH_PTHREAD_NUMBER=1`.
Release builds of `suite_setup` (features `mpi,scotch`), `suite_models` and
`cohesive_extrinsic`. Before: `24f73b94e` (`perf/wave2-base`); after:
`818a43195` (`perf/distribution`; the commits after it change documents and
scripts only). Interleaved A/B, the order rotated per repetition, 5
repetitions (3 at size `l` and for the cohesive driver); medians [min, max]
of the time per call, paired ratios after / before per repetition. Scripts:
`scripts/perf/distribution/`. Dates: 2026-11-16 and 2026-11-17 (UTC).

The machine is a virtual machine on a shared host: its memory bandwidth
varies with the neighbours (the explicit step on one rank ran at 13.0 to
16.0 ms in different sessions), and under load the vCPUs do not run at the
same speed (see "The explicit step"). Ratios inside the [min, max] spread
are no measurable change.

Raw data: `fin-m/`, `fin-heat/`, `fin-tiny/`, `fin-l/` (suite JSON lines,
`<variant>/np<P>/<case>.r<rep>.jsonl`, `meta.jsonl`), `coh1/` (cohesive
driver: `summary.json`, `chrono_rank<r>.csv` per run), `screening/` (the
intermediate A/B series named in the text), `profiles/` (perf reports).

Every digest (owned state gathered by global id, or per-rank mesh digests
in rank order) is identical before and after at 1, 2, 4 and 8 ranks, and
the cohesive driver's physical summary (energies, displacement norms and
sums, 17 digits) is identical across variants and repetitions at 2, 4 and
8 ranks.

## Results

Times per call; ratio = after / before, paired per repetition.

### Distribution (`suite_setup` `dist_mesh_tet4`; rank 0 partitions, then the collective distribution)

`partition` is rank 0's Scotch partition with the dual graph; the other
ranks' `distribute` includes waiting for it, so `distribute` is the
maximum over the ranks of the time from the partition's start on rank 0.

| size | ranks | partition before | after | ratio | distribute before | after | ratio |
|---|---|---|---|---|---|---|---|
| `m` (162,000 Tet4) | 2 | 757 ms [732, 799] | 217 ms [214, 239] | 0.291 [0.283, 0.299] | 917 ms [892, 995] | 316 ms [307, 351] | 0.344 [0.334, 0.353] |
| `m` | 4 | 849 ms [790, 1217] | 305 ms [290, 356] | 0.364 [0.292, 0.371] | 1065 ms [982, 1455] | 394 ms [371, 442] | 0.375 [0.304, 0.387] |
| `m` | 8 | 984 ms [897, 1005] | 420 ms [389, 423] | 0.421 [0.418, 0.439] | 1277 ms [1192, 1348] | 511 ms [467, 516] | 0.390 [0.379, 0.430] |
| `l` (1,296,000 Tet4) | 2 | 6.51 s [5.89, 7.43] | 1.92 s [1.65, 2.27] | 0.295 [0.280, 0.305] | 8.48 s [7.69, 9.38] | 2.84 s [2.44, 3.20] | 0.335 [0.317, 0.341] |
| `l` | 4 | 7.11 s [6.92, 7.33] | 2.05 s [1.88, 2.17] | 0.279 [0.264, 0.313] | 9.61 s [9.40, 9.88] | 2.70 s [2.44, 2.79] | 0.273 [0.254, 0.297] |
| `l` | 8 | 7.46 s [7.06, 7.48] | 2.65 s [2.64, 3.41] | 0.374 [0.355, 0.456] | 11.72 s [11.31, 11.73] | 3.26 s [3.21, 4.13] | 0.284 [0.278, 0.352] |

From 2 to 8 ranks at `l`, the distribution grew by 38 % before (8.48 →
11.72 s) and by 15 % after (2.84 → 3.26 s). The other distributed cases
show the same (`distribute`: `heat_explicit_tet4` 0.77 / 0.69 / 0.56 at
2 / 4 / 8 ranks, `phasefield_staggered_tri3` 0.82 / 0.79 / 0.60, the
cohesive driver's `dist_mesh` below).

### The distributed explicit step (`explicit_step_dist_tet4`, size `m`, 162,000 Tet4)

| ranks | `solve_step` before | after | ratio | two energies before | after | ratio |
|---|---|---|---|---|---|---|
| 1 | 15.95 ms [13.39, 21.51] | 14.37 ms [13.29, 21.86] | 0.977 [0.866, 1.016] | 17.37 ms [16.11, 24.88] | 14.72 ms [13.07, 15.73] | 0.786 [0.601, 0.905] |
| 2 | 13.19 ms [8.79, 14.84] | 10.10 ms [7.71, 13.03] | 0.878 [0.679, 0.974] | 12.48 ms [7.79, 13.04] | 8.63 ms [6.46, 10.98] | 0.828 [0.662, 0.870] |
| 4 | 8.17 ms [7.77, 9.45] | 8.32 ms [6.76, 8.91] | 0.935 [0.837, 1.090] | 6.12 ms [4.14, 6.76] | 5.77 ms [3.52, 6.28] | 0.869 [0.820, 1.027] |
| 8 | 8.23 ms [5.59, 9.50] | 5.72 ms [5.07, 6.54] | 0.788 [0.612, 0.908] | 3.26 ms [3.08, 3.65] | 3.00 ms [2.67, 3.12] | 0.866 [0.830, 0.947] |

The step's spread is that of the machine (see above); in a quieter
session (`screening/s5`) the build with every step change (`h7`) ran
8.0 / 5.3 / 4.7 ms at 2 / 4 / 8 ranks.

### Exchanges and reductions (`ghost_nodes_tet4`, `ghost_elements_tet4`, `reduce_scalars`)

| case | ranks | before | after | ratio |
|---|---|---|---|---|
| node residual exchange (slave reduction + synchronize, 3 reals per node) | 2 | 79.4 µs [61.9, 107.3] | 62.8 µs [58.4, 71.2] | 0.839 [0.664, 0.942] |
|  | 4 | 97.6 µs [83.3, 123.7] | 79.4 µs [70.8, 101.4] | 0.937 [0.726, 0.985] |
|  | 8 | 122.5 µs [105.5, 127.8] | 98.5 µs [84.1, 106.7] | 0.837 [0.762, 0.873] |
| element exchange (9 reals per ghost element) | 2 | 324 µs [217, 424] | 217 µs [147, 360] | 0.850 [0.454, 0.997] |
|  | 4 | 406 µs [257, 464] | 402 µs [232, 471] | 0.975 [0.872, 1.066] |
|  | 8 | 403 µs [315, 451] | 371 µs [309, 439] | 0.975 [0.823, 1.098] |
| `all_reduce` of 2 reals | 1 | 0.66 µs [0.59, 0.67] | 0.34 µs [0.32, 0.51] | 0.540 [0.504, 0.759] |
|  | 2 | 2.11 µs [1.49, 2.50] | 1.32 µs [0.95, 1.63] | 0.639 [0.622, 0.791] |
|  | 4 | 4.58 µs [4.12, 5.92] | 2.88 µs [2.19, 3.30] | 0.558 [0.521, 0.651] |
|  | 8 | 10.6 µs [9.5, 11.9] | 6.2 µs [4.7, 6.6] | 0.524 [0.448, 0.695] |
| `all_reduce` of 4,096 reals | 2 / 4 / 8 | 28 / 120 / 552 µs | 35 / 115 / 600 µs | 1.00 / 0.95 / 1.05 (no change) |

### Serial bookkeeping (`py_twin_explicit_tri3`, one rank)

| phase | before | after | ratio |
|---|---|---|---|
| `solve_step_tiny` (4 × 4 square, 32 Tri3) | 7.93 µs [7.82, 11.43] | 5.36 µs [5.17, 5.86] | 0.661 [0.490, 0.746] |
| `solve_step` (80,000 Tri3) | 4.21 ms [4.07, 4.82] | 4.20 ms [4.04, 6.19] | 0.991 [0.978, 1.284] |

### Cohesive driver, insertion off (`cohesive_extrinsic`, frozen `cube-h0.12` mesh, 23,156 Tet4, 500 steps, the port's Eigen-order static solver)

Maximum over the ranks of each phase (`chrono_rank<r>.csv`), 3 repetitions.

| ranks | `dist_mesh` before | after | ratio | `solve_step` (500 steps) before | after | ratio |
|---|---|---|---|---|---|---|
| 2 | 188 ms [186, 189] | 92 ms [90, 94] | 0.496 [0.479, 0.498] | 1.143 s [1.123, 1.197] | 1.124 s [1.103, 1.147] | 0.982 [0.940, 1.004] |
| 4 | 211 ms [211, 211] | 109 ms [109, 141] | 0.517 [0.517, 0.668] | 1.028 s [0.806, 1.029] | 0.940 s [0.846, 0.948] | 0.914 [0.822, 1.177] |
| 8 | 245 ms [241, 278] | 145 ms [127, 151] | 0.601 [0.457, 0.615] | 0.714 s [0.698, 0.744] | 0.642 s [0.635, 0.690] | 0.921 [0.890, 0.927] |

`init_full` and `static_solve` show no measurable change.

### Other distributed models (`suite_models`)

`heat_explicit_tet4` `solve_step` 1.044 [0.992, 1.096] / 1.024 / 0.988 /
0.961 [0.821, 0.982] at 1 / 2 / 4 / 8 ranks; `phasefield_staggered_tri3`:
no phase outside its spread except `distribute`.


## What was changed

Each change keeps every value's floating-point operations and their order
(D-122); the partition and the package bytes are unchanged.

1. **Partition** (`crates/akantu-mesh/src/partition.rs`):
   `build_dual_graph` counted the shared nodes of each element in a
   `BTreeMap<Element, usize>` (28 % of rank 0's samples in `dist_mesh_tet4`
   at 8 ranks, nearly the whole `partition` phase; the Scotch call itself
   was about 2 %). The counts now live in an array over linearised element
   indices with a stamp per vertex; the touched elements, sorted, are the
   neighbours in `Element` order (linear order is `Element` order for the
   not-ghost elements of the partitioned dimension). `node_to_elements` is
   one CSR of linear indices instead of a `Vec<Element>` per node (the
   report's unverified idea). A test keeps the ordered-map construction as
   the reference; the Scotch oracle tests (partitions, weighted
   partitions, reordering, graph hashes) pass unchanged.
2. **Packaging** (`crates/akantu-mesh/src/distribution.rs`):
   `Plan::package` rebuilt `element_to_groups` for every rank and type and
   scanned every node group in full for every rank. The plan now holds one
   element → groups CSR per type and one global node → groups CSR.
3. **Checked collectives** (`crates/akantu-comm/src/mpi_backend.rs`):
   every collective of the MPI back-end starts with a header all-gather;
   the header now has a fixed 64-byte payload slot, and an all-gather,
   gather or broadcast whose contributions all fit completes with the
   header alone. Every agreement of the collective error protocol (five
   words) and every scalar reduction is one MPI collective instead of two;
   the contributions and their rank-order fold are unchanged.
4. **Byte messages** (`crates/akantu-comm/src/communicator.rs`): `isend`
   of a `Vec<u8>` (every synchronizer message) hands the buffer to MPI and
   gets it back at completion instead of sending a packed copy;
   `wait_received` returns the received bytes instead of copying them.
5. **Accessors** (`crates/akantu-comm/src/accessor.rs`): record ranges are
   checked inline (the error is built only when out of range); the
   reduction accessor unpacks a record at once, then combines its values in
   order.
6. **One-rank agreements** (`crates/akantu-comm/src/phase.rs`): on a
   communicator of one rank an agreement returns the rank's own outcome
   without the all-reduce, the qualified phase name and the step guard (the
   protocol was about 40 % of the 32-triangle model's step).
7. **Energies** (`crates/akantu-core/src/math.rs`): `Math::reduce` sorts
   with an LSD radix sort of the `total_cmp` keys from 512 values on
   (values with equal keys have equal bits, so the sorted array is the
   same); property test against the comparison sort with signed zeros,
   infinities, NaNs of both signs and several payloads, subnormals.
   A micro-benchmark on this machine: 162,000 values 1.5 ms against 4.2 ms
   (11-bit digits; 8- and 16-bit digits 2.6 and 2.7 ms).
8. **The solid model's parallel step** (outside the task's files, each in
   its own commit): one internal-force scratch set per ghost type
   (`crates/akantu-solid/src/model.rs`; the shared set was shrunk to the
   ghost block and grown back with a zero fill twice per step: 8 % of
   rank 0 at 4 ranks); `split_by_law` and the stress accessor look up the
   law-index, numbering and stress blocks once per run of elements of one
   `(type, ghost)` instead of taking a read guard per element
   (`crates/akantu-solid/src/model/parallel.rs`, 7 % → 2 %); the
   `_smm_stress` exchange is posted before the not-ghost assembly and
   completed before the ghost one, as upstream
   (`crates/akantu-solid/src/model/assembly.rs`, with
   `Synchronizer::post_in` / `wait_in`, which keep the protocol's
   guarantee: the not-ghost pass's outcome is agreed with the unpacking).
9. **Measurement**: `akantu_comm::phase::agreement_stats` counts the
   agreements and the time in them; `explicit_step_dist_tet4` reports
   agreements per step and that time on the fastest and the slowest rank.


## What remains, and why

### The explicit step: memory bandwidth and core imbalance, not communication

The part of the distributed step that does not shrink with the rank count
is not communication. Four independent one-rank runs of the step at the
same time each slow from 12.9 ms to 18.7–22.0 ms, eight from 12.9 ms to
22.9–51.9 ms (`screening/contention.txt`): the step is bound by memory
bandwidth, which the ranks share. On rank 0 at 4 ranks (before), the
communication and its bookkeeping were about 20 % of the step (the stress
exchange 10.5 %, the scratch re-zeroing 8.3 %), the material history copy
19 %, the element kernels the rest.

The waiting of the remaining sync points comes from core speed under load:
with equal work per rank (40,110 to 40,673 local and 4,850 to 6,051 ghost
tetrahedra at 4 ranks), the rank on vCPU 3 waits 2.7–3.3 ms per step in
agreements and the others 0.2–0.6 ms; swapping the rank-to-core mapping
moves the waiting with vCPU 3, and alone every vCPU runs the one-rank step
in 13.4–13.9 ms (`screening/imbalance.txt`). That is the host's topology
seen through the virtual machine; it is not in the code.

### Finding 4.4: agreements per step

Measured: 15 agreements per explicit step at every rank count (5 per pair
of energies). After the inline header, the least-waiting rank spends 83,
137 and 256 µs per step in them at 2, 4 and 8 ranks (5.5, 9 and 17 µs per
agreement); the most-waiting rank 0.14, 1.0 and 1.6 ms (`screening/s8`).
The protocol's guarantee needs each exchange's prepare agreement (a rank
that failed earlier returns before the exchange; without the agreement its
peers would wait for messages it never posts) and its post agreement
(nobody may use an exchange that failed on another rank). Not implemented,
for the maintainers: dropping the agreement after every successful
`DofManager::step` (failures then reach the other ranks at the next
agreement, after they ran their own next steps and callbacks) takes 15
agreements down to 3 per step; an experiment build of exactly that (not
committed) gave no measurable change of the step at 2, 4 and 8 ranks
(paired ratios 1.03 [0.93, 1.23], 1.08 [0.80, 1.39], 0.96 [0.87, 1.23]):
the waiting moves to the next sync point. Not recommended.

### Other items left

- **History copy** (`InternalField::save_current_values`, finding 2.6 of
  the report): 17–19 % of rank 0's step at 4 ranks, pure memory traffic.
  The step rewrites `stress` and `grad_u` completely, so a copy-on-write
  history still copies every step; only swapping the buffers with a
  "fully overwritten" contract inside the solid kernels removes it, which
  also changes the buffer identity Python views hold (D-047). Expected
  15 % of the step at one rank and more at P ranks (bandwidth-bound).
  Bitwise-safe; solid-model and exec work.
- **Cohesive tractions exchange** (finding 2.2, second site,
  `akantu-cohesive/src/model.rs`): it would have to stay open across the
  solid residual (which runs its own agreed exchange) and be completed in
  the solid's ghost pass: a new hook in the solid residual and a change in
  the insertion task's file. The bulk overlap gave no measurable change on
  this machine. Not done.
- **Large reductions**: the rank-order all-reduce of 4,096 reals stays an
  all-gather and a local fold (28, 115, 600 µs at 2, 4, 8 ranks). A gather
  to rank 0, a fold there and a broadcast (same bits) was tried: 1.45×
  slower at 2 ranks, 1.14× at 4, 0.90× at 8 within a wide spread; dropped.
  Fast mode: native `MPI_Allreduce` (`ReductionPolicy::NativeMpi` exists).
- **Root memory of the distribution**: the root still holds every package
  before the `scatter_v` (send each package as it is built and free it).
  Packaging is no longer the root's main cost (0.33 s of the root's
  `distribute` at size `m`, 8 ranks, of which the Scotch call and
  `fill_partition_information` are most of the partition phase).
- **Buffer pools per synchronizer** (finding 2.5): the copies are gone
  (item 4); keeping the allocations between exchanges would save one
  `malloc` / `free` per message; not measurable against the exchanges'
  60–400 µs here.
- **Energies**: what remains is the potential energy's per-point
  `double_dot` in Eigen order (`eigen_order::redux_tree`, 37 % of the
  energies) and the GEBP integration product (28 %): files of W2-GEBP.
  Fast mode: unsorted (pairwise) energy sums instead of the sort (the sort
  is now 11 %).

