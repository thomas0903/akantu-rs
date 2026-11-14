// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs test helpers, ported in intent from Akantu
// (EPFL/LSMS): test/test_solver/test_sparse_solver_mumps.cc (the diagonal
// system), src/model/common/dof_manager/dof_manager_default.cc (rank-partial
// assembly into global equations, owned/shared/ghost equations).

//! Distributed test systems for the MUMPS backend: every system is an
//! **add sequence** `(i, j, v)` per rank (global equations), so the same
//! rank-partial AIJ matrices — same entries, same storage order — can be
//! rebuilt by the C++ oracle probe (`oracle/probes/mumps/`) from the
//! exported files.

#![allow(dead_code)] // the test binary and the scaling example use subsets

use std::io::Write;
use std::path::Path;

use akantu_core::enums::MatrixType;
use akantu_core::{GlobalIdx, Real};
use akantu_linalg::layout::EquationOwnership::{Owned, PureGhost, Shared};
use akantu_linalg::{EquationLayout, SparseMatrixAij};

/// One distributed system for `p` ranks.
#[derive(Clone, Debug)]
pub struct System {
    /// Name (file prefix).
    pub name: String,
    /// Global order.
    pub n: usize,
    /// Symmetric AIJ storage (MUMPS `sym = 2`) or unsymmetric (`sym = 0`).
    pub sym: bool,
    /// Add sequence of every rank.
    pub per_rank: Vec<Vec<(GlobalIdx, GlobalIdx, Real)>>,
    /// Global right-hand side.
    pub b: Vec<Real>,
}

impl System {
    /// The rank-partial AIJ matrix of `rank` (upstream `add(i, j, v)`
    /// semantics: first-inserted coordinates, duplicates summed in order).
    pub fn matrix(&self, rank: usize) -> SparseMatrixAij {
        let ty = if self.sym {
            MatrixType::Symmetric
        } else {
            MatrixType::Unsymmetric
        };
        let mut a = SparseMatrixAij::new(self.name.clone(), self.n as GlobalIdx, ty);
        for &(i, j, v) in &self.per_rank[rank] {
            a.add(i, j, v).expect("valid entry");
        }
        a
    }

    /// `y = A x` of the global operator (sum of every rank's entries; a
    /// symmetric entry `(i, j)`, `i != j`, acts on both triangles).
    pub fn apply(&self, x: &[Real]) -> Vec<Real> {
        let mut y = vec![0.0; self.n];
        for part in &self.per_rank {
            for &(i, j, v) in part {
                let (i, j) = (i as usize, j as usize);
                y[i] += v * x[j];
                if self.sym && i != j {
                    y[j] += v * x[i];
                }
            }
        }
        y
    }

    /// Frobenius norm of the global operator (duplicates summed across
    /// ranks; a symmetric off-diagonal entry counts in both triangles) —
    /// an upper bound of `‖A‖₂` for the backward-error diagnostics.
    pub fn frobenius_norm(&self) -> Real {
        let mut entries: std::collections::BTreeMap<(GlobalIdx, GlobalIdx), Real> =
            std::collections::BTreeMap::new();
        for part in &self.per_rank {
            for &(i, j, v) in part {
                *entries.entry((i, j)).or_insert(0.0) += v;
            }
        }
        entries
            .iter()
            .map(|(&(i, j), v)| {
                if self.sym && i != j {
                    2.0 * v * v
                } else {
                    v * v
                }
            })
            .sum::<Real>()
            .sqrt()
    }

