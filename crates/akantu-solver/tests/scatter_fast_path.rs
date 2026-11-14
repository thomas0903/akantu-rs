// SPDX-License-Identifier: LGPL-3.0-or-later
// Part of akantu-rs, a Rust port of Akantu (EPFL/LSMS): property test of
// DofManager::assemble_elemental_array_local_array (upstream
// assembleElementalArrayLocalArray, src/model/common/dof_manager/
// dof_manager.cc) after the rewrite of its loops.

//! The rewritten scatter (filter validated in place, compile-time component
//! counts 1…3) against the previous implementation, kept here verbatim as
//! the reference: bitwise identical nodal arrays (accumulated over several
//! calls, special values included, exact and inexact scales: see
//! [`scale_of`]) and identical errors.

use akantu_core::array::ArrayView;
use akantu_core::{Array, ElementType, Error, GhostType, Idx, Real, Result};
use akantu_mesh::{Mesh, MeshAccessor};
use akantu_solver::DofManager;

/// The loops of `assemble_elemental_array_local_array` before the fast path.
fn reference(
    mesh: &Mesh,
    elementary: ArrayView<'_, Real>,
    out: &mut Array<Real>,
    ty: ElementType,
    ghost: GhostType,
    scale: Real,
    filter: Option<&[Idx]>,
) -> Result<()> {
    let conn = mesh.connectivity(ty, ghost)?;
    let nnpe = conn.nb_component();
    let nb_dof = elementary.nb_component() / nnpe;
    let elements: Vec<usize> = match filter {
        Some(fl) => fl
            .iter()
            .map(|&e| {
                usize::try_from(e)
                    .ok()
                    .filter(|&u| u < conn.len())
                    .ok_or(Error::OutOfRange {
                        what: "element",
                        index: usize::try_from(e).unwrap_or(usize::MAX),
                        len: conn.len(),
                    })
            })
            .collect::<Result<_>>()?,
        None => (0..conn.len()).collect(),
    };
    if elementary.len() != elements.len() {
        return Err(Error::DimensionMismatch {
            context: "assembleElementalArrayLocalArray (elements)",
            expected: elements.len(),
            got: elementary.len(),
        });
    }
    let nb_nodes = out.len();
    for &e in &elements {
        for &n in conn.tuple(e) {
            if usize::try_from(n).map_or(true, |u| u >= nb_nodes) {
                return Err(Error::OutOfRange {
                    what: "node",
                    index: usize::try_from(n).unwrap_or(usize::MAX),
                    len: nb_nodes,
                });
            }
        }
    }
    let o = out.as_mut_slice();
    for (k, &e) in elements.iter().enumerate() {
        let el = elementary.tuple(k);
        for (i, &n) in conn.tuple(e).iter().enumerate() {
            let base = usize::try_from(n).unwrap_or(0) * nb_dof;
            for d in 0..nb_dof {
                o[base + d] += scale * el[i * nb_dof + d];
            }
        }
    }
    Ok(())
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

fn mesh_with(ty: ElementType, nb_nodes: usize, nb_el: usize, v: &mut Values) -> Mesh {
    let dim = ty.spatial_dimension();
    let mut mesh = Mesh::new(dim, "m").unwrap();
    {
        let mut acc = MeshAccessor::new(&mut mesh);
        for _ in 0..nb_nodes {
            acc.push_node(&vec![0.0; dim]).unwrap();
        }
        let nnpe = ty.nb_nodes_per_element();
        let mut c = acc.connectivity_mut(ty, GhostType::NotGhost);
        for _ in 0..nb_el {
            let t: Vec<Idx> = (0..nnpe)
                .map(|_| (v.next_u64() % nb_nodes as u64) as Idx)
                .collect();
            c.push_tuple(&t);
        }
    }
    mesh
}

/// Scale of call `k`: the model's `±1`, scales whose products `scale·el`
/// are exact (`0.5`, `−0`: a contracted `dst = fma(scale, el, dst)` gives
/// the same bits with them), and inexact ones (`0.1`, `−0.3`, `3e-300`,
/// `7e300`, a random finite value) with which a contraction changes bits
/// (a hand-fused update passed every other test).
fn scale_of(v: &mut Values, k: usize) -> Real {
    const SCALES: [Real; 8] = [-1.0, 1.0, 0.5, -0.0, 0.1, -0.3, 3e-300, 7e300];
    match SCALES.get(k % (SCALES.len() + 1)) {
        Some(&s) => s,
        None => v.finite(),
    }
}

const TYPES: [ElementType; 5] = [
    ElementType::Segment2,
    ElementType::Triangle3,
    ElementType::Tetrahedron4,
    ElementType::Tetrahedron10,
    ElementType::Hexahedron8,
];

/// One round (3 accumulated calls) against the reference: (values, NaN
/// entries). With `payload`, one payload NaN among the round's draws, all
/// others finite.
fn scatter_round(
    v: &mut Values,
    ty: ElementType,
    nb_dof: usize,
    round: usize,
    payload: bool,
) -> (usize, usize) {
    let nnpe = ty.nb_nodes_per_element();
    let (nb_nodes, nb_el) = (60, 200);
    let mesh = mesh_with(ty, nb_nodes, nb_el, v);
    let filter: Option<Vec<Idx>> = (round % 2 == 1).then(|| {
        (0..nb_el as Idx)
            .filter(|_| !v.next_u64().is_multiple_of(3))
            .collect()
    });
    let n_used = filter.as_ref().map_or(nb_el, Vec::len);
    if payload {
        let k = 1 + v.next_u64() % (nb_nodes * nb_dof + 3 * n_used * nnpe * nb_dof) as u64;
        v.payload_at(k);
    }
    let mut fast = Array::try_zeros(nb_nodes, nb_dof).unwrap();
    for x in fast.as_mut_slice() {
        *x = v.next();
    }
    let mut general = fast.clone();
    let mut values = 0;
    for call in 0..3 {
        let el: Vec<Real> = (0..n_used * nnpe * nb_dof).map(|_| v.next()).collect();
        let el = ArrayView::new(&el, nnpe * nb_dof).unwrap();
        let scale = scale_of(v, round * 3 + call);
        DofManager::assemble_elemental_array_local_array(
            &mesh,
            el,
            &mut fast,
            ty,
            GhostType::NotGhost,
            scale,
            filter.as_deref(),
        )
        .unwrap();
        reference(
            &mesh,
            el,
            &mut general,
            ty,
            GhostType::NotGhost,
            scale,
            filter.as_deref(),
        )
        .unwrap();
        values += n_used * nnpe * nb_dof;
    }
    v.default_mode();
    for (k, (a, b)) in fast.as_slice().iter().zip(general.as_slice()).enumerate() {
        assert!(
            same(*a, *b),
            "{ty:?} nb_dof {nb_dof} entry {k}: {:#x} vs {:#x}",
            a.to_bits(),
            b.to_bits()
        );
    }
    (
        values,
        fast.as_slice().iter().filter(|x| x.is_nan()).count(),
    )
}

#[test]
fn scatter_matches_previous_loops_bitwise() {
    let mut v = Values::new(0x0bad_cafe_1234_5678);
    let mut values = 0usize;
    for ty in TYPES {
        for nb_dof in 1..=4 {
            for round in 0..6 {
                values += scatter_round(&mut v, ty, nb_dof, round, false).0;
            }
        }
    }
    assert!(values >= 100_000, "{values}");
}

#[test]
fn scatter_propagates_a_payload_nan_like_previous_loops() {
    let mut v = Values::new(0x3141_5926_5358_9793);
    let (mut values, mut nans) = (0usize, 0usize);
    for ty in TYPES {
        for nb_dof in 1..=4 {
            for round in 0..4 {
                let (n, k) = scatter_round(&mut v, ty, nb_dof, round, true);
                values += n;
                nans += k;
            }
        }
    }
    assert!(values >= 20_000 && nans > 0, "{values} values, {nans} NaNs");
}

#[test]
fn scatter_errors_match_previous_loops() {
    let mut v = Values::new(7);
    let ty = ElementType::Tetrahedron4;
    let mut mesh = mesh_with(ty, 10, 5, &mut v);
    let el = vec![1.0; 5 * 4 * 3];
    let view = ArrayView::new(&el, 12).unwrap();
    let mut a = Array::try_zeros(10, 3).unwrap();
    let mut b = a.clone();
    // bad filter entries (negative, too large) before a bad length
    for filter in [vec![0, -1, 2, 3, 4], vec![0, 1, 2, 9, 4], vec![0, 1]] {
        let fast = DofManager::assemble_elemental_array_local_array(
            &mesh,
            view,
            &mut a,
            ty,
            GhostType::NotGhost,
            1.0,
            Some(&filter),
        );
        let general = reference(
            &mesh,
            view,
            &mut b,
            ty,
            GhostType::NotGhost,
            1.0,
            Some(&filter),
        );
        assert_eq!(format!("{fast:?}"), format!("{general:?}"));
        assert!(fast.is_err());
    }
    // a node out of range: nothing is written
    MeshAccessor::new(&mut mesh)
        .connectivity_mut(ty, GhostType::NotGhost)
        .tuple_mut(3)[1] = 10;
    let fast = DofManager::assemble_elemental_array_local_array(
        &mesh,
        view,
        &mut a,
        ty,
        GhostType::NotGhost,
        1.0,
        None,
    );
    let general = reference(&mesh, view, &mut b, ty, GhostType::NotGhost, 1.0, None);
    assert_eq!(format!("{fast:?}"), format!("{general:?}"));
    assert!(fast.is_err());
    assert!(a.as_slice().iter().all(|&x| x == 0.0));
}
