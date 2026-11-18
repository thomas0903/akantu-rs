# Testing

This guide is for anyone who changes the port: how the tests are organised in
tiers, how to run each tier, how the differential tests against the C++ oracle
work, how to add a test or a fixture, and which tests depend on the machine.
The comparison method and every tolerance are in
`docs/NUMERICAL_VALIDATION.md`; how to build the C++ oracle and regenerate
fixtures is in `docs/ORACLE_BUILD.md`; the CI that runs these tiers is
described in `ci/README.md` and `ci/github/README.md`.

## The tiers

Every tier is a script under `ci/tiers/`, run through one entry point,
`ci/run-all.sh`. The same command runs on a developer machine, on any Linux
host with Docker and in the GitHub workflows. Times are wall times of the final
release check on an 8-vCPU x86_64 Linux machine (AMD, 16 GB), including loading
the project images, unless the cell says otherwise.

| Tier (preset) | What it covers | Runs in | Time | When |
|---|---|---|---|---|
| crate tests | `cargo test -p <crate>`: unit tests, ported upstream tests and the differential tests of one crate on the committed fixtures | plain cargo, Linux or macOS | seconds to a few minutes per crate | while you work |
| `lint` (fast) | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `bash -n` and `shellcheck -x` of `ci/` and `scripts/`; `ruff check python/` | MPI image (cargo), host (shellcheck, ruff) | part of `fast` | every change |
| `test` (fast) | `cargo test --workspace --locked --no-fail-fast` with every test binary accounted for, the known-failures baseline and a test-count floor; then `exec_threads.rs` and the fixture tests of `akantu-solid` again with `AKANTU_NUM_THREADS=3 AKANTU_EXEC_GRAIN=finest` (step `threads`, D-123; about 35 s of test time on 4 cores) | MPI image | `fast` preset: 1,064 s | every pull request |
| `fixtures`, `parity`, `licence` (fast) | checksums of every `fixtures/**/SHA256SUMS`; the generated blocks of `docs/FEATURE_PARITY.md` and `README.md`; third-party licences and the LGPL/GPL separation (D-042) | host | 12 s, 1 s, 14 s on an Apple M4 Pro | every pull request |
| `exportall` | the `test` tier built with the test-only feature `akantu-core/export-all` | MPI image | 773 s on 4 vCPU | weekly; changes to features or `unsafe` code; before a release |
| `features` (features) | `cargo check` of every feature alone, none, default and all, per member, with `-D warnings`; clippy and rustdoc with `--all-features` | MUMPS image, and PETSc image for the `petsc` checks | 424 s | weekly; changes to features; before a release |
| `gates` (features) | every command of `ci/tools/feature-gates.txt`: feature-gated clippy, test and doc lines, release-only tests, the convergence studies | MPI image | 1,208 s | weekly; changes to feature-gated code; before a release |
| `msrv` (features) | `cargo check` with the toolchain of `workspace.package.rust-version` (1.90) | host, rustup | 134 s | every pull request |
| `release` (medium) | optimised-build runs: cohesive (all cases), linalg, solid 3-D implicit dynamics, core and parser, dumpers, the facade crate with the examples' whole runs; `akantu-solid` and `akantu-solver` with `AKANTU_NUM_THREADS=3 AKANTU_EXEC_GRAIN=finest` (run `threads`, D-123; not in the 693 s) | MPI image | 693 s | weekly; before a release |
| `mpi` (medium) | real-MPI scenarios of `akantu-comm` at 1, 2 and 4 ranks | MPI image | 341 s | weekly; before a release |
| `par` (medium) | distributed models on real MPI with Scotch at 1, 2 and 4 ranks, including the oracle replays at their recorded rank counts | Scotch image | 365 s | weekly; before a release |
| `mumps` (medium) | the MUMPS back-end at 1, 2 and 4 ranks and three thread levels | MUMPS image | 429 s | weekly; before a release |
| `petsc` (medium) | the PETSc back-end at 1, 2 and 4 ranks and two thread levels | PETSc image | 371 s | weekly; before a release |
| `examples` (medium) | the parallel examples `parallel_2d` and `phase_field_parallel` at 2 ranks | MPI image | 119 s | weekly; before a release |
| `wheel`, `pydocs` (python) | maturin abi3 wheel, `import akantu`, `pytest python/tests` with and without greenlet, pdoc HTML | host Python | 3,170 s | weekly; every push to `main` (in the documentation site's build); changes to `python/` |
| `package`, `minver`, `version` (packaging) | `cargo package --list` per member and a downstream consumer; direct minimal versions; tag = `workspace.package.version` | MPI image, host nightly | 149 s (crates); 3,255 s (manylinux wheel) | tags, weekly |
| `docs` | `cargo doc --workspace --no-deps` with `RUSTDOCFLAGS=-D warnings` | backend (host in the workflow) | 53 s | every pull request |
| `memcheck` | valgrind memcheck of the FFI `unsafe` code (`CI_MEMCHECK_KIND` = `mpi`, `mumps`, `petsc`) | project images plus valgrind | 501 s, 331 s, 157 s | weekly, before a release |
| `miri` | Miri on `akantu-python` and the `akantu-core` array storage, Stacked and Tree Borrows | host, pinned nightly | 177 s | changes to `unsafe` code, before a release |
| `expensive` | the `mpi` and `par` tiers at 8 ranks (oversubscribed) | MPI and Scotch images | 637 s | manual, before a release |
| `coverage` | `cargo llvm-cov` over the workspace tests; a report, no threshold | MPI image | 2,270 s | weekly |
| `distro` | `features` and `test` on a digest-pinned `ubuntu:22.04` with its own Open MPI 4.1 and Scotch 6; diagnostic | Docker container, host backend inside | 1,407 s | weekly |

`ci/run-all.sh --list` prints every tier and preset. The presets are
`fast` (lint test fixtures parity licence, the default), `features` (features
msrv gates), `medium` (mpi par mumps release examples petsc exportall),
`packaging` (package wheel minver version), `python` (wheel pydocs), `docs`,
`miri`, `memcheck`, `coverage`, `distro` (features test on the host),
`expensive`, and `all` (everything except miri, memcheck, coverage, distro and
expensive).

Every pull request runs the `fast` preset, `msrv`, `docs`, the manual's
build and `ci/selftest.sh` (`ci/github/README.md`). The `features` preset, `exportall`, `medium`,
`python`, `packaging`, `memcheck`, `coverage` and `distro` run weekly; run the
ones a change touches before asking for its merge (`features` and `exportall`
for a change to cargo features, feature-gated code or `unsafe` code; `medium`
for parallel or solver code; `python` for the Python package). Before a
release, run all of them, plus `miri` and `expensive` (`docs/MAINTAINING.md`).

## Running the tiers

### On a Linux machine with Docker

Requirements: Docker, git, rustup (it installs the toolchain of
`rust-toolchain.toml`, 1.99.1, with rustfmt and clippy), Python 3.9 or newer,
`jq`, `shellcheck` (Ubuntu package) and `ruff` 0.17.3 (PyPI) for the lint tier.
The heavy tiers need about 8 GB of RAM and 60 GB of free disk: the four project
images are 3.7 to 4.4 GB each and the cargo target volume of a workspace build
reached 21 to 27 GB. The `miri`, `minver` and `coverage` tiers install their
toolchains and tools when `CI_INSTALL_TOOLCHAINS=1` is set; otherwise a missing
tool makes the tier incomplete.

Build the images once (each `FROM` the MPI image):

```sh
ci/github/images.sh ~/akantu-images mpi scotch mumps petsc   # plain docker build, cached as a tar
```

or one at a time:

```sh
docker build -t akantu-rs-mpi:dev    docker/mpi
docker build -t akantu-rs-scotch:dev docker/scotch
docker build -t akantu-rs-mumps:dev  docker/mumps
docker build -t akantu-rs-petsc:dev  docker/petsc
```

The memcheck tier builds `akantu-rs-memcheck-<kind>:dev` from
`docker/memcheck/Dockerfile` itself. `scripts/mpi/build-image.sh` and
`scripts/petsc/build-image.sh` build the same images and also rewrite the
committed records (`docker/<image>/packages.tsv`, `image-id.txt`, and for PETSc
its configuration and BLAS alternatives): use them only when you mean to update
those records.

Then run the tiers:

```sh
ci/run-all.sh --backend docker                       # preset fast
ci/run-all.sh --backend docker --preset features
ci/run-all.sh --backend docker --tiers exportall
ci/run-all.sh --backend docker --preset medium
ci/run-all.sh --backend docker --preset expensive
CI_MEMCHECK_KIND=mumps ci/run-all.sh --tiers memcheck
CI_INSTALL_TOOLCHAINS=1 ci/run-all.sh --tiers miri
ci/run-all.sh --tiers fixtures,parity                # light, host only
ci/run-all.sh --preset fast --dry-run                # records the commands, runs nothing
```

Options: `--preset P` or `--tiers a,b`, `--backend host|docker`, `--out DIR`
(default `target/ci/<UTC stamp>`), `--image IMG`, `--target-name NAME`,
`--step-timeout S`, `--label TEXT`, `--dry-run`, and arguments after `--` that go
to the `mpi`, `par`, `mumps` and `expensive` tiers (for example
`-- --ranks "1 2 4 8" --repeat 3`).

`--backend docker` runs cargo inside the MPI image through
`scripts/mpi/in-docker.sh`: the repository is mounted read-only at `/src`,
cargo's target directory is the volume `akantu-rs-mpi-target` under
`/target/<target-name>` and the registry the volume `akantu-rs-mpi-cargo`. This
is the authoritative configuration for bitwise results (D-043). The MPI, par,
MUMPS, PETSc and memcheck tiers always use Docker, whatever the backend.

Verdicts (`ci/lib.sh`): a step passes on exit 0 only. A step that cannot run
(missing tool, image, toolchain or native library) is a skip, and its tier is
`incomplete`, never `pass`. A step that does not apply to the tree is `na`. The
last line is `CI: PASS` (exit 0), `CI: FAIL (tiers)` (1) or
`CI: INCOMPLETE (tiers)` (3); 2 is a usage error. The output directory holds
`provenance.txt` (git head, tree, dirty count, host, toolchains, image ids),
one directory per tier with `steps.tsv`, `commands.tsv` and one log per step,
and `summary.tsv` / `summary.json`.

The real-MPI and back-end scripts can also be run directly:

| What | Command |
|---|---|
| `akantu-comm` MPI tier, 1/2/4 ranks | `scripts/mpi/run-tests.sh [--ranks "8"] [--repeat 3] [--skip-lint]` |
| distributed models, Scotch | `AKANTU_RS_MPI_IMAGE=akantu-rs-scotch:dev scripts/mpi/run-par-tests.sh --scotch [--ranks "1 2 4"] [--repeat N]` |
| MUMPS | `scripts/mumps/run-mumps-tests.sh [--ranks "1 2 4"] [--repeat N] [--skip-lint]` |
| PETSc | `scripts/petsc/run-petsc-tests.sh [--ranks "1 2 4"] [--levels "funneled serialized"] [--skip-lint]` |
| any command in the MPI image | `scripts/mpi/in-docker.sh cargo test -p akantu-comm --features mpi --locked` |
| one MPI binary by hand | `scripts/mpi/in-docker.sh bash -c 'mpirun -np 4 "$(cargo test -p akantu-comm --features mpi --no-run --message-format=json 2>/dev/null \| jq -r "select(.target.name==\"mpi_acceptance\" and .executable!=null).executable")"'` |
| evidence checker self-test | `python3 scripts/mpi/test_check_evidence.py` |

Logs go to the target volume (`/target/mpi-logs/<stamp>/`, `par-logs`,
`mumps-logs`, `petsc` logs) with a `provenance.txt` and a `results.tsv`. Pick
the test binary of a cargo JSON message by its target name: packages with a
`[[bin]]` also report the binary, in no fixed order.

### Container limits and runner variables

`scripts/mpi/in-docker.sh` caps each container at 4 CPUs and 6 GiB without swap
(`--memory 6g --memory-swap 6g --cpus 4`, cargo `-j4`), runs as a non-root user
(Open MPI refuses root) with `OMPI_MCA_btl=tcp,self,sm`, oversubscription
allowed and no binding, builds without incremental compilation and with
line-table debug information. Libtest uses its default thread count. A test
binary killed by the out-of-memory killer prints no result line and fails the
tier by name; a test that needs more than 6 GiB at default threads is a defect.

| Variable | Effect |
|---|---|
| `AKANTU_RS_DOCKER_CPUS`, `AKANTU_RS_DOCKER_MEMORY` | override the caps (2 and `6g` on a 2-vCPU runner; 8 and `12g` to investigate on a larger machine) |
| `AKANTU_RS_MPI_TARGET` | target directory inside the volume (`/target/<name>`); give each checkout its own and never share one between checkouts |
| `AKANTU_RS_MPI_IMAGE`, `AKANTU_RS_SCOTCH_IMAGE` | image of the MPI scripts and of the `par` / `expensive` tiers |
| `AKANTU_RS_CARGO_INCREMENTAL=1`, `AKANTU_RS_DEV_DEBUG=2` | restore cargo's defaults for a debugger session |
| `CI_STEP_TIMEOUT` | seconds per step (default 3600) |
| `CI_INSTALL_TOOLCHAINS=1` | let `miri`, `minver`, `msrv` and `coverage` install their toolchains and tools |
| `CI_FEATURES_IMAGE`, `CI_FEATURES_PETSC_IMAGE` (`off` disables the PETSc leg), `CI_NATIVE_NA` | images of the `features` tier; native features declared not applicable (the `distro` tier sets `mumps,petsc`) |
| `CI_RELEASE_ONLY=facade` | run only the facade's release run of the `release` tier |
| `CI_WHEEL_PYTHON`, `CI_WHEEL_COMPAT` (`linux`, `manylinux_2_28`, `off` on macOS), `CI_WHEEL_SMOKE` | interpreter, wheel tag and extra CPython versions of the `wheel` tier |
| `CI_VERSION_TAG` | tag checked by the `version` tier (else `GITHUB_REF_NAME`) |
| `CI_MIRI_NIGHTLY` (default `nightly-2026-10-01`), `CI_NIGHTLY` (`minver`, default `nightly-2025-03-01`), `CI_LLVM_COV_VERSION` (default 0.9.1) | pinned tool versions |

### On macOS

Crate tests run natively on macOS arm64 with plain `cargo test -p <crate>`.
The MPI, MUMPS, PETSc and Scotch features need the Linux images; the default
build never compiles or links them. The tiers `fixtures`, `parity` and
`licence` and `ci/selftest.sh` run on macOS with the host backend; the Docker
tiers run through any Linux VM that provides Docker. Bitwise parity is claimed for x86_64 Linux in the project image;
`docs/PLATFORMS.md` lists what holds on macOS and aarch64 Linux.

## Environment variables of the tests

| Variable | Read by | Effect |
|---|---|---|
| `AKANTU_FIXTURES_DIR` | `akantu-validation` | fixtures root (default `<workspace>/fixtures`) |
| `AKANTU_FIXTURE_ENV` | `akantu-validation` | numerical environment whose golden values are loaded for cases with platform variants (below); for diagnostics only |
| `OPENBLAS_CORETYPE` | `akantu-validation`, MUMPS / PETSc tests, oracle | part of the environment id on x86_64 (`x86_64-haswell` with it unset or `Haswell`); the MUMPS and PETSc images set `Haswell` and one OpenBLAS thread on amd64, as the oracle does (D-050) |
| `AKANTU_VALIDATION_OUT` | `akantu-validation` | failure artefacts (default `<workspace>/target/akantu-validation/<test>/`) |
| `AKANTU_VALIDATION_WRITE_ALL=1` | `akantu-validation` | also write reports for passing tests |
| `AKANTU_VALIDATION_SKIP_PYTHON=1` | `akantu-validation` | skip, explicitly, the tests that need `python3` and `bash` (`probe_scripts`, `unsafe_inventory`) |
| `COH_ALL=1`, `COH_CASES=<id>,…`, `COH_FIXTURE_ROOT=<dir>` | `akantu-cohesive` | replay all cohesive cases in a debug build (the 3-D cases and the adversarial explicit cases A and B are skipped there by default, announced on stderr); restrict the cases; read staged cases |
| `PERF_ALL=1` | `akantu-bench` | the benchmark crate's replays of every case |
| `AKANTU_RNG_LIBM_LONG=1` | `akantu-core` | the long libm run (14 domains × 2²² inputs against committed glibc hashes): `cargo test -p akantu-core --release --test random_libm` |
| `AKA_SEED`, `SCOTCH_PTHREAD_NUMBER=1` | MPI tests, oracle | fixed seed and one Scotch thread make partitions repeatable (the images set the latter) |
| `AKANTU_MPI_TEST_TIMEOUT_S`, `AKANTU_MPI_WATCHDOG_S` | MPI binaries | deadline of multi-rank waits (default 20 s); in-process watchdog |
| `AKANTU_COLLECTIVE_CHECKS` | `akantu-comm` (every communicator family) | `every-phase` or `commit-points` (default): where the collective error protocol agrees (D-126); the `test` and `par` tiers run once in each mode; `scripts/mpi/in-docker.sh` passes it into the container |
| `AKANTU_COLLECTIVE_HEADER` | `akantu-comm` (MPI) | `auto` (default), `allgather`, `gather-broadcast`: how the header of every checked collective is exchanged (D-126) |
| `AKANTU_PERF_FACTOR` | MPI tier | limit of the MPI performance gate (default 8) |
| `AKANTU_MUMPS_THREAD_LEVEL`, `AKANTU_MUMPS_ONLY`, `AKANTU_MUMPS_ORACLE=off`, `AKANTU_MUMPS_WATCHDOG_S` | `mumps_mpi` | thread level, scenario filter, disable the fixture check, watchdog |
| `AKANTU_PETSC_THREAD_LEVEL`, `AKANTU_PETSC_ONLY` | `petsc_mpi` | thread level, scenario filter |
| `AKANTU_DOC_GUARD_ROOT` | `doc_integrity_guard` | read the guarded documents from another directory |

The Python example comparisons depend on the Python environment of the oracle
runs. `ORACLE_ENVIRONMENT` in `python/examples/cases.py` records it: CPython
3.13.5, NumPy 2.2.4, SciPy 1.15.3, matplotlib present, `numpy_size1_to_scalar`
true, `platform` = `x86_64-linux-glibc`, `numpy_avx512f` true. Each case names
the keys its result depends on (`env_skip`: the case skips with the reason;
`env_ignore`: one variable is left out). Elsewhere the case is compared
bitwise.

| Key | Why it exists |
|---|---|
| `platform` | NumPy's transcendental and BLAS-backed kernels give other last bits on other platforms (arm64 macOS: up to 3 ulp). `dynamics` and `custom_material` compute their initial pulse with NumPy alone; `phasefield_static*` ignore two printed error norms. |
| `numpy_avx512f` | NumPy 2.2.4 runs float64 `np.exp` through an AVX-512F kernel exactly when the CPU has AVX-512F. The oracle host had it. Without it the pulse of `dynamics` and `custom_material` differs in 149 of 4,221 values by up to 2 ulp, so these two cases skip on such a CPU and are compared bitwise on an AVX-512F CPU. NumPy can only switch features off, so no setting reproduces the AVX-512F bits on a CPU without it. |
| `numpy`, `scipy`, `matplotlib` | `eigen_modes*` use SciPy's `eigsh` and an image saver: compared only with the oracle's versions. A PyPI SciPy 1.15.3 differs from Debian's build by 2.8e-14 on the eigenmodes. |
| `numpy_size1_to_scalar` | `solver_callback*` call `int()` on one-element arrays, which NumPy 2.5 refuses. |

`ci/pytest-skips.txt` lists the expected skips of the `wheel` tier's two pytest
runs (`run | path | reason | count`); a line `run@<platform key>` applies on
that platform only. A skip that is not listed, or more of it than listed, fails
the tier. The tier's `environment` step (`ci/tools/python_env.py`) prints the
CPU, NumPy's features and these keys, so a run's log says which comparisons
ran.

## Differential tests

A differential test runs the port on the inputs of an oracle run and compares
its results with the oracle's. Most fixtures live in `fixtures/` (committed,
at most 1 MiB per case), one directory per case: `manifest.json` (provenance,
parameters, the list of quantities with dtype, shape, axes and SHA-256),
`data/*.npy`, `inputs/`. An area directory holds `tolerances.json`, one rule per
quantity pattern with its justification and evidence. Some areas keep their
oracle records inside a crate (`crates/*/tests/fixtures/`, for example the RNG,
parser and dense records) or as upstream `.verified` files.

