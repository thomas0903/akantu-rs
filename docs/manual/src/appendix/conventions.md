# Coding conventions and design artefacts

Upstream ships, besides the library and the manual, a few files for its
developers: coding conventions with Emacs helpers
(`doc/coding_convention/`), a UML diagram (`doc/uml.dia`), formatter and
linter settings (`.clang-format`, `.clang-tidy`) and, since 2026, an
`AGENTS.md` with instructions for coding agents. This appendix says what
the port provides for each, or why it does not.

| Upstream artefact | What it is | In the port |
|---|---|---|
| `doc/coding_convention/README` | naming (`MyClass`, `myMethod()`, `my_variable`, `my_class.hh`), inline functions in `*_inline_impl.hh`, Doxygen comments, Emacs set-up | Rust conventions instead (below): `rustfmt` formatting, `snake_case` functions, `CamelCase` types, one module per file, rustdoc comments |
| `doc/coding_convention/akantu-input-mode.el` | Emacs major mode for input files: indentation of `[ … ]` sections and `\` continuations, highlighting of section keywords and parameter names | Emacs mode not ported or shipped (below) |
| `doc/coding_convention/snippet/{akantu, class, for, inout, once}` | yasnippet templates: `namespace akantu`, class skeleton with `printself` and `operator<<`, `for` loop, `AKANTU_DEBUG_IN/OUT`, include guard | not applicable to Rust (below) |
| `doc/uml.dia` | Dia class diagram of the original design (`StaticMemory`, `Vector`, `Mesh`, `FEM`, `Model`, `Material`) | not ported; the port's design document is [`docs/ARCHITECTURE.md`](../../../ARCHITECTURE.md) |
| `.clang-format`, `.clang-tidy`, `.clangd` | C++ formatting (LLVM style) and lint configuration | `cargo fmt` (default `rustfmt` style) and `cargo clippy --all-targets -- -D warnings` with the workspace lint table (`[workspace.lints]` of the root `Cargo.toml`); toolchain pinned in `rust-toolchain.toml` |
| `AGENTS.md` (upstream) | build, test, lint and commit instructions for coding agents working on the C++ code | the port has its own [`AGENTS.md`](../../../../AGENTS.md), with instructions for contributors and their coding agents; upstream's describes the C++ build and does not apply |

## Conventions of the port

The rules are in the port's `AGENTS.md`, `CONTRIBUTING.md`
and [`docs/ARCHITECTURE.md`](../../../ARCHITECTURE.md):

* Names. Upstream names keep their meaning and lose their C++ casing:
  `initFull` → `init_full`, `getDamage` → `damage()`, `SolidMechanicsModel`
  stays `SolidMechanicsModel`; enumeration values drop the leading
  underscore (`_explicit_lumped_mass` → `AnalysisMethod::ExplicitLumpedMass`);
  input-file keywords are upstream's spellings
  ([Input files](../input-files.md)).
* Files. One module per file, no header/implementation split, no
  `*_inline_impl.hh`; each file starts with
  `// SPDX-License-Identifier: LGPL-3.0-or-later` and a line naming the
  upstream file(s) it is ported from (`GPL-3.0-or-later` in
  `akantu-geometry-cgal`, [Geometry](../geometry.md#the-cgal-feature-gpl-30-or-later)).
* Documentation. rustdoc comments (`cargo doc`) instead of Doxygen;
  this manual for user-level documentation, every example a doc-test.
* Errors. Functions return `Result<_, akantu_core::Error>` instead of
  throwing; `AKANTU_DEBUG_ASSERT` becomes a debug assertion for internal
  invariants only; no panics on user input.
* Tests. Each upstream test is ported to a Rust test named after it
  (the mapping is in [`docs/FEATURE_PARITY.md`](../../../FEATURE_PARITY.md));
  test tiers are in [`docs/TESTING.md`](../../../TESTING.md).

## Why the Emacs mode is not ported

The mode is an editor tool outside the library. A Rust port of it would
make no sense, and shipping a copy would add an Emacs Lisp file to
maintain without a way to test it here. Since the port reads upstream's
input-file grammar unchanged ([Input files](../input-files.md)), upstream's
file can be used with the port's input files as it is, with two limits
visible in its source:

* its highlighted section keywords are `global`, `material`, `model`,
  `mesh`, `heat`, `contact`, `friction`, `embedded_interface`, `rules`,
  `non_local` and `user` (`akantu-input-mode.el`, `font-lock` list). All
  are section types of the port, but the newer section types
  (`phasefield`, `solver`, `time_step_solver`, `non_linear_solver`,
  `integration_scheme` and others) are not highlighted (test
  `manual_claims_2.rs::doc024_emacs_mode_keywords`);
* its section-opening pattern accepts names made of letters, digits and
  `_` only, so a section whose name contains `:` (the split-node friction
  laws such as `friction rate_and_state:dieterich:ageing [`, [Split-node
  contact](../models/ntn-contact.md#friction-laws)) is not recognised as
  a section by its indentation function.

## Why the snippets are not ported

They generate C++ boilerplate that has no Rust counterpart: modules
replace `namespace akantu { … }` and include guards, `Debug`/`Display`
replace `printself` and `operator<<`, and the `log` crate replaces the
`AKANTU_DEBUG_IN()`/`AKANTU_DEBUG_OUT()` tracing macros.

## Why the UML diagram is not ported

`doc/uml.dia` (a gzip-compressed Dia file) shows the first design of
upstream: a `StaticMemory` singleton managing typed `Vector`s identified
by `VectorID`/`MemoryID`, a `FEM` object and materials holding
`Vector<double>` references. Upstream itself replaced that design long
ago (`Array`, `FEEngine`, `ElementTypeMapArray`, …), so the diagram
documents neither upstream's current code nor the port. The port's
structure (crates, ownership of models, solvers and laws, traits) is
described in [`docs/ARCHITECTURE.md`](../../../ARCHITECTURE.md).
