// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): differential test of the
// sub-registry and sub-section machinery of the parameter registry against
// fixtures/parser/subregistry (written by oracle/probes/cpp/parsable_probe.cc
// from upstream ParameterRegistry::registerSubRegistry / get / set /
// printself and Parsable::registerSubSection / parseSection /
// parseSubSection, src/io/parser/{parameter_registry,parsable}.cc).

//! `parser::subregistry_matches_oracle` (PARSE-008/PARSE-009):
//! the same registry trees as the probe are built with
//! [`SubRegistryDescriptor`] / [`SubSectionDescriptor`] tables, every
//! recorded operation is replayed in order and compared with the oracle —
//! values exactly, errors as errors (parse-error texts without the C++
//! source location), the order of parameter applications, sub-section
//! dispatches and `updateInternalParameters` calls, listings and the
//! `printself` text byte for byte. The registry tree is replayed twice:
//! inside the UD-497 compat switch
//! ([`with_upstream_write_access`], upstream `ParameterTyped::set` without
//! access check) it must equal the oracle with no compensation at all; with
//! the port's default (D-003 category B) the only
//! differences are the documented deviation: `set R ro 1` is refused before
//! anything is written or updated (no partial update), so `B.ro` keeps 8.

use std::cell::RefCell;

use akantu_core::Result;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    Parsable, SubRegistryDescriptor, SubSectionDescriptor, print_registry, print_registry_bytes,
    table_description, table_get, table_info, table_names, table_print_value, table_set,
    table_set_from_input, table_sub_parsable_mut, table_sub_registry, table_sub_registry_ids,
    table_sub_registry_mut, with_upstream_write_access,
};
use akantu_parser::{Parser, ParserType};
use akantu_validation::Fixture;

thread_local! {
    static EVENTS: RefCell<Vec<String>> = const { RefCell::new(Vec::new()) };
}

fn event(e: String) {
    EVENTS.with(|v| v.borrow_mut().push(e));
}

fn take_events() -> Vec<String> {
    EVENTS.with(|v| std::mem::take(&mut *v.borrow_mut()))
}

macro_rules! real_param {
    ($ty:ty, $name:literal, $field:ident, $access:expr, $desc:literal) => {
        ParamDescriptor::<$ty>::new(
            $name,
            ParamKind::Real,
            $access,
            $desc,
            |s| ParamValue::Real(s.$field),
            Some(|s, v| {
                s.$field = v.as_real()?;
                Ok(())
            }),
        )
    };
}

/// Registry boilerplate over a parameter table, a sub-registry table and a
/// sub-section table.
macro_rules! registry {
    ($ty:ty, $params:expr, $subs:expr, $sections:expr, $consider:expr) => {
        impl ParameterRegistry for $ty {
            fn get_param(&self, name: &str) -> Result<ParamValue> {
                table_get($params, self, name)
            }
            fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
                table_set($params, self, name, value)
            }
            fn param_names(&self) -> Vec<&'static str> {
                table_names($params)
            }
            fn param_info(&self, name: &str) -> Option<ParamInfo> {
                table_info($params, name)
            }
            fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
                event(format!("param:{}:{name}", self.name));
                table_set_from_input($params, self, name, value)
            }
            fn update_internal_parameters(&mut self) -> Result<()> {
                event(format!("update:{}", self.name));
                Ok(())
            }
            fn param_description(&self, name: &str) -> Option<String> {
                table_description($params, name)
            }
            fn param_print_value(&self, name: &str) -> Option<ParamValue> {
                table_print_value($params, self, name)
            }
            fn sub_registry_ids(&self) -> Vec<&str> {
                table_sub_registry_ids::<$ty>($subs, $sections)
            }
            fn sub_registry(&self, id: &str) -> Option<&dyn ParameterRegistry> {
                table_sub_registry::<$ty>($subs, $sections, self, id)
            }
            fn sub_registry_mut(&mut self, id: &str) -> Option<&mut dyn ParameterRegistry> {
                table_sub_registry_mut::<$ty>($subs, $sections, self, id)
            }
            fn considers_sub_registries(&self) -> bool {
                $consider
            }
        }
    };
}

const PM: ParameterAccess = ParameterAccess::PARSMOD;

