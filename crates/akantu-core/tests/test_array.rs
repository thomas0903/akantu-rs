// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_common/test_array.cc
// (ArrayConstructor.*, ArrayFixture.*).

//! Port of `test_array.cc` (TEST-003, CORE-012…016) for the three upstream
//! value types `Real`, `UInt` and a non-trivial class.
//!
//! Upstream checks C++ view *types* (`VectorProxy<T>`, `MatrixProxy<T, 2,
//! 5>`) with `typeid`; the Rust equivalents are checked by construction
//! (the compiler fixes the types) and by the shapes they expose: dynamic
//! tuple views are `ArrayView`/slices, static ones `as_fixed::<N>()`
//! (`&[[T; N]]`), matrix views `tuple_matrix::<R, C>` / `as_matrix_of`
//! (column major, every `T` like upstream's `MatrixProxy<T>`; upstream's
//! run-time-size `make_view(array, R, C)` maps to
//! the same compile-time-size views). Upstream exceptions
//! (`debug::ArrayException` for incompatible view sizes) are `Err` values.

use akantu_core::array::ArrayView;
use akantu_core::array_shared::SharedArray;
use akantu_core::tensor::{as_matrix, as_matrix_of};
use akantu_core::{Array, UInt};

/// Upstream `NonTrivial` (a class type: non-POD storage path upstream).
#[derive(Clone, Debug, Default, PartialEq)]
struct NonTrivial {
    a: i32,
}

impl From<i32> for NonTrivial {
    fn from(a: i32) -> Self {
        Self { a }
    }
}

/// Value types of the typed test suites (`TestTypes = Real, UInt, NonTrivial`).
trait TestType: Clone + Default + PartialEq + std::fmt::Debug {
    fn from_i32(v: i32) -> Self;
}

impl TestType for f64 {
    fn from_i32(v: i32) -> Self {
        f64::from(v)
    }
}

impl TestType for UInt {
    fn from_i32(v: i32) -> Self {
        UInt::try_from(v).unwrap()
    }
}

impl TestType for NonTrivial {
    fn from_i32(v: i32) -> Self {
        NonTrivial::from(v)
    }
}

fn v<T: TestType>(x: i32) -> T {
    T::from_i32(x)
}

/// `ArrayFixture::SetUp`: `Array<T>(1000, 10)` (default values).
fn fixture<T: TestType>() -> Array<T> {
    Array::zeros(1000, 10)
}

// --- ArrayConstructor --------------------------------------------------------

fn construct_default_1<T: TestType>() {
    let array = Array::<T>::default();
    assert_eq!(0, array.len());
    assert_eq!(1, array.nb_component());
    assert_eq!("", array.id());
}

fn construct_default_2<T: TestType>() {
    let array = Array::<T>::zeros(1000, 1);
    assert_eq!(1000, array.len());
    assert_eq!(1, array.nb_component());
    assert_eq!("", array.id());
}

fn construct_default_3<T: TestType>() {
    let array = Array::<T>::zeros(1000, 10);
    assert_eq!(1000, array.len());
    assert_eq!(10, array.nb_component());
    assert_eq!("", array.id());
}

fn construct_default_4<T: TestType>() {
    let mut array = Array::<T>::with_id(10, "test");
    array.resize(1000, T::default());
    assert_eq!(1000, array.len());
    assert_eq!(10, array.nb_component());
    assert_eq!("test", array.id());
}

fn construct_default_5<T: TestType>() {
    let array = Array::<T>::filled(1000, 10, v(1));
    assert_eq!(1000, array.len());
    assert_eq!(10, array.nb_component());
    assert_eq!(v::<T>(1), array[(10, 6)]);
    assert_eq!("", array.id());
}

fn reserve<T: TestType>() {
    let mut array = Array::<T>::filled(1000, 10, v(1));
    // upstream reserve(2000) asks for a total of 2000 tuples
    array.reserve(2000 - array.len());
    assert_eq!(1000, array.len());
    assert_eq!(10, array.nb_component());
    assert!(2000 <= array.capacity());
}

// --- ArrayFixture --------------------------------------------------------------

fn copy<T: TestType>() {
    let array = fixture::<T>();
    let copy = array.clone();
    assert_eq!(1000, copy.len());
    assert_eq!(10, copy.nb_component());
    assert_ne!(array.as_slice().as_ptr(), copy.as_slice().as_ptr());
    assert_eq!(array, copy);
}

fn set<T: TestType>() {
    let mut arr = fixture::<T>();
    arr.fill(v(12));
    assert_eq!(v::<T>(12), arr[(156, 5)]);
    assert_eq!(v::<T>(12), arr[(520, 7)]);
    assert_eq!(v::<T>(12), arr[(999, 9)]);
}

fn resize<T: TestType>() {
    let mut arr = fixture::<T>();
    arr.resize(0, T::default());
    assert_eq!(0, arr.len());
    arr.resize(3000, T::default());
    assert_eq!(3000, arr.len());
    assert!(3000 <= arr.capacity());
    arr.resize(0, T::default());
    assert_eq!(0, arr.len());
}

