// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the golden-fixture loader of the
// validation harness (docs/NUMERICAL_VALIDATION.md).

//! Fixture discovery and loading.
//!
//! **Platform variants (decision D-073 (1)).** A few oracle
//! results depend on the numerical environment of the oracle (today: MUMPS
//! solves, whose OpenBLAS kernels differ between x86_64 `Haswell` and
//! aarch64). Such a case keeps its authoritative golden values at the top
//! level (`manifest.json`, `data/`, `inputs/`; x86_64 per D-043) and the
//! values of every other environment in a sibling tree
//! `variants/<environment>/` holding `variant.json` (the manifest of that
//! environment's run, moved unchanged: its provenance and checksums are the
//! original ones) and its own `data/`; the inputs are shared with the top
//! level and must be identical. The environment of a manifest is
//! [`environment_id`] (`<arch>-<OpenBLAS core type>`, e.g. `x86_64-haswell`,
//! `aarch64-auto`). Every opening function except the `*_top` ones selects
//! the values of the running environment ([`running_environment`]); a case
//! with variants fails to open when the running environment is unknown or
//! has no recorded values ([`Error::FixtureEnvironment`]). Cases without
//! variants are the same golden on every platform. Tree walkers that look
//! for `manifest.json` never see a variant (its manifest is `variant.json`),
//! so `fixture-diff`, `finalize_fixtures.py` and the checksum index keep
//! working on the top level (the variant files are indexed in
//! `fixtures/SHA256SUMS` like every other file).

use std::path::{Path, PathBuf};

use crate::array::Field;
use crate::checksum::verify_file;
use crate::error::{Error, Result};
use crate::manifest::{BuildInfo, FixtureStatus, Layout, Manifest, Quantity, StorageMode};
use crate::npy::{NpyArray, read_npy};
use crate::tolerance::{self, ResolvedTolerance};

/// Environment variable overriding the fixtures root directory.
pub const FIXTURES_DIR_ENV: &str = "AKANTU_FIXTURES_DIR";

/// Name of the manifest file in a case directory.
pub const MANIFEST_FILE: &str = "manifest.json";

/// Sub-directory of a case holding its platform variants (D-073 (1)).
pub const VARIANTS_DIR: &str = "variants";

/// Manifest of a platform variant, in `variants/<environment>/`.
pub const VARIANT_MANIFEST_FILE: &str = "variant.json";

/// Environment variable naming the numerical environment whose golden
/// values a case with platform variants provides (an [`environment_id`]),
/// instead of the rule of [`running_environment`]. For diagnostics, e.g.
/// comparing against the aarch64 values on an x86_64 host.
pub const FIXTURE_ENV_ENV: &str = "AKANTU_FIXTURE_ENV";

/// Numerical environment of the oracle run recorded in `build`:
/// `<arch>-<OpenBLAS core type>` in lower case, the core type being
/// `build.libraries.openblas_coretype` (recorded on x86_64 since D-050) or
/// `auto` when OpenBLAS chose its kernels from the host CPU (unset, as on
/// the aarch64 oracle). Examples: `x86_64-haswell`, `aarch64-auto`.
#[must_use]
pub fn environment_id(build: &BuildInfo) -> String {
    let core = build
        .libraries
        .get("openblas_coretype")
        .map(|c| c.trim())
        .filter(|c| !c.is_empty() && !c.starts_with("auto"))
        .map_or_else(|| "auto".to_string(), str::to_ascii_lowercase);
    format!("{}-{core}", build.arch)
}

/// [`environment_id`] of a manifest's build; `unknown` without one (Rust
/// snapshots).
#[must_use]
pub fn manifest_environment(manifest: &Manifest) -> String {
    manifest
        .build
        .as_ref()
        .map_or_else(|| "unknown".to_string(), environment_id)
}

