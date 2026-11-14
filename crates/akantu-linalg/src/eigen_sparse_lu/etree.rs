// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. Modified: translated from C++ to Rust (akantu-rs,
// 2026). The complete notice of every Eigen file this file derives from is
// reproduced verbatim below (MPL-2.0 §3.4; the SuperLU/Xerox notice is
// retained and this notice states that the code was modified, as it
// requires).
//
// ---- Eigen/src/SparseCore/SparseColEtree.h
//      (etree_find, coletree, nr_etdfs, treePostorder):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//
//   /*
//
//    * NOTE: This file is the modified version of sp_coletree.c file in SuperLU
//
//    * -- SuperLU routine (version 3.1) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * August 1, 2008
//    *
//    * Copyright (c) 1994 by Xerox Corporation.  All rights reserved.
//    *
//    * THIS MATERIAL IS PROVIDED AS IS, WITH ABSOLUTELY NO WARRANTY
//    * EXPRESSED OR IMPLIED.  ANY USE IS AT YOUR OWN RISK.
//    *
//    * Permission is hereby granted to use or copy this program for any
//    * purpose, provided the above notices are retained on all copies.
//    * Permission to modify the code and to distribute modified code is
//    * granted, provided the above notices are retained, and a notice that
//    * the code was modified is included with the above copyright notice.
//    */
//! Column elimination tree and its postorder (Eigen's `coletree`,
//! `treePostorder`), on the column-permuted pattern used by `SparseLU`.

/// `etree_find` with path halving.
fn etree_find(mut i: usize, pp: &mut [usize]) -> usize {
    let mut p = pp[i];
    let mut gp = pp[p];
    while gp != p {
        pp[i] = gp;
        i = gp;
        p = pp[i];
        gp = pp[p];
    }
    p
}

/// `coletree(mat, parent, firstRowElt)` for the column pattern given by
/// `col_rows(c)` (row indices of column `c`, increasing); returns `parent`
/// (`n` for roots).
pub(crate) fn coletree<'a>(
    m: usize,
    n: usize,
    col_rows: impl Fn(usize) -> &'a [usize],
) -> Vec<usize> {
    let diag_size = m.min(n);
    let mut root = vec![0usize; n];
    let mut pp = vec![0usize; n];
    let mut parent = vec![0usize; n];
    let mut first_row_elt: Vec<usize> = (0..m).map(|r| if r < diag_size { r } else { n }).collect();
    for col in 0..n {
        for &row in col_rows(col) {
            first_row_elt[row] = first_row_elt[row].min(col);
        }
    }
    for col in 0..n {
        let mut found_diag = col >= m;
        pp[col] = col;
        let mut cset = col;
        root[cset] = col;
        parent[col] = n;
        let rows = col_rows(col);
        let mut k = 0;
        // The diagonal is processed even if it is not stored: once more
        // after the column's entries when it was not met.
        while k < rows.len() || !found_diag {
            let i = if k < rows.len() { rows[k] } else { col };
            k += 1;
            if i == col {
                found_diag = true;
            }
            let row = first_row_elt[i];
            if row >= col {
                continue;
            }
            let rset = etree_find(row, &mut pp);
            let rroot = root[rset];
            if rroot != col {
                parent[rroot] = col;
                pp[cset] = rset;
                cset = rset;
                root[cset] = col;
            }
        }
    }
    parent
}

/// `treePostorder(n, parent, post)`: `post` has `n + 1` entries (the dummy
/// root `n` is numbered `n`).
pub(crate) fn tree_postorder(n: usize, parent: &[usize]) -> Vec<usize> {
    const NONE: usize = usize::MAX;
    let mut first_kid = vec![NONE; n + 1];
    let mut next_kid = vec![0usize; n + 1];
    let mut post = vec![0usize; n + 1];
    for v in (0..n).rev() {
        let dad = parent[v];
        next_kid[v] = first_kid[dad];
        first_kid[dad] = v;
    }
    // nr_etdfs: depth-first search from the dummy root n, no recursion
    let mut postnum = 0usize;
    let mut current = n;
    while postnum != n {
        let first = first_kid[current];
        if first == NONE {
            post[current] = postnum;
            postnum += 1;
            let mut next = next_kid[current];
            while next == NONE {
                current = parent[current];
                post[current] = postnum;
                postnum += 1;
                next = next_kid[current];
            }
            if postnum == n + 1 {
                return post;
            }
            current = next;
        } else {
            current = first;
        }
    }
    post
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chain_and_postorder() {
        // tridiagonal 4x4: etree is a chain 0 -> 1 -> 2 -> 3
        let cols: Vec<Vec<usize>> = vec![vec![0, 1], vec![0, 1, 2], vec![1, 2, 3], vec![2, 3]];
        let parent = coletree(4, 4, |c| &cols[c]);
        assert_eq!(parent, [1, 2, 3, 4]);
        let post = tree_postorder(4, &parent);
        assert_eq!(post, [0, 1, 2, 3, 4]);
        // diagonal: forest of roots, postorder is the identity
        let d: Vec<Vec<usize>> = (0..3).map(|i| vec![i]).collect();
        let parent = coletree(3, 3, |c| &d[c]);
        assert_eq!(parent, [3, 3, 3]);
        assert_eq!(tree_postorder(3, &parent), [0, 1, 2, 3]);
    }
}
