// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This file has no
// direct C++ counterpart: it belongs to the validation harness (see
// docs/NUMERICAL_VALIDATION.md).

//! Error type of the validation crate.
//!
//! This crate is a leaf utility and deliberately does not depend on
//! `akantu-core`; its error type is self-contained and implements
//! [`std::error::Error`] so that it converts into any `Box<dyn Error>`.

use std::fmt;
use std::path::PathBuf;

/// Result alias used throughout the crate.
pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Errors raised while loading fixtures, parsing arrays or writing reports.
#[derive(Debug)]
#[non_exhaustive]
pub enum Error {
    /// Underlying I/O failure, with the path involved.
    Io {
        /// File or directory being accessed.
        path: PathBuf,
        /// The original error.
        source: std::io::Error,
    },
    /// JSON (de)serialisation failure.
    Json {
        /// File being parsed or written (empty when in-memory).
        path: PathBuf,
        /// The original error.
        source: serde_json::Error,
    },
    /// Malformed or unsupported `.npy` file.
    Npy {
        /// File being parsed (empty when parsing from memory).
        path: PathBuf,
        /// Human-readable reason.
        reason: String,
    },
    /// The manifest is syntactically valid JSON but violates the fixture
    /// format specification.
    Manifest {
        /// Manifest path.
        path: PathBuf,
        /// Human-readable reason.
        reason: String,
    },
    /// A file's SHA-256 digest does not match the one recorded in the manifest.
    Checksum {
        /// File whose digest was checked.
        path: PathBuf,
        /// Digest recorded in the manifest.
        expected: String,
        /// Digest of the file on disk.
        actual: String,
    },
    /// A quantity requested by name is not present in the fixture manifest.
    UnknownQuantity {
        /// Fixture case identifier.
        case_id: String,
        /// Requested quantity name.
        name: String,
    },
    /// The fixture is registered as externally stored and has not been
    /// generated or downloaded locally.
    FixtureNotMaterialized {
        /// Fixture directory that was expected.
        path: PathBuf,
        /// Command that reproduces it.
        hint: String,
    },
    /// No tolerance is defined for a quantity (tolerances are never implicit).
    MissingTolerance {
        /// Fixture case identifier.
        case_id: String,
        /// Quantity name.
        quantity: String,
    },
    /// A tolerance specification is invalid (e.g. negative bound, empty
    /// justification).
    InvalidTolerance {
        /// Quantity name, or a description of where the spec came from.
        context: String,
        /// Human-readable reason.
        reason: String,
    },
    /// Array shapes or data lengths are inconsistent.
    Shape {
        /// Human-readable reason.
        reason: String,
    },
    /// Ordering normalisation could not build a bijection.
    Ordering {
        /// Human-readable reason.
        reason: String,
    },
    /// The solver stopping rule recorded in a fixture (`parameters.solver`)
    /// is missing, malformed or has no checkable rule.
    SolverCriterion {
        /// Fixture case identifier (empty when parsed from a bare value).
        case_id: String,
        /// Human-readable reason.
        reason: String,
    },
    /// The case holds platform variants (decision D-073 (1): golden values
    /// that depend on the numerical environment) and none of them belongs to
    /// the running environment, or the running environment is unknown.
    FixtureEnvironment {
        /// Fixture case identifier.
        case_id: String,
        /// The running numerical environment ([`crate::fixture::running_environment`]),
        /// `None` when it is unknown.
        environment: Option<String>,
        /// Environments the case has golden values for.
        available: Vec<String>,
    },
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Error::Io {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn json(path: impl Into<PathBuf>, source: serde_json::Error) -> Self {
        Error::Json {
            path: path.into(),
            source,
        }
    }

    pub(crate) fn npy(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Error::Npy {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn manifest(path: impl Into<PathBuf>, reason: impl Into<String>) -> Self {
        Error::Manifest {
            path: path.into(),
            reason: reason.into(),
        }
    }

    pub(crate) fn shape(reason: impl Into<String>) -> Self {
        Error::Shape {
            reason: reason.into(),
        }
    }

    pub(crate) fn ordering(reason: impl Into<String>) -> Self {
        Error::Ordering {
            reason: reason.into(),
        }
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Error::Io { path, source } => write!(f, "I/O error on {}: {source}", path.display()),
            Error::Json { path, source } => {
                write!(f, "JSON error in {}: {source}", path.display())
            }
            Error::Npy { path, reason } => write!(f, "invalid .npy {}: {reason}", path.display()),
            Error::Manifest { path, reason } => {
                write!(f, "invalid fixture manifest {}: {reason}", path.display())
            }
            Error::Checksum {
                path,
                expected,
                actual,
            } => write!(
                f,
                "checksum mismatch for {}: manifest says sha256={expected}, file has sha256={actual}",
                path.display()
            ),
            Error::UnknownQuantity { case_id, name } => {
                write!(f, "fixture `{case_id}` has no quantity named `{name}`")
            }
            Error::FixtureNotMaterialized { path, hint } => write!(
                f,
                "fixture {} is not present locally (externally stored); reproduce it with: {hint}",
                path.display()
            ),
            Error::MissingTolerance { case_id, quantity } => write!(
                f,
                "no tolerance defined for quantity `{quantity}` of fixture `{case_id}` \
                 (tolerances are never implicit: add an entry to tolerances.json with a justification)"
            ),
            Error::InvalidTolerance { context, reason } => {
                write!(f, "invalid tolerance for {context}: {reason}")
            }
            Error::Shape { reason } => write!(f, "shape error: {reason}"),
            Error::Ordering { reason } => write!(f, "ordering normalisation failed: {reason}"),
            Error::SolverCriterion { case_id, reason } => {
                write!(f, "solver stopping rule of fixture `{case_id}`: {reason}")
            }
            Error::FixtureEnvironment {
                case_id,
                environment,
                available,
            } => match environment {
                Some(env) => write!(
                    f,
                    "fixture `{case_id}` has no golden values for the numerical environment \
                     `{env}` (available: {}); set AKANTU_FIXTURE_ENV to one of them only for \
                     a diagnostic run",
                    available.join(", ")
                ),
                None => write!(
                    f,
                    "fixture `{case_id}` has platform variants (D-073; available: {}) but the \
                     running numerical environment is unknown (architecture or OPENBLAS_CORETYPE \
                     without a recorded variant); set AKANTU_FIXTURE_ENV",
                    available.join(", ")
                ),
            },
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Io { source, .. } => Some(source),
            Error::Json { source, .. } => Some(source),
            _ => None,
        }
    }
}
