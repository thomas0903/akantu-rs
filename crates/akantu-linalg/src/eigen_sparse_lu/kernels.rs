// SPDX-License-Identifier: MPL-2.0
//
// Rust translation of parts of Eigen 3.4.0 (https://eigen.tuxfamily.org),
// for akantu-rs (a port of Akantu, EPFL/LSMS). Source used: Debian
// libeigen3-dev 3.4.0-5, /usr/include/eigen3 in the oracle image
// akantu-oracle:dev. Modified: translated from C++ to Rust (akantu-rs,
// 2026); the factor arrays grow on demand instead of through
// `memInit`/`memXpand` (same contents and positions). The complete notice
// of every Eigen file this file derives from is reproduced verbatim below
// (MPL-2.0 §3.4; the SuperLU/Xerox notices are retained and this notice
// states that the code was modified, as they require).
//
// ---- Eigen/src/SparseLU/SparseLU_Structs.h
//      (LU_GlobalLU_t):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /*
//    * NOTE: This file comes from a partly modified version of files slu_[s,d,c,z]defs.h
//    * -- SuperLU routine (version 4.1) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * November, 2010
//    *
//    * Global data structures used in LU factorization -
//    *
//    *   nsuper: #supernodes = nsuper + 1, numbered [0, nsuper].
//    *   (xsup,supno): supno[i] is the supernode no to which i belongs;
//    *  xsup(s) points to the beginning of the s-th supernode.
//    *  e.g.   supno 0 1 2 2 3 3 3 4 4 4 4 4   (n=12)
//    *          xsup 0 1 2 4 7 12
//    *  Note: dfs will be performed on supernode rep. relative to the new
//    *        row pivoting ordering
//    *
//    *   (xlsub,lsub): lsub[*] contains the compressed subscript of
//    *  rectangular supernodes; xlsub[j] points to the starting
//    *  location of the j-th column in lsub[*]. Note that xlsub
//    *  is indexed by column.
//    *  Storage: original row subscripts
//    *
//    *      During the course of sparse LU factorization, we also use
//    *  (xlsub,lsub) for the purpose of symmetric pruning. For each
//    *  supernode {s,s+1,...,t=s+r} with first column s and last
//    *  column t, the subscript set
//    *    lsub[j], j=xlsub[s], .., xlsub[s+1]-1
//    *  is the structure of column s (i.e. structure of this supernode).
//    *  It is used for the storage of numerical values.
//    *  Furthermore,
//    *    lsub[j], j=xlsub[t], .., xlsub[t+1]-1
//    *  is the structure of the last column t of this supernode.
//    *  It is for the purpose of symmetric pruning. Therefore, the
//    *  structural subscripts can be rearranged without making physical
//    *  interchanges among the numerical values.
//    *
//    *  However, if the supernode has only one column, then we
//    *  only keep one set of subscripts. For any subscript interchange
//    *  performed, similar interchange must be done on the numerical
//    *  values.
//    *
//    *  The last column structures (for pruning) will be removed
//    *  after the numercial LU factorization phase.
//    *
//    *   (xlusup,lusup): lusup[*] contains the numerical values of the
//    *  rectangular supernodes; xlusup[j] points to the starting
//    *  location of the j-th column in storage vector lusup[*]
//    *  Note: xlusup is indexed by column.
//    *  Each rectangular supernode is stored by column-major
//    *  scheme, consistent with Fortran 2-dim array storage.
//    *
//    *   (xusub,ucol,usub): ucol[*] stores the numerical values of
//    *  U-columns outside the rectangular supernodes. The row
//    *  subscript of nonzero ucol[k] is stored in usub[k].
//    *  xusub[i] points to the starting location of column i in ucol.
//    *  Storage: new row subscripts; that is subscripts of PA.
//    */
//
// ---- Eigen/src/SparseLU/SparseLU_Memory.h
//      (LUnumTempV, emptyIdxLU, LUNoMarker):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /*
//
//    * NOTE: This file is the modified version of [s,d,c,z]memory.c files in SuperLU
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
//
// ---- Eigen/src/SparseLU/SparseLU_relax_snode.h
//      (relax_snode):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /* This file is a modified version of heap_relax_snode.c file in SuperLU
//    * -- SuperLU routine (version 3.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * October 15, 2003
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
//
// ---- Eigen/src/SparseLU/SparseLU_column_dfs.h
//      (column_dfs):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /*
//
//    * NOTE: This file is the modified version of [s,d,c,z]column_dfs.c file in SuperLU
//
//    * -- SuperLU routine (version 2.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * November 15, 1997
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
//
// ---- Eigen/src/SparseLU/SparseLU_panel_dfs.h
//      (dfs_kernel, panel_dfs):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /*
//
//    * NOTE: This file is the modified version of [s,d,c,z]panel_dfs.c file in SuperLU
//
//    * -- SuperLU routine (version 2.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * November 15, 1997
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
//
// ---- Eigen/src/SparseLU/SparseLU_panel_bmod.h
//      (panel_bmod):
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
//   /*
//
//    * NOTE: This file is the modified version of [s,d,c,z]panel_bmod.c file in SuperLU
//
//    * -- SuperLU routine (version 3.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * October 15, 2003
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
//
// ---- Eigen/src/SparseLU/SparseLU_column_bmod.h
//      (column_bmod):
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
//   /*
//
//    * NOTE: This file is the modified version of xcolumn_bmod.c file in SuperLU
//
//    * -- SuperLU routine (version 3.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * October 15, 2003
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
//
// ---- Eigen/src/SparseLU/SparseLU_kernel_bmod.h
//      (LU_kernel_bmod):
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
// ---- Eigen/src/SparseLU/SparseLU_copy_to_ucol.h
//      (copy_to_ucol):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//   /*
//
//    * NOTE: This file is the modified version of [s,d,c,z]copy_to_ucol.c file in SuperLU
//
//    * -- SuperLU routine (version 2.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * November 15, 1997
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
//
// ---- Eigen/src/SparseLU/SparseLU_pivotL.h
//      (pivotL):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /*
//
//    * NOTE: This file is the modified version of xpivotL.c file in SuperLU
//
//    * -- SuperLU routine (version 3.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * October 15, 2003
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
//
// ---- Eigen/src/SparseLU/SparseLU_pruneL.h
//      (pruneL):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.
//
//   /*
//
//    * NOTE: This file is the modified version of [s,d,c,z]pruneL.c file in SuperLU
//
//    * -- SuperLU routine (version 2.0) --
//    * Univ. of California Berkeley, Xerox Palo Alto Research Center,
//    * and Lawrence Berkeley National Lab.
//    * November 15, 1997
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
//
// ---- Eigen/src/SparseLU/SparseLU_Utils.h
//      (countnz, fixupL):
//   This file is part of Eigen, a lightweight C++ template library
//   for linear algebra.
//
//   Copyright (C) 2012 Désiré Nuentsa-Wakam <desire.nuentsa_wakam@inria.fr>
//
//   This Source Code Form is subject to the terms of the Mozilla
//   Public License v. 2.0. If a copy of the MPL was not distributed
//   with this file, You can obtain one at http://mozilla.org/MPL/2.0/.

