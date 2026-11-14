# Numerical validation

This document states how akantu-rs is compared with the C++ Akantu library
and what the comparison shows, area by area. The first four sections give
the method: the oracle and what bit identity means here, the golden
fixtures and the differential test framework, the tolerance method, and the
effects of the platform. The sections after them give, for each area of the
parity matrix, what is compared with the oracle, how, with how many cases
and values, which independent mathematical checks back it, which upstream
defects change the comparison, and what is not compared or is compared
more weakly. The last section lists the commands that rerun the evidence.

Related documents: `VALIDATION_SUMMARY.md` (one table per area, the
convergence studies, partition independence and the full list of what is
not validated), `MPI_VALIDATION.md` (every distributed model, rank count
and partitioner), `TOLERANCE_LEDGER.md` (every tolerance rule, generated),
`UPSTREAM_DEFECTS.md` (the register of defects of the reference, `UD-…`),
`FEATURE_PARITY.md` (the parity matrix; its rows cite the tests named
here) and `TESTING.md` (how to run each tier).

## 1. Principles

### 1.1 The oracle

The reference is Akantu at commit
`35a851780d0b9471d258ed53a003ae98aaa95ea0` (`v5.0.7-post1-554-g35a851780`).
It is built out of tree in Docker and never edited; the few build
adjustments it needs are patches kept in `oracle/patches/` and recorded in
the provenance of every fixture (`ORACLE_BUILD.md`). Probes, small C++ and
Python programs that call upstream's own classes, run chosen cases and
write golden fixtures: the inputs, every intermediate state that matters
(element kernels, quadrature, assembled operators, per-step fields, Newton
histories, energies, written files) and the results. The port reads the
same inputs, computes the same quantities and compares them with the
record.

Two builds of the oracle exist. The strict build
(`-O2 -ffp-contract=off -DEIGEN_DONT_VECTORIZE -frounding-math`) is the one
every golden comes from. The default build keeps upstream's own flags,
with FMA contraction and vectorised Eigen. It serves only to measure how
much the oracle itself moves under a legitimate change of flags; that
baseline is indicative and is cited as such by the rules that use it
(section 3.3).

The platform of record is x86_64 Linux with glibc on an x86-64-v3 CPU (AVX2
and FMA), inside the oracle's Debian trixie image (D-043). The corpus was
first generated on aarch64 and then regenerated on x86_64; every difference
was classified (section 4). Values that agree on both platforms keep their
original files; where the x86_64 oracle differs, its values are the golden
ones and the aarch64 values are kept as a platform variant.

### 1.2 What bitwise means

Most comparisons are exact. An exact comparison of two `f64` passes when
their bit patterns are identical, with one exception: two default quiet
NaNs that differ only in the sign bit match (D-046). IEEE 754 leaves the
sign of the NaN produced by an invalid operation to the hardware; aarch64
gives `0x7ff8000000000000`, x86_64 SSE gives `0xfff8000000000000`, and the
oracle and the port both produce their own platform's NaN. Payload NaNs,
signalling NaNs and NaN against a number stay violations. `+0` and `−0` do
not match (D-051). Infinities and finite values are compared bit for bit.
Integers are always compared exactly. Digests of states hash the values
with default NaNs canonicalised and nothing else changed
(`akantu_validation::exact::canonical_bits`), so a digest recorded on
aarch64 is valid on x86_64.

A golden case that holds NaN must list every NaN-bearing quantity with its
exact count in its manifest `notes`; the corpus test
`oracle_fixtures.rs::every_nan_in_a_golden_fixture_is_documented` checks the
list, the counts and that every NaN is a default quiet NaN. Reductions in
the tests use NaN-propagating maxima (`akantu_validation::reduce`), so
`worst < tol` fails on NaN data instead of skipping it.

Comparisons of written files are byte comparisons. Comparisons with
upstream's printed reference outputs (`.verified` files) use the rounding
interval of the printed format (section 2.5).

### 1.3 What makes bit identity reachable

The port computes the same operations in the same order as the strict
oracle. The rules that follow from that, enforced in review and by the
tests:

- `f64` wherever upstream uses `Real`; no fast-math, no reassociation, no
  approximate intrinsics.
- No fused multiply-add unless the reference computes one. The strict build
  forbids contraction, so the port's arithmetic is unfused, with two
  documented exceptions where the reference itself runs fused code: the
  aarch64 runtime's complex division (section 5.1) and the glibc libm
  routines whose x86-64-v3 variants use FMA (section 5.3).
- Formulas are translated term by term, with upstream's parenthesisation,
  loop order and accumulation order. Where Eigen decides the order (dense
  products, `SparseLU`), the port reproduces Eigen 3.4.0's evaluation:
  `akantu_core::eigen_gebp` replays the blocked GEMM kernel with its
  blocking computed from Eigen's default cache sizes (L1 16384, L2 = L3
  524288 bytes), and `akantu_core::eigen_order` and the `SparseLU`
  translation follow Eigen's supernodal factorisation step by step
  (sections 5.1 and 5.7).
- Library routines whose results enter the record are reproduced. glibc's
  `log`, `exp`, `pow`, `tan`, `sin` and `cos`, libstdc++'s random
  distributions and ordered containers, and libgcc's complex division are
  reproduced bit for bit. Where the licence did not allow translation
  (GPL code of GCC, D-030), the behaviour was reproduced clean room from
  the specification and black-box experiments on the oracle image, and the
  experiments are committed with the code. LGPL, MPL and BSD-style code
  (Eigen, CGAL's LGPL kernel) is translated with its notices; CGAL's GPL
  packages are ported only into the separate GPL crate
  `akantu-geometry-cgal` (D-042).
- The oracle is pinned where its own result depends on the machine (D-050):
  Eigen is compiled with `-DEIGEN_NO_CPUID` and its default cache sizes, and
  OpenBLAS runs with `OPENBLAS_CORETYPE=Haswell` and
  `OPENBLAS_NUM_THREADS=1` in every probe, generator and example run. A
  comparison with an unpinned oracle is diagnostic only.
- Where upstream solves with MUMPS by default (its `auto` solver), the
  probes that need a bitwise solve select upstream's Eigen `SparseLU`
  backend instead; the MUMPS records are kept as cross-solver evidence
  (class X, section 2.6).

### 1.4 Upstream defects

The reference has defects. Each confirmed one gets an id `UD-…` in
`UPSTREAM_DEFECTS.md` and a category (D-003):

- A, undefined behaviour (uninitialised memory, out-of-bounds access,
  crashes): the port defines the behaviour. Comparisons use the defined
  part of upstream's output, a patched oracle or analytical references.
- B, a bug with wrong results: corrected by default in the port, with a
  compatibility switch that reproduces upstream where that is practical.
  The oracle replays run with the switch on and cite the UD id. The
  corrected behaviour rests on analytical checks, on a patched oracle where
  one exists and on an independent scientific review.
- C, a quirk or an API semantics: upstream's behaviour is reproduced by
  default and documented; an opt-in may exist. The sub-label *C (documented
  difference)* marks a deliberate difference of the port's API or file
  format where upstream has no numerical behaviour to keep (D-115).

Where upstream's result is undefined or wrong and no reviewed corrected
formulation exists, the port refuses the configuration with an error by
default and offers the upstream behaviour behind a switch; only the switch
is compared with the oracle (`VALIDATION_SUMMARY.md` lists these).

Where upstream does not compile or cannot run a feature, two routes exist.
A patched oracle variant (D-060) restores compilation in a build copy
with patches that adapt APIs, fix the build or zero-initialise memory that
upstream reads uninitialised, and never change a formula; each hunk is
reviewed, and fixtures from such a build name the patch in their
provenance. Otherwise the evidence is upstream's own test criteria and
reference outputs, closed-form solutions and independent checks. A class-B
correction is never compared with the buggy value under a widened
tolerance.

## 2. Golden fixtures and the differential framework

### 2.1 Layout

One directory per case; the case id is its path under `fixtures/`:

```
fixtures/
  SHA256SUMS                  checksum of every generated committed file
  <area>/tolerances.json      area rules
  <area>/<case>/
    manifest.json             provenance and the semantics of every array
    tolerances.json           optional case rules
    data/<quantity>.npy       one array per quantity ('/' in names -> '__')
    inputs/…                  copies of the inputs (meshes, .dat, histories)
    variants/<environment>/   values of another numerical environment (section 4.2)
```

Arrays are NumPy `.npy` files (format 1.0, little-endian, C order): raw
IEEE 754 values with no decimal round trip, written by a dependency-free
C++ writer (`oracle/probes/include/akantu_probe/fixture_writer.hh`) or by
NumPy (byte-identical, `selftest_fixtures.rs`) and read by a checking Rust
reader (`crates/akantu-validation/src/npy.rs`).

`manifest.json` (unknown fields rejected) records the status (`golden` is
the only comparable one), the probe, its version and the regeneration
command, the reference commit and the oracle's patches with content hashes,
the build (configuration, the library's and the probe's actual compile
flags, architecture, library versions, image id, hashes of the library and
of the probe sources), the inputs with checksums, free parameters (material
constants, the non-linear solver and its stopping rule, measured
conditioning), the parallel layout, the recorded steps, and per quantity its
file, checksum and one axis name per dimension from a closed vocabulary
(`step`, `iteration`, `node`, `element`, `quad`, `node_local`, `component`,
`row`, `col`, `dof`, `natural_dim`, `spatial_dim`, and a few more).
Conventions a reader of the data needs: per-point data are
`[step?, element, quad, …]` per element type and ghost type; matrices are
stored `[row, col]` row-major (Akantu stores tensors column-major, so the
probes transpose; asymmetric sentinels guard this); Voigt quantities
record upstream's order and engineering shears; `dnds`, `dndx` and `jmat`
keep upstream's layouts; sparse matrices are triplets read from
`getIRN`/`getJCN`/`getA` (upstream's `saveMatrix` prints 15 digits); Newton
histories are ragged; inputs the probe generates are recorded with an exact
rule and fed back unchanged. Akantu's static residual is `f_ext + f_int`;
the time integrator adds `−M a`. An error-path case passes only when the
test observes the expected error kind and message.

A case is committed when its data are at most 1 MiB; a larger one commits
its manifest with checksums and a regeneration command. Two dense text
corpora of `crates/akantu-linalg` (3.2 MB and 3.9 MB) are committed above
that limit under a named exception, capped at 4 MiB each. Finalisation of a
generation run is transactional: a failure leaves every file unchanged.

### 2.2 Probes and generation

The probes live in `oracle/probes/` (C++ under `cpp/` and per-area
directories, Python under `python/`), one or more per area; each section of
this document names the probe of its fixtures. They record and never
compare: every sanity check of oracle data is in Rust
(`crates/akantu-validation/tests/oracle_fixtures.rs`: Gauss rules integrate
monomials exactly to their degree, partition of unity, `Σ ∂N/∂s = 0`,
constant `det J` and exact volume of affine elements, inverse-map round
trips, unit normals, recorded conditioning within the bounds the rule
justifications cite), so a broken probe cannot hide behind its own check.
Element-class kernels are header templates compiled into the probe, so the
probes are compiled with the library's floating-point flags.

`oracle/scripts/generate-fixtures.sh` runs a target in the oracle image with
the reference checkout, the oracle volume and the probes mounted read-only.
It refuses a reference checkout that is not exactly the pinned commit with
a clean tree, and an install whose recorded commit, configuration or image
does not match. Every probe writes into a private staging directory; only
when every target succeeded does `finalize_fixtures.py` promote the cases,
apply the size policy and rewrite `SHA256SUMS`. It never writes
`tolerances.json`. Generators of the later areas (`oracle/probes/*/
generate.sh`, `oracle/x86/regenerate-corpus.sh`) follow the same rules;
`ORACLE_BUILD.md` gives the procedure.

Determinism rule (D-035): a golden comes from at least three identical runs
of the probe with fixed seeds, data and inputs byte-identical, manifests
identical apart from their timestamp and the host fields. Output that is
not reproducible, because upstream reads uninitialised memory or depends on
message arrival order, is never committed as golden: the mesh generator,
for instance, runs its probe twice and accepts a difference only when it is
in the reviewed allow-list and explained by its defect (the six `tag_j`
arrays of UD-064). Such quantities are compared through their defined part
or masked exactly (sections 5.6 and 5.23).

The 121 meshes that upstream generates with Gmsh at build time are frozen:
generated once with the oracle image's Gmsh 4.13.1 by
`oracle/probes/mesh_io/freeze-meshes.sh` and committed under
`fixtures/meshes/gmsh-4.13.1/` with provenance and checksums (D-009). Gmsh
gives other meshes on another architecture (section 4.1).

### 2.3 The differential framework

`crates/akantu-validation` holds the reader, the comparator, the reports
and the helpers. A differential test opens a case, reads the inputs, runs
the port and checks each quantity:

```rust
let mut t = DiffTest::new("fe::shapes_triangle_6", "fe/element_class/_triangle_6")?;
for order in 1..=5 {
    let pts = t.reference_f64(&format!("quad_points/order_{order}"))?;
    let n = shapes::<Triangle6>(&pts.data);
    t.check_f64(&format!("shapes/order_{order}"), &n); // rule from tolerances.json
}
t.assert_pass();
```

- `DiffTest::new` refuses a case that is not golden or not materialised,
  and opens the values of the running numerical environment (section 4.2).
- The `check_*` calls never panic: a missing quantity, a missing rule, a
  shape mismatch or a rule whose grouping axes the quantity lacks are
  recorded as failures. `finish` refuses a report without any compared
  quantity; empty arrays count, their shape is compared.
- The report (text and JSON) gives per quantity the outcome, the rule with
  its origin and justification, counts, the entries with the largest
  absolute, relative and ULP error and the largest fraction of the
  tolerance used, each located by axis names with values and bit patterns,
  the first violations, histograms, statistics per component, per step and
  per acceptance group, and the first failing step.
- On failure the test writes `target/akantu-validation/<test>/` with the
  report, the port's arrays as a `rust_snapshot` fixture and, when the
  inputs were not already a fixture, a `pending_oracle` case that
  `generate-fixtures.sh regression:<case_id>` turns into a golden one
  (exercised by `fixtures/regression/material/replay_elastic_2d`).
- Orderings that may legitimately differ are declared per axis in the
  manifest (`fixed` by default; `global_id`, `coordinates`, `connectivity`,
  `unordered`) and normalised by `akantu_validation::ordering` before the
  comparison.
- Distributed cases have one directory per rank (`np<P>/rank<r>`) with the
  local-to-global maps, node flags, ghost lists and the partition. The
  Rust test feeds upstream's recorded partition, or a rule written into
  both the probe and the test, so no partitioner difference enters the
  comparison. Tests that use the port's own partitioner compare with the
  port's serial run instead.
- `fixture-diff` (`cargo run -p akantu-validation --bin fixture-diff -- <a>
  <b>`) compares two fixture trees value by value and reports bit identity
  separately; it serves regeneration checks and measurements, never a pass.

### 2.4 Solver acceptance

A tolerance bounds the port's difference from the oracle; it does not show
that the port's own solve met its stopping rule. Whenever a test checks a
convergence quantity (`newton/nb_iterations`, `newton/error`,
`newton/residual_norm`), `akantu_validation::solver` checks the port's own
values against the rule of the solver recorded in `parameters.solver`, per
step. For Akantu's Newton-Raphson solvers (norms are 2-norms over the free
DOFs, the test is `error < criterion`): with the `residual` criterion,
`|r_0| < threshold` stops with no iteration, otherwise the criterion is
`|r_0| · threshold` and must be met at the last iteration and at no earlier
one; with the `solution` criterion, `|du_k| < threshold` with at least one
iteration; at most `max_iterations`. The history must have
`nb_iterations + 1` finite, non-negative entries and `error` must equal the
last tested norm bit for bit. Solver types without a rule here are refused,
never accepted. `tests/solver_acceptance.rs` keeps the counterexample this
was built against: adding 9e-5 N to every final residual of the
implicit-dynamic fixture passed every difference check while no step met
`|r_n| < 1e-10 |r_0|`.

### 2.5 Upstream `.verified` files

Upstream's test driver exits before it compares output with the
`.verified` files (UD-018), so those files are untrusted until confirmed
against the oracle's own run. `akantu_validation::verified` parses both
texts with a declared printing format per column (integer, C++ default
`%.6g`, fixed, scientific) and gives each value the rounding interval of its
format: exact for integers and for a printed zero, half a unit of the last
digit otherwise, asymmetric at powers of ten. Unrepresentable tokens are
errors. A text reference carries about six significant digits, so it is a
weak check and never replaces a probe fixture for the same quantity.

### 2.6 Value classes

Each comparison in this document has one of these classes (strongest
first). `VALIDATION_SUMMARY.md` and `MPI_VALIDATION.md` use the same
letters.

| Class | Meaning |
|---|---|
| B | bit identity with the oracle's record (section 1.2), or byte identity of the written files |
| B/D-039 | as B, except sums that upstream reduces over ranks or in message-arrival order: the port's value equals the rank-order fold of the per-rank contributions exactly, and the recorded value equals their fold in the association observed for the oracle's Open MPI (section 5.23) |
| E | envelope (D-082): upstream's own distributed result varies from run to run through a solver the port does not run (MUMPS); the port is compared with every distinct upstream result of a study of at least 12 runs, bound 1e-14 relative to the maximum |
| X | cross-solver: the oracle's record was solved by MUMPS, the port solves with its Eigen `SparseLU` translation (faer in one distributed case); a bound from the measured difference with a stated margin |
| R | the same terms summed in another order for a documented reason (UD-408); bound `m ε max|K|` |
| S | the port at P ranks against the port's serial run, under a bound stated in the test |
| U | upstream's own test criteria and reference outputs at their printed precision |
| A | analytical solutions and independent checks (patch tests, closed forms, invariants) with a numerical bound |
| C | observed order of convergence (rule CONV-1, section 3.4) |
| I | invariants without a numerical reference (ownership, global ids, agreed errors) |
| D | recorded but diagnostic only, not a test target |

"Ordering-insensitive" comparisons are B after the declared normalisation of
section 2.3; they are marked where they occur.

## 3. Tolerance method

### 3.1 Acceptance criterion

For a recorded value `e` and the port's value `a`, both finite, an entry
passes when

```
|a − e| ≤ abs + rel · scale(e)        or        ulp_distance(a, e) ≤ ulps
```

The scale is `|e|` entry-wise, the largest `|e|` of the quantity
(`max_abs`, for fields with zero crossings), per component, per step, per
group of axes (`max_abs_per_group`, e.g. one scale per tensor component
over a history) or a fixed value. A grouped scale on a quantity that lacks
one of its axes is an invalid tolerance and fails; it is never replaced by
a whole-array scale. NaN must meet NaN (under a non-exact rule any NaN, or
`forbid`), infinities must match exactly. A rule with `abs = rel = 0` and no
ULP allowance is exact (section 1.2).

### 3.2 When a tolerance is allowed

The default is bit identity. A tolerance is allowed only where the port
cannot compute the oracle's operations in the oracle's order for a stated
reason, and each kind of reason has its own class (section 2.6): the
oracle solved with MUMPS (X), upstream's order is irreproducible (R),
upstream's result varies from run to run (E), the comparison is with an
analytical solution (A), a convergence order (C) or the port's serial run
(S). A tolerance is never introduced or loosened to make a test pass. A
failing comparison is investigated: a defect of the port is fixed, a defect
of the oracle is handled under D-003 (section 1.4), and a wrong rule is
changed only with a written numerical argument, new evidence and an
independent review. Tightening is always allowed.

A bound is derived from the operation, never chosen to fit:

- for a reordered sum of `m` terms, `m ε max|term|`;
- for a linear solve, `κ(K) (η_oracle + η_port)` with the condition number
  measured by the probe (`parameters.conditioning`: `cond2_K_free`, the
  backward error) and the backward errors measured on both sides;
- for a Newton iterate, a value far below the stopping threshold, with the
  stopping decision itself checked by solver acceptance (section 2.4);
- for a time history, the step-1 bound grown by the measured variation over
  the recorded steps;
- for an analytical comparison, the truncation error of the method or a
  multiple of `ε` with the derivation written next to the bound.

The measured value and the margin are written into the rule's evidence.
`oracle_fixtures.rs::recorded_conditioning_supports_the_tolerance_justifications`
checks that the recorded conditioning stays within the values the
justifications cite.

### 3.3 Where tolerances live

Rules live in `tolerances.json` files next to the fixtures, per area and per
case, resolved from the most specific file to its ancestors; inside a file
the first matching rule wins. A rule has a quantity glob, an optional case
glob, the criterion, a written justification, the measured evidence and the
review that accepted it (`reviewed_by`; a rule without it is reported as
`PROPOSED` and cannot support a *Verified* parity claim). There is no
implicit default: a quantity without a rule is an error, and
`oracle_fixtures.rs` checks that every floating-point quantity of every
golden fixture resolves to an applicable, justified rule. Regenerating a
fixture never touches a rule.

`TOLERANCE_LEDGER.md` lists every rule with its criterion and justification;
`scripts/gold/tolerance_ledger.py` writes it and
`crates/akantu-validation/tests/tolerance_ledger.rs` fails when a rule and
its ledger row disagree. Today: 240 rules in 44 files, all accepted, none
proposed; 197 exact and 43 absolute or relative, none in ULPs. The 43
non-exact rules are the embedded model's reordered sums (20, class R), the
cross-solver rules of `fixtures/model` (8, class X), the envelope of the
distributed phase field (12, class E) and the static distributed solid case
(3, class X; two of them describe quantities that no test compares).

Bounds of analytical, convergence and distributed-against-serial checks
are written in the tests themselves (`DiffTest::check_f64_with`, explicit
asserts) and are justified in the area sections below; they are not in the
ledger.

Upstream test tolerances (for example `5e-14` in
`test_fe_engine_gauss_integration`) are kept by the ported test where they
are a property of the test; the differential checks next to them use the
rules above.

### 3.4 Rules in force

The changes made to the rules since the first fixtures reduce to these:

- Element kernels, FE-engine quantities, materials, models and the dense
  layer were first given ULP or small relative bounds; every one of them is
  now exact, after the port reproduced upstream's order (including Eigen's
  GEMM kernel order) and every value was observed bit-identical.
