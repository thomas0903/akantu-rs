// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_matrix_aij_inline_impl.hh
// (addValues), src/solver/sparse_matrix_aij.cc (matVecMul, add, copyProfile,
// applyBoundary); the split over threads has no upstream counterpart (D-123).

//! Threaded forms of the [`SparseMatrixAij`] operations, bitwise at every
//! thread count (D-122, D-123).
//!
//! * Element assembly ([`SparseMatrixAij::add_values_cached_batch`]): every
//!   thread owns a contiguous range of entry positions and visits the
//!   elements in their order, adding only the contributions whose position
//!   lies in its range. Every entry then receives its contributions in
//!   element order, as in the serial loop, whatever the thread count and
//!   the ranges.
//! * Entry-wise operations (sums of matrices with the same entries, scaling,
//!   zeroing, Dirichlet rows): every entry is an independent output.
//! * Matrix-vector product ([`SparseMatrixAij::mat_vec_mul_with`]): rows are
//!   independent outputs once each row's terms are listed in storage order
//!   ([`RowGather`]); every row adds them in that order, as the serial pass
//!   over the entries does.

use std::sync::{Arc, Mutex};

use akantu_core::exec::{Exec, Tuples, TuplesRef};

use super::*;

/// Smallest number of elemental blocks per part of a threaded assembly
/// (each block is tens to hundreds of additions).
const MIN_BLOCKS: usize = 256;

/// Status of one block in the verification pass of a threaded assembly.
const MATCHED: u8 = 0;
const RESOLVE: u8 = 1;
const UNCACHED: u8 = 2;

/// Span marker of a block without any stored contribution.
const NO_SPAN: (u32, u32) = (u32::MAX, 0);

