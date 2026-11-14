<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# `ci/`: build, packaging and CI tooling

Scripts for every CI tier of akantu-rs. One entry point, `ci/run-all.sh`, runs
the same tiers on a developer machine, on any Linux host with Docker and in the
GitHub workflows of `ci/github/` (copied to `.github/workflows/`; see
`ci/github/README.md`). Nothing here publishes anything or reads a token. How
upstream's GitLab CI maps onto these tiers, row by row (CI-001 to CI-029), is
in `ci/MAPPING.md`. How a contributor runs the tiers, what each covers and the
measured times are in `docs/TESTING.md`.

## Layout

| Path | Content |
|---|---|
| `run-all.sh` | the entry point: presets, tiers, backends, summary |
| `lib.sh` | shared helpers of the tier scripts (steps, skips, verdicts, logs) |
| `tiers/<tier>.sh` | one script per tier |
| `selftest.sh`, `fault_injection.sh` | offline self-test of this directory: tiny synthetic workspaces with injected faults, the gate map, workflow consistency |
| `tools/` | Python helpers: the test analyser `analyze_tests.py` and its baseline `known-failures.txt`, the list of feature-gated commands `feature-gates.txt`, `count_tests.py` (floors), `feature_matrix.py`, `licence_audit.py`, `package_report.py`, `pytest_skips.py`, `python_env.py`, `release_scope.py`, `version_check.py`, `gate_map.py`, `summarize.py`, and their tests `test_tools.py` |
| `floors.txt` | minimum test counts per run |
| `licence-policy.toml` | allowed third-party licences and the GPL features (D-042) |
| `pytest-skips.txt` | expected skips of the `wheel` tier's pytest runs, with platform lines |
| `memcheck/` | the valgrind runner, its helpers and the Open MPI suppressions |
| `github/` | the workflow files and their helpers (`images.sh`, `free-disk.sh`, `distro-run.sh`, `distro-inner.sh`, `pages-site.sh`, `replay.sh`, `replay-all.sh`) |

## Tiers