A test finds its case by id, relative to the fixtures root:

```rust
use akantu_validation::DiffTest;

let mut t = DiffTest::new("solid::patch_tri3", "model/smm/patch_triangle_3").unwrap();
let u: Vec<f64> = model.displacement().to_vec();      // computed by the port
t.check_f64("displacement", &u);                       // rule from tolerances.json
t.assert_pass();                                       // panics with a report on failure
```

`Fixture::open_by_id(id)` gives direct access to the arrays. The harness
verifies each file's checksum before it reads it. A case with platform variants
(`variants/<environment>/`) is opened for the running environment:
`AKANTU_FIXTURE_ENV` when set, else `x86_64-haswell` on x86_64 (with
`OPENBLAS_CORETYPE` unset or `Haswell`) and `aarch64-auto` on aarch64. Any
other environment makes those cases fail to open (`Error::FixtureEnvironment`);
cases without variants are the same golden everywhere. Nine cases have
variants (MUMPS-solved states, class X2 of `docs/NUMERICAL_VALIDATION.md`).
`AKANTU_FIXTURE_ENV=aarch64-auto cargo test …` on x86_64 compares against the
other platform's values as a diagnostic.

Bitwise means the `f64` bit patterns are identical. The one exception (D-046)
is a default quiet NaN that differs only in its sign bit (aarch64 produces
`0x7ff8000000000000`, x86_64 `0xfff8000000000000`); NaNs with a payload, ±0 and
infinities stay bitwise. Most cases are compared bitwise; the others name a
rule (ULP, relative, norm-wise, a reordering of the oracle's numbering) whose
reason and evidence are in `tolerances.json` and in the generated
`docs/TOLERANCE_LEDGER.md`.

A test passes only with evidence: at least one compared quantity (or a solver
acceptance check), and for an error-path fixture the error the port observed
(`DiffTest::observe_error`). A missing tolerance is a test error, never a
silent default. When a test checks one of `newton/nb_iterations`,
`newton/error`, `newton/residual_norm`, the report also checks the port's own
values against the stopping rule recorded in the fixture, and fails unless all
three were checked, agree and satisfy the rule at every step. External (large)
fixtures that are not materialised fail with the command that generates them;
the fixture-integrity test skips them, so a passing fast tier does not cover
them.

When a comparison fails, the harness writes `report.txt`, `report.json`, the
port's arrays as a fixture (`actual/`) and a pending regression case
(`regression/`) under `AKANTU_VALIDATION_OUT`. Start from the report: it names
every failing quantity with its largest difference. Then decide, following the
method of `docs/NUMERICAL_VALIDATION.md`, whether the port or the oracle is
wrong. A confirmed upstream defect gets a UD id in `docs/UPSTREAM_DEFECTS.md`
and, where the port corrects it, a compatibility switch that the test sets to
replay upstream's behaviour (D-003). To compare two fixture trees (a
regeneration, another platform): `cargo run -p akantu-validation --bin
fixture-diff -- <root_a> <root_b> [--json] [--case <glob>]`. To regenerate a
case: `docs/ORACLE_BUILD.md`, "Probes and fixture generation".

