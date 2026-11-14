# Oracle build (C++ reference)

The oracle is the C++ Akantu library built from the reference commit in a
pinned Docker image. It produces the golden data that the differential tests
of the port compare against (`fixtures/`, a few crate-local fixture files and
`oracle/results/`). This document says how to build it, how to run its tests
and probes, and how to regenerate and check fixtures. The comparison method is
in `docs/NUMERICAL_VALIDATION.md`; how the Rust tests read fixtures is in
`docs/TESTING.md`.

Everything here runs in Docker on Linux (x86_64 or aarch64). The host scripts
run with bash 3.2 or newer, so they also work from macOS with a Linux VM
behind Docker. Heavy steps (the library build, full ctest runs, fixture
campaigns) need a machine with at least 16 GB of RAM available to Docker.

## The reference and the two platforms

| Item | Value |
|---|---|
| Reference | `35a851780d0b9471d258ed53a003ae98aaa95ea0` (`v5.0.7-post1-554-g35a851780`, Akantu 5.0.7-post1; `docs/REFERENCE.md`) |
| Local checkout | path in `AKANTU_SRC_HOST`; mounted read-only at `/akantu-src` |
| Authoritative platform | x86_64 Linux (D-043), image `akantu-oracle:dev` for linux/amd64, id `sha256:2da26d87e063f3c88264f589cf73381feff583eeaa7770d2e01ae034651d59f5` (`oracle/docker/image-id.amd64.txt`) |
| First platform | aarch64 Linux, image id `sha256:6683233d0a3b9341eb5cedf5879bc9ccb76ddf3d91e9aa0fd35b99dc7866d7a6` (`oracle/docker/image-id.txt`); built and first run under Docker on an Apple M4 Pro |

Every oracle script refuses a reference checkout that is not exactly the
pinned commit or that has modified or untracked files. Set `AKANTU_SRC_HOST`
before running any of them:

```sh
git clone https://gitlab.com/akantu/akantu.git "$HOME/akantu-ref"
git -C "$HOME/akantu-ref" checkout --detach 35a851780d0b9471d258ed53a003ae98aaa95ea0
export AKANTU_SRC_HOST="$HOME/akantu-ref"
```

The fixture corpus was generated on aarch64 first and regenerated on x86_64
with the pins described below. On the pinned x86_64 oracle every case is
value-identical to the aarch64 goldens except the classes of differences
listed in `docs/NUMERICAL_VALIDATION.md` (MUMPS solves, default-NaN signs,
complex division, Gmsh meshes, uninitialised memory, MPI line order). A
case whose values depend on the platform keeps the x86_64 values at the top
level and the aarch64 values in `variants/aarch64-auto/` (D-073; loader:
`crates/akantu-validation/src/fixture.rs`). `docs/PLATFORMS.md` describes the
platforms from the user's side.

## Image

`oracle/docker/Dockerfile` derives from upstream's
`test/ci/debian:stable/Dockerfile` (documentation and lint tools dropped). Base
`debian:trixie@sha256:9c6e381421d0f48d1b8133e573f33c185ee16cbf505eda9c4a8a4791b25ea8a6`
(a multi-architecture index). Non-root user `akantu` (uid/gid 1000); Open MPI
settings for containers in the image environment (`btl=tcp,self,sm`, no CMA,
oversubscription allowed, no binding); `OPENBLAS_NUM_THREADS=OMP_NUM_THREADS=1`.
The amd64 image adds `OPENBLAS_CORETYPE=Haswell` (D-050, below).

| Component | Version (Debian package) |
|---|---|
| GCC / G++ / gfortran | 14.2.0 (`14.2.0-19`), the compiler of every build |
| CMake / Ninja | 3.31.6 / 1.12.1 |
| glibc / binutils | 2.41 / 2.44 |
| Open MPI | 5.0.7 (MPI standard 3.1) |
| MUMPS | 5.7.3 (`libmumps-dev 5.7.3-3+b1`, parallel, double) |
| Scotch / PT-Scotch | 7.0.7 |
| PETSc | 3.22.5 (`libpetsc-real3.22-dev`, real scalars, 32-bit indices) |
| ScaLAPACK | 2.2.2 (openmpi) |
| BLAS / LAPACK | OpenBLAS 0.3.29 pthread variant (selected by `update-alternatives`) / LAPACK 3.12.1 |
| Boost / Eigen | 1.83.0 / 3.4.0 (`libeigen3-dev 3.4.0-5`) |
| CGAL | 5.6.2 header-only, upstream release tarball (SHA-256 `2d93d96c8c73c73c9999556d512775648d26e1e04035ceb958208d25e6e2d10f`) at `/opt/CGAL-5.6.2`; Debian's 6.0.1 is removed because `src/geometry` does not compile with it (D-032) |
| Python / NumPy / SciPy / mpi4py / pybind11 | 3.13.5 / 2.2.4 / 1.15.3 / 4.0.3 / 2.13.6 |
| googletest | 1.16.0 sources of Debian in `/usr/src/googletest` (version of the recorded image, not pinned in the Dockerfile) |
| Gmsh | 4.13.1 |
| Tools | ccache 4.11.2, gdb 16.3, GNU time, xsltproc, jq, rsync, git |

