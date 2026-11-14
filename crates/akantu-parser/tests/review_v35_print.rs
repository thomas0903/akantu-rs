// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Regression tests
// of the registry printout against upstream
// src/io/parser/parameter_registry_tmpl.hh: generic
// `ParameterTyped<T>::printself` (`stream << param << "\n"`, std::string
// bytes unchanged), the Matrix / std::vector / std::set specialisations
// (`"[ " … "]\n"`) and `ParameterTyped<bool>::printself`
// (`stream << std::boolalpha << param`).

//! `review_v35_print` (row PARSE-008): [`print_registry_bytes`] writes the
//! exact bytes of upstream `ParameterRegistry::printself` for every
//! parameter type of the port, and [`print_registry`] is its text form
//! (lossy only for bytes that are not UTF-8). The expected lines are
//! derived from the reference source (the oracle probe covers `Real` and
//! `Int` only; the other types are owed to an extended probe run, see
//! docs/NUMERICAL_VALIDATION.md).

use std::collections::BTreeSet;

use akantu_core::Result;
use akantu_core::tensor::MatrixX;
use akantu_parser::ByteString;
use akantu_parser::param::{
    ParamDescriptor, ParamInfo, ParamKind, ParamValue, ParameterAccess, ParameterRegistry,
    print_registry, print_registry_bytes, table_description, table_get, table_info, table_names,
    table_print_value, table_set,
};

#[derive(Debug)]
struct Reg {
    flag: bool,
    off: bool,
    bytes: ByteString,
    set: BTreeSet<ByteString>,
    vector: Vec<f64>,
    matrix: MatrixX,
}

const PM: ParameterAccess = ParameterAccess::PARSMOD;

static PARAMS: &[ParamDescriptor<Reg>] = &[
    ParamDescriptor::new(
        "flag",
        ParamKind::Bool,
        PM,
        "a bool",
        |s| ParamValue::Bool(s.flag),
        None,
    ),
    ParamDescriptor::new(
        "off",
        ParamKind::Bool,
        PM,
        "",
        |s| ParamValue::Bool(s.off),
        None,
    ),
    ParamDescriptor::new(
        "id",
        ParamKind::ByteString,
        PM,
        "bytes",
        |s| ParamValue::ByteString(s.bytes.clone()),
        None,
    ),
    ParamDescriptor::new(
        "names",
        ParamKind::ByteStringSet,
        PM,
        "set",
        |s| ParamValue::ByteStringSet(s.set.clone()),
        None,
    ),
    ParamDescriptor::new(
        "v",
        ParamKind::VectorReal,
        PM,
        "",
        |s| ParamValue::VectorReal(s.vector.clone()),
        None,
    ),
    ParamDescriptor::new(
        "m",
        ParamKind::MatrixReal,
        PM,
        "",
        |s| ParamValue::MatrixReal(s.matrix.clone()),
        None,
    ),
];

