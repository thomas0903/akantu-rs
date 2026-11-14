// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_common/test_tensors.cc
// (TensorConstructorFixture.*, TensorFixture.*).

//! Port of `test_tensors.cc` (TEST-004, CORE-018): construction, proxies,
//! assignment, arithmetic, column slicing and column iteration of the dense
//! `Vector`/`Matrix` types. `MatrixEigs` (CORE-019) is ported in
//! `akantu-linalg` (`dense::tests::upstream_test_tensors_matrix_eigs`, the
//! eigen-solvers live there) and not repeated here.
//!
//! Type mapping (D-011): upstream `Vector<double>` / `Matrix<double>` with
//! the reference sizes 24 and 4 × 6 → fixed [`Vector<24>`] / [`Matrix<4, 6>`]
//! and the dynamic [`MatrixX`]; `VectorProxy` / `MatrixProxy` (`Eigen::Map`
//! over external memory) → [`as_matrix`] / [`as_matrix_mut`] (zero-copy
//! column-major views of a slice). Upstream compares with
//! `EXPECT_DOUBLE_EQ` (4 ulp); every expected value here is the same single
//! IEEE operation as the upstream expression, so the comparisons are exact.
//! The reference values come from `rand()` upstream (integers stored as
//! doubles); here from the `minstd_rand0` engine of `akantu_core::random`.

// Index loops mirror upstream's `for (int i = 0; i < size_; ++i)` checks of
// two arrays at once.
#![allow(clippy::needless_range_loop)]

use akantu_core::Real;
use akantu_core::random::MinstdRand0;
use akantu_core::tensor::{
    Matrix, MatrixExt, MatrixX, Vector, VectorExt, as_matrix, as_matrix_mut,
};
use akantu_validation::reduce::NanMinMax;

const SIZE: usize = 24;
const ROWS: usize = 4;
const COLS: usize = 6;

/// `TensorConstructorFixture`: 24 random reference values (integers, as
/// `rand()` gives).
fn reference() -> [Real; SIZE] {
    let mut g = MinstdRand0::with_seed(42);
    std::array::from_fn(|_| Real::from(g.next_u32()))
}

/// Another random scalar (`double r = rand()`).
fn rand_scalar() -> Real {
    let mut g = MinstdRand0::with_seed(7);
    Real::from(g.next_u32())
}

/// `vref`: proxy over the reference values.
fn vref(r: &[Real; SIZE]) -> &Matrix<SIZE, 1> {
    as_matrix::<SIZE, 1>(r).unwrap()
}

/// `mref`: 4 × 6 proxy over the reference values (column major).
fn mref(r: &[Real; SIZE]) -> &Matrix<ROWS, COLS> {
    as_matrix::<ROWS, COLS>(r).unwrap()
}

fn compare_to_ref(r: &[Real; SIZE], data: &[Real]) {
    assert_eq!(data.len(), SIZE);
    for (a, b) in r.iter().zip(data) {
        assert_eq!(a.to_bits(), b.to_bits());
    }
}

// --- Vector -------------------------------------------------------------------

#[test]
fn vector_default_construct() {
    let v = MatrixX::default();
    assert_eq!(0, v.as_slice().len());
    let w: Vec<Real> = Vec::new();
    assert!(w.is_empty());
}

#[test]
fn vector_construct1() {
    let v = MatrixX::zeros(SIZE, 1);
    assert_eq!(SIZE, v.as_slice().len());
    let w: Vector<SIZE> = Vector::zeros();
    assert_eq!(SIZE, w.len());
}

#[test]
fn vector_construct_wrapped() {
    let r = reference();
    let v = as_matrix::<SIZE, 1>(&r).unwrap();
    assert_eq!(SIZE, v[0].len());
    assert_eq!(v.as_ptr().cast::<Real>(), r.as_ptr());
    for i in 0..SIZE {
        assert_eq!(r[i], v.at(i, 0));
        assert_eq!(r[i], v[0][i]);
    }
}

#[test]
fn vector_construct_initializer() {
    let v: Vector<6> = [0., 1., 2., 3., 4., 5.];
    assert_eq!(6, v.len());
    for (i, x) in v.iter().enumerate() {
        assert_eq!(i as Real, *x);
    }
}

#[test]
fn vector_construct_copy1() {
    let r = reference();
    let v: Vector<SIZE> = vref(&r)[0];
    assert_eq!(SIZE, v.len());
    compare_to_ref(&r, &v);
}

#[test]
fn vector_construct_proxy1() {
    let r = reference();
    let p = vref(&r);
    assert_eq!(SIZE, p[0].len());
    compare_to_ref(&r, p.as_flat());
    let v = MatrixX::from_fixed(p);
    assert_eq!(SIZE, v.as_slice().len());
    compare_to_ref(&r, v.as_slice());
}

