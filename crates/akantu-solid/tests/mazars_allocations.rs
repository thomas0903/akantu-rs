// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS).
// Mazars (src/model/solid_mechanics/materials/material_damage/
// material_mazars_inline_impl.hh) must not allocate per quadrature point:
// neither must anisotropic damage
// (material_anisotropic_damage_tmpl.hh).

//! Heap allocations of one `computeStress` of `mazars` / `plastic_mazars`
//! on a block of 1 and of 64 elements (one point each), counted per thread
//! with a global allocator wrapper: the counts must be equal (zero
//! allocations per point; the per-block clones of the internal arrays are
//! independent of the number of points). Before the fix every point allocated
//! a `MatrixX` and the buffers of the dynamic eigen-solver. The same holds
//! for `anisotropic_damage` in its upstream, experimental and corrected
//! (UD-058) modes, 2-D and
//! 3-D (before the fix every point allocated in `tensorPlusTrace` /
//! `tensorPlusOp` / the symmetric solves).

mod common;

use std::alloc::{GlobalAlloc, Layout, System};
use std::cell::Cell;

use akantu_core::Real;
use akantu_model::constitutive_law::LawPhase;
use akantu_solid::material::LawOptions;
use akantu_solid::materials::anisotropic_damage::AnisotropicDamageModel;
use common::{NG, SimplexModel, materials_from_dat_with, simplex};

struct Counting;
thread_local! {
    static ALLOCS: Cell<usize> = const { Cell::new(0) };
}

// SAFETY: forwards every call unchanged to the system allocator; the only
// addition is a const-initialised thread-local counter increment.
unsafe impl GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let _ = ALLOCS.try_with(|c| c.set(c.get() + 1));
        // SAFETY: same contract as the caller's.
        unsafe { System.alloc(layout) }
    }
    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        // SAFETY: `ptr` was returned by `System.alloc` with this `layout`.
        unsafe { System.dealloc(ptr, layout) }
    }
}

#[global_allocator]
static GLOBAL: Counting = Counting;

/// `n` disjoint tetrahedra.
fn tets(n: usize) -> SimplexModel {
    let mut nodes = Vec::new();
    let mut conn = Vec::new();
    for e in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let o = 2. * e as Real;
        let b = i32::try_from(4 * e).unwrap();
        nodes.extend([
            vec![o, 0., 0.],
            vec![o + 1.1, 0.1, 0.05],
            vec![o + 0.2, 0.9, 0.1],
            vec![o + 0.1, 0.2, 1.2],
        ]);
        conn.push(vec![b, b + 1, b + 2, b + 3]);
    }
    // serial services: the allocation counts below assume the serial point
    // loops (a thread pool allocates its parts)
    let mut m = SimplexModel::new(3, &nodes, &conn);
    m.serial = true;
    m
}

/// `n` disjoint triangles.
fn tris(n: usize) -> SimplexModel {
    let mut nodes = Vec::new();
    let mut conn = Vec::new();
    for e in 0..n {
        #[allow(clippy::cast_precision_loss)]
        let o = 2. * e as Real;
        let b = i32::try_from(3 * e).unwrap();
        nodes.extend([vec![o, 0.], vec![o + 1.1, 0.2], vec![o + 0.3, 0.9]]);
        conn.push(vec![b, b + 1, b + 2]);
    }
    // serial services: the allocation counts below assume the serial point
    // loops (a thread pool allocates its parts)
    let mut m = SimplexModel::new(2, &nodes, &conn);
    m.serial = true;
    m
}

/// Allocations of one `computeStress` (after a first, warm-up step).
fn allocations(dat: &str, n: usize) -> usize {
    allocations_with(dat, n, 3, LawOptions::default(), [1., 2.])
}

/// Allocations of one `computeStress` of a `dim`-D block of `n` elements
/// with `options`, after a warm-up step (gradient scales `scales`).
fn allocations_with(
    dat: &str,
    n: usize,
    dim: usize,
    options: LawOptions,
    scales: [Real; 2],
) -> usize {
    let model = if dim == 3 { tets(n) } else { tris(n) };
    let mut h = materials_from_dat_with(&model, dat.as_bytes(), Some(options)).unwrap();
    let law = h.law_mut(0).unwrap();
    let ty = simplex(dim);
    let g3 = [6e-4, 1e-4, -2e-4, 1e-4, -3e-4, 5e-5, -2e-4, 5e-5, 2e-4];
    let g2 = [6e-4, 1e-4, 1e-4, -3e-4];
    let g: &[Real] = if dim == 3 { &g3 } else { &g2 };
    let mut run = |scale: Real| -> usize {
        law.before_solve_step(&model.ctx(LawPhase::BeforeSolveStep))
            .unwrap();
        {
            let mut gu = law.base_mut().grad_u.get_mut(ty, NG).unwrap();
            for t in gu.as_mut_slice().chunks_exact_mut(dim * dim) {
                for (v, x) in t.iter_mut().zip(g) {
                    *v = x * scale;
                }
            }
        }
        let before = ALLOCS.with(Cell::get);
        law.compute_stress(&model.ctx(LawPhase::Stress), ty, NG)
            .unwrap();
        ALLOCS.with(Cell::get) - before
    };
    run(scales[0]);
    run(scales[1])
}

#[test]
fn mazars_compute_stress_allocates_nothing_per_point() {
    let mazars =
        "material mazars [\n  name = m\n  rho = 1\n  E = 25e9\n  nu = 0.2\n  K0 = 1e-4\n]\n";
    let pm = "material plastic_mazars [\n  name = m\n  rho = 1\n  E = 25e9\n  nu = 0.2\n  \
              sigma_y = 1e6\n  h = 1e9\n  K0 = 1e-4\n]\n";
    for dat in [mazars, pm] {
        let (one, many) = (allocations(dat, 1), allocations(dat, 64));
        eprintln!("allocations per computeStress: 1 point {one}, 64 points {many}");
        assert_eq!(one, many, "per-point allocations in {dat}");
    }
}

/// The upstream mode (`tensorPlusTrace`, `tensorPlusOp`
/// through `EigenSolver` at compile-time size), the experimental mode and
/// the corrected UD-058 mode (symmetric 3 × 3 solves; the acceptance tests of its specification)
/// allocate nothing per point, below and above the damage threshold, in
/// 2-D and 3-D.
#[test]
fn anisotropic_damage_compute_stress_allocates_nothing_per_point() {
    let dat = "material anisotropic_damage [\n  name = m\n  rho = 1\n  E = 30e9\n  nu = 0.2\n  \
               Dc = 0.99\n]\n";
    for model in [
        AnisotropicDamageModel::Upstream,
        AnisotropicDamageModel::DesmoratExperimental,
        AnisotropicDamageModel::Corrected,
    ] {
        let options = LawOptions {
            anisotropic_damage: model,
            ..LawOptions::default()
        };
        // scales: below the threshold (ε̂ < K0 = 5e-5), then damaging
        for (dim, scales) in [
            (2, [0.01, 0.1]),
            (3, [0.01, 0.1]),
            (2, [0.1, 0.15]),
            (3, [0.1, 0.15]),
        ] {
            let one = allocations_with(dat, 1, dim, options, scales);
            let many = allocations_with(dat, 64, dim, options, scales);
            eprintln!(
                "anisotropic_damage {model:?} {dim}-D {scales:?}: allocations per computeStress: \
                 1 point {one}, 64 points {many}"
            );
            assert_eq!(
                one, many,
                "per-point allocations ({model:?}, {dim}-D, {scales:?})"
            );
        }
    }
}
