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
    Exec, FmaTask, Lanes4, SimdTask, SimdToken, Tuples, TuplesRef, with_fma, with_simd,
};
use akantu_core::{Idx, Real, Result};

use super::{Block, id_index};
use crate::eigen::{Dims, product_route};
use crate::element_class::LagrangeElement;

/// `true` when every node of the elements `ids` is a valid index of a
/// nodal field of `nb_nodes` tuples (`conn` holds `nnpe` nodes per element
/// and every id is an element of the block, checked by the caller).
#[inline(always)]
fn nodes_valid(conn: &[Idx], nnpe: usize, ids: &[Idx], nb_nodes: usize) -> bool {
    ids.iter().all(|&e| {
        let e = id_index(e);
        conn.get(e * nnpe..(e + 1) * nnpe).is_some_and(|row| {
            row.iter()
                .all(|&n| usize::try_from(n).is_ok_and(|n| n < nb_nodes))
        })
    })
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
    if nd == 0 || nq == 0 || nn > TREE_MAX || block.nnpe != nn {
        return None;
    }
    let route = product_route(Dims::new(nb_dof, nn, false, true), Dims::fixed(nd, nn).t());
    if route != Route::Coeff(Reduction::Tree)
        || dshapes.len() != block.nb_element * nq * nd * nn
        || out.len() != ids.len() * nq * nb_dof * nd
        || !(1..=3).contains(&nb_dof)
        || u.len() != block.nb_nodes * nb_dof
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
            out: o,
            element: std::marker::PhantomData::<E>,
        };
        match (fast, nb_dof) {
            // FAST MODE (D-122): the FMA demonstrator, validated against
            // this kernel by `akantu-solid/tests/fast_mode_gradient.rs`
            (Some(t), 1) => with_fma(t, FmaGradient::<E, 1>(g)),
            (Some(t), 2) => with_fma(t, FmaGradient::<E, 2>(g)),
            (Some(t), _) => with_fma(t, FmaGradient::<E, 3>(g)),
            (None, 1) => with_simd(simd, LaneGradient::<E, 1>(g)),
            (None, 2) => with_simd(simd, LaneGradient::<E, 2>(g)),
            (None, _) => with_simd(simd, LaneGradient::<E, 3>(g)),
        }
    });
    valid
        .load(std::sync::atomic::Ordering::Relaxed)
        .then_some(Ok(()))
}

/// The operands of one part of [`gradient`] (nodes checked).
struct Gradient<'a, E> {
    conn: &'a [Idx],
    u: &'a [Real],
    dshapes: &'a [Real],
    nq: usize,
    ids: &'a [Idx],
    out: &'a mut [Real],
    element: std::marker::PhantomData<E>,
}

/// The bitwise gradient: lanes over the `NBD` components of `U` (one
/// lane per row `i` of `∇u`), each lane the scalar sequence
/// `tree_l(U(i, l)·B(j, l))` of the general path.
struct LaneGradient<'a, E, const NBD: usize>(Gradient<'a, E>);

impl<E: LagrangeElement, const NBD: usize> SimdTask for LaneGradient<'_, E, NBD> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Gradient {
            conn,
            u,
            dshapes,
            nq,
            ids,
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
        for (&e, oe) in ids.iter().zip(out.chunks_exact_mut(nq * tsz)) {
            let e = id_index(e);
            // extractNodalToElementField (nodes checked by the caller)
            for (ue_n, &node) in ue.iter_mut().zip(&conn[e * nn..(e + 1) * nn]) {
                *ue_n = Lanes4::from_prefix(&u[id_index(node)]);
            }
            let de = &dshapes[e * nq * bsz..(e + 1) * nq * bsz];
            for (b, g) in de.chunks_exact(bsz).zip(oe.chunks_exact_mut(tsz)) {
                for (j, gj) in g.chunks_exact_mut(NBD).enumerate() {
                    // lhs(i, l) = U(i, l), rhs(l, j) = B(j, l)
                    for (l, pl) in p.iter_mut().enumerate().take(nn) {
                        *pl = ue[l] * Lanes4::splat(b[j + l * nd]);
                    }
                    gj.copy_from_slice(&tree_sum_of(&p, nn).prefix::<NBD>());
                }
            }
        }
    }
}