The package manifests are `oracle/docker/packages.tsv` (arm64) and
`oracle/docker/packages.amd64.tsv`. Both architectures resolve the same
(package, version) pairs apart from architecture-named cross packages,
binNMU suffixes and six amd64-only runtime libraries
(`oracle/results/x86_64/packages-amd64-vs-arm64.diff`).

Reproducibility scope: the base image digest, the CGAL tarball, the reference
commit, the oracle patch and the CMake initial caches are pinned. The 56
Debian packages named in the Dockerfile (569 with their dependencies) are not
version-pinned, so their versions are fixed only inside a recorded image. A
rebuild that resolves other versions is a different numerical environment, and
fixtures from it must be regenerated before use. `oracle/scripts/build-image.sh`
builds `akantu-oracle:dev-candidate`, writes its manifest to
`oracle/docker/packages.candidate.tsv` (git-ignored) and compares it with the
recorded one; it records nothing without `--promote`, and on package drift it
exits 3 and promotes only with `--accept-package-drift`. The manifest, not the
image id, identifies a rebuild (a cached rebuild gets a new id). Configure runs
without network. Exact clean rebuilds would need a dated `snapshot.debian.org`
source, which was not set up.

## Build configurations

Two configurations with the same feature set, each loaded as a CMake initial
cache on top of `oracle/configs/common.cmake`
(`cmake --fresh -C oracle/configs/<config>.cmake -G Ninja`). Source: a copy of
the reference in the build volume (`/build/src-<config>`) with
`oracle/patches/0001-tests-dereference-mpiexec-max-numprocs.patch` applied
(test registration only, D-033). Install prefix `/build/install-<config>`.

| Configuration | Flags | Use |
|---|---|---|
| `strict` | upstream flags plus `-ffp-contract=off -DEIGEN_DONT_VECTORIZE` | every golden fixture |
| `default` | upstream flags | measuring the floating-point variation between two legitimate builds |

Effective flags of a library translation unit in `strict`:

```
g++ … -ffp-contract=off -DEIGEN_DONT_VECTORIZE  -O2 -g -DNDEBUG -g3 -ggdb3 -flarge-source-files
    -std=c++20 -flto=auto -fno-fat-lto-objects -fPIC
    -Wall -Wextra -pedantic -Wno-array-bounds -Wno-use-after-free -Wno-stringop-overread
    -DOMPI_SKIP_MPICXX -Wno-literal-suffix -frounding-math
```

`-frounding-math` comes from CGAL's interface target. On x86_64 both
configurations also carry the Eigen cache pin of D-050. Rationale for `strict`:
D-031 and `oracle/fp-probe/README.md`.

Feature set (both configurations, `oracle/configs/common.cmake`):
`RelWithDebInfo`, where `AKANTU_NDEBUG` is not defined, so Akantu's debug
assertions stay active (they can turn silent upstream inconsistencies into
exceptions, UD-078); tests including unstable ones (upstream CI leaves those
off); examples registered as tests; Python; parallel; implicit with
`Mumps+PETSc`; every model package, non-local damage, dumpers, debug tools, CGAL
geometry and traction at split-node contact; `MPIEXEC_MAX_NUMPROCS=8`, so with
patch 0001 the parallel tests run on 8, 4, 2 and 1 ranks; LTO for `libakantu`
as upstream. Off: `embedded`, `extra_materials`, `extra_materials_non_local`
and `IGFEM` (they do not compile at the reference commit; the patched variants
below make parts usable), `documentation`, `AKANTU_BENCHMARK`.

### x86_64 pins (D-050)

Two run-time choices would make an x86_64 oracle depend on its host CPU:

