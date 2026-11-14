// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. Modified: translated from C++ to Rust (akantu-rs,
// 2026); the Row/Col structures live in separate vectors instead of the
// tail of the workspace `A`, with the same effective workspace length. The
// complete notice of every Eigen file this file derives from is reproduced
// verbatim below (MPL-2.0 §3.4; COLAMD notice retained as it requires).
//
// ---- Eigen/src/OrderingMethods/Eigen_Colamd.h
//      (colamd, recommended, set_defaults, compute_ordering, init_rows_cols, init_scoring, find_ordering, order_children, detect_super_cols, garbage_collection, clear_mark):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Desire Nuentsa Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   This file is modified from the colamd/symamd library. The copyright is below
//
//     The authors of the code itself are Stefan I. Larimore and Timothy A.
//     Davis (davis@cise.ufl.edu), University of Florida.  The algorithm was
//     developed in collaboration with John Gilbert, Xerox PARC, and Esmond
//     Ng, Oak Ridge National Laboratory.
//
//       Date:
//
//     September 8, 2003.  Version 2.3.
//
//       Acknowledgements:
//
//     This work was supported by the National Science Foundation, under
//     grants DMS-9504974 and DMS-9803599.
//
//       Notice:
//
//     Copyright (c) 1998-2003 by the University of Florida.
//     All Rights Reserved.
//
//     THIS MATERIAL IS PROVIDED AS IS, WITH ABSOLUTELY NO WARRANTY
//     EXPRESSED OR IMPLIED.  ANY USE IS AT YOUR OWN RISK.
//
//     Permission is hereby granted to use, copy, modify, and/or distribute
//     this program, provided that the Copyright, this License, and the
//     Availability of the original version is retained on all copies and made
//     accessible to the end-user of any code or package that includes COLAMD
//     or any modified version of COLAMD.
//
//       Availability:
//
//     The colamd/symamd library is available at
//
//         http://www.suitesparse.com
//
// ---- Eigen/src/OrderingMethods/Ordering.h
//      (COLAMDOrdering::operator()):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012  Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Column approximate minimum degree ordering (Eigen's `COLAMDOrdering`,
//! COLAMD 2.3), translated statement by statement. Integer arithmetic is
//! Eigen's `StorageIndex` (`int`, here `i32`), the hash is `unsigned int`
//! (`u32`), so tie-breaking, garbage collections and mark clearing happen
//! exactly as in the oracle.

// Statement-by-statement translation of index-based C++: index loops and
// explicit counters are kept so that a reader can follow Eigen's source line
// by line.
#![allow(clippy::needless_range_loop, clippy::explicit_counter_loop)]

use akantu_core::{Error, Result};

const EMPTY: i32 = -1;
const ALIVE: i32 = 0;
const DEAD: i32 = -1;
const DEAD_PRINCIPAL: i32 = -1;
const DEAD_NON_PRINCIPAL: i32 = -2;
/// `knobs[DenseRow]` and `knobs[DenseCol]` of `set_defaults`.
const DENSE_KNOB: f64 = 0.5;

/// `ColStructure` (the unions are single fields: `shared1` thickness /
/// parent, `shared2` score / order, `shared3` headhash / hash / prev,
/// `shared4` degree_next / hash_next).
#[derive(Clone, Copy, Debug, Default)]
struct Col {
    start: i32,
    length: i32,
    shared1: i32,
    shared2: i32,
    shared3: i32,
    shared4: i32,
}

impl Col {
    fn is_dead(&self) -> bool {
        self.start < ALIVE
    }
    fn is_alive(&self) -> bool {
        self.start >= ALIVE
    }
    fn is_dead_principal(&self) -> bool {
        self.start == DEAD_PRINCIPAL
    }
    fn kill_principal(&mut self) {
        self.start = DEAD_PRINCIPAL;
    }
    fn kill_non_principal(&mut self) {
        self.start = DEAD_NON_PRINCIPAL;
    }
}

