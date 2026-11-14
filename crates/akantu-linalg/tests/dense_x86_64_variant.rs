// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): no direct counterpart; data checks of the
// x86_64 platform variant of the dense parity fixture (records of
// src/common/aka_types.hh / aka_voigthelper.hh computed by
// oracle/probes/dense/run.sh on the pinned x86_64 oracle).

//! The x86_64 platform variant of `dense_strict.txt` (D-043, D-073 (1)/(3)):
//! `tests/fixtures/dense/dense_strict_x86_64_delta.txt` holds
//! the 188 records the pinned x86_64 oracle computes differently, keyed by
//! their line in `dense_strict.txt`. This test checks the variant's data
//! claims, independently of the port's complex division (whose x86_64
//! runtime and default are `ComplexDivision::Unfused`): every
//! entry names an existing record with the same operation and key; the 87
//! `detinv` records differ only by the sign of the default NaN (class X3,
//! equal under D-046's canonicalisation); the 101 others are exactly the
//! `cdiv` (45) and `eig` (56) records (class X4, the platform's
//! `__divdc3`). The aarch64 records stay authoritative for
//! `dense_parity.rs` until the x86_64 complex-division runtime is the
//! port's default (D-073 (3)); the replay of these 101 records with the
//! unfused runtime belongs to that change.

use akantu_validation::exact::canonical_bits;

const AARCH64: &str = include_str!("fixtures/dense/dense_strict.txt");
const X86_64: &str = include_str!("fixtures/dense/dense_strict_x86_64_delta.txt");

/// One value token of a record: an IEEE-754 bit pattern (16 hex digits) or
/// a marker such as `EXC` (an exception raised by the oracle).
#[derive(Debug, PartialEq)]
enum Token<'a> {
    Bits(u64),
    Marker(&'a str),
}

/// (operation key before the first `|`, the value tokens).
fn split(record: &str) -> (&str, Vec<Token<'_>>) {
    let (key, rest) = record.split_once('|').expect("record without '|'");
    let tokens = rest
        .split(|c: char| c == '|' || c.is_whitespace())
        .filter(|t| !t.is_empty())
        .map(|t| match u64::from_str_radix(t, 16) {
            Ok(u) if t.len() == 16 => Token::Bits(u),
            _ => Token::Marker(t),
        })
        .collect();
    (key.trim(), tokens)
}

/// D-046: the default NaN's sign is not significant.
fn canonical<'a>(tokens: &[Token<'a>]) -> Vec<Token<'a>> {
    tokens
        .iter()
        .map(|t| match t {
            Token::Bits(u) => Token::Bits(canonical_bits(f64::from_bits(*u))),
            Token::Marker(m) => Token::Marker(m),
        })
        .collect()
}

#[test]
fn x86_64_variant_records_are_the_classified_differences() {
    let aarch64: Vec<&str> = AARCH64.lines().collect();
    let (mut detinv, mut cdiv, mut eig) = (0, 0, 0);
    let mut last = 0;
    for line in X86_64.lines().filter(|l| !l.starts_with('#')) {
        let (n, record) = line.split_once(" | ").expect("`<line> | <record>`");
        let n: usize = n.parse().expect("line number");
        assert!(n > last, "entries in increasing line order: {n}");
        last = n;
        let reference = aarch64[n - 1];
        assert_ne!(record, reference, "line {n}: identical record listed");
        let (key_x, bits_x) = split(record);
        let (key_a, bits_a) = split(reference);
        assert_eq!(key_x, key_a, "line {n}: operation/key");
        assert_eq!(bits_x.len(), bits_a.len(), "line {n}: number of values");
        match key_x.split_whitespace().next() {
            Some("detinv") => {
                detinv += 1;
                assert_eq!(
                    canonical(&bits_x),
                    canonical(&bits_a),
                    "line {n}: detinv differs beyond the default-NaN sign"
                );
            }
            Some("cdiv") => cdiv += 1,
            Some("eig") => eig += 1,
            other => panic!("line {n}: unexpected operation {other:?}"),
        }
    }
    assert_eq!((detinv, cdiv, eig), (87, 45, 56));
}
