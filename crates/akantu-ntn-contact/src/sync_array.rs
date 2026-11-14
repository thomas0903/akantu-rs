// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// src/common/{synchronized_array.hh, synchronized_array.cc,
// synchronized_array_inline_impl.hh}.

//! Per-contact-node arrays kept in step with the contact node list
//! (upstream `SynchronizedArray<T>`, TASN-038).
//!
//! Upstream links the arrays of a contact and of its friction into a tree of
//! raw pointers (`registerDependingArray`): a `push_back`/`erase` on the root
//! (`slaves`) followed by `syncElements(_added/_deleted)` appends default
//! values to / replays the same erasures on every dependent array. Here the
//! owner of the arrays (contact or friction) propagates explicitly:
//! [`SynchronizedArray::sync_elements`] replays the pending change of a root
//! onto the dependents it is given ([`SyncDependent`]), and the contact keeps
//! a [`SyncJournal`] of the structural changes so that a friction object
//! created later, or held separately, catches up
//! ([`SyncJournal::replay_since`]) exactly as a registered dependent would
//! have been updated. This is a local mechanism, distinct from the MPI
//! exchanges of the contact (`_cf_nodal`, `_cf_incr`).
//!
//! Restart files (`dumpRestartFile` / `readRestartFile`) keep the upstream
//! text format: `<file>-<restart_name>.rs` with the line `size nb_component`
//! then all values separated by single spaces (C++ `setprecision(12)` default
//! float format, booleans as `0`/`1`), with a trailing space.
//!
//! **UD-394 (category A/B):** upstream `readRestartFile` reads every value
//! into a *copy* (`for (auto v : make_view(...)) data >> v;`), so a restart
//! read never restores any value, and it overwrites the array size and
//! component count with the file's without reallocating (out-of-bounds when
//! they differ). The port restores the values and refuses a file whose size
//! or component count differs from the array; the compat switch
//! [`crate::TasnUpstreamCompat::restart_read_ignores_values`] reproduces the
//! no-op read (sizes still checked).

use std::fmt;
use std::fs;
use std::path::{Path, PathBuf};

use akantu_core::{Error, Idx, Real, Result};
use akantu_parser::stream;

/// Upstream `SyncChoice`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncChoice {
    /// `_added`.
    Added,
    /// `_deleted`.
    Deleted,
}

/// Value types of the synchronized arrays (upstream instantiates `Real`,
/// `UInt`, `Int`, `bool`) with their restart text codec.
pub trait SyncValue: Copy + PartialEq + Default + fmt::Debug + 'static {
    /// `out << std::setprecision(12) << v` (C++ default float format).
    fn write_restart(self, out: &mut String);
    /// C++ `operator>>` from byte `start`: the value and the number of bytes
    /// consumed, `None` when the extraction fails.
    fn read_restart(text: &[u8], start: usize) -> (Option<Self>, usize);
}

impl SyncValue for Real {
    fn write_restart(self, out: &mut String) {
        out.push_str(&stream::format_general(self, 12));
    }
    fn read_restart(text: &[u8], start: usize) -> (Option<Self>, usize) {
        let r = stream::extract_double(text, start);
        ((!r.fail).then_some(r.value).flatten(), r.consumed)
    }
}

impl SyncValue for Idx {
    fn write_restart(self, out: &mut String) {
        out.push_str(&self.to_string());
    }
    fn read_restart(text: &[u8], start: usize) -> (Option<Self>, usize) {
        let r = stream::extract_int::<Idx>(text, start);
        ((!r.fail).then_some(r.value).flatten(), r.consumed)
    }
}

impl SyncValue for u32 {
    fn write_restart(self, out: &mut String) {
        out.push_str(&self.to_string());
    }
    fn read_restart(text: &[u8], start: usize) -> (Option<Self>, usize) {
        let r = stream::extract_int::<u32>(text, start);
        ((!r.fail).then_some(r.value).flatten(), r.consumed)
    }
}

impl SyncValue for bool {
    fn write_restart(self, out: &mut String) {
        out.push(if self { '1' } else { '0' });
    }
    fn read_restart(text: &[u8], start: usize) -> (Option<Self>, usize) {
        let r = stream::extract_bool(text, start, false);
        ((!r.fail).then_some(r.value).flatten(), r.consumed)
    }
}

