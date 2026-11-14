// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No Akantu
// counterpart: cross-language check of the fixture format. The fixtures
// are written by oracle/probes/selftest/writer_selftest.{cc,py}.

use akantu_validation::fixture::Fixture;
use akantu_validation::{DiffTest, FixtureStatus, fixtures_root};

fn special_values() -> Vec<f64> {
    vec![
        0.0,
        -0.0,
        1.0,
        0.1,
        -1.0 / 3.0,
        f64::from_bits(1),
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::EPSILON,
        f64::from_bits(1.0f64.to_bits() + 1),
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
    ]
}

fn check_case(case_id: &str) {
    let fx = Fixture::open_by_id(case_id).unwrap();
    fx.verify_checksums().unwrap();
    assert_eq!(fx.manifest().status, FixtureStatus::Golden);

    // Bit-exact read of special values (NaN payload aside).
    let special = fx.load_f64("special").unwrap();
    for (got, want) in special.data.iter().zip(special_values()) {
        if want.is_nan() {
            assert!(got.is_nan());
        } else {
            assert_eq!(got.to_bits(), want.to_bits(), "{got:e} vs {want:e}");
        }
    }
    assert_eq!(
        fx.load_i64("i32").unwrap().data,
        vec![i64::from(i32::MIN), -1, 0, i64::from(i32::MAX)]
    );
    assert_eq!(fx.load_i64("u8").unwrap().data, vec![0, 255]);
    assert_eq!(fx.load_i64("flags").unwrap().data, vec![1, 0, 1]);
    let empty = fx.load_f64("empty").unwrap();
    assert_eq!(empty.shape, vec![0, 3]);
    let scalar = fx.load_f64("scalar").unwrap();
    assert!(scalar.shape.is_empty());
    assert_eq!(scalar.data, vec![42.5]);
    let rows = fx.load_ragged_f64("residuals").unwrap();
    assert_eq!(rows, vec![vec![1.0, 0.5], vec![], vec![0.25]]);
    let u = fx.quantity("u").unwrap();
    assert_eq!(
        u.components.as_deref(),
        Some(&["x".to_string(), "y".to_string()][..])
    );
    assert_eq!(u.ordering[0].key_quantity.as_deref(), Some("node_ids"));
    assert_eq!(fx.manifest().parameters["pi"], std::f64::consts::PI);

    // End-to-end through the harness with the area tolerance file.
    let mut t = DiffTest::new(&format!("selftest::{case_id}"), case_id).unwrap();
    let uv: Vec<f64> = (0..4)
        .flat_map(|n| (0..2).map(move |c| f64::from(n) + 0.25 * f64::from(c)))
        .collect();
    assert!(t.check_f64("u", &uv));
    assert!(t.check_f64("special", &special_values()));
    assert!(t.check_i64("node_ids", &[10, 11, 12, 13]));
    t.assert_pass();
}

#[test]
fn cpp_writer_roundtrip() {
    check_case("selftest/cpp_writer_roundtrip");
}

#[test]
fn python_writer_roundtrip() {
    check_case("selftest/python_writer_roundtrip");
}

#[test]
fn writers_are_byte_identical() {
    let root = fixtures_root().join("selftest");
    let cpp = Fixture::open(&root.join("cpp_writer_roundtrip")).unwrap();
    let py = Fixture::open(&root.join("python_writer_roundtrip")).unwrap();
    for q in &cpp.manifest().quantities {
        let other = py.quantity(&q.name).unwrap();
        assert_eq!(
            q.sha256, other.sha256,
            "quantity {} differs between writers",
            q.name
        );
    }
}
