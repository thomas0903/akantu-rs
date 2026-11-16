// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Structured meshes
// of the benchmark suite; no upstream counterpart (node orders follow
// upstream's element classes, src/fe_engine/element_classes/).

//! Structured box meshes generated in code (deterministic, no Gmsh).
//!
//! The box `[lo, hi]` is cut into `n[0] × n[1] (× n[2])` cells, each cell
//! split into the pieces of the element type: one quadrangle or hexahedron,
//! two triangles (diagonal from the lower-left corner) or the six Kuhn
//! tetrahedra around the main diagonal (conforming between cells).
//! Quadratic types get their mid-edge nodes on the half-cell lattice.
//! Every piece is positively oriented (checked; two vertices are swapped
//! otherwise). Nodes are numbered in order of first use by the elements,
//! cells in x-fastest order.

use akantu_comm::Communicator;
use akantu_core::{ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};

/// The six Kuhn tetrahedra of the unit cube (vertex `i` at
/// `(i & 1, (i >> 1) & 1, (i >> 2) & 1)`), around the diagonal 0–7.
const KUHN: [[usize; 4]; 6] = [
    [0, 1, 3, 7],
    [0, 3, 2, 7],
    [0, 2, 6, 7],
    [0, 6, 4, 7],
    [0, 4, 5, 7],
    [0, 5, 1, 7],
];

/// Hexahedron_8 corners (bottom face counter-clockwise, then top).
const HEX: [usize; 8] = [0, 1, 3, 2, 4, 5, 7, 6];

/// Mid-edge pairs of tetrahedron_10 (nodes 4–9) and triangle_6 (3–5).
const TET10_EDGES: [(usize, usize); 6] = [(0, 1), (1, 2), (2, 0), (0, 3), (1, 3), (2, 3)];
const TRI6_EDGES: [(usize, usize); 3] = [(0, 1), (1, 2), (2, 0)];

/// A structured box.
#[derive(Clone, Copy, Debug)]
pub struct BoxSpec {
    /// Element type.
    pub ty: ElementType,
    /// Cells per axis (the unused axes are ignored).
    pub n: [usize; 3],
    /// Lower corner.
    pub lo: [Real; 3],
    /// Upper corner.
    pub hi: [Real; 3],
}

impl BoxSpec {
    /// A unit cube (square in 2-D) with `n` cells per axis.
    #[must_use]
    pub fn unit(ty: ElementType, n: usize) -> Self {
        Self {
            ty,
            n: [n; 3],
            lo: [0.0; 3],
            hi: [1.0; 3],
        }
    }

    /// The same box on `[lo, hi]` per axis.
    #[must_use]
    pub fn with_bounds(mut self, lo: Real, hi: Real) -> Self {
        self.lo = [lo; 3];
        self.hi = [hi; 3];
        self
    }

    /// Element count.
    #[must_use]
    pub fn nb_elements(&self) -> usize {
        let d = self.ty.spatial_dimension();
        let cells: usize = self.n[..d].iter().product();
        cells * pieces_per_cell(self.ty)
    }
}

fn pieces_per_cell(ty: ElementType) -> usize {
    match ty {
        ElementType::Triangle3 | ElementType::Triangle6 => 2,
        ElementType::Tetrahedron4 | ElementType::Tetrahedron10 => 6,
        _ => 1,
    }
}

/// Vertex `i` of a cell in lattice units (2 per cell).
fn corner(i: usize, d: usize) -> [i64; 3] {
    let mut c = [0i64; 3];
    for (k, ck) in c.iter_mut().enumerate().take(d) {
        *ck = 2 * ((i >> k) & 1) as i64;
    }
    c
}

fn signed_volume(p: &[[i64; 3]], d: usize) -> i64 {
    let e = |a: usize, k: usize| p[a][k] - p[0][k];
    if d == 2 {
        e(1, 0) * e(2, 1) - e(1, 1) * e(2, 0)
    } else {
        e(1, 0) * (e(2, 1) * e(3, 2) - e(2, 2) * e(3, 1))
            - e(1, 1) * (e(2, 0) * e(3, 2) - e(2, 2) * e(3, 0))
            + e(1, 2) * (e(2, 0) * e(3, 1) - e(2, 1) * e(3, 0))
    }
}

