// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test helpers for
// the dumper differential tests against the oracle fixtures written by
// oracle/probes/dump/dump_probe.cc (fixtures/dump/<case>/).

#![allow(dead_code)] // each test binary uses a subset of the helpers

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use akantu_core::{ElementType, ElementTypeMapArray, GhostType, Int, Real};
use akantu_mesh::{Mesh, MeshAccessor, MeshDataValues};

/// `fixtures/dump`.
pub fn fixtures() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/dump")
}

/// A fresh scratch directory for one test (below `$AKANTU_DUMP_EXPORT`
/// when set, so that the outputs can be checked by an external reader:
/// `oracle/probes/dump/check.sh`).
pub fn scratch(name: &str) -> PathBuf {
    let d = match std::env::var_os("AKANTU_DUMP_EXPORT") {
        Some(root) => PathBuf::from(root).join(name),
        None => std::env::temp_dir().join(format!("akantu-io-{name}-{}", std::process::id())),
    };
    let _ = std::fs::remove_dir_all(&d);
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// Every file below `dir`, relative, sorted.
pub fn files_below(dir: &Path) -> Vec<String> {
    fn walk(base: &Path, d: &Path, out: &mut Vec<String>) {
        for e in std::fs::read_dir(d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                walk(base, &p, out);
            } else {
                out.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    let mut v = Vec::new();
    if dir.exists() {
        walk(dir, dir, &mut v);
    }
    v.sort();
    v
}

/// Byte comparison with a readable first difference.
pub fn assert_same_file(got: &Path, want: &Path) {
    let g = std::fs::read(got).unwrap_or_else(|e| panic!("{got:?}: {e}"));
    let w = std::fs::read(want).unwrap_or_else(|e| panic!("{want:?}: {e}"));
    if g != w {
        let i = g
            .iter()
            .zip(&w)
            .position(|(a, b)| a != b)
            .unwrap_or(g.len().min(w.len()));
        let ctx = |b: &[u8]| {
            String::from_utf8_lossy(&b[i.saturating_sub(80)..(i + 80).min(b.len())]).into_owned()
        };
        panic!(
            "{got:?} differs from {want:?} at byte {i} (len {} vs {}):\n--- got\n{}\n--- want\n{}",
            g.len(),
            w.len(),
            ctx(&g),
            ctx(&w)
        );
    }
}

/// Compare the tree written below `got` with the oracle tree `want`
/// (same file set, byte-identical files).
pub fn assert_same_tree(got: &Path, want: &Path) {
    let g = files_below(got);
    let w = files_below(want);
    assert_eq!(g, w, "file sets differ between {got:?} and {want:?}");
    for f in &g {
        assert_same_file(&got.join(f), &want.join(f));
    }
}

/// `inputs.txt` of a probe case.
#[derive(Debug, Default)]
pub struct Inputs {
    arrays: HashMap<String, (usize, usize, Vec<String>)>,
    values: HashMap<String, String>,
}

impl Inputs {
    pub fn load(case: &str) -> Self {
        let text = std::fs::read_to_string(fixtures().join(case).join("inputs.txt")).unwrap();
        let mut me = Self::default();
        let mut lines = text.lines();
        while let Some(l) = lines.next() {
            let w: Vec<&str> = l.split_whitespace().collect();
            match w.first() {
                Some(&"array") => {
                    let (n, c): (usize, usize) = (w[2].parse().unwrap(), w[3].parse().unwrap());
                    let mut vals = Vec::with_capacity(n * c);
                    for _ in 0..n {
                        vals.extend(lines.next().unwrap().split_whitespace().map(str::to_owned));
                    }
                    me.arrays.insert(w[1].to_owned(), (n, c, vals));
                }
                Some(&"value") => {
                    me.values.insert(w[1].to_owned(), w[2].to_owned());
                }
                _ => {}
            }
        }
        me
    }

    fn get(&self, name: &str) -> &(usize, usize, Vec<String>) {
        self.arrays
            .get(name)
            .unwrap_or_else(|| panic!("no input array {name}"))
    }

    pub fn shape(&self, name: &str) -> (usize, usize) {
        let (n, c, _) = self.get(name);
        (*n, *c)
    }

    pub fn reals(&self, name: &str) -> Vec<Real> {
        self.get(name)
            .2
            .iter()
            .map(|h| f64::from_bits(u64::from_str_radix(h, 16).unwrap()))
            .collect()
    }

    pub fn ints(&self, name: &str) -> Vec<Int> {
        self.get(name)
            .2
            .iter()
            .map(|s| s.parse().unwrap())
            .collect()
    }

    pub fn uints(&self, name: &str) -> Vec<u32> {
        self.get(name)
            .2
            .iter()
            .map(|s| s.parse().unwrap())
            .collect()
    }

    pub fn bools(&self, name: &str) -> Vec<bool> {
        self.get(name).2.iter().map(|s| s != "0").collect()
    }

    pub fn value(&self, name: &str) -> Real {
        f64::from_bits(u64::from_str_radix(&self.values[name], 16).unwrap())
    }

    pub fn has(&self, name: &str) -> bool {
        self.arrays.contains_key(name)
    }
}

/// The 2-D mixed mesh of the probe (`build_m2`).
pub fn m2_mesh(with_segments: bool) -> Mesh {
    let mut m = Mesh::new(2, "m2").unwrap();
    {
        let mut a = MeshAccessor::new(&mut m);
        for x in [
            [0.0, 0.0],
            [1.0, 0.0],
            [2.0, 0.0],
            [0.0, 1.0],
            [1.0 / 3.0, 1.0],
            [2.0, 1.1],
            [0.5, 2.0],
            [1.5, -2.5e-7],
        ] {
            a.push_node(&x).unwrap();
        }
        let mut q = a.connectivity_mut(ElementType::Quadrangle4, GhostType::NotGhost);
        q.push_tuple(&[0, 1, 4, 3]);
        q.push_tuple(&[1, 2, 5, 4]);
        drop(q);
        let mut t = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        t.push_tuple(&[3, 4, 6]);
        t.push_tuple(&[4, 7, 5]);
        t.push_tuple(&[4, 7, 6]);
        drop(t);
        if with_segments {
            let mut s = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
            s.push_tuple(&[0, 1]);
            s.push_tuple(&[1, 2]);
        }
    }
    m
}

/// `add_tag_data` of the probe: Int mesh data `tag_0 = 10·type + e`.
pub fn add_tag_data(m: &mut Mesh) {
    let mut tag = ElementTypeMapArray::<Int>::new("tag_0");
    for ty in [
        ElementType::Segment2,
        ElementType::Triangle3,
        ElementType::Quadrangle4,
    ] {
        let n = m.nb_element(ty, GhostType::NotGhost);
        let a = tag.alloc(n, 1, ty, GhostType::NotGhost, 0).unwrap();
        for e in 0..n {
            a.as_mut_slice()[e] = 10 * (ty as Int) + Int::try_from(e).unwrap();
        }
    }
    m.data_mut()
        .insert_elemental("tag_0", MeshDataValues::Int(tag.into()));
}

/// A probe element-type key (`<name>:<C++ enum value>`).
pub fn key(name: &str, ty: ElementType) -> String {
    format!("{name}:{}", ty as usize)
}

/// One element of an XML document read by [`xml_elements`].
#[derive(Debug, Clone)]
pub struct XmlElement {
    pub name: String,
    /// Attributes in document order, references resolved.
    pub attrs: Vec<(String, String)>,
    /// Character data up to the next tag, references resolved.
    pub text: String,
}

impl XmlElement {
    pub fn attr(&self, name: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == name)
            .map(|(_, v)| v.as_str())
    }
}

fn xml_unescape(s: &str) -> Result<String, String> {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(i) = rest.find('&') {
        out.push_str(&rest[..i]);
        let end = rest[i..]
            .find(';')
            .ok_or_else(|| format!("unterminated reference in {s:?}"))?
            + i;
        let r = &rest[i + 1..end];
        let c = match r {
            "amp" => '&',
            "lt" => '<',
            "gt" => '>',
            "quot" => '"',
            "apos" => '\'',
            _ => {
                let code = if let Some(h) = r.strip_prefix("#x") {
                    u32::from_str_radix(h, 16)
                } else if let Some(d) = r.strip_prefix('#') {
                    d.parse()
                } else {
                    return Err(format!("unknown entity &{r}; in {s:?}"));
                }
                .map_err(|e| format!("bad character reference &{r};: {e}"))?;
                char::from_u32(code).ok_or_else(|| format!("bad character reference &{r};"))?
            }
        };
        out.push(c);
        rest = &rest[end + 1..];
    }
    out.push_str(rest);
    Ok(out)
}

/// Strict reader for the XML subset written by the ParaView dumper
/// (declarations, start/end/empty tags, double-quoted attributes, text):
/// checks well-formedness (one root, balanced tags, legal names, no `<`
/// or bare `&` in attribute values or text, legal characters, unique
/// attributes) and returns the elements in document order. Attribute
/// values are normalised as XML requires (literal tab/LF/CR → space).
pub fn xml_elements(doc: &str) -> Result<Vec<XmlElement>, String> {
    let legal = |c: char| matches!(c, '\t' | '\n' | '\r' | '\u{20}'..='\u{D7FF}' | '\u{E000}'..='\u{FFFD}' | '\u{10000}'..);
    if let Some(c) = doc.chars().find(|&c| !legal(c)) {
        return Err(format!("illegal character U+{:04X}", u32::from(c)));
    }
    let is_name_start = |c: char| c.is_ascii_alphabetic() || c == '_' || c == ':';
    let is_name = |c: char| is_name_start(c) || c.is_ascii_digit() || c == '-' || c == '.';
    let mut out: Vec<XmlElement> = Vec::new();
    let mut stack: Vec<String> = Vec::new();
    let mut roots = 0;
    let mut rest = doc;
    let mut text_target: Option<usize> = None;
    loop {
        let i = rest.find('<').unwrap_or(rest.len());
        let text = &rest[..i];
        if stack.is_empty() && !text.trim().is_empty() {
            return Err(format!("text outside the root: {text:?}"));
        }
        let t = xml_unescape(text)?;
        if let Some(k) = text_target {
            out[k].text.push_str(&t);
        }
        if i == rest.len() {
            break;
        }
        rest = &rest[i..];
        if let Some(r) = rest.strip_prefix("<?") {
            let e = r.find("?>").ok_or("unterminated declaration")?;
            rest = &r[e + 2..];
            continue;
        }
        if let Some(r) = rest.strip_prefix("</") {
            let e = r.find('>').ok_or("unterminated end tag")?;
            let name = r[..e].trim_end();
            match stack.pop() {
                Some(open) if open == name => {}
                other => return Err(format!("end tag </{name}> closes {other:?}")),
            }
            rest = &r[e + 1..];
            text_target = None;
            continue;
        }
        let r = &rest[1..];
        let n = r.find(|c: char| !is_name(c)).unwrap_or(r.len());
        let name = &r[..n];
        if name.is_empty() || !name.starts_with(is_name_start) {
            return Err(format!("bad tag name in {:?}", &rest[..rest.len().min(40)]));
        }
        let mut r = &r[n..];
        let mut attrs: Vec<(String, String)> = Vec::new();
        let empty;
        loop {
            let trimmed = r.trim_start_matches([' ', '\t', '\n', '\r']);
            let had_space = trimmed.len() != r.len();
            r = trimmed;
            if let Some(x) = r.strip_prefix("/>") {
                r = x;
                empty = true;
                break;
            }
            if let Some(x) = r.strip_prefix('>') {
                r = x;
                empty = false;
                break;
            }
            if !had_space {
                return Err(format!("missing space before attribute in <{name}"));
            }
            let k = r.find(|c: char| !is_name(c)).unwrap_or(r.len());
            let key = &r[..k];
            if key.is_empty() || !key.starts_with(is_name_start) {
                return Err(format!("bad attribute name in <{name}"));
            }
            r = r[k..].trim_start();
            r = r
                .strip_prefix('=')
                .ok_or_else(|| format!("attribute {key} without '='"))?
                .trim_start();
            r = r
                .strip_prefix('"')
                .ok_or_else(|| format!("attribute {key} not double-quoted"))?;
            let e = r
                .find('"')
                .ok_or_else(|| format!("unterminated attribute {key}"))?;
            let raw = &r[..e];
            if raw.contains('<') {
                return Err(format!("'<' in attribute {key}={raw:?}"));
            }
            let normalised: String = raw
                .chars()
                .map(|c| {
                    if matches!(c, '\t' | '\n' | '\r') {
                        ' '
                    } else {
                        c
                    }
                })
                .collect();
            let value = xml_unescape(&normalised)?;
            if attrs.iter().any(|(a, _)| a == key) {
                return Err(format!("duplicate attribute {key} in <{name}"));
            }
            attrs.push((key.to_owned(), value));
            r = &r[e + 1..];
        }
        if stack.is_empty() {
            roots += 1;
        }
        out.push(XmlElement {
            name: name.to_owned(),
            attrs,
            text: String::new(),
        });
        if empty {
            text_target = None;
        } else {
            stack.push(name.to_owned());
            text_target = Some(out.len() - 1);
        }
        rest = r;
    }
    if !stack.is_empty() {
        return Err(format!("unclosed elements {stack:?}"));
    }
    if roots != 1 {
        return Err(format!("{roots} root elements"));
    }
    Ok(out)
}
