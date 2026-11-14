// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element_class_structural.hh,
// element_classes/element_class_{hermite,bernoulli_beam,kirchhoff_shell}_inline_impl.hh,
// shape_structural*, fe_engine_template_tmpl_struct.hh and the element-level
// parts of src/model/structural_mechanics/*.

//! Structural elements (ELEM-021/022/023/027, FE-016/035/036,
//! element-level parts of STRUCT-003…012).
//!
//! * [`element`]: the [`StructuralElement`] kernels (Hermite helper,
//!   Bernoulli beams 2-D/3-D, discrete Kirchhoff triangle) and the
//!   [`with_structural_element!`](crate::with_structural_element) dispatch;
//! * [`engine`]: [`StructuralFeEngine`] (upstream
//!   `FEEngineTemplate<IntegratorGauss, ShapeStructural, _ek_structural>`):
//!   rotations, `det J · w`, `N·T`, `B·T`, interpolation, generalised
//!   strains, integration, `Bᵀσ`, `BᵀDB`, `Nᵀb`, `NᵀbN`, stresses, element
//!   stiffness, consistent mass, line loads;
//! * [`kernels`]: generalised tangents of the three types (with the UD-007
//!   switch) and the element part of the beam lumped mass.
//!
//! Contract (`docs/ARCHITECTURE.md`): fallible primitive kernels
//! (`Error::Numerical` for degenerate geometry, UD-296), D-003 switches in
//! [`StructuralUpstreamCompat`] (corrected 3-D beam θy row by default,
//! UD-293; inadmissible 3-D beam normals refused by default, UD-343),
//! engine blocks stamped with the mesh revision and the `extra_normal` bit
//! pattern, checked against the mesh by every operation that reads them
//! and rebuilt in full on mesh events (a failed rebuild drops the ghost
//! type).

pub mod element;
pub mod engine;
pub mod kernels;

pub use element::{
    BEAM3_NORMAL_ADMISSIBILITY, Coords, StructuralElement, StructuralUpstreamCompat,
};
pub use engine::{EXTRA_NORMAL, StructuralFeEngine};
pub use kernels::DktBendingStiffness;
