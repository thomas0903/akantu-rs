// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_array.hh, aka_array_tmpl.hh,
// aka_array_view.hh, aka_view_iterators.hh (make_view), ArrayProxy.

//! [`Array<T>`]: the workhorse container — a flat, contiguous buffer
//! interpreted as `len()` *tuples* of `nb_component()` values each
//! (struct-of-arrays per field, tuple-major: component `j` of tuple `i` is at
//! `i * nb_component + j`, exactly as upstream and as numpy `(len, nbc)`
//! C-ordered arrays).
//!
//! Typical tuple interpretations (upstream `make_view(array, …)`):
//! * nodal vector fields: `nb_component = spatial_dimension`;
//! * per-quadrature-point tensors: `nb_component = rows * cols`, each tuple
//!   viewed as a **column-major** matrix ([`Array::tuple_matrix`]), identical
//!   to upstream's Eigen `Map`;
//! * connectivities: `Array<Idx>` with `nb_component = nodes_per_element`.
//!
//! # Zero components
//!
//! As upstream (`ArrayBase::size_` is stored separately from the data), the
//! number of tuples is stored explicitly, so `nb_component == 0` is valid: an
//! array of `len` empty tuples with no storage (e.g. shape derivatives of
//! `_point_1`, natural dimension 0). Tuple accessors and iterators return
//! empty slices for such arrays.
//!
//! # Checked sizes
//!
//! Every size product is checked. The `try_*` constructors / mutators
//! ([`Array::try_filled`], [`Array::try_resize`], [`Array::try_reserve`], …)
//! return [`Error::CapacityExceeded`] **before** modifying any state, also
//! for allocations the allocator would refuse (`Vec::try_reserve`); they are
//! the entry points for user, parser, file and FFI sizes. The infallible
//! versions are for internally pre-validated sizes and panic with a clear
//! message instead of silently wrapping (never a wrong-sized array).
//!
//! Borrowed data (numpy buffers, MPI receive buffers, sub-ranges) is handled
//! by [`ArrayView`] / [`ArrayViewMut`] instead of upstream's `ArrayProxy`.
//! Numerical kernels should accept views so that both owned arrays and
//! foreign buffers can be passed without copies.
//!
//! An `Array` is **never exported** to foreign code, so its un-guarded
//! slice accessors cannot alias NumPy/C memory. Exportable fields use
//! [`crate::array_shared::SharedArray`], whose elements are reachable only
//! through guards (foreign-view contract,
//! `docs/ARCHITECTURE.md`); it reuses this type and its export-capable
//! storage.

use std::fmt;

use crate::array_storage::ArrayStorage;
use crate::error::{Error, Result};
use crate::types::{Id, Real, checked_len};

/// Owned, contiguous array of fixed-size tuples.
#[derive(Clone, PartialEq)]
pub struct Array<T> {
    /// `len * nb_component` values (invariant); export-capable storage,
    /// exported only through [`crate::array_shared::SharedArray`].
    pub(crate) data: ArrayStorage<T>,
    /// Number of tuples (stored: `nb_component` may be 0).
    len: usize,
    nb_component: usize,
    id: Id,
}

impl<T> Default for Array<T> {
    /// Empty single-component array (upstream `Array()`).
    fn default() -> Self {
        Self::new(1)
    }
}

/// Map an allocation failure / overflow to [`Error::CapacityExceeded`].
fn reserve_error<T>(what: &'static str, values: usize) -> Error {
    Error::CapacityExceeded {
        what,
        requested: (values as u128) * (std::mem::size_of::<T>() as u128),
        limit: isize::MAX as u128,
    }
}

impl<T> Array<T> {
    /// Empty array with `nb_component` components per tuple (`0` allowed).
    #[must_use]
    pub fn new(nb_component: usize) -> Self {
        Self {
            data: ArrayStorage::new(),
            len: 0,
            nb_component,
            id: Id::new(),
        }
    }

    /// Empty array with an identifier (upstream `Array(0, nbc, id)`).
    #[must_use]
    pub fn with_id(nb_component: usize, id: impl Into<Id>) -> Self {
        let mut a = Self::new(nb_component);
        a.id = id.into();
        a
    }

    /// Wrap an existing flat vector. Fails if `data.len()` is not a multiple
    /// of `nb_component`. With `nb_component == 0`, `data` must be empty and
    /// the result has zero tuples (use [`Array::try_from_vec_with_len`] to
    /// give a tuple count).
    pub fn from_vec(data: Vec<T>, nb_component: usize) -> Result<Self> {
        let len = match nb_component {
            0 if data.is_empty() => 0,
            0 => {
                return Err(Error::DimensionMismatch {
                    context: "Array::from_vec (nb_component = 0)",
                    expected: 0,
                    got: data.len(),
                });
            }
            n if data.len().is_multiple_of(n) => data.len() / n,
            n => {
                return Err(Error::DimensionMismatch {
                    context: "Array::from_vec",
                    expected: n,
                    got: data.len(),
                });
            }
        };
        Ok(Self {
            data: data.into(),
            len,
            nb_component,
            id: Id::new(),
        })
    }

    /// Wrap a flat vector holding exactly `len` tuples (checked product).
    pub fn try_from_vec_with_len(data: Vec<T>, len: usize, nb_component: usize) -> Result<Self> {
        let n = checked_len(len, nb_component, "Array storage")?;
        if n != data.len() {
            return Err(Error::DimensionMismatch {
                context: "Array::try_from_vec_with_len",
                expected: n,
                got: data.len(),
            });
        }
        Ok(Self {
            data: data.into(),
            len,
            nb_component,
            id: Id::new(),
        })
    }

