// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): tests of the fast paths of
// sparse_matrix_aij.cc (addValues, matVecMul, applyBoundary, add(B),
// copyProfile) against the straightforward loops they replace.

//! Property tests: every fast path of [`SparseMatrixAij`] (cached element
//! positions, identity layouts, entry-wise matrix sums, profile copies)
//! gives the bits, errors and release counters of the plain loop it
//! replaces, on random inputs with signed zeros, infinities, NaNs and
//! subnormals.

use super::*;
use crate::layout::{EquationOwnership, NoReduction};

/// xorshift64* (deterministic test input). `special` in 1/1000: the share
/// of special values (signed zeros, infinities, NaN, subnormals, huge);
/// kept small, or accumulated entries all become NaN and hide wrong
/// positions.
struct Rng(u64, u64);

impl Rng {
    fn next(&mut self) -> u64 {
        self.0 ^= self.0 >> 12;
        self.0 ^= self.0 << 25;
        self.0 ^= self.0 >> 27;
        self.0.wrapping_mul(0x2545_f491_4f6c_dd1d)
    }
    fn below(&mut self, n: usize) -> usize {
        (self.next() % n as u64) as usize
    }
    fn value(&mut self) -> Real {
        const SPECIAL: [Real; 10] = [
            0.0,
            -0.0,
            1.0,
            -1.5,
            Real::INFINITY,
            Real::NEG_INFINITY,
            Real::NAN,
            5e-324,
            -2.2e-308,
            1e308,
        ];
        if self.below(1000) < self.1 as usize {
            SPECIAL[self.below(SPECIAL.len())]
        } else {
            (self.next() >> 11) as Real / (1u64 << 53) as Real * 4.0 - 2.0
        }
    }
}

fn bits(v: &[Real]) -> Vec<u64> {
    v.iter().map(|x| x.to_bits()).collect()
}

fn same_state(a: &SparseMatrixAij, b: &SparseMatrixAij, what: &str) {
    assert_eq!(a.irn, b.irn, "{what}: rows");
    assert_eq!(a.jcn, b.jcn, "{what}: columns");
    assert_eq!(bits(&a.a), bits(&b.a), "{what}: values");
    assert_eq!(a.matrix_type, b.matrix_type, "{what}: type");
    assert_eq!(a.size, b.size, "{what}: size");
    assert_eq!(a.value_release, b.value_release, "{what}: value release");
    assert_eq!(
        a.profile_release, b.profile_release,
        "{what}: profile release"
    );
}

fn same_result(a: &Result<()>, b: &Result<()>, what: &str) {
    match (a, b) {
        (Ok(()), Ok(())) => {}
        (Err(x), Err(y)) => assert_eq!(x.to_string(), y.to_string(), "{what}"),
        _ => panic!("{what}: {a:?} vs {b:?}"),
    }
}

/// One element block of `nb` equations (some `>= size`, some repeated).
fn element_of(rng: &mut Rng, n: GlobalIdx, nb: usize) -> Vec<GlobalIdx> {
    (0..nb)
        .map(|_| {
            let g = rng.below(usize::try_from(n).unwrap() + 3);
            GlobalIdx::try_from(g).unwrap()
        })
        .collect()
}

