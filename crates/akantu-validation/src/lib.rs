// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS). This crate has no
// direct C++ counterpart: it is the differential-testing harness that
// validates akantu-rs against the Akantu C++ reference ("oracle").

//! # akantu-validation
//!
//! Golden-fixture format, loader and differential comparison harness used
//! to validate akantu-rs against the Akantu C++ oracle. The methodology and
//! the on-disk format are specified in `docs/NUMERICAL_VALIDATION.md`; the
//! C++/Python probes producing fixtures live in `oracle/probes/`.
//!
//! * [`npy`]: bit-exact NumPy `.npy` reader/writer (array container);
//! * [`manifest`]: `manifest.json` schema and validation;
//! * [`fixture`]: fixture discovery, checksum verification, loading;
//! * [`tolerance`]: justified tolerance specs and `tolerances.json`;
//! * [`compare`]: entry-wise comparison (abs/rel/ULP, NaN/Inf, shapes);
//! * [`exact`]: exact (bitwise) comparison of `f64` values and digest
//!   canonicalisation: the single implementation of the default-NaN
//!   convention (decision D-046);
//! * [`reduce`]: NaN-propagating maxima/minima for test pass conditions
//!   (`f64::max`/`f64::min` skip NaN);
//! * [`report`]: structured reports (text + JSON);
//! * [`ordering`]: numbering normalisation (ids, coordinates,
//!   connectivity, ghosts, set semantics);
//! * [`harness`]: [`DiffTest`], the high-level test driver;
//! * [`regression`]: persisting failing inputs as new cases;
//! * [`solver`]: solver acceptance: the port's own stopping decision
//!   against the solver rule recorded in a fixture;
//! * [`verified`]: reuse of upstream `.verified` text outputs;
//! * [`writer`]: writing fixtures from Rust.
//!
//! This crate is a leaf utility: it does not depend on any other akantu-rs
//! crate, so every crate can use it in its tests. It has no upstream
//! counterpart and no Cargo features.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod array;
pub mod checksum;
pub mod compare;
pub mod error;
pub mod exact;
pub mod fixture;
pub mod harness;
pub mod manifest;
pub mod npy;
pub mod ordering;
pub mod reduce;
pub mod regression;
pub mod report;
pub mod solver;
pub mod tolerance;
pub mod verified;
pub mod writer;

pub use array::Field;
pub use compare::{CompareOptions, compare_f64, compare_i64, ulp_distance};
pub use error::{Error, Result};
pub use exact::{canonical_bits, canonical_le_bytes, exact_eq};
pub use fixture::{Fixture, fixtures_root};
pub use harness::DiffTest;
pub use manifest::{FixtureStatus, Layout, Manifest, Quantity};
pub use report::{CaseReport, Outcome, QuantityReport};
pub use solver::{SolverAcceptance, SolverStep, StoppingCriterion};
pub use tolerance::{NanPolicy, ResolvedTolerance, Scale, ToleranceSpec};