    /// Number of tuples (upstream `size()`).
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` if there are no tuples.
    #[inline(always)]
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Components per tuple (upstream `getNbComponent()`).
    #[inline(always)]
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Set the identifier.
    pub fn set_id(&mut self, id: impl Into<Id>) {
        self.id = id.into();
    }

    /// Flat data (`len() * nb_component()` values).
    #[inline(always)]
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    /// Iterator over all values (tuple-major; `as_slice().iter()`).
    pub fn iter(&self) -> std::slice::Iter<'_, T> {
        self.as_slice().iter()
    }

    /// Mutable flat data.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Consume into the flat vector.
    #[must_use]
    pub fn into_vec(self) -> Vec<T> {
        self.data.into_vec()
    }

    /// Borrowed view.
    #[inline]
    #[must_use]
    pub fn view(&self) -> ArrayView<'_, T> {
        ArrayView {
            data: &self.data,
            len: self.len,
            nb_component: self.nb_component,
        }
    }

    /// Mutable borrowed view.
    #[inline]
    pub fn view_mut(&mut self) -> ArrayViewMut<'_, T> {
        ArrayViewMut {
            data: &mut self.data,
            len: self.len,
            nb_component: self.nb_component,
        }
    }

    /// Tuple `i`. Out-of-bounds `i` panics (internal invariant; use
    /// [`Array::get_tuple`] for unvalidated indices).
    #[inline(always)]
    #[must_use]
    #[track_caller]
    pub fn tuple(&self, i: usize) -> &[T] {
        assert!(i < self.len, "Array::tuple: index out of bounds");
        let n = self.nb_component;
        &self.data[i * n..(i + 1) * n]
    }

    /// Mutable tuple `i` (same contract as [`Array::tuple`]).
    #[inline(always)]
    #[track_caller]
    pub fn tuple_mut(&mut self, i: usize) -> &mut [T] {
        assert!(i < self.len, "Array::tuple_mut: index out of bounds");
        let n = self.nb_component;
        &mut self.data[i * n..(i + 1) * n]
    }

    /// Checked tuple access.
    pub fn get_tuple(&self, i: usize) -> Result<&[T]> {
        if i < self.len {
            Ok(self.tuple(i))
        } else {
            Err(Error::OutOfRange {
                what: "array tuple",
                index: i,
                len: self.len,
            })
        }
    }

    /// Checked mutable tuple access.
    pub fn get_tuple_mut(&mut self, i: usize) -> Result<&mut [T]> {
        if i < self.len {
            Ok(self.tuple_mut(i))
        } else {
            Err(Error::OutOfRange {
                what: "array tuple",
                index: i,
                len: self.len,
            })
        }
    }

    /// Iterator over tuples (upstream `make_view(array, nbc)`); yields `len()`
    /// slices, empty ones when `nb_component == 0`.
    pub fn tuples(&self) -> Tuples<'_, T> {
        Tuples::new(&self.data, self.len, self.nb_component)
    }

    /// Mutable iterator over tuples.
    pub fn tuples_mut(&mut self) -> TuplesMut<'_, T> {
        TuplesMut::new(&mut self.data, self.len, self.nb_component)
    }

    /// Reinterpret as a slice of fixed-size tuples `[T; N]`
    /// (zero-cost, safe; `N` must equal `nb_component()`, `N = 0` allowed).
    pub fn as_fixed<const N: usize>(&self) -> Result<&[[T; N]]> {
        if N != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "Array::as_fixed",
                expected: self.nb_component,
                got: N,
            });
        }
        if N == 0 {
            return Ok(zst_slice(self.len));
        }
        Ok(self.data.as_chunks::<N>().0)
    }

    /// Mutable version of [`Array::as_fixed`].
    pub fn as_fixed_mut<const N: usize>(&mut self) -> Result<&mut [[T; N]]> {
        if N != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "Array::as_fixed_mut",
                expected: self.nb_component,
                got: N,
            });
        }
        if N == 0 {
            return Ok(zst_slice_mut(self.len));
        }
        Ok(self.data.as_chunks_mut::<N>().0)
    }

    /// Reserve capacity for `additional` more tuples, checked; no state
    /// change on error.
    pub fn try_reserve(&mut self, additional: usize) -> Result<()> {
        let values = checked_len(additional, self.nb_component, "Array reserve")?;
        self.data
            .try_reserve(values)
            .map_err(|_| reserve_error::<T>("Array reserve", values))
    }

    /// Reserve capacity for `additional` more tuples.
    ///
    /// # Panics
    /// On size overflow / allocation refusal (use [`Array::try_reserve`]).
    #[track_caller]
    pub fn reserve(&mut self, additional: usize) {
        if let Err(e) = self.try_reserve(additional) {
            panic!("Array::reserve: {e}");
        }
    }

    /// Capacity in tuples (`usize::MAX` for zero-component arrays, which
    /// never allocate; upstream divides by zero there).
    #[must_use]
    pub fn capacity(&self) -> usize {
        self.data
            .capacity()
            .checked_div(self.nb_component)
            .unwrap_or(usize::MAX)
    }

    /// An exported block is retired on **every** decrease of the tuple
    /// count, also when no value is removed (zero-component arrays): views
    /// detach monotonically.
    /// Set the tuple count and record it in the export anchor (a cached
    /// lease then knows whether it still covers every live tuple).
    fn set_len(&mut self, len: usize) {
        self.len = len;
        self.data.note_tuples(len);
    }

    fn detach_on_tuple_shrink(&mut self, new_len: usize) {
        if new_len < self.len {
            self.data.detach_if_exported();
        }
    }

    /// Remove all tuples (upstream `clear()` = `resize(0)`).
    pub fn clear(&mut self) {
        self.detach_on_tuple_shrink(0);
        self.data.clear();
        self.set_len(0);
    }

    /// Keep the first `new_len` tuples (no-op if `new_len >= len()`).
    pub fn truncate(&mut self, new_len: usize) {
        if new_len < self.len {
            self.detach_on_tuple_shrink(new_len);
            self.data.truncate(new_len * self.nb_component);
            self.set_len(new_len);
        }
    }

    /// Remove tuple `i` by moving the **last** tuple into its place
    /// (upstream `Array::erase` semantics — order is *not* preserved). Never
    /// reallocates an unexported array (an exported one is detached first,
    /// so its views keep the pre-erase values).
    pub fn swap_remove_tuple(&mut self, i: usize) -> Result<()> {
        let len = self.len;
        if i >= len {
            return Err(Error::OutOfRange {
                what: "array tuple",
                index: i,
                len,
            });
        }
        self.data.detach_if_exported();
        let n = self.nb_component;
        if i != len - 1 {
            let (head, tail) = self.data.split_at_mut((len - 1) * n);
            head[i * n..(i + 1) * n].swap_with_slice(&mut tail[..n]);
        }
        self.data.truncate((len - 1) * n);
        self.set_len(len - 1);
        Ok(())
    }

    /// Memory used by the values in bytes (upstream `getMemorySize`).
    #[must_use]
    pub fn memory_size(&self) -> usize {
        self.data.capacity() * std::mem::size_of::<T>()
    }
}

impl<T> Array<T> {
    /// Tuple `i` viewed as a column-major `R × C` matrix `m[col][row]`
    /// (upstream `make_view<R, C>(array)`, for any `T`; for `Real` the type is [`Matrix<R, C>`](crate::tensor::Matrix)); requires
    /// `nb_component == R * C` (`R` or `C` may be 0).
    #[inline]
    pub fn tuple_matrix<const R: usize, const C: usize>(&self, i: usize) -> Result<&[[T; R]; C]> {
        crate::tensor::as_matrix_of::<T, R, C>(self.get_tuple(i)?)
    }

    /// Mutable version of [`Array::tuple_matrix`].
    #[inline]
    pub fn tuple_matrix_mut<const R: usize, const C: usize>(
        &mut self,
        i: usize,
    ) -> Result<&mut [[T; R]; C]> {
        crate::tensor::as_matrix_of_mut::<T, R, C>(self.get_tuple_mut(i)?)
    }
}

impl<T: Clone> Array<T> {
    /// `size` tuples, all components set to `value`
    /// (upstream `Array(size, nbc, value)`); checked, never allocates past a
    /// size overflow.
    pub fn try_filled(size: usize, nb_component: usize, value: T) -> Result<Self> {
        let n = checked_len(size, nb_component, "Array storage")?;
        let mut data = Vec::new();
        data.try_reserve_exact(n)
            .map_err(|_| reserve_error::<T>("Array storage", n))?;
        data.resize(n, value);
        Ok(Self {
            data: data.into(),
            len: size,
            nb_component,
            id: Id::new(),
        })
    }

    /// Infallible [`Array::try_filled`] for pre-validated sizes.
    ///
    /// # Panics
    /// On size overflow or allocation refusal.
    #[must_use]
    #[track_caller]
    pub fn filled(size: usize, nb_component: usize, value: T) -> Self {
        match Self::try_filled(size, nb_component, value) {
            Ok(a) => a,
            Err(e) => panic!("Array::filled: {e}"),
        }
    }

    /// Resize to `new_len` tuples, new components set to `fill`. Checked:
    /// on error the array is unchanged.
    pub fn try_resize(&mut self, new_len: usize, fill: T) -> Result<()> {
        let n = checked_len(new_len, self.nb_component, "Array storage")?;
        self.detach_on_tuple_shrink(new_len);
        self.data
            .try_resize(n, fill)
            .map_err(|_| reserve_error::<T>("Array storage", n))?;
        self.set_len(new_len);
        Ok(())
    }

    /// Infallible [`Array::try_resize`] for pre-validated sizes.
    ///
    /// # Panics
    /// On size overflow or allocation refusal.
    #[track_caller]
    pub fn resize(&mut self, new_len: usize, fill: T) {
        if let Err(e) = self.try_resize(new_len, fill) {
            panic!("Array::resize: {e}");
        }
    }

    /// Append a tuple (upstream `push_back(Vector)`).
    ///
    /// # Panics
    /// If `tuple.len() != nb_component()`; use [`Array::try_push_tuple`] for
    /// unvalidated input.
    #[track_caller]
    pub fn push_tuple(&mut self, tuple: &[T]) {
        assert_eq!(
            tuple.len(),
            self.nb_component,
            "Array::push_tuple: wrong tuple size"
        );
        if let Err(e) = self.data.try_extend_from_slice(tuple) {
            panic!("Array::push_tuple: {e}");
        }
        self.set_len(self.len + 1);
    }

    /// Fallible [`Array::push_tuple`] (shape and allocation checked; no state
    /// change on error).
    pub fn try_push_tuple(&mut self, tuple: &[T]) -> Result<()> {
        if tuple.len() != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "Array::push_tuple",
                expected: self.nb_component,
                got: tuple.len(),
            });
        }
        self.data
            .try_extend_from_slice(tuple)
            .map_err(|_| reserve_error::<T>("Array::push_tuple", tuple.len()))?;
        self.set_len(self.len + 1);
        Ok(())
    }

    /// Append a tuple with every component equal to `value`
    /// (upstream `push_back(const T &)`).
    ///
    /// # Panics
    /// On size overflow or allocation refusal.
    pub fn push_value(&mut self, value: T) {
        let new_len = self.data.len() + self.nb_component;
        if let Err(e) = self.data.try_resize(new_len, value) {
            panic!("Array::push_value: {e}");
        }
        self.set_len(self.len + 1);
    }

    /// Set every component to `value` (upstream `set(t)`).
    pub fn fill(&mut self, value: T) {
        self.data.fill(value);
    }

    /// Set every tuple to `tuple` (upstream `set(vector)`).
    pub fn fill_tuples(&mut self, tuple: &[T]) -> Result<()> {
        if tuple.len() != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "Array::fill_tuples",
                expected: self.nb_component,
                got: tuple.len(),
            });
        }
        for t in self.tuples_mut() {
            t.clone_from_slice(tuple);
        }
        Ok(())
    }

    /// Append all tuples of `other` (upstream `append`); checked.
    pub fn append(&mut self, other: &Array<T>) -> Result<()> {
        if other.nb_component != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "Array::append",
                expected: self.nb_component,
                got: other.nb_component,
            });
        }
        let new_len = self
            .len
            .checked_add(other.len)
            .ok_or(Error::CapacityExceeded {
                what: "Array append",
                requested: self.len as u128 + other.len as u128,
                limit: usize::MAX as u128,
            })?;
        self.data
            .try_extend_from_slice(&other.data)
            .map_err(|_| reserve_error::<T>("Array append", other.data.len()))?;
        self.set_len(new_len);
        Ok(())
    }

    /// Replace the content by a copy of `other` (upstream `copy`), keeping
    /// this array's id. With equal lengths the copy is done **in place** (the
    /// storage is not replaced, so views of a [`crate::array_shared::SharedArray`]
    /// stay attached — upstream `InternalField::restorePreviousValues`
    /// semantics).
    pub fn copy_from(&mut self, other: &Array<T>) -> Result<()> {
        if other.nb_component != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "Array::copy_from",
                expected: self.nb_component,
                got: other.nb_component,
            });
        }
        if other.len == self.len {
            self.data.clone_from_slice(&other.data);
        } else {
            self.detach_on_tuple_shrink(other.len);
            self.data
                .try_clone_from_slice(&other.data)
                .map_err(|_| reserve_error::<T>("Array::copy_from", other.data.len()))?;
            self.set_len(other.len);
        }
        Ok(())
    }
}

impl<T: Clone + Default> Array<T> {
    /// Checked `size` tuples set to `T::default()` (zero for numbers).
    pub fn try_zeros(size: usize, nb_component: usize) -> Result<Self> {
        Self::try_filled(size, nb_component, T::default())
    }

    /// `size` tuples set to `T::default()` (zero for numbers).
    ///
    /// # Panics
    /// On size overflow or allocation refusal (use [`Array::try_zeros`]).
    #[must_use]
    #[track_caller]
    pub fn zeros(size: usize, nb_component: usize) -> Self {
        Self::filled(size, nb_component, T::default())
    }

    /// Set everything to `T::default()` (upstream `zero()`).
    pub fn zero(&mut self) {
        self.data.fill(T::default());
    }
}

impl<T: PartialEq> Array<T> {
    /// Position of the first tuple equal to `tuple` (upstream `find`, which
    /// returns `-1` when absent).
    #[must_use]
    pub fn find_tuple(&self, tuple: &[T]) -> Option<usize> {
        if tuple.len() != self.nb_component {
            return None;
        }
        self.tuples().position(|t| t == tuple)
    }
}

/// Element comparison of [`Array::find`] (upstream `Array<T>::find`).
///
/// Upstream's generic `find(elem)` (`aka_array_tmpl.hh:639-645`) is
/// `std::find` (`==`) and returns the index of the first match; the
/// `Array<Real>` specialisation (`aka_array.cc:35-44`) matches the first
/// stored `a` with `|a − elem| ≤ ε`, `ε = numeric_limits<Real>::epsilon()`
/// (an **absolute** bound: `find(0.0)` matches `1e-300`, `find(1.0)`
/// matches `1 + ε` but not `1 + 2ε`, `find(1e20)` matches only `1e20`
/// itself; NaN and the infinities never match, `∞ − ∞` being NaN), and
/// returns the distance from the end (UD-042). Implemented with the
/// default (`==`, index) for the integer, boolean, text and mesh-entity
/// element types and specialised for [`Real`].
pub trait FindElement: PartialEq {
    /// `true` if the stored value `stored` matches the searched `value`.
    fn find_matches(stored: &Self, value: &Self) -> bool {
        stored == value
    }

    /// `true` when upstream's `find` returns `end − it` (the `Array<Real>`
    /// specialisation, UD-042) instead of `it − begin`.
    const UPSTREAM_DISTANCE_FROM_END: bool = false;
}

macro_rules! exact_find_element {
    ($($t:ty),* $(,)?) => { $(impl FindElement for $t {})* };
}

exact_find_element!(
    i8,
    i16,
    i32,
    i64,
    i128,
    isize,
    u8,
    u16,
    u32,
    u64,
    u128,
    usize,
    bool,
    char,
    f32,
    String,
    crate::element::Element,
    crate::element_type::ElementType,
    crate::element_type::GhostType,
    crate::node_flag::NodeFlag,
);

impl FindElement for Real {
    /// `|stored − value| ≤ ε` (`aka_array.cc:38-41`, the operands in
    /// upstream's order).
    fn find_matches(stored: &Self, value: &Self) -> bool {
        (stored - value).abs() <= Real::EPSILON
    }

    const UPSTREAM_DISTANCE_FROM_END: bool = true;
}

impl<T: FindElement> Array<T> {
    /// Upstream `find(elem)` on the scalar values ([`FindElement`]: exact for
    /// every element type, `|a − elem| ≤ ε` for [`Real`]): the index of the
    /// first match, `None` when absent (upstream `-1`).
    ///
    /// The returned index is the corrected value of UD-042 (category B):
    /// upstream's `Array<Real>::find` returns `size − index` (never `0` for
    /// a match, so the first tuple is indistinguishable from "one from the
    /// end"); [`Array::find_upstream`] returns upstream's raw value.
    ///
    /// # Errors
    /// [`Error::DimensionMismatch`] for a non-empty array whose
    /// `nb_component ≠ 1`: upstream's scalar iterator (`begin()`, checking
    /// `nb_component · size == size`, `aka_array_tmpl.hh:832-840`) throws an
    /// `ArrayException` there. An empty array never throws upstream (`0 ==
    /// 0`) and gives `None` whatever its `nb_component`.
    pub fn find(&self, value: &T) -> Result<Option<usize>> {
        let nb_scalars = self.nb_component.checked_mul(self.len);
        if nb_scalars != Some(self.len) {
            return Err(Error::DimensionMismatch {
                context: "Array::find (scalar iterator)",
                expected: 1,
                got: self.nb_component,
            });
        }
        Ok(self.data.iter().position(|v| T::find_matches(v, value)))
    }

