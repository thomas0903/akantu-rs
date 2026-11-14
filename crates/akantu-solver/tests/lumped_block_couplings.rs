// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): contract tests of
// the block-lumped extension of the lumped matrices of
// src/model/common/dof_manager/dof_manager_default.cc
// (assembleLumpedMatMulVectToResidual) and
// non_linear_solver_lumped.cc (solveLumped). No upstream test (D-059).

//! Symmetric 3×3 couplings of a lumped matrix: the product and the solve
//! against a dense reference built independently from the same numbers,
//! the blocked-DOF rule (one, two or three blocked components of a block),
//! bitwise equality with the plain diagonal path when there are no
//! couplings, and the refusals.

// the dense reference elimination reads as its textbook formula
#![allow(clippy::needless_range_loop)]

mod common;

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::{Error, Real};
use akantu_mesh::MeshAccessor;
use akantu_solver::DofManager;

use common::my_model::gen_mesh;

const NBC: usize = 6;
const NT: usize = 4;

/// Diagonal (per DOF) and couplings (per tuple, components 3..6) of a
/// symmetric positive definite block-lumped matrix.
fn data() -> (Vec<Real>, Vec<Real>) {
    let mut diag = Vec::new();
    let mut off = Vec::new();
    for t in 0..NT {
        let s = 1.0 + t as Real * 0.37;
        diag.extend([2.0 * s, 2.0 * s, 2.0 * s, 0.9 * s, 1.3 * s, 0.7 * s]);
        // |o| small enough for diagonal dominance (SPD)
        off.extend([0.21 * s, -0.17 * s, 0.11 * s]);
    }
    (diag, off)
}

/// Dense matrix of the block-lumped operator (row-major `n × n`).
fn dense(diag: &[Real], off: &[Real]) -> Vec<Vec<Real>> {
    let n = diag.len();
    let mut a = vec![vec![0.0; n]; n];
    for (i, row) in a.iter_mut().enumerate() {
        row[i] = diag[i];
    }
    for t in 0..NT {
        let e = [t * NBC + 3, t * NBC + 4, t * NBC + 5];
        let o = &off[t * 3..t * 3 + 3];
        for (p, q, v) in [(0, 1, o[0]), (0, 2, o[1]), (1, 2, o[2])] {
            a[e[p]][e[q]] = v;
            a[e[q]][e[p]] = v;
        }
    }
    a
}

/// Gaussian elimination with partial pivoting (dense reference).
fn dense_solve(a: &[Vec<Real>], b: &[Real]) -> Vec<Real> {
    let n = b.len();
    let mut m: Vec<Vec<Real>> = a.to_vec();
    let mut x = b.to_vec();
    for c in 0..n {
        let p = (c..n)
            .max_by(|&i, &j| m[i][c].abs().total_cmp(&m[j][c].abs()))
            .unwrap();
        m.swap(c, p);
        x.swap(c, p);
        for r in c + 1..n {
            let f = m[r][c] / m[c][c];
            for k in c..n {
                m[r][k] -= f * m[c][k];
            }
            x[r] -= f * x[c];
        }
    }
    for r in (0..n).rev() {
        let mut s = x[r];
        for k in r + 1..n {
            s -= m[r][k] * x[k];
        }
        x[r] = s / m[r][r];
    }
    x
}

fn manager(with_couplings: bool) -> DofManager {
    let (diag, off) = data();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("u", NT, NBC).unwrap();
    dm.register_blocked_dofs("u").unwrap();
    dm.new_lumped_matrix("M").unwrap();
    dm.assemble_to_lumped_matrix("u", ArrayView::new(&diag, NBC).unwrap(), "M", 1.0)
        .unwrap();
    if with_couplings {
        dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&off, 3).unwrap(), 1.0)
            .unwrap();
    }
    dm
}

fn rhs() -> Vec<Real> {
    (0..NT * NBC)
        .map(|i| ((i * 7 + 3) % 11) as Real / 3.0 - 1.4)
        .collect()
}

