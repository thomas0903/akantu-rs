// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the node-ordered scatter of the execution policy (D-123).

//! Node-ordered scatter of element vectors into a nodal array.
//!
//! The serial assembly (`DofManager::assemble_elemental_array_local_array`,
//! upstream `assembleElementalArrayLocalArray`) loops over the elements `k`
//! of a block and their local nodes `i`: `out[n] += scale · v[k][i]` with
//! `n = conn[filter[k]][i]`. Each node therefore receives its contributions
//! in the order of increasing `(k, i)`. [`NodeScatter`] stores, per node,
//! the positions `k·nnpe + i` that contribute to it in that order (a counting
//! sort of the block's connectivity), so threads that own disjoint node
//! ranges perform for every node the serial sequence of additions: the
//! result is bitwise the serial one for every thread count.
//!
//! The table depends on the block's connectivity rows only. A caller keeps
//! it between steps and checks it with [`NodeScatter::matches`] before each
//! use (a topology change, e.g. cohesive insertion, changes the rows; the
//! check compares every row with the copy the table was built from).

use super::{Exec, Tuples};
use crate::error::{Error, Result};
use crate::types::{Idx, Real};

/// Entries per part below which a scatter is not split.
const MIN_ENTRIES_PER_PART: usize = 4096;

/// Per-node contribution lists of one block (see the module documentation).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NodeScatter {
    nb_nodes: usize,
    nnpe: usize,
    /// `offsets[n]..offsets[n + 1]`: the entries of node `n`.
    offsets: Vec<u32>,
    /// Positions `k·nnpe + i`, by node, increasing within a node.
    entries: Vec<u32>,
    /// The connectivity rows of the block's elements in block order (what
    /// the table was built from, for [`NodeScatter::matches`]).
    rows: Vec<Idx>,
}

/// Mesh element of block position `k`.
fn element_of(filter: Option<&[Idx]>, k: usize, nb_rows: usize) -> Result<usize> {
    let e = match filter {
        Some(f) => usize::try_from(f[k]).unwrap_or(usize::MAX),
        None => k,
    };
    if e >= nb_rows {
        return Err(Error::OutOfRange {
            what: "element",
            index: e,
            len: nb_rows,
        });
    }
    Ok(e)
}

impl NodeScatter {
    /// The table of the block `filter` (all rows when `None`) of the
    /// connectivity `conn` (`nnpe` nodes per row) for a nodal array of
    /// `nb_nodes` nodes.
    ///
    /// # Errors
    /// An element or node out of range, `nnpe == 0`, or more than
    /// `u32::MAX` entries.
    pub fn new(conn: &[Idx], nnpe: usize, filter: Option<&[Idx]>, nb_nodes: usize) -> Result<Self> {
        if nnpe == 0 || !conn.len().is_multiple_of(nnpe) {
            return Err(Error::invalid(
                "NodeScatter: connectivity length is not a multiple of the nodes per element",
            ));
        }
        let nb_rows = conn.len() / nnpe;
        let nb_elements = filter.map_or(nb_rows, <[Idx]>::len);
        let total = nb_elements
            .checked_mul(nnpe)
            .filter(|&t| u32::try_from(t).is_ok() && u32::try_from(nb_nodes).is_ok())
            .ok_or_else(|| Error::invalid("NodeScatter: block too large for 32-bit positions"))?;
        let mut rows = Vec::with_capacity(total);
        for k in 0..nb_elements {
            let e = element_of(filter, k, nb_rows)?;
            rows.extend_from_slice(&conn[e * nnpe..(e + 1) * nnpe]);
        }
        let mut counts = vec![0u32; nb_nodes + 1];
        for &n in &rows {
            let node =
                usize::try_from(n)
                    .ok()
                    .filter(|&u| u < nb_nodes)
                    .ok_or(Error::OutOfRange {
                        what: "node",
                        index: usize::try_from(n).unwrap_or(usize::MAX),
                        len: nb_nodes,
                    })?;
            counts[node + 1] += 1;
        }
        for n in 0..nb_nodes {
            counts[n + 1] += counts[n];
        }
        let offsets = counts;
        let mut next: Vec<u32> = offsets[..nb_nodes].to_vec();
        let mut entries = vec![0u32; total];
        // positions in increasing order: each node's list is increasing
        for (pos, &n) in rows.iter().enumerate() {
            let node = usize::try_from(n).unwrap_or(0);
            entries[next[node] as usize] = pos as u32;
            next[node] += 1;
        }
        Ok(Self {
            nb_nodes,
            nnpe,
            offsets,
            entries,
            rows,
        })
    }

