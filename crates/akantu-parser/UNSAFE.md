# `unsafe` sites of `akantu-parser`

Every `unsafe` site of this crate is listed below, one row per site
(`scripts/unsafe_inventory.py --check`, run by `cargo test -p
akantu-validation --test unsafe_inventory`, fails when a site is missing,
moved or gone; `--update-lines` rewrites the Line column after an edit
that only moves code).
Each row is also pinned to a fingerprint of its site's code (comments and
whitespace ignored) in `UNSAFE.fingerprints`: a site
whose code changed fails the check, and `--update-lines` does not move its
row, until the row is reviewed and re-pinned with `--refresh <Id>`.
A fingerprint covers the Rust site's own tokens only: not the code around
it that sets up its invariants, and not C sources, which the guard
does not read; a change there shows only in the commit's diff and its
review.

**Where `unsafe` is allowed.** `src/lib.rs`: `#![deny(unsafe_code)]`; only
the module `src/cmath.rs` opts out (`#![allow(unsafe_code)] // FFI to
libm`). It calls seven C99 `<math.h>` functions of the platform libm that
Rust's standard library does not forward (`erf`, `erfc`, `tgamma`, `asinh`,
`acosh`, `atanh`) or that it does not provide (`lgamma`, called as the
reentrant `lgamma_r` so that no process-global `signgam` is written: the
parser uses only the value). The libm is linked by `std` on every
supported (Linux, macOS) target. D-030: no glibc source was read; the
signatures come from the C standard and the platform headers.

**Counts**: block 1, `extern` block 1 (six `safe fn` and one `unsafe fn`
declared in it) — **2 library sites, no test site**.

## Site table

### src/cmath.rs

| Id | Line | Site | What it does | Invariant (its `SAFETY:` comment) | Established by | Test |
|---|---|---|---|---|---|---|
| PA-01 | 52 | `mod ffi` extern 1 [erf, erfc, tgamma, asinh, acosh, atanh, lgamma_r] | declares the libm functions; the six `double f(double)` are declared **`safe fn`** (callable without `unsafe`), `lgamma_r` stays `unsafe` (it takes a pointer) | the exact C99 signatures (`double f(double)`, `double lgamma_r(double, int *)`); the `safe` ones have no precondition (any `f64`, including NaN and infinities, is a valid argument; their only side effects are the thread-local `errno` and the floating-point flags) | declaration (C99 7.12, Darwin and glibc headers: `lgamma_r` under `_DEFAULT_SOURCE` / macOS ≥ 10.6) | `cmath::tests::ffi_functions`, `tests/oracle.rs` (values against the C++ oracle, `LIBM_EXCEPTIONS`) |
| PA-02 | 92 | `fn lgamma` block 1 | `lgamma_r(x, &sign)` | `sign` is a valid, initialised, exclusively borrowed `int` living for the call; `lgamma_r` writes only the sign through it and does not retain it; other effects (`errno`, FP flags) are not memory-safety concerns; `signgam` untouched | the local variable (the pointer is a `&raw mut` of a live local) | `cmath::tests::ffi_functions` (incl. a pole), `tests/oracle.rs` |

## Dynamic checks

* **Miri** (`nightly-2026-10-01`): Miri implements
  `lgamma_r`, `erf`, `erfc` and `tgamma` as shims and ran
  `cmath::tests::ffi_functions` up to `atanh`, which it does not implement
  ("unsupported operation: can't call foreign function `atanh`"); no
  undefined behaviour was reported for the `lgamma_r` call (PA-02). Without
  `-Zmiri-deterministic-floats` the test stops earlier on its 2-ulp
  assertion of `lgamma(3)` (Miri adds deliberate imprecision to libm
  results).
* **valgrind memcheck**: the crate's unit tests (one
  process, MPI image): 0 errors.
