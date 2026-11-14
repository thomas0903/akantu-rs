// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_common_inline_impl.hh
// (to_lower, trim, trim(str, c), printMemorySize<T>).

//! String helpers of `aka_common` (CORE-039).
//!
//! Upstream works on `std::string` bytes in the "C" locale (`tolower`,
//! `isspace`); here the same byte classes are applied to UTF-8 `&str`, so
//! only ASCII bytes are changed or trimmed (on the aarch64 oracle `char` is
//! unsigned and bytes ≥ 0x80 are neither upper-case nor space in the "C"
//! locale, so the results are identical; on signed-`char` platforms upstream
//! calls `tolower`/`isspace` with negative values, which is undefined).

use crate::random::glibc_libm::log;

/// `std::isspace` in the "C" locale: space, `\t`, `\n`, `\v`, `\f`, `\r`
/// (note `\v`, which `char::is_ascii_whitespace` does not include).
#[inline]
#[must_use]
pub fn is_c_space(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\x0B' | '\x0C' | '\r')
}

/// Upstream `to_lower`: ASCII upper-case letters lowered (C locale).
#[must_use]
pub fn to_lower(s: &str) -> String {
    s.to_ascii_lowercase()
}

/// Upstream `trim(str)`: C-locale white space removed at both ends.
#[must_use]
pub fn trim(s: &str) -> &str {
    s.trim_matches(is_c_space)
}

/// Upstream `trim(str, c)`: every leading and trailing `c` removed (upstream
/// takes a byte; pass an ASCII character for identical results).
#[must_use]
pub fn trim_char(s: &str, c: char) -> &str {
    s.trim_matches(c)
}

/// Binary prefixes of `printMemorySize`.
const PREFIXES: [&str; 9] = ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi", "Yi"];

/// Upstream `printMemorySize<T>(size)`: the memory of `size` values of `T`
/// as `"x.xx<prefix>Byte"`, the prefix chosen by
/// `mult = UInt((log(bytes) / log(2)) / 10)` with glibc's `log` (the port
/// in [`crate::random`], so the boundary cases such as exactly 1024 bytes
/// follow the oracle) and two decimals (`std::fixed`,
/// `std::setprecision(2)`, ties to even on the exact value as glibc).
///
/// Upstream divides by `Real(1 << (10 * mult))` with a 32-bit `int` shift,
/// undefined for `mult ≥ 4` (≥ 1 TiB, unreachable with a `UInt` element
/// count times `sizeof(Real)`); the port divides by `2^(10·mult)` (UD-266).
#[must_use]
pub fn print_memory_size<T>(size: usize) -> String {
    print_memory_size_bytes(size.saturating_mul(std::mem::size_of::<T>()))
}

/// [`print_memory_size`] for a byte count.
#[must_use]
pub fn print_memory_size_bytes(bytes: usize) -> String {
    #[allow(clippy::cast_precision_loss)] // upstream: size_t → double
    let mut real_size = bytes as f64;
    let mut mult = 0usize;
    if real_size != 0.0 {
        let m = (log(real_size) / log(2.0)) / 10.0;
        // C++ double → UInt conversion truncates; m ∈ [0, 6.4] for any usize
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        {
            mult = m as usize;
        }
    }
    let shift = i32::try_from(10 * mult).unwrap_or(i32::MAX);
    real_size /= 2f64.powi(shift);
    let prefix = PREFIXES.get(mult).copied().unwrap_or("?");
    format!("{real_size:.2}{prefix}Byte")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trim_and_lower_follow_the_c_locale() {
        assert_eq!(trim(" \t\x0B\x0Cab c\r\n"), "ab c");
        assert_eq!(trim("   "), "");
        assert_eq!(trim("\u{a0}x\u{a0}"), "\u{a0}x\u{a0}"); // NBSP is not a C space
        assert_eq!(trim_char("\"name\"", '"'), "name");
        assert_eq!(trim_char("xxaxx", 'x'), "a");
        assert_eq!(to_lower("MiXeD_Élan"), "mixed_Élan");
        assert!(is_c_space('\x0B') && !'\x0B'.is_ascii_whitespace());
    }

    #[test]
    fn memory_sizes() {
        assert_eq!(print_memory_size::<f64>(0), "0.00Byte");
        assert_eq!(print_memory_size::<f64>(1), "8.00Byte");
        assert_eq!(print_memory_size::<f64>(1000), "7.81KiByte");
        assert_eq!(print_memory_size::<u8>(1536), "1.50KiByte");
        assert_eq!(print_memory_size::<f64>(3 << 20), "24.00MiByte");
        assert_eq!(print_memory_size_bytes(1 << 30), "1.00GiByte");
    }
}
