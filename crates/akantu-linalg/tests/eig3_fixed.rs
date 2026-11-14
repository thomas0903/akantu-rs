// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs (port of Akantu, EPFL/LSMS): the
// allocation-free compile-time 3 × 3 eigenvalue path used per quadrature point
// by Mazars (src/model/solid_mechanics/materials/material_damage/
// material_mazars_inline_impl.hh: `epsilon.eig(Fdiag)` on a `Matrix<Real, 3, 3>`).

//! `dense::eig3_fixed` is bit-identical to `dense::eig(·, Storage::Fixed)`
//! (same Eigen-order kernels, inline buffers) and performs no heap
//! allocation (counted with a global allocator wrapper).

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use akantu_core::tensor::MatrixX;
use akantu_linalg::dense::{Storage, eig, eig3_fixed};

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

fn matrices() -> Vec<[[f64; 3]; 3]> {
    let mut rng = Rng(7);
    let mut out = Vec::new();
    for k in 0..20_000 {
        let mut a = [[0.0; 3]; 3];
        for col in &mut a {
            for v in col.iter_mut() {
                *v = rng.next() * 1e-3;
            }
        }
        match k % 5 {
            // symmetric strain-like (Mazars)
            0 | 1 => {
                let c = a;
                for (j, col) in a.iter_mut().enumerate() {
                    for (i, v) in col.iter_mut().enumerate().take(j) {
                        *v = c[i][j];
                    }
                }
            }
            // embedded 2-D / 1-D strains (zero out-of-plane rows/columns)
            2 => {
                a[0][1] = a[1][0];
                for col in &mut a {
                    col[2] = 0.;
                }
                a[2] = [0.; 3];
            }
            3 => {
                for (j, col) in a.iter_mut().enumerate() {
                    for (i, v) in col.iter_mut().enumerate() {
                        if i != 0 || j != 0 {
                            *v = 0.;
                        }
                    }
                }
            }
            // general (complex pairs possible)
            _ => {}
        }
        out.push(a);
    }
    out.push([[0.0; 3]; 3]);
    out.push([[1e-3, 0., 0.], [0., 1e-3, 0.], [0., 0., 1e-3]]);
    out.push([[0., 1., 0.], [-1., 0., 0.], [0., 0., 2.]]);
    out
}

#[test]
fn eig3_fixed_is_bit_identical_to_eig_fixed() {
    for a in matrices() {
        let m = MatrixX::from_col_major(3, 3, a.as_flattened().to_vec()).unwrap();
        let general = eig(&m, Storage::Fixed).unwrap();
        let fixed = eig3_fixed(&a).unwrap();
        for (g, f) in general.iter().zip(&fixed) {
            assert_eq!(g.to_bits(), f.to_bits(), "{a:?}");
        }
    }
}

#[test]
fn eig3_fixed_does_not_allocate() {
    let ms = matrices();
    let mut sink = 0.0;
    let before = ALLOCS.with(Cell::get);
    for a in &ms {
        sink += eig3_fixed(a).unwrap()[0];
    }
    let after = ALLOCS.with(Cell::get);
    assert!(sink.is_finite());
    assert_eq!(
        after - before,
        0,
        "allocations in {} eig3_fixed calls",
        ms.len()
    );
}
