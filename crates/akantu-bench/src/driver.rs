// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu's JOSS scalability test (EPFL/LSMS):
// performance-testing-cohesive @ 483bace8, cohesive_extrinsic.cc (`main`),
// with the instrumentation of the akantu-rs harness copy
// benchmarks/cohesive-perf/src/cohesive_extrinsic.cc.

//! The cohesive scalability driver (upstream `main`).
//!
//! Phases and their Chrono names, as upstream: `read_mesh` (rank 0),
//! `dist_mesh`, `init_full` (model construction — facet mesh included — and
//! `initFull(_static, extrinsic)`), `initial_conditons` (sic; boundary
//! conditions), `static_solve`, `before_step` (`initNewSolver(
//! _explicit_lumped_mass)`, energies of step 0, stable time step), then per
//! step `boundary_conditions` (every 100 steps), `check_cohesive_stress`,
//! `solve_step`, `energies`.
//!
//! Benchmark-only mode `rust_static_solve = false` (not the upstream
//! problem): the phase is recorded as `static_solve_skipped` instead of
//! `static_solve`, `energies.csv` and `insertions.csv` start with the
//! [`NO_STATIC_SOLVE_MARKER`] comment line and both summaries carry
//! `"rust_static_solve": false`, so that no consumer of the upstream
//! outputs can take such a run for an upstream run.

use std::path::Path;
use std::time::Instant;

use akantu_cohesive::{CohesiveUpstreamCompat, SolidMechanicsModelCohesive};
use akantu_comm::{Communicator, ReduceOp};
use akantu_core::enums::{AnalysisMethod, MeshIoType, ModelType, SpatialDirection};
use akantu_core::tensor::MatrixX;
use akantu_core::{ElementKind, ElementType, Error, GhostType, Idx, Real, Result, TypeFilter};
use akantu_mesh::distribution::{
    MeshPartitioner, PrescribedPartition, RcbPartitioner, distribute_with,
};
use akantu_mesh::partition::MeshPartition;
use akantu_mesh::{Mesh, SharedMesh};
use akantu_model::ModelOptions;
use akantu_model::boundary_condition::{FixedValue, FromHigherDim};
use akantu_parser::{Parser, ParserParameter, ParserSection, ParserType};
use akantu_solid::SolidMechanicsModel;

use crate::chrono::Chrono;
use crate::config::{BlockedNode, CompatMode, Config, ModelChoice, PartitionChoice};
use crate::diag::{self, DecisionLog};
use crate::load_check::{LoadApplication, LoadCheck};
use crate::memory::{self, MemoryRecord};
use crate::output::{
    BlockedNodeInfo, EnergyRow, MatrixStats, MemorySummary, RankSummary, Summary, energies_csv,
    insertions_csv, write_file,
};

const DIM: usize = 3;

/// First line of `energies.csv` and `insertions.csv` of a
/// `rust_static_solve = false` run (no such line in upstream outputs).
pub const NO_STATIC_SOLVE_MARKER: &str = "# rust_static_solve = false: static preload skipped \
     (akantu-rs benchmark-only timing mode; not the upstream problem)";

/// Result of a run (rank 0 holds the global values).
#[derive(Clone, Debug)]
pub struct RunReport {
    /// Phase timers of this rank.
    pub chrono: Chrono,
    /// Energies of every step (`output_energy`; rank 0).
    pub energies: Vec<EnergyRow>,
    /// (step, global number inserted) of the steps with insertions.
    pub insertions: Vec<(usize, usize)>,
    /// Physical summary (global values).
    pub summary: Summary,
    /// This rank's summary.
    pub rank_summary: RankSummary,
}

/// The model the driver runs.
enum Driven {
    Cohesive(Box<SolidMechanicsModelCohesive>),
    Solid(Box<SolidMechanicsModel>),
}

impl Driven {
    fn smm(&mut self) -> &mut SolidMechanicsModel {
        match self {
            Self::Cohesive(m) => &mut m.smm,
            Self::Solid(m) => m,
        }
    }

    fn smm_ref(&self) -> &SolidMechanicsModel {
        match self {
            Self::Cohesive(m) => &m.smm,
            Self::Solid(m) => m,
        }
    }

    fn name(&self) -> &'static str {
        match self {
            Self::Cohesive(_) => "cohesive",
            Self::Solid(_) => "solid",
        }
    }

    fn solve_step(&mut self, id: &str) -> Result<()> {
        match self {
            Self::Cohesive(m) => m.solve_step(Some(id)),
            Self::Solid(m) => m.solve_step(Some(id)),
        }
    }

    fn energy(&mut self, id: &str) -> Result<Real> {
        match self {
            Self::Cohesive(m) => m.energy(id),
            Self::Solid(m) => m.energy(id),
        }
    }

    fn stable_time_step(&mut self) -> Result<Real> {
        match self {
            Self::Cohesive(m) => m.stable_time_step(),
            Self::Solid(m) => m.stable_time_step(),
        }
    }

    /// The cohesive model (diagnostics only).
    fn cohesive_mut(&mut self) -> Result<&mut SolidMechanicsModelCohesive> {
        match self {
            Self::Cohesive(m) => Ok(m),
            Self::Solid(_) => Err(Error::Unsupported(
                "the rust_diag_* diagnostics need the cohesive model".into(),
            )),
        }
    }

    /// The model's `check_cohesive_stress` (a local count, see
    /// [`global_cohesive_count`] for how the driver obtains upstream's
    /// global count).
    fn check_cohesive_stress(&mut self) -> Result<usize> {
        match self {
            Self::Cohesive(m) => m.check_cohesive_stress(),
            Self::Solid(_) => Err(Error::Unsupported(
                "cohesive_insertion = true needs the cohesive model (rust_model = solid \
                 runs the elastic-only variant)"
                    .into(),
            )),
        }
    }
}

