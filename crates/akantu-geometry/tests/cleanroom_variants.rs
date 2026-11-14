// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): behaviour of the C++ standard-library calls
// made by src/geometry. Clean-room (D-030) evidence: plausible alternative
// designs of the models in akantu_geometry::cxx, each shown to disagree with
// the black-box fixtures (CLEANROOM_LOG.md, V-series).

//! V-series of `crates/akantu-geometry/CLEANROOM_LOG.md`: every design
//! choice of the clean-room models that is observable is checked against
//! its plausible alternatives; each alternative must disagree with the
//! libstdc++ black-box fixtures on at least one case (so the fixtures do
//! discriminate, and the chosen design is not an arbitrary pick among
//! indistinguishable ones).

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

/// Switchable selection algorithm (the model is `Variant::default()`).
#[derive(Clone, Copy, Debug)]
struct Variant {
    /// Median-of-three candidates `(first+1, mid, last-1)` (model) or
    /// `(first, mid, last-1)`.
    candidates_from_first_plus_one: bool,
    /// Ranges longer than this are partitioned (model: 3).
    threshold: usize,
    /// Depth limit factor (model: `2⌊log₂ n⌋`; `None` = unlimited).
    depth_factor: Option<usize>,
    /// Bottom-up heap adjustment (model) or classic top-down sift-down.
    bottom_up_heap: bool,
    /// Median-of-three decision tree: after `!(a < b)` the model tests
    /// `a < c` first; the alternative tests `b < c` first.
    median_else_b_first: bool,
    /// Bottom-up sift: on equal children the model descends to the right
    /// child (left only if `right < left`); the alternative descends left
    /// unless `left < right`.
    heap_ties_left: bool,
}

impl Default for Variant {
    fn default() -> Self {
        Self {
            candidates_from_first_plus_one: true,
            threshold: 3,
            depth_factor: Some(2),
            bottom_up_heap: true,
            median_else_b_first: false,
            heap_ties_left: false,
        }
    }
}

type E = (i64, i64);

fn floor_log2(n: usize) -> usize {
    (usize::BITS - 1 - n.leading_zeros()) as usize
}

fn select(v: &mut [E], nth: usize, var: Variant) -> i64 {
    let mut n = 0_i64;
    let mut less = |a: &E, b: &E| {
        n += 1;
        a.0 < b.0
    };
    let len = v.len();
    if len == 0 || nth >= len {
        return 0;
    }
    let mut first = 0;
    let mut last = len;
    let mut depth = var.depth_factor.map_or(usize::MAX, |f| f * floor_log2(len));
    let mut done = false;
    while last - first > var.threshold {
        if depth == 0 {
            select_by_heap(v, first, nth + 1, last, var, &mut less);
            v.swap(first, nth);
            done = true;
            break;
        }
        depth -= 1;
        let mid = first + (last - first) / 2;
        let a = if var.candidates_from_first_plus_one {
            first + 1
        } else {
            first
        };
        median(v, first, a, mid, last - 1, var, &mut less);
        let mut lo = first + 1;
        let mut hi = last;
        let cut = loop {
            while less(&v[lo], &v[first]) {
                lo += 1;
            }
            hi -= 1;
            while less(&v[first], &v[hi]) {
                hi -= 1;
            }
            if lo >= hi {
                break lo;
            }
            v.swap(lo, hi);
            lo += 1;
        };
        if cut <= nth {
            first = cut;
        } else {
            last = cut;
        }
    }
    if !done && first != last {
        for i in first + 1..last {
            if less(&v[i], &v[first]) {
                v[first..=i].rotate_right(1);
            } else {
                let mut j = i;
                while less(&v[i], &v[j - 1]) {
                    j -= 1;
                }
                v[j..=i].rotate_right(1);
            }
        }
    }
    n
}

fn median(
    v: &mut [E],
    r: usize,
    a: usize,
    b: usize,
    c: usize,
    var: Variant,
    less: &mut impl FnMut(&E, &E) -> bool,
) {
    let m = if less(&v[a], &v[b]) {
        if less(&v[b], &v[c]) {
            b
        } else if less(&v[a], &v[c]) {
            c
        } else {
            a
        }
    } else if var.median_else_b_first {
        if less(&v[b], &v[c]) {
            if less(&v[a], &v[c]) { a } else { c }
        } else {
            b
        }
    } else if less(&v[a], &v[c]) {
        a
    } else if less(&v[b], &v[c]) {
        c
    } else {
        b
    };
    v.swap(r, m);
}

