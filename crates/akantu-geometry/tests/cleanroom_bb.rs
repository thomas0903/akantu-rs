// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): behaviour of the C++ standard-library calls
// made by src/geometry (std::nth_element via CGAL AABB_traits; std::set with
// segmentPairsLess in mesh_segment_intersector_tmpl.hh). Clean-room (D-030).

//! Replays the black-box fixtures recorded with the oracle's toolchain
//! (Debian trixie GCC 14.2 libstdc++, x86_64; experiments B1/B2 of
//! `crates/akantu-geometry/CLEANROOM_LOG.md`) through the clean-room
//! models in `akantu_geometry::cxx` and requires identical observable
//! results: the full permutation and comparison count for
//! `std::nth_element`, the insert flags and iteration order for `std::set`.

use akantu_geometry::cxx::{RbSet, nth_element_traced};
use serde_json::Value;

fn load(name: &str) -> Vec<Value> {
    let path = format!(
        "{}/tests/fixtures/cleanroom/{name}",
        env!("CARGO_MANIFEST_DIR")
    );
    let text = std::fs::read_to_string(&path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let v: Value = serde_json::from_str(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
    v["cases"].as_array().cloned().unwrap_or_default()
}

fn ints(v: &Value) -> Vec<i64> {
    v.as_array()
        .map(|a| a.iter().map(|x| x.as_i64().unwrap_or(i64::MIN)).collect())
        .unwrap_or_default()
}

#[test]
fn b1_nth_element_matches_libstdcxx() {
    let cases = load("nth_element.json");
    assert!(cases.len() > 1000, "fixture too small: {}", cases.len());
    let mut kinds = std::collections::BTreeMap::<String, usize>::new();
    let mut heap_cases = 0_usize;
    for (ci, c) in cases.iter().enumerate() {
        let keys = ints(&c["keys"]);
        let nth = usize::try_from(c["nth"].as_u64().unwrap_or(0)).unwrap_or(0);
        let want_perm = ints(&c["perm"]);
        let want_ncomp = c["ncomp"].as_i64().unwrap_or(-1);
        let mut v: Vec<(i64, i64)> = keys
            .iter()
            .enumerate()
            .map(|(i, &k)| (k, i64::try_from(i).unwrap_or(-1)))
            .collect();
        let mut ncomp = 0_i64;
        let heap = nth_element_traced(&mut v, nth, |a, b| {
            ncomp += 1;
            a.0 < b.0
        });
        if heap {
            heap_cases += 1;
        }
        let perm: Vec<i64> = v.iter().map(|e| e.1).collect();
        let kind = c["kind"].as_str().unwrap_or("?").to_owned();
        assert_eq!(
            perm,
            want_perm,
            "case {ci} ({kind}, len {}, nth {nth})",
            keys.len()
        );
        assert_eq!(ncomp, want_ncomp, "case {ci} ({kind}) comparison count");
        *kinds.entry(kind).or_default() += 1;
    }
    assert!(
        kinds.get("adversary").copied().unwrap_or(0) >= 20,
        "{kinds:?}"
    );
    // McIlroy's adversary drives the selection to its depth limit: the
    // heap-selection fallback is covered by the comparison.
    assert!(
        heap_cases >= 10,
        "heap-select path exercised in {heap_cases} cases"
    );
}

#[test]
fn b2_set_insert_matches_libstdcxx() {
    let cases = load("rb_set.json");
    assert!(cases.len() > 1000, "fixture too small: {}", cases.len());
    for (ci, c) in cases.iter().enumerate() {
        let comp = c["comp"].as_str().unwrap_or("?");
        let keys = ints(&c["keys"]);
        let want_inserted = ints(&c["inserted"]);
        let want_order = ints(&c["order"]);
        let less: fn(&(i64, i64), &(i64, i64)) -> bool = match comp {
            "lt" => |a, b| a.0 < b.0,
            "ne" => |a, b| a.0 != b.0,
            "le" => |a, b| a.0 <= b.0,
            other => panic!("unknown comparator {other}"),
        };
        let mut s = RbSet::new();
        let inserted: Vec<i64> = keys
            .iter()
            .enumerate()
            .map(|(i, &k)| i64::from(s.insert((k, i64::try_from(i).unwrap_or(-1)), less)))
            .collect();
        let order: Vec<i64> = s.in_order().iter().map(|e| e.1).collect();
        assert_eq!(
            inserted, want_inserted,
            "case {ci} ({comp}) insert flags, keys {keys:?}"
        );
        assert_eq!(
            order, want_order,
            "case {ci} ({comp}) iteration order, keys {keys:?}"
        );
    }
}

/// B3: `nth_element` on B1's
/// adversarial inputs with a few keys duplicated, so that the
/// heap-selection fallback runs on tied keys (the child chosen on equal keys
/// becomes observable). Permutations and comparison counts.
#[test]
fn b3_nth_element_heap_ties_match_libstdcxx() {
    let cases = load("nth_element_b3.json");
    assert_eq!(cases.len(), 100);
    let mut heap_cases = 0_usize;
    for (ci, c) in cases.iter().enumerate() {
        let keys = ints(&c["keys"]);
        let nth = usize::try_from(c["nth"].as_u64().unwrap_or(0)).unwrap_or(0);
        let mut v: Vec<(i64, i64)> = keys
            .iter()
            .enumerate()
            .map(|(i, &k)| (k, i64::try_from(i).unwrap_or(-1)))
            .collect();
        let mut ncomp = 0_i64;
        heap_cases += usize::from(nth_element_traced(&mut v, nth, |a, b| {
            ncomp += 1;
            a.0 < b.0
        }));
        let perm: Vec<i64> = v.iter().map(|e| e.1).collect();
        assert_eq!(
            perm,
            ints(&c["perm"]),
            "B3 case {ci} (len {}, nth {nth})",
            keys.len()
        );
        assert_eq!(
            ncomp,
            c["ncomp"].as_i64().unwrap_or(-1),
            "B3 case {ci} comparison count"
        );
    }
    assert_eq!(
        heap_cases, 100,
        "every B3 case reaches the heap-selection fallback"
    );
}
