<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# GitHub Actions workflows

The workflow files of this directory are the delivered CI. GitHub runs the
copies in `.github/workflows/`; `ci/selftest.sh` checks that every file there is
an unchanged copy of its file here, so edit the file here and copy it. Every
job calls the same entry point as a local run, `ci/run-all.sh`
(`ci/README.md`), so any job can be reproduced on a Linux machine with Docker
("Replaying a job" below). No workflow reads a secret or publishes a package.

## Workflows and jobs

The `on:` block of each file is authoritative. A pull request and a push to
`main` run the checks of `fast.yml` and `docs.yml`, without path filters. The
heavier tiers run weekly, on Saturday (UTC, one hour apart), and on manual
dispatch. A maintainer dispatches `features.yml`, and the others where they
apply, on the branch of a pull request that needs them (`docs/MAINTAINING.md`).

| File | Triggers | Jobs |
|---|---|---|
| `fast.yml` | pull requests; pushes to `main` and `v*` tags; manual | `fast`, `selftest`, `msrv` |
| `docs.yml` | pull requests; pushes to `main`; manual. `site` and `deploy` run only on a push to `main` or a manual run on `main` | `rustdoc`, `manual`, `site`, `deploy` |
| `features.yml` | weekly (Saturday 01:17 UTC); manual | `features`, `gates`, `facade-release`, `exportall` |
| `medium.yml` | weekly (Saturday 02:17); manual | `medium` × {mpi, par, mumps, petsc, release, examples, exportall} |
| `python.yml` | weekly (Saturday 03:17); manual | `python` |
| `packaging.yml` | `v*` tags; weekly (Saturday 04:17); manual | `detect`, `crates-dry-run`, `wheel-linux`, `wheel-macos` |
| `memcheck.yml` | weekly (Saturday 05:17); manual | `memcheck` × {mpi, mumps, petsc} |
| `coverage.yml` | weekly (Saturday 06:17); manual | `coverage` |
| `distro.yml` | weekly (Saturday 07:17); manual | `ubuntu-22-04` |
| `expensive.yml` | manual only | `ranks-8`, `miri` |

What each job runs, with its measured wall times: first on an 8-vCPU x86_64
Linux machine where the job's command was replayed, then on GitHub's standard
runner of the public repository (`ubuntu-24.04`, 4 vCPU and 16 GB;
`macos-15` arm64 for `wheel-macos`), in the first runs of 14 November 2026
(`docs/TESTING.md`, "State at release").

| Job | What runs | Time (8-vCPU replay; GitHub) |
|---|---|---|
| `fast` | the `docker/mpi` image, then `run-all.sh --backend docker --preset fast` (lint, test, fixtures, parity, licence) | 1,064 s; 2,126 s |
| `selftest` | `ci/selftest.sh` with shellcheck, PyYAML, jq and actionlint (from its digest-pinned image) | 36 s; 76 s |
| `msrv` | `cargo check` with the toolchain of `workspace.package.rust-version` | 134 s; 138 s |
| `rustdoc` | `cargo doc --workspace --no-deps` with `-D warnings` (HTML artifact, 7 days) | 53 s; 72 s |
| `manual` | `mdbook build docs/manual` with mdBook 0.4.40 (HTML artifact, 7 days) | not measured; 86 s |
| `site`, `deploy` | the manual, rustdoc and the `python` preset (wheel, both pytest runs, pdoc), assembled by `ci/github/pages-site.sh`; the GitHub Pages deployment | 2,969 s, not replayable; 2,820 s, 11 s |
| `features` | the feature matrix in the MUMPS image (PETSc checks in the PETSc image) | 424 s; 603 s |
| `gates` | every line of `ci/tools/feature-gates.txt` in the MPI image | 1,208 s; 1,268 s |
| `facade-release` | the facade's release run (`CI_RELEASE_ONLY=facade`) | not measured; 879 s |
| `exportall` | the workspace tests with `akantu-core/export-all` | 773 s on 4 vCPU; 1,433 s |
| `medium` | mpi, par, mumps, petsc, release and examples legs; exportall leg | 341, 365, 429, 371, 693, 119 s, exportall as above; 521, 578, 775, 489, 997, 323, 1,815 s |
| `python` | the `python` preset on the host: maturin abi3 wheel, `import akantu`, pytest with and without greenlet, pdoc (artifact, 7 days) | 3,170 s; 2,774 s |
| `crates-dry-run` | version check, `cargo package --list` per member with a downstream consumer, direct minimal versions | 149 s; 186 s |
| `wheel-linux`, `wheel-macos` | the `wheel` tier in the digest-pinned manylinux_2_28 image (CPython 3.9 and 3.13 imports, both pytest runs) and on `macos-15` arm64; wheels kept 14 days | 3,255 s, not replayable; 4,242 s, 3,075 s |
| `memcheck` | valgrind memcheck of the FFI code (`ci/README.md`, tier `memcheck`) | 501, 331, 157 s; 714, 420, 440 s |
| `coverage` | `cargo llvm-cov` 0.9.1 over the workspace tests in the MPI image; report only | 2,270 s; 2,125 s |
| `ubuntu-22-04` | `features` and `test` in a digest-pinned `ubuntu:22.04` container on an `ubuntu-24.04` runner (`ci/github/distro-run.sh`), with Ubuntu's Open MPI 4.1, Scotch 6 and Python; MUMPS and PETSc declared not applicable; diagnostic (`continue-on-error`) | 1,407 s; 2,366 s |
| `ranks-8`, `miri` | the `mpi` and `par` tiers at 8 ranks, oversubscribed; Miri | 637 s, 177 s; 819 s, 140 s |

