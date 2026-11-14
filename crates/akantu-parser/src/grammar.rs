// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/parser/input_file_parser.hh
// (`InputFileGrammar`, `create_subsection`, `create_parameter`),
// src/io/parser/parser_input_files.cc (`Parser::parse`).
// Copyright (©) 2013-2023 EPFL (Ecole Polytechnique Fédérale de Lausanne)
// Laboratory (LSMS - Laboratoire de Simulation en Mécanique des Solides)
//
// This file is part of akantu-rs, a derivative work of Akantu distributed
// under the GNU Lesser General Public License, version 3 or (at your option)
// any later version (see COPYING, COPYING.lesser and NOTICE.md).
//
// Positions reproduce the behaviour of Boost.Spirit's
// `classic::position_iterator2` (boost/spirit/home/classic/iterator/
// position_iterator.hpp, impl/position_iterator.ipp: Copyright (c) 2002 Juan
// Carlos Arevalo-Baeza, Copyright (c) 2002-2006 Hartmut Kaiser, Copyright (c)
// 2003 Giovanni Bajo; Boost Software License, Version 1.0, see
// LICENSES/BSL-1.0.txt): 1-based lines and columns, tab stops every 4.

//! The input-file grammar (upstream Boost.Spirit Qi `InputFileGrammar`),
//! reproduced as a byte-level parser with the same backtracking and skipping
//! behaviour (sections are nested with an explicit stack, see below):
//!
//! ```text
//! start         = mini_section
//! mini_section  = *( entry | section )
//! entry         = raw[key] >> '=' > value             (fails if the name is a duplicate)
//! section       = no_case[section_type] > lexeme[-section_name > -section_option]
//!                 > '[' > mini_section > ']'
//! section_name  = [a-zA-Z_] [a-zA-Z0-9_:]*            (no skipping)
//! section_option= +ascii::space >> section_name       (no skipping)
//! key           = [a-zA-Z_] *[a-zA-Z_0-9]             (with skipping between characters!)
//! value         = mono_line_value > *( '\' > mono_line_value )   (pieces concatenated)
//! mono_line_value = lexeme[ +(char - ('=' | eol | '#' | ';' | '\')) ]
//! skipper       = ascii::space | '#' *(char - eol)
//! ```
//!
//! Faithfully reproduced details:
//! * the skipper (whitespace and `#` comments) runs before every token, so
//!   a value may start on a later line (`a =\n 5`) and `\` continuations may
//!   be preceded by blank or comment lines;
//! * values keep trailing whitespace (`X135 = 1 + 1 # c` → `"1 + 1 "`) and end
//!   at `=`, `#`, `;`, `\` or end of line (so `;` is not a separator: it ends
//!   the value and then fails to parse);
//! * the key rule skips between its characters and the raw text is trimmed:
//!   `my param = 1` defines the parameter `"my param"`;
//! * section types are matched case-insensitively by longest prefix with no
//!   word boundary: `contact_resolution_penalty [` is a `contact_resolution`
//!   section named `_penalty`, `materials [` a `material` named `s`;
//! * a duplicated parameter name makes the `entry` alternative fail (parse
//!   error, upstream `create_parameter` returns `false`);
//! * unnamed sections are called `anonymous_<n>` with a counter that is
//!   incremented for every created section (see
//!   [`crate::section::AnonymousIds`]);
//! * positions are those of Spirit's `position_iterator2`: 1-based lines and
//!   columns, tab stops every 4 columns, `\r\n` and lone `\r` end a line.
//!
//! * bytes ≥ 0x80 are ordinary characters: Qi's `ascii::space` and
//!   `ascii::char_` test that a character is in the ASCII range before
//!   classifying it, so such bytes are never white space and are kept as
//!   they are in names, values and comments (no decoding).
//!
//! Sections are parsed with an explicit stack of open sections: any nesting
//! depth is accepted (upstream recurses and crashes between 5 000 and
//! 20 000 nested sections on an 8 MiB stack; deeper input is accepted by the
//! port, category A).

use std::cell::Cell;
use std::collections::HashSet;

use akantu_core::{Error, Result};

use crate::bytes::ByteString;
use crate::expr::longest_match;
use crate::section::{AnonymousIds, ParserParameter, ParserSection, ParserType};
use crate::stream::{is_c_space, trim_c_space};

static SECTION_TYPES: [(&str, ParserType); 25] = {
    let mut t = [("", ParserType::NotDefined); 25];
    let mut i = 0;
    while i < 25 {
        t[i] = (ParserType::ALL[i].name(), ParserType::ALL[i]);
        i += 1;
    }
    t
};

