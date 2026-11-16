// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/model/solid_mechanics/material.cc
// (assembleInternalForcesFiniteDeformation: `B_tensor.transpose() *
// S_voight` per quadrature point), material_inline_impl.hh (stressToVoigt),
// src/common/aka_voigthelper_tmpl.hh (transferBMatrixToSymVoigtBMatrix,
// transferBMatrixToBL2), with the Eigen 3.4 evaluation order of the product.

//! Fixed-size total-Lagrangian residual per quadrature point.
//!
//! The general path (`internal_forces` in the parent module) builds
//! `B_L0` and `B_L1` in `MatrixX` scratch matrices through
//! `akantu_core::voigt`, sums them and evaluates `Bᵀ · S_voigt` with
//! `akantu_fe::eigen::product_slices` (compile-time `ts × dim·nn` operand,
//! transposed, times a compile-time `ts × 1` vector: route
//! coefficient-based, halving tree over the `ts` Voigt components). This
//! kernel forms every entry of `B` with the same arithmetic — the
//! `transferBMatrixToSymVoigtBMatrix` entry (a derivative or the zero it
//! leaves), plus the `transferBMatrixToBL2` product or accumulation into
//! zero, the `B += B2` addition — column by column without storing the
//! matrices, then the product with `akantu_core::eigen_unrolled::tree_sum`,
//! so every value is bitwise the general path's (property test below).
//! Only selected when the route is the tree and `nn ≤ 32`.

use akantu_core::eigen_gebp::{GEMV_BLOCKING_MIN_DEPTH, GemvMatrix, Order, Reduction, Route};
use akantu_core::eigen_unrolled::{TREE_MAX, tree_sum, tree_sum_of};
use akantu_core::exec::{
    FastToken, FmaTask, Lanes4, SimdTask, SimdToken, Tuples, TuplesRef, with_fma, with_simd,
};
use akantu_core::voigt::VoigtTable;
use akantu_core::{Idx, Real};
use akantu_fe::eigen::{Dims, product_route};

use super::{Derivatives, Sizes};

/// `r_q = (B_L0 + B_L1(∇u_q))ᵀ · stressToVoigt(S_q)` for every quadrature
/// point of `filter` into `out` (`filter.len()·nq` tuples of `dim·nn`), from
/// the stored `grad_u` (`gs`) and `piola_kirchhoff_2` (`ps`). `None` when
/// the product route is not the halving tree, `nn > 32`, `dim ∉ 1…3` or a
/// buffer has an unexpected length (the caller then runs the general path).
#[allow(clippy::too_many_arguments)] // the general path's arguments and the executor
pub(super) fn residuals(
    exec: &akantu_core::exec::Exec,
    s: &Sizes,
    table: &VoigtTable,
    dsh: &Derivatives<'_>,
    filter: &[Idx],
    gs: &[Real],
    ps: &[Real],
    out: &mut [Real],
) -> Option<()> {
    let route = product_route(Dims::fixed(s.ts, s.bt).t(), Dims::fixed(s.ts, 1));
    let np = filter.len() * s.nq;
    let d2 = s.dim * s.dim;
    if route != Route::Coeff(Reduction::Tree)
        || s.nn > TREE_MAX
        || table.dim != s.dim
        || table.size != s.ts
        || gs.len() != np * d2
        || ps.len() != np * d2
        || out.len() != np * s.bt
    {
        return None;
    }
    let run: ResidualPart = match s.dim {
        1 => run::<1, 1>,
        2 => run::<2, 3>,
        3 => run::<3, 6>,
        _ => return None,
    };
    if s.nq == 0 || s.bt == 0 {
        return None;
    }
    // every quadrature point from its own data: contiguous parts of the
    // filter on the executor's threads (D-123)
    let items = (
        TuplesRef::new(filter, 1),
        TuplesRef::new(gs, s.nq * d2),
        TuplesRef::new(ps, s.nq * d2),
        Tuples::new(out, s.nq * s.bt),
    );
    let min = (2048 / s.nq).max(1);
    exec.for_each(items, min, |_, (f, g, p, mut o)| {
        run(
            s,
            table,
            dsh,
            f.as_slice(),
            g.as_slice(),
            p.as_slice(),
            o.as_mut_slice(),
        );
    });
    Some(())
}

