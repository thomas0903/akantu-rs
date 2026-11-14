// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_matrix_profile.cc,
// test/test_solver/test_sparse_matrix_assemble.cc,
// test/test_solver/test_sparse_solver_eigen.cc (+ triangle.geo, the
// genMesh helper) and src/solver/sparse_solver_eigen.cc (the parallel
// assertion of the constructor), at 35a851780d0b9471d258ed53a003ae98aaa95ea0.

//! Full-intent ports of three upstream
//! solver tests whose earlier ports were matrix-only
//! (`akantu-linalg/tests/upstream_solver_tests.rs`, kept): like upstream,
//! the matrix comes from a DOF manager whose nodal DOFs are registered on a
//! mesh, so its size is the mesh's DOF count, not a constant.
//!
//! * TEST-155 `test_sparse_matrix_profile` (serial, `PACKAGE implicit`):
//!   `triangle.msh` (the frozen Gmsh 4.13.1 mesh of upstream's
//!   `triangle.geo`, order 1: 513 nodes), a nodal family of 2 components,
//!   `getNewMatrix("A", _symmetric)`, profile `add(i, i)` (i < 10) and
//!   `add(0, 9)`, `saveProfile`, then the values `10 i` and `100`,
//!   `saveMatrix` — the Matrix Market texts of upstream's `SparseMatrixAIJ`.
//! * TEST-156 `test_sparse_matrix_assemble` (serial): the same set-up; the
//!   assembly loop is commented out upstream (UD-181), so the saved matrix
//!   is the empty symmetric matrix of the system size.
//! * TEST-159 `test_sparse_solver_eigen` (`PACKAGE mumps`, `PARALLEL`:
//!   1/2/4 ranks here on the in-process local world): an 11-node segment
//!   mesh generated on rank 0 (`genMesh`) and distributed, a nodal family
//!   `x`, `A` pre-assembled through `TermsToAssemble` with
//!   `A(i, i) = 1/(1 + g)` for every local-or-master DOF of global equation
//!   `g`, `b = 1` assembled to the residual, `SparseSolverEigen(dof_manager,
//!   "A").solve()`: serially the solution is `1 + g` within upstream's
//!   relative `1e-15`; on more than one rank upstream's constructor asserts
//!   `communicator.getNbProc() == 1` ("This solver does not work in
//!   parallel", active in the oracle build) and the port's
//!   `SparseSolverFactory::create(Eigen, nb_proc)` refuses likewise
//!   (D-033) — asserted on every rank, after checking the distributed
//!   pre-assembled diagonal.

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::array::ArrayView;
use akantu_core::enums::{MatrixType, MeshIoType, SparseSolverType};
use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_linalg::{SparseSolverFactory, TermsToAssemble};
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::DofManager;
use akantu_validation::fixtures_root;

/// `mesh.read("triangle.msh")` (serial) and a DOF manager with the nodal
/// family `test_synchronize` of 2 components, as both upstream tests.
fn triangle_dof_manager() -> (Mesh, DofManager) {
    let comm = Communicator::serial();
    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).unwrap();
    let path = fixtures_root().join("meshes/gmsh-4.13.1/test/test_solver/triangle.msh");
    akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).unwrap();
    assert_eq!(mesh.nb_nodes(), 513, "order-1 mesh of triangle.geo");
    let mut dm = DofManager::new("test_dof_manager", comm);
    dm.register_nodal_dofs(&mesh, "test_synchronize", 2)
        .unwrap();
    (mesh, dm)
}

