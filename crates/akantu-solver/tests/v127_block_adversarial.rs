// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS). Adversarial checks of
// the block-lumped rotary matrices (D-059), written independently of the
// port; no upstream counterpart.

#![allow(clippy::needless_range_loop)]

use akantu_comm::Communicator;
use akantu_core::array::ArrayView;
use akantu_core::{Error, Real};
use akantu_solver::DofManager;

/// Deterministic pseudo-random numbers in [-1, 1).
struct Lcg(u64);
impl Lcg {
    fn next(&mut self) -> Real {
        self.0 = self
            .0
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        ((self.0 >> 11) as Real / (1u64 << 53) as Real) * 2.0 - 1.0
    }
}

/// Random rotation (Gram–Schmidt of three random vectors).
fn rotation(r: &mut Lcg) -> [[Real; 3]; 3] {
    let mut q = [[0.0; 3]; 3];
    for c in 0..3 {
        let mut v = [r.next(), r.next(), r.next()];
        for p in 0..c {
            let d: Real = (0..3).map(|i| v[i] * q[i][p]).sum();
            for i in 0..3 {
                v[i] -= d * q[i][p];
            }
        }
        let n = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt();
        for i in 0..3 {
            q[i][c] = v[i] / n;
        }
    }
    q
}

/// SPD block `Q diag(λ) Qᵀ` with a wide spectrum (condition `λmax/λmin`).
fn spd_block(r: &mut Lcg, lambdas: [Real; 3]) -> [[Real; 3]; 3] {
    let q = rotation(r);
    let mut b = [[0.0; 3]; 3];
    for i in 0..3 {
        for j in 0..3 {
            b[i][j] = (0..3).map(|k| q[i][k] * lambdas[k] * q[j][k]).sum();
        }
    }
    // exact symmetry of the stored data
    for i in 0..3 {
        for j in 0..i {
            b[i][j] = b[j][i];
        }
    }
    b
}

/// Dense Cholesky solve (reference, independent of the kernel's LDLᵀ).
fn chol_solve(a: &[Vec<Real>], b: &[Real]) -> Vec<Real> {
    let n = b.len();
    let mut l = vec![vec![0.0; n]; n];
    for i in 0..n {
        for j in 0..=i {
            let mut s = a[i][j];
            for k in 0..j {
                s -= l[i][k] * l[j][k];
            }
            if i == j {
                assert!(s > 0.0);
                l[i][i] = s.sqrt();
            } else {
                l[i][j] = s / l[j][j];
            }
        }
    }
    let mut y = vec![0.0; n];
    for i in 0..n {
        let mut s = b[i];
        for k in 0..i {
            s -= l[i][k] * y[k];
        }
        y[i] = s / l[i][i];
    }
    let mut x = vec![0.0; n];
    for i in (0..n).rev() {
        let mut s = y[i];
        for k in i + 1..n {
            s -= l[k][i] * x[k];
        }
        x[i] = s / l[i][i];
    }
    x
}

const NA: usize = 3; // tuples of family "a" (2 components, registered first)
const NB: usize = 5; // tuples of family "b" (7 components, blocks on 2..5)
const NBC: usize = 7;
const FIRST: usize = 2;

struct Setup {
    dm: DofManager,
    /// global dense operator (local equations)
    dense: Vec<Vec<Real>>,
    diag: Vec<Real>,
    eq_b: Vec<usize>,
}

