// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Reader of the oracle
// runs of the upstream examples (fixtures/examples/<case>/, written by
// oracle/probes/examples/collect.py) and the comparisons of the example
// integration tests: file digests, per-array VTU payload digests, kept
// files, 17-digit CSV values.

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use super::{base64_decode, files_below, fixtures, sha256};

/// Digest record of one VTU data array of the oracle.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ArrayDigest {
    /// VTU type (`Float64`, …).
    pub ty: String,
    /// `NumberOfComponents`.
    pub nb_component: usize,
    /// Number of values.
    pub nb_values: i64,
    /// SHA-256 of the decoded payload.
    pub sha256: String,
    /// Identical in every oracle run.
    pub deterministic: bool,
}

/// One oracle case (`fixtures/examples/<case>`).
#[derive(Debug)]
pub struct OracleCase {
    /// Case directory.
    pub dir: PathBuf,
    /// Output file → SHA-256 (run 1).
    pub digests: BTreeMap<String, String>,
    /// Output files that differ between the oracle runs.
    pub nondeterministic: BTreeSet<String>,
    /// `(file, array)` → digest record.
    pub arrays: BTreeMap<(String, String), ArrayDigest>,
}

/// Open the oracle case `name` (panics if absent: a missing fixture is a
/// failure, never a skip).
pub fn oracle(name: &str) -> OracleCase {
    let dir = fixtures().join(name);
    let read = |f: &str| {
        std::fs::read_to_string(dir.join(f))
            .unwrap_or_else(|e| panic!("oracle case {name}: {f}: {e}"))
    };
    let digests = read("digests.txt")
        .lines()
        .map(|l| {
            let (h, p) = l.split_once("  ").expect("`<sha256>  <path>`");
            (p.to_owned(), h.to_owned())
        })
        .collect();
    let nondeterministic = std::fs::read_to_string(dir.join("NONDETERMINISTIC"))
        .map(|t| t.lines().map(str::to_owned).collect())
        .unwrap_or_default();
    // absent for the cases recorded without per-array digests (`arrays` no)
    let arrays = std::fs::read_to_string(dir.join("vtu_arrays.txt"))
        .unwrap_or_default()
        .lines()
        .map(|l| {
            let c: Vec<&str> = l.split('\t').collect();
            assert_eq!(c.len(), 7, "vtu_arrays.txt line {l}");
            (
                (c[0].to_owned(), c[1].to_owned()),
                ArrayDigest {
                    ty: c[2].to_owned(),
                    nb_component: c[3].parse().unwrap(),
                    nb_values: c[4].parse().unwrap(),
                    sha256: c[5].to_owned(),
                    deterministic: c[6] == "det",
                },
            )
        })
        .collect();
    OracleCase {
        dir,
        digests,
        nondeterministic,
        arrays,
    }
}

/// `(name, type, components, values, payload)` of every data array of a
/// VTU file (binary arrays only; ASCII arrays as their normalised text).
pub fn vtu_payloads(path: &Path) -> Vec<(String, String, usize, usize, Vec<u8>)> {
    let doc = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path:?}: {e}"));
    let attr = |tag: &str, name: &str| -> Option<String> {
        let key = format!(" {name}=\"");
        let i = tag.find(&key).map(|i| i + key.len()).or_else(|| {
            let key = format!("{name}=\"");
            tag.find(&key).map(|i| i + key.len())
        })?;
        Some(tag[i..i + tag[i..].find('"')?].to_owned())
    };
    let mut out = Vec::new();
    let mut rest = doc.as_str();
    while let Some(i) = rest.find("<DataArray") {
        rest = &rest[i..];
        let j = rest.find('>').unwrap();
        let tag = &rest[..j];
        let k = rest.find("</DataArray>").unwrap();
        let body = &rest[j + 1..k];
        let ty = attr(tag, "type").unwrap();
        let name = attr(tag, "Name").unwrap_or_default();
        let comps = attr(tag, "NumberOfComponents").map_or(1, |c| c.parse().unwrap());
        let (payload, n) = if attr(tag, "format").as_deref().unwrap_or("ascii") == "ascii" {
            let tokens: Vec<&str> = body.split_whitespace().collect();
            (tokens.join(" ").into_bytes(), tokens.len())
        } else {
            let raw = base64_decode(body).unwrap();
            let width = match ty.as_str() {
                "Float64" | "Int64" | "UInt64" => 8,
                "Int32" | "UInt32" | "Float32" => 4,
                _ => 1,
            };
            let payload = raw[4..].to_vec();
            let n = payload.len() / width;
            (payload, n)
        };
        out.push((name, ty, comps, n, payload));
        rest = &rest[k..];
    }
    out
}

