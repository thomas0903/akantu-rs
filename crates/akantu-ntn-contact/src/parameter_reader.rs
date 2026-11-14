// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/common/{parameter_reader.hh, parameter_reader.cc}.

//! Key/value input reader of the package (upstream `ParameterReader`,
//! TASN-039), independent of the Akantu input-file parser.
//!
//! Grammar, one entry per line: `<type> <keyword> = <value>` with `type` in
//! `elementtype`, `string`, `int`, `uint`, `real`, `bool` (case-insensitive),
//! `#` starting a comment, the `=` possibly attached to the keyword or the
//! value. Malformed lines are ignored with a warning, as upstream.
//!
//! Upstream semantics are preserved (UD-401, category C): values are
//! converted with C++ stream extraction (`int x = 3.7` stores 3, `real x =
//! 1.5e` stores 1.5, an unconvertible number stores 0, a negative `uint`
//! wraps), the first occurrence of a key wins (`std::map::insert`), and
//! [`ParameterReader::write_input_string`] prints reals with the default
//! C++ precision (6 significant digits, a lossy round trip). Two upstream
//! crashes become defined behaviour (category A): a missing key is an error
//! instead of `exit(EXIT_FAILURE)`, and a line with a keyword but no `=`
//! (upstream: `std::out_of_range` from `substr(1)` on an empty string) is
//! ignored with a warning.
//!
//! The input is tokenised on **bytes**, as upstream's `std::string`
//! operations do (`substr(1)`, `equal[0]`, `find_first_of`): no input can
//! make the reader panic. Keywords and string values are
//! stored as Rust strings, so an accepted line whose keyword or value is not
//! valid UTF-8 is an error naming the line (upstream stores the raw bytes);
//! nothing is converted lossily.

use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::path::Path;

use akantu_core::{ElementType, Error, Int, Real, Result, UInt};
use akantu_parser::stream;

use crate::sync_array::io_error;

/// Element types accepted by `elementtype` entries (upstream conversion map).
const ELEMENT_TYPES: [(&str, ElementType); 9] = [
    ("_segment_2", ElementType::Segment2),
    ("_segment_3", ElementType::Segment3),
    ("_triangle_3", ElementType::Triangle3),
    ("_triangle_6", ElementType::Triangle6),
    ("_tetrahedron_4", ElementType::Tetrahedron4),
    ("_tetrahedron_10", ElementType::Tetrahedron10),
    ("_quadrangle_4", ElementType::Quadrangle4),
    ("_quadrangle_8", ElementType::Quadrangle8),
    ("_hexahedron_8", ElementType::Hexahedron8),
];

/// Upstream `ParameterReader`.
#[derive(Clone, Debug, PartialEq)]
pub struct ParameterReader {
    data_types: BTreeSet<&'static str>,
    element_type_data: BTreeMap<String, ElementType>,
    string_data: BTreeMap<String, String>,
    int_data: BTreeMap<String, Int>,
    uint_data: BTreeMap<String, UInt>,
    real_data: BTreeMap<String, Real>,
    bool_data: BTreeMap<String, bool>,
}

impl Default for ParameterReader {
    fn default() -> Self {
        Self::new()
    }
}

/// C++ `operator>>` into a `std::string`: the first white-space-delimited
/// token from `pos` (empty if none); returns the token and the new position.
fn next_token(bytes: &[u8], pos: usize) -> (&[u8], usize) {
    let mut i = pos.min(bytes.len());
    while i < bytes.len() && stream::is_c_space(bytes[i]) {
        i += 1;
    }
    let start = i;
    while i < bytes.len() && !stream::is_c_space(bytes[i]) {
        i += 1;
    }
    (&bytes[start..i], i)
}

/// Bytes shown in a warning (display only; the data is never converted).
fn shown(bytes: &[u8]) -> std::borrow::Cow<'_, str> {
    String::from_utf8_lossy(bytes)
}

impl ParameterReader {
    /// Upstream constructor (the six data types and the element-type map).
    #[must_use]
    pub fn new() -> Self {
        Self {
            data_types: ["elementtype", "string", "uint", "int", "real", "bool"]
                .into_iter()
                .collect(),
            element_type_data: BTreeMap::new(),
            string_data: BTreeMap::new(),
            int_data: BTreeMap::new(),
            uint_data: BTreeMap::new(),
            real_data: BTreeMap::new(),
            bool_data: BTreeMap::new(),
        }
    }

