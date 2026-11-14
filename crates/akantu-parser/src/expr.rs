// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/algebraic_parser_x3.cc
// (scalar expressions, `parser::algebraic::parse_real`),
// src/io/parser/algebraic_parser.hh (Qi `AlgebraicGrammar`, `VectorGrammar`,
// `MatrixGrammar`, `RandomGeneratorGrammar`), src/io/parser/parser_real.cc,
// src/io/parser/parser_grammar_tmpl.hh (`Parser::parseType`).
// Copyright (©) 2010-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).
//
// The real-number literal parsers (section "Real-number literals" below:
// the power-of-ten table, sign/inf/nan/exponent parsing, digit accumulation,
// scaling and `x3_real`/`qi_real`) are derived from Boost.Spirit 1.83
// (`x3::double_`, `qi::real_parser<double>`), the Boost version of the
// oracle image, files boost/spirit/home/x3/support/numeric_utils/
// {extract_real.hpp, extract_int.hpp, pow10.hpp, detail/extract_int.hpp},
// x3/numeric/real_policies.hpp, x3/string/detail/string_parse.hpp,
// qi/numeric/{real_policies.hpp, numeric_utils.hpp, detail/real_impl.hpp,
// detail/numeric_utils.hpp}, qi/detail/string_parse.hpp and
// support/detail/pow10.hpp, which carry the notices:
//   Copyright (c) 2001-2019 Joel de Guzman (x3: 2001-2014; qi real
//     parser: 2001-2019; qi numeric utilities, string_parse, pow10: 2001-2011)
//   Copyright (c) 2001-2011 Hartmut Kaiser
//   Copyright (c) 2011 Jan Frederick Eick
//   Copyright (c) 2011 Christopher Jefferson
//   Copyright (c) 2006 Stephen Nutt
//   Distributed under the Boost Software License, Version 1.0. (See
//   LICENSES/BSL-1.0.txt or http://www.boost.org/LICENSE_1_0.txt)
// The derived code is distributed here under LGPL-3.0-or-later together with
// the Boost notices above (BSL-1.0 permits this).

//! Algebraic expressions of input-file values.
//!
//! Upstream has **two** expression grammars (UD-055):
//!
//! | | scalars (`Real` parameters, variables) | vector/matrix entries, random parameters |
//! |---|---|---|
//! | implementation | Spirit X3 (`algebraic_parser_x3.cc`) | Spirit Qi (`algebraic_parser.hh`) |
//! | `number` alternatives | real, `-`n, `+`n, **function, constant**, variable, `(`e`)` | real, `-`n, `+`n, **constant, function**, `(`e`)`, variable |
//! | function names | case-sensitive, binary tried before unary | case-insensitive, unary tried before binary |
//! | real-number literal | X3: mantissa accumulated in `double` | Qi: mantissa accumulated in `uint64_t`, ≤ 17 integer digits |
//!
//! Both grammars are reproduced exactly (two dialects of one
//! recursive-descent evaluator). Consequences that are preserved on purpose:
//! in vectors, `exp(…)`, `erf(…)`, … are rejected (the constant `e` matches
//! first) and `atan2(…)` is rejected (unary `atan` matches first), while
//! `COS(0)` is accepted; variable names that start with a function name
//! (`cosine`) or with `e`/`pi`/`inf`/`nan` cannot be referenced. Grammar
//! (both dialects):
//!
//! ```text
//! expr   = term   (('+' | '-') term)*        left associative
//! term   = factor (('*' | '/') factor)*      left associative
//! factor = number ("**" number)*             left associative: 2**3**2 = 64
//! number = real | '-' number | '+' number | function | constant
//!        | variable | '(' expr ')'           (order: see table)
//! ```
//! `-2**2 = 4` (the sign belongs to the number). Variables are other
//! parameters of the current section or of its ancestors, always evaluated
//! with the scalar (X3) grammar in the section that defines them.
//!
//! Error classes: upstream X3 expectation failures escape as
//! `std::runtime_error` (not `akantu::debug::Exception`), so
//! `ParserSection::getParameter(name, default)` does *not* fall back to the
//! default for them; [`ConversionError::is_hard`] records this.
//!
//! Nesting: upstream evaluates by native recursion (Spirit rules, and a new
//! parse for every variable reference) and overflows the stack on deep input
//! (measured on the oracle with an 8 MiB stack: 5 000 nested parentheses
//! work, 20 000 crash) and on cyclic variable references (`a = a`). The port
//! evaluates with an explicit heap stack, including variable references, so
//! every finite acyclic expression is evaluated whatever its depth, and
//! detects cycles exactly: a reference to a parameter whose value is being
//! evaluated can only recurse forever, so it is a hard error with
//! [`ConversionError::is_cyclic_reference`] (category A).

use std::collections::HashSet;
use std::fmt;

use crate::cmath::{BINARY, Binary, CONSTANTS, UNARY, Unary};
use crate::scope::{ScopeRef, SearchScope};
use crate::section::ParserParameter;
use crate::stream::is_c_space;

/// Which upstream expression grammar to emulate.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Dialect {
    /// `algebraic_parser_x3.cc`: scalar `Real` values and variables.
    X3,
    /// `algebraic_parser.hh`: vector/matrix entries and random parameters.
    Qi,
}

/// A failed value conversion.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ConversionError {
    message: String,
    offset: usize,
    hard: bool,
    cyclic: bool,
    invalid_utf8: bool,
}

impl ConversionError {
    pub(crate) fn soft(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
            hard: false,
            cyclic: false,
            invalid_utf8: false,
        }
    }

    pub(crate) fn hard(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
            hard: true,
            cyclic: false,
            invalid_utf8: false,
        }
    }

    /// A value upstream converts successfully, but whose bytes are not valid
    /// UTF-8 and therefore cannot be returned as the Rust text (`String`)
    /// the caller explicitly asked for.
    pub(crate) fn invalid_utf8(message: impl Into<String>, offset: usize) -> Self {
        Self {
            message: message.into(),
            offset,
            hard: false,
            cyclic: false,
            invalid_utf8: true,
        }
    }

    /// Human-readable description.
    #[must_use]
    pub fn message(&self) -> &str {
        &self.message
    }

    /// Byte offset in the converted text where the error was detected.
    #[must_use]
    pub fn offset(&self) -> usize {
        self.offset
    }

    /// `true` if upstream raises an exception that is *not* an
    /// `akantu::debug::Exception` (a Spirit X3 `expectation_failure`, or a
    /// crash for cyclic references): `getParameter(name, default)` does not
    /// fall back to the default for those.
    #[must_use]
    pub fn is_hard(&self) -> bool {
        self.hard
    }

    /// `true` for a cyclic variable reference (`a = b`, `b = a + 1`), on
    /// which upstream recurses until the stack overflows (category A: the
    /// port reports this hard error instead).
    #[must_use]
    pub fn is_cyclic_reference(&self) -> bool {
        self.cyclic
    }

    /// `true` if upstream converts the value successfully but the requested
    /// Rust text type (`String`, `Vec<String>`, `BTreeSet<String>`,
    /// [`crate::ParamKind::String`], …) cannot hold it because the bytes are
    /// not valid UTF-8. This is a Rust text-boundary error, not an upstream
    /// exception: [`crate::ParserSection::get_parameter_or`] reports it
    /// instead of substituting the default. The byte conversions
    /// ([`crate::ByteString`], [`crate::ParamKind::ByteString`], …) never
    /// produce it.
    #[must_use]
    pub fn is_invalid_utf8(&self) -> bool {
        self.invalid_utf8
    }

    fn context(self, prefix: &str) -> Self {
        Self {
            message: format!("{prefix}: {}", self.message),
            ..self
        }
    }
}

impl fmt::Display for ConversionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for ConversionError {}

type R<T> = Result<Option<T>, ConversionError>;

// ---------------------------------------------------------------------------
// Real-number literals (Boost.Spirit 1.83)
// ---------------------------------------------------------------------------

