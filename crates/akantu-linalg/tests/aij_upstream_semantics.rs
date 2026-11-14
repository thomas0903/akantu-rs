// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): expected values hand-derived from
// src/solver/sparse_matrix_aij.hh:153-158 (key), sparse_matrix_aij_inline_impl.hh
// :30-58 (add(i, j): first-inserted coordinates), :113-128
// (addSymmetricValuesToSymmetric), :131-147 (addUnsymmetricValuesToSymmetric),
// :150-164 (addValuesToUnsymmetric), :183-196 (addValues dispatch) and
// src/model/common/dof_manager/dof_manager_default.cc:295-388 (addToProfile:
// full local i, j loop) / dof_manager_inline_impl.hh:233-305 (elemental
// assembly through local→global equation numbers).

//! Contract tests: COO profile and value
//! semantics of `SparseMatrixAij` reproduce upstream's arrays exactly.
//!
//! Elemental matrices below are written row-wise in comments and passed
//! column-major (`values[i + j * n]`), as upstream `Matrix<Real>`.

use akantu_core::GlobalIdx;
use akantu_core::enums::MatrixType;
use akantu_linalg::{
    EquationLayout, EquationOwnership, NoReduction, SparseMatrixAij, SymmetryPolicy,
};

fn one_based(m: &SparseMatrixAij) -> (Vec<i32>, Vec<i32>) {
    m.coo_one_based_i32(u64::MAX).unwrap()
}

/// Assemble like `DOFManagerDefault::assembleElementalMatricesToMatrix`:
/// profile the element's equations first (full local loop), then add values.
fn assemble(m: &mut SparseMatrixAij, eqs: &[GlobalIdx], col_major: &[f64], ty: MatrixType) {
    m.add_block_to_profile(eqs, eqs).unwrap();
    m.add_values(eqs, eqs, col_major, ty).unwrap();
}

/// Non-monotone equation map, symmetric matrix, symmetric elemental blocks.
/// Element A has equations [2, 0], element B [0, 1].
///
/// Profile (addToProfile, rows i then cols j):
/// A: (2,2) new k0 · (2,0) key (0,2) new k1 stored **(2,0)** · (0,2) exists ·
///    (0,0) new k2;
/// B: (0,0) exists · (0,1) new k3 stored **(0,1)** · (1,0) exists · (1,1) k4.
/// So a symmetric matrix holds coordinates from **both** triangles.
///
/// Values (local upper j >= i): A = [[1,2],[2,3]] → (2,2)+=1, key(0,2)+=2,
/// (0,0)+=3; B = [[10,20],[20,30]] → (0,0)+=10, (0,1)+=20, (1,1)+=30.
#[test]
fn symmetric_profile_keeps_first_inserted_coordinates() {
    let mut k = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
    assemble(&mut k, &[2, 0], &[1., 2., 2., 3.], MatrixType::Symmetric);
    assemble(
        &mut k,
        &[0, 1],
        &[10., 20., 20., 30.],
        MatrixType::Symmetric,
    );
    let (irn, jcn) = one_based(&k);
    assert_eq!(irn, [3, 3, 1, 1, 2]);
    assert_eq!(jcn, [3, 1, 1, 2, 2]);
    assert_eq!(k.coo().2, [1., 2., 13., 20., 30.]);
    // Lookup is canonical: (0,2) and (2,0) are the same entry.
    assert_eq!((k.get(0, 2), k.get(2, 0)), (2., 2.));
}

/// UD-034 compatibility mode: an unsymmetric block added to a symmetric
/// matrix keeps only entries with global column >= global row.
/// Element eqs [2, 0], block [[1,2],[3,4]]:
/// i=0 (row eq 2): j=0 col 2 >= 2 → (2,2)+=1; j=1 col 0 < 2 → dropped (2).
/// i=1 (row eq 0): j=0 col 2 → key(0,2)+=3; j=1 col 0 → (0,0)+=4.
/// With the reversed equation order [0, 2] the **other** off-diagonal value
/// survives — the stored operator depends on the equation permutation.
#[test]
fn upstream_upper_selection_drops_global_lower_half() {
    let mut k = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
    k.set_symmetry_policy(SymmetryPolicy::UpstreamUpperSelection);
    assemble(&mut k, &[2, 0], &[1., 3., 2., 4.], MatrixType::Unsymmetric);
    let (irn, jcn) = one_based(&k);
    assert_eq!((irn, jcn), (vec![3, 3, 1], vec![3, 1, 1]));
    assert_eq!(k.coo().2, [1., 3., 4.]);
    assert_eq!(k.matrix_type(), MatrixType::Symmetric);

    let mut k2 = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
    k2.set_symmetry_policy(SymmetryPolicy::UpstreamUpperSelection);
    // Same element, local order swapped: eqs [0, 2], block [[4,3],[2,1]].
    assemble(&mut k2, &[0, 2], &[4., 2., 3., 1.], MatrixType::Unsymmetric);
    assert_eq!(k2.get(0, 2), 3.0, "global (0,2) = value(0,1) = 3 here too");
    // ...but a local-upper rule would have chosen differently:
    let mut k3 = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
    assemble(&mut k3, &[2, 0], &[1., 3., 2., 4.], MatrixType::Symmetric);
    assert_eq!(k3.get(0, 2), 2.0, "local upper takes value(0,1) = 2");
}