| Pin | Where | Why |
|---|---|---|
| Eigen cache sizes `-DEIGEN_NO_CPUID -DEIGEN_DEFAULT_L1_CACHE_SIZE=16384 -DEIGEN_DEFAULT_L2_CACHE_SIZE=524288 -DEIGEN_DEFAULT_L3_CACHE_SIZE=524288` | `oracle/configs/common.cmake` when the build host is x86_64; every probe takes the install's flags (`build-info.txt`, `oracle-flags.cmake`) | Eigen 3.4 sizes its GEMM and triangular-solve blocking (also inside `SparseLU`) from the cache sizes. On x86_64 it queries CPUID (64 KiB / 512 KiB / 16 MiB on the Zen 5 hosts used); on aarch64 it uses 16 KiB / 512 KiB / 512 KiB. The pin gives the aarch64 values on every host, which is what the port's Eigen translation reproduces. |
| `OPENBLAS_CORETYPE=Haswell`, `OPENBLAS_NUM_THREADS=1`, `OMP_NUM_THREADS=1` | amd64 image environment and `ORACLE_ENV_ARGS` of `oracle/scripts/oracle-env.sh`, passed by every script that starts an oracle container | OpenBLAS picks its kernels from the host CPU otherwise; MUMPS and PETSc results change with the kernel set (5 result groups over 8 core types, up to 5.9e-13 relative). |

The cache sizes are one process-wide static that `libakantu.so` exports, so a
probe compiled without the pin could change them inside the pinned library.
Every C++ fixture probe includes
`oracle/probes/include/akantu_probe/eigen_pin.hh` after its Akantu headers; on
x86_64 it refuses to compile without the four flags and measures the effective
sizes, and the fixture writer refuses to write unless they are
16384 / 524288 / 524288 (recorded as
`build.libraries.eigen_cache_sizes_measured`). Diagnostics that force other
sizes (`oracle/x86/fp-probe/eigen-cache-run.sh`) set
`AKANTU_PROBE_ALLOW_UNPINNED_EIGEN=1`. Older probes run against an install built
before the compile-time pin pin the sizes at run time with
`Eigen::setCpuCacheSizes(16384, 524288, 524288)` as the first statement of
`main` and record that in their manifests.

`oracle-env.sh` refuses an amd64 host without every x86-64-v3 feature
(`avx avx2 bmi1 bmi2 f16c fma movbe xsave abm`): OpenBLAS's Haswell kernels need
AVX2 and FMA, and glibc's libm selects other ifunc variants without them (27 of
604 `libm.json` leaves change). `ORACLE_ALLOW_NON_V3=1` allows a
non-authoritative diagnostic run. On x86_64 every manifest records
`openblas_coretype`, `openblas_num_threads`, `eigen_cache_sizes`, `host_cpu` and
`x86_64_level`.

The pinned libraries contain no `cpuid` instruction (the unpinned
`libakantu.so` had 7), and the pinned x86_64 oracle reproduces the aarch64
goldens of `fixtures/solver` bitwise (15 cases, 468 quantities) on two hosts.

## Building

```sh
oracle/scripts/build-image.sh                 # candidate image + manifest comparison (exit 3 on package drift)
oracle/scripts/build-image.sh --promote       # tag akantu-oracle:dev, rewrite the image id record
oracle/scripts/configure-build.sh strict -j3  # detached: copy + patch source, configure, ninja, install
docker logs -f akantu-oracle-build-strict     # follow the build
oracle/scripts/finalize-install.sh strict     # oracle-flags.cmake / env.sh, C++ and Python smoke tests, READY marker
oracle/scripts/configure-build.sh default -j3 # the same for `default`, then finalize-install.sh default
```

`build-image.sh` builds for the Docker server's platform (`ORACLE_PLATFORM`
overrides) and keeps one record per architecture. `common.sh` sends results
of an amd64 oracle to `oracle/results/x86_64/` (override with
`ORACLE_RESULTS_DIR`) so they never overwrite the aarch64 records in
`oracle/results/`.

`configure-build.sh` options: `-j N` (default 2), `--configure-only`,
`--build-only`, `--install-only`, `--foreground`. A failed ninja run is retried
once with `-j1` in case a translation unit was OOM-killed. Failing test or
example targets do not stop the install while `akantu` and `py11_akantu` are
complete; they are listed in `/build/logs/<config>/failed-targets.txt` and the
container exits 1. At the reference commit the only failing target is
`test_material_mazars` (it does not compile upstream). Full clean rebuild:
`docker volume rm akantu-oracle-build` (removes every build tree, install and
log).

Memory and time, measured:

| Build | Parallelism | Wall time | Memory |
|---|---|---|---|
| aarch64 `strict`, Apple M4 Pro, empty ccache | `-j2` | 41 min 21 s | container peak 10.35 GiB, largest process 7.8 GB |
| aarch64 `default` | `-j3` | 30 min 03 s | container peak 12.56 GiB, largest process 7.4 GB |
| amd64 `strict` pinned, 8 vCPU AMD EPYC 9455, 16 GB + 16 GB swap | `-j3` | 50 min 32 s | largest process 6.75 GB |
| amd64 `default` | `-j3` | 46 min 18 s | largest process 7.02 GB |