//! The supernodal left-looking kernels of Eigen's `SparseLUImpl`
//! (SuperLU 4.1 lineage), translated statement by statement. Index values
//! are `i64` (Eigen: `int` storage indices, `Index` arithmetic; `-1` is
//! `emptyIdxLU`).

// Statement-by-statement translation of index-based C++: index loops and
// explicit counters are kept so that a reader can follow Eigen's source line
// by line.
#![allow(clippy::needless_range_loop, clippy::explicit_counter_loop)]

use super::dense;

/// `emptyIdxLU`.
pub(crate) const EMPTY: i64 = -1;

/// `LU_GlobalLU_t`: the factor storage.
#[derive(Clone, Debug, Default)]
pub(crate) struct Glu {
    pub xsup: Vec<i64>,
    pub supno: Vec<i64>,
    pub lusup: Vec<f64>,
    pub lsub: Vec<i64>,
    pub xlusup: Vec<i64>,
    pub xlsub: Vec<i64>,
    pub ucol: Vec<f64>,
    pub usub: Vec<i64>,
    pub xusub: Vec<i64>,
}

fn grow<T: Clone + Default>(v: &mut Vec<T>, len: usize) {
    if v.len() < len {
        let new_len = len.max(v.len() + v.len() / 2);
        v.resize(new_len, T::default());
    }
}

fn u(i: i64) -> usize {
    debug_assert!(i >= 0, "negative index {i}");
    i as usize
}

/// The column-permuted input matrix `m_mat` of `SparseLU::factorize`:
/// column `j` is the original column `iperm_c[j]` (rows increasing).
pub(crate) struct PermutedCols<'a> {
    pub col_ptr: &'a [usize],
    pub row_idx: &'a [usize],
    pub values: &'a [f64],
    pub iperm_c: &'a [i64],
}

impl PermutedCols<'_> {
    fn col(&self, j: usize) -> std::ops::Range<usize> {
        let c = self.iperm_c[j] as usize;
        self.col_ptr[c]..self.col_ptr[c + 1]
    }
}

