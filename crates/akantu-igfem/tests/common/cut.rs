// SPDX-License-Identifier: LGPL-3.0-or-later
// Test helper of akantu-rs (no upstream counterpart): an independent IGFEM
// mesh generator for straight interfaces, following the connectivity
// conventions of extra_packages/igfem/src/mesh_igfem_spherical_growing_gel_tmpl.hh
// (buildIGFEMMesh: the cut edge of an `_igfem_triangle_4` is local edge
// (1, 2); node 0 of an `_igfem_triangle_5` is the isolated vertex, node 3 on
// edge (0, 1), node 4 on edge (2, 0)).

//! Cut a triangle mesh by the straight line `φ(x) = a·x + b·y − c = 0`.

#![allow(dead_code)]

use akantu_core::{ElementType, GhostType, Idx, Real};
use akantu_mesh::{Mesh, MeshAccessor};

/// Straight interface `a·x + b·y = c`.
#[derive(Clone, Copy, Debug)]
pub struct Line {
    pub a: Real,
    pub b: Real,
    pub c: Real,
}

impl Line {
    pub fn phi(&self, x: &[Real]) -> Real {
        self.a * x[0] + self.b * x[1] - self.c
    }
}

/// A structured `n × n` triangulation of `[x0, x1] × [y0, y1]` (each square
/// split along its diagonal, counter-clockwise triangles).
pub fn structured(
    n: usize,
    x0: Real,
    x1: Real,
    y0: Real,
    y1: Real,
) -> (Vec<[Real; 2]>, Vec<[usize; 3]>) {
    let mut nodes = Vec::new();
    for j in 0..=n {
        for i in 0..=n {
            #[allow(clippy::cast_precision_loss)]
            let (fi, fj, fn_) = (i as Real, j as Real, n as Real);
            nodes.push([x0 + (x1 - x0) * fi / fn_, y0 + (y1 - y0) * fj / fn_]);
        }
    }
    let id = |i: usize, j: usize| j * (n + 1) + i;
    let mut tris = Vec::new();
    for j in 0..n {
        for i in 0..n {
            let (a, b, c, d) = (id(i, j), id(i + 1, j), id(i + 1, j + 1), id(i, j + 1));
            tris.push([a, b, c]);
            tris.push([a, c, d]);
        }
    }
    (nodes, tris)
}

/// Result of cutting: mesh with `_triangle_3`, `_igfem_triangle_4` and
/// `_igfem_triangle_5` blocks; enriched nodes follow the standard nodes.
pub struct CutMesh {
    pub mesh: Mesh,
    pub nb_standard_nodes: usize,
}

/// Intersection of edge `(p, q)` with the line (linear interpolation of φ).
fn cut_point(line: &Line, p: [Real; 2], q: [Real; 2]) -> [Real; 2] {
    let (fp, fq) = (line.phi(&p), line.phi(&q));
    let t = fp / (fp - fq);
    [p[0] + t * (q[0] - p[0]), p[1] + t * (q[1] - p[1])]
}

/// Cut `tris` by `line`. A vertex with `|φ| ≤ on_tol` lies on the line. A
/// triangle with two vertices on opposite sides becomes an
/// `_igfem_triangle_5` (two cut edges) or, when its third vertex lies on
/// the line, an `_igfem_triangle_4`; enriched nodes are shared between
/// neighbours.
pub fn cut(nodes: &[[Real; 2]], tris: &[[usize; 3]], line: &Line, on_tol: Real) -> CutMesh {
    let mut all_nodes: Vec<[Real; 2]> = nodes.to_vec();
    let mut edge_nodes: std::collections::BTreeMap<(usize, usize), usize> = Default::default();
    let mut enriched = |p: usize, q: usize, all: &mut Vec<[Real; 2]>| -> usize {
        let key = (p.min(q), p.max(q));
        *edge_nodes.entry(key).or_insert_with(|| {
            all.push(cut_point(line, nodes[key.0], nodes[key.1]));
            all.len() - 1
        })
    };
    let sign = |n: usize| {
        let f = line.phi(&nodes[n]);
        if f.abs() <= on_tol {
            0
        } else if f > 0.0 {
            1
        } else {
            -1
        }
    };
    let mut t3: Vec<Vec<usize>> = Vec::new();
    let mut t4: Vec<Vec<usize>> = Vec::new();
    let mut t5: Vec<Vec<usize>> = Vec::new();
    for t in tris {
        let s = [sign(t[0]), sign(t[1]), sign(t[2])];
        let rot = |k: usize| [t[k % 3], t[(k + 1) % 3], t[(k + 2) % 3]];
        // tri4: one vertex on the line, the two others on opposite sides
        if let Some(k) = (0..3).find(|&k| s[k] == 0 && s[(k + 1) % 3] * s[(k + 2) % 3] < 0) {
            let r = rot(k);
            let e = enriched(r[1], r[2], &mut all_nodes);
            t4.push(vec![r[0], r[1], r[2], e]);
            continue;
        }
        // tri5: one isolated vertex, strictly on the other side of both
        if let Some(k) =
            (0..3).find(|&k| s[k] != 0 && s[(k + 1) % 3] == -s[k] && s[(k + 2) % 3] == -s[k])
        {
            let r = rot(k);
            let e01 = enriched(r[0], r[1], &mut all_nodes);
            let e20 = enriched(r[2], r[0], &mut all_nodes);
            t5.push(vec![r[0], r[1], r[2], e01, e20]);
            continue;
        }
        t3.push(t.to_vec());
    }
    let mut mesh = Mesh::new(2, "cut").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for p in &all_nodes {
            acc.push_node(p).unwrap();
        }
        for (ty, list) in [
            (ElementType::Triangle3, &t3),
            (ElementType::IgfemTriangle4, &t4),
            (ElementType::IgfemTriangle5, &t5),
        ] {
            if list.is_empty() {
                continue;
            }
            let mut conn = acc.connectivity_mut(ty, GhostType::NotGhost);
            for e in list {
                let row: Vec<Idx> = e.iter().map(|&n| Idx::try_from(n).unwrap()).collect();
                conn.push_tuple(&row);
            }
        }
    }
    CutMesh {
        mesh,
        nb_standard_nodes: nodes.len(),
    }
}

/// Barycentric coordinates of `x` in the triangle `(p0, p1, p2)` (Cramer).
pub fn barycentric(p: [[Real; 2]; 3], x: [Real; 2]) -> [Real; 3] {
    let det = (p[1][0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (p[1][1] - p[0][1]);
    let l1 =
        ((x[0] - p[0][0]) * (p[2][1] - p[0][1]) - (p[2][0] - p[0][0]) * (x[1] - p[0][1])) / det;
    let l2 =
        ((p[1][0] - p[0][0]) * (x[1] - p[0][1]) - (x[0] - p[0][0]) * (p[1][1] - p[0][1])) / det;
    [1.0 - l1 - l2, l1, l2]
}

/// Signed area of a polygon (shoelace).
pub fn polygon_area(p: &[[Real; 2]]) -> Real {
    let mut a = 0.0;
    for i in 0..p.len() {
        let (x0, x1) = (p[i], p[(i + 1) % p.len()]);
        a += x0[0] * x1[1] - x1[0] * x0[1];
    }
    0.5 * a
}
