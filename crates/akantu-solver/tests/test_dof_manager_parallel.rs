// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// {test_dof_manager.cc, test_dof_mesh_distribute.cc, CMakeLists.txt}
// (`register_gtest_test(test_dof_manager … PARALLEL)`: the suites run at
// 1, 2, 4 and 8 ranks), default DOF manager, on the frozen mesh.msh.

//! TEST-070 / TEST-071 at 2 and 4 ranks.
//!
//! What upstream's test really does there (pinned x86_64
//! oracle, `mpiexec -n 2|4 test_dof_manager_gtest`, gtest XML report of
//! every rank): rank 0 passes all 21 tests; **every other rank fails**
//! `DOFManagerFixture.splitArrayPerDOFs` and `.AssembleVector` of the
//! default DOF manager (2 ranks: 25 failed expectations each on rank 1;
//! 4 ranks: 15 / 15 / 25 on ranks 1 / 2 / 3), the PETSc variant and every
//! other test pass, so the ctest runs at 2 / 4 ranks fail. The failing
//! expectations are exactly the slave nodes of the rank (shared nodes it
//! does not own): the test fills only the local-or-master nodes, and
//! `assembleToGlobalArray` / `getArrayPerDOFs` are local (no exchange), so
//! a slave node reads 0 where the test expects its global id (or 1). Rank
//! 0 owns every node it shares (the lowest rank owns a shared node), so it
//! has no slave node and passes. The residual sizes (`getResidual().size()`
//! is the **global** system size) and `TestMesh.TestMeshDistribute` pass on
//! every rank.
//!
//! These tests reproduce exactly that outcome: every check upstream makes,
//! on every rank, with the failing expectations asserted to be the slave
//! nodes and only them (none on rank 0). The partition is RCB (upstream:
//! Scotch); the claims do not depend on it.

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::{Array, ElementType, GhostType, Idx};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, MeshEvent, MeshEventHandler};
use akantu_solver::DofManager;
use akantu_validation::fixtures_root;

const HEX: ElementType = ElementType::Hexahedron8;
const TIMEOUT: Duration = Duration::from_secs(120);

/// Rank 0 reads `mesh.msh`, the mesh is distributed (upstream
/// `if (prank == 0) mesh->read("mesh.msh"); mesh->distribute();`).
fn distributed_mesh(comm: &Communicator) -> (Mesh, Vec<MeshEvent>) {
    let path = fixtures_root().join("meshes/gmsh-4.13.1/test/test_model/test_common/mesh.msh");
    let mut mesh = Mesh::with_communicator(3, "mesh", comm.clone()).unwrap();
    if comm.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).unwrap();
    }
    let mut p = RcbPartitioner;
    let events = distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
    (mesh, events.0)
}

struct Fixture {
    mesh: Mesh,
    dm: DofManager,
    nb_nodes: usize,
    nb_total_nodes: usize,
    nb_pure_local: usize,
}

fn set_up(comm: &Communicator) -> Fixture {
    let (mesh, _) = distributed_mesh(comm);
    let nb_nodes = mesh.nb_nodes();
    let nb_pure_local = (0..nb_nodes)
        .filter(|&n| mesh.node_flag(idx(n)).is_local_or_master())
        .count();
    Fixture {
        nb_total_nodes: mesh.nb_global_nodes(),
        dm: DofManager::new("dof_manager", comm.clone()),
        mesh,
        nb_nodes,
        nb_pure_local,
    }
}

fn idx(n: usize) -> Idx {
    Idx::try_from(n).unwrap()
}

fn global_size(n: usize) -> i64 {
    i64::try_from(n).unwrap()
}

impl Fixture {
    /// `registerDOFs(dst1, dst2)` with upstream's size checks
    /// (`getResidual().size()` = global system size).
    fn register(&mut self, nodal1: bool, nodal2: bool) {
        if nodal1 {
            self.dm.register_nodal_dofs(&self.mesh, "dofs1", 3).unwrap();
        } else {
            self.dm
                .register_generic_dofs("dofs1", self.nb_pure_local, 3)
                .unwrap();
        }
        assert_eq!(self.dm.system_size(), global_size(self.nb_total_nodes * 3));
        if nodal2 {
            self.dm.register_nodal_dofs(&self.mesh, "dofs2", 5).unwrap();
        } else {
            self.dm
                .register_generic_dofs("dofs2", self.nb_pure_local, 5)
                .unwrap();
        }
        assert_eq!(self.dm.system_size(), global_size(self.nb_total_nodes * 8));
    }

    /// The nodes where upstream's value expectations are checked (not pure
    /// ghost) split into (owned, slave).
    fn checked_nodes(&self) -> (Vec<usize>, Vec<usize>) {
        (0..self.nb_nodes)
            .filter(|&n| !self.mesh.node_flag(idx(n)).is_pure_ghost())
            .partition(|&n| self.mesh.node_flag(idx(n)).is_local_or_master())
    }
}