/// `relax_snode(n, et, relax_columns, descendants, relax_end)`.
pub(crate) fn relax_snode(n: usize, et: &[i64], relax_columns: i64, relax_end: &mut [i64]) {
    let ni = n as i64;
    let mut descendants = vec![0i64; n + 1];
    relax_end.fill(EMPTY);
    for j in 0..n {
        let parent = et[j];
        if parent != ni {
            descendants[u(parent)] += descendants[j] + 1;
        }
    }
    let mut j = 0usize;
    while j < n {
        let mut parent = et[j];
        let snode_start = j;
        while parent != ni && descendants[u(parent)] < relax_columns {
            j = u(parent);
            parent = et[j];
        }
        relax_end[snode_start] = j as i64;
        j += 1;
        // `while (descendants(j) != 0 && j < n)`: descendants(n) is 0 in
        // Eigen's (3m-long, zeroed) work array.
        while j < n && descendants[j] != 0 {
            j += 1;
        }
    }
}

/// Where `dfs_kernel` writes the L subscripts it finds, and the traits of
/// its caller.
pub(crate) enum DfsTarget<'a> {
    /// `panel_dfs`: into `panel_lsub`; segment representatives are
    /// recorded once per panel through `marker1 = marker[m..2m]`.
    Panel {
        panel_lsub: &'a mut [i64],
        jcol: i64,
        m: usize,
    },
    /// `column_dfs`: into `glu.lsub`; a new L row not in column `jcol-1`
    /// ends the supernode (`jsuper = emptyIdxLU`).
    Column { jcol: i64, jsuper: &'a mut i64 },
}

/// Work arrays shared by the symbolic routines.
pub(crate) struct Work {
    pub segrep: Vec<i64>,
    pub parent: Vec<i64>,
    pub xplore: Vec<i64>,
    pub repfnz: Vec<i64>,
    pub panel_lsub: Vec<i64>,
    pub xprune: Vec<i64>,
    pub marker: Vec<i64>,
    pub dense: Vec<f64>,
    pub tempv: Vec<f64>,
}

/// `dfs_kernel`: `repfnz_off` / `marker_off` locate `repfnz_col` and the
/// visited marker inside their arrays.
#[allow(clippy::too_many_arguments)] // translation of SparseLUImpl::dfs_kernel
fn dfs_kernel(
    jj: i64,
    perm_r: &[i64],
    nseg: &mut i64,
    target: &mut DfsTarget<'_>,
    segrep: &mut [i64],
    repfnz: &mut [i64],
    repfnz_off: usize,
    xprune: &[i64],
    marker: &mut [i64],
    marker_off: usize,
    parent: &mut [i64],
    xplore: &mut [i64],
    glu: &mut Glu,
    nextl_col: &mut i64,
    krow: i64,
) {
    // write `row` at nextl_col++ then the traits' mem_expand(chmark)
    fn place(
        target: &mut DfsTarget<'_>,
        glu: &mut Glu,
        nextl_col: &mut i64,
        row: i64,
        chmark: i64,
    ) {
        match target {
            DfsTarget::Panel { panel_lsub, .. } => {
                panel_lsub[u(*nextl_col)] = row;
                *nextl_col += 1;
            }
            DfsTarget::Column { jcol, jsuper } => {
                grow(&mut glu.lsub, u(*nextl_col) + 1);
                glu.lsub[u(*nextl_col)] = row;
                *nextl_col += 1;
                if chmark != *jcol - 1 {
                    **jsuper = EMPTY;
                }
            }
        }
    }
    let kmark = marker[marker_off + u(krow)];
    marker[marker_off + u(krow)] = jj;
    let kperm = perm_r[u(krow)];
    if kperm == EMPTY {
        place(target, glu, nextl_col, krow, kmark);
        return;
    }
    let mut krep = glu.xsup[u(glu.supno[u(kperm)] + 1)] - 1;
    let myfnz = repfnz[repfnz_off + u(krep)];
    if myfnz != EMPTY {
        if myfnz > kperm {
            repfnz[repfnz_off + u(krep)] = kperm;
        }
        return;
    }
    let mut oldrep = EMPTY;
    parent[u(krep)] = oldrep;
    repfnz[repfnz_off + u(krep)] = kperm;
    let mut xdfs = glu.xlsub[u(krep)];
    let mut maxdfs = xprune[u(krep)];
    loop {
        while xdfs < maxdfs {
            let kchild = glu.lsub[u(xdfs)];
            xdfs += 1;
            let chmark = marker[marker_off + u(kchild)];
            if chmark != jj {
                marker[marker_off + u(kchild)] = jj;
                let chperm = perm_r[u(kchild)];
                if chperm == EMPTY {
                    place(target, glu, nextl_col, kchild, chmark);
                } else {
                    let chrep = glu.xsup[u(glu.supno[u(chperm)] + 1)] - 1;
                    let myfnz = repfnz[repfnz_off + u(chrep)];
                    if myfnz != EMPTY {
                        if myfnz > chperm {
                            repfnz[repfnz_off + u(chrep)] = chperm;
                        }
                    } else {
                        xplore[u(krep)] = xdfs;
                        oldrep = krep;
                        krep = chrep;
                        parent[u(krep)] = oldrep;
                        repfnz[repfnz_off + u(krep)] = chperm;
                        xdfs = glu.xlsub[u(krep)];
                        maxdfs = xprune[u(krep)];
                    }
                }
            }
        }
        let record = match target {
            DfsTarget::Panel { jcol, m, .. } => {
                let mk = &mut marker[*m + u(krep)];
                if *mk < *jcol {
                    *mk = jj;
                    true
                } else {
                    false
                }
            }
            DfsTarget::Column { .. } => true,
        };
        if record {
            segrep[u(*nseg)] = krep;
            *nseg += 1;
        }
        let kpar = parent[u(krep)];
        if kpar == EMPTY {
            break;
        }
        krep = kpar;
        xdfs = xplore[u(krep)];
        maxdfs = xprune[u(krep)];
    }
}

