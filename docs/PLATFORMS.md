<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Platforms

akantu-rs builds and passes its tests on three platforms: x86_64 Linux,
aarch64 Linux and macOS on Apple silicon (arm64). x86_64 Linux is the
reference platform: the oracle fixtures were recorded there and the
reference runs of every tier are made there (D-043 in `docs/DECISIONS.md`).
On 14 and 15 November 2026 the test tiers also ran on ARM, natively on
macOS arm64 and in Docker on aarch64 Linux. Every tier passed except two
items. One self-consistency check of the optional PETSc tier exceeds its
bound on aarch64 Linux and is a known failure there (below). On macOS the
three MPI documentation lines of the feature gates fail because no MPI is
installed there. On aarch64 Linux one MUMPS scenario and five PETSc
scenarios do not compare values with the C++ reference, because their
oracle records exist for x86_64 only (D-121); the tiers say so and count
them. This file gives the results, what differs between the platforms and
how to rerun each line.

## Supported platforms

| Platform | Status | How it is verified |
|---|---|---|
| x86_64 Linux (glibc; Debian trixie images, Ubuntu 22.04 and 24.04 hosts) | reference platform: oracle fixtures, bitwise parity claims, reference runs of every tier | every tier, and the release check (`docs/TESTING.md`, "State at release") |
| aarch64 Linux (glibc; the same Debian trixie images built for arm64) | supported; every tier runs; one MUMPS scenario and five PETSc scenarios do not compare values with the C++ reference (their records exist for x86_64 only, D-121); one PETSc self-consistency check is a known failure | the runs below (Docker on an Apple M4 Pro); the aarch64 oracle values of the nine MUMPS cases are kept as platform variants (D-073) |
| macOS arm64 (Apple silicon) | supported for development and for the Python wheel; no MPI back-end tested; last-bit differences of the system libm where the port calls it (below) | the runs below (native, Apple M4 Pro); the GitHub job `packaging.yml:wheel-macos` on `macos-15` (Apple M1, virtual), green on 14 November 2026 in run 43626081603: with greenlet 302 / 0 / 7 skipped, without 296 / 0 / 13 skipped, the counts of the native run (`docs/TESTING.md`, "State at release") |
| other platforms (Windows, macOS x86_64, other CPUs) | not tested | none |

## Results on ARM

The machine is an Apple M4 Pro (14 cores, 24 GB) running macOS 27.2. The
Linux runs used Docker (OrbStack) with images built from `docker/` for
linux/arm64 and containers of 8 CPUs and 14 GB. The runs of 14 November
used the release candidate's code; the MUMPS and PETSc tiers were run again
on 15 November after the change recorded in D-121.
Counts are the test results that cargo or pytest printed, accounted for by
the analyser `ci/tools/analyze_tests.py` (every test binary and doc-test
crate must report). Times are wall times on a shared developer machine,
for information only; they are not performance results.