/// Run `f` on 2 and 4 ranks; every rank must return `Ok`.
fn on_ranks(f: impl Fn(&Communicator) -> Result<(), String> + Send + Sync + Copy + 'static) {
    for n in [2, 4] {
        for (r, o) in run_local(n, TIMEOUT, move |c| f(&c))
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("{n} ranks, rank {r}: {e}");
            }
        }
    }
}

/// Upstream's outcome of a value check over the checked nodes: no failed
/// expectation on rank 0, failed expectations exactly at the slave nodes on
/// the other ranks (and at least one: every rank > 0 shares nodes with a
/// lower rank).
fn upstream_outcome(
    comm: &Communicator,
    what: &str,
    failed: &[usize],
    slaves: &[usize],
) -> Result<(), String> {
    if failed != slaves {
        return Err(format!(
            "{what}: failed expectations at {failed:?}, slave nodes {slaves:?}"
        ));
    }
    match (comm.rank(), failed.len()) {
        (0, 0) => Ok(()),
        (0, k) => Err(format!("{what}: rank 0 fails {k} expectations")),
        (_, 0) => Err(format!("{what}: a rank > 0 without slave nodes")),
        _ => Ok(()),
    }
}

/// `Construction`, `DoubleConstruction`.
#[test]
fn construction_and_double_construction_parallel() {
    on_ranks(|c| {
        let mut f = set_up(c);
        f.dm = DofManager::new("dof_manager", c.clone());
        (f.dm.system_size() == 0)
            .then_some(())
            .ok_or_else(|| "a new manager has equations".into())
    });
}

/// `RegisterGenericDOF1`, `RegisterNodalDOF1` (`EXPECT_GE(size, 3·N)`).
#[test]
fn register_dof1_parallel() {
    on_ranks(|c| {
        let mut f = set_up(c);
        f.dm.register_generic_dofs("dofs1", f.nb_pure_local, 3)
            .unwrap();
        let generic = f.dm.system_size();
        let mut f = set_up(c);
        f.dm.register_nodal_dofs(&f.mesh, "dofs1", 3).unwrap();
        let nodal = f.dm.system_size();
        let want = global_size(f.nb_total_nodes * 3);
        (generic >= want && nodal >= want)
            .then_some(())
            .ok_or_else(|| format!("sizes {generic}, {nodal} < {want}"))
    });
}

/// `RegisterGenericDOF2`, `RegisterNodalDOF2`, `RegisterMixedDOF`.
#[test]
fn register_dof2_parallel() {
    on_ranks(|c| {
        set_up(c).register(false, false);
        set_up(c).register(true, true);
        set_up(c).register(true, false);
        Ok(())
    });
}

/// `splitArrayPerDOFs`: owned nodes get their global id back; the slave
/// nodes read 0 (upstream's failing expectations on the ranks > 0).
#[test]
fn split_array_per_dofs_parallel() {
    on_ranks(|c| {
        let mut f = set_up(c);
        f.dm.register_nodal_dofs(&f.mesh, "dofs1", 1).unwrap();
        let gid = |n: usize| -> f64 {
            let g = f.mesh.node_global_id(idx(n));
            #[allow(clippy::cast_precision_loss)] // global ids of a small mesh
            let v = g as f64;
            v
        };
        let dofs: Vec<f64> = (0..f.nb_nodes)
            .map(|n| {
                if f.mesh.node_flag(idx(n)).is_local_or_master() {
                    gid(n)
                } else {
                    0.0
                }
            })
            .collect();
        f.dm.new_lumped_matrix("nodes").unwrap();
        let mut nodes = std::mem::take(&mut f.dm.lumped_matrix_mut("nodes").unwrap().values);
        f.dm.assemble_to_global_array("dofs1", &dofs, &mut nodes, 1.0)
            .unwrap();
        let mut out = Array::try_zeros(f.nb_nodes, 1).unwrap();
        f.dm.get_array_per_dofs("dofs1", &nodes, &mut out).unwrap();
        let (owned, slaves) = f.checked_nodes();
        for &n in &owned {
            if out.as_slice()[n] != gid(n) {
                return Err(format!(
                    "owned node {n}: {} != {}",
                    out.as_slice()[n],
                    gid(n)
                ));
            }
        }
        let failed: Vec<usize> = (0..f.nb_nodes)
            .filter(|&n| !f.mesh.node_flag(idx(n)).is_pure_ghost())
            .filter(|&n| out.as_slice()[n] != gid(n))
            .collect();
        if slaves.iter().any(|&n| out.as_slice()[n] != 0.0) {
            return Err("a slave node does not read 0".into());
        }
        upstream_outcome(c, "splitArrayPerDOFs", &failed, &slaves)
    });
}