/// The numerical environment this process compares against: the value of
/// [`FIXTURE_ENV_ENV`] when set and not empty, otherwise
/// [`running_environment_for`]`(std::env::consts::ARCH, $OPENBLAS_CORETYPE)`.
#[must_use]
pub fn running_environment() -> Option<String> {
    if let Some(v) = std::env::var_os(FIXTURE_ENV_ENV) {
        let v = v.to_string_lossy().trim().to_string();
        if !v.is_empty() {
            return Some(v);
        }
    }
    let core = std::env::var("OPENBLAS_CORETYPE").ok();
    running_environment_for(std::env::consts::ARCH, core.as_deref())
}

/// The authoritative oracle environment of a platform (D-043, D-050):
/// x86_64 → `x86_64-haswell` (the pinned oracle; `OPENBLAS_CORETYPE` unset
/// or `Haswell`), aarch64 → `aarch64-auto` (the committed aarch64 corpus,
/// OpenBLAS's run-time choice; `OPENBLAS_CORETYPE` unset). Any other
/// combination — another architecture, or an explicit other OpenBLAS core
/// type, under which BLAS-backed results of the port belong to no recorded
/// oracle environment — is unknown (`None`).
#[must_use]
pub fn running_environment_for(arch: &str, openblas_coretype: Option<&str>) -> Option<String> {
    let core = openblas_coretype.map(str::trim).filter(|c| !c.is_empty());
    match (arch, core) {
        ("x86_64", None) => Some("x86_64-haswell".to_string()),
        ("x86_64", Some(c)) if c.eq_ignore_ascii_case("haswell") => {
            Some("x86_64-haswell".to_string())
        }
        ("aarch64", None) => Some("aarch64-auto".to_string()),
        _ => None,
    }
}

/// Whether this process compares values with an oracle record made in the
/// environment `recorded` (an [`environment_id`], e.g. `x86_64-haswell`),
/// D-121: `None` = compare; `Some(reason)` = do not compare the oracle
/// values, run the scenario's other checks and report the reason.
///
/// The values are compared on x86_64, the authoritative platform (D-043),
/// whatever the running environment (a missing or different BLAS pin there
/// is reported by the comparison itself), and on any other platform whose
/// [`running_environment`] equals `recorded`. Elsewhere the record's values
/// depend on OpenBLAS kernels this process does not run.
#[must_use]
pub fn oracle_values_not_comparable(recorded: &str) -> Option<String> {
    oracle_values_not_comparable_for(
        std::env::consts::ARCH,
        running_environment().as_deref(),
        recorded,
    )
}

/// [`oracle_values_not_comparable`] for a given architecture and running
/// environment.
#[must_use]
pub fn oracle_values_not_comparable_for(
    arch: &str,
    running: Option<&str>,
    recorded: &str,
) -> Option<String> {
    if arch == "x86_64" || running == Some(recorded) {
        return None;
    }
    Some(format!(
        "the oracle record was made in {recorded}; this process runs in {} (D-121)",
        running.map_or_else(
            || format!("{arch}, no recorded environment"),
            str::to_string
        )
    ))
}

/// The one line a scenario that compares with an oracle record prints per
/// launch (D-121); the MUMPS and PETSc tier scripts count these lines.
#[must_use]
pub fn oracle_report_line(scenario: &str, recorded: &str, not_comparable: Option<&str>) -> String {
    match not_comparable {
        None => format!("ORACLE {scenario}: compared with the record of {recorded}"),
        Some(why) => format!("ORACLE {scenario}: not compared: {why}"),
    }
}

/// Root of the fixtures tree: `$AKANTU_FIXTURES_DIR` if set, otherwise
/// `<workspace>/fixtures` (located from this crate's manifest directory at
/// compile time).
#[must_use]
pub fn fixtures_root() -> PathBuf {
    if let Some(dir) = std::env::var_os(FIXTURES_DIR_ENV) {
        return PathBuf::from(dir);
    }
    workspace_root().join("fixtures")
}

/// Workspace root (two levels above this crate).
#[must_use]
pub fn workspace_root() -> PathBuf {
    let crate_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    crate_dir
        .ancestors()
        .nth(2)
        .map_or_else(|| crate_dir.to_path_buf(), Path::to_path_buf)
}

