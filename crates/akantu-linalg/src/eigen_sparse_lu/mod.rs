// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. Modified: translated from C++ to Rust (akantu-rs,
// 2026); errors are returned as values instead of `m_info`/asserts. The
// backend type `EigenSparseLuSolver` reproduces Akantu's
// `SparseSolverEigen` (src/solver/sparse_solver_eigen.cc, LGPL-3.0-or-later,
// EPFL/LSMS) on top of it. The complete notice of every Eigen file this
// file derives from is reproduced verbatim below (MPL-2.0 §3.4).
//
// ---- Eigen/src/SparseLU/SparseLU.h
//      (SparseLU::analyzePattern, factorize, _solve_impl, initperfvalues, SparseLUMatrixUReturnType::solveInPlace):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//   Copyright (C) 2012-2014 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/SparseLU/SparseLUImpl.h
//      (SparseLUImpl):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/SparseLU/SparseLU_SupernodalMatrix.h
//      (MappedSuperNodalMatrix::solveInPlace, InnerIterator):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//   Copyright (C) 2012 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
// ---- Eigen/src/Core/PermutationMatrix.h
//      (permutation products and inverse):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2009 Benoit Jacob <jacob.benoit.1@gmail.com>
//   Copyright (C) 2009-2015 Gael Guennebaud <gael.guennebaud@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! Translation of Eigen 3.4.0's `SparseLU<SparseMatrix<double>,
//! COLAMDOrdering<int>>` (SOLV-004), the factorisation behind upstream
//! `SparseSolverEigen`, reproducing the oracle's results **bit for bit** in
//! its strict configuration (`-ffp-contract=off -DEIGEN_DONT_VECTORIZE`,
//! aarch64 cache defaults): same COLAMD ordering and elimination-tree
//! postorder, same supernodes, panels (16 columns), relaxed supernodes
//! (`relax = 1`), threshold partial pivoting (`diagpivotthresh = 1`), same
//! dense kernel order (module `dense`). Evidence:
//! `tests/eigen_sparse_lu_oracle.rs` against `fixtures/solver/*`
//! (permutations, elimination tree, every factor array and solution).

mod colamd;
mod dense;
mod etree;
mod kernels;
// Run-time instruction-set dispatch of the dense kernels: calls of
// `#[target_feature]` functions after the CPU check (`UNSAFE.md`).
#[allow(unsafe_code)]
mod simd;

use akantu_core::enums::{MatrixType, SparseSolverType};
use akantu_core::{Error, Real, Result};

use crate::direct::{
    BackendCapabilities, CscMatrix, SparseDirectSolver, arithmetic_range_failure,
    centralized_capabilities, check_finite_matrix, check_finite_rhs, singular_matrix,
};
use crate::sparse::SparseMatrixAij;
use kernels::{EMPTY, Glu, PermutedCols, Work};

/// `initperfvalues()`.
const PANEL_SIZE: usize = kernels::SPA_W;
const RELAX: i64 = 1;
const MAXSUPER: i64 = 128;
/// `m_diagpivotthresh` default.
const DIAG_PIVOT_THRESH: f64 = 1.0;

/// Factor arrays of a successful factorisation (for differential checks).
#[derive(Clone, Debug, Default)]
pub struct SparseLuFactors {
    /// `rowsPermutation().indices()`.
    pub perm_r: Vec<i64>,
    /// `glu.xsup[0..=nsuper+1]`.
    pub xsup: Vec<i64>,
    /// `glu.supno[0..=n]`.
    pub supno: Vec<i64>,
    /// `glu.xlsub` (after `fixupL`).
    pub xlsub: Vec<i64>,
    /// `glu.lsub[0..xlsub[n]]`.
    pub lsub: Vec<i64>,
    /// `glu.xlusup`.
    pub xlusup: Vec<i64>,
    /// `glu.lusup[0..xlusup[n]]`.
    pub lusup: Vec<f64>,
    /// `glu.xusub`.
    pub xusub: Vec<i64>,
    /// `glu.usub[0..xusub[n]]`.
    pub usub: Vec<i64>,
    /// `glu.ucol[0..xusub[n]]`.
    pub ucol: Vec<f64>,
    /// `nnzL()`.
    pub nnz_l: i64,
    /// `nnzU()`.
    pub nnz_u: i64,
}

