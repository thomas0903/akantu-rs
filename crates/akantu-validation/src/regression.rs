// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it belongs to the validation harness.

//! Persisting failing inputs as new regression cases.
//!
//! When a Rust test fails on inputs that are not already a fixture (a
//! randomly generated strain path, a property-test counterexample, a mesh
//! built in code), the inputs are written as a fixture case with status
//! [`FixtureStatus::PendingOracle`]. Running it through the oracle
//! (`oracle/scripts/generate-fixtures.sh regression:<case_id>`, the single
//! `regression:<case_id>` target of the driver) turns it into a golden case
//! that is then committed under `fixtures/regression/`.

use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::fixture::MANIFEST_FILE;
use crate::manifest::{FixtureStatus, Generator, Layout, ReferenceInfo};
use crate::npy::NpyArray;
use crate::writer::FixtureWriter;

/// Reference SHA recorded in persisted cases (the oracle commit this port
/// tracks; see `docs/REFERENCE.md`).
pub const REFERENCE_SHA: &str = "35a851780d0b9471d258ed53a003ae98aaa95ea0";

/// An input array of a regression case.
#[derive(Debug, Clone)]
pub struct InputArray {
    /// Quantity name.
    pub name: String,
    /// Data.
    pub array: NpyArray,
    /// Axis names.
    pub axes: Vec<String>,
    /// Layout.
    pub layout: Layout,
}

/// Description of a regression case to persist.
#[derive(Debug, Clone, Default)]
pub struct RegressionCase {
    /// Requested case id (`regression/<area>/<name>`); a numeric suffix is
    /// appended if it already exists.
    pub case_id: String,
    /// What the case reproduces.
    pub description: String,
    /// Where it came from (test name, fixture, report path).
    pub origin: String,
    /// Oracle probe expected to process the case (e.g. `material_probe`).
    pub probe: String,
    /// Free-form parameters (material constants, ...).
    pub parameters: serde_json::Value,
    /// Input files `(role, path)` copied into the case.
    pub input_files: Vec<(String, PathBuf)>,
    /// Input arrays (e.g. prescribed strain history).
    pub input_arrays: Vec<InputArray>,
}

/// Write `case` under `root` with status `pending_oracle`. Returns the case
/// directory. Existing cases are never overwritten.
pub fn persist_regression_case(root: &Path, case: &RegressionCase) -> Result<PathBuf> {
    let case_id = free_case_id(root, &case.case_id);
    let mut w = FixtureWriter::create(
        root,
        &case_id,
        &case.description,
        FixtureStatus::PendingOracle,
        Generator {
            probe: if case.probe.is_empty() {
                "pending".into()
            } else {
                case.probe.clone()
            },
            probe_version: "0".into(),
            command: replay_command(&case_id),
        },
        ReferenceInfo {
            sha: REFERENCE_SHA.into(),
            describe: None,
            patches: Vec::new(),
        },
        None,
    )?;
    {
        let m = w.manifest_mut();
        m.parameters = case.parameters.clone();
        m.tags = vec!["regression".into()];
        m.notes = Some(format!("persisted from: {}", case.origin));
    }
    for (role, path) in &case.input_files {
        w.add_input_file(role, path, Some(format!("persisted:{}", path.display())))?;
    }
    for a in &case.input_arrays {
        let axes: Vec<&str> = a.axes.iter().map(String::as_str).collect();
        w.add_array(&a.name, &a.array, &axes, a.layout)?.description = "input".into();
    }
    w.finish()
}

/// Exact driver command replaying a persisted case through the oracle
/// (`oracle/scripts/generate-fixtures.sh` accepts the single target
/// `regression:<case_id>`; see `oracle/probes/run-in-container.sh`).
#[must_use]
pub fn replay_command(case_id: &str) -> String {
    format!("oracle/scripts/generate-fixtures.sh regression:{case_id}")
}

fn free_case_id(root: &Path, wanted: &str) -> String {
    let exists = |id: &str| {
        id.split('/')
            .fold(root.to_path_buf(), |p, s| p.join(s))
            .join(MANIFEST_FILE)
            .exists()
    };
    if !exists(wanted) {
        return wanted.to_string();
    }
    (2..)
        .map(|k| format!("{wanted}_{k}"))
        .find(|id| !exists(id))
        .unwrap_or_else(|| wanted.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fixture::Fixture;
    use crate::npy::NpyData;

    #[test]
    fn persist_twice_gets_suffix() {
        let dir = tempfile::tempdir().unwrap();
        let case = RegressionCase {
            case_id: "regression/materials/strain_path".into(),
            description: "random strain path that broke the return mapping".into(),
            origin: "materials::plastic::random_paths seed 42".into(),
            probe: "material_probe".into(),
            parameters: serde_json::json!({"material": "plastic_linear_isotropic_hardening", "E": 2.1e11}),
            input_files: vec![],
            input_arrays: vec![InputArray {
                name: "grad_u".into(),
                array: NpyArray::new(vec![2, 3, 3], NpyData::F64(vec![0.0; 18])).unwrap(),
                axes: vec!["step".into(), "row".into(), "col".into()],
                layout: Layout::Table,
            }],
        };
        let a = persist_regression_case(dir.path(), &case).unwrap();
        let b = persist_regression_case(dir.path(), &case).unwrap();
        assert!(a.ends_with("strain_path"));
        assert!(b.ends_with("strain_path_2"));
        let fx = Fixture::open(&b).unwrap();
        assert_eq!(fx.manifest().status, FixtureStatus::PendingOracle);
        assert!(fx.require_golden().is_err());
        assert_eq!(fx.load_f64("grad_u").unwrap().shape, vec![2, 3, 3]);
        // The persisted command must be the exact, executable replay
        // syntax of the driver (`regression:<case_id>`, colon form).
        assert_eq!(
            fx.manifest().generator.command,
            "oracle/scripts/generate-fixtures.sh regression:regression/materials/strain_path_2"
        );
        assert_eq!(
            replay_command("regression/material/x"),
            "oracle/scripts/generate-fixtures.sh regression:regression/material/x"
        );
    }

    /// The persisted command is accepted verbatim by the driver: run it
    /// with `--dry-run` (validates the target grammar without Docker). The
    /// former space form `regression <case_id>` is rejected.
    #[test]
    fn replay_command_is_accepted_by_the_driver() {
        let root = crate::fixture::workspace_root();
        let run = |cmd: &str| {
            let mut args = cmd.split(' ');
            let script = args.next().unwrap();
            std::process::Command::new("bash")
                .arg(root.join(script))
                .arg("--dry-run")
                .args(args)
                .output()
                .unwrap()
        };
        let cmd = replay_command("regression/material/strain_path_2");
        let o = run(&cmd);
        let out = String::from_utf8_lossy(&o.stdout);
        assert!(
            o.status.success(),
            "{out}{}",
            String::from_utf8_lossy(&o.stderr)
        );
        assert!(
            out.contains("targets=regression:regression/material/strain_path_2"),
            "{out}"
        );
        let old = "oracle/scripts/generate-fixtures.sh regression regression/material/x";
        assert!(!run(old).status.success(), "space form must be rejected");
    }
}
