// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/solver/sparse_solver.hh,
// sparse_solver.cc, sparse_solver_eigen.hh, sparse_solver_eigen.cc,
// sparse_solver_mumps.hh (parallel method, symmetry), sparse_solver_petsc.hh,
// src/model/common/model_solver.cc:325-339 (solver selection).

//! Sparse direct solver backends (SOLV-001/004/006, decision records D-014
//! as amended by D-040).
//!
//! Lifecycle mirrors upstream: `analysis` (symbolic, redone when the profile
//! release **or the effective symmetry** changes), `factorize` (numeric,
//! redone when the value release changes), `solve`. Backends report
//! failures as values, never panic:
//!
//! * singular matrices: [`Error::LinearSolver`] with the message of
//!   upstream's `debug::SingularMatrixException`, "Solver encountered
//!   singular matrix", followed by the backend's diagnostic
//!   ([`singular_matrix`]);
//! * **non-finite systems** (UD-183): a matrix or
//!   right-hand side holding `±inf`/`NaN` is refused with
//!   [`Error::Numerical`] before any arithmetic; finite inputs whose
//!   factors or solution leave the binary64 range (overflow of a pivot
//!   reciprocal, of an elimination update or of the solution) give an
//!   *arithmetic range failure*, also [`Error::Numerical`] — distinct from
//!   singularity. Upstream reports success and returns `NaN`/`inf`;
//! * a failed `analysis`/`factorize` invalidates every later stage (a solve
//!   after a failed refactorisation is refused, never answered with stale
//!   factors);
//! * capacity limits ([`BackendCapabilities`]) are checked before any
//!   conversion or allocation ([`BackendCapabilities::check_local`],
//!   [`BackendCapabilities::check_centralized`]).
//!
//! | `SparseSolverType` | Backend | Matrix input | RHS / solution | Feature |
//! |---|---|---|---|---|
//! | `Eigen` / `eigen` | [`crate::eigen_sparse_lu::EigenSparseLuSolver`] (translation of Eigen 3.4.0 `SparseLU` + COLAMD, bit-identical with the strict oracle on valid finite input) | centralised on rank 0 | centralised on rank 0 | default |
//! | `Faer` / `faer` (port extension, D-040) | [`FaerLuSolver`] (pure-Rust `faer` simplicial sparse LU; qualified, no bitwise claim) | centralised on rank 0 | centralised on rank 0 | default |
//! | `Mumps` / `mumps` | MUMPS via C shim | distributed assembled (`ICNTL(18)=3`) | **centralised** dense RHS on rank 0 (`ICNTL(20)=ICNTL(21)=0`, `sparse_solver_mumps.cc:140-182`) | `mumps` |
//! | `Petsc` / `petsc` | PETSc KSP via C shim | distributed (PETSc ownership) | distributed (PETSc ownership) | `petsc` |
//! | `Auto` / `auto` | MUMPS if registered, else `eigen` (upstream `model_solver.cc:325-339`); faer is only selected explicitly | | | |
//!
//! Neither pure-Rust backend substitutes for the other: the factory
//! allocates exactly the resolved kind and refuses an allocator whose
//! reported [`SparseDirectSolver::kind`] differs from it.
//!
//! Centralised paths allocate `O(global system size)` on rank 0; they are
//! labelled with [`RhsDistribution::CentralizedOnRoot`] /
//! [`MatrixDistribution::CentralizedOnRoot`] and subject to
//! [`BackendCapabilities::max_centralized_size`]. Index narrowing
//! goes through `SparseMatrixAij::coo_one_based_i32`.

use std::collections::BTreeMap;

use akantu_core::enums::{MatrixType, SparseSolverType};
use akantu_core::{Error, GlobalIdx, Real, Result};

use akantu_comm::phase::{Phases, Severity};

use crate::layout::EquationLayout;
use crate::sparse::SparseMatrixAij;

/// Upstream `SolverParallelMethod`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SolverParallelMethod {
    /// `_not_parallel`
    NotParallel,
    /// `_fully_distributed`
    FullyDistributed,
    /// `_master_slave_distributed`
    MasterSlaveDistributed,
}

/// How a backend receives the matrix.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MatrixDistribution {
    /// Whole matrix on rank 0 (Eigen/faer; MUMPS `ICNTL(18)=0`).
    CentralizedOnRoot,
    /// Each rank passes its rank-partial COO entries (MUMPS `ICNTL(18)=3`).
    DistributedAssembled,
    /// Backend-owned distributed matrix (PETSc `MatSetValues` + collective
    /// `MatAssemblyBegin/End`).
    BackendOwned,
}

/// How the right-hand side and the solution are laid out.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum RhsDistribution {
    /// Gathered to rank 0 before the solve and scattered after it
    /// (`O(system size)` memory on rank 0).
    CentralizedOnRoot,
    /// Each rank holds its owned rows (PETSc; MUMPS distributed RHS is a
    /// separately validated extension, not upstream behaviour).
    Distributed,
}

/// Static description of a backend, checked before any collective
/// conversion (capacity limits are agreed on all ranks first).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BackendCapabilities {
    /// Matrix input mode.
    pub matrix: MatrixDistribution,
    /// RHS/solution mode.
    pub rhs: RhsDistribution,
    /// Largest representable 0-based global equation (e.g. `i32::MAX - 1`
    /// for 32-bit 1-based `MUMPS_INT`, detected `PetscInt` width otherwise).
    pub max_global_index: GlobalIdx,
    /// Largest number of entries one rank may pass (for a centralised
    /// backend also the largest gathered matrix: rank 0 passes them all).
    pub max_local_nnz: u64,
    /// Largest system a centralised path accepts on rank 0 (`None`: no
    /// centralised allocation).
    pub max_centralized_size: Option<GlobalIdx>,
}

impl BackendCapabilities {
    /// `true` if both the matrix and the right-hand side are centralised on
    /// rank 0 (the gather/solve/scatter path of [`crate::centralize`]).
    #[must_use]
    pub fn is_fully_centralized(&self) -> bool {
        self.matrix == MatrixDistribution::CentralizedOnRoot
            && self.rhs == RhsDistribution::CentralizedOnRoot
    }