/// Eigen's `SparseLU` (non-symmetric mode, COLAMD ordering).
#[derive(Clone, Debug, Default)]
pub struct SparseLu {
    n: usize,
    /// `m_perm_c.indices()` after `analyzePattern`.
    perm_c: Vec<i64>,
    /// `m_etree` renumbered in postorder (`n + 1` entries, the last unused).
    etree: Vec<i64>,
    /// Pattern analysed (column pointers, row indices).
    pattern: Option<(Vec<usize>, Vec<usize>)>,
    factors: Option<(Vec<i64>, Glu)>,
    /// The work arrays of the last factorisation and the factor storage of
    /// the last invalidated factorisation, kept for the next one (no fresh
    /// allocation and page faults per factorisation; no effect on results).
    work: Option<Box<Work>>,
    spare_glu: Option<Glu>,
}

impl SparseLu {
    /// Drops the factorisation, keeping its storage for the next one.
    fn invalidate(&mut self) {
        if let Some((_, g)) = self.factors.take() {
            self.spare_glu = Some(g);
        }
    }

    /// `analyzePattern(mat)` on the compressed column matrix `a`.
    pub fn analyze_pattern(&mut self, a: &CscMatrix) -> Result<()> {
        self.invalidate();
        self.pattern = None;
        let n = a.n();
        if i32::try_from(n).is_err() || i32::try_from(a.nnz()).is_err() {
            return Err(Error::CapacityExceeded {
                what: "Eigen SparseLU order or nonzeros (int storage index)",
                requested: n.max(a.nnz()) as u128,
                limit: i32::MAX as u128,
            });
        }
        let perm_c = colamd::colamd_ordering(n, n, a.col_ptr(), a.row_idx())?;
        // column perm_c[i] of the permuted matrix is the original column i
        let mut iperm = vec![0usize; n];
        for (i, &p) in perm_c.iter().enumerate() {
            iperm[p as usize] = i;
        }
        let etree = etree::coletree(n, n, |c| {
            let o = iperm[c];
            &a.row_idx()[a.col_ptr()[o]..a.col_ptr()[o + 1]]
        });
        let post = etree::tree_postorder(n, &etree);
        let mut iwork = vec![0i64; n + 1];
        for i in 0..n {
            iwork[post[i]] = post[etree[i]] as i64;
        }
        // m_perm_c = post_perm * m_perm_c: indices(i) = post(perm_c(i))
        let perm_c: Vec<i64> = perm_c.iter().map(|&p| post[p as usize] as i64).collect();
        self.n = n;
        self.perm_c = perm_c;
        self.etree = iwork;
        self.pattern = Some((a.col_ptr().to_vec(), a.row_idx().to_vec()));
        Ok(())
    }

    /// `colsPermutation().indices()`.
    #[must_use]
    pub fn perm_c(&self) -> &[i64] {
        &self.perm_c
    }

    /// The postordered column elimination tree (`n` entries).
    #[must_use]
    pub fn etree(&self) -> &[i64] {
        &self.etree[..self.n.min(self.etree.len())]
    }

