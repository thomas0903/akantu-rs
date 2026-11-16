# Dependencies and licence audit

This file lists everything akantu-rs depends on or derives from, with its
licence: the Rust crates, the optional native libraries, the third-party
code ported into the repository, the behaviour reproduced clean-room, the
licence consequences of the optional Cargo features, and the dependencies
of the C++ reference that the oracle is built with. `NOTICE.md` carries the
notices that must ship with the code.

Reference commit of upstream Akantu: `35a851780d0b9471d258ed53a003ae98aaa95ea0`
(`v5.0.7-post1-554-g35a851780`). Versions of Debian packages are those of
the recorded oracle image `akantu-oracle:dev` (Debian trixie 13.7; full
list in `oracle/docker/packages.tsv`, image id in
`oracle/docker/image-id.txt`) and of the project's test images under
`docker/`. Debian packages are not version-pinned in the Dockerfiles: the
versions are fixed only within those images, and a rebuild may resolve
others (`docs/ORACLE_BUILD.md`). Licence facts for Debian packages were
read from `/usr/share/doc/<pkg>/copyright` in the images; CGAL licence facts
from the SPDX headers of the CGAL 5.6.2 release.

## 1. Licence of Akantu and of akantu-rs

| Item | Fact |
|---|---|
| `COPYING` | GNU GPL version 3, 29 June 2007 (sha256 `8ceb4b9e…5eb65b903`) |
| `COPYING.lesser` | GNU LGPL version 3, 29 June 2007 (sha256 `da7eabb7…64b4464768`) |
| File headers | "Copyright (©) <years> EPFL (Ecole Polytechnique Fédérale de Lausanne) Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)" + "Akantu is free software … GNU Lesser General Public License … either version 3 of the License, or (at your option) any later version" → LGPL-3.0-or-later |
| `AUTHORS` | Same notice (2010-2015), list of ~40 EPFL authors |
| Other holders | None found: every copyright line in `src/`, `test/`, `examples/`, `python/`, `cmake/`, `packages/`, `extra_packages/`, `third-party/` names EPFL/LSMS only (grep of all `Copyright` lines). No MIT/BSD/Apache/MPL-licensed code is vendored. |
| `third-party/iohelper` | Separate EPFL project (v1.1.1), own `COPYING`/`COPYING.lesser`, headers "IOHelper is free software … LGPL version 3 or later" → LGPL-3.0-or-later |
| `third-party/akantu_iterators` | Separate EPFL project, own `COPYING`/`COPYING.lesser`, headers "akantu-iterators … LGPL version 3 or later" → LGPL-3.0-or-later |
| `cmake/Modules/Find*.cmake` | EPFL-copyrighted, LGPL-3.0-or-later (not copies of Kitware/third-party modules) |
| Python package metadata | `setup.py`/`pyproject.toml`; `python/akantu/__init__.py` `__copyright__` = EPFL, license LGPL |

Consequence for akantu-rs: a translation is a derivative work of
LGPL-3.0-or-later code (D-001). The repository keeps `COPYING` and
`COPYING.lesser` verbatim, carries the EPFL notice in `NOTICE.md`, and every
source file written for the port has an SPDX header and a line naming the
Akantu file it is ported from. Files copied unchanged from elsewhere keep
their original notices instead: upstream's Python examples under
`python/examples/` and the benchmark driver
`benchmarks/cohesive-perf/src/cohesive_extrinsic.cc` keep Akantu's headers,
and the benchmark data files copied without a notice are described in
`NOTICE.md`.

Package licences of the workspace crates: every crate is
`LGPL-3.0-or-later` except `akantu-core` and `akantu-linalg`
(`LGPL-3.0-or-later AND MPL-2.0`: they contain the MPL-2.0 Eigen
translations of section 4) and `akantu-geometry-cgal` (`GPL-3.0-or-later`,
section 6).

### License compatibility notes for an LGPL-3.0-or-later Rust port

- Compatible for inclusion/linking: MIT, BSD-2/3, Apache-2.0 (compatible with
  (L)GPLv3), MPL-2.0 (file-level copyleft, compatible), BSL-1.0, ISC, Zlib,
  Unicode-3.0 (permissive data licence of the Unicode tables in
  `unicode-ident`; compile-time only; listed in `ci/licence-policy.toml`),
  LGPL-2.1-or-later/LGPL-3.0, CeCILL-C (weak copyleft for the component;
  keep MUMPS/Scotch as separately linked libraries and ship their notices).
- Avoid as dependencies of the library: GPL-only code (Gmsh library), which
  would force GPL on distributed binaries; "commercial-or-GPL" dual licenses.
  Exception (D-042): CGAL's GPL AABB tree / spherical kernel are ported into
  the opt-in GPL-3.0-or-later crate `akantu-geometry-cgal`.
- Rust links crates statically: LGPL relinking obligations then apply to the
  akantu-rs code itself (users must be able to rebuild with a modified
  akantu-rs, which shipping the source and the Cargo build satisfies).
  Document this in `NOTICE.md` when binaries are distributed.
- Every adopted crate goes into this file with version and license; the CI
  licence tier (`ci/tools/licence_audit.py`) checks every resolved crate
  against the allow-list of `ci/licence-policy.toml`, which matches the list
  above.

## 2. Rust crates

### Direct dependencies

Requirements are those of the workspace `Cargo.toml`; locked versions are
those of `Cargo.lock` at the time of writing.

| Crate | Requirement / locked | Licence | Used by | Build |
|---|---|---|---|---|
| `thiserror` | 2.0.21 / 2.0.21 | MIT OR Apache-2.0 | `akantu-core` (`Error`) | default |
| `faer` | 0.24.4 / 0.24.4 (`default-features = false`, `std`, `sparse-linalg`) | MIT | `akantu-linalg` (`FaerLuSolver`, the `faer` port extension: COLAMD ordering and the sequential simplicial LU; the triangular solves on its factors are the port's own code) | default |
| `log` | 0.4.29 / 0.4.35 | MIT OR Apache-2.0 | warnings of readers and parser, `akantu_core::debug::LogBridge` (routes `log` records into upstream-formatted debug output); dev-dependency of the facade | default |
| `serde`, `serde_json` | 1.0.228 / 1.0.229, 1.0.149 / 1.0.154 (`float_roundtrip`) | MIT OR Apache-2.0 | `akantu-validation` (fixtures, manifests); dev-dependency of most crates | default (validation), dev |
| `sha2` | 0.10.9 | MIT OR Apache-2.0 | `akantu-validation` (checksums); dev-dependency of several crates | default (validation), dev |
| `proptest` | 1.11.0 / 1.12.0 | MIT OR Apache-2.0 | property tests (`akantu-core`, `akantu-io`, `akantu-parser`, `akantu-ntn-contact`) | dev |
| `tempfile` | 3.27.0 | MIT OR Apache-2.0 | `akantu-validation` tests | dev |
| `mpi` (rsmpi) | 0.8.2 (`default-features = false`: no `user-operations`/`libffi`, no `derive`) | MIT OR Apache-2.0 | `akantu-comm`, `akantu-linalg`, facade: `mpi::ffi` raw bindings, `initialize_with_threading` / `Universe`, `is_initialized` / `is_finalized` / `threading_support`; tests also use its safe API | feature `mpi` |
| `pyo3` | 0.30.1 (`abi3-py39`) | MIT OR Apache-2.0 | `akantu-python`: extension module `py11_akantu`, classes, exceptions, callbacks | feature `python` |
| `numpy` (rust-numpy) | 0.30.0 | BSD-2-Clause | `akantu-python`: NumPy C-API, arrays over export leases (`PyArray_NewFromDescr` + base object), input conversion | feature `python` |
| `cc` | 1.2.66 / 1.11.1 | MIT OR Apache-2.0 | build scripts of `akantu-comm` (`src/mpi_shim.c`), `akantu-mesh` (`src/scotch_shim.c`), `akantu-linalg` (`src/mumps/shim.c`, `src/petsc/shim.c`) | features `mpi`, `scotch`, `mumps`, `petsc` |
| `pkg-config` | 0.3.33 / 0.3.34 | MIT OR Apache-2.0 | build script of `akantu-linalg` (locates PETSc) | feature `petsc` |

Notes:

- The thread pool of the execution policy (D-123) is the port's own
  (`akantu_core::exec`, on `std::thread`). `rayon` (MIT OR Apache-2.0) was
  allowed and measured, and not taken: one region cost 5–14 µs with it
  against about 1 µs with the own pool on the 8-core measurement machine
  (`docs/benchmarks/perf-campaign/exec/`). `rayon` remains a dependency of
  the standalone microbenchmark `scripts/perf/exec/pool-overhead` only (not
  a workspace member, not shipped).
- faer's SIMD kernels use FMA internally. That is acceptable only because
  faer is qualified by the conditioning-aware protocol of
  `docs/NUMERICAL_VALIDATION.md` and never compared bitwise; it is never
  selected by `auto` (D-040).
- `serde_json` enables `float_roundtrip` in the workspace and in
  `akantu-validation`'s own declaration (identical parsing in a single-crate
  build). With it, serde_json parses floats correctly rounded: the default
  parser read `1.6271440317223231e-07` as `0x3e85d6d306bf4482` instead of
  `0x3e85d6d306bf4481`. Every crate that reads JSON numbers through
  serde_json is affected (feature unification), in the direction of the
  decimal text actually written. The fast-tier test
  `serde_json_reads_numbers_correctly_rounded` fails without the feature.
  The feature adds no dependency.
- Test-only crate feature: `serde_json` `unbounded_depth`
  (dev-dependency of `akantu-parser`) to read deeply nested oracle dumps.