/// `Matrix<Real>{{a, b, c}, {d, e, f}, {g, h, i}}` (row lists, stored
/// column-major).
fn matrix3(rows: [[Real; 3]; 3]) -> Result<MatrixX> {
    let mut data = Vec::with_capacity(9);
    for c in 0..3 {
        for r in rows {
            data.push(r[c]);
        }
    }
    MatrixX::from_col_major(3, 3, data)
}

/// Read `partition.csv` (C++ harness, `export_partition = true`): the parts
/// of the elements of dimension 3, linearised in type order.
fn read_partition_csv(path: &Path) -> Result<Vec<Idx>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    let mut by_type: std::collections::BTreeMap<ElementType, Vec<(usize, Idx)>> =
        std::collections::BTreeMap::new();
    for (i, line) in text.lines().enumerate().skip(1) {
        let f: Vec<&str> = line.split(',').map(str::trim).collect();
        if f.len() != 3 {
            return Err(Error::invalid(format!(
                "{}:{}: expected `type, element, partition`",
                path.display(),
                i + 1
            )));
        }
        let ty: ElementType = f[0].parse()?;
        if ty.spatial_dimension() != DIM {
            continue;
        }
        let parse = |s: &str| {
            s.parse::<i64>()
                .map_err(|e| Error::invalid(format!("{}:{}: {e}", path.display(), i + 1)))
        };
        let el = usize::try_from(parse(f[1])?).map_err(|_| {
            Error::invalid(format!("{}:{}: negative element", path.display(), i + 1))
        })?;
        let part = Idx::try_from(parse(f[2])?).map_err(|_| {
            Error::invalid(format!("{}:{}: part out of range", path.display(), i + 1))
        })?;
        by_type.entry(ty).or_default().push((el, part));
    }
    let mut parts = Vec::new();
    for (ty, mut v) in by_type {
        v.sort_unstable();
        for (k, (el, p)) in v.into_iter().enumerate() {
            if el != k {
                return Err(Error::invalid(format!(
                    "{}: {ty} element {k} missing from the partition",
                    path.display()
                )));
            }
            parts.push(p);
        }
    }
    Ok(parts)
}

/// Write `partition.csv` as the C++ harness does: every not-ghost element
/// type of the partition (all dimensions, type order), its element index
/// and part.
fn write_partition_csv(dir: &Path, p: &MeshPartition) -> Result<()> {
    use std::fmt::Write as _;
    let mut s = String::from("type, element, partition\n");
    for (ty, ghost, parts) in p.partitions.as_map().iter_all() {
        if ghost != GhostType::NotGhost {
            continue;
        }
        for (el, part) in parts.as_slice().iter().enumerate() {
            let _ = writeln!(s, "{ty}, {el}, {part}");
        }
    }
    write_file(dir, "partition.csv", &s)
}

/// A partitioner that keeps a copy of the partition it computed (for
/// `partition.csv`).
struct Recording<'a> {
    inner: &'a mut dyn MeshPartitioner,
    keep: bool,
    out: Option<MeshPartition>,
}

impl MeshPartitioner for Recording<'_> {
    fn partition(&mut self, mesh: &Mesh, nb_parts: usize) -> Result<MeshPartition> {
        let p = self.inner.partition(mesh, nb_parts)?;
        if self.keep {
            self.out = Some(p.clone());
        }
        Ok(p)
    }
}

/// Distribute the mesh (upstream `mesh.distribute()`, or the harness's
/// explicit partition). Collective; a partitioning failure on the root is
/// agreed by the distribution. Returns the partition on the root when
/// `keep` (P > 1).
fn distribute_mesh(mesh: &mut Mesh, cfg: &Config, keep: bool) -> Result<Option<MeshPartition>> {
    let comm = mesh.communicator().clone();
    if comm.size() == 1 {
        // Upstream `mesh.distribute()` on one rank only creates empty
        // synchronizers and flags the mesh as distributed. The port keeps
        // the mesh serial: a distributed flag makes every mesh mutation
        // (cohesive insertion) run the distributed hooks (collectives),
        // and a one-rank distribution has nothing to
        // synchronise.
        return Ok(None);
    }
    let root = comm.rank() == 0;
    let mut inner: Box<dyn MeshPartitioner> = match &cfg.partitioner {
        PartitionChoice::File(path) => {
            // read on the root only; a read error is agreed below
            let parts = if root {
                read_partition_csv(path)
            } else {
                Ok(Vec::new())
            };
            match parts {
                Ok(parts) => Box::new(PrescribedPartition::new(parts)),
                Err(e) => Box::new(Failing(Some(e))),
            }
        }
        PartitionChoice::Rcb => Box::new(RcbPartitioner),
        PartitionChoice::Scotch => scotch_partitioner(cfg.seed),
    };
    let mut rec = Recording {
        inner: inner.as_mut(),
        keep,
        out: None,
    };
    // no consumer yet: the events have no recipient
    let _events = distribute_with(
        mesh,
        if root {
            Some(&mut rec as &mut dyn MeshPartitioner)
        } else {
            None
        },
        0,
    )?;
    Ok(rec.out)
}

