// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/{model.hh, model.cc,
// model_inline_impl.hh, model_options.hh}, src/model/common/
// {boundary_condition/, constitutive_laws/, non_local_toolbox/}.

//! # akantu-model
//!
//! The framework shared by every physics model, ported from upstream's
//! `src/model` (`model`, `model_options`) and `src/model/common`
//! (`boundary_condition`, `constitutive_laws`, `non_local_toolbox`). The
//! crate has no Cargo features. Contents:
//!
//! * [`model::Model`]: the model trait (`init_full`, `solve_step`,
//!   `assemble_*`, dumping, FE-engine registry) and [`model::ModelOptions`].
//! * [`constitutive_law`]: the generic constitutive-law framework
//!   (`ConstitutiveLaw` trait, `ConstitutiveLawsHandler`, selectors,
//!   factory registry) used by solid materials, diffusion laws and
//!   phase-field laws (upstream `ConstitutiveLaw<Handler>`).
//! * [`internal_field`]: per-quadrature-point state with optional history
//!   (upstream `InternalField<T>`, `RandomInternalField`).
//! * [`boundary_condition`]: Dirichlet/Neumann functors applied on groups
//!   (upstream `BoundaryCondition`, `BC::Dirichlet::*`, `BC::Neumann::*`),
//!   including Python-defined functors.
//! * [`non_local`]: non-local averaging (upstream `NonLocalManager`,
//!   neighbourhoods, weight functions).
//! * [`energy_split`]: the elastic energy splits of phase-field fracture
//!   (upstream `src/model/phase_field/energy_split*`), shared by the
//!   phase-field laws and the solid `phasefield` material (one owner).

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod boundary_condition;
pub mod constitutive_law;
pub mod constitutive_laws_handler;
pub mod coupling;
pub mod energy_split;
pub mod internal_field;
pub mod model;
pub mod neumann_store;
pub mod non_local;
pub mod params;

pub use constitutive_law::{ConstitutiveLaw, ConstitutiveLawFactory};
pub use constitutive_laws_handler::ConstitutiveLawsHandler;
pub use internal_field::InternalField;
pub use model::{Model, ModelOptions};
