// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). Structured meshes of
// the model suite (no upstream counterpart).

//! Structured meshes generated in code, written as Gmsh MSH 2.2 text with
//! physical names and read through `akantu_io::read_mesh_from_str`: the
//! model sees what a user's Gmsh mesh gives (element groups for the named
//! volumes and boundaries, `tag_*` and `physical_names` element data), and
//! no file is involved.
//!
//! A [`Block`] is a box split into cells of one type; its volume group is
//! the block's name, its six boundary groups `<name>_xmin` … `<name>_zmax`
//! (2-D: four), made of the boundary facets of its elements, oriented
//! outward. Blocks never share nodes (two blocks touching along a face
//! have coincident, distinct nodes: contact and split-node set-ups).

use std::fmt::Write as _;

use akantu_comm::Communicator;
use akantu_core::enums::MeshIoType;
use akantu_core::{Real, Result};
use akantu_io::{ReadOptions, read_mesh_from_str};
use akantu_mesh::Mesh;

/// Cell type of a block.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Cell {
    /// Linear triangles (two per square, diagonal `(i, j)–(i+1, j+1)`).
    Tri3,
    /// Quadratic triangles (same split, mid-edge nodes).
    Tri6,
    /// Bilinear quadrangles.
    Quad4,
    /// Linear tetrahedra (six per cube around the diagonal `0–6`, conforming).
    Tet4,
    /// Trilinear hexahedra.
    Hex8,
}

impl Cell {
    fn dim(self) -> usize {
        match self {
            Self::Tri3 | Self::Tri6 | Self::Quad4 => 2,
            Self::Tet4 | Self::Hex8 => 3,
        }
    }
    fn gmsh_type(self) -> u32 {
        match self {
            Self::Tri3 => 2,
            Self::Tri6 => 9,
            Self::Quad4 => 3,
            Self::Tet4 => 4,
            Self::Hex8 => 5,
        }
    }
    /// Local facets (nodes in element order) and their Gmsh type.
    fn facets(self) -> (&'static [&'static [usize]], u32) {
        match self {
            Self::Tri3 => (&[&[0, 1], &[1, 2], &[2, 0]], 1),
            Self::Tri6 => (&[&[0, 1, 3], &[1, 2, 4], &[2, 0, 5]], 8),
            Self::Quad4 => (&[&[0, 1], &[1, 2], &[2, 3], &[3, 0]], 1),
            Self::Tet4 => (&[&[0, 1, 2], &[0, 1, 3], &[0, 2, 3], &[1, 2, 3]], 2),
            Self::Hex8 => (
                &[
                    &[0, 1, 2, 3],
                    &[4, 5, 6, 7],
                    &[0, 1, 5, 4],
                    &[1, 2, 6, 5],
                    &[2, 3, 7, 6],
                    &[3, 0, 4, 7],
                ],
                3,
            ),
        }
    }
}

/// A box split into cells.
#[derive(Clone, Debug)]
pub struct Block {
    /// Volume group name (and prefix of the boundary groups).
    pub name: String,
    /// Cell type.
    pub cell: Cell,
    /// Lower corner.
    pub origin: [Real; 3],
    /// Edge lengths.
    pub size: [Real; 3],
    /// Cells per direction (the third ignored in 2-D).
    pub n: [usize; 3],
}

impl Block {
    /// A block named `name`.
    #[must_use]
    pub fn new(name: &str, cell: Cell, origin: [Real; 3], size: [Real; 3], n: [usize; 3]) -> Self {
        Self {
            name: name.to_owned(),
            cell,
            origin,
            size,
            n,
        }
    }
}

/// Gmsh text builder.
#[derive(Debug, Default)]
pub struct Msh {
    dim: usize,
    nodes: Vec<[Real; 3]>,
    /// (gmsh type, physical tag, 0-based nodes)
    elements: Vec<(u32, usize, Vec<usize>)>,
    /// (dimension, name) indexed by physical tag − 1
    physicals: Vec<(usize, String)>,
}

const SIDES: [&str; 6] = ["xmin", "xmax", "ymin", "ymax", "zmin", "zmax"];

impl Msh {
    /// Empty mesh of dimension `dim`.
    #[must_use]
    pub fn new(dim: usize) -> Self {
        Self {
            dim,
            ..Self::default()
        }
    }

