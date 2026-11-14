// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): extra_packages/igfem/src/element_class_igfem.hh,
// element_classes_igfem/element_class_igfem_{segment_3,triangle_4,triangle_5}_inline_impl.hh,
// interpolation_element_igfem{.cc,_tmpl.hh}, geometrical_element_igfem.hh,
// igfem_helper.{hh,cc}.

//! IGFEM element classes (IGFEM-001, ELEM-024…026) and the static queries of
//! upstream's `IGFEMHelper` (IGFEM-003, FE-038).
//!
//! An IGFEM element is a linear parent element (`_segment_2`, `_triangle_3`)
//! whose nodes come first in the connectivity, followed by its *enriched*
//! nodes (intersection points of the interface with the parent's edges).
//! It is split into two sub-elements (local connectivities in the core
//! table [`akantu_core::element_type::IgfemInterpolationInfo`]):
//!
//! | type | parent | sub-elements | sub-connectivities | enriched nodes |
//! |---|---|---|---|---|
//! | `_igfem_segment_3` | `_segment_2` | `_segment_2`, `_segment_2` | `{0,2}`, `{2,1}` | node 2 |
//! | `_igfem_triangle_4` | `_triangle_3` | `_triangle_3`, `_triangle_3` | `{0,1,3}`, `{0,3,2}` | node 3 |
//! | `_igfem_triangle_5` | `_triangle_3` | `_triangle_3`, `_quadrangle_4` | `{0,3,4}`, `{3,1,2,4}` | nodes 3, 4 |
//!
//! The shape functions at a point of sub-element `s` are the parent shapes
//! `N₁ … N_p` (evaluated at the point mapped to the parent's reference
//! element) followed by one enrichment function per enriched node: the
//! sub-element's Lagrange shape function of that node
//! (`assembleShapes`, upstream `InterpolationElement<_itk_igfem>`).
//! Enriched degrees of freedom are therefore *jumps of the gradient* (the
//! enrichments vanish on every parent node and are continuous), and the
//! physical value at an enriched node is `Σ N_i(x_e) u_i + u_e`.
//!
//! Upstream defects handled here (D-003, `docs/UPSTREAM_DEFECTS.md`):
//! * UD-602 (C): the debug assertions of `getOrientation` index
//!   `is_inside(2)` of a two-node segment and test `is_inside(2) &&
//!   is_inside(2)` for `_igfem_triangle_5`; the port checks the evident
//!   intent (the inside flags must make the orientation determinable) and
//!   reports an inconsistent set of flags as an error, as the oracle's debug
//!   build would. `_igfem_segment_3` returns `1` when node 0 is inside and
//!   `_igfem_triangle_5` returns `0`, as upstream.

use akantu_core::element_type::{ElementKind, ElementType, IgfemInterpolationInfo};
use akantu_core::{Error, Real, Result};

/// Number of sub-elements of every IGFEM element (upstream
/// `nb_sub_elements = 2`).
pub const NB_SUB_ELEMENTS: usize = 2;

/// IGFEM interpolation extras of `ty`, or an error for non-IGFEM types
/// (upstream `AKANTU_BOOST_IGFEM_ELEMENT_SWITCH` has no fallback).
///
/// # Errors
/// `ty` is not an IGFEM type.
pub fn igfem_info(ty: ElementType) -> Result<&'static IgfemInterpolationInfo> {
    ty.interpolation_type()
        .info()
        .igfem
        .as_ref()
        .ok_or(Error::UnsupportedElementType {
            ty,
            context: "IGFEM element class (IGFEM types only)",
        })
}

/// `true` for `_igfem_segment_3`, `_igfem_triangle_4`, `_igfem_triangle_5`.
#[must_use]
pub fn is_igfem(ty: ElementType) -> bool {
    ty.kind() == ElementKind::Igfem
}

