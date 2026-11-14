// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_model/test_common/
// test_dof_manager.cc (typed suite DOFManagerFixture, instance
// `dof_manager_petsc_t`: DOFManagerFactory::allocate("petsc", mesh,
// "dof_manager")), on the frozen mesh.msh (hexahedra).

//! DOF-015, TEST-070: the PETSc instance of upstream's typed
//! `test_dof_manager` suite, at 1 / 2 / 4 ranks (`mpirun`, real MPI).
//!
//! **What upstream's PETSc instance does** (read from the reference; the
//! oracle's per-rank gtest reports show it passing on every
//! rank at 2 / 4 ranks while the default instance fails on the ranks > 0,
//! UD-871). Its vectors are ghosted PETSc vectors (`VECMPI` +
//! `VecMPISetGhost` over the non-owned local equations,
//! `dof_manager_petsc.cc:198-213`). `assembleToGlobalArray` /
//! `assembleToResidual` add into the **local form** with
//! `VecSetValuesLocal` (`solver_vector_petsc.cc:206-230`), so a slave
//! node's contribution stays in its ghost slot (no reverse scatter: it never
//! reaches the owner), and `getArrayPerDOFs` → `getValuesLocal` first runs
//! `VecGhostUpdate(INSERT_VALUES, SCATTER_FORWARD)` (`:122-125, 145-187`):
//! every ghost slot is overwritten with the owner's value. The tests fill
//! only the local-or-master nodes (slaves 0), so after the forward scatter a
//! slave reads its owner's value — the global id, or 1 — and upstream's
//! expectations hold everywhere. The default manager reads local values
//! (no exchange), hence UD-871.
//!
//! **The port.** The PETSc DOF manager keeps the DOF manager's
//! local-equation vectors (no ghosted PETSc vectors; ghost
//! contributions are reduced to the owners in the solves, where upstream's
//! PETSc manager drops them), so `assemble_to_global_array` /
//! `get_array_per_dofs` are the default manager's local operations on both
//! back-ends. This scenario checks every upstream expectation and asserts:
//! every registration size as upstream; `splitArrayPerDOFs` and
//! `AssembleVector` give exactly the default manager's values, i.e. on rank
//! 0 (and at one rank) upstream's expectations hold, on every rank > 0 they
//! fail exactly at the rank's slave nodes (which read their local 0) —
//! where upstream's PETSc instance passes: the documented difference
//! UD-1021; `AssembleMatrixNodal` assembles on every rank and the PETSc
//! back-end's assembly store gives the default manager's product bit for
//! bit (upstream only saves the matrix).

use akantu_comm::{Communicator, ReduceOp};
use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_solver::DofManager;
use akantu_validation::fixtures_root;

use crate::{Outcome, e2s};

const HEX: ElementType = ElementType::Hexahedron8;

/// Rank 0 reads `mesh.msh`; the mesh is distributed over `c` (upstream
/// `if (prank == 0) mesh->read("mesh.msh"); mesh->distribute();`).
fn mesh_on(c: &Communicator) -> Result<Mesh, String> {
    let path = fixtures_root().join("meshes/gmsh-4.13.1/test/test_model/test_common/mesh.msh");
    let mut mesh = Mesh::with_communicator(3, "mesh", c.clone()).map_err(e2s)?;
    if c.rank() == 0 {
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).map_err(e2s)?;
    }
    if c.size() > 1 {
        let mut p = RcbPartitioner;
        // No component exists yet: the events need no dispatch.
        let _events =
            distribute_with(&mut mesh, Some(&mut p as &mut dyn MeshPartitioner), 0).map_err(e2s)?;
    }
    Ok(mesh)
}

fn idx(n: usize) -> Idx {
    Idx::try_from(n).unwrap_or(Idx::MAX)
}

/// Upstream `DOFManagerFactory::allocate(type, mesh, "dof_manager")`.
fn alloc(kind: &str, c: &Communicator) -> Result<DofManager, String> {
    DofManager::of_type(kind, "dof_manager", c.clone()).map_err(e2s)
}

struct Counts {
    nb_nodes: usize,
    nb_total: i64,
    nb_pure_local: usize,
}