/// `RowStructure` (`shared1` degree / p, `shared2` mark / first_column).
#[derive(Clone, Copy, Debug, Default)]
struct Row {
    start: i32,
    length: i32,
    shared1: i32,
    shared2: i32,
}

impl Row {
    fn is_dead(&self) -> bool {
        self.shared2 < ALIVE
    }
    fn is_alive(&self) -> bool {
        self.shared2 >= ALIVE
    }
    fn kill(&mut self) {
        self.shared2 = DEAD;
    }
}

/// `colamd_c(n_col)`: `(n_col + 1) * sizeof(ColStructure) / sizeof(int)`.
fn colamd_c(n_col: i64) -> i64 {
    (n_col + 1) * 6
}

/// `colamd_r(n_row)`: `(n_row + 1) * sizeof(RowStructure) / sizeof(int)`.
fn colamd_r(n_row: i64) -> i64 {
    (n_row + 1) * 4
}

/// `Colamd::recommended(nnz, n_row, n_col)`.
fn recommended(nnz: i64, n_row: i64, n_col: i64) -> i64 {
    2 * nnz + colamd_c(n_col) + colamd_r(n_row) + n_col + nnz / 5
}

fn ones_complement(r: i32) -> i32 {
    -r - 1
}

fn to_i32(v: i64, what: &'static str) -> Result<i32> {
    i32::try_from(v).map_err(|_| Error::CapacityExceeded {
        what,
        requested: u128::try_from(v).unwrap_or(u128::MAX),
        limit: i32::MAX as u128,
    })
}

/// `COLAMDOrdering<int>::operator()(mat, perm)` for the compressed column
/// pattern `(col_ptr, row_idx)` of an `n_row × n_col` matrix: returns
/// `perm.indices()`, i.e. `perm[original column] = new position`.
pub(crate) fn colamd_ordering(
    n_row: usize,
    n_col: usize,
    col_ptr: &[usize],
    row_idx: &[usize],
) -> Result<Vec<i64>> {
    let nnz = row_idx.len();
    let alen = recommended(nnz as i64, n_row as i64, n_col as i64);
    let alen = to_i32(alen, "COLAMD workspace (int)")?;
    let m = to_i32(n_row as i64, "matrix rows (int)")?;
    let n = to_i32(n_col as i64, "matrix columns (int)")?;
    let mut p = vec![0i32; n_col + 1];
    for (pi, &c) in p.iter_mut().zip(col_ptr) {
        *pi = to_i32(c as i64, "column pointer (int)")?;
    }
    // `IndexVector A(Alen)`: the first nnz entries are the row indices; the
    // rest is uninitialised in Eigen and never read before being written.
    let mut a = vec![0i32; usize::try_from(alen).unwrap_or(0)];
    for (ai, &r) in a.iter_mut().zip(row_idx) {
        *ai = r as i32;
    }
    compute_ordering(m, n, alen, &mut a, &mut p)?;
    let mut perm = vec![0i64; n_col];
    for i in 0..n_col {
        perm[p[i] as usize] = i as i64;
    }
    Ok(perm)
}