/// Two families (offset equations), blocks of condition up to 1e6 on
/// components 2..5 of the 7-component family "b".
fn setup(seed: u64) -> Setup {
    let mut r = Lcg(seed);
    let mut dm = DofManager::new("dm", Communicator::serial());
    dm.register_generic_dofs("a", NA, 2).unwrap();
    dm.register_generic_dofs("b", NB, NBC).unwrap();
    dm.register_blocked_dofs("a").unwrap();
    dm.register_blocked_dofs("b").unwrap();
    dm.new_lumped_matrix("M").unwrap();
    let diag_a: Vec<Real> = (0..NA * 2).map(|_| 1.5 + r.next()).collect();
    let mut diag_b = vec![0.0; NB * NBC];
    let mut off = vec![0.0; NB * 3];
    let mut blocks = Vec::new();
    for t in 0..NB {
        for c in 0..NBC {
            diag_b[t * NBC + c] = 2.0 + r.next();
        }
        let scale = 10f64.powi(t as i32 - 2);
        let b = spd_block(&mut r, [scale * 1e3, scale * 1.0, scale * 1e-3]);
        for i in 0..3 {
            diag_b[t * NBC + FIRST + i] = b[i][i];
        }
        off[t * 3] = b[0][1];
        off[t * 3 + 1] = b[0][2];
        off[t * 3 + 2] = b[1][2];
        blocks.push(b);
    }
    dm.assemble_to_lumped_matrix("a", ArrayView::new(&diag_a, 2).unwrap(), "M", 1.0)
        .unwrap();
    dm.assemble_to_lumped_matrix("b", ArrayView::new(&diag_b, NBC).unwrap(), "M", 1.0)
        .unwrap();
    // couplings assembled in two halves (additivity)
    let half: Vec<Real> = off.iter().map(|v| 0.5 * v).collect();
    dm.assemble_lumped_block_couplings("b", "M", FIRST, ArrayView::new(&half, 3).unwrap(), 1.0)
        .unwrap();
    dm.assemble_lumped_block_couplings("b", "M", FIRST, ArrayView::new(&off, 3).unwrap(), 0.5)
        .unwrap();
    let n = dm.lumped_matrix("M").unwrap().values.len();
    let eq_a: Vec<usize> = dm
        .family("a")
        .unwrap()
        .local_equation_numbers
        .iter()
        .map(|&e| e as usize)
        .collect();
    let eq_b: Vec<usize> = dm
        .family("b")
        .unwrap()
        .local_equation_numbers
        .iter()
        .map(|&e| e as usize)
        .collect();
    let mut dense = vec![vec![0.0; n]; n];
    let mut diag = vec![0.0; n];
    for (k, &e) in eq_a.iter().enumerate() {
        dense[e][e] = diag_a[k];
        diag[e] = diag_a[k];
    }
    for (k, &e) in eq_b.iter().enumerate() {
        dense[e][e] = diag_b[k];
        diag[e] = diag_b[k];
    }
    for (t, b) in blocks.iter().enumerate() {
        for i in 0..3 {
            for j in 0..3 {
                if i != j {
                    let (ei, ej) = (eq_b[t * NBC + FIRST + i], eq_b[t * NBC + FIRST + j]);
                    dense[ei][ej] = b[i][j];
                }
            }
        }
    }
    Setup {
        dm,
        dense,
        diag,
        eq_b,
    }
}

/// Product with an offset family and a scale against the dense operator.
#[test]
fn v127_product_with_offset_family_and_scale() {
    let mut s = setup(7);
    let mut r = Lcg(99);
    let xb: Vec<Real> = (0..NB * NBC).map(|_| r.next()).collect();
    let n = s.diag.len();
    let mut xg = vec![0.0; n];
    for (k, &e) in s.eq_b.iter().enumerate() {
        xg[e] = xb[k];
    }
    s.dm.zero_residual();
    s.dm.assemble_lumped_mat_mul_vect_to_residual("b", "M", &xb, -0.3)
        .unwrap();
    let got = s.dm.residual().values().to_vec();
    let mut worst: Real = 0.0;
    for i in 0..n {
        let want: Real = -0.3 * (0..n).map(|j| s.dense[i][j] * xg[j]).sum::<Real>();
        let scale: Real = (0..n)
            .map(|j| (s.dense[i][j] * xg[j]).abs())
            .sum::<Real>()
            .max(Real::MIN_POSITIVE);
        worst = worst.max((got[i] - want).abs() / (0.3 * scale));
    }
    eprintln!(
        "block product (offset family, scale −0.3): {:.2} ε",
        worst / f64::EPSILON
    );
    assert!(worst <= 8.0 * f64::EPSILON);
}