/// `panel_dfs(m, w, jcol, A, perm_r, nseg, dense, panel_lsub, segrep,
/// repfnz, xprune, marker, parent, xplore, glu)`; returns `nseg`.
pub(crate) fn panel_dfs(
    m: usize,
    w: usize,
    jcol: usize,
    a: &PermutedCols<'_>,
    perm_r: &[i64],
    wk: &mut Work,
    glu: &mut Glu,
) -> i64 {
    let mut nseg: i64 = 0;
    let Work {
        segrep,
        parent,
        xplore,
        repfnz,
        panel_lsub,
        xprune,
        marker,
        dense,
        ..
    } = wk;
    let mut target = DfsTarget::Panel {
        panel_lsub: panel_lsub.as_mut_slice(),
        jcol: jcol as i64,
        m,
    };
    for jj in jcol..jcol + w {
        let mut nextl_col = ((jj - jcol) * m) as i64;
        let col_off = (jj - jcol) * m;
        for p in a.col(jj) {
            let krow = a.row_idx[p];
            dense[col_off + krow] = a.values[p];
            let kmark = marker[krow];
            if kmark == jj as i64 {
                continue;
            }
            dfs_kernel(
                jj as i64,
                perm_r,
                &mut nseg,
                &mut target,
                segrep,
                repfnz,
                col_off,
                xprune,
                marker,
                0,
                parent,
                xplore,
                glu,
                &mut nextl_col,
                krow as i64,
            );
        }
    }
    nseg
}

/// `column_dfs(m, jcol, perm_r, maxsuper, nseg, lsub_col, segrep, repfnz,
/// xprune, marker, parent, xplore, glu)` with `lsub_col = panel_lsub[k..]`
/// and `repfnz = repfnz[k..]`, `k = col_off`.
#[allow(clippy::too_many_arguments)] // translation of SparseLUImpl::column_dfs
pub(crate) fn column_dfs(
    m: usize,
    jcol: usize,
    perm_r: &[i64],
    maxsuper: i64,
    nseg: &mut i64,
    col_off: usize,
    wk: &mut Work,
    glu: &mut Glu,
) {
    let ji = jcol as i64;
    let mut jsuper = glu.supno[jcol];
    let mut nextl = glu.xlsub[jcol];
    let marker2 = 2 * m;
    {
        let Work {
            segrep,
            parent,
            xplore,
            repfnz,
            panel_lsub,
            xprune,
            marker,
            ..
        } = &mut *wk;
        let mut k = 0usize;
        while k < m && panel_lsub[col_off + k] != EMPTY {
            let krow = panel_lsub[col_off + k];
            panel_lsub[col_off + k] = EMPTY;
            k += 1;
            let kmark = marker[marker2 + u(krow)];
            if kmark == ji {
                continue;
            }
            let mut target = DfsTarget::Column {
                jcol: ji,
                jsuper: &mut jsuper,
            };
            dfs_kernel(
                ji,
                perm_r,
                nseg,
                &mut target,
                segrep,
                repfnz,
                col_off,
                xprune,
                marker,
                marker2,
                parent,
                xplore,
                glu,
                &mut nextl,
                krow,
            );
        }
    }
    let mut nsuper = glu.supno[jcol];
    let jcolp1 = jcol + 1;
    if jcol == 0 {
        nsuper = 0;
        glu.supno[0] = 0;
    } else {
        let jcolm1 = jcol - 1;
        let fsupc = glu.xsup[u(nsuper)];
        let jptr = glu.xlsub[jcol];
        let jm1ptr = glu.xlsub[jcolm1];
        if nextl - jptr != jptr - jm1ptr - 1 {
            jsuper = EMPTY;
        }
        if ji - fsupc >= maxsuper {
            jsuper = EMPTY;
        }
        if jsuper == EMPTY {
            if fsupc < jcolm1 as i64 - 1 {
                let mut ito = glu.xlsub[u(fsupc + 1)];
                glu.xlsub[jcolm1] = ito;
                let istop = ito + jptr - jm1ptr;
                wk.xprune[jcolm1] = istop;
                glu.xlsub[jcol] = istop;
                let mut ifrom = jm1ptr;
                while ifrom < nextl {
                    glu.lsub[u(ito)] = glu.lsub[u(ifrom)];
                    ifrom += 1;
                    ito += 1;
                }
                nextl = ito;
            }
            nsuper += 1;
            glu.supno[jcol] = nsuper;
        }
    }
    glu.xsup[u(nsuper + 1)] = jcolp1 as i64;
    glu.supno[jcolp1] = nsuper;
    wk.xprune[jcol] = nextl;
    glu.xlsub[jcolp1] = nextl;
}