fn counts(mesh: &Mesh) -> Counts {
    let nb_nodes = mesh.nb_nodes();
    Counts {
        nb_nodes,
        nb_total: i64::try_from(mesh.nb_global_nodes()).unwrap_or(i64::MAX),
        nb_pure_local: (0..nb_nodes)
            .filter(|&n| mesh.node_flag(idx(n)).is_local_or_master())
            .count(),
    }
}

/// `registerDOFs(dst1, dst2)` with upstream's size checks
/// (`getResidual().size()` is the global system size).
fn register(dm: &mut DofManager, mesh: &Mesh, k: &Counts, nodal1: bool, nodal2: bool) -> Outcome {
    if nodal1 {
        dm.register_nodal_dofs(mesh, "dofs1", 3).map_err(e2s)?;
    } else {
        dm.register_generic_dofs("dofs1", k.nb_pure_local, 3)
            .map_err(e2s)?;
    }
    if dm.system_size() != k.nb_total * 3 {
        return Err(format!(
            "dofs1: system size {} != {}",
            dm.system_size(),
            k.nb_total * 3
        ));
    }
    if nodal2 {
        dm.register_nodal_dofs(mesh, "dofs2", 5).map_err(e2s)?;
    } else {
        dm.register_generic_dofs("dofs2", k.nb_pure_local, 5)
            .map_err(e2s)?;
    }
    if dm.system_size() != k.nb_total * 8 {
        return Err(format!(
            "dofs2: system size {} != {}",
            dm.system_size(),
            k.nb_total * 8
        ));
    }
    Ok(())
}

/// The not-pure-ghost nodes (where upstream checks values), split into
/// (local-or-master, slave).
fn checked(mesh: &Mesh, k: &Counts) -> (Vec<usize>, Vec<usize>) {
    (0..k.nb_nodes)
        .filter(|&n| !mesh.node_flag(idx(n)).is_pure_ghost())
        .partition(|&n| mesh.node_flag(idx(n)).is_local_or_master())
}

/// The port's verdict of a value check (UD-1021): no failed expectation at
/// one rank or on rank 0; on every other rank failed expectations exactly
/// at its slave nodes (at least one).
fn verdict(c: &Communicator, what: &str, failed: &[usize], slaves: &[usize]) -> Outcome {
    if failed != slaves {
        return Err(format!(
            "{what}: failed expectations at {failed:?}, slave nodes {slaves:?}"
        ));
    }
    match (c.rank(), failed.len()) {
        (_, 0) if c.size() == 1 || c.rank() == 0 => Ok(()),
        (0, n) => Err(format!("{what}: rank 0 fails {n} expectations")),
        (_, 0) => Err(format!("{what}: a rank > 0 without slave nodes")),
        _ => Ok(()),
    }
}

#[allow(clippy::cast_precision_loss)] // global ids of a small mesh
fn gid(mesh: &Mesh, n: usize) -> Real {
    mesh.node_global_id(idx(n)) as Real
}

/// `splitArrayPerDOFs` on a manager of `kind`: the extracted array.
fn split_array(kind: &str, c: &Communicator, mesh: &Mesh, k: &Counts) -> Result<Vec<Real>, String> {
    let mut dm = alloc(kind, c)?;
    dm.register_nodal_dofs(mesh, "dofs1", 1).map_err(e2s)?;
    let dofs: Vec<Real> = (0..k.nb_nodes)
        .map(|n| {
            if mesh.node_flag(idx(n)).is_local_or_master() {
                gid(mesh, n)
            } else {
                0.0
            }
        })
        .collect();
    dm.new_lumped_matrix("nodes").map_err(e2s)?;
    let mut nodes = std::mem::take(&mut dm.lumped_matrix_mut("nodes").map_err(e2s)?.values);
    dm.assemble_to_global_array("dofs1", &dofs, &mut nodes, 1.0)
        .map_err(e2s)?;
    let mut out = Array::try_zeros(k.nb_nodes, 1).map_err(e2s)?;
    dm.get_array_per_dofs("dofs1", &nodes, &mut out)
        .map_err(e2s)?;
    Ok(out.as_slice().to_vec())
}