impl SparseMatrixAij {
    /// The element loop of [`SparseMatrixAij::add_values_cached`] over every
    /// block of a batch, on the threads of `exec`: block `k` has slot
    /// `slots[k]`, the square block of equations `equations(k, ·)` (rows =
    /// columns, `nb_eq` of them) and the column-major values
    /// `values[k·nb_eq²..(k+1)·nb_eq²]`.
    ///
    /// Returns `true` when the batch was added: the values of every entry
    /// are those of the serial loop `for k { add_values_cached(cache,
    /// slots[k], eqs_k, eqs_k, values_k, values_type) }` bit for bit (every
    /// entry receives its contributions in block order, see the module
    /// documentation), and so are the value release, the cache contents and
    /// its resolve counter. Returns `false`, having changed nothing but the
    /// cache's configuration (as the serial loop's first call would), when
    /// the batch does not qualify: one thread, no stored positions yet (the
    /// first rounds of [`BlockPositionCache`]), a released matrix, a block
    /// that the serial loop would not take from the cache (an equation that
    /// fails or is negative, a promotion), a slot repeated in the batch, or
    /// a block whose positions cannot be resolved (a missing entry: the
    /// serial loop reports it after adding the blocks before it). The
    /// caller then runs the serial loop.
    #[allow(clippy::too_many_arguments)] // the serial loop's arguments, batched
    pub fn add_values_cached_batch(
        &mut self,
        exec: &Exec,
        cache: &mut BlockPositionCache,
        slots: &[usize],
        nb_eq: usize,
        equations: &(dyn Fn(usize, &mut Vec<GlobalIdx>) -> Result<()> + Sync),
        values: &[Real],
        values_type: MatrixType,
    ) -> bool {
        let nb = slots.len();
        let symmetric = self.matrix_type == MatrixType::Symmetric;
        if !exec.is_parallel()
            || self.released
            || nb == 0
            || nb_eq == 0
            || cache.stamp != self.position_epoch.0
            || cache.rounds == 0
            || nb_eq.checked_mul(nb_eq).and_then(|n2| n2.checked_mul(nb)) != Some(values.len())
            || (symmetric
                && values_type != MatrixType::Symmetric
                && self.policy == SymmetryPolicy::PromoteOnUnsymmetric)
            || !u32::try_from(self.a.len()).is_ok_and(|n| n < SKIP)
            || !u32::try_from(self.size).is_ok_and(|n| n < u32::MAX)
        {
            return false;
        }
        let mode = match (self.matrix_type, values_type) {
            (MatrixType::Symmetric, MatrixType::Symmetric) => Selection::LocalUpper,
            (MatrixType::Symmetric, _) => Selection::GlobalUpper,
            _ => Selection::All,
        };
        cache.configure(mode, nb_eq, nb_eq, self.size);
        if !cache.rows_only || has_repeated_slot(slots) {
            return false;
        }
        let stride = cache.stride;
        let size = self.size;
        // Pass 1 (threads): equations of every block against the stored
        // slots; span and number of stored contributions of the matched.
        let mut status = vec![MATCHED; nb];
        let mut spans = vec![NO_SPAN; nb];
        let mut touched = vec![0u32; nb];
        {
            let cache_ref: &BlockPositionCache = cache;
            exec.for_each(
                (
                    Tuples::new(&mut status, 1),
                    Tuples::new(&mut spans, 1),
                    Tuples::new(&mut touched, 1),
                ),
                MIN_BLOCKS,
                |first, (mut st, mut sp, mut to)| {
                    verify_part(
                        cache_ref,
                        slots,
                        nb_eq,
                        size,
                        equations,
                        first,
                        st.as_mut_slice(),
                        sp.as_mut_slice(),
                        to.as_mut_slice(),
                    );
                },
            );
        }
        if status.contains(&UNCACHED) {
            return false;
        }
        // Pass 2 (threads): resolve the blocks whose slot is empty or
        // stale, without touching the matrix; then store them in block
        // order (the serial loop's stores and resolve count).
        let todo: Vec<usize> = (0..nb).filter(|&k| status[k] == RESOLVE).collect();
        if !todo.is_empty() {
            let mut positions = vec![0u32; todo.len() * stride];
            let mut keys = vec![0u32; todo.len() * nb_eq];
            let mut failed = vec![false; todo.len()];
            {
                let this: &Self = self;
                exec.for_each(
                    (
                        Tuples::new(&mut positions, stride),
                        Tuples::new(&mut keys, nb_eq),
                        Tuples::new(&mut failed, 1),
                        TuplesRef::new(&todo, 1),
                    ),
                    MIN_BLOCKS / 4,
                    |_, (mut pos, mut key, mut fail, ks)| {
                        let (pos, key, fail) =
                            (pos.as_mut_slice(), key.as_mut_slice(), fail.as_mut_slice());
                        let mut eqs = Vec::with_capacity(nb_eq);
                        let mut buf = Vec::with_capacity(stride);
                        for (t, &k) in ks.as_slice().iter().enumerate() {
                            buf.clear();
                            let ok = equations(k, &mut eqs).is_ok()
                                && eqs.len() == nb_eq
                                && encode_key(&eqs, size, &mut key[t * nb_eq..(t + 1) * nb_eq])
                                && this
                                    .resolve_block_positions(&eqs, &eqs, mode, &mut buf)
                                    .is_ok()
                                && buf.len() == stride;
                            if ok {
                                pos[t * stride..(t + 1) * stride].copy_from_slice(&buf);
                            } else {
                                fail[t] = true;
                            }
                        }
                    },
                );
            }
            if failed.contains(&true) {
                return false;
            }
            for (t, &k) in todo.iter().enumerate() {
                let p = &positions[t * stride..(t + 1) * stride];
                cache.store_with_key(slots[k], &keys[t * nb_eq..(t + 1) * nb_eq], p);
                cache.resolves += 1;
                let (span, n) = span_of(p);
                spans[k] = span;
                touched[k] = n;
            }
        }
        // Pass 3 (threads): the scatter by entry ranges.
        let vidx = value_indices(mode, nb_eq);
        let nnz = self.a.len();
        let parts = exec.nb_parts(nb, MIN_BLOCKS).min(nnz.max(1));
        let bounds: Vec<usize> = (0..=parts).map(|p| p * nnz / parts).collect();
        let (first, last) = block_ranges(&spans, &bounds);
        let positions = &cache.positions;
        exec.for_each_bounds(&bounds, Tuples::new(&mut self.a, 1), |lo, mut part| {
            let a = part.as_mut_slice();
            if a.is_empty() {
                return;
            }
            // the range starting at `lo` (the last of equal bounds)
            let r = bounds.partition_point(|&b| b <= lo) - 1;
            if first[r] > last[r] {
                return;
            }
            scatter_range(
                a,
                lo,
                &slots[first[r]..=last[r]],
                &spans[first[r]..=last[r]],
                &values[first[r] * nb_eq * nb_eq..(last[r] + 1) * nb_eq * nb_eq],
                positions,
                stride,
                &vidx,
                nb_eq * nb_eq,
            );
        });
        let total: u64 = touched.iter().map(|&n| u64::from(n)).sum();
        self.bump_values(total);
        true
    }
}