/// `LU_kernel_bmod<SegSizeAtCompileTime>::run`: `fixed` is the compile-time
/// segment size (0 = `Dynamic`); `dense` is the column's block.
#[allow(clippy::too_many_arguments)] // translation of LU_kernel_bmod::run
fn kernel_bmod(
    fixed: usize,
    segsize: usize,
    dense: &mut [f64],
    tempv: &mut [f64],
    lusup: &[f64],
    mut luptr: usize,
    lda: usize,
    nrow: usize,
    lsub: &[i64],
    lptr: usize,
    no_zeros: usize,
) {
    if fixed == 1 {
        let f = dense[u(lsub[lptr + no_zeros])];
        luptr += lda * no_zeros + no_zeros + 1;
        let irow = &lsub[lptr + no_zeros + 1..];
        let mut i = 0;
        while i + 1 < nrow {
            let (i0, i1) = (u(irow[i]), u(irow[i + 1]));
            let (a0, a1) = (lusup[luptr + i], lusup[luptr + i + 1]);
            let mut d0 = dense[i0];
            let mut d1 = dense[i1];
            d0 -= f * a0;
            d1 -= f * a1;
            dense[i0] = d0;
            dense[i1] = d1;
            i += 2;
        }
        if i < nrow {
            dense[u(irow[i])] -= f * lusup[luptr + i];
        }
        return;
    }
    let mut isub = lptr + no_zeros;
    for t in tempv.iter_mut().take(segsize) {
        *t = dense[u(lsub[isub])];
        isub += 1;
    }
    luptr += lda * no_zeros + no_zeros;
    if fixed == 2 || fixed == 3 {
        dense::unit_lower_unrolled(segsize, lusup, luptr, lda, tempv, 0);
    } else {
        dense::trsv_unit_lower(segsize, lusup, luptr, lda, tempv, 0);
    }
    luptr += segsize;
    let (useg, l) = tempv.split_at_mut(segsize);
    l[..nrow].fill(0.0);
    dense::sparselu_gemm(
        nrow, 1, segsize, lusup, luptr, lda, useg, 0, segsize, l, 0, nrow,
    );
    let mut isub = lptr + no_zeros;
    for &t in useg.iter() {
        dense[u(lsub[isub])] = t;
        isub += 1;
    }
    for &li in l.iter().take(nrow) {
        dense[u(lsub[isub])] -= li;
        isub += 1;
    }
}

