// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): third-party/akantu_iterators/test/
// test_iterators.cc (zip, named zip, arange, enumerate, transform, filter,
// filter_if, concat, repeat_n) and the arange semantics of
// include/iterators/aka_arange_iterator.hh.

//! Port of `akantu_iterators`' `test_iterators.cc` (ITER-001…006) — the
//! intent of each upstream case expressed with the Rust standard iterator
//! adaptors that replace the library (proposed status `Superseded`, see
//! `docs/FEATURE_PARITY.md` ITER rows). Each test names the upstream case
//! and the Rust idiom ported code uses:
//!
//! | upstream | Rust |
//! |---|---|
//! | `zip(a, b, …)` | `a.iter().zip(&b)` / `izip`-style nested zips |
//! | named zip `zip(get<"a"_h>() = a, …)`, `data["a"_n]` | a struct or named bindings in the pattern |
//! | `zip_cat`, `zip_append`, `zip_replace`, `zip_remove` | tuple patterns `((a, b), c)` built by the closure |
//! | `arange(stop)`, `arange(start, stop[, step])` | `0..stop`, `(start..stop).step_by(step)`, negative steps `(stop + 1..=start).rev().step_by(…)` |
//! | `enumerate(a, b)` | `a.iter().zip(&b).enumerate()` |
//! | `make_keys_adaptor(map)` / `make_values_adaptor(map)` | `BTreeMap::keys` / `values` (same sorted order as `std::map`) |
//! | `make_transform_adaptor(c, f)` | `c.iter().map(f)` |
//! | `filter(index_list, c)` | `index_list.iter().map(\|&i\| &c[i])` |
//! | `filter_if(c, pred)` | `c.iter().filter(pred)` |
//! | `concat(r1, r2, …)` | `r1.chain(r2)…` |
//! | `repeat_n(c, n)` (each element n times) | `c.iter().flat_map(\|x\| std::iter::repeat_n(x, n))` |
//!
//! Upstream `arange(start, stop, step)` rounds `stop` up to `start + k·step`
//! and iterates until it reaches it: identical to the Rust ranges for every
//! well-formed range; an inverted range (`stop < start` with `step > 0`)
//! never terminates upstream (UD-269) and is empty in Rust.

use std::collections::BTreeMap;

/// Upstream `ArangeContainer` size: `(stop' − start) / step` with `stop'`
/// rounded up to a multiple of `step` from `start`.
fn upstream_arange(start: i64, stop: i64, step: i64) -> Vec<i64> {
    let stop = if (stop - start) % step == 0 {
        stop
    } else {
        start + (1 + (stop - start) / step) * step
    };
    let n = (stop - start) / step;
    (0..n).map(|i| start + i * step).collect()
}

#[test]
fn test_zip_simple() {
    // TestZipFixutre.SimpleTest: lock-step iteration, mutation through zip
    let mut a = vec![0, 10, 20, 30, 40];
    let b = [0, 1, 2, 3, 4];
    for (x, y) in a.iter_mut().zip(&b) {
        *x += y;
    }
    assert_eq!(a, [0, 11, 22, 33, 44]);
    // MixteTest / ConstTest: shared and mutable references mixed
    let mut c = vec![0; 5];
    for ((x, y), z) in a.iter().zip(&b).zip(c.iter_mut()) {
        *z = x - y;
    }
    assert_eq!(c, [0, 10, 20, 30, 40]);
    // Bidirectional / RandomAccess: double-ended and indexed access
    let rev: Vec<_> = a.iter().zip(&b).rev().map(|(x, y)| x - y).collect();
    assert_eq!(rev, [40, 30, 20, 10, 0]);
    assert_eq!(a.iter().zip(&b).nth(3), Some((&33, &3)));
}

#[test]
fn test_zip_cat_append_replace_remove() {
    // Cat / Append / Replace / Remove: building the tuple in the closure
    let a = [1, 2, 3];
    let b = [4, 5, 6];
    let c = [7, 8, 9];
    let cat: Vec<(i32, i32, i32)> = a
        .iter()
        .zip(&b)
        .zip(&c)
        .map(|((x, y), z)| (*x, *y, *z))
        .collect();
    assert_eq!(cat[1], (2, 5, 8));
    let replaced: Vec<(i32, i32)> = cat.iter().map(|&(x, _, z)| (x, z)).collect();
    assert_eq!(replaced, [(1, 7), (2, 8), (3, 9)]);
}

#[test]
fn test_named_zip_simple() {
    // TestNamedZipFixutre.Simple: named members → struct / named bindings
    let a = [0, 10, 20, 30, 40];
    let mut b = vec![0, 1, 2, 3, 4];
    struct Named<'x> {
        a: &'x i32,
        b: &'x mut i32,
    }
    for data in a.iter().zip(b.iter_mut()).map(|(a, b)| Named { a, b }) {
        *data.b *= 10;
        assert_eq!(*data.b, *data.a);
    }
    for (a, b) in a.iter().zip(&b) {
        assert_eq!(b, a);
    }
}