/// `true` if a slot occurs twice in `slots`.
fn has_repeated_slot(slots: &[usize]) -> bool {
    if slots.windows(2).all(|w| w[0] < w[1]) {
        return false;
    }
    let max = slots.iter().copied().max().unwrap_or(0);
    let mut seen = vec![false; max + 1];
    for &s in slots {
        if std::mem::replace(&mut seen[s], true) {
            return true;
        }
    }
    false
}

/// Encode equations as [`BlockPositionCache::set_key`] does (`false` for a
/// negative equation).
fn encode_key(eqs: &[GlobalIdx], size: GlobalIdx, out: &mut [u32]) -> bool {
    for (o, &g) in out.iter_mut().zip(eqs) {
        *o = if g < 0 {
            return false;
        } else if g >= size {
            u32::MAX
        } else {
            match u32::try_from(g) {
                Ok(v) => v,
                Err(_) => return false,
            }
        };
    }
    true
}

/// Smallest and largest stored position of a block and the number of
/// stored (not skipped) contributions.
fn span_of(positions: &[u32]) -> ((u32, u32), u32) {
    let mut span = NO_SPAN;
    let mut n = 0u32;
    for &p in positions {
        if p != SKIP {
            span.0 = span.0.min(p);
            span.1 = span.1.max(p);
            n += 1;
        }
    }
    (span, n)
}

/// Pass 1 of [`SparseMatrixAij::add_values_cached_batch`] over the blocks
/// `first..first + status.len()`.
#[allow(clippy::too_many_arguments)] // one part of the pass
#[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
fn verify_part(
    cache: &BlockPositionCache,
    slots: &[usize],
    nb_eq: usize,
    size: GlobalIdx,
    equations: &(dyn Fn(usize, &mut Vec<GlobalIdx>) -> Result<()> + Sync),
    first: usize,
    status: &mut [u8],
    spans: &mut [(u32, u32)],
    touched: &mut [u32],
) {
    let mut eqs = Vec::with_capacity(nb_eq);
    let mut key = vec![0u32; nb_eq];
    let stride = cache.stride;
    for (t, ((st, sp), to)) in status
        .iter_mut()
        .zip(spans.iter_mut())
        .zip(touched.iter_mut())
        .enumerate()
    {
        let k = first + t;
        if equations(k, &mut eqs).is_err()
            || eqs.len() != nb_eq
            || !encode_key(&eqs, size, &mut key)
        {
            *st = UNCACHED;
            continue;
        }
        let slot = slots[k];
        let stored = cache.filled.get(slot) == Some(&true)
            && cache.equations[slot * nb_eq..(slot + 1) * nb_eq] == *key;
        if !stored {
            *st = RESOLVE;
            continue;
        }
        let (span, n) = span_of(&cache.positions[slot * stride..(slot + 1) * stride]);
        *sp = span;
        *to = n;
    }
}

