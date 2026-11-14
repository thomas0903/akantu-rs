<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Maintainability scan of the akantu-rs repository

This scan covers code health and maintainability. It was made on 14
November 2026, before the repository was prepared for publication, and
revised for the public tree on 15 November 2026. It looks for what would
make it hard for people and coding agents to keep the port alive once it is
public, and for what would make it easier. The library code is in good
shape. There are no `todo!` or `unimplemented!` markers in merged code,
there is no `unwrap()` outside test modules, and the `unsafe` code is
inventoried with a check script that passes. Most of the cost lies around
the code: 472 integration-test binaries, a fixture tree of 73,938 small
files with one 11 MB checksum file, test files named after the reviews
that produced them, and documents and scripts that a large part of the
first version of this scan asked to move out of the public tree.

The preparation of the public repository resolved the findings of section
1 except 1.8, which is partly done, and findings 2.1 (mostly), 2.8, 2.9 and
3.3 (in part). Each of them now says what was done and what remains. The
section and finding numbers are those of the first version, because
`04-possible-improvements.md` cites them (M2.3, M2.10 and so on). Counts
were taken again on 15 November 2026 where they are cheap to take (`wc`,
`git ls-files`, `git grep -c`, `scripts/unsafe_inventory.py --list`,
`cargo metadata --no-deps`); the others say that they date from the scan.

## Method

Everything below was measured with light local commands on the tree:
`wc`, `grep`, `git ls-files`, `cargo metadata --no-deps`, `cargo tree -d
--offline`, `python3 scripts/unsafe_inventory.py --check --strict-safety`
and `--list`, and small Python scanners kept outside the repository
(function lengths from brace matching, 15-line duplicate windows, regex
counts of identifiers). No build and no test run was made. `tokei`, `cargo
machete` and `cargo udeps` were not installed, so unused dependencies were
not checked. Line counts are physical lines, comments and blank lines
included. The function-length scanner is approximate and can be fooled by
macros. Run times come from `ci/github/README.md` (the first runs of the
workflows on GitHub, 14 November 2026) unless another source is named.
`D-nnn` are entries of `docs/DECISIONS.md`, `UD-n` entries of
`docs/UPSTREAM_DEFECTS.md`.

## The repository in numbers

Taken on 15 November 2026.

| Quantity | Value | Source |
|---|---|---|
| Workspace crates | 27 | `Cargo.toml`, `crates/` |
| Rust files under `crates/` | 1,002 | `git ls-files 'crates/*.rs'` |
| Rust lines under `crates/` | 514,305 (`src/` 263,511, `tests/` 240,811, `examples/` 9,852) | `wc -l` |
| `#[test]` attributes | 3,394 (more tests come from macros: the workspace run reports 3,769 passed, 25 ignored) | grep; `docs/TESTING.md` |
| Integration-test files directly under `crates/*/tests/` | 472, each its own test binary (see 2.3 for the 462 binaries of a default workspace run) | `ls crates/*/tests/*.rs` |
| Python tests | 34 files, 235 `def test_` functions under `python/` | grep |
| Tracked files under `fixtures/` | 73,938; 241,836,216 bytes of content (467 MB on disk) | `git ls-files`, `du` |
| Tracked files under `docs/` | 819 files, 24.2 MB | `git ls-files` |
| `unsafe` sites | 221 in 8 crates | `scripts/unsafe_inventory.py --list` |
| `#[allow(...)]` attributes | 458 | grep |
| Cargo features | 48 in 16 crates (16 in the facade crate `akantu`) | `cargo metadata` |
| Packages in `Cargo.lock` | 183 | `Cargo.lock` |

## 1. Before the repository goes public

### 1.1 Root instructions describe the agent campaign

