// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_csr.hh (CSR<T>, DataCSR<T>).

//! Compressed sparse rows with a variable number of entries per row
//! (upstream `CSR<T>` / `DataCSR<T>`, CORE-029).
//!
//! The container keeps upstream's **two-pass build protocol**, which defines
//! the order of the entries inside a row (= insertion order):
//!
//! 1. count: [`Csr::count_in_row`] (upstream `++csr.rowOffset(i)`) once per
//!    future entry, the counts are stored in `offsets[row]`;
//! 2. [`Csr::count_to_csr`] turns the counts into row starts (exclusive
//!    prefix sum, `offsets[nb_rows]` = total);
//! 3. [`Csr::resize_cols`] allocates the values;
//! 4. [`Csr::begin_insertions`] (no-op, kept for API parity), then
//!    [`Csr::insert_in_row`] — each insertion writes at `offsets[row]` and
//!    advances it (afterwards `offsets[row]` is the start of row `row + 1`);
//! 5. [`Csr::end_insertions`] shifts the offsets back by one row.
//!
//! Rows are read as slices ([`Csr::row`]); upstream's forward and reverse
//! row iterators are `row(i).iter()` and `row(i).iter().rev()`.
//!
//! Differences from upstream (all on misuse only, which upstream leaves
//! undefined in release builds): out-of-range rows and insertions beyond the
//! counted capacity return [`Error::OutOfRange`] instead of writing out of
//! bounds; offsets are `usize` (memory positions) instead of `Idx`.
//!
//! Specialised node → element maps built in one pass already exist in
//! `akantu-mesh` (`NodeToElements`, `NodeToElementIndices`, `PartCsr`); they
//! produce the same layout as this container filled with the two-pass
//! protocol in upstream's visiting order.

use crate::error::{Error, Result};
use crate::types::Idx;

/// Compressed rows of `T` (upstream `CSR<T>`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Csr<T> {
    /// `nb_rows + 1` offsets (counts before [`Csr::count_to_csr`], running
    /// insertion positions between [`Csr::begin_insertions`] and
    /// [`Csr::end_insertions`]).
    offsets: Vec<usize>,
    /// Values of all rows.
    values: Vec<T>,
}

fn row_out_of_range(row: usize, len: usize) -> Error {
    Error::OutOfRange {
        what: "CSR row",
        index: row,
        len,
    }
}

impl<T> Csr<T> {
    /// Empty container with `nb_rows` rows, all counts zero (upstream
    /// `CSR(nb_rows)`).
    #[must_use]
    pub fn new(nb_rows: usize) -> Self {
        Self {
            offsets: vec![0; nb_rows + 1],
            values: Vec::new(),
        }
    }

    /// Number of rows (upstream `getNbRows`).
    #[must_use]
    pub fn nb_rows(&self) -> usize {
        self.offsets.len().saturating_sub(1)
    }

    /// Number of entries of `row` (upstream `getNbCols`); valid once the
    /// offsets are row starts (after [`Csr::count_to_csr`] or
    /// [`Csr::end_insertions`]).
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `row >= nb_rows`.
    pub fn nb_cols(&self, row: usize) -> Result<usize> {
        let (a, b) = self.bounds(row)?;
        Ok(b.saturating_sub(a))
    }

    fn bounds(&self, row: usize) -> Result<(usize, usize)> {
        match (self.offsets.get(row), self.offsets.get(row + 1)) {
            (Some(&a), Some(&b)) => Ok((a, b)),
            _ => Err(row_out_of_range(row, self.nb_rows())),
        }
    }

    /// Mutable access to `offsets[row]` (upstream `rowOffset(row)`), used by
    /// the counting pass.
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `row >= nb_rows`.
    pub fn row_offset_mut(&mut self, row: usize) -> Result<&mut usize> {
        let n = self.nb_rows();
        if row >= n {
            return Err(row_out_of_range(row, n));
        }
        Ok(&mut self.offsets[row])
    }