A translation unit needs up to about 7.8 GB, and a container at `-j3` reached
12.6 GiB. Use at most `-j3` and run one heavy C++ build at a time. A build tree
takes about 12 GB, an install 1.2 GB.

The build volume `akantu-oracle-build` (`/build`) holds the build trees
`/build/<config>`, the installs, the logs (`/build/logs/<config>/`:
`steps.log`, `configure.log`, `CMakeCache.txt`, `build.log`, `install.log`,
GNU time records) and the patched source copies. Probes, generators and the
dense records need only the installs; ctest, the `.verified` checks, the
`--standalone` probes and the variant rebuilds need the `strict` build tree.

`oracle/scripts/in-container/fp-verify.sh <config>` counts fused
multiply-add instructions in the installed libraries (`objdump -d`). On aarch64
`strict` has 14 in `libakantu.so.5.0.7`, all in libgcc's complex-division helper
`__divdc3`, and `default` 5,445 (Eigen kernels, `sparselu_gemm`,
`computeStress`, shape derivatives, Jacobians) plus 611 in `py11_akantu`. On
x86_64 both have none (baseline x86-64 has no FMA; `default` differs from
`strict` there by Eigen's SSE2 vectorisation). Fused arithmetic left in a
`strict` run comes from system libraries (OpenBLAS, MUMPS, PETSc, Scotch, libm),
which matters for implicit solves (D-031).

## Using the install

`/build/install-<config>/READY` exists once `finalize-install.sh` has checked
the install with a C++ consumer (`oracle/consumer-smoke`: `find_package(Akantu)`
and a static solve) and a Python import.

```sh
oracle/scripts/shell.sh                              # shell with /akantu-src, /build, /oracle mounted
. /build/install-strict/share/akantu-oracle/env.sh   # CMAKE_PREFIX_PATH, LD_LIBRARY_PATH, PYTHONPATH
```

A CMake consumer (mount its sources with `ORACLE_EXTRA_MOUNTS="-v $PWD/probe:/work"`):

```cmake
cmake_minimum_required(VERSION 3.16)
project(probe CXX)
# Imports Akantu from the install tree with the work-arounds for upstream's
# AkantuConfig.cmake and applies the oracle compile flags.
include(/build/install-strict/share/akantu-oracle/use-akantu-oracle.cmake)
add_executable(probe probe.cc)
target_link_libraries(probe PRIVATE akantu)
```

`oracle-flags.cmake` appends the oracle `CMAKE_CXX_FLAGS` and
`-frounding-math`, forces C++20 and sets the build type to `RelWithDebInfo`
when none is given. Compiling against `install-strict` without
`-DEIGEN_DONT_VECTORIZE` is an ODR and ABI violation (Eigen alignment).
`share/akantu-oracle/build-info.txt` records the exact flags and package set.

## Upstream's tests on the oracle

```sh
oracle/scripts/run-ctest.sh strict -R '^test_solid_mechanics_model_linear_elastic'  # subset, foreground
oracle/scripts/run-ctest.sh strict --detach -j 12    # full suite in the background
oracle/scripts/run-ctest.sh strict --collect         # copy the summary to oracle/results/
oracle/scripts/check-verified.sh strict              # upstream's .verified files, fresh executions
oracle/scripts/run-isolated.sh strict oracle/isolated/ob04-strict.txt --repeats 3 [--aka-seed 1]
```

Each run gets its own `/build/logs/<config>/ctest/<stamp>/` (`Test.xml`,
`junit.xml`, `tests.json`, `summary.json`, `ctest.log` and an immutable copy of
every `*.lastout` and `*.csv` with a SHA-256 manifest). Defaults:
`ctest -T test --output-on-failure --no-compress-output --timeout 3600`. ctest
schedules MPI tests by their rank count, so `-j 12` never runs more than 12
ranks at once. A single test by hand: `cd /build/strict && ctest -R <name> -V`
shows the driver command; MPI runs use `mpiexec --oversubscribe -n <N>`.

Results of the full suite (1,611 tests: 1,416 registered by upstream and 195
extra 2, 4 and 8-rank variants from patch 0001):

| Run | Passed | Failed | Not run |
|---|---|---|---|
| aarch64 `strict` | 1,529 | 57 | 25 |
| aarch64 `default` | 1,528 | 58 | 25 |
| x86_64 `strict` pinned (`-j 4`, 533 s) | 1,528 | 58 | 25 |
| x86_64 `default` pinned (`-j 3`, 713 s) | 1,529 | 57 | 25 |

Every test of the feature set that upstream CI builds passes. The 7 failures
among upstream-registered tests are 5 tests marked `UNSTABLE` upstream and 2
CGAL geometry tests (CGAL is not built by upstream CI). The only per-test
difference between the runs is the intermittent `Communicator.ReceiveAny_4` /
`_8` (UD-076). Every failure has a cause category in
`oracle/results/known-failures.json` (strict, 57: genuine parallel failure 35,
of which 34 are implicit dynamics; sync contract unresolved 6; test bug 5;
unsupported path 3; unfinished path 3; numerical discrepancy 2; library bug 2;
debug check 1). Under D-035 the genuine parallel failures are excluded from
golden data. All 27 registered examples pass, and 22 of the 23 Python tests
and examples (`test_linear_anisotropic_explicit_python` fails on its input
registration).

`oracle/scripts/in-container/compare-outputs.py` compares the printed outputs
of two build trees or two runs (anchored, file-scoped masks for timings, dates
and addresses, each counted). Between `strict` and `default` on aarch64, 24 of
161 compared files differ numerically, 21 of them below 1e-15 norm-wise and the
largest at 6.2e-8 (a post-convergence Newton residual); between two full
`strict` runs no printed number of the 99 numeric single-writer captures
differs (`oracle/results/strict-vs-default.json`,
`oracle/results/strict-run1-vs-run2.json`). These compare printed values only,
not full-precision state.

Upstream's driver never compares the `.verified` reference files (UD-018).
`check-verified.sh` runs each test fresh and compares: aarch64 `strict` 14/14,
aarch64 `default` 12/14 (two non-local weight tests differ by FMA), x86_64
`strict` and `default` 14/14.

Repeatability: Akantu seeds its random generator from `time(nullptr)` unless
`AKA_SEED` (or `--aka_seed`, or a `seed` in the input file) is given, and that
seed drives Scotch partitioning. A fixed seed is not enough: Scotch 7
partitions with several threads by default. Every multi-rank oracle run used
for fixtures or timings sets `SCOTCH_PTHREAD_NUMBER=1` and records the
partition. Two-rank MUMPS solves are not bitwise reproducible from run to run
(energies differ from about 1e-14 at step 0); one-rank runs are. Akantu's MUMPS
out-of-workspace retry returns a wrong solution (UD-216), so runs that take it
are invalid. Open MPI's `prterun` can hang after every rank has exited
(environment, not Akantu; `docs/DEPENDENCIES.md`).

## Probes and fixture generation

A probe is a small C++ or Python program under `oracle/probes/<area>/` built
against the `strict` install. It runs an upstream code path on chosen inputs
and writes a fixture case (`manifest.json`, `data/`, `inputs/`) through
`oracle/probes/include/akantu_probe/fixture_writer.hh` or
`oracle/probes/python/akantu_probe.py`. Fixture format and provenance fields:
`docs/NUMERICAL_VALIDATION.md`.

Two entry points:

```sh
# the generic generator (targets: selftest element fe_engine fe_extra mesh material model
# cl_random core_init parsable all, or one case: mesh:<id>, material:<id>, model:<id>)
oracle/scripts/generate-fixtures.sh [--config strict] [--out DIR] <target>...
oracle/scripts/generate-fixtures.sh --dry-run selftest     # checks the command line, no Docker

# an area's own generator
oracle/probes/<area>/generate.sh [--out DIR]
```

The area generators share these switches (the prefix names the area, for
example `SMM`, `COH`, `DIFF`, `SOLVER_MODEL`): `<AREA>_PROBE_CASES=<id>[,<id>…]`
generates and promotes only the listed cases and leaves the others untouched;
`<AREA>_PROBE_STAGE_ONLY=1 … --out <dir>` stages without promoting. Other
switches are in each script's header. The generic generator marks the area
`fe` complete (its stale-case check) only when `element`, `fe_engine` and
`fe_extra` run in the same invocation.

Rules every generator follows:

- Each case is run at least twice (three times for multi-rank and some area
  cases, in separate container starts) and nothing is promoted unless every run
  is identical (D-035). Expected run-to-run differences are listed in
  `oracle/probes/python/known_nondeterminism.json` (reviewed changes only).
- Finalisation (`oracle/probes/python/finalize_fixtures.py`) checks the whole
  candidate tree (checksums, materialisation, size policy, stale cases) and
  swaps it in transactionally: a failed finalisation leaves `fixtures/`
  unchanged. A leftover `fixtures/.finalize-transaction/` (a killed process)
  blocks the next run: restore the paths in its `journal.json` from its `old/`
  directory (or `git checkout fixtures`) and remove it.
- `fixtures/SHA256SUMS` (and the other `SHA256SUMS` files under `fixtures/`)
  is rewritten by finalisation; only the lines of regenerated cases change.
- The writer records the image id, the install's scripts commit
  (`/build/install-<config>/READY`), the architecture and the pins.
  `build_env.py --expect-image` matches only the recorded image;
  `AKANTU_ALLOW_IMAGE_MISMATCH=1` records instead of refusing an install built
  in another image.
- Never overwrite a committed golden in place with values from another
  platform. Generators finalise into `fixtures/` of the checkout they run in:
  run them in a scratch clone and copy back only the cases you own.
- Gmsh output depends on the platform (D-009). The meshes used by fixtures are
  frozen in `fixtures/meshes/` and regenerated only with
  `oracle/probes/mesh_io/freeze-meshes.sh` (its default output is
  `target/frozen-meshes`; an amd64 host refuses to write `fixtures/meshes`).

Python unit tests of the generation scripts (determinism filter, transactional
finalisation, provenance, driver grammar) run without Docker:

```sh
cd oracle/probes/python && python3 -m unittest discover -s tests -t tests -v
```

Whole-corpus regeneration on x86_64 (lanes may run as separate processes;
finalisation serialises them with a lock on the fixtures root):

```sh
git clone <your checkout> ~/stage
oracle/x86/regenerate-corpus.sh ~/stage ~/stage-logs [core model mesh misc]
oracle/x86/compare-corpus.sh <your checkout> ~/stage ~/cmp ~/stage-logs/meshes
python3 -I oracle/x86/summarize.py ~/cmp/fixture-diff.json ~/stage/fixtures
```

Known generator limits on x86_64: the mesh_io probe can refuse to promote
(UD-094 makes `diana_3d*` differ between its two runs), `oracle/probes/dense/run.sh`
needs the `default` install, and the `regression` case cannot be replayed.

### Generators recorded under an older path

Some fixture manifests and provenance files record the path their
generator had when the fixtures were made, in a directory that is not part
of the public tree. The generators are in the tree at these paths. Their comments were
updated; in the scripts, the recorded command names the new path,
`AKANTU_SRC_HOST` has no default and the scripts that ran from the old
directory read their sources from the new one. The probe programs are
unchanged:

| Recorded path | Generator now | Fixtures |
|---|---|---|
| `docs/reviews/2026-11-11-smm-2-review-artifacts/v87_probe/` | `oracle/probes/v87/` | `fixtures/model/v87/*` |
| `docs/reviews/2026-11-11-verify-4-apply-recheck-artifacts/v104_probe/` | `oracle/probes/v104/` | `crates/akantu-core/tests/data/oracle/v104_printself.txt`, `crates/akantu-linalg/tests/data/oracle/v104_sort_ties.txt` |
| `docs/reviews/2026-11-11-followups-2-review-artifacts/oracle/` | `oracle/probes/periodic_order/` | `fixtures/mesh_utils/periodic_order/` |
| `docs/reviews/2026-11-11-mesh-2-review-artifacts/adversarial-cases.txt` | `oracle/probes/mesh2/adversarial-cases.txt` | the adversarial cases of `fixtures/mesh2/` |
| `docs/reviews/2026-11-11-verification-audit-4-artifacts/pin_probes.py`, `adv_pf_cases.sh` | `oracle/probes/phase_field/adversarial/` | `fixtures/model/phase_field/patch_*_linear_voldev_history` |
| `docs/reviews/2026-11-11-verify-4-fix-recheck-artifacts/cdiv/` | `oracle/probes/dense/complex_guard/` | `crates/akantu-linalg/tests/fixtures/dense/complex_div_guard_x86_64.txt` |

Some fixture notes also cite review reports by path. Those reports are not
in the public tree; what they established is stated in the notes and in
the tolerance files.

The provenance notes and manifests under `fixtures/` were written when each
fixture was recorded. They are kept as recorded, apart from the removal of
machine identifiers, so a note may refer to a check or a record made during
the port that is not part of the repository. The table above maps the
recorded generator paths to the current ones.

## Checking that a regeneration is byte-identical

A regeneration of committed cases must give the committed bytes. To check:

1. Regenerate into a scratch directory (`--out DIR`, or
   `<AREA>_PROBE_STAGE_ONLY=1`), never into `fixtures/`.
2. Compare the SHA-256 of every regenerated data and input file with its line
   in `fixtures/SHA256SUMS`, and run `diff -r` of each case directory against
   the committed one (this also finds files on one side only).
3. `manifest.json` records the run (creation date, command line with `--out`,
   probe version and sources, host CPU): compare manifests as JSON and accept
   differences in exactly those fields.
4. For a difference, `cargo run -p akantu-validation --bin fixture-diff --
   <root_a> <root_b> [--json] [--case <glob>]` lists the numerical
   differences per quantity (exit 0: compared; 1: structural problems or
   nothing selected; 2: usage error or missing root). A differing case is
   investigated before anything is promoted.

The final check of the release regenerated five cases on the pinned x86_64
oracle (a coupler case, an 8-rank explicit solid case, a 2-process Newton
trace, a serial cohesive case, an 8-rank split-node contact case): 112, 282,
118, 50 and 421 files byte-identical, manifests equal apart from provenance.

## Patched oracle variants

Some evidence needs a C++ build that differs from the regular `strict` oracle
by a minimal patch. Two kinds exist; both are applied only to copies of the
reference sources (never to the reference checkout), and their fixtures record
the patch and its hash (`parameters.oracle_variant`, `reference.patches`).

Variants that define upstream undefined behaviour (D-003 category A), in
`oracle/patches/variants/`, follow these rules. One defect per patch, the
minimal change, a header naming the UD id; the regular build never applies
them. They are built only on a disposable machine: the script patches the
volume's source copy `/build/src-strict` and rebuilds incrementally (at most
`-j3`), after the regular cases have been generated; then `material_cases.py`
refuses regular goldens on the variant build (and variant cases on a regular
one) unless `AKANTU_PROBE_ALLOW_ON_VARIANT=1` stages regular cases for the
inertness check. Goldens are labelled (`parameters.oracle_variant`, a
description naming the variant) and live in `…/variant_<defect>/`, except the
seven UD-001 goldens, which predate the rule and live beside the regular cases.
Regular cases regenerated on the variant build must be data-identical to their
regular run. D-035 (three identical runs in separate container starts) and the
D-050 pins apply.