#[test]
fn block_product_matches_the_dense_operator() {
    let (diag, off) = data();
    let a = dense(&diag, &off);
    let x = rhs();
    let mut dm = manager(true);
    let c = dm.lumped_block_couplings("M").unwrap();
    assert_eq!(
        (c.family(), c.first_component(), c.nb_blocks()),
        ("u", 3, NT)
    );
    dm.zero_residual();
    dm.assemble_lumped_mat_mul_vect_to_residual("u", "M", &x, -2.0)
        .unwrap();
    let r = dm.residual().values().to_vec();
    for (i, row) in a.iter().enumerate() {
        let want: Real = -2.0 * row.iter().zip(&x).map(|(p, q)| p * q).sum::<Real>();
        assert!(
            (r[i] - want).abs() <= 8.0 * f64::EPSILON * want.abs().max(1.0),
            "row {i}: {} vs {want}",
            r[i]
        );
    }
}

/// Without couplings the product and the solve are the upstream diagonal
/// operations, bit for bit; with couplings of zero value they are too
/// (each extra term adds `+0·x`).
#[test]
fn zero_couplings_and_no_couplings_are_the_diagonal_path_bitwise() {
    let (diag, _) = data();
    let x = rhs();
    let zeros = vec![0.0; NT * 3];
    let run = |dm: &mut DofManager| -> (Vec<Real>, Vec<Real>) {
        dm.zero_residual();
        dm.assemble_lumped_mat_mul_vect_to_residual("u", "M", &x, 1.0)
            .unwrap();
        let r = dm.residual().values().to_vec();
        dm.residual_vector_mut()
            .values_mut()
            .as_mut_slice()
            .copy_from_slice(&x);
        dm.update_global_blocked_dofs();
        dm.solve_lumped("M", 0.75).unwrap();
        (r, dm.solution().to_vec())
    };
    let mut plain = manager(false);
    let (r0, s0) = run(&mut plain);
    for i in 0..x.len() {
        assert_eq!(r0[i].to_bits(), (diag[i] * x[i]).to_bits());
        assert_eq!(s0[i].to_bits(), (0.75 * (x[i] / diag[i])).to_bits());
    }
    let mut zero = manager(false);
    zero.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&zeros, 3).unwrap(), 1.0)
        .unwrap();
    let (r1, s1) = run(&mut zero);
    assert_eq!(
        r0.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        r1.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
    assert_eq!(
        s0.iter().map(|v| v.to_bits()).collect::<Vec<_>>(),
        s1.iter().map(|v| v.to_bits()).collect::<Vec<_>>()
    );
}

/// The block solve honours blocked DOFs: on every block the free sub-block
/// is solved (`M_FF x_F = b_F`), for none, one, two and three blocked
/// components (every combination over the 4 blocks), against a dense
/// elimination of the free system; blocked DOFs get `α·b/a` (skipped by the
/// correctors), and the translational DOFs `α·b/a`.
#[test]
fn block_solve_honours_one_two_and_three_blocked_components() {
    let (diag, off) = data();
    let a = dense(&diag, &off);
    let b = rhs();
    let alpha = 0.75;
    // per block, the blocked mask of its three coupled components
    let patterns: [[[bool; 3]; NT]; 3] = [
        [
            [false, false, false],
            [true, false, false],
            [false, true, true],
            [true, true, true],
        ],
        [
            [false, true, false],
            [false, false, true],
            [true, false, true],
            [false, false, false],
        ],
        [
            [true, true, false],
            [false, false, false],
            [true, true, true],
            [false, true, false],
        ],
    ];
    let mut seen = [false; 4];
    for pat in patterns {
        let mut dm = manager(true);
        let mut blocked = vec![false; NT * NBC];
        for (t, mask) in pat.iter().enumerate() {
            for (i, &bl) in mask.iter().enumerate() {
                blocked[t * NBC + 3 + i] = bl;
            }
            seen[mask.iter().filter(|b| **b).count()] = true;
        }
        // a blocked translation too (diagonal path)
        blocked[NBC] = true;
        dm.blocked_dofs_mut("u")
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&blocked);
        dm.residual_vector_mut()
            .values_mut()
            .as_mut_slice()
            .copy_from_slice(&b);
        dm.update_global_blocked_dofs();
        dm.solve_lumped("M", alpha).unwrap();
        let x = dm.family("u").unwrap().solution.as_slice().to_vec();
        assert_eq!(x, dm.solution());
        // dense reference: the free system (blocked increments are zero)
        let free: Vec<usize> = (0..b.len()).filter(|&i| !blocked[i]).collect();
        let af: Vec<Vec<Real>> = free
            .iter()
            .map(|&i| free.iter().map(|&j| a[i][j]).collect())
            .collect();
        let bf: Vec<Real> = free.iter().map(|&i| b[i]).collect();
        let xf = dense_solve(&af, &bf);
        // both eliminations are backward stable on this diagonally
        // dominant system (condition number < 3): agreement to a few ulps
        // of the largest component
        let scale = xf.iter().fold(0.0_f64, |m, v| m.max(alpha * v.abs()));
        for (k, &i) in free.iter().enumerate() {
            let want = alpha * xf[k];
            assert!(
                (x[i] - want).abs() <= 16.0 * f64::EPSILON * scale,
                "pattern {pat:?}, free DOF {i}: {} vs {want}",
                x[i]
            );
        }
        for i in (0..b.len()).filter(|&i| blocked[i]) {
            assert_eq!(
                x[i].to_bits(),
                (alpha * (b[i] / diag[i])).to_bits(),
                "DOF {i}"
            );
        }
    }
    assert_eq!(seen, [true; 4], "0, 1, 2 and 3 blocked components covered");
}