/// Every one of the 8 blocked masks on every block (5 blocks, condition up
/// to 1e6, scales 1e-2…1e2) against a dense Cholesky solve of the free
/// system; blocked DOFs get the diagonal rule.
#[test]
fn v127_solve_all_masks_ill_conditioned() {
    let alpha = 1.25;
    let mut worst_all: Real = 0.0;
    for mask_seed in 0..8u32 {
        let mut s = setup(11);
        let n = s.diag.len();
        let mut r = Lcg(1234 + mask_seed as u64);
        let b: Vec<Real> = (0..n).map(|_| r.next()).collect();
        let mut blocked_b = vec![false; NB * NBC];
        for t in 0..NB {
            // mask (mask_seed + t) mod 8 on the three coupled components
            let m = (mask_seed as usize + t) % 8;
            for i in 0..3 {
                blocked_b[t * NBC + FIRST + i] = m >> i & 1 == 1;
            }
            // a blocked uncoupled component too
            blocked_b[t * NBC] = t % 2 == 0;
        }
        s.dm.blocked_dofs_mut("b")
            .unwrap()
            .as_mut_slice()
            .copy_from_slice(&blocked_b);
        let mut blocked = vec![false; n];
        for (k, &e) in s.eq_b.iter().enumerate() {
            blocked[e] = blocked_b[k];
        }
        s.dm.residual_vector_mut()
            .values_mut()
            .as_mut_slice()
            .copy_from_slice(&b);
        s.dm.update_global_blocked_dofs();
        s.dm.solve_lumped("M", alpha).unwrap();
        let x = s.dm.solution().to_vec();
        let free: Vec<usize> = (0..n).filter(|&i| !blocked[i]).collect();
        let af: Vec<Vec<Real>> = free
            .iter()
            .map(|&i| free.iter().map(|&j| s.dense[i][j]).collect())
            .collect();
        let bf: Vec<Real> = free.iter().map(|&i| b[i]).collect();
        let xf = chol_solve(&af, &bf);
        for (k, &i) in free.iter().enumerate() {
            let want = alpha * xf[k];
            // backward-stable solves of a 3×3 block of condition ≤ 1e6:
            // relative error ≲ cond·ε per component, measured against the
            // largest component of the same block
            let err = (x[i] - want).abs()
                / want
                    .abs()
                    .max(alpha * xf.iter().fold(0.0, |m: Real, v| m.max(v.abs())) * 1e-6);
            worst_all = worst_all.max(err);
            assert!(
                err <= 1e6 * 16.0 * f64::EPSILON,
                "mask {mask_seed}, eq {i}: {} vs {want}",
                x[i]
            );
        }
        for i in (0..n).filter(|&i| blocked[i]) {
            assert_eq!(x[i].to_bits(), (alpha * (b[i] / s.diag[i])).to_bits());
        }
    }
    eprintln!("block solve, all masks, condition 1e6: worst relative {worst_all:.2e}");
}

/// The block solve uses the blocked snapshot of the last
/// `update_global_blocked_dofs`, not the live flags (as the correctors and
/// the norms of the lumped solver; upstream's solve ignores blocked flags).
#[test]
fn v127_solve_uses_the_blocked_snapshot() {
    let mut s = setup(5);
    let n = s.diag.len();
    let b: Vec<Real> = (0..n).map(|i| (i as Real * 0.37).sin()).collect();
    s.dm.residual_vector_mut()
        .values_mut()
        .as_mut_slice()
        .copy_from_slice(&b);
    s.dm.update_global_blocked_dofs();
    s.dm.solve_lumped("M", 1.0).unwrap();
    let x0 = s.dm.solution().to_vec();
    // block a coupled component without a new snapshot: same solve
    s.dm.blocked_dofs_mut("b").unwrap().as_mut_slice()[FIRST] = true;
    s.dm.residual_vector_mut()
        .values_mut()
        .as_mut_slice()
        .copy_from_slice(&b);
    s.dm.solve_lumped("M", 1.0).unwrap();
    assert_eq!(x0, s.dm.solution());
    // after the snapshot: different
    s.dm.update_global_blocked_dofs();
    s.dm.residual_vector_mut()
        .values_mut()
        .as_mut_slice()
        .copy_from_slice(&b);
    s.dm.solve_lumped("M", 1.0).unwrap();
    assert_ne!(x0, s.dm.solution());
}

/// Couplings on a manager of a 2-rank local world are refused on every
/// rank, before any state change (the check is rank-independent).
#[test]
fn v127_distributed_refusal_on_every_rank() {
    use std::time::Duration;
    let res = akantu_comm::run_local(2, Duration::from_secs(20), |comm: Communicator| {
        let mut dm = DofManager::new("dm", comm);
        dm.register_generic_dofs("u", 2, 6).unwrap();
        dm.new_lumped_matrix("M").unwrap();
        let off = vec![0.1; 6];
        let r =
            dm.assemble_lumped_block_couplings("u", "M", 3, ArrayView::new(&off, 3).unwrap(), 1.0);
        (
            matches!(r, Err(Error::Unsupported(_))),
            dm.lumped_block_couplings("M").is_none(),
        )
    });
    eprintln!("block distributed refusal: {res:?}");
    assert!(res.iter().all(|&(refused, none)| refused && none));
}
