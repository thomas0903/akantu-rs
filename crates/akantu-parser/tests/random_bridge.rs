// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs integration test of the RNG ↔ parser bridge (`docs/ARCHITECTURE.md`):
// parsed random parameters of src/io/parser/parser_real.cc
// (Parser::parseRandomParameter → make_random_parameter) sampled and printed
// by akantu_core::random (src/common/aka_random_generator.hh).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! Evidence that the integration of the parser and the random generators
//! changed no value:
//!
//! * `.dat` text → parser → [`RandomParameter::to_core`] → draws: bit-exact
//!   against the oracle's `make_random_parameter` records
//!   (`akantu-core/tests/fixtures/random/random_parameter.json`), including
//!   engine state and `printself` after every chunk;
//! * the parser's printing (now delegated to the core clean-room printing)
//!   against the random-number printing records;
//! * a differential sweep of the delegated printing against the parser's
//!   former formatter (copied verbatim below from the clean-room version of
//!   `src/param.rs`), over special and pseudo-random doubles.
//!
//! The parser's own fixtures (`stream_blackbox.jsonl`: 264 distribution
//! texts; the oracle dumps: `printself` of every random-parameter
//! conversion) are checked by `tests/stream_blackbox.rs` and
//! `tests/oracle.rs` through the same delegated printing.

mod common;

use std::path::Path;

use akantu_core::random::{
    MinstdRand0, RandomDistributionType, RandomParameter as CoreRandomParameter,
};
use akantu_parser::{RandomDistribution, RandomParameter, SearchScope, stream};
use serde_json::Value;

fn core_fixture(name: &str) -> Value {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../akantu-core/tests/fixtures/random")
        .join(name);
    serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap()
}

fn hex(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().unwrap(), 16).unwrap())
}

/// The descriptor the parser produces for `base type [a, b]` (file order).
fn descriptor(ty: &str, a: f64, b: f64) -> RandomDistribution {
    match ty {
        "uniform" => RandomDistribution::Uniform(a, b),
        "exponential" => RandomDistribution::Exponential(a),
        "gamma" => RandomDistribution::Gamma(a, b),
        "weibull" => RandomDistribution::Weibull(a, b),
        "extreme_value" => RandomDistribution::ExtremeValue(a, b),
        "normal" => RandomDistribution::Normal(a, b),
        "lognormal" => RandomDistribution::Lognormal(a, b),
        "chi_squared" => RandomDistribution::ChiSquared(a),
        "cauchy" => RandomDistribution::Cauchy(a, b),
        "fisher_f" => RandomDistribution::FisherF(a, b),
        "student_t" => RandomDistribution::StudentT(a),
        other => panic!("unknown distribution {other}"),
    }
}

