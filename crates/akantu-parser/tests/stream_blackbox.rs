// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs clean-room evidence test (D-030): compares
// `akantu_parser::stream` (and `RandomDistribution::parameters_text`) with
// the behaviour of the C++ standard library of the oracle toolchain, observed
// black-box by oracle/probes/parser/stream_blackbox.cc and recorded in
// tests/fixtures/parser/stream_blackbox.jsonl (regenerate with
// oracle/probes/parser/run.sh).

//! Every recorded observation must be reproduced exactly: stored value (or
//! "not stored"), `failbit`, `eofbit`, `badbit` (never set) and the number of
//! consumed characters for each extraction; output text for each insertion
//! and distribution.

mod common;

use akantu_parser::RandomDistribution;
use akantu_parser::stream::{
    self, IntExtraction, StreamInt, extract_bool, extract_double, extract_int, extract_word,
};
use serde_json::Value;

/// `std::scientific` precisions recorded by the probe (`sci_prec`).
const SCI_PREC: [usize; 11] = [0, 1, 2, 3, 5, 6, 10, 16, 17, 20, 40];
/// Default-floatfield precisions recorded by the probe (`gen_prec`).
const GEN_PREC: [usize; 12] = [0, 1, 2, 3, 5, 6, 10, 15, 16, 17, 20, 40];

fn unhex(s: &str) -> Vec<u8> {
    (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect()
}

fn bits(s: &str) -> f64 {
    f64::from_bits(u64::from_str_radix(s, 16).expect("bits"))
}

/// Recorded extraction: `"<value or ->,<fail><eof><bad>,<consumed>"`.
struct Observed<'a> {
    value: Option<&'a str>,
    fail: bool,
    eof: bool,
    consumed: usize,
}

fn observed(s: &str) -> Observed<'_> {
    let mut it = s.split(',');
    let v = it.next().unwrap();
    let st = it.next().unwrap().as_bytes();
    let consumed = it.next().unwrap().parse().unwrap();
    assert_eq!(st[2], b'0', "badbit is never expected");
    Observed {
        value: (v != "-").then_some(v),
        fail: st[0] == b'1',
        eof: st[1] == b'1',
        consumed,
    }
}

fn same<T>(o: &Observed<'_>, e: &IntExtraction<T>, show: impl Fn(&T) -> String) -> bool {
    o.value.map(str::to_string) == e.value.as_ref().map(show)
        && o.fail == e.fail
        && o.eof == e.eof
        && o.consumed == e.consumed
}

fn check_int<T: StreamInt + ToString + std::fmt::Debug>(
    errors: &mut Vec<String>,
    input: &[u8],
    line: &Value,
    key: &str,
) {
    let o = observed(line[key].as_str().unwrap());
    let e = extract_int::<T>(input, 0);
    if !same(&o, &e, T::to_string) {
        errors.push(format!(
            "{key} {input:?}: oracle {}, port {e:?}",
            line[key].as_str().unwrap()
        ));
    }
}