    /// `(‖A‖_F‖x‖₂/‖b‖₂, ‖Ax − b‖₂/(‖A‖_F‖x‖₂ + ‖b‖₂))`: the amplification
    /// factor of the a-priori residual bound and the normwise backward
    /// error (Rigal–Gaches, with `‖A‖_F ≥ ‖A‖₂`), for the documentation of
    /// the residual smoke check. Diagnostics only.
    pub fn backward_error(&self, x: &[Real]) -> (Real, Real) {
        let norm = |v: &[Real]| v.iter().map(|a| a * a).sum::<Real>().sqrt();
        let ax = self.apply(x);
        let r: Vec<Real> = ax.iter().zip(&self.b).map(|(a, b)| a - b).collect();
        let (na, nx, nb) = (self.frobenius_norm(), norm(x), norm(&self.b));
        (na * nx / nb, norm(&r) / (na * nx + nb))
    }

    /// Relative residual `‖A x − b‖₂ / ‖b‖₂`.
    pub fn residual(&self, x: &[Real]) -> Real {
        let ax = self.apply(x);
        let r: Real = ax
            .iter()
            .zip(&self.b)
            .map(|(a, b)| (a - b) * (a - b))
            .sum::<Real>()
            .sqrt();
        let nb: Real = self.b.iter().map(|b| b * b).sum::<Real>().sqrt();
        if nb == 0.0 { r } else { r / nb }
    }
}

/// Contiguous block `[start, end)` of `n` items for rank `r` of `p`.
pub fn block(n: usize, p: usize, r: usize) -> (usize, usize) {
    (n * r / p, n * (r + 1) / p)
}

/// Split an add sequence into `p` contiguous blocks (an element-wise
/// assembly partitioned by element number).
pub fn split<T: Clone>(all: &[T], p: usize) -> Vec<Vec<T>> {
    (0..p)
        .map(|r| {
            let (s, e) = block(all.len(), p, r);
            all[s..e].to_vec()
        })
        .collect()
}

/// Equation layout of rank `r` of `p` for a system of order `n`: the owned
/// block of equations, preceded (r > 0) by a **shared** copy of the last
/// equation of rank `r − 1` and followed (r + 1 < p) by a **pure ghost**
/// (no equation) — the shapes the DOF manager produces for a distributed
/// mesh.
pub fn layout(n: usize, p: usize, r: usize) -> EquationLayout {
    let (s, e) = block(n, p, r);
    let mut own = Vec::new();
    let shared = r > 0 && s > 0;
    if shared {
        own.push(Shared);
    }
    own.extend(std::iter::repeat_n(Owned, e - s));
    if r + 1 < p {
        own.push(PureGhost);
    }
    let mut l = EquationLayout::new();
    l.append_block(&own, s as GlobalIdx, n, |i| {
        (shared && i == 0).then_some(s as GlobalIdx - 1)
    })
    .expect("layout");
    l
}

/// Local right-hand side of `layout` from the global `b` (pure ghosts get a
/// poison value that must never be read).
pub fn local_rhs(layout: &EquationLayout, b: &[Real]) -> Vec<Real> {
    (0..layout.nb_local())
        .map(|l| layout.global(l).map_or(f64::NAN, |g| b[g as usize]))
        .collect()
}

/// `test_sparse_solver_mumps`: `A = diag(1 / (1 + g))` with every diagonal
/// term added by the rank owning the equation, `b = 1` ⇒ `x_g = 1 + g`.
pub fn upstream_diagonal(p: usize) -> System {
    let n = 11;
    let per_rank = (0..p)
        .map(|r| {
            let (s, e) = block(n, p, r);
            (s..e)
                .map(|g| (g as GlobalIdx, g as GlobalIdx, 1.0 / (1.0 + g as Real)))
                .collect()
        })
        .collect();
    System {
        name: "upstream_diagonal".into(),
        n,
        sym: true,
        per_rank,
        b: vec![1.0; n],
    }
}

/// Deterministic right-hand side.
pub fn rhs(n: usize) -> Vec<Real> {
    (0..n)
        .map(|g| (0.37 * g as Real + 0.1).sin() + 0.5)
        .collect()
}