| Patch | Defect | Build | Goldens |
|---|---|---|---|
| `ud031-anisotropic-damage-zero-init.patch` | UD-031: `A_diag` read uninitialised in `tensorPlusOp` of `material_anisotropic_damage_tmpl.hh`; one line `A_diag.zero();` | `AKANTU_DISPOSABLE_HOST=yes oracle/scripts/build-ud031-variant.sh -j3` (with the next patch; `VARIANT_PATCHES=ud031` for this one alone) | `fixtures/material/anisotropic_damage_mazars/variant_ud031/` (7 cases) |
| `ud814-eig-noconvergence-zero-init.patch` | UD-814: `MatrixBase::eig` returns uninitialised eigenpairs when Eigen's solver does not converge; the patch returns zeros, as the rest of the function does when the solver's two unwritten members are zero | rebuilds every translation unit (2,685 s at `-j3` on 8 vCPU) | same cases |
| `ud001-contact-zero-init.patch` | UD-001: the four missing zero-initialisations in contact detection and penalty resolution | `AKANTU_DISPOSABLE_HOST=yes oracle/probes/contact/build-ud001-variant.sh` (`-j1`) | `model/contact/{explicit_friction, adversarial_curved3d_friction, adversarial_tri_edges, adversarial_tri_fallback, adversarial_tri_valley}`, `model/couplers/{contact_static_friction, friction_cohesive_contact_explicit}` |

