// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): the
// allocation-free compile-time eigen paths used per quadrature point by the
// anisotropic damage law (src/model/solid_mechanics/materials/
// material_damage/material_anisotropic_damage_tmpl.hh: `A.eig(A_eigs)` and
// `A.eig(A_eigs, A_directions)` on `Matrix<Real, dim, dim>`;
// src/common/aka_types.hh `MatrixBase::eig` / `eigh`).

//! `dense::eig_fixed`, `dense::eig_vectors_fixed` and
//! `dense::eigh3_vectors_fixed` are bit-identical to the general
//! `eig` / `eig_vectors` / `eigh_vectors` with `Storage::Fixed` (values,
//! vectors and errors) for every admitted size `N = 1, 2, 3, 4`, and perform
//! no heap allocation (counted with a global allocator wrapper). Besides
//! the strain-like inputs of the law, adversarial families of matrices
//! are compared: scales from
//! 1e-310 to 1e300, repeated and defective (Jordan) spectra, rank-deficient
//! and zero rows, non-finite entries, signed zeros with subnormals,
//! entries near `f64::MAX`, nearly symmetric `V·diag·Vᵀ`, rotations and
//! cyclic permutations (complex pairs), `I − d·e1⊗e1` with `d → 1`, mixed
//! scales.

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use akantu_core::tensor::MatrixX;
use akantu_linalg::dense::{
    SortOrder, Storage, eig, eig_fixed, eig_vectors, eig_vectors_fixed, eigh_vectors,
    eigh3_vectors_fixed,
};

