# Possible performance improvements of akantu-rs

This is a read-only scan of the port for performance work, made on
14 November 2026 and revised for the public tree on 15 November 2026. No
code was changed for it, and no timing was run except the parser
measurement of 1.8. Every finding names the code it rests on and the
measurement that sizes it; where nothing measures it, the finding says so.
The revision updated file references and section names to the current
tree. Two figures that the scan quotes come from development notes that
are not in the repository; the findings say so where they use them.

During the port, optimisation waited until parity with the C++ library was
reached. The port reproduces the C++ results bit for bit in most areas
(`docs/NUMERICAL_VALIDATION.md`), so each proposal below states whether it
keeps the order of the floating-point operations. The explicit step had a
dedicated optimisation round (`docs/BENCHMARKS.md`, "Explicit step"), and a
few hot spots were fixed along the way (the copy plan of the insertion check,
the MPI packing, the array accessors); the rest is a first port.

## Where the time goes today

The figures that decide the ranking come from `docs/BENCHMARKS.md`:

- Full-size cohesive run (JOSS mesh, 4.39 M tetrahedra, 5,000 steps, one
  64-core host, section "The full-size run"): the port's wall
  time is 0.875 of upstream's at 64 ranks (2,387 s against 2,729 s). The
  insertion check takes 53 % of the wall at 64 ranks, 65 % at 32 and 75 % at
  8. The explicit step takes 11 % to 23 % and scales worst (29 % efficiency
  from 8 to 64 ranks). The static solve takes 8 % to 13 %, the mesh
  distribution up to 6 %, the per-step energies 4 % to 5 %.
- At 64 ranks the insertion check costs 3.71 core-µs per element and step
  in the 5,000-step run against 1.77 in the 20-step test before any
  insertion. Insertion therefore adds about 1.9 core-µs per element and
  step, about 130 ms of wall time per step on each rank
  (1.9e-6 × 4.39e6 / 64). Nothing has profiled that part.
- Memory at 64 ranks: 227 GiB summed over the ranks at the peak, of which
  MUMPS holds 170 GiB (`INFOG(19)`). Before the static solve the ranks hold
  20.1 GiB in total and 0.92 GiB on the largest rank; after it, 204.9 GiB and
  5.8 GiB (`docs/benchmarks/c1-2026-11-12/rust-np64-a1/summary.json`,
  `rss_before_static`, `rss_after_static`).
- Small meshes at 1 to 8 ranks ("Small and mid sizes at 1 to 8 ranks"):
  the port's pre-insertion loop takes 0.39 to 0.54 of
  upstream's time. The explicit step loses that lead as ranks are added:
  0.44 of upstream's time at 1 rank and 0.76 at 8 in that series, 0.82 at
  64 ranks in the full-size run.
- Serial explicit step after the optimisation round: 0.31 to 0.60 of the
  strict C++ build's time ("The explicit step on one core"). Its remaining
  profile is spread over the gradient (14 % to 15 %), `D·B` (12 % to 22 %),
  the scatter (9 % to 10 %), the integration (8 % to 10 %) and the history
  copies (13 % on hex8 small strain) ("What was optimised").

## Group 1: cheap and parity-preserving

### 1.1 Release the MUMPS factors after the static solve

- Where: the static solver kept by the model after the cohesive driver's
  preload (`crates/akantu-bench/src/driver.rs`, static phase; MUMPS
  instance in `crates/akantu-linalg/src/mumps/mod.rs`, which already has a
  collective `destroy()` at line 930).
- Now: the factors stay allocated for the 5,000 explicit steps, as in
  upstream. `docs/BENCHMARKS.md` ("Memory") says that whether anything uses
  them after the static solve was not checked. In the driver, the only later
  access is the read of the solver statistics right after the solve
  (`driver.rs`, lines 613 to 620); the loop is explicit.
- Change: a model call (or solver option) that destroys the
  factorisation once the caller says no re-solve will follow, and a driver
  option that uses it after the preload and after the statistics are read. Release the assembled
  stiffness matrix with it if nothing else reads it.
- Gain: memory. At 64 ranks the summed RSS would fall from about 205 GiB to
  about 21 GiB for the loop, and the largest rank from 6.13 GiB to about
  1 GiB plus the loop's growth (the loop's high-water mark is 170 MiB above
  the static one on the largest rank). At 8 ranks the largest rank holds
  28.8 GiB after the static solve against 1.8 GiB before. The full-size run
  could then use smaller hosts or more ranks per host. Basis: the
  `rss_before_static` / `rss_after_static` records named above.
