// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io/mesh_io_msh.cc (`File`
// helper: line-wise reading with `trim` + `std::stringstream` extraction),
// src/io/mesh_io/mesh_io_diana.cc (`my_getline`).

//! Line-oriented tokenizer shared by the mesh readers.
//!
//! Upstream reads one line at a time, trims it (`std::isspace`) and extracts
//! whitespace-separated values with `operator>>`; a failed extraction leaves
//! the target uninitialised and is never reported. Here every missing or
//! malformed token is an [`Error::Parse`] carrying `file:line:column`, so a
//! malformed file can never panic or silently produce garbage.

use akantu_core::{Error, Result};

/// `std::isspace` in the "C" locale (space, `\t`, `\n`, `\v`, `\f`, `\r`).
#[inline]
pub(crate) fn is_c_space(b: u8) -> bool {
    matches!(b, b' ' | b'\t' | b'\n' | b'\x0B' | b'\x0C' | b'\r')
}

/// Trim C whitespace on both sides (upstream `trim`).
pub(crate) fn trim_c(s: &str) -> &str {
    let b = s.as_bytes();
    let start = b.iter().position(|&c| !is_c_space(c)).unwrap_or(b.len());
    let end = b
        .iter()
        .rposition(|&c| !is_c_space(c))
        .map_or(start, |e| e + 1);
    &s[start..end]
}

/// A whole input file, split into lines on demand.
#[derive(Debug)]
pub(crate) struct Source<'a> {
    name: &'a str,
    text: &'a str,
    pos: usize,
    line_no: usize,
}

impl<'a> Source<'a> {
    /// `name` is used in error messages (normally the file path).
    pub(crate) fn new(name: &'a str, text: &'a str) -> Self {
        Self {
            name,
            text,
            pos: 0,
            line_no: 0,
        }
    }

    /// Display name of the file.
    pub(crate) fn name(&self) -> &'a str {
        self.name
    }

    /// Number of the last line returned (1-based, 0 before the first).
    pub(crate) fn line_no(&self) -> usize {
        self.line_no
    }

    /// Bytes not read yet (an upper bound for the number of entries that
    /// can still follow: counts from headers never size allocations beyond
    /// it).
    pub(crate) fn remaining(&self) -> usize {
        self.text.len().saturating_sub(self.pos)
    }

    /// Total size of the input in bytes.
    pub(crate) fn len(&self) -> usize {
        self.text.len()
    }

    /// Next raw line (without the `\n`), or `None` at end of file.
    pub(crate) fn next_line(&mut self) -> Option<Line<'a>> {
        if self.pos >= self.text.len() {
            return None;
        }
        let rest = &self.text[self.pos..];
        let (raw, advance) = match rest.find('\n') {
            Some(i) => (&rest[..i], i + 1),
            None => (rest, rest.len()),
        };
        self.pos += advance;
        self.line_no += 1;
        Some(Line {
            file: self.name,
            no: self.line_no,
            raw,
            cursor: 0,
        })
    }

    /// Next line; end of file is a parse error mentioning `context`.
    pub(crate) fn expect_line(&mut self, context: &str) -> Result<Line<'a>> {
        let (name, next) = (self.name, self.line_no + 1);
        self.next_line().ok_or_else(|| {
            parse_error(
                name,
                next,
                1,
                format!("unexpected end of file in {context}"),
            )
        })
    }

    /// Parse error located after the last line read.
    pub(crate) fn error_here(&self, message: impl Into<String>) -> Error {
        parse_error(self.name, self.line_no.max(1), 1, message.into())
    }
}

/// Build an [`Error::Parse`].
pub(crate) fn parse_error(file: &str, line: usize, column: usize, message: String) -> Error {
    Error::Parse {
        file: file.to_owned(),
        line,
        column,
        message,
    }
}

/// One line of input with a token cursor.
#[derive(Debug, Clone)]
pub(crate) struct Line<'a> {
    file: &'a str,
    no: usize,
    raw: &'a str,
    cursor: usize,
}

impl<'a> Line<'a> {
    /// 1-based line number.
    pub(crate) fn no(&self) -> usize {
        self.no
    }