#[test]
fn couplings_are_additive_and_dropped_by_zero_and_renumbering() {
    let (_, off) = data();
    let mut dm = manager(true);
    dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&off, 3).unwrap(), 0.5)
        .unwrap();
    let c = dm.lumped_block_couplings("M").unwrap();
    for (got, v) in c.off_diagonal().iter().zip(&off) {
        assert_eq!(*got, v + 0.5 * v);
    }
    assert_eq!(c.equations()[..3], [3, 4, 5]);
    // another structure on the same matrix is refused
    assert!(matches!(
        dm.assemble_lumped_block_couplings("u", "M", 2, ArrayView::new(&off, 3).unwrap(), 1.0),
        Err(Error::Unsupported(_))
    ));
    dm.zero_lumped_matrix("M").unwrap();
    assert!(dm.lumped_block_couplings("M").is_none());
    dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&off, 3).unwrap(), 1.0)
        .unwrap();
    // a numbering change (new family) drops every coupling
    dm.register_generic_dofs("w", 2, 1).unwrap();
    assert!(dm.lumped_block_couplings("M").is_none());
}

#[test]
fn invalid_couplings_are_refused() {
    let (_, off) = data();
    let mut dm = manager(false);
    // unknown lumped matrix, unknown family, component range, shapes
    assert!(
        dm.assemble_lumped_block_couplings("u", "C", 3, ArrayView::new(&off, 3).unwrap(), 1.0)
            .is_err()
    );
    assert!(
        dm.assemble_lumped_block_couplings("v", "M", 3, ArrayView::new(&off, 3).unwrap(), 1.0)
            .is_err()
    );
    assert!(matches!(
        dm.assemble_lumped_block_couplings("u", "M", 4, ArrayView::new(&off, 3).unwrap(), 1.0),
        Err(Error::DimensionMismatch { .. })
    ));
    assert!(matches!(
        dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&off[..9], 3).unwrap(), 1.0),
        Err(Error::DimensionMismatch { .. })
    ));
    assert!(matches!(
        dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&off, 6).unwrap(), 1.0),
        Err(Error::DimensionMismatch { .. })
    ));
    assert!(dm.lumped_block_couplings("M").is_none());
    // an indefinite free sub-block is refused by the solve
    let bad = vec![10.0; NT * 3];
    dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&bad, 3).unwrap(), 1.0)
        .unwrap();
    dm.residual_vector_mut().values_mut().fill(1.0);
    dm.update_global_blocked_dofs();
    assert!(matches!(
        dm.solve_lumped("M", 1.0),
        Err(Error::Numerical(_))
    ));
}

/// Periodic slaves alias their master's equations: refused.
#[test]
fn couplings_on_a_periodic_family_are_refused() {
    let mut mesh = gen_mesh(3);
    MeshAccessor::new(&mut mesh)
        .add_periodic_pair(2, 0)
        .unwrap();
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_nodal_dofs(&mesh, "u", 3).unwrap();
    dm.new_lumped_matrix("M").unwrap();
    let off = vec![0.1; 9];
    assert!(matches!(
        dm.assemble_lumped_block_couplings("u", "M", 0, ArrayView::new(&off, 3).unwrap(), 1.0),
        Err(Error::Unsupported(_))
    ));
}
