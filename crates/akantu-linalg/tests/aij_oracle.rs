// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): differential replay of
// DOFManagerDefault::addToProfile + DOFManager::assembleElementalMatricesToMatrix_
// (dof_manager_default.cc:295-388, dof_manager_inline_impl.hh:233-305),
// assemblePreassembledMatrix_ (:309-322), SparseMatrixAIJ::matVecMul,
// copyProfile, add(B, alpha) (as NewmarkBeta::assembleJacobian,
// newmark-beta.cc:231-247), applyBoundary, saveMatrix/saveProfile and
// SolverVectorDefault::saveVector against fixtures/solver/aij/*; the intent
// of test/test_solver/test_sparse_matrix_assemble.cc and
// test_sparse_matrix_product.cc on the upstream meshes.

//! Bitwise differential tests of the AIJ matrix (SPM-002…007, SPM-010,
//! SPM-013) against the oracle.

mod solver_oracle;

use akantu_core::GlobalIdx;
use akantu_core::enums::MatrixType;
use akantu_linalg::matrix_market::MatrixMarketOptions;
use akantu_linalg::{
    EquationLayout, NoReduction, SolverVector, SparseMatrixAij, SymmetryPolicy, TermsToAssemble,
};
use akantu_validation::Fixture;
use solver_oracle::*;

fn check_text(f: &Fixture, role: &str, text: &str) {
    let cid = f.case_id();
    if has_input(f, role) {
        let want = input_bytes(f, role);
        assert!(
            text.as_bytes() == want.as_slice(),
            "{cid}: {role} text differs\n--- rust (head)\n{}\n--- oracle (head)\n{}",
            &text[..text.len().min(400)],
            String::from_utf8_lossy(&want[..want.len().min(400)])
        );
    } else {
        assert_eq!(
            sha256_hex(text.as_bytes()),
            param_str(f, &format!("{role}_sha256")),
            "{cid}: {role} text digest"
        );
    }
}

#[test]
fn aij_assembly_products_terms_boundary_text_match_oracle() {
    let mut checked = 0;
    for f in cases("aij") {
        let cid = f.case_id().to_owned();
        let n: GlobalIdx = param(&f, "system_size").as_i64().unwrap();
        let ktype = matrix_type(&param_str(&f, "matrix_type"));
        let etype = matrix_type(&param_str(&f, "elemental_matrix_type"));
        let layout = EquationLayout::serial(usize::try_from(n).unwrap()).unwrap();

        // K: profile of every element, then values (UD-034 compat mode for
        // unsymmetric blocks into a symmetric matrix).
        let (eqs, kes) = elements(&f, "element_matrices");
        let mut k = SparseMatrixAij::new("K", n, ktype);
        k.set_symmetry_policy(SymmetryPolicy::UpstreamUpperSelection);
        assemble(&mut k, &eqs, &kes, etype);
        assert_matrix(&f, "K", &k);
        // byte comparison with the oracle: upstream precision (UD-061)
        let upstream = MatrixMarketOptions::upstream_precision();
        check_text(&f, "save_matrix", &k.matrix_market_text_with(&upstream));
        check_text(&f, "save_profile", &k.profile_text());

        // matVecMul (serial, upstream operation order)
        let x = f64s(&f, "matvec/x");
        let mut y = f64s(&f, "matvec/y0");
        k.mat_vec_mul(&layout, &NoReduction, &x, &mut y, 0.75, -1.25)
            .unwrap();
        assert!(
            same_bits(&y, &f64s(&f, "matvec/y_a0.75_b-1.25")),
            "{cid}: matVecMul(0.75, -1.25) {:?}",
            diff_stats(&y, &f64s(&f, "matvec/y_a0.75_b-1.25"))
        );
        k.mat_vec_mul(&layout, &NoReduction, &x, &mut y, 1.0, 0.0)
            .unwrap();
        assert!(
            same_bits(&y, &f64s(&f, "matvec/y_a1_b0")),
            "{cid}: matVecMul(1, 0)"
        );

        // Newmark J = copyProfile(M) + 0.3 K + 2.5 M (upstream sums)
        if f.quantity("M/values").is_ok() {
            let (_, mes) = elements(&f, "element_mass_matrices");
            let mut m = SparseMatrixAij::new("M", n, MatrixType::Symmetric);
            assemble(&mut m, &eqs, &mes, MatrixType::Symmetric);
            assert_matrix(&f, "M", &m);
            let jtype = if ktype == MatrixType::Unsymmetric {
                MatrixType::Unsymmetric
            } else {
                MatrixType::Symmetric
            };
            let mut j = SparseMatrixAij::new("J", n, jtype);
            j.copy_profile(&m);
            assert_matrix(&f, "J_copy_profile", &j);
            // compatibility mode of UD-177 (upstream `addMeTo`)
            j.add_matrix_upstream(&k, 0.3).unwrap();
            j.add_matrix_upstream(&m, 2.5).unwrap();
            assert_matrix(&f, "J", &j);
            // UD-177: with an unsymmetric J the corrected sum differs from
            // upstream exactly by M's missing mirror entries.
            if jtype == MatrixType::Unsymmetric {
                let mut jc = SparseMatrixAij::new("J", n, jtype);
                jc.copy_profile(&m);
                jc.add_matrix(&k, 0.3).unwrap();
                jc.add_matrix(&m, 2.5).unwrap();
                assert!(jc.nnz() >= j.nnz());
                let (irn, jcn, mv) = m.coo();
                let mut mirrored = 0;
                for ((&r, &c), &v) in irn.iter().zip(jcn).zip(mv) {
                    if r != c && v != 0.0 {
                        // jc(c, r) = (0 + 0.3 K(c, r)) + 2.5 M(r, c) = J(c, r) + 2.5 v
                        assert_eq!(
                            jc.get(c, r).to_bits(),
                            (j.get(c, r) + 2.5 * v).to_bits(),
                            "{cid}: mirror ({c},{r})"
                        );
                        mirrored += 1;
                    }
                }
                assert!(mirrored > 0);
            }
        }

        // pre-assembled terms (serial: local DOF == global equation)
        let (ti, tj, tv) = (
            i64s(&f, "terms/i"),
            i64s(&f, "terms/j"),
            f64s(&f, "terms/values"),
        );
        let mut terms = TermsToAssemble::new("u", "u");
        for ((&i, &jj), &v) in ti.iter().zip(&tj).zip(&tv) {
            terms.add(i32::try_from(i).unwrap(), i32::try_from(jj).unwrap(), v);
        }
        for t in &terms {
            k.add(GlobalIdx::from(t.i), GlobalIdx::from(t.j), t.value)
                .unwrap();
        }
        assert_matrix(&f, "K_terms", &k);

        // applyBoundary(1.0) on the blocked local DOFs
        let blocked: Vec<bool> = i64s(&f, "blocked").iter().map(|&b| b != 0).collect();
        k.apply_boundary(&layout, &blocked, 1.0).unwrap();
        assert_matrix(&f, "K_bc", &k);

        // saveVector of the residual
        let b = f64s(&f, "eigen/b");
        let mut r = SolverVector::zeros(b.len());
        r.values.copy_from_slice(&b);
        check_text(
            &f,
            "save_vector",
            &r.matrix_market_text_with(&layout, &MatrixMarketOptions::upstream_precision())
                .unwrap(),
        );
        checked += 1;
        eprintln!("{cid}: K {} entries, all bit-identical", k.nnz());
    }
    assert_eq!(checked, 6);
}
