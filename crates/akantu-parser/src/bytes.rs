// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs: byte strings of input files.

//! [`ByteString`]: parameter names and values as read from an input file.
//!
//! Upstream stores them in `std::string`, i.e. arbitrary bytes: an input file
//! need not be UTF-8 (a value `abc\xff` is kept byte for byte, comments and
//! hence parameter names such as `E # caf\xe9` may contain any byte). The
//! port keeps the exact bytes, in the parsed tree and through the parameter
//! registry (`ParamKind::ByteString`/`ByteStringSet`, the kinds for upstream
//! `std::string`/`ID` members); text decoding happens explicitly at the Rust
//! API boundaries that need a `str` ([`ByteString::to_str`], or conversions
//! to `String` that fail on invalid UTF-8) and is lossy only for display
//! ([`std::fmt::Display`], error messages).

use std::borrow::Cow;
use std::fmt;
use std::ops::Deref;

/// A byte string from an input file (upstream `std::string`).
#[derive(Clone, Default, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ByteString(Vec<u8>);

impl ByteString {
    /// Empty byte string.
    #[must_use]
    pub const fn new() -> Self {
        Self(Vec::new())
    }

    /// The bytes.
    #[must_use]
    pub fn as_bytes(&self) -> &[u8] {
        &self.0
    }

    /// The bytes, by value.
    #[must_use]
    pub fn into_bytes(self) -> Vec<u8> {
        self.0
    }

    /// The text, if the bytes are valid UTF-8.
    #[must_use]
    pub fn to_str(&self) -> Option<&str> {
        std::str::from_utf8(&self.0).ok()
    }

    /// The text with invalid UTF-8 sequences replaced by U+FFFD (for
    /// display only).
    #[must_use]
    pub fn to_string_lossy(&self) -> Cow<'_, str> {
        String::from_utf8_lossy(&self.0)
    }

    /// The text, or the bytes back if they are not valid UTF-8.
    pub fn into_string(self) -> Result<String, Vec<u8>> {
        String::from_utf8(self.0).map_err(std::string::FromUtf8Error::into_bytes)
    }
}

impl Deref for ByteString {
    type Target = [u8];
    fn deref(&self) -> &[u8] {
        &self.0
    }
}

impl AsRef<[u8]> for ByteString {
    fn as_ref(&self) -> &[u8] {
        &self.0
    }
}

impl From<Vec<u8>> for ByteString {
    fn from(v: Vec<u8>) -> Self {
        Self(v)
    }
}

impl From<&[u8]> for ByteString {
    fn from(v: &[u8]) -> Self {
        Self(v.to_vec())
    }
}

impl<const N: usize> From<&[u8; N]> for ByteString {
    fn from(v: &[u8; N]) -> Self {
        Self(v.to_vec())
    }
}

impl From<String> for ByteString {
    fn from(s: String) -> Self {
        Self(s.into_bytes())
    }
}

impl From<&str> for ByteString {
    fn from(s: &str) -> Self {
        Self(s.as_bytes().to_vec())
    }
}

impl From<&String> for ByteString {
    fn from(s: &String) -> Self {
        Self(s.as_bytes().to_vec())
    }
}

impl PartialEq<str> for ByteString {
    fn eq(&self, other: &str) -> bool {
        self.0 == other.as_bytes()
    }
}

impl PartialEq<&str> for ByteString {
    fn eq(&self, other: &&str) -> bool {
        self.0 == other.as_bytes()
    }
}

impl PartialEq<String> for ByteString {
    fn eq(&self, other: &String) -> bool {
        self.0 == other.as_bytes()
    }
}

impl PartialEq<[u8]> for ByteString {
    fn eq(&self, other: &[u8]) -> bool {
        self.0 == other
    }
}

impl PartialEq<&[u8]> for ByteString {
    fn eq(&self, other: &&[u8]) -> bool {
        self.0 == *other
    }
}

impl PartialEq<ByteString> for str {
    fn eq(&self, other: &ByteString) -> bool {
        self.as_bytes() == other.0
    }
}

impl PartialEq<ByteString> for &str {
    fn eq(&self, other: &ByteString) -> bool {
        self.as_bytes() == other.0
    }
}

impl fmt::Display for ByteString {
    /// Lossy UTF-8 (for messages); use [`ByteString::as_bytes`] for the
    /// exact content.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.to_string_lossy())
    }
}

impl fmt::Debug for ByteString {
    /// `"text"` for valid UTF-8, else a byte-string literal `b"ab\xff"`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self.to_str() {
            Some(s) => fmt::Debug::fmt(s, f),
            None => {
                f.write_str("b\"")?;
                for &b in &self.0 {
                    for c in std::ascii::escape_default(b) {
                        fmt::Write::write_char(f, char::from(c))?;
                    }
                }
                f.write_str("\"")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_are_kept() {
        let b = ByteString::from(&b"abc\xff"[..]);
        assert_eq!(b.as_bytes(), b"abc\xff");
        assert_eq!(b.to_str(), None);
        assert_eq!(format!("{b}"), "abc\u{fffd}");
        assert_eq!(format!("{b:?}"), "b\"abc\\xff\"");
        assert_eq!(b.clone().into_string(), Err(b"abc\xff".to_vec()));
        let t = ByteString::from("steel");
        assert_eq!(t, "steel");
        assert_eq!("steel", t);
        assert_eq!(format!("{t:?}"), "\"steel\"");
        assert_eq!(t.into_string().unwrap(), "steel");
    }
}