/// Value index (`i + j·n`, column-major) of every pair the positions of a
/// slot list, in their order (rows outer, columns inner, `j ≥ i` for the
/// local upper triangle: `resolve_block_positions`).
fn value_indices(mode: Selection, n: usize) -> Vec<usize> {
    let mut v = Vec::new();
    for i in 0..n {
        let j0 = if mode == Selection::LocalUpper { i } else { 0 };
        for j in j0..n {
            v.push(i + j * n);
        }
    }
    v
}

/// First and last block whose span meets each entry range
/// `bounds[r]..bounds[r + 1]` (`first > last` when none does).
fn block_ranges(spans: &[(u32, u32)], bounds: &[usize]) -> (Vec<usize>, Vec<usize>) {
    let parts = bounds.len() - 1;
    let mut first = vec![usize::MAX; parts];
    let mut last = vec![0usize; parts];
    // range of position p: the last r with bounds[r] <= p (non-empty ranges)
    let range_of = |p: u32| bounds.partition_point(|&b| b <= p as usize) - 1;
    for (k, &(lo, hi)) in spans.iter().enumerate() {
        if lo > hi {
            continue;
        }
        for r in range_of(lo)..=range_of(hi).min(parts - 1) {
            if first[r] == usize::MAX {
                first[r] = k;
            }
            last[r] = k;
        }
    }
    (first, last)
}

/// Pass 3 of [`SparseMatrixAij::add_values_cached_batch`] for the entries
/// `lo..lo + a.len()`: the blocks in order, each adding the contributions
/// whose position lies in the range, in the block's pair order.
#[allow(clippy::too_many_arguments)] // one part of the pass
#[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
fn scatter_range(
    a: &mut [Real],
    lo: usize,
    slots: &[usize],
    spans: &[(u32, u32)],
    values: &[Real],
    positions: &[u32],
    stride: usize,
    vidx: &[usize],
    block_len: usize,
) {
    let len = a.len();
    let hi = lo + len;
    for (k, (&slot, &(s0, s1))) in slots.iter().zip(spans).enumerate() {
        if s0 > s1 || (s1 as usize) < lo || (s0 as usize) >= hi {
            continue;
        }
        let pos = &positions[slot * stride..(slot + 1) * stride];
        let v = &values[k * block_len..(k + 1) * block_len];
        for (&p, &vi) in pos.iter().zip(vidx) {
            // SKIP (u32::MAX) is never inside a range (nnz < SKIP)
            let off = (p as usize).wrapping_sub(lo);
            if off < len {
                a[off] += v[vi];
            }
        }
    }
}

/// Smallest number of entries (or rows) per part of a threaded entry-wise
/// operation or product: short streaming loops lose more to the region
/// than they gain.
const MIN_ENTRIES: usize = 32_768;

impl SparseMatrixAij {
    /// [`SparseMatrixAij::set`] on the threads of `exec` (entries are
    /// independent outputs).
    pub fn set_with(&mut self, exec: &Exec, val: Real) {
        self.revive();
        exec.for_each(Tuples::new(&mut self.a, 1), MIN_ENTRIES, |_, mut a| {
            a.as_mut_slice().fill(val);
        });
        self.bump_values(1);
    }

    /// [`SparseMatrixAij::zero`] on the threads of `exec`.
    pub fn zero_with(&mut self, exec: &Exec) {
        self.set_with(exec, 0.0);
    }

    /// [`SparseMatrixAij::mul`] on the threads of `exec`.
    pub fn mul_with(&mut self, exec: &Exec, alpha: Real) {
        self.revive();
        exec.for_each(Tuples::new(&mut self.a, 1), MIN_ENTRIES, |_, mut a| {
            scale_part(a.as_mut_slice(), alpha);
        });
        self.bump_values(1);
    }

