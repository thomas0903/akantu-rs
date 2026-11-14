// SPDX-License-Identifier: LGPL-3.0-or-later
// Ported from Akantu (EPFL/LSMS): src/common/aka_config.hh.in,
// cmake/AkantuMacros.cmake (declare_akantu_types).

//! Scalar and index types (decision record D-012).
//!
//! Upstream configures `Real = double` and `Int = Idx = int` /
//! `UInt = unsigned int` by default (`AKANTU_TYPE_INTEGER = "int (32bit)"`,
//! `AKANTU_SIGNED_INTEGER_TYPE`, optional 64-bit build). akantu-rs fixes the
//! widths (Cargo features must be additive, so a width-changing feature would
//! break the ecosystem):
//!
//! | Type | Width | Meaning |
//! |---|---|---|
//! | [`Real`] | `f64` | upstream `Real` |
//! | [`Idx`] (= [`NodeIdx`], [`ElemIdx`], [`DofIdx`]) | `i32` | stored **local** index: node, element within its `(type, ghost)` block, local DOF / local equation |
//! | [`GlobalIdx`] | `i64` | global (cross-rank) node / element id, **global equation** number |
//! | [`Rank`] | `i32` | MPI rank (C `int`) |
//! | `usize` | – | counts, sizes and in-memory index arithmetic |
//! | [`Int`] / [`UInt`] | `i32` / `u32` | payload integers (mesh data, integer parameters) |
//!
//! `Idx = i32` is upstream's default signed 32-bit `Idx`: connectivity arrays
//! have the same dtype (`int32`) as upstream's NumPy exports, and the `-1`
//! sentinel ([`INVALID_IDX`]) is the upstream sentinel, so foreign buffers can
//! be shared without conversion. `GlobalIdx = i64` is signed like the native
//! APIs it feeds (PETSc/MUMPS 64-bit builds) and keeps upstream's `-1`
//! sentinel semantics; it lifts upstream's `2^31` limit on global systems.
//!
//! # Representable limits
//!
//! | Quantity | Maximum count | Maximum valid index |
//! |---|---|---|
//! | local entities of one kind (nodes, elements of one `(type, ghost)`) | [`MAX_LOCAL_COUNT`] = `2^31 − 1` | [`MAX_IDX`] = `2^31 − 2` |
//! | local DOFs / local equations (`nb_nodes × nb_dof_per_node`, summed over DOF families) | [`MAX_LOCAL_COUNT`] | [`MAX_IDX`] |
//! | global ids / global equations | [`MAX_GLOBAL_COUNT`] = `2^63 − 1` | [`MAX_GLOBAL_IDX`] = `2^63 − 2` |
//!
//! The maximum count is `Idx::MAX` (not `Idx::MAX + 1`) so that every count is
//! itself representable as an `Idx` (upstream passes sizes as `Int`).
//! A *node* bound does **not** bound local DOFs: a rank with
//! `n` nodes and `d` DOFs per node needs `n·d ≤ MAX_LOCAL_COUNT`, checked by
//! [`checked_local_dofs`] before any state is modified.
//!
//! Native back-ends add their own limits; they are checked at the boundary,
//! collectively, before conversion:
//! * MUMPS (5.x, `MUMPS_INT` = 32-bit unless built with 64-bit ints): matrix
//!   order and 1-based COO coordinates `≤ 2^31 − 1`, i.e. global equation
//!   index `≤ 2^31 − 2` ([`to_one_based_i32`]); `nnz` is 64-bit
//!   (`MUMPS_INT8`).
//! * PETSc: `PetscInt` width detected at build time (the oracle image has a
//!   32-bit build); global equations beyond `i32::MAX` require a 64-bit
//!   PETSc.
//! * Scotch: `SCOTCH_Num` width detected at build time.
//! * MPI (3.1): element counts are C `int` (`≤ 2^31 − 1` per call; larger
//!   messages are refused by `akantu-comm`, see its `MAX_MESSAGE_BYTES`).
//!   Integer payloads use fixed-width datatypes (`MPI_INT32_T` /
//!   `MPI_INT64_T`).
//!
//! # Conversions
//!
//! Never convert with a bare `as` on values that may be negative or large.
//! * stored index → `usize`: [`to_usize`] (internal invariant, panics on a
//!   negative value) / [`try_to_usize`] (user input);
//! * `usize` → stored index: [`to_idx`] (internal) / [`try_idx`] (user input);
//! * sentinel-aware: [`decode_idx`] (`-1` → `None`, other negatives rejected) /
//!   [`encode_idx`];
//! * global: [`try_global_idx`], [`global_to_usize`], [`to_one_based_i32`];
//! * sizes: [`checked_len`], [`try_local_count`], [`checked_local_dofs`].