Each job uploads its `summary.json`, step logs and reports as an artifact
(`if: always()`, so a failed or cancelled job keeps them).

Concurrency: `fast.yml` and `docs.yml` cancel the older run of the same pull
request or branch when a newer push arrives. The other workflows queue a run
behind a running one of the same ref (a manual dispatch never cancels the
weekly run).

Caches: every job that loads the project images keeps them with
`actions/cache` (one entry per image set and Dockerfile hash). The jobs that
run cargo on the runner itself (`msrv`, `rustdoc`, `site`, `python`,
`crates-dry-run`, `miri`) also cache the cargo registry (`index` and `cache`,
keyed by `Cargo.lock`), and `manual` and `site` cache the mdBook binary. The
Docker-backend jobs keep their registry in the named volume
`akantu-rs-mpi-cargo`, which the container's user (uid 1000) owns; a cache
restored on the runner (uid 1001) would need its ownership changed, so these
jobs download their crates each time. The cargo target directory is not cached
(21 to 27 GB against the 10 GB cache quota of a repository).

Time limits (`timeout-minutes`): `fast` 75, `selftest` 15, `msrv` 45,
`rustdoc` 45, `manual` 30, `site` 120, `deploy` 10, `features` 90, `gates`
180, `facade-release` 120, `exportall` 120, `medium` 180, `memcheck` 240,
`coverage` 240, `python` 90, `detect` 5, `crates-dry-run` 60, `wheel-linux` 90,
`wheel-macos` 150, `ubuntu-22-04` 120, `ranks-8` 150, `miri` 120. The two
wheel limits were raised after the first runs: the pytest suite is
single-threaded and CPU-bound (89 % of it in seven example runs), and one run
took about 1,940 s on the slower Linux runner.

### Runner minutes

The sums of the measured job times above (first runs on the public
repository, 14 November 2026), before GitHub rounds each job up to a whole
minute. A cold image cache adds the image builds to every job that needs one.

| Event | Jobs | Runner minutes |
|---|---|---|
| each push to a pull request | `fast`, `selftest`, `msrv`, `rustdoc`, `manual` | about 42; the wait is set by `fast` (35 min) |
| each push to `main` | the same, plus `site` and `deploy` | about 89; the wait is set by `rustdoc`, `site` and `deploy`, which run in sequence (49 min) |
| each week | `features`, `medium`, `python`, `packaging`, `memcheck`, `coverage`, `distro` | about 383 Linux minutes (6.4 hours) and 51 macOS minutes for `wheel-macos`; the longest job is `wheel-linux` (71 min) |
| manual | `expensive` | about 16 |

Standard runners are free for public repositories. On a private repository
the plan's minutes are consumed (Linux ×1, macOS ×10).

## Gate map