/// 3-D Laplacian on an `m³` grid (edge "elements" `[1 −1; −1 1]`, element
/// order x, y, z edges) plus `c = 0.01` on the diagonal (one term per node
/// after its edges), assembled element-wise in contiguous element blocks
/// per rank. `sym`: symmetric storage; otherwise unsymmetric storage with a
/// skew convection term `±0.3` on the x edges.
pub fn laplacian(m: usize, sym: bool, p: usize) -> System {
    let n = m * m * m;
    let id = |i: usize, j: usize, k: usize| ((i * m + j) * m + k) as GlobalIdx;
    let mut elements: Vec<Vec<(GlobalIdx, GlobalIdx, Real)>> = Vec::new();
    for dir in 0..3 {
        for i in 0..m {
            for j in 0..m {
                for k in 0..m {
                    let (ii, jj, kk) = match dir {
                        0 => (i + 1, j, k),
                        1 => (i, j + 1, k),
                        _ => (i, j, k + 1),
                    };
                    if ii >= m || jj >= m || kk >= m {
                        continue;
                    }
                    let (a, b) = (id(i, j, k), id(ii, jj, kk));
                    let mut e = vec![(a, a, 1.0), (a, b, -1.0)];
                    if !sym {
                        let c = if dir == 0 { 0.3 } else { 0.0 };
                        e.push((b, a, -1.0 - c));
                        e[1].2 += c;
                    }
                    e.push((b, b, 1.0));
                    elements.push(e);
                }
            }
        }
    }
    for g in 0..n as GlobalIdx {
        elements.push(vec![(g, g, 0.01)]);
    }
    let per_rank = split(&elements, p)
        .into_iter()
        .map(|blk| blk.into_iter().flatten().collect())
        .collect();
    System {
        name: format!("laplacian_{}_{m}", if sym { "sym" } else { "unsym" }),
        n,
        sym,
        per_rank,
        b: rhs(n),
    }
}

/// Static linear elasticity ("SMM-like"): a unit cube split in `m³` cubes
/// of 6 linear tetrahedra (Kuhn split), 3 DOFs per node (equation
/// `3 node + d`), E = 1, ν = 0.3, element stiffness `Bᵀ D B V` added
/// element by element (symmetric storage: upper `j ≥ i` local entries, as
/// upstream's symmetric-into-symmetric assembly), contiguous element blocks
/// per rank; Dirichlet on the face x = 0 as upstream `applyBoundary`
/// produces it (entries touching a blocked equation dropped, unit diagonal
/// once, by the rank owning the equation); unit body force along −z.
pub fn elasticity(m: usize, p: usize) -> System {
    let nn = (m + 1) * (m + 1) * (m + 1);
    let n = 3 * nn;
    let node = |i: usize, j: usize, k: usize| (i * (m + 1) + j) * (m + 1) + k;
    let h = 1.0 / m as Real;
    let pos = |v: usize| {
        let k = v % (m + 1);
        let j = (v / (m + 1)) % (m + 1);
        let i = v / ((m + 1) * (m + 1));
        [i as Real * h, j as Real * h, k as Real * h]
    };
    let blocked = |eq: usize| pos(eq / 3)[0] == 0.0;
    // Kuhn split of the cube (0..1)^3 along the main diagonal: 6 tets.
    const PERMS: [[usize; 3]; 6] = [
        [0, 1, 2],
        [0, 2, 1],
        [1, 0, 2],
        [1, 2, 0],
        [2, 0, 1],
        [2, 1, 0],
    ];
    let (e, nu) = (1.0, 0.3);
    let lambda = e * nu / ((1.0 + nu) * (1.0 - 2.0 * nu));
    let mu = e / (2.0 * (1.0 + nu));
    let mut elements = Vec::new();
    for i in 0..m {
        for j in 0..m {
            for k in 0..m {
                for perm in PERMS {
                    let mut c = [0usize; 3];
                    let mut verts = vec![node(i, j, k)];
                    for &d in &perm {
                        c[d] += 1;
                        verts.push(node(i + c[0], j + c[1], k + c[2]));
                    }
                    elements.push(tet_stiffness(&verts, &pos, lambda, mu));
                }
            }
        }
    }
    let sym_entries: Vec<Vec<(GlobalIdx, GlobalIdx, Real)>> = elements
        .into_iter()
        .map(|(eqs, ke)| {
            let mut out = Vec::new();
            for a in 0..12 {
                for b in a..12 {
                    let (gi, gj) = (eqs[a], eqs[b]);
                    if blocked(gi) || blocked(gj) {
                        continue;
                    }
                    out.push((gi as GlobalIdx, gj as GlobalIdx, ke[a * 12 + b]));
                }
            }
            out
        })
        .collect();
    let mut per_rank: Vec<Vec<_>> = split(&sym_entries, p)
        .into_iter()
        .map(|blk| blk.into_iter().flatten().collect())
        .collect();
    for (r, part) in per_rank.iter_mut().enumerate() {
        let (s, e) = block(n, p, r);
        for g in s..e {
            if blocked(g) {
                part.push((g as GlobalIdx, g as GlobalIdx, 1.0));
            }
        }
    }
    let b = (0..n)
        .map(|g| {
            if blocked(g) {
                0.0
            } else if g % 3 == 2 {
                -h * h * h
            } else {
                0.0
            }
        })
        .collect();
    System {
        name: format!("elasticity_{m}"),
        n,
        sym: true,
        per_rank,
        b,
    }
}

