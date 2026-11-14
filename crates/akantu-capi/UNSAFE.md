# `unsafe` sites of `akantu-capi`

The crate is `#![deny(unsafe_code)]`; the C API is a stub today (one
exported function; D-100: the full C API is an approved exception for now).
Every site is listed below (`scripts/unsafe_inventory.py --check`, run by
`cargo test -p akantu-validation --test unsafe_inventory`; `--update-lines`
after an edit that only moves code).
Each row is also pinned to a fingerprint of its site's code (comments and
whitespace ignored) in `UNSAFE.fingerprints`: a site
whose code changed fails the check, and `--update-lines` does not move its
row, until the row is reviewed and re-pinned with `--refresh <Id>`.
A fingerprint covers the Rust site's own tokens only: not the code around
it that sets up its invariants, and not C sources, which the guard
does not read; a change there shows only in the commit's diff and its
review.

**Counts**: library — unsafe attribute 1, foreign-ABI function 1.

## Site table

### src/lib.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| CA-01 | 26 | `(crate root)` attr 1 | `#[unsafe(no_mangle)]` on `akantu_version` (edition 2024 marks unmangled exports unsafe: a symbol clash is link-time UB) | the comment above it: the function performs no unsafe operation; the exported name carries the `akantu_` prefix | the symbol name (unique prefix) | unit test `version_matches_crate` |
| CA-02 | 27 | `fn akantu_version` ffi-fn 1 | returns a pointer to a `'static`, NUL-terminated version string | never unwinds (no panicking operation), returns a pointer to static data that callers must not free | the body (`c"0.1.0".as_ptr()`) | `version_matches_crate` |

## Dynamic checks

* **valgrind memcheck**: the crate's unit tests (one
  process, MPI image): 0 errors.
