<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Contributing to akantu-rs

akantu-rs is a Rust port of the Akantu finite-element library. Its results are
checked against the C++ library at one pinned commit, most of them bit for bit.
Contributions keep that evidence intact, so the rules below are stricter than
in most Rust projects, and each comes with its reason. If you know the C++
library and not Rust, start with the user manual (`docs/manual/`); its chapter
on differences from the C++ library lists the conventions of the Rust API.

The port was written by AI coding agents under independent review;
`docs/reports/` describes how it was made and verified.

## Getting started

You need git and rustup. rustup installs the toolchain pinned in
`rust-toolchain.toml` (Rust 1.99.1 with rustfmt and clippy) on the first
`cargo` command; the oldest supported Rust is 1.90 (`rust-version` in
`Cargo.toml`).

```sh
git clone <repository URL> akantu-rs && cd akantu-rs
cargo build --workspace --locked
cargo test -p akantu-solid --locked      # the tests of one crate, minutes
```

The default build is pure Rust. MPI, Scotch, MUMPS, PETSc and the Python
package need native libraries; instead of installing them, use the project's
Linux images under `docker/`, which the test scripts start for you:

```sh
ci/github/images.sh ~/akantu-images mpi scotch mumps petsc   # builds the images once
ci/run-all.sh --backend docker                               # the fast tier, as CI runs it
```

The fast tier builds and tests the whole workspace: its cargo target reaches
21 to 27 GB, and the heavier tiers need about 8 GB of RAM and 60 GB of free
disk. On macOS the crate tests run natively; the Docker tiers need a Linux VM
with Docker. `docs/TESTING.md` describes every tier, the images and the
environment variables, and `docs/PLATFORMS.md` what holds on each platform.

## Finding something to work on

- Open issues, and the questions in the repository's discussions.
- `docs/FEATURE_PARITY.md`: rows with the status "Exception (approved)" are
  upstream features the port does not provide or provides differently, each
  with its reason.
- `docs/reports/`: the reports on the port list possible improvements of
  functionality, performance and maintainability.
- Upstream commits after the pinned one (`docs/REFERENCE.md`): following them
  is described in `docs/MAINTAINING.md`.

## Workflow

1. For anything larger than a small fix, open an issue first and describe what
   you want to change. A change of results or of a default needs agreement
   before code is written.
2. Fork the repository, create a branch and keep each pull request to one
   subject.
3. Run the checks locally (below), then open the pull request and fill in its
   checklist.
4. CI runs on every pull request: the fast tier (formatting, clippy, the
   workspace tests with every test binary accounted for, fixture checksums,
   parity statistics, licences), the CI self-test, the check with the oldest
   supported Rust, the API documentation and the manual. Feature-gated code,
   MPI, MUMPS and PETSc, the Python package, memcheck and coverage run weekly;
   the maintainer runs them for your change when it needs them.
5. The maintainer reviews the change. A change of numerical code is reviewed
   against the C++ code it ports.

Before you open the pull request:

```sh
cargo fmt --all
cargo clippy --workspace --all-targets --locked -- -D warnings
CI_INSTALL_TOOLCHAINS=1 ci/run-all.sh --backend docker \
  --tiers lint,test,fixtures,parity,licence,msrv,docs
```

The last command runs what CI runs on a pull request, except the manual's
build and `ci/selftest.sh` (run the latter too after a change under `ci/`).
Its lint tier needs `shellcheck` and ruff 0.17.3 on the host; without them it
records a skip and the run ends `CI: INCOMPLETE`. If you cannot run it (no
Docker, not enough disk), run the tests of the crates you changed and say so
in the pull request.

## Technical rules and their reasons

Floating-point semantics. Use `f64` where upstream uses `Real`, keep the order
of floating-point operations of the C++ code, and use no fast-math, no
reassociation and no approximate intrinsics. `mul_add` is allowed only where it
is shown to give the C++ build's results, with the reason written next to it
(the three existing places reproduce fused operations of the oracle's build:
`crates/akantu-core/src/glibc_libm/sincos.rs`,
`crates/akantu-core/src/random/libm.rs` and
`crates/akantu-linalg/src/dense/complex.rs`). The golden data were
computed by the C++ library; a sum in another order changes the last bits, and
then a bitwise comparison can no longer tell a real difference from rounding.
Use the index types of `akantu-core` (`docs/ARCHITECTURE.md`).

Tolerances. Never loosen a tolerance to make a test pass. A tolerance that
hides a difference hides the bug behind it. A comparison that cannot be
bitwise names a rule in the area's `tolerances.json`, with the reason and the
evidence; `docs/TOLERANCE_LEDGER.md` lists every rule and is generated from
those files (`python3 scripts/gold/tolerance_ledger.py`). A new rule needs a
review and a section in `docs/NUMERICAL_VALIDATION.md`.

Failing tests. Never `#[ignore]` or delete a failing test to make CI green. The
test tier counts every test binary and compares the failures with
`ci/tools/known-failures.txt`, which is empty at release; a known failure is
listed there with its reason, through a reviewed change.

