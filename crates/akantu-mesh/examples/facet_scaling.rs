// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Scaling measurement
// of the mesh set-up paths (ported behaviour:
// src/mesh/mesh.cc initMeshFacets, src/mesh_utils/mesh_utils.cc
// buildAllFacets / fillElementToSubElementsData / purifyMesh,
// Mesh::eraseElements).
//
// Run: `cargo run --release -p akantu-mesh --example facet_scaling`.
// Structured unit-cube meshes of n³ cubes split into 6 tetrahedra each
// (Kuhn subdivision) plus their boundary triangles; prints the wall time of
// each operation per size and per element (results: docs/BENCHMARKS.md).

use std::time::Instant;

use akantu_core::{Element, ElementType, GhostType, Idx, Real};
use akantu_mesh::removal::erase_elements_setup;
use akantu_mesh::utils::{
    build_node_to_elements, fill_element_to_subelements_data, init_mesh_facets, purify_mesh,
};
use akantu_mesh::{Mesh, MeshAccessor};

fn cube(n: usize) -> Mesh {
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
                    #[allow(clippy::cast_precision_loss)]
                    a.push_node(&[i as Real * h, j as Real * h, k as Real * h])
                        .expect("node");
                }
            }
        }
        let mut tets = a.connectivity_mut(ElementType::Tetrahedron4, GhostType::NotGhost);
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
        // boundary triangles of the z = 0 face (mixed dimensions)
        drop(tets);
        let mut tris = a.connectivity_mut(ElementType::Triangle3, GhostType::NotGhost);
        for j in 0..n {
            for i in 0..n {
                tris.push_tuple(&[id(i, j, 0), id(i + 1, j, 0), id(i + 1, j + 1, 0)]);
                tris.push_tuple(&[id(i, j, 0), id(i + 1, j + 1, 0), id(i, j + 1, 0)]);
            }
        }
    }
    m.compute_bounding_box().expect("bbox");
    m
}

fn time<T>(f: impl FnOnce() -> T) -> (T, f64) {
    let t = Instant::now();
    let r = f();
    (r, t.elapsed().as_secs_f64())
}

/// Wall times `[csr, fill_e2s, init_mesh_facets, erase, purify]` of one run
/// on a fresh mesh, and the number of facets built.
fn run(n: usize) -> ([f64; 5], usize, usize) {
    let mut m = cube(n);
    let nb = m.nb_element(ElementType::Tetrahedron4, GhostType::NotGhost);
    let (_, t_csr) = time(|| build_node_to_elements(&m, None).expect("csr"));
    let (_, t_fill) = time(|| fill_element_to_subelements_data(&mut m).expect("fill"));
    let (_, t_facets) = time(|| init_mesh_facets(&mut m).expect("facets"));
    let nb_facets = m.mesh_facets().map_or(0, |f| {
        f.nb_element(ElementType::Triangle3, GhostType::NotGhost)
    });
    let erase: Vec<Element> = (0..nb)
        .step_by(3)
        .map(|e| Element::try_new(ElementType::Tetrahedron4, e, GhostType::NotGhost))
        .collect::<Result<_, _>>()
        .expect("elements");
    let (_, t_erase) = time(|| erase_elements_setup(&mut m, &erase).expect("erase"));
    let (_, t_purify) = time(|| purify_mesh(&mut m).expect("purify"));
    ([t_csr, t_fill, t_facets, t_erase, t_purify], nb, nb_facets)
}

fn main() {
    const REPS: usize = 5;
    println!(
        "median [min, max] of {REPS} runs after one warm-up, seconds\n\
         n  tetrahedra  facets   csr               fill_e2s          init_mesh_facets  erase             purify"
    );
    for n in [8usize, 16, 32] {
        let _ = run(n); // warm-up
        let mut samples: Vec<[f64; 5]> = Vec::new();
        let (mut nb, mut nb_facets) = (0, 0);
        for _ in 0..REPS {
            let (t, a, b) = run(n);
            samples.push(t);
            (nb, nb_facets) = (a, b);
        }
        let mut line = format!("{n:<2} {nb:>10} {nb_facets:>7}");
        for k in 0..5 {
            let mut v: Vec<f64> = samples.iter().map(|s| s[k]).collect();
            v.sort_by(f64::total_cmp);
            line.push_str(&format!(
                "  {:.4} [{:.4},{:.4}]",
                v[REPS / 2],
                v[0],
                v[REPS - 1]
            ));
        }
        println!("{line}");
    }
}
