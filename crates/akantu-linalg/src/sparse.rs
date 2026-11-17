// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_matrix.hh,
// sparse_matrix.cc, sparse_matrix_inline_impl.hh, sparse_matrix_aij.hh,
// sparse_matrix_aij.cc, sparse_matrix_aij_inline_impl.hh.

//! Sparse matrices (upstream `SparseMatrix` / `SparseMatrixAIJ`, SPM-001…007,
//! decision record D-014).
//!
//! # Semantics (normative)
//!
//! Upstream separates three things that the port reproduces separately:
//!
//! 1. **Lookup key.** `key(i, j)` is `(min, max)` for symmetric matrices and
//!    `(i, j)` otherwise (`sparse_matrix_aij.hh:153-158`). It is only used
//!    to find an existing entry.
//! 2. **Stored coordinates.** A new entry stores the coordinates **as first
//!    inserted** — `irn.push_back(i + 1); jcn.push_back(j + 1)` in
//!    `sparse_matrix_aij_inline_impl.hh:30-58` — so a symmetric matrix can
//!    hold entries from either triangle; there is no "lower triangle" rule.
//!    Entry order is first-insertion order (profile construction order), so
//!    the COO arrays handed to MUMPS equal upstream's.
//! 3. **Value selection** when an elemental block is added
//!    (`addValues`, `:183-196`):
//!    * symmetric block into symmetric matrix — local upper triangle
//!      `j >= i` (`:113-128`);
//!    * unsymmetric block into unsymmetric matrix — all entries (`:150-164`);
//!    * unsymmetric block into symmetric matrix — upstream keeps only the
//!      entries whose **global** column is `>= ` the global row
//!      (`c_jcn >= c_irn`, `:131-147`), silently dropping the other half
//!      (UD-034). This is reproduced only under
//!      [`SymmetryPolicy::UpstreamUpperSelection`]; the default
//!      [`SymmetryPolicy::PromoteOnUnsymmetric`] converts the matrix to
//!      unsymmetric storage first (mirror entries appended in storage
//!      order, profile release bumped) so the full block is kept and solver
//!      backends re-analyse with a general factorisation.
//!
//! Values are accumulated in upstream loop order (rows outer, columns
//! inner, each `a[k] += value` a single binary64 addition), so assembled
//! values are bit-identical to the oracle's.
//!
//! `add_values` requires every touched entry to be in the profile (upstream
//! asserts in debug and is undefined in release): the port checks all
//! entries first and returns an error **without modifying** the matrix —
//! including its symmetry: a promotion is committed only after every entry
//! of the block was found. A local promotion is a
//! *proposal*: the DOF manager agrees the effective symmetry on all ranks
//! at the collective end of the assembly phase and propagates it to
//! derived operators (`J`) and solver backends.
//!
//! # Release counters
//!
//! `profile_release` and `value_release` start at 1 and are incremented
//! exactly where upstream increments them: `add(i, j)` of a new entry
//! (both), `add(i, j, v)` (value, after a possible profile insertion), every
//! entry touched by `addValues` (value, upstream's mutable `operator()`),
//! `set`/`zero`, `mul`, `applyBoundary`, `copyContent` (value),
//! `clearProfile` (both), `copyProfile` (profile copied from the source,
//! value incremented). Upstream compares them with `!=` only; the port
//! increments with wrapping arithmetic. Port-only operations (promotion)
//! increment both.
//!
//! # Coordinates
//!
//! Coordinates are **global equation numbers** ([`GlobalIdx`]), 0-based in
//! memory. Local rows are obtained through an
//! [`EquationLayout`]. Backends with narrower
//! index types convert through [`SparseMatrixAij::coo_one_based_i32`], which
//! rejects unrepresentable coordinates before allocating anything.

use std::collections::hash_map::Entry;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use akantu_core::enums::MatrixType;
use akantu_core::{Error, GlobalIdx, INVALID_GLOBAL_IDX, Rank, Real, Result};

use crate::int_hash::IntHashMap;
use crate::layout::{EquationLayout, EquationOwnership, PartialSumReduction};
use crate::matrix_market;

/// What happens when an unsymmetric elemental block is added to a matrix
/// declared symmetric (UD-034, D-003 class B).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum SymmetryPolicy {
    /// Default (UD-034 fix): the matrix becomes unsymmetric (mirror entries
    /// appended in storage order, `profile_release` bumped) and the full
    /// block is added. The change is visible through
    /// [`SparseMatrixAij::matrix_type`] and [`SparseMatrixAij::was_promoted`]
    /// and must be propagated by the DOF manager to derived matrices (`J`)
    /// and to the solver backend (MUMPS `sym = 0`).
    #[default]
    PromoteOnUnsymmetric,
    /// Upstream compatibility: keep symmetric storage and add only entries
    /// with global column `>=` global row (`addUnsymmetricValuesToSymmetric`).
    UpstreamUpperSelection,
}

