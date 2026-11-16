// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Structured meshes of
// the set-up suite (no upstream counterpart).

//! Deterministic structured meshes, written as Gmsh text so that every case
//! gets the same nodes, elements, tags and physical groups as a Gmsh file:
//!
//! * [`Grid::Tet4`]: the unit cube in `n³` cubes of 6 Kuhn tetrahedra
//!   (positive volume) and its boundary in triangles;
//! * [`Grid::Hex8`]: the unit cube in `n³` hexahedra and its boundary in
//!   quadrangles;
//! * [`Grid::Tri3`]: the unit square in `n²` squares of 2 triangles and its
//!   boundary in segments.
//!
//! Physical groups: one per boundary side (`xmin`, `xmax`, `ymin`, `ymax`,
//! `zmin`, `zmax`) and `body` for the volume elements. Coordinates are
//! `i as f64 * (1.0 / n as f64)`, printed with the shortest round-trip
//! representation (the Python benchmarks generate the same values).

use std::fmt::Write as _;

use akantu_core::enums::MeshIoType;
use akantu_core::{Real, Result};
use akantu_io::ReadOptions;
use akantu_mesh::Mesh;

/// Which structured mesh.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Grid {
    /// Unit cube, Kuhn tetrahedra.
    Tet4,
    /// Unit cube, hexahedra.
    Hex8,
    /// Unit square, triangles.
    Tri3,
}

impl Grid {
    /// Spatial dimension.
    pub fn dim(self) -> usize {
        match self {
            Grid::Tet4 | Grid::Hex8 => 3,
            Grid::Tri3 => 2,
        }
    }
}

/// Element blocks of a structured mesh (1-based node tags).
#[derive(Clone, Debug)]
pub struct Blocks {
    /// Spatial dimension.
    pub dim: usize,
    /// Coordinates, 3 per node (z = 0 in 2-D).
    pub nodes: Vec<[Real; 3]>,
    /// Volume elements: Gmsh type and connectivities (1-based).
    pub body: (u32, Vec<Vec<usize>>),
    /// Boundary elements per side (`xmin`, `xmax`, `ymin`, `ymax`[, `zmin`,
    /// `zmax`]): Gmsh type and connectivities.
    pub sides: Vec<(u32, Vec<Vec<usize>>)>,
}

/// Side names in physical-tag order (tags 1..).
pub const SIDES: [&str; 6] = ["xmin", "xmax", "ymin", "ymax", "zmin", "zmax"];

#[allow(clippy::cast_precision_loss)] // n is small
fn coord(i: usize, n: usize) -> Real {
    i as Real * (1.0 / n as Real)
}