struct Grammar<'b, 'c, 'd> {
    s: &'b [u8],
    pos: usize,
    file: &'c str,
    ids: &'c mut AnonymousIds<'d>,
    /// Last computed position `(offset, line, column)`.
    cursor: Cell<(usize, usize, usize)>,
}

/// A section whose body is being parsed.
struct Open {
    section: ParserSection,
    /// Names already defined in this section (upstream `std::map` lookup).
    names: HashSet<ByteString>,
    /// Offset and text of the header (`type name option`), for messages.
    header: usize,
    header_text: String,
}

/// Parse `bytes` into `root` (which must be empty). On error the sections
/// parsed so far stay in the tree, as upstream.
pub(crate) fn parse_into(
    root: &mut ParserSection,
    bytes: &[u8],
    file: &str,
    ids: &mut AnonymousIds<'_>,
) -> Result<()> {
    let mut g = Grammar {
        s: bytes,
        pos: 0,
        file,
        ids,
        cursor: Cell::new((0, 1, 1)),
    };
    let top = std::mem::replace(root, ParserSection::new(ParserType::Global, "global"));
    let names = top.parameters.iter().map(|p| p.name.clone()).collect();
    let mut open = vec![Open {
        section: top,
        names,
        header: 0,
        header_text: String::new(),
    }];
    let r = g.sections(&mut open);
    // Attach the sections still open (on error) to their parents.
    while let Some(o) = open.pop() {
        match open.last_mut() {
            Some(parent) => parent.section.sub_sections.push(o.section),
            None => *root = o.section,
        }
    }
    r?;
    g.skip();
    if g.pos != g.s.len() {
        return Err(g.error_at(
            g.pos,
            "expecting a parameter (`name = value`) or a section (`type [name [option]] [ ... ]`)",
        ));
    }
    Ok(())
}

fn is_key_start(c: u8) -> bool {
    c.is_ascii_alphabetic() || c == b'_'
}

fn is_key_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_'
}

fn is_section_name_char(c: u8) -> bool {
    c.is_ascii_alphanumeric() || c == b'_' || c == b':'
}

fn ends_value(c: u8) -> bool {
    matches!(c, b'=' | b'\n' | b'\r' | b'#' | b';' | b'\\')
}