    /// Local preflight of one rank's part of a system of global order
    /// `system_size` holding `local_nnz` stored entries: every global
    /// equation must be representable by the backend
    /// (`system_size − 1 ≤ max_global_index`) and the rank's entries within
    /// `max_local_nnz`. Purely local; run it before any conversion or
    /// allocation and agree its outcome on every rank.
    pub fn check_local(&self, system_size: GlobalIdx, local_nnz: u64) -> Result<()> {
        if system_size < 0 {
            return Err(Error::invalid(format!(
                "negative system size {system_size}"
            )));
        }
        if system_size > 0 && system_size - 1 > self.max_global_index {
            return Err(Error::CapacityExceeded {
                what: "largest global equation of the sparse solver backend",
                requested: (system_size - 1) as u128,
                limit: u128::try_from(self.max_global_index).unwrap_or(0),
            });
        }
        if local_nnz > self.max_local_nnz {
            return Err(Error::CapacityExceeded {
                what: "stored entries one rank passes to the sparse solver backend",
                requested: u128::from(local_nnz),
                limit: u128::from(self.max_local_nnz),
            });
        }
        Ok(())
    }

    /// Preflight of a system centralised on rank 0: the backend must accept
    /// a centralised allocation of order `system_size`
    /// (`max_centralized_size`) and the gathered matrix of `total_nnz`
    /// entries (`max_local_nnz`, rank 0 passing every entry), plus
    /// [`BackendCapabilities::check_local`]. Callers compute `total_nnz`
    /// with checked sums of identical data on every rank, so the outcome is
    /// common knowledge without a further agreement.
    pub fn check_centralized(&self, system_size: GlobalIdx, total_nnz: u64) -> Result<()> {
        let Some(limit) = self.max_centralized_size else {
            return Err(Error::Unsupported(
                "this sparse solver backend accepts no system centralised on rank 0".into(),
            ));
        };
        if system_size > limit {
            return Err(Error::CapacityExceeded {
                what: "order of the system centralised on rank 0",
                requested: u128::try_from(system_size).unwrap_or(0),
                limit: u128::try_from(limit).unwrap_or(0),
            });
        }
        if total_nnz > self.max_local_nnz {
            return Err(Error::CapacityExceeded {
                what: "stored entries of the matrix centralised on rank 0",
                requested: u128::from(total_nnz),
                limit: u128::from(self.max_local_nnz),
            });
        }
        self.check_local(system_size, 0)
    }
}

/// MUMPS `sym` parameter for `a` (upstream `sparse_solver_mumps.cc:221`:
/// `2 * (matrix_type == _symmetric)`): symmetric storage is passed as
/// "general symmetric" (2), anything else — including a matrix promoted by
/// the UD-034 fix — as unsymmetric (0).
#[must_use]
pub fn mumps_symmetry(a: &SparseMatrixAij) -> i32 {
    if a.matrix_type() == MatrixType::Symmetric {
        2
    } else {
        0
    }
}

/// Error of a singular matrix (upstream `debug::SingularMatrixException`).
#[must_use]
pub fn singular_matrix(detail: impl std::fmt::Display) -> Error {
    Error::LinearSolver(format!("Solver encountered singular matrix ({detail})"))
}

/// Refuse a non-finite matrix: the first stored entry
/// that is `±inf` or `NaN` is reported, as `Error::Numerical`.
pub fn check_finite_matrix(backend: &str, a: &CscMatrix) -> Result<()> {
    for j in 0..a.n {
        for p in a.col_ptr[j]..a.col_ptr[j + 1] {
            let v = a.values[p];
            if !v.is_finite() {
                return Err(Error::Numerical(format!(
                    "{backend}: non-finite matrix entry {v} at ({}, {j}); sparse direct \
                     solvers refuse non-finite systems",
                    a.row_idx[p]
                )));
            }
        }
    }
    Ok(())
}

/// Refuse a non-finite right-hand side.
pub fn check_finite_rhs(backend: &str, b: &[Real]) -> Result<()> {
    match b.iter().position(|v| !v.is_finite()) {
        Some(i) => Err(Error::Numerical(format!(
            "{backend}: non-finite right-hand side entry {} at row {i}; sparse direct solvers \
             refuse non-finite systems",
            b[i]
        ))),
        None => Ok(()),
    }
}

/// Arithmetic range failure: finite inputs gave non-finite factors or a
/// non-finite solution (overflow), distinct from singularity.
#[must_use]
pub fn arithmetic_range_failure(backend: &str, what: impl std::fmt::Display) -> Error {
    Error::Numerical(format!(
        "{backend}: arithmetic range failure: {what} left the binary64 range (finite input)"
    ))
}

/// Upstream-compatibility switches of the sparse direct solvers, selected
/// **per DOF manager** (D-041 (4): `akantu_solver::UpstreamCompat`) and
/// applied by the solver stack to every sparse solver of the manager before
/// each solve ([`SparseDirectSolver::set_upstream_compat`]); one switch
/// applies to every solver of that manager, also to reallocated ones.
/// Default: every corrected behaviour.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SparseSolverCompat {
    /// **UD-216** (category B): MUMPS reproduces upstream's workspace
    /// retry, whose outer solve uses the previous *solution* as right-hand
    /// side (`A^-(k+1) b` after `k` factorisation retries;
    /// `mumps::RetryRhs::UpstreamSolution`). For differential replays only.
    pub mumps_retry_with_solution_rhs: bool,
}