/* ------------------------------------------------------------------------ */
/* Plain registries R -> {a_sub: A -> {deep: D}, b_sub: B}                    */
/* ------------------------------------------------------------------------ */

#[derive(Debug)]
struct D {
    name: &'static str,
    z: f64,
}
static D_PARAMS: &[ParamDescriptor<D>] = &[real_param!(D, "z", z, PM, "in D (sub-registry of A)")];
registry!(D, D_PARAMS, &[], &[], true);

#[derive(Debug)]
struct A {
    name: &'static str,
    y: f64,
    shared: f64,
    d: D,
}
static A_PARAMS: &[ParamDescriptor<A>] = &[
    real_param!(A, "y", y, PM, "in A"),
    real_param!(A, "shared", shared, PM, "in A and B"),
];
static A_SUBS: &[SubRegistryDescriptor<A>] =
    &[SubRegistryDescriptor::new("deep", |s| &s.d, |s| &mut s.d)];
registry!(A, A_PARAMS, A_SUBS, &[], true);

#[derive(Debug)]
struct B {
    name: &'static str,
    x: f64,
    shared: f64,
    ro: f64,
}
static B_PARAMS: &[ParamDescriptor<B>] = &[
    real_param!(B, "x", x, PM, "in B"),
    real_param!(B, "shared", shared, PM, "in A and B"),
    // read only; the setter is reachable only through the UD-497 compat
    // switch (upstream assigns it)
    ParamDescriptor::new(
        "ro",
        ParamKind::Real,
        ParameterAccess::READABLE,
        "read only, in B",
        |s| ParamValue::Real(s.ro),
        Some(|s, v| {
            s.ro = v.as_real()?;
            Ok(())
        }),
    ),
];
registry!(B, B_PARAMS, &[], &[], true);

#[derive(Debug)]
struct R {
    name: &'static str,
    r: f64,
    a: A,
    b: B,
}
static R_PARAMS: &[ParamDescriptor<R>] = &[real_param!(R, "r", r, PM, "root parameter")];
// registration order b_sub, a_sub (the listing is sorted, as upstream)
static R_SUBS: &[SubRegistryDescriptor<R>] = &[
    SubRegistryDescriptor::new("b_sub", |s| &s.b, |s| &mut s.b),
    SubRegistryDescriptor::new("a_sub", |s| &s.a, |s| &mut s.a),
];
registry!(R, R_PARAMS, R_SUBS, &[], true);

fn tree() -> R {
    R {
        name: "R",
        r: 1.0,
        a: A {
            name: "A",
            y: 4.0,
            shared: 5.0,
            d: D { name: "D", z: 6.0 },
        },
        b: B {
            name: "B",
            x: 2.0,
            shared: 3.0,
            ro: 8.0,
        },
    }
}

fn reg<'a>(r: &'a R, target: &str) -> &'a dyn ParameterRegistry {
    match target {
        "R" => r,
        "A" => &r.a,
        "B" => &r.b,
        "D" => &r.a.d,
        _ => panic!("unknown registry {target}"),
    }
}

fn reg_mut<'a>(r: &'a mut R, target: &str) -> &'a mut dyn ParameterRegistry {
    match target {
        "R" => r,
        "A" => &mut r.a,
        "B" => &mut r.b,
        "D" => &mut r.a.d,
        _ => panic!("unknown registry {target}"),
    }
}

/* ------------------------------------------------------------------------ */
/* Parsables parent -> {weight_parameter: weight -> {inner}, rule_a (x2)}     */
/* ------------------------------------------------------------------------ */

macro_rules! parsable {
    ($ty:ty, $section:expr, $sections:expr) => {
        impl Parsable for $ty {
            fn section_type(&self) -> ParserType {
                $section
            }
            fn parsable_id(&self) -> &str {
                self.name
            }
            fn sub_parsable_mut(
                &mut self,
                section_type: ParserType,
                name: &str,
            ) -> Option<&mut dyn Parsable> {
                event(format!("sub:{}:{name}", self.name));
                table_sub_parsable_mut::<$ty>($sections, self, section_type, name)
            }
        }
    };
}