    /// Upstream's raw return value of `find(elem)` (compatibility switch of
    /// UD-042 for differential tests): `-1` when absent, else the index
    /// (generic `Array<T>`) or `size − index` (`Array<Real>`, `end − it`).
    ///
    /// # Errors
    /// As [`Array::find`]; an index beyond the [`Idx`](crate::types::Idx)
    /// range.
    pub fn find_upstream(&self, value: &T) -> Result<crate::types::Idx> {
        match self.find(value)? {
            None => Ok(-1),
            Some(i) if T::UPSTREAM_DISTANCE_FROM_END => crate::types::try_idx(self.len - i),
            Some(i) => crate::types::try_idx(i),
        }
    }
}

impl Array<Real> {
    /// Position of the first value within `DBL_EPSILON` (absolute) of
    /// `value`: a thin wrapper of [`Array::find`] (D-061), which already
    /// applies upstream `Array<Real>::find`'s comparison `|a − value| <= ε`
    /// (`aka_array.cc:35-44`) and returns the index (UD-042: upstream
    /// returns the distance from the end, see [`Array::find_upstream`]).
    /// `None` if absent or if the array is non-empty with more than one
    /// component (where [`Array::find`] returns an error: upstream throws).
    #[must_use]
    pub fn find_real(&self, value: Real) -> Option<usize> {
        self.find(&value).ok().flatten()
    }