- Exact rules distinguish `+0` from `−0` (D-051) and accept only the
  sign-only default-NaN difference (D-046). The dense layer and the
  comparators of other crates use the same convention. The libm
  special-value tables are the one documented exception: they record every
  NaN result as `0x7ff8000000000000` and compare NaN as a class, because
  glibc's propagation of NaN inputs is platform-specific; every non-NaN
  result stays bitwise.
- `newton/error` of the implicit-dynamic MUMPS case is a difference
  diagnostic with an absolute bound of 1e-7 N, not a convergence check
  (section 5.9).
- Grouped scales must match how rounding enters the quantity: per tensor
  component where each component's rounding is relative to itself, whole
  tensor where components are coupled or vanish by cancellation. The rule's
  justification says which.
- Spatial convergence is accepted under rule CONV-1: for levels `n → 2n`,
  `|r_obs − r_th| ≤ 1/N` with `N = n·p` of the coarser mesh. The `1/N` form
  follows from the error expansion; the constant 1 is empirical (the
  largest measured `d·N` over the four studies is 0.99). Acceptance needs,
  for every element type, one asserted pair with band at most 1/8 in a run
  of the merge gate, where `|d| + band < 0.5` holds (largest 0.25), so a
  defect of half an order fails.

## 4. Platform effects

### 4.1 What differs between x86_64 and aarch64, and why

The whole corpus was regenerated on x86_64 (AMD Zen 5 hosts, AVX2, FMA and
AVX-512) from the same Dockerfile, package versions, configurations and
patches as the aarch64 oracle, once with the host's own configuration and
once pinned (D-050). Each difference class was traced to one cause by an
experiment that removes that cause on x86_64 and reproduces the aarch64
value bit for bit (scripts in `oracle/x86/fp-probe/`).

| Class | Cause | What it touches | Handling |
|---|---|---|---|
| X0 | none | the strict Akantu code itself: `fe`, `material`, `mesh_io`, `mesh_utils`, `selftest`, explicit and layout models, Scotch partitions, every random fixture | identical, files unchanged |
| X1 (Eigen) | Eigen 3.4.0 reads cache sizes with CPUID on x86_64 (64 KiB / 512 KiB / 16 MiB on the test host) and uses 16 KiB / 512 KiB / 512 KiB on aarch64; the depth blocking of GEMM and of the triangular solves in `SparseLU` follows, and so does the summation order | quadratic pentahedra in `model/smm` and `model/smm_fd`, `solver/lu/dense_block`, 9 of 812 GEMM records | removed by the pin: Eigen compiled with `-DEIGEN_NO_CPUID` and the aarch64 defaults; the port hardcodes the same blocking |
| X2 (BLAS) | MUMPS calls OpenBLAS, which selects its kernels from the CPU at run time; no x86 core type reproduces aarch64 | the MUMPS-solved cases: `model/static` (5), `model/implicit_dynamic` (1), `parallel/smm_static_cube_n3` (3 rank records), the MUMPS systems | pinned to `OPENBLAS_CORETYPE=Haswell`, one thread; x86_64 values golden, aarch64 values a platform variant (section 4.2); 40 quantities |
| X3 (NaN sign) | default NaN `0x7ff8…` on aarch64, `0xfff8…` on x86_64 | parser `pow(-8, 1./3.)`, the diverged finite-deformation explicit case from step 49, 87 singular `detinv` records | D-046 (section 1.2) |
| X4 (complex division) | libgcc's `__divdc3` runs Smith's algorithm with fused multiply-adds in Debian's aarch64 build and unfused in the x86_64 build | complex eigenvectors and complex division records: 45 `cdiv` and 56 `eig` records of the dense corpus | the port models both runtimes (`ComplexDivision::{Fused, Unfused}`, section 5.1) |
| X1 (NumPy) | NumPy's transcendental and BLAS-backed kernels depend on the platform, and on x86_64 `np.exp` uses an AVX-512F kernel exactly when the CPU has AVX-512F | four Python example cases whose input NumPy computes alone | environment keys, section 5.27 |
| X5 (Gmsh) | Debian's Gmsh is compiled per architecture; meshing decisions differ | re-freezing on x86_64: 17 of 122 meshes identical, 85 differ in coordinates, 20 in structure | the frozen aarch64 meshes are the inputs everywhere; outputs of upstream tests that mesh at build time are platform-specific |
| X6 (undefined behaviour) | upstream reads uninitialised memory | `diana_3d*` ghost types (UD-094), padded dump values, the anisotropic-damage tangent (UD-031) | never a bitwise golden; compared through the defined part |
| X7, X8 | MPI line interleaving and backtrace depth in text; provenance strings | `dump/text_mpi_*` status files; manifests | not numerical |

The label X1 names two unrelated classes in the repository's documents:
Eigen's cache-size blocking (from the corpus regeneration) and NumPy's
kernels (from the Python example comparisons); the table keeps both with
their qualifier.

Not observed: glibc libm differences. On an AVX2 and FMA host glibc selects
its FMA variants, which reproduce the aarch64 build: every random fixture
and every hash of 2²² inputs of `log`, `exp`, `pow` and `tan` agrees. This
makes x86-64-v3 a requirement of the platform of record: on a host without
FMA, glibc selects other variants, and the oracle probes refuse such a
host. `long double` (80 bits on x86, 128 on aarch64) affects only metadata.

Pinned against unpinned on the same host, exactly the X1 cases (69
quantities in 11 cases) and the X2 cases (41 quantities in 9 cases) change.
Two complete pinned regenerations agree on 666 cases and 13,237 quantities;
three further regenerations agree on 686 cases and 13,298 quantities,
MUMPS-solved and two-rank cases included. A second host (AMD Ryzen 7 9700X)
gives the same `solver` and pentahedron values and the same MUMPS values
under the same core type.

### 4.2 How the fixtures handle it

- Platform variants (D-073). A case whose oracle values depend on the
  numerical environment keeps the x86_64 values at the top level and the
  others under `variants/<environment>/` (`variant.json` with the original
  provenance, its own `data/`; inputs shared and required identical). The
  environment id is `<arch>-<OpenBLAS core type>` (`x86_64-haswell`,
  `aarch64-auto`). The loader picks `AKANTU_FIXTURE_ENV` when set, else the
  platform's oracle environment, and refuses a case with variants on an
  unknown environment (`Error::FixtureEnvironment`). Tests that assert
  recorded oracle numbers select them by `Fixture::environment()` of the
  loaded data, never by `cfg(target_arch)`. Nine cases carry variants (the
  X2 list above); `oracle_fixtures.rs::platform_variants_are_complete_and_verifiable`
  checks them. A last regeneration of the nine on x86_64 gave 119 of 119
  quantities bit-identical to the committed top level.
- Records without a variant (D-121). The oracle records of BLAS-backed
  solvers that exist for x86_64 only (the two-process MUMPS trace
  `solver_model/dbltrace_newton_raphson_np2` and the PETSc records under
  `fixtures/solver/petsc/oracle/`) are compared only in their environment,
  `x86_64-haswell`. In any other environment the scenario runs its other
  checks and prints `ORACLE <scenario>: not compared: <reason>`; the MUMPS
  and PETSc tiers count these lines. The trace case cannot carry a variant
  because its per-iteration matrices and solutions are stored as inputs,
  which a variant shares; an aarch64 oracle run of it (three identical
  runs) equals the port's aarch64 run bit for bit on all 28 displacements
  (not committed).
- Crate-local x86_64 records: `dense_strict_x86_64_delta.txt` (188 records:
  87 X3, 45 + 56 X4; checked by `dense_x86_64_variant.rs`),
  `complex_div_cleanroom_x86_64.txt` and `complex_div_guard_x86_64.txt`.
- Environment keys of the Python example comparisons:
  `python/examples/cases.py` names the platform key
  (`x86_64-linux-glibc`) and `numpy_avx512f` for the cases whose input
  NumPy computes alone (section 5.27).
- The port hardcodes Eigen's default blocking, never a host-dependent one,
  and BLAS-backed tests run under the same OpenBLAS pin as the oracle.

### 4.3 What remains platform-dependent in the port

- The port's default complex division is the aarch64 runtime's
  (`ComplexDivision::Fused`), the one `/`, `Complex::quotient` and every
  `eig` entry point use. On x86_64 the oracle's complex eigenvectors and
  complex quotients are reproduced bit for bit only through
  `ComplexDivision::Unfused` (`Complex::quotient_with`,
  `dense::eig_vectors_complex_with`); with the default, exactly the
  complex-vector fields of those records differ in their last bits. Real
  eigendecompositions and every other dense operation are identical on both
  platforms.
- The MUMPS backend is bitwise to the pinned oracle at one rank and with the
  same OpenBLAS core type only. On aarch64 Linux the nine MUMPS cases are
  compared with their `aarch64-auto` variants and the two-process trace's
  displacements and files are not compared (D-121).
- On aarch64 Linux the PETSc oracle scenarios are not compared (D-121).
  One self-consistency guard of the PETSc tier does not hold there:
  `test_phase_field_optimal_linear_distributed` compares its `pc_type none`
  run on 2 and 4 ranks with the same run on one process under D-082's
  1e-14 bound; x86_64 measures 5.2e-15 and aarch64 1.996e-14 (78 of 2,000
  damages differ, the same value at 2 and 4 ranks and at both thread
  levels), so the check fails on aarch64: a known failure of the optional
  PETSc tier there. The bound was not changed, because no tolerance is
  changed without a derivation.
- A few call sites outside the parser use the host libm (section 5.3 lists
  them); on macOS their results can differ from the oracle in the last bit.
- Two Python example comparisons run bitwise only on a CPU with AVX-512F,
  and two phase-field norms only on the platform of record (section 5.27).

## 5. Results by area

The areas follow the order of the parity matrix. Case counts are directories
with a `manifest.json` under `fixtures/` (`python3
scripts/docs/validation_counts.py fixtures` recounts them); value counts are
those measured when the comparison was accepted, and the area says so where
the corpus has grown since.

### 5.1 Core utilities and dense algebra

`core_probe` (`oracle/probes/core/`) records `SpatialGrid` (a replay of
`test_grid.cc` and point sets in 1, 2 and 3-D), `BBox` (empty, touching,
disjoint and signed-zero boxes, every pair), `Math::reduce`, the scalar
`Math::NewtonRaphson`, `Math::pow`, tolerant comparisons,
`printMemorySize`, string helpers, `CSR`, `CircularArray`, and the text of
the debug macros and of `Array::printself`. Thirteen cases
(`fixtures/core/common`), 25 exact rules; `core_oracle.rs` and
`test_grid.rs` fail when a recorded quantity is not consumed. On the first
eight cases 17,628 values are bit-identical, and their x86_64 regeneration
equals the aarch64 record. Each value is an input read back, a
comparison-based selection or the same short sequence of IEEE operations in
the same order (the `±s` updates of `saveAsMesh`, the sort and pairwise
folding of `Math::reduce`). Signed zeros of box bounds and grid nodes are
compared bitwise; the distributed box keeps upstream's operand on `±0`
ties. Debug output is compared as text after masking the process id, time
stamp, address and C++ source location; no number is masked. The
`printself` texts of `Array` and `CircularArray` match 120 of 120 oracle
texts. Library initialisation (`akantu::initialize`) is replayed from 36
rank records at one and two ranks: seed, the first draws of both global
generators, remaining arguments and debug level, exact; the 30 fixed-input
records were qualified by three oracle runs, the 6 clock-dependent ones are
compared within their recorded clock window, and the `AKA_DEBUG_LEVEL`
override is tested on the Rust side only. Defects: UD-272 (`BBox::empty()`
on inverted `±DBL_MAX` bounds, asserted as a documented difference),
UD-265 to UD-271, UD-273, UD-274, UD-547 to UD-549 (no value change on the
recorded inputs), UD-498, UD-502.

