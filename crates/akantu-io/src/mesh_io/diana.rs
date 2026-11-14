// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/mesh_io/mesh_io_diana.hh,
// src/io/mesh_io/mesh_io_diana.cc (`MeshIODiana::read`, `readCoordinates`,
// `readElements`, `readConnectivity`, `readMaterialElement`, `readMaterial`,
// `readGroups`, `readInterval`).

//! TNO Diana mesh reader (upstream `MeshIODiana`).
//!
//! Sections (header lines): `'COORDINATES'` (`id x y z`), `'ELEMENTS'`
//! (`CONNECTIVITY`: `id TYPE n1 … nk`, an element may continue on the next
//! line(s); `MATERIALS`: `/ a-b c-d … / mat` → `Int` mesh data `material`),
//! `'MATERIALS'` (`[id] YOUNG|DENSIT|POISON value` → elastic material
//! definitions, written by upstream to `material_<file>`), `'GROUPS'`
//! (`NODES` / `ELEMEN` blocks of `id NAME / intervals /` lines, up to
//! `'SUPPORTS'`). Element types: `T9TM`, `CT6CM`, `Q12TM`, `CQ8CM`,
//! `TP18L`, `CTP45`, `TE12L`, `HX24L`, `CHX60`.
//!
//! Differences with upstream (all on inputs where upstream is undefined or
//! hangs; see UD-048 and UD-092 in `docs/UPSTREAM_DEFECTS.md`):
//!
//! * UD-048: coordinates are stored with the mesh's spatial dimension
//!   (upstream pushes 3-vectors into a `dim`-component array, which fails
//!   or corrupts memory in 2-D). Rule: a node line needs at least `dim`
//!   coordinates; coordinates beyond `dim` must be `0` (a non-planar node
//!   in a 2-D mesh is an error, not a silent projection).
//! * Node and element numbers are looked up in maps built from the file
//!   (upstream subtracts the smallest node number, which is only right for
//!   contiguous, ordered numbering — identical results in that case).
//! * End of file inside a section, malformed numbers and references to
//!   undefined nodes are errors (upstream loops forever, or reads garbage).
//! * Elements without a material interval get `material = 0` (upstream:
//!   uninitialised); single numbers are accepted in material intervals
//!   (upstream only parses `a-b`).
//! * Element groups have no nodes, as upstream (`add(element, false,
//!   false)`): kept for parity.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use akantu_core::{Element, ElementType, Error, GhostType, Idx, Result};

use super::lexer::{Line, Source, parse_error, parse_i64, trim_c};
use super::numfmt::format_g;
use super::staging::{IntData, MeshBuild};
use super::tables::{diana_read_order, diana_to_akantu};

/// One elastic material of a Diana `'MATERIALS'` section.
#[derive(Clone, Debug, PartialEq)]
pub struct DianaMaterial {
    /// `material<k>`, numbered in order of appearance from 1 (upstream
    /// ignores the material numbers of the file).
    pub name: String,
    /// Akantu parameter name (`E`, `nu`, `rho`) → value.
    pub properties: BTreeMap<String, f64>,
}

/// Materials of a Diana file.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DianaMaterials {
    /// In order of appearance.
    pub materials: Vec<DianaMaterial>,
}

impl DianaMaterials {
    /// Text of upstream's `material_<file>` input file: one
    /// `material elastic [ … ]` section per material. Values are written
    /// with `%g` and precision 6 like upstream, raised to the smallest
    /// precision that round-trips when 6 digits would lose information
    /// (UD-061 policy).
    #[must_use]
    pub fn to_input_file(&self) -> String {
        let mut s = String::new();
        for m in &self.materials {
            s.push_str("material elastic [\n");
            s.push_str(&format!("\tname = {}\n", m.name));
            for (k, v) in &m.properties {
                s.push_str(&format!("\t{k} = {}\n", shortest_g(*v)));
            }
            s.push_str("]\n");
        }
        s
    }
}