    /// Upstream `readInputFile(file_name)`.
    ///
    /// # Errors
    /// The file cannot be read (upstream: message and `exit(EXIT_FAILURE)`),
    /// or an accepted line has a keyword or value that is not valid UTF-8
    /// (the message names the file and the line).
    pub fn read_input_file(&mut self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        let text = std::fs::read(path)
            .map_err(|e| Error::invalid(format!("Cannot open file {}!!! ({e})", path.display())))?;
        self.read_input_bytes(&text)
            .map_err(|e| Error::invalid(format!("{}: {e}", path.display())))
    }

    /// Parse input text (see [`Self::read_input_bytes`]; text is valid
    /// UTF-8, so only the conventions of upstream apply).
    ///
    /// # Errors
    /// None for valid UTF-8 text in practice (the byte tokenisation of a
    /// UTF-8 text splits only at ASCII bytes); kept for uniformity.
    pub fn read_input_str(&mut self, text: &str) -> Result<()> {
        self.read_input_bytes(text.as_bytes())
    }

    /// Parse input bytes (the body of [`Self::read_input_file`]): lines
    /// split at `\n` as `std::getline`, tokens split at C white space as
    /// `operator>>`. Malformed lines are ignored with a warning (upstream).
    ///
    /// # Errors
    /// An accepted line whose keyword or value is not valid UTF-8 (the
    /// message names the 1-based line number). Lines before it are kept.
    pub fn read_input_bytes(&mut self, text: &[u8]) -> Result<()> {
        for (i, line) in text.split(|&b| b == b'\n').enumerate() {
            self.read_line(line, i + 1)?;
        }
        Ok(())
    }

    fn read_line(&mut self, line: &[u8], line_no: usize) -> Result<()> {
        let clean_line = match line.iter().position(|&b| b == b'#') {
            Some(p) => &line[..p],
            None => line,
        };
        if clean_line.is_empty() {
            return Ok(());
        }
        let (type_tok, pos) = next_token(clean_line, 0);
        // `::tolower` of the "C" locale: ASCII letters only
        let ty = type_tok.to_ascii_lowercase();
        let Some(ty) = std::str::from_utf8(&ty)
            .ok()
            .and_then(|t| self.data_types.get(t).copied())
        else {
            log::warn!(
                " *** WARNING *** Data type {} does not exist in this input data structure. Ignore line: {}",
                shown(&ty),
                shown(clean_line)
            );
            return Ok(());
        };
        let (kw_tok, pos) = next_token(clean_line, pos);
        let (keyword, mut equal): (&[u8], &[u8]) = match kw_tok.iter().position(|&b| b == b'=') {
            Some(p) => (&kw_tok[..p], &kw_tok[p..]),
            None => (kw_tok, &[]),
        };
        let mut pos = pos;
        if equal.is_empty() {
            let (e, p) = next_token(clean_line, pos);
            equal = e;
            pos = p;
        }
        let mut value: &[u8] = &[];
        if equal.len() != 1 {
            if equal.is_empty() {
                // upstream: equal.substr(1) throws std::out_of_range (UD-401)
                log::warn!(
                    " *** WARNING *** Unrespected convention! Ignore line: {}",
                    shown(clean_line)
                );
                return Ok(());
            }
            // byte semantics of `equal.substr(1)` / `equal = equal[0]`
            value = &equal[1..];
            equal = &equal[..1];
        }
        if equal != b"=" {
            log::warn!(
                " *** WARNING *** Unrespected convention! Ignore line: {}",
                shown(clean_line)
            );
            return Ok(());
        }
        if value.is_empty() {
            value = next_token(clean_line, pos).0;
        }
        if value.is_empty() {
            log::warn!(
                " *** WARNING *** No value given! Ignore line: {}",
                shown(clean_line)
            );
            return Ok(());
        }
        let text = |what: &str, b: &[u8]| -> Result<String> {
            String::from_utf8(b.to_vec()).map_err(|_| {
                Error::invalid(format!(
                    "parameter input line {line_no}: the {what} {:?} is not valid UTF-8 (\
                     keys and values are stored as text)",
                    shown(b)
                ))
            })
        };
        // the keyword is validated where an entry is stored (an ignored
        // line is never an error)
        let key = || text("keyword", keyword);
        let bytes = value;
        match ty {
            "elementtype" => match ELEMENT_TYPES.iter().find(|(n, _)| n.as_bytes() == value) {
                Some(&(_, t)) => {
                    self.element_type_data.entry(key()?).or_insert(t);
                }
                None => log::warn!(
                    " *** WARNING *** ElementType {} does not exist. Ignore line: {}",
                    shown(value),
                    shown(clean_line)
                ),
            },
            "string" => {
                let value = text("value", value)?;
                self.string_data.entry(key()?).or_insert(value);
            }
            "int" => {
                let v = stream::extract_int::<Int>(bytes, 0).value.unwrap_or(0);
                self.int_data.entry(key()?).or_insert(v);
            }
            "uint" => {
                let v = stream::extract_int::<UInt>(bytes, 0).value.unwrap_or(0);
                self.uint_data.entry(key()?).or_insert(v);
            }
            "real" => {
                let v = stream::extract_double(bytes, 0).value.unwrap_or(0.0);
                self.real_data.entry(key()?).or_insert(v);
            }
            "bool" => match value.to_ascii_lowercase().as_slice() {
                b"true" => {
                    self.bool_data.entry(key()?).or_insert(true);
                }
                b"false" => {
                    self.bool_data.entry(key()?).or_insert(false);
                }
                _ => log::warn!(
                    " *** WARNING *** boolean cannot be {}. Ignore line: {}",
                    shown(value),
                    shown(clean_line)
                ),
            },
            _ => {}
        }
        Ok(())
    }