impl Grammar<'_, '_, '_> {
    fn peek(&self) -> Option<u8> {
        self.s.get(self.pos).copied()
    }

    /// The skipper: whitespace and `#` comments (a comment stops before the
    /// end of line).
    fn skip(&mut self) {
        loop {
            match self.peek() {
                Some(c) if is_c_space(c) => self.pos += 1,
                Some(b'#') => {
                    self.pos += 1;
                    while let Some(c) = self.peek() {
                        if c == b'\n' || c == b'\r' {
                            break;
                        }
                        self.pos += 1;
                    }
                }
                _ => break,
            }
        }
    }

    /// Line/column of `offset` as computed by Spirit's `position_iterator2`
    /// (resumes from the previous query when moving forward).
    fn position(&self, offset: usize) -> (usize, usize) {
        let end = offset.min(self.s.len());
        let (mut i, mut line, mut col) = match self.cursor.get() {
            c @ (o, _, _) if o <= end => c,
            _ => (0, 1, 1),
        };
        while i < end {
            match self.s[i] {
                b'\n' => {
                    line += 1;
                    col = 1;
                }
                b'\r' => {
                    if self.s.get(i + 1) != Some(&b'\n') {
                        line += 1;
                        col = 1;
                    }
                }
                b'\t' => col += 4 - (col - 1) % 4,
                _ => col += 1,
            }
            i += 1;
        }
        self.cursor.set((end, line, col));
        (line, col)
    }

    fn current_line(&self, offset: usize) -> String {
        let off = offset.min(self.s.len());
        let start = self.s[..off]
            .iter()
            .rposition(|&c| c == b'\n' || c == b'\r')
            .map_or(0, |p| p + 1);
        let end = self.s[off..]
            .iter()
            .position(|&c| c == b'\n' || c == b'\r')
            .map_or(self.s.len(), |p| off + p);
        String::from_utf8_lossy(&self.s[start..end]).into_owned()
    }

    fn error_at(&self, offset: usize, what: &str) -> Error {
        let (line, column) = self.position(offset);
        let found = match self.s.get(offset) {
            None => "end of input".to_string(),
            Some(&c) if c.is_ascii_graphic() || c == b' ' => format!("'{}'", c as char),
            Some(&c) => format!("byte 0x{c:02x}"),
        };
        Error::Parse {
            file: self.file.to_string(),
            line,
            column,
            message: format!(
                "{what}, found {found}\n'{}'\n{}^- here",
                self.current_line(offset),
                " ".repeat(column.saturating_sub(1))
            ),
        }
    }

    /// `mini_section = *(entry | section)` for the innermost open section,
    /// and `section = … '[' > mini_section > ']'` with an explicit stack:
    /// `open[0]` is the root, a header pushes a section, `]` pops it.
    fn sections(&mut self, open: &mut Vec<Open>) -> Result<()> {
        loop {
            let save = self.pos;
            let Some(top) = open.last_mut() else {
                return Ok(());
            };
            if self.entry(&mut top.section, &mut top.names)? {
                continue;
            }
            self.pos = save;
            if let Some(o) = self.section_header(&mut top.section)? {
                open.push(o);
                continue;
            }
            self.pos = save;
            // End of the innermost mini_section.
            if open.len() == 1 {
                return Ok(());
            }
            self.skip();
            if self.peek() != Some(b']') {
                let Some(o) = open.last() else {
                    return Ok(());
                };
                let (l, _) = self.position(o.header);
                return Err(self.error_at(
                    self.pos,
                    &format!(
                        "expecting a parameter, a section or ']' closing the section '{}' opened at line {l}",
                        o.header_text.trim()
                    ),
                ));
            }
            self.pos += 1;
            if let Some(done) = open.pop()
                && let Some(parent) = open.last_mut()
            {
                parent.section.sub_sections.push(done.section);
            }
        }
    }

    /// `entry = raw[key] >> '=' > value` with the `create_parameter` action.
    fn entry(&mut self, sect: &mut ParserSection, names: &mut HashSet<ByteString>) -> Result<bool> {
        self.skip();
        let key_start = self.pos;
        if !self.peek().is_some_and(is_key_start) {
            return Ok(false);
        }
        self.pos += 1;
        loop {
            self.skip();
            match self.peek() {
                Some(c) if is_key_char(c) => self.pos += 1,
                _ => break,
            }
        }
        let raw_end = self.pos;
        self.skip();
        if self.peek() != Some(b'=') {
            return Ok(false);
        }
        self.pos += 1;
        let value = self.value()?;
        let name = ByteString::from(trim_c_space(&self.s[key_start..raw_end]));
        if names.contains(&name) {
            // `create_parameter` catches the duplicate-name exception and
            // fails the match (`_pass = false`).
            return Ok(false);
        }
        let (line, column) = self.position(key_start);
        names.insert(name.clone());
        sect.parameters.push(ParserParameter {
            name,
            value,
            file: self.file.to_string(),
            line,
            column,
        });
        Ok(true)
    }

    /// `value = mono_line_value > *('\\' > mono_line_value)` (bytes kept).
    fn value(&mut self) -> Result<ByteString> {
        let Some(first) = self.mono_line_value() else {
            return Err(self.error_at(self.pos, "expecting a parameter value"));
        };
        let mut bytes = first.to_vec();
        loop {
            self.skip();
            if self.peek() != Some(b'\\') {
                break;
            }
            self.pos += 1;
            let Some(more) = self.mono_line_value() else {
                return Err(
                    self.error_at(self.pos, "expecting the continuation of a value after '\\'")
                );
            };
            bytes.extend_from_slice(more);
        }
        Ok(ByteString::from(bytes))
    }

    /// `mono_line_value` (a lexeme with pre-skip): at least one character.
    fn mono_line_value(&mut self) -> Option<&[u8]> {
        self.skip();
        let start = self.pos;
        while self.peek().is_some_and(|c| !ends_value(c)) {
            self.pos += 1;
        }
        (self.pos > start).then(|| &self.s[start..self.pos])
    }

    /// `section_name` (no skipping; ASCII by construction).
    fn section_name(&mut self) -> Option<String> {
        if !self.peek().is_some_and(is_key_start) {
            return None;
        }
        let start = self.pos;
        self.pos += 1;
        while self.peek().is_some_and(is_section_name_char) {
            self.pos += 1;
        }
        Some(
            self.s[start..self.pos]
                .iter()
                .map(|&c| char::from(c))
                .collect(),
        )
    }

    /// `section_option = +ascii::space >> section_name` (no skipping).
    fn section_option(&mut self) -> Option<String> {
        let save = self.pos;
        let mut n = 0;
        while self.peek().is_some_and(is_c_space) {
            self.pos += 1;
            n += 1;
        }
        if n > 0
            && let Some(opt) = self.section_name()
        {
            return Some(opt);
        }
        self.pos = save;
        None
    }

    /// `section = no_case[section_type] > lexeme[-section_name >
    /// -section_option] > '['`: the header and its `[`, creating the
    /// section (`create_subsection`, which increments the anonymous counter
    /// for every section). `None` if no section type matches. If the `[` is
    /// missing, the created (empty) section is added to `parent` before the
    /// error, as upstream's semantic action has already run.
    fn section_header(&mut self, parent: &mut ParserSection) -> Result<Option<Open>> {
        self.skip();
        let header = self.pos;
        let Some((ty, len)) = longest_match(&SECTION_TYPES, self.s, self.pos, true) else {
            return Ok(None);
        };
        self.pos += len;
        // lexeme[-section_name > -section_option] (with pre-skip)
        self.skip();
        let name = self.section_name();
        let option = self.section_option();
        let id = self.ids.next();
        let name = name.unwrap_or_else(|| format!("anonymous_{id}"));
        let mut section = ParserSection::new(ty, name);
        section.option = option.unwrap_or_default();
        let header_text = String::from_utf8_lossy(&self.s[header..self.pos]).into_owned();
        self.skip();
        if self.peek() != Some(b'[') {
            parent.sub_sections.push(section);
            return Err(self.error_at(
                self.pos,
                &format!(
                    "expecting '[' after the section header '{}'",
                    header_text.trim()
                ),
            ));
        }
        self.pos += 1;
        Ok(Some(Open {
            section,
            names: HashSet::new(),
            header,
            header_text,
        }))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::section::Parser;

    fn parse(text: &str) -> Result<Parser> {
        let mut c = crate::section::FIRST_ANONYMOUS_ID;
        Parser::parse_str_with(text, "t.dat", AnonymousIds::Local(&mut c))
    }

    #[test]
    fn values_keep_trailing_space_and_continuations() {
        let p = parse("a = 1 + 3 # c\nb =\n\n  5\nc = [1, 2,\\\n   3]\n").unwrap();
        assert_eq!(p.root.parameter("a").unwrap().value, "1 + 3 ");
        assert_eq!(p.root.parameter("b").unwrap().value, "5");
        assert_eq!(p.root.parameter("c").unwrap().value, "[1, 2,3]");
    }

    #[test]
    fn keys_skip_between_characters() {
        let p = parse("my param = 1\nE # odd\n = 2\nmaterial elastic\n  F = 3\n").unwrap();
        assert!(p.root.parameter("my param").is_some());
        assert_eq!(p.root.parameter("E # odd").unwrap().value, "2");
        // Without '[' a section header followed by an entry is one key.
        assert_eq!(
            p.root.parameter("material elastic\n  F").unwrap().value,
            "3"
        );
        assert!(p.root.sub_sections.is_empty());
    }

    #[test]
    fn section_headers() {
        let p = parse(
            "MATERIAL elastic opt [ ]\ncontact_resolution_penalty [ ]\nmaterials [ ]\nuser [ ]\nmodel_solver [ ]\n",
        )
        .unwrap();
        let s = &p.root.sub_sections;
        assert_eq!(
            (s[0].section_type, s[0].name.as_str(), s[0].option.as_str()),
            (ParserType::Material, "elastic", "opt")
        );
        assert_eq!(
            (s[1].section_type, s[1].name.as_str()),
            (ParserType::ContactResolution, "_penalty")
        );
        assert_eq!(
            (s[2].section_type, s[2].name.as_str()),
            (ParserType::Material, "s")
        );
        assert_eq!(
            (s[3].section_type, s[3].name.as_str()),
            (ParserType::User, "anonymous_15")
        );
        assert_eq!(
            (s[4].section_type, s[4].name.as_str()),
            (ParserType::ModelSolver, "anonymous_16")
        );
    }

    #[test]
    fn positions_use_tab_stops() {
        let p = parse("\tname = x\n \t E = 1\r\nF = 2\rG = 3\n").unwrap();
        let get = |n: &str| {
            let q = p.root.parameter(n).unwrap();
            (q.line, q.column)
        };
        assert_eq!(get("name"), (1, 5));
        assert_eq!(get("E"), (2, 6));
        assert_eq!(get("F"), (3, 1));
        assert_eq!(get("G"), (4, 1));
    }

    #[test]
    fn errors() {
        for (text, line, col) in [
            ("a = 1; b = 2\n", 1, 6),
            ("a = 1\na = 2\n", 2, 1),
            ("material elastic\n", 2, 1),
            ("material elastic opt1 opt2 [ ]\n", 1, 23),
            ("material elastic [\n E = 1\n", 3, 1),
            ("a = \n", 2, 1),
            ("]", 1, 1),
            ("a = 1 \\\n", 2, 1),
        ] {
            match parse(text) {
                Err(Error::Parse {
                    line: l, column: c, ..
                }) => assert_eq!((l, c), (line, col), "{text:?}"),
                other => panic!("{text:?}: {other:?}"),
            }
        }
    }
}