/// TEST-155: `test_sparse_matrix_profile.cc`.
#[test]
fn test_sparse_matrix_profile() {
    let (_mesh, mut dm) = triangle_dof_manager();
    let n = dm.system_size();
    assert_eq!(n, 1026, "513 nodes x 2 components");
    dm.new_matrix("A", MatrixType::Symmetric).unwrap();
    let a = dm.backend_mut().unwrap().aij_mut("A").unwrap();
    for i in 0..10 {
        a.add_to_profile(i, i).unwrap();
    }
    a.add_to_profile(0, 9).unwrap();
    let mut profile = format!("%%MatrixMarket matrix coordinate pattern symmetric\n{n} {n} 11\n");
    for i in 1..=10 {
        profile.push_str(&format!("{i} {i} 1\n"));
    }
    profile.push_str("1 10 1\n");
    assert_eq!(a.profile_text(), profile);
    for i in 0..10 {
        a.add(i, i, (i * 10) as Real).unwrap();
    }
    a.add(0, 9, 100.0).unwrap();
    let mut matrix = format!("%%MatrixMarket matrix coordinate real symmetric\n{n} {n} 11\n");
    for i in 1..=10 {
        matrix.push_str(&format!("{i} {i} {}\n", (i - 1) * 10));
    }
    matrix.push_str("1 10 100\n");
    assert_eq!(a.matrix_market_text(), matrix);
    // upstream writes profile_hand.mtx and matrix_hand.mtx
    let dir = std::env::temp_dir().join(format!(
        "akantu-solver-profile-full-intent-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    a.save_profile(&dir.join("profile_hand.mtx")).unwrap();
    let written = a.save_matrix(&dir.join("matrix_hand.mtx")).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("profile_hand.mtx")).unwrap(),
        profile
    );
    assert_eq!(std::fs::read_to_string(&written).unwrap(), matrix);
    std::fs::remove_dir_all(&dir).unwrap();
}

