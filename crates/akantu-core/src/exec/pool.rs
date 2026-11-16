// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). No upstream
// counterpart: the thread pool and the parallel primitives of the
// execution policy (D-123).

//! [`Exec`]: a policy with its thread pool, and the primitive that runs a
//! closure over contiguous parts of independent outputs.

use std::sync::{Arc, LazyLock, Mutex};

use super::{ExecPolicy, Precision, SimdToken};
use crate::error::{Error, Result};
use crate::types::Real;

/// Parts per thread of an evenly split batch: a few more parts than
/// threads, so that a thread delayed by the system does not hold the
/// region up (the work-stealing pool hands its remaining parts to others).
const PARTS_PER_THREAD: usize = 4;

/// A policy with the thread pool of its thread count. Cloning is cheap (the
/// pool is shared); every `Exec` of a process with the same thread count
/// shares one pool.
#[derive(Clone)]
pub struct Exec {
    policy: ExecPolicy,
    pool: Option<Arc<rayon::ThreadPool>>,
}

impl std::fmt::Debug for Exec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Exec")
            .field("policy", &self.policy)
            .finish_non_exhaustive()
    }
}

impl Default for Exec {
    /// [`Exec::serial`].
    fn default() -> Self {
        Self::serial()
    }
}

/// The process's pools, one per thread count (created on first use, kept
/// until the process ends).
static POOLS: Mutex<Vec<(usize, Arc<rayon::ThreadPool>)>> = Mutex::new(Vec::new());

fn pool_of(threads: usize) -> Result<Arc<rayon::ThreadPool>> {
    let mut pools = POOLS.lock().unwrap_or_else(|e| e.into_inner());
    if let Some((_, p)) = pools.iter().find(|(n, _)| *n == threads) {
        return Ok(p.clone());
    }
    let pool = rayon::ThreadPoolBuilder::new()
        .num_threads(threads)
        .thread_name(move |i| format!("akantu-exec-{threads}-{i}"))
        .build()
        .map_err(|e| Error::invalid(format!("cannot start {threads} threads: {e}")))?;
    let pool = Arc::new(pool);
    pools.push((threads, pool.clone()));
    Ok(pool)
}

impl Exec {
    /// The executor of `policy` (starts the pool of its thread count on
    /// first use).
    ///
    /// # Errors
    /// The operating system refuses the threads.
    pub fn new(policy: ExecPolicy) -> Result<Self> {
        let pool = if policy.threads() > 1 {
            Some(pool_of(policy.threads())?)
        } else {
            None
        };
        Ok(Self { policy, pool })
    }

    /// One thread, bitwise, the detected SIMD level.
    #[must_use]
    pub fn serial() -> Self {
        Self {
            policy: ExecPolicy::serial(),
            pool: None,
        }
    }