/// `%g` with upstream's default precision 6, raised to the smallest
/// precision that converts back to `v` exactly.
fn shortest_g(v: f64) -> String {
    (6..=17)
        .map(|p| format_g(v, p, false))
        .find(|s| s.parse::<f64>().is_ok_and(|x| x.to_bits() == v.to_bits()))
        .unwrap_or_else(|| format_g(v, 17, false))
}

/// Path of upstream's material side file for input `path`:
/// `material_<file name>` in the directory of the input (upstream prefixes
/// the whole path string, which is the same for a bare file name).
#[must_use]
pub fn material_file_path(path: &Path) -> PathBuf {
    let name = path
        .file_name()
        .map_or_else(|| "mesh".into(), |n| n.to_string_lossy().into_owned());
    path.with_file_name(format!("material_{name}"))
}

/// Write `materials` next to `path` (see [`material_file_path`]).
pub(crate) fn write_material_file(path: &Path, materials: &DianaMaterials) -> Result<()> {
    let out = material_file_path(path);
    std::fs::write(&out, materials.to_input_file()).map_err(|source| Error::Io {
        path: out.display().to_string(),
        source,
    })
}

/// Result of a Diana read.
#[derive(Debug)]
pub(crate) struct DianaResult {
    pub(crate) build: MeshBuild,
    pub(crate) warnings: Vec<String>,
    pub(crate) materials: Option<DianaMaterials>,
}

struct Reader<'a> {
    src: Source<'a>,
    dim: usize,
    build: MeshBuild,
    node_ids: BTreeMap<i64, Idx>,
    elements: BTreeMap<i64, Element>,
    materials: Option<DianaMaterials>,
    warnings: Vec<String>,
}

/// Section header or keyword line (upstream compares whole lines; here
/// surrounding blanks are ignored).
fn key<'a>(line: &Line<'a>) -> &'a str {
    // `my_getline` cuts at the first '\r'.
    let raw = line.raw();
    trim_c(raw.split('\r').next().unwrap_or(raw))
}

/// Read Diana text.
pub(crate) fn read(name: &str, text: &str, dim: usize) -> Result<DianaResult> {
    let mut r = Reader {
        src: Source::new(name, text),
        dim,
        build: MeshBuild::new(dim),
        node_ids: BTreeMap::new(),
        elements: BTreeMap::new(),
        materials: None,
        warnings: Vec::new(),
    };
    let mut pending: Option<Line<'_>> = None;
    loop {
        let line = match pending.take() {
            Some(l) => l,
            None => match r.src.next_line() {
                Some(l) => l,
                None => break,
            },
        };
        pending = match key(&line) {
            "'COORDINATES'" => Some(r.coordinates()?),
            "'ELEMENTS'" => r.elements()?,
            "'MATERIALS'" => r.material_section()?,
            "'GROUPS'" => r.groups()?,
            _ => None,
        };
    }
    Ok(DianaResult {
        build: r.build,
        warnings: r.warnings,
        materials: r.materials,
    })
}

