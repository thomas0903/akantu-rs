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
                    4 if trial % 3 == 0 => {
                        // an appended entry (existing positions stay valid)
                        let (i, j) = (rng.below(3) as GlobalIdx, rng.below(3) as GlobalIdx);
                        plain.add(i, j, 0.25).unwrap();
                        cached.add(i, j, 0.25).unwrap();
                    }
                    4 if trial % 3 == 2 => {
                        // `copy_profile` from a matrix that stores the
                        // same entries in another order: every position
                        // may move
                        let mut other = SparseMatrixAij::new("O", n, cached.matrix_type);
                        for e in elems[..profiled].iter().rev() {
                            other.add_block_to_profile(e, e).unwrap();
                        }
                        plain.copy_profile(&other);
                        cached.copy_profile(&other);
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
    // the boundary of the identity check: a stored coordinate equal to
    // `nb_local` (the largest stored coordinate is `nb_local`) is an error,
    // not an out-of-bounds index
    let nl = identity.nb_local();
    let edge = GlobalIdx::try_from(nl).unwrap();
    let mut m = SparseMatrixAij::new("A", edge + 1, MatrixType::Symmetric);
    m.add(0, 1, 1.0).unwrap();
    m.add(3, edge, 2.0).unwrap();
    let x = vec![1.0; nl];
    let mut y = vec![0.5; nl];
    let r = m.mat_vec_mul(&identity, &NoReduction, &x, &mut y, 1.0, 1.0);
    same_result(&identity.local(edge).map(drop), &r, "coordinate = nb_local");
    assert_eq!(bits(&y), bits(&vec![0.5; nl]), "y untouched");
    let before = bits(&m.a);
    let r = m.apply_boundary(&identity, &vec![true; nl], 1.0);
    same_result(
        &identity.local(edge).map(drop),
        &r,
        "coordinate = nb_local (boundary)",
    );
    assert_eq!(bits(&m.a), before);

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

/// Profiling every element and then adding every element's values gives
/// the state of the fused `add_block_to_profile_and_values` per element
/// (no promotion possible); the promoting combination is refused without
/// a change.
#[test]
fn fused_first_assembly_is_profile_then_values() {
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
    for &(mt, vt, policy) in &combos {
        for trial in 0..40u64 {
            let mut rng = Rng(trial * 7 + 11, if trial % 2 == 0 { 0 } else { 30 });
            let n: GlobalIdx = 6 + GlobalIdx::try_from(rng.below(10)).unwrap();
            let nb = 1 + (trial as usize) % 5;
            let elems: Vec<Vec<GlobalIdx>> = (0..10).map(|_| element_of(&mut rng, n, nb)).collect();
            let values: Vec<Vec<Real>> = elems
                .iter()
                .map(|e| (0..e.len() * e.len()).map(|_| rng.value()).collect())
                .collect();
            let mut two_pass = SparseMatrixAij::new("K", n, mt);
            two_pass.set_symmetry_policy(policy);
            if trial % 3 == 0 {
                // a profile that already holds entries
                two_pass.add(1, 2, 0.5).unwrap();
                two_pass.add(3, 3, -0.0).unwrap();
            }
            let mut fused = two_pass.clone();
            fused.value_release = two_pass.value_release;
            fused.profile_release = two_pass.profile_release;
            for e in &elems {
                two_pass.add_block_to_profile(e, e).unwrap();
            }
            for (e, v) in elems.iter().zip(&values) {
                two_pass.add_values(e, e, v, vt).unwrap();
            }
            for (e, v) in elems.iter().zip(&values) {
                fused.add_block_to_profile_and_values(e, e, v, vt).unwrap();
            }
            same_state(
                &two_pass,
                &fused,
                &format!("{mt:?}/{vt:?}/{policy:?} trial {trial}"),
            );
            assert_eq!(two_pass.profile, fused.profile);
        }
    }
    let mut m = SparseMatrixAij::new("K", 4, MatrixType::Symmetric);
    let before = m.clone();
    assert!(
        m.add_block_to_profile_and_values(&[0, 1], &[0, 1], &[1.0; 4], MatrixType::Unsymmetric)
            .is_err()
    );
    assert_eq!(
        (m.nnz(), m.value_release),
        (before.nnz(), before.value_release)
    );
}

/// `CscMatrix::from_aij` (no triplet vector) equals `from_triplets` of the
/// triplets upstream's `setA` builds (storage order, mirror after each
/// off-diagonal entry of symmetric storage), bit for bit; duplicate
/// coordinates (a copied profile with a collapsed key rule) included.
#[test]
fn csc_from_aij_matches_the_triplet_path() {
    use crate::direct::CscMatrix;
    for trial in 0..60u64 {
        let mut rng = Rng(trial * 13 + 1, if trial % 2 == 0 { 0 } else { 30 });
        let ty = if trial % 3 == 0 {
            MatrixType::Unsymmetric
        } else {
            MatrixType::Symmetric
        };
        let mut m = random_matrix(&mut rng, 12, ty, 50);
        if trial % 5 == 0 {
            // duplicate coordinates: an unsymmetric profile copied into a
            // symmetric matrix keeps both (i, j) and (j, i)
            let src = random_matrix(&mut rng, 12, MatrixType::Unsymmetric, 50);
            let mut d = SparseMatrixAij::new("D", 12, MatrixType::Symmetric);
            d.copy_profile(&src);
            for k in 0..d.a.len() {
                d.a[k] = rng.value();
            }
            m = d;
        }
        let n = usize::try_from(m.size()).unwrap();
        let mut triplets = Vec::new();
        for ((&i, &j), &v) in m.irn.iter().zip(&m.jcn).zip(&m.a) {
            let (iu, ju) = (i as usize, j as usize);
            triplets.push((iu, ju, v));
            if m.matrix_type == MatrixType::Symmetric && i != j {
                triplets.push((ju, iu, v));
            }
        }
        let a = CscMatrix::from_aij(&m).unwrap();
        let b = CscMatrix::from_triplets(n, &triplets).unwrap();
        assert_eq!(a.col_ptr(), b.col_ptr(), "trial {trial}");
        assert_eq!(a.row_idx(), b.row_idx(), "trial {trial}");
        assert_eq!(bits(a.values()), bits(b.values()), "trial {trial}");
    }
    // a coordinate outside the order: the error of the triplet path's check
    let mut m = SparseMatrixAij::new("A", 4, MatrixType::Unsymmetric);
    m.add(1, 3, 1.0).unwrap();
    m.set_size(3);
    let e = CscMatrix::from_aij(&m).unwrap_err().to_string();
    assert!(e.contains("coordinate 3 outside the matrix order 3"), "{e}");
}

/// Executors of 1, 2, 3, 4 and 8 threads with the finest split (every
/// batch of more than one item is cut into the largest number of parts).
fn execs() -> Vec<akantu_core::exec::Exec> {
    use akantu_core::exec::{Exec, ExecPolicy, Grain};
    [1, 2, 3, 4, 8]
        .into_iter()
        .map(|n| {
            Exec::new(
                ExecPolicy::serial()
                    .with_threads(n)
                    .unwrap()
                    .with_grain(Grain::Finest),
            )
            .unwrap()
        })
        .collect()
}

/// `add_values_cached_batch` (or, when it declines, the serial loop the
/// caller then runs) against `add_values` block by block over several
/// assembly rounds at 1, 2, 3, 4 and 8 threads: values, errors, releases
/// and the cache's resolve counter, for every (matrix type, block type,
/// policy) combination, all slots or a subset (one material of several),
/// a repeated slot, changed slot equations, appended entries, moved
/// positions, missing entries and resizes between rounds.
#[test]
fn threaded_batch_assembly_is_add_values() {
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
    let execs = execs();
    let mut batched = 0usize;
    for (seed, &(mt, vt, policy)) in combos.iter().enumerate() {
        for trial in 0..12u64 {
            let special = if trial % 4 < 2 { 0 } else { 30 };
            let mut rng = Rng(
                (0x51ed_270b ^ ((seed as u64) << 32) ^ trial.wrapping_mul(104_729)) | 1,
                special,
            );
            let n: GlobalIdx = 20 + GlobalIdx::try_from(rng.below(60)).unwrap();
            let nb = 1 + (trial as usize / 2) % 5;
            let nb_elem = 40 + rng.below(40);
            let elems: Vec<Vec<GlobalIdx>> =
                (0..nb_elem).map(|_| element_of(&mut rng, n, nb)).collect();
            let profiled = if rng.below(4) == 0 {
                nb_elem - 1
            } else {
                nb_elem
            };
            // the slots of the batch: all, every other one (a material),
            // or with a repeated slot
            let slots: Vec<usize> = match trial % 3 {
                0 => (0..nb_elem).collect(),
                1 => (0..nb_elem).filter(|s| s % 2 == 1).collect(),
                _ => (0..nb_elem).chain([3]).collect(),
            };
            let mut base = SparseMatrixAij::new("K", n, mt);
            base.set_symmetry_policy(policy);
            for e in &elems[..profiled] {
                base.add_block_to_profile(e, e).unwrap();
            }
            for exec in &execs {
                let mut plain = base.clone();
                plain.profile_release = base.profile_release;
                let mut threaded = plain.clone();
                threaded.profile_release = plain.profile_release;
                let mut elems = elems.clone();
                let mut cache = BlockPositionCache::new();
                let mut rng = Rng(rng.0 ^ 0x1234_5678, special);
                for round in 0..7 {
                    match round {
                        4 => {
                            let k = rng.below(profiled);
                            elems[k] = elems[(k + 1) % profiled].clone();
                        }
                        5 if trial % 2 == 0 => {
                            let (i, j) = (rng.below(3) as GlobalIdx, rng.below(3) as GlobalIdx);
                            plain.add(i, j, 0.25).unwrap();
                            threaded.add(i, j, 0.25).unwrap();
                        }
                        5 => {
                            for m in [&mut plain, &mut threaded] {
                                m.clear_profile();
                                for e in elems[..profiled].iter().rev() {
                                    m.add_block_to_profile(e, e).unwrap();
                                }
                            }
                        }
                        6 if trial % 4 == 1 => {
                            plain.set_size(n - 1);
                            threaded.set_size(n - 1);
                        }
                        _ => {}
                    }
                    let values: Vec<Real> =
                        (0..slots.len() * nb * nb).map(|_| rng.value()).collect();
                    let what = format!(
                        "{mt:?}/{vt:?}/{policy:?} trial {trial} round {round} threads {}",
                        exec.threads()
                    );
                    let mut r1 = Ok(());
                    for (k, &s) in slots.iter().enumerate() {
                        r1 = plain.add_values(
                            &elems[s],
                            &elems[s],
                            &values[k * nb * nb..(k + 1) * nb * nb],
                            vt,
                        );
                        if r1.is_err() {
                            break;
                        }
                    }
                    cache.begin(&threaded);
                    let resolves = cache.resolves();
                    let eqs = |k: usize, out: &mut Vec<GlobalIdx>| -> Result<()> {
                        out.clear();
                        out.extend_from_slice(&elems[slots[k]]);
                        Ok(())
                    };
                    let mut r2 = Ok(());
                    if threaded
                        .add_values_cached_batch(exec, &mut cache, &slots, nb, &eqs, &values, vt)
                    {
                        batched += 1;
                    } else {
                        // the caller's serial loop
                        let mut serial_cache = cache.clone();
                        assert_eq!(serial_cache.resolves(), resolves, "{what}");
                        for (k, &s) in slots.iter().enumerate() {
                            r2 = threaded.add_values_cached(
                                &mut serial_cache,
                                s,
                                &elems[s],
                                &elems[s],
                                &values[k * nb * nb..(k + 1) * nb * nb],
                                vt,
                            );
                            if r2.is_err() {
                                break;
                            }
                        }
                        cache = serial_cache;
                    }
                    same_result(&r1, &r2, &what);
                    same_state(&plain, &threaded, &what);
                }
            }
        }
    }
    assert!(batched > 100, "the threaded batch ran {batched} times");
}

/// The resolve counter and the stored slots after a threaded batch are
/// those of the serial loop (the batch resolves what the loop resolves).
#[test]
fn threaded_batch_resolves_like_the_serial_loop() {
    let mut rng = Rng(7, 0);
    let n = 50;
    let elems: Vec<Vec<GlobalIdx>> = (0..64).map(|_| element_of(&mut rng, n - 3, 4)).collect();
    let mut m = SparseMatrixAij::new("K", n, MatrixType::Symmetric);
    for e in &elems {
        m.add_block_to_profile(e, e).unwrap();
    }
    let slots: Vec<usize> = (0..elems.len()).collect();
    let values: Vec<Real> = (0..elems.len() * 16).map(|_| rng.value()).collect();
    let eqs = |k: usize, out: &mut Vec<GlobalIdx>| -> Result<()> {
        out.clear();
        out.extend_from_slice(&elems[k]);
        Ok(())
    };
    let exec = &execs()[3];
    let mut serial = m.clone();
    let mut threaded = m.clone();
    let (mut cs, mut ct) = (BlockPositionCache::new(), BlockPositionCache::new());
    for round in 0..3 {
        cs.begin(&serial);
        for (k, e) in elems.iter().enumerate() {
            serial
                .add_values_cached(
                    &mut cs,
                    k,
                    e,
                    e,
                    &values[k * 16..(k + 1) * 16],
                    MatrixType::Symmetric,
                )
                .unwrap();
        }
        ct.begin(&threaded);
        let done = threaded.add_values_cached_batch(
            exec,
            &mut ct,
            &slots,
            4,
            &eqs,
            &values,
            MatrixType::Symmetric,
        );
        if !done {
            for (k, e) in elems.iter().enumerate() {
                threaded
                    .add_values_cached(
                        &mut ct,
                        k,
                        e,
                        e,
                        &values[k * 16..(k + 1) * 16],
                        MatrixType::Symmetric,
                    )
                    .unwrap();
            }
        }
        assert_eq!(done, round > 0, "round {round}");
        assert_eq!(cs.resolves(), ct.resolves(), "round {round}");
        assert_eq!(cs.positions, ct.positions, "round {round}");
        assert_eq!(cs.equations, ct.equations, "round {round}");
        assert_eq!(bits(&serial.a), bits(&threaded.a), "round {round}");
        assert_eq!(
            serial.value_release, threaded.value_release,
            "round {round}"
        );
    }
}

/// A reduction that reports a distributed vector and changes nothing (the
/// distributed branch of the product on one process).
struct DistributedNoop;

impl PartialSumReduction for DistributedNoop {
    fn is_distributed(&self) -> bool {
        true
    }
    fn reduce_and_synchronize(&self, local: &mut [f64]) -> Result<()> {
        for v in local.iter_mut() {
            *v *= 1.0;
        }
        Ok(())
    }
}

/// The whole-matrix operations on threads (`*_with`, `*_on`) give the bits
/// and releases of their serial forms at 1, 2, 3, 4 and 8 threads:
/// products (symmetric and unsymmetric storage, both reductions, with an
/// appended entry after a first product), Dirichlet rows, matrix sums,
/// profile copies, scaling and zeroing.
#[test]
fn threaded_matrix_operations_match_the_serial_ones() {
    let execs = execs();
    for (seed, ty) in [MatrixType::Symmetric, MatrixType::Unsymmetric]
        .into_iter()
        .enumerate()
    {
        for trial in 0..6u64 {
            let special = if trial % 2 == 0 { 0 } else { 20 };
            let mut rng = Rng(
                (0xabcd_ef01 ^ (seed as u64) << 40 ^ trial << 8) | 1,
                special,
            );
            let n: GlobalIdx = 30 + GlobalIdx::try_from(rng.below(40)).unwrap();
            let nu = usize::try_from(n).unwrap();
            let layout = EquationLayout::serial(nu).unwrap();
            let nnz = 3 * nu + rng.below(4 * nu);
            let base = random_matrix(&mut rng, n, ty, nnz);
            let mut other = SparseMatrixAij::new("B", n, ty);
            other.copy_profile(&base);
            for v in other.a.iter_mut() {
                *v = rng.value();
            }
            let x: Vec<Real> = (0..nu).map(|_| rng.value()).collect();
            let y0: Vec<Real> = (0..nu).map(|_| rng.value()).collect();
            let blocked: Vec<bool> = (0..nu).map(|_| rng.below(5) == 0).collect();
            let (alpha, beta) = (rng.value(), rng.value());
            for exec in &execs {
                let what = format!("{ty:?} trial {trial} threads {}", exec.threads());
                for red in [&NoReduction as &dyn PartialSumReduction, &DistributedNoop] {
                    let mut m = base.clone();
                    for round in 0..2 {
                        if round == 1 {
                            // an appended entry: the gathered rows are rebuilt
                            let (i, j) = (rng.below(nu) as GlobalIdx, rng.below(nu) as GlobalIdx);
                            m.add(i, j, 0.75).unwrap();
                        }
                        let (mut y1, mut y2) = (y0.clone(), y0.clone());
                        let r1 = m.mat_vec_mul(&layout, red, &x, &mut y1, alpha, beta);
                        let r2 = m.mat_vec_mul_with(exec, &layout, red, &x, &mut y2, alpha, beta);
                        same_result(&r1, &r2, &what);
                        assert_eq!(bits(&y1), bits(&y2), "{what}: product round {round}");
                    }
                }
                let (mut a, mut b) = (base.clone(), base.clone());
                let r1 = a.apply_boundary(&layout, &blocked, 1.0);
                let r2 = b.apply_boundary_on(exec, &layout, &blocked, 1.0);
                same_result(&r1, &r2, &what);
                same_state(&a, &b, &format!("{what}: boundary"));
                let (mut a, mut b) = (other.clone(), other.clone());
                a.add_matrix(&base, alpha).unwrap();
                b.add_matrix_with(exec, &base, alpha).unwrap();
                same_state(&a, &b, &format!("{what}: add_matrix"));
                let (mut a, mut b) = (other.clone(), other.clone());
                a.add_matrix_upstream(&base, alpha).unwrap();
                b.add_matrix_upstream_with(exec, &base, alpha).unwrap();
                same_state(&a, &b, &format!("{what}: add_matrix_upstream"));
                let (mut a, mut b) = (other.clone(), other.clone());
                a.copy_profile(&base);
                b.copy_profile_with(exec, &base);
                same_state(&a, &b, &format!("{what}: copy_profile"));
                let (mut a, mut b) = (other.clone(), other.clone());
                a.mul(beta);
                b.mul_with(exec, beta);
                same_state(&a, &b, &format!("{what}: mul"));
                a.zero();
                b.zero_with(exec);
                same_state(&a, &b, &format!("{what}: zero"));
            }
        }
    }
}

/// A profile rebuilt with the same number of entries in another order
/// between two threaded products: the row index of the first profile must
/// not be reused (its key holds the entry positions' identity), so the
/// second product equals the serial one bit for bit.
#[test]
fn threaded_product_after_a_reordered_profile_of_equal_size() {
    let execs = execs();
    for ty in [MatrixType::Symmetric, MatrixType::Unsymmetric] {
        let mut rng = Rng(0x0dd_ba11 | 1, 0);
        let n: GlobalIdx = 40;
        let nu = usize::try_from(n).unwrap();
        let layout = EquationLayout::serial(nu).unwrap();
        let base = random_matrix(&mut rng, n, ty, 6 * nu);
        let x: Vec<Real> = (0..nu).map(|_| rng.value()).collect();
        let y0: Vec<Real> = (0..nu).map(|_| rng.value()).collect();
        for exec in &execs {
            let mut m = base.clone();
            let mut y = y0.clone();
            m.mat_vec_mul_with(exec, &layout, &NoReduction, &x, &mut y, 1.25, 0.5)
                .unwrap();
            // the same entries in reverse order, new values
            let (irn, jcn) = (m.irn.clone(), m.jcn.clone());
            let nnz = irn.len();
            m.clear_profile();
            for (&i, &j) in irn.iter().zip(&jcn).rev() {
                let v = rng.value();
                m.add(i, j, v).unwrap();
            }
            assert_eq!(m.a.len(), nnz, "same number of entries");
            let (mut y1, mut y2) = (y0.clone(), y0.clone());
            m.mat_vec_mul(&layout, &NoReduction, &x, &mut y1, 1.25, 0.5)
                .unwrap();
            m.mat_vec_mul_with(exec, &layout, &NoReduction, &x, &mut y2, 1.25, 0.5)
                .unwrap();
            assert_eq!(bits(&y1), bits(&y2), "{ty:?} threads {}", exec.threads());
        }
    }
}

/// The scatter plan follows the cache: a caller that vouches for the
/// equations with a key gets the planned scatter, and the plan is rebuilt
/// when the key changes (equations changed), when the profile grows, when
/// it is rebuilt in another order, and when the batch's slots change (one
/// material of two): every round bit for bit `add_values`, at 1, 2, 3, 4
/// and 8 threads, finest and automatic grain.
#[test]
fn threaded_batch_plan_follows_the_cache() {
    use akantu_core::exec::{Exec, ExecPolicy, Grain};
    let mut execs = Vec::new();
    for n in [1, 2, 3, 4, 8] {
        for g in [Grain::Finest, Grain::Auto] {
            execs.push(
                Exec::new(ExecPolicy::serial().with_threads(n).unwrap().with_grain(g)).unwrap(),
            );
        }
    }
    for ty in [MatrixType::Symmetric, MatrixType::Unsymmetric] {
        for exec in &execs {
            let mut rng = Rng(0x5eed_f00d | 1, 0);
            let n: GlobalIdx = 60;
            let nb = 4;
            let mut elems: Vec<Vec<GlobalIdx>> =
                (0..120).map(|_| element_of(&mut rng, n - 3, nb)).collect();
            let mut plain = SparseMatrixAij::new("K", n, ty);
            for e in &elems {
                plain.add_block_to_profile(e, e).unwrap();
            }
            let mut planned = plain.clone();
            let mut cache = BlockPositionCache::new();
            let all: Vec<usize> = (0..elems.len()).collect();
            let odd: Vec<usize> = all.iter().copied().filter(|k| k % 2 == 1).collect();
            let mut key = [1, 0, 0, 0, 0];
            for round in 0..9 {
                let slots: &[usize] = if round == 7 { &odd } else { &all };
                match round {
                    3 => {
                        // equations changed: the caller's key changes
                        elems.swap(4, 9);
                        elems[11].reverse();
                        key[0] += 1;
                    }
                    4 => {
                        // the profile grows (appended entries)
                        plain.add(1, n - 1, 0.5).unwrap();
                        planned.add(1, n - 1, 0.5).unwrap();
                    }
                    6 => {
                        // the profile rebuilt in another order
                        for m in [&mut plain, &mut planned] {
                            m.clear_profile();
                            for e in elems.iter().rev() {
                                m.add_block_to_profile(e, e).unwrap();
                            }
                        }
                    }
                    _ => {}
                }
                let values: Vec<Real> = (0..slots.len() * nb * nb).map(|_| rng.value()).collect();
                for (k, &s) in slots.iter().enumerate() {
                    plain
                        .add_values(
                            &elems[s],
                            &elems[s],
                            &values[k * nb * nb..(k + 1) * nb * nb],
                            ty,
                        )
                        .unwrap();
                }
                cache.begin(&planned);
                cache.vouch_equations(key);
                let eqs = |k: usize, out: &mut Vec<GlobalIdx>| -> Result<()> {
                    out.clear();
                    out.extend_from_slice(&elems[slots[k]]);
                    Ok(())
                };
                if !planned.add_values_cached_batch(exec, &mut cache, slots, nb, &eqs, &values, ty)
                {
                    for (k, &s) in slots.iter().enumerate() {
                        planned
                            .add_values_cached(
                                &mut cache,
                                s,
                                &elems[s],
                                &elems[s],
                                &values[k * nb * nb..(k + 1) * nb * nb],
                                ty,
                            )
                            .unwrap();
                    }
                }
                same_state(
                    &plain,
                    &planned,
                    &format!(
                        "{ty:?} threads {} {:?} round {round}",
                        exec.threads(),
                        exec.policy().grain()
                    ),
                );
            }
        }
    }
}