- Parity: none (the factors are not used again). It departs from
  upstream's memory behaviour, so it should be an explicit call or option,
  off by default in the library.
- Effort and risk: hours for the driver, about a day for a library API
  with a test that a later `solve_step("static")` refactorises. Risk: a
  model that re-solves with the same matrix (implicit dynamics) must not
  call it.
- Verify: `cohesive_extrinsic` with `rust_static_solve = true` at h = 0.12
  (energies and insertion counts byte-identical to the current run), and
  the memory sampler of `crates/akantu-bench/scripts/memsample.py`.

### 1.2 A specialised kernel for the stress interpolation of the insertion check

- Where: `FeEngine::interpolate_elemental_field_with`
  (`crates/akantu-fe/src/engine/elemental.rs:444`), called once per bulk law
  and check by `SolidMechanicsModelCohesive::interpolate_stress`
  (`crates/akantu-cohesive/src/model.rs:1486`).
- Now: two calls of the generic Eigen-order product emulation
  `eigen::product_slices` per element and check, with run-time
  dimensions, the route chosen anew in each call and closures for the
  coefficient access. Each call also copies the element filter twice
  (`to_vec`, `resolve_filter`) and builds a slot vector.
- Change: choose the route once per (element type, law) and run a
  fixed-size kernel that keeps the emulation's reduction tree, as the
  explicit step's fast paths do (`docs/BENCHMARKS.md`, "What was
  optimised", first row). For linear tetrahedra
  (`nq = 1`) the inverse and interpolation matrices are the 1 × 1 and 4 × 1
  matrices of ones, and both products are depth-1 coefficient products, so
  the result is the stored stress itself: the copy plan can read the law's
  stress array directly and skip the intermediate buffer. This shortcut is
  only valid if the plan checks at build time that every matrix entry is
  exactly 1.0 and that `eigen::product_route` gives the coefficient-based
  route; otherwise it falls back.
- Gain: the check is 53 % to 75 % of the full-size wall. The only recorded
  split of the check was measured during the port and is kept in its
  development notes, not in the repository: at h = 0.12, on one thread of
  the Apple M4 Pro and before the copy plan of the check existed, the
  interpolation took 7.2 ms of the 13.2 ms call, the law checks 1.7 ms and
  the insertion 4.3 ms on average. The same notes estimate × 0.5 to 0.6 for
  the interpolation with a fixed-size kernel, about −30 % of the check.
  `examples/check_cost.rs` (below) measures the split again on today's
  code. The tet4 copy shortcut would remove the product entirely; its gain
  is unmeasured.
- Parity: none if the kernel keeps the trees (checked like the explicit
  kernels: property tests against the generic path on 10⁵ random cases,
  `docs/BENCHMARKS.md`, "What was optimised").
- Effort and risk: two to four days. Low risk; the generic path stays as the
  reference.
- Verify: `cargo test --release -p akantu-cohesive` (the 69 bitwise
  fixture cases of `tests/cohesive_fixtures.rs`), the insertion replay of
  the driver (`coh_np1_eigen`, 1,500 steps), and
  `crates/akantu-bench/scripts/timing_ab.sh` at h = 0.12 for the
  `check_cohesive_stress` phase; `crates/akantu-cohesive/examples/check_cost.rs`
  gives the split between interpolation, law checks and insertion.

### 1.3 Skip the insertion-order exchange when nothing is flagged anywhere

- Where: `CohesiveElementInserter::synchronize_insertion_in`
  (`crates/akantu-cohesive/src/inserter/parallel.rs:193`), called on every
  check of a distributed run by `insert_flagged_elements`
  (`crates/akantu-cohesive/src/model.rs:2076`).
- Now: on every step, also when no rank flagged a facet, each rank runs two
  facet-synchronizer exchanges (`CeGroups`, `CeInsertionOrder`), an
  exclusive scan and an all-reduce, and allocates an order array over all
  facets (`order.alloc(...)` filled with −1).
- Change: one all-reduce of the local count of flagged not-ghost facets
  first; when the global count is zero, clear the local ghost flags (what
  the owners would have sent) and return. The rest is unchanged when
  something is flagged.
- Gain: unmeasured. It removes two neighbour exchanges, their four
  agreements (see 4.4) and an O(facets) allocation from every step without
  insertion: all 500 steps of the pre-insertion series and 886 of the 5,000
  full-size steps (4,114 steps inserted).