    /// Physical tag of `name` (created on first use).
    pub fn physical(&mut self, dim: usize, name: &str) -> usize {
        if let Some(i) = self
            .physicals
            .iter()
            .position(|(d, n)| *d == dim && n == name)
        {
            return i + 1;
        }
        self.physicals.push((dim, name.to_owned()));
        self.physicals.len()
    }

    /// Rename the physical group `name` of dimension `dim` to `new_name`.
    pub fn alias(&mut self, dim: usize, name: &str, new_name: &str) {
        if let Some(p) = self
            .physicals
            .iter_mut()
            .find(|(d, n)| *d == dim && n == name)
        {
            p.1 = new_name.to_owned();
        }
    }

    /// Add a node, returns its index.
    pub fn node(&mut self, x: [Real; 3]) -> usize {
        self.nodes.push(x);
        self.nodes.len() - 1
    }

    /// Add an element of Gmsh type `ty` in the physical group `phys`.
    pub fn element(&mut self, ty: u32, phys: usize, nodes: Vec<usize>) {
        self.elements.push((ty, phys, nodes));
    }

    /// Add a block; returns the number of volume elements added.
    pub fn block(&mut self, b: &Block) -> usize {
        let dim = b.cell.dim();
        assert_eq!(dim, self.dim, "block dimension");
        let quadratic = b.cell == Cell::Tri6;
        let k = if quadratic { 2 } else { 1 };
        let n = [
            b.n[0] * k,
            b.n[1] * k,
            if dim == 3 { b.n[2] * k } else { 0 },
        ];
        let base = self.nodes.len();
        let id = |i: usize, j: usize, l: usize| base + (l * (n[1] + 1) + j) * (n[0] + 1) + i;
        for l in 0..=n[2] {
            for j in 0..=n[1] {
                for i in 0..=n[0] {
                    let mut x = [0.0; 3];
                    let ijk = [i, j, l];
                    for d in 0..dim {
                        x[d] = b.origin[d] + b.size[d] * (ijk[d] as Real / n[d] as Real);
                    }
                    self.nodes.push(x);
                }
            }
        }
        let vol = self.physical(dim, &b.name);
        let mut cells: Vec<Vec<usize>> = Vec::new();
        let nz = if dim == 3 { b.n[2] } else { 1 };
        for l in 0..nz {
            for j in 0..b.n[1] {
                for i in 0..b.n[0] {
                    match b.cell {
                        Cell::Quad4 => cells.push(vec![
                            id(i, j, 0),
                            id(i + 1, j, 0),
                            id(i + 1, j + 1, 0),
                            id(i, j + 1, 0),
                        ]),
                        Cell::Tri3 => {
                            let (a, bb, c, d) = (
                                id(i, j, 0),
                                id(i + 1, j, 0),
                                id(i + 1, j + 1, 0),
                                id(i, j + 1, 0),
                            );
                            cells.push(vec![a, bb, c]);
                            cells.push(vec![a, c, d]);
                        }
                        Cell::Tri6 => {
                            let (i2, j2) = (2 * i, 2 * j);
                            let p = |di: usize, dj: usize| id(i2 + di, j2 + dj, 0);
                            // (0,0) (2,0) (2,2) and (0,0) (2,2) (0,2)
                            cells.push(vec![p(0, 0), p(2, 0), p(2, 2), p(1, 0), p(2, 1), p(1, 1)]);
                            cells.push(vec![p(0, 0), p(2, 2), p(0, 2), p(1, 1), p(1, 2), p(0, 1)]);
                        }
                        Cell::Hex8 | Cell::Tet4 => {
                            let v = [
                                id(i, j, l),
                                id(i + 1, j, l),
                                id(i + 1, j + 1, l),
                                id(i, j + 1, l),
                                id(i, j, l + 1),
                                id(i + 1, j, l + 1),
                                id(i + 1, j + 1, l + 1),
                                id(i, j + 1, l + 1),
                            ];
                            if b.cell == Cell::Hex8 {
                                cells.push(v.to_vec());
                            } else {
                                for t in [
                                    [0, 1, 2, 6],
                                    [0, 2, 3, 6],
                                    [0, 3, 7, 6],
                                    [0, 7, 4, 6],
                                    [0, 4, 5, 6],
                                    [0, 5, 1, 6],
                                ] {
                                    cells.push(t.iter().map(|&q| v[q]).collect());
                                }
                            }
                        }
                    }
                }
            }
        }
        // boundary facets, oriented outward
        let lo = b.origin;
        let hi = [
            b.origin[0] + b.size[0],
            b.origin[1] + b.size[1],
            b.origin[2] + b.size[2],
        ];
        let (facets, fty) = b.cell.facets();
        let mut boundary: Vec<(usize, Vec<usize>)> = Vec::new();
        for c in &cells {
            for f in facets {
                let nodes: Vec<usize> = f.iter().map(|&q| c[q]).collect();
                for (s, side) in SIDES.iter().enumerate().take(2 * dim) {
                    let axis = s / 2;
                    let plane = if s % 2 == 0 { lo[axis] } else { hi[axis] };
                    if nodes.iter().all(|&q| self.nodes[q][axis] == plane) {
                        let _ = side;
                        boundary.push((s, self.orient(nodes.clone(), axis, s % 2 == 1)));
                    }
                }
            }
        }
        let nb = cells.len();
        for c in cells {
            self.elements.push((b.cell.gmsh_type(), vol, c));
        }
        for (s, nodes) in boundary {
            let tag = self.physical(dim - 1, &format!("{}_{}", b.name, SIDES[s]));
            self.elements.push((fty, tag, nodes));
        }
        nb
    }