## Test accounting, the known-failures list and the floors

`cargo test` exits 0 even when a test binary never ran. The `test`,
`exportall`, `gates` and `release` tiers therefore judge a run with
`ci/tools/analyze_tests.py`:

- every test binary of `cargo test --no-run --message-format=json` must have
  run and printed exactly one `test result:` line (targets with
  `harness = false` are judged by their exit code); a binary killed by a
  signal, aborted, timed out or ending early, even with status 0, counts as a
  failed target, by name;
- every library with doc-tests must have a `Doc-tests` section;
- the failing names of a binary must agree with its result line, and cargo's
  list of failed targets with the derived one;
- the set of failing tests must equal `ci/tools/known-failures.txt` exactly.
  A listed failure that now passes gives `IMPROVED` (remove the line in the
  same change); anything else fails.

`ci/tools/known-failures.txt` is the only waiver. Each line is
`package | cargo target | test | reason` and needs an open, documented problem;
it changes only through a reviewed change. At release it is empty.

`ci/floors.txt` sets, per run, the minimum number of result lines and passed
tests (`ci/tools/count_tests.py`). A run below a floor fails: a vanished test
binary, an empty workspace and a filter that matches nothing all exit 0 for
cargo. Lowering a floor is a reviewed change; raise it after adding tests.