/// One part of [`residuals`].
type ResidualPart =
    fn(&Sizes, &VoigtTable, &Derivatives<'_>, &[Idx], &[Real], &[Real], &mut [Real]);

#[inline(never)] // a part: its slices are no-alias parameters
fn run<const D: usize, const TS: usize>(
    s: &Sizes,
    table: &VoigtTable,
    dsh: &Derivatives<'_>,
    filter: &[Idx],
    gs: &[Real],
    ps: &[Real],
    out: &mut [Real],
) {
    let mut sv = [0.0; TS];
    let voigt: [(usize, usize); TS] = std::array::from_fn(|big_i| table.vec(big_i));
    for (pos, &e) in filter.iter().enumerate() {
        for q in 0..s.nq {
            let t = pos * s.nq + q;
            for (v, &(i, j)) in sv.iter_mut().zip(&voigt) {
                *v = ps[t * D * D + i + j * D];
            }
            point::<D, TS>(
                s.nn,
                dsh.at(e, q),
                &gs[t * D * D..(t + 1) * D * D],
                &sv,
                &mut out[t * s.bt..(t + 1) * s.bt],
            );
        }
    }
}

/// One quadrature point: `dndx` column-major `D × nn`, `gu` column-major
/// `D × D`, `sv` the Voigt stress, `out` the `D·nn` residual entries.
///
/// Column `c = D·a + k` (node `a`, component `k`) of `B = B_L0 + B_L1` is
/// formed entry by entry with the general path's arithmetic: `B_L0(I, c)`
/// is the entry `transferBMatrixToSymVoigtBMatrix` assigns into its zeroed
/// matrix (a derivative or `0.0`), `B_L1(I, c)` the product
/// `transferBMatrixToBL2` assigns (normal rows) or its two accumulations
/// into `0.0` in loop order (shear rows), and `B(I, c) = B_L0 + B_L1` the
/// `B += B2` addition; then `r_c = tree(B(I, c) · S_I)`. No entry of either
/// matrix is assigned twice, so this is the same value as building both
/// matrices first.
#[inline(always)]
pub(super) fn point<const D: usize, const TS: usize>(
    nn: usize,
    dndx: &[Real],
    gu: &[Real],
    sv: &[Real; TS],
    out: &mut [Real],
) {
    let dn = |i: usize, n: usize| dndx[i + n * D];
    let g = |i: usize, j: usize| gu[i + j * D];
    let mut p = [0.0; TS];
    for a in 0..nn {
        for k in 0..D {
            let mut b = [0.0; TS];
            match D {
                1 => {
                    let dx = dn(0, a);
                    b[0] = dx + g(0, 0) * dx;
                }
                2 => {
                    let (dx, dy) = (dn(0, a), dn(1, a));
                    let (g0, g1) = (g(k, 0), g(k, 1));
                    b[0] = (if k == 0 { dx } else { 0.0 }) + g0 * dx;
                    b[1] = (if k == 1 { dy } else { 0.0 }) + g1 * dy;
                    // row 2: L0 = (dN/dy, dN/dx)[k]; L1 = 0 + g(k,0)·dN/dy + g(k,1)·dN/dx
                    let l2 = (0.0 + g0 * dy) + g1 * dx;
                    b[2] = (if k == 0 { dy } else { dx }) + l2;
                }
                _ => {
                    let (dx, dy, dz) = (dn(0, a), dn(1, a), dn(2, a));
                    let (g0, g1, g2) = (g(k, 0), g(k, 1), g(k, 2));
                    b[0] = (if k == 0 { dx } else { 0.0 }) + g0 * dx;
                    b[1] = (if k == 1 { dy } else { 0.0 }) + g1 * dy;
                    b[2] = (if k == 2 { dz } else { 0.0 }) + g2 * dz;
                    // shear rows (iv, aux = iv − 3; m ≠ aux in increasing order,
                    // dN index 3 − m − aux): L0 from the sym-Voigt pattern
                    let l3 = (0.0 + g1 * dz) + g2 * dy;
                    b[3] = [0.0, dz, dy][k] + l3;
                    let l4 = (0.0 + g0 * dz) + g2 * dx;
                    b[4] = [dz, 0.0, dx][k] + l4;
                    let l5 = (0.0 + g0 * dy) + g1 * dx;
                    b[5] = [dy, dx, 0.0][k] + l5;
                }
            }
            // r = B.transpose() * S_voight: lhs(c, I) = B(I, c), rhs(I, 0) = S_I
            for big_i in 0..TS {
                p[big_i] = b[big_i] * sv[big_i];
            }
            out[a * D + k] = tree_sum(&p, TS);
        }
    }
}

/// [`residuals`] followed by the integration of the point residuals over
/// each element (`r_e = Σ_q r_q · J_q`, the GEMV order of the FE engine's
/// `integrate` with `nb_dof = dim·nn`), without the per-point array: every
/// point residual is the value [`point`] computes, and it enters the
/// element's accumulators in the order `integrate` reads it back, so `out`
/// (`filter.len()` tuples of `dim·nn`) receives the bits of the two steps.
/// `jac` holds `det J · w` of every element of the block (`nq` each; the
/// filter is checked by the caller against the shape derivatives, whose
/// element count `jac` must match). `None` when a route, a size or `dim`
/// differs from what this kernel implements.
#[allow(clippy::too_many_arguments)] // the general path's arguments and the executor
pub(super) fn residuals_integrate(
    exec: &akantu_core::exec::Exec,
    s: &Sizes,
    table: &VoigtTable,
    dsh: &Derivatives<'_>,
    filter: &[Idx],
    gs: &[Real],
    ps: &[Real],
    jac: &[Real],
    out: &mut [Real],
) -> Option<()> {
    let np = filter.len() * s.nq;
    let d2 = s.dim * s.dim;
    if !residuals_integrate_supported(s, table, dsh, jac)
        || gs.len() != np * d2
        || ps.len() != np * d2
        || out.len() != filter.len() * s.bt
    {
        return None;
    }
    let run = residual_integrate_kernel(s.dim)?;
    let items = (
        TuplesRef::new(filter, 1),
        TuplesRef::new(gs, s.nq * d2),
        TuplesRef::new(ps, s.nq * d2),
        Tuples::new(out, s.bt),
    );
    let min = (2048 / s.nq).max(1);
    let (simd, fast) = (exec.simd_token(), exec.fast_token());
    exec.for_each(items, min, |_, (f, g, p, mut o)| {
        run(
            simd,
            fast,
            s,
            table,
            dsh.data,
            jac,
            f.as_slice(),
            g.as_slice(),
            p.as_slice(),
            o.as_mut_slice(),
        );
    });
    Some(())
}

/// `true` when [`residuals_integrate`] takes its kernel for the block
/// (routes, sizes, `dim`; the filter is checked by the caller).
pub(super) fn residuals_integrate_supported(
    s: &Sizes,
    table: &VoigtTable,
    dsh: &Derivatives<'_>,
    jac: &[Real],
) -> bool {
    let route = product_route(Dims::fixed(s.ts, s.bt).t(), Dims::fixed(s.ts, 1));
    let int_route = product_route(Dims::dynamic(s.bt, s.nq), Dims::new(s.nq, 1, false, true));
    let gemv = Route::Gemv {
        matrix: GemvMatrix::Lhs,
        kernel: Order::ColMajor,
    };
    route == Route::Coeff(Reduction::Tree)
        && int_route == gemv
        && s.nq > 0
        && s.nq < GEMV_BLOCKING_MIN_DEPTH
        && s.bt > 0
        && s.nn <= TREE_MAX
        && table.dim == s.dim
        && table.size == s.ts
        && dsh.data.len() == dsh.nb_elements * s.nq * s.bt
        && jac.len() == dsh.nb_elements * s.nq
        && residual_integrate_kernel(s.dim).is_some()
}

/// The fused kernel of `dim`.
fn residual_integrate_kernel(dim: usize) -> Option<ResidualIntegratePart> {
    Some(match dim {
        1 => residual_integrate_part::<1, 1>,
        2 => residual_integrate_part::<2, 3>,
        3 => residual_integrate_part::<3, 6>,
        _ => return None,
    })
}

/// [`residuals_integrate`] of the elements `filter` on the calling thread
/// (a caller that checked [`residuals_integrate_supported`]).
#[allow(clippy::too_many_arguments)]
pub(super) fn residuals_integrate_serial(
    simd: SimdToken,
    fast: Option<FastToken>,
    s: &Sizes,
    table: &VoigtTable,
    dsh: &[Real],
    jac: &[Real],
    filter: &[Idx],
    gs: &[Real],
    ps: &[Real],
    out: &mut [Real],
) {
    if let Some(run) = residual_integrate_kernel(s.dim) {
        run(simd, fast, s, table, dsh, jac, filter, gs, ps, out);
    }
}

/// One part of [`residuals_integrate`].
type ResidualIntegratePart = fn(
    SimdToken,
    Option<FastToken>,
    &Sizes,
    &VoigtTable,
    &[Real],
    &[Real],
    &[Idx],
    &[Real],
    &[Real],
    &mut [Real],
);

#[inline(never)] // a part: its slices are no-alias parameters
#[allow(clippy::too_many_arguments)]
fn residual_integrate_part<const D: usize, const TS: usize>(
    simd: SimdToken,
    fast: Option<FastToken>,
    s: &Sizes,
    table: &VoigtTable,
    dsh: &[Real],
    jac: &[Real],
    filter: &[Idx],
    gs: &[Real],
    ps: &[Real],
    out: &mut [Real],
) {
    let voigt: [(usize, usize); TS] = std::array::from_fn(|big_i| table.vec(big_i));
    match fast {
        // FAST MODE (D-122): fused multiply-adds in B, in Bᵀ·S and in the
        // integration, validated by `tests/fast_mode_gradient.rs`
        Some(t) => with_fma(
            t,
            LaneResidualIntegrate::<D, TS, true> {
                nn: s.nn,
                nq: s.nq,
                voigt,
                dsh,
                jac,
                filter,
                gs,
                ps,
                out,
            },
        ),
        None => with_simd(
            simd,
            LaneResidualIntegrate::<D, TS, false> {
                nn: s.nn,
                nq: s.nq,
                voigt,
                dsh,
                jac,
                filter,
                gs,
                ps,
                out,
            },
        ),
    }
}

/// [`residuals_integrate`] of the elements of a part, lanes over the
/// displacement component `k` of each node.
struct LaneResidualIntegrate<'a, const D: usize, const TS: usize, const FMA: bool> {
    nn: usize,
    nq: usize,
    voigt: [(usize, usize); TS],
    dsh: &'a [Real],
    jac: &'a [Real],
    filter: &'a [Idx],
    gs: &'a [Real],
    ps: &'a [Real],
    out: &'a mut [Real],
}