/// Exact powers of ten `1e0 … 1e308` as written in Spirit's
/// `pow10_helper<double>` table (correctly rounded literals).
#[rustfmt::skip]
static POW10: [f64; 309] = [
    1e0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9,
    1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16, 1e17, 1e18, 1e19,
    1e20, 1e21, 1e22, 1e23, 1e24, 1e25, 1e26, 1e27, 1e28, 1e29,
    1e30, 1e31, 1e32, 1e33, 1e34, 1e35, 1e36, 1e37, 1e38, 1e39,
    1e40, 1e41, 1e42, 1e43, 1e44, 1e45, 1e46, 1e47, 1e48, 1e49,
    1e50, 1e51, 1e52, 1e53, 1e54, 1e55, 1e56, 1e57, 1e58, 1e59,
    1e60, 1e61, 1e62, 1e63, 1e64, 1e65, 1e66, 1e67, 1e68, 1e69,
    1e70, 1e71, 1e72, 1e73, 1e74, 1e75, 1e76, 1e77, 1e78, 1e79,
    1e80, 1e81, 1e82, 1e83, 1e84, 1e85, 1e86, 1e87, 1e88, 1e89,
    1e90, 1e91, 1e92, 1e93, 1e94, 1e95, 1e96, 1e97, 1e98, 1e99,
    1e100, 1e101, 1e102, 1e103, 1e104, 1e105, 1e106, 1e107, 1e108, 1e109,
    1e110, 1e111, 1e112, 1e113, 1e114, 1e115, 1e116, 1e117, 1e118, 1e119,
    1e120, 1e121, 1e122, 1e123, 1e124, 1e125, 1e126, 1e127, 1e128, 1e129,
    1e130, 1e131, 1e132, 1e133, 1e134, 1e135, 1e136, 1e137, 1e138, 1e139,
    1e140, 1e141, 1e142, 1e143, 1e144, 1e145, 1e146, 1e147, 1e148, 1e149,
    1e150, 1e151, 1e152, 1e153, 1e154, 1e155, 1e156, 1e157, 1e158, 1e159,
    1e160, 1e161, 1e162, 1e163, 1e164, 1e165, 1e166, 1e167, 1e168, 1e169,
    1e170, 1e171, 1e172, 1e173, 1e174, 1e175, 1e176, 1e177, 1e178, 1e179,
    1e180, 1e181, 1e182, 1e183, 1e184, 1e185, 1e186, 1e187, 1e188, 1e189,
    1e190, 1e191, 1e192, 1e193, 1e194, 1e195, 1e196, 1e197, 1e198, 1e199,
    1e200, 1e201, 1e202, 1e203, 1e204, 1e205, 1e206, 1e207, 1e208, 1e209,
    1e210, 1e211, 1e212, 1e213, 1e214, 1e215, 1e216, 1e217, 1e218, 1e219,
    1e220, 1e221, 1e222, 1e223, 1e224, 1e225, 1e226, 1e227, 1e228, 1e229,
    1e230, 1e231, 1e232, 1e233, 1e234, 1e235, 1e236, 1e237, 1e238, 1e239,
    1e240, 1e241, 1e242, 1e243, 1e244, 1e245, 1e246, 1e247, 1e248, 1e249,
    1e250, 1e251, 1e252, 1e253, 1e254, 1e255, 1e256, 1e257, 1e258, 1e259,
    1e260, 1e261, 1e262, 1e263, 1e264, 1e265, 1e266, 1e267, 1e268, 1e269,
    1e270, 1e271, 1e272, 1e273, 1e274, 1e275, 1e276, 1e277, 1e278, 1e279,
    1e280, 1e281, 1e282, 1e283, 1e284, 1e285, 1e286, 1e287, 1e288, 1e289,
    1e290, 1e291, 1e292, 1e293, 1e294, 1e295, 1e296, 1e297, 1e298, 1e299,
    1e300, 1e301, 1e302, 1e303, 1e304, 1e305, 1e306, 1e307, 1e308,
];

fn pow10(exp: i64) -> f64 {
    // Callers guarantee 0 <= exp <= 308.
    usize::try_from(exp)
        .ok()
        .and_then(|e| POW10.get(e))
        .copied()
        .unwrap_or(f64::INFINITY)
}

/// Result of a real-number literal parser.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) enum Num {
    /// Parsed value and end offset.
    Ok(f64, usize),
    /// No match, nothing consumed.
    Fail,
    /// No match, but (as in Spirit) the input up to this offset was consumed:
    /// this happens when the decimal exponent is out of range
    /// (`scale()` fails, e.g. `1e400`). The following alternatives of the
    /// enclosing `number` rule continue from there.
    FailAt(usize),
}

const INT_MAX: i64 = i32::MAX as i64;
const INT_MIN: i64 = i32::MIN as i64;

fn digit(c: u8) -> Option<u32> {
    c.is_ascii_digit().then(|| u32::from(c - b'0'))
}

/// `extract_sign`: returns `true` for `-`.
fn parse_sign(s: &[u8], i: &mut usize) -> bool {
    match s.get(*i) {
        Some(b'-') => {
            *i += 1;
            true
        }
        Some(b'+') => {
            *i += 1;
            false
        }
        _ => false,
    }
}

/// Case-insensitive `string_parse` (advances only on a full match).
fn string_parse(s: &[u8], i: &mut usize, lower: &[u8]) -> bool {
    let end = *i + lower.len();
    if end <= s.len() && s[*i..end].eq_ignore_ascii_case(lower) {
        *i = end;
        true
    } else {
        false
    }
}

/// `parse_nan` of `real_policies` (`nan`, `nan(...)`, any case).
fn parse_nan(s: &[u8], i: &mut usize) -> Option<f64> {
    if !matches!(s.get(*i), Some(b'n' | b'N')) {
        return None;
    }
    let mut j = *i;
    if !string_parse(s, &mut j, b"nan") {
        return None;
    }
    if s.get(j) == Some(&b'(') {
        // Skip the trailing "(...)"; give up if there is no ')'. (Upstream
        // leaves the iterator after "nan" in that case, but every caller then
        // restores it.)
        let close = s[j + 1..].iter().position(|&c| c == b')')?;
        j = j + 1 + close + 1;
    }
    *i = j;
    Some(f64::NAN)
}

/// `parse_inf` of `real_policies` (`inf`, `infinity`, any case).
fn parse_inf(s: &[u8], i: &mut usize) -> Option<f64> {
    if !matches!(s.get(*i), Some(b'i' | b'I')) {
        return None;
    }
    let mut j = *i;
    if !string_parse(s, &mut j, b"inf") {
        return None;
    }
    string_parse(s, &mut j, b"inity");
    *i = j;
    Some(f64::INFINITY)
}

fn negate(neg: bool, n: f64) -> f64 {
    if neg { n.copysign(-1.0) } else { n }
}

/// `extract_int<int, 10, 1, -1>` (exponent): optional sign, digits, fails
/// (consuming nothing) on overflow of a 32-bit `int`.
fn parse_exp_n(s: &[u8], i: &mut usize) -> Option<i64> {
    let save = *i;
    if *i >= s.len() {
        return None;
    }
    let neg = parse_sign(s, i);
    let mut j = *i;
    let mut count = 0usize;
    while s.get(j) == Some(&b'0') {
        j += 1;
        count += 1;
    }
    let mut val: i64 = 0;
    while let Some(d) = s.get(j).copied().and_then(digit) {
        let d = i64::from(d);
        // Checked accumulation (equivalent to Spirit's count-based
        // checked/unchecked adds: results differ only on overflow).
        val = val * 10 + if neg { -d } else { d };
        if !(INT_MIN..=INT_MAX).contains(&val) {
            *i = save;
            return None;
        }
        j += 1;
        count += 1;
    }
    if count == 0 {
        *i = save;
        return None;
    }
    *i = j;
    Some(val)
}

// --- X3 (`x3::double_`, Boost 1.83) -----------------------------------------