    /// `true` if every value is finite (upstream `isFinite`).
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.data.iter().all(|v| v.is_finite())
    }

    /// `self += other` entry-wise (upstream `operator+=`).
    pub fn add_assign(&mut self, other: &Array<Real>) -> Result<()> {
        self.check_same_shape(other, "Array::add_assign")?;
        for (a, b) in self.data.iter_mut().zip(other.data.iter()) {
            *a += *b;
        }
        Ok(())
    }

    /// `self -= other` entry-wise (upstream `operator-=`).
    pub fn sub_assign(&mut self, other: &Array<Real>) -> Result<()> {
        self.check_same_shape(other, "Array::sub_assign")?;
        for (a, b) in self.data.iter_mut().zip(other.data.iter()) {
            *a -= *b;
        }
        Ok(())
    }

    /// `self *= alpha` (upstream `operator*=`).
    pub fn scale(&mut self, alpha: Real) {
        for a in self.data.iter_mut() {
            *a *= alpha;
        }
    }

    fn check_same_shape(&self, other: &Array<Real>, context: &'static str) -> Result<()> {
        if self.nb_component != other.nb_component || self.len != other.len {
            return Err(Error::DimensionMismatch {
                context,
                expected: self.len,
                got: other.len,
            });
        }
        Ok(())
    }
}

