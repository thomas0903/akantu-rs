// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): examples/c++/solid_mechanics_model/
// parallel/parallel_2d.cc (+ material.dat, square_2d.geo).

//! `parallel_2d` (EX-022): explicit dynamics on a distributed mesh. The
//! unit square (linear triangles) is read on rank 0 and distributed; the
//! left and right sides are pushed inwards by `1e-6` (x blocked);
//! `Δt = 0.8 Δt_crit`, 10 000 steps; dumps every 200 steps
//! (`paraview/multi_<n>.pvtu` + one `.vtu` per rank).
//!
//! Ranks: with the `mpi` feature the example runs on the MPI world
//! (`mpirun -np 2 target/release/examples/parallel_2d`); otherwise on one
//! rank, or on `--local-ranks N` in-process ranks (threads, the local
//! communicator of `akantu-comm`).
//!
//! Partition: upstream `mesh.distribute()` partitions with Scotch on rank
//! 0. The port uses Scotch with the `scotch` feature (seed 1, as upstream
//! with `AKA_SEED=1`), the pure-Rust recursive coordinate bisection
//! otherwise, and
//! `--partition <file>` replays a recorded partition (one part per element
//! of the mesh dimension in linearised order, as written by the oracle
//! variant `parallel_2d_partition`, decision D-009).
//!
//! `--upstream-compat` writes the padded stress in upstream's invalid
//! 9 × 9-values-per-cell layout (UD-110), for byte comparisons.
//!
//! Dumps (UD-475, as upstream): each rank's `.vtu` piece also holds its
//! **pure-ghost** points, whose nodal fields are integrated but never
//! synchronised; after 10 000 steps they reach about 1.76 while the real
//! nodes stay near 1.2e-6, so ParaView's whole-array colour range is
//! meaningless for `displacement`, `velocity` and `acceleration`: restrict
//! it to the cells.
//!
//! ```text
//! cargo run -p akantu --release --example parallel_2d -- --local-ranks 2
//! ```

#[path = "../common/mod.rs"]
pub mod common;

use std::path::Path;
use std::process::ExitCode;
use std::time::Duration;

use akantu::comm::Communicator;
use akantu::core::enums::AnalysisMethod;
use akantu::core::{Error, Idx, Real, Result};
use akantu::mesh::distribution::{MeshPartitioner, PrescribedPartition, distribute_with};
use akantu::mesh::{Mesh, SharedMesh};
use akantu::solid::SolidMechanicsModel;
use common::{Example, cout};

/// How rank 0 partitions the mesh.
#[derive(Clone, Debug)]
pub enum Partition {
    /// The default partitioner (Scotch with the `scotch` feature, else
    /// recursive coordinate bisection).
    Default,
    /// A recorded linearised partition (D-009).
    Prescribed(Vec<Idx>),
}

/// Result of a run on one rank.
#[derive(Debug)]
pub struct Outcome {
    /// The model in its final state (local part).
    pub model: SolidMechanicsModel,
    /// Time step used.
    pub time_step: Real,
}

/// Default configuration (upstream directory and input file).
#[must_use]
pub fn example() -> Example {
    Example::new(
        "parallel_2d",
        "solid_mechanics_model/parallel",
        "material.dat",
    )
}

/// Read a recorded partition (whitespace-separated integers).
///
/// # Errors
/// Unreadable file or non-integer entries.
pub fn read_partition(path: &Path) -> Result<Vec<Idx>> {
    let text = std::fs::read_to_string(path).map_err(|source| Error::Io {
        path: path.display().to_string(),
        source,
    })?;
    text.split_whitespace()
        .map(|t| {
            t.parse::<Idx>()
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
/// Any error of the mesh, the distribution, the model or the dumper.
pub fn run_on(comm: Communicator, ex: &Example, partition: &Partition) -> Result<Outcome> {
    let spatial_dimension = 2;
    let max_steps = ex.steps_or(10_000);
    let time_factor = 0.8;
    let max_disp = 1e-6;

    let prank = comm.rank();
    if prank == 0 {
        ex.create_output_dir()?;
    }
    let mut mesh = Mesh::with_communicator(spatial_dimension, "mesh", comm.clone())?;
    if prank == 0 {
        // Read the mesh
        akantu::io::read_mesh(
            &mut mesh,
            &ex.mesh_path("square_2d.msh"),
            akantu::core::enums::MeshIoType::Auto,
        )?;
    }
    distribute(&mut mesh, partition, 1)?;
    let mesh = SharedMesh::new(mesh);

    let mut model = SolidMechanicsModel::with_parser(
        mesh.clone(),
        None,
        "solid_mechanics_model",
        ex.parser()?,
    )?;
    model.init_full(&ex.model_options(AnalysisMethod::ExplicitLumpedMass))?;

    if prank == 0 {
        // upstream `std::cout << model.getMaterial(0) << "\n"`
        ex.say(common::law_printout(model.material(0)?, &|_| None)?);
    }

    model.set_directory(&ex.paraview_dir())?;
    if ex.upstream_compat {
        // UD-110: upstream writes the padded 2-D stress as 9 × 9 values per
        // cell (invalid VTK); the compat layout reproduces those bytes
        model
            .dumpers
            .dumpable
            .dumper_as::<akantu::io::DumperParaview>("solid_mechanics_model")?
            .set_compat_layout(true);
    }
    model.set_base_name("multi")?;
    model.add_dump_field_vector("displacement")?;
    model.add_dump_field_vector("velocity")?;
    model.add_dump_field_vector("acceleration")?;
    model.add_dump_field_tensor("stress")?;
    model.add_dump_field_tensor("grad_u")?;

    // boundary conditions
    let eps = 1e-16;
    {
        let (pos, left_side, right_side) = {
            let m = mesh.read()?;
            (
                m.nodes().as_slice().to_vec(),
                m.lower_bounds()[0],
                m.upper_bounds()[0],
            )
        };
        let nb_nodes = pos.len() / spatial_dimension;
        let mut disp = model.displacement()?.as_slice().to_vec();
        let mut boun = model.blocked_dofs()?.as_slice().to_vec();
        for i in 0..nb_nodes {
            let x = pos[i * spatial_dimension];
            if (x - left_side).abs() < eps {
                disp[i * spatial_dimension] = max_disp;
                boun[i * spatial_dimension] = true;
            }
            if (x - right_side).abs() < eps {
                disp[i * spatial_dimension] = -max_disp;
                boun[i * spatial_dimension] = true;
            }
        }
        model
            .displacement_mut()?
            .as_mut_slice()
            .copy_from_slice(&disp);
        model
            .blocked_dofs_mut()?
            .as_mut_slice()
            .copy_from_slice(&boun);
    }

    let time_step = model.stable_time_step()? * time_factor;
    ex.say(format!("Time Step = {}s", cout(time_step)));
    model.set_time_step(time_step, None)?;

    if ex.dump {
        model.dump(None, None)?;
    }
    for s in 1..=max_steps {
        model.solve_step(None)?;
        if s.is_multiple_of(200) && ex.dump {
            model.dump(None, None)?;
        }
        if prank == 0 && s.is_multiple_of(100) {
            ex.say(format!("passing step {s}/{max_steps}"));
        }
    }

    Ok(Outcome { model, time_step })
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