/// `int_extractor::call` for `double` (`positive_accumulator<10>`): unchecked
/// for the first 14 counted digits, then checked against `DBL_MAX`.
fn x3_accumulate(n: &mut f64, d: u32, count: usize) -> bool {
    let d = f64::from(d);
    if count < 14 {
        *n = *n * 10.0 + d;
        return true;
    }
    let max = f64::MAX;
    if *n > max / 10.0 {
        return false;
    }
    let tmp = *n * 10.0;
    if tmp > max - d {
        return false;
    }
    *n = tmp + d;
    true
}

/// `extract_uint<double, 10, 1, -1, Accumulate>` (the `MinDigits = 1,
/// MaxDigits = -1` specialisation). On failure `i` and `attr` are unchanged.
fn x3_extract_uint(s: &[u8], i: &mut usize, attr: &mut f64, accumulate: bool) -> bool {
    let mut it = *i;
    let mut count = 0usize;
    if !accumulate {
        while s.get(it) == Some(&b'0') {
            it += 1;
            count += 1;
        }
        if it == s.len() {
            if count == 0 {
                return false;
            }
            *attr = 0.0;
            *i = it;
            return true;
        }
    }
    let mut val = if accumulate { *attr } else { 0.0 };
    let first_ok = match s.get(it).copied().and_then(digit) {
        Some(d) => x3_accumulate(&mut val, d, 0),
        None => false,
    };
    if !first_ok {
        if count == 0 {
            return false;
        }
        *attr = val;
        *i = it;
        return true;
    }
    count = 0;
    it += 1;
    while let Some(d) = s.get(it).copied().and_then(digit) {
        if !x3_accumulate(&mut val, d, count) {
            return false;
        }
        it += 1;
        count += 1;
    }
    *attr = val;
    *i = it;
    true
}

/// `extension::scale(exp, n)` of X3.
fn x3_scale(mut exp: i64, n: &mut f64) -> bool {
    if exp >= 0 {
        if exp > 308 {
            return false;
        }
        *n *= pow10(exp);
    } else if exp < -307 {
        *n /= pow10(307);
        exp += 307;
        if exp < -307 {
            return false;
        }
        *n /= pow10(-exp);
    } else {
        *n /= pow10(-exp);
    }
    true
}

/// `x3::double_` (Boost 1.83 `extract_real` with `real_policies<double>`),
/// starting at `start` (the caller has already skipped whitespace).
pub(crate) fn x3_real(s: &[u8], start: usize) -> Num {
    if start >= s.len() {
        return Num::Fail;
    }
    let mut i = start;
    let neg = parse_sign(s, &mut i);
    let mut n = 0.0f64;
    let got_a_number = x3_extract_uint(s, &mut i, &mut n, false);
    // Not a number: maybe NaN/Inf, else a leading dot (allow_leading_dot).
    if !got_a_number && let Some(v) = parse_nan(s, &mut i).or_else(|| parse_inf(s, &mut i)) {
        return Num::Ok(negate(neg, v), i);
    }
    let mut frac_digits: i64 = 0;
    if s.get(i) == Some(&b'.') {
        i += 1;
        let savef = i;
        if x3_extract_uint(s, &mut i, &mut n, true) {
            frac_digits = i64::try_from(i - savef).unwrap_or(i64::MAX);
        } else if !got_a_number {
            return Num::Fail; // neither digits nor a fraction
        }
    } else if !got_a_number {
        return Num::Fail; // no dot and no number
    }
    let e_pos = i;
    let e_hit = parse_exp(s, &mut i);
    if e_hit {
        if let Some(exp) = parse_exp_n(s, &mut i) {
            if !x3_scale(exp - frac_digits, &mut n) {
                return Num::FailAt(i);
            }
        } else {
            i = e_pos;
            if !x3_scale(-frac_digits, &mut n) {
                return Num::FailAt(i);
            }
        }
    } else if frac_digits != 0 && !x3_scale(-frac_digits, &mut n) {
        return Num::FailAt(i);
    }
    Num::Ok(negate(neg, n), i)
}

fn parse_exp(s: &[u8], i: &mut usize) -> bool {
    if matches!(s.get(*i), Some(b'e' | b'E')) {
        *i += 1;
        true
    } else {
        false
    }
}

// --- Qi (`qi::real_parser<double>`, Boost 1.83) ----------------------------

/// `max_digits10<double>` = 2 + 53·30103/100000 = 17.
const QI_MAX_INT_DIGITS: usize = 17;

/// `ureal_policies::parse_n`: `extract_uint<uint64_t, 10, 1, 17, false,
/// true>` — at most 17 digits *including skipped leading zeros*.
fn qi_parse_n(s: &[u8], i: &mut usize, acc: &mut u64) -> bool {
    let mut it = *i;
    let mut leading_zeros = 0usize;
    while s.get(it) == Some(&b'0') && leading_zeros < QI_MAX_INT_DIGITS {
        it += 1;
        leading_zeros += 1;
    }
    let mut val: u64 = 0;
    let mut count = 0usize;
    while count + leading_zeros < QI_MAX_INT_DIGITS {
        let Some(d) = s.get(it).copied().and_then(digit) else {
            break;
        };
        val = val * 10 + u64::from(d);
        it += 1;
        count += 1;
    }
    if count + leading_zeros >= 1 {
        *acc = val;
        *i = it;
        true
    } else {
        false
    }
}

/// `ignore_excess_digits`: number of consecutive digits skipped.
fn qi_ignore_digits(s: &[u8], i: &mut usize) -> i64 {
    let start = *i;
    while s.get(*i).is_some_and(u8::is_ascii_digit) {
        *i += 1;
    }
    i64::try_from(*i - start).unwrap_or(i64::MAX)
}

/// `ureal_policies::parse_frac_n`: accumulate into the `uint64_t` mantissa,
/// stop (successfully) at the first digit that would overflow, then skip the
/// remaining (non-significant) digits.
fn qi_parse_frac(s: &[u8], i: &mut usize, acc: &mut u64, frac_digits: &mut i64) -> bool {
    let savef = *i;
    let mut it = *i;
    let mut val = *acc;
    let mut count = 0usize;
    let mut stopped = false;
    while let Some(d) = s.get(it).copied().and_then(digit) {
        let d = u64::from(d);
        // checked add (AlwaysCheckOverflow)
        if val > u64::MAX / 10 || val * 10 > u64::MAX - d {
            stopped = true;
            break;
        }
        val = val * 10 + d;
        it += 1;
        count += 1;
    }
    if !stopped && count == 0 {
        return false;
    }
    *acc = val;
    *i = it;
    *frac_digits = i64::try_from(it - savef).unwrap_or(i64::MAX);
    qi_ignore_digits(s, i);
    true
}

/// `traits::scale(exp, n, acc_n)` of Qi.
fn qi_scale(mut exp: i64, n: &mut f64, acc: u64) -> bool {
    #[allow(clippy::cast_precision_loss)] // u64 -> double conversion, as upstream
    let accf = acc as f64;
    if exp >= 0 {
        if exp > 308 {
            return false;
        }
        *n = accf * pow10(exp);
    } else if exp < -307 {
        // compensate_roundoff for an integral accumulator
        #[allow(clippy::cast_precision_loss)]
        {
            *n = ((acc / 10) * 10) as f64;
            *n += (acc % 10) as f64;
        }
        *n /= pow10(307);
        exp += 307;
        if exp < -307 {
            return false;
        }
        *n /= pow10(-exp);
    } else {
        *n = accf / pow10(-exp);
    }
    true
}