/// A loaded fixture case.
#[derive(Debug, Clone)]
pub struct Fixture {
    dir: PathBuf,
    root: PathBuf,
    manifest: Manifest,
    /// Directory the quantity files are relative to: `dir`, or
    /// `dir/variants/<environment>` for a platform variant.
    data_dir: PathBuf,
    /// Environment of the selected platform variant (`None`: top level).
    variant: Option<String>,
}

impl Fixture {
    /// Open the case `case_id` under [`fixtures_root`], with the values of
    /// the running environment (module docs).
    pub fn open_by_id(case_id: &str) -> Result<Self> {
        Self::open_case(&fixtures_root(), case_id)
    }

    /// Open the case `case_id` under `root`, with the values of the running
    /// environment (module docs).
    pub fn open_case(root: &Path, case_id: &str) -> Result<Self> {
        Self::open_case_in(root, case_id, running_environment().as_deref())
    }

    /// Open the case `case_id` under `root` with the values of the
    /// numerical environment `environment` (`None`: unknown, which only a
    /// case without platform variants accepts).
    pub fn open_case_in(root: &Path, case_id: &str, environment: Option<&str>) -> Result<Self> {
        Self::open_case_top(root, case_id)?.select(environment)
    }

    /// Open the top level of the case `case_id` under `root` (its
    /// authoritative golden values), ignoring platform variants.
    pub fn open_case_top(root: &Path, case_id: &str) -> Result<Self> {
        let dir = case_id
            .split('/')
            .fold(root.to_path_buf(), |p, s| p.join(s));
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest = Manifest::read(&manifest_path)?;
        if manifest.case_id != case_id {
            return Err(Error::manifest(
                &manifest_path,
                format!(
                    "case_id `{}` does not match its directory `{case_id}`",
                    manifest.case_id
                ),
            ));
        }
        Ok(Self {
            data_dir: dir.clone(),
            dir,
            root: root.to_path_buf(),
            manifest,
            variant: None,
        })
    }

    /// Open a case directory directly, with the values of the running
    /// environment (module docs); the fixtures root is derived from the
    /// directory and the manifest's `case_id`.
    pub fn open(dir: &Path) -> Result<Self> {
        Self::open_top(dir)?.select(running_environment().as_deref())
    }

    /// Open the top level of a case directory, ignoring platform variants.
    pub fn open_top(dir: &Path) -> Result<Self> {
        let manifest_path = dir.join(MANIFEST_FILE);
        let manifest = Manifest::read(&manifest_path)?;
        let n = manifest.case_id.split('/').count();
        let suffix: PathBuf = manifest.case_id.split('/').collect();
        if !dir.ends_with(&suffix) {
            return Err(Error::manifest(
                &manifest_path,
                format!(
                    "case_id `{}` does not match directory {}",
                    manifest.case_id,
                    dir.display()
                ),
            ));
        }
        let root = dir
            .ancestors()
            .nth(n)
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
        Ok(Self {
            dir: dir.to_path_buf(),
            root,
            manifest,
            data_dir: dir.to_path_buf(),
            variant: None,
        })
    }

    /// Environments of the platform variants of this case
    /// (`variants/<environment>/variant.json`), sorted; empty for a case
    /// without variants.
    pub fn variant_environments(&self) -> Result<Vec<String>> {
        let vdir = self.dir.join(VARIANTS_DIR);
        if !vdir.is_dir() {
            return Ok(Vec::new());
        }
        let mut out = Vec::new();
        for entry in std::fs::read_dir(&vdir).map_err(|e| Error::io(&vdir, e))? {
            let entry = entry.map_err(|e| Error::io(&vdir, e))?;
            let path = entry.path();
            if !path.join(VARIANT_MANIFEST_FILE).is_file() {
                return Err(Error::manifest(
                    &path,
                    format!("platform variant directory without {VARIANT_MANIFEST_FILE}"),
                ));
            }
            let name = entry.file_name().into_string().map_err(|_| {
                Error::manifest(&path, "platform variant directory name is not UTF-8")
            })?;
            out.push(name);
        }
        out.sort();
        Ok(out)
    }

