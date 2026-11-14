// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_io/test_parser/test_parser_gtest.cc
// (AlgebraicParser.Number/Expr/Term/Factor/Func/Constant/Complex).
// (The upstream test file carries no notice of its own; Akantu project
// notice:)
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! Port of the upstream gtest suite of the scalar expression parser
//! (`parser::algebraic::parse_real`, here `ParamValue::parse(Real, …)` in an
//! empty section). Upstream uses `EXPECT_NEAR(…, eps)`; the port computes the
//! reference with the same C library functions, so the comparison is exact.

// The literals are copied verbatim from the upstream test (they are the
// inputs whose parsing is tested).
#![allow(clippy::approx_constant, clippy::excessive_precision)]

use akantu_parser::{ParamKind, ParamValue, ParserSection, ParserType};

fn parse_real(text: &str) -> f64 {
    let s = ParserSection::new(ParserType::Global, "global");
    match ParamValue::parse(ParamKind::Real, text, &s) {
        Ok(ParamValue::Real(v)) => v,
        other => panic!("{text}: {other:?}"),
    }
}

#[test]
fn algebraic_parser_number() {
    assert_eq!(parse_real("3."), 3.);
    assert_eq!(parse_real("10"), 10.);
    assert_eq!(parse_real("-100.0"), -100.);
    assert_eq!(parse_real("+1e10"), 1e10);
}

#[test]
fn algebraic_parser_expr() {
    assert_eq!(parse_real("3. + 10."), 13.);
    assert_eq!(parse_real("10 - 100"), -90.);
    assert_eq!(parse_real("-1 - -1"), 0.);
    assert_eq!(
        parse_real("1e10 + 4e5 - 1e9 + 1e10"),
        1e10 + 4e5 - 1e9 + 1e10
    );
}

#[test]
fn algebraic_parser_term() {
    assert_eq!(parse_real("3. * 10."), 30.);
    assert_eq!(parse_real("10 * 100"), 1000.);
    assert_eq!(parse_real("-1 / -1"), 1.);
    assert_eq!(parse_real("1e10 / 4e5"), 1e10 / 4e5);
}

#[test]
fn algebraic_parser_factor() {
    assert_eq!(parse_real("3. ** 10."), 3f64.powf(10.));
}

#[test]
fn algebraic_parser_func() {
    // Binary functions
    assert_eq!(parse_real("pow(3., 10.)"), 3f64.powf(10.));
    assert_eq!(parse_real("min(3., 10.)"), 3.);
    assert_eq!(parse_real("max(3., 10.)"), 10.);
    assert_eq!(parse_real("atan2(3.1415, 2.)"), 3.1415f64.atan2(2.));
    assert_eq!(parse_real("fmod(10., 3.)"), 10f64 % 3.);
    assert_eq!(parse_real("hypot(3., 10.)"), 3f64.hypot(10.));
    // Unary functions
    assert_eq!(parse_real("abs(3.)"), 3.);
    assert_eq!(parse_real("acos(.5)"), 0.5f64.acos());
    assert_eq!(parse_real("asin(.5)"), 0.5f64.asin());
    assert_eq!(parse_real("atan(.5)"), 0.5f64.atan());
    assert_eq!(parse_real("ceil(3.)"), 3.);
    assert_eq!(parse_real("cos(3.)"), 3f64.cos());
    assert_eq!(parse_real("cosh(3.)"), 3f64.cosh());
    assert_eq!(parse_real("exp(3.)"), 3f64.exp());
    assert_eq!(parse_real("floor(3.)"), 3.);
    assert_eq!(parse_real("log10(3.)"), 3f64.log10());
    assert_eq!(parse_real("log(3.)"), 3f64.ln());
    assert_eq!(parse_real("sin(3.)"), 3f64.sin());
    assert_eq!(parse_real("sinh(3.)"), 3f64.sinh());
    assert_eq!(parse_real("sqrt(3.)"), 3f64.sqrt());
    assert_eq!(parse_real("tan(3.)"), 3f64.tan());
    assert_eq!(parse_real("tanh(3.)"), 3f64.tanh());
    // acosh/asinh/atanh/erf/erfc/lgamma/tgamma call the C library (not
    // available or not forwarded to libm by Rust's std): compare with
    // upstream's EXPECT_NEAR tolerance against independent values.
    let eps = 1e-12;
    assert!((parse_real("acosh(3.)") - 1.762_747_174_039_086).abs() < eps);
    assert!((parse_real("asinh(3.)") - 1.818_446_459_232_066_7).abs() < eps);
    assert!((parse_real("atanh(.5)") - 0.549_306_144_334_054_8).abs() < eps);
    assert_eq!(parse_real("exp2(3.)"), 8.);
    assert_eq!(parse_real("expm1(3.)"), 3f64.exp_m1());
    assert_eq!(parse_real("log1p(3.)"), 3f64.ln_1p());
    assert_eq!(parse_real("log2(3.)"), 3f64.log2());
    assert!((parse_real("erf(3.)") - 0.999_977_909_503_001_4).abs() < eps);
    assert!((parse_real("erfc(3.)") - 2.209_049_699_858_544e-5).abs() < eps);
    assert!((parse_real("lgamma(3.)") - std::f64::consts::LN_2).abs() < eps);
    assert!((parse_real("tgamma(3.)") - 2.).abs() < eps);
    assert_eq!(parse_real("trunc(3.14)"), 3.);
    assert_eq!(parse_real("round(3.14)"), 3.);
}

#[test]
fn algebraic_parser_constant() {
    assert_eq!(parse_real("e"), 2.718_281_828_459_045_235_4);
    assert_eq!(parse_real("pi"), 3.141_592_653_589_793_238_46);
}

#[test]
fn algebraic_parser_complex() {
    let pi = std::f64::consts::PI;
    assert_eq!(
        parse_real("10. * (cos(3 * pi) + sin(5 * pi / 2)) ** 2"),
        10. * ((3. * pi).cos() + (5. * pi / 2.).sin()).powf(2.)
    );
}