| Tier | What it runs | Where cargo runs | Upstream (row) |
|---|---|---|---|
| `lint` | `cargo fmt --all --check`; `cargo clippy --workspace --all-targets --locked -- -D warnings`; `bash -n` and `shellcheck -x` of `ci/` and `scripts/` (`CI_SHELLCHECK_PATHS`; the shell scripts under `oracle/`, `python/oracle`, `python/examples`, `benchmarks/` and `crates/*/scripts` are not linted); `ruff check python/` (rules `E4,E7,E9,F`, line length 79, as upstream's job) | backend (shellcheck and ruff on the host; a missing one is a skip) | code quality (CI-002) |
| `test` | `cargo fetch --locked` (every package of `Cargo.lock` in the registry: the licence tests run `cargo tree --offline` for feature sets the test build never downloads), `cargo metadata`, `cargo test --no-run --message-format=json`, `cargo test --workspace --locked --no-fail-fast`; verdict by `tools/analyze_tests.py` against `tools/known-failures.txt` (every binary accounted for, failures equal to the baseline; the tier fails without them), then the floor (`tools/count_tests.py`, `floors.txt`) | backend | ctest job (CI-003 to CI-006, CI-020) |
| `exportall` | the `test` tier built and run with the test-only feature `akantu-core/export-all`, with the same accounting and its own floor | backend | CI-003 |
| `fixtures` | `sha256sum -c` (or `shasum -a 256 -c`) of every `fixtures/**/SHA256SUMS` | host | golden data |
| `parity` | the STATS block of `docs/FEATURE_PARITY.md` and the generated status block of `README.md` equal the output of `scripts/parity/stats.py --update` (run on a scratch copy) | host | bookkeeping |
| `licence` | `tools/licence_audit.py`: third-party licences against `licence-policy.toml`, member licences, the GPL separation of D-042 (`cargo tree` per member with default, no-default, all and each feature; every allowed GPL feature must bring GPL code in), SPDX headers of every file under a member (GPL ids only in `akantu-geometry-cgal`, no `include!` or `#[path]` of GPL code into an LGPL member) | backend | CI-026, CI-029 |
| `features` | `tools/feature_matrix.py`: workspace default, all features and no default; per member with features: none, each feature alone (`--lib`), default and all (`--all-targets`); `RUSTFLAGS=-D warnings`; clippy and rustdoc with `--all-features`; `check --no-default-features --all-targets`. Default image `akantu-rs-mumps:dev` (`CI_FEATURES_IMAGE`); the checks that need `petsc` (13: the workspace with all features and the `petsc` feature of five crates) run in `akantu-rs-petsc:dev` (`CI_FEATURES_PETSC_IMAGE`, `off` disables that leg) with their own target directory. An absent image is a skip; `CI_NATIVE_NA=petsc` declares the checks not applicable. `legs.tsv` records the leg of every check | backend | CI-018, CI-026, CI-027 |
| `gates` | every command of `tools/feature-gates.txt` (feature-gated clippy, test, check and doc lines: IGFEM and `cgal`, the umbrella's `cgal` and `igfem`, the release cohesive tests, the `python` feature, extra-materials-non-local, the release run of `akantu-solid` and the convergence studies of diffusion, phase field and couplers, nine `cargo doc --no-deps` lines, the release replays of `akantu-bench`), one step each, in the MPI image; each `cargo test` line gets its own `--no-run` build, the analyser scoped to the line's packages and targets, and a floor `gates-N`. The line `cargo test --release -p akantu` is the `facade` run of tier `release` | backend | CI-003, CI-018, CI-026 |
| `msrv` | `cargo check` with the toolchain of `workspace.package.rust-version` (1.90), libraries and binaries, then all targets | host (rustup) | D-025, CI-025 |
| `minver` | `cargo +nightly update -Z direct-minimal-versions` on a `git archive` copy, then `cargo check --workspace --all-targets` | host (rustup) | CI-029 |
| `package` | `cargo package --list -p <member>` for every member, `tools/package_report.py` (sizes, gzip estimate, readiness findings), a downstream consumer crate with default and no-default features | backend | CI-021, CI-022, CI-029 |
| `version` | `tools/version_check.py`: every member on `workspace.package.version`, the Python package takes it from Cargo, a tag (`CI_VERSION_TAG` or `GITHUB_REF_NAME`) is `v<version>`; `git describe` printed | backend | CI-017 |
| `wheel` | maturin abi3 wheel, `pip install` and `import akantu` in a throw-away venv, the step `environment` (`tools/python_env.py`: CPU, NumPy's features and dispatch targets, the example comparisons' environment keys), `pytest python/tests` twice (with and without greenlet; expected skips in `pytest-skips.txt`); `CI_WHEEL_PYTHON`, `CI_WHEEL_COMPAT` (`linux`, `manylinux_2_28`, `off` on macOS) and `CI_WHEEL_SMOKE` select the interpreter, the tag and more CPython versions to import in | host (the image has no pip) | CI-013, CI-014, CI-021 |
| `pydocs` | HTML API documentation of the Python package with pdoc 16.0.0 over the `wheel` tier's wheel; fails without the package and extension pages; nothing is hosted | host | CI-024 |
| `docs` | `cargo doc --workspace --no-deps`, `RUSTDOCFLAGS=-D warnings` | backend | CI-024 |
| `miri` | `cargo +nightly miri test -p akantu-python` and `-p akantu-core array_storage`, Stacked and Tree Borrows; nightly pinned (`CI_MIRI_NIGHTLY`) | host (rustup) | valgrind / ASAN (CI-010, CI-020) |
| `memcheck` | valgrind memcheck of the FFI `unsafe` code in the project images plus valgrind (`docker/memcheck`; `CI_MEMCHECK_KIND` = `mpi`, `mumps`, `petsc`); the binaries and the verdict rule are in the header of `memcheck/run-in-image.sh`; `--error-exitcode=99 --fair-sched=yes -v`, one `ERROR SUMMARY` per rank required, a failed build or an empty executable list fails; suppressions `memcheck/openmpi.supp`, each justified, the count of every used suppression recorded. Not covered: the CPython and NumPy FFI of `akantu-python` | Docker | valgrind job (CI-010) |
| `coverage` | `cargo fetch --locked`, then `cargo llvm-cov` 0.9.1 (installed with `CI_INSTALL_TOOLCHAINS=1`) over the workspace tests (`-- -p <crate>` selects); stale profile data is cleaned first and the report follows only a passing run; per-file table, lcov and JSON with the host backend; no threshold | backend | gcovr (CI-020) |
| `mpi` | `scripts/mpi/run-tests.sh` (1, 2 and 4 ranks, `akantu-comm`) | Docker | parallel tests (CI-003) |
| `par` | `scripts/mpi/run-par-tests.sh --scotch` (1, 2 and 4 ranks) in `akantu-rs-scotch:dev` | Docker | parallel tests |
| `mumps` | `scripts/mumps/run-mumps-tests.sh` (1, 2 and 4 ranks, `akantu-rs-mumps:dev`; `na` on a tree without MUMPS) | Docker | CI-027, CI-011 |
| `petsc` | `scripts/petsc/run-petsc-tests.sh` (1, 2 and 4 ranks × thread levels, `akantu-rs-petsc:dev` from `docker/petsc`; `na` on a tree without it) | Docker | CI-027 |
| `release` | optimised test runs: cohesive with `COH_ALL=1`, linalg, solid 3-D implicit dynamics, core and parser, the facade crate with the examples' whole runs, dumpers; each with its own `--no-run` build, judged by the analyser against the same waiver list (scoped to the run by `tools/release_scope.py`) and a floor; fails without the analyser | backend | build profiles (CI-028) |
| `examples` | the parallel examples `parallel_2d` and `phase_field_parallel` (release, feature `mpi`) with `mpirun -np 2`; every rank must exit 0 and write its output files | Docker (MPI image) or a host with Open MPI | CI-009 |
| `expensive` | `run-tests.sh --ranks 8`, `run-par-tests.sh --scotch --ranks 8` | Docker | CI-003 |

Presets: `fast` = lint test fixtures parity licence (default), `features` =
features msrv gates, `medium` = mpi par mumps release examples petsc
exportall, `packaging` = package wheel minver version, `python` = wheel
pydocs, `docs`, `miri`, `memcheck`, `coverage`, `distro` = features test (the
second distribution, host backend), `expensive`, and `all` = everything except
miri, memcheck, coverage, distro and expensive. `ci/run-all.sh --list` prints
them. The gate map in `ci/github/README.md` says which workflow job runs which
tier (checked by `tools/gate_map.py`).

## Running

```sh
ci/run-all.sh [--preset P | --tiers a,b] [--backend host|docker] [--out DIR]
              [--image IMG] [--target-name NAME] [--step-timeout S] [--label TEXT]
              [--dry-run] [--list] [-- args for mpi/par/mumps/expensive]
ci/selftest.sh                    # offline self-test of ci/ (no Docker; compiles tiny workspaces)
python3 ci/tools/test_tools.py    # unit tests of the Python helpers
```

- `--backend host` runs cargo on this machine (rustup honours
  `rust-toolchain.toml`). `--backend docker` runs it in the project's Linux
  image through `scripts/mpi/in-docker.sh`: repository read-only at `/src`,
  cargo target in the volume `akantu-rs-mpi-target` under
  `/target/<--target-name>` (default `ci`). The docker backend on x86_64 is the
  authoritative configuration (D-043).
- Light enough for a laptop: `fixtures`, `parity`, `licence` (cargo resolution
  only), `ci/selftest.sh` and the Python unit tests. `package` compiles the
  default `akantu` closure twice (its downstream-consumer check); the other
  tiers want a Linux machine with Docker (`docs/TESTING.md`).
- Status rules (`lib.sh`): a step passes on exit 0 only. The one exception,
  `--ok-exit "0 101"` for `cargo test`, is used only where the analyser step
  that follows judges the failures. A step that cannot run (missing tool,
  image, toolchain or native library) is `skip`, and its tier is `incomplete`,
  never `pass`. A step that does not apply to the tree is `na`. Verdict:
  `CI: PASS` (exit 0), `CI: FAIL` (1), `CI: INCOMPLETE` (3), usage error 2. The
  only waiver anywhere is the reviewed `tools/known-failures.txt`.
- A step runs under `timeout` (`CI_STEP_TIMEOUT`, default 3600 s); a docker
  step's container is named after the step, so a timeout kills only that
  container.
- Output (`--out`, default `target/ci/<UTC>`): `provenance.txt` (git head,
  tree, dirty count, host, toolchains, image ids when Docker is used); per tier
  `steps.tsv` (step, status, exit, seconds, reason), `commands.tsv` (exact
  commands), one log per step and the tier's reports (`licence/report.json`,
  `package/report.json`, `features/plan.tsv`, `features/features.md`,
  `test/test-counts.txt`); `summary.tsv` and `summary.json` (schema
  `akantu-rs-ci-summary/1`: verdict, failed and incomplete tiers, provenance,
  every step with status, exit code, seconds, command and log path).

To reproduce one GitHub job exactly on a Linux machine with Docker and Rust:
`ci/github/replay.sh <workflow>:<job>[:<leg>] <out-dir>`
(`ci/github/README.md`, "Replaying a job").

## Results

The release check (every tier and every workflow job replayed, all passing)
and the first runs on GitHub are summarised in `docs/TESTING.md`, "State at
release".
