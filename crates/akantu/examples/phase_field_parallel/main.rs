// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/phase_field_model/
// phase_field_parallel.cc (+ material_notch.dat, square_notch.geo).

//! `phase_field_parallel` (EX-011): the notched plate of
//! `phase_field_notch` on a distributed mesh (`mesh.distribute()`: rank 0
//! reads and partitions it), the material and phase-field selectors on the
//! `physical_names` mesh data, 999 load steps of 10 staggered solves, a dump
//! of the phase-field model first (with the partitions when the mesh is
//! distributed: always, UD-011) and of the solid model every 100 steps
//! (`paraview/phase_notch_parallel_<n>.pvtu`); every rank prints its number
//! of nodes, rank 0 the progress line every 10 steps (wall-clock timings).
//! Upstream sets the material selector **after** `model.initFull`, so it
//! changes nothing (the default selector assigned the materials); the port
//! does the same.
//!
//! On several ranks (`--local-ranks <n>`, `--partition <file>` as
//! `parallel_2d`; MPI with the `mpi` feature) the mesh is distributed and
//! the coupler runs on it (the phase-field model's `_pfm_damage` /
//! `_for_dump` exchanges, MPI-044/045): with the oracle's
//! recorded partitions the 2- and 4-rank initial dumps and the step-100
//! mesh / partition arrays equal upstream's, and the values agree with the
//! port's one-rank run to 1e-14 (D-082); upstream's own values at 2 / 4
//! ranks are run-dependent, see `crates/akantu/examples/README.md` (with
//! the ghost defect UD-004, inert in the solves). On one rank upstream's `mesh.distribute()` only builds empty synchronizers
//! and flags the mesh distributed (UD-011), which changes no value; the port
//! keeps the mesh serial there (as `scalability_test`) and dumps the
//! partitions as upstream does for its (always) distributed mesh.
//!
//! `--upstream-compat` selects the defects of the coupled phase-field path
//! as in `phase_field_notch`; they are not reached here: on one rank **the
//! default run reproduces upstream** bitwise (with `--solver eigen`; the
//! oracle comparison is tested in both modes).
//!
//! ```text
//! cargo run -p akantu --release --example phase_field_parallel [-- --local-ranks 2]
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::path::Path;
use std::process::ExitCode;
use std::time::{Duration, Instant};

use akantu::comm::Communicator;
use akantu::core::enums::{AnalysisMethod, SpatialDirection};
use akantu::core::{Error, Idx, Result};
use akantu::couplers::CouplerSolidPhaseField;
use akantu::mesh::distribution::{MeshPartitioner, PrescribedPartition, distribute_with};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::model::Model;
use akantu::model::boundary_condition::{FixedValue, IncrementValue};
use akantu::model::constitutive_law::ElementDataSelector;
use akantu::phase_field::selector::MeshDataPhaseFieldSelector;
use akantu::solid::material::LawOptions;
use common::Example;

/// Spatial dimension of the example (upstream `const UInt spatial_dimension`).
pub const SPATIAL_DIMENSION: usize = 2;

/// How rank 0 partitions the mesh (as `parallel_2d`).
#[derive(Clone, Debug)]
pub enum Partition {
    /// The default partitioner (Scotch with the `scotch` feature, else
    /// recursive coordinate bisection).
    Default,
    /// A recorded linearised partition (D-009).
    Prescribed(Vec<Idx>),
}

/// Result of a run.
#[derive(Debug)]
pub struct Outcome {
    /// The coupler in its final state.
    pub coupler: CouplerSolidPhaseField,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "phase_field_parallel",
        "phase_field_model",
        "material_notch.dat",
    )
}

/// Read a recorded partition (whitespace-separated integers).
///
/// # Errors
/// I/O or parse errors.
pub fn read_partition(path: &Path) -> Result<Vec<Idx>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    text.split_whitespace()
        .map(|t| {
            t.parse()
                .map_err(|e| Error::invalid(format!("partition entry '{t}': {e}")))
        })
        .collect()
}

