// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu's JOSS scalability test (EPFL/LSMS):
// performance-testing-cohesive @ 483bace8, cohesive_extrinsic.cc (user
// section parameters), with the optional parameters of the akantu-rs
// harness benchmarks/cohesive-perf/src/cohesive_extrinsic.cc.

//! Run configuration of the cohesive scalability driver.
//!
//! Upstream parameters come from the `user section` of the input file
//! (same names, same defaults as the C++ driver and its harness copy).
//! The port's own options are optional `rust_*` parameters of the same
//! section (ignored by the C++ driver, so one input file serves both) and
//! can be overridden on the command line.

use std::path::{Path, PathBuf};

use akantu_core::enums::SparseSolverType;
use akantu_core::{Error, Real, Result};
use akantu_parser::{Parser, ParserSection, SearchScope};

use crate::diag::{DiagConfig, parse_steps};
use crate::load_check::LoadCheckConfig;

/// Which node is blocked in `y` (harness parameter `blocked_node`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BlockedNode {
    /// Upstream: `blocked_dofs(3, _y)` on the **local** node 3 of every
    /// rank (the problem depends on the partition).
    Local,
    /// The node of global id 3 (node 3 of the mesh file) on every rank
    /// holding it: the 1-rank problem at every rank count.
    Global,
}

impl BlockedNode {
    /// Input-file spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Local => "local",
            Self::Global => "global",
        }
    }
}

/// Model used for the run (`rust_model`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ModelChoice {
    /// The cohesive model (also on a distributed mesh); without
    /// insertion, the solid model, with a message, if the cohesive model
    /// returns `Error::Unsupported` for the run.
    Auto,
    /// `SolidMechanicsModelCohesive`, extrinsic (upstream).
    Cohesive,
    /// `SolidMechanicsModel` with the bulk material only (elastic-only
    /// runs; same physics as the cohesive model without insertion).
    Solid,
}

/// Mesh partitioner for more than one rank (`rust_partitioner`).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum PartitionChoice {
    /// Scotch with unit weights seeded with `AKA_SEED` (upstream
    /// `mesh.distribute()`; needs the `scotch` feature).
    Scotch,
    /// Pure-Rust recursive coordinate bisection (not upstream's partition).
    Rcb,
    /// A frozen partition: `partition.csv` written by the C++ harness
    /// (`export_partition = true`), Scotch parts of every element.
    File(PathBuf),
}

/// Cohesive-law upstream-compatibility mode (`rust_cohesive_compat`,
/// D-003).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CompatMode {
    /// Every upstream behaviour (`CohesiveUpstreamCompat::upstream()` and the
    /// UD-352 position-cache switch of the solid model): reproduces the C++
    /// benchmark (UD-217 3-D tangents in particular).
    Upstream,
    /// The port's corrected defaults.
    Corrected,
}

impl CompatMode {
    /// Input-file spelling.
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Upstream => "upstream",
            Self::Corrected => "corrected",
        }
    }
}

/// Placement of the accumulated Neumann load of a doubled node
/// (`rust_doubled_node_load`, D-105 / UD-1280).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DoubledNodeLoad {
    /// As the cohesive compatibility mode: upstream's placement with
    /// `rust_cohesive_compat = upstream` (the oracle replays), the corrected
    /// one with `corrected`.
    Compat,
    /// Upstream's placement (the original copy keeps the load; switch
    /// `CohesiveUpstreamCompat::doubled_node_keeps_external_force`).
    Upstream,
    /// The corrected placement: the load follows the boundary elements of
    /// each copy (with upstream's laws in `rust_cohesive_compat = upstream`:
    /// the rank-count studies of D-105).
    FollowsElements,
}

impl DoubledNodeLoad {
    /// Whether the run keeps upstream's placement under `compat`.
    #[must_use]
    pub fn keeps_external_force(self, compat: CompatMode) -> bool {
        match self {
            Self::Compat => compat == CompatMode::Upstream,
            Self::Upstream => true,
            Self::FollowsElements => false,
        }
    }

