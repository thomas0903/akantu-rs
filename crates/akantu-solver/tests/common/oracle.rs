// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Fixture access for
// the differential tests against fixtures/solver_model/* (generated
// by oracle/probes/solver_model/solver_model_probe.cc). Test support only.

use akantu_validation::reduce::NanMinMax;
use std::path::PathBuf;

use akantu_core::enums::MatrixType;
use akantu_linalg::SparseMatrixAij;
use akantu_validation::{Fixture, fixtures_root};
use sha2::{Digest, Sha256};

/// Every case directory below `fixtures/solver_model` (checksums verified).
pub fn cases() -> Vec<Fixture> {
    let dir = fixtures_root().join("solver_model");
    let mut out: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("{}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.join("manifest.json").exists())
        .collect();
    out.sort();
    assert!(!out.is_empty(), "no case in {}", dir.display());
    out.iter()
        .map(|p| {
            let f = Fixture::open(p).unwrap();
            f.require_golden().unwrap();
            f.verify_checksums().unwrap();
            f
        })
        .collect()
}

pub fn case(name: &str) -> Fixture {
    cases()
        .into_iter()
        .find(|f| f.case_id() == format!("solver_model/{name}"))
        .unwrap_or_else(|| panic!("no fixture solver_model/{name}"))
}

pub fn f64s(f: &Fixture, name: &str) -> Vec<f64> {
    f.load_f64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

pub fn i64s(f: &Fixture, name: &str) -> Vec<i64> {
    f.load_i64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

pub fn has(f: &Fixture, name: &str) -> bool {
    f.quantity(name).is_ok()
}

pub fn param<'a>(f: &'a Fixture, key: &str) -> &'a serde_json::Value {
    &f.manifest().parameters[key]
}

pub fn param_str(f: &Fixture, key: &str) -> String {
    param(f, key)
        .as_str()
        .unwrap_or_else(|| panic!("{}: parameter {key}", f.case_id()))
        .to_owned()
}

pub fn input_text(f: &Fixture, role: &str) -> String {
    String::from_utf8(std::fs::read(f.input_path(role).unwrap()).unwrap()).unwrap()
}

pub fn sha256_hex(bytes: &[u8]) -> String {
    let d = Sha256::digest(bytes);
    d.iter().map(|b| format!("{b:02x}")).collect()
}

/// SHA-256 of the concatenation of binary64 arrays (little endian).
pub fn digest_arrays(arrays: &[&[f64]]) -> String {
    let mut h = Sha256::new();
    for a in arrays {
        for x in *a {
            // D-046: default NaNs canonicalised (no recorded digest changes)
            h.update(akantu_validation::canonical_le_bytes(*x));
        }
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Exact comparison with the oracle record (decision D-046: default NaNs
/// of either sign are identical, everything else bitwise).
pub fn same_bits(a: &[f64], b: &[f64]) -> bool {
    akantu_validation::exact::exact_slices_eq(b, a)
}

/// Number of entries whose bits differ and the largest relative difference
/// (relative to the largest magnitude of `b`).
pub fn diff_stats(a: &[f64], b: &[f64]) -> (usize, f64) {
    let scale = b.iter().fold(0.0f64, |m, v| m.nan_max(v.abs()));
    let mut n = a.len().abs_diff(b.len());
    let mut worst = 0.0f64;
    for (x, y) in a.iter().zip(b) {
        if !akantu_validation::exact_eq(*y, *x) {
            n += 1;
            worst = worst.nan_max((x - y).abs() / scale.max(f64::MIN_POSITIVE));
        }
    }
    (n, worst)
}

/// Assert bit identity of `got` with the recorded quantity `name`.
pub fn assert_bits(f: &Fixture, name: &str, got: &[f64]) -> usize {
    let want = f64s(f, name);
    let (n, worst) = diff_stats(got, &want);
    assert!(
        same_bits(got, &want),
        "{}: {name}: {n} of {} values differ (max rel {worst:e})",
        f.case_id(),
        want.len()
    );
    want.len()
}

fn coords_digest(m: &SparseMatrixAij) -> String {
    let mut bytes = Vec::with_capacity(16 * m.nnz());
    for r in m.irn_one_based() {
        bytes.extend_from_slice(&r.to_le_bytes());
    }
    for c in m.jcn_one_based() {
        bytes.extend_from_slice(&c.to_le_bytes());
    }
    sha256_hex(&bytes)
}

/// Assert that `m` equals the recorded matrix `prefix` (COO or digests,
/// releases, order, symmetry). Returns the number of compared values.
pub fn assert_matrix(f: &Fixture, prefix: &str, m: &SparseMatrixAij) -> usize {
    let cid = f.case_id();
    let p = &f.manifest().parameters;
    assert_eq!(
        m.nnz() as u64,
        p[format!("{prefix}/nnz")].as_u64().unwrap(),
        "{cid}: {prefix} nnz"
    );
    assert_eq!(
        coords_digest(m),
        p[format!("{prefix}/coords_sha256")].as_str().unwrap(),
        "{cid}: {prefix} coordinates"
    );
    assert_eq!(
        digest_arrays(&[m.values()]),
        p[format!("{prefix}/values_sha256")].as_str().unwrap(),
        "{cid}: {prefix} values ({:?})",
        has(f, &format!("{prefix}/values"))
            .then(|| diff_stats(m.values(), &f64s(f, &format!("{prefix}/values"))))
    );
    let rel = |k: &str| p[format!("{prefix}/{k}")].as_i64().unwrap();
    assert_eq!(
        m.profile_release() as i64,
        rel("profile_release"),
        "{cid}: {prefix} profile release"
    );
    assert_eq!(
        m.value_release() as i64,
        rel("value_release"),
        "{cid}: {prefix} value release"
    );
    assert_eq!(m.size(), rel("size"), "{cid}: {prefix} size");
    assert_eq!(
        m.matrix_type() == MatrixType::Symmetric,
        p[format!("{prefix}/symmetric")].as_bool().unwrap(),
        "{cid}: {prefix} symmetry"
    );
    m.nnz()
}

/// Upstream `.verified` comparison (`diff -biw`): lines compared after
/// removing all whitespace.
pub fn same_text_ignoring_whitespace(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase()
            })
            .collect()
    };
    norm(a) == norm(b)
}

/// `std::setw(w)` right alignment.
pub fn setw(s: &str, w: usize) -> String {
    format!("{s:>w$}")
}
