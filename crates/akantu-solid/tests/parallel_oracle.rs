// SPDX-License-Identifier: LGPL-3.0-or-later
// akantu-rs differential tests (no upstream counterpart): solid
// mechanics on a mesh distributed with the oracle's Scotch partition
// (prescribed, D-009) against the oracle's per-rank records
// fixtures/parallel/smm_{explicit_cube_n4,explicit_two_laws_cube_n4,static_cube_n3}/np{1,2,4}/rank*
// (smm_explicit_cube_n4 also np8)
// (oracle/probes/parallel; ghost filters in canonical rank order, see the
// probe and NUMERICAL_VALIDATION).

#[path = "../../akantu-mesh/tests/par_common/mod.rs"]
mod par_common;
mod par_oracle;
mod par_smm;

use akantu_validation::reduce::NanMinMax;
use std::time::Duration;

use akantu_comm::{Communicator, run_local};
use par_smm::Outcome;

fn on_oracle_ranks(scenario: &str, expected: &[usize], f: fn(Communicator) -> Outcome) {
    let counts = par_common::oracle_rank_counts(scenario);
    assert_eq!(counts, expected, "oracle fixtures present for {scenario}");
    for np in counts {
        for (r, o) in run_local(np, Duration::from_secs(120), f)
            .into_iter()
            .enumerate()
        {
            if let Err(e) = o {
                panic!("{scenario} np {np} rank {r}: {e}");
            }
        }
    }
}

#[test]
fn smm_explicit_cube_n4_matches_the_oracle() {
    // 8 ranks: recorded later
    on_oracle_ranks(
        "smm_explicit_cube_n4",
        &[1, 2, 4, 8],
        par_oracle::explicit_matches,
    );
}

/// Two laws, law 1 in finite
/// deformation (probe version 5).
#[test]
fn smm_explicit_two_laws_cube_n4_matches_the_oracle() {
    on_oracle_ranks(
        "smm_explicit_two_laws_cube_n4",
        &[1, 2, 4],
        par_oracle::explicit_two_laws_matches,
    );
}

#[test]
fn smm_static_cube_n3_matches_the_oracle() {
    // MUMPS on 4 processes is not reproducible: recorded on 1 and 2 only
    on_oracle_ranks("smm_static_cube_n3", &[1, 2], par_oracle::static_matches);
}

/// The folds of the exact energy check. With
/// `u = 2^-53` (half an ulp of 1), the rank order and the observed trees of
/// the oracle's `MPI_Allreduce` give different bits at 4 and 8 ranks; a
/// rank count without an observed association is refused; the check
/// accepts the rank-order port value against the tree-folded record,
/// refuses it under `NativeMpi`, and refuses a contribution changed by one
/// ulp.
#[test]
fn energy_folds_and_the_exact_energy_check() {
    let u = 2f64.powi(-53);
    let one_ulp = 2f64.powi(-52);
    let p4 = [1.0, u, u, u];
    let p8 = [1.0, u, u, u, u, u, u, u];
    assert_eq!(par_common::rank_order_fold(&p4), Some(1.0));
    assert_eq!(par_common::rank_order_fold(&p8), Some(1.0));
    assert_eq!(par_common::oracle_allreduce_fold(&p4), Ok(1.0 + one_ulp));
    assert_eq!(
        par_common::oracle_allreduce_fold(&p8),
        Ok(1.0 + 3.0 * one_ulp)
    );
    assert_eq!(par_common::oracle_allreduce_fold(&[0.25]), Ok(0.25));
    assert_eq!(par_common::oracle_allreduce_fold(&[0.25, 0.5]), Ok(0.75));
    for n in [0, 3, 5, 6, 7, 9, 16] {
        assert!(
            par_common::oracle_allreduce_fold(&vec![1.0; n]).is_err(),
            "{n} ranks: no observed association, must be refused"
        );
    }
    // two steps on 8 ranks: step 0 the case above, step 1 rank-order
    // compatible values
    let parts: Vec<Vec<f64>> = (0..8)
        .map(|r| vec![p8[r], 0.5f64.powi(r as i32 + 1)])
        .collect();
    let col = |s: usize| parts.iter().map(|p| p[s]).collect::<Vec<_>>();
    let tree: Vec<f64> = (0..2)
        .map(|s| par_common::oracle_allreduce_fold(&col(s)).unwrap())
        .collect();
    let chain: Vec<f64> = (0..2)
        .map(|s| par_common::rank_order_fold(&col(s)).unwrap())
        .collect();
    assert_ne!(tree[0].to_bits(), chain[0].to_bits());
    let e = "model/kinetic_energy";
    assert_eq!(
        par_oracle::energies_match(e, &chain, &tree, &parts, false, false),
        Ok(())
    );
    assert_eq!(
        par_oracle::energies_match(e, &tree, &tree, &parts, true, false),
        Ok(())
    );
    assert!(par_oracle::energies_match(e, &chain, &tree, &parts, true, false).is_err());
    assert!(par_oracle::energies_match(e, &tree, &tree, &parts, false, false).is_err());
    // rank 0's contribution at step 1 one ulp off (kept by both folds:
    // every partial sum lies in [0.5, 1)): the recorded value is no longer
    // the fold of the contributions
    let mut bad = parts.clone();
    bad[0][1] = f64::from_bits(bad[0][1].to_bits() + 1);
    let bad_chain: Vec<f64> = (0..2)
        .map(|s| {
            par_common::rank_order_fold(&bad.iter().map(|p| p[s]).collect::<Vec<_>>()).unwrap()
        })
        .collect();
    assert!(par_oracle::energies_match(e, &bad_chain, &tree, &bad, false, false).is_err());
    // a rank count without an observed association is refused
    assert!(par_oracle::energies_match(e, &chain, &tree, &parts[..7], false, false).is_err());
}