impl<T> std::ops::Index<(usize, usize)> for Array<T> {
    type Output = T;
    /// `array[(i, j)]`: component `j` of tuple `i` (upstream `operator()(i, j)`).
    #[inline(always)]
    #[track_caller]
    fn index(&self, (i, j): (usize, usize)) -> &T {
        debug_assert!(i < self.len && j < self.nb_component);
        &self.data[i * self.nb_component + j]
    }
}

impl<T> std::ops::IndexMut<(usize, usize)> for Array<T> {
    #[inline(always)]
    #[track_caller]
    fn index_mut(&mut self, (i, j): (usize, usize)) -> &mut T {
        debug_assert!(i < self.len && j < self.nb_component);
        &mut self.data[i * self.nb_component + j]
    }
}

impl<T: fmt::Debug> fmt::Debug for Array<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Array")
            .field("id", &self.id)
            .field("len", &self.len)
            .field("nb_component", &self.nb_component)
            .field("data", &self.data)
            .finish()
    }
}

/// A `'static` slice of `len` zero-sized arrays, built without allocation
/// (a `Vec` of a zero-sized type never allocates, so leaking it is free).
fn zst_slice<'a, T, const N: usize>(len: usize) -> &'a [[T; N]] {
    debug_assert_eq!(std::mem::size_of::<[T; N]>(), 0);
    zst_slice_mut(len)
}

/// Mutable version of [`zst_slice`] (empty arrays cannot alias).
fn zst_slice_mut<'a, T, const N: usize>(len: usize) -> &'a mut [[T; N]] {
    debug_assert_eq!(std::mem::size_of::<[T; N]>(), 0);
    let v: Vec<[T; N]> = std::iter::repeat_with(|| -> [T; N] {
        // Only reached for N == 0; `from_fn` with N == 0 never calls the closure.
        std::array::from_fn(|_| unreachable!("zero-sized tuple"))
    })
    .take(len)
    .collect();
    v.leak()
}

/// Iterator over the tuples of an array or view (handles `nb_component = 0`).
#[derive(Clone, Debug)]
pub struct Tuples<'a, T> {
    data: &'a [T],
    remaining: usize,
    nb_component: usize,
}

impl<'a, T> Tuples<'a, T> {
    fn new(data: &'a [T], len: usize, nb_component: usize) -> Self {
        debug_assert_eq!(data.len(), len * nb_component);
        Self {
            data,
            remaining: len,
            nb_component,
        }
    }
}