impl<'a> Reader<'a> {
    fn coordinates(&mut self) -> Result<Line<'a>> {
        loop {
            let mut line = self
                .src
                .expect_line("'COORDINATES' (no 'ELEMENTS' section)")?;
            if key(&line).starts_with('\'') {
                return Ok(line);
            }
            if line.is_blank() {
                continue;
            }
            let id = line.i64("node number")?;
            let mut x = [0.0; 3];
            let mut n = 0;
            while n < 3 && !line.at_end() {
                x[n] = line.f64("coordinate")?;
                n += 1;
            }
            if n < self.dim {
                return Err(line.error(format!(
                    "node {id} has {n} coordinates, the mesh is {}-dimensional",
                    self.dim
                )));
            }
            if let Some(c) = x[self.dim..].iter().find(|&&c| c != 0.0) {
                return Err(line.error_at(
                    1,
                    format!(
                        "node {id} has a non-zero out-of-plane coordinate {c} in a {}-dimensional mesh (UD-048)",
                        self.dim
                    ),
                ));
            }
            let idx = self.build.push_node(&x)?;
            if self.node_ids.insert(id, idx).is_some() {
                return Err(line.error_at(1, format!("duplicate node number {id}")));
            }
        }
    }

    fn node(&self, line: &Line<'a>, col: usize, id: i64) -> Result<Idx> {
        self.node_ids
            .get(&id)
            .copied()
            .ok_or_else(|| line.error_at(col, format!("node {id} is not defined in 'COORDINATES'")))
    }

    /// `'ELEMENTS'`: `CONNECTIVITY` then optional `MATERIALS`; returns the
    /// line that ended the section, if it must be dispatched.
    fn elements(&mut self) -> Result<Option<Line<'a>>> {
        let mut line = self.src.expect_line("'ELEMENTS'")?;
        if key(&line) == "CONNECTIVITY" {
            line = self.connectivity()?;
        }
        if key(&line) == "MATERIALS" {
            return self.element_materials().map(Some);
        }
        Ok(Some(line))
    }

    fn connectivity(&mut self) -> Result<Line<'a>> {
        loop {
            let mut line = self.src.expect_line("CONNECTIVITY (no MATERIALS line)")?;
            let k = key(&line);
            if k == "MATERIALS" || k.starts_with('\'') {
                return Ok(line);
            }
            if line.is_blank() {
                continue;
            }
            let Some((_, id_tok)) = line.next_token() else {
                continue;
            };
            let Some(id) = parse_i64(id_tok) else {
                self.warnings.push(format!(
                    "{}:{}: line ignored in CONNECTIVITY: '{}'",
                    self.src.name(),
                    line.no(),
                    line.trimmed()
                ));
                continue;
            };
            let ty_name = line.next_token().map_or("", |t| t.1);
            let Some(ty) = diana_to_akantu(ty_name) else {
                self.warnings.push(format!(
                    "{}:{}: unsupported Diana element type '{ty_name}' (element {id}) ignored",
                    self.src.name(),
                    line.no()
                ));
                continue;
            };
            let nnpe = ty.nb_nodes_per_element();
            let order = diana_read_order(ty);
            let mut local = vec![0; nnpe];
            let mut j = 0;
            let mut cur = line;
            while j < nnpe {
                if cur.at_end() {
                    // Element continued on the next line.
                    cur = self
                        .src
                        .expect_line(&format!("connectivity of element {id}"))?;
                    if cur.is_blank()
                        || !cur
                            .trimmed()
                            .split_ascii_whitespace()
                            .all(|t| parse_i64(t).is_some())
                    {
                        return Err(cur.error_at(
                            1,
                            format!("element {id} ({ty_name}) needs {nnpe} nodes, found {j}"),
                        ));
                    }
                    continue;
                }
                let col = cur.column();
                let nid = cur.i64("node number")?;
                local[order.map_or(j, |o| o[j])] = self.node(&cur, col, nid)?;
                j += 1;
            }
            let e = self.build.push_element(ty, &local)?;
            if self
                .elements
                .insert(id, Element::new(ty, e, GhostType::NotGhost))
                .is_some()
            {
                return Err(cur.error_at(1, format!("duplicate element number {id}")));
            }
        }
    }

    /// `MATERIALS` sub-section of `'ELEMENTS'` (upstream
    /// `readMaterialElement`), up to the `'MATERIALS'` line.
    fn element_materials(&mut self) -> Result<Line<'a>> {
        let types: Vec<ElementType> = self.build.conn.keys().copied().collect();
        let mut data = IntData {
            per_type: BTreeMap::new(),
            fill: 0,
        };
        for ty in types {
            data.per_type
                .insert(ty, vec![0; self.build.nb_elements(ty)]);
        }
        loop {
            let line = self
                .src
                .expect_line("element MATERIALS (no 'MATERIALS' line)")?;
            let k = key(&line);
            if k.starts_with('\'') {
                self.build.int_data.insert("material".into(), data);
                return Ok(line);
            }
            if line.is_blank() {
                continue;
            }
            // "/ a-b c-d … / mat", intervals possibly continued on the next
            // lines until the closing '/'.
            let start = line.no();
            let mut text = match k.find('/') {
                Some(p) => k[p + 1..].to_owned(),
                None => k.to_owned(),
            };
            let (intervals, mat) = loop {
                if let Some(p) = text.find('/') {
                    let (iv, rest) = text.split_at(p);
                    let mat_tok = rest[1..].split_ascii_whitespace().next().unwrap_or("");
                    let mat = parse_i64(mat_tok)
                        .and_then(|m| i32::try_from(m).ok())
                        .ok_or_else(|| {
                            self.src.error_here(format!(
                                "material interval list starting at line {start}: invalid material number '{mat_tok}'"
                            ))
                        })?;
                    break (iv.to_owned(), mat);
                }
                let next = self.src.expect_line("material interval list")?;
                text.push(' ');
                text.push_str(key(&next));
            };
            for tok in intervals.split_ascii_whitespace() {
                let (a, b) = parse_range(tok).ok_or_else(|| {
                    self.src.error_here(format!(
                        "material interval list starting at line {start}: invalid interval '{tok}'"
                    ))
                })?;
                // Undefined element numbers are ignored (upstream).
                if a <= b {
                    for el in self.elements.range(a..=b).map(|(_, el)| *el) {
                        let v = data.per_type.entry(el.ty()).or_default();
                        let i = el.idx();
                        if i < v.len() {
                            v[i] = mat;
                        }
                    }
                }
            }
        }
    }

    /// `'MATERIALS'` section (upstream `readMaterial`), up to `'GROUPS'` or
    /// `'END'`.
    fn material_section(&mut self) -> Result<Option<Line<'a>>> {
        let mut out = DianaMaterials::default();
        let mut current: BTreeMap<String, f64> = BTreeMap::new();
        let mut first = true;
        let flush = |out: &mut DianaMaterials, current: &mut BTreeMap<String, f64>| {
            if !current.is_empty() {
                let name = format!("material{}", out.materials.len() + 1);
                out.materials.push(DianaMaterial {
                    name,
                    properties: std::mem::take(current),
                });
            }
        };
        loop {
            let mut line = self
                .src
                .expect_line("'MATERIALS' (no 'GROUPS' or 'END' line)")?;
            let k = key(&line);
            if k == "'GROUPS'" || k == "'END'" {
                flush(&mut out, &mut current);
                self.materials = Some(out);
                return Ok(Some(line));
            }
            if line.is_blank() {
                continue;
            }
            let probe = line.clone();
            let mut probe = probe;
            if let Some((_, t)) = probe.next_token()
                && parse_i64(t).is_some()
            {
                line.next_token();
                if !first {
                    flush(&mut out, &mut current);
                }
                first = false;
            }
            let Some((_, prop)) = line.next_token() else {
                continue;
            };
            let akantu = match prop {
                "YOUNG" => "E",
                "DENSIT" => "rho",
                "POISON" => "nu",
                _ => {
                    self.warnings.push(format!(
                        "{}:{}: In material reader, property {prop} not recognized",
                        self.src.name(),
                        line.no()
                    ));
                    continue;
                }
            };
            let v = line.f64(&format!("value of {prop}"))?;
            current.insert(akantu.to_owned(), v);
        }
    }

    /// `'GROUPS'` section (upstream `readGroups`), up to `'SUPPORTS'`.
    ///
    /// Node groups keep their name; an element group `X` also owns the node
    /// group `X_nodes` (upstream `createElementGroup`), so a node group of
    /// that name is a clash, like a repeated group name (upstream throws
    /// "already exists" for both).
    fn groups(&mut self) -> Result<Option<Line<'a>>> {
        let mut nodes_mode = false;
        loop {
            let mut line = self.src.expect_line("'GROUPS' (no 'SUPPORTS' line)")?;
            match key(&line) {
                "'SUPPORTS'" => return Ok(Some(line)),
                "NODES" => {
                    nodes_mode = true;
                    continue;
                }
                "ELEMEN" => {
                    nodes_mode = false;
                    continue;
                }
                _ if line.is_blank() => continue,
                _ => {}
            }
            line.i64("group number")?;
            let (ncol, name) = line.token("group name")?;
            let name = name.to_owned();
            if let Some(clash) = self.group_clash(&name, nodes_mode) {
                return Err(line.error_at(ncol, clash));
            }
            // Upstream reads one separator character (normally '/').
            let rest = line.rest();
            let sep = rest.chars().next().map_or(0, char::len_utf8);
            let mut text: &'a str = &rest[sep..];
            let mut text_line = line.no();
            let mut text_offset = offset_in(line.raw(), text);
            let mut ids: Vec<Interval> = Vec::new();
            loop {
                let found = ids.len();
                let scan = scan_intervals(text, &mut ids);
                for iv in &mut ids[found..] {
                    iv.line = text_line;
                    iv.column += text_offset;
                }
                match scan {
                    Scan::Terminated => break,
                    Scan::Malformed(at) => {
                        return Err(parse_error(
                            self.src.name(),
                            text_line,
                            text_offset + offset_in(text, at) + 1,
                            format!("group '{name}': malformed interval near '{at}'"),
                        ));
                    }
                    Scan::EndOfLine => {
                        let next = self
                            .src
                            .expect_line(&format!("interval list of group '{name}'"))?;
                        text = key(&next);
                        text_line = next.no();
                        text_offset = offset_in(next.raw(), text);
                    }
                }
            }
            if nodes_mode {
                // Every listed node must exist: expanding a range stops at
                // its first undefined number.
                let mut nodes = Vec::new();
                for iv in ids {
                    let mut id = iv.first;
                    while id <= iv.last {
                        let idx = self.node_ids.get(&id).copied().ok_or_else(|| {
                            parse_error(
                                self.src.name(),
                                iv.line,
                                iv.column,
                                format!(
                                    "node group '{name}': node {id} is not defined in 'COORDINATES'"
                                ),
                            )
                        })?;
                        nodes.push(idx);
                        match id.checked_add(1) {
                            Some(next) => id = next,
                            None => break,
                        }
                    }
                }
                self.build.node_groups.insert(name, nodes);
            } else {
                self.build.group_mut(&name, None);
                // Undefined element numbers are skipped (upstream).
                for iv in ids {
                    if iv.first > iv.last {
                        continue;
                    }
                    let els: Vec<Element> = self
                        .elements
                        .range(iv.first..=iv.last)
                        .map(|(_, e)| *e)
                        .collect();
                    for el in els {
                        self.build.group_add(&name, el.ty(), el.index(), false);
                    }
                }
                self.build.group_mut(&name, None).optimize();
            }
        }
    }

    /// Why a new node group (`nodes`) or element group `name` cannot be
    /// created, if it cannot.
    fn group_clash(&self, name: &str, nodes: bool) -> Option<String> {
        if nodes {
            if self.build.node_groups.contains_key(name) {
                return Some(format!("duplicate node group '{name}'"));
            }
            if let Some(owner) = name.strip_suffix("_nodes")
                && self.build.element_groups.contains_key(owner)
            {
                return Some(format!(
                    "node group '{name}' clashes with the node group of element group '{owner}'"
                ));
            }
        } else {
            if self.build.element_groups.contains_key(name) {
                return Some(format!("duplicate element group '{name}'"));
            }
            let owned = format!("{name}_nodes");
            if self.build.node_groups.contains_key(&owned) {
                return Some(format!(
                    "element group '{name}' needs the node group '{owned}', which exists already"
                ));
            }
        }
        None
    }
}