    /// Every environment this case has golden values for: the top level's
    /// first, then the variants'.
    pub fn environments(&self) -> Result<Vec<String>> {
        let top = self.top_environment();
        let mut all = vec![top.clone()];
        for v in self.variant_environments()? {
            if v == top {
                return Err(Error::manifest(
                    self.dir.join(VARIANTS_DIR).join(&v),
                    format!("a platform variant repeats the top level's environment `{top}`"),
                ));
            }
            all.push(v);
        }
        Ok(all)
    }

    /// Environment of the top level of this case.
    fn top_environment(&self) -> String {
        if self.variant.is_some() {
            Manifest::read(&self.dir.join(MANIFEST_FILE))
                .map_or_else(|_| String::new(), |m| manifest_environment(&m))
        } else {
            manifest_environment(&self.manifest)
        }
    }

    /// The values of `environment` (module docs): the top level, or the
    /// platform variant of that environment.
    fn select(self, environment: Option<&str>) -> Result<Self> {
        let variants = self.variant_environments()?;
        if variants.is_empty() {
            return Ok(self);
        }
        let available = self.environments()?;
        let top = available[0].clone();
        let unavailable = |env: Option<&str>| Error::FixtureEnvironment {
            case_id: self.manifest.case_id.clone(),
            environment: env.map(str::to_string),
            available: available.clone(),
        };
        let Some(env) = environment else {
            return Err(unavailable(None));
        };
        if env == top {
            return Ok(self);
        }
        if !variants.iter().any(|v| v == env) {
            return Err(unavailable(Some(env)));
        }
        let vdir = self.dir.join(VARIANTS_DIR).join(env);
        let mpath = vdir.join(VARIANT_MANIFEST_FILE);
        let m = Manifest::read(&mpath)?;
        if m.case_id != self.manifest.case_id {
            return Err(Error::manifest(
                &mpath,
                format!(
                    "case_id `{}` differs from its case `{}`",
                    m.case_id, self.manifest.case_id
                ),
            ));
        }
        let got = manifest_environment(&m);
        if got != env {
            return Err(Error::manifest(
                &mpath,
                format!("variant directory `{env}` holds values of the environment `{got}`"),
            ));
        }
        if m.inputs != self.manifest.inputs {
            return Err(Error::manifest(
                &mpath,
                "the inputs of a platform variant must be those of the top level (shared inputs/)",
            ));
        }
        Ok(Self {
            dir: self.dir,
            root: self.root,
            manifest: m,
            data_dir: vdir,
            variant: Some(env.to_string()),
        })
    }

    /// Environment of the selected platform variant, `None` for the top
    /// level.
    #[must_use]
    pub fn variant(&self) -> Option<&str> {
        self.variant.as_deref()
    }

    /// Numerical environment of the loaded values ([`environment_id`] of
    /// the loaded manifest).
    #[must_use]
    pub fn environment(&self) -> String {
        manifest_environment(&self.manifest)
    }

    /// Directory the quantity files are relative to (the case directory, or
    /// `variants/<environment>` of a platform variant).
    #[must_use]
    pub fn data_dir(&self) -> &Path {
        &self.data_dir
    }

    /// Path of the loaded manifest.
    fn manifest_path(&self) -> PathBuf {
        if self.variant.is_some() {
            self.data_dir.join(VARIANT_MANIFEST_FILE)
        } else {
            self.dir.join(MANIFEST_FILE)
        }
    }

    /// The manifest.
    #[must_use]
    pub fn manifest(&self) -> &Manifest {
        &self.manifest
    }

    /// Case directory.
    #[must_use]
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Fixtures root the case was opened from.
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// Case id.
    #[must_use]
    pub fn case_id(&self) -> &str {
        &self.manifest.case_id
    }

    /// Fail unless the fixture holds oracle (golden) values.
    pub fn require_golden(&self) -> Result<()> {
        if self.manifest.status == FixtureStatus::Golden {
            Ok(())
        } else {
            Err(Error::manifest(
                self.manifest_path(),
                format!(
                    "fixture status is {:?}; only golden fixtures can be compared against",
                    self.manifest.status
                ),
            ))
        }
    }