use crate::error::{Error, Result};

/// Floating-point scalar (upstream `Real`, always IEEE-754 binary64).
pub type Real = f64;

/// Generic signed integer payload (upstream default `Int` = 32-bit `int`).
pub type Int = i32;

/// Generic unsigned integer payload (upstream default `UInt` = 32-bit).
pub type UInt = u32;

/// Stored local index (node, element within its `(type, ghost)` block, local
/// DOF / local equation). Upstream `Idx` (default `int`).
pub type Idx = i32;

/// Local node index (alias of [`Idx`], for documentation).
pub type NodeIdx = Idx;

/// Index of an element inside its `(ElementType, GhostType)` block.
pub type ElemIdx = Idx;

/// Local degree-of-freedom / local equation number.
pub type DofIdx = Idx;

/// Global (cross-rank) id of a node or element, or a global equation number.
pub type GlobalIdx = i64;

/// MPI rank (MPI uses C `int`).
pub type Rank = i32;

/// Identifier strings (upstream `ID = std::string`).
pub type Id = String;

/// Sentinel for "no local index" (upstream `-1`, `ElementNull.element`).
pub const INVALID_IDX: Idx = -1;

/// Sentinel for "no global id / no global equation" (upstream `-1`).
pub const INVALID_GLOBAL_IDX: GlobalIdx = -1;

/// Sentinel for "no rank" (upstream `getNodePrank` returns `-1`).
pub const NO_RANK: Rank = -1;

/// Largest number of local entities of one kind, and of local DOFs /
/// equations, on one rank (`2^31 − 1`, so that a count fits an [`Idx`]).
pub const MAX_LOCAL_COUNT: usize = Idx::MAX as usize;

/// Largest valid local index (`MAX_LOCAL_COUNT − 1`).
pub const MAX_IDX: Idx = Idx::MAX - 1;

/// Largest number of global ids / global equations (`2^63 − 1`).
pub const MAX_GLOBAL_COUNT: u64 = GlobalIdx::MAX as u64;

/// Largest valid global id / global equation (`MAX_GLOBAL_COUNT − 1`).
pub const MAX_GLOBAL_IDX: GlobalIdx = GlobalIdx::MAX - 1;

/// Convert a stored, valid (non-negative) [`Idx`] to `usize` for indexing.
///
/// # Panics
/// If `i < 0` (a sentinel or corrupted index reaching an indexing path is an
/// internal bug). Use [`try_to_usize`] / [`decode_idx`] for unvalidated data.
#[inline(always)]
#[must_use]
#[track_caller]
pub const fn to_usize(i: Idx) -> usize {
    assert!(i >= 0, "negative local index used for indexing");
    i as usize
}

/// Fallible [`to_usize`] for user / file / foreign data.
pub fn try_to_usize(i: Idx) -> Result<usize> {
    usize::try_from(i).map_err(|_| Error::InvalidIndex {
        what: "local index",
        value: i64::from(i),
    })
}

/// Convert an in-memory `usize` index to a stored [`Idx`].
///
/// # Panics
/// If `i > MAX_IDX`; local sizes are validated with [`try_local_count`] /
/// [`checked_local_dofs`] when meshes and DOF managers are created or grown,
/// so this can only fire on an internal bug. Use [`try_idx`] otherwise.
#[inline(always)]
#[must_use]
#[track_caller]
pub fn to_idx(i: usize) -> Idx {
    match try_idx(i) {
        Ok(v) => v,
        Err(_) => panic!("local index exceeds the 32-bit local index range"),
    }
}

/// Fallible conversion of a `usize` index to a stored [`Idx`] (user input).
pub fn try_idx(i: usize) -> Result<Idx> {
    if i <= MAX_IDX as usize {
        // In range by the check above.
        Ok(i as Idx)
    } else {
        Err(Error::CapacityExceeded {
            what: "local index",
            requested: i as u128,
            limit: MAX_IDX as u128,
        })
    }
}

/// Decode a possibly-sentinel stored index: `-1` → `None`, `0..=MAX_IDX` →
/// `Some`, any other value (other negatives, `i32::MAX`) → error.
pub fn decode_idx(i: Idx) -> Result<Option<usize>> {
    match i {
        INVALID_IDX => Ok(None),
        0..=MAX_IDX => Ok(Some(i as usize)),
        _ => Err(Error::InvalidIndex {
            what: "local index",
            value: i64::from(i),
        }),
    }
}

