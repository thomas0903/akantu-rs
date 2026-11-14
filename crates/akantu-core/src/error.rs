// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_error.hh (debug::Exception,
// AKANTU_EXCEPTION, AKANTU_TO_IMPLEMENT), src/model/common/non_linear_solver/
// non_linear_solver.hh (debug::NLSNotConvergedException).

//! The workspace-wide error type and error-handling policy (D-018).
//!
//! Policy (binding for every crate):
//! * Library code returns [`Result<T>`]; crate-specific error enums must
//!   implement `From<TheirError> for akantu_core::Error` (or be variants of it).
//! * No `panic!`/`unwrap()`/`expect()` on anything a user, an input file, a
//!   mesh file or a remote MPI rank can influence.
//! * Internal invariants use `debug_assert!` (upstream `AKANTU_DEBUG_ASSERT`);
//!   indexing panics on out-of-bounds are acceptable only for internal
//!   invariants already validated at the API boundary.
//! * Paths that upstream leaves unimplemented (`AKANTU_TO_IMPLEMENT()`) return
//!   [`Error::NotImplemented`], never `todo!()`.
//! * Errors crossing MPI must be agreed on collectively (see
//!   `akantu-comm`), so that one failing rank does not dead-lock the others.

use crate::element_type::ElementType;

/// Convenience alias used throughout the workspace.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Workspace-wide error type.
#[derive(Debug, thiserror::Error)]
#[non_exhaustive]
pub enum Error {
    /// An argument violates a documented precondition.
    #[error("invalid argument: {0}")]
    InvalidArgument(String),

    /// An index is outside the valid range.
    #[error("{what} index {index} out of range (len {len})")]
    OutOfRange {
        /// What was indexed (e.g. "node", "element", "component").
        what: &'static str,
        /// Offending index.
        index: usize,
        /// Valid length.
        len: usize,
    },

    /// A named entity (array, group, material, solver, parameter…) is missing.
    #[error("{kind} '{id}' not found")]
    NotFound {
        /// Entity kind.
        kind: &'static str,
        /// Requested id.
        id: String,
    },

    /// A named entity already exists.
    #[error("{kind} '{id}' already exists")]
    AlreadyExists {
        /// Entity kind.
        kind: &'static str,
        /// Duplicate id.
        id: String,
    },

    /// Operation not supported for an element type.
    #[error("element type {ty} is not supported by {context}")]
    UnsupportedElementType {
        /// Element type.
        ty: ElementType,
        /// The operation or subsystem.
        context: &'static str,
    },

    /// Shape mismatch between arrays/matrices.
    #[error("dimension mismatch in {context}: expected {expected}, got {got}")]
    DimensionMismatch {
        /// Operation.
        context: &'static str,
        /// Expected size.
        expected: usize,
        /// Actual size.
        got: usize,
    },

    /// Upstream also leaves this path unimplemented (`AKANTU_TO_IMPLEMENT`).
    #[error("not implemented: {0}")]
    NotImplemented(&'static str),

    /// Input-file / mesh-file syntax or semantic error.
    #[error("parse error in {file}:{line}:{column}: {message}")]
    Parse {
        /// File name (or `<string>`).
        file: String,
        /// 1-based line.
        line: usize,
        /// 1-based column.
        column: usize,
        /// Description.
        message: String,
    },

    /// I/O failure.
    #[error("I/O error on '{path}': {source}")]
    Io {
        /// Path concerned.
        path: String,
        /// Underlying error.
        #[source]
        source: std::io::Error,
    },

    /// Inconsistent mesh topology or mesh state.
    #[error("mesh error: {0}")]
    Mesh(String),

    /// Communication (MPI) failure.
    #[error("communication error: {0}")]
    Communication(String),

    /// Non-linear solver did not converge (upstream
    /// `debug::NLSNotConvergedException`).
    #[error(
        "non-linear solver did not converge: threshold {threshold:e}, {niter} iterations, error {error:e}"
    )]
    NotConverged {
        /// Convergence threshold.
        threshold: f64,
        /// Iterations performed.
        niter: usize,
        /// Final error measure.
        error: f64,
    },

    /// Linear-solver / factorisation failure (singular matrix, backend error).
    #[error("linear solver error: {0}")]
    LinearSolver(String),

    /// Numerical failure (NaN/Inf detected, negative Jacobian, …).
    #[error("numerical error: {0}")]
    Numerical(String),

    /// Error raised by an optional native backend (MUMPS, PETSc, Scotch, MPI).
    #[error("{backend} error (code {code}): {message}")]
    Backend {
        /// Backend name.
        backend: &'static str,
        /// Backend error code.
        code: i64,
        /// Description.
        message: String,
    },

    /// Error originating from user code (Python callbacks, user materials).
    #[error("user error: {0}")]
    User(String),

    /// Feature declared upstream but not supported by it either (e.g.
    /// `_miot_abaqus`, UD-049).
    #[error("unsupported: {0}")]
    Unsupported(String),

    /// A size, count or index exceeds a representable limit (checked size
    /// arithmetic, index widths, native-API widths, allocation limits). Raised
    /// **before** any state is modified.
    #[error("{what}: {requested} exceeds the limit {limit}")]
    CapacityExceeded {
        /// The quantity (e.g. "array storage", "local DOF count").
        what: &'static str,
        /// Requested value (exact; products are computed in 128 bits).
        requested: u128,
        /// Largest representable / allowed value.
        limit: u128,
    },

    /// A negative or otherwise malformed index (other than an accepted
    /// sentinel) in user, file or foreign data.
    #[error("invalid {what} {value}")]
    InvalidIndex {
        /// What was indexed.
        what: &'static str,
        /// Offending value.
        value: i64,
    },

    /// A collective operation left its owner (DOF manager, mesh domain, …)
    /// **unusable**: the owner is poisoned and refuses every further use.
    /// This is the explicit signal for the top-level
    /// runtime's abort policy — never parse error strings for it:
    /// [`FatalKind::Transport`] requires `MPI_Abort` (peers may be blocked
    /// in a collective), [`FatalKind::Agreed`] was agreed on every rank (an
    /// orderly shutdown or a rebuild of the simulation is possible). Never
    /// continue on the poisoned object.
    #[error("fatal ({kind:?}): {source}")]
    Fatal {
        /// Why the owner is unusable.
        kind: FatalKind,
        /// The error that made it unusable (or, for later calls, the
        /// poisoning record).
        #[source]
        source: Box<Error>,
    },
}

