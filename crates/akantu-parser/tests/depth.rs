// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests: finite deep
// nesting that upstream accepts must be accepted, cyclic references must be
// a defined error, and no tree operation may recurse natively.

//! Upstream limits measured on the oracle (8 MiB main-thread stack,
//! `oracle/probes/parser/dump_parser`): 5 000 nested parentheses and 20 000
//! nested signs convert, 20 000 parentheses crash; 5 000 nested sections
//! parse, 20 000 crash; cyclic references always crash. The port has no
//! depth limit. These tests run on the default 2 MiB test-thread stack, so
//! any depth-proportional native recursion would overflow.

mod common;

use akantu_parser::{
    AnonymousIds, ParamKind, ParamValue, Parser, ParserSection, ParserType, SearchScope,
};

fn global_with(params: &[(&str, &str)]) -> ParserSection {
    let mut s = ParserSection::new(ParserType::Global, "global");
    for (n, v) in params {
        s.add_parameter(akantu_parser::ParserParameter::new(*n, *v))
            .unwrap();
    }
    s
}

fn real(s: &ParserSection, name: &str) -> Result<f64, akantu_parser::ConversionError> {
    s.get_parameter(name, SearchScope::CurrentAndParent)
        .unwrap()
        .try_get::<f64>()
}

#[test]
fn review_reproductions() {
    // 130 nested parentheses convert to 1 upstream (0x3ff0…).
    let p = common::parse_text(&format!(
        "deep = {}1{}\nv = [{}2{}]\n",
        "(".repeat(130),
        ")".repeat(130),
        "(".repeat(130),
        ")".repeat(130)
    ))
    .unwrap();
    assert_eq!(
        real(&p.root, "deep").unwrap().to_bits(),
        0x3ff0_0000_0000_0000
    );
    let v: Vec<f64> = p
        .root
        .get_parameter_value("v", SearchScope::Current)
        .unwrap();
    assert_eq!(v, vec![2.0]);
    // 257 nested `user` sections parse upstream.
    let text = format!("{}{}", "user [\n".repeat(257), "]\n".repeat(257));
    let p = common::parse_text(&text).unwrap();
    let mut depth = 0;
    let mut s = &p.root;
    while let Some(c) = s.sub_sections.first() {
        assert_eq!(c.section_type, ParserType::User);
        assert_eq!(c.name, format!("anonymous_{}", 12 + depth));
        s = c;
        depth += 1;
    }
    assert_eq!(depth, 257);
}

#[test]
fn deep_expressions_are_evaluated() {
    let s = global_with(&[]);
    let n = 100_000;
    let paren = format!("{}1{}", "(".repeat(n), ")".repeat(n));
    for kind in [ParamKind::Real, ParamKind::Random] {
        let v = ParamValue::parse(kind, &paren, &s).unwrap();
        match v {
            ParamValue::Real(x) => assert_eq!(x, 1.0),
            ParamValue::Random(r) => assert_eq!((r.base_value, r.distribution), (1.0, None)),
            other => panic!("{other:?}"),
        }
    }
    let v = ParamValue::parse(ParamKind::VectorReal, &format!("[{paren}, 2]"), &s).unwrap();
    assert_eq!(v, ParamValue::VectorReal(vec![1.0, 2.0]));
    // An even number of minus signs.
    let v = ParamValue::parse(ParamKind::Real, &format!("{}3", "-".repeat(n)), &s).unwrap();
    assert_eq!(v, ParamValue::Real(3.0));
    let v =
        ParamValue::parse(ParamKind::Real, &format!("{}3", "+-".repeat(n / 2 + 1)), &s).unwrap();
    assert_eq!(v, ParamValue::Real(-3.0));
    // Nested functions (unary and binary) and powers.
    let sq = format!("{}16{}", "sqrt(".repeat(4), ")".repeat(4));
    assert_eq!(
        ParamValue::parse(ParamKind::Real, &sq, &s).unwrap(),
        ParamValue::Real(16f64.sqrt().sqrt().sqrt().sqrt())
    );
    let m = format!("{}1{}", "max(0, ".repeat(10_000), ")".repeat(10_000));
    assert_eq!(
        ParamValue::parse(ParamKind::Real, &m, &s).unwrap(),
        ParamValue::Real(1.0)
    );
    let pow = format!("2{}", "**1".repeat(50_000));
    assert_eq!(
        ParamValue::parse(ParamKind::Real, &pow, &s).unwrap(),
        ParamValue::Real(2.0)
    );
    // Unbalanced input is still an error, not a crash.
    assert!(ParamValue::parse(ParamKind::Real, &"(".repeat(n), &s).is_err());
    assert!(ParamValue::parse(ParamKind::Real, &format!("{}1", "sqrt(".repeat(n)), &s).is_err());
}