The table below says, for every check that the port's results rely on, which
workflow job runs it, which tiers that job runs and when. Rows named
`feature-gates.txt line N` are the lines of `ci/tools/feature-gates.txt`; rows
named `tier T` are groups of checks by tier. Format: gate |
command | `file.yml:job` (a comma-separated list for a gate spread over several
jobs) or `none: reason` | tiers of `ci/run-all.sh` | when the job runs: `pr`
(a `pull_request` trigger), `merge` (a push to `main`), `schedule`, `-` (manual
only, or no job).

`ci/tools/gate_map.py` (run by `ci/selftest.sh` and `ci/tools/test_tools.py`)
checks the table against the tree: a new line in the feature-gates list, a
renamed job, a tier that no workflow runs or a claimed trigger the workflow
lacks fails the self-test. It reads each workflow's `on:` block. The rows of
the checks of every pull request (the tiers fmt, clippy, test, fixtures and
fast) must claim `pr`; every feature-gates line must claim `pr` or `schedule`;
a step that runs a tier may carry no `if:`. Path filters are not part of the
claim. A change to the triggers therefore needs the matching change of the
`When` column here.

<!-- GATE-MAP-BEGIN -->
| Gate | Command | Workflow job | Tiers | When |
|---|---|---|---|---|
| feature-gates.txt line 1 | `cargo clippy -p akantu-igfem --all-targets --features cgal --locked -- -D warnings` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 2 | `cargo test -p akantu-igfem --features cgal --locked --no-fail-fast` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 3 | `cargo check -p akantu --features igfem --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 4 | `cargo test -p akantu --features cgal --locked --no-fail-fast` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 5 | `cargo test --release -p akantu-cohesive --locked --no-fail-fast` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 6 | `cargo test --release -p akantu --locked --no-fail-fast` | `features.yml:facade-release` | release | schedule |
| feature-gates.txt line 7 | `cargo clippy -p akantu-python --all-targets --features python --locked -- -D warnings` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 8 | `cargo test -p akantu-python --features python --locked --no-fail-fast` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 9 | `cargo check -p akantu --features extra-materials-non-local --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 10 | `cargo test --release -p akantu-solid --locked --no-fail-fast` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 11 | `cargo test --release -p akantu-diffusion --locked --no-fail-fast --test spatial_convergence` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 12 | `cargo test --release -p akantu-phase-field --locked --no-fail-fast --test spatial_convergence` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 13 | `cargo test --release -p akantu-couplers --locked --no-fail-fast --test spatial_convergence_phase_field` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 14 | `cargo doc --workspace --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 15 | `cargo doc -p akantu-comm --features mpi --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 16 | `cargo doc -p akantu --features mpi --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 17 | `cargo doc -p akantu-bench --features mpi --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 18 | `cargo doc -p akantu --features cgal --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 19 | `cargo doc -p akantu-igfem --features cgal --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 20 | `cargo doc -p akantu-python --features python --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 21 | `cargo doc -p akantu-core --features export-all --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 22 | `cargo doc -p akantu-extra-materials --features damage-non-local --no-deps --locked` | `features.yml:gates` | gates | schedule |
| feature-gates.txt line 23 | `cargo test --release -p akantu-bench --locked --no-fail-fast` | `features.yml:gates` | gates | schedule |
| tier fmt | `cargo fmt --all --check` | `fast.yml:fast` | lint | pr, merge |
| tier clippy | `cargo clippy --workspace --all-targets --locked -- -D warnings` | `fast.yml:fast` | lint | pr, merge |
| tier test | `cargo test --workspace --locked --no-fail-fast` | `fast.yml:fast` | test | pr, merge |
| tier fixtures | `sha256sum -c` of the fixture manifests | `fast.yml:fast` | fixtures | pr, merge |
| tier gates | every command of `ci/tools/feature-gates.txt` | `features.yml:gates, features.yml:facade-release` | gates, release | schedule |
| tier fast | test + fixtures | `fast.yml:fast` | test, fixtures | pr, merge |
| tier exportall | `cargo test --workspace --locked --features akantu-core/export-all` | `features.yml:exportall` | exportall | schedule |
| tier mpi | `scripts/mpi/run-tests.sh` (1/2/4 ranks) | `medium.yml:medium` | mpi | schedule |
| tier par | `scripts/mpi/run-par-tests.sh --scotch` | `medium.yml:medium` | par | schedule |
| tier mumps | `scripts/mumps/run-mumps-tests.sh` | `medium.yml:medium` | mumps | schedule |
| tier petsc | `scripts/petsc/run-petsc-tests.sh` (PETSc image) | `medium.yml:medium` | petsc | schedule |
| tier pytest | `pytest python/tests` in the wheel's environment | `python.yml:python` | wheel | schedule |
| tier ranks-8 | `run-tests.sh --ranks 8`, `run-par-tests.sh --scotch --ranks 8` | `expensive.yml:ranks-8` | expensive | - |
<!-- GATE-MAP-END -->