    /// A shared [`Exec::serial`] (for code without a model, e.g. a default
    /// trait method).
    #[must_use]
    pub fn serial_ref() -> &'static Self {
        static SERIAL: LazyLock<Exec> = LazyLock::new(Exec::serial);
        &SERIAL
    }

    /// The policy.
    #[must_use]
    pub const fn policy(&self) -> ExecPolicy {
        self.policy
    }

    /// Thread count.
    #[must_use]
    pub const fn threads(&self) -> usize {
        self.policy.threads()
    }

    /// Precision mode.
    #[must_use]
    pub const fn precision(&self) -> Precision {
        self.policy.precision()
    }

    /// The SIMD level as a dispatch token.
    #[must_use]
    pub const fn simd_token(&self) -> SimdToken {
        self.policy.simd_token()
    }

    /// `true` when regions run on more than one thread.
    #[must_use]
    pub const fn is_parallel(&self) -> bool {
        self.pool.is_some()
    }

    /// Number of parts an even split of `len` items with at least
    /// `min_items` per part uses (1 without a pool).
    #[must_use]
    pub fn nb_parts(&self, len: usize, min_items: usize) -> usize {
        if self.pool.is_none() {
            return 1;
        }
        let by_size = len / min_items.max(1);
        by_size.clamp(1, self.threads() * PARTS_PER_THREAD)
    }

    /// Run `f(first, part)` on contiguous parts of `items` (a [`Split`]:
    /// one or several arrays of equal length in items), each part at least
    /// `min_items` long; `first` is the index of the part's first item.
    /// Without a pool, or when the batch is too small to split, one call
    /// with every item on the calling thread.
    ///
    /// The parts must be independent: `f` writes only its part of the
    /// split arrays (the borrow checker enforces it) and reads shared data.
    /// Each output item is then computed by the same code whatever the
    /// split, so the results do not depend on the thread count.
    ///
    /// # Panics
    /// The arrays of `items` differ in length (an internal invariant of the
    /// caller).
    pub fn for_each<S: Split>(&self, items: S, min_items: usize, f: impl Fn(usize, S) + Sync) {
        assert!(
            items.lens_agree(),
            "exec: split arrays of different lengths"
        );
        let len = items.len();
        let parts = self.nb_parts(len, min_items);
        match &self.pool {
            Some(pool) if parts > 1 => {
                let bounds = even_bounds(len, parts);
                run_parts(pool, &bounds, items, &|first, part| f(first, part));
            }
            _ => f(0, items),
        }
    }

    /// [`Exec::for_each`] with a fallible `f`: every part runs; the error
    /// returned is that of the first failing part in item order (the error
    /// the serial loop reports when the parts stop at their first error).
    /// On an error, items after the failing one may have been written (by
    /// other parts), unlike the serial loop.
    ///
    /// # Errors
    /// The first error in item order.
    ///
    /// # Panics
    /// As [`Exec::for_each`].
    pub fn try_for_each<S: Split>(
        &self,
        items: S,
        min_items: usize,
        f: impl Fn(usize, S) -> Result<()> + Sync,
    ) -> Result<()> {
        assert!(
            items.lens_agree(),
            "exec: split arrays of different lengths"
        );
        let len = items.len();
        let parts = self.nb_parts(len, min_items);
        match &self.pool {
            Some(pool) if parts > 1 => {
                let bounds = even_bounds(len, parts);
                let errors: Mutex<Vec<(usize, Error)>> = Mutex::new(Vec::new());
                run_parts(pool, &bounds, items, &|first, part| {
                    if let Err(e) = f(first, part) {
                        errors
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .push((first, e));
                    }
                });
                let errors = errors.into_inner().unwrap_or_else(|e| e.into_inner());
                match errors.into_iter().min_by_key(|(first, _)| *first) {
                    Some((_, e)) => Err(e),
                    None => Ok(()),
                }
            }
            _ => f(0, items),
        }
    }

    /// Run `f(first, part)` on the parts of `items` delimited by `bounds`
    /// (`bounds[0] = 0`, increasing, last = `items.len()`), in parallel when
    /// there is a pool; for splits that are not even (e.g. node ranges with
    /// balanced work, [`super::NodeScatter`]).
    ///
    /// # Panics
    /// Invalid `bounds`, or arrays of different lengths.
    pub fn for_each_bounds<S: Split>(
        &self,
        bounds: &[usize],
        items: S,
        f: impl Fn(usize, S) + Sync,
    ) {
        assert!(
            items.lens_agree(),
            "exec: split arrays of different lengths"
        );
        assert!(
            bounds.first() == Some(&0)
                && bounds.last() == Some(&items.len())
                && bounds.windows(2).all(|w| w[0] <= w[1]),
            "exec: invalid part bounds"
        );
        match &self.pool {
            Some(pool) if bounds.len() > 2 => {
                run_parts(pool, bounds, items, &|first, part| f(first, part))
            }
            _ => f(0, items),
        }
    }

    /// `Σ values` in [`Precision::Bitwise`]: the serial left-to-right sum
    /// from `0.0` (the same bits for every thread count). In
    /// [`Precision::Fast`]: per-part sums added in part order (the result
    /// depends on the thread count).
    #[must_use]
    pub fn sum_in_order(&self, values: &[Real]) -> Real {
        let parts = self.nb_parts(values.len(), 4096);
        if self.precision() == Precision::Bitwise || parts == 1 {
            return values.iter().fold(0.0, |acc, &v| acc + v);
        }
        let mut partial = vec![0.0; parts];
        let bounds = even_bounds(values.len(), parts);
        if let Some(pool) = &self.pool {
            let per_part: Vec<usize> = (0..=parts).collect();
            run_parts(
                pool,
                &per_part,
                Tuples::new(&mut partial, 1),
                &|p, mut out| {
                    let v = &values[bounds[p]..bounds[p + 1]];
                    out.as_mut_slice()[0] = v.iter().fold(0.0, |acc, &x| acc + x);
                },
            );
        }
        partial.iter().fold(0.0, |acc, &v| acc + v)
    }
}