/// A partitioner that reports a preparation error (agreed by the
/// distribution instead of leaving the other ranks waiting).
struct Failing(Option<Error>);

impl MeshPartitioner for Failing {
    fn partition(&mut self, _mesh: &Mesh, _nb_parts: usize) -> Result<MeshPartition> {
        Err(self
            .0
            .take()
            .unwrap_or_else(|| Error::invalid("partitioner failed")))
    }
}

#[cfg(feature = "scotch")]
fn scotch_partitioner(seed: i64) -> Box<dyn MeshPartitioner> {
    Box::new(akantu_mesh::scotch::ScotchPartitioner::new(seed))
}

#[cfg(not(feature = "scotch"))]
fn scotch_partitioner(_seed: i64) -> Box<dyn MeshPartitioner> {
    Box::new(Failing(Some(Error::Unsupported(
        "rust_partitioner = scotch needs the `scotch` feature (Docker image docker/scotch); \
         use rcb or file:<partition.csv>"
            .into(),
    ))))
}

/// The input without the cohesive laws and the inserter section (the solid
/// model of elastic-only distributed runs reads the same model section).
fn solid_parser(parser: &Parser) -> Parser {
    let mut p = parser.clone();
    for model in &mut p.root.sub_sections {
        if model.section_type != ParserType::Model {
            continue;
        }
        model.sub_sections.retain(|s| {
            !(s.section_type == ParserType::CohesiveInserter
                || (s.section_type == ParserType::Material && s.name.starts_with("cohesive")))
        });
    }
    p
}

/// Set the parameter `name` (`sparse_solver_type`, `sparse_solver_ordering`)
/// of every `time_step_solver` section of the model section (upstream input
/// syntax) to `value`.
fn override_solver_parameter(parser: &mut Parser, name: &str, value: &str) -> Result<()> {
    fn visit(s: &mut ParserSection, name: &str, value: &str) -> Result<()> {
        for sub in &mut s.sub_sections {
            if sub.section_type == ParserType::TimeStepSolver {
                if let Some(p) = sub
                    .parameters
                    .iter_mut()
                    .find(|p| p.name.as_bytes() == name.as_bytes())
                {
                    p.value = value.into();
                } else {
                    sub.add_parameter(ParserParameter::new(name, value))?;
                }
            } else {
                visit(sub, name, value)?;
            }
        }
        Ok(())
    }
    visit(&mut parser.root, name, value)
}

fn count(mesh: &Mesh, ghost: GhostType, kind: Option<ElementKind>) -> usize {
    mesh.element_types(TypeFilter {
        spatial_dimension: Some(DIM),
        ghost_type: ghost,
        kind,
    })
    .map(|t| mesh.nb_element(t, ghost))
    .sum()
}

fn all_reduce_usize(comm: &Communicator, v: usize, op: ReduceOp) -> Result<usize> {
    let mut x = [u64::try_from(v).map_err(|_| Error::invalid("count overflow"))?];
    comm.all_reduce(&mut x, op)?;
    usize::try_from(x[0]).map_err(|_| Error::invalid("count overflow"))
}

fn all_reduce_u64(comm: &Communicator, v: u64, op: ReduceOp) -> Result<u64> {
    let mut x = [v];
    comm.all_reduce(&mut x, op)?;
    Ok(x[0])
}

/// Maximum and sum over the ranks of every memory field measured on all
/// ranks (collective; the same number of reductions on every rank).
fn memory_summary(comm: &Communicator, m: &MemoryRecord) -> Result<MemorySummary> {
    let peak_reset = all_reduce_u64(comm, u64::from(m.peak_reset), ReduceOp::Min)? == 1;
    let mut fields = Vec::new();
    for (key, v) in m.fields() {
        if all_reduce_u64(comm, u64::from(v.is_some()), ReduceOp::Min)? == 1 {
            let v = v.unwrap_or(0);
            let max = all_reduce_u64(comm, v, ReduceOp::Max)?;
            let sum = all_reduce_u64(comm, v, ReduceOp::Sum)?;
            fields.push((key, max, sum));
        }
    }
    Ok(MemorySummary { peak_reset, fields })
}

/// Global number of cohesive elements (each is not-ghost on exactly one
/// rank). Collective when `comm` has more than one rank.
///
/// Upstream `checkCohesiveStress()` returns the **global** number of new
/// elements (`CohesiveElementInserter::insertElements` →
/// `MeshAccessor::updateGlobalData`, cohesive_element_inserter.cc:264-267),
/// while the port's `SolidMechanicsModelCohesive::check_cohesive_stress`
/// returns the number inserted **on this rank**. The driver therefore takes
/// the number inserted at a step from the mesh (difference of this global
/// count), which holds whichever contract the model follows; on one rank it
/// also checks the model's return value against it.
fn global_cohesive_count(shared: &SharedMesh, comm: &Communicator) -> Result<usize> {
    let local = {
        let m = shared.read()?;
        count(&m, GhostType::NotGhost, Some(ElementKind::Cohesive))
    };
    if comm.size() > 1 {
        all_reduce_usize(comm, local, ReduceOp::Sum)
    } else {
        Ok(local)
    }
}