Guards on the documents run with the `test` tier, in `akantu-validation`:
`doc_integrity_guard` reads every Markdown file of the repository outside
`fixtures/` and checks two things only, that code fences are balanced and
that no merge-conflict marker is left (it no longer checks duplicate
headings); `public_tree_guard` refuses home-directory paths and broken
links between documents; `tolerance_ledger` checks that the generated
ledger is current, and `parity_scripts` runs the self-tests of the parity
scripts (the `parity` tier checks the generated blocks). The manual has
its own tests (`cargo test -p akantu --test manual`, the `manual_claims*`
tests and `cargo test -p akantu --doc`).

## Adding tests and fixtures

Some test files and functions carry the number of a review round of the port
in their name (`review_v66_regressions`, `review_v54_native`,
`review_v103_allocations`, `mi03_…`). These names are historical: the parity
matrix and the defect register cite them, so they were kept. Name a new test
after what it checks.

Rules that hold for every change:

- Never loosen a tolerance to make a test pass. A new or changed rule needs a
  written justification and evidence in its `tolerances.json`, which the
  tolerance ledger (`docs/TOLERANCE_LEDGER.md`, generated by
  `scripts/gold/tolerance_ledger.py`) collects, and an independent review.
- Never `#[ignore]` a failing test to hide it. A test that is ignored for a
  reason (a debug-only cost, an optional tool) prints the reason; the CI counts
  ignored tests (25 at release).