/// TEST-156: `test_sparse_matrix_assemble.cc` (assembly loop commented out
/// upstream, UD-181: the real assembly is `akantu-linalg/tests/aij_oracle.rs`).
#[test]
fn test_sparse_matrix_assemble() {
    let (_mesh, mut dm) = triangle_dof_manager();
    dm.new_matrix("A", MatrixType::Symmetric).unwrap();
    let a = dm.matrix("A").unwrap();
    assert_eq!(
        a.matrix_market_text(),
        "%%MatrixMarket matrix coordinate real symmetric\n1026 1026 0\n"
    );
    let dir = std::env::temp_dir().join(format!(
        "akantu-solver-assemble-full-intent-{}",
        std::process::id()
    ));
    std::fs::create_dir_all(&dir).unwrap();
    let written = a.save_matrix(&dir.join("matrix.mtx")).unwrap();
    assert_eq!(
        std::fs::read_to_string(&written).unwrap(),
        a.matrix_market_text()
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

const NB_GLOBAL_DOF: usize = 11;

/// Upstream `genMesh(mesh, nb_nodes)`: nodes `n / (nb_nodes - 1)` on
/// `[0, 1]`, segments `(n, n + 1)`.
fn gen_mesh(mesh: &mut Mesh, nb_nodes: usize) {
    let mut a = MeshAccessor::new(mesh);
    for n in 0..nb_nodes {
        a.push_node(&[n as Real * (1. / (nb_nodes - 1) as Real)])
            .unwrap();
    }
    let mut conn = a.connectivity_mut(ElementType::Segment2, GhostType::NotGhost);
    for n in 0..nb_nodes - 1 {
        conn.push_tuple(&[n as Idx, (n + 1) as Idx]);
    }
}

/// The program of `test_sparse_solver_eigen.cc` on `comm`.
fn sparse_solver_eigen(comm: Communicator) -> Result<(), String> {
    let e = |e: akantu_core::Error| e.to_string();
    let psize = comm.size();
    let mut mesh = Mesh::with_communicator(1, "mesh", comm.clone()).map_err(e)?;
    if comm.rank() == 0 {
        gen_mesh(&mut mesh, NB_GLOBAL_DOF);
    }
    mesh.compute_bounding_box().map_err(e)?;
    let mut partitioner = RcbPartitioner;
    let _ = distribute_with(
        &mut mesh,
        Some(&mut partitioner as &mut dyn MeshPartitioner),
        0,
    )
    .map_err(e)?;
    let nb_nodes = mesh.nb_nodes();
    if psize > 1 && nb_nodes >= NB_GLOBAL_DOF {
        return Err(format!("rank {} holds all {nb_nodes} nodes", comm.rank()));
    }

    let mut dm = DofManager::new("test_dof_manager", comm.clone());
    dm.register_nodal_dofs(&mesh, "x", 1).map_err(e)?;
    if dm.system_size() != NB_GLOBAL_DOF as akantu_core::GlobalIdx {
        return Err(format!("system size {}", dm.system_size()));
    }
    let equations = dm.local_equation_numbers("x").map_err(e)?.to_vec();
    let layout = dm.layout().clone();
    dm.new_matrix("A", MatrixType::Symmetric).map_err(e)?;
    let mut terms = TermsToAssemble::new("x", "x");
    let global_of = |i: usize| -> Result<(usize, Real), String> {
        let li = usize::try_from(equations[i]).map_err(|x| x.to_string())?;
        let gi = layout.local_to_global()[li];
        Ok((li, gi as Real))
    };
    for i in 0..nb_nodes {
        let (li, gi) = global_of(i)?;
        if layout.is_owned(li) {
            let i = Idx::try_from(i).map_err(|x| x.to_string())?;
            terms.term(i, i).value = 1. / (1. + gi);
        }
    }
    dm.assemble_preassembled_matrix("A", &terms).map_err(e)?;
    // the pre-assembled diagonal on the owned rows, 1 / (1 + g)
    {
        let a = dm.matrix("A").map_err(e)?;
        for i in 0..nb_nodes {
            let (li, gi) = global_of(i)?;
            if layout.is_owned(li) {
                let g = layout.local_to_global()[li];
                let v = akantu_linalg::SparseMatrix::value(a, g, g).map_err(e)?;
                if v != 1. / (1. + gi) {
                    return Err(format!("A({g}, {g}) = {v}"));
                }
            }
        }
    }
    let b = vec![1.0; nb_nodes];
    dm.assemble_to_residual("x", ArrayView::new(&b, 1).map_err(e)?, 1.0)
        .map_err(e)?;

    // SparseSolverEigen solver(dof_manager, "A")
    let factory = SparseSolverFactory::default();
    let solver = factory.create(SparseSolverType::Eigen, psize);
    if psize > 1 {
        // upstream: AKANTU_DEBUG_ASSERT(communicator.getNbProc() == 1,
        // "This solver does not work in parallel")
        return match solver {
            Err(akantu_core::Error::Unsupported(m)) if m.contains("does not work in parallel") => {
                Ok(())
            }
            Err(other) => Err(format!("unexpected refusal: {other}")),
            Ok(_) => Err("the Eigen solver was accepted on more than one rank".into()),
        };
    }
    let mut s = solver.map_err(e)?;
    let rhs = dm.residual().values().to_vec();
    let mut x = vec![0.0; rhs.len()];
    {
        let a = dm.matrix("A").map_err(e)?;
        s.analysis(a).map_err(e)?;
        s.factorize(a).map_err(e)?;
        s.solve(&rhs, &mut x).map_err(e)?;
    }
    dm.set_solution(&x).map_err(e)?;
    let y = dm.dofs_solution("x").map_err(e)?;
    // upstream's check: the n-th value is n + 1 within 1e-15 relative
    for (n, &yn) in y.as_slice().iter().enumerate() {
        let d = (n + 1) as Real;
        if (yn - d).abs() / d > 1e-15 {
            return Err(format!("Error in the solution: {yn} != {d}"));
        }
    }
    Ok(())
}

/// TEST-159 on 1, 2 and 4 ranks (upstream `PARALLEL` registration).
#[test]
fn test_sparse_solver_eigen() {
    for n in [1, 2, 4] {
        for (r, res) in run_local(n, Duration::from_secs(60), sparse_solver_eigen)
            .into_iter()
            .enumerate()
        {
            res.unwrap_or_else(|m| panic!("{n} ranks, rank {r}: {m}"));
        }
    }
}