/// `qi::real_parser<double, qi::real_policies<double>>` (Boost 1.83
/// `real_impl::parse`), starting at `start` (whitespace already skipped).
pub(crate) fn qi_real(s: &[u8], start: usize) -> Num {
    if start >= s.len() {
        return Num::Fail;
    }
    let mut i = start;
    let neg = parse_sign(s, &mut i);
    let mut acc: u64 = 0;
    let mut n = 0.0f64;
    let got_a_number = qi_parse_n(s, &mut i, &mut acc);
    let mut excess_n: i64 = 0;
    if got_a_number {
        excess_n = qi_ignore_digits(s, &mut i);
    } else if let Some(v) = parse_nan(s, &mut i).or_else(|| parse_inf(s, &mut i)) {
        return Num::Ok(negate(neg, v), i);
    }
    let mut frac_digits: i64 = 0;
    if s.get(i) == Some(&b'.') {
        i += 1;
        if excess_n != 0 {
            // Fraction digits are skipped once the integer part overflowed.
            qi_ignore_digits(s, &mut i);
        } else if !qi_parse_frac(s, &mut i, &mut acc, &mut frac_digits) && !got_a_number {
            return Num::Fail; // neither digits nor a fraction
        }
    } else if !got_a_number {
        return Num::Fail; // no dot and no number
    }
    let e_pos = i;
    let e_hit = parse_exp(s, &mut i);
    if e_hit {
        if let Some(exp) = parse_exp_n(s, &mut i) {
            if !qi_scale(exp + excess_n - frac_digits, &mut n, acc) {
                return Num::FailAt(i);
            }
        } else {
            i = e_pos;
            // BOOST_VERIFY: result ignored in NDEBUG builds.
            let _ = qi_scale(-frac_digits, &mut n, acc);
        }
    } else if frac_digits != 0 {
        let _ = qi_scale(-frac_digits, &mut n, acc);
    } else if excess_n != 0 {
        if !qi_scale(excess_n, &mut n, acc) {
            return Num::FailAt(i);
        }
    } else {
        #[allow(clippy::cast_precision_loss)]
        {
            n = acc as f64;
        }
    }
    Num::Ok(negate(neg, n), i)
}

// ---------------------------------------------------------------------------
// Symbol tables
// ---------------------------------------------------------------------------

/// Longest key of `table` that is a prefix of `s[at..]` (Spirit `symbols`
/// TST lookup); `no_case` compares the input lower-cased (keys are lower
/// case).
pub(crate) fn longest_match<T: Copy>(
    table: &[(&str, T)],
    s: &[u8],
    at: usize,
    no_case: bool,
) -> Option<(T, usize)> {
    let rest = s.get(at..)?;
    let mut best: Option<(T, usize)> = None;
    for &(key, value) in table {
        let k = key.as_bytes();
        if rest.len() < k.len() {
            continue;
        }
        let head = &rest[..k.len()];
        let matches = if no_case {
            head.iter()
                .zip(k)
                .all(|(a, b)| a.to_ascii_lowercase() == *b)
        } else {
            head == k
        };
        if matches && best.is_none_or(|(_, l)| k.len() > l) {
            best = Some((value, k.len()));
        }
    }
    best
}

fn is_key_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_key_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

// ---------------------------------------------------------------------------
// Evaluator
// ---------------------------------------------------------------------------

/// Distribution keywords of `RandomGeneratorGrammar` (case-sensitive).
pub(crate) static DISTRIBUTIONS: &[(&str, DistributionKind)] = &[
    ("uniform", DistributionKind::Uniform),
    ("exponential", DistributionKind::Exponential),
    ("gamma", DistributionKind::Gamma),
    ("weibull", DistributionKind::Weibull),
    ("extreme_value", DistributionKind::ExtremeValue),
    ("normal", DistributionKind::Normal),
    ("lognormal", DistributionKind::Lognormal),
    ("chi_squared", DistributionKind::ChiSquared),
    ("cauchy", DistributionKind::Cauchy),
    ("fisher_f", DistributionKind::FisherF),
    ("student_t", DistributionKind::StudentT),
];

/// Upstream `RandomDistributionType` (without `_not_defined`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DistributionKind {
    Uniform,
    Exponential,
    Gamma,
    Weibull,
    ExtremeValue,
    Normal,
    Lognormal,
    ChiSquared,
    Cauchy,
    FisherF,
    StudentT,
}

/// Upstream `ParsableRandomGenerator`.
#[derive(Clone, Debug, PartialEq)]
pub(crate) struct ParsedRandom {
    pub(crate) base: f64,
    pub(crate) kind: Option<DistributionKind>,
    pub(crate) parameters: Vec<f64>,
}

/// One text being evaluated: the converted value itself, or the value of a
/// referenced variable (always the X3 dialect, in the scope that holds it).
#[derive(Clone, Copy)]
struct Ctx<'x> {
    s: &'x [u8],
    pos: usize,
    dialect: Dialect,
    scope: ScopeRef<'x>,
}

/// A grammar rule that can be "called".
#[derive(Clone, Copy)]
enum Rule {
    Expr,
    Term,
    Factor,
    Number,
}

/// A rule activation waiting for the result of the rule it called. The
/// evaluator keeps these on an explicit heap stack instead of the native
/// call stack, so nesting is bounded only by memory.
enum Cont<'x> {
    /// `expr = term …`: after the first term.
    ExprFirst,
    /// `expr = … (('+' | '-') term)*`: after a further term.
    ExprRest { acc: f64, add: bool },
    /// `term = factor …`: after the first factor.
    TermFirst,
    /// `term = … (('*' | '/') factor)*`: after a further factor.
    TermRest { acc: f64, mul: bool },
    /// `factor = number …`: after the first number.
    FactorFirst,
    /// `factor = … ("**" number)*`: after a further number.
    FactorRest { acc: f64 },
    /// `number`: restores the position when no alternative matched.
    Number { entry: usize },
    /// `'-' number | '+' number`.
    Sign { negative: bool },
    /// `'(' expr ')'`.
    Parenthesis,
    /// `unary '(' expr ')'`.
    Unary { f: Unary, name: String },
    /// `binary '(' expr ',' expr ')'`: after the first argument.
    Binary1 { f: Binary, name: String },
    /// `binary '(' expr ',' expr ')'`: after the second argument.
    Binary2 { f: Binary, a: f64, name: String },
    /// A variable reference: after the evaluation of its value (in its own
    /// context); `outer` is the context to resume.
    Variable {
        start: usize,
        param: &'x ParserParameter,
        outer: Ctx<'x>,
    },
}

/// What the machine does next.
enum Step {
    Call(Rule),
    Return(R<f64>),
}

struct Eval<'x> {
    ctx: Ctx<'x>,
    /// Parameters whose value is being evaluated (by address): a reference
    /// to one of them is a cycle.
    active: HashSet<*const ParserParameter>,
    /// Context of an error unwinding through variable references, innermost
    /// first.
    wraps: Vec<String>,
}

/// Contexts shown in full in a message before eliding the middle of a long
/// chain of variable references.
const MAX_SHOWN_CONTEXTS: usize = 16;

impl<'x> Eval<'x> {
    fn new(text: &'x [u8], dialect: Dialect, scope: ScopeRef<'x>) -> Self {
        Self {
            ctx: Ctx {
                s: text,
                pos: 0,
                dialect,
                scope,
            },
            active: HashSet::new(),
            wraps: Vec::new(),
        }
    }

    fn peek(&self) -> Option<u8> {
        self.ctx.s.get(self.ctx.pos).copied()
    }

    fn skip_ws(&mut self) {
        while self.peek().is_some_and(is_c_space) {
            self.ctx.pos += 1;
        }
    }

    /// Current position after whitespace (for messages).
    fn error_pos(&self) -> usize {
        let s = self.ctx.s;
        let mut p = self.ctx.pos.min(s.len());
        while p < s.len() && is_c_space(s[p]) {
            p += 1;
        }
        p
    }

    fn here(&self) -> String {
        let p = self.error_pos();
        format!(
            "'{}<HERE>{}'",
            String::from_utf8_lossy(&self.ctx.s[..p]),
            String::from_utf8_lossy(&self.ctx.s[p..])
        )
    }

