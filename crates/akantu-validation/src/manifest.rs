// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it implements the manifest of the golden fixture
// format (docs/NUMERICAL_VALIDATION.md).

//! Fixture manifest (`manifest.json`) types and validation.
//!
//! The manifest is the single source of truth describing a fixture case:
//! provenance (reference SHA, build configuration, generator command),
//! inputs with checksums, parallel layout, and every recorded quantity with
//! its shape semantics. The schema is versioned by
//! [`Manifest::format_version`]; unknown fields are rejected so that typos
//! never silently drop information.

use std::collections::{BTreeMap, BTreeSet};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::npy::Dtype;

/// Current manifest schema version.
pub const FORMAT_VERSION: u32 = 1;

/// Axis names allowed in [`Quantity::axes`].
///
/// The vocabulary is closed on purpose: every axis name has one documented
/// meaning (docs/NUMERICAL_VALIDATION.md), which is what
/// makes reports and ordering normalisation generic. Extending it is a code
/// and documentation change.
pub const AXIS_VOCABULARY: &[&str] = &[
    "step",         // recorded time step / load step (see `steps`)
    "iteration",    // solver iteration inside a step
    "node",         // mesh node (local numbering of the producing process)
    "element",      // element of one type and ghost type
    "quad",         // quadrature (integration) point inside an element
    "node_local",   // node inside an element, local numbering
    "facet",        // facet of an element (local facet index)
    "component",    // vector/flattened-tensor component (see `components`)
    "row",          // first index of a matrix-valued entry
    "col",          // second index of a matrix-valued entry
    "dof",          // degree of freedom (global equation numbering)
    "natural_dim",  // natural (reference) coordinate direction
    "spatial_dim",  // physical coordinate direction
    "point",        // arbitrary evaluation point (not a quadrature point)
    "order",        // quadrature order or polynomial degree
    "rank",         // MPI rank
    "entry",        // generic table row / list entry
    "sample",       // independent sample of a parameter sweep
    "group_member", // member of a node or element group
    "nnz",          // stored non-zero of a sparse matrix
];

/// Life-cycle status of a fixture.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FixtureStatus {
    /// Values produced by the C++ oracle: the only status a differential
    /// test may compare against.
    Golden,
    /// Inputs persisted (e.g. from a failing Rust test) that still have to
    /// be run through the oracle. Not comparable.
    PendingOracle,
    /// Values produced by akantu-rs itself (diagnostics only, never used as
    /// a reference).
    RustSnapshot,
}

/// How the fixture's arrays are stored.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StorageMode {
    /// Arrays committed to git next to the manifest.
    #[default]
    Committed,
    /// Arrays too large for git: only the manifest is committed; the data is
    /// regenerated with [`Storage::generate`] or fetched from
    /// [`Storage::location`] and verified with the per-file checksums.
    External,
}

/// Storage policy of a fixture (size policy in docs/NUMERICAL_VALIDATION.md).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Storage {
    /// Committed or external.
    #[serde(default)]
    pub mode: StorageMode,
    /// Total size of the data files in bytes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bytes: Option<u64>,
    /// Where an external fixture's data lives (URL / bucket / path).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub location: Option<String>,
    /// Command (run from the repository root) regenerating the data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub generate: Option<String>,
    /// SHA-256 of the packed archive of an external fixture, if any.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub archive_sha256: Option<String>,
}

/// Program that produced the fixture.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Generator {
    /// Probe name (e.g. `element_probe`, `mesh_probe`, `akantu-validation`).
    pub probe: String,
    /// Probe version, bumped whenever its output semantics change.
    pub probe_version: String,
    /// Command line reproducing this case (from the repository root).
    #[serde(default)]
    pub command: String,
}

/// Reference (C++ Akantu) source identification.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReferenceInfo {
    /// Full git SHA of the reference checkout.
    pub sha: String,
    /// `git describe` of the reference checkout.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub describe: Option<String>,
    /// Patches from `oracle/patches/` applied to the reference build.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub patches: Vec<String>,
}

