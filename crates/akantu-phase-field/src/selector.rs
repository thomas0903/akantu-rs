// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/phasefield_selector.hh,
// phasefield_selector_tmpl.hh (empty upstream).

//! Phase-field selectors (PF-011): upstream aliases of the generic
//! constitutive-law selectors of `akantu-model`.
//!
//! | upstream | port |
//! |---|---|
//! | `PhaseFieldSelector` | [`PhaseFieldSelector`] (`dyn ConstitutiveLawSelector`) |
//! | `DefaultPhaseFieldSelector` | [`DefaultPhaseFieldSelector`] (the law index already stored per element, fallback 0) |
//! | `MeshDataPhaseFieldSelector<std::string>("physical_names", model)` | [`MeshDataPhaseFieldSelector::new`]`("physical_names")` |
//! | `ElementDataPhaseFieldSelector<T>` | [`ElementDataPhaseFieldSelector`] |
//!
//! Install one with [`crate::PhaseFieldModel::set_phase_field_selector`]
//! (the default chain is kept as fallback, upstream
//! `setConstitutiveLawSelector`).

pub use akantu_model::constitutive_law::ConstitutiveLawSelector as PhaseFieldSelector;
pub use akantu_model::constitutive_law::ElementDataSelector as MeshDataPhaseFieldSelector;
pub use akantu_model::constitutive_law::ElementDataSelector as ElementDataPhaseFieldSelector;
pub use akantu_model::constitutive_law::LawIndexSelector as DefaultPhaseFieldSelector;
