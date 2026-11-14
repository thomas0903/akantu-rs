// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element.hh,
// src/mesh/mesh_inline_impl.hh (Element::kind).

//! The [`Element`] identifier `(type, index, ghost_type)`.
//!
//! # Invariants
//!
//! Fields are private and every constructor canonicalises, so a value is
//! either the single canonical [`Element::NULL`] or a *valid* element
//! (`ty != NotDefined`, `0 ≤ index ≤ MAX_IDX`). Consequently the derived
//! structural `Eq`/`Hash` (upstream `operator==` is structural) agree with
//! `Ord` for every representable value: `a == b ⇔ a.cmp(&b) == Equal`.
//!
//! Upstream's `operator<` treats any `_not_defined` element as `ElementNull`
//! (greater than everything, but not irreflexive for null). Because a
//! non-canonical null cannot be constructed here, the port's ordering is a
//! total order that coincides with upstream on all valid elements and on the
//! canonical null; this intentional correction is recorded in
//! `docs/UPSTREAM_DEFECTS.md` (null ordering is not a strict weak order
//! upstream).

use std::cmp::Ordering;
use std::fmt;

use crate::element_type::{ElementKind, ElementType, GhostType};
use crate::error::{Error, Result};
use crate::types::{ElemIdx, INVALID_IDX, MAX_IDX};

/// Identifies one element of a mesh: its type, its index inside the
/// `(type, ghost_type)` connectivity block, and its ghost status.
///
/// Layout: 8 bytes (`u8`, `u8`, padding, `i32`), suitable for compact storage
/// in `Array<Element>`-like containers and MPI buffers.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct Element {
    ty: ElementType,
    ghost_type: GhostType,
    index: ElemIdx,
}

impl Element {
    /// Upstream `ElementNull` (`{_not_defined, -1, _casper}`; the port has no
    /// `_casper` ghost type and uses `NotGhost`). The **only** null value.
    /// Stored in connectivity-like tables (e.g. `element_to_subelement`) to
    /// mark "no element". Public APIs should prefer `Option<Element>`.
    pub const NULL: Element = Element {
        ty: ElementType::NotDefined,
        ghost_type: GhostType::NotGhost,
        index: INVALID_IDX,
    };

    /// Construct an element id from internal, already validated data.
    ///
    /// Canonicalising: `ty == NotDefined` or `index < 0` yields
    /// [`Element::NULL`] (whatever the other fields). Use
    /// [`Element::try_new`] / [`Element::from_raw_parts`] for unvalidated
    /// input, which reject malformed values instead.
    #[inline]
    #[must_use]
    pub const fn new(ty: ElementType, index: ElemIdx, ghost_type: GhostType) -> Self {
        if matches!(ty, ElementType::NotDefined) || index < 0 {
            Self::NULL
        } else {
            Self {
                ty,
                ghost_type,
                index,
            }
        }
    }

    /// Checked constructor for user input: rejects `NotDefined` and indices
    /// beyond [`MAX_IDX`].
    pub fn try_new(ty: ElementType, index: usize, ghost_type: GhostType) -> Result<Self> {
        if matches!(ty, ElementType::NotDefined) {
            return Err(Error::invalid(
                "Element::try_new: _not_defined is only valid as ElementNull",
            ));
        }
        let index = crate::types::try_idx(index)?;
        Ok(Self {
            ty,
            ghost_type,
            index,
        })
    }

    /// Decode raw parts (MPI buffers, foreign arrays): accepts exactly the
    /// canonical null `(NotDefined, NotGhost, -1)` or a valid element; every
    /// other combination is an error (no silent canonicalisation of remote
    /// data).
    pub fn from_raw_parts(ty: ElementType, index: ElemIdx, ghost_type: GhostType) -> Result<Self> {
        let candidate = Self {
            ty,
            ghost_type,
            index,
        };
        if candidate == Self::NULL {
            return Ok(Self::NULL);
        }
        if matches!(ty, ElementType::NotDefined) || !(0..=MAX_IDX).contains(&index) {
            return Err(Error::InvalidIndex {
                what: "element (malformed raw parts)",
                value: i64::from(index),
            });
        }
        Ok(candidate)
    }