/// Build configuration of the program that produced the data.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BuildInfo {
    /// Oracle configuration name (`strict`, `default`, ...) or `rust`.
    pub config: String,
    /// Compiler identification (e.g. `g++ 14.2.0`).
    pub compiler: String,
    /// C++ flags of the oracle *library* build (`CMAKE_CXX_FLAGS` +
    /// build-type flags, including FP-relevant flags).
    #[serde(default)]
    pub cxx_flags: String,
    /// Effective C++ flags the *probe* translation unit was compiled with
    /// (recorded by the probe itself at compile time; for Python probes the
    /// library flags apply and this says so).
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub probe_cxx_flags: String,
    /// Actual compile command of the probe (from the probe build's
    /// `compile_commands.json`) or the interpreter invocation of a Python
    /// probe with the identity of the Akantu module it imported.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub probe_compile_command: String,
    /// CMake build type.
    #[serde(default)]
    pub build_type: String,
    /// Target architecture (`aarch64`, `x86_64`).
    pub arch: String,
    /// Operating system / distribution.
    #[serde(default)]
    pub os: String,
    /// Versions of numerically relevant libraries (Eigen, BLAS, MUMPS,
    /// PETSc, MPI, ...).
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub libraries: BTreeMap<String, String>,
}

/// An input file of the case (mesh, material `.dat`, parameter file...).
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InputFile {
    /// Role of the file (`mesh`, `material`, `parameters`, ...).
    pub role: String,
    /// Path relative to the case directory, when the file is stored with the
    /// fixture.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    /// Origin of the file, e.g. `upstream:test/test_fe_engine/_triangle_3.msh`
    /// (path inside the reference checkout) or `generated:<command>`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<String>,
    /// SHA-256 of the file content (lower-case hex).
    pub sha256: String,
}

/// How entries are distributed over MPI ranks.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Parallel {
    /// Number of MPI ranks of the oracle run.
    #[serde(default = "one")]
    pub ranks: u32,
    /// Rank whose local data this case holds (`None`: global / gathered).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rank: Option<u32>,
    /// Partitioner used (e.g. `scotch`), `None` for serial runs.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partitioner: Option<String>,
    /// Name of the quantity holding the element-to-rank partition.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub partition_quantity: Option<String>,
}

fn one() -> u32 {
    1
}

impl Default for Parallel {
    fn default() -> Self {
        Self {
            ranks: 1,
            rank: None,
            partitioner: None,
            partition_quantity: None,
        }
    }
}

/// Description of the recorded time steps.
#[derive(Debug, Clone, PartialEq, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Steps {
    /// Indices of the steps recorded along the `step` axis.
    pub recorded: Vec<u64>,
    /// Constant time step, when applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub dt: Option<f64>,
    /// Quantity holding the physical time of each recorded step.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time_quantity: Option<String>,
}

/// Expected outcome of the oracle run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Expectation {
    /// `ok` or `error`.
    #[serde(default = "ok_status")]
    pub status: ExpectedStatus,
    /// Process exit code.
    #[serde(default)]
    pub exit_code: i32,
    /// C++ exception type / error category, for `error` cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error_kind: Option<String>,
    /// Substring the error message must contain.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message_contains: Option<String>,
}

/// Expected status of the run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExpectedStatus {
    /// The run succeeds.
    Ok,
    /// The run must fail (error-path fixture).
    Error,
}

fn ok_status() -> ExpectedStatus {
    ExpectedStatus::Ok
}

impl Default for Expectation {
    fn default() -> Self {
        Self {
            status: ExpectedStatus::Ok,
            exit_code: 0,
            error_kind: None,
            message_contains: None,
        }
    }
}

