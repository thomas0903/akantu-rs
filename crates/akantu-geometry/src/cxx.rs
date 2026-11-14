// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): the behaviour of the C++ standard library
// calls made by src/geometry (through CGAL's AABB_traits::Split_primitives:
// `std::nth_element`; mesh_segment_intersector_tmpl.hh:
// `std::set<std::pair<Segment_3, Int>, segmentPairsLess>::insert`).
// CLEAN-ROOM implementation (D-030): written from published algorithms and
// confirmed by black-box experiments against the oracle's compiled GCC 14
// libstdc++; no libstdc++/libc++ source was read. Provenance:
// crates/akantu-geometry/CLEANROOM_LOG.md.

//! Clean-room behavioural models of two C++ standard-library algorithms
//! whose *unspecified* details are observable in upstream's geometry
//! output (D-030, `CLEANROOM_LOG.md`):
//!
//! * [`nth_element`]: `std::nth_element` reorders the primitives of a CGAL
//!   AABB tree at every split; the resulting order of equivalent elements
//!   (and of both halves) fixes the tree shape and hence the order in which
//!   `all_intersections` reports intersections. Model: Musser's
//!   *introselect* (median-of-three quickselect with a `2⌊log₂ n⌋` depth
//!   limit, heap-select fallback, insertion sort below four elements).
//! * [`RbSet`]: `std::set::insert` with upstream's `segmentPairsLess`,
//!   which is **not** a strict weak ordering (UD-408), so the set's content
//!   and order depend on the red–black tree shape. Model: a classic
//!   (Guibas–Sedgewick / CLRS) red–black tree with the unique-insert rule
//!   "descend with `comp(k, node)`, then test `comp(predecessor, k)`".
//!
//! Both models are exercised by the black-box comparison tests listed in
//! `CLEANROOM_LOG.md` (fixtures generated on an x86_64 host with the oracle's
//! compiler and standard library).

/// `⌊log₂ n⌋` for `n ≥ 1`.
fn floor_log2(n: usize) -> usize {
    (usize::BITS - 1 - n.leading_zeros()) as usize
}

/// Behavioural model of `std::nth_element(first, first + nth, last, less)`
/// (GCC libstdc++, see the module documentation). Does nothing when
/// `nth >= v.len()` (`nth == last`).
pub fn nth_element<T, F>(v: &mut [T], nth: usize, less: F)
where
    F: FnMut(&T, &T) -> bool,
{
    let _ = nth_element_traced(v, nth, less);
}

/// [`nth_element`], returning whether the depth limit was reached and the
/// heap-selection fallback ran (used by the clean-room evidence tests).
pub fn nth_element_traced<T, F>(v: &mut [T], nth: usize, mut less: F) -> bool
where
    F: FnMut(&T, &T) -> bool,
{
    let len = v.len();
    if len == 0 || nth >= len {
        return false;
    }
    introselect(v, 0, nth, len, 2 * floor_log2(len), &mut less)
}

fn introselect<T, F>(
    v: &mut [T],
    mut first: usize,
    nth: usize,
    mut last: usize,
    mut depth: usize,
    less: &mut F,
) -> bool
where
    F: FnMut(&T, &T) -> bool,
{
    while last - first > 3 {
        if depth == 0 {
            select_by_heap(v, first, nth + 1, last, less);
            v.swap(first, nth);
            return true;
        }
        depth -= 1;
        let cut = median_of_three_partition(v, first, last, less);
        if cut <= nth {
            first = cut;
        } else {
            last = cut;
        }
    }
    insertion_sort(v, first, last, less);
    false
}