/// `parts + 1` boundaries of an even split of `len` items.
fn even_bounds(len: usize, parts: usize) -> Vec<usize> {
    (0..=parts).map(|p| p * len / parts).collect()
}

/// Split `items` at `bounds` and run `f` on every part in `pool`.
fn run_parts<S: Split>(
    pool: &rayon::ThreadPool,
    bounds: &[usize],
    items: S,
    f: &(dyn Fn(usize, S) + Sync),
) {
    let mut parts = Vec::with_capacity(bounds.len().saturating_sub(1));
    let mut rest = items;
    for w in bounds.windows(2) {
        let (head, tail) = rest.split_at(w[1] - w[0]);
        parts.push((w[0], head));
        rest = tail;
    }
    pool.install(|| {
        use rayon::prelude::*;
        parts
            .into_par_iter()
            .with_max_len(1)
            .for_each(|(first, part)| f(first, part));
    });
}

/// Arrays that split into contiguous parts at an item index: [`Tuples`]
/// (mutable), [`TuplesRef`] (shared) and tuples of these (all of the same
/// length in items).
pub trait Split: Sized + Send {
    /// Number of items.
    fn len(&self) -> usize;
    /// `true` without items.
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
    /// The items `[0, mid)` and `[mid, len)`.
    fn split_at(self, mid: usize) -> (Self, Self);
    /// `true` when every array has the same number of items.
    fn lens_agree(&self) -> bool {
        true
    }
}

/// A mutable array of items of `width` values each.
#[derive(Debug)]
pub struct Tuples<'a, T> {
    data: &'a mut [T],
    width: usize,
}

impl<'a, T> Tuples<'a, T> {
    /// `data` as items of `width` values.
    ///
    /// # Panics
    /// `width == 0` or `data.len()` not a multiple of `width` (an internal
    /// invariant of the caller's arrays).
    pub fn new(data: &'a mut [T], width: usize) -> Self {
        assert!(
            width > 0 && data.len().is_multiple_of(width),
            "exec: {} values are not items of {width}",
            data.len()
        );
        Self { data, width }
    }

    /// The values.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.data
    }

    /// The values, consuming the part.
    pub fn into_slice(self) -> &'a mut [T] {
        self.data
    }

    /// Values per item.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }
}

impl<T: Send> Split for Tuples<'_, T> {
    fn len(&self) -> usize {
        self.data.len() / self.width
    }
    fn split_at(self, mid: usize) -> (Self, Self) {
        let (a, b) = self.data.split_at_mut(mid * self.width);
        (
            Self {
                data: a,
                width: self.width,
            },
            Self {
                data: b,
                width: self.width,
            },
        )
    }
}

/// A shared array of items of `width` values each.
#[derive(Debug, Clone, Copy)]
pub struct TuplesRef<'a, T> {
    data: &'a [T],
    width: usize,
}

impl<'a, T> TuplesRef<'a, T> {
    /// `data` as items of `width` values.
    ///
    /// # Panics
    /// As [`Tuples::new`].
    pub fn new(data: &'a [T], width: usize) -> Self {
        assert!(
            width > 0 && data.len().is_multiple_of(width),
            "exec: {} values are not items of {width}",
            data.len()
        );
        Self { data, width }
    }

    /// The values.
    #[must_use]
    pub const fn as_slice(&self) -> &'a [T] {
        self.data
    }

    /// Values per item.
    #[must_use]
    pub const fn width(&self) -> usize {
        self.width
    }
}

impl<T: Sync> Split for TuplesRef<'_, T> {
    fn len(&self) -> usize {
        self.data.len() / self.width
    }
    fn split_at(self, mid: usize) -> (Self, Self) {
        let (a, b) = self.data.split_at(mid * self.width);
        (
            Self {
                data: a,
                width: self.width,
            },
            Self {
                data: b,
                width: self.width,
            },
        )
    }
}