/// The default partitioner of rank 0.
fn default_partitioner(seed: i64) -> Box<dyn MeshPartitioner> {
    #[cfg(feature = "scotch")]
    {
        Box::new(akantu::mesh::scotch::ScotchPartitioner::new(seed))
    }
    #[cfg(not(feature = "scotch"))]
    {
        let _ = seed;
        Box::new(akantu::mesh::distribution::RcbPartitioner)
    }
}

/// Upstream `mesh.distribute()`: rank 0 partitions (more than one rank)
/// and sends every rank its part; one rank only builds the synchronizers.
fn distribute(mesh: &mut Mesh, partition: &Partition, seed: i64) -> Result<()> {
    let comm = mesh.communicator();
    let mut partitioner: Option<Box<dyn MeshPartitioner>> = if comm.size() > 1 && comm.rank() == 0 {
        Some(match partition {
            Partition::Prescribed(parts) => Box::new(PrescribedPartition::new(parts.clone())),
            Partition::Default => default_partitioner(seed),
        })
    } else {
        None
    };
    // no mesh consumer exists yet: the distribution events have no receiver
    let _events = match partitioner.as_mut() {
        Some(p) => distribute_with(mesh, Some(p.as_mut()), 0)?,
        None => distribute_with(mesh, None, 0)?,
    };
    Ok(())
}

/// The upstream `main` after `initialize`, on the ranks of `comm`.
///
/// # Errors
/// Any error of the mesh, the distribution, the models, the solvers or the
/// dumpers (agreed on every rank on a distributed mesh).
pub fn run_on(comm: Communicator, ex: &Example, partition: &Partition) -> Result<Outcome> {
    // create mesh
    let mut mesh = Mesh::with_communicator(SPATIAL_DIMENSION, "mesh", comm.clone())?;

    let prank = comm.rank();
    if prank == 0 {
        ex.create_output_dir()?;
        // Read the mesh
        akantu::io::read_mesh(
            &mut mesh,
            &ex.mesh_path("square_notch.msh"),
            akantu::core::enums::MeshIoType::Auto,
        )?;
    }

    // upstream `mesh.distribute()`; on one rank the port keeps the mesh
    // serial (see the module docs), upstream's `isDistributed()` is true
    let is_distributed = if comm.size() > 1 {
        distribute(&mut mesh, partition, 1)?;
        mesh.is_distributed()
    } else {
        true
    };

    let mut coupler = CouplerSolidPhaseField::with_parser(
        SharedMesh::new(mesh),
        None,
        "coupler_solid_phasefield",
        ex.parser()?,
    )?;
    if ex.upstream_compat {
        coupler.set_upstream_compat(true);
        coupler.solid.physics.law_options = Some(LawOptions::upstream());
    }

    coupler
        .solid
        .init_full(&ex.model_options(AnalysisMethod::Static))?;
    // upstream sets the material selector after initFull: no effect
    coupler
        .solid
        .set_material_selector(Box::new(ElementDataSelector::new("physical_names")));

    coupler
        .phase
        .set_phase_field_selector(Box::new(MeshDataPhaseFieldSelector::new("physical_names")));

    coupler
        .phase
        .init_full(&ex.model_options(AnalysisMethod::Static))?;

    let fixed = |value, axis| FixedValue { value, axis };
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(0., SpatialDirection::Y), "bottom")?;
    coupler
        .solid
        .apply_dirichlet_bc(&fixed(0., SpatialDirection::X), "left")?;

    coupler.solid.set_directory(&ex.paraview_dir())?;
    coupler.solid.set_base_name("phase_notch_parallel")?;
    coupler.solid.add_dump_field("stress")?;
    coupler.solid.add_dump_field("grad_u")?;
    coupler.solid.add_dump_field_vector("displacement")?;
    coupler.solid.add_dump_field("damage")?;
    // upstream `phase.addDumpField("partitions")` and `phase.dump()`: every
    // model registers its dumper on the mesh and makes it the mesh's
    // default, so both models' `addDumpField` / `dump` act on one series
    // (the phase-field model's dumper, renamed `phase_notch_parallel` by
    // `model.setBaseName`); the port's models have their own dumpers: the
    // field goes to, and the dump comes from, the solid's
    if is_distributed {
        coupler.solid.add_dump_field("partitions")?;
    }
    if ex.dump {
        coupler.solid.dump(None, None)?;
    }

    let nb_steps = ex.steps.map_or(1000, |s| (s + 1).min(1000));
    let increment = 4e-6;
    let nb_staggered_steps = 10;

    let nb_nodes = coupler.solid.mesh().read()?.nb_nodes();
    ex.say(format!("Number of nodes: {nb_nodes}"));

    let start_time = Instant::now();

    for s in 1..nb_steps {
        if s.is_multiple_of(10) && prank == 0 {
            ex.print_raw(progress_line(s, nb_steps, start_time.elapsed()));
        }
        coupler.solid.apply_dirichlet_bc(
            &IncrementValue {
                value: increment,
                axis: SpatialDirection::Y,
            },
            "top",
        )?;

        for _ in 0..nb_staggered_steps {
            coupler.solve(None, None)?;
        }

        if s.is_multiple_of(100) && ex.dump {
            coupler.solid.dump(None, None)?;
        }
    }

    Ok(Outcome { coupler })
}