The dense layer is compared from the real upstream headers in the strict
build (`oracle/probes/dense/`, corpora under
`crates/akantu-linalg/tests/fixtures/dense/`):

| Corpus | Content | Comparisons |
|---|---|---|
| `dense_strict.txt` | 2,250 records (521 `eig`): small tensors, det and inverse (fixed 1 to 6, 8; dynamic 1 to 9), Voigt transfers, `eig`/`eigh` (fixed 1 to 6, 8, 9; dynamic 1 to 9, 12) | 5,897, bitwise |
| `dense_gemm_strict.txt` | 812 product records: `computeBtDB`, gradients, `computeNtbN`, structural products, anisotropic material products, all fixed/dynamic combinations of 17 shapes, depths up to 1,000 | 54,909 entries, bitwise, also through the FE entry points |
| `dense_eig_large_strict.txt` | 11 `eig` records up to `n = 48`, complex pairs, tied spectra | 88, bitwise |

`akantu_core::eigen_gebp` translates Eigen 3.4.0's product dispatch and
kernels (MPL-2.0): the route table on compile-time size classes, small GEMM
evaluated coefficient-wise when `depth + rows + cols < 20`, coefficient
reductions ordered by the left operand's compile-time extent, the scalar
`gebp` kernel with even/odd accumulation chains, depth blocking with
Eigen's balanced `kc` from L1 = 16 KiB. Seven mutations of these rules each
fail between 6 and 191 records. The 4-column GEMV block (at least 4,000
rows) has a unit test only.

Scope of the bitwise eigendecomposition claim: `n ≤ 48`, and with
`sort = true` only for `n ≤ 16` or tie-free returned eigenvalues. Beyond it
(blocked LU for `n > 16`, eigenvector accumulation for `n > 48`, sort ties)
the port is checked alone against backward-error criteria; there are no
oracle records at those boundaries, and no upstream call site reaches them
(Voigt stiffness ≤ 6 × 6, 3 × 3 tensors, interpolation matrices ≤ 9 × 9).

Complex division goes through libgcc's `__divdc3`, fused on Debian's
aarch64 and unfused on x86_64. The port reproduces both clean room (D-030):
`ComplexDivision::Fused`, the default of `/`, `quotient` and every `eig`
entry point, and `ComplexDivision::Unfused`, both with the up-scaling guard
at `0x1.fffffffffffffp+970`. The aarch64 records replay with `Fused`, the
x86_64 records (4,500 clean-room records, 162 at the guard, the 101 complex
dense records) with `Unfused`; on 2·10⁷ random and 104,976 grid inputs the
unfused model leaves no mismatch (section 4.3 for the consequence).

The dense probe built with NEON kernels differs from the strict build in
1,070 of 2,250 records, with upstream's default flags in 1,265; only the
strict build is a reproducible target.

### 5.2 Parser

The parser's values (literals, algebraic expressions, vectors, matrices,
random-parameter bases, stream conversions, number printing, strings) are
compared without tolerance: doubles by bit pattern (NaN sign and payload
included, under D-046), strings byte for byte.