- Parity: none (integers and flags only).
- Effort and risk: hours. Low risk; the distributed insertion tests cover
  the path (`crates/akantu-cohesive/src/par_scenarios.rs` and the
  distributed cohesive tests of the crate).
- Verify: `crates/akantu-bench/scripts/par2b_sanity.sh`, the distributed
  cohesive tests of `akantu-cohesive`, and the `ins` series of
  `scaling_series.sh` at 2 to 8 ranks.

### 1.4 Reuse the buffers of the cohesive residual and energies

- Where: `MaterialCohesive::assemble_internal_forces_impl`
  (`crates/akantu-cohesive/src/material.rs`, around lines 840 to 905),
  `update_energies_impl` (around line 1018), `filter_of` (line 671),
  `compute_opening` and `compute_normal`.
- Now: every step allocates per cohesive type `cpy` (`nb·nq × dim·s`),
  `partial` and `int_t_n`, copies the element filter (`filter_of`
  returns a `Vec`), and the energy update copies four whole arrays
  (`tractions`, previous tractions, `opening`, previous opening) with
  `to_vec()`.
- Change: keep the buffers in the law between steps and read the arrays
  through borrows, as the explicit-step work did for the bulk laws.
- Gain: unmeasured for the cohesive part. The same change on the bulk
  internal forces took the tet4 small-strain step from 151 to 95 ns per
  element at 179 k elements (`docs/BENCHMARKS.md`, "What was optimised",
  row "internal-force assembly buffers kept between steps"). The cohesive
  elements grow to 476,343 at the end of the full-size run, and the port's
  lead per element and step shrinks once the mesh fragments ("Known slow
  paths").
- Parity: none.
- Effort and risk: about a day. Low risk.
- Verify: `cargo test --release -p akantu-cohesive`; the insertion replay of
  the driver; a timing of the insertion regime (`ins` series, h = 0.12,
  1,500 steps), which no timing of the explicit-step work covered.

### 1.5 Remove the whole-mesh scans of every check

- Where: `flagged_insertion_facets` (`crates/akantu-cohesive/src/model.rs:1916`,
  scans every facet flag on each check to remember the flags before it),
  `filter_key` (line 188, hashes the whole element filter on each check to
  validate the copy plan), `insertion_marks`.
- Now: a few O(facets) and O(elements) passes per step that do no
  numerical work.
- Change: keep a flagged count in the inserter (normally zero between
  checks, so nothing to save), and key the copy plan on the law's filter
  release instead of a hash of its contents.
- Gain: small; unmeasured. Each pass is linear in the facets or elements
  and does a few operations per entry, against an 11.6 ms check per step at
  h = 0.12 on one rank.
- Parity: none.
- Effort and risk: hours. Risk: a filter release counter must change on
  every filter edit, which `model::perf_tests` can pin.
- Verify: `model::perf_tests` in `akantu-cohesive`, the cohesive fixture
  tier.

### 1.6 A cheaper lookup in sparse matrix assembly

- Where: `SparseMatrixAij` (`crates/akantu-linalg/src/sparse.rs:239`):
  `profile: HashMap<(GlobalIdx, GlobalIdx), usize>` with the standard
  library's default SipHash hasher, read once per entry in `resolve_block`
  (line 597) for every element matrix of every assembly.
- Now: a tet4 element matrix touches 78 entries of a symmetric matrix or
  144 of an unsymmetric one, each a SipHash lookup, on every assembly. No
  workspace crate uses a faster hasher (no `rustc-hash`, `ahash` or
  `fxhash` in any `Cargo.toml`).
- Change: either a fast non-cryptographic hasher for this map, or better a
  cached scatter map per (matrix profile release, element type): the entry
  positions of each element computed once and reused while the profile is
  unchanged, so later assemblies do no lookup at all.
- Gain: unmeasured. Candidates it addresses: the port's static-solve phase,
  which includes its assembly, takes 1.02 to 1.31 times upstream's at 13 of
  14 points of the paired sandbox series ("Known slow paths"), and every
  Newton re-assembly in implicit runs.
- Parity: none. The values are accumulated in the same order; only the
  lookup changes. Changing the hasher is also safe for iteration order:
  the default hasher is seeded randomly per process, so no reproducible
  result can depend on the order of this map.
- Effort and risk: one to two days for the scatter cache with its
  invalidation, hours for the hasher.
- Verify: `crates/akantu-solid/tests/model_smm_fixtures.rs` and the implicit
  fixture cases, the MUMPS and Eigen static patch tests
  (`crates/akantu-solid/tests/patch_mumps.rs`), and a timing of the
  assembly before and after (no assembly benchmark exists in the
  repository).

### 1.7 Loop order of the SparseLU dense kernels

- Where: `sparselu_gemm` (`crates/akantu-linalg/src/eigen_sparse_lu/dense.rs:539`)
  and `trsm_unit_lower` (line 452), the dense parts of the pure-Rust
  translation of Eigen's SparseLU (the `eigen` backend, used when the port
  is built without MUMPS).