/// Default policy (UD-034 fix): the matrix is promoted to unsymmetric
/// storage; mirror entries are appended in storage order, both releases
/// bump, and the full block is kept.
#[test]
fn promotion_keeps_the_full_unsymmetric_block() {
    let mut k = SparseMatrixAij::new("K", 3, MatrixType::Symmetric);
    assert_eq!(k.symmetry_policy(), SymmetryPolicy::PromoteOnUnsymmetric);
    k.add_block_to_profile(&[2, 0], &[2, 0]).unwrap();
    let release = k.profile_release();
    k.add_values(&[2, 0], &[2, 0], &[1., 3., 2., 4.], MatrixType::Unsymmetric)
        .unwrap();
    assert!(k.was_promoted());
    assert_eq!(k.matrix_type(), MatrixType::Unsymmetric);
    assert!(k.profile_release() > release, "backends must re-analyse");
    let (irn, jcn) = one_based(&k);
    // k0 (2,2), k1 (2,0), k2 (0,0) from the profile; k3 = mirror (0,2).
    assert_eq!((irn, jcn), (vec![3, 3, 1, 1], vec![3, 1, 1, 3]));
    // K[2][0] = value(0,1) = 2, K[0][2] = value(1,0) = 3.
    assert_eq!(k.coo().2, [1., 2., 4., 3.]);
    assert_eq!(akantu_linalg::direct::mumps_symmetry(&k), 0);
}

/// Symmetric blocks into an unsymmetric matrix use every entry
/// (`addValues` dispatches on the matrix type only).
#[test]
fn symmetric_block_into_unsymmetric_matrix_adds_all_entries() {
    let mut k = SparseMatrixAij::new("K", 2, MatrixType::Unsymmetric);
    assemble(&mut k, &[1, 0], &[1., 2., 2., 3.], MatrixType::Symmetric);
    let (irn, jcn) = one_based(&k);
    assert_eq!((irn, jcn), (vec![2, 2, 1, 1], vec![2, 1, 2, 1]));
    assert_eq!(k.coo().2, [1., 2., 2., 3.]);
    assert_eq!(akantu_linalg::direct::mumps_symmetry(&k), 0);
}

/// Two distinct DOF families: rows of family u (eqs 0, 1), columns of family
/// p (eq 5, numbered after u). Rectangular cross-family blocks are always
/// "unsymmetric"; into a symmetric matrix in compat mode only the global
/// upper entries survive, under the default policy the matrix is promoted.
#[test]
fn rectangular_cross_family_blocks() {
    let mut m = SparseMatrixAij::new("J", 6, MatrixType::Unsymmetric);
    m.add_block_to_profile(&[0, 1], &[5]).unwrap();
    m.add_values(&[0, 1], &[5], &[7., 8.], MatrixType::Unsymmetric)
        .unwrap();
    assert_eq!(one_based(&m), (vec![1, 2], vec![6, 6]));
    assert_eq!(m.coo().2, [7., 8.]);

    let mut s = SparseMatrixAij::new("J", 6, MatrixType::Symmetric);
    s.set_symmetry_policy(SymmetryPolicy::UpstreamUpperSelection);
    s.add_block_to_profile(&[5], &[0, 1]).unwrap();
    // rows eq 5, cols 0/1: column < row → both dropped in compat mode.
    s.add_values(&[5], &[0, 1], &[7., 8.], MatrixType::Unsymmetric)
        .unwrap();
    assert_eq!(s.coo().2, [0., 0.]);
    assert_eq!(one_based(&s), (vec![6, 6], vec![1, 2]));
}

/// Missing profile entries are an error and leave the matrix unchanged
/// (upstream: debug assertion / undefined behaviour in release).
#[test]
fn add_values_outside_profile_is_atomic_error() {
    let mut k = SparseMatrixAij::new("K", 3, MatrixType::Unsymmetric);
    k.add(0, 0, 1.0).unwrap();
    let v = k.value_release();
    assert!(
        k.add_values(&[0, 1], &[0, 1], &[1., 1., 1., 1.], MatrixType::Unsymmetric)
            .is_err()
    );
    assert_eq!(k.coo().2, [1.0]);
    assert_eq!(k.value_release(), v);
    // Equations >= size are skipped as upstream (`c_irn < size_`).
    k.add_values(&[0, 7], &[0, 7], &[1., 9., 9., 9.], MatrixType::Unsymmetric)
        .unwrap();
    assert_eq!(k.get(0, 0), 2.0);
}

/// Global coordinates beyond 32 bits are representable without any
/// global-size allocation; narrowing for a 32-bit backend is rejected before
/// conversion; products go through the explicit local ↔ global layout.
#[test]
fn global_offset_above_u32_range() {
    let base: GlobalIdx = 5_000_000_000;
    // This rank owns global equations [base, base + 2) of a system of
    // base + 2 equations (all lower ranks own `base`).
    let mut layout = EquationLayout::new();
    layout
        .append_block(&[EquationOwnership::Owned; 2], base, 5_000_000_002, |_| {
            None
        })
        .unwrap();
    assert_eq!(layout.system_size(), base + 2);
    let mut k = SparseMatrixAij::new("K", layout.system_size(), MatrixType::Symmetric);
    let eqs = [base + 1, base];
    assemble(&mut k, &eqs, &[2., -1., -1., 2.], MatrixType::Symmetric);
    assert_eq!(k.coo().0, [base + 1, base + 1, base]);
    assert!(k.coo_one_based_i32(u64::MAX).is_err());
    let mut y = [0.0; 2];
    k.mat_vec_mul(&layout, &NoReduction, &[1.0, 1.0], &mut y, 1.0, 0.0)
        .unwrap();
    assert_eq!(y, [1.0, 1.0]);
    // An nnz limit is enforced too (MUMPS `nnz` is 64-bit, others may not be).
    let small = SparseMatrixAij::new("M", 2, MatrixType::Unsymmetric);
    assert!(small.coo_one_based_i32(0).is_ok());
    assert!(k.coo_one_based_i32(2).is_err());
}