impl<const D: usize, const TS: usize, const FMA: bool> FmaTask
    for LaneResidualIntegrate<'_, D, TS, FMA>
{
    type Output = ();

    #[inline(always)]
    fn run(self) {
        SimdTask::run(self);
    }
}

impl<const D: usize, const TS: usize, const FMA: bool> SimdTask
    for LaneResidualIntegrate<'_, D, TS, FMA>
{
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Self {
            nn,
            nq,
            voigt,
            dsh,
            jac,
            filter,
            gs,
            ps,
            out,
        } = self;
        let bt = D * nn;
        let groups = nn.div_ceil(4);
        // accumulators c(a, k) of the element, lane = node `a` of a group of
        // four, one vector per (group, component k)
        let mut c = [[Lanes4::default(); D]; TREE_MAX / 4];
        // the point's shape derivatives, transposed: per group and natural
        // direction, the four nodes' values (zeros beyond `nn`)
        let mut dt = [[Lanes4::default(); D]; TREE_MAX / 4];
        for (((&e, ge), pe), oe) in filter
            .iter()
            .zip(gs.chunks_exact(nq * D * D))
            .zip(ps.chunks_exact(nq * D * D))
            .zip(out.chunks_exact_mut(bt))
        {
            let e = usize::try_from(e).unwrap_or(usize::MAX);
            let de = &dsh[e * nq * bt..(e + 1) * nq * bt];
            let je = &jac[e * nq..(e + 1) * nq];
            // the GEMV accumulators of the element: c = 0, then for every
            // point c = c + fl(r_q · J_q)
            for cg in c.iter_mut().take(groups) {
                *cg = [Lanes4::splat(0.0); D];
            }
            for (((dndx, gu), pk2), &jq) in de
                .chunks_exact(bt)
                .zip(ge.chunks_exact(D * D))
                .zip(pe.chunks_exact(D * D))
                .zip(je)
            {
                let sv: [Real; TS] = std::array::from_fn(|big_i| {
                    let (i, j) = voigt[big_i];
                    pk2[i + j * D]
                });
                let (nodes, _) = dndx.as_chunks::<D>();
                for (gi, dg) in dt.iter_mut().enumerate().take(groups) {
                    for (i, di) in dg.iter_mut().enumerate() {
                        *di = Lanes4(std::array::from_fn(|l| {
                            nodes.get(4 * gi + l).map_or(0.0, |n| n[i])
                        }));
                    }
                }
                let jq = Lanes4::splat(jq);
                for (cg, dg) in c.iter_mut().zip(&dt).take(groups) {
                    for (k, ck) in cg.iter_mut().enumerate() {
                        *ck = if FMA {
                            node_lanes_fma::<D, TS>(k, dg, gu, &sv).mul_add(jq, *ck)
                        } else {
                            *ck + node_lanes::<D, TS>(k, dg, gu, &sv) * jq
                        };
                    }
                }
            }
            // res = 0 + c (zeroed destination)
            for (a, oa) in oe.chunks_exact_mut(D).enumerate() {
                for (k, o) in oa.iter_mut().enumerate() {
                    *o = 0.0 + c[a / 4][k].0[a % 4];
                }
            }
        }
    }
}

