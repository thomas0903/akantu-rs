// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/common/non_local_toolbox/
// (non_local_manager.*, non_local_manager_callback.hh,
// non_local_neighborhood*.hh, neighborhood_base.*, base_weight_function.*,
// neighborhoods_criterion_evaluation/neighborhood_max_criterion.*).

//! Non-local averaging (NL-001…004, NL-009).
//!
//! The manager lives below the physics crates and talks to the laws through
//! object-safe interfaces, so `akantu-model` never imports `akantu-solid`:
//!
//! * [`NonLocalManagerCallback`] — implemented by the model owning the
//!   laws (upstream `NonLocalManagerCallback`): registration of the
//!   neighbourhoods and variables and insertion of the integration points
//!   (`insertIntegrationPointsInNeighborhoods`), flattening of the law
//!   internals (`updateLocalInternal`), copy-back of the averages
//!   (`updateNonLocalInternal`), non-local stresses
//!   (`computeNonLocalContribution`);
//! * [`WeightFunction`] — `base_wf` here ([`BaseWeightFunction`]); the
//!   damage-dependent ones (`damage_wf`, `remove_wf`, …) are solid laws
//!   registered into the manager's [`WeightFunctionFactory`] by the solid
//!   model. A weight function's **internals** (e.g. `damage`) are flattened
//!   from the laws before every weight update and exchanged on ghosts,
//!   never averaged;
//! * [`NonLocalGhostExchange`] — the ghost machinery (upstream grid
//!   synchronisers); serial: [`NoGhosts`].
//!
//! # Data layout
//!
//! Upstream keeps per-point data in `ElementTypeMapArray`s indexed by
//! `(type, ghost, global_num)`. The manager uses one slot per integration
//! point of every `(type, ghost)` block of the mesh ([`PointLayout`]):
//! volumes, integration weights, flattened variables and averages are
//! per-slot arrays; the callback still exchanges `ElementTypeMapArray`s
//! with the laws, exactly upstream's flattened arrays.
//!
//! # Cycle (upstream `computeAllNonLocalContribution`)
//!
//! flatten the local variables → start the ghost exchange (`ForAverage`) →
//! refresh the weight-function internals, zero the volumes and recompute
//! the weights of every neighbourhood whose update is due
//! (`compute_stress_calls % update_rate == 0`) → average the owned pairs →
//! wait → average the ghost pairs → copy the averages back → non-local
//! stresses → `compute_stress_calls += 1`. The arithmetic (pair order, two
//! weights per pair, reciprocal normalisation, componentwise accumulation)
//! is upstream's, see [`neighborhood`]; [`SummationOrder::KeyOrder`] is the
//! opt-in partition-independent route (D-008).
//!
//! # Variables (UD-558)
//!
//! Laws declare their averaged variables per neighbourhood
//! ([`NonLocalManager::register_neighborhood_variable`]); by default
//! ([`VariableRegistration::PerNeighborhood`]) every neighbourhood averages
//! what its laws declared, [`VariableRegistration::UpstreamShared`] keeps
//! upstream's single map (compat). [`NonLocalOptions::upstream`] sets every
//! upstream-defect switch of the manager (UD-037, UD-557 through
//! [`WeightArithmetic`], UD-558, UD-561).
//!
//! # Rebuild contract
//!
//! Weight functions and variables are registered once and kept across
//! rebuilds. A topology event on elements of the model's dimension
//! invalidates the neighbourhoods; averaging is refused with an error until
//! [`NonLocalManager::initialize`] rebuilt them (it may be called any number
//! of times). There is no silent no-op cycle.
//!
//! # Parallel runs
//!
//! A distributed model initialises the manager in stages
//! ([`NonLocalManager::begin_initialize`], then the grid synchronisers —
//! [`ghosts`]: one per agreed global neighbourhood name, built from
//! [`NeighborhoodGridCells`] by a mesh mutation of the model —, the law ids
//! of the new ghost elements, [`NonLocalManager::insert_ghost_points`],
//! [`NonLocalManager::update_pair_lists`], the removal of the ghost elements
//! in no pair ([`NonLocalManager::relevant_ghost_elements`],
//! [`NonLocalManager::renumber_ghost_points`]) and
//! [`NonLocalManager::finish_initialize`]); the cycle then exchanges the
//! flattened variables (`_mnl_for_average`) and the weight-function
//! internals (`_mnl_weight`) at the ghost slots over [`GridGhosts`]
//! ([`GridGhostExchange`]). The solid model drives it
//! (`akantu-solid/src/model/non_local.rs`), with the upstream defects of
//! that path (UD-1100…1103) documented there. The point identity and the
//! upstream summation route are local (owned pairs only compare owned
//! points); [`SummationOrder::KeyOrder`] needs global element ids
//! ([`QuadPoint::global_element`]) that a distributed mesh does not carry,
//! and is refused there.
pub mod ghosts;
mod manager;
pub mod max_criterion;
pub mod neighborhood;
mod point;
mod weight;

pub use ghosts::{
    EmptyGridCells, GridGhostExchange, GridGhosts, NeighborhoodGridCells,
    agree_neighborhood_names_in,
};
pub use manager::{
    FePointSource, MissingNonLocalInternal, NoGhosts, NonLocalGhostExchange, NonLocalManager,
    NonLocalManagerCallback, NonLocalOptions, NonLocalPointSource, NonLocalTag, NonLocalVariable,
    PointLayout, SummationOrder, VariableRegistration, WeightExchange,
};
pub use neighborhood::{NonLocalNeighborhood, distance};
pub use point::{PointIdentity, PointKey, QuadPoint};
pub use weight::{
    BaseWeightFunction, HasWeightBase, PointRef, WeightArithmetic, WeightFunction,
    WeightFunctionBase, WeightFunctionConstructor, WeightFunctionFactory, WeightInternals,
    base_weight, set_update_rate,
};
