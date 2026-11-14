// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/geometry/mesh_geom_common.hh (number
// types of `cgal::Spherical`). Derived from CGAL 5.6.2 Number_types
// (LGPL-3.0-or-later); see the individual files for the CGAL notices.

//! Exact number types of upstream's spherical kernel
//! (`CGAL::Quotient<CGAL::MP_Float>` and its `Root_of_2`), ported from
//! CGAL's LGPL Number_types package (D-042 (3)).

pub mod mp_float;
pub mod quotient;
pub mod sqrt_extension;

pub use mp_float::MpFloat;
pub use quotient::Quotient;
pub use sqrt_extension::RootOf2;
