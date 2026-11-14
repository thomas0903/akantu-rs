// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: robustness of the user-input parsers of the
// traction-at-split-node contact package (src/common/parameter_reader.cc,
// src/common/synchronized_array.cc restart text, src/common/manual_restart.cc,
// the friction factory names of src/ntn_contact/ntn_initiation_function.cc).
//
// AGENTS, "Rules for numerical code": no panic on user-controllable input. Regression
// (a fuzz harness found 148 distinct panicking inputs, all
// in ParameterReader, e.g. `real x é5`, `int y €=3`, `bool - ü`), plus
// property tests over arbitrary bytes.

use std::panic::{AssertUnwindSafe, catch_unwind};

use akantu_core::Array;
use akantu_ntn_contact::factory::regularisation_from_option;
use akantu_ntn_contact::friction_law_from_name;
use akantu_ntn_contact::manual_restart::load_array_text;
use akantu_ntn_contact::parameter_reader::ParameterReader;
use akantu_ntn_contact::sync_array::SynchronizedArray;
use proptest::prelude::*;

/// The xorshift generator of the fuzz harness (its seed and token set).
struct Rng(u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        self.0
    }

    fn pick<'a>(&mut self, v: &[&'a str]) -> &'a str {
        let n = u64::try_from(v.len()).unwrap();
        v[usize::try_from(self.next() % n).unwrap()]
    }
}

const TOKENS: &[&str] = &[
    "real",
    "int",
    "uint",
    "bool",
    "string",
    "elementtype",
    "x",
    "=",
    "=3",
    "3",
    "-1",
    "1e400",
    "nan",
    "inf",
    "-",
    "é",
    "é5",
    "€=",
    "x=é",
    "\u{fffd}",
    "\t",
    " ",
    "#",
    "true",
    "TRUE",
    "_quadrangle_4",
    "0x1p3",
    "1.5e-3",
    "",
    "\r",
    "ü",
    "=\u{00e9}",
];

fn read_and_write(text: &[u8]) {
    let mut r = ParameterReader::new();
    // an error (invalid UTF-8 in a stored keyword or value) is fine; a
    // panic is not
    let _ = r.read_input_bytes(text);
    let _ = r.write_input_string();
}