impl OracleCase {
    /// A file kept verbatim by the oracle run.
    pub fn kept(&self, rel: &str) -> PathBuf {
        let p = self.dir.join("files").join(rel);
        assert!(p.exists(), "oracle kept file {p:?}");
        p
    }

    /// Lines of a kept text file.
    pub fn kept_lines(&self, rel: &str) -> Vec<String> {
        std::fs::read_to_string(self.kept(rel))
            .unwrap()
            .lines()
            .map(str::to_owned)
            .collect()
    }

    /// Every file written below `out` (relative path, `skip` excluded) is
    /// an oracle output with the same SHA-256; with `complete`, the oracle
    /// wrote no other file (outside `skip`). Returns the number of files
    /// compared.
    pub fn assert_files_match(
        &self,
        out: &Path,
        complete: bool,
        skip: &dyn Fn(&str) -> bool,
    ) -> usize {
        let got: Vec<String> = files_below(out).into_iter().filter(|f| !skip(f)).collect();
        assert!(!got.is_empty(), "no output below {out:?}");
        for f in &got {
            let want = self
                .digests
                .get(f)
                .unwrap_or_else(|| panic!("{f}: not an output of the oracle run ({:?})", self.dir));
            assert!(
                !self.nondeterministic.contains(f),
                "{f} is nondeterministic upstream"
            );
            assert_eq!(&sha256(&out.join(f)), want, "{f} differs from the oracle's");
        }
        if complete {
            let want: Vec<&String> = self.digests.keys().filter(|f| !skip(f)).collect();
            let have: Vec<&String> = got.iter().collect();
            assert_eq!(have, want, "file sets differ from the oracle's");
        }
        got.len()
    }

    /// Every data array of the VTU file `rel` written below `out` matches
    /// the oracle's payload digest, except the arrays for which `skip`
    /// returns true; nondeterministic oracle arrays must be skipped.
    /// Returns the number of arrays compared.
    pub fn assert_vtu_arrays_match(
        &self,
        out: &Path,
        rel: &str,
        skip: &dyn Fn(&str) -> bool,
    ) -> usize {
        let mut n = 0;
        for (name, ty, comps, nvalues, payload) in vtu_payloads(&out.join(rel)) {
            if skip(&name) {
                continue;
            }
            let want = self
                .arrays
                .get(&(rel.to_owned(), name.clone()))
                .unwrap_or_else(|| panic!("{rel}: array {name} not in the oracle's"));
            assert!(
                want.deterministic,
                "{rel}: {name} is nondeterministic upstream: skip it"
            );
            assert_eq!(
                (&ty, comps, nvalues as i64),
                (&want.ty, want.nb_component, want.nb_values),
                "{rel}: {name} layout"
            );
            assert_eq!(
                akantu_validation::checksum::sha256_bytes(&payload),
                want.sha256,
                "{rel}: {name} differs from the oracle's"
            );
            n += 1;
        }
        n
    }
}

/// Standard output of the oracle run (`stdout.txt`: run 1, first 200 kB).
impl OracleCase {
    /// The recorded standard output.
    pub fn stdout(&self) -> String {
        std::fs::read_to_string(self.dir.join("stdout.txt"))
            .unwrap_or_else(|e| panic!("{:?}: stdout.txt: {e}", self.dir))
    }
}