/// `AssembleVector` on a manager of `kind`: `(dofs1, dofs2)` read back
/// from the residual.
fn assemble_vector(
    kind: &str,
    c: &Communicator,
    mesh: &Mesh,
    k: &Counts,
) -> Result<(Vec<Real>, Vec<Real>), String> {
    let mut dm = alloc(kind, c)?;
    register(&mut dm, mesh, k, true, false)?;
    dm.zero_residual();
    let d1: Vec<Real> = (0..k.nb_nodes)
        .flat_map(|n| [Real::from(u8::from(mesh.node_flag(idx(n)).is_local_or_master())); 3])
        .collect();
    let d2 = vec![2.0; k.nb_pure_local * 5];
    dm.assemble_to_residual(
        "dofs1",
        akantu_core::array::ArrayView::new(&d1, 3).map_err(e2s)?,
        1.0,
    )
    .map_err(e2s)?;
    dm.assemble_to_residual(
        "dofs2",
        akantu_core::array::ArrayView::new(&d2, 5).map_err(e2s)?,
        1.0,
    )
    .map_err(e2s)?;
    let r = dm.residual().values().to_vec();
    let mut a1 = Array::try_zeros(0, 3).map_err(e2s)?;
    let mut a2 = Array::try_zeros(0, 5).map_err(e2s)?;
    dm.get_array_per_dofs("dofs1", &r, &mut a1).map_err(e2s)?;
    dm.get_array_per_dofs("dofs2", &r, &mut a2).map_err(e2s)?;
    Ok((a1.as_slice().to_vec(), a2.as_slice().to_vec()))
}

/// `AssembleMatrixNodal` on a manager of `kind`, then `y = 1.5 · K · x`
/// restricted to `dofs1` (`assembleMatMulVectToArray`, collective).
fn assemble_matrix(
    kind: &str,
    c: &Communicator,
    mesh: &Mesh,
    k: &Counts,
) -> Result<Vec<Real>, String> {
    let mut dm = alloc(kind, c)?;
    register(&mut dm, mesh, k, true, true)?;
    dm.new_matrix("K", MatrixType::Symmetric).map_err(e2s)?;
    dm.zero_matrix("K").map_err(e2s)?;
    let ne = mesh.nb_element(HEX, GhostType::NotGhost);
    let ones = Array::from_vec(vec![1.0; ne * 24 * 24], 24 * 24).map_err(e2s)?;
    let twos = Array::from_vec(vec![2.0; ne * 40 * 40], 40 * 40).map_err(e2s)?;
    for (id, m) in [("dofs1", &ones), ("dofs2", &twos)] {
        dm.assemble_elemental_matrices_to_matrix(
            mesh,
            "K",
            id,
            m.view(),
            HEX,
            GhostType::NotGhost,
            MatrixType::Symmetric,
            None,
        )
        .map_err(|e| format!("{kind}: assemble {id}: {e}"))?;
    }
    dm.finalize_matrix("K").map_err(e2s)?;
    // x: a function of the global node id (consistent on every copy).
    let x: Vec<Real> = (0..k.nb_nodes)
        .flat_map(|n| {
            let g = gid(mesh, n);
            [0.5 + 0.01 * g, -0.25 + 0.02 * g, 1.0 - 0.005 * g]
        })
        .collect();
    let mut y = Array::try_zeros(k.nb_nodes, 3).map_err(e2s)?;
    dm.assemble_mat_mul_vect_to_array("dofs1", "K", &x, &mut y, 1.5)
        .map_err(e2s)?;
    Ok(y.as_slice().to_vec())
}