/// A linear solver for `A x = b` (upstream `SparseSolver`).
///
/// Contract (D-040): `kind()`/`name()` identify the backend
/// actually running (no backend reports another one's kind); every failure
/// is a returned error (see the module documentation for singular,
/// non-finite and range failures); a failed `analysis` or `factorize`
/// leaves the solver unable to solve until a later successful
/// factorisation; on error `solve` leaves `x` unchanged.
pub trait SparseDirectSolver: std::fmt::Debug + Send {
    /// Backend kind (upstream factory key; `Faer` for the port extension).
    fn kind(&self) -> SparseSolverType;
    /// Backend name (`eigen`, `faer`, `mumps`, `petsc`; `custom` for
    /// third-party backends that do not override it).
    fn name(&self) -> &'static str {
        "custom"
    }
    /// Distribution and capacity of this backend.
    fn capabilities(&self) -> BackendCapabilities;
    /// Upstream `parallel_method`.
    fn parallel_method(&self) -> SolverParallelMethod {
        SolverParallelMethod::NotParallel
    }
    /// Symbolic analysis of `a`'s profile; must record `a.matrix_type()`
    /// (a promotion to unsymmetric forces a new analysis).
    fn analysis(&mut self, a: &SparseMatrixAij) -> Result<()>;
    /// Numeric factorisation (after [`SparseDirectSolver::analysis`] of the
    /// same profile).
    fn factorize(&mut self, a: &SparseMatrixAij) -> Result<()>;
    /// Solve with the current factorisation; `b` and `x` are global vectors
    /// on rank 0 for [`RhsDistribution::CentralizedOnRoot`] backends (empty
    /// on other ranks), owned rows for distributed ones.
    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()>;
    /// Release internal data (upstream `destroyInternalData`, called on
    /// communicator finalisation); a later solve needs a new analysis.
    fn destroy_internal_data(&mut self) {}
    /// Release the analysis and the factors on the caller's request (port
    /// extension; upstream keeps them until the solver is destroyed): the
    /// next solve analyses and factorises again. Unlike
    /// [`SparseDirectSolver::destroy_internal_data`], a failure is
    /// reported. Collective for a distributed solver (MUMPS: `JOB = -2` on
    /// every rank of its communicator, at the same program point). Default:
    /// `destroy_internal_data`.
    fn release_factorization(&mut self) -> Result<()> {
        self.destroy_internal_data();
        Ok(())
    }
    /// Apply the manager-wide upstream-compatibility switches (D-041 (4)).
    /// The solver stack calls it before every solve, identically on every
    /// rank; backends without a switch of their own ignore it (default).
    fn set_upstream_compat(&mut self, compat: SparseSolverCompat) {
        let _ = compat;
    }
    /// Diagnostic statistics of the last solve as `(name, value)` pairs in a
    /// fixed order (MUMPS: ordering, memory and flop figures of its
    /// `INFOG`/`RINFOG` arrays, workspace retries, for the
    /// memory and time basis of the full-size benchmark run). Read-only, no
    /// effect on any result; empty by default (and before the first solve).
    fn statistics(&self) -> Vec<(&'static str, f64)> {
        Vec::new()
    }
    /// Collective solve of a [`MatrixDistribution::DistributedAssembled`]
    /// backend (MUMPS `ICNTL(18) = 3`) over `phases` (the DOF
    /// manager's operation scope): every rank passes its **rank-partial**
    /// matrix `a` (global equations; duplicates across ranks are summed by
    /// the backend) and its local-equation vectors; the backend reads the
    /// owned rows of `rhs` and writes the owned rows of `solution` (shared
    /// rows are synchronised by the manager afterwards, `docs/ARCHITECTURE.md`
    /// solve contract). `a` is the local matrix lookup: its error is agreed
    /// like any local validation failure. Analysis / factorisation reuse is
    /// the backend's business (agreed across ranks). The default refuses on
    /// every rank (agreed): only distributed-assembled backends implement
    /// it.
    fn solve_distributed(
        &mut self,
        phases: &mut Phases,
        name: &str,
        layout: &EquationLayout,
        a: Result<&SparseMatrixAij>,
        rhs: &[Real],
        solution: &mut [Real],
    ) -> Result<()> {
        let _ = (layout, a, rhs, solution);
        phases.agree(
            &format!("{name}:distributed"),
            Err(Error::Unsupported(format!(
                "the {} sparse solver takes no distributed-assembled system",
                self.name()
            ))),
            Severity::Recoverable,
        )
    }
}

/// Capabilities of the serial, centralised pure-Rust backends.
#[must_use]
pub fn centralized_capabilities() -> BackendCapabilities {
    BackendCapabilities {
        matrix: MatrixDistribution::CentralizedOnRoot,
        rhs: RhsDistribution::CentralizedOnRoot,
        max_global_index: akantu_core::MAX_GLOBAL_IDX,
        max_local_nnz: u64::MAX,
        // Policy limit for the rank-0 gather (the factor storage, not the
        // index type, is the practical limit).
        max_centralized_size: Some(GlobalIdx::from(u32::MAX)),
    }
}

/// `vec![value; len]` with a fallible allocation (no abort on a huge
/// request).
fn try_filled<T: Clone>(what: &'static str, len: usize, value: T) -> Result<Vec<T>> {
    let mut v = Vec::new();
    v.try_reserve_exact(len)
        .map_err(|_| Error::CapacityExceeded {
            what,
            requested: len as u128,
            limit: (isize::MAX as usize / std::mem::size_of::<T>().max(1)) as u128,
        })?;
    v.resize(len, value);
    Ok(v)
}

/// Compressed sparse column matrix built from an AIJ matrix exactly as
/// upstream `SparseSolverEigen::setA` builds its `Eigen::SparseMatrix`:
/// triplets `(i, j, a)` in storage order, followed for symmetric storage and
/// `i != j` by the mirror `(j, i, a)`; `setFromTriplets` then sums
/// duplicates **in triplet order** (`a1 + a2 + …`) and stores every column
/// with increasing row indices (the transposed copy of a row-major
/// temporary). Explicit zeros stay structural entries.
///
/// Invariants (enforced by every constructor; the fields
/// are private so that no malformed matrix reaches a consumer):
/// `col_ptr.len() == n + 1`, `col_ptr[0] == 0`, `col_ptr` non-decreasing,
/// `col_ptr[n] == row_idx.len() == values.len()`, every row index `< n`
/// and row indices **strictly increasing** within each column (canonical
/// compressed storage, no duplicates).
#[derive(Clone, Debug, PartialEq)]
pub struct CscMatrix {
    n: usize,
    col_ptr: Vec<usize>,
    row_idx: Vec<usize>,
    values: Vec<Real>,
}

impl Default for CscMatrix {
    /// The empty `0 × 0` matrix (`col_ptr = [0]`).
    fn default() -> Self {
        Self {
            n: 0,
            col_ptr: vec![0],
            row_idx: Vec::new(),
            values: Vec::new(),
        }
    }
}

impl CscMatrix {
    /// Checked constructor from raw compressed-column arrays: every
    /// invariant of the type is validated (see the type documentation);
    /// violations are [`Error::InvalidArgument`] /
    /// [`Error::DimensionMismatch`] / [`Error::CapacityExceeded`], never a
    /// panic.
    pub fn try_new(
        n: usize,
        col_ptr: Vec<usize>,
        row_idx: Vec<usize>,
        values: Vec<Real>,
    ) -> Result<Self> {
        let np1 = n.checked_add(1).ok_or(Error::CapacityExceeded {
            what: "CSC matrix order",
            requested: n as u128,
            limit: (usize::MAX - 1) as u128,
        })?;
        if col_ptr.len() != np1 {
            return Err(Error::DimensionMismatch {
                context: "CscMatrix: column pointers (n + 1)",
                expected: np1,
                got: col_ptr.len(),
            });
        }
        if col_ptr[0] != 0 {
            return Err(Error::invalid(format!(
                "CscMatrix: col_ptr[0] = {} (must be 0)",
                col_ptr[0]
            )));
        }
        if row_idx.len() != values.len() {
            return Err(Error::DimensionMismatch {
                context: "CscMatrix: values (one per row index)",
                expected: row_idx.len(),
                got: values.len(),
            });
        }
        if col_ptr[n] != row_idx.len() {
            return Err(Error::DimensionMismatch {
                context: "CscMatrix: row indices (col_ptr[n])",
                expected: col_ptr[n],
                got: row_idx.len(),
            });
        }
        // Validate every column pointer before slicing.
        // With `col_ptr[0] = 0`, `col_ptr[n] = nnz` and non-decreasing
        // pointers, every `col_ptr[j] <= nnz`, so the slices below are in
        // bounds.
        if let Some(j) = (0..n).find(|&j| col_ptr[j + 1] < col_ptr[j]) {
            return Err(Error::invalid(format!(
                "CscMatrix: column pointers decrease at column {j} ({} > {})",
                col_ptr[j],
                col_ptr[j + 1]
            )));
        }
        for j in 0..n {
            let (s, e) = (col_ptr[j], col_ptr[j + 1]);
            let mut prev: Option<usize> = None;
            for &r in &row_idx[s..e] {
                if r >= n {
                    return Err(Error::OutOfRange {
                        what: "CscMatrix row index",
                        index: r,
                        len: n,
                    });
                }
                if prev.is_some_and(|p| r <= p) {
                    return Err(Error::invalid(format!(
                        "CscMatrix: row indices of column {j} are not strictly increasing \
                         (canonical storage required)"
                    )));
                }
                prev = Some(r);
            }
        }
        Ok(Self {
            n,
            col_ptr,
            row_idx,
            values,
        })
    }