#[test]
fn test_arange_iterator() {
    // Stop
    assert!((0..10).zip(0..).all(|(i, r)| i == r));
    // StartStop
    assert_eq!((1..10).collect::<Vec<_>>(), (1..10).collect::<Vec<_>>());
    // StartStopStep: 1, 3, …, 21 (stop 22 rounded to 23)
    let r: Vec<i64> = (1..22).step_by(2).collect();
    assert_eq!(r, upstream_arange(1, 22, 2));
    assert_eq!(r.last(), Some(&21));
    // StartStopStepZipped: negative step
    let neg: Vec<i64> = (-9..=-1).rev().collect();
    assert_eq!(neg, upstream_arange(-1, -10, -1));
    for ((i1, i2), k) in neg.iter().zip((1..18).step_by(2)).zip(0..) {
        assert_eq!(*i1, -1 - k);
        assert_eq!(i2, 1 + 2 * k);
    }
    // sizes agree on non-divisible ranges
    for (s, e, st) in [(0, 10, 3), (5, 6, 4), (0, 0, 1), (-7, 8, 5)] {
        let rust: Vec<i64> = (s..e).step_by(usize::try_from(st).unwrap()).collect();
        assert_eq!(rust, upstream_arange(s, e, st));
    }
    let rust_neg: Vec<i64> = (-6..=10).rev().step_by(4).collect();
    assert_eq!(rust_neg, upstream_arange(10, -7, -4));
}

#[test]
fn test_enumerate_iterator() {
    // TestEnumerateIterator.SimpleTest
    let a = [0, 10, 20, 30, 40];
    let b = [0, 2, 4, 6, 8];
    for (i, (x, y)) in a.iter().zip(&b).enumerate() {
        let i = i32::try_from(i).unwrap();
        assert_eq!(i * 10, *x);
        assert_eq!(i * 2, *y);
    }
}

#[test]
fn test_transform_adaptor() {
    // Keys / Values (std::map order = BTreeMap order; duplicate key ignored)
    let map: BTreeMap<String, i32> = [("1", 1), ("2", 2), ("3", 3), ("3", 3), ("4", 4)]
        .into_iter()
        .map(|(k, v)| (k.to_owned(), v))
        .collect();
    let keys: Vec<char> = map.keys().map(|k| k.chars().next().unwrap()).collect();
    assert_eq!(keys, ['1', '2', '3', '4']);
    assert_eq!(map.values().copied().collect::<Vec<_>>(), [1, 2, 3, 4]);
    // Lambda / LambdaLambda / Function / Functor
    fn plus1(v: i32) -> i32 {
        v + 1
    }
    struct Plus(i32);
    impl Plus {
        fn call(&self, b: i32) -> i32 {
            self.0 + b
        }
    }
    let p = Plus(1);
    for (x, y) in (0..10).zip((0..10).map(|v| v + 1)) {
        assert_eq!(x + 1, y);
    }
    for (c, d) in (1..).zip(map.values().map(|v| v + 1)) {
        assert_eq!(c + 1, d);
    }
    assert!((0..10).zip((0..10).map(plus1)).all(|(x, y)| x + 1 == y));
    assert!(
        (0..10)
            .zip((0..10).map(|v| p.call(v)))
            .all(|(x, y)| x + 1 == y)
    );
}

#[test]
fn test_filtered_iterator() {
    // TestFilteredIterator.Simple / Temporary
    let values: Vec<i32> = (0..=10).collect();
    let filter = [1usize, 3, 4, 10, 8, 6];
    for (f, v) in filter.iter().zip(filter.iter().map(|&i| values[i])) {
        assert_eq!(i32::try_from(*f).unwrap(), v);
    }
    // TestFilteredIfIterator.Simple / Temporary
    assert!(values.iter().filter(|&&a| a % 2 == 0).all(|a| a % 2 == 0));
    assert_eq!((0..10).filter(|a| a % 2 == 0).count(), 5);
}

#[test]
fn test_concatenate_iterator() {
    // TestConcatenateIterator.SimpleTest
    for (a, b) in (0..13).zip((0..5).chain(5..10).chain(10..13)) {
        assert_eq!(a, b);
    }
}

#[test]
fn test_repeat_n_iterator() {
    // TestRepeatNIterator.SimpleTest: each element repeated n times
    let reference = [0, 0, 0, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 4];
    let rep: Vec<i32> = (0..5).flat_map(|x| std::iter::repeat_n(x, 3)).collect();
    assert_eq!(rep, reference);
    // IncTest: random access into the repeated sequence
    let v = [0, 1, 2, 3, 4];
    let r: Vec<i32> = v.iter().flat_map(|&x| std::iter::repeat_n(x, 3)).collect();
    assert_eq!((r[0], r[3], r[4], r[5]), (0, 1, 1, 1));
    assert_eq!((r[1 + 3], r[1 + 4], r[1 + 5]), (1, 1, 2));
    assert_eq!((r[5], *r.last().unwrap()), (1, 4));
}