/// The pieces of one cell in lattice offsets (node order of the type).
fn cell_pieces(ty: ElementType) -> Result<Vec<Vec<[i64; 3]>>> {
    let d = ty.spatial_dimension();
    let simplex = |verts: &[usize], edges: &[(usize, usize)]| -> Vec<[i64; 3]> {
        let mut v: Vec<[i64; 3]> = verts.iter().map(|&i| corner(i, d)).collect();
        if signed_volume(&v, d) < 0 {
            v.swap(1, 2);
        }
        for &(a, b) in edges {
            let mut m = [0i64; 3];
            for k in 0..3 {
                m[k] = (v[a][k] + v[b][k]) / 2;
            }
            v.push(m);
        }
        v
    };
    Ok(match ty {
        ElementType::Quadrangle4 => vec![[0, 1, 3, 2].iter().map(|&i| corner(i, 2)).collect()],
        ElementType::Hexahedron8 => vec![HEX.iter().map(|&i| corner(i, 3)).collect()],
        ElementType::Triangle3 | ElementType::Triangle6 => {
            let e: &[(usize, usize)] = if ty == ElementType::Triangle6 {
                &TRI6_EDGES
            } else {
                &[]
            };
            vec![simplex(&[0, 1, 3], e), simplex(&[0, 3, 2], e)]
        }
        ElementType::Tetrahedron4 | ElementType::Tetrahedron10 => {
            let e: &[(usize, usize)] = if ty == ElementType::Tetrahedron10 {
                &TET10_EDGES
            } else {
                &[]
            };
            KUHN.iter().map(|t| simplex(t, e)).collect()
        }
        _ => {
            return Err(Error::Unsupported(format!(
                "suite mesh generator: element type {ty}"
            )));
        }
    })
}

/// Node coordinates and connectivity of the box (rank-independent).
///
/// # Errors
/// Unsupported element type, sizes past the index range.
pub fn box_arrays(spec: &BoxSpec) -> Result<(Vec<Real>, Vec<Idx>)> {
    let d = spec.ty.spatial_dimension();
    let pieces = cell_pieces(spec.ty)?;
    let mut lat = [1usize; 3];
    for (l, &n) in lat.iter_mut().zip(&spec.n).take(d) {
        *l = 2 * n + 1;
    }
    let total = lat[0]
        .checked_mul(lat[1])
        .and_then(|x| x.checked_mul(lat[2]))
        .ok_or_else(|| Error::invalid("suite mesh: lattice too large"))?;
    let mut id = vec![u32::MAX; total];
    let mut nodes: Vec<Real> = Vec::new();
    let mut conn: Vec<Idx> =
        Vec::with_capacity(spec.nb_elements() * spec.ty.nb_nodes_per_element());
    let cells = [
        spec.n[0],
        if d > 1 { spec.n[1] } else { 1 },
        if d > 2 { spec.n[2] } else { 1 },
    ];
    let mut next: u32 = 0;
    for cz in 0..cells[2] {
        for cy in 0..cells[1] {
            for cx in 0..cells[0] {
                let origin = [2 * cx as i64, 2 * cy as i64, 2 * cz as i64];
                for piece in &pieces {
                    for off in piece {
                        let mut key = [0usize; 3];
                        for k in 0..3 {
                            key[k] = (origin[k] + off[k]) as usize;
                        }
                        let flat = (key[2] * lat[1] + key[1]) * lat[0] + key[0];
                        if id[flat] == u32::MAX {
                            id[flat] = next;
                            next = next
                                .checked_add(1)
                                .ok_or_else(|| Error::invalid("suite mesh: too many nodes"))?;
                            for (k, &kk) in key.iter().enumerate().take(d) {
                                let t = kk as Real / (2 * spec.n[k]) as Real;
                                nodes.push(spec.lo[k] + (spec.hi[k] - spec.lo[k]) * t);
                            }
                        }
                        conn.push(Idx::try_from(id[flat]).map_err(|_| {
                            Error::invalid("suite mesh: node index past the index type")
                        })?);
                    }
                }
            }
        }
    }
    Ok((nodes, conn))
}