/// Semantic layout of a quantity (what its leading axes index).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Layout {
    /// One value (or vector/tensor) per node: axes `[step?, node, ...]`.
    Nodal,
    /// One value per element: axes `[step?, element, ...]`.
    Elemental,
    /// One value per quadrature point: axes `[step?, element, quad, ...]`
    /// (Akantu's `ElementTypeMapArray` on integration points), or
    /// `[quad, ...]` for reference-element data.
    Quadrature,
    /// One value per degree of freedom: axes `[step?, dof]`.
    Dof,
    /// Scalar(s) per step or a single scalar: axes `[]` or `[step]`.
    Scalar,
    /// Per-element-type reference data (element class tables).
    Reference,
    /// Generic table (columns described by `components`).
    Table,
    /// Flat values of a ragged array (see [`Quantity::ragged`]).
    RaggedValues,
    /// CSR offsets of a ragged array (length = number of rows + 1).
    RaggedOffsets,
    /// Sparse matrix triplets or dense matrix.
    Matrix,
}

/// How entries along an axis are identified, for ordering normalisation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrderingKind {
    /// The order is canonical and must match exactly (default).
    Fixed,
    /// Entries are identified by integer ids held in `key_quantity`.
    GlobalId,
    /// Entries are identified by coordinates held in `key_quantity`
    /// (shape `[n, dim]`), matched within a tolerance.
    Coordinates,
    /// Elements identified by their connectivity in `key_quantity`
    /// (node sets, independent of local node rotation).
    Connectivity,
    /// The axis has set semantics: compare after sorting.
    Unordered,
}

/// Ordering key of one axis.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct OrderingKey {
    /// Axis name (must be one of the quantity's axes).
    pub axis: String,
    /// Identification method.
    pub kind: OrderingKind,
    /// Quantity providing the keys (required for `global_id`,
    /// `coordinates` and `connectivity`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub key_quantity: Option<String>,
}

/// Ragged-array description: `values` rows delimited by `offsets`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Ragged {
    /// Name of the `ragged_offsets` quantity.
    pub offsets: String,
    /// Meaning of a row (axis name, e.g. `step` for per-step Newton
    /// residual histories).
    pub row_axis: String,
}

/// One recorded quantity: an `.npy` file plus its semantics.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Quantity {
    /// Unique name inside the case; `/` may be used for grouping
    /// (`shapes/order_2`, `stress/_triangle_3`).
    pub name: String,
    /// `.npy` path relative to the case directory.
    pub file: String,
    /// Element type name (`f64`, `i64`, `i32`, `u64`, `u32`, `u8`, `bool`).
    pub dtype: String,
    /// Array shape (row-major).
    pub shape: Vec<usize>,
    /// One axis name per dimension, from [`AXIS_VOCABULARY`].
    pub axes: Vec<String>,
    /// Semantic layout.
    pub layout: Layout,
    /// SHA-256 of the `.npy` file.
    pub sha256: String,
    /// Free-text description.
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub description: String,
    /// Akantu element type, for per-type data (e.g. `_triangle_3`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub element_type: Option<String>,
    /// Akantu ghost type (`not_ghost` / `ghost`), for per-type data.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ghost_type: Option<String>,
    /// Labels of the `component` axis (e.g. `["xx","xy","yx","yy"]`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub components: Option<Vec<String>>,
    /// Ordering keys of axes whose order may legitimately differ.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ordering: Vec<OrderingKey>,
    /// Ragged structure, for `ragged_values` quantities.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ragged: Option<Ragged>,
    /// Physical units, if meaningful.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub units: Option<String>,
    /// Tolerance class hint (`kernel`, `linear_solve`, `nonlinear_solve`,
    /// `time_integration`, `exact`), see the tolerance methodology.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tolerance_class: Option<String>,
}

impl Quantity {
    /// Parsed element type.
    pub fn parsed_dtype(&self) -> Option<Dtype> {
        Dtype::from_name(&self.dtype)
    }

    /// Ordering key of `axis`, if any.
    #[must_use]
    pub fn ordering_of(&self, axis: &str) -> Option<&OrderingKey> {
        self.ordering.iter().find(|k| k.axis == axis)
    }
}