- A known failure goes into `ci/tools/known-failures.txt` with its reason, or is
  fixed.
- Tests behind a cargo feature that no member enables by default (`cgal`,
  `igfem`, `python`, `mpi`, release-only tests) are invisible to
  `cargo test --workspace`. Add a line to `ci/tools/feature-gates.txt` in the
  same change (plain `cargo …` with `--locked`; `cargo test` lines with
  `--no-fail-fast`), a floor `gates-N` in `ci/floors.txt` for a `cargo test`
  line, and its row in the gate map of `ci/github/README.md`.
- In a multi-rank test, agree every rank-local check (an all-reduced failure
  flag) before the next collective call, and defer checks on data that only
  rank 0 holds to the end of the scenario: a rank that leaves a scenario early
  while its peers enter the next collective hangs them.
- New `unsafe` code updates the crate's `UNSAFE.md` in the same commit; the
  guard `cargo test -p akantu-validation --test unsafe_inventory` checks that
  every site has its row and its adjacent `SAFETY:` comment
  (`python3 scripts/unsafe_inventory.py --check --strict-safety`;
  `--update-lines` after an edit that only moved code; `--skeleton <crate>` for
  a new file).

An upstream test port keeps the intent and the criteria of the original, is
named after it, and gets its row in `docs/FEATURE_PARITY.md` (test name,
fixture, tolerance). Add independent checks where they exist (patch tests,
partition of unity, rigid-body modes, analytical solutions, convergence
rates).

A new fixture case comes from a probe (`docs/ORACLE_BUILD.md`). Generate it on
the pinned x86_64 oracle, check that two or three runs are identical (the
generators refuse otherwise), commit the case with its `SHA256SUMS` lines and,
if the area has none yet, a `tolerances.json` rule. A case that contains NaN
documents which quantities, from which step and why, in its manifest and in
`docs/NUMERICAL_VALIDATION.md`.

### Upstream golden files and mesh data

`crates/akantu-validation/tests/data/upstream_data_manifest.json` accounts for
the 438 upstream data files (meshes, `.geo`, `.dat`, golden files): which are
copied, which frozen meshes replace build-time Gmsh output, which ported tests
need them. The test `cargo test -p akantu-validation --test
upstream_data_manifest` (needs `git`) re-derives everything that depends on this
repository and fails when the manifest is stale. A change that adds, removes
or changes a copy of an upstream file (crate test data, fixture `inputs/`),
changes `fixtures/meshes/frozen_meshes.json` or changes the status of a TEST,
EX or PY row regenerates it in the same commit:

