// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Test helpers of the
// model-coupler tests: frozen upstream meshes, fixtures, bitwise
// comparisons.

#![allow(dead_code)] // each test binary uses a subset

use std::path::{Path, PathBuf};

use akantu_core::Real;
use akantu_core::enums::MeshIoType;
use akantu_mesh::Mesh;
use akantu_validation::{Fixture, fixtures_root};
use sha2::{Digest, Sha256};

/// Frozen upstream mesh `<rel>` (path relative to the reference tree).
pub fn frozen_mesh(rel: &str) -> PathBuf {
    fixtures_root().join("meshes/gmsh-4.13.1").join(rel)
}

/// Upstream test inputs copied into this crate (`tests/data`).
pub fn data_dir() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/data")
}

/// Read a Gmsh mesh.
pub fn read_mesh(path: &Path, dim: usize) -> Mesh {
    let mut mesh = Mesh::new(dim, "mesh").unwrap();
    akantu_io::read_mesh(&mut mesh, path, MeshIoType::Gmsh)
        .unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    mesh
}

/// Cases present under `fixtures/<area>` whose name starts with `prefix`
/// (golden, checksums verified).
pub fn cases(area: &str, prefix: &str) -> Vec<Fixture> {
    let dir = fixtures_root().join(area);
    let mut names: Vec<String> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(|e| e.ok())
                .filter(|e| e.path().join("manifest.json").exists())
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .filter(|n| n.starts_with(prefix))
                .collect()
        })
        .unwrap_or_default();
    names.sort();
    names
        .into_iter()
        .map(|n| {
            let f = Fixture::open(&dir.join(&n)).unwrap();
            f.require_golden().unwrap();
            f.verify_checksums().unwrap();
            f
        })
        .collect()
}

/// `f64` quantity of a fixture.
pub fn f64s(f: &Fixture, name: &str) -> Vec<f64> {
    f.load_f64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

/// `i64` quantity of a fixture.
pub fn i64s(f: &Fixture, name: &str) -> Vec<i64> {
    f.load_i64(name)
        .unwrap_or_else(|e| panic!("{}: {name}: {e}", f.case_id()))
        .data
}

/// Text of an input file of a fixture.
pub fn read_input_text(f: &Fixture, role: &str) -> String {
    std::fs::read_to_string(f.input_path(role).unwrap()).unwrap()
}

/// D-046: equal bits, or two default quiet NaNs differing in sign only.
pub fn same_bits(a: f64, b: f64) -> bool {
    let canon = |x: f64| {
        let bits = x.to_bits();
        if bits & 0x7fff_ffff_ffff_ffff == 0x7ff8_0000_0000_0000 {
            0x7ff8_0000_0000_0000
        } else {
            bits
        }
    };
    canon(a) == canon(b)
}

/// Bitwise comparison (D-046 default-NaN rule).
pub fn assert_bits(case: &str, what: &str, got: &[f64], expected: &[f64]) {
    assert_eq!(got.len(), expected.len(), "{case}: {what}: length");
    let bad: Vec<usize> = (0..got.len())
        .filter(|&i| !same_bits(got[i], expected[i]))
        .collect();
    if let Some(&i) = bad.first() {
        panic!(
            "{case}: {what}: {} of {} entries differ; first at {i}: {:e} vs {:e} (Δ = {:e})",
            bad.len(),
            got.len(),
            got[i],
            expected[i],
            got[i] - expected[i]
        );
    }
}

/// Number of entries that are bitwise equal.
pub fn count_bits(got: &[f64], expected: &[f64]) -> usize {
    got.iter()
        .zip(expected)
        .filter(|(a, b)| same_bits(**a, **b))
        .count()
}

/// SHA-256 of the little-endian bytes of `a` (the probes' digests).
pub fn sha_hex(a: &[f64]) -> String {
    let mut h = Sha256::new();
    for v in a {
        h.update(v.to_le_bytes());
    }
    h.finalize().iter().map(|b| format!("{b:02x}")).collect()
}

/// Relative error `|a − b| / |b|`.
pub fn rel(a: Real, b: Real) -> Real {
    (a - b).abs() / b.abs()
}
pub mod trace;
