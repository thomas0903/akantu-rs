// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/fe_engine/shape_lagrange_inline_impl.hh
// (gradientOnIntegrationPoints: `u_e * B.transpose()` per quadrature point),
// fe_engine_template_tmpl.hh (computeBtD: `D * B`), integrator_gauss_inline_impl.hh
// (integrate: `f * J` per element), with the Eigen 3.4 evaluation orders of
// those expressions (`crate::eigen`, `akantu_core::eigen_gebp`).

//! Specialised batch kernels for the hot FE products.
//!
//! The general path evaluates every per-point product with
//! [`crate::eigen::product_slices`]: route selection, shape checks, closures
//! and a recursive reduction **per product**. These kernels select the route
//! **once per batch** with the same function ([`crate::eigen::product_route`]
//! on the same operand descriptions), and only when it is the route they
//! implement evaluate the products with fixed-size loops and the
//! straight-line reductions of [`akantu_core::eigen_unrolled`], which perform
//! the same additions on the same operands in the same order. Any other
//! route, size or layout returns `None` and the caller runs the general path
//! unchanged, so the results are bitwise those of the general path in every
//! case (property tests below and in `tests/fast_kernels.rs`).
//!
//! Each kernel computes every output tuple (quadrature point or element) from
//! its own inputs only, so it splits its elements into contiguous parts run
//! by the engine's executor ([`Exec::for_each`], D-123): the same code per
//! output for every split, hence the same bits for every thread count.

use akantu_core::eigen_gebp::{GEMV_BLOCKING_MIN_DEPTH, GemvMatrix, Order, Reduction, Route};
use akantu_core::eigen_unrolled::{TREE_MAX, tree_sum_of};
use akantu_core::exec::{
    Exec, FastToken, FmaTask, Lanes4, SimdTask, SimdToken, Tuples, TuplesRef, with_fma, with_simd,
};
use akantu_core::{Idx, Real, Result};

use super::{Block, id_index};
use crate::eigen::{Dims, product_route};
use crate::element_class::LagrangeElement;

/// The largest of `v` read as unsigned (a negative index reads as at least
/// `2³¹`), `None` when empty: one vectorisable pass.
#[inline]
pub(super) fn max_unsigned(v: &[Idx]) -> Option<usize> {
    #[allow(clippy::cast_sign_loss)] // the reinterpretation is the point
    let m = v.iter().fold(0u32, |m, &x| m.max(x as u32));
    (!v.is_empty()).then_some(m as usize)
}

/// [`max_unsigned`] on `exec`'s threads: the maximum of each part, then
/// the maximum of those (an integer maximum: the same value for every
/// split).
pub(super) fn max_unsigned_with(exec: &Exec, v: &[Idx]) -> Option<usize> {
    // parts of at least 8,192 indices (a few microseconds: the check runs
    // once per batch, between threaded kernels)
    let parts = exec.nb_parts(v.len(), 8_192);
    if parts <= 1 {
        return max_unsigned(v);
    }
    let bounds: Vec<usize> = (0..=parts).map(|p| p * v.len() / parts).collect();
    let mut maxima = vec![0u32; parts];
    let per_part: Vec<usize> = (0..=parts).collect();
    exec.for_each_bounds(&per_part, Tuples::new(&mut maxima, 1), |p, mut m| {
        let part = &v[bounds[p]..bounds[p + 1]];
        // a value of `max_unsigned`, i.e. a u32
        m.as_mut_slice()[0] =
            max_unsigned(part).map_or(0, |x| u32::try_from(x).unwrap_or(u32::MAX));
    });
    let m = maxima.iter().fold(0u32, |a, &b| a.max(b));
    (!v.is_empty()).then_some(m as usize)
}

/// `true` when `ids` are consecutive (`ids[k] = ids[0] + k`).
fn consecutive(ids: &[Idx]) -> bool {
    let Some(&first) = ids.first() else {
        return true;
    };
    ids.iter().zip(0..).fold(true, |ok, (&e, k): (&Idx, Idx)| {
        ok & (e == first.wrapping_add(k))
    })
}

/// `true` when every node of the elements `ids` is a valid index of a
/// nodal field of `nb_nodes` tuples (`conn` holds `nnpe` nodes per element
/// and every id is an element of the block, checked by the caller). One
/// pass over the connectivity of consecutive ids, else one per element.
fn nodes_valid(conn: &[Idx], nnpe: usize, ids: &[Idx], nb_nodes: usize) -> bool {
    nodes_valid_with(Exec::serial_ref(), conn, nnpe, ids, nb_nodes)
}

/// [`nodes_valid`] with the pass over the connectivity of consecutive ids,
/// or the per-element passes, on `exec`'s threads.
fn nodes_valid_with(exec: &Exec, conn: &[Idx], nnpe: usize, ids: &[Idx], nb_nodes: usize) -> bool {
    match (ids.first(), ids.last()) {
        (Some(&a), Some(&b)) if consecutive(ids) => {
            let (a, b) = (id_index(a), id_index(b));
            conn.get(a * nnpe..(b + 1) * nnpe)
                .is_some_and(|r| max_unsigned_with(exec, r).is_none_or(|m| m < nb_nodes))
        }
        // one element's row is a few indices: its maximum serially, the
        // elements in parts (a logical and)
        _ => !any_with(exec, ids.len(), 8_192, |k| {
            let e = id_index(ids[k]);
            !conn
                .get(e * nnpe..(e + 1) * nnpe)
                .is_some_and(|r| max_unsigned(r).is_none_or(|m| m < nb_nodes))
        }),
    }
}

/// The checks of [`gradient`] without its connectivity check: route,
/// sizes, `nb_dof ∈ 1…3`.
fn gradient_shape_ok<E: LagrangeElement>(
    block: &Block<'_>,
    u: &[Real],
    nb_dof: usize,
    dshapes: &[Real],
    nq: usize,
) -> bool {
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    if nd == 0 || nq == 0 || nn > TREE_MAX || block.nnpe != nn {
        return false;
    }
    let route = product_route(Dims::new(nb_dof, nn, false, true), Dims::fixed(nd, nn).t());
    route == Route::Coeff(Reduction::Tree)
        && dshapes.len() == block.nb_element * nq * nd * nn
        && (1..=3).contains(&nb_dof)
        && u.len() == block.nb_nodes * nb_dof
}

/// `true` when [`gradient`] takes its kernel for every element of `ids`
/// (route, sizes and the connectivity of `ids`).
pub(super) fn gradient_supported<E: LagrangeElement>(
    exec: &Exec,
    block: &Block<'_>,
    u: &[Real],
    nb_dof: usize,
    dshapes: &[Real],
    nq: usize,
    ids: &[Idx],
) -> bool {
    gradient_shape_ok::<E>(block, u, nb_dof, dshapes, nq)
        && nodes_valid_with(exec, block.conn(), block.nnpe, ids, block.nb_nodes)
}

