// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the intent of
// test/test_solver/test_sparse_matrix_profile.cc (add(i, j) profile,
// add(i, j, v) values, saveProfile/saveMatrix) and the Matrix Market output
// of SparseMatrixAIJ::saveMatrix/saveProfile and
// SolverVectorDefault::saveVector, compared byte for byte with
// fixtures/solver/text/matrix_market (black-box record of libstdc++'s
// `std::ostream << double` at precision 15, D-030).

//! Matrix Market text and release counters against the oracle (SPM-007).
//! Byte comparisons select upstream's precision explicitly
//! (`MatrixMarketOptions::upstream_precision`, UD-061); the default policy
//! (17 digits) round-trips bit for bit.

mod solver_oracle;

use akantu_core::GlobalIdx;
use akantu_core::enums::MatrixType;
use akantu_linalg::matrix_market::{self, MatrixMarketOptions, format_value_upstream};
use akantu_linalg::{EquationLayout, SolverVector, SparseMatrixAij};
use solver_oracle::*;

#[test]
fn ostream_precision_15_matches_oracle() {
    let f = &cases("text")[0];
    let values = f64s(f, "ostream/values");
    let text = String::from_utf8(input_bytes(f, "ostream_text")).unwrap();
    let lines: Vec<&str> = text.lines().collect();
    assert_eq!(lines.len(), values.len());
    for (v, want) in values.iter().zip(lines) {
        assert_eq!(
            format_value_upstream(*v),
            want,
            "value bits {:016x}",
            v.to_bits()
        );
    }
}

#[test]
fn profile_matrix_vector_text_and_releases_match_oracle() {
    let f = &cases("text")[0];
    let n: GlobalIdx = param(f, "system_size").as_i64().unwrap();
    let special = f64s(f, "ostream/values");
    for (id, ty) in [("S", MatrixType::Symmetric), ("U", MatrixType::Unsymmetric)] {
        let mut a = SparseMatrixAij::new(id, n, ty);
        let mut seq_p = vec![a.profile_release() as i64];
        let mut seq_v = vec![a.value_release() as i64];
        let mut snap = |a: &SparseMatrixAij| {
            seq_p.push(a.profile_release() as i64);
            seq_v.push(a.value_release() as i64);
        };
        for (k, &v) in special.iter().enumerate() {
            let (i, j) = (((k * 7) % 13) as GlobalIdx, ((k * 5 + 3) % 13) as GlobalIdx);
            a.add_to_profile(i, j).unwrap();
            snap(&a);
            a.add(i, j, v).unwrap();
            snap(&a);
        }
        a.add(2, 9, 1.0).unwrap();
        a.add(9, 2, 2.0).unwrap();
        snap(&a);
        let upstream = MatrixMarketOptions::upstream_precision();
        assert_eq!(
            a.matrix_market_text_with(&upstream).as_bytes(),
            input_bytes(f, &format!("save_matrix_{id}")).as_slice(),
            "{id}: saveMatrix text (upstream precision)"
        );
        assert_eq!(
            a.profile_text().as_bytes(),
            input_bytes(f, &format!("save_profile_{id}")).as_slice(),
            "{id}: saveProfile text"
        );
        assert_matrix(f, id, &a);
        // Default policy (17 digits, UD-061): the round trip is bit-exact
        // (same coordinates, same order, same value bits; NaN stays NaN).
        let mm = matrix_market::parse(&a.matrix_market_text()).unwrap();
        let back = SparseMatrixAij::from_matrix_market("B", &mm).unwrap();
        assert_eq!(back.coo().0, a.coo().0);
        assert_eq!(back.coo().1, a.coo().1);
        for (x, y) in back.values().iter().zip(a.values()) {
            assert!(
                akantu_validation::exact_eq(*y, *x), // D-046
                "default round trip {y:e} -> {x:e}"
            );
        }
        // Upstream precision (15 digits) is lossy (UD-061): for normal,
        // non-overflowing values the relative read-back change is at most
        // half a unit in the 15th digit, i.e. ≤ 5e-15 (e.g. the binary64
        // value of 1.000000000000005 prints 1.00000000000001, a change of
        // ≈4.9e-15); subnormals are quantised in absolute terms and DBL_MAX
        // prints as 1.79769313486232e+308, which reads back as +inf.
        let mm = matrix_market::parse(&a.matrix_market_text_with(&upstream)).unwrap();
        let back = SparseMatrixAij::from_matrix_market("B", &mm).unwrap();
        assert_eq!(back.coo().0, a.coo().0);
        assert_eq!(back.coo().1, a.coo().1);
        assert_eq!(back.matrix_type(), a.matrix_type());
        for (x, y) in back.values().iter().zip(a.values()) {
            assert!(
                akantu_validation::exact_eq(*y, *x) // D-046
                    || (x - y).abs() <= 5e-15 * y.abs()
                    || (y.abs() < f64::MIN_POSITIVE && (x - y).abs() <= 5e-15 * 1e-308)
                    || (x.is_infinite() && y.abs() > 1.797_693_134_862_315e308),
                "upstream-precision round trip {y:e} -> {x:e}"
            );
        }
        a.mul(2.0);
        snap(&a);
        a.set(0.5);
        snap(&a);
        a.zero();
        snap(&a);
        a.clear_profile();
        snap(&a);
        assert_eq!(
            seq_p,
            i64s(f, &format!("{id}/release_sequence/profile")),
            "{id}: profile releases"
        );
        assert_eq!(
            seq_v,
            i64s(f, &format!("{id}/release_sequence/value")),
            "{id}: value releases"
        );
    }
    // saveVector: the residual is 0 + r (assembled into a zero vector)
    let r = f64s(f, "residual");
    let layout = EquationLayout::serial(r.len()).unwrap();
    let mut v = SolverVector::zeros(r.len());
    for (a, b) in v.values.iter_mut().zip(&r) {
        *a += *b;
    }
    assert_eq!(
        v.matrix_market_text_with(&layout, &MatrixMarketOptions::upstream_precision())
            .unwrap()
            .as_bytes(),
        input_bytes(f, "save_vector").as_slice()
    );
}