/// The residual entries `r(D·a + k)` of one component `k` for four nodes
/// `a` of one point, lane = node (`d[i]` holds the four nodes' `dN/dX_i`):
/// per lane the statements of [`point`] for column `D·a + k` of `B` (the
/// same operands in the same order; the selections of `B_L0` depend on
/// `k` only, so they are the same in every lane).
#[inline(always)]
pub(super) fn node_lanes<const D: usize, const TS: usize>(
    k: usize,
    d: &[Lanes4; D],
    gu: &[Real],
    sv: &[Real; TS],
) -> Lanes4 {
    let sp = Lanes4::splat;
    let zero = sp(0.0);
    // g(k, j) of the column-major `∇u`
    let g = |j: usize| sp(gu[k + j * D]);
    let mut b = [zero; TS];
    match D {
        1 => {
            let dx = d[0];
            b[0] = dx + g(0) * dx;
        }
        2 => {
            let (dx, dy) = (d[0], d[1]);
            let (g0, g1) = (g(0), g(1));
            b[0] = (if k == 0 { dx } else { zero }) + g0 * dx;
            b[1] = (if k == 1 { dy } else { zero }) + g1 * dy;
            let l2 = (zero + g0 * dy) + g1 * dx;
            b[2] = (if k == 0 { dy } else { dx }) + l2;
        }
        _ => {
            let (dx, dy, dz) = (d[0], d[1], d[2]);
            let (g0, g1, g2) = (g(0), g(1), g(2));
            b[0] = (if k == 0 { dx } else { zero }) + g0 * dx;
            b[1] = (if k == 1 { dy } else { zero }) + g1 * dy;
            b[2] = (if k == 2 { dz } else { zero }) + g2 * dz;
            let l3 = (zero + g1 * dz) + g2 * dy;
            b[3] = [zero, dz, dy][k] + l3;
            let l4 = (zero + g0 * dz) + g2 * dx;
            b[4] = [dz, zero, dx][k] + l4;
            let l5 = (zero + g0 * dy) + g1 * dx;
            b[5] = [dy, dx, zero][k] + l5;
        }
    }
    // r = B.transpose() * S_voight: lhs(c, I) = B(I, c), rhs(I, 0) = S_I
    let mut p = [zero; TS];
    for big_i in 0..TS {
        p[big_i] = b[big_i] * sp(sv[big_i]);
    }
    tree_sum_of(&p, TS)
}

