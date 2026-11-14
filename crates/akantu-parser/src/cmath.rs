// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the function tables of
// src/io/parser/algebraic_parser_x3.cc and src/io/parser/algebraic_parser.hh
// (unary/binary functions of the algebraic grammars).
// Copyright (©) 2013-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! The C math functions available in input-file expressions.
//!
//! Upstream binds the `<cmath>` functions (`std::sin`, `std::erf`, …). To
//! reproduce its values exactly, every function here calls the **same C
//! library function** (the platform `libm`, e.g. glibc on the Linux oracle):
//!
//! * Rust's `f64::{sin, cos, tan, asin, acos, atan, sinh, cosh, tanh, exp,
//!   exp2, exp_m1, ln, ln_1p, log2, log10, powf, atan2, hypot}` lower to
//!   calls of the identically named C functions; `sqrt`, `abs`, `floor`,
//!   `ceil`, `trunc`, `round` and `%` (`fmod`) are exact IEEE operations, so
//!   they agree with C by construction.
//! * `asinh`, `acosh` and `atanh` are *not* forwarded to libm by Rust's
//!   standard library (it uses its own formulas, which can differ in the last
//!   bit), and `erf`, `erfc`, `tgamma` and `lgamma` are not available on
//!   stable Rust: these seven are declared below and called through FFI.
//!   `lgamma` is called as the reentrant `lgamma_r`: it returns the same
//!   value as `lgamma` and writes the sign of Γ(x) to a caller-provided
//!   `int` instead of the process-global `signgam`, which C `lgamma`
//!   updates without synchronisation (a data race when conversions run on
//!   several threads). The parser only uses the returned value, never the
//!   sign, so the result is unchanged (implementation adaptation, proposed
//!   category A). `lgamma_r` is declared by glibc (`_DEFAULT_SOURCE`) and by
//!   the Darwin SDK (since macOS 10.6); the measured values differ from
//!   glibc only where listed in `tests/oracle.rs` (`LIBM_EXCEPTIONS`).
//! * `std::min`/`std::max` are the C++ templates (`(b < a) ? b : a` and
//!   `(a < b) ? b : a`), not IEEE `fmin`/`fmax`: their NaN and signed-zero
//!   behaviour is reproduced.
//!
//! Results are therefore bitwise equal to the C++ reference **on the same
//! platform libm**; transcendental functions may differ in the last bit
//! between platforms (e.g. glibc vs. Apple libm), exactly as the C++ code
//! would.

#![allow(unsafe_code)] // FFI to libm, see the SAFETY comments below.

mod ffi {
    // SAFETY (declarations): these are the C99 <math.h> functions with their
    // exact C signatures (`double f(double)`, `double lgamma_r(double, int*)`)
    // exported by the platform libm, which the Rust standard library already
    // links on all supported (Linux/macOS) targets.
    unsafe extern "C" {
        pub safe fn erf(x: f64) -> f64;
        pub safe fn erfc(x: f64) -> f64;
        pub safe fn tgamma(x: f64) -> f64;
        pub safe fn asinh(x: f64) -> f64;
        pub safe fn acosh(x: f64) -> f64;
        pub safe fn atanh(x: f64) -> f64;
        pub unsafe fn lgamma_r(x: f64, sign: *mut core::ffi::c_int) -> f64;
    }
}

fn erf(x: f64) -> f64 {
    ffi::erf(x)
}
fn erfc(x: f64) -> f64 {
    ffi::erfc(x)
}
fn tgamma(x: f64) -> f64 {
    ffi::tgamma(x)
}
fn asinh(x: f64) -> f64 {
    ffi::asinh(x)
}
fn acosh(x: f64) -> f64 {
    ffi::acosh(x)
}
fn atanh(x: f64) -> f64 {
    ffi::atanh(x)
}

/// `std::lgamma` (via `lgamma_r`, see the module documentation).
#[must_use]
pub fn lgamma(x: f64) -> f64 {
    let mut sign: core::ffi::c_int = 0;
    // SAFETY: `sign` is a valid, initialised, exclusively borrowed `int` that
    // lives for the whole call; `lgamma_r` writes only the sign of Γ(x)
    // through this pointer and does not retain it. Its other effects are
    // those of every libm function (thread-local `errno` and floating-point
    // exception flags), which are not memory-safety concerns; it does not
    // touch the global `signgam`.
    unsafe { ffi::lgamma_r(x, &raw mut sign) }
}