#[test]
fn stream_conversions_match_oracle_observations() {
    let path = common::fixtures_dir().join("parser/stream_blackbox.jsonl");
    let text = std::fs::read_to_string(&path).unwrap();
    let mut errors = Vec::new();
    let (mut inputs, mut formats, mut dists) = (0, 0, 0);
    for raw in text.lines() {
        let line: Value = serde_json::from_str(raw).unwrap();
        match line["k"].as_str().unwrap() {
            "in" => {
                inputs += 1;
                let input = unhex(line["in"].as_str().unwrap());
                let input = input.as_slice();
                check_int::<i16>(&mut errors, input, &line, "short");
                check_int::<i32>(&mut errors, input, &line, "int");
                check_int::<i64>(&mut errors, input, &line, "long");
                check_int::<i64>(&mut errors, input, &line, "llong");
                check_int::<u16>(&mut errors, input, &line, "ushort");
                check_int::<u32>(&mut errors, input, &line, "uint");
                check_int::<u64>(&mut errors, input, &line, "ulong");
                check_int::<u64>(&mut errors, input, &line, "ullong");
                check_int::<i8>(&mut errors, input, &line, "schar");
                check_int::<u8>(&mut errors, input, &line, "uchar");
                let o = observed(line["double"].as_str().unwrap());
                let e = extract_double(input, 0);
                let ok = o.value.map(bits).map(f64::to_bits) == e.value.map(f64::to_bits)
                    && o.fail == e.fail
                    && o.eof == e.eof
                    && o.consumed == e.consumed;
                if !ok {
                    errors.push(format!(
                        "double {input:?}: oracle {}, port {e:?}",
                        line["double"]
                    ));
                }
                for (key, alpha) in [("bool_alpha", true), ("bool_num", false)] {
                    let o = observed(line[key].as_str().unwrap());
                    let e = extract_bool(input, 0, alpha);
                    if !same(&o, &e, |b| if *b { "1" } else { "0" }.to_string()) {
                        errors.push(format!("{key} {input:?}: oracle {}, port {e:?}", line[key]));
                    }
                }
                // Akantu's two-step conversion; "-" (target never written,
                // upstream reads an uninitialised bool) is `false` in the
                // port (category A).
                let expected = line["akantu_bool"].as_str().unwrap() == "1";
                if stream::parse_bool(input) != expected {
                    errors.push(format!(
                        "akantu_bool {input:?}: oracle {}",
                        line["akantu_bool"]
                    ));
                }
                let w = line["word"].as_str().unwrap();
                let (word_hex, state) = w.split_once(',').unwrap();
                let word = unhex(word_hex);
                let state = format!("-,{state}");
                let o = observed(&state);
                let (ours, fail, eof, consumed) = match extract_word(input, 0) {
                    Some(x) => {
                        let end = x.as_ptr() as usize - input.as_ptr() as usize + x.len();
                        (x.to_vec(), false, end == input.len(), end)
                    }
                    None => (b"SENTINEL".to_vec(), true, true, input.len()),
                };
                if ours != word || o.fail != fail || o.eof != eof || o.consumed != consumed {
                    errors.push(format!("word {input:?}: oracle {w}, port {ours:?}"));
                }
                let list: Vec<Vec<u8>> = match line["list"].as_str().unwrap() {
                    "" => Vec::new(),
                    l => l.split('.').map(unhex).collect(),
                };
                if stream::string_list_bytes(input) != list {
                    errors.push(format!("list {input:?}: oracle {list:?}"));
                }
            }
            "fmt" => {
                formats += 1;
                let x = bits(line["x"].as_str().unwrap());
                if stream::format_double(x) != line["def"].as_str().unwrap() {
                    errors.push(format!("default {x:e}: oracle {}", line["def"]));
                }
                for (p, s) in SCI_PREC.iter().zip(line["sci"].as_array().unwrap()) {
                    let ours = stream::format_scientific(x, *p);
                    if ours != s.as_str().unwrap() {
                        errors.push(format!("scientific({p}) {x:e}: oracle {s}, port {ours}"));
                    }
                }
                for (p, s) in GEN_PREC.iter().zip(line["gen"].as_array().unwrap()) {
                    let ours = stream::format_general(x, *p);
                    if ours != s.as_str().unwrap() {
                        errors.push(format!("general({p}) {x:e}: oracle {s}, port {ours}"));
                    }
                }
            }
            "dist" => {
                dists += 1;
                let a = bits(line["a"].as_str().unwrap());
                let b = bits(line["b"].as_str().unwrap());
                let d = match line["d"].as_str().unwrap() {
                    "uniform" => RandomDistribution::Uniform(a, b),
                    "exponential" => RandomDistribution::Exponential(a),
                    "gamma" => RandomDistribution::Gamma(a, b),
                    // recorded as std::weibull_distribution(b, a), like
                    // upstream make_random_parameter
                    "weibull" => RandomDistribution::Weibull(a, b),
                    "extreme_value" => RandomDistribution::ExtremeValue(a, b),
                    "normal" => RandomDistribution::Normal(a, b),
                    "lognormal" => RandomDistribution::Lognormal(a, b),
                    "chi_squared" => RandomDistribution::ChiSquared(a),
                    "cauchy" => RandomDistribution::Cauchy(a, b),
                    "fisher_f" => RandomDistribution::FisherF(a, b),
                    "student_t" => RandomDistribution::StudentT(a),
                    other => panic!("unknown distribution {other}"),
                };
                let ours = d.parameters_text();
                if ours != line["t"].as_str().unwrap() {
                    errors.push(format!("{d:?}: oracle {}, port {ours}", line["t"]));
                }
            }
            other => panic!("unknown record {other}"),
        }
    }
    eprintln!("{inputs} inputs, {formats} doubles, {dists} distributions compared");
    assert!(inputs > 1000 && formats > 500 && dists > 200);
    assert!(
        errors.is_empty(),
        "{} mismatches:\n{}",
        errors.len(),
        errors[..errors.len().min(60)].join("\n")
    );
}
