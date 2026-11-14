// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs regression tests of
// `Array::find` ported from Akantu (EPFL/LSMS): src/common/aka_array.cc
// (`Array<Real>::find`, lines 35-44), src/common/aka_array_tmpl.hh
// (generic `Array<T>::find`, lines 639-645; `get_iterator`, lines 819-844).

//! Expectations derived from the upstream source (no oracle needed):
//!
//! * `Array<Real>::find` matches the first stored `a` with `|a − elem| ≤ ε`
//!   (`ε = numeric_limits<double>::epsilon()`, absolute) and returns
//!   `end − it` (UD-042: the port's `find` returns the index, its
//!   `find_upstream` the raw value);
//! * the generic `find` is `std::find` (`==`) and returns `it − begin`;
//! * both iterate with the scalar iterator, which throws unless
//!   `nb_component · size == size` (empty arrays never throw).

use akantu_core::{Array, Element, ElementType, Error, GhostType, Idx, Real};

fn reals(v: &[Real]) -> Array<Real> {
    Array::from_vec(v.to_vec(), 1).unwrap()
}

#[test]
fn real_find_uses_upstream_absolute_epsilon() {
    let eps = Real::EPSILON;
    let a = reals(&[3.0, 1.0, 2.0]);
    assert_eq!(a.find(&1.0).unwrap(), Some(1));
    assert_eq!(a.find_upstream(&1.0).unwrap(), 2); // end − it = 3 − 1

    // |1 − (1 + ε)| = ε ≤ ε: found; 1 + 2ε: 2ε > ε, not found; the double
    // below 1 (1 − ε/2) is found
    let one = reals(&[1.0]);
    assert_eq!(one.find(&(1.0 + eps)).unwrap(), Some(0));
    assert_eq!(one.find_upstream(&(1.0 + eps)).unwrap(), 1);
    assert_eq!(one.find(&(1.0 + 2.0 * eps)).unwrap(), None);
    assert_eq!(one.find_upstream(&(1.0 + 2.0 * eps)).unwrap(), -1);
    assert_eq!(one.find(&(1.0 - eps / 2.0)).unwrap(), Some(0));

    // absolute, not relative: the first stored value within ε of 0 wins
    let z = reals(&[5.0, 1e-300, 0.0]);
    assert_eq!(z.find(&0.0).unwrap(), Some(1));
    assert_eq!(z.find_upstream(&0.0).unwrap(), 2);
    assert_eq!(reals(&[0.0]).find(&1e-17).unwrap(), Some(0));
    assert_eq!(reals(&[0.0]).find(&-0.0).unwrap(), Some(0));
    assert_eq!(reals(&[-eps]).find(&0.0).unwrap(), Some(0));
    // large magnitudes: only the value itself (1e20 + 1 ulp differs by 16384)
    let big = 1e20_f64;
    let next = Real::from_bits(big.to_bits() + 1);
    assert_eq!(reals(&[next]).find(&big).unwrap(), None);
    assert_eq!(reals(&[next, big]).find(&big).unwrap(), Some(1));

    // NaN and the infinities never match (∞ − ∞ = NaN)
    assert_eq!(
        reals(&[Real::INFINITY]).find(&Real::INFINITY).unwrap(),
        None
    );
    assert_eq!(
        reals(&[Real::NEG_INFINITY])
            .find(&Real::NEG_INFINITY)
            .unwrap(),
        None
    );
    assert_eq!(reals(&[Real::NAN]).find(&Real::NAN).unwrap(), None);
    assert_eq!(
        reals(&[Real::INFINITY])
            .find_upstream(&Real::INFINITY)
            .unwrap(),
        -1
    );

    // a match on the first value is `size` upstream (never 0)
    let b = reals(&[7.0, 8.0, 9.0, 10.0]);
    assert_eq!(b.find(&7.0).unwrap(), Some(0));
    assert_eq!(b.find_upstream(&7.0).unwrap(), 4);
    assert_eq!(b.find_upstream(&10.0).unwrap(), 1);
}

#[test]
fn find_on_multi_component_arrays_is_an_error_unless_empty() {
    let a = Array::from_vec(vec![0.0, 1.0, 2.0, 3.0], 2).unwrap();
    assert!(matches!(
        a.find(&1.0),
        Err(Error::DimensionMismatch {
            expected: 1,
            got: 2,
            ..
        })
    ));
    assert!(a.find_upstream(&1.0).is_err());
    let ints: Array<Idx> = Array::from_vec(vec![1, 2, 3], 3).unwrap();
    assert!(ints.find(&2).is_err());

    // `begin()` checks `nb_component · size == size`: empty arrays pass
    let empty: Array<Real> = Array::new(3);
    assert_eq!(empty.find(&0.0).unwrap(), None);
    assert_eq!(empty.find_upstream(&0.0).unwrap(), -1);
    // zero components and a non-zero size: 0 ≠ size, upstream throws
    let zero: Array<Real> = Array::try_from_vec_with_len(Vec::new(), 2, 0).unwrap();
    assert!(zero.find(&0.0).is_err());
}

#[test]
fn generic_find_is_exact_and_returns_the_index() {
    let a: Array<Idx> = Array::from_vec(vec![4, 7, 7], 1).unwrap();
    assert_eq!(a.find(&7).unwrap(), Some(1));
    assert_eq!(a.find_upstream(&7).unwrap(), 1); // it − begin
    assert_eq!(a.find(&9).unwrap(), None);
    assert_eq!(a.find_upstream(&9).unwrap(), -1);

    // only `Array<Real>` (double) is specialised: float stays exact
    let f: Array<f32> = Array::from_vec(vec![1.0], 1).unwrap();
    assert_eq!(f.find(&(1.0 + f32::EPSILON)).unwrap(), None);
    assert_eq!(f.find(&1.0).unwrap(), Some(0));

    let tri = |i| Element::new(ElementType::Triangle3, i, GhostType::NotGhost);
    let e: Array<Element> = Array::from_vec(vec![tri(0), tri(3)], 1).unwrap();
    assert_eq!(e.find(&tri(3)).unwrap(), Some(1));
    assert_eq!(e.find_upstream(&tri(3)).unwrap(), 1);
}
