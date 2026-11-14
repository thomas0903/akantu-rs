// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Timing harness of
// the lower-dimensional part assignment of src/mesh_utils/mesh_partition.cc
// (MeshPartition::fillPartitionInformation) as ported in
// `akantu_mesh::partition`: a bench,
// not a test.
//
// Run: `cargo run --release -p akantu-mesh --example partition_cost [n] [rounds]`
// (default n = 30, rounds = 7). Kuhn cube of n³ cubes × 6 Tet4, prescribed
// partition in 8 z-slabs, timed `PrescribedPartition::partition` in the
// default (UD-231 corrected) and compat (upstream) modes, interleaved:
//   tet4        tetrahedra only (no lower-dimensional element);
//   tet4+tri3   the same plus every boundary triangle (conforming adjacency:
//               the conformity check runs for each triangle).
// Prints min / median / max in milliseconds per case and mode.

use std::time::Instant;

use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::partition::{MeshPartitioner, PartitionCompat, PrescribedPartition};
use akantu_mesh::utils::fill_element_to_subelements_data;
use akantu_mesh::{Mesh, MeshAccessor};

const NG: GhostType = GhostType::NotGhost;

fn cube(n: usize, boundary: bool) -> Mesh {
    let mut m = Mesh::new(3, "cube").expect("dimension 3");
    let id = |i: usize, j: usize, k: usize| -> Idx {
        Idx::try_from((k * (n + 1) + j) * (n + 1) + i).expect("small mesh")
    };
    {
        let mut a = MeshAccessor::new(&mut m);
        #[allow(clippy::cast_precision_loss)] // n is small
        let h = 1.0 / n as Real;
        for k in 0..=n {
            for j in 0..=n {
                for i in 0..=n {
                    #[allow(clippy::cast_precision_loss)] // n is small
                    a.push_node(&[i as Real * h, j as Real * h, k as Real * h])
                        .expect("node");
                }
            }
        }
        let mut tets = a.connectivity_mut(ElementType::Tetrahedron4, NG);
        for k in 0..n {
            for j in 0..n {
                for i in 0..n {
                    let v = |a: usize, b: usize, c: usize| id(i + a, j + b, k + c);
                    let (v000, v111) = (v(0, 0, 0), v(1, 1, 1));
                    for (p, q) in [
                        (v(1, 0, 0), v(1, 1, 0)),
                        (v(1, 1, 0), v(0, 1, 0)),
                        (v(0, 1, 0), v(0, 1, 1)),
                        (v(0, 1, 1), v(0, 0, 1)),
                        (v(0, 0, 1), v(1, 0, 1)),
                        (v(1, 0, 1), v(1, 0, 0)),
                    ] {
                        tets.push_tuple(&[v000, p, q, v111]);
                    }
                }
            }
        }
        drop(tets);
        if boundary {
            // every boundary square as two triangles sharing the diagonal
            // from its lowest to its highest corner (faces of the Kuhn
            // tetrahedra)
            let mut tris = a.connectivity_mut(ElementType::Triangle3, NG);
            for s in [0, n] {
                for b in 0..n {
                    for c in 0..n {
                        for f in [
                            |s, b, c| (s, b, c),
                            |s, b, c| (b, s, c),
                            |s, b, c| (b, c, s),
                        ] {
                            let p = |db: usize, dc: usize| {
                                let (i, j, k) = f(s, b + db, c + dc);
                                id(i, j, k)
                            };
                            tris.push_tuple(&[p(0, 0), p(1, 0), p(1, 1)]);
                            tris.push_tuple(&[p(0, 0), p(0, 1), p(1, 1)]);
                        }
                    }
                }
            }
        }
    }
    fill_element_to_subelements_data(&mut m).expect("adjacency");
    m
}

fn main() {
    let mut args = std::env::args().skip(1);
    let n: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(30);
    let rounds: usize = args.next().and_then(|a| a.parse().ok()).unwrap_or(7);
    println!("n = {n}, {rounds} interleaved rounds after one warm-up; ms min / median / max");
    for (name, boundary) in [("tet4", false), ("tet4+tri3", true)] {
        let m = cube(n, boundary);
        let nt = m.nb_element(ElementType::Tetrahedron4, NG);
        let lin: Vec<Idx> = (0..nt)
            .map(|e| Idx::try_from((e / (6 * n * n)) * 8 / n).expect("part"))
            .collect();
        let mut default = PrescribedPartition::new(lin.clone());
        let mut compat = PrescribedPartition::new(lin).with_compat(PartitionCompat {
            orphan_lower_dimensional_in_part_zero: true,
        });
        let mut t = [Vec::new(), Vec::new()];
        for round in 0..=rounds {
            for (mode, p) in [&mut default, &mut compat].into_iter().enumerate() {
                let start = Instant::now();
                let r = p.partition(&m, 8).expect("partition");
                let ms = start.elapsed().as_secs_f64() * 1e3;
                std::hint::black_box(r);
                if round > 0 {
                    t[mode].push(ms);
                }
            }
        }
        for (mode, v) in ["default", "compat"].iter().zip(t.iter_mut()) {
            v.sort_by(f64::total_cmp);
            println!(
                "{name:10} {:>7} tri3 {:>6}  {mode:8} {:8.2} / {:8.2} / {:8.2}",
                nt,
                m.nb_element(ElementType::Triangle3, NG),
                v[0],
                v[v.len() / 2],
                v[v.len() - 1]
            );
        }
    }
}