#[test]
fn deep_variable_chains_are_evaluated() {
    // (lookups are linear in the number of parameters of a section, so the
    // chain is kept moderate; upstream crashes on much shorter chains than
    // the port's limit, which is memory)
    let n = 5_000;
    let mut text = String::from("a0 = 1\n");
    for i in 1..=n {
        text.push_str(&format!("a{i} = a{} + 1\n", i - 1));
    }
    text.push_str(&format!("m = [[a{n}, 2], [3, a0]]\nbad0 = 1 +\n"));
    for i in 1..=n {
        text.push_str(&format!("bad{i} = bad{}\n", i - 1));
    }
    let p = common::parse_text(&text).unwrap();
    assert_eq!(real(&p.root, &format!("a{n}")).unwrap(), f64::from(n + 1));
    let m: akantu_core::tensor::MatrixX = p
        .root
        .get_parameter_value("m", SearchScope::Current)
        .unwrap();
    assert_eq!(m.at(0, 0), f64::from(n + 1));
    // An error at the bottom of a long chain keeps its class (the X3
    // expectation failure is hard) and gets a bounded context message.
    let e = real(&p.root, &format!("bad{n}")).unwrap_err();
    assert!(e.is_hard() && !e.is_cyclic_reference(), "{e:?}");
    assert!(e.message().contains("more variables"), "{}", e.message());
    assert!(e.message().len() < 4_000, "{}", e.message().len());
}

#[test]
fn cyclic_references_are_errors() {
    let s = global_with(&[
        ("a", "b"),
        ("b", "a + 1"),
        ("c", "c"),
        ("d", "2 * f"),
        ("f", "max(1, d)"),
        ("v", "[1, a]"),
        ("w", "[c]"),
        ("m", "[w, [1]]"),
        // acyclic diamond: b2 is evaluated twice, not a cycle
        ("top", "b2 + b2 * b2"),
        ("b2", "c2 * 3"),
        ("c2", "2"),
    ]);
    for name in ["a", "b", "c", "d", "f"] {
        let e = real(&s, name).unwrap_err();
        assert!(e.is_hard() && e.is_cyclic_reference(), "{name}: {e:?}");
    }
    let r = s.get_parameter("v", SearchScope::Current).unwrap();
    assert!(r.try_get::<Vec<f64>>().unwrap_err().is_cyclic_reference());
    let r = s.get_parameter("m", SearchScope::Current).unwrap();
    assert!(
        r.try_get::<akantu_core::tensor::MatrixX>()
            .unwrap_err()
            .is_cyclic_reference()
    );
    // Hard: no fall-back to the default (upstream crashes).
    assert!(s.get_parameter_or("a", 5.0, SearchScope::Current).is_err());
    assert_eq!(real(&s, "top").unwrap(), 6.0 + 36.0);
    // Cycles through parent scopes.
    let p = common::parse_text("x = sub\nmaterial m [\n sub = x\n y = sub\n]\n").unwrap();
    let m = &p.root.sub_sections[0];
    // `sub` in the material refers to the global `x`, which refers to the
    // global-scope `sub`… which does not exist: a soft error, not a cycle.
    let e = real(m, "y").unwrap_err();
    assert!(!e.is_cyclic_reference(), "{e:?}");
    let p = common::parse_text("x = y\nmaterial m [\n y = x\n z = y\n]\n").unwrap();
    // material y -> global x -> global y (absent): soft error.
    assert!(
        !real(&p.root.sub_sections[0], "z")
            .unwrap_err()
            .is_cyclic_reference()
    );
}

#[test]
fn deep_trees_have_no_native_recursion() {
    let n = 100_000;
    let text = format!(
        "root = 7\n{}deepest = root * 2\n{}",
        "user [\n".repeat(n),
        "]\n".repeat(n)
    );
    let mut ids = 12;
    let p = Parser::parse_str_with(&text, "deep.dat", AnonymousIds::Local(&mut ids)).unwrap();
    assert_eq!(ids, 12 + n);
    let mut s = &p.root;
    while let Some(c) = s.sub_sections.first() {
        s = c;
    }
    // Variable lookup through 100 000 ancestor scopes.
    assert_eq!(real(s, "deepest").unwrap(), 14.0);
    assert!(s.has_parameter("root", SearchScope::Parent));
    let q = p.clone();
    assert_eq!(p, q);
    let mut r = p.clone();
    r.root.relink();
    assert_eq!(p, r);
    // Debug output is bounded in depth.
    let dbg = format!("{p:?}");
    assert!(dbg.contains("nb_sub_sections"));
    drop((q, r));
    // Missing closing brackets: an error, the open sections are kept.
    let unclosed = "user [\n".repeat(n);
    let mut parser = Parser::default();
    let tmp = std::env::temp_dir().join(format!("akantu-rs-depth-{}.dat", std::process::id()));
    std::fs::write(&tmp, &unclosed).unwrap();
    assert!(parser.parse(&tmp).is_err());
    std::fs::remove_file(&tmp).unwrap();
    let mut depth = 0;
    let mut s = &parser.root;
    while let Some(c) = s.sub_sections.first() {
        s = c;
        depth += 1;
    }
    assert_eq!(depth, n);
    // Printing (output size grows quadratically with the depth).
    let p = common::parse_text(&format!(
        "{}{}",
        "user [\n".repeat(1_000),
        "]\n".repeat(1_000)
    ))
    .unwrap();
    let out = p.to_string();
    assert!(
        out.starts_with(
            "Section(global) global [\n Subsections [\n  Section(user) anonymous_12 [\n"
        )
    );
    assert!(out.ends_with("\n ]\n]\n"));
}