impl ParameterRegistry for Reg {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(PARAMS, self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set(PARAMS, self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(PARAMS)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(PARAMS, name)
    }
    fn param_description(&self, name: &str) -> Option<String> {
        table_description(PARAMS, name)
    }
    fn param_print_value(&self, name: &str) -> Option<ParamValue> {
        table_print_value(PARAMS, self, name)
    }
}

fn reg(bytes: Vec<u8>) -> Reg {
    let mut matrix = MatrixX::zeros(2, 2);
    for (i, j, v) in [(0, 0, 1.0), (0, 1, 2.5), (1, 0, -3.0), (1, 1, 1e-7)] {
        *matrix.at_mut(i, j) = v;
    }
    Reg {
        flag: true,
        off: false,
        bytes: ByteString::from(bytes.clone()),
        set: [ByteString::from(b"b".to_vec()), ByteString::from(bytes)]
            .into_iter()
            .collect(),
        vector: vec![0.5, 1e10, -2.0],
        matrix,
    }
}

/// `bool` prints with `std::boolalpha` (`true` / `false`), in the
/// text and the byte forms.
#[test]
fn vg03_bool_parameter_prints_true_false() {
    let r = reg(b"abc".to_vec());
    let out = print_registry(&r, 0);
    assert!(
        out.lines()
            .any(|l| l.contains("flag") && l.ends_with(": true")),
        "{out:?}"
    );
    assert!(
        out.lines()
            .any(|l| l.contains("off") && l.ends_with(": false")),
        "{out:?}"
    );
    assert_eq!(print_registry_bytes(&r, 0), out.into_bytes());
}

/// `std::string` (and the elements of `std::set<std::string>`) are
/// streamed unchanged: the byte form carries `0xff`; the text form is the
/// same output decoded lossily.
#[test]
fn vg03_non_utf8_byte_string_prints_raw_bytes() {
    let r = reg(vec![0x61, 0xff]);
    let bytes = print_registry_bytes(&r, 0);
    let line = |name: &[u8]| -> Vec<u8> {
        bytes
            .split(|&b| b == b'\n')
            .find(|l| l.windows(name.len()).any(|w| w == name))
            .unwrap()
            .to_vec()
    };
    assert!(line(b" id").ends_with(b": a\xff"), "{:x?}", line(b" id"));
    // std::set order is byte-wise: "a\xff" < "b"
    assert!(
        line(b" names").ends_with(b": [ a\xff b ]"),
        "{:x?}",
        line(b" names")
    );
    assert!(!bytes.windows(3).any(|w| w == "\u{FFFD}".as_bytes()));
    assert_eq!(
        print_registry(&r, 0),
        String::from_utf8_lossy(&bytes).into_owned()
    );
}

/// The whole output, derived from upstream's `printself` with the default
/// stream state (`%g`, precision 6): parameters in name order, flags,
/// name and description widths (10 / 50), every type's line form.
#[test]
fn vg03_every_type_prints_as_upstream() {
    let r = reg(b"steel".to_vec());
    let expected = concat!(
        " rwp flag     [a bool]                               : true\n",
        " rwp id       [bytes]                                : steel\n",
        " rwp m                                               : [ [ 1 2.5 ] [ -3 1e-07 ] ]\n",
        " rwp names    [set]                                  : [ b steel ]\n",
        " rwp off                                             : false\n",
        " rwp v                                               : [ 0.5 1e+10 -2 ]\n",
    );
    assert_eq!(print_registry(&r, 0), expected);
    assert_eq!(print_registry_bytes(&r, 0), expected.as_bytes());
}

/// A registry with marigo's random threshold `Yd` (parsable only, as
/// upstream `registerParam("Yd", Yd, _pat_parsable, "Damaging energy
/// threshold")`, `material_marigo.cc:42`).
#[derive(Debug)]
struct RandomReg {
    yd: akantu_parser::param::RandomParameter,
}

static RANDOM_PARAMS: &[ParamDescriptor<RandomReg>] = &[ParamDescriptor::new(
    "Yd",
    ParamKind::Random,
    ParameterAccess::PARSABLE,
    "Damaging energy threshold",
    |s| ParamValue::Random(s.yd.clone()),
    None,
)];

impl ParameterRegistry for RandomReg {
    fn get_param(&self, name: &str) -> Result<ParamValue> {
        table_get(RANDOM_PARAMS, self, name)
    }
    fn set_param(&mut self, name: &str, value: ParamValue) -> Result<()> {
        table_set(RANDOM_PARAMS, self, name, value)
    }
    fn param_names(&self) -> Vec<&'static str> {
        table_names(RANDOM_PARAMS)
    }
    fn param_info(&self, name: &str) -> Option<ParamInfo> {
        table_info(RANDOM_PARAMS, name)
    }
    fn param_description(&self, name: &str) -> Option<String> {
        table_description(RANDOM_PARAMS, name)
    }
    fn param_print_value(&self, name: &str) -> Option<ParamValue> {
        table_print_value(RANDOM_PARAMS, self, name)
    }
}

/// A random parameter prints as upstream's
/// `RandomInternalField [ … ]` descriptor (`random_internal_field_tmpl.hh:58-69`;
/// upstream can register random parameters only as `RandomInternalField`
/// members). Expected line: the `Yd` line of the oracle print probe,
/// `std::cout << model.getMaterial(0)` for `Yd = 50 uniform [0, 10]`
/// (x86_64 strict install),
/// byte for byte at the material's indent.
#[test]
fn vg2_02_random_parameter_prints_the_random_internal_field_descriptor() {
    let r = RandomReg {
        // `Yd = 50 uniform [0, 10]`
        yd: akantu_parser::param::RandomParameter {
            base_value: 50.0,
            distribution: Some(akantu_parser::param::RandomDistribution::Uniform(0.0, 10.0)),
        },
    };
    let expected = concat!(
        "  --p Yd       [Damaging energy threshold]            : ",
        "RandomInternalField [ 50 + uniform [ 0.00000000000000000e+00 1.00000000000000000e+01 ] ]\n"
    );
    assert_eq!(print_registry_bytes(&r, 1), expected.as_bytes());
}