    /// `true` when the table is the one [`NodeScatter::new`] would build from
    /// these arguments (compares every connectivity row of the block with
    /// the rows the table was built from; parallel with `exec`).
    #[must_use]
    pub fn matches(
        &self,
        exec: &Exec,
        conn: &[Idx],
        nnpe: usize,
        filter: Option<&[Idx]>,
        nb_nodes: usize,
    ) -> bool {
        if nnpe != self.nnpe || nb_nodes != self.nb_nodes || !conn.len().is_multiple_of(nnpe) {
            return false;
        }
        let nb_rows = conn.len() / nnpe;
        let nb_elements = filter.map_or(nb_rows, <[Idx]>::len);
        if nb_elements * nnpe != self.rows.len() {
            return false;
        }
        if filter.is_none() {
            return conn == self.rows.as_slice();
        }
        let mut ok = vec![true; exec.nb_parts(nb_elements, 8192)];
        let bounds: Vec<usize> = (0..=ok.len()).map(|p| p * nb_elements / ok.len()).collect();
        exec.for_each_bounds(
            &(0..=ok.len()).collect::<Vec<_>>(),
            Tuples::new(&mut ok, 1),
            |p, mut flag| {
                let mut good = true;
                for k in bounds[p]..bounds[p + 1] {
                    good &= element_of(filter, k, nb_rows).is_ok_and(|e| {
                        conn[e * nnpe..(e + 1) * nnpe] == self.rows[k * nnpe..(k + 1) * nnpe]
                    });
                }
                flag.as_mut_slice()[0] = good;
            },
        );
        ok.into_iter().all(|g| g)
    }

    /// Number of block elements.
    #[must_use]
    pub fn nb_elements(&self) -> usize {
        self.rows.len() / self.nnpe
    }

    /// `out[n·nb_dof + c] += scale · values[(k·nnpe + i)·nb_dof + c]` for every
    /// contribution `(k, i)` of every node `n`, in the serial order (see the
    /// module documentation); threads own disjoint node ranges with
    /// balanced numbers of contributions.
    ///
    /// # Errors
    /// `values` or `out` of the wrong length (nothing is written).
    pub fn scatter_add(
        &self,
        exec: &Exec,
        values: &[Real],
        nb_dof: usize,
        scale: Real,
        out: &mut [Real],
    ) -> Result<()> {
        if nb_dof == 0
            || values.len() != self.rows.len() * nb_dof
            || out.len() != self.nb_nodes * nb_dof
        {
            return Err(Error::DimensionMismatch {
                context: "NodeScatter::scatter_add (element values or nodal array)",
                expected: self.rows.len() * nb_dof,
                got: values.len(),
            });
        }
        let parts = exec.nb_parts(self.entries.len(), MIN_ENTRIES_PER_PART);
        let total = self.entries.len();
        let mut bounds = Vec::with_capacity(parts + 1);
        bounds.push(0);
        for p in 1..parts {
            let target = (p * total / parts) as u32;
            let node = self
                .offsets
                .partition_point(|&o| o < target)
                .min(self.nb_nodes);
            bounds.push(node.max(*bounds.last().unwrap_or(&0)));
        }
        bounds.push(self.nb_nodes);
        let (offsets, entries) = (&self.offsets, &self.entries);
        match nb_dof {
            1 => exec.for_each_bounds(&bounds, Tuples::new(out, 1), |n0, o| {
                node_range::<1>(offsets, entries, values, scale, n0, o.into_slice());
            }),
            2 => exec.for_each_bounds(&bounds, Tuples::new(out, 2), |n0, o| {
                node_range::<2>(offsets, entries, values, scale, n0, o.into_slice());
            }),
            3 => exec.for_each_bounds(&bounds, Tuples::new(out, 3), |n0, o| {
                node_range::<3>(offsets, entries, values, scale, n0, o.into_slice());
            }),
            d => exec.for_each_bounds(&bounds, Tuples::new(out, d), |n0, o| {
                let o = o.into_slice();
                for (j, dst) in o.chunks_exact_mut(d).enumerate() {
                    let n = n0 + j;
                    for &pos in &entries[offsets[n] as usize..offsets[n + 1] as usize] {
                        let src = &values[pos as usize * d..(pos as usize + 1) * d];
                        for c in 0..d {
                            dst[c] += scale * src[c];
                        }
                    }
                }
            }),
        }
        Ok(())
    }
}