    /// Check that every data file is present; for external fixtures report
    /// how to regenerate them.
    pub fn check_materialized(&self) -> Result<()> {
        let missing = self
            .manifest
            .quantities
            .iter()
            .any(|q| !self.data_dir.join(&q.file).is_file());
        if !missing {
            return Ok(());
        }
        let hint = match (&self.manifest.storage.mode, &self.manifest.storage.generate) {
            (StorageMode::External, Some(cmd)) => cmd.clone(),
            _ => format!(
                "{} (committed fixture with missing files: check your checkout)",
                self.manifest.generator.command
            ),
        };
        Err(Error::FixtureNotMaterialized {
            path: self.data_dir.clone(),
            hint,
        })
    }

    /// Verify the SHA-256 of every data file and stored input.
    pub fn verify_checksums(&self) -> Result<()> {
        self.check_materialized()?;
        for q in &self.manifest.quantities {
            verify_file(&self.data_dir.join(&q.file), &q.sha256)?;
        }
        for i in &self.manifest.inputs {
            if let Some(p) = &i.path {
                verify_file(&self.dir.join(p), &i.sha256)?;
            }
        }
        Ok(())
    }

    /// Metadata of a quantity.
    pub fn quantity(&self, name: &str) -> Result<&Quantity> {
        self.manifest
            .quantity(name)
            .ok_or_else(|| Error::UnknownQuantity {
                case_id: self.manifest.case_id.clone(),
                name: name.to_string(),
            })
    }

    /// Names of quantities matching a glob pattern, in manifest order.
    #[must_use]
    pub fn quantity_names(&self, pattern: &str) -> Vec<&str> {
        self.manifest
            .quantities
            .iter()
            .filter(|q| tolerance::glob_match(pattern, &q.name))
            .map(|q| q.name.as_str())
            .collect()
    }

    /// Load the raw array of a quantity, verifying its checksum and that the
    /// file agrees with the manifest (dtype, shape).
    pub fn load(&self, name: &str) -> Result<NpyArray> {
        let q = self.quantity(name)?;
        let path = self.data_dir.join(&q.file);
        if !path.is_file() {
            self.check_materialized()?;
        }
        verify_file(&path, &q.sha256)?;
        let a = read_npy(&path)?;
        if a.shape != q.shape {
            return Err(Error::manifest(
                &path,
                format!(
                    "file shape {:?} differs from manifest shape {:?}",
                    a.shape, q.shape
                ),
            ));
        }
        if Some(a.dtype()) != q.parsed_dtype() {
            return Err(Error::manifest(
                &path,
                format!(
                    "file dtype {} differs from manifest dtype {}",
                    a.dtype().name(),
                    q.dtype
                ),
            ));
        }
        Ok(a)
    }

    /// Load a floating-point quantity as a [`Field`] with its axis names.
    pub fn load_f64(&self, name: &str) -> Result<Field<f64>> {
        let q = self.quantity(name)?;
        let a = self.load(name)?;
        Field::new(a.shape.clone(), a.to_f64()?, q.axes.clone())
    }

    /// Load an integer quantity as a [`Field`] with its axis names.
    pub fn load_i64(&self, name: &str) -> Result<Field<i64>> {
        let q = self.quantity(name)?;
        let a = self.load(name)?;
        Field::new(a.shape.clone(), a.to_i64()?, q.axes.clone())
    }

    /// Load a ragged quantity as one vector per row.
    pub fn load_ragged_f64(&self, name: &str) -> Result<Vec<Vec<f64>>> {
        let q = self.quantity(name)?;
        let ragged = q
            .ragged
            .as_ref()
            .filter(|_| q.layout == Layout::RaggedValues)
            .ok_or_else(|| {
                Error::manifest(
                    self.manifest_path(),
                    format!("`{name}` is not a ragged quantity"),
                )
            })?;
        let values = self.load(name)?.to_f64()?;
        let offsets = self.load(&ragged.offsets)?.to_i64()?;
        split_ragged(&values, &offsets)
    }