    /// The line without its `\n` (may still end with `\r`).
    pub(crate) fn raw(&self) -> &'a str {
        self.raw
    }

    /// The trimmed line (upstream `File::line`).
    pub(crate) fn trimmed(&self) -> &'a str {
        trim_c(self.raw)
    }

    /// `true` if the line contains only whitespace.
    pub(crate) fn is_blank(&self) -> bool {
        self.trimmed().is_empty()
    }

    /// 1-based column of the cursor (after skipping whitespace).
    pub(crate) fn column(&self) -> usize {
        let b = self.raw.as_bytes();
        let mut c = self.cursor;
        while c < b.len() && is_c_space(b[c]) {
            c += 1;
        }
        c + 1
    }

    /// Error at the current cursor position.
    pub(crate) fn error(&self, message: impl Into<String>) -> Error {
        parse_error(self.file, self.no, self.column(), message.into())
    }

    /// Error at an explicit column.
    pub(crate) fn error_at(&self, column: usize, message: impl Into<String>) -> Error {
        parse_error(self.file, self.no, column, message.into())
    }

    /// Next whitespace-separated token with its 1-based column.
    pub(crate) fn next_token(&mut self) -> Option<(usize, &'a str)> {
        let b = self.raw.as_bytes();
        let mut i = self.cursor;
        while i < b.len() && is_c_space(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            self.cursor = i;
            return None;
        }
        let start = i;
        while i < b.len() && !is_c_space(b[i]) {
            i += 1;
        }
        self.cursor = i;
        Some((start + 1, &self.raw[start..i]))
    }

    /// Next token; a missing token is an error naming `what`.
    pub(crate) fn token(&mut self, what: &str) -> Result<(usize, &'a str)> {
        let col = self.column();
        self.next_token()
            .ok_or_else(|| self.error_at(col, format!("missing {what}")))
    }

    /// `true` if no token is left.
    pub(crate) fn at_end(&self) -> bool {
        self.raw.as_bytes()[self.cursor..]
            .iter()
            .all(|&c| is_c_space(c))
    }

    /// Remaining text after the cursor, trimmed.
    pub(crate) fn rest(&self) -> &'a str {
        trim_c(&self.raw[self.cursor..])
    }

    /// `std::quoted` extraction: after leading blanks, a `"…"` string in
    /// which `\` escapes the next character, or (no opening quote) a
    /// single whitespace-delimited word.
    pub(crate) fn quoted(&mut self, what: &str) -> Result<String> {
        let b = self.raw.as_bytes();
        let mut i = self.cursor;
        while i < b.len() && is_c_space(b[i]) {
            i += 1;
        }
        if b.get(i) != Some(&b'"') {
            return self.token(what).map(|(_, t)| t.to_owned());
        }
        let col = i + 1;
        let mut out = String::new();
        let mut chars = self.raw[i + 1..].char_indices();
        while let Some((k, c)) = chars.next() {
            match c {
                '\\' => match chars.next() {
                    Some((_, e)) => out.push(e),
                    None => break,
                },
                '"' => {
                    self.cursor = i + 1 + k + 1;
                    return Ok(out);
                }
                _ => out.push(c),
            }
        }
        Err(self.error_at(col, format!("unterminated quoted {what}")))
    }

    /// Next token as a finite `f64` (correctly rounded decimal conversion,
    /// like `strtod` behind `operator>>`).
    pub(crate) fn f64(&mut self, what: &str) -> Result<f64> {
        let (col, t) = self.token(what)?;
        parse_f64(t).ok_or_else(|| {
            self.error_at(
                col,
                format!("invalid {what} '{t}' (expected a finite number)"),
            )
        })
    }

    /// Next token as an `i64`.
    pub(crate) fn i64(&mut self, what: &str) -> Result<i64> {
        let (col, t) = self.token(what)?;
        parse_i64(t).ok_or_else(|| {
            self.error_at(col, format!("invalid {what} '{t}' (expected an integer)"))
        })
    }

    /// Next token as an `i32`.
    pub(crate) fn i32(&mut self, what: &str) -> Result<i32> {
        let (col, t) = self.token(what)?;
        parse_i64(t)
            .and_then(|v| i32::try_from(v).ok())
            .ok_or_else(|| {
                self.error_at(
                    col,
                    format!("invalid {what} '{t}' (expected a 32-bit integer)"),
                )
            })
    }

    /// Next token as a non-negative integer (`size_t` in upstream).
    pub(crate) fn u64(&mut self, what: &str) -> Result<u64> {
        let (col, t) = self.token(what)?;
        parse_i64(t)
            .and_then(|v| u64::try_from(v).ok())
            .ok_or_else(|| {
                self.error_at(
                    col,
                    format!("invalid {what} '{t}' (expected a non-negative integer)"),
                )
            })
    }

    /// Next token as a non-negative count that fits `usize`.
    pub(crate) fn count(&mut self, what: &str) -> Result<usize> {
        let (col, t) = self.token(what)?;
        parse_i64(t)
            .and_then(|v| usize::try_from(v).ok())
            .ok_or_else(|| {
                self.error_at(
                    col,
                    format!("invalid {what} '{t}' (expected a non-negative count)"),
                )
            })
    }
}