    /// Element type (`NotDefined` only for [`Element::NULL`]).
    #[inline(always)]
    #[must_use]
    pub const fn ty(&self) -> ElementType {
        self.ty
    }

    /// Ghost status.
    #[inline(always)]
    #[must_use]
    pub const fn ghost_type(&self) -> GhostType {
        self.ghost_type
    }

    /// Stored index (`-1` only for [`Element::NULL`]).
    #[inline(always)]
    #[must_use]
    pub const fn index(&self) -> ElemIdx {
        self.index
    }

    /// `true` for [`Element::NULL`].
    #[inline]
    #[must_use]
    pub const fn is_null(&self) -> bool {
        matches!(self.ty, ElementType::NotDefined)
    }

    /// `None` for [`Element::NULL`], `Some(self)` otherwise.
    #[inline]
    #[must_use]
    pub const fn as_option(self) -> Option<Element> {
        if self.is_null() { None } else { Some(self) }
    }

    /// Kind of the element (upstream `Element::kind()`).
    #[inline]
    #[must_use]
    pub const fn kind(&self) -> ElementKind {
        self.ty.kind()
    }

    /// Index as `usize` (`None` for [`Element::NULL`]).
    #[inline]
    #[must_use]
    pub const fn try_idx(&self) -> Option<usize> {
        if self.is_null() {
            None
        } else {
            Some(self.index as usize)
        }
    }

    /// Index as `usize`.
    ///
    /// # Panics
    /// On [`Element::NULL`] (internal invariant: indexing with the null
    /// element is a bug). Use [`Element::try_idx`] when null is possible.
    #[inline]
    #[must_use]
    #[track_caller]
    pub const fn idx(&self) -> usize {
        crate::types::to_usize(self.index)
    }
}

impl Default for Element {
    fn default() -> Self {
        Element::NULL
    }
}

impl Ord for Element {
    /// Upstream `Element::operator<`: lexicographic on
    /// `(ghost_type, type, element)`, with `ElementNull` greater than every
    /// non-null element. Total and consistent with `Eq` because the null
    /// representation is canonical.
    fn cmp(&self, other: &Self) -> Ordering {
        (self.is_null(), self.ghost_type, self.ty, self.index).cmp(&(
            other.is_null(),
            other.ghost_type,
            other.ty,
            other.index,
        ))
    }
}

