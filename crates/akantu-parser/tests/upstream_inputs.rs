// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs acceptance test: every input file of the upstream Akantu tree
// (test/, examples/, extra_packages/ at 35a85178) must parse.

//! Parses every `.dat` file (plus `examples/c++/node_to_node/ras_ss.in`, an
//! input file with another extension) of upstream `test/`, `examples/` and
//! `extra_packages/`, copied under `tests/fixtures/` with their relative
//! paths.
//!
//! Files upstream itself rejects are listed in [`UPSTREAM_REJECTED`] with
//! the reason (derived from the grammar in `input_file_parser.hh`).

mod common;

use akantu_parser::{ParamKind, ParserSection, SearchScope};

/// Not Akantu input files: upstream `Parser::parse` rejects them too.
const UPSTREAM_REJECTED: &[(&str, &str)] = &[(
    "test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/K_matrix_verified.dat",
    // A MatrixMarket file read with std::ifstream by
    // test_assembling_K_cohe_elements.cc: the first character '%' is neither
    // a parameter key [a-zA-Z_] nor a section type, so `mini_section`
    // matches nothing and `Parser::parse` reports a parse error at 1:1.
    "MatrixMarket matrix, not an input file",
)];

/// Number of input files copied from the reference tree.
const EXPECTED_FILES: usize = 153;

fn visit(s: &ParserSection, f: &mut impl FnMut(&ParserSection)) {
    f(s);
    for c in &s.sub_sections {
        visit(c, f);
    }
}

#[test]
fn every_upstream_input_parses() {
    let files = common::upstream_inputs();
    assert_eq!(files.len(), EXPECTED_FILES, "fixture set changed");
    let mut parsed = 0;
    let mut params = 0;
    let mut sections = 0;
    for rel in &files {
        let rejected = UPSTREAM_REJECTED.iter().any(|(r, _)| r == rel);
        match common::parse_fixture(rel) {
            Ok(p) => {
                assert!(!rejected, "{rel} is expected to be rejected (as upstream)");
                parsed += 1;
                visit(&p.root, &mut |s| {
                    sections += 1;
                    params += s.parameters.len();
                    // Every conversion of every parameter must terminate
                    // without panicking (errors are fine: the consumer
                    // chooses the type).
                    for q in &s.parameters {
                        let r = s.get_parameter(&q.name, SearchScope::Current).unwrap();
                        for k in [
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
                        ] {
                            let _ = r.to_value(k);
                        }
                    }
                });
            }
            Err(e) => {
                assert!(rejected, "{rel} failed to parse: {e}");
                match e {
                    akantu_core::Error::Parse { line, column, .. } => {
                        assert_eq!((line, column), (1, 1));
                    }
                    other => panic!("{rel}: unexpected error {other}"),
                }
            }
        }
    }
    assert_eq!(parsed, EXPECTED_FILES - UPSTREAM_REJECTED.len());
    eprintln!("parsed {parsed} files, {sections} sections, {params} parameters");
}
