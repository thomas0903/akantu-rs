// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_types.hh (MatrixBase::eig,
// MatrixBase::eigh overloads built on Eigen::EigenSolver /
// Eigen::SelfAdjointEigenSolver), aka_types_eigen_matrix_base_plugin.hh
// (default arguments).

//! Dense eigen-decompositions with upstream's exact overload contracts
//! and bit-level parity with Eigen 3.4.0.
//!
//! # Contracts (upstream `src/common/aka_types.hh:440-575`)
//!
//! | Upstream call | Port | Order / semantics |
//! |---|---|---|
//! | `A.eig<double>()`, `A.eig(values)` (real `values`) | [`eig`] | real parts of the eigenvalues in **`EigenSolver` order** (Schur order, *unsorted*) |
//! | `A.eig<complex>()`, `A.eig(values)` (complex `values`) | [`eig_complex`] | complex eigenvalues, `EigenSolver` order |
//! | `A.eig(values, vectors)` / `(…, true)` (real outputs; `sort` defaults to `true`) | [`eig_vectors`] with [`SortOrder::Descending`] | error if `max |Im v| > 1e-15·‖A‖_F`; values sorted **descending** (libstdc++ `std::sort` with `(a − b) > 0`), eigenvector columns permuted with them |
//! | `A.eig(values, vectors, false)` (real outputs) | [`eig_vectors`] with [`SortOrder::Natural`] | same complex check, `EigenSolver` order |
//! | `A.eig(values, vectors, false)` (complex outputs) | [`eig_vectors_complex`] | `EigenSolver` order; (`sort = true` with complex outputs throws upstream — not offered) |
//! | `A.eigh(values)` | [`eigh`] | **natural** order of `SelfAdjointEigenSolver` (ascending); only the lower triangle is read |
//! | `A.eigh(values, vectors)` / `(…, true)` (`sort` defaults to `true`) | [`eigh_vectors`] with [`SortOrder::Descending`] | descending, columns permuted |
//! | `A.eigh(values, vectors, false)` | [`eigh_vectors`] with [`SortOrder::Natural`] | ascending |
//!
//! Eigenvectors are Eigen's: unit 2-norm columns with Eigen's sign and, for
//! repeated eigenvalues, Eigen's basis of the eigenspace (they are what the
//! algorithm produces, not a canonical choice).
//!
//! `storage` selects Eigen's compile-time-size (`Matrix<Real, n, n>`) or
//! run-time-size (`Matrix<Real>`, dynamic `Map`) evaluation; match the
//! upstream call site (e.g. Mazars' `Matrix<Real, 3, 3>` strain → `Fixed`,
//! `MaterialElasticLinearAnisotropic::C` (`Matrix<Real>`) → `Dynamic`).
//!
//! # Deviations from upstream (failure handling)
//!
//! Upstream ignores `ComputationInfo`; on `NoConvergence` / `NumericalIssue`
//! (non-finite eigenvalues) it returns stale or uninitialised values. The port
//! returns [`Error::Numerical`] instead (no silent garbage). Upstream's
//! "This matrix has complex eigenvectors()" exception maps to
//! [`Error::Numerical`] with the same message.
//!
//! # Celerity and UD-033
//!
//! `MaterialElasticLinearAnisotropic` uses `eigC(0)` of the *unsorted*
//! values as wave-speed modulus (UD-033). Both behaviours are expressible:
//! compatibility mode = `eig(C, Dynamic)?[0]`; corrected maximum =
//! `eigh(C, Dynamic)?.last()` (or the maximum of `eig`). The choice belongs to
//! the materials layer.
//!
//! # Parity evidence
//!
//! `tests/dense_parity.rs` asserts bit equality against fixtures produced by
//! the C++ probe `oracle/probes/dense/` (real Eigen 3.4.0, GCC 14.2.0,
//! aarch64, `-O2 -std=c++20 -ffp-contract=off -DEIGEN_DONT_VECTORIZE`).
//!
//! **Bitwise scope (D-065 and its addendum):** sizes
//! `n ≤ 48` (above, Eigen accumulates the Householder reflectors blocked);
//! with `sort = true` ([`SortOrder::Descending`]) only when `n ≤ 16` or there
//! are no ties among the **returned** eigenvalues (`+0` and `−0` count as a
//! tie; distinct input entries one ulp apart can come back tied, since both
//! solvers scale the matrix): above 16 values
//! libstdc++ sorts by introsort, and once `n ≥ 17` a single tied pair can come
//! out in another order than this port's insertion sort (32 of 48 tied
//! oracle cases at `n ≥ 17` differ in the eigenvector column order; every
//! tie-free case and every case at `n = 16` matches). Outside that scope the
//! result is a correct eigen-decomposition under the criteria of
//! `docs/NUMERICAL_VALIDATION.md` (same values, an eigenbasis of each
//! tied eigenvalue's subspace), not bitwise to Eigen. Evidence:
//! `tests/dense_parity.rs` (aarch64 oracle records of sizes 1–9, 12 and, in
//! `fixtures/dense/dense_eig_large_strict.txt`, x86_64 records of sizes
//! 13–48) and `tests/dense_sort_ties_oracle.rs` (72 tie cases). Upstream
//! calls `eig` on matrices of size ≤ 6 only.
//!
//! **Complex eigenvectors and the platform (D-043).** Eigen's eigenvector
//! back-substitution for complex-conjugate pairs divides `std::complex`
//! numbers, and the oracle runtimes divide differently: the aarch64 one with
//! fused multiply-adds (the default here, whose fixtures are authoritative
//! for merges until the x86_64 corpus exists), the x86_64 one without
//! ([`ComplexDivision`]). [`eig_vectors_complex_with`] selects the runtime;
//! the x86_64 records above replay bit for bit with
//! [`ComplexDivision::Unfused`]. Eigenvalues, real eigenvectors and `eigh`
//! do not depend on it.