/// `∇u = U·Bᵀ` for every quadrature point of the elements `ids` (regular
/// kind, no cohesive reduction): the operands of `gradient_kernel`
/// (`U`: `Matrix<Dynamic, nn>` map, `B`: fixed `nd × nn` map, transposed),
/// route coefficient-based with the halving tree over the `nn` shape
/// functions. `ids` are elements of the block (checked by the caller);
/// `out` already has `ids.len()·nq` tuples of `nb_dof·nd`. `None` when the
/// route, `nb_dof` (1…3), `nn` (≤ 32), the storage layout or a node of the
/// connectivity is not the one this kernel implements; the general path then
/// reports the connectivity errors with the outputs of the elements before
/// the failing one written (with one thread nothing is written here before
/// the connectivity of the batch is checked).
#[allow(clippy::too_many_arguments)] // the general kernel's arguments
pub(super) fn gradient<E: LagrangeElement>(
    exec: &Exec,
    block: &Block<'_>,
    u: &[Real],
    nb_dof: usize,
    dshapes: &[Real],
    nq: usize,
    ids: &[Idx],
    out: &mut [Real],
) -> Option<Result<()>> {
    let nd = E::NATURAL_DIM;
    let nn = E::NB_SHAPES;
    if !gradient_shape_ok::<E>(block, u, nb_dof, dshapes, nq)
        || out.len() != ids.len() * nq * nb_dof * nd
    {
        return None;
    }
    // parts of at least ~2048 quadrature points (tens of microseconds)
    let min = (2048 / nq.max(1)).max(1);
    let conn = block.conn();
    let nb_nodes = block.nb_nodes;
    let items = (Tuples::new(out, nq * nb_dof * nd), TuplesRef::new(ids, 1));
    let fast = exec.fast_token();
    let simd = exec.simd_token();
    // a part with an invalid node writes nothing and reports it; the batch
    // then runs the general path
    let valid = std::sync::atomic::AtomicBool::new(true);
    exec.for_each(items, min, |_, (mut o, ids)| {
        let (o, ids) = (o.as_mut_slice(), ids.as_slice());
        if !nodes_valid(conn, nn, ids, nb_nodes) {
            valid.store(false, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        let g = Gradient {
            conn,
            u,
            dshapes,
            nq,
            ids,
            minus: &[],
            out: o,
            element: std::marker::PhantomData::<E>,
        };
        run_gradient(fast, simd, nb_dof, g);
    });
    valid
        .load(std::sync::atomic::Ordering::Relaxed)
        .then_some(Ok(()))
}

/// The operands of one part of [`gradient`] (nodes checked): with a
/// non-empty `minus` (one value per output value), every stored value is
/// `fl(∇u − minus)`, the subtraction `grad_u -= eigen_grad_u` of the solid
/// laws applied to the rounded gradient as it is written.
pub(super) struct Gradient<'a, E> {
    pub(super) conn: &'a [Idx],
    pub(super) u: &'a [Real],
    pub(super) dshapes: &'a [Real],
    pub(super) nq: usize,
    pub(super) ids: &'a [Idx],
    pub(super) minus: &'a [Real],
    pub(super) out: &'a mut [Real],
    pub(super) element: std::marker::PhantomData<E>,
}

/// Run one part of the gradient: the FMA variant in fast mode (then the
/// subtraction of `minus` as a separate pass), the bitwise lane kernel
/// otherwise, for `nb_dof ∈ 1…3`.
#[inline(always)]
pub(super) fn run_gradient<E: LagrangeElement>(
    fast: Option<FastToken>,
    simd: SimdToken,
    nb_dof: usize,
    g: Gradient<'_, E>,
) {
    match (fast, nb_dof, g.minus.is_empty()) {
        // FAST MODE (D-122): the FMA demonstrator, validated by
        // `akantu-solid/tests/fast_mode_gradient.rs`
        (Some(t), 1, true) => with_fma(t, LaneGradient::<E, 1, false, true>(g)),
        (Some(t), 2, true) => with_fma(t, LaneGradient::<E, 2, false, true>(g)),
        (Some(t), _, true) => with_fma(t, LaneGradient::<E, 3, false, true>(g)),
        (Some(t), 1, false) => with_fma(t, LaneGradient::<E, 1, true, true>(g)),
        (Some(t), 2, false) => with_fma(t, LaneGradient::<E, 2, true, true>(g)),
        (Some(t), _, false) => with_fma(t, LaneGradient::<E, 3, true, true>(g)),
        (None, 1, true) => with_simd(simd, LaneGradient::<E, 1, false, false>(g)),
        (None, 2, true) => with_simd(simd, LaneGradient::<E, 2, false, false>(g)),
        (None, _, true) => with_simd(simd, LaneGradient::<E, 3, false, false>(g)),
        (None, 1, false) => with_simd(simd, LaneGradient::<E, 1, true, false>(g)),
        (None, 2, false) => with_simd(simd, LaneGradient::<E, 2, true, false>(g)),
        (None, _, false) => with_simd(simd, LaneGradient::<E, 3, true, false>(g)),
    }
}

/// The gradient with lanes over the `NBD` components of `U` (one lane per
/// row `i` of `∇u`): without `FMA` (the bitwise default) each lane the
/// scalar sequence `tree_l(U(i, l)·B(j, l))` of the general path; with
/// `FMA` (FAST MODE, D-122, the demonstrator of the fast-mode harness, only
/// through [`with_fma`]) each entry is a chain of fused multiply-adds in the
/// order `l = 0, 1, …` (one rounding per step), bounded against the default
/// by the rule of `fixtures/fast_mode/solid/tolerances.json`. Then
/// `− minus` when `MINUS`.
struct LaneGradient<'a, E, const NBD: usize, const MINUS: bool, const FMA: bool>(Gradient<'a, E>);

impl<E: LagrangeElement, const NBD: usize, const MINUS: bool, const FMA: bool> FmaTask
    for LaneGradient<'_, E, NBD, MINUS, FMA>
{
    type Output = ();

    #[inline(always)]
    fn run(self) {
        SimdTask::run(self);
    }
}

impl<E: LagrangeElement, const NBD: usize, const MINUS: bool, const FMA: bool> SimdTask
    for LaneGradient<'_, E, NBD, MINUS, FMA>
{
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Gradient {
            conn,
            u,
            dshapes,
            nq,
            ids,
            minus,
            out,
            ..
        } = self.0;
        let nd = E::NATURAL_DIM;
        let nn = E::NB_SHAPES;
        let bsz = nd * nn;
        let tsz = NBD * nd;
        let (u, _) = u.as_chunks::<NBD>();
        let mut ue = [Lanes4::default(); TREE_MAX];
        let mut p = [Lanes4::default(); TREE_MAX];
        for (k, (&e, oe)) in ids.iter().zip(out.chunks_exact_mut(nq * tsz)).enumerate() {
            let e = id_index(e);
            // extractNodalToElementField (nodes checked by the caller)
            for (ue_n, &node) in ue.iter_mut().zip(&conn[e * nn..(e + 1) * nn]) {
                *ue_n = Lanes4::from_prefix(&u[id_index(node)]);
            }
            let de = &dshapes[e * nq * bsz..(e + 1) * nq * bsz];
            // the element's values of `minus` (none without MINUS)
            let me = if MINUS {
                &minus[k * nq * tsz..(k + 1) * nq * tsz]
            } else {
                &[]
            };
            for (t, (b, g)) in de
                .chunks_exact(bsz)
                .zip(oe.chunks_exact_mut(tsz))
                .enumerate()
            {
                for (j, gj) in g.as_chunks_mut::<NBD>().0.iter_mut().enumerate() {
                    // lhs(i, l) = U(i, l), rhs(l, j) = B(j, l)
                    let mut v = if FMA {
                        let mut acc = ue[0] * Lanes4::splat(b[j]);
                        for (l, ue_l) in ue.iter().enumerate().take(nn).skip(1) {
                            acc = ue_l.mul_add(Lanes4::splat(b[j + l * nd]), acc);
                        }
                        acc
                    } else {
                        for (l, pl) in p.iter_mut().enumerate().take(nn) {
                            *pl = ue[l] * Lanes4::splat(b[j + l * nd]);
                        }
                        tree_sum_of(&p, nn)
                    };
                    if MINUS {
                        // grad_u -= eigen_grad_u on the rounded gradient
                        let (m, _) = me[(t * nd + j) * NBD..].as_chunks::<NBD>();
                        v = v - Lanes4::from_prefix(&m[0]);
                    }
                    gj.copy_from_slice(&v.prefix::<NBD>());
                }
            }
        }
    }
}

/// `true` when `ids` (elements of a block of `nb_element`, checked by the
/// caller) can index `nq` shape-derivative tuples of `bsz` values each.
fn derivatives_cover(exec: &Exec, ids: &[Idx], nq: usize, bsz: usize, bs: &[Real]) -> bool {
    max_unsigned_with(exec, ids).is_none_or(|m| (m + 1).saturating_mul(nq * bsz) <= bs.len())
}