/// `Colamd::compute_ordering` with the default knobs; on return `p[k]` is
/// the original column placed at position `k`.
fn compute_ordering(n_row: i32, n_col: i32, alen: i32, a: &mut [i32], p: &mut [i32]) -> Result<()> {
    let nnz = p[n_col as usize];
    if nnz < 0 || p[0] != 0 {
        return Err(Error::invalid("COLAMD: invalid column pointers"));
    }
    let col_size = colamd_c(i64::from(n_col));
    let row_size = colamd_r(i64::from(n_row));
    let need = 2 * i64::from(nnz) + i64::from(n_col) + col_size + row_size;
    if need > i64::from(alen) {
        return Err(Error::invalid("COLAMD: workspace too small"));
    }
    // Alen -= Col_size + Row_size (the structures are kept separately).
    let alen_eff = (i64::from(alen) - col_size - row_size) as i32;
    let mut col = vec![Col::default(); n_col as usize + 1];
    let mut row = vec![Row::default(); n_row as usize + 1];
    init_rows_cols(n_row, n_col, &mut row, &mut col, a, p)?;
    let (n_col2, max_deg) = init_scoring(n_row, n_col, &mut row, &mut col, a, p);
    find_ordering(
        n_row,
        n_col,
        alen_eff,
        &mut row,
        &mut col,
        a,
        p,
        n_col2,
        max_deg,
        2 * nnz,
    );
    order_children(n_col, &col, p);
    Ok(())
}

fn init_rows_cols(
    n_row: i32,
    n_col: i32,
    row: &mut [Row],
    col: &mut [Col],
    a: &mut [i32],
    p: &mut [i32],
) -> Result<()> {
    for c in 0..n_col as usize {
        col[c].start = p[c];
        col[c].length = p[c + 1] - p[c];
        if col[c].length < 0 {
            return Err(Error::invalid(
                "COLAMD: column pointers must be non-decreasing",
            ));
        }
        col[c].shared1 = 1; // thickness
        col[c].shared2 = 0; // score
        col[c].shared3 = EMPTY; // prev
        col[c].shared4 = EMPTY; // degree_next
    }
    let mut jumbled = false;
    for r in row.iter_mut().take(n_row as usize) {
        r.length = 0;
        r.shared2 = -1; // mark
    }
    for c in 0..n_col {
        let mut last_row = -1;
        let (b, e) = (p[c as usize] as usize, p[c as usize + 1] as usize);
        for k in b..e {
            let r = a[k];
            if r < 0 || r >= n_row {
                return Err(Error::invalid("COLAMD: row index out of bounds"));
            }
            let ru = r as usize;
            if r <= last_row || row[ru].shared2 == c {
                jumbled = true;
            }
            if row[ru].shared2 != c {
                row[ru].length += 1;
            } else {
                col[c as usize].length -= 1;
            }
            row[ru].shared2 = c;
            last_row = r;
        }
    }
    // row pointers: the row form starts after the column form
    if n_row > 0 || !row.is_empty() {
        row[0].start = p[n_col as usize];
        row[0].shared1 = row[0].start; // p
        row[0].shared2 = -1;
    }
    for r in 1..n_row as usize {
        row[r].start = row[r - 1].start + row[r - 1].length;
        row[r].shared1 = row[r].start;
        row[r].shared2 = -1;
    }
    // create the row form
    if jumbled {
        for c in 0..n_col {
            let (b, e) = (p[c as usize] as usize, p[c as usize + 1] as usize);
            for k in b..e {
                let ru = a[k] as usize;
                if row[ru].shared2 != c {
                    let dst = row[ru].shared1 as usize;
                    a[dst] = c;
                    row[ru].shared1 += 1;
                    row[ru].shared2 = c;
                }
            }
        }
    } else {
        for c in 0..n_col {
            let (b, e) = (p[c as usize] as usize, p[c as usize + 1] as usize);
            for k in b..e {
                let ru = a[k] as usize;
                let dst = row[ru].shared1 as usize;
                a[dst] = c;
                row[ru].shared1 += 1;
            }
        }
    }
    // clear the row marks and set row degrees
    for r in row.iter_mut().take(n_row as usize) {
        r.shared2 = 0;
        r.shared1 = r.length; // degree
    }
    if jumbled {
        // re-create the column form (pruned of duplicates)
        col[0].start = 0;
        p[0] = col[0].start;
        for c in 1..n_col as usize {
            col[c].start = col[c - 1].start + col[c - 1].length;
            p[c] = col[c].start;
        }
        for r in 0..n_row as usize {
            let b = row[r].start as usize;
            let e = b + row[r].length as usize;
            for k in b..e {
                let c = a[k] as usize;
                let dst = p[c] as usize;
                a[dst] = r as i32;
                p[c] += 1;
            }
        }
    }
    Ok(())
}