/// `add_values_cached` against `add_values` over several assembly rounds,
/// for every (matrix type, block type, policy) combination, with changed
/// slot equations, profile changes, missing entries and resizes between
/// rounds.
#[test]
fn cached_element_assembly_is_add_values() {
    let combos = [
        (
            MatrixType::Symmetric,
            MatrixType::Symmetric,
            SymmetryPolicy::PromoteOnUnsymmetric,
        ),
        (
            MatrixType::Symmetric,
            MatrixType::Unsymmetric,
            SymmetryPolicy::UpstreamUpperSelection,
        ),
        (
            MatrixType::Symmetric,
            MatrixType::Unsymmetric,
            SymmetryPolicy::PromoteOnUnsymmetric,
        ),
        (
            MatrixType::Unsymmetric,
            MatrixType::Symmetric,
            SymmetryPolicy::PromoteOnUnsymmetric,
        ),
        (
            MatrixType::Unsymmetric,
            MatrixType::Unsymmetric,
            SymmetryPolicy::PromoteOnUnsymmetric,
        ),
    ];
    for (seed, &(mt, vt, policy)) in combos.iter().enumerate() {
        let mut stored = false;
        for trial in 0..20u64 {
            // half of the trials without special values (exact detection
            // of a wrong position), half with them
            let special = if trial % 4 < 2 { 0 } else { 30 };
            let mut rng = Rng(
                (0x9e37_79b9 ^ ((seed as u64) << 32) ^ trial.wrapping_mul(7919)) | 1,
                special,
            );
            let n: GlobalIdx = 6 + GlobalIdx::try_from(rng.below(10)).unwrap();
            // one block size per trial (one element type), 1 to 5 equations
            let nb = 1 + (trial as usize / 4) % 5;
            let mut elems: Vec<Vec<GlobalIdx>> =
                (0..8).map(|_| element_of(&mut rng, n, nb)).collect();
            let mut plain = SparseMatrixAij::new("K", n, mt);
            plain.set_symmetry_policy(policy);
            // profile every element but (sometimes) the last one
            let profiled = if rng.below(3) == 0 {
                elems.len() - 1
            } else {
                elems.len()
            };
            for e in &elems[..profiled] {
                plain.add_block_to_profile(e, e).unwrap();
            }
            let mut cached = plain.clone();
            cached.profile_release = plain.profile_release;
            let mut cache = BlockPositionCache::new();
            for round in 0..6 {
                match round {
                    3 => {
                        // a slot's equations change (re-resolved)
                        let k = rng.below(profiled);
                        elems[k] = elems[(k + 1) % profiled].clone();
                    }
                    4 if trial % 2 == 0 => {
                        // an appended entry (existing positions stay valid)
                        let (i, j) = (rng.below(3) as GlobalIdx, rng.below(3) as GlobalIdx);
                        plain.add(i, j, 0.25).unwrap();
                        cached.add(i, j, 0.25).unwrap();
                    }
                    4 => {
                        // the profile rebuilt in another order: every
                        // position may move
                        for m in [&mut plain, &mut cached] {
                            m.clear_profile();
                            for e in elems[..profiled].iter().rev() {
                                m.add_block_to_profile(e, e).unwrap();
                            }
                        }
                    }
                    5 => {
                        plain.set_size(n - 1);
                        cached.set_size(n - 1);
                    }
                    _ => {}
                }
                cache.begin(&cached);
                for (slot, e) in elems.iter().enumerate() {
                    let values: Vec<Real> = (0..e.len() * e.len()).map(|_| rng.value()).collect();
                    let r1 = plain.add_values(e, e, &values, vt);
                    let r2 = cached.add_values_cached(&mut cache, slot, e, e, &values, vt);
                    let what =
                        format!("{mt:?}/{vt:?}/{policy:?} trial {trial} round {round} slot {slot}");
                    same_result(&r1, &r2, &what);
                    same_state(&plain, &cached, &what);
                }
                stored |= cache.memory_bytes() > 0;
            }
        }
        assert!(stored, "{mt:?}/{vt:?}/{policy:?}: the cache was never used");
    }
}

/// Rectangular blocks (rows ≠ columns) into an unsymmetric matrix, and a
/// slot whose rows/columns layout changes, use the cache correctly.
#[test]
fn cached_rectangular_blocks() {
    let mut rng = Rng(42, 30);
    let n = 9;
    let mut plain = SparseMatrixAij::new("K", n, MatrixType::Unsymmetric);
    let blocks: Vec<(Vec<GlobalIdx>, Vec<GlobalIdx>)> = (0..6)
        .map(|_| (element_of(&mut rng, n, 3), element_of(&mut rng, n, 2)))
        .collect();
    for (r, c) in &blocks {
        plain.add_block_to_profile(r, c).unwrap();
        plain.add_block_to_profile(r, r).unwrap();
    }
    let mut cached = plain.clone();
    let mut cache = BlockPositionCache::new();
    for round in 0..4 {
        cache.begin(&cached);
        for (slot, (r, c)) in blocks.iter().enumerate() {
            let c = if round == 2 { r } else { c };
            let values: Vec<Real> = (0..r.len() * c.len()).map(|_| rng.value()).collect();
            let r1 = plain.add_values(r, c, &values, MatrixType::Unsymmetric);
            let r2 =
                cached.add_values_cached(&mut cache, slot, r, c, &values, MatrixType::Unsymmetric);
            same_result(&r1, &r2, "rectangular");
            same_state(&plain, &cached, "rectangular");
        }
    }
    assert!(
        cache.memory_bytes() > 0,
        "positions stored from the second round"
    );
}

