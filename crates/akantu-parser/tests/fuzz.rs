// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs robustness tests: malformed input must never panic or
// overflow the stack.

//! Property-based "fuzz" tests: mutated upstream input files, random byte
//! strings, random token soups as values of every kind, and pathological
//! nesting. Every outcome must be `Ok` or `Err` — never a panic.

mod common;

use akantu_parser::{ParamKind, ParamValue, Parser, ParserSection, ParserType, SearchScope};
use proptest::prelude::*;

const KINDS: [ParamKind; 13] = [
    ParamKind::Real,
    ParamKind::Int,
    ParamKind::UInt,
    ParamKind::Bool,
    ParamKind::String,
    ParamKind::VectorReal,
    ParamKind::MatrixReal,
    ParamKind::Random,
    ParamKind::Enum,
    ParamKind::StringList,
    ParamKind::StringSet,
    ParamKind::ByteString,
    ParamKind::ByteStringSet,
];

fn convert_all(s: &ParserSection) {
    for q in &s.parameters {
        let r = s
            .get_parameter(&q.name, SearchScope::Current)
            .expect("parameter exists");
        for k in KINDS {
            let _ = r.to_value(k);
        }
        // The upstream `std::string` kinds accept every value:
        // the raw bytes, unchanged.
        assert_eq!(
            r.to_value(ParamKind::ByteString).expect("never fails"),
            ParamValue::ByteString(q.value.clone())
        );
        assert!(r.to_value(ParamKind::ByteStringSet).is_ok());
        let _ = r.parameter().to_string();
    }
    let _ = s.to_string();
    for c in &s.sub_sections {
        convert_all(c);
    }
}

fn parse_and_convert(bytes: &[u8]) {
    if let Ok(p) = Parser::parse_bytes(bytes, "fuzz.dat", akantu_parser::AnonymousIds::Global) {
        convert_all(&p.root);
        let _ = p.to_string();
    }
}

/// Characters that are significant to the grammars.
const ALPHABET: &[u8] =
    b" \t\n\r#;=\\[](),.+-*/eEpi0123456789_:abcxyzMATERIALmaterialuser\"'\x00\xff";

fn fixtures() -> Vec<Vec<u8>> {
    common::upstream_inputs()
        .iter()
        .map(|r| std::fs::read(common::fixtures_dir().join(r)).unwrap())
        .collect()
}

#[derive(Debug, Clone)]
enum Edit {
    Replace(usize, u8),
    Insert(usize, u8),
    Delete(usize),
    Truncate(usize),
    Duplicate(usize, usize),
}

fn edit() -> impl Strategy<Value = Edit> {
    let byte = prop_oneof![
        3 => proptest::sample::select(ALPHABET.to_vec()),
        1 => any::<u8>(),
    ];
    prop_oneof![
        (any::<usize>(), byte.clone()).prop_map(|(i, b)| Edit::Replace(i, b)),
        (any::<usize>(), byte).prop_map(|(i, b)| Edit::Insert(i, b)),
        any::<usize>().prop_map(Edit::Delete),
        any::<usize>().prop_map(Edit::Truncate),
        (any::<usize>(), 1usize..64).prop_map(|(i, n)| Edit::Duplicate(i, n)),
    ]
}

fn apply(mut v: Vec<u8>, edits: &[Edit]) -> Vec<u8> {
    for e in edits {
        let n = v.len().max(1);
        match *e {
            Edit::Replace(i, b) => {
                if !v.is_empty() {
                    let i = i % v.len();
                    v[i] = b;
                }
            }
            Edit::Insert(i, b) => v.insert(i % (v.len() + 1), b),
            Edit::Delete(i) => {
                if !v.is_empty() {
                    v.remove(i % v.len());
                }
            }
            Edit::Truncate(i) => v.truncate(i % n),
            Edit::Duplicate(i, len) => {
                if !v.is_empty() {
                    let i = i % v.len();
                    let end = (i + len).min(v.len());
                    let chunk = v[i..end].to_vec();
                    v.splice(i..i, chunk);
                }
            }
        }
    }
    v
}

const TOKENS: &[&str] = &[
    "1",
    "0",
    "2.5",
    ".5",
    "1e5",
    "1e400",
    "-",
    "+",
    "*",
    "/",
    "**",
    "(",
    ")",
    "[",
    "]",
    ",",
    " ",
    "\t",
    "pi",
    "e",
    "E",
    "x",
    "y",
    "nan",
    "inf",
    "nan(",
    "sqrt",
    "pow",
    "atan2",
    "cos",
    "COS",
    "exp",
    "uniform",
    "weibull",
    "normal",
    "student_t",
    "v",
    "m",
    "true",
    "false",
    "_",
    "007",
    "99999999999999999999",
    "0.",
    "e+",
    "1.2.3",
];

fn token_soup() -> impl Strategy<Value = String> {
    proptest::collection::vec(proptest::sample::select(TOKENS.to_vec()), 0..24)
        .prop_map(|t| t.concat())
}