/// A structural change of a root array, as replayed on its dependents.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum SyncRecord {
    /// `n` elements appended (dependents append their default value).
    Added(usize),
    /// Elements erased, in erasure order (each erasure moves the last element
    /// into the erased slot, as upstream `SynchronizedArray::erase`).
    Deleted(Vec<usize>),
}

/// The dependent side of the synchronisation (upstream
/// `SynchronizedArrayBase`), object safe.
pub trait SyncDependent {
    /// Identifier (upstream `getID`).
    fn sync_id(&self) -> &str;
    /// Upstream `syncAddedElements(n)`: append `n` default values; returns
    /// the new size.
    ///
    /// # Errors
    /// The array has unsynchronised changes of its own.
    fn sync_added_elements(&mut self, nb_added: usize) -> Result<usize>;
    /// Upstream `syncDeletedElements(list)`: replay the erasures; returns the
    /// new size.
    ///
    /// # Errors
    /// The array has unsynchronised changes of its own, or an index is out of
    /// range.
    fn sync_deleted_elements(&mut self, deleted: &[usize]) -> Result<usize>;
}

/// Upstream `SynchronizedArray<T>`: an array of `len × nb_component` values
/// with a default value for appended elements, a restart name and the list of
/// unsynchronised changes.
#[derive(Clone, Debug)]
pub struct SynchronizedArray<T: SyncValue> {
    data: Vec<T>,
    nb_component: usize,
    default_value: T,
    id: String,
    restart_name: String,
    nb_added_elements: usize,
    deleted_elements: Vec<usize>,
}

impl<T: SyncValue> SynchronizedArray<T> {
    /// Upstream `SynchronizedArray(size, nb_component, value, id,
    /// default_value, restart_name)`: `size` elements of `value` (counted as
    /// added and not yet synchronised, as upstream's `nb_added_elements(size)`).
    ///
    /// # Errors
    /// `nb_component == 0`.
    pub fn with_size(
        size: usize,
        nb_component: usize,
        value: T,
        id: impl Into<String>,
        default_value: T,
        restart_name: impl Into<String>,
    ) -> Result<Self> {
        if nb_component == 0 {
            return Err(Error::invalid(
                "SynchronizedArray: the number of components must be positive",
            ));
        }
        Ok(Self {
            data: vec![value; size * nb_component],
            nb_component,
            default_value,
            id: id.into(),
            restart_name: restart_name.into(),
            nb_added_elements: size,
            deleted_elements: Vec::new(),
        })
    }

    /// Empty array (the only form the package constructs:
    /// `SynchronizedArray(0, nb_component, value, id, default, restart_name)`).
    ///
    /// # Errors
    /// `nb_component == 0`.
    pub fn new(
        nb_component: usize,
        id: impl Into<String>,
        default_value: T,
        restart_name: impl Into<String>,
    ) -> Result<Self> {
        Self::with_size(
            0,
            nb_component,
            default_value,
            id,
            default_value,
            restart_name,
        )
    }

    /// Number of elements (tuples).
    #[must_use]
    pub fn len(&self) -> usize {
        self.data.len() / self.nb_component
    }