/// Parent element type (upstream `ElementClassProperty::parent_element_type`).
///
/// # Errors
/// Not an IGFEM type.
pub fn parent_type(ty: ElementType) -> Result<ElementType> {
    igfem_info(ty)?;
    ty.info()
        .parent_element_type
        .ok_or(Error::UnsupportedElementType {
            ty,
            context: "IGFEM parent element type",
        })
}

/// Type of sub-element `s` (upstream `IGFEMHelper::getSubElementType`).
///
/// # Errors
/// Not an IGFEM type or `s ≥ 2`.
pub fn sub_element_type(ty: ElementType, s: usize) -> Result<ElementType> {
    igfem_info(ty)?;
    let subs = ty
        .info()
        .igfem_sub_element_types
        .ok_or(Error::UnsupportedElementType {
            ty,
            context: "IGFEM sub-element types",
        })?;
    subs.get(s).copied().ok_or(Error::OutOfRange {
        what: "IGFEM sub-element",
        index: s,
        len: NB_SUB_ELEMENTS,
    })
}

/// Nodes of the parent element (upstream `IGFEMHelper::getNbParentNodes`).
///
/// # Errors
/// Not an IGFEM type.
pub fn nb_parent_nodes(ty: ElementType) -> Result<usize> {
    Ok(parent_type(ty)?.nb_nodes_per_interpolation_element())
}

/// Enriched nodes per element (upstream `IGFEMHelper::getNbEnrichedNodes`,
/// `getNbEnrichments`).
///
/// # Errors
/// Not an IGFEM type.
pub fn nb_enrichments(ty: ElementType) -> Result<usize> {
    Ok(igfem_info(ty)?.nb_enrichments)
}

/// Nodes of sub-element `s` (upstream `IGFEMHelper::getNbNodesPerSubElement`).
///
/// # Errors
/// Not an IGFEM type or `s ≥ 2`.
pub fn nb_nodes_per_sub_element(ty: ElementType, s: usize) -> Result<usize> {
    Ok(sub_element_type(ty, s)?.nb_nodes_per_interpolation_element())
}

/// Local connectivity of sub-element `s` in the IGFEM element (upstream
/// `IGFEMHelper::getSubElementConnectivity`).
///
/// # Errors
/// Not an IGFEM type or `s ≥ 2`.
pub fn sub_element_connectivity(ty: ElementType, s: usize) -> Result<&'static [u8]> {
    let info = igfem_info(ty)?;
    let n0 = info.sub_element_nb_nodes[0];
    match s {
        0 => Ok(&info.sub_element_connectivity[..n0]),
        1 => Ok(&info.sub_element_connectivity[n0..]),
        _ => Err(Error::OutOfRange {
            what: "IGFEM sub-element",
            index: s,
            len: NB_SUB_ELEMENTS,
        }),
    }
}

/// Local index, **in sub-element `s`**, of each enriched node (upstream
/// `getSubElementEnrichments(s)`): enrichment `e` of a point of sub-element
/// `s` is the sub-element shape function number `enrichments(s)[e]`.
///
/// # Errors
/// Not an IGFEM type or `s ≥ 2`.
pub fn sub_element_enrichments(ty: ElementType, s: usize) -> Result<&'static [u8]> {
    let info = igfem_info(ty)?;
    let k = info.nb_enrichments;
    if s >= NB_SUB_ELEMENTS {
        return Err(Error::OutOfRange {
            what: "IGFEM sub-element",
            index: s,
            len: NB_SUB_ELEMENTS,
        });
    }
    Ok(&info.enrichments[s * k..(s + 1) * k])
}

/// IGFEM types derived from a regular type (upstream
/// `IGFEMHelper::getIGFEMElementTypes<type>()`, `ElementTypeIGFEMData`):
/// `_segment_2` → `[_igfem_segment_3]`, `_triangle_3` →
/// `[_igfem_triangle_4, _igfem_triangle_5]`, every other type → none
/// (upstream lists `_not_defined` with a count of 0).
#[must_use]
pub fn igfem_element_types(ty: ElementType) -> &'static [ElementType] {
    match ty {
        ElementType::Segment2 => &[ElementType::IgfemSegment3],
        ElementType::Triangle3 => &[ElementType::IgfemTriangle4, ElementType::IgfemTriangle5],
        _ => &[],
    }
}