impl<'a, T> Iterator for Tuples<'a, T> {
    type Item = &'a [T];

    #[inline]
    fn next(&mut self) -> Option<&'a [T]> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let (head, tail) = self.data.split_at(self.nb_component);
        self.data = tail;
        Some(head)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> DoubleEndedIterator for Tuples<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let (head, tail) = self.data.split_at(self.data.len() - self.nb_component);
        self.data = head;
        Some(tail)
    }
}

impl<T> ExactSizeIterator for Tuples<'_, T> {}
impl<T> std::iter::FusedIterator for Tuples<'_, T> {}

/// Mutable iterator over tuples (handles `nb_component = 0`).
#[derive(Debug)]
pub struct TuplesMut<'a, T> {
    data: &'a mut [T],
    remaining: usize,
    nb_component: usize,
}

impl<'a, T> TuplesMut<'a, T> {
    fn new(data: &'a mut [T], len: usize, nb_component: usize) -> Self {
        debug_assert_eq!(data.len(), len * nb_component);
        Self {
            data,
            remaining: len,
            nb_component,
        }
    }
}

impl<'a, T> Iterator for TuplesMut<'a, T> {
    type Item = &'a mut [T];

    #[inline]
    fn next(&mut self) -> Option<&'a mut [T]> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let data = std::mem::take(&mut self.data);
        let (head, tail) = data.split_at_mut(self.nb_component);
        self.data = tail;
        Some(head)
    }

    #[inline]
    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.remaining, Some(self.remaining))
    }
}

impl<T> DoubleEndedIterator for TuplesMut<'_, T> {
    #[inline]
    fn next_back(&mut self) -> Option<Self::Item> {
        if self.remaining == 0 {
            return None;
        }
        self.remaining -= 1;
        let data = std::mem::take(&mut self.data);
        let split = data.len() - self.nb_component;
        let (head, tail) = data.split_at_mut(split);
        self.data = head;
        Some(tail)
    }
}

impl<T> ExactSizeIterator for TuplesMut<'_, T> {}
impl<T> std::iter::FusedIterator for TuplesMut<'_, T> {}

/// Borrowed, read-only tuple view over foreign or owned memory.
#[derive(Clone, Copy, Debug)]
pub struct ArrayView<'a, T> {
    data: &'a [T],
    len: usize,
    nb_component: usize,
}

impl<'a, T> ArrayView<'a, T> {
    /// View `data` as tuples of `nb_component` values. With
    /// `nb_component == 0`, `data` must be empty and the view has zero tuples
    /// (use [`ArrayView::with_len`]).
    pub fn new(data: &'a [T], nb_component: usize) -> Result<Self> {
        let len = match nb_component {
            0 if data.is_empty() => 0,
            n if n != 0 && data.len().is_multiple_of(n) => data.len() / n,
            _ => {
                return Err(Error::DimensionMismatch {
                    context: "ArrayView::new",
                    expected: nb_component,
                    got: data.len(),
                });
            }
        };
        Ok(Self {
            data,
            len,
            nb_component,
        })
    }

    /// View `data` as exactly `len` tuples (checked product).
    pub fn with_len(data: &'a [T], len: usize, nb_component: usize) -> Result<Self> {
        let n = checked_len(len, nb_component, "ArrayView")?;
        if n != data.len() {
            return Err(Error::DimensionMismatch {
                context: "ArrayView::with_len",
                expected: n,
                got: data.len(),
            });
        }
        Ok(Self {
            data,
            len,
            nb_component,
        })
    }

    /// Number of tuples.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Components per tuple.
    #[inline(always)]
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Flat data.
    #[inline(always)]
    #[must_use]
    pub fn as_slice(&self) -> &'a [T] {
        self.data
    }

    /// Tuple `i` (panics if out of bounds — internal invariant).
    #[inline(always)]
    #[must_use]
    #[track_caller]
    pub fn tuple(&self, i: usize) -> &'a [T] {
        assert!(i < self.len, "ArrayView::tuple: index out of bounds");
        &self.data[i * self.nb_component..(i + 1) * self.nb_component]
    }

    /// Iterator over tuples.
    pub fn tuples(&self) -> Tuples<'a, T> {
        Tuples::new(self.data, self.len, self.nb_component)
    }
}

/// Borrowed, mutable tuple view (cannot change the length).
#[derive(Debug)]
pub struct ArrayViewMut<'a, T> {
    data: &'a mut [T],
    len: usize,
    nb_component: usize,
}

impl<'a, T> ArrayViewMut<'a, T> {
    /// View `data` as tuples of `nb_component` values (see [`ArrayView::new`]).
    pub fn new(data: &'a mut [T], nb_component: usize) -> Result<Self> {
        let len = ArrayView::new(data, nb_component)
            .map_err(|_| Error::DimensionMismatch {
                context: "ArrayViewMut::new",
                expected: nb_component,
                got: data.len(),
            })?
            .len;
        Ok(Self {
            data,
            len,
            nb_component,
        })
    }

    /// View `data` as exactly `len` tuples (checked product).
    pub fn with_len(data: &'a mut [T], len: usize, nb_component: usize) -> Result<Self> {
        ArrayView::with_len(data, len, nb_component)?;
        Ok(Self {
            data,
            len,
            nb_component,
        })
    }

    /// Number of tuples.
    #[inline(always)]
    #[must_use]
    pub fn len(&self) -> usize {
        self.len
    }