    /// Build from triplets with Eigen 3.4.0 `setFromTriplets` semantics
    /// (`Eigen/src/SparseCore/SparseMatrix.h`, `set_from_triplets`: count
    /// per row of a row-major temporary, insert in triplet order, collapse
    /// duplicates with `+` in insertion order, transposed copy). Sizes are
    /// checked and allocations fallible: an unrepresentable order is an
    /// error, not an overflow or an abort.
    pub fn from_triplets(n: usize, triplets: &[(usize, usize, Real)]) -> Result<Self> {
        for &(i, j, _) in triplets {
            if i >= n || j >= n {
                return Err(Error::OutOfRange {
                    what: "triplet coordinate",
                    index: i.max(j),
                    len: n,
                });
            }
        }
        let np1 = n.checked_add(1).ok_or(Error::CapacityExceeded {
            what: "CSC matrix order",
            requested: n as u128,
            limit: (usize::MAX - 1) as u128,
        })?;
        // Row-major temporary: per row, (col, value) in triplet order.
        let mut row_count = try_filled("CSC row counts", np1, 0usize)?;
        for &(i, _, _) in triplets {
            row_count[i + 1] += 1;
        }
        for r in 0..n {
            row_count[r + 1] += row_count[r];
        }
        let mut fill = row_count.clone();
        let mut tr_cols = vec![0usize; triplets.len()];
        let mut tr_vals = vec![0.0; triplets.len()];
        for &(i, j, v) in triplets {
            let p = fill[i];
            tr_cols[p] = j;
            tr_vals[p] = v;
            fill[i] += 1;
        }
        // collapseDuplicates: first occurrence keeps its position, later
        // ones are summed into it in insertion order.
        let mut first = try_filled("CSC duplicate markers", n, usize::MAX)?;
        let mut tr_ptr = try_filled("CSC row pointers", np1, 0usize)?;
        let mut out = 0usize;
        for r in 0..n {
            let start = out;
            for p in row_count[r]..row_count[r + 1] {
                let c = tr_cols[p];
                let q = first[c];
                if q != usize::MAX && q >= start {
                    tr_vals[q] += tr_vals[p];
                } else {
                    first[c] = out;
                    tr_cols[out] = c;
                    tr_vals[out] = tr_vals[p];
                    out += 1;
                }
            }
            tr_ptr[r + 1] = out;
        }
        // Transposed copy: rows visited in increasing order, so each column
        // receives increasing row indices.
        let nnz = out;
        let mut col_ptr = try_filled("CSC column pointers", np1, 0usize)?;
        for &c in &tr_cols[..nnz] {
            col_ptr[c + 1] += 1;
        }
        for c in 0..n {
            col_ptr[c + 1] += col_ptr[c];
        }
        let mut pos = col_ptr.clone();
        let mut row_idx = vec![0usize; nnz];
        let mut values = vec![0.0; nnz];
        for r in 0..n {
            for p in tr_ptr[r]..tr_ptr[r + 1] {
                let c = tr_cols[p];
                row_idx[pos[c]] = r;
                values[pos[c]] = tr_vals[p];
                pos[c] += 1;
            }
        }
        Ok(Self {
            n,
            col_ptr,
            row_idx,
            values,
        })
    }

    /// The matrix upstream's Eigen backend factorises for `a` (symmetric
    /// storage expanded).
    pub fn from_aij(a: &SparseMatrixAij) -> Result<Self> {
        let n = usize::try_from(a.size())
            .map_err(|_| Error::invalid("matrix order exceeds the address space"))?;
        let (irn, jcn, vals) = a.coo();
        let symmetric = a.matrix_type() == MatrixType::Symmetric;
        let mut triplets = Vec::with_capacity(vals.len() * if symmetric { 2 } else { 1 });
        let idx = |g: GlobalIdx| -> Result<usize> {
            usize::try_from(g).ok().filter(|&u| u < n).ok_or_else(|| {
                Error::invalid(format!("coordinate {g} outside the matrix order {n}"))
            })
        };
        for ((&i, &j), &v) in irn.iter().zip(jcn).zip(vals) {
            let (iu, ju) = (idx(i)?, idx(j)?);
            triplets.push((iu, ju, v));
            if symmetric && i != j {
                triplets.push((ju, iu, v));
            }
        }
        Self::from_triplets(n, &triplets)
    }

    /// Order.
    #[must_use]
    pub fn n(&self) -> usize {
        self.n
    }

    /// Column pointers (`n + 1`).
    #[must_use]
    pub fn col_ptr(&self) -> &[usize] {
        &self.col_ptr
    }

    /// Row index of each entry, strictly increasing within a column.
    #[must_use]
    pub fn row_idx(&self) -> &[usize] {
        &self.row_idx
    }

    /// Values.
    #[must_use]
    pub fn values(&self) -> &[Real] {
        &self.values
    }

    /// `(n, col_ptr, row_idx, values)`.
    #[must_use]
    pub fn into_parts(self) -> (usize, Vec<usize>, Vec<usize>, Vec<Real>) {
        (self.n, self.col_ptr, self.row_idx, self.values)
    }

    /// Number of stored entries.
    #[must_use]
    pub fn nnz(&self) -> usize {
        self.values.len()
    }