/// Error-free transformations (Knuth TwoSum; Dekker TwoProduct with
/// Veltkamp splitting, no FMA): `a + b = s + e`, `a · b = p + e` exactly
/// (no overflow/underflow for the magnitudes here).
fn two_sum(a: f64, b: f64) -> (f64, f64) {
    let s = a + b;
    let z = s - a;
    (s, (a - (s - z)) + (b - z))
}

fn split(a: f64) -> (f64, f64) {
    let c = 134_217_729.0 * a; // 2^27 + 1
    let hi = c - (c - a);
    (hi, a - hi)
}

fn two_product(a: f64, b: f64) -> (f64, f64) {
    let p = a * b;
    let (ah, al) = split(a);
    let (bh, bl) = split(b);
    (p, al * bl - (((p - ah * bh) - al * bh) - ah * bl))
}

/// `Σ x_k` evaluated as in twice the working precision (Ogita, Rump,
/// Oishi 2005, Sum2/Dot2), for the residual of a backward-stable solve
/// (whose true size is below the rounding of a plain evaluation).
fn accurate_sum(terms: impl Iterator<Item = (f64, f64)>) -> f64 {
    let (mut s, mut c) = (0.0, 0.0);
    for (a, b) in terms {
        let (p, ep) = two_product(a, b);
        let (t, es) = two_sum(s, p);
        s = t;
        c += ep + es;
    }
    s + c
}

fn norm2(v: &[f64]) -> f64 {
    v.iter().map(|x| x * x).sum::<f64>().sqrt()
}

