// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests: input
// files are byte strings; names and values keep their exact bytes, also
// through the parameter registry (oracle evidence:
// tests/fixtures/extra/bytes.dat compared by tests/oracle.rs, and
// tests/fixtures/parser/registry_bytes.json from
// oracle/probes/parser/registry_probe.cc compared here).

mod common;

use std::collections::BTreeSet;

use akantu_core::Result;
use akantu_parser::param::{
    ParamInfo, table_get, table_info, table_names, table_set, table_set_from_input,
};
use akantu_parser::{
    ByteString, ParamDescriptor, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, ParserType, SearchScope,
};
use serde_json::Value;

#[test]
fn values_keep_their_bytes() {
    let p = akantu_parser::Parser::parse_bytes(
        b"q = abc\xff\nE # caf\xe9\n = 2\nl = [a\xff, b\xa0c]\ns = \xa0 5\n",
        "bytes.dat",
        akantu_parser::AnonymousIds::Local(&mut 12),
    )
    .unwrap();
    let root = &p.root;
    // The bytes 61 62 63 ff (not UTF-8), also through the
    // `std::string` conversion and the registry's `std::string` kind.
    let q = root.get_parameter("q", SearchScope::Current).unwrap();
    assert_eq!(q.value().as_bytes(), b"abc\xff");
    assert_eq!(q.to_word().as_bytes(), b"abc\xff");
    assert_eq!(q.get::<ByteString>().unwrap().as_bytes(), b"abc\xff");
    assert_eq!(
        q.to_value(ParamKind::ByteString).unwrap(),
        ParamValue::ByteString(b"abc\xff".into())
    );
    assert_eq!(
        root.get_parameter_or("q", ByteString::new(), SearchScope::Current)
            .unwrap()
            .as_bytes(),
        b"abc\xff"
    );
    // A Rust `String` cannot hold them: the explicit text boundary fails
    // (no replacement), and `get_parameter_or` reports it instead of
    // returning the default (upstream returns the bytes).
    let e = q.try_get::<String>().unwrap_err();
    assert!(
        !e.is_hard() && e.is_invalid_utf8() && e.message().contains("UTF-8"),
        "{e:?}"
    );
    assert!(
        root.get_parameter_or("q", String::from("dflt"), SearchScope::Current)
            .is_err()
    );
    assert!(q.to_value(ParamKind::String).is_err());
    assert!(q.to_value(ParamKind::Enum).is_err());
    assert!(!q.to_bool());
    assert_eq!(q.to_int::<i32>(), 0);
    // A comment inside a key is part of the name, byte for byte.
    let e = root
        .get_parameter(b"E # caf\xe9", SearchScope::Current)
        .unwrap();
    assert_eq!(e.to_real().unwrap(), 2.0);
    assert!(root.parameter("E # caf\u{e9}").is_none()); // UTF-8 é is c3 a9
    // 0xa0 is not white space in the "C" locale.
    let l = root.parameter("l").unwrap();
    assert_eq!(
        l.to_string_list(),
        vec![
            ByteString::from(&b"a\xff"[..]),
            ByteString::from(&b"b\xa0c"[..])
        ]
    );
    let s = root.parameter("s").unwrap();
    assert_eq!(s.to_word().as_bytes(), b"\xa0");
    // Printing: exact bytes, or lossy text for Display.
    let mut out = Vec::new();
    root.write_upstream_bytes(&mut out, 0);
    assert!(out.windows(9).any(|w| w == b"q: abc\xff ("));
    assert!(root.to_string().contains("q: abc\u{fffd} ("));
    // Valid UTF-8 is a plain string.
    let p = common::parse_text("name = \u{e9}t\u{e9} x\n").unwrap();
    let n = p.root.get_parameter("name", SearchScope::Current).unwrap();
    assert_eq!(n.get::<String>().unwrap(), "\u{e9}t\u{e9}");
    assert_eq!(
        n.to_value(ParamKind::String).unwrap(),
        ParamValue::String("\u{e9}t\u{e9} x".into())
    );
}