/// The `manifest.json` of a fixture case.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Manifest {
    /// Schema version, must equal [`FORMAT_VERSION`].
    pub format_version: u32,
    /// Case identifier = path of the case directory relative to the
    /// fixtures root, e.g. `fe/element_class/_triangle_3`.
    pub case_id: String,
    /// What the case exercises.
    pub description: String,
    /// Life-cycle status.
    pub status: FixtureStatus,
    /// Producing program.
    pub generator: Generator,
    /// Reference source identification.
    pub reference: ReferenceInfo,
    /// Build configuration (required for golden fixtures).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub build: Option<BuildInfo>,
    /// UTC creation timestamp (RFC 3339), informational.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub created: Option<String>,
    /// Input files.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub inputs: Vec<InputFile>,
    /// Free-form case parameters (material constants, time step, ...).
    #[serde(default, skip_serializing_if = "serde_json::Value::is_null")]
    pub parameters: serde_json::Value,
    /// Parallel layout.
    #[serde(default)]
    pub parallel: Parallel,
    /// Storage policy.
    #[serde(default)]
    pub storage: Storage,
    /// Recorded steps, for time-dependent cases.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub steps: Option<Steps>,
    /// Recorded quantities.
    pub quantities: Vec<Quantity>,
    /// Expected outcome of the run.
    #[serde(default)]
    pub expect: Expectation,
    /// Search tags (`regular`, `cohesive`, `mpi`, ...).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub tags: Vec<String>,
    /// Free-text notes (known upstream issues, caveats).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
}

impl Manifest {
    /// Parse a manifest from JSON text and validate it.
    pub fn from_json_str(text: &str, path: &Path) -> Result<Self> {
        let m: Manifest = serde_json::from_str(text).map_err(|e| Error::json(path, e))?;
        m.validate(path)?;
        Ok(m)
    }

    /// Read and validate `manifest.json`.
    pub fn read(path: &Path) -> Result<Self> {
        let text = std::fs::read_to_string(path).map_err(|e| Error::io(path, e))?;
        Self::from_json_str(&text, path)
    }

    /// Serialise to pretty JSON (stable field order, trailing newline).
    pub fn to_json_string(&self) -> Result<String> {
        let mut s = serde_json::to_string_pretty(self).map_err(|e| Error::json("", e))?;
        s.push('\n');
        Ok(s)
    }

    /// Look a quantity up by name.
    #[must_use]
    pub fn quantity(&self, name: &str) -> Option<&Quantity> {
        self.quantities.iter().find(|q| q.name == name)
    }

    /// Area of the case (first component of the case id).
    #[must_use]
    pub fn area(&self) -> &str {
        self.case_id.split('/').next().unwrap_or("")
    }

