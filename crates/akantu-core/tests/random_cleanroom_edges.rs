// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): bitwise checks of the
// distributions behind src/common/aka_random_generator.hh in boundary cases,
// against the black-box fixture cleanroom_edges.json produced by
// oracle/probes/random/cleanroom/edges.cc on the oracle toolchain
// (GCC 14.2 / libstdc++ 14 / glibc 2.41, Debian trixie, aarch64,
// -O2 -ffp-contract=off).

//! Boundary cases that seeded engines practically never reach, replayed with
//! a scripted generator: the `generate_canonical` clamp, polar-method
//! rejections (`r2 == 0`, `r2 > 1`) and acceptance of `r2 == 1` (with the
//! sign of the resulting zero through nested distributions), Cauchy
//! `u == 0.5`, gamma `u == 0`, `v <= 0` and `log(u) ==` threshold, signed
//! zeros and infinities at `u == 0`, `reset()` and copies with cached state,
//! parameters outside the preconditions (NaN-sensitive loops and branches;
//! NaN results are compared by NaN-ness only) and `operator<<` of special
//! parameter values.

use akantu_core::random::distributions::*;
use akantu_core::random::{
    Distribution, MinstdRand0, UniformRandomBitGenerator, generate_canonical,
};
use serde_json::Value;

/// Returns the scripted values, then the outputs of `minstd_rand0` seeded 1
/// (same as the probe's `Scripted` generator).
struct Scripted {
    script: Vec<u64>,
    pos: usize,
    fallback: MinstdRand0,
    calls: u64,
}

impl Scripted {
    fn new(script: Vec<u64>) -> Self {
        Self {
            script,
            pos: 0,
            fallback: MinstdRand0::with_seed(1),
            calls: 0,
        }
    }
}

impl UniformRandomBitGenerator for Scripted {
    const MIN: u64 = 1;
    const MAX: u64 = 2_147_483_646;

    fn next_value(&mut self) -> u64 {
        self.calls += 1;
        if let Some(&v) = self.script.get(self.pos) {
            self.pos += 1;
            v
        } else {
            u64::from(self.fallback.next_u32())
        }
    }
}

fn fixture() -> Value {
    let path = format!(
        "{}/tests/fixtures/random/cleanroom_edges.json",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn hex(v: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(v.as_str().expect("hex string"), 16).expect("hex"))
}

fn distribution(name: &str, p: &[f64]) -> Distribution {
    match name {
        "uniform_real" => Distribution::Uniform(UniformRealDistribution::new(p[0], p[1])),
        "exponential" => Distribution::Exponential(ExponentialDistribution::new(p[0])),
        "gamma" => Distribution::Gamma(GammaDistribution::new(p[0], p[1])),
        "weibull" => Distribution::Weibull(WeibullDistribution::new(p[0], p[1])),
        "extreme_value" => Distribution::ExtremeValue(ExtremeValueDistribution::new(p[0], p[1])),
        "normal" => Distribution::Normal(NormalDistribution::new(p[0], p[1])),
        "lognormal" => Distribution::Lognormal(LognormalDistribution::new(p[0], p[1])),
        "chi_squared" => Distribution::ChiSquared(ChiSquaredDistribution::new(p[0])),
        "cauchy" => Distribution::Cauchy(CauchyDistribution::new(p[0], p[1])),
        "fisher_f" => Distribution::FisherF(FisherFDistribution::new(p[0], p[1])),
        "student_t" => Distribution::StudentT(StudentTDistribution::new(p[0])),
        other => panic!("unknown distribution {other}"),
    }
}

#[test]
fn edge_fixture_provenance() {
    let meta = &fixture()["meta"];
    assert_eq!(meta["arch"], "aarch64");
    assert_eq!(meta["glibc"], "2.41");
    assert_eq!(meta["glibcxx_release"], 14);
    assert!(
        meta["flags"]
            .as_str()
            .unwrap()
            .contains("-ffp-contract=off")
    );
}

#[test]
fn edge_cases_match_oracle() {
    let f = fixture();
    let cases = f["cases"].as_array().unwrap();
    assert!(cases.len() >= 70);
    for case in cases {
        let name = case["dist"].as_str().unwrap();
        let note = case["name"].as_str().unwrap();
        let params: Vec<f64> = case["params"].as_array().unwrap().iter().map(hex).collect();
        let script: Vec<u64> = case["script"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        let mut g = Scripted::new(script);
        let mut d = (name != "canonical").then(|| distribution(name, &params));
        let (mut values, mut calls, mut printed) = (vec![], vec![], vec![]);
        for op in case["ops"].as_str().unwrap().chars() {
            match (op, d.as_mut()) {
                ('d', None) => {
                    values.push(generate_canonical(&mut g).to_bits());
                    calls.push(g.calls);
                }
                ('d', Some(d)) => {
                    values.push(d.sample(&mut g).to_bits());
                    calls.push(g.calls);
                }
                ('r', Some(d)) => d.reset(),
                ('p', Some(d)) => printed.push(d.params_string()),
                ('c', Some(d)) => {
                    let copy = d.clone();
                    *d = copy;
                }
                (other, _) => panic!("{note}: unexpected op {other}"),
            }
        }
        let context = format!("{name}{params:?}: {note}");
        let expected: Vec<u64> = case["values"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| hex(v).to_bits())
            .collect();
        assert_eq!(values.len(), expected.len(), "{context}");
        for (i, (got, exp)) in values.iter().zip(&expected).enumerate() {
            // NaN payloads and signs are not significant (any NaN matches):
            // the documented D-046 exception for the libm special-value
            // tables (NUMERICAL_VALIDATION "Default-NaN convention
            // (D-046)"); non-NaN values stay bitwise.
            let both_nan = f64::from_bits(*got).is_nan() && f64::from_bits(*exp).is_nan();
            assert!(
                both_nan || got == exp,
                "{context}: value {i}: got {:e}, expected {:e}",
                f64::from_bits(*got),
                f64::from_bits(*exp)
            );
        }
        let expected_calls: Vec<u64> = case["calls"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_u64().unwrap())
            .collect();
        assert_eq!(calls, expected_calls, "{context}: engine calls");
        let expected_printed: Vec<&str> = case["printed"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| v.as_str().unwrap())
            .collect();
        assert_eq!(printed, expected_printed, "{context}: operator<<");
    }
}