/// Byte offset of `sub` (a subslice of `whole`) in `whole`.
fn offset_in(whole: &str, sub: &str) -> usize {
    (sub.as_ptr() as usize).saturating_sub(whole.as_ptr() as usize)
}

/// An interval of a group's list with its location (1-based column of its
/// first character).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Interval {
    first: i64,
    last: i64,
    line: usize,
    column: usize,
}

/// `a-b` or `a` → inclusive range.
fn parse_range(tok: &str) -> Option<(i64, i64)> {
    if let Some(a) = parse_i64(tok) {
        return Some((a, a));
    }
    let body = tok.strip_prefix(['+', '-']).unwrap_or(tok);
    let split = body.find('-')? + (tok.len() - body.len());
    let (a, b) = (parse_i64(&tok[..split])?, parse_i64(&tok[split + 1..])?);
    Some((a, b))
}

enum Scan<'s> {
    /// A non-number ended the list (rest of the line ignored).
    Terminated,
    /// The line ended inside the list: it continues on the next line.
    EndOfLine,
    /// `a-` without a second number (the text from `a` on).
    Malformed(&'s str),
}

/// Upstream `readInterval` loop over one line: integers, `a-b` ranges (the
/// dash directly after `a`), anything else terminates the list. Ranges are
/// returned unexpanded (`a` → `a-a`), with `line = 0` and `column` = 1-based
/// column of the interval in `text`.
fn scan_intervals<'s>(text: &'s str, ids: &mut Vec<Interval>) -> Scan<'s> {
    let b = text.as_bytes();
    let mut i = 0;
    let int_at = |i: &mut usize| -> Option<i64> {
        let start = *i;
        let mut j = *i;
        if j < b.len() && (b[j] == b'+' || b[j] == b'-') {
            j += 1;
        }
        let d0 = j;
        while j < b.len() && b[j].is_ascii_digit() {
            j += 1;
        }
        if j == d0 {
            return None;
        }
        *i = j;
        text[start..j].parse().ok()
    };
    loop {
        while i < b.len() && super::lexer::is_c_space(b[i]) {
            i += 1;
        }
        if i >= b.len() {
            return Scan::EndOfLine;
        }
        let column = i + 1;
        let Some(first) = int_at(&mut i) else {
            return Scan::Terminated;
        };
        if i < b.len() && b[i] == b'-' {
            let at = &text[column - 1..];
            i += 1;
            while i < b.len() && super::lexer::is_c_space(b[i]) {
                i += 1;
            }
            let Some(second) = int_at(&mut i) else {
                return Scan::Malformed(at);
            };
            ids.push(Interval {
                first,
                last: second,
                line: 0,
                column,
            });
        } else {
            ids.push(Interval {
                first,
                last: first,
                line: 0,
                column,
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ranges(v: &[Interval]) -> Vec<(i64, i64, usize)> {
        v.iter().map(|i| (i.first, i.last, i.column)).collect()
    }

    #[test]
    fn intervals() {
        let mut v = Vec::new();
        assert!(matches!(
            scan_intervals(" 1-3 7 /", &mut v),
            Scan::Terminated
        ));
        assert_eq!(ranges(&v), vec![(1, 3, 2), (7, 7, 6)]);
        let mut v = Vec::new();
        assert!(matches!(scan_intervals("4-5", &mut v), Scan::EndOfLine));
        assert!(matches!(scan_intervals(" 9/ 3", &mut v), Scan::Terminated));
        assert_eq!(ranges(&v), vec![(4, 5, 1), (9, 9, 2)]);
        assert!(matches!(
            scan_intervals("1-/", &mut Vec::new()),
            Scan::Malformed(_)
        ));
        assert_eq!(parse_range("3-6"), Some((3, 6)));
        assert_eq!(parse_range("8"), Some((8, 8)));
        assert_eq!(parse_range("x"), None);
        assert_eq!(parse_range("1-"), None);
    }

    #[test]
    fn material_file_text() {
        let m = DianaMaterials {
            materials: vec![DianaMaterial {
                name: "material1".into(),
                properties: [
                    ("E".into(), 2.1e11),
                    ("nu".into(), 0.3),
                    ("rho".into(), 7800.0),
                ]
                .into_iter()
                .collect(),
            }],
        };
        assert_eq!(
            m.to_input_file(),
            "material elastic [\n\tname = material1\n\tE = 2.1e+11\n\tnu = 0.3\n\trho = 7800\n]\n"
        );
        assert_eq!(
            material_file_path(Path::new("dir/m.diana")),
            PathBuf::from("dir/material_m.diana")
        );
        assert_eq!(shortest_g(0.1 + 0.2), "0.30000000000000004");
    }
}