    /// A failed `>` (expectation) in the current dialect.
    fn expectation(&self, what: &str) -> ConversionError {
        let msg = format!("expecting {what} at {}", self.here());
        let at = self.error_pos();
        match self.ctx.dialect {
            Dialect::X3 => ConversionError::hard(msg, at),
            // Qi grammars install an `on_error` handler that turns
            // expectation failures into `akantu::debug::Exception`.
            Dialect::Qi => ConversionError::soft(msg, at),
        }
    }

    fn fail(&self, what: &str) -> Step {
        Step::Return(Err(self.expectation(what)))
    }

    fn expect_char(&mut self, c: u8, what: &str) -> Result<(), ConversionError> {
        self.skip_ws();
        if self.peek() == Some(c) {
            self.ctx.pos += 1;
            Ok(())
        } else {
            Err(self.expectation(what))
        }
    }

    /// Runs the machine from `rule` in the current context.
    fn run(&mut self, rule: Rule) -> R<f64> {
        let mut stack: Vec<Cont<'x>> = Vec::new();
        let mut step = Step::Call(rule);
        loop {
            step = match step {
                Step::Call(rule) => self.call(rule, &mut stack),
                Step::Return(r) => match stack.pop() {
                    Some(cont) => self.resume(cont, r, &mut stack),
                    None => return self.with_contexts(r),
                },
            };
        }
    }

    fn expr(&mut self) -> R<f64> {
        self.run(Rule::Expr)
    }

    /// Prefixes the message of an error that unwound through variable
    /// references with their contexts (outermost first).
    fn with_contexts(&mut self, r: R<f64>) -> R<f64> {
        if self.wraps.is_empty() {
            return r;
        }
        let mut wraps = std::mem::take(&mut self.wraps);
        r.map_err(|e| {
            wraps.reverse();
            if wraps.len() > MAX_SHOWN_CONTEXTS {
                let n = wraps.len() - MAX_SHOWN_CONTEXTS;
                let tail = wraps.split_off(wraps.len() - MAX_SHOWN_CONTEXTS / 2);
                wraps.truncate(MAX_SHOWN_CONTEXTS / 2);
                wraps.push(format!("… {n} more variables …"));
                wraps.extend(tail);
            }
            ConversionError {
                message: format!("{}: {}", wraps.join(": "), e.message),
                ..e
            }
        })
    }

    fn call(&mut self, rule: Rule, stack: &mut Vec<Cont<'x>>) -> Step {
        match rule {
            Rule::Expr => {
                stack.push(Cont::ExprFirst);
                Step::Call(Rule::Term)
            }
            Rule::Term => {
                stack.push(Cont::TermFirst);
                Step::Call(Rule::Factor)
            }
            Rule::Factor => {
                stack.push(Cont::FactorFirst);
                Step::Call(Rule::Number)
            }
            Rule::Number => {
                stack.push(Cont::Number {
                    entry: self.ctx.pos,
                });
                self.number(stack)
            }
        }
    }

    /// `number`: the first alternative whose prefix matches is committed.
    fn number(&mut self, stack: &mut Vec<Cont<'x>>) -> Step {
        self.skip_ws();
        let lit = match self.ctx.dialect {
            Dialect::X3 => x3_real(self.ctx.s, self.ctx.pos),
            Dialect::Qi => qi_real(self.ctx.s, self.ctx.pos),
        };
        match lit {
            Num::Ok(v, end) => {
                self.ctx.pos = end;
                return Step::Return(Ok(Some(v)));
            }
            Num::Fail => {}
            Num::FailAt(p) => self.ctx.pos = p,
        }
        self.skip_ws();
        if let Some(c @ (b'-' | b'+')) = self.peek() {
            self.ctx.pos += 1;
            stack.push(Cont::Sign {
                negative: c == b'-',
            });
            return Step::Call(Rule::Number);
        }
        match self.ctx.dialect {
            Dialect::X3 => {
                if let Some(step) = self.function(stack) {
                    return step;
                }
                if let Some(v) = self.constant() {
                    return Step::Return(Ok(Some(v)));
                }
                if let Some(step) = self.variable(stack) {
                    return step;
                }
                self.parenthesis(stack).unwrap_or(Step::Return(Ok(None)))
            }
            Dialect::Qi => {
                if let Some(v) = self.constant() {
                    return Step::Return(Ok(Some(v)));
                }
                if let Some(step) = self.function(stack) {
                    return step;
                }
                if let Some(step) = self.parenthesis(stack) {
                    return step;
                }
                self.variable(stack).unwrap_or(Step::Return(Ok(None)))
            }
        }
    }

    fn constant(&mut self) -> Option<f64> {
        self.skip_ws();
        let (v, len) = longest_match(CONSTANTS, self.ctx.s, self.ctx.pos, false)?;
        self.ctx.pos += len;
        Some(v)
    }

    fn parenthesis(&mut self, stack: &mut Vec<Cont<'x>>) -> Option<Step> {
        self.skip_ws();
        if self.peek() != Some(b'(') {
            return None;
        }
        self.ctx.pos += 1;
        stack.push(Cont::Parenthesis);
        Some(Step::Call(Rule::Expr))
    }

    /// X3 tries binary functions before unary ones (case-sensitive), Qi the
    /// reverse (case-insensitive).
    fn function(&mut self, stack: &mut Vec<Cont<'x>>) -> Option<Step> {
        match self.ctx.dialect {
            Dialect::X3 => self
                .binary(false, stack)
                .or_else(|| self.unary(false, stack)),
            Dialect::Qi => self.unary(true, stack).or_else(|| self.binary(true, stack)),
        }
    }

    /// Matches a function name (`None` if there is none) and its `(`.
    fn function_name<F: Copy>(
        &mut self,
        table: &[(&str, F)],
        no_case: bool,
    ) -> Option<(F, String)> {
        self.skip_ws();
        let (f, len) = longest_match(table, self.ctx.s, self.ctx.pos, no_case)?;
        let p = self.ctx.pos;
        let name = String::from_utf8_lossy(&self.ctx.s[p..p + len]).into_owned();
        self.ctx.pos += len;
        Some((f, name))
    }

    fn unary(&mut self, no_case: bool, stack: &mut Vec<Cont<'x>>) -> Option<Step> {
        let (f, name) = self.function_name::<Unary>(UNARY, no_case)?;
        if let Err(e) = self.expect_char(b'(', &format!("'(' after function '{name}'")) {
            return Some(Step::Return(Err(e)));
        }
        stack.push(Cont::Unary { f, name });
        Some(Step::Call(Rule::Expr))
    }

    fn binary(&mut self, no_case: bool, stack: &mut Vec<Cont<'x>>) -> Option<Step> {
        let (f, name) = self.function_name::<Binary>(BINARY, no_case)?;
        if let Err(e) = self.expect_char(b'(', &format!("'(' after function '{name}'")) {
            return Some(Step::Return(Err(e)));
        }
        stack.push(Cont::Binary1 { f, name });
        Some(Step::Call(Rule::Expr))
    }

    /// `key[eval]`: a contiguous identifier, looked up in the current and
    /// parent scopes and evaluated (X3) in the scope that defines it.
    /// Upstream recurses without bound; a reference to a parameter that is
    /// already being evaluated can only recurse forever (the evaluation is
    /// deterministic), so it is reported as a cyclic reference.
    fn variable(&mut self, stack: &mut Vec<Cont<'x>>) -> Option<Step> {
        let start = self.ctx.pos;
        if !self.peek().is_some_and(is_key_start) {
            return None;
        }
        self.ctx.pos += 1;
        while self.peek().is_some_and(is_key_char) {
            self.ctx.pos += 1;
        }
        let name = &self.ctx.s[start..self.ctx.pos];
        let Some((param, scope)) = self.ctx.scope.lookup(name, SearchScope::CurrentAndParent)
        else {
            return Some(Step::Return(Err(ConversionError::soft(
                format!(
                    "the parameter '{}' has not been found in the specified context (at {})",
                    String::from_utf8_lossy(name),
                    self.here()
                ),
                start,
            ))));
        };
        if !self.active.insert(std::ptr::from_ref(param)) {
            return Some(Step::Return(Err(ConversionError {
                cyclic: true,
                ..ConversionError::hard(
                    format!(
                        "cyclic variable reference: the value of '{}' depends on itself (at {})",
                        String::from_utf8_lossy(name),
                        self.here()
                    ),
                    start,
                )
            })));
        }
        stack.push(Cont::Variable {
            start,
            param,
            outer: self.ctx,
        });
        self.ctx = Ctx {
            s: param.value.as_bytes(),
            pos: 0,
            dialect: Dialect::X3,
            scope,
        };
        Some(Step::Call(Rule::Expr))
    }

