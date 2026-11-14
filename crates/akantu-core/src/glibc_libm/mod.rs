// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a port of Akantu (EPFL/LSMS). Stable entry point of the
// pure-Rust glibc 2.41 libm ports; the translated glibc
// sources and their notices are in `crate::random::libm` (log, exp, pow, tan)
// and `sincos.rs` / `sincos_table.rs` (sin, cos, sincos).

//! Platform-independent, bit-exact ports of the oracle's glibc libm.
//!
//! # Policy (D-008, `docs/NUMERICAL_VALIDATION.md`)
//!
//! IEEE-754 does not fix the results of transcendental functions: macOS libm
//! and glibc differ (344 `sin` and 403 `cos` differences were measured on
//! 8,999 friction angles), and glibc differs between architectures. Code
//! whose results must reproduce the oracle **bit for bit** on every platform
//! calls these functions instead of [`f64::sin`], [`f64::exp`], …. They
//! reproduce, operation by operation, the code GCC 14 generated for the
//! Debian trixie aarch64 glibc 2.41 (`libm.so.6` sha256 `a8afb2d1…`),
//! including every fused multiply-add of that binary ([`f64::mul_add`],
//! correctly rounded on every platform; Rust never contracts on its own).
//!
//! | Function | glibc symbol | Domain | Verification |
//! |---|---|---|---|
//! | [`log`](fn@log), [`exp`], [`pow`] | `log/exp/pow@@GLIBC_2.29` | all inputs | `tests/random_libm*.rs` (> 10⁸ inputs vs glibc) |
//! | [`tan_checked`] | `tan@@GLIBC_2.17` | `|x| ≤ 25` (`None` beyond) | same |
//! | [`sin_checked`], [`cos_checked`], [`sincos_checked`] | `sin/cos/sincos@@GLIBC_2.17` | `|x| < 105414350`, ±∞, NaN (`None` in the `__branred` range) | `tests/glibc_libm_sincos.rs` (`fixtures/random/libm_sincos.json`: 2²⁰-input hashes per domain and the 8,999-angle Drucker–Prager sweep) |
//!
//! Errors (`errno`, floating-point flags) are not reproduced; return values
//! are, including signed zeros, infinities and NaN-ness.
//!
//! This module is the stable path; `crate::random::glibc_libm` is a hidden
//! alias kept for existing callers.

mod sincos;
mod sincos_table;

pub use crate::random::libm::{exp, log, pow, tan_checked};
pub use sincos::{cos_checked, sin_checked, sincos_checked};