```sh
python3 scripts/gold/upstream_data_manifest.py            # writes the JSON; exit 1 on an owed file
python3 scripts/gold/upstream_data_manifest.py --check    # exit 1 and the differences when stale
```

It needs the reference checkout (`--reference DIR` or `AKANTU_SRC_HOST`). A new
file under a `crates/*/tests/data/upstream/` tree also needs its row (file,
upstream path, SHA-256) in that tree's `README.md`. An upstream file that a
ported test needs but that is deliberately not copied gets an entry in the
generator's `NOTES`. The golden files produced by upstream's own drivers on the
oracle are generated with `oracle/probes/gold/generate.sh --out <dir>` (each
driver run three times, staged only if identical).

## Platform notes

These are the differences a tester meets. `docs/NUMERICAL_VALIDATION.md`
classifies the x86_64/aarch64 differences of the corpus (classes X1 to X8);
`docs/PLATFORMS.md` describes the platforms.

- NaN signs: handled by D-046 (above). A golden case that contains NaN
  documents it.
- libm: the port calls the platform's libm where upstream does
  (`akantu-parser/src/cmath.rs`). On Linux with glibc 2.41 the parser oracle
  test has no exception; on macOS it accepts exactly 18 enumerated Apple-libm
  last-bit differences (`LIBM_EXCEPTIONS` in
  `crates/akantu-parser/tests/oracle.rs`). Tests whose values depend on libm
  compare them on x86_64 glibc only.
- FMA: the port uses no fused multiply-add and no fast-math flags; the golden
  data come from the `strict` oracle build, which disables contraction and
  Eigen's vectorisation (D-031). Residual fused arithmetic in the oracle comes
  from system libraries (OpenBLAS, MUMPS, PETSc), which is why MUMPS-solved
  states are platform variants.
- Eigen's cache sizes: Eigen blocks its dense and `SparseLU` kernels by the
  cache sizes, which it queries from the CPU on x86_64. The oracle is pinned
  to 16 KiB / 512 KiB / 512 KiB and the port's Eigen translation reproduces
  that blocking on every host.
- Gmsh: the same `.geo` gives different meshes on aarch64 and x86_64 (D-009).
  Tests read the frozen meshes of `fixtures/meshes/gmsh-4.13.1/`; nothing calls
  Gmsh at test time.
- NumPy kernels: see `platform` and `numpy_avx512f` above.
- Open MPI: the image's Open MPI 5.0.7 overruns receive buffers shorter than
  the message, so the port receives every message at its full length and
  reports a truncation afterwards; PRRTE's launcher can delay or hang its
  return after several ranks exit with non-zero status
  (`docs/DEPENDENCIES.md`).

Tests that depend on the machine, and what was measured:

| Test | Behaviour | Measured |
|---|---|---|
| MPI performance gate (`mpi_bench calibrated`, 3 rows) | median over 9 interleaved blocks of the backend ratio `(akantu − calibration) / raw` ≤ 8; the plain ratio `akantu / raw` is reported in `perf.tsv`, not gated, because it moves with the allocator's state | healthy jobs at most 3.45 under load (0.08 the smallest); the mildest broken-pacing mutant 13. The plain 16 MiB ratio exceeded 8 in 15 of 30 idle jobs |
| abort rows of the MPI tier | the 30 s bound applies to the end of the last rank process (`scripts/mpi/abort_watch.py`); the launcher's exit status is required and its teardown time is recorded in `launch-times.tsv`, not bounded | the launcher returned within 2.81 s in 3,880 runs on 4 vCPU, idle and loaded; before the `PMIx_Abort` request, launchers hung in 9 of 2,000 and 3 of 1,000 runs at 8 ranks on 8 vCPU; once a launcher returned the right code 71 after 100.5 s |
| `mpi_progress` timed rounds | crossed rounds < 4 s with a 5 s operation deadline | 1.000 to 1.086 s |
| `akantu-solid` `model_collective` | requires that no wait ended at the local world's 20 s timeout (counted per rank) | a stall of the whole process longer than 20 s still times out collectives |
| `memcheck` `mpi` leg, `mpi_progress/delayed_sender_completes` and `serialized_progress` | valgrind slows the 16 MiB payload work; the tier sets a 60 s deadline and a 300 s watchdog for the first scenario | passes at a 0.5-CPU cap; the margin inside the 20 s deadlines of `serialized_progress` is not measured (28 s whole scenario at 0.5 CPU, 32 s on a 2-vCPU runner) |
| `mumps_mpi`, `review_v66_regressions mu2_05_concurrent_solvers` | two concurrent MUMPS instances under `MPI_THREAD_MULTIPLE` | after the test fix: 0 failures in 600 and 270 launches (16 in 350 before) |
| `pe01_refusal_reverse` under valgrind | retries its timing window up to 60 times; the memcheck tier uses `--fair-sched=yes` | 79 % of attempts miss the window under the default scheduler; 0 failures in 60 launches with the fix |
| two-rank MUMPS solves | not bitwise reproducible run to run | the oracle's 2-rank cases are diagnostic (D-035) |
| distributed periodic sums at 4 ranks | upstream's arrival order depends on the run | masked in the fixtures (D-039) |

## Memory checking, Miri, coverage and mutation checks

`memcheck` runs valgrind memcheck in the project images plus valgrind and
`libc6-dbg` (`docker/memcheck`) over the real-MPI acceptance, lifecycle,
progress and self-message binaries, the MUMPS acceptance binary and its
regressions, the Scotch binding, `akantu-parser` (libm FFI), `akantu-capi`, the
PETSc acceptance binary and the PETSc DOF-manager models. Valgrind runs with
`--error-exitcode=99 --fair-sched=yes -v`; one `ERROR SUMMARY` per rank is
required, and a failed build or an empty executable list fails. Each run first
checks that a planted defect is detected. Suppressions:
`ci/memcheck/openmpi.supp` (two entries, each anchored at a third-party
function, with the count of every used suppression recorded). Not covered: the
CPython and NumPy FFI of `akantu-python`, the `akantu-linalg` PETSc unit tests
and the PETSc-feature unit tests of the other crates.

