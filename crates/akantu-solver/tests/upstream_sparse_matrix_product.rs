// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_matrix_product.cc
// (+ test_sparse_matrix_product.verified, bar.msh); the printout format is
// that of src/common/aka_array_tmpl.hh (`Array::printself`, debug level
// `dblTest`) and src/common/aka_common_inline_impl.hh (`printMemorySize`).
//! Full-intent port of upstream `test_sparse_matrix_product` (TEST-157;
//! the matrix-only checks are in `akantu-linalg/tests/upstream_solver_tests.rs`):
//! `bar.msh` (121 nodes, the copy kept with the mesh fixtures) read on the
//! root and distributed, a nodal DOF family of 2 components registered in a
//! DOF manager, a symmetric matrix `A` from the manager with `A(i, i) = 2`
//! for every local-or-master DOF, `x = 1`, `x = A x`, and the result
//! gathered on rank 0.
//!
//! * Serial (upstream's registration: one rank): the program's standard
//!   output — the four progress lines and the `Array<double>` printout of
//!   the result — is compared with `test_sparse_matrix_product.verified` as
//!   the upstream driver compares it (`diff -biw`: case and whitespace
//!   ignored, line by line; the driver's timing-line filter matches no line
//!   here).
//! * 2 and 4 ranks (upstream's `psize > 1` branch, not registered upstream)
//!   on the in-process local world, the mesh distributed by recursive
//!   coordinate bisection: the gathered result holds 121 × {2, 2}.

use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use akantu_core::enums::{MatrixType, MeshIoType};
use akantu_core::{Idx, Real};
use akantu_linalg::TermsToAssemble;
use akantu_mesh::Mesh;
use akantu_mesh::distribution::{MeshPartitioner, RcbPartitioner, distribute_with};
use akantu_solver::DofManager;
use akantu_solver::dof_synchronizer::{DofEntities, build_dof_synchronizer};
use akantu_solver::format::c_general;
use akantu_validation::fixtures_root;

const NB_DOF: usize = 2;

/// Upstream `printMemorySize<T>(size)` for `T = double`: the byte size is
/// multiplied by `sizeof(T)` once more (upstream quirk, kept: it is what
/// the `.verified` file shows), scaled by the largest power of 1024 and
/// printed `std::fixed` with 2 decimals.
fn print_memory_size_f64(memory_size: usize) -> String {
    let real_size = (memory_size * 8) as Real;
    let mult = if real_size != 0.0 {
        ((real_size.ln() / 2f64.ln()) / 10.0) as u32
    } else {
        0
    };
    let scaled = real_size / Real::from(1u32 << (10 * mult));
    let prefix = ["", "Ki", "Mi", "Gi", "Ti", "Pi", "Ei", "Zi", "Yi"][mult as usize];
    format!("{scaled:.2}{prefix}Byte")
}

/// Upstream `Array<double>::printself` at debug level `dblTest` (no
/// address line, values printed with stream precision 2).
fn print_array(id: &str, values: &[Real], nb_component: usize) -> String {
    let size = values.len() / nb_component;
    let mut s = String::from("Array<double> [\n");
    s += &format!(" + id             : {id}\n");
    s += &format!(" + size           : {size}\n");
    s += &format!(" + nb_component   : {nb_component}\n");
    s += &format!(" + allocated size : {size}\n");
    s += &format!(
        " + memory size    : {}\n",
        print_memory_size_f64(size * nb_component * 8)
    );
    let tuples: Vec<String> = values
        .chunks_exact(nb_component)
        .map(|t| {
            let v: Vec<String> = t.iter().map(|&x| c_general(x, 2)).collect();
            format!("{{{}}}", v.join(", "))
        })
        .collect();
    s += &format!(" + values         : {{{}}}\n", tuples.join(", "));
    s += "]\n";
    s
}

/// `diff -biw` equality: lines compared without whitespace, case folded.
fn same_text_ignoring_whitespace(a: &str, b: &str) -> bool {
    let norm = |s: &str| -> Vec<String> {
        s.lines()
            .map(|l| {
                l.chars()
                    .filter(|c| !c.is_whitespace())
                    .collect::<String>()
                    .to_lowercase()
            })
            .collect()
    };
    norm(a) == norm(b)
}