/// Decimal integer with optional sign (`operator>>` for integers accepts
/// the same strings, minus overflow).
pub(crate) fn parse_i64(t: &str) -> Option<i64> {
    let digits = t.strip_prefix(['+', '-']).unwrap_or(t);
    if digits.is_empty() || !digits.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse::<i64>().ok()
}

/// Decimal floating-point number; rejects `inf`/`nan` spellings and
/// overflow (libstdc++'s `operator>>` fails on both).
pub(crate) fn parse_f64(t: &str) -> Option<f64> {
    let ok_chars = t
        .bytes()
        .all(|b| b.is_ascii_digit() || matches!(b, b'+' | b'-' | b'.' | b'e' | b'E'));
    if !ok_chars || !t.bytes().any(|b| b.is_ascii_digit()) {
        return None;
    }
    t.parse::<f64>().ok().filter(|v| v.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lines_tokens_and_columns() {
        let text = "  $Nodes \r\n2\n1 0.5 -1e-3 7\n\n";
        let mut s = Source::new("f.msh", text);
        let mut l = s.next_line().unwrap();
        assert_eq!(l.trimmed(), "$Nodes");
        assert_eq!(l.next_token(), Some((3, "$Nodes")));
        assert!(l.at_end());
        let mut l = s.next_line().unwrap();
        assert_eq!(l.count("n").unwrap(), 2);
        let mut l = s.next_line().unwrap();
        assert_eq!(l.no(), 3);
        assert_eq!(l.u64("tag").unwrap(), 1);
        assert_eq!(l.f64("x").unwrap(), 0.5);
        assert_eq!(l.f64("y").unwrap(), -1e-3);
        assert_eq!(l.i32("z").unwrap(), 7);
        let e = l.f64("w").unwrap_err();
        assert!(
            matches!(
                e,
                Error::Parse {
                    line: 3,
                    column: 14,
                    ..
                }
            ),
            "{e}"
        );
        assert!(s.next_line().unwrap().is_blank());
        assert!(s.next_line().is_none());
        assert!(matches!(
            s.expect_line("block").unwrap_err(),
            Error::Parse { line: 5, .. }
        ));
    }

    #[test]
    fn number_validation() {
        assert_eq!(parse_i64("-12"), Some(-12));
        assert_eq!(parse_i64("+3"), Some(3));
        for bad in ["", "-", "1.0", "1e3", "0x10", "99999999999999999999"] {
            assert_eq!(parse_i64(bad), None, "{bad}");
        }
        assert_eq!(parse_f64("1."), Some(1.0));
        assert_eq!(parse_f64(".5"), Some(0.5));
        assert_eq!(parse_f64("-2.5E+02"), Some(-250.0));
        for bad in ["inf", "nan", "NaN", "1e400", "-", ".", "e5", "1,5", "0x1p3"] {
            assert_eq!(parse_f64(bad), None, "{bad}");
        }
        let mut l = Line {
            file: "x",
            no: 1,
            raw: "-1 4294967296",
            cursor: 0,
        };
        assert!(l.u64("a").is_err());
        assert!(l.i32("b").is_err());
    }

    #[test]
    fn trim_matches_isspace() {
        assert_eq!(trim_c("\x0B\x0C a b \r"), "a b");
        assert_eq!(trim_c("   "), "");
    }
}