    /// `y = A x` (plain accumulation in storage order; used by checks).
    pub fn mul_vec(&self, x: &[Real]) -> Result<Vec<Real>> {
        if x.len() != self.n {
            return Err(Error::DimensionMismatch {
                context: "CscMatrix::mul_vec",
                expected: self.n,
                got: x.len(),
            });
        }
        let mut y = vec![0.0; self.n];
        for (j, &xj) in x.iter().enumerate() {
            for p in self.col_ptr[j]..self.col_ptr[j + 1] {
                y[self.row_idx[p]] += self.values[p] * xj;
            }
        }
        Ok(y)
    }
}

// ----------------------------------------------------------------- faer

/// Numeric state of [`FaerLuSolver`]: faer's simplicial factors, the row
/// permutation (pivot position → original row) and `U`'s diagonal.
#[derive(Debug)]
struct FaerNumeric {
    lu: faer::sparse::linalg::lu::simplicial::SimplicialLu<usize, f64>,
    row_perm: Vec<usize>,
    pivots: Vec<f64>,
}

/// Pure-Rust port extension (`SparseSolverType::Faer`, D-040) on `faer`'s
/// sparse LU: COLAMD column ordering and faer's **simplicial**
/// (left-looking, Gilbert–Peierls) factorisation with partial pivoting, run
/// sequentially. The simplicial variant is used because it exposes the `L`
/// and `U` factors:
///
/// * every pivot is validated directly — a zero pivot is a singular matrix,
///   a non-finite factor entry from finite input an arithmetic range
///   failure; no probe solve and no tolerance is involved;
/// * the triangular solves run here on those factors (forward substitution
///   with `L`'s unit diagonal, backward substitution **dividing** by `U`'s
///   pivots), because faer's own sparse triangular solve multiplies by the
///   reciprocal pivot, which overflows for a subnormal pivot: with the
///   division, a tiny but nonsingular `A = [1e-310]` solves to `x = 1`.
///   Pivots below 2^-1024 that have sub-diagonal entries remain range
///   failures (faer's factorisation multiplies by the reciprocal pivot),
///   as for Eigen.
///
/// Its results are **not** bit-identical with upstream's Eigen `SparseLU`;
/// it is qualified by the protocol of `docs/NUMERICAL_VALIDATION.md`
/// ("Sparse algebra and direct solvers": LAPACK `dget02` backward
/// error, componentwise residual, conditioning-aware forward error,
/// singular, indefinite, uniform-scale and zero-RHS cases) — see
/// `tests/faer_qualification.rs`.
#[derive(Debug, Default)]
pub struct FaerLuSolver {
    n: usize,
    symbolic: Option<faer::sparse::linalg::lu::SymbolicLu<usize>>,
    numeric: Option<FaerNumeric>,
    pattern: Option<(Vec<usize>, Vec<usize>)>,
}

fn faer_alloc_error(what: &str) -> Error {
    Error::Backend {
        backend: "faer",
        code: -2,
        message: format!("cannot allocate the {what}"),
    }
}

impl FaerLuSolver {
    fn faer_matrix(c: &CscMatrix) -> faer::sparse::SparseColMat<usize, f64> {
        // The CscMatrix invariants (canonical storage, rows < n) are exactly
        // faer's: `new_checked` cannot fail here.
        let symbolic = faer::sparse::SymbolicSparseColMat::new_checked(
            c.n,
            c.n,
            c.col_ptr.clone(),
            None,
            c.row_idx.clone(),
        );
        faer::sparse::SparseColMat::new(symbolic, c.values.clone())
    }

    /// Validate the pivots and factor entries of a successful simplicial
    /// factorisation (U's diagonal must be nonzero; every L/U entry finite,
    /// the input being finite); returns `U`'s diagonal.
    fn validate_factors(
        n: usize,
        lu: &faer::sparse::linalg::lu::simplicial::SimplicialLu<usize, f64>,
    ) -> Result<Vec<f64>> {
        let u = lu.u_factor_unsorted();
        let l = lu.l_factor_unsorted();
        let mut pivots = Vec::with_capacity(n);
        for j in 0..n {
            let rows = u.row_idx_of_col_raw(j);
            let vals = u.val_of_col(j);
            let mut pivot = None;
            for (&r, &v) in rows.iter().zip(vals) {
                if r == j {
                    pivot = Some(v);
                }
                if !v.is_finite() {
                    return Err(arithmetic_range_failure(
                        "faer",
                        format_args!("U factor entry ({r}, {j}) = {v}"),
                    ));
                }
            }
            let Some(p) = pivot else {
                return Err(singular_matrix(format!("faer: no pivot in column {j}")));
            };
            // ±0 exactly: no tolerance (a tiny nonzero pivot is nonsingular)
            if p == 0.0 {
                return Err(singular_matrix(format!("faer: zero pivot in column {j}")));
            }
            pivots.push(p);
            for (&r, &v) in l.row_idx_of_col_raw(j).iter().zip(l.val_of_col(j)) {
                if !v.is_finite() {
                    return Err(arithmetic_range_failure(
                        "faer",
                        format_args!("L factor entry ({r}, {j}) = {v}"),
                    ));
                }
            }
        }
        Ok(pivots)
    }

    /// `x = Q U⁻¹ L⁻¹ P b` on the validated simplicial factors (rows of
    /// `L`/`U` are pivot positions; column `j` of the factors is original
    /// column `col_perm_fwd[j]`).
    fn substitute(num: &FaerNumeric, col_perm_inv: &[usize], b: &[Real]) -> Vec<Real> {
        let n = num.pivots.len();
        let mut y: Vec<Real> = num.row_perm.iter().map(|&r| b[r]).collect();
        let l = num.lu.l_factor_unsorted();
        for j in 0..n {
            let yj = y[j];
            for (&r, &v) in l.row_idx_of_col_raw(j).iter().zip(l.val_of_col(j)) {
                if r != j {
                    y[r] -= v * yj;
                }
            }
        }
        let u = num.lu.u_factor_unsorted();
        for j in (0..n).rev() {
            y[j] /= num.pivots[j];
            let yj = y[j];
            for (&r, &v) in u.row_idx_of_col_raw(j).iter().zip(u.val_of_col(j)) {
                if r != j {
                    y[r] -= v * yj;
                }
            }
        }
        col_perm_inv.iter().map(|&p| y[p]).collect()
    }
}

impl SparseDirectSolver for FaerLuSolver {
    fn kind(&self) -> SparseSolverType {
        SparseSolverType::Faer
    }