mod complex;
mod eigen;

pub use complex::{Complex, ComplexDivision};
pub use eigen::Storage;

use akantu_core::eigen_order as eo;
use akantu_core::tensor::MatrixX;
use akantu_core::{Error, Real, Result};
use eigen::Mat;
use eigen::fixed::{FMat, real_schur_fixed, schur_eigenvalues_fixed};
use eigen::real_schur::{EigenSolver, Info, do_compute_eigenvectors, eigenvectors_into};
use eigen::self_adjoint::self_adjoint_eigen;

/// Ordering of eigenpairs returned with eigenvectors (upstream `sort`
/// argument).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SortOrder {
    /// `sort = false`: the solver's natural order (`EigenSolver`: Schur
    /// order; `SelfAdjointEigenSolver`: ascending).
    Natural,
    /// `sort = true` (upstream default for real outputs): descending,
    /// eigenvectors permuted with their values.
    Descending,
}

fn to_mat(a: &MatrixX, context: &'static str) -> Result<Mat> {
    if a.rows() != a.cols() {
        return Err(Error::DimensionMismatch {
            context,
            expected: a.rows(),
            got: a.cols(),
        });
    }
    Ok(Mat {
        n: a.rows(),
        d: eigen::Buf::from_slice(a.as_slice()),
    })
}

fn check_info(info: Info, what: &str) -> Result<()> {
    match info {
        Info::Success => Ok(()),
        Info::NoConvergence => Err(Error::Numerical(format!(
            "{what}: no convergence (Eigen NoConvergence; upstream would return uninitialised values)"
        ))),
        Info::NumericalIssue => Err(Error::Numerical(format!(
            "{what}: non-finite eigenvalue (Eigen NumericalIssue)"
        ))),
    }
}

/// Upstream `A.eig()` / `A.eig(values)` with complex output.
pub fn eig_complex(a: &MatrixX, storage: Storage) -> Result<Vec<Complex>> {
    let m = to_mat(a, "eig")?;
    let s = EigenSolver::compute(&m, false, storage, ComplexDivision::Fused);
    check_info(s.info, "eig")?;
    Ok(s.values)
}

/// Upstream `A.eig(values)` with real output: real parts, unsorted (Eigen
/// order).
pub fn eig(a: &MatrixX, storage: Storage) -> Result<Vec<Real>> {
    Ok(eig_complex(a, storage)?.into_iter().map(|z| z.re).collect())
}

/// [`eig`] of a compile-time `3 × 3` matrix (`Storage::Fixed`) given
/// column-major (`a[col][row]`), without heap allocation
/// (Mazars calls it per quadrature point). Same arithmetic as [`eig`] (the
/// same Eigen-order kernels on inline buffers), hence bit-identical results.
///
/// # Errors
/// As [`eig`]: no convergence or a non-finite eigenvalue.
pub fn eig3_fixed(a: &[[Real; 3]; 3]) -> Result<[Real; 3]> {
    eig_fixed(a)
}