/// The fuzz inputs: targeted lines and the 20,000 random token
/// lines of the harness (same seed), every one of them read and
/// written back without a panic.
#[test]
fn parameter_reader_review_v72_fuzz_inputs_do_not_panic() {
    let mut lines: Vec<String> = [
        "real x é5",
        "real x é",
        "int y €=3",
        "real x \u{fffd}5",
        "real x",
        "real",
        "bool b = maybe",
        "uint u = -1",
        "real z =",
        "string s = a b",
        "bool - ü",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
    for _ in 0..20000 {
        let n = rng.next() % 6;
        let mut line = String::new();
        for i in 0..n {
            if i > 0 && !rng.next().is_multiple_of(3) {
                line.push(' ');
            }
            line.push_str(rng.pick(TOKENS));
        }
        lines.push(line);
    }
    let mut panics: Vec<&str> = lines
        .iter()
        .filter(|l| catch_unwind(AssertUnwindSafe(|| read_and_write(l.as_bytes()))).is_err())
        .map(String::as_str)
        .collect();
    panics.sort_unstable();
    panics.dedup();
    assert!(
        panics.is_empty(),
        "{} distinct panicking inputs, e.g. {:?}",
        panics.len(),
        &panics[..panics.len().min(5)]
    );
}

/// Byte semantics of upstream's `std::string` tokenisation: a third token
/// starting with a multi-byte character is a convention error (its first
/// byte is not `=`), the line is ignored; an accepted line keeps its UTF-8
/// value; a stored keyword or value that is not UTF-8 is an error naming
/// the line.
#[test]
fn parameter_reader_tokenises_bytes() {
    let mut r = ParameterReader::new();
    r.read_input_bytes("real x é5\nint y €=3\nstring s =é\nreal ok = 2\n".as_bytes())
        .unwrap();
    assert!(!r.has_real("x"));
    assert!(!r.has_int("y"));
    assert_eq!(r.get_string("s").unwrap(), "é");
    assert_eq!(r.get_real("ok").unwrap(), 2.0);
    // invalid UTF-8 in a value (line 2) and a keyword (line 3)
    let mut r = ParameterReader::new();
    let e = r
        .read_input_bytes(b"real a = 1\nstring s = \xff\xfe\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("line 2"), "{e}");
    let mut r = ParameterReader::new();
    let e = r
        .read_input_bytes(b"\n\nreal k\xe9y = 1\n")
        .unwrap_err()
        .to_string();
    assert!(e.contains("line 3"), "{e}");
    // invalid bytes on an ignored line are not an error (upstream ignores
    // the line)
    let mut r = ParameterReader::new();
    r.read_input_bytes(b"real x \xc3\x00\nfoo \xff = 1\n# \xff\n")
        .unwrap();
    // a file with invalid UTF-8: the error names the file and the line
    let dir = std::env::temp_dir().join(format!("ntn-param-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let path = dir.join("bad.txt");
    std::fs::write(&path, b"real a = 1\n\nstring s = a\xffb\n").unwrap();
    let e = ParameterReader::new()
        .read_input_file(&path)
        .unwrap_err()
        .to_string();
    assert!(e.contains("line 3") && e.contains("bad.txt"), "{e}");
    std::fs::remove_dir_all(&dir).ok();
}

fn restart_parsers(text: &[u8]) {
    let mut a = SynchronizedArray::<f64>::with_size(2, 1, 0.0, "a", 0.0, "a").unwrap();
    let _ = a.read_restart_text(text, false);
    let _ = a.read_restart_text(text, true);
    let mut b = SynchronizedArray::<bool>::with_size(2, 1, false, "b", false, "b").unwrap();
    let _ = b.read_restart_text(text, false);
    let mut m = Array::<f64>::zeros(2, 1);
    let _ = load_array_text(&mut m, text);
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    /// Arbitrary bytes never panic the parameter reader.
    #[test]
    fn parameter_reader_arbitrary_bytes(text in proptest::collection::vec(any::<u8>(), 0..256)) {
        read_and_write(&text);
    }

    /// Lines built from grammar-like tokens and arbitrary bytes.
    #[test]
    fn parameter_reader_token_soup(
        parts in proptest::collection::vec(
            prop_oneof![
                Just(b"real".to_vec()), Just(b"int".to_vec()), Just(b"uint".to_vec()),
                Just(b"bool".to_vec()), Just(b"string".to_vec()), Just(b"elementtype".to_vec()),
                Just(b"=".to_vec()), Just(b" ".to_vec()), Just(b"\n".to_vec()),
                Just(b"#".to_vec()), Just(b"\r".to_vec()), Just("é".as_bytes().to_vec()),
                proptest::collection::vec(any::<u8>(), 0..4),
            ],
            0..24,
        )
    ) {
        read_and_write(&parts.concat());
    }

    /// Arbitrary bytes never panic the restart-text parsers (Real and bool
    /// synchronized arrays, both modes) or the manual-restart loader.
    #[test]
    fn restart_parsers_arbitrary_bytes(text in proptest::collection::vec(any::<u8>(), 0..128)) {
        restart_parsers(&text);
    }

    /// Restart-like texts (numbers, signs, exponents, separators).
    #[test]
    fn restart_parsers_number_soup(
        parts in proptest::collection::vec(
            prop_oneof![
                Just("2".to_owned()), Just("1".to_owned()), Just("0".to_owned()),
                Just("-1".to_owned()), Just("nan".to_owned()), Just("1e999".to_owned()),
                Just("0.5".to_owned()), Just(" ".to_owned()), Just("\n".to_owned()),
                Just("1e".to_owned()), Just("18446744073709551616".to_owned()),
                Just("-".to_owned()), "[0-9.eE+-]{0,6}",
            ],
            0..16,
        )
    ) {
        restart_parsers(parts.concat().as_bytes());
    }

    /// Arbitrary factory names and regularisation options never panic.
    #[test]
    fn factory_names_arbitrary(name in any::<String>(), option in any::<String>()) {
        let _ = friction_law_from_name(&name);
        let _ = friction_law_from_name(&format!("rate_and_state:{name}"));
        let _ = friction_law_from_name(&format!("rate_and_state_n_shaped_{name}"));
        let _ = regularisation_from_option(&option, false);
        let _ = regularisation_from_option(&option, true);
    }
}
