// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Accounting
// of the upstream test meshes, input files and golden files (parity rows
// GOLD-001…015, MDATA-001…006); no C++ counterpart. The manifest is generated
// from the reference tree by scripts/gold/upstream_data_manifest.py.

//! Checks `tests/data/upstream_data_manifest.json` against this repository,
//! without the reference checkout. The manifest records what only the
//! reference can tell (the upstream data files, their SHA-256 and sizes,
//! whether a `.geo` has a build rule); everything that depends on this
//! repository is **re-derived here** and must equal the committed manifest
//! (the accounting cannot go stale silently):
//!
//! * `copies`: the git-visible files of the manifest's `copy_roots` are
//!   hashed (those whose size is the size of some upstream data file) and
//!   every file whose SHA-256 is that of an upstream data file must be
//!   listed, every listed copy must exist with that SHA-256 (a modified,
//!   removed or unlisted copy fails);
//! * `frozen`: the meshes of `fixtures/meshes/frozen_meshes.json` (D-009) of
//!   each `.geo`, with their SHA-256 checked on disk;
//! * `consumers`: the appendix of `docs/inventory/tests_examples_python.md`
//!   (or the manifest's `extra_consumers`), plus the successors (`split →
//!   …` in the Notes cell of `docs/FEATURE_PARITY.md`) of every consumer TEST/EX/PY row that
//!   is `Superseded`; a file without a consumer row, or a
//!   superseded row without a successor, fails;
//! * `ported_consumers` (the consumer TEST/EX/PY rows that are `In
//!   progress`, `Implemented, unverified` or `Verified` in
//!   `docs/FEATURE_PARITY.md`) and the `status` of every file; a file
//!   consumed by a ported row that is not in the repository (`owed`) fails
//!   unless it carries a written note;
//! * every file under a `crates/*/tests/data/upstream/` tree and under
//!   `crates/akantu-fe/tests/data/inradius/` is a verbatim upstream copy
//!   with a row (file name, upstream path, **computed** SHA-256) in the
//!   nearest `README.md`, and every README row names a file that exists.
//!
//! On a failure, regenerate the manifest with
//! `python3 scripts/gold/upstream_data_manifest.py` (needs the reference
//! checkout; `--check` shows the differences) in the commit that changes
//! the data (docs/TESTING.md "Upstream golden files and mesh data").

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};
use std::process::Command;

use serde_json::Value;
use sha2::{Digest, Sha256};

const REGENERATE: &str = "regenerate the manifest with `python3 scripts/gold/upstream_data_manifest.py` \
     (docs/TESTING.md, \"Upstream golden files and mesh data\")";

/// Statuses of a TEST/EX/PY row that count as ported.
const PORTED: [&str; 3] = ["In progress", "Implemented, unverified", "Verified"];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn manifest() -> Value {
    let p =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data/upstream_data_manifest.json");
    serde_json::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap()
}

fn read_text(rel: &str) -> String {
    let p = repo_root().join(rel);
    std::fs::read_to_string(&p).unwrap_or_else(|e| panic!("{}: {e}", p.display()))
}