/// The blocks of `grid` with `n` cells per side.
pub fn blocks(grid: Grid, n: usize) -> Blocks {
    match grid {
        Grid::Tri3 => {
            let id = |i: usize, j: usize| j * (n + 1) + i + 1;
            let mut nodes = Vec::with_capacity((n + 1) * (n + 1));
            for j in 0..=n {
                for i in 0..=n {
                    nodes.push([coord(i, n), coord(j, n), 0.0]);
                }
            }
            let mut tris = Vec::with_capacity(2 * n * n);
            for j in 0..n {
                for i in 0..n {
                    tris.push(vec![id(i, j), id(i + 1, j), id(i + 1, j + 1)]);
                    tris.push(vec![id(i, j), id(i + 1, j + 1), id(i, j + 1)]);
                }
            }
            let seg = |f: &dyn Fn(usize) -> (usize, usize)| -> Vec<Vec<usize>> {
                (0..n)
                    .map(|k| {
                        let (a, b) = f(k);
                        let (c, d) = f(k + 1);
                        vec![id(a, b), id(c, d)]
                    })
                    .collect()
            };
            Blocks {
                dim: 2,
                nodes,
                body: (2, tris),
                sides: vec![
                    (1, seg(&|k| (0, k))),
                    (1, seg(&|k| (n, k))),
                    (1, seg(&|k| (k, 0))),
                    (1, seg(&|k| (k, n))),
                ],
            }
        }
        Grid::Tet4 | Grid::Hex8 => {
            let id = |i: usize, j: usize, k: usize| (k * (n + 1) + j) * (n + 1) + i + 1;
            let mut nodes = Vec::with_capacity((n + 1).pow(3));
            for k in 0..=n {
                for j in 0..=n {
                    for i in 0..=n {
                        nodes.push([coord(i, n), coord(j, n), coord(k, n)]);
                    }
                }
            }
            let mut body = Vec::new();
            for k in 0..n {
                for j in 0..n {
                    for i in 0..n {
                        let v = |a: usize, b: usize, c: usize| id(i + a, j + b, k + c);
                        if grid == Grid::Hex8 {
                            body.push(vec![
                                v(0, 0, 0),
                                v(1, 0, 0),
                                v(1, 1, 0),
                                v(0, 1, 0),
                                v(0, 0, 1),
                                v(1, 0, 1),
                                v(1, 1, 1),
                                v(0, 1, 1),
                            ]);
                        } else {
                            let (v000, v111) = (v(0, 0, 0), v(1, 1, 1));
                            for (p, q) in [
                                (v(1, 0, 0), v(1, 1, 0)),
                                (v(1, 1, 0), v(0, 1, 0)),
                                (v(0, 1, 0), v(0, 1, 1)),
                                (v(0, 1, 1), v(0, 0, 1)),
                                (v(0, 0, 1), v(1, 0, 1)),
                                (v(1, 0, 1), v(1, 0, 0)),
                            ] {
                                body.push(vec![v000, p, q, v111]);
                            }
                        }
                    }
                }
            }
            // boundary squares: axis `d` fixed at `s` (0 or n), (b, c) the
            // two other indices in increasing axis order
            let mut sides = Vec::new();
            for d in 0..3 {
                for s in [0, n] {
                    let at = |b: usize, c: usize| match d {
                        0 => id(s, b, c),
                        1 => id(b, s, c),
                        _ => id(b, c, s),
                    };
                    let mut elems = Vec::new();
                    for c in 0..n {
                        for b in 0..n {
                            if grid == Grid::Hex8 {
                                elems.push(vec![
                                    at(b, c),
                                    at(b + 1, c),
                                    at(b + 1, c + 1),
                                    at(b, c + 1),
                                ]);
                            } else {
                                // the diagonal from the lowest to the highest
                                // corner: faces of the Kuhn tetrahedra
                                elems.push(vec![at(b, c), at(b + 1, c), at(b + 1, c + 1)]);
                                elems.push(vec![at(b, c), at(b, c + 1), at(b + 1, c + 1)]);
                            }
                        }
                    }
                    sides.push((if grid == Grid::Hex8 { 3 } else { 2 }, elems));
                }
            }
            Blocks {
                dim: 3,
                nodes,
                body: (if grid == Grid::Hex8 { 5 } else { 4 }, body),
                sides,
            }
        }
    }
}

/// Gmsh MSH 2.2 ASCII text of `grid` (physical names, two tags per
/// element: physical, elementary).
pub fn msh22(grid: Grid, n: usize) -> String {
    let b = blocks(grid, n);
    let dim = b.dim;
    let mut s = String::with_capacity(b.nodes.len() * 64 + b.body.1.len() * 48);
    s.push_str("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", b.sides.len() + 1);
    for (k, name) in SIDES.iter().take(b.sides.len()).enumerate() {
        let _ = writeln!(s, "{} {} \"{name}\"", dim - 1, k + 1);
    }
    let body_tag = b.sides.len() + 1;
    let _ = writeln!(s, "{dim} {body_tag} \"body\"\n$EndPhysicalNames\n$Nodes");
    let _ = writeln!(s, "{}", b.nodes.len());
    for (i, x) in b.nodes.iter().enumerate() {
        let _ = writeln!(s, "{} {:?} {:?} {:?}", i + 1, x[0], x[1], x[2]);
    }
    s.push_str("$EndNodes\n$Elements\n");
    let total = b.body.1.len() + b.sides.iter().map(|x| x.1.len()).sum::<usize>();
    let _ = writeln!(s, "{total}");
    let mut eid = 0;
    let mut put = |s: &mut String, ty: u32, tag: usize, conn: &[usize]| {
        eid += 1;
        let _ = write!(s, "{eid} {ty} 2 {tag} {tag}");
        for c in conn {
            let _ = write!(s, " {c}");
        }
        s.push('\n');
    };
    for (k, (ty, elems)) in b.sides.iter().enumerate() {
        for e in elems {
            put(&mut s, *ty, k + 1, e);
        }
    }
    for e in &b.body.1 {
        put(&mut s, b.body.0, body_tag, e);
    }
    s.push_str("$EndElements\n");
    s
}

