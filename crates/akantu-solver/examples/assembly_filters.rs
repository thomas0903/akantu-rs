// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs performance probe (no upstream counterpart): the elemental
// matrix assembly of DOFManagerDefault::assembleElementalMatricesToMatrix
// (src/model/common/dof_manager/dof_manager_default.cc) with one and with
// two element filters (materials) on one element type.

//! `assembly_filters [n] [reps]`: a structured `n³` cube of hexahedra split
//! into six tetrahedra, 3 DOFs per node, symmetric 12 × 12 elemental
//! matrices into a symmetric `K`. Times one assembly round (median of
//! `reps` rounds after two warm-up rounds that fill the position cache) for
//! (a) the cache disabled (one profile lookup per entry), (b) one filter
//! holding every element, (c) two filters (even and odd elements), and
//! checks that the three give the same bits.

use std::time::Instant;

use akantu_comm::Communicator;
use akantu_core::enums::MatrixType;
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
    let mesh = cube(n);
    let ty = ElementType::Tetrahedron4;
    let ne = mesh.nb_element(ty, GhostType::NotGhost);
    // deterministic symmetric elemental matrices
    let mut x = 0x2545_f491_4f6c_dd1du64;
    let mut ke = Array::new(144);
    for _ in 0..ne {
        let mut b = [0.0; 144];
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
    let even: Vec<Idx> = all.iter().copied().filter(|e| e % 2 == 0).collect();
    let odd: Vec<Idx> = all.iter().copied().filter(|e| e % 2 == 1).collect();
    let block = |filter: &[Idx]| {
        let mut a = Array::new(144);
        for &e in filter {
            a.push_tuple(ke.tuple(e as usize));
        }
        a
    };
    let (k_all, k_even, k_odd) = (block(&all), block(&even), block(&odd));
    let run = |cached: bool, two: bool| -> (f64, Vec<u64>) {
        let mut dm = DofManager::new("dm", Communicator::serial());
        dm.register_nodal_dofs(&mesh, "u", 3).expect("dofs");
        dm.set_assembly_cache(cached);
        dm.new_matrix("K", MatrixType::Symmetric).expect("K");
        let round = |dm: &mut DofManager| {
            dm.backend_zero("K").expect("zero");
            let parts: Vec<(&[Idx], &Array<Real>)> = if two {
                vec![(&even, &k_even), (&odd, &k_odd)]
            } else {
                vec![(&all, &k_all)]
            };
            for (f, k) in parts {
                dm.assemble_elemental_matrices_to_matrix(
                    &mesh,
                    "K",
                    "u",
                    k.view(),
                    ty,
                    GhostType::NotGhost,
                    MatrixType::Symmetric,
                    Some(f),
                )
                .expect("assembly");
            }
        };
        // two warm-up rounds: profile, then positions stored
        round(&mut dm);
        round(&mut dm);
        let mut t = Vec::new();
        for _ in 0..reps {
            let t0 = Instant::now();
            round(&mut dm);
            t.push(t0.elapsed().as_secs_f64());
        }
        let bits = dm
            .matrix("K")
            .expect("K")
            .values()
            .iter()
            .map(|v| v.to_bits())
            .collect();
        (median(t), bits)
    };
    let (t_plain, b_plain) = run(false, false);
    let (t_one, b_one) = run(true, false);
    let (t_two, b_two) = run(true, true);
    let (t_two_plain, b_two_plain) = run(false, true);
    assert_eq!(b_one, b_plain, "one filter, cached");
    assert_eq!(b_two, b_two_plain, "two filters, cached");
    println!("n {n}: {ne} tetrahedra; one assembly round, median of {reps}:");
    println!("  uncached (profile lookups), one filter : {t_plain:.4} s");
    println!("  uncached (profile lookups), two filters: {t_two_plain:.4} s");
    println!("  cached, one filter                     : {t_one:.4} s");
    println!("  cached, two filters                    : {t_two:.4} s");
}