    /// `factorize(matrix)`: numerical factorisation with the analysed
    /// pattern. A zero pivot column is upstream's "THE MATRIX IS
    /// STRUCTURALLY SINGULAR ... ZERO COLUMN AT k" (`m_info =
    /// NumericalIssue`), reported as a singular-matrix error.
    ///
    /// Port policy outside Eigen's arithmetic (UD-183;
    /// pure inspection, so valid finite input is factorised bit for bit as
    /// by Eigen): a non-finite matrix entry is refused before any
    /// arithmetic, and factors holding a non-finite entry produced from
    /// finite input (e.g. the reciprocal of a subnormal pivot multiplying
    /// the entries below it) are an arithmetic range failure; both are
    /// [`Error::Numerical`]. Every failure leaves no factorisation.
    pub fn factorize(&mut self, a: &CscMatrix) -> Result<()> {
        self.invalidate();
        let Some((cp, ri)) = &self.pattern else {
            return Err(Error::LinearSolver(
                "factorize() called before analyzePattern()".into(),
            ));
        };
        if a.n() != self.n || a.col_ptr() != cp.as_slice() || a.row_idx() != ri.as_slice() {
            return Err(Error::LinearSolver(
                "the matrix pattern differs from the analysed one (re-analyse first)".into(),
            ));
        }
        check_finite_matrix("Eigen SparseLU", a)?;
        let (m, n) = (self.n, self.n);
        let mut iperm_c = vec![0i64; n];
        for (i, &p) in self.perm_c.iter().enumerate() {
            iperm_c[p as usize] = i as i64;
        }
        // reuse the work arrays and the factor storage of the previous
        // factorisation (reinitialised; the factor values are rewritten)
        let mut glu = self.spare_glu.take().unwrap_or_default();
        glu.reset(n);
        let mut wk = self.work.take().unwrap_or_default();
        wk.reset(m, n, PANEL_SIZE, MAXSUPER as usize);
        let mut relax_end = vec![EMPTY; n];
        kernels::relax_snode(n, &self.etree, RELAX, &mut relax_end);
        let mut perm_r = vec![EMPTY; m];
        let pc = PermutedCols {
            col_ptr: a.col_ptr(),
            row_idx: a.row_idx(),
            values: a.values(),
            iperm_c: &iperm_c,
        };
        let mut jcol = 0usize;
        while jcol < n {
            // adjust the panel so that it does not overlap the next relaxed snode
            let mut panel_size = PANEL_SIZE;
            let mut k = jcol + 1;
            while k < (jcol + panel_size).min(n) {
                if relax_end[k] != EMPTY {
                    panel_size = k - jcol;
                    break;
                }
                k += 1;
            }
            if k == n {
                panel_size = n - jcol;
            }
            let nseg1 = kernels::panel_dfs(m, panel_size, jcol, &pc, &perm_r, &mut wk, &mut glu);
            kernels::panel_bmod(m, panel_size, jcol, nseg1, &mut wk, &mut glu);
            for jj in jcol..jcol + panel_size {
                let col_off = (jj - jcol) * m;
                let mut nseg = nseg1;
                kernels::column_dfs(
                    m, jj, &perm_r, MAXSUPER, &mut nseg, col_off, &mut wk, &mut glu,
                );
                kernels::column_bmod(
                    jj,
                    nseg - nseg1,
                    nseg1 as usize,
                    col_off,
                    jj - jcol,
                    jcol,
                    &mut wk,
                    &mut glu,
                );
                kernels::copy_to_ucol(jj, nseg, col_off, jj - jcol, &perm_r, &mut wk, &mut glu);
                let pivrow = match kernels::pivot_l(
                    jj,
                    DIAG_PIVOT_THRESH,
                    &mut perm_r,
                    &iperm_c,
                    &mut glu,
                ) {
                    Ok(p) => p,
                    Err(info) => {
                        return Err(singular_matrix(format!(
                            "Eigen SparseLU: THE MATRIX IS STRUCTURALLY SINGULAR ... ZERO COLUMN AT {info}"
                        )));
                    }
                };
                kernels::prune_l(jj, &perm_r, pivrow, nseg, col_off, &mut wk, &mut glu);
                for i in 0..nseg as usize {
                    let irep = wk.segrep[i] as usize;
                    wk.repfnz[col_off + irep] = EMPTY;
                }
            }
            jcol += panel_size;
        }
        kernels::fixup_l(n, &perm_r, &mut glu);
        let used_l = usize::try_from(glu.xlusup[n]).unwrap_or(0);
        let used_u = usize::try_from(glu.xusub[n]).unwrap_or(0);
        if let Some(v) = glu.lusup[..used_l]
            .iter()
            .chain(&glu.ucol[..used_u])
            .find(|v| !v.is_finite())
        {
            return Err(arithmetic_range_failure(
                "Eigen SparseLU",
                format_args!("factor entry {v}"),
            ));
        }
        self.factors = Some((perm_r, glu));
        self.work = Some(wk);
        Ok(())
    }

    /// Copies of the factor arrays (trimmed to their used lengths).
    #[must_use]
    pub fn factors(&self) -> Option<SparseLuFactors> {
        let (perm_r, g) = self.factors.as_ref()?;
        let n = self.n;
        let (nnz_l, nnz_u) = kernels::countnz(n, g);
        let nsuper = if n > 0 { g.supno[n] } else { -1 };
        let take = |v: &[i64], len: i64| v[..usize::try_from(len).unwrap_or(0)].to_vec();
        Some(SparseLuFactors {
            perm_r: perm_r.clone(),
            xsup: take(&g.xsup, nsuper + 2),
            supno: g.supno.clone(),
            xlsub: g.xlsub.clone(),
            lsub: take(&g.lsub, g.xlsub[n]),
            xlusup: g.xlusup.clone(),
            lusup: g.lusup[..g.xlusup[n] as usize].to_vec(),
            xusub: g.xusub.clone(),
            usub: take(&g.usub, g.xusub[n]),
            ucol: g.ucol[..g.xusub[n] as usize].to_vec(),
            nnz_l,
            nnz_u,
        })
    }