| Evidence | Content |
|---|---|
| `akantu-parser/tests/oracle.rs` | C++ oracle dumps of 162 input files (the 153 upstream input files and 9 generated ones), 29,079 conversions |
| `tests/stream_blackbox.rs` | 1,328 inputs × 16 conversions (14 stream extractions and Akantu's `bool` and string-list conversions), 700 doubles × 24 output formats, 264 distribution texts |
| `tests/cli_oracle.rs` | 460 command lines (115 × 4 parse flags), 7 help texts |
| `tests/bytes.rs` | registered string, `ID` and `std::set<ID>` parameters with bytes ≥ 0x80, byte for byte |
| `tests/subregistry_matches_oracle.rs` | `fixtures/parser/subregistry` (2 cases): 45 operations on sub-registries and `Parsable` sub-sections (values, errors, the order of `parseParam` / `parseSubSection` / `updateInternalParameters` calls, printed text) |
| `tests/random_bridge.rs` | parsed random parameters through to the sampler: draws, engine state and printing for 13 input-expressible records, about 22,000 printing cases (special values, subnormals, random bit patterns) |

The parser calls the platform libm, as upstream does, so the comparison is
bitwise on Linux with glibc (the oracle's libm). On macOS the measured
differences are enumerated one by one in `LIBM_EXCEPTIONS` (18 entries:
one-ulp differences of a single call to `acos`, `asin`, `atan`, `atanh`,
`erf`, `erfc` or `lgamma_r` on an argument that is the same double on both
sides); anything else fails, and an entry no longer needed fails too.
Values without a transcendental call are bitwise on both platforms. Other
platforms have no measured list. On x86_64 the parser's corpus agrees with
the aarch64 record apart from the default-NaN sign of `pow(-8, 1./3.)`
(D-046).

Strings: the upstream-compatible conversions (`ByteString` and its
collections) reproduce the oracle's bytes, trailing white space of
`std::string` registry values included. Conversions to Rust `String` fail
on invalid UTF-8 instead of altering bytes; they are an API boundary and
are not compared with upstream values they cannot represent.

Registry printing: `print_registry_bytes` reproduces upstream's
`printself` bytes for every type upstream can register directly and for
random parameters. Not reproduced: the printing of plain `InternalField`
parameters (upstream prints the field's identity and block layout, which are
not registry data) and the field arrays upstream appends at `dblDump`.

Upstream defects: UD-497 (B: upstream lets `set` write a read-only
parameter through a sub-registry; the port refuses by default before
anything is written, `with_upstream_write_access` reproduces upstream and
its replay equals the oracle with no compensation), UD-501 (C: setting an
unknown name through a sub-registry search silently does nothing,
reproduced), UD-055 (`erf`/`erfc` inside vectors rejected by both). Inputs
on which upstream crashes (cyclic references, nesting deeper than its
native stack, `UPSTREAM_STACK_EXHAUSTION`) have no oracle value; the port
returns an error for cycles and the defined value for deep acyclic input
(category A).

### 5.3 Random numbers

`akantu_core::random` reproduces upstream's random numbers bit for bit on
every platform: the engine `minstd_rand0`, `generate_canonical`, the eleven
distributions Akantu wraps (libstdc++'s algorithms, reproduced clean room),
`RandomParameter`, the global generators, and the fill order of
`RandomInternalField`. The distributions sample through glibc's `log`,
`exp`, `pow` and `tan`, which the port carries as pure-Rust translations of
the oracle's glibc 2.41 aarch64 binary (fused multiply-adds exactly where
`libm.so.6` has them). Evidence (`akantu-core/tests/random_*.rs`, fixtures
`tests/fixtures/random/*.json`, probes `oracle/probes/random/`), all exact:

- engine and canonical sequences for 22 seeds (0, negative, 2³¹,
  `LONG_MIN` among them), 1,000 values each, with engine call counts;
- 11 distributions × 2 to 6 parameter sets × 3 seeds × 1,000 values,
  engine call counts, the printed state after drawing, copy and interleave
  semantics, clean-room edge records;
- `RandomParameter` chunks on explicit and global engines and their
  printing;
- the libm translations: hashes over 14 domains × 2²² inputs (long run
  behind `AKANTU_RNG_LIBM_LONG=1`), the 58 × 58 special-value `pow`
  product, 1.4·10⁹ inputs against the host glibc on Linux aarch64.

Measured on macOS arm64, Linux aarch64 and Linux x86_64. GCC's default
contraction would change about 26 % of the canonical draws by one ulp; the
target is the strict build. On an x86_64 host with AVX2 and FMA glibc
selects its FMA variants, which equal the aarch64 build on every recorded
input; on a host without FMA they would not (section 4.1). The
special-value tables compare NaN results as a class (section 3.4).

`akantu_core::glibc_libm` also carries `sin`, `cos` and `sincos` (GCC
compiles upstream's `sin(φ)`/`cos(φ)` pair in
`MaterialDruckerPrager::updateInternalParameters` into one `sincos` call),
checked by FNV hashes over 7 domains × 2²⁰ inputs covering every branch and
an 8,999-angle sweep (`glibc_libm_sincos.rs`, fixture `libm_sincos.json`);
the host libm differs from glibc's `sincos` on 699 of those angles. Each
function states its domain and returns `None` outside it (`tan` beyond
`|x| ≤ 25`, `sin`/`cos`/`sincos` from `|x| ≥ 105414350`). Code compared
bitwise with the oracle calls this module, with these exceptions, which call
the host libm: the parser (section 5.2); `powf` in the cohesive linear law's
volume scaling of the insertion strength
(`crates/akantu-cohesive/src/laws/linear.rs`, two sites, upstream
`std::pow`); `powf(3.0)` in the contact penalty's slip moduli
(`crates/akantu-contact/src/resolution.rs`, upstream `pow(traction_norm,
3)`); `atan2`, `atan` and `tan` in the anisotropic damage law
(`crates/akantu-solid/src/materials/anisotropic_damage.rs`; `tan` uses this
module for `|x| ≤ 25` and the host beyond). On glibc platforms these calls
give the oracle's values, since the oracle uses the same libm. On macOS they
can differ from the oracle in the last bit; the macOS test runs showed no
such difference in the cases compared (`docs/PLATFORMS.md`), and no
exception list exists for them.

Random internal fields of constitutive laws are replayed inside a
`SolidMechanicsModel` (`fixtures/cl/random_internal`, 2 cases,
`law_random_probe.cc`, `random_internal_matches_oracle.rs`): uniform,
Weibull and stateful normal fields with one and two components, history and
a cohesive-kind field, through initialisation, ghost additions, default
values, element events with renumbering and erasures. `smm_2d`: 516 values
and the next 4 engine values bit for bit over 7 steps, 168 blocks of layout
exact. `smm_2d_mixed`: one interleaved addition that grows both ghost types
in one resize, 72 values and the next 4 draws bit for bit; a mutant that
reverses the ghost-type order passes `smm_2d` and fails this case. One
extra, missing or reordered draw changes every later value, so bit equality
plus equal next engine values proves the draw order. Upstream defects:
UD-106 (upstream keeps the `previous_` history at the pre-removal size until
the next resize, the port compacts it; asserted), UD-500 (zero-size blocks
of a field whose kind changed; the port has none). Random fields at several
ranks: section 5.24.

### 5.4 Finite-element layer

Element formulas are transcribed term by term, and products follow Eigen's
selection on the compile-time operand sizes of each upstream call site
(`akantu_fe::eigen::Dims`, evaluated by `eigen_gebp`). Where upstream
computes the same quantity in two orders at two call sites (`IntegratorGauss`
builds `J` from a fixed `dnds`, `ShapeLagrange` from dynamic slices), the
port mirrors each site.

| Fixtures (probe) | Content | Values |
|---|---|---|
| `fe/element_class` (23 types: 13 regular, 7 cohesive, 3 structural; `element_probe`) | Gauss points and weights of every supported order, shapes, `dN/dξ`, `d²N/dξ²` where upstream implements it, facets, natural coordinates; on affine, curved and embedded elements `J`, `det J`, `dN/dx`, quadrature coordinates, normals, monomial integrals, inradius, inverse map, `contains` | 1,035 arrays + 38 `d2nds2` arrays |
| `fe/engine` (12 types), `fe/engine_cohesive` (7; `fe_engine_probe`) | integration points, shapes, derivatives, `det J · w`, interpolation, gradient and integration (full and filtered), `BᵀD`, `BᵀDB`, `Nᵀb`, `NᵀbN`, inverse map, lumped mass, facet normals; cohesive mid-surface quantities | 388,670 |
| `fe/engine_extra` (12 types with ghost copies; `fe_extra_probe`) | elemental consistent matrices and lumped arrays captured at the DOF-manager calls, `integrate<type, n>` for n = 1…5, elemental-field interpolation | 140,407 |
| `fe/engine_real` (12 types) | shapes, derivatives and interpolation at real coordinates | 26,684 |
| `fe/structural` (8 cases; `structural_probe`) | below | 32,917 |

All rules exact, all values bitwise. The FE replay compares what the FE
engine passes to the DOF manager; the scatter into global arrays is
compared in section 5.8. Independent checks (`oracle_fixtures.rs`,
`fe_engine_properties.rs`): exact monomial integration (more than 500
moments), partition of unity, `Σ ∂N/∂s = 0`, affine identities, inverse-map
round trips, unit normals, iso-parametric reproduction of linear fields,
patch equilibrium, consistent against lumped mass. Ported upstream tests
keep their tolerances (`test_fe_engine_gauss_integration` 5e-14,
`test_fe_engine_precomputation` 1e-10, `test_fe_engine_inradius` 1e-6,
`test_gradient`, `test_integrate`, `test_interpolate`, `test_inverse_map`).
`computeD2NDS2` exists upstream for three interpolations only and
elemental-field interpolation for 8 types; the port returns `NotImplemented`
for the same types (UD-096). UD-043 and UD-097 to UD-101 have regression
tests; the unused pentahedron quadrature tables are not ported (QUAD-018).

#### Structural element kernels

The Hermite, Bernoulli-beam and DKT formulas follow each call site's mix
of fixed and dynamic operands (the beam length is computed separately at
three sites because they can differ in the last bit; `N·T` and `B·T` of the
3-D beam and DKT reach `gebp`). Eight cases on upstream's test meshes and
generated inclined, oriented and distorted elements (`t = 0.05…0.17`): 152
quantity comparisons, 32,917 values bitwise on aarch64 and x86_64
(rotations, `N·T`, `B·T`, strains, `BᵀDB`, stiffness, stresses, internal
forces, line loads, consistent mass); the structural element-class records
too (330 values). Probe-defined quantities: `field/interpolated` and
`field/strain` (upstream reads element values with a stride of `nq`
elements), `ntbn` (upstream's `computeNtbN` is `AKANTU_TO_IMPLEMENT`), the
element tangent and stress; DKT `N·T` is not recorded (uninitialised
upstream, UD-053). Defects: UD-293 (B: the θy row of upstream's 3-D beam
`N` is `+dw/ds`; corrected by default, the replays use `UPSTREAM` mode and
check that the two engines differ only by the exact negation of that row),
UD-007 (B: DKT bending, identical at `t = 1`; section 5.16), UD-296
(degenerate elements refused by exact criteria, valid inputs unchanged),
UD-343 (beam normals off by more than 1e-10 from unit or orthogonal are
refused; input classification, off in `UPSTREAM` mode). Independent checks
(`structural_properties.rs`): Hermite reproduction of cubic fields,
rigid-body null spaces of dimension 3, 6 and 9 (DKT: 6 rigid modes and 3
drilling DOFs without stiffness, as upstream), patch tests, the closed-form
inclined-beam stiffness, frame indifference, total mass `ρAL`, and
upstream's structural precomputation and displacement tests on a dense
assembly of the port's element matrices.

### 5.5 Meshes and mesh utilities

`oracle/probes/mesh_utils/mesh_utils_probe.cc` records facet meshes
(`initMeshFacets`, `buildAllFacets`), in-place boundary facets
(`buildFacets`), the readers' element and sub-element data, node-to-element
CSR and sets, clusters and boundary groups, groups from mesh data,
`purifyMesh`, the renumbering of `eraseElements`, `makePeriodic` and the
local part of `makeConsistentForPeriodicity`, on upstream's test meshes,
frozen Gmsh meshes and generated cohesive and mixed meshes: 55 cases
(`fixtures/mesh_utils`), one exact rule, 1,791 quantities and 329,619 values
(the reals are copied coordinates and periodic sums), all
identical (`mesh_utils_oracle.rs`): facet connectivities with their
orientation, adjacency content and slot order, facet names and groups, CSR
rows in visiting order, cluster and group order, renumberings, periodic
pairs and sums. Upstream's default-constructed `Element()` left for
unmatched elements is compared as the null element (UD-128).

Periodic sums are bitwise for any data: upstream iterates an
`unordered_multimap` whose equal-key order with the oracle's g++ 14.2 is
reverse insertion order, established black box over 178 container patterns
(UD-654, D-030), and the port reproduces it. 67 adversarial oracle cases
with up to seven slaves per master (`mesh_utils/periodic_order`, 42,465
values) are bitwise; insertion or sorted order would change 1,624 or 1,164
of them. Another standard library may iterate equal keys differently.

Ported tests: `test_buildfacets` (output equal line for line to the
confirmed `.verified`), `test_purify_mesh`, `test_mesh_iterators`,
`test_mesh_periodic` (serial part), `test_facet_element_mapping`,
`test_mesh_boundary`, `test_mesh_data`. Not ported: `test_segment_nodetype`
of an API removed in 2018 and the legacy periodic maps (approved
exceptions). Independent checks on every input: facets on one or two
elements, adjacency both ways, Euler characteristics of the facet complex
and its boundary, CSR multiplicities, removal invariants under random
subsets, periodic pairing symmetry, journaled removals that roll back.

Upstream leaves facet meshes and attached state stale after a removal
(UD-117), so the port's corrected removal (UD-117, UD-103) is checked
analytically and only the renumbering of a fresh mesh against the oracle.
Other deviations: UD-118 to UD-127, UD-129, UD-135, UD-136 (corrected),
UD-121 (upstream's fuzzy sort in `makePeriodic` is not a strict weak
ordering; same pairing on every case), UD-133, UD-134 (preserved), UD-131
(structural facet building aborts upstream; nothing compared).

The mesh container (`oracle/probes/mesh2`, 46 cases in `fixtures/mesh2`,
exact) is compared through queries over dimensions, ghost types and kinds,
type maps, filtered views, group dumps (VTU and PVTU bytes), Graphviz output
and Scotch reordering and weighted partitions (the 19 Scotch cases need the
`scotch` feature); 22 cases are adversarial. Upstream's `nb_element/all/*`
fails an assertion (UD-669) and the port's count is checked against the
oracle's type list; UD-672 and UD-673 are defined behaviour without oracle
value. Distribution and synchronizers: section 5.23.

### 5.6 I/O and dumpers

Mesh readers and writers: `mesh_probe` records every mesh upstream commits
and the 121 frozen Gmsh meshes (nodes, bounds, connectivities per type and
ghost type, `tag_*` and `physical_names` data, node and element groups): 226
cases (`fixtures/mesh_io`). `mesh_io_probe` adds 28 cases
(`fixtures/io/mesh_io`): `Mesh::write` with the written text, data blocks,
MSH 4 physical semantics, MSH 1.0, binary headers, Diana meshes with their
material file, expected-error reads. Formats: Gmsh MSH 1, 2, 4.0 and 4.1,
Diana. One exact rule (decimal conversion is correctly rounded on both
sides). Every case equals the oracle except: UD-064 (A: upstream's Gmsh 2
reader shifts `tag_j` data and leaves its tail uninitialised; six arrays of
two meshes are excluded and the port defines them), UD-045 (B: empty
`msh_block_*` groups; filled by default, `fill_unnamed_physical_groups =
false` for the comparison), UD-047 (B: `$ElementNodeData` read partially
upstream), UD-093 (upstream's MSH writer and reader disagree on quadratic
node order), UD-094 (A: Diana group members' ghost type uninitialised
upstream; only the defined part is compared).

Dumpers write files, so the comparison is byte equality, with only the
deterministic entries where upstream writes uninitialised memory and no
numerical tolerance. Reference: `fixtures/dump/` (16 directories with their
own `SHA256SUMS`, probe `oracle/probes/dump`), compared by `dump_oracle.rs`,
`dump_smm.rs`, `test_dumper.rs`: VTU, PVTU and PVD in base64 and ASCII, the
text dumper, IOHelper's tests, LAMMPS, group dumpers, 21 element types; an
independent reader (`oracle/probes/dump/check.sh`) reads every file of both
sides. Upstream's ParaView layout defects (matrix fields padded to squares,
UD-110; inconsistent tuple lengths, UD-113; invalid base64 for arrays under
two bytes, UD-114) are corrected by default, and
`DumperParaview::set_compat_layout(true)` writes upstream's bytes for every
comparison. Upstream's padded 2-D stress holds uninitialised entries
(UD-111): only its 459 deterministic entries are compared
(`fixtures/dump/smm2d/NONDETERMINISTIC`, `model/smm2/dump_padded_2d`). A
padded 2-D strain cannot be dumped upstream (the registration aborts,
UD-112); the port's is compared with a recomputation from the oracle's
`grad_u` (54 entries). Not ported: the legacy restart writer and reader, the C
facades and the base64 helper (approved exceptions). Model-level dumps are
compared in the model sections.

ParaView check: every C++ and Python example that writes ParaView files ran
with both codes (52 runs) and all 16,090 files of the port and 16,086 of
upstream were opened in ParaView 5.13.3 and compared byte for byte, the
differing ones array by array (`scripts/paraview/vtu_diff.py`,
`docs/reports/2026-11-12-paraview-visual-check.md`). ParaView opened every
file of the port without a message; 12,772 files are byte-identical and
every other difference has a register entry (UD-010, UD-029, UD-110,
UD-111, UD-114, UD-217, UD-363, UD-365, UD-473, UD-474, UD-850, UD-980,
UD-1270; UD-851 for `embedded`). The check shows plausibility only: three dumps
drawn per run, one platform, multi-process examples run in-process,
upstream run once per case.

### 5.7 Sparse algebra and direct solvers

`oracle/probes/solver/solver_probe.cc` records 15 cases (`fixtures/solver`):
AIJ assembly on upstream's `bar.msh` and a frozen triangle mesh in
symmetric and unsymmetric storage (values, profile, releases), Matrix
Market text, products, `J = copyProfile(M) + 0.3 K + 2.5 M`, preassembled
terms, `applyBoundary`, and the Eigen `SparseLU` solution with, through a
subclass that exposes its protected state, the permutations, elimination
tree, supernodes and every factor array (on the `triangle` cases only
permutations, tree, nonzero counts, supernode count and solution); eight
`lu/*` systems (upstream's test matrix, weak diagonal, 150 dense columns with
128-column supernodes, arrow, unsymmetric convection-diffusion, explicit
zeros and duplicates, structurally and numerically singular); 30 special
values printed at precision 15. One exact rule; everything bitwise or
byte-identical (`aij_oracle.rs`, `matrix_market_oracle.rs`,
`eigen_sparse_lu_oracle.rs`: 12 factorisations and 2 singular refusals with
upstream's message).

`akantu_linalg::eigen_sparse_lu` translates Eigen 3.4.0's `SparseLU`
statement by statement (MPL-2.0): COLAMD with 32-bit arithmetic, the
column elimination tree, relaxed supernodes, panels of 16, threshold
partial pivoting, pruning and the strict-build dense kernels (triangular
solves in panels of 8, `gebp` panel updates with `KcFactor = 4`,
`sparselu_gemm` with `RK = 2`; the vectorised values 16 and 4 match every
integer array and miss about 10 % of `lusup` by one ulp). The vectorised
`default` build is not claimed. Upstream's `eigen` solver maps to this
translation, and `auto` resolves as upstream (MUMPS when registered, else
`eigen`; D-040). Matrix Market text is compared in upstream's 15-digit mode
(UD-061); the port's default writes 17 digits and round-trips exactly.

faer is a qualified extension (`SparseSolverType::Faer`), never chosen by
`auto`. Qualification (`faer_qualification.rs`, `faer_histories.rs`):
LAPACK's `dget02` backward error below 30 (largest 0.604), agreement with the
oracle's solution within a finite-perturbation bound (two to three orders
inside; `κ∞` estimated), singular matrices refused as by the oracle, and
every Newton case of `fixtures/solver_model` replayed with identical
iteration counts and decisions over 2,412 iterations (states within 4.9e-15).
These are 1-D bar systems (`κ₂ ≤ ~4·10³`); faer is not qualified against
the oracle on 2-D or 3-D implicit histories, where it is compared with the
serial Eigen solve only (section 5.24).

Symmetric and unsymmetric storage of symmetric element matrices give the
same operator bit for bit; invalid input returns an error and leaves the
matrix unchanged. Defects: UD-177 (B, a symmetric matrix added into an
unsymmetric one adds only the stored triangle), UD-178 (B, `info()` never
checked), UD-179 (B, copy loses the lookup map), UD-180 (B, multi-rank
`saveProfile` truncates one file on every rank; no parallel parity
claimed), UD-183 (B, non-finite systems reported as solved), UD-181 and
UD-182 (upstream tests that assemble nothing or are not built).

### 5.8 DOF manager, non-linear solvers and time integration

`oracle/probes/solver_model/solver_model_probe.cc` includes upstream's toy
model `test_model_solver_my_model.hh` unmodified (a 1-D bar) and records,
with the Eigen solver selected: upstream's `test_model_solver` (static,
explicit and implicit, 400 steps), `test_dof_manager_default`,
`test_dof_manager`; every integration scheme and solution type (central
difference, trapezoidal rules 1 and 2, Fox-Goodwin, linear acceleration,
Newmark-β, forward and backward Euler, generalised trapezoidal), damped
variants, two solvers sharing one DOF manager, new-node events, a cubic bar
under Newton-Raphson (solution, residual and `residual_mass_wgh` criteria,
modified, contact type, linear solver, non-convergence with restore), the
`dblTrace` dumps of a traced solve. Per step: energies, Newton counts and
errors, a digest of `u‖u̇‖ü`, matrices with releases, callback traces,
printed tables. 38 cases (`fixtures/solver_model`, one of them a two-process
case), one exact rule, every value bitwise: the 400-step explicit and
implicit runs (250,641 and 252,043 values, tables equal to upstream's
`.verified`), 16 scheme cases (147,630), damped variants (42,178), solver
variants, DOF-manager assemblies (39,849), the shared-`J` sequence including
upstream's stale-`J` failures, the trace files byte for byte at upstream's
15-digit precision, and the two-process trace per rank.

The replays run with this area's compat switches on. Corrected defaults:
UD-010 (B, a residual criterion met at the first test skips
`afterSolveStep`), UD-194 (B, `residual_mass_wgh` never refreshes the
residual: refused), UD-195 (B, forward Euler declares `M` and reads `K`),
UD-193 (B, solvers sharing `J` do not notice each other's changes), UD-012
(B, `hasDOFsDerivatives` off by one); reproduced and documented: UD-202 (C,
the `increment` of displacement-type solutions is the last Newton
correction), UD-196, UD-199, UD-201; defined behaviour: UD-013, UD-197,
UD-198, UD-200.

Independent checks (`integration_scheme_checks.rs`): measured orders on
`ü + u = 0` of 2.000 (trapezoidal rule 2, linear acceleration, central
difference) and 4.000 (Fox-Goodwin), printed by the test, which asserts at
least 1.95 and 3.9; trapezoidal energy drift at most
5.7e-15 over 500 steps; stability limits of central difference and forward
Euler at `ωΔt` or `λΔt` = 1.9 against 2.1; static Newton equal to a direct
solve (3.4e-13). New nodes, where upstream reads out of bounds (UD-653): the
extended bar against its exact solution within `1e-10|u|`. The translated
python_fe `DynamicSolver` (TEST-232) on TEST-074's oracle snapshots differs
by 5.336e-2 as written because the script's own Dirichlet treatment differs
(UD-1082), and agrees to 4.222e-15 with Akantu's; the as-written values are
asserted against upstream's unchanged script within 1e-6 (rule PFE-1: the
reference carries 7 digits).

Distributed solves on the local world (17-node bar in 1, 2 and 4 contiguous
parts): static, linear and explicit runs bitwise to serial (numbering and
assembly-order identity, since contiguous parts keep the serial
numbering); implicit trapezoidal runs within `1e-13 · max|field|` for 5
steps, scaled with the step count (measured 2.7e-16 and, over 400 steps,
2.4e-15), because the corrected distributed product (UD-003) sums each row
and then the rank partials in rank order. Upstream's own distributed product
reduces a zero temporary (UD-003), so this toy has no multi-rank oracle.
Every solver runs as one collective: a failure on one rank fails every rank
promptly. After a failed `solve_step` the state is that of the agreed
failure point; only `NotConverged` restores, as upstream.

### 5.9 Solid mechanics model

The oracle's default sparse solver is MUMPS, so the solid probes select
upstream's Eigen solver; the earliest model cases, solved by MUMPS, are
kept as cross-solver evidence.

| Fixtures (probe) | Cases | Content | Result |
|---|---|---|---|
| `model/smm` (`oracle/probes/smm/smm_probe.cc`) | 128 | upstream's patch-test procedure on the 12 patch meshes (static, explicit and implicit; elastic, anisotropic and small-strain finite-deformation laws; plane strain and stress; 100 dynamic steps with a state digest per step; `AnisotropicStatic`), lumped masses and stable time steps, eigenstrain, material reassignment, consistent-mass dynamics with four energies, boundary-condition functors, periodic static cases, adversarial mixed meshes, Eigen-pinned twins of the MUMPS loaded cases | bitwise (more than 2.1 M values at acceptance, digests counted per value) |
| `model/smm_fd` (`smm_fd_probe.cc`) | 82 | total-Lagrangian finite deformation: static, explicit and implicit patch procedures, clamped bars with non-affine fields, loaded static cases with displacements up to 0.5 on the unit cube, eigenstrain stiffness, upstream's large-rotation and finite-deformation tests | bitwise (1,620,091 non-NaN values) |
| `model/smm2`, `model/v87` (`smm2_probe.cc`) | 18, 4 | stress interpolation on facets and elements, the model dumper and two extra dumpers (255,703 bytes), energies per step, group and element, a user law through the factory, the padded 2-D stress dump, adversarial interpolation cases | bitwise and byte-identical |
| `model/smm_init_solver`, `model/explicit`, `model/layout`, `model/upstream_tests_2b` | 1, 2, 2, 3 | call orderings around `initNewSolver`, explicit patch runs, tensor-layout sentinels, ported tests | bitwise |
| `model/static` (`model_probe.py`, MUMPS) | 6 | static patch tests of 4 types, 2 loaded cases | `K`, `f_ext`, iteration counts and everything recomputed from the oracle's `u` bitwise; solved fields class X |
| `model/implicit_dynamic` (MUMPS) | 1 | plastic patch `_triangle_6`, 5 steps, 14 to 21 Newton iterations each | consistent mass and iteration counts exact; states class X |

All rules are exact except the cross-solver ones (class X): static patch
rel 1e-11 and loaded rel 1e-10 (worst measured 8.5e-16 and 2.9e-15 of the
maximum; bound `cond2(K_ff)(η_oracle + η_port)` with condition numbers
1.06 to 312.4 recorded by the probe), the implicit-dynamic patch rel 1e-10
per step (`cond2(J_ff) = 28.5`; measured at most 2.9e-14) with
`newton/error` as a difference diagnostic of 1e-7 N absolute (final
residuals differ by at most 4.8e-10 N). The loaded cases have Eigen-pinned
bitwise twins and the patch types bitwise Eigen patch tests in `model/smm`.
These seven cases carry x86_64 and aarch64 values (section 4.2).
Upstream's Newton with linear isotropic hardening converges linearly
(contraction 0.28 to 0.34) because its tangent is the elastic one; the port
reproduces it with exact iteration counts (UD-079, C).

Checks beside the oracle: upstream's patch-test assertions on every
replayed case (`checkDOFs` 1e-10, `checkGradient` 1e-8, `checkResults`
1e-9, `checkForces` 1e-8, `E_kin` 1e-16 / 1e-13); ported dynamics waves
(error below 2e-3), energy closed forms, lumped and consistent mass totals,
the analytical stable time step, energy drift; for finite deformation, the
tangent against central differences of the internal force for 12 types
(at most 1.2e-10, bound 1e-8), objectivity under rotation (3.8e-14, bound
1e-12), stress-free rigid rotation and the small-strain limit.

Spatial convergence (`spatial_convergence.rs`, no oracle), rule CONV-1: a
smooth non-polynomial manufactured field on unit cubes mapped by a fixed
smooth distortion (quadrangles and hexahedra are not parallelograms,
second-order elements are curved), Dirichlet data on the whole boundary,
the consistent load of `−div σ(u)`. The stiffness is the library's; loads,
errors and derivatives use the test's own quadrature (exact to degree 7)
and differences of the library's `N`, so a defect of a library rule or of
`dN/dξ` changes the orders. Observed orders of the finest pair (expected L2
`p + 1`, energy `p`):

| Type | L2 | energy | Type | L2 | energy |
|---|---|---|---|---|---|
| `_segment_2` | 2.000 | 1.000 | `_tetrahedron_4` | 1.964 | 0.991 |
| `_segment_3` | 2.998 | 1.998 | `_tetrahedron_10` | 2.971 | 1.935 |
| `_triangle_3` | 1.999 | 1.000 | `_pentahedron_6` | 1.990 | 0.998 |
| `_triangle_6` | 2.998 | 1.997 | `_pentahedron_15` | 3.013 | 2.002 |
| `_quadrangle_4` | 1.999 | 1.000 | `_hexahedron_8` | 1.997 | 1.000 |
| `_quadrangle_8` | 3.002 | 2.000 | `_hexahedron_20` | 3.074 | 2.052 |

The finest 3-D levels run in the optimised build of the merge gate; the
default profile is a sanity check for 3-D. Plane stress gives the same
orders. An explicit standing P-wave at a fixed CFL number of 0.387
converges at L2 order 1.998; started from `a₀ = 0` instead of the
consistent acceleration (as upstream integrates) it becomes first order in
time, which a negative control asserts. A wrong triangle quadrature weight
and two swapped `dN/dξ` of `_quadrangle_8` (which pass the linear patch
test) are both detected.

Upstream defects that change the comparison:

- UD-208 (B): upstream evaluates the stiffness at `∇u` instead of
  `∇u − ε⁰` (also in the `L2` part of finite deformation); the default is
  derivative-consistent (9.6e-12 against finite differences, compat 0.20)
  and `stiffness_grad_u_ignores_eigen_grad_u` reproduces upstream. With
  modified Newton converging at its first test, upstream's state ratchets
  from step to step; the default does not move.
- UD-352 (B): upstream's `initSolver` resets the cached current position
  to `X` without changing its release; the port keeps the cache and
  `init_solver_resets_current_position` reproduces upstream. The nine
  orderings of `smm_init_solver/orderings` are bitwise in both modes where
  they agree; the two UD-352 orderings equal the oracle in compat mode.
- UD-033, UD-104: compat switches on for the anisotropic patch cases only.
- UD-210 (C): upstream's finite-deformation patch tests never run finite
  deformation; `model/smm_fd` sets it. Its explicit `_hexahedron_20` patch
  at upstream's time step is unstable for the finite-deformation tangent
  (`ω_max Δt = 2.204`); both codes overflow to NaN from step 49, and the
  finite values and NaN positions are compared (D-046).
- UD-684 (the upstream test law's `μ ε`, reproduced; `local_material_live`
  compares its stresses, 61,924 values), UD-685 (a stale re-initialised
  interpolation, checked against the exact answer within 1e-12), UD-111 and
  UD-112 (padded 2-D dumps, section 5.6), UD-078, UD-682,
  UD-683.

Node removal under a live model has no oracle (`onNodesRemoved` is empty
upstream, UD-209). The port renumbers the DOFs as a fresh registration
would; the criterion is that a removal between two solves equals a fresh
model on the final mesh with the state transferred by the test's own
renumbering. Bitwise for static, explicit and implicit-dynamic solid runs
(serial, and a node shared by every rank at 1, 2 and 4 ranks), an
IGFEM-shaped update, a cohesive run after `purify` and the phase field,
because the erased elements are the last of their blocks; a removal in the
middle of a block is checked against the patch solution (1e-13). Ten
mutants are killed. Refused, recoverably: compositions of node events the
DOF manager cannot read from the final mesh, distributed removals on
periodic meshes or with a facet mesh, removals inconsistent between ranks
(UD-1070, UD-1072), removal through the IGFEM handler and the diffusion
model.

Every model callback and reduction runs as an agreed collective; an
injected failure on one rank fails every rank promptly. Distributed solid
mechanics: section 5.24.

### 5.10 Materials

Constitutive laws are compared at a material point. `material_probe`
(`oracle/probes/cpp/material_probe.cc`, catalogue
`oracle/probes/python/material_cases.py`) creates each law from the case's
`material.dat` on a one-element mesh and records a prescribed strain
history (uniaxial, shear, hydrostatic, composed, cyclic, rotation; ramp,
hold and unload with varying time steps for the viscous laws): gradient,
stress, tangent, energies before and after `afterSolveStep`, wave speeds,
every internal, post-step internals (`damage dissipated energy`, `integral
of sigma`) and repeated evaluations per step. The Rust tests
(`material_fixtures*.rs`, `inelastic_material_fixtures.rs`,
`neohookean_kernel_fixtures.rs`) create the law the same way, set its
options before `initMaterial`, feed the probe's own input gradient through
the bitwise FE engine (the recorded `grad_u` is the one the law stores after
the step, compared as well) and assert identity of every value. 229 cases
(`fixtures/material`), exact rules per law directory.

| Family | Cases | Values, upstream mode | Corrected defaults compared |
|---|---|---|---|
| elastic, thermal, St-Venant-Kirchhoff, anisotropic, orthotropic | 30 | 16,980 | 11,560 |
| `marigo`, `mazars` | 32, 28 | 12,560, 10,060 | 11,400, 8,900 |
| J2 (with the legacy finite branch), Drucker-Prager, `plastic_mazars` | 38, 20, 22 | 14,120, 9,600, 10,060 | 6,760, 4,300, 6,640 |
| `sls_deviatoric`, `viscoelastic_maxwell` (with thermal) | 10, 10 | 8,863, 13,398 | 7,664, 2,856 |
| `phasefield_no_split`, `phasefield_volumetric_deviatoric` | 10, 10 | 6,440 each | 9,960 together |
| `anisotropic_damage_mazars` (unpatched, patched variant) | 10 | 920 + 5,540 | none (refused) |
| `neohookean` (error path), `neohookean_kernel` | 2, 7 | 3,020 | the corrected law |

Counts are those at acceptance; post-step records were added later. The
damage and plasticity laws also have 70 x86_64 twins with post-step
records, bitwise in both modes; their 623 quantities shared with the
aarch64 goldens are identical. The order of operations follows upstream per
coefficient (halving trees where Eigen unrolls, sequential sums where it
does not, as for the 9-term 3-D Maxwell work against the 4-term 2-D one);
`exp`, `log`, `pow` and `sincos` go through the glibc translations.

Defects (`LawOptions::default()` corrects, `LawOptions::upstream()`
reproduces and is the mode of the replays): UD-038 (thermal strain
`(1+ν)αΔT`, stale `σ_th` in the damage laws, wrong thermal increment in J2
and Drucker-Prager), UD-109 (thermal energy `½σ:∇u` instead of the stored
elastic energy), UD-033 (celerity from `eig(C)[0]` instead of the Kelvin
bound), UD-104 (material axes rotated with `Rᵀ`), UD-105 (orthotropic parent
update skipped), UD-137 (truncated J2 return in 1-D and 2-D), UD-138 (trial
state from the current instead of the committed state), UD-139 to UD-141
(`plastic_mazars` history, Mazars thermal and out-of-plane strain), UD-144 to
UD-146 (Drucker-Prager return; upstream's `2d_shear` stresses lie outside
the yield surface in 10 of 20 steps), UD-029, UD-063, UD-157, UD-161,
UD-328, UD-329 (viscoelastic laws), UD-164, UD-165, UD-387 (phase-field
tangent and energy split), UD-026, UD-057 (Neo-Hookean stress
`NotImplemented` upstream). Refused by default, compat mode compared: UD-030
(finite-deformation J2; the legacy branch is replayed bitwise, including two
cases on the decision boundary of its small-strain pass that six reviewer
mutants fail), UD-142 (finite-deformation damage), UD-327 (thermal
Maxwell, compat path compared: 3,780 values), UD-031 / UD-058 (anisotropic
damage).

Anisotropic damage: upstream reads uninitialised memory (UD-031, and a
failed eigensolve returned without checking `info()`, UD-814), so identical
oracle runs differ. The unpatched cases record only quantities that do not
depend on that memory (920 values, damage excluded, sign-strict). A
patched variant that defines both reads (valgrind-clean on all 13 cases,
D-060) gives 7 cases compared bitwise including the damage and default NaNs.
The port's default refuses the law; its `Corrected` model (UD-058) is an
opt-in without oracle, checked against its own properties.

Neo-Hookean: upstream's stress aborts, so the probe registers
`neohookean_kernel`, upstream's class without the aborting parent calls,
and the port's kernels are compared with it (PK2, tangent, energy kernel,
upstream's plane-stress `C33` iteration bit for bit). The port solves
`S33 = 0` to machine precision where upstream stops at `|f| ≤ 1e-5`.

Independent checks (bounds derived per test): closed forms within 64 u,
frame covariance, energies as potentials of the stress by finite
differences, acoustic speeds below the Kelvin bound, J2 consistency (1e-11),
Drucker-Prager yield residual, secant damage unloading with constant
dissipation, Maxwell and SLS relaxation and creep with second-order
convergence, energy balance `W = ψ + D`, phase-field split identities
(1e-12). Ported upstream tests keep their tolerances
(`test_damage_materials` against `py_mazars.py`, `test_plastic_materials`,
`test_elastic_materials`, the SLS and Maxwell relaxation tests; upstream's
Maxwell test is marked unstable and its mode is off by 0.33, UD-029).

Not compared: the corrected defaults where they differ from upstream
(analytical evidence only); the implicit-dynamic plastic patch has no solver
acceptance on the port's residual history (the solver exposes no
per-iteration norms there; iteration counts are exact); no oracle model
case drives an eigenstrain with a strain-dependent tangent or a mixed
plastic and damage law through the solid model.