/// [`eig`] of a compile-time `N × N` matrix (`Storage::Fixed`, `1 ≤ N ≤ 4`)
/// given column-major (`a[col][row]`), without heap allocation
/// (anisotropic damage calls it several times per quadrature point).
/// Same arithmetic as [`eig`] (the same Eigen-order kernels on inline
/// buffers), hence bit-identical results.
///
/// # Errors
/// As [`eig`]: no convergence or a non-finite eigenvalue.
pub fn eig_fixed<const N: usize>(a: &[[Real; N]; N]) -> Result<[Real; N]> {
    const { assert!(N >= 1 && N <= 4, "eig_fixed: 1 <= N <= 4 (inline buffers)") };
    // the general path's operations on stack arrays (`eigen::fixed`)
    let (t, _, info) = real_schur_fixed::<N, false>(&FMat(*a));
    check_info(info, "eig")?;
    let mut values = [Complex::default(); N];
    check_info(schur_eigenvalues_fixed(&t, &mut values), "eig")?;
    Ok(values.map(|z| z.re))
}

/// [`eig_vectors`] of a compile-time `N × N` matrix (`Storage::Fixed`,
/// `1 ≤ N ≤ 4`) given column-major (`a[col][row]`), without heap allocation:
/// eigenvalues and real eigenvector columns
/// (`vectors[col][row]`), upstream's complex-eigenvector check, the order of
/// `order`. Same arithmetic as [`eig_vectors`] with [`Storage::Fixed`],
/// hence bit-identical results.
///
/// # Errors
/// As [`eig_vectors`]: no convergence, a non-finite eigenvalue, or complex
/// eigenvectors (upstream's "This matrix has complex eigenvectors()").
pub fn eig_vectors_fixed<const N: usize>(
    a: &[[Real; N]; N],
    order: SortOrder,
) -> Result<([Real; N], [[Real; N]; N])> {
    const {
        assert!(
            N >= 1 && N <= 4,
            "eig_vectors_fixed: 1 <= N <= 4 (inline buffers)"
        )
    };
    // EigenSolver::compute(matrix, true): the Schur form on stack arrays
    // (`eigen::fixed`), the eigenvectors by the general kernels
    let (tf, uf, info) = real_schur_fixed::<N, true>(&FMat(*a));
    check_info(info, "eig")?;
    let mut values = [Complex::default(); N];
    check_info(schur_eigenvalues_fixed(&tf, &mut values), "eig")?;
    let mut t = Mat {
        n: N,
        d: eigen::Buf::from_slice(tf.0.as_flattened()),
    };
    let mut eivec = Mat {
        n: N,
        d: eigen::Buf::from_slice(uf.0.as_flattened()),
    };
    do_compute_eigenvectors(
        &mut t,
        &values,
        &mut eivec,
        Storage::Fixed,
        ComplexDivision::Fused,
    );
    // eigenvectors(): normalised complex columns (column-major)
    let mut cols = [[Complex::default(); N]; N];
    eigenvectors_into(
        &eivec,
        &values,
        Storage::Fixed,
        ComplexDivision::Fused,
        cols.as_flattened_mut(),
    );
    let norm = slice_norm(a.as_flattened(), Storage::Fixed);
    if imag_max(cols.as_flattened()) > 1e-15 * norm {
        return Err(Error::Numerical(
            "This matrix has complex eigenvectors()".into(),
        ));
    }
    let re: [Real; N] = values.map(|z| z.re);
    let mut p = [0usize; N];
    match order {
        SortOrder::Natural => p.iter_mut().enumerate().for_each(|(i, x)| *x = i),
        SortOrder::Descending => descending_permutation_into(&re, &mut p),
    }
    let vals = std::array::from_fn(|i| re[p[i]]);
    let vecs = std::array::from_fn(|j| std::array::from_fn(|i| cols[p[j]][i].re));
    Ok((vals, vecs))
}