    fn expr_loop(&mut self, v: f64, stack: &mut Vec<Cont<'x>>) -> Step {
        let save = self.ctx.pos;
        self.skip_ws();
        match self.peek() {
            Some(c @ (b'+' | b'-')) => {
                self.ctx.pos += 1;
                stack.push(Cont::ExprRest {
                    acc: v,
                    add: c == b'+',
                });
                Step::Call(Rule::Term)
            }
            _ => {
                self.ctx.pos = save;
                Step::Return(Ok(Some(v)))
            }
        }
    }

    fn term_loop(&mut self, v: f64, stack: &mut Vec<Cont<'x>>) -> Step {
        let save = self.ctx.pos;
        self.skip_ws();
        match self.peek() {
            Some(c @ (b'*' | b'/')) => {
                self.ctx.pos += 1;
                stack.push(Cont::TermRest {
                    acc: v,
                    mul: c == b'*',
                });
                Step::Call(Rule::Factor)
            }
            _ => {
                self.ctx.pos = save;
                Step::Return(Ok(Some(v)))
            }
        }
    }

    fn factor_loop(&mut self, v: f64, stack: &mut Vec<Cont<'x>>) -> Step {
        let save = self.ctx.pos;
        self.skip_ws();
        if self.ctx.s[self.ctx.pos..].starts_with(b"**") {
            self.ctx.pos += 2;
            stack.push(Cont::FactorRest { acc: v });
            Step::Call(Rule::Number)
        } else {
            self.ctx.pos = save;
            Step::Return(Ok(Some(v)))
        }
    }

    fn resume(&mut self, cont: Cont<'x>, r: R<f64>, stack: &mut Vec<Cont<'x>>) -> Step {
        if let Cont::Variable {
            start,
            param,
            outer,
        } = cont
        {
            let r = r.and_then(|v| self.finish(v, "a Real expression"));
            self.ctx = outer;
            self.active.remove(&std::ptr::from_ref(param));
            return Step::Return(match r {
                Ok(v) => Ok(Some(v)),
                Err(e) => {
                    self.wraps
                        .push(format!("in variable '{}' = '{}'", param.name, param.value));
                    Err(ConversionError { offset: start, ..e })
                }
            });
        }
        if let Cont::Number { entry } = cont {
            if matches!(r, Ok(None)) {
                self.ctx.pos = entry;
            }
            return Step::Return(r);
        }
        let v = match r {
            Ok(Some(v)) => v,
            // Errors propagate unchanged; `None` ends the first operand of a
            // rule (no match) or is a failed expectation.
            Err(e) => return Step::Return(Err(e)),
            Ok(None) => {
                return match cont {
                    Cont::ExprFirst | Cont::TermFirst | Cont::FactorFirst => Step::Return(Ok(None)),
                    Cont::ExprRest { .. } => self.fail("a term"),
                    Cont::TermRest { .. } => self.fail("a factor"),
                    Cont::FactorRest { .. } => self.fail("a number after '**'"),
                    Cont::Sign { negative: true } => self.fail("a number after '-'"),
                    Cont::Sign { negative: false } => self.fail("a number after '+'"),
                    Cont::Parenthesis => self.fail("an expression after '('"),
                    Cont::Unary { name, .. } => self.fail(&format!("the argument of '{name}'")),
                    Cont::Binary1 { name, .. } => {
                        self.fail(&format!("the first argument of '{name}'"))
                    }
                    Cont::Binary2 { name, .. } => {
                        self.fail(&format!("the second argument of '{name}'"))
                    }
                    Cont::Number { .. } | Cont::Variable { .. } => unreachable!("handled above"),
                };
            }
        };
        match cont {
            Cont::ExprFirst => self.expr_loop(v, stack),
            Cont::ExprRest { acc, add } => {
                self.expr_loop(if add { acc + v } else { acc - v }, stack)
            }
            Cont::TermFirst => self.term_loop(v, stack),
            Cont::TermRest { acc, mul } => {
                self.term_loop(if mul { acc * v } else { acc / v }, stack)
            }
            Cont::FactorFirst => self.factor_loop(v, stack),
            Cont::FactorRest { acc } => self.factor_loop(acc.powf(v), stack),
            Cont::Sign { negative } => Step::Return(Ok(Some(if negative { -v } else { v }))),
            Cont::Parenthesis => match self.expect_char(b')', "')'") {
                Ok(()) => Step::Return(Ok(Some(v))),
                Err(e) => Step::Return(Err(e)),
            },
            Cont::Unary { f, .. } => match self.expect_char(b')', "')'") {
                Ok(()) => Step::Return(Ok(Some(f(v)))),
                Err(e) => Step::Return(Err(e)),
            },
            Cont::Binary1 { f, name } => {
                if let Err(e) =
                    self.expect_char(b',', &format!("',' between the arguments of '{name}'"))
                {
                    return Step::Return(Err(e));
                }
                stack.push(Cont::Binary2 { f, a: v, name });
                Step::Call(Rule::Expr)
            }
            Cont::Binary2 { f, a, .. } => match self.expect_char(b')', "')'") {
                Ok(()) => Step::Return(Ok(Some(f(a, v)))),
                Err(e) => Step::Return(Err(e)),
            },
            Cont::Number { .. } | Cont::Variable { .. } => unreachable!("handled above"),
        }
    }

    /// Qi `VectorGrammar`: `'[' > (number >> *(',' >> number)) > ']'`.
    fn vector(&mut self) -> R<Vec<f64>> {
        self.skip_ws();
        if self.peek() != Some(b'[') {
            return Ok(None);
        }
        self.ctx.pos += 1;
        let first = self
            .expr()?
            .ok_or_else(|| self.expectation("a vector entry"))?;
        let mut cells = vec![first];
        loop {
            let save = self.ctx.pos;
            self.skip_ws();
            if self.peek() != Some(b',') {
                self.ctx.pos = save;
                break;
            }
            self.ctx.pos += 1;
            match self.expr()? {
                Some(v) => cells.push(v),
                None => {
                    self.ctx.pos = save;
                    break;
                }
            }
        }
        self.expect_char(b']', "',' or ']' in vector")?;
        Ok(Some(cells))
    }

    /// Qi `MatrixGrammar`: `'[' >> rows >> *(',' >> rows) >> ']'`.
    fn matrix(&mut self) -> R<Vec<Vec<f64>>> {
        self.skip_ws();
        if self.peek() != Some(b'[') {
            return Ok(None);
        }
        self.ctx.pos += 1;
        let Some(first) = self.matrix_row()? else {
            return Ok(None);
        };
        let mut rows = vec![first];
        loop {
            let save = self.ctx.pos;
            self.skip_ws();
            if self.peek() != Some(b',') {
                self.ctx.pos = save;
                break;
            }
            self.ctx.pos += 1;
            match self.matrix_row()? {
                Some(r) => rows.push(r),
                None => {
                    self.ctx.pos = save;
                    break;
                }
            }
        }
        self.skip_ws();
        if self.peek() != Some(b']') {
            return Ok(None);
        }
        self.ctx.pos += 1;
        Ok(Some(rows))
    }