/// Orientation of an element with respect to the interface from the
/// inside flags of its parent nodes (upstream `ElementClass::getOrientation`,
/// `IGFEMHelper::getElementOrientation`):
/// * `_igfem_segment_3`: `1` if node 0 is inside, else `0`;
/// * `_igfem_triangle_4` (nodes 1, 2 lie on both sides of the cut edge):
///   `0` = only node 1 inside, `1` = only node 2 inside, `2` = both,
///   `3` = neither;
/// * `_igfem_triangle_5` (node 0 is the isolated vertex): `0` if node 0
///   is inside, else `1`.
///
/// # Errors
/// Not an IGFEM type, wrong number of flags, or flags for which upstream's
/// debug assertion "orientation not determinable" fires (UD-602: with the
/// evident intent of the two defective assertions).
pub fn orientation(ty: ElementType, is_inside: &[bool]) -> Result<usize> {
    let np = nb_parent_nodes(ty)?;
    if is_inside.len() != np {
        return Err(Error::DimensionMismatch {
            context: "IGFEM getOrientation: one inside flag per parent node",
            expected: np,
            got: is_inside.len(),
        });
    }
    let undeterminable = || Error::InvalidArgument(format!("{ty}: orientation not determinable"));
    match ty {
        ElementType::IgfemSegment3 => {
            // upstream: `UInt sub_el_is_inside = false; if (is_inside(0))
            // {sub_el_is_inside = true; assert(!is_inside(1))} else
            // assert(is_inside(2))` — index 2 does not exist (UD-602)
            if is_inside[0] {
                if is_inside[1] {
                    return Err(undeterminable());
                }
                Ok(1)
            } else {
                if !is_inside[1] {
                    return Err(undeterminable());
                }
                Ok(0)
            }
        }
        ElementType::IgfemTriangle4 => Ok(match (is_inside[1], is_inside[2]) {
            (true, false) => 0,
            (false, true) => 1,
            (true, true) => 2,
            (false, false) => 3,
        }),
        ElementType::IgfemTriangle5 => {
            if is_inside[0] {
                if is_inside[1] || is_inside[2] {
                    return Err(undeterminable());
                }
                Ok(0)
            } else {
                // upstream asserts `is_inside(2) && is_inside(2)` (UD-602)
                if !(is_inside[1] && is_inside[2]) {
                    return Err(undeterminable());
                }
                Ok(1)
            }
        }
        _ => Err(Error::UnsupportedElementType {
            ty,
            context: "IGFEM getOrientation",
        }),
    }
}

/// Coordinates of the nodes of sub-element `s` (upstream
/// `getSubElementCoords`): `element_coords` is column-major `sdim × nn`
/// (all nodes of the IGFEM element), `sub_coords` receives column-major
/// `sdim × nn_sub`.
///
/// # Errors
/// Not an IGFEM type, `s ≥ 2`, or slices of the wrong length.
pub fn sub_element_coords(
    ty: ElementType,
    element_coords: &[Real],
    sdim: usize,
    s: usize,
    sub_coords: &mut [Real],
) -> Result<()> {
    let conn = sub_element_connectivity(ty, s)?;
    check_len(
        "IGFEM getSubElementCoords: element coordinates",
        sdim * ty.nb_nodes_per_element(),
        element_coords.len(),
    )?;
    check_len(
        "IGFEM getSubElementCoords: sub-element coordinates",
        sdim * conn.len(),
        sub_coords.len(),
    )?;
    for (i, &lc) in conn.iter().enumerate() {
        let lc = usize::from(lc);
        sub_coords[i * sdim..(i + 1) * sdim]
            .copy_from_slice(&element_coords[lc * sdim..(lc + 1) * sdim]);
    }
    Ok(())
}

