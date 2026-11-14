// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential test: compares the Rust
// `cppargparse` port with the C++ reference `cppargparse` (installed oracle
// library), recorded by oracle/probes/parser/cli_probe.cc + cli_cases.py in
// tests/fixtures/parser/cli_blackbox.jsonl (regenerate with
// oracle/probes/parser/run.sh).

//! For every (argument set, parse flags, command line) case: upstream either
//! exits with status 1 after printing a message and the help text (port:
//! `Err(InvalidArgument(message))`, and `help_string()` equals the printed
//! help), exits with status 0 after printing the help for `-h`/`--help`
//! (port: `Ok(HelpRequested)`), or returns; then the remaining `argv`
//! (`_remove_parsed`), `has(name)`, the single typed value of each argument
//! (doubles by bit pattern) and the `printself` text must be identical.

mod common;

use akantu_core::Error;
use akantu_parser::cli::{
    ArgValue, ArgumentParser, ArgumentSpec, ArgumentType, Nargs, ParseFlags, ParseOutcome,
    register_akantu_options,
};
use serde_json::Value;

fn unhex(s: &str) -> String {
    let b: Vec<u8> = (0..s.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&s[i..i + 2], 16).expect("hex"))
        .collect();
    String::from_utf8(b).expect("ASCII probe output")
}

/// The argument sets of `cli_probe.cc::setup`.
fn setup(scenario: &str) -> ArgumentParser {
    use ArgumentType::{Boolean, Float, Integer, String as Str};
    let mut p = ArgumentParser::default();
    let specs: Vec<ArgumentSpec> = match scenario {
        "float" => vec![ArgumentSpec::new("--val", "value").arg_type(Float)],
        "float_def" => vec![
            ArgumentSpec::new("--val", "value")
                .arg_type(Float)
                .default_value("2.5"),
            ArgumentSpec::new("--opt", "optional")
                .nargs(Nargs::OneIfPossible)
                .arg_type(Float)
                .default_value("1.5")
                .const_value("3.25"),
        ],
        "float_pos" => vec![
            ArgumentSpec::new("x", "first").arg_type(Float),
            ArgumentSpec::new("y", "second")
                .nargs(Nargs::OneIfPossible)
                .arg_type(Float),
        ],
        "int" => vec![
            ArgumentSpec::new("-n;--number", "number").arg_type(Integer),
            ArgumentSpec::new("--many", "many")
                .nargs(Nargs::Any)
                .arg_type(Integer),
        ],
        "bool" => vec![
            ArgumentSpec::new("--b", "boolean").arg_type(Boolean),
            ArgumentSpec::new("--flag", "flag")
                .nargs(Nargs::Exactly(0))
                .arg_type(Boolean)
                .default_value("false")
                .const_value("true"),
        ],
        "mixed" => vec![
            ArgumentSpec::new("files", "files")
                .nargs(Nargs::AtLeastOne)
                .arg_type(Str),
            ArgumentSpec::new("--val", "value").arg_type(Float),
            ArgumentSpec::new("-n", "number").arg_type(Integer),
            ArgumentSpec::new("--opt", "option")
                .nargs(Nargs::OneIfPossible)
                .default_value("d")
                .const_value("c"),
        ],
        "akantu" => {
            register_akantu_options(&mut p).unwrap();
            Vec::new()
        }
        other => panic!("unknown scenario {other}"),
    };
    for s in specs {
        p.add_argument(s).unwrap();
    }
    p
}

fn flags(f: u64) -> ParseFlags {
    ParseFlags {
        stop_on_not_parsed: f & 1 != 0,
        remove_parsed: f & 2 != 0,
    }
}

fn show(v: &ArgValue) -> String {
    match v {
        ArgValue::String(s) => s.bytes().map(|b| format!("{b:02x}")).collect(),
        ArgValue::Float(x) => format!("{:016x}", x.to_bits()),
        ArgValue::Integer(i) => i.to_string(),
        ArgValue::Boolean(b) => if *b { "1" } else { "0" }.to_string(),
    }
}

#[test]
fn cli_matches_cppargparse() {
    let path = common::fixtures_dir().join("parser/cli_blackbox.jsonl");
    let text = std::fs::read_to_string(path).unwrap();
    let mut helps = std::collections::HashMap::new();
    let mut errors = Vec::new();
    let mut cases = 0;
    for raw in text.lines() {
        let line: Value = serde_json::from_str(raw).unwrap();
        let scenario = line["scenario"].as_str().unwrap().to_string();
        if line["kind"] == "help" {
            helps.insert(scenario, line["text"].as_str().unwrap().to_string());
            continue;
        }
        cases += 1;
        let args: Vec<String> = line["args"]
            .as_array()
            .unwrap()
            .iter()
            .map(|a| a.as_str().unwrap().to_string())
            .collect();
        let f = line["flags"].as_u64().unwrap();
        let mut argv = vec!["prog".to_string()];
        argv.extend(args.iter().cloned());
        let mut p = setup(&scenario);
        let r = p.parse_argv(&mut argv, flags(f), true);
        let case = format!("{scenario} flags={f} {args:?}");
        let exit = line["exit"].as_i64().unwrap();
        if line["help"].as_bool().unwrap() && p.help_string() != helps[&scenario] {
            errors.push(format!(
                "{case}: help differs:\n{}\nvs\n{}",
                p.help_string(),
                helps[&scenario]
            ));
        }
        match (exit, &line["out"], r) {
            (1, _, Err(Error::InvalidArgument(m))) => {
                if line["msg"] != m.as_str() {
                    errors.push(format!("{case}: message {:?} vs {m:?}", line["msg"]));
                }
            }
            (0, Value::Null, Ok(ParseOutcome::HelpRequested)) => {}
            (0, out @ Value::Object(_), Ok(ParseOutcome::Parsed)) => {
                let expected_argv: Vec<String> = out["argv"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|a| unhex(a.as_str().unwrap()))
                    .collect();
                if expected_argv != argv {
                    errors.push(format!("{case}: argv {expected_argv:?} vs {argv:?}"));
                }
                for (name, o) in out["args"].as_object().unwrap() {
                    let ours = match p.values(name) {
                        Err(_) => Value::Null,
                        Ok([v]) => Value::from(vec![show(v)]),
                        Ok(_) => Value::from("many"),
                    };
                    if o != &ours || p.has(name) == o.is_null() {
                        errors.push(format!("{case}: {name} oracle {o}, port {ours}"));
                    }
                }
                let print = unhex(out["print"].as_str().unwrap());
                if print != p.to_string() {
                    errors.push(format!(
                        "{case}: printself {print:?} vs {:?}",
                        p.to_string()
                    ));
                }
            }
            (e, o, r) => errors.push(format!("{case}: oracle exit {e} out {o}, port {r:?}")),
        }
    }
    eprintln!("{cases} command lines compared");
    assert!(cases > 400);
    assert!(
        errors.is_empty(),
        "{} mismatches:\n{}",
        errors.len(),
        errors.join("\n")
    );
}