    /// Check every rule of the format specification that the type system
    /// does not enforce. `path` is only used in error messages.
    pub fn validate(&self, path: &Path) -> Result<()> {
        let bad = |reason: String| Err(Error::manifest(path, reason));
        if self.format_version != FORMAT_VERSION {
            return bad(format!(
                "format_version {} is not supported (expected {FORMAT_VERSION})",
                self.format_version
            ));
        }
        if !valid_case_id(&self.case_id) {
            return bad(format!(
                "case_id `{}` must be a relative path of [A-Za-z0-9_.+-] segments, at least <area>/<case>",
                self.case_id
            ));
        }
        if self.description.trim().is_empty() {
            return bad("description must not be empty".into());
        }
        if self.generator.probe.trim().is_empty() {
            return bad("generator.probe must not be empty".into());
        }
        if self.status == FixtureStatus::Golden {
            if !is_hex_sha(&self.reference.sha, 40) {
                return bad(format!(
                    "golden fixtures need the full 40-hex reference SHA, got `{}`",
                    self.reference.sha
                ));
            }
            match &self.build {
                None => return bad("golden fixtures must record the build configuration".into()),
                Some(b) if b.config.is_empty() || b.compiler.is_empty() || b.arch.is_empty() => {
                    return bad("build.config, build.compiler and build.arch are required".into());
                }
                Some(_) => {}
            }
        }
        if self.parallel.ranks == 0 {
            return bad("parallel.ranks must be >= 1".into());
        }
        if let Some(r) = self.parallel.rank
            && r >= self.parallel.ranks
        {
            return bad(format!(
                "parallel.rank {r} >= ranks {}",
                self.parallel.ranks
            ));
        }
        if self.storage.mode == StorageMode::External && self.storage.generate.is_none() {
            return bad("external fixtures must give storage.generate".into());
        }
        for input in &self.inputs {
            if input.path.is_none() && input.source.is_none() {
                return bad(format!("input `{}` needs a path or a source", input.role));
            }
            if let Some(p) = &input.path
                && !valid_relative_path(p)
            {
                return bad(format!(
                    "input path `{p}` must be relative and stay inside the case"
                ));
            }
            if !is_hex_sha(&input.sha256, 64) {
                return bad(format!("input `{}` has an invalid sha256", input.role));
            }
        }
        let mut names = BTreeSet::new();
        let mut files = BTreeSet::new();
        for q in &self.quantities {
            self.validate_quantity(q, path)?;
            if !names.insert(q.name.as_str()) {
                return bad(format!("duplicate quantity name `{}`", q.name));
            }
            if !files.insert(q.file.as_str()) {
                return bad(format!("file `{}` is used by two quantities", q.file));
            }
        }
        // Cross references.
        for q in &self.quantities {
            for key in &q.ordering {
                if let Some(k) = &key.key_quantity
                    && self.quantity(k).is_none()
                {
                    return bad(format!(
                        "quantity `{}`: ordering key quantity `{k}` does not exist",
                        q.name
                    ));
                }
            }
            if let Some(r) = &q.ragged {
                match self.quantity(&r.offsets) {
                    Some(o) if o.layout == Layout::RaggedOffsets => {
                        let total = o.shape.first().copied().unwrap_or(0);
                        if total == 0 {
                            return bad(format!("offsets `{}` must not be empty", r.offsets));
                        }
                    }
                    _ => {
                        return bad(format!(
                            "quantity `{}`: ragged offsets `{}` missing or not ragged_offsets",
                            q.name, r.offsets
                        ));
                    }
                }
            }
        }
        if let Some(steps) = &self.steps {
            if let Some(t) = &steps.time_quantity
                && self.quantity(t).is_none()
            {
                return bad(format!("steps.time_quantity `{t}` does not exist"));
            }
            if steps.recorded.windows(2).any(|w| w[0] >= w[1]) {
                return bad("steps.recorded must be strictly increasing".into());
            }
            for q in &self.quantities {
                if let Some(pos) = q.axes.iter().position(|a| a == "step")
                    && q.shape[pos] != steps.recorded.len()
                {
                    return bad(format!(
                        "quantity `{}` has {} steps, steps.recorded has {}",
                        q.name,
                        q.shape[pos],
                        steps.recorded.len()
                    ));
                }
            }
        }
        if let Some(pq) = &self.parallel.partition_quantity
            && self.quantity(pq).is_none()
        {
            return bad(format!("parallel.partition_quantity `{pq}` does not exist"));
        }
        Ok(())
    }