`miri` runs, under Stacked and Tree Borrows:

```sh
cargo +nightly-2026-10-01 miri test -p akantu-python --locked
cargo +nightly-2026-10-01 miri test -p akantu-core --lib --locked array_storage
```

Miri cannot run the FFI code (MPI, MUMPS, PETSc, Scotch, CPython). With
`-Zmiri-deterministic-floats` the parser's libm calls run up to `atanh`, which
Miri does not implement.

`coverage` installs `cargo-llvm-cov` 0.9.1 and the `llvm-tools-preview`
component (with `CI_INSTALL_TOOLCHAINS=1`), runs `cargo fetch --locked`, then
`cargo llvm-cov --no-report` over the workspace (arguments after `--` replace
the selection, for example `-- -p akantu-core`) and the report. A failing test
fails the tier; doc-tests are not instrumented. A whole-workspace run measured
87.86 % line coverage (1,458 s on 4 vCPU).

Mutation checks are scripts that apply known defects to a scratch copy and
expect the tests to fail. They edit and restore files, so run them only in a
clone without uncommitted changes:

| Script | What it mutates |
|---|---|
| `crates/akantu-solid/scripts/conv_mutants.sh`, `scripts/conv/conv2_mutants.sh` | the spatial convergence studies of the solid, diffusion and phase-field models |
| `scripts/diffusion/mutants_v102.py <clone> <out.json> [M01,…]` | the diffusion model (16 mutants and a baseline) |
| `scripts/couplers/mutants_acc_coupler.py` | the phase-field acceptance table through the coupler |
| `crates/akantu-extra-materials/scripts/mutations.sh` | the extra materials |
| `crates/akantu-bench/scripts/c1load_mutants.py` | the load placement of the cohesive benchmark driver |
| `crates/akantu-geometry/tests/license_separation_mutations.py` | the licence separation test |
| `ci/fault_injection.sh` (run by `ci/selftest.sh`) | the CI tooling: every injected fault must be detected |

Cargo can keep a mutated build when a restored file keeps an old modification
time; touch restored files before the next build.

## Notes per area

- Python bindings: build into a project-local venv with
  `python3 -m venv .venv && .venv/bin/pip install maturin numpy scipy pytest`,
  then `cd python && ../.venv/bin/maturin develop --release` and
  `../.venv/bin/python -m pytest -q tests`. The Rust side:
  `cargo test -p akantu-python --features python` (set `PYO3_PYTHON`). The
  example comparisons are `python -m pytest -q -p no:cacheprovider
  tests/examples`; in the oracle image's environment every case runs.
- Release-only tests: some cases run only in optimised builds (the 3-D
  cohesive cases, the examples' whole runs, the finest convergence levels,
  `test_solid_mechanics_model_dynamics_implicit_3d`). The `release` and
  `gates` tiers run them.
- Fixture integrity on any host: `cd fixtures && shasum -a 256 -c SHA256SUMS`.
- Generation-script tests (no Docker):
  `cd oracle/probes/python && python3 -m unittest discover -s tests -t tests -v`.
- MPI scenarios that need a second rank print `NOT RUN <name>: <why>` at one
  rank; `scripts/mpi/par-not-run.txt` lists the expected ones and the `par`
  tier fails on any difference at 1, 2 and 4 ranks.
- MPI calls of host code outside akantu, including native solver libraries, are
  not covered by the communicator's thread-level rule; the waits that have no
  deadline are documented at `MpiOptions::timeout`, `Communicator::split` and
  `ReductionPolicy::NativeMpi`.

## State at release

<!-- ORCHESTRATOR: final gate -->
The final gate ran on 14 November 2026 on the tree of the first public
commit, after the publication cleanup, on two 8-vCPU x86_64 Linux machines
(AMD Ryzen 9 9950X with AVX-512F, 16 GB, Ubuntu 24.04). The project images were
built from `docker/` and the containers kept the CI caps of 4 CPUs and 6 GiB.
The pull-request set ran with the command of `CONTRIBUTING.md`; the other tiers
ran as their workflow jobs, through `ci/github/replay.sh`. Every tier ended
`CI: PASS`.

| Part | Result |
|---|---|
| `ci/selftest.sh` | 0 failures, 0 skipped checks |
| pull-request set: `lint`, `test`, `fixtures`, `parity`, `licence`, `msrv`, `docs` | pass (1,055 s); 494 result lines, 3,769 passed, 0 failed, 25 ignored; 462 of 462 test binaries and 27 doc-test sections accounted for |
| the manual's build | pass |
| `exportall` | pass (1,005 s); the same counts as `test` |
| `features` | pass (421 s); 102 checks, 89 in the MUMPS image and 13 in the PETSc image |
| `gates` | pass (975 s); 22 of the 23 lines (the facade line runs in `release`); the 9 `cargo test` lines gave 180 result lines, 1,399 passed, 0 failed |
| `release` | pass (761 s); six runs, 123 result lines, 1,172 passed, 0 failed; the facade run alone (job `facade-release`) 270 passed |
| `examples` | pass (125 s) |
| `mpi` at 1 / 2 / 4 ranks | pass (300 s); 245 runs, 0 failed (78 / 78 / 77, 12 without a rank count); performance gate met |
| `par` at 1 / 2 / 4 ranks | pass (230 s); 8 binaries per rank count, 145 / 167 / 165 scenarios passed, 0 failed, the not-run scenarios as listed |
| `mumps` | pass (333 s); 15 launches at 1 / 2 / 4 ranks and three thread levels, the two regression suites at every rank count and level, 0 failed |
| `petsc` | pass (220 s); 16 launches at 1 / 2 / 4 ranks, 20 oracle comparisons, 0 failed |
| `wheel`, `pydocs` | pass (2,536 s); pytest 306 passed and 3 skipped with greenlet, 300 passed and 9 skipped without, every skip listed; `dynamics` and `custom_material` compared on the AVX-512F CPU |
| `version`, `package`, `minver` (the crates dry run) | pass (133 s); 27 members listed, the downstream consumer with and without default features |