The scripts check `AKANTU_DISPOSABLE_HOST=yes` to confirm that they run on a
disposable machine, since they modify the build volume's source copy and
install. The UD-031 script writes `/build/install-strict/VARIANT` (patches,
hashes of the installed and regular libraries), which `material_cases.py`
checks before it writes variant goldens, and installs the library with the
install RUNPATH.

Variants that make a package usable that does not compile or run at the
reference commit (D-060), in `oracle/patches/<variant>/`. The patch changes no
formula (API adaptation, build fixes, behaviour of a released upstream version,
zero-initialisation of memory read uninitialised, each tied to a UD); upstream's
own tests of the package pass on it with their own criteria. They are built
consumer-side: a script copies the affected upstream sources into a scratch
tree inside the oracle container, applies the series and compiles them with the
probe against the unchanged `strict` install and its flags.

| Directory | Patches | Purpose | Build |
|---|---|---|---|
| `oracle/patches/embedded/` | `0001`…`0008` (named-argument arity UD-073, API adaptations UD-025 / UD-587, a destructor definition, the pre-stress internal of v5.0.0 UD-588, the 1-D reinforcement law of v5.0.0 UD-589, a test functor signature, a test-driver file name); combined SHA-256 `fca0995b2042020a180057fab9aae348e28e471376ab0f23a21edfd74f22bedc` | the embedded interface model (`fixtures/model/embedded/`); UD-590 is not patched (it is in every released version), so every probe case has one reinforcement law and one background element type | `oracle/probes/embedded/generate.sh --out <dir>` (`ninja -j1`: three translation units in parallel were OOM-killed in a 6 GB container) |
| `oracle/patches/geometry/` | `0001` compile fixes of `MeshSphereIntersector` (UD-409), `0002` zero fill (UD-414) | the header-only sphere intersector, used by the geometry probes | `oracle/probes/geometry/generate.sh` |

