// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// {test_dof_manager.cc, test_dof_mesh_distribute.cc} (gtest suites
// DOFManagerFixture and TestMesh), default DOF manager, serial, on the
// frozen mesh.msh (hexahedra); oracle values from
// fixtures/solver_model/dof_manager_hex.

//! `test_dof_manager` (registrations, residual sizes, `splitArrayPerDOFs`,
//! `AssembleVector`, `AssembleMatrixNodal`) and `test_dof_mesh_distribute`,
//! with the assembled matrix and products compared bit for bit with the
//! oracle.

mod common;

use akantu_comm::Communicator;
use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::{Array, ElementType, GhostType, Idx};
use akantu_mesh::{Mesh, MeshAccessor, MeshEvent, MeshEventHandler};
use akantu_solver::DofManager;
use akantu_validation::fixtures_root;

use common::oracle::*;

const HEX: ElementType = ElementType::Hexahedron8;

fn mesh() -> Mesh {
    let path = fixtures_root().join("meshes/gmsh-4.13.1/test/test_model/test_common/mesh.msh");
    let mut m = Mesh::new(3, "mesh").unwrap();
    akantu_io::read_mesh(&mut m, &path, MeshIoType::Gmsh).unwrap();
    m
}

struct Fixture {
    mesh: Mesh,
    dm: DofManager,
    nb_nodes: usize,
    nb_total_nodes: usize,
    nb_pure_local: usize,
}

fn set_up() -> Fixture {
    let mesh = mesh();
    let nb_nodes = mesh.nb_nodes();
    let nb_pure_local = (0..nb_nodes)
        .filter(|&n| mesh.node_flag(n as Idx).is_local_or_master())
        .count();
    Fixture {
        nb_total_nodes: nb_nodes,
        mesh,
        dm: DofManager::new("dof_manager", Communicator::serial()),
        nb_nodes,
        nb_pure_local,
    }
}

impl Fixture {
    /// `registerDOFs(dst1, dst2)` of the fixture: `dofs1` (3 components),
    /// `dofs2` (5 components), with the residual-size checks.
    fn register(&mut self, nodal1: bool, nodal2: bool) {
        if nodal1 {
            self.dm.register_nodal_dofs(&self.mesh, "dofs1", 3).unwrap();
        } else {
            self.dm
                .register_generic_dofs("dofs1", self.nb_pure_local, 3)
                .unwrap();
        }
        assert_eq!(self.dm.residual().values().len(), self.nb_total_nodes * 3);
        if nodal2 {
            self.dm.register_nodal_dofs(&self.mesh, "dofs2", 5).unwrap();
        } else {
            self.dm
                .register_generic_dofs("dofs2", self.nb_pure_local, 5)
                .unwrap();
        }
        assert_eq!(self.dm.residual().values().len(), self.nb_total_nodes * 8);
    }
}

#[test]
fn construction_and_double_construction() {
    let mut f = set_up();
    f.dm = DofManager::new("dof_manager", Communicator::serial());
    assert_eq!(f.dm.system_size(), 0);
}

#[test]
fn register_generic_dof1() {
    let mut f = set_up();
    f.dm.register_generic_dofs("dofs1", f.nb_pure_local, 3)
        .unwrap();
    assert!(f.dm.residual().values().len() >= f.nb_total_nodes * 3);
}

#[test]
fn register_nodal_dof1() {
    let mut f = set_up();
    f.dm.register_nodal_dofs(&f.mesh, "dofs1", 3).unwrap();
    assert!(f.dm.residual().values().len() >= f.nb_total_nodes * 3);
}

#[test]
fn register_generic_dof2() {
    set_up().register(false, false);
}

#[test]
fn register_nodal_dof2() {
    set_up().register(true, true);
}

#[test]
fn register_mixed_dof() {
    set_up().register(true, false);
}

#[test]
fn split_array_per_dofs() {
    let mut f = set_up();
    f.dm.register_nodal_dofs(&f.mesh, "dofs1", 1).unwrap();
    let gid: Vec<f64> = (0..f.nb_nodes)
        .map(|n| {
            if f.mesh.node_flag(n as Idx).is_local_or_master() {
                f.mesh.node_global_id(n as Idx) as f64
            } else {
                0.0
            }
        })
        .collect();
    f.dm.new_lumped_matrix("nodes").unwrap();
    let mut nodes = std::mem::take(&mut f.dm.lumped_matrix_mut("nodes").unwrap().values);
    f.dm.assemble_to_global_array("dofs1", &gid, &mut nodes, 1.0)
        .unwrap();
    let mut dofs = Array::try_zeros(f.nb_nodes, 1).unwrap();
    f.dm.get_array_per_dofs("dofs1", &nodes, &mut dofs).unwrap();
    for (i, &v) in dofs.as_slice().iter().enumerate() {
        if !f.mesh.node_flag(i as Idx).is_pure_ghost() {
            assert_eq!(v, f.mesh.node_global_id(i as Idx) as f64);
        }
    }
}