    /// `x = solve(b)` with a column-vector destination (upstream
    /// `SparseSolverEigen::solve`): `X = P_r b`, supernodal L solve, U solve,
    /// `x = P_c⁻¹ X`. A non-finite `b` is refused and a non-finite result
    /// from finite input is an arithmetic range failure (both
    /// [`Error::Numerical`]); `x` is only written on success.
    pub fn solve(&self, b: &[Real], x: &mut [Real]) -> Result<()> {
        self.solve_with(b, x, &mut Vec::new())
    }

    /// [`solve`](Self::solve) with the caller's scratch buffer (`2 n`
    /// values; reused across solves, no allocation once grown).
    pub fn solve_with(&self, b: &[Real], x: &mut [Real], scratch: &mut Vec<f64>) -> Result<()> {
        let (perm_r, g) = self.factors.as_ref().ok_or_else(|| {
            Error::LinearSolver("solve() called before a successful factorize()".into())
        })?;
        let n = self.n;
        if b.len() != n || x.len() != n {
            return Err(Error::DimensionMismatch {
                context: "SparseLu::solve",
                expected: n,
                got: if b.len() == n { x.len() } else { b.len() },
            });
        }
        check_finite_rhs("Eigen SparseLU", b)?;
        scratch.clear();
        scratch.resize(2 * n, 0.0);
        let (xv, work) = scratch.split_at_mut(n);
        for (i, &bi) in b.iter().enumerate() {
            xv[perm_r[i] as usize] = bi;
        }
        if n > 0 {
            solve_l(n, g, xv, work);
            solve_u(n, g, xv);
        }
        if let Some(i) = xv.iter().position(|v| !v.is_finite()) {
            return Err(arithmetic_range_failure(
                "Eigen SparseLU",
                format_args!("a solution entry = {}", xv[i]),
            ));
        }
        for (i, xi) in x.iter_mut().enumerate() {
            *xi = xv[self.perm_c[i] as usize];
        }
        Ok(())
    }
}

/// `MappedSuperNodalMatrix::solveInPlace` for one right-hand side.
#[allow(clippy::explicit_counter_loop)] // keeps Eigen's `iptr` counter
/// `work`: `n` zeros (left zero).
fn solve_l(n: usize, g: &Glu, x: &mut [f64], work: &mut [f64]) {
    let nsuper = g.supno[n] as usize;
    for k in 0..=nsuper {
        let fsupc = g.xsup[k] as usize;
        let istart = g.xlsub[fsupc] as usize;
        let nsupr = g.xlsub[fsupc + 1] as usize - istart;
        let nsupc = g.xsup[k + 1] as usize - fsupc;
        let nrow = nsupr - nsupc;
        if nsupc == 1 {
            // InnerIterator over column fsupc, skipping the diagonal
            let (v0, v1) = (g.xlusup[fsupc] as usize, g.xlusup[fsupc + 1] as usize);
            let xf = x[fsupc];
            let rows = &g.lsub[istart + 1..istart + (v1 - v0)];
            for (&irow, &lv) in rows.iter().zip(&g.lusup[v0 + 1..v1]) {
                x[irow as usize] -= xf * lv;
            }
        } else {
            let luptr = g.xlusup[fsupc] as usize;
            let lda = g.xlusup[fsupc + 1] as usize - luptr;
            dense::trsv_unit_lower(nsupc, &g.lusup, luptr, lda, x, fsupc);
            work[..nrow].fill(0.0);
            dense::gemv_product(
                nrow,
                nsupc,
                &g.lusup,
                luptr + nsupc,
                lda,
                x,
                fsupc,
                work,
                0,
                1.0,
            );
            let mut iptr = istart + nsupc;
            for w in work.iter_mut().take(nrow) {
                let irow = g.lsub[iptr] as usize;
                x[irow] -= *w;
                *w = 0.0;
                iptr += 1;
            }
        }
    }
}