    /// No element.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.data.is_empty()
    }

    /// Components per element.
    #[must_use]
    pub fn nb_component(&self) -> usize {
        self.nb_component
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Restart name (file suffix).
    #[must_use]
    pub fn restart_name(&self) -> &str {
        &self.restart_name
    }

    /// Default value of appended elements (upstream `operator T()`, which is
    /// what `Parsable::get` of a per-node parameter returns).
    #[must_use]
    pub fn default_value(&self) -> T {
        self.default_value
    }

    /// Upstream `setDefaultValue`.
    pub fn set_default_value(&mut self, value: T) {
        self.default_value = value;
    }

    /// All values, element-major.
    #[must_use]
    pub fn as_slice(&self) -> &[T] {
        &self.data
    }

    /// All values, mutable.
    pub fn as_mut_slice(&mut self) -> &mut [T] {
        &mut self.data
    }

    /// Element `i` (all components).
    ///
    /// # Panics
    /// `i >= len()` (internal invariant of the owners).
    #[must_use]
    pub fn tuple(&self, i: usize) -> &[T] {
        &self.data[i * self.nb_component..(i + 1) * self.nb_component]
    }

    /// Element `i`, mutable.
    ///
    /// # Panics
    /// `i >= len()` (internal invariant of the owners).
    pub fn tuple_mut(&mut self, i: usize) -> &mut [T] {
        let n = self.nb_component;
        &mut self.data[i * n..(i + 1) * n]
    }

    /// Element `i`, checked.
    ///
    /// # Errors
    /// `i >= len()`.
    pub fn get_tuple(&self, i: usize) -> Result<&[T]> {
        let len = self.len();
        if i < len {
            Ok(self.tuple(i))
        } else {
            Err(Error::OutOfRange {
                what: "synchronized array element",
                index: i,
                len,
            })
        }
    }

    /// First component of element `i` (upstream `operator()(i)`).
    ///
    /// # Panics
    /// `i >= len()` (internal invariant of the owners).
    #[must_use]
    pub fn at(&self, i: usize) -> T {
        self.data[i * self.nb_component]
    }

    /// Set the first component of element `i`.
    ///
    /// # Panics
    /// `i >= len()` (internal invariant of the owners).
    pub fn set_at(&mut self, i: usize, v: T) {
        self.data[i * self.nb_component] = v;
    }

    /// Upstream `push_back(value)`: one element whose components all equal
    /// `value` (Akantu `Array::push_back(const T &)`).
    ///
    /// # Errors
    /// Erasures are pending (upstream debug assertion).
    pub fn push_value(&mut self, value: T) -> Result<()> {
        self.check_no_deletion("push_back")?;
        for _ in 0..self.nb_component {
            self.data.push(value);
        }
        self.nb_added_elements += 1;
        Ok(())
    }

    /// Upstream `push_back(vector)`.
    ///
    /// # Errors
    /// Erasures are pending, or `tuple` has the wrong length.
    pub fn push_tuple(&mut self, tuple: &[T]) -> Result<()> {
        self.check_no_deletion("push_back")?;
        if tuple.len() != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "SynchronizedArray::push_back",
                expected: self.nb_component,
                got: tuple.len(),
            });
        }
        self.data.extend_from_slice(tuple);
        self.nb_added_elements += 1;
        Ok(())
    }

    /// Upstream `erase(i)`: the last element is copied into slot `i`, the
    /// array shrinks by one, `i` is recorded for the dependents.
    ///
    /// # Errors
    /// Additions are pending (upstream debug assertion), or `i` is out of
    /// range.
    pub fn erase(&mut self, i: usize) -> Result<()> {
        if self.nb_added_elements != 0 {
            return Err(Error::invalid(format!(
                "Cannot erase element if SynchronizedArray {} is already modified without synchronization",
                self.id
            )));
        }
        let len = self.len();
        if i >= len {
            return Err(Error::OutOfRange {
                what: "synchronized array element",
                index: i,
                len,
            });
        }
        let n = self.nb_component;
        for j in 0..n {
            self.data[i * n + j] = self.data[(len - 1) * n + j];
        }
        self.data.truncate((len - 1) * n);
        self.deleted_elements.push(i);
        Ok(())
    }

    /// Upstream `set(t)`: every value set to `t`.
    pub fn set(&mut self, t: T) {
        self.data.fill(t);
    }

    /// Upstream `set(vector)` on a multi-component array: every element set
    /// to `tuple`.
    ///
    /// # Errors
    /// `tuple` has the wrong length.
    pub fn set_tuples(&mut self, tuple: &[T]) -> Result<()> {
        if tuple.len() != self.nb_component {
            return Err(Error::DimensionMismatch {
                context: "SynchronizedArray::set",
                expected: self.nb_component,
                got: tuple.len(),
            });
        }
        for chunk in self.data.chunks_exact_mut(self.nb_component) {
            chunk.copy_from_slice(tuple);
        }
        Ok(())
    }

    /// Upstream `setAndChangeDefault(t)`.
    pub fn set_and_change_default(&mut self, t: T) {
        self.set(t);
        self.default_value = t;
    }

    /// Upstream `zero()`: every value set to `T()` (`0`, `false`).
    pub fn zero(&mut self) {
        self.data.fill(T::default());
    }

    /// Upstream `find(value)` on a one-component array: first index holding
    /// `value`.
    #[must_use]
    pub fn find(&self, value: T) -> Option<usize> {
        self.data
            .chunks_exact(self.nb_component)
            .position(|c| c[0] == value)
    }

    /// Pending (unsynchronised) change of this array, if any.
    #[must_use]
    pub fn pending(&self) -> Option<SyncRecord> {
        if self.nb_added_elements > 0 {
            Some(SyncRecord::Added(self.nb_added_elements))
        } else if !self.deleted_elements.is_empty() {
            Some(SyncRecord::Deleted(self.deleted_elements.clone()))
        } else {
            None
        }
    }

    /// Upstream `syncElements(choice)`: replay this array's pending change of
    /// kind `choice` on every dependent (each must end with this array's
    /// size, upstream debug assertion), then clear it. Returns the replayed
    /// record (`None` if nothing of that kind was pending).
    ///
    /// # Errors
    /// A dependent fails or ends with a different size.
    pub fn sync_elements(
        &mut self,
        choice: SyncChoice,
        dependents: &mut [&mut dyn SyncDependent],
    ) -> Result<Option<SyncRecord>> {
        let record = match choice {
            SyncChoice::Added => {
                (self.nb_added_elements > 0).then_some(SyncRecord::Added(self.nb_added_elements))
            }
            SyncChoice::Deleted => (!self.deleted_elements.is_empty())
                .then(|| SyncRecord::Deleted(self.deleted_elements.clone())),
        };
        let len = self.len();
        for d in dependents.iter_mut() {
            let size = match choice {
                SyncChoice::Added => d.sync_added_elements(self.nb_added_elements)?,
                SyncChoice::Deleted => d.sync_deleted_elements(&self.deleted_elements)?,
            };
            if size != len {
                return Err(Error::invalid(format!(
                    "Synchronized arrays do not have the same length ({} has {size}, {} has {len}; may be a double synchronization)",
                    d.sync_id(),
                    self.id
                )));
            }
        }
        match choice {
            SyncChoice::Added => self.nb_added_elements = 0,
            SyncChoice::Deleted => self.deleted_elements.clear(),
        }
        Ok(record)
    }

    /// Apply a root's record to this array as a dependent.
    ///
    /// # Errors
    /// See [`SyncDependent`].
    pub fn apply_record(&mut self, record: &SyncRecord) -> Result<usize> {
        match record {
            SyncRecord::Added(n) => self.sync_added_elements(*n),
            SyncRecord::Deleted(list) => self.sync_deleted_elements(list),
        }
    }

    fn check_no_deletion(&self, what: &str) -> Result<()> {
        if self.deleted_elements.is_empty() {
            Ok(())
        } else {
            Err(Error::invalid(format!(
                "Cannot {what} element if SynchronizedArray {} is already modified without synchronization",
                self.id
            )))
        }
    }

    fn check_synchronized(&self, what: &str) -> Result<()> {
        if self.nb_added_elements == 0 && self.deleted_elements.is_empty() {
            Ok(())
        } else {
            Err(Error::invalid(format!(
                "Restart File for SynchronizedArray {} should not be {what} as it is not synchronized yet",
                self.id
            )))
        }
    }

    /// Restart file of this array for the prefix `file_name`
    /// (`<file_name>-<restart_name>.rs`).
    #[must_use]
    pub fn restart_path(&self, file_name: &str) -> PathBuf {
        PathBuf::from(format!("{file_name}-{}.rs", self.restart_name))
    }

    /// Restart text (upstream `dumpRestartFile` content).
    ///
    /// # Errors
    /// The array has unsynchronised changes (upstream debug assertion).
    pub fn restart_text(&self) -> Result<String> {
        self.check_synchronized("dumped")?;
        let mut out = format!("{} {}\n", self.len(), self.nb_component);
        for &v in &self.data {
            v.write_restart(&mut out);
            out.push(' ');
        }
        out.push('\n');
        Ok(out)
    }

    /// Upstream `dumpRestartFile(file_name)`.
    ///
    /// # Errors
    /// Unsynchronised changes, or the file cannot be written.
    pub fn dump_restart_file(&self, file_name: &str) -> Result<()> {
        let text = self.restart_text()?;
        let path = self.restart_path(file_name);
        fs::write(&path, text).map_err(|e| io_error(&path, &e))
    }

    /// Upstream `readRestartFile(file_name)` (see the module docs for UD-394;
    /// `ignore_values` = compat switch).
    ///
    /// # Errors
    /// Unsynchronised changes, unreadable file, a header that differs from
    /// the array, or fewer values than elements.
    pub fn read_restart_file(&mut self, file_name: &str, ignore_values: bool) -> Result<()> {
        let path = self.restart_path(file_name);
        let text = fs::read(&path).map_err(|e| io_error(&path, &e))?;
        self.read_restart_text(&text, ignore_values)
            .map_err(|e| match e {
                Error::InvalidArgument(m) => {
                    Error::invalid(format!("{m} (restart file {})", path.display()))
                }
                other => other,
            })
    }

    /// Parse restart text into this array (see [`Self::read_restart_file`]).
    ///
    /// # Errors
    /// See [`Self::read_restart_file`].
    pub fn read_restart_text(&mut self, text: &[u8], ignore_values: bool) -> Result<()> {
        self.check_synchronized("read")?;
        // first line: size and nb_component (upstream getline + >>)
        let (line1, rest) = match text.iter().position(|&b| b == b'\n') {
            Some(p) => (&text[..p], &text[p + 1..]),
            None => (text, &[][..]),
        };
        let size = stream::extract_int::<i64>(line1, 0);
        let comp = stream::extract_int::<i64>(line1, size.consumed);
        let (Some(size_v), Some(comp_v)) = (size.value, comp.value) else {
            return Err(Error::invalid(format!(
                "Could not read the size line of the restart file for SynchronizedArray {}",
                self.id
            )));
        };
        if usize::try_from(size_v).ok() != Some(self.len())
            || usize::try_from(comp_v).ok() != Some(self.nb_component)
        {
            return Err(Error::invalid(format!(
                "Restart file for SynchronizedArray {} holds {size_v} x {comp_v} values, the array has {} x {}",
                self.id,
                self.len(),
                self.nb_component
            )));
        }
        // second line: the values
        let line2 = match rest.iter().position(|&b| b == b'\n') {
            Some(p) => &rest[..p],
            None => rest,
        };
        let mut values = Vec::with_capacity(self.data.len());
        let mut pos = 0;
        for _ in 0..self.data.len() {
            if pos >= line2.len() {
                return Err(Error::invalid(format!(
                    "Read SynchronizedArray {} got to the end of the file before having read all data!",
                    self.id
                )));
            }
            let (v, consumed) = T::read_restart(line2, pos);
            pos += consumed;
            match v {
                Some(v) => values.push(v),
                None => {
                    return Err(Error::invalid(format!(
                        "Read SynchronizedArray {}: malformed value in the restart file",
                        self.id
                    )));
                }
            }
        }
        if !ignore_values {
            self.data.copy_from_slice(&values);
        }
        Ok(())
    }
}