#[test]
fn high_bytes_are_not_white_space_in_the_grammar() {
    // Qi checks that a character is ASCII before classifying it: 0xa0/0xff
    // never act as white space or end a token, so they stay in values and
    // in section-header text; after a section type they are not a name.
    let p = akantu_parser::Parser::parse_bytes(
        b"v =\xa0[1, 2]\n# \xff\xfe comment\nw = 1\n",
        "b.dat",
        akantu_parser::AnonymousIds::Local(&mut 12),
    )
    .unwrap();
    assert_eq!(
        p.root.parameter("v").unwrap().value.as_bytes(),
        b"\xa0[1, 2]"
    );
    assert!(
        p.root
            .get_parameter("v", SearchScope::Current)
            .unwrap()
            .try_get::<Vec<f64>>()
            .is_err()
    );
    assert_eq!(p.root.parameter("w").unwrap().value, "1");
    assert!(
        akantu_parser::Parser::parse_bytes(
            b"user\xa0[ ]\n",
            "b.dat",
            akantu_parser::AnonymousIds::Local(&mut 12)
        )
        .is_err()
    );
}

/// Upstream members as the oracle probe registers them: `std::string q`,
/// `ID id`, `std::set<ID> names` and `std::string untouched` (default
/// `"default \xff"`, not in the file).
#[derive(Debug)]
struct Registered {
    q: ByteString,
    id: ByteString,
    names: BTreeSet<ByteString>,
    untouched: ByteString,
}

impl Default for Registered {
    fn default() -> Self {
        Self {
            q: ByteString::new(),
            id: ByteString::new(),
            names: BTreeSet::new(),
            untouched: ByteString::from(&b"default \xff"[..]),
        }
    }
}

