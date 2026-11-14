// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/io/dumper/dumper_iohelper.cc
// (getIOHelperType), third-party/iohelper/src/iohelper_common.hh (ElemType,
// nb_node_per_elem) and paraview_helper.{hh,cc} (VTKCellType values,
// paraview_code_type, write_reorder).
// IOHelper (third-party/iohelper, v1.1.1): Copyright (©) 2010-2012, 2014 EPFL
// (Ecole Polytechnique Fédérale de Lausanne), Laboratory (LSMS - Laboratoire
// de Simulation en Mécanique des Solides). IOHelper is free software: you can
// redistribute it and/or modify it under the terms of the GNU Lesser General
// Public License as published by the Free Software Foundation, either version
// 3 of the License, or (at your option) any later version.

//! Element type → IOHelper type → VTK cell type and node order.
//!
//! Upstream maps 21 Akantu types; `_cohesive_3d_16` and
//! `_discrete_kirchhoff_triangle_18` throw `AKANTU_TO_IMPLEMENT` when
//! dumped (UD-053, oracle fixture `fixtures/dump/coh3d16`, `dkt18`).
//! akantu-rs adds both (UD-053 decision: dump support with an appropriate
//! topology): DKT18 is a 3-node triangle (`VTK_TRIANGLE`); a
//! `_cohesive_3d_16` (two 8-node quadrangle facets) is drawn as the
//! linear hexahedron of its 8 corner nodes (VTK has no 16-node
//! hexahedron; the same choice upstream makes for `_cohesive_3d_8`).

use akantu_core::{ElementType, Error, Result};

/// IOHelper element types (`iohelper::ElemType`), plus the akantu-rs
/// extensions for UD-053.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum IoHelperType {
    /// `TRIANGLE1`
    Triangle1,
    /// `TRIANGLE2`
    Triangle2,
    /// `TETRA1`
    Tetra1,
    /// `TETRA2`
    Tetra2,
    /// `POINT_SET`
    PointSet,
    /// `LINE1`
    Line1,
    /// `LINE2`
    Line2,
    /// `QUAD1`
    Quad1,
    /// `QUAD2`
    Quad2,
    /// `HEX1`
    Hex1,
    /// `HEX2`
    Hex2,
    /// `BEAM2`
    Beam2,
    /// `BEAM3`
    Beam3,
    /// `PRISM1`
    Prism1,
    /// `PRISM2`
    Prism2,
    /// `COH1D2`
    Coh1d2,
    /// `COH2D4`
    Coh2d4,
    /// `COH2D6`
    Coh2d6,
    /// `COH3D6`
    Coh3d6,
    /// `COH3D12`
    Coh3d12,
    /// `COH3D8`
    Coh3d8,
    /// akantu-rs extension: `_cohesive_3d_16` (corner hexahedron).
    Coh3d16,
    /// akantu-rs extension: `_discrete_kirchhoff_triangle_18` (triangle).
    Dkt18,
}

/// `getIOHelperType(type)`; `Err` for types upstream cannot dump either
/// (IGFEM types need the IGFEM extension, `_not_defined`).
pub fn iohelper_type(ty: ElementType) -> Result<IoHelperType> {
    use ElementType as E;
    use IoHelperType as H;
    Ok(match ty {
        E::Point1 => H::PointSet,
        E::Segment2 => H::Line1,
        E::Segment3 => H::Line2,
        E::Triangle3 => H::Triangle1,
        E::Triangle6 => H::Triangle2,
        E::Quadrangle4 => H::Quad1,
        E::Quadrangle8 => H::Quad2,
        E::Tetrahedron4 => H::Tetra1,
        E::Tetrahedron10 => H::Tetra2,
        E::Hexahedron8 => H::Hex1,
        E::Hexahedron20 => H::Hex2,
        E::Pentahedron6 => H::Prism1,
        E::Pentahedron15 => H::Prism2,
        E::Cohesive1d2 => H::Coh1d2,
        E::Cohesive2d4 => H::Coh2d4,
        E::Cohesive2d6 => H::Coh2d6,
        E::Cohesive3d6 => H::Coh3d6,
        E::Cohesive3d12 => H::Coh3d12,
        E::Cohesive3d8 => H::Coh3d8,
        E::BernoulliBeam2 => H::Beam2,
        E::BernoulliBeam3 => H::Beam3,
        E::Cohesive3d16 => H::Coh3d16,
        E::DiscreteKirchhoffTriangle18 => H::Dkt18,
        E::NotDefined | E::IgfemSegment3 | E::IgfemTriangle4 | E::IgfemTriangle5 => {
            return Err(Error::UnsupportedElementType {
                ty,
                context: "the ParaView dumper (IGFEM dumps need the IGFEM package)",
            });
        }
    })
}