impl<T: SyncValue> SyncDependent for SynchronizedArray<T> {
    fn sync_id(&self) -> &str {
        &self.id
    }

    fn sync_added_elements(&mut self, nb_added: usize) -> Result<usize> {
        if self.nb_added_elements != 0 || !self.deleted_elements.is_empty() {
            return Err(Error::invalid(format!(
                "Cannot sync with SynchronizedArray {} as it has already been modified",
                self.id
            )));
        }
        for _ in 0..nb_added {
            for _ in 0..self.nb_component {
                self.data.push(self.default_value);
            }
        }
        Ok(self.len())
    }

    fn sync_deleted_elements(&mut self, deleted: &[usize]) -> Result<usize> {
        if self.nb_added_elements != 0 || !self.deleted_elements.is_empty() {
            return Err(Error::invalid(format!(
                "Cannot sync with SynchronizedArray {} as it has already been modified",
                self.id
            )));
        }
        for &i in deleted {
            self.erase(i)?;
        }
        self.deleted_elements.clear();
        Ok(self.len())
    }
}

impl<T: SyncValue + fmt::Display> fmt::Display for SynchronizedArray<T> {
    /// Upstream `printself` (layout simplified: the values are listed per
    /// element).
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "SynchronizedArray [")?;
        writeln!(f, " + id                : {}", self.id)?;
        writeln!(f, " + default_value     : {}", self.default_value)?;
        writeln!(f, " + nb_added_elements : {}", self.nb_added_elements)?;
        write!(f, " + deleted_elements  : ")?;
        for d in &self.deleted_elements {
            write!(f, "{d} ")?;
        }
        writeln!(f)?;
        for (i, t) in self.data.chunks_exact(self.nb_component).enumerate() {
            write!(f, "  {i}:")?;
            for v in t {
                write!(f, " {v}")?;
            }
            writeln!(f)?;
        }
        writeln!(f, "]")
    }
}