Lines 11 to 13 of the feature-gates list are the spatial convergence studies of
the diffusion, phase-field and coupler packages; the `gates` tier judges each
`cargo test` line with its own `--no-run` build, the analyser scoped to the
line's package and target, and a floor `gates-N` in `ci/floors.txt`. Lines 14
to 22 are the rustdoc gate: the lints of `[workspace.lints.rustdoc]` are denied,
so a `cargo doc` line fails on a broken or private intra-doc link or an invalid
HTML tag; the tier judges them by exit code, like the clippy and check lines.
Line 14 has the same effect as `docs.yml:rustdoc`; both stay, because the
`gates` leg runs in the project image with the other feature-gated lines and
`docs.yml:rustdoc` runs on every pull request and keeps the HTML. Lines 15 to 22 document the code
behind the features the MPI image can build. Line 6 is run by the `release`
tier's `facade` run (job `facade-release`); it is derived from
`ci/tiers/release.sh`, so narrowing the facade selection there makes `gates`
run the line itself.

Required status checks for `main` (the job names as GitHub shows them):
`fast`, `selftest`, `msrv`, `rustdoc` and `manual`. None of their workflows
has a path filter, so every pull request reports all five. The weekly and
manual jobs stay non-required; a maintainer reads their results and dispatches
them on a pull request's branch when the change needs them.

## Runners and their limits

Only GitHub-hosted runners: `ubuntu-24.04`, the digest-pinned `ubuntu:22.04`
container on an `ubuntu-24.04` runner (the `ubuntu-22.04` runner image is being
retired by GitHub), and `macos-15` for one wheel job (GitHub retired `macos-14`
on 2026-11-02; check that the label is still offered). No self-hosted or larger
runner.

- CPUs and memory. A standard Linux runner of a public repository has 4 vCPU
  and 16 GB; that of a private repository 2 vCPU and about 7 GB.
  `scripts/mpi/in-docker.sh` asks Docker for 4 CPUs and 6 GiB, which a 2-vCPU
  runner refuses. The workflows read the caps from the repository variables
  `AKANTU_RS_DOCKER_CPUS` and `AKANTU_RS_DOCKER_MEMORY` (defaults 4 and `6g`).
  On a public repository leave them unset or set 4 and `6g`: these are the
  container limits under which the test suite is specified (`docs/TESTING.md`,
  a test that needs more than 6 GiB is a defect), and they leave the runner
  about 10 GB for the image loads and the host. On a private repository set 2
  and `6g`.
- Disk. The four project images are 3.7 to 4.4 GB each and the cargo target
  volume of a workspace build reached 21 to 27 GB, against about 14 to 20 GB
  free on a standard runner. Every job that stores images starts with
  `ci/github/free-disk.sh`, which removes the runner image's .NET, Android, GHC
  and CodeQL toolchains and prints `df` before and after (a no-op outside
  GitHub).