impl IoHelperType {
    /// `nb_node_per_elem[type]` (connectivity entries written per cell).
    #[must_use]
    pub const fn nb_nodes(self) -> usize {
        use IoHelperType as H;
        match self {
            H::PointSet => 1,
            H::Line1 | H::Beam2 | H::Beam3 | H::Coh1d2 => 2,
            H::Triangle1 | H::Line2 | H::Dkt18 => 3,
            H::Tetra1 | H::Quad1 | H::Coh2d4 => 4,
            H::Triangle2 | H::Prism1 | H::Coh2d6 | H::Coh3d6 => 6,
            H::Quad2 | H::Hex1 | H::Coh3d8 | H::Coh3d16 => 8,
            H::Tetra2 => 10,
            H::Coh3d12 => 12,
            H::Prism2 => 15,
            H::Hex2 => 20,
        }
    }

    /// `paraview_code_type[type]` (VTK cell type id).
    #[must_use]
    pub const fn vtk_cell_type(self) -> u32 {
        use IoHelperType as H;
        match self {
            H::Triangle1 | H::Dkt18 => 5,                    // VTK_TRIANGLE
            H::Triangle2 => 22,                              // VTK_QUADRATIC_TRIANGLE
            H::Tetra1 => 10,                                 // VTK_TETRA
            H::Tetra2 => 24,                                 // VTK_QUADRATIC_TETRA
            H::PointSet => 2,                                // VTK_POLY_VERTEX
            H::Line1 | H::Beam2 | H::Beam3 | H::Coh1d2 => 3, // VTK_LINE
            H::Line2 => 21,                                  // VTK_QUADRATIC_EDGE
            H::Quad1 => 9,                                   // VTK_QUAD
            H::Quad2 => 23,                                  // VTK_QUADRATIC_QUAD
            H::Hex1 | H::Coh3d8 | H::Coh3d16 => 12,          // VTK_HEXAHEDRON
            H::Hex2 => 25,                                   // VTK_QUADRATIC_HEXAHEDRON
            H::Prism1 | H::Coh3d6 => 13,                     // VTK_WEDGE
            H::Prism2 => 26,                                 // VTK_QUADRATIC_WEDGE
            H::Coh2d4 | H::Coh2d6 => 7,                      // VTK_POLYGON
            H::Coh3d12 => 31,                                // VTK_QUADRATIC_LINEAR_WEDGE
        }
    }

    /// `write_reorder[type]`: VTK node `i` is Akantu node `reorder[i]`.
    #[must_use]
    pub fn reorder(self) -> &'static [usize] {
        use IoHelperType as H;
        const ID: [usize; 20] = [
            0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19,
        ];
        match self {
            H::Coh3d12 => &[0, 1, 2, 6, 7, 8, 3, 4, 5, 9, 10, 11],
            H::Coh2d6 => &[0, 2, 1, 4, 5, 3],
            H::Coh2d4 => &[0, 1, 3, 2],
            H::Hex2 => &[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 16, 17, 18, 19, 12, 13, 14, 15,
            ],
            H::Prism2 => &[0, 1, 2, 3, 4, 5, 6, 7, 8, 12, 13, 14, 9, 10, 11],
            // Corner nodes of the two quadrangle_8 facets.
            H::Coh3d16 => &[0, 1, 2, 3, 8, 9, 10, 11],
            _ => &ID[..self.nb_nodes()],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reorders_are_permutations_of_the_written_nodes() {
        for ty in ElementType::ALL {
            let Ok(h) = iohelper_type(ty) else { continue };
            let r = h.reorder();
            assert_eq!(r.len(), h.nb_nodes(), "{ty:?}");
            assert!(r.iter().all(|&i| i < ty.nb_nodes_per_element()), "{ty:?}");
            let mut s = r.to_vec();
            s.sort_unstable();
            s.dedup();
            assert_eq!(s.len(), r.len(), "{ty:?}");
            if !matches!(h, IoHelperType::Coh3d16) {
                assert_eq!(h.nb_nodes(), ty.nb_nodes_per_element(), "{ty:?}");
            }
        }
    }
}
