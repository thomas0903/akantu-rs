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