/// Coordinates of the parent nodes (upstream `getParentCoords`: the first
/// `nb_parent_nodes` columns).
///
/// # Errors
/// Not an IGFEM type or slices of the wrong length.
pub fn parent_coords(
    ty: ElementType,
    element_coords: &[Real],
    sdim: usize,
    parent: &mut [Real],
) -> Result<()> {
    let np = nb_parent_nodes(ty)?;
    check_len(
        "IGFEM getParentCoords: parent coordinates",
        sdim * np,
        parent.len(),
    )?;
    let src = element_coords
        .get(..sdim * np)
        .ok_or(Error::DimensionMismatch {
            context: "IGFEM getParentCoords: element coordinates",
            expected: sdim * np,
            got: element_coords.len(),
        })?;
    parent.copy_from_slice(src);
    Ok(())
}

/// Upstream `assembleShapes(parent, sub, out, s)`: `out[i] = parent[i]`
/// for the parent nodes, then `out[p + e] = sub[enrichments(s)[e]]`.
///
/// # Errors
/// Not an IGFEM type, `s ≥ 2`, or slices of the wrong length.
pub fn assemble_shapes(
    ty: ElementType,
    parent: &[Real],
    sub: &[Real],
    out: &mut [Real],
    s: usize,
) -> Result<()> {
    let np = nb_parent_nodes(ty)?;
    let enr = sub_element_enrichments(ty, s)?;
    check_len("IGFEM assembleShapes: parent shapes", np, parent.len())?;
    check_len(
        "IGFEM assembleShapes: sub-element shapes",
        nb_nodes_per_sub_element(ty, s)?,
        sub.len(),
    )?;
    check_len(
        "IGFEM assembleShapes: shapes",
        ty.nb_nodes_per_interpolation_element(),
        out.len(),
    )?;
    out[..np].copy_from_slice(parent);
    for (e, &k) in enr.iter().enumerate() {
        out[np + e] = sub[usize::from(k)];
    }
    Ok(())
}

/// Upstream `assembleShapeDerivatives(parent, sub, out, s)`: column-major
/// `dim × nn` matrices; the parent columns, then column
/// `enrichments(s)[e]` of the sub-element derivatives for each enrichment.
///
/// # Errors
/// Not an IGFEM type, `s ≥ 2`, or slices of the wrong length.
pub fn assemble_shape_derivatives(
    ty: ElementType,
    dim: usize,
    parent: &[Real],
    sub: &[Real],
    out: &mut [Real],
    s: usize,
) -> Result<()> {
    let np = nb_parent_nodes(ty)?;
    let enr = sub_element_enrichments(ty, s)?;
    check_len(
        "IGFEM assembleShapeDerivatives: parent derivatives",
        dim * np,
        parent.len(),
    )?;
    check_len(
        "IGFEM assembleShapeDerivatives: sub-element derivatives",
        dim * nb_nodes_per_sub_element(ty, s)?,
        sub.len(),
    )?;
    check_len(
        "IGFEM assembleShapeDerivatives: derivatives",
        dim * ty.nb_nodes_per_interpolation_element(),
        out.len(),
    )?;
    out[..dim * np].copy_from_slice(parent);
    for (e, &k) in enr.iter().enumerate() {
        let k = usize::from(k);
        out[(np + e) * dim..(np + e + 1) * dim].copy_from_slice(&sub[k * dim..(k + 1) * dim]);
    }
    Ok(())
}