/// `got` (an example's transcript, `Example::transcript`) equals the
/// oracle's standard output `want` line by line: all of it
/// (`lines = None`) or its first `lines` lines (reduced runs, whose later
/// lines name the reduced step count).
pub fn assert_stdout(what: &str, got: &str, want: &str, lines: Option<usize>) {
    let g: Vec<&str> = got.split('\n').collect();
    let w: Vec<&str> = want.split('\n').collect();
    let n = lines.unwrap_or(w.len());
    assert!(
        g.len() >= n && w.len() >= n,
        "{what}: {} / {} lines, {n} compared",
        g.len(),
        w.len()
    );
    for k in 0..n {
        assert_eq!(g[k], w[k], "{what}: standard output line {}", k + 1);
    }
    if lines.is_none() {
        assert_eq!(g.len(), w.len(), "{what}: number of output lines");
    }
}

/// Bitwise comparison of the values of `got` and `want` (decimal texts,
/// 17 significant digits round-trip exactly).
pub fn assert_csv_values_bitwise(what: &str, got: &[f64], want: &[&str]) {
    assert_eq!(got.len(), want.len(), "{what}: number of values");
    for (k, (g, w)) in got.iter().zip(want).enumerate() {
        let w: f64 = w.parse().unwrap_or_else(|e| panic!("{what}: {w}: {e}"));
        assert_eq!(
            g.to_bits(),
            w.to_bits(),
            "{what}[{k}]: {g:e} vs oracle {w:e}"
        );
    }
}

/// SHA-256 (lowercase hex) of the raw little-endian bytes of `arrays`,
/// concatenated in order (the `state_sha` of the oracle variants,
/// `oracle/probes/examples/*_full.cc`, `*_eigen.cc`).
pub fn state_sha(arrays: &[&[f64]]) -> String {
    let mut bytes = Vec::new();
    for a in arrays {
        for v in *a {
            bytes.extend_from_slice(&v.to_le_bytes());
        }
    }
    akantu_validation::checksum::sha256_bytes(&bytes)
}

/// The kept CSV `rel` of the oracle case as records: header checked, one
/// `Vec<String>` per line, split on `sep`.
pub fn kept_records(o: &OracleCase, rel: &str, header: &str, sep: char) -> Vec<Vec<String>> {
    let lines = o.kept_lines(rel);
    assert_eq!(lines[0], header, "{rel}: header");
    lines[1..]
        .iter()
        .map(|l| l.split(sep).map(str::to_owned).collect())
        .collect()
}

/// The recorded standard error of the oracle run (`stderr.txt`, run 1,
/// without the runner's cache-pin lines; cases).
impl OracleCase {
    /// The recorded standard error.
    pub fn stderr(&self) -> String {
        std::fs::read_to_string(self.dir.join("stderr.txt"))
            .unwrap_or_else(|e| panic!("{:?}: stderr.txt: {e}", self.dir))
    }
}

/// `text` with every number (a run of decimal digits and `.`, with the
/// spaces before it, which pad it to a field width) replaced by `#`
/// (wall-clock timings of the progress lines; phase-field
/// examples).
pub fn mask_numbers(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let mut k = 0;
    while k < chars.len() {
        let start = k;
        while k < chars.len() && chars[k] == ' ' {
            k += 1;
        }
        if k < chars.len() && chars[k].is_ascii_digit() {
            while k < chars.len() && (chars[k].is_ascii_digit() || chars[k] == '.') {
                k += 1;
            }
            out.push('#');
        } else {
            out.extend(&chars[start..k]);
            if k < chars.len() {
                out.push(chars[k]);
                k += 1;
            }
        }
    }
    out
}

/// The dumps (`paraview/…`) of the oracle variant `variant` are those of
/// the upstream binary's case `upstream`: same files, same SHA-256 (the
/// variant only adds records). Returns the number of files.
pub fn assert_variant_dumps_equal(upstream: &str, variant: &str) -> usize {
    let (u, v) = (oracle(upstream), oracle(variant));
    let dumps = |o: &OracleCase| -> BTreeMap<String, String> {
        o.digests
            .iter()
            .filter(|(f, _)| f.starts_with("paraview/"))
            .map(|(f, h)| (f.clone(), h.clone()))
            .collect()
    };
    let (a, b) = (dumps(&u), dumps(&v));
    assert!(!a.is_empty(), "{upstream}: no dump");
    assert_eq!(a, b, "the dumps of {variant} differ from {upstream}'s");
    a.len()
}
