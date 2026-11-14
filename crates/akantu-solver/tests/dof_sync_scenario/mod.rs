// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_synchronizer/test_dof_synchronizer.cc
// (PARALLEL, FILES_TO_COPY bar.msh): a nodal DOF family on a distributed
// mesh, its DOF synchronizer (MPI-018) synchronising, gathering and
// scattering a per-DOF array. Shared by `test_dof_synchronizer.rs` (local
// world) and `akantu-solid`'s `mpi_smm` (real MPI).

#![allow(dead_code)] // each including binary uses a subset

use std::path::PathBuf;

use akantu_comm::Communicator;
use akantu_core::enums::MeshIoType;
use akantu_core::{ElementType, GhostType, Idx};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::DofManager;
use akantu_solver::dof_synchronizer::{DofEntities, build_dof_synchronizer};

/// Upstream `bar.msh` of `test/test_synchronizer` (copied verbatim, see
/// `tests/data/upstream/README.md`).
pub fn bar_msh_path() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../akantu-solver/tests/data/upstream/test_synchronizer/bar.msh")
}

fn distribute_rcb(m: &mut Mesh) -> Result<(), String> {
    m.compute_bounding_box().map_err(|e| e.to_string())?;
    let mut p = RcbPartitioner;
    distribute_with(m, Some(&mut p as &mut dyn MeshPartitioner), 0)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

/// Upstream set-up: `bar.msh` read on rank 0 (`Mesh mesh(2)`), then
/// `mesh.distribute()`. Partitioner: the pure-Rust bisection instead of
/// upstream's default Scotch (the DOF-synchronizer contract checked below
/// does not depend on the partition; Scotch is a Docker-only feature).
pub fn bar_msh(comm: &Communicator) -> Result<Mesh, String> {
    let mut m = Mesh::with_communicator(2, "mesh", comm.clone()).map_err(|e| e.to_string())?;
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut m, &bar_msh_path(), MeshIoType::Gmsh)
            .map_err(|e| format!("read bar.msh: {e}"))?;
        if m.nb_nodes() != 25 {
            return Err(format!("bar.msh: {} nodes, expected 25", m.nb_nodes()));
        }
    }
    distribute_rcb(&mut m)?;
    Ok(m)
}

/// A structured triangle grid of `2n × n` cells built on rank 0 and
/// distributed by bisection.
pub fn grid(comm: &Communicator, n: usize) -> Result<Mesh, String> {
    let mut m = Mesh::with_communicator(2, "bar", comm.clone()).map_err(|e| e.to_string())?;
    if comm.rank() == 0 {
        let mut a = MeshAccessor::new(&mut m);
        for j in 0..=n {
            for i in 0..=2 * n {
                a.push_node(&[i as f64, j as f64])
                    .map_err(|e| e.to_string())?;
            }
        }
        let w = (2 * n + 1) as Idx;
        let mut c = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..n as Idx {
            for i in 0..2 * n as Idx {
                let n0 = j * w + i;
                c.push_tuple(&[n0, n0 + 1, n0 + w + 1]);
                c.push_tuple(&[n0, n0 + w + 1, n0 + w]);
            }
        }
    }
    distribute_rcb(&mut m)?;
    Ok(m)
}

/// The upstream test body on a distributed 2-D `mesh`: register a nodal
/// family of 2 components, fill the local-or-master DOFs with
/// `[2g, 2g + 1]` (`g` the global equation number) and the others with
/// `[-1, -1]`; `synchronizeArray` must give every local, master and slave
/// DOF its expected value (pure ghosts keep `[-1, -1]`); then `gather` on
/// rank 0 (every global DOF in order), reset to `-1`, `scatter` back, and
/// the same check.
pub fn check_dof_synchronizer(comm: &Communicator, mesh: &Mesh) -> Result<(), String> {
    let mut dm = DofManager::new("test_dof_manager", comm.clone());
    dm.register_nodal_dofs(mesh, "test_synchronize", 2)
        .map_err(|e| e.to_string())?;
    let sync = build_dof_synchronizer(&dm, mesh).map_err(|e| e.to_string())?;
    let layout = dm.layout().clone();
    let n = dm.local_system_size();
    let expected = |l: usize| -> [i32; 2] {
        match layout.global(l) {
            Some(g) => [2 * g as i32, 2 * g as i32 + 1],
            None => [-1, -1],
        }
    };
    let mut data = vec![0i32; 2 * n];
    for l in 0..n {
        let v = if layout.is_owned(l) {
            expected(l)
        } else {
            [-1, -1]
        };
        data[2 * l..2 * l + 2].copy_from_slice(&v);
    }
    sync.synchronize_array(&mut data, 2)
        .map_err(|e| e.to_string())?;
    let check = |data: &[i32]| -> Result<(), String> {
        for l in 0..n {
            let want = expected(l);
            if data[2 * l..2 * l + 2] != want {
                return Err(format!(
                    "rank {} dof {l}: {:?} != {want:?}",
                    comm.rank(),
                    &data[2 * l..2 * l + 2]
                ));
            }
        }
        Ok(())
    };
    check(&data)?;
    let e = DofEntities { dm: &dm };
    let global_nodes = mesh_global_nodes(comm, mesh)?; // collective
    let gathered = sync.gather(&e, &data, 2).map_err(|e| e.to_string())?;
    if comm.rank() == 0 {
        let g = gathered.as_ref().ok_or("no gather on root")?;
        if g.len() != 2 * 2 * global_nodes {
            return Err(format!("gathered {} values", g.len()));
        }
        for (i, w) in g.chunks(2).enumerate() {
            if w != [2 * i as i32, 2 * i as i32 + 1] {
                return Err(format!("gathered dof {i}: {w:?}"));
            }
        }
    } else if gathered.is_some() {
        return Err("gather returned data on a non-root rank".into());
    }
    data.fill(-1);
    sync.scatter(&e, &mut data, gathered.as_deref(), 2)
        .map_err(|e| e.to_string())?;
    check(&data)
}

/// Number of global nodes (largest global id + 1, agreed by every rank).
fn mesh_global_nodes(comm: &Communicator, mesh: &Mesh) -> Result<usize, String> {
    let mut m = [(0..mesh.nb_nodes())
        .map(|n| mesh.node_global_id(n as Idx) + 1)
        .max()
        .unwrap_or(0)];
    comm.all_reduce(&mut m, akantu_comm::ReduceOp::Max)
        .map_err(|e| e.to_string())?;
    usize::try_from(m[0]).map_err(|e| e.to_string())
}