fn sift(
    v: &mut [E],
    first: usize,
    hole: usize,
    len: usize,
    var: Variant,
    less: &mut impl FnMut(&E, &E) -> bool,
) {
    if var.bottom_up_heap {
        let top = hole;
        let mut hole = hole;
        let mut child = hole;
        while child < (len - 1) / 2 {
            child = 2 * (child + 1);
            let go_left = if var.heap_ties_left {
                !less(&v[first + child - 1], &v[first + child])
            } else {
                less(&v[first + child], &v[first + child - 1])
            };
            if go_left {
                child -= 1;
            }
            v.swap(first + hole, first + child);
            hole = child;
        }
        if len.is_multiple_of(2) && child == (len - 2) / 2 {
            child = 2 * (child + 1);
            v.swap(first + hole, first + child - 1);
            hole = child - 1;
        }
        while hole > top {
            let parent = (hole - 1) / 2;
            if less(&v[first + parent], &v[first + hole]) {
                v.swap(first + parent, first + hole);
                hole = parent;
            } else {
                break;
            }
        }
    } else {
        let mut hole = hole;
        loop {
            let l = 2 * hole + 1;
            if l >= len {
                break;
            }
            let mut big = l;
            if l + 1 < len && less(&v[first + l], &v[first + l + 1]) {
                big = l + 1;
            }
            if less(&v[first + hole], &v[first + big]) {
                v.swap(first + hole, first + big);
                hole = big;
            } else {
                break;
            }
        }
    }
}

fn select_by_heap(
    v: &mut [E],
    first: usize,
    middle: usize,
    last: usize,
    var: Variant,
    less: &mut impl FnMut(&E, &E) -> bool,
) {
    let len = middle - first;
    if len >= 2 {
        let mut parent = (len - 2) / 2;
        loop {
            sift(v, first, parent, len, var, less);
            if parent == 0 {
                break;
            }
            parent -= 1;
        }
    }
    for i in middle..last {
        if less(&v[i], &v[first]) {
            v.swap(first, i);
            sift(v, first, 0, len, var, less);
        }
    }
}

/// Number of B1 cases on which `var` disagrees (permutation or comparison
/// count).
fn nth_mismatches(var: Variant) -> usize {
    nth_mismatches_in("nth_element.json", var)
}

/// Number of cases of the `nth_element` fixture `file` on which `var`
/// disagrees.
fn nth_mismatches_in(file: &str, var: Variant) -> usize {
    load(file)
        .iter()
        .filter(|c| {
            let keys = ints(&c["keys"]);
            let nth = usize::try_from(c["nth"].as_u64().unwrap_or(0)).unwrap_or(0);
            let mut v: Vec<E> = keys
                .iter()
                .enumerate()
                .map(|(i, &k)| (k, i64::try_from(i).unwrap_or(-1)))
                .collect();
            let n = select(&mut v, nth, var);
            let perm: Vec<i64> = v.iter().map(|e| e.1).collect();
            perm != ints(&c["perm"]) || n != c["ncomp"].as_i64().unwrap_or(-1)
        })
        .count()
}

#[test]
fn v1_model_reimplementation_agrees() {
    assert_eq!(nth_mismatches(Variant::default()), 0);
}

#[test]
fn v2_median_candidates_from_first_are_rejected() {
    let var = Variant {
        candidates_from_first_plus_one: false,
        ..Variant::default()
    };
    assert!(nth_mismatches(var) > 0);
}

#[test]
fn v3_other_insertion_thresholds_are_rejected() {
    // (A threshold below 3 is not a valid variant: the unguarded partition
    // needs the median-of-three sentinels of at least four elements.)
    for threshold in [4, 5, 8, 16, 32] {
        let var = Variant {
            threshold,
            ..Variant::default()
        };
        assert!(nth_mismatches(var) > 0, "threshold {threshold}");
    }
}

#[test]
fn v4_other_depth_limits_are_rejected() {
    for depth_factor in [None, Some(1), Some(3)] {
        let var = Variant {
            depth_factor,
            ..Variant::default()
        };
        assert!(nth_mismatches(var) > 0, "depth {depth_factor:?}");
    }
}

#[test]
fn v5_top_down_heap_is_rejected() {
    let var = Variant {
        bottom_up_heap: false,
        ..Variant::default()
    };
    assert!(nth_mismatches(var) > 0);
}

/// Unique-insert rules for the set model.
#[derive(Clone, Copy, Debug, PartialEq)]
enum InsertRule {
    /// Model: descend with `comp(k, n)`, then test `comp(pred, k)`.
    Predecessor,
    /// Two-sided test at every node (`comp(k,n)` left, `comp(n,k)` right,
    /// otherwise equal).
    TwoSided,
}