/// `panel_bmod(m, w, jcol, nseg, dense, tempv, segrep, repfnz, glu)`.
pub(crate) fn panel_bmod(m: usize, w: usize, jcol: usize, nseg: i64, wk: &mut Work, glu: &mut Glu) {
    let mut k = nseg - 1;
    for _ in 0..nseg {
        let krep = wk.segrep[u(k)];
        k -= 1;
        let fsupc = glu.xsup[u(glu.supno[u(krep)])];
        let nsupc = krep - fsupc + 1;
        let nsupr = glu.xlsub[u(fsupc + 1)] - glu.xlsub[u(fsupc)];
        let nrow = nsupr - nsupc;
        let lptr = glu.xlsub[u(fsupc)];
        // actual number of columns and rows
        let mut u_rows: i64 = 0;
        let mut u_cols: usize = 0;
        for jj in jcol..jcol + w {
            let kfnz = wk.repfnz[(jj - jcol) * m + u(krep)];
            if kfnz == EMPTY {
                continue;
            }
            let segsize = krep - kfnz + 1;
            u_cols += 1;
            u_rows = u_rows.max(segsize);
        }
        if nsupc >= 2 {
            let ldu = u(u_rows);
            let ur = u(u_rows);
            // gather U
            let mut u_col = 0;
            for jj in jcol..jcol + w {
                let off = (jj - jcol) * m;
                let kfnz = wk.repfnz[off + u(krep)];
                if kfnz == EMPTY {
                    continue;
                }
                let segsize = u(krep - kfnz + 1);
                let no_zeros = u(kfnz - fsupc);
                let mut isub = u(lptr) + no_zeros;
                let offz = ur - segsize;
                for i in 0..offz {
                    wk.tempv[i + u_col * ldu] = 0.0;
                }
                for i in 0..segsize {
                    let irow = u(glu.lsub[isub]);
                    wk.tempv[i + offz + u_col * ldu] = wk.dense[off + irow];
                    isub += 1;
                }
                u_col += 1;
            }
            // solve U = A^-1 U
            let mut luptr = u(glu.xlusup[u(fsupc)]);
            let lda = u(glu.xlusup[u(fsupc + 1)] - glu.xlusup[u(fsupc)]);
            let no_zeros = u((krep - u_rows + 1) - fsupc);
            luptr += lda * no_zeros + no_zeros;
            dense::trsm_unit_lower(ur, u_cols, &glu.lusup, luptr, lda, &mut wk.tempv, 0, ldu);
            // update: L = B * U
            luptr += ur;
            let nr = u(nrow);
            let ldl = nr;
            let (tu, tl) = wk.tempv.split_at_mut(w * ldu);
            tl[..nr * u_cols].fill(0.0);
            dense::sparselu_gemm(
                nr, u_cols, ur, &glu.lusup, luptr, lda, tu, 0, ldu, tl, 0, ldl,
            );
            // scatter U and L
            let mut u_col = 0;
            for jj in jcol..jcol + w {
                let off = (jj - jcol) * m;
                let kfnz = wk.repfnz[off + u(krep)];
                if kfnz == EMPTY {
                    continue;
                }
                let segsize = u(krep - kfnz + 1);
                let no_zeros = u(kfnz - fsupc);
                let mut isub = u(lptr) + no_zeros;
                let offz = ur - segsize;
                for i in 0..segsize {
                    let irow = u(glu.lsub[isub]);
                    isub += 1;
                    wk.dense[off + irow] = tu[i + offz + u_col * ldu];
                    tu[i + offz + u_col * ldu] = 0.0;
                }
                for i in 0..nr {
                    let irow = u(glu.lsub[isub]);
                    isub += 1;
                    wk.dense[off + irow] -= tl[i + u_col * ldl];
                    tl[i + u_col * ldl] = 0.0;
                }
                u_col += 1;
            }
        } else {
            // level 2 only: each column of the panel
            for jj in jcol..jcol + w {
                let off = (jj - jcol) * m;
                let kfnz = wk.repfnz[off + u(krep)];
                if kfnz == EMPTY {
                    continue;
                }
                let segsize = u(krep - kfnz + 1);
                let luptr = u(glu.xlusup[u(fsupc)]);
                let lda = u(glu.xlusup[u(fsupc + 1)] - glu.xlusup[u(fsupc)]);
                let no_zeros = u(kfnz - fsupc);
                let fixed = if segsize <= 3 { segsize } else { 0 };
                let Work { dense, tempv, .. } = &mut *wk;
                kernel_bmod(
                    fixed,
                    segsize,
                    &mut dense[off..off + m],
                    tempv,
                    &glu.lusup,
                    luptr,
                    lda,
                    u(nrow),
                    &glu.lsub,
                    u(lptr),
                    no_zeros,
                );
            }
        }
    }
}

/// `column_bmod(jcol, nseg, dense, tempv, segrep, repfnz, fpanelc, glu)`
/// with `segrep = segrep[seg_off..]`, `dense`/`repfnz` the column blocks at
/// `col_off`.
#[allow(clippy::too_many_arguments)] // translation of SparseLUImpl::column_bmod
pub(crate) fn column_bmod(
    jcol: usize,
    nseg: i64,
    seg_off: usize,
    col_off: usize,
    m: usize,
    fpanelc: usize,
    wk: &mut Work,
    glu: &mut Glu,
) {
    let jsupno = glu.supno[jcol];
    let fp = fpanelc as i64;
    let mut k = nseg - 1;
    for _ in 0..nseg {
        let krep = wk.segrep[seg_off + u(k)];
        k -= 1;
        let ksupno = glu.supno[u(krep)];
        if jsupno != ksupno {
            let fsupc = glu.xsup[u(ksupno)];
            let fst_col = fsupc.max(fp);
            let d_fsupc = fst_col - fsupc;
            let luptr = glu.xlusup[u(fst_col)] + d_fsupc;
            let lptr = glu.xlsub[u(fsupc)] + d_fsupc;
            let kfnz = wk.repfnz[col_off + u(krep)].max(fp);
            let segsize = krep - kfnz + 1;
            let nsupc = krep - fst_col + 1;
            let nsupr = glu.xlsub[u(fsupc + 1)] - glu.xlsub[u(fsupc)];
            let nrow = nsupr - d_fsupc - nsupc;
            let lda = glu.xlusup[u(fst_col + 1)] - glu.xlusup[u(fst_col)];
            let no_zeros = kfnz - fst_col;
            let fixed = if segsize == 1 { 1 } else { 0 };
            let Work { dense, tempv, .. } = &mut *wk;
            kernel_bmod(
                fixed,
                u(segsize),
                &mut dense[col_off..col_off + m],
                tempv,
                &glu.lusup,
                u(luptr),
                u(lda),
                u(nrow),
                &glu.lsub,
                u(lptr),
                u(no_zeros),
            );
        }
    }
    // copy the SPA dense into L\U[*, jcol]
    let mut nextlu = glu.xlusup[jcol];
    let fsupc = glu.xsup[u(jsupno)];
    let new_next = nextlu + glu.xlsub[u(fsupc + 1)] - glu.xlsub[u(fsupc)];
    grow(&mut glu.lusup, u(new_next));
    for isub in glu.xlsub[u(fsupc)]..glu.xlsub[u(fsupc + 1)] {
        let irow = u(glu.lsub[u(isub)]);
        glu.lusup[u(nextlu)] = wk.dense[col_off + irow];
        wk.dense[col_off + irow] = 0.0;
        nextlu += 1;
    }
    glu.xlusup[jcol + 1] = nextlu;
    // updates within the supernode (columns fst_col..jcol-1)
    let fst_col = fsupc.max(fp);
    if fst_col < jcol as i64 {
        let d_fsupc = fst_col - fsupc;
        let luptr = u(glu.xlusup[u(fst_col)] + d_fsupc);
        let nsupr = glu.xlsub[u(fsupc + 1)] - glu.xlsub[u(fsupc)];
        let nsupc = jcol as i64 - fst_col;
        let nrow = u(nsupr - d_fsupc - nsupc);
        let base = u(glu.xlusup[jcol]);
        let ufirst = u(d_fsupc); // relative to `base`
        let lda = u(glu.xlusup[jcol + 1] - glu.xlusup[jcol]);
        let nsupc = u(nsupc);
        let (left, right) = glu.lusup.split_at_mut(base);
        dense::trsv_unit_lower(nsupc, left, luptr, lda, right, ufirst);
        dense::gemv_product_in_place(
            nrow,
            nsupc,
            left,
            luptr + nsupc,
            lda,
            right,
            ufirst,
            ufirst + nsupc,
            -1.0,
        );
    }
}

