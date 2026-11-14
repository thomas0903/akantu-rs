// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it is the Rust counterpart of the C++ probe writer
// (oracle/probes/include/akantu_probe/fixture_writer.hh).

//! Writing fixture cases from Rust (synthetic fixtures in tests,
//! regression cases persisted from failing tests, Rust snapshots).

use std::path::{Path, PathBuf};

use crate::checksum::{sha256_bytes, sha256_file};
use crate::error::{Error, Result};
use crate::fixture::MANIFEST_FILE;
use crate::manifest::{
    BuildInfo, Expectation, FORMAT_VERSION, FixtureStatus, Generator, InputFile, Layout, Manifest,
    Parallel, Quantity, ReferenceInfo, Storage, valid_case_id, valid_relative_path,
};
use crate::npy::{NpyArray, NpyData, encode_npy};

/// Incrementally builds a fixture case directory.
#[derive(Debug)]
pub struct FixtureWriter {
    dir: PathBuf,
    manifest: Manifest,
    bytes: u64,
}

impl FixtureWriter {
    /// Start a case `case_id` under `root`. Fails if the directory already
    /// holds a manifest (fixtures are never overwritten implicitly).
    pub fn create(
        root: &Path,
        case_id: &str,
        description: &str,
        status: FixtureStatus,
        generator: Generator,
        reference: ReferenceInfo,
        build: Option<BuildInfo>,
    ) -> Result<Self> {
        if !valid_case_id(case_id) {
            return Err(Error::manifest(
                root,
                format!("invalid case id `{case_id}`"),
            ));
        }
        let dir = case_id
            .split('/')
            .fold(root.to_path_buf(), |p, s| p.join(s));
        if dir.join(MANIFEST_FILE).exists() {
            return Err(Error::manifest(
                dir.join(MANIFEST_FILE),
                "refusing to overwrite an existing fixture",
            ));
        }
        std::fs::create_dir_all(&dir).map_err(|e| Error::io(&dir, e))?;
        Ok(Self {
            dir,
            manifest: Manifest {
                format_version: FORMAT_VERSION,
                case_id: case_id.to_string(),
                description: description.to_string(),
                status,
                generator,
                reference,
                build,
                created: None,
                inputs: Vec::new(),
                parameters: serde_json::Value::Null,
                parallel: Parallel::default(),
                storage: Storage::default(),
                steps: None,
                quantities: Vec::new(),
                expect: Expectation::default(),
                tags: Vec::new(),
                notes: None,
            },
            bytes: 0,
        })
    }

    /// Case directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Mutable access to the manifest (parameters, steps, parallel, tags...).
    pub fn manifest_mut(&mut self) -> &mut Manifest {
        &mut self.manifest
    }

    /// Copy an input file into `inputs/` and record it.
    pub fn add_input_file(&mut self, role: &str, src: &Path, source: Option<String>) -> Result<()> {
        let name = src
            .file_name()
            .and_then(|n| n.to_str())
            .ok_or_else(|| Error::manifest(src, "input file has no UTF-8 file name"))?;
        let rel = format!("inputs/{name}");
        if !valid_relative_path(&rel) {
            return Err(Error::manifest(
                src,
                format!("input file name `{name}` is not portable"),
            ));
        }
        let dst = self.dir.join(&rel);
        std::fs::create_dir_all(self.dir.join("inputs")).map_err(|e| Error::io(&self.dir, e))?;
        std::fs::copy(src, &dst).map_err(|e| Error::io(src, e))?;
        self.manifest.inputs.push(InputFile {
            role: role.to_string(),
            path: Some(rel),
            source,
            sha256: sha256_file(&dst)?,
        });
        Ok(())
    }

    /// Record an input by reference only (not copied).
    pub fn add_input_reference(&mut self, role: &str, source: &str, sha256: &str) {
        self.manifest.inputs.push(InputFile {
            role: role.to_string(),
            path: None,
            source: Some(source.to_string()),
            sha256: sha256.to_string(),
        });
    }

    /// Write an array as `data/<sanitised name>.npy` and record it. Returns
    /// the quantity entry for further annotation.
    pub fn add_array(
        &mut self,
        name: &str,
        array: &NpyArray,
        axes: &[&str],
        layout: Layout,
    ) -> Result<&mut Quantity> {
        let file = format!("data/{}.npy", sanitize(name));
        let bytes = encode_npy(array)?;
        let path = self.dir.join(&file);
        std::fs::create_dir_all(self.dir.join("data")).map_err(|e| Error::io(&self.dir, e))?;
        std::fs::write(&path, &bytes).map_err(|e| Error::io(&path, e))?;
        self.bytes += u64::try_from(bytes.len()).unwrap_or(u64::MAX);
        self.manifest.quantities.push(Quantity {
            name: name.to_string(),
            file,
            dtype: array.dtype().name().to_string(),
            shape: array.shape.clone(),
            axes: axes.iter().map(|a| (*a).to_string()).collect(),
            layout,
            sha256: sha256_bytes(&bytes),
            description: String::new(),
            element_type: None,
            ghost_type: None,
            components: None,
            ordering: Vec::new(),
            ragged: None,
            units: None,
            tolerance_class: None,
        });
        let last = self.manifest.quantities.len() - 1;
        Ok(&mut self.manifest.quantities[last])
    }