    /// The effective placement's name (`summary.json`).
    #[must_use]
    pub fn effective(self, compat: CompatMode) -> &'static str {
        if self.keeps_external_force(compat) {
            "upstream"
        } else {
            "follows_elements"
        }
    }
}

/// Parse a `rust_doubled_node_load` value.
///
/// # Errors
/// Unknown value.
pub fn parse_doubled_node_load(v: &str) -> Result<DoubledNodeLoad> {
    match v {
        "compat" => Ok(DoubledNodeLoad::Compat),
        "upstream" => Ok(DoubledNodeLoad::Upstream),
        "follows_elements" => Ok(DoubledNodeLoad::FollowsElements),
        _ => Err(Error::invalid(format!(
            "rust_doubled_node_load must be compat, upstream or follows_elements, not \"{v}\""
        ))),
    }
}

/// Everything a run needs.
#[derive(Clone, Debug)]
pub struct Config {
    /// The parsed input file.
    pub parser: Parser,
    /// Directory against which the relative mesh path is resolved (the
    /// C++ driver: the working directory).
    pub base_dir: PathBuf,
    /// Directory receiving the output files (C++: the working directory).
    pub output_dir: PathBuf,
    /// `compression` (Pa).
    pub compression: Real,
    /// `shear` (Pa).
    pub shear: Real,
    /// `inc_shear` (Pa, added every 100 steps).
    pub inc_shear: Real,
    /// `output_energy` (default true).
    pub output_energy: bool,
    /// `output_paraview` (default true; not supported by the port yet).
    pub output_paraview: bool,
    /// `cohesive_insertion` (default true).
    pub cohesive_insertion: bool,
    /// `max_steps`.
    pub max_steps: usize,
    /// `mesh` (path as written).
    pub mesh: String,
    /// `export_partition` (default false).
    pub export_partition: bool,
    /// `energy_precision` (default 6; a negative value means 6, as for
    /// `std::setprecision`).
    pub energy_precision: usize,
    /// `blocked_node` (default local).
    pub blocked_node: BlockedNode,
    /// `rust_sparse_solver`: overrides `sparse_solver_type` of the static
    /// time-step solver (default: the input file's, i.e. `auto`).
    pub sparse_solver: Option<SparseSolverType>,
    /// `rust_partitioner` (default: Scotch with the `scotch` feature, else
    /// RCB).
    pub partitioner: PartitionChoice,
    /// `rust_model` (default auto).
    pub model: ModelChoice,
    /// `rust_cohesive_compat` (default upstream).
    pub compat: CompatMode,
    /// `rust_doubled_node_load` (default `compat`).
    pub doubled_node_load: DoubledNodeLoad,
    /// Seed of the Scotch partitioner (`AKA_SEED`, default 1 as the
    /// harness sets it).
    pub seed: i64,
    /// `rust_static_solve` (default true). `false` skips the static
    /// preload: a benchmark-only mode for timing the explicit loop on meshes
    /// whose static solve the port's sparse solvers cannot do in reasonable
    /// time (the physics then differ from upstream; the summary records it).
    pub static_solve: bool,
    /// Print upstream's progress messages on rank 0.
    pub verbose: bool,
    /// Diagnostics (`rust_diag_*`, all off by default;
    /// [`crate::diag`]).
    pub diag: DiagConfig,
    /// The per-copy check of the loads (`rust_load_check_every`,
    /// `rust_load_check_steps`; off by default, [`crate::load_check`]).
    pub load_check: LoadCheckConfig,
}

fn user(s: &ParserSection, name: &str) -> Result<Option<String>> {
    match s.get_parameter_or::<String>(name, String::new(), SearchScope::Current)? {
        v if v.is_empty() && s.parameter(name).is_none() => Ok(None),
        v => Ok(Some(v)),
    }
}