/// The matrix-vector product of the layout-mapped loop it replaces.
fn reference_mat_vec(
    m: &SparseMatrixAij,
    layout: &EquationLayout,
    x: &[Real],
    y: &mut [Real],
    alpha: Real,
    beta: Real,
) -> Result<()> {
    let mut locals = Vec::new();
    for (&gi, &gj) in m.irn.iter().zip(&m.jcn) {
        locals.push((layout.local(gi)?, layout.local(gj)?));
    }
    let symmetric = m.matrix_type == MatrixType::Symmetric;
    for yi in y.iter_mut() {
        *yi *= beta;
    }
    for (&(i, j), &a) in locals.iter().zip(&m.a) {
        y[i] += alpha * a * x[j];
        if symmetric && i != j {
            y[j] += alpha * a * x[i];
        }
    }
    for yi in y.iter_mut() {
        *yi += 0.0;
    }
    Ok(())
}

fn reference_apply_boundary(
    m: &mut SparseMatrixAij,
    layout: &EquationLayout,
    blocked: &[bool],
    block_val: Real,
) -> Result<()> {
    let mut locals = Vec::new();
    for (&gi, &gj) in m.irn.iter().zip(&m.jcn) {
        locals.push((layout.local(gi)?, layout.local(gj)?));
    }
    for ((li, lj), (a, (&gi, &gj))) in locals
        .into_iter()
        .zip(m.a.iter_mut().zip(m.irn.iter().zip(&m.jcn)))
    {
        if blocked[li] || blocked[lj] {
            *a = if gi != gj {
                0.0
            } else if layout.is_owned(li) {
                block_val
            } else {
                0.0
            };
        }
    }
    m.bump_values(1);
    Ok(())
}

fn random_matrix(rng: &mut Rng, n: GlobalIdx, ty: MatrixType, nnz: usize) -> SparseMatrixAij {
    let mut m = SparseMatrixAij::new("A", n, ty);
    let nu = usize::try_from(n).unwrap();
    for _ in 0..nnz {
        let (i, j) = (rng.below(nu) as GlobalIdx, rng.below(nu) as GlobalIdx);
        let v = rng.value();
        m.add(i, j, v).unwrap();
    }
    m
}

/// A serial identity layout and a layout with shared and pure-ghost
/// equations (general path): products and Dirichlet treatment as the
/// reference loops, including the error of an absent coordinate.
#[test]
fn mat_vec_and_boundary_match_the_reference() {
    use EquationOwnership::{Owned, PureGhost, Shared};
    let identity = EquationLayout::serial(12).unwrap();
    assert!(identity.is_identity());
    let mut general = EquationLayout::new();
    let own: Vec<EquationOwnership> = (0..14)
        .map(|l| match l {
            3 | 9 => Shared,
            12 => PureGhost,
            _ => Owned,
        })
        .collect();
    // 11 owned equations numbered 0..11; the shared copies alias 2 and 5
    general
        .append_block(&own, 0, 11, |i| match i {
            3 => Some(2),
            9 => Some(5),
            _ => None,
        })
        .unwrap();
    assert!(!general.is_identity());
    for (layout, n) in [(&identity, 12), (&general, 11)] {
        for trial in 0..40u64 {
            let mut rng = Rng(trial * 31 + 7, 30);
            let ty = if trial % 2 == 0 {
                MatrixType::Symmetric
            } else {
                MatrixType::Unsymmetric
            };
            let m = random_matrix(&mut rng, n, ty, 40);
            let nl = layout.nb_local();
            let x: Vec<Real> = (0..nl).map(|_| rng.value()).collect();
            let y0: Vec<Real> = (0..nl).map(|_| rng.value()).collect();
            let (alpha, beta) = (rng.value(), rng.value());
            let (mut y1, mut y2) = (y0.clone(), y0.clone());
            reference_mat_vec(&m, layout, &x, &mut y1, alpha, beta).unwrap();
            m.mat_vec_mul(layout, &NoReduction, &x, &mut y2, alpha, beta)
                .unwrap();
            assert_eq!(bits(&y1), bits(&y2), "mat_vec trial {trial}");

            let blocked: Vec<bool> = (0..nl).map(|_| rng.below(3) == 0).collect();
            let (mut m1, mut m2) = (m.clone(), m.clone());
            m2.value_release = m1.value_release;
            m2.profile_release = m1.profile_release;
            let bv = rng.value();
            reference_apply_boundary(&mut m1, layout, &blocked, bv).unwrap();
            m2.apply_boundary(layout, &blocked, bv).unwrap();
            same_state(&m1, &m2, "apply_boundary");
        }
    }
    // a coordinate absent from the layout: the same error, nothing changed
    let mut m = SparseMatrixAij::new("A", 20, MatrixType::Unsymmetric);
    m.add(1, 2, 1.0).unwrap();
    m.add(3, 15, 1.0).unwrap();
    m.add(17, 0, 1.0).unwrap();
    for layout in [&identity, &general] {
        let nl = layout.nb_local();
        let x = vec![1.0; nl];
        let (mut y1, mut y2) = (vec![0.5; nl], vec![0.5; nl]);
        let r1 = reference_mat_vec(&m, layout, &x, &mut y1, 1.0, 1.0);
        let r2 = m.mat_vec_mul(layout, &NoReduction, &x, &mut y2, 1.0, 1.0);
        same_result(&r1, &r2, "absent coordinate");
        assert_eq!(bits(&y2), bits(&[0.5; 14][..nl]), "y untouched");
        let mut m2 = m.clone();
        let before = bits(&m2.a);
        let r = m2.apply_boundary(layout, &vec![true; nl], 1.0);
        let r1: Result<()> = layout.local(15).map(drop);
        same_result(&r1, &r, "absent coordinate (boundary)");
        assert_eq!(bits(&m2.a), before);
    }
}