`oracle/patches/unused/0002-embedded-fix-named-argument-arity.patch` is not
applied by any build (the embedded series carries the same change as `0001`).
The hunk-by-hunk reasons of each patch are in the patch files' header comments
and in `docs/UPSTREAM_DEFECTS.md`.

## Valgrind on probes

Valgrind is not in the oracle image. It is installed into a throwaway root
container of the image, so the image itself is never changed:

```sh
docker run --rm --init --user 0 -v akantu-oracle-build:/build:ro \
  -e OMPI_ALLOW_RUN_AS_ROOT=1 -e OMPI_ALLOW_RUN_AS_ROOT_CONFIRM=1 \
  -e OPENBLAS_CORETYPE=Haswell -e OPENBLAS_NUM_THREADS=1 -e OMP_NUM_THREADS=1 \
  akantu-oracle:dev bash -c 'apt-get update -qq && apt-get install -y -qq valgrind &&
    valgrind --tool=memcheck --track-origins=yes <probe> <arguments>'
```

`AKANTU_DISPOSABLE_HOST=yes oracle/probes/couplers/memcheck-friction.sh
<variant|regular> <steps> <out>` is a complete example (it mounts the
reference, the build volume read-only, the probes and the frozen meshes): it
builds the probe as the generator does, runs it once
under memcheck with a scratch fixtures root (the staged case is labelled as a
memcheck run and never promoted; no value is taken from it) and writes
`vg.log`, `run.log`, `summary.txt` and the library hash. Results on record:
the UD-001 variant, 5,100 steps of `friction_cohesive_contact_explicit`, 0
errors from 0 contexts; the regular install, 2,300 steps, 15 errors from 3
contexts, all on the never-initialised `slave_normal`. The anisotropic damage
cases with both UD-031 and UD-814 patches: 0 errors in 13 cases; with UD-031
alone memcheck found the uninitialised eigenpairs that became UD-814.

