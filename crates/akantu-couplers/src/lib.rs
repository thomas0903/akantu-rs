// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/model_couplers/*
// (coupler_solid_contact{.hh,.cc,_tmpl.hh}, coupler_solid_cohesive_contact{.hh,.cc},
// cohesive_contact_solvercallback{.hh,.cc}, coupler_solid_phasefield{.hh,.cc}).

//! # akantu-couplers
//!
//! The model couplers of upstream's `model_couplers` package
//! (`src/model/model_couplers`, parity rows CPL-001…007, D-021). Cargo
//! features, off by default: `petsc` (the models on the PETSc DOF manager;
//! a build with it is distributed under GPL-3.0-or-later as a whole, D-076),
//! `mpi` and `scotch` (test builds under real MPI and with Scotch). Two
//! coupling styles:
//!
//! * monolithic: solid–contact ([`CouplerSolidContact`], CPL-001/002)
//!   and solid–cohesive–contact ([`CouplerSolidCohesiveContact`], CPL-003):
//!   one `ModelSolver` (the solid's DOF manager, as upstream passes one
//!   `DOFManager` to both parts) driven by a combined solver callback
//!   ([`SolidContactCallback`]) that calls the parts in upstream order; the
//!   contact part of the cohesive coupler lives on the facet mesh.
//!   [`CohesiveContactSolverCallback`] (CPL-004) is the stand-alone
//!   callback of upstream, a thin adapter over the same implementation;
//! * staggered: solid–phase-field ([`CouplerSolidPhaseField`],
//!   CPL-005…007): two complete models, each with its own `ModelSolver`,
//!   solver ids and time steps; `solve` runs solid solve → strain transfer
//!   → phase-field solve → damage transfer
//!   (`coupler_solid_phasefield.cc:410-425`).
//!
//! Every coupler owns its component models by value (public fields: the
//! borrowed handles of upstream's `getSolidMechanicsModel()`,
//! `getContactMechanicsModel()`, `getPhaseFieldModel()`).
//!
//! Upstream defects of this package (`docs/UPSTREAM_DEFECTS.md`): UD-382
//! (unfiltered damage transfer, category B, corrected by default with a
//! compat switch), UD-617…624, UD-374 (per-iterate friction history of the
//! static / implicit couplers, B) and UD-772…776.
//!
//! The two contact couplers run on one process, like the contact model
//! (CONT-019): on a distributed mesh the contact model refuses its entry
//! points. The solid–phase-field coupler also runs on a distributed mesh.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod contact;
pub mod dump;
pub mod phase_field;

pub use contact::{
    CohesiveContactSolverCallback, CohesiveForwarder, CouplerSolidCohesiveContact,
    CouplerSolidContact, SolidContactCallback, SolidContactCompat,
};
pub use phase_field::{CouplerSolidPhaseField, CouplerSolidPhaseFieldCompat};
