<!-- SPDX-License-Identifier: LGPL-3.0-or-later -->
# Clean-room provenance log — `src/cxx.rs` (D-030)

Behavioural models of two C++ standard-library facilities whose
*unspecified* details are observable in upstream's geometry output:

* `std::nth_element` — called by CGAL's `AABB_traits::Split_primitives`
  at every node of an AABB tree (`AABB_traits.h`); the permutation it leaves
  (order of the two halves and of equivalent elements) fixes the tree shape
  and therefore the order in which `AABB_tree::all_intersections` reports
  intersections, which becomes the insertion order of
  `MeshSegmentIntersector::computeSegments`.
* `std::set<std::pair<Segment_3, Int>, segmentPairsLess>::insert` —
  `segmentPairsLess` (`src/geometry/geom_helper_functions.hh:66-73`) casts a
  `Comparison_result` to `bool` and is therefore **not** a strict weak
  ordering (UD-408): `less(a, b)` is true whenever the two segments differ.
  Which segments are kept and the iteration order (= element order of the
  result mesh) depend on the red–black tree shape.

The oracle uses GCC 14 libstdc++ (GPL-3.0 with the GCC Runtime Library
Exception: D-030 forbids reading or translating it).

Implementer: a coding agent of the port, 2026-11-11.

## 1. Rules followed

**Not read** at any time during this work: libstdc++ or libc++ sources or
headers (`/usr/include/c++/**` was never opened, listed, grepped or copied,
on macOS, on Linux or in Docker); no disassembly of compiled library code.
Compiler diagnostics never displayed library header text.

**Inputs used:**

* Published algorithms: D. R. Musser, *Introspective Sorting and Selection
  Algorithms*, Software — Practice and Experience 27(8), 1997
  (introselect: median-of-three quickselect, depth limit proportional to
  `log n`, heap-selection fallback, insertion sort of small ranges);
  C. A. R. Hoare's partition scheme; J. W. J. Williams / R. W. Floyd heaps
  and the "bottom-up" heap adjustment (sift the hole to a leaf, then push the
  value up; I. Wegener, *Bottom-up heapsort*, TCS 118, 1993); L. J. Guibas,
  R. Sedgewick, *A dichromatic framework for balanced trees* (1978) and
  Cormen–Leiserson–Rivest–Stein, *Introduction to Algorithms*, ch. 13
  (red–black insertion fix-up); M. D. McIlroy, *A killer adversary for
  quicksort*, Software — Practice and Experience 29(4), 1999 (used only to
  build test inputs that reach the depth limit).
* The ISO C++ specification of `std::nth_element`, `std::set`, `std::min`,
  `std::max` (public working draft N4950), from the implementer's knowledge
  of the published wording.
* Black-box experiments B1, B2 (below), which call only the public API and
  record observable results.

**Disclosure.** The implementer is a language model whose general
knowledge may include descriptions of standard-library implementations.
All such knowledge was treated as a *hypothesis only*: every observable
design choice was confirmed against the black-box fixtures, and the
plausible alternatives were implemented and shown to disagree with them
(V/W-series, §3). Code was written from the published algorithms above, in
our own structure (index-based, swap-only Rust).

## 2. Black-box experiments

Programs in `oracle/probes/geometry/cleanroom/` (`run.sh` builds and runs
them with `-std=c++20 -O2 -ffp-contract=off`); they were run on a remote x86_64 host in `debian:trixie` with
`g++ (Debian 14.2.0-19) 14.2.0`, `libstdc++6 14.2.0-19` — the oracle's
distribution and compiler (`docs/ORACLE_BUILD.md`). Recorded toolchain:
`crates/akantu-geometry/tests/fixtures/cleanroom/toolchain.txt`.

| Id | Program | Records | Fixture |
|---|---|---|---|
| B1 | `nth_element_bb.cc` | 1 158 calls of `std::nth_element` with a counting comparator on `(key, tag)` elements: every length 0–64 × key ranges {1, 2, 3, 8, 10⁶} (ties), lengths 100–3001, sorted / reversed / equal / organ-pipe / saw-tooth inputs, and 27 inputs frozen by McIlroy's adversary (depth limit reached); output = the full permutation and the comparison count | `tests/fixtures/cleanroom/nth_element.json` |
| B2 | `rb_set_bb.cc` | 1 458 insertion sequences into `std::set` with three comparators: strict `a < b`, the symmetric `a != b` (shape of `segmentPairsLess`) and `a <= b`; output = each `insert(...).second` and the iteration order | `tests/fixtures/cleanroom/rb_set.json` |

## 3. Design choices and evidence

Replay test `tests/cleanroom_bb.rs` (B1, B2: **0 mismatches**, permutation
*and* comparison count for B1; the heap-selection branch is reached in ≥ 10
adversarial cases). Alternatives in `tests/cleanroom_variants.rs`, each
required to disagree with the fixtures:

| Choice (model) | Alternative(s) rejected | Test |
|---|---|---|
| Median of `(first+1, mid, last−1)` swapped into `first`, Hoare partition of `[first+1, last)` with the pivot as sentinel | median of `(first, mid, last−1)` | `v2_*` |
| Partition while the range has more than 3 elements, then straight insertion sort | thresholds 4, 5, 8, 16, 32 (below 3 the unguarded partition is invalid) | `v3_*` |
| Depth limit `2⌊log₂ n⌋`, then heap selection of `[first, nth+1)` and a final swap of `first` and `nth` | unlimited depth, `1·⌊log₂ n⌋`, `3·⌊log₂ n⌋` | `v4_*` |
| Bottom-up heap adjustment (make-heap from the last parent down, pop by swapping the top with the candidate) | classic top-down sift-down | `v5_*` |
| Set: descend with `comp(k, node)` (left if true), then — unless the last step went left at the leftmost node — test `comp(predecessor, k)`; insert as left child iff `comp(k, parent)`; red–black fix-up | two-sided test at every node (`comp(k,n)` / `comp(n,k)` / equal) | `w1_*` |
| Balanced (red–black) tree | the same rule on an unbalanced BST | `w2_*` |