/// FAST MODE (D-122), the demonstrator of the fast-mode harness (D-123):
/// `∇u = U·Bᵀ` with each entry `Σ_l U(i, l)·B(j, l)` evaluated as a chain of
/// fused multiply-adds in the order `l = 0, 1, …` (one rounding per step
/// instead of a rounded product and the halving tree of
/// [`LaneGradient`]). Not bitwise the default path; its difference is
/// bounded by the rule of `fixtures/fast_mode/solid/tolerances.json`.
struct FmaGradient<'a, E, const NBD: usize>(Gradient<'a, E>);

impl<E: LagrangeElement, const NBD: usize> FmaTask for FmaGradient<'_, E, NBD> {
    type Output = ();

    #[inline(always)]
    fn run(self) {
        let Gradient {
            conn,
            u,
            dshapes,
            nq,
            ids,
            out,
            ..
        } = self.0;
        let nd = E::NATURAL_DIM;
        let nn = E::NB_SHAPES;
        let bsz = nd * nn;
        let tsz = NBD * nd;
        let (u, _) = u.as_chunks::<NBD>();
        let mut ue = [[0.0; NBD]; TREE_MAX];
        for (&e, oe) in ids.iter().zip(out.chunks_exact_mut(nq * tsz)) {
            let e = id_index(e);
            for (ue_n, &node) in ue.iter_mut().zip(&conn[e * nn..(e + 1) * nn]) {
                *ue_n = u[id_index(node)];
            }
            let de = &dshapes[e * nq * bsz..(e + 1) * nq * bsz];
            for (b, g) in de.chunks_exact(bsz).zip(oe.chunks_exact_mut(tsz)) {
                for j in 0..nd {
                    for i in 0..NBD {
                        let mut acc = ue[0][i] * b[j];
                        for l in 1..nn {
                            acc = ue[l][i].mul_add(b[j + l * nd], acc);
                        }
                        g[i + j * NBD] = acc;
                    }
                }
            }
        }
    }
}

/// `true` when `ids` (elements of a block of `nb_element`, checked by the
/// caller) can index `nq` shape-derivative tuples of `bsz` values each.
fn derivatives_cover(ids: &[Idx], nq: usize, bsz: usize, bs: &[Real]) -> bool {
    ids.iter()
        .all(|&e| (id_index(e) + 1).saturating_mul(nq * bsz) <= bs.len())
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
    if !derivatives_cover(ids, nq, sdim * nn, bs) {
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
                for (bj, oj) in b.chunks_exact(S).zip(o.chunks_exact_mut(R)) {
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
    let btd_route = product_route(Dims::dynamic(r, sdim), Dims::dynamic(sdim, nn));
    let nb_dof = r * nn;
    let int_route = product_route(Dims::dynamic(nb_dof, nq), Dims::new(nq, 1, false, true));
    let gemv = Route::Gemv {
        matrix: GemvMatrix::Lhs,
        kernel: Order::ColMajor,
    };
    if btd_route != Route::Coeff(Reduction::Sequential)
        || int_route != gemv
        || nq == 0
        || nq >= GEMV_BLOCKING_MIN_DEPTH
        || r == 0
        || nn == 0
        || nn > TREE_MAX
    {
        return None;
    }
    if ds.len() != ids.len() * nq * r * sdim || os.len() != ids.len() * nb_dof {
        return None;
    }
    if !derivatives_cover(ids, nq, sdim * nn, bs) || !derivatives_cover(ids, nq, 1, jac) {
        return None;
    }
    let run: BtdIntegratePart = match (r, sdim) {
        (1, 1) => btd_integrate_part::<1, 1>,
        (1, 2) => btd_integrate_part::<1, 2>,
        (1, 3) => btd_integrate_part::<1, 3>,
        (2, 2) => btd_integrate_part::<2, 2>,
        (3, 3) => btd_integrate_part::<3, 3>,
        _ => return None,
    };
    let min = (2048 / nq.max(1)).max(1);
    let items = (
        Tuples::new(os, nb_dof),
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
            jac,
            o.as_mut_slice(),
        );
    });
    Some(())
}

/// One part of [`btd_integrate`].
type BtdIntegratePart = fn(SimdToken, usize, &[Idx], usize, &[Real], &[Real], &[Real], &mut [Real]);