- Now: `sparselu_gemm` runs `for j, for i, for k` with `a[i + k·lda]`, a
  strided read per term. `trsm` reads through closures with bounds checks.
- Change: `for j, for k (in pairs), for i` with contiguous columns of `A`.
  Each entry `c(i, j)` still receives `c + a(i,k)·b(k,j)` for `k = 0, 1, …`
  in the same order, so the result is unchanged and the inner loop
  vectorises across `i` without reassociation (each lane is a different
  entry).
- Gain: `sparselu_gemm` was 18 % of the samples of the cohesive h = 0.12 run
  with the `eigen` backend (`docs/BENCHMARKS.md`, "The SparseLU translation
  against upstream's SparseLU"). The port's SparseLU takes 1.6 to 2.0 times
  upstream's SparseLU at h = 0.12 and h = 0.06; part of that gap is
  expected here.
- Parity: none (same per-entry order; check the odd-depth tail `rd`).
- Effort and risk: hours to a day.
- Verify: the `eigen` backend fixtures of `akantu-linalg` and the static
  solve of the driver with `sparse_solver_type = eigen` (bitwise against the
  C++ oracle with pinned Eigen cache sizes, D-050).

### 1.8 Cache variable values in the input-file parser

- Where: `Eval::variable` in `crates/akantu-parser/src/expr.rs`
  (lines 1076 to 1126): a reference to another parameter of the `.dat`
  file looks the parameter up and parses its value string again, in the
  scope that defines it.
- Now: nothing is cached, so a parameter that refers twice to the previous
  one doubles the work per line. With `a0 = 1` and `a<i> = a<i-1> + a<i-1>`,
  evaluating the last of k lines parses about 2^k expressions. Measured on
  15 November 2026 on the Apple M4 Pro (release build of `akantu-parser`,
  one run each, the time of one `get_parameter_value` call): 16 lines
  0.015 s, 20 lines 0.12 s, 22 lines 0.60 s, 24 lines 1.76 s. Upstream
  evaluates a variable the same way (the port follows its X3 grammar), so
  by reading it has the same cost; it was not timed. Real input files
  rarely chain more than a few variables, so this matters for generated or
  adversarial inputs, and for the parser's fuzzing (F5.1 of the features
  scan).
- Change: a cache per top-level evaluation, keyed by the parameter and the
  scope it is evaluated in, that stores each variable's value after its
  first successful evaluation. Errors and the cyclic-reference check keep
  their present behaviour because only successful values are stored. The
  expression functions are deterministic (`crates/akantu-parser/src/cmath.rs`),
  so a cached value equals a recomputed one; if a random function is ever
  added to the scalar grammar, the cache must keep the order of its draws.
- Gain: the chain above becomes linear in k.
- Parity: none (the same value bits; the evaluation order of each
  expression is unchanged).
- Effort and risk: hours to two days, with a test on a long chain and the
  parser's oracle comparisons (`crates/akantu-parser/tests/oracle.rs`).
- Verify: `cargo test -p akantu-parser`, and the timing above before and
  after.

## Group 2: larger but parity-preserving

### 2.1 Make the insertion step incremental on a distributed mesh

- Where: the insertion transaction and its distributed hook:
  `CohesiveGlobalDataUpdater::apply` (`crates/akantu-cohesive/src/parallel.rs:780`),
  `MeshTransaction::validate_events` (`crates/akantu-mesh/src/mutation.rs:1169`),
  `update_stress_plan` (`crates/akantu-cohesive/src/model.rs:308`).