- The C shims are akantu-rs code (LGPL-3.0-or-later); each build script
  acts only under its `CARGO_FEATURE_*`, so the default build compiles and
  links no native code.

### Every resolved third-party crate

Generated with `cargo metadata --format-version 1 --all-features` over the
committed `Cargo.lock` (all features, all dependency kinds, build tools
included: 156 crates); `/` in a declared licence is read as `OR`.

| Declared licence | Crates (locked version) |
|---|---|
| MIT OR Apache-2.0 | bitflags 2.13.2, block-buffer 0.10.4, borsh 1.8.1, build-probe-mpi 0.1.6, cc 1.11.1, cfg-if 1.0.5, chacha20 0.10.2, conv 0.3.3, cpufeatures 0.2.17, cpufeatures 0.3.0, crypto-common 0.1.7, custom_derive 0.1.7, digest 0.10.7, either 1.15.0, enum-as-inner 0.6.1, errno 0.3.14, generic-array 0.14.7, getrandom 0.4.2, glob 0.3.3, half 2.7.1, heck 0.5.0, indoc 2.0.7, itertools 0.13.0, itoa 1.0.18, lazy_static 1.5.0, libc 0.2.191, log 0.4.35, mpi 0.8.2, mpi-sys 0.2.4, num-complex 0.4.6, num-integer 0.1.46, num-traits 0.2.19, once_cell 1.21.4, paste 1.0.15, pkg-config 0.3.34, portable-atomic 1.14.0, ppv-lite86 0.2.21, proc-macro2 1.0.107, proptest 1.12.0, pulp 0.22.3, pulp-wasm-simd-flag 0.1.1, pyo3 0.30.1, pyo3-build-config 0.30.1, pyo3-ffi 0.30.1, pyo3-macros 0.30.1, pyo3-macros-backend 0.30.1, quote 1.0.47, rand 0.10.1, rand_chacha 0.10.0, rand_core 0.10.1, rand_xorshift 0.5.0, regex 1.12.4, regex-automata 0.4.15, regex-syntax 0.8.11, rustc-hash 2.1.2, rustc_version 0.4.1, seq-macro 0.3.6, serde 1.0.229, serde_core 1.0.229, serde_derive 1.0.229, serde_json 1.0.154, sha2 0.10.9, shlex 1.3.0, smallvec 1.16.5, syn 2.0.119, syn 3.0.10, tempfile 3.27.0, thiserror 1.0.69, thiserror 2.0.21, thiserror-impl 1.0.69, thiserror-impl 2.0.21, typenum 1.20.0, version_check 0.9.5 |
| MIT | bytes 1.12.1, cfg_aliases 0.2.2, crunchy 0.2.4, dyn-stack 0.13.2, dyn-stack-macros 0.1.3, equator 0.2.2, equator 0.6.0, equator-macro 0.2.1, equator-macro 0.6.0, faer 0.24.4, faer-traits 0.24.0, gemm 0.19.0, gemm-c32 0.19.0, gemm-c64 0.19.0, gemm-common 0.19.0, gemm-f16 0.19.0, gemm-f32 0.19.0, gemm-f64 0.19.0, generativity 1.2.1, libm 0.2.16, memoffset 0.9.1, nano-gemm 0.2.2, nano-gemm-c32 0.2.2, nano-gemm-c64 0.2.2, nano-gemm-codegen 0.2.2, nano-gemm-core 0.2.2, nano-gemm-f32 0.2.2, nano-gemm-f64 0.2.2, ndarray 0.17.2, nom 7.1.3, numpy-macros 0.1.0, qd 0.8.0, raw-cpuid 11.6.0, reborrow 0.5.5, sysctl 0.6.0, unarray 0.1.4, wait-timeout 0.2.1, winnow 1.0.5, zmij 1.0.21 |
| Apache-2.0 OR MIT | autocfg 1.5.1, bit-set 0.11.1, bit-vec 0.10.1, cexpr 0.6.0, equivalent 1.0.3, fastrand 2.5.0, fnv 1.0.7, hashbrown 0.17.1, indexmap 2.14.2, matrixmultiply 0.3.10, minimal-lexical 0.2.1, num-bigint 0.4.6, quick-error 1.2.3, rawpointer 0.2.1, semver 1.0.28, toml 1.1.2, toml_datetime 1.1.1, toml_parser 1.1.2, toml_writer 1.1.1 |
| Unlicense OR MIT | aho-corasick 1.1.5, byteorder 1.5.0, memchr 2.8.3, same-file 1.0.6, walkdir 2.5.0, winapi-util 0.1.11 |
| Apache-2.0 | borsh-derive 1.8.1, clang-sys 1.9.1 |
| Apache-2.0 WITH LLVM-exception OR Apache-2.0 OR MIT | linux-raw-sys 0.12.1, rustix 1.1.6 |
| BSD-2-Clause OR Apache-2.0 OR MIT | zerocopy 0.8.63, zerocopy-derive 0.8.63 |
| Zlib OR Apache-2.0 OR MIT | bytemuck 1.25.2, bytemuck_derive 1.12.1 |
| (MIT OR Apache-2.0) AND Unicode-3.0 | unicode-ident 1.0.27 |
| Apache-2.0 WITH LLVM-exception | target-lexicon 0.13.5 |
| BSD-2-Clause | numpy 0.30.0 |
| BSD-3-Clause | bindgen 0.72.1 |
| ISC | libloading 0.8.9 |
| MIT OR Apache-2.0 OR LGPL-2.1-or-later | r-efi 6.0.0 |

`bindgen`, `clang-sys`, `libloading`, `cexpr`, `nom`, `regex` and related
crates are build-time only (the `mpi-sys` build step, which also compiles a
small C shim `rsmpi.c` with `mpicc` exposing MPI constants); they are not
linked into akantu-rs binaries.

### Tools that are not dependencies

| Tool | Licence | Use |
|---|---|---|
| maturin 1.16.0 (`python/pyproject.toml` requires `>=1.9,<2`) | MIT OR Apache-2.0 | builds the Python wheel; not linked |
| mdBook | MPL-2.0 | builds the user manual on demand (`mdbook build docs/manual`); nothing of it is redistributed |
| `cargo-llvm-cov` | MIT OR Apache-2.0 | coverage job of the CI |

## 3. Native libraries (optional, dynamically linked)

None of these is needed by the default build, and none is copied,
translated or redistributed. Binaries built with a native feature link the
library dynamically; whoever distributes such a binary must ship the
notices of the linked libraries.