macro_rules! split_tuple {
    ($first:ident $(, $name:ident)*) => {
        #[allow(non_snake_case)] // the type parameters name the elements
        impl<$first: Split $(, $name: Split)*> Split for ($first, $($name,)*) {
            fn len(&self) -> usize {
                self.0.len()
            }
            fn split_at(self, mid: usize) -> (Self, Self) {
                let ($first, $($name,)*) = self;
                let $first = $first.split_at(mid);
                $(let $name = $name.split_at(mid);)*
                (($first.0, $($name.0,)*), ($first.1, $($name.1,)*))
            }
            fn lens_agree(&self) -> bool {
                let ($first, $($name,)*) = self;
                $first.lens_agree() $(&& $name.len() == $first.len() && $name.lens_agree())*
            }
        }
    };
}

split_tuple!(A);
split_tuple!(A, B);
split_tuple!(A, B, C);
split_tuple!(A, B, C, D);
split_tuple!(A, B, C, D, E);
split_tuple!(A, B, C, D, E, F);

#[cfg(test)]
mod tests {
    use super::*;

    fn execs() -> Vec<Exec> {
        [1, 2, 3, 4, 8]
            .into_iter()
            .map(|n| Exec::new(ExecPolicy::serial().with_threads(n).unwrap()).unwrap())
            .collect()
    }

    #[test]
    fn independent_outputs_do_not_depend_on_the_split() {
        let x: Vec<Real> = (0..10_001).map(|i| (i as Real).sin()).collect();
        let reference: Vec<Real> = x.chunks(1).map(|v| v[0] * 3.0 + 1.0).collect();
        for exec in execs() {
            for min in [1, 7, 1000, 100_000] {
                let mut out = vec![0.0; x.len()];
                exec.for_each(
                    (Tuples::new(&mut out, 1), TuplesRef::new(&x, 1)),
                    min,
                    |first, (mut o, xs)| {
                        for (k, (o, &v)) in
                            o.as_mut_slice().iter_mut().zip(xs.as_slice()).enumerate()
                        {
                            assert_eq!(v.to_bits(), x[first + k].to_bits());
                            *o = v * 3.0 + 1.0;
                        }
                    },
                );
                assert_eq!(out, reference);
            }
        }
    }

    #[test]
    fn first_error_in_item_order() {
        for exec in execs() {
            let mut out = vec![0usize; 1000];
            let r = exec.try_for_each(Tuples::new(&mut out, 2), 1, |first, mut o| {
                for (k, v) in o.as_mut_slice().as_chunks_mut::<2>().0.iter_mut().enumerate() {
                    let i = first + k;
                    if i == 123 || i == 400 {
                        return Err(Error::invalid(format!("item {i}")));
                    }
                    v[0] = i;
                }
                Ok(())
            });
            assert_eq!(
                r.unwrap_err().to_string(),
                Error::invalid("item 123").to_string()
            );
        }
    }

    #[test]
    fn uneven_bounds() {
        for exec in execs() {
            let mut out = vec![0u32; 20];
            exec.for_each_bounds(
                &[0, 0, 3, 3, 19, 20],
                Tuples::new(&mut out, 1),
                |first, mut o| {
                    for (k, v) in o.as_mut_slice().iter_mut().enumerate() {
                        *v = u32::try_from(first + k).unwrap();
                    }
                },
            );
            assert_eq!(out, (0..20).collect::<Vec<u32>>());
        }
    }

    #[test]
    fn bitwise_sum_is_the_serial_sum() {
        let v: Vec<Real> = (0..100_000).map(|i| 1.0 / (1.0 + i as Real)).collect();
        let serial = v.iter().fold(0.0, |a, &x| a + x);
        for exec in execs() {
            assert_eq!(exec.sum_in_order(&v).to_bits(), serial.to_bits());
            let fast = Exec::new(exec.policy().with_precision(Precision::Fast)).unwrap();
            assert!((fast.sum_in_order(&v) - serial).abs() <= 1e-12 * serial);
        }
    }

    #[test]
    #[should_panic(expected = "different lengths")]
    fn mismatched_lengths_panic() {
        let mut a = vec![0.0; 6];
        let b = vec![0.0; 6];
        Exec::serial().for_each(
            (Tuples::new(&mut a, 2), TuplesRef::new(&b, 3)),
            1,
            |_, _| {},
        );
    }

    #[test]
    fn pools_are_shared() {
        let p = ExecPolicy::serial().with_threads(3).unwrap();
        let (a, b) = (Exec::new(p).unwrap(), Exec::new(p).unwrap());
        assert!(Arc::ptr_eq(
            a.pool.as_ref().unwrap(),
            b.pool.as_ref().unwrap()
        ));
        assert!(!Exec::serial().is_parallel());
    }
}
