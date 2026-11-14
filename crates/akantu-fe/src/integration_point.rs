// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/integration_point.hh.

//! Integration-point identifier (upstream `IntegrationPoint`).

use akantu_core::{Element, Idx};

/// An element plus a quadrature-point number.
///
/// Identity is `(element, num_point)`, as upstream `operator==` and
/// `operator<` (`integration_point.hh:52-64`): `global_num` is a cached
/// position that depends on the number of points per element and takes no
/// part in equality, hashing or ordering. Upstream's `operator!=` also
/// compares `global_num` and is therefore not the negation of `==` (two
/// points can be both `==` and `!=`); Rust keeps `!=` as the negation of
/// `==`, a deliberate consistent adaptation.
#[derive(Clone, Copy, Debug)]
pub struct IntegrationPoint {
    /// Element.
    pub element: Element,
    /// Quadrature point within the element.
    pub num_point: Idx,
    /// Global index of the point inside the `(type, ghost)` quadrature
    /// array: `element.index * nb_quad + num_point` (upstream `global_num`).
    pub global_num: Idx,
}

impl PartialEq for IntegrationPoint {
    fn eq(&self, other: &Self) -> bool {
        self.element == other.element && self.num_point == other.num_point
    }
}

impl Eq for IntegrationPoint {}

impl std::hash::Hash for IntegrationPoint {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.element.hash(state);
        self.num_point.hash(state);
    }
}

impl PartialOrd for IntegrationPoint {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for IntegrationPoint {
    /// Upstream `operator<`: by element, then by point number.
    fn cmp(&self, other: &Self) -> std::cmp::Ordering {
        self.element
            .cmp(&other.element)
            .then(self.num_point.cmp(&other.num_point))
    }
}

impl IntegrationPoint {
    /// Build from an element, point number and points per element.
    #[must_use]
    pub fn new(element: Element, num_point: Idx, nb_quad_per_element: Idx) -> Self {
        Self {
            element,
            num_point,
            global_num: element.index() * nb_quad_per_element + num_point,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::{ElementType, GhostType};

    #[test]
    fn global_numbering() {
        let e = Element::new(ElementType::Quadrangle4, 3, GhostType::NotGhost);
        let q = IntegrationPoint::new(e, 2, 4);
        assert_eq!(q.global_num, 14);
    }

    /// Same element and point with different cached
    /// `global_num` (4 vs 8 points per element) are equal, hash alike and
    /// are not ordered (upstream `==` true, `<` false both ways).
    #[test]
    fn identity_ignores_the_cached_global_number() {
        use std::collections::{BTreeSet, HashSet};
        let e = Element::new(ElementType::Quadrangle4, 3, GhostType::NotGhost);
        let (a, b) = (
            IntegrationPoint::new(e, 2, 4),
            IntegrationPoint::new(e, 2, 8),
        );
        assert_eq!((a.global_num, b.global_num), (14, 26));
        assert_eq!(a, b);
        assert!(!(a < b) && !(b < a));
        assert_eq!(a.cmp(&b), std::cmp::Ordering::Equal);
        assert_eq!(HashSet::from([a, b]).len(), 1);
        assert_eq!(BTreeSet::from([a, b]).len(), 1);
    }

    /// Upstream `operator<`: element first, then point number.
    #[test]
    fn ordering_is_element_then_point() {
        let e3 = Element::new(ElementType::Quadrangle4, 3, GhostType::NotGhost);
        let e4 = Element::new(ElementType::Quadrangle4, 4, GhostType::NotGhost);
        let p = |e, q| IntegrationPoint::new(e, q, 4);
        assert!(p(e3, 3) < p(e4, 0));
        assert!(p(e3, 1) < p(e3, 2));
        assert_ne!(p(e3, 1), p(e3, 2));
        assert_eq!(e3 < e4, p(e3, 0) < p(e4, 0));
    }
}