#[derive(Debug)]
struct Inner {
    name: &'static str,
    k: f64,
}
static INNER_PARAMS: &[ParamDescriptor<Inner>] = &[real_param!(Inner, "k", k, PM, "inner value")];
registry!(Inner, INNER_PARAMS, &[], &[], false);
parsable!(Inner, ParserType::Rules, &[]);

#[derive(Debug)]
struct Rule {
    name: &'static str,
    value: f64,
}
static RULE_PARAMS: &[ParamDescriptor<Rule>] = &[real_param!(Rule, "value", value, PM, "value")];
registry!(Rule, RULE_PARAMS, &[], &[], false);
parsable!(Rule, ParserType::Rules, &[]);

#[derive(Debug)]
struct Weight {
    name: &'static str,
    radius: f64,
    update_rate: i64,
    inner: Inner,
}
static WEIGHT_PARAMS: &[ParamDescriptor<Weight>] = &[
    real_param!(Weight, "radius", radius, PM, "radius"),
    ParamDescriptor::new(
        "update_rate",
        ParamKind::Int,
        PM,
        "update rate",
        |s| ParamValue::Int(s.update_rate),
        Some(|s, v| {
            s.update_rate = v.as_int()?;
            Ok(())
        }),
    ),
];
static WEIGHT_SECTIONS: &[SubSectionDescriptor<Weight>] = &[SubSectionDescriptor::new(
    ParserType::Rules,
    "inner",
    |s| &s.inner,
    |s| &mut s.inner,
)];
registry!(Weight, WEIGHT_PARAMS, &[], WEIGHT_SECTIONS, false);
parsable!(Weight, ParserType::WeightFunction, WEIGHT_SECTIONS);

#[derive(Debug)]
struct Parent {
    name: &'static str,
    e: f64,
    nu: f64,
    weight: Weight,
    rule_a_first: Rule,
    rule_a_second: Rule,
}
static PARENT_PARAMS: &[ParamDescriptor<Parent>] = &[
    real_param!(Parent, "E", e, PM, "Young's modulus"),
    real_param!(Parent, "nu", nu, PM, "Poisson's ratio"),
];
// rule_a registered twice: the second registration wins (insert_or_assign)
static PARENT_SECTIONS: &[SubSectionDescriptor<Parent>] = &[
    SubSectionDescriptor::new(
        ParserType::WeightFunction,
        "weight_parameter",
        |s| &s.weight,
        |s| &mut s.weight,
    ),
    SubSectionDescriptor::new(
        ParserType::Rules,
        "rule_a",
        |s| &s.rule_a_first,
        |s| &mut s.rule_a_first,
    ),
    SubSectionDescriptor::new(
        ParserType::Rules,
        "rule_a",
        |s| &s.rule_a_second,
        |s| &mut s.rule_a_second,
    ),
];
registry!(Parent, PARENT_PARAMS, &[], PARENT_SECTIONS, false);
parsable!(Parent, ParserType::Material, PARENT_SECTIONS);

fn parsable_tree() -> Parent {
    Parent {
        name: "parent",
        e: 1.0,
        nu: 0.3,
        weight: Weight {
            name: "weight",
            radius: 1.0,
            update_rate: 1,
            inner: Inner {
                name: "inner",
                k: 0.0,
            },
        },
        rule_a_first: Rule {
            name: "rule_a_first",
            value: 0.0,
        },
        rule_a_second: Rule {
            name: "rule_a_second",
            value: 0.0,
        },
    }
}

fn parsable_reg<'a>(p: &'a Parent, target: &str) -> &'a dyn ParameterRegistry {
    match target {
        "parent" => p,
        "weight" => &p.weight,
        "inner" => &p.weight.inner,
        "rule_a_first" => &p.rule_a_first,
        "rule_a_second" => &p.rule_a_second,
        _ => panic!("unknown parsable {target}"),
    }
}

/* ------------------------------------------------------------------------ */

/// The oracle's exception text without the class prefix and the C++
/// source location (`akantu::debug::Exception : <text> [<file>:<line>]`).
fn oracle_message(e: &str) -> &str {
    let e = e.split_once(" : ").map_or(e, |(_, m)| m);
    e.rsplit_once(" [").map_or(e, |(m, _)| m)
}

fn real_of(v: &ParamValue) -> f64 {
    v.as_real().unwrap()
}