    /// `rows = eval_vector | vector`. `eval_vector` reads an identifier
    /// **with whitespace skipping between its characters** (the Qi `key`
    /// rule of `MatrixGrammar` is not a lexeme), looks it up (an error if it
    /// does not exist) and parses the **first word** of its value as a
    /// vector prefix in the scope of the *matrix* parameter.
    fn matrix_row(&mut self) -> R<Vec<f64>> {
        let save = self.ctx.pos;
        self.skip_ws();
        if self.peek().is_some_and(is_key_start) {
            let start = self.ctx.pos;
            let mut name = vec![self.ctx.s[self.ctx.pos]];
            self.ctx.pos += 1;
            loop {
                self.skip_ws();
                match self.peek() {
                    Some(c) if is_key_char(c) => {
                        name.push(c);
                        self.ctx.pos += 1;
                    }
                    _ => break,
                }
            }
            let Some((param, _)) = self.ctx.scope.lookup(&name, SearchScope::CurrentAndParent)
            else {
                return Err(ConversionError::soft(
                    format!(
                        "the parameter '{}' has not been found in the specified context (at {})",
                        String::from_utf8_lossy(&name),
                        self.here()
                    ),
                    start,
                ));
            };
            // `std::string value = section.getParameter(...)` uses the stream
            // conversion: only the first whitespace-delimited word is parsed
            // (so `v = [1, 2]` cannot be used as a row, `v = [1,2]` can).
            let word = crate::stream::word_bytes(param.value.as_bytes());
            let mut sub = Eval::new(word, Dialect::Qi, self.ctx.scope);
            match sub.vector() {
                Ok(Some(row)) => return Ok(Some(row)),
                Ok(None) => {}
                Err(e) => {
                    return Err(ConversionError {
                        offset: start,
                        ..e.context(&format!(
                            "in matrix row '{}' = '{}'",
                            String::from_utf8_lossy(&name),
                            param.value
                        ))
                    });
                }
            }
        }
        self.ctx.pos = save;
        self.vector()
    }

    /// Qi `RandomGeneratorGrammar`:
    /// `hold[number >> type >> '[' >> (number >> *(',' > number)) >> ']'] | number`.
    fn random(&mut self) -> R<ParsedRandom> {
        let save = self.ctx.pos;
        if let Some(base) = self.expr()? {
            self.skip_ws();
            if let Some((kind, len)) = longest_match(DISTRIBUTIONS, self.ctx.s, self.ctx.pos, false)
            {
                self.ctx.pos += len;
                self.skip_ws();
                if self.peek() == Some(b'[') {
                    self.ctx.pos += 1;
                    if let Some(p0) = self.expr()? {
                        let mut parameters = vec![p0];
                        loop {
                            self.skip_ws();
                            if self.peek() != Some(b',') {
                                break;
                            }
                            self.ctx.pos += 1;
                            let p = self
                                .expr()?
                                .ok_or_else(|| self.expectation("a distribution parameter"))?;
                            parameters.push(p);
                        }
                        self.skip_ws();
                        if self.peek() == Some(b']') {
                            self.ctx.pos += 1;
                            return Ok(Some(ParsedRandom {
                                base,
                                kind: Some(kind),
                                parameters,
                            }));
                        }
                    }
                }
            }
        }
        self.ctx.pos = save;
        Ok(self.expr()?.map(|base| ParsedRandom {
            base,
            kind: None,
            parameters: Vec::new(),
        }))
    }

    /// `parseType`/`parse_real`: the whole text (up to trailing whitespace)
    /// must be consumed.
    fn finish<T>(&mut self, r: Option<T>, what: &str) -> Result<T, ConversionError> {
        self.skip_ws();
        match r {
            Some(v) if self.ctx.pos == self.ctx.s.len() => Ok(v),
            _ => Err(ConversionError::soft(
                format!(
                    "could not parse '{}' as {what} (at {})",
                    String::from_utf8_lossy(self.ctx.s),
                    self.here()
                ),
                self.error_pos(),
            )),
        }
    }
}

/// Upstream `Parser::parseReal` (`parser::algebraic::parse_real`, X3).
pub(crate) fn eval_real<'x>(text: &'x [u8], scope: ScopeRef<'x>) -> Result<f64, ConversionError> {
    let mut e = Eval::new(text, Dialect::X3, scope);
    let r = e.expr()?;
    e.finish(r, "a Real expression")
}

/// Upstream `Parser::parseVector` (`VectorGrammar`, Qi).
pub(crate) fn eval_vector<'x>(
    text: &'x [u8],
    scope: ScopeRef<'x>,
) -> Result<Vec<f64>, ConversionError> {
    let mut e = Eval::new(text, Dialect::Qi, scope);
    let r = e.vector()?;
    e.finish(r, "a vector")
}

/// Upstream `Parser::parseMatrix` (`MatrixGrammar`, Qi): rows in order.
pub(crate) fn eval_matrix<'x>(
    text: &'x [u8],
    scope: ScopeRef<'x>,
) -> Result<Vec<Vec<f64>>, ConversionError> {
    let mut e = Eval::new(text, Dialect::Qi, scope);
    let r = e.matrix()?;
    e.finish(r, "a matrix")
}

/// Upstream `Parser::parseRandomParameter` (`RandomGeneratorGrammar`, Qi).
pub(crate) fn eval_random<'x>(
    text: &'x [u8],
    scope: ScopeRef<'x>,
) -> Result<ParsedRandom, ConversionError> {
    let mut e = Eval::new(text, Dialect::Qi, scope);
    let r = e.random()?;
    e.finish(r, "a random parameter")
}