/// Signature of the unary functions.
pub type Unary = fn(f64) -> f64;
/// Signature of the binary functions.
pub type Binary = fn(f64, f64) -> f64;

/// `std::min(a, b)` = `(b < a) ? b : a`.
#[must_use]
#[allow(clippy::float_cmp)] // exact comparison semantics of std::min
pub fn cxx_min(a: f64, b: f64) -> f64 {
    if b < a { b } else { a }
}

/// `std::max(a, b)` = `(a < b) ? b : a`.
#[must_use]
pub fn cxx_max(a: f64, b: f64) -> f64 {
    if a < b { b } else { a }
}

/// Unary functions in upstream registration order (names are matched by
/// longest prefix, so order only matters for documentation).
pub static UNARY: &[(&str, Unary)] = &[
    ("abs", f64::abs),
    ("acos", f64::acos),
    ("asin", f64::asin),
    ("atan", f64::atan),
    ("ceil", f64::ceil),
    ("cos", f64::cos),
    ("cosh", f64::cosh),
    ("exp", f64::exp),
    ("floor", f64::floor),
    ("log10", f64::log10),
    ("log", f64::ln),
    ("sin", f64::sin),
    ("sinh", f64::sinh),
    ("sqrt", f64::sqrt),
    ("tan", f64::tan),
    ("tanh", f64::tanh),
    ("acosh", acosh),
    ("asinh", asinh),
    ("atanh", atanh),
    ("exp2", f64::exp2),
    ("expm1", f64::exp_m1),
    ("log1p", f64::ln_1p),
    ("log2", f64::log2),
    ("erf", erf),
    ("erfc", erfc),
    ("lgamma", lgamma),
    ("tgamma", tgamma),
    ("trunc", f64::trunc),
    ("round", f64::round),
];

fn fmod(a: f64, b: f64) -> f64 {
    a % b
}

/// Binary functions in upstream registration order.
pub static BINARY: &[(&str, Binary)] = &[
    ("pow", f64::powf),
    ("min", cxx_min),
    ("max", cxx_max),
    ("atan2", f64::atan2),
    ("fmod", fmod),
    ("hypot", f64::hypot),
];

/// Constants (`pi`, `e`): `std::numbers::pi_v<double>` / `M_PI` and
/// `std::numbers::e_v<double>` / `M_E` — the correctly rounded doubles.
pub static CONSTANTS: &[(&str, f64)] = &[("pi", std::f64::consts::PI), ("e", std::f64::consts::E)];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn constants_are_the_cxx_literals() {
        assert_eq!(CONSTANTS[0].1.to_bits(), 0x4009_21FB_5444_2D18);
        assert_eq!(CONSTANTS[1].1.to_bits(), 0x4005_BF0A_8B14_5769);
        let pi: f64 = "3.14159265358979323846".parse().unwrap();
        let e: f64 = "2.7182818284590452354".parse().unwrap();
        assert_eq!(CONSTANTS[0].1, pi);
        assert_eq!(CONSTANTS[1].1, e);
    }

    #[test]
    fn min_max_semantics() {
        assert!(cxx_min(f64::NAN, 1.0).is_nan());
        assert_eq!(cxx_min(1.0, f64::NAN), 1.0);
        assert!(cxx_max(f64::NAN, 1.0).is_nan());
        assert_eq!(cxx_max(1.0, f64::NAN), 1.0);
        assert!(cxx_min(0.0, -0.0).is_sign_positive());
        assert!(cxx_max(-0.0, 0.0).is_sign_negative());
    }

    #[test]
    fn ffi_functions() {
        assert!((lgamma(3.0) - 2.0f64.ln()).abs() <= 2.0 * f64::EPSILON);
        assert_eq!(ffi::tgamma(5.0), 24.0);
        assert_eq!(ffi::erf(0.0), 0.0);
        assert_eq!(ffi::erfc(0.0), 1.0);
        assert_eq!(ffi::acosh(1.0), 0.0);
        assert_eq!(ffi::asinh(0.0), 0.0);
        assert_eq!(ffi::atanh(0.0), 0.0);
        assert!(lgamma(-1.0).is_infinite());
    }
}
