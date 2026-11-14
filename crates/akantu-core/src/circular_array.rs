// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_circular_array.hh,
// src/common/aka_circular_array_inline_impl.hh (CircularArray<T>).

//! Ring buffer of tuples (upstream `CircularArray<T>`, CORE-037).
//!
//! A fixed number of tuples whose logical start moves by one tuple at each
//! [`CircularArray::make_step`]: after a step the former first tuple is the
//! last one. Logical tuple `i` is stored at `(i + start) mod size`.
//! Unused anywhere in upstream `src/`, `test/` and `python/` at the
//! reference commit; ported because it is tiny and has a clear contract.
//! `printself` is [`CircularArray::print_self`] (in `array_print`, next to
//! `Array::print_self`, whose body it reuses).
//!
//! Deviations (misuse only): upstream divides by the allocated size, which
//! is `0` for an empty array (undefined behaviour) — construction with
//! `size = 0` is refused here; out-of-range accesses are errors (upstream
//! debug assertions). Upstream's debug assertion also rejects every access
//! to a one-tuple array (it tests `end_position != start_position`, true
//! only for `size > 1`); a one-tuple ring is accepted here (release
//! behaviour of upstream).

use crate::error::{Error, Result};
use crate::types::Id;

/// Ring buffer of `size` tuples of `nb_component` values.
#[derive(Clone, Debug, PartialEq)]
pub struct CircularArray<T> {
    values: Vec<T>,
    size: usize,
    nb_component: usize,
    start: usize,
    end: usize,
    id: Id,
}

impl<T: Clone> CircularArray<T> {
    /// `size` tuples filled with `value` (upstream
    /// `CircularArray(size, nb_component, value, id)`).
    ///
    /// # Errors
    /// [`Error::InvalidArgument`] if `size == 0`; [`Error::CapacityExceeded`]
    /// if `size · nb_component` overflows.
    pub fn new(size: usize, nb_component: usize, value: T, id: impl Into<Id>) -> Result<Self> {
        if size == 0 {
            return Err(Error::invalid("a circular array needs at least one tuple"));
        }
        let n = crate::types::checked_len(size, nb_component, "circular array")?;
        Ok(Self {
            values: vec![value; n],
            size,
            nb_component,
            start: 0,
            end: size - 1,
            id: id.into(),
        })
    }
}

impl<T> CircularArray<T> {
    /// Advance the start and end by one tuple (upstream `makeStep`).
    pub fn make_step(&mut self) {
        self.start = (self.start + 1) % self.size;
        self.end = (self.end + 1) % self.size;
    }

    /// Number of tuples (upstream `size`).
    #[must_use]
    pub fn len(&self) -> usize {
        self.size
    }

    /// Always `false` (construction requires at least one tuple).
    #[must_use]
    pub fn is_empty(&self) -> bool {
        false
    }

    /// Components per tuple.
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Physical index of the first logical tuple (upstream
    /// `start_position`).
    #[must_use]
    pub fn start_position(&self) -> usize {
        self.start
    }

    /// Physical index of the last logical tuple (upstream `end_position`).
    #[must_use]
    pub fn end_position(&self) -> usize {
        self.end
    }

    fn offset(&self, i: usize, j: usize) -> Result<usize> {
        if i >= self.size {
            return Err(Error::OutOfRange {
                what: "circular array tuple",
                index: i,
                len: self.size,
            });
        }
        if j >= self.nb_component {
            return Err(Error::OutOfRange {
                what: "circular array component",
                index: j,
                len: self.nb_component,
            });
        }
        Ok(((i + self.start) % self.size) * self.nb_component + j)
    }

    /// Component `j` of logical tuple `i` (upstream `operator()(i, j)`).
    ///
    /// # Errors
    /// [`Error::OutOfRange`] for `i >= len()` or `j >= nb_component()`.
    pub fn get(&self, i: usize, j: usize) -> Result<&T> {
        let o = self.offset(i, j)?;
        Ok(&self.values[o])
    }

    /// Mutable component `j` of logical tuple `i`.
    ///
    /// # Errors
    /// As [`CircularArray::get`].
    pub fn get_mut(&mut self, i: usize, j: usize) -> Result<&mut T> {
        let o = self.offset(i, j)?;
        Ok(&mut self.values[o])
    }

    /// Physical storage (tuple-major, independent of the steps made).
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn steps_rotate_the_logical_order() {
        let mut c = CircularArray::new(3, 2, 0, "c").unwrap();
        for i in 0..3 {
            for j in 0..2 {
                *c.get_mut(i, j).unwrap() = 10 * i + j;
            }
        }
        assert_eq!((c.start_position(), c.end_position()), (0, 2));
        c.make_step();
        assert_eq!((c.start_position(), c.end_position()), (1, 0));
        // the former first tuple is now the last one
        assert_eq!(*c.get(0, 0).unwrap(), 10);
        assert_eq!(*c.get(2, 1).unwrap(), 1);
        c.make_step();
        c.make_step();
        assert_eq!(*c.get(0, 1).unwrap(), 1);
        assert!(c.get(3, 0).is_err() && c.get(0, 2).is_err());
        assert_eq!(c.as_slice(), &[0, 1, 10, 11, 20, 21]);
        assert_eq!((c.len(), c.nb_component(), c.id()), (3, 2, "c"));
    }

    #[test]
    fn degenerate_sizes() {
        assert!(CircularArray::new(0, 1, 0.0, "").is_err());
        let mut one = CircularArray::new(1, 1, 5.0, "").unwrap();
        one.make_step();
        assert_eq!(*one.get(0, 0).unwrap(), 5.0);
        assert!(CircularArray::new(usize::MAX, 2, 0u8, "").is_err());
    }
}