fn required<T: akantu_parser::FromParameter>(s: &ParserSection, name: &str) -> Result<T> {
    s.get_parameter_value::<T>(name, SearchScope::Current)
}

/// `std::setprecision(p)` of the harness's energy output: the stream
/// formats like C `printf("%.*g", p, x)`, where a negative precision is
/// taken as if it were omitted, i.e. 6 (C11 7.21.6.1).
fn precision_or_default(p: i64) -> usize {
    usize::try_from(p).unwrap_or(6)
}

/// Default partitioner of this build.
#[must_use]
pub fn default_partitioner() -> PartitionChoice {
    if cfg!(feature = "scotch") {
        PartitionChoice::Scotch
    } else {
        PartitionChoice::Rcb
    }
}

/// Parse a `rust_partitioner` value (`scotch`, `rcb`, `file:<path>`).
///
/// # Errors
/// Unknown value.
pub fn parse_partitioner(v: &str, base: &Path) -> Result<PartitionChoice> {
    match v {
        "scotch" => Ok(PartitionChoice::Scotch),
        "rcb" => Ok(PartitionChoice::Rcb),
        _ => match v.strip_prefix("file:") {
            Some(p) => Ok(PartitionChoice::File(base.join(p))),
            None => Err(Error::invalid(format!(
                "rust_partitioner must be scotch, rcb or file:<partition.csv>, not \"{v}\""
            ))),
        },
    }
}

/// Parse a `rust_model` value.
///
/// # Errors
/// Unknown value.
pub fn parse_model(v: &str) -> Result<ModelChoice> {
    match v {
        "auto" => Ok(ModelChoice::Auto),
        "cohesive" => Ok(ModelChoice::Cohesive),
        "solid" => Ok(ModelChoice::Solid),
        _ => Err(Error::invalid(format!(
            "rust_model must be auto, cohesive or solid, not \"{v}\""
        ))),
    }
}

/// Parse a `rust_cohesive_compat` value.
///
/// # Errors
/// Unknown value.
pub fn parse_compat(v: &str) -> Result<CompatMode> {
    match v {
        "upstream" => Ok(CompatMode::Upstream),
        "corrected" => Ok(CompatMode::Corrected),
        _ => Err(Error::invalid(format!(
            "rust_cohesive_compat must be upstream or corrected, not \"{v}\""
        ))),
    }
}

impl Config {
    /// Read the input file `input` (relative paths resolved against
    /// `base_dir`, outputs written to `output_dir`).
    ///
    /// # Errors
    /// Parse errors, missing required parameters, invalid values.
    pub fn from_file(input: &Path, base_dir: &Path, output_dir: &Path) -> Result<Self> {
        let mut parser = Parser::default();
        parser.parse(input)?;
        Self::from_parser(parser, base_dir, output_dir)
    }

