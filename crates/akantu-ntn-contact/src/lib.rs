// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/traction-at-split-node-contact/
// (package.cmake, README.txt, src/ntn_contact_solver.hh and the sources of the
// modules below).

//! # akantu-ntn-contact
//!
//! Port of upstream's `traction-at-split-node-contact` extra package
//! (`extra_packages/traction-at-split-node-contact`, parity rows
//! TASN-000…043): explicit-dynamics contact and friction at split nodes for
//! the [`akantu_solid::SolidMechanicsModel`] (Kammer's "traction at split
//! node" method). Cargo features, off by default: `mpi` and `scotch` (test
//! builds under real MPI and with Scotch); the facade enables the crate with
//! its feature `ntn-contact`.
//!
//! * [`contact`]: node-to-node pairs, node-to-rigid-flat and mode-II
//!   antisymmetric contact ([`NtnBaseContact`]);
//! * [`friction`]: friction machinery, laws and regularisations
//!   ([`NtnFriction`]);
//! * [`factory`]: friction factory from the input file
//!   ([`initialize_ntn_friction`]);
//! * [`solver_callback`]: the solver callback that adds the contact and
//!   friction forces to the residual of an explicit step
//!   ([`NtnContactSolverCallback`]);
//! * [`boundary_conditions`], [`boundary_functions`], [`node_filter`]:
//!   boundary-condition helpers of the package;
//! * [`sync_array`], [`parameter_reader`], [`manual_restart`]: support code.
//!
//! On a distributed mesh, [`NtnBaseContact::init_parallel`] (upstream
//! `initParallel`, MPI-051/052) enables the `_cf_nodal` / `_cf_incr`
//! exchanges ([`parallel`]); without it a contact step is refused (UD-904).
//! The manual restart helpers ([`manual_restart`]) stay serial.
//!
//! Upstream defects of the package and their handling: `docs/UPSTREAM_DEFECTS.md`
//! UD-039 and UD-392…406; the compatibility switches reproducing upstream are
//! [`TasnUpstreamCompat`].

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod boundary_conditions;
pub mod boundary_functions;
pub mod contact;
pub mod dump;
pub mod factory;
pub mod friction;
pub mod manual_restart;
pub mod node_filter;
pub mod parallel;
pub mod parameter_reader;
pub mod solver_callback;
pub mod state;
pub mod sync_array;

pub use contact::{ContactKind, ContactVariant, ElementBlocks, NtnBaseContact};
pub use dump::ContactDumper;
pub use factory::{
    allocate_friction, friction_law_from_name, initialize_ntn_friction,
    initialize_ntn_friction_with, registered_friction_laws,
};
pub use friction::{FrictionLaw, NtnFriction, Regularisation, RsCoefficient, RsState};
pub use solver_callback::NtnContactSolverCallback;
pub use state::ModelState;
pub use sync_array::{SyncChoice, SyncJournal, SyncRecord, SynchronizedArray};

/// Upstream-defect switches of the package (D-003 category B). The default
/// (`false` everywhere) is the corrected behaviour; differential replays
/// against the unpatched oracle set the switch of the defect a case
/// reproduces, or all of them ([`TasnUpstreamCompat::upstream`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TasnUpstreamCompat {
    /// **UD-039 (selector):** every regularisation option other than
    /// `simplified_prakash_clifton` (including the default
    /// `no_regularisation` and unknown names) allocates the Rubin–Ampuero
    /// regularisation.
    pub regularisation_fallback_rubin_ampuero: bool,
    /// **UD-039 (zero speed):** a rate-and-state coefficient that is not
    /// finite at a node in contact (e.g. `ln 0` of the Dieterich and
    /// N-shaped laws at zero slip velocity) enters the strength and the
    /// force assembly instead of raising a domain error.
    pub rate_and_state_non_finite_propagates: bool,
    /// **UD-392:** node pairing keeps searching after a match, so a slave
    /// close to several masters appears in several pairs.
    pub pairing_keeps_searching: bool,
    /// **UD-394:** reading a restart file leaves the array values untouched.
    pub restart_read_ignores_values: bool,
    /// **UD-395:** the friction restart does not read the `slip` file.
    pub friction_restart_skips_slip: bool,
    /// **UD-397:** the simplified Prakash–Clifton regularisation divides by
    /// `t* = 0` (NaN strength) instead of using the unregularised limit.
    pub spc_zero_t_star_nan: bool,
    /// **UD-398:** `boundaryFix` fails on groups that exist and does not
    /// create the missing ones.
    pub boundary_fix_inverted: bool,
}

impl TasnUpstreamCompat {
    /// Every switch on: the reference behaviour.
    #[must_use]
    pub fn upstream() -> Self {
        Self {
            regularisation_fallback_rubin_ampuero: true,
            rate_and_state_non_finite_propagates: true,
            pairing_keeps_searching: true,
            restart_read_ignores_values: true,
            friction_restart_skips_slip: true,
            spc_zero_t_star_nan: true,
            boundary_fix_inverted: true,
        }
    }
}