    /// Count one future entry of `row` (upstream `++csr.rowOffset(row)`).
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `row >= nb_rows`.
    pub fn count_in_row(&mut self, row: usize) -> Result<()> {
        *self.row_offset_mut(row)? += 1;
        Ok(())
    }

    /// Turn the per-row counts into row starts (upstream `countToCSR`):
    /// `offsets[i] = Σ_{k<i} count[k]`, `offsets[nb_rows]` = total. As
    /// upstream, a value stored in `offsets[nb_rows]` before the call is
    /// discarded.
    pub fn count_to_csr(&mut self) {
        let n = self.nb_rows();
        for i in 1..n {
            self.offsets[i] += self.offsets[i - 1];
        }
        for i in (1..=n).rev() {
            self.offsets[i] = self.offsets[i - 1];
        }
        self.offsets[0] = 0;
    }

    /// Start of the insertion pass (upstream `beginInsertions`, a no-op).
    pub fn begin_insertions(&mut self) {}

    /// Write `value` at the current position of `row` and advance it
    /// (upstream `insertInRow`); returns the position written.
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `row >= nb_rows` or the position is beyond
    /// the allocated values (more insertions than counted, or
    /// [`Csr::resize_cols`] not called).
    pub fn insert_in_row(&mut self, row: usize, value: T) -> Result<usize> {
        let n = self.nb_rows();
        if row >= n {
            return Err(row_out_of_range(row, n));
        }
        let pos = self.offsets[row];
        let len = self.values.len();
        let slot = self.values.get_mut(pos).ok_or(Error::OutOfRange {
            what: "CSR insertion position",
            index: pos,
            len,
        })?;
        *slot = value;
        self.offsets[row] += 1;
        Ok(pos)
    }

    /// End of the insertion pass (upstream `endInsertions`): every offset
    /// moves one row up, `offsets[0] = 0`.
    pub fn end_insertions(&mut self) {
        let n = self.nb_rows();
        for i in (1..=n).rev() {
            self.offsets[i] = self.offsets[i - 1];
        }
        self.offsets[0] = 0;
    }

    /// Zero every offset and drop the values (upstream `clearRows`).
    pub fn clear_rows(&mut self) {
        self.offsets.fill(0);
        self.values.clear();
    }

    /// Set the number of rows and zero every offset (upstream
    /// `resizeRows`); the values are kept, as upstream.
    pub fn resize_rows(&mut self, nb_rows: usize) {
        self.offsets.clear();
        self.offsets.resize(nb_rows + 1, 0);
    }

    /// Entries of `row` (upstream `getRow`, `begin(row)..end(row)`).
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if `row >= nb_rows` or the offsets do not
    /// describe a valid range of the values (container in the middle of a
    /// build pass).
    pub fn row(&self, row: usize) -> Result<&[T]> {
        let (a, b) = self.bounds(row)?;
        let len = self.values.len();
        self.values.get(a..b).ok_or(Error::OutOfRange {
            what: "CSR row range",
            index: b,
            len,
        })
    }

    /// Mutable entries of `row`.
    ///
    /// # Errors
    /// As [`Csr::row`].
    pub fn row_mut(&mut self, row: usize) -> Result<&mut [T]> {
        let (a, b) = self.bounds(row)?;
        let len = self.values.len();
        self.values.get_mut(a..b).ok_or(Error::OutOfRange {
            what: "CSR row range",
            index: b,
            len,
        })
    }

    /// Entry `col` of `row` (upstream `operator()(row, col)`).
    ///
    /// # Errors
    /// [`Error::OutOfRange`] if the entry does not exist.
    pub fn get(&self, row: usize, col: usize) -> Result<&T> {
        let r = self.row(row)?;
        let len = r.len();
        r.get(col).ok_or(Error::OutOfRange {
            what: "CSR column",
            index: col,
            len,
        })
    }