    /// [`Config::from_file`] on an already parsed input.
    ///
    /// # Errors
    /// Missing required parameters, invalid values.
    pub fn from_parser(parser: Parser, base_dir: &Path, output_dir: &Path) -> Result<Self> {
        let s = parser.user_section()?.clone();
        let blocked_node = match s
            .get_parameter_or::<String>("blocked_node", "local".into(), SearchScope::Current)?
            .as_str()
        {
            "local" => BlockedNode::Local,
            "global" => BlockedNode::Global,
            _ => {
                return Err(Error::invalid(
                    "blocked_node must be \"local\" or \"global\"",
                ));
            }
        };
        let max_steps: u64 = required(&s, "max_steps")?;
        let energy_precision: i64 =
            s.get_parameter_or("energy_precision", 6_i64, SearchScope::Current)?;
        let sparse_solver = match user(&s, "rust_sparse_solver")? {
            Some(v) => Some(v.parse::<SparseSolverType>()?),
            None => None,
        };
        let partitioner = match user(&s, "rust_partitioner")? {
            Some(v) => parse_partitioner(&v, base_dir)?,
            None => default_partitioner(),
        };
        let model = match user(&s, "rust_model")? {
            Some(v) => parse_model(&v)?,
            None => ModelChoice::Auto,
        };
        let compat = match user(&s, "rust_cohesive_compat")? {
            Some(v) => parse_compat(&v)?,
            None => CompatMode::Upstream,
        };
        let diag_path = |name: &str| -> Result<Option<PathBuf>> {
            Ok(user(&s, name)?.map(|v| base_dir.join(v)))
        };
        let diag_from: i64 = s.get_parameter_or("rust_diag_from", 1_i64, SearchScope::Current)?;
        let diag = DiagConfig {
            static_out: diag_path("rust_diag_static_out")?,
            static_in: diag_path("rust_diag_static_in")?,
            dir: diag_path("rust_diag_dir")?,
            log_from: usize::try_from(diag_from)
                .map_err(|_| Error::invalid("rust_diag_from must be a step (≥ 0)"))?,
            node_steps: match user(&s, "rust_diag_node_steps")? {
                Some(v) => parse_steps(&v)?,
                None => Vec::new(),
            },
            near: s.get_parameter_or("rust_diag_near", 1e-6, SearchScope::Current)?,
            reapply_neumann: s.get_parameter_or(
                "rust_diag_reapply_neumann",
                false,
                SearchScope::Current,
            )?,
        };
        if !diag.node_steps.is_empty() && diag.dir.is_none() {
            return Err(Error::invalid(
                "rust_diag_node_steps needs rust_diag_dir (the dump directory)",
            ));
        }
        let load_check = LoadCheckConfig::parse(
            user(&s, "rust_load_check_every")?,
            user(&s, "rust_load_check_steps")?,
        )?;
        let doubled_node_load = match user(&s, "rust_doubled_node_load")? {
            Some(v) => parse_doubled_node_load(&v)?,
            None => DoubledNodeLoad::Compat,
        };
        let seed = std::env::var("AKA_SEED")
            .ok()
            .and_then(|v| v.trim().parse::<i64>().ok())
            .unwrap_or(1);
        Ok(Self {
            compression: required(&s, "compression")?,
            shear: required(&s, "shear")?,
            inc_shear: required(&s, "inc_shear")?,
            output_energy: s.get_parameter_or("output_energy", true, SearchScope::Current)?,
            output_paraview: s.get_parameter_or("output_paraview", true, SearchScope::Current)?,
            cohesive_insertion: s.get_parameter_or(
                "cohesive_insertion",
                true,
                SearchScope::Current,
            )?,
            max_steps: usize::try_from(max_steps)
                .map_err(|_| Error::invalid("max_steps out of range"))?,
            mesh: required(&s, "mesh")?,
            export_partition: s.get_parameter_or(
                "export_partition",
                false,
                SearchScope::Current,
            )?,
            energy_precision: precision_or_default(energy_precision),
            blocked_node,
            sparse_solver,
            partitioner,
            model,
            compat,
            doubled_node_load,
            seed,
            static_solve: s.get_parameter_or("rust_static_solve", true, SearchScope::Current)?,
            verbose: true,
            diag,
            load_check,
            parser,
            base_dir: base_dir.to_path_buf(),
            output_dir: output_dir.to_path_buf(),
        })
    }

    /// The mesh path resolved against [`Config::base_dir`].
    #[must_use]
    pub fn mesh_path(&self) -> PathBuf {
        self.base_dir.join(&self.mesh)
    }
}

#[cfg(test)]
mod tests {
    use super::precision_or_default;

    #[test]
    fn negative_energy_precision_means_six_digits() {
        assert_eq!(precision_or_default(-1), 6);
        assert_eq!(precision_or_default(-17), 6);
        assert_eq!(precision_or_default(0), 0);
        assert_eq!(precision_or_default(17), 17);
    }
}