/// `SparseLUMatrixUReturnType::solveInPlace` for one right-hand side.
fn solve_u(n: usize, g: &Glu, x: &mut [f64]) {
    let nsuper = g.supno[n];
    let mut k = nsuper;
    while k >= 0 {
        let ku = k as usize;
        let fsupc = g.xsup[ku] as usize;
        let lda = (g.xlusup[fsupc + 1] - g.xlusup[fsupc]) as usize;
        let nsupc = g.xsup[ku + 1] as usize - fsupc;
        let luptr = g.xlusup[fsupc] as usize;
        if nsupc == 1 {
            x[fsupc] /= g.lusup[luptr];
        } else {
            dense::trsv_upper(nsupc, &g.lusup, luptr, lda, x, fsupc);
        }
        for jcol in fsupc..fsupc + nsupc {
            let (p0, p1) = (g.xusub[jcol] as usize, g.xusub[jcol + 1] as usize);
            // the U rows of a column lie above its supernode (`irow < fsupc`),
            // so `x[jcol]` does not change inside the loop
            let xj = x[jcol];
            for (&irow, &uv) in g.usub[p0..p1].iter().zip(&g.ucol[p0..p1]) {
                x[irow as usize] -= xj * uv;
            }
        }
        k -= 1;
    }
}

/// Upstream `SparseSolverEigen`: Eigen `SparseLU` on the matrix built by
/// `setA()` ([`CscMatrix::from_aij`]); analysis when the profile release or
/// the effective symmetry changed, factorisation when the value release
/// changed (the caller — `AijBackend` — tracks the releases, as upstream's
/// `last_profile_release` / `last_value_release`). Unlike upstream, which
/// never checks `info()` after `factorize` (UD-178), a failed
/// factorisation is an error and leaves no factorisation to solve with;
/// non-finite systems are refused (UD-183).
#[derive(Debug, Default)]
pub struct EigenSparseLuSolver {
    lu: SparseLu,
    analysed: Option<(u64, MatrixType)>,
    /// Scratch of the solves ([`SparseLu::solve_with`]).
    scratch: Vec<f64>,
}

impl EigenSparseLuSolver {
    /// The factorisation object (permutations and factor arrays).
    #[must_use]
    pub fn lu(&self) -> &SparseLu {
        &self.lu
    }
}

impl SparseDirectSolver for EigenSparseLuSolver {
    fn kind(&self) -> SparseSolverType {
        SparseSolverType::Eigen
    }