    /// Iterator over the rows (as slices). Meaningful when the offsets are
    /// row starts: after [`Csr::end_insertions`] (or [`Csr::from_rows`]).
    /// Between [`Csr::count_to_csr`]/[`Csr::begin_insertions`] and
    /// [`Csr::end_insertions`] the offsets are running insertion positions
    /// (upstream reads them as row bounds anyway, giving shifted or
    /// overlapping ranges); a row whose range is then invalid is yielded
    /// as `&[]` here, never as another row's values. Use [`Csr::row`] to
    /// get that case as an error.
    pub fn rows(&self) -> impl ExactSizeIterator<Item = &[T]> + '_ {
        (0..self.nb_rows()).map(|r| self.row(r).unwrap_or(&[]))
    }

    /// The offsets (upstream `getRowsOffset`, `nb_rows + 1` entries).
    #[must_use]
    pub fn offsets(&self) -> &[usize] {
        &self.offsets
    }

    /// The values of all rows (upstream `getRows`).
    #[must_use]
    pub fn values(&self) -> &[T] {
        &self.values
    }

    /// Mutable values of all rows (upstream non-const `getRows`).
    pub fn values_mut(&mut self) -> &mut [T] {
        &mut self.values
    }

    /// Build from complete rows in one pass (same result as the two-pass
    /// protocol with insertions in row order).
    pub fn from_rows<R, I>(rows: R) -> Self
    where
        R: IntoIterator<Item = I>,
        I: IntoIterator<Item = T>,
    {
        let mut offsets = vec![0];
        let mut values = Vec::new();
        for r in rows {
            values.extend(r);
            offsets.push(values.len());
        }
        Self { offsets, values }
    }

    /// Decompose into `(offsets, values)` (upstream `copy(offsets, values)`
    /// without the copy).
    #[must_use]
    pub fn into_parts(self) -> (Vec<usize>, Vec<T>) {
        (self.offsets, self.values)
    }
}

impl<T: Clone + Default> Csr<T> {
    /// Allocate `offsets[nb_rows]` values (upstream `resizeCols`); new
    /// values are `T::default()` (upstream: zero for POD types).
    pub fn resize_cols(&mut self) {
        let total = self.offsets.last().copied().unwrap_or(0);
        self.values.resize(total, T::default());
    }
}

impl<T: Clone> Csr<T> {
    /// Copies of the offsets and values (upstream `copy(offsets, values)`).
    #[must_use]
    pub fn to_parts(&self) -> (Vec<usize>, Vec<T>) {
        (self.offsets.clone(), self.values.clone())
    }
}

/// [`Csr<Idx>`] structure with a parallel value array (upstream
/// `DataCSR<T>`, e.g. column indices + matrix values). Unused upstream at
/// the reference commit; kept for API parity.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct DataCsr<T> {
    structure: Csr<Idx>,
    data: Vec<T>,
}

impl<T: Clone + Default> DataCsr<T> {
    /// Empty structure with `nb_rows` rows (upstream `DataCSR(nb_rows)`).
    #[must_use]
    pub fn new(nb_rows: usize) -> Self {
        Self {
            structure: Csr::new(nb_rows),
            data: Vec::new(),
        }
    }

    /// Allocate structure and data to the counted size (upstream
    /// `DataCSR::resizeCols`).
    pub fn resize_cols(&mut self) {
        self.structure.resize_cols();
        let total = self.structure.offsets().last().copied().unwrap_or(0);
        self.data.resize(total, T::default());
    }

    /// The structure (`CSR<Idx>` base class upstream).
    #[must_use]
    pub fn structure(&self) -> &Csr<Idx> {
        &self.structure
    }

    /// Mutable structure (counting and insertion passes).
    pub fn structure_mut(&mut self) -> &mut Csr<Idx> {
        &mut self.structure
    }

    /// The data values (upstream `getData`), parallel to the structure's
    /// values.
    #[must_use]
    pub fn data(&self) -> &[T] {
        &self.data
    }