/// Median of `v[a], v[b], v[c]` swapped into `v[result]`.
fn median_of_three_into<T, F>(
    v: &mut [T],
    result: usize,
    a: usize,
    b: usize,
    c: usize,
    less: &mut F,
) where
    F: FnMut(&T, &T) -> bool,
{
    if less(&v[a], &v[b]) {
        if less(&v[b], &v[c]) {
            v.swap(result, b);
        } else if less(&v[a], &v[c]) {
            v.swap(result, c);
        } else {
            v.swap(result, a);
        }
    } else if less(&v[a], &v[c]) {
        v.swap(result, a);
    } else if less(&v[b], &v[c]) {
        v.swap(result, c);
    } else {
        v.swap(result, b);
    }
}

/// Hoare partition of `[first, last)` around `v[pivot]` without bounds
/// checks in the inner loops (the pivot is a sentinel).
fn hoare_partition_sentinel<T, F>(
    v: &mut [T],
    mut first: usize,
    mut last: usize,
    pivot: usize,
    less: &mut F,
) -> usize
where
    F: FnMut(&T, &T) -> bool,
{
    loop {
        while less(&v[first], &v[pivot]) {
            first += 1;
        }
        last -= 1;
        while less(&v[pivot], &v[last]) {
            last -= 1;
        }
        if first >= last {
            return first;
        }
        v.swap(first, last);
        first += 1;
    }
}

fn median_of_three_partition<T, F>(v: &mut [T], first: usize, last: usize, less: &mut F) -> usize
where
    F: FnMut(&T, &T) -> bool,
{
    let mid = first + (last - first) / 2;
    median_of_three_into(v, first, first + 1, mid, last - 1, less);
    hoare_partition_sentinel(v, first + 1, last, first, less)
}

/// Straight insertion sort of `[first, last)`: an element smaller than the
/// first one is rotated to the front, otherwise it is shifted left past
/// every larger element (stable).
fn insertion_sort<T, F>(v: &mut [T], first: usize, last: usize, less: &mut F)
where
    F: FnMut(&T, &T) -> bool,
{
    if first == last {
        return;
    }
    for i in first + 1..last {
        if less(&v[i], &v[first]) {
            v[first..=i].rotate_right(1);
        } else {
            let mut j = i;
            while less(&v[i], &v[j - 1]) {
                j -= 1;
            }
            // `v[i]` moves to `j` (comparisons were made against the value
            // being inserted, which stays at `i` until the rotation).
            v[j..=i].rotate_right(1);
        }
    }
}