/// FAST MODE (D-122): [`node_lanes`] with fused multiply-adds: every
/// `L0 + g·dN` and `(0 + g·dN) + g·dN` of `B` and the sum `Σ_I B(I)·S_I`
/// (a chain in `I` order instead of the halving tree) round once per
/// multiply-add.
#[inline(always)]
pub(super) fn node_lanes_fma<const D: usize, const TS: usize>(
    k: usize,
    d: &[Lanes4; D],
    gu: &[Real],
    sv: &[Real; TS],
) -> Lanes4 {
    let sp = Lanes4::splat;
    let zero = sp(0.0);
    let g = |j: usize| sp(gu[k + j * D]);
    let mut b = [zero; TS];
    match D {
        1 => {
            let dx = d[0];
            b[0] = g(0).mul_add(dx, dx);
        }
        2 => {
            let (dx, dy) = (d[0], d[1]);
            let (g0, g1) = (g(0), g(1));
            b[0] = g0.mul_add(dx, if k == 0 { dx } else { zero });
            b[1] = g1.mul_add(dy, if k == 1 { dy } else { zero });
            b[2] = g1.mul_add(dx, g0 * dy) + if k == 0 { dy } else { dx };
        }
        _ => {
            let (dx, dy, dz) = (d[0], d[1], d[2]);
            let (g0, g1, g2) = (g(0), g(1), g(2));
            b[0] = g0.mul_add(dx, if k == 0 { dx } else { zero });
            b[1] = g1.mul_add(dy, if k == 1 { dy } else { zero });
            b[2] = g2.mul_add(dz, if k == 2 { dz } else { zero });
            b[3] = g2.mul_add(dy, g1 * dz) + [zero, dz, dy][k];
            b[4] = g2.mul_add(dx, g0 * dz) + [dz, zero, dx][k];
            b[5] = g1.mul_add(dx, g0 * dy) + [dy, dx, zero][k];
        }
    }
    let mut r = b[0] * sp(sv[0]);
    for big_i in 1..TS {
        r = b[big_i].mul_add(sp(sv[big_i]), r);
    }
    r
}

#[cfg(test)]
mod tests {
    use super::*;
    use akantu_core::tensor::MatrixX;
    use akantu_core::voigt;

    /// Deterministic test values. Default mode: special values (signed zeros,
    /// infinities, subnormals, extremes), arbitrary bit patterns and finite
    /// values over a wide exponent range, where every NaN drawn is a *default*
    /// quiet NaN of either sign (see [`same`]). Payload mode
    /// ([`Values::payload_at`]): finite values only, except one draw, a NaN
    /// with a payload.
    struct Values {
        state: u64,
        draws: u64,
        nan_at: Option<u64>,
    }

    /// Bits of the default quiet NaN without the sign.
    const DEFAULT_NAN: u64 = 0x7ff8_0000_0000_0000;
    /// The NaN of the payload mode (quiet, negative, payload `0xabc`).
    const PAYLOAD_NAN: u64 = 0xfff8_0000_0000_0abc;

