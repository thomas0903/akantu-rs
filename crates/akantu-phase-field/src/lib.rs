// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/* (phase_field_model,
// phasefield, phasefields/*, energy_split*, energy_splits/*,
// phasefield_selector*, phase_field_element_filter.hh).

//! # akantu-phase-field
//!
//! The phase-field fracture model (upstream `PhaseFieldModel`, ported from
//! `src/model/phase_field`): a scalar nodal damage `d ∈ [0, 1]` solved, for
//! a given strain field, from the regularised fracture energy of
//! Ambrosio–Tortorelli type. It runs on one process or on a distributed
//! mesh. Cargo feature: `petsc` (off by default: the PETSc DOF manager and
//! the bounded `petsc_tao` solve; a build with it is distributed under
//! GPL-3.0-or-later as a whole, D-076).
//!
//! * [`PhaseFieldModel`]: the model (`init_full`, static Newton–Raphson
//!   solve of `d`, boundary conditions, energies, selectors), PF-001…004,
//!   PF-011, PF-012;
//! * [`PhaseField`] / [`PhaseFieldBase`]: the law contract and the common
//!   parameters and internals, PF-005;
//! * [`laws`]: `quadratic` (AT2) and `linear` (AT1), PF-006/007,
//!   PF-013…018;
//! * [`energy_split`]: `no_split` and `volumetric_deviatoric`, PF-008…010;
//! * [`selector`]: phase-field selector aliases, PF-011;
//! * [`element_filter`]: [`PhaseFieldElementFilter`] (broken-element
//!   clustering filter), PF-011.
//!
//! # Coupling with solid mechanics
//!
//! Upstream couples the phase field to the solid in a staggered way
//! (`CouplerSolidPhaseField` of `akantu-couplers`, CPL-005…007; the solid side
//! is the `phasefield` material of `akantu-solid`). The phase-field side
//! of that contract is, per law `i` and element type `ty` of the model
//! dimension:
//!
//! 1. strain in (upstream `computeStrainOnQuadPoints`,
//!    `coupler_solid_phasefield.cc:372-407`): flatten the solid's `grad_u`
//!    and `epsilon_th` over all materials into mesh-indexed blocks
//!    (`InternalField::flatten`), form `ε = ½(∇u + ∇uᵀ) − ε_th I`
//!    (column-major `dim × dim` per quadrature point) per mesh element, and
//!    inflate it into every phase-field law's `strain` through that
//!    law's own element filter (`InternalField::inflate` on
//!    `model.phase_field_mut(i)?.base_mut().strain` with
//!    `base().element_filter`; upstream `inflateInternal("strain")`). No
//!    name matching: this is correct even when the solid and phase-field
//!    selectors assign elements differently;
//! 2. solve: `model.solve_step(None)` (and `model.save_previous_state()`
//!    after a converged staggered iteration when the history must advance);
//! 3. damage out: interpolate `model.damage()?` on the solid material's
//!    quadrature points with `model.fe_engine()?` (upstream
//!    `interpolateOnIntegrationPoints(damage, solid_damage, 1, type, ghost)`
//!    without a filter: UD-382, the coupler's own defect) and write it into the `damage` internal of the
//!    material whose name equals the phase field's (name matching is
//!    used for this damage path only, CPL-007).
//!
//! Both models must use the same mesh and quadrature (Lagrange engines of
//! the model dimension, upstream default orders). Upstream's coupler
//! `checkConvergence` (UD-620)
//! (`coupler_solid_phasefield.cc:455-462`) never advances the damage
//! iterators and does not square the differences (`norm2 = n·(d_new[0] −
//! d_old[0])`, `sqrt` of a possibly negative number, then
//! `std::max(norm, NaN)` returns `norm`): the damage criterion is
//! effectively ignored.
//!
//! # Solvers
//!
//! The default solver is `static` (Newton–Raphson on `K`, pseudo-time
//! scheme), created by the solver factory of `akantu-solver` from the
//! model's [`akantu_solver::ModelSolverOptions`] (sparse solver kind
//! included), so the PETSc-backed solvers of the three PETSc-only upstream
//! tests (`_petsc_tao` with bounds, NLS-007) plug in through the same
//! options (feature `petsc`).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod element_filter;
pub mod energy_split;
pub mod foreign;
pub mod laws;
pub mod model;
pub mod phasefield;
pub mod selector;

pub use element_filter::PhaseFieldElementFilter;
pub use energy_split::{EnergySplit, EnergySplitKind};
pub use foreign::{ForeignPhaseField, ForeignPhaseFieldMethods, ForeignPhaseFieldState};
pub use laws::{
    PhaseFieldFactory, PhaseFieldLinear, PhaseFieldQuadratic, register_builtin_phasefields,
};
pub use model::{DEFAULT_ID, DOF_ID, FE_ENGINE_NAME, PhaseFieldModel, PhaseFieldPhysics};
pub use phasefield::{Irreversibility, PhaseField, PhaseFieldBase, PhaseFieldUpstreamCompat};