- Now (by reading; not profiled): each insertion step at P > 1 runs, besides
  the topology change itself,
  - `check_facet_mesh_consistency` (`crates/akantu-mesh/src/utils.rs:1133`)
    in release builds, in stage 1 of every mutation: bitwise comparison of
    all node coordinates, flags, global ids and owners of the facet mesh with
    the main mesh, then every row of `element_to_subelement` and
    `subelement_to_element`;
  - a rebuild of the stress copy plan from scratch, since its key is the mesh
    revision: one pass over every bulk element and facet, then a sort of all
    pairs;
  - `update_global_ids_in`, which returns a vector of global ids for every
    node, and a full rebuild of the node synchronizer
    (`rebuild_node_schemes_in`) and of the element groups' node lists when
    nodes were doubled;
  - copies of the synchronizers to stage the changes, and the agreements of
    the transaction stages (`docs/ARCHITECTURE.md` describes the staged
    mutation: preflight, topology, hook phases, capability check, one
    agreement per event, consumer and level at dispatch).
- Change: profile a fragmenting mid-size run first (h = 0.06 on 8 ranks,
  full schedule; `docs/BENCHMARKS.md` asks for this too). Then, in order of
  the profile: append to the copy plan instead of rebuilding it (new facets
  and the facets whose neighbours changed); run the full consistency check
  in debug builds and keep only O(inserted) checks in release builds (this
  removes a safety net, so it is a decision for the maintainers); update
  the node schemes incrementally for the doubled nodes.
- Gain: insertion adds about 1.9 core-µs per element and step at 64 ranks
  (about 130 ms per step per rank, see above), or about 2.4 core-µs per
  element if only the 4,114 insertion steps are counted. In the serial
  measurement of `check_cost.rs` an insertion pass took 7.6 ms at h = 0.12
  (0.33 µs per tetrahedron), on another machine and with fewer elements
  per pass. The gap suggests, without showing it, that much of the
  full-size cost comes with the distributed path or the mesh size. How much
  of it the items above explain is not known.
- Parity: none (bookkeeping only; insertion decisions and orders unchanged).
- Effort and risk: one to three weeks. Medium risk: the transaction protocol
  is gate-tested and its error paths (rollback, poisoning) must stay
  correct.
- Verify: the transaction tests of `akantu-mesh`
  (`crates/akantu-mesh/tests/review_v48_transaction.rs`,
  `group_transactions.rs`), the cohesive fixture tier, `par_scenarios.rs`,
  the per-copy load check of the driver
  (`docs/benchmarks/mn-2026-11-13/percopy/`), and a full-schedule timing at 8
  ranks.

### 2.2 Overlap the stress exchange with the local assembly

- Where: `SolidMechanicsPhysics::assemble_internal_forces`
  (`crates/akantu-solid/src/model/assembly.rs:327` to 345) and
  `exchange_cohesive_tractions` (`crates/akantu-cohesive/src/model.rs:717`).
- Now: the `_smm_stress` exchange runs to completion before the not-ghost
  assembly starts. Upstream posts it asynchronously, assembles the
  not-ghost elements, waits, then assembles the ghost elements; the code
  comment says the values and the order are the same "done in one step".
- Change: `Synchronizer::asynchronous_synchronize` and
  `wait_end_synchronize` exist (`crates/akantu-comm/src/synchronizer.rs:423`
  and 457). Post before the not-ghost loop and wait before the ghost loop.
  The assembly order of the values stays the same.
- Gain: unmeasured. It hides the exchange behind the not-ghost assembly. The
  explicit step's parallel overhead ("Known slow paths") is 0.45, 0.82 and
  1.58 core-µs per element and step at 8, 32 and 64 ranks, with the ghost
  layer growing from 8 % to 24 % of a rank's tetrahedra.
- Parity: none.
- Effort and risk: two to four days, because the running operation's scope
  must be parked across the local assembly (`docs/ARCHITECTURE.md`, the
  collective error protocol).
- Verify: the distributed SMM and cohesive tests, the 2-rank replay
  `ela_np2_global_local_world_runs`, and per-rank step timings (which the
  driver does not record yet; add them first).

### 2.3 The remaining serial kernels of the explicit step

`docs/BENCHMARKS.md` ("What remains in the serial step") ranks the
remaining items by their share of the profile. The estimates in the table
were made during the port and are kept in its development notes, not in the
repository. Each item keeps each entry's summation order:

| Item | Where | Estimate |
|---|---|---|
| Fuse `D·B` with the integration, no per-point `ne·nq·dim·nn` array | `akantu-fe` fast paths | −10 % to −15 % of the hex8 and tet10 small-strain step |
| One connectivity validation pass in the gradient, shape derivatives read contiguously | `akantu-fe` fast gradient | −5 % |
| Subtract `eigen_grad_u` inside the gradient writes | `akantu-solid` stress path | −3 % |
| Per-node solver loops: blocked flags copied twice (`blocked_of`, `crates/akantu-solver/src/integration_scheme.rs:257`), `update_global_blocked_dofs` rebuilt every step (`dof_manager/blocked.rs:29`) | `akantu-solver` | −2 % to −5 % |
| Scatter to nodes: prefetching or blocking that keeps the element order | `akantu-solver` scatter | at most −3 % |