/// Apply the shear increments / initial tractions `m` on the four loaded
/// surfaces.
fn apply_neumann(
    model: &mut SolidMechanicsModel,
    m: &FromHigherDim,
    groups: &[&str],
) -> Result<()> {
    for g in groups {
        model.apply_neumann_bc(m, g)?;
    }
    Ok(())
}

/// The driver's load history up to a step: compression on "top" /
/// "bottom", the initial shear and `increments` shear increments on the
/// four sheared groups, in the order the driver applies them (the
/// re-application of [`crate::load_check`]).
fn load_history<'a>(
    compression: &'a FromHigherDim,
    shear: &'a FromHigherDim,
    increment: &'a FromHigherDim,
    increments: usize,
) -> Vec<LoadApplication<'a>> {
    const TOP_BOTTOM: [&str; 2] = ["top", "bottom"];
    let mut v: Vec<LoadApplication<'a>> = vec![(compression, &TOP_BOTTOM), (shear, &SHEARED)];
    v.extend(std::iter::repeat_n((increment, &SHEARED[..]), increments));
    v
}

/// The four groups carrying the shear (upstream's list).
const SHEARED: [&str; 4] = ["top", "bottom", "side left", "side right"];

/// The number of shear increments applied up to and including `step` (one
/// at the start of every step that is a multiple of 100, below 10,000).
fn increments_at(step: usize) -> usize {
    step.min(9999) / 100
}