/// Returns `(n_col2, max_deg)`.
fn init_scoring(
    n_row: i32,
    n_col: i32,
    row: &mut [Row],
    col: &mut [Col],
    a: &mut [i32],
    head: &mut [i32],
) -> (i32, i32) {
    // `IndexType(knobs * n)`: truncation toward zero.
    let dense_row_count = 0.max(((DENSE_KNOB * f64::from(n_col)) as i32).min(n_col));
    let dense_col_count = 0.max(((DENSE_KNOB * f64::from(n_row)) as i32).min(n_row));
    let mut max_deg = 0;
    let mut n_col2 = n_col;
    // kill empty columns (ordered last, natural order)
    for c in (0..n_col as usize).rev() {
        if col[c].length == 0 {
            n_col2 -= 1;
            col[c].shared2 = n_col2; // order
            col[c].kill_principal();
        }
    }
    // kill dense columns
    for c in (0..n_col as usize).rev() {
        if col[c].is_dead() {
            continue;
        }
        if col[c].length > dense_col_count {
            n_col2 -= 1;
            col[c].shared2 = n_col2;
            let b = col[c].start as usize;
            for k in b..b + col[c].length as usize {
                row[a[k] as usize].shared1 -= 1;
            }
            col[c].kill_principal();
        }
    }
    // kill dense and empty rows
    for r in row.iter_mut().take(n_row as usize) {
        let deg = r.shared1;
        if deg > dense_row_count || deg == 0 {
            r.kill();
        } else {
            max_deg = max_deg.max(deg);
        }
    }
    // initial column scores
    for c in (0..n_col as usize).rev() {
        if col[c].is_dead() {
            continue;
        }
        let mut score: i32 = 0;
        let start = col[c].start as usize;
        let end = start + col[c].length as usize;
        let mut new_cp = start;
        for cp in start..end {
            let r = a[cp];
            if row[r as usize].is_dead() {
                continue;
            }
            a[new_cp] = r;
            new_cp += 1;
            score += row[r as usize].shared1 - 1;
            score = score.min(n_col);
        }
        let col_length = (new_cp - start) as i32;
        if col_length == 0 {
            n_col2 -= 1;
            col[c].shared2 = n_col2;
            col[c].kill_principal();
        } else {
            col[c].length = col_length;
            col[c].shared2 = score;
        }
    }
    // degree lists
    for h in head.iter_mut().take(n_col as usize + 1) {
        *h = EMPTY;
    }
    for c in (0..n_col as usize).rev() {
        if col[c].is_alive() {
            let score = col[c].shared2 as usize;
            let next_col = head[score];
            col[c].shared3 = EMPTY;
            col[c].shared4 = next_col;
            if next_col != EMPTY {
                col[next_col as usize].shared3 = c as i32;
            }
            head[score] = c as i32;
        }
    }
    (n_col2, max_deg)
}