## Known upstream build problems

| Problem | Workaround |
|---|---|
| `IGFEM`, `extra_materials(_non_local)`, `embedded` do not compile | off in the build (D-032); `embedded` usable through its patched variant |
| `src/geometry` incompatible with CGAL 6 | CGAL 5.6.2 in the image (D-032) |
| parallel tests registered with 1 rank only (`set(N MPIEXEC_MAX_NUMPROCS)`, UD-015) | patch 0001 and `MPIEXEC_MAX_NUMPROCS=8` (D-033) |
| googletest fetched from an unpinned `main`; `find_package(gtest)` does not find Debian's config | FetchContent from `/usr/src/googletest` 1.16.0 (D-034); the built `libgtest.so` reports 1.15.0, googletest's own sub-project version string |
| the Python module is installed to `lib/python./site-packages` and nested one level too deep, so `import akantu` fails from the install | `finalize-install.sh` builds a package view of symlinks in `share/akantu-oracle/python/akantu`; `env.sh` puts it on `PYTHONPATH` |
| the exported `akantu` target links `Boost::graph` and `CGAL`, but `AkantuConfig.cmake` imports neither (UD-065) | consumers call `find_package(Boost REQUIRED COMPONENTS graph)`, `find_package(Eigen3 REQUIRED NO_MODULE)` and `find_package(CGAL REQUIRED)` before `find_package(Akantu)`, or include `use-akantu-oracle.cmake` |
| `test_material_mazars` does not compile (`Array<UInt>&` bound to `Array<Idx>`) | listed as the one failed target |
| `AKANTU_BENCHMARK` points to the missing `test/benchmark` | not enabled |

The parallel cohesive suite is not built upstream (its directory is commented
out and its tests name a non-existent package), nor are the heat-transfer patch
tests (UD-014, UD-019); the oracle builds neither. `nproc` prints 1 in the
image because GNU `nproc` honours `OMP_NUM_THREADS=1`; no oracle script relies
on it.