/// A single Qi `AlgebraicGrammar` expression (used by tests to compare the
/// two dialects).
#[cfg(test)]
pub(crate) fn eval_real_qi<'x>(
    text: &'x [u8],
    scope: ScopeRef<'x>,
) -> Result<f64, ConversionError> {
    let mut e = Eval::new(text, Dialect::Qi, scope);
    let r = e.expr()?;
    e.finish(r, "a Real expression")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section::{ParserParameter, ParserSection, ParserType};

    fn section() -> ParserSection {
        let mut s = ParserSection::new(ParserType::Global, "global");
        for (n, v) in [
            ("general", "50"),
            ("E", "2"),
            ("v1", "[1,2]"),
            ("v2", "[1, 2]"),
            ("cosine", "1"),
            ("cyc", "cyc + 1"),
            ("bad", "1 +"),
        ] {
            s.parameters.push(ParserParameter::new(n, v));
        }
        s
    }

    fn x3(t: &str) -> Result<f64, ConversionError> {
        let s = section();
        eval_real(t.as_bytes(), ScopeRef::Section(&s))
    }

    fn qi(t: &str) -> Result<f64, ConversionError> {
        let s = section();
        eval_real_qi(t.as_bytes(), ScopeRef::Section(&s))
    }

    #[test]
    fn literals() {
        // Upstream gtest AlgebraicParser.Number
        assert_eq!(x3("3.").unwrap(), 3.0);
        assert_eq!(x3("10").unwrap(), 10.0);
        assert_eq!(x3("-100.0").unwrap(), -100.0);
        assert_eq!(x3("+1e10").unwrap(), 1e10);
        assert_eq!(x3(".5").unwrap(), 0.5);
        assert_eq!(x3("1.e5").unwrap(), 1e5);
        assert_eq!(x3("0.1").unwrap().to_bits(), 0.1f64.to_bits());
        assert!(x3("-0").unwrap().is_sign_negative());
        assert!(x3(".").is_err());
        assert!(x3("2e").is_err()); // exponent marker without digits is not consumed
        assert_eq!(x3("2e+3").unwrap(), 2000.0);
        assert_eq!(x3("inf").unwrap(), f64::INFINITY);
        assert_eq!(x3("-Infinity").unwrap(), f64::NEG_INFINITY);
        assert!(x3("NaN").unwrap().is_nan());
        assert!(x3("nan(abc)").unwrap().is_nan());
        assert!(x3("nan(abc").is_err());
        assert!(x3("info").is_err());
        // Out-of-range decimal exponent: the literal fails *after* consuming
        // its text, the next `number` alternative continues there.
        assert!(x3("1e400").is_err());
        assert_eq!(x3("1e400 + 1").unwrap(), 1.0);
        assert_eq!(x3("1e400-2").unwrap(), -2.0);
        assert_eq!(qi("1e400 + 1").unwrap(), 1.0);
        assert_eq!(x3("1e308").unwrap(), 1e308);
        assert_eq!(x3("10e308").unwrap(), f64::INFINITY);
        assert_eq!(x3("1e-320").unwrap(), 1e-320);
        assert!(x3("0e999").is_err());
        assert!(x3("1e2147483648").is_err());
        // 17 leading zeros: Qi stops counting digits.
        let z = format!("{}1234", "0".repeat(17));
        assert_eq!(x3(&z).unwrap(), 1234.0);
        assert_eq!(qi(&z).unwrap(), 0.0);
        let z = format!("{}.5", "0".repeat(20));
        assert_eq!(x3(&z).unwrap(), 0.5);
        assert_eq!(qi(&z).unwrap(), 0.0);
    }

    #[test]
    fn operators() {
        // Upstream gtest AlgebraicParser.Expr/Term/Factor/Complex
        assert_eq!(x3("3. + 10.").unwrap(), 13.0);
        assert_eq!(x3("10 - 100").unwrap(), -90.0);
        assert_eq!(x3("-1 - -1").unwrap(), 0.0);
        assert_eq!(
            x3("1e10 + 4e5 - 1e9 + 1e10").unwrap(),
            1e10 + 4e5 - 1e9 + 1e10
        );
        assert_eq!(x3("3. * 10.").unwrap(), 30.0);
        assert_eq!(x3("-1 / -1").unwrap(), 1.0);
        assert_eq!(x3("1e10 / 4e5").unwrap(), 1e10 / 4e5);
        assert_eq!(x3("3. ** 10.").unwrap(), 3f64.powf(10.0));
        assert_eq!(x3("-2**2").unwrap(), 4.0);
        assert_eq!(x3("- 2**2").unwrap(), 4.0);
        assert_eq!(x3("2**3**2").unwrap(), 64.0);
        assert_eq!(x3("2**-1").unwrap(), 0.5);
        assert_eq!(x3("2 - - 3").unwrap(), 5.0);
        assert_eq!(x3("1 - 2 - 3").unwrap(), -4.0);
        assert_eq!(x3("8 / 4 / 2").unwrap(), 1.0);
        let pi = std::f64::consts::PI;
        assert_eq!(
            x3("10. * (cos(3 * pi) + sin(5 * pi / 2)) ** 2").unwrap(),
            10. * ((3.0 * pi).cos() + (5.0 * pi / 2.0).sin()).powf(2.0)
        );
        assert_eq!(
            x3("pi * 0.05**2 / 4").unwrap(),
            (pi * 0.05f64.powf(2.0)) / 4.0
        );
        assert!(x3("2 * * 3").unwrap_err().is_hard());
        assert!(!qi("2 * * 3").unwrap_err().is_hard());
        assert!(x3("(1 + 2").unwrap_err().is_hard());
        assert!(!x3("1 2").unwrap_err().is_hard());
        assert!(!x3("").unwrap_err().is_hard());
    }

    #[test]
    fn functions_and_dialects() {
        assert_eq!(x3("max(2, general)").unwrap(), 50.0);
        assert_eq!(x3("pow(3., 10.)").unwrap(), 3f64.powf(10.0));
        assert_eq!(x3("atan2(1, 2)").unwrap(), 1f64.atan2(2.0));
        assert_eq!(x3("log10(1000)").unwrap(), 3.0);
        assert_eq!(x3("exp2(3)").unwrap(), 8.0);
        assert_eq!(x3("fmod(10., 3.)").unwrap(), 1.0);
        assert_eq!(x3("exp(0)").unwrap(), 1.0);
        assert_eq!(x3("e").unwrap(), std::f64::consts::E);
        // Qi tries constants before functions: `exp(` is `e` + garbage.
        assert!(qi("exp(0)").is_err());
        assert!(qi("erf(0)").is_err());
        // Qi tries unary before binary functions: `atan2` is `atan` + garbage.
        assert!(qi("atan2(1, 2)").is_err());
        // Qi function names are case-insensitive, X3 ones are not.
        assert_eq!(qi("COS(0)").unwrap(), 1.0);
        assert_eq!(qi("EXP(0)").unwrap(), 1.0);
        assert!(!x3("COS(0)").unwrap_err().is_hard()); // unknown variable
        assert_eq!(qi("min(3, 1)").unwrap(), 1.0);
        // A variable whose name starts with a function name cannot be used.
        assert!(x3("cosine").unwrap_err().is_hard());
        assert!(!qi("cosine").unwrap_err().is_hard());
        // Variables: case-sensitive `E` is not the constant `e`.
        assert_eq!(x3("E * 2").unwrap(), 4.0);
        assert_eq!(qi("E * 2").unwrap(), 4.0);
        assert!(!x3("unknown").unwrap_err().is_hard());
        // Errors inside variables keep their class.
        assert!(x3("bad").unwrap_err().is_hard());
        let e = x3("cyc").unwrap_err();
        assert!(e.is_hard() && e.is_cyclic_reference(), "{e:?}"); // cyc = cyc + 1
    }

    #[test]
    fn vectors_matrices_random() {
        let s = section();
        let sc = ScopeRef::Section(&s);
        assert_eq!(
            eval_vector(b"[ 1, 23+2, 5, E ]", sc).unwrap(),
            vec![1.0, 25.0, 5.0, 2.0]
        );
        assert!(eval_vector(b"[]", sc).is_err());
        assert!(eval_vector(b"[1, ]", sc).is_err());
        assert!(eval_vector(b"[1 2]", sc).is_err());
        assert!(eval_vector(b"1, 2", sc).is_err());
        assert!(eval_vector(b"[nan(1, 2)]", sc).unwrap()[0].is_nan());
        assert_eq!(
            eval_matrix(b"[[1, 2], [3]]", sc).unwrap(),
            vec![vec![1.0, 2.0], vec![3.0]]
        );
        // Row given by the name of a vector parameter; whitespace inside the
        // name is skipped (Qi `key` rule with skipper).
        assert_eq!(
            eval_matrix(b"[v1, v 1, [3, 4]]", sc).unwrap(),
            vec![vec![1.0, 2.0], vec![1.0, 2.0], vec![3.0, 4.0]]
        );
        assert!(eval_matrix(b"[nope]", sc).is_err());
        // Only the first word of a row parameter is used: "[1," fails.
        assert!(eval_matrix(b"[v2]", sc).is_err());
        assert!(eval_matrix(b"[]", sc).is_err());
        let r = eval_random(b"10 uniform [0.2, 0.5 ]", sc).unwrap();
        assert_eq!(
            r,
            ParsedRandom {
                base: 10.0,
                kind: Some(DistributionKind::Uniform),
                parameters: vec![0.2, 0.5]
            }
        );
        let r = eval_random(b"2exponential [1]", sc).unwrap();
        assert_eq!((r.base, r.kind), (2.0, Some(DistributionKind::Exponential)));
        let r = eval_random(b"10", sc).unwrap();
        assert_eq!((r.base, r.kind), (10.0, None));
        assert!(eval_random(b"10 uniform [0.2, ]", sc).is_err());
        assert!(eval_random(b"10 uniform [0.2 0.5]", sc).is_err());
        assert!(eval_random(b"10 normalx [1, 2]", sc).is_err());
    }

    #[test]
    fn symbol_matching() {
        assert_eq!(
            longest_match(UNARY, b"log10(", 0, false).map(|x| x.1),
            Some(5)
        );
        assert_eq!(
            longest_match(UNARY, b"expm1(", 0, false).map(|x| x.1),
            Some(5)
        );
        assert_eq!(longest_match(UNARY, b"LOG(", 0, true).map(|x| x.1), Some(3));
        assert!(longest_match(UNARY, b"LOG(", 0, false).is_none());
        assert!(longest_match(BINARY, b"atan(", 0, false).is_none());
    }
}
