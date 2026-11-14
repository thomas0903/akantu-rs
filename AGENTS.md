<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Instructions for coding agents

These instructions are for any coding agent asked to change this repository.
People should read `CONTRIBUTING.md`; the step-by-step procedures are in
`docs/MAINTAINING.md`.

## What this project is

akantu-rs is a Rust port of Akantu, the C++ finite-element library of EPFL
(LGPL-3.0-or-later). The C++ library at one pinned commit (`docs/REFERENCE.md`)
is the oracle: golden data generated from it (`fixtures/`) are compared with
the port's results by the differential tests. Most comparisons are bitwise.

The property that must never break: the port's results equal the C++
library's, bit for bit where a fixture says bitwise, and within the written
tolerance rule where it names one. A change that moves a single bit of a
bitwise result is a semantic change, even if it looks more accurate.

The port was written by AI coding agents under independent review;
`docs/reports/` describes how.

## Layout

- `crates/akantu-core`, `akantu-parser`, `akantu-comm`: scalar and index types, arrays, element tables, errors; `.dat` input files; the communicator (serial or MPI).
- `crates/akantu-mesh`, `akantu-fe`, `akantu-io`, `akantu-geometry`: meshes and their utilities, the FE engine, mesh readers and dumpers, geometric queries.
- `crates/akantu-geometry-cgal`: the only GPL-3.0-or-later crate (code ported from CGAL), opt-in through the feature `cgal`.
- `crates/akantu-linalg`, `akantu-solver`, `akantu-model`: sparse matrices and solver back-ends, DOF manager and solvers, the model and constitutive-law framework.
- Physics crates: `akantu-solid`, `akantu-cohesive`, `akantu-contact`, `akantu-diffusion`, `akantu-phase-field`, `akantu-structural`, `akantu-embedded`, `akantu-couplers`, `akantu-extra-materials`, `akantu-igfem`, `akantu-ntn-contact`.
- `crates/akantu` is the facade (its features select the packages); `akantu-python` and `python/` are the Python package; `akantu-capi` is a placeholder for a C API (it exports its version string only); `akantu-bench` holds the benchmark drivers.
- `crates/akantu-validation`: the fixture harness, the `fixture-diff` tool and the tests that guard documents and inventories.
- `fixtures/`: golden data with `SHA256SUMS` manifests. `oracle/`: the C++ oracle's image, probes and patches.
- `ci/`: the test tiers and their entry point `ci/run-all.sh`; `ci/github/`: the workflow sources copied to `.github/workflows/`.
- `docs/`: the documents (index in `docs/README.md`) and the user manual (`docs/manual/`, mdBook); `scripts/`: maintenance tools; `docker/`: the Linux images with MPI, Scotch, MUMPS and PETSc.

## Commands

```sh
cargo build --workspace --locked
cargo test -p <crate> --locked                     # one crate: start here
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
ci/run-all.sh --backend docker                     # the fast tier in the project image
CI_INSTALL_TOOLCHAINS=1 ci/run-all.sh --backend docker \
  --tiers lint,test,fixtures,parity,licence,msrv,docs   # what CI runs on a pull request, except the manual and the self-test
ci/run-all.sh --tiers fixtures,parity              # light checks, no Docker
ci/selftest.sh                                     # after any change under ci/
```

`ci/run-all.sh --list` prints every tier and preset. The Docker backend is the
authoritative configuration for bitwise results; build the images first
(`docs/TESTING.md`). A workspace build fills 21 to 27 GB of cargo target, so
prefer one crate's tests while you work.

## Rules for numerical code

- `f64` wherever upstream uses `Real`. Use the index types of `akantu-core`
  (`docs/ARCHITECTURE.md`); do not invent new ones.
- No fast-math of any kind: no reassociation, no approximate intrinsics, no
  compiler flags that relax IEEE 754, and no `mul_add` unless it is shown to
  give the C++ build's results and the reason is documented next to it (the
  three existing places reproduce fused operations of the oracle's build:
  `crates/akantu-core/src/glibc_libm/sincos.rs`,
  `crates/akantu-core/src/random/libm.rs`,
  `crates/akantu-linalg/src/dense/complex.rs`).
