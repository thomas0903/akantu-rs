// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs performance probe (no upstream counterpart): the repeated
// elemental matrix assembly of DOFManagerDefault::assembleElementalMatricesToMatrix
// (src/model/common/dof_manager/dof_manager_default.cc) on 1, 2, 4 and 8
// threads (the execution policy, D-123).

//! `assembly_scaling [n] [reps] [threads...]`: a structured `n³` cube of
//! hexahedra split into six tetrahedra (the mesh of
//! `akantu-linalg/examples/assembly_threads.rs`), 3 DOFs per node,
//! symmetric 12 × 12 elemental matrices into a symmetric `K`, one filter
//! holding every element. Times one assembly round
//! (`assemble_elemental_matrices_to_matrix` with cached positions; median of
//! `reps` rounds after two rounds that build the profile and store the
//! positions) per thread count, and checks the bits against one thread.

use std::time::Instant;

use akantu_comm::Communicator;
use akantu_core::enums::MatrixType;
use akantu_core::exec::{Exec, ExecPolicy};
use akantu_core::{Array, ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::DofManager;

fn cube(n: usize) -> Mesh {
    let mut m = Mesh::new(3, "cube").expect("mesh");
    {
        let mut a = MeshAccessor::new(&mut m);
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    a.push_node(&[i as Real, j as Real, k as Real])
                        .expect("node");
                }
            }
        }
        let id = |i: usize, j: usize, k: usize| (i + (n + 1) * (j + (n + 1) * k)) as Idx;
        let mut c = a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let v = [
                        id(i, j, k),
                        id(i + 1, j, k),
                        id(i + 1, j + 1, k),
                        id(i, j + 1, k),
                        id(i, j, k + 1),
                        id(i + 1, j, k + 1),
                        id(i + 1, j + 1, k + 1),
                        id(i, j + 1, k + 1),
                    ];
                    for t in [
                        [0, 1, 2, 6],
                        [0, 2, 3, 6],
                        [0, 3, 7, 6],
                        [0, 7, 4, 6],
                        [0, 4, 5, 6],
                        [0, 5, 1, 6],
                    ] {
                        c.push_tuple(&[v[t[0]], v[t[1]], v[t[2]], v[t[3]]]);
                    }
                }
            }
        }
    }
    m
}

fn median(mut v: Vec<f64>) -> f64 {
    v.sort_by(f64::total_cmp);
    v[v.len() / 2]
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    let n: usize = args.get(1).and_then(|s| s.parse().ok()).unwrap_or(30);
    let reps: usize = args.get(2).and_then(|s| s.parse().ok()).unwrap_or(7);
    let threads: Vec<usize> = if args.len() > 3 {
        args[3..].iter().filter_map(|s| s.parse().ok()).collect()
    } else {
        vec![1, 2, 4, 8]
    };
    let mesh = cube(n);
    let ty = ElementType::Tetrahedron4;
    let ne = mesh.nb_element(ty, GhostType::NotGhost);
    let mut x = 0x2545_f491_4f6c_dd1du64;
    let mut ke = Array::new(144);
    let mut b = [0.0; 144];
    for _ in 0..ne {
        for i in 0..12 {
            for j in i..12 {
                x ^= x >> 12;
                x ^= x << 25;
                x ^= x >> 27;
                let v = (x.wrapping_mul(0x9e37_79b9_7f4a_7c15) >> 11) as Real
                    / (1u64 << 53) as Real
                    - 0.5;
                b[i + 12 * j] = v;
                b[j + 12 * i] = v;
            }
        }
        ke.push_tuple(&b);
    }
    let all: Vec<Idx> = (0..ne as Idx).collect();
    let mut reference: Option<Vec<u64>> = None;
    let mut t1 = 0.0;
    println!("n {n}: {ne} tetrahedra");
    for &p in &threads {
        let exec = Exec::new(ExecPolicy::serial().with_threads(p).expect("threads")).expect("exec");
        let mut dm = DofManager::new("dm", Communicator::serial());
        dm.set_exec(exec);
        dm.register_nodal_dofs(&mesh, "u", 3).expect("dofs");
        dm.new_matrix("K", MatrixType::Symmetric).expect("K");
        let round = |dm: &mut DofManager| {
            dm.backend_zero("K").expect("zero");
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                "u",
                ke.view(),
                ty,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                Some(&all),
            )
            .expect("assembly");
        };
        round(&mut dm);
        round(&mut dm);
        let mut t = Vec::new();
        for _ in 0..reps {
            dm.backend_zero("K").expect("zero");
            let t0 = Instant::now();
            dm.assemble_elemental_matrices_to_matrix(
                &mesh,
                "K",
                "u",
                ke.view(),
                ty,
                GhostType::NotGhost,
                MatrixType::Symmetric,
                Some(&all),
            )
            .expect("assembly");
            t.push(t0.elapsed().as_secs_f64());
        }
        let bits: Vec<u64> = dm
            .matrix("K")
            .expect("K")
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        match &reference {
            None => reference = Some(bits),
            Some(r) => assert_eq!(r, &bits, "{p} threads: values differ"),
        }
        let tm = median(t);
        if p == threads[0] {
            t1 = tm;
        }
        println!(
            "{p} threads: {:.4} s (x{:.2}), threaded assemblies {}",
            tm,
            t1 / tm,
            dm.threaded_assemblies()
        );
    }
}