fn push_back<T: TestType>() {
    let mut arr = fixture::<T>();
    arr.resize(0, T::default());
    assert_eq!(0, arr.len());

    // upstream push_back(value) on a POD array appends a tuple filled with
    // `value`; for class types it appends ONE value but counts one tuple
    // (UD-265) — the port appends a whole tuple for every type
    arr.push_value(v(1));
    assert_eq!(v::<T>(1), arr.as_slice()[0]);
    assert_eq!(1, arr.len());
    assert!(1 <= arr.capacity());
    assert!(arr.tuple(0).iter().all(|x| *x == v::<T>(1)));

    arr.reserve(10 - arr.len());
    assert!(10 <= arr.capacity());

    let ptr = arr.as_slice().as_ptr();
    let allocated = arr.capacity();
    arr.push_value(v(1));
    assert_eq!(2, arr.len());
    assert_eq!(v::<T>(1), arr.as_slice()[0]);
    assert_eq!(allocated, arr.capacity());
    assert_eq!(ptr, arr.as_slice().as_ptr());
}

fn view_vector_dynamic<T: TestType>() {
    let array = fixture::<T>();
    let view = ArrayView::new(array.as_slice(), 10).unwrap();
    let first = view.tuples().next().unwrap();
    assert_eq!(10, first.len());
    assert_eq!(10, view.tuple(0).len());
}

fn view_vector_static<T: TestType>() {
    let array = fixture::<T>();
    let view = array.as_fixed::<10>().unwrap();
    assert_eq!(10, view[0].len());
}

fn view_vector_wrong<T: TestType>() {
    let array = fixture::<T>();
    assert!(ArrayView::new(array.as_slice(), 11).is_err());
}

fn view_matrix_wrong<T: TestType>() {
    let array = fixture::<T>();
    assert!(ArrayView::new(array.as_slice(), 3 * 7).is_err());
}

fn const_view_vector<T: TestType>() {
    let array = fixture::<T>();
    let carray: &Array<T> = &array;
    let view = carray.view();
    assert_eq!(10, view.tuples().next().unwrap().len());
    assert_eq!(10, view.tuple(0).len());
}

fn enumerate_array<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    let view = ArrayView::new(array.as_slice(), 2).unwrap();
    let mut i = 0;
    for (k, data) in view.tuples().enumerate() {
        assert_eq!(i, k);
        assert_eq!(v::<T>(12), data[0]);
        assert_eq!(v::<T>(12), data[1]);
        i += 1;
    }
    assert_eq!(5000, i);
}

fn zip_array<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    let n = array.len() * array.nb_component() / 2;
    let view = ArrayView::new(array.as_slice(), 2).unwrap();
    let mut i = 0;
    for (k, data) in (0..n).zip(view.tuples()) {
        assert_eq!(i, k);
        assert_eq!(v::<T>(12), data[0]);
        i += 1;
    }
    assert_eq!(n, i);
}

fn iterator_increment<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    let mut it = array.tuples();
    let t = it.nth(10).unwrap();
    assert_eq!(v::<T>(12), t[0]);
}

fn iterator_fix_size<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    {
        let view = array.as_fixed::<10>().unwrap();
        assert_eq!(v::<T>(12), view[0][0]);
    }
    let view = array.as_fixed_mut::<10>().unwrap();
    view[0][0] = v(120);
    assert_eq!(v::<T>(120), array[(0, 0)]);
}

fn iterator_bracket<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    let vect = array.tuple(10);
    assert_eq!(v::<T>(12), vect[0]);
}

fn iterator_simple<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    let first = array.tuples().next().unwrap();
    assert_eq!(v::<T>(12), first[0]);
}

fn iterator_throw<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    // upstream begin(2 · nb_component) throws: the view size must equal the
    // number of components
    assert!(array.as_fixed::<20>().is_err());
}

fn iterator_range<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    let mut n = 0;
    for data in array.tuples() {
        assert_eq!(v::<T>(12), data[0]);
        n += 1;
    }
    assert_eq!(1000, n);
}

fn dynamic_size_iterator_filter<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    // upstream (*array)(3) = component 0 of tuple 3
    array[(3, 0)] = v(13);
    array[(50, 0)] = v(13);
    let list_filter = [3usize, 50];
    let view = ArrayView::new(array.as_slice(), 10).unwrap();
    for data in list_filter.iter().map(|&i| view.tuple(i)) {
        assert_eq!(v::<T>(13), data[0]);
        assert_eq!(v::<T>(12), data[1]);
    }
}

fn iterator_filter<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(12));
    array[(3, 0)] = v(13);
    array[(50, 0)] = v(13);
    let list_filter = [3usize, 50];
    let view = array.as_fixed::<10>().unwrap();
    for data in list_filter.iter().map(|&i| &view[i]) {
        assert_eq!(v::<T>(13), data[0]);
        assert_eq!(v::<T>(12), data[1]);
    }
}