/// `true` when `pred(k)` holds for some `k` in `0..n`: the parts of
/// `exec` each test their range (a logical or: the same answer for every
/// split).
fn any_with(exec: &Exec, n: usize, min: usize, pred: impl Fn(usize) -> bool + Sync) -> bool {
    let parts = exec.nb_parts(n, min);
    if parts <= 1 {
        return (0..n).any(pred);
    }
    let bounds: Vec<usize> = (0..=parts).map(|p| p * n / parts).collect();
    let mut found = vec![false; parts];
    let per_part: Vec<usize> = (0..=parts).collect();
    exec.for_each_bounds(&per_part, Tuples::new(&mut found, 1), |p, mut f| {
        f.as_mut_slice()[0] = (bounds[p]..bounds[p + 1]).any(&pred);
    });
    found.contains(&true)
}

/// `D·B` per quadrature point (upstream `computeBtD`): `D` dynamic
/// `r × sdim`, `B` dynamic `sdim × nn`, both column-major (the operands of
/// `FeEngine::compute_btd`), route coefficient-based with the sequential
/// order (`r + sdim + nn < 20`). `bs` holds the shape derivatives of every
/// mesh element (`sdim·nn` per point), `ds` and `os` the filtered points.
/// `None` for another route or `(r, sdim)` outside `{(1, 1…3), (2, 2),
/// (3, 3)}`.
#[allow(clippy::too_many_arguments)]
pub(super) fn btd(
    exec: &Exec,
    r: usize,
    sdim: usize,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    ds: &[Real],
    os: &mut [Real],
) -> Option<()> {
    let route = product_route(Dims::dynamic(r, sdim), Dims::dynamic(sdim, nn));
    if route != Route::Coeff(Reduction::Sequential)
        || sdim > TREE_MAX
        || nq == 0
        || r == 0
        || nn == 0
    {
        return None;
    }
    let np = ids.len() * nq;
    if ds.len() != np * r * sdim || os.len() != np * r * nn {
        return None;
    }
    if !derivatives_cover(exec, ids, nq, sdim * nn, bs) {
        return None;
    }
    let run: BtdPart = match (r, sdim) {
        (1, 1) => btd_part::<1, 1>,
        (1, 2) => btd_part::<1, 2>,
        (1, 3) => btd_part::<1, 3>,
        (2, 2) => btd_part::<2, 2>,
        (3, 3) => btd_part::<3, 3>,
        _ => return None,
    };
    let min = (2048 / nq.max(1)).max(1);
    let items = (
        Tuples::new(os, nq * r * nn),
        TuplesRef::new(ds, nq * r * sdim),
        TuplesRef::new(ids, 1),
    );
    let simd = exec.simd_token();
    exec.for_each(items, min, |_, (mut o, d, ids)| {
        run(
            simd,
            nn,
            ids.as_slice(),
            nq,
            bs,
            d.as_slice(),
            o.as_mut_slice(),
        );
    });
    Some(())
}

/// One part of [`btd`]: `(simd, nn, ids, nq, bs, ds, os)`.
type BtdPart = fn(SimdToken, usize, &[Idx], usize, &[Real], &[Real], &mut [Real]);

// Not inlined into the part closure: as a function its slices are
// no-alias parameters (inlined into the closure, the kernels ran up to 1.6
// times slower at one thread).
#[inline(never)]
fn btd_part<const R: usize, const S: usize>(
    simd: SimdToken,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    ds: &[Real],
    os: &mut [Real],
) {
    with_simd(
        simd,
        LaneBtd::<R, S> {
            nn,
            ids,
            nq,
            bs,
            ds,
            os,
        },
    );
}

/// The columns `D(·, l)` of one point's `R × S` operand as lanes (lane `i`
/// = row `i`).
#[inline(always)]
fn d_columns<const R: usize, const S: usize>(d: &[Real]) -> [Lanes4; S] {
    let (cols, _) = d.as_chunks::<R>();
    std::array::from_fn(|l| Lanes4::from_prefix(&cols[l]))
}

/// One column of `D·B`: `Σ_l D(·, l)·B(l, j)` per lane, sequential from
/// the first term (`Reduction::Sequential`).
#[inline(always)]
fn db_column<const S: usize>(dcol: &[Lanes4; S], bj: &[Real]) -> Lanes4 {
    let mut acc = dcol[0] * Lanes4::splat(bj[0]);
    for l in 1..S {
        acc = acc + dcol[l] * Lanes4::splat(bj[l]); // r = r + D(i, l)·B(l, j)
    }
    acc
}

/// FAST MODE (D-122): [`db_column`] as a chain of fused multiply-adds.
#[inline(always)]
fn db_column_fma<const S: usize>(dcol: &[Lanes4; S], bj: &[Real]) -> Lanes4 {
    let mut acc = dcol[0] * Lanes4::splat(bj[0]);
    for l in 1..S {
        acc = dcol[l].mul_add(Lanes4::splat(bj[l]), acc);
    }
    acc
}

/// `D·B` of the points of a part, lanes over the `R` rows of `D`.
struct LaneBtd<'a, const R: usize, const S: usize> {
    nn: usize,
    ids: &'a [Idx],
    nq: usize,
    bs: &'a [Real],
    ds: &'a [Real],
    os: &'a mut [Real],
}

impl<const R: usize, const S: usize> SimdTask for LaneBtd<'_, R, S> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Self {
            nn,
            ids,
            nq,
            bs,
            ds,
            os,
        } = self;
        let bsz = S * nn;
        for ((&e, de), oe) in ids
            .iter()
            .zip(ds.chunks_exact(nq * R * S))
            .zip(os.chunks_exact_mut(nq * R * nn))
        {
            let e = id_index(e);
            let be = &bs[e * nq * bsz..(e + 1) * nq * bsz];
            for ((b, d), o) in be
                .chunks_exact(bsz)
                .zip(de.chunks_exact(R * S))
                .zip(oe.chunks_exact_mut(R * nn))
            {
                // lhs(i, l) = D(i, l), rhs(l, j) = B(l, j)
                let dcol = d_columns::<R, S>(d);
                for (bj, oj) in b.as_chunks::<S>().0.iter().zip(o.as_chunks_mut::<R>().0) {
                    oj.copy_from_slice(&db_column(&dcol, bj).prefix::<R>());
                }
            }
        }
    }
}

/// [`btd`] followed by the integration of its point values over each
/// element (`out_e = Σ_q (D·B)_q · J_q`, the GEMV order of [`integrate`]
/// with `nb_dof = r·nn`), without the per-point array: every point value
/// is the value [`btd`] stores, and it enters the element's accumulators
/// in the order [`integrate`] reads it back, so `os` receives the bits of
/// `btd` then `integrate`. `jac` holds `det J · w` of every mesh element
/// (`nq` per element), `os` one tuple of `r·nn` per element of `ids`. `None`
/// when either route or a size is not the one of the two kernels.
#[allow(clippy::too_many_arguments)]
pub(super) fn btd_integrate(
    exec: &Exec,
    r: usize,
    sdim: usize,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    ds: &[Real],
    jac: &[Real],
    os: &mut [Real],
) -> Option<()> {
    let nb_dof = r * nn;
    if !btd_integrate_supported(exec, r, sdim, nn, ids, nq, bs, jac)
        || ds.len() != ids.len() * nq * r * sdim
        || os.len() != ids.len() * nb_dof
    {
        return None;
    }
    let run = btd_integrate_kernel(r, sdim)?;
    let min = (2048 / nq.max(1)).max(1);
    let items = (
        Tuples::new(os, nb_dof),
        TuplesRef::new(ds, nq * r * sdim),
        TuplesRef::new(ids, 1),
    );
    let (simd, fast) = (exec.simd_token(), exec.fast_token());
    exec.for_each(items, min, |_, (mut o, d, ids)| {
        run(
            simd,
            fast,
            nn,
            ids.as_slice(),
            nq,
            bs,
            d.as_slice(),
            jac,
            o.as_mut_slice(),
        );
    });
    Some(())
}