fn scope() -> ParserSection {
    let p = Parser::parse_str(
        "x = 2\ny = x * 3\nv = [1,2]\nm = [[1, 2], [3, 4]]\ncyc = cyc\nmaterial a [\n z = y + x\n]\n",
        "scope.dat",
    )
    .unwrap();
    p.root.sub_sections[0].clone()
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn mutated_upstream_inputs_never_panic(
        idx in any::<usize>(),
        edits in proptest::collection::vec(edit(), 1..12),
    ) {
        let files = fixtures();
        let base = files[idx % files.len()].clone();
        parse_and_convert(&apply(base, &edits));
    }

    #[test]
    fn random_bytes_never_panic(bytes in proptest::collection::vec(
        prop_oneof![3 => proptest::sample::select(ALPHABET.to_vec()), 1 => any::<u8>()], 0..400)) {
        parse_and_convert(&bytes);
    }

    #[test]
    fn random_values_never_panic(text in token_soup()) {
        let s = scope();
        for k in KINDS {
            let _ = ParamValue::parse(k, &text, &s);
        }
        let doc = format!("q = {text}\nmaterial b [\n w = {text}\n]\n");
        parse_and_convert(doc.as_bytes());
    }
}

/// Deep and pathological nesting never crashes (there is no
/// depth limit, finite acyclic input is evaluated, unbalanced or cyclic
/// input is an error).
#[test]
fn pathological_nesting_is_an_error_not_a_crash() {
    let s = scope();
    for text in [
        format!("{}1{}", "(".repeat(100_000), ")".repeat(100_000)),
        format!("{}1", "-".repeat(100_000)),
        format!("{}1", "+-".repeat(50_000)),
        format!("[{}1{}]", "(".repeat(10_000), ")".repeat(10_000)),
        format!("[{}]", "[".repeat(10_000)),
        format!("{}1", "sqrt(".repeat(10_000)),
        "cyc".to_string(),
    ] {
        for k in [
            ParamKind::Real,
            ParamKind::VectorReal,
            ParamKind::MatrixReal,
            ParamKind::Random,
        ] {
            let _ = ParamValue::parse(k, &text, &s);
        }
    }
    // Balanced nesting is evaluated at any depth.
    for depth in [100, 100_000] {
        let v = ParamValue::parse(
            ParamKind::Real,
            &format!("{}1{}", "(".repeat(depth), ")".repeat(depth)),
            &s,
        )
        .unwrap();
        assert_eq!(v, ParamValue::Real(1.0));
    }
    // Deeply nested sections: unclosed is an error, closed is accepted.
    let deep = "user [\n".repeat(100_000);
    assert!(Parser::parse_str(&deep, "deep.dat").is_err());
    let closed = format!("{deep}{}", "]\n".repeat(100_000));
    assert!(Parser::parse_str(&closed, "deep.dat").is_ok());
    let ok = format!("{}{}", "user a [\n".repeat(100), "]\n".repeat(100));
    let p = Parser::parse_str(&ok, "deep.dat").unwrap();
    assert_eq!(p.root.sub_sections[0].section_type, ParserType::User);
    // Cyclic variables: an error (upstream overflows the stack).
    let e = s
        .get_parameter("cyc", SearchScope::CurrentAndParent)
        .unwrap();
    let err = e.try_get::<f64>().unwrap_err();
    assert!(err.is_hard() && err.is_cyclic_reference(), "{err:?}");
}

#[test]
fn huge_inputs_are_handled() {
    // 1 MB value, 10^5 parameters.
    let big = format!("a = {}\n", "1+".repeat(500_000) + "1");
    let p = Parser::parse_str(&big, "big.dat").unwrap();
    let v: f64 = p
        .root
        .get_parameter_value("a", SearchScope::Current)
        .unwrap();
    assert_eq!(v, 500_001.0);
    let many: String = (0..100_000).map(|i| format!("p{i} = {i}\n")).collect();
    let p = Parser::parse_str(&many, "many.dat").unwrap();
    assert_eq!(p.root.parameters.len(), 100_000);
}

const ARGS: &[&str] = &[
    "--aka_input_file",
    "--aka_debug_level",
    "--aka_print_backtrace",
    "--aka_seed",
    "-h",
    "--help",
    "x.dat",
    "3",
    "-3",
    "1.5",
    "abc",
    "",
    " ",
    "--unknown",
    "-n",
    "files",
    "0",
    "1",
];

proptest! {
    #![proptest_config(ProptestConfig::with_cases(512))]

    #[test]
    fn command_lines_never_panic(
        argv in proptest::collection::vec(proptest::sample::select(ARGS.to_vec()), 0..12),
        stop in any::<bool>(),
        remove in any::<bool>(),
        help in any::<bool>(),
    ) {
        use akantu_parser::cli::*;
        let mut p = ArgumentParser::default();
        register_akantu_options(&mut p).unwrap();
        p.add_argument(ArgumentSpec::new("-n;--number", "n").arg_type(ArgumentType::Float)).unwrap();
        p.add_argument(ArgumentSpec::new("files", "f").nargs(Nargs::Any)).unwrap();
        let mut v: Vec<String> = std::iter::once("prog".to_string())
            .chain(argv.iter().map(|s| (*s).to_string()))
            .collect();
        let flags = ParseFlags { stop_on_not_parsed: stop, remove_parsed: remove };
        let _ = p.parse_argv(&mut v, flags, help);
        let _ = p.help_string();
        let _ = p.to_string();
        for n in ["aka_input_file", "aka_debug_level", "aka_seed", "number", "files", "help"] {
            let _ = p.value(n);
            let _ = p.get(n);
        }
    }
}