    /// The exact rule of D-046/D-051: identical bits, or two default quiet NaNs
    /// that differ only in the sign (`+0` and `−0` differ). When two NaNs meet
    /// in one addition, which one IEEE 754 returns is unspecified: x86 SSE
    /// returns the first operand's, and the compiler may commute the operands
    /// in either code path (Rust and LLVM leave the payload of such a NaN
    /// unspecified). Default-mode inputs therefore carry default NaNs only, and
    /// payload propagation is checked in payload mode, where a single NaN
    /// enters each case and every result bit is determined.
    fn same(a: Real, b: Real) -> bool {
        let (x, y) = (a.to_bits(), b.to_bits());
        let sign = 1u64 << 63;
        x == y || (x & !sign == DEFAULT_NAN && y & !sign == DEFAULT_NAN)
    }

    impl Values {
        fn new(seed: u64) -> Self {
            Self {
                state: seed,
                draws: 0,
                nan_at: None,
            }
        }

        fn next_u64(&mut self) -> u64 {
            // xorshift64*
            let mut x = self.state;
            x ^= x >> 12;
            x ^= x << 25;
            x ^= x >> 27;
            self.state = x;
            x.wrapping_mul(0x2545_f491_4f6c_dd1d)
        }

        /// Finite value with a moderate exponent spread (cancellations, no
        /// overflow in the kernels' few operations).
        fn finite(&mut self) -> Real {
            let r = self.next_u64();
            let m = (self.next_u64() >> 11) as Real / (1u64 << 53) as Real;
            let e = ((r >> 8) % 41) as i32 - 20;
            let s = if (r >> 40) & 1 == 1 { -1.0 } else { 1.0 };
            s * m * Real::powi(2.0, e)
        }

        fn next(&mut self) -> Real {
            const SPECIAL: [u64; 14] = [
                0x0000_0000_0000_0000, // +0
                0x8000_0000_0000_0000, // -0
                0x7ff0_0000_0000_0000, // +inf
                0xfff0_0000_0000_0000, // -inf
                0x7ff8_0000_0000_0000, // default quiet NaN (aarch64)
                0xfff8_0000_0000_0000, // default quiet NaN (x86_64)
                0x0000_0000_0000_0001, // smallest subnormal
                0x800f_ffff_ffff_ffff, // largest negative subnormal
                0x0010_0000_0000_0000, // MIN_POSITIVE
                0x7fef_ffff_ffff_ffff, // MAX
                0x3ff0_0000_0000_0000, // 1
                0xbff0_0000_0000_0000, // -1
                0x3cb0_0000_0000_0000, // 2^-52
                0x4340_0000_0000_0000, // 2^53
            ];
            self.draws += 1;
            if let Some(k) = self.nan_at {
                return if self.draws == k {
                    Real::from_bits(PAYLOAD_NAN)
                } else {
                    self.finite()
                };
            }
            let r = self.next_u64();
            let x = match r % 8 {
                0 => Real::from_bits(SPECIAL[(r >> 8) as usize % SPECIAL.len()]),
                1 => Real::from_bits(self.next_u64()), // any bit pattern
                _ => self.finite(),
            };
            if x.is_nan() {
                Real::from_bits(DEFAULT_NAN | (x.to_bits() & (1 << 63)))
            } else {
                x
            }
        }

        /// Payload mode: finite draws, the `k`-th draw from now (`k ≥ 1`) is
        /// the payload NaN.
        fn payload_at(&mut self, k: u64) {
            self.nan_at = Some(self.draws + k);
        }

        /// Back to the default mode.
        fn default_mode(&mut self) {
            self.nan_at = None;
        }
    }

    impl Values {
        fn vec(&mut self, n: usize) -> Vec<Real> {
            (0..n).map(|_| self.next()).collect()
        }
    }

    /// The general path's statements for one point (`b_l0_plus_l1`,
    /// `stress_to_voigt`, `product_slices`).
    fn general_point(dim: usize, nn: usize, dndx: &[Real], gu: &[Real], pk2: &[Real]) -> Vec<Real> {
        let t = voigt::voigt_table(dim).unwrap();
        let (ts, bt) = (t.size, dim * nn);
        let s = Sizes {
            dim,
            nn,
            nq: 1,
            ts,
            bt,
        };
        let mut scratch = super::super::Scratch {
            dndx: MatrixX::zeros(dim, nn),
            grad_u: MatrixX::zeros(dim, dim),
            b: MatrixX::zeros(ts, bt),
            b2: MatrixX::zeros(ts, bt),
        };
        super::super::b_l0_plus_l1(&s, dndx, gu, &mut scratch).unwrap();
        let mut sv = vec![0.0; ts];
        super::super::stress_to_voigt(t, pk2, &mut sv);
        let mut out = vec![0.0; bt];
        akantu_fe::eigen::product_slices(
            Dims::fixed(ts, bt).t(),
            Dims::fixed(ts, 1),
            scratch.b.as_slice(),
            &sv,
            &mut out,
        )
        .unwrap();
        out
    }

