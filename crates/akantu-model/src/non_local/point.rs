// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/integration_point.hh
// (IntegrationPoint: element, num_point, global_num; to_string) as used by
// src/model/common/non_local_toolbox/neighborhood_base.cc (half pair lists,
// UD-037) and non_local_neighborhood_tmpl.hh (self pairs).

//! Integration points of the non-local toolbox.

use std::fmt;

use akantu_core::{ElementType, GhostType, GlobalIdx, Idx};

/// Partition-independent identity of an integration point: (element type,
/// **global** element id, quadrature index). Orders the half pair lists and
/// identifies self pairs by default (UD-037: upstream compares the per-type
/// `global_num` only, see [`PointIdentity`]).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct PointKey {
    /// Element type.
    pub ty: ElementType,
    /// Global element id.
    pub global_element: GlobalIdx,
    /// Quadrature point index within the element.
    pub quad: Idx,
}

/// An integration point inserted in a neighbourhood (upstream
/// `IntegrationPoint` as built by
/// `ConstitutiveLawNonLocal::insertIntegrationPointsInNeighborhoods`).
///
/// `element` is the element index inside its `(type, ghost)` block and may
/// be any value a callback chooses (upstream's own test model inserts every
/// point with the null element index `-1`); `global_num` indexes the
/// per-point arrays of the manager (volumes, jacobians, flattened fields)
/// of block `(ty, ghost)`, upstream `element·nb_quad + num_point`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct QuadPoint {
    /// Element type.
    pub ty: ElementType,
    /// Ghost status.
    pub ghost: GhostType,
    /// Element index in the `(ty, ghost)` block (upstream `element`).
    pub element: Idx,
    /// Quadrature point within the element (upstream `num_point`).
    pub num_point: Idx,
    /// Position in the `(ty, ghost)` per-point arrays (upstream `global_num`).
    pub global_num: Idx,
    /// Global (partition-independent) element id, the identity used by
    /// [`PointIdentity::Complete`] (serial meshes: equal to `element`).
    pub global_element: GlobalIdx,
}

impl QuadPoint {
    /// A point of element `element` of `(ty, ghost)` with `nb_quad` points
    /// per element (upstream `IntegrationPoint(element, num_point,
    /// nb_quad)`), its global element id equal to `element`.
    #[must_use]
    pub fn new(
        ty: ElementType,
        ghost: GhostType,
        element: Idx,
        num_point: Idx,
        nb_quad: Idx,
    ) -> Self {
        Self {
            ty,
            ghost,
            element,
            num_point,
            // saturated instead of overflowing: such a point is outside every
            // block and refused by the manager's layout lookup
            global_num: element.saturating_mul(nb_quad).saturating_add(num_point),
            global_element: GlobalIdx::from(element),
        }
    }

    /// The partition-independent key.
    #[must_use]
    pub fn key(&self) -> PointKey {
        PointKey {
            ty: self.ty,
            global_element: self.global_element,
            quad: self.num_point,
        }
    }

    /// Upstream `IntegrationPoint::operator==`: same element (type, index,
    /// ghost status) and same point number (`global_num` not compared).
    #[must_use]
    pub fn same_point(&self, other: &Self) -> bool {
        self.ty == other.ty
            && self.element == other.element
            && self.ghost == other.ghost
            && self.num_point == other.num_point
    }

    /// Upstream `std::to_string(IntegrationPoint)`:
    /// `IntegrationPoint[ Element [_quadrangle_4, 3, 0], 1: 13]` (the ghost
    /// type prints as its integer value, the element index as stored, even
    /// `-1`).
    #[must_use]
    pub fn upstream_string(&self) -> String {
        format!(
            "IntegrationPoint[ Element [{}, {}, {}], {}: {}]",
            self.ty, self.element, self.ghost as u8, self.num_point, self.global_num
        )
    }
}

impl fmt::Display for QuadPoint {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.upstream_string())
    }
}

/// Identity used for the half pair lists (`q1 ≤ q2` among not-ghost points)
/// and for the self pairs of the weight computation (UD-037).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum PointIdentity {
    /// Complete identity [`PointKey`] (type, global element, quadrature
    /// index) — the corrected default (UD-037, category B). On a mesh with
    /// a single element type it orders and identifies exactly like
    /// upstream (`global_num = element·nb_quad + num_point` is increasing
    /// in `(element, num_point)`), so the results are bit for bit
    /// upstream's.
    #[default]
    Complete,
    /// Upstream: compare the per-type `global_num` only, also across
    /// element types (`neighborhood_base.cc:85-89`,
    /// `non_local_neighborhood_tmpl.hh:155`): on a mixed-type mesh two
    /// distinct points of different types with equal `global_num` pass the
    /// half-list test from both sides, so their pair is stored twice, and
    /// each copy is treated as a self pair (zero second weight); the two
    /// copies contribute exactly what the single corrected pair does, so
    /// only the accumulation order differs. No pair is ever dropped.
    /// Compat switch reproducing the oracle.
    UpstreamGlobalNum,
}

impl PointIdentity {
    /// `q1 ≤ q2` of the half pair list.
    #[must_use]
    pub fn half_list_le(self, q1: &QuadPoint, q2: &QuadPoint) -> bool {
        match self {
            Self::Complete => q1.key() <= q2.key(),
            Self::UpstreamGlobalNum => q1.global_num <= q2.global_num,
        }
    }

    /// `q1` and `q2` are the same point (self pair: no second weight).
    #[must_use]
    pub fn is_self_pair(self, q1: &QuadPoint, q2: &QuadPoint) -> bool {
        match self {
            Self::Complete => q1.key() == q2.key() && q1.ghost == q2.ghost,
            Self::UpstreamGlobalNum => q1.global_num == q2.global_num,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn upstream_printing_and_identities() {
        let q = QuadPoint {
            ty: ElementType::Quadrangle4,
            ghost: GhostType::NotGhost,
            element: -1,
            num_point: 16,
            global_num: 16,
            global_element: -1,
        };
        assert_eq!(
            q.upstream_string(),
            "IntegrationPoint[ Element [_quadrangle_4, -1, 0], 16: 16]"
        );
        let a = QuadPoint::new(ElementType::Triangle3, GhostType::NotGhost, 2, 0, 1);
        let b = QuadPoint::new(ElementType::Quadrangle4, GhostType::NotGhost, 0, 2, 4);
        // UD-037: equal global_num across types
        assert_eq!(a.global_num, b.global_num);
        assert!(PointIdentity::UpstreamGlobalNum.is_self_pair(&a, &b));
        assert!(!PointIdentity::Complete.is_self_pair(&a, &b));
        assert!(PointIdentity::Complete.is_self_pair(&a, &a));
        assert!(a.same_point(&a) && !a.same_point(&b));
    }
}