/// The PETSc instance of `test_dof_manager` (see the module documentation).
pub fn test_dof_manager_petsc(c: &Communicator) -> Outcome {
    let mesh = mesh_on(c)?;
    let k = counts(&mesh);
    let mut fails: Vec<String> = Vec::new();
    let mut check = |name: &str, r: Outcome| {
        if let Err(e) = r {
            fails.push(format!("{name}: {e}"));
        }
    };
    // Construction, DoubleConstruction (the new manager is allocated before
    // the old one is destroyed, as the unique_ptr assignment).
    check(
        "Construction",
        alloc("petsc", c).and_then(|dm| {
            (dm.system_size() == 0)
                .then_some(())
                .ok_or_else(|| "a new manager has equations".into())
        }),
    );
    check(
        "DoubleConstruction",
        alloc("petsc", c).and_then(|first| {
            let second = alloc("petsc", c)?;
            drop(first);
            drop(second);
            Ok(())
        }),
    );
    // RegisterGenericDOF1, RegisterNodalDOF1: EXPECT_GE(size, 3·N).
    check(
        "RegisterDOF1",
        (|| {
            let mut a = alloc("petsc", c)?;
            a.register_generic_dofs("dofs1", k.nb_pure_local, 3)
                .map_err(e2s)?;
            let mut b = alloc("petsc", c)?;
            b.register_nodal_dofs(&mesh, "dofs1", 3).map_err(e2s)?;
            let want = k.nb_total * 3;
            (a.system_size() >= want && b.system_size() >= want)
                .then_some(())
                .ok_or_else(|| format!("sizes {}, {} < {want}", a.system_size(), b.system_size()))
        })(),
    );
    // RegisterGenericDOF2, RegisterNodalDOF2, RegisterMixedDOF.
    for (name, n1, n2) in [
        ("RegisterGenericDOF2", false, false),
        ("RegisterNodalDOF2", true, true),
        ("RegisterMixedDOF", true, false),
    ] {
        check(
            name,
            alloc("petsc", c).and_then(|mut dm| register(&mut dm, &mesh, &k, n1, n2)),
        );
    }
    let (owned, slaves) = checked(&mesh, &k);
    // splitArrayPerDOFs.
    check(
        "splitArrayPerDOFs",
        (|| {
            let p = split_array("petsc", c, &mesh, &k)?;
            let d = split_array("default", c, &mesh, &k)?;
            if p != d {
                return Err("PETSc and default managers differ".into());
            }
            if let Some(&n) = owned.iter().find(|&&n| p[n] != gid(&mesh, n)) {
                return Err(format!("owned node {n}: {} != {}", p[n], gid(&mesh, n)));
            }
            let failed: Vec<usize> = owned
                .iter()
                .chain(&slaves)
                .copied()
                .filter(|&n| p[n] != gid(&mesh, n))
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if slaves.iter().any(|&n| p[n] != 0.0) {
                return Err("a slave node does not read its local 0".into());
            }
            verdict(c, "splitArrayPerDOFs", &failed, &slaves)
        })(),
    );
    // AssembleVector (|dof − ref| / |ref| < 1e-14 upstream; exact here).
    check(
        "AssembleVector",
        (|| {
            let (p1, p2) = assemble_vector("petsc", c, &mesh, &k)?;
            let (d1, d2) = assemble_vector("default", c, &mesh, &k)?;
            if p1 != d1 || p2 != d2 {
                return Err("PETSc and default managers differ".into());
            }
            let failed: Vec<usize> = owned
                .iter()
                .chain(&slaves)
                .copied()
                .filter(|&n| p1[3 * n..3 * n + 3] != [1.0; 3])
                .collect::<std::collections::BTreeSet<_>>()
                .into_iter()
                .collect();
            if slaves.iter().any(|&n| p1[3 * n..3 * n + 3] != [0.0; 3]) {
                return Err("dofs1: a slave node does not read its local 0".into());
            }
            verdict(c, "AssembleVector dofs1", &failed, &slaves)?;
            if p2.iter().any(|&x| x != 2.0) {
                return Err("dofs2: not 2 everywhere".into());
            }
            Ok(())
        })(),
    );
    // AssembleMatrixNodal (+ the product, compared with the default manager).
    check(
        "AssembleMatrixNodal",
        (|| {
            let p = assemble_matrix("petsc", c, &mesh, &k)?;
            let d = assemble_matrix("default", c, &mesh, &k)?;
            let differing = p
                .iter()
                .zip(&d)
                .filter(|(a, b)| a.to_bits() != b.to_bits())
                .count();
            if differing > 0 || p.len() != d.len() {
                return Err(format!(
                    "K·x: {differing} of {} values differ from the default manager",
                    p.len()
                ));
            }
            Ok(())
        })(),
    );
    if c.rank() == 0 {
        println!(
            "INFO test_dof_manager_petsc: ranks={} nodes(rank 0)={} slaves(rank 0)={} global={}",
            c.size(),
            k.nb_nodes,
            slaves.len(),
            k.nb_total
        );
    }
    let mut ok = [u8::from(fails.is_empty())];
    c.all_reduce(&mut ok, ReduceOp::Min).map_err(e2s)?;
    if !fails.is_empty() {
        return Err(fails.join("; "));
    }
    if ok[0] != 1 {
        return Err("failed on another rank".into());
    }
    Ok(())
}