/// `true` when [`btd_integrate`] takes its kernel for the elements `ids`
/// (both routes, sizes, the shape derivatives and Jacobians of `ids`).
#[allow(clippy::too_many_arguments)] // the fused kernel's operands and its executor
pub(super) fn btd_integrate_supported(
    exec: &Exec,
    r: usize,
    sdim: usize,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    jac: &[Real],
) -> bool {
    let btd_route = product_route(Dims::dynamic(r, sdim), Dims::dynamic(sdim, nn));
    let int_route = product_route(Dims::dynamic(r * nn, nq), Dims::new(nq, 1, false, true));
    let gemv = Route::Gemv {
        matrix: GemvMatrix::Lhs,
        kernel: Order::ColMajor,
    };
    btd_route == Route::Coeff(Reduction::Sequential)
        && int_route == gemv
        && nq > 0
        && nq < GEMV_BLOCKING_MIN_DEPTH
        && r > 0
        && nn > 0
        && nn <= TREE_MAX
        && btd_integrate_kernel(r, sdim).is_some()
        && derivatives_cover(exec, ids, nq, sdim * nn, bs)
        && derivatives_cover(exec, ids, nq, 1, jac)
}

/// The fused kernel of `(r, sdim)`.
fn btd_integrate_kernel(r: usize, sdim: usize) -> Option<BtdIntegratePart> {
    Some(match (r, sdim) {
        (1, 1) => btd_integrate_part::<1, 1>,
        (1, 2) => btd_integrate_part::<1, 2>,
        (1, 3) => btd_integrate_part::<1, 3>,
        (2, 2) => btd_integrate_part::<2, 2>,
        (3, 3) => btd_integrate_part::<3, 3>,
        _ => return None,
    })
}

/// [`btd_integrate`] of the elements `ids` on the calling thread, for a
/// caller that checked [`btd_integrate_supported`] (a plan); does nothing
/// for an unsupported `(r, sdim)`.
#[allow(clippy::too_many_arguments)]
pub(super) fn btd_integrate_serial(
    simd: SimdToken,
    fast: Option<FastToken>,
    r: usize,
    sdim: usize,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    ds: &[Real],
    jac: &[Real],
    os: &mut [Real],
) {
    if let Some(run) = btd_integrate_kernel(r, sdim) {
        run(simd, fast, nn, ids, nq, bs, ds, jac, os);
    }
}

/// One part of [`btd_integrate`].
type BtdIntegratePart =
    fn(SimdToken, Option<FastToken>, usize, &[Idx], usize, &[Real], &[Real], &[Real], &mut [Real]);

#[inline(never)] // as `btd_part`
#[allow(clippy::too_many_arguments)]
fn btd_integrate_part<const R: usize, const S: usize>(
    simd: SimdToken,
    fast: Option<FastToken>,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    ds: &[Real],
    jac: &[Real],
    os: &mut [Real],
) {
    match fast {
        // FAST MODE (D-122): fused multiply-adds in D·B and in the
        // integration, validated by `akantu-solid/tests/fast_mode_gradient.rs`
        Some(t) => with_fma(
            t,
            LaneBtdIntegrate::<R, S, true> {
                nn,
                ids,
                nq,
                bs,
                ds,
                jac,
                os,
            },
        ),
        None => with_simd(
            simd,
            LaneBtdIntegrate::<R, S, false> {
                nn,
                ids,
                nq,
                bs,
                ds,
                jac,
                os,
            },
        ),
    }
}

/// [`btd_integrate`] of the elements of a part, lanes over the `R` rows of
/// `D`. With `FMA` (fast mode only, through [`with_fma`]) the sums of
/// `D·B` and the integration are chains of fused multiply-adds.
struct LaneBtdIntegrate<'a, const R: usize, const S: usize, const FMA: bool> {
    nn: usize,
    ids: &'a [Idx],
    nq: usize,
    bs: &'a [Real],
    ds: &'a [Real],
    jac: &'a [Real],
    os: &'a mut [Real],
}

impl<const R: usize, const S: usize, const FMA: bool> FmaTask for LaneBtdIntegrate<'_, R, S, FMA> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        SimdTask::run(self);
    }
}

impl<const R: usize, const S: usize, const FMA: bool> SimdTask for LaneBtdIntegrate<'_, R, S, FMA> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Self {
            nn,
            ids,
            nq,
            bs,
            ds,
            jac,
            os,
        } = self;
        let bsz = S * nn;
        let mut c = [Lanes4::default(); TREE_MAX];
        for ((&e, de), oe) in ids
            .iter()
            .zip(ds.chunks_exact(nq * R * S))
            .zip(os.chunks_exact_mut(R * nn))
        {
            let e = id_index(e);
            let be = &bs[e * nq * bsz..(e + 1) * nq * bsz];
            let je = &jac[e * nq..(e + 1) * nq];
            // the GEMV accumulators of the element: c = 0, then for every
            // point c = c + fl((D·B)_q · J_q)
            c[..nn].fill(Lanes4::splat(0.0));
            for ((b, d), &jq) in be.chunks_exact(bsz).zip(de.chunks_exact(R * S)).zip(je) {
                let dcol = d_columns::<R, S>(d);
                let jq = Lanes4::splat(jq);
                for (cj, bj) in c.iter_mut().zip(b.as_chunks::<S>().0) {
                    *cj = if FMA {
                        db_column_fma(&dcol, bj).mul_add(jq, *cj)
                    } else {
                        *cj + db_column(&dcol, bj) * jq
                    };
                }
            }
            // res = 0 + c (zeroed destination)
            for (oj, &cj) in oe.as_chunks_mut::<R>().0.iter_mut().zip(&c) {
                oj.copy_from_slice(&(Lanes4::splat(0.0) + cj).prefix::<R>());
            }
        }
    }
}

/// Per-element integrals of a **scalar** field (`nb_dof = 1`): the route
/// of `f_e · J_e` (`f_e` dynamic `1 × nq`, `J_e` a `Dynamic × 1` vector) is
/// the run-time `1 × 1` inner-product fallback with the sequential order
/// (`out = 0; out = out + (t0 + t1 + …)`, `t_l = fl(f(l)·J(l))`). `None`
/// for another route (the caller then runs the general path).
pub(super) fn integrate_scalar<'j>(
    f: &[Real],
    nq: usize,
    jac: &impl Fn(usize) -> &'j [Real],
    nb_element: usize,
    o: &mut [Real],
) -> Option<()> {
    let route = product_route(Dims::dynamic(1, nq), Dims::new(nq, 1, false, true));
    if route != Route::Dot(Reduction::Sequential)
        || nq == 0
        || f.len() != nb_element * nq
        || o.len() != nb_element
    {
        return None;
    }
    if (0..nb_element).any(|pos| jac(pos).len() < nq) {
        return None;
    }
    for (pos, (fe, oe)) in f.chunks_exact(nq).zip(o.iter_mut()).enumerate() {
        let j = &jac(pos)[..nq];
        // r = t0; r = r + t_l (Reduction::Sequential)
        let mut r = fe[0] * j[0];
        for (&fl, &jl) in fe[1..].iter().zip(&j[1..]) {
            r += fl * jl;
        }
        // dst(0, 0) = 0; dst(0, 0) += r
        let mut d = 0.0;
        d += r;
        *oe = d;
    }
    Some(())
}