/// Sift `value` (held in `slot`) down from `hole` in the heap
/// `v[first .. first+len)`, then up again ("bottom-up" adjustment).
fn bottom_up_sift<T, F>(
    v: &mut [T],
    first: usize,
    mut hole: usize,
    len: usize,
    slot: usize,
    less: &mut F,
) where
    F: FnMut(&T, &T) -> bool,
{
    // `slot` holds the value being placed; positions are written by
    // swapping it along the path so that no element is cloned.
    let top = hole;
    let mut child = hole;
    // Move the value out: swap it into the hole position.
    v.swap(first + hole, slot);
    // From now on, `first + hole` holds the value; walk it down.
    while child < (len - 1) / 2 {
        child = 2 * (child + 1);
        if less(&v[first + child], &v[first + child - 1]) {
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
    // Push up.
    while hole > top {
        let parent = (hole - 1) / 2;
        if less(&v[first + parent], &v[first + hole]) {
            v.swap(first + parent, first + hole);
            hole = parent;
        } else {
            break;
        }
    }
}

fn build_max_heap<T, F>(v: &mut [T], first: usize, last: usize, less: &mut F)
where
    F: FnMut(&T, &T) -> bool,
{
    let len = last - first;
    if len < 2 {
        return;
    }
    let mut parent = (len - 2) / 2;
    loop {
        let slot = first + parent;
        bottom_up_sift(v, first, parent, len, slot, less);
        if parent == 0 {
            return;
        }
        parent -= 1;
    }
}

/// Keep the `middle − first` smallest elements of `[first, last)` in a
/// max-heap at the front.
fn select_by_heap<T, F>(v: &mut [T], first: usize, middle: usize, last: usize, less: &mut F)
where
    F: FnMut(&T, &T) -> bool,
{
    build_max_heap(v, first, middle, less);
    for i in middle..last {
        if less(&v[i], &v[first]) {
            // heap pop: the top goes to `i` and the
            // value previously at `i` is sifted from the root.
            v.swap(first, i);
            bottom_up_sift(v, first, 0, middle - first, first, less);
        }
    }
}

/// Behavioural model of a `std::set<K, Comp>` (GCC libstdc++ red–black
/// tree) supporting only `insert` (without hint) and in-order iteration,
/// for comparators that need not be strict weak orderings (see the module
/// documentation).
#[derive(Debug)]
pub struct RbSet<K> {
    nodes: Vec<RbNode<K>>,
    root: Option<usize>,
    leftmost: Option<usize>,
}

#[derive(Debug)]
struct RbNode<K> {
    key: K,
    left: Option<usize>,
    right: Option<usize>,
    parent: Option<usize>,
    red: bool,
}

impl<K> Default for RbSet<K> {
    fn default() -> Self {
        Self {
            nodes: Vec::new(),
            root: None,
            leftmost: None,
        }
    }
}

impl<K> RbSet<K> {
    /// Empty set.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Number of elements.
    #[must_use]
    pub fn len(&self) -> usize {
        self.nodes.len()
    }

    /// `true` when empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }

    /// In-order predecessor of node `n` (`n` is not the leftmost node).
    fn predecessor(&self, n: usize) -> Option<usize> {
        if let Some(mut l) = self.nodes[n].left {
            while let Some(r) = self.nodes[l].right {
                l = r;
            }
            return Some(l);
        }
        let mut x = n;
        let mut p = self.nodes[x].parent;
        while let Some(pp) = p {
            if self.nodes[pp].left == Some(x) {
                x = pp;
                p = self.nodes[pp].parent;
            } else {
                return Some(pp);
            }
        }
        None
    }

    /// `insert(k)` with comparator `less`; returns `true` if `k` was
    /// inserted (no equivalent key found by the unique-insert rule).
    pub fn insert<F>(&mut self, k: K, mut less: F) -> bool
    where
        F: FnMut(&K, &K) -> bool,
    {
        let mut x = self.root;
        let mut y: Option<usize> = None;
        let mut went_left = true;
        while let Some(n) = x {
            y = Some(n);
            went_left = less(&k, &self.nodes[n].key);
            x = if went_left {
                self.nodes[n].left
            } else {
                self.nodes[n].right
            };
        }
        let Some(parent) = y else {
            // Empty tree.
            self.nodes.push(RbNode {
                key: k,
                left: None,
                right: None,
                parent: None,
                red: false,
            });
            self.root = Some(0);
            self.leftmost = Some(0);
            return true;
        };
        let j = if went_left {
            if Some(parent) == self.leftmost {
                None
            } else {
                self.predecessor(parent)
            }
        } else {
            Some(parent)
        };
        if let Some(j) = j
            && !less(&self.nodes[j].key, &k)
        {
            return false;
        }
        let insert_left = less(&k, &self.nodes[parent].key);
        let id = self.nodes.len();
        self.nodes.push(RbNode {
            key: k,
            left: None,
            right: None,
            parent: Some(parent),
            red: true,
        });
        if insert_left {
            self.nodes[parent].left = Some(id);
            if Some(parent) == self.leftmost {
                self.leftmost = Some(id);
            }
        } else {
            self.nodes[parent].right = Some(id);
        }
        self.rebalance(id);
        true
    }

    fn rotate_left(&mut self, x: usize) {
        let Some(y) = self.nodes[x].right else { return };
        self.nodes[x].right = self.nodes[y].left;
        if let Some(yl) = self.nodes[y].left {
            self.nodes[yl].parent = Some(x);
        }
        self.nodes[y].parent = self.nodes[x].parent;
        match self.nodes[x].parent {
            None => self.root = Some(y),
            Some(p) => {
                if self.nodes[p].left == Some(x) {
                    self.nodes[p].left = Some(y);
                } else {
                    self.nodes[p].right = Some(y);
                }
            }
        }
        self.nodes[y].left = Some(x);
        self.nodes[x].parent = Some(y);
    }

    fn rotate_right(&mut self, x: usize) {
        let Some(y) = self.nodes[x].left else { return };
        self.nodes[x].left = self.nodes[y].right;
        if let Some(yr) = self.nodes[y].right {
            self.nodes[yr].parent = Some(x);
        }
        self.nodes[y].parent = self.nodes[x].parent;
        match self.nodes[x].parent {
            None => self.root = Some(y),
            Some(p) => {
                if self.nodes[p].right == Some(x) {
                    self.nodes[p].right = Some(y);
                } else {
                    self.nodes[p].left = Some(y);
                }
            }
        }
        self.nodes[y].right = Some(x);
        self.nodes[x].parent = Some(y);
    }

    fn is_red(&self, n: Option<usize>) -> bool {
        n.is_some_and(|n| self.nodes[n].red)
    }

    /// Classic red–black insert fix-up.
    fn rebalance(&mut self, mut x: usize) {
        while Some(x) != self.root {
            let Some(xp) = self.nodes[x].parent else {
                break;
            };
            if !self.nodes[xp].red {
                break;
            }
            let Some(xpp) = self.nodes[xp].parent else {
                break;
            };
            if self.nodes[xpp].left == Some(xp) {
                let y = self.nodes[xpp].right;
                if self.is_red(y) {
                    self.nodes[xp].red = false;
                    if let Some(y) = y {
                        self.nodes[y].red = false;
                    }
                    self.nodes[xpp].red = true;
                    x = xpp;
                } else {
                    let mut xp = xp;
                    if self.nodes[xp].right == Some(x) {
                        x = xp;
                        self.rotate_left(x);
                        xp = self.nodes[x].parent.unwrap_or(x);
                    }
                    self.nodes[xp].red = false;
                    self.nodes[xpp].red = true;
                    self.rotate_right(xpp);
                }
            } else {
                let y = self.nodes[xpp].left;
                if self.is_red(y) {
                    self.nodes[xp].red = false;
                    if let Some(y) = y {
                        self.nodes[y].red = false;
                    }
                    self.nodes[xpp].red = true;
                    x = xpp;
                } else {
                    let mut xp = xp;
                    if self.nodes[xp].left == Some(x) {
                        x = xp;
                        self.rotate_right(x);
                        xp = self.nodes[x].parent.unwrap_or(x);
                    }
                    self.nodes[xp].red = false;
                    self.nodes[xpp].red = true;
                    self.rotate_left(xpp);
                }
            }
        }
        if let Some(r) = self.root {
            self.nodes[r].red = false;
        }
    }

    /// Keys in iteration (in-order) order.
    #[must_use]
    pub fn in_order(&self) -> Vec<&K> {
        let mut out = Vec::with_capacity(self.nodes.len());
        let mut stack = Vec::new();
        let mut cur = self.root;
        while cur.is_some() || !stack.is_empty() {
            while let Some(n) = cur {
                stack.push(n);
                cur = self.nodes[n].left;
            }
            if let Some(n) = stack.pop() {
                out.push(&self.nodes[n].key);
                cur = self.nodes[n].right;
            }
        }
        out
    }

    /// Consume the set, returning keys in iteration order.
    #[must_use]
    pub fn into_in_order(self) -> Vec<K> {
        let order: Vec<usize> = {
            let mut out = Vec::with_capacity(self.nodes.len());
            let mut stack = Vec::new();
            let mut cur = self.root;
            while cur.is_some() || !stack.is_empty() {
                while let Some(n) = cur {
                    stack.push(n);
                    cur = self.nodes[n].left;
                }
                if let Some(n) = stack.pop() {
                    out.push(n);
                    cur = self.nodes[n].right;
                }
            }
            out
        };
        let mut slots: Vec<Option<K>> = self.nodes.into_iter().map(|n| Some(n.key)).collect();
        order.into_iter().filter_map(|i| slots[i].take()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Tiny deterministic generator for tests (xorshift64*).
    struct Rng(u64);
    impl Rng {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 >> 12;
            self.0 ^= self.0 << 25;
            self.0 ^= self.0 >> 27;
            self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }
        fn below(&mut self, n: u64) -> u64 {
            self.next() % n
        }
    }

    #[test]
    fn nth_element_postcondition() {
        let mut rng = Rng(0x9e37_79b9_7f4a_7c15);
        for len in 0..120 {
            for _ in 0..20 {
                let mut v: Vec<(u64, usize)> = (0..len).map(|i| (rng.below(7), i)).collect();
                let orig = v.clone();
                let nth = if len == 0 {
                    0
                } else {
                    usize::try_from(rng.below(len as u64)).unwrap()
                };
                nth_element(&mut v, nth, |a, b| a.0 < b.0);
                if len == 0 {
                    continue;
                }
                let mut sorted = orig.clone();
                sorted.sort_by_key(|e| e.0);
                assert_eq!(v[nth].0, sorted[nth].0);
                assert!(v[..nth].iter().all(|e| e.0 <= v[nth].0));
                assert!(v[nth + 1..].iter().all(|e| e.0 >= v[nth].0));
                let mut a = v.clone();
                a.sort();
                let mut b = orig;
                b.sort();
                assert_eq!(a, b, "permutation");
            }
        }
    }

    #[test]
    fn heap_select_path_is_a_valid_selection() {
        // Force the heap-select fallback (depth 0) directly.
        let mut rng = Rng(42);
        for len in 4..60 {
            let mut v: Vec<u64> = (0..len).map(|_| rng.below(10)).collect();
            let nth = usize::try_from(rng.below(len as u64)).unwrap();
            let mut sorted = v.clone();
            sorted.sort_unstable();
            let mut less = |a: &u64, b: &u64| a < b;
            assert!(introselect(&mut v, 0, nth, len, 0, &mut less));
            assert_eq!(v[nth], sorted[nth]);
            assert!(v[..nth].iter().all(|&e| e <= v[nth]));
            assert!(v[nth + 1..].iter().all(|&e| e >= v[nth]));
        }
    }

    #[test]
    fn rb_set_with_strict_order_is_sorted_and_unique() {
        let mut rng = Rng(7);
        let mut s = RbSet::new();
        let mut reference = std::collections::BTreeSet::new();
        for _ in 0..500 {
            let k = rng.below(200);
            assert_eq!(s.insert(k, |a, b| a < b), reference.insert(k));
        }
        let got: Vec<u64> = s.in_order().into_iter().copied().collect();
        let want: Vec<u64> = reference.into_iter().collect();
        assert_eq!(got, want);
    }

    #[test]
    fn rb_set_with_symmetric_comparator() {
        // `less(a, b) = a != b` (upstream segmentPairsLess shape): distinct
        // keys always descend left, so a fresh key becomes the leftmost.
        let mut s = RbSet::new();
        for k in [1, 2, 3] {
            assert!(s.insert(k, |a, b| a != b));
        }
        let got: Vec<i32> = s.in_order().into_iter().copied().collect();
        assert_eq!(got, vec![3, 2, 1]);
        // 2 is the root after the rotation: found on the descent path.
        assert!(!s.insert(2, |a, b| a != b));
        // 1 is the root's right child: not on the left spine → duplicated.
        assert!(s.insert(1, |a, b| a != b));
        assert_eq!(s.len(), 4);
    }
}