    fn validate_quantity(&self, q: &Quantity, path: &Path) -> Result<()> {
        let bad = |reason: String| {
            Err(Error::manifest(
                path,
                format!("quantity `{}`: {reason}", q.name),
            ))
        };
        if q.name.is_empty() || q.name.chars().any(|c| c.is_whitespace()) {
            return bad("name must be non-empty without whitespace".into());
        }
        if !valid_relative_path(&q.file) || !q.file.ends_with(".npy") {
            return bad(format!(
                "file `{}` must be a relative .npy path inside the case",
                q.file
            ));
        }
        if q.parsed_dtype().is_none() {
            return bad(format!("unknown dtype `{}`", q.dtype));
        }
        if q.axes.len() != q.shape.len() {
            return bad(format!(
                "{} axes for a {}-d shape",
                q.axes.len(),
                q.shape.len()
            ));
        }
        let mut seen = BTreeSet::new();
        for a in &q.axes {
            if !AXIS_VOCABULARY.contains(&a.as_str()) {
                return bad(format!("axis `{a}` is not in the axis vocabulary"));
            }
            if !seen.insert(a.as_str()) {
                return bad(format!("axis `{a}` appears twice"));
            }
        }
        if !is_hex_sha(&q.sha256, 64) {
            return bad("invalid sha256".into());
        }
        if let Some(c) = &q.components {
            match q.axes.iter().position(|a| a == "component") {
                Some(pos) if q.shape[pos] == c.len() => {}
                Some(pos) => {
                    return bad(format!(
                        "{} component labels for a component axis of length {}",
                        c.len(),
                        q.shape[pos]
                    ));
                }
                None => return bad("component labels given but no `component` axis".into()),
            }
        }
        for key in &q.ordering {
            if !q.axes.contains(&key.axis) {
                return bad(format!("ordering key for absent axis `{}`", key.axis));
            }
            let needs_key = matches!(
                key.kind,
                OrderingKind::GlobalId | OrderingKind::Coordinates | OrderingKind::Connectivity
            );
            if needs_key && key.key_quantity.is_none() {
                return bad(format!(
                    "ordering of axis `{}` needs key_quantity",
                    key.axis
                ));
            }
        }
        if q.layout == Layout::RaggedValues && q.ragged.is_none() {
            return bad("ragged_values quantities need a `ragged` description".into());
        }
        if q.layout == Layout::RaggedOffsets
            && (q.shape.len() != 1 || !matches!(q.parsed_dtype(), Some(Dtype::I64 | Dtype::U64)))
        {
            return bad("ragged_offsets must be a 1-d i64/u64 array".into());
        }
        Ok(())
    }
}

/// `true` if `s` is a lower-case hex string of `len` characters.
#[must_use]
pub fn is_hex_sha(s: &str, len: usize) -> bool {
    s.len() == len
        && s.bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn valid_segment(seg: &str) -> bool {
    !seg.is_empty()
        && seg != "."
        && seg != ".."
        && seg
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'-' | b'.' | b'+'))
}

/// Case ids are `<area>/<case>[/<sub>...]` with portable segments.
#[must_use]
pub fn valid_case_id(id: &str) -> bool {
    let segs: Vec<&str> = id.split('/').collect();
    segs.len() >= 2 && segs.iter().all(|s| valid_segment(s))
}

/// Relative path with portable segments, no `..`, no absolute prefix.
#[must_use]
pub fn valid_relative_path(p: &str) -> bool {
    !p.is_empty() && !p.starts_with('/') && p.split('/').all(valid_segment)
}

#[cfg(test)]
mod tests {
    use super::*;

    const SHA40: &str = "35a851780d0b9471d258ed53a003ae98aaa95ea0";
    const SHA64: &str = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";

    fn sample() -> Manifest {
        Manifest {
            format_version: FORMAT_VERSION,
            case_id: "fe/element_class/_triangle_3".into(),
            description: "reference data".into(),
            status: FixtureStatus::Golden,
            generator: Generator {
                probe: "element_probe".into(),
                probe_version: "1".into(),
                command: "oracle/scripts/generate-fixtures.sh element".into(),
            },
            reference: ReferenceInfo {
                sha: SHA40.into(),
                describe: None,
                patches: vec![],
            },
            build: Some(BuildInfo {
                config: "strict".into(),
                compiler: "g++ 14.2.0".into(),
                cxx_flags: "-ffp-contract=off".into(),
                probe_cxx_flags: String::new(),
                probe_compile_command: String::new(),
                build_type: "RelWithDebInfo".into(),
                arch: "aarch64".into(),
                os: "debian trixie".into(),
                libraries: BTreeMap::new(),
            }),
            created: None,
            inputs: vec![],
            parameters: serde_json::Value::Null,
            parallel: Parallel::default(),
            storage: Storage::default(),
            steps: None,
            quantities: vec![Quantity {
                name: "shapes/order_1".into(),
                file: "data/shapes_order_1.npy".into(),
                dtype: "f64".into(),
                shape: vec![1, 3],
                axes: vec!["quad".into(), "node_local".into()],
                layout: Layout::Quadrature,
                sha256: SHA64.into(),
                description: String::new(),
                element_type: Some("_triangle_3".into()),
                ghost_type: None,
                components: None,
                ordering: vec![],
                ragged: None,
                units: None,
                tolerance_class: Some("kernel".into()),
            }],
            expect: Expectation::default(),
            tags: vec![],
            notes: None,
        }
    }