/// The upstream program on `comm`; returns its standard output on rank 0.
fn scenario(comm: Communicator) -> Result<Option<String>, String> {
    let e = |e: akantu_core::Error| e.to_string();
    let prank = comm.rank();
    let psize = comm.size();
    let mut out = String::new();

    let mut mesh = Mesh::with_communicator(2, "mesh", comm.clone()).map_err(e)?;
    if prank == 0 {
        let path = fixtures_root().join("mesh_io/test_solver/bar/inputs/bar.msh");
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

    let nb_nodes = mesh.nb_nodes();
    let mut dof_manager = DofManager::new("test_dof_manager", comm.clone());
    dof_manager
        .register_nodal_dofs(&mesh, "test_synchronize", NB_DOF)
        .map_err(e)?;

    if prank == 0 {
        out += "Creating a SparseMatrix\n";
    }
    dof_manager
        .new_matrix("A", MatrixType::Symmetric)
        .map_err(e)?;

    if prank == 0 {
        out += "Filling the matrix\n";
    }
    let equations = dof_manager
        .local_equation_numbers("test_synchronize")
        .map_err(e)?
        .to_vec();
    let layout = dof_manager.layout().clone();
    let mut terms = TermsToAssemble::new("test_synchronize", "test_synchronize");
    for (i, &eq) in equations.iter().enumerate() {
        let eq = usize::try_from(eq).map_err(|x| x.to_string())?;
        if layout.is_owned(eq) {
            let i = Idx::try_from(i).map_err(|x| x.to_string())?;
            terms.term(i, i).value = 2.;
        }
    }
    dof_manager
        .assemble_preassembled_matrix("A", &terms)
        .map_err(e)?;

    // dof_vector = 1 (per DOF), in local equation order for the product
    let n_local = layout.nb_local();
    let x = vec![1.0; n_local];
    let mut y = vec![0.0; n_local];
    if prank == 0 {
        out += "Computing x = A * x\n";
    }
    dof_manager
        .mat_vec(&mesh, "A", &x, &mut y, 1.0, 0.0)
        .map_err(e)?;

    if prank == 0 {
        out += "Gathering the results on proc 0\n";
    }
    if psize > 1 {
        let sync = build_dof_synchronizer(&dof_manager, &mesh).map_err(e)?;
        let entities = DofEntities { dm: &dof_manager };
        let gathered = sync.gather(&entities, &y, 1).map_err(e)?;
        if prank == 0 {
            let g = gathered.ok_or("no gathered array on the root")?;
            if g.len() != 121 * NB_DOF || g.iter().any(|&v| v != 2.0) {
                return Err(format!("gathered result: {g:?}"));
            }
            out += &print_array("", &g, NB_DOF);
        }
    } else {
        // back to DOF (node, component) order
        let mut dof_vector = vec![0.0; nb_nodes * NB_DOF];
        for (i, &eq) in equations.iter().enumerate() {
            dof_vector[i] = y[usize::try_from(eq).map_err(|x| x.to_string())?];
        }
        out += &print_array("vector", &dof_vector, NB_DOF);
    }
    // `std::cout << array << std::endl`: printself ends with a newline
    out += "\n";
    Ok((prank == 0).then_some(out))
}

/// Serial run vs `test_sparse_matrix_product.verified`.
#[test]
fn test_sparse_matrix_product() {
    let results = run_local(1, Duration::from_secs(60), scenario);
    let out = results.into_iter().next().unwrap().unwrap().unwrap();
    let verified = include_str!("data/upstream/test_sparse_matrix_product.verified");
    assert!(
        same_text_ignoring_whitespace(&out, verified),
        "output differs from test_sparse_matrix_product.verified:\n{out}"
    );
    assert!(out.contains(" + memory size    : 15.12KiByte"));
}

/// Upstream's `psize > 1` branch on 2 and 4 ranks (gathered values).
#[test]
fn test_sparse_matrix_product_distributed() {
    for n in [2, 4] {
        for (r, o) in run_local(n, Duration::from_secs(60), scenario)
            .into_iter()
            .enumerate()
        {
            let out = o.unwrap_or_else(|e| panic!("{n} ranks, rank {r}: {e}"));
            assert_eq!(out.is_some(), r == 0);
        }
    }
}