/// Per-element integrals `out_e = f_e · J_e` (`f_e` dynamic `nb_dof × nq`,
/// `J_e` a `Dynamic × 1` vector; `integrate_with_jacobians`) when the route
/// is the column-major GEMV kernel without depth blocking (`nq < 128`):
/// `c = 0; c = c + f(i, l)·J(l)` over `l`, then `out(i) = 0 + c`. `None`
/// otherwise (e.g. the `1 × 1` inner-product fallback of a scalar field).
pub(super) fn integrate<'j>(
    exec: &Exec,
    f: &[Real],
    nb_dof: usize,
    nq: usize,
    jac: &(impl Fn(usize) -> &'j [Real] + Sync),
    nb_element: usize,
    o: &mut [Real],
) -> Option<()> {
    let route = product_route(Dims::dynamic(nb_dof, nq), Dims::new(nq, 1, false, true));
    let gemv = Route::Gemv {
        matrix: GemvMatrix::Lhs,
        kernel: Order::ColMajor,
    };
    if route != gemv
        || nb_dof == 0
        || nq == 0
        || nq >= GEMV_BLOCKING_MIN_DEPTH
        || f.len() != nb_element * nq * nb_dof
        || o.len() != nb_element * nb_dof
    {
        return None;
    }
    if any_with(exec, nb_element, 8_192, |pos| jac(pos).len() < nq) {
        return None;
    }
    let min = (4096 / (nq * nb_dof).max(1)).max(1);
    let items = (Tuples::new(o, nb_dof), TuplesRef::new(f, nq * nb_dof));
    let (simd, fast) = (exec.simd_token(), exec.fast_token());
    exec.for_each(items, min, |first, (mut o, f)| {
        integrate_part(
            simd,
            fast,
            f.as_slice(),
            nb_dof,
            nq,
            |pos| jac(first + pos),
            o.as_mut_slice(),
        );
    });
    Some(())
}

/// [`integrate`] of the elements whose values are `f` and whose integrals
/// go to `o` (`jac(pos)` the weighted Jacobians of the part's `pos`-th
/// element, checked by the caller).
#[inline(never)] // as `btd_part`
fn integrate_part<'j>(
    simd: SimdToken,
    fast: Option<FastToken>,
    f: &[Real],
    nb_dof: usize,
    nq: usize,
    jac: impl Fn(usize) -> &'j [Real],
    o: &mut [Real],
) {
    match fast {
        // FAST MODE (D-122): the accumulation as fused multiply-adds
        Some(t) => with_fma(
            t,
            LaneIntegrate::<_, true> {
                f,
                nb_dof,
                nq,
                jac,
                o,
            },
        ),
        None => with_simd(
            simd,
            LaneIntegrate::<_, false> {
                f,
                nb_dof,
                nq,
                jac,
                o,
            },
        ),
    }
}

/// [`integrate_part`]'s body: the accumulators of one element are
/// independent lanes (one per component).
struct LaneIntegrate<'a, J, const FMA: bool> {
    f: &'a [Real],
    nb_dof: usize,
    nq: usize,
    jac: J,
    o: &'a mut [Real],
}

impl<'j, J: Fn(usize) -> &'j [Real], const FMA: bool> FmaTask for LaneIntegrate<'_, J, FMA> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        SimdTask::run(self);
    }
}