    /// [`SparseMatrixAij::add_matrix`] with its entry-wise path (`self` and
    /// `b` store the same entries) on the threads of `exec`.
    pub fn add_matrix_with(&mut self, exec: &Exec, b: &SparseMatrixAij, alpha: Real) -> Result<()> {
        if std::ptr::eq(self, b) {
            return Err(Error::invalid(
                "add_matrix: a matrix cannot be added to itself in place",
            ));
        }
        b.check_not_released("add_matrix")?;
        self.revive();
        let mirror =
            b.matrix_type == MatrixType::Symmetric && self.matrix_type != MatrixType::Symmetric;
        if !mirror && self.same_entries_as(b) {
            self.add_entrywise_with(exec, b, alpha);
            return Ok(());
        }
        self.add_matrix(b, alpha)
    }

    /// [`SparseMatrixAij::add_matrix_upstream`] with its entry-wise path on
    /// the threads of `exec`.
    pub fn add_matrix_upstream_with(
        &mut self,
        exec: &Exec,
        b: &SparseMatrixAij,
        alpha: Real,
    ) -> Result<()> {
        if std::ptr::eq(self, b) {
            return Err(Error::invalid(
                "add_matrix: a matrix cannot be added to itself in place",
            ));
        }
        b.check_not_released("add_matrix")?;
        self.revive();
        if self.same_entries_as(b) {
            self.add_entrywise_with(exec, b, alpha);
            return Ok(());
        }
        self.add_matrix_upstream(b, alpha)
    }

    /// `add_entrywise` on threads: `self[k] += alpha · b[k]` per entry.
    fn add_entrywise_with(&mut self, exec: &Exec, b: &SparseMatrixAij, alpha: Real) {
        exec.for_each(
            (Tuples::new(&mut self.a, 1), TuplesRef::new(&b.a, 1)),
            MIN_ENTRIES,
            |_, (mut s, v)| axpy_part(s.as_mut_slice(), v.as_slice(), alpha),
        );
        self.bump_values(u64::try_from(b.a.len()).unwrap_or(u64::MAX));
    }

    /// [`SparseMatrixAij::copy_profile`] whose zeroing of the values runs
    /// on the threads of `exec` when the coordinates are already those of
    /// `other` (the case of `J = copy_profile(K)` at every step).
    pub fn copy_profile_with(&mut self, exec: &Exec, other: &SparseMatrixAij) {
        if std::ptr::eq(self, other) || !self.same_entries_as(other) || self.released {
            self.copy_profile(other);
            return;
        }
        exec.for_each(Tuples::new(&mut self.a, 1), MIN_ENTRIES, |_, mut a| {
            a.as_mut_slice().fill(0.0);
        });
        self.size = other.size;
        self.profile_release = other.profile_release;
        self.profile_stamp = ProfileStamp::fresh();
        self.position_epoch = ProfileStamp::fresh();
        self.bump_values(1);
    }

