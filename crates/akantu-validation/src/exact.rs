// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the exact (bitwise) comparison of
// binary64 values of the differential-testing harness, including the
// cross-platform default-NaN convention of decision D-046
// (docs/DECISIONS.md, docs/NUMERICAL_VALIDATION.md "Default-NaN convention
// (D-046)").

//! Exact (bitwise) comparison of `f64` values: the single implementation of
//! decision **D-046**.
//!
//! IEEE 754 leaves the sign and the payload of the NaN produced by an
//! invalid operation (`0/0`, `inf − inf`, `0 · inf`, ...) to the
//! implementation. aarch64 produces the *default quiet NaN*
//! `0x7ff8000000000000`; x86_64 SSE produces `0xfff8000000000000` ("real
//! indefinite"). Both the C++ oracle and the port produce their platform's
//! default NaN, so a golden value generated on one platform cannot be met
//! bit for bit on the other.
//!
//! Under an exact rule two values therefore match when
//!
//! 1. their bit patterns are identical, **or**
//! 2. both are default quiet NaNs that differ only in the sign bit
//!    (exponent all ones, quiet bit set, zero payload).
//!
//! Everything else stays bitwise: NaNs with a non-zero payload, signalling
//! NaNs, NaN against a non-NaN, `+0` against `−0`, infinities and all finite
//! values. This is not a numerical tolerance.
//!
//! Helpers that hash raw bits (state digests of a replay) use
//! [`canonical_bits`] / [`canonical_le_bytes`]: a default NaN of either sign
//! is hashed as `0x7ff8000000000000` and every other value as its own bits,
//! so digests computed before D-046 on aarch64 are unchanged.
//!
//! [`exact_eq`] is the predicate of every exact comparison: raw-bit
//! comparisons (digests, bit identity counts, the parser oracle) and the
//! tolerance comparator's exact rules ([`crate::compare::compare_f64`]),
//! where `+0` and `−0` do not match either (decision D-051). Digests
//! canonicalise default NaNs only, never zeros (D-051 (3)).

/// The positive default quiet NaN (`0x7ff8000000000000`): aarch64's NaN for
/// invalid operations and the canonical form used by [`canonical_bits`].
pub const DEFAULT_NAN_BITS: u64 = 0x7ff8_0000_0000_0000;

/// The negative default quiet NaN (`0xfff8000000000000`): x86_64 SSE's
/// "real indefinite".
pub const NEGATIVE_DEFAULT_NAN_BITS: u64 = 0xfff8_0000_0000_0000;

const SIGN_BIT: u64 = 1 << 63;

/// `true` if the bit pattern `bits` is a default quiet NaN of either sign
/// (`0x7ff8000000000000` or `0xfff8000000000000`).
#[must_use]
pub const fn is_default_nan_bits(bits: u64) -> bool {
    bits & !SIGN_BIT == DEFAULT_NAN_BITS
}

/// `true` if `x` is a default quiet NaN of either sign.
#[must_use]
pub fn is_default_nan(x: f64) -> bool {
    is_default_nan_bits(x.to_bits())
}

/// D-046 exact match of two bit patterns: identical, or both default quiet
/// NaNs (any sign).
#[must_use]
pub const fn exact_bits_eq(expected: u64, actual: u64) -> bool {
    expected == actual || (is_default_nan_bits(expected) && is_default_nan_bits(actual))
}

/// D-046 exact match of two values: identical bit patterns, or both default
/// quiet NaNs (any sign). `+0` and `−0` do **not** match.
#[must_use]
pub fn exact_eq(expected: f64, actual: f64) -> bool {
    exact_bits_eq(expected.to_bits(), actual.to_bits())
}

/// Bits of `x` with a default NaN of either sign canonicalised to
/// [`DEFAULT_NAN_BITS`]; every other value (other NaNs included) keeps its
/// own bits. Two values have equal canonical bits iff [`exact_eq`] holds.
#[must_use]
pub fn canonical_bits(x: f64) -> u64 {
    let bits = x.to_bits();
    if is_default_nan_bits(bits) {
        DEFAULT_NAN_BITS
    } else {
        bits
    }
}