#[allow(clippy::too_many_arguments)] // translation of COLAMD's find_ordering signature
fn find_ordering(
    n_row: i32,
    n_col: i32,
    alen: i32,
    row: &mut [Row],
    col: &mut [Col],
    a: &mut [i32],
    head: &mut [i32],
    n_col2: i32,
    mut max_deg: i32,
    mut pfree: i32,
) {
    let max_mark = i32::MAX - n_col;
    let mut tag_mark = clear_mark(n_row, row);
    let mut min_score: i32 = 0;
    let mut k: i32 = 0;
    while k < n_col2 {
        // select pivot column from the head of the minimum degree list
        while min_score < n_col && head[min_score as usize] == EMPTY {
            min_score += 1;
        }
        let pivot_col = head[min_score as usize] as usize;
        let next_col = col[pivot_col].shared4;
        head[min_score as usize] = next_col;
        if next_col != EMPTY {
            col[next_col as usize].shared3 = EMPTY;
        }
        let pivot_col_score = col[pivot_col].shared2;
        col[pivot_col].shared2 = k; // order
        let pivot_col_thickness = col[pivot_col].shared1;
        k += pivot_col_thickness;
        // garbage collection, if necessary
        let needed_memory = pivot_col_score.min(n_col - k);
        if pfree + needed_memory >= alen {
            pfree = garbage_collection(n_row, n_col, row, col, a, pfree);
            tag_mark = clear_mark(n_row, row);
        }
        // pivot row pattern
        let pivot_row_start = pfree;
        let mut pivot_row_degree: i32 = 0;
        col[pivot_col].shared1 = -pivot_col_thickness;
        let cs = col[pivot_col].start as usize;
        let ce = cs + col[pivot_col].length as usize;
        for cp in cs..ce {
            let r = a[cp] as usize;
            if row[r].is_dead() {
                continue;
            }
            let rs = row[r].start as usize;
            let re = rs + row[r].length as usize;
            for rp in rs..re {
                let c = a[rp] as usize;
                let col_thickness = col[c].shared1;
                if col_thickness > 0 && col[c].is_alive() {
                    col[c].shared1 = -col_thickness;
                    a[pfree as usize] = c as i32;
                    pfree += 1;
                    pivot_row_degree += col_thickness;
                }
            }
        }
        col[pivot_col].shared1 = pivot_col_thickness;
        max_deg = max_deg.max(pivot_row_degree);
        // kill all rows used to construct the pivot row
        for cp in cs..ce {
            let r = a[cp] as usize;
            row[r].kill();
        }
        let pivot_row_length = pfree - pivot_row_start;
        let pivot_row = if pivot_row_length > 0 {
            a[col[pivot_col].start as usize]
        } else {
            EMPTY
        };
        // set differences
        let prs = pivot_row_start as usize;
        let pre = prs + pivot_row_length as usize;
        for rp in prs..pre {
            let c = a[rp] as usize;
            let col_thickness = -col[c].shared1;
            col[c].shared1 = col_thickness;
            // remove the column from its degree list
            let cur_score = col[c].shared2;
            let prev_col = col[c].shared3;
            let next_col = col[c].shared4;
            if prev_col == EMPTY {
                head[cur_score as usize] = next_col;
            } else {
                col[prev_col as usize].shared4 = next_col;
            }
            if next_col != EMPTY {
                col[next_col as usize].shared3 = prev_col;
            }
            // scan the column
            let s = col[c].start as usize;
            let e = s + col[c].length as usize;
            for cp in s..e {
                let r = a[cp] as usize;
                if row[r].is_dead() {
                    continue;
                }
                let row_mark = row[r].shared2;
                let mut set_difference = row_mark - tag_mark;
                if set_difference < 0 {
                    set_difference = row[r].shared1;
                }
                set_difference -= col_thickness;
                if set_difference == 0 {
                    row[r].kill();
                } else {
                    row[r].shared2 = set_difference + tag_mark;
                }
            }
        }
        // add up set differences for each column
        for rp in prs..pre {
            let c = a[rp] as usize;
            let mut hash: u32 = 0;
            let mut cur_score: i32 = 0;
            let s = col[c].start as usize;
            let e = s + col[c].length as usize;
            let mut new_cp = s;
            for cp in s..e {
                let r = a[cp];
                if row[r as usize].is_dead() {
                    continue;
                }
                let row_mark = row[r as usize].shared2;
                a[new_cp] = r;
                new_cp += 1;
                hash = hash.wrapping_add(r as u32);
                cur_score += row_mark - tag_mark;
                cur_score = cur_score.min(n_col);
            }
            col[c].length = (new_cp - s) as i32;
            if col[c].length == 0 {
                // mass elimination
                col[c].kill_principal();
                pivot_row_degree -= col[c].shared1;
                col[c].shared2 = k;
                k += col[c].shared1;
            } else {
                col[c].shared2 = cur_score;
                hash %= (n_col + 1) as u32;
                let head_column = head[hash as usize];
                let first_col;
                if head_column > EMPTY {
                    first_col = col[head_column as usize].shared3;
                    col[head_column as usize].shared3 = c as i32;
                } else {
                    first_col = -(head_column + 2);
                    head[hash as usize] = -(c as i32 + 2);
                }
                col[c].shared4 = first_col;
                col[c].shared3 = hash as i32;
            }
        }
        detect_super_cols(col, a, head, pivot_row_start, pivot_row_length);
        col[pivot_col].kill_principal();
        tag_mark += max_deg + 1;
        if tag_mark >= max_mark {
            tag_mark = clear_mark(n_row, row);
        }
        // finalize the new pivot row and the column scores
        let mut new_rp = prs;
        for rp in prs..pre {
            let c = a[rp];
            let cu = c as usize;
            if col[cu].is_dead() {
                continue;
            }
            a[new_rp] = c;
            new_rp += 1;
            let at = (col[cu].start + col[cu].length) as usize;
            a[at] = pivot_row;
            col[cu].length += 1;
            let mut cur_score = col[cu].shared2 + pivot_row_degree;
            let max_score = n_col - k - col[cu].shared1;
            cur_score -= col[cu].shared1;
            cur_score = cur_score.min(max_score);
            col[cu].shared2 = cur_score;
            let next_col = head[cur_score as usize];
            col[cu].shared4 = next_col;
            col[cu].shared3 = EMPTY;
            if next_col != EMPTY {
                col[next_col as usize].shared3 = c;
            }
            head[cur_score as usize] = c;
            min_score = min_score.min(cur_score);
        }
        if pivot_row_degree > 0 {
            let pr = pivot_row as usize;
            row[pr].start = pivot_row_start;
            row[pr].length = (new_rp - prs) as i32;
            row[pr].shared1 = pivot_row_degree;
            row[pr].shared2 = 0;
        }
    }
}