/// Barycenter of sub-element `s` of an element with column-major
/// coordinates `element_coords` (`sdim × nn`): the sum of the sub-element's
/// node coordinates divided by their number (upstream
/// `SolidMechanicsModelIGFEM::getSubElementBarycenter` → `Math::barycenter`,
/// v5 form: accumulate, then divide).
///
/// # Errors
/// Not an IGFEM type, `s ≥ 2`, or slices of the wrong length.
pub fn sub_element_barycenter(
    ty: ElementType,
    element_coords: &[Real],
    sdim: usize,
    s: usize,
    out: &mut [Real],
) -> Result<()> {
    let conn = sub_element_connectivity(ty, s)?;
    check_len(
        "IGFEM getSubElementBarycenter: element coordinates",
        sdim * ty.nb_nodes_per_element(),
        element_coords.len(),
    )?;
    check_len("IGFEM getSubElementBarycenter: barycenter", sdim, out.len())?;
    out.fill(0.0);
    for &lc in conn {
        let lc = usize::from(lc);
        for (o, x) in out
            .iter_mut()
            .zip(&element_coords[lc * sdim..(lc + 1) * sdim])
        {
            *o += *x;
        }
    }
    #[allow(clippy::cast_precision_loss)] // at most 4 nodes
    let n = conn.len() as Real;
    for o in out.iter_mut() {
        *o /= n;
    }
    Ok(())
}