#[test]
fn vector_equal() {
    let r = reference();
    let v: Vector<SIZE> = vref(&r)[0];
    compare_to_ref(&r, &v);
    assert_eq!(SIZE, v.len());
}

#[test]
fn vector_equal_proxy() {
    let r = reference();
    let proxy = *vref(&r);
    let v = MatrixX::from_fixed(&proxy);
    compare_to_ref(&r, v.as_slice());
    assert_eq!(SIZE, v.rows());
}

#[test]
fn vector_equal_proxy2() {
    let r = reference();
    let mut store = [0.0; SIZE];
    {
        let v = as_matrix_mut::<SIZE, 1>(&mut store).unwrap();
        *v = *vref(&r);
        compare_to_ref(&r, v.as_flat());
    }
    compare_to_ref(&r, &store);
}

#[test]
fn vector_set() {
    let r = reference();
    let mut v: Vector<SIZE> = vref(&r)[0];
    compare_to_ref(&r, &v);
    let s = rand_scalar();
    v.fill(s);
    assert!(v.iter().all(|&x| x == s));
}

#[test]
fn vector_clear() {
    let r = reference();
    let mut v: Vector<SIZE> = vref(&r)[0];
    compare_to_ref(&r, &v);
    v = Vector::zeros();
    assert!(v.iter().all(|&x| x == 0.0));
}

#[test]
fn vector_divide() {
    let r = reference();
    let s = rand_scalar();
    let v = vref(&r)[0].div(s);
    for i in 0..SIZE {
        assert_eq!(r[i] / s, v[i]);
    }
}

#[test]
fn vector_multiply1() {
    let r = reference();
    let s = rand_scalar();
    let v = vref(&r)[0].scale(s);
    for i in 0..SIZE {
        assert_eq!(r[i] * s, v[i]);
    }
}

#[test]
fn vector_multiply2() {
    let r = reference();
    let s = rand_scalar();
    // r * vref: IEEE multiplication is commutative
    let v = vref(&r)[0].scale(s);
    for i in 0..SIZE {
        assert_eq!(s * r[i], v[i]);
    }
}

#[test]
fn vector_addition() {
    let r = reference();
    let v = vref(&r)[0].add(&vref(&r)[0]);
    for i in 0..SIZE {
        assert_eq!(r[i] * 2., v[i]);
    }
}

#[test]
fn vector_substract() {
    let r = reference();
    let v = vref(&r)[0].sub(&vref(&r)[0]);
    assert!(v.iter().all(|&x| x == 0.0));
}

#[test]
fn vector_divide_equal() {
    let r = reference();
    let s = rand_scalar();
    let mut v: Vector<SIZE> = vref(&r)[0];
    v = v.div(s);
    for i in 0..SIZE {
        assert_eq!(r[i] / s, v[i]);
    }
}

#[test]
fn vector_multiply_equal1() {
    let r = reference();
    let s = rand_scalar();
    let mut v: Vector<SIZE> = vref(&r)[0];
    v = v.scale(s);
    for i in 0..SIZE {
        assert_eq!(r[i] * s, v[i]);
    }
}

#[test]
fn vector_multiply_equal2() {
    let r = reference();
    let mut v: Vector<SIZE> = vref(&r)[0];
    v = v.cwise_mul(&v);
    for i in 0..SIZE {
        assert_eq!(r[i] * r[i], v[i]);
    }
}

#[test]
fn vector_addition_equal() {
    let r = reference();
    let mut v: Vector<SIZE> = vref(&r)[0];
    v = v.add(&vref(&r)[0]);
    for i in 0..SIZE {
        assert_eq!(r[i] * 2., v[i]);
    }
}

#[test]
fn vector_substract_equal() {
    let r = reference();
    let mut v: Vector<SIZE> = vref(&r)[0];
    v = v.sub(&vref(&r)[0]);
    assert!(v.iter().all(|&x| x == 0.0));
}

// --- Matrix -------------------------------------------------------------------

#[test]
fn matrix_default_construct() {
    let m = MatrixX::default();
    assert_eq!(0, m.as_slice().len());
    assert_eq!(0, m.rows());
    assert_eq!(0, m.cols());
}

#[test]
fn matrix_construct1() {
    let m = MatrixX::zeros(ROWS, COLS);
    assert_eq!(SIZE, m.as_slice().len());
    assert_eq!(ROWS, m.rows());
    assert_eq!(COLS, m.cols());
}