macro_rules! typed_tests {
    ($($name:ident),+ $(,)?) => {
        mod real { $( #[test] fn $name() { super::$name::<f64>(); } )+ }
        mod uint { use super::UInt; $( #[test] fn $name() { super::$name::<UInt>(); } )+ }
        mod non_trivial { use super::NonTrivial; $( #[test] fn $name() { super::$name::<NonTrivial>(); } )+ }
    };
}

typed_tests!(
    construct_default_1,
    construct_default_2,
    construct_default_3,
    construct_default_4,
    construct_default_5,
    reserve,
    copy,
    set,
    resize,
    push_back,
    view_vector_dynamic,
    view_vector_static,
    view_vector_wrong,
    view_matrix_wrong,
    const_view_vector,
    enumerate_array,
    zip_array,
    iterator_increment,
    iterator_fix_size,
    iterator_bracket,
    iterator_simple,
    iterator_throw,
    iterator_range,
    dynamic_size_iterator_filter,
    iterator_filter,
);

// --- matrix views (every T) ---------------------

/// `ViewMatrixStatic`: `make_view(array, 2, 5)` — 2 × 5 matrices over each
/// 10-component tuple, column major.
fn view_matrix_static<T: TestType>() {
    let mut array = fixture::<T>();
    for (k, x) in array.as_mut_slice().iter_mut().enumerate() {
        *x = v(i32::try_from(k).unwrap());
    }
    let m = as_matrix_of::<T, 2, 5>(array.tuple(0)).unwrap();
    assert_eq!((m.len(), m[0].len()), (5, 2)); // 5 columns of 2 rows
    assert_eq!(m.iter().map(|c| c.len()).sum::<usize>(), 10);
    // column-major: M(i, j) = tuple[i + 2 j]
    assert_eq!(m[3][1], array.tuple(0)[1 + 2 * 3]);
    assert_eq!(m[3][1], v::<T>(7));
    let m = as_matrix_of::<T, 2, 5>(array.tuple(999)).unwrap();
    assert_eq!(m[4][1], v::<T>(9999));
}

/// `ViewMatrixDynamic`: `make_view<2, 5>(array)` (compile-time sizes).
fn view_matrix_dynamic<T: TestType>() {
    let mut array = fixture::<T>();
    array.fill(v(3));
    let m = array.tuple_matrix::<2, 5>(999).unwrap();
    assert_eq!((m[0].len(), m.len()), (2, 5));
    assert!(m.iter().flatten().all(|x| *x == v::<T>(3)));
    assert!(array.tuple_matrix::<3, 3>(0).is_err());
    array.tuple_matrix_mut::<2, 5>(10).unwrap()[4][1] = v(4);
    assert_eq!(array[(10, 9)], v::<T>(4));
    // the same views through the exportable storage's guards (D-056)
    let mut shared = SharedArray::new(array);
    shared
        .write()
        .unwrap()
        .tuple_matrix_mut::<2, 5>(11)
        .unwrap()[0][1] = v(5);
    let g = shared.read().unwrap();
    assert_eq!(g[(11, 1)], v::<T>(5));
    assert_eq!(g.tuple_matrix::<2, 5>(10).unwrap()[4][1], v::<T>(4));
    assert!(g.tuple_matrix::<3, 3>(0).is_err());
}

/// `ViewMatrixIter`: `make_view(array, 10, 10)` reinterprets the 10 000
/// values as 100 matrices of 10 × 10.
fn view_matrix_iter<T: TestType>() {
    let array = fixture::<T>();
    let view = ArrayView::new(array.as_slice(), 100).unwrap();
    let mut count = 0;
    for t in view.tuples() {
        let m = as_matrix_of::<T, 10, 10>(t).unwrap();
        assert_eq!((m.len(), m[0].len()), (10, 10));
        count += 1;
    }
    assert_eq!(100, count);
}

mod matrix_views {
    use super::{NonTrivial, UInt};
    macro_rules! typed {
        ($($name:ident),+) => {
            mod real { $( #[test] fn $name() { super::super::$name::<f64>(); } )+ }
            mod uint { use super::UInt; $( #[test] fn $name() { super::super::$name::<UInt>(); } )+ }
            mod non_trivial { use super::NonTrivial; $( #[test] fn $name() { super::super::$name::<NonTrivial>(); } )+ }
        };
    }
    typed!(view_matrix_static, view_matrix_dynamic, view_matrix_iter);
}

/// The `Real` views keep their `tensor::Matrix` type (`as_matrix` is
/// `as_matrix_of::<Real, _, _>`).
#[test]
fn real_matrix_views_are_tensor_matrices() {
    let array = fixture::<f64>();
    let m: &akantu_core::tensor::Matrix<2, 5> = as_matrix(array.tuple(0)).unwrap();
    let n: &akantu_core::tensor::Matrix<2, 5> = array.tuple_matrix::<2, 5>(0).unwrap();
    assert_eq!(m, n);
}
