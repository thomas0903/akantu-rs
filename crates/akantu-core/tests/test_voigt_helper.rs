// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_common/test_voigt_helper.cc
// (VoigtHelperFixture, fixture values copied from the upstream test).

//! `VoigtHelperFixture.{Size, Indicies, Factors, MatrixToVoight,
//! MatrixToVoightFactors, VoightToMatrix}` for dimensions 1, 2 and 3
//! (TEST-005, CORE-021): exact comparisons, as upstream (`EXPECT_EQ`).
//! Upstream's `Matrix{{a, b}, {c, d}}` initialiser is row-wise.

use akantu_core::tensor::{Matrix, MatrixExt};
use akantu_core::voigt::{
    matrix_to_voigt, matrix_to_voigt_with_factors, voigt_size, voigt_table, voigt_to_matrix,
};

struct Fixture<const D: usize, const S: usize> {
    indices: [(usize, usize); S],
    matrix: Matrix<D, D>,
    vector: [f64; S],
    vector_factor: [f64; S],
}

fn fixture_1d() -> Fixture<1, 1> {
    Fixture {
        indices: [(0, 0)],
        matrix: Matrix::from_rows([[10.]]),
        vector: [10.],
        vector_factor: [10.],
    }
}

fn fixture_2d() -> Fixture<2, 3> {
    Fixture {
        indices: [(0, 0), (1, 1), (0, 1)],
        matrix: Matrix::from_rows([[10., 33.], [0., 56.]]),
        vector: [10., 56., 33.],
        vector_factor: [10., 56., 2. * 33.],
    }
}

fn fixture_3d() -> Fixture<3, 6> {
    Fixture {
        indices: [(0, 0), (1, 1), (2, 2), (1, 2), (0, 2), (0, 1)],
        matrix: Matrix::from_rows([[10., 33., 20.], [0., 56., 27.], [0., 0., 98.]]),
        vector: [10., 56., 98., 27., 20., 33.],
        vector_factor: [10., 56., 98., 2. * 27., 2. * 20., 2. * 33.],
    }
}

fn run<const D: usize, const S: usize>(f: &Fixture<D, S>) {
    let t = voigt_table(D).unwrap();
    // Size
    assert_eq!(voigt_size(D), [1, 3, 6][D - 1]);
    assert_eq!(t.size, S);
    // Indicies
    for (big_i, &(i, j)) in f.indices.iter().enumerate() {
        assert_eq!(t.vec(big_i), (i, j), "dim {D}, I = {big_i}");
    }
    // Factors
    for big_i in 0..S {
        assert_eq!(t.factor(big_i), if big_i < D { 1. } else { 2. });
    }
    // MatrixToVoight
    assert_eq!(matrix_to_voigt::<D, S>(&f.matrix), f.vector);
    // MatrixToVoightFactors
    assert_eq!(
        matrix_to_voigt_with_factors::<D, S>(&f.matrix),
        f.vector_factor
    );
    // VoightToMatrix
    let m = voigt_to_matrix::<D, S>(&f.vector);
    for i in 0..D {
        for j in 0..D {
            assert_eq!(
                m.at(i, j),
                f.matrix.at(i.min(j), i.max(j)),
                "dim {D}, ({i}, {j})"
            );
        }
    }
}

#[test]
fn voigt_helper_fixture_1d() {
    run(&fixture_1d());
}

#[test]
fn voigt_helper_fixture_2d() {
    run(&fixture_2d());
}

#[test]
fn voigt_helper_fixture_3d() {
    run(&fixture_3d());
}