#[test]
fn matrix_construct_wrapped() {
    let r = reference();
    let m = as_matrix::<ROWS, COLS>(&r).unwrap();
    assert_eq!(SIZE, m.as_flat().len());
    assert_eq!((ROWS, COLS), (m[0].len(), m.len()));
    for i in 0..ROWS {
        for j in 0..COLS {
            assert_eq!(r[i + j * ROWS], m.at(i, j));
        }
    }
    compare_to_ref(&r, m.as_flat());
}

#[test]
fn matrix_construct_initializer() {
    // upstream `Matrix<double> m{{0., 1., 2.}, {3., 4., 5.}}` lists ROWS
    let m = Matrix::<2, 3>::from_rows([[0., 1., 2.], [3., 4., 5.]]);
    assert_eq!(6, m.as_flat().len());
    assert_eq!((2, 3), (m[0].len(), m.len()));
    let mut c = 0.0;
    for i in 0..2 {
        for j in 0..3 {
            assert_eq!(c, m.at(i, j));
            c += 1.0;
        }
    }
}

#[test]
fn matrix_construct_copy1() {
    let r = reference();
    let m: Matrix<ROWS, COLS> = *mref(&r);
    assert_eq!(SIZE, m.as_flat().len());
    compare_to_ref(&r, m.as_flat());
    let d = MatrixX::from_fixed(mref(&r));
    assert_eq!((ROWS, COLS), (d.rows(), d.cols()));
    compare_to_ref(&r, d.as_slice());
}

#[test]
fn matrix_construct_proxy1() {
    let r = reference();
    let p = mref(&r);
    assert_eq!(SIZE, p.as_flat().len());
    assert_eq!(ROWS, p[0].len());
    assert_eq!(COLS, p.len());
    compare_to_ref(&r, p.as_flat());
    let m = MatrixX::from_col_major(ROWS, COLS, p.as_flat().to_vec()).unwrap();
    assert_eq!((ROWS, COLS), (m.rows(), m.cols()));
    compare_to_ref(&r, m.as_slice());
}

#[test]
fn matrix_equal() {
    let r = reference();
    let m: Matrix<ROWS, COLS> = *mref(&r);
    compare_to_ref(&r, m.as_flat());
}

#[test]
fn matrix_equal_proxy1() {
    let r = reference();
    let proxy = as_matrix::<ROWS, COLS>(mref(&r).as_flat()).unwrap();
    let m = MatrixX::from_fixed(proxy);
    compare_to_ref(&r, m.as_slice());
    assert_eq!((ROWS, COLS), (m.rows(), m.cols()));
}

#[test]
fn matrix_equal_proxy2() {
    let r = reference();
    let mut store = MatrixX::zeros(ROWS, COLS);
    {
        let m = as_matrix_mut::<ROWS, COLS>(store.as_mut_slice()).unwrap();
        *m = *mref(&r);
        compare_to_ref(&r, m.as_flat());
    }
    compare_to_ref(&r, store.as_slice());
}

#[test]
fn matrix_equal_slice() {
    let r = reference();
    // fixed size: column assignment `m(i) = mref(i)`
    let mut m: Matrix<ROWS, COLS> = Matrix::zeros();
    for (col, src) in m.iter_mut().zip(mref(&r)) {
        *col = *src;
    }
    compare_to_ref(&r, m.as_flat());
    // dynamic size
    let src = MatrixX::from_fixed(mref(&r));
    let mut d = MatrixX::zeros(ROWS, COLS);
    for i in 0..d.cols() {
        d.col_mut(i).unwrap().copy_from_slice(src.col(i).unwrap());
    }
    compare_to_ref(&r, d.as_slice());
    assert!(d.col(COLS).is_none());
}

#[test]
fn matrix_set() {
    let r = reference();
    let mut m = MatrixX::from_fixed(mref(&r));
    compare_to_ref(&r, m.as_slice());
    let s = rand_scalar();
    m.fill(s);
    assert!(m.as_slice().iter().all(|&x| x == s));
}

#[test]
fn matrix_clear() {
    let r = reference();
    let mut m = MatrixX::from_fixed(mref(&r));
    compare_to_ref(&r, m.as_slice());
    m.fill(0.0);
    assert!(m.as_slice().iter().all(|&x| x == 0.0));
}

#[test]
fn matrix_divide() {
    let r = reference();
    let s = rand_scalar();
    let m = mref(&r).div(s);
    let d = MatrixX::from_fixed(mref(&r)).div(s);
    for i in 0..SIZE {
        assert_eq!(r[i] / s, m.as_flat()[i]);
        assert_eq!(r[i] / s, d.as_slice()[i]);
    }
}