/// Journal of the structural changes of a contact's node list, replayed on
/// the friction arrays (stand-in for upstream's dependent-array pointers,
/// see the module docs).
#[derive(Clone, Debug, Default)]
pub struct SyncJournal {
    records: Vec<SyncRecord>,
}

impl SyncJournal {
    /// Record a synchronised change.
    pub fn push(&mut self, record: SyncRecord) {
        self.records.push(record);
    }

    /// Number of records (a position for [`Self::replay_since`]).
    #[must_use]
    pub fn len(&self) -> usize {
        self.records.len()
    }

    /// No change recorded.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.records.is_empty()
    }

    /// Records from position `since` on.
    #[must_use]
    pub fn since(&self, since: usize) -> &[SyncRecord] {
        self.records.get(since..).unwrap_or(&[])
    }

    /// Replay the records from `since` on onto `dependents`; returns the new
    /// position.
    ///
    /// # Errors
    /// A dependent fails.
    pub fn replay_since(
        &self,
        since: usize,
        dependents: &mut [&mut dyn SyncDependent],
    ) -> Result<usize> {
        for r in self.since(since) {
            for d in dependents.iter_mut() {
                match r {
                    SyncRecord::Added(n) => d.sync_added_elements(*n)?,
                    SyncRecord::Deleted(list) => d.sync_deleted_elements(list)?,
                };
            }
        }
        Ok(self.records.len())
    }
}