/// The box as a mesh on `comm` (`None`: serial). With `full = false` the
/// mesh stays empty (the other ranks of a distribution from rank 0).
///
/// # Errors
/// Mesh errors, unsupported type.
pub fn box_mesh(spec: &BoxSpec, comm: Option<&Communicator>, full: bool) -> Result<Mesh> {
    let d = spec.ty.spatial_dimension();
    let mut mesh = match comm {
        Some(c) => Mesh::with_communicator(d, "mesh", c.clone())?,
        None => Mesh::new(d, "mesh")?,
    };
    if full {
        let (nodes, conn) = box_arrays(spec)?;
        {
            let mut a = MeshAccessor::new(&mut mesh);
            for x in nodes.chunks_exact(d) {
                a.push_node(x)?;
            }
            let mut c = a.connectivity_mut(spec.ty, GhostType::NotGhost);
            for e in conn.chunks_exact(spec.ty.nb_nodes_per_element()) {
                c.push_tuple(e);
            }
        }
        mesh.compute_bounding_box()?;
    }
    Ok(mesh)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn coords(nodes: &[Real], d: usize, n: Idx) -> [Real; 3] {
        let n = n as usize;
        let mut p = [0.0; 3];
        p[..d].copy_from_slice(&nodes[n * d..n * d + d]);
        p
    }

    #[test]
    fn counts_and_orientation() {
        for (ty, nodes_expected) in [
            (ElementType::Triangle3, 9),
            (ElementType::Triangle6, 25),
            (ElementType::Quadrangle4, 9),
            (ElementType::Tetrahedron4, 27),
            (ElementType::Tetrahedron10, 125),
            (ElementType::Hexahedron8, 27),
        ] {
            let spec = BoxSpec::unit(ty, 2);
            let (nodes, conn) = box_arrays(&spec).unwrap();
            let d = ty.spatial_dimension();
            let nn = ty.nb_nodes_per_element();
            assert_eq!(nodes.len() / d, nodes_expected, "{ty}");
            assert_eq!(conn.len() / nn, spec.nb_elements(), "{ty}");
            // positive measure of the vertex simplex / first corner triad
            for e in conn.chunks_exact(nn) {
                let p0 = coords(&nodes, d, e[0]);
                let (a, b, c) = match ty {
                    ElementType::Quadrangle4 => (e[1], e[3], e[0]),
                    ElementType::Hexahedron8 => (e[1], e[3], e[4]),
                    _ => (e[1], e[2], if d == 3 { e[3] } else { e[0] }),
                };
                let v = |n: Idx| {
                    let p = coords(&nodes, d, n);
                    [p[0] - p0[0], p[1] - p0[1], p[2] - p0[2]]
                };
                let (u, w, z) = (v(a), v(b), v(c));
                let det = if d == 2 {
                    u[0] * w[1] - u[1] * w[0]
                } else {
                    u[0] * (w[1] * z[2] - w[2] * z[1]) - u[1] * (w[0] * z[2] - w[2] * z[0])
                        + u[2] * (w[0] * z[1] - w[1] * z[0])
                };
                assert!(det > 0.0, "{ty}: element {e:?} det {det}");
            }
        }
    }

    #[test]
    fn mid_nodes_are_midpoints() {
        let spec = BoxSpec::unit(ElementType::Tetrahedron10, 2);
        let (nodes, conn) = box_arrays(&spec).unwrap();
        for e in conn.as_chunks::<10>().0 {
            for (m, &(a, b)) in TET10_EDGES.iter().enumerate() {
                let (pa, pb, pm) = (
                    coords(&nodes, 3, e[a]),
                    coords(&nodes, 3, e[b]),
                    coords(&nodes, 3, e[4 + m]),
                );
                for k in 0..3 {
                    assert_eq!(pm[k], 0.5 * (pa[k] + pb[k]));
                }
            }
        }
    }
}