    /// Path of a stored input file by role.
    pub fn input_path(&self, role: &str) -> Result<PathBuf> {
        let input = self
            .manifest
            .inputs
            .iter()
            .find(|i| i.role == role)
            .ok_or_else(|| {
                Error::manifest(self.manifest_path(), format!("no input with role `{role}`"))
            })?;
        let p = input.path.as_ref().ok_or_else(|| {
            Error::manifest(
                self.manifest_path(),
                format!(
                    "input `{role}` is not stored with the fixture (source: {})",
                    input.source.as_deref().unwrap_or("?")
                ),
            )
        })?;
        let full = self.dir.join(p);
        verify_file(&full, &input.sha256)?;
        Ok(full)
    }

    /// Tolerance of a quantity from the `tolerances.json` files.
    pub fn tolerance(&self, name: &str) -> Result<ResolvedTolerance> {
        tolerance::resolve(&self.root, &self.manifest.case_id, name)
    }
}

/// Split CSR-style ragged values into rows.
pub fn split_ragged(values: &[f64], offsets: &[i64]) -> Result<Vec<Vec<f64>>> {
    if offsets.first() != Some(&0) {
        return Err(Error::shape("ragged offsets must start at 0"));
    }
    let mut rows = Vec::with_capacity(offsets.len().saturating_sub(1));
    for w in offsets.windows(2) {
        let (a, b) = (
            usize::try_from(w[0]).map_err(|_| Error::shape("negative ragged offset"))?,
            usize::try_from(w[1]).map_err(|_| Error::shape("negative ragged offset"))?,
        );
        if a > b || b > values.len() {
            return Err(Error::shape(format!(
                "invalid ragged offsets {a}..{b} for {} values",
                values.len()
            )));
        }
        rows.push(values[a..b].to_vec());
    }
    if offsets.last().and_then(|&l| usize::try_from(l).ok()) != Some(values.len()) {
        return Err(Error::shape(
            "last ragged offset must equal the number of values",
        ));
    }
    Ok(rows)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ragged_split() {
        let rows = split_ragged(&[1.0, 2.0, 3.0], &[0, 2, 2, 3]).unwrap();
        assert_eq!(rows, vec![vec![1.0, 2.0], vec![], vec![3.0]]);
        assert!(split_ragged(&[1.0], &[1, 1]).is_err());
        assert!(split_ragged(&[1.0], &[0, 2]).is_err());
        assert!(split_ragged(&[1.0, 2.0], &[0, 1]).is_err());
        assert!(split_ragged(&[1.0], &[0, -1]).is_err());
    }

    use crate::manifest::{BuildInfo, Generator, ReferenceInfo};
    use crate::writer::FixtureWriter;

    fn build(arch: &str, core: Option<&str>) -> BuildInfo {
        let mut b = BuildInfo {
            config: "strict".into(),
            compiler: "g++ 14.2.0".into(),
            arch: arch.into(),
            ..BuildInfo::default()
        };
        if let Some(c) = core {
            b.libraries.insert("openblas_coretype".into(), c.into());
        }
        b
    }

    /// A golden case `test/variants/case` under `root` with value `v`.
    fn write_case(root: &Path, b: BuildInfo, v: f64, input: &Path) -> PathBuf {
        let mut w = FixtureWriter::create(
            root,
            "test/variants/case",
            "platform variant unit test",
            FixtureStatus::Golden,
            Generator {
                probe: "unit-test".into(),
                probe_version: "1".into(),
                command: String::new(),
            },
            ReferenceInfo {
                sha: "35a851780d0b9471d258ed53a003ae98aaa95ea0".into(),
                ..ReferenceInfo::default()
            },
            Some(b),
        )
        .unwrap();
        w.add_f64("u", &[2], &[v, 2.0 * v], &["node"], Layout::Nodal)
            .unwrap();
        w.add_input_file("mesh", input, Some("generated:test".into()))
            .unwrap();
        w.finish().unwrap()
    }

    /// x86_64 top level + aarch64 variant, built like the D-073 migration:
    /// the aarch64 manifest moved to `variants/aarch64-auto/variant.json`.
    fn case_with_variant() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("mesh.msh");
        std::fs::write(&input, "$MeshFormat\n").unwrap();
        let top_root = tmp.path().join("fixtures");
        let arm_root = tmp.path().join("arm");
        let case = write_case(&top_root, build("x86_64", Some("Haswell")), 1.0, &input);
        let arm = write_case(&arm_root, build("aarch64", None), 3.0, &input);
        let vdir = case.join(VARIANTS_DIR).join("aarch64-auto");
        std::fs::create_dir_all(&vdir).unwrap();
        std::fs::rename(arm.join(MANIFEST_FILE), vdir.join(VARIANT_MANIFEST_FILE)).unwrap();
        std::fs::rename(arm.join("data"), vdir.join("data")).unwrap();
        (tmp, top_root)
    }

    /// D-121: x86_64 always compares (bit for bit the earlier behaviour);
    /// another platform compares only in the recorded environment.
    #[test]
    fn oracle_comparability() {
        let rec = "x86_64-haswell";
        assert_eq!(
            oracle_values_not_comparable_for("x86_64", Some(rec), rec),
            None
        );
        assert_eq!(oracle_values_not_comparable_for("x86_64", None, rec), None);
        assert_eq!(
            oracle_values_not_comparable_for("x86_64", Some("aarch64-auto"), rec),
            None
        );
        let why = oracle_values_not_comparable_for("aarch64", Some("aarch64-auto"), rec).unwrap();
        assert!(
            why.contains("x86_64-haswell") && why.contains("aarch64-auto"),
            "{why}"
        );
        let why = oracle_values_not_comparable_for("riscv64", None, rec).unwrap();
        assert!(why.contains("riscv64, no recorded environment"), "{why}");
        assert_eq!(
            oracle_values_not_comparable_for("aarch64", Some("aarch64-auto"), "aarch64-auto"),
            None
        );
        assert_eq!(
            oracle_report_line("s", rec, None),
            "ORACLE s: compared with the record of x86_64-haswell"
        );
        assert_eq!(
            oracle_report_line("s", rec, Some("why")),
            "ORACLE s: not compared: why"
        );
    }

    #[test]
    fn environment_ids() {
        assert_eq!(
            environment_id(&build("x86_64", Some("Haswell"))),
            "x86_64-haswell"
        );
        assert_eq!(environment_id(&build("aarch64", None)), "aarch64-auto");
        assert_eq!(
            environment_id(&build(
                "x86_64",
                Some("auto (unset: chosen from the host CPU)")
            )),
            "x86_64-auto"
        );
        assert_eq!(
            running_environment_for("x86_64", None).as_deref(),
            Some("x86_64-haswell")
        );
        assert_eq!(
            running_environment_for("x86_64", Some("HASWELL")).as_deref(),
            Some("x86_64-haswell")
        );
        assert_eq!(running_environment_for("x86_64", Some("Cooperlake")), None);
        assert_eq!(
            running_environment_for("aarch64", None).as_deref(),
            Some("aarch64-auto")
        );
        assert_eq!(running_environment_for("aarch64", Some("NEOVERSEN1")), None);
        assert_eq!(running_environment_for("riscv64", None), None);
    }

    #[test]
    fn variant_selected_by_environment() {
        let (_tmp, root) = case_with_variant();
        let id = "test/variants/case";
        let x86 = Fixture::open_case_in(&root, id, Some("x86_64-haswell")).unwrap();
        assert_eq!(x86.variant(), None);
        assert_eq!(x86.environment(), "x86_64-haswell");
        assert_eq!(x86.load_f64("u").unwrap().data, vec![1.0, 2.0]);
        x86.verify_checksums().unwrap();
        let arm = Fixture::open_case_in(&root, id, Some("aarch64-auto")).unwrap();
        assert_eq!(arm.variant(), Some("aarch64-auto"));
        assert_eq!(arm.environment(), "aarch64-auto");
        assert_eq!(arm.load_f64("u").unwrap().data, vec![3.0, 6.0]);
        arm.verify_checksums().unwrap();
        assert_eq!(
            arm.input_path("mesh").unwrap(),
            x86.input_path("mesh").unwrap()
        );
        assert_eq!(
            arm.environments().unwrap(),
            vec!["x86_64-haswell".to_string(), "aarch64-auto".to_string()]
        );
        // The top-level opening ignores the variants.
        let top = Fixture::open_case_top(&root, id).unwrap();
        assert_eq!(top.load_f64("u").unwrap().data, vec![1.0, 2.0]);
        let top_dir = Fixture::open_top(&root.join("test/variants/case")).unwrap();
        assert_eq!(top_dir.variant(), None);
    }

    #[test]
    fn case_with_variants_fails_for_an_unknown_environment() {
        let (_tmp, root) = case_with_variant();
        let id = "test/variants/case";
        let e = Fixture::open_case_in(&root, id, None).unwrap_err();
        assert!(
            matches!(&e, Error::FixtureEnvironment { environment: None, available, .. }
                if available == &["x86_64-haswell", "aarch64-auto"]),
            "{e}"
        );
        let e = Fixture::open_case_in(&root, id, Some("x86_64-cooperlake")).unwrap_err();
        assert!(
            matches!(&e, Error::FixtureEnvironment { environment: Some(env), .. } if env == "x86_64-cooperlake"),
            "{e}"
        );
    }

    #[test]
    fn case_without_variants_opens_everywhere() {
        let tmp = tempfile::tempdir().unwrap();
        let input = tmp.path().join("mesh.msh");
        std::fs::write(&input, "$MeshFormat\n").unwrap();
        let root = tmp.path().join("fixtures");
        write_case(&root, build("aarch64", None), 1.0, &input);
        for env in [None, Some("x86_64-haswell"), Some("riscv64-auto")] {
            let fx = Fixture::open_case_in(&root, "test/variants/case", env).unwrap();
            assert_eq!(fx.variant(), None);
        }
    }

    #[test]
    fn inconsistent_variants_are_refused() {
        // Directory name and the variant's recorded environment disagree.
        let (_tmp, root) = case_with_variant();
        let case = root.join("test/variants/case");
        let v = case.join(VARIANTS_DIR);
        std::fs::rename(v.join("aarch64-auto"), v.join("aarch64-neoversen1")).unwrap();
        assert!(
            Fixture::open_case_in(&root, "test/variants/case", Some("aarch64-neoversen1")).is_err()
        );
        // A variant repeating the top level's environment.
        std::fs::rename(v.join("aarch64-neoversen1"), v.join("x86_64-haswell")).unwrap();
        assert!(
            Fixture::open_case_in(&root, "test/variants/case", Some("x86_64-haswell")).is_err()
        );
        // A variant directory without its manifest.
        std::fs::rename(v.join("x86_64-haswell"), v.join("aarch64-auto")).unwrap();
        std::fs::create_dir_all(v.join("empty")).unwrap();
        assert!(Fixture::open_case_in(&root, "test/variants/case", Some("aarch64-auto")).is_err());
    }

    #[test]
    fn variant_inputs_must_be_the_shared_ones() {
        let (_tmp, root) = case_with_variant();
        let vm = root
            .join("test/variants/case/variants/aarch64-auto")
            .join(VARIANT_MANIFEST_FILE);
        let mut m = Manifest::read(&vm).unwrap();
        m.inputs[0].sha256 = "0".repeat(64);
        std::fs::write(&vm, m.to_json_string().unwrap()).unwrap();
        assert!(Fixture::open_case_in(&root, "test/variants/case", Some("aarch64-auto")).is_err());
    }

    #[test]
    fn root_is_workspace_fixtures() {
        if std::env::var_os(FIXTURES_DIR_ENV).is_none() {
            let root = fixtures_root();
            assert!(root.ends_with("fixtures"));
            assert!(workspace_root().join("Cargo.toml").is_file());
        }
    }
}