    /// Convenience wrapper for `f64` data.
    pub fn add_f64(
        &mut self,
        name: &str,
        shape: &[usize],
        data: &[f64],
        axes: &[&str],
        layout: Layout,
    ) -> Result<&mut Quantity> {
        let a = NpyArray::new(shape.to_vec(), NpyData::F64(data.to_vec()))?;
        self.add_array(name, &a, axes, layout)
    }

    /// Convenience wrapper for `i64` data.
    pub fn add_i64(
        &mut self,
        name: &str,
        shape: &[usize],
        data: &[i64],
        axes: &[&str],
        layout: Layout,
    ) -> Result<&mut Quantity> {
        let a = NpyArray::new(shape.to_vec(), NpyData::I64(data.to_vec()))?;
        self.add_array(name, &a, axes, layout)
    }

    /// Validate and write `manifest.json`. Returns the case directory.
    pub fn finish(mut self) -> Result<PathBuf> {
        self.manifest.storage.bytes = Some(self.bytes);
        let path = self.dir.join(MANIFEST_FILE);
        self.manifest.validate(&path)?;
        std::fs::write(&path, self.manifest.to_json_string()?).map_err(|e| Error::io(&path, e))?;
        Ok(self.dir)
    }
}

/// Map a quantity name to a portable file stem (`/` becomes `__`).
#[must_use]
pub fn sanitize(name: &str) -> String {
    let s: String = name
        .replace('/', "__")
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '+') {
                c
            } else {
                '_'
            }
        })
        .collect();
    if s.is_empty() || s == "." || s == ".." {
        "_".into()
    } else {
        s
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;

    #[test]
    fn write_then_read_back() {
        let dir = tempfile::tempdir().unwrap();
        let mut w = FixtureWriter::create(
            dir.path(),
            "test/synthetic/basic",
            "synthetic",
            FixtureStatus::RustSnapshot,
            Generator {
                probe: "unit-test".into(),
                probe_version: "1".into(),
                command: String::new(),
            },
            ReferenceInfo::default(),
            None,
        )
        .unwrap();
        w.add_f64(
            "u/x",
            &[2, 2],
            &[1.0, 2.0, 3.0, 4.0],
            &["node", "component"],
            Layout::Nodal,
        )
        .unwrap()
        .components = Some(vec!["x".into(), "y".into()]);
        w.add_i64("ids", &[2], &[7, 9], &["node"], Layout::Nodal)
            .unwrap();
        let input = dir.path().join("mesh.msh");
        std::fs::write(&input, "$MeshFormat\n").unwrap();
        w.add_input_file("mesh", &input, Some("generated:test".into()))
            .unwrap();
        let case_dir = w.finish().unwrap();

        let fx = Fixture::open(&case_dir).unwrap();
        fx.verify_checksums().unwrap();
        assert_eq!(fx.case_id(), "test/synthetic/basic");
        assert_eq!(fx.root(), dir.path());
        let u = fx.load_f64("u/x").unwrap();
        assert_eq!(u.axes, vec!["node", "component"]);
        assert_eq!(u.data, vec![1.0, 2.0, 3.0, 4.0]);
        assert_eq!(fx.load_i64("ids").unwrap().data, vec![7, 9]);
        assert!(
            fx.load_i64("u/x").is_err(),
            "f64 data must not load as integers"
        );
        assert!(fx.input_path("mesh").unwrap().is_file());
        assert!(fx.require_golden().is_err());
        assert_eq!(fx.quantity_names("u/*"), vec!["u/x"]);
        assert!(matches!(
            fx.quantity("nope").unwrap_err(),
            Error::UnknownQuantity { .. }
        ));

        // Second creation of the same case is refused.
        assert!(
            FixtureWriter::create(
                dir.path(),
                "test/synthetic/basic",
                "again",
                FixtureStatus::RustSnapshot,
                Generator {
                    probe: "unit-test".into(),
                    probe_version: "1".into(),
                    command: String::new()
                },
                ReferenceInfo::default(),
                None
            )
            .is_err()
        );

        // Tampering is detected.
        let file = case_dir.join("data/u__x.npy");
        let mut bytes = std::fs::read(&file).unwrap();
        let last = bytes.len() - 1;
        bytes[last] ^= 1;
        std::fs::write(&file, bytes).unwrap();
        assert!(matches!(
            fx.load_f64("u/x").unwrap_err(),
            Error::Checksum { .. }
        ));
        // Missing data is reported as not materialised.
        std::fs::remove_file(&file).unwrap();
        assert!(matches!(
            fx.load_f64("u/x").unwrap_err(),
            Error::FixtureNotMaterialized { .. }
        ));
    }

    #[test]
    fn sanitize_names() {
        assert_eq!(
            sanitize("stress/_triangle_3/not_ghost"),
            "stress___triangle_3__not_ghost"
        );
        assert_eq!(sanitize("a b"), "a_b");
        assert_eq!(sanitize(".."), "_");
    }
}