The step is 11 % to 23 % of the full-size wall, so together these are a
few per cent of a full-size run. Effort: about a week for all five.
Verify with the explicit-step harness (`scripts/perf/explicit-step`,
`digests.sh` and `run_ab.sh`) and the fixture tiers named in "Correctness"
of the explicit-step section.

### 2.4 Mesh distribution from the root

- Where: `distribute_impl` and `Plan::package`
  (`crates/akantu-mesh/src/distribution.rs:156` and 564).
- Now: the root plans and packs the packages of all ranks, holds all of
  them in memory, then sends them in one `scatter_v`. Per package and
  element type it rebuilds `element_to_groups` over every element group
  (line 615), and per package it scans every node group in full (lines
  729 to 750). The port's `dist_mesh` grows by about 2.1 s per added rank
  (32, 84 and 151 s at 8, 32 and 64 ranks) where upstream's takes 66 and
  81 s at 32 and 64 ranks.
- Change: profile first; the reading above does not explain 2.1 s per rank.
  Candidates: build `element_to_groups` once per type; send each package as
  it is built and free it (lower root memory as well); build the packages
  in parallel threads (integer data only).
- Gain: up to the 70 s difference to upstream at 64 ranks (3 % of the
  wall); more at higher rank counts, where the trend would make it dominate
  the set-up.
- Parity: none (integer bookkeeping; the package contents stay the same).
- Effort and risk: days after the profile.
- Verify: the distribution tests of `akantu-mesh`, the C++ partition replays
  of the driver (`partition_matches = true`), and `dist_mesh` in the
  scaling series.

### 2.5 Buffer reuse in the communication layer

- Where: `akantu-comm` send and receive paths (`docs/BENCHMARKS.md`,
  "Point-to-point latency against raw MPI").
- Now: one owned copy per message on the send side, one allocation,
  zero-fill and decode per message on the receive side, about 0.45 µs fixed
  cost per small message. For 16 MiB messages the allocator returned every
  block to the system in the one-job measurement (about 12,000 minor page
  faults per operation).
- Change: per-synchronizer send and receive buffers kept between exchanges
  (the schemes are fixed between insertions), received in place.
- Gain: the ping-pong runs at 3.6 to 4.1 times raw MPI for 1 to 16 MiB, the
  8 KiB 4-rank halo at 1.6 to 3.5 times. What this changes in a model run
  is unmeasured: no per-step communication time is recorded.
- Parity: none.
- Effort and risk: about a week; the leak-safety rule (a send leaked at a
  deadline never refers to user memory) must be kept.
- Verify: the MPI tier of `docs/TESTING.md` (`mpi_bench calibrated`).

### 2.6 History copies of the explicit step

`save_previous_state` copies `stress`, `grad_u`, `sigma_th` and
`epsilon_th` every step, as upstream does (13 % of the hex8 small-strain
profile). Double buffering would remove the copy but changes the buffer
identity that zero-copy Python views hold (D-047). `docs/BENCHMARKS.md`
lists it as not pursued. It stays possible as an opt-in mode for runs
without Python views.

## Group 3: needs a parity switch or a new numerical mode

### 3.1 Build for the host CPU

- Now: the release profile targets baseline x86-64 (`debug = 1`, 16 codegen
  units, no LTO, no `target-cpu`; `Cargo.toml` `[profile.release]`).
- Change: an opt-in build with `-C target-cpu`. Rust never contracts `a·b +
  c` into an FMA by itself, so AVX2 without FMA keeps the arithmetic; a
  review during the port found identical explicit-step digests and replays
  with `target-cpu=native`. `f64::min`/`max` on signed zeros and the sign of
  default NaNs can still differ elsewhere (`docs/BENCHMARKS.md`, end of
  "What remains in the serial step").
- Gain: unmeasured. Fat LTO with one codegen unit gave no consistent gain on
  the specialised code (×0.78 to 1.17, within noise).
- Parity: to be proven per build with the digests and the replay tier; a
  switch, never the default.
- Effort: a day plus a replay-tier run on the target CPU.

### 3.2 A fast numerical mode without the Eigen-order emulation