    fn name(&self) -> &'static str {
        "faer"
    }

    fn capabilities(&self) -> BackendCapabilities {
        centralized_capabilities()
    }

    fn analysis(&mut self, a: &SparseMatrixAij) -> Result<()> {
        use faer::sparse::linalg::SupernodalThreshold;
        use faer::sparse::linalg::lu::{LuSymbolicParams, factorize_symbolic_lu};
        self.symbolic = None;
        self.numeric = None;
        self.pattern = None;
        let c = CscMatrix::from_aij(a)?;
        let m = Self::faer_matrix(&c);
        let params = LuSymbolicParams {
            supernodal_flop_ratio_threshold: SupernodalThreshold::FORCE_SIMPLICIAL,
            ..Default::default()
        };
        let symbolic = factorize_symbolic_lu(m.symbolic(), params).map_err(|e| Error::Backend {
            backend: "faer",
            code: -1,
            message: format!("symbolic LU failed: {e:?}"),
        })?;
        let (n, col_ptr, row_idx, _) = c.into_parts();
        self.n = n;
        self.symbolic = Some(symbolic);
        self.pattern = Some((col_ptr, row_idx));
        Ok(())
    }

    fn factorize(&mut self, a: &SparseMatrixAij) -> Result<()> {
        use faer::dyn_stack::{MemBuffer, MemStack};
        use faer::sparse::linalg::lu::simplicial;
        // Every failure below leaves no usable factorisation.
        self.numeric = None;
        let symbolic = self
            .symbolic
            .as_ref()
            .ok_or_else(|| Error::LinearSolver("factorize() called before analysis()".into()))?;
        let c = CscMatrix::from_aij(a)?;
        if self
            .pattern
            .as_ref()
            .is_none_or(|(cp, ri)| cp != &c.col_ptr || ri != &c.row_idx)
        {
            return Err(Error::LinearSolver(
                "the matrix profile changed since analysis() (re-analyse first)".into(),
            ));
        }
        check_finite_matrix("faer", &c)?;
        let n = c.n;
        let m = Self::faer_matrix(&c);
        let req = simplicial::factorize_simplicial_numeric_lu_scratch::<usize, f64>(n, n);
        let mut mem = MemBuffer::try_new(req).map_err(|_| faer_alloc_error("LU workspace"))?;
        let mut lu = simplicial::SimplicialLu::<usize, f64>::new();
        let mut row_perm = try_filled("faer row permutation", n, 0usize)?;
        let mut row_perm_inv = try_filled("faer inverse row permutation", n, 0usize)?;
        // faer's own (unvalidated) solve is never used: see `substitute`.
        simplicial::factorize_simplicial_numeric_lu(
            &mut row_perm,
            &mut row_perm_inv,
            &mut lu,
            m.as_ref(),
            symbolic.col_perm(),
            MemStack::new(&mut mem),
        )
        .map_err(|e| match e {
            faer::sparse::linalg::LuError::SymbolicSingular { index } => {
                singular_matrix(format!("faer: no pivot in column {index}"))
            }
            faer::sparse::linalg::LuError::Generic(g) => Error::Backend {
                backend: "faer",
                code: -3,
                message: format!("{g:?}"),
            },
        })?;
        let pivots = Self::validate_factors(n, &lu)?;
        self.numeric = Some(FaerNumeric {
            lu,
            row_perm,
            pivots,
        });
        Ok(())
    }

    fn solve(&mut self, b: &[Real], x: &mut [Real]) -> Result<()> {
        if b.len() != self.n || x.len() != self.n {
            return Err(Error::DimensionMismatch {
                context: "FaerLuSolver::solve",
                expected: self.n,
                got: if b.len() == self.n { x.len() } else { b.len() },
            });
        }
        let (Some(symbolic), Some(num)) = (self.symbolic.as_ref(), self.numeric.as_ref()) else {
            return Err(Error::LinearSolver(
                "solve() called before a successful factorize()".into(),
            ));
        };
        check_finite_rhs("faer", b)?;
        let sol = Self::substitute(num, symbolic.col_perm().arrays().1, b);
        if let Some(i) = sol.iter().position(|v| !v.is_finite()) {
            return Err(arithmetic_range_failure(
                "faer",
                format_args!("solution entry {i} = {}", sol[i]),
            ));
        }
        x.copy_from_slice(&sol);
        Ok(())
    }

    fn destroy_internal_data(&mut self) {
        *self = Self::default();
    }
}

// -------------------------------------------------------------- factory

/// Allocator of a sparse solver.
pub type SparseSolverAllocator = fn() -> Box<dyn SparseDirectSolver>;

/// Upstream `SparseSolverFactory` (`Factory<SparseSolver, SparseSolverType,
/// …>` filled by `instantiateSparseSolver`): allocators keyed by upstream
/// name (`eigen`, `mumps`, `petsc`) plus the port extension `faer`
/// (D-040). Runtime registration replaces an allocator (the MUMPS and
/// PETSc backends register theirs behind their features).
#[derive(Clone, Debug)]
pub struct SparseSolverFactory {
    allocators: BTreeMap<String, SparseSolverAllocator>,
}

impl Default for SparseSolverFactory {
    fn default() -> Self {
        let mut f = Self {
            allocators: BTreeMap::new(),
        };
        f.register("eigen", || {
            Box::new(crate::eigen_sparse_lu::EigenSparseLuSolver::default())
        });
        f.register("faer", || Box::new(FaerLuSolver::default()));
        // Compiled in with the `mumps` feature (parallel MUMPS,
        // needs MPI); `auto` resolves to it on MPI communicators
        // (`resolve_available`).
        #[cfg(feature = "mumps")]
        f.register("mumps", || Box::new(crate::mumps::MumpsSolver::default()));
        f
    }
}

impl SparseSolverFactory {
    /// Register (or replace) the allocator for `name`.
    pub fn register(&mut self, name: &str, allocator: SparseSolverAllocator) {
        self.allocators.insert(name.to_owned(), allocator);
    }

    /// `true` if `name` is registered.
    #[must_use]
    pub fn is_registered(&self, name: &str) -> bool {
        self.allocators.contains_key(name)
    }

    /// Registered names (sorted).
    #[must_use]
    pub fn names(&self) -> Vec<&str> {
        self.allocators.keys().map(String::as_str).collect()
    }

    /// Resolve `auto` like upstream (`model_solver.cc:325-339`): MUMPS if
    /// registered (available in this build), else Eigen. Every other kind
    /// — including the explicit extension `Faer` — resolves to itself.
    #[must_use]
    pub fn resolve(&self, ty: SparseSolverType) -> SparseSolverType {
        match ty {
            SparseSolverType::Auto if self.is_registered("mumps") => SparseSolverType::Mumps,
            SparseSolverType::Auto => SparseSolverType::Eigen,
            t => t,
        }
    }