| Platform | Tier (what runs) | Result | Passed / failed / ignored or skipped | Date | Wall time |
|---|---|---|---|---|---|
| macOS arm64 | `lint`: `cargo fmt --check`, `cargo clippy --workspace --all-targets -D warnings`, `bash -n`, shellcheck, ruff | pass | | 14 Nov | 13 s |
| macOS arm64 | `test`: `cargo test --workspace --locked --no-fail-fast` (debug, with the doc tests) | pass | 3,760 / 0 / 25 (494 result lines, 462 / 462 binaries, 27 doc-test crates) | 14 Nov | 1,234 s |
| macOS arm64 | `fixtures`, `parity`, `licence` | pass | | 14 Nov | 19 s |
| macOS arm64 | `gates`: the lines of `ci/tools/feature-gates.txt` (IGFEM and `cgal`, `extra-materials-non-local`, the `python` feature, the release tests, the convergence studies, `cargo doc`) | 19 of 22 lines pass; the 3 `cargo doc … --features mpi` lines fail (no `mpicc`) | the 9 test lines: 1,397 / 0 / 0 | 14 Nov | 736 s |
| macOS arm64 | `release`: the six optimised test runs | pass | 1,170 / 0 / 0 | 14 Nov | 422 s |
| macOS arm64 | the facade with `igfem,ntn-contact,extra-materials-non-local`; `akantu-solid` and `akantu-extra-materials` with `damage-non-local` | pass | 1,017 / 0 / 0 | 14 Nov | 570 s |
| macOS arm64 | `wheel`: abi3 wheel (`macosx_11_0_arm64`, CPython 3.12.12, NumPy 2.5.4), `import akantu`, pytest with and without greenlet, the skip checks | pass | with greenlet 302 / 0 / 7 skipped; without 296 / 0 / 13 skipped | 14 Nov | 1,715 s |
| macOS arm64 | `ci/selftest.sh` | pass | 55 checks, 1 skipped (actionlint not installed) | 14 Nov | |
| aarch64 Linux | `lint` | pass | | 14 Nov | 24 s |
| aarch64 Linux | `test` | pass | 3,762 / 0 / 25 (494 result lines, 462 / 462 binaries, 27 doc-test crates) | 14 Nov | 939 s |
| aarch64 Linux | `fixtures`, `parity`, `licence` | pass | | 14 Nov | 12 s |
| aarch64 Linux | `exportall`: `test` with `akantu-core/export-all` | pass | 3,762 / 0 / 25 (494 result lines, 462 / 462 binaries) | 14 Nov | 776 s |
| aarch64 Linux | `mpi`: real-MPI scenarios at 1, 2 and 4 ranks, performance gate | pass | 245 scenario lines, 0 failed | 14 Nov | 179 s |
| aarch64 Linux | `par`: the par binaries with Scotch at 1, 2 and 4 ranks | pass | 28 runs (8 binaries at 3 rank counts, 4 lifecycle runs), 477 scenarios, 0 failed | 14 Nov | 245 s |
| aarch64 Linux | `mumps`: MUMPS at 1, 2 and 4 ranks and three thread levels, the regression scripts, the MUMPS patch tests | pass | 22 of 22 steps; oracle comparisons: 0 compared, 1 not compared (the two-process trace, D-121) | 15 Nov | 341 s |
| aarch64 Linux | `examples`: `parallel_2d` and `phase_field_parallel` at 2 ranks | pass | | 14 Nov | 54 s |
| aarch64 Linux | `gates` (in the MPI image) | pass | 22 lines pass, 1 run by `release`; the 9 test lines: 1,398 / 0 / 0 | 14 Nov | 680 s |
| aarch64 Linux | `release` | pass | 1,171 / 0 / 0 | 14 Nov | 396 s |
| aarch64 Linux | `features`: the feature matrix with `-D warnings`, clippy and rustdoc of `--all-features`, both legs (MUMPS and PETSc images) | pass | 103 / 103 checks | 14 Nov | 230 s |
| aarch64 Linux | `wheel` and `pydocs`: wheel built in the arm64 image (CPython 3.13.5, NumPy 2.2.4), pytest twice with the skip checks, pdoc | pass | with greenlet 304 / 0 / 5 skipped; without 298 / 0 / 11 skipped | 14 Nov | 1,851 s |
| aarch64 Linux | `petsc` (optional): PETSc at 1, 2 and 4 ranks and two thread levels, the regressions, the patch tests | fail: 19 of 23 steps pass | 321 / 0 / 0 libtest tests; the 4 failing steps are `petsc_models` at 2 and 4 ranks, each failing only the self-consistency guard described below; oracle comparisons: 0 compared, 20 not compared (5 scenarios, D-121) | 15 Nov | 209 s |

