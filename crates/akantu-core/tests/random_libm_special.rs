// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): special-value checks of the
// pure-Rust ports of glibc 2.41 log/exp/pow/tan (used by the distributions
// behind src/common/aka_random_generator.hh) against the oracle's glibc, from
// oracle/probes/random/libm_special.cc.

//! Cartesian special-value tests of `random::glibc_libm`.
//!
//! `pow` is evaluated on all 58 × 58 pairs of a set of special values (signed
//! zeros, ±1, odd/even integers, half-integers, subnormals, values next to 1,
//! exponent transitions, the largest finite value, infinities, and quiet and
//! signalling NaNs of both signs), and `log`, `exp`, `tan` on each value.
//! Results must equal glibc's bit for bit (NaN results by NaN-ness). These
//! tests run in every configuration of the test suite; with overflow checks
//! enabled (debug, or `-C overflow-checks=on`) they also show that no C
//! unsigned-arithmetic idiom of the ports panics.

use akantu_core::random::glibc_libm::{exp, log, pow, tan_checked};
use serde_json::Value;

fn fixture() -> Value {
    let path = format!(
        "{}/tests/fixtures/random/libm_special_cartesian.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn h(v: &Value) -> u64 {
    u64::from_str_radix(v.as_str().expect("hex string"), 16).expect("hex")
}

/// NaN-ness only, by design: the documented **D-046 exception** for the
/// libm special-value tables (docs/NUMERICAL_VALIDATION.md "Default-NaN
/// convention (D-046)"). The probe records every NaN
/// result as the canonical `0x7ff8000000000000`, and glibc's NaN
/// propagation for NaN *inputs* (which operand's payload, the quieting of
/// signalling NaNs, the sign) is platform-specific, so these tables compare
/// NaN as a class; every non-NaN result stays bitwise.
fn canon(x: f64) -> u64 {
    if x.is_nan() {
        0x7ff8_0000_0000_0000
    } else {
        x.to_bits()
    }
}

fn inputs(fx: &Value) -> Vec<f64> {
    fx["x"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| f64::from_bits(h(v)))
        .collect()
}

#[test]
fn special_fixture_provenance() {
    let fx = fixture();
    assert_eq!(fx["meta"]["glibc"], "2.41");
    // Either authoritative oracle platform (D-043, D-073): the x86-64-v3
    // regeneration reproduces every value of these fixtures bit for bit
    // (class X0); only the metadata names the platform.
    assert!(
        matches!(fx["meta"]["arch"].as_str(), Some("aarch64" | "x86_64")),
        "{}",
        fx["meta"]["arch"]
    );
    let xs = inputs(&fx);
    assert_eq!(xs.len(), 58);
    // Both signs of zero, infinity, quiet and signalling NaNs are present.
    for bits in [
        0x0000_0000_0000_0000_u64,
        0x8000_0000_0000_0000,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff4_0000_0000_0000,
        0xfff4_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0xfff0_0000_0000_0001,
    ] {
        assert!(xs.iter().any(|x| x.to_bits() == bits), "{bits:016x}");
    }
}

#[test]
fn pow_cartesian_special_values_match_glibc() {
    let fx = fixture();
    let xs = inputs(&fx);
    let rows = fx["pow"].as_array().unwrap();
    assert_eq!(rows.len(), xs.len());
    let mut mismatches = Vec::new();
    for (&x, row) in xs.iter().zip(rows) {
        let row = row.as_array().unwrap();
        assert_eq!(row.len(), xs.len());
        for (&y, expected) in xs.iter().zip(row) {
            let got = canon(pow(x, y));
            if got != h(expected) {
                mismatches.push(format!(
                    "pow({:016x}, {:016x}) = {got:016x}, glibc {}",
                    x.to_bits(),
                    y.to_bits(),
                    expected.as_str().unwrap()
                ));
            }
        }
    }
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches.join("\n")
    );
}

#[test]
fn review_rng02_reproducers() {
    // Special cases that overflowed and panicked in debug builds before the fix.
    assert_eq!(pow(-1.0, 0.0).to_bits(), 1.0_f64.to_bits());
    assert_eq!(pow(-0.0, -0.0).to_bits(), 1.0_f64.to_bits());
    assert_eq!(pow(1.0, f64::NEG_INFINITY).to_bits(), 1.0_f64.to_bits());
    assert_eq!(pow(-f64::NAN, 0.0).to_bits(), 1.0_f64.to_bits());
    // A signalling NaN base/exponent is not absorbed by y == 0 / x == 1.
    let snan = f64::from_bits(0x7ff4_0000_0000_0000);
    let neg_snan = f64::from_bits(0xfff0_0000_0000_0001);
    assert!(pow(snan, 0.0).is_nan());
    assert!(pow(neg_snan, -0.0).is_nan());
    assert!(pow(1.0, snan).is_nan());
    assert!(pow(1.0, neg_snan).is_nan());
}

#[test]
fn log_exp_tan_special_values_match_glibc() {
    let fx = fixture();
    let xs = inputs(&fx);
    let mut checked_tan = 0;
    for (i, &x) in xs.iter().enumerate() {
        assert_eq!(canon(log(x)), h(&fx["log"][i]), "log({:016x})", x.to_bits());
        assert_eq!(canon(exp(x)), h(&fx["exp"][i]), "exp({:016x})", x.to_bits());
        // Finite |x| > 25 is outside the ported tan domain (not reachable
        // from the Cauchy distribution); tan_checked reports it as None.
        match tan_checked(x) {
            Some(t) => {
                assert_eq!(canon(t), h(&fx["tan"][i]), "tan({:016x})", x.to_bits());
                checked_tan += 1;
            }
            None => assert!(x.is_finite() && x.abs() > 25.0, "{x:e}"),
        }
    }
    // 19 magnitudes (14 finite <= 25, inf, 4 NaNs) with both signs.
    assert_eq!(checked_tan, 38);
}

/// The public verification entry point never panics and
/// reports arguments outside the ported domain explicitly (no host fallback).
#[test]
fn tan_checked_public_boundary() {
    use akantu_core::random::glibc_libm::tan_checked;
    assert!(tan_checked(25.0).is_some());
    assert!(tan_checked(-25.0).is_some());
    for x in [
        25.000_000_000_000_004,
        26.0,
        -26.0,
        f64::MAX,
        f64::MIN,
        1e300,
    ] {
        assert_eq!(tan_checked(x), None, "tan_checked({x:e})");
    }
    assert!(tan_checked(std::f64::consts::PI * 0.999_999_999).is_some());
}