/// The progress line (as `phase_field_notch`: `\r[wheel] s/n (ms/step -
/// elapsed - ETA)` and 32 characters `\x14` from the swapped arguments of
/// `std::string(' ', 20)`).
#[must_use]
pub fn progress_line(s: usize, nb_steps: usize, elapsed: Duration) -> String {
    const WHEEL: [char; 4] = ['/', '-', '\\', '|'];
    let elapsed_ns = elapsed.as_nanos();
    let per_step_ns = elapsed_ns / s as u128;
    #[allow(clippy::cast_precision_loss)] // wall-clock durations
    let (ms_per_step, elapsed_s, eta_s) = (
        per_step_ns as f64 / 1e6,
        elapsed_ns as f64 / 1e9,
        ((nb_steps - s) as u128 * per_step_ns) as f64 / 1e9,
    );
    format!(
        "\r[{}] {s:>5}/{nb_steps} ({ms_per_step:>8.2}ms/step - elapsed: {elapsed_s:>8.2}s - ETA: \
         {eta_s:>8.2}s){}",
        WHEEL[(s / 10) % 4],
        "\u{14}".repeat(32)
    )
}

/// Command line: the common options plus `--partition <file>` and
/// `--local-ranks <n>`.
fn split_args(args: &[String]) -> Result<(Vec<String>, Partition, usize)> {
    let mut rest = Vec::new();
    let mut partition = Partition::Default;
    let mut local_ranks = 1;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--partition" => {
                let f = it
                    .next()
                    .ok_or_else(|| Error::invalid("--partition needs a file"))?;
                partition = Partition::Prescribed(read_partition(Path::new(f))?);
            }
            "--local-ranks" => {
                let n = it
                    .next()
                    .ok_or_else(|| Error::invalid("--local-ranks needs a number"))?;
                local_ranks = n
                    .parse()
                    .map_err(|e| Error::invalid(format!("--local-ranks {n}: {e}")))?;
            }
            _ => rest.push(a.clone()),
        }
    }
    Ok((rest, partition, local_ranks))
}

/// The world communicator (MPI with the `mpi` feature, else serial).
fn world() -> Result<Communicator> {
    #[cfg(feature = "mpi")]
    {
        Communicator::mpi_world(akantu::comm::MpiOptions::default())
    }
    #[cfg(not(feature = "mpi"))]
    {
        Ok(Communicator::serial())
    }
}

fn main() -> ExitCode {
    common::main_with(|args| {
        let (args, partition, local_ranks) = split_args(args)?;
        let ex = example().with_args(&args)?;
        if local_ranks > 1 {
            let results = akantu::comm::run_local(local_ranks, Duration::from_secs(3600), |c| {
                run_on(c, &ex, &partition).map(|_| ())
            });
            return results.into_iter().collect();
        }
        run_on(world()?, &ex, &partition).map(|_| ())
    })
}
