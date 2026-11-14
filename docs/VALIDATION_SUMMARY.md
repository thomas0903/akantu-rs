# Validation summary

| | |
|---|---|
| Purpose | The summary of the port's numerical validation, for a reader who knows Akantu and numerical analysis and has an hour |
| Evidence | Nothing here is new evidence. Every count is recomputed by `python3 scripts/docs/validation_counts.py` (fixture manifests, defect register, tolerance ledger) or `python3 scripts/parity/stats.py` (parity matrix), or quoted from the document named next to it |
| Detail | `docs/NUMERICAL_VALIDATION.md` ("NV" below; one section per area, cited by its title), `docs/MPI_VALIDATION.md` ("MPI"), `docs/TOLERANCE_LEDGER.md` (every tolerance rule), `docs/FEATURE_PARITY.md` (the parity matrix; its cells give the evidence row by row), `docs/UPSTREAM_DEFECTS.md` |

Contents: [1. Method](#1-method) · [2. Results per area](#2-results-per-area) ·
[3. Convergence](#3-convergence) · [4. Partition independence](#4-partition-independence) ·
[5. What is not validated, or validated less](#5-what-is-not-validated-or-validated-less) ·
[6. How to re-run](#6-how-to-re-run) · [Appendix: how the counts were taken](#appendix-how-the-counts-were-taken)

## 1. Method

### 1.1 The oracle

The C++ library at the reference commit `35a85178` is the oracle. It is
built out of tree in Docker and never edited (patches live in
`oracle/patches/`), in a strict configuration
(`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`) and a default one used only
to measure baselines. Probes (C++ and Python, `oracle/probes/`) run
upstream's own classes on chosen cases and write golden fixtures: one
directory per case with `manifest.json`, `.npy` data and the inputs;
tolerances live apart in `tolerances.json` files, so regenerating a fixture
never touches a rule. Intermediate states are recorded, not only results:
element kernels, quadrature, assembled operators, per-step fields, Newton
histories, energies (NV "Golden fixtures and the differential framework").

- Reproducibility (D-035). A golden comes from at least three identical
  fixed-seed runs; where a bitwise solve is wanted the probe selects
  upstream's Eigen sparse LU instead of its default MUMPS.
- Platform (D-043). x86_64 Linux (glibc, the oracle's Debian base, an
  x86-64-v3 CPU) is authoritative. The first corpus was generated on
  aarch64; it was regenerated on x86_64 and every difference classified
  (NV "Platform effects"). Cases whose values depend on the environment keep
  the x86_64 values at the top level and the others as platform variants
  (D-073; 40 quantities of 9 MUMPS-solved cases).
- Pins (D-050). Eigen's cache sizes are pinned to its own defaults (L1
  16384, L2 = L3 524288 bytes; they set the blocking, hence the summation
  order, of dense kernels and of the supernodal `SparseLU`), and BLAS to
  `OPENBLAS_CORETYPE=Haswell`, `OPENBLAS_NUM_THREADS=1`, in every probe,
  generator and example run. A comparison with an unpinned oracle is
  diagnostic. The port hardcodes the same blocking.
- Patched oracle variants (D-060). Where upstream does not compile or reads
  uninitialised memory, a numerics-preserving patch of a build copy (API
  adaptation, build fixes, zero-initialising memory upstream reads
  uninitialised; never a formula) can serve as labelled evidence, reviewed
  hunk by hunk. Used for the embedded model (§2.13), the anisotropic damage
  law and the friction contact cases.
- The gate. Every merge was judged on x86_64 inside the project's Docker
  images: fmt, clippy, every test binary accounted for, fixture checksums,
  and the MPI, distributed-model and MUMPS tiers (`docs/TESTING.md`).

### 1.2 Comparison classes

The tables of §2 give each comparison one of these classes (strongest
first). The codes B, B/D-039, E, S and I are those of MPI.

| Class | Meaning | Defined in |
|---|---|---|
| **B** | bit identity with the oracle's record (exact rule: `+0` ≠ `−0`, D-051; NaNs per D-046), or byte identity of the files written | NV "What bitwise means"; ledger "exact" |
| **B/D-039** | as B, except sums that upstream reduces over ranks (`MPI_Allreduce`) or in message-arrival order: the port's value equals the rank-order fold of the per-rank contributions exactly (the recorded ones where the probe records them, else the port's own, e.g. the explicit energies), and the recorded value equals their fold in the association observed for the oracle's Open MPI | D-039 and its amendment; MPI |
| **E** | D-082 envelope: upstream's own distributed result varies run to run through a solver the port does not run (MUMPS); the port is compared with **every** distinct upstream result of a study of at least 12 runs, bound 1e-14 relative to the maximum | D-082 |
| **X** | cross-solver: the oracle's record was solved by MUMPS, the port solves with its own direct solver (the Eigen `SparseLU` translation; faer in the distributed static case); a bound from the measured difference with a stated margin. The two loaded static cases also have Eigen-pinned bitwise twins (`model/smm/loaded_static_*`); the four patch cases have bitwise Eigen-pinned static patch tests of the same element types in `model/smm` | ledger, `fixtures/model/tolerances.json` [7]-[12], [14], [15]; `fixtures/parallel/tolerances.json` [2] |
| **R** | same terms, summed in another order for a documented reason (UD-408: upstream's interface order cannot be reproduced); bound `m ε max\|K\|` | ledger, `fixtures/model/embedded/tolerances.json` |
| **S** | the port at P ranks against the port's serial run, under a bound stated in the test | MPI; per test in NV |
| **U** | upstream's own test criteria and reference outputs (`.verified` files) at their printed precision | NV "Upstream `.verified` files" |
| **A** | analytical solutions and independent mathematical checks (patch tests, closed forms, invariants with a numerical bound) | NV, per area |
| **C** | observed order of convergence (spatial studies under rule CONV-1) | NV "Rules in force", "Solid mechanics model" |
| **I** | invariants without a numerical reference (ownership, global ids, agreed errors) | MPI |
| **D** | recorded but diagnostic only: not a test target | e.g. the oracle's MUMPS runs |

### 1.3 Tolerances

Every compared floating-point quantity resolves to a rule with a written
justification; a quantity without a rule is a test error, never a silent
default, and `crates/akantu-validation/tests/oracle_fixtures.rs` checks
that every quantity of every golden fixture resolves. A tolerance is never
loosened to make a test pass; changing one needs a numerical
justification, new evidence and an independent review (NV "Tolerance
method").

The rules are listed one per row with their justification in
`docs/TOLERANCE_LEDGER.md` (generated by
`scripts/gold/tolerance_ledger.py`, kept current by
`crates/akantu-validation/tests/tolerance_ledger.rs`): **240 rules in 44
files, all accepted by a review; 197 exact, 43 absolute / relative, none in
ULPs.** The 43 non-exact rules are the embedded model's reordered sums (20,
class R), the cross-solver rules of `fixtures/model` (8, class X), the
D-082 envelope of the distributed phase field (12, class E) and the static
distributed solid case (3, class X; two of them document quantities no test
compares).

Bounds of analytical, convergence and distributed-vs-serial checks are
written in the tests themselves (`DiffTest::check_f64_with`, explicit
asserts) and justified in the NV section of their area; they are not in
the ledger.

### 1.4 Upstream defects (D-003)

Defects of the reference are registered in `docs/UPSTREAM_DEFECTS.md`
and handled by category (D-003; the register's own definitions):

- A: undefined behaviour (uninitialised memory, out of bounds, crashes).
  The port defines the correct behaviour; differential tests use a patched
  oracle or analytical references.
- B: a clear bug with wrong results. Corrected by default, with a
  documented compatibility switch that reproduces upstream where practical
  (the oracle replays run in that mode and cite the UD id); needs an
  independent scientific review.
- C: a quirk or API semantics. Upstream's behaviour is reproduced by
  default and documented; an explicit opt-in may exist. The sub-label *C
  (documented difference)* (D-115) marks a deliberate, documented deviation
  of the port's API or file format where upstream has no numerical
  behaviour to keep and no bug to correct (e.g. UD-474); it counts with C.

The register has **596 entries**. By the category its *Cat.* column
starts with: **A 148** (20 of them also name another category, e.g.
`A/B`), **B 242** (10 also name another), **C 176** (8 also name another);
test infrastructure 20, build 3, scope 1, dead code 1, and 5 entries
without a category (withdrawn or not a defect). By status (first word):
538 implemented, 48 documented, 4 withdrawn, 2 superseded, 1 decided, 2
`registered` (candidate rows pointing to UD-263, UD-264 / UD-312) and 1
`not ported` (UD-790); **no entry is `open`** (D-115, §5.5;
`scripts/docs/validation_counts.py defects`).

### 1.5 Coverage in numbers

- **Parity matrix** (`python3 scripts/parity/stats.py`): 1,066 rows, 21
  superseded by finer rows; of the other 1,045, **1,019 Verified** and
  **26 approved exceptions**, 0 *Implemented, unverified*, 0 *In
  progress*, 0 *Not started*, 0 *Blocked*. The last three rows to become
  Verified were CI rows (CI-001, CI-014, CI-024), on the first runs of the
  delivered GitHub workflows on the public repository (§5, "CI"). A row is
  Verified only with test names, a golden fixture or analytical reference,
  a justified tolerance and an independent review (matrix header).
- **Golden fixtures** (`scripts/docs/validation_counts.py fixtures`):
  **2,023 case directories** with a `manifest.json` (a distributed case
  has one per rank), plus 62 recorded example runs (`fixtures/examples`),
  31 Python-example runs (`fixtures/python/examples`) and 16 dumper
  directories (`fixtures/dump`) that record files and digests instead of
  manifests. Integrity: `fixtures/SHA256SUMS`.
- **Upstream tests**: the TEST area of the matrix has 182 rows, 177
  Verified and 5 approved exceptions (upstream tests are ported to Rust
  tests with the same intent and named after them).

## 2. Results per area

Each table lists the main comparisons of an area: what is compared with
what, how many recorded cases (directories with a manifest, counted today;
a matrix cell may quote the count when it was accepted: fifteen rows,
MODEL-006, PF-001, PF-003 to PF-009, PF-011 to PF-014, PF-016 and PF-017,
quote 30 solver-model cases against 38 directories today (36 serial cases
and one two-process case) and 39 phase-field cases (42 in PF-013) against
43 today), the class (§1.2), the worst measured deviation where the class is
not bitwise, the tests and fixtures, and the NV section with the detail.
"—" in the deviation column means bitwise for class B; for classes A and I
it means that no single measured value is quoted here (the detail section
has them). The first line of each area gives its parity rows (Verified +
approved exceptions of the rows of the area).

### 2.1 Core utilities, dense algebra, parser, random numbers

Parity: CORE 35 / 35, PARSE 12 / 12, CL 8 / 8.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Math helpers (`Math::reduce`, scalar Newton, tolerant comparisons), `BBox`, CSR and circular arrays, spatial grid, strings, debug output vs oracle records | 13 (`fixtures/core/common`) | B | — | `akantu-core/tests/core_oracle.rs` | NV "Core utilities and dense algebra" |
| Library initialisation (`akantu::initialize`: CLI, input file, seeds, debug level) under `mpirun` | 36 (`fixtures/core/initialize`, per scenario × rank) | B | — | `akantu/tests/initialize_matches_oracle.rs` | same |
| Dense layer: small tensors, det / inverse, Voigt transfers, eigen-decompositions (general and symmetric, sizes 1 to 9 and 12) vs the strict oracle | `akantu-linalg/tests/fixtures/dense/dense_strict.txt`: 2,250 records (521 `eig`; 5,897 comparisons); `dense_gemm_strict.txt`: 812 product records (54,909 entries) | B | — | `akantu-linalg` dense tests | same |
| Parser: stream, CLI and registry records; every upstream input file | 153 upstream inputs (1 rejected by upstream too); `fixtures/parser` 2 | B (platform libm only where enumerated) | — | `akantu-parser/tests/` (`upstream_inputs`, `test_parser`, `oracle`) | NV "Parser" |
| Random numbers: `minstd_rand0`, 11 distributions, `RandomParameter`, the glibc `log`/`exp`/`pow`/`tan` they sample through (pure-Rust ports) | 22 seeds × 1000 values; 11 distributions × 2 to 6 parameter sets × 3 seeds × 1000; `fixtures/cl/random_internal` 2 | B ("tolerance: none", bit patterns, on every platform) | — | `akantu-core/tests/random_*.rs` | NV "Random numbers" |

### 2.2 Mesh, mesh utilities, I/O, dumpers

Parity: MESH 35 / 35, MUTIL 16 + 2 exceptions, IO 14 / 14, DUMP 24 + 5
exceptions, GOLD 13 + 2 exceptions, MDATA 6 / 6.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Mesh readers (Gmsh MSH 1, 2, 4.0, 4.1; Diana) on every upstream test mesh, the committed example meshes and 121 frozen Gmsh 4.13.1 meshes (D-009) | 226 (`fixtures/mesh_io`) + 28 (`fixtures/io/mesh_io`, reader probe) | B (coordinates bitwise, integers exact), except the documented deviations UD-045 (compat switch), UD-047, UD-093, UD-094 and 6 `tag_j` arrays upstream leaves uninitialised (UD-064, excluded) | — | `akantu-io/tests/mesh_io_fixtures.rs`, `mesh_io_probe_fixtures.rs` | NV "I/O and dumpers" |
| Mesh utilities: facet meshes, boundary groups, node → element CSR, clusters, `purifyMesh`, element removal, `makePeriodic` | 55 (`fixtures/mesh_utils`) | B | — | `akantu-mesh/tests/mesh_utils_oracle.rs` | NV "Meshes and mesh utilities" |
| Mesh container queries, type maps and filters, group dumps, Graphviz output, Scotch reordering and weighted partitions | 46 (`fixtures/mesh2`) | B (integers and written bytes) | — | `akantu-mesh/tests/mesh2_oracle.rs`, `mesh2_scotch_oracle.rs` | same |
| Dumpers (VTU / PVTU / PVD in base64 and ASCII, text, IOHelper, LAMMPS; group and model dumpers) vs upstream's files | 16 directories (`fixtures/dump`); model dumps in `fixtures/model/smm2` | B (byte identity; upstream's layout bugs only under `set_compat_layout`); upstream's uninitialised out-of-plane values (DUMP-029) excluded | — | `akantu-io/tests/dump_oracle.rs`, `test_dumper.rs`, `dump_smm.rs` | NV "I/O and dumpers"; parity DUMP rows |
| Upstream golden files (`.verified`, `K_matrix_test.dat`, …) replayed as upstream's driver compares them | GOLD rows | U; `K_matrix_test.dat` byte-identical to the pinned oracle | at upstream's printed precision | per GOLD row | NV "Upstream tests and golden files" |

### 2.3 Finite elements: element classes, quadrature, shape functions, FE engine

Parity: ELEM 27 / 27, QUAD 18 + 1 exception, FE 39 / 39.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Element classes: shapes, first and second derivatives, quadrature points and weights at every upstream-supported order, Jacobians, inverse map, normals, inradius | 23 element types (`fixtures/fe/element_class`, 1,035 arrays at their first generation; quadrature tables of 20 types, parity QUAD rows) | B | — | `akantu-fe/tests/element_class_matches_oracle.rs`, `fe_engine_properties.rs` | NV "Finite-element layer" |
| FE engine on meshes: interpolation, gradients, integration, precomputation, cohesive and extra quantities; real-coordinate inverse map | 12 + 7 + 12 + 12 (`fixtures/fe/{engine,engine_cohesive,engine_extra,engine_real}`) | B (GEMM-class products in Eigen's `gebp` order) | — | `fe_engine_matches_oracle.rs`, `fe_engine_extra_matches_oracle.rs`, `fe_engine_real_matches_oracle.rs` | same |
| Structural element kernels (Bernoulli beams 2-D / 3-D, DKT18) | 8 (`fixtures/fe/structural`) | B | — | `structural_matches_oracle.rs`, `structural_element_class_records.rs` | same, "Structural element kernels" |
| Independent checks on the same records: Gauss exactness (> 500 moments), partition of unity, affine identities, inverse map | — | A | — | `akantu-validation/tests/oracle_fixtures.rs` | same |

### 2.4 Sparse algebra, DOF manager, solvers, time integration, boundary conditions

Parity: SPM 13 / 13, SOLV 7 / 7, DOF 17 / 17, NLS 14 / 14, TS 16 + 1
exception, BC 12 / 12, MODEL 11 / 11.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| AIJ assembly, products, Dirichlet terms, Matrix Market bytes; the Eigen `SparseLU` translation (permutations, elimination tree, factors, solutions) | 15 (`fixtures/solver`) | B (pinned blocking, D-050) | — | `akantu-linalg/tests/eigen_sparse_lu_oracle.rs`, `upstream_solver_tests.rs` | NV "Sparse algebra and direct solvers" |
| faer sparse LU (alternative back-end) against the oracle's solutions | the same systems | A: LAPACK `dget02` backward error < 30, a finite-perturbation bound against the oracle | (per case, `-- --nocapture`) | `faer_qualification.rs` | same |
| DOF manager, Newton-Raphson (plain, modified, contact, residual criteria), time-step solvers and every integration scheme (Newmark-β family, central difference, trapezoidal, Fox-Goodwin, linear acceleration, generalised trapezoidal, forward / backward Euler) on upstream's `test_model_solver` model, damped cases included | 38 (`fixtures/solver_model`: 36 serial cases and the two-process trace case, one directory per rank) | B (histories, per-step digests) | — | `akantu-solver/tests/test_model_solver.rs`, `test_dof_manager.rs` | NV "DOF manager, non-linear solvers and time integration" |
| Orders of the integration schemes on `ü + u = 0` | — | C | observed 2.000 (trapezoidal, linear acceleration, central difference), 4.000 (Fox-Goodwin) | `integration_scheme_checks.rs` | same |
| MUMPS back-end vs upstream's `SparseSolverMumps` on the same systems (pinned OpenBLAS) | 13 systems | B at 1 rank with the same OpenBLAS core type (D-050); D at several ranks (UD-348: upstream not repeatable) | — | `mumps_mpi.rs`, MUMPS tier | NV "MUMPS back-end", D-050 |
| PETSc back-end (KSP, SNES, TAO; the models on the PETSc DOF manager) vs the pinned x86_64 serial record | `fixtures/solver/petsc/oracle/` | B; bounded TAO vs the closed-form obstacle solution (A); the dynamic run within rule PETSC-DYN (energies 1.41e-15) | TAO 3.0e-16, KKT residual 2.8e-14 | PETSc tier (`docs/TESTING.md`) | NV "PETSc back-end" |
| Boundary conditions: functors, Dirichlet / Neumann on periodic meshes | `fixtures/model/smm/{bc_functors_*,periodic_static_*}` | B (10,428 values on the periodic cases) | — | `model_smm_fixtures.rs` (`periodic_bc_functors_bitwise`) | parity BC rows; NV "Solid mechanics model" |

### 2.5 Solid mechanics model and constitutive laws

Parity: SMM 32 / 32, MAT 31 / 31.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Constitutive laws at a point under prescribed strain histories (elastic family incl. thermal, finite deformation, anisotropic, orthotropic; Marigo, Mazars, plastic Mazars; J2 linear isotropic hardening incl. finite deformation; Drucker-Prager; SLS; Maxwell; Neo-Hookean kernel; phase-field laws; anisotropic damage) | 229 (`fixtures/material`) | B, in upstream-compat mode where a UD applies (where the corrected default computes, the parity MAT cells give its own counts, e.g. MAT-018, MAT-023; where it refuses (finite-deformation J2 and damage, anisotropic damage, thermal Maxwell) only the compat mode is compared, §5.3) | — | `akantu-solid/tests/material_fixtures*.rs`, `inelastic_material_fixtures.rs`, `neohookean_kernel_fixtures.rs` | NV "Materials" |
| `SolidMechanicsModel` static, explicit and implicit, Eigen-pinned oracle: K, residuals, fields, energies per step, dumps, events, interpolation, 12 element types | 128 (`model/smm`) + 18 (`smm2`) + 4 (`v87`) + 3 (`upstream_tests_2b`) + 2 (`layout`) + 2 (`explicit`) + 1 (`smm_init_solver`) | B | — | `model_smm_fixtures.rs`, `smm2_*.rs`, `model_fixtures.rs` | NV "Solid mechanics model" |
| Finite-deformation assembly (total Lagrangian), static and explicit | 82 (`model/smm_fd`; 1,620,091 non-NaN values, parity SMM-023) | B | — | `model_smm_fd_fixtures.rs` | same |
| Static patch and loaded cases solved by MUMPS in the oracle, by Eigen `SparseLU` in the port | 6 (`model/static`) | X (rules rel 1e-11 / 1e-10). The two loaded static cases also have Eigen-pinned bitwise twins (`model/smm/loaded_static_*`); the four patch cases have bitwise Eigen-pinned static patch tests of the same element types in `model/smm` | 8.5e-16 (patch), 2.9e-15 (loaded), relative to the maximum | `model_fixtures.rs` | ledger `fixtures/model/tolerances.json` [7]-[12]; NV "Solid mechanics model" |
| Implicit-dynamic plastic patch (MUMPS in the oracle) | 1 (`model/implicit_dynamic`) | X (rel 1e-10 per step; Newton error abs 1e-7 N) | per step u 4.3e-16, v 1.44e-14, a 2.9e-14, energies 7.5e-16 (relative); final Newton residuals differ by ≤ 4.8e-10 N | `model_fixtures.rs::implicit_dynamic_plastic_patch_replay` | ledger [14], [15]; same |
| Spatial convergence (12 element types; plane strain, plane stress, explicit standing wave) | — | C | §3 | `akantu-solid/tests/spatial_convergence.rs` | same |

### 2.6 Cohesive model

Parity: COH 13 / 13, CMAT 6 / 6.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Intrinsic and extrinsic insertion, cohesive laws, selectors (default, mesh data, element data, rules), fragments, implicit cases, finite-deformation bulk | 81 (`model/cohesive`; at the last full count 75 cases, 70 compared bitwise, 5 recording an upstream exception; the release tier 1,518,374 values bitwise on x86_64, parity COH-001) | B (upstream-compat switches where a UD applies) | — | `akantu-cohesive/tests/cohesive_fixtures.rs` (3-D and adversarial explicit cases in the release tier) | NV "Cohesive model" |
| Cohesive dumping (upstream layout, base64 and text) | 10 (`model/cohesive_dump`) | B (every file byte for byte, compat switch) | — | `cohesive_dump_oracle.rs` | same |
| `cohesive_linear_friction` (2-D / 3-D, modes I / II, implicit) | in `model/cohesive` | B; UD-225 (A: no NaN under penetration with zero tangential opening) | — | `cohesive_fixtures.rs`, friction regression tests | same |
| Laws upstream cannot run: `cohesive_exponential` (upstream aborts, UD-028), `cohesive_linear_fatigue` (UD-221) | — | A (closed-form envelopes, energy balance after insertion) | — | `akantu-cohesive/tests/independent_checks.rs` | parity CMAT-003 / 004; same |
| Upstream's `test_cohesive` at 2 ranks with the port's partition handed to upstream | 80 (`parallel_test_cohesive`, 40 cases × 2 ranks: 30 in 2-D, 10 in 3-D) + 80 boundary records | B (rule `ptc_exact`) | — | `upstream_cohesive.rs::two_ranks::*` | NV "Parallel models" |
| Distributed cohesive model | §2.15 | | | | MPI |

### 2.7 Contact mechanics (generic contact, split-node contact, contact couplers)

Parity: CONT 18 + 1 exception, TASN 43 / 43, CPL 7 / 7 (shared with
§2.10).

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Generic contact: detection (flat, rotated 30° / 60°, adversarial ridges, rotations, curved 3-D), moduli, explicit dynamics (2,000 steps), explicit friction (10,000 steps, UD-001 variant oracle) | 24 (`model/contact`) | B under the compat switches of UD-002, UD-363, UD-365, UD-368 | — | `akantu-contact/tests/` (`contact_detection.rs`, `contact_explicit.rs`, `contact_model.rs`) | NV "Contact mechanics" |
| Corrected projection metric (UD-363) on rotated interfaces vs the analytical gap | — | A (rule rel 1e-11) | 1.28e-13 (30°), 7.2e-14 (60°), 3.97e-14 (upstream metric, 30°) | `contact_detection.rs` | same |
| Couplers solid-contact and cohesive-contact: static Hertz (100 Newton steps, 52,084 values), static friction, cohesive + contact explicit (7,500 steps), callback (3,000 steps), static and implicit-dynamic cohesive set-ups | 8 (`model/couplers`, non-`pf_*`) + `model/contact/explicit_{dynamic,friction}` | B; the corrected contact defaults reach the same states (A), except the corrected contact-search order of UD-1291 (B, D-109, switch `search_in_corrector_only`), which departs from upstream on `cohesive_implicit_dynamic_qs` and replays with the switch | corrected vs upstream mode ≤ 1.5e-14 relative (in-code bound 1e-10) | `coupler_fixtures.rs`, `contact_coupler_fixtures.rs`, `contact_static.rs`, `friction_history.rs` | NV "Model couplers"; parity CPL-001…004 |
| Split-node contact (NTN, NTRF, rate-and-state friction), restart files | 49 (`model/ntn_contact`) | B (restart files byte for byte) | — | `akantu-ntn-contact/tests/oracle_ntn_fixtures.rs`, `oracle_ntn_adversarial.rs` | NV "Split-node contact (traction at split nodes)" |
| `InclinedFlatDirichlet` (does not compile upstream, UD-403) | — | A | — | per TASN-043 | parity TASN-043 |
| Distributed split-node contact | §2.15 | | | | MPI |

### 2.8 Structural mechanics

Parity: STRUCT 13 / 13.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Structural model (Bernoulli beams 2-D / 3-D, DKT18, a mixed beam / shell case): static, line loads, implicit and explicit dynamics, lumped mass, dumps, in upstream-compat mode (`StructuralModelUpstreamCompat::UPSTREAM`) | 21 (`model/structural`, incl. 8 adversarial cases) | B (Eigen-pinned oracle) | — | `akantu-structural/tests/structural_model_matches_oracle.rs` | NV "Structural mechanics model" |
| Corrected defaults where upstream is wrong (UD-005 lumped mass always zero, UD-007 DKT bending ∝ t⁴), exact lumped rotary inertia of 3-D beams (D-059) | — | A, C (beam frequencies: consistent mass O(h⁴), lumped O(h²); simply supported DKT plate → Navier solution; torsion O(h²) once `h < h*`, ratios 4.00) | e.g. torsional frequency −1.116e-5 relative at 96 elements | `structural_model_dynamics_checks.rs`, `structural_model_plate_checks.rs`, `lumped_rotary_inertia.rs` | same |

### 2.9 Diffusion (heat transfer)

Parity: DIFF 12 / 12.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Heat diffusion: static, explicit (lumped capacity), implicit, laws, patch tests (upstream's heat-transfer tests, never built upstream: UD-014, built in the oracle with the package name patched) | 72 (`model/diffusion`: 56 cases, ≈ 2.05 M values, + 16 adversarial cases) | B (UD-287 solver switch on for the replays) | — | `akantu-diffusion/tests/model_diffusion_fixtures.rs`, `model_diffusion_v102_fixtures.rs` | NV "Diffusion (heat transfer)" |
| Spatial convergence: static isotropic (12 types) and anisotropic (10) conductivity, transient P1 at fixed Fourier number | — | C | §3 | `akantu-diffusion/tests/spatial_convergence.rs` | same |
| Distributed diffusion | §2.15 | | | | MPI |

### 2.10 Phase field and the solid-phase-field coupler

Parity: PF 18 / 18, CPL 7 / 7.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Phase-field model: AT1 / AT2, energy splits, `history` and `penalization` irreversibility, error paths | 43 (`model/phase_field`; 39 cases and 76,438 values when parity PF-001 was accepted) | B in upstream-compat mode; each corrected switch differs exactly where it is declared to | — | `akantu-phase-field/tests/phase_field_fixtures.rs` | NV "Phase field" |
| Bounded TAO irreversibility on the PETSc DOF manager | PETSc records | B | — | PETSc tier | NV "PETSc back-end" |
| Coupler `CouplerSolidPhaseField` (staggered), incl. upstream's `test_multi_material` failure and a converging multi-material run; the twelve cells split × law × irreversibility through the coupler | 12 (`model/couplers/pf_*`) + 4 TAO records (`fixtures/solver/petsc/oracle/pftao/cpl_tao_*`) | B under every upstream switch of the coupled path, and again with the corrected defaults plus only the switches a cell needs | — | `akantu-couplers/tests/coupler_fixtures.rs`, `petsc_models.rs::coupler_tao_oracle` (PETSc tier) | NV "Model couplers" |
| Spatial convergence at fixed `l0`: AT2 and AT1 (12 types), coupled displacement and damage (4 2-D types) | — | C | §3 | `akantu-phase-field/tests/spatial_convergence.rs`, `akantu-couplers/tests/spatial_convergence_phase_field.rs` | NV "Phase field", "Model couplers" |
| Distributed phase field | §2.15 (B at 1 rank, E at 2 / 4) | | | | MPI; D-082 |

### 2.11 Non-local framework and extra materials

Parity: NL 8 + 1 exception, XMAT 16 / 16.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Non-local framework and laws: pairs, weights, volumes, Jacobians, averages over two averaging calls, law internals and internal force over 14 steps; weight-function rate sweep | 33 (`model/non_local`: 15 cases, 17 adversarial cases, 1 sweep) | B | — | `akantu-solid/tests/non_local_fixtures.rs`, `non_local_adversarial.rs` | NV "Non-local framework and laws" |
| Extra materials (`brittle`, `damage_iterative`, `damage_linear`, `iterative_stiffness_reduction`, `viscoplastic`, `ve_stiffness_prop`, orthotropic damage, Vreepeerlings, their non-local variants, the RVE homogenisation): **no oracle** (the package does not compile upstream) | — | U (upstream assertions verbatim; `.verified` 44 lines identical) and A (closed forms, rule XMAT-T1 rel 1e-12) | XMAT-T1: worst 2.7e-16 over 34 checks; elastic homogenisation 5.1e-11 (bound 1e-10) | `akantu-extra-materials/tests/` (`point_laws.rs`, `upstream_damage_iterative.rs`, …) | NV "Extra materials" |
| Distributed non-local laws | §2.15 | | | | MPI |

### 2.12 IGFEM

Parity: IGFEM 11 / 11. **No oracle**: the package does not compile at the
reference commit (D-007).

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| FE engine of the IGFEM elements vs the `.verified` outputs of upstream's last working implementation (TEST-193…196) | > 500 printed values | U (6 significant digits, upstream's print) | at 6 digits | `akantu-igfem/tests/test_fe_engine.rs` | NV "IGFEM" |
| Partition of unity, kinked-field reproduction, Kronecker property, two independent shape evaluations, sub-element quadrature | 6 structured meshes + a 1-D mesh | A (bounds 4ε; 1e-13 / 1e-12; 1e-14) | — | `independent_checks.rs` | same |
| Patch tests (TEST-190, 191) and interface positions (TEST-192) | — | U, A | L2 error 1.8e-16 (`_igfem_triangle_4`), 2.5e-13 (`_igfem_triangle_5`, criterion 1e-12) | `akantu-igfem/tests/` | same |
| Circular inclusion (TEST-200) vs `.verified` (29 printed displacements equal) and vs the exact Eshelby-type solution | — | U; C as a regression guard only (rate of the two finest meshes ≥ 1.8) | measured rate 1.87 (`n = 32 → 128`), not yet asymptotic | `circular_inclusion_convergence`, `off_centre_inclusion_convergence` | same |
| Live interface moves, damage steps, extrapolation, weighted averages, dumpers (IGFEM-004…010) | — | U (upstream test programs' criteria), A | live move vs fresh model: mesh bitwise, solution ≤ 1e-13; `standard_fem` vs `igfem` 41 damage steps `\|Δd\|` ≤ 1e-12; brute-force average 9.8e-16 of `Σ W J \|f\| / Σ W J` (bound 1e-12 on that scale) | `akantu-igfem/tests/` | same |

### 2.13 Embedded interface model

Parity: EMB 5 / 5. Reference: the **patched oracle variant** `embedded`
(D-060; upstream's model does not compile at the reference commit).

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Model replays (K, u, f_int, law energies) vs the patched oracle | 15 (`model/embedded`) | B (rule EMB-BIT) where the interface order is upstream's; R (rule `emb_order`) for the 4 case families where UD-408 makes upstream's order irreproducible | f_int 1.45e-13 of the maximum (t127; bound 1e-12, margin 6.9), u 1.07e-15; the other families ≤ 2.0e-16 | `akantu-embedded/tests/oracle_fixtures.rs` | NV "Embedded interface model"; ledger `fixtures/model/embedded` |
| Upstream's three tests with their criteria; the 2015 golden `matrix` (91 entries) byte for byte; prestress reactions vs transformed-section beam theory | — | U | prestress 4.18e-4 relative (criterion 1e-3) | `test_embedded_*.rs` | same |
| Closed-form stiffness and rigid-body modes, affine patch tests, bar in a matrix, interface topology | — | A | — | `embedded_checks_{2d,3d}.rs` | same |
| Upstream example `embedded_model`: crashes on its own second-order mesh even on the patched variant (UD-851, cause UD-1140); the first-order version of its mesh is recorded on the variant (`fixtures/examples/embedded_eigen_p1`) | — | B for geometry and loads, R (`emb_order`) for `u`, `f_int` and stress on the first-order mesh; A only on the example's own mesh | — | `akantu/tests/examples_embedded.rs`, `example_embedded_model.rs` | NV "Examples"; UD-851 |

### 2.14 Geometry

Parity: GEOM 9 / 9.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Kernel predicates and constructions; spherical kernel (line-arc-sphere: 3,000 samples, 793 tangents); AABB trees (primitive lists, intersection order); segment intersector (upstream mode incl. libstdc++ set order, 1,797 grid segments); sphere intersector (44 runs, 23 of them UD-413 error runs, vs a compile-fixed and zero-fill-patched class); adversarial cases | 17 (`fixtures/geometry`) | B (in upstream mode; the corrected default modes have no oracle) | — | `akantu-geometry/tests/oracle_kernel.rs`, `akantu-geometry-cgal/tests/oracle_geometry.rs`, `oracle_adversarial.rs` | NV "Geometry" |
| libstdc++ ordered-container behaviour, reproduced clean room (D-030) | black-box fixtures (GCC 14.2, x86_64) | B | — | `akantu-geometry/tests/fixtures/cleanroom/` | parity GEOM-009 |

### 2.15 Parallel execution

Parity: MPI 53 / 53, PART 17 + 1 exception. Full table with every rank
count, partitioner and test: MPI "Summary per distributed model". Cases are
per-rank records (`np<P>/rank<r>`).

| Model / feature | Records | Ranks (oracle) | Class | Worst deviation | Detail |
|---|---|---|---|---|---|
| Mesh distribution (Scotch, rule and RCB partitions; bounding boxes) | 50 (`parallel/distribution_*`, `pbd_strip_*`) | 1 / 2 / 4 / 8 (`distribution_cube_msh`), 1 / 2 / 4 | B | — | MPI |
| Facet, grid and periodic synchronizers | 43 + 36 + 72 (`facets_*`, `grid_*`, `periodic_*`, `ud1060_*`, `v101_*`) | 1 / 2 / 4 / 8 on one case each, else 1 / 2 / 4 | B; periodic `_sum` B/D-039 (exact mask of the arrival-order entries) | — | MPI; NV "Communication and distribution" |
| Solid mechanics, explicit (one and two laws) | 22 | 1 / 2 / 4 / 8 (two laws: 1 / 2 / 4) | B/D-039; S vs serial | the two `MPI_Allreduce` associations differ by ≤ 1.861e-16 relative (kinetic), both folds exact; serial (bound 1e-12): \|du\| ≤ 1.179e-18 of 6.835e-4 | MPI |
| Solid mechanics, static | 3 (`smm_static_cube_n3`) | 1 / 2 | X (MUMPS vs the port's solver, rule 1e-12); S | oracle 1.920e-16; serial (1e-10) \|du\| ≤ 2.776e-17 of 1.452e-1 | MPI; UD-348 |
| Solid mechanics, implicit dynamic | — | — (port only) | S | cube (1e-10) \|dv\| ≤ 1.518e-18 of 1.243e-3; bars (TEST-109, 1e-12) \|dv\| ≤ 1.235e-15 of 6.103e-2 | MPI |
| CL-Marigo random fields | 7 | 1 / 2 / 4 | B | — | MPI |
| Cohesive, intrinsic / extrinsic, facets-only, clusters, fragments | 107 (`parallel_cohesive`) | 1 / 2 / 4 / 8 (two 2-D scenarios), 1 / 2 / 4 | B/D-039 (energies, fragment integrals) | — | MPI; NV "Parallel models" |
| Diffusion | 15 | 1 / 2 / 4 / 8 | B (gradient); S | serial: explicit (1e-13) \|dṪ\| ≤ 1.634e-13 of 9.151e1, implicit (1e-12) ≤ 3.264e-14 of 1.345 | MPI |
| Phase field and coupler | 13 | 1 (B), 2 / 4 (E) | B, E, S | E: 3.55e-15 over every recorded upstream result (bound 1e-14); serial (1e-14) \|dd\| ≤ 2.220e-16 of 5.224e-1 | MPI; D-082 |
| Non-local laws | 51 (`nl_*`) | 1 / 2 / 4 / 8 (two scenarios), fewer for the others | B under the compat switches UD-1101…1103; S with the corrected defaults | corrected vs serial ≤ 2.381e-15 (damage, bound 1e-14), 1.722e-14 (internal force, bound 1e-12) | MPI |
| Split-node contact | 45 (`ntn_*`, `ntrf_coulomb`) | 1 / 2 / 4 / 8 | B/D-039 (external work) | — (rank-ordered work 1 ulp from upstream's `MPI_Allreduce` at 8 ranks, both folds exact) | MPI |
| PETSc and MUMPS back-ends on several ranks | — | — (no multi-rank oracle: UD-006 / UD-429, UD-348) | S | MUMPS vs serial Eigen: static linear 9.592e-15 (bound 1e-12); PETSc asserted bounds 1e-12 / 1e-14, not re-measured in MPI | MPI |
| Python module, mesh level | — | 1 / 2 / 4 | B | 0 (7 / 7 rank outputs equal) | MPI |
| Lifecycle and error propagation (agreed failures, aborts, finalisation) | — | 1 / 2 / 4 / 8 | I | — | MPI "Lifecycle and error propagation" |

### 2.16 Python interface

Parity: PY 85 + 2 exceptions.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| Upstream's Python examples run **unchanged** with the port's package and with upstream's module in the same pinned image | 31 (`fixtures/python/examples`; where a script solves with MUMPS, the unmodified run is diagnostic and its Eigen-pinned `_eigen` variant is the target) | B ("no tolerance"), with four documented rules only: UD-110 matrix arrays, UD-114 empty arrays, UD-061 Matrix Market digits, UD-085 dangling view not compared; four cases depend on values NumPy computes alone and are compared bitwise only on the oracle's platform (two of them only with NumPy's AVX-512F kernel) | 0 | pytest `python/tests/examples` | NV "Python interface" |
| Bindings: zero-copy views, lifetimes, error mapping, the API surface | — | I (behaviour) and B where the matrix row cites an oracle comparison | — | pytest, `akantu-python` tests | same |
| Models on several ranks from Python | — | **not provided** (D-101) | | | MPI |

### 2.17 Examples

Parity: EX 51 / 51.

| Compared | Cases | Class | Worst deviation | Tests, fixtures | Detail |
|---|---|---|---|---|---|
| The C++ examples, port vs the upstream binary: SHA-256 of every output file, VTU array digests, standard output (three identical oracle runs per case, pinned) | 62 (`fixtures/examples`) | B (byte-identical files or bitwise arrays; `example_parser_eigen` and `implicit_dynamic_eigen` exact since the D-050 pin) | — | `crates/akantu/tests/examples_*.rs` (whole runs in the medium tier) | NV "Examples" |
| Examples run by upstream with MUMPS (`static`, `example_parser`, `implicit_dynamic`): solve-independent arrays | in the 62 | B; the solved fields are D | `example_parser` displacement 1.04e-15 from MUMPS with the D-050 pin (diagnostic) | same | same |
| `parallel_2d` with each code's own partition (no shared partition) | in the 62 | D for the displacement (printed, not asserted) | u 6.3e-12 of the maximum after 10,000 steps; v 3.9e-14, a 3.6e-14 (asserted at 1e-11 of each field's maximum, a partition-noise check) | `examples_parallel_2d` | same |
| Python examples | §2.16 | | | | |

## 3. Convergence

**Method and rule.** The spatial studies use smooth, non-polynomial
manufactured solutions on unit cubes meshed by subdivision and mapped by a
fixed smooth distortion (so quadrangles and hexahedra are not
parallelograms and second-order elements are curved), with Dirichlet data
on the whole boundary and the consistent load of `−div σ(u)`. The model's
matrices and residuals are the library's; loads, error integrals and
derivatives use the test's own quadrature (Gauss-Legendre / collapsed,
exact to total degree 7) and differences of the library's `N`, so a
defect of a library rule or of `dN/dξ` changes the measured orders instead
of cancelling. Expected orders: L2 `p + 1`, energy `p` (Ciarlet-Raviart
mapped elements; serendipity types on asymptotically parallelogram
meshes; Aubin-Nitsche on the convex cube). **Rule CONV-1**: for levels
`n → 2n`, `|r_obs − r_th| ≤ 1/N` with `N = n·p` of the coarser mesh; the
`1/N` form follows from the error expansion, the constant 1 is
**empirical** (largest measured `d·N` over the four studies 0.99, a margin
of 1.01×). Acceptance needs, for every type, one asserted pair with band
≤ 1/8 in a run of the merge gate, where `|d| + band < 0.5` holds (largest
0.25), so a defect of half an order fails it. A failing pair may be
resolved as pre-asymptotic only if its deviation halves at the next pair
and the finest asserted pair is within its band; it then stays in the run
as reported only (NV "Rules in force").

**Results.** *Solid mechanics* (`akantu-solid/tests/spatial_convergence.rs`):
all 12 regular element types in plane strain / 3-D, the four 2-D types in
plane stress, and an explicit standing P-wave at fixed CFL (h and Δt
together) converge at their theoretical orders; findings: none. Mutants
of a quadrature weight and of a `_quadrangle_8` derivative are killed by
the study (the derivative mutant passes the linear patch test).
*Diffusion, phase field, coupled* (three binaries of 27, 27 and 5 tests):
static isotropic (12 types) and anisotropic (10) diffusion, transient
diffusion at fixed Fourier number, AT2 and AT1 damage equations at fixed
`l0` (12 types), and the coupled solid / phase-field problem through the
coupler (4 2-D types, both fields) converge at their orders; findings:
none; one pre-asymptotic pair (phase field `_hexahedron_20`, `2 → 4` at
`h = l0`) is resolved under CONV-1's conditions and kept as reported only.
Six mutants (conductivity off-diagonal, the `l0` factor, the coupler's
strain ½, a weight, a derivative, the capacity) are each killed by a study;
three of them (the off-diagonal, the `l0` factor, the derivative) pass
every patch test. *Structural*: beam frequencies converge at O(h⁴)
(consistent mass) and O(h²) (lumped), the simply supported DKT plate
converges to Navier's solution, the lumped rotary inertia's torsion at
O(h²) (NV "Structural mechanics model"). *IGFEM*: the circular inclusion
against the exact Eshelby-type solution is guarded by a rate ≥ 1.8 on the
two finest meshes; the measured 1.87 is not yet the asymptotic rate.
*Time integration*: observed orders 2.000 and 4.000 (Fox-Goodwin) on
`ü + u = 0` (NV "DOF manager, non-linear solvers and time integration").

**Where an order is not the criterion**: the cohesive model (cracks follow
element boundaries; the meaningful criterion is mesh objectivity of the
global response once the cohesive zone is resolved, and **no such study
exists**), and node-to-segment penalty contact (its consistency error
depends on the penalty; the port reproduces upstream's discretisation
bitwise instead). Not covered: Γ-convergence of the phase field as
`l0 → 0`, the non-linear energy splits and the `penalization` / `tao`
modes in a convergence study (they have bitwise oracle fixtures), a 3-D
coupled case.

## 4. Partition independence

`docs/MPI_VALIDATION.md` assesses every distributed model (summarised in
§2.15 above). Two kinds of evidence: **oracle comparisons at a fixed
partition** (upstream's seeded Scotch partition recorded by the probe and
handed to the port, or a rule written in both the probe and the test), at
1 / 2 / 4 ranks and at 8 ranks for the scenarios recorded there; and the
**port at P ranks against its own serial run** (RCB partitions, 1 / 2 / 4 /
8 ranks, bounds between 1e-14 and 1e-10 stated per test; the measured
values are in MPI). The distributed-model tier ran every binary three times
per rank count. Upstream itself depends on the rank count or the run in
documented places, and the port's comparisons are built around them rather
than through a tolerance (MPI "Where upstream itself depends on the rank
count or the run"): the association of `MPI_Allreduce` in the oracle's Open
MPI (rank order at 2 and 3 ranks, `(0+1)+(2+3)` at 4, pairwise at 8;
D-039), arrival-order sums of the synchronizers (exact masks), ghost
filters in `waitAny` order (UD-232, pinned by the probes), multi-rank MUMPS
(UD-348, D-082), and upstream's corruption of ghost boundary elements at
distributed extrinsic insertion (UD-890: the port keeps the serial
problem).

**Partition independence of distributed cohesive runs with loaded faces:
corrected in the port, not in upstream** (UD-1280, D-105 as amended by
D-108). Upstream leaves the accumulated Neumann load of a doubled node with
the copy that keeps the original identity, which follows the facet
orientation, which on a distributed mesh depends on the partition; the C++
oracle does the same bit for bit. The port's default moves each boundary
element's recorded load to the copy that element uses, by node identity;
the compat switch `doubled_node_keeps_external_force` keeps upstream's
placement for the oracle replays. Evidence: the per-copy forces at 2 / 4
ranks equal to the serial run (`crates/akantu-bench/tests/doubled_node_load_side.rs`),
the same with unequal nodal shares on upstream's test meshes
(`crates/akantu-cohesive/tests/doubled_node_unequal_shares.rs`), a closed
form (`crates/akantu-cohesive/tests/doubled_node_neumann.rs`) and the
h = 0.06 benchmark at 2 / 4 / 8 ranks keeping the serial insertion list for
2,500 steps (NV "Cohesive model"). The cohesive oracle scenarios drive the
bodies by prescribed velocities and displacements and do not exercise this
placement. The full-size benchmark run C1 (five of six points on one AWS
node) ran before the correction and depends on the rank count in both
codes; no count or energy of it is compared between rank counts or codes as
a correctness criterion. Its rerun with the corrected placement (the port
only, 64 and 32 ranks, 2026-11-13; MPI "The full-size run (C1)",
`docs/BENCHMARKS.md`) keeps the same insertion counts for 3,176 steps and
ends 4 elements apart (8.5e-6) with energies within 3.5e-5; judged against
the statement D-108 wrote before it, parts 1 (a) and 1 (c) are met and
**1 (b) is not met as written** (the external work's difference first
exceeds 1e-10 of the step's largest energy at step 2,716, before the
counts part; its jump criterion holds). The comparison shows no
partition-dependent placement within its detection window (≈ 2,700 steps),
and a control between two runs of the same binary and partition (one node
against eight hosts), where no placement can differ, leaves rounding at the
same step (2,656 against 2,653). The per-copy check of every copy's load
against a re-application, at every step to 3,200 on the full-size
partitions at 64 and 32 ranks, found no failure in 3,201 checks each
(largest difference 2.57e-15 of the largest nodal load): it proves the
placement mechanism through step 3,200, not the boundary connectivity after
an insertion nor the traction integration, and only for boundary elements
whose nodal shares are equal, as the benchmark's are. With unequal shares,
such as the four-point `_quadrangle_4` of hexahedral cohesive meshes, the
placement was corrected afterwards (records kept by node identity; verified
at 1 / 2 / 4 ranks on upstream's test meshes): these full-size runs predate
that correction and describe the corrected code to rounding.

Refused on several ranks, with the reason (MPI "What is refused on several
ranks, and why"): the generic contact model (CONT-019, approved exception
D-089: upstream is serial only), the structural model (UD-902: upstream's
distributed residual is wrong), parallel IGFEM (D-085: does not compile
upstream), a distributed 1-D facet mesh (UD-1141: upstream refuses), the
non-local `KeyOrder` route (D-097), split-node contact on partitions that
separate a pair (UD-1105), split-node contact without `init_parallel`
(UD-904), phase-field element events, and the Python module at model level
(D-101). Multi-node execution was run once on 2026-11-13 (MPI "Multi-node
execution and scaling"): one job across eight AWS hosts at up to 64 ranks;
with the Eigen static solve the 8-rank run on eight hosts is byte-identical
to the one-host run (h = 0.12, 1,500 steps), and the full-size runs across
eight hosts have the single-node run's distribution and differ from it by
MUMPS's run-to-run rounding only. Every other run of this summary is on one
node.

## 5. What is not validated, or validated less

### 5.1 Approved exceptions (26 rows of the parity matrix)

Not ported, by a recorded decision:

- upstream code that is never built or never called (D-028): QUAD-018
  (unused pentahedron tables), MUTIL-009 and PART-014 (legacy PBC maps),
  MUTIL-018 and TEST-214 (segment node-type test of a removed API),
  DUMP-019…023 (legacy restart and C facades, base64 helper), TS-012
  (dead explicit time-step solver), TEST-213 (`generate_test`), TEST-231
  (stand-alone PETSc dynamic test), TEST-234 (CI helper headers);
- dead or uninstantiable upstream code: NL-009 (abstract
  `NeighborhoodMaxCriterion`), PY-068 (a binding of a template upstream
  never instantiates), PY-049 (`dumper_module`, not importable from the
  oracle's module; D-079), TEST-211 (never built since 2017, removed API,
  empty reference file; D-076);
- **CONT-019, distributed contact search and resolution** (D-089):
  upstream's generic contact model has no distributed protocol; the port
  refuses a distributed mesh;
- GOLD-003 (D-087) and GOLD-007 (D-085): the byte comparison with two
  legacy `.verified` files and six 2017 `.out` files written through
  removed APIs;
- CI-007 (`akantu_diff.sh`, referenced nowhere), CI-008 (build-time Gmsh
  mesh generation: platform-dependent meshes, frozen instead, D-009),
  CI-023 (Jenkins) (D-076);
- CI-017 (version determination, D-114): the port takes the version from
  `Cargo.toml` and checks it against the release tag, where upstream
  derives it from `git describe`; CI-022 (deploy and release, D-114): no
  publication for now, the dry run exists.

### 5.2 No reference from upstream as it is

- **IGFEM**: upstream's package does not compile at the reference commit
  (D-007). Evidence: the `.verified` outputs of its last working version at
  6 printed digits, upstream's test criteria, independent checks and
  analytical solutions (§2.12). Its convergence study is a regression guard
  (1.87, not asymptotic). The physical values at enriched nodes
  (`compute_values_on_enriched_nodes`) are accurate only to `|∇u| · 1e-10`
  when an enriched node lies within 1e-10 of its parent node 0, as upstream
  (NV "IGFEM").
- **Extra materials**: 11 of 12 translation units do not compile; the one
  that does instantiates nothing. Evidence: upstream assertions, one
  `.verified` file, closed forms (§2.11).
- **Embedded model**: compared with a **patched** oracle variant (D-060),
  not with upstream as it is; the patch restores compilation and two
  released behaviours and changes no formula (reviewed hunk by hunk). The
  upstream example `embedded_model` crashes even on the variant (UD-851) on
  its own second-order mesh, which is checked analytically only; its
  first-order version is compared with the variant (§2.13).
- Single features upstream cannot run: `cohesive_exponential` (aborts,
  UD-028), `cohesive_linear_fatigue` (UD-221), `penalty_quadratic`
  contact (cannot be constructed, UD-370), `InclinedFlatDirichlet` (does
  not compile, UD-403), the Neo-Hookean law's stress (upstream
  `NotImplemented`, UD-026; compared through a probe-only kernel). They
  rest on closed forms and independent checks.
- **The corrected defaults of category-B defects** are compared with the
  oracle only through the compatibility switch that reproduces upstream;
  the corrected behaviour itself rests on analytical checks, patched oracle
  variants where one exists, and the scientific review D-003 requires.
  Likewise the geometry intersectors' default (corrected) modes have no
  oracle (§2.14).

### 5.3 Refused by default

Where upstream's result is undefined or wrong and no reviewed formulation
exists, the port refuses the configuration with an error by default. A
compatibility switch reproduces upstream, and only that mode is compared
with the oracle. A user who runs such an upstream input gets an error, not
a validated result:

- finite-deformation J2 plasticity (`FiniteStrainPlasticity::Unsupported`,
  UD-030; `UpstreamLegacy` reproduces upstream) and finite-deformation
  damage laws (`FiniteStrainDamage::Unsupported`, UD-142);
- the anisotropic damage law (`AnisotropicDamageModel::Unsupported`,
  UD-031 / UD-058; the `Upstream` mode is compared with a zero-initialising
  oracle variant, and on the unpatched oracle only through the quantities
  that do not depend on uninitialised memory: the unpatched 2-D tangent is
  not part of the x86_64 corpus; the specified `Corrected` model is an
  opt-in awaiting its independent review);
- Maxwell viscoelasticity with thermal loading, `alpha ≠ 0` (UD-327);
- intrinsic insertion with `cohesive_linear` (UD-224), and extrinsic
  insertion of `_cohesive_3d_8` / `_cohesive_3d_16` (UD-223: no oracle
  case is possible);
- the `residual_mass_wgh` convergence test (UD-194);
- bounded `petsc_tao` solves whose bounds exclude a zero increment
  (UD-1022, D-088);
- finite deformation in the IGFEM laws (UD-613).

Many more entries refuse inputs that upstream accepts with undefined
results (invalid parameters, non-finite values, inconsistent restart
headers). The register's *Decision* column names each. The refusals on
several ranks are listed in §4.

### 5.4 Comparisons weaker than bit identity

- **X** (MUMPS in the oracle, Eigen `SparseLU` in the port): the six static
  patch / loaded cases (worst 2.9e-15), the implicit-dynamic plastic patch
  (worst 2.9e-14 per step), the static distributed solid case (1.920e-16);
  the two loaded static cases also have Eigen-pinned bitwise twins
  (`model/smm/loaded_static_*`); the four patch cases have bitwise
  Eigen-pinned static patch tests of the same element types in
  `model/smm` (§2.5, §2.15).
- **R** (embedded, UD-408 order): worst 1.45e-13 relative, margin 6.9
  (§2.13).
- **E** (distributed phase field, D-082): 3.55e-15 against every recorded
  upstream result, bound 1e-14 (§2.15).
- **D** (diagnostic only, no test target): every MUMPS-solved field of the
  oracle (examples, Python scripts, multi-rank MUMPS, UD-348), comparisons
  with an unpinned oracle (D-050 (3)), `parallel_2d` with each code's own
  partition (displacement 6.3e-12, printed only).
- The PETSc dynamic run is not bitwise (energies within 1.41e-15, rule
  PETSC-DYN) and multi-rank PETSc has no oracle (§2.4, §2.15).
- **The strict-vs-default baseline** of the oracle is recorded as
  indicative only; the rules that cite it say so in their evidence
  (`fixtures/model/tolerances.json` [14], [15]).
- In-code bounds of the analytical and distributed-vs-serial checks are
  not in the ledger; each is justified in its NV section.
- faer is qualified against the oracle on the 1-D bar histories of the
  solver toy model only; on 2-D and 3-D implicit runs it is compared with
  the serial Eigen solve (NV "Sparse algebra and direct solvers").

### 5.5 Upstream defects reproduced on purpose, and open defects

**176 entries whose category starts with C** reproduce upstream's behaviour
by default (those labelled *C (documented difference)* are deliberate,
documented deviations of the port's API or file format, D-115; the
*Decision* column of each says which). Reproduced knowingly, whatever the
category: the linear convergence of Newton with J2 plasticity, because
upstream's tangent is the elastic one on purpose, not the consistent one
(UD-079, C, D-115: the port returns the same tangent and documents it, and
iteration counts are compared exactly); the corrector's increment of
displacement-type solutions, the last Newton correction (UD-202, C,
D-115); the Newmark parameter naming (UD-017, C); the
distributed-on-one-rank semantics (UD-011, C); and, in the compat mode of
the oracle replays only, the doubled-node load placement (UD-1280,
category B per D-105: corrected by default).

**Open entries: none** (D-115). Every entry carries a settled category and
status; UD-890 is B, corrected without a switch (the port keeps the serial
problem).

### 5.6 Tolerance rules still PROPOSED: none

Every rule of every `tolerances.json` file names the review that accepted
it (`docs/TOLERANCE_LEDGER.md`).

### 5.7 Remaining limits of resolved problems

- The corrected Neumann load placement on doubled nodes (§4) places the
  loads as integrated: after an insertion rewrites a boundary row, the
  boundary engine's integration data are not refreshed (UD-1296, category
  C, D-117), and a mutation that renumbers nodes and then adds nodes is
  refused while loads are recorded.
- Per-DOF extraction on distributed periodic meshes: upstream makes every
  extracted vector periodic-consistent; the port's extraction is local and
  does so only for the vectors it makes consistent beforehand (the solution
  and the distributed products). A user vector with rank-local values at
  non-owned periodic master copies keeps them there (parity DOF-011); the
  models' lumped matrices are assembled from periodic-consistent arrays and
  agree.
- The MPI tier's performance gate bounds the backend ratio against raw MPI
  only; the API's own copy cost is not gated.
- The distributed-model test binaries still contain per-rank verdicts
  before a collective call (about 60), which would hang a run instead of
  failing it if a rank disagreed; the ones found in the node-removal and
  law scenarios were fixed.
- Open MPI's launcher (PRRTE 3.0.8) has a teardown defect and dumps core on
  multi-program launches; the tier bounds the rank processes' end and
  records the launcher's teardown (`docs/DEPENDENCIES.md`).

### 5.8 Other limits

- Refused on several ranks: the features listed at the end of §4 (MPI).
- Multi-node runs: one cluster of eight hosts, up to 64 ranks, one run
  per point (MPI "Multi-node execution and scaling"); nothing beyond 64
  ranks or on other interconnects, and no oracle recorded across hosts (the
  observed `MPI_Allreduce` association is a one-node fact). The full-size
  rerun's D-108 statement is not met in its part 1 (b) as written (§4).
  8-rank oracle comparisons exist only for the scenarios listed in MPI "The
  8-rank oracle cases"; the 3-D cohesive scenarios have no 8-rank rule; the
  PETSc tier's multi-rank values were not re-measured by the MPI report.
- Convergence: no mesh-objectivity study of the cohesive model; no
  Γ-convergence study of the phase field; no 3-D coupled study (§3).
- Platform: the port's default complex division reproduces the aarch64
  runtime; on x86_64 the oracle's complex eigenvectors are reproduced
  bitwise only with `ComplexDivision::Unfused` (NV "Platform effects"). The
  parser calls the platform libm; on macOS 18 enumerated last-bit
  differences remain, other platforms have no measured list. A few other
  call sites (`powf` in the cohesive linear law and the contact slip
  moduli, `atan2`, `atan` and `tan` in the anisotropic damage law) also
  call the platform libm: identical to the oracle on glibc, possibly
  different in the last bit on macOS, with no exception list (NV
  "Platform effects"). Examples whose
  input calls libm directly are authoritative on x86_64 glibc only.
- CI: of the 29 rows of the CI area (pipelines, images, wheels, release
  jobs), 24 are Verified and 5 are approved exceptions (CI-007, CI-008,
  CI-017, CI-022, CI-023; D-076, D-114); they carry no numerical claim. 21
  rows were Verified on replays of their jobs. The other three (every job
  green on a GitHub runner, CI-001; the macOS arm64 wheel job, CI-014; the
  documentation deployment, CI-024) were open at delivery: a first run on
  14 November 2026, while the repository was private, had 21 of 29 jobs
  green, and its causes were fixed off GitHub (D-118, D-119). They closed
  the same day, when every workflow ran once on the public repository and
  all 30 jobs passed (`docs/TESTING.md`, "State at release"). One of the findings bears on a numerical comparison: two
  Python example cases whose input NumPy computes alone are compared bit for
  bit only where NumPy uses the oracle host's AVX-512F kernel, and are
  skipped with that reason elsewhere (NV "Python interface").
- Performance and scaling are reported elsewhere (`docs/BENCHMARKS.md`) and
  are not part of this validation.

## 6. How to re-run

1. Fast tier, including every differential test on the committed
   fixtures: `cargo test --workspace`; fixture integrity:
   `cd fixtures && sha256sum -c SHA256SUMS`.
2. Tolerance ledger and counts: `python3 scripts/gold/tolerance_ledger.py --check`,
   `python3 scripts/docs/validation_counts.py`, `python3 scripts/parity/stats.py`.
3. MPI: `scripts/mpi/run-tests.sh` (1 / 2 / 4 ranks) and
   `scripts/mpi/run-par-tier.sh --ranks "1 2 4 8" --repeat 3` in the project
   images (`docs/TESTING.md`).
4. Oracle side: regenerate a fixture area with its generator on the pinned
   x86_64 oracle (`docs/ORACLE_BUILD.md`; `oracle/scripts/generate-fixtures.sh`,
   `oracle/probes/*/generate.sh`) and compare trees with
   `cargo run -p akantu-validation --bin fixture-diff -- <a> <b>`.

## Appendix: how the counts were taken

| Count | Source and rule |
|---|---|
| Parity rows (1,066 / 1,045 / 1,019 / 26 / …) and the per-area lines of §2 | `python3 scripts/parity/stats.py` (the generated block of `docs/FEATURE_PARITY.md`) |
| Fixture cases (2,023 and the per-group counts of §2) | `python3 scripts/docs/validation_counts.py fixtures`: directories under `fixtures/` with a `manifest.json`; a distributed case has one per rank (`np<P>/rank<r>`). Example runs, Python-example runs and dumper directories are counted as directories |
| Upstream-defect categories and statuses | `python3 scripts/docs/validation_counts.py defects`: the *Cat.* column's leading A / B / C ("+" when the cell names a second category); rows are split as GitHub splits them, and a row with another cell count than the register's is an error (exit 1) |
| Tolerance rules | `docs/TOLERANCE_LEDGER.md` (marker line), regenerated by `scripts/gold/tolerance_ledger.py` |
| Values, measured deviations, case counts at acceptance | quoted from the NV section, MPI section, ledger rule or parity cell named in the same row |