impl PartialOrd for Element {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl fmt::Display for Element {
    /// Upstream `std::to_string(Element)`: `Element [_triangle_3, 4, 0]`
    /// (the ghost type prints as its integer value upstream) or
    /// `ElementNull`.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        if self.is_null() {
            f.write_str("ElementNull")
        } else {
            write!(
                f,
                "Element [{}, {}, {}]",
                self.ty, self.index, self.ghost_type as u8
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::{BTreeSet, HashSet};

    #[test]
    fn layout_is_compact() {
        assert_eq!(std::mem::size_of::<Element>(), 8);
    }

    #[test]
    fn null_semantics() {
        assert!(Element::NULL.is_null());
        assert_eq!(Element::default(), Element::NULL);
        assert_eq!(Element::NULL.as_option(), None);
        assert_eq!(Element::NULL.try_idx(), None);
        let e = Element::new(ElementType::Triangle3, 4, GhostType::NotGhost);
        assert_eq!(e.as_option(), Some(e));
        assert_eq!(e.try_idx(), Some(4));
        assert_eq!(e.to_string(), "Element [_triangle_3, 4, 0]");
        assert_eq!(Element::NULL.to_string(), "ElementNull");
        assert_eq!(e.kind(), ElementKind::Regular);
    }

    #[test]
    fn null_is_canonical() {
        // Null-like values collapse onto NULL.
        let b = Element::new(ElementType::NotDefined, 0, GhostType::Ghost);
        assert_eq!(b, Element::NULL);
        assert_eq!(
            Element::new(ElementType::Triangle3, -1, GhostType::Ghost),
            Element::NULL
        );
        assert_eq!(
            Element::new(ElementType::Triangle3, -7, GhostType::NotGhost),
            Element::NULL
        );
        let mut bt = BTreeSet::new();
        let mut hs = HashSet::new();
        for e in [
            Element::NULL,
            b,
            Element::new(ElementType::Point1, -3, GhostType::Ghost),
        ] {
            bt.insert(e);
            hs.insert(e);
        }
        assert_eq!(bt.len(), 1);
        assert_eq!(hs.len(), 1);
    }

    #[test]
    fn checked_constructors() {
        assert!(Element::try_new(ElementType::NotDefined, 0, GhostType::NotGhost).is_err());
        assert!(
            Element::try_new(
                ElementType::Segment2,
                MAX_IDX as usize + 1,
                GhostType::NotGhost
            )
            .is_err()
        );
        let e = Element::try_new(ElementType::Segment2, 3, GhostType::Ghost).unwrap();
        assert_eq!(
            (e.ty(), e.index(), e.ghost_type()),
            (ElementType::Segment2, 3, GhostType::Ghost)
        );
        assert_eq!(
            Element::from_raw_parts(ElementType::NotDefined, -1, GhostType::NotGhost).unwrap(),
            Element::NULL
        );
        assert!(Element::from_raw_parts(ElementType::NotDefined, -1, GhostType::Ghost).is_err());
        assert!(Element::from_raw_parts(ElementType::NotDefined, 0, GhostType::NotGhost).is_err());
        assert!(Element::from_raw_parts(ElementType::Triangle3, -1, GhostType::NotGhost).is_err());
        assert!(
            Element::from_raw_parts(ElementType::Triangle3, i32::MAX, GhostType::NotGhost).is_err()
        );
        assert_eq!(
            Element::from_raw_parts(ElementType::Triangle3, 5, GhostType::Ghost).unwrap(),
            Element::new(ElementType::Triangle3, 5, GhostType::Ghost)
        );
    }

    #[test]
    #[should_panic(expected = "negative local index")]
    fn idx_of_null_panics() {
        let _ = Element::NULL.idx();
    }

    #[test]
    fn ordering_matches_upstream() {
        let a = Element::new(ElementType::Triangle3, 10, GhostType::NotGhost);
        let b = Element::new(ElementType::Quadrangle4, 0, GhostType::NotGhost);
        let c = Element::new(ElementType::Segment2, 0, GhostType::Ghost);
        // ghost_type first, then type (enum order), then index
        assert!(a < b);
        assert!(b < c);
        assert!(c < Element::NULL);
        assert!(Element::NULL > a);
        let mut v = vec![Element::NULL, c, b, a];
        v.sort();
        assert_eq!(v, vec![a, b, c, Element::NULL]);
    }

    /// Deterministic exhaustive-ish property check of the `Eq`/`Hash`/`Ord`
    /// laws over raw inputs including every malformed/null-like shape.
    #[test]
    fn eq_hash_ord_agree_on_all_constructible_values() {
        use std::hash::{BuildHasher, RandomState};
        let rs = RandomState::new();
        let indices = [i32::MIN, -2, -1, 0, 1, 2, MAX_IDX - 1, MAX_IDX, i32::MAX];
        let mut all = Vec::new();
        for ty in [
            ElementType::NotDefined,
            ElementType::Point1,
            ElementType::Triangle3,
            ElementType::Cohesive2d4,
        ] {
            for g in GhostType::ALL {
                for &i in &indices {
                    all.push(Element::new(ty, i, g));
                    if let Ok(e) = Element::from_raw_parts(ty, i, g) {
                        all.push(e);
                    }
                }
            }
        }
        for a in &all {
            assert_eq!(a.cmp(a), Ordering::Equal);
            for b in &all {
                let eq = a == b;
                assert_eq!(eq, a.cmp(b) == Ordering::Equal, "{a:?} {b:?}");
                assert_eq!(a.cmp(b), b.cmp(a).reverse());
                if eq {
                    assert_eq!(rs.hash_one(a), rs.hash_one(b));
                }
                for c in &all {
                    if a <= b && b <= c {
                        assert!(a <= c);
                    }
                }
            }
        }
        let bt: BTreeSet<_> = all.iter().copied().collect();
        let hs: HashSet<_> = all.iter().copied().collect();
        assert_eq!(bt.len(), hs.len());
    }
}