- Images. `ci/github/images.sh <cache-dir> [mpi] [scotch] [mumps] [petsc]`
  loads the images from a tar in the cache directory or builds them from
  `docker/` with plain `docker build` and saves them; `actions/cache` keeps one
  entry per image set and Dockerfile hash. There are four image sets (`mpi`;
  `mpi scotch mumps`; `mpi mumps petsc`; `mpi scotch mumps petsc`), each a
  `docker save` tar of images of 3.7 to 4.4 GB that share the MPI image's
  layers, besides the cargo-registry entries. After the first runs the
  repository's cache list held the four image sets at 774, 815, 959 and
  959 MiB (in the order above), the cargo registry at 6 MiB and mdBook at
  4 MiB: about 3.4 GiB for `main`, 4.2 GiB with the entries of the first pull
  request, within the repository's cache quota (10 GB on GitHub Free). When
  the quota is reached GitHub evicts the oldest entries, and a pull request
  whose `mpi` entry was evicted builds the image again. In the first runs the
  cache was cold: the jobs of one image set all built the images in
  parallel, and only the first to finish could save the entry ("Unable to
  reserve cache" in the others' logs). The base image is pinned by digest, apt is
  not version-pinned (D-034), so the image ids differ from the ones recorded
  in `docker/*/image-id.txt`; they are printed and kept in `provenance.txt`.
  Publishing the images to a registry would remove that difference and needs a
  token and a registry decision.
- CPU features. Linux runners are of several machine types, with and without
  AVX-512F. The Python example cases `dynamics` and `custom_material` run only
  where NumPy uses its AVX-512F `exp` kernel (key `numpy_avx512f`,
  `docs/TESTING.md`); a green `python` job does not show that they were
  compared. The `environment` step and the skip check's `note:` lines of each
  run say whether they ran. In the first runs on the public repository the
  `python` job had an Intel Xeon 6975P-C with AVX-512F (both cases compared)
  and `wheel-linux` an AMD EPYC 7763 without it (both skipped).
- Minutes. The first run of every job, on the then private repository, used
  670 Linux minutes and 61 macOS minutes (billed as 610). The first runs on
  the public repository took 487 Linux minutes and 51 macOS minutes of job
  time, free on a public repository. Figures for regular use are in "Runner
  minutes" above.
- Memcheck margins. The memcheck `mpi` leg passed on a 2-vCPU runner only after
  `mpi_progress/delayed_sender_completes` got a 60 s deadline under valgrind.
  `mpi_progress/serialized_progress` passed at a 0.5-CPU cap (28 s for the
  whole scenario, 32 s on the GitHub runner), but how much of its 20 s
  deadlines it uses is not measured; time its phases before relying on the
  weekly job on small runners.

## Repository settings

- Variables (all optional): `AKANTU_RS_DOCKER_CPUS`, `AKANTU_RS_DOCKER_MEMORY`
  (above), and `MANYLINUX_IMAGE`, a digest reference of
  `quay.io/pypa/manylinux_2_28_x86_64`. Without the variable `packaging.yml`
  uses the digest that was reviewed on 2026-11-11:
  `quay.io/pypa/manylinux_2_28_x86_64@sha256:9555ba6c479fa69a2aa80ad1dc57154a60d562b19df159361d9379a02beb1e36`.
  Set the variable to use another digest after reviewing it.
- Private vulnerability reporting: Settings → Code security → Private
  vulnerability reporting = enabled. `SECURITY.md` and `CODE_OF_CONDUCT.md`
  send reporters to "Security → Report a vulnerability", which exists only
  with this setting.
- Discussions: Settings → General → Features → Discussions = enabled.
  `CONTRIBUTING.md` and `.github/ISSUE_TEMPLATE/config.yml` point to them.
- Label: create the label `difference from C++`, which the issue form
  `.github/ISSUE_TEMPLATE/cpp-difference.yml` applies.
- Pages: Settings → Pages → Build and deployment → Source = "GitHub Actions"
  before the first push that runs `docs.yml` on `main`; otherwise the `deploy`
  job fails at `actions/configure-pages` ("Get Pages site failed"). Set on the
  public repository on 14 November 2026; the site is served at
  <https://thomas0903.github.io/akantu-rs/>. The
  `github-pages` environment is created by the first deployment; a custom
  deployment-branch rule must allow `main`.
- Actions policy: allow GitHub-owned actions, or these by commit SHA:
  `actions/checkout` v7.0.1, `actions/cache` v6.1.0,
  `actions/upload-artifact` v7.0.2, `actions/configure-pages` v6.0.0,
  `actions/upload-pages-artifact` v5.0.0, `actions/deploy-pages` v5.0.1. All
  six SHAs equal their release tags (`git ls-remote --tags
  https://github.com/actions/<name>`, 2026-11-14). Dependabot
  (`.github/dependabot.yml`) proposes updates of these pins; copy an accepted
  update into `ci/github/` as well, or the self-test fails.
- Branch protection on `main` (Settings → Branches): a pull request is
  required, the checks `fast`, `selftest`, `msrv`, `rustdoc` and `manual` must
  pass, force pushes and deletion of the branch are blocked, and "Do not allow
  bypassing the above settings" is on, so the rule binds administrators too.
  No approving review is required while the project has one maintainer; raise
  the count when there are more. In force since 15 November 2026. On another
  platform, set the equivalent: a protected default branch that changes only
  through merge requests, with the same five jobs required.
- Coverage has no threshold and is uploaded nowhere; `--fail-under-lines` and an
  upload service are open choices.

Security: no workflow reads `secrets.*` (checked by `ci/selftest.sh`). Every
workflow has `permissions: contents: read`, and so has every job except
`docs.yml:deploy` (`pages: write`, `id-token: write`: the Pages deployment's
OIDC token, no repository secret); `deploy` checks out and runs nothing of the
repository. Checkout uses `persist-credentials: false`. Triggers are
`pull_request` (never `pull_request_target`), `push`, `schedule` and
`workflow_dispatch`. Nothing is published except the documentation site; other
results are workflow artifacts.

Network: jobs download what a developer would: Debian packages and the pinned
`rust:1.99.1-slim-trixie` base image when the image cache is cold (valgrind and
`libc6-dbg` for the memcheck image), crates from crates.io (`cargo install
cargo-llvm-cov` for coverage, mdBook for the site), rustup toolchains (MSRV,
nightly, llvm-tools), maturin, pytest, greenlet, pdoc and ruff from PyPI, the
PyPA manylinux image and the Ubuntu packages of the second distribution.

PETSc: Debian's PETSc links GPL-2+ libraries. The `petsc` tier only runs tests
in an image; a distributed binary built with the `petsc` feature is GPL as a
whole, like one built with `cgal`.

## Replaying a job

`ci/github/replay.sh <workflow>:<job>[:<leg>] <out-dir>` runs the steps of one
job on any Linux machine with Docker, a Rust toolchain and a checkout: the image
step (`ci/github/images.sh` with the same image set, cached in
`~/akantu-images`), the job's environment and its command word for word.
`ci/selftest.sh` checks that every `ci/run-all.sh`, `ci/selftest.sh` and
`ci/github/distro-run.sh` command of a workflow occurs in `replay.sh`, so a
workflow edit that is not mirrored there fails the self-test. Results go to
`<out-dir>/ci-out/`; the exit code is the command's (0 pass, 1 fail, 3
incomplete).