pub(crate) fn check_len(context: &'static str, expected: usize, got: usize) -> Result<()> {
    if expected == got {
        Ok(())
    } else {
        Err(Error::DimensionMismatch {
            context,
            expected,
            got,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use ElementType as Et;

    #[test]
    fn tables_match_upstream() {
        // interpolation_element_igfem.cc
        assert_eq!(
            sub_element_connectivity(Et::IgfemSegment3, 0).unwrap(),
            &[0, 2]
        );
        assert_eq!(
            sub_element_connectivity(Et::IgfemSegment3, 1).unwrap(),
            &[2, 1]
        );
        assert_eq!(
            sub_element_connectivity(Et::IgfemTriangle4, 0).unwrap(),
            &[0, 1, 3]
        );
        assert_eq!(
            sub_element_connectivity(Et::IgfemTriangle4, 1).unwrap(),
            &[0, 3, 2]
        );
        assert_eq!(
            sub_element_connectivity(Et::IgfemTriangle5, 0).unwrap(),
            &[0, 3, 4]
        );
        assert_eq!(
            sub_element_connectivity(Et::IgfemTriangle5, 1).unwrap(),
            &[3, 1, 2, 4]
        );
        assert_eq!(sub_element_enrichments(Et::IgfemSegment3, 0).unwrap(), &[1]);
        assert_eq!(sub_element_enrichments(Et::IgfemSegment3, 1).unwrap(), &[0]);
        assert_eq!(
            sub_element_enrichments(Et::IgfemTriangle4, 0).unwrap(),
            &[2]
        );
        assert_eq!(
            sub_element_enrichments(Et::IgfemTriangle4, 1).unwrap(),
            &[1]
        );
        assert_eq!(
            sub_element_enrichments(Et::IgfemTriangle5, 0).unwrap(),
            &[1, 2]
        );
        assert_eq!(
            sub_element_enrichments(Et::IgfemTriangle5, 1).unwrap(),
            &[0, 3]
        );
        assert_eq!(parent_type(Et::IgfemTriangle5).unwrap(), Et::Triangle3);
        assert_eq!(
            sub_element_type(Et::IgfemTriangle5, 1).unwrap(),
            Et::Quadrangle4
        );
        assert_eq!(nb_parent_nodes(Et::IgfemSegment3).unwrap(), 2);
        assert_eq!(nb_enrichments(Et::IgfemTriangle5).unwrap(), 2);
        assert!(igfem_info(Et::Triangle3).is_err());
        assert!(sub_element_type(Et::IgfemTriangle4, 2).is_err());
    }

    /// Every enriched node is a node of both sub-elements, at the local
    /// position given by the enrichment table (structural invariant of the
    /// tables: the enrichment of sub-element `s` is the sub-element shape of
    /// the enriched node).
    #[test]
    fn enrichment_table_points_at_the_enriched_nodes() {
        for ty in [Et::IgfemSegment3, Et::IgfemTriangle4, Et::IgfemTriangle5] {
            let np = nb_parent_nodes(ty).unwrap();
            for s in 0..2 {
                let conn = sub_element_connectivity(ty, s).unwrap();
                let enr = sub_element_enrichments(ty, s).unwrap();
                for (e, &k) in enr.iter().enumerate() {
                    assert_eq!(usize::from(conn[usize::from(k)]), np + e, "{ty} sub {s}");
                }
            }
        }
    }

    #[test]
    fn igfem_types_per_regular_type() {
        assert_eq!(igfem_element_types(Et::Segment2), &[Et::IgfemSegment3]);
        assert_eq!(
            igfem_element_types(Et::Triangle3),
            &[Et::IgfemTriangle4, Et::IgfemTriangle5]
        );
        for ty in ElementType::ALL {
            if !matches!(ty, Et::Segment2 | Et::Triangle3) {
                assert!(igfem_element_types(ty).is_empty());
            }
        }
    }

    #[test]
    fn orientation_cases() {
        assert_eq!(
            orientation(Et::IgfemTriangle4, &[false, true, false]).unwrap(),
            0
        );
        assert_eq!(
            orientation(Et::IgfemTriangle4, &[true, false, true]).unwrap(),
            1
        );
        assert_eq!(
            orientation(Et::IgfemTriangle4, &[false, true, true]).unwrap(),
            2
        );
        assert_eq!(
            orientation(Et::IgfemTriangle4, &[true, false, false]).unwrap(),
            3
        );
        assert_eq!(
            orientation(Et::IgfemTriangle5, &[true, false, false]).unwrap(),
            0
        );
        assert_eq!(
            orientation(Et::IgfemTriangle5, &[false, true, true]).unwrap(),
            1
        );
        assert!(orientation(Et::IgfemTriangle5, &[false, true, false]).is_err());
        assert!(orientation(Et::IgfemTriangle5, &[true, true, false]).is_err());
        assert_eq!(orientation(Et::IgfemSegment3, &[true, false]).unwrap(), 1);
        assert_eq!(orientation(Et::IgfemSegment3, &[false, true]).unwrap(), 0);
        assert!(orientation(Et::IgfemSegment3, &[true, true]).is_err());
        assert!(orientation(Et::IgfemSegment3, &[true]).is_err());
        assert!(orientation(Et::Triangle3, &[true, true, true]).is_err());
    }

    #[test]
    fn assemble_shapes_and_derivatives() {
        let mut out = [0.0; 5];
        assemble_shapes(
            Et::IgfemTriangle5,
            &[0.1, 0.2, 0.7],
            &[0.4, 0.3, 0.2, 0.1],
            &mut out,
            1,
        )
        .unwrap();
        assert_eq!(out, [0.1, 0.2, 0.7, 0.4, 0.1]);
        let mut d = [0.0; 10];
        let parent = [1.0, 2.0, 3.0, 4.0, 5.0, 6.0];
        let sub = [10.0, 11.0, 12.0, 13.0, 14.0, 15.0];
        assemble_shape_derivatives(Et::IgfemTriangle5, 2, &parent, &sub, &mut d, 0).unwrap();
        assert_eq!(d, [1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 12.0, 13.0, 14.0, 15.0]);
        assert!(assemble_shapes(Et::IgfemTriangle4, &[0.0; 3], &[0.0; 4], &mut out, 0).is_err());
    }

    #[test]
    fn coordinates_and_barycenter() {
        // igfem_triangle_4: (0,0) (1,0) (0,1) + enriched (0.5,0.5)
        let x = [0.0, 0.0, 1.0, 0.0, 0.0, 1.0, 0.5, 0.5];
        let mut sub = [0.0; 6];
        sub_element_coords(Et::IgfemTriangle4, &x, 2, 1, &mut sub).unwrap();
        assert_eq!(sub, [0.0, 0.0, 0.5, 0.5, 0.0, 1.0]);
        let mut p = [0.0; 6];
        parent_coords(Et::IgfemTriangle4, &x, 2, &mut p).unwrap();
        assert_eq!(p, x[..6]);
        let mut b = [0.0; 2];
        sub_element_barycenter(Et::IgfemTriangle4, &x, 2, 0, &mut b).unwrap();
        assert_eq!(b, [1.5 / 3.0, 0.5 / 3.0]);
    }
}