    fn fast_point(dim: usize, nn: usize, dndx: &[Real], gu: &[Real], pk2: &[Real]) -> Vec<Real> {
        let t = voigt::voigt_table(dim).unwrap();
        let mut out = vec![Real::NAN; dim * nn];
        match dim {
            1 => point::<1, 1>(nn, dndx, gu, &[pk2[0]], &mut out),
            2 => {
                let sv = std::array::from_fn(|k| {
                    let (i, j) = t.vec(k);
                    pk2[i + j * 2]
                });
                point::<2, 3>(nn, dndx, gu, &sv, &mut out);
            }
            _ => {
                let sv = std::array::from_fn(|k| {
                    let (i, j) = t.vec(k);
                    pk2[i + j * 3]
                });
                point::<3, 6>(nn, dndx, gu, &sv, &mut out);
            }
        }
        out
    }

    /// [`node_lanes`] for every node and component of one point, at every
    /// supported SIMD level (`with_simd`), stored as `r(D·a + k)`.
    fn lanes_point(dim: usize, nn: usize, dndx: &[Real], gu: &[Real], pk2: &[Real]) -> Vec<Real> {
        struct Task<'a, const D: usize, const TS: usize>(usize, &'a [Real], &'a [Real], [Real; TS]);
        impl<const D: usize, const TS: usize> SimdTask for Task<'_, D, TS> {
            type Output = Vec<Real>;
            #[inline(always)]
            fn run(self) -> Vec<Real> {
                let nn = self.0;
                let mut out = vec![0.0; D * nn];
                for gi in 0..nn.div_ceil(4) {
                    let d: [Lanes4; D] = std::array::from_fn(|i| {
                        Lanes4(std::array::from_fn(|l| {
                            let a = 4 * gi + l;
                            if a < nn { self.1[i + a * D] } else { 0.0 }
                        }))
                    });
                    for k in 0..D {
                        let r = node_lanes::<D, TS>(k, &d, self.2, &self.3);
                        for l in 0..4 {
                            if 4 * gi + l < nn {
                                out[(4 * gi + l) * D + k] = r.0[l];
                            }
                        }
                    }
                }
                out
            }
        }
        let t = voigt::voigt_table(dim).unwrap();
        let mut result: Option<Vec<Real>> = None;
        for level in akantu_core::exec::SimdLevel::supported() {
            let token = level.token().unwrap();
            let r = match dim {
                1 => with_simd(token, Task::<1, 1>(nn, dndx, gu, [pk2[0]])),
                2 => {
                    let sv = std::array::from_fn(|k| {
                        let (i, j) = t.vec(k);
                        pk2[i + j * 2]
                    });
                    with_simd(token, Task::<2, 3>(nn, dndx, gu, sv))
                }
                _ => {
                    let sv = std::array::from_fn(|k| {
                        let (i, j) = t.vec(k);
                        pk2[i + j * 3]
                    });
                    with_simd(token, Task::<3, 6>(nn, dndx, gu, sv))
                }
            };
            if let Some(prev) = &result {
                for (a, b) in prev.iter().zip(&r) {
                    assert!(same(*a, *b), "{level:?}: {a:e} vs {b:e}");
                }
            } else {
                result = Some(r);
            }
        }
        result.unwrap()
    }

    /// The fused residual integration of random batches (filter, threads,
    /// SIMD levels) against the per-point residuals of [`point`] integrated
    /// by the general product path.
    #[test]
    fn residuals_integrate_matches_points_then_integrate_bitwise() {
        use akantu_core::exec::{Exec, ExecPolicy, Grain, SimdLevel};
        let mut v = Values::new(0x9c3a_55f0_7d21_e4b3);
        let levels = SimdLevel::supported();
        let mut elements = 0;
        for (case, (dim, nn, nq)) in [
            (1, 2, 1),
            (2, 3, 1),
            (2, 4, 4),
            (3, 4, 1),
            (3, 10, 4),
            (3, 8, 8),
            (3, 20, 27),
        ]
        .into_iter()
        .cycle()
        .take(70)
        .enumerate()
        {
            let t = voigt::voigt_table(dim).unwrap();
            let s = Sizes {
                dim,
                nn,
                nq,
                ts: t.size,
                bt: dim * nn,
            };
            let ne = 40;
            let filter: Vec<Idx> = (0..ne as Idx)
                .filter(|_| !v.next_u64().is_multiple_of(4))
                .collect();
            let np = filter.len() * nq;
            if case % 9 == 8 {
                // payload mode: one NaN among finite inputs
                let k = 1 + v.next_u64() % (ne * nq * (s.bt + 1) + 2 * np * dim * dim) as u64;
                v.payload_at(k);
            }
            let data = v.vec(ne * nq * s.bt);
            let jac = v.vec(ne * nq);
            let gs = v.vec(np * dim * dim);
            let ps = v.vec(np * dim * dim);
            v.default_mode();
            let dsh = Derivatives {
                data: &data,
                stride: s.bt,
                nq,
                nb_elements: ne,
            };
            let p = ExecPolicy::serial()
                .with_threads(1 + case % 4)
                .unwrap()
                .with_simd(levels[case % levels.len()])
                .unwrap();
            let exec = Exec::new(p.with_grain(Grain::Finest)).unwrap();
            let mut fused = vec![Real::NAN; filter.len() * s.bt];
            residuals_integrate(&exec, &s, t, &dsh, &filter, &gs, &ps, &jac, &mut fused).unwrap();
            let mut general = vec![0.0; fused.len()];
            let d2 = dim * dim;
            for (pos, &e) in filter.iter().enumerate() {
                let mut pts = vec![0.0; nq * s.bt];
                for q in 0..nq {
                    let tq = pos * nq + q;
                    pts[q * s.bt..(q + 1) * s.bt].copy_from_slice(&fast_point(
                        dim,
                        nn,
                        dsh.at(e, q),
                        &gs[tq * d2..(tq + 1) * d2],
                        &ps[tq * d2..(tq + 1) * d2],
                    ));
                }
                let e = e as usize;
                akantu_fe::eigen::product_slices(
                    Dims::dynamic(s.bt, nq),
                    Dims::new(nq, 1, false, true),
                    &pts,
                    &jac[e * nq..(e + 1) * nq],
                    &mut general[pos * s.bt..(pos + 1) * s.bt],
                )
                .unwrap();
            }
            for (k, (a, b)) in fused.iter().zip(&general).enumerate() {
                assert!(
                    same(*a, *b),
                    "case {case} dim {dim} entry {k}: {a:e} vs {b:e}"
                );
            }
            elements += filter.len();
        }
        assert!(elements >= 2_000, "{elements}");
    }

    /// All shapes; `payload`: one NaN per point among `dN/dX`, `∇u`, `S`.
    /// Returns (points, NaN entries).
    fn residual_points(v: &mut Values, payload: bool, per_shape: usize) -> (usize, usize) {
        let (mut points, mut nans) = (0, 0);
        for dim in 1..=3 {
            for nn in [2, 3, 4, 6, 8, 10, 15, 20] {
                let bt = dim * nn;
                let ts = voigt::voigt_table(dim).unwrap().size;
                assert_eq!(
                    product_route(Dims::fixed(ts, bt).t(), Dims::fixed(ts, 1)),
                    Route::Coeff(Reduction::Tree)
                );
                for _ in 0..per_shape {
                    if payload {
                        let k = 1 + v.next_u64() % (dim * nn + 2 * dim * dim) as u64;
                        v.payload_at(k);
                    }
                    let dndx = v.vec(dim * nn);
                    let gu = v.vec(dim * dim);
                    let pk2 = v.vec(dim * dim);
                    v.default_mode();
                    let f = fast_point(dim, nn, &dndx, &gu, &pk2);
                    let g = general_point(dim, nn, &dndx, &gu, &pk2);
                    let l = lanes_point(dim, nn, &dndx, &gu, &pk2);
                    for (k, (a, b)) in l.iter().zip(&g).enumerate() {
                        assert!(
                            same(*a, *b),
                            "lanes: dim {dim} nn {nn} entry {k}: {a:e} vs {b:e}"
                        );
                    }
                    for (k, (a, b)) in f.iter().zip(&g).enumerate() {
                        assert!(
                            same(*a, *b),
                            "dim {dim} nn {nn} entry {k}: {a:e} ({:#x}) vs {b:e} ({:#x})",
                            a.to_bits(),
                            b.to_bits()
                        );
                    }
                    nans += f.iter().filter(|x| x.is_nan()).count();
                    points += 1;
                }
            }
        }
        (points, nans)
    }

    #[test]
    fn residual_point_matches_general_path_bitwise() {
        let mut v = Values::new(0xa076_1d64_78bd_642f);
        let (points, _) = residual_points(&mut v, false, 4_500);
        assert!(points >= 100_000, "{points}");
    }

    #[test]
    fn residual_point_propagates_a_payload_nan_like_the_general_path() {
        let mut v = Values::new(0x2d35_8dcc_aa6c_78a5);
        let (points, nans) = residual_points(&mut v, true, 1_000);
        assert!(points >= 20_000 && nans > 0, "{points} points, {nans} NaNs");
    }
}