### 5.11 Non-local framework and laws

The default summation route (`SummationOrder::UpstreamHalfPairs`) follows
upstream: grid pair order, the two weights accumulated into the volumes in
pair order, normalisation, then `a += w·v` per component (owned pairs, then
ghost pairs). Oracle cases: 33 (`fixtures/model/non_local`, probes
`oracle/probes/non_local/`, pinned x86_64 oracle), one exact rule, all
bitwise (`non_local_fixtures.rs`, `non_local_adversarial.rs`): manager pairs,
weights, volumes and Jacobians; averages of two averaging calls (the second
after a heterogeneous damage field) for the base, removal and damage weight
functions on several meshes; `mazars_non_local` (10,556 values per
variant), `plastic_mazars_non_local` (14,140) and upstream's
`MaterialMarigoNonLocal` (10,556) over 14 steps; an update-rate sweep; the
band cases below.

- UD-557 (B): upstream's `damage_wf` exponent converts a double to `Int`
  and is undefined when it leaves the range, when `ln(1 − x)` rounds to
  `+0` and when `1 − x < 0`; on x86_64 this gives `INT_MIN`, infinite
  weights and NaN averages. The default keeps the exponent in `f64` (equal
  to upstream wherever upstream is defined) and replaces the
  rounded-to-zero logarithm by `−x`, its correctly rounded value; every
  weight is then finite and in `[0, 1]`. `UpstreamX86_64` replays four band
  cases bitwise, infinities and default NaNs included. The aarch64 oracle's
  saturating conversion was not measured.
- UD-558 (B): two neighbourhoods registering the same variable share one
  entry upstream, so one law averages the other's variable. The default
  gives each its declared variable (equal bitwise to two oracle reference
  cases) and refuses conflicting declarations; compat replays the defect.
- UD-037 (C, switch): upstream stores 4 cross-type pairs of a mixed mesh
  twice with a zero second weight; averages differ by 4.3e-16.
- UD-560 (the oracle segfaults; no fixture), UD-561, UD-559.

Upstream tests and checks: the `.verified` outputs of TEST-075, TEST-076
and TEST-078 identical; TEST-077, TEST-080, TEST-147, TEST-148 under
upstream's criteria; partition of unity within `(n + 1)ε`; averages against
a brute force `Σ W·J·v / Σ W·J` within `84ε`; non-local laws under uniform
strain equal the local laws within 1e-12. The abstract
`NeighborhoodMaxCriterion` is not ported (NL-009, approved exception). The
`KeyOrder` route is refused on distributed meshes.

### 5.12 Cohesive model

`oracle/probes/cohesive/cohesive_probe.cc` records 81 cases
(`fixtures/model/cohesive`): upstream's `TestSMMCFixture` in extrinsic and
intrinsic mode I and II on the 9 frozen meshes (1-D; 2-D linear, quadratic
and mixed; 3-D tetrahedra) and its finite-deformation variant on 8 types; a
random-strength case; each extrinsic law; probe-defined implicit static
cases from upstream's never-built `test_cohesive_intrinsic_impl`;
`test_cohesive_buildfragments`; adversarial cases (a 3-D two-law case with
every facet checkable, cyclic loading into compression, unsymmetric
implicit tangents, prestressed implicit reversal); facets-only insertion;
the rules and zone selectors of the `cohesive_extrinsic_ig_tg` example;
prescribed rigid motions that drive the friction law through contact,
stick, slip, reverse sliding and failure. Recorded per step: insertions,
energies, state digests; at the end every nodal field, the mesh and
facet-mesh topology, cohesive internals, facet laws; implicit cases add
Newton histories, reactions and `K`. One exact rule, every recorded
quantity bitwise (`cohesive_fixtures.rs` asserts the probe's case list, the
value total per family, and fails on any unconsumed quantity). At the last
full count: 75 cases, 70 compared and 5 recording upstream's exception,
1,518,374 values in the release tier (the 3-D cases run only there); the
six later cases add 54,460 values. Compat switches are set only where a
case depends on them. Cohesive dumping (10 cases,
`fixtures/model/cohesive_dump`): every file byte for byte under the compat
layout.

| Law | Bitwise against the oracle (upstream mode) | Independent checks only |
|---|---|---|
| `cohesive_linear` | explicit fixtures, random strengths, κ cases, adversarial cases, the rules selector; implicit compat cases | corrected defaults (UD-040, UD-220, UD-224, UD-241 to UD-245, UD-1160); closed forms at 1e-13 |
| `cohesive_bilinear` | 18 intrinsic fixtures, implicit and adversarial implicit cases | UD-218, UD-220, UD-241 to UD-245; closed form at 1e-13 |
| `cohesive_exponential` | nothing: upstream aborts (UD-028) | closed-form tractions and energies, mixed mode, contact, the tangent against finite differences, Newton-path independence |
| `cohesive_linear_fatigue` | nothing: upstream raises in 2-D and 3-D (UD-221) | transcriptions of upstream's `computeTraction`, every internal bitwise over cycles, contact and failure; not Newton-consistent in implicit runs (documented) |
| `cohesive_linear_friction` | explicit 2-D and 3-D cases through contact, stick, slip, reverse sliding, failure; an implicit prestressed sliding case | UD-249 play operator, UD-225, UD-247 |
| `cohesive_linear_uncoupled` | law cases, the adversarial 3-D case, the prestressed implicit reversal | UD-241, UD-244 defaults |

Corrected defaults (each with a switch reproducing upstream for the
replays):

- UD-040 (B): facet and strength pairing with several points per facet;
  the default compares a facet's stress with the mean of its points'
  strengths.
- UD-217 (B): 3-D facet tangents (upstream opens 134 facets at 0.95 σc
  under hydrostatic tension, the port none).