```sh
ci/github/replay.sh fast:fast        "$HOME/replay/fast"
ci/github/replay.sh medium:medium:mpi "$HOME/replay/mpi"
AKANTU_RS_DOCKER_CPUS=2 ci/github/replay.sh memcheck:memcheck:mpi "$HOME/replay/mc"   # a 2-vCPU runner's caps
ci/github/replay-all.sh [--out DIR] [JOB...]   # every job in turn, one REPLAY line each
```

Leg names: `medium:medium:{mpi,par,mumps,petsc,release,examples,exportall}`,
`memcheck:memcheck:{mpi,mumps,petsc}`. `packaging:wheel-macos` needs a macOS
runner and cannot be replayed on Linux; the same tier runs on a Mac with
`CI_WHEEL_COMPAT=off ci/run-all.sh --preset python`. The `deploy` job (the
Pages actions, the environment, the OIDC token) runs only on GitHub.

A replay on a machine that has built the project before can hide what a clean
runner shows: the first GitHub runs failed on a registry that lacked packages
the test build never downloads and on a CPU without AVX-512F, neither of which
the earlier replays had. To approach a clean runner, use a fresh cargo registry
volume and the runner's container caps.

## State

Every job's command passed when replayed on the release commit
(`docs/TESTING.md`, "State at release"). On 14 November 2026 every workflow
was dispatched once on `main` of the public repository (commit `f0877f053`;
the ten runs are listed in `docs/TESTING.md` and public in the Actions tab),
and all 30 jobs passed at their first attempt, including `wheel-macos` and
the Pages deployment. A run
on the then private repository earlier that day had 21 of 29 jobs green; its
causes were fixed before publication. The parity rows CI-001, CI-014 and
CI-024 are *Verified* on these runs (`ci/MAPPING.md`). The first Dependabot
pull request failed `selftest` as designed until its bumps were copied into
`ci/github/` (`docs/MAINTAINING.md`, "Updating a dependency without changing
results").