impl<'a, 'j, J: Fn(usize) -> &'j [Real], const FMA: bool> SimdTask for LaneIntegrate<'a, J, FMA> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Self {
            f,
            nb_dof,
            nq,
            jac,
            o,
        } = self;
        // accumulators of one element: c(i) = 0; then, for l = 0, 1, …,
        // c(i) = c(i) + fl(f(i, l)·J(l)) — every c(i) sees the same additions
        // in the same order as the per-row loop of the GEMV kernel, the
        // loops are only interchanged (contiguous columns of f)
        let mut c = vec![0.0; nb_dof];
        for (pos, (fe, oe)) in f
            .chunks_exact(nq * nb_dof)
            .zip(o.chunks_exact_mut(nb_dof))
            .enumerate()
        {
            let j = &jac(pos)[..nq];
            c.fill(0.0);
            for (col, &jl) in fe.chunks_exact(nb_dof).zip(j) {
                for (ci, &fl) in c.iter_mut().zip(col) {
                    if FMA {
                        *ci = fl.mul_add(jl, *ci);
                    } else {
                        *ci += fl * jl; // c = c + fl(f·J)
                    }
                }
            }
            for (oi, &ci) in oe.iter_mut().zip(&c) {
                *oi = 0.0;
                *oi += ci; // res = res + c (zeroed destination)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    //! Bitwise property tests: every fast kernel against the general
    //! per-product path (`crate::eigen::product_slices` with the operand
    //! descriptions of the call site) on random inputs including signed
    //! zeros, infinities, NaNs with payloads and subnormals.

    use super::*;
    use crate::eigen::product_slices;
    use crate::element_class::ElementClass;
    use crate::with_element_class;
    use akantu_core::array_shared::ArrayRef;
    use akantu_core::exec::{ExecPolicy, Grain, SimdLevel};
    use akantu_core::{Array, ElementType, GhostType, Idx};

    /// The threaded `any` of the plans' checks equals the serial one at
    /// every thread count and grain, wherever the matching index lies.
    #[test]
    fn threaded_any_equals_the_serial_one() {
        for n in [0usize, 1, 5, 70_000, 300_001] {
            let hits: Vec<Option<usize>> = vec![
                None,
                Some(0),
                Some(n / 2),
                n.checked_sub(1),
                Some(n / 3 + 1),
            ];
            for hit in hits {
                let pred = |k: usize| Some(k) == hit && k < n;
                let serial = (0..n).any(pred);
                for threads in [1, 2, 3, 4, 8] {
                    for grain in [Grain::Finest, Grain::Auto] {
                        let exec = Exec::new(
                            ExecPolicy::serial()
                                .with_threads(threads)
                                .unwrap()
                                .with_grain(grain),
                        )
                        .unwrap();
                        assert_eq!(
                            any_with(&exec, n, 8_192, pred),
                            serial,
                            "n {n} hit {hit:?} threads {threads} {grain:?}"
                        );
                    }
                }
            }
        }
    }

    /// The threaded maximum of the connectivity check equals the serial
    /// one at every thread count and grain, negative indices (huge as
    /// unsigned) included.
    #[test]
    fn threaded_maximum_equals_the_serial_one() {
        let mut x: u64 = 0x9e37_79b9_7f4a_7c15;
        let mut next = || {
            x ^= x << 13;
            x ^= x >> 7;
            x ^= x << 17;
            x
        };
        for len in [0usize, 1, 7, 65_535, 65_536, 300_001] {
            let mut v: Vec<Idx> = (0..len).map(|_| (next() % 1_000_000) as Idx).collect();
            for case in 0..3 {
                if case == 1 && len > 0 {
                    let k = (next() as usize) % len;
                    v[k] = -3;
                }
                if case == 2 && len > 0 {
                    v[len - 1] = 2_000_000;
                }
                let serial = max_unsigned(&v);
                for threads in [1, 2, 3, 4, 8] {
                    for grain in [Grain::Finest, Grain::Auto] {
                        let exec = Exec::new(
                            ExecPolicy::serial()
                                .with_threads(threads)
                                .unwrap()
                                .with_grain(grain),
                        )
                        .unwrap();
                        assert_eq!(
                            max_unsigned_with(&exec, &v),
                            serial,
                            "len {len} case {case} threads {threads} {grain:?}"
                        );
                    }
                }
            }
        }
    }

    /// The connectivity check of non-consecutive ids (one pass per element)
    /// gives the serial answer at every thread count and grain: a valid
    /// connectivity, a node out of range in one element, a negative node,
    /// an id past the connectivity.
    #[test]
    fn threaded_per_element_check_equals_the_serial_one() {
        let (nnpe, nb_el, nb_nodes) = (4usize, 40_000usize, 50_000usize);
        let base: Vec<Idx> = (0..nb_el * nnpe)
            .map(|k| ((k * 7_919) % nb_nodes) as Idx)
            .collect();
        // every other element: not consecutive
        let ids: Vec<Idx> = (0..nb_el as Idx).step_by(2).collect();
        let mut cases = vec![(base.clone(), ids.clone())];
        for e in [0usize, nb_el / 2, nb_el - 2] {
            let mut c = base.clone();
            c[e * nnpe + 3] = nb_nodes as Idx;
            cases.push((c, ids.clone()));
        }
        let mut c = base.clone();
        c[((nb_el / 3) & !1) * nnpe + 1] = -1;
        cases.push((c, ids.clone()));
        let mut far = ids.clone();
        far.push(nb_el as Idx);
        cases.push((base, far));
        for (k, (conn, ids)) in cases.iter().enumerate() {
            let serial = nodes_valid(conn, nnpe, ids, nb_nodes);
            assert_eq!(serial, k == 0, "case {k}");
            for threads in [1, 2, 3, 4, 8] {
                for grain in [Grain::Finest, Grain::Auto] {
                    let exec = Exec::new(
                        ExecPolicy::serial()
                            .with_threads(threads)
                            .unwrap()
                            .with_grain(grain),
                    )
                    .unwrap();
                    assert_eq!(
                        nodes_valid_with(&exec, conn, nnpe, ids, nb_nodes),
                        serial,
                        "case {k} threads {threads} {grain:?}"
                    );
                }
            }
        }
    }

    /// An executor of SIMD level `level` whose thread count (1 to 4) varies
    /// with the case and that splits every batch as finely as it can, so the
    /// property tests also cover the split batches (D-123).
    fn test_exec(salt: usize, level: SimdLevel) -> Exec {
        let n = 1 + salt % 4;
        let p = ExecPolicy::serial()
            .with_threads(n)
            .unwrap()
            .with_simd(level)
            .unwrap();
        Exec::new(p.with_grain(Grain::Finest)).unwrap()
    }

    /// Run `kernel` once per SIMD level the CPU supports (the scalar level
    /// first) into a fresh output of `len` NaNs: every level must take the
    /// batch or decline it alike and give the scalar level's bits. Returns
    /// the scalar level's output and answer.
    fn every_level<T: PartialEq + std::fmt::Debug>(
        salt: usize,
        len: usize,
        mut kernel: impl FnMut(&Exec, &mut [Real]) -> T,
    ) -> (Vec<Real>, T) {
        let mut first: Option<(Vec<Real>, T)> = None;
        for level in SimdLevel::supported() {
            let mut out = vec![Real::NAN; len];
            let t = kernel(&test_exec(salt, level), &mut out);
            match &first {
                None => first = Some((out, t)),
                Some((o, t0)) => {
                    assert_eq!(&t, t0, "{level:?}: the kernel's answer differs");
                    assert_bits(&out, o, &format!("{level:?} against the scalar level"));
                }
            }
        }
        first.expect("the scalar level is supported")
    }

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

        /// Filter of about ¾ of `0..ne`, in increasing order.
        fn ids(&mut self, ne: usize) -> Vec<Idx> {
            (0..ne as Idx)
                .filter(|_| !self.next_u64().is_multiple_of(4))
                .collect()
        }
    }

    /// Entry-wise [`same`]; returns the number of NaN entries.
    fn assert_bits(fast: &[Real], general: &[Real], what: &str) -> usize {
        assert_eq!(fast.len(), general.len(), "{what}: lengths");
        for (k, (a, b)) in fast.iter().zip(general).enumerate() {
            assert!(
                same(*a, *b),
                "{what}: entry {k}: {a:e} ({:#x}) vs {b:e} ({:#x})",
                a.to_bits(),
                b.to_bits()
            );
        }
        fast.iter().filter(|x| x.is_nan()).count()
    }

    const TYPES: [ElementType; 12] = [
        ElementType::Segment2,
        ElementType::Segment3,
        ElementType::Triangle3,
        ElementType::Triangle6,
        ElementType::Quadrangle4,
        ElementType::Quadrangle8,
        ElementType::Tetrahedron4,
        ElementType::Tetrahedron10,
        ElementType::Pentahedron6,
        ElementType::Pentahedron15,
        ElementType::Hexahedron8,
        ElementType::Hexahedron20,
    ];

    /// One random batch of `gradient::<E>` against the general path;
    /// returns (products compared, NaN entries). 0 products when the kernel
    /// declines, which must coincide with a route other than the tree.
    fn gradient_batch<E: LagrangeElement>(
        v: &mut Values,
        nb_dof: usize,
        nq: usize,
        ne: usize,
    ) -> (usize, usize) {
        let (nn, nd) = (E::NB_SHAPES, E::NATURAL_DIM);
        let nb_nodes = 40;
        let conn: Vec<Idx> = (0..ne * nn)
            .map(|_| (v.next_u64() % nb_nodes as u64) as Idx)
            .collect();
        let nodes = Array::from_vec(vec![0.0; nb_nodes * nd], nd).unwrap();
        let conn = Array::from_vec(conn, nn).unwrap();
        let block = Block {
            ty: E::TYPE,
            ghost: GhostType::NotGhost,
            nodes: ArrayRef::from(&nodes),
            nodes_override: None,
            nb_nodes,
            sdim: nd,
            conn: Some(ArrayRef::from(&conn)),
            nnpe: nn,
            nb_element: ne,
        };
        let u = v.vec(nb_nodes * nb_dof);
        let dsh = v.vec(ne * nq * nd * nn);
        let ids = v.ids(ne);
        let tsz = nb_dof * nd;
        let (fast, res) = every_level(ids.len(), ids.len() * nq * tsz, |ex, out| {
            gradient::<E>(ex, &block, &u, nb_dof, &dsh, nq, &ids, out).map(|r| r.is_ok())
        });
        let ud = Dims::new(nb_dof, nn, false, true);
        let bt = Dims::fixed(nd, nn).t();
        let tree = product_route(ud, bt) == Route::Coeff(Reduction::Tree);
        let Some(res) = res else {
            assert!(
                !tree || nb_dof > 3,
                "{}: declined a tree route",
                E::TYPE.name()
            );
            return (0, 0);
        };
        assert!(tree);
        assert!(res);
        let mut general = vec![0.0; fast.len()];
        let mut ue = vec![0.0; nb_dof * nn];
        let bsz = nd * nn;
        for (pos, &e) in ids.iter().enumerate() {
            let e = e as usize;
            block.gather(e, &u, nb_dof, &mut ue).unwrap();
            for q in 0..nq {
                let t = pos * nq + q;
                product_slices(
                    ud,
                    bt,
                    &ue,
                    &dsh[(e * nq + q) * bsz..(e * nq + q + 1) * bsz],
                    &mut general[t * tsz..(t + 1) * tsz],
                )
                .unwrap();
            }
        }
        let nans = assert_bits(&fast, &general, E::TYPE.name());
        (ids.len() * nq, nans)
    }

    /// All gradient batches; in payload mode one NaN per batch (somewhere
    /// in `u` or the shape derivatives). Returns (products, NaN entries).
    fn gradient_all(v: &mut Values, payload: bool, rounds: usize) -> (usize, usize) {
        let (mut products, mut nans) = (0, 0);
        for ty in TYPES {
            for nb_dof in 1..=4 {
                for nq in [1, 4, 8] {
                    for _ in 0..rounds {
                        if payload {
                            let draws = with_element_class!(ty, E => {
                                40 * nb_dof + 120 * nq * E::NATURAL_DIM * E::NB_SHAPES
                            }, else unreachable!());
                            let k = 1 + v.next_u64() % draws as u64;
                            v.payload_at(k);
                        }
                        let (p, n) = with_element_class!(ty, E => {
                            gradient_batch::<E>(v, nb_dof, nq, 120)
                        }, else unreachable!());
                        v.default_mode();
                        products += p;
                        nans += n;
                    }
                }
            }
        }
        (products, nans)
    }

    #[test]
    fn gradient_matches_general_path_bitwise() {
        let mut v = Values::new(0x0123_4567_89ab_cdef);
        let (products, _) = gradient_all(&mut v, false, 12);
        assert!(products >= 100_000, "{products} products compared");
    }

    #[test]
    fn gradient_propagates_a_payload_nan_like_the_general_path() {
        let mut v = Values::new(0x7a3c_91e2_0b55_d00d);
        let (products, nans) = gradient_all(&mut v, true, 3);
        assert!(
            products >= 20_000 && nans > 0,
            "{products} products, {nans} NaNs"
        );
    }

    /// The gradient with a subtracted array (the plans' `minus`) against
    /// the gradient then `out -= minus`, at every SIMD level.
    #[test]
    fn gradient_minus_matches_gradient_then_subtraction() {
        type E = crate::element_class::types::Hexahedron8;
        let mut v = Values::new(0x5eed_0f9a_1de7);
        let (nn, nd, nq, nb_nodes, ne) = (8, 3, 8, 30, 50);
        for _ in 0..40 {
            let conn: Vec<Idx> = (0..ne * nn)
                .map(|_| (v.next_u64() % nb_nodes as u64) as Idx)
                .collect();
            let nodes = Array::from_vec(vec![0.0; nb_nodes * nd], nd).unwrap();
            let conn = Array::from_vec(conn, nn).unwrap();
            let block = Block {
                ty: E::TYPE,
                ghost: GhostType::NotGhost,
                nodes: ArrayRef::from(&nodes),
                nodes_override: None,
                nb_nodes,
                sdim: nd,
                conn: Some(ArrayRef::from(&conn)),
                nnpe: nn,
                nb_element: ne,
            };
            let u = v.vec(nb_nodes * 3);
            let dsh = v.vec(ne * nq * nd * nn);
            let ids = v.ids(ne);
            let minus = v.vec(ids.len() * nq * 9);
            // every level: against the subtraction afterwards and against
            // the scalar level
            let mut scalar: Option<Vec<Real>> = None;
            for level in SimdLevel::supported() {
                let simd = level.token().unwrap();
                let mut a = vec![Real::NAN; minus.len()];
                let mut b = vec![Real::NAN; minus.len()];
                run_gradient(
                    None,
                    simd,
                    3,
                    Gradient {
                        conn: block.conn(),
                        u: &u,
                        dshapes: &dsh,
                        nq,
                        ids: &ids,
                        minus: &minus,
                        out: &mut a,
                        element: std::marker::PhantomData::<E>,
                    },
                );
                run_gradient(
                    None,
                    simd,
                    3,
                    Gradient {
                        conn: block.conn(),
                        u: &u,
                        dshapes: &dsh,
                        nq,
                        ids: &ids,
                        minus: &[],
                        out: &mut b,
                        element: std::marker::PhantomData::<E>,
                    },
                );
                for (o, m) in b.iter_mut().zip(&minus) {
                    *o -= *m;
                }
                assert_bits(&a, &b, "gradient minus");
                match &scalar {
                    None => scalar = Some(a),
                    Some(s0) => {
                        assert_bits(&a, s0, &format!("gradient minus, {level:?} against scalar"));
                    }
                }
            }
        }
    }

    #[test]
    fn gradient_reports_bad_connectivity_like_the_general_path() {
        type E = crate::element_class::types::Tetrahedron4;
        let conn: Array<Idx> = Array::from_vec(vec![0, 1, 2, 7], 4).unwrap();
        let nodes = Array::from_vec(vec![0.0; 4 * 3], 3).unwrap();
        let block = Block {
            ty: E::TYPE,
            ghost: GhostType::NotGhost,
            nodes: ArrayRef::from(&nodes),
            nodes_override: None,
            nb_nodes: 4,
            sdim: 3,
            conn: Some(ArrayRef::from(&conn)),
            nnpe: 4,
            nb_element: 1,
        };
        let u = vec![1.0; 12];
        let dsh = vec![1.0; 12];
        for salt in 0..8 {
            let mut out = vec![0.5; 9];
            // the kernel declines (the general path reports the error) and
            // writes nothing
            for level in SimdLevel::supported() {
                let ex = test_exec(salt, level);
                let fast = gradient::<E>(&ex, &block, &u, 3, &dsh, 1, &[0], &mut out);
                assert!(fast.is_none());
                assert_eq!(out, vec![0.5; 9]);
            }
        }
    }

    const BTD_PAIRS: [(usize, usize); 7] = [(1, 1), (1, 2), (1, 3), (2, 2), (3, 3), (2, 3), (6, 3)];
    const BTD_NN: [usize; 9] = [2, 3, 4, 6, 8, 10, 13, 15, 20];

    /// One random `btd` batch against the general path: (products, NaNs);
    /// checks that the kernel declines exactly the other routes. With
    /// `payload`, one NaN in the batch's inputs.
    fn btd_batch(
        v: &mut Values,
        r: usize,
        sdim: usize,
        nn: usize,
        nq: usize,
        payload: bool,
    ) -> (usize, usize) {
        let ne = 150;
        let ids = v.ids(ne);
        let np = ids.len() * nq;
        if payload {
            let k = 1 + v.next_u64() % (ne * nq * sdim * nn + np * r * sdim) as u64;
            v.payload_at(k);
        }
        let bs = v.vec(ne * nq * sdim * nn);
        let ds = v.vec(np * r * sdim);
        v.default_mode();
        let (fast, done) = every_level(ids.len(), np * r * nn, |ex, out| {
            btd(ex, r, sdim, nn, &ids, nq, &bs, &ds, out).is_some()
        });
        let seq = product_route(Dims::dynamic(r, sdim), Dims::dynamic(sdim, nn))
            == Route::Coeff(Reduction::Sequential);
        let supported = matches!((r, sdim), (1, 1..=3) | (2, 2) | (3, 3));
        assert_eq!(done, seq && supported, "r={r} sdim={sdim} nn={nn}");
        if !done {
            return (0, 0);
        }
        let mut general = vec![0.0; fast.len()];
        let bsz = sdim * nn;
        for (pos, &e) in ids.iter().enumerate() {
            let e = e as usize;
            for q in 0..nq {
                let t = pos * nq + q;
                product_slices(
                    Dims::dynamic(r, sdim),
                    Dims::dynamic(sdim, nn),
                    &ds[t * r * sdim..(t + 1) * r * sdim],
                    &bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz],
                    &mut general[t * r * nn..(t + 1) * r * nn],
                )
                .unwrap();
            }
        }
        (np, assert_bits(&fast, &general, "btd"))
    }

    #[test]
    fn btd_matches_general_path_bitwise() {
        let mut v = Values::new(0xfeed_beef_dead_cafe);
        let mut products = 0;
        for (r, sdim) in BTD_PAIRS {
            for nn in BTD_NN {
                for nq in [1, 4] {
                    for _ in 0..20 {
                        products += btd_batch(&mut v, r, sdim, nn, nq, false).0;
                    }
                }
            }
        }
        assert!(products >= 100_000, "{products} products compared");
    }

    #[test]
    fn btd_propagates_a_payload_nan_like_the_general_path() {
        let mut v = Values::new(0x0dd5_eed5_1234_9876);
        let (mut products, mut nans) = (0, 0);
        for (r, sdim) in BTD_PAIRS {
            for nn in BTD_NN {
                for nq in [1, 4] {
                    for _ in 0..4 {
                        let (p, n) = btd_batch(&mut v, r, sdim, nn, nq, true);
                        products += p;
                        nans += n;
                    }
                }
            }
        }
        assert!(
            products >= 20_000 && nans > 0,
            "{products} products, {nans} NaNs"
        );
    }

    /// One random fused `btd_integrate` batch against the general path
    /// (`D·B` per point, then the GEMV integral over the element's points):
    /// (elements compared, NaNs). Checks that the kernel declines exactly
    /// the other routes.
    fn btd_integrate_batch(
        v: &mut Values,
        r: usize,
        sdim: usize,
        nn: usize,
        nq: usize,
        payload: bool,
    ) -> (usize, usize) {
        let ne = 120;
        let ids = v.ids(ne);
        let np = ids.len() * nq;
        if payload {
            let k = 1 + v.next_u64() % (ne * nq * sdim * nn + np * r * sdim + ne * nq) as u64;
            v.payload_at(k);
        }
        let bs = v.vec(ne * nq * sdim * nn);
        let ds = v.vec(np * r * sdim);
        let jac = v.vec(ne * nq);
        v.default_mode();
        let nb_dof = r * nn;
        let (fast, done) = every_level(ids.len(), ids.len() * nb_dof, |ex, out| {
            btd_integrate(ex, r, sdim, nn, &ids, nq, &bs, &ds, &jac, out).is_some()
        });
        let seq = product_route(Dims::dynamic(r, sdim), Dims::dynamic(sdim, nn))
            == Route::Coeff(Reduction::Sequential);
        let gemv = product_route(Dims::dynamic(nb_dof, nq), Dims::new(nq, 1, false, true))
            == Route::Gemv {
                matrix: GemvMatrix::Lhs,
                kernel: Order::ColMajor,
            };
        let supported = matches!((r, sdim), (1, 1..=3) | (2, 2) | (3, 3));
        assert_eq!(
            done,
            seq && gemv && supported && nq < GEMV_BLOCKING_MIN_DEPTH,
            "r={r} sdim={sdim} nn={nn} nq={nq}"
        );
        if !done {
            return (0, 0);
        }
        let bsz = sdim * nn;
        let mut general = vec![0.0; fast.len()];
        let mut point = vec![0.0; nq * nb_dof];
        for (pos, &e) in ids.iter().enumerate() {
            let e = e as usize;
            for q in 0..nq {
                let t = pos * nq + q;
                product_slices(
                    Dims::dynamic(r, sdim),
                    Dims::dynamic(sdim, nn),
                    &ds[t * r * sdim..(t + 1) * r * sdim],
                    &bs[(e * nq + q) * bsz..(e * nq + q + 1) * bsz],
                    &mut point[q * nb_dof..(q + 1) * nb_dof],
                )
                .unwrap();
            }
            product_slices(
                Dims::dynamic(nb_dof, nq),
                Dims::new(nq, 1, false, true),
                &point,
                &jac[e * nq..(e + 1) * nq],
                &mut general[pos * nb_dof..(pos + 1) * nb_dof],
            )
            .unwrap();
        }
        (ids.len(), assert_bits(&fast, &general, "btd_integrate"))
    }

    #[test]
    fn btd_integrate_matches_general_path_bitwise() {
        let mut v = Values::new(0x0bad_f00d_1357_9bdf);
        let mut elements = 0;
        for (r, sdim) in BTD_PAIRS {
            for nn in BTD_NN {
                for nq in [1, 2, 4, 8] {
                    for _ in 0..6 {
                        elements += btd_integrate_batch(&mut v, r, sdim, nn, nq, false).0;
                    }
                }
            }
        }
        assert!(elements >= 20_000, "{elements} elements compared");
    }

    #[test]
    fn btd_integrate_propagates_a_payload_nan_like_the_general_path() {
        let mut v = Values::new(0x1234_5678_9abc_def0);
        let (mut elements, mut nans) = (0, 0);
        for (r, sdim) in BTD_PAIRS {
            for nn in BTD_NN {
                for nq in [1, 4] {
                    for _ in 0..4 {
                        let (p, n) = btd_integrate_batch(&mut v, r, sdim, nn, nq, true);
                        elements += p;
                        nans += n;
                    }
                }
            }
        }
        assert!(
            elements >= 5_000 && nans > 0,
            "{elements} elements, {nans} NaNs"
        );
    }

    /// One random integration batch against the general path:
    /// (products, NaNs); checks that only the scalar case (inner product)
    /// and depth-blocked GEMV are declined.
    fn integrate_batch(v: &mut Values, nb_dof: usize, nq: usize, payload: bool) -> (usize, usize) {
        let ne = 200;
        if payload {
            let k = 1 + v.next_u64() % (ne * nq * (nb_dof + 1)) as u64;
            v.payload_at(k);
        }
        let f = v.vec(ne * nq * nb_dof);
        let jac = v.vec(ne * nq);
        v.default_mode();
        let jf = |pos: usize| &jac[pos * nq..(pos + 1) * nq];
        let (fast, done) = every_level(ne, ne * nb_dof, |ex, out| {
            integrate(ex, &f, nb_dof, nq, &jf, ne, out).is_some()
        });
        assert_eq!(done, nb_dof > 1 && nq < 128, "nb_dof={nb_dof} nq={nq}");
        if !done {
            return (0, 0);
        }
        let mut general = vec![0.0; fast.len()];
        for pos in 0..ne {
            product_slices(
                Dims::dynamic(nb_dof, nq),
                Dims::new(nq, 1, false, true),
                &f[pos * nq * nb_dof..(pos + 1) * nq * nb_dof],
                jf(pos),
                &mut general[pos * nb_dof..(pos + 1) * nb_dof],
            )
            .unwrap();
        }
        (ne, assert_bits(&fast, &general, "integrate"))
    }

    const INT_DOF: [usize; 8] = [1, 2, 3, 9, 12, 24, 30, 60];
    const INT_NQ: [usize; 8] = [1, 2, 3, 4, 8, 27, 127, 128];

    #[test]
    fn integrate_matches_general_path_bitwise() {
        let mut v = Values::new(0x5151_a5a5_0f0f_3c3c);
        let mut products = 0;
        for nb_dof in INT_DOF {
            for nq in INT_NQ {
                for _ in 0..12 {
                    products += integrate_batch(&mut v, nb_dof, nq, false).0;
                }
            }
        }
        assert!(products >= 100_000, "{products} products compared");
    }

    #[test]
    fn integrate_propagates_a_payload_nan_like_the_general_path() {
        let mut v = Values::new(0xabad_1dea_5eed_0001);
        let (mut products, mut nans) = (0, 0);
        for nb_dof in INT_DOF {
            for nq in INT_NQ {
                for _ in 0..3 {
                    let (p, n) = integrate_batch(&mut v, nb_dof, nq, true);
                    products += p;
                    nans += n;
                }
            }
        }
        assert!(
            products >= 20_000 && nans > 0,
            "{products} products, {nans} NaNs"
        );
    }

    /// The scalar integration kernel against the general route (the
    /// closure evaluation of `akantu_core::eigen_gebp::product_into`).
    #[test]
    fn integrate_scalar_matches_general_path_bitwise() {
        use akantu_core::eigen_gebp::{Extent, ProductShape, product_into};
        let mut v = Values::new(0x0123_4567_89ab_cdef);
        let (mut products, mut nans) = (0, 0);
        for nq in INT_NQ {
            for round in 0..40 {
                let ne = 100;
                let payload = round % 4 == 3;
                if payload {
                    let k = 1 + v.next_u64() % (2 * ne * nq) as u64;
                    v.payload_at(k);
                }
                let f = v.vec(ne * nq);
                let jac = v.vec(ne * nq);
                v.default_mode();
                let jf = |pos: usize| &jac[pos * nq..(pos + 1) * nq];
                let mut fast = vec![Real::NAN; ne];
                assert!(integrate_scalar(&f, nq, &jf, ne, &mut fast).is_some());
                let mut general = vec![7.0; ne];
                let shape = ProductShape::new(
                    Extent::Dynamic(1),
                    Extent::Dynamic(nq),
                    Extent::Dynamic(nq),
                    Extent::Fixed(1),
                );
                for pos in 0..ne {
                    let fe = &f[pos * nq..(pos + 1) * nq];
                    let je = jf(pos);
                    product_into(
                        &shape,
                        &|_, l| fe[l],
                        &|l, _| je[l],
                        &mut general[pos..=pos],
                    )
                    .unwrap();
                }
                products += ne;
                nans += assert_bits(&fast, &general, "integrate_scalar");
            }
        }
        assert!(
            products >= 20_000 && nans > 0,
            "{products} products, {nans} NaNs"
        );
        // other routes are declined
        let f = [1.0; 4];
        let mut o = [0.0; 2];
        assert!(integrate_scalar(&f, 2, &|_| &f[..2], 2, &mut o[..1]).is_none());
    }
}