/// `AssembleVector`: `dofs1` (nodal) reads 1 at owned nodes and 0 at the
/// slave nodes (upstream's failing expectations on the ranks > 0); `dofs2`
/// (generic) reads 2 everywhere.
#[test]
fn assemble_vector_parallel() {
    on_ranks(|c| {
        let mut f = set_up(c);
        f.register(true, false);
        f.dm.zero_residual();
        let d1: Vec<f64> = (0..f.nb_nodes)
            .flat_map(|n| [f64::from(u8::from(f.mesh.node_flag(idx(n)).is_local_or_master())); 3])
            .collect();
        let d2 = vec![2.0; f.nb_pure_local * 5];
        f.dm.assemble_to_residual(
            "dofs1",
            akantu_core::array::ArrayView::new(&d1, 3).unwrap(),
            1.0,
        )
        .unwrap();
        f.dm.assemble_to_residual(
            "dofs2",
            akantu_core::array::ArrayView::new(&d2, 5).unwrap(),
            1.0,
        )
        .unwrap();
        let r = f.dm.residual().values().to_vec();
        let mut a1 = Array::try_zeros(0, 3).unwrap();
        let mut a2 = Array::try_zeros(0, 5).unwrap();
        f.dm.get_array_per_dofs("dofs1", &r, &mut a1).unwrap();
        f.dm.get_array_per_dofs("dofs2", &r, &mut a2).unwrap();
        // upstream: |dof − ref| / |ref| < 1e-14 per non-pure-ghost node
        let (_, slaves) = f.checked_nodes();
        let failed: Vec<usize> = (0..f.nb_nodes)
            .filter(|&n| !f.mesh.node_flag(idx(n)).is_pure_ghost())
            .filter(|&n| a1.as_slice()[3 * n..3 * n + 3] != [1.0; 3])
            .collect();
        if slaves
            .iter()
            .any(|&n| a1.as_slice()[3 * n..3 * n + 3] != [0.0; 3])
        {
            return Err("dofs1: a slave node does not read 0".into());
        }
        upstream_outcome(c, "AssembleVector dofs1", &failed, &slaves)?;
        if a2.as_slice().iter().any(|&x| x != 2.0) {
            return Err("dofs2: not 2 everywhere".into());
        }
        Ok(())
    });
}

/// `AssembleMatrixNodal`: the elemental assemblies of both families into
/// the symmetric `K` succeed on every rank (upstream only saves the
/// matrix).
#[test]
fn assemble_matrix_nodal_parallel() {
    on_ranks(|c| {
        let mut f = set_up(c);
        f.register(true, true);
        f.dm.new_matrix("K", MatrixType::Symmetric).unwrap();
        f.dm.zero_matrix("K").unwrap();
        let ne = f.mesh.nb_element(HEX, GhostType::NotGhost);
        let ones = Array::from_vec(vec![1.0; ne * 24 * 24], 24 * 24).unwrap();
        let twos = Array::from_vec(vec![2.0; ne * 40 * 40], 40 * 40).unwrap();
        for (id, m) in [("dofs1", &ones), ("dofs2", &twos)] {
            f.dm.assemble_elemental_matrices_to_matrix(
                &f.mesh,
                "K",
                id,
                m.view(),
                HEX,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                None,
            )
            .map_err(|e| format!("{id}: {e}"))?;
        }
        Ok(())
    });
}

/// `TestMesh.TestMeshDistribute`: a manager created on the mesh before its
/// distribution refuses the distribution event (upstream's
/// `mesh1.distribute()` throws), a manager created after it accepts it.
#[test]
fn test_dof_mesh_distribute_parallel() {
    on_ranks(|c| {
        let path = fixtures_root().join("meshes/gmsh-4.13.1/test/test_model/test_common/mesh.msh");
        let mut mesh1 = Mesh::with_communicator(3, "mesh1", c.clone()).unwrap();
        if c.rank() == 0 {
            akantu_io::read_mesh(&mut mesh1, &path, MeshIoType::Gmsh).unwrap();
        }
        let mut dm1 = DofManager::new("test_dof_manager_1__failing", c.clone());
        dm1.register_nodal_dofs(&mesh1, "u", 3).unwrap();
        let mut p = RcbPartitioner;
        let events =
            distribute_with(&mut mesh1, Some(&mut p as &mut dyn MeshPartitioner), 0).unwrap();
        let ev = events
            .0
            .iter()
            .find(|e| matches!(e, MeshEvent::MeshIsDistributed { .. }))
            .ok_or("no MeshIsDistributed event")?;
        if dm1.check_mesh_event(&mesh1, ev).is_ok() {
            return Err("the manager of the undistributed mesh accepts the distribution".into());
        }
        let (mesh2, events2) = distributed_mesh(c);
        let mut dm2 = DofManager::new("test_dof_manager_2__succeeding", c.clone());
        dm2.register_nodal_dofs(&mesh2, "u", 3).unwrap();
        let ev2 = events2
            .iter()
            .find(|e| matches!(e, MeshEvent::MeshIsDistributed { .. }))
            .ok_or("no MeshIsDistributed event")?;
        dm2.check_mesh_event(&mesh2, ev2)
            .map_err(|e| format!("manager of the distributed mesh: {e}"))
    });
}