    /// [`SparseMatrixAij::apply_boundary`] on the threads of `exec` (every
    /// entry is an independent output).
    pub fn apply_boundary_on(
        &mut self,
        exec: &Exec,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()> {
        if !exec.is_parallel() {
            return self.apply_boundary(layout, blocked, block_val);
        }
        self.revive();
        if blocked.len() != layout.nb_local() {
            return Err(Error::DimensionMismatch {
                context: "SparseMatrixAij::apply_boundary",
                expected: layout.nb_local(),
                got: blocked.len(),
            });
        }
        self.check_local_coordinates(layout)?;
        let identity = layout.is_identity();
        let (irn, jcn) = (&self.irn, &self.jcn);
        exec.for_each(
            (
                Tuples::new(&mut self.a, 1),
                TuplesRef::new(irn, 1),
                TuplesRef::new(jcn, 1),
            ),
            MIN_ENTRIES,
            |_, (mut a, i, j)| {
                boundary_part(
                    a.as_mut_slice(),
                    i.as_slice(),
                    j.as_slice(),
                    layout,
                    identity,
                    blocked,
                    block_val,
                );
            },
        );
        self.bump_values(1);
        Ok(())
    }

    /// [`SparseMatrixAij::mat_vec_mul`] with the rows on the threads of
    /// `exec`: on a layout whose global and local equations coincide, every
    /// row adds its terms (direct entries of the row and, for symmetric
    /// storage, the mirrors of the entries of its column) in storage order
    /// ([`RowGather`]), the order of the serial pass over the entries, so
    /// each row gets the bits of the serial product. Other layouts and
    /// small matrices run the serial product.
    #[allow(clippy::too_many_arguments)] // `mat_vec_mul` + the executor
    pub fn mat_vec_mul_with(
        &self,
        exec: &Exec,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        let n = layout.nb_local();
        if !exec.is_parallel()
            || !layout.is_identity()
            || self.released
            || x.len() != n
            || y.len() != n
            || exec.nb_parts(self.a.len(), MIN_ENTRIES) < 2
        {
            return self.mat_vec_mul(layout, reduction, x, y, alpha, beta);
        }
        self.check_local_coordinates(layout)?;
        let gather = self.row_gather(n);
        if reduction.is_distributed() {
            let mut tmp: Vec<Real> = y
                .iter()
                .enumerate()
                .map(|(l, &yi)| {
                    if layout.ownership(l) == Some(EquationOwnership::Shared) {
                        0.0
                    } else {
                        yi * beta
                    }
                })
                .collect();
            gather.accumulate(exec, &self.a, x, &mut tmp, alpha, None);
            reduction.reduce_and_synchronize(&mut tmp)?;
            for (yi, t) in y.iter_mut().zip(tmp) {
                *yi = t + 0.0;
            }
        } else {
            gather.accumulate(exec, &self.a, x, y, alpha, Some(beta));
        }
        Ok(())
    }

    /// The row-gathered terms of the current entries (built on first use
    /// and kept until an entry moves or is appended).
    fn row_gather(&self, nb_rows: usize) -> Arc<RowGather> {
        let key = (
            self.position_epoch.0,
            self.a.len(),
            self.matrix_type == MatrixType::Symmetric,
            nb_rows,
        );
        let mut slot = self.row_gather.0.lock().unwrap_or_else(|e| e.into_inner());
        if let Some((k, g)) = slot.as_ref()
            && *k == key
        {
            return g.clone();
        }
        let g = Arc::new(RowGather::new(&self.irn, &self.jcn, key.2, nb_rows));
        *slot = Some((key, g.clone()));
        g
    }
}

/// The terms of every row of a matrix-vector product in storage order:
/// row `r` has the entries `k` with row `r` (term `a[k]·x[col]`) and, for
/// symmetric storage, the off-diagonal entries with column `r` (term
/// `a[k]·x[row]`), sorted by `k`. 8 bytes per term: about `16 · nnz` bytes
/// for symmetric storage.
#[derive(Debug)]
pub(super) struct RowGather {
    /// Row `r`'s terms are `start[r]..start[r + 1]`.
    start: Vec<usize>,
    /// Entry of each term.
    entry: Vec<u32>,
    /// Equation `x` is read at for each term.
    other: Vec<u32>,
}

impl RowGather {
    /// The terms of the stored entries `(irn[k], jcn[k])` (local = global
    /// equations, checked by the caller).
    fn new(irn: &[GlobalIdx], jcn: &[GlobalIdx], symmetric: bool, nb_rows: usize) -> Self {
        let idx = |g: GlobalIdx| usize::try_from(g).unwrap_or(usize::MAX);
        let mut start = vec![0usize; nb_rows + 1];
        for (&i, &j) in irn.iter().zip(jcn) {
            start[idx(i) + 1] += 1;
            if symmetric && i != j {
                start[idx(j) + 1] += 1;
            }
        }
        for r in 0..nb_rows {
            start[r + 1] += start[r];
        }
        let total = start[nb_rows];
        let mut fill = start.clone();
        let mut entry = vec![0u32; total];
        let mut other = vec![0u32; total];
        let to32 = |v: usize| u32::try_from(v).unwrap_or(u32::MAX);
        for (k, (&i, &j)) in irn.iter().zip(jcn).enumerate() {
            let (i, j) = (idx(i), idx(j));
            entry[fill[i]] = to32(k);
            other[fill[i]] = to32(j);
            fill[i] += 1;
            if symmetric && i != j {
                entry[fill[j]] = to32(k);
                other[fill[j]] = to32(i);
                fill[j] += 1;
            }
        }
        Self {
            start,
            entry,
            other,
        }
    }

