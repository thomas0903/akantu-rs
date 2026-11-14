// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/phase_field/energy_split.hh,
// energy_split.cc, energy_splits/* (implementation in akantu-model).

//! Energy splits (PF-008…010): re-export of [`akantu_model::energy_split`],
//! the single implementation shared with the solid `phasefield` material.
//! The paths
//! `akantu_phase_field::energy_split::*` and
//! `akantu_phase_field::{EnergySplit, EnergySplitKind}` are unchanged.

pub use akantu_model::energy_split::{EnergySplit, EnergySplitKind, PhaseFieldTangent};