static REGISTERED: &[ParamDescriptor<Registered>] = &[
    ParamDescriptor::new(
        "q",
        ParamKind::ByteString,
        ParameterAccess::PARSMOD,
        "std::string",
        |s| ParamValue::ByteString(s.q.clone()),
        Some(|s, v| {
            s.q = v.into_byte_string()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "id",
        ParamKind::ByteString,
        ParameterAccess::PARSMOD,
        "ID",
        |s| ParamValue::ByteString(s.id.clone()),
        Some(|s, v| {
            s.id = v.into_byte_string()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "names",
        ParamKind::ByteStringSet,
        ParameterAccess::PARSMOD,
        "std::set<ID>",
        |s| ParamValue::ByteStringSet(s.names.clone()),
        Some(|s, v| {
            s.names = v.into_byte_string_set()?;
            Ok(())
        }),
    ),
    ParamDescriptor::new(
        "untouched",
        ParamKind::ByteString,
        ParameterAccess::PARSMOD,
        "not in the file",
        |s| ParamValue::ByteString(s.untouched.clone()),
        Some(|s, v| {
            s.untouched = v.into_byte_string()?;
            Ok(())
        }),
    ),
];

impl ParameterRegistry for Registered {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(REGISTERED, self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set(REGISTERED, self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(REGISTERED)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(REGISTERED, name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set_from_input(REGISTERED, self, name, value)
    }
}

impl Parsable for Registered {
    fn section_type(&self) -> ParserType {
        ParserType::Global
    }
    fn parsable_id(&self) -> &str {
        "probe"
    }
}

fn hex(b: &[u8]) -> String {
    b.iter().map(|x| format!("{x:02x}")).collect()
}

fn oracle_hex(v: &Value) -> &str {
    v.as_str().expect("hex string in the oracle fixture")
}

/// `q = abc\xff trailing ` registered as an upstream
/// `std::string` and filled by `Parsable::parseSection` stores every byte,
/// trailing space included (C++ oracle: `616263ff20747261696c696e6720`);
/// likewise an `ID` whose raw value ends in white space before a comment,
/// and a `std::set<ID>` with bytes >= 0x80 in byte-wise order.
#[test]
fn registry_keeps_bytes_like_the_oracle() {
    let fixtures = common::fixtures_dir();
    let oracle: Value = serde_json::from_slice(
        &std::fs::read(fixtures.join("parser/registry_bytes.json")).unwrap(),
    )
    .unwrap();
    let input = std::fs::read(fixtures.join("registry/registry_bytes.dat")).unwrap();
    assert_eq!(
        input,
        b"q = abc\xff trailing \nid = \xa0name\xfe x  # c\xe9\nnames = [b\xff, a, \xa0c, a, bA]\n"
    );
    assert_eq!(hex(&input), oracle_hex(&oracle["input"]));
    assert_eq!(oracle["file"], "registry_bytes.dat");
    let p = common::parse_fixture("registry/registry_bytes.dat").unwrap();

    // Through the registry (upstream `ParameterTyped<…>::setAuto`).
    let mut r = Registered::default();
    r.parse_section(&p.root, false).unwrap();
    let reg = &oracle["registry"];
    assert_eq!(hex(&r.q), oracle_hex(&reg["q"]));
    assert_eq!(hex(&r.q), "616263ff20747261696c696e6720");
    assert_eq!(hex(&r.id), oracle_hex(&reg["id"]));
    let names: Vec<String> = r.names.iter().map(|n| hex(n)).collect();
    let oracle_names: Vec<&str> = reg["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(oracle_hex)
        .collect();
    assert_eq!(names, oracle_names);
    assert_eq!(hex(&r.untouched), oracle_hex(&reg["untouched"]));
    // The getter returns the same bytes.
    assert_eq!(
        r.get_param("q").unwrap(),
        ParamValue::ByteString(r.q.clone())
    );

    // The parser-side conversions (`getParameterValue<std::string>`,
    // `getParameter(name, default)`): first word, exact bytes.
    let pv = &oracle["parameter_value"];
    for name in ["q", "id"] {
        let v: ByteString = p
            .root
            .get_parameter_value(name, SearchScope::Current)
            .unwrap();
        assert_eq!(hex(&v), oracle_hex(&pv[name]), "{name}");
    }
    let por = &oracle["parameter_or"];
    let dflt = ByteString::from("dflt");
    let v = p
        .root
        .get_parameter_or("q", dflt.clone(), SearchScope::Current)
        .unwrap();
    assert_eq!(hex(&v), oracle_hex(&por["q"]));
    let v = p
        .root
        .get_parameter_or("missing", dflt, SearchScope::Current)
        .unwrap();
    assert_eq!(hex(&v), oracle_hex(&por["missing"]));
    // The Rust text request cannot hold `abc\xff`: an error, not "dflt".
    let e = p
        .root
        .get_parameter_or("q", String::from("dflt"), SearchScope::Current)
        .unwrap_err();
    assert!(e.to_string().contains("not valid UTF-8"), "{e}");
}

/// The text kinds stay an explicit (documented) UTF-8 boundary; text and
/// bytes coerce where that is lossless or checked.
#[test]
fn registry_text_and_byte_kinds() {
    let p = akantu_parser::Parser::parse_bytes(
        b"q = abc\xff trailing \nnames = [b\xff, a]\n",
        "r.dat",
        akantu_parser::AnonymousIds::Local(&mut 12),
    )
    .unwrap();
    let q = p.root.get_parameter("q", SearchScope::Current).unwrap();
    assert_eq!(
        q.to_value(ParamKind::ByteString).unwrap(),
        ParamValue::ByteString(b"abc\xff trailing ".into())
    );
    let e = q.to_value(ParamKind::String).unwrap_err();
    assert!(e.to_string().contains("not valid UTF-8"), "{e}");
    let names = p.root.get_parameter("names", SearchScope::Current).unwrap();
    assert_eq!(
        names.to_value(ParamKind::ByteStringSet).unwrap(),
        ParamValue::ByteStringSet(
            [ByteString::from("a"), ByteString::from(&b"b\xff"[..])]
                .into_iter()
                .collect()
        )
    );
    assert!(names.to_value(ParamKind::StringSet).is_err());
    let s = &p.root;
    assert_eq!(
        ParamValue::parse_bytes(ParamKind::ByteString, b"x\xff ", s).unwrap(),
        ParamValue::ByteString(b"x\xff ".into())
    );
    assert!(ParamValue::parse_bytes(ParamKind::String, b"x\xff ", s).is_err());

    // Setters: text for a byte kind (e.g. from Python), bytes for a byte kind.
    let mut r = Registered::default();
    r.set_param("q", ParamValue::String("steel ".into()))
        .unwrap();
    assert_eq!(r.q, "steel ");
    r.set_param(
        "names",
        ParamValue::StringSet(["b".to_owned(), "a".to_owned()].into_iter().collect()),
    )
    .unwrap();
    assert_eq!(
        r.names.iter().cloned().collect::<Vec<_>>(),
        vec![ByteString::from("a"), ByteString::from("b")]
    );
    r.set_param("q", ParamValue::ByteString(b"\xff".into()))
        .unwrap();
    assert_eq!(r.q.as_bytes(), b"\xff");
    assert!(r.set_param("q", ParamValue::Real(1.0)).is_err());

    // Value accessors: bytes always, text only for valid UTF-8.
    let b = ParamValue::ByteString(b"abc\xff".into());
    assert_eq!(b.as_bytes().unwrap(), b"abc\xff");
    assert!(b.as_str().is_err());
    assert_eq!(ParamValue::ByteString("ok".into()).as_str().unwrap(), "ok");
    assert_eq!(
        ParamValue::String("ok".into()).into_byte_string().unwrap(),
        "ok"
    );
    assert!(ParamValue::Int(1).into_byte_string().is_err());
    assert!(ParamValue::Int(1).into_byte_string_set().is_err());
}

/// A table registering a parameter with the Rust text kind: bytes that are
/// valid UTF-8 are accepted from a byte value, others are an error (never a
/// lossy replacement).
#[test]
fn text_kind_setter_checks_utf8() {
    #[derive(Debug, Default)]
    struct Text {
        t: String,
        set: BTreeSet<String>,
    }
    static TEXT: &[ParamDescriptor<Text>] = &[
        ParamDescriptor::new(
            "t",
            ParamKind::String,
            ParameterAccess::PARSMOD,
            "text",
            |s| ParamValue::String(s.t.clone()),
            Some(|s, v| {
                s.t = v.as_str()?.to_owned();
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "set",
            ParamKind::StringSet,
            ParameterAccess::PARSMOD,
            "text set",
            |s| ParamValue::StringSet(s.set.clone()),
            Some(|s, v| {
                if let ParamValue::StringSet(x) = v {
                    s.set = x;
                }
                Ok(())
            }),
        ),
    ];
    let mut t = Text::default();
    table_set(
        TEXT,
        &mut t,
        "t",
        ParamValue::ByteString("caf\u{e9}".into()),
    )
    .unwrap();
    assert_eq!(t.t, "caf\u{e9}");
    let e = table_set(TEXT, &mut t, "t", ParamValue::ByteString(b"caf\xe9".into())).unwrap_err();
    assert!(e.to_string().contains("not valid UTF-8"), "{e}");
    assert_eq!(t.t, "caf\u{e9}");
    table_set(
        TEXT,
        &mut t,
        "set",
        ParamValue::ByteStringSet([ByteString::from("x")].into_iter().collect()),
    )
    .unwrap();
    assert_eq!(t.set.iter().collect::<Vec<_>>(), vec!["x"]);
    assert!(
        table_set(
            TEXT,
            &mut t,
            "set",
            ParamValue::ByteStringSet([ByteString::from(&b"\xff"[..])].into_iter().collect()),
        )
        .is_err()
    );
}