#[test]
fn matrix_multiply1() {
    let r = reference();
    let s = rand_scalar();
    let m = mref(&r).scale(s);
    let d = MatrixX::from_fixed(mref(&r)).scale(s);
    for i in 0..SIZE {
        assert_eq!(r[i] * s, m.as_flat()[i]);
        assert_eq!(r[i] * s, d.as_slice()[i]);
    }
}

#[test]
fn matrix_multiply2() {
    let r = reference();
    let s = rand_scalar();
    let m = mref(&r).scale(s);
    for i in 0..SIZE {
        assert_eq!(s * r[i], m.as_flat()[i]);
    }
}

#[test]
fn matrix_addition() {
    let r = reference();
    let m = mref(&r).add(mref(&r));
    let d0 = MatrixX::from_fixed(mref(&r));
    let d = d0.add(&d0).unwrap();
    for i in 0..SIZE {
        assert_eq!(r[i] * 2., m.as_flat()[i]);
        assert_eq!(r[i] * 2., d.as_slice()[i]);
    }
    assert!(d0.add(&MatrixX::zeros(COLS, ROWS)).is_err());
}

#[test]
fn matrix_substract() {
    let r = reference();
    let m = mref(&r).sub(mref(&r));
    let d0 = MatrixX::from_fixed(mref(&r));
    let d = d0.sub(&d0).unwrap();
    assert!(m.as_flat().iter().chain(d.as_slice()).all(|&x| x == 0.0));
}

#[test]
fn matrix_divide_equal() {
    let r = reference();
    let s = rand_scalar();
    let mut m: Matrix<ROWS, COLS> = *mref(&r);
    m = m.div(s);
    for i in 0..SIZE {
        assert_eq!(r[i] / s, m.as_flat()[i]);
    }
}

#[test]
fn matrix_multiply_equal1() {
    let r = reference();
    let s = rand_scalar();
    let mut m: Matrix<ROWS, COLS> = *mref(&r);
    m = m.scale(s);
    for i in 0..SIZE {
        assert_eq!(r[i] * s, m.as_flat()[i]);
    }
}

#[test]
fn matrix_addition_equal() {
    let r = reference();
    let mut m: Matrix<ROWS, COLS> = *mref(&r);
    m = m.add(mref(&r));
    for i in 0..SIZE {
        assert_eq!(r[i] * 2., m.as_flat()[i]);
    }
}

#[test]
fn matrix_substract_equal() {
    let r = reference();
    let mut m: Matrix<ROWS, COLS> = *mref(&r);
    m = m.sub(mref(&r));
    assert!(m.as_flat().iter().all(|&x| x == 0.0));
}

/// Range-for over a matrix iterates its **columns** (Akantu's plugin).
#[test]
fn matrix_iterator() {
    let r = reference();
    let m: Matrix<ROWS, COLS> = *mref(&r);
    let mut col_count = 0;
    for col in &m {
        let col_hand = &m.as_flat()[col_count * ROWS..(col_count + 1) * ROWS];
        assert_eq!(&col[..], col_hand);
        col_count += 1;
    }
    assert_eq!(COLS, col_count);
    let d = MatrixX::from_fixed(&m);
    let mut n = 0;
    for (k, col) in d.columns().enumerate() {
        assert_eq!(col, &d.as_slice()[k * ROWS..(k + 1) * ROWS]);
        n += 1;
    }
    assert_eq!(COLS, n);
}

#[test]
fn matrix_iterator_zip() {
    let r = reference();
    let m1: Matrix<ROWS, COLS> = *mref(&r);
    let m2: Matrix<ROWS, COLS> = *mref(&r);
    for (c1, c2) in m1.iter().zip(&m2) {
        let comp = c1.sub(c2).iter().fold(0.0_f64, |a, x| a.nan_max(x.abs()));
        assert_eq!(0., comp);
    }
}

/// `zip(m1.colwise(), m2.colwise())`.
#[test]
fn vector_vise_iterator_zip() {
    let r = reference();
    let m1 = MatrixX::from_fixed(mref(&r));
    let m2 = MatrixX::from_fixed(mref(&r));
    for (c1, c2) in m1.columns().zip(m2.columns()) {
        let comp = c1
            .iter()
            .zip(c2)
            .fold(0.0_f64, |a, (x, y)| a.nan_max((x - y).abs()));
        assert_eq!(0., comp);
    }
}

/// `zip(m1.col(0), m2.col(0))`: entries of one column.
#[test]
fn vector_iterator_zip() {
    let r = reference();
    let m1: Matrix<ROWS, COLS> = *mref(&r);
    let m2: Matrix<ROWS, COLS> = *mref(&r);
    for (a, b) in m1[0].iter().zip(&m2[0]) {
        assert_eq!(0., a - b);
    }
}