    /// `true` if empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len == 0
    }

    /// Components per tuple.
    #[inline(always)]
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Flat data.
    #[inline(always)]
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        self.data
    }

    /// Mutable tuple `i` (panics if out of bounds — internal invariant).
    #[inline(always)]
    #[track_caller]
    pub fn tuple_mut(&mut self, i: usize) -> &mut [T] {
        assert!(i < self.len, "ArrayViewMut::tuple_mut: index out of bounds");
        let n = self.nb_component;
        &mut self.data[i * n..(i + 1) * n]
    }

    /// Mutable iterator over tuples.
    pub fn tuples_mut(&mut self) -> TuplesMut<'_, T> {
        TuplesMut::new(self.data, self.len, self.nb_component)
    }

    /// Reborrow as read-only.
    #[must_use]
    pub fn as_view(&self) -> ArrayView<'_, T> {
        ArrayView {
            data: self.data,
            len: self.len,
            nb_component: self.nb_component,
        }
    }

    /// Reborrow mutably for a shorter lifetime (to pass the view on
    /// several times, e.g. once per element type).
    pub fn reborrow(&mut self) -> ArrayViewMut<'_, T> {
        ArrayViewMut {
            data: self.data,
            len: self.len,
            nb_component: self.nb_component,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn layout_is_tuple_major() {
        let mut a: Array<Real> = Array::zeros(3, 2);
        a[(1, 0)] = 1.0;
        a[(1, 1)] = 2.0;
        a[(2, 1)] = 3.0;
        assert_eq!(a.as_slice(), &[0.0, 0.0, 1.0, 2.0, 0.0, 3.0]);
        assert_eq!(a.tuple(1), &[1.0, 2.0]);
        assert_eq!(a.len(), 3);
        assert_eq!(a.as_fixed::<2>().unwrap()[2], [0.0, 3.0]);
        assert!(a.as_fixed::<3>().is_err());
        assert_eq!(a.tuples().next_back().unwrap(), &[0.0, 3.0]);
        assert_eq!(a.tuples().len(), 3);
    }

    #[test]
    fn find_real_uses_the_upstream_epsilon_comparison() {
        // D-061: `find` applies upstream's comparison, `find_real` is a thin
        // wrapper of it and `find_upstream` returns upstream's raw value.
        let a = Array::from_vec(vec![3.0, 1.0 + f64::EPSILON, 1.0], 1).unwrap();
        // |(1 + eps) - 1| <= eps: index 1 (upstream would return 3 - 1 = 2)
        assert_eq!(a.find_real(1.0), Some(1));
        assert_eq!(a.find(&1.0).unwrap(), Some(1));
        assert_eq!(a.find_upstream(&1.0).unwrap(), 2);
        assert_eq!(a.find_real(1.0 + 4.0 * f64::EPSILON), None);
        assert_eq!(a.find(&(1.0 + 4.0 * f64::EPSILON)).unwrap(), None);
        assert_eq!(a.find_upstream(&(1.0 + 4.0 * f64::EPSILON)).unwrap(), -1);
        // several components: `find` is an error (upstream throws), the
        // wrapper keeps its `None` contract; an empty array never throws
        let multi = Array::<f64>::zeros(2, 2);
        assert!(multi.find(&0.0).is_err());
        assert_eq!(multi.find_real(0.0), None);
        assert_eq!(Array::<f64>::zeros(0, 2).find(&0.0).unwrap(), None);
    }

    #[test]
    fn push_resize_find() {
        let mut a: Array<u32> = Array::with_id(3, "conn");
        a.push_tuple(&[0, 1, 2]);
        a.push_value(7);
        assert_eq!(a.tuple(1), &[7, 7, 7]);
        assert!(a.try_push_tuple(&[1, 2]).is_err());
        a.resize(4, 9);
        assert_eq!(a.len(), 4);
        assert_eq!(a.tuple(3), &[9, 9, 9]);
        assert_eq!(a.find_tuple(&[7, 7, 7]), Some(1));
        assert_eq!(a.find_tuple(&[1, 1, 1]), None);
        assert_eq!(a.id(), "conn");
        let b: Array<u32> = Array::from_vec(vec![5], 1).unwrap();
        assert_eq!(b.find(&5).unwrap(), Some(0));
        assert!(Array::<u32>::from_vec(vec![1, 2, 3], 2).is_err());
        a.truncate(2);
        assert_eq!(a.len(), 2);
        assert_eq!(a.as_slice().len(), 6);
        // by exactly one tuple
        a.truncate(1);
        assert_eq!(a.len(), 1);
        assert_eq!(a.as_slice(), &[0, 1, 2]);
    }

    #[test]
    fn erase_moves_last_tuple() {
        let mut a: Array<u32> = Array::from_vec(vec![0, 0, 1, 1, 2, 2, 3, 3], 2).unwrap();
        a.swap_remove_tuple(1).unwrap();
        assert_eq!(a.as_slice(), &[0, 0, 3, 3, 2, 2]);
        a.swap_remove_tuple(2).unwrap();
        assert_eq!(a.as_slice(), &[0, 0, 3, 3]);
        assert!(a.swap_remove_tuple(5).is_err());
    }

    #[test]
    fn arithmetic_and_shape_checks() {
        let mut a: Array<Real> = Array::filled(2, 2, 1.0);
        let b: Array<Real> = Array::filled(2, 2, 0.5);
        a.add_assign(&b).unwrap();
        a.scale(2.0);
        assert_eq!(a.as_slice(), &[3.0; 4]);
        a.sub_assign(&b).unwrap();
        assert_eq!(a.as_slice(), &[2.5; 4]);
        let c: Array<Real> = Array::filled(2, 1, 0.5);
        assert!(a.add_assign(&c).is_err());
        assert!(a.append(&c).is_err());
        assert!(a.is_finite());
        a[(0, 0)] = Real::NAN;
        assert!(!a.is_finite());
    }

    #[test]
    fn tuple_as_column_major_matrix() {
        // Tuple [a00, a10, a01, a11] is the column-major 2x2 matrix.
        let a: Array<Real> = Array::from_vec(vec![1.0, 2.0, 3.0, 4.0], 4).unwrap();
        let m = a.tuple_matrix::<2, 2>(0).unwrap();
        use crate::tensor::MatrixExt;
        assert_eq!(m.at(0, 0), 1.0);
        assert_eq!(m.at(1, 0), 2.0);
        assert_eq!(m.at(0, 1), 3.0);
        assert_eq!(m.at(1, 1), 4.0);
        assert!(a.tuple_matrix::<3, 1>(0).is_err());
        assert!(a.tuple_matrix::<2, 2>(1).is_err());
    }

    #[test]
    fn views() {
        let mut buf = vec![1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        {
            let mut v = ArrayViewMut::new(&mut buf, 3).unwrap();
            assert_eq!(v.len(), 2);
            v.tuple_mut(1)[0] = 40.0;
            assert_eq!(v.as_view().tuple(1), &[40.0, 5.0, 6.0]);
            assert_eq!(v.tuples_mut().count(), 2);
        }
        let v = ArrayView::new(&buf, 2).unwrap();
        assert_eq!(v.tuples().count(), 3);
        assert!(ArrayView::new(&buf, 4).is_err());
        assert!(ArrayView::new(&buf, 0).is_err());
        assert!(ArrayView::with_len(&buf, 3, 2).is_ok());
        assert!(ArrayView::with_len(&buf, 2, 2).is_err());
        let owned: Array<Real> = Array::from_vec(buf.clone(), 3).unwrap();
        assert_eq!(owned.view().tuple(0), &[1.0, 2.0, 3.0]);
    }

    #[test]
    fn zero_component_arrays() {
        // Upstream point specialisations allocate 0-component blocks.
        let mut a: Array<Real> = Array::zeros(5, 0);
        assert_eq!(a.len(), 5);
        assert_eq!(a.nb_component(), 0);
        assert!(a.as_slice().is_empty());
        assert_eq!(a.tuple(4), &[] as &[Real]);
        assert_eq!(a.tuples().count(), 5);
        assert_eq!(a.tuples_mut().count(), 5);
        assert_eq!(a.tuples().rev().count(), 5);
        assert_eq!(a.as_fixed::<0>().unwrap().len(), 5);
        assert_eq!(a.as_fixed_mut::<0>().unwrap().len(), 5);
        assert!(a.get_tuple(5).is_err());
        assert_eq!(a.tuple_matrix::<0, 3>(2).unwrap().len(), 3);
        assert_eq!(a.tuple_matrix::<2, 0>(2).unwrap().len(), 0);
        a.push_tuple(&[]);
        a.push_value(1.0);
        assert_eq!(a.len(), 7);
        a.resize(2, 0.0);
        assert_eq!(a.len(), 2);
        a.swap_remove_tuple(0).unwrap();
        assert_eq!(a.len(), 1);
        let b: Array<Real> = Array::zeros(3, 0);
        a.append(&b).unwrap();
        assert_eq!(a.len(), 4);
        a.copy_from(&b).unwrap();
        assert_eq!(a.len(), 3);
        a.fill_tuples(&[]).unwrap();
        assert_eq!(a.capacity(), usize::MAX);
        assert!(Array::<Real>::from_vec(vec![1.0], 0).is_err());
        assert_eq!(Array::<Real>::from_vec(vec![], 0).unwrap().len(), 0);
        assert_eq!(
            Array::<Real>::try_from_vec_with_len(vec![], 9, 0)
                .unwrap()
                .len(),
            9
        );
        assert!(Array::<Real>::try_from_vec_with_len(vec![], usize::MAX, 0).is_ok());
        let v = ArrayView::<Real>::with_len(&[], 4, 0).unwrap();
        assert_eq!(v.tuples().count(), 4);
        assert_eq!(Array::<Real>::new(0).len(), 0);
        a.clear();
        assert!(a.is_empty());
    }

    #[test]
    fn copy_from_in_place_keeps_storage() {
        let mut a: Array<Real> = Array::filled(4, 2, 1.0);
        let b: Array<Real> = Array::filled(4, 2, 2.0);
        let p = a.as_slice().as_ptr();
        a.copy_from(&b).unwrap();
        assert_eq!(a.as_slice().as_ptr(), p);
        assert_eq!(a.as_slice(), &[2.0; 8]);
        let c: Array<Real> = Array::filled(1, 2, 3.0);
        a.copy_from(&c).unwrap();
        assert_eq!(a.len(), 1);
    }

    /// Sizes that a release build must reject, plus sizes whose byte
    /// count exceeds `isize::MAX` (rejected without attempting an allocation).
    #[test]
    fn extreme_sizes_are_rejected_before_state_changes() {
        let n = usize::MAX / 2 + 1;
        assert!(matches!(
            Array::<f64>::try_zeros(n, 2),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(Array::<f64>::try_zeros(usize::MAX / 8, 1).is_err());
        assert!(Array::<u8>::try_filled(usize::MAX, 1, 0).is_err());
        assert!(std::panic::catch_unwind(|| Array::<f64>::zeros(n, 2)).is_err());

        let mut a: Array<f64> = Array::filled(3, 2, 1.0);
        let before = a.clone();
        assert!(a.try_resize(n, 0.0).is_err());
        assert!(a.try_resize(usize::MAX / 16, 0.0).is_err());
        assert!(a.try_reserve(n).is_err());
        assert!(a.try_reserve(usize::MAX / 16).is_err());
        assert_eq!(a, before);
        assert!(Array::<f64>::try_from_vec_with_len(vec![], n, 2).is_err());
        assert!(ArrayView::<f64>::with_len(&[], n, 2).is_err());
        let mut aa = a.clone();
        assert!(std::panic::catch_unwind(move || aa.resize(n, 0.0)).is_err());
    }
}