    /// Upstream `writeInputFile` content (element types, strings, ints,
    /// uints, reals, bools; keys in order; reals with 6 significant digits).
    #[must_use]
    pub fn write_input_string(&self) -> String {
        let mut out = String::new();
        for (k, t) in &self.element_type_data {
            let mut names: Vec<&str> = ELEMENT_TYPES
                .iter()
                .filter(|(_, et)| et == t)
                .map(|(n, _)| *n)
                .collect();
            names.sort_unstable();
            for n in names {
                out.push_str(&format!("ElementType {k} = {n}\n"));
            }
        }
        for (k, v) in &self.string_data {
            out.push_str(&format!("string {k} = {v}\n"));
        }
        for (k, v) in &self.int_data {
            out.push_str(&format!("Int {k} = {v}\n"));
        }
        for (k, v) in &self.uint_data {
            out.push_str(&format!("UInt {k} = {v}\n"));
        }
        for (k, v) in &self.real_data {
            out.push_str(&format!("Real {k} = {}\n", stream::format_double(*v)));
        }
        for (k, v) in &self.bool_data {
            out.push_str(&format!(
                "bool {k} = {}\n",
                if *v { "true" } else { "false" }
            ));
        }
        out
    }

    /// Upstream `writeInputFile(file_name)`.
    ///
    /// # Errors
    /// I/O failure.
    pub fn write_input_file(&self, path: impl AsRef<Path>) -> Result<()> {
        let path = path.as_ref();
        std::fs::write(path, self.write_input_string()).map_err(|e| io_error(path, &e))
    }

    fn missing(ty: &str, key: &str) -> Error {
        Error::not_found(
            "input parameter",
            format!(
                "{key}: This data was not in input file. You need the following line in your input file: {ty} {key} = ???"
            ),
        )
    }

    /// Upstream `get<ElementType>(key)`.
    ///
    /// # Errors
    /// Missing key (upstream exits).
    pub fn get_element_type(&self, key: &str) -> Result<ElementType> {
        self.element_type_data
            .get(key)
            .copied()
            .ok_or_else(|| Self::missing("ElementType", key))
    }

    /// Upstream `get<std::string>(key)`.
    ///
    /// # Errors
    /// Missing key.
    pub fn get_string(&self, key: &str) -> Result<&str> {
        self.string_data
            .get(key)
            .map(String::as_str)
            .ok_or_else(|| Self::missing("string", key))
    }

    /// Upstream `get<Int>(key)`.
    ///
    /// # Errors
    /// Missing key.
    pub fn get_int(&self, key: &str) -> Result<Int> {
        self.int_data
            .get(key)
            .copied()
            .ok_or_else(|| Self::missing("Int", key))
    }

    /// Upstream `get<UInt>(key)`.
    ///
    /// # Errors
    /// Missing key.
    pub fn get_uint(&self, key: &str) -> Result<UInt> {
        self.uint_data
            .get(key)
            .copied()
            .ok_or_else(|| Self::missing("UInt", key))
    }