/// Little-endian bytes of [`canonical_bits`], for digests (drop-in
/// replacement for `x.to_le_bytes()` when hashing oracle-compared state).
#[must_use]
pub fn canonical_le_bytes(x: f64) -> [u8; 8] {
    canonical_bits(x).to_le_bytes()
}

/// `true` if both slices have the same length and every entry matches
/// under [`exact_eq`].
#[must_use]
pub fn exact_slices_eq(expected: &[f64], actual: &[f64]) -> bool {
    expected.len() == actual.len() && expected.iter().zip(actual).all(|(&e, &a)| exact_eq(e, a))
}

/// Number of entries that do not match under [`exact_eq`]; a length
/// difference counts as that many mismatches.
#[must_use]
pub fn exact_mismatch_count(expected: &[f64], actual: &[f64]) -> usize {
    expected.len().abs_diff(actual.len())
        + expected
            .iter()
            .zip(actual)
            .filter(|&(&e, &a)| !exact_eq(e, a))
            .count()
}

#[cfg(test)]
mod tests {
    use super::*;

    const PAYLOAD_NAN: u64 = 0x7ff8_0000_0000_0001;
    const NEG_PAYLOAD_NAN: u64 = 0xfff8_0000_0000_0001;
    const SIGNALLING_NAN: u64 = 0x7ff0_0000_0000_0001;
    const NEG_SIGNALLING_NAN: u64 = 0xfff4_0000_0000_0000;

    fn f(bits: u64) -> f64 {
        f64::from_bits(bits)
    }

    #[test]
    fn identical_values_match() {
        for x in [
            0.0,
            -0.0,
            1.0,
            -2.5,
            f64::MIN_POSITIVE,
            f64::from_bits(1), // smallest subnormal
            f64::MAX,
            f64::INFINITY,
            f64::NEG_INFINITY,
            f(DEFAULT_NAN_BITS),
            f(NEGATIVE_DEFAULT_NAN_BITS),
            f(PAYLOAD_NAN),
            f(SIGNALLING_NAN),
        ] {
            assert!(exact_eq(x, x), "{:#018x}", x.to_bits());
        }
    }

    #[test]
    fn default_nans_of_either_sign_match() {
        let (pos, neg) = (f(DEFAULT_NAN_BITS), f(NEGATIVE_DEFAULT_NAN_BITS));
        assert!(exact_eq(pos, neg));
        assert!(exact_eq(neg, pos));
        assert!(is_default_nan(pos) && is_default_nan(neg));
        // Rust's `f64::NAN` is the positive default NaN.
        assert_eq!(f64::NAN.to_bits(), DEFAULT_NAN_BITS);
        assert!(exact_eq(f64::NAN, -f64::NAN));
    }

    /// The x86_64 default-NaN pattern: aarch64 golden
    /// `0x7ff8000000000000`, x86_64 port `0xfff8000000000000`. Raw bit
    /// equality rejects it; D-046 accepts it.
    #[test]
    fn x86_pattern_against_aarch64_golden() {
        let golden = f(DEFAULT_NAN_BITS);
        let x86 = f(NEGATIVE_DEFAULT_NAN_BITS);
        assert_ne!(golden.to_bits(), x86.to_bits());
        assert!(exact_eq(golden, x86));
        assert!(exact_slices_eq(&[1.0, golden, -0.0], &[1.0, x86, -0.0]));
        assert_eq!(exact_mismatch_count(&[1.0, golden], &[1.0, x86]), 0);
        assert_eq!(canonical_bits(x86), canonical_bits(golden));
    }

    #[test]
    fn payload_nan_does_not_match_a_default_nan() {
        for d in [DEFAULT_NAN_BITS, NEGATIVE_DEFAULT_NAN_BITS] {
            for p in [PAYLOAD_NAN, NEG_PAYLOAD_NAN] {
                assert!(!exact_eq(f(d), f(p)), "{d:#x} vs {p:#x}");
                assert!(!exact_eq(f(p), f(d)), "{p:#x} vs {d:#x}");
            }
        }
        // payload NaNs differing only in sign are not default NaNs either
        assert!(!exact_eq(f(PAYLOAD_NAN), f(NEG_PAYLOAD_NAN)));
        assert!(!is_default_nan(f(PAYLOAD_NAN)));
    }