/// Stiffness of a linear tetrahedron: equations and the dense 12×12 matrix
/// (row-major).
fn tet_stiffness(
    verts: &[usize],
    pos: &dyn Fn(usize) -> [Real; 3],
    lambda: Real,
    mu: Real,
) -> (Vec<usize>, Vec<Real>) {
    let x: Vec<[Real; 3]> = verts.iter().map(|&v| pos(v)).collect();
    // Jacobian J = [x1-x0, x2-x0, x3-x0] (columns), shape gradients from J^-T.
    let mut jm = [[0.0; 3]; 3];
    for c in 0..3 {
        for r in 0..3 {
            jm[r][c] = x[c + 1][r] - x[0][r];
        }
    }
    let det = jm[0][0] * (jm[1][1] * jm[2][2] - jm[1][2] * jm[2][1])
        - jm[0][1] * (jm[1][0] * jm[2][2] - jm[1][2] * jm[2][0])
        + jm[0][2] * (jm[1][0] * jm[2][1] - jm[1][1] * jm[2][0]);
    let inv = |r: usize, c: usize| -> Real {
        let (r1, r2) = ((c + 1) % 3, (c + 2) % 3);
        let (c1, c2) = ((r + 1) % 3, (r + 2) % 3);
        (jm[r1][c1] * jm[r2][c2] - jm[r1][c2] * jm[r2][c1]) / det
    };
    // grad N_a (a = 1..3) = row a-1 of J^-1; grad N_0 = -sum.
    let mut g = [[0.0; 3]; 4];
    for (a, row) in g.iter_mut().enumerate().skip(1) {
        for (d, v) in row.iter_mut().enumerate() {
            *v = inv(a - 1, d);
        }
    }
    let g0: [Real; 3] = std::array::from_fn(|d| -(g[1][d] + g[2][d] + g[3][d]));
    g[0] = g0;
    let vol = det.abs() / 6.0;
    let mut ke = vec![0.0; 144];
    for a in 0..4 {
        for b in 0..4 {
            for i in 0..3 {
                for j in 0..3 {
                    let mut v = lambda * g[a][i] * g[b][j] + mu * g[a][j] * g[b][i];
                    if i == j {
                        v += mu * (0..3).map(|k| g[a][k] * g[b][k]).sum::<Real>();
                    }
                    ke[(3 * a + i) * 12 + 3 * b + j] = v * vol;
                }
            }
        }
    }
    let eqs = verts
        .iter()
        .flat_map(|&v| (0..3).map(move |d| 3 * v + d))
        .collect();
    (eqs, ke)
}

