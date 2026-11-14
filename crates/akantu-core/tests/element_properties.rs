// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/element.hh (Element
// equality/ordering); property tests.

//! `Eq`, `Hash` and `Ord` of [`Element`] must agree for every constructible
//! value (including null-like and malformed raw inputs), and set/map
//! containers must agree on membership.

use std::collections::{BTreeSet, HashSet};
use std::hash::{BuildHasher, RandomState};

use akantu_core::{Element, ElementType, GhostType};
use proptest::prelude::*;

fn any_element() -> impl Strategy<Value = Element> {
    (
        0..ElementType::COUNT,
        any::<bool>(),
        prop_oneof![any::<i32>(), -3i32..4, Just(i32::MAX), Just(i32::MAX - 1)],
        any::<bool>(),
    )
        .prop_map(|(t, ghost, index, raw)| {
            let ty = ElementType::from_index(t).unwrap_or_default();
            let g = if ghost {
                GhostType::Ghost
            } else {
                GhostType::NotGhost
            };
            if raw {
                Element::from_raw_parts(ty, index, g).unwrap_or(Element::NULL)
            } else {
                Element::new(ty, index, g)
            }
        })
}

proptest! {
    #[test]
    fn eq_ord_hash_agree(a in any_element(), b in any_element(), c in any_element()) {
        let rs = RandomState::new();
        prop_assert_eq!(a == b, a.cmp(&b) == std::cmp::Ordering::Equal);
        prop_assert_eq!(a.cmp(&b), b.cmp(&a).reverse());
        if a == b {
            prop_assert_eq!(rs.hash_one(a), rs.hash_one(b));
        }
        if a <= b && b <= c {
            prop_assert!(a <= c);
        }
        // null is the unique greatest value
        prop_assert!(a <= Element::NULL);
        prop_assert_eq!(a.is_null(), a == Element::NULL);
        // valid elements have a non-negative index
        prop_assert!(a.is_null() || a.index() >= 0);
    }

    #[test]
    fn containers_agree(v in proptest::collection::vec(any_element(), 0..64)) {
        let bt: BTreeSet<Element> = v.iter().copied().collect();
        let hs: HashSet<Element> = v.iter().copied().collect();
        prop_assert_eq!(bt.len(), hs.len());
        for e in &bt {
            prop_assert!(hs.contains(e));
        }
    }
}