    /// `y[r] (·beta when given) + Σ (alpha·a[k])·x[other]` in term order,
    /// then `+ 0.0` (upstream's zero temporary) when `beta` is given, per
    /// row, rows on the threads of `exec`.
    fn accumulate(
        &self,
        exec: &Exec,
        a: &[Real],
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Option<Real>,
    ) {
        exec.for_each(Tuples::new(y, 1), MIN_ENTRIES / 8, |first, mut part| {
            rows_part(self, a, x, part.as_mut_slice(), first, alpha, beta);
        });
    }
}

/// [`RowGather::accumulate`] for the rows `first..first + y.len()`.
#[inline(never)] // no-alias parameters (docs/benchmarks/perf-campaign/exec)
fn rows_part(
    g: &RowGather,
    a: &[Real],
    x: &[Real],
    y: &mut [Real],
    first: usize,
    alpha: Real,
    beta: Option<Real>,
) {
    for (t, yr) in y.iter_mut().enumerate() {
        let r = first + t;
        let mut acc = match beta {
            Some(b) => *yr * b,
            None => *yr,
        };
        for s in g.start[r]..g.start[r + 1] {
            acc += alpha * a[g.entry[s] as usize] * x[g.other[s] as usize];
        }
        *yr = if beta.is_some() { acc + 0.0 } else { acc };
    }
}

/// The row-gather cache of a matrix: a clone starts empty (another
/// matrix object).
#[derive(Debug, Default)]
pub(super) struct RowGatherCache(Mutex<Option<GatherEntry>>);

/// Key (position epoch, entries, symmetric storage, rows) and terms.
type GatherEntry = ((u64, usize, bool, usize), Arc<RowGather>);

impl Clone for RowGatherCache {
    fn clone(&self) -> Self {
        Self::default()
    }
}

#[inline(never)] // no-alias parameters
fn scale_part(a: &mut [Real], alpha: Real) {
    for v in a {
        *v *= alpha;
    }
}

#[inline(never)] // no-alias parameters
fn axpy_part(s: &mut [Real], v: &[Real], alpha: Real) {
    for (s, &v) in s.iter_mut().zip(v) {
        *s += alpha * v;
    }
}

/// `apply_boundary_with` for one part of the entries.
#[inline(never)] // no-alias parameters
fn boundary_part(
    a: &mut [Real],
    irn: &[GlobalIdx],
    jcn: &[GlobalIdx],
    layout: &EquationLayout,
    identity: bool,
    blocked: &[bool],
    block_val: Real,
) {
    let local = |g: GlobalIdx| {
        if identity {
            g as usize
        } else {
            layout.try_local(g).unwrap_or(usize::MAX)
        }
    };
    for (a, (&gi, &gj)) in a.iter_mut().zip(irn.iter().zip(jcn)) {
        let (li, lj) = (local(gi), local(gj));
        if blocked[li] || blocked[lj] {
            *a = if gi != gj {
                0.0
            } else if layout.is_owned(li) {
                block_val
            } else {
                0.0
            };
        }
    }
}