    /// Upstream `get<Real>(key)`.
    ///
    /// # Errors
    /// Missing key.
    pub fn get_real(&self, key: &str) -> Result<Real> {
        self.real_data
            .get(key)
            .copied()
            .ok_or_else(|| Self::missing("Real", key))
    }

    /// Upstream `get<bool>(key)`.
    ///
    /// # Errors
    /// Missing key.
    pub fn get_bool(&self, key: &str) -> Result<bool> {
        self.bool_data
            .get(key)
            .copied()
            .ok_or_else(|| Self::missing("bool", key))
    }

    /// Upstream `has<std::string>(key)`.
    #[must_use]
    pub fn has_string(&self, key: &str) -> bool {
        self.string_data.contains_key(key)
    }

    /// Upstream `has<Int>(key)`.
    #[must_use]
    pub fn has_int(&self, key: &str) -> bool {
        self.int_data.contains_key(key)
    }

    /// Upstream `has<UInt>(key)`.
    #[must_use]
    pub fn has_uint(&self, key: &str) -> bool {
        self.uint_data.contains_key(key)
    }

    /// Upstream `has<Real>(key)`.
    #[must_use]
    pub fn has_real(&self, key: &str) -> bool {
        self.real_data.contains_key(key)
    }

    /// Upstream `has<bool>(key)`.
    #[must_use]
    pub fn has_bool(&self, key: &str) -> bool {
        self.bool_data.contains_key(key)
    }

    /// `has<ElementType>` (declared but not specialised upstream: a link
    /// error there).
    #[must_use]
    pub fn has_element_type(&self, key: &str) -> bool {
        self.element_type_data.contains_key(key)
    }
}

impl fmt::Display for ParameterReader {
    /// Upstream `printself` (prints only the frame).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "ParameterReader [")?;
        writeln!(f, "]")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn grammar_and_conversions() {
        let mut r = ParameterReader::new();
        r.read_input_str(
            "# comment\n\
             REAL dt = 1.5e-3 # trailing comment\n\
             real x=2\n\
             real y =3.5\n\
             real z= 4.25\n\
             int n = 3.7\n\
             int bad = abc\n\
             uint u = -1\n\
             bool flag = TRUE\n\
             bool other = maybe\n\
             string name = slider_top extra\n\
             elementtype et = _quadrangle_4\n\
             elementtype nope = _pentahedron_6\n\
             foo bar = 1\n\
             real noequal 3\n\
             real k\n\
             real dt = 9\n\
             real empty =\n",
        )
        .unwrap();
        assert_eq!(r.get_real("dt").unwrap(), 1.5e-3, "first occurrence wins");
        assert_eq!(r.get_real("x").unwrap(), 2.0);
        assert_eq!(r.get_real("y").unwrap(), 3.5);
        assert_eq!(r.get_real("z").unwrap(), 4.25);
        assert_eq!(r.get_int("n").unwrap(), 3);
        assert_eq!(r.get_int("bad").unwrap(), 0);
        assert_eq!(r.get_uint("u").unwrap(), u32::MAX);
        assert!(r.get_bool("flag").unwrap());
        assert!(!r.has_bool("other"));
        assert_eq!(r.get_string("name").unwrap(), "slider_top");
        assert_eq!(r.get_element_type("et").unwrap(), ElementType::Quadrangle4);
        assert!(!r.has_element_type("nope"));
        assert!(!r.has_real("noequal"));
        assert!(!r.has_real("k"));
        assert!(!r.has_real("empty"));
        assert!(r.get_real("missing").is_err());
    }

    #[test]
    fn write_read_round_trip() {
        let mut r = ParameterReader::new();
        r.read_input_str(
            "real a = 0.123456789\nint b = -4\nuint c = 5\nbool d = false\nstring e = s\nelementtype f = _triangle_3\n",
        )
        .unwrap();
        let text = r.write_input_string();
        assert_eq!(
            text,
            "ElementType f = _triangle_3\nstring e = s\nInt b = -4\nUInt c = 5\nReal a = 0.123457\nbool d = false\n"
        );
        let mut r2 = ParameterReader::new();
        r2.read_input_str(&text).unwrap();
        assert_eq!(r2.get_real("a").unwrap(), 0.123457, "upstream precision 6");
        assert_eq!(r2.get_int("b").unwrap(), -4);
        assert_eq!(r2.get_element_type("f").unwrap(), ElementType::Triangle3);
    }
}