/// `copy_to_ucol(jcol, nseg, segrep, repfnz, perm_r, dense, glu)` with the
/// column blocks at `col_off`.
pub(crate) fn copy_to_ucol(
    jcol: usize,
    nseg: i64,
    col_off: usize,
    perm_r: &[i64],
    wk: &mut Work,
    glu: &mut Glu,
) {
    let jsupno = glu.supno[jcol];
    let mut k = nseg - 1;
    let mut nextu = glu.xusub[jcol];
    for _ in 0..nseg {
        let krep = wk.segrep[u(k)];
        k -= 1;
        let ksupno = glu.supno[u(krep)];
        if jsupno != ksupno {
            let kfnz = wk.repfnz[col_off + u(krep)];
            if kfnz != EMPTY {
                let fsupc = glu.xsup[u(ksupno)];
                let mut isub = glu.xlsub[u(fsupc)] + kfnz - fsupc;
                let segsize = krep - kfnz + 1;
                let new_next = nextu + segsize;
                grow(&mut glu.ucol, u(new_next));
                grow(&mut glu.usub, u(new_next));
                for _ in 0..segsize {
                    let irow = u(glu.lsub[u(isub)]);
                    glu.usub[u(nextu)] = perm_r[irow];
                    glu.ucol[u(nextu)] = wk.dense[col_off + irow];
                    wk.dense[col_off + irow] = 0.0;
                    nextu += 1;
                    isub += 1;
                }
            }
        }
    }
    glu.xusub[jcol + 1] = nextu;
}

/// `pivotL(jcol, diagpivotthresh, perm_r, iperm_c, pivrow, glu)`: `Ok(pivrow)`
/// or `Err(jcol + 1)` (zero pivot column).
pub(crate) fn pivot_l(
    jcol: usize,
    diagpivotthresh: f64,
    perm_r: &mut [i64],
    iperm_c: &[i64],
    glu: &mut Glu,
) -> Result<i64, i64> {
    let fsupc = glu.xsup[u(glu.supno[jcol])];
    let nsupc = u(jcol as i64 - fsupc);
    let lptr = u(glu.xlsub[u(fsupc)]);
    let nsupr = u(glu.xlsub[u(fsupc + 1)]) - lptr;
    let lda = u(glu.xlusup[u(fsupc + 1)] - glu.xlusup[u(fsupc)]);
    let lu_sup = u(glu.xlusup[u(fsupc)]);
    let lu_col = u(glu.xlusup[jcol]);
    let diagind = iperm_c[jcol];
    let mut pivmax = -1.0f64;
    let mut pivptr = nsupc;
    let mut diag: i64 = EMPTY;
    for isub in nsupc..nsupr {
        let rtemp = glu.lusup[lu_col + isub].abs();
        if rtemp > pivmax {
            pivmax = rtemp;
            pivptr = isub;
        }
        if glu.lsub[lptr + isub] == diagind {
            diag = isub as i64;
        }
    }
    if pivmax <= 0.0 {
        let pivrow = if pivmax < 0.0 {
            diagind
        } else {
            glu.lsub[lptr + pivptr]
        };
        perm_r[u(pivrow)] = jcol as i64;
        return Err(jcol as i64 + 1);
    }
    let thresh = diagpivotthresh * pivmax;
    if diag >= 0 {
        let rtemp = glu.lusup[lu_col + u(diag)].abs();
        if rtemp != 0.0 && rtemp >= thresh {
            pivptr = u(diag);
        }
    }
    let pivrow = glu.lsub[lptr + pivptr];
    perm_r[u(pivrow)] = jcol as i64;
    if pivptr != nsupc {
        glu.lsub.swap(lptr + pivptr, lptr + nsupc);
        for icol in 0..=nsupc {
            let itemp = pivptr + icol * lda;
            glu.lusup.swap(lu_sup + itemp, lu_sup + nsupc + icol * lda);
        }
    }
    let temp = 1.0 / glu.lusup[lu_col + nsupc];
    for k in nsupc + 1..nsupr {
        glu.lusup[lu_col + k] *= temp;
    }
    Ok(pivrow)
}