    /// Reorder a boundary facet so that its normal points along `+axis`
    /// (`positive`) or `-axis`.
    fn orient(&self, mut f: Vec<usize>, axis: usize, positive: bool) -> Vec<usize> {
        let x = |q: usize| self.nodes[f[q]];
        let normal = if self.dim == 2 {
            // normal of a 2-D segment a→b: (dy, −dx)
            let (a, b) = (x(0), x(1));
            [b[1] - a[1], -(b[0] - a[0]), 0.0]
        } else {
            let (a, b, c) = (x(0), x(1), x(2));
            let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
            let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
            [
                u[1] * v[2] - u[2] * v[1],
                u[2] * v[0] - u[0] * v[2],
                u[0] * v[1] - u[1] * v[0],
            ]
        };
        let ok = (normal[axis] > 0.0) == positive;
        if !ok {
            match f.len() {
                // segment_2: swap; segment_3 (a, b, mid): swap the ends
                2 | 3 if self.dim == 2 => f.swap(0, 1),
                3 => f.swap(1, 2),
                4 => f.swap(1, 3),
                _ => unreachable!("facet of {} nodes", f.len()),
            }
        }
        f
    }

    /// The Gmsh MSH 2.2 text.
    #[must_use]
    pub fn text(&self) -> String {
        let mut s = String::with_capacity(64 * (self.nodes.len() + self.elements.len()));
        s.push_str("$MeshFormat\n2.2 0 8\n$EndMeshFormat\n$PhysicalNames\n");
        let _ = writeln!(s, "{}", self.physicals.len());
        for (i, (d, n)) in self.physicals.iter().enumerate() {
            let _ = writeln!(s, "{d} {} \"{n}\"", i + 1);
        }
        s.push_str("$EndPhysicalNames\n$Nodes\n");
        let _ = writeln!(s, "{}", self.nodes.len());
        for (i, x) in self.nodes.iter().enumerate() {
            // `{:?}` prints the shortest representation that reads back
            // exactly
            let _ = writeln!(s, "{} {:?} {:?} {:?}", i + 1, x[0], x[1], x[2]);
        }
        s.push_str("$EndNodes\n$Elements\n");
        let _ = writeln!(s, "{}", self.elements.len());
        for (i, (ty, phys, nodes)) in self.elements.iter().enumerate() {
            let _ = write!(s, "{} {ty} 2 {phys} {phys}", i + 1);
            for q in nodes {
                let _ = write!(s, " {}", q + 1);
            }
            s.push('\n');
        }
        s.push_str("$EndElements\n");
        s
    }

    /// Read into a mesh on `comm`: rank 0 holds the whole mesh, the other
    /// ranks an empty one (distribute afterwards).
    ///
    /// # Errors
    /// Reader errors.
    pub fn read(&self, comm: &Communicator, id: &str) -> Result<Mesh> {
        let mut mesh = Mesh::with_communicator(self.dim, id, comm.clone())?;
        if comm.rank() == 0 {
            read_mesh_from_str(
                &mut mesh,
                "suite.msh",
                &self.text(),
                MeshIoType::Gmsh,
                &ReadOptions::default(),
            )?;
        }
        mesh.compute_bounding_box()?;
        Ok(mesh)
    }
}