struct Op<'a> {
    op: &'a str,
    result: Option<&'a str>,
    error: Option<&'a str>,
    events: Vec<String>,
}

fn ops(fixture: &Fixture) -> Vec<Op<'_>> {
    fixture.manifest().parameters["ops"]
        .as_array()
        .unwrap()
        .iter()
        .map(|o| Op {
            op: o["op"].as_str().unwrap(),
            result: o.get("result").and_then(|r| r.as_str()),
            error: o.get("error").and_then(|r| r.as_str()),
            events: o["events"]
                .as_array()
                .unwrap()
                .iter()
                .map(|e| e.as_str().unwrap().to_owned())
                .collect(),
        })
        .collect()
}

fn open(case: &str) -> Fixture {
    let f = Fixture::open_by_id(case).unwrap();
    f.require_golden().unwrap();
    f.verify_checksums().unwrap();
    f
}

/// Replay of the registry-tree operations. `upstream`: inside the UD-497
/// compat switch, every operation must equal the oracle; otherwise the
/// documented deviation is asserted where it applies. Returns the number of
/// operations that differed from the oracle because of the deviation.
fn replay_registry_tree(fixture: &Fixture, upstream: bool) -> usize {
    let mut r = tree();
    let mut deviations = 0;
    for (k, op) in ops(fixture).iter().enumerate() {
        let words: Vec<&str> = op.op.split(' ').collect();
        take_events();
        let ctx = format!("op {k} `{}` (upstream switch: {upstream})", op.op);
        match words[0] {
            "get" => {
                let got = reg(&r, words[1]).get_param_recursive(words[2]);
                match (op.result, &got) {
                    (Some(v), Ok(g)) if !upstream && words[2] == "ro" && v == "1" => {
                        // consequence of the refused `set R ro 1` (UD-497)
                        assert_eq!(real_of(g), 8.0, "{ctx}");
                        deviations += 1;
                    }
                    (Some(v), Ok(g)) => {
                        assert_eq!(real_of(g), v.parse::<f64>().unwrap(), "{ctx}");
                    }
                    (None, Err(_)) => {}
                    _ => panic!("{ctx}: oracle {:?}/{:?}, port {got:?}", op.result, op.error),
                }
            }
            "set" => {
                let value: f64 = words[3].parse().unwrap();
                let res = reg_mut(&mut r, words[1])
                    .set_param_recursive(words[2], ParamValue::Real(value));
                let events = take_events();
                if !upstream && words[2] == "ro" {
                    // UD-497 (D-003 B): B refuses the read-only parameter,
                    // checked before anything is written or updated
                    assert!(
                        res.as_ref()
                            .is_err_and(|e| e.to_string().contains("not writable")),
                        "{ctx}: the port refuses a read-only parameter: {res:?}"
                    );
                    assert_eq!(op.result, Some("ok"), "{ctx}");
                    assert_eq!(events, Vec::<String>::new(), "{ctx}: no partial update");
                    deviations += 1;
                    continue;
                }
                assert_eq!(res.is_ok(), op.result == Some("ok"), "{ctx}: {res:?}");
                assert_eq!(events, op.events, "{ctx}: events");
                continue;
            }
            "has" => {
                let has = reg(&r, words[1]).has_parameter(words[2]);
                assert_eq!(op.result, Some(if has { "true" } else { "false" }), "{ctx}");
            }
            "list" => {
                let g = reg(&r, words[1]);
                let got = match words[2] {
                    "params" => {
                        let mut n = g.param_names();
                        n.sort_unstable();
                        n.join(",")
                    }
                    "subs" => g.list_sub_registries().join(","),
                    _ => panic!("{ctx}"),
                };
                assert_eq!(op.result, Some(got.as_str()), "{ctx}");
            }
            "print" => {
                let got = print_registry(reg(&r, words[1]), 0);
                assert_eq!(
                    print_registry_bytes(reg(&r, words[1]), 0),
                    got.as_bytes(),
                    "{ctx}: text and byte forms"
                );
                let oracle = op.result.unwrap();
                let mut expected = oracle.to_owned();
                if !upstream {
                    // B.ro kept its value in the port (refused `set R ro 1`)
                    let ro_line = "[read only, in B]                      : ";
                    expected = oracle.replace(&format!("{ro_line}1\n"), &format!("{ro_line}8\n"));
                    if expected != oracle {
                        deviations += 1;
                    }
                }
                assert_eq!(got, expected, "{ctx}");
            }
            _ => panic!("{ctx}: unknown operation"),
        }
        assert_eq!(take_events(), op.events, "{ctx}: events");
    }
    let finals = fixture.load_f64("final_values").unwrap().data;
    let ours = [r.r, r.b.x, r.b.shared, r.a.y, r.a.shared, r.a.d.z, r.b.ro];
    if upstream {
        assert_eq!(ours.to_vec(), finals, "final values (upstream switch)");
    } else {
        assert_eq!(ours[..6], finals[..6]);
        assert_eq!(
            (finals[6], ours[6]),
            (1.0, 8.0),
            "B.ro: upstream sets it, the port refuses (UD-497)"
        );
    }
    deviations
}