    fn name(&self) -> &'static str {
        "eigen"
    }

    fn capabilities(&self) -> BackendCapabilities {
        centralized_capabilities()
    }

    fn analysis(&mut self, a: &SparseMatrixAij) -> Result<()> {
        // Invalidate first: a failed conversion leaves nothing usable.
        self.analysed = None;
        self.lu.pattern = None;
        self.lu.invalidate();
        let c = CscMatrix::from_aij(a)?;
        self.lu.analyze_pattern(&c)?;
        self.analysed = Some((a.profile_release(), a.matrix_type()));
        Ok(())
    }

    fn factorize(&mut self, a: &SparseMatrixAij) -> Result<()> {
        // Invalidate first: no stale factors survive a
        // failed refactorisation, whatever step fails.
        self.lu.invalidate();
        let c = CscMatrix::from_aij(a)?;
        self.lu.factorize(&c)
    }

    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()> {
        self.lu.solve_with(b, x, &mut self.scratch)
    }

    fn destroy_internal_data(&mut self) {
        *self = Self::default();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn csc(n: usize, t: &[(usize, usize, f64)]) -> CscMatrix {
        CscMatrix::from_triplets(n, t).unwrap()
    }

    /// A deterministic sparse test matrix: a 2-D 5-point stencil of `k × k`
    /// nodes with unsymmetric off-diagonal values from `seed`.
    fn stencil(k: usize, seed: u64) -> CscMatrix {
        let mut r = seed | 1;
        let mut next = || {
            r ^= r << 13;
            r ^= r >> 7;
            r ^= r << 17;
            (r >> 11) as f64 / (1u64 << 53) as f64
        };
        let mut t = Vec::new();
        for i in 0..k {
            for j in 0..k {
                let p = i * k + j;
                t.push((p, p, 4.0 + next()));
                for (di, dj) in [(0i64, 1i64), (1, 0), (0, -1), (-1, 0)] {
                    let (a, b) = (i as i64 + di, j as i64 + dj);
                    if a >= 0 && b >= 0 && (a as usize) < k && (b as usize) < k {
                        t.push((p, a as usize * k + b as usize, -1.0 + 0.5 * next()));
                    }
                }
            }
        }
        CscMatrix::from_triplets(k * k, &t).unwrap()
    }

    fn factor_bits(lu: &SparseLu) -> Vec<u64> {
        let f = lu.factors().unwrap();
        let mut v: Vec<u64> = f.lusup.iter().chain(&f.ucol).map(|x| x.to_bits()).collect();
        for a in [
            &f.perm_r, &f.xsup, &f.supno, &f.xlsub, &f.lsub, &f.xlusup, &f.xusub, &f.usub,
        ] {
            v.extend(a.iter().map(|&i| i as u64));
        }
        v
    }

    #[test]
    fn reused_storage_gives_the_factors_of_a_fresh_factorisation() {
        let big = stencil(30, 7);
        let a1 = stencil(20, 1);
        let a2 = stencil(20, 2);
        let mut fresh = SparseLu::default();
        fresh.analyze_pattern(&a2).unwrap();
        fresh.factorize(&a2).unwrap();
        let want = factor_bits(&fresh);
        // larger system first, then the same pattern with other values
        let mut lu = SparseLu::default();
        lu.analyze_pattern(&big).unwrap();
        lu.factorize(&big).unwrap();
        lu.analyze_pattern(&a1).unwrap();
        lu.factorize(&a1).unwrap();
        lu.factorize(&a2).unwrap();
        assert_eq!(factor_bits(&lu), want);
        // after a failed factorisation (a zero column)
        let mut t: Vec<(usize, usize, f64)> = Vec::new();
        for (c, w) in a2.col_ptr().windows(2).enumerate() {
            for p in w[0]..w[1] {
                let v = if c == 7 { 0.0 } else { a2.values()[p] };
                t.push((a2.row_idx()[p], c, v));
            }
        }
        let bad = CscMatrix::from_triplets(400, &t).unwrap();
        assert!(lu.factorize(&bad).is_err());
        lu.factorize(&a2).unwrap();
        assert_eq!(factor_bits(&lu), want);
        let b: Vec<f64> = (0..400).map(|i| (i % 7) as f64 - 3.0).collect();
        let (mut x1, mut x2) = (vec![0.0; 400], vec![0.0; 400]);
        lu.solve(&b, &mut x1).unwrap();
        fresh.solve(&b, &mut x2).unwrap();
        assert_eq!(
            x1.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
            x2.iter().map(|x| x.to_bits()).collect::<Vec<_>>()
        );
    }

    #[test]
    fn solves_small_systems_and_reports_singularity() {
        let a = csc(
            3,
            &[
                (0, 0, 4.0),
                (0, 1, 1.0),
                (1, 0, 2.0),
                (1, 1, 5.0),
                (2, 2, 3.0),
                (2, 0, 1.0),
            ],
        );
        let mut lu = SparseLu::default();
        lu.analyze_pattern(&a).unwrap();
        lu.factorize(&a).unwrap();
        let xe = [1.0, -2.0, 0.5];
        let b = a.mul_vec(&xe).unwrap();
        let mut x = [0.0; 3];
        lu.solve(&b, &mut x).unwrap();
        for (xi, e) in x.iter().zip(xe) {
            assert!((xi - e).abs() < 1e-14);
        }
        let s = csc(2, &[(0, 0, 1.0), (0, 1, 2.0), (1, 0, 2.0), (1, 1, 4.0)]);
        let mut lu = SparseLu::default();
        lu.analyze_pattern(&s).unwrap();
        assert!(matches!(lu.factorize(&s), Err(Error::LinearSolver(_))));
        assert!(lu.solve(&[1.0, 1.0], &mut [0.0; 2]).is_err());
        // empty system
        let e = csc(0, &[]);
        let mut lu = SparseLu::default();
        lu.analyze_pattern(&e).unwrap();
        lu.factorize(&e).unwrap();
        lu.solve(&[], &mut []).unwrap();
    }
}