/// [`eigh_vectors`] of a compile-time `3 × 3` symmetric matrix
/// (`Storage::Fixed`; only the lower triangle is read) given column-major
/// (`a[col][row]`), without heap allocation: eigenvalues
/// and eigenvector columns (`vectors[col][row]`) in the order of `order`.
/// Same arithmetic as [`eigh_vectors`] with [`Storage::Fixed`], hence
/// bit-identical results.
///
/// # Errors
/// As [`eigh_vectors`]: no convergence.
pub fn eigh3_vectors_fixed(
    a: &[[Real; 3]; 3],
    order: SortOrder,
) -> Result<([Real; 3], [[Real; 3]; 3])> {
    let m = Mat {
        n: 3,
        d: eigen::Buf::from_slice(a.as_flattened()),
    };
    let s = self_adjoint_eigen(&m, true, Storage::Fixed);
    check_info(s.info, "eigh")?;
    let q = s
        .vectors
        .ok_or_else(|| Error::Numerical("eigh: eigenvectors not computed".into()))?;
    let values = [s.values[0], s.values[1], s.values[2]];
    let mut p = [0usize, 1, 2];
    if order == SortOrder::Descending {
        descending_permutation_into(&values, &mut p);
    }
    let vals = std::array::from_fn(|i| values[p[i]]);
    let vecs = std::array::from_fn(|j| std::array::from_fn(|i| q.at(i, p[j])));
    Ok((vals, vecs))
}

/// `solver.eigenvectors().imag().lpNorm<Infinity>()` over column-major
/// complex entries (first value, then strictly larger ones).
fn imag_max(cols: &[Complex]) -> Real {
    let mut imag_max = 0.0_f64;
    let mut first = true;
    for z in cols {
        let v = z.im.abs();
        if first {
            imag_max = v;
            first = false;
        } else if imag_max < v {
            imag_max = v;
        }
    }
    imag_max
}

/// `this->norm()` of the input (compile-time size: unrolled when
/// `3·n² − 1 ≤ 110`).
fn input_norm(a: &MatrixX, storage: Storage) -> Real {
    slice_norm(a.as_slice(), storage)
}

/// [`input_norm`] of column-major data.
fn slice_norm(d: &[Real], storage: Storage) -> Real {
    eo::sum(
        d.len(),
        eo::UNARY_TERM_COST,
        storage == Storage::Fixed,
        &|l| d[l] * d[l],
    )
    .sqrt()
}

/// libstdc++ `std::sort` on the index permutation with upstream's comparator
/// `(values(a) − values(b)) > 0`. For `n ≤ 16` libstdc++ performs exactly this
/// insertion sort (`__insertion_sort`, threshold 16); larger inputs use
/// introsort in libstdc++: without ties the sorted order is unique and equal,
/// with any tie (`±0` included) at `n ≥ 17` the tied columns may come out in
/// another order (D-065 addendum; black-box oracle evidence only, D-030).
fn descending_permutation(values: &[Real]) -> Vec<usize> {
    let mut p: Vec<usize> = vec![0; values.len()];
    descending_permutation_into(values, &mut p);
    p
}

/// [`descending_permutation`] into `p` (`p.len() == values.len()`).
fn descending_permutation_into(values: &[Real], p: &mut [usize]) {
    let comp = |a: usize, b: usize| (values[a] - values[b]) > 0.0;
    for (i, x) in p.iter_mut().enumerate() {
        *x = i;
    }
    for i in 1..p.len() {
        let val = p[i];
        if comp(val, p[0]) {
            p.copy_within(0..i, 1);
            p[0] = val;
        } else {
            let mut j = i;
            while comp(val, p[j - 1]) {
                p[j] = p[j - 1];
                j -= 1;
            }
            p[j] = val;
        }
    }
}

fn permuted(values: &[Real], vectors: &MatrixX, order: SortOrder) -> Result<(Vec<Real>, MatrixX)> {
    match order {
        SortOrder::Natural => Ok((values.to_vec(), vectors.clone())),
        SortOrder::Descending => {
            let p = descending_permutation(values);
            let n = values.len();
            let mut v = MatrixX::try_zeros(vectors.rows(), n)?;
            for (i, &pi) in p.iter().enumerate() {
                for r in 0..vectors.rows() {
                    *v.at_mut(r, i) = vectors.at(r, pi);
                }
            }
            Ok((p.iter().map(|&i| values[i]).collect(), v))
        }
    }
}