#[test]
fn assemble_vector_matches_oracle() {
    let fx = case("dof_manager_hex");
    let mut f = set_up();
    f.register(true, false);
    assert_eq!(
        f.dm.system_size(),
        param(&fx, "system_size").as_i64().unwrap()
    );
    f.dm.zero_residual();
    let d1: Vec<f64> = (0..f.nb_nodes)
        .flat_map(|n| {
            let v = if f.mesh.node_flag(n as Idx).is_local_or_master() {
                1.0
            } else {
                0.0
            };
            [v; 3]
        })
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
    // upstream criterion: |dof − ref| / |ref| < 1e-14 on non-pure-ghost
    // nodes; here exact, and equal to the oracle's arrays bit for bit.
    assert!(a1.as_slice().iter().all(|&x| x == 1.0));
    assert!(a2.as_slice().iter().all(|&x| x == 2.0));
    // the oracle registered dofs2 as nodal too; compare dofs1 + its residual part
    let want = f64s(&fx, "assemble_vector/dofs1");
    assert!(same_bits(a1.as_slice(), &want));
}

#[test]
fn assemble_matrix_nodal_matches_oracle() {
    let fx = case("dof_manager_hex");
    let mut f = set_up();
    f.register(true, true);
    assert_eq!(
        f.dm.residual().values().len() as i64,
        param(&fx, "residual_size_after_dofs2").as_i64().unwrap()
    );
    // AssembleVector with both families nodal (the oracle's registration)
    f.dm.zero_residual();
    let d1: Vec<f64> = (0..f.nb_nodes)
        .flat_map(|n| {
            let v = if f.mesh.node_flag(n as Idx).is_local_or_master() {
                1.0
            } else {
                0.0
            };
            [v; 3]
        })
        .collect();
    let d2 = vec![2.0; f.nb_nodes * 5];
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
    let mut n = assert_bits(&fx, "assemble_vector/residual", &f.dm.residual().values());
    let r = f.dm.residual().values().to_vec();
    let mut a2 = Array::try_zeros(0, 5).unwrap();
    f.dm.get_array_per_dofs("dofs2", &r, &mut a2).unwrap();
    n += assert_bits(&fx, "assemble_vector/dofs2", a2.as_slice());

    f.dm.new_matrix("K", MatrixType::Symmetric).unwrap();
    f.dm.zero_matrix("K").unwrap();
    let ne = f.mesh.nb_element(HEX, GhostType::NotGhost);
    let ones = Array::from_vec(vec![1.0; ne * 24 * 24], 24 * 24).unwrap();
    f.dm.assemble_elemental_matrices_to_matrix(
        &f.mesh,
        "K",
        "dofs1",
        ones.view(),
        HEX,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .unwrap();
    let twos = Array::from_vec(vec![2.0; ne * 40 * 40], 40 * 40).unwrap();
    f.dm.assemble_elemental_matrices_to_matrix(
        &f.mesh,
        "K",
        "dofs2",
        twos.view(),
        HEX,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .unwrap();
    n += assert_matrix(&fx, "matrix/K", f.dm.matrix("K").unwrap());
    // assembleMatMulVectToArray(dofs1, K, x, y, 1.5)
    let x = f64s(&fx, "matmul/x");
    let mut y = Array::try_zeros(f.nb_nodes, 3).unwrap();
    f.dm.assemble_mat_mul_vect_to_array("dofs1", "K", &x, &mut y, 1.5)
        .unwrap();
    n += assert_bits(&fx, "matmul/y", y.as_slice());
    println!("dof_manager_hex: {n} values bit-identical");
}

/// `test_dof_mesh_distribute`: a manager whose DOFs were numbered on the
/// undistributed mesh rejects the distribution event; a manager built
/// after the distribution accepts it.
#[test]
fn test_dof_mesh_distribute() {
    let mut mesh1 = mesh();
    let mut dm1 = DofManager::new("test_dof_manager_1__failing", Communicator::serial());
    dm1.register_nodal_dofs(&mesh1, "u", 3).unwrap();
    MeshAccessor::new(&mut mesh1).set_distributed();
    let ev = MeshEvent::MeshIsDistributed {
        origin: "test".into(),
    };
    // The same refusal in the capability check, before
    // the mesh commit (recoverable through `SharedMesh::mutate`)
    let check = dm1.check_mesh_event(&mesh1, &ev);
    assert!(check.is_err(), "{check:?}");
    assert!(dm1.on_mesh_event(&mesh1, &ev).is_err());

    let mut mesh2 = mesh();
    MeshAccessor::new(&mut mesh2).set_distributed();
    let mut dm2 = DofManager::new("test_dof_manager_2__succeeding", Communicator::serial());
    dm2.register_nodal_dofs(&mesh2, "u", 3).unwrap();
    assert!(dm2.check_mesh_event(&mesh2, &ev).is_ok());
    assert!(dm2.on_mesh_event(&mesh2, &ev).is_ok());
}