/// Encode an optional index with the upstream `-1` sentinel.
pub fn encode_idx(i: Option<usize>) -> Result<Idx> {
    match i {
        None => Ok(INVALID_IDX),
        Some(i) => try_idx(i),
    }
}

/// Validate a local entity count (`≤ MAX_LOCAL_COUNT`).
pub fn try_local_count(n: usize) -> Result<usize> {
    if n <= MAX_LOCAL_COUNT {
        Ok(n)
    } else {
        Err(Error::CapacityExceeded {
            what: "local entity count",
            requested: n as u128,
            limit: MAX_LOCAL_COUNT as u128,
        })
    }
}

/// `nb_nodes × nb_dof_per_node`, checked against [`MAX_LOCAL_COUNT`] (local
/// equation numbers are [`DofIdx`]). Call before resizing any DOF storage.
pub fn checked_local_dofs(nb_nodes: usize, nb_dof_per_node: usize) -> Result<usize> {
    let n = (nb_nodes as u128) * (nb_dof_per_node as u128);
    if n <= MAX_LOCAL_COUNT as u128 {
        Ok(n as usize)
    } else {
        Err(Error::CapacityExceeded {
            what: "local DOF count",
            requested: n,
            limit: MAX_LOCAL_COUNT as u128,
        })
    }
}

/// `len × nb_component` as a checked `usize` product (storage length of an
/// array / matrix). `what` names the quantity in the error.
#[inline]
pub fn checked_len(len: usize, nb_component: usize, what: &'static str) -> Result<usize> {
    // lazy: the error is built only on overflow
    len.checked_mul(nb_component)
        .ok_or_else(|| Error::CapacityExceeded {
            what,
            requested: (len as u128) * (nb_component as u128),
            limit: usize::MAX as u128,
        })
}

/// Convert an in-memory global position to a [`GlobalIdx`] (`≤ MAX_GLOBAL_IDX`).
pub fn try_global_idx(i: u64) -> Result<GlobalIdx> {
    if i <= MAX_GLOBAL_IDX as u64 {
        Ok(i as GlobalIdx)
    } else {
        Err(Error::CapacityExceeded {
            what: "global index",
            requested: u128::from(i),
            limit: MAX_GLOBAL_IDX as u128,
        })
    }
}

/// Convert a valid global id to `usize` (rejects negatives and values beyond
/// the address space, e.g. on 32-bit targets).
pub fn global_to_usize(g: GlobalIdx) -> Result<usize> {
    if g < 0 {
        return Err(Error::InvalidIndex {
            what: "global index",
            value: g,
        });
    }
    usize::try_from(g).map_err(|_| Error::CapacityExceeded {
        what: "global index (address space)",
        requested: g as u128,
        limit: usize::MAX as u128,
    })
}

/// 0-based global equation → 1-based 32-bit index (MUMPS `IRN`/`JCN`, 32-bit
/// PETSc / Fortran APIs). Rejects negatives and `g + 1 > i32::MAX`.
pub fn to_one_based_i32(g: GlobalIdx) -> Result<i32> {
    if g < 0 {
        return Err(Error::InvalidIndex {
            what: "global equation",
            value: g,
        });
    }
    // `g < i32::MAX` ⇔ `g + 1 ≤ i32::MAX`; no addition on the rejected path.
    if g < i64::from(i32::MAX) {
        Ok(g as i32 + 1)
    } else {
        Err(Error::CapacityExceeded {
            what: "1-based 32-bit native index",
            requested: (g as u128) + 1,
            limit: i32::MAX as u128,
        })
    }
}

