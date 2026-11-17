// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Helpers shared by the
// cases of the model suite (no upstream counterpart).

//! Helpers of the model cases: the world communicator, distribution of a
//! mesh read on rank 0, gathering of nodal results by global id (digests at
//! P > 1 ranks), input files.

use akantu_comm::Communicator;
use akantu_core::{Error, Real, Result};
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::synchronizers::NodeEntities;
use akantu_parser::Parser;

use akantu_core::enums::{AnalysisMethod, SparseSolverType};
use akantu_model::ModelOptions;
use akantu_solver::ModelSolverOptions;

use super::harness::{Digest, Run};

/// The world: MPI with the `mpi` feature, else serial.
///
/// # Errors
/// MPI initialisation.
pub fn world() -> Result<Communicator> {
    #[cfg(feature = "mpi")]
    {
        Communicator::mpi_world(akantu_comm::MpiOptions::default())
    }
    #[cfg(not(feature = "mpi"))]
    {
        Ok(Communicator::serial())
    }
}

/// Distribute a mesh read on rank 0 over its communicator (recursive
/// coordinate bisection, the partitioner every build has). No-op on one
/// rank.
///
/// # Errors
/// Distribution errors.
pub fn distribute(mesh: &mut Mesh) -> Result<()> {
    if mesh.communicator().size() == 1 {
        return Ok(());
    }
    let mut p = RcbPartitioner;
    let root = mesh.communicator().rank() == 0;
    let part: Option<&mut dyn MeshPartitioner> = if root { Some(&mut p) } else { None };
    // no other consumer is registered on the mesh yet: nothing to dispatch
    let _events = distribute_with(mesh, part, 0)?;
    Ok(())
}

/// Gather a nodal array (`nb_component` values per local node) by global
/// node id on rank 0: `Some` on rank 0, `None` elsewhere. One rank: the
/// array itself.
///
/// # Errors
/// Communication errors.
pub fn gather_nodal<T: akantu_comm::CommData + Copy>(
    mesh: &Mesh,
    values: &[T],
    nb_component: usize,
) -> Result<Option<Vec<T>>> {
    if mesh.communicator().size() == 1 {
        return Ok(Some(values.to_vec()));
    }
    let sync = mesh
        .synchronizers()
        .node
        .as_ref()
        .ok_or_else(|| Error::invalid("distributed mesh without node synchronizer"))?;
    sync.gather(&NodeEntities { mesh }, values, nb_component)
}

/// Absorb a nodal real array gathered by global id into `d` (rank 0 only;
/// collective).
///
/// # Errors
/// Communication errors.
pub fn digest_nodal(
    d: &mut Digest,
    mesh: &Mesh,
    values: &[Real],
    nb_component: usize,
) -> Result<()> {
    if let Some(g) = gather_nodal(mesh, values, nb_component)? {
        d.reals(&g);
    }
    Ok(())
}

/// Parse an input file given as text.
///
/// # Errors
/// Parse errors.
pub fn parser(text: &str) -> Result<Parser> {
    Parser::parse_str(text, "suite.dat")
}

/// Sum of a count over the ranks.
///
/// # Errors
/// Communication errors.
pub fn global_count(comm: &Communicator, local: usize) -> Result<usize> {
    let mut v = [local as u64];
    comm.all_reduce(&mut v, akantu_comm::ReduceOp::Sum)?;
    Ok(usize::try_from(v[0]).unwrap_or(usize::MAX))
}

/// Not-ghost elements of dimension `dim` in `mesh` (local).
#[must_use]
pub fn nb_elements(mesh: &Mesh, dim: usize) -> usize {
    mesh.nb_element_filtered(akantu_core::TypeFilter {
        spatial_dimension: Some(dim),
        ghost_type: akantu_core::GhostType::NotGhost,
        kind: None,
    })
}

/// Local nodes that this rank owns (all nodes on one rank).
#[must_use]
pub fn nb_owned_nodes(mesh: &Mesh) -> usize {
    if mesh.communicator().size() == 1 {
        return mesh.nb_nodes();
    }
    (0..mesh.nb_nodes())
        .filter(|&n| {
            let n = akantu_core::Idx::try_from(n).unwrap_or(akantu_core::Idx::MAX);
            mesh.is_local_or_master_node(n).unwrap_or(false)
        })
        .count()
}

/// Options of a case with a sparse solve: the port's translation of Eigen's
/// SparseLU pinned (`_auto` would resolve to MUMPS on the MPI world in a
/// build with the `mumps` feature, which changes the digest and the timing
/// and does not repeat its bits above 2 ranks). Records the solver in the
/// run's JSON lines (`sparse_solver`).
pub fn eigen_options(run: &mut Run, method: AnalysisMethod) -> ModelOptions {
    run.sparse_solver = "eigen";
    ModelOptions::new(method).with_solver_options(ModelSolverOptions {
        sparse_solver_type: Some(SparseSolverType::Eigen),
        sparse_solver_ordering: akantu_bench::suite::sparse_ordering(),
        ..ModelSolverOptions::default()
    })
}