    /// Resolve `ty` for a run whose DOF manager communicates over an MPI
    /// communicator (`mpi = true`) or not: `auto` → MUMPS only if it is
    /// registered **and available** — the registered MUMPS is the parallel
    /// library, which needs an MPI communicator (upstream's MPI build uses
    /// it on any number of processes, `_fully_distributed`); on a serial or
    /// in-process communicator `auto` → Eigen, as in a build without MUMPS
    /// (the sequential MUMPS build, upstream `_not_parallel`, is not
    /// provided; `docs/DEPENDENCIES.md`). Explicit kinds resolve to
    /// themselves (an explicit `mumps` on a non-MPI communicator fails at
    /// the solve, on every rank).
    #[must_use]
    pub fn resolve_available(&self, ty: SparseSolverType, mpi: bool) -> SparseSolverType {
        match ty {
            SparseSolverType::Auto if mpi && self.is_registered("mumps") => SparseSolverType::Mumps,
            SparseSolverType::Auto => SparseSolverType::Eigen,
            t => t,
        }
    }

    /// Allocate exactly the resolved kind (no substitution) and check that
    /// the allocated backend reports that kind (D-040).
    fn allocate(&self, ty: SparseSolverType) -> Result<Box<dyn SparseDirectSolver>> {
        let ty = self.resolve(ty);
        let solver = self.allocate_raw(ty.name())?;
        if solver.kind() != ty {
            return Err(Error::Backend {
                backend: "sparse solver factory",
                code: -1,
                message: format!(
                    "the allocator registered as '{}' built a '{}' backend (kind {}); sparse \
                     solvers are never substituted",
                    ty.name(),
                    solver.name(),
                    solver.kind()
                ),
            });
        }
        Ok(solver)
    }

    /// Allocate a solver of upstream type `ty` (after `auto` resolution) for
    /// **direct** use by a run on `nb_proc` processes. The serial-only
    /// centralised backends (`eigen`, `faer`) refuse `nb_proc > 1` here
    /// (upstream asserts "This solver does not work in parallel" in debug
    /// builds only and computes garbage otherwise); multi-rank use of them
    /// is the rank-0 gather/solve/scatter of [`crate::centralize`], whose
    /// solver comes from [`SparseSolverFactory::create_centralized`].
    /// Unregistered types are [`Error::Unsupported`].
    pub fn create(
        &self,
        ty: SparseSolverType,
        nb_proc: usize,
    ) -> Result<Box<dyn SparseDirectSolver>> {
        let resolved = self.resolve(ty);
        if matches!(resolved, SparseSolverType::Eigen | SparseSolverType::Faer) && nb_proc > 1 {
            return Err(Error::Unsupported(format!(
                "the {resolved} sparse solver does not work in parallel (upstream: \"This solver \
                 does not work in parallel\"); use the centralised rank-0 execution \
                 (SparseSolverFactory::create_centralized + akantu_linalg::centralize)"
            )));
        }
        self.allocate(resolved)
    }

    /// Allocate a solver of type `ty` (after `auto` resolution) for the
    /// rank-0 **centralised** execution of a multi-rank system
    /// ([`crate::centralize`]: gather, solve on rank 0, scatter). The
    /// backend must take both its matrix and its right-hand side
    /// centralised on rank 0 ([`BackendCapabilities::is_fully_centralized`]);
    /// other backends (MUMPS distributed assembly, PETSc) are refused.
    pub fn create_centralized(&self, ty: SparseSolverType) -> Result<Box<dyn SparseDirectSolver>> {
        let solver = self.allocate(ty)?;
        if !solver.capabilities().is_fully_centralized() {
            return Err(Error::Unsupported(format!(
                "the {} sparse solver does not take a system centralised on rank 0",
                solver.name()
            )));
        }
        Ok(solver)
    }

    /// Allocate by registered name. Names of the upstream solver types
    /// (`auto`, `eigen`, `faer`, `mumps`, `petsc`) go through the same
    /// `auto` resolution and D-040 kind check as
    /// [`SparseSolverFactory::create`] (no silent
    /// substitution through this path either); other names are custom
    /// registrations allocated as registered. This path does not apply the
    /// `nb_proc > 1` refusal — callers that run on several processes use
    /// [`SparseSolverFactory::create`] or
    /// [`SparseSolverFactory::create_centralized`].
    pub fn create_by_name(&self, name: &str) -> Result<Box<dyn SparseDirectSolver>> {
        match name.parse::<SparseSolverType>() {
            Ok(ty) => self.allocate(ty),
            Err(_) => self.allocate_raw(name),
        }
    }