/// Initial value of newly allocated `Real` storage. Upstream
/// `REAL_INIT_VALUE` is NaN in debug and `0.` in release builds; per
/// UD-054 the port always uses the release semantics (`0.0`), so debug and
/// release builds compute identical results.
pub const REAL_INIT_VALUE: Real = 0.0;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn widths_are_fixed() {
        assert_eq!(std::mem::size_of::<Real>(), 8);
        assert_eq!(std::mem::size_of::<Idx>(), 4);
        assert_eq!(std::mem::size_of::<GlobalIdx>(), 8);
        assert_eq!(std::mem::size_of::<Rank>(), 4);
        // signed, as upstream (NumPy int32 connectivity, -1 sentinels)
        assert_eq!(Idx::MIN, i32::MIN);
        assert_eq!(GlobalIdx::MIN, i64::MIN);
    }

    #[test]
    fn limits() {
        assert_eq!(MAX_LOCAL_COUNT, 2_147_483_647);
        assert_eq!(MAX_IDX, 2_147_483_646);
        assert_eq!(MAX_GLOBAL_COUNT, 9_223_372_036_854_775_807);
        assert_eq!(MAX_GLOBAL_IDX, 9_223_372_036_854_775_806);
        assert_eq!(INVALID_IDX, -1);
        assert_eq!(INVALID_GLOBAL_IDX, -1);
        assert_eq!(NO_RANK, -1);
    }

    #[test]
    fn index_conversions() {
        assert_eq!(to_usize(7), 7usize);
        assert_eq!(to_idx(7), 7);
        assert_eq!(try_idx(42).unwrap(), 42);
        assert_eq!(try_idx(MAX_IDX as usize).unwrap(), MAX_IDX);
        // unsigned-to-signed rejection above the valid range
        assert!(try_idx(MAX_IDX as usize + 1).is_err());
        assert!(try_idx(i32::MAX as usize).is_err());
        assert!(try_idx(u32::MAX as usize).is_err());
        assert!(try_idx(usize::MAX).is_err());
        // negative input
        assert!(try_to_usize(-1).is_err());
        assert!(try_to_usize(i32::MIN).is_err());
        assert_eq!(try_to_usize(0).unwrap(), 0);
        assert_eq!(try_to_usize(MAX_IDX).unwrap(), MAX_IDX as usize);
    }

    #[test]
    fn sentinels() {
        assert_eq!(decode_idx(-1).unwrap(), None);
        assert_eq!(decode_idx(0).unwrap(), Some(0));
        assert_eq!(decode_idx(MAX_IDX).unwrap(), Some(MAX_IDX as usize));
        assert!(decode_idx(-2).is_err());
        assert!(decode_idx(i32::MIN).is_err());
        assert!(decode_idx(i32::MAX).is_err());
        assert_eq!(encode_idx(None).unwrap(), -1);
        assert_eq!(encode_idx(Some(3)).unwrap(), 3);
        assert!(encode_idx(Some(MAX_LOCAL_COUNT)).is_err());
    }

    #[test]
    fn counts() {
        assert_eq!(try_local_count(MAX_LOCAL_COUNT).unwrap(), MAX_LOCAL_COUNT);
        assert!(try_local_count(MAX_LOCAL_COUNT + 1).is_err());
        // node count fits, DOF count does not
        let nodes = 1_000_000_000usize;
        assert!(try_local_count(nodes).is_ok());
        assert!(checked_local_dofs(nodes, 3).is_err());
        assert_eq!(checked_local_dofs(715_827_882, 3).unwrap(), 2_147_483_646);
        assert!(checked_local_dofs(715_827_883, 3).is_err());
        assert!(checked_local_dofs(usize::MAX, usize::MAX).is_err());
        assert_eq!(checked_len(3, 4, "x").unwrap(), 12);
        assert!(checked_len(usize::MAX / 2 + 1, 2, "x").is_err());
        assert_eq!(checked_len(usize::MAX, 0, "x").unwrap(), 0);
    }

    #[test]
    fn global_conversions() {
        assert_eq!(try_global_idx(5).unwrap(), 5);
        assert_eq!(
            try_global_idx(u64::from(u32::MAX) + 1).unwrap(),
            4_294_967_296
        );
        assert!(try_global_idx(i64::MAX as u64).is_err());
        assert!(try_global_idx(u64::MAX).is_err());
        assert!(global_to_usize(-1).is_err());
        assert_eq!(global_to_usize(1 << 40).unwrap(), 1usize << 40);
        assert_eq!(to_one_based_i32(0).unwrap(), 1);
        assert_eq!(to_one_based_i32(i64::from(i32::MAX) - 1).unwrap(), i32::MAX);
        assert!(to_one_based_i32(i64::from(i32::MAX)).is_err());
        assert!(to_one_based_i32(-1).is_err());
        assert!(to_one_based_i32(i64::MAX).is_err());
    }

    #[test]
    #[should_panic(expected = "negative local index")]
    fn to_usize_rejects_sentinel() {
        let _ = to_usize(INVALID_IDX);
    }

    #[test]
    #[should_panic(expected = "32-bit local index range")]
    fn to_idx_rejects_overflow() {
        let _ = to_idx(i32::MAX as usize);
    }
}