/// Run the benchmark on `comm` (collective). Writes the output files on
/// rank 0 (and `summary_rank<r>.json`, `chrono_rank<r>.csv` and, for the
/// cohesive model, `insertion_stats_rank<r>.csv` on every rank) into
/// [`Config::output_dir`].
///
/// # Errors
/// Input, mesh, model, solver or I/O errors; `output_paraview = true` (not
/// supported by the port's driver yet).
#[allow(clippy::too_many_lines)] // one function mirrors upstream `main` phase by phase
pub fn run(comm: &Communicator, cfg: &Config) -> Result<RunReport> {
    let prank = usize::try_from(comm.rank()).map_err(|_| Error::invalid("negative rank"))?;
    let psize = comm.size();
    let verbose = cfg.verbose && prank == 0;
    let mut chrono = Chrono::new(prank, psize, cfg.verbose);
    if cfg.output_paraview {
        return Err(Error::Unsupported(
            "output_paraview = true: the ParaView dumps of the cohesive driver are not ported \
             yet (the JOSS inputs set output_paraview = false)"
                .into(),
        ));
    }
    if verbose {
        println!(
            "Paramters:\n - output_energy: {}\n - output_paraview: {}\n - cohesive_insertion: {}\n - max_steps: {}\n - mesh_filename: {}",
            u8::from(cfg.output_energy),
            u8::from(cfg.output_paraview),
            u8::from(cfg.cohesive_insertion),
            cfg.max_steps,
            cfg.mesh
        );
    }
    chrono.start();
    let start_time = Instant::now();

    // ---- mesh: read on rank 0, distribute -------------------------------
    let mut mesh = Mesh::with_communicator(DIM, "mesh", comm.clone())?;
    if prank == 0 {
        akantu_io::read_mesh(&mut mesh, &cfg.mesh_path(), MeshIoType::Gmsh)?;
        chrono.store_time("read_mesh");
    }
    let partition = distribute_mesh(&mut mesh, cfg, cfg.export_partition)?;
    if let Some(p) = &partition {
        write_partition_csv(&cfg.output_dir, p)?;
    }
    drop(partition);
    chrono.store_time("dist_mesh");

    // ---- model -----------------------------------------------------------
    let shared = SharedMesh::new(mesh);
    let mut parser = cfg.parser.clone();
    if let Some(ty) = cfg.sparse_solver {
        override_solver_parameter(&mut parser, "sparse_solver_type", &ty.to_string())?;
    }
    if let Some(o) = cfg.sparse_ordering {
        override_solver_parameter(&mut parser, "sparse_solver_ordering", o.name())?;
    }
    let mut model = build_model(&shared, cfg, &parser, verbose)?;
    chrono.store_time("init_full");

    // ---- boundary conditions --------------------------------------------
    let blocked_y_nodes = {
        let smm = model.smm();
        smm.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::Z,
            },
            "bottom",
        )?;
        smm.apply_dirichlet_bc(
            &FixedValue {
                value: 0.0,
                axis: SpatialDirection::X,
            },
            "right line",
        )?;
        let m = shared.read()?;
        let nodes: Vec<usize> = match cfg.blocked_node {
            BlockedNode::Local => {
                if m.nb_nodes() <= 3 {
                    return Err(Error::invalid(
                        "blocked_node = local: the rank holds fewer than 4 nodes (upstream \
                         blocked_dofs(3, _y) is out of range)",
                    ));
                }
                vec![3]
            }
            BlockedNode::Global => (0..m.nb_nodes())
                .filter(|&n| m.node_global_id(n as Idx) == 3)
                .collect(),
        };
        drop(m);
        let mut b_g = smm.blocked_dofs_mut()?;
        let b = b_g.as_mut_slice();
        for &n in &nodes {
            b[n * DIM + 1] = true;
        }
        nodes
    };
    let (c, s, inc) = (cfg.compression, cfg.shear, cfg.inc_shear);
    let compression = FromHigherDim::new(matrix3([[c, 0., 0.], [0., c, 0.], [0., 0., c]])?);
    let shear = FromHigherDim::new(matrix3([[0., 0., s], [0., 0., 0.], [s, 0., 0.]])?);
    let new_shear = FromHigherDim::new(matrix3([[0., 0., inc], [0., 0., 0.], [inc, 0., 0.]])?);
    {
        let smm = model.smm();
        smm.external_force_mut()?.as_mut_slice().fill(0.0);
        apply_neumann(smm, &compression, &["top", "bottom"])?;
        apply_neumann(smm, &shear, &SHEARED)?;
    }
    chrono.store_time("initial_conditons");

    // ---- static solve ----------------------------------------------------
    let mut mem = MemoryRecord {
        rss_before_static: memory::status_kib("VmRSS"),
        ..MemoryRecord::default()
    };
    let mut diag_notes: Vec<String> = Vec::new();
    if cfg.static_solve {
        model.solve_step("static")?;
        chrono.store_time("static_solve");
        // Diagnostics (input-gated, off by default): impose / export
        // the static solution
        if let Some(dir) = &cfg.diag.static_in {
            let n = diag::impose_static(dir, &shared, model.cohesive_mut()?)?;
            diag_notes.push(format!(
                "static solution imposed from {} ({n} nodes on rank {prank})",
                dir.display()
            ));
            if verbose {
                println!(
                    "akantu-rs: diagnostics: static solution imposed from {}",
                    dir.display()
                );
            }
        }
        if let Some(dir) = &cfg.diag.static_out {
            diag::write_static(dir, comm, &shared, model.cohesive_mut()?)?;
            diag_notes.push(format!("static solution written to {}", dir.display()));
        }
    } else if cfg.diag.static_in.is_some() || cfg.diag.static_out.is_some() {
        return Err(Error::invalid(
            "rust_diag_static_in / rust_diag_static_out need the static solve",
        ));
    } else {
        if verbose {
            println!("akantu-rs: rust_static_solve = false: static preload skipped (timing mode)");
        }
        // not `static_solve`: such a run must not look like an upstream run
        chrono.store_time("static_solve_skipped");
    }
    // Memory and solver figures of the static solve, outside the
    // Chrono phases' work (a few /proc reads); the peak is then reset so
    // that the loop's peak is measured apart
    mem.hwm_static = memory::status_kib("VmHWM");
    mem.rss_after_static = memory::status_kib("VmRSS");
    mem.peak_reset = memory::reset_peak();
    let (static_sparse_solver, static_solver_stats, static_nnz, static_order) = {
        let backend = model.smm().dof_manager().backend();
        match (cfg.static_solve, backend.aij("J")) {
            (true, Ok(a)) => (
                backend
                    .sparse_solver_kind("J")
                    .map_or_else(|| "none".to_string(), |k| k.to_string()),
                backend.sparse_solver_statistics("J"),
                Some(u64::try_from(a.nnz()).map_err(|_| Error::invalid("nnz overflow"))?),
                u64::try_from(a.size()).ok(),
            ),
            _ => ("none".to_string(), Vec::new(), None, None),
        }
    };
    if verbose && cfg.static_solve {
        println!("akantu-rs: static solve with the {static_sparse_solver} sparse solver");
    }
    // Port option (off by default, upstream keeps them): release the
    // static solver's factors and matrices, after the statistics above
    // were read; the explicit loop never uses them.
    if cfg.static_solve && cfg.release_static_solver {
        model.smm().release_static_solver()?;
        mem.rss_after_release = memory::status_kib("VmRSS");
        if verbose {
            println!("akantu-rs: static solver released (rust_release_static_solver = true)");
        }
    }

    model
        .smm()
        .init_new_solver(AnalysisMethod::ExplicitLumpedMass)?;

    let mut energies: Vec<EnergyRow> = Vec::new();
    let (mut ed, mut ep, mut ek, mut ew) = (0.0, 0.0, 0.0, 0.0);
    if cfg.output_energy {
        ep = model.energy("potential")?;
        ek = model.energy("kinetic")?;
        ew += model.energy("external work")?;
    }
    let mut et = ed + ep + ek - ew;
    if cfg.output_energy && prank == 0 {
        energies.push((0, [ed, ep, ek, ew, et]));
    }
    let time_step = model.stable_time_step()? * 0.05;
    model.smm().set_time_step(time_step, None)?;
    if verbose {
        println!("Time step: {}", crate::output::g(time_step, 6));
    }
    let init_time = start_time.elapsed();
    chrono.store_time("before_step");

    let nb_dofs_start = shared.read()?.nb_global_nodes() * DIM;
    let mut insertions: Vec<(usize, usize)> = Vec::new();
    let mut nb_cohesive_global = if cfg.cohesive_insertion {
        global_cohesive_count(&shared, comm)?
    } else {
        0
    };
    // Decision logs (input-gated, off by default)
    let mut decisions = match &cfg.diag.dir {
        Some(dir) => {
            model.cohesive_mut()?;
            Some(DecisionLog::new(&cfg.diag, dir, comm, &shared)?)
        }
        None => None,
    };
    let mut reapplied = 0_usize;
    // Input-gated, off by default: the per-copy check of the
    // loads, at step 0 (after the static solve) when asked
    let mut load_check = if cfg.load_check.enabled() {
        let mut lc = LoadCheck::new(
            &cfg.load_check,
            comm,
            &cfg.output_dir,
            nb_dofs_start / DIM,
            verbose,
        )?;
        if cfg.load_check.is_due(0) {
            let loads = load_history(&compression, &shear, &new_shear, 0);
            let m = shared.read()?;
            lc.run(0, 0, comm, &m, model.smm_ref(), &loads)?;
            drop(m);
            chrono.store_time("load_check");
        }
        Some(lc)
    } else {
        None
    };
    let loop_start = Instant::now();

    // ---- main loop -------------------------------------------------------
    for step in 1..=cfg.max_steps {
        if step % 100 == 0 && step < 10000 {
            apply_neumann(model.smm(), &new_shear, &SHEARED)?;
            chrono.store_time("boundary_conditions");
        }
        if cfg.cohesive_insertion {
            // Read only when the re-application is on
            let nodes_before = if cfg.diag.reapply_neumann {
                shared.read()?.nb_nodes()
            } else {
                0
            };
            let returned = match decisions.as_mut() {
                None => model.check_cohesive_stress()?,
                Some(log) => {
                    // `check_cohesive_stress` = prepare + finish (no runtime
                    // law here), with the facet margins read in between
                    log.before_check(&shared)?;
                    let m = model.cohesive_mut()?;
                    let check = m.check_cohesive_stress_prepare()?;
                    if !check.foreign_laws.is_empty() {
                        m.abort_insertion_check(&check)?;
                        return Err(Error::Unsupported(
                            "the rust_diag_dir logs do not support runtime cohesive laws".into(),
                        ));
                    }
                    log.facet_margins(step, &shared, m)?;
                    let n = m.check_cohesive_stress_finish(check)?;
                    log.new_nodes(step, &shared, m)?;
                    n
                }
            };
            // upstream returns the global count; see `global_cohesive_count`
            let total = global_cohesive_count(&shared, comm)?;
            let nb_new = total.checked_sub(nb_cohesive_global).ok_or_else(|| {
                Error::invalid("check_cohesive_stress: the number of cohesive elements decreased")
            })?;
            if psize == 1 && returned != nb_new {
                return Err(Error::invalid(format!(
                    "check_cohesive_stress returned {returned} new cohesive elements but the \
                     mesh holds {nb_new} more"
                )));
            }
            nb_cohesive_global = total;
            if nb_new > 0 {
                insertions.push((step, nb_new));
            }
            // Diagnostics (input-gated, off by default): rebuild the
            // external force on the current topology after a doubling
            if cfg.diag.reapply_neumann && shared.read()?.nb_nodes() > nodes_before {
                let increments = increments_at(step);
                let smm = model.smm();
                smm.external_force_mut()?.as_mut_slice().fill(0.0);
                apply_neumann(smm, &compression, &["top", "bottom"])?;
                apply_neumann(smm, &shear, &SHEARED)?;
                for _ in 0..increments {
                    apply_neumann(smm, &new_shear, &SHEARED)?;
                }
                reapplied += 1;
            }
            chrono.store_time("check_cohesive_stress");
        }
        // The external force this step's solve uses against a
        // re-application of the loads (reads the model only)
        if let Some(lc) = load_check.as_mut()
            && cfg.load_check.is_due(step)
        {
            let k = increments_at(step);
            let loads = load_history(&compression, &shear, &new_shear, k);
            let m = shared.read()?;
            lc.run(step, k, comm, &m, model.smm_ref(), &loads)?;
            drop(m);
            chrono.store_time("load_check");
        }
        model.solve_step("explicit_lumped")?;
        chrono.store_time("solve_step");
        if let Some(log) = decisions.as_mut() {
            log.after_solve(step, &shared, model.cohesive_mut()?)?;
        }
        if cfg.output_energy {
            ed = model.energy("dissipated")?;
            ep = model.energy("potential")?;
            ek = model.energy("kinetic")?;
            ew += model.energy("external work")?;
            et = ed + ep + ek - ew;
            if prank == 0 {
                energies.push((step, [ed, ep, ek, ew, et]));
            }
            chrono.store_time("energies");
        }
    }
    let loop_time = loop_start.elapsed();
    if let Some(log) = decisions.as_mut() {
        log.finish()?;
        diag_notes.push(format!(
            "decision logs in {} from step {}; facets whose re-evaluated criterion disagreed \
             with the law's flag on rank {prank}: {}",
            cfg.diag
                .dir
                .as_ref()
                .map_or_else(String::new, |d| d.display().to_string()),
            cfg.diag.log_from,
            log.mismatches
        ));
    }
    if cfg.diag.reapply_neumann {
        diag_notes.push(format!(
            "rust_diag_reapply_neumann: external force rebuilt after {reapplied} insertion steps \
             that added nodes on rank {prank}"
        ));
    }
    if let Some(lc) = &load_check
        && verbose
    {
        println!("{}", lc.final_line());
    }
    if !diag_notes.is_empty() {
        let mut text =
            String::from("# akantu-rs cohesive driver diagnostics of this run (rust_diag_*)\n");
        for n in &diag_notes {
            text.push_str(n);
            text.push('\n');
        }
        write_file(&cfg.output_dir, &format!("diag.r{prank}.txt"), &text)?;
    }
    mem.hwm_end = memory::status_kib("VmHWM");
    mem.rss_end = memory::status_kib("VmRSS");

    // ---- end of run: upstream prints, then the harness outputs ------------
    let (nb_dofs_end, nb_coh_local, nb_bulk_regular, nb_bulk, nb_bulk_ghost, nb_nodes) = {
        let m = shared.read()?;
        (
            m.nb_global_nodes() * DIM,
            count(&m, GhostType::NotGhost, Some(ElementKind::Cohesive)),
            count(&m, GhostType::NotGhost, Some(ElementKind::Regular)),
            count(&m, GhostType::NotGhost, None),
            count(&m, GhostType::Ghost, None),
            m.nb_nodes(),
        )
    };
    let nb_cohesive_elements = all_reduce_usize(comm, nb_coh_local, ReduceOp::Sum)?;
    let ed_end = model.energy("dissipated")?;
    if verbose {
        let full = (init_time + loop_time).as_secs_f64();
        println!(
            "Cohesive info: dissipated energy: {} - nb_cohesive_element: {nb_cohesive_elements}",
            crate::output::g(ed_end, 6)
        );
        println!("Nb proc: {psize}");
        println!("Full time: {}", crate::output::g(full, 6));
        println!(
            "Init time: {}",
            crate::output::g(init_time.as_secs_f64(), 6)
        );
        println!(
            "Step time: {} - nb_steps: {} - time_per_step: {}",
            crate::output::g(loop_time.as_secs_f64(), 6),
            cfg.max_steps,
            crate::output::g(loop_time.as_secs_f64() / cfg.max_steps.max(1) as f64, 6)
        );
        println!("Ns DOFs - start: {nb_dofs_start} - end: {nb_dofs_end}");
    }
    let marker = (!cfg.static_solve).then_some(NO_STATIC_SOLVE_MARKER);
    if prank == 0 {
        let csv = chrono.to_csv();
        if verbose {
            // the C++ harness prints the Chrono CSV on stderr
            eprintln!("{csv}");
        }
        write_file(&cfg.output_dir, "chrono.csv", &csv)?;
        if cfg.output_energy {
            write_file(
                &cfg.output_dir,
                "energies.csv",
                &energies_csv(&energies, cfg.energy_precision, marker),
            )?;
        }
    }

    // summary of the physical state (owned nodes)
    let (u2, u_max, u_sum, nb_owned_nodes, blocked_info) = {
        let m = shared.read()?;
        let u = model.smm().displacement()?.as_slice().to_vec();
        let mut u2 = 0.0;
        let mut u_max: Real = 0.0;
        let mut u_sum = [0.0; 3];
        let mut owned = 0_usize;
        for n in 0..m.nb_nodes() {
            if !m.is_local_or_master_node(n as Idx)? {
                continue;
            }
            owned += 1;
            for d in 0..DIM {
                let v = u[n * DIM + d];
                u2 += v * v;
                u_sum[d] += v;
                u_max = u_max.max(v.abs());
            }
        }
        let x_g = m.nodes();
        let x = x_g.as_slice();
        let info: Vec<BlockedNodeInfo> = blocked_y_nodes
            .iter()
            .map(|&n| BlockedNodeInfo {
                local: n,
                global: m.node_global_id(n as Idx),
                x: [x[n * DIM], x[n * DIM + 1], x[n * DIM + 2]],
            })
            .collect();
        (u2, u_max, u_sum, owned, info)
    };
    let mut r = [u2];
    comm.all_reduce(&mut r, ReduceOp::Sum)?;
    let u2 = r[0];
    let mut r = [u_max];
    comm.all_reduce(&mut r, ReduceOp::Max)?;
    let u_max = r[0];
    let mut us = u_sum;
    for v in &mut us {
        let mut r = [*v];
        comm.all_reduce(&mut r, ReduceOp::Sum)?;
        *v = r[0];
    }
    let nb_owned_global = all_reduce_usize(comm, nb_owned_nodes, ReduceOp::Sum)?;
    let ep_end = model.energy("potential")?;
    let ek_end = model.energy("kinetic")?;
    let nb_bulk_global = all_reduce_usize(comm, nb_bulk_regular, ReduceOp::Sum)?;
    let memory = memory_summary(comm, &mem)?;
    let static_matrix = match (static_nnz, static_order) {
        (Some(nnz), Some(order)) => Some(MatrixStats {
            order,
            nnz_sum: all_reduce_u64(comm, nnz, ReduceOp::Sum)?,
            nnz_max: all_reduce_u64(comm, nnz, ReduceOp::Max)?,
        }),
        _ => None,
    };

    let rank_summary = RankSummary {
        rank: prank,
        blocked_node_rule: cfg.blocked_node.as_str().into(),
        blocked_y_nodes: blocked_info,
        nb_nodes,
        nb_owned_nodes,
        nb_regular_elements_not_ghost: nb_bulk_regular,
        nb_elements_dim3_not_ghost: nb_bulk,
        nb_elements_dim3_ghost: nb_bulk_ghost,
        nb_cohesive_not_ghost: nb_coh_local,
        memory: mem,
        static_matrix_nnz: static_nnz,
        static_solve: cfg.static_solve,
    };
    write_file(
        &cfg.output_dir,
        &format!("summary_rank{prank}.json"),
        &rank_summary.to_json(),
    )?;
    // port-specific: this rank's phase times (the harness writes rank 0's
    // only) and, for the cohesive model, the split of its insertion check
    write_file(
        &cfg.output_dir,
        &format!("chrono_rank{prank}.csv"),
        &chrono.to_csv(),
    )?;
    if let Driven::Cohesive(m) = &model {
        let st = &m.ext.stats;
        write_file(
            &cfg.output_dir,
            &format!("insertion_stats_rank{prank}.csv"),
            &format!(
                "calls, interpolation, checks, insertion, inserted\n{}, {}, {}, {}, {}\n",
                st.calls,
                crate::output::g(st.interpolation, 6),
                crate::output::g(st.checks, 6),
                crate::output::g(st.insertion, 6),
                st.inserted
            ),
        )?;
    }
    let summary = Summary {
        psize,
        mesh: cfg.mesh.clone(),
        max_steps: cfg.max_steps,
        cohesive_insertion: cfg.cohesive_insertion,
        blocked_node: cfg.blocked_node.as_str().into(),
        fill_nodes_to_elements: "none".into(),
        time_step,
        nb_global_nodes_start: nb_dofs_start / DIM,
        nb_global_nodes_end: nb_dofs_end / DIM,
        nb_owned_nodes_sum: nb_owned_global,
        nb_regular_elements_dim3: nb_bulk_global,
        nb_cohesive_elements,
        nb_insertion_steps: insertions.len(),
        first_insertion_step: insertions.first().map_or(0, |i| i.0),
        energy_dissipated: ed_end,
        energy_potential: ep_end,
        energy_kinetic: ek_end,
        displacement_l2_norm: u2.sqrt(),
        displacement_max_abs: u_max,
        displacement_sum: us,
        loop_time_s: loop_time.as_secs_f64(),
        init_time_s: init_time.as_secs_f64(),
        model: model.name().into(),
        cohesive_compat: cfg.compat.as_str().into(),
        doubled_node_load: cfg.doubled_node_load.effective(cfg.compat).into(),
        static_solve: cfg.static_solve,
        sparse_solver: cfg
            .sparse_solver
            .map_or_else(|| "input".to_string(), |t| t.to_string()),
        static_sparse_solver,
        static_solver_stats,
        static_matrix,
        memory,
        load_check: load_check.map(|lc| lc.summary),
        exec: crate::output::ExecSummary::process_default(),
    };
    if prank == 0 {
        write_file(
            &cfg.output_dir,
            "insertions.csv",
            &insertions_csv(&insertions, marker),
        )?;
        write_file(&cfg.output_dir, "summary.json", &summary.to_json())?;
    }
    Ok(RunReport {
        chrono,
        energies,
        insertions,
        summary,
        rank_summary,
    })
}