Golden files. Never edit a file under `fixtures/` by hand. Each case records
the oracle image, the upstream commit and the command that produced it, and is
regenerated from the oracle (`docs/ORACLE_BUILD.md`). A hand-edited value has
no provenance. The recorded oracle image is not published and its Debian
packages are not version-pinned, so an image you build yourself is a different
numerical environment until it passes the byte-identity check of
`docs/ORACLE_BUILD.md` ("Checking that a regeneration is byte-identical";
`oracle/scripts/build-image.sh` reports package drift).

`unsafe` code. Crates without `unsafe` code forbid it. Every `unsafe` block
carries a `// SAFETY:` comment with its invariants, and its row in the crate's
`UNSAFE.md` changes in the same commit
(`python3 scripts/unsafe_inventory.py --check --strict-safety`). The inventory
lets a reviewer check each invariant; memcheck and Miri cover the foreign
function interfaces.

Licensing. Every new source file starts with
`// SPDX-License-Identifier: LGPL-3.0-or-later` and a line naming the Akantu
file it is ported from. Code derived from CGAL's GPL packages lives only in
`crates/akantu-geometry-cgal`, which is opt-in, so the default build stays
LGPL. Never copy code whose licence is not compatible with LGPL-3.0-or-later
(GPL-only code such as libstdc++); reproduce such behaviour from its
specification instead. Third-party code and native libraries are listed in
`docs/DEPENDENCIES.md` and `NOTICE.md`.

Errors and lints. Library code returns `Result<_, akantu_core::Error>` and does
not panic on input a user controls. `cargo fmt` and
`cargo clippy -- -D warnings` are clean; an `#[allow(...)]` carries a comment
saying why.

Tests. A ported upstream test keeps the intent and the criteria of the
original, is named after it and gets its row in `docs/FEATURE_PARITY.md`. Add
independent checks where they exist: patch tests, partition of unity,
rigid-body modes, analytical solutions, convergence rates. `docs/TESTING.md`
explains how to add a test and a fixture.

Documents. A change updates the documents it affects: the parity matrix for a
feature's status, `docs/UPSTREAM_DEFECTS.md` for a defect of the C++ library,
`docs/DECISIONS.md` for a technical decision, `CHANGELOG.md` for anything a
user notices. Blocks marked `BEGIN GENERATED` are written by scripts. The
README and the manual's pages are compiled and run as doc-tests.

## Reporting a difference from C++ Akantu

Use the issue form "Difference from C++ Akantu". A difference can be a bug of
the port, a defect of the C++ library, or a platform effect, and the details
below decide which:

- the input file (`.dat`) and the mesh (`.msh`; for a `.geo` file also the Gmsh
  version, since Gmsh output differs between platforms);
- the driver: the C++ program or Python script, and the Rust or Python code
  that runs the same case with akantu-rs;
- the C++ Akantu version or commit and how it was built (build type, solver
  back-end, compiler, BLAS);
- the akantu-rs commit and the cargo features;
- the platform: operating system, CPU architecture and model, number of MPI
  ranks;
- the quantities that differ and by how much (largest absolute and relative
  difference, or ULPs), and at which step.

Search `docs/UPSTREAM_DEFECTS.md` first: many differences are known defects of
the C++ library that the port corrects on purpose, each with a switch that
reproduces upstream's behaviour.

## Changing behaviour where the port reproduces an upstream defect

The port classifies each defect of the C++ library (`docs/UPSTREAM_DEFECTS.md`,
decision D-003 in `docs/DECISIONS.md`):

- category A, undefined behaviour (uninitialised memory, out-of-bounds): the
  port uses defined behaviour;
- category B, a clear bug with wrong results: the port computes the correct
  result by default, and a compatibility switch reproduces upstream for the
  differential tests where that is practical;
- category C, a quirk of behaviour or API semantics: the port keeps upstream's
  behaviour by default and may add an explicit opt-in.

To propose a change, open an issue with the defect's id (or a new one), the
evidence (the upstream code, an oracle run, an analytical result) and the
category you propose. A change from C to B changes default results and needs
the maintainer's agreement before code is written. The procedure is in
`docs/MAINTAINING.md`.

## Commits and pull requests

- One logical change per commit. The subject line is short and imperative and
  names the area (`solid: return the plastic work of the last step`); the body
  says why.
- A pull request describes what changed, how it was tested, and whether any
  result changed. Link the issue.
- Keep refactoring separate from changes of behaviour.

## Licence of contributions

Contributions are accepted under the licence of the files they change
(inbound equals outbound): LGPL-3.0-or-later for the project, GPL-3.0-or-later
for `crates/akantu-geometry-cgal`. You keep your copyright; do not remove
existing copyright notices. By opening a pull request you confirm that you
have the right to submit the code under these terms.

## AI-assisted contributions

Contributions written with AI tools are welcome. You are responsible for the
result as if you had written it yourself: you have read it, you understand it,
and you have run the tests. Say in the pull request which tools you used and
for what. Coding agents find their instructions in `AGENTS.md`.

## Conduct and security

The project follows the code of conduct in `CODE_OF_CONDUCT.md`. Report
security problems privately, as described in `SECURITY.md`, not in a public
issue.