/// Gmsh MSH 4.1 ASCII text of `grid`: one entity per boundary side and one
/// volume entity, each with its physical tag; every node in the volume
/// entity's block.
pub fn msh41(grid: Grid, n: usize) -> String {
    let b = blocks(grid, n);
    let dim = b.dim;
    let nb_sides = b.sides.len();
    let body_tag = nb_sides + 1;
    let mut s = String::with_capacity(b.nodes.len() * 64 + b.body.1.len() * 48);
    s.push_str("$MeshFormat\n4.1 0 8\n$EndMeshFormat\n$PhysicalNames\n");
    let _ = writeln!(s, "{}", nb_sides + 1);
    for (k, name) in SIDES.iter().take(nb_sides).enumerate() {
        let _ = writeln!(s, "{} {} \"{name}\"", dim - 1, k + 1);
    }
    let _ = writeln!(s, "{dim} {body_tag} \"body\"\n$EndPhysicalNames\n$Entities");
    // counts of points, curves, surfaces, volumes
    if dim == 3 {
        let _ = writeln!(s, "0 0 {nb_sides} 1");
    } else {
        let _ = writeln!(s, "0 {nb_sides} 1 0");
    }
    for k in 0..nb_sides {
        let _ = writeln!(s, "{} 0 0 0 1 1 1 1 {} 0", k + 1, k + 1);
    }
    let _ = write!(s, "1 0 0 0 1 1 1 1 {body_tag} {nb_sides}");
    for k in 0..nb_sides {
        let _ = write!(s, " {}", k + 1);
    }
    s.push_str("\n$EndEntities\n$Nodes\n");
    let nn = b.nodes.len();
    let _ = writeln!(s, "1 {nn} 1 {nn}\n{dim} 1 0 {nn}");
    for i in 0..nn {
        let _ = writeln!(s, "{}", i + 1);
    }
    for x in &b.nodes {
        let _ = writeln!(s, "{:?} {:?} {:?}", x[0], x[1], x[2]);
    }
    s.push_str("$EndNodes\n$Elements\n");
    let total = b.body.1.len() + b.sides.iter().map(|x| x.1.len()).sum::<usize>();
    let _ = writeln!(s, "{} {total} 1 {total}", nb_sides + 1);
    let mut eid = 0;
    let mut block = |s: &mut String, edim: usize, etag: usize, ty: u32, elems: &[Vec<usize>]| {
        let _ = writeln!(s, "{edim} {etag} {ty} {}", elems.len());
        for e in elems {
            eid += 1;
            let _ = write!(s, "{eid}");
            for c in e {
                let _ = write!(s, " {c}");
            }
            s.push('\n');
        }
    };
    for (k, (ty, elems)) in b.sides.iter().enumerate() {
        block(&mut s, dim - 1, k + 1, *ty, elems);
    }
    block(&mut s, dim, 1, b.body.0, &b.body.1);
    s.push_str("$EndElements\n");
    s
}

/// A mesh of `grid` read from its MSH 2.2 text (groups from the physical
/// names, as a Gmsh file gives them).
pub fn mesh(grid: Grid, n: usize, id: &str) -> Result<Mesh> {
    let text = msh22(grid, n);
    mesh_from_text(grid.dim(), &text, id)
}

/// A mesh read from MSH text.
pub fn mesh_from_text(dim: usize, text: &str, id: &str) -> Result<Mesh> {
    let mut m = Mesh::new(dim, id)?;
    akantu_io::read_mesh_from_str(
        &mut m,
        "suite.msh",
        text,
        MeshIoType::Gmsh,
        &ReadOptions::default(),
    )?;
    Ok(m)
}