Re-implementations of the model inside the variant harness agree with the
fixtures (`v1_*`, `w0_*`).

## 4. Scope and limits

* The models cover what upstream observes: `nth_element` on random-access
  ranges with an arbitrary strict-weak-order comparator (CGAL `less_x/y/z`
  on doubles), and `set::insert` without hint followed by iteration.
  Hinted insertion, erasure, `std::sort` and other algorithms are not
  modelled.
* NaN coordinates make CGAL's comparator inconsistent; the model then still
  follows the same comparison sequence (the comparator is called with the
  same arguments in the same order, B1's comparison counts), so results
  agree as long as the comparator answers agree.

## 5. Addendum: fix round, 2026-11-11

The §1 rules held for this round too: no libstdc++/libgcc source or header was
opened, listed, searched or copied, and no compiled library code was
disassembled. Only the model in `src/cxx.rs`, the published algorithms of §1
and black-box output were used.

**(a) Identifier names.** A review noted that some helper names of
`src/cxx.rs` (`move_median_to_first(result, a, b, c)`,
`unguarded_partition`, `partition_pivot`, `adjust_heap`, `make_heap`,
`heap_select`) coincide with libstdc++'s internal function names. Those
names also appear outside the library source, as public symbol names (in
backtraces and profiles of any program that calls `std::nth_element`) and
in the documentation generated from GCC's headers. They are also the usual
names of the algorithm steps. A name carries no code, but to remove the
appearance of derivation this round renamed the helpers after the
literature of §1. The behaviour is unchanged (every B1/B2/B3 replay and V
test passes):

| Before | After | Source of the new name |
|---|---|---|
| `move_median_to_first` | `median_of_three_into` | median-of-three pivot (Musser 1997; Sedgewick) |
| `unguarded_partition` | `hoare_partition_sentinel` | Hoare partition with the pivot as sentinel |
| `partition_pivot` | `median_of_three_partition` | — |
| `adjust_heap` | `bottom_up_sift` | bottom-up heap adjustment (Wegener 1993) |
| `make_heap` | `build_max_heap` | BUILD-MAX-HEAP (Cormen–Leiserson–Rivest–Stein, ch. 6) |
| `heap_select` | `select_by_heap` | heap selection fallback of introselect (Musser 1997) |

`introselect` keeps its name: it is the published name of Musser's
algorithm.

**(b) Second comparison on insert.** `set::insert` calls `comp(k, parent)`
once more after the descent (`src/cxx.rs`, `RbSet::insert`). B2 records no
comparison counts, so B2 does not show this call. The review's independent
black-box re-check shows it. That re-check (program `bb_recheck.cc` and
replay `review_bb_recheck.rs`) covers
1,200 insertion sequences with five comparators: strict `<`, `!=`, `<=`,
`>` and a deterministic random tournament. Its output includes the
**comparison count** of every insertion, and the model matches all of them
(0 mismatches, `g++ (Debian 14.2.0-19)`, x86_64).

**(c) Median decision tree and heap tie rule.** Two new variant tests in
`tests/cleanroom_variants.rs`:

* `v6_other_median_decision_tree_is_rejected`: after `!(a < b)` the
  alternative tests `b < c` before `a < c`. It picks the same median for
  distinct keys but disagrees with **872** B1 cases (comparison counts and
  ties). B1 therefore discriminates the decision tree.
* `v7_heap_child_tie_rule_is_rejected`: when the two children are equal,
  the bottom-up sift descends to the **right** child (to the left only if
  `right < left`). The alternative (left unless `left < right`) agrees with
  **all** B1 cases, because B1 never reaches the heap-selection fallback with
  tied keys. New experiment **B3** fills that gap:

| Id | Program | Records | Fixture |
|---|---|---|---|
| B3 | `nth_element_input_bb.cc` (stdin: `b3_inputs.txt`) | 100 `std::nth_element` calls on B1's adversarial inputs with 1–4 keys duplicated, all reaching the heap-selection fallback (80 chosen so that the two tie rules give different permutations, 20 where they agree), lengths 33–128; permutation and comparison count | `tests/fixtures/cleanroom/nth_element_b3.json` |

  The inputs were chosen with the model itself: random duplications of the
  18 B1 heap-branch inputs, keeping those for which the model still takes
  the fallback. Only the inputs depend on the model; the expected outputs
  come from the black box. B3 ran with the same toolchain as B1/B2
  (`oracle/probes/geometry/cleanroom/run.sh akantu-oracle:dev`, a remote x86_64 sandbox, `g++ (Debian 14.2.0-19) 14.2.0`,
  `libstdc++6 14.2.0-19`, x86_64). In the same run B1 and B2 were
  regenerated byte-identical to the committed fixtures. Replay
  `cleanroom_bb::b3_nth_element_heap_ties_match_libstdcxx`: **0
  mismatches** (100/100 permutations and comparison counts, every case in
  the fallback). The alternative tie rule disagrees on **81** B3 cases, so
  the tie rule is now black-box evidenced.