/// A saddle-point system that keeps MUMPS's workspace estimate too small
/// (`INFOG(1) = -9` even with large `ICNTL(14)`): an `m × m` grid Laplacian
/// with a tiny diagonal (`1e-8`) coupled to `m²/2` multiplier rows with a
/// zero diagonal block, unsymmetric storage (heavy delayed pivoting).
pub fn saddle(m: usize, p: usize) -> System {
    let na = m * m;
    let nb = na / 2;
    let n = na + nb;
    let mut all = Vec::new();
    for i in 0..m {
        for j in 0..m {
            let r = (i * m + j) as GlobalIdx;
            all.push((r, r, 1e-8));
            if j + 1 < m {
                all.push((r, r + 1, -1.0));
                all.push((r + 1, r, -1.0));
            }
            if i + 1 < m {
                let d = m as GlobalIdx;
                all.push((r, r + d, -1.0));
                all.push((r + d, r, -1.0));
            }
        }
    }
    for k in 0..nb {
        let row = (na + k) as GlobalIdx;
        let c1 = ((k * 7919) % na) as GlobalIdx;
        let c2 = ((k * 104_729 + 3) % na) as GlobalIdx;
        all.push((row, c1, 1.0));
        all.push((c1, row, 1.0));
        all.push((row, c2, 0.5));
        all.push((c2, row, 0.5));
    }
    System {
        name: format!("saddle_{m}"),
        n,
        sym: false,
        per_rank: split(&all, p),
        b: rhs(n),
    }
}

/// Write rank `rank`'s system file (`<name>.p<P>.r<rank>.txt`): header
/// `n sym nnz`, then the rank-partial AIJ matrix in storage order (0-based
/// `i j v`, shortest round-trip decimal), and on rank 0 the global
/// right-hand side (`<name>.p<P>.rhs.txt`). The oracle probe re-adds the
/// entries in this order, which reproduces the same storage.
pub fn export(dir: &Path, sys: &System, p: usize, rank: usize, a: &SparseMatrixAij) {
    std::fs::create_dir_all(dir).expect("export dir");
    let (i, j, v) = a.coo();
    let mut f = std::io::BufWriter::new(
        std::fs::File::create(dir.join(format!("{}.p{p}.r{rank}.txt", sys.name))).expect("file"),
    );
    writeln!(f, "{} {} {}", sys.n, u8::from(sys.sym), i.len()).expect("write");
    for k in 0..i.len() {
        writeln!(f, "{} {} {:?}", i[k], j[k], v[k]).expect("write");
    }
    if rank == 0 {
        let mut f = std::io::BufWriter::new(
            std::fs::File::create(dir.join(format!("{}.p{p}.rhs.txt", sys.name))).expect("file"),
        );
        writeln!(f, "{}", sys.n).expect("write");
        for x in &sys.b {
            writeln!(f, "{x:?}").expect("write");
        }
    }
}

/// Write a gathered solution (rank 0) as `<name>.p<P>.<tag>.x.txt`.
pub fn export_solution(dir: &Path, name: &str, p: usize, tag: &str, x: &[Real]) {
    std::fs::create_dir_all(dir).expect("export dir");
    let mut f = std::io::BufWriter::new(
        std::fs::File::create(dir.join(format!("{name}.p{p}.{tag}.x.txt"))).expect("file"),
    );
    for v in x {
        writeln!(f, "{v:?}").expect("write");
    }
}

/// Read a solution file (one value per line).
pub fn read_solution(path: &Path) -> Option<Vec<Real>> {
    let s = std::fs::read_to_string(path).ok()?;
    s.lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| l.trim().parse().ok())
        .collect()
}