/// `rust_cohesive_compat = upstream` also reproduces upstream's
/// current-position cache: `initSolver` resets it to `X` on every call
/// (UD-352, D-064, `SmmUpstreamCompat::init_solver_resets_current_position`).
/// The benchmark reads no position between its static solve and
/// `initNewSolver` (no cohesive element exists then), so both rules give
/// the same Δt and trajectory; the replays run with the switch set.
fn upstream_position_cache(smm: &mut SolidMechanicsModel) {
    smm.physics
        .upstream_compat
        .init_solver_resets_current_position = true;
}

/// Construct and initialise the model (`SolidMechanicsModelCohesive model
/// (mesh); model.initFull(_static, _is_extrinsic = true)`, or the solid
/// model for elastic-only runs).
fn build_model(
    shared: &SharedMesh,
    cfg: &Config,
    parser: &Parser,
    verbose: bool,
) -> Result<Driven> {
    let options = ModelOptions::new(AnalysisMethod::Static);
    let solid = |why: Option<&str>| -> Result<Driven> {
        if let Some(why) = why
            && verbose
        {
            println!(
                "akantu-rs: running the elastic-only variant with SolidMechanicsModel ({why})"
            );
        }
        let mut smm = SolidMechanicsModel::with_parser_and_type(
            shared.clone(),
            Some(DIM),
            "solid_mechanics_model",
            solid_parser(parser),
            ModelType::SolidMechanicsModelCohesive,
        )?;
        if cfg.compat == CompatMode::Upstream {
            upstream_position_cache(&mut smm);
        }
        smm.init_full(&options)?;
        Ok(Driven::Solid(Box::new(smm)))
    };
    let cohesive = || -> Result<Driven> {
        let mut m = SolidMechanicsModelCohesive::with_parser(
            shared.clone(),
            Some(DIM),
            akantu_cohesive::model::DEFAULT_ID,
            parser.clone(),
        )?;
        if cfg.compat == CompatMode::Upstream {
            m.ext.compat = CohesiveUpstreamCompat::upstream();
            upstream_position_cache(&mut m.smm);
        }
        // D-105 / UD-1280: the load placement at doubled nodes, separately
        // from the laws' mode
        m.ext.compat.doubled_node_keeps_external_force =
            cfg.doubled_node_load.keeps_external_force(cfg.compat);
        m.init_full(&options, true)?;
        Ok(Driven::Cohesive(Box::new(m)))
    };
    match cfg.model {
        ModelChoice::Cohesive => cohesive(),
        ModelChoice::Solid => {
            if cfg.cohesive_insertion {
                return Err(Error::invalid(
                    "rust_model = solid cannot insert cohesive elements (cohesive_insertion = true)",
                ));
            }
            solid(None)
        }
        ModelChoice::Auto => match cohesive() {
            Err(Error::Unsupported(msg)) if !cfg.cohesive_insertion => solid(Some(&msg)),
            Err(Error::Unsupported(msg)) => Err(Error::Unsupported(format!(
                "the cohesive model cannot run this insertion: {msg}"
            ))),
            other => other,
        },
    }
}
