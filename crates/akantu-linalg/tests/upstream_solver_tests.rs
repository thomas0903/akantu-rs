// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): test/test_solver/test_sparse_matrix_profile.cc,
// test_sparse_matrix_assemble.cc, test_sparse_matrix_product.cc (+ .verified),
// test_sparse_solver_eigen.cc and test_sparse_solver_mumps.cc (same intent
// with the pure-Rust backends: MUMPS has its own tier), at
// 35a851780d0b9471d258ed53a003ae98aaa95ea0; plus independent checks of the
// AIJ semantics and robustness against invalid input.

//! Ports of the upstream sparse-matrix and sparse-solver tests.

use akantu_core::GlobalIdx;
use akantu_core::enums::MatrixType;
use akantu_linalg::direct::{CscMatrix, SparseSolverFactory};
use akantu_linalg::{
    EquationLayout, NoReduction, SparseMatrix, SparseMatrixAij, SymmetryPolicy, TermsToAssemble,
    matrix_market,
};

/// `test/test_solver/triangle.geo` meshed at order 1 (frozen Gmsh 4.13.1
/// mesh): 513 nodes, 2 DOFs per node.
const TRIANGLE_SYSTEM_SIZE: GlobalIdx = 1026;

/// test_sparse_matrix_profile: a symmetric matrix of the system size,
/// `add(i, i)` for i < 10 and `add(0, 9)`, `saveProfile`; then values
/// `add(i, i, 10 i)`, `add(0, 9, 100)`, `saveMatrix`.
#[test]
fn test_sparse_matrix_profile() {
    let mut a = SparseMatrixAij::new("A", TRIANGLE_SYSTEM_SIZE, MatrixType::Symmetric);
    for i in 0..10 {
        a.add_to_profile(i, i).unwrap();
    }
    a.add_to_profile(0, 9).unwrap();
    let mut profile =
        String::from("%%MatrixMarket matrix coordinate pattern symmetric\n1026 1026 11\n");
    for i in 1..=10 {
        profile.push_str(&format!("{i} {i} 1\n"));
    }
    profile.push_str("1 10 1\n");
    assert_eq!(a.profile_text(), profile);
    for i in 0..10 {
        a.add(i, i, (i * 10) as f64).unwrap();
    }
    a.add(0, 9, 100.0).unwrap();
    let mut matrix =
        String::from("%%MatrixMarket matrix coordinate real symmetric\n1026 1026 11\n");
    for i in 1..=10 {
        matrix.push_str(&format!("{i} {i} {}\n", (i - 1) * 10));
    }
    matrix.push_str("1 10 100\n");
    assert_eq!(a.matrix_market_text(), matrix);
    // files: saveMatrix appends ".mtx"
    let dir = std::env::temp_dir().join(format!("akantu-linalg-profile-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let written = a.save_matrix(&dir.join("matrix_hand")).unwrap();
    assert_eq!(written, dir.join("matrix_hand.mtx"));
    assert_eq!(std::fs::read_to_string(&written).unwrap(), matrix);
    a.save_profile(&dir.join("profile_hand.mtx")).unwrap();
    assert_eq!(
        std::fs::read_to_string(dir.join("profile_hand.mtx")).unwrap(),
        profile
    );
    std::fs::remove_dir_all(&dir).unwrap();
}

/// test_sparse_matrix_assemble: upstream only creates the symmetric matrix
/// and saves it (its assembly loop is commented out); the assembly itself
/// is checked bitwise by `aij_oracle`.
#[test]
fn test_sparse_matrix_assemble() {
    let a = SparseMatrixAij::new("A", TRIANGLE_SYSTEM_SIZE, MatrixType::Symmetric);
    assert_eq!(
        a.matrix_market_text(),
        "%%MatrixMarket matrix coordinate real symmetric\n1026 1026 0\n"
    );
}

/// test_sparse_matrix_product on `bar.msh` (121 nodes, 2 DOFs): `A(i, i)
/// = 2` for every local-or-master DOF, `x = 1`, `x = A x`; the
/// `.verified` output lists 121 × {2, 2}.
#[test]
fn test_sparse_matrix_product() {
    let n = 121 * 2;
    let layout = EquationLayout::serial(n).unwrap();
    let mut a = SparseMatrixAij::new("A", n as GlobalIdx, MatrixType::Symmetric);
    for i in 0..n {
        if layout.is_owned(i) {
            a.add(i as GlobalIdx, i as GlobalIdx, 2.0).unwrap();
        }
    }
    let x = vec![1.0; n];
    let mut y = x.clone();
    a.mat_vec_mul(&layout, &NoReduction, &x, &mut y, 1.0, 0.0)
        .unwrap();
    assert!(
        y.iter().all(|&v| v == 2.0),
        "values: {{2, 2}} x 121 (.verified)"
    );
    let text = a.matrix_market_text();
    assert!(
        text.starts_with("%%MatrixMarket matrix coordinate real symmetric\n242 242 242\n1 1 2\n")
    );
}

/// test_sparse_solver_eigen / test_sparse_solver_mumps (serial): 11 DOFs,
/// pre-assembled diagonal `1 / (1 + g)`, `b = 1`, solution `1 + g` within
/// a relative 1e-15 — with every registered pure-Rust backend (`eigen`,
/// and `faer` in place of MUMPS).
#[test]
fn test_sparse_solver_eigen_and_mumps_intent() {
    let n = 11;
    let mut terms = TermsToAssemble::new("x", "x");
    for i in 0..n {
        terms.assign(i, i, 1.0 / (1.0 + f64::from(i)));
    }
    let mut a = SparseMatrixAij::new("A", n as GlobalIdx, MatrixType::Symmetric);
    for t in &terms {
        a.add(GlobalIdx::from(t.i), GlobalIdx::from(t.j), t.value)
            .unwrap();
    }
    let factory = SparseSolverFactory::default();
    for name in ["eigen", "faer"] {
        let mut s = factory.create_by_name(name).unwrap();
        s.analysis(&a).unwrap();
        s.factorize(&a).unwrap();
        let mut x = vec![0.0; n as usize];
        s.solve(&vec![1.0; n as usize], &mut x).unwrap();
        for (i, xi) in x.iter().enumerate() {
            let d = 1.0 + i as f64;
            assert!((xi - d).abs() / d <= 1e-15, "{name}: {xi} != {d}");
        }
    }
}

/// Independent check: symmetric storage of symmetric element matrices and
/// unsymmetric storage of the same matrices represent the same operator,
/// bit for bit (each global pair receives the same terms in the same
/// element order), and their products agree within rounding.
#[test]
fn symmetric_and_unsymmetric_assembly_agree() {
    // a small unstructured "mesh": 7 triangles on 6 nodes, 2 DOFs
    let conn = [
        [0, 1, 2],
        [1, 3, 2],
        [2, 3, 4],
        [3, 5, 4],
        [0, 2, 4],
        [1, 5, 3],
        [0, 4, 5],
    ];
    let n = 12;
    let mut sym = SparseMatrixAij::new("S", n, MatrixType::Symmetric);
    let mut uns = SparseMatrixAij::new("U", n, MatrixType::Unsymmetric);
    let mut elems = Vec::new();
    for (e, c) in conn.iter().enumerate() {
        let eqs: Vec<GlobalIdx> = c.iter().flat_map(|&v| [2 * v, 2 * v + 1]).collect();
        let mut ke = vec![0.0; 36];
        for r in 0..6 {
            for q in r..6 {
                let v = if r == q {
                    3.0 + (e + r) as f64 * 0.25
                } else {
                    0.1 * ((r * 7 + q * 3 + e) % 11) as f64 - 0.5
                };
                ke[r + q * 6] = v;
                ke[q + r * 6] = v;
            }
        }
        elems.push((eqs, ke));
    }
    for m in [&mut sym, &mut uns] {
        for (eqs, _) in &elems {
            m.add_block_to_profile(eqs, eqs).unwrap();
        }
        for (eqs, ke) in &elems {
            m.add_values(eqs, eqs, ke, MatrixType::Symmetric).unwrap();
        }
    }
    let cs = CscMatrix::from_aij(&sym).unwrap();
    let cu = CscMatrix::from_aij(&uns).unwrap();
    assert_eq!(cs, cu, "same operator, bit for bit");
    let layout = EquationLayout::serial(n as usize).unwrap();
    let x: Vec<f64> = (0..n).map(|i| 1.0 - i as f64 / 5.0).collect();
    let (mut ys, mut yu) = (vec![0.0; n as usize], vec![0.0; n as usize]);
    sym.mat_vec_mul(&layout, &NoReduction, &x, &mut ys, 1.0, 0.0)
        .unwrap();
    uns.mat_vec_mul(&layout, &NoReduction, &x, &mut yu, 1.0, 0.0)
        .unwrap();
    for (a, b) in ys.iter().zip(&yu) {
        // same terms, different summation order: |Δ| <= 2 γ_12 Σ|a x|
        assert!((a - b).abs() <= 2.0 * 12.0 * f64::EPSILON * 40.0, "{a} {b}");
    }
    // applyBoundary keeps the operator symmetric in both storages
    let blocked: Vec<bool> = (0..n).map(|i| i % 5 == 0).collect();
    sym.apply_boundary(&layout, &blocked, 1.0).unwrap();
    uns.apply_boundary(&layout, &blocked, 1.0).unwrap();
    let cs = CscMatrix::from_aij(&sym).unwrap();
    let cu = CscMatrix::from_aij(&uns).unwrap();
    assert_eq!(cs, cu);
    for j in 0..cu.n() {
        for p in cu.col_ptr()[j]..cu.col_ptr()[j + 1] {
            let i = cu.row_idx()[p];
            assert_eq!(
                uns.get(i as GlobalIdx, j as GlobalIdx),
                uns.get(j as GlobalIdx, i as GlobalIdx)
            );
            if blocked[i] || blocked[j] {
                assert_eq!(cu.values()[p], if i == j { 1.0 } else { 0.0 });
            }
        }
    }
}

/// UD-034: an unsymmetric block into a symmetric matrix is promoted by
/// default (full operator kept) and selected by global upper triangle in
/// compatibility mode, exactly as upstream.
#[test]
fn unsymmetric_block_into_symmetric_matrix_policies() {
    let eqs = [3, 1];
    let ke = [1.0, 2.0, 3.0, 4.0]; // column-major [[1, 3], [2, 4]]
    let mut up = SparseMatrixAij::new("K", 4, MatrixType::Symmetric);
    up.set_symmetry_policy(SymmetryPolicy::UpstreamUpperSelection);
    up.add_block_to_profile(&eqs, &eqs).unwrap();
    up.add_values(&eqs, &eqs, &ke, MatrixType::Unsymmetric)
        .unwrap();
    // kept: global column >= row: (3,3) 1, (1,3) 2, (1,1) 4; dropped (3,1) 3
    assert_eq!((up.get(3, 3), up.get(1, 3), up.get(1, 1)), (1.0, 2.0, 4.0));
    let mut pr = SparseMatrixAij::new("K", 4, MatrixType::Symmetric);
    pr.add_block_to_profile(&eqs, &eqs).unwrap();
    pr.add_values(&eqs, &eqs, &ke, MatrixType::Unsymmetric)
        .unwrap();
    assert!(pr.was_promoted());
    assert_eq!((pr.get(3, 1), pr.get(1, 3)), (3.0, 2.0));
}

/// No panic on user-controllable input: every invalid call returns an
/// error and leaves the matrix unchanged.
#[test]
fn invalid_input_is_an_error_not_a_panic() {
    let mut m = SparseMatrixAij::new("K", 3, MatrixType::Unsymmetric);
    m.add(0, 0, 1.0).unwrap();
    let snapshot = (m.coo().0.to_vec(), m.coo().2.to_vec(), m.value_release());
    let layout = EquationLayout::serial(3).unwrap();
    assert!(m.add(-2, 0, 1.0).is_err());
    assert!(
        m.add_values(&[0, 1], &[0], &[1.0], MatrixType::Unsymmetric)
            .is_err()
    );
    assert!(
        m.add_values(&[0, 1], &[0, 1], &[1.0; 4], MatrixType::Unsymmetric)
            .is_err()
    ); // not profiled
    assert!(m.apply_boundary(&layout, &[true], 1.0).is_err());
    let mut y = [0.0; 2];
    assert!(
        m.mat_vec_mul(&layout, &NoReduction, &[0.0; 3], &mut y, 1.0, 0.0)
            .is_err()
    );
    assert!(
        m.copy_content(&SparseMatrixAij::new("X", 3, MatrixType::Unsymmetric))
            .is_err()
    );
    assert!(m.get_mut(2, 2).is_err());
    assert_eq!(
        (m.coo().0.to_vec(), m.coo().2.to_vec(), m.value_release()),
        snapshot
    );
    assert!(SparseMatrix::value(&m, 5, 5).is_ok()); // absent entries read as 0
    // a layout that does not know a stored equation
    let mut big = SparseMatrixAij::new("B", 10, MatrixType::Unsymmetric);
    big.add(9, 9, 1.0).unwrap();
    assert!(big.apply_boundary(&layout, &[false; 3], 1.0).is_err());
    let mut y3 = [0.0; 3];
    assert!(
        big.mat_vec_mul(&layout, &NoReduction, &[0.0; 3], &mut y3, 1.0, 0.0)
            .is_err()
    );
    // solvers
    let f = SparseSolverFactory::default();
    for name in ["eigen", "faer"] {
        let mut s = f.create_by_name(name).unwrap();
        assert!(
            s.solve(&[1.0], &mut [0.0]).is_err(),
            "{name}: solve before factorize"
        );
        assert!(
            s.factorize(&m).is_err(),
            "{name}: factorize before analysis"
        );
        s.analysis(&m).unwrap();
        assert!(s.factorize(&m).is_err(), "{name}: singular (zero rows)");
    }
    assert!(f.create_by_name("superlu").is_err());
    // Matrix Market input
    for bad in [
        "",
        "%%MatrixMarket matrix coordinate real general\n2 2 1\n0 1 1\n",
        "garbage",
    ] {
        assert!(matrix_market::parse(bad).is_err());
    }
    let mm =
        matrix_market::parse("%%MatrixMarket matrix coordinate real general\n2 3 0\n").unwrap();
    assert!(SparseMatrixAij::from_matrix_market("R", &mm).is_err());
    assert!(CscMatrix::from_triplets(2, &[(0, 5, 1.0)]).is_err());
}