/// The nodes `n0..` of `out` (`D` values per node). Not inlined into the
/// part closure, so that its slices are no-alias parameters.
#[inline(never)]
fn node_range<const D: usize>(
    offsets: &[u32],
    entries: &[u32],
    values: &[Real],
    scale: Real,
    n0: usize,
    out: &mut [Real],
) {
    for (j, dst) in out.as_chunks_mut::<D>().0.iter_mut().enumerate() {
        let n = n0 + j;
        for &pos in &entries[offsets[n] as usize..offsets[n + 1] as usize] {
            let src = &values[pos as usize * D..(pos as usize + 1) * D];
            for c in 0..D {
                dst[c] += scale * src[c];
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::exec::ExecPolicy;

    /// The serial loop of `assembleElementalArrayLocalArray`.
    fn serial(
        conn: &[Idx],
        nnpe: usize,
        filter: Option<&[Idx]>,
        v: &[Real],
        d: usize,
        scale: Real,
        out: &mut [Real],
    ) {
        let nb = filter.map_or(conn.len() / nnpe, <[Idx]>::len);
        for k in 0..nb {
            let e = filter.map_or(k, |f| f[k] as usize);
            for (i, &n) in conn[e * nnpe..(e + 1) * nnpe].iter().enumerate() {
                for c in 0..d {
                    out[n as usize * d + c] += scale * v[(k * nnpe + i) * d + c];
                }
            }
        }
    }

    struct Lcg(u64);
    impl Lcg {
        fn next(&mut self) -> u64 {
            self.0 = self
                .0
                .wrapping_mul(6_364_136_223_846_793_005)
                .wrapping_add(1_442_695_040_888_963_407);
            self.0 >> 11
        }
        fn real(&mut self) -> Real {
            match self.next() % 23 {
                0 => -0.0,
                1 => Real::INFINITY,
                2 => 5e-324,
                3 => Real::NAN,
                _ => (self.next() as Real / (1u64 << 53) as Real - 0.5) * 1e6,
            }
        }
    }

    fn same(a: &[Real], b: &[Real]) -> bool {
        a.iter()
            .zip(b)
            .all(|(x, y)| x.to_bits() == y.to_bits() || (x.is_nan() && y.is_nan()))
    }

    #[test]
    fn bitwise_the_serial_scatter_for_every_thread_count() {
        let mut rng = Lcg(7);
        for (nb_nodes, nb_rows, nnpe) in
            [(1, 3, 1), (50, 200, 4), (3000, 20_000, 4), (5000, 9000, 8)]
        {
            let conn: Vec<Idx> = (0..nb_rows * nnpe)
                .map(|_| (rng.next() % nb_nodes as u64) as Idx)
                .collect();
            let filter: Vec<Idx> = (0..nb_rows)
                .filter(|_| !rng.next().is_multiple_of(3))
                .map(|e| e as Idx)
                .rev()
                .collect();
            for f in [None, Some(filter.as_slice())] {
                let nb = f.map_or(nb_rows, <[Idx]>::len);
                let table = NodeScatter::new(&conn, nnpe, f, nb_nodes).unwrap();
                for d in [1, 2, 3, 5] {
                    let v: Vec<Real> = (0..nb * nnpe * d).map(|_| rng.real()).collect();
                    let o0: Vec<Real> = (0..nb_nodes * d).map(|_| rng.real()).collect();
                    let mut reference = o0.clone();
                    serial(&conn, nnpe, f, &v, d, -1.0, &mut reference);
                    for t in [1, 2, 3, 8] {
                        let exec =
                            Exec::new(ExecPolicy::serial().with_threads(t).unwrap()).unwrap();
                        assert!(table.matches(&exec, &conn, nnpe, f, nb_nodes));
                        let mut out = o0.clone();
                        table.scatter_add(&exec, &v, d, -1.0, &mut out).unwrap();
                        assert!(same(&out, &reference), "t={t} d={d} nodes={nb_nodes}");
                    }
                }
            }
        }
    }

    #[test]
    fn a_changed_row_is_detected() {
        let conn: Vec<Idx> = vec![0, 1, 2, 2, 3, 0];
        let exec = Exec::new(ExecPolicy::serial().with_threads(2).unwrap()).unwrap();
        let t = NodeScatter::new(&conn, 3, Some(&[1, 0]), 5).unwrap();
        assert!(t.matches(&exec, &conn, 3, Some(&[1, 0]), 5));
        let mut changed = conn.clone();
        changed[1] = 4;
        assert!(!t.matches(&exec, &changed, 3, Some(&[1, 0]), 5));
        assert!(!t.matches(&exec, &conn, 3, Some(&[0, 1]), 5));
        assert!(!t.matches(&exec, &conn, 3, Some(&[1]), 5));
        assert!(!t.matches(&exec, &conn, 3, Some(&[1, 0]), 6));
        assert!(!t.matches(&exec, &conn, 3, Some(&[1, 7]), 5));
        assert!(NodeScatter::new(&conn, 3, None, 3).is_err());
        assert!(NodeScatter::new(&conn, 3, Some(&[2]), 5).is_err());
        let mut out = vec![0.0; 4];
        assert!(t.scatter_add(&exec, &[0.0; 6], 1, 1.0, &mut out).is_err());
    }
}
