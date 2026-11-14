// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/parameter_registry_tmpl.hh
// (ParameterTyped<std::string>::setAuto, ParameterTyped<std::set<T>>),
// src/io/parser/parsable.cc (Parsable::parseSection) with the runtime
// registration of python/py_parser.cc (registerParam with runtime names).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! The runtime
//! registry `DynamicParameters` stores upstream `std::string`, `ID` and
//! `std::set<ID>` parameters byte for byte, exactly like the C++ oracle
//! (record `akantu-parser/tests/fixtures/parser/registry_bytes.json`,
//! produced by `oracle/probes/parser/registry_probe.cc`, which registers the
//! same four parameters in a C++ `Parsable`).

use std::path::Path;

use akantu_core::Result;
use akantu_model::params::DynamicParameters;
use akantu_parser::param::{ParamInfo, ParamKind, ParamValue, ParameterAccess};
use akantu_parser::{AnonymousIds, ByteString, ParameterRegistry, Parsable, Parser, ParserType};
use serde_json::Value;

/// A law-like object whose parameters are all registered at run time.
struct Runtime {
    params: DynamicParameters,
}

impl ParameterRegistry for Runtime {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        self.params.get(name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.params.set(name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        Vec::new() // runtime names are owned: `DynamicParameters::names`
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        self.params.param_info(name)
    }
    fn set_param_from_input(&mut self, name: &str, value: ParamValue) -> Result<()> {
        self.params.set_from_input(name, value)
    }
}

impl Parsable for Runtime {
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

fn bytes_of(v: &ParamValue) -> &[u8] {
    match v {
        ParamValue::ByteString(b) => b.as_bytes(),
        other => panic!("expected a byte string, got {other:?}"),
    }
}

#[test]
fn runtime_registry_keeps_bytes_like_the_oracle() {
    let fixtures = Path::new(env!("CARGO_MANIFEST_DIR")).join("../akantu-parser/tests/fixtures");
    let oracle: Value = serde_json::from_slice(
        &std::fs::read(fixtures.join("parser/registry_bytes.json")).unwrap(),
    )
    .unwrap();
    let input = std::fs::read(fixtures.join("registry/registry_bytes.dat")).unwrap();
    assert_eq!(hex(&input), oracle["input"].as_str().unwrap());

    // The probe's registrations (`std::string q`, `ID id`, `std::set<ID>
    // names`, `std::string untouched = "default \xff"`), at run time.
    let mut r = Runtime {
        params: DynamicParameters::new(),
    };
    let pm = ParameterAccess::PARSMOD;
    r.params
        .register("q", ParamKind::ByteString, pm, "std::string", None)
        .unwrap();
    r.params
        .register("id", ParamKind::ByteString, pm, "ID", None)
        .unwrap();
    r.params
        .register("names", ParamKind::ByteStringSet, pm, "std::set<ID>", None)
        .unwrap();
    r.params
        .register(
            "untouched",
            ParamKind::ByteString,
            pm,
            "not in the file",
            Some(ParamValue::ByteString(ByteString::from(
                &b"default \xff"[..],
            ))),
        )
        .unwrap();

    let mut counter = 12;
    let p = Parser::parse_bytes(
        &input,
        "registry_bytes.dat",
        AnonymousIds::Local(&mut counter),
    )
    .unwrap();
    r.parse_section(&p.root, false).unwrap();

    let reg = &oracle["registry"];
    for name in ["q", "id", "untouched"] {
        assert_eq!(
            hex(bytes_of(&r.get_param(name).unwrap())),
            reg[name].as_str().unwrap(),
            "{name}"
        );
    }
    assert_eq!(
        hex(bytes_of(&r.get_param("q").unwrap())),
        "616263ff20747261696c696e6720",
        "trailing white space kept"
    );
    let ParamValue::ByteStringSet(names) = r.get_param("names").unwrap() else {
        panic!("names is a byte-string set");
    };
    let names: Vec<String> = names.iter().map(|n| hex(n)).collect();
    let oracle_names: Vec<&str> = reg["names"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap())
        .collect();
    assert_eq!(names, oracle_names, "byte-wise std::set order");

    // A Python `str` sets the byte kind losslessly.
    r.set_param("q", ParamValue::String("steel".into()))
        .unwrap();
    assert_eq!(
        r.get_param("q").unwrap(),
        ParamValue::ByteString(ByteString::from("steel"))
    );
}