/// Upstream `SparseMatrix` abstract base (SPM-001): the operations every
/// sparse matrix type provides. [`SparseMatrixAij`] is the only in-tree
/// implementation; native backends (PETSc) implement their own
/// storage behind `akantu-solver`'s `LinearSystemBackend` and may implement
/// this trait as well. Operations upstream leaves unimplemented in the base
/// (`AKANTU_TO_IMPLEMENT`) return [`Error::NotImplemented`] by default.
pub trait SparseMatrix: std::fmt::Debug + Send {
    /// Identifier.
    fn id(&self) -> &str;
    /// Global order (upstream `size()`).
    fn size(&self) -> GlobalIdx;
    /// Effective symmetry (upstream `getMatrixType`).
    fn matrix_type(&self) -> MatrixType;
    /// Number of stored entries (upstream `getNbNonZero`).
    fn nb_non_zero(&self) -> usize;
    /// Value release (upstream `getRelease`).
    fn release(&self) -> u64;
    /// Remove the profile (upstream `clearProfile`).
    fn clear_profile(&mut self);
    /// Set every stored value to `val` (upstream `set`).
    fn set(&mut self, val: Real);
    /// Set every stored value to zero (upstream `zero`).
    fn zero(&mut self) {
        self.set(0.0);
    }
    /// Add `(i, j)` to the profile; returns the entry position.
    fn add_to_profile(&mut self, i: GlobalIdx, j: GlobalIdx) -> Result<usize>;
    /// `A(i, j) += value`, creating the entry if needed.
    fn add(&mut self, i: GlobalIdx, j: GlobalIdx, value: Real) -> Result<()>;
    /// `A *= alpha`.
    fn mul(&mut self, alpha: Real);
    /// `self += alpha · B` (upstream `add(B, alpha)` → `B.addMeTo(*this,
    /// alpha)`, double dispatch on the type of `B`).
    fn add_matrix(&mut self, b: &dyn SparseMatrix, alpha: Real) -> Result<()>
    where
        Self: Sized,
    {
        b.add_me_to(self, alpha)
    }
    /// `B += alpha · self` (upstream `addMeTo`).
    fn add_me_to(&self, b: &mut dyn SparseMatrix, alpha: Real) -> Result<()>;
    /// `y = beta·y + alpha·A·x` over local equations.
    fn mat_vec_mul(
        &self,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()>;
    /// Dirichlet modification (upstream `applyBoundary(block_val)`).
    fn apply_boundary(
        &mut self,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()>;
    /// Copy the profile of `other` (upstream `copyProfile`).
    fn copy_profile(&mut self, other: &dyn SparseMatrix) -> Result<()>;
    /// `true` if every stored value is finite (upstream `isFinite`).
    fn is_finite(&self) -> Result<bool> {
        Err(Error::NotImplemented("SparseMatrix::isFinite"))
    }
    /// Value at `(i, j)` (upstream `operator() const`).
    fn value(&self, _i: GlobalIdx, _j: GlobalIdx) -> Result<Real> {
        Err(Error::NotImplemented("SparseMatrix::operator()"))
    }
    /// Smallest stored value (upstream `min()`).
    fn min(&self) -> Result<Real>;
    /// Save the profile as a Matrix Market pattern (upstream `saveProfile`).
    fn save_profile(&self, _path: &Path) -> Result<()> {
        Err(Error::NotImplemented("SparseMatrix::saveProfile"))
    }
    /// Save the matrix as Matrix Market (upstream `saveMatrix`).
    fn save_matrix(&self, _path: &Path) -> Result<PathBuf> {
        Err(Error::NotImplemented("SparseMatrix::saveMatrix"))
    }
    /// Downcast to the AIJ storage (double dispatch of `addMeTo`).
    fn as_aij(&self) -> Option<&SparseMatrixAij> {
        None
    }
    /// Mutable downcast to the AIJ storage.
    fn as_aij_mut(&mut self) -> Option<&mut SparseMatrixAij> {
        None
    }
}

/// COO matrix with a profile (upstream `SparseMatrixAIJ`).
///
/// Invariants: `irn.len() == jcn.len() == a.len()`; each lookup key appears
/// at most once in the profile map; every coordinate is a valid global
/// equation.
/// Source of [`ProfileStamp`]s (process-wide, never repeats in practice:
/// 2^64 profile changes).
static NEXT_PROFILE_STAMP: AtomicU64 = AtomicU64::new(1);

/// Process-unique identity of one profile of one matrix object:
/// drawn afresh at construction, at every profile change and for
/// every clone, so two matrix objects (or two profiles of one object) never
/// share it — unlike the upstream release counters, which collide between
/// matrices built with the same number of additions and are copied by
/// `copy_profile`.
#[derive(Debug, PartialEq, Eq)]
struct ProfileStamp(u64);

impl ProfileStamp {
    fn fresh() -> Self {
        Self(NEXT_PROFILE_STAMP.fetch_add(1, Ordering::Relaxed))
    }
}

impl Clone for ProfileStamp {
    /// A clone is another matrix object: it gets its own identity.
    fn clone(&self) -> Self {
        Self::fresh()
    }
}

/// A [`ProfileStamp`] drawn when it is first read after a change (`0`:
/// not drawn yet), so that the profile construction, which changes the
/// profile at every new entry, does not draw one per entry from the
/// process-wide counter. Unique as a drawn stamp: a change discards the
/// stamp, the next read draws a new one.
#[derive(Debug, Default)]
struct LazyStamp(AtomicU64);

impl LazyStamp {
    /// Discard the stamp (the profile changed).
    fn invalidate(&mut self) {
        *self.0.get_mut() = 0;
    }

    /// The stamp, drawn now if it was discarded.
    fn get(&self) -> u64 {
        let s = self.0.load(Ordering::Relaxed);
        if s != 0 {
            return s;
        }
        let new = ProfileStamp::fresh().0;
        match self
            .0
            .compare_exchange(0, new, Ordering::Relaxed, Ordering::Relaxed)
        {
            Ok(_) => new,
            Err(current) => current,
        }
    }
}

impl Clone for LazyStamp {
    /// A clone is another matrix object: it draws its own identity.
    fn clone(&self) -> Self {
        Self::default()
    }
}

/// Global sparse matrix in coordinate (COO) storage (0-based coordinates, with
/// 1-based accessors for the solvers) with
/// a first-insertion-order profile (upstream `SparseMatrixAIJ`,
/// `src/solver/sparse_matrix_aij.hh`).
///
/// Holds the row/column/value arrays handed to the direct solvers, the
/// lookup from `(row, column)` to entry position, and the profile and value
/// release counters. Which entries an elemental block contributes, and how a
/// symmetric matrix is stored, is described in the module documentation.
#[derive(Clone, Debug)]
pub struct SparseMatrixAij {
    id: String,
    size: GlobalIdx,
    matrix_type: MatrixType,
    policy: SymmetryPolicy,
    promoted: bool,
    /// Row (global equation, 0-based) of each entry, first-insertion order.
    irn: Vec<GlobalIdx>,
    /// Column (global equation, 0-based) of each entry.
    jcn: Vec<GlobalIdx>,
    /// Values.
    a: Vec<Real>,
    /// Lookup key → entry position (upstream `irn_jcn_k`).
    profile: IntHashMap<(GlobalIdx, GlobalIdx), usize>,
    /// Incremented when the profile changes (upstream `profile_release`).
    profile_release: u64,
    /// Incremented when values change (upstream `value_release`).
    value_release: u64,
    /// Process-unique identity of the current profile (port extension;
    /// see [`SparseMatrixAij::profile_identity`]).
    profile_stamp: LazyStamp,
    /// Identity of the current *entry positions*: drawn afresh whenever an
    /// existing entry may move or change its key (construction, clone,
    /// `clear_profile`, `copy_profile`, promotion), but not when entries
    /// are appended, which leaves every existing position valid
    /// ([`BlockPositionCache`]).
    position_epoch: ProfileStamp,
    /// Reusable scratch for `add_values` (no per-element allocation).
    scratch: Vec<usize>,
    /// Largest stored coordinate (row or column), `-1` without entries
    /// (stored coordinates are never negative).
    max_coord: GlobalIdx,
    /// Values and lookup map released, coordinates kept
    /// ([`SparseMatrixAij::release_values`]); revived by the next mutation.
    released: bool,
    /// Row-gathered terms of the threaded matrix-vector product (built on
    /// first use, keyed by the entry positions).
    row_gather: threaded::RowGatherCache,
}

impl SparseMatrixAij {
    /// Empty `size × size` matrix (global size; upstream initialises
    /// `size_` to the DOF manager's system size).
    #[must_use]
    pub fn new(id: impl Into<String>, size: GlobalIdx, matrix_type: MatrixType) -> Self {
        Self {
            id: id.into(),
            size,
            matrix_type,
            policy: SymmetryPolicy::default(),
            promoted: false,
            irn: Vec::new(),
            jcn: Vec::new(),
            a: Vec::new(),
            profile: IntHashMap::default(),
            profile_release: 1,
            value_release: 1,
            profile_stamp: LazyStamp::default(),
            position_epoch: ProfileStamp::fresh(),
            scratch: Vec::new(),
            max_coord: -1,
            released: false,
            row_gather: threaded::RowGatherCache::default(),
        }
    }

    /// Copy of `other` under a new id (upstream copy constructor
    /// `SparseMatrixAIJ(matrix, id)`: arrays and type copied, the hash map
    /// and the releases are **not** copied by upstream — the copy starts at
    /// release 1 with an empty lookup map, so lookups on it fail. The port
    /// rebuilds the lookup map (UD-179, class B: upstream's copy cannot be
    /// assembled into) and keeps upstream's fresh releases).
    #[must_use]
    pub fn copy_with_id(other: &Self, id: impl Into<String>) -> Self {
        let mut m = other.clone();
        m.id = id.into();
        m.profile_release = 1;
        m.value_release = 1;
        m.scratch = Vec::new();
        m
    }

    /// Select the UD-034 policy (default: [`SymmetryPolicy::PromoteOnUnsymmetric`]).
    pub fn set_symmetry_policy(&mut self, policy: SymmetryPolicy) {
        self.policy = policy;
    }

    /// Current UD-034 policy.
    #[must_use]
    pub fn symmetry_policy(&self) -> SymmetryPolicy {
        self.policy
    }

    /// Identifier.
    #[must_use]
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Global order of the matrix.
    #[must_use]
    pub fn size(&self) -> GlobalIdx {
        self.size
    }

    /// Grow the order (upstream `resize(system_size)` before profiling).
    pub fn resize(&mut self, size: GlobalIdx) {
        if size > self.size {
            self.size = size;
        }
    }

    /// Set the order exactly (upstream `resize(n)`, which may also shrink;
    /// entries at equations `>= n` stay stored, as upstream).
    pub fn set_size(&mut self, size: GlobalIdx) {
        self.size = size;
    }

    /// Effective symmetry (after a possible promotion).
    #[must_use]
    pub fn matrix_type(&self) -> MatrixType {
        self.matrix_type
    }

    /// `true` if the matrix was declared symmetric and promoted to
    /// unsymmetric storage by [`SymmetryPolicy::PromoteOnUnsymmetric`].
    #[must_use]
    pub fn was_promoted(&self) -> bool {
        self.promoted
    }

    /// Number of stored entries (upstream `getNbNonZero`).
    #[must_use]
    pub fn nnz(&self) -> usize {
        self.a.len()
    }

    /// Profile release counter (upstream `getProfileRelease`).
    #[must_use]
    pub fn profile_release(&self) -> u64 {
        self.profile_release
    }

    /// Value release counter (upstream `getValueRelease`, `getRelease`).
    #[must_use]
    pub fn value_release(&self) -> u64 {
        self.value_release
    }

    /// Process-unique identity of this matrix object's **current profile**
    /// (port extension): changes whenever the profile
    /// changes (with [`SparseMatrixAij::profile_release`], and also on
    /// `copy_profile`, which copies upstream's release), and is never shared
    /// with another matrix object, a clone or another process-local matrix.
    /// Together with [`SparseMatrixAij::value_release`] it identifies the
    /// operator a solver's analysis and factors belong to: equal identities
    /// and value releases mean the same coordinates in the same order with
    /// unchanged values. Local information (each process draws its own).
    #[must_use]
    pub fn profile_identity(&self) -> u64 {
        self.profile_stamp.get()
    }

    /// Raw COO data `(irn, jcn, a)`, 0-based global coordinates.
    #[must_use]
    pub fn coo(&self) -> (&[GlobalIdx], &[GlobalIdx], &[Real]) {
        (&self.irn, &self.jcn, &self.a)
    }

    /// Stored values (upstream `getA`).
    #[must_use]
    pub fn values(&self) -> &[Real] {
        &self.a
    }

    /// 1-based rows as upstream stores them (upstream `getIRN`).
    #[must_use]
    pub fn irn_one_based(&self) -> Vec<GlobalIdx> {
        self.irn.iter().map(|&i| i + 1).collect()
    }

    /// 1-based columns as upstream stores them (upstream `getJCN`).
    #[must_use]
    pub fn jcn_one_based(&self) -> Vec<GlobalIdx> {
        self.jcn.iter().map(|&j| j + 1).collect()
    }

    fn bump_profile(&mut self) {
        self.profile_release = self.profile_release.wrapping_add(1);
        self.profile_stamp.invalidate();
    }

    fn bump_values(&mut self, n: u64) {
        self.value_release = self.value_release.wrapping_add(n);
    }

    fn key(&self, i: GlobalIdx, j: GlobalIdx) -> (GlobalIdx, GlobalIdx) {
        if self.matrix_type == MatrixType::Symmetric && i > j {
            (j, i)
        } else {
            (i, j)
        }
    }

    fn check_coordinate(g: GlobalIdx) -> Result<()> {
        if g == INVALID_GLOBAL_IDX || g < 0 || g == GlobalIdx::MAX {
            return Err(Error::InvalidArgument(format!(
                "invalid global equation {g} in sparse matrix coordinates"
            )));
        }
        Ok(())
    }

    /// Position of the entry `(i, j)` in storage, if it is in the profile.
    #[must_use]
    pub fn position(&self, i: GlobalIdx, j: GlobalIdx) -> Option<usize> {
        self.profile.get(&self.key(i, j)).copied()
    }

    /// Ensure the entry `(i, j)` exists (upstream `add(i, j)`): a new entry
    /// stores `(i, j)` as given and is appended; an existing key is reused
    /// whatever triangle it was first inserted from. Returns the position.
    pub fn add_to_profile(&mut self, i: GlobalIdx, j: GlobalIdx) -> Result<usize> {
        self.revive();
        Self::check_coordinate(i)?;
        Self::check_coordinate(j)?;
        let key = self.key(i, j);
        let k = self.a.len();
        match self.profile.entry(key) {
            Entry::Occupied(o) => return Ok(*o.get()),
            Entry::Vacant(v) => {
                v.insert(k);
            }
        }
        self.size = self.size.max(i + 1).max(j + 1);
        self.max_coord = self.max_coord.max(i).max(j);
        self.irn.push(i);
        self.jcn.push(j);
        self.a.push(0.0);
        self.bump_profile();
        self.bump_values(1);
        Ok(k)
    }

    /// Profile an elemental block: for every row `i` (in order) and every
    /// column `j` (in order) whose global equations are `< size`, ensure the
    /// entry exists (upstream `DOFManagerDefault::addToProfile`,
    /// `dof_manager_default.cc:295-388`, full local loop for symmetric and
    /// unsymmetric matrices alike). All coordinates are validated first: no
    /// partial profile on error.
    pub fn add_block_to_profile(&mut self, rows: &[GlobalIdx], cols: &[GlobalIdx]) -> Result<()> {
        for &g in rows.iter().chain(cols) {
            if g < self.size {
                Self::check_coordinate(g)?;
            }
        }
        for &gi in rows {
            if gi < self.size {
                for &gj in cols {
                    if gj < self.size {
                        self.add_to_profile(gi, gj)?;
                    }
                }
            }
        }
        Ok(())
    }

    /// `true` if `add_values` with blocks of type `values_type` would
    /// promote this matrix to unsymmetric storage
    /// ([`SymmetryPolicy::PromoteOnUnsymmetric`]).
    #[must_use]
    pub fn would_promote(&self, values_type: MatrixType) -> bool {
        self.matrix_type == MatrixType::Symmetric
            && values_type != MatrixType::Symmetric
            && self.policy == SymmetryPolicy::PromoteOnUnsymmetric
    }

    /// `add_block_to_profile(rows, cols)` followed by `add_values(rows,
    /// cols, values, values_type)` with one profile lookup per pair instead
    /// of two: the positions found while profiling are the ones
    /// `add_values` would look up. The profile, the values, their order and
    /// the release counters are those of the two calls. Refused (an error,
    /// nothing changed) when `add_values` would promote the matrix
    /// ([`SparseMatrixAij::would_promote`]): a promotion between the
    /// profiling of two elements would change the entry order, so the
    /// caller profiles every element first in that case.
    pub fn add_block_to_profile_and_values(
        &mut self,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.revive();
        let (nr, nc) = (rows.len(), cols.len());
        if self.would_promote(values_type) {
            return Err(Error::invalid(
                "add_block_to_profile_and_values: the block would promote the matrix",
            ));
        }
        if values.len() != nr * nc {
            return Err(Error::DimensionMismatch {
                context: "SparseMatrixAij::add_values",
                expected: nr * nc,
                got: values.len(),
            });
        }
        let symmetric = self.matrix_type == MatrixType::Symmetric;
        if symmetric && values_type == MatrixType::Symmetric && nr != nc {
            return Err(Error::InvalidArgument(
                "a symmetric elemental block must be square".into(),
            ));
        }
        for &g in rows.iter().chain(cols) {
            if g < self.size {
                Self::check_coordinate(g)?;
            }
        }
        // Profile (upstream order), recording the position of each pair.
        let mut pos = std::mem::take(&mut self.scratch);
        pos.clear();
        pos.resize(nr * nc, usize::MAX);
        for (i, &gi) in rows.iter().enumerate() {
            if gi < self.size {
                for (j, &gj) in cols.iter().enumerate() {
                    if gj < self.size {
                        match self.add_to_profile(gi, gj) {
                            Ok(k) => pos[i * nc + j] = k,
                            Err(e) => {
                                self.scratch = pos;
                                return Err(e);
                            }
                        }
                    }
                }
            }
        }
        // Values: the loop of `add_values`, positions from the profiling.
        let mode = if symmetric && values_type == MatrixType::Symmetric {
            Selection::LocalUpper
        } else if symmetric {
            Selection::GlobalUpper
        } else {
            Selection::All
        };
        let mut touched: u64 = 0;
        for (i, &gi) in rows.iter().enumerate() {
            if gi >= self.size {
                continue;
            }
            let j0 = if mode == Selection::LocalUpper { i } else { 0 };
            for (j, &gj) in cols.iter().enumerate().skip(j0) {
                if gj >= self.size || (mode == Selection::GlobalUpper && gj < gi) {
                    continue;
                }
                self.a[pos[i * nc + j]] += values[i + j * nr];
                touched += 1;
            }
        }
        self.bump_values(touched);
        self.scratch = pos;
        Ok(())
    }

    /// Add `value` at `(i, j)`, creating the entry if needed (upstream
    /// `add(i, j, value)`, used for pre-assembled terms and matrix sums).
    pub fn add(&mut self, i: GlobalIdx, j: GlobalIdx, value: Real) -> Result<()> {
        let k = self.add_to_profile(i, j)?;
        self.a[k] += value;
        self.bump_values(1);
        Ok(())
    }

    /// Value at `(i, j)` (0 if not in the profile; upstream `operator() const`).
    #[must_use]
    pub fn get(&self, i: GlobalIdx, j: GlobalIdx) -> Real {
        self.profile
            .get(&self.key(i, j))
            .map_or(0.0, |&k| self.a[k])
    }

    /// Mutable access to an existing entry (upstream mutable `operator()`:
    /// asserts the entry exists and increments the value release because
    /// the caller may change it). Missing entries are an error.
    pub fn get_mut(&mut self, i: GlobalIdx, j: GlobalIdx) -> Result<&mut Real> {
        self.revive();
        let k = self.position(i, j).ok_or_else(|| self.missing(i, j))?;
        self.bump_values(1);
        Ok(&mut self.a[k])
    }

    fn missing(&self, i: GlobalIdx, j: GlobalIdx) -> Error {
        Error::InvalidArgument(format!(
            "entry ({i}, {j}) of matrix '{}' is not in the profile",
            self.id
        ))
    }

    /// Diagonal of the matrix over global equations `0..size` (0 where no
    /// diagonal entry is stored). Port helper (upstream reads diagonals
    /// through `operator()(i, i)`).
    pub fn diagonal(&self) -> Result<Vec<Real>> {
        let n = usize::try_from(self.size)
            .map_err(|_| Error::invalid("matrix order exceeds the address space"))?;
        let mut d = vec![0.0; n];
        for ((&i, &j), &v) in self.irn.iter().zip(&self.jcn).zip(&self.a) {
            if i == j
                && let Some(slot) = usize::try_from(i).ok().and_then(|u| d.get_mut(u))
            {
                *slot = v;
            }
        }
        Ok(d)
    }

    /// Add an elemental block with global row equations `rows` and column
    /// equations `cols`; `values` is column-major `rows.len() × cols.len()`
    /// (upstream `addValues(is, js, values, values_type)`). Equations
    /// `>= size` are skipped as upstream (`c_irn < size_`). See the module
    /// documentation for the value-selection rules. The value release is
    /// incremented once per touched entry (upstream mutable `operator()`).
    pub fn add_values(
        &mut self,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.revive();
        let (nr, nc) = (rows.len(), cols.len());
        if values.len() != nr * nc {
            return Err(Error::DimensionMismatch {
                context: "SparseMatrixAij::add_values",
                expected: nr * nc,
                got: values.len(),
            });
        }
        let symmetric = self.matrix_type == MatrixType::Symmetric;
        if symmetric && values_type == MatrixType::Symmetric && nr != nc {
            return Err(Error::InvalidArgument(
                "a symmetric elemental block must be square".into(),
            ));
        }
        if symmetric
            && values_type != MatrixType::Symmetric
            && self.policy == SymmetryPolicy::PromoteOnUnsymmetric
        {
            // Validate before promoting: after a promotion every
            // canonical key exists in both orientations, so the block is
            // resolvable afterwards iff all its canonical keys exist now.
            let mut probe = std::mem::take(&mut self.scratch);
            probe.clear();
            let checked = self.resolve_block(rows, cols, Selection::All, &mut probe);
            self.scratch = probe;
            checked?;
            self.promote_to_unsymmetric();
        }
        let mode = match (self.matrix_type, values_type) {
            (MatrixType::Symmetric, MatrixType::Symmetric) => Selection::LocalUpper,
            (MatrixType::Symmetric, _) => Selection::GlobalUpper,
            _ => Selection::All,
        };
        // Pass 1: resolve every touched entry (no mutation on error).
        let mut scratch = std::mem::take(&mut self.scratch);
        scratch.clear();
        let result = self.resolve_block(rows, cols, mode, &mut scratch);
        if let Err(e) = result {
            self.scratch = scratch;
            return Err(e);
        }
        // Pass 2: accumulate in upstream loop order.
        let mut s = scratch.iter();
        for (i, &gi) in rows.iter().enumerate() {
            if gi >= self.size {
                continue;
            }
            let j0 = if mode == Selection::LocalUpper { i } else { 0 };
            for (j, &gj) in cols.iter().enumerate().skip(j0) {
                if gj >= self.size || (mode == Selection::GlobalUpper && gj < gi) {
                    continue;
                }
                if let Some(&k) = s.next() {
                    self.a[k] += values[i + j * nr];
                }
            }
        }
        let touched = u64::try_from(scratch.len()).unwrap_or(u64::MAX);
        self.bump_values(touched);
        self.scratch = scratch;
        Ok(())
    }

    /// [`SparseMatrixAij::add_values`] for slot `slot` of a repeated
    /// assembly (one slot per element of an element loop), with the entry
    /// positions of the block cached in `cache`.
    ///
    /// The values added, their order, the value-release increments, the
    /// errors and the promotion of [`SymmetryPolicy::PromoteOnUnsymmetric`]
    /// are exactly those of `add_values(rows, cols, values, values_type)`:
    /// the cache only replaces the profile lookups of a block whose
    /// equations, entry positions (unchanged since the positions were
    /// resolved, up to appended entries: an appended entry moves no other
    /// one), order and selection mode equal those the positions were
    /// resolved for. Positions are stored from the second assembly round
    /// ([`BlockPositionCache::begin`]) with unchanged positions, so a
    /// one-shot assembly costs no memory; see [`BlockPositionCache`] for
    /// the memory a stored slot takes.
    pub fn add_values_cached(
        &mut self,
        cache: &mut BlockPositionCache,
        slot: usize,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        values: &[Real],
        values_type: MatrixType,
    ) -> Result<()> {
        self.revive();
        let (nr, nc) = (rows.len(), cols.len());
        let symmetric = self.matrix_type == MatrixType::Symmetric;
        let usable = cache.stamp == self.position_epoch.0
            && values.len() == nr * nc
            && !(symmetric && values_type == MatrixType::Symmetric && nr != nc)
            && !(symmetric
                && values_type != MatrixType::Symmetric
                && self.policy == SymmetryPolicy::PromoteOnUnsymmetric)
            && u32::try_from(self.a.len()).is_ok_and(|n| n < SKIP)
            && u32::try_from(self.size).is_ok_and(|n| n < u32::MAX);
        if !usable {
            return self.add_values(rows, cols, values, values_type);
        }
        let mode = match (self.matrix_type, values_type) {
            (MatrixType::Symmetric, MatrixType::Symmetric) => Selection::LocalUpper,
            (MatrixType::Symmetric, _) => Selection::GlobalUpper,
            _ => Selection::All,
        };
        cache.configure(mode, nr, nc, self.size);
        if !cache.set_key(rows, cols, self.size) {
            return self.add_values(rows, cols, values, values_type);
        }
        if !cache.matches(slot) {
            if cache.rounds == 0 {
                return self.add_values(rows, cols, values, values_type);
            }
            // Resolve (no mutation of the matrix on error) and store.
            let mut positions = std::mem::take(&mut cache.scratch);
            positions.clear();
            let r = self.resolve_block_positions(rows, cols, mode, &mut positions);
            if let Err(e) = r {
                cache.scratch = positions;
                return Err(e);
            }
            cache.store(slot, &positions);
            cache.resolves += 1;
            cache.scratch = positions;
        }
        let stride = cache.stride;
        let pos = &cache.positions[slot * stride..(slot + 1) * stride];
        let mut touched: u64 = 0;
        let mut t = 0;
        for i in 0..nr {
            let j0 = if mode == Selection::LocalUpper { i } else { 0 };
            for j in j0..nc {
                let k = pos[t];
                t += 1;
                if k != SKIP {
                    self.a[k as usize] += values[i + j * nr];
                    touched += 1;
                }
            }
        }
        self.bump_values(touched);
        Ok(())
    }

    /// Positions of every `(i, j)` pair `add_values` visits in `mode`
    /// (rows outer, columns inner, `j >= i` for the local upper triangle),
    /// [`SKIP`] for the pairs it skips (equation `>= size`, or global
    /// column `<` global row in [`Selection::GlobalUpper`]).
    fn resolve_block_positions(
        &self,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        mode: Selection,
        out: &mut Vec<u32>,
    ) -> Result<()> {
        for (i, &gi) in rows.iter().enumerate() {
            let j0 = if mode == Selection::LocalUpper { i } else { 0 };
            for &gj in cols.iter().skip(j0) {
                if gi >= self.size || gj >= self.size || (mode == Selection::GlobalUpper && gj < gi)
                {
                    out.push(SKIP);
                    continue;
                }
                let k = self
                    .profile
                    .get(&self.key(gi, gj))
                    .ok_or_else(|| self.missing(gi, gj))?;
                // `nnz < SKIP` was checked by the caller.
                out.push(u32::try_from(*k).unwrap_or(SKIP));
            }
        }
        Ok(())
    }

    fn resolve_block(
        &self,
        rows: &[GlobalIdx],
        cols: &[GlobalIdx],
        mode: Selection,
        out: &mut Vec<usize>,
    ) -> Result<()> {
        for (i, &gi) in rows.iter().enumerate() {
            if gi >= self.size {
                continue;
            }
            let j0 = if mode == Selection::LocalUpper { i } else { 0 };
            for &gj in cols.iter().skip(j0) {
                if gj >= self.size || (mode == Selection::GlobalUpper && gj < gi) {
                    continue;
                }
                let k = self
                    .profile
                    .get(&self.key(gi, gj))
                    .ok_or_else(|| self.missing(gi, gj))?;
                out.push(*k);
            }
        }
        Ok(())
    }

    /// Convert symmetric storage to unsymmetric storage: every off-diagonal
    /// entry `(i, j)` gets a mirror `(j, i)` with the same value, appended in
    /// storage order; lookup keys become exact. Bumps both releases.
    pub fn promote_to_unsymmetric(&mut self) {
        self.revive();
        if self.matrix_type != MatrixType::Symmetric {
            return;
        }
        let n = self.a.len();
        self.profile.clear();
        for k in 0..n {
            self.profile.insert((self.irn[k], self.jcn[k]), k);
        }
        for k in 0..n {
            let (i, j, v) = (self.irn[k], self.jcn[k], self.a[k]);
            if i != j {
                let m = self.a.len();
                self.irn.push(j);
                self.jcn.push(i);
                self.a.push(v);
                self.profile.insert((j, i), m);
            }
        }
        self.matrix_type = MatrixType::Unsymmetric;
        self.promoted = true;
        self.bump_profile();
        self.position_epoch = ProfileStamp::fresh();
        self.bump_values(1);
    }

    /// Set all values to `val`, keeping the profile (upstream `set`).
    pub fn set(&mut self, val: Real) {
        self.revive();
        self.a.fill(val);
        self.bump_values(1);
    }

    /// Set all values to zero, keeping the profile (upstream `zero`).
    pub fn zero(&mut self) {
        self.set(0.0);
    }

    /// Multiply every value by `alpha` (upstream `mul`, `a *= alpha`).
    pub fn mul(&mut self, alpha: Real) {
        self.revive();
        for v in &mut self.a {
            *v *= alpha;
        }
        self.bump_values(1);
    }

    /// `true` if every stored value is finite (upstream `isFinite`).
    #[must_use]
    pub fn is_finite(&self) -> bool {
        self.a.iter().all(|v| v.is_finite())
    }

    /// Smallest stored value (upstream `min()`: `std::min_element`, i.e. the
    /// first value no later value is `<` than; NaN never replaces nor is
    /// replaced by a comparison). An empty matrix is an error (upstream
    /// dereferences the end iterator).
    pub fn min(&self) -> Result<Real> {
        let mut it = self.a.iter();
        let mut m = *it
            .next()
            .ok_or_else(|| Error::invalid(format!("min() of the empty matrix '{}'", self.id)))?;
        for &v in it {
            if v < m {
                m = v;
            }
        }
        Ok(m)
    }

    /// `self += alpha · B` exactly as upstream (`B.addMeTo(*this, alpha)`,
    /// `sparse_matrix_aij.cc:252-268`): every stored entry `(i, j, b)` of
    /// `B`, in storage order, is added with `add(i, j, alpha * b)` (the
    /// profile of `self` may grow). When `B` has symmetric storage and
    /// `self` does not, upstream adds only the stored triangle of `B`
    /// (UD-177, class B: `J.add(M, c)` with an unsymmetric `J` loses the
    /// mirror of `M`) — a compatibility mode for differential tests; use
    /// [`SparseMatrixAij::add_matrix`] (default) for the corrected sum.
    pub fn add_matrix_upstream(&mut self, b: &SparseMatrixAij, alpha: Real) -> Result<()> {
        if std::ptr::eq(self, b) {
            return Err(Error::invalid(
                "add_matrix: a matrix cannot be added to itself in place",
            ));
        }
        b.check_not_released("add_matrix")?;
        self.revive();
        if self.same_entries_as(b) {
            self.add_entrywise(b, alpha);
            return Ok(());
        }
        for ((&i, &j), &v) in b.irn.iter().zip(&b.jcn).zip(&b.a) {
            self.add(i, j, alpha * v)?;
        }
        Ok(())
    }

    /// `true` if `self` stores the coordinates of `b` at the same positions
    /// and every entry of `self` has its own lookup key (no two entries
    /// share a key, which only `copy_profile` from a matrix with another key
    /// rule can produce). Then `add(i, j, ·)` of the `k`-th entry of `b`
    /// finds the existing `k`-th entry of `self`.
    fn same_entries_as(&self, b: &SparseMatrixAij) -> bool {
        self.profile.len() == self.a.len() && self.irn == b.irn && self.jcn == b.jcn
    }

    /// `self[k] += alpha * b[k]` for every entry in storage order: what
    /// `add(i, j, alpha * b[k])` does entry by entry when
    /// [`SparseMatrixAij::same_entries_as`] holds (the same binary64
    /// operations, the same value-release increments, no profile change).
    fn add_entrywise(&mut self, b: &SparseMatrixAij, alpha: Real) {
        for (s, &v) in self.a.iter_mut().zip(&b.a) {
            *s += alpha * v;
        }
        self.bump_values(u64::try_from(b.a.len()).unwrap_or(u64::MAX));
    }

    /// `self += alpha · B` with the symmetric-storage fix: identical to
    /// [`SparseMatrixAij::add_matrix_upstream`] (same entries, same order,
    /// bit-identical values) except that when `B` has symmetric storage and
    /// `self` is unsymmetric, every off-diagonal entry `(i, j)` of `B` is
    /// added at `(i, j)` and then at `(j, i)`, so `self` receives the full
    /// operator `B` represents.
    pub fn add_matrix(&mut self, b: &SparseMatrixAij, alpha: Real) -> Result<()> {
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
            self.add_entrywise(b, alpha);
            return Ok(());
        }
        for ((&i, &j), &v) in b.irn.iter().zip(&b.jcn).zip(&b.a) {
            let s = alpha * v;
            self.add(i, j, s)?;
            if mirror && i != j {
                self.add(j, i, s)?;
            }
        }
        Ok(())
    }

    /// Copy the profile of `other` (upstream `copyProfile`,
    /// `sparse_matrix_aij.cc:224-249`): coordinates copied in storage
    /// order, lookup map rebuilt with **this** matrix's key rule (a later
    /// entry with the same key overwrites an earlier one, as upstream's
    /// `irn_jcn_k[key] = k`), values zeroed, order and profile release
    /// copied from `other`, value release incremented. The matrix type is
    /// not copied (upstream).
    pub fn copy_profile(&mut self, other: &SparseMatrixAij) {
        if std::ptr::eq(self, other) {
            self.a.fill(0.0);
            self.bump_values(1);
            return;
        }
        self.revive();
        let same_rule = (self.matrix_type == MatrixType::Symmetric)
            == (other.matrix_type == MatrixType::Symmetric);
        if self.same_entries_as(other) {
            // Same coordinates in the same order with one key per entry:
            // the map rebuilt below would equal the current one.
            self.a.fill(0.0);
        } else {
            self.irn.clone_from(&other.irn);
            self.jcn.clone_from(&other.jcn);
            self.max_coord = other.max_coord;
            if same_rule && !other.released && other.profile.len() == other.a.len() {
                // Same key rule and one key per entry in `other`: its map
                // is the map the loop below would build.
                self.profile.clone_from(&other.profile);
            } else {
                self.profile.clear();
                for k in 0..self.irn.len() {
                    let key = self.key(self.irn[k], self.jcn[k]);
                    self.profile.insert(key, k);
                }
            }
            self.a.clear();
            self.a.resize(self.irn.len(), 0.0);
        }
        self.size = other.size;
        self.profile_release = other.profile_release;
        self.profile_stamp.invalidate();
        self.position_epoch = ProfileStamp::fresh();
        self.bump_values(1);
    }

    /// Copy the values of `other`, whose profile must have the same number
    /// of entries (upstream `copyContent`: debug assertion + `memcpy`; a
    /// different count is an error here and nothing is modified).
    pub fn copy_content(&mut self, other: &SparseMatrixAij) -> Result<()> {
        other.check_not_released("copy_content")?;
        self.revive();
        if self.a.len() != other.a.len() {
            return Err(Error::DimensionMismatch {
                context: "SparseMatrixAij::copy_content (profiles differ)",
                expected: self.a.len(),
                got: other.a.len(),
            });
        }
        self.a.copy_from_slice(&other.a);
        self.bump_values(1);
        Ok(())
    }

    /// Remove the profile (upstream `clearProfile`) and set the order to
    /// `size` (upstream resets it to 0 and lets the next `addToProfile`
    /// resize it to the system size). The effective symmetry (an agreed
    /// promotion) is kept: it is a property of the assembled operator.
    pub fn reset_profile(&mut self, size: GlobalIdx) {
        self.clear_profile();
        self.size = size;
    }

    /// Release the values and the lookup map, keeping the coordinates in
    /// their storage order (16 bytes per entry; the values take 8 and the
    /// map about 20 to 40). Port extension, for a matrix that will be
    /// assembled again but not read before. The value release is
    /// incremented; the profile, its release and the entry positions are
    /// unchanged, so the next assembly finds every entry at its former
    /// position and the matrix, its order and everything derived from it
    /// (`copy_profile`, solver input) come back bit for bit. The first
    /// mutation afterwards (`set`, `zero`, `add_*`, ...) revives the matrix
    /// with zero values and the map rebuilt from the coordinates. While it
    /// is released, [`SparseMatrixAij::nnz`] and the values are empty (its
    /// coordinates stay readable through [`SparseMatrixAij::coo`]), lookups
    /// find nothing and products or conversions that need the values
    /// return an error.
    pub fn release_values(&mut self) {
        if self.released {
            return;
        }
        self.a = Vec::new();
        self.profile = IntHashMap::default();
        self.scratch = Vec::new();
        self.released = true;
        self.bump_values(1);
    }

    /// `true` while the values are released
    /// ([`SparseMatrixAij::release_values`]).
    #[must_use]
    pub fn is_released(&self) -> bool {
        self.released
    }

    /// Undo [`SparseMatrixAij::release_values`]: zero values and the lookup
    /// map rebuilt from the coordinates in storage order (a later entry with
    /// the same key overwrites an earlier one, the rule every rebuild of the
    /// map follows; with one key per entry it is the map that was freed).
    fn revive(&mut self) {
        if !self.released {
            return;
        }
        self.released = false;
        self.a = vec![0.0; self.irn.len()];
        let mut map = IntHashMap::default();
        map.reserve(self.irn.len());
        for k in 0..self.irn.len() {
            map.insert(self.key(self.irn[k], self.jcn[k]), k);
        }
        self.profile = map;
    }

    fn check_not_released(&self, what: &str) -> Result<()> {
        if self.released {
            return Err(Error::invalid(format!(
                "{what}: the values of matrix '{}' are released (assemble it first)",
                self.id
            )));
        }
        Ok(())
    }

    /// [`SparseMatrixAij::clear_profile`] that also returns the storage of
    /// the coordinates, the values and the lookup map to the allocator
    /// (`clear_profile` keeps their capacity for a profile of the same
    /// size). Port extension, for a matrix no longer needed (released
    /// after a one-shot solve); the next assembly profiles it again.
    pub fn release_storage(&mut self) {
        self.clear_profile();
        self.released = false;
        self.irn = Vec::new();
        self.jcn = Vec::new();
        self.a = Vec::new();
        self.profile = IntHashMap::default();
        self.scratch = Vec::new();
    }

    /// Remove the profile (upstream `clearProfile`), keeping the order.
    /// The effective symmetry (an agreed promotion) is kept.
    pub fn clear_profile(&mut self) {
        self.released = false;
        self.max_coord = -1;
        self.position_epoch = ProfileStamp::fresh();
        self.irn.clear();
        self.jcn.clear();
        self.a.clear();
        self.profile.clear();
        self.bump_profile();
        self.bump_values(1);
    }

    /// Dirichlet handling (upstream `applyBoundary`,
    /// `sparse_matrix_aij.cc:52-80`): every entry touching a blocked
    /// equation is zeroed, except diagonal entries whose local equation is
    /// **owned** by this rank, which receive `block_val`. On a rank-partial
    /// matrix this puts the unit diagonal exactly once in the global
    /// operator. `blocked` is indexed by local equation.
    pub fn apply_boundary(
        &mut self,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()> {
        self.revive();
        if blocked.len() != layout.nb_local() {
            return Err(Error::DimensionMismatch {
                context: "SparseMatrixAij::apply_boundary",
                expected: layout.nb_local(),
                got: blocked.len(),
            });
        }
        // Resolve first: no partial update on an inconsistent layout.
        self.check_local_coordinates(layout)?;
        if layout.is_identity() {
            // Global == local; every coordinate was checked to be in
            // `0..nb_local` above.
            self.apply_boundary_with(|g| g as usize, layout, blocked, block_val);
        } else {
            self.apply_boundary_with(
                |g| layout.try_local(g).unwrap_or(usize::MAX),
                layout,
                blocked,
                block_val,
            );
        }
        self.bump_values(1);
        Ok(())
    }

    /// `y = beta·y + alpha·A·x` over local equations (upstream `matVecMul`,
    /// `sparse_matrix_aij.cc:170-195`), accumulated **in place** in
    /// upstream's operation order: `y *= beta`, then every entry in storage
    /// order adds `(alpha·a)·x[j]` to its row (and the mirror term for
    /// symmetric storage), then upstream's `y += tmp` with a zero `tmp` (one
    /// `+ 0.0` per entry, which turns a `-0.0` into `+0.0`).
    ///
    /// * Serial (`!reduction.is_distributed()`): that is the whole product,
    ///   bit-identical to the oracle.
    /// * Distributed: the matrix is rank-partial, so the rows of shared
    ///   equations hold partial sums. Upstream reduces a zero temporary
    ///   instead of them (UD-003, class B); the port reduces the partial
    ///   rows (`reduction`): a shared copy ([`EquationOwnership::Shared`])
    ///   starts from `0.0` instead of `beta·y`, the copies' partial sums are
    ///   added to the owner's row, `(beta·y + own partial) + copies`, and
    ///   the owner's total is copied back. Every row the reduction does not
    ///   touch keeps upstream's in-place bits: all rows on one rank
    ///   (a synchronizer after `Mesh::distribute` has nothing to reduce),
    ///   interior and pure-ghost rows on several ranks. The copies of a
    ///   shared row take the owner's value. The product is computed on a
    ///   copy of `y`, so `y` is unchanged if the reduction fails.
    pub fn mat_vec_mul(
        &self,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        self.check_not_released("mat_vec_mul")?;
        let n = layout.nb_local();
        if x.len() != n || y.len() != n {
            return Err(Error::DimensionMismatch {
                context: "SparseMatrixAij::mat_vec_mul",
                expected: n,
                got: if x.len() == n { y.len() } else { x.len() },
            });
        }
        // Resolve every coordinate first: no partial product on error.
        self.check_local_coordinates(layout)?;
        if layout.is_identity() {
            // Global == local; every coordinate was checked to be in
            // `0..nb_local` above.
            self.mat_vec_with(|g| g as usize, layout, reduction, x, y, alpha, beta)
        } else {
            self.mat_vec_with(
                |g| layout.try_local(g).unwrap_or(usize::MAX),
                layout,
                reduction,
                x,
                y,
                alpha,
                beta,
            )
        }
    }

    /// The error `layout.local(g)` gives for the first stored coordinate
    /// (entry order, row before column) that is not a local equation of
    /// `layout`, or `Ok`.
    fn check_local_coordinates(&self, layout: &EquationLayout) -> Result<()> {
        let bad = if layout.is_identity() {
            let n = layout.nb_local();
            if usize::try_from(self.max_coord).map_or(true, |m| m < n) {
                // every stored coordinate is in `0..=max_coord`
                return Ok(());
            }
            let out = |g: GlobalIdx| usize::try_from(g).map_or(true, |u| u >= n);
            if self.irn.iter().chain(&self.jcn).any(|&g| out(g)) {
                self.irn
                    .iter()
                    .zip(&self.jcn)
                    .flat_map(|(&i, &j)| [i, j])
                    .find(|&g| out(g))
            } else {
                None
            }
        } else {
            self.irn
                .iter()
                .zip(&self.jcn)
                .flat_map(|(&i, &j)| [i, j])
                .find(|&g| layout.try_local(g).is_none())
        };
        match bad {
            Some(g) => layout.local(g).map(drop),
            None => Ok(()),
        }
    }

    /// Body of [`SparseMatrixAij::apply_boundary`] after the coordinates
    /// were checked; `local` maps a stored global coordinate to its local
    /// equation.
    fn apply_boundary_with(
        &mut self,
        local: impl Fn(GlobalIdx) -> usize,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) {
        for (a, (&gi, &gj)) in self.a.iter_mut().zip(self.irn.iter().zip(&self.jcn)) {
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

    /// Body of [`SparseMatrixAij::mat_vec_mul`] after the coordinates were
    /// checked; `local` maps a stored global coordinate to its local
    /// equation. The operations and their order are the ones documented on
    /// `mat_vec_mul`.
    #[allow(clippy::too_many_arguments)] // the arguments of `mat_vec_mul` + the map
    fn mat_vec_with(
        &self,
        local: impl Fn(GlobalIdx) -> usize,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        let symmetric = self.matrix_type == MatrixType::Symmetric;
        let acc = |target: &mut [Real]| {
            for ((&gi, &gj), &a) in self.irn.iter().zip(&self.jcn).zip(&self.a) {
                let (i, j) = (local(gi), local(gj));
                target[i] += alpha * a * x[j];
                if symmetric && i != j {
                    target[j] += alpha * a * x[i];
                }
            }
        };
        if reduction.is_distributed() {
            // In place on a copy of `y`: shared copies start from
            // zero, every other row from `y·beta`; the reduction adds the
            // copies' partial sums to the owners and copies the totals back.
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
            acc(&mut tmp);
            reduction.reduce_and_synchronize(&mut tmp)?;
            // Upstream `y += tmp` with its (zero) reduced `tmp`.
            for (yi, t) in y.iter_mut().zip(tmp) {
                *yi = t + 0.0;
            }
        } else {
            for yi in y.iter_mut() {
                *yi *= beta;
            }
            acc(y);
            // Upstream `y += tmp` with the (serially untouched) zero `tmp`.
            for yi in y.iter_mut() {
                *yi += 0.0;
            }
        }
        Ok(())
    }

    /// 1-based `i32` coordinates for backends with 32-bit indices (MUMPS
    /// `MUMPS_INT`, Python `getIRN/getJCN`). Fails — before allocating the
    /// output — if any coordinate `+ 1` exceeds `i32::MAX` or `nnz` exceeds
    /// `max_nnz`.
    pub fn coo_one_based_i32(&self, max_nnz: u64) -> Result<(Vec<i32>, Vec<i32>)> {
        let nnz = u64::try_from(self.a.len()).unwrap_or(u64::MAX);
        if nnz > max_nnz {
            return Err(Error::InvalidArgument(format!(
                "matrix '{}' has {nnz} entries, backend limit is {max_nnz}",
                self.id
            )));
        }
        let to = akantu_core::to_one_based_i32;
        // Validate everything first (no partial output).
        let max_row = self.irn.iter().chain(&self.jcn).copied().max();
        if let Some(m) = max_row {
            to(m)?;
        }
        let irn = self.irn.iter().map(|&g| to(g)).collect::<Result<_>>()?;
        let jcn = self.jcn.iter().map(|&g| to(g)).collect::<Result<_>>()?;
        Ok((irn, jcn))
    }

    fn symmetry_word(&self) -> &'static str {
        if self.matrix_type == MatrixType::Symmetric {
            " symmetric"
        } else {
            " general"
        }
    }

    /// Text of `saveMatrix` (SPM-007) with the default output policy: 17
    /// significant digits, round-trip exact (UD-061). Header, `size size
    /// nnz`, then `i j a` per entry in storage order (1-based).
    #[must_use]
    pub fn matrix_market_text(&self) -> String {
        self.matrix_market_text_with(&matrix_market::MatrixMarketOptions::default())
    }

    /// Text of `saveMatrix` under `options`;
    /// [`matrix_market::MatrixMarketOptions::upstream_precision`] gives the
    /// bytes upstream writes (precision 15, lossy).
    #[must_use]
    pub fn matrix_market_text_with(&self, options: &matrix_market::MatrixMarketOptions) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(64 + 40 * self.a.len());
        out.push_str("%%MatrixMarket matrix coordinate real");
        out.push_str(self.symmetry_word());
        out.push('\n');
        let _ = writeln!(out, "{} {} {}", self.size, self.size, self.a.len());
        for ((&i, &j), &v) in self.irn.iter().zip(&self.jcn).zip(&self.a) {
            let _ = writeln!(out, "{} {} {}", i + 1, j + 1, options.format(v));
        }
        out
    }

    /// Text upstream `saveProfile` writes on a single process
    /// (byte-identical): pattern header, `size size nnz`, `i j 1` per entry.
    #[must_use]
    pub fn profile_text(&self) -> String {
        use std::fmt::Write as _;
        let mut out = String::with_capacity(64 + 24 * self.a.len());
        out.push_str("%%MatrixMarket matrix coordinate pattern");
        out.push_str(self.symmetry_word());
        out.push('\n');
        let _ = writeln!(out, "{} {} {}", self.size, self.size, self.a.len());
        for (&i, &j) in self.irn.iter().zip(&self.jcn) {
            let _ = writeln!(out, "{} {} 1", i + 1, j + 1);
        }
        out
    }

    /// Upstream `saveMatrix(filename)` on rank `rank` of `nb_proc`: the file
    /// name gets `.mtx` if it has no extension and `_rank-<rank>.mtx` with
    /// several processes; each rank writes its own (rank-partial) entries.
    /// Default output policy (17 digits, UD-061). Returns the path written.
    pub fn save_matrix_on(&self, path: &Path, rank: Rank, nb_proc: usize) -> Result<PathBuf> {
        self.save_matrix_on_with(
            path,
            rank,
            nb_proc,
            &matrix_market::MatrixMarketOptions::default(),
        )
    }

    /// [`SparseMatrixAij::save_matrix_on`] under `options` (e.g.
    /// [`matrix_market::MatrixMarketOptions::upstream_precision`]).
    pub fn save_matrix_on_with(
        &self,
        path: &Path,
        rank: Rank,
        nb_proc: usize,
        options: &matrix_market::MatrixMarketOptions,
    ) -> Result<PathBuf> {
        let file = matrix_market::matrix_file_name(path, rank, nb_proc);
        matrix_market::write_text(&file, &self.matrix_market_text_with(options))?;
        Ok(file)
    }

    /// Serial `saveMatrix(filename)` (default output policy); returns the
    /// path written.
    pub fn save_matrix(&self, path: &Path) -> Result<PathBuf> {
        self.save_matrix_on(path, 0, 1)
    }

    /// Serial `saveProfile(filename)` (the name is used as given); see
    /// [`SparseMatrixAij::save_profile_on`] for several processes.
    pub fn save_profile(&self, path: &Path) -> Result<()> {
        matrix_market::write_text(path, &self.profile_text())
    }

    /// Upstream `saveProfile(filename)` on rank `rank` of `nb_proc`
    /// (UD-180, category B; `sparse_matrix_aij.cc:85-120`). One process:
    /// the serial file under the name as given, byte-identical with
    /// upstream. Several processes: upstream lets every rank open the one
    /// file in truncation mode, rank 0 writes the header with its **local**
    /// entry count, then the ranks write their entries in turn from their
    /// own offset 0 (the barriers do not repair the offsets): a corrupt file.
    /// The port writes one valid Matrix Market pattern **per rank**, named as
    /// `saveMatrix` names its per-rank files (`_rank-<rank>.mtx` replaces
    /// the extension, [`matrix_market::profile_file_name`]), with the rank's
    /// own (rank-partial) entries and their count in the header; the union
    /// of the files is the global profile. No compat switch (it would
    /// reproduce a corrupt file). Returns the path written.
    pub fn save_profile_on(&self, path: &Path, rank: Rank, nb_proc: usize) -> Result<PathBuf> {
        let file = matrix_market::profile_file_name(path, rank, nb_proc);
        matrix_market::write_text(&file, &self.profile_text())?;
        Ok(file)
    }

    /// Build a matrix from a parsed Matrix Market file (entries added in file
    /// order with `add`, so the stored coordinates and their order equal the
    /// file's). Used for round trips and hand-written test systems.
    ///
    /// The public arrays of `mm` are validated first:
    /// equal lengths, every 1-based coordinate in `1..=order`; nothing is
    /// converted or inserted before that, and violations are errors.
    pub fn from_matrix_market(
        id: impl Into<String>,
        mm: &matrix_market::MatrixMarket,
    ) -> Result<Self> {
        if mm.nrows != mm.ncols {
            return Err(Error::invalid(format!(
                "Matrix Market matrix is {}×{}, a square matrix is required",
                mm.nrows, mm.ncols
            )));
        }
        if mm.cols.len() != mm.rows.len() || mm.values.len() != mm.rows.len() {
            return Err(Error::DimensionMismatch {
                context: "Matrix Market entries (rows, cols, values)",
                expected: mm.rows.len(),
                got: if mm.cols.len() == mm.rows.len() {
                    mm.values.len()
                } else {
                    mm.cols.len()
                },
            });
        }
        let ty = match mm.symmetry {
            matrix_market::MmSymmetry::Symmetric => MatrixType::Symmetric,
            matrix_market::MmSymmetry::General => MatrixType::Unsymmetric,
        };
        let n = GlobalIdx::try_from(mm.nrows)
            .map_err(|_| Error::invalid("Matrix Market order exceeds the index type"))?;
        for (k, &c) in mm.rows.iter().chain(&mm.cols).enumerate() {
            if !(1..=n).contains(&c) {
                return Err(Error::invalid(format!(
                    "Matrix Market entry {}: 1-based coordinate {c} outside 1..={n}",
                    k % mm.rows.len().max(1)
                )));
            }
        }
        let mut m = Self::new(id, n, ty);
        for ((&i, &j), &v) in mm.rows.iter().zip(&mm.cols).zip(&mm.values) {
            m.add(i - 1, j - 1, v)?;
        }
        Ok(m)
    }
}

impl SparseMatrix for SparseMatrixAij {
    fn id(&self) -> &str {
        &self.id
    }
    fn size(&self) -> GlobalIdx {
        self.size
    }
    fn matrix_type(&self) -> MatrixType {
        self.matrix_type
    }
    fn nb_non_zero(&self) -> usize {
        self.a.len()
    }
    fn release(&self) -> u64 {
        self.value_release
    }
    fn clear_profile(&mut self) {
        SparseMatrixAij::clear_profile(self);
    }
    fn set(&mut self, val: Real) {
        SparseMatrixAij::set(self, val);
    }
    fn add_to_profile(&mut self, i: GlobalIdx, j: GlobalIdx) -> Result<usize> {
        SparseMatrixAij::add_to_profile(self, i, j)
    }
    fn add(&mut self, i: GlobalIdx, j: GlobalIdx, value: Real) -> Result<()> {
        SparseMatrixAij::add(self, i, j, value)
    }
    fn mul(&mut self, alpha: Real) {
        SparseMatrixAij::mul(self, alpha);
    }
    /// Upstream `addMeTo`: into an AIJ matrix, the corrected
    /// [`SparseMatrixAij::add_matrix`]; any other type is silently ignored
    /// upstream (`sparse_matrix_aij.cc:262-267`), an error here.
    fn add_me_to(&self, b: &mut dyn SparseMatrix, alpha: Real) -> Result<()> {
        match b.as_aij_mut() {
            Some(b) => b.add_matrix(self, alpha),
            None => Err(Error::Unsupported(
                "adding an AIJ matrix to a non-AIJ matrix (upstream silently does nothing)".into(),
            )),
        }
    }
    fn mat_vec_mul(
        &self,
        layout: &EquationLayout,
        reduction: &dyn PartialSumReduction,
        x: &[Real],
        y: &mut [Real],
        alpha: Real,
        beta: Real,
    ) -> Result<()> {
        SparseMatrixAij::mat_vec_mul(self, layout, reduction, x, y, alpha, beta)
    }
    fn apply_boundary(
        &mut self,
        layout: &EquationLayout,
        blocked: &[bool],
        block_val: Real,
    ) -> Result<()> {
        SparseMatrixAij::apply_boundary(self, layout, blocked, block_val)
    }
    fn copy_profile(&mut self, other: &dyn SparseMatrix) -> Result<()> {
        let other = other
            .as_aij()
            .ok_or_else(|| Error::Unsupported("copyProfile from a non-AIJ matrix".into()))?;
        SparseMatrixAij::copy_profile(self, other);
        Ok(())
    }
    fn is_finite(&self) -> Result<bool> {
        Ok(SparseMatrixAij::is_finite(self))
    }
    fn value(&self, i: GlobalIdx, j: GlobalIdx) -> Result<Real> {
        Ok(self.get(i, j))
    }
    fn min(&self) -> Result<Real> {
        SparseMatrixAij::min(self)
    }
    fn save_profile(&self, path: &Path) -> Result<()> {
        SparseMatrixAij::save_profile(self, path)
    }
    fn save_matrix(&self, path: &Path) -> Result<PathBuf> {
        SparseMatrixAij::save_matrix(self, path)
    }
    fn as_aij(&self) -> Option<&SparseMatrixAij> {
        Some(self)
    }
    fn as_aij_mut(&mut self) -> Option<&mut SparseMatrixAij> {
        Some(self)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Selection {
    LocalUpper,
    GlobalUpper,
    All,
}

/// What the equations of the slots of a batch depend on, as the caller
/// sees it ([`BlockPositionCache::vouch_equations`]): compared for
/// equality only.
pub type EquationsKey = [u64; 5];

/// Most scatter plans a [`BlockPositionCache`] keeps (one per element
/// filter assembled into it, e.g. one per material of an element type).
const MAX_PLANS: usize = 16;

/// Marker of a pair `add_values` skips, in [`BlockPositionCache`].
const SKIP: u32 = u32::MAX;

/// Entry positions of the elemental blocks of a repeated assembly into one
/// matrix, per slot (element of the element loop), for
/// [`SparseMatrixAij::add_values_cached`].
///
/// A stored slot holds the block's equations (4 bytes each, rows only when
/// the rows equal the columns) and one 4-byte position per pair
/// `add_values` visits: `n (n + 1) / 2` for a symmetric `n × n` block into a
/// symmetric matrix, `n²` otherwise. For example 360 bytes per `Tetrahedron4`
/// element of a 3-D solid (12 equations, 78 pairs), 1.3 kB per
/// `Hexahedron8` (24 equations, 300 pairs), so 360 MB and 1.3 GB per 10⁶
/// elements. Nothing is stored before the second assembly round with
/// unchanged entry positions (entries appended to the profile do not
/// count as a change), and the storage is released when they change
/// (`clear_profile`, `copy_profile`, promotion, another matrix object).
#[derive(Clone, Debug, Default)]
pub struct BlockPositionCache {
    /// Position epoch of the matrix the positions belong to (0: none).
    stamp: u64,
    /// Assembly rounds begun with this epoch after the first one.
    rounds: u32,
    /// Matrix order, selection mode and block shape of the stored slots.
    size: GlobalIdx,
    mode: Option<Selection>,
    shape: (usize, usize),
    /// Rows only (`true`) or rows then columns per slot.
    rows_only: bool,
    /// Positions per slot.
    stride: usize,
    filled: Vec<bool>,
    /// Equations per slot: `g` for `0 <= g < size`, `u32::MAX` for an
    /// equation `add_values` skips (`>= size`).
    equations: Vec<u32>,
    positions: Vec<u32>,
    /// Per slot: smallest and largest stored position and the number of
    /// stored (not skipped) pairs (the threaded scatter's spans).
    spans: Vec<(u32, u32)>,
    counts: Vec<u32>,
    /// The threaded scatter's plans, one per batch of slots (one element
    /// filter, e.g. one material of the type; at most [`MAX_PLANS`]),
    /// dropped with the stored positions, and a plan whenever one of its
    /// slots is stored.
    plans: Vec<threaded::ScatterPlan>,
    /// The caller's statement for the next batch
    /// ([`BlockPositionCache::vouch_equations`]).
    equations_key: Option<EquationsKey>,
    key: Vec<u32>,
    scratch: Vec<u32>,
    /// Blocks resolved and stored since the cache was created (kept
    /// across resets; diagnostic).
    resolves: u64,
}

impl BlockPositionCache {
    /// Empty cache.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Start an assembly round into `matrix` (call before the element loop):
    /// moved entry positions release the stored ones; positions are
    /// stored from the second round with unchanged positions on.
    pub fn begin(&mut self, matrix: &SparseMatrixAij) {
        let stamp = matrix.position_epoch.0;
        if stamp == self.stamp {
            self.rounds = self.rounds.saturating_add(1);
        } else {
            *self = Self {
                stamp,
                resolves: self.resolves,
                ..Self::default()
            };
        }
    }

    /// `true` when [`BlockPositionCache::begin`] with `matrix` continues
    /// this cache (the entry positions it was filled for are still
    /// `matrix`'s), so that the round can take positions from it.
    #[must_use]
    pub fn continues(&self, matrix: &SparseMatrixAij) -> bool {
        self.stamp != 0 && self.stamp == matrix.position_epoch.0
    }

    /// State, for the next threaded batch only
    /// ([`SparseMatrixAij::add_values_cached_batch`]), that the equations
    /// of every slot are those they had when the batch's plan was built
    /// with the same `key`: the batch then skips checking them. The caller
    /// derives `key` from everything the equations depend on (connectivity
    /// revision, DOF numbering) and must not vouch when it cannot know (a
    /// connectivity foreign code may write). Without it, or with another
    /// key, the batch checks every block as before.
    pub fn vouch_equations(&mut self, key: EquationsKey) {
        self.equations_key = Some(key);
    }

    /// Keep `plan`, replacing the plan of the same slots (the oldest plan
    /// goes when [`MAX_PLANS`] are kept).
    fn set_plan(&mut self, plan: threaded::ScatterPlan) {
        self.plans.retain(|p| p.slots != plan.slots);
        if self.plans.len() >= MAX_PLANS {
            self.plans.remove(0);
        }
        self.plans.push(plan);
    }

    /// The plan of a batch of `slots` into `matrix` (square blocks of
    /// `nb_eq` equations of type `values_type`) built with the equations
    /// `key` the caller vouches for, for the current entry positions,
    /// profile size and split of `exec`, as a handle: the batch can then be
    /// added chunk by chunk ([`SparseMatrixAij::add_values_planned_chunk`]
    /// with this handle, valid until the cache changes).
    /// Asked before the round is begun: [`BlockPositionCache::begin`] then
    /// continues the cache (same entry positions) and keeps the plan.
    #[must_use]
    pub fn plan_ready(
        &self,
        exec: &akantu_core::exec::Exec,
        matrix: &SparseMatrixAij,
        slots: &[usize],
        nb_eq: usize,
        values_type: MatrixType,
        key: Option<EquationsKey>,
    ) -> Option<usize> {
        let mode = match (matrix.matrix_type, values_type) {
            (MatrixType::Symmetric, MatrixType::Symmetric) => Selection::LocalUpper,
            (MatrixType::Symmetric, _) => Selection::GlobalUpper,
            _ => Selection::All,
        };
        let nnz = matrix.a.len();
        if key.is_none()
            || matrix.released
            || self.stamp != matrix.position_epoch.0
            || self.mode != Some(mode)
            || self.shape != (nb_eq, nb_eq)
            || self.size != matrix.size
        {
            return None;
        }
        let parts = threaded::scatter_parts(exec, slots.len(), nnz);
        self.plans.iter().position(|p| {
            p.equations_key == key && p.nnz == nnz && p.parts == parts && p.slots == slots
        })
    }

    /// After a serial loop of `add_values_cached` that added every block
    /// of `slots` to `matrix` without error (each block's equations
    /// checked against, or stored into, its slot), build the threaded
    /// scatter's plan for the next batch with the equations `key` the
    /// caller vouches for (nothing when a slot is not stored, e.g. in the
    /// first rounds, or without a key). Returns whether a plan was built.
    pub fn plan_after_serial_round(
        &mut self,
        exec: &akantu_core::exec::Exec,
        matrix: &SparseMatrixAij,
        slots: &[usize],
        key: Option<EquationsKey>,
    ) -> bool {
        let nnz = matrix.a.len();
        if key.is_none()
            || slots.is_empty()
            || self.stamp != matrix.position_epoch.0
            || !self.rows_only
            || !u32::try_from(slots.len()).is_ok_and(|n| n < (1 << 31))
            || slots.iter().any(|&s| self.filled.get(s) != Some(&true))
            || threaded::has_repeated_slot(slots)
        {
            return false;
        }
        let parts = threaded::scatter_parts(exec, slots.len(), nnz);
        let plan = threaded::ScatterPlan::new(self, slots, nnz, parts, key);
        self.set_plan(plan);
        true
    }

    /// Number of blocks whose positions were looked up and stored since
    /// the cache was created (diagnostic: a round that finds every block
    /// stored adds none).
    #[must_use]
    pub fn resolves(&self) -> u64 {
        self.resolves
    }

    /// Bytes held by the stored slots (capacity of the slot arrays).
    #[must_use]
    pub fn memory_bytes(&self) -> usize {
        self.filled.capacity()
            + 4 * (self.equations.capacity() + self.positions.capacity() + self.counts.capacity())
            + 8 * self.spans.capacity()
            + self
                .plans
                .iter()
                .map(threaded::ScatterPlan::memory_bytes)
                .sum::<usize>()
    }

    /// Set the order, mode and block shape; stored slots of another
    /// configuration are dropped.
    fn configure(&mut self, mode: Selection, nr: usize, nc: usize, size: GlobalIdx) {
        if self.mode == Some(mode) && self.shape == (nr, nc) && self.size == size {
            return;
        }
        let stamp = self.stamp;
        let rounds = self.rounds;
        *self = Self {
            stamp,
            rounds,
            resolves: self.resolves,
            size,
            mode: Some(mode),
            shape: (nr, nc),
            rows_only: true,
            stride: if mode == Selection::LocalUpper {
                (0..nr).map(|i| nc.saturating_sub(i)).sum()
            } else {
                nr * nc
            },
            ..Self::default()
        };
    }

    /// Encode the equations of a block into `self.key` (`false` if an
    /// equation is negative: such a block is never cached).
    fn set_key(&mut self, rows: &[GlobalIdx], cols: &[GlobalIdx], size: GlobalIdx) -> bool {
        let enc = |g: GlobalIdx| -> Option<u32> {
            if g < 0 {
                None
            } else if g >= size {
                Some(u32::MAX)
            } else {
                u32::try_from(g).ok()
            }
        };
        self.key.clear();
        for &g in rows {
            let Some(e) = enc(g) else { return false };
            self.key.push(e);
        }
        if rows != cols {
            if self.rows_only && !self.filled.is_empty() {
                // the stored slots hold rows only: start again with both
                self.filled.clear();
                self.equations.clear();
                self.positions.clear();
                self.spans.clear();
                self.counts.clear();
                self.plans.clear();
            }
            self.rows_only = false;
        }
        if !self.rows_only {
            for &g in cols {
                let Some(e) = enc(g) else { return false };
                self.key.push(e);
            }
        }
        true
    }

    fn eq_len(&self) -> usize {
        if self.rows_only {
            self.shape.0
        } else {
            self.shape.0 + self.shape.1
        }
    }

    fn matches(&self, slot: usize) -> bool {
        let w = self.eq_len();
        self.key.len() == w
            && self.filled.get(slot) == Some(&true)
            && self.equations[slot * w..(slot + 1) * w] == *self.key
    }

    fn store(&mut self, slot: usize, positions: &[u32]) {
        let key = std::mem::take(&mut self.key);
        self.store_with_key(slot, &key, positions);
        self.key = key;
    }

    /// [`BlockPositionCache::store`] with the encoded equations `key`.
    fn store_with_key(&mut self, slot: usize, key: &[u32], positions: &[u32]) {
        let w = self.eq_len();
        debug_assert_eq!(key.len(), w);
        debug_assert_eq!(positions.len(), self.stride);
        if slot >= self.filled.len() {
            self.filled.resize(slot + 1, false);
            self.equations.resize((slot + 1) * w, 0);
            self.positions.resize((slot + 1) * self.stride, SKIP);
            self.spans.resize(slot + 1, (u32::MAX, 0));
            self.counts.resize(slot + 1, 0);
        }
        self.equations[slot * w..(slot + 1) * w].copy_from_slice(key);
        self.positions[slot * self.stride..(slot + 1) * self.stride].copy_from_slice(positions);
        let (span, n) = threaded::span_of(positions);
        self.spans[slot] = span;
        self.counts[slot] = n;
        self.filled[slot] = true;
        // the plans that list this slot hold its former span
        self.plans.retain(|p| !p.contains(slot));
    }
}

#[path = "sparse_threaded.rs"]
mod threaded;

#[cfg(test)]
#[path = "sparse_perf_tests.rs"]
mod perf_tests;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::layout::{EquationOwnership, NoReduction};

    /// A `SparseMatrix` that is not an AIJ matrix (test double counting the
    /// writes it receives).
    #[derive(Debug, Default)]
    struct OtherMatrix {
        writes: usize,
    }

    impl SparseMatrix for OtherMatrix {
        fn id(&self) -> &str {
            "other"
        }
        fn size(&self) -> GlobalIdx {
            2
        }
        fn matrix_type(&self) -> MatrixType {
            MatrixType::Unsymmetric
        }
        fn nb_non_zero(&self) -> usize {
            0
        }
        fn release(&self) -> u64 {
            0
        }
        fn clear_profile(&mut self) {}
        fn set(&mut self, _val: Real) {
            self.writes += 1;
        }
        fn add_to_profile(&mut self, _i: GlobalIdx, _j: GlobalIdx) -> Result<usize> {
            self.writes += 1;
            Ok(0)
        }
        fn add(&mut self, _i: GlobalIdx, _j: GlobalIdx, _value: Real) -> Result<()> {
            self.writes += 1;
            Ok(())
        }
        fn mul(&mut self, _alpha: Real) {
            self.writes += 1;
        }
        fn add_me_to(&self, _b: &mut dyn SparseMatrix, _alpha: Real) -> Result<()> {
            Err(Error::Unsupported("test double".into()))
        }
        fn mat_vec_mul(
            &self,
            _layout: &EquationLayout,
            _reduction: &dyn PartialSumReduction,
            _x: &[Real],
            _y: &mut [Real],
            _alpha: Real,
            _beta: Real,
        ) -> Result<()> {
            Ok(())
        }
        fn apply_boundary(
            &mut self,
            _layout: &EquationLayout,
            _blocked: &[bool],
            _block_val: Real,
        ) -> Result<()> {
            Ok(())
        }
        fn copy_profile(&mut self, _other: &dyn SparseMatrix) -> Result<()> {
            Ok(())
        }
        fn min(&self) -> Result<Real> {
            Ok(0.0)
        }
    }

    /// UD-438: upstream
    /// `SparseMatrixAIJ::addMeTo` (`sparse_matrix_aij.cc:266-274`) silently
    /// does nothing for a non-AIJ target; the port refuses with
    /// `Error::Unsupported` and writes nothing. `copyProfile` from a non-AIJ
    /// matrix is refused likewise.
    #[test]
    fn ud438_adding_into_a_non_aij_matrix_is_refused() {
        let mut a = SparseMatrixAij::new("A", 2, MatrixType::Unsymmetric);
        a.add(0, 0, 1.0).unwrap();
        a.add(1, 0, 2.0).unwrap();
        let mut other = OtherMatrix::default();
        let e = SparseMatrix::add_me_to(&a, &mut other, 3.0).unwrap_err();
        assert!(
            matches!(e, Error::Unsupported(ref m) if m.contains("non-AIJ")),
            "{e:?}"
        );
        assert_eq!(other.writes, 0);
        // the same through `B.add(A, alpha)` (double dispatch)
        assert!(matches!(
            other.add_matrix(&a, 3.0),
            Err(Error::Unsupported(_))
        ));
        assert_eq!(other.writes, 0);
        let (p, v) = (a.profile_release(), a.value_release());
        assert!(matches!(
            SparseMatrix::copy_profile(&mut a, &other),
            Err(Error::Unsupported(_))
        ));
        assert_eq!((a.profile_release(), a.value_release()), (p, v));
        assert_eq!(a.nnz(), 2);
    }

    /// The profile identity is process-unique — two
    /// matrices with equal release counters, a clone and a copied profile
    /// each have their own; it changes with the profile only.
    #[test]
    fn profile_identity_is_unique_per_object_and_profile() {
        let build = |pairs: &[(GlobalIdx, GlobalIdx)]| {
            let mut m = SparseMatrixAij::new("m", 3, MatrixType::Unsymmetric);
            for &(i, j) in pairs {
                m.add(i, j, 1.0).expect("entry");
            }
            m
        };
        let mut a = build(&[(0, 1), (1, 2)]);
        let b = build(&[(1, 0), (2, 1)]);
        assert_eq!(a.profile_release(), b.profile_release());
        assert_ne!(a.profile_identity(), b.profile_identity());
        assert_ne!(a.profile_identity(), a.clone().profile_identity());
        let id = a.profile_identity();
        a.add(0, 1, 2.0).expect("existing entry");
        assert_eq!(a.profile_identity(), id, "values only");
        a.add(2, 2, 1.0).expect("new entry");
        assert_ne!(a.profile_identity(), id, "profile grew");
        let mut c = build(&[(0, 0)]);
        let before = c.profile_identity();
        c.copy_profile(&b);
        assert_eq!(c.profile_release(), b.profile_release());
        assert_ne!(c.profile_identity(), before);
        assert_ne!(c.profile_identity(), b.profile_identity());
    }

    #[test]
    fn construction_and_releases() {
        let mut m = SparseMatrixAij::new("K", 4, MatrixType::Symmetric);
        assert_eq!((m.size(), m.nnz()), (4, 0));
        let (p, v) = (m.profile_release(), m.value_release());
        assert_eq!((p, v), (1, 1));
        m.zero();
        assert_eq!(m.value_release(), v + 1);
        m.clear_profile();
        assert_eq!(m.profile_release(), p + 1);
        assert_eq!(m.matrix_type(), MatrixType::Symmetric);
        assert_eq!(m.coo().0.len(), 0);
    }

    /// Release increments follow upstream call by call.
    #[test]
    fn release_counts_match_upstream() {
        let mut m = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
        m.add_to_profile(0, 0).unwrap(); // new: p 2, v 2
        m.add_to_profile(0, 0).unwrap(); // existing: no change
        assert_eq!((m.profile_release(), m.value_release()), (2, 2));
        m.add(1, 0, 1.0).unwrap(); // new (p 3, v 3) then value (v 4)
        assert_eq!((m.profile_release(), m.value_release()), (3, 4));
        m.add_block_to_profile(&[0, 1], &[0, 1]).unwrap(); // (1,1) new: p 4, v 5
        // symmetric block: local upper (0,0) (0,1) (1,1): 3 touched entries
        m.add_values(&[0, 1], &[0, 1], &[1.0; 4], MatrixType::Symmetric)
            .unwrap();
        assert_eq!((m.profile_release(), m.value_release()), (4, 8));
        *m.get_mut(1, 1).unwrap() += 1.0; // v 9
        assert!(m.get_mut(2, 2).is_err());
        m.mul(2.0); // v 10
        let l = EquationLayout::serial(3).unwrap();
        m.apply_boundary(&l, &[false; 3], 1.0).unwrap(); // v 11
        assert_eq!((m.profile_release(), m.value_release()), (4, 11));
        let mut c = SparseMatrixAij::new("C", 3, MatrixType::Symmetric);
        c.copy_profile(&m);
        assert_eq!((c.profile_release(), c.value_release()), (4, 2));
        c.copy_content(&m).unwrap();
        assert_eq!(c.value_release(), 3);
        assert_eq!(c.coo().2, m.coo().2);
    }

    /// A failing unsymmetric block into a symmetric matrix leaves
    /// type, entries and releases unchanged (no promotion on error).
    #[test]
    fn promotion_is_atomic_on_error() {
        let mut m = SparseMatrixAij::new("K", 2, MatrixType::Symmetric);
        m.add(0, 0, 1.0).unwrap();
        let before = (m.profile_release(), m.value_release(), m.nnz());
        let err = m.add_values(&[0, 1], &[0, 1], &[1.0; 4], MatrixType::Unsymmetric);
        assert!(err.is_err());
        assert_eq!(m.matrix_type(), MatrixType::Symmetric);
        assert!(!m.was_promoted());
        assert_eq!((m.profile_release(), m.value_release(), m.nnz()), before);
        // A profiled block (either triangle stored) promotes and adds all.
        m.add_block_to_profile(&[1, 0], &[1, 0]).unwrap();
        m.add_values(
            &[0, 1],
            &[0, 1],
            &[1.0, 2.0, 3.0, 4.0],
            MatrixType::Unsymmetric,
        )
        .unwrap();
        assert!(m.was_promoted());
        assert_eq!(
            (m.get(0, 0), m.get(1, 0), m.get(0, 1), m.get(1, 1)),
            (2.0, 2.0, 3.0, 4.0)
        );
        m.reset_profile(5);
        assert_eq!(
            (m.size(), m.nnz(), m.matrix_type()),
            (5, 0, MatrixType::Unsymmetric)
        );
    }

    #[test]
    fn owner_only_dirichlet_diagonal() {
        // Rank-local view of a 3-equation system: eq 0 owned, eq 1 shared
        // (owned elsewhere), eq 2 owned.
        let mut layout = EquationLayout::new();
        layout
            .append_block(
                &[
                    EquationOwnership::Owned,
                    EquationOwnership::Shared,
                    EquationOwnership::Owned,
                ],
                0,
                3,
                |_| Some(2),
            )
            .unwrap();
        // global numbers: local 0 → 0, local 1 → 2 (shared), local 2 → 1
        let mut m = SparseMatrixAij::new("K", 3, MatrixType::Unsymmetric);
        m.add_block_to_profile(&[0, 2, 1], &[0, 2, 1]).unwrap();
        m.add_values(&[0, 2, 1], &[0, 2, 1], &[1.0; 9], MatrixType::Unsymmetric)
            .unwrap();
        m.apply_boundary(&layout, &[false, true, false], 7.0)
            .unwrap();
        // The diagonal of the shared (non-owned) blocked equation is zero:
        // its owner contributes the block value.
        assert_eq!(m.get(2, 2), 0.0);
        assert_eq!(m.get(0, 2), 0.0);
        assert_eq!(m.get(0, 0), 1.0);
        let owned = EquationLayout::serial(3).unwrap();
        let mut m2 = SparseMatrixAij::new("K", 3, MatrixType::Unsymmetric);
        m2.add(1, 1, 3.0).unwrap();
        m2.apply_boundary(&owned, &[false, true, false], 7.0)
            .unwrap();
        assert_eq!(m2.get(1, 1), 7.0);
    }

    #[test]
    fn serial_mat_vec_follows_storage_order() {
        let l = EquationLayout::serial(2).unwrap();
        let mut m = SparseMatrixAij::new("K", 2, MatrixType::Symmetric);
        m.add(1, 0, 2.0).unwrap(); // stored as (1, 0): first-inserted coordinates
        m.add(0, 0, 1.0).unwrap();
        m.add(1, 1, 4.0).unwrap();
        let mut y = [1.0, 1.0];
        m.mat_vec_mul(&l, &NoReduction, &[1.0, 2.0], &mut y, 1.0, 0.5)
            .unwrap();
        // y0 = 0.5 + 2*2 + 1*1, y1 = 0.5 + 2*1 + 4*2
        assert_eq!(y, [5.5, 10.5]);
        // Upstream's final `y += tmp` (zero) turns -0.0 into +0.0.
        let mut z = SparseMatrixAij::new("Z", 2, MatrixType::Unsymmetric);
        z.add(0, 0, -1.0).unwrap();
        let mut y = [-0.0, -0.0];
        z.mat_vec_mul(&l, &NoReduction, &[0.0, 0.0], &mut y, 1.0, 1.0)
            .unwrap();
        assert!(y.iter().all(|v| v.to_bits() == 0));
        assert!(
            z.mat_vec_mul(&l, &NoReduction, &[0.0], &mut y, 1.0, 1.0)
                .is_err()
        );
    }

    /// One rank's view of a distributed reduction: records the vector it
    /// receives, then adds `remote` (the other ranks' partial sums) to the
    /// owned rows and overwrites the shared copies with `owner_totals`
    /// (what the owners send back). Empty maps: a synchronizer with nothing
    /// to reduce (one rank after `Mesh::distribute`).
    #[derive(Default)]
    struct RankView {
        received: std::cell::RefCell<Vec<Vec<Real>>>,
        remote: Vec<(usize, Real)>,
        owner_totals: Vec<(usize, Real)>,
        fail: bool,
    }

    impl PartialSumReduction for RankView {
        fn is_distributed(&self) -> bool {
            true
        }
        fn reduce_and_synchronize(&self, local: &mut [f64]) -> Result<()> {
            self.received.borrow_mut().push(local.to_vec());
            if self.fail {
                return Err(Error::Communication("injected".into()));
            }
            for &(l, v) in &self.remote {
                local[l] += v;
            }
            for &(l, v) in &self.owner_totals {
                local[l] = v;
            }
            Ok(())
        }
    }

    fn bits(v: &[Real]) -> Vec<u64> {
        v.iter().map(|x| x.to_bits()).collect()
    }

    /// A distributed product without shared rows
    /// (one rank after `Mesh::distribute`) keeps upstream's in-place order
    /// bit for bit. Row 0 is `1 + 2⁻⁵³ + 2⁻⁵³`: in place it rounds to `1`
    /// twice (ties to even), while the former zeroed temporary gave
    /// `1 + (2⁻⁵³ + 2⁻⁵³) = 1 + 2⁻⁵²`.
    #[test]
    fn b016_distributed_product_without_shared_rows_is_upstream_in_place() {
        let l = EquationLayout::serial(3).unwrap();
        let h = 2f64.powi(-53);
        let mut m = SparseMatrixAij::new("M", 3, MatrixType::Unsymmetric);
        m.add(0, 1, h).unwrap();
        m.add(0, 2, h).unwrap();
        m.add(1, 1, 3.0).unwrap();
        let x = [0.0, 1.0, 1.0];
        let y0 = [1.0, 0.25, -0.0];
        let mut up = y0;
        m.mat_vec_mul(&l, &NoReduction, &x, &mut up, 1.0, 1.0)
            .unwrap();
        assert_eq!(bits(&up), bits(&[1.0, 3.25, 0.0]), "upstream order");
        let red = RankView::default();
        let mut y = y0;
        m.mat_vec_mul(&l, &red, &x, &mut y, 1.0, 1.0).unwrap();
        assert_eq!(red.received.borrow().len(), 1, "the reduction still runs");
        assert_eq!(bits(&y), bits(&up));
    }

    /// `b016_*` on several ranks: rows the reduction does not touch keep
    /// upstream's in-place bits; a shared copy contributes its partial sum
    /// only (not `beta·y`, which the owner counts); the owner's row is
    /// `(beta·y + own partial) + remote partials`; the copy takes the
    /// owner's total; a failed reduction leaves `y` unchanged.
    #[test]
    fn b016_distributed_product_reduces_partial_rows_in_place() {
        use EquationOwnership::{Owned, PureGhost, Shared};
        // local 0, 1 owned (g0, g1), local 2 a copy of g2 (owned elsewhere),
        // local 3 a pure ghost.
        let mut l = EquationLayout::new();
        l.append_block(&[Owned, Owned, Shared, PureGhost], 0, 3, |i| {
            (i == 2).then_some(2)
        })
        .unwrap();
        l.validate().unwrap();
        let h = 2f64.powi(-53);
        let mut m = SparseMatrixAij::new("M", 3, MatrixType::Unsymmetric);
        m.add(0, 1, h).unwrap(); // interior row 0: 1 + 2⁻⁵³ + 2⁻⁵³
        m.add(0, 2, h).unwrap();
        m.add(1, 1, h).unwrap(); // owned row 1, shared with another rank
        m.add(2, 0, 2.0).unwrap(); // the copy's partial sum: 2·x0
        let x = [1.0, 1.0, 1.0, 0.0];
        let y0 = [1.0, 1.0, 7.0, -0.0];
        let beta = 1.0;
        let mut up = y0;
        m.mat_vec_mul(&l, &NoReduction, &x, &mut up, 1.0, beta)
            .unwrap();
        let red = RankView {
            remote: vec![(1, h)],
            owner_totals: vec![(2, 7.5)],
            ..RankView::default()
        };
        let mut y = y0;
        m.mat_vec_mul(&l, &red, &x, &mut y, 1.0, beta).unwrap();
        let sent = red.received.borrow()[0].clone();
        assert_eq!(sent[2], 2.0, "a copy sends its partial sum only");
        assert_eq!(sent[1].to_bits(), 1.0f64.to_bits(), "(1 + 2⁻⁵³) in place");
        assert_eq!(
            y[0].to_bits(),
            up[0].to_bits(),
            "interior row: upstream bits"
        );
        assert_eq!(y[0], 1.0);
        assert_eq!(y[1], (1.0 + h) + h, "(beta·y + own) + remote");
        assert_eq!(y[2], 7.5, "the copy takes the owner's total");
        assert_eq!(y[3].to_bits(), up[3].to_bits(), "pure ghost: upstream bits");
        assert_eq!(y[3].to_bits(), 0, "-0.0 + 0.0");
        let failing = RankView {
            fail: true,
            ..RankView::default()
        };
        let mut y = y0;
        assert!(m.mat_vec_mul(&l, &failing, &x, &mut y, 1.0, beta).is_err());
        assert_eq!(bits(&y), bits(&y0), "unchanged after a failed reduction");
    }

    #[test]
    fn matrix_sums_and_symmetric_mirror_fix() {
        let mut m = SparseMatrixAij::new("M", 2, MatrixType::Symmetric);
        m.add(0, 1, 3.0).unwrap();
        m.add(1, 1, 5.0).unwrap();
        // Upstream: only the stored triangle reaches an unsymmetric J.
        let mut j_up = SparseMatrixAij::new("J", 2, MatrixType::Unsymmetric);
        j_up.add_matrix_upstream(&m, 2.0).unwrap();
        assert_eq!(
            (j_up.get(0, 1), j_up.get(1, 0), j_up.get(1, 1)),
            (6.0, 0.0, 10.0)
        );
        let mut j = SparseMatrixAij::new("J", 2, MatrixType::Unsymmetric);
        j.add_matrix(&m, 2.0).unwrap();
        assert_eq!((j.get(0, 1), j.get(1, 0), j.get(1, 1)), (6.0, 6.0, 10.0));
        // Symmetric into symmetric: both methods agree.
        let mut s1 = SparseMatrixAij::new("S", 2, MatrixType::Symmetric);
        let mut s2 = s1.clone();
        s1.add_matrix(&m, 0.5).unwrap();
        s2.add_matrix_upstream(&m, 0.5).unwrap();
        assert_eq!(s1.coo(), s2.coo());
        // Trait double dispatch.
        let mut t = SparseMatrixAij::new("T", 2, MatrixType::Unsymmetric);
        SparseMatrix::add_matrix(&mut t, &m, 1.0).unwrap();
        assert_eq!(t.get(1, 0), 3.0);
        assert_eq!(m.min().unwrap(), 3.0);
        assert!(
            SparseMatrixAij::new("E", 1, MatrixType::Symmetric)
                .min()
                .is_err()
        );
    }

    #[test]
    fn copy_profile_rebuilds_keys_with_own_rule() {
        let mut u = SparseMatrixAij::new("U", 2, MatrixType::Unsymmetric);
        u.add(0, 1, 1.0).unwrap();
        u.add(1, 0, 2.0).unwrap();
        let mut s = SparseMatrixAij::new("S", 2, MatrixType::Symmetric);
        s.copy_profile(&u);
        assert_eq!(s.nnz(), 2);
        // Both entries share the canonical key (0, 1): the later one wins.
        assert_eq!(s.position(0, 1), Some(1));
        assert!(
            s.copy_content(&SparseMatrixAij::new("X", 2, MatrixType::Symmetric))
                .is_err()
        );
    }

    /// UD-179: upstream's copy constructor loses the lookup map, so `add`
    /// on the copy appends duplicate coordinates; the port's copy finds the
    /// existing entries (fresh releases, as upstream).
    #[test]
    fn copy_rebuilds_lookup_map() {
        let mut a = SparseMatrixAij::new("A", 2, MatrixType::Symmetric);
        a.add(0, 1, 1.0).unwrap();
        a.add(1, 1, 2.0).unwrap();
        let mut c = SparseMatrixAij::copy_with_id(&a, "C");
        assert_eq!((c.profile_release(), c.value_release()), (1, 1));
        c.add(1, 0, 0.5).unwrap();
        c.add(1, 1, 1.0).unwrap();
        assert_eq!(c.nnz(), 2, "no duplicate coordinates");
        assert_eq!((c.get(0, 1), c.get(1, 1)), (1.5, 3.0));
        assert_eq!(c.position(1, 0), Some(0));
    }

    #[test]
    fn invalid_coordinates_are_errors() {
        let mut m = SparseMatrixAij::new("K", 2, MatrixType::Unsymmetric);
        assert!(m.add(-1, 0, 1.0).is_err());
        assert!(m.add(0, GlobalIdx::MAX, 1.0).is_err());
        assert!(m.add_block_to_profile(&[0, -5], &[0]).is_err());
        assert_eq!(m.nnz(), 0);
        assert!(
            m.add_values(&[0], &[0, 1], &[1.0], MatrixType::Unsymmetric)
                .is_err()
        );
    }
}