fn sha256_file(p: &Path) -> String {
    let bytes = std::fs::read(p).unwrap_or_else(|e| panic!("{}: {e}", p.display()));
    Sha256::digest(&bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn str_vec(v: &Value) -> Vec<String> {
    v.as_array()
        .map(|a| a.iter().map(|s| s.as_str().unwrap().to_owned()).collect())
        .unwrap_or_default()
}

fn strs(v: &Value, key: &str) -> Vec<String> {
    v.get(key).map(str_vec).unwrap_or_default()
}

fn files(m: &Value) -> &Vec<Value> {
    m["files"].as_array().unwrap()
}

fn up(e: &Value) -> &str {
    e["upstream"].as_str().unwrap()
}

/// Files of this repository visible to git (tracked, or untracked and not
/// ignored) under `roots` that exist on disk, as the generator lists them
/// (`git ls-files --cached --others --exclude-standard`): ignored data is
/// not part of a clone and cannot serve as a copy.
fn git_visible_files(roots: &[String]) -> Vec<String> {
    let root = repo_root();
    let out = Command::new("git")
        .arg("-C")
        .arg(&root)
        .args([
            "ls-files",
            "-z",
            "--cached",
            "--others",
            "--exclude-standard",
            "--",
        ])
        .args(roots)
        .output()
        .expect("this test lists the git-visible files of the repository: `git` is needed");
    assert!(
        out.status.success(),
        "git ls-files failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let mut v: Vec<String> = String::from_utf8(out.stdout)
        .unwrap()
        .split('\0')
        .filter(|p| !p.is_empty() && root.join(p).is_file())
        .map(str::to_owned)
        .collect();
    v.sort();
    v.dedup();
    v
}

/// Copies of upstream data files on disk: SHA-256 → sorted repository
/// paths, over the git-visible files of the manifest's `copy_roots` (empty
/// files and files above `max_copy_bytes` excluded, as in the generator).
/// Only files whose size is that of an upstream data file are hashed.
fn copies_on_disk(m: &Value) -> BTreeMap<String, Vec<String>> {
    let roots = strs(m, "copy_roots");
    assert!(!roots.is_empty(), "copy_roots missing: {REGENERATE}");
    let max = m["max_copy_bytes"].as_u64().unwrap();
    let upstream_sha: BTreeSet<&str> = files(m)
        .iter()
        .map(|e| e["sha256"].as_str().unwrap())
        .collect();
    let sizes: BTreeSet<u64> = files(m)
        .iter()
        .map(|e| e["bytes"].as_u64().unwrap())
        .collect();
    let root = repo_root();
    let mut by_sha: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for rel in git_visible_files(&roots) {
        let size = std::fs::metadata(root.join(&rel)).unwrap().len();
        if size == 0 || size > max || !sizes.contains(&size) {
            continue;
        }
        let sha = sha256_file(&root.join(&rel));
        if upstream_sha.contains(sha.as_str()) {
            by_sha.entry(sha).or_default().push(rel);
        }
    }
    for v in by_sha.values_mut() {
        v.sort();
    }
    by_sha
}

// --- the generator's parsing of the inventory and the parity matrix ---------

fn is_word(c: char) -> bool {
    c.is_alphanumeric() || c == '_'
}

/// The IDs `\b[A-Z]+-\d{3}\b` of `text`, with their byte ranges.
fn ids_in(text: &str) -> Vec<(usize, usize, String)> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let prev_word = i > 0 && is_word(chars[i - 1].1);
        if prev_word || !chars[i].1.is_ascii_uppercase() {
            i += 1;
            continue;
        }
        let mut j = i;
        while j < chars.len() && chars[j].1.is_ascii_uppercase() {
            j += 1;
        }
        let digits = (j + 1..j + 4).all(|k| k < chars.len() && chars[k].1.is_ascii_digit());
        let after_ok = j + 4 >= chars.len() || !is_word(chars[j + 4].1);
        if j < chars.len() && chars[j].1 == '-' && digits && after_ok {
            let start = chars[i].0;
            let end = if j + 4 < chars.len() {
                chars[j + 4].0
            } else {
                text.len()
            };
            out.push((start, end, text[start..end].to_owned()));
            i = j + 4;
        } else {
            i = j.max(i + 1);
        }
    }
    out
}

/// IDs of `text` plus the members of its `A…B` ranges (same prefix), sorted
/// and unique (`expand_ranges` of the generator).
fn ids_with_ranges(text: &str) -> BTreeSet<String> {
    let ids = ids_in(text);
    let mut out: BTreeSet<String> = ids.iter().map(|(_, _, s)| s.clone()).collect();
    let mut k = 0;
    while k + 1 < ids.len() {
        let (_, a_end, a) = &ids[k];
        let (b_start, _, b) = &ids[k + 1];
        if &text[*a_end..*b_start] == "…" {
            let (pa, na) = a.rsplit_once('-').unwrap();
            let (pb, nb) = b.rsplit_once('-').unwrap();
            if pa == pb {
                let (na, nb): (u32, u32) = (na.parse().unwrap(), nb.parse().unwrap());
                for n in na..=nb {
                    out.insert(format!("{pa}-{n:03}"));
                }
            }
            k += 2;
        } else {
            k += 1;
        }
    }
    out
}

/// Appendix "file coverage" of the inventory: upstream path → consumer IDs.
fn appendix_consumers() -> BTreeMap<String, BTreeSet<String>> {
    let text = read_text("docs/inventory/tests_examples_python.md");
    let mut out = BTreeMap::new();
    let mut in_appendix = false;
    for line in text.lines() {
        if line.starts_with("## Appendix: file coverage") {
            in_appendix = true;
            continue;
        }
        if !in_appendix {
            continue;
        }
        // `^\| `([^`]+)` \| (.+) \|\s*$`
        let Some(rest) = line.strip_prefix("| `") else {
            continue;
        };
        let Some(tick) = rest.find('`') else {
            continue;
        };
        let (path, rest) = rest.split_at(tick);
        let Some(rest) = rest.strip_prefix("` | ") else {
            continue;
        };
        let Some(cell) = rest.trim_end().strip_suffix(" |") else {
            continue;
        };
        if path.is_empty() || cell.is_empty() {
            continue;
        }
        out.insert(path.to_owned(), ids_with_ranges(cell));
    }
    out
}

/// Cells of one Markdown table line (`re.split(r"(?<!\\)\|", line.strip())[1:-1]`).
fn table_cells(line: &str) -> Vec<String> {
    let line = line.trim();
    let mut cells = Vec::new();
    let mut cur = String::new();
    let mut prev = '\0';
    for c in line.chars() {
        if c == '|' && prev != '\\' {
            cells.push(std::mem::take(&mut cur));
        } else {
            cur.push(c);
        }
        prev = c;
    }
    cells.push(cur);
    if cells.len() < 2 {
        return Vec::new();
    }
    cells[1..cells.len() - 1]
        .iter()
        .map(|c| c.trim().to_owned())
        .collect()
}

fn is_row_id(s: &str) -> bool {
    let ids = ids_in(s);
    ids.len() == 1 && ids[0].0 == 0 && ids[0].1 == s.len()
}

/// Parity rows: ID → (Status, Notes) (`Discrepancies` in the matrix's
/// earlier layout).
fn parity_rows() -> BTreeMap<String, (String, String)> {
    let text = read_text("docs/FEATURE_PARITY.md");
    let mut rows = BTreeMap::new();
    let mut header: Option<Vec<String>> = None;
    for line in text.lines() {
        if line.starts_with("| ID |") {
            header = Some(table_cells(line));
            continue;
        }
        let Some(h) = &header else {
            continue;
        };
        if !line.starts_with("| ") {
            continue;
        }
        let cells = table_cells(line);
        if cells.is_empty() || !is_row_id(&cells[0]) {
            continue;
        }
        let cell = |name: &str| {
            h.iter()
                .position(|c| c == name)
                .and_then(|i| cells.get(i))
                .cloned()
                .unwrap_or_default()
        };
        let notes = if h.iter().any(|c| c == "Notes") {
            cell("Notes")
        } else {
            cell("Discrepancies")
        };
        rows.insert(cells[0].clone(), (cell("Status"), notes));
    }
    rows
}

fn is_superseded(status: &str) -> bool {
    status
        .trim_matches(|c| c == '*' || c == ' ')
        .starts_with("Superseded")
}

/// Successors of a `Superseded` row: the IDs and ranges after `→` in its
/// Status/Notes cells, recursively (the generator's `successors`).
fn successors(
    row: &str,
    rows: &BTreeMap<String, (String, String)>,
    seen: &BTreeSet<String>,
) -> BTreeSet<String> {
    let (status, disc) = rows.get(row).cloned().unwrap_or_default();
    let text = format!("{status} {disc}");
    let mut out = BTreeSet::new();
    let mut rest = text.as_str();
    while let Some(i) = rest.find('→') {
        let seg = rest[i + '→'.len_utf8()..].trim_start();
        let end = seg.find([';', '(']).unwrap_or(seg.len());
        let (seg, tail) = seg.split_at(end);
        for s in ids_with_ranges(seg) {
            if s == row || seen.contains(&s) {
                continue;
            }
            let sup = rows.get(&s).is_some_and(|(st, _)| is_superseded(st));
            out.insert(s.clone());
            if sup {
                let mut seen2 = seen.clone();
                seen2.insert(row.to_owned());
                out.extend(successors(&s, rows, &seen2));
            }
        }
        rest = tail;
    }
    out
}

// --- tests -------------------------------------------------------------------

#[test]
fn every_copy_is_byte_identical_to_its_upstream_file() {
    let m = manifest();
    assert_eq!(
        m["reference_sha"].as_str(),
        Some("35a851780d0b9471d258ed53a003ae98aaa95ea0")
    );
    let root = repo_root();
    let (mut copies, mut frozen) = (0, 0);
    let mut errors = Vec::new();
    for e in files(&m) {
        let sha = e["sha256"].as_str().unwrap();
        for c in strs(e, "copies") {
            let p = root.join(&c);
            if !p.is_file() {
                errors.push(format!(
                    "{c}: listed as a copy of {} but no longer exists",
                    up(e)
                ));
                continue;
            }
            copies += 1;
            let got = sha256_file(&p);
            if got != sha {
                errors.push(format!("{c}: {got} != upstream {} ({sha})", up(e)));
            }
        }
        for f in e
            .get("frozen")
            .and_then(Value::as_array)
            .into_iter()
            .flatten()
        {
            frozen += 1;
            let p = f["path"].as_str().unwrap();
            if !root.join(p).is_file() {
                errors.push(format!("{p}: frozen mesh of {} missing", up(e)));
            } else if sha256_file(&root.join(p)) != f["sha256"].as_str().unwrap() {
                errors.push(format!("{p}: frozen mesh changed"));
            }
        }
    }
    assert!(errors.is_empty(), "{}\n{REGENERATE}", errors.join("\n"));
    println!(
        "{} upstream files, {copies} verbatim copies and {frozen} frozen meshes checked",
        files(&m).len()
    );
    assert!(copies > 0 && frozen > 0);
}

/// Every git-visible file of `copy_roots` that is byte-identical to an
/// upstream data file is listed as its copy, and nothing else is.
#[test]
fn copies_on_disk_match_the_manifest() {
    let m = manifest();
    let on_disk = copies_on_disk(&m);
    let mut errors = Vec::new();
    let (mut pairs, mut distinct) = (0u64, BTreeSet::new());
    for e in files(&m) {
        let sha = e["sha256"].as_str().unwrap();
        let recorded: BTreeSet<String> = strs(e, "copies").into_iter().collect();
        let found: BTreeSet<String> = on_disk
            .get(sha)
            .map(|v| v.iter().cloned().collect())
            .unwrap_or_default();
        for c in found.difference(&recorded) {
            errors.push(format!("{c}: copy of {} not in the manifest", up(e)));
        }
        for c in recorded.difference(&found) {
            errors.push(format!(
                "{c}: listed as a copy of {} but not on disk with its SHA-256",
                up(e)
            ));
        }
        pairs += found.len() as u64;
        distinct.extend(found);
    }
    assert!(errors.is_empty(), "{}\n{REGENERATE}", errors.join("\n"));
    assert_eq!(
        (m["copy_pairs"].as_u64(), m["copy_files"].as_u64()),
        (Some(pairs), Some(distinct.len() as u64)),
        "copy counts of the manifest; {REGENERATE}"
    );
    println!(
        "{pairs} (upstream file, copy) pairs, {} distinct copy files",
        distinct.len()
    );
}

/// The frozen Gmsh outputs of every `.geo` are those of
/// `fixtures/meshes/frozen_meshes.json` (D-009).
#[test]
fn frozen_meshes_match_the_manifest() {
    let m = manifest();
    let frozen: Value = serde_json::from_str(&read_text("fixtures/meshes/frozen_meshes.json"))
        .expect("frozen_meshes.json");
    let mut by_geo: BTreeMap<String, Vec<Value>> = BTreeMap::new();
    let mut geo_sha: BTreeMap<String, String> = BTreeMap::new();
    for f in frozen["meshes"].as_array().unwrap() {
        let geo = f["geo"].as_str().unwrap().to_owned();
        geo_sha.insert(geo.clone(), f["geo_sha256"].as_str().unwrap().to_owned());
        by_geo.entry(geo).or_default().push(serde_json::json!({
            "path": format!("fixtures/meshes/{}", f["path"].as_str().unwrap()),
            "sha256": f["sha256"],
            "order": f["order"],
            "dim": f["dim"],
        }));
    }
    let entries: BTreeMap<&str, &Value> = files(&m).iter().map(|e| (up(e), e)).collect();
    let mut errors = Vec::new();
    for (geo, meshes) in &by_geo {
        match entries.get(geo.as_str()) {
            None => errors.push(format!(
                "{geo}: frozen meshes of a .geo not in the manifest"
            )),
            Some(e) => {
                if e["sha256"].as_str() != Some(geo_sha[geo].as_str()) {
                    errors.push(format!("{geo}: frozen_meshes.json geo SHA-256 differs"));
                }
                let recorded = e.get("frozen").cloned().unwrap_or(Value::Array(vec![]));
                if recorded != Value::Array(meshes.clone()) {
                    errors.push(format!(
                        "{geo}: frozen meshes differ from frozen_meshes.json"
                    ));
                }
            }
        }
    }
    for e in files(&m) {
        if e.get("frozen").is_some() && !by_geo.contains_key(up(e)) {
            errors.push(format!(
                "{}: frozen meshes recorded, none in frozen_meshes.json",
                up(e)
            ));
        }
    }
    assert!(errors.is_empty(), "{}\n{REGENERATE}", errors.join("\n"));
    println!("{} .geo files with frozen meshes", by_geo.len());
}

/// Consumers, ported consumers and statuses re-derived from the inventory
/// appendix and the current parity matrix; no file of a ported test is
/// missing (`owed` without a note).
#[test]
fn every_file_of_a_ported_test_is_accounted_for() {
    let m = manifest();
    let root = repo_root();
    let appendix = appendix_consumers();
    let rows = parity_rows();
    let prefixes = strs(&m, "consumer_prefixes");
    assert!(
        !prefixes.is_empty(),
        "consumer_prefixes missing: {REGENERATE}"
    );
    let is_consumer_row = |c: &str| prefixes.iter().any(|p| c.split('-').next() == Some(p));
    let extra: BTreeMap<String, Vec<String>> = m["extra_consumers"]
        .as_object()
        .expect("extra_consumers")
        .iter()
        .map(|(k, v)| (k.clone(), str_vec(v)))
        .collect();
    let mut summary: BTreeMap<String, u64> = BTreeMap::new();
    let mut superseded: BTreeMap<String, Vec<String>> = BTreeMap::new();
    let mut errors = Vec::new();
    for e in files(&m) {
        let up = up(e);
        let mut direct: BTreeSet<String> = appendix.get(up).cloned().unwrap_or_default();
        direct.extend(extra.get(up).cloned().unwrap_or_default());
        if direct.is_empty() {
            errors.push(format!(
                "{up}: no consumer row (inventory appendix or extra_consumers)"
            ));
        }
        let mut consumers = direct.clone();
        for c in &direct {
            let sup = rows.get(c).is_some_and(|(st, _)| is_superseded(st));
            if is_consumer_row(c) && sup {
                let succ = successors(c, &rows, &BTreeSet::new());
                if succ.is_empty() {
                    errors.push(format!(
                        "{up}: consumer {c} is Superseded but names no successor (`split → …`)"
                    ));
                }
                superseded.insert(c.clone(), succ.iter().cloned().collect());
                consumers.extend(succ);
            }
        }
        let consumers: Vec<String> = consumers.into_iter().collect();
        if consumers != strs(e, "consumers") {
            errors.push(format!(
                "{up}: consumers {:?}, manifest {:?}",
                consumers,
                strs(e, "consumers")
            ));
        }
        let ported: Vec<String> = consumers
            .iter()
            .filter(|c| {
                is_consumer_row(c)
                    && rows
                        .get(c.as_str())
                        .is_some_and(|(st, _)| PORTED.contains(&st.as_str()))
            })
            .cloned()
            .collect();
        if ported != strs(e, "ported_consumers") {
            errors.push(format!(
                "{up}: ported consumers now {:?}, manifest {:?}",
                ported,
                strs(e, "ported_consumers")
            ));
        }
        let copies = strs(e, "copies");
        let frozen = e
            .get("frozen")
            .and_then(Value::as_array)
            .is_some_and(|f| !f.is_empty());
        let embedded = strs(e, "embedded_in");
        if !embedded.iter().all(|f| root.join(f).is_file()) {
            errors.push(format!("{up}: embedded_in names a missing file"));
        }
        let is_geo = up.ends_with(".geo");
        if is_geo && e.get("build_rule").and_then(Value::as_bool).is_none() {
            errors.push(format!("{up}: build_rule missing"));
        }
        let status = if !copies.is_empty() {
            "copied"
        } else if frozen {
            "frozen"
        } else if !embedded.is_empty() {
            "embedded"
        } else if is_geo && e.get("build_rule").and_then(Value::as_bool) == Some(false) {
            "source-only"
        } else if !ported.is_empty() {
            "owed"
        } else {
            "not-needed"
        };
        let recorded = e["status"].as_str().unwrap();
        if status != recorded {
            errors.push(format!("{up}: status now {status}, manifest {recorded}"));
        }
        if status == "owed" && e.get("note").is_none() {
            errors.push(format!(
                "{up}: consumed by ported rows {ported:?} but not in the repository \
                 (copy it verbatim, freeze it, or add a written note to the generator)"
            ));
        }
        *summary.entry(status.to_owned()).or_default() += 1;
    }
    let recorded_sup: BTreeMap<String, Vec<String>> = m["superseded_consumers"]
        .as_object()
        .expect("superseded_consumers")
        .iter()
        .map(|(k, v)| (k.clone(), str_vec(v)))
        .collect();
    if superseded != recorded_sup {
        errors.push(format!(
            "superseded consumers now {superseded:?}, manifest {recorded_sup:?}"
        ));
    }
    let recorded: BTreeMap<String, u64> = m["summary"]
        .as_object()
        .unwrap()
        .iter()
        .map(|(k, v)| (k.clone(), v.as_u64().unwrap()))
        .collect();
    if summary != recorded {
        errors.push(format!("summary now {summary:?}, manifest {recorded:?}"));
    }
    assert!(errors.is_empty(), "{}\n{REGENERATE}", errors.join("\n"));
    println!("accounting: {summary:?}; superseded consumers: {superseded:?}");
}

/// The golden files reproduced by Rust tests of the merged areas (coverage
/// list `docs/inventory/golden_files.md`) all have verbatim copies.
#[test]
fn reproduced_golden_files_have_copies() {
    let m = manifest();
    let by_path: BTreeMap<&str, &Value> = files(&m).iter().map(|e| (up(e), e)).collect();
    let mut golden: Vec<String> = [
        "test/test_io/test_parser/test_parser.verified",
        "test/test_mesh_utils/test_mesh_io/test_mesh_io_msh_physical_names.verified",
        "test/test_mesh_utils/test_buildfacets/test_buildfacets_gtest.verified",
        "test/test_model/test_common/test_model_solver/test_dof_manager_default.verified",
        "test/test_model/test_common/test_model_solver/test_model_solver_mumps.verified",
        "test/test_model/test_common/test_model_solver/test_model_solver_dynamic_explicit.verified",
        "test/test_model/test_common/test_model_solver/test_model_solver_dynamic_implicit.verified",
        "test/test_solver/test_sparse_matrix_product.verified",
        "test/test_model/test_solid_mechanics_model/test_cohesive/test_cohesive_extrinsic_implicit/K_matrix_verified.dat",
    ]
    .iter()
    .map(|s| (*s).to_owned())
    .collect();
    for ty in [
        "segment_2",
        "segment_3",
        "triangle_3",
        "triangle_6",
        "quadrangle_4",
        "quadrangle_8",
        "tetrahedron_4",
        "tetrahedron_10",
        "pentahedron_6",
        "pentahedron_15",
        "hexahedron_8",
        "hexahedron_20",
    ] {
        golden.push(format!("test/test_fe_engine/_{ty}.csv"));
    }
    for g in &golden {
        let e = by_path
            .get(g.as_str())
            .unwrap_or_else(|| panic!("{g} not in the manifest"));
        assert!(
            !strs(e, "copies").is_empty(),
            "{g}: no verbatim copy in the repository"
        );
    }
}

/// The directory whose `README.md` documents a copy: `crates/<c>/tests/data/
/// upstream` (or the inradius directory of `akantu-fe`, GOLD-002); `None`
/// for other files.
fn documented_base(rel: &str) -> Option<&str> {
    const UP: &str = "/tests/data/upstream/";
    const INRADIUS: &str = "crates/akantu-fe/tests/data/inradius/";
    if rel.starts_with("crates/")
        && let Some(i) = rel.find(UP)
        && rel[..i].matches('/').count() == 1
    {
        return Some(&rel[..i + UP.len() - 1]);
    }
    rel.starts_with(INRADIUS)
        .then_some(&INRADIUS[..INRADIUS.len() - 1])
}

/// Every file under a `crates/*/tests/data/upstream/` tree (and under
/// `crates/akantu-fe/tests/data/inradius/`, GOLD-002), found on disk among
/// the git-visible files, is a verbatim copy of an upstream data file and is
/// documented: the nearest `README.md` (in its directory or an ancestor up to
/// the data directory) has a row naming the file, the upstream path and the
/// SHA-256 computed here. Every file row of those READMEs names a file that
/// exists.
#[test]
fn upstream_data_directories_are_documented() {
    let m = manifest();
    let root = repo_root();
    let mut upstream_by_sha: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
    for e in files(&m) {
        upstream_by_sha
            .entry(e["sha256"].as_str().unwrap())
            .or_default()
            .push(up(e));
    }
    let mut documented: BTreeMap<String, BTreeSet<String>> = BTreeMap::new();
    let mut readmes: BTreeSet<String> = BTreeSet::new();
    let mut found = 0;
    let mut checked = 0;
    let mut errors = Vec::new();
    for rel in git_visible_files(&["crates".to_owned()]) {
        let Some(base) = documented_base(&rel) else {
            continue;
        };
        let name = rel.rsplit('/').next().unwrap();
        if name == "README.md" {
            readmes.insert(rel.clone());
            continue;
        }
        found += 1;
        let sha = sha256_file(&root.join(&rel));
        let Some(ups) = upstream_by_sha.get(sha.as_str()) else {
            errors.push(format!(
                "{rel}: not a verbatim copy of an upstream data file ({sha})"
            ));
            continue;
        };
        let mut dir = Path::new(&rel).parent().unwrap().to_path_buf();
        let readme = loop {
            let r = dir.join("README.md");
            if root.join(&r).is_file() {
                break Some(r.to_string_lossy().into_owned());
            }
            if dir.as_os_str() == base || !dir.pop() {
                break None;
            }
        };
        let Some(readme) = readme else {
            errors.push(format!("{rel}: no README.md up to {base}"));
            continue;
        };
        documented
            .entry(readme.clone())
            .or_default()
            .insert(name.to_owned());
        let text = std::fs::read_to_string(root.join(&readme)).unwrap();
        let row = text.lines().any(|l| {
            l.contains(&format!("`{name}`"))
                && l.contains(&sha)
                && ups.iter().any(|u| l.contains(&format!("`{u}`")))
        });
        if row {
            checked += 1;
        } else {
            errors.push(format!(
                "{rel}: {readme} has no row with `{name}`, its upstream path (one of {ups:?}) \
                 and its SHA-256 `{sha}`"
            ));
        }
    }
    for readme in &readmes {
        let text = std::fs::read_to_string(root.join(readme)).unwrap();
        let names = documented.get(readme).cloned().unwrap_or_default();
        for l in text.lines() {
            let Some(cell) = l.strip_prefix("| `") else {
                continue;
            };
            let Some(end) = cell.find('`') else {
                continue;
            };
            let name = &cell[..end];
            if !names.contains(name) {
                errors.push(format!(
                    "{readme}: row for `{name}`, which is not a file documented by this README"
                ));
            }
        }
    }
    assert!(errors.is_empty(), "{}", errors.join("\n"));
    assert!(found > 0, "no file under crates/*/tests/data/upstream");
    assert_eq!(checked, found);
    println!(
        "{checked} files in tests/data directories documented ({} READMEs)",
        readmes.len()
    );
}

#[test]
fn id_parsing_matches_the_generator() {
    // `\b[A-Z]+-\d{3}\b` and `A…B` ranges, as in the Python generator
    let ids: Vec<String> = ids_in("TEST-001, xTEST-002 TEST-0034 (EX-018…EX-020) GOLD-013.")
        .into_iter()
        .map(|(_, _, s)| s)
        .collect();
    assert_eq!(ids, ["TEST-001", "EX-018", "EX-020", "GOLD-013"]);
    let r: Vec<String> = ids_with_ranges("split → TEST-235…TEST-238")
        .into_iter()
        .collect();
    assert_eq!(r, ["TEST-235", "TEST-236", "TEST-237", "TEST-238"]);
    assert_eq!(
        table_cells("| A | b\\|c | d |"),
        ["A", "b\\|c", "d"].map(str::to_owned)
    );
}