/// Upstream `A.eig(values, vectors, false)` with complex outputs:
/// eigenvalues and normalised complex eigenvector columns, Eigen order.
/// Complex divisions as the aarch64 oracle runtime does them
/// ([`ComplexDivision::Fused`]; [`eig_vectors_complex_with`]).
pub fn eig_vectors_complex(
    a: &MatrixX,
    storage: Storage,
) -> Result<(Vec<Complex>, Vec<Vec<Complex>>)> {
    eig_vectors_complex_with(a, storage, ComplexDivision::Fused)
}

/// [`eig_vectors_complex`] with the complex divisions of the eigenvector
/// computation (back-substitution of complex-conjugate pairs and the
/// normalisation) as the given oracle runtime does them (`std::complex`
/// division, [`Complex::quotient_with`]; D-043). The eigenvalues, and the
/// eigenvectors of real eigenvalues, do not depend on it; complex
/// eigenvectors differ in the last bits of many entries.
pub fn eig_vectors_complex_with(
    a: &MatrixX,
    storage: Storage,
    division: ComplexDivision,
) -> Result<(Vec<Complex>, Vec<Vec<Complex>>)> {
    let m = to_mat(a, "eig")?;
    let s = EigenSolver::compute(&m, true, storage, division);
    check_info(s.info, "eig")?;
    let vectors = s
        .eigenvectors()
        .ok_or_else(|| Error::Numerical("eig: eigenvectors not computed".into()))?;
    Ok((s.values, vectors))
}

/// Upstream `A.eig(values, vectors, sort)` with real outputs.
pub fn eig_vectors(
    a: &MatrixX,
    storage: Storage,
    order: SortOrder,
) -> Result<(Vec<Real>, MatrixX)> {
    let (values, cols) = eig_vectors_complex(a, storage)?;
    let n = values.len();
    let norm = input_norm(a, storage);
    // solver.eigenvectors().imag().lpNorm<Infinity>() (column-major max)
    let mut imag_max = 0.0_f64;
    let mut first = true;
    for col in &cols {
        for z in col {
            let v = z.im.abs();
            if first {
                imag_max = v;
                first = false;
            } else if imag_max < v {
                imag_max = v;
            }
        }
    }
    if imag_max > 1e-15 * norm {
        return Err(Error::Numerical(
            "This matrix has complex eigenvectors()".into(),
        ));
    }
    let re: Vec<Real> = values.iter().map(|z| z.re).collect();
    let mut v = MatrixX::try_zeros(n, n)?;
    for (j, col) in cols.iter().enumerate() {
        for (i, z) in col.iter().enumerate() {
            *v.at_mut(i, j) = z.re;
        }
    }
    permuted(&re, &v, order)
}

/// Upstream `A.eigh(values)`: eigenvalues of the symmetric matrix whose lower
/// triangle is `a`, in `SelfAdjointEigenSolver`'s natural (ascending) order.
pub fn eigh(a: &MatrixX, storage: Storage) -> Result<Vec<Real>> {
    let m = to_mat(a, "eigh")?;
    let s = self_adjoint_eigen(&m, false, storage);
    check_info(s.info, "eigh")?;
    Ok(s.values.to_vec())
}

