// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_io/test_parser/test_parser.cc,
// test_parser.verified, test_parser.py (parser part).
// (test_parser.cc: 2013-2023, test_parser.py: 2020-2025)
// Copyright (©) 2013-2025 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).

//! Port of upstream `test_parser`: parses `input_file.dat`, prints the tree
//! and the converted values, and compares with `test_parser.verified`
//! **byte for byte** (the upstream test compares the same output).

mod common;

use akantu_core::tensor::MatrixX;
use akantu_parser::stream::format_double;
use akantu_parser::{Parser, RandomParameter, SearchScope};

const INPUT: &str = "test/test_io/test_parser/input_file.dat";

/// Akantu's Eigen `IOFormat(StreamPrecision, DontAlignCols, ", ", ", ", "[",
/// "]", "[", "]")`.
fn eigen(m: &MatrixX) -> String {
    let rows: Vec<String> = (0..m.rows())
        .map(|i| {
            let r: Vec<String> = (0..m.cols()).map(|j| format_double(m.at(i, j))).collect();
            format!("[{}]", r.join(", "))
        })
        .collect();
    format!("[{}]", rows.join(", "))
}

fn run(p: &Parser) -> String {
    let g = &p.root;
    let mut out = String::new();
    // initialize("input_file.dat") seeds the RNG with the file's `seed`.
    let seed: i64 = g
        .get_parameter("seed", SearchScope::Current)
        .unwrap()
        .to_int();
    out += &format!("{seed}==123456\n");
    out += &format!("{p}\n");
    let toto: f64 = g.get_parameter_value("toto", SearchScope::Current).unwrap();
    let reference = 2.0 * std::f64::consts::PI + 2f64.max(50.0);
    assert!((toto - reference).abs() <= f64::EPSILON);
    out += &format!("{}=={}\n", format_double(toto), format_double(reference));
    let vect: Vec<f64> = g.get_parameter_value("vect", SearchScope::Current).unwrap();
    let row = MatrixX::from_col_major(1, vect.len(), vect).unwrap();
    out += &format!("{}\n", eigen(&row));
    let mat: MatrixX = g.get_parameter_value("mat", SearchScope::Current).unwrap();
    out += &format!("{}\n", eigen(&mat));
    for r in ["rand1", "rand2", "rand3"] {
        let v: RandomParameter = g.get_parameter_value(r, SearchScope::Current).unwrap();
        out += &format!("{v}\n");
    }
    out
}

#[test]
fn test_parser_matches_verified_output() {
    let p = common::parse_fixture(INPUT).unwrap();
    let verified = std::fs::read_to_string(
        common::fixtures_dir().join("test/test_io/test_parser/test_parser.verified"),
    )
    .unwrap();
    assert_eq!(run(&p), verified);
    assert!(p.is_permissive());
    assert_eq!(p.last_parsed_file(), "input_file.dat");
}

#[test]
fn test_parser_python() {
    // test_parser.py::TestParser::test_parse_input
    let p = common::parse_fixture(INPUT).unwrap();
    let toto = p.root.get_parameter("toto", SearchScope::Current).unwrap();
    assert_eq!(toto.value(), "2*pi + max(2, general)");
    let v = toto.to_real().unwrap();
    assert_eq!(v, 2.0 * std::f64::consts::PI + 50.0);
    let mat = p
        .root
        .get_parameter("mat", SearchScope::Current)
        .unwrap()
        .to_matrix()
        .unwrap();
    let reference = [[1.0, 25.0, 5.0, v], [0.0, 10.0, 50.0, 13.0]];
    for (i, row) in reference.iter().enumerate() {
        for (j, x) in row.iter().enumerate() {
            assert_eq!(mat.at(i, j), *x);
        }
    }
    // test_parser.py::TestParsable: the material section's `E`.
    let sect = p
        .root
        .sub_sections_of(akantu_parser::ParserType::Material)
        .next()
        .unwrap();
    let e = sect.get_parameter("E", SearchScope::Current).unwrap();
    assert_eq!(e.to_real().unwrap(), 1.0);
    // X135 = 1 + 3* debug_level: debug_level is found in the global scope.
    let x: f64 = sect
        .get_parameter_value("X135", SearchScope::Current)
        .unwrap();
    assert_eq!(x, 4.0);
    // n = [ 1, 23+2, 5, toto ] (toto from the parent scope)
    let n: Vec<f64> = sect.get_parameter_value("n", SearchScope::Current).unwrap();
    assert_eq!(n, vec![1.0, 25.0, 5.0, v]);
}

mod parsable_py {
    //! test_parser.py::TestParameterRegistry / TestParsable with a table
    //! registry (`E` parsmod, `X135` parsable only).
    use akantu_parser::param::*;
    use akantu_parser::{ParserType, SearchScope};

    #[derive(Debug, Default)]
    struct Mat {
        e: f64,
        x135: f64,
    }

    static TABLE: &[ParamDescriptor<Mat>] = &[
        ParamDescriptor::new(
            "E",
            ParamKind::Real,
            ParameterAccess::PARSMOD,
            "Young's modulus",
            |s| ParamValue::Real(s.e),
            Some(|s, v| {
                s.e = v.as_real()?;
                Ok(())
            }),
        ),
        ParamDescriptor::new(
            "X135",
            ParamKind::Real,
            ParameterAccess::PARSABLE,
            "Random parameter",
            |s| ParamValue::Real(s.x135),
            Some(|s, v| {
                s.x135 = v.as_real()?;
                Ok(())
            }),
        ),
    ];

    impl ParameterRegistry for Mat {
        fn get_param(&self, name: &str) -> akantu_core::Result<ParamValue> {
            table_get(TABLE, self, name)
        }
        fn set_param(&mut self, name: &str, v: ParamValue) -> akantu_core::Result<()> {
            table_set(TABLE, self, name, v)
        }
        fn param_names(&self) -> Vec<&'static str> {
            table_names(TABLE)
        }
        fn param_info(&self, name: &str) -> Option<ParamInfo> {
            table_info(TABLE, name)
        }
        fn set_param_from_input(&mut self, name: &str, v: ParamValue) -> akantu_core::Result<()> {
            table_set_from_input(TABLE, self, name, v)
        }
    }

    impl Parsable for Mat {
        fn section_type(&self) -> ParserType {
            ParserType::Material
        }
        fn parsable_id(&self) -> &str {
            "test_material"
        }
    }

    #[test]
    fn test_parsable() {
        let mut m = Mat::default();
        m.set_param("E", ParamValue::Real(210_000.0)).unwrap();
        assert_eq!(m.get_param("E").unwrap(), ParamValue::Real(210_000.0));
        let p = super::common::parse_fixture(super::INPUT).unwrap();
        let sect = p.root.sub_sections_of(ParserType::Material).next().unwrap();
        let e: f64 = sect.get_parameter_value("E", SearchScope::Current).unwrap();
        assert_eq!(e, 1.0);
        // The file sets `permissive_parser = true`: unknown parameters
        // (name, yop, a, n) and the `rules` sub-section are warnings.
        m.parse_section(sect, p.is_permissive()).unwrap();
        assert_eq!(m.get_param("E").unwrap(), ParamValue::Real(1.0));
        assert!(m.get_param("X135").is_err()); // not readable
        assert_eq!(m.x135, 4.0);
        // Not permissive: the first unknown parameter in upstream order.
        let err = m.parse_section(sect, false).unwrap_err().to_string();
        assert!(
            err.contains("No parameter named a registered in test_material."),
            "{err}"
        );
    }
}