/// Class of an [`Error::Fatal`]. Combining two fatal
/// classifications keeps the more urgent one ([`FatalKind::most_urgent`],
/// used by [`Error::fatal`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum FatalKind {
    /// A communication primitive failed (local-world timeout, MPI error):
    /// the communicator state is unknown and only the ranks that observed
    /// the failure know it. The runtime must abort.
    Transport,
    /// A failure after an irreversible change (committed numbering,
    /// symmetry promotion on some ranks, consumer update after a mesh
    /// commit), agreed on every rank: every rank's owner is poisoned.
    Agreed,
}

impl FatalKind {
    /// The more urgent of two classifications: `Transport` (the runtime
    /// must abort) over `Agreed`.
    #[must_use]
    pub fn most_urgent(self, other: FatalKind) -> FatalKind {
        if self == FatalKind::Transport || other == FatalKind::Transport {
            FatalKind::Transport
        } else {
            FatalKind::Agreed
        }
    }
}

impl Error {
    /// Stable integer code for the C API (`akantu-capi`). `0` is reserved for
    /// success. New variants must append new codes, never renumber.
    #[must_use]
    pub fn code(&self) -> i32 {
        match self {
            Error::InvalidArgument(_) => 1,
            Error::OutOfRange { .. } => 2,
            Error::NotFound { .. } => 3,
            Error::AlreadyExists { .. } => 4,
            Error::UnsupportedElementType { .. } => 5,
            Error::DimensionMismatch { .. } => 6,
            Error::NotImplemented(_) => 7,
            Error::Parse { .. } => 8,
            Error::Io { .. } => 9,
            Error::Mesh(_) => 10,
            Error::Communication(_) => 11,
            Error::NotConverged { .. } => 12,
            Error::LinearSolver(_) => 13,
            Error::Numerical(_) => 14,
            Error::Backend { .. } => 15,
            Error::User(_) => 16,
            Error::Unsupported(_) => 17,
            Error::CapacityExceeded { .. } => 18,
            Error::InvalidIndex { .. } => 19,
            Error::Fatal { .. } => 20,
        }
    }

    /// Wrap `source` as an [`Error::Fatal`] of `kind`. An error that
    /// already is fatal keeps its source and the **more urgent** of the two
    /// kinds (`Transport` over `Agreed`): a transport
    /// failure observed anywhere on the way keeps the runtime's abort
    /// signal, an agreed fatal error never becomes recoverable.
    #[must_use]
    pub fn fatal(kind: FatalKind, source: Error) -> Self {
        match source {
            Error::Fatal { kind: k, source } => Error::Fatal {
                kind: k.most_urgent(kind),
                source,
            },
            e => Error::Fatal {
                kind,
                source: Box::new(e),
            },
        }
    }

    /// Combine two errors of **one** outcome:
    /// `self` keeps its diagnostic (source and message), the result carries
    /// the most urgent classification of both — `Transport` over `Agreed`
    /// over recoverable — so a fatal signal carried by `other` is never
    /// lost behind an earlier or more specific error.
    #[must_use]
    pub fn escalated_by(self, other: &Error) -> Self {
        match other.fatal_kind() {
            Some(kind) => Error::fatal(kind, self),
            None => self,
        }
    }

    /// `Some(kind)` for an [`Error::Fatal`]: the owner that returned it is
    /// unusable (the runtime's abort policy keys on this).
    #[must_use]
    pub fn fatal_kind(&self) -> Option<FatalKind> {
        match self {
            Error::Fatal { kind, .. } => Some(*kind),
            _ => None,
        }
    }