/// Upstream `A.eigh(values, vectors, sort)`.
pub fn eigh_vectors(
    a: &MatrixX,
    storage: Storage,
    order: SortOrder,
) -> Result<(Vec<Real>, MatrixX)> {
    let m = to_mat(a, "eigh")?;
    let s = self_adjoint_eigen(&m, true, storage);
    check_info(s.info, "eigh")?;
    let q = s
        .vectors
        .ok_or_else(|| Error::Numerical("eigh: eigenvectors not computed".into()))?;
    let v = MatrixX::from_col_major(q.n, q.n, q.d.to_vec())?;
    permuted(&s.values, &v, order)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn mx(n: usize, rows: &[&[Real]]) -> MatrixX {
        let mut m = MatrixX::zeros(n, n);
        for (i, r) in rows.iter().enumerate() {
            for (j, &v) in r.iter().enumerate() {
                *m.at_mut(i, j) = v;
            }
        }
        m
    }

    #[test]
    fn upstream_test_tensors_matrix_eigs() {
        // test/test_common/test_tensors.cc MatrixEigs (4x4, sorted vectors)
        let a = mx(
            4,
            &[
                &[0., 1., 0., 0.],
                &[1., 0., 0., 0.],
                &[0., 1., 0., 1.],
                &[0., 0., 4., 0.],
            ],
        );
        let (l, v) = eig_vectors(&a, Storage::Fixed, SortOrder::Descending).unwrap();
        let refs = [2.0, 1.0, -1.0, -2.0];
        for (li, ri) in l.iter().zip(refs) {
            assert!((li - ri).abs() < 1e-14, "{l:?}");
        }
        let av = a.matmul(&v).unwrap();
        for (i, li) in l.iter().enumerate() {
            for r in 0..4 {
                assert!((li * v.at(r, i) - av.at(r, i)).abs() < 1e-14);
            }
        }
    }

    #[test]
    fn values_only_are_unsorted_and_match_vectors_overload() {
        let a = mx(3, &[&[1., 2., 0.], &[2., 5., 1.], &[0., 1., 3.]]);
        for st in [Storage::Fixed, Storage::Dynamic] {
            let vals = eig(&a, st).unwrap();
            let (nat, _) = eig_vectors(&a, st, SortOrder::Natural).unwrap();
            assert_eq!(vals, nat);
            let (desc, _) = eig_vectors(&a, st, SortOrder::Descending).unwrap();
            let mut sorted = vals.clone();
            sorted.sort_by(|x, y| y.total_cmp(x));
            assert_eq!(desc, sorted);
            let h = eigh(&a, st).unwrap();
            assert!(h.windows(2).all(|w| w[0] <= w[1]));
            for (x, y) in h.iter().zip(sorted.iter().rev()) {
                assert!((x - y).abs() < 1e-13);
            }
        }
    }

    #[test]
    fn complex_eigenvectors_are_rejected_for_real_output() {
        // rotation: eigenvalues ±i
        let a = mx(2, &[&[0., -1.], &[1., 0.]]);
        let z = eig_complex(&a, Storage::Fixed).unwrap();
        assert_eq!(z[0].im.abs(), 1.0);
        assert_eq!(eig(&a, Storage::Fixed).unwrap(), vec![0.0, 0.0]);
        assert!(eig_vectors(&a, Storage::Fixed, SortOrder::Natural).is_err());
        let (zz, vv) = eig_vectors_complex(&a, Storage::Fixed).unwrap();
        assert_eq!(zz.len(), 2);
        assert_eq!(vv.len(), 2);
    }

    #[test]
    fn celerity_compat_vs_maximum() {
        // UD-033: MaterialElasticLinearAnisotropic uses eigC(0) of the
        // *unsorted* values. For a diagonal C the Schur order is the diagonal
        // order, so eigC(0) is not the maximum here.
        let c = mx(3, &[&[1., 0., 0.], &[0., 3., 0.], &[0., 0., 2.]]);
        let unsorted = eig(&c, Storage::Dynamic).unwrap();
        assert_eq!(unsorted, vec![1.0, 3.0, 2.0]);
        let compat = unsorted[0];
        let corrected = *eigh(&c, Storage::Dynamic).unwrap().last().unwrap();
        assert_eq!((compat, corrected), (1.0, 3.0));
        let (sorted, _) = eig_vectors(&c, Storage::Dynamic, SortOrder::Descending).unwrap();
        assert_eq!(sorted[0], corrected);
    }

    #[test]
    fn sort_is_stable_insertion_sort() {
        assert_eq!(
            descending_permutation(&[1.0, 3.0, 3.0, 2.0]),
            vec![1, 2, 3, 0]
        );
        assert_eq!(descending_permutation(&[]), Vec::<usize>::new());
        assert_eq!(descending_permutation(&[5.0, 4.0]), vec![0, 1]);
    }

    #[test]
    fn errors() {
        assert!(eig(&MatrixX::zeros(2, 3), Storage::Dynamic).is_err());
        assert!(eigh(&MatrixX::zeros(2, 3), Storage::Dynamic).is_err());
        let mut bad = MatrixX::zeros(2, 2);
        *bad.at_mut(0, 0) = f64::NAN;
        assert!(
            eig(&bad, Storage::Dynamic).is_err()
                || eig(&bad, Storage::Dynamic).unwrap()[0].is_nan()
        );
        assert_eq!(
            eig(&MatrixX::zeros(0, 0), Storage::Dynamic).unwrap(),
            Vec::<f64>::new()
        );
        assert_eq!(
            eigh(&MatrixX::zeros(1, 1), Storage::Fixed).unwrap(),
            vec![0.0]
        );
    }
}