#[test]
fn registry_tree_matches_oracle() {
    let fixture = open("parser/subregistry/registry_tree");
    // UD-497 compat switch: the oracle sequence exactly, no compensation
    let compensations = with_upstream_write_access(|| replay_registry_tree(&fixture, true));
    assert_eq!(compensations, 0);
    // default (D-003 B): the read-only refusal and its two consequences
    let deviations = replay_registry_tree(&fixture, false);
    assert_eq!(deviations, 3, "the read-only deviation is exercised");
}

#[test]
fn parsable_tree_matches_oracle() {
    let fixture = open("parser/subregistry/parsable_tree");
    let inputs = &fixture.manifest().parameters["inputs"];
    let mut p = parsable_tree();
    for (k, op) in ops(&fixture).iter().enumerate() {
        let words: Vec<&str> = op.op.split(' ').collect();
        take_events();
        let ctx = format!("op {k} `{}`", op.op);
        match words[0] {
            "get" => {
                let got = parsable_reg(&p, words[1]).get_param_recursive(words[2]);
                match (op.result, &got) {
                    (Some(v), Ok(ParamValue::Int(i))) => assert_eq!(v, i.to_string(), "{ctx}"),
                    (Some(v), Ok(g)) => {
                        assert_eq!(real_of(g), v.parse::<f64>().unwrap(), "{ctx}");
                    }
                    (None, Err(_)) => {}
                    _ => panic!("{ctx}: oracle {:?}/{:?}, port {got:?}", op.result, op.error),
                }
            }
            "set" => {
                let value: f64 = words[3].parse().unwrap();
                let res = p.set_param_recursive(words[2], ParamValue::Real(value));
                assert_eq!(res.is_ok(), op.result == Some("ok"), "{ctx}: {res:?}");
            }
            "list" => {
                let got = parsable_reg(&p, words[1]).list_sub_registries().join(",");
                assert_eq!(op.result, Some(got.as_str()), "{ctx}");
            }
            "parse" => {
                let file = format!("parsable_{}.dat", words[1]);
                let text = inputs[&file].as_str().unwrap();
                let parser = Parser::parse_str(text, &file).unwrap();
                let section = parser
                    .root
                    .sub_sections_of(ParserType::Material)
                    .next()
                    .unwrap();
                let res = p.parse_section(section, parser.is_permissive());
                match (op.error, &res) {
                    (None, Ok(())) => {}
                    (Some(e), Err(err)) => {
                        assert!(
                            err.to_string().ends_with(oracle_message(e)),
                            "{ctx}: port `{err}`, oracle `{e}`"
                        );
                    }
                    _ => panic!("{ctx}: oracle {:?}/{:?}, port {res:?}", op.result, op.error),
                }
            }
            "print" => {
                let got = print_registry(&p, 0);
                assert_eq!(op.result, Some(got.as_str()), "{ctx}");
            }
            _ => panic!("{ctx}: unknown operation"),
        }
        assert_eq!(take_events(), op.events, "{ctx}: events");
    }
    let finals = fixture.load_f64("final_values").unwrap().data;
    #[allow(clippy::cast_precision_loss)] // small test value
    let ours = [
        p.e,
        p.nu,
        p.weight.radius,
        p.weight.update_rate as f64,
        p.weight.inner.k,
        p.rule_a_first.value,
        p.rule_a_second.value,
    ];
    assert_eq!(ours.to_vec(), finals);
}