Not run on ARM: the 8-rank and expensive tiers, `memcheck`, `miri`,
`coverage`, `msrv`, `minver`, `package`, `version` and the documentation
site. On macOS the feature matrix and `exportall` were also left out
(the Mac's free disk space); both ran on aarch64 Linux.

For comparison, x86_64 in the release check (`docs/TESTING.md`, "State at release"): the
workspace tests gave 3,762 passed, 0 failed, 25 ignored and 462 / 462
binaries. The test lists differ by platform-gated tests: aarch64 Linux runs
`libm_exhaustive_against_host_glibc` and not `explicit_dynamic_matches_the_oracle`
(so its count equals x86_64's); macOS runs neither and not
`memory::tests::reads_this_process` either (two fewer).

### The PETSc guard that does not hold on aarch64

`test_phase_field_optimal_linear_distributed` (`crates/akantu-couplers/tests/petsc_models.rs`)
runs TEST-095 with the TAO option `pc_type none` on 2 and 4 ranks and
compares the damage with the same run on one process, under D-082's
distributed-against-serial bound of 1e-14 relative to the maximum. On
x86_64 the difference is 5.2e-15. On aarch64 Linux it is 1.996e-14: 78 of
2,000 nodal damages differ, with the same value at 2 and 4 ranks and at
both thread levels; TAO converges in 10 iterations on every partition. This
is a known failure of the optional PETSc tier on aarch64 Linux. It is kept
failing: no tolerance is changed without a derivation, and no derivation of
a bound that covers the aarch64 value exists. The other checks of the
scenario (upstream's criterion at every rank count, bit identity at one
rank, the option's effect) pass.

## What is compared on aarch64 for MUMPS and PETSc

OpenBLAS selects its kernels from the CPU, so results of solvers that call
it differ in their last bits between x86_64 (pinned to the Haswell kernels,
D-050) and aarch64. D-121 sets which comparisons run where:

| Solver | Compared with the oracle on aarch64 Linux | Not compared on aarch64 Linux (x86_64 records only) |
|---|---|---|
| MUMPS | the nine cases with an `aarch64-auto` variant (`model/static` 5, `model/implicit_dynamic` 1, `parallel/smm_static_cube_n3` 3 rank records); every multi-rank solve against the port's serial Eigen solve | the displacements and the trace files' contents of the two-process trace `solver_model/dbltrace_newton_raphson_np2`; its local nodes, iteration counts and file names are compared (they are the same on both platforms) |
| PETSc | none (no aarch64 PETSc oracle exists); every multi-rank run against the port's own serial run, the patch tests under upstream's criteria | `oracle_serial`, `blas_pin`, the record assertions of `phase_field_tao_oracle` and `coupler_tao_oracle` and the one-rank record check of `test_phase_field_optimal_linear_distributed`; their runs, bounds, irreversibility and record checks still run |

In all, one MUMPS scenario and five PETSc scenarios do not compare values
with the C++ reference on aarch64 Linux. A scenario that does not compare
prints one line `ORACLE <scenario>: not compared: <reason>`, and the MUMPS and PETSc tiers end with the count of
scenarios compared and not compared. An aarch64 oracle run of the
two-process trace (three identical runs) equals the port's aarch64 run bit
for bit on all 28 displacements; it is not committed, because the case
stores its per-iteration matrices and solutions as inputs, which a
platform variant shares with the top level.

## What differs between the platforms

The arithmetic of the port is the same on all three platforms: no
fast-math, no fused multiply-add unless the reference has one, and the
mathematical functions come from `akantu_core::glibc_libm`, a translation
of the oracle's glibc 2.41 (`docs/NUMERICAL_VALIDATION.md`, "Platform
effects"), except in the input-file parser and the few call sites listed in
the table below, which call the platform libm. The oracle fixtures were recorded on
aarch64 first and regenerated on x86_64; most values are identical on both.
The differences that remain, and how the tests handle each one:

| Difference | Where it shows | How the tests handle it |
|---|---|---|
| Sign of the default NaN: `0x7ff8…` on ARM, `0xfff8…` on x86_64 | cases that overflow or divide 0 by 0, e.g. the parser expression `pow(-8, 1./3.)` | exact comparisons treat two default quiet NaNs of either sign as equal and keep every other value bitwise (D-046; `crates/akantu-validation/src/exact.rs`) |
| OpenBLAS kernels inside MUMPS | 9 fixture cases solved with MUMPS (40 quantities) | the x86_64 values are the golden values, the aarch64 values a recorded variant under `variants/aarch64-auto/`; the loader picks the variant of the running platform (`AKANTU_FIXTURE_ENV` overrides it; D-073) |
| OpenBLAS kernels inside MUMPS and PETSc, records without a variant | the two-process MUMPS trace and the PETSc oracle records | compared only in `x86_64-haswell`; elsewhere reported as not compared (D-121, previous section) |
| Complex division in the x86_64 runtime | dense eigenvalue and determinant records | an x86_64 delta fixture (`crates/akantu-linalg/tests/fixtures/dense/dense_strict_x86_64_delta.txt`); the aarch64 records stay the default |
| Host libm in the input-file parser (the parser calls the platform libm, as upstream does) | 18 conversions of `extra/expressions.dat` differ by one unit in the last place on macOS (Apple libm: `acos`, `asin`, `atan`, `atanh`, `erf`, `erfc`, `lgamma_r`) | an enumerated exception list for macOS in `crates/akantu-parser/tests/oracle.rs`; on Linux (glibc, both architectures) the list is empty and the comparison is bitwise |
| Host libm in library code outside the parser: `powf` in the cohesive linear law's insertion-traction scaling by element volume (`crates/akantu-cohesive/src/laws/linear.rs`, two sites, upstream `std::pow`), `powf(3.0)` in the contact penalty's slip moduli (`crates/akantu-contact/src/resolution.rs`), `atan2`, `atan` and `tan` in the anisotropic damage law (`crates/akantu-solid/src/materials/anisotropic_damage.rs`; its `tan` uses the ported glibc function for `\|x\| ≤ 25` and the platform's beyond) | cohesive insertion with a volume-scaled strength, frictional contact with the upstream slip moduli, anisotropic damage | on glibc (both Linux architectures) these calls give the oracle's values, since the oracle uses the same libm; on macOS they can differ from the oracle in the last bit; the macOS test runs above showed no such difference in the cases compared, and no exception list exists for them |
| Host libm in one C++ example | `explicit_dynamic`: the initial pulse calls `sin` and `exp` of the C library | the comparison with the oracle runs on x86_64 glibc only (`crates/akantu/tests/examples_explicit_dynamic.rs`); the other tests of the example run everywhere |
| NumPy's own kernels in two Python examples | `dynamics` and `custom_material` compute their initial pulse with `np.sin * np.exp` | compared bitwise only where the platform key and NumPy's AVX-512F kernel match the oracle's (`python/examples/cases.py`, `ORACLE_ENVIRONMENT`); elsewhere the cases are skipped and the skips are listed per platform in `ci/pytest-skips.txt` |
| NumPy / SciPy versions | `eigen_modes*` (SciPy 1.15.3), `solver_callback*` (NumPy < 2.5) | skipped where the installed versions differ; listed in `ci/pytest-skips.txt` |
| Gmsh mesh generation (Debian builds Gmsh per architecture; the same `.geo` file gives different meshes on aarch64 and x86_64) | every mesh a test or example reads | no test runs Gmsh: the meshes are generated once and committed under `fixtures/meshes/` |
| Linux-only and platform-only tests | `memory::tests::reads_this_process` (`/proc`, Linux); `libm_exhaustive_against_host_glibc` (aarch64 glibc only: it compares the translated libm with the host's) | `cfg` attributes; they explain the different test counts in the table |
| `c_char` is `u8` on aarch64 Linux and `i8` on x86_64 and macOS | FFI code and its tests | conversions to bytes use `to_ne_bytes`, never `as u8` (clippy rejects a same-type cast on aarch64) |
| matplotlib backend | macOS uses the GUI backend `macosx` by default, so `plt.show()` in an example opens a window and waits for it to be closed; on a Linux host without a display matplotlib falls back to Agg | the Python tests set `MPLBACKEND=Agg` (`python/tests/conftest.py`, the example harness and `ci/tiers/wheel.sh`); `test_examples_cannot_open_a_window` checks it |

Bitwise comparisons with the oracle therefore hold on all three platforms
for every case except those listed above. The cases compared on x86_64 only
are the `explicit_dynamic` oracle comparison, the two NumPy-pulse Python
examples on a CPU with AVX-512F, the oracle comparisons that need the
oracle's exact SciPy or NumPy version, the two-process MUMPS trace and the
PETSc oracle scenarios. The MPI, Scotch and MUMPS tiers otherwise compare
the distributed results with the oracle on aarch64 Linux as they do on
x86_64.

Native MPI, Scotch, MUMPS and PETSc were not run on macOS: `mpirun` is not
installed on the Mac used here. The three `cargo doc … --features mpi`
lines of the feature gates fail there for that reason (`mpicc` not found).
These back-ends are covered on aarch64 Linux.

## Performance numbers

The Rust against C++ comparisons, the scaling series and the full-size run
in `docs/BENCHMARKS.md` were measured on x86_64 cloud machines. A few early
micro-measurements in that file were taken on the Mac and say so. No timing
from the ARM runs in this file is a performance result.

## How to rerun

Commands are those of `ci/README.md`; the output directory and the
summary format are described there. Export `MPLBACKEND=Agg` on a desktop
machine.

macOS arm64, from the repository (the default `target/`):

```sh
ci/run-all.sh --preset fast --backend host            # lint test fixtures parity licence
PYO3_PYTHON=python3.12 ci/run-all.sh --tiers gates,release --backend host
cargo test -p akantu --features igfem,ntn-contact,extra-materials-non-local --locked --no-fail-fast
cargo test -p akantu-solid --features damage-non-local --locked --no-fail-fast
cargo test -p akantu-extra-materials --features damage-non-local --locked --no-fail-fast
CI_WHEEL_COMPAT=off CI_WHEEL_PYTHON=python3.12 ci/run-all.sh --backend host --tiers wheel
```

The lint tier needs `shellcheck` and `ruff` on the `PATH` (ruff 0.17.3 as
in `ci/github/fast.yml`; these runs used shellcheck 0.10.0 from PyPI, the
CI pins 0.9.0). Without MPI the three `cargo doc … --features mpi` lines of
`gates` fail.

aarch64 Linux, on an arm64 Docker host. Build the images from `docker/`
for the host's architecture (they are Debian packages only), then run the
tiers from a clone of the repository, not from a git worktree: in a
worktree, `.git` is a file that points outside the read-only mount, and
the tests that call `git` fail.

```sh
docker build -t akantu-rs-mpi:dev docker/mpi
docker build -t akantu-rs-scotch:dev docker/scotch
docker build -t akantu-rs-mumps:dev docker/mumps
docker build -t akantu-rs-petsc:dev docker/petsc
git clone --local <repository> akantu-rs-clone && cd akantu-rs-clone
AKANTU_RS_DOCKER_CPUS=8 AKANTU_RS_DOCKER_MEMORY=14g \
  ci/run-all.sh --preset fast --backend docker
AKANTU_RS_DOCKER_CPUS=8 AKANTU_RS_DOCKER_MEMORY=14g \
  ci/run-all.sh --tiers mpi,par,mumps,examples,petsc,gates,release,features,exportall --backend docker
```

Images with other tags are selected with `CI_IMAGE`, `AKANTU_RS_MPI_IMAGE`,
`AKANTU_RS_SCOTCH_IMAGE`, `AKANTU_RS_MUMPS_IMAGE`, `AKANTU_RS_PETSC_IMAGE`,
`CI_FEATURES_IMAGE` and `CI_FEATURES_PETSC_IMAGE`. The cargo target lives
in the Docker volume `akantu-rs-mpi-target`; a debug build of the workspace
tests takes about 27 GB there.

The Python tier runs with the host backend inside a container, because the
project image has no `venv` module: an image `FROM akantu-rs-mpi:dev` with
`python3-venv` and `python3-pip` added, the clone mounted at `/src`, and

```sh
CI_WHEEL_PIP_BASE=numpy==2.2.4 ci/run-all.sh --backend host --preset python --out /out
```

(NumPy pinned as in `ci/github/python.yml`).

The aarch64 oracle run of the two-process trace (three runs, compared
before anything is written; output to a scratch directory):

```sh
SOLVER_MODEL_PROBE_NP=2 SOLVER_MODEL_PROBE_RUNS=3 \
SOLVER_MODEL_PROBE_CASES=dbltrace_newton_raphson_np2 \
  oracle/probes/solver_model/generate.sh --out <scratch directory>
```