fn order_children(n_col: i32, col: &[Col], p: &mut [i32]) {
    // `Col` is mutated through parent links: work on a local copy of the
    // two fields involved (order, parent) with the principal flags.
    let n = n_col as usize;
    let mut order: Vec<i32> = col.iter().map(|c| c.shared2).collect();
    let mut parent_of: Vec<i32> = col.iter().map(|c| c.shared1).collect();
    for i in 0..n {
        if !col[i].is_dead_principal() && order[i] == EMPTY {
            let mut parent = i;
            loop {
                parent = parent_of[parent] as usize;
                if col[parent].is_dead_principal() {
                    break;
                }
            }
            let mut c = i;
            let mut ord = order[parent];
            loop {
                order[c] = ord;
                ord += 1;
                parent_of[c] = parent as i32;
                c = parent_of[c] as usize;
                if order[c] != EMPTY {
                    break;
                }
            }
            order[parent] = ord;
        }
    }
    for c in 0..n {
        p[order[c] as usize] = c as i32;
    }
}

fn detect_super_cols(
    col: &mut [Col],
    a: &[i32],
    head: &mut [i32],
    row_start: i32,
    row_length: i32,
) {
    let rs = row_start as usize;
    for rp in rs..rs + row_length as usize {
        let cidx = a[rp] as usize;
        if col[cidx].is_dead() {
            continue;
        }
        let hash = col[cidx].shared3 as usize;
        let head_column = head[hash];
        let first_col = if head_column > EMPTY {
            col[head_column as usize].shared3
        } else {
            -(head_column + 2)
        };
        let mut super_c = first_col;
        while super_c != EMPTY {
            let sc = super_c as usize;
            let length = col[sc].length;
            let mut prev_c = super_c;
            let mut c = col[sc].shared4;
            while c != EMPTY {
                let cu = c as usize;
                if col[cu].length != length || col[cu].shared2 != col[sc].shared2 {
                    prev_c = c;
                    c = col[cu].shared4;
                    continue;
                }
                let (s1, s2) = (col[sc].start as usize, col[cu].start as usize);
                let mut i = 0;
                while i < length as usize {
                    if a[s1 + i] != a[s2 + i] {
                        break;
                    }
                    i += 1;
                }
                if i != length as usize {
                    prev_c = c;
                    c = col[cu].shared4;
                    continue;
                }
                // identical: absorb c into super_c
                col[sc].shared1 += col[cu].shared1;
                col[cu].shared1 = super_c; // parent
                col[cu].kill_non_principal();
                col[cu].shared2 = EMPTY; // order
                let next = col[cu].shared4;
                col[prev_c as usize].shared4 = next;
                c = next;
            }
            super_c = col[sc].shared4;
        }
        if head_column > EMPTY {
            col[head_column as usize].shared3 = EMPTY;
        } else {
            head[hash] = EMPTY;
        }
    }
}