/// `.dat` → parser → bridge → sampler, against the oracle records.
#[test]
fn parsed_random_parameters_sample_like_the_oracle() {
    let f = core_fixture("random_parameter.json");
    let mut checked = 0;
    for case in f["make"].as_array().unwrap() {
        let ty = case["type"].as_str().unwrap();
        let nparams = case["nparams"].as_u64().unwrap();
        let (base, a, b) = (hex(&case["base"]), hex(&case["a"]), hex(&case["b"]));
        // The grammar needs at least one parameter after a distribution
        // name; `uniform` with none is a direct `make_random_parameter` call
        // of the probe that no input file can express.
        let text = match (ty, nparams) {
            ("not_defined", 0) => format!("x = {base:e}\n"),
            (_, 0) => continue,
            (_, 1) => format!("x = {base:e} {ty} [{a:e}]\n"),
            _ => format!("x = {base:e} {ty} [{a:e}, {b:e}]\n"),
        };
        let parser = common::parse_text(&text).unwrap();
        let parsed: RandomParameter = parser
            .root
            .get_parameter_value("x", SearchScope::Current)
            .unwrap();
        // The decimal text must denote the recorded doubles.
        assert_eq!(parsed.base_value.to_bits(), base.to_bits(), "{text}");
        let expected_descriptor = match (ty, nparams) {
            ("not_defined", _) => None,
            (_, 1) => Some(descriptor(ty, a, 0.0)),
            _ => Some(descriptor(ty, a, b)),
        };
        assert_eq!(parsed.distribution, expected_descriptor, "{text}");

        // Bridge = core `from_parsed` with the parsed parameter list.
        let mut sampler = parsed.to_core();
        let ty_core: RandomDistributionType = ty.parse().unwrap();
        let params: &[f64] = match nparams {
            0 => &[],
            1 => &[a],
            _ => &[a, b],
        };
        assert_eq!(
            sampler,
            CoreRandomParameter::from_parsed(base, ty_core, params),
            "{text}"
        );
        assert_eq!(sampler, CoreRandomParameter::from(&parsed), "{text}");

        let context = format!("{text:?}");
        assert_eq!(case["printed_initial"], parsed.to_string(), "{context}");
        assert_eq!(case["printed_initial"], sampler.to_string(), "{context}");
        // The probe seeds RandomGenerator<Idx> with 1 before each case.
        let mut e = MinstdRand0::with_seed_long(1);
        for (k, chunk) in case["chunks"].as_array().unwrap().iter().enumerate() {
            let expected: Vec<u64> = chunk["values"]
                .as_array()
                .unwrap()
                .iter()
                .map(|v| hex(v).to_bits())
                .collect();
            let mut got = vec![0.0; expected.len()];
            sampler.set_values_with(&mut e, &mut got);
            let got: Vec<u64> = got.iter().map(|x| x.to_bits()).collect();
            assert_eq!(got, expected, "{context} chunk {k}");
            assert_eq!(chunk["state"], e.state(), "{context} chunk {k}");
            assert_eq!(chunk["printed"], sampler.to_string(), "{context} chunk {k}");
        }
        checked += 1;
    }
    assert_eq!(checked, 13, "every expressible record was checked");
}

/// The random-number printing records, through the parser's types.
#[test]
fn parser_printing_matches_the_rng_printing_records() {
    let f = core_fixture("random_parameter.json");
    let mut n = 0;
    for case in f["print_base"].as_array().unwrap() {
        let x = hex(&case["x"]);
        let p = RandomParameter {
            base_value: x,
            distribution: None,
        };
        let expected = format!(
            "{} + uniform [ 0.00000000000000000e+00 0.00000000000000000e+00 ]",
            case["s"].as_str().unwrap()
        );
        assert_eq!(p.to_string(), expected, "base {x:e}");
        n += 1;
    }
    for case in f["print_sci"].as_array().unwrap() {
        let x = hex(&case["x"]);
        assert_eq!(
            case["s"],
            RandomDistribution::Exponential(x).parameters_text(),
            "scientific {x:e}"
        );
        n += 1;
    }
    assert!(n > 20, "{n} printing records");
}

// --- former parser formatter (src/param.rs), verbatim bodies ------

fn legacy_parameters_text(d: &RandomDistribution) -> String {
    let r = |x: f64| stream::format_scientific(x, 17);
    let normal = |mean: f64, sd: f64| format!("{} {} 0", r(mean), r(sd));
    let gamma = |alpha: f64, beta: f64| format!("{} {} {}", r(alpha), r(beta), normal(0.0, 1.0));
    match *d {
        RandomDistribution::Uniform(a, b)
        | RandomDistribution::ExtremeValue(a, b)
        | RandomDistribution::Cauchy(a, b) => {
            format!("{} {}", r(a), r(b))
        }
        RandomDistribution::Exponential(a) => r(a),
        RandomDistribution::Weibull(a, b) => format!("{} {}", r(b), r(a)),
        RandomDistribution::Normal(a, b) => normal(a, b),
        RandomDistribution::Gamma(a, b) => gamma(a, b),
        RandomDistribution::Lognormal(a, b) => {
            format!("{} {} {}", r(a), r(b), normal(0.0, 1.0))
        }
        RandomDistribution::ChiSquared(a) => format!("{} {}", r(a), gamma(a / 2.0, 1.0)),
        RandomDistribution::FisherF(a, b) => format!(
            "{} {} {} {}",
            r(a),
            r(b),
            gamma(a / 2.0, 1.0),
            gamma(b / 2.0, 1.0)
        ),
        RandomDistribution::StudentT(a) => {
            format!("{} {} {}", r(a), normal(0.0, 1.0), gamma(a / 2.0, 2.0))
        }
    }
}