    /// Mutable data values.
    pub fn data_mut(&mut self) -> &mut [T] {
        &mut self.data
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn build(counts: &[usize]) -> Csr<Idx> {
        let mut csr = Csr::new(counts.len());
        for (r, &c) in counts.iter().enumerate() {
            for _ in 0..c {
                csr.count_in_row(r).unwrap();
            }
        }
        csr.count_to_csr();
        csr.resize_cols();
        csr.begin_insertions();
        for (r, &c) in counts.iter().enumerate() {
            for j in 0..c {
                csr.insert_in_row(r, Idx::try_from(c - j).unwrap()).unwrap();
            }
        }
        csr.end_insertions();
        csr
    }

    #[test]
    fn two_pass_protocol_matches_upstream() {
        let csr = build(&[2, 0, 3, 1]);
        assert_eq!(csr.offsets(), &[0, 2, 2, 5, 6]);
        assert_eq!(csr.values(), &[2, 1, 3, 2, 1, 1]);
        assert_eq!(csr.row(1).unwrap(), &[] as &[Idx]);
        assert_eq!(csr.nb_cols(2).unwrap(), 3);
        assert_eq!(*csr.get(2, 1).unwrap(), 2);
        assert!(csr.get(1, 0).is_err());
        assert_eq!(
            csr.rows().map(<[Idx]>::len).collect::<Vec<_>>(),
            vec![2, 0, 3, 1]
        );
        assert_eq!(
            Csr::from_rows([vec![2, 1], vec![], vec![3, 2, 1], vec![1]]),
            csr
        );
    }

    #[test]
    fn count_to_csr_offsets_and_positions() {
        let mut csr: Csr<u8> = Csr::new(3);
        *csr.row_offset_mut(0).unwrap() = 1;
        *csr.row_offset_mut(2).unwrap() = 2;
        csr.count_to_csr();
        assert_eq!(csr.offsets(), &[0, 1, 1, 3]);
        csr.resize_cols();
        assert_eq!(csr.insert_in_row(2, 7).unwrap(), 1);
        assert_eq!(csr.insert_in_row(0, 5).unwrap(), 0);
        assert_eq!(csr.insert_in_row(2, 8).unwrap(), 2);
        // one insertion too many: refused, nothing written
        assert!(csr.insert_in_row(2, 9).is_err());
        csr.end_insertions();
        assert_eq!(csr.values(), &[5, 7, 8]);
        assert_eq!(csr.offsets(), &[0, 1, 1, 3]);
    }

    #[test]
    fn empty_and_misuse() {
        let mut csr: Csr<i32> = Csr::new(0);
        csr.count_to_csr();
        csr.end_insertions();
        assert_eq!((csr.nb_rows(), csr.offsets()), (0, &[0usize][..]));
        assert!(csr.count_in_row(0).is_err());
        assert!(csr.insert_in_row(0, 1).is_err());
        let mut c = build(&[1, 2]);
        c.clear_rows();
        assert_eq!((c.nb_rows(), c.values().len()), (2, 0));
        c.resize_rows(4);
        assert_eq!(c.offsets(), &[0, 0, 0, 0, 0]);
        // not resized: insertion refused
        let mut d: Csr<i32> = Csr::new(1);
        d.count_in_row(0).unwrap();
        d.count_to_csr();
        assert!(d.insert_in_row(0, 3).is_err());
    }

    #[test]
    fn data_csr_allocates_both_arrays() {
        let mut d: DataCsr<f64> = DataCsr::new(2);
        d.structure_mut().count_in_row(1).unwrap();
        d.structure_mut().count_in_row(1).unwrap();
        d.structure_mut().count_to_csr();
        d.resize_cols();
        assert_eq!((d.structure().values().len(), d.data().len()), (2, 2));
        d.data_mut()[1] = 2.5;
        assert_eq!(d.data(), &[0.0, 2.5]);
    }
}