Done. At the scan, `AGENTS.md` was the operating manual of the porting
campaign (agent models, remote machines, budgets, a publishing script and
process files to update). It is now a short file for contributors and
their agents: the layout, the commands, the numerical rules, the rules for
tests, fixtures, `unsafe` and licensing, which document to update, and
what to ask a maintainer first. `CLAUDE.md` includes it.

### 1.2 Personal paths and machine defaults in tracked files

Done. At the scan, 195 tracked files contained the home-directory path of
the author's machine, and the oracle generators used it as a default. The
generators now require `AKANTU_SRC_HOST` (for example
`oracle/probes/core/generate.sh:22` stops with a message when it is
unset), and the test `public_tree_guard` (`crates/akantu-validation`)
rejects home-directory paths outside recorded run outputs. What remains:
four recorded outputs under `oracle/results/` still name the remote
machines they ran on (`git grep -l 'bx_'`); the guard exempts that
directory because editing a record would alter it.

### 1.3 Process history in `docs/`

Done. The review reports, briefs, proposals, progress and budget records,
the remote-host and publishing scripts and the raw benchmark runs left the
tree, and the public repository starts from a fresh history. The documents
a maintainer needs remain: `ARCHITECTURE.md`, `TESTING.md`,
`NUMERICAL_VALIDATION.md`, `TOLERANCE_LEDGER.md`, `UPSTREAM_DEFECTS.md`,
`FEATURE_PARITY.md`, `VALIDATION_SUMMARY.md`, `MPI_VALIDATION.md`,
`DEPENDENCIES.md`, `ORACLE_BUILD.md`, `REFERENCE.md`, `BENCHMARKS.md`,
`PLATFORMS.md`, `MAINTAINING.md`, `inventory/` and `manual/`.

### 1.4 No entry point for contributors

Done. The root now has `CONTRIBUTING.md`, `SECURITY.md`, `CHANGELOG.md`
and `CODE_OF_CONDUCT.md`. `docs/MAINTAINING.md` has the procedures this
finding asked for: adding a material, an element type and a dumper field,
fixing a bug that changes results, adding or regenerating fixtures,
changing a tolerance, following a new upstream commit, updating a
dependency and cutting a release. `docs/TESTING.md` is a guide of 536
lines (8,191 at the scan).

### 1.5 Tests that guard process documents

Done in the main. The audit test `parity_audit3.rs` and the audit scripts
left with the process records; `doc_integrity_guard`, the tolerance-ledger,
parity-statistics and upstream-data-manifest tests remain and guard
documents that stay maintained. A contributor who edits a document still
runs `cargo test -p akantu-validation` and `cargo test -p akantu --test
manual`, as `AGENTS.md` says.

### 1.6 The C API crate documents files that do not exist

Done. `crates/akantu-capi/src/lib.rs` (40 lines) now documents what it
exports, one function `akantu_version`, and says that a C API is not part
of the release. At the scan it described handles, a generated header and a
C++ wrapper that did not exist.

### 1.7 Choose the pull-request CI before enabling the workflows