fn legacy_display(p: &RandomParameter) -> String {
    let plain = RandomDistribution::Uniform(0.0, 0.0);
    let d = p.distribution.as_ref().unwrap_or(&plain);
    format!(
        "{} + {} [ {} ]",
        stream::format_double(p.base_value),
        d.type_name(),
        legacy_parameters_text(d)
    )
}

/// splitmix64 (deterministic pseudo-random bit patterns).
fn splitmix64(state: &mut u64) -> u64 {
    *state = state.wrapping_add(0x9e37_79b9_7f4a_7c15);
    let mut z = *state;
    z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
    z ^ (z >> 31)
}

fn sweep_values() -> Vec<f64> {
    let mut v = vec![
        0.0,
        -0.0,
        1.0,
        -1.0,
        0.5,
        2.0,
        3.0,
        5.0,
        0.1,
        0.3,
        1e-7,
        3e7,
        3e8,
        94000.0,
        1e15,
        1e16,
        1e17,
        123_456.789,
        f64::MAX,
        -f64::MAX,
        f64::MIN_POSITIVE,
        f64::MIN_POSITIVE / 2.0,
        f64::from_bits(1),
        f64::EPSILON,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::NAN,
        -f64::NAN,
        f64::from_bits(0x7ff0_0000_0000_0001), // signalling NaN pattern
        f64::from_bits(1e23_f64.to_bits() - 1), // just below 1e23 (rounding edge)
        5e-324,
        1.797_693_134_862_315_7e308,
    ];
    let mut s = 0x5eed_u64;
    for _ in 0..1500 {
        v.push(f64::from_bits(splitmix64(&mut s)));
    }
    for _ in 0..500 {
        // moderate magnitudes, the usual material-parameter range
        let u = splitmix64(&mut s) >> 11;
        #[allow(clippy::cast_precision_loss)] // 53-bit integer → exact f64
        let x = (u as f64) * (1.0 / 9_007_199_254_740_992.0) * 1e9 - 5e8;
        v.push(x);
    }
    v
}

/// The delegated printing equals the former parser formatter.
#[test]
fn delegated_printing_equals_the_former_parser_formatter() {
    let values = sweep_values();
    let mut compared = 0usize;
    let mut mismatches = Vec::new();
    let mut check = |d: RandomDistribution, base: f64| {
        let p = RandomParameter {
            base_value: base,
            distribution: Some(d.clone()),
        };
        let (old_t, new_t) = (legacy_parameters_text(&d), d.parameters_text());
        let (old_p, new_p) = (legacy_display(&p), p.to_string());
        if old_t != new_t || old_p != new_p {
            mismatches.push(format!("{d:?} base {base:e}: {old_p:?} vs {new_p:?}"));
        }
        compared += 1;
    };
    for (i, &a) in values.iter().enumerate() {
        let b = values[(i * 7 + 3) % values.len()];
        let base = values[(i * 13 + 5) % values.len()];
        for d in [
            RandomDistribution::Uniform(a, b),
            RandomDistribution::Exponential(a),
            RandomDistribution::Gamma(a, b),
            RandomDistribution::Weibull(a, b),
            RandomDistribution::ExtremeValue(a, b),
            RandomDistribution::Normal(a, b),
            RandomDistribution::Lognormal(a, b),
            RandomDistribution::ChiSquared(a),
            RandomDistribution::Cauchy(a, b),
            RandomDistribution::FisherF(a, b),
            RandomDistribution::StudentT(a),
        ] {
            check(d, base);
        }
    }
    for &base in &values {
        let p = RandomParameter {
            base_value: base,
            distribution: None,
        };
        if legacy_display(&p) != p.to_string() {
            mismatches.push(format!("plain base {base:e}"));
        }
        compared += 1;
    }
    assert!(compared > 20_000, "{compared} comparisons");
    assert!(
        mismatches.is_empty(),
        "{} mismatches:\n{}",
        mismatches.len(),
        mismatches[..mismatches.len().min(40)].join("\n")
    );
}