Not run by this gate: 8 ranks, `memcheck`, `coverage`, `miri`, `distro` and
the manylinux and macOS wheel jobs. The check of 13 and 14 November, on an
earlier state of the same tree, ran `mpi` and `par` at 8 ranks (489 and 100
runs, 0 failed), replayed the 23 CI jobs and regenerated five fixture cases
byte-identically on the pinned x86_64 oracle. Nothing has run above 8 ranks
here (the full-size and multi-node runs are described in `docs/BENCHMARKS.md`).

Before publication the workflows ran once on GitHub, on 14 November 2026,
on the 2-vCPU runners of the then private repository: 21 of 29 jobs passed,
six failed for three reasons the replays had not shown (an offline
`cargo tree` against an incomplete registry, two Python example comparisons
that need NumPy's AVX-512F `exp` kernel, a memcheck deadline at 2 vCPUs), the
documentation deployment failed because GitHub Pages was off, and the macOS
wheel job was cancelled at its limit of 60 minutes. The tiers now run
`cargo fetch --locked` first, the two cases skip on CPUs without AVX-512F
(`numpy_avx512f`), the scenario gets a 60 s deadline under memcheck and the
wheel jobs' limits are 90 and 150 minutes (D-118, D-119).

The first runs on the public repository followed the same day. From 18:20 to
19:32 UTC every workflow was started once by manual dispatch on `main` at
commit `f0877f053`, on GitHub's standard runners (`ubuntu-24.04` with 4 vCPU
and 16 GB; `macos-15` on arm64 for the macOS wheel). All 30 jobs passed at
their first attempt and none was rerun. The runs are public in the
repository's Actions tab under the run ids below; each job's artifact keeps
its `summary.json` and step logs for 7 to 14 days.

| Workflow (run id) | Job | Minutes | Result |
|---|---|---:|---|
| `fast` (43626005524) | `fast` | 35 | `CI: PASS`; 494 result lines, 3,769 passed, 0 failed, 25 ignored; 462 of 462 test binaries and 27 doc-test sections accounted for |
| | `msrv` | 2 | pass |
| | `selftest` | 1 | pass |
| `docs` (43626054675) | `rustdoc` | 1 | pass |
| | `manual` | 1 | pass |
| | `site` | 47 | pass; the manual, the Rust API and the Python API in one site |
| | `deploy` | under 1 | deployed; served at <https://thomas0903.github.io/akantu-rs/> |
| `features` (43626061497) | `features` | 10 | 102 checks, 0 failed |
| | `gates` | 21 | 22 of the 23 lines (the facade line runs in `facade-release`); the 9 `cargo test` lines gave 180 result lines, 1,399 passed, 0 failed |
| | `facade-release` | 14 | 37 result lines, 270 passed, 0 failed |
| | `exportall` | 23 | the same counts as `fast` |
| `medium` (43626068416) | `mpi` | 8 | 245 runs at 1 / 2 / 4 ranks (78 / 78 / 77, 12 without a rank count), 0 failed; performance gate met |
| | `par` | 9 | 8 binaries per rank count, 145 / 167 / 165 scenarios passed at 1 / 2 / 4 ranks, 0 failed |
| | `mumps` | 12 | 15 launches at 1 / 2 / 4 ranks and the two regression suites, 0 failed; 1 oracle comparison |
| | `petsc` | 8 | 20 oracle comparisons, 0 failed |
| | `examples` | 5 | pass |
| | `release` | 16 | six runs, 123 result lines, 1,172 passed, 0 failed |
| | `exportall` | 30 | the same counts as `fast` |
| `python` (43626074948) | `python` | 46 | pytest 306 passed and 3 skipped with greenlet, 300 passed and 9 skipped without; on the runner's Intel Xeon 6975P-C (AVX-512F) `dynamics` and `custom_material` were compared, `eigen_modes` and `eigen_modes_eigen` skipped (SciPy 1.18.1, the oracle's is 1.15.3); pdoc built |
| `packaging` (43626081603) | `detect` | under 1 | pass |
| | `crates-dry-run` | 3 | 27 members listed, the downstream consumer and the minimal versions checked |
| | `wheel-linux` | 70 | manylinux_2_28 wheel imported on CPython 3.9 and 3.13; pytest 304 passed and 5 skipped with greenlet, 298 passed and 11 skipped without (AMD EPYC 7763, no AVX-512F: `dynamics` and `custom_material` skipped) |
| | `wheel-macos` | 51 | arm64 wheel built and imported in 208 s; pytest 302 passed and 7 skipped with greenlet, 296 passed and 13 skipped without; `wheel` tier `CI: PASS` after 3,044 s |
| `memcheck` (43626087936) | `mpi` | 11 | 36 runs under valgrind, 0 failed; the planted defect detected |
| | `mumps` | 7 | 15 runs, 0 failed; the planted defect detected |
| | `petsc` | 7 | 4 runs, 0 failed; the planted defect detected |
| `coverage` (43626094060) | `coverage` | 35 | 87.35 % of lines, 82.88 % of functions, 87.32 % of regions |
| `distro` (43626100455) | `ubuntu-22-04` | 39 | `features` 85 checks passed, 18 not applicable (MUMPS and PETSc); `test` with the same counts as `fast` |
| `expensive` (43626107422) | `ranks-8` | 13 | `mpi` at 8 ranks: 88 runs (76 at 8 ranks, 12 without a rank count), 0 failed; `par` at 8 ranks: 8 binaries, 123 scenarios passed, 0 failed |
| | `miri` | 2 | pass |

With these runs the rows CI-001, CI-014 and CI-024 are *Verified*.