- Mathematical functions come from `akantu_core::glibc_libm` (the oracle's
  glibc, translated), not the platform libm. The functions of `.dat`
  expressions are the exception, on purpose: upstream calls `<cmath>`.
- Keep the order of floating-point operations of the C++ code. A sum taken in
  another order, a factored expression or a different loop nesting changes
  the last bits and breaks bitwise fixtures.
- No relaxed solver tolerances.
- Library code returns `Result<_, akantu_core::Error>`; no `panic!` or
  `unwrap()` on input a user controls.

## Rules for tests and fixtures

- Never loosen a tolerance to make a test pass. A new or changed tolerance
  rule goes into the area's `tolerances.json` with its justification and
  evidence, and into the ledger (`python3 scripts/gold/tolerance_ledger.py`).
- Never `#[ignore]` or delete a failing test to hide it. A known failure goes
  into `ci/tools/known-failures.txt` with its reason, and only with a human's
  agreement.
- Never edit a golden file by hand. Fixtures are regenerated from the oracle
  (`docs/ORACLE_BUILD.md`), and their `SHA256SUMS` lines change with them.
- A ported upstream test keeps the intent and the criteria of the original,
  is named after it and gets its row in `docs/FEATURE_PARITY.md`.
- Tests behind a feature that no crate enables by default need a line in
  `ci/tools/feature-gates.txt`, a floor in `ci/floors.txt` and a row in the
  gate map of `ci/github/README.md`.

## Rules for `unsafe` and licensing

- Crates without `unsafe` keep `#![forbid(unsafe_code)]`. Every `unsafe` block
  has a `// SAFETY:` comment stating its invariants, and the crate's
  `UNSAFE.md` gets its row in the same change. Check with
  `python3 scripts/unsafe_inventory.py --check --strict-safety`.
- Every new source file starts with `// SPDX-License-Identifier:
  LGPL-3.0-or-later` and a line naming the Akantu file it is ported from.
- GPL code lives only in `crates/akantu-geometry-cgal`. Never copy code whose
  licence is not compatible with LGPL-3.0-or-later (GPL-only code, libstdc++).
- `cargo fmt` and `clippy -D warnings` clean. An `#[allow(...)]` needs a
  one-line comment saying why.

## Which document to update

| Change | Document |
|---|---|
| a feature's status or evidence | `docs/FEATURE_PARITY.md`, then `python3 scripts/parity/stats.py --update` |
| a defect found in the C++ library, or a change in how the port handles one | `docs/UPSTREAM_DEFECTS.md` |
| a technical decision (numerics, API semantics, licensing, platforms) | `docs/DECISIONS.md` |
| anything a user notices | `CHANGELOG.md`, the section marked "Unreleased" |
| a tolerance | `tolerances.json`, `docs/TOLERANCE_LEDGER.md` (generated), `docs/NUMERICAL_VALIDATION.md` |
| a dependency | `docs/DEPENDENCIES.md` (and `NOTICE.md` if code is copied) |

Blocks between `<!-- BEGIN GENERATED` markers are written by scripts; run the
script, do not edit them. The README and the manual's pages are doc-tests, and
several tests read `docs/`; `cargo test -p akantu-validation` and
`cargo test -p akantu --test manual` catch a broken document.

## Before you say a task is done

1. `cargo fmt --all` and `cargo clippy` for the crates you touched.
2. `cargo test -p <crate> --locked` for every crate you changed and for the
   crates that depend on it.
3. The pull-request command above, or say which part you could not run and why.
4. For feature-gated code, MPI, MUMPS or PETSc, Python or `unsafe` code, the
   matching tier as well (`docs/TESTING.md` lists them).
5. Report exactly what you ran, with the results. A skipped check is not a
   pass.

## Ask a human first

- any change of results, defaults or semantics, including the fix of an
  upstream defect that the port reproduces today;
- a new tolerance or a change to one;
- a new dependency, or a dependency update that changes results;
- anything that touches the licence boundary (the GPL crate, code from
  another project);
- regenerating fixtures, adding a known failure, or changing what CI requires.