    #[test]
    fn json_roundtrip_and_validation() {
        let m = sample();
        let text = m.to_json_string().unwrap();
        let back = Manifest::from_json_str(&text, Path::new("m.json")).unwrap();
        assert_eq!(m, back);
        assert_eq!(back.area(), "fe");
        assert!(back.quantity("shapes/order_1").is_some());
    }

    #[test]
    fn rejects_unknown_fields_and_bad_values() {
        let text = sample().to_json_string().unwrap();
        let typo = text.replacen("\"description\"", "\"descripton\"", 1);
        assert!(Manifest::from_json_str(&typo, Path::new("m")).is_err());

        let mut m = sample();
        m.quantities[0].axes[0] = "quadrature".into();
        let e = m.validate(Path::new("m")).unwrap_err().to_string();
        assert!(e.contains("axis vocabulary"), "{e}");

        let mut m = sample();
        m.build = None;
        assert!(m.validate(Path::new("m")).is_err());
        m.status = FixtureStatus::PendingOracle;
        m.validate(Path::new("m")).unwrap();

        let mut m = sample();
        m.case_id = "../escape".into();
        assert!(m.validate(Path::new("m")).is_err());

        let mut m = sample();
        m.quantities[0].file = "../x.npy".into();
        assert!(m.validate(Path::new("m")).is_err());

        let mut m = sample();
        m.quantities.push(m.quantities[0].clone());
        m.quantities[1].file = "data/other.npy".into();
        assert!(
            m.validate(Path::new("m"))
                .unwrap_err()
                .to_string()
                .contains("duplicate")
        );

        let mut m = sample();
        m.quantities[0].components = Some(vec!["x".into()]);
        assert!(m.validate(Path::new("m")).is_err());

        let mut m = sample();
        m.quantities[0].ordering.push(OrderingKey {
            axis: "quad".into(),
            kind: OrderingKind::GlobalId,
            key_quantity: None,
        });
        assert!(m.validate(Path::new("m")).is_err());

        let mut m = sample();
        m.storage.mode = StorageMode::External;
        assert!(m.validate(Path::new("m")).is_err());
        m.storage.generate = Some("cmd".into());
        m.validate(Path::new("m")).unwrap();

        let mut m = sample();
        m.steps = Some(Steps {
            recorded: vec![0, 2, 1],
            dt: None,
            time_quantity: None,
        });
        assert!(m.validate(Path::new("m")).is_err());
    }

    #[test]
    fn step_axis_must_match_recorded_steps() {
        let mut m = sample();
        m.quantities[0].axes = vec!["step".into(), "node".into()];
        m.quantities[0].shape = vec![2, 3];
        m.steps = Some(Steps {
            recorded: vec![0, 1, 2],
            dt: Some(0.1),
            time_quantity: None,
        });
        assert!(m.validate(Path::new("m")).is_err());
        m.steps.as_mut().unwrap().recorded = vec![0, 5];
        m.validate(Path::new("m")).unwrap();
    }

    #[test]
    fn id_and_path_rules() {
        assert!(valid_case_id("mesh_io/_triangle_3"));
        assert!(!valid_case_id("single"));
        assert!(!valid_case_id("a//b"));
        assert!(!valid_case_id("a/b c"));
        assert!(valid_relative_path("data/x.npy"));
        assert!(!valid_relative_path("/abs/x.npy"));
        assert!(!valid_relative_path("data/../x.npy"));
        assert!(is_hex_sha(SHA64, 64));
        assert!(!is_hex_sha(&SHA64.to_uppercase(), 64));
    }
}