| Library | Version in the test images | Licence | Feature | How it is used |
|---|---|---|---|---|
| Open MPI | 5.0.7 (Debian trixie `libopenmpi40` / `libopenmpi-dev` 5.0.7-1, same as the oracle image) | BSD-3-Clause (+ embedded components under BSD-style licenses: PMIx, PRRTE, hwloc) | `mpi` | the MPI implementation of the Linux test image; any MPI-3 implementation works (MPI-3 non-blocking collectives and matched probes are required) |
| libclang (build host only) | 19 (`libclang-dev` 1:19.0-63) | Apache-2.0 WITH LLVM-exception | `mpi` | bindgen's parser for `mpi.h`; not linked into akantu-rs binaries |
| Scotch | 7.0.7 (Debian trixie `libscotch-dev`, 32-bit `SCOTCH_Num`; image `docker/scotch`) | CeCILL-C | `scotch` | `libscotch`/`libscotcherr` linked by the shim `crates/akantu-mesh/src/scotch_shim.c`; partitions equal the oracle's for the same graph, seed and `SCOTCH_PTHREAD_NUMBER=1` (Scotch 7 is not reproducible multi-threaded) |
| MUMPS (double, parallel `libdmumps`, `libmumps_common`) | 5.7.3 (Debian `libmumps-5.7` / `libmumps-dev` 5.7.3-3+b1, headers `libmumps-headers-dev` 5.7.3-3; 32-bit `MUMPS_INT`; image `docker/mumps`) | CeCILL-C (PORD: public domain) | `mumps` (needs `mpi`) | dynamically linked (`-ldmumps`) by the C shim `crates/akantu-linalg/src/mumps/shim.c`; the shim (LGPL-3.0-or-later, our code) only sets/reads documented `DMUMPS_STRUC_C` fields through the installed `dmumps_c.h` |
| ScaLAPACK (OpenMPI) | 2.2.2-1 | BSD-3 | `mumps` | through MUMPS only |
| PT-Scotch / Scotch | 7.0.7-1 | CeCILL-C | `mumps` | through MUMPS orderings only (`SCOTCH_PTHREAD_NUMBER=1` in the image) |
| METIS | 5.1.0.dfsg-7+b2 | Apache-2.0 | `mumps` | through MUMPS orderings only |
| OpenBLAS (pthread variant, selected as `libblas.so.3`/`liblapack.so.3` alternative) | 0.3.29+ds-3 (`OPENBLAS_NUM_THREADS=1`) | BSD-3 | `mumps`, `petsc` | through MUMPS/ScaLAPACK/PETSc only |
| PETSc | 3.22.5 (Debian trixie `petsc-dev` = `libpetsc-real3.22-dev`, real scalars, 32-bit `PetscInt`, Debian's configure options, the oracle's package; image `docker/petsc`, recorded in `docker/petsc/{packages.tsv,petsc-configure.txt}`) | BSD-2-Clause | `petsc` | the C shim `crates/akantu-linalg/src/petsc/shim.c` (plain C types only, ported from Akantu's PETSc classes, LGPL-3.0-or-later) compiled with `cc` and the MPI compiler wrapper against the system PETSc found with `pkg-config` (`PETSc`/`petsc`); KSP, SNES (`newtonls`), TAO (`gpcg`); bitwise with the oracle's PETSc in serial (`docs/NUMERICAL_VALIDATION.md`); its own linked libraries and their GPL consequence: section 6 |
| CPython | ≥ 3.9 (abi3; tested with 3.15.2) | PSF-2.0 | `python` | runtime (dynamically loaded host interpreter) |
| NumPy, SciPy (Python packages) | tested with 2.5.4 / 1.18.1 | BSD-3-Clause | `python` | runtime dependencies of the `akantu` package (as upstream `install_requires`) |
| pytest | tested with 9.2.0 | MIT | tests | Python tests |
| mpi4py (test only) | 4.0.3 (`python3-mpi4py` 4.0.3-4) | BSD-3-Clause | tests | mpi4py-first lifecycle scenario (`scripts/mpi/mpi4py_first.py`) |
| python3-gmsh (test only) | 4.13.1 (Debian trixie `python3-gmsh` 4.13.1+ds1-6, with its `libgmsh4.13` and OpenCASCADE dependencies) | GPL-2.0-or-later with the Gmsh exception (dependencies: OpenCASCADE LGPL-2.1 with exception, others per package) | tests | the Python `gmsh` module of the derived test image `akantu-oracle-gmsh` (`FROM akantu-oracle:dev` + `apt-get install python3-gmsh`) that runs `python/tests/test_gmsh_conversion.py` and the oracle snippet `python/oracle/snippets/gmsh_convert.py` (PY-095); imported by the tests only: not linked, not shipped, and the fixture `fixtures/python/gmsh_convert/oracle.txt` holds numbers from a model built through the API, no Gmsh code |
| Rust toolchain image | `rust:1.99.1-slim-trixie` (digest in `docker/mpi/Dockerfile`) | Rust: MIT OR Apache-2.0; Debian packages: per package | test images | test image base |

The resolved package set of each image is recorded next to its Dockerfile
(`docker/mpi/packages.tsv`, `docker/mumps/packages.tsv`,
`docker/petsc/packages.tsv`; `scripts/mpi/build-image.sh`).

### MPI

The backend calls MPI through rsmpi's generated raw bindings; no
third-party source is copied into akantu-rs. `crates/akantu-comm/build.rs`
compiles `src/mpi_shim.c` (akantu-rs code, LGPL-3.0-or-later: returns
`MPI_DOUBLE_INT`, `MPI_MINLOC`, `MPI_MAXLOC`, `MPI_IN_PLACE` and the C layout
of the double/int pair for the `NativeMpi` reduction policy, D-039) with
`$MPICC` (default `mpicc`); it is a no-op without the feature.
`akantu-comm` also declares the C library's exit-handler registration for
the MPI finalisation hook (`release_at_exit`, D-067): glibc's `on_exit` on
glibc Linux (GNU extension, used from the on_exit(3) manual page only,
D-030) and `atexit` (ISO C 7.22.4.2) on other targets, both linked through
std's libc.

Test-only C helpers (akantu-rs code, LGPL-3.0-or-later):
`scripts/mpi/openmpi_self_truncation.c` (pure-C probe of the Open MPI
truncated-receive overrun below, self and inter-process, compiled and run
by the MPI tier with `mpicc`, result recorded as provenance);
`scripts/mpi/abort_shim.c` (PMPI interception of `MPI_Abort` through
`LD_PRELOAD`, recording the calling thread's main-thread status and thread
level for the abort-path scenarios); `scripts/mpi/pmix_abort_shims.c`
(`PMIx_Abort` interposers that never return or fail at once, for the MPI
tier's adversarial runs).

#### Known defect: Open MPI 5.0.7 overruns truncated receives

- Defect. In the pinned Open MPI 5.0.7 (Debian trixie 5.0.7-1, pml
  `ob1`), a message longer than the matching posted receive is written
  past the receive buffer on two transports:
  - btl `self` (a process's message to itself): below the eager limit
    it is silently truncated (success, count = posted); at 4 KiB and above,
    `MPI_ERR_TRUNCATE` is returned after the whole message was written
    past the buffer: 4094 / 65534 / 1048574 bytes past a 2-byte receive
    for 4 KiB / 64 KiB / 1 MiB, at 1 process and for self-messages at 2/4
    processes;
  - btl `tcp` (inter-process): from 64 KiB on (rendezvous),
    `MPI_ERR_TRUNCATE` after 65534 / 1048574 bytes written past a 2-byte
    receive for 64 KiB / 1 MiB; 64 B and 4 KiB (eager) are truncated
    cleanly. Through a receive path that posts fixed-size receives this
    segfaults at 2 and 4 ranks (`--mca btl tcp,self`; fault addresses made
    of payload bytes).
  The shared-memory btl (`sm`) truncates cleanly at every size tested.
  TCP is the transport of multi-node runs.
- Reproducer. `scripts/mpi/openmpi_self_truncation.c` (pure C, no akantu
  code; self case at 1 process, inter-process case rank 1 → rank 0 at 2);
  the MPI tier builds it and runs it at 1 process and at 2 processes over
  `sm` and over `tcp` on every run, and records its
  `OMPI_TRUNCATION peer=… size=… class=… overrun=…` lines in the run's
  `provenance.txt`, so an image upgrade shows whether the defect is still
  present.
- Upstream status. Not found in Open MPI's public trackers (one web
  search, 2026-11-10); whether a newer 5.0.x release fixes it is
  unverified (it needs another Open MPI build than the pinned package).
- Workaround in akantu-rs, independent of the transport. MPI never gets
  a receive buffer shorter than the matched message: (1) messages to self
  never enter MPI: every send to the own rank, every receive and probe from
  the own rank and the self part of an any-source probe go through an
  in-process channel with MPI's semantics
  (`crates/akantu-comm/src/self_channel.rs`); (2) every receive from
  another rank, posted or dynamic, is a slot of a matching engine
  (`mpi_backend::PostedEngine`) queued per (source, tag) in posting order
  and filled by `MPI_Improbe` + `MPI_Imrecv` of exactly the matched length;
  a message longer than its posted receive is delivered whole and reported
  as an ordinary truncation error. No `MPI_Irecv`/`MPI_Cancel` is used.
  Regressions: `mpi_self_messages` `self_4k`/`self_64k`/`self_1m` and
  `cross_4k`/`cross_64k`/`cross_1m` (shared memory and TCP) under a
  red-zone allocator, 1/2/4/8 ranks; with fixed-size posted receives every
  `self_*` aborts and `cross_64k`/`cross_1m` segfault over TCP.
- Oracle exposure. Upstream Akantu posts fixed-size `MPI_Irecv`s
  (`asyncReceive`) for self and inter-process messages. Its receive
  buffers are sized from the exchanged schemes, so the overrun needs a
  truncated message, i.e. an upstream protocol error; an upstream protocol
  error over TCP with messages of 64 KiB or more would corrupt the oracle's
  heap rather than fail cleanly. Whether any upstream scheme contains the
  own rank (e.g. periodic synchronisation at one process) was not
  verified.

#### Known defect: PRRTE 3.0.8 `prterun` teardown hang after simultaneous non-zero exits

- Defect. Open MPI 5.0.7's launcher (`prterun`, PRRTE 3.0.8, BSD;
  Debian trixie 5.0.7-1) occasionally hangs in its teardown when every
  rank of a job exits with a non-zero code at the same time without
  finalising: all ranks have exited (zombies), the job's exit code is set,
  but `prterun` never returns. Measured with akantu's emergency abort at
  8 ranks (`mpi_abort_paths worker_abort_funneled`, each rank `_exit(71)`):
  9/2,000 runs and 5/2,200 before the workaround below.
- Stack (gdb with Debian debuginfod inside the container, 3/3
  symbolised hangs). `prterun`'s main thread waits in `futex_wait` ←
  `pthread_mutex_lock` ← `pmix_obj_update(object, inc=-1)`
  (`pmix_object.h:757`) ← `cleanup_job` (`state_dvm.c:888`) ←
  `event_base_loop` ← `main` (`prte.c:1242`); the PMIx progress thread is
  idle in `epoll_wait`, the OOB listener in `select`.
- Mechanism (source reading, PRRTE 3.0.8 / OpenPMIx 5.0.7).
  errmgr's `TERM_NON_ZERO` and `KILLED_BY_CMD` paths activate the job's
  `TERMINATED` state on `num_terminated >= num_procs`
  (`errmgr_dvm.c:399-401, 540-542`) and its default branch on `==`
  (`:566`); every activation retains the job object in its own caddy
  (`state_base_fns.c:77-83`), and `cleanup_job` drops two references
  (`state_dvm.c:888` plus the caddy destructor, `state_base_frame.c:147-152`;
  `PMIX_RELEASE` NULLs the pointer only when it frees,
  `pmix_object.h:508-525`). Duplicate termination processing therefore
  over-releases the job, and the second `cleanup_job` locks the mutex of a
  freed object. The exact reference-count interleaving was not proven (no
  ASan/rr).
- Versions. The code paths are unchanged in PRRTE 3.0.12 (the diff
  3.0.8 → 3.0.12 touches psets cleanup, an fd helper and a help message),
  so a newer Open MPI 5.0.x does not fix it as far as known.
- Upstream status. No PRRTE issue or commit addressing it was found
  (the 3.0.8 → 3.0.12 diff and the commit messages were read); no report to
  the PRRTE project is recorded.
- Sources consulted. PRRTE 3.0.8 and 3.0.12 release sources
  (`src/mca/errmgr/dvm/errmgr_dvm.c`, `src/mca/state/dvm/state_dvm.c`,
  `src/mca/state/base/state_base_fns.c`, `state_base_frame.c`), OpenPMIx
  5.0.7 (`src/class/pmix_object.h`, `include/pmix.h` for the `PMIx_Abort`
  signature, `include/pmix_common.h` for `pmix_status_t`), Open MPI 5.0.7
  `ompi/runtime/ompi_rte.c` (`ompi_rte_abort`: `PMIx_Abort`, a 100 µs
  pause, `_exit`), the PMIx Standard v4/v5 (`PMIx_Abort`: `procs = NULL`,
  `nprocs = 0` = the caller's namespace).
- Workaround in akantu-rs (D-069). The non-MPI emergency fallback (a
  thread that may not call `MPI_Abort`) asks the launcher to abort the job
  through `PMIx_Abort(code, msg, NULL, 0)`, the PMIx half of Open MPI's own
  `MPI_Abort`, before `_exit(code)`, with a 2 s bound. The launcher then
  tears the job down through its abort path instead of the
  simultaneous-exit accounting: 0/2,000 hangs (interleaved with 9/2,000
  before) and 0/5,000. The symbol is resolved and its helper thread started
  once when MPI starts, so the abort path does no loader work, creates no
  thread and allocates nothing before the bound. Without a PMIx client
  (other MPIs, Open MPI 4's renamed internal PMIx, static linking) the
  fallback is the plain `_exit` as before.
- Remaining exposure. Any other case where every rank exits non-zero
  at once still meets the defect: panics on all ranks (exit 101),
  `std::process::exit` on all ranks, failing oracle test binaries. akantu
  installs no such abort request in a panic hook; an opt-in panic hook for
  binaries the project owns that makes the same prepared `PMIx_Abort`
  request was considered and is not implemented.
- The MPI tier and the launcher's teardown. The tier's time-bounded
  launcher rows measure when the last rank process ended
  (`scripts/mpi/abort_watch.py`) and bound that, require the launcher's
  exit status, and record the launcher's teardown after the last rank in
  `launch-times.tsv`; a launcher that never returns still fails at the
  runner's timeout. The hang was not reproduced in 3,880 watched runs
  (`docs/TESTING.md`). One run (the rank-0 interposer form at 2 ranks on a
  4-vCPU machine) returned the right code 71 after 100.5 s; the rank
  processes cannot outlive the fallback's 2 s bound.
- Also: the image's `prterun` 3.0.8 dumps core on every multi-program
  (MPMD) launch; the MPI tier does not use MPMD launches for that reason.

### MUMPS

MUMPS is CeCILL-C, an LGPL-compatible weak copyleft; linking it
dynamically through the shim's FFI keeps it a separate component. The
backend uses its own C shim and `extern "C"` declarations (no bindgen, no
third-party crate). The default build compiles and links nothing of MUMPS (`build.rs` acts only
under `CARGO_FEATURE_MUMPS`). The shared objects of the test image are
byte-identical to the oracle image's (SHA-256 of `libdmumps-5.7.1.so`
`e26ca085…`, `libmumps_common` `39a4b84d…`, `libopenblas.so.0` `46b0e90c…`,
`libscotch-7.0.so` `4803b6a7…`, `libmetis.so.5` `143ee203…`,
`libscalapack-openmpi.so.2.2` `0d4f7321…`, `libmpi.so.40` `f41d5a36…`,
checked 2026-11-11), which is what the observed bitwise agreement with the
oracle's MUMPS solves rests on. The resolved package set is
`docker/mumps/packages.tsv` (image id in `docker/mumps/image-id.txt`); the
BLAS/LAPACK alternatives are recorded in the image
(`/usr/share/akantu-rs-mumps-alternatives.txt`).

Build-time: `cc` compiles the shim with `$MPICC` (default `mpicc`);
`AKANTU_MUMPS_INCLUDE_DIR`, `AKANTU_MUMPS_LIB_DIR` and `AKANTU_MUMPS_LIB`
override the layout. The sequential MUMPS library (Debian
`libmumps-seq-dev`, upstream's `_not_parallel` / `ICNTL(18) = 0` path of a
non-MPI build) is not supported: the backend needs an MPI communicator,
and `auto` falls back to `eigen` on serial and in-process communicators
(`SparseSolverFactory::resolve_available`). A sequential build would link
`libdmumps_seq` + `libmpiseq` instead and pass a fake communicator; it is
not provided because the oracle uses the parallel library on every process
count. Distribution note: binaries built with the feature link MUMPS
dynamically; ship MUMPS's CeCILL-C notice with them.

Test-only oracle helper `oracle/probes/mumps/icntl14_interposer.c`
(LGPL-3.0-or-later, our code): an `LD_PRELOAD` wrapper of `dmumps_c` used to
drive the unmodified oracle library through its workspace-retry path
(UD-216); built against the oracle image's MUMPS headers.

## 4. Ported third-party code

Code translated from third-party projects, with the licence of the
translated files. Every translated file reproduces the complete notice
block of every source file it derives from (generated from the sources,
not retyped) and states that the code was modified; `NOTICE.md` lists the
holders.

| Origin | Version | Licence of the origin | Licence of the akantu-rs files | Files |
|---|---|---|---|---|
| Eigen | 3.4.0 | MPL-2.0 | MPL-2.0 | `crates/akantu-linalg/src/dense/eigen/{mod,real_schur,self_adjoint}.rs`, `crates/akantu-core/src/eigen_gebp.rs`, `crates/akantu-linalg/src/eigen_sparse_lu/{mod,kernels,colamd,etree,dense}.rs` |
| SuperLU, COLAMD (carried by Eigen's sparse LU) | SuperLU 3.0–4.1, COLAMD 2.3 | permissive notices | MPL-2.0 with the notices retained | `crates/akantu-linalg/src/eigen_sparse_lu/{kernels,colamd,etree}.rs` |
| GNU C Library libm | 2.41 | LGPL-2.1-or-later | LGPL-3.0-or-later (the "any later version" clause) | `crates/akantu-core/src/random/{libm,libm_tables}.rs`, `crates/akantu-core/src/glibc_libm/{sincos,sincos_table}.rs` |
| Boost.Spirit | 1.83 | BSL-1.0 | BSL-1.0 notices kept | `crates/akantu-parser/src/expr.rs` (number literals), `crates/akantu-parser/src/grammar.rs` (position counting) |
| CGAL (LGPL packages) | 5.6.2 | LGPL-3.0-or-later OR LicenseRef-Commercial | LGPL-3.0-or-later | `crates/akantu-geometry/src/{kernel.rs,intersections.rs,number/}` |
| CGAL (GPL packages) | 5.6.2 | GPL-3.0-or-later OR LicenseRef-Commercial | GPL-3.0-or-later | `crates/akantu-geometry-cgal/src/{aabb,spherical,sphere_intersector}.rs` |
| IOHelper (EPFL) | 1.1.1 | LGPL-3.0-or-later | LGPL-3.0-or-later | `crates/akantu-io/src/dumper/{base,base64,cfmt,lammps,paraview,text,vtk}.rs` |
| Akantu (EPFL) | reference commit | LGPL-3.0-or-later | LGPL-3.0-or-later | everything else; upstream test inputs copied verbatim are listed with their SHA-256 in the `tests/data/upstream/README.md` of each crate |

License texts: `COPYING`, `COPYING.lesser`, `LICENSES/MPL-2.0.txt` (from
https://www.mozilla.org/media/MPL/2.0/index.txt, SHA-256
`3f3d9e0024b1921b067d6f7f88deb4a60cbe7a78e76c64e3f1d7fc3b779b9d04`),
`LICENSES/BSL-1.0.txt`.

### Eigen 3.4.0 (dense algebra, products, sparse LU)

Source: Debian `libeigen3-dev 3.4.0-5`, `/usr/include/eigen3` in
`akantu-oracle:dev`.

| Files | Derived from | License of the derived files | Notes |
|---|---|---|---|
| `crates/akantu-linalg/src/dense/eigen/mod.rs`, `real_schur.rs`, `self_adjoint.rs` | Per file: `mod.rs` ← `Householder/Householder.h`, `Householder/HouseholderSequence.h`, `Jacobi/Jacobi.h`, `Core/GeneralProduct.h`, `Core/ProductEvaluators.h`, `Core/products/GeneralMatrixVector.h`, `Core/Redux.h`, `Core/Dot.h`; `real_schur.rs` ← `Eigenvalues/{HessenbergDecomposition,RealSchur,EigenSolver}.h`, `Core/MathFunctions.h`, `Core/Redux.h`, `Core/Dot.h`; `self_adjoint.rs` ← `Eigenvalues/{SelfAdjointEigenSolver,Tridiagonalization}.h`, `Core/products/{SelfadjointMatrixVector,SelfadjointRank2Update}.h`, `Core/MathFunctionsImpl.h`, `Core/MathFunctions.h`, `Core/Dot.h`, `Core/Redux.h` (the `Core/` product/reduction headers contribute evaluation orders and small helpers such as `maxi`/`mini`, `squaredNorm`, `normalize`, `dot`). Copyright holders named in those notices: Benoit Jacob (2006–2010), Gael Guennebaud (2008–2016), Jitse Niesen (2010–2012), Pedro Gonnet (2014, `MathFunctionsImpl.h`), NVIDIA CORPORATION (2021, `MathFunctions.h`). | MPL-2.0 (`SPDX-License-Identifier: MPL-2.0`) | Rust translation that keeps Eigen's evaluation order for bit parity (D-011). MPL-2.0 is file-level copyleft and compatible with LGPL-3.0-or-later (§3.3 Larger Work; the LGPL is a Secondary License and no Eigen header carries an Exhibit B notice); the files stay MPL-2.0 (§3.1: distributed in source form with the license), the rest of the crate LGPL-3.0-or-later; notices preserved per §3.4 and listed in `NOTICE.md`. `akantu-linalg`'s package license is therefore `LGPL-3.0-or-later AND MPL-2.0`. |
| `crates/akantu-core/src/eigen_gebp.rs` | `Core/products/GeneralBlockPanelKernel.h` (scalar `gebp_traits`, `gebp_kernel`/`lhs_process_one_packet` accumulation order, default cache sizes, `evaluateProductBlockingSizesHeuristic` depth blocking), `Core/products/GeneralMatrixMatrix.h` (block loops, `gemm_blocking_space`, `GemmProduct` `evalTo`/`scaleAndAddTo`), `Core/products/GeneralMatrixVector.h` (GEMV accumulation order and column blocking), `Core/GeneralProduct.h` (`product_size_category`, `gemv_dense_selector`, `EIGEN_GEMM_TO_COEFFBASED_THRESHOLD`), `Core/ProductEvaluators.h` (inner/outer/lazy products, GEMV inner-product fallback), `Core/CwiseBinaryOp.h` (`traits<CwiseBinaryOp>`: an element-wise binary expression takes its compile-time sizes from its left operand, which orders the coefficient reductions), `Core/Dot.h` (`dot_nocheck`: the binary expression the run-time `1 × 1` inner-product fallback reduces is built on its left operand). Copyright holders named in those notices: Gael Guennebaud (2008–2016; `CwiseBinaryOp.h` 2008–2014), Benoit Jacob (2006–2008; `Dot.h` 2006–2008 and 2010), Jitse Niesen (2011), all listed in `NOTICE.md` (Benoit Jacob 2006–2010). | MPL-2.0 | Rust translation of Eigen's strict-build (`EIGEN_DONT_VECTORIZE`) product dispatch and kernel order for bit parity (D-008, D-036). Same MPL-2.0/LGPL-3.0-or-later arrangement as `dense/eigen/` (file-level copyleft, Larger Work §3.3); `akantu-core`'s package license is therefore `LGPL-3.0-or-later AND MPL-2.0`. |
| `crates/akantu-linalg/src/eigen_sparse_lu/mod.rs` | `SparseLU/SparseLU.h` (`analyzePattern`, `factorize`, `_solve_impl`, `initperfvalues`, `SparseLUMatrixUReturnType::solveInPlace`), `SparseLU/SparseLUImpl.h`, `SparseLU/SparseLU_SupernodalMatrix.h` (`MappedSuperNodalMatrix::solveInPlace`, `InnerIterator`), `Core/PermutationMatrix.h` (permutation products/inverse). Holders: Désiré Nuentsa-Wakam (2012), Gael Guennebaud (2009–2015), Benoit Jacob (2009) | MPL-2.0 | `EigenSparseLuSolver` (upstream `SparseSolverEigen` on top) |
| `crates/akantu-linalg/src/eigen_sparse_lu/kernels.rs` | `SparseLU/SparseLU_{Structs,Memory,relax_snode,column_dfs,panel_dfs,panel_bmod,column_bmod,kernel_bmod,copy_to_ucol,pivotL,pruneL,Utils}.h`. Holders: Désiré Nuentsa-Wakam (2012), Gael Guennebaud (2012); SuperLU notices Copyright (c) 1994 by Xerox Corporation (SuperLU 3.0/4.1, Univ. of California Berkeley, Xerox PARC, LBNL) | MPL-2.0 (+ SuperLU permissive notice retained, modification stated) | memory expansion replaced by on-demand growth (same contents and positions) |
| `crates/akantu-linalg/src/eigen_sparse_lu/colamd.rs` | `OrderingMethods/Eigen_Colamd.h` (COLAMD 2.3), `OrderingMethods/Ordering.h` (`COLAMDOrdering`). Holders: Désiré Nuentsa-Wakam (2012); COLAMD Copyright (c) 1998-2003 by the University of Florida (authors Stefan I. Larimore, Timothy A. Davis) | MPL-2.0 (+ COLAMD notice retained: "Copyright, this License, and the Availability of the original version retained on all copies") | `int` arithmetic and in-workspace garbage collection reproduced (Row/Col structures stored separately with the same effective workspace length) |
| `crates/akantu-linalg/src/eigen_sparse_lu/etree.rs` | `SparseCore/SparseColEtree.h` (`coletree`, `treePostorder`, `nr_etdfs`). Holders: Désiré Nuentsa-Wakam (2012); SuperLU `sp_coletree.c` notice Copyright (c) 1994 by Xerox Corporation | MPL-2.0 (+ SuperLU notice retained, modification stated) | |
| `crates/akantu-linalg/src/eigen_sparse_lu/dense.rs` | `Core/products/TriangularSolverVector.h`, `Core/products/TriangularSolverMatrix.h`, `Core/SolveTriangular.h`, `Core/products/GeneralMatrixVector.h`, `Core/products/GeneralBlockPanelKernel.h` (kc rule with `KcFactor = 4`, gebp accumulation), `Core/Dot.h`, `Core/Redux.h`, `Core/GeneralProduct.h`, `SparseLU/SparseLU_gemm_kernel.h`. Holders: Gael Guennebaud (2008–2016), Benoit Jacob (2006–2010) | MPL-2.0 | strict-build (`EIGEN_DONT_VECTORIZE`) kernel order: trsv panels of 8, `sparselu_gemm` with `RK = 2` |

Re-verification: extract `Eigen/src` from the image
(`docker run --rm --network none akantu-oracle:dev tar cf - -C /usr/include/eigen3 Eigen/src`)
and compare each listed header's leading comment block with the Rust
header.

Files that follow Eigen's evaluation order without copying Eigen code stay
LGPL-3.0-or-later:

| Files | Origin | License | Notes |
|---|---|---|---|
| `crates/akantu-core/src/eigen_order.rs`, `tensor.rs` (det/inverse/LU/triangular solve), `voigt.rs` | Akantu (`aka_types.hh`, `aka_voigthelper*.hh`); evaluation order of Eigen 3.4.0 described in our own words | LGPL-3.0-or-later | No Eigen code copied: closed-form formulas and textbook LU / substitution written from the mathematics with Eigen's order (verified bitwise, `tests/dense_parity.rs`). Products in `tensor.rs` call the MPL-2.0 `eigen_gebp.rs`. |
| `crates/akantu-linalg/src/direct.rs` (`CscMatrix::from_triplets`) | behaviour of Eigen 3.4.0 `SparseMatrix::setFromTriplets` (row-major temporary, duplicates summed in insertion order, transposed copy) described in our own words | LGPL-3.0-or-later | no Eigen code copied (same arrangement as `eigen_order.rs`) |

### GNU C Library libm 2.41

| Component | Version | Upstream files | Ported to | Upstream license | Use in akantu-rs | Notices |
|---|---|---|---|---|---|---|
| GNU C Library (glibc) libm | 2.41 (Debian `2.41-12+deb13u4`, the oracle's libc; libm.so.6 sha256 `a8afb2d1f4a0c5e14c0fc82f794ec839eafe29738907aa92895410f57630bbe2`) | `sysdeps/ieee754/dbl-64/`: `e_log.c`, `e_exp.c`, `e_pow.c`, `math_config.h`, `math_err.c`, `e_exp_data.c`, `e_log_data.c`, `e_pow_log_data.c` (© 2018–2025 FSF; code contributed from Arm Optimized Routines); `s_tan.c` (IBM Accurate Mathematical Library, written by International Business Machines Corp., © 2001–2025 FSF); `dla.h`, `utan.h`, `utan.tbl` (IBM Accurate Mathematical Library, © 2001–2025 FSF); `s_sin.c` (IBM Accurate Mathematical Library, © 2001–2025 FSF), `usncs.h`, `sincostab.c` (IBM Accurate Mathematical Library, © 2001–2025 FSF), `s_sincos.c` (© 1997–2025 FSF): `sin`, `cos`, `sincos` | `crates/akantu-core/src/random/libm.rs`, `libm_tables.rs` (generated by `oracle/probes/random/gen_libm_tables.py`); `crates/akantu-core/src/glibc_libm/sincos.rs`, `sincos_table.rs` (generated by `oracle/probes/random/gen_sincos_table.py`, table verified byte-for-byte in libm.so.6) | LGPL-2.1-or-later | Used under LGPL-3.0-or-later via the "or (at your option) any later version" clause (LGPL-2.1 §13; GNU license FAQ) | Upstream copyright/license notices in both file headers (the generator re-extracts them from the sources) and in `NOTICE.md` |

The translations follow the instruction sequence of the oracle's compiled
libm (permitted for LGPL code, D-030).

### Boost.Spirit 1.83 and the `.dat` parser

| Port file | Derived from | Version | License | Copyright notices (kept in the file header and `NOTICE.md`) |
|---|---|---|---|---|
| `crates/akantu-parser/src/expr.rs` (real-number literals: `POW10`, `parse_sign`, `string_parse`, `parse_nan`, `parse_inf`, `parse_exp_n`, `x3_*`, `qi_*`) | Boost.Spirit: `boost/spirit/home/x3/support/numeric_utils/{extract_real.hpp, extract_int.hpp, pow10.hpp, detail/extract_int.hpp}`, `x3/numeric/real_policies.hpp`, `x3/string/detail/string_parse.hpp`, `qi/numeric/{real_policies.hpp, numeric_utils.hpp, detail/real_impl.hpp, detail/numeric_utils.hpp}`, `qi/detail/string_parse.hpp`, `support/detail/pow10.hpp` | 1.83 (oracle image) | BSL-1.0 (`LICENSES/BSL-1.0.txt`) | Joel de Guzman 2001-2019 (x3 2001-2014, qi real 2001-2019, others 2001-2011); Hartmut Kaiser 2001-2011; Jan Frederick Eick 2011; Christopher Jefferson 2011; Stephen Nutt 2006 |
| `crates/akantu-parser/src/grammar.rs` (line/column counting) | Boost.Spirit Classic `boost/spirit/home/classic/iterator/{position_iterator.hpp, impl/position_iterator.ipp}` (behaviour) | 1.83 | BSL-1.0 | Juan Carlos Arevalo-Baeza 2002; Hartmut Kaiser 2002-2006; Giovanni Bajo 2003 |
| `crates/akantu-parser/src/**`, `tests/test_parser*.rs` | Akantu `src/io/parser/**`, `src/common/aka_random_generator.hh`, `src/common/aka_common.cc`, `test/test_io/test_parser/**` | `35a851780d0b` | LGPL-3.0-or-later | EPFL (LSMS) 2010-2023 (test_parser.py 2020-2025) |
| `crates/akantu-parser/src/stream.rs`, `RandomDistribution::parameters_text` (a delegation to `akantu_core::random`'s clean-room printing; the former body is kept in `tests/random_bridge.rs`) | clean-room from the ISO C/C++ standards and black-box behaviour of GCC 14.2 libstdc++ / glibc 2.41 (D-030; no source read, see `crates/akantu-parser/CLEANROOM_LOG.md`) | — | (no third-party code) | — |
| `oracle/probes/parser/stream_blackbox.cc` (`akantu_bool`, `akantu_list`) | Akantu `src/io/parser/parser_tmpl.hh` | `35a851780d0b` | LGPL-3.0-or-later | EPFL (LSMS) 2013-2023 |

### CGAL 5.6.2

Source read: the public CGAL 5.6.2 release `CGAL-5.6.2-library.tar.xz`
(sha256 `2d93d96c8c73c73c9999556d512775648d26e1e04035ceb958208d25e6e2d10f`,
the tarball of the oracle image), downloaded to a scratch directory outside
the repository. No external Rust crate was added; CGAL's own LGPL exact
number types are ported.

| akantu-rs file | Derived from (CGAL 5.6.2) | CGAL licence | akantu-rs licence |
|---|---|---|---|
| `crates/akantu-geometry/src/number/{mp_float,quotient,sqrt_extension}.rs` | Number_types: `MP_Float.h`, `MP_Float_impl.h`, `Quotient.h`, `Sqrt_extension/Sqrt_extension_type.h`, `Sqrt_extension/Real_embeddable_traits.h`, `Root_of_traits.h` | LGPL-3.0-or-later | LGPL-3.0-or-later |
| `crates/akantu-geometry/src/kernel.rs` | Cartesian_kernel / Kernel_23: `Cartesian/function_objects.h`, `Cartesian/{Segment_3,Ray_3,Line_3,Vector_3,Tetrahedron_3,solve_3}.h`, `predicates/kernel_ftC2.h`, `predicates/kernel_ftC3.h`, `constructions/kernel_ftC3.h`, `determinant.h`, `Kernel/solve.h`, `Bbox_3.h`, `predicates/sign_of_determinant.h` | LGPL-3.0-or-later | LGPL-3.0-or-later |
| `crates/akantu-geometry/src/intersections.rs` | Intersections_3: `internal/Segment_3_Triangle_3_intersection.h`, `internal/Line_3_Plane_3_intersection.h`, `internal/Bbox_3_Segment_3_do_intersect.h`, `internal/Bbox_3_Ray_3_do_intersect.h`, `internal/Ray_3_Triangle_3_do_intersect.h` | LGPL-3.0-or-later | LGPL-3.0-or-later |
| `crates/akantu-geometry-cgal/src/aabb.rs` | AABB_tree: `AABB_tree.h`, `AABB_traits.h`, `AABB_tree/internal/AABB_node.h`, `AABB_tree/internal/AABB_traversal_traits.h` | GPL-3.0-or-later | GPL-3.0-or-later (D-042) |
| `crates/akantu-geometry-cgal/src/spherical.rs` | Circular_kernel_3: `Circular_kernel_3/{Line_arc_3,internal_functions_on_line_arc_3,internal_functions_on_sphere_3,internal_function_has_on_spherical_kernel}.h`; Algebraic_kernel_for_spheres: `Root_for_spheres_2_3.h`, `Polynomials_for_line_3.h`, `Algebraic_kernel_for_spheres/internal_functions_on_roots_and_polynomial_1_3_and_2_3.h`, `…/internal_functions_comparison_root_for_spheres_2_3.h`; plus (LGPL) Cartesian_kernel `Cartesian/Sphere_3.h`, `constructions/kernel_ftC3.h` for the sphere side predicates | GPL-3.0-or-later (the Cartesian_kernel parts: LGPL-3.0-or-later) | GPL-3.0-or-later (D-042) |
| `crates/akantu-geometry-cgal/src/sphere_intersector.rs` | Akantu `mesh_sphere_intersector*.hh` etc. (LGPL), built on `spherical.rs` | — | GPL-3.0-or-later |

CGAL copyright holders (file headers carry the exact notices): Utrecht
University, ETH Zurich, INRIA Sophia-Antipolis, Max-Planck-Institute
Saarbruecken, Tel-Aviv University (kernel consortium); GeometryFactory;
authors include Sylvain Pion, Stefan Schirra, Michael Hemmer, Andreas Fabri,
Hervé Brönnimann, Laurent Rineau, Stéphane Tayeb, Camille Wormser, Pierre
Alliez, Monique Teillaud, Pedro Machado, Philippe Guigue, Geert-Jan Giezeman.

The orientation predicates of `crates/akantu-geometry/src/intersection.rs`
follow Kernel_23's (LGPL) sign convention and `is_approx` follows Eigen's
`isApprox` formula (MPL-2.0); nothing in that file derives from a GPL
package. The sphere side predicates `SkSphere::{bounded_side, has_on_*}` in
the GPL crate are translations of the LGPL `Cartesian/Sphere_3.h`.
`crates/akantu-igfem/src/enrichment.rs::has_on_unbounded_side` implements
the definition of CGAL's Kernel_23 predicate
`Sphere_3::has_on_unbounded_side` (LGPL) with
`akantu_geometry::number::Quotient` (exact).

### Other upstream material

- The dumpers in `crates/akantu-io/src/dumper/` listed above are Rust ports
  of IOHelper 1.1.1 (`third-party/iohelper` of the reference, a separate
  EPFL project, LGPL-3.0-or-later); each file carries the notice.
- `docs/manual/` adapts the structure and parts of the text of the upstream
  manual (`doc/dev-doc/manual/*.rst`, `appendix/elements.rst`; © EPFL/LSMS,
  LGPL-3.0-or-later, same licence as the port); attribution in
  `docs/manual/NOTICE`. No upstream figure is copied (the SVG figures of
  `doc/dev-doc/manual/figures/` carry no separate licence statement; the
  element tables are generated from the port's code instead). The manual's
  tests (`crates/akantu/src/manual.rs`, `crates/akantu/tests/manual*.rs`) use
  only the workspace crates and `std`.
- No Python or PyO3 code is translated from upstream sources other than
  Akantu's own LGPL `python/` directory (`akantu/__init__.py` is copied with
  its copyright notice; the upstream test scripts are copied unchanged with
  their notices).
- `crates/akantu-core/tests/data/upstream/test_sparse_matrix_product.verified`
  and the other upstream test inputs copied verbatim (Akantu,
  LGPL-3.0-or-later) are listed with their SHA-256 in the
  `tests/data/upstream/README.md` of each crate (for example
  `crates/akantu-igfem/tests/data/upstream/README.md`) and in the data
  manifest checked by `crates/akantu-validation/tests/upstream_data_manifest.rs`.
- The frozen meshes in `fixtures/meshes` are Gmsh 4.13.1 *output* (data
  generated from the reference's `.geo` files, LGPL-3.0-or-later inputs),
  not Gmsh code.

## 5. Behaviour reproduced clean-room

Code under licences that do not allow a source translation to be
distributed under LGPL-3.0-or-later (GCC's libstdc++ and libgcc:
GPL-3.0-or-later with the GCC Runtime Library Exception) is never read for
translation, and its compiled code is not disassembled or inspected
(D-030). Its observable behaviour is reproduced from public specifications,
published algorithms and black-box experiments on the compiled libraries;
each such file has a provenance log.

| akantu-rs file | Behaviour reproduced | Provenance |
|---|---|---|
| `crates/akantu-core/src/random/engine.rs`, `distributions_impl.rs` | the oracle's GCC libstdc++ `<random>` engine and distributions | `crates/akantu-core/src/random/CLEANROOM_LOG.md` |
| `crates/akantu-parser/src/stream.rs` | libstdc++/glibc stream extraction and number printing | `crates/akantu-parser/CLEANROOM_LOG.md` |
| `crates/akantu-geometry/src/cxx.rs` | libstdc++'s `std::nth_element` (used by CGAL's AABB tree build) and `std::set::insert` with upstream's invalid comparator `segmentPairsLess` (black-box fixtures from Debian trixie GCC 14.2) | `crates/akantu-geometry/CLEANROOM_LOG.md` |
| `crates/akantu-linalg/src/dense/complex.rs` | `std::complex<double>` division as executed by the oracle (libgcc `__divdc3`) | `crates/akantu-linalg/src/dense/COMPLEX_CLEANROOM_LOG.md` |
| `crates/akantu-linalg/src/matrix_market.rs` (`format_g`) | the C standard's `%g` (C11 §7.21.6.1) and black-box output of the oracle's libstdc++ (`fixtures/solver/text/matrix_market`) | no C/C++ library code read |
| `crates/akantu-io/src/mesh_io/numfmt.rs` | the C `%g` formatting of the MSH writer, from the C standard's description | no C library code read |

The complex division in detail: written from Smith (1962) Algorithm 116,
Baudin & Smith (2012) numerators for an underflowing ratio, ISO C11 Annex
G.5.1 (EXAMPLE 2) special-value recovery and IEEE 754 FMA; every threshold,
scale factor, branch rule and FMA placement was established by black-box
input/output experiments on `std::complex<double>` division in
`akantu-oracle:dev` (g++ 14.2.0, Debian trixie, aarch64; probes
`oracle/probes/dense/complex_cleanroom/`). It replaced an earlier version
that had been derived with machine-code inspection of libgcc `__divdc3` and
was removed; no GCC/libgcc/libstdc++ source was read, no compiled library
code was disassembled or inspected for it, and the removed implementation
was not read. It is verified bit for bit (NaN payload/sign excepted) on the
`dense_strict.txt` `cdiv` records, the 4500-record
`complex_div_cleanroom.txt` fixture and 6e6 opt-in bulk records.

glibc and libstdc++ are also used as black boxes by the oracle probes in
`oracle/probes/random/` (not distributed with the library). The reference
semantics of the random numbers are tied to libstdc++: an Akantu built with
libc++ draws other numbers (D-006).

## 6. Licence consequences of the optional features

The source and the default build of akantu-rs are LGPL-3.0-or-later (with
the MPL-2.0 files of section 4). Three optional features change the licence
of a build.

### `cgal`, `igfem` and `akantu-igfem/cgal`: GPL-3.0-or-later as a whole

`akantu-geometry-cgal` declares `license = "GPL-3.0-or-later"` (D-042). It
is an optional dependency of the umbrella crate (feature `cgal`, OFF by
default; implied by the feature `igfem`, also OFF by default) and of
`akantu-igfem` (its feature `cgal`, OFF by default, turned on by the
umbrella's `igfem`). `crates/akantu-igfem` itself is LGPL-3.0-or-later
(Akantu code only); its feature `cgal` adds the optional dependency on the
GPL crate (sphere intersector) and compiles the modules that use it
(`src/growing_gel.rs`, the spherical part of `src/enrichment.rs`; their
sources stay LGPL). The umbrella feature `igfem` is
`["solid", "cgal", "dep:akantu-igfem", "akantu-igfem/cgal"]`. A build
that enables `cgal` or `igfem` is GPL-3.0-or-later as a whole, and so is a
build of `akantu-igfem` with its feature `cgal`. Neither `igfem` nor
`akantu-igfem/cgal` is a default feature. The `embedded` feature never
implies `cgal`.

The separation is guarded by `crates/akantu-geometry/tests/license_separation.rs`
and `crates/akantu-geometry-cgal/tests/license_headers.rs` (GPL SPDX header
and CGAL notices in every file). The first guard checks the resolved
dependency graph: it reads the workspace manifests with
`cargo metadata --no-deps` and replays Cargo's feature resolution over the
members. Target-specific tables count as active and features are unified
across dependency kinds, so the replay can only over-approximate. The only
exceptions are the declared GPL switches listed in the test
(`GPL_SWITCHES`): `(akantu, cgal)`, `(akantu, igfem)` and
`(akantu-igfem, cgal)`. Each switch must exist and reach the GPL crate
(resolver and `cargo tree`); no default feature set of any LGPL member may
reach a GPL member or turn on a switch (its own crate's `default`, or
transitively a dependent's); every LGPL member with all its features except
its own switches (`--all-features` when it has none), dev- and
build-dependencies included, must not reach any GPL member. The umbrella's
default features, each of its features alone and all its non-switch
features together are also resolved by `cargo tree`, which must agree;
`igfem` must imply `cgal` and enable `akantu-igfem/cgal`. Adding a switch is
a licensing decision recorded in `docs/DECISIONS.md`. What the guard
rejects is recorded by the mutation campaign
`crates/akantu-geometry/tests/license_separation_mutations.py` (19 bypass
forms killed, 3 controls pass; `docs/TESTING.md`). The guard uses the
existing dev-dependency `serde_json` and the running `cargo`. The CI
licence tier repeats the separation check with `cargo tree` per member.

### `petsc`: GPL-3.0-or-later for binaries linked against Debian's PETSc

Debian's `libpetsc_real.so.3.22.5` (the image `akantu-rs-petsc:dev` built
from `docker/petsc`, Debian trixie `libpetsc-real3.22`
3.22.5+dfsg1-2+b1: the same package and shared object as the oracle image)
lists, under `ldd`, besides Open MPI, OpenBLAS/LAPACK, MUMPS (CeCILL-C),
ScaLAPACK (BSD-3), SuperLU (BSD-3), SuperLU_DIST (BSD-3), hypre (Apache-2.0
OR MIT), PT-Scotch (CeCILL-C), METIS (Apache-2.0), HDF5 (BSD-3 style) and
libyaml (Expat), these libraries with GPL code:

| Linked library (Debian package, version) | License of the linked code (Debian `copyright`) |
|---|---|
| `libumfpack.so.6` (`libumfpack6` 1:7.10.1+dfsg-1) | `UMFPACK/*`: GPL-2+ |
| `libspqr.so.4` (`libspqr4` 1:7.10.1+dfsg-1) | `SPQR/*`: GPL-2+ |
| `libcholmod.so.5` (`libcholmod5` 1:7.10.1+dfsg-1) | `*`: LGPL-2.1+; `CHOLMOD/{Supernodal, MatrixOps, Modify, GPU}/*` (and demos, MATLAB, Tcov): GPL-2+. The library exports Supernodal (10) and Modify (22: `updown`/`rowadd`/`rowdel`) symbols, so GPL modules are compiled in (not only `Supernodal`) |
| `libklu.so.2`, `libamd/colamd/camd/ccolamd` | LGPL-2.1+ / BSD-3 |
| `libfftw3.so.3`, `libfftw3_mpi.so.3` (`libfftw3-double3`, `libfftw3-mpi3` 3.3.10-2+b1) | GPL-2+ |
| `libCombBLAS.so.2.0.0` (pulled in by `libsuperlu_dist.so.8`) | header-only `psort-1.0/include/psort/{funnel,sort}.*`: GPL-2+ per Debian's `copyright`; no `psort` symbol is exported, and whether that code is instantiated inside the library was not determined (an open item for a GPL-free build) |

Analysis:

1. Source distribution of akantu-rs is unaffected: no GPL code is in the
   repository's LGPL crates; `crates/akantu-linalg/src/petsc/shim.c` calls
   only PETSc's public API (BSD-2-Clause), and akantu-rs never calls
   UMFPACK, SPQR, CHOLMOD or FFTW (none of PETSc's solver types that use
   them is selected; upstream Akantu neither).
2. Default build (feature `petsc` off, also the default of the umbrella
   crate): no PETSc, nothing GPL is linked.
3. Binaries built with the `petsc` feature against Debian's PETSc are
   dynamically linked with GPL-2.0-or-later libraries (through
   `libpetsc_real`'s own dependencies). Under the FSF's reading, dynamic
   linking forms a combined work, so such a binary may be distributed only
   under terms compatible with the GPL, in practice GPL-3.0-or-later as
   a whole (LGPL-3.0-or-later and GPL-2.0-or-later are both upgradable to
   GPL-3.0-or-later; Apache-2.0 hypre and CeCILL-C MUMPS/Scotch are
   GPL-3-compatible). This is the situation of D-042 (feature `cgal`).
   Test binaries and CI images are not distributed and create no
   obligation.
4. LGPL-only distribution with PETSc needs a PETSc without those
   components: PETSc configured with `--with-suitesparse=0 --with-fftw=0`,
   and without SuperLU_DIST (CombBLAS) unless the `psort` question above is
   settled (everything akantu-rs uses remains: KSP, PC ILU/block Jacobi/LU,
   MUMPS through PETSc, SNES `newtonls`, TAO `gpcg`). Debian ships no such
   variant; it would be a source build in a separate image, and the bitwise
   oracle claims would then need the oracle to run against the same PETSc
   (or a check that the results do not change, expected since the removed
   components are never called).

Decision D-076. The `petsc` feature of `akantu-linalg` /
`akantu-solver` is off by default (the umbrella crate `akantu` has no
`petsc` feature) and no reduced PETSc build is made. A binary built with
the `petsc` feature against Debian's PETSc is linked with the
GPL-2.0-or-later components above and is distributed under
GPL-3.0-or-later as a whole, exactly as a build with `cgal` (D-042); the
source and the default build stay LGPL-3.0-or-later. Stated in `NOTICE.md`,
`README.md` and the user manual (`docs/manual/src/petsc.md`); the CI
licence policy's informational `[native] petsc` entry says the same.

## 7. Dependencies of the C++ reference (the oracle)

Legend: M mandatory, O optional package, B build or test tool
only. "Used for" is what Akantu actually calls (grep of the reference
source).

| Dependency | Kind | Oracle version | License | Used for (features needing it) |
|---|---|---|---|---|
| C++20 compiler | M | GCC 14.2.0 (`g++-14 14.2.0-19`); clang 19.1.7 also in image | GPL-3+ w/ runtime exception | `AKANTU_CXX_STANDARD 20` |
| CMake | B | 3.31.6 | BSD-3 | build system (≥ 3.16.3 required) |
| Ninja | B | 1.12.1 | Apache-2.0 | generator (upstream CI uses Ninja) |
| Boost (headers) | M | 1.83.0 | BSL-1.0 | `preprocessor` (element-type/material code generation macros), `spirit` Qi/X3/classic + `phoenix` + `fusion` + `variant` (input-file parser `src/io/parser`, algebraic expression parser), `config` |
| Boost.Graph | O (`debug_tools`) | 1.83.0 | BSL-1.0 | `mesh_utils/boost_graph_converter` (graphviz export of mesh graphs) |
| Eigen | M | 3.4.0 (`libeigen3-dev 3.4.0-5`); upstream fallback fetch would be 5.0.1 | MPL-2.0 (some files BSD-3 / LGPL-2.1+) | Akantu 5 `Vector/Matrix/Tensor` types are Eigen maps (`aka_types.hh`: dense algebra, eigen-decomposition, inverse, determinant); `SparseSolverEigen` (Eigen sparse direct/iterative solvers) |
| MPI | O (`parallel`) | Open MPI 5.0.7 (MPI 3.1) | BSD-3 | `Communicator` (`src/synchronizer`): point-to-point (blocking/non-blocking incl. `Issend/Irsend`), `Probe/Iprobe`, `Wait*/Test*`, `Ibarrier` (MPI-3), `Allreduce/Reduce/Scan/Exscan/Gather(v)/Allgather(v)/Bcast/Barrier`; error handlers |
| Scotch | O (`parallel`) | 7.0.7 (`libscotch-dev`; PT-Scotch also installed) | CeCILL-C | `MeshPartitionScotch`: `SCOTCH_graphBuild/graphPart/graphOrder`, `SCOTCH_mesh*`, strategies, `SCOTCH_randomSeed` (mesh partitioning, node reordering) |
| MUMPS | O (`implicit`, solver `Mumps`) | 5.7.3 (parallel `libdmumps`, + PORD, ScaLAPACK) | CeCILL-C (PORD public domain; `ana_orderings.F` LGPL/CeCILL-C/BSD-3) | `SparseSolverMumps`: `dmumps_c`, centralized or fully distributed input (ICNTL(18)=0/3), analysis/factorization/solve phases, memory relaxation loop on ICNTL(14) |
| PETSc | O (`implicit`, solver `PETSc`) | 3.22.5 (`libpetsc-real3.22-dev`, real, 32-bit `PetscInt`) | BSD-2-Clause | `SparseSolverPETSc`, `SparseMatrixPETSc`, `SolverVectorPETSc`: `KSP*` iterative solvers, `Mat*` (local-to-global mapping, `MatZeroRowsColumnsLocal`), `Vec*` |
| ScaLAPACK | indirect | 2.2.2 (openmpi) | BSD-3 | link dependency of parallel MUMPS (Akantu's own `scalapack` package option is unused, OFF) |
| BLAS/LAPACK | indirect | OpenBLAS 0.3.29 (pthread variant via alternatives); reference LAPACK 3.12.1 | BSD-3 | through MUMPS/PETSc/ScaLAPACK only; Akantu calls no BLAS/LAPACK routine directly |
| METIS, SuperLU(-dist), hypre, SuiteSparse … | indirect | METIS 5.1.0, SuperLU 7.0.1, SuperLU_DIST 8.2.1, hypre 2.32.0, SuiteSparse 7.10.1 | Apache-2.0 / BSD-3 / MIT / LGPL-2.1 mix | pulled in by Debian `petsc-dev`/`libmumps-dev`; not referenced by Akantu |
| CGAL | O (`CGAL`, needed by `embedded`, `igfem`) | 5.6.2 (header-only, pinned upstream release; Debian's 6.0.1 is API-incompatible, D-032) | AABB tree, AABB traits, spherical kernel: GPL-3.0-or-later (or commercial); kernels/number types (`Cartesian`, `Simple_cartesian`, `Lazy_exact_nt`, `MP_Float`, `Quotient`, `intersections`): LGPL-3.0-or-later | `src/geometry`: AABB-tree segment/triangle/tetrahedron intersectors, mesh-sphere intersection (`mesh_geom_*`, `mesh_*_intersector*`) |
| GMP / MPFR | indirect (CGAL) | 6.3.0 / 4.2.2 | LGPL-3+ (GMP: LGPL-3+ or GPL-2+) | CGAL exact number types |
| Python 3 | O (`python_interface`) | 3.13.5 | PSF-2.0 | pybind11 module `py11_akantu`, python tests/examples |
| pybind11 | O (`python_interface`) | 2.13.6 (`pyproject.toml` pins `~=2.13.6`) | BSD-3 | Python bindings (`python/*.cc`) |
| NumPy | O (python) | 2.2.4 | BSD-3 | array interop in bindings, tests, examples |
| SciPy | O (python) | 1.15.3 | BSD-3 | `python/akantu/__init__.py` imports `scipy.sparse`; tests (`test/python_fe`), examples |
| mpi4py | O (python, parallel) | 4.0.3 | BSD-2/3 | parallel python tests/examples |
| matplotlib, pandas, termcolor, click, tqdm, jinja2 | B (examples/tools) | 3.10.1, 2.2.3, … | PSF-like / BSD-3 / MIT | plotting in examples, CI helper scripts |
| pytest | B | 8.3.5 | MIT | python tests |
| googletest | B (tests) | 1.16.0 (Debian `googletest 1.16.0-1` sources in the recorded image, not version-pinned in the Dockerfile; upstream fetches unpinned `main`) | BSD-3 | gtest-based tests (`GTEST` in `register_test`) |
| Gmsh (executable) | B (tests/examples) | 4.13.1 | GPL-2+ (some parts LGPL/MIT) | `add_mesh()` (`cmake/Modules/FindGMSH.cmake`) runs `gmsh -<dim> -order <k> -o x.msh x.geo` at build time; Akantu reads `.msh` 2.2/4.x itself (no Gmsh library link) |
| HDF5 | — | 1.14.5 installed (upstream CI image) | BSD-3 | not used by Akantu at this commit (only listed in CI images) |
| Google Benchmark | — | not installed | Apache-2.0 | `AKANTU_BENCHMARK` / `gbenchmark` option points to `test/benchmark`, which does not exist at this commit → unusable |
| Sphinx, Doxygen, breathe | B (`documentation`) | not installed | BSD / GPL-2 (tool) | documentation package (OFF) |
| scikit-build, setuptools, wheel | B (wheel packaging) | not used | MIT | `setup.py` wheel build |

Notes on reproducibility-relevant dependencies:

- Gmsh is part of the numerical input chain: test meshes are generated
  from `.geo` files at build time, so node numbering/coordinates of those
  meshes depend on the Gmsh version (4.13.1 here). Golden fixtures that use
  generated meshes must store the `.msh` file itself, not regenerate it
  (D-009).
- OpenBLAS kernels (used inside MUMPS/PETSc) are hand-written assembly
  with FMA on aarch64 and are not affected by our compiler flags; the image
  forces `OPENBLAS_NUM_THREADS=1` (single-threaded kernels, no thread-count
  dependence of the reduction order) and the core type is pinned (D-050).
- Scotch partitioning is seeded from the wall clock: Akantu seeds its
  `RandomGenerator` from `time(nullptr)` unless `AKA_SEED`, `--aka_seed` or an
  input-file `seed` is given (`src/common/aka_common.cc:105-117`) and passes
  that seed to Scotch (`mesh_partition_scotch.cc:62`); multi-rank partitions,
  and everything that depends on them, change between runs unless the seed is
  fixed (`docs/ORACLE_BUILD.md`).
- Debian-built MUMPS/PETSc/Scotch are compiled with Debian's default
  flags (GCC default `-ffp-contract=fast`), so implicit-solver results of the
  `strict` build are not "IEEE strict" inside those libraries (D-031).

Bundled code in the reference tree:

| Path | What | License | Notes |
|---|---|---|---|
| `third-party/akantu_iterators` | header-only C++ iterator library (`zip`, `enumerate`, `arange`, `transform`, views, `make_view`), v1.1.1 | LGPL-3.0-or-later (EPFL) | used everywhere in `src/`; idiomatic Rust iterators replace it |
| `third-party/iohelper` | IOHelper 1.1.1: ParaView VTU/PVTU writer (ASCII/binary/base64), text dumper | LGPL-3.0-or-later (EPFL) | used by `dumpers` package; ported to Rust (byte-compatible output where tests compare files) |
| `third-party/cmake/*.cmake` | FetchContent scripts: `eigen3` (Eigen 5.0.1), `pybind11`, `gtest` (`main`), `gbenchmark`, `blackdynamite`, `blas`, `iohelper`, `akantu_iterators` | EPFL scripts (no header; part of Akantu) | download unpinned/pinned upstream sources only when system packages are not used |
| `cmake/Modules/` | `CMakePackagesSystem*` (package framework), `Find{GMSH,METIS,Mumps,PETSc,PTScotch,ParMETIS,ScaLAPACK,Scotch,Sphinx}.cmake` | LGPL-3.0-or-later (EPFL) | |
| `src/io/parser/cppargparse/` | command-line argument parser | LGPL-3.0-or-later (EPFL) | own code, not a third-party copy |
| `test/ci/` | CI images (Debian trixie, Ubuntu LTS, manylinux, macOS/spack), `ctest2junit.xsl` | EPFL | basis of our oracle image |

No third-party source code under a different license is vendored in the
reference tree.
