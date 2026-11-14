// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_matrix_profile_parallel.cc
// (+ triangle.geo; the test is not registered upstream and uses a removed
// API: `SparseMatrix(mesh, _symmetric, 2)`, `buildProfile`, `SolverMumps`).
//! Same-intent port of upstream `test_sparse_matrix_profile_parallel`
//! (TEST-229): the profile of a symmetric matrix of a 2-component nodal DOF
//! family, built from the element connectivity of a mesh distributed over
//! several ranks, is the profile of the undistributed mesh. Mesh: the frozen
//! `test/test_solver/triangle.msh` (Gmsh 4.13.1, D-009; upstream partitions
//! it with Scotch, here recursive coordinate bisection), read on rank 0 and
//! distributed on the in-process local world at 1, 2 and 4 ranks. On every
//! rank the profile is built by assembling zero element matrices of the
//! local elements (upstream `buildProfile` declares the same entries); each
//! stored entry `(i, j)` (global equations) is translated to
//! `((global node, component), (global node, component))` with the rank's
//! numbering. On every rank the symmetric matrix must store each unordered
//! pair once (upstream `SparseMatrixAIJ::add` keys a symmetric entry by
//! `(min, max)`, so neither the transposed entry nor a duplicate is ever
//! added): the number of stored entries equals the number of distinct
//! normalised pairs. The union over the ranks must equal the serial profile,
//! every rank's entries must belong to it, and the serial profile must be the
//! symmetric triangle of all node pairs of the elements (independent
//! reconstruction from the connectivity). The upstream per-rank
//! `profile_<rank>.mtx` files are not written.

use std::collections::BTreeSet;
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::{ElementType, GhostType, GlobalIdx, Idx};
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_solver::DofManager;
use akantu_validation::fixtures_root;

const NB_DOF: usize = 2;
const TY: ElementType = ElementType::Triangle3;

/// A DOF identified independently of the numbering: (global node, component).
type NodeDof = (GlobalIdx, usize);
type Entry = (NodeDof, NodeDof);

/// Normalised entry of a symmetric profile (upper triangle in node-DOF order).
fn sym(a: NodeDof, b: NodeDof) -> Entry {
    if a <= b { (a, b) } else { (b, a) }
}

/// The profile of this rank, translated to node DOFs, plus (on a single
/// rank) the profile expected from the connectivity.
fn scenario(comm: Communicator) -> Result<(BTreeSet<Entry>, Option<BTreeSet<Entry>>), String> {
    let e = |e: akantu_core::Error| e.to_string();
    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).map_err(e)?;
    if comm.rank() == 0 {
        let path = fixtures_root().join("meshes/gmsh-4.13.1/test/test_solver/triangle.msh");
        akantu_io::read_mesh(&mut mesh, &path, MeshIoType::Gmsh).map_err(e)?;
    }
    mesh.compute_bounding_box().map_err(e)?;
    let mut partitioner = RcbPartitioner;
    let _ = distribute_with(
        &mut mesh,
        Some(&mut partitioner as &mut dyn MeshPartitioner),
        0,
    )
    .map_err(e)?;

    let mut dm = DofManager::new("profile", comm.clone());
    dm.register_nodal_dofs(&mesh, "displacement", NB_DOF)
        .map_err(e)?;
    dm.new_matrix("A", MatrixType::Symmetric).map_err(e)?;

    // buildProfile: every local element contributes its (zero) block
    let nb_element = mesh.nb_element(TY, GhostType::NotGhost);
    let nnpe = TY.nb_nodes_per_element();
    let block = (nnpe * NB_DOF) * (nnpe * NB_DOF);
    let zeros = akantu_core::Array::<f64>::zeros(nb_element, block);
    dm.assemble_elemental_matrices_to_matrix(
        &mesh,
        "A",
        "displacement",
        zeros.view(),
        TY,
        GhostType::NotGhost,
        MatrixType::Symmetric,
        None,
    )
    .map_err(e)?;

    // global equation → node DOF with this rank's numbering
    let equations = dm.local_equation_numbers("displacement").map_err(e)?;
    let layout = dm.layout();
    let mut of_global = std::collections::BTreeMap::new();
    for (i, &eq) in equations.iter().enumerate() {
        let eq = usize::try_from(eq).map_err(|x| x.to_string())?;
        if let Some(g) = layout.global(eq) {
            let node = Idx::try_from(i / NB_DOF).map_err(|x| x.to_string())?;
            of_global.insert(g, (mesh.node_global_id(node), i % NB_DOF));
        }
    }
    let a = dm.matrix("A").map_err(e)?;
    if a.matrix_type() != MatrixType::Symmetric {
        return Err("the matrix is not symmetric".into());
    }
    let (irn, jcn, _) = a.coo();
    let mut profile = BTreeSet::new();
    for (&i, &j) in irn.iter().zip(jcn) {
        let (Some(&di), Some(&dj)) = (of_global.get(&i), of_global.get(&j)) else {
            return Err(format!(
                "rank {}: entry ({i}, {j}) outside the local DOFs",
                comm.rank()
            ));
        };
        profile.insert(sym(di, dj));
    }
    // one stored entry per unordered pair (no transposed copy, no duplicate)
    if irn.len() != profile.len() {
        return Err(format!(
            "rank {}: {} stored entries for {} distinct symmetric pairs",
            comm.rank(),
            irn.len(),
            profile.len()
        ));
    }

    // independent reconstruction on one rank
    let expected = (comm.size() == 1).then(|| {
        let mut set = BTreeSet::new();
        let conn = mesh.connectivity(TY, GhostType::NotGhost).unwrap();
        for el in conn.as_slice().chunks_exact(nnpe) {
            for &na in el {
                for &nb in el {
                    for da in 0..NB_DOF {
                        for db in 0..NB_DOF {
                            set.insert(sym(
                                (mesh.node_global_id(na), da),
                                (mesh.node_global_id(nb), db),
                            ));
                        }
                    }
                }
            }
        }
        set
    });
    Ok((profile, expected))
}

#[test]
fn test_sparse_matrix_profile_parallel() {
    let timeout = Duration::from_secs(120);
    let (serial, expected) = run_local(1, timeout, scenario)
        .pop()
        .unwrap()
        .unwrap_or_else(|e| panic!("serial: {e}"));
    let expected = expected.unwrap();
    assert!(!serial.is_empty());
    assert_eq!(
        serial, expected,
        "serial profile vs the connectivity's node pairs"
    );
    for n in [2, 4] {
        let mut union = BTreeSet::new();
        for (r, o) in run_local(n, timeout, scenario).into_iter().enumerate() {
            let (p, _) = o.unwrap_or_else(|e| panic!("{n} ranks, rank {r}: {e}"));
            assert!(!p.is_empty(), "{n} ranks, rank {r}: empty profile");
            assert!(
                p.is_subset(&serial),
                "{n} ranks, rank {r}: entries outside the serial profile"
            );
            union.extend(p);
        }
        assert_eq!(union, serial, "{n} ranks: union of the rank profiles");
    }
    eprintln!("profile: {} symmetric entries", serial.len());
}