/// Returns the new `pfree`.
fn garbage_collection(
    n_row: i32,
    n_col: i32,
    row: &mut [Row],
    col: &mut [Col],
    a: &mut [i32],
    pfree: i32,
) -> i32 {
    let mut pdest: usize = 0;
    for c in 0..n_col as usize {
        if col[c].is_alive() {
            let mut psrc = col[c].start as usize;
            col[c].start = pdest as i32;
            let length = col[c].length;
            for _ in 0..length {
                let r = a[psrc];
                psrc += 1;
                if row[r as usize].is_alive() {
                    a[pdest] = r;
                    pdest += 1;
                }
            }
            col[c].length = (pdest - col[c].start as usize) as i32;
        }
    }
    for r in 0..n_row as usize {
        if row[r].is_alive() {
            if row[r].length == 0 {
                row[r].kill();
            } else {
                let psrc = row[r].start as usize;
                row[r].shared2 = a[psrc]; // first_column
                a[psrc] = ones_complement(r as i32);
            }
        }
    }
    let mut psrc = pdest;
    while psrc < pfree as usize {
        let v = a[psrc];
        psrc += 1;
        if v < 0 {
            psrc -= 1;
            let r = ones_complement(a[psrc]) as usize;
            a[psrc] = row[r].shared2;
            row[r].start = pdest as i32;
            let length = row[r].length;
            for _ in 0..length {
                let c = a[psrc];
                psrc += 1;
                if col[c as usize].is_alive() {
                    a[pdest] = c;
                    pdest += 1;
                }
            }
            row[r].length = (pdest - row[r].start as usize) as i32;
        }
    }
    pdest as i32
}

fn clear_mark(n_row: i32, row: &mut [Row]) -> i32 {
    for r in row.iter_mut().take(n_row as usize) {
        if r.is_alive() {
            r.shared2 = 0;
        }
    }
    1
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn small_orderings_are_permutations() {
        // arrow matrix: the dense column (last) is ordered last
        let n = 6;
        let mut col_ptr = vec![0];
        let mut rows = Vec::new();
        for c in 0..n {
            if c + 1 < n {
                rows.extend([c, n - 1]);
            } else {
                rows.extend(0..n);
            }
            col_ptr.push(rows.len());
        }
        let p = colamd_ordering(n, n, &col_ptr, &rows).unwrap();
        let mut seen = p.clone();
        seen.sort_unstable();
        assert_eq!(seen, (0..n as i64).collect::<Vec<_>>());
        assert_eq!(p[n - 1], n as i64 - 1);
        // empty matrix
        assert!(colamd_ordering(0, 0, &[0], &[]).unwrap().is_empty());
    }
}