Done. At the scan a pull request touching `crates/` would have started
about 249 runner minutes of jobs. In the public workflows
(`ci/github/README.md`) a pull request runs `fast.yml` (`fast`,
`selftest`, `msrv`) and `docs.yml` (`rustdoc`, `manual`); the feature
matrix, `gates`, `facade-release`, `exportall`, the MPI legs, Python,
packaging, memcheck, coverage and the distribution check run weekly or on
manual dispatch. Still open from the proposal: a cargo registry or target
cache (the only `actions/cache` entries hold Docker images, and the
target of 21 to 27 GB does not fit GitHub's 10 GB cache, see 2.3) and
parallel pytest (`pytest-xdist`; the `python` job took 4,288 s on a 2-vCPU
runner).

### 1.8 Licence headers outside Rust

Partly done. All 1,003 tracked `.rs` files outside `fixtures/` carry an
`SPDX-License-Identifier` line in their first five lines, as do all `.c`,
`.h` and `.yml` files. Missing: 22 of 225 `.py`, 2 of 241 `.sh`, 1 of 162
`.cc` and 1 of 33 `.toml`. They are 21 upstream Python examples (20 under
`python/examples/`, one under `crates/akantu/examples/`) that keep
upstream's `__copyright__` and `__license__` lines, the upstream benchmark
driver `benchmarks/cohesive-perf/src/cohesive_extrinsic.cc` with its own
LGPL notice, the two scripts of `docs/benchmarks/mpi-latency-2026-11-14/`
and `docs/manual/book.toml`. `NOTICE.md` and `docs/DEPENDENCIES.md` list
the third-party code, and the `licence` CI tier (`ci/tools/licence_audit.py`,
`ci/licence-policy.toml`) checks crate licences and the GPL separation of
`akantu-geometry-cgal`.

Why it matters: automated licence scanners read SPDX lines, and the
upstream examples state "LGPLv3" without the "or later" of the rest of the
port.

Proposed change: add SPDX lines to the two scripts and `book.toml`, add an
SPDX line next to upstream's notices on the examples where that is
compatible with them, and extend the `licence` tier's header check to
`.py`, `.sh`, `.c` and `.cc` outside `fixtures/`.

Effort: two hours. Risk: none.

## 2. Soon after going public

### 2.1 Internal ids in code comments, test names and messages

Mostly done. At the scan, 7,241 lines under `crates/` carried an id of the
porting process or a pointer into `docs/` (2,382 review numbers, 1,217
work-package ids among them). The comments were rewritten for the public
tree; on 15 November 2026 `git grep` finds 5 lines with a work-package id
and 3 with a review number in `crates/**/*.rs`. The ids that the public
tree defines remain, and carry information: 3,104 lines cite an upstream
defect (`UD-n`, documented in `docs/UPSTREAM_DEFECTS.md`) and 1,159 a
decision (`D-nnn`, documented in `docs/DECISIONS.md`). What remains are
the review ids in test file names (2.2).

Proposed change: a CI grep that rejects new work-package and review ids in
`crates/`.

Effort: an hour. Risk: none.

### 2.2 Test files named after reviews

Evidence: 66 test files are named after a review, an audit or a work
package of the port (`review_v32_regressions.rs`, `v119_scoped_adversarial.rs`,
`smm2_interpolation_v87.rs`, `sweep1_damping.rs`, `par2a_common/mod.rs`,
and so on). Together they hold 409 `#[test]` functions in 33,971 lines.
`crates/akantu-solid/tests/` alone has 14 `review_v*` files. 178 test
functions carry a `UD` id (`ud039_singular_rate_and_state_at_rest`), which
is fine.

Why it matters: a maintainer looking for the tests of the Drucker-Prager law
has to grep through `review_v40_regressions.rs`, `review_v41_regressions.rs`
and others. File names do not say what the tests cover.

Proposed change: merge the review-named files into the subject files
(`materials_drucker_prager.rs`, `dof_manager_removal.rs`) or into the
binaries proposed in 2.3, and rename the few functions that carry such an
id. Do it together with 2.3, because the CI floors (`ci/floors.txt`) and
the test accounting of `ci/tools/analyze_tests.py` count test binaries.

Effort: medium (two to three days). Risk: low. Test bodies do not change.

### 2.3 One test binary per integration-test file

Evidence: 472 Rust files sit directly under `crates/*/tests/`, and cargo
builds each one as a separate binary that links the crate and its
dependency closure. A default `cargo test --workspace` builds 462 test
binaries (`docs/TESTING.md`, "State at release"): 433 of the 472 files
(the other 39 declare `required-features` that are off by default, such as
`mpi`, `cgal` or `petsc`) and the unit tests of the 27 library crates and
2 binaries (`cargo metadata --no-deps`). The largest counts of files:
`akantu-solid` 84, `akantu-solver` 38, `akantu` 38, `akantu-mesh` 27,
`akantu-cohesive` 25 (at the scan). `ci/github/README.md` and `AGENTS.md`
record 21 to 27 GB of cargo target for a workspace build, against 14 to 20
GB free on a standard runner.

Why it matters: link time and disk grow with the number of binaries. A
target this large cannot be cached on GitHub (10 GB per repository), so
every CI job builds from cold. Contributors with laptops hit the same
limit.

Proposed change: per crate, one `tests/it/main.rs` with `mod` entries for
the current files, keeping separate binaries only where a test needs its
own process: the MPI launches named in `scripts/mpi/`, `harness = false`
targets, and the tests that install process-wide hooks. Then update the
binary accounting in `ci/tools/analyze_tests.py` and `ci/floors.txt`.

Effort: medium (two days plus a full test run). Risk: medium. Tests that
rely on process-global state (MPI initialisation, the exit hook,
`catch_unwind` counters) can interfere once they share a process. Those
stay separate.

### 2.4 `#[allow]` attributes that suppress nothing

Evidence: 458 `#[allow(...)]` attributes. At the scan, 403 had a comment
on the same line or right above it and 55 had none (for example
`#![allow(dead_code)]` without a comment at the top of 18 shared
test-helper modules). About 219 of them name lints of clippy's `pedantic`
group (`cast_precision_loss` 125, `cast_possible_truncation` 44,
`too_many_lines` 22, `cast_sign_loss` 11, `cast_possible_wrap` 7, and 10
more for `float_cmp`, `many_single_char_names`, `similar_names` and
`unreadable_literal`). The workspace enables only `clippy::all`
(`Cargo.toml` `[workspace.lints.clippy]`) and does not enable `pedantic`,
so these attributes suppress nothing. `#[expect(...)]`, which reports an
unfulfilled expectation, is used 0 times; the MSRV (1.90) has it.

Why it matters: allows that do nothing hide which suppressions are real and
invite more copies.

Proposed change: delete the pedantic allows, or enable a chosen subset of
pedantic lints in the workspace table so that they mean something. Turn the
remaining allows into `#[expect(..., reason = "...")]` so that a stale one
fails clippy.

Effort: half a day with a script. Risk: low.

### 2.5 Lock in the panic-free library code

Evidence (at the scan): outside `#[cfg(test)]` modules, library code had 0
`unwrap()` calls. There were 10 `panic!` calls in library code, most in the
convenience constructors of `akantu-core` that panic on an allocation-size
error (`crates/akantu-core/src/array.rs`, `tensor.rs`) and one for a local
index above the 32-bit range (`types.rs`). Indexing that can panic was not
counted: the scanner cannot tell bounds-checked indices from
user-controlled ones.

Why it matters: the rule "no `panic!` or `unwrap()` on input a user
controls" (`AGENTS.md`, "Rules for numerical code") holds today only
because reviewers enforced it.

Proposed change: add `unwrap_used`, `expect_used` and `panic` as `warn` in
`[workspace.lints.clippy]`, allowed in test targets with `clippy.toml`'s
`allow-unwrap-in-tests` and `allow-expect-in-tests`. Give the remaining
core constructors a `try_` twin, or document their panics in a `# Panics`
section. Property-based robustness tests already feed mutated input to the
`.dat` parser (`crates/akantu-parser/tests/fuzz.rs`) and to the Gmsh reader
(`crates/akantu-io/tests/mesh_io_gmsh.rs`, `fuzzed_gmsh_files_never_panic`);
there are no coverage-guided `cargo fuzz` targets. Add them for the parser
and the mesh readers.

Effort: half a day for the lints, two days for fuzz targets. Risk: low.

### 2.6 Tests that depend on the machine

Evidence (`docs/TESTING.md`, `docs/PLATFORMS.md` and `docs/DECISIONS.md`):

| Test or group | Dependence | State |
|---|---|---|
| `akantu-geometry` `license_separation`, `akantu-embedded` `license_guard` | `cargo tree --offline` needs every package of `Cargo.lock` in the local registry | the CI tiers run `cargo fetch --locked` first; a contributor's `cargo test` still fails on a fresh machine without it |
| `akantu-validation` `probe_scripts` | needs `git` on the path, and fails in a git worktree mounted read-only in Docker (`docs/PLATFORMS.md`) | the project images have `git` |
| Python examples `dynamics`, `custom_material` | NumPy's `np.exp` kernel depends on AVX-512F | skipped without AVX-512F by an environment key |
| Bitwise fixtures recorded on aarch64 | the default NaN's sign differs on x86_64 | D-046 comparison rule |
| Oracle fixtures regenerated on x86_64 | Eigen blocking sized from the host's cache sizes | D-050 pins |
| Oracle records of MUMPS and PETSc solves | OpenBLAS kernels differ between x86_64 and aarch64 | compared only on the platform of the record (D-121) |
| `akantu-solid` `model_collective`, `akantu-comm` timing gates | 20 s collective timeouts and performance ratios under host stalls | changed to a count of timed-out waits and a calibrated ratio |
| `mpi_progress/delayed_sender_completes` under memcheck | deadlines under valgrind | longer deadline in that tier |

Why it matters: the fast tier should pass on any contributor's machine with
network access. Today two of these need set-up (a full registry and
`git`).

Proposed change: make the licence tests skip with a clear message when the
registry is incomplete, and run them as a required check in the `licence`
tier only.

Effort: half a day. Risk: low.

### 2.7 `unsafe` code: keep the ledger, explain it

Evidence: `scripts/unsafe_inventory.py --check --strict-safety` passes
("0 problems"). It lists 221 sites: 163 blocks, 20 `unsafe impl`, 18
`unsafe fn`, 11 `extern` blocks, 6 FFI functions, 3 attributes. Per crate:
`akantu-linalg` 90 (PETSc and MUMPS bindings), `akantu-comm` 89 (MPI),
`akantu-python` 17 (NumPy views), `akantu-mesh` 13 (Scotch),
`akantu-solid` 5, `akantu` 3, `akantu-capi` 2 and `akantu-parser` 2
(libm). 21 of 27 crates `#![forbid(unsafe_code)]` and 3 deny it.
`akantu-comm`, `akantu-linalg` and `akantu-mesh` have no crate-level
attribute and open `unsafe` per module. Each crate with `unsafe` keeps an
`UNSAFE.md` ledger and fingerprints. At the scan, a sample of 12 `//
SAFETY:` comments in the MPI, PETSc and NumPy-view code stated real
preconditions and nothing looked unsound. The sample is small; the FFI
code had an `unsafe` audit during the port, which found and fixed a memory
corruption in `petsc::Matrix::add_row`. Valgrind covers the MPI, MUMPS,
PETSc, Scotch and libm FFI (`memcheck` tier), Miri covers `akantu-python`
and the core array storage, and nothing runs CPython and NumPy under
valgrind (`ci/README.md`, tier `memcheck`). `mpi_backend.rs` registers its
exit hook with glibc's `on_exit`, which ties the MPI back-end to glibc.

Why it matters: a contributor who edits a line near an `unsafe` block
changes its fingerprint and fails the check without knowing why.

Proposed change: have the check's failure message say which command to run
(`--pin-missing` or `--refresh`), and add the crate-level
`#![deny(unsafe_code)]` with module-level allows to the three crates
without one.

Effort: two hours. Risk: none.

### 2.8 Scripts: maintenance tools and campaign tools

Done in the main. The remote-host tooling, the publishing scripts, the
audit application scripts, the mutation campaigns and the dated
regeneration records left the tree, and `analyze_tests.py`,
`known-failures.txt` and `feature-gates.txt` moved to `ci/tools/`. What
stays and is not needed to maintain the library: `scripts/perf/explicit-step/`
(the explicit-step timing harness), `benchmarks/cohesive-perf/` (upstream's
benchmark files) and `oracle/results/` (recorded oracle runs). They are
cited by `docs/BENCHMARKS.md` and `docs/NUMERICAL_VALIDATION.md` as
evidence.

### 2.9 Benchmark results inside a crate

Done. `crates/akantu-bench/results/` (3,176 files at the scan) left the
tree. `docs/benchmarks/` keeps 346 files: the summaries and records that
`docs/BENCHMARKS.md` cites.

### 2.10 Fixtures: many small files and one checksum file

Evidence: 73,938 tracked files, 241,836,216 bytes of content. On APFS they
occupy 467 MB because 65,494 files are 4 KiB or smaller (median 304 bytes,
90th percentile 5,712 bytes). 68,125 are `.npy` arrays, and there are 2,023
`manifest.json` cases. The largest subtrees are `parallel_cohesive` (20,573
files), `model` (18,173) and `parallel` (16,312). One file,
`fixtures/SHA256SUMS`, lists 73,889 checksums in 10,967,535 bytes. A
further 535 data files sit under `crates/*/tests/fixtures` and
`crates/*/tests/data`. Regeneration: each area has its generator under
`oracle/probes/<area>/generate.sh`, built against the pinned oracle image
(`docs/ORACLE_BUILD.md`); each generator writes to a scratch tree and
promotes after three identical runs (`docs/MAINTAINING.md`, "Adding or
regenerating fixtures"). x86_64 Linux is the authoritative platform
(D-043).

Why it matters: every new fixture edits the one checksum file, so two pull
requests that add fixtures always conflict. Tens of thousands of small files
slow clones, IDE indexing and file-system tools.

Options, judged by whether the evidence stays the same bytes with the same
checksums:

| Option | Files | Evidence | Cost |
|---|---|---|---|
| Per-case checksums: a `SHA256SUMS` in each case directory or the hashes in `manifest.json` | unchanged | unchanged | ends merge conflicts; small script change in the `fixtures` tier |
| Pack each case into one `.npz` (zip of the same `.npy` bytes, stored without compression or deflated) | about 2,200 archives in place of 68,125 arrays | same array bytes; checksums over the members | loader change in one place, `akantu-validation::fixture` (`read_npy`); generators write the archive |
| Separate repository as a git submodule pinned by commit | main repository loses 242 MB | pinned by commit hash | two repositories to keep in step; contributors must init the submodule |
| Git LFS | unchanged | pointers in git, content on LFS | 74,000 pointer files remain; GitHub LFS storage and bandwidth quotas apply to every CI clone; not recommended |
| Release asset with a checksum | main repository loses 242 MB | hash pinned in the repository | download step in every tier and every contributor set-up |

Proposed change: do per-case checksums first. Then pack each case, keeping
the fixtures in the repository. Revisit a submodule only if the repository
size becomes a problem.

Effort: per-case checksums half a day. Packing two to three days, plus a
full test run to show that every test reads the same values. Risk: medium
for packing (every fixture test passes through the new loader). Low for
checksums.

### 2.11 Dependency hygiene and supply chain

Evidence: workspace inheritance is used for package fields, lints (all 27
crates declare `[lints] workspace = true`) and most dependencies. The
exceptions: `sha2 = "0.10.9"` is declared directly in 7 crates (diffusion,
couplers, contact, linalg, solid, solver, validation), and at the scan
`serde`, `serde_json` or `tempfile` directly in `akantu-parser` and
`akantu-validation`. `Cargo.lock` has 183 packages. Duplicate versions
come from third-party crates: `equator` 0.2 and 0.6 and `thiserror` 1 and
2 through `faer` (`nano-gemm`, `gemm-common`, `sysctl`), plus `syn`,
`cpufeatures` and `shlex` (at the scan). None is declared twice by the
workspace itself. The toolchain is pinned to 1.99.1
(`rust-toolchain.toml`). The MSRV is `rust-version = "1.90"`, checked by
the `msrv` job, which passed on GitHub in 167 s. There is no `deny.toml`.
The licence policy is the project's own `ci/licence-policy.toml`. No tool
in `ci/`, `scripts/` or `.github/` checks RustSec advisories. All 27 crates
have `publish = false`, while the `packaging` workflow runs a crates.io
dry run.

Why it matters: a public library needs advisory alerts, and the release
question (crates.io or not) decides how versions and features are managed.

Proposed change: add `sha2` and the other direct entries to
`[workspace.dependencies]`. Add a weekly `cargo deny check advisories` job,
or enable Dependabot alerts. Decide whether the library crates will be
published.

Effort: two hours, plus the publishing decision. Risk: none.

## 3. Long-term refactoring

### 3.1 Large files and long functions

Evidence: the largest library files (lines, 15 November 2026):
`akantu-comm/src/communicator.rs` 4,247; `akantu-comm/src/mpi_backend.rs`
3,762; `akantu-solid/src/model.rs` 3,518; `akantu-cohesive/src/model.rs`
3,307; `akantu-mesh/src/mutation.rs` 3,035; `akantu-python/src/py/laws.rs`
2,917; `akantu-solver/src/dof_manager.rs` 2,869;
`akantu-python/src/py/physics.rs` 2,723;
`akantu-core/src/random/libm_tables.rs` 2,538 (data tables). The largest
test files are `akantu-cohesive/tests/cohesive_fixtures.rs` 4,835,
`akantu/tests/manual.rs` 3,220, and the shared parallel harnesses
`akantu-mesh/tests/par2a_common/mod.rs` 2,812 and `par_common/mod.rs`
2,613. At the scan about 110 library functions were longer than 100 lines.
Those over 200 lines:

| Function | Lines | Location |
|---|---:|---|
| `run` | 533 | `crates/akantu-bench/src/driver.rs:469` |
| `init` | 443 | `crates/akantu-python/src/py/physics.rs` |
| `consistent_friction_moduli` | 387 | `crates/akantu-contact/src/resolution.rs` |
| `make_periodic_distributed` | 304 | `crates/akantu-mesh/src/periodic.rs` |
| `mutate_inner` | 234 | `crates/akantu-mesh/src/mutation.rs` |
| `precompute` | 228 | `crates/akantu-igfem/src/engine.rs` |
| `exhaustive_spelling_table` | 200 | `crates/akantu-core/src/enums.rs` (a table) |

Why it matters: files over 3,000 lines are hard to review and hard for
agents to edit without reading all of them. Some long functions follow the
shape of the upstream C++ function on purpose, which helps line-by-line
comparison with Akantu.

Proposed change: split by concern where no upstream function is mirrored.
`communicator.rs` can be split into point-to-point, collectives and the
local-world back-end, `mpi_backend.rs` into the matching engine, lifecycle
and FFI wrappers, and the model files into their existing `model/`
submodules. Leave functions that mirror upstream as they are, and say so in
a comment.

Effort: one to two days per large file. Risk: low if done as pure moves,
one file per pull request.

### 3.2 Repeated model scaffolding and test harnesses

Evidence (at the scan): a 15-line window scan finds little duplication in
library code: 385 duplicated windows across files in 263,443 lines. Most of
it is model scaffolding repeated between physics: `akantu-diffusion` and
`akantu-solid` model files (75 windows, starting at the nodal-shape
checks), `akantu-solid` and `akantu-structural` model files (47), the
`model/services.rs` and `model/io.rs` of diffusion, solid, phase-field and
structural (12, 9, 8 windows). The Drucker-Prager and plastic laws share
28 windows. Tests repeat more: 678 windows, for example between
`cmat_friction_regressions.rs` and `review_v57_regressions.rs` (72), and
in the parallel harnesses of diffusion, solid, couplers and ntn-contact
(`par_*/mod.rs`, 12 to 31 windows each). Element types and materials are
already generic: element kernels go through one `ElementType` enum
(`crates/akantu-core/src/element_type.rs`) and the tables of `akantu-fe`.

Why it matters: a fix to a nodal accessor or a dumper registration has to
be repeated in four models.

Proposed change: a shared helper module in `akantu-model` (or a small
macro) for the nodal-field accessors and the dumper and service
registration. Merge the parallel test harnesses into one test-support
module in `akantu-validation`.

Effort: three to five days. Risk: low to medium. The bitwise replays catch
any numerical change.

### 3.3 Documents that are logs

Done in part. At the scan `docs/NUMERICAL_VALIDATION.md` had 16,248 lines,
`docs/TESTING.md` 8,191 lines in sections named after work packages and
`docs/DECISIONS.md` 3,260 lines. For the public tree they were rewritten
as topic documents (2,109, 536 and 2,099 lines), and `docs/PLATFORMS.md`
and `docs/MAINTAINING.md` were added. Still open: `docs/FEATURE_PARITY.md`
is 1.0 MB in 1,405 lines, so the average line has about 740 characters and
the longest about 5,800, and one changed cell shows up as a whole-line
diff.

Proposed change: generate `FEATURE_PARITY.md` from a structured table, one
TOML or TSV record per row, so that diffs show the changed field.

Effort: three to five days. Risk: low. The parity tests read the matrix,
so the generator must keep its format.

### 3.4 Feature combinations

Evidence: 48 features in 16 crates. The `features` tier builds the
workspace default, all-features and no-default, and for each member each
feature alone, the default and all (`ci/README.md`,
`ci/tools/feature_matrix.py`). It does not test pairwise combinations of
the facade's 16 features, such as `cohesive` with `mpi` but without
`scotch`. The tier needs four project images (MPI, Scotch, MUMPS, PETSc).

Why it matters: a combination that does not build is found by a user.

Proposed change: add `cargo hack --feature-powerset --depth 2` on the
facade crate to the weekly run.

Effort: two hours plus the run time. Risk: none.

## The first actions

The ten actions of the first version, with their state on 15 November 2026:

| Rank | Action | Section | Effort | Risk | State |
|---:|---|---|---|---|---|
| 1 | Move process history out of the public tree | 1.3 | 1 day | medium | done |
| 2 | Replace `AGENTS.md` with contributor and agent instructions | 1.1 | 0.5 day | low | done |
| 3 | Write `CONTRIBUTING.md` and `SECURITY.md`, and the procedures for materials, elements and fixtures | 1.4 | 1 to 2 days | none | done |
| 4 | Remove personal paths; make `AKANTU_SRC_HOST` required in the generators | 1.2 | 2 hours | low | done |
| 5 | Choose the pull-request CI set, add a registry cache and parallel pytest | 1.7 | 1 day | low | CI set done; cache and pytest open |
| 6 | Retarget the document-guard tests; move the test analyser and the known-failures list into `ci/` | 1.5, 2.8 | 1 day | low | done |
| 7 | Per-case fixture checksums in place of the single 11 MB `SHA256SUMS` | 2.10 | 0.5 day | low | open |
| 8 | Rewrite internal ids in comments and messages; block new ones in CI | 2.1 | large, splittable | low | rewrite done; CI check open |
| 9 | Consolidate the 472 test binaries and rename the 66 review-named test files | 2.3, 2.2 | 3 to 5 days | medium | open |
| 10 | Clean the `#[allow]` attributes, switch to `#[expect]`, add the panic lints and an advisory check | 2.4, 2.5, 2.11 | 1 day | low | open |

The open items, in the order to do them: 7, then 10, then 9 together with
2.2, then 1.8 and 3.3.