/// Inputs of the a-priori bound of `fixtures/parallel/tolerances.json`
/// rule [2] (`model/displacement` of `smm_static_cube_n3`, rel 1e-12,
/// the committed form of the
/// former scratch computation). At one rank, on the port's assembled `K`:
/// `K_ff` (the 144 DOFs off the clamped face) has `cond₂ = λ_max/λ_min ≈
/// 4.08e2` (dense symmetric eigensolver); the normwise backward errors
/// `η = ‖f − K u‖₂ / (‖K‖₂ ‖u‖₂ + ‖f‖₂)` of the port's solution (faer) and
/// of the oracle's np1 solution (MUMPS), with the residual evaluated in
/// doubled precision, are ≈ 4e-17 (aarch64: 4.015e-17 / 3.740e-17); the
/// test requires `√144 · 2 · cond₂ · (η₁ + η₂) < 1e-12` (Higham, Thm 7.2:
/// each solution within `2 cond₂ η` of the exact one; max norm ≤ 2-norm ≤
/// `√n` max norm; aarch64: 7.59e-13). Also the consistent mass on the same
/// free DOFs: `cond₂(M_ff) = 25.1 ≤ cond₂(K_ff)`, so the implicit-dynamic
/// iteration matrix `J = K + c M` has `cond₂(J_ff) ≤ 4.08e2` (bound of the
/// distributed-vs-serial assertions, NUMERICAL_VALIDATION "Mesh distribution
/// and parallel SMM").
#[test]
fn smm_static_cube_n3_tolerance_bound_inputs() {
    for o in run_local(1, Duration::from_secs(120), |comm| -> Outcome {
        let (mut model, fx) = par_oracle::static_solved(&comm)?;
        // consistent mass of the same cube and boundary conditions, for the
        // implicit-dynamic vs-serial assertion (J = K + c M)
        model.assemble_mass().map_err(|e| e.to_string())?;
        let dm = model.dof_manager();
        let eq = dm
            .global_equation_numbers("displacement")
            .map_err(|e| e.to_string())?;
        let blocked = model.blocked_dofs().map_err(|e| e.to_string())?;
        let free: Vec<usize> = (0..eq.len()).filter(|&i| !blocked.as_slice()[i]).collect();
        let n = free.len();
        if n != 144 {
            return Err(format!("{n} free DOFs, expected 144"));
        }
        let pos: std::collections::HashMap<u64, usize> = free
            .iter()
            .enumerate()
            .map(|(k, &l)| (eq[l].map(|g| g as u64).unwrap_or(u64::MAX), k))
            .collect();
        // dense free-free block of an assembled matrix (one triangle stored
        // when symmetric)
        let block = |id: &str| -> std::result::Result<Vec<Vec<f64>>, String> {
            let mat = dm.matrix(id).map_err(|e| e.to_string())?;
            let sym = mat.matrix_type() == akantu_core::enums::MatrixType::Symmetric;
            let (r, c, v) = mat.coo();
            let mut k = vec![vec![0.0; n]; n];
            for ((ri, ci), x) in r.iter().zip(c).zip(v) {
                if let (Some(&i), Some(&j)) = (pos.get(&(*ri as u64)), pos.get(&(*ci as u64))) {
                    k[i][j] += x;
                    if sym && i != j {
                        k[j][i] += x;
                    }
                }
            }
            Ok(k)
        };
        let extreme_eigenvalues = |k: &[Vec<f64>]| -> std::result::Result<(f64, f64), String> {
            let mut a = akantu_core::tensor::MatrixX::zeros(n, n);
            for (i, row) in k.iter().enumerate() {
                for (j, x) in row.iter().enumerate() {
                    *a.at_mut(i, j) = *x;
                }
            }
            let ev = akantu_linalg::dense::eigh(&a, akantu_linalg::dense::Storage::Dynamic)
                .map_err(|e| e.to_string())?;
            Ok((
                ev.iter().copied().fold(f64::INFINITY, NanMinMax::nan_min),
                ev.iter()
                    .copied()
                    .fold(f64::NEG_INFINITY, NanMinMax::nan_max),
            ))
        };
        let k = block("K")?;
        let (lmin, lmax) = extreme_eigenvalues(&k)?;
        let cond2 = lmax / lmin;
        let (mmin, mmax) = extreme_eigenvalues(&block("M")?)?;
        let f_all_g = model.external_force().map_err(|e| e.to_string())?;
        let f_all = f_all_g.as_slice();
        let f: Vec<f64> = free.iter().map(|&i| f_all[i]).collect();
        let eta = |u_all: &[f64]| -> f64 {
            let u: Vec<f64> = free.iter().map(|&i| u_all[i]).collect();
            let res: Vec<f64> = (0..n)
                .map(|i| {
                    accurate_sum(
                        std::iter::once((f[i], 1.0))
                            .chain(k[i].iter().zip(&u).map(|(kij, uj)| (-kij, *uj))),
                    )
                })
                .collect();
            norm2(&res) / (lmax * norm2(&u) + norm2(&f))
        };
        let eta_port = eta(model.displacement().map_err(|e| e.to_string())?.as_slice());
        let oracle = fx
            .load_f64("model/displacement")
            .map_err(|e| e.to_string())?
            .data;
        let eta_oracle = eta(&oracle);
        let bound = (n as f64).sqrt() * 2.0 * cond2 * (eta_port + eta_oracle);
        println!(
            "INFO smm_static_cube_n3: n_free {n}, lambda_min {lmin:.6e}, lambda_max {lmax:.6e}, \
             cond2 {cond2:.6e}, eta port {eta_port:.3e}, eta oracle {eta_oracle:.3e}, bound {bound:.3e}"
        );
        println!(
            "INFO smm_static_cube_n3: consistent mass on the free DOFs: lambda {mmin:.6e} .. {mmax:.6e}, cond2 {:.4}",
            mmax / mmin
        );
        let near = |x: f64, cited: f64, rel: f64| (x / cited - 1.0).abs() <= rel;
        // the rule's figures (4 significant digits)
        if !(near(cond2, 4.076e2, 1e-3) && near(lmax, 2.813, 1e-3) && near(lmin, 6.900e-3, 1e-3)) {
            return Err(format!(
                "conditioning changed: cond2 {cond2:e}, lambda {lmin:e}..{lmax:e}"
            ));
        }
        // the bound is the criterion (η₁ + η₂ < 1.02e-16 suffices); the
        // backward errors depend on the platform's faer kernels
        if bound >= 1e-12 || eta_port.is_nan() || eta_oracle.is_nan() {
            return Err(format!(
                "a-priori bound {bound:e} does not support rel 1e-12"
            ));
        }
        // implicit dynamics solves with J = K + c M (c > 0, M SPD), so
        // cond2(J) <= max(cond2(K), cond2(M)) (mediant inequality on the
        // extreme eigenvalues): bounded by cond2(K_ff) when M_ff is better
        // conditioned
        if !(mmin > 0.0 && mmax / mmin <= cond2) {
            return Err(format!(
                "cond2(M_ff) = {:e} exceeds cond2(K_ff)",
                mmax / mmin
            ));
        }
        Ok(())
    }) {
        o.unwrap();
    }
}