/// `pruneL(jcol, perm_r, pivrow, nseg, segrep, repfnz, xprune, glu)` with
/// `repfnz` the column block at `col_off`.
pub(crate) fn prune_l(
    jcol: usize,
    perm_r: &[i64],
    pivrow: i64,
    nseg: i64,
    col_off: usize,
    wk: &mut Work,
    glu: &mut Glu,
) {
    let jsupno = glu.supno[jcol];
    for i in 0..u(nseg) {
        let irep = wk.segrep[i];
        let irep1 = irep + 1;
        let mut do_prune = false;
        if wk.repfnz[col_off + u(irep)] == EMPTY {
            continue;
        }
        if glu.supno[u(irep)] == glu.supno[u(irep1)] {
            continue;
        }
        if glu.supno[u(irep)] != jsupno {
            let mut kmin = 0;
            let mut kmax = 0;
            if wk.xprune[u(irep)] >= glu.xlsub[u(irep1)] {
                kmin = glu.xlsub[u(irep)];
                kmax = glu.xlsub[u(irep1)] - 1;
                for krow in kmin..=kmax {
                    if glu.lsub[u(krow)] == pivrow {
                        do_prune = true;
                        break;
                    }
                }
            }
            if do_prune {
                let movnum = irep == glu.xsup[u(glu.supno[u(irep)])];
                while kmin <= kmax {
                    if perm_r[u(glu.lsub[u(kmax)])] == EMPTY {
                        kmax -= 1;
                    } else if perm_r[u(glu.lsub[u(kmin)])] != EMPTY {
                        kmin += 1;
                    } else {
                        glu.lsub.swap(u(kmin), u(kmax));
                        if movnum {
                            let minloc = glu.xlusup[u(irep)] + (kmin - glu.xlsub[u(irep)]);
                            let maxloc = glu.xlusup[u(irep)] + (kmax - glu.xlsub[u(irep)]);
                            glu.lusup.swap(u(minloc), u(maxloc));
                        }
                        kmin += 1;
                        kmax -= 1;
                    }
                }
                wk.xprune[u(irep)] = kmin;
            }
        }
    }
}

/// `countnz(n, nnzL, nnzU, glu)`.
pub(crate) fn countnz(n: usize, glu: &Glu) -> (i64, i64) {
    let mut nnz_l = 0;
    let mut nnz_u = glu.xusub[n];
    if n == 0 {
        return (nnz_l, nnz_u);
    }
    let nsuper = glu.supno[n];
    for i in 0..=u(nsuper) {
        let fsupc = glu.xsup[i];
        let mut jlen = glu.xlsub[u(fsupc + 1)] - glu.xlsub[u(fsupc)];
        for j in fsupc..glu.xsup[i + 1] {
            nnz_l += jlen;
            nnz_u += j - fsupc + 1;
            jlen -= 1;
        }
    }
    (nnz_l, nnz_u)
}

/// `fixupL(n, perm_r, glu)`.
pub(crate) fn fixup_l(n: usize, perm_r: &[i64], glu: &mut Glu) {
    if n == 0 {
        return;
    }
    let mut nextl: i64 = 0;
    let nsuper = glu.supno[n];
    for i in 0..=u(nsuper) {
        let fsupc = glu.xsup[i];
        let jstart = glu.xlsub[u(fsupc)];
        glu.xlsub[u(fsupc)] = nextl;
        for j in jstart..glu.xlsub[u(fsupc + 1)] {
            glu.lsub[u(nextl)] = perm_r[u(glu.lsub[u(j)])];
            nextl += 1;
        }
        for k in fsupc + 1..glu.xsup[i + 1] {
            glu.xlsub[u(k)] = nextl;
        }
    }
    glu.xlsub[n] = nextl;
}