/// Minimal unbalanced BST / red–black switch for the set alternatives.
fn set_mismatches(rule: InsertRule, balanced: bool) -> usize {
    use akantu_geometry::cxx::RbSet;
    load("rb_set.json")
        .iter()
        .filter(|c| {
            let comp = c["comp"].as_str().unwrap_or("?");
            let less: fn(&E, &E) -> bool = match comp {
                "lt" => |a, b| a.0 < b.0,
                "ne" => |a, b| a.0 != b.0,
                _ => |a, b| a.0 <= b.0,
            };
            let keys = ints(&c["keys"]);
            let (inserted, order): (Vec<i64>, Vec<i64>) = if rule == InsertRule::Predecessor
                && balanced
            {
                let mut s = RbSet::new();
                let ins = keys
                    .iter()
                    .enumerate()
                    .map(|(i, &k)| i64::from(s.insert((k, i64::try_from(i).unwrap_or(-1)), less)))
                    .collect();
                (ins, s.in_order().iter().map(|e| e.1).collect())
            } else {
                // Unbalanced binary search tree with the chosen rule.
                let mut nodes: Vec<(E, Option<usize>, Option<usize>)> = Vec::new();
                let mut ins = Vec::new();
                for (i, &k) in keys.iter().enumerate() {
                    let e = (k, i64::try_from(i).unwrap_or(-1));
                    if nodes.is_empty() {
                        nodes.push((e, None, None));
                        ins.push(1);
                        continue;
                    }
                    let mut cur = 0;
                    let mut last_right: Option<usize> = None;
                    let inserted = loop {
                        let go_left = less(&e, &nodes[cur].0);
                        if rule == InsertRule::TwoSided && !go_left && !less(&nodes[cur].0, &e) {
                            break false;
                        }
                        if !go_left {
                            last_right = Some(cur);
                        }
                        let next = if go_left { nodes[cur].1 } else { nodes[cur].2 };
                        if let Some(n) = next {
                            cur = n;
                            continue;
                        }
                        // Predecessor rule: the predecessor of the new leaf is
                        // the last node where the descent went right.
                        if rule == InsertRule::Predecessor
                            && let Some(j) = last_right
                            && !less(&nodes[j].0, &e)
                        {
                            break false;
                        }
                        let id = nodes.len();
                        nodes.push((e, None, None));
                        if go_left {
                            nodes[cur].1 = Some(id);
                        } else {
                            nodes[cur].2 = Some(id);
                        }
                        break true;
                    };
                    ins.push(i64::from(inserted));
                }
                let mut order = Vec::new();
                let mut stack = Vec::new();
                let mut cur = Some(0);
                while cur.is_some() || !stack.is_empty() {
                    while let Some(n) = cur {
                        stack.push(n);
                        cur = nodes[n].1;
                    }
                    if let Some(n) = stack.pop() {
                        order.push(nodes[n].0.1);
                        cur = nodes[n].2;
                    }
                }
                (ins, order)
            };
            inserted != ints(&c["inserted"]) || order != ints(&c["order"])
        })
        .count()
}

#[test]
fn w1_two_sided_equivalence_test_is_rejected() {
    assert!(set_mismatches(InsertRule::TwoSided, false) > 0);
}

#[test]
fn w2_unbalanced_tree_is_rejected() {
    assert!(set_mismatches(InsertRule::Predecessor, false) > 0);
}

#[test]
fn w0_model_agrees() {
    assert_eq!(set_mismatches(InsertRule::Predecessor, true), 0);
}

/// The median-of-three decision tree. The
/// alternative order of the comparisons after `!(a < b)` selects the same
/// median for distinct keys, so only ties and comparison counts can tell
/// them apart; B1's comparison counts do.
#[test]
fn v6_other_median_decision_tree_is_rejected() {
    let var = Variant {
        median_else_b_first: true,
        ..Variant::default()
    };
    let m = nth_mismatches(var);
    eprintln!("v6: {m} mismatching B1 cases");
    assert!(m > 0);
}

/// The child chosen on equal keys in the
/// bottom-up sift. It is observable only in the heap-selection branch with
/// tied keys, which B1 does not contain (the alternative agrees with all of
/// B1); experiment B3 (`nth_element_b3.json`) was built for it.
#[test]
fn v7_heap_child_tie_rule_is_rejected() {
    let var = Variant {
        heap_ties_left: true,
        ..Variant::default()
    };
    let b1 = nth_mismatches_in("nth_element.json", var);
    let b3 = nth_mismatches_in("nth_element_b3.json", var);
    eprintln!("v7: {b1} mismatching B1 cases, {b3} mismatching B3 cases");
    assert_eq!(
        nth_mismatches_in("nth_element_b3.json", Variant::default()),
        0
    );
    assert!(b3 > 0);
}