    /// The registered allocator for `name`, without any check.
    fn allocate_raw(&self, name: &str) -> Result<Box<dyn SparseDirectSolver>> {
        self.allocators.get(name).map(|a| a()).ok_or_else(|| {
            Error::Unsupported(format!(
                "sparse solver '{name}' is not available in this build (registered: {})",
                self.names().join(", ")
            ))
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mumps_sym_follows_effective_symmetry() {
        let mut a = SparseMatrixAij::new("K", 2, MatrixType::Symmetric);
        assert_eq!(mumps_symmetry(&a), 2);
        a.promote_to_unsymmetric();
        assert_eq!(mumps_symmetry(&a), 0);
        let caps = FaerLuSolver::default().capabilities();
        assert_eq!(caps.rhs, RhsDistribution::CentralizedOnRoot);
        assert!(caps.max_centralized_size.is_some());
        assert!(caps.is_fully_centralized());
    }

    #[test]
    fn set_from_triplets_sums_in_order_and_sorts_rows() {
        let t = [
            (1, 0, 1.0),
            (0, 0, 2.0),
            (1, 0, 1e-16),
            (1, 0, -1.0),
            (0, 1, 3.0),
        ];
        let c = CscMatrix::from_triplets(2, &t).unwrap();
        assert_eq!(c.col_ptr(), [0, 2, 3]);
        assert_eq!(c.row_idx(), [0, 1, 0]);
        // (1 + 1e-16) + (-1) in triplet order = 0 (1e-16 absorbed), not 1e-16.
        assert_eq!(c.values(), [2.0, 0.0, 3.0]);
        assert!(CscMatrix::from_triplets(2, &[(2, 0, 1.0)]).is_err());
        // round trip through the checked constructor
        let (n, cp, ri, v) = c.clone().into_parts();
        assert_eq!(CscMatrix::try_new(n, cp, ri, v).unwrap(), c);
        assert_eq!(CscMatrix::default().col_ptr(), [0]);
    }

    #[test]
    fn capability_checks() {
        let caps = BackendCapabilities {
            matrix: MatrixDistribution::CentralizedOnRoot,
            rhs: RhsDistribution::CentralizedOnRoot,
            max_global_index: 0,
            max_local_nnz: 1,
            max_centralized_size: Some(1),
        };
        caps.check_local(1, 1).unwrap();
        caps.check_centralized(1, 1).unwrap();
        caps.check_local(0, 0).unwrap();
        assert!(matches!(
            caps.check_local(2, 0),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(matches!(
            caps.check_local(1, 2),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(matches!(
            caps.check_centralized(2, 0),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(matches!(
            caps.check_centralized(1, 2),
            Err(Error::CapacityExceeded { .. })
        ));
        assert!(caps.check_local(-1, 0).is_err());
        let none = BackendCapabilities {
            max_centralized_size: None,
            ..caps
        };
        assert!(matches!(
            none.check_centralized(1, 0),
            Err(Error::Unsupported(_))
        ));
    }

    /// The default factory without the MUMPS registration (the feature-less
    /// build's factory).
    fn without_mumps(f: SparseSolverFactory) -> SparseSolverFactory {
        let mut g = SparseSolverFactory {
            allocators: BTreeMap::new(),
        };
        for (name, alloc) in f.allocators {
            if name != "mumps" {
                g.allocators.insert(name, alloc);
            }
        }
        g
    }

    /// Explicit `Faer` kind, `auto` resolution,
    /// reported kind = resolved kind, no substitution.
    #[test]
    fn factory_names_and_auto() {
        let mut f = SparseSolverFactory::default();
        // With the `mumps` feature MUMPS is registered (`auto` → MUMPS on
        // MPI communicators, checked by tests/mumps_mpi.rs); this test is
        // about the build without it.
        assert_eq!(f.is_registered("mumps"), cfg!(feature = "mumps"));
        assert_eq!(
            f.resolve_available(SparseSolverType::Auto, false),
            SparseSolverType::Eigen
        );
        let f_mumps = f.clone();
        f = without_mumps(f);
        assert_eq!(
            f_mumps.resolve(SparseSolverType::Auto),
            if cfg!(feature = "mumps") {
                SparseSolverType::Mumps
            } else {
                SparseSolverType::Eigen
            }
        );
        assert_eq!(f.resolve(SparseSolverType::Auto), SparseSolverType::Eigen);
        assert_eq!(f.resolve(SparseSolverType::Faer), SparseSolverType::Faer);
        let e = f.create(SparseSolverType::Eigen, 1).unwrap();
        assert_eq!((e.name(), e.kind()), ("eigen", SparseSolverType::Eigen));
        let a = f.create(SparseSolverType::Auto, 1).unwrap();
        assert_eq!((a.name(), a.kind()), ("eigen", SparseSolverType::Eigen));
        let x = f.create(SparseSolverType::Faer, 1).unwrap();
        assert_eq!((x.name(), x.kind()), ("faer", SparseSolverType::Faer));
        assert_eq!(
            f.create_by_name("faer").unwrap().kind(),
            SparseSolverType::Faer
        );
        assert_eq!(
            "faer".parse::<SparseSolverType>().unwrap(),
            SparseSolverType::Faer
        );
        assert_eq!(SparseSolverType::Faer.to_string(), "faer");
        // serial-only backends refuse direct multi-process use, but are
        // available for the centralised execution
        assert!(f.create(SparseSolverType::Eigen, 2).is_err());
        assert!(f.create(SparseSolverType::Faer, 2).is_err());
        assert!(f.create(SparseSolverType::Auto, 2).is_err());
        assert_eq!(
            f.create_centralized(SparseSolverType::Faer).unwrap().kind(),
            SparseSolverType::Faer
        );
        assert!(matches!(
            f.create(SparseSolverType::Mumps, 1),
            Err(Error::Unsupported(_))
        ));
        assert!(f.create(SparseSolverType::Petsc, 1).is_err());
    }

    #[derive(Debug)]
    struct MockMumps;
    impl SparseDirectSolver for MockMumps {
        fn kind(&self) -> SparseSolverType {
            SparseSolverType::Mumps
        }
        fn name(&self) -> &'static str {
            "mumps"
        }
        fn capabilities(&self) -> BackendCapabilities {
            BackendCapabilities {
                matrix: MatrixDistribution::DistributedAssembled,
                ..centralized_capabilities()
            }
        }
        fn analysis(&mut self, _: &SparseMatrixAij) -> Result<()> {
            Ok(())
        }
        fn factorize(&mut self, _: &SparseMatrixAij) -> Result<()> {
            Ok(())
        }
        fn solve(&mut self, _: &[Real], _: &mut [Real]) -> Result<()> {
            Ok(())
        }
    }

    /// `auto` with a registered MUMPS allocator resolves to MUMPS (upstream);
    /// a MUMPS-like backend (distributed matrix) is not a centralised one;
    /// an allocator reporting another kind than its key is refused.
    #[test]
    fn factory_auto_with_registered_mumps_and_no_substitution() {
        let mut f = SparseSolverFactory::default();
        f.register("mumps", || Box::new(MockMumps));
        assert_eq!(f.resolve(SparseSolverType::Auto), SparseSolverType::Mumps);
        let m = f.create(SparseSolverType::Auto, 4).unwrap();
        assert_eq!((m.name(), m.kind()), ("mumps", SparseSolverType::Mumps));
        assert!(matches!(
            f.create_centralized(SparseSolverType::Auto),
            Err(Error::Unsupported(_))
        ));
        // "eigen" silently served by faer: refused
        f.register("eigen", || Box::new(FaerLuSolver::default()));
        assert!(matches!(
            f.create(SparseSolverType::Eigen, 1),
            Err(Error::Backend { .. })
        ));
    }

    #[test]
    fn faer_solves_and_refuses_singular() {
        let mut a = SparseMatrixAij::new("A", 3, MatrixType::Unsymmetric);
        for (i, j, v) in [
            (0, 0, 4.0),
            (0, 1, 1.0),
            (1, 0, 2.0),
            (1, 1, 5.0),
            (2, 2, 3.0),
        ] {
            a.add(i, j, v).unwrap();
        }
        let mut s = FaerLuSolver::default();
        assert!(s.factorize(&a).is_err(), "factorize before analysis");
        s.analysis(&a).unwrap();
        s.factorize(&a).unwrap();
        let mut x = vec![0.0; 3];
        s.solve(&[5.0, 7.0, 6.0], &mut x).unwrap();
        for (xi, e) in x.iter().zip([1.0, 1.0, 2.0]) {
            assert!((xi - e).abs() < 1e-14, "{x:?}");
        }
        assert!(s.solve(&[1.0], &mut x).is_err());
        let mut sing = SparseMatrixAij::new("S", 2, MatrixType::Unsymmetric);
        for (i, j, v) in [(0, 0, 1.0), (0, 1, 2.0), (1, 0, 2.0), (1, 1, 4.0)] {
            sing.add(i, j, v).unwrap();
        }
        let mut s = FaerLuSolver::default();
        s.analysis(&sing).unwrap();
        assert!(matches!(s.factorize(&sing), Err(Error::LinearSolver(_))));
    }
}