- UD-220, UD-034, UD-028, UD-241 (B): upstream's implicit tangent uses the
  base σc, keeps the upper triangle of `K`, does not activate the exponential
  law's history and raises δ_max at every traction evaluation, so an
  unconverged iterate can leave damage in a converged state. The default
  reaches the same equilibrium by another Newton path (202 against 446
  iterations on `implicit_static_bilinear`); reactions are compared within
  `√n_free(e_a + e_b)` plus a sanity allowance of `1e-3 max|R|`, and the one
  step outside (where upstream's state carries an overshoot of 1.34e-2 δc)
  is a recorded regression value. With the converged history, consistent
  and upstream tangents agree within the residual bound alone.
- UD-224 (B): intrinsic insertion with zero effective strength is refused;
  UD-223: extrinsic `_cohesive_3d_8` and `_cohesive_3d_16` are refused (no
  oracle case possible).
- UD-242 to UD-245, UD-1160 (B): closure band, penetration branch,
  uncoupled tangent, standalone-evaluation lifecycle, zero cohesive tangent
  once δ_max ≥ δc; each default is the derivative of the traction, checked
  by central differences.
- UD-249 (B), UD-225 (A), UD-247 (B): friction residual sliding with the
  signed `τ`, zero force at exactly zero tangential opening (upstream: NaN),
  implicit friction from the converged damage.
- UD-808 (B), UD-036, UD-222, UD-250 (zones, fragments).

Independent checks: closed-form traction-separation laws within 1e-13 of
the scale (measured at most 1.42e-15); dissipated energy of the mode-I
fixture `G_c × area` within upstream's 0.5 (20.003 against 20); energy
conservation of a free run after insertion; full intrinsic insertion with
adjacency, doubled nodes, fragments and mass conservation; upstream's
`test_material_cohesive_linear` at its tolerances.

Neumann loads on doubled nodes (UD-1280, B, D-105 as amended by D-108).
Upstream leaves the accumulated Neumann load of a doubled node with the copy
that keeps the original identity, which follows the facet orientation and,
on a distributed mesh, the partition; the oracle does the same bit for bit.
The port records each boundary element's nodal forces and moves the record
to the copy its element uses, by node identity, also after the insertion
rewrites the boundary rows; `doubled_node_keeps_external_force` keeps
upstream's placement for the replays. A node set to zero through the
external force is a reset; other written values stay with the original
copy; rescaling Neumann loads by writing the external force is not
supported by the default. Evidence: a closed form on upstream's cohesive
plate (deviation at most 1.9e-13; under the switch each lip is off by
exactly upstream's predicted misplacement), per-copy forces at 1, 2 and 4
ranks equal to the serial run (2.4e-16) and to an independent
re-application (2.7e-16), also with unequal nodal shares (2.75e-16).

The cohesive benchmark (x86_64, 2,500 steps with a check at every step,
upstream's laws) checks the placement at 1, 2, 4 and 8 ranks
(`docs/benchmarks/percopy-2026-11-13/analysis.txt`). With the corrected
placement every check passes at every rank count, the largest
`|f − f_re| / max|f|` is 1.836e-15 at h = 0.12 and 1.976e-15 at h = 0.06,
and the insertion history is the same at every rank count: at h = 0.06 the
serial insertion list (8,699 cohesive elements in 1,486 insertion steps) is
kept at 2, 4 and 8 ranks. With upstream's placement the first check after
the first doubled loaded node fails (h = 0.12 at step 838, h = 0.06 at
step 892), 1,663 and 1,609 of the 2,501 checks fail, and the insertion
counts differ from the corrected runs.

The loads are placed as integrated (UD-1296, C), and a mutation that renumbers
and then adds nodes is refused while loads are recorded. The cohesive
oracle scenarios drive the bodies by prescribed motion and do not exercise
this placement.

Not compared or compared weakly: the corrected defaults (analytical
evidence only); the exponential and fatigue laws (no oracle); there is no
mesh-objectivity study of the global response (convergence orders are not
the criterion for cracks along element boundaries). Distributed cohesive
runs and upstream's `test_cohesive` at two ranks: section 5.24.

### 5.13 Contact mechanics

Upstream's contact expressions operate on run-time sized Eigen objects; the
port reproduces their strict-build evaluation (`akantu-contact/src/eigen_ops.rs`:
sequential reductions, GEMV from `+0`, the scalar extracted from
`s * A.transpose() * v`, the run-time `1 × 1` inner-product fallback,
partial-pivot LU inverses, the nested `−J⁻¹`, halving-tree cross products).

`oracle/probes/contact/contact_probe.cc` records 24 cases
(`fixtures/model/contact`): detection on upstream's meshes (two-pass
`flat_on_flat`, the detector's triangle, quadrangle and mixed meshes,
interfaces rotated by 30° and 60°, adversarial ridges and curved 3-D
surfaces), penalty forces, moduli and the node-indexed `K`, the full
trajectories of `test_explicit_dynamic` (2,000 steps, a digest of
displacement and contact force per step, checkpoints, final stresses) and
`test_explicit_friction` (10,000 steps through stick and slip, generated on
a patched variant that zero-initialises the slave normal upstream reads
uninitialised, UD-001), and the error path of `penalty_quadratic`, which
upstream cannot construct (UD-370). All bitwise under the compat switches
each case reaches (`contact_fixtures.rs`, `contact_detection.rs`,
`contact_explicit.rs`, `contact_model.rs`):

- UD-363 (B): upstream's projection metric on rotated facets converges
  linearly and fails at 60°; the corrected metric is the default and
  changes no bit on axis-aligned facets. Rotated interfaces against the
  analytical gap, bound rel 1e-11: corrected 1.28e-13 (30°) and 7.2e-14
  (60°), upstream metric 3.97e-14 (30°).
- UD-002 (assembly and the node-indexed `K` with doubled off-diagonals),
  UD-008 and UD-368 (normal moduli: upstream's are off by 0.20 against
  finite differences), UD-365 (friction on the previous facet), UD-364,
  UD-850 (array resizing in `search()`).
- UD-369 (B): upstream's friction moduli are off by about 2 (stick) and up
  to 84 (3-D slip) against finite differences. The default is the
  consistent tangent `K_T = −∂f_T/∂u` with the history and the nodal area
  frozen, checked against central finite differences of the port's force
  on 36 configurations (2-D segments, 3-D quadrangles and triangles; flat,
  tilted, warped and curved masters; stick and slip; both pressure laws):
  worst 1.7e-9 relative, also across a change of master facet.

Independent checks: analytical gaps, normals, nodal areas and penalty
forces on flat interfaces (gap rel 1e-12, Σ nodal areas = face area within
1e-12, the nodal area on `flat_on_flat.msh` within 1e-10 because Gmsh
places its transfinite nodes with about 1e-13 absolute error), rotation
covariance, force balance with a deformable master, normal moduli against
central differences (1e-7 in 2-D, 1e-6 in 3-D), and upstream's two
registered tests with their own criteria (`|σ_yy|/7e5 − 1 ≤ 1e-2`; mean
`|t_T| ≤ 0.3·1.4e6·(1 + 1e-3)`).

Not compared: distributed contact search and resolution, which upstream
does not have (CONT-019, approved exception D-089; the port refuses a
distributed mesh); `penalty_quadratic` and other corrected behaviour rest
on analytical checks. Node-to-segment penalty contact is not a subject of
a convergence study (its consistency error depends on the penalty).
Couplers with contact: section 5.17.

### 5.14 Diffusion (heat transfer)

`oracle/probes/diffusion/diffusion_probe.cc` runs upstream's
`HeatTransferModel` with the Eigen solver and records 72 cases
(`fixtures/model/diffusion`): the intent of upstream's heat patch tests,
which upstream never builds (UD-014, UD-288; built in the oracle with the
package name patched), for the 12 regular types in static, implicit (100
steps) and explicit (100 steps at half the stable step) form; non-linear
cases with a non-symmetric anisotropic, temperature-dependent conductivity;
upstream's examples `heat_diffusion_{static_2d,dynamics_2d,dynamics_3d}` on
the frozen irregular meshes (11,857 nodes of `_triangle_6`, `_tetrahedron_10`),
shortened; two laws on a mixed mesh; 16 adversarial cases. Recorded:
temperature, rate, internal flow, per-step digests, Newton histories,
gradients and fluxes, matrices, energies. One exact rule; every value
bitwise (`model_diffusion_fixtures.rs`, `model_diffusion_v102_fixtures.rs`;
the first 56 cases held about 2.05 M values counted through the digests).
The replays run with the corrected defaults except the switches a case
reproduces: UD-287 (the solver's backward-Euler path, all cases) and UD-276
(upstream skips the second law on a mixed mesh).

Upstream defects: UD-275 (upstream throws `Unknown ghost synchronization
tag : diffusion` at the first distributed solve, so the distributed model
has no full oracle, section 5.24), UD-276, UD-278 (upstream's thermal
energy asserts on the mixed case, recorded), UD-280 to UD-287 (corrected
behaviours with switches), UD-282 (temperature dependence on every entry of
the conductivity), UD-283 (explicit stability: at half the stable step
`_quadrangle_8` and `_hexahedron_20` diverge, as upstream; every type is
stable at 0.2), UD-811 (cohesive elements in a diffusion mesh refused, as
upstream's `initFull`; a compat switch reproduces upstream's zero time
step), UD-012 (a second dynamic solver, which upstream refuses).

Independent checks (`diffusion_independent_checks.rs`,
`patch_test_linear_heat_transfer.rs`): upstream's patch-test assertions for
12 types (`T` 1e-10 absolute, gradient 1e-8 and flux 1e-9 relative);
steady 1-D solutions nodally exact for constant and for Kirchhoff-type
`k(T)` conductivity; temporal orders on the discrete sine mode: backward
Euler 0.971, 0.985, 0.993, trapezoidal rule 1 second order from a
consistent initial rate; heat balance of an insulated body with a source
at every step; lumped and consistent capacities equal to `∫ρc` within
1e-13; the stable time step against an independent estimate within 1e-13.

Spatial convergence (`akantu-diffusion/tests/spatial_convergence.rs`, the
method of section 5.9): static `−div(κ∇T) = s` with a smooth manufactured
field, isotropic conductivity for all 12 types and a full anisotropic
tensor for the ten 2-D and 3-D types; transient decay of a mode with the
lumped-capacity explicit scheme at a fixed Fourier number (P1 types,
`ρ`, `c`, `κ` distinct so that a misuse of the capacity is visible). All
converge at their theoretical orders under rule CONV-1 (L2 `p + 1`, energy
`p`; transient 2 and 1 with `h` and `Δt` refined together). Mutants of the
conductivity's off-diagonal term, of the capacity, of a quadrature weight
and of a derivative are each killed by a study; the off-diagonal mutant
passes every patch test.

### 5.15 Phase field

The splits work on run-time-size matrices upstream, so `trace()` and
`doubleDot` are sequential sums; assembly uses the bitwise FE engine and
Eigen solve. `oracle/probes/phase_field/phase_field_probe.cc` records 43
cases (`fixtures/model/phase_field`, x86_64 oracle): upstream's one- and
four-element loadings and optimal profile; patch tests over 11 types and
every law variant (AT2 and AT1 × `no_split` and `volumetric_deviatoric` ×
`history`, `penalization` and `tao`, random `g_c`); non-uniform histories
with unloading into compression; adversarial cases (mixed laws, AT1
recovery, full unloading, vol/dev in 1, 2 and 3-D); the dumper (17 files
byte-identical). Upstream's Newton failures are recorded and replayed. One
exact rule; in upstream mode every value bitwise (79,719 values in 42
cases), and a replay with one switch corrected differs in exactly the
groups declared for the case: UD-377 (several laws: the residual no longer
has `K` as its Jacobian), UD-378 (quadrangles of a mixed mesh not
assembled), UD-379, UD-380 (energies with random `g_c` or several laws),
UD-385 (negative `ρ_rec`, refused), UD-387 (B: the vol/dev energy driving
the damage in 1-D and 2-D; 13 cases), UD-388 (AT1 penalisation fully
unloaded). Explicit phase field fails upstream at `solveStep` (lumped) or
`initFull` (consistent) and the port fails at the same calls (UD-758).

Checks: the phase-field half of upstream's coupled tests; the exact
penalised point problem (AT2 1e-11, measured 8.7e-15; AT1 1e-12);
`test_phase_field_optimal` against the exact discrete solution (8.9e-14)
and the analytical profile within the derived phase error; `K` as the
derivative of the residual (1e-12); closed-form dissipation (1e-14);
`PhaseFieldElementFilter` against a probe; the deviatoric-split test in
both modes against its closed form.

Spatial convergence (`spatial_convergence.rs`): the `history`-mode damage
equations of AT2 and AT1 at fixed `l0` with a manufactured damage in
`(0.2, 0.7]`, 12 types: theoretical orders under CONV-1; one pre-asymptotic
pair (`_hexahedron_20`, `2 → 4` at `h = l0`) is resolved under CONV-1's
conditions and kept as reported only. Not covered: Γ-convergence as
`l0 → 0`, the non-linear splits, the `penalization` and `tao` modes (they
have bitwise oracle fixtures), the obstacle problem of AT1. TAO through
PETSc: section 5.26; the coupler: section 5.17.

### 5.16 Structural mechanics model

The structural model (Bernoulli beams in 2-D and 3-D, DKT18 plates, mixed
beam and shell meshes) is replayed from 21 oracle cases
(`fixtures/model/structural`, Eigen-pinned oracle): upstream's
`test_structural_mechanics_model` scenarios (static, line loads, implicit
and explicit dynamics, consistent and lumped mass, the Batoz patch at
`t = 1` and `t = 0.5`, a mixed beam and shell case, dumps) and 8
adversarial cases (a 3-D frame in general orientation with `Iy ≠ Iz` and
θy moments, tilted DKT meshes with imposed DOFs, a DKT strip with edge
beams, 3-D beam polylines with torsion in implicit and explicit runs, the
kinetic energy before the first step). In
`StructuralModelUpstreamCompat::UPSTREAM` mode 19,891 values are bitwise and
22,218 matrix coordinates equal (counted over 20 of the cases) (`structural_model_matches_oracle.rs`); 25
values of upstream's lumped mass, which is identically zero (UD-005), are
checked against those zeros and not counted as bitwise. Per switch, the
default mode differs from upstream in exactly the declared quantity
groups:

- UD-293 (B): the θy row of the 3-D beam `N` (section 5.4); line-load
  forces and the static state of 3-D frames.
- UD-007 (B): upstream's DKT bending stiffness scales with `t⁴` instead of
  `t³`; the default gives `w(0.1)/w(1) = 1000` exactly (upstream `10⁴`).
- UD-507 (B): the torsional inertia of the 3-D beam uses `ρ(Iy + Iz)`.
- UD-005 (B), UD-059: upstream's lumped mass is zero. The default lumps
  `m/2` per node for translations and an exact nodal 3 × 3 rotary block
  `Σ_e J_t e₁e₁ᵀ + J_b(e₂e₂ᵀ + e₃e₃ᵀ)` (D-059; an isotropic diagonal option
  remains); there is no upstream rule to compare with.
- UD-510 (kinetic energy before the first step), UD-508 (reassembly
  decision by bit patterns), UD-343 (beam normals, section 5.4).

Independent checks of the corrected defaults
(`structural_model_dynamics_checks.rs`, `structural_model_plate_checks.rs`,
`lumped_rotary_blocks.rs`): beam natural frequencies converge at `O(h⁴)`
with consistent mass and `O(h²)` with lumped mass (error ratios 4.00); the
simply supported DKT plate under a central load converges to Navier's
series (3.0 %, 1.0 %, 0.35 % at `n` = 4, 8, 16); the torsional frequency of
a clamped-free shaft equals the closed form of the lumped chain,
`ω_h/ω = (4N/π) sin(π/(4N))`, to rounding (−1.1384 %, −0.2853 %, −0.0714 %
at N = 3, 6, 12) and converges at `O(h²)` once `h < h*`; rotary blocks
against their closed form within 16ε (measured 3.4ε); rigid-rotation
kinetic energy `½ωᵀJω` within 16ε; an explicit torsional vibration of a
rotated shaft against a dense central-difference run within 1e-12 of the
amplitude (measured 5.2e-15) over 464 steps; objectivity of the blocks
(3.4ε) and of the spectrum (2.0e-12).

Not compared: the corrected defaults (analytical only, by construction);
the structural model on several ranks is refused, because upstream's
distributed residual is wrong (UD-902).

### 5.17 Model couplers

`oracle/probes/couplers/couplers_probe.cc` runs upstream's couplers (Eigen
solver pinned) and records 20 cases (`fixtures/model/couplers`) with
per-step digests, Newton histories, energies and checkpoint arrays; one
exact rule, every value bitwise under the upstream switches of the coupled
path (`coupler_fixtures.rs`, `contact_coupler_fixtures.rs`,
`contact_static.rs`, `friction_history.rs`, `cohesive_static.rs`):

- `CouplerSolidContact`: the explicit dynamic and friction trajectories of
  section 5.13; static Hertz (100 Newton steps with a search in each
  corrector, 52,084 values); static friction on the UD-001 variant (13,960);
  adversarial triangle pairings.
- `CouplerCohesiveContact`: 7,500 explicit steps with insertion and contact
  on the crack faces; the callback route (3,000 steps); a static set-up
  failing after 100 iterations; friction with insertion on the UD-001
  variant.
- `CouplerSolidPhaseField` (staggered): the twelve cells split × law ×
  irreversibility (the four TAO cells in the PETSc tier against
  `fixtures/solver/petsc/oracle/pftao/cpl_tao_*` under UD-422's switch);
  upstream's `test_multi_material`, whose Newton fails at step 1 (count,
  error and threshold reproduced), and a converging variant. Each cell is
  compared with every upstream switch and again with the corrected defaults
  plus only the switches it needs (UD-387, UD-388, UD-422); the six
  single-law cases need none.

Corrected defaults: UD-382, UD-619, UD-620, UD-622 (B), UD-617, UD-618,
UD-621 (A), UD-623, UD-624 (C), UD-772, UD-374. The static Hertz solve with
the corrected contact defaults reaches upstream's states within 1.5e-14 with
the same Newton counts (bound 1e-10); the coupled `K` equals solid plus
contact `K` within `8ε` per entry. UD-1291 (B, D-109): the static and
implicit solid-contact couplers search for contact before the first
residual of a step; upstream searches in the corrector only, so a step that
starts in penetration sees no contact force at iteration 0. Closed forms on
two pressed blocks hold to 2.3e-16 in the default and are missed by
upstream's order; `search_in_corrector_only` restores it, and
`cohesive_implicit_dynamic_qs` replays the oracle with it. Upstream's
coupled tests keep their criteria (`phase_field_coupling.rs`).

Spatial convergence of the coupled solid and phase-field problem through
the coupler (`spatial_convergence_phase_field.rs`, four 2-D types, both
fields): orders under CONV-1; the mutant of the coupler's strain factor ½
is killed. Not covered: a 3-D coupled case.

### 5.18 Embedded interface model

Upstream's embedded model does not compile at the reference commit
(UD-073, UD-587). The reference is a patched oracle variant (D-060,
`ORACLE_BUILD.md`): the patches restore compilation and two released
behaviours and change no formula, each hunk reviewed. On it upstream's
three tests pass and the current upstream writes the 2015 golden `matrix`
of `test_embedded_interface_model` byte for byte.

Fixtures: 15 cases from the variant (`fixtures/model/embedded`, x86_64,
three identical runs): stiffness, displacement, internal force and law
energies of upstream's tests, oblique and quadratic-background bars. Rules
(`fixtures/model/embedded/tolerances.json`): exact where the interface order
is upstream's (`t125`, `t126`, `oblique`, `triangle6_oblique`, bitwise also
in the default mode); class R (`emb_order`, bound `m ε max|K|` with
`m ≤ 12`) for the four case families where upstream's interface order
cannot be reproduced (UD-408: the pieces upstream collects depend on CGAL
intersection order and a duplicate survivor). Worst measured: `f_int`
1.45e-13 of the maximum (`t127`, bound 1e-12, margin 6.9), `u` 1.07e-15;
the other families at most 2.0e-16. CGAL's face intersections of grazing
bars are compared bitwise (3 bars × 162 tetrahedra × 4 faces).

Upstream tests with their criteria: the element matrix of one bar in one
triangle (1e-8), the 2015 golden `matrix` byte for byte (91 entries), the
pre-stress reactions against transformed-section beam theory (4.18e-4,
criterion 1e-3). Independent checks (`embedded_checks_{2d,3d}.rs`,
`review_v82_independent.rs`): affine patch tests with a non-symmetric
gradient, also with two laws over mixed background types (1e-12); a bar in
a matrix under uniaxial extension against the exact field and reaction
(1e-10 for solved fields); energy against work; interface topology (pieces
tile every bar without gap or overlap, lie in their element, sum to the bar
length, 1e-12) over a sweep of 600 3-D and 800 2-D bars through vertices,
edges and faces at tilts down to 1e-14 (worst length error 1.4e-13); grazing
bars within 1e-11 (measured 1.35e-12).

Upstream defects: UD-408 (above), UD-588 to UD-601 (indexing of mixed
background types UD-590, the collected piece set UD-599 and UD-601,
corrected by an exact tiling), UD-787 (one Gauss point per interface
segment). Upstream's example `embedded_model` crashes even on the variant
on its own second-order mesh (UD-851: the reinforcement's element filter
keeps the mesh's `_segment_3` boundary elements and dereferences a null
background filter, cause UD-1140); upstream is memory-safe when the
background mesh has no regular 1-D type other than `_segment_2`, so the
first-order version of the example's mesh is recorded on the variant
(`fixtures/examples/embedded_eigen_p1`) and compared: geometry and loads
bitwise, `u`, `f_int` and stress under `emb_order`. The example's own mesh
keeps independent checks only. Not compared: anything against the
unpatched upstream, which cannot run.

### 5.19 Split-node contact (traction at split nodes)

`oracle/probes/ntn_contact/ntn_probe.cc` records 49 cases
(`fixtures/model/ntn_contact`, x86_64 oracle, three identical runs): the
basics (pairs, normals, lumped boundaries, restart files and their
read-back), 15 rate-and-state runs on upstream's data files (5,000 energy
values each and the state every 10 steps), 20 friction-law cases (150
steps each, every regularisation), NTRF Coulomb contact, mode-II
asymmetric fields, factory selection, the parameter reader, and adversarial
cases. Every value bitwise and every written file byte for byte
(`oracle_ntn_fixtures.rs`, `oracle_ntn_adversarial.rs`); `ln` goes through
the glibc translation. The cases hold 19,658 default NaNs in 6 cases, each
caused by an upstream defect and documented in the manifest: UD-397
(simplified Prakash-Clifton with its default `t* = 0` gives `∞/∞`), UD-039
(the factory's fallback and the unstable explicit ageing update), UD-405
(revised N-shaped law: `0·(−∞)` where the slip velocity drops to zero),
UD-394 (upstream's restart read restores no value).

Upstream tests: `TestCompression` (28 contact pressures printed by upstream
with 12 decimals, upstream tolerance 5e-9; measured about 1e-15 relative,
in corrected and compat mode). Independent check of the rate-and-state
runs: the power balance `E_kin + E_pot = W_ext + dissipation`; upstream's
friction "external work" is the dissipation `−Σ f_fric·Δu ≥ 0`, so this
check cannot detect a sign error of the friction work. Restart files are
compared byte for byte; the corrected restart read restores values to the
12 significant digits upstream writes.

Upstream defects with switches: UD-039 (factory fallback to Rubin-Ampuero
with `t* = 0`), UD-394, UD-397, UD-405 and others in the register. The
`InclinedFlatDirichlet` boundary condition does not compile upstream
(UD-403); its port rests on an analytical check. Distributed split-node
contact: section 5.24 (refused on partitions that separate a pair, UD-1105,
and without `init_parallel`, UD-904).

### 5.20 IGFEM

Upstream's IGFEM package does not compile at the reference commit, so there
is no oracle (D-007). The evidence is the `.verified` outputs of upstream's
last working implementation, upstream's test criteria, independent checks
and analytical solutions:

| Compared | Class | Result |
|---|---|---|
| FE engine of the IGFEM elements against the `.verified` outputs of TEST-193 to TEST-196 (more than 500 printed values) | U, 6 significant digits | equal at 6 digits (`akantu-igfem/tests/test_fe_engine.rs`) |
| Partition of unity, reproduction of kinked fields, the Kronecker property, two independent shape evaluations, sub-element quadrature (6 structured meshes and a 1-D mesh) | A (bounds 4ε; 1e-13 / 1e-12; 1e-14) | pass (`independent_checks.rs`) |
| Patch tests TEST-190, TEST-191, also with general, through-node and near-degenerate cuts (`δ` down to 1e-14 of `h`) and two alternating sub-materials | U, A (1e-12 absolute) | L2 error 1.8e-16 (`_igfem_triangle_4`), 2.5e-13 (`_igfem_triangle_5`, criterion 1e-12) |
| Interface positions TEST-192 on one live model | U (area 4, L2 ≤ 1e-13 after each solve, element counts, enriched nodes) | L2 at most 1.28e-16; counts and nodes as upstream |
| Circular inclusion TEST-200 against `.verified` (29 printed displacements) and against the exact Eshelby-type solution | U; C as a regression guard only | equal; rate 1.87 on the two finest meshes (`n = 32 → 128`), guard ≥ 1.8, still increasing: not the asymptotic rate |
| Off-centre inclusion | C as a regression guard | rates 1.456, 1.751, 1.844 (`n = 8 … 64`) |
| Live interface moves, damage steps, extrapolation, weighted averages, dumpers (IGFEM-004 to IGFEM-010, TEST-201 to TEST-205) | U (upstream programs' criteria), A | live move against a fresh model: mesh bitwise, solution within 1e-13; `standard_fem` against `igfem` over 41 damage steps `|Δd| ≤ 1e-12`; a brute-force average within 9.8e-16 of `Σ W J |f| / Σ W J`; the element dumper at the 16 digits the ASCII writer prints |

The physical values at the enriched nodes
(`compute_values_on_enriched_nodes`, upstream
`computeValuesOnEnrichedNodes`) are accurate only to `|∇u| · 1e-10`
(absolute) when an enriched node lies within `1e-10` of its parent node 0:
the parent inverse map starts at node 0 and stops at upstream's absolute
physical residual `1e-10` without iterating. Otherwise they are exact to
rounding. This is upstream's behaviour, kept (category C). Measured errors
are 4.1e-11, 4.1e-13, 4.1e-14 and 4.0e-15 at distances 1e-10·h to 1e-14·h,
and at most 2.2e-16 from 1e-8·h on
(`review_v89_regressions::patch_tests_arbitrary_and_degenerate_cuts`). The
degrees of freedom themselves are exact for every cut.

Upstream defects (UD-602 to UD-616, UD-1200 to UD-1210) are defined
behaviour without oracle, each with a regression test; among them UD-611
(upstream's filter-position connectivity assigns wrong sub-materials with
several laws, O(1) errors), UD-606 (`contains`) and UD-613 (finite
deformation in the IGFEM laws is refused). Not available: parallel IGFEM
(approved exception, D-085: it does not compile upstream), node removal
through the IGFEM handler (refused; the interface move is ported as
separate mutations).

### 5.21 Extra materials

Upstream's extra-materials package does not compile at the reference
commit: 11 of 12 translation units fail, and the one that compiles
instantiates nothing (UD-572; the four non-local classes are even abstract
there, UD-1002). There is no oracle (D-007). Evidence for `brittle`,
`damage_iterative`, `damage_linear`, `iterative_stiffness_reduction`,
`viscoplastic`, `ve_stiffness_prop`, orthotropic damage, Vreepeerlings,
their non-local variants and the RVE homogenisation
(`akantu-extra-materials/tests/`):

- Upstream's own tests and reference outputs: the
  `test_material_iterative_stiffness_reduction.verified` text (44 lines,
  `setprecision(12)`) identical; `test_material_damage_iterative`'s
  assertions at upstream's tolerances; `test_eigenstrain_homogenization`
  (4.3e-19 against 1e-10); `test_elastic_homogenization` (worst 5.1e-11
  against 1e-10: the unit eigenstrain makes the strain columns nearly
  parallel).
- Closed forms written in the tests, never a call into the law, under rule
  XMAT-T1: relative 1e-12, with a floor of `1e-11 ×` the test's stress
  scale where a value passes zero; two chained evaluations 1e-11. Worst
  measured 2.7e-16 over 34 checks. Examples: the bilinear softening curve
  of the corrected `damage_linear` (`σ` within 2.3e-15 `Sigc`, work to
  failure `G_c + Sigc²/(2E)` within 8.5e-16), the orthotropic damage
  tangent against central differences (4.0e-16), objectivity and isotropic
  limits.
- Non-local variants on the non-local framework: averages against an
  independent brute force (rule XNL-1: the two evaluations differ only in
  summation order); the measured levels are not a claim of oracle
  agreement.
- Mutation testing: 31 of 32 mutants killed.

Upstream defects that the port corrects by default with a compat switch
include UD-780 (`damage_linear`'s curve: upstream's peak is 2.1016 `Sigc`),
UD-573 (the maximum equivalent stress taken from the last element type),
UD-585 (RVE volume on mixed meshes), UD-578 (`Crate` read into `Brate`).
Distributed extra-material non-local laws: section 5.24.

### 5.22 Geometry

`akantu-geometry` (LGPL: CGAL's `Simple_cartesian` kernel, the segment and
sphere intersectors, clean-room models of libstdc++'s ordered containers)
and `akantu-geometry-cgal` (GPL-3.0-or-later, opt-in feature `cgal`: AABB
trees and the spherical kernel, D-042). `geometry_probe` and
`geometry_adversarial_probe` record 17 cases (`fixtures/geometry`), one
exact rule, all bitwise in upstream mode (`oracle_kernel.rs`,
`oracle_geometry.rs`, `oracle_adversarial.rs`):

| Cases | Content |
|---|---|
| `kernel/triangle_segment` | 3,000 triangle/segment pairs (random, coplanar, through vertices, degenerate): intersection type and coordinates (511 points, 348 segments, one `CGAL_error()`), box/segment, ray/triangle, tetrahedron sides |
| `aabb/*` | primitive lists; 300 queries per tree: all intersections in traversal order (2,552 and 545 hits) and intersected primitives of rays |
| `segment_intersector/*` | upstream's `MeshSegmentIntersector` on upstream's test meshes and structured meshes, batches with physical names (1,797 segments on the triangle grid), including libstdc++'s set order |
| `spherical/line_arc_sphere` | 3,000 line-arc/sphere intersections: counts, multiplicities (793 tangents), points |
| `sphere_intersector/*` | 44 runs of upstream's class compile-fixed (UD-409) and zero-fill patched (UD-414), 23 of them ending in upstream's UD-413 assertion (partial results compared) |
| `adversarial/*` | thresholds, tangents, overflow (2,070 default NaNs from constructions that overflow to `±∞` and then `∞ − ∞`, an x86_64 record matched under D-046) |

libstdc++'s ordered-container behaviour that upstream's results depend on
(set order, equal-key order) is reproduced clean room (D-030) and checked
against black-box fixtures from GCC 14.2 on x86_64
(`akantu-geometry/tests/fixtures/cleanroom/`, `CLEANROOM_LOG.md`).

The port's defaults deviate from upstream on purpose (UD-407, UD-408,
UD-410 to UD-412: tolerant comparisons, tangent points rejected, overflow
handled) and have no oracle; they are checked by analytical invariants and
the ported upstream tests, and flipping a default back to upstream's makes
its test fail. Only upstream mode is compared with the oracle.

### 5.23 Communication and distribution

MPI lets the implementation choose the evaluation order of a reduction and
gives no rank-order guarantee; floating-point sums depend on it, and so do
`MPI_MIN`/`MPI_MAX` on signed zeros and NaN. `akantu_comm::Communicator`
therefore carries a reduction policy (D-039), agreed by every collective:

- `ReductionPolicy::RankOrdered` (the default, every backend): every
  reduction and scan is a rank-ordered fold of the gathered contributions,
  computed with the same code on every rank (ties keep the accumulated
  value, a NaN never replaces it). For a fixed rank count and partition,
  MPI at N ranks, the local world of N ranks and, for N = 1, the serial run
  agree bitwise (`mpi_acceptance::serial_local_mpi_equivalence`). This is
  not rank-count or partition independence.
- `ReductionPolicy::NativeMpi` (MPI only): upstream's own blocking calls
  (`MPI_Allreduce`, `MPI_Scan`, `MPI_Exscan`, `MPI_Reduce`, `MPI_DOUBLE_INT`
  with `MINLOC`/`MAXLOC`). It reproduces the oracle's reductions only with
  the same MPI implementation and version, configuration, rank count,
  datatype and count. The blocking native calls have no progress deadline
  after their checked header.

The association of `MPI_Allreduce` in the oracle's Open MPI 5.0.7 on one
node was measured: rank order at 2 and 3 ranks, `(0+1)+(2+3)` at 4, mixed
trees at 5 to 7, fully pairwise at 8, for 1 to 100,000 doubles. The tests use
it only to check data recorded with it and refuse a rank count without a
measured entry; the port relies on its own rank order. Class B/D-039
(section 2.6) compares such sums exactly through the per-rank
contributions. Sums that upstream folds in message-arrival order
(`waitAny`: the periodic synchronizer's `_sum`) are run-dependent upstream;
the fixtures mask exactly the order-dependent entries and the port's value
equals the rank-order fold of the recorded contributions.
`exclusive_scan` gives zero on every element of rank 0 under both policies
(upstream resets only the first, UD-187). On exceptional values the two
policies differ, and only the rank-ordered one is asserted: Open MPI's
native `MPI_MIN`/`MPI_MAX` propagate a NaN where the rank-ordered fold keeps
the accumulated value, and its result for a signed-zero tie depends on the
element count (reported as a diagnostic, never asserted).

Mesh distribution and synchronizers are compared bitwise with upstream's
per-rank records given the same partition (upstream's seeded Scotch
partition, recorded and prescribed, or a rule written in both the probe and
the test; the port's own Scotch binding gives the oracle's 2-, 4- and
8-part partitions of `cube.msh`). Records (`fixtures/parallel`, one
directory per rank): distribution of `cube.msh` at 1, 2, 4 and 8 ranks and
of adversarial 2-D, sparse mixed, mesh-data and periodic strip meshes at 1,
2 and 4 (bounding boxes bitwise); the facet, grid and periodic
synchronizers, one scenario each at 8 ranks and the others at 1, 2 and 4,
with additional adversarial cases. Tests: `mpi_distribution`,
`mpi_synchronizers` (real MPI), `parallel_oracle*.rs` (local world).
Upstream defects: UD-231 (an orphan point, compat), UD-251 (mixed facet
types: upstream does not complete), UD-654 (periodic slave order), UD-655,
UD-1060 and UD-1061 (a mesh made periodic before the distribution crashes
upstream when a relocated rank holds a periodic slave, and nodal data of
missing partners are read past the buffer; the port corrects this and the
strip cases avoid the crash). Upstream creates no periodic node
synchronizer when it distributes a periodic mesh (UD-254); the port refuses
`makeConsistentForPeriodicity` on such a mesh until the periodic
synchronizer is updated, after which the models run upstream's distributed
periodic-consistency algorithm. Lifecycle and error propagation (agreed
failures, aborts, finalisation): `MPI_VALIDATION.md`.

### 5.24 Parallel models

`MPI_VALIDATION.md` gives every distributed model with its rank counts,
partitioners, class, worst deviation, tests and fixtures; this section
states the evidence in short. Two kinds of comparison exist: oracle
comparisons at a fixed partition (class B or B/D-039, at 1, 2 and 4 ranks
and at 8 for the scenarios recorded there), and the port at P ranks against
its own serial run with RCB partitions at 1, 2, 4 and 8 ranks (class S,
bounds between 1e-14 and 1e-10 stated per test). The par tier ran every
binary three times per rank count.

| Model | Oracle | Against serial |
|---|---|---|
| Solid mechanics, explicit (one and two laws) | B/D-039 at 1, 2, 4, 8 (two laws: 1, 2, 4): fields bitwise; energies exact through the per-rank contributions (the two associations differ by at most 1.861e-16 relative) | `|du| ≤ 1.179e-18` of 6.835e-4 (bound 1e-12) |
| Solid mechanics, static | X at 1, 2: 1.920e-16 against MUMPS (rule 1e-12; upstream's 4-rank MUMPS solve is not repeatable, UD-348) | `|du| ≤ 2.776e-17` of 1.452e-1 (1e-10) |
| Solid mechanics, implicit dynamic | none | cube `|dv| ≤ 1.518e-18` of 1.243e-3 (1e-10); TEST-109 bars `|dv| ≤ 1.235e-15` of 6.103e-2 (1e-12) |
| Random fields (`clh_marigo`) | B at 1, 2, 4 | none |
| Cohesive (intrinsic, extrinsic, facets-only, clusters, fragments) | B/D-039 at 1, 2, 4, 8 (two 2-D scenarios) and 1, 2, 4 (107 rank records) | dissipated energy within 1e-12 |
| Upstream's `test_cohesive` at 2 ranks, with the port's partition handed to upstream | B (40 cases × 2 ranks and 80 boundary records, rule `ptc_exact`) | none |
| Diffusion | B for the gradient at 1, 2, 4, 8 (upstream's distributed model throws at its first solve, UD-275) | explicit `|dṪ| ≤ 1.634e-13` of 91.51 (1e-13), implicit 3.264e-14 of 1.345 (1e-12), static 7.105e-15 of 100 (1e-14) |
| Phase field and coupler | B at 1 rank; E at 2 and 4: 3.55e-15 over every recorded upstream result (bound 1e-14) | `|dd| ≤ 2.220e-16` of 0.5224 (1e-14) |
| Non-local laws | B at 1, 2, 4, 8 (two scenarios) under the compat switches UD-1101 to UD-1103 | corrected defaults: damage 2.381e-15 (1e-14), internal force 1.722e-14 (1e-12); upstream's exchange (UD-1103) is 4.272e-2 from serial |
| Split-node contact | B/D-039 at 1, 2, 4, 8 (external work: rank order 1 ulp from upstream's `MPI_Allreduce` at 8 ranks, both folds exact; NativeMpi bitwise) | none |
| MUMPS and PETSc back-ends | none (sections 5.25, 5.26) | MUMPS against serial Eigen: 9.592e-15 static linear (1e-12) |
| Python module, mesh level | B at 1, 2, 4 (7 of 7 rank outputs equal) | none |

Where upstream itself depends on the rank count or the run (the
`MPI_Allreduce` association, arrival-order sums, ghost filters in `waitAny`
order (UD-232), multi-rank MUMPS (UD-348, D-082), ghost boundary elements
corrupted at distributed extrinsic insertion (UD-890), a local node blocked
per rank (UD-349), aborts with two non-local neighbourhoods (UD-1100),
per-rank split-node pairing (UD-1105), Neumann loads on doubled nodes
(UD-1280)), the comparisons are built around it, never through a tolerance;
`MPI_VALIDATION.md` explains each. Refused on several ranks: the generic
contact model (CONT-019), the structural model (UD-902), parallel IGFEM
(D-085), distributed 1-D facet meshes (UD-1141), the non-local `KeyOrder`
route (D-097), split-node contact on partitions that separate a pair
(UD-1105) or without `init_parallel` (UD-904), phase-field element events,
and the Python module at model level (D-101).

Limits: 8-rank oracle comparisons exist only for the scenarios recorded at
8 (no 3-D cohesive rule at 8); one multi-node cluster (eight hosts, up to 64
ranks, one run per point) was run, with no oracle recorded across hosts;
no count or energy of the full-size benchmark runs is compared between rank
counts or codes as a correctness criterion (`MPI_VALIDATION.md`).

### 5.25 MUMPS back-end

The port's MUMPS back-end calls the same MUMPS, ScaLAPACK, OpenBLAS and
Open MPI shared objects as the oracle (byte-identical in both images). On
13 comparison systems (elasticity, the `lu/*` systems, upstream's
diagonal, `fixtures/solver/mumps`), with OpenBLAS pinned to Haswell and one
thread on x86_64 and three runs per side:

| Ranks | Port against oracle | Run to run |
|---|---|---|
| 1 | 13 of 13 bitwise | bitwise on both sides |
| 2 | 13 of 13 bitwise in these runs | up to 5.4e-16 measured elsewhere on x86_64: diagnostic only |
| 4 | 6 bitwise, the others within 1.96e-15 of `max|x|` | port up to 1.76e-15, oracle up to 4.61e-15: diagnostic only |

One-rank bit identity holds only for identical shared objects and the same
OpenBLAS core type and thread count on the same ISA; it is promised only
there (the aarch64 records are a platform variant, `PLATFORM-VARIANTS.txt`).
Multi-rank MUMPS is not repeatable run to run, upstream's included (UD-348),
so multi-rank results are compared with the port's serial Eigen solve
instead (static linear 9.592e-15, static Newton 0, implicit Newmark
2.864e-16, bound 1e-12, identical Newton counts; `mumps_mpi` at three
thread levels, MUMPS tier). A relative residual `‖Ax − b‖/‖b‖ ≤ 1e-10` on
every system is an empirical smoke check, not an error bound.
Refactorisations and re-solves with the same factors are bitwise at one
rank and within 1e-12 at more. On the cohesive benchmark set-up (h = 0.12
cube, 1,500 steps), the port's MUMPS run equals upstream's MUMPS run
bitwise at one rank in 9 of 9 pairs, and so do the two Eigen runs.

UD-216 (B): upstream's workspace retry (`ICNTL(14)`) re-solves with the
already overwritten right-hand side and returns `A⁻⁹b` (1.443e23 relative
from the answer in the recorded case); the port's default re-solves the
original right-hand side (bitwise equal to the solve without retry at 1
and 2 ranks), and the compat mode reproduces upstream's vector bitwise at
1 and 2 ranks. `mu2_05_concurrent_solvers` (two solvers under
`MPI_THREAD_MULTIPLE`) passed 200 of 200 repetitions at 1, 2 and 4
processes after the refused `destroy()` was retried (it failed 3 to 7 %
before).

### 5.26 PETSc back-end

The PETSc back-end (KSP, SNES, TAO, the models on the PETSc DOF manager)
is compared with the pinned x86_64 serial oracle record
(`fixtures/solver/petsc/oracle/`; PETSc tier, `docs/TESTING.md`). Bitwise:
upstream's `test_sparse_solver_petsc` (default KSP), `test_model_solver_petsc`
static (Newton with GMRES and ILU(0)), a 17-node cubic bar under Newton with
GMRES+ILU and with a direct solve, SNES on the linear bar, SNES on the
cubic bar reproducing upstream's `DIVERGED_LINE_SEARCH` (UD-428 compat),
TAO on the linear bar, and the phase field under `petsc_tao` (TEST-095 and
TEST-100 per law and split, 1,000 steps, five records) under the DOF
manager's UD-422 compat switch (row-oriented blocks). Not bitwise: the
dynamic run, whose energies differ in 2,081 of 2,406 values (at most
1.41e-15 relative) with identical iteration counts at every step, within
rule PETSC-DYN; making it bitwise would need PETSc's own product order.

Bounded TAO: a bounded step of the toy bar against the closed form and a
dense primal-dual active-set solution of the discrete obstacle problem
(3.0e-16; KKT residual 2.8e-14). Bounds that exclude a zero increment are
refused by default (UD-1022, D-088); under the compat switch the result
differs from the obstacle solution of the load by 0.32 and equals that of
`f − Kδ₀` within 2.2e-16. Multi-rank PETSc has no oracle, because
upstream's parallel PETSc path is unfinished (UD-006, UD-429); multi-rank
runs are compared with serial Eigen (bounds 1e-12 for direct solves and
1e-14 for `pc_type none` models against one rank, rule PETSC-KSP-NEWTON for
the default KSP), not re-measured by the MPI report; iteration counts across
solvers (SNES's line search against Akantu's Newton) are reported, not
compared. Upstream's patch tests on the PETSc DOF manager (`auto[petsc]`, 80
cases; `petsc_snes[petsc]`, 48, since upstream skips the explicit ones for
SNES) run under upstream's criteria. Other defects with
switches: UD-422, UD-427 (reactions; with upstream's, the cubic bar stalls
on 2 and 4 ranks), UD-428.

Not compared outside `x86_64-haswell` (D-121): `oracle_serial` and
`blas_pin` (they report and return), the oracle assertions of
`phase_field_tao_oracle` and `coupler_tao_oracle` (the runs, bounds,
irreversibility and record checks still run, the differences are printed),
and the one-rank record check of `test_phase_field_optimal_linear_distributed`.
On aarch64 Linux these give 20 `not compared` lines over the tier's
launches.

### 5.27 Python interface

Bindings. Snippets under `python/oracle/snippets/` run the same script
against the port's package and upstream's module in the same pinned image;
`python/oracle/compare.py` compares every printed key at relative tolerance
0 (floats as `float.hex`, arrays bitwise, dump files byte for byte). They
cover mesh reading, parser values, the FE engine, static and explicit solid
models, the solver layer with upstream's `InterceptSolverCallback` flow,
custom and bi-material laws, the laws API, dumpers, cohesive runs,
phase-field subclasses registered from NumPy, Gmsh conversion and the other
binding rows. Where a script uses upstream's default solver (MUMPS), the
comparison uses its Eigen-pinned twin, and the MUMPS run is diagnostic (for
example `bi_material`: 5.4e-15 relative, the Eigen twin bitwise). Not
compared because upstream raises, aborts or reads freed memory: UD-830,
UD-832, UD-834 getters, UD-366 (`GeometryUtils` aborts), UD-836 (upstream
assembles zeros), UD-024 (a view that outlives its owner dangles upstream;
the port's views hold a lease). Zero-copy views, lifetimes and error mapping
are checked as behaviour.

Upstream's Python examples run unchanged with the port's package and with
upstream's module: 31 cases (`fixtures/python/examples`, pytest
`python/tests/examples`), standard output, dump files and final arrays,
with no tolerance and four documented comparison rules: UD-110 (matrix
arrays written as zero-filled squares; the squares' top-left blocks must
equal the port's tuples bit for bit), UD-114 (an invalid base64 stream of an
empty payload, compared as empty), UD-061 (Matrix Market at 17 digits,
re-rendered at `%.15g` must have the oracle file's SHA-256), UD-085 (a
NumPy view taken before insertions grew the mesh reads freed storage
upstream: not compared). Where a script solves with MUMPS, the unmodified
run is diagnostic and its Eigen-pinned `_eigen` variant is the target
(for example `solver_callback`: printed energies 3.7e-15 relative between
MUMPS and Eigen). Cases where both sides raise at the same line
(UD-980), where the port refuses (UD-981) or where upstream aborts after
an insertion (UD-986) are compared up to that point; wall-clock values are
masked.

Platform keys (class X1, section 4.1): four example comparisons depend on a
value NumPy computes alone. `dynamics` and `custom_material` build an
initial pulse with `np.exp`, whose float64 kernel on x86_64 is an
AVX-512F kernel exactly when the CPU has AVX-512F; the oracle ran on an
AVX-512F CPU (AMD EPYC 9455), so these two cases are compared bitwise only
where `python/examples/cases.py` reports the platform key
`x86_64-linux-glibc` and `numpy_avx512f`, and skip with that reason
elsewhere (on an x86_64 CPU without AVX-512F 149 of 4,221 pulse values
differ by at most 2 ulp). `phasefield_static` and `phasefield_static_eigen`
skip only the two final norms `np.linalg.norm` computes (one ulp of NumPy on
identical inputs off the platform of record). `ci/pytest-skips.txt` lists the
accepted skips. Whether a given CI runner has AVX-512F was not observed; a
failure of these cases on a runner that reports `numpy_avx512f: True`
would refute the key.

Models on several ranks from Python are not provided (D-101); the
mesh-level module is compared at 1, 2 and 4 ranks (section 5.24).

### 5.28 Examples

Upstream's C++ examples are run by the port and by the upstream binary
(`oracle/probes/examples/run.sh`, three identical runs per case on the
frozen meshes) and compared through the SHA-256 of every output file, VTU
array digests and standard output: 62 recorded runs (`fixtures/examples`),
replayed by `crates/akantu/tests/examples_*.rs` (short runs in the fast
tier, whole runs in the medium tier). Every case is byte-identical or
bitwise in its arrays, with these qualifications:

- Examples that solve with MUMPS upstream (`static`, `example_parser`,
  `implicit_dynamic`, `heat_diffusion_static_2d`, `contact_explicit_static`,
  `material_viscoelastic_maxwell_energies`, `phase_field_notch`): the
  solve-independent arrays are bitwise, the solved fields are diagnostic
  (`example_parser` displacement 1.04e-15 from MUMPS with the pin), and the
  `--solver eigen` variants are the targets, byte-identical against the
  pinned oracle (`example_parser_eigen` and `implicit_dynamic_eigen` exact
  since the Eigen pin, D-050).
- Examples whose input calls libm transcendental functions directly
  (`user_defined_bc`, `explicit_dynamic`): authoritative on x86_64 glibc;
  on macOS the affected arrays differ in the last bits.
- Examples whose upstream output depends on a defect run with
  `--upstream-compat` for the comparison (UD-472, UD-473, UD-474: upstream
  writes invalid VTK for `dumpable_interface`; UD-217 for
  `scalability_test_cohesive`, where the corrected default is bitwise until
  upstream's first insertion at step 969 and then inserts later; UD-850 for
  `cohesive_contact_explicit_dynamic`; UD-029 and UD-010 for the Maxwell
  energies example). The defaults are checked against closed forms.
- `parallel_2d` with each code's own partition shares no partition: the
  displacement is diagnostic (6.3e-12 of the maximum after 10,000 steps,
  printed), velocity and acceleration are asserted within 1e-11 of each
  field's maximum (3.9e-14 and 3.6e-14); with the recorded partition
  replayed on two local ranks, the run is bitwise apart from upstream's
  uninitialised stress padding.
- Not compared: `embedded` on its own second-order mesh (upstream ends with
  signal 11 even on the patched variant, UD-851; independent checks only;
  the first-order variant of its mesh is compared, section 5.18),
  `phase_field_parallel`
  on 2 and 4 ranks (the oracle's runs are not reproducible: 18 of 30 and 36
  of 50 files differ between three identical runs, distributed MUMPS),
  `bernoulli_beam_2_example` as upstream runs it (one explicit step with a
  zero lumped mass printing `-nan`, UD-514; the port refuses the zero mass,
  and the static variant is compared).

Python examples: section 5.27.

### 5.29 Upstream tests and golden files

Every upstream test is ported to a Rust test with the same intent and name,
and the mapping is in the TEST rows of `FEATURE_PARITY.md` (182 rows, 177
Verified, 5 approved exceptions: tests never built, of removed APIs, or CI
helpers). Ported tests keep upstream's criteria; the oracle comparisons of
the same quantities are in the area sections. Some upstream tests were
extended where upstream only printed: for example the MUMPS instances of
the patch tests (`patch_mumps`), which upstream prints and does not assert,
compare the same `K` bitwise.

Upstream's golden files (`.verified`, `K_matrix_verified.dat`, `matrix`,
`python_fe` references) are replayed the way upstream's harness compares
them (`docs/inventory/golden_files.md`), and upstream's driver never
actually diffs `.verified` files (UD-018), so each is first confirmed
against the oracle's own run. Examples: `K_matrix_test.dat` of
`test_assembling_K_cohe_elements` byte-identical to the oracle's output of
the same driver (with a one-line constructor fix in the driver, UD-697);
upstream's own comparison with its 2015 golden rejects 7 of 99 pairs in
both codes because that golden holds an inserted cohesive element that the
current driver no longer inserts (stale golden, UD-697); `python_fe`'s
criterion `‖u_py − u‖/‖u_py‖ ≤ 1e-14` kept unchanged (two repairs needed
to run upstream's script, UD-699). Not compared byte for byte: two legacy
`.verified` files and six 2017 `.out` files written through removed APIs
(GOLD-003, GOLD-007, approved exceptions). Where an upstream test only
printed, the port asserts what can be asserted and says what it does not
compare: for example a ported plasticity test compares the printed
reactions up to the step where upstream's Newton stalls (UD-732), and
`TestDKT18.DISABLED_PrecomputeRotations`, which upstream only prints and does
not run, becomes per-element checks of the rotation blocks.

## 6. How to rerun

`TESTING.md` has the detail (images, environment keys, tiers and their
run times).

| What | Command |
|---|---|
| Every differential test on the committed fixtures, one crate | `cargo test -p <crate>` (for example `akantu-solid`, `akantu-cohesive`); the 3-D cohesive cases and the finest convergence levels need `--release` |
| Fast tier, whole workspace | `cargo test --workspace` |
| Fixture integrity | `cd fixtures && sha256sum -c SHA256SUMS` |
| Tolerance ledger and counts | `python3 scripts/gold/tolerance_ledger.py --check`; `python3 scripts/docs/validation_counts.py`; `python3 scripts/parity/stats.py` |
| A platform variant | `AKANTU_FIXTURE_ENV=aarch64-auto cargo test -p akantu-validation` (or `x86_64-haswell`) |
| MPI, 1 / 2 / 4 ranks and 8 | `scripts/mpi/run-tests.sh`; `scripts/mpi/run-tests.sh --ranks 8 --skip-lint` |
| Distributed models against the oracle and the serial runs | `scripts/mpi/run-par-tier.sh --ranks "1 2 4 8" --repeat 3` (Scotch and MUMPS images as in `TESTING.md`) |
| Python comparisons | pytest `python/tests/examples` in the oracle image; `python/oracle/compare.py` for the snippets |
| Regenerate an area's fixtures | `oracle/scripts/generate-fixtures.sh --config strict <target>` or the area's `oracle/probes/<area>/generate.sh`, on the pinned x86_64 oracle (`ORACLE_BUILD.md`) |
| Compare two fixture trees | `cargo run -p akantu-validation --bin fixture-diff -- <a> <b>` |
| Convergence mutants | `crates/akantu-solid/scripts/conv_mutants.sh <log-dir>`; `scripts/conv/conv2_mutants.sh` |