    /// `true` for an [`Error::Fatal`].
    #[must_use]
    pub fn is_fatal(&self) -> bool {
        self.fatal_kind().is_some()
    }

    /// `true` for [`Error::NotFound`] (an absent entry or block).
    #[must_use]
    pub fn is_not_found(&self) -> bool {
        matches!(self, Error::NotFound { .. })
    }

    /// Helper for [`Error::NotFound`].
    pub fn not_found(kind: &'static str, id: impl Into<String>) -> Self {
        Error::NotFound {
            kind,
            id: id.into(),
        }
    }

    /// Helper for [`Error::InvalidArgument`].
    pub fn invalid(msg: impl Into<String>) -> Self {
        Error::InvalidArgument(msg.into())
    }
}

/// `Ok(None)` for an [`Error::NotFound`] result (an absent block, e.g. a
/// type without elements), the value as `Some` otherwise; every other error
/// is returned. Code that skips absent blocks uses it so that a protocol
/// violation of an exported array is not mistaken for absence.
pub fn if_present<T>(r: Result<T>) -> Result<Option<T>> {
    match r {
        Ok(v) => Ok(Some(v)),
        Err(e) if e.is_not_found() => Ok(None),
        Err(e) => Err(e),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_unique_and_nonzero() {
        let errs = [
            Error::invalid("x"),
            Error::OutOfRange {
                what: "node",
                index: 1,
                len: 0,
            },
            Error::not_found("material", "steel"),
            Error::AlreadyExists {
                kind: "group",
                id: "g".into(),
            },
            Error::UnsupportedElementType {
                ty: ElementType::Point1,
                context: "test",
            },
            Error::DimensionMismatch {
                context: "t",
                expected: 1,
                got: 2,
            },
            Error::NotImplemented("x"),
            Error::Parse {
                file: "f".into(),
                line: 1,
                column: 1,
                message: "m".into(),
            },
            Error::Io {
                path: "p".into(),
                source: std::io::Error::other("e"),
            },
            Error::Mesh("m".into()),
            Error::Communication("c".into()),
            Error::NotConverged {
                threshold: 1e-10,
                niter: 3,
                error: 1.0,
            },
            Error::LinearSolver("s".into()),
            Error::Numerical("n".into()),
            Error::Backend {
                backend: "mumps",
                code: -9,
                message: "m".into(),
            },
            Error::User("u".into()),
            Error::Unsupported("abaqus".into()),
            Error::CapacityExceeded {
                what: "w",
                requested: 2,
                limit: 1,
            },
            Error::InvalidIndex {
                what: "node",
                value: -2,
            },
            Error::fatal(FatalKind::Agreed, Error::Communication("c".into())),
        ];
        let mut codes: Vec<i32> = errs.iter().map(Error::code).collect();
        assert!(codes.iter().all(|&c| c > 0));
        codes.sort_unstable();
        codes.dedup();
        assert_eq!(codes.len(), errs.len());
        assert_eq!(
            Error::not_found("material", "steel").to_string(),
            "material 'steel' not found"
        );
    }

    /// Re-wrapping keeps the source and the more urgent kind.
    #[test]
    fn fatal_wrapping_keeps_the_most_urgent_kind() {
        let agreed = Error::fatal(FatalKind::Agreed, Error::invalid("x"));
        let t = Error::fatal(FatalKind::Transport, agreed);
        assert_eq!(t.fatal_kind(), Some(FatalKind::Transport));
        assert_eq!(t.to_string(), "fatal (Transport): invalid argument: x");
        let back = Error::fatal(FatalKind::Agreed, t);
        assert_eq!(back.fatal_kind(), Some(FatalKind::Transport));
        assert_eq!(
            FatalKind::Agreed.most_urgent(FatalKind::Agreed),
            FatalKind::Agreed
        );
    }

    /// Combining keeps the first diagnostic and the most urgent
    /// classification of both errors.
    #[test]
    fn escalation_keeps_the_diagnostic_and_the_most_urgent_kind() {
        let plain = || Error::invalid("callback");
        let agreed = || Error::fatal(FatalKind::Agreed, Error::invalid("driver"));
        let transport = || Error::fatal(FatalKind::Transport, Error::invalid("driver"));
        let e = plain().escalated_by(&transport());
        assert_eq!(e.fatal_kind(), Some(FatalKind::Transport));
        assert_eq!(
            e.to_string(),
            "fatal (Transport): invalid argument: callback"
        );
        assert_eq!(
            plain().escalated_by(&agreed()).fatal_kind(),
            Some(FatalKind::Agreed)
        );
        assert_eq!(
            agreed().escalated_by(&transport()).fatal_kind(),
            Some(FatalKind::Transport)
        );
        assert_eq!(
            transport().escalated_by(&agreed()).fatal_kind(),
            Some(FatalKind::Transport)
        );
        assert_eq!(
            agreed().escalated_by(&plain()).fatal_kind(),
            Some(FatalKind::Agreed)
        );
        assert_eq!(plain().escalated_by(&plain()).fatal_kind(), None);
    }
}