/// `add_matrix` / `add_matrix_upstream` with equal entry layouts (the
/// entry-wise path) give the result of `add` entry by entry; so do
/// copies with another key rule (duplicate keys: the general path).
#[test]
fn matrix_sums_match_entry_by_entry_adds() {
    let types = [MatrixType::Symmetric, MatrixType::Unsymmetric];
    for trial in 0..30u64 {
        for &ts in &types {
            for &td in &types {
                let mut rng = Rng(trial * 977 + 3, 30);
                let src = random_matrix(&mut rng, 10, ts, 30);
                let mut dst = SparseMatrixAij::new("J", 10, td);
                dst.copy_profile(&src);
                // some values already in dst
                for k in 0..dst.a.len() {
                    dst.a[k] = rng.value();
                }
                let alpha = rng.value();
                let mirror = ts == MatrixType::Symmetric && td != MatrixType::Symmetric;
                for upstream in [false, true] {
                    let mut fast = dst.clone();
                    fast.value_release = dst.value_release;
                    fast.profile_release = dst.profile_release;
                    let mut slow = fast.clone();
                    slow.value_release = fast.value_release;
                    slow.profile_release = fast.profile_release;
                    for ((&i, &j), &v) in src.irn.iter().zip(&src.jcn).zip(&src.a) {
                        let s = alpha * v;
                        slow.add(i, j, s).unwrap();
                        if !upstream && mirror && i != j {
                            slow.add(j, i, s).unwrap();
                        }
                    }
                    if upstream {
                        fast.add_matrix_upstream(&src, alpha).unwrap();
                    } else {
                        fast.add_matrix(&src, alpha).unwrap();
                    }
                    same_state(
                        &slow,
                        &fast,
                        &format!("{ts:?} into {td:?}, upstream {upstream}"),
                    );
                }
            }
        }
    }
}

/// `copy_profile` (map reused, map cloned, map rebuilt) equals the
/// rebuild from the copied coordinates, lookups included.
#[test]
fn copy_profile_matches_the_rebuild() {
    let types = [MatrixType::Symmetric, MatrixType::Unsymmetric];
    for trial in 0..30u64 {
        for &ts in &types {
            for &td in &types {
                let mut rng = Rng(trial * 131 + 5, 30);
                let src = random_matrix(&mut rng, 9, ts, 25);
                let mut dst = random_matrix(&mut rng, 9, td, 10);
                for twice in 0..2 {
                    // reference: the rebuild of the copied coordinates
                    let mut reference = dst.clone();
                    reference.irn.clone_from(&src.irn);
                    reference.jcn.clone_from(&src.jcn);
                    reference.profile.clear();
                    for k in 0..reference.irn.len() {
                        let key = reference.key(reference.irn[k], reference.jcn[k]);
                        reference.profile.insert(key, k);
                    }
                    reference.a = vec![0.0; reference.irn.len()];
                    reference.size = src.size;
                    reference.profile_release = src.profile_release;
                    reference.value_release = dst.value_release.wrapping_add(1);
                    dst.copy_profile(&src);
                    same_state(&reference, &dst, &format!("{ts:?} → {td:?} ({twice})"));
                    assert_eq!(reference.profile, dst.profile, "lookup map");
                    for i in 0..9 {
                        for j in 0..9 {
                            assert_eq!(reference.position(i, j), dst.position(i, j));
                        }
                    }
                    dst.a.iter_mut().for_each(|v| *v = 1.0);
                }
            }
        }
    }
}