    #[test]
    fn signalling_nan_does_not_match() {
        for s in [SIGNALLING_NAN, NEG_SIGNALLING_NAN] {
            assert!(f(s).is_nan());
            assert!(!is_default_nan_bits(s));
            for d in [DEFAULT_NAN_BITS, NEGATIVE_DEFAULT_NAN_BITS, PAYLOAD_NAN] {
                assert!(!exact_bits_eq(s, d), "{s:#x} vs {d:#x}");
                assert!(!exact_bits_eq(d, s), "{d:#x} vs {s:#x}");
            }
        }
        assert!(!exact_bits_eq(SIGNALLING_NAN, NEG_SIGNALLING_NAN));
    }

    #[test]
    fn nan_does_not_match_a_number() {
        for n in [DEFAULT_NAN_BITS, NEGATIVE_DEFAULT_NAN_BITS, PAYLOAD_NAN] {
            for x in [0.0, -0.0, 1.0, f64::INFINITY, f64::NEG_INFINITY, f64::MAX] {
                assert!(!exact_eq(f(n), x), "{n:#x} vs {x}");
                assert!(!exact_eq(x, f(n)), "{x} vs {n:#x}");
            }
        }
    }

    #[test]
    fn signed_zeros_do_not_match() {
        assert!(!exact_eq(0.0, -0.0));
        assert!(!exact_eq(-0.0, 0.0));
        assert_ne!(canonical_bits(0.0), canonical_bits(-0.0));
    }

    #[test]
    fn finite_and_infinite_values_stay_bitwise() {
        let one_up = f64::from_bits(1.0f64.to_bits() + 1);
        assert!(!exact_eq(1.0, one_up));
        assert!(!exact_eq(f64::INFINITY, f64::NEG_INFINITY));
        assert!(!exact_eq(f64::MAX, f64::INFINITY));
        assert!(!exact_eq(f64::from_bits(1), 0.0));
    }

    #[test]
    fn canonical_bits_change_only_the_negative_default_nan() {
        assert_eq!(
            canonical_bits(f(NEGATIVE_DEFAULT_NAN_BITS)),
            DEFAULT_NAN_BITS
        );
        assert_eq!(canonical_bits(f(DEFAULT_NAN_BITS)), DEFAULT_NAN_BITS);
        for b in [
            0u64,
            SIGN_BIT,
            1.0f64.to_bits(),
            PAYLOAD_NAN,
            NEG_PAYLOAD_NAN,
            SIGNALLING_NAN,
            NEG_SIGNALLING_NAN,
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
        ] {
            assert_eq!(canonical_bits(f(b)), b, "{b:#x}");
            assert_eq!(canonical_le_bytes(f(b)), f(b).to_le_bytes(), "{b:#x}");
        }
        assert_eq!(
            canonical_le_bytes(f(NEGATIVE_DEFAULT_NAN_BITS)),
            f(DEFAULT_NAN_BITS).to_le_bytes()
        );
    }

    /// `exact_eq(a, b)` ⇔ `canonical_bits(a) == canonical_bits(b)` over a
    /// set of bit patterns covering every class.
    #[test]
    fn canonical_bits_agree_with_exact_eq() {
        let set = [
            0u64,
            SIGN_BIT,
            1.0f64.to_bits(),
            (-1.0f64).to_bits(),
            1,
            DEFAULT_NAN_BITS,
            NEGATIVE_DEFAULT_NAN_BITS,
            PAYLOAD_NAN,
            NEG_PAYLOAD_NAN,
            SIGNALLING_NAN,
            NEG_SIGNALLING_NAN,
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
        ];
        for &a in &set {
            for &b in &set {
                assert_eq!(
                    exact_eq(f(a), f(b)),
                    canonical_bits(f(a)) == canonical_bits(f(b)),
                    "{a:#x} vs {b:#x}"
                );
            }
        }
    }

    #[test]
    fn slice_helpers_count_lengths_and_mismatches() {
        assert!(!exact_slices_eq(&[1.0], &[1.0, 2.0]));
        assert_eq!(exact_mismatch_count(&[1.0], &[1.0, 2.0, 3.0]), 2);
        assert_eq!(exact_mismatch_count(&[0.0, 1.0], &[-0.0, 1.0]), 1);
        assert!(exact_slices_eq(&[], &[]));
    }
}