pub(crate) fn io_error(path: &Path, e: &std::io::Error) -> Error {
    Error::invalid(format!("{}: {e}", path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn real(id: &str, nb: usize, def: Real) -> SynchronizedArray<Real> {
        SynchronizedArray::new(nb, id, def, id).unwrap()
    }

    #[test]
    fn push_and_propagate_defaults() {
        let mut root: SynchronizedArray<Idx> =
            SynchronizedArray::new(1, "slaves", 0, "slaves").unwrap();
        let mut a = real("a", 2, 3.5);
        let mut b: SynchronizedArray<bool> = SynchronizedArray::new(1, "b", true, "b").unwrap();
        root.push_value(4).unwrap();
        root.push_value(7).unwrap();
        let rec = root
            .sync_elements(SyncChoice::Added, &mut [&mut a, &mut b])
            .unwrap();
        assert_eq!(rec, Some(SyncRecord::Added(2)));
        assert_eq!(a.as_slice(), &[3.5; 4]);
        assert_eq!(b.as_slice(), &[true, true]);
        assert_eq!(root.pending(), None);
        // a second sync is a no-op
        assert_eq!(
            root.sync_elements(SyncChoice::Added, &mut [&mut a])
                .unwrap(),
            None
        );
    }

    #[test]
    fn erase_replays_swap_removal() {
        let mut root: SynchronizedArray<Idx> = SynchronizedArray::new(1, "r", 0, "r").unwrap();
        let mut dep = real("d", 1, 0.0);
        for i in 0..5 {
            root.push_value(10 + i).unwrap();
        }
        root.sync_elements(SyncChoice::Added, &mut [&mut dep])
            .unwrap();
        for (i, v) in dep.as_mut_slice().iter_mut().enumerate() {
            *v = i as Real;
        }
        // erase index 1 then 0: [10,11,12,13,14] -> [10,14,12,13] -> [13,14,12]
        root.erase(1).unwrap();
        root.erase(0).unwrap();
        assert!(root.push_value(1).is_err(), "push while erasures pending");
        root.sync_elements(SyncChoice::Deleted, &mut [&mut dep])
            .unwrap();
        assert_eq!(root.as_slice(), &[13, 14, 12]);
        assert_eq!(dep.as_slice(), &[3.0, 4.0, 2.0]);
    }

    #[test]
    fn size_mismatch_is_reported() {
        let mut root: SynchronizedArray<Idx> = SynchronizedArray::new(1, "r", 0, "r").unwrap();
        let mut dep = real("d", 1, 0.0);
        dep.push_value(1.0).unwrap();
        dep.sync_elements(SyncChoice::Added, &mut []).unwrap();
        root.push_value(1).unwrap();
        root.push_value(2).unwrap();
        assert!(
            root.sync_elements(SyncChoice::Added, &mut [&mut dep])
                .is_err()
        );
    }

    #[test]
    fn restart_round_trip_and_upstream_format() {
        let mut a = real("a", 2, 0.0);
        a.push_tuple(&[0.1, -2.5e-7]).unwrap();
        a.push_tuple(&[1.0 / 3.0, 1e20]).unwrap();
        a.sync_elements(SyncChoice::Added, &mut []).unwrap();
        let text = a.restart_text().unwrap();
        assert_eq!(
            text, "2 2\n0.1 -2.5e-07 0.333333333333 1e+20 \n",
            "upstream setprecision(12) default format"
        );
        let mut b = real("b", 2, 0.0);
        b.push_value(0.0).unwrap();
        b.push_value(0.0).unwrap();
        b.sync_elements(SyncChoice::Added, &mut []).unwrap();
        // compat: values untouched
        b.read_restart_text(text.as_bytes(), true).unwrap();
        assert_eq!(b.as_slice(), &[0.0; 4]);
        b.read_restart_text(text.as_bytes(), false).unwrap();
        assert_eq!(b.as_slice(), &[0.1, -2.5e-7, 0.333333333333, 1e20]);
        // header mismatch refused
        let mut c = real("c", 2, 0.0);
        assert!(c.read_restart_text(text.as_bytes(), false).is_err());
        // truncated data refused
        assert!(b.read_restart_text(b"2 2\n0.1 0.2\n", false).is_err());
    }

    #[test]
    fn bool_and_int_restart() {
        let mut s: SynchronizedArray<bool> = SynchronizedArray::new(1, "s", false, "s").unwrap();
        s.push_value(true).unwrap();
        s.push_value(false).unwrap();
        s.sync_elements(SyncChoice::Added, &mut []).unwrap();
        let t = s.restart_text().unwrap();
        assert_eq!(t, "2 1\n1 0 \n");
        s.set(false);
        s.read_restart_text(t.as_bytes(), false).unwrap();
        assert_eq!(s.as_slice(), &[true, false]);
        let mut i: SynchronizedArray<Idx> = SynchronizedArray::new(1, "i", 0, "i").unwrap();
        i.push_value(-3).unwrap();
        i.sync_elements(SyncChoice::Added, &mut []).unwrap();
        assert_eq!(i.restart_text().unwrap(), "1 1\n-3 \n");
        assert!(i.dump_restart_file("/nonexistent-dir/x").is_err());
    }

    #[test]
    fn journal_replay_matches_direct_dependents() {
        let mut j = SyncJournal::default();
        j.push(SyncRecord::Added(4));
        j.push(SyncRecord::Deleted(vec![0]));
        j.push(SyncRecord::Added(1));
        let mut late = real("late", 1, 2.0);
        let pos = j.replay_since(0, &mut [&mut late]).unwrap();
        assert_eq!(pos, 3);
        assert_eq!(late.len(), 4);
        assert_eq!(j.replay_since(pos, &mut [&mut late]).unwrap(), 3);
        assert_eq!(late.len(), 4);
    }
}