- Now: every small product follows the reduction tree Eigen 3.4's strict
  build uses (`akantu_core::eigen_order`, `eigen_unrolled`), which forces
  scalar halving trees and blocks SIMD reductions inside one product.
- Change: a mode that evaluates the small products in natural order with
  SIMD reductions and FMA, compared with the default mode by tolerances
  instead of bits.
- Gain: unmeasured. The explicit step is already faster than upstream on
  one rank, so the main target would be the insertion check and implicit
  runs.
- Parity: changes results; needs its own tolerance study
  (`docs/NUMERICAL_VALIDATION.md`) and a switch.
- Effort: weeks, mostly validation.

## Group 4: architectural

### 4.1 Threads inside a rank

`docs/ARCHITECTURE.md` and decision D-017 allow threads where results stay
bit-identical; no crate uses threads in a hot loop today (no `rayon`
dependency). Two parts of the
explicit step qualify:

- Per-element work (gradient, constitutive law, `D·B`, integration, the
  check's interpolation and criterion) writes disjoint outputs and is
  bit-identical when split over threads.
- The scatter to nodes sums contributions in element order. A gather form
  (for each node, its elements in increasing order through a node→element
  CSR) reproduces that order exactly and can run in parallel over nodes.

Hybrid MPI and threads would reduce the number of ranks per host, and with
it the ghost layer (24 % of a rank's tetrahedra at 64 ranks) and the number
of global collectives. Gain unmeasured; effort several weeks, with a
serial-against-threaded bitwise test for each loop as D-017 requires.

### 4.2 SIMD across elements

The fixed-size kernels keep a scalar operation order per element. Running
the same sequence of operations on 2 or 4 elements at once (one element per
SIMD lane) gives each lane exactly the scalar result for `+ − × ÷ sqrt`
without FMA. It needs a structure-of-arrays layout or gathers per block of
elements, and run-time dispatch for AVX2 on baseline builds. Parity holds
if no lane operation is contracted or reassociated (to be shown with the
existing property tests). Effort: weeks. Gain unmeasured; the serial step
is already 0.31 to 0.60 of strict C++.

### 4.3 GPU

The explicit-step kernels and the check's interpolation map well to GPUs.
Bit-identical results with the CPU are possible only without FMA
contraction (CUDA `-fmad=false`), with the same per-entry order, a gather
form of the assembly and fixed-order reductions for the energies. The mesh
mutation of insertion would stay on the CPU and force transfers at every
insertion step (4,114 of 5,000 steps at full size). Months of work; not
advised before items 1.2, 2.1 and 4.1.

### 4.4 Fewer global agreements per step

- Where: the collective error protocol (`docs/ARCHITECTURE.md`,
  `crates/akantu-comm/src/phase.rs`): every `DofManager::step`, every
  synchronizer exchange (`Phases::run`: one agreement on the prepared data,
  one after unpacking) and every `run_collective` ends in an agreement, a
  5-word `MPI_Allreduce` (`reduce_status`, line 234).
- Now: by a reading of the code, one explicit step and one pre-insertion
  check at P > 1 make about 19 agreements: 7 in the lumped solver's
  collective (`non_linear_lumped.rs:116`, six steps and the closing one),
  2 for each of the bulk and cohesive stress exchanges, the facet-stress
  exchange and the two insertion-order exchanges, and 2 more in the
  insertion order. Upstream has no such agreements; it has the neighbour
  exchanges and its own reductions. The count was not measured at run time.
- Effect: a 40-byte all-reduce on one host costs microseconds (not
  measured here), small against a 108 ms step. Each is also a global synchronisation point, so a
  rank that is slow in one phase makes every rank wait at the next
  agreement; per-phase imbalance then adds up over the phases. The cohesive
  elements per rank range from 0 to 2.6 times the median at 64 ranks.
  Whether this matters is unmeasured.
- Change: measure first (a counter of agreements per step in the
  communicator and per-rank phase times in the driver). If it matters,
  agree once per collective operation instead of once per step, or carry
  the status word on the existing reductions.
- Parity: none numerically. It changes the error protocol (when a failure
  on one rank is noticed by the others), which was designed and reviewed
  with care; a decision for the maintainers.
- Effort: days for the measurement, weeks for a protocol change.

### 4.5 A faster pure-Rust sparse factorisation

Without MUMPS the port uses its translation of Eigen's SparseLU. Upstream's
own SparseLU takes 314.6 s and 7.5 GiB at h = 0.06 (97,551 DOFs), 18 times
MUMPS; faer's simplicial LU did not finish in 2 minutes (`docs/BENCHMARKS.md`,
"The SparseLU translation against upstream's SparseLU").
A supernodal or multifrontal factorisation (for example a supernodal
Cholesky for the symmetric positive definite elastic preload) would make
the pure-Rust build usable from h = 0.06 on. It gives different rounding
from upstream's SparseLU, so it is a new solver choice, not a replacement
of the `eigen` backend. Effort: weeks to months.

### 4.6 Distributed mesh input

Both codes read and partition the mesh on rank 0 and send packages (section
2.4). Reading in parallel and partitioning with a parallel partitioner would
remove the serial part that grows with the rank count. Effort: weeks; the
partition would differ from the replayed C++ partitions, which matters for
comparisons but not for results.

## Unverified ideas

These came up in the scan but have no measurement or profile behind them:

- ASCII output of the ParaView dumper goes through a `printf` emulation
  (`crates/akantu-io/src/dumper/cfmt.rs`) and each file is built in memory
  before it is written; its cost was never measured (the benchmarks run
  without dumps).
- The facet mesh keeps its own copies of the node coordinates, flags, global
  ids and owners (compared bitwise in `check_facet_mesh_consistency`) where
  upstream shares the parent's arrays. For the JOSS mesh that is tens of
  MiB out of 20 GiB, so it matters mainly through the per-mutation
  comparison of 2.1.
- `node_to_elements` in `crates/akantu-mesh/src/partition.rs:294` returns a
  `Vec<Vec<Element>>` (one allocation per node) during partitioning; a CSR
  would avoid 734,594 allocations on the JOSS mesh.
- The mesh transaction snapshots groups, synchronizers and possibly mesh
  data on first access (`crates/akantu-mesh/src/mutation.rs:587` to 627);
  whether the insertion path takes a snapshot of a large dataset was not
  checked.
- The per-step energies (4 % to 5 % of the full-size wall) are a setting of
  this benchmark; their kernels were not profiled.
- Contact detection (`crates/akantu-contact/src/detector.rs`) and the
  non-local neighbour search were read briefly; nothing stood out and no
  benchmark covers them.
- Build time and binary size were not measured anywhere in the repository.
- The Python bindings return zero-copy views for stored arrays (D-047);
  inputs from NumPy are copied (`Array::from_vec` in
  `crates/akantu-python/src/py/physics.rs`). Not measured.

## The ten changes to do first

| Rank | Change | Section | Parity | Effort | Expected effect and its basis |
|---:|---|---|---|---|---|
| 1 | Specialised stress interpolation for the check (tet4 copy shortcut included) | 1.2 | none | 2 to 4 days | check −30 % (an estimate made during the port); check is 53 % to 75 % of the full-size wall |
| 2 | Release the MUMPS factors after the static solve | 1.1 | none | hours to 1 day | loop memory at 64 ranks from about 205 to about 21 GiB (run records) |
| 3 | Profile, then make the distributed insertion step incremental | 2.1 | none | 1 to 3 weeks | targets about 1.9 core-µs per element and step at 64 ranks (measured, not profiled) |
| 4 | Skip the insertion-order exchange when nothing is flagged | 1.3 | none | hours | unmeasured; two exchanges and an O(facets) allocation per step without insertion |
| 5 | Reuse the buffers of the cohesive residual and energies | 1.4 | none | 1 day | unmeasured; the bulk equivalent gave −33 % to −37 % on the tet4 small-strain step |
| 6 | Overlap the stress exchange with the local assembly | 2.2 | none | 2 to 4 days | unmeasured; addresses the explicit step's parallel overhead (29 % efficiency 8 to 64 ranks) |
| 7 | Count agreements and record per-rank phase times (measurement only) | 4.4 | none | days | decides whether the error protocol's synchronisation points cost anything |
| 8 | Cached scatter map or fast hasher for sparse assembly | 1.6 | none | hours to 2 days | unmeasured; static-solve phase about 1.1 times upstream's at small and mid size |
| 9 | Profile, then fix the mesh distribution | 2.4 | none | days | up to the 70 s gap to upstream at 64 ranks, growing with the rank count |
| 10 | Loop order of `sparselu_gemm` and `trsm` | 1.7 | none | hours to 1 day | `sparselu_gemm` 18 % of the samples with the `eigen` backend |

Items 2.3 (serial explicit kernels, a few per cent at full size) and 4.1
(threads) come next: 2.3 is ready to implement from the existing analysis,
and 4.1 is the largest change that keeps bitwise parity.