struct Counting;
thread_local! {
    /// Allocations of the current thread (tests run in parallel threads).
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call unchanged to the system allocator; the only
// addition is a thread-local counter increment (const-initialised, so the
// access itself never allocates).
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's (valid, non-zero-size layout).
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was returned by `System.alloc` with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// SplitMix64 in [-1, 1).
struct Rng(u64);
impl Rng {
    fn next(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^= z >> 31;
        #[allow(clippy::cast_precision_loss)] // 53-bit draw
        let v = (z >> 11) as f64 / (1u64 << 53) as f64;
        2. * v - 1.
    }
}

/// Column-major `a[col][row]` test matrices: symmetric strain-like ones (the
/// law's inputs), damage-like `I − D` with repeated eigenvalues, nearly
/// symmetric ones (rounding-level asymmetry, as `V·diag·Vᵀ` produces), and
/// general ones (complex pairs: the error path).
fn matrices<const N: usize>(seed: u64) -> Vec<[[f64; N]; N]> {
    let mut rng = Rng(seed);
    let mut out = Vec::new();
    for k in 0..6_000 {
        let mut a = [[0.0; N]; N];
        for col in &mut a {
            for v in col.iter_mut() {
                *v = rng.next() * 1e-3;
            }
        }
        match k % 6 {
            0..=2 => {
                let c = a;
                for (j, col) in a.iter_mut().enumerate() {
                    for (i, v) in col.iter_mut().enumerate().take(j) {
                        *v = c[i][j];
                    }
                }
            }
            3 => {
                let c = a;
                for (j, col) in a.iter_mut().enumerate() {
                    for (i, v) in col.iter_mut().enumerate().take(j) {
                        *v = c[i][j] * (1. + 1e-16 * rng.next());
                    }
                }
            }
            4 => {
                // I − D with D = d e1⊗e1 (repeated unit eigenvalues)
                a = [[0.0; N]; N];
                for (i, col) in a.iter_mut().enumerate() {
                    col[i] = 1.0;
                }
                a[0][0] = 1.0 - 0.5 * (rng.next() + 1.0);
            }
            _ => {}
        }
        out.push(a);
    }
    out.push([[0.0; N]; N]);
    out
}

fn mx<const N: usize>(a: &[[f64; N]; N]) -> MatrixX {
    MatrixX::from_col_major(N, N, a.as_flattened().to_vec()).unwrap()
}

fn check_eig<const N: usize>(seed: u64) {
    let (mut ok, mut err) = (0, 0);
    for a in matrices::<N>(seed) {
        let general = eig(&mx(&a), Storage::Fixed);
        let fixed = eig_fixed(&a);
        match (general, fixed) {
            (Ok(g), Ok(f)) => {
                ok += 1;
                for (x, y) in g.iter().zip(&f) {
                    assert_eq!(x.to_bits(), y.to_bits(), "{a:?}");
                }
            }
            (Err(g), Err(f)) => {
                err += 1;
                assert_eq!(g.to_string(), f.to_string());
            }
            (g, f) => panic!("{a:?}: general {g:?} fixed {f:?}"),
        }
    }
    assert!(ok > 0);
    let _ = err;
}

fn check_eig_vectors<const N: usize>(seed: u64, order: SortOrder) -> (usize, usize) {
    let (mut ok, mut err) = (0, 0);
    for a in matrices::<N>(seed) {
        let general = eig_vectors(&mx(&a), Storage::Fixed, order);
        let fixed = eig_vectors_fixed(&a, order);
        match (general, fixed) {
            (Ok((gv, gw)), Ok((fv, fw))) => {
                ok += 1;
                for (x, y) in gv.iter().zip(&fv) {
                    assert_eq!(x.to_bits(), y.to_bits(), "{a:?}");
                }
                for (j, col) in fw.iter().enumerate() {
                    for (i, y) in col.iter().enumerate() {
                        assert_eq!(gw.at(i, j).to_bits(), y.to_bits(), "{a:?} ({i}, {j})");
                    }
                }
            }
            (Err(g), Err(f)) => {
                err += 1;
                assert_eq!(g.to_string(), f.to_string());
            }
            (g, f) => panic!("{a:?}: general {g:?} fixed {f:?}"),
        }
    }
    (ok, err)
}

#[test]
fn eig_fixed_is_bit_identical_to_eig_fixed_storage() {
    check_eig::<1>(11);
    check_eig::<2>(12);
    check_eig::<3>(13);
    check_eig::<4>(14);
}

#[test]
fn eig_vectors_fixed_is_bit_identical_to_eig_vectors_fixed_storage() {
    for order in [SortOrder::Descending, SortOrder::Natural] {
        let (ok1, _) = check_eig_vectors::<1>(21, order);
        let (ok2, err2) = check_eig_vectors::<2>(22, order);
        let (ok3, err3) = check_eig_vectors::<3>(23, order);
        let (ok4, err4) = check_eig_vectors::<4>(24, order);
        assert!(ok1 > 0 && ok2 > 0 && ok3 > 0 && ok4 > 0);
        // the general matrices exercise the complex-eigenvector error path
        assert!(
            err2 > 0 && err3 > 0 && err4 > 0,
            "error path not exercised: {err2} {err3} {err4}"
        );
    }
}

#[test]
fn eigh3_vectors_fixed_is_bit_identical_to_eigh_vectors_fixed_storage() {
    for order in [SortOrder::Natural, SortOrder::Descending] {
        for a in matrices::<3>(31) {
            let (gv, gw) = eigh_vectors(&mx(&a), Storage::Fixed, order).unwrap();
            let (fv, fw) = eigh3_vectors_fixed(&a, order).unwrap();
            for (x, y) in gv.iter().zip(&fv) {
                assert_eq!(x.to_bits(), y.to_bits(), "{a:?}");
            }
            for (j, col) in fw.iter().enumerate() {
                for (i, y) in col.iter().enumerate() {
                    assert_eq!(gw.at(i, j).to_bits(), y.to_bits(), "{a:?} ({i}, {j})");
                }
            }
        }
    }
}

#[test]
fn fixed_eigen_paths_do_not_allocate() {
    let m1 = matrices::<1>(41);
    let m2 = matrices::<2>(42);
    let m3 = matrices::<3>(43);
    let m4 = matrices::<4>(44);
    let mut sink = 0.0;
    let before = ALLOCS.with(Cell::get);
    for a in &m1 {
        sink += eig_fixed(a).map_or(0., |v| v[0]);
        sink += eig_vectors_fixed(a, SortOrder::Descending).map_or(0., |v| v.0[0]);
    }
    for a in &m2 {
        sink += eig_fixed(a).map_or(0., |v| v[0]);
        sink += eig_vectors_fixed(a, SortOrder::Descending).map_or(0., |v| v.0[0]);
    }
    for a in &m3 {
        sink += eig_fixed(a).map_or(0., |v| v[0]);
        sink += eig_vectors_fixed(a, SortOrder::Descending).map_or(0., |v| v.0[0]);
        sink += eigh3_vectors_fixed(a, SortOrder::Natural).map_or(0., |v| v.0[0]);
    }
    for a in &m4 {
        sink += eig_fixed(a).map_or(0., |v| v[0]);
        sink += eig_vectors_fixed(a, SortOrder::Descending).map_or(0., |v| v.0[0]);
    }
    let after = ALLOCS.with(Cell::get);
    assert!(sink.is_finite());
    // The complex-eigenvector error carries a `String` (one allocation per
    // refused matrix); count the successful calls only.
    let refused = m2
        .iter()
        .filter(|a| eig_vectors_fixed(a, SortOrder::Descending).is_err())
        .count()
        + m3.iter()
            .filter(|a| eig_vectors_fixed(a, SortOrder::Descending).is_err())
            .count()
        + m4.iter()
            .filter(|a| eig_vectors_fixed(a, SortOrder::Descending).is_err())
            .count();
    assert_eq!(
        after - before,
        refused,
        "allocations beyond the {refused} error messages in {} calls",
        2 * (m1.len() + m2.len() + m4.len()) + 3 * m3.len()
    );
}

/* ------------------------------------------------------------------------ */
/* Adversarial families                                                       */
/* ------------------------------------------------------------------------ */

/// Householder reflector `I − 2vvᵀ/vᵀv` (orthogonal, symmetric), column-major.
fn householder<const N: usize>(r: &mut Rng) -> [[f64; N]; N] {
    let v: [f64; N] = std::array::from_fn(|_| r.next());
    let vv: f64 = v.iter().map(|x| x * x).sum();
    std::array::from_fn(|j| {
        std::array::from_fn(|i| f64::from(u8::from(i == j)) - 2.0 * v[i] * v[j] / vv)
    })
}

fn matmul<const N: usize>(a: &[[f64; N]; N], b: &[[f64; N]; N]) -> [[f64; N]; N] {
    std::array::from_fn(|j| {
        std::array::from_fn(|i| {
            let mut s = 0.0;
            for k in 0..N {
                s += a[k][i] * b[j][k];
            }
            s
        })
    })
}

fn diag<const N: usize>(d: &[f64; N]) -> [[f64; N]; N] {
    std::array::from_fn(|j| std::array::from_fn(|i| if i == j { d[i] } else { 0.0 }))
}

fn transpose<const N: usize>(a: &[[f64; N]; N]) -> [[f64; N]; N] {
    std::array::from_fn(|j| std::array::from_fn(|i| a[i][j]))
}

/// Matrix `k` of the adversarial families, column-major `a[col][row]`.
/// The generator is deterministic (seeded `Rng`).
// one arm per family; the index loops follow the matrix definitions
#[allow(clippy::too_many_lines, clippy::needless_range_loop)]
fn family<const N: usize>(r: &mut Rng, k: usize) -> [[f64; N]; N] {
    const SCALES: [f64; 11] = [
        1e-310, 1e-300, 1e-200, 1e-150, 1e-20, 1e-3, 1.0, 1e20, 1e150, 1e200, 1e300,
    ];
    let s = SCALES[k % SCALES.len()];
    let kind = (k / SCALES.len()) % 16;
    let mut m: [[f64; N]; N] = std::array::from_fn(|_| std::array::from_fn(|_| r.next()));
    let symmetrise = |m: &mut [[f64; N]; N]| {
        for j in 0..N {
            for i in 0..j {
                m[j][i] = m[i][j];
            }
        }
    };
    match kind {
        0 => {
            symmetrise(&mut m);
            m.as_flattened_mut().iter_mut().for_each(|x| *x *= s);
        }
        1 => m.as_flattened_mut().iter_mut().for_each(|x| *x *= s),
        2 => {
            // repeated eigenvalues Q diag(a, …, a, b) Qᵀ
            let q = householder::<N>(r);
            let a = r.next() * s;
            let mut d = [a; N];
            if N > 1 {
                d[N - 1] = r.next() * s;
            }
            m = matmul(&matmul(&q, &diag(&d)), &q);
        }
        3 => m = diag(&[r.next() * s; N]),
        4 => {
            // Jordan block λI + N (defective)
            let lam = [0.0, 1.0, -2.0, s][k % 4];
            m = diag(&[lam; N]);
            for i in 0..N.saturating_sub(1) {
                m[i + 1][i] = s.max(1e-300);
            }
        }
        5 => {
            // P J Pᵀ of a Jordan block (defective-like clusters)
            let mut j = diag(&[1.0; N]);
            for i in 0..N.saturating_sub(1) {
                j[i + 1][i] = 1.0;
            }
            let p: [[f64; N]; N] = std::array::from_fn(|c| {
                std::array::from_fn(|i| f64::from(u8::from(i == c)) * 2.0 + 0.1 * r.next())
            });
            m = matmul(&matmul(&p, &j), &transpose(&p));
            m.as_flattened_mut().iter_mut().for_each(|x| *x *= s);
        }
        6 => {
            // rank one u vᵀ / u uᵀ
            let u: [f64; N] = std::array::from_fn(|_| r.next() * s);
            let v: [f64; N] = std::array::from_fn(|_| r.next());
            let sym = k.is_multiple_of(2);
            m = std::array::from_fn(|c| {
                std::array::from_fn(|i| {
                    if sym {
                        u[i] * u[c] / s.max(1e-300)
                    } else {
                        u[i] * v[c]
                    }
                })
            });
        }
        7 => {
            // zero row and column
            m.as_flattened_mut().iter_mut().for_each(|x| *x *= s);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // index draw
            let c = ((((r.next() + 1.0) * 0.5) * N as f64) as usize).min(N - 1);
            for i in 0..N {
                m[c][i] = 0.0;
                m[i][c] = 0.0;
            }
        }
        8 => {
            // one non-finite entry
            m.as_flattened_mut().iter_mut().for_each(|x| *x *= s);
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // index draw
            let p = ((((r.next() + 1.0) * 0.5) * (N * N) as f64) as usize).min(N * N - 1);
            m.as_flattened_mut()[p] =
                [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -f64::NAN][k % 4];
        }
        9 => {
            // non-finite entries and mixed infinities
            let vals = [f64::NAN, f64::INFINITY, f64::NEG_INFINITY];
            for (t, x) in m.as_flattened_mut().iter_mut().enumerate() {
                *x = if (t + k).is_multiple_of(3) {
                    vals[(t + k) % 3]
                } else {
                    *x * s
                };
            }
        }
        10 => {
            // signed zeros, sometimes one subnormal
            for (t, x) in m.as_flattened_mut().iter_mut().enumerate() {
                *x = if (t + k).is_multiple_of(2) { -0.0 } else { 0.0 };
            }
            if k.is_multiple_of(3) {
                #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)] // index draw
                let p = ((((r.next() + 1.0) * 0.5) * (N * N) as f64) as usize).min(N * N - 1);
                m.as_flattened_mut()[p] = 5e-324;
            }
        }
        11 => {
            // entries near f64::MAX
            m.as_flattened_mut().iter_mut().for_each(|x| *x *= f64::MAX);
            if k.is_multiple_of(2) {
                symmetrise(&mut m);
            }
        }
        12 => {
            // nearly symmetric V diag Vᵀ (the law's tensorPlus output)
            let q = householder::<N>(r);
            let d: [f64; N] = std::array::from_fn(|_| (r.next() * s).max(0.0));
            m = matmul(&matmul(&q, &diag(&d)), &transpose(&q));
        }
        13 => {
            // rotation in the (0, 1) plane / cyclic permutation (complex pairs)
            m = diag(&[1.0; N]);
            if N >= 2 {
                let t = r.next() * 3.0;
                m[0][0] = t.cos() * s;
                m[0][1] = t.sin() * s;
                m[1][0] = -t.sin() * s;
                m[1][1] = t.cos() * s;
            }
            if k.is_multiple_of(2) {
                m = [[0.0; N]; N];
                for i in 0..N {
                    m[i][(i + 1) % N] = s;
                }
            }
        }
        14 => {
            // I − d e1⊗e1, d → 1
            m = diag(&[1.0; N]);
            m[0][0] = [0.0, 1e-16, 0.5, 1.0 - 1e-16, 1.0, -1e-300][k % 6];
        }
        _ => {
            // mixed scales within one matrix
            for (t, x) in m.as_flattened_mut().iter_mut().enumerate() {
                *x *= if t.is_multiple_of(2) { 1e200 } else { 1e-200 };
            }
        }
    }
    m
}

fn outcome_vals<const N: usize>(r: akantu_core::Result<[f64; N]>) -> String {
    match r {
        Ok(v) => format!("ok:{:?}", v.map(f64::to_bits)),
        Err(e) => format!("err:{e}"),
    }
}

fn outcome_general_vals(r: akantu_core::Result<Vec<f64>>) -> String {
    match r {
        Ok(v) => format!("ok:{:?}", v.iter().map(|x| x.to_bits()).collect::<Vec<_>>()),
        Err(e) => format!("err:{e}"),
    }
}

/// Every adversarial family, N = 1…4: the fixed-size paths give the same
/// values, vectors (bit for bit), error texts or panics as the general
/// paths with `Storage::Fixed` (`eigh3_vectors_fixed` vs `eigh_vectors` for
/// N = 3); every error kind is reached.
#[test]
fn adversarial_families_are_bit_identical_n1_to_n4() {
    use std::panic::{AssertUnwindSafe, catch_unwind};
    fn run<F: FnOnce() -> String>(f: F) -> String {
        catch_unwind(AssertUnwindSafe(f)).unwrap_or_else(|_| "panic".to_owned())
    }
    fn check<const N: usize>(seed: u64, counts: &mut [usize; 4]) {
        let mut r = Rng(seed);
        for k in 0..16 * 11 * 12 {
            let a = family::<N>(&mut r, k);
            let general = run(|| outcome_general_vals(eig(&mx(&a), Storage::Fixed)));
            let fixed = run(|| outcome_vals(eig_fixed(&a)));
            assert_eq!(general, fixed, "eig N = {N}, family {k}: {a:?}");
            for order in [SortOrder::Descending, SortOrder::Natural] {
                let g = run(|| match eig_vectors(&mx(&a), Storage::Fixed, order) {
                    Ok((v, w)) => format!(
                        "ok:{:?}|{:?}",
                        v.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                        (0..N * N)
                            .map(|t| w.at(t % N, t / N).to_bits())
                            .collect::<Vec<_>>()
                    ),
                    Err(e) => format!("err:{e}"),
                });
                let f = run(|| match eig_vectors_fixed(&a, order) {
                    Ok((v, w)) => format!(
                        "ok:{:?}|{:?}",
                        v.map(f64::to_bits),
                        w.as_flattened()
                            .iter()
                            .map(|x| x.to_bits())
                            .collect::<Vec<_>>()
                    ),
                    Err(e) => format!("err:{e}"),
                });
                assert_eq!(g, f, "eig_vectors N = {N} {order:?}, family {k}: {a:?}");
                let slot = if f.starts_with("ok:") {
                    0
                } else if f.contains("no convergence") {
                    1
                } else if f.contains("non-finite") {
                    2
                } else {
                    3
                };
                counts[slot] += 1;
            }
            if N == 3 {
                let a3: [[f64; 3]; 3] = std::array::from_fn(|j| std::array::from_fn(|i| a[j][i]));
                for order in [SortOrder::Descending, SortOrder::Natural] {
                    let g = run(|| match eigh_vectors(&mx(&a3), Storage::Fixed, order) {
                        Ok((v, w)) => format!(
                            "ok:{:?}|{:?}",
                            v.iter().map(|x| x.to_bits()).collect::<Vec<_>>(),
                            (0..9)
                                .map(|t| w.at(t % 3, t / 3).to_bits())
                                .collect::<Vec<_>>()
                        ),
                        Err(e) => format!("err:{e}"),
                    });
                    let f = run(|| match eigh3_vectors_fixed(&a3, order) {
                        Ok((v, w)) => format!(
                            "ok:{:?}|{:?}",
                            v.map(f64::to_bits),
                            w.as_flattened()
                                .iter()
                                .map(|x| x.to_bits())
                                .collect::<Vec<_>>()
                        ),
                        Err(e) => format!("err:{e}"),
                    });
                    assert_eq!(g, f, "eigh N = 3 {order:?}, family {k}: {a3:?}");
                }
            }
        }
    }
    let mut counts = [0usize; 4];
    check::<1>(101, &mut counts);
    check::<2>(102, &mut counts);
    check::<3>(103, &mut counts);
    check::<4>(104, &mut counts);
    eprintln!(
        "adversarial eig_vectors outcomes (ok, no convergence, non-finite, complex): {counts:?}"
    );
    assert!(
        counts.iter().all(|&c| c > 0),
        "every outcome is reached: {counts:?}"
    );
}