#[inline(never)] // as `btd_part`
#[allow(clippy::too_many_arguments)]
fn btd_integrate_part<const R: usize, const S: usize>(
    simd: SimdToken,
    nn: usize,
    ids: &[Idx],
    nq: usize,
    bs: &[Real],
    ds: &[Real],
    jac: &[Real],
    os: &mut [Real],
) {
    with_simd(
        simd,
        LaneBtdIntegrate::<R, S> {
            nn,
            ids,
            nq,
            bs,
            ds,
            jac,
            os,
        },
    );
}

/// [`btd_integrate`] of the elements of a part, lanes over the `R` rows of
/// `D`.
struct LaneBtdIntegrate<'a, const R: usize, const S: usize> {
    nn: usize,
    ids: &'a [Idx],
    nq: usize,
    bs: &'a [Real],
    ds: &'a [Real],
    jac: &'a [Real],
    os: &'a mut [Real],
}

impl<const R: usize, const S: usize> SimdTask for LaneBtdIntegrate<'_, R, S> {
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
                for (cj, bj) in c.iter_mut().zip(b.chunks_exact(S)) {
                    *cj = *cj + db_column(&dcol, bj) * jq;
                }
            }
            // res = 0 + c (zeroed destination)
            for (oj, &cj) in oe.chunks_exact_mut(R).zip(&c) {
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
    if (0..nb_element).any(|pos| jac(pos).len() < nq) {
        return None;
    }
    let min = (4096 / (nq * nb_dof).max(1)).max(1);
    let items = (Tuples::new(o, nb_dof), TuplesRef::new(f, nq * nb_dof));
    let simd = exec.simd_token();
    exec.for_each(items, min, |first, (mut o, f)| {
        integrate_part(
            simd,
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
    f: &[Real],
    nb_dof: usize,
    nq: usize,
    jac: impl Fn(usize) -> &'j [Real],
    o: &mut [Real],
) {
    with_simd(
        simd,
        LaneIntegrate {
            f,
            nb_dof,
            nq,
            jac,
            o,
        },
    );
}

/// [`integrate_part`]'s body: the accumulators of one element are
/// independent lanes (one per component).
struct LaneIntegrate<'a, J> {
    f: &'a [Real],
    nb_dof: usize,
    nq: usize,
    jac: J,
    o: &'a mut [Real],
}

impl<'a, 'j, J: Fn(usize) -> &'j [Real]> SimdTask for LaneIntegrate<'a, J> {
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
                    *ci += fl * jl; // c = c + fl(f·J)
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
    use akantu_core::exec::{ExecPolicy, Grain};
    use akantu_core::{Array, ElementType, GhostType, Idx};

    /// An executor whose thread count (1 to 4) and SIMD level (every level
    /// the CPU supports, the scalar one included) vary with the case and
    /// that splits every batch as finely as it can, so the property tests
    /// also cover the split batches and every instance of the lane kernels
    /// (D-123).
    fn test_exec(salt: usize) -> Exec {
        let n = 1 + salt % 4;
        let levels = akantu_core::exec::SimdLevel::supported();
        let level = levels[(salt / 4) % levels.len()];
        let p = ExecPolicy::serial()
            .with_threads(n)
            .unwrap()
            .with_simd(level)
            .unwrap();
        Exec::new(p.with_grain(Grain::Finest)).unwrap()
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
        let mut fast = vec![Real::NAN; ids.len() * nq * tsz];
        let res = gradient::<E>(
            &test_exec(ids.len()),
            &block,
            &u,
            nb_dof,
            &dsh,
            nq,
            &ids,
            &mut fast,
        );
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
        res.unwrap();
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
            let fast = gradient::<E>(&test_exec(salt), &block, &u, 3, &dsh, 1, &[0], &mut out);
            assert!(fast.is_none());
            assert_eq!(out, vec![0.5; 9]);
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
        let mut fast = vec![Real::NAN; np * r * nn];
        let done = btd(
            &test_exec(ids.len()),
            r,
            sdim,
            nn,
            &ids,
            nq,
            &bs,
            &ds,
            &mut fast,
        )
        .is_some();
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
        let mut fast = vec![Real::NAN; ids.len() * nb_dof];
        let done = btd_integrate(
            &test_exec(ids.len()),
            r,
            sdim,
            nn,
            &ids,
            nq,
            &bs,
            &ds,
            &jac,
            &mut fast,
        )
        .is_some();
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
        let mut fast = vec![Real::NAN; ne * nb_dof];
        let done = integrate(&test_exec(ne), &f, nb_dof, nq, &jf, ne, &mut fast).is_some();
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
